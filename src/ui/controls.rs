//! Buttons shared by the header and the drawer. Related choices sit together
//! in one framed group; the current one is lifted, the others stay quiet.

use iced::widget::{Row, button, container, text, tooltip};
use iced::{Border, Color, Element, Length};

use super::{Message, palette};

const RADIUS: f32 = 8.0;

/// A framed group of choices where the current one is highlighted.
pub fn segmented<'a, T, const N: usize>(
    options: [(T, String); N],
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
pub fn action<'a>(label: String, on_press: Option<Message>) -> Element<'a, Message> {
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

/// The button that confirms, like Save: lifted like a current choice.
pub fn primary<'a>(label: String, on_press: Message) -> Element<'a, Message> {
    frame(
        button(text(label).size(12))
            .padding([4, 12])
            .style(|_, status| segment(true, status))
            .on_press(on_press),
    )
}

/// A quiet text button, like Edit beside a menu row.
pub fn link<'a>(label: String, on_press: Message) -> Element<'a, Message> {
    button(text(label).size(11))
        .padding([2, 6])
        .style(|_, status| segment(false, status))
        .on_press(on_press)
        .into()
}

/// A framed button that opens something, like the host menu.
pub fn chip<'a>(
    content: impl Into<Element<'a, Message>>,
    on_press: Message,
) -> Element<'a, Message> {
    frame(
        button(content)
            .padding([4, 10])
            .style(|_, status| segment(false, status))
            .on_press(on_press),
    )
}

/// A full width row in a menu; the current one is lifted.
pub fn menu_item<'a>(
    content: impl Into<Element<'a, Message>>,
    current: bool,
    on_press: Message,
) -> Element<'a, Message> {
    button(content)
        .width(Length::Fill)
        .padding([6, 10])
        .style(move |_, status| segment(current, status))
        .on_press(on_press)
        .into()
}

/// A small round status light.
pub fn dot<'a>(color: Color) -> Element<'a, Message> {
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

/// A small ⓘ that explains a term on hover.
pub fn info<'a>(help: String) -> Element<'a, Message> {
    tooltip(
        text("ⓘ").size(12).color(palette::MUTED),
        container(text(help).size(12))
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
