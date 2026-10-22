//! Side panel with everything known about the selected timer.

use iced::widget::{Column, button, column, container, row, scrollable, text};
use iced::{Border, Element, Length};
use jiff::Timestamp;
use jiff::tz::TimeZone;

use super::Message;
use super::label;
use super::palette;
use crate::model::{MonotonicBase, Outcome, Timer};
use crate::systemd::timespan;

const RECENT_RUNS: usize = 8;

pub fn view<'a>(timer: &'a Timer, now: Timestamp, tz: &TimeZone) -> Element<'a, Message> {
    let mut body = Column::new().spacing(14);

    body = body.push(
        column![
            row![
                text(timer.name()).size(20).width(Length::Fill),
                button(text("✕").size(14))
                    .style(button::text)
                    .on_press(Message::Deselect),
            ],
            text(format!("{} → {}", timer.id, timer.service))
                .size(12)
                .color(palette::MUTED),
        ]
        .spacing(4),
    );

    if !timer.description.is_empty() {
        body = body.push(text(&timer.description).size(13));
    }

    let state = if timer.active { "active" } else { "inactive" };
    body = body.push(section(
        "State",
        vec![format!("{state} · {}", timer.unit_file_state)],
    ));

    let triggers: Vec<String> = timer
        .calendar
        .iter()
        .map(|trigger| format!("OnCalendar={}", trigger.expression))
        .chain(timer.monotonic.iter().map(|trigger| {
            let key = match trigger.base {
                MonotonicBase::TimerActivated => "OnActiveSec",
                MonotonicBase::Boot => "OnBootSec",
                MonotonicBase::Startup => "OnStartupSec",
                MonotonicBase::UnitActivated => "OnUnitActiveSec",
                MonotonicBase::UnitDeactivated => "OnUnitInactiveSec",
            };
            format!("{key}={}", timespan::format(trigger.offset))
        }))
        .collect();
    body = body.push(section("Triggers", triggers));

    let when = |at: Option<Timestamp>| match at {
        Some(at) => format!("{} ({})", label::moment(at, tz), label::relative(at, now)),
        None => "—".to_owned(),
    };
    body = body.push(section(
        "Timing",
        vec![
            format!("next: {}", when(timer.next_elapse)),
            format!("last: {}", when(timer.last_trigger)),
        ],
    ));

    body = body.push(section(
        "Settings",
        vec![
            format!("Persistent={}", yes_no(timer.persistent)),
            format!("WakeSystem={}", yes_no(timer.wake_system)),
            format!("AccuracySec={}", timespan::format(timer.accuracy)),
            format!(
                "RandomizedDelaySec={}",
                timespan::format(timer.randomized_delay)
            ),
        ],
    ));

    let mut runs = Column::new().spacing(4);
    if timer.runs.is_empty() {
        runs = runs.push(text("no runs in this range").size(13).color(palette::MUTED));
    }
    for run in timer.runs.iter().rev().take(RECENT_RUNS) {
        let (result, color) = match &run.outcome {
            Outcome::Success => ("success".to_owned(), palette::MUTED),
            Outcome::Running => ("running".to_owned(), palette::NOTE),
            Outcome::Failed(result) => (result.clone(), palette::FAILED),
        };
        let duration = run
            .end
            .map(|end| timespan::format(end.duration_since(run.start)))
            .unwrap_or_default();
        runs = runs.push(
            row![
                text(label::moment(run.start, tz))
                    .size(13)
                    .width(Length::Fill),
                text(duration).size(13).color(palette::MUTED),
                text(result).size(13).color(color),
            ]
            .spacing(10),
        );
    }
    body = body.push(column![heading("Recent runs"), runs].spacing(6));

    container(scrollable(body).spacing(8))
        .width(340)
        .height(Length::Fill)
        .padding(18)
        .style(|_| container::Style {
            background: Some(palette::SURFACE.into()),
            border: Border {
                color: palette::GRID_MAJOR,
                width: 1.0,
                radius: 10.0.into(),
            },
            ..container::Style::default()
        })
        .into()
}

fn heading(title: &str) -> Element<'_, Message> {
    text(title.to_uppercase())
        .size(11)
        .color(palette::MUTED)
        .into()
}

fn section(title: &str, lines: Vec<String>) -> Element<'_, Message> {
    let mut content = Column::new().spacing(3).push(heading(title));
    for line in lines {
        content = content.push(text(line).size(13));
    }
    content.into()
}

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}
