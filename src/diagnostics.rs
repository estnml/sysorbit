//! The logging boundary accepts codes, counts and pseudonyms, never user text.

use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use hmac::{Hmac, Mac};
use sha2::Sha256;
use tracing::Instrument;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::filter::filter_fn;
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::prelude::*;
use tracing_subscriber::{EnvFilter, Registry};

const TARGET: &str = "sysorbit::diagnostics";
const FILE_LIMIT: u64 = 1_400_000;
const KEEP: usize = 7;
static SALT: OnceLock<[u8; 32]> = OnceLock::new();

/// Only the digest is retained: formatting cannot reveal the original value.
#[derive(Clone, Copy)]
pub struct Redacted {
    kind: &'static str,
    digest: Option<[u8; 4]>,
}

impl Redacted {
    pub fn host(value: &str) -> Self {
        Self::new("host", value, SALT.get())
    }

    pub fn unit(value: &str) -> Self {
        Self::new("unit", value, SALT.get())
    }

    fn new(kind: &'static str, value: &str, salt: Option<&[u8; 32]>) -> Self {
        let digest = salt.and_then(|salt| {
            let mut mac = Hmac::<Sha256>::new_from_slice(salt).ok()?;
            // Domain separation keeps a host and a unit with the same name apart.
            mac.update(kind.as_bytes());
            mac.update(&[0]);
            mac.update(value.as_bytes());
            let bytes = mac.finalize().into_bytes();
            Some([bytes[0], bytes[1], bytes[2], bytes[3]])
        });
        Self { kind, digest }
    }
}

impl fmt::Display for Redacted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}#", self.kind)?;
        match self.digest {
            Some(bytes) => {
                for byte in bytes {
                    write!(f, "{byte:02x}")?;
                }
                Ok(())
            }
            None => f.write_str("unavailable"),
        }
    }
}

impl fmt::Debug for Redacted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Phase {
    Snapshot,
    BusConnect,
    Capabilities,
    Clock,
    ClockOffset,
    Units,
    Calendar,
    JournalHistory,
    Downtime,
    Probe,
    Command,
    SshCommand,
    SshConnect,
    LiveDebounce,
    Rebuild,
    CanvasRebuild,
    LogLoading,
    CreateTimer,
}

/// Explicitly instrument futures: holding an entered span across await would
/// misattribute concurrent work on the same executor thread.
pub async fn measure<T>(phase: Phase, future: impl Future<Output = T>) -> T {
    future.instrument(span(phase)).await
}

pub async fn measure_host<T>(phase: Phase, host: Redacted, future: impl Future<Output = T>) -> T {
    future
        .instrument(tracing::info_span!(target: TARGET, "phase", phase = ?phase, host = %host))
        .await
}

pub fn span(phase: Phase) -> tracing::Span {
    tracing::info_span!(target: TARGET, "phase", phase = ?phase)
}

#[derive(Debug, Clone, Copy)]
pub enum Event {
    Start,
    Exit,
    HostSwitch,
    ScopeChange,
    ZoomChange,
    DialogOpen,
    DialogClose,
    Connected,
    Lost,
    Reconnecting,
    Refresh,
    PollRefresh,
    LiveRefresh,
    CalendarSingleRetry,
    BootsTextFallback,
    ServiceInactiveFallback,
    ClockDateFallback,
    TimerCreated,
}

pub fn event(event: Event) {
    tracing::info!(target: TARGET, event = ?event);
}

pub fn scope_change(scope: crate::model::Scope) {
    tracing::info!(target: TARGET, event = ?Event::ScopeChange, scope = ?scope);
}

pub fn zoom_change(seconds: i64) {
    tracing::info!(target: TARGET, event = ?Event::ZoomChange, zoom_seconds = seconds);
}

pub fn host_event(event: Event, host: Redacted) {
    tracing::info!(target: TARGET, event = ?event, host = %host);
}

