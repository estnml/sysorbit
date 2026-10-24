//! Turns a [`Timer`] into the marks drawn on its music box row: what ran,
//! what will run, what was skipped and what was caught up.

use jiff::{SignedDuration, Timestamp};

use crate::model::{Downtime, Outcome, Timer};

/// Slack added on top of `AccuracySec=` when matching a run to an occurrence.
const MATCH_SLACK: SignedDuration = SignedDuration::from_mins(2);

#[derive(Debug, Clone, PartialEq)]
pub enum Mark {
    Run {
        start: Timestamp,
        end: Option<Timestamp>,
        outcome: Outcome,
    },
    /// Future elapse of an active timer. `delay` is the randomized window
    /// after `at` in which the run will actually happen; `scheduled` is the
    /// moment systemd already picked inside it, known for the next one only.
    Planned {
        at: Timestamp,
        delay: SignedDuration,
        scheduled: Option<Timestamp>,
        wakes: bool,
    },
    /// Elapse an inactive timer would have had.
    Ghost { at: Timestamp },
    /// Past elapse with no run.
    Skipped { at: Timestamp, while_down: bool },
    /// `Persistent=true` running a skipped elapse later.
    CatchUp { from: Timestamp, to: Timestamp },
    /// Monotonic trigger counting from `from` until `to`.
    Tether { from: Timestamp, to: Timestamp },
}

impl Mark {
    /// Where the mark sits on the time axis, for hit testing and sorting.
    pub fn anchor(&self) -> Timestamp {
        match self {
            Self::Run { start, .. } => *start,
            Self::Planned { at, .. } | Self::Ghost { at } | Self::Skipped { at, .. } => *at,
            Self::CatchUp { to, .. } | Self::Tether { to, .. } => *to,
        }
    }
}

pub fn marks(
    timer: &Timer,
    downtime: &[Downtime],
    history_known: bool,
    now: Timestamp,
) -> Vec<Mark> {
    let mut marks: Vec<Mark> = timer
        .runs
        .iter()
        .map(|run| Mark::Run {
            start: run.start,
            // A running run may carry a tentative end from the journal; it
            // is drawn up to now.
            end: run.end.filter(|_| run.outcome != Outcome::Running),
            outcome: run.outcome.clone(),
        })
        .collect();

    let mut occurrences: Vec<Timestamp> = timer
        .calendar
        .iter()
        .flat_map(|trigger| trigger.occurrences.iter().copied())
        .collect();
    occurrences.sort();
    occurrences.dedup();

    if !timer.active {
        marks.extend(occurrences.into_iter().map(|at| Mark::Ghost { at }));
        marks.sort_by_key(Mark::anchor);
        return marks;
    }

    let tolerance = timer.randomized_delay + timer.accuracy + MATCH_SLACK;
    // Judging the past needs history, and a timer that never ran has none.
    let judge_past = history_known && (timer.last_trigger.is_some() || !timer.runs.is_empty());

    for (index, &at) in occurrences.iter().enumerate() {
        if at > now {
            let scheduled = timer
                .next_elapse
                .filter(|&next| next >= at && next.duration_since(at) <= tolerance);
            marks.push(Mark::Planned {
                at,
                delay: timer.randomized_delay,
                scheduled,
                wakes: timer.wake_system,
            });
            continue;
        }

        if !judge_past {
            continue;
        }
        // An elapse while the machine was down can only have been caught up
        // later, however close that run is.
        let while_down = downtime.iter().any(|down| down.interval.contains(at));
        let ran = timer
            .runs
            .iter()
            .any(|run| run.start >= at && run.start.duration_since(at) <= tolerance);
        if ran && !while_down {
            continue;
        }

        marks.push(Mark::Skipped { at, while_down });

        if timer.persistent && while_down {
            let next = occurrences.get(index + 1).copied();
            let catch_up = timer
                .runs
                .iter()
                .find(|run| run.start > at && next.is_none_or(|next| run.start < next));
            if let Some(run) = catch_up {
                marks.push(Mark::CatchUp {
                    from: at,
                    to: run.start,
                });
            }
        }
    }

    marks.extend(monotonic_marks(timer, &occurrences, now));
    marks.sort_by_key(Mark::anchor);
    marks
}

