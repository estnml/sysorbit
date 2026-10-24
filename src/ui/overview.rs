//! The drawer's overview tab: when the timer fires, where it stands between
//! its last and next elapse, and how its recent runs went. Drawn with the
//! same marks as the music box, with plain English one hover away.

use iced::alignment::Vertical;
use iced::mouse;
use iced::widget::canvas::{self, Canvas, Frame, Path, Stroke, Text};
use iced::widget::text::Alignment;
use iced::widget::{Column, Row, column, container, responsive, row, scrollable, text, tooltip};
use iced::{Border, Color, Element, Font, Length, Point, Rectangle, Renderer, Size, Theme};
use jiff::tz::TimeZone;
use jiff::{SignedDuration, Timestamp};

use super::{Message, explain, label, musicbox, palette};
use crate::model::{MonotonicBase, Outcome, Run, Timer};
use crate::systemd::timespan;

/// Space between the cards.
const GAP: f32 = 12.0;
/// All cards share one height so their edges line up; longer content
/// scrolls inside its card.
const CARD_HEIGHT: f32 = 184.0;
/// `AccuracySec=` at or below this is systemd's default slack, not worth a band.
const DEFAULT_ACCURACY: SignedDuration = SignedDuration::from_mins(1);
const ACCURACY: Color = Color {
    a: 0.22,
    ..palette::COMB
};
const DELAY: Color = Color {
    a: 0.25,
    ..palette::NOTE
};

/// `● shadow.timer → ● shadow.service` and badges for the timer's state,
/// each explained on hover.
pub fn identity(timer: &Timer) -> Element<'_, Message> {
    let unit = |name: &str, lit: bool| {
        row![
            dot(if lit { palette::NOTE } else { palette::FAINT }),
            text(name.to_owned()).size(12).color(palette::MUTED),
        ]
        .spacing(6)
        .align_y(Vertical::Center)
    };
    let service = hint(
        unit(&timer.service, timer.service_active),
        if timer.service_active {
            "The service is running right now."
        } else {
            "The service is not running."
        },
    );

    let mut badges = vec![
        badge(
            if timer.active { "active" } else { "inactive" },
            if timer.active {
                "The timer is started and triggers its service when it elapses."
            } else {
                "The timer is stopped; it will not trigger until it is started."
            },
        ),
        badge(
            &timer.unit_file_state,
            unit_file_help(&timer.unit_file_state),
        ),
    ];
    if timer.persistent {
        badges.push(badge(
            "Persistent",
            "Persistent=yes: an elapse missed while the machine was off or \
             asleep runs right after it comes back.",
        ));
    }
    if timer.wake_system {
        badges.push(badge(
            "WakeSystem",
            "WakeSystem=yes: wakes the machine from suspend when the timer elapses.",
        ));
    }

    row![
        unit(&timer.id, timer.active),
        text("→").size(12).color(palette::FAINT),
        service,
        Row::from_vec(badges).spacing(6),
    ]
    .spacing(8)
    .align_y(Vertical::Center)
    .into()
}

pub fn view<'a>(
    timer: &'a Timer,
    selected_run: Option<Timestamp>,
    now: Timestamp,
    tz: &TimeZone,
) -> Element<'a, Message> {
    let tz = tz.clone();
    responsive(move |size| {
        // The drawer is as wide as the music box and its content inset by
        // the box's margin, so the schedule card ends where the grid starts
        // and the other cards sit under the grid.
        let grid_left = musicbox::grid_left(size.width + 2.0 * musicbox::PAD) - musicbox::PAD;
        let cards = row![
            card("Schedule", schedule(timer, now, &tz))
                .width(Length::Fixed((grid_left - GAP).max(160.0))),
            card("Next run", timing(timer, now, &tz)).width(Length::FillPortion(3)),
            card("Recent runs", recent(timer, selected_run, now, &tz))
                .width(Length::FillPortion(2)),
        ]
        .spacing(GAP)
        .height(Length::Fixed(CARD_HEIGHT));

        scrollable(cards).height(Length::Fill).into()
    })
    .into()
}