#[derive(Debug, Clone, Copy)]
pub enum ErrorCode {
    Spawn,
    Command,
    Parse,
    Bus,
    BusUnknownMethod,
    BusAccessDenied,
    BusDisconnected,
    JournalRestricted,
}

pub fn error(code: ErrorCode) {
    tracing::warn!(target: TARGET, error = ?code, count = 1);
}

pub fn unit_error(unit: Redacted, code: ErrorCode) {
    tracing::warn!(target: TARGET, unit = %unit, error = ?code, count = 1);
}

#[derive(Debug, Clone, Copy)]
pub enum Parser {
    Calendar,
    Journal,
    JournalLogs,
    Clock,
    Boots,
    Capabilities,
}

pub fn parse_error(parser: Parser) {
    tracing::warn!(target: TARGET, error = ?ErrorCode::Parse, parser = ?parser, count = 1);
}

pub fn parsed(parser: Parser, lines: usize, parsed: usize) {
    tracing::info!(target: TARGET, parser = ?parser, journal_lines = lines, parsed);
    if parsed < lines {
        parse_error(parser);
    }
}

pub fn command_output(bytes: usize, success: bool) {
    tracing::debug!(target: TARGET, output_bytes = bytes, success);
}

pub fn counts(timers: usize, runs: usize, marks: usize, lines: usize) {
    tracing::info!(target: TARGET, timers, runs, marks, journal_lines = lines);
}

pub fn capabilities(host: Redacted, caps: crate::systemd::Capabilities) {
    tracing::info!(target: TARGET, host = %host, systemd = ?caps.version,
        calendar_iterations = caps.calendar_iterations(),
        calendar_base_time = caps.calendar_base_time(), boots_json = caps.boots_json());
}

pub fn bus_error(error: &zbus::Error) -> ErrorCode {
    match error {
        zbus::Error::MethodError(name, _, _) => match name.as_str() {
            "org.freedesktop.DBus.Error.UnknownMethod" => ErrorCode::BusUnknownMethod,
            "org.freedesktop.DBus.Error.AccessDenied" => ErrorCode::BusAccessDenied,
            "org.freedesktop.DBus.Error.Disconnected" => ErrorCode::BusDisconnected,
            _ => ErrorCode::Bus,
        },
        zbus::Error::InputOutput(_) => ErrorCode::BusDisconnected,
        _ => ErrorCode::Bus,
    }
}

/// Only this module's audited records are eligible, including with a user
/// supplied filter. Dependency warnings can contain private D-Bus messages too.
pub fn layer<W>(writer: W, filter: EnvFilter) -> impl tracing_subscriber::Layer<Registry>
where
    W: for<'a> tracing_subscriber::fmt::MakeWriter<'a> + Send + Sync + 'static,
{
    tracing_subscriber::fmt::layer()
        .with_writer(writer)
        .with_ansi(false)
        .with_target(false)
        .with_span_events(FmtSpan::CLOSE)
        .log_internal_errors(false)
        .with_filter(filter_fn(|metadata| metadata.target() == TARGET))
        .with_filter(filter)
}

pub fn init() -> Option<WorkerGuard> {
    let directory = log_directory()?;
    private_directory(&directory).ok()?;
    let salt = load_salt(&directory).ok()?;
    let _ = SALT.set(salt);
    let writer = DailyWriter::new(directory);
    let (writer, guard) = tracing_appender::non_blocking::NonBlockingBuilder::default()
        .buffered_lines_limit(512)
        .lossy(true)
        .finish(writer);
    let filter = std::env::var("SYSORBIT_LOG")
        .ok()
        .and_then(|value| EnvFilter::builder().with_regex(false).parse(value).ok())
        .unwrap_or_else(|| EnvFilter::new("warn,sysorbit=info"));
    tracing_subscriber::registry()
        .with(layer(writer, filter))
        .try_init()
        .ok()?;
    // Chain to the previous hook so the panic message still reaches stderr.
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic| {
        let (file, line) = panic.location().map_or(("unknown", 0), |location| {
            // Dependency paths can include a username or a package cache path.
            let file = location.file().strip_prefix("src/").unwrap_or("dependency");
            (file, location.line())
        });
        tracing::error!(target: TARGET, event = "Panic", file, line,
            thread = ?std::thread::current().id());
        previous(panic);
    }));
    event(Event::Start);
    tracing::info!(target: TARGET, version = env!("CARGO_PKG_VERSION"),
        os = std::env::consts::OS, os_version = %os_version(),
        architecture = std::env::consts::ARCH, locale = %crate::i18n::language(),
        profile = if cfg!(debug_assertions) { "debug" } else { "release" });
    Some(guard)
}

