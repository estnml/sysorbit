//! The dialog that adds or edits a host saved in `hosts.toml`: where it
//! is, how to log in, and a test of the connection before saving.

use iced::widget::{Column, column, container, radio, row, space, text, text_input};
use iced::{Border, Element, Length, Shadow, Vector};

use super::{Message, controls, label, palette};
use crate::hosts::{HostEntry, HostKind, Origin, SavedHost};
use crate::i18n::fl;
use crate::systemd::Probe;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Auth {
    /// The agent's keys and whatever `~/.ssh/config` says.
    SshSetup,
    /// One private key file, such as a PEM file.
    KeyFile,
}

#[derive(Debug, Clone)]
pub enum Field {
    Name(String),
    Address(String),
    Port(String),
    User(String),
    Auth(Auth),
    KeyFile(String),
}

#[derive(Debug, Clone)]
pub enum Test {
    Idle,
    Running,
    Passed(Probe),
    Failed(String),
}

#[derive(Debug, Clone)]
pub struct HostForm {
    /// The id of the saved host being edited; `None` adds a new one.
    pub editing: Option<String>,
    name: String,
    address: String,
    port: String,
    user: String,
    auth: Auth,
    key_file: String,
    /// Options from `hosts.toml` the dialog does not show; kept as they are.
    options: Vec<String>,
    pub test: Test,
    /// Counts tests, so the result of one started before an edit is dropped.
    pub test_run: u64,
    pub error: Option<String>,
}

impl HostForm {
    pub fn new() -> Self {
        Self {
            editing: None,
            name: String::new(),
            address: String::new(),
            port: String::new(),
            user: String::new(),
            auth: Auth::SshSetup,
            key_file: String::new(),
            options: Vec::new(),
            test: Test::Idle,
            test_run: 0,
            error: None,
        }
    }

    /// A form for `entry`: a saved host to edit, or a `~/.ssh/config` alias
    /// to save under a name of its own.
    pub fn for_entry(entry: &HostEntry) -> Self {
        let mut form = Self::new();
        match (&entry.saved, &entry.kind) {
            (Some(saved), _) => {
                // `destination` may carry `user@` and `:port` itself.
                let target = crate::ssh::Target::parse(&saved.destination);
                let (user, address) = match target.destination.split_once('@') {
                    Some((user, address)) => (Some(user.to_owned()), address.to_owned()),
                    None => (None, target.destination.clone()),
                };
                form.editing = Some(saved.id());
                form.name = saved.name.clone().unwrap_or_default();
                form.address = address;
                form.port = saved
                    .port
                    .or(target.port)
                    .map(|port| port.to_string())
                    .unwrap_or_default();
                form.user = saved.user.clone().or(user).unwrap_or_default();
                if let Some(key) = &saved.key_file {
                    form.auth = Auth::KeyFile;
                    form.key_file = key.clone();
                }
                form.options = saved.options.clone();
            }
            (None, HostKind::Ssh(target)) if entry.origin == Origin::SshConfig => {
                form.name = entry.name.clone();
                form.address = target.destination.clone();
            }
            _ => {}
        }
        form
    }

    pub fn update(&mut self, field: Field) {
        match field {
            Field::Name(value) => self.name = value,
            Field::Address(value) => self.address = value,
            Field::Port(value) => self.port = value,
            Field::User(value) => self.user = value,
            Field::Auth(value) => self.auth = value,
            Field::KeyFile(value) => self.key_file = value,
        }
        // A result no longer describes what the form says.
        self.test = Test::Idle;
        self.test_run += 1;
        self.error = None;
    }

    /// The host the form describes, or what to fix first.
    pub fn host(&self) -> Result<SavedHost, String> {
        let trimmed = |value: &str| Some(value.trim().to_owned()).filter(|value| !value.is_empty());
        let destination = trimmed(&self.address).ok_or_else(|| fl!("form-error-address"))?;
        let port = match trimmed(&self.port) {
            None => None,
            Some(port) => Some(
                port.parse::<u16>()
                    .ok()
                    .filter(|&port| port > 0)
                    .ok_or_else(|| fl!("form-error-port"))?,
            ),
        };
        let key_file = match self.auth {
            Auth::SshSetup => None,
            Auth::KeyFile => Some(trimmed(&self.key_file).ok_or_else(|| fl!("form-error-key"))?),
        };
        Ok(SavedHost {
            id: self.editing.clone(),
            name: trimmed(&self.name),
            destination,
            user: trimmed(&self.user),
            port,
            key_file,
            options: self.options.clone(),
        })
    }