/// Each trigger in systemd's notation, with its meaning behind an info icon.
fn schedule<'a>(timer: &'a Timer, now: Timestamp, tz: &TimeZone) -> Element<'a, Message> {
    let calendar = timer.calendar.iter().map(|trigger| {
        let meaning = explain::calendar(&trigger.expression)
            .unwrap_or_else(|| "No plain form for this expression.".to_owned());
        let upcoming: Vec<String> = trigger
            .occurrences
            .iter()
            .filter(|&&at| at >= now)
            .take(3)
            .map(|&at| label::moment(at, tz))
            .collect();
        let mut help = column![text(meaning).size(13)].spacing(4);
        if !upcoming.is_empty() {
            help = help.push(
                text(format!("Next: {}", upcoming.join(", ")))
                    .size(12)
                    .color(palette::MUTED),
            );
        }
        trigger_row("OnCalendar", &trigger.expression, help.into())
    });

    let monotonic = timer.monotonic.iter().map(|trigger| {
        let key = match trigger.base {
            MonotonicBase::TimerActivated => "OnActiveSec",
            MonotonicBase::Boot => "OnBootSec",
            MonotonicBase::Startup => "OnStartupSec",
            MonotonicBase::UnitActivated => "OnUnitActiveSec",
            MonotonicBase::UnitDeactivated => "OnUnitInactiveSec",
        };
        trigger_row(
            key,
            &timespan::format(trigger.offset),
            text(explain::monotonic(trigger)).size(13).into(),
        )
    });

    let rows: Vec<Element<'a, Message>> = calendar.chain(monotonic).collect();
    if rows.is_empty() {
        return muted("No triggers.");
    }
    Column::from_vec(rows).spacing(8).into()
}

/// The setting's name over its value, so long expressions fit the card.
fn trigger_row<'a>(key: &str, value: &str, help: Element<'a, Message>) -> Element<'a, Message> {
    row![
        tip(info_icon(), help),
        column![
            text(key.to_owned()).size(11).color(palette::MUTED),
            text(value.to_owned()).size(12).font(Font::MONOSPACE),
        ]
        .spacing(2),
    ]
    .spacing(8)
    .align_y(Vertical::Center)
    .into()
}

/// The bar from the last elapse to the next, and what each band on it means.
fn timing<'a>(timer: &'a Timer, now: Timestamp, tz: &TimeZone) -> Element<'a, Message> {
    let bar = Canvas::new(TimingBar {
        timer,
        now,
        tz: tz.clone(),
    })
    .width(Length::Fill)
    .height(Length::Fixed(62.0));

    let mut notes: Vec<Element<'a, Message>> = Vec::new();
    if let Some(run) = timer
        .runs
        .last()
        .filter(|run| run.outcome == Outcome::Running)
    {
        notes.push(note(
            palette::NOTE,
            format!("Running now, since {}.", label::clock(run.start, tz)),
        ));
    }
    if !timer.active {
        notes.push(note(
            palette::FAINT,
            "The timer is inactive and will not fire until it is started.".to_owned(),
        ));
    } else if timer.next_elapse.is_none() {
        notes.push(note(
            palette::FAINT,
            "systemd has no upcoming elapse for this timer.".to_owned(),
        ));
    }
    if timer.accuracy > DEFAULT_ACCURACY {
        notes.push(note(
            ACCURACY.scale_alpha(2.5),
            format!(
                "May run up to {} late (AccuracySec={}): systemd batches \
                 wakeups inside this window.",
                explain::span_words(timer.accuracy),
                timespan::format(timer.accuracy)
            ),
        ));
    }
    if timer.randomized_delay.is_positive() {
        notes.push(note(
            DELAY.scale_alpha(2.5),
            format!(
                "Waits a random 0 to {} after each elapse (RandomizedDelaySec={}).",
                explain::span_words(timer.randomized_delay),
                timespan::format(timer.randomized_delay)
            ),
        ));
    }

    column![bar, Column::from_vec(notes).spacing(4)]
        .spacing(8)
        .into()
}

