//! Composing a new timer in the drawer, while a draft track in the music
//! box shows its elapses among the existing timers. The schedule is painted
//! on the rhythm machine rather than typed; a card shows what systemd makes
//! of it. systemd itself expands the schedule, so the draft matches what
//! the timer will do.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use iced::widget::canvas::Canvas;
use iced::widget::{
    Column, Row, button, checkbox, column, container, mouse_area, row, scrollable, slider, space,
    stack, text, text_input,
};
use iced::{Border, Color, Element, Font, Length};
use jiff::tz::TimeZone;
use jiff::{SignedDuration, Timestamp};

use super::rhythm::{self, Division, Edit, Hint, Month, Preset, Week, WeekGrid};
use super::{Message, controls, explain, label, musicbox, palette};
use crate::i18n::fl;
use crate::model::{CalendarTrigger, Scope, Timer};
use crate::systemd::calendar::Preview;
use crate::systemd::{NewTimer, Problem};

/// How many upcoming elapses are listed beside the form.
const UPCOMING: usize = 5;
/// The summary and the systemd card take at least this height, room for
/// what they usually say, so the form below does not jump while it is
/// filled in. Only unusually long text makes them grow.
const TOP_HEIGHT: f32 = 128.0;
/// Room for the upcoming list at its longest, for the same reason.
const UPCOMING_HEIGHT: f32 = 200.0;

#[derive(Debug, Clone)]
pub enum Field {
    Name(String),
    Description(String),
    Command(String),
    /// Expressions typed by hand, separated by `;`.
    Calendar(String),
    Persistent(bool),
}

/// How the schedule is put together.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Week,
    Month,
    /// `OnCalendar=` expressions typed by hand.
    Advanced,
}

/// What systemd made of the schedule so far.
#[derive(Debug, Clone)]
pub enum Schedule {
    Empty,
    Checking,
    /// One preview per expression, in order.
    Ready(Vec<Preview>),
    /// systemd's reason for rejecting it.
    Invalid(String),
}

#[derive(Debug, Clone)]
pub struct Composer {
    name: String,
    description: String,
    command: String,
    mode: Mode,
    week: Week,
    month: Month,
    calendar: String,
    persistent: bool,
    /// Whether the host is this machine, which decides how root is asked for.
    local: bool,
    pub schedule: Schedule,
    /// Counts schedule changes; shared with pending previews, so one that
    /// waited out a typing pause can tell it is no longer wanted.
    revision: Arc<AtomicU64>,
    show_files: bool,
    /// What the pointer is on in the schedule editor, for the hint line.
    hint: Option<Hint>,
    pub creating: bool,
    pub error: Option<String>,
}

impl Composer {
    pub fn new(local: bool) -> Self {
        Self {
            name: String::new(),
            description: String::new(),
            command: String::new(),
            mode: Mode::Week,
            week: Week::default(),
            month: Month::default(),
            calendar: String::new(),
            persistent: true,
            local,
            schedule: Schedule::Empty,
            revision: Arc::new(AtomicU64::new(0)),
            show_files: false,
            hint: None,
            creating: false,
            error: None,
        }
    }

    /// Applies an edit; true when the schedule changed and needs a preview.
    pub fn update(&mut self, field: Field) -> bool {
        self.error = None;
        match field {
            Field::Name(value) => self.name = value,
            Field::Description(value) => self.description = value,
            Field::Command(value) => self.command = value,
            Field::Persistent(value) => self.persistent = value,
            Field::Calendar(value) => {
                self.calendar = value;
                return true;
            }
        }
        false
    }

    /// Applies a change to the rhythm machine. It always changes the schedule.
    pub fn edit(&mut self, edit: Edit) {
        self.error = None;
        match edit {
            Edit::Paint { day, hour, on } => {
                self.week.set(day, hour, on);
                self.hint = Some(Hint::Cell(day, hour));
            }
            Edit::ToggleDay(day) => self.week.toggle_day(day),
            Edit::ToggleHour(hour) => self.week.toggle_hour(hour),
            Edit::Minute(minute) => self.week.set_minute(minute),
            Edit::Division(division) => self.week.set_division(division),
            Edit::Preset(preset) => self.week = Week::preset(preset),
            Edit::MonthDay(day) => self.month.toggle_day(day),
            Edit::MonthLast => self.month.toggle_last(),
            Edit::MonthHour(hour) => self.month.toggle_hour(hour),
            Edit::MonthMinute(minute) => self.month.set_minute(minute),
        }
    }

