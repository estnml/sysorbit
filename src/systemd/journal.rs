//! Past runs and machine downtime from `journalctl`.

use std::collections::HashMap;

use jiff::{SignedDuration, Timestamp};
use serde::Deserialize;

use jiff::civil::DateTime;
use jiff::tz::TimeZone;

use super::{Capabilities, Error, run, run_output};
use crate::host::{Host, Invocation};
use crate::model::{Downtime, DowntimeKind, Interval, LogLine, Outcome, Run, Scope};

// Catalog message IDs from systemd's sd-messages.h.
const UNIT_STARTING: &str = "7d4958e842da4a758f6c1cdc7b36dcc5";
const UNIT_STARTED: &str = "39f53479d3a045ac8e11786248231fbf";
const UNIT_SUCCESS: &str = "7ad2d189f7e94e70a38c781354912448";
const UNIT_FAILURE_RESULT: &str = "d9b373ed55a64feb8242e02dbe79a49c";
const UNIT_FAILED: &str = "be02cf6855d2428ba40df7e9d022f03d";
const UNIT_STOPPED: &str = "9d1aaa27d60140bd96365fd3e7aed5d5";
const SLEEP_START: &str = "6bbd95ee977941e497c48be27c254128";
const SLEEP_STOP: &str = "8811e6df2a8e40f58a94cea26f8ebf14";

/// Messages this close after a run's end still describe that run.
const JUST_ENDED: SignedDuration = SignedDuration::from_secs(5);

/// Gaps between boots shorter than this are reboots, not downtime worth showing.
const MIN_DOWNTIME: SignedDuration = SignedDuration::from_mins(2);

/// A timer stopped or started this close to a shutdown or boot was stopped
/// or started by it. Also the shortest stop worth hiding elapses for.
const BOOT_SLACK: SignedDuration = SignedDuration::from_mins(10);

#[derive(Deserialize)]
struct Entry {
    #[serde(rename = "__REALTIME_TIMESTAMP")]
    realtime: String,
    #[serde(rename = "__MONOTONIC_TIMESTAMP")]
    monotonic: Option<String>,
    #[serde(rename = "MESSAGE_ID")]
    message_id: Option<String>,
    #[serde(rename = "UNIT")]
    unit: Option<String>,
    #[serde(rename = "USER_UNIT")]
    user_unit: Option<String>,
    #[serde(rename = "UNIT_RESULT")]
    unit_result: Option<String>,
    #[serde(rename = "INVOCATION_ID")]
    invocation: Option<String>,
    #[serde(rename = "USER_INVOCATION_ID")]
    user_invocation: Option<String>,
}

impl Entry {
    fn at(&self) -> Option<Timestamp> {
        Timestamp::from_microsecond(self.realtime.parse().ok()?).ok()
    }

    fn unit(&self) -> Option<&String> {
        self.unit.as_ref().or(self.user_unit.as_ref())
    }
}

/// A timer being started (`on`) or stopped, from its job messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Switch {
    pub at: Timestamp,
    pub on: bool,
}

/// What the journal knows about a set of units, by unit name.
#[derive(Debug, Default)]
pub struct History {
    /// Runs of services.
    pub runs: HashMap<String, Vec<Run>>,
    /// Starts and stops of timers, oldest first.
    pub switches: HashMap<String, Vec<Switch>>,
}

fn parse_entries(stdout: &str) -> Vec<Entry> {
    let entries: Vec<Entry> = stdout
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    crate::diagnostics::parsed(
        crate::diagnostics::Parser::Journal,
        stdout.lines().count(),
        entries.len(),
    );
    entries
}

fn journalctl(since: Timestamp) -> Invocation {
    Invocation::new("journalctl").args([
        "--output=json".to_owned(),
        "--no-pager".to_owned(),
        "--quiet".to_owned(),
        format!("--since=@{}", since.as_second()),
        "--output-fields=MESSAGE_ID,UNIT,USER_UNIT,UNIT_RESULT,INVOCATION_ID,USER_INVOCATION_ID"
            .to_owned(),
    ])
}

