//! Plain English for timer triggers, e.g. `Mon..Fri *-*-* 03:00:00` becomes
//! "On weekdays at 03:00".
//!
//! Works on the normalized `OnCalendar=` form systemd reports over D-Bus:
//! `[weekdays] year-month-day hour:minute:second [timezone]`, where each
//! component is `*` or a comma separated list of `start[..stop][/repeat]`,
//! and `~` before the day counts days from the end of the month. Common
//! shapes get a sentence; anything else is still described field by field.

use jiff::SignedDuration;

use crate::model::{MonotonicBase, MonotonicTrigger};

/// Describes a normalized `OnCalendar=` expression. `None` only for input
/// that is not in systemd's normalized form.
pub fn calendar(expression: &str) -> Option<String> {
    let spec = Spec::parse(expression)?;
    let mut sentence = spec.sentence();
    if let Some(zone) = &spec.zone {
        sentence.push_str(&format!(" ({zone} time)"));
    }
    Some(capitalize(&sentence))
}

/// Describes a monotonic trigger such as `OnBootSec=15min`.
pub fn monotonic(trigger: &MonotonicTrigger) -> String {
    let offset = span_words(trigger.offset);
    match trigger.base {
        MonotonicBase::TimerActivated => format!("{offset} after the timer was started"),
        MonotonicBase::Boot => format!("{offset} after boot"),
        MonotonicBase::Startup => {
            format!("{offset} after the service manager started (for user timers: login)")
        }
        MonotonicBase::UnitActivated => {
            format!("Every {offset}, counted from the last time the service started")
        }
        MonotonicBase::UnitDeactivated => {
            format!("{offset} after the service last finished, again after every run")
        }
    }
}

/// `1 day 2 hours`, keeping the two largest units.
pub fn span_words(span: SignedDuration) -> String {
    const UNITS: [(&str, i64); 6] = [
        ("week", 604_800_000),
        ("day", 86_400_000),
        ("hour", 3_600_000),
        ("minute", 60_000),
        ("second", 1000),
        ("millisecond", 1),
    ];

    let mut millis = span.as_millis().abs() as i64;
    if millis == 0 {
        return "0 seconds".to_owned();
    }
    let parts: Vec<String> = UNITS
        .iter()
        .filter_map(|&(name, size)| {
            let count = millis / size;
            millis %= size;
            (count > 0).then(|| plural(count, name))
        })
        .take(2)
        .collect();
    parts.join(" ")
}

/// One `start[..stop][/repeat]` element of a component.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Item {
    start: u32,
    stop: Option<u32>,
    repeat: Option<u32>,
}

/// A component; `None` is `*`.
type Field = Option<Vec<Item>>;

#[derive(Debug)]
struct Spec {
    /// Monday is 0.
    weekdays: Option<Vec<u32>>,
    year: Field,
    month: Field,
    day: Field,
    /// `~`: days count back from the end of the month, `~01` being the last.
    from_end: bool,
    hour: Field,
    minute: Field,
    second: Field,
    zone: Option<String>,
}

impl Spec {
    fn parse(expression: &str) -> Option<Self> {
        let mut words = expression.split_whitespace().peekable();
        let weekdays = match words.peek() {
            Some(word) if word.starts_with(|c: char| c.is_ascii_alphabetic()) => {
                Some(parse_weekdays(words.next()?)?)
            }
            _ => None,
        };

        let date = words.next()?;
        let (year, rest) = date.split_once('-')?;
        let (month, day, from_end) = match rest.split_once('~') {
            Some((month, day)) => (month, day, true),
            None => {
                let (month, day) = rest.split_once('-')?;
                (month, day, false)
            }
        };

        let time = words.next()?;
        let mut clock = time.split(':');
        let (hour, minute, second) = (clock.next()?, clock.next()?, clock.next()?);
        if clock.next().is_some() {
            return None;
        }

        let zone = words.next().map(str::to_owned);
        if words.next().is_some() {
            return None;
        }

        Some(Self {
            weekdays,
            year: parse_field(year)?,
            month: parse_field(month)?,
            day: parse_field(day)?,
            from_end,
            hour: parse_field(hour)?,
            minute: parse_field(minute)?,
            second: parse_field(second)?,
            zone,
        })
    }