    /// Switches how the schedule is made. Typing by hand starts from what
    /// the grid made, so a painted rhythm can be refined.
    pub fn set_mode(&mut self, mode: Mode) {
        if mode == Mode::Advanced && self.calendar.trim().is_empty() {
            self.calendar = self.expressions().join("; ");
        }
        self.mode = mode;
        self.error = None;
    }

    pub fn set_hint(&mut self, hint: Option<Hint>) {
        self.hint = hint;
    }

    // Only the Linux test that opens the composer from the header needs it.
    #[cfg(all(test, target_os = "linux"))]
    pub fn revision_for_test(&self) -> u64 {
        self.revision.load(Ordering::SeqCst)
    }

    #[cfg(test)]
    pub fn update_rhythm_for_test(&mut self) {
        self.edit(Edit::Preset(Preset::Nightly));
    }

    pub fn toggle_files(&mut self) {
        self.show_files = !self.show_files;
    }

    /// The `OnCalendar=` expressions the current mode makes.
    pub fn expressions(&self) -> Vec<String> {
        match self.mode {
            Mode::Week => self.week.expressions(),
            Mode::Month => self.month.expressions(),
            Mode::Advanced => self
                .calendar
                .split(';')
                .map(str::trim)
                .filter(|expression| !expression.is_empty())
                .map(str::to_owned)
                .collect(),
        }
    }

    /// Starts a new revision of the schedule and returns it, with a handle
    /// that tells whether it is still the latest.
    pub fn next_revision(&mut self) -> (u64, Arc<AtomicU64>) {
        let revision = self.revision.fetch_add(1, Ordering::SeqCst) + 1;
        (revision, self.revision.clone())
    }

    pub fn is_current(&self, revision: u64) -> bool {
        self.revision.load(Ordering::SeqCst) == revision
    }

    /// The timer the form describes, or what to fix first.
    pub fn timer(&self) -> Result<NewTimer, String> {
        NewTimer::new(
            &self.name,
            &self.description,
            &self.command,
            &self.expressions(),
            self.persistent,
        )
        .map_err(|problem| match problem {
            Problem::NameEmpty => fl!("timer-form-error-name-empty"),
            Problem::NameInvalid => fl!("timer-form-error-name"),
            Problem::CommandEmpty => fl!("timer-form-error-command"),
            Problem::CalendarEmpty => fl!("timer-form-error-calendar"),
            Problem::LineBreak => fl!("timer-form-error-line"),
        })
    }

    /// The name on the draft track, before the form is complete.
    pub fn title(&self) -> String {
        match self.name.trim() {
            "" => fl!("composer-untitled"),
            name => name.to_owned(),
        }
    }

    /// One line under the draft's title: the schedule in words, or what
    /// is wrong with it.
    pub fn subtitle(&self) -> String {
        match &self.schedule {
            Schedule::Empty => fl!("composer-no-schedule"),
            Schedule::Checking => fl!("composer-checking"),
            // Short like the other tracks' schedules.
            Schedule::Ready(previews) => previews
                .iter()
                .map(|preview| label::calendar(&preview.normalized))
                .collect::<Vec<_>>()
                .join("; "),
            Schedule::Invalid(_) => fl!("composer-invalid"),
        }
    }

    /// The timer as the music box will show it once created: active, never
    /// run, with the elapses systemd computed. `None` until the schedule
    /// is understood.
    pub fn draft(&self, now: Timestamp) -> Option<Timer> {
        let Schedule::Ready(previews) = &self.schedule else {
            return None;
        };
        let name = match self.name.trim() {
            "" => "new",
            name => name,
        };
        let next_elapse = previews
            .iter()
            .flat_map(|preview| preview.occurrences.iter().copied())
            .filter(|&at| at > now)
            .min();
        Some(Timer {
            id: format!("{name}.timer"),
            description: self.description.trim().to_owned(),
            service: format!("{name}.service"),
            calendar: previews
                .iter()
                .map(|preview| CalendarTrigger {
                    expression: preview.normalized.clone(),
                    occurrences: preview.occurrences.clone(),
                })
                .collect(),
            next_elapse,
            persistent: self.persistent,
            // systemd's default AccuracySec=.
            accuracy: SignedDuration::from_mins(1),
            ..placeholder()
        })
    }

    pub fn view(&self, scope: Scope, now: Timestamp, tz: &TimeZone) -> Element<'_, Message> {
        let title = row![
            text(fl!("timer-form-title")).size(18),
            space::horizontal(),
            controls::icon("✕", Message::Composer(ComposerMessage::Cancel)),
        ]
        .align_y(iced::Alignment::Center);

        let top = row![
            container(self.summary(scope, now, tz)).width(Length::FillPortion(3)),
            container(self.card()).width(Length::FillPortion(2)),
        ]
        .spacing(16);