/// The recent runs as bars, and numbers summing them up.
fn recent<'a>(
    timer: &'a Timer,
    selected_run: Option<Timestamp>,
    now: Timestamp,
    tz: &TimeZone,
) -> Element<'a, Message> {
    if timer.runs.is_empty() {
        return muted("No runs in the loaded range.");
    }

    let chart = Canvas::new(RunChart {
        runs: &timer.runs,
        selected: selected_run,
        now,
        tz: tz.clone(),
    })
    .width(Length::Fill)
    .height(Length::Fixed(RunChart::HEIGHT));

    let mut durations: Vec<SignedDuration> = timer
        .runs
        .iter()
        .filter(|run| run.outcome != Outcome::Running)
        .filter_map(|run| Some(run.end?.duration_since(run.start)))
        // Zero means systemd logged no end, as for simple user services.
        .filter(|duration| !duration.is_zero())
        .collect();
    durations.sort();
    let failed = timer
        .runs
        .iter()
        .filter(|run| matches!(run.outcome, Outcome::Failed(_)))
        .count();
    let succeeded = timer
        .runs
        .iter()
        .filter(|run| run.outcome == Outcome::Success)
        .count();

    let mut stats = vec![
        stat(format!("{succeeded} succeeded"), palette::MUTED),
        stat(
            format!("{failed} failed"),
            if failed > 0 {
                palette::FAILED
            } else {
                palette::MUTED
            },
        ),
    ];
    if !durations.is_empty() {
        let total: SignedDuration = durations.iter().copied().sum();
        let average = total / durations.len() as i32;
        let p95 = durations[(durations.len() * 95).div_ceil(100).saturating_sub(1)];
        stats.push(stat(
            format!("avg {}", timespan::format(average)),
            palette::MUTED,
        ));
        stats.push(stat(
            format!("p95 {}", timespan::format(p95)),
            palette::MUTED,
        ));
    } else if succeeded + failed > 0 {
        stats.push(stat("durations not logged".to_owned(), palette::FAINT));
    }

    column![chart, Row::from_vec(stats).spacing(12)]
        .spacing(8)
        .into()
}

struct TimingBar<'a> {
    timer: &'a Timer,
    now: Timestamp,
    tz: TimeZone,
}

