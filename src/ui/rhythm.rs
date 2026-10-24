//! The rhythm machine: the hours of a week painted on a grid, like steps on
//! a drum machine, and turned into `OnCalendar=` expressions. Days of the
//! month have their own page; anything else is typed by hand.

use iced::alignment::Vertical;
use iced::mouse;
use iced::widget::canvas::{self, Frame, Path, Stroke, Text};
use iced::widget::text::Alignment;
use iced::{Point, Rectangle, Renderer, Size, Theme};
use jiff::civil::Weekday;

use super::composer::ComposerMessage;
use super::{Message, label, palette};
use crate::i18n::fl;

/// systemd's weekday names, Monday first like the grid.
const DAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const WEEKDAYS: [Weekday; 7] = [
    Weekday::Monday,
    Weekday::Tuesday,
    Weekday::Wednesday,
    Weekday::Thursday,
    Weekday::Friday,
    Weekday::Saturday,
    Weekday::Sunday,
];

/// How often a painted hour plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Division {
    Once,
    Twice,
    Four,
    Twelve,
}

impl Division {
    pub const ALL: [Self; 4] = [Self::Once, Self::Twice, Self::Four, Self::Twelve];

    pub fn times(self) -> u8 {
        match self {
            Self::Once => 1,
            Self::Twice => 2,
            Self::Four => 4,
            Self::Twelve => 12,
        }
    }

    /// Minutes between two runs inside the hour.
    pub fn step(self) -> u8 {
        60 / self.times()
    }
}

/// Ready-made rhythms that fill the grid in one click.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    Hourly,
    Quarterly,
    Nightly,
    WorkdayMornings,
    Weekly,
}

impl Preset {
    pub const ALL: [Self; 5] = [
        Self::Hourly,
        Self::Quarterly,
        Self::Nightly,
        Self::WorkdayMornings,
        Self::Weekly,
    ];

    pub fn label(self) -> String {
        match self {
            Self::Hourly => fl!("rhythm-preset-hourly"),
            Self::Quarterly => fl!("rhythm-preset-quarterly"),
            Self::Nightly => fl!("rhythm-preset-nightly"),
            Self::WorkdayMornings => fl!("rhythm-preset-workdays"),
            Self::Weekly => fl!("rhythm-preset-weekly"),
        }
    }
}

/// Hours painted on the days of a week.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Week {
    /// `cells[day][hour]`, Monday first.
    cells: [[bool; 24]; 7],
    /// The first run's minute in every painted hour.
    minute: u8,
    pub division: Division,
}

impl Default for Week {
    fn default() -> Self {
        Self {
            cells: [[false; 24]; 7],
            minute: 0,
            division: Division::Once,
        }
    }
}

impl Week {
    pub fn preset(preset: Preset) -> Self {
        let mut week = Self::default();
        let mut paint = |days: std::ops::Range<usize>, hours: std::ops::Range<usize>| {
            for day in days {
                for hour in hours.clone() {
                    week.cells[day][hour] = true;
                }
            }
        };
        match preset {
            Preset::Hourly | Preset::Quarterly => paint(0..7, 0..24),
            Preset::Nightly => paint(0..7, 3..4),
            Preset::WorkdayMornings => paint(0..5, 9..10),
            Preset::Weekly => paint(6..7, 2..3),
        }
        if preset == Preset::Quarterly {
            week.division = Division::Four;
        }
        week
    }

    pub fn is_on(&self, day: usize, hour: usize) -> bool {
        self.cells[day][hour]
    }

    pub fn set(&mut self, day: usize, hour: usize, on: bool) {
        self.cells[day][hour] = on;
    }

    /// Fills the day, or clears it when it is already full.
    pub fn toggle_day(&mut self, day: usize) {
        let on = !self.cells[day].iter().all(|&cell| cell);
        self.cells[day] = [on; 24];
    }

    /// Fills the hour on every day, or clears it when it is already full.
    pub fn toggle_hour(&mut self, hour: usize) {
        let on = !self.cells.iter().all(|day| day[hour]);
        for day in &mut self.cells {
            day[hour] = on;
        }
    }

