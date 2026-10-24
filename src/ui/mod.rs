//! The iced application: state, messages and layout.

mod clock;
mod composer;
mod controls;
mod drawer;
mod explain;
mod host_form;
mod label;
mod live;
mod musicbox;
mod overview;
mod palette;
mod rhythm;
mod scale;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use iced::keyboard::{self, key::Named};
use iced::widget::canvas::{Cache, Canvas};
use iced::widget::pane_grid::{self, PaneGrid};
use iced::widget::{
    Column, center, column, container, mouse_area, opaque, pin, responsive, row, scrollable, space,
    stack, text,
};
use iced::{Element, Length, Subscription, Task, Theme};
use jiff::tz::TimeZone;
use jiff::{SignedDuration, Timestamp};

use crate::diagnostics::{self, Event, Phase, Redacted};
use crate::host::Host;
use crate::hosts::{self, HostEntry, Origin};
use crate::i18n::fl;
use crate::model::{LogLine, Outcome, Scope, Snapshot};
use crate::ssh::SshHost;
use crate::systemd;
use crate::timeline::{self, Mark};
use composer::{Composer, ComposerMessage, Schedule};
use drawer::{Drawer, Logs, Tab};
use host_form::{Field, FormMessage, HostForm, Test};
use musicbox::{Layer, MusicBox, Row as Track};
use scale::Zoom;

/// How long a track lights up after its service started or failed.
const FLASH: Duration = Duration::from_millis(2500);

/// Most the music box takes while a timer is composed below it: the
/// header and three tracks.
const MUSIC_BOX_WHILE_COMPOSING: f32 = 250.0;
/// The window's size when the app starts.
const WINDOW: (f32, f32) = (1280.0, 760.0);
/// Height below the body: the status line, the spacing above it and the
/// window's padding.
const BELOW_BODY: f32 = 68.0;

/// Where the body below the header starts, measured from the window's top.
const BODY_TOP: f32 = HEADER_BOTTOM + 12.0;

/// Typing pause before the composed schedule is sent to systemd.
const SCHEDULE_PAUSE: Duration = Duration::from_millis(300);

/// How long a connection test may take before it counts as failed.
const TEST_TIMEOUT: Duration = Duration::from_secs(20);

/// Where the header ends, measured from the window's top: the host menu
/// opens below it.
const HEADER_BOTTOM: f32 = 54.0;

/// Polling interval without live updates.
const REFRESH: Duration = Duration::from_secs(30);
/// Safety net poll while live updates run, for anything signals miss.
const REFRESH_LIVE: Duration = Duration::from_secs(300);

/// Runs the app showing `hosts[selected]`, or nothing when there are no
/// hosts yet. `problems` are troubles reading the host files.
pub fn run(hosts: Vec<HostEntry>, selected: Option<usize>, problems: Vec<String>) -> iced::Result {
    iced::application(
        move || App::new(hosts.clone(), selected, problems.clone()),
        App::update,
        App::view,
    )
    .window(iced::window::Settings {
        #[cfg(target_os = "linux")]
        platform_specific: iced::window::settings::PlatformSpecific {
            application_id: "io.github.estnml.sysorbit".to_owned(),
            ..Default::default()
        },
        ..Default::default()
    })
    .title(App::title)
    .subscription(App::subscription)
    .theme(App::theme)
    .window_size(WINDOW)
    .antialiasing(true)
    .run()
}

#[derive(Debug, Clone)]
pub enum Message {
    Tick,
    Refresh,
    Poll,
    /// A snapshot for the host connection of the given generation.
    Loaded(u64, Result<Arc<Snapshot>, String>),
    ToggleHostMenu,
    SelectHost(usize),
    /// A frame is due while tracks are lighting up.
    Frame,
    /// Opens the host dialog for `hosts[index]`, or for a new host.
    OpenForm(Option<usize>),
    Form(FormMessage),
    /// Opens the composer for a new timer on the shown host.
    OpenComposer,
    Composer(ComposerMessage),
    SetZoom(Zoom),
    SetScope(Scope),
    /// A track was clicked.
    Select(usize),
    /// A note on a track, i.e. one run, was clicked.
    SelectRun(usize, Timestamp),
    /// Another mark on a track, by its anchor, was clicked.
    Focus(usize, Timestamp),
    /// A run was picked in the drawer's run list.
    OpenRun(Timestamp),
    Navigate(Nav),
    SetTab(Tab),
    Deselect,
    LogsLoaded(String, Result<Arc<Vec<LogLine>>, String>),
    ResizeDrawer(pane_grid::ResizeEvent),
    /// The window's new height.
    WindowHeight(f32),
    Live(live::Event),
}

/// Keyboard moves through the selected timer's marks and the timers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Nav {
    Previous,
    Next,
    First,
    Last,
    Up,
    Down,
    /// Opens the logs of the focused run.
    Open,
    /// Leaves the focused mark, then the timer.
    Back,
}

#[derive(Debug, Clone, Copy)]
enum Pane {
    MusicBox,
    Drawer,
}

struct App {
    hosts: Vec<HostEntry>,
    /// Index of the host shown in `hosts`.
    selected: Option<usize>,
    host: Option<Arc<dyn Host>>,
    /// Counts host switches, so a snapshot of the previous host that
    /// arrives late is dropped.
    generation: u64,
    host_menu: bool,
    host_problems: Vec<String>,
    /// The host dialog, while open.
    form: Option<HostForm>,
    /// A new timer being composed in the drawer, drafted in the music box.
    composer: Option<Composer>,
    /// The window's height, to fit the music box above the composer.
    window_height: f32,
    /// How many tracks the split was fitted to while composing; `None`
    /// once the split was dragged, which then stays as it is.
    fitted: Option<usize>,
    /// Timers whose service just started or failed, by id: when, and in
    /// which color their track lights up.
    flashes: HashMap<String, (Instant, iced::Color)>,
    scope: Scope,
    zoom: Zoom,
    /// The time zone times are shown in: the host's when it is remote and
    /// reports one, else this machine's.
    tz: TimeZone,
    /// The host's clock minus this machine's; zero for this machine.
    offset: SignedDuration,
    now: Timestamp,
    snapshot: Option<Arc<Snapshot>>,
    tracks: Vec<Track>,
    /// The drawer below the music box, open while a timer is selected.
    drawer: Option<Drawer>,
    /// Split between music box and drawer; keeps its ratio while closed.
    panes: pane_grid::State<Pane>,
    loading: bool,
    /// Scope or zoom changed while loading; the result is outdated.
    stale: bool,
    error: Option<String>,
    live: live::State,
    cache: Cache,
}

impl App {
    fn new(
        hosts: Vec<HostEntry>,
        selected: Option<usize>,
        host_problems: Vec<String>,
    ) -> (Self, Task<Message>) {
        let host = selected.map(|index| hosts[index].connect());
        let mut app = Self {
            hosts,
            selected,
            host,
            generation: 0,
            host_menu: false,
            host_problems,
            form: None,
            composer: None,
            window_height: WINDOW.1,
            fitted: None,
            flashes: HashMap::new(),
            scope: Scope::default(),
            zoom: Zoom::default(),
            tz: TimeZone::system(),
            offset: SignedDuration::ZERO,
            now: Timestamp::now(),
            snapshot: None,
            tracks: Vec::new(),
            drawer: None,
            panes: pane_grid::State::with_configuration(pane_grid::Configuration::Split {
                axis: pane_grid::Axis::Horizontal,
                ratio: 0.6,
                a: Box::new(pane_grid::Configuration::Pane(Pane::MusicBox)),
                b: Box::new(pane_grid::Configuration::Pane(Pane::Drawer)),
            }),
            loading: false,
            stale: false,
            error: None,
            live: live::State::default(),
            cache: Cache::new(),
        };
        if let Some(host) = &app.host {
            diagnostics::host_event(Event::HostSwitch, Redacted::host(host.name()));
        }
        let task = app.load();
        (app, task)
    }