/// Systemd only reports the next monotonic elapse; repeating triggers are
/// projected forward from it.
fn monotonic_marks(timer: &Timer, occurrences: &[Timestamp], now: Timestamp) -> Vec<Mark> {
    let mut marks = Vec::new();
    let Some(next) = timer.next_elapse else {
        return marks;
    };
    if timer.monotonic.is_empty() || next <= now {
        return marks;
    }

    // With mixed triggers the next elapse may belong to a calendar one.
    let window = timer.randomized_delay + timer.accuracy + MATCH_SLACK;
    let from_calendar = occurrences
        .iter()
        .any(|&at| at <= next && next.duration_since(at) <= window);
    if from_calendar {
        return marks;
    }

    let anchor = timer
        .runs
        .last()
        .map(|run| run.start)
        .or(timer.last_trigger);
    if let Some(from) = anchor {
        marks.push(Mark::Tether { from, to: next });
    }

    // Planned marks for a repeating trigger stop where the data ends; the
    // caller clips them to the visible window.
    let period = timer
        .monotonic
        .iter()
        .filter(|trigger| trigger.base.repeats())
        .map(|trigger| trigger.offset)
        .filter(|offset| offset.is_positive())
        .min();
    let horizon = SignedDuration::from_hours(24 * 45);
    let mut at = next;
    loop {
        marks.push(Mark::Planned {
            at,
            delay: timer.randomized_delay,
            scheduled: None,
            wakes: timer.wake_system,
        });
        match period {
            Some(period) if at.duration_since(next) + period <= horizon => at += period,
            _ => break,
        }
    }

    marks
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{CalendarTrigger, DowntimeKind, Interval, Run};

    fn ts(hours: i64) -> Timestamp {
        Timestamp::from_second(1_800_000_000 + hours * 3600).unwrap()
    }

    fn timer(occurrences: Vec<Timestamp>, runs: Vec<Run>) -> Timer {
        Timer {
            id: "cleanup.timer".into(),
            description: String::new(),
            service: "cleanup.service".into(),
            service_active: false,
            active: true,
            unit_file_state: "enabled".into(),
            calendar: vec![CalendarTrigger {
                expression: "daily".into(),
                occurrences,
            }],
            monotonic: Vec::new(),
            next_elapse: None,
            last_trigger: Some(ts(0)),
            persistent: true,
            wake_system: false,
            accuracy: SignedDuration::from_mins(1),
            randomized_delay: SignedDuration::ZERO,
            runs,
        }
    }

    fn run_at(hours: i64) -> Run {
        Run {
            start: ts(hours),
            end: Some(ts(hours)),
            outcome: Outcome::Success,
            invocation: None,
        }
    }

    #[test]
    fn persistent_timer_catches_up_after_downtime() {
        let timer = timer(vec![ts(0), ts(24), ts(48)], vec![run_at(0), run_at(30)]);
        let downtime = [Downtime {
            interval: Interval {
                start: ts(20),
                end: ts(30),
            },
            kind: DowntimeKind::PoweredOff,
        }];

        let marks = marks(&timer, &downtime, true, ts(40));

        assert!(marks.contains(&Mark::Skipped {
            at: ts(24),
            while_down: true
        }));
        assert!(marks.contains(&Mark::CatchUp {
            from: ts(24),
            to: ts(30)
        }));
        assert!(
            marks
                .iter()
                .any(|mark| matches!(mark, Mark::Planned { at, .. } if *at == ts(48)))
        );
    }

    #[test]
    fn run_right_after_downtime_is_a_catch_up() {
        // Generous accuracy would otherwise count the late run as on time.
        let mut timer = timer(vec![ts(0), ts(24)], vec![run_at(0), run_at(26)]);
        timer.accuracy = SignedDuration::from_hours(12);
        let downtime = [Downtime {
            interval: Interval {
                start: ts(23),
                end: ts(26),
            },
            kind: DowntimeKind::PoweredOff,
        }];

        let marks = marks(&timer, &downtime, true, ts(30));

        assert!(marks.contains(&Mark::CatchUp {
            from: ts(24),
            to: ts(26)
        }));
    }

    #[test]
    fn inactive_timer_only_has_ghosts() {
        let mut timer = timer(vec![ts(0), ts(24)], Vec::new());
        timer.active = false;

        let marks = marks(&timer, &[], true, ts(12));

        assert_eq!(
            marks,
            vec![Mark::Ghost { at: ts(0) }, Mark::Ghost { at: ts(24) }]
        );
    }

    #[test]
    fn unknown_history_skips_nothing() {
        let timer = timer(vec![ts(0)], Vec::new());
        let marks = marks(&timer, &[], false, ts(12));
        assert!(marks.is_empty());
    }
}
