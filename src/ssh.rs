//! Linux hosts reached through the system's OpenSSH client.
//!
//! Running `ssh` rather than speaking SSH in-process keeps everything the
//! user set up working as in a terminal: `~/.ssh/config`, keys, the agent,
//! `ProxyJump` and `known_hosts`. Commands run as `ssh host -- command`;
//! the system or user bus is reached through `systemd-stdio-bridge` on the
//! host, whose stdin and stdout carry the D-Bus connection.

use std::fmt;
use std::io;
use std::process::Stdio;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::OnceCell;
use zbus::Connection;
use zbus::conn::AuthMechanism;
use zbus::connection::socket::{ReadHalf, Socket, Split, WriteHalf};

use crate::host::{BoxFuture, Host, Invocation, Output};
use crate::model::Scope;

/// How long to wait for ssh's last words after a failed connection.
const STDERR_WAIT: std::time::Duration = std::time::Duration::from_secs(2);

/// Where to connect: anything `ssh` accepts as a destination, such as a
/// `Host` alias from `~/.ssh/config` or `user@host`, and an optional port.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Target {
    pub destination: String,
    pub port: Option<u16>,
    /// Extra `-o` options, e.g. `StrictHostKeyChecking=no`.
    pub options: Vec<String>,
}

impl Target {
    /// Reads `host`, `user@host`, `user@host:port` or the same behind
    /// `ssh://`, as `ssh` and its URLs write them.
    pub fn parse(text: &str) -> Self {
        let text = text.strip_prefix("ssh://").unwrap_or(text);
        let (destination, port) = match text.rsplit_once(':') {
            Some((host, port)) if !host.is_empty() => match port.parse() {
                Ok(port) => (host, Some(port)),
                Err(_) => (text, None),
            },
            _ => (text, None),
        };
        Self {
            destination: destination.to_owned(),
            port,
            options: Vec::new(),
        }
    }
}

impl fmt::Display for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.port {
            Some(port) => write!(f, "{}:{port}", self.destination),
            None => f.write_str(&self.destination),
        }
    }
}

pub struct SshHost {
    target: Target,
    name: String,
    system_bus: OnceCell<Connection>,
    session_bus: OnceCell<Connection>,
}

impl SshHost {
    pub fn new(target: Target) -> Self {
        Self {
            name: target.to_string(),
            target,
            system_bus: OnceCell::new(),
            session_bus: OnceCell::new(),
        }
    }

    /// `ssh` with the options every call shares, up to the destination.
    fn ssh(&self) -> Command {
        let mut command = Command::new("ssh");
        command.args([
            "-T",
            // A GUI cannot answer prompts on a terminal; a host that needs a
            // password fails instead of hanging.
            "-o",
            "BatchMode=yes",
            "-o",
            "ServerAliveInterval=15",
        ]);
        if let Some(port) = self.target.port {
            command.arg("-p").arg(port.to_string());
        }
        for option in &self.target.options {
            command.arg("-o").arg(option);
        }
        command
            .arg(&self.target.destination)
            .arg("--")
            .kill_on_drop(true);
        command
    }

    async fn connect(&self, scope: Scope) -> zbus::Result<Connection> {
        let bridge = match scope {
            Scope::System => "systemd-stdio-bridge".to_owned(),
            // `--user` only exists since systemd 248; the bus path works on
            // every supported version.
            Scope::User => {
                "systemd-stdio-bridge --bus-path=unix:path=/run/user/$(id -u)/bus".to_owned()
            }
        };
        let mut child = self
            .ssh()
            .arg(bridge)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| zbus::Error::InputOutput(error.into()))?;
        let (Some(stdin), Some(stdout), Some(mut stderr)) =
            (child.stdin.take(), child.stdout.take(), child.stderr.take())
        else {
            return Err(zbus::Error::Failure("ssh started without pipes".to_owned()));
        };
        // ssh explains a failed login on stderr, e.g. "Permission denied
        // (publickey)"; without it the user would only see the bus give up.
        let messages = tokio::spawn(async move {
            let mut text = String::new();
            let _ = stderr.read_to_string(&mut text).await;
            text
        });

