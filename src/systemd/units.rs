//! Timer definitions from systemd's D-Bus API.
//!
//! The D-Bus interface is systemd's stable API: properties arrive as raw
//! numbers, independent of the systemd version, locale or output format.

use std::collections::BTreeSet;

use super::dbus::{ManagerProxy, TimerProxy, UnitProxy};
use futures_util::future::join_all;
use jiff::{SignedDuration, Timestamp};
use zbus::Connection;

use super::Error;
use crate::model::{CalendarTrigger, MonotonicBase, MonotonicTrigger, Timer, Warning};

/// Loads every timer unit: installed unit files plus loaded instances.
/// Templates (`foo@.timer`) are skipped, they cannot run by themselves.
///
/// `clock_offset` is the host's realtime minus its monotonic clock; it turns
/// monotonic elapses into wall clock time. Timers that fail to load are
/// returned as warnings instead of failing the whole list.
pub async fn load(
    bus: &Connection,
    clock_offset: Option<SignedDuration>,
) -> Result<(Vec<Timer>, Vec<Warning>), Error> {
    let manager = ManagerProxy::new(bus).await?;
    let patterns = vec!["*.timer".to_owned()];
    let files = manager
        .list_unit_files_by_patterns(Vec::new(), patterns.clone())
        .await?;
    let loaded = manager.list_units_by_patterns(Vec::new(), patterns).await?;

    let names: BTreeSet<String> = files
        .into_iter()
        .filter_map(|(path, _state)| Some(path.rsplit_once('/')?.1.to_owned()))
        .chain(loaded.into_iter().map(|unit| unit.0))
        .filter(|name| !name.contains("@."))
        .collect();

    let results = join_all(
        names
            .iter()
            .map(|name| load_timer(bus, &manager, name, clock_offset)),
    )
    .await;

    let mut timers = Vec::new();
    let mut warnings = Vec::new();
    for (name, result) in names.iter().zip(results) {
        match result {
            Ok(timer) => timers.push(timer),
            Err(error) => warnings.push(Warning::Unreadable {
                unit: name.clone(),
                error: error.to_string(),
            }),
        }
    }

    // Aliases such as `systemd-sysupdate.timer` resolve to the same unit.
    timers.sort_by(|a, b| a.id.cmp(&b.id));
    timers.dedup_by(|a, b| a.id == b.id);

    Ok((timers, warnings))
}

async fn load_timer(
    bus: &Connection,
    manager: &ManagerProxy<'_>,
    name: &str,
    clock_offset: Option<SignedDuration>,
) -> Result<Timer, Error> {
    // LoadUnit also works for timers that are installed but not loaded.
    let path = manager.load_unit(name.to_owned()).await?;
    let unit = UnitProxy::builder(bus).path(path.clone()).build().await?;
    let timer = TimerProxy::builder(bus).path(path).build().await?;

    let calendar = timer
        .timers_calendar()
        .await?
        .into_iter()
        .map(|(_base, expression, _next)| CalendarTrigger {
            expression,
            occurrences: Vec::new(),
        })
        .collect();

    let monotonic = timer
        .timers_monotonic()
        .await?
        .into_iter()
        .filter_map(|(base, offset, _next)| {
            Some(MonotonicTrigger {
                base: monotonic_base(&base)?,
                offset: duration(offset)?,
            })
        })
        .collect();

    // systemd keeps the next calendar and the next monotonic elapse apart;
    // whichever comes first is the next elapse.
    let realtime = timestamp(timer.next_elapse_u_sec_realtime().await?);
    let monotonic_next = duration(timer.next_elapse_u_sec_monotonic().await?)
        .filter(|elapse| elapse.is_positive())
        .zip(clock_offset)
        .and_then(|(elapse, offset)| {
            Timestamp::from_microsecond((elapse + offset).as_micros() as i64).ok()
        });
    let next_elapse = match (realtime, monotonic_next) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    };

    let service = timer.unit().await?;
    // GetUnit fails for services that are not loaded, which means inactive.
    let service_active = match manager.get_unit(service.clone()).await {
        Ok(path) => {
            let state = UnitProxy::builder(bus)
                .path(path)
                .build()
                .await?
                .active_state()
                .await?;
            matches!(state.as_str(), "active" | "activating" | "reloading")
        }
        Err(_) => false,
    };

    Ok(Timer {
        id: unit.id().await?,
        description: unit.description().await?,
        service,
        service_active,
        active: unit.active_state().await? == "active",
        unit_file_state: unit.unit_file_state().await?,
        calendar,
        monotonic,
        next_elapse,
        last_trigger: timestamp(timer.last_trigger_u_sec().await?),
        persistent: timer.persistent().await?,
        wake_system: timer.wake_system().await?,
        accuracy: duration(timer.accuracy_u_sec().await?).unwrap_or_default(),
        randomized_delay: duration(timer.randomized_delay_u_sec().await?).unwrap_or_default(),
        runs: Vec::new(),
        stopped: Vec::new(),
    })
}

fn monotonic_base(name: &str) -> Option<MonotonicBase> {
    Some(match name {
        "OnActiveUSec" => MonotonicBase::TimerActivated,
        "OnBootUSec" => MonotonicBase::Boot,
        "OnStartupUSec" => MonotonicBase::Startup,
        "OnUnitActiveUSec" => MonotonicBase::UnitActivated,
        "OnUnitInactiveUSec" => MonotonicBase::UnitDeactivated,
        _ => return None,
    })
}

/// systemd uses 0 for "never" and `u64::MAX` for "infinity".
fn timestamp(usec: u64) -> Option<Timestamp> {
    if usec == 0 || usec == u64::MAX {
        return None;
    }
    Timestamp::from_microsecond(i64::try_from(usec).ok()?).ok()
}

fn duration(usec: u64) -> Option<SignedDuration> {
    if usec == u64::MAX {
        return None;
    }
    Some(SignedDuration::from_micros(i64::try_from(usec).ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_systemd_sentinels() {
        assert_eq!(timestamp(0), None);
        assert_eq!(timestamp(u64::MAX), None);
        assert_eq!(
            timestamp(1_792_789_200_000_000).unwrap().as_second(),
            1_792_789_200
        );
        assert_eq!(duration(u64::MAX), None);
        assert_eq!(duration(900_000_000), Some(SignedDuration::from_mins(15)));
    }

    #[test]
    fn maps_monotonic_bases() {
        assert_eq!(monotonic_base("OnBootUSec"), Some(MonotonicBase::Boot));
        assert_eq!(
            monotonic_base("OnUnitActiveUSec"),
            Some(MonotonicBase::UnitActivated)
        );
        assert_eq!(monotonic_base("OnSomethingNew"), None);
    }
}