        let body = column![
            top,
            step(
                1,
                fl!("composer-step-what"),
                fl!("composer-step-what-hint"),
                self.what()
            ),
            step(
                2,
                fl!("composer-step-when"),
                fl!("composer-step-when-hint"),
                row![
                    container(self.schedule_editor(now, tz)).width(Length::FillPortion(3)),
                    container(self.when(now, tz))
                        .width(Length::FillPortion(1))
                        .height(UPCOMING_HEIGHT),
                ]
                .spacing(24)
                .into(),
            ),
            step(
                3,
                fl!("composer-step-how"),
                fl!("composer-step-how-hint"),
                self.install(scope)
            ),
            step(
                4,
                fl!("composer-step-create"),
                fl!("composer-step-create-hint"),
                self.finish()
            ),
        ]
        .spacing(22);

        container(column![title, scrollable(body).spacing(6)].spacing(14))
            .width(Length::Fill)
            .height(Length::Fill)
            // With the border, content starts where the music box's labels do.
            .padding([16.0, musicbox::PAD - 1.0])
            .style(|_| container::Style {
                background: Some(palette::SURFACE.into()),
                border: Border {
                    color: palette::DRAFT.scale_alpha(0.5),
                    width: 1.0,
                    radius: 10.0.into(),
                },
                ..container::Style::default()
            })
            .into()
    }

    /// Everything the form says so far, as a few plain sentences, with
    /// what is still missing in their place.
    fn summary(&self, scope: Scope, now: Timestamp, tz: &TimeZone) -> Element<'_, Message> {
        let mut parts = vec![match self.command.trim() {
            "" => fl!("composer-summary-no-command", name = self.title()),
            command => fl!(
                "composer-summary-command",
                name = self.title(),
                command = shorten(command, 48)
            ),
        }];
        parts.push(match &self.schedule {
            Schedule::Empty => fl!("composer-summary-no-schedule"),
            Schedule::Checking => fl!("composer-summary-checking"),
            Schedule::Invalid(_) => fl!("composer-summary-invalid"),
            Schedule::Ready(previews) => sentences(previews)
                .iter()
                .map(|sentence| format!("{sentence}."))
                .collect::<Vec<_>>()
                .join(" "),
        });
        parts.push(match scope {
            Scope::System => fl!("composer-summary-system"),
            Scope::User => fl!("composer-summary-user"),
        });
        if self.persistent {
            parts.push(fl!("composer-summary-persistent"));
        }

        let next = match &self.schedule {
            Schedule::Ready(previews) => Some(
                previews
                    .iter()
                    .flat_map(|preview| preview.occurrences.iter().copied())
                    .filter(|&at| at > now)
                    .min(),
            ),
            _ => None,
        };
        let mut summary = column![
            caption(fl!("composer-summary-title")),
            text(parts.join(" ")).size(14),
        ]
        .spacing(6);
        summary = match next {
            Some(Some(at)) => summary.push(
                text(fl!(
                    "composer-summary-next",
                    moment = label::moment(at, tz),
                    relative = label::relative(at, now)
                ))
                .size(13)
                .color(palette::DRAFT),
            ),
            Some(None) => summary.push(
                text(fl!("composer-none-in-view"))
                    .size(12)
                    .color(palette::MUTED),
            ),
            None => summary,
        };
        container(at_least(TOP_HEIGHT - 24.0, summary))
            .width(Length::Fill)
            .padding(12)
            .style(|_| container::Style {
                background: Some(palette::DRAFT.scale_alpha(0.06).into()),
                border: Border {
                    color: palette::DRAFT.scale_alpha(0.35),
                    width: 1.0,
                    radius: 8.0.into(),
                },
                ..container::Style::default()
            })
            .into()
    }

    /// The unit's name, description and command.
    fn what(&self) -> Element<'_, Message> {
        let input = |placeholder: String, value: &str, field: fn(String) -> Field| {
            text_input(&placeholder, value)
                .id(field_id(field))
                .on_input(move |value| Message::Composer(ComposerMessage::Edit(field(value))))
                .on_submit(Message::Composer(ComposerMessage::Create))
                .size(13)
                .padding([6, 8])
        };
        let labelled =
            |title: String, widget: Element<'static, Message>| -> Element<'static, Message> {
                column![caption(title), widget].spacing(4).into()
            };
        column![
            row![
                container(labelled(
                    fl!("timer-form-name"),
                    input(fl!("timer-form-name-placeholder"), &self.name, Field::Name).into(),
                ))
                .width(Length::FillPortion(2)),
                container(labelled(
                    fl!("timer-form-description"),
                    input(
                        fl!("timer-form-description-placeholder"),
                        &self.description,
                        Field::Description
                    )
                    .into(),
                ))
                .width(Length::FillPortion(3)),
                container(labelled(
                    fl!("timer-form-command"),
                    input(
                        fl!("timer-form-command-placeholder"),
                        &self.command,
                        Field::Command
                    )
                    .font(Font::MONOSPACE)
                    .into(),
                ))
                .width(Length::FillPortion(5)),
            ]
            .spacing(10),
            text(fl!("timer-form-command-hint"))
                .size(11)
                .color(palette::FAINT),
        ]
        .spacing(6)
        .into()
    }

    fn schedule_editor(&self, now: Timestamp, tz: &TimeZone) -> Element<'_, Message> {
        let modes = hover(
            controls::segmented(
                [
                    (Mode::Week, fl!("rhythm-mode-week")),
                    (Mode::Month, fl!("rhythm-mode-month")),
                    (Mode::Advanced, fl!("rhythm-mode-advanced")),
                ],
                self.mode,
                |mode| Message::Composer(ComposerMessage::Mode(mode)),
            ),
            Hint::Modes,
        );
        let editor = match self.mode {
            Mode::Week => self.week_editor(now, tz),
            Mode::Month => self.month_editor(),
            Mode::Advanced => text_input(&fl!("timer-form-calendar-placeholder"), &self.calendar)
                .id(field_id(Field::Calendar))
                .on_input(|value| Message::Composer(ComposerMessage::Edit(Field::Calendar(value))))
                .font(Font::MONOSPACE)
                .size(13)
                .padding([6, 8])
                .into(),
        };
        column![
            modes,
            editor,
            // The explanation of whatever the pointer is on.
            row![
                text("→").size(12).color(palette::DRAFT),
                text(self.hint_text()).size(12).color(palette::MUTED),
            ]
            .spacing(8)
            .height(34),
        ]
        .spacing(10)
        .into()
    }

    /// What the hovered part does, else how this mode works.
    fn hint_text(&self) -> String {
        let week = &self.week;
        match self.hint {
            Some(Hint::Cell(day, hour)) => {
                let times = rhythm::run_times(hour, week.minute(), week.division);
                if week.is_on(day, hour) {
                    fl!(
                        "rhythm-hint-cell-on",
                        days = rhythm::days_name(day),
                        times = times
                    )
                } else {
                    fl!(
                        "rhythm-hint-cell-off",
                        days = rhythm::days_name(day),
                        times = times
                    )
                }
            }
            Some(Hint::Day(day)) => fl!("rhythm-hint-day", days = rhythm::days_name(day)),
            Some(Hint::Hour(hour)) => fl!("rhythm-hint-hour", hour = format!("{hour:02}:00")),
            Some(Hint::Divisions) => fl!(
                "rhythm-hint-divisions",
                times = rhythm::run_times(9, week.minute(), week.division)
            ),
            Some(Hint::Minute) => fl!(
                "rhythm-hint-minute",
                times = rhythm::run_times(9, week.minute(), week.division)
            ),
            Some(Hint::Preset(preset)) => fl!(
                "rhythm-hint-preset",
                schedule = Week::preset(preset)
                    .expressions()
                    .iter()
                    .map(|expression| sentence(expression))
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
            Some(Hint::Modes) => fl!("rhythm-hint-modes"),
            Some(Hint::MonthDay(day)) if day >= 28 => {
                let day = day as u32 + 1;
                fl!("rhythm-hint-month-day-late", day = day)
            }
            Some(Hint::MonthDay(day)) => {
                let day = day as u32 + 1;
                fl!("rhythm-hint-month-day", day = day)
            }
            Some(Hint::LastDay) => fl!("rhythm-hint-last-day"),
            Some(Hint::MonthHour(hour)) => fl!(
                "rhythm-hint-month-hour",
                time = format!("{hour:02}:{:02}", self.month.minute())
            ),
            Some(Hint::MonthMinute) => fl!("rhythm-hint-month-minute"),
            None => match self.mode {
                Mode::Week => fl!("rhythm-grid-hint"),
                Mode::Month => fl!("rhythm-month-hint"),
                Mode::Advanced => fl!("rhythm-advanced-hint"),
            },
        }
    }

    fn week_editor(&self, now: Timestamp, tz: &TimeZone) -> Element<'_, Message> {
        let presets = Row::from_iter(Preset::ALL.map(|preset| {
            hover(
                controls::link(preset.label(), rhythm_message(Edit::Preset(preset))),
                Hint::Preset(preset),
            )
        }))
        .spacing(6)
        .align_y(iced::Alignment::Center);

        let here = now.to_zoned(tz.clone());
        let grid = Canvas::new(WeekGrid {
            week: &self.week,
            now: Some((rhythm::day_index(here.weekday()), here.hour() as usize)),
        })
        .width(Length::Fill)
        .height(rhythm::HEIGHT);

        let division = self.week.division;
        let step = division.step();
        let divisions = hover(
            controls::segmented(
                Division::ALL.map(|division| (division, format!("{}×", division.times()))),
                division,
                |division| rhythm_message(Edit::Division(division)),
            ),
            Hint::Divisions,
        );
        let minute = self.week.minute();
        let timing = row![
            caption(fl!("rhythm-per-hour")),
            divisions,
            caption(fl!("rhythm-minute")),
            hover(
                slider(0..=step - 1, minute, |minute| rhythm_message(Edit::Minute(
                    minute
                )))
                .style(draft_slider)
                .width(Length::Fill),
                Hint::Minute,
            ),
            text(format!(":{minute:02}")).size(12).font(Font::MONOSPACE),
        ]
        .spacing(10)
        .align_y(iced::Alignment::Center);

        column![
            row![caption(fl!("rhythm-presets")), presets]
                .spacing(10)
                .align_y(iced::Alignment::Center),
            grid,
            timing,
        ]
        .spacing(10)
        .into()
    }

    fn month_editor(&self) -> Element<'_, Message> {
        let mut days = Column::new().spacing(4);
        for week in 0..5 {
            let mut line = Row::new().spacing(4);
            for day in week * 7..(week * 7 + 7).min(31) {
                line = line.push(hover(
                    toggle(
                        format!("{}", day + 1),
                        self.month.is_day(day),
                        Edit::MonthDay(day),
                        36.0,
                    ),
                    Hint::MonthDay(day),
                ));
            }
            if week == 4 {
                line = line.push(hover(
                    toggle(
                        fl!("rhythm-last-day"),
                        self.month.is_last(),
                        Edit::MonthLast,
                        116.0,
                    ),
                    Hint::LastDay,
                ));
            }
            days = days.push(line);
        }
        let hours = Row::from_iter((0..24).map(|hour| {
            hover(
                toggle(
                    format!("{hour:02}"),
                    self.month.is_hour(hour),
                    Edit::MonthHour(hour),
                    28.0,
                ),
                Hint::MonthHour(hour),
            )
        }))
        .spacing(2)
        .wrap();
        let minute = self.month.minute();
        column![
            caption(fl!("rhythm-month-days")),
            days,
            caption(fl!("rhythm-month-hours")),
            hours,
            row![
                caption(fl!("rhythm-minute")),
                hover(
                    slider(0..=59, minute, |minute| rhythm_message(Edit::MonthMinute(
                        minute
                    )))
                    .style(draft_slider)
                    .width(Length::Fill),
                    Hint::MonthMinute,
                ),
                text(format!(":{minute:02}")).size(12).font(Font::MONOSPACE),
            ]
            .spacing(10)
            .align_y(iced::Alignment::Center),
        ]
        .spacing(8)
        .into()
    }

    /// What systemd will read: the `OnCalendar=` lines and their meaning.
    fn card(&self) -> Element<'_, Message> {
        let expressions = self.expressions();
        let mut card = Column::new().spacing(6).push(
            row![
                caption(fl!("rhythm-card-title")),
                controls::info(fl!("info-oncalendar")),
            ]
            .spacing(6)
            .align_y(iced::Alignment::Center),
        );
        if expressions.is_empty() {
            card = card.push(
                text(fl!("composer-no-schedule"))
                    .size(12)
                    .color(palette::FAINT),
            );
        }
        for expression in &expressions {
            card = card.push(
                text(format!("OnCalendar={expression}"))
                    .size(12)
                    .font(Font::MONOSPACE)
                    .color(palette::DRAFT),
            );
        }
        if let Schedule::Invalid(reason) = &self.schedule {
            card = card.push(text(reason.clone()).size(12).color(palette::FAILED));
        }
        container(at_least(TOP_HEIGHT - 24.0, card))
            .width(Length::Fill)
            .padding(12)
            .style(|_| container::Style {
                background: Some(palette::BACKGROUND.into()),
                border: Border {
                    color: palette::GRID_MAJOR,
                    width: 1.0,
                    radius: 8.0.into(),
                },
                ..container::Style::default()
            })
            .into()
    }

    /// The next elapses, as the draft track has them.
    fn when(&self, now: Timestamp, tz: &TimeZone) -> Element<'_, Message> {
        let when = Column::new().spacing(6).push(caption(fl!("composer-when")));
        let Schedule::Ready(previews) = &self.schedule else {
            return when.push(text("—").size(12).color(palette::FAINT)).into();
        };
        let mut upcoming: Vec<Timestamp> = previews
            .iter()
            .flat_map(|preview| preview.occurrences.iter().copied())
            .filter(|&at| at > now)
            .collect();
        upcoming.sort();
        upcoming.dedup();
        if upcoming.is_empty() {
            return when
                .push(
                    text(fl!("composer-none-in-view"))
                        .size(12)
                        .color(palette::MUTED),
                )
                .into();
        }
        upcoming
            .into_iter()
            .take(UPCOMING)
            .fold(when, |when, at| {
                when.push(
                    column![
                        row![
                            text("◆").size(10).color(palette::DRAFT),
                            text(label::moment(at, tz)).size(12),
                        ]
                        .spacing(8)
                        .align_y(iced::Alignment::Center),
                        text(label::relative(at, now))
                            .size(11)
                            .color(palette::MUTED),
                    ]
                    .spacing(1),
                )
            })
            .into()
    }

    /// Missed runs and where the timer goes, each with what it means.
    fn install(&self, scope: Scope) -> Element<'_, Message> {
        let note = match (scope, self.local) {
            (Scope::User, _) => fl!("timer-form-note-user"),
            (Scope::System, true) => fl!("timer-form-note-system-local"),
            (Scope::System, false) => fl!("timer-form-note-system-remote"),
        };
        let missed = if self.persistent {
            fl!("composer-persistent-on")
        } else {
            fl!("composer-persistent-off")
        };
        let persistent = column![
            row![
                checkbox(self.persistent)
                    .label(fl!("timer-form-persistent"))
                    .on_toggle(|value| {
                        Message::Composer(ComposerMessage::Edit(Field::Persistent(value)))
                    })
                    .size(14)
                    .text_size(13),
                controls::info(fl!("info-persistent")),
            ]
            .spacing(6)
            .align_y(iced::Alignment::Center),
            text(missed).size(11).color(palette::FAINT),
        ]
        .spacing(6);
        let place = column![
            row![
                caption(fl!("timer-form-scope")),
                controls::segmented(
                    [
                        (Scope::System, fl!("scope-system")),
                        (Scope::User, fl!("scope-user")),
                    ],
                    scope,
                    Message::SetScope,
                ),
                controls::info(fl!("info-scope")),
            ]
            .spacing(10)
            .align_y(iced::Alignment::Center),
            text(note).size(11).color(palette::FAINT),
        ]
        .spacing(6);
        row![
            container(persistent).width(Length::Fill),
            container(place).width(Length::Fill),
        ]
        .spacing(24)
        .into()
    }

    /// The unit files on request, the outcome, and the buttons.
    fn finish(&self) -> Element<'_, Message> {
        let mut finish = Column::new().spacing(10);
        if self.show_files
            && let Ok(timer) = self.timer()
        {
            let file = |name: String, content: String| {
                column![
                    caption(name),
                    container(text(content).size(11).font(Font::MONOSPACE))
                        .width(Length::Fill)
                        .padding(8)
                        .style(|_| container::Style {
                            background: Some(palette::BACKGROUND.into()),
                            border: Border {
                                color: palette::GRID_MAJOR,
                                width: 1.0,
                                radius: 6.0.into(),
                            },
                            ..container::Style::default()
                        }),
                ]
                .spacing(4)
                .width(Length::Fill)
            };
            finish = finish.push(
                row![
                    file(timer.service_name(), timer.service_file()),
                    file(timer.timer_name(), timer.timer_file()),
                ]
                .spacing(12),
            );
        }

        let files = controls::link(
            if self.show_files {
                fl!("composer-hide-files")
            } else {
                fl!("composer-show-files")
            },
            Message::Composer(ComposerMessage::ToggleFiles),
        );
        let create = if self.creating {
            controls::action(fl!("timer-form-creating"), None)
        } else {
            controls::primary(
                fl!("timer-form-create"),
                Message::Composer(ComposerMessage::Create),
            )
        };
        finish
            .push(
                row![
                    files,
                    text(self.error.clone().unwrap_or_default())
                        .size(12)
                        .color(palette::FAILED),
                    space::horizontal(),
                    controls::action(
                        fl!("form-cancel"),
                        Some(Message::Composer(ComposerMessage::Cancel)),
                    ),
                    create,
                ]
                .spacing(10)
                .align_y(iced::Alignment::Center),
            )
            .into()
    }
}

