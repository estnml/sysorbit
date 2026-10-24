//! Buttons shared by the header and the drawer. Related choices sit together
//! in one framed group; the current one is lifted, the others stay quiet.

use iced::widget::{Row, button, container, text};
use iced::{Border, Element};

use super::{Message, palette};

const RADIUS: f32 = 8.0;

/// A framed group of choices where the current one is highlighted.
pub fn segmented<'a, T, const N: usize>(
    options: [(T, &'a str); N],
    current: T,
    on_press: fn(T) -> Message,
) -> Element<'a, Message>
where
    T: Copy + PartialEq + 'a,
{
    let buttons = options.map(|(value, name)| {
        let selected = value == current;
        button(text(name).size(12))
            .padding([4, 12])
            .style(move |_, status| segment(selected, status))
            .on_press(on_press(value))
            .into()
    });
    frame(Row::from_iter(buttons).spacing(2))
}

/// A single action in the same frame as the groups, e.g. Refresh.
pub fn action<'a>(label: &'a str, on_press: Option<Message>) -> Element<'a, Message> {
    frame(
        button(text(label).size(12))
            .padding([4, 12])
            .style(|_, status| segment(false, status))
            .on_press_maybe(on_press),
    )
}

/// A bare icon button, for closing.
pub fn icon<'a>(glyph: &'a str, on_press: Message) -> Element<'a, Message> {
    button(text(glyph).size(13))
        .padding([4, 9])
        .style(|_, status| segment(false, status))
        .on_press(on_press)
        .into()
}

fn frame<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    container(content)
        .padding(2)
        .style(|_| container::Style {
            background: Some(palette::BACKGROUND.into()),
            border: Border {
                color: palette::GRID_MAJOR,
                width: 1.0,
                radius: (RADIUS + 2.0).into(),
            },
            ..container::Style::default()
        })
        .into()
}

fn segment(selected: bool, status: button::Status) -> button::Style {
    let (background, text_color) = match (selected, status) {
        (true, _) => (Some(palette::GRID_MAJOR), palette::TEXT),
        (false, button::Status::Hovered | button::Status::Pressed) => {
            (Some(palette::GRID), palette::TEXT)
        }
        (false, button::Status::Disabled) => (None, palette::FAINT),
        (false, button::Status::Active) => (None, palette::MUTED),
    };
    button::Style {
        background: background.map(Into::into),
        text_color,
        border: Border {
            radius: RADIUS.into(),
            ..Border::default()
        },
        ..button::Style::default()
    }
}
