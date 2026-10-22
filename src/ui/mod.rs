//! The iced application: state, messages and layout.

mod detail;
mod label;
mod live;
mod musicbox;
mod palette;
mod scale;

use std::sync::Arc;
use std::time::Duration;

use iced::widget::canvas::{Cache, Canvas};
use iced::widget::{
    Row, button, column, container, responsive, row, scrollable, space, stack, text,
};
use iced::{Element, Length, Subscription, Task, Theme};
use jiff::Timestamp;
use jiff::tz::TimeZone;

use crate::host::Host;
use crate::model::{Scope, Snapshot};
use crate::systemd;
use crate::timeline;
use musicbox::{Layer, MusicBox, Row as Track};
use scale::Zoom;

/// Polling interval without live updates.
const REFRESH: Duration = Duration::from_secs(30);
/// Safety net poll while live updates run, for anything signals miss.
const REFRESH_LIVE: Duration = Duration::from_secs(300);

pub fn run(host: Arc<dyn Host>) -> iced::Result {
    iced::application(move || App::new(host.clone()), App::update, App::view)
        .title(App::title)
        .subscription(App::subscription)
        .theme(App::theme)
        .window_size((1280.0, 760.0))
        .antialiasing(true)
        .run()
}

#[derive(Debug, Clone)]
pub enum Message {
    Tick,
    Refresh,
    Loaded(Result<Arc<Snapshot>, String>),
    SetZoom(Zoom),
    SetScope(Scope),
    Select(usize),
    Deselect,
    Live(live::Event),
}

struct App {
    host: Arc<dyn Host>,
    scope: Scope,
    zoom: Zoom,
    tz: TimeZone,
    now: Timestamp,
    snapshot: Option<Arc<Snapshot>>,
    tracks: Vec<Track>,
    /// Timer id, so the selection survives refreshes that reorder rows.
    selected: Option<String>,
    loading: bool,
    /// Scope or zoom changed while loading; the result is outdated.
    stale: bool,
    error: Option<String>,
    live: live::State,
    cache: Cache,
}

impl App {
    fn new(host: Arc<dyn Host>) -> (Self, Task<Message>) {
        let mut app = Self {
            host,
            scope: Scope::default(),
            zoom: Zoom::default(),
            tz: TimeZone::system(),
            now: Timestamp::now(),
            snapshot: None,
            tracks: Vec::new(),
            selected: None,
            loading: false,
            stale: false,
            error: None,
            live: live::State::default(),
            cache: Cache::new(),
        };
        let task = app.load();
        (app, task)
    }

    fn title(&self) -> String {
        format!("sysorbit · {}", self.host.name())
    }

    fn theme(&self) -> Theme {
        palette::theme()
    }

    fn subscription(&self) -> Subscription<Message> {
        let refresh = match self.live {
            live::State::Live => REFRESH_LIVE,
            live::State::Connecting | live::State::Lost(_) => REFRESH,
        };
        // Watching needs the unit names, so it starts with the first snapshot.
        let watch = self
            .snapshot
            .as_ref()
            .filter(|snapshot| snapshot.scope == self.scope)
            .map(|snapshot| live::subscription(&self.host, snapshot))
            .unwrap_or_else(Subscription::none);

        Subscription::batch([
            iced::time::every(self.zoom.tick()).map(|_| Message::Tick),
            iced::time::every(refresh).map(|_| Message::Refresh),
            watch,
        ])
    }