impl TimingBar<'_> {
    const PAD: f32 = 10.0;
    const BAR_Y: f32 = 32.0;

    fn draw(&self, frame: &mut Frame) {
        let timer = self.timer;
        let now = self.now;
        let width = frame.width();
        let last = timer.last_trigger.filter(|&last| last <= now);
        let next = timer.next_elapse.filter(|_| timer.active);
        let late = if timer.accuracy > DEFAULT_ACCURACY {
            timer.accuracy
        } else {
            SignedDuration::ZERO
        };
        let tail = late.max(timer.randomized_delay);

        let start = last.unwrap_or(now);
        let mut end = next.map_or(now, |next| next + tail).max(now);
        if end <= start {
            end = start + SignedDuration::from_hours(1);
        }
        let span = end.duration_since(start).as_secs_f64();
        let x = |at: Timestamp| {
            let ratio = (at.duration_since(start).as_secs_f64() / span).clamp(0.0, 1.0);
            Self::PAD + (width - 2.0 * Self::PAD) * ratio as f32
        };
        let band = |frame: &mut Frame, from: Timestamp, to: Timestamp, height: f32, color| {
            let (x0, x1) = (x(from), x(to));
            frame.fill(
                &Path::rounded_rectangle(
                    Point::new(x0, Self::BAR_Y - height / 2.0),
                    Size::new((x1 - x0).max(height), height),
                    (height / 2.0).into(),
                ),
                color,
            );
        };

        band(frame, start, end, 4.0, palette::GRID_MAJOR);
        if let Some(next) = next {
            if late.is_positive() {
                band(frame, next, next + late, 14.0, ACCURACY);
            }
            if timer.randomized_delay.is_positive() {
                band(frame, next, next + timer.randomized_delay, 10.0, DELAY);
            }
        }

        if let Some(last) = last {
            band(frame, last, now, 4.0, palette::NOTE.scale_alpha(0.6));
            let failed = timer
                .runs
                .last()
                .is_some_and(|run| matches!(run.outcome, Outcome::Failed(_)));
            let color = if failed {
                palette::FAILED
            } else {
                palette::NOTE
            };
            frame.fill(&Path::circle(Point::new(x(last), Self::BAR_Y), 5.0), color);
            self.label(
                frame,
                Alignment::Left,
                format!(
                    "last {} · {}",
                    label::moment(last, &self.tz),
                    label::relative(last, now)
                ),
                palette::TEXT,
            );
        }

        let now_x = x(now);
        frame.stroke(
            &Path::line(
                Point::new(now_x, Self::BAR_Y - 9.0),
                Point::new(now_x, Self::BAR_Y + 9.0),
            ),
            Stroke::default().with_color(palette::COMB).with_width(2.0),
        );
        frame.fill_text(Text {
            content: format!("now {}", label::clock(now, &self.tz)),
            position: Point::new(now_x.clamp(30.0, width - 30.0), Self::BAR_Y + 21.0),
            color: palette::COMB,
            size: 11.0.into(),
            align_x: Alignment::Center,
            align_y: Vertical::Center,
            ..Text::default()
        });

        match next {
            Some(next) => {
                let at = Point::new(x(next), Self::BAR_Y);
                frame.fill(&Path::circle(at, 6.0), palette::SURFACE);
                frame.fill(&Path::circle(at, 5.0), palette::NOTE.scale_alpha(0.55));
                if timer.wake_system {
                    frame.stroke(
                        &Path::circle(at, 9.0),
                        Stroke::default()
                            .with_color(palette::NOTE.scale_alpha(0.8))
                            .with_width(1.5),
                    );
                }
                self.label(
                    frame,
                    Alignment::Right,
                    format!(
                        "next {} · {}",
                        label::moment(next, &self.tz),
                        label::relative(next, now)
                    ),
                    palette::TEXT,
                );
            }
            None => self.label(
                frame,
                Alignment::Right,
                "not scheduled".to_owned(),
                palette::MUTED,
            ),
        }
    }

    /// A label above the bar at the left (last) or right (next) edge, so
    /// the two never collide however close their marks are.
    fn label(&self, frame: &mut Frame, align: Alignment, content: String, color: Color) {
        let x = match align {
            Alignment::Left => Self::PAD,
            _ => frame.width() - Self::PAD,
        };
        frame.fill_text(Text {
            content,
            position: Point::new(x, 9.0),
            color,
            size: 12.0.into(),
            align_x: align,
            align_y: Vertical::Center,
            ..Text::default()
        });
    }
}

impl canvas::Program<Message> for TimingBar<'_> {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        TimingBar::draw(self, &mut frame);
        vec![frame.into_geometry()]
    }
}

/// One bar per run, newest on the right: height is the duration, color the
/// outcome. Hovering names the run, clicking opens its logs.
struct RunChart<'a> {
    runs: &'a [Run],
    selected: Option<Timestamp>,
    now: Timestamp,
    tz: TimeZone,
}

impl RunChart<'_> {
    const HEIGHT: f32 = 92.0;
    /// Room above the bars for the hovered run's description.
    const TOP: f32 = 22.0;
    /// Bars widen when few runs are shown, up to `BAR_MAX`.
    const BAR_MIN: f32 = 8.0;
    const BAR_MAX: f32 = 24.0;
    const GAP: f32 = 3.0;

    /// The bar width and the runs that fit, newest last, with each bar's x.
    fn bars(&self, width: f32) -> (f32, Vec<(f32, &Run)>) {
        let fits = ((width + Self::GAP) / (Self::BAR_MIN + Self::GAP)).floor() as usize;
        let shown = &self.runs[self.runs.len().saturating_sub(fits)..];
        let count = shown.len().max(1) as f32;
        let bar = ((width + Self::GAP) / count - Self::GAP).clamp(Self::BAR_MIN, Self::BAR_MAX);
        let left = width - count * (bar + Self::GAP) + Self::GAP;
        let bars = shown
            .iter()
            .enumerate()
            .map(|(index, run)| (left + index as f32 * (bar + Self::GAP), run))
            .collect();
        (bar, bars)
    }

    fn hovered(&self, bounds: Rectangle, cursor: mouse::Cursor) -> Option<&Run> {
        let position = cursor.position_in(bounds)?;
        let (bar, bars) = self.bars(bounds.width);
        bars.into_iter()
            .find(|(x, _)| {
                position.x >= x - Self::GAP / 2.0 && position.x < x + bar + Self::GAP / 2.0
            })
            .map(|(_, run)| run)
    }

    fn duration(&self, run: &Run) -> SignedDuration {
        let end = match run.outcome {
            Outcome::Running => self.now,
            _ => run.end.unwrap_or(run.start),
        };
        end.duration_since(run.start)
    }
}