        let built = zbus::connection::Builder::socket(Bridge {
            child,
            stdin,
            stdout,
        })
        .build()
        .await;
        match built {
            Ok(connection) => Ok(connection),
            Err(error) => {
                // ssh has exited by now, so its stderr ends soon.
                let said = tokio::time::timeout(STDERR_WAIT, messages)
                    .await
                    .ok()
                    .and_then(Result::ok)
                    .unwrap_or_default();
                Err(match said.trim() {
                    "" => error,
                    said => zbus::Error::Failure(format!("ssh: {said}")),
                })
            }
        }
    }
}

impl Host for SshHost {
    fn name(&self) -> &str {
        &self.name
    }

    fn run<'a>(&'a self, invocation: &'a Invocation) -> BoxFuture<'a, io::Result<Output>> {
        Box::pin(async move {
            // ssh joins its arguments into one line for the remote shell, so
            // every word is quoted to arrive as it was given.
            let env = [
                ("LC_ALL", "C"),
                ("SYSTEMD_PAGER", ""),
                ("SYSTEMD_COLORS", "0"),
            ]
            .into_iter()
            .chain(invocation.env.iter().map(|(k, v)| (k.as_str(), v.as_str())));
            let mut line = vec!["env".to_owned()];
            line.extend(env.map(|(key, value)| quote(&format!("{key}={value}"))));
            line.push(quote(&invocation.program));
            line.extend(invocation.args.iter().map(|arg| quote(arg)));

            let output = self.ssh().arg(line.join(" ")).output().await?;
            Ok(Output {
                success: output.status.success(),
                stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            })
        })
    }

    fn bus(&self, scope: Scope) -> BoxFuture<'_, zbus::Result<Connection>> {
        Box::pin(async move {
            let cell = match scope {
                Scope::System => &self.system_bus,
                Scope::User => &self.session_bus,
            };
            let connection = cell.get_or_try_init(|| self.connect(scope)).await?;
            Ok(connection.clone())
        })
    }
}

/// A word for a POSIX shell, in single quotes.
fn quote(word: &str) -> String {
    format!("'{}'", word.replace('\'', r"'\''"))
}

/// `systemd-stdio-bridge` running over ssh, as a D-Bus socket: its stdout
/// is read, its stdin written. The child ends with the connection.
#[derive(Debug)]
struct Bridge {
    child: Child,
    stdin: ChildStdin,
    stdout: ChildStdout,
}

impl Socket for Bridge {
    type ReadHalf = BridgeReader;
    type WriteHalf = BridgeWriter;

    fn split(self) -> Split<BridgeReader, BridgeWriter> {
        Split::new(
            BridgeReader {
                stdout: self.stdout,
                _child: self.child,
            },
            BridgeWriter { stdin: self.stdin },
        )
    }
}

#[derive(Debug)]
struct BridgeReader {
    stdout: ChildStdout,
    /// Kept so that dropping the connection kills ssh.
    _child: Child,
}

#[zbus::export::async_trait::async_trait]
impl ReadHalf for BridgeReader {
    #[cfg(unix)]
    async fn recvmsg(
        &mut self,
        buffer: &mut [u8],
    ) -> io::Result<(usize, Vec<std::os::fd::OwnedFd>)> {
        Ok((self.stdout.read(buffer).await?, Vec::new()))
    }

    #[cfg(not(unix))]
    async fn recvmsg(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.stdout.read(buffer).await
    }

    fn auth_mechanism(&self) -> AuthMechanism {
        // Pipes carry no credentials, and the bridge already authenticated
        // to the bus as the user ssh logged in as.
        AuthMechanism::Anonymous
    }
}

#[derive(Debug)]
struct BridgeWriter {
    stdin: ChildStdin,
}

