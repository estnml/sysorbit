//! The music box: one row ("track") per timer, time flowing right to left
//! under a fixed comb. Runs and elapses are the notes.

use iced::alignment::Vertical;
use iced::mouse;
use iced::widget::canvas::{self, Cache, Frame, Path, Stroke, Text, stroke};
use iced::widget::text::Alignment;
use iced::{Color, Point, Rectangle, Renderer, Size, Theme, Vector};
use jiff::tz::TimeZone;
use jiff::{RoundMode, SignedDuration, Timestamp, ToSpan, Unit, Zoned, ZonedRound};

use super::Message;
use super::label;
use super::palette;
use super::scale::{TimeAxis, Zoom};
use crate::model::{Downtime, DowntimeKind, Interval, Outcome, Timer};
use crate::systemd::timespan;
use crate::timeline::Mark;

/// Margin left of the labels and right of the grid; the header and the
/// drawer use it too, so everything lines up with the box.
pub const PAD: f32 = 24.0;
/// Label column width bounds; it takes a share of the width in between.
const LABEL_MIN: f32 = 150.0;
const LABEL_MAX: f32 = 250.0;
/// Narrower windows scroll sideways instead of squeezing the notes together.
pub const MIN_WIDTH: f32 = 720.0;
const HEADER: f32 = 44.0;
const ROW: f32 = 58.0;
const FOOTER: f32 = 28.0;
const LEGEND: f32 = 64.0;
/// Notes closer than this are drawn as one tremolo bar.
const TREMOLO_GAP: f32 = 5.0;

pub struct Row {
    pub timer: Timer,
    pub title: String,
    pub subtitle: String,
    pub marks: Vec<Mark>,
}

/// The music box is drawn as two stacked canvases. Text always renders above
/// the shapes of its own layer, so a tooltip sharing the content's layer
/// would show the labels underneath through its background.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    /// Grid, tracks and notes; cached.
    Content,
    /// Hover line and tooltip; handles the mouse.
    Overlay,
}

pub struct MusicBox<'a> {
    pub layer: Layer,
    pub rows: &'a [Row],
    pub downtime: &'a [Downtime],
    pub zoom: Zoom,
    pub now: Timestamp,
    pub tz: &'a TimeZone,
    pub selected: Option<usize>,
    /// Start of the run whose logs are open; ringed on the selected track.
    pub selected_run: Option<Timestamp>,
    /// Anchor of the keyboard focused mark on the selected track.
    pub focus: Option<Timestamp>,
    pub cache: &'a Cache,
}

/// Canvas height needed for `rows` tracks.
pub fn height(rows: usize) -> f32 {
    HEADER + ROW * rows.max(1) as f32 + FOOTER + LEGEND
}

/// Where the grid starts in a box `width` wide, i.e. the label column's width.
pub fn grid_left(width: f32) -> f32 {
    (width * 0.26).clamp(LABEL_MIN, LABEL_MAX)
}

