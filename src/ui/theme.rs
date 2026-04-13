use gpui::{div, px, Div, ParentElement as _, Rgba, Styled as _};
use serde::Deserialize;
use std::sync::LazyLock;

// ── Color type with hex deserialization ─────────────────────────

/// A color parsed from a hex string like "#1a1b26" or "#ffffff0a".
#[derive(Clone, Copy)]
pub struct Color(pub Rgba);

impl<'de> Deserialize<'de> for Color {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s: String = Deserialize::deserialize(deserializer)?;
        parse_hex_to_rgba(&s)
            .map(Color)
            .ok_or_else(|| serde::de::Error::custom(format!("invalid hex color: {s}")))
    }
}

fn parse_hex_to_rgba(hex: &str) -> Option<Rgba> {
    let hex = hex.trim_start_matches('#');
    let (r, g, b, a) = match hex.len() {
        6 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            (r, g, b, 255u8)
        }
        8 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
            (r, g, b, a)
        }
        _ => return None,
    };
    Some(Rgba {
        r: r as f32 / 255.0,
        g: g as f32 / 255.0,
        b: b as f32 / 255.0,
        a: a as f32 / 255.0,
    })
}

// ── Theme struct — every field maps 1:1 to JSON ────────────────

#[derive(Deserialize)]
pub struct ThemeColors {
    pub bg_base: Color,
    pub bg_terminal: Color,
    pub terminal_foreground: Color,
    pub terminal_cursor: Color,
    pub bg_surface: Color,
    pub bg_elevated: Color,
    pub bg_hover: Color,
    pub bg_active: Color,
    pub bg_selected: Color,
    pub bg_input: Color,

    pub border: Color,
    pub border_subtle: Color,
    pub border_strong: Color,
    pub border_focus: Color,
    pub transparent: Color,
    pub accent: Color,
    pub selection_bg: Color,

    pub text_primary: Color,
    pub text_secondary: Color,
    pub text_tertiary: Color,
    pub text_muted: Color,
    pub text_dim: Color,
    pub text_ghost: Color,
    pub text_faint: Color,
    pub text_invisible: Color,

    pub status_green_dim: Color,
    pub status_dim: Color,

    pub agent_running: Color,
    pub agent_needs_input: Color,

    pub error_text: Color,
    pub error_bg: Color,
    pub error_border: Color,


    pub diff_add_bg: Color,
    pub diff_del_bg: Color,
    pub diff_add_text: Color,
    pub diff_del_text: Color,
    pub diff_hunk_header: Color,
}

static THEME: LazyLock<ThemeColors> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../assets/themes/superhq-dark.json"))
        .expect("Failed to parse default theme (superhq-dark.json)")
});

// ── Public accessors (same API as before) ──────────────────────

pub fn bg_base() -> Rgba { THEME.bg_base.0 }
pub fn bg_terminal() -> Rgba { THEME.bg_terminal.0 }
pub fn terminal_foreground() -> Rgba { THEME.terminal_foreground.0 }
pub fn terminal_cursor() -> Rgba { THEME.terminal_cursor.0 }
pub fn bg_surface() -> Rgba { THEME.bg_surface.0 }
pub fn bg_elevated() -> Rgba { THEME.bg_elevated.0 }
pub fn bg_hover() -> Rgba { THEME.bg_hover.0 }
pub fn bg_active() -> Rgba { THEME.bg_active.0 }
pub fn bg_selected() -> Rgba { THEME.bg_selected.0 }
pub fn bg_input() -> Rgba { THEME.bg_input.0 }

pub fn border() -> Rgba { THEME.border.0 }
pub fn border_subtle() -> Rgba { THEME.border_subtle.0 }
pub fn border_strong() -> Rgba { THEME.border_strong.0 }
pub fn border_focus() -> Rgba { THEME.border_focus.0 }
pub fn transparent() -> Rgba { THEME.transparent.0 }
pub fn accent() -> Rgba { THEME.accent.0 }
pub fn selection_bg() -> Rgba { THEME.selection_bg.0 }

