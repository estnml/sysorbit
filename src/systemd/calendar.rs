//! Expands `OnCalendar=` expressions into elapse times with
//! `systemd-analyze calendar`, so results match systemd exactly.

use std::collections::HashMap;

use jiff::Timestamp;
use jiff::civil::DateTime;
use jiff::tz::TimeZone;

use super::{Capabilities, Error, run};
use crate::host::{Host, Invocation};
use crate::model::Interval;

/// Upper bound per expression; keeps `*:*:0/1` style expressions sane.
const MAX_ITERATIONS: usize = 5000;

/// Returns the elapse times of each expression inside `window`.
/// Expressions systemd rejects map to an empty list.
///
/// Older systemd versions get less: without `--base-time` (< 244) the times
/// start now instead of at the window start, without `--iterations` (< 242)
/// only the next elapse is known.
pub async fn occurrences(
    host: &dyn Host,
    expressions: &[String],
    window: Interval,
    capabilities: Capabilities,
) -> HashMap<String, Vec<Timestamp>> {
    let mut result = HashMap::new();
    if expressions.is_empty() {
        return result;
    }

    match analyze(host, expressions, window, capabilities).await {
        Ok(parsed) => result.extend(parsed),
        // One bad expression fails the whole batch; retry one by one.
        Err(_) => {
            crate::diagnostics::event(crate::diagnostics::Event::CalendarSingleRetry);
            for expression in expressions {
                let single = std::slice::from_ref(expression);
                if let Ok(parsed) = analyze(host, single, window, capabilities).await {
                    result.extend(parsed);
                }
            }
        }
    }

    for times in result.values_mut() {
        times.retain(|&at| window.start <= at && at < window.end);
    }

    result
}

async fn analyze(
    host: &dyn Host,
    expressions: &[String],
    window: Interval,
    capabilities: Capabilities,
) -> Result<HashMap<String, Vec<Timestamp>>, Error> {
    let mut invocation = Invocation::new("systemd-analyze").arg("calendar");
    if capabilities.calendar_iterations() {
        invocation = invocation.arg(format!("--iterations={MAX_ITERATIONS}"));
    }
    if capabilities.calendar_base_time() {
        invocation = invocation.arg(format!("--base-time=@{}", window.start.as_second()));
    }
    let invocation = invocation.args(expressions.iter().cloned());

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
            // systemd before 255 or so abbreviates `Iteration #2` to `Iter. #2`.
            let is_elapse = label == "Next elapse"
                || label == "(in UTC)"
                || label.starts_with("Iteration #")
                || label.starts_with("Iter. #");
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
    fn parses_abbreviated_iterations() {
        // systemd 252 on a host in UTC.
        let stdout = "\
Normalized form: *-*-* *:00/15:00
    Next elapse: Thu 2026-10-22 21:00:00 UTC
       From now: 1h 48min ago
       Iter. #2: Thu 2026-10-22 21:15:00 UTC
       From now: 1h 33min ago
";
        let parsed = parse(stdout, &["*-*-* *:00/15:00".to_owned()]);
        assert_eq!(parsed["*-*-* *:00/15:00"].len(), 2);
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