/// Runs of the given services and switches of the given timers since
/// `since`, in one journal query.
pub async fn history(
    host: &dyn Host,
    scope: Scope,
    units: &[String],
    since: Timestamp,
) -> Result<History, Error> {
    if units.is_empty() {
        return Ok(History::default());
    }

    // Matches on different fields are ANDed, on the same field ORed.
    let (field, invocation) = match scope {
        Scope::System => ("UNIT", journalctl(since).arg("_PID=1")),
        Scope::User => ("USER_UNIT", journalctl(since).arg("--user")),
    };
    let invocation = invocation
        .args(units.iter().map(|unit| format!("{field}={unit}")))
        .args(
            [
                UNIT_STARTING,
                UNIT_STARTED,
                UNIT_SUCCESS,
                UNIT_FAILURE_RESULT,
                UNIT_FAILED,
                UNIT_STOPPED,
            ]
            .map(|id| format!("MESSAGE_ID={id}")),
        );

    // The system's entries are hidden from users outside `systemd-journal`
    // and `adm`; journalctl then succeeds with nothing to show.
    let (output, restricted) = match scope {
        Scope::System => tokio::join!(run(host, invocation), system_hidden(host)),
        Scope::User => (run(host, invocation).await, false),
    };
    if restricted {
        return Err(Error::JournalRestricted);
    }
    let (timers, services): (Vec<Entry>, Vec<Entry>) = parse_entries(&output?)
        .into_iter()
        .partition(|entry| entry.unit().is_some_and(|unit| unit.ends_with(".timer")));
    Ok(History {
        runs: assemble_runs(services),
        switches: assemble_switches(timers),
    })
}

/// Whether the journal hides other users' and the system's entries from
/// this user. journalctl says so on stderr, though not with JSON output;
/// asking for no lines gets just that notice.
pub(super) async fn system_hidden(host: &dyn Host) -> bool {
    let check = Invocation::new("journalctl").args(["--no-pager", "--lines=0"]);
    run_output(host, check)
        .await
        .is_ok_and(|output| restricted(&output.stderr))
}

fn restricted(stderr: &str) -> bool {
    stderr.contains("not seeing messages from other users and the system")
}

fn assemble_switches(entries: Vec<Entry>) -> HashMap<String, Vec<Switch>> {
    let mut switches: HashMap<String, Vec<Switch>> = HashMap::new();
    for entry in entries {
        let (Some(at), Some(unit)) = (entry.at(), entry.unit()) else {
            continue;
        };
        let on = match entry.message_id.as_deref() {
            Some(UNIT_STARTED) => true,
            // A stop logs both "Deactivated successfully" and "Stopped".
            Some(UNIT_SUCCESS | UNIT_STOPPED) => false,
            _ => continue,
        };
        let unit_switches = switches.entry(unit.clone()).or_default();
        if unit_switches.last().is_none_or(|last| last.on != on) {
            unit_switches.push(Switch { at, on });
        }
    }
    switches
}

/// Stretches inside `window` when a timer was stopped while the machine
/// was up, so its elapses there were never due. A timer is stopped at every
/// shutdown and started at boot; such a pair brackets downtime and does not
/// count, because elapses missed while powered off still matter for
/// `Persistent=`. With no stop seen, the timer did not run before its first
/// start, e.g. because it did not exist yet.
pub fn stopped_periods(
    switches: &[Switch],
    active_now: bool,
    window: Interval,
    downtime: &[Downtime],
) -> Vec<Interval> {
    let near_power = |at: Timestamp, edge: fn(&Interval) -> Timestamp| {
        downtime.iter().any(|down| {
            down.kind == DowntimeKind::PoweredOff
                && edge(&down.interval).duration_since(at).abs() <= BOOT_SLACK
        })
    };

    let initially_stopped = switches.first().map_or(!active_now, |first| first.on);
    // Start of the current stopped stretch, and whether a stop for a
    // shutdown began it; `None` when it began before the window.
    let mut stopped: Option<(Timestamp, Option<bool>)> =
        initially_stopped.then_some((window.start, None));
    let mut periods = Vec::new();

    for switch in switches {
        match (switch.on, stopped) {
            (true, Some((from, for_shutdown))) => {
                stopped = None;
                let for_boot =
                    near_power(switch.at, |down| down.end) && for_shutdown.unwrap_or(true);
                if !for_boot && switch.at.duration_since(from) > BOOT_SLACK {
                    periods.push(Interval {
                        start: from,
                        end: switch.at,
                    });
                }
            }
            (false, None) => {
                let for_shutdown = near_power(switch.at, |down| down.start);
                stopped = Some((switch.at, Some(for_shutdown)));
            }
            _ => {}
        }
    }
    if let (Some((from, _)), false) = (stopped, active_now) {
        periods.push(Interval {
            start: from,
            end: window.end,
        });
    }

    periods.retain(|period| period.overlaps(&window));
    periods
}