impl MusicBox<'_> {
    fn axis(&self, width: f32) -> TimeAxis {
        let left = grid_left(width);
        TimeAxis {
            window: self.zoom.visible(self.now),
            left,
            right: (width - PAD).max(left + 1.0),
        }
    }

    fn grid(&self, size: Size) -> Rectangle {
        let axis = self.axis(size.width);
        Rectangle {
            x: axis.left,
            y: HEADER,
            width: axis.right - axis.left,
            height: ROW * self.rows.len().max(1) as f32,
        }
    }

    fn row_center(index: usize) -> f32 {
        HEADER + ROW * (index as f32 + 0.5)
    }

    fn row_at(&self, y: f32) -> Option<usize> {
        let index = ((y - HEADER) / ROW).floor();
        (index >= 0.0 && (index as usize) < self.rows.len()).then_some(index as usize)
    }

    fn draw_content(&self, frame: &mut Frame) {
        let size = frame.size();
        let axis = self.axis(size.width);
        let grid = self.grid(size);

        if let Some(index) = self.selected.filter(|&index| index < self.rows.len()) {
            // Ends at the grid's right edge instead of spilling into the margin.
            frame.fill_rectangle(
                Point::new(0.0, Self::row_center(index) - ROW / 2.0),
                Size::new(grid.x + grid.width, ROW),
                palette::SURFACE,
            );
        }

        self.draw_downtime(frame, &axis, grid);
        self.draw_ticks(frame, &axis, grid);

        for (index, row) in self.rows.iter().enumerate() {
            self.draw_track(frame, &axis, grid, index, row);
        }

        frame.stroke(
            &Path::rounded_rectangle(grid.position(), grid.size(), 6.0.into()),
            Stroke::default()
                .with_color(palette::GRID_MAJOR)
                .with_width(1.0),
        );

        self.draw_comb(frame, &axis, grid);
        draw_legend(frame, grid.y + grid.height + FOOTER);
    }

    fn draw_downtime(&self, frame: &mut Frame, axis: &TimeAxis, grid: Rectangle) {
        for down in self.downtime {
            let x0 = axis.x(down.interval.start).max(grid.x);
            let x1 = axis.x(down.interval.end).min(grid.x + grid.width);
            if x1 <= x0 {
                continue;
            }
            let area = Rectangle {
                x: x0,
                y: grid.y,
                width: x1 - x0,
                height: grid.height,
            };
            let alpha = match down.kind {
                DowntimeKind::PoweredOff => 0.45,
                DowntimeKind::Suspended => 0.25,
            };
            hatch(frame, area, palette::DOWNTIME.scale_alpha(alpha));
        }
    }

    fn draw_ticks(&self, frame: &mut Frame, axis: &TimeAxis, grid: Rectangle) {
        let ticks = ticks(self.zoom, axis.window, self.tz);

        for tick in &ticks {
            let x = axis.x(tick.at);
            if x < grid.x || x > grid.x + grid.width {
                continue;
            }
            let color = if tick.major {
                palette::GRID_MAJOR
            } else {
                palette::GRID
            };
            frame.stroke(
                &Path::line(Point::new(x, grid.y), Point::new(x, grid.y + grid.height)),
                Stroke::default().with_color(color).with_width(1.0),
            );
        }

        // Day columns carry their name in the middle, hour lines at the line.
        // When labels would collide, major ones are kept first.
        let mut labels: Vec<(f32, &Tick, String)> = ticks
            .iter()
            .filter_map(|tick| {
                let content = tick.label.clone()?;
                let x = match self.zoom {
                    Zoom::Week | Zoom::Month => axis.x(tick.at) + axis.width(tick.span) / 2.0,
                    Zoom::Hours | Zoom::Day => axis.x(tick.at),
                };
                let half = text_width(&content, 13.0) / 2.0;
                (x - half >= grid.x && x + half <= grid.x + grid.width)
                    .then_some((x, tick, content))
            })
            .collect();
        labels.sort_by_key(|(_, tick, _)| !tick.major);

        let mut taken: Vec<(f32, f32)> = Vec::new();
        for (x, tick, content) in labels {
            let half = text_width(&content, 13.0) / 2.0 + 6.0;
            if taken
                .iter()
                .any(|&(start, end)| x - half < end && x + half > start)
            {
                continue;
            }
            taken.push((x - half, x + half));
            frame.fill_text(Text {
                content,
                position: Point::new(x, HEADER / 2.0),
                color: if tick.major {
                    palette::TEXT
                } else {
                    palette::MUTED
                },
                size: 13.0.into(),
                align_x: Alignment::Center,
                align_y: Vertical::Center,
                ..Text::default()
            });
        }
    }

    fn draw_track(
        &self,
        frame: &mut Frame,
        axis: &TimeAxis,
        grid: Rectangle,
        index: usize,
        row: &Row,
    ) {
        let center = Self::row_center(index);
        let active = row.timer.active;

        let line = Path::line(
            Point::new(grid.x, center),
            Point::new(grid.x + grid.width, center),
        );
        if active {
            frame.stroke(
                &line,
                Stroke::default().with_color(palette::GRID).with_width(1.0),
            );
        } else {
            frame.stroke(&line, dashed(palette::GRID_MAJOR, 1.0, &[4.0, 5.0]));
        }
        if index > 0 {
            let top = center - ROW / 2.0;
            frame.stroke(
                &Path::line(Point::new(PAD, top), Point::new(grid.x, top)),
                Stroke::default().with_color(palette::GRID).with_width(1.0),
            );
        }

        let (title, subtitle) = if active {
            (palette::TEXT, palette::MUTED)
        } else {
            (palette::MUTED, palette::FAINT)
        };
        let label_width = grid.x - PAD - 16.0;
        frame.fill_text(Text {
            content: ellipsize(&row.title, label_width, 15.0),
            position: Point::new(PAD, center - 9.0),
            color: title,
            size: 15.0.into(),
            align_y: Vertical::Center,
            ..Text::default()
        });
        frame.fill_text(Text {
            content: ellipsize(&row.subtitle, label_width, 12.0),
            position: Point::new(PAD, center + 10.0),
            color: subtitle,
            size: 12.0.into(),
            align_y: Vertical::Center,
            ..Text::default()
        });

        let selected_run = self.selected_run.filter(|_| self.selected == Some(index));
        draw_marks(frame, axis, center, &row.marks, self.now, selected_run);

        if let Some(next) = beyond_window(row, axis.window) {
            frame.fill_text(Text {
                content: format!("→ {}", label::moment(next, self.tz)),
                position: Point::new(grid.x + grid.width - 8.0, center - 14.0),
                color: palette::MUTED,
                size: 11.0.into(),
                align_x: Alignment::Right,
                align_y: Vertical::Center,
                ..Text::default()
            });
        }
    }

    fn draw_comb(&self, frame: &mut Frame, axis: &TimeAxis, grid: Rectangle) {
        let x = axis.x(self.now);
        frame.stroke(
            &Path::line(
                Point::new(x, grid.y - 6.0),
                Point::new(x, grid.y + grid.height + 6.0),
            ),
            Stroke::default().with_color(palette::COMB).with_width(2.0),
        );
        frame.fill_text(Text {
            content: format!("now {}", label::clock(self.now, self.tz)),
            position: Point::new(x, grid.y + grid.height + 16.0),
            color: palette::COMB,
            size: 12.0.into(),
            align_x: Alignment::Center,
            align_y: Vertical::Center,
            ..Text::default()
        });
    }

    /// Draws the hover line and tooltip; true when a tooltip was drawn.
    fn draw_hover(&self, frame: &mut Frame, cursor: Point) -> bool {
        let size = frame.size();
        let axis = self.axis(size.width);
        let grid = self.grid(size);
        let Some(index) = self.row_at(cursor.y) else {
            return false;
        };
        let row = &self.rows[index];

        if cursor.x < grid.x {
            let mut lines = vec![(row.timer.id.clone(), palette::TEXT)];
            if !row.timer.description.is_empty() {
                lines.push((row.timer.description.clone(), palette::MUTED));
            }
            lines.push((row.subtitle.clone(), palette::MUTED));
            tooltip(frame, cursor, &lines);
            return true;
        }
        if cursor.x > grid.x + grid.width {
            return false;
        }

        frame.stroke(
            &Path::line(
                Point::new(cursor.x, grid.y),
                Point::new(cursor.x, grid.y + grid.height),
            ),
            dashed(palette::MUTED.scale_alpha(0.5), 1.0, &[2.0, 3.0]),
        );
        frame.fill_text(Text {
            content: label::moment(axis.time(cursor.x), self.tz),
            position: Point::new(cursor.x, grid.y - 2.0),
            color: palette::MUTED,
            size: 11.0.into(),
            align_x: Alignment::Center,
            align_y: Vertical::Bottom,
            ..Text::default()
        });

        let center = Self::row_center(index);
        let lines = hit(&row.marks, &axis, center, cursor, self.now)
            .map(|mark| describe(mark, self.now, self.tz))
            .or_else(|| {
                let at = axis.time(cursor.x);
                self.downtime
                    .iter()
                    .find(|down| down.interval.contains(at))
                    .map(|down| describe_downtime(down, self.tz))
            });
        match lines {
            Some(lines) => {
                tooltip(frame, cursor, &lines);
                true
            }
            None => false,
        }
    }

    /// Rings the focused mark and, unless the mouse shows another tooltip,
    /// pins the mark's tooltip next to it.
    fn draw_focus(&self, frame: &mut Frame, with_tooltip: bool) {
        let (Some(index), Some(at)) = (self.selected, self.focus) else {
            return;
        };
        let Some(mark) = self.rows.get(index).and_then(|row| {
            row.marks
                .iter()
                .find(|mark| focusable(mark) && mark.anchor() == at)
        }) else {
            return;
        };
        let axis = self.axis(frame.width());
        if !axis.window.contains(at) {
            return;
        }

        let point = mark_point(mark, &axis, Self::row_center(index));
        frame.stroke(
            &Path::circle(point, 12.0),
            Stroke::default().with_color(palette::TEXT).with_width(1.5),
        );
        if with_tooltip {
            tooltip(frame, point, &describe(mark, self.now, self.tz));
        }
    }
}

