//! Past runs and machine downtime from `journalctl`.

use std::collections::HashMap;

use jiff::{SignedDuration, Timestamp};
use serde::Deserialize;

use jiff::civil::DateTime;
use jiff::tz::TimeZone;

use super::{Capabilities, Error, run};
use crate::host::{Host, Invocation};
use crate::model::{Downtime, DowntimeKind, Interval, LogLine, Outcome, Run, Scope};

// Catalog message IDs from systemd's sd-messages.h.
const UNIT_STARTING: &str = "7d4958e842da4a758f6c1cdc7b36dcc5";
const UNIT_STARTED: &str = "39f53479d3a045ac8e11786248231fbf";
const UNIT_SUCCESS: &str = "7ad2d189f7e94e70a38c781354912448";
const UNIT_FAILURE_RESULT: &str = "d9b373ed55a64feb8242e02dbe79a49c";
const UNIT_FAILED: &str = "be02cf6855d2428ba40df7e9d022f03d";
const SLEEP_START: &str = "6bbd95ee977941e497c48be27c254128";
const SLEEP_STOP: &str = "8811e6df2a8e40f58a94cea26f8ebf14";

/// Messages this close after a run's end still describe that run.
const JUST_ENDED: SignedDuration = SignedDuration::from_secs(5);

/// Gaps between boots shorter than this are reboots, not downtime worth showing.
const MIN_DOWNTIME: SignedDuration = SignedDuration::from_mins(2);

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
}

fn parse_entries(stdout: &str) -> Vec<Entry> {
    stdout
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
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

/// Runs of the given services since `since`, keyed by service name.
pub async fn runs(
    host: &dyn Host,
    scope: Scope,
    services: &[String],
    since: Timestamp,
) -> Result<HashMap<String, Vec<Run>>, Error> {
    if services.is_empty() {
        return Ok(HashMap::new());
    }

    // Matches on different fields are ANDed, on the same field ORed.
    let (field, invocation) = match scope {
        Scope::System => ("UNIT", journalctl(since).arg("_PID=1")),
        Scope::User => ("USER_UNIT", journalctl(since).arg("--user")),
    };
    let invocation = invocation
        .args(services.iter().map(|service| format!("{field}={service}")))
        .args(
            [
                UNIT_STARTING,
                UNIT_STARTED,
                UNIT_SUCCESS,
                UNIT_FAILURE_RESULT,
                UNIT_FAILED,
            ]
            .map(|id| format!("MESSAGE_ID={id}")),
        );

    Ok(assemble_runs(parse_entries(&run(host, invocation).await?)))
}

fn assemble_runs(entries: Vec<Entry>) -> HashMap<String, Vec<Run>> {
    let mut runs: HashMap<String, Vec<Run>> = HashMap::new();

    for entry in entries {
        let (Some(at), Some(id)) = (entry.at(), entry.message_id.as_deref()) else {
            continue;
        };
        let Some(unit) = entry.unit.as_ref().or(entry.user_unit.as_ref()) else {
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
            // Without "Starting" logged it opens the run. A oneshot can log
            // it right after it ended; then it belongs to the run that ended.
            UNIT_STARTED => {
                let just_ended = unit_runs
                    .last()
                    .and_then(|run| run.end)
                    .is_some_and(|end| at.duration_since(end) < JUST_ENDED);
                if open {
                    if let Some(run) = unit_runs.last_mut() {
                        run.end = run.end.or(Some(at));
                    }
                } else if !just_ended {
                    unit_runs.push(Run {
                        start: at,
                        end: None,
                        outcome: Outcome::Running,
                        invocation,
                    });
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
    Ok(stdout
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
        .collect())
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
        if let Ok(boots) =
            json.and_then(|stdout| serde_json::from_str::<Vec<Boot>>(&stdout).map_err(Error::parse))
        {
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
}