    fn sentence(&self) -> String {
        let time = self.time();
        let restrictions = self.date();

        match (restrictions.is_empty(), time) {
            (true, Time::At(times)) => format!("every day at {}", join(&times)),
            (true, Time::Repeat(phrase)) => phrase,
            (false, Time::At(times)) => format!("{} at {}", restrictions.join(" "), join(&times)),
            (false, Time::Repeat(phrase)) => format!("{phrase}, {}", restrictions.join(" ")),
        }
    }

    fn date(&self) -> Vec<String> {
        let weekdays = self
            .weekdays
            .as_ref()
            .map(|days| Shape::of(days.clone(), 0, 6))
            .filter(|shape| *shape != Shape::Every);
        let month = shape(&self.month, 1, 12);
        let day = match (&self.day, self.from_end) {
            // `~07/1` repeats toward the end of the month: 7th-to-last, 6th-to-last…
            (Some(items), true) => Shape::of(expand_from_end(items), 1, 31),
            (day, _) => shape(day, 1, 31),
        };
        let mut parts = Vec::new();

        // A weekday inside a seven day stretch is "the nth weekday".
        let nth = match (&weekdays, &day, self.from_end) {
            (Some(Shape::One(weekday)), Shape::Range(1, 7), true) => {
                Some(format!("on the last {}", weekday_singular(*weekday)))
            }
            (Some(Shape::One(weekday)), Shape::Range(start, end), false)
                if end - start == 6 && start % 7 == 1 && *start <= 22 =>
            {
                let which = ["first", "second", "third", "fourth"][(*start / 7) as usize];
                Some(format!("on the {which} {}", weekday_singular(*weekday)))
            }
            _ => None,
        };

        match nth {
            Some(phrase) => {
                parts.push(phrase);
                parts.push(match &month {
                    Shape::Every => "of every month".to_owned(),
                    month => format!("of {}", month_words(month)),
                });
            }
            None => {
                if let Some(weekdays) = &weekdays {
                    parts.push(weekday_words(weekdays));
                }
                match (&day, &month) {
                    (Shape::Every, Shape::Every) => {}
                    (Shape::Every, Shape::Range(a, b)) => {
                        parts.push(format!("from {} to {}", month_name(*a), month_name(*b)))
                    }
                    (Shape::Every, month) => parts.push(format!("in {}", month_words(month))),
                    (Shape::One(day), Shape::One(month)) if !self.from_end => {
                        parts.push(format!("on {day} {}", month_name(*month)));
                    }
                    (day, Shape::Every) => {
                        parts.push(format!("{} of every month", self.day_words(day)));
                    }
                    (day, month) => {
                        parts.push(format!("{} of {}", self.day_words(day), month_words(month)));
                    }
                }
            }
        }

        if let Some(years) = &self.year {
            parts.push(year_words(years));
        }
        parts
    }

    fn day_words(&self, day: &Shape) -> String {
        if self.from_end {
            return match day {
                Shape::One(1) => "on the last day".to_owned(),
                Shape::One(n) => format!("on the {}-to-last day", ordinal(*n)),
                Shape::Range(1, n) => format!("on the last {n} days"),
                other => format!("on the {} days counted from the end", other.values_words()),
            };
        }
        match day {
            Shape::Every => "every day".to_owned(),
            Shape::One(n) => format!("on the {}", ordinal(*n)),
            Shape::Range(a, b) => format!("on the {} to the {}", ordinal(*a), ordinal(*b)),
            Shape::Step {
                start: 1, step: 2, ..
            } => "on odd days".to_owned(),
            Shape::Step {
                start: 2, step: 2, ..
            } => "on even days".to_owned(),
            Shape::Step { start, step, .. } => {
                format!("every {step} days from the {}", ordinal(*start))
            }
            Shape::List(values) => {
                let days: Vec<String> = values.iter().map(|&n| ordinal(n)).collect();
                format!("on the {}", join(&days))
            }
        }
    }