    pub fn minute(&self) -> u8 {
        self.minute
    }

    /// The first minute stays inside one step of the division.
    pub fn set_minute(&mut self, minute: u8) {
        self.minute = minute.min(self.division.step() - 1);
    }

    pub fn set_division(&mut self, division: Division) {
        self.division = division;
        self.set_minute(self.minute);
    }

    /// One expression per set of days that share their hours, e.g.
    /// `Mon..Fri *-*-* 09,18:00:00` and `Sat *-*-* 10:00:00`.
    pub fn expressions(&self) -> Vec<String> {
        let mut groups: Vec<([bool; 24], [bool; 7])> = Vec::new();
        for (day, hours) in self.cells.iter().enumerate() {
            if !hours.contains(&true) {
                continue;
            }
            match groups.iter_mut().find(|(shared, _)| shared == hours) {
                Some((_, days)) => days[day] = true,
                None => {
                    let mut days = [false; 7];
                    days[day] = true;
                    groups.push((*hours, days));
                }
            }
        }
        let minute = minutes(self.minute, self.division);
        groups
            .iter()
            .map(|(hours, days)| {
                let days = if days.contains(&false) {
                    format!("{} ", list(days, |day| DAYS[day].to_owned()))
                } else {
                    String::new()
                };
                format!("{days}*-*-* {}:{minute}:00", hour_list(hours))
            })
            .collect()
    }
}

/// Days of the month and the hours to run on them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Month {
    /// The 1st to the 31st; months without a day skip it.
    days: [bool; 31],
    /// The last day of every month, however long.
    last: bool,
    hours: [bool; 24],
    minute: u8,
}

impl Default for Month {
    fn default() -> Self {
        let mut hours = [false; 24];
        hours[3] = true;
        Self {
            days: [false; 31],
            last: false,
            hours,
            minute: 0,
        }
    }
}

impl Month {
    pub fn is_day(&self, day: usize) -> bool {
        self.days[day]
    }

    pub fn toggle_day(&mut self, day: usize) {
        self.days[day] = !self.days[day];
    }

    pub fn is_last(&self) -> bool {
        self.last
    }

    pub fn toggle_last(&mut self) {
        self.last = !self.last;
    }

    pub fn is_hour(&self, hour: usize) -> bool {
        self.hours[hour]
    }

    pub fn toggle_hour(&mut self, hour: usize) {
        self.hours[hour] = !self.hours[hour];
    }

    pub fn minute(&self) -> u8 {
        self.minute
    }

    pub fn set_minute(&mut self, minute: u8) {
        self.minute = minute.min(59);
    }

    pub fn expressions(&self) -> Vec<String> {
        if !self.hours.contains(&true) {
            return Vec::new();
        }
        let time = format!("{}:{:02}:00", hour_list(&self.hours), self.minute);
        let mut expressions = Vec::new();
        if self.days.contains(&true) {
            let days = list(&self.days, |day| format!("{:02}", day + 1));
            expressions.push(format!("*-*-{days} {time}"));
        }
        // `~01` counts from the month's end and takes no list of its own.
        if self.last {
            expressions.push(format!("*-*~01 {time}"));
        }
        expressions
    }
}

/// `05` once an hour, `05/15` four times from minute 5.
fn minutes(first: u8, division: Division) -> String {
    match division {
        Division::Once => format!("{first:02}"),
        division => format!("{first:02}/{}", division.step()),
    }
}

/// Every hour is `*`; otherwise a list such as `09..17` or `03,15`.
fn hour_list(hours: &[bool; 24]) -> String {
    if hours.iter().all(|&on| on) {
        "*".to_owned()
    } else {
        list(hours, |hour| format!("{hour:02}"))
    }
}

/// The set positions as systemd lists them: runs of three or more as a
/// range, shorter ones one by one.
fn list(on: &[bool], name: impl Fn(usize) -> String) -> String {
    let mut parts = Vec::new();
    let mut index = 0;
    while index < on.len() {
        if !on[index] {
            index += 1;
            continue;
        }
        let start = index;
        while index + 1 < on.len() && on[index + 1] {
            index += 1;
        }
        match index - start {
            0 => parts.push(name(start)),
            1 => parts.extend([name(start), name(index)]),
            _ => parts.push(format!("{}..{}", name(start), name(index))),
        }
        index += 1;
    }
    parts.join(",")
}

