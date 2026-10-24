//! Human readable text: schedules, dates, durations and lists, in the
//! current language.

use jiff::civil::Weekday;
use jiff::tz::TimeZone;
use jiff::{SignedDuration, Timestamp};

use crate::i18n::{self, fl};
use crate::model::{HostClock, MonotonicBase, Timer, Warning};

pub fn weekday(day: Weekday) -> String {
    match day {
        Weekday::Monday => fl!("weekday-short-mon"),
        Weekday::Tuesday => fl!("weekday-short-tue"),
        Weekday::Wednesday => fl!("weekday-short-wed"),
        Weekday::Thursday => fl!("weekday-short-thu"),
        Weekday::Friday => fl!("weekday-short-fri"),
        Weekday::Saturday => fl!("weekday-short-sat"),
        Weekday::Sunday => fl!("weekday-short-sun"),
    }
}

pub fn month(month: i8) -> String {
    match month {
        1 => fl!("month-short-1"),
        2 => fl!("month-short-2"),
        3 => fl!("month-short-3"),
        4 => fl!("month-short-4"),
        5 => fl!("month-short-5"),
        6 => fl!("month-short-6"),
        7 => fl!("month-short-7"),
        8 => fl!("month-short-8"),
        9 => fl!("month-short-9"),
        10 => fl!("month-short-10"),
        11 => fl!("month-short-11"),
        _ => fl!("month-short-12"),
    }
}

/// `Fri 23 Oct 14:05` in English.
pub fn moment(at: Timestamp, tz: &TimeZone) -> String {
    let zoned = at.to_zoned(tz.clone());
    fl!(
        "moment",
        weekday = weekday(zoned.weekday()),
        day = zoned.day(),
        month = month(zoned.month()),
        time = clock(at, tz)
    )
}

/// `14:05`
pub fn clock(at: Timestamp, tz: &TimeZone) -> String {
    let zoned = at.to_zoned(tz.clone());
    format!("{:02}:{:02}", zoned.hour(), zoned.minute())
}

/// `in 3d`, `12min ago`
pub fn relative(at: Timestamp, now: Timestamp) -> String {
    let delta = at.duration_since(now);
    if delta.abs() < SignedDuration::from_secs(60) {
        return fl!("relative-now");
    }
    let span = span(delta.abs());
    if delta.is_negative() {
        fl!("relative-past", span = span)
    } else {
        fl!("relative-future", span = span)
    }
}

/// A compact duration with its two largest units, `1h 40min` in English.
/// Settings in systemd's own notation use `timespan::format` instead.
pub fn span(span: SignedDuration) -> String {
    let parts = units(span, |unit, n| match unit {
        Unit::Week => fl!("short-week", n = n),
        Unit::Day => fl!("short-day", n = n),
        Unit::Hour => fl!("short-hour", n = n),
        Unit::Minute => fl!("short-minute", n = n),
        Unit::Second => fl!("short-second", n = n),
        Unit::Millisecond => fl!("short-millisecond", n = n),
    });
    if parts.is_empty() {
        fl!("short-zero")
    } else {
        parts.join(" ")
    }
}

/// A duration for sentences, `1 day 2 hours` in English.
pub fn span_words(span: SignedDuration) -> String {
    let parts = units(span, |unit, n| match unit {
        Unit::Week => fl!("span-week", n = n),
        Unit::Day => fl!("span-day", n = n),
        Unit::Hour => fl!("span-hour", n = n),
        Unit::Minute => fl!("span-minute", n = n),
        Unit::Second => fl!("span-second", n = n),
        Unit::Millisecond => fl!("span-millisecond", n = n),
    });
    if parts.is_empty() {
        fl!("span-zero")
    } else {
        parts.join(" ")
    }
}

#[derive(Clone, Copy)]
enum Unit {
    Week,
    Day,
    Hour,
    Minute,
    Second,
    Millisecond,
}