/// A numbered part of the form: a title, one line on what it is for, and
/// its fields.
fn step<'a>(
    number: u8,
    title: String,
    purpose: String,
    content: Element<'a, Message>,
) -> Element<'a, Message> {
    let badge = container(text(number.to_string()).size(12).color(Color::BLACK))
        .center_x(22)
        .center_y(22)
        .style(|_| container::Style {
            background: Some(palette::DRAFT.into()),
            border: Border {
                radius: 11.0.into(),
                ..Border::default()
            },
            ..container::Style::default()
        });
    column![
        row![
            badge,
            column![
                text(title).size(15),
                text(purpose).size(12).color(palette::MUTED),
            ]
            .spacing(2),
        ]
        .spacing(10)
        .align_y(iced::Alignment::Center),
        container(content).padding(iced::Padding {
            left: 32.0,
            ..iced::Padding::ZERO
        }),
    ]
    .spacing(10)
    .into()
}

/// Reports the pointer entering and leaving `content` to the hint line.
fn hover<'a>(content: impl Into<Element<'a, Message>>, hint: Hint) -> Element<'a, Message> {
    mouse_area(content)
        .on_enter(Message::Composer(ComposerMessage::Hint(Some(hint))))
        .on_exit(Message::Composer(ComposerMessage::Hint(None)))
        .into()
}