/// Marks keyboard focus can land on; a tether only links two others.
pub fn focusable(mark: &Mark) -> bool {
    !matches!(mark, Mark::Tether { .. })
}

/// Where a mark is drawn on its track.
fn mark_point(mark: &Mark, axis: &TimeAxis, center: f32) -> Point {
    match mark {
        Mark::CatchUp { from, to } => {
            Point::new((axis.x(*from) + axis.x(*to)) / 2.0, center - 18.0)
        }
        Mark::Planned { at, scheduled, .. } => Point::new(axis.x(scheduled.unwrap_or(*at)), center),
        Mark::Run { start, .. } => Point::new(axis.x(*start), center),
        other => Point::new(axis.x(other.anchor()), center),
    }
}

/// The mark under the cursor, or else the nearest one beside it on the
/// track, so small marks are easy to click.
fn pick<'a>(
    marks: &'a [Mark],
    axis: &TimeAxis,
    center: f32,
    cursor: Point,
    now: Timestamp,
) -> Option<&'a Mark> {
    const REACH: f32 = 14.0;
    hit(marks, axis, center, cursor, now).or_else(|| {
        marks
            .iter()
            .filter(|mark| focusable(mark))
            .map(|mark| (mark, (mark_point(mark, axis, center).x - cursor.x).abs()))
            .filter(|(_, distance)| *distance <= REACH)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(mark, _)| mark)
    })
}