/// What the pointer is on, for the line that explains it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hint {
    Cell(usize, usize),
    Day(usize),
    Hour(usize),
    Divisions,
    Minute,
    Preset(Preset),
    Modes,
    MonthDay(usize),
    LastDay,
    MonthHour(usize),
    MonthMinute,
}

impl From<Spot> for Hint {
    fn from(spot: Spot) -> Self {
        match spot {
            Spot::Cell(day, hour) => Self::Cell(day, hour),
            Spot::Day(day) => Self::Day(day),
            Spot::Hour(hour) => Self::Hour(hour),
        }
    }
}

/// `Mondays`, for saying what a painted cell repeats on.
pub fn days_name(day: usize) -> String {
    match day {
        0 => fl!("weekdays-mon"),
        1 => fl!("weekdays-tue"),
        2 => fl!("weekdays-wed"),
        3 => fl!("weekdays-thu"),
        4 => fl!("weekdays-fri"),
        5 => fl!("weekdays-sat"),
        _ => fl!("weekdays-sun"),
    }
}

/// The run times inside `hour`: `09:00` once, `09:00, 09:15, 09:30, 09:45`
/// four times, the first two and the last when there are more.
pub fn run_times(hour: usize, minute: u8, division: Division) -> String {
    let times: Vec<String> = (0..division.times())
        .map(|run| format!("{hour:02}:{:02}", minute + run * division.step()))
        .collect();
    if times.len() <= 4 {
        times.join(", ")
    } else {
        format!("{}, {}, … {}", times[0], times[1], times[times.len() - 1])
    }
}

#[derive(Debug, Clone)]
pub enum Edit {
    Paint { day: usize, hour: usize, on: bool },
    ToggleDay(usize),
    ToggleHour(usize),
    Minute(u8),
    Division(Division),
    Preset(Preset),
    MonthDay(usize),
    MonthLast,
    MonthHour(usize),
    MonthMinute(u8),
}

fn publish(edit: Edit) -> canvas::Action<Message> {
    canvas::Action::publish(Message::Composer(ComposerMessage::Rhythm(edit))).and_capture()
}

const LABEL: f32 = 44.0;
const HEAD: f32 = 22.0;
const CELL_HEIGHT: f32 = 28.0;
const GAP: f32 = 2.0;
/// The grid's height on screen.
pub const HEIGHT: f32 = HEAD + CELL_HEIGHT * 7.0;

/// The week as a grid to paint: a drag paints or erases every cell it
/// crosses, a day's name or an hour fills or clears its whole line.
pub struct WeekGrid<'a> {
    pub week: &'a Week,
    /// The day and hour it is now on the host, outlined.
    pub now: Option<(usize, usize)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Spot {
    Cell(usize, usize),
    Day(usize),
    Hour(usize),
}

#[derive(Debug, Default)]
pub struct GridState {
    /// While the button is held: whether the drag paints or erases.
    painting: Option<bool>,
    hover: Option<Spot>,
}

impl WeekGrid<'_> {
    fn cell_width(width: f32) -> f32 {
        (width - LABEL) / 24.0
    }

    fn spot(size: Size, position: Point) -> Option<Spot> {
        let column = ((position.x - LABEL) / Self::cell_width(size.width)).floor();
        let row = ((position.y - HEAD) / CELL_HEIGHT).floor();
        let column = (0.0..24.0).contains(&column).then_some(column as usize);
        let row = (0.0..7.0).contains(&row).then_some(row as usize);
        match (row, column) {
            (Some(day), Some(hour)) => Some(Spot::Cell(day, hour)),
            (Some(day), None) if position.x < LABEL => Some(Spot::Day(day)),
            (None, Some(hour)) if position.y < HEAD => Some(Spot::Hour(hour)),
            _ => None,
        }
    }

    fn cell(size: Size, day: usize, hour: usize) -> Rectangle {
        let width = Self::cell_width(size.width);
        Rectangle {
            x: LABEL + width * hour as f32 + GAP / 2.0,
            y: HEAD + CELL_HEIGHT * day as f32 + GAP / 2.0,
            width: width - GAP,
            height: CELL_HEIGHT - GAP,
        }
    }
}

