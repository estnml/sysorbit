//! Zoom levels and the mapping between time and canvas coordinates.

use std::time::Duration;

use jiff::{SignedDuration, Timestamp};

use crate::model::Interval;

/// Fraction of the grid width left of the comb. The comb stays put while the
/// cylinder turns underneath it.
pub const COMB_RATIO: f64 = 0.3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Zoom {
    Hours,
    Day,
    #[default]
    Week,
    Month,
}

impl Zoom {
    pub const ALL: [Zoom; 4] = [Zoom::Hours, Zoom::Day, Zoom::Week, Zoom::Month];

    pub fn label(self) -> &'static str {
        match self {
            Zoom::Hours => "6 hours",
            Zoom::Day => "Day",
            Zoom::Week => "Week",
            Zoom::Month => "Month",
        }
    }

    pub fn span(self) -> SignedDuration {
        SignedDuration::from_hours(match self {
            Zoom::Hours => 6,
            Zoom::Day => 24,
            Zoom::Week => 24 * 7,
            Zoom::Month => 24 * 30,
        })
    }

    /// The visible stretch of time with the comb at `now`.
    pub fn visible(self, now: Timestamp) -> Interval {
        let span = self.span();
        let past = span.mul_f64(COMB_RATIO);
        Interval {
            start: now - past,
            end: now + (span - past),
        }
    }

    /// What to fetch: the visible stretch plus margins, so the view can keep
    /// turning until the next refresh and skipped runs near the left edge
    /// still find their catch-up.
    pub fn fetch_window(self, now: Timestamp) -> Interval {
        let visible = self.visible(now);
        let margin = self.span().mul_f64(0.25);
        Interval {
            start: visible.start - margin,
            end: visible.end + margin,
        }
    }

    /// How often the comb needs to move: about once per pixel, rounded down
    /// to a divisor of a minute so the ticks also land on every minute.
    pub fn tick(self) -> Duration {
        let per_pixel = self.span().as_secs() / 2000;
        let secs = [60, 30, 20, 15, 12, 10, 6, 5, 4, 3, 2]
            .into_iter()
            .find(|&secs| secs <= per_pixel)
            .unwrap_or(1);
        Duration::from_secs(secs as u64)
    }
}

/// Converts between time and x inside the grid area.
#[derive(Debug, Clone, Copy)]
pub struct TimeAxis {
    pub window: Interval,
    pub left: f32,
    pub right: f32,
}

impl TimeAxis {
    pub fn x(&self, at: Timestamp) -> f32 {
        let total = self
            .window
            .end
            .duration_since(self.window.start)
            .as_secs_f64();
        let offset = at.duration_since(self.window.start).as_secs_f64();
        self.left + ((offset / total) as f32) * (self.right - self.left)
    }

    pub fn time(&self, x: f32) -> Timestamp {
        let total = self.window.end.duration_since(self.window.start);
        let ratio = f64::from((x - self.left) / (self.right - self.left));
        self.window.start + total.mul_f64(ratio)
    }

    /// Pixels covered by `span`.
    pub fn width(&self, span: SignedDuration) -> f32 {
        let total = self
            .window
            .end
            .duration_since(self.window.start)
            .as_secs_f64();
        ((span.as_secs_f64() / total) as f32) * (self.right - self.left)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comb_sits_at_ratio() {
        let now = Timestamp::from_second(1_800_000_000).unwrap();
        let axis = TimeAxis {
            window: Zoom::Week.visible(now),
            left: 100.0,
            right: 1100.0,
        };
        assert!((axis.x(now) - 400.0).abs() < 0.01);
        assert!(axis.time(400.0).duration_since(now).abs() < SignedDuration::from_secs(1));
    }
}