impl canvas::Program<Message> for RunChart<'_> {
    type State = ();

    fn update(
        &self,
        _state: &mut (),
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        match event {
            canvas::Event::Mouse(mouse::Event::CursorMoved { .. } | mouse::Event::CursorLeft) => {
                Some(canvas::Action::request_redraw())
            }
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let run = self.hovered(bounds, cursor)?;
                Some(canvas::Action::publish(Message::OpenRun(run.start)).and_capture())
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let (bar, bars) = self.bars(bounds.width);
        let hovered = self.hovered(bounds, cursor).map(|run| run.start);
        let longest = bars
            .iter()
            .map(|(_, run)| self.duration(run))
            .max()
            .unwrap_or_default()
            .max(SignedDuration::from_millis(1));
        let room = Self::HEIGHT - Self::TOP;

        frame.stroke(
            &Path::line(
                Point::new(0.0, Self::HEIGHT - 0.5),
                Point::new(bounds.width, Self::HEIGHT - 0.5),
            ),
            Stroke::default().with_color(palette::GRID).with_width(1.0),
        );

        for &(x, run) in &bars {
            let ratio = self.duration(run).as_secs_f64() / longest.as_secs_f64();
            let height = (room * ratio as f32).max(3.0);
            let shape = Path::rounded_rectangle(
                Point::new(x, Self::HEIGHT - height),
                Size::new(bar, height),
                2.0.into(),
            );
            let color = match run.outcome {
                Outcome::Success => palette::NOTE,
                Outcome::Failed(_) => palette::FAILED,
                Outcome::Running => palette::NOTE.scale_alpha(0.55),
            };
            let emphasis = hovered == Some(run.start) || self.selected == Some(run.start);
            frame.fill(
                &shape,
                if emphasis || hovered.is_none() {
                    color
                } else {
                    color.scale_alpha(0.7)
                },
            );
            if emphasis {
                frame.stroke(
                    &shape,
                    Stroke::default().with_color(palette::TEXT).with_width(1.0),
                );
            }
        }

        let (content, color) =
            match hovered.and_then(|start| self.runs.iter().find(|run| run.start == start)) {
                Some(run) => {
                    let outcome = match &run.outcome {
                        Outcome::Success => "success".to_owned(),
                        Outcome::Running => "running".to_owned(),
                        Outcome::Failed(result) => format!("failed: {result}"),
                    };
                    (
                        format!(
                            "{} · {} · {outcome}",
                            label::moment(run.start, &self.tz),
                            duration_label(self.duration(run))
                        ),
                        palette::TEXT,
                    )
                }
                None => (
                    format!(
                        "last {} · longest {}",
                        bars.len(),
                        timespan::format(longest)
                    ),
                    palette::MUTED,
                ),
            };
        frame.fill_text(Text {
            content,
            position: Point::new(0.0, 8.0),
            color,
            size: 12.0.into(),
            align_y: Vertical::Center,
            ..Text::default()
        });

        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        _state: &(),
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if self.hovered(bounds, cursor).is_some() {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::default()
        }
    }
}

/// `1.2s`, or a note when systemd logged no end for the run.
fn duration_label(duration: SignedDuration) -> String {
    if duration.is_zero() {
        "duration not logged".to_owned()
    } else {
        timespan::format(duration)
    }
}