/// A stable identity for each text field, so tests and focus operations
/// can find it.
pub fn field_id(field: fn(String) -> Field) -> &'static str {
    match field(String::new()) {
        Field::Name(_) => "composer-name",
        Field::Description(_) => "composer-description",
        Field::Command(_) => "composer-command",
        Field::Calendar(_) => "composer-calendar",
        Field::Persistent(_) => "composer-persistent",
    }
}

/// `content`, taking at least `height` even when it needs less.
fn at_least<'a>(height: f32, content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    let content: Element<'a, Message> = content.into();
    // The first layer sets the stack's size: as wide as the content may be.
    stack![space::horizontal().height(height), content].into()
}

/// Cuts `text` to `limit` characters, marking the cut.
fn shorten(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        text.to_owned()
    } else {
        let kept: String = text.chars().take(limit - 1).collect();
        format!("{kept}…")
    }
}

/// The previews in words, one sentence per expression.
fn sentences(previews: &[Preview]) -> Vec<String> {
    previews
        .iter()
        .map(|preview| sentence(&preview.normalized))
        .collect()
}

/// `On weekdays at 09:00`; systemd's short form when there is no sentence.
fn sentence(expression: &str) -> String {
    explain::calendar(expression).unwrap_or_else(|| label::calendar(expression))
}