    pub fn view(&self) -> Element<'_, Message> {
        let input = |placeholder: String, value: &str, field: fn(String) -> Field| {
            text_input(&placeholder, value)
                .on_input(move |value| Message::Form(FormMessage::Edit(field(value))))
                .on_submit(Message::Form(FormMessage::Save))
                .size(13)
                .padding([6, 8])
        };
        let labelled =
            |title: String, widget: Element<'static, Message>| -> Element<'static, Message> {
                column![text(title).size(11).color(palette::MUTED), widget]
                    .spacing(4)
                    .into()
            };

        let title = if self.editing.is_some() {
            fl!("form-title-edit")
        } else {
            fl!("form-title-add")
        };

        let address = row![
            container(labelled(
                fl!("form-address"),
                input(
                    fl!("form-address-placeholder"),
                    &self.address,
                    Field::Address
                )
                .into()
            ))
            .width(Length::Fill),
            container(labelled(
                fl!("form-port"),
                input("22".to_owned(), &self.port, Field::Port).into()
            ))
            .width(80),
        ]
        .spacing(10);

        let auth = |label: String, value: Auth| {
            radio(label, value, Some(self.auth), |auth| {
                Message::Form(FormMessage::Edit(Field::Auth(auth)))
            })
            .size(14)
            .text_size(13)
            .spacing(8)
        };
        let mut login = Column::new()
            .spacing(8)
            .push(text(fl!("form-auth")).size(11).color(palette::MUTED))
            .push(auth(fl!("form-auth-ssh"), Auth::SshSetup))
            .push(auth(fl!("form-auth-key"), Auth::KeyFile));
        if self.auth == Auth::KeyFile {
            login = login.push(
                row![
                    input(fl!("form-key-placeholder"), &self.key_file, Field::KeyFile)
                        .width(Length::Fill),
                    controls::action(fl!("form-browse"), Some(Message::Form(FormMessage::Browse))),
                ]
                .spacing(8)
                .align_y(iced::Alignment::Center),
            );
        }

        let (result, color) = match (&self.error, &self.test) {
            (Some(error), _) => (error.clone(), palette::FAILED),
            (None, Test::Idle) => (String::new(), palette::MUTED),
            (None, Test::Running) => (fl!("form-testing"), palette::MUTED),
            (None, Test::Failed(error)) => (error.clone(), palette::FAILED),
            // Connected, but without the system's journal history is unknown.
            (None, Test::Passed(probe)) if !probe.journal_readable => {
                (passed(probe), palette::NOTE)
            }
            (None, Test::Passed(probe)) => (passed(probe), palette::CATCH_UP),
        };

        let mut buttons = row![].spacing(8).align_y(iced::Alignment::Center);
        if self.editing.is_some() {
            buttons = buttons.push(controls::action(
                fl!("form-delete"),
                Some(Message::Form(FormMessage::Delete)),
            ));
        }
        let testing = matches!(self.test, Test::Running);
        buttons = buttons
            .push(space::horizontal())
            .push(controls::action(
                fl!("form-test"),
                (!testing).then_some(Message::Form(FormMessage::Test)),
            ))
            .push(controls::action(
                fl!("form-cancel"),
                Some(Message::Form(FormMessage::Cancel)),
            ))
            .push(controls::primary(
                fl!("form-save"),
                Message::Form(FormMessage::Save),
            ));

        let body = column![
            text(title).size(16),
            labelled(
                fl!("form-name"),
                input(fl!("form-name-placeholder"), &self.name, Field::Name).into()
            ),
            address,
            labelled(
                fl!("form-user"),
                input(fl!("form-user-placeholder"), &self.user, Field::User).into()
            ),
            login,
            text(result).size(12).color(color),
            buttons,
        ]
        .spacing(14);

        container(body)
            .width(460)
            .padding(20)
            .style(|_| container::Style {
                background: Some(palette::SURFACE.into()),
                border: Border {
                    color: palette::GRID_MAJOR,
                    width: 1.0,
                    radius: 12.0.into(),
                },
                shadow: Shadow {
                    color: iced::Color::BLACK.scale_alpha(0.6),
                    offset: Vector::new(0.0, 10.0),
                    blur_radius: 30.0,
                },
                ..container::Style::default()
            })
            .into()
    }
}

/// `systemd 252 · journal readable · server clock in sync`
fn passed(probe: &Probe) -> String {
    let mut parts = vec![match probe.version {
        Some(version) => fl!("form-test-systemd", version = version),
        None => fl!("form-test-connected"),
    }];
    parts.push(if probe.journal_readable {
        fl!("form-test-journal")
    } else {
        fl!("form-test-journal-restricted")
    });
    if let Some(clock) = &probe.clock {
        parts.push(label::host_clock(clock));
    }
    format!("✓ {}", parts.join(" · "))
}

#[derive(Debug, Clone)]
pub enum FormMessage {
    Edit(Field),
    Browse,
    Picked(Option<String>),
    Test,
    Tested(u64, Result<Probe, String>),
    Save,
    Delete,
    Cancel,
}