    fn time(&self) -> Time {
        let hour = shape(&self.hour, 0, 23);
        let minute = shape(&self.minute, 0, 59);
        let second = shape(&self.second, 0, 59);

        if second != Shape::One(0) {
            return match (&hour, &minute, &second) {
                (Shape::Every, Shape::Every, Shape::Every) => Time::Repeat("every second".into()),
                (Shape::Every, Shape::Every, second) if second.clean_step(60).is_some() => {
                    let step = second.clean_step(60).unwrap_or(1);
                    Time::Repeat(format!("every {step} seconds"))
                }
                (Shape::Every, Shape::Every, Shape::One(s)) => {
                    Time::Repeat(format!("every minute at second {s}"))
                }
                (hour, minute, Shape::One(s)) if hour.len().saturating_mul(minute.len()) <= 4 => {
                    Time::At(times(hour, minute, Some(*s)))
                }
                _ => Time::Repeat(format!(
                    "when the hour is {}, the minute {} and the second {}",
                    hour.values_words(),
                    minute.values_words(),
                    second.values_words()
                )),
            };
        }

        match (&hour, &minute) {
            (Shape::Every, minute) => Time::Repeat(minute_words(minute)),
            (hour, minute) if hour.len().saturating_mul(minute.len()) <= 4 => {
                Time::At(times(hour, minute, None))
            }
            (hour, Shape::One(m)) => match hour {
                Shape::Range(a, b) => Time::Repeat(format!(
                    "every hour from {} to {}",
                    hhmm(*a, *m),
                    hhmm(*b, *m)
                )),
                Shape::Step { start, step, last } => {
                    let at = if *m == 0 {
                        String::new()
                    } else {
                        format!(" at :{m:02}")
                    };
                    if *start < *step && 24_u32.is_multiple_of(*step) {
                        Time::Repeat(format!("every {step} hours{at}"))
                    } else {
                        Time::Repeat(format!(
                            "every {step} hours from {} to {}",
                            hhmm(*start, *m),
                            hhmm(*last, *m)
                        ))
                    }
                }
                hour if hour.len() <= 8 => Time::At(times(hour, &Shape::One(*m), None)),
                hour => Time::Repeat(format!("at :{m:02} past hours {}", hour.values_words())),
            },
            (hour, minute) => {
                Time::Repeat(format!("{} {}", minute_words(minute), hour_window(hour)))
            }
        }
    }
}

enum Time {
    /// Fixed times of day, `03:00`.
    At(Vec<String>),
    /// Repeating within the day, "every 15 minutes".
    Repeat(String),
}

/// The values a component takes, recognized as a pattern.
#[derive(Debug, Clone, PartialEq)]
enum Shape {
    Every,
    One(u32),
    /// Consecutive, at least three values.
    Range(u32, u32),
    /// Evenly spaced, at least three values.
    Step {
        start: u32,
        step: u32,
        last: u32,
    },
    List(Vec<u32>),
}

impl Shape {
    fn of(mut values: Vec<u32>, low: u32, high: u32) -> Self {
        values.sort_unstable();
        values.dedup();
        values.retain(|&value| (low..=high).contains(&value));
        if values.len() == (high - low + 1) as usize {
            return Self::Every;
        }
        match values.as_slice() {
            [] => Self::List(values),
            [one] => Self::One(*one),
            [first, second, ..] if values.len() >= 3 => {
                let step = second - first;
                let even = values.windows(2).all(|pair| pair[1] - pair[0] == step);
                let last = *values.last().unwrap_or(first);
                match (even, step) {
                    (true, 1) => Self::Range(*first, last),
                    (true, _) => Self::Step {
                        start: *first,
                        step,
                        last,
                    },
                    _ => Self::List(values),
                }
            }
            _ => Self::List(values),
        }
    }

    fn values(&self) -> Vec<u32> {
        match self {
            Self::Every => Vec::new(),
            Self::One(value) => vec![*value],
            Self::Range(a, b) => (*a..=*b).collect(),
            Self::Step { start, step, last } => (*start..=*last).step_by(*step as usize).collect(),
            Self::List(values) => values.clone(),
        }
    }

    /// Number of values; `Every` counts as many.
    fn len(&self) -> usize {
        match self {
            Self::Every => usize::MAX / 1024,
            other => other.values().len(),
        }
    }

