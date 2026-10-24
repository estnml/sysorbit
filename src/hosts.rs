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
//! destination = "203.0.113.10"   # or a ~/.ssh/config alias
//! user = "admin"
//! port = 22
//! key_file = "~/keys/prod.pem"   # leave out to use the agent and ~/.ssh/config
//! options = ["ServerAliveInterval=30"]   # more `ssh -o` options
//! ```
//!
//! The app writes the file when a host is added, edited or removed in its
//! host dialog, editing it in place so that comments stay.
//!
//! Files in `hosts.d/` next to it use the same format and are only read:
//! tools that set up hosts, like `tests/fleet/run.sh`, drop theirs there
//! and remove them again without touching the user's own file.

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
    /// The entry in `hosts.toml`, for hosts saved there.
    pub saved: Option<SavedHost>,
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
    /// A file in `hosts.d/`, owned by whatever wrote it.
    DropIn,
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
            saved: None,
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
        saved: None,
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

    if let Some(dir) = config_dir().map(|dir| dir.join("hosts.d")) {
        let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
            .into_iter()
            .flatten()
            .filter_map(|entry| Some(entry.ok()?.path()))
            .filter(|path| {
                path.extension()
                    .is_some_and(|extension| extension == "toml")
            })
            .collect();
        files.sort();
        for path in files {
            let parsed = std::fs::read_to_string(&path)
                .map_err(|error| error.to_string())
                .and_then(|text| parse_saved(&text).map_err(|error| error.to_string()));
            match parsed {
                Ok(dropped) => hosts.extend(dropped.into_iter().map(|entry| HostEntry {
                    origin: Origin::DropIn,
                    ..entry
                })),
                Err(error) => problems.push(format!("{}: {error}", path.display())),
            }
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
                    saved: None,
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
    hosts: Vec<SavedHost>,
}

/// A host as `hosts.toml` describes it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedHost {
    /// Written by the app; a hand-written entry without one is known by
    /// its address.
    pub id: Option<String>,
    pub name: Option<String>,
    /// A hostname, an address or a `~/.ssh/config` alias, which may carry
    /// `user@` and `:port` itself.
    pub destination: String,
    pub user: Option<String>,
    pub port: Option<u16>,
    /// A private key, e.g. a PEM file, used instead of the agent's keys.
    pub key_file: Option<String>,
    /// More `ssh -o` options.
    #[serde(default)]
    pub options: Vec<String>,
}

impl SavedHost {
    pub fn target(&self) -> Target {
        let mut target = Target::parse(&self.destination);
        if let Some(user) = self
            .user
            .as_ref()
            .filter(|_| !target.destination.contains('@'))
        {
            target.destination = format!("{user}@{}", target.destination);
        }
        target.port = self.port.or(target.port);
        target.options = self.options.clone();
        if let Some(key) = &self.key_file {
            // ssh expands a leading `~` in IdentityFile itself.
            target.options.push(format!("IdentityFile={key}"));
            target.options.push("IdentitiesOnly=yes".to_owned());
        }
        target
    }

    pub fn id(&self) -> String {
        self.id
            .clone()
            .unwrap_or_else(|| format!("saved:{}", self.target()))
    }

    fn entry(self) -> HostEntry {
        let target = self.target();
        HostEntry {
            id: self.id(),
            name: self.name.clone().unwrap_or_else(|| target.to_string()),
            kind: HostKind::Ssh(target),
            origin: Origin::Saved,
            saved: Some(self),
        }
    }
}

fn parse_saved(text: &str) -> Result<Vec<HostEntry>, toml::de::Error> {
    let file: SavedFile = toml::from_str(text)?;
    Ok(file.hosts.into_iter().map(SavedHost::entry).collect())
}

/// Writes `host` to `hosts.toml`, in place of the entry with id
/// `replacing` if given, and returns its id.
pub fn save(host: &SavedHost, replacing: Option<&str>) -> Result<String, String> {
    let path = hosts_file().ok_or("no config directory")?;
    save_in(&path, host, replacing)
}

/// Removes the entry with id `id` from `hosts.toml`.
pub fn delete(id: &str) -> Result<(), String> {
    let path = hosts_file().ok_or("no config directory")?;
    delete_in(&path, id)
}

fn delete_in(path: &Path, id: &str) -> Result<(), String> {
    edit_file(path, |hosts| {
        if let Some(index) = position(hosts, id) {
            hosts.remove(index);
        }
        Ok(())
    })
}