#[zbus::export::async_trait::async_trait]
impl WriteHalf for BridgeWriter {
    #[cfg(unix)]
    async fn sendmsg(
        &mut self,
        buffer: &[u8],
        _fds: &[std::os::fd::BorrowedFd<'_>],
    ) -> io::Result<usize> {
        self.stdin.write(buffer).await
    }

    #[cfg(not(unix))]
    async fn sendmsg(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.stdin.write(buffer).await
    }

    async fn flush(&mut self) -> io::Result<()> {
        self.stdin.flush().await
    }

    async fn close(&mut self) -> io::Result<()> {
        self.stdin.shutdown().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::systemd;

    #[test]
    fn parses_destinations() {
        let target = |text| {
            let target = Target::parse(text);
            (target.destination, target.port)
        };
        assert_eq!(target("web1"), ("web1".to_owned(), None));
        assert_eq!(target("me@web1"), ("me@web1".to_owned(), None));
        assert_eq!(target("me@web1:2222"), ("me@web1".to_owned(), Some(2222)));
        assert_eq!(
            target("ssh://me@web1:2222"),
            ("me@web1".to_owned(), Some(2222))
        );
    }

    #[test]
    fn quotes_words_for_the_remote_shell() {
        assert_eq!(quote("a b"), "'a b'");
        assert_eq!(quote("it's"), r"'it'\''s'");
        assert_eq!(quote("$(id -u)"), "'$(id -u)'");
    }

    /// The container from `tests/ssh-host/run.sh`, as `user`.
    fn test_host_as(user: &str) -> SshHost {
        SshHost::new(Target {
            destination: format!("{user}@localhost"),
            port: Some(2222),
            // The container's host key changes with every build.
            options: vec![
                "StrictHostKeyChecking=no".to_owned(),
                "UserKnownHostsFile=/dev/null".to_owned(),
                "LogLevel=ERROR".to_owned(),
            ],
        })
    }

    /// Runs a command and reads the system bus over SSH. Run with
    /// `cargo test ssh_host -- --ignored --nocapture`.
    #[tokio::test]
    #[ignore = "needs the container from tests/ssh-host/run.sh"]
    async fn ssh_host_runs_commands_and_reaches_the_bus() {
        let host = test_host_as("sysorbit");
        let output = host
            .run(&Invocation::new("echo").args(["it's", "$HOME"]))
            .await
            .expect("ssh");
        assert_eq!(output.stdout.trim(), "it's $HOME", "{}", output.stderr);

        for scope in [Scope::System, Scope::User] {
            let bus = host.bus(scope).await.expect("bus over ssh");
            let capabilities = systemd::Capabilities::detect(&bus).await;
            println!("{scope:?}: systemd {:?}", capabilities.version);
            assert!(capabilities.version.is_some(), "{scope:?}");
        }
    }

    /// Reads a whole snapshot over SSH. Run with
    /// `cargo test ssh_snapshot -- --ignored --nocapture`.
    #[tokio::test]
    #[ignore = "needs the container from tests/ssh-host/run.sh"]
    async fn ssh_snapshot() {
        let host = test_host_as("sysorbit");
        let now = jiff::Timestamp::now();
        let window = crate::model::Interval {
            start: now - jiff::SignedDuration::from_hours(24),
            end: now + jiff::SignedDuration::from_hours(24),
        };
        for scope in [Scope::System, Scope::User] {
            let started = std::time::Instant::now();
            let snapshot = systemd::snapshot(&host, scope, window)
                .await
                .unwrap_or_else(|error| panic!("{scope:?}: {error}"));
            println!(
                "{scope:?}: {} timers in {:?}, history={}, warnings={:?}",
                snapshot.timers.len(),
                started.elapsed(),
                snapshot.history_known,
                snapshot.warnings
            );
            assert!(snapshot.history_known, "{scope:?}");
        }
    }

    /// A rejected login reports what ssh said. Run with
    /// `cargo test ssh_login -- --ignored`.
    #[tokio::test]
    #[ignore = "needs the container from tests/ssh-host/run.sh"]
    async fn ssh_login_failure_says_why() {
        let mut host = test_host_as("sysorbit");
        host.target.options.extend([
            "IdentityAgent=none".to_owned(),
            "IdentitiesOnly=yes".to_owned(),
            "IdentityFile=/nonexistent".to_owned(),
        ]);
        let error = host.bus(Scope::System).await.expect_err("no key, no login");
        let message = error.to_string();
        assert!(message.contains("Permission denied"), "{message}");
    }
}
