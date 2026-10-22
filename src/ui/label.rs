//! Human readable text: schedules, dates and durations.

use jiff::civil::Weekday;
use jiff::tz::TimeZone;
use jiff::{SignedDuration, Timestamp};

use crate::model::{MonotonicBase, Timer};
use crate::systemd::timespan;

pub fn weekday(day: Weekday) -> &'static str {
    match day {
        Weekday::Monday => "Mon",
        Weekday::Tuesday => "Tue",
        Weekday::Wednesday => "Wed",
        Weekday::Thursday => "Thu",
        Weekday::Friday => "Fri",
        Weekday::Saturday => "Sat",
        Weekday::Sunday => "Sun",
    }
}

pub fn month(month: i8) -> &'static str {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    MONTHS[(month.clamp(1, 12) - 1) as usize]
}

/// `Fri 23 Oct 14:05`
pub fn moment(at: Timestamp, tz: &TimeZone) -> String {
    let zoned = at.to_zoned(tz.clone());
    format!(
        "{} {} {} {:02}:{:02}",
        weekday(zoned.weekday()),
        zoned.day(),
        month(zoned.month()),
        zoned.hour(),
        zoned.minute()
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
        return "now".to_owned();
    }
    let span = timespan::format(delta.abs());
    if delta.is_negative() {
        format!("{span} ago")
    } else {
        format!("in {span}")
    }
}

/// One line summary of when a timer fires, e.g. `Mon..Fri 03:00, +6h random`.
pub fn schedule(timer: &Timer) -> String {
    let mut parts: Vec<String> = timer
        .calendar
        .iter()
        .map(|trigger| calendar(&trigger.expression))
        .collect();

    parts.extend(timer.monotonic.iter().map(|trigger| {
        let offset = timespan::format(trigger.offset);
        match trigger.base {
            MonotonicBase::TimerActivated => format!("activation +{offset}"),
            MonotonicBase::Boot => format!("boot +{offset}"),
            MonotonicBase::Startup => format!("startup +{offset}"),
            MonotonicBase::UnitActivated => format!("every {offset}"),
            MonotonicBase::UnitDeactivated => format!("{offset} after finish"),
        }
    }));

    if timer.randomized_delay.is_positive() {
        parts.push(format!(
            "+{} random",
            timespan::format(timer.randomized_delay)
        ));
    }
    if timer.persistent {
        parts.push("Persistent".to_owned());
    }
    if timer.wake_system {
        parts.push("Wake".to_owned());
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
    let (repeat, time) = match time {
        "*:00" => (true, "hourly"),
        "*:*" => (true, "every minute"),
        time => (false, time),
    };

    match (days, repeat) {
        (None, true) => time.to_owned(),
        (None, false) => format!("daily {time}"),
        (Some(days), _) => format!("{days} {time}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortens_common_expressions() {
        assert_eq!(calendar("*-*-* 00:00:00"), "daily 00:00");
        assert_eq!(calendar("Mon..Fri *-*-* 03:00:00"), "Mon..Fri 03:00");
        assert_eq!(calendar("*-*-* *:00:00"), "hourly");
        assert_eq!(calendar("Sat *-*-* *:00:00"), "Sat hourly");
        assert_eq!(calendar("*-*-01 00:00:00"), "*-*-01 00:00:00");
        assert_eq!(calendar("*-*-* *:0/15:00"), "*-*-* *:0/15:00");
    }
}