fn save_in(path: &Path, host: &SavedHost, replacing: Option<&str>) -> Result<String, String> {
    edit_file(path, |hosts| {
        let index = replacing.and_then(|id| position(hosts, id));
        let id = match (&host.id, replacing) {
            (Some(id), _) => id.clone(),
            (None, Some(id)) if index.is_some() => id.to_owned(),
            (None, _) => unique_id(hosts, host.name.as_deref().unwrap_or(&host.destination)),
        };
        let table = match index {
            Some(index) => hosts.get_mut(index).ok_or("entry vanished")?,
            None => {
                hosts.push(toml_edit::Table::new());
                let last = hosts.len() - 1;
                hosts.get_mut(last).ok_or("entry vanished")?
            }
        };
        set(table, "id", Some(id.clone()));
        set(table, "name", host.name.clone());
        set(table, "destination", Some(host.destination.clone()));
        set(table, "user", host.user.clone());
        match host.port {
            Some(port) => table["port"] = toml_edit::value(i64::from(port)),
            None => {
                table.remove("port");
            }
        }
        set(table, "key_file", host.key_file.clone());
        if host.options.is_empty() {
            table.remove("options");
        } else {
            table["options"] = toml_edit::value(host.options.iter().collect::<toml_edit::Array>());
        }
        Ok(id)
    })
}

/// Runs `change` on the `[[host]]` tables of the file at `path` and
/// writes it back, keeping everything else, comments included.
fn edit_file<T>(
    path: &Path,
    change: impl FnOnce(&mut toml_edit::ArrayOfTables) -> Result<T, String>,
) -> Result<T, String> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error.to_string()),
    };
    let mut document: toml_edit::DocumentMut = text.parse().map_err(|error| format!("{error}"))?;
    let hosts = document
        .entry("host")
        .or_insert(toml_edit::Item::ArrayOfTables(
            toml_edit::ArrayOfTables::new(),
        ))
        .as_array_of_tables_mut()
        .ok_or("`host` is not a list of [[host]] tables")?;
    let result = change(hosts)?;

    let dir = path.parent().ok_or("no config directory")?;
    std::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    // Written next to the file and renamed over it, so a crash never
    // leaves half a file.
    let temporary = path.with_extension("toml.new");
    std::fs::write(&temporary, document.to_string()).map_err(|error| error.to_string())?;
    std::fs::rename(&temporary, path).map_err(|error| error.to_string())?;
    Ok(result)
}

/// The index of the table whose entry has id `id`.
fn position(hosts: &toml_edit::ArrayOfTables, id: &str) -> Option<usize> {
    hosts.iter().position(|table| {
        toml::from_str::<SavedHost>(&table.to_string()).is_ok_and(|host| host.id() == id)
    })
}

/// An id made from `name`, e.g. `prod-web`, that no entry has yet.
fn unique_id(hosts: &toml_edit::ArrayOfTables, name: &str) -> String {
    let slug: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let base = if slug.is_empty() {
        "host".to_owned()
    } else {
        slug
    };
    (1..)
        .map(|n| {
            if n == 1 {
                base.clone()
            } else {
                format!("{base}-{n}")
            }
        })
        .find(|id| position(hosts, id).is_none())
        .unwrap_or(base)
}

/// Sets `key` to `value`, or removes it for `None`; a value already there
/// keeps the comments around it.
fn set(table: &mut toml_edit::Table, key: &str, value: Option<String>) {
    match value {
        Some(value) => table[key] = toml_edit::value(value),
        None => {
            table.remove(key);
        }
    }
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

    /// A file of its own in the temp directory, removed when dropped.
    struct TempFile(PathBuf);

    impl TempFile {
        fn new(name: &str, text: &str) -> Self {
            let path =
                std::env::temp_dir().join(format!("sysorbit-{}-{name}.toml", std::process::id()));
            std::fs::write(&path, text).expect("write temp file");
            Self(path)
        }

        fn read(&self) -> String {
            std::fs::read_to_string(&self.0).expect("read temp file")
        }
    }

    impl Drop for TempFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    #[test]
    fn saving_keeps_comments_and_other_hosts() {
        let file = TempFile::new(
            "save",
            "# My servers\n\n[[host]]\n# the old box\nname = \"Old\"\ndestination = \"old\"\n",
        );
        let new = SavedHost {
            name: Some("Prod web".to_owned()),
            destination: "203.0.113.10".to_owned(),
            user: Some("admin".to_owned()),
            key_file: Some("~/keys/prod.pem".to_owned()),
            ..SavedHost::default()
        };
        let id = save_in(&file.0, &new, None).expect("save new");
        assert_eq!(id, "prod-web");

        // Edit the hand-written entry, known by its address.
        let old = SavedHost {
            name: Some("Old box".to_owned()),
            destination: "old".to_owned(),
            port: Some(2200),
            ..SavedHost::default()
        };
        save_in(&file.0, &old, Some("saved:old")).expect("save edit");

        let text = file.read();
        assert!(text.starts_with("# My servers"), "{text}");
        assert!(text.contains("# the old box\nname = \"Old box\""), "{text}");
        let hosts = parse_saved(&text).expect("valid file");
        assert_eq!(hosts.len(), 2);
        assert_eq!(hosts[0].id, "saved:old");
        let HostKind::Ssh(target) = &hosts[1].kind else {
            panic!("ssh host")
        };
        assert_eq!(target.destination, "admin@203.0.113.10");
        assert!(
            target
                .options
                .contains(&"IdentityFile=~/keys/prod.pem".to_owned())
        );

        delete_in(&file.0, "prod-web").expect("delete");
        assert_eq!(parse_saved(&file.read()).expect("valid file").len(), 1);
    }
}