impl canvas::Program<Message> for MusicBox<'_> {
    type State = ();

    fn update(
        &self,
        _state: &mut (),
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        if self.layer == Layer::Content {
            return None;
        }
        match event {
            canvas::Event::Mouse(mouse::Event::CursorMoved { .. } | mouse::Event::CursorLeft) => {
                Some(canvas::Action::request_redraw())
            }
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let position = cursor.position_in(bounds)?;
                let index = self.row_at(position.y)?;
                let axis = self.axis(bounds.width);
                let center = Self::row_center(index);
                // A note opens the logs of its run, another mark gets the
                // focus, anything else selects the timer.
                let picked = (position.x >= axis.left)
                    .then(|| pick(&self.rows[index].marks, &axis, center, position, self.now))
                    .flatten();
                let message = match picked {
                    Some(Mark::Run { start, .. }) => Message::SelectRun(index, *start),
                    Some(mark) => Message::Focus(index, mark.anchor()),
                    None => Message::Select(index),
                };
                Some(canvas::Action::publish(message).and_capture())
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
        match self.layer {
            Layer::Content => vec![
                self.cache
                    .draw(renderer, bounds.size(), |frame| self.draw_content(frame)),
            ],
            Layer::Overlay => {
                let mut frame = Frame::new(renderer, bounds.size());
                let hovering = cursor
                    .position_in(bounds)
                    .is_some_and(|position| self.draw_hover(&mut frame, position));
                self.draw_focus(&mut frame, !hovering);
                vec![frame.into_geometry()]
            }
        }
    }

    fn mouse_interaction(
        &self,
        _state: &(),
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        match cursor.position_in(bounds) {
            Some(position) if self.layer == Layer::Overlay && self.row_at(position.y).is_some() => {
                mouse::Interaction::Pointer
            }
            _ => mouse::Interaction::default(),
        }
    }
}

/// Draws one track's marks. Everything is clipped to the grid by hand:
/// clip layers are costly and showed corrupted meshes on wgpu.
fn draw_marks(
    frame: &mut Frame,
    axis: &TimeAxis,
    center: f32,
    marks: &[Mark],
    now: Timestamp,
    selected_run: Option<Timestamp>,
) {
    let (left, right) = (axis.left, axis.right);
    let inside = |x: f32| x >= left && x <= right;

    // Secondary marks go underneath the notes.
    for mark in marks {
        match mark {
            Mark::Tether { from, to } => {
                let (x0, x1) = (axis.x(*from).max(left), axis.x(*to).min(right));
                if x0 < x1 {
                    frame.stroke(
                        &Path::line(Point::new(x0, center), Point::new(x1, center)),
                        dashed(palette::NOTE.scale_alpha(0.5), 1.5, &[3.0, 4.0]),
                    );
                }
            }
            Mark::CatchUp { from, to } => {
                let (x0, x1) = (axis.x(*from), axis.x(*to));
                if inside(x0) && inside(x1) {
                    draw_catch_up(frame, x0, x1, center);
                }
            }
            Mark::Planned { at, delay, .. } if delay.is_positive() => {
                let x = axis.x(*at);
                let width = axis.width(*delay).max(10.0);
                pill(
                    frame,
                    (x - 5.0).max(left),
                    (x + width).min(right),
                    center,
                    12.0,
                    palette::NOTE.scale_alpha(0.25),
                );
            }
            _ => {}
        }
    }

    let positions = |pick: fn(&Mark) -> Option<Timestamp>| -> Vec<f32> {
        marks
            .iter()
            .filter_map(pick)
            .map(|at| axis.x(at))
            .filter(|&x| inside(x))
            .collect()
    };
    let planned = positions(|mark| match mark {
        Mark::Planned { at, scheduled, .. } => Some(scheduled.unwrap_or(*at)),
        _ => None,
    });
    let ghosts = positions(|mark| match mark {
        Mark::Ghost { at } => Some(*at),
        _ => None,
    });
    let runs = positions(|mark| match mark {
        Mark::Run { start, .. } => Some(*start),
        _ => None,
    });

    let ghost_clusters = clusters(&ghosts);
    let planned_clusters = clusters(&planned);
    let run_clusters = clusters(&runs);
    for (cluster, color) in [
        (&ghost_clusters, palette::FAINT),
        (&planned_clusters, palette::NOTE.scale_alpha(0.55)),
        (&run_clusters, palette::NOTE),
    ] {
        for &(start, end) in cluster {
            tremolo(frame, start, end, center, color);
        }
    }

    let dense = |clusters: &[(f32, f32)], x: f32| {
        clusters.iter().any(|&(start, end)| x >= start && x <= end)
    };

    for mark in marks {
        match mark {
            Mark::Ghost { at } => {
                let x = axis.x(*at);
                if inside(x) && !dense(&ghost_clusters, x) {
                    frame.fill(&Path::circle(Point::new(x, center), 4.0), palette::FAINT);
                }
            }
            Mark::Skipped { at, .. } => {
                let x = axis.x(*at);
                if inside(x) {
                    frame.stroke(
                        &Path::circle(Point::new(x, center), 5.0),
                        Stroke::default().with_color(palette::MUTED).with_width(1.5),
                    );
                }
            }
            Mark::Planned {
                at,
                scheduled,
                wakes,
                ..
            } => {
                let x = axis.x(scheduled.unwrap_or(*at));
                if !inside(x) {
                    continue;
                }
                if !dense(&planned_clusters, x) {
                    frame.fill(
                        &Path::circle(Point::new(x, center), 5.0),
                        palette::NOTE.scale_alpha(0.55),
                    );
                }
                if *wakes {
                    frame.stroke(
                        &Path::circle(Point::new(x, center), 9.0),
                        Stroke::default()
                            .with_color(palette::NOTE.scale_alpha(0.8))
                            .with_width(1.5),
                    );
                }
            }
            Mark::Run {
                start,
                end,
                outcome,
            } => {
                let (x0, x1) = (axis.x(*start), axis.x(end.unwrap_or(now)));
                if x1 < left || x0 > right {
                    continue;
                }
                let failed = matches!(outcome, Outcome::Failed(_));
                // Failures always stand out, even inside a tremolo.
                if dense(&run_clusters, x0) && !failed {
                    continue;
                }
                let color = if failed {
                    palette::FAILED
                } else {
                    palette::NOTE
                };
                // A dot for short runs, a bar when the run is wide enough to
                // show its length.
                if x1 - x0 < 6.0 {
                    if inside(x0) {
                        frame.fill(&Path::circle(Point::new(x0, center), 5.0), color);
                    }
                } else {
                    pill(
                        frame,
                        (x0 - 5.0).max(left),
                        (x1 + 5.0).min(right),
                        center,
                        10.0,
                        color,
                    );
                }
                if selected_run == Some(*start) && inside(x0) {
                    frame.stroke(
                        &Path::circle(Point::new(x0, center), 10.0),
                        Stroke::default().with_color(palette::TEXT).with_width(2.0),
                    );
                }
                if *outcome == Outcome::Running && inside(x1) {
                    frame.stroke(
                        &Path::circle(Point::new(x1, center), 9.0),
                        Stroke::default().with_color(color).with_width(1.5),
                    );
                }
            }
            _ => {}
        }
    }
}