fn assemble_runs(entries: Vec<Entry>) -> HashMap<String, Vec<Run>> {
    let mut runs: HashMap<String, Vec<Run>> = HashMap::new();

    for entry in entries {
        let (Some(at), Some(id)) = (entry.at(), entry.message_id.as_deref()) else {
            continue;
        };
        let Some(unit) = entry.unit() else {
            continue;
        };
        let invocation = entry.invocation.clone().or(entry.user_invocation.clone());
        let unit_runs = runs.entry(unit.clone()).or_default();
        let open = unit_runs
            .last()
            .is_some_and(|run| run.outcome == Outcome::Running);

        match id {
            UNIT_STARTING => {
                // The user manager logs no "Deactivated" for finished oneshots;
                // a new start means the previous run is over.
                if let Some(run) = unit_runs.last_mut().filter(|_| open) {
                    run.end = run.end.or(Some(at));
                    run.outcome = Outcome::Success;
                }
                unit_runs.push(Run {
                    start: at,
                    end: None,
                    outcome: Outcome::Running,
                    invocation,
                });
            }
            // "Started" for long running services, "Finished" for oneshots;
            // the journal cannot tell them apart. On an open run it records
            // when the start job finished, kept as a tentative end that
            // `finish_dangling` uses once the service is known to be inactive.
            // Without "Starting" logged it opens the run, whose start job is
            // then already done; the user manager logs only this for simple
            // services, so a second one on such a run starts the next run.
            // A oneshot can log it right after it ended; then it belongs to
            // the run that ended.
            UNIT_STARTED => {
                let just_ended = unit_runs
                    .last()
                    .and_then(|run| run.end)
                    .is_some_and(|end| at.duration_since(end) < JUST_ENDED);
                match unit_runs.last_mut().filter(|_| open) {
                    Some(run) if run.end.is_none() => run.end = Some(at),
                    Some(run) => {
                        run.outcome = Outcome::Success;
                        unit_runs.push(Run {
                            start: at,
                            end: Some(at),
                            outcome: Outcome::Running,
                            invocation,
                        });
                    }
                    None if !just_ended => unit_runs.push(Run {
                        start: at,
                        end: Some(at),
                        outcome: Outcome::Running,
                        invocation,
                    }),
                    None => {}
                }
            }
            UNIT_SUCCESS => close(unit_runs, at, Outcome::Success, invocation),
            UNIT_FAILURE_RESULT | UNIT_FAILED => {
                // A failing start logs both messages; keep the first.
                let duplicate = unit_runs.last().is_some_and(|run| {
                    matches!(run.outcome, Outcome::Failed(_))
                        && run
                            .end
                            .is_some_and(|end| at.duration_since(end) < JUST_ENDED)
                });
                if !duplicate {
                    let result = entry.unit_result.unwrap_or_else(|| "failed".to_owned());
                    close(unit_runs, at, Outcome::Failed(result), invocation);
                }
            }
            _ => {}
        }
    }

    runs
}

/// Settles the last run of a service that is no longer active but whose end
/// was never logged: it ended when its start job finished, or right away.
pub fn finish_dangling(runs: &mut [Run]) {
    if let Some(run) = runs
        .last_mut()
        .filter(|run| run.outcome == Outcome::Running)
    {
        run.end = run.end.or(Some(run.start));
        run.outcome = Outcome::Success;
    }
}

