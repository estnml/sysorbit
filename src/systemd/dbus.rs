//! The parts of systemd's D-Bus API the app reads.
//!
//! Declared here rather than taken from a crate covering the whole API:
//! such generated code also exposes calls that pass file descriptors,
//! which do not exist on Windows, where the app runs to reach Linux hosts
//! over SSH. The signatures follow systemd's `org.freedesktop.systemd1(5)`.

use zbus::OwnedObjectPath;
use zbus::proxy;

/// One row of `ListUnitsByPatterns`: name, description, load state, active
/// state, sub state, followed unit, object path, queued job id, job type and
/// job path.
pub type UnitRow = (
    String,
    String,
    String,
    String,
    String,
    String,
    OwnedObjectPath,
    u32,
    String,
    OwnedObjectPath,
);

#[proxy(
    interface = "org.freedesktop.systemd1.Manager",
    default_service = "org.freedesktop.systemd1",
    default_path = "/org/freedesktop/systemd1"
)]
pub trait Manager {
    #[zbus(property(emits_changed_signal = "const"), name = "Version")]
    fn version(&self) -> zbus::Result<String>;

    #[zbus(name = "Subscribe")]
    fn subscribe(&self) -> zbus::Result<()>;

    #[zbus(name = "ListUnitFilesByPatterns")]
    fn list_unit_files_by_patterns(
        &self,
        states: Vec<String>,
        patterns: Vec<String>,
    ) -> zbus::Result<Vec<(String, String)>>;

    #[zbus(name = "ListUnitsByPatterns")]
    fn list_units_by_patterns(
        &self,
        states: Vec<String>,
        patterns: Vec<String>,
    ) -> zbus::Result<Vec<UnitRow>>;

    #[zbus(name = "LoadUnit")]
    fn load_unit(&self, name: String) -> zbus::Result<OwnedObjectPath>;

    #[zbus(name = "GetUnit")]
    fn get_unit(&self, name: String) -> zbus::Result<OwnedObjectPath>;
}

#[proxy(
    interface = "org.freedesktop.systemd1.Unit",
    default_service = "org.freedesktop.systemd1",
    assume_defaults = false
)]
pub trait Unit {
    #[zbus(property(emits_changed_signal = "const"), name = "Id")]
    fn id(&self) -> zbus::Result<String>;

    #[zbus(property(emits_changed_signal = "const"), name = "Description")]
    fn description(&self) -> zbus::Result<String>;

    #[zbus(property(emits_changed_signal = "true"), name = "ActiveState")]
    fn active_state(&self) -> zbus::Result<String>;

    #[zbus(property(emits_changed_signal = "false"), name = "UnitFileState")]
    fn unit_file_state(&self) -> zbus::Result<String>;
}

#[proxy(
    interface = "org.freedesktop.systemd1.Timer",
    default_service = "org.freedesktop.systemd1",
    assume_defaults = false
)]
pub trait Timer {
    #[zbus(property(emits_changed_signal = "const"), name = "Unit")]
    fn unit(&self) -> zbus::Result<String>;

    /// Base, offset and next elapse of each monotonic trigger.
    #[zbus(
        property(emits_changed_signal = "invalidates"),
        name = "TimersMonotonic"
    )]
    fn timers_monotonic(&self) -> zbus::Result<Vec<(String, u64, u64)>>;

    /// Base, normalized expression and next elapse of each calendar trigger.
    #[zbus(
        property(emits_changed_signal = "invalidates"),
        name = "TimersCalendar"
    )]
    fn timers_calendar(&self) -> zbus::Result<Vec<(String, String, u64)>>;

    #[zbus(
        property(emits_changed_signal = "true"),
        name = "NextElapseUSecRealtime"
    )]
    fn next_elapse_u_sec_realtime(&self) -> zbus::Result<u64>;

    #[zbus(
        property(emits_changed_signal = "true"),
        name = "NextElapseUSecMonotonic"
    )]
    fn next_elapse_u_sec_monotonic(&self) -> zbus::Result<u64>;

    #[zbus(property(emits_changed_signal = "true"), name = "LastTriggerUSec")]
    fn last_trigger_u_sec(&self) -> zbus::Result<u64>;

    #[zbus(property(emits_changed_signal = "const"), name = "AccuracyUSec")]
    fn accuracy_u_sec(&self) -> zbus::Result<u64>;

    #[zbus(property(emits_changed_signal = "const"), name = "RandomizedDelayUSec")]
    fn randomized_delay_u_sec(&self) -> zbus::Result<u64>;

    #[zbus(property(emits_changed_signal = "const"), name = "Persistent")]
    fn persistent(&self) -> zbus::Result<bool>;

    #[zbus(property(emits_changed_signal = "const"), name = "WakeSystem")]
    fn wake_system(&self) -> zbus::Result<bool>;
}

/// systemd-timedated: the host's clock and time zone. Started on demand by
/// the bus; missing in some containers.
#[proxy(
    interface = "org.freedesktop.timedate1",
    default_service = "org.freedesktop.timedate1",
    default_path = "/org/freedesktop/timedate1"
)]
pub trait Timedate {
    #[zbus(property(emits_changed_signal = "true"), name = "Timezone")]
    fn timezone(&self) -> zbus::Result<String>;

    /// Microseconds since the epoch, read when the property is fetched.
    #[zbus(property(emits_changed_signal = "false"), name = "TimeUSec")]
    fn time_usec(&self) -> zbus::Result<u64>;
}
