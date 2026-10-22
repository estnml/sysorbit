//! Reads timers, their schedules and history from a [`Host`].

mod calendar;
mod capabilities;
mod journal;
mod units;

pub mod timespan;

use std::collections::HashMap;
use std::fmt;

use jiff::Timestamp;

pub use capabilities::Capabilities;

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
    let bus = bus?;
    let capabilities = Capabilities::detect(&bus).await;
    warnings.extend(capabilities.limitations());
    let (mut timers, unit_warnings) = units::load(&bus, clock_offset).await?;
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
        calendar::occurrences(host, &expressions, window, capabilities),
        journal::runs(host, scope, &services, window.start),
        journal::downtime(host, window, capabilities)
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
        if !timer.service_active {
            journal::finish_dangling(&mut timer.runs);
        }
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
    use std::path::PathBuf;

    use tokio::sync::OnceCell;
    use zbus::Connection;

    use super::*;
    use crate::host::{BoxFuture, LocalHost, Output};

    /// A container from `tests/systemd-matrix/run.sh`: commands go through
    /// `docker exec`, D-Bus through the socket directory bind mounted on the
    /// host.
    struct DockerHost {
        container: String,
        socket: PathBuf,
        bus: OnceCell<Connection>,
    }

    impl Host for DockerHost {
        fn name(&self) -> &str {
            &self.container
        }

        fn run<'a>(&'a self, invocation: &'a Invocation) -> BoxFuture<'a, std::io::Result<Output>> {
            Box::pin(async move {
                let mut command = tokio::process::Command::new("docker");
                command.arg("exec");
                let env = [
                    ("LC_ALL", "C"),
                    ("SYSTEMD_PAGER", ""),
                    ("SYSTEMD_COLORS", "0"),
                ]
                .map(|(key, value)| (key.to_owned(), value.to_owned()));
                for (key, value) in env.iter().chain(&invocation.env) {
                    command.arg("-e").arg(format!("{key}={value}"));
                }
                let output = command
                    .arg(&self.container)
                    .arg(&invocation.program)
                    .args(&invocation.args)
                    .output()
                    .await?;
                Ok(Output {
                    success: output.status.success(),
                    stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
                    stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
                })
            })
        }

        fn bus(&self, _scope: Scope) -> BoxFuture<'_, zbus::Result<Connection>> {
            Box::pin(async move {
                let address = format!("unix:path={}", self.socket.display());
                let connection = self
                    .bus
                    .get_or_try_init(|| async {
                        zbus::connection::Builder::address(address.as_str())
                            .build()
                            .await
                    })
                    .await?;
                Ok(connection.clone())
            })
        }
    }

    /// Runs a snapshot against every container started by
    /// `tests/systemd-matrix/run.sh`. Run with
    /// `cargo test systemd_matrix -- --ignored --nocapture`.
    #[tokio::test]
    #[ignore = "needs the containers from tests/systemd-matrix/run.sh"]
    async fn systemd_matrix() {
        let runtime = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_owned());
        let root = PathBuf::from(runtime).join("sysorbit-matrix");
        let mut names: Vec<String> = std::fs::read_dir(&root)
            .expect("run tests/systemd-matrix/run.sh first")
            .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
            .collect();
        names.sort();
        assert!(!names.is_empty(), "no containers in {}", root.display());

        let now = Timestamp::now();
        let window = Interval {
            start: now - jiff::SignedDuration::from_hours(2),
            end: now + jiff::SignedDuration::from_hours(6),
        };

        for name in names {
            let host = DockerHost {
                container: format!("sysorbit-matrix-{name}"),
                socket: root.join(&name).join("system_bus_socket"),
                bus: OnceCell::new(),
            };
            let capabilities = Capabilities::detect(&host.bus(Scope::System).await.unwrap()).await;
            let snapshot = snapshot(&host, Scope::System, window)
                .await
                .unwrap_or_else(|error| panic!("{name}: {error}"));

            let timer = |id: &str| {
                snapshot
                    .timers
                    .iter()
                    .find(|timer| timer.id == id)
                    .unwrap_or_else(|| panic!("{name}: {id} missing"))
            };
            let quarter = timer("sysorbit-quarter.timer");
            let boot = timer("sysorbit-boot.timer");
            let occurrences = quarter.calendar[0].occurrences.len();

            println!(
                "{name:12} systemd {:?}: {} timers, quarter occurrences={occurrences}, \
                 boot next={:?}, warnings={:?}",
                capabilities.version,
                snapshot.timers.len(),
                boot.next_elapse
                    .map(|next| next.duration_since(now).as_secs()),
                snapshot.warnings
            );

            // Every version must deliver the next elapses and the history;
            // only the depth of the calendar depends on the version.
            assert!(quarter.next_elapse.is_some(), "{name}: calendar next");
            assert!(boot.next_elapse.is_some(), "{name}: monotonic next");
            assert!(snapshot.history_known, "{name}: history");
            assert!(
                !snapshot.warnings.iter().any(|w| w.contains("unavailable")),
                "{name}: {:?}",
                snapshot.warnings
            );
            let expected = if capabilities.calendar_iterations() {
                20
            } else {
                1
            };
            assert!(occurrences >= expected, "{name}: {occurrences} occurrences");
        }
    }

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
