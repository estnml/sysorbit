//! The hosts the app can show: this machine (on Linux), hosts saved in
//! `hosts.toml`, `Host` aliases from `~/.ssh/config`, and one given on the
//! command line.
//!
//! `hosts.toml` lives in the platform's config directory
//! (`~/.config/sysorbit` on Linux, `~/Library/Application Support/sysorbit`
//! on macOS, `%APPDATA%\sysorbit\config` on Windows) and is only read, so
//! comments and order stay as the user wrote them. The app's own state,
//! such as the last host shown, goes to `state.toml` next to it.
//!
//! ```toml
//! [[host]]
//! name = "Prod web"
//! destination = "admin@203.0.113.10"   # or a ~/.ssh/config alias
//! port = 22
//! options = ["IdentityFile=~/keys/prod.pem"]
//! ```

use std::path::{Path, PathBuf};
use std::sync::Arc;

use directories::{BaseDirs, ProjectDirs};
use serde::{Deserialize, Serialize};

use crate::host::Host;
#[cfg(target_os = "linux")]
use crate::host::LocalHost;
use crate::ssh::{SshHost, Target};

/// The id of this machine's entry.
#[cfg(target_os = "linux")]
const LOCAL: &str = "local";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostEntry {
    /// Stable across runs; remembers the last host shown.
    pub id: String,
    /// What the menu shows: the hostname for this machine, the name given
    /// in `hosts.toml`, or the alias.
    pub name: String,
    pub kind: HostKind,
    pub origin: Origin,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostKind {
    /// This machine; only Linux has one to show.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    Local,
    Ssh(Target),
}

/// Where an entry came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    ThisComputer,
    Saved,
    SshConfig,
    CommandLine,
}

impl HostEntry {
    /// A fresh connection to the host.
    pub fn connect(&self) -> Arc<dyn Host> {
        match &self.kind {
            #[cfg(target_os = "linux")]
            HostKind::Local => Arc::new(LocalHost::new()),
            // Only Linux has a local entry; see `load`.
            #[cfg(not(target_os = "linux"))]
            HostKind::Local => unreachable!("no local host outside Linux"),
            HostKind::Ssh(target) => Arc::new(SshHost::new(target.clone())),
        }
    }

    /// The host given on the command line, shown but not saved.
    pub fn from_command_line(target: Target) -> Self {
        Self {
            id: "command-line".to_owned(),
            name: target.to_string(),
            kind: HostKind::Ssh(target),
            origin: Origin::CommandLine,
        }
    }
}

