//! Plain data describing the timers of one host. Filled by [`crate::systemd`],
//! read by [`crate::timeline`] and the UI.

use jiff::{SignedDuration, Timestamp};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Scope {
    #[default]
    System,
    User,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Interval {
    pub start: Timestamp,
    pub end: Timestamp,
}

impl Interval {
    pub fn contains(&self, at: Timestamp) -> bool {
        self.start <= at && at < self.end
    }

    pub fn overlaps(&self, other: &Interval) -> bool {
        self.start < other.end && other.start < self.end
    }
}

#[derive(Debug, Clone)]
pub struct Snapshot {
    pub scope: Scope,
    pub fetched_at: Timestamp,
    pub timers: Vec<Timer>,
    pub downtime: Vec<Downtime>,
    /// False when the journal could not be read; past occurrences then cannot
    /// be judged as skipped.
    pub history_known: bool,
    pub warnings: Vec<Warning>,
}

/// Something the snapshot could not fully show. Errors carry systemd's or
/// the tool's own message, which stays untranslated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Warning {
    OldSystemd {
        version: u32,
        minimum: u32,
    },
    /// systemd < 242 lists only the next calendar elapse.
    NextElapseOnly,
    /// systemd < 244 cannot list calendar elapses before now.
    NoPastElapses,
    MonotonicUnavailable(String),
    HistoryUnavailable(String),
    DowntimeUnavailable(String),
    Unreadable {
        unit: String,
        error: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DowntimeKind {
    PoweredOff,
    Suspended,
}

#[derive(Debug, Clone, Copy)]
pub struct Downtime {
    pub interval: Interval,
    pub kind: DowntimeKind,
}

#[derive(Debug, Clone)]
pub struct Timer {
    pub id: String,
    pub description: String,
    pub service: String,
    /// Whether the service is running (or starting) right now.
    pub service_active: bool,
    pub active: bool,
    /// `enabled`, `disabled`, `static`, ...
    pub unit_file_state: String,
    pub calendar: Vec<CalendarTrigger>,
    pub monotonic: Vec<MonotonicTrigger>,
    pub next_elapse: Option<Timestamp>,
    pub last_trigger: Option<Timestamp>,
    pub persistent: bool,
    pub wake_system: bool,
    pub accuracy: SignedDuration,
    pub randomized_delay: SignedDuration,
    pub runs: Vec<Run>,
    /// Stretches when the timer was stopped while the machine was up, or
    /// did not exist yet; its elapses there were never due.
    pub stopped: Vec<Interval>,
}

impl Timer {
    pub fn name(&self) -> &str {
        self.id.strip_suffix(".timer").unwrap_or(&self.id)
    }
}

#[derive(Debug, Clone)]
pub struct CalendarTrigger {
    /// Normalized `OnCalendar=` expression as reported by systemd.
    pub expression: String,
    /// Elapse times inside the snapshot window, ignoring randomized delay.
    pub occurrences: Vec<Timestamp>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MonotonicBase {
    /// `OnActiveSec=`
    TimerActivated,
    /// `OnBootSec=`
    Boot,
    /// `OnStartupSec=`
    Startup,
    /// `OnUnitActiveSec=`
    UnitActivated,
    /// `OnUnitInactiveSec=`
    UnitDeactivated,
}

impl MonotonicBase {
    /// Whether the trigger fires again after every run.
    pub fn repeats(self) -> bool {
        matches!(self, Self::UnitActivated | Self::UnitDeactivated)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MonotonicTrigger {
    pub base: MonotonicBase,
    pub offset: SignedDuration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Running,
    Success,
    /// Carries systemd's result string, e.g. `exit-code`.
    Failed(String),
}

#[derive(Debug, Clone)]
pub struct Run {
    pub start: Timestamp,
    pub end: Option<Timestamp>,
    pub outcome: Outcome,
    /// systemd's invocation ID, which tags every log line of this run.
    pub invocation: Option<String>,
}

/// One line a run wrote to the journal.
#[derive(Debug, Clone)]
pub struct LogLine {
    pub at: Timestamp,
    /// syslog priority: 0 emergency … 3 error, 4 warning … 7 debug.
    pub priority: u8,
    pub message: String,
}
