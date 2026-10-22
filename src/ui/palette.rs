//! Colors shared by the canvas and the widgets.

use iced::Color;
use iced::theme::{Palette, Theme};

pub const BACKGROUND: Color = rgb(0x14, 0x14, 0x14);
pub const SURFACE: Color = rgb(0x1c, 0x1c, 0x1c);
pub const GRID: Color = rgb(0x2c, 0x2c, 0x2c);
pub const GRID_MAJOR: Color = rgb(0x3a, 0x3a, 0x3a);
pub const TEXT: Color = rgb(0xec, 0xea, 0xe6);
pub const MUTED: Color = rgb(0x8c, 0x8a, 0x86);
pub const FAINT: Color = rgb(0x55, 0x54, 0x52);
pub const COMB: Color = rgb(0x9a, 0x98, 0x94);

/// A run or an elapse: the "note".
pub const NOTE: Color = rgb(0xd0, 0x8a, 0x2a);
pub const FAILED: Color = rgb(0xe0, 0x55, 0x55);
/// `Persistent=true` catching up.
pub const CATCH_UP: Color = rgb(0x4f, 0xb3, 0xa9);
pub const DOWNTIME: Color = rgb(0x5a, 0x58, 0x55);

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::from_rgb8(r, g, b)
}

pub fn theme() -> Theme {
    Theme::custom(
        "sysorbit",
        Palette {
            background: BACKGROUND,
            text: TEXT,
            primary: NOTE,
            success: CATCH_UP,
            warning: NOTE,
            danger: FAILED,
        },
    )
}