fn log_directory() -> Option<PathBuf> {
    let base = directories::BaseDirs::new()?;
    #[cfg(target_os = "linux")]
    return Some(base.state_dir()?.join("sysorbit/logs"));
    #[cfg(target_os = "macos")]
    return Some(base.home_dir().join("Library/Logs/sysorbit"));
    #[cfg(target_os = "windows")]
    return Some(base.data_local_dir().join("sysorbit/logs"));
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        let _ = base;
        None
    }
}

fn private_directory(path: &Path) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)
}

fn private_file(path: &Path, new: bool) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true);
    if new {
        options.create_new(true);
    } else {
        // Reject links rather than following them into a user's other files.
        if fs::symlink_metadata(path).is_ok_and(|metadata| !metadata.is_file()) {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        options.create(true).append(true);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

fn load_salt(directory: &Path) -> io::Result<[u8; 32]> {
    let path = directory.join("salt");
    let read = || -> io::Result<[u8; 32]> {
        if !fs::symlink_metadata(&path)?.is_file() {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        let mut key = [0; 32];
        let mut file = File::open(&path)?;
        if file.metadata()?.len() != 32 {
            return Err(io::ErrorKind::InvalidData.into());
        }
        file.read_exact(&mut key)?;
        Ok(key)
    };
    match private_file(&path, true) {
        Ok(mut file) => {
            let mut key = [0; 32];
            getrandom::fill(&mut key).map_err(|_| io::Error::other("random source unavailable"))?;
            file.write_all(&key)?;
            Ok(key)
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => read(),
        Err(error) => Err(error),
    }
}

fn log_files(directory: &Path) -> io::Result<Vec<PathBuf>> {
    let mut files: Vec<_> = fs::read_dir(directory)?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    name.len() == 23
                        && name.starts_with("sysorbit-")
                        && name.ends_with(".log")
                        && name[9..19].bytes().enumerate().all(|(i, b)| {
                            if i == 4 || i == 7 {
                                b == b'-'
                            } else {
                                b.is_ascii_digit()
                            }
                        })
                })
        })
        .collect();
    files.sort();
    Ok(files)
}

/// Runs only on tracing-appender's worker. At the daily budget, discard records
/// until tomorrow rather than creating an unbounded number of rotated files.
struct DailyWriter {
    directory: PathBuf,
    day: String,
    file: Option<File>,
    size: u64,
    disabled: bool,
}

impl DailyWriter {
    fn new(directory: PathBuf) -> Self {
        Self {
            directory,
            day: String::new(),
            file: None,
            size: 0,
            disabled: false,
        }
    }

    fn write_record(&mut self, bytes: &[u8]) -> io::Result<()> {
        let day = jiff::Timestamp::now().strftime("%Y-%m-%d").to_string();
        if day != self.day {
            self.file = None;
            let files = log_files(&self.directory)?;
            let current = self.directory.join(format!("sysorbit-{day}.log"));
            // Keep today's file even if the clock moved backwards.
            let others: Vec<_> = files.into_iter().filter(|path| *path != current).collect();
            let excess = others.len().saturating_sub(KEEP - 1);
            for (index, path) in others.into_iter().enumerate() {
                if index < excess || oversized(&path)? {
                    remove_managed(&path)?;
                }
            }
            if oversized(&current)? {
                remove_managed(&current)?;
            }
            let file = private_file(&current, false)?;
            self.size = file.metadata()?.len();
            self.file = Some(file);
            self.day = day;
        }
        if let Some(file) = &mut self.file {
            // Account for another running instance appending to the same file.
            self.size = file.metadata()?.len();
            if self.size + bytes.len() as u64 <= FILE_LIMIT {
                file.write_all(bytes)?;
                self.size += bytes.len() as u64;
            }
        }
        Ok(())
    }
}