/// The two largest non-zero units of `span`, each named by `name`.
/// Below a second only milliseconds count.
fn units(span: SignedDuration, name: impl Fn(Unit, i64) -> String) -> Vec<String> {
    const UNITS: [(Unit, i64); 5] = [
        (Unit::Week, 604_800),
        (Unit::Day, 86_400),
        (Unit::Hour, 3600),
        (Unit::Minute, 60),
        (Unit::Second, 1),
    ];

    let mut seconds = span.as_secs().abs();
    if seconds == 0 {
        let millis = span.as_millis().abs() as i64;
        return if millis == 0 {
            Vec::new()
        } else {
            vec![name(Unit::Millisecond, millis)]
        };
    }
    UNITS
        .iter()
        .filter_map(|&(unit, size)| {
            let count = seconds / size;
            seconds %= size;
            (count > 0).then(|| name(unit, count))
        })
        .take(2)
        .collect()
}

/// `1st` in English, `1.` in Turkish.
pub fn ordinal(n: u32) -> String {
    fl!("ordinal", n = n, category = i18n::ordinal_category(n))
}

/// `a`, `a and b`, `a, b and c` in English.
pub fn join(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => fl!(
            "list-and",
            rest = rest.join(&fl!("list-separator")),
            last = last.clone()
        ),
    }
}

/// How the remote host's clock relates to this machine's, and its zone:
/// `server clock 3s ahead · server time (Europe/Istanbul)`. Offsets within
/// a second, or within the measuring error, count as in sync.
pub fn host_clock(clock: &HostClock) -> String {
    let tolerance = clock.uncertainty.max(SignedDuration::from_secs(1));
    let mut text = if clock.offset.abs() < tolerance {
        fl!("host-clock-synced")
    } else {
        let direction = if clock.offset.is_negative() {
            "behind"
        } else {
            "ahead"
        };
        fl!(
            "host-clock",
            direction = direction,
            span = span(clock.offset.abs())
        )
    };
    if let Some(zone) = &clock.zone {
        text.push_str(" · ");
        text.push_str(&fl!("host-zone", zone = zone.as_str()));
    }
    text
}

/// A snapshot warning for the status line.
pub fn warning(warning: &Warning) -> String {
    match warning {
        Warning::OldSystemd { version, minimum } => fl!(
            "warning-old-systemd",
            version = (*version),
            minimum = (*minimum)
        ),
        Warning::NextElapseOnly => fl!("warning-next-elapse-only"),
        Warning::NoPastElapses => fl!("warning-no-past-elapses"),
        Warning::MonotonicUnavailable(error) => fl!("warning-monotonic", error = error.as_str()),
        Warning::HistoryUnavailable(error) => fl!("warning-history", error = error.as_str()),
        Warning::JournalRestricted => fl!("warning-journal-restricted"),
        Warning::ClockUnavailable(error) => fl!("warning-clock", error = error.as_str()),
        Warning::DowntimeUnavailable(error) => fl!("warning-downtime", error = error.as_str()),
        Warning::Unreadable { unit, error } => fl!(
            "warning-unreadable",
            unit = unit.as_str(),
            error = error.as_str()
        ),
    }
}

/// How systemd reports a unit file's state, e.g. `enabled`, in words.
pub fn unit_file_state(state: &str) -> String {
    fl!("unit-file-state", state = state)
}

/// One line summary of when a timer fires, e.g. `Mon..Fri 03:00, +6h random`.
pub fn schedule(timer: &Timer) -> String {
    let mut parts: Vec<String> = timer
        .calendar
        .iter()
        .map(|trigger| calendar(&trigger.expression))
        .collect();

    parts.extend(timer.monotonic.iter().map(|trigger| {
        let offset = span(trigger.offset);
        match trigger.base {
            MonotonicBase::TimerActivated => fl!("schedule-activation", offset = offset),
            MonotonicBase::Boot => fl!("schedule-boot", offset = offset),
            MonotonicBase::Startup => fl!("schedule-startup", offset = offset),
            MonotonicBase::UnitActivated => fl!("schedule-every", offset = offset),
            MonotonicBase::UnitDeactivated => fl!("schedule-after-finish", offset = offset),
        }
    }));

    if timer.randomized_delay.is_positive() {
        parts.push(fl!("schedule-random", delay = span(timer.randomized_delay)));
    }
    if timer.persistent {
        parts.push(fl!("schedule-persistent"));
    }
    if timer.wake_system {
        parts.push(fl!("schedule-wake"));
    }

    parts.join(", ")
}