/// Ends the open run, or records an instant run when its start was not logged.
fn close(runs: &mut Vec<Run>, at: Timestamp, outcome: Outcome, invocation: Option<String>) {
    match runs
        .last_mut()
        .filter(|run| run.outcome == Outcome::Running)
    {
        Some(run) => {
            run.end = Some(at);
            run.outcome = outcome;
            run.invocation = run.invocation.take().or(invocation);
        }
        None => runs.push(Run {
            start: at,
            end: Some(at),
            outcome,
            invocation,
        }),
    }
}

/// Upper bound of log lines fetched for one run.
const MAX_LOG_LINES: usize = 5000;

/// Everything one run logged: the service's own output plus what systemd
/// said about it (start, exit status, result), oldest first.
pub async fn logs(host: &dyn Host, scope: Scope, invocation: &str) -> Result<Vec<LogLine>, Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Text {
        Plain(String),
        // Messages that are not valid UTF-8 arrive as byte arrays.
        Bytes(Vec<u8>),
    }

    #[derive(Deserialize)]
    struct Line {
        #[serde(rename = "__REALTIME_TIMESTAMP")]
        realtime: String,
        #[serde(rename = "PRIORITY")]
        priority: Option<String>,
        #[serde(rename = "MESSAGE")]
        message: Option<Text>,
    }

    // Lines of the service carry _SYSTEMD_INVOCATION_ID, the manager's lines
    // about it INVOCATION_ID (USER_INVOCATION_ID from the user manager);
    // `+` joins the two matches with OR.
    let manager_field = match scope {
        Scope::System => "INVOCATION_ID",
        Scope::User => "USER_INVOCATION_ID",
    };
    let mut invocation_args = vec![
        "--output=json".to_owned(),
        "--no-pager".to_owned(),
        "--quiet".to_owned(),
        format!("--lines={MAX_LOG_LINES}"),
        "--output-fields=MESSAGE,PRIORITY".to_owned(),
    ];
    if scope == Scope::User {
        invocation_args.push("--user".to_owned());
    }
    invocation_args.extend([
        format!("_SYSTEMD_INVOCATION_ID={invocation}"),
        "+".to_owned(),
        format!("{manager_field}={invocation}"),
    ]);

    let stdout = run(host, Invocation::new("journalctl").args(invocation_args)).await?;
    let lines: Vec<LogLine> = stdout
        .lines()
        .filter_map(|line| serde_json::from_str::<Line>(line).ok())
        .filter_map(|line| {
            let at = Timestamp::from_microsecond(line.realtime.parse().ok()?).ok()?;
            let message = match line.message? {
                Text::Plain(text) => text,
                Text::Bytes(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
            };
            Some(LogLine {
                at,
                priority: line.priority.and_then(|p| p.parse().ok()).unwrap_or(6),
                message,
            })
        })
        .collect();
    crate::diagnostics::parsed(
        crate::diagnostics::Parser::JournalLogs,
        stdout.lines().count(),
        lines.len(),
    );
    Ok(lines)
}

/// The host's realtime clock minus its monotonic clock, taken from the
/// newest journal entry of the current boot, which carries both. Works on
/// remote hosts and old systemd versions alike. `None` when the journal of
/// this boot is empty.
pub async fn clock_offset(host: &dyn Host) -> Result<Option<SignedDuration>, Error> {
    let stdout = run(
        host,
        Invocation::new("journalctl").args([
            "--boot",
            "--lines=1",
            "--output=json",
            "--no-pager",
            "--quiet",
            "--output-fields=MESSAGE_ID",
        ]),
    )
    .await?;

    Ok(parse_entries(&stdout).first().and_then(|entry| {
        let realtime: i64 = entry.realtime.parse().ok()?;
        let monotonic: i64 = entry.monotonic.as_ref()?.parse().ok()?;
        Some(SignedDuration::from_micros(realtime - monotonic))
    }))
}