    fn title(&self) -> String {
        match self.entry() {
            Some(entry) => format!("sysorbit · {}", entry.name),
            None => "sysorbit".to_owned(),
        }
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
            .zip(self.host.as_ref())
            .map(|(snapshot, host)| live::subscription(host, snapshot))
            .unwrap_or_else(Subscription::none);

        Subscription::batch([
            clock::subscription(self.zoom.tick()),
            iced::time::every(refresh).map(|_| Message::Poll),
            keyboard::listen().filter_map(navigation),
            // Frames only while a track lights up.
            if self.flashes.is_empty() {
                Subscription::none()
            } else {
                iced::window::frames().map(|_| Message::Frame)
            },
            // Coming back to the window, e.g. after a suspend, shows the time.
            iced::event::listen_with(|event, _, _| match event {
                iced::Event::Window(iced::window::Event::Focused) => Some(Message::Tick),
                _ => None,
            }),
            watch,
            iced::window::resize_events().map(|(_, size)| Message::WindowHeight(size.height)),
        ])
    }

    fn load(&mut self) -> Task<Message> {
        let Some(host) = self.host.clone() else {
            return Task::none();
        };
        if self.loading {
            self.stale = true;
            return Task::none();
        }
        self.loading = true;
        self.stale = false;

        let scope = self.scope;
        let window = self.zoom.fetch_window(self.host_now());
        let generation = self.generation;
        Task::perform(
            async move {
                systemd::snapshot(host.as_ref(), scope, window)
                    .await
                    .map(Arc::new)
                    .map_err(|error| error.to_string())
            },
            move |result| Message::Loaded(generation, result),
        )
    }

    fn entry(&self) -> Option<&HostEntry> {
        self.selected.map(|index| &self.hosts[index])
    }