fn unit_file_help(state: &str) -> &'static str {
    match state {
        "enabled" | "enabled-runtime" => {
            "Enabled: started automatically at boot, or at login for user timers."
        }
        "disabled" => "Disabled: installed, but only runs when started by hand.",
        "static" => {
            "Static: has no [Install] section, so it cannot be enabled; another unit pulls it in."
        }
        "masked" | "masked-runtime" => "Masked: systemd refuses to start it.",
        "transient" => "Transient: created at runtime, e.g. by systemd-run; gone after a reboot.",
        "generated" => "Generated: created by a generator from other configuration.",
        "indirect" => "Indirect: enabled through another unit's Also=.",
        "linked" | "linked-runtime" => "Linked: the unit file lives outside the search path.",
        "alias" => "Alias: another name for a different unit.",
        _ => "The unit file state systemd reports.",
    }
}

fn card<'a>(title: &'a str, body: Element<'a, Message>) -> container::Container<'a, Message> {
    container(
        column![
            text(title.to_uppercase()).size(11).color(palette::MUTED),
            scrollable(body).height(Length::Fill)
        ]
        .spacing(10),
    )
    .padding(12)
    .height(Length::Fill)
    .style(|_| container::Style {
        border: Border {
            color: palette::GRID,
            width: 1.0,
            radius: 8.0.into(),
        },
        ..container::Style::default()
    })
}

fn dot<'a>(color: Color) -> Element<'a, Message> {
    container(text(""))
        .width(8)
        .height(8)
        .style(move |_| container::Style {
            background: Some(color.into()),
            border: Border {
                radius: 4.0.into(),
                ..Border::default()
            },
            ..container::Style::default()
        })
        .into()
}

fn badge<'a>(name: &str, help: &'a str) -> Element<'a, Message> {
    let pill = container(text(name.to_owned()).size(11).color(palette::MUTED))
        .padding([2, 8])
        .style(|_| container::Style {
            border: Border {
                color: palette::GRID_MAJOR,
                width: 1.0,
                radius: 9.0.into(),
            },
            ..container::Style::default()
        });
    hint(pill, help)
}

fn info_icon<'a>() -> Element<'a, Message> {
    container(text("i").size(10).color(palette::MUTED))
        .width(16)
        .height(16)
        .align_x(Alignment::Center)
        .align_y(Vertical::Center)
        .style(|_| container::Style {
            border: Border {
                color: palette::FAINT,
                width: 1.0,
                radius: 8.0.into(),
            },
            ..container::Style::default()
        })
        .into()
}

fn hint<'a>(content: impl Into<Element<'a, Message>>, help: &'a str) -> Element<'a, Message> {
    tip(content, text(help).size(12).into())
}

fn tip<'a>(
    content: impl Into<Element<'a, Message>>,
    help: Element<'a, Message>,
) -> Element<'a, Message> {
    tooltip(
        content,
        container(help)
            .padding(10)
            .max_width(340)
            .style(|_| container::Style {
                background: Some(palette::GRID.into()),
                border: Border {
                    color: palette::GRID_MAJOR,
                    width: 1.0,
                    radius: 8.0.into(),
                },
                ..container::Style::default()
            }),
        tooltip::Position::Bottom,
    )
    .gap(6)
    .into()
}

fn note<'a>(color: Color, content: String) -> Element<'a, Message> {
    row![
        container(text(""))
            .width(10)
            .height(10)
            .style(move |_| container::Style {
                background: Some(color.into()),
                border: Border {
                    radius: 3.0.into(),
                    ..Border::default()
                },
                ..container::Style::default()
            }),
        text(content).size(12).color(palette::MUTED),
    ]
    .spacing(8)
    .align_y(Vertical::Center)
    .into()
}

fn stat<'a>(content: String, color: Color) -> Element<'a, Message> {
    text(content).size(12).color(color).into()
}

fn muted(message: &str) -> Element<'_, Message> {
    text(message).size(13).color(palette::MUTED).into()
}
