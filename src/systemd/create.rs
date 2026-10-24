//! Creates a timer with its service: two unit files, then `daemon-reload`
//! and `enable --now`, in one shell script run through the [`Host`].
//!
//! The system scope needs root. On this machine the script runs through
//! `pkexec`, which asks for an administrator password in a polkit dialog;
//! over SSH through `sudo -n`, as `BatchMode` leaves no way to answer a
//! prompt. A root login needs neither.

use super::dbus::ManagerProxy;
use super::{Error, run};
use crate::diagnostics::{self, ErrorCode, Event, Phase};
use crate::host::{Host, Invocation};
use crate::model::Scope;

/// The first line of every file, to tell them apart from the user's own.
const MARK: &str = "# Created by sysorbit";

/// What the dialog asks for, already checked by [`NewTimer::new`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewTimer {
    /// Without `.timer` or `.service`.
    pub name: String,
    pub description: String,
    pub command: String,
    /// One `OnCalendar=` line each; the timer elapses at all of them.
    pub calendars: Vec<String>,
    pub persistent: bool,
}

/// What to fix before a timer can be created.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Problem {
    NameEmpty,
    NameInvalid,
    CommandEmpty,
    CalendarEmpty,
    /// A line break would start a new setting in the unit file.
    LineBreak,
}

impl NewTimer {
    pub fn new(
        name: &str,
        description: &str,
        command: &str,
        calendars: &[String],
        persistent: bool,
    ) -> Result<Self, Problem> {
        if [name, description, command]
            .into_iter()
            .chain(calendars.iter().map(String::as_str))
            .any(|value| value.chars().any(char::is_control))
        {
            return Err(Problem::LineBreak);
        }
        let name = name.trim();
        let name = name
            .strip_suffix(".timer")
            .or_else(|| name.strip_suffix(".service"))
            .unwrap_or(name);
        if name.is_empty() {
            return Err(Problem::NameEmpty);
        }
        // systemd's unit name characters, without `@` for templates and `\`
        // for escapes; the longest suffix, `.service`, must still fit in 255.
        let valid = name.len() <= 255 - ".service".len()
            && !name.starts_with('.')
            && name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b":-_.".contains(&b));
        if !valid {
            return Err(Problem::NameInvalid);
        }
        let command = command.trim();
        if command.is_empty() {
            return Err(Problem::CommandEmpty);
        }
        let calendars: Vec<String> = calendars
            .iter()
            .map(|calendar| calendar.trim())
            .filter(|calendar| !calendar.is_empty())
            .map(str::to_owned)
            .collect();
        if calendars.is_empty() {
            return Err(Problem::CalendarEmpty);
        }
        let description = match description.trim() {
            "" => name,
            description => description,
        };
        Ok(Self {
            name: name.to_owned(),
            description: description.to_owned(),
            command: command.to_owned(),
            calendars,
            persistent,
        })
    }

    pub fn service_name(&self) -> String {
        format!("{}.service", self.name)
    }

    pub fn timer_name(&self) -> String {
        format!("{}.timer", self.name)
    }

    pub fn service_file(&self) -> String {
        format!(
            "{MARK}\n[Unit]\nDescription={}\n\n[Service]\nType=oneshot\nExecStart={}\n",
            literal(&self.description),
            literal(&self.command),
        )
    }

    pub fn timer_file(&self) -> String {
        let calendars: String = self
            .calendars
            .iter()
            .map(|calendar| format!("OnCalendar={calendar}\n"))
            .collect();
        format!(
            "{MARK}\n[Unit]\nDescription={}\n\n[Timer]\n{calendars}Persistent={}\n\n[Install]\nWantedBy=timers.target\n",
            literal(&self.description),
            self.persistent,
        )
    }
}

/// `%` starts a specifier such as `%h`; what the user typed runs as typed.
fn literal(value: &str) -> String {
    value.replace('%', "%%")
}

