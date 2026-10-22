//! Reads timers, their schedules and history from a [`Host`].

mod calendar;
mod journal;
mod units;

pub mod timespan;

use std::collections::HashMap;
use std::fmt;

use jiff::Timestamp;

use crate::host::{Host, Invocation};
use crate::model::{Interval, Scope, Snapshot};

#[derive(Debug, Clone)]
pub enum Error {
    Spawn { command: String, reason: String },
    Command { command: String, stderr: String },
    Parse(String),
    Bus(String),
}

impl From<zbus::Error> for Error {
    fn from(error: zbus::Error) -> Self {
        Self::Bus(error.to_string())
    }
}

impl Error {
    fn parse(error: impl fmt::Display) -> Self {
        Self::Parse(error.to_string())
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Spawn { command, reason } => write!(f, "`{command}` could not be run: {reason}"),
            Self::Command { command, stderr } => write!(f, "`{command}` failed: {stderr}"),
            Self::Parse(reason) => write!(f, "unreadable output: {reason}"),
            Self::Bus(reason) => write!(f, "D-Bus error: {reason}"),
        }
    }
}

impl std::error::Error for Error {}

/// Runs `invocation` and returns its stdout, failing on a non-zero exit.
async fn run(host: &dyn Host, invocation: Invocation) -> Result<String, Error> {
    let output = host.run(&invocation).await.map_err(|error| Error::Spawn {
        command: invocation.to_string(),
        reason: error.to_string(),
    })?;

    if output.success {
        Ok(output.stdout)
    } else {
        Err(Error::Command {
            command: invocation.to_string(),
            stderr: output.stderr.trim().to_owned(),
        })
    }
}

/// Collects everything the UI needs for `window` in one pass.
pub async fn snapshot(host: &dyn Host, scope: Scope, window: Interval) -> Result<Snapshot, Error> {
    let mut warnings = Vec::new();
    let (bus, clock_offset) = tokio::join!(host.bus(scope), journal::clock_offset(host));
    let clock_offset = clock_offset.unwrap_or_else(|error| {
        warnings.push(format!("Monotonic times unavailable: {error}"));
        None
    });
    let (mut timers, unit_warnings) = units::load(&bus?, clock_offset).await?;
    warnings.extend(unit_warnings);

    let mut expressions: Vec<String> = timers
        .iter()
        .flat_map(|timer| {
            timer
                .calendar
                .iter()
                .map(|trigger| trigger.expression.clone())
        })
        .collect();
    expressions.sort();
    expressions.dedup();

    let services: Vec<String> = timers.iter().map(|timer| timer.service.clone()).collect();

    let (occurrences, runs, downtime) = tokio::join!(
        calendar::occurrences(host, &expressions, window),
        journal::runs(host, scope, &services, window.start),
        journal::downtime(host, window)
    );

    let history_known = runs.is_ok();
    let mut runs = runs.unwrap_or_else(|error| {
        warnings.push(format!("History unavailable: {error}"));
        HashMap::new()
    });
    let downtime = downtime.unwrap_or_else(|error| {
        warnings.push(format!("Downtime unavailable: {error}"));
        Vec::new()
    });

    for timer in &mut timers {
        timer.runs = runs.remove(&timer.service).unwrap_or_default();
        for trigger in &mut timer.calendar {
            trigger.occurrences = occurrences
                .get(&trigger.expression)
                .cloned()
                .unwrap_or_default();
        }
    }

    Ok(Snapshot {
        scope,
        fetched_at: Timestamp::now(),
        timers,
        downtime,
        history_known,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::LocalHost;

    /// Reads the real timers of this machine. Run with
    /// `cargo test live_snapshot -- --ignored --nocapture`.
    #[tokio::test]
    #[ignore = "needs systemd on the test machine"]
    async fn live_snapshot() {
        let now = Timestamp::now();
        let window = Interval {
            start: now - jiff::SignedDuration::from_hours(24 * 3),
            end: now + jiff::SignedDuration::from_hours(24 * 7),
        };

        let snapshot = snapshot(&LocalHost::new(), Scope::System, window)
            .await
            .expect("snapshot");

        for timer in &snapshot.timers {
            let occurrences: usize = timer.calendar.iter().map(|c| c.occurrences.len()).sum();
            println!(
                "{:40} active={:5} next={:?} occurrences={occurrences} runs={}",
                timer.id,
                timer.active,
                timer.next_elapse,
                timer.runs.len()
            );
        }
        println!("downtime: {:#?}", snapshot.downtime);
        println!("warnings: {:?}", snapshot.warnings);
        assert!(!snapshot.timers.is_empty());
    }
}