/// Each boot as the span from its first to its last journal entry, oldest
/// first. JSON output needs systemd 251; older versions get the text table,
/// printed in UTC so its timestamps parse the same everywhere.
async fn boots(host: &dyn Host, capabilities: Capabilities) -> Result<Vec<Interval>, Error> {
    #[derive(Deserialize)]
    struct Boot {
        first_entry: i64,
        last_entry: i64,
    }

    let list_boots = || Invocation::new("journalctl").args(["--list-boots", "--no-pager"]);

    if capabilities.boots_json() {
        let json = run(host, list_boots().arg("--output=json")).await;
        if let Ok(boots) = json.and_then(|stdout| {
            serde_json::from_str::<Vec<Boot>>(&stdout)
                .map_err(|error| Error::parse(crate::diagnostics::Parser::Boots, error))
        }) {
            let mut boots: Vec<Interval> = boots
                .into_iter()
                .filter_map(|boot| {
                    Some(Interval {
                        start: Timestamp::from_microsecond(boot.first_entry).ok()?,
                        end: Timestamp::from_microsecond(boot.last_entry).ok()?,
                    })
                })
                .collect();
            boots.sort_by_key(|boot| boot.start);
            return Ok(boots);
        }
    }

    crate::diagnostics::event(crate::diagnostics::Event::BootsTextFallback);
    let text = run(host, list_boots().env("TZ", "UTC")).await?;
    Ok(parse_boots_text(&text))
}

/// Parses the `--list-boots` table. Rows differ between versions (header or
/// not, an em dash between the two entries or not), so each row is scanned
/// for `YYYY-MM-DD HH:MM:SS UTC` timestamps and rows with two are kept.
fn parse_boots_text(text: &str) -> Vec<Interval> {
    let mut boots: Vec<Interval> = text
        .lines()
        .filter_map(|line| {
            let tokens: Vec<&str> = line.split_whitespace().collect();
            let times: Vec<Timestamp> = tokens
                .windows(3)
                .filter(|window| window[2].starts_with("UTC"))
                .filter_map(|window| {
                    let civil: DateTime = format!("{}T{}", window[0], window[1]).parse().ok()?;
                    Some(civil.to_zoned(TimeZone::UTC).ok()?.timestamp())
                })
                .collect();
            match times.as_slice() {
                [start, end, ..] => Some(Interval {
                    start: *start,
                    end: *end,
                }),
                _ => None,
            }
        })
        .collect();
    boots.sort_by_key(|boot| boot.start);
    boots
}

