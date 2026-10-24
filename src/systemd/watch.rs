//! Change notifications from systemd over D-Bus, so the view updates the
//! moment a timer fires or a run ends instead of on the next poll.

use std::collections::HashSet;
use std::future::ready;
use std::num::NonZeroUsize;

use futures_util::{Stream, StreamExt, stream};
use zbus::message::Type;

use super::dbus::ManagerProxy;
use zbus::{Connection, MatchRule, MessageStream, OwnedObjectPath};

use super::Error;

const MANAGER_PATH: &str = "/org/freedesktop/systemd1";
const UNIT_PATHS: &str = "/org/freedesktop/systemd1/unit";
const QUEUE: Option<NonZeroUsize> = NonZeroUsize::new(256);

/// What to react to.
///
/// Reading an inactive timer loads it, and systemd unloads it again right
/// away, announcing both with `UnitNew` and `UnitRemoved`. Reacting to those
/// would make every refresh trigger the next one, so inactive timers are
/// only known by name: they cannot fire, and enabling one sends
/// `UnitFilesChanged`.
#[derive(Debug, Clone, Default, Hash)]
pub struct Watched {
    /// Active timers and their services; any property change matters.
    pub active: Vec<String>,
    /// Every timer in the current view, active or not.
    pub timers: Vec<String>,
}

/// A stream that yields `()` whenever a watched unit changes, or the set of
/// timers itself changed. Signals come in bursts; callers debounce.
pub async fn changes(
    bus: &Connection,
    watched: &Watched,
) -> Result<impl Stream<Item = ()> + Send + Unpin + use<>, Error> {
    // systemd only emits unit signals while at least one client subscribed.
    match ManagerProxy::new(bus).await?.subscribe().await {
        Ok(()) => {}
        // A new watcher on a connection that already subscribed.
        Err(error) if error.to_string().contains("AlreadySubscribed") => {}
        Err(error) => return Err(error.into()),
    }

    let properties = MatchRule::builder()
        .msg_type(Type::Signal)
        .interface("org.freedesktop.DBus.Properties")
        .member("PropertiesChanged")
        .path_namespace(UNIT_PATHS)
        .build()?;
    let manager = MatchRule::builder()
        .msg_type(Type::Signal)
        .interface("org.freedesktop.systemd1.Manager")
        .path(MANAGER_PATH)
        .build()?;

    let properties = MessageStream::for_match_rule(properties, bus, QUEUE).await?;
    let manager = MessageStream::for_match_rule(manager, bus, QUEUE).await?;

    let paths: HashSet<String> = watched.active.iter().map(|unit| unit_path(unit)).collect();
    let units_changed = properties.filter_map(move |message| {
        let relevant = message.ok().is_some_and(|message| {
            message
                .header()
                .path()
                .is_some_and(|path| paths.contains(path.as_str()))
        });
        ready(relevant.then_some(()))
    });

    let active: HashSet<String> = watched.active.iter().cloned().collect();
    let known: HashSet<String> = watched.timers.iter().cloned().collect();
    let timers_changed = manager.filter_map(move |message| {
        let relevant = message.ok().is_some_and(|message| {
            let member = message.header().member().map(|member| member.to_string());
            let timer = || {
                message
                    .body()
                    .deserialize::<(String, OwnedObjectPath)>()
                    .ok()
                    .map(|(name, _)| name)
                    .filter(|name| name.ends_with(".timer"))
            };
            match member.as_deref() {
                Some("UnitFilesChanged" | "Reloading") => true,
                // A timer nobody has seen, e.g. from `systemd-run --on-calendar`.
                Some("UnitNew") => timer().is_some_and(|name| !known.contains(&name)),
                // An active (e.g. transient) timer going away.
                Some("UnitRemoved") => timer().is_some_and(|name| active.contains(&name)),
                _ => false,
            }
        });
        ready(relevant.then_some(()))
    });

    Ok(stream::select(units_changed, timers_changed))
}

/// systemd's object path for a unit: bytes other than ASCII letters and
/// digits, and a leading digit, become `_xx` in hex.
pub fn unit_path(name: &str) -> String {
    let mut path = format!("{UNIT_PATHS}/");
    if name.is_empty() {
        path.push('_');
    }
    for (index, byte) in name.bytes().enumerate() {
        if byte.is_ascii_alphabetic() || (byte.is_ascii_digit() && index > 0) {
            path.push(byte as char);
        } else {
            path.push_str(&format!("_{byte:02x}"));
        }
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_unit_names_like_systemd() {
        assert_eq!(
            unit_path("shadow.service"),
            "/org/freedesktop/systemd1/unit/shadow_2eservice"
        );
        assert_eq!(
            unit_path("systemd-tmpfiles-clean.timer"),
            "/org/freedesktop/systemd1/unit/systemd_2dtmpfiles_2dclean_2etimer"
        );
        assert_eq!(
            unit_path("0x_a@b.timer"),
            "/org/freedesktop/systemd1/unit/_30x_5fa_40b_2etimer"
        );
    }

    /// Starts a transient user timer and expects a notification. Run with
    /// `cargo test live_changes -- --ignored`.
    #[cfg(target_os = "linux")]
    #[tokio::test]
    #[ignore = "needs a systemd user session"]
    async fn live_changes() {
        use std::time::Duration;

        use crate::host::{Host, Invocation, LocalHost};
        use crate::model::Scope;

        let host = LocalHost::new();
        let bus = host.bus(Scope::User).await.expect("session bus");
        let mut changes = changes(&bus, &Watched::default()).await.expect("watch");

        let unit = format!("sysorbit-watch-test-{}", std::process::id());
        let output = host
            .run(&Invocation::new("systemd-run").args([
                "--user",
                "--on-active=30s",
                &format!("--unit={unit}"),
                "/bin/true",
            ]))
            .await
            .expect("systemd-run");
        assert!(output.success, "{}", output.stderr);

        let event = tokio::time::timeout(Duration::from_secs(5), changes.next()).await;

        let _ = host
            .run(&Invocation::new("systemctl").args(["--user", "stop", &format!("{unit}.timer")]))
            .await;
        assert_eq!(event, Ok(Some(())), "no change notification");
    }
}