/// A horizontal rounded bar from `x0` to `x1`, centered on `center`.
fn pill(frame: &mut Frame, x0: f32, x1: f32, center: f32, height: f32, color: Color) {
    if x1 <= x0 {
        return;
    }
    let radius = (height / 2.0).min((x1 - x0) / 2.0);
    frame.fill(
        &Path::rounded_rectangle(
            Point::new(x0, center - height / 2.0),
            Size::new(x1 - x0, height),
            radius.into(),
        ),
        color,
    );
}

fn draw_catch_up(frame: &mut Frame, x0: f32, x1: f32, center: f32) {
    let start = Point::new(x0, center - 7.0);
    let end = Point::new(x1, center - 7.0);
    let control = Point::new((x0 + x1) / 2.0, center - 30.0);
    let stroke = Stroke::default()
        .with_color(palette::CATCH_UP)
        .with_width(1.5);

    frame.stroke(
        &Path::new(|path| {
            path.move_to(start);
            path.quadratic_curve_to(control, end);
        }),
        stroke,
    );

    // Arrow head along the curve's final tangent.
    let direction = Vector::new(end.x - control.x, end.y - control.y);
    let length = (direction.x * direction.x + direction.y * direction.y)
        .sqrt()
        .max(0.001);
    let unit = Vector::new(direction.x / length, direction.y / length);
    let normal = Vector::new(-unit.y, unit.x);
    let back = Point::new(end.x - unit.x * 7.0, end.y - unit.y * 7.0);
    frame.fill(
        &Path::new(|path| {
            path.move_to(end);
            path.line_to(Point::new(back.x + normal.x * 4.0, back.y + normal.y * 4.0));
            path.line_to(Point::new(back.x - normal.x * 4.0, back.y - normal.y * 4.0));
            path.close();
        }),
        palette::CATCH_UP,
    );
}

/// Repeated notes too close to tell apart, drawn like the tremolo marking in
/// sheet music: a bar crossed by slashes.
fn tremolo(frame: &mut Frame, start: f32, end: f32, center: f32, color: Color) {
    frame.fill(
        &Path::rounded_rectangle(
            Point::new(start - 4.0, center - 4.0),
            Size::new(end - start + 8.0, 8.0),
            4.0.into(),
        ),
        color.scale_alpha(0.6),
    );
    let middle = (start + end) / 2.0;
    for offset in [-6.0, 0.0, 6.0] {
        let x = middle + offset;
        frame.stroke(
            &Path::line(
                Point::new(x - 3.0, center + 7.0),
                Point::new(x + 3.0, center - 7.0),
            ),
            Stroke::default().with_color(color).with_width(2.0),
        );
    }
}