pub fn text_primary() -> Rgba { THEME.text_primary.0 }
pub fn text_secondary() -> Rgba { THEME.text_secondary.0 }
pub fn text_tertiary() -> Rgba { THEME.text_tertiary.0 }
pub fn text_muted() -> Rgba { THEME.text_muted.0 }
pub fn text_dim() -> Rgba { THEME.text_dim.0 }
pub fn text_ghost() -> Rgba { THEME.text_ghost.0 }
pub fn text_faint() -> Rgba { THEME.text_faint.0 }
pub fn text_invisible() -> Rgba { THEME.text_invisible.0 }

pub fn status_green_dim() -> Rgba { THEME.status_green_dim.0 }
pub fn status_dim() -> Rgba { THEME.status_dim.0 }

pub fn agent_running() -> Rgba { THEME.agent_running.0 }
pub fn agent_needs_input() -> Rgba { THEME.agent_needs_input.0 }

pub fn error_text() -> Rgba { THEME.error_text.0 }
pub fn error_bg() -> Rgba { THEME.error_bg.0 }
pub fn error_border() -> Rgba { THEME.error_border.0 }


pub fn diff_add_bg() -> Rgba { THEME.diff_add_bg.0 }
pub fn diff_del_bg() -> Rgba { THEME.diff_del_bg.0 }
pub fn diff_add_text() -> Rgba { THEME.diff_add_text.0 }
pub fn diff_del_text() -> Rgba { THEME.diff_del_text.0 }
pub fn diff_hunk_header() -> Rgba { THEME.diff_hunk_header.0 }

// ── Shared styles ──────────────────────────────────────────────

pub fn popover() -> Div {
    div()
        .bg(bg_surface())
        .border_1()
        .border_color(border())
        .rounded(px(8.0))
        .shadow_lg()
        .py_1()
        .px_1()
        .flex()
        .flex_col()
}

pub fn menu_item() -> Div {
    div()
        .px_2p5()
        .py(px(5.0))
        .rounded(px(4.0))
        .text_xs()
        .cursor_pointer()
        .text_color(text_secondary())
        .flex()
        .items_center()
        .gap(px(6.0))
}

pub fn button(label: &str) -> Div {
    div()
        .px_3()
        .py(px(5.0))
        .rounded(px(6.0))
        .text_xs()
        .font_weight(gpui::FontWeight::MEDIUM)
        .cursor_pointer()
        .text_color(text_dim())
        .flex()
        .items_center()
        .gap(px(6.0))
        .child(label.to_string())
}

pub fn button_primary(label: &str) -> Div {
    div()
        .px_3()
        .py(px(5.0))
        .rounded(px(6.0))
        .text_xs()
        .font_weight(gpui::FontWeight::MEDIUM)
        .cursor_pointer()
        .bg(bg_selected())
        .text_color(text_secondary())
        .flex()
        .items_center()
        .gap(px(6.0))
        .child(label.to_string())
}

pub fn button_danger(label: &str) -> Div {
    div()
        .px_3()
        .py(px(5.0))
        .rounded(px(6.0))
        .text_xs()
        .font_weight(gpui::FontWeight::MEDIUM)
        .cursor_pointer()
        .text_color(error_text())
        .flex()
        .items_center()
        .gap(px(6.0))
        .child(label.to_string())
}

pub fn menu_separator() -> Div {
    div()
        .mx_2()
        .my_1()
        .h(px(1.0))
        .bg(border())
}

// ── Utilities ──────────────────────────────────────────────────

pub fn parse_hex_color(hex: &str) -> Option<Rgba> {
    parse_hex_to_rgba(hex)
}

/// Returns (r, g, b) as u8 values for a theme color. Used by terminal config.
pub fn rgb_bytes(color: Rgba) -> (u8, u8, u8) {
    (
        (color.r * 255.0) as u8,
        (color.g * 255.0) as u8,
        (color.b * 255.0) as u8,
    )
}
