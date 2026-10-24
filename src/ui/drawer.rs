//! The drawer below the music box: everything about the selected timer,
//! its runs, and the logs of one run.

use std::sync::Arc;

use iced::widget::{Column, button, column, container, row, scrollable, space, text};
use iced::{Border, Element, Font, Length};
use jiff::Timestamp;
use jiff::tz::TimeZone;

use super::label;
use super::overview;
use super::palette;
use super::{Message, controls, musicbox};
use crate::model::{LogLine, Outcome, Run, Timer};
use crate::systemd::timespan;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Overview,
    Runs,
    Logs,
}

/// The open drawer. The timer is kept by id, so it survives refreshes.
#[derive(Debug, Clone)]
pub struct Drawer {
    pub timer: String,
    pub tab: Tab,
    /// The run whose logs to show, by start time; `None` means the latest.
    pub run: Option<Timestamp>,
    pub logs: Logs,
}

impl Drawer {
    pub fn new(timer: String) -> Self {
        Self {
            timer,
            tab: Tab::default(),
            run: None,
            logs: Logs::None,
        }
    }

    /// The run shown in the logs tab.
    pub fn selected_run<'a>(&self, timer: &'a Timer) -> Option<&'a Run> {
        match self.run {
            Some(start) => timer.runs.iter().find(|run| run.start == start),
            None => timer.runs.last(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Logs {
    None,
    Loading {
        invocation: String,
    },
    Loaded {
        invocation: String,
        lines: Arc<Vec<LogLine>>,
    },
    Failed {
        invocation: String,
        error: String,
    },
}

impl Logs {
    pub fn invocation(&self) -> Option<&str> {
        match self {
            Logs::None => None,
            Logs::Loading { invocation }
            | Logs::Loaded { invocation, .. }
            | Logs::Failed { invocation, .. } => Some(invocation),
        }
    }
}

pub fn view<'a>(
    drawer: &'a Drawer,
    timer: &'a Timer,
    now: Timestamp,
    tz: &TimeZone,
) -> Element<'a, Message> {
    let tabs = controls::segmented(
        [
            (Tab::Overview, "Overview"),
            (Tab::Runs, "Runs"),
            (Tab::Logs, "Logs"),
        ],
        drawer.tab,
        Message::SetTab,
    );
    let title = row![
        text(timer.name()).size(18),
        overview::identity(timer),
        space::horizontal(),
        tabs,
        controls::icon("✕", Message::Deselect),
    ]
    .spacing(10)
    .align_y(iced::Alignment::Center);

    let body = match drawer.tab {
        Tab::Overview => overview::view(timer, drawer.run, now, tz),
        Tab::Runs => runs(drawer, timer, tz),
        Tab::Logs => logs(drawer, timer, tz),
    };

    let mut content = Column::new().spacing(14).push(title);
    if !timer.description.is_empty() {
        content = content.push(text(&timer.description).size(13).color(palette::MUTED));
    }

    container(content.push(body))
        .width(Length::Fill)
        .height(Length::Fill)
        // With the border, content starts where the music box's labels do.
        .padding([16.0, musicbox::PAD - 1.0])
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

fn runs<'a>(drawer: &'a Drawer, timer: &'a Timer, tz: &TimeZone) -> Element<'a, Message> {
    if timer.runs.is_empty() {
        return muted("No runs in the loaded range.");
    }

    let selected = drawer.selected_run(timer).map(|run| run.start);
    let rows = timer.runs.iter().rev().map(|run| {
        let (result, color) = outcome(run);
        let line = row![
            text(label::moment(run.start, tz))
                .size(13)
                .width(Length::FillPortion(3)),
            text(duration(run))
                .size(13)
                .color(palette::MUTED)
                .width(Length::FillPortion(2)),
            text(result)
                .size(13)
                .color(color)
                .width(Length::FillPortion(3)),
            text("logs →").size(12).color(palette::MUTED),
        ]
        .spacing(12);

        let is_selected = Some(run.start) == selected;
        button(line)
            .width(Length::Fill)
            .style(move |_, status| run_row(is_selected, status))
            .on_press(Message::OpenRun(run.start))
            .into()
    });

    scrollable(Column::from_iter(rows).spacing(2))
        .height(Length::Fill)
        .into()
}

fn logs<'a>(drawer: &'a Drawer, timer: &'a Timer, tz: &TimeZone) -> Element<'a, Message> {
    let Some(run) = drawer.selected_run(timer) else {
        return muted("No runs in the loaded range.");
    };
    let (result, color) = outcome(run);
    let header = row![
        text(format!("Run · {}", label::moment(run.start, tz))).size(13),
        text(duration(run)).size(13).color(palette::MUTED),
        text(result).size(13).color(color),
    ]
    .spacing(12);

    let body: Element<'a, Message> = match (&drawer.logs, run.invocation.as_deref()) {
        (_, None) => muted("systemd did not record an invocation ID for this run."),
        (Logs::Failed { error, .. }, _) => text(format!("Could not read the logs: {error}"))
            .size(13)
            .color(palette::FAILED)
            .into(),
        (Logs::Loaded { lines, .. }, _) if lines.is_empty() => {
            muted("This run wrote nothing to the journal.")
        }
        (Logs::Loaded { lines, .. }, _) => {
            let rows = lines.iter().map(|line| log_line(line, tz));
            scrollable(Column::from_iter(rows).spacing(2))
                .anchor_bottom()
                .height(Length::Fill)
                .width(Length::Fill)
                .into()
        }
        (Logs::None | Logs::Loading { .. }, _) => muted("Loading logs…"),
    };

    column![header, body].spacing(10).into()
}