/// Groups of at least three positions with gaps under [`TREMOLO_GAP`].
fn clusters(xs: &[f32]) -> Vec<(f32, f32)> {
    let mut sorted = xs.to_vec();
    sorted.sort_by(f32::total_cmp);

    let mut result = Vec::new();
    let mut index = 0;
    while index < sorted.len() {
        let mut end = index;
        while end + 1 < sorted.len() && sorted[end + 1] - sorted[end] < TREMOLO_GAP {
            end += 1;
        }
        if end - index >= 2 {
            result.push((sorted[index], sorted[end]));
        }
        index = end + 1;
    }
    result
}

/// Diagonal stripes over `area`, clipped by hand and batched in one path.
fn hatch(frame: &mut Frame, area: Rectangle, color: Color) {
    const SPACING: f32 = 9.0;

    frame.fill_rectangle(area.position(), area.size(), color.scale_alpha(0.25));

    let bottom = area.y + area.height;
    let stripes = Path::new(|path| {
        // Each stripe rises at 45°: (x + t, bottom - t) for t in 0..height.
        let mut x = area.x - area.height;
        while x < area.x + area.width {
            let t0 = (area.x - x).max(0.0);
            let t1 = (area.x + area.width - x).min(area.height);
            if t0 < t1 {
                path.move_to(Point::new(x + t0, bottom - t0));
                path.line_to(Point::new(x + t1, bottom - t1));
            }
            x += SPACING;
        }
    });
    frame.stroke(
        &stripes,
        Stroke::default().with_color(color).with_width(1.0),
    );
}

fn dashed(color: Color, width: f32, segments: &'static [f32]) -> Stroke<'static> {
    Stroke {
        line_dash: stroke::LineDash {
            segments,
            offset: 0,
        },
        ..Stroke::default().with_color(color).with_width(width)
    }
}

/// The next elapse when it lies past the right edge and nothing is planned
/// inside the window.
fn beyond_window(row: &Row, window: Interval) -> Option<Timestamp> {
    if !row.timer.active {
        return None;
    }
    let planned_inside = row
        .marks
        .iter()
        .any(|mark| matches!(mark, Mark::Planned { at, .. } if *at <= window.end));
    if planned_inside {
        return None;
    }
    row.marks
        .iter()
        .find_map(|mark| match mark {
            Mark::Planned { at, .. } if *at > window.end => Some(*at),
            _ => None,
        })
        .or(row.timer.next_elapse.filter(|&next| next > window.end))
}

fn hit<'a>(
    marks: &'a [Mark],
    axis: &TimeAxis,
    center: f32,
    cursor: Point,
    now: Timestamp,
) -> Option<&'a Mark> {
    let distance = |mark: &Mark| -> f32 {
        match mark {
            Mark::Run { start, end, .. } => {
                let x0 = axis.x(*start);
                let x1 = axis.x(end.unwrap_or(now));
                if cursor.x >= x0 && cursor.x <= x1 {
                    (cursor.y - center).abs()
                } else {
                    (cursor.x - x0)
                        .abs()
                        .min((cursor.x - x1).abs())
                        .max((cursor.y - center).abs())
                }
            }
            Mark::CatchUp { from, to } => {
                let middle = Point::new((axis.x(*from) + axis.x(*to)) / 2.0, center - 18.0);
                middle.distance(cursor)
            }
            Mark::Tether { .. } => f32::INFINITY,
            Mark::Planned { at, scheduled, .. } => {
                Point::new(axis.x(scheduled.unwrap_or(*at)), center).distance(cursor)
            }
            other => Point::new(axis.x(other.anchor()), center).distance(cursor),
        }
    };

    marks
        .iter()
        .map(|mark| (mark, distance(mark)))
        .filter(|(_, distance)| *distance <= 10.0)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(mark, _)| mark)
}

