# Local diagnostics

Sysorbit writes diagnostic files only on the local machine running the app,
including when monitoring remote hosts over SSH. Logging adds no network
requests, telemetry, crash reporting or uploads. For a bug report, attach the
log files from the directory below; each run records the app version, OS version,
architecture and the systemd version of every host it reaches.

Files live in:

| Platform | Directory |
| --- | --- |
| Linux | `$XDG_STATE_HOME/sysorbit/logs`, default `~/.local/state/sysorbit/logs` |
| macOS | `~/Library/Logs/sysorbit` |
| Windows | `%LOCALAPPDATA%\sysorbit\logs` |

Files are named `sysorbit-YYYY-MM-DD.log` using the UTC date. At most seven files
are kept, each at most 1,400,000 bytes (9.8 MB altogether). Cleanup happens when
the writer starts and when the UTC date changes. Once today's file is full,
additional records are discarded until the next UTC day. Oversized existing
managed files are removed during cleanup. Unrelated files are left alone.
On Unix, newly created directories have mode 0700 and files have mode 0600.

The writer uses tracing-appender's bounded, lossy queue (512 records). Disk I/O,
rotation and size checks run on its worker thread. A full queue drops records;
a filesystem write failure disables the writer until the app restarts. Failure
to initialize logging leaves the app running without it.

## Privacy policy

Only the typed API in `src/diagnostics.rs` emits records. Allowed fields are:

- App version, OS and numeric OS version, architecture, parsed locale and build
  profile. Linux reports the numeric kernel release; distro/custom suffixes are
  omitted. macOS reports the product version and Windows the OS version.
- Systemd's numeric major version, capability flags and named fallbacks.
- Lifecycle and UI actions, scope and zoom, connection states and refresh reasons.
- Fixed error codes and counts, parser kinds, command success and output byte
  counts, timer/run/mark counts and journal parsing counts.
- Phase durations and panic source location (relative application file, or
  `dependency`, plus line) and numeric thread identity. Panic payloads and thread
  names are never formatted.

Host and unit identities must pass through `Redacted`. It holds only the first
four bytes of HMAC-SHA256, formatted as `host#be8ec092` or `unit#12345678`. A
random 32-byte key is stored in `salt` next to the files and reused across days
and sessions. Host and unit hashes use separate domains. **Do not share the
salt.** It is not a log file and never needs attaching. Deleting it changes future pseudonyms;
short pseudonyms can collide.

Hostnames, IPs, ports, destinations, aliases, usernames, SSH options and key
paths, unit names/descriptions, calendar expressions, invocation IDs, journal
messages, command output, D-Bus contents and raw error strings are never emitted.
Errors displayed in the app retain their existing detail; the log receives only
codes. Attach `.log` files to a report without editing
out personal data.

## Verbosity and format

The default filter is `warn,sysorbit=info`. Set `SYSORBIT_LOG=debug` for output
sizes, or use an EnvFilter expression such as
`SYSORBIT_LOG=warn,sysorbit=trace`. `SYSORBIT_LOG=off` disables records; an invalid
filter uses the default. Field filter regular expressions are disabled.

A second, unconditional filter admits only the audited diagnostics target.
Dependency records, including warnings, are suppressed at every verbosity:
they can include private D-Bus bodies or errors. The privacy filter applies
regardless of the configured log level. Zbus's tracing feature is not enabled.

Output is plain text, one record per line, with UTC timestamps, levels, nested
phase context and key/value fields. Closing spans include `time.busy` (time
entered) and `time.idle` (time waiting); their sum is the phase's elapsed duration.
Concurrent phases overlap, so their durations should not be added together.

The privacy tests use an in-memory fake host and D-Bus server with recognizable
secrets to check that private data is absent even at trace verbosity. Run them
with `cargo test diagnostics::tests`.

On Linux, measure snapshot logging overhead against the local systemd bus with:

```sh
XDG_STATE_HOME=/tmp/sysorbit-measure cargo test local_logging_overhead -- --ignored --nocapture
```

The measurement alternates enabled and disabled logging over 20 snapshot pairs
after warming the connection. Results include subprocess and scheduler variability.