fn caption<'a>(title: String) -> Element<'a, Message> {
    text(title).size(11).color(palette::MUTED).into()
}

fn rhythm_message(edit: Edit) -> Message {
    Message::Composer(ComposerMessage::Rhythm(edit))
}

/// Sliders in the composer's green rather than the theme's.
fn draft_slider(_theme: &iced::Theme, status: slider::Status) -> slider::Style {
    let color = match status {
        slider::Status::Active => palette::DRAFT,
        slider::Status::Hovered | slider::Status::Dragged => palette::DRAFT.scale_alpha(0.8),
    };
    slider::Style {
        rail: slider::Rail {
            backgrounds: (color.into(), palette::GRID_MAJOR.into()),
            width: 4.0,
            border: Border {
                radius: 2.0.into(),
                ..Border::default()
            },
        },
        handle: slider::Handle {
            shape: slider::HandleShape::Circle { radius: 7.0 },
            background: color.into(),
            border_width: 0.0,
            border_color: Color::TRANSPARENT,
        },
    }
}

/// A square that is lit while chosen, like a cell of the grid.
fn toggle<'a>(label: String, on: bool, edit: Edit, width: f32) -> Element<'a, Message> {
    button(text(label).size(11).center().width(Length::Fill))
        .width(width)
        .padding([4, 0])
        .style(move |_, status| {
            let (background, color) = if on {
                (palette::DRAFT, Color::BLACK)
            } else {
                (palette::GRID, palette::MUTED)
            };
            button::Style {
                background: Some(
                    match status {
                        button::Status::Hovered => background.scale_alpha(0.8),
                        _ => background,
                    }
                    .into(),
                ),
                text_color: color,
                border: Border {
                    radius: 3.0.into(),
                    ..Border::default()
                },
                ..button::Style::default()
            }
        })
        .on_press(rhythm_message(edit))
        .into()
}