fn describe(mark: &Mark, now: Timestamp, tz: &TimeZone) -> Vec<(String, Color)> {
    match mark {
        Mark::Run {
            start,
            end,
            outcome,
        } => {
            let mut lines = vec![(
                format!("Ran · {}", label::moment(*start, tz)),
                palette::TEXT,
            )];
            match end {
                Some(end) => lines.push((
                    format!("took {}", timespan::format(end.duration_since(*start))),
                    palette::MUTED,
                )),
                None => lines.push(("still running".to_owned(), palette::NOTE)),
            }
            match outcome {
                Outcome::Success => lines.push(("success".to_owned(), palette::MUTED)),
                Outcome::Failed(result) => {
                    lines.push((format!("failed: {result}"), palette::FAILED));
                }
                Outcome::Running => {}
            }
            lines
        }
        Mark::Planned {
            at,
            delay,
            scheduled,
            wakes,
        } => {
            let when = scheduled.unwrap_or(*at);
            let mut lines = vec![
                (
                    format!("Planned · {}", label::moment(when, tz)),
                    palette::TEXT,
                ),
                (label::relative(when, now), palette::MUTED),
            ];
            if delay.is_positive() {
                let text = match scheduled {
                    Some(_) => format!(
                        "calendar {}, +{} random delay",
                        label::moment(*at, tz),
                        timespan::format(when.duration_since(*at))
                    ),
                    None => format!(
                        "randomly within {1} after {0}",
                        label::moment(*at, tz),
                        timespan::format(*delay)
                    ),
                };
                lines.push((text, palette::MUTED));
            }
            if *wakes {
                lines.push(("wakes the machine from sleep".to_owned(), palette::MUTED));
            }
            lines
        }
        Mark::Ghost { at } => vec![
            (label::moment(*at, tz), palette::TEXT),
            (
                "timer inactive; it would fire here if enabled".to_owned(),
                palette::MUTED,
            ),
        ],
        Mark::Skipped { at, while_down } => vec![
            (
                format!("Skipped · {}", label::moment(*at, tz)),
                palette::TEXT,
            ),
            (
                if *while_down {
                    "machine was down".to_owned()
                } else {
                    "no run in the journal".to_owned()
                },
                palette::MUTED,
            ),
        ],
        Mark::CatchUp { from, to } => vec![
            ("Persistent catch-up".to_owned(), palette::CATCH_UP),
            (
                format!("{} → {}", label::moment(*from, tz), label::moment(*to, tz)),
                palette::MUTED,
            ),
        ],
        Mark::Tether { from, to } => vec![
            ("Monotonic countdown".to_owned(), palette::TEXT),
            (
                format!("{} → {}", label::moment(*from, tz), label::moment(*to, tz)),
                palette::MUTED,
            ),
        ],
    }
}

fn describe_downtime(down: &Downtime, tz: &TimeZone) -> Vec<(String, Color)> {
    let title = match down.kind {
        DowntimeKind::PoweredOff => "Powered off",
        DowntimeKind::Suspended => "Suspended",
    };
    vec![
        (title.to_owned(), palette::TEXT),
        (
            format!(
                "{} → {}",
                label::moment(down.interval.start, tz),
                label::moment(down.interval.end, tz)
            ),
            palette::MUTED,
        ),
    ]
}

fn tooltip(frame: &mut Frame, cursor: Point, lines: &[(String, Color)]) {
    const SIZE: f32 = 13.0;
    const LINE: f32 = 18.0;

    let width = lines
        .iter()
        .map(|(text, _)| text_width(text, SIZE))
        .fold(0.0, f32::max)
        + 24.0;
    let height = LINE * lines.len() as f32 + 14.0;
    let bounds = frame.size();

    let mut origin = Point::new(cursor.x + 16.0, cursor.y + 16.0);
    if origin.x + width > bounds.width {
        origin.x = cursor.x - width - 12.0;
    }
    if origin.y + height > bounds.height {
        origin.y = cursor.y - height - 12.0;
    }

    let shape = Path::rounded_rectangle(origin, Size::new(width, height), 8.0.into());
    frame.fill(&shape, palette::SURFACE);
    frame.stroke(
        &shape,
        Stroke::default()
            .with_color(palette::GRID_MAJOR)
            .with_width(1.0),
    );

    for (index, (content, color)) in lines.iter().enumerate() {
        frame.fill_text(Text {
            content: content.clone(),
            position: Point::new(
                origin.x + 12.0,
                origin.y + 7.0 + LINE * (index as f32 + 0.5),
            ),
            color: *color,
            size: SIZE.into(),
            align_y: Vertical::Center,
            ..Text::default()
        });
    }
}

/// Canvas text cannot be measured up front; this approximates the default font.
fn text_width(text: &str, size: f32) -> f32 {
    text.chars().count() as f32 * size * 0.56
}

fn ellipsize(text: &str, width: f32, size: f32) -> String {
    let fits = (width / (size * 0.56)) as usize;
    if text.chars().count() <= fits {
        return text.to_owned();
    }
    let mut short: String = text.chars().take(fits.saturating_sub(1)).collect();
    short.push('…');
    short
}