    /// The step when the values repeat evenly around the whole cycle, as in
    /// `00/15` within an hour.
    fn clean_step(&self, cycle: u32) -> Option<u32> {
        match self {
            Self::Step { start, step, last } if start < step && cycle.is_multiple_of(*step) => {
                (last + step >= cycle).then_some(*step)
            }
            _ => None,
        }
    }

    /// `9`, `9–17`, `8, 10 and 12`.
    fn values_words(&self) -> String {
        match self {
            Self::Every => "any".to_owned(),
            Self::Range(a, b) => format!("{a}–{b}"),
            Self::Step { start, step, last } if self.len() > 6 => {
                format!("every {step} from {start} to {last}")
            }
            other => {
                let values: Vec<String> = other.values().iter().map(u32::to_string).collect();
                join(&values)
            }
        }
    }
}

fn shape(field: &Field, low: u32, high: u32) -> Shape {
    match field {
        None => Shape::Every,
        Some(items) => Shape::of(expand(items, high), low, high),
    }
}

fn expand(items: &[Item], high: u32) -> Vec<u32> {
    let mut values = Vec::new();
    for item in items {
        let stop = item.stop.unwrap_or(if item.repeat.is_some() {
            high
        } else {
            item.start
        });
        let step = item.repeat.unwrap_or(1).max(1) as usize;
        values.extend((item.start..=stop.min(high)).step_by(step));
    }
    values
}

/// Day numbers counted from the end of the month, where a repeat without a
/// stop runs down to the last day.
fn expand_from_end(items: &[Item]) -> Vec<u32> {
    let mut values = Vec::new();
    for item in items {
        match (item.stop, item.repeat) {
            (None, Some(step)) => {
                values.extend((1..=item.start).rev().step_by(step.max(1) as usize));
            }
            _ => values.extend(expand(std::slice::from_ref(item), 31)),
        }
    }
    values
}

/// `*`, or items like `01,04`, `08..18`, `00/15`. Seconds may carry a
/// fraction, which is accepted only when it is zero.
fn parse_field(text: &str) -> Option<Field> {
    if text == "*" {
        return Some(None);
    }
    let number = |text: &str| -> Option<u32> {
        let (whole, fraction) = text.split_once('.').unwrap_or((text, "0"));
        fraction.bytes().all(|b| b == b'0').then_some(())?;
        whole.parse().ok()
    };
    text.split(',')
        .map(|item| {
            let (range, repeat) = match item.split_once('/') {
                Some((range, repeat)) => (range, Some(number(repeat)?)),
                None => (item, None),
            };
            let (start, stop) = match range.split_once("..") {
                Some((start, stop)) => (number(start)?, Some(number(stop)?)),
                None => (number(range)?, None),
            };
            Some(Item {
                start,
                stop,
                repeat,
            })
        })
        .collect::<Option<Vec<_>>>()
        .map(Some)
}

/// `Mon..Fri`, `Sat,Sun`, `Mon,Wed..Fri`.
fn parse_weekdays(text: &str) -> Option<Vec<u32>> {
    const NAMES: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    let index = |name: &str| NAMES.iter().position(|&n| n == name).map(|i| i as u32);
    let mut days = Vec::new();
    for item in text.split(',') {
        match item.split_once("..") {
            Some((start, stop)) => days.extend(index(start)?..=index(stop)?),
            None => days.push(index(item)?),
        }
    }
    Some(days)
}

fn minute_words(minute: &Shape) -> String {
    match minute {
        Shape::Every => "every minute".to_owned(),
        Shape::One(0) => "every hour".to_owned(),
        Shape::One(m) => format!("every hour at :{m:02}"),
        Shape::Range(a, b) => format!("every minute from :{a:02} to :{b:02} past the hour"),
        Shape::Step { start, step, last } => match minute.clean_step(60) {
            Some(step) if *start == 0 => format!("every {step} minutes"),
            Some(step) => format!("every {step} minutes from :{start:02}"),
            None if minute.len() <= 6 => past_the_hour(&minute.values()),
            None => format!("every {step} minutes from :{start:02} to :{last:02} past the hour"),
        },
        Shape::List(values) => past_the_hour(values),
    }
}

