//! Durations in systemd notation, such as `1h 40min`.

use jiff::SignedDuration;

/// Formats a span compactly in systemd notation, keeping the two largest units.
pub fn format(span: SignedDuration) -> String {
    const UNITS: [(&str, i64); 5] = [
        ("w", 604_800),
        ("d", 86_400),
        ("h", 3600),
        ("min", 60),
        ("s", 1),
    ];

    let mut seconds = span.as_secs().abs();
    if seconds == 0 {
        let millis = span.as_millis().abs();
        return if millis == 0 {
            "0".to_owned()
        } else {
            format!("{millis}ms")
        };
    }

    let parts: Vec<String> = UNITS
        .iter()
        .filter_map(|&(name, size)| {
            let count = seconds / size;
            seconds %= size;
            (count > 0).then(|| format!("{count}{name}"))
        })
        .take(2)
        .collect();

    parts.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_two_largest_units() {
        assert_eq!(format(SignedDuration::from_mins(100)), "1h 40min");
        assert_eq!(format(SignedDuration::from_secs(86_400 + 905)), "1d 15min");
        assert_eq!(format(SignedDuration::from_millis(300)), "300ms");
    }
}
