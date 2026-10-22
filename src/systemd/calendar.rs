//! Expands `OnCalendar=` expressions into elapse times with
//! `systemd-analyze calendar`, so results match systemd exactly.

use std::collections::HashMap;

use jiff::Timestamp;
use jiff::civil::DateTime;
use jiff::tz::TimeZone;

use super::{Error, run};
use crate::host::{Host, Invocation};
use crate::model::Interval;

/// Upper bound per expression; keeps `*:*:0/1` style expressions sane.
const MAX_ITERATIONS: usize = 5000;

/// Returns the elapse times of each expression inside `window`.
/// Expressions systemd rejects map to an empty list.
pub async fn occurrences(
    host: &dyn Host,
    expressions: &[String],
    window: Interval,
) -> HashMap<String, Vec<Timestamp>> {
    let mut result = HashMap::new();
    if expressions.is_empty() {
        return result;
    }

    match analyze(host, expressions, window).await {
        Ok(parsed) => result.extend(parsed),
        // One bad expression fails the whole batch; retry one by one.
        Err(_) => {
            for expression in expressions {
                if let Ok(parsed) = analyze(host, std::slice::from_ref(expression), window).await {
                    result.extend(parsed);
                }
            }
        }
    }

    for times in result.values_mut() {
        times.retain(|&at| at < window.end);
    }

    result
}

async fn analyze(
    host: &dyn Host,
    expressions: &[String],
    window: Interval,
) -> Result<HashMap<String, Vec<Timestamp>>, Error> {
    let invocation = Invocation::new("systemd-analyze")
        .arg("calendar")
        .arg(format!("--iterations={MAX_ITERATIONS}"))
        .arg(format!("--base-time=@{}", window.start.as_second()))
        .args(expressions.iter().cloned());

    Ok(parse(&run(host, invocation).await?, expressions))
}

/// Parses blocks like the one below, one per expression in input order.
/// `Original form` is left out when the input is already normalized, so
/// blocks are matched to `expressions` by position.
///
/// ```text
///   Original form: daily
/// Normalized form: *-*-* 00:00:00
///     Next elapse: Sat 2026-10-24 00:00:00 +03
///        (in UTC): Fri 2026-10-23 21:00:00 UTC
///        From now: 23h left
///    Iteration #2: Sun 2026-10-25 00:00:00 +03
///        (in UTC): Sat 2026-10-24 21:00:00 UTC
/// ```
///
/// On a host whose zone is UTC the `(in UTC)` lines are missing and the
/// main lines end in `UTC` themselves.
fn parse(stdout: &str, expressions: &[String]) -> HashMap<String, Vec<Timestamp>> {
    let blocks = stdout
        .split("\n\n")
        .filter(|block| !block.trim().is_empty());

    expressions
        .iter()
        .zip(blocks)
        .map(|(expression, block)| (expression.clone(), parse_block(block)))
        .collect()
}

fn parse_block(block: &str) -> Vec<Timestamp> {
    block
        .lines()
        .filter_map(|line| {
            let (label, value) = line.split_once(": ")?;
            let label = label.trim();
            let is_elapse =
                label == "Next elapse" || label == "(in UTC)" || label.starts_with("Iteration #");
            // Lines in a non-UTC zone do not parse; their `(in UTC)` twin does.
            is_elapse.then(|| parse_utc(value)).flatten()
        })
        .collect()
}

/// `Fri 2026-10-23 21:00:00 UTC`
fn parse_utc(value: &str) -> Option<Timestamp> {
    let mut parts = value.split_whitespace();
    let (_weekday, date, time, zone) = (parts.next()?, parts.next()?, parts.next()?, parts.next()?);
    if zone != "UTC" {
        return None;
    }

    let civil: DateTime = format!("{date}T{time}").parse().ok()?;
    Some(civil.to_zoned(TimeZone::UTC).ok()?.timestamp())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_local_zone_output() {
        let stdout = "\
Normalized form: Mon..Fri *-*-* 03:00:00
    Next elapse: Fri 2026-10-23 03:00:00 +03
       (in UTC): Fri 2026-10-23 00:00:00 UTC
       From now: 2h 5min left
   Iteration #2: Mon 2026-10-26 03:00:00 +03
       (in UTC): Mon 2026-10-26 00:00:00 UTC
       From now: 3 days left

  Original form: daily
Normalized form: *-*-* 00:00:00
    Next elapse: Sat 2026-10-24 00:00:00 +03
       (in UTC): Fri 2026-10-23 21:00:00 UTC
       From now: 23h left
";
        let expressions = ["Mon..Fri *-*-* 03:00:00".to_owned(), "daily".to_owned()];
        let parsed = parse(stdout, &expressions);
        let weekdays = &parsed["Mon..Fri *-*-* 03:00:00"];
        assert_eq!(weekdays.len(), 2);
        assert_eq!(weekdays[0].to_string(), "2026-10-23T00:00:00Z");
        assert_eq!(weekdays[1].to_string(), "2026-10-26T00:00:00Z");
        assert_eq!(parsed["daily"].len(), 1);
    }

    #[test]
    fn parses_utc_zone_output() {
        let stdout = "\
  Original form: daily
Normalized form: *-*-* 00:00:00
    Next elapse: Sat 2026-10-24 00:00:00 UTC
       From now: 23h left
";
        let parsed = parse(stdout, &["daily".to_owned()]);
        assert_eq!(parsed["daily"][0].to_string(), "2026-10-24T00:00:00Z");
    }
}