fn past_the_hour(minutes: &[u32]) -> String {
    let minutes: Vec<String> = minutes.iter().map(|m| format!(":{m:02}")).collect();
    format!("at {} past the hour", join(&minutes))
}

fn hour_window(hour: &Shape) -> String {
    match hour {
        Shape::Every => String::new(),
        Shape::One(h) => format!("between {} and {}", hhmm(*h, 0), hhmm(*h, 59)),
        Shape::Range(a, b) => format!("between {} and {}", hhmm(*a, 0), hhmm(*b, 59)),
        other => format!("during hours {}", other.values_words()),
    }
}

fn times(hour: &Shape, minute: &Shape, second: Option<u32>) -> Vec<String> {
    let mut times = Vec::new();
    for h in hour.values() {
        for m in minute.values() {
            times.push(match second {
                Some(s) => format!("{h:02}:{m:02}:{s:02}"),
                None => hhmm(h, m),
            });
        }
    }
    times.sort();
    times
}

fn hhmm(hour: u32, minute: u32) -> String {
    format!("{hour:02}:{minute:02}")
}

fn weekday_words(weekdays: &Shape) -> String {
    match weekdays {
        Shape::Range(0, 4) => "on weekdays".to_owned(),
        Shape::List(days) if days == &[5, 6] => "on weekends".to_owned(),
        Shape::Range(a, b) => format!("from {} to {}", weekday_singular(*a), weekday_singular(*b)),
        other => {
            let names: Vec<String> = other
                .values()
                .into_iter()
                .map(|day| weekday_name(day).to_owned())
                .collect();
            format!("on {}", join(&names))
        }
    }
}

fn weekday_name(day: u32) -> &'static str {
    const NAMES: [&str; 7] = [
        "Mondays",
        "Tuesdays",
        "Wednesdays",
        "Thursdays",
        "Fridays",
        "Saturdays",
        "Sundays",
    ];
    NAMES[day.min(6) as usize]
}

fn weekday_singular(day: u32) -> &'static str {
    weekday_name(day).trim_end_matches('s')
}

fn month_words(month: &Shape) -> String {
    match month {
        Shape::Range(a, b) => format!("{} to {}", month_name(*a), month_name(*b)),
        other => {
            let names: Vec<String> = other
                .values()
                .into_iter()
                .map(|month| month_name(month).to_owned())
                .collect();
            join(&names)
        }
    }
}

fn month_name(month: u32) -> &'static str {
    const NAMES: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    NAMES[(month.clamp(1, 12) - 1) as usize]
}

fn year_words(items: &[Item]) -> String {
    match items {
        [
            Item {
                start,
                stop: None,
                repeat: None,
            },
        ] => format!("in {start}"),
        [
            Item {
                start,
                stop: Some(stop),
                repeat: None,
            },
        ] => format!("from {start} to {stop}"),
        [
            Item {
                start,
                stop: None,
                repeat: Some(step),
            },
        ] => format!("every {step} years from {start}"),
        items => {
            let years: Vec<String> = expand(items, 9999).iter().map(u32::to_string).collect();
            format!("in {}", join(&years))
        }
    }
}