/// Arguments: scope flag, name, service file, timer file. `set -C` keeps
/// existing files; a timer that cannot be written takes its service along.
const SCRIPT: &str = r#"set -eu
scope=$1 name=$2
if [ "$scope" = --user ]; then
    dir=${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user
    # An SSH session may lack it, and systemctl --user needs it.
    : "${XDG_RUNTIME_DIR:=/run/user/$(id -u)}"
    export XDG_RUNTIME_DIR
else
    dir=/etc/systemd/system
fi
umask 022
mkdir -p "$dir"
set -C
printf '%s' "$3" > "$dir/$name.service"
if ! printf '%s' "$4" > "$dir/$name.timer"; then
    rm -f "$dir/$name.service"
    exit 1
fi
systemctl "$scope" daemon-reload
systemctl "$scope" enable --now "$name.timer"
"#;

/// Runs the rest of its arguments as root: directly when already root,
/// else through the tool in the first argument.
const ELEVATE: &str = r#"tool=$1
shift
if [ "$(id -u)" -eq 0 ]; then
    exec "$@"
fi
exec $tool "$@""#;

/// Checks the schedule and that no unit has the name, then installs and
/// starts the timer.
pub async fn create(host: &dyn Host, scope: Scope, timer: &NewTimer) -> Result<(), Error> {
    diagnostics::measure(Phase::CreateTimer, create_inner(host, scope, timer)).await
}

async fn create_inner(host: &dyn Host, scope: Scope, timer: &NewTimer) -> Result<(), Error> {
    // systemd-analyze names what it cannot parse.
    run(
        host,
        Invocation::new("systemd-analyze")
            .arg("calendar")
            .args(timer.calendars.iter().cloned()),
    )
    .await?;

    let bus = host.bus(scope).await?;
    let taken = ManagerProxy::new(&bus)
        .await?
        .list_unit_files_by_patterns(Vec::new(), vec![timer.service_name(), timer.timer_name()])
        .await?;
    if let Some((path, _)) = taken.first() {
        let name = path.rsplit('/').next().unwrap_or(path);
        return Err(Error::UnitExists(name.to_owned()));
    }

    install(host, scope, timer).await?;
    diagnostics::event(Event::TimerCreated);
    Ok(())
}

async fn install(host: &dyn Host, scope: Scope, timer: &NewTimer) -> Result<(), Error> {
    let invocation = installer(host.is_local(), scope, timer);
    let output = host.run(&invocation).await.map_err(|error| {
        diagnostics::error(ErrorCode::Spawn);
        Error::Install(error.to_string())
    })?;
    if output.success {
        return Ok(());
    }
    diagnostics::error(ErrorCode::Command);
    let stderr = output.stderr.trim();
    if stderr.contains("sudo:") && stderr.contains("password") {
        return Err(Error::PasswordRequired);
    }
    Err(Error::Install(stderr.to_owned()))
}

fn installer(local: bool, scope: Scope, timer: &NewTimer) -> Invocation {
    let flag = match scope {
        Scope::System => "--system",
        Scope::User => "--user",
    };
    let script = ["sh", "-c", SCRIPT, "sh", flag, &timer.name]
        .map(str::to_owned)
        .into_iter()
        .chain([timer.service_file(), timer.timer_file()]);
    match scope {
        Scope::User => Invocation::new("sh").args(script.skip(1)),
        Scope::System => {
            let tool = if local { "pkexec" } else { "sudo -n" };
            Invocation::new("sh")
                .args(["-c", ELEVATE, "sh", tool])
                .args(script)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn backup() -> NewTimer {
        NewTimer::new(
            " backup.timer ",
            "",
            "/usr/bin/echo 100% done",
            &["daily".to_owned()],
            true,
        )
        .unwrap()
    }

    #[test]
    fn names_are_checked_and_suffixes_dropped() {
        assert_eq!(backup().name, "backup");
        assert_eq!(backup().description, "backup");
        let daily = ["daily".to_owned()];
        let new = |name: &str| NewTimer::new(name, "", "true", &daily, false);
        assert_eq!(new("db:nightly-1_x.y").unwrap().name, "db:nightly-1_x.y");
        assert_eq!(new("  "), Err(Problem::NameEmpty));
        assert_eq!(new(".timer"), Err(Problem::NameEmpty));
        for invalid in [
            "a b",
            "a/b",
            "a@b",
            "a\\x2d",
            ".hidden",
            "ü",
            &"a".repeat(248),
        ] {
            assert_eq!(new(invalid), Err(Problem::NameInvalid), "{invalid}");
        }
        assert_eq!(
            NewTimer::new("a", "x\nExecStartPre=/bin/evil", "true", &daily, false),
            Err(Problem::LineBreak)
        );
        assert_eq!(
            NewTimer::new("a", "", " ", &daily, false),
            Err(Problem::CommandEmpty)
        );
        assert_eq!(
            NewTimer::new("a", "", "true", &[" ".to_owned()], false),
            Err(Problem::CalendarEmpty)
        );
    }

    #[test]
    fn unit_files_keep_percent_signs_literal() {
        let timer = backup();
        assert_eq!(
            timer.service_file(),
            "# Created by sysorbit\n[Unit]\nDescription=backup\n\n[Service]\nType=oneshot\nExecStart=/usr/bin/echo 100%% done\n"
        );
        assert_eq!(
            timer.timer_file(),
            "# Created by sysorbit\n[Unit]\nDescription=backup\n\n[Timer]\nOnCalendar=daily\nPersistent=true\n\n[Install]\nWantedBy=timers.target\n"
        );
    }

    #[test]
    fn every_calendar_gets_its_own_line() {
        let timer = NewTimer::new(
            "split",
            "",
            "true",
            &[
                "Mon..Fri 09:00".to_owned(),
                " ".to_owned(),
                "Sat 10:00".to_owned(),
            ],
            false,
        )
        .unwrap();
        assert_eq!(timer.calendars, ["Mon..Fri 09:00", "Sat 10:00"]);
        assert!(
            timer
                .timer_file()
                .contains("OnCalendar=Mon..Fri 09:00\nOnCalendar=Sat 10:00\nPersistent=false\n")
        );
    }

    #[test]
    fn the_system_scope_asks_for_root() {
        let timer = backup();
        let user = installer(true, Scope::User, &timer);
        assert_eq!(user.args[..5], ["-c", SCRIPT, "sh", "--user", "backup"]);
        let local = installer(true, Scope::System, &timer);
        assert_eq!(local.args[..4], ["-c", ELEVATE, "sh", "pkexec"]);
        assert_eq!(local.args[4..9], ["sh", "-c", SCRIPT, "sh", "--system"]);
        let remote = installer(false, Scope::System, &timer);
        assert_eq!(remote.args[3], "sudo -n");
        assert_eq!(remote.args.last(), Some(&timer.timer_file()));
    }

    /// Runs the real script with a stand-in `systemctl` that records its
    /// arguments, installing into a temporary `XDG_CONFIG_HOME`.
    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn the_script_installs_once_and_enables_the_timer() {
        use std::os::unix::fs::PermissionsExt;

        let mut random = [0; 8];
        getrandom::fill(&mut random).unwrap();
        let root = std::env::temp_dir().join(format!(
            "sysorbit-create-test-{:x}",
            u64::from_ne_bytes(random)
        ));
        let bin = root.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let calls = root.join("calls");
        let systemctl = bin.join("systemctl");
        std::fs::write(
            &systemctl,
            format!("#!/bin/sh\necho \"$*\" >> '{}'\n", calls.display()),
        )
        .unwrap();
        std::fs::set_permissions(&systemctl, std::fs::Permissions::from_mode(0o755)).unwrap();

        let host = &crate::host::LocalHost::new();
        let timer = backup();
        let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap());
        let run = |invocation: Invocation| {
            let invocation = invocation
                .env("PATH", &path)
                .env("XDG_CONFIG_HOME", root.display().to_string())
                .env("XDG_RUNTIME_DIR", "/nonexistent");
            async move { host.run(&invocation).await.unwrap() }
        };

        let first = run(installer(true, Scope::User, &timer)).await;
        assert!(first.success, "{}", first.stderr);
        let units = root.join("systemd/user");
        assert_eq!(
            std::fs::read_to_string(units.join("backup.service")).unwrap(),
            timer.service_file()
        );
        assert_eq!(
            std::fs::read_to_string(units.join("backup.timer")).unwrap(),
            timer.timer_file()
        );
        assert_eq!(
            std::fs::read_to_string(&calls).unwrap(),
            "--user daemon-reload\n--user enable --now backup.timer\n"
        );

        // A second run must not replace the files it finds.
        let other =
            NewTimer::new("backup", "", "/bin/false", &["hourly".to_owned()], false).unwrap();
        let second = run(installer(true, Scope::User, &other)).await;
        assert!(!second.success);
        assert_eq!(
            std::fs::read_to_string(units.join("backup.service")).unwrap(),
            timer.service_file()
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    /// Creates `sysorbit-selftest.timer` for this user on the real session
    /// bus, then disables and removes it. Run with
    /// `cargo test real_user_timer -- --ignored`.
    #[cfg(target_os = "linux")]
    #[tokio::test]
    #[ignore = "changes the user's systemd units for a moment"]
    async fn real_user_timer_is_created_and_refused_twice() {
        let host = crate::host::LocalHost::new();
        let timer = NewTimer::new(
            "sysorbit-selftest",
            "",
            "/usr/bin/true",
            &["2099-01-01".to_owned()],
            false,
        )
        .unwrap();
        let created = create(&host, Scope::User, &timer).await;
        let again = create(&host, Scope::User, &timer).await;
        let active = std::process::Command::new("systemctl")
            .args(["--user", "is-active", "sysorbit-selftest.timer"])
            .output()
            .unwrap();

        let cleanup = "systemctl --user disable --now sysorbit-selftest.timer; \
            rm -f \"${XDG_CONFIG_HOME:-$HOME/.config}\"/systemd/user/sysorbit-selftest.*; \
            systemctl --user daemon-reload";
        std::process::Command::new("sh")
            .args(["-c", cleanup])
            .status()
            .unwrap();

        created.unwrap();
        assert!(matches!(again, Err(Error::UnitExists(_))), "{again:?}");
        assert_eq!(String::from_utf8_lossy(&active.stdout).trim(), "active");
    }
}