/// The draft track's timer before systemd understood the schedule: no
/// elapses to draw yet.
pub fn placeholder() -> Timer {
    Timer {
        id: "new.timer".to_owned(),
        description: String::new(),
        service: "new.service".to_owned(),
        service_active: false,
        active: true,
        unit_file_state: String::new(),
        calendar: Vec::new(),
        monotonic: Vec::new(),
        next_elapse: None,
        last_trigger: None,
        persistent: false,
        wake_system: false,
        accuracy: SignedDuration::ZERO,
        randomized_delay: SignedDuration::ZERO,
        runs: Vec::new(),
        stopped: Vec::new(),
    }
}

#[derive(Debug, Clone)]
pub enum ComposerMessage {
    Edit(Field),
    Rhythm(Edit),
    /// The pointer moved onto, or off, a part of the schedule editor.
    Hint(Option<Hint>),
    Mode(Mode),
    /// systemd's reading of the schedule at the given revision; `None`
    /// when a newer change superseded it before it was asked for.
    Previewed(u64, Option<Result<Vec<Preview>, String>>),
    ToggleFiles,
    Create,
    Created(Result<(), String>),
    Cancel,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ready(occurrences: Vec<Timestamp>) -> Schedule {
        Schedule::Ready(vec![Preview {
            normalized: "*-*-* 03:00:00".to_owned(),
            occurrences,
        }])
    }