fn ordinal(n: u32) -> String {
    let suffix = match (n % 10, n % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

fn plural(count: i64, unit: &str) -> String {
    if count == 1 {
        format!("1 {unit}")
    } else {
        format!("{count} {unit}s")
    }
}

/// `a`, `a and b`, `a, b and c`.
fn join(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn says(expression: &str) -> String {
        calendar(expression).unwrap_or_else(|| panic!("{expression} did not parse"))
    }

    #[test]
    fn describes_systemd_shorthands() {
        // minutely, hourly, daily, weekly, monthly, quarterly, semiannually, yearly
        assert_eq!(says("*-*-* *:*:00"), "Every minute");
        assert_eq!(says("*-*-* *:00:00"), "Every hour");
        assert_eq!(says("*-*-* 00:00:00"), "Every day at 00:00");
        assert_eq!(says("Mon *-*-* 00:00:00"), "On Mondays at 00:00");
        assert_eq!(
            says("*-*-01 00:00:00"),
            "On the 1st of every month at 00:00"
        );
        assert_eq!(
            says("*-01,04,07,10-01 00:00:00"),
            "On the 1st of January, April, July and October at 00:00"
        );
        assert_eq!(
            says("*-01,07-01 00:00:00"),
            "On the 1st of January and July at 00:00"
        );
        assert_eq!(says("*-01-01 00:00:00"), "On 1 January at 00:00");
    }

    #[test]
    fn describes_repeating_times() {
        assert_eq!(says("*-*-* *:00/15:00"), "Every 15 minutes");
        assert_eq!(says("*-*-* *:05/15:00"), "Every 15 minutes from :05");
        assert_eq!(says("*-*-* *:30:00"), "Every hour at :30");
        assert_eq!(
            says("*-*-* *:00,20,45:00"),
            "At :00, :20 and :45 past the hour"
        );
        assert_eq!(says("*-*-* 00/2:00:00"), "Every 2 hours");
        assert_eq!(
            says("*-*-* 08/2:30:00"),
            "Every 2 hours from 08:30 to 22:30"
        );
        assert_eq!(says("*-*-* 09..17:00:00"), "Every hour from 09:00 to 17:00");
        assert_eq!(
            says("Mon..Fri *-*-* 09..17:00/10:00"),
            "Every 10 minutes between 09:00 and 17:59, on weekdays"
        );
        assert_eq!(says("*-*-* *:*:*"), "Every second");
        assert_eq!(says("*-*-* *:*:00/10"), "Every 10 seconds");
    }

    #[test]
    fn describes_dates() {
        assert_eq!(says("Mon..Fri *-*-* 03:00:00"), "On weekdays at 03:00");
        assert_eq!(says("Sat,Sun *-*-* 10:00:00"), "On weekends at 10:00");
        assert_eq!(
            says("Mon,Wed,Fri *-*-* 06,18:00:00"),
            "On Mondays, Wednesdays and Fridays at 06:00 and 18:00"
        );
        assert_eq!(
            says("Tue..Thu *-*-* 12:00:00"),
            "From Tuesday to Thursday at 12:00"
        );
        assert_eq!(
            says("Mon *-*-01..07 04:00:00"),
            "On the first Monday of every month at 04:00"
        );
        assert_eq!(
            says("Fri *-*~01..07 17:00:00"),
            "On the last Friday of every month at 17:00"
        );
        assert_eq!(
            says("Fri *-*~07/1 17:00:00"),
            "On the last Friday of every month at 17:00"
        );
        assert_eq!(
            says("*-*~01 23:30:00"),
            "On the last day of every month at 23:30"
        );
        assert_eq!(
            says("*-02~03 00:00:00"),
            "On the 3rd-to-last day of February at 00:00"
        );
        assert_eq!(
            says("*-*-01/2 00:00:00"),
            "On odd days of every month at 00:00"
        );
        assert_eq!(
            says("*-*-01,15 12:00:00"),
            "On the 1st and 15th of every month at 12:00"
        );
        assert_eq!(
            says("*-03..10-* 08:00:00"),
            "From March to October at 08:00"
        );
        assert_eq!(
            says("2027-12-25 18:00:00"),
            "On 25 December in 2027 at 18:00"
        );
        assert_eq!(
            says("*-*-* 02:00:00 Europe/Istanbul"),
            "Every day at 02:00 (Europe/Istanbul time)"
        );
    }

    #[test]
    fn falls_back_to_fields_for_unusual_times() {
        assert_eq!(
            says("*-*-* 01,05,09:10..20:30"),
            "When the hour is 1, 5 and 9, the minute 10–20 and the second 30"
        );
        assert_eq!(calendar("daily"), None);
    }

    #[test]
    fn describes_monotonic_triggers() {
        let trigger = |base, mins| MonotonicTrigger {
            base,
            offset: SignedDuration::from_mins(mins),
        };
        assert_eq!(
            monotonic(&trigger(MonotonicBase::Boot, 15)),
            "15 minutes after boot"
        );
        assert_eq!(
            monotonic(&trigger(MonotonicBase::UnitActivated, 60 * 24 + 90)),
            "Every 1 day 1 hour, counted from the last time the service started"
        );
        assert_eq!(
            span_words(SignedDuration::from_millis(1500)),
            "1 second 500 milliseconds"
        );
    }
}