/// Periods in `window` where the machine was powered off or suspended.
pub async fn downtime(
    host: &dyn Host,
    window: Interval,
    capabilities: Capabilities,
) -> Result<Vec<Downtime>, Error> {
    let sleeps = run(
        host,
        journalctl(window.start).args([
            format!("MESSAGE_ID={SLEEP_START}"),
            format!("MESSAGE_ID={SLEEP_STOP}"),
        ]),
    );
    let (boots, sleeps) = tokio::join!(boots(host, capabilities), sleeps);

    let mut downtime: Vec<Downtime> = boots?
        .windows(2)
        .map(|pair| Downtime {
            interval: Interval {
                start: pair[0].end,
                end: pair[1].start,
            },
            kind: DowntimeKind::PoweredOff,
        })
        .collect();

    let mut asleep_since = None;
    for entry in parse_entries(&sleeps?) {
        match (entry.message_id.as_deref(), entry.at()) {
            (Some(SLEEP_START), Some(at)) => asleep_since = Some(at),
            (Some(SLEEP_STOP), Some(at)) => {
                if let Some(start) = asleep_since.take() {
                    downtime.push(Downtime {
                        interval: Interval { start, end: at },
                        kind: DowntimeKind::Suspended,
                    });
                }
            }
            _ => {}
        }
    }

    downtime.retain(|down| {
        down.interval.end.duration_since(down.interval.start) >= MIN_DOWNTIME
            && down.interval.overlaps(&window)
    });
    downtime.sort_by_key(|down| down.interval.start);

    Ok(downtime)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(usec: i64, id: &str, result: Option<&str>) -> Entry {
        Entry {
            realtime: usec.to_string(),
            monotonic: None,
            message_id: Some(id.to_owned()),
            unit: Some("backup.service".to_owned()),
            user_unit: None,
            unit_result: result.map(str::to_owned),
            invocation: Some(format!("run-{usec}")),
            user_invocation: None,
        }
    }

    #[test]
    fn pairs_start_and_end() {
        let runs = assemble_runs(vec![
            entry(1_000_000, UNIT_STARTED, None),
            entry(1_300_000, UNIT_SUCCESS, None),
            entry(9_000_000, UNIT_STARTED, None),
            entry(9_500_000, UNIT_FAILURE_RESULT, Some("exit-code")),
            entry(9_600_000, UNIT_FAILED, None),
            entry(20_000_000, UNIT_STARTED, None),
        ]);
        let runs = &runs["backup.service"];

        assert_eq!(runs.len(), 3);
        assert_eq!(runs[0].outcome, Outcome::Success);
        assert_eq!(runs[0].end.unwrap().as_microsecond(), 1_300_000);
        assert_eq!(runs[1].outcome, Outcome::Failed("exit-code".to_owned()));
        assert_eq!(runs[2].outcome, Outcome::Running);
    }

    #[test]
    fn oneshot_finished_after_end_opens_nothing() {
        let runs = assemble_runs(vec![
            entry(1_000_000, UNIT_STARTING, None),
            entry(6_000_000, UNIT_SUCCESS, None),
            entry(6_100_000, UNIT_STARTED, None),
        ]);
        let runs = &runs["backup.service"];

        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].outcome, Outcome::Success);
        assert_eq!(runs[0].start.as_microsecond(), 1_000_000);
    }

    #[test]
    fn parses_boot_table_of_any_version() {
        // systemd 262 with header row.
        let modern = "\
IDX  BOOT ID                          FIRST ENTRY                 LAST ENTRY
 -1 960c19c5652542b0862c6b4dd938d12b Fri 2026-06-05 12:44:38 UTC Fri 2026-06-05 12:44:42 UTC
  0 d39e515e7d1e4367adeb9b55f1a7e409 Thu 2026-10-22 21:21:06 UTC Thu 2026-10-22 22:40:50 UTC
";
        // Older versions join the two entries with an em dash.
        let old = " -1 960c19c5652542b0862c6b4dd938d12b Fri 2026-06-05 12:44:38 UTC—Fri 2026-06-05 12:44:42 UTC\n";

        let boots = parse_boots_text(modern);
        assert_eq!(boots.len(), 2);
        assert_eq!(boots[1].start.to_string(), "2026-10-22T21:21:06Z");
        assert_eq!(boots[1].end.to_string(), "2026-10-22T22:40:50Z");

        let boots = parse_boots_text(old);
        assert_eq!(boots.len(), 1);
        assert_eq!(boots[0].end.to_string(), "2026-06-05T12:44:42Z");
    }

    #[test]
    fn user_manager_runs_close_on_next_start() {
        // The user manager logs "Starting" and "Finished", never "Deactivated".
        let mut runs = assemble_runs(vec![
            entry(1_000_000, UNIT_STARTING, None),
            entry(1_200_000, UNIT_STARTED, None),
            entry(61_000_000, UNIT_STARTING, None),
            entry(61_300_000, UNIT_STARTED, None),
        ])
        .remove("backup.service")
        .unwrap();

        assert_eq!(runs[0].outcome, Outcome::Success);
        assert_eq!(runs[0].end.unwrap().as_microsecond(), 1_200_000);
        assert_eq!(runs[1].outcome, Outcome::Running);

        finish_dangling(&mut runs);
        assert_eq!(runs[1].outcome, Outcome::Success);
        assert_eq!(runs[1].end.unwrap().as_microsecond(), 61_300_000);
    }

    #[test]
    fn end_without_start_is_instant_run() {
        let runs = assemble_runs(vec![entry(5_000_000, UNIT_SUCCESS, None)]);
        let run = &runs["backup.service"][0];
        assert_eq!(Some(run.start), run.end);
    }

    fn at(mins: i64) -> Timestamp {
        Timestamp::from_second(1_800_000_000 + mins * 60).unwrap()
    }

    fn window() -> Interval {
        Interval {
            start: at(0),
            end: at(1000),
        }
    }

    fn switch(mins: i64, on: bool) -> Switch {
        Switch { at: at(mins), on }
    }

    #[test]
    fn timer_did_not_run_before_its_first_start() {
        let periods = stopped_periods(&[switch(300, true)], true, window(), &[]);
        assert_eq!(
            periods,
            vec![Interval {
                start: at(0),
                end: at(300)
            }]
        );
    }

    #[test]
    fn shutdown_and_boot_do_not_stop_a_timer() {
        let off = Downtime {
            interval: Interval {
                start: at(100),
                end: at(400),
            },
            kind: DowntimeKind::PoweredOff,
        };
        let switches = [switch(99, false), switch(401, true)];
        assert!(stopped_periods(&switches, true, window(), &[off]).is_empty());
        // Started at boot with nothing seen before: there all along.
        assert!(stopped_periods(&switches[1..], true, window(), &[off]).is_empty());
    }

    #[test]
    fn stopping_by_hand_hides_until_started_again() {
        let switches = [switch(100, false), switch(500, true), switch(800, false)];
        assert_eq!(
            stopped_periods(&switches, false, window(), &[]),
            vec![
                Interval {
                    start: at(100),
                    end: at(500)
                },
                Interval {
                    start: at(800),
                    end: at(1000)
                },
            ]
        );
        // A restart is too short to hide anything.
        let restart = [switch(100, false), switch(101, true)];
        assert!(stopped_periods(&restart, true, window(), &[]).is_empty());
    }

    #[test]
    fn splits_timer_switches_from_service_runs() {
        let mut stop = entry(1_000_000, UNIT_SUCCESS, None);
        stop.unit = Some("backup.timer".to_owned());
        let mut stopped = entry(1_000_100, UNIT_STOPPED, None);
        stopped.unit = Some("backup.timer".to_owned());
        let mut start = entry(5_000_000, UNIT_STARTED, None);
        start.unit = Some("backup.timer".to_owned());
        let switches = assemble_switches(vec![stop, stopped, start]);
        let switches = &switches["backup.timer"];
        assert_eq!(switches.len(), 2, "the double stop counts once");
        assert!(!switches[0].on && switches[1].on);
    }

    #[test]
    fn repeated_started_without_starting_are_separate_runs() {
        // The user manager logs only "Started" for simple services.
        let runs = assemble_runs(vec![
            entry(1_000_000, UNIT_STARTED, None),
            entry(120_000_000, UNIT_STARTED, None),
            entry(240_000_000, UNIT_STARTED, None),
        ]);
        let runs = &runs["backup.service"];
        assert_eq!(runs.len(), 3);
        assert_eq!(runs[0].outcome, Outcome::Success);
        assert_eq!(runs[0].end, runs[0].start.into());
        assert_eq!(runs[2].outcome, Outcome::Running);
    }

    /// A host whose journal hides the system's entries.
    struct RestrictedJournal;

    impl Host for RestrictedJournal {
        fn name(&self) -> &str {
            "restricted"
        }

        fn run<'a>(
            &'a self,
            _invocation: &'a Invocation,
        ) -> crate::host::BoxFuture<'a, std::io::Result<crate::host::Output>> {
            Box::pin(async {
                Ok(crate::host::Output {
                    success: true,
                    stdout: String::new(),
                    stderr: "Hint: You are currently not seeing messages from other users and \
                             the system.\n"
                        .to_owned(),
                })
            })
        }

        fn bus(&self, _scope: Scope) -> crate::host::BoxFuture<'_, zbus::Result<zbus::Connection>> {
            Box::pin(async { Err(zbus::Error::Unsupported) })
        }
    }

    #[tokio::test]
    async fn a_journal_hiding_the_system_is_not_an_empty_history() {
        let units = ["backup.service".to_owned()];
        let system = history(&RestrictedJournal, Scope::System, &units, at(0)).await;
        assert!(matches!(system, Err(Error::JournalRestricted)));
        // The user's own journal is complete.
        let user = history(&RestrictedJournal, Scope::User, &units, at(0)).await;
        assert!(user.is_ok());
    }
}