// Another running instance may clean up the same file first; a file that is
// already gone is not a failure.
fn oversized(path: &Path) -> io::Result<bool> {
    match fs::metadata(path) {
        Ok(metadata) => Ok(metadata.len() > FILE_LIMIT),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

fn remove_managed(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

impl Write for DailyWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if !self.disabled && self.write_record(bytes).is_err() {
            // Retrying a broken filesystem for every record can stall the worker.
            self.disabled = true;
            self.file = None;
        }
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn os_version() -> String {
    #[cfg(target_os = "linux")]
    let raw = rustix::system::uname()
        .release()
        .to_string_lossy()
        .into_owned();
    #[cfg(target_os = "macos")]
    let raw = std::process::Command::new("sw_vers")
        .arg("-productVersion")
        .output()
        .ok()
        .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
        .unwrap_or_default();
    #[cfg(target_os = "windows")]
    let raw = std::process::Command::new("cmd")
        .args(["/C", "ver"])
        .output()
        .ok()
        .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
        .unwrap_or_default();
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    let raw = String::new();
    let version: String = raw
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .take(40)
        .collect();
    if version.is_empty() {
        "unknown".to_owned()
    } else {
        version
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};
    use tracing::instrument::WithSubscriber;

    use super::*;
    use crate::host::{BoxFuture, Host, Invocation, Output};
    use crate::model::{Interval, Scope};

    const SECRET: &str = "secret-host.example.com alice 10.1.2.3 customer-invoice.timer";

    #[derive(Clone, Default)]
    struct Capture(Arc<Mutex<Vec<u8>>>);

    impl Write for Capture {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn pseudonyms_are_stable_keyed_and_never_format_the_input() {
        let first = Redacted::new("host", SECRET, Some(&[1; 32]));
        let again = Redacted::new("host", SECRET, Some(&[1; 32]));
        let other = Redacted::new("host", SECRET, Some(&[2; 32]));
        assert_eq!(first.to_string(), again.to_string());
        assert_ne!(first.to_string(), other.to_string());
        assert_ne!(
            first.to_string(),
            Redacted::new("unit", SECRET, Some(&[1; 32])).to_string()
        );
        assert_ne!(
            first.to_string(),
            Redacted::new("host", "other", Some(&[1; 32])).to_string()
        );
        assert_eq!(first.to_string().len(), 13);
        for redacted in [first, Redacted::new("host", SECRET, None)] {
            for text in [format!("{redacted}"), format!("{redacted:?}")] {
                for secret in SECRET.split_whitespace() {
                    assert!(!text.contains(secret));
                }
            }
        }
    }

    fn temporary_directory() -> PathBuf {
        let mut random = [0; 8];
        getrandom::fill(&mut random).unwrap();
        let path = std::env::temp_dir().join(format!(
            "sysorbit-logging-test-{:x}",
            u64::from_ne_bytes(random)
        ));
        private_directory(&path).unwrap();
        path
    }

    #[test]
    fn salt_is_persisted_and_private() {
        let directory = temporary_directory();
        let first = load_salt(&directory).unwrap();
        assert_eq!(first, load_salt(&directory).unwrap());
        assert_ne!(first, [0; 32]);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(directory.join("salt"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn rotation_and_daily_budget_bound_disk_usage() {
        let directory = temporary_directory();
        fs::write(directory.join("salt"), [7; 32]).unwrap();
        fs::write(directory.join("unrelated.txt"), b"keep").unwrap();
        for day in 1..=10 {
            fs::write(
                directory.join(format!("sysorbit-2000-01-{day:02}.log")),
                b"old\n",
            )
            .unwrap();
        }
        let mut writer = DailyWriter::new(directory.clone());
        writer.write_all(b"current\n").unwrap();
        assert_eq!(log_files(&directory).unwrap().len(), KEEP);
        assert_eq!(fs::read(directory.join("salt")).unwrap(), [7; 32]);
        assert!(directory.join("unrelated.txt").exists());
        let remaining = FILE_LIMIT - writer.size;
        writer.write_all(&vec![b'x'; remaining as usize]).unwrap();
        writer.write_all(b"discard\n").unwrap();
        assert_eq!(
            writer.file.as_ref().unwrap().metadata().unwrap().len(),
            FILE_LIMIT
        );
        let total: u64 = log_files(&directory)
            .unwrap()
            .iter()
            .map(|p| fs::metadata(p).unwrap().len())
            .sum();
        assert!(total <= KEEP as u64 * FILE_LIMIT);
        // Future-dated files simulate the wall clock moving backwards.
        for day in 1..=10 {
            fs::write(
                directory.join(format!("sysorbit-9999-01-{day:02}.log")),
                b"future\n",
            )
            .unwrap();
        }
        writer.day = "previous day".to_owned();
        writer.write_all(b"rollover\n").unwrap();
        assert_eq!(log_files(&directory).unwrap().len(), KEEP);
        assert!(
            directory
                .join(format!("sysorbit-{}.log", writer.day))
                .exists()
        );
        drop(writer);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn failed_writer_silently_disables_itself() {
        let directory = temporary_directory();
        let mut writer = DailyWriter::new(directory.join("missing"));
        assert_eq!(writer.write(b"test\n").unwrap(), 5);
        assert!(writer.disabled);
        writer.write_all(b"still harmless\n").unwrap();
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn cleanup_tolerates_files_removed_by_another_instance() {
        let directory = temporary_directory();
        let gone = directory.join("sysorbit-2000-01-01.log");
        assert!(!oversized(&gone).unwrap());
        remove_managed(&gone).unwrap();
        fs::remove_dir_all(directory).unwrap();
    }

    type UnitRow = (
        String,
        String,
        String,
        String,
        String,
        String,
        zbus::OwnedObjectPath,
        u32,
        String,
        zbus::OwnedObjectPath,
    );

    struct Manager;

    #[zbus::interface(name = "org.freedesktop.systemd1.Manager")]
    impl Manager {
        #[zbus(property)]
        fn version(&self) -> String {
            format!("262 {SECRET}")
        }
        fn list_unit_files_by_patterns(
            &self,
            _states: Vec<String>,
            _patterns: Vec<String>,
        ) -> Vec<(String, String)> {
            vec![(
                "/home/alice/customer-invoice.timer".to_owned(),
                "enabled".to_owned(),
            )]
        }
        fn list_units_by_patterns(
            &self,
            _states: Vec<String>,
            _patterns: Vec<String>,
        ) -> Vec<UnitRow> {
            Vec::new()
        }
        fn load_unit(&self, _name: String) -> zbus::OwnedObjectPath {
            zbus::OwnedObjectPath::try_from("/org/freedesktop/systemd1/unit/secret").unwrap()
        }
        fn get_unit(&self, _name: String) -> zbus::fdo::Result<zbus::OwnedObjectPath> {
            Err(zbus::fdo::Error::UnknownMethod(SECRET.to_owned()))
        }
    }

    struct Unit;
    #[zbus::interface(name = "org.freedesktop.systemd1.Unit")]
    impl Unit {
        #[zbus(property)]
        fn id(&self) -> &str {
            "customer-invoice.timer"
        }
        #[zbus(property)]
        fn description(&self) -> &str {
            SECRET
        }
        #[zbus(property)]
        fn active_state(&self) -> &str {
            "active"
        }
        #[zbus(property)]
        fn unit_file_state(&self) -> &str {
            "enabled"
        }
    }

    struct Timer;
    #[zbus::interface(name = "org.freedesktop.systemd1.Timer")]
    impl Timer {
        #[zbus(property)]
        fn unit(&self) -> &str {
            "customer-invoice.service"
        }
        #[zbus(property)]
        fn timers_monotonic(&self) -> Vec<(String, u64, u64)> {
            Vec::new()
        }
        #[zbus(property)]
        fn timers_calendar(&self) -> Vec<(String, String, u64)> {
            vec![(SECRET.to_owned(), SECRET.to_owned(), 0)]
        }
        #[zbus(property, name = "NextElapseUSecRealtime")]
        fn next_realtime(&self) -> u64 {
            0
        }
        #[zbus(property, name = "NextElapseUSecMonotonic")]
        fn next_monotonic(&self) -> u64 {
            0
        }
        #[zbus(property, name = "LastTriggerUSec")]
        fn last_trigger(&self) -> u64 {
            0
        }
        #[zbus(property)]
        fn persistent(&self) -> bool {
            true
        }
        #[zbus(property)]
        fn wake_system(&self) -> bool {
            false
        }
        #[zbus(property, name = "AccuracyUSec")]
        fn accuracy(&self) -> u64 {
            1_000_000
        }
        #[zbus(property, name = "RandomizedDelayUSec")]
        fn randomized_delay(&self) -> u64 {
            0
        }
    }

    struct FakeHost {
        bus: zbus::Connection,
        failure: bool,
    }

    impl Host for FakeHost {
        fn name(&self) -> &str {
            SECRET
        }
        fn run<'a>(&'a self, invocation: &'a Invocation) -> BoxFuture<'a, io::Result<Output>> {
            Box::pin(async move {
                if self.failure {
                    return Err(io::Error::other(SECRET));
                }
                let calendar = invocation.program == "systemd-analyze";
                let stdout = if invocation.program == "date" {
                    SECRET.to_owned()
                } else {
                    format!(
                        "{{\"__REALTIME_TIMESTAMP\":\"1800000000000000\",\"UNIT\":\"customer-invoice.timer\",\"MESSAGE\":\"{SECRET}\"}}\n"
                    )
                };
                Ok(Output {
                    success: !calendar,
                    stdout,
                    stderr: SECRET.to_owned(),
                })
            })
        }
        fn bus(&self, _scope: Scope) -> BoxFuture<'_, zbus::Result<zbus::Connection>> {
            Box::pin(async move {
                if self.failure {
                    Err(zbus::Error::Failure(SECRET.to_owned()))
                } else {
                    Ok(self.bus.clone())
                }
            })
        }
    }

    #[tokio::test]
    async fn snapshot_logs_never_contain_private_host_or_unit_data_even_at_trace() {
        use zbus::connection::{Builder, socket::Channel};
        // With only one registered dispatcher, tracing caches the current thread's
        // interest. Another test thread has no scoped subscriber and can disable
        // shared callsites. Keep a second dispatcher alive to avoid that fast path.
        let _other_dispatch = tracing::Dispatch::new(tracing::subscriber::NoSubscriber::default());
        let capture = Capture::default();
        let writer = capture.clone();
        let subscriber = tracing_subscriber::registry()
            .with(layer(move || writer.clone(), EnvFilter::new("trace")));
        async {
            // Exercise callsite registration on a thread without the scoped subscriber.
            std::thread::spawn(|| {
                tracing::callsite::rebuild_interest_cache();
                let _span = span(Phase::BusConnect);
            })
            .join()
            .unwrap();
            let (client, server) = Channel::pair();
            let guid = zbus::Guid::generate();
            let server = Builder::authenticated_socket(server, guid.clone())
                .p2p()
                .serve_at("/org/freedesktop/systemd1", Manager)
                .serve_at("/org/freedesktop/systemd1/unit/secret", Unit)
                .serve_at("/org/freedesktop/systemd1/unit/secret", Timer)
                .build()
                .await
                .unwrap();
            let bus = Builder::authenticated_socket(client, guid)
                .p2p()
                .build()
                .await
                .unwrap();
            let mut host = FakeHost {
                bus,
                failure: false,
            };
            let now = jiff::Timestamp::now();
            let window = Interval {
                start: now - jiff::SignedDuration::from_hours(1),
                end: now,
            };
            let snapshot = crate::systemd::snapshot(&host, Scope::System, window)
                .await
                .unwrap();
            assert_eq!(snapshot.timers.len(), 1, "exercise the entire snapshot");
            crate::systemd::logs(&host, Scope::System, SECRET)
                .await
                .unwrap();
            // Deliberately hostile dependency records must not get through the boundary.
            tracing::warn!(target: "zbus", message = SECRET);
            tracing::trace!(target: "zbus", body = SECRET);
            host.failure = true;
            assert!(
                crate::systemd::snapshot(&host, Scope::System, window)
                    .await
                    .is_err()
            );
            assert!(crate::systemd::probe(&host).await.is_err());
            drop(server);
        }
        .with_subscriber(subscriber)
        .await;
        let output = String::from_utf8(capture.0.lock().unwrap().clone()).unwrap();
        for secret in
            SECRET
                .split_whitespace()
                .chain(["customer-invoice.service", "/home/", "MESSAGE"])
        {
            assert!(!output.contains(secret), "leaked {secret}: {output}");
        }
        for expected in [
            "Snapshot",
            "BusConnect",
            "Capabilities",
            "Clock",
            "Units",
            "Calendar",
            "JournalHistory",
            "Downtime",
            "LogLoading",
            "Spawn",
            "Command",
            "Parse",
            "Bus",
            "systemd=Some(262)",
        ] {
            assert!(output.contains(expected), "missing {expected}: {output}");
        }
    }

    /// Produces a real local sample and compares paired warm snapshots. Log to
    /// a temporary state directory when running this manually, never print data.
    #[cfg(target_os = "linux")]
    #[tokio::test]
    #[ignore = "needs access to a local systemd bus"]
    async fn local_logging_overhead() {
        use std::time::Instant;
        use tracing::subscriber::NoSubscriber;
        crate::i18n::init();
        let guard = init().expect("logging initialized");
        let host = crate::host::LocalHost::new();
        let now = jiff::Timestamp::now();
        let window = Interval {
            start: now - jiff::SignedDuration::from_hours(2),
            end: now + jiff::SignedDuration::from_hours(6),
        };
        crate::systemd::snapshot(&host, Scope::System, window)
            .await
            .unwrap();
        let mut disabled = Vec::new();
        let mut enabled = Vec::new();
        for pair in 0..20 {
            for logging in [pair % 2 == 0, pair % 2 != 0] {
                let started = Instant::now();
                let result = if logging {
                    crate::systemd::snapshot(&host, Scope::System, window).await
                } else {
                    crate::systemd::snapshot(&host, Scope::System, window)
                        .with_subscriber(NoSubscriber::default())
                        .await
                };
                assert!(result.is_ok());
                if logging {
                    enabled.push(started.elapsed().as_secs_f64());
                } else {
                    disabled.push(started.elapsed().as_secs_f64());
                }
            }
        }
        enabled.sort_by(f64::total_cmp);
        disabled.sort_by(f64::total_cmp);
        println!(
            "20 alternating pairs: disabled median {:.3} ms; enabled median {:.3} ms; difference {:.3} ms ({:.2}%)",
            disabled[10] * 1000.0,
            enabled[10] * 1000.0,
            (enabled[10] - disabled[10]) * 1000.0,
            (enabled[10] / disabled[10] - 1.0) * 100.0
        );
        event(Event::Exit);
        drop(guard);
    }
}