    /// Shows another host: a new connection, and nothing kept from the
    /// previous host's timers, clock or selection.
    fn select_host(&mut self, index: usize) -> Task<Message> {
        self.host_menu = false;
        if self.selected == Some(index) || index >= self.hosts.len() {
            return Task::none();
        }
        let entry = &self.hosts[index];
        if entry.origin != Origin::CommandLine {
            hosts::remember(&entry.id);
        }
        self.host = Some(entry.connect());
        if let Some(host) = &self.host {
            diagnostics::host_event(Event::HostSwitch, Redacted::host(host.name()));
        }
        self.selected = Some(index);
        self.composer = None;
        self.generation += 1;
        self.snapshot = None;
        self.tracks.clear();
        self.drawer = None;
        self.flashes.clear();
        self.offset = SignedDuration::ZERO;
        self.tz = TimeZone::system();
        self.error = None;
        self.live = live::State::default();
        self.loading = false;
        self.stale = false;
        self.cache.clear();
        self.load()
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Tick => {
                self.now = self.host_now();
                self.rebuild();
                Task::none()
            }
            Message::Refresh => {
                diagnostics::event(Event::Refresh);
                self.load()
            }
            Message::Poll => {
                diagnostics::event(Event::PollRefresh);
                self.load()
            }
            Message::Loaded(generation, _) if generation != self.generation => Task::none(),
            Message::Loaded(_, result) => {
                self.loading = false;
                match result {
                    Ok(snapshot) => {
                        self.error = None;
                        if let Some(old) = self
                            .snapshot
                            .as_ref()
                            .filter(|old| old.scope == snapshot.scope)
                        {
                            let now = Instant::now();
                            for (id, color) in flashes(old, &snapshot) {
                                self.flashes.insert(id, (now, color));
                            }
                        }
                        if let Some(clock) = &snapshot.clock {
                            self.offset = clock.offset;
                            self.tz = clock
                                .zone
                                .as_deref()
                                .and_then(|zone| TimeZone::get(zone).ok())
                                .unwrap_or_else(TimeZone::system);
                        }
                        self.snapshot = Some(snapshot);
                        self.now = self.host_now();
                        self.rebuild();
                    }
                    Err(error) => self.error = Some(error),
                }
                // Logs of a run still going grow with every refresh.
                let logs = self.ensure_logs(true);
                if self.stale {
                    Task::batch([self.load(), logs])
                } else {
                    logs
                }
            }
            Message::SetZoom(zoom) => {
                diagnostics::zoom_change(zoom.span().as_secs());
                self.zoom = zoom;
                self.rebuild();
                let draft = match self.composer.as_ref().map(|composer| &composer.schedule) {
                    Some(Schedule::Ready(_) | Schedule::Checking) => self.preview_schedule(),
                    _ => Task::none(),
                };
                Task::batch([self.load(), draft])
            }
            Message::SetScope(scope) => {
                diagnostics::scope_change(scope);
                self.scope = scope;
                self.live = live::State::default();
                self.drawer = None;
                self.snapshot = None;
                self.rebuild();
                self.load()
            }
            Message::Select(_) | Message::SelectRun(..) | Message::Focus(..)
                if self.composer.is_some() =>
            {
                Task::none()
            }
            Message::Select(index) => {
                let Some(id) = self.tracks.get(index).map(|track| track.timer.id.clone()) else {
                    return Task::none();
                };
                self.drawer = match self.drawer.take() {
                    // Clicking the open timer again closes the drawer.
                    Some(drawer) if drawer.timer == id => None,
                    // Another timer keeps the tab, but not the picked run.
                    Some(drawer) => Some(Drawer {
                        tab: drawer.tab,
                        ..Drawer::new(id)
                    }),
                    None => Some(Drawer::new(id)),
                };
                self.cache.clear();
                self.ensure_logs(false)
            }
            Message::SelectRun(index, start) => {
                let Some(id) = self.tracks.get(index).map(|track| track.timer.id.clone()) else {
                    return Task::none();
                };
                let mut drawer = Drawer::new(id);
                drawer.tab = Tab::Logs;
                drawer.run = Some(start);
                drawer.focus = Some(start);
                self.drawer = Some(drawer);
                self.cache.clear();
                self.ensure_logs(false)
            }
            Message::Focus(index, at) => {
                let Some(id) = self.tracks.get(index).map(|track| track.timer.id.clone()) else {
                    return Task::none();
                };
                if self.drawer.as_ref().is_none_or(|drawer| drawer.timer != id) {
                    let tab = self.drawer.as_ref().map(|drawer| drawer.tab);
                    self.drawer = Some(Drawer {
                        tab: tab.unwrap_or_default(),
                        ..Drawer::new(id)
                    });
                    self.cache.clear();
                }
                self.focus(at)
            }
            Message::OpenRun(start) => {
                if let Some(drawer) = &mut self.drawer {
                    drawer.tab = Tab::Logs;
                    drawer.run = Some(start);
                    drawer.focus = Some(start);
                }
                self.cache.clear();
                self.ensure_logs(false)
            }
            Message::Navigate(Nav::Back) if self.form.is_some() => {
                diagnostics::event(Event::DialogClose);
                self.form = None;
                Task::none()
            }
            Message::Navigate(Nav::Back) if self.composer.is_some() => {
                self.update_composer(ComposerMessage::Cancel)
            }
            Message::Navigate(Nav::Back) if self.host_menu => {
                self.host_menu = false;
                Task::none()
            }
            // Keys go to the dialog's fields, not to the music box behind it.
            Message::Navigate(_) if self.form.is_some() || self.composer.is_some() => Task::none(),
            Message::Navigate(nav) => self.navigate(nav),
            Message::Frame => {
                self.flashes
                    .retain(|_, (started, _)| started.elapsed() < FLASH);
                Task::none()
            }
            Message::ToggleHostMenu => {
                self.host_menu = !self.host_menu;
                Task::none()
            }
            Message::SelectHost(index) => self.select_host(index),
            Message::OpenForm(index) => {
                diagnostics::event(Event::DialogOpen);
                self.host_menu = false;
                self.form = Some(match index.and_then(|index| self.hosts.get(index)) {
                    Some(entry) => HostForm::for_entry(entry),
                    None => HostForm::new(),
                });
                Task::none()
            }
            Message::Form(message) => self.update_form(message),
            Message::OpenComposer => {
                let Some(host) = &self.host else {
                    return Task::none();
                };
                if self.composer.is_none() {
                    diagnostics::event(Event::DialogOpen);
                    self.host_menu = false;
                    self.drawer = None;
                    self.composer = Some(Composer::new(host.is_local()));
                    self.fitted = Some(0);
                    self.rebuild();
                }
                Task::none()
            }
            Message::Composer(message) => self.update_composer(message),
            Message::SetTab(tab) => {
                if let Some(drawer) = &mut self.drawer {
                    drawer.tab = tab;
                }
                self.ensure_logs(false)
            }
            Message::Deselect => {
                self.drawer = None;
                self.cache.clear();
                Task::none()
            }
            Message::LogsLoaded(invocation, result) => {
                if let Some(drawer) = self
                    .drawer
                    .as_mut()
                    .filter(|drawer| drawer.logs.invocation() == Some(invocation.as_str()))
                {
                    drawer.logs = match result {
                        Ok(lines) => Logs::Loaded { invocation, lines },
                        Err(error) => Logs::Failed { invocation, error },
                    };
                }
                Task::none()
            }
            Message::WindowHeight(height) => {
                self.window_height = height;
                Task::none()
            }
            Message::ResizeDrawer(event) => {
                if self.composer.is_some() {
                    self.fitted = None;
                }
                self.panes.resize(event.split, event.ratio.clamp(0.2, 0.85));
                Task::none()
            }
            Message::Live(event) => match event {
                live::Event::Connected => {
                    self.live = live::State::Live;
                    Task::none()
                }
                live::Event::Changed => {
                    diagnostics::event(Event::LiveRefresh);
                    self.load()
                }
                live::Event::Lost(reason) => {
                    self.live = live::State::Lost(reason);
                    Task::none()
                }
            },
        }
    }

    /// The host's current time, which for a remote host may differ from
    /// this machine's clock.
    fn host_now(&self) -> Timestamp {
        Timestamp::now() + self.offset
    }

    /// Index of the selected timer's track.
    fn selected(&self) -> Option<usize> {
        let drawer = self.drawer.as_ref()?;
        self.tracks
            .iter()
            .position(|track| track.timer.id == drawer.timer)
    }

    fn navigate(&mut self, nav: Nav) -> Task<Message> {
        let Some(index) = self.selected() else {
            return Task::none();
        };
        let Some(drawer) = self.drawer.as_mut() else {
            return Task::none();
        };

        match nav {
            Nav::Up | Nav::Down => {
                let target = match nav {
                    Nav::Up => index.checked_sub(1),
                    _ => Some(index + 1).filter(|&next| next < self.tracks.len()),
                };
                let Some(track) = target.and_then(|target| self.tracks.get(target)) else {
                    return Task::none();
                };
                *drawer = Drawer {
                    tab: drawer.tab,
                    ..Drawer::new(track.timer.id.clone())
                };
                self.cache.clear();
                self.ensure_logs(false)
            }
            Nav::Back => {
                if drawer.focus.take().is_none() {
                    self.drawer = None;
                    self.cache.clear();
                }
                Task::none()
            }
            Nav::Open => match drawer.focus {
                Some(at)
                    if self.tracks[index]
                        .marks
                        .iter()
                        .any(|mark| is_run_at(mark, at)) =>
                {
                    self.update(Message::OpenRun(at))
                }
                _ => Task::none(),
            },
            Nav::Previous | Nav::Next | Nav::First | Nav::Last => {
                let visible = self.zoom.visible(self.now);
                let mut anchors: Vec<Timestamp> = self.tracks[index]
                    .marks
                    .iter()
                    .filter(|mark| musicbox::focusable(mark))
                    .map(Mark::anchor)
                    .filter(|&at| visible.contains(at))
                    .collect();
                anchors.dedup();
                let Some(last) = anchors.len().checked_sub(1) else {
                    return Task::none();
                };
                // Without a focused mark, the arrows start from now.
                let current = drawer
                    .focus
                    .and_then(|focus| anchors.iter().position(|&at| at == focus));
                let target = match (nav, current) {
                    (Nav::First, _) => 0,
                    (Nav::Last, _) => last,
                    (Nav::Previous, Some(current)) => current.saturating_sub(1),
                    (_, Some(current)) => (current + 1).min(last),
                    (Nav::Previous, None) => {
                        anchors.iter().rposition(|&at| at <= self.now).unwrap_or(0)
                    }
                    (_, None) => anchors.iter().position(|&at| at > self.now).unwrap_or(last),
                };
                self.focus(anchors[target])
            }
        }
    }

    /// Focuses the mark at `at` on the selected track; a run also becomes
    /// the drawer's run.
    fn focus(&mut self, at: Timestamp) -> Task<Message> {
        let is_run = self.selected().is_some_and(|index| {
            self.tracks[index]
                .marks
                .iter()
                .any(|mark| is_run_at(mark, at))
        });
        let Some(drawer) = self.drawer.as_mut() else {
            return Task::none();
        };
        drawer.focus = Some(at);
        if is_run && drawer.run != Some(at) {
            drawer.run = Some(at);
            self.cache.clear();
            return self.ensure_logs(false);
        }
        Task::none()
    }

    /// Fetches the logs of the run shown in the logs tab when they are not
    /// loaded yet. `refresh` reloads them if that run is still going.
    fn ensure_logs(&mut self, refresh: bool) -> Task<Message> {
        let Some(drawer) = self
            .drawer
            .as_mut()
            .filter(|drawer| drawer.tab == Tab::Logs)
        else {
            return Task::none();
        };
        let Some(track) = self
            .tracks
            .iter()
            .find(|track| track.timer.id == drawer.timer)
        else {
            return Task::none();
        };
        let Some(run) = drawer.selected_run(&track.timer) else {
            drawer.logs = Logs::None;
            return Task::none();
        };
        let Some(invocation) = run.invocation.clone() else {
            drawer.logs = Logs::None;
            return Task::none();
        };

        let current = drawer.logs.invocation() == Some(invocation.as_str());
        if current && !(refresh && run.outcome == Outcome::Running) {
            return Task::none();
        }
        // A refresh keeps showing the old lines until the new ones arrive.
        if !current {
            drawer.logs = Logs::Loading {
                invocation: invocation.clone(),
            };
        }

        let Some(host) = self.host.clone() else {
            return Task::none();
        };
        let scope = self.scope;
        Task::perform(
            async move {
                let result = systemd::logs(host.as_ref(), scope, &invocation)
                    .await
                    .map(Arc::new)
                    .map_err(|error| error.to_string());
                (invocation, result)
            },
            |(invocation, result)| Message::LogsLoaded(invocation, result),
        )
    }

    /// Recomputes marks: whether an elapse is planned or skipped depends on now.
    fn rebuild(&mut self) {
        let _span = diagnostics::span(Phase::Rebuild).entered();
        self.cache.clear();
        let snapshot = self.snapshot.as_ref().filter(|s| s.scope == self.scope);
        let mut tracks: Vec<Track> = snapshot
            .map(|snapshot| snapshot.timers.as_slice())
            .unwrap_or_default()
            .iter()
            .map(|timer| {
                let mut subtitle = label::schedule(timer);
                if !timer.active {
                    subtitle = format!(
                        "{} · {subtitle}",
                        label::unit_file_state(&timer.unit_file_state)
                    );
                }
                Track {
                    title: timer.name().to_owned(),
                    subtitle,
                    marks: snapshot.map_or_else(Vec::new, |snapshot| {
                        timeline::marks(timer, &snapshot.downtime, snapshot.history_known, self.now)
                    }),
                    timer: timer.clone(),
                    draft: false,
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
        diagnostics::counts(
            tracks.len(),
            tracks.iter().map(|track| track.timer.runs.len()).sum(),
            tracks.iter().map(|track| track.marks.len()).sum(),
            0,
        );

        // The draft leads, so it is in view while the schedule is typed.
        if let Some(composer) = &self.composer {
            let timer = composer.draft(self.now);
            let marks = timer.as_ref().map_or_else(Vec::new, |timer| {
                // It never ran, so only what is ahead is drawn.
                timeline::marks(timer, &[], false, self.now)
            });
            tracks.insert(
                0,
                Track {
                    title: composer.title(),
                    subtitle: composer.subtitle(),
                    marks,
                    timer: timer.unwrap_or_else(composer::placeholder),
                    draft: true,
                },
            );
        }
        self.tracks = tracks;

        // Timers may arrive or the scope change while composing.
        if self.composer.is_some() && self.fitted.is_some_and(|rows| rows != self.tracks.len()) {
            self.fit_music_box();
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let body: Element<'_, Message> = if self.tracks.is_empty() {
            let message = match (&self.error, self.loading) {
                _ if self.host.is_none() => fl!(
                    "no-hosts",
                    path = hosts::hosts_file()
                        .map(|path| path.display().to_string())
                        .unwrap_or_default()
                ),
                (Some(error), _) => error.clone(),
                (None, true) => fl!("reading-timers"),
                (None, false) => fl!("no-timers"),
            };
            container(text(message).color(palette::MUTED))
                .center(Length::Fill)
                .into()
        } else if let Some(composer) = &self.composer {
            PaneGrid::new(&self.panes, move |_, pane, _| {
                pane_grid::Content::new(match pane {
                    Pane::MusicBox => self.music_box(),
                    Pane::Drawer => composer.view(self.scope, self.now, &self.tz),
                })
            })
            .spacing(12)
            .on_resize(12, Message::ResizeDrawer)
            .into()
        } else {
            let open = self.drawer.as_ref().and_then(|drawer| {
                let index = self
                    .tracks
                    .iter()
                    .position(|track| track.timer.id == drawer.timer)?;
                Some((drawer, index))
            });
            match open {
                Some((drawer, index)) => PaneGrid::new(&self.panes, move |_, pane, _| {
                    pane_grid::Content::new(match pane {
                        Pane::MusicBox => self.music_box(),
                        Pane::Drawer => {
                            drawer::view(drawer, &self.tracks[index].timer, self.now, &self.tz)
                        }
                    })
                })
                .spacing(12)
                .on_resize(12, Message::ResizeDrawer)
                .into(),
                None => self.music_box(),
            }
        };

        let content = column![self.header(), body, self.status()]
            .spacing(12)
            .padding(16);
        if let Some(form) = &self.form {
            // The dialog dims the window and keeps clicks from reaching it.
            let shade = container(space::horizontal())
                .width(Length::Fill)
                .height(Length::Fill)
                .style(|_| container::Style {
                    background: Some(palette::BACKGROUND.scale_alpha(0.7).into()),
                    ..container::Style::default()
                });
            return stack![content, opaque(shade), center(opaque(form.view()))].into();
        }
        if !self.host_menu {
            return content.into();
        }
        // A click anywhere outside the menu closes it.
        let backdrop = mouse_area(space::horizontal().width(Length::Fill).height(Length::Fill))
            .on_press(Message::ToggleHostMenu);
        stack![
            content,
            backdrop,
            pin(opaque(self.host_menu()))
                .x(16.0 + musicbox::PAD)
                .y(HEADER_BOTTOM),
        ]
        .into()
    }

    fn update_form(&mut self, message: FormMessage) -> Task<Message> {
        let Some(form) = self.form.as_mut() else {
            return Task::none();
        };
        match message {
            FormMessage::Edit(field) => {
                form.update(field);
                Task::none()
            }
            FormMessage::Browse => Task::perform(
                async {
                    rfd::AsyncFileDialog::new()
                        .set_title(fl!("form-browse-title"))
                        .pick_file()
                        .await
                        .map(|file| file.path().display().to_string())
                },
                |path| Message::Form(FormMessage::Picked(path)),
            ),
            FormMessage::Picked(Some(path)) => {
                form.update(Field::KeyFile(path));
                Task::none()
            }
            FormMessage::Picked(None) => Task::none(),
            FormMessage::Test => match form.host() {
                Err(error) => {
                    form.error = Some(error);
                    Task::none()
                }
                Ok(saved) => {
                    form.test_run += 1;
                    form.test = Test::Running;
                    let run = form.test_run;
                    let host = SshHost::new(saved.target());
                    Task::perform(
                        async move {
                            match tokio::time::timeout(TEST_TIMEOUT, systemd::probe(&host)).await {
                                Ok(result) => result.map_err(|error| error.to_string()),
                                Err(_) => Err(fl!("form-test-timeout")),
                            }
                        },
                        move |result| Message::Form(FormMessage::Tested(run, result)),
                    )
                }
            },
            FormMessage::Tested(run, result) => {
                if run == form.test_run {
                    form.test = match result {
                        Ok(probe) => Test::Passed(probe),
                        Err(error) => Test::Failed(error),
                    };
                }
                Task::none()
            }
            FormMessage::Save => {
                let saved = form
                    .host()
                    .and_then(|host| hosts::save(&host, form.editing.as_deref()));
                match saved {
                    Ok(id) => {
                        diagnostics::event(Event::DialogClose);
                        self.form = None;
                        self.reload_hosts(Some(&id))
                    }
                    Err(error) => {
                        form.error = Some(error);
                        Task::none()
                    }
                }
            }
            FormMessage::Delete => {
                let Some(id) = form.editing.clone() else {
                    return Task::none();
                };
                match hosts::delete(&id) {
                    Ok(()) => {
                        diagnostics::event(Event::DialogClose);
                        self.form = None;
                        self.reload_hosts(None)
                    }
                    Err(error) => {
                        form.error = Some(error);
                        Task::none()
                    }
                }
            }
            FormMessage::Cancel => {
                diagnostics::event(Event::DialogClose);
                self.form = None;
                Task::none()
            }
        }
    }

    fn update_composer(&mut self, message: ComposerMessage) -> Task<Message> {
        let Some(composer) = self.composer.as_mut() else {
            return Task::none();
        };
        match message {
            ComposerMessage::Edit(field) => {
                let reschedule = composer.update(field);
                self.rebuild();
                if reschedule {
                    self.preview_schedule()
                } else {
                    Task::none()
                }
            }
            ComposerMessage::Rhythm(edit) => {
                composer.edit(edit);
                self.preview_schedule()
            }
            ComposerMessage::Hint(hint) => {
                composer.set_hint(hint);
                Task::none()
            }
            ComposerMessage::Mode(mode) => {
                composer.set_mode(mode);
                self.preview_schedule()
            }
            ComposerMessage::Previewed(revision, Some(result)) if composer.is_current(revision) => {
                composer.schedule = match result {
                    Ok(previews) => Schedule::Ready(previews),
                    Err(reason) => Schedule::Invalid(reason),
                };
                self.rebuild();
                Task::none()
            }
            ComposerMessage::Previewed(..) => Task::none(),
            ComposerMessage::ToggleFiles => {
                composer.toggle_files();
                Task::none()
            }
            ComposerMessage::Create if composer.creating => Task::none(),
            ComposerMessage::Create => {
                let Some(host) = self.host.clone() else {
                    return Task::none();
                };
                let timer = match composer.timer() {
                    Ok(timer) => timer,
                    Err(error) => {
                        composer.error = Some(error);
                        return Task::none();
                    }
                };
                composer.creating = true;
                composer.error = None;
                let scope = self.scope;
                Task::perform(
                    async move {
                        systemd::create(host.as_ref(), scope, &timer)
                            .await
                            .map_err(|error| match error {
                                systemd::Error::UnitExists(name) => {
                                    fl!("timer-form-error-exists", name = name)
                                }
                                systemd::Error::PasswordRequired => {
                                    fl!("timer-form-error-password")
                                }
                                error => error.to_string(),
                            })
                    },
                    |result| Message::Composer(ComposerMessage::Created(result)),
                )
            }
            ComposerMessage::Created(Ok(())) => {
                diagnostics::event(Event::DialogClose);
                // The real timer takes the draft's place on the next snapshot.
                self.composer = None;
                self.rebuild();
                self.load()
            }
            ComposerMessage::Created(Err(error)) => {
                composer.creating = false;
                composer.error = Some(error);
                Task::none()
            }
            ComposerMessage::Cancel => {
                diagnostics::event(Event::DialogClose);
                self.composer = None;
                self.rebuild();
                Task::none()
            }
        }
    }

    /// Splits the body so the music box shows its tracks, up to a few,
    /// and the composer gets the rest. The split stays draggable.
    fn fit_music_box(&mut self) {
        self.fitted = Some(self.tracks.len());
        let body = self.window_height - BODY_TOP - BELOW_BODY;
        let music_box = musicbox::height(self.tracks.len(), false).min(MUSIC_BOX_WHILE_COMPOSING)
                // Half the split's spacing, and a little air.
                + 12.0;
        if let pane_grid::Node::Split { id, .. } = self.panes.layout() {
            self.panes.resize(*id, (music_box / body).clamp(0.2, 0.6));
        }
    }

    /// Asks systemd for the composed schedule's elapses in the fetched
    /// window, once changes pause; a preview superseded while waiting is
    /// never asked for.
    fn preview_schedule(&mut self) -> Task<Message> {
        let (Some(composer), Some(host)) = (self.composer.as_mut(), self.host.clone()) else {
            return Task::none();
        };
        let (revision, latest) = composer.next_revision();
        let expressions = composer.expressions();
        if expressions.is_empty() {
            composer.schedule = Schedule::Empty;
            self.rebuild();
            return Task::none();
        }
        composer.schedule = Schedule::Checking;
        self.rebuild();
        let capabilities = self
            .snapshot
            .as_ref()
            .map(|snapshot| snapshot.capabilities)
            .unwrap_or_default();
        let window = self.zoom.fetch_window(self.host_now());
        Task::perform(
            async move {
                tokio::time::sleep(SCHEDULE_PAUSE).await;
                if latest.load(std::sync::atomic::Ordering::SeqCst) != revision {
                    return None;
                }
                let result =
                    systemd::calendar::preview(host.as_ref(), &expressions, window, capabilities)
                        .await
                        .map_err(|error| match error {
                            systemd::Error::Command { stderr, .. } => stderr,
                            error => error.to_string(),
                        });
                Some(result)
            },
            move |result| Message::Composer(ComposerMessage::Previewed(revision, result)),
        )
    }

    /// Reads the host list again after the dialog changed `hosts.toml`.
    /// A saved host is shown, connecting anew with its settings; after a
    /// delete the current host stays, unless it was the one deleted.
    fn reload_hosts(&mut self, show: Option<&str>) -> Task<Message> {
        let current = self.entry().map(|entry| entry.id.clone());
        let command_line = self
            .hosts
            .iter()
            .find(|entry| entry.origin == Origin::CommandLine)
            .cloned();
        let (mut hosts, problems) = hosts::load();
        hosts.extend(command_line);
        self.hosts = hosts;
        self.host_problems = problems;

        let find = |id: &str| self.hosts.iter().position(|entry| entry.id == id);
        match (show.and_then(find), current.as_deref().and_then(find)) {
            (Some(index), _) => {
                // Forget the selection so that `select_host` reconnects.
                self.selected = None;
                self.select_host(index)
            }
            (None, Some(index)) => {
                self.selected = Some(index);
                Task::none()
            }
            (None, None) if self.hosts.is_empty() => {
                self.selected = None;
                self.host = None;
                self.generation += 1;
                self.snapshot = None;
                self.tracks.clear();
                self.drawer = None;
                self.cache.clear();
                Task::none()
            }
            (None, None) => {
                self.selected = None;
                self.select_host(0)
            }
        }
    }

    /// The light on the host chip: how the connection to the shown host is.
    fn connection_color(&self) -> iced::Color {
        match (&self.live, &self.error, self.snapshot.is_some()) {
            (_, Some(_), false) => palette::FAILED,
            (live::State::Live, _, _) => palette::CATCH_UP,
            (live::State::Lost(_), _, _) => palette::NOTE,
            (live::State::Connecting, _, _) => palette::MUTED,
        }
    }

    fn host_menu(&self) -> Element<'_, Message> {
        let items = self.hosts.iter().enumerate().map(|(index, entry)| {
            let current = self.selected == Some(index);
            let detail = match (&entry.origin, &entry.kind) {
                (Origin::ThisComputer, _) => fl!("host-this-computer"),
                (Origin::SshConfig, _) => fl!("host-ssh-config"),
                (_, hosts::HostKind::Ssh(target)) => target.to_string(),
                (_, hosts::HostKind::Local) => String::new(),
            };
            let light = if current {
                self.connection_color()
            } else {
                palette::FAINT
            };
            let mut line = row![
                controls::dot(light),
                column![
                    text(entry.name.clone()).size(13),
                    text(detail).size(11).color(palette::MUTED),
                ]
                .spacing(1),
                space::horizontal(),
            ]
            .spacing(10)
            .align_y(iced::Alignment::Center);
            // Saved hosts are edited; an alias is saved under a name of its own.
            if matches!(entry.origin, Origin::Saved | Origin::SshConfig) {
                line = line.push(controls::link(
                    fl!("menu-edit"),
                    Message::OpenForm(Some(index)),
                ));
            }
            controls::menu_item(line, current, Message::SelectHost(index))
        });

        let mut menu = Column::from_iter(items)
            .spacing(2)
            .push(controls::menu_item(
                text(fl!("menu-add")).size(13),
                false,
                Message::OpenForm(None),
            ));
        if let Some(path) = hosts::hosts_file() {
            menu = menu.push(
                container(
                    text(fl!("host-add-hint", path = path.display().to_string()))
                        .size(11)
                        .color(palette::FAINT),
                )
                .padding([6, 10]),
            );
        }
        for problem in &self.host_problems {
            menu = menu.push(
                container(text(problem.clone()).size(11).color(palette::FAILED)).padding([2, 10]),
            );
        }

        container(scrollable(menu))
            .width(340)
            .max_height(420)
            .padding(6)
            .style(|_| container::Style {
                background: Some(palette::SURFACE.into()),
                border: iced::Border {
                    color: palette::GRID_MAJOR,
                    width: 1.0,
                    radius: 10.0.into(),
                },
                shadow: iced::Shadow {
                    color: iced::Color::BLACK.scale_alpha(0.5),
                    offset: iced::Vector::new(0.0, 6.0),
                    blur_radius: 18.0,
                },
                ..container::Style::default()
            })
            .into()
    }

    fn music_box(&self) -> Element<'_, Message> {
        let selected = self.drawer.as_ref().and_then(|drawer| {
            let index = self
                .tracks
                .iter()
                .position(|track| track.timer.id == drawer.timer)?;
            let run = (drawer.tab == Tab::Logs)
                .then(|| drawer.selected_run(&self.tracks[index].timer))
                .flatten()
                .map(|run| run.start);
            Some((index, run))
        });
        let downtime = self
            .snapshot
            .as_ref()
            .map(|snapshot| snapshot.downtime.as_slice())
            .unwrap_or_default();

        // Fades out quickly, then lingers: strength (1 - t)².
        let flashes: Vec<(usize, iced::Color)> = self
            .tracks
            .iter()
            .enumerate()
            .filter_map(|(index, track)| {
                let (started, color) = self.flashes.get(&track.timer.id)?;
                let t = started.elapsed().as_secs_f32() / FLASH.as_secs_f32();
                (t < 1.0).then(|| (index, color.scale_alpha(0.28 * (1.0 - t).powi(2))))
            })
            .collect();
        let legend = self.composer.is_none();
        let rows = musicbox::height(self.tracks.len(), legend);
        responsive(move |size| {
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
                    selected: selected.map(|(index, _)| index),
                    selected_run: selected.and_then(|(_, run)| run),
                    focus: self.drawer.as_ref().and_then(|drawer| drawer.focus),
                    flashes: flashes.clone(),
                    legend,
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
        })
        .into()
    }

    fn header(&self) -> Element<'_, Message> {
        let scopes = controls::segmented(
            [
                (Scope::System, fl!("scope-system")),
                (Scope::User, fl!("scope-user")),
            ],
            self.scope,
            Message::SetScope,
        );
        let zooms = controls::segmented(
            Zoom::ALL.map(|zoom| (zoom, zoom.label())),
            self.zoom,
            Message::SetZoom,
        );
        let refresh = controls::action(
            if self.loading {
                fl!("refreshing")
            } else {
                fl!("refresh")
            },
            (!self.loading).then_some(Message::Refresh),
        );

        let host = controls::chip(
            row![
                controls::dot(self.connection_color()),
                text(
                    self.entry()
                        .map_or_else(|| fl!("no-host"), |entry| entry.name.clone())
                )
                .size(13),
                text("▾").size(11).color(palette::MUTED),
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center),
            Message::ToggleHostMenu,
        );
        let title = row![text("sysorbit").size(22), host]
            .spacing(16)
            .align_y(iced::Alignment::Center);

        let new_timer = controls::action(
            fl!("new-timer"),
            (self.host.is_some() && self.composer.is_none()).then_some(Message::OpenComposer),
        );

        // Only the controls wrap, so a fill spacer cannot push them down
        // while there is still room beside the title.
        let controls = row![scopes, zooms, refresh, new_timer]
            .spacing(10)
            .align_y(iced::Alignment::Center)
            .wrap()
            .vertical_spacing(10);

        row![title, space::horizontal(), controls]
            .spacing(16)
            .padding([0.0, musicbox::PAD])
            .align_y(iced::Alignment::Center)
            .into()
    }

    fn status(&self) -> Element<'_, Message> {
        let mut parts = Vec::new();
        if let Some(snapshot) = &self.snapshot {
            parts.push(fl!(
                "status-timers",
                count = snapshot.timers.len(),
                time = label::clock(snapshot.fetched_at, &self.tz)
            ));
            parts.extend(snapshot.warnings.iter().map(label::warning));
        }
        if let (Some(error), false) = (&self.error, self.tracks.is_empty()) {
            parts.push(error.clone());
        }

        let (indicator, color) = match &self.live {
            live::State::Live => (fl!("status-live"), palette::CATCH_UP),
            live::State::Connecting => (fl!("status-connecting"), palette::MUTED),
            live::State::Lost(reason) => (
                fl!(
                    "status-polling",
                    seconds = REFRESH.as_secs(),
                    reason = reason.as_str()
                ),
                palette::NOTE,
            ),
        };

        let keys = if self.drawer.is_some() {
            fl!("status-keys")
        } else {
            String::new()
        };

        // A clock off by more than a few seconds is worth noticing.
        let clock = self
            .snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.clock.as_ref())
            .map(|clock| {
                let color = if clock.offset.abs() >= SignedDuration::from_secs(5) {
                    palette::NOTE
                } else {
                    palette::MUTED
                };
                text(label::host_clock(clock)).size(12).color(color)
            });

        let mut status = row![
            text(indicator).size(12).color(color),
            text(parts.join("  ·  ")).size(12).color(palette::MUTED),
        ];
        if let Some(clock) = clock {
            status = status.push(clock);
        }
        status
            .push(space::horizontal())
            .push(text(keys).size(12).color(palette::FAINT))
            .spacing(12)
            .padding([0.0, musicbox::PAD])
            .into()
    }
}