impl canvas::Program<Message> for WeekGrid<'_> {
    type State = GridState;

    fn update(
        &self,
        state: &mut GridState,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        let spot = cursor
            .position_in(bounds)
            .and_then(|position| Self::spot(bounds.size(), position));
        match event {
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => match spot? {
                Spot::Cell(day, hour) => {
                    let on = !self.week.is_on(day, hour);
                    state.painting = Some(on);
                    Some(publish(Edit::Paint { day, hour, on }))
                }
                Spot::Day(day) => Some(publish(Edit::ToggleDay(day))),
                Spot::Hour(hour) => Some(publish(Edit::ToggleHour(hour))),
            },
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                state.painting = None;
                None
            }
            canvas::Event::Mouse(mouse::Event::CursorMoved { .. } | mouse::Event::CursorLeft) => {
                let moved = state.hover != spot;
                state.hover = spot;
                match (state.painting, spot) {
                    (Some(on), Some(Spot::Cell(day, hour))) if self.week.is_on(day, hour) != on => {
                        Some(publish(Edit::Paint { day, hour, on }))
                    }
                    // Other widgets still see the move, so no capture.
                    _ if moved => Some(canvas::Action::publish(Message::Composer(
                        ComposerMessage::Hint(spot.map(Hint::from)),
                    ))),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        state: &GridState,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let size = bounds.size();
        let width = Self::cell_width(size.width);
        let (hover_day, hover_hour) = match state.hover {
            Some(Spot::Cell(day, hour)) => (Some(day), Some(hour)),
            Some(Spot::Day(day)) => (Some(day), None),
            Some(Spot::Hour(hour)) => (None, Some(hour)),
            None => (None, None),
        };

        for hour in (0..24).step_by(3) {
            frame.fill_text(Text {
                content: format!("{hour:02}"),
                position: Point::new(LABEL + width * hour as f32 + 2.0, HEAD / 2.0),
                color: if hover_hour == Some(hour) {
                    palette::TEXT
                } else {
                    palette::MUTED
                },
                size: 11.0.into(),
                align_y: Vertical::Center,
                ..Text::default()
            });
        }
        for (day, weekday) in WEEKDAYS.into_iter().enumerate() {
            frame.fill_text(Text {
                content: label::weekday(weekday),
                position: Point::new(LABEL - 8.0, HEAD + CELL_HEIGHT * (day as f32 + 0.5)),
                color: if hover_day == Some(day) {
                    palette::TEXT
                } else {
                    palette::MUTED
                },
                size: 12.0.into(),
                align_x: Alignment::Right,
                align_y: Vertical::Center,
                ..Text::default()
            });
        }

        for day in 0..7 {
            for hour in 0..24 {
                let cell = Self::cell(size, day, hour);
                // Night hours sit a shade darker, like the keys of a piano.
                let off = if (7..19).contains(&hour) {
                    palette::GRID
                } else {
                    palette::GRID.scale_alpha(0.6)
                };
                let color = if self.week.is_on(day, hour) {
                    palette::DRAFT
                } else {
                    off
                };
                let path = Path::rounded_rectangle(cell.position(), cell.size(), 3.0.into());
                frame.fill(&path, color);
                let lit = match state.hover {
                    Some(Spot::Cell(d, h)) => d == day && h == hour,
                    Some(Spot::Day(d)) => d == day,
                    Some(Spot::Hour(h)) => h == hour,
                    None => false,
                };
                if lit {
                    frame.stroke(
                        &path,
                        Stroke::default().with_color(palette::TEXT).with_width(1.0),
                    );
                } else if self.now == Some((day, hour)) {
                    frame.stroke(
                        &path,
                        Stroke::default().with_color(palette::COMB).with_width(1.5),
                    );
                }
            }
        }
        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        _state: &GridState,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        match cursor
            .position_in(bounds)
            .and_then(|position| Self::spot(bounds.size(), position))
        {
            Some(_) => mouse::Interaction::Pointer,
            None => mouse::Interaction::default(),
        }
    }
}

/// The grid position of `weekday` (Monday first).
pub fn day_index(weekday: Weekday) -> usize {
    weekday.to_monday_zero_offset() as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn days_sharing_hours_share_an_expression() {
        let mut week = Week::default();
        for day in 0..5 {
            week.set(day, 9, true);
            week.set(day, 18, true);
        }
        week.set(5, 10, true);
        assert_eq!(
            week.expressions(),
            ["Mon..Fri *-*-* 09,18:00:00", "Sat *-*-* 10:00:00"]
        );
    }

    #[test]
    fn full_lines_and_runs_are_shortened() {
        let mut week = Week::preset(Preset::Hourly);
        assert_eq!(week.expressions(), ["*-*-* *:00:00"]);
        week.set_division(Division::Four);
        week.set_minute(40);
        // The first minute stays inside the 15 minute step.
        assert_eq!(week.minute(), 14);
        assert_eq!(week.expressions(), ["*-*-* *:14/15:00"]);

        let mut week = Week::default();
        for hour in 9..=17 {
            week.set(0, hour, true);
            week.set(2, hour, true);
        }
        week.set(0, 20, true);
        week.set(2, 20, true);
        week.set_minute(30);
        assert_eq!(week.expressions(), ["Mon,Wed *-*-* 09..17,20:30:00"]);
    }

    #[test]
    fn whole_days_and_hours_toggle() {
        let mut week = Week::default();
        week.toggle_day(6);
        assert_eq!(week.expressions(), ["Sun *-*-* *:00:00"]);
        week.toggle_day(6);
        assert!(week.expressions().is_empty());
        week.toggle_hour(3);
        assert_eq!(week.expressions(), ["*-*-* 03:00:00"]);
        assert_eq!(Week::preset(Preset::Nightly), week);
    }

    #[test]
    fn presets_say_what_they_are_called() {
        assert_eq!(
            Week::preset(Preset::WorkdayMornings).expressions(),
            ["Mon..Fri *-*-* 09:00:00"]
        );
        assert_eq!(
            Week::preset(Preset::Weekly).expressions(),
            ["Sun *-*-* 02:00:00"]
        );
        assert_eq!(
            Week::preset(Preset::Quarterly).expressions(),
            ["*-*-* *:00/15:00"]
        );
    }

    #[test]
    fn run_times_list_the_runs_inside_an_hour() {
        assert_eq!(run_times(9, 0, Division::Once), "09:00");
        assert_eq!(
            run_times(9, 5, Division::Four),
            "09:05, 09:20, 09:35, 09:50"
        );
        assert_eq!(run_times(23, 0, Division::Twelve), "23:00, 23:05, … 23:55");
    }

    #[test]
    fn month_days_and_the_last_day() {
        let mut month = Month::default();
        assert!(month.expressions().is_empty());
        month.toggle_day(0);
        month.toggle_day(14);
        month.set_minute(30);
        assert_eq!(month.expressions(), ["*-*-01,15 03:30:00"]);
        month.toggle_last();
        assert_eq!(
            month.expressions(),
            ["*-*-01,15 03:30:00", "*-*~01 03:30:00"]
        );
        month.toggle_hour(3);
        assert!(month.expressions().is_empty());
    }

    #[test]
    fn the_grid_finds_cells_labels_and_nothing() {
        let size = Size::new(LABEL + 240.0, HEIGHT);
        assert_eq!(
            WeekGrid::spot(size, Point::new(LABEL + 95.0, HEAD + 30.0)),
            Some(Spot::Cell(1, 9))
        );
        assert_eq!(
            WeekGrid::spot(size, Point::new(10.0, HEAD + 5.0)),
            Some(Spot::Day(0))
        );
        assert_eq!(
            WeekGrid::spot(size, Point::new(LABEL + 235.0, 5.0)),
            Some(Spot::Hour(23))
        );
        assert_eq!(WeekGrid::spot(size, Point::new(5.0, 5.0)), None);
    }
}
