//! Where systemd commands run.
//!
//! Every query goes through [`Host`], so the parsers and the UI do not care
//! whether the machine is local or reached over SSH. Adding remote support
//! means adding one more `Host` implementation.

use std::future::Future;
use std::io;
use std::pin::Pin;

#[cfg(target_os = "linux")]
use tokio::sync::OnceCell;
use zbus::Connection;

use crate::model::Scope;

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// A program with its arguments, independent of how it is executed.
#[derive(Debug, Clone)]
pub struct Invocation {
    pub program: String,
    pub args: Vec<String>,
    /// Extra environment, on top of what every host sets for parsing.
    pub env: Vec<(String, String)>,
}

impl Invocation {
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            env: Vec::new(),
        }
    }

    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.push((key.into(), value.into()));
        self
    }

    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }
}

impl std::fmt::Display for Invocation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (key, value) in &self.env {
            write!(f, "{key}={value} ")?;
        }
        write!(f, "{}", self.program)?;
        for arg in &self.args {
            write!(f, " {arg}")?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct Output {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

pub trait Host: Send + Sync {
    /// Human readable name shown in the UI.
    fn name(&self) -> &str;

    fn run<'a>(&'a self, invocation: &'a Invocation) -> BoxFuture<'a, io::Result<Output>>;

    /// The host's system bus, or the user's session bus for [`Scope::User`].
    fn bus(&self, scope: Scope) -> BoxFuture<'_, zbus::Result<Connection>>;

    /// Whether this is the machine the app runs on, sharing its clock.
    fn is_local(&self) -> bool {
        false
    }
}

/// This machine's hostname, e.g. `blade`.
#[cfg(target_os = "linux")]
pub fn local_hostname() -> String {
    let uname = rustix::system::uname();
    let name = uname.nodename().to_string_lossy();
    if name.is_empty() {
        "localhost".to_owned()
    } else {
        name.into_owned()
    }
}

/// Runs commands on this machine. Only Linux has systemd to ask.
#[cfg(target_os = "linux")]
pub struct LocalHost {
    name: String,
    system_bus: OnceCell<Connection>,
    session_bus: OnceCell<Connection>,
}

#[cfg(target_os = "linux")]
impl LocalHost {
    pub fn new() -> Self {
        Self {
            name: local_hostname(),
            system_bus: OnceCell::new(),
            session_bus: OnceCell::new(),
        }
    }
}

#[cfg(target_os = "linux")]
impl Host for LocalHost {
    fn name(&self) -> &str {
        &self.name
    }

    fn is_local(&self) -> bool {
        true
    }

    fn run<'a>(&'a self, invocation: &'a Invocation) -> BoxFuture<'a, io::Result<Output>> {
        Box::pin(async move {
            let output = tokio::process::Command::new(&invocation.program)
                .args(&invocation.args)
                // Parsers expect untranslated, pager-free output.
                .env("LC_ALL", "C")
                .env("SYSTEMD_PAGER", "")
                .env("SYSTEMD_COLORS", "0")
                .envs(invocation.env.iter().map(|(key, value)| (key, value)))
                .kill_on_drop(true)
                .output()
                .await?;

            Ok(Output {
                success: output.status.success(),
                stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            })
        })
    }

    fn bus(&self, scope: Scope) -> BoxFuture<'_, zbus::Result<Connection>> {
        Box::pin(async move {
            let connection = match scope {
                Scope::System => self.system_bus.get_or_try_init(Connection::system).await?,
                Scope::User => {
                    self.session_bus
                        .get_or_try_init(Connection::session)
                        .await?
                }
            };
            Ok(connection.clone())
        })
    }
}