/// All known hosts, this machine first. Problems reading the files are
/// returned for the status line rather than failing.
pub fn load() -> (Vec<HostEntry>, Vec<String>) {
    let mut hosts = Vec::new();
    let mut problems = Vec::new();

    #[cfg(target_os = "linux")]
    hosts.push(HostEntry {
        id: LOCAL.to_owned(),
        name: crate::host::local_hostname(),
        kind: HostKind::Local,
        origin: Origin::ThisComputer,
    });

    if let Some(path) = hosts_file() {
        match std::fs::read_to_string(&path) {
            Ok(text) => match parse_saved(&text) {
                Ok(saved) => hosts.extend(saved),
                Err(error) => problems.push(format!("{}: {error}", path.display())),
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => problems.push(format!("{}: {error}", path.display())),
        }
    }

    let ssh_config = BaseDirs::new().map(|dirs| dirs.home_dir().join(".ssh").join("config"));
    if let Some(text) = ssh_config.and_then(|path| std::fs::read_to_string(path).ok()) {
        for alias in ssh_config_aliases(&text) {
            // A saved host for the same alias carries the user's own name.
            let saved = hosts.iter().any(
                |host| matches!(&host.kind, HostKind::Ssh(target) if target.destination == alias),
            );
            if !saved {
                hosts.push(HostEntry {
                    id: format!("ssh-config:{alias}"),
                    name: alias.clone(),
                    kind: HostKind::Ssh(Target::parse(&alias)),
                    origin: Origin::SshConfig,
                });
            }
        }
    }

    (hosts, problems)
}

/// `hosts.toml`, which the user edits.
pub fn hosts_file() -> Option<PathBuf> {
    config_dir().map(|dir| dir.join("hosts.toml"))
}

fn config_dir() -> Option<PathBuf> {
    ProjectDirs::from("", "", "sysorbit").map(|dirs| dirs.config_dir().to_owned())
}

#[derive(Debug, Default, Deserialize)]
struct SavedFile {
    #[serde(default, rename = "host")]
    hosts: Vec<Saved>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Saved {
    id: Option<String>,
    name: Option<String>,
    destination: String,
    port: Option<u16>,
    #[serde(default)]
    options: Vec<String>,
}

fn parse_saved(text: &str) -> Result<Vec<HostEntry>, toml::de::Error> {
    let file: SavedFile = toml::from_str(text)?;
    Ok(file
        .hosts
        .into_iter()
        .map(|saved| {
            let mut target = Target::parse(&saved.destination);
            target.port = saved.port.or(target.port);
            target.options = saved.options;
            let name = saved.name.unwrap_or_else(|| target.to_string());
            HostEntry {
                id: saved.id.unwrap_or_else(|| format!("saved:{}", target)),
                name,
                kind: HostKind::Ssh(target),
                origin: Origin::Saved,
            }
        })
        .collect())
}

/// Concrete `Host` names in an ssh config; patterns with `*`, `?` or `!`
/// match many hosts and name none.
fn ssh_config_aliases(text: &str) -> Vec<String> {
    let mut aliases = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        let Some((keyword, rest)) = line.split_once(|c: char| c.is_whitespace() || c == '=') else {
            continue;
        };
        if !keyword.eq_ignore_ascii_case("host") {
            continue;
        }
        for name in rest.split_whitespace() {
            let name = name.trim_matches('"');
            if !name.contains(['*', '?', '!']) && !aliases.iter().any(|alias| alias == name) {
                aliases.push(name.to_owned());
            }
        }
    }
    aliases
}

/// What the app remembers between runs.
#[derive(Debug, Default, Serialize, Deserialize)]
struct State {
    last_host: Option<String>,
}

/// The id of the host shown when the app was last closed.
pub fn last_host() -> Option<String> {
    let path = config_dir()?.join("state.toml");
    let text = std::fs::read_to_string(path).ok()?;
    toml::from_str::<State>(&text).ok()?.last_host
}

/// Remembers `id` as the host to show next time. Failing to save only
/// loses that convenience, so errors are ignored.
pub fn remember(id: &str) {
    let Some(dir) = config_dir() else {
        return;
    };
    let state = State {
        last_host: Some(id.to_owned()),
    };
    if let Ok(text) = toml::to_string(&state) {
        let _ = write(&dir, "state.toml", &text);
    }
}

fn write(dir: &Path, name: &str, text: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    std::fs::write(dir.join(name), text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_concrete_aliases_from_ssh_config() {
        let config = "\
Host *
    ServerAliveInterval 30
Host prod web-?
    HostName 203.0.113.10
host=staging
Host !bastion backup
Match host foo
";
        assert_eq!(ssh_config_aliases(config), ["prod", "staging", "backup"]);
    }

    #[test]
    fn reads_saved_hosts() {
        let hosts = parse_saved(
            r#"
            [[host]]
            name = "Prod web"
            destination = "admin@203.0.113.10:2222"
            options = ["IdentityFile=~/keys/prod.pem"]

            [[host]]
            destination = "staging"
            port = 2200
            "#,
        )
        .expect("valid hosts.toml");

        assert_eq!(hosts[0].name, "Prod web");
        let HostKind::Ssh(target) = &hosts[0].kind else {
            panic!("ssh host")
        };
        assert_eq!(target.destination, "admin@203.0.113.10");
        assert_eq!(target.port, Some(2222));
        assert_eq!(target.options, ["IdentityFile=~/keys/prod.pem"]);
        assert_eq!(hosts[1].name, "staging:2200");
        assert!(parse_saved("[[host]]\nname = \"no destination\"").is_err());
    }
}