/// Shortens a normalized `OnCalendar=` expression where it is unambiguous.
/// Anything unusual is shown as systemd wrote it.
pub fn calendar(expression: &str) -> String {
    let fields: Vec<&str> = expression.split_whitespace().collect();
    let (days, date, time) = match fields.as_slice() {
        [date, time] if date.contains('-') => (None, *date, *time),
        [days, date, time] => (Some(*days), *date, *time),
        _ => return expression.to_owned(),
    };
    if date != "*-*-*" || time.contains(['/', ',', '.', '~']) {
        return expression.to_owned();
    }

    let time = time.strip_suffix(":00").unwrap_or(time);
    let repeat = match time {
        "*:00" => Some(fl!("schedule-hourly")),
        "*:*" => Some(fl!("schedule-every-minute")),
        _ => None,
    };

    match (days.map(weekday_tokens), repeat) {
        (None, Some(repeat)) => repeat,
        (None, None) => fl!("schedule-daily", time = time),
        (Some(days), Some(repeat)) => format!("{days} {repeat}"),
        (Some(days), None) => format!("{days} {time}"),
    }
}

/// systemd's weekday list, e.g. `Mon..Fri,Sun`, with localized names.
fn weekday_tokens(days: &str) -> String {
    const TOKENS: [(&str, Weekday); 7] = [
        ("Mon", Weekday::Monday),
        ("Tue", Weekday::Tuesday),
        ("Wed", Weekday::Wednesday),
        ("Thu", Weekday::Thursday),
        ("Fri", Weekday::Friday),
        ("Sat", Weekday::Saturday),
        ("Sun", Weekday::Sunday),
    ];
    let mut text = days.to_owned();
    for (token, day) in TOKENS {
        text = text.replace(token, &weekday(day));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortens_common_expressions() {
        let _language = i18n::test_language("en");
        assert_eq!(calendar("*-*-* 00:00:00"), "daily 00:00");
        assert_eq!(calendar("Mon..Fri *-*-* 03:00:00"), "Mon..Fri 03:00");
        assert_eq!(calendar("*-*-* *:00:00"), "hourly");
        assert_eq!(calendar("Sat *-*-* *:00:00"), "Sat hourly");
        assert_eq!(calendar("*-*-01 00:00:00"), "*-*-01 00:00:00");
        assert_eq!(calendar("*-*-* *:0/15:00"), "*-*-* *:0/15:00");
    }

    #[test]
    fn formats_in_the_current_language() {
        let at = Timestamp::from_second(1_792_800_000).unwrap(); // Sat 2026-10-24
        let tz = TimeZone::UTC;
        let span_100 = SignedDuration::from_mins(100);
        {
            let _language = i18n::test_language("en");
            assert_eq!(moment(at, &tz), "Sat 24 Oct 00:00");
            assert_eq!(span(span_100), "1h 40min");
            assert_eq!(span_words(SignedDuration::from_hours(25)), "1 day 1 hour");
            assert_eq!(ordinal(22), "22nd");
            assert_eq!(join(&["a".into(), "b".into(), "c".into()]), "a, b and c");
        }
        let _language = i18n::test_language("tr");
        assert_eq!(moment(at, &tz), "24 Eki Cmt 00:00");
        assert_eq!(span(span_100), "1sa 40dk");
        assert_eq!(span_words(SignedDuration::from_hours(25)), "1 gün 1 saat");
        assert_eq!(ordinal(22), "22.");
        assert_eq!(calendar("Mon..Fri *-*-* 03:00:00"), "Pzt..Cum 03:00");
        assert_eq!(relative(at, at + span_100), "1sa 40dk önce");
    }
}