fn draw_legend(frame: &mut Frame, top: f32) {
    type Glyph = fn(&mut Frame, Point);

    let items: [(&str, Glyph); 8] = [
        ("ran", |frame, at| {
            frame.fill(&Path::circle(at, 5.0), palette::NOTE)
        }),
        ("failed", |frame, at| {
            frame.fill(&Path::circle(at, 5.0), palette::FAILED)
        }),
        ("planned", |frame, at| {
            frame.fill(&Path::circle(at, 5.0), palette::NOTE.scale_alpha(0.55));
        }),
        ("RandomizedDelaySec", |frame, at| {
            frame.fill(
                &Path::rounded_rectangle(
                    Point::new(at.x - 9.0, at.y - 6.0),
                    Size::new(20.0, 12.0),
                    6.0.into(),
                ),
                palette::NOTE.scale_alpha(0.25),
            );
        }),
        ("WakeSystem", |frame, at| {
            frame.fill(&Path::circle(at, 5.0), palette::NOTE.scale_alpha(0.55));
            frame.stroke(
                &Path::circle(at, 9.0),
                Stroke::default()
                    .with_color(palette::NOTE.scale_alpha(0.8))
                    .with_width(1.5),
            );
        }),
        ("skipped", |frame, at| {
            frame.stroke(
                &Path::circle(at, 5.0),
                Stroke::default().with_color(palette::MUTED).with_width(1.5),
            );
        }),
        ("Persistent catch-up", |frame, at| {
            draw_catch_up(frame, at.x - 10.0, at.x + 10.0, at.y + 8.0);
        }),
        ("machine down", |frame, at| {
            hatch(
                frame,
                Rectangle::new(Point::new(at.x - 10.0, at.y - 8.0), Size::new(22.0, 16.0)),
                palette::DOWNTIME.scale_alpha(0.45),
            );
        }),
    ];

    let width = frame.width();
    let mut x = PAD + 10.0;
    let mut y = top + 16.0;
    for (name, glyph) in items {
        let item_width = 34.0 + text_width(name, 13.0) + 24.0;
        if x + item_width > width - PAD {
            x = PAD + 10.0;
            y += 28.0;
        }
        glyph(frame, Point::new(x, y));
        frame.fill_text(Text {
            content: name.to_owned(),
            position: Point::new(x + 20.0, y),
            color: palette::MUTED,
            size: 13.0.into(),
            align_y: Vertical::Center,
            ..Text::default()
        });
        x += item_width;
    }
}

struct Tick {
    at: Timestamp,
    /// Distance to the next tick, for centering column labels.
    span: SignedDuration,
    major: bool,
    label: Option<String>,
}

/// Grid lines in local time. Minor lines every step, major lines on round
/// boundaries; labels depend on the zoom.
fn ticks(zoom: Zoom, window: Interval, tz: &TimeZone) -> Vec<Tick> {
    let (unit, increment) = match zoom {
        Zoom::Hours => (Unit::Minute, 15),
        Zoom::Day => (Unit::Hour, 1),
        Zoom::Week => (Unit::Hour, 6),
        Zoom::Month => (Unit::Day, 1),
    };
    let round = ZonedRound::new()
        .smallest(unit)
        .increment(increment)
        .mode(RoundMode::Trunc);

    let Ok(mut zoned) = window.start.to_zoned(tz.clone()).round(round) else {
        return Vec::new();
    };

    let mut ticks = Vec::new();
    while zoned.timestamp() <= window.end {
        let Ok(next) = step(&zoned, zoom) else {
            break;
        };
        let major = is_major(&zoned, zoom);
        // A week view label names the whole day, not the 6 hours until the
        // next line.
        let end = if zoom == Zoom::Week && major {
            zoned
                .tomorrow()
                .and_then(|day| day.start_of_day())
                .unwrap_or_else(|_| next.clone())
        } else {
            next.clone()
        };
        ticks.push(Tick {
            at: zoned.timestamp(),
            span: end.timestamp().duration_since(zoned.timestamp()),
            major,
            label: tick_label(&zoned, zoom),
        });
        zoned = next;
    }

    ticks
}

fn step(zoned: &Zoned, zoom: Zoom) -> Result<Zoned, jiff::Error> {
    match zoom {
        Zoom::Hours => zoned.checked_add(15.minutes()),
        Zoom::Day => zoned.checked_add(1.hour()),
        Zoom::Week => zoned.checked_add(6.hours()),
        Zoom::Month => zoned.checked_add(1.day()),
    }
}

fn is_major(zoned: &Zoned, zoom: Zoom) -> bool {
    match zoom {
        Zoom::Hours => zoned.minute() == 0,
        Zoom::Day => zoned.hour() % 3 == 0,
        Zoom::Week => zoned.hour() == 0,
        Zoom::Month => zoned.weekday() == jiff::civil::Weekday::Monday,
    }
}

fn tick_label(zoned: &Zoned, zoom: Zoom) -> Option<String> {
    let day = || format!("{} {}", label::weekday(zoned.weekday()), zoned.day());
    match zoom {
        Zoom::Hours if zoned.minute() == 0 => Some(if zoned.hour() == 0 {
            day()
        } else {
            format!("{:02}:00", zoned.hour())
        }),
        Zoom::Day if zoned.hour() % 3 == 0 => Some(if zoned.hour() == 0 {
            day()
        } else {
            format!("{:02}:00", zoned.hour())
        }),
        Zoom::Week if zoned.hour() == 0 => Some(day()),
        Zoom::Month => Some(if zoned.day() == 1 {
            format!("1 {}", label::month(zoned.month()))
        } else {
            zoned.day().to_string()
        }),
        _ => None,
    }
}