fn is_run_at(mark: &Mark, at: Timestamp) -> bool {
    matches!(mark, Mark::Run { start, .. } if *start == at)
}

/// Keys for [`Nav`]; anything with Ctrl, Alt or Super is left alone.
fn navigation(event: keyboard::Event) -> Option<Message> {
    let keyboard::Event::KeyPressed {
        key: keyboard::Key::Named(key),
        modifiers,
        ..
    } = event
    else {
        return None;
    };
    if modifiers.control() || modifiers.alt() || modifiers.logo() {
        return None;
    }
    let nav = match key {
        Named::ArrowLeft => Nav::Previous,
        Named::ArrowRight => Nav::Next,
        Named::Home => Nav::First,
        Named::End => Nav::Last,
        Named::ArrowUp => Nav::Up,
        Named::ArrowDown => Nav::Down,
        Named::Enter => Nav::Open,
        Named::Escape => Nav::Back,
        _ => return None,
    };
    Some(Message::Navigate(nav))
}

/// Timers whose service started a run, or whose latest run failed, since
/// `old`: orange for a start, red for a failure.
fn flashes(old: &Snapshot, new: &Snapshot) -> Vec<(String, iced::Color)> {
    new.timers
        .iter()
        .filter_map(|timer| {
            let before = old.timers.iter().find(|before| before.id == timer.id)?;
            let latest = timer.runs.last()?;
            let previous = before.runs.last();
            let new_run = previous.is_none_or(|run| latest.start > run.start);
            let started = new_run || (timer.service_active && !before.service_active);
            let failed = matches!(latest.outcome, Outcome::Failed(_))
                && previous.is_none_or(|run| {
                    run.start != latest.start || !matches!(run.outcome, Outcome::Failed(_))
                });
            let color = match (failed, started) {
                (true, _) => palette::FAILED,
                (false, true) => palette::NOTE,
                (false, false) => return None,
            };
            Some((timer.id.clone(), color))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Run, Timer};

    fn timer(runs: Vec<Run>) -> Timer {
        Timer {
            id: "backup.timer".into(),
            description: String::new(),
            service: "backup.service".into(),
            service_active: false,
            active: true,
            unit_file_state: "enabled".into(),
            calendar: Vec::new(),
            monotonic: Vec::new(),
            next_elapse: None,
            last_trigger: None,
            persistent: false,
            wake_system: false,
            accuracy: SignedDuration::from_mins(1),
            randomized_delay: SignedDuration::ZERO,
            runs,
            stopped: Vec::new(),
        }
    }

    fn run(second: i64, outcome: Outcome) -> Run {
        Run {
            start: Timestamp::from_second(1_800_000_000 + second).unwrap(),
            end: None,
            outcome,
            invocation: None,
        }
    }

    fn snapshot(timers: Vec<Timer>) -> Snapshot {
        Snapshot {
            scope: Scope::System,
            fetched_at: Timestamp::from_second(1_800_000_000).unwrap(),
            timers,
            downtime: Vec::new(),
            history_known: true,
            clock: None,
            warnings: Vec::new(),
            capabilities: crate::systemd::Capabilities::default(),
        }
    }

    #[test]
    fn tracks_light_up_for_new_runs_and_failures() {
        let old = snapshot(vec![timer(vec![run(0, Outcome::Success)])]);

        let same = snapshot(vec![timer(vec![run(0, Outcome::Success)])]);
        assert!(flashes(&old, &same).is_empty());

        let started = snapshot(vec![timer(vec![
            run(0, Outcome::Success),
            run(60, Outcome::Running),
        ])]);
        assert_eq!(flashes(&old, &started)[0].1, palette::NOTE);

        let failed = snapshot(vec![timer(vec![run(
            0,
            Outcome::Failed("exit-code".into()),
        )])]);
        assert_eq!(flashes(&old, &failed)[0].1, palette::FAILED);
        // Still the same failure on the next refresh: no new flash.
        assert!(flashes(&failed, &failed).is_empty());
    }

    /// Drives the app the way iced's runtime does: each event goes to a
    /// user interface built from the current view with the previous one's
    /// widget state, and its messages go to `update`. Tasks are dropped.
    struct Harness {
        app: App,
        cache: iced_test::runtime::user_interface::Cache,
        renderer: iced::Renderer,
        cursor: iced::mouse::Cursor,
    }

    const SIZE: iced::Size = iced::Size::new(1280.0, 760.0);

    impl Harness {
        fn new(app: App) -> Self {
            use iced_test::core::renderer::Headless;
            let renderer =
                iced_test::futures::futures::executor::block_on(<iced::Renderer as Headless>::new(
                    iced::Font::DEFAULT,
                    iced::Pixels(16.0),
                    Some("tiny-skia"),
                ))
                .expect("headless renderer");
            Self {
                app,
                cache: Default::default(),
                renderer,
                cursor: iced::mouse::Cursor::Unavailable,
            }
        }

        fn send(&mut self, events: impl IntoIterator<Item = iced::Event>) {
            for event in events {
                let mut messages = Vec::new();
                let mut ui = iced_test::runtime::UserInterface::build(
                    self.app.view(),
                    SIZE,
                    std::mem::take(&mut self.cache),
                    &mut self.renderer,
                );
                let _ = ui.update(
                    &[event],
                    self.cursor,
                    &mut self.renderer,
                    &mut iced_test::core::clipboard::Null,
                    &mut messages,
                );
                self.cache = ui.into_cache();
                for message in messages {
                    let _ = self.app.update(message);
                }
            }
        }

        fn find<S>(&mut self, selector: S) -> Option<S::Output>
        where
            S: iced_test::Selector + Send,
            S::Output: Clone + Send,
        {
            use iced_test::core::widget::Operation;
            let mut ui = iced_test::runtime::UserInterface::build(
                self.app.view(),
                SIZE,
                std::mem::take(&mut self.cache),
                &mut self.renderer,
            );
            let mut operation = selector.find();
            ui.operate(
                &self.renderer,
                &mut iced_test::core::widget::operation::black_box(&mut operation),
            );
            self.cache = ui.into_cache();
            match operation.finish() {
                iced_test::core::widget::operation::Outcome::Some(found) => found,
                _ => None,
            }
        }

        fn click(&mut self, id: &'static str) {
            let target = self
                .find(iced_test::selector::id(id))
                .unwrap_or_else(|| panic!("{id} not found"));
            // A click only lands where the widget is on screen.
            let visible = target
                .visible_bounds()
                .filter(|visible| visible.height + 1.0 >= target.bounds().height)
                .unwrap_or_else(|| panic!("{id} is scrolled out of view"));
            self.cursor = iced::mouse::Cursor::Available(visible.center());
            self.send(iced_test::simulator::click());
        }

        fn type_text(&mut self, text: &str) {
            for event in iced_test::simulator::typewrite(text).collect::<Vec<_>>() {
                self.send([event]);
                // The runtime redraws after every change.
                self.send([iced::Event::Window(iced::window::Event::RedrawRequested(
                    iced::time::Instant::now(),
                ))]);
            }
        }

        fn focused(&mut self) -> Option<String> {
            self.find(iced_test::selector::is_focused())
                .map(|target| format!("{target:?}"))
        }
    }

    fn composing() -> App {
        let (mut app, _) = App::new(Vec::new(), None, Vec::new());
        app.composer = Some(Composer::new(true));
        app.fitted = Some(0);
        app.rebuild();
        app
    }

    fn typing_keeps_focus(app: App) {
        let mut harness = Harness::new(app);
        for (field, text) in [
            (
                composer::Field::Name as fn(String) -> composer::Field,
                "backup",
            ),
            (composer::Field::Description, "Back up my documents"),
            (composer::Field::Command, "/usr/bin/true"),
        ] {
            let id = composer::field_id(field);
            harness.click(id);
            harness.type_text(text);
            assert!(
                harness
                    .focused()
                    .is_some_and(|focused| focused.contains(id)),
                "{id} lost the focus: {:?}",
                harness.focused()
            );
            assert!(
                harness.find(text).is_some(),
                "{id} holds only part of {text:?}"
            );
        }
    }

    #[test]
    fn composer_fields_keep_the_focus_while_typing() {
        let _language = crate::i18n::test_language("en");
        let mut failed = Vec::new();
        for (timers, ready, files) in [
            (false, false, false),
            (true, false, false),
            (false, true, false),
            (true, true, false),
            (true, true, true),
        ] {
            let mut app = composing();
            let now = app.now;
            if timers {
                app.snapshot = Some(Arc::new(snapshot(vec![timer(Vec::new())])));
            }
            let composer = app.composer.as_mut().unwrap();
            if ready {
                composer.update_rhythm_for_test();
                composer.schedule = Schedule::Ready(vec![systemd::calendar::Preview {
                    normalized: "*-*-* 03:00:00".to_owned(),
                    occurrences: vec![now + SignedDuration::from_hours(5)],
                }]);
            }
            if files {
                composer.toggle_files();
            }
            app.rebuild();
            if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| typing_keeps_focus(app)))
                .is_err()
            {
                failed.push((timers, ready, files));
            }
        }
        assert!(
            failed.is_empty(),
            "focus lost with (timers, ready, files) = {failed:?}"
        );
    }

    /// The way a person gets there: this machine's timers, the composer
    /// opened from the header, keys typed while the clock ticks.
    #[cfg(target_os = "linux")]
    #[test]
    fn composer_opened_from_the_header_keeps_the_focus() {
        let _language = crate::i18n::test_language("en");
        let local = HostEntry {
            id: "local".to_owned(),
            name: "here".to_owned(),
            kind: hosts::HostKind::Local,
            origin: Origin::ThisComputer,
            saved: None,
        };
        let (mut app, _) = App::new(vec![local], Some(0), Vec::new());
        let _ = app.update(Message::Loaded(
            app.generation,
            Ok(Arc::new(snapshot(vec![timer(Vec::new())]))),
        ));
        let _ = app.update(Message::OpenComposer);
        // A painted rhythm whose preview arrives while the name is typed.
        let _ = app.update(Message::Composer(ComposerMessage::Rhythm(
            rhythm::Edit::Preset(rhythm::Preset::Nightly),
        )));
        let revision = app.composer.as_ref().unwrap().revision_for_test();
        let now = app.now;
        let mut harness = Harness::new(app);
        let id = composer::field_id(composer::Field::Name);
        harness.click(id);
        for (index, key) in "backup".chars().enumerate() {
            harness.type_text(&key.to_string());
            if index == 0 {
                let _ = harness
                    .app
                    .update(Message::Composer(ComposerMessage::Previewed(
                        revision,
                        Some(Ok(vec![systemd::calendar::Preview {
                            normalized: "*-*-* 03:00:00".to_owned(),
                            occurrences: vec![now + SignedDuration::from_hours(5)],
                        }])),
                    )));
            }
            let _ = harness.app.update(Message::Tick);
            assert!(
                harness
                    .focused()
                    .is_some_and(|focused| focused.contains(id)),
                "lost after {key:?}"
            );
        }
        assert!(harness.find("backup").is_some());
    }

    /// Renders the composer in a few states to PNG files for a look,
    /// without a display. Run with
    /// `SYSORBIT_SNAPSHOTS=/some/dir cargo test composer_snapshots -- --ignored`.
    #[test]
    #[ignore = "writes images for people to look at"]
    fn composer_snapshots() {
        let Some(directory) = std::env::var_os("SYSORBIT_SNAPSHOTS") else {
            return;
        };
        let directory = std::path::PathBuf::from(directory);
        let _language = crate::i18n::test_language("en");
        let render = |app: &App, name: &str| {
            let path = directory.join(name);
            // The file name gets the renderer's name; an old one would be
            // compared instead of replaced.
            for entry in std::fs::read_dir(&directory)
                .into_iter()
                .flatten()
                .flatten()
            {
                if entry.file_name().to_string_lossy().starts_with(name) {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
            let mut simulator =
                iced_test::Simulator::with_size(iced::Settings::default(), SIZE, app.view());
            let snapshot = simulator.snapshot(&app.theme()).expect("snapshot");
            assert!(snapshot.matches_image(&path).expect("write"));
        };

        render(&composing(), "0-no-timers");
        let mut app = composing();
        app.snapshot = Some(Arc::new(snapshot(vec![timer(Vec::new())])));
        app.rebuild();
        render(&app, "1-empty");

        let now = app.now;
        let composer = app.composer.as_mut().unwrap();
        composer.update(composer::Field::Name("backup".to_owned()));
        composer.update(composer::Field::Command(
            "/usr/bin/rsync -a /home/me/docs /mnt/backup".to_owned(),
        ));
        composer.edit(rhythm::Edit::Preset(rhythm::Preset::WorkdayMornings));
        composer.edit(rhythm::Edit::Paint {
            day: 5,
            hour: 10,
            on: true,
        });
        composer.schedule = Schedule::Ready(vec![
            systemd::calendar::Preview {
                normalized: "Mon..Fri *-*-* 09:00:00".to_owned(),
                occurrences: (1..4)
                    .map(|day| now + SignedDuration::from_hours(24 * day))
                    .collect(),
            },
            systemd::calendar::Preview {
                normalized: "Sat *-*-* 10:00:00".to_owned(),
                occurrences: vec![now + SignedDuration::from_hours(30)],
            },
        ]);
        composer.set_hint(Some(rhythm::Hint::Cell(2, 9)));
        app.rebuild();
        render(&app, "2-filled");
    }

    #[test]
    fn filling_in_the_composer_does_not_move_its_fields() {
        let _language = crate::i18n::test_language("en");
        let mut harness = Harness::new(composing());
        let name = composer::field_id(composer::Field::Name);
        let place = |harness: &mut Harness| {
            harness
                .find(iced_test::selector::id(name))
                .expect("name field")
                .bounds()
                .y
        };
        let empty = place(&mut harness);

        let now = harness.app.now;
        let composer = harness.app.composer.as_mut().unwrap();
        composer.update(composer::Field::Name("db-dump".to_owned()));
        composer.update(composer::Field::Command(
            "/usr/local/bin/dump-databases --all --compress".to_owned(),
        ));
        composer.schedule = Schedule::Checking;
        harness.app.rebuild();
        assert_eq!(place(&mut harness), empty, "while systemd is asked");

        let composer = harness.app.composer.as_mut().unwrap();
        composer.schedule = Schedule::Ready(vec![
            systemd::calendar::Preview {
                normalized: "Mon..Fri *-*-* 09,18:00:00".to_owned(),
                occurrences: vec![now + SignedDuration::from_hours(3)],
            },
            systemd::calendar::Preview {
                normalized: "Sat *-*-* 10:00:00".to_owned(),
                occurrences: vec![now + SignedDuration::from_hours(30)],
            },
        ]);
        harness.app.rebuild();
        assert_eq!(place(&mut harness), empty, "once systemd answered");

        let composer = harness.app.composer.as_mut().unwrap();
        composer.schedule = Schedule::Invalid("Failed to parse calendar specification".to_owned());
        harness.app.rebuild();
        assert_eq!(place(&mut harness), empty, "with systemd's complaint");
    }

    #[test]
    fn the_composer_split_fits_the_tracks_and_can_be_dragged() {
        let _language = crate::i18n::test_language("en");
        let mut harness = Harness::new(composing());
        let ratio = |app: &App| match app.panes.layout() {
            pane_grid::Node::Split { ratio, .. } => *ratio,
            pane_grid::Node::Pane(_) => panic!("no split"),
        };
        // One draft track needs little room.
        assert!(ratio(&harness.app) < 0.35, "{}", ratio(&harness.app));

        let name = |harness: &mut Harness| {
            harness
                .find(iced_test::selector::id(composer::field_id(
                    composer::Field::Name,
                )))
                .expect("name field")
                .bounds()
                .y
        };
        let before = name(&mut harness);
        let pane_grid::Node::Split { id, .. } = *harness.app.panes.layout() else {
            panic!("no split");
        };
        let _ = harness
            .app
            .update(Message::ResizeDrawer(pane_grid::ResizeEvent {
                split: id,
                ratio: 0.6,
            }));
        assert!((ratio(&harness.app) - 0.6).abs() < 0.001);
        assert!(name(&mut harness) > before + 100.0, "the form moved down");

        // Once dragged, new timers leave the split alone.
        harness.app.snapshot = Some(Arc::new(snapshot(vec![timer(Vec::new())])));
        harness.app.rebuild();
        assert!((ratio(&harness.app) - 0.6).abs() < 0.001);
    }

    #[test]
    fn the_split_follows_the_tracks_until_dragged() {
        let _language = crate::i18n::test_language("en");
        let mut app = composing();
        let ratio = |app: &App| match app.panes.layout() {
            pane_grid::Node::Split { ratio, .. } => *ratio,
            pane_grid::Node::Pane(_) => panic!("no split"),
        };
        let alone = ratio(&app);
        app.snapshot = Some(Arc::new(snapshot(vec![timer(Vec::new())])));
        app.rebuild();
        assert!(ratio(&app) > alone, "{} > {alone}", ratio(&app));
    }
}