fn log_line<'a>(line: &'a LogLine, tz: &TimeZone) -> Element<'a, Message> {
    let zoned = line.at.to_zoned(tz.clone());
    let time = format!(
        "{:02}:{:02}:{:02}.{:03}",
        zoned.hour(),
        zoned.minute(),
        zoned.second(),
        zoned.millisecond()
    );
    // syslog priorities: 0-3 are errors, 4 warnings.
    let color = match line.priority {
        0..=3 => palette::FAILED,
        4 => palette::NOTE,
        _ => palette::TEXT,
    };

    row![
        text(time)
            .size(12)
            .font(Font::MONOSPACE)
            .color(palette::MUTED),
        text(&line.message)
            .size(12)
            .font(Font::MONOSPACE)
            .color(color)
            .width(Length::Fill),
    ]
    .spacing(12)
    .into()
}

/// A quiet row: a dark band when selected, a lighter one under the mouse,
/// so the row's own text colors stay readable.
fn run_row(selected: bool, status: button::Status) -> button::Style {
    let background = match (selected, status) {
        (_, button::Status::Hovered | button::Status::Pressed) => Some(palette::GRID_MAJOR),
        (true, _) => Some(palette::GRID),
        (false, _) => None,
    };
    button::Style {
        background: background.map(Into::into),
        text_color: palette::TEXT,
        border: Border {
            radius: 4.0.into(),
            ..Border::default()
        },
        ..button::Style::default()
    }
}

fn outcome(run: &Run) -> (String, iced::Color) {
    match &run.outcome {
        Outcome::Success => ("success".to_owned(), palette::MUTED),
        Outcome::Running => ("running".to_owned(), palette::NOTE),
        Outcome::Failed(result) => (format!("failed: {result}"), palette::FAILED),
    }
}

fn duration(run: &Run) -> String {
    match (run.end, &run.outcome) {
        // An end equal to the start means systemd logged no end.
        (Some(end), outcome) if *outcome != Outcome::Running && end > run.start => {
            timespan::format(end.duration_since(run.start))
        }
        _ => String::new(),
    }
}

fn muted(message: &str) -> Element<'_, Message> {
    text(message).size(13).color(palette::MUTED).into()
}
