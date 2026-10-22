//! Past runs and machine downtime from `journalctl`.

use std::collections::HashMap;

use jiff::{SignedDuration, Timestamp};
use serde::Deserialize;

use super::{Error, run};
use crate::host::{Host, Invocation};
use crate::model::{Downtime, DowntimeKind, Interval, Outcome, Run, Scope};

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
        "--output-fields=MESSAGE_ID,UNIT,USER_UNIT,UNIT_RESULT".to_owned(),
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
        let unit_runs = runs.entry(unit.clone()).or_default();
        let open = unit_runs
            .last()
            .is_some_and(|run| run.outcome == Outcome::Running);

        match id {
            UNIT_STARTING => {
                if !open {
                    unit_runs.push(Run {
                        start: at,
                        end: None,
                        outcome: Outcome::Running,
                    });
                }
            }
            // Some services log "Starting" and some do not; "Started" (or
            // "Finished" for oneshots) may open the run. A oneshot can log it
            // right after it ended; then it belongs to the run that just ended.
            UNIT_STARTED => {
                let just_ended = unit_runs
                    .last()
                    .and_then(|run| run.end)
                    .is_some_and(|end| at.duration_since(end) < JUST_ENDED);
                if !open && !just_ended {
                    unit_runs.push(Run {
                        start: at,
                        end: None,
                        outcome: Outcome::Running,
                    });
                }
            }
            UNIT_SUCCESS => close(unit_runs, at, Outcome::Success),
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
                    close(unit_runs, at, Outcome::Failed(result));
                }
            }
            _ => {}
        }
    }

    runs
}

/// Ends the open run, or records an instant run when its start was not logged.
fn close(runs: &mut Vec<Run>, at: Timestamp, outcome: Outcome) {
    match runs
        .last_mut()
        .filter(|run| run.outcome == Outcome::Running)
    {
        Some(run) => {
            run.end = Some(at);
            run.outcome = outcome;
        }
        None => runs.push(Run {
            start: at,
            end: Some(at),
            outcome,
        }),
    }
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

/// Periods in `window` where the machine was powered off or suspended.
pub async fn downtime(host: &dyn Host, window: Interval) -> Result<Vec<Downtime>, Error> {
    #[derive(Deserialize)]
    struct Boot {
        first_entry: i64,
        last_entry: i64,
    }

    let boots = run(
        host,
        Invocation::new("journalctl").args(["--list-boots", "--output=json", "--no-pager"]),
    );
    let sleeps = run(
        host,
        journalctl(window.start).args([
            format!("MESSAGE_ID={SLEEP_START}"),
            format!("MESSAGE_ID={SLEEP_STOP}"),
        ]),
    );
    let (boots, sleeps) = tokio::join!(boots, sleeps);

    let boots: Vec<Boot> = serde_json::from_str(&boots?).map_err(Error::parse)?;
    let mut downtime: Vec<Downtime> = boots
        .windows(2)
        .filter_map(|pair| {
            Some(Interval {
                start: Timestamp::from_microsecond(pair[0].last_entry).ok()?,
                end: Timestamp::from_microsecond(pair[1].first_entry).ok()?,
            })
        })
        .map(|interval| Downtime {
            interval,
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
    fn end_without_start_is_instant_run() {
        let runs = assemble_runs(vec![entry(5_000_000, UNIT_SUCCESS, None)]);
        let run = &runs["backup.service"][0];
        assert_eq!(Some(run.start), run.end);
    }
}