    fn load(&mut self) -> Task<Message> {
        if self.loading {
            self.stale = true;
            return Task::none();
        }
        self.loading = true;
        self.stale = false;

        let host = self.host.clone();
        let scope = self.scope;
        let window = self.zoom.fetch_window(Timestamp::now());
        Task::perform(
            async move {
                systemd::snapshot(host.as_ref(), scope, window)
                    .await
                    .map(Arc::new)
                    .map_err(|error| error.to_string())
            },
            Message::Loaded,
        )
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Tick => {
                self.now = Timestamp::now();
                self.rebuild();
                Task::none()
            }
            Message::Refresh => self.load(),
            Message::Loaded(result) => {
                self.loading = false;
                match result {
                    Ok(snapshot) => {
                        self.error = None;
                        self.snapshot = Some(snapshot);
                        self.now = Timestamp::now();
                        self.rebuild();
                    }
                    Err(error) => self.error = Some(error),
                }
                if self.stale {
                    self.load()
                } else {
                    Task::none()
                }
            }
            Message::SetZoom(zoom) => {
                self.zoom = zoom;
                self.rebuild();
                self.load()
            }
            Message::SetScope(scope) => {
                self.scope = scope;
                self.live = live::State::default();
                self.selected = None;
                self.snapshot = None;
                self.rebuild();
                self.load()
            }
            Message::Select(index) => {
                let id = self.tracks.get(index).map(|track| track.timer.id.clone());
                self.selected = if self.selected == id { None } else { id };
                self.cache.clear();
                Task::none()
            }
            Message::Deselect => {
                self.selected = None;
                self.cache.clear();
                Task::none()
            }
            Message::Live(event) => match event {
                live::Event::Connected => {
                    self.live = live::State::Live;
                    Task::none()
                }
                live::Event::Changed => self.load(),
                live::Event::Lost(reason) => {
                    self.live = live::State::Lost(reason);
                    Task::none()
                }
            },
        }
    }

    /// Recomputes marks: whether an elapse is planned or skipped depends on now.
    fn rebuild(&mut self) {
        self.cache.clear();
        let Some(snapshot) = self.snapshot.as_ref().filter(|s| s.scope == self.scope) else {
            self.tracks.clear();
            return;
        };

        let mut tracks: Vec<Track> = snapshot
            .timers
            .iter()
            .map(|timer| {
                let mut subtitle = label::schedule(timer);
                if !timer.active {
                    subtitle = format!("{} · {subtitle}", timer.unit_file_state);
                }
                Track {
                    title: timer.name().to_owned(),
                    subtitle,
                    marks: timeline::marks(
                        timer,
                        &snapshot.downtime,
                        snapshot.history_known,
                        self.now,
                    ),
                    timer: timer.clone(),
                }
            })
            .collect();

        // Stable order: rows must not jump around as time passes.
        tracks.sort_by(|a, b| {
            b.timer
                .active
                .cmp(&a.timer.active)
                .then_with(|| a.title.cmp(&b.title))
        });
        self.tracks = tracks;
    }

    fn view(&self) -> Element<'_, Message> {
        let body: Element<'_, Message> = if self.tracks.is_empty() {
            let message = match (&self.error, self.loading) {
                (Some(error), _) => error.clone(),
                (None, true) => "Reading timers…".to_owned(),
                (None, false) => "No timers in this scope.".to_owned(),
            };
            container(text(message).color(palette::MUTED))
                .center(Length::Fill)
                .into()
        } else {
            let selected = self
                .selected
                .as_ref()
                .and_then(|id| self.tracks.iter().position(|track| &track.timer.id == id));
            let downtime = self
                .snapshot
                .as_ref()
                .map(|snapshot| snapshot.downtime.as_slice())
                .unwrap_or_default();

            let rows = musicbox::height(self.tracks.len());
            let music_box = responsive(move |size| {
                // Below the minimum width the box keeps its size and scrolls
                // sideways; scrolling content must not fill its scroll axis.
                let width = if size.width >= musicbox::MIN_WIDTH {
                    Length::Fill
                } else {
                    Length::Fixed(musicbox::MIN_WIDTH)
                };
                let layer = |layer| {
                    Canvas::new(MusicBox {
                        layer,
                        rows: &self.tracks,
                        downtime,
                        zoom: self.zoom,
                        now: self.now,
                        tz: &self.tz,
                        selected,
                        cache: &self.cache,
                    })
                    .width(width)
                    .height(rows)
                };
                let canvas = stack![layer(Layer::Content), layer(Layer::Overlay)];

                if size.width >= musicbox::MIN_WIDTH {
                    scrollable(canvas).into()
                } else {
                    scrollable(canvas)
                        .direction(scrollable::Direction::Both {
                            vertical: scrollable::Scrollbar::default(),
                            horizontal: scrollable::Scrollbar::default(),
                        })
                        .into()
                }
            });

            let mut content = Row::new().spacing(16).push(music_box);
            if let Some(index) = selected {
                content = content.push(detail::view(&self.tracks[index].timer, self.now, &self.tz));
            }
            content.into()
        };

        column![self.header(), body, self.status()]
            .spacing(12)
            .padding(16)
            .into()
    }

    fn header(&self) -> Element<'_, Message> {
        let scopes = segmented(
            [(Scope::System, "System"), (Scope::User, "User")],
            self.scope,
            Message::SetScope,
        );
        let zooms = segmented(
            Zoom::ALL.map(|zoom| (zoom, zoom.label())),
            self.zoom,
            Message::SetZoom,
        );
        let refresh = button(
            text(if self.loading {
                "Refreshing…"
            } else {
                "Refresh"
            })
            .size(13),
        )
        .style(button::secondary)
        .on_press_maybe((!self.loading).then_some(Message::Refresh));

        let title = row![
            text("sysorbit").size(22),
            text(self.host.name()).size(14).color(palette::MUTED),
        ]
        .spacing(16)
        .align_y(iced::Alignment::Center);

        // Only the controls wrap, so a fill spacer cannot push them down
        // while there is still room beside the title.
        let controls = row![scopes, zooms, refresh]
            .spacing(16)
            .align_y(iced::Alignment::Center)
            .wrap()
            .vertical_spacing(10);

        row![title, space::horizontal(), controls]
            .spacing(16)
            .align_y(iced::Alignment::Center)
            .into()
    }

    fn status(&self) -> Element<'_, Message> {
        let mut parts = Vec::new();
        if let Some(snapshot) = &self.snapshot {
            parts.push(format!(
                "{} timers · updated {}",
                snapshot.timers.len(),
                label::clock(snapshot.fetched_at, &self.tz)
            ));
            parts.extend(snapshot.warnings.iter().cloned());
        }
        if let (Some(error), false) = (&self.error, self.tracks.is_empty()) {
            parts.push(error.clone());
        }

        let (indicator, color) = match &self.live {
            live::State::Live => ("● live".to_owned(), palette::CATCH_UP),
            live::State::Connecting => ("○ connecting".to_owned(), palette::MUTED),
            live::State::Lost(reason) => (
                format!(
                    "○ polling every {}s, live updates unavailable: {reason}",
                    REFRESH.as_secs()
                ),
                palette::NOTE,
            ),
        };

        row![
            text(indicator).size(12).color(color),
            text(parts.join("  ·  ")).size(12).color(palette::MUTED),
        ]
        .spacing(12)
        .into()
    }
}

/// A row of buttons where the current choice is highlighted.
fn segmented<'a, T, const N: usize>(
    options: [(T, &'a str); N],
    current: T,
    on_press: fn(T) -> Message,
) -> Element<'a, Message>
where
    T: Copy + PartialEq + 'a,
{
    let buttons = options.map(|(value, name)| {
        button(text(name).size(13))
            .style(if value == current {
                button::primary
            } else {
                button::secondary
            })
            .on_press(on_press(value))
            .into()
    });

    Row::from_iter(buttons).spacing(4).into()
}