    #[test]
    fn the_draft_appears_once_systemd_understood_the_schedule() {
        let _language = crate::i18n::test_language("en");
        let now = Timestamp::from_second(1_800_000_000).unwrap();
        let mut composer = Composer::new(true);
        assert!(!composer.update(Field::Name("backup".to_owned())));
        composer.edit(Edit::Preset(Preset::Nightly));
        assert_eq!(composer.expressions(), ["*-*-* 03:00:00"]);
        assert!(composer.draft(now).is_none());

        let past = now - SignedDuration::from_hours(3);
        let next = now + SignedDuration::from_hours(21);
        composer.schedule = ready(vec![past, next]);
        let draft = composer.draft(now).unwrap();
        assert_eq!(draft.id, "backup.timer");
        assert_eq!(draft.next_elapse, Some(next));
        assert!(draft.active && draft.persistent && draft.runs.is_empty());

        // A timer that never ran is not judged for the past: only what
        // is ahead is drawn.
        let marks = crate::timeline::marks(&draft, &[], true, now);
        assert_eq!(marks.len(), 1);
        assert_eq!(marks[0].anchor(), next);
    }

    #[test]
    fn typing_by_hand_starts_from_the_painted_rhythm() {
        let mut composer = Composer::new(true);
        composer.edit(Edit::Preset(Preset::WorkdayMornings));
        composer.edit(Edit::Paint {
            day: 5,
            hour: 10,
            on: true,
        });
        composer.set_mode(Mode::Advanced);
        assert_eq!(
            composer.expressions(),
            ["Mon..Fri *-*-* 09:00:00", "Sat *-*-* 10:00:00"]
        );
        assert!(composer.update(Field::Calendar("daily ; ".to_owned())));
        assert_eq!(composer.expressions(), ["daily"]);
        // The grid keeps its rhythm for going back.
        composer.set_mode(Mode::Week);
        assert_eq!(composer.expressions().len(), 2);
    }

    #[test]
    fn the_hint_line_explains_what_the_pointer_is_on() {
        let _language = crate::i18n::test_language("en");
        let mut composer = Composer::new(true);
        assert_eq!(composer.hint_text(), fl!("rhythm-grid-hint"));
        composer.edit(Edit::Division(Division::Twice));
        composer.set_hint(Some(Hint::Cell(0, 9)));
        assert_eq!(
            composer.hint_text(),
            "Click to add: runs Mondays at 09:00, 09:30."
        );
        // Painting a cell explains the cell just painted.
        composer.edit(Edit::Paint {
            day: 6,
            hour: 3,
            on: true,
        });
        assert!(
            composer
                .hint_text()
                .starts_with("Painted: runs Sundays at 03:00")
        );
        composer.set_hint(Some(Hint::Preset(Preset::WorkdayMornings)));
        assert!(composer.hint_text().starts_with("Replaces the grid with: "));
    }

    #[test]
    fn only_the_latest_revision_counts() {
        let mut composer = Composer::new(true);
        let (first, _) = composer.next_revision();
        let (second, handle) = composer.next_revision();
        assert!(!composer.is_current(first));
        assert!(composer.is_current(second));
        assert_eq!(handle.load(Ordering::SeqCst), second);
    }

    #[test]
    fn the_draft_track_says_what_is_missing() {
        let _language = crate::i18n::test_language("en");
        let mut composer = Composer::new(false);
        assert_eq!(composer.title(), fl!("composer-untitled"));
        assert_eq!(composer.subtitle(), fl!("composer-no-schedule"));
        composer.schedule = Schedule::Invalid("Failed to parse".to_owned());
        assert_eq!(composer.subtitle(), fl!("composer-invalid"));
        composer.update(Field::Name("  db-dump ".to_owned()));
        assert_eq!(composer.title(), "db-dump");
    }
}
