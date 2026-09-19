use iced::theme::Mode;
use iced::theme::palette as iced_palette;
use iced::widget::container;
use iced::{Background, Border, Color, Theme};
use native_theme_iced::{ColorMode, ResolvedTheme};

use crate::config::ThemeMode;

#[derive(Debug, Clone, Copy)]
pub struct Colors {
    pub surface: Color,
    pub card_hover: Color,
    pub card_pressed: Color,
    pub card_selected: Color,
    pub border: Color,
    pub border_hover: Color,
    pub accent: Color,
    pub text: Color,
}

#[derive(Debug, Clone, Copy)]
pub struct Metrics {
    pub chip_size: f32,
    pub card_radius: f32,
    pub card_padding: f32,
    pub icon_size: f32,
    pub icon_gap: f32,
    pub name_size: f32,
    pub line_height: f32,
}

pub const SPACING: f32 = 6.0;
pub const VIEW_PADDING: f32 = 8.0;
pub const WINDOW_RADIUS: f32 = 12.0;

const SURFACE_ALPHA: f32 = 0.72;
pub const CARD_PADDING: f32 = 8.0;
pub const ICON_SIZE: f32 = 64.0;
const ICON_GAP: f32 = 4.0;

fn chip_size(icon_size: f32, name_size: f32, line_height: f32) -> f32 {
    (CARD_PADDING * 2.0 + icon_size + ICON_GAP + name_size * line_height).ceil()
}

impl Default for Metrics {
    fn default() -> Self {
        Self {
            chip_size: chip_size(ICON_SIZE, 12.5, 1.3),
            card_radius: 10.0,
            card_padding: CARD_PADDING,
            icon_size: ICON_SIZE,
            icon_gap: ICON_GAP,
            name_size: 12.5,
            line_height: 1.3,
        }
    }
}

impl Metrics {
    pub fn of(resolved: &ResolvedTheme) -> Self {
        let name_size = native_theme_iced::font_size(resolved).clamp(10.0, 20.0);
        let line_height = native_theme_iced::line_height_multiplier(resolved).max(1.0);

        Self {
            chip_size: chip_size(ICON_SIZE, name_size, line_height),
            card_radius: native_theme_iced::border_radius(resolved).clamp(4.0, 16.0),
            card_padding: CARD_PADDING,
            icon_size: ICON_SIZE,
            icon_gap: ICON_GAP,
            name_size,
            line_height,
        }
    }
}

impl Colors {
    pub fn of(theme: &Theme) -> Self {
        let base = theme.palette();
        let extended = theme.extended_palette();
        let dark = extended.is_dark;
        let background = base.background.scale_alpha(SURFACE_ALPHA);

        let shade = |amount: f32| {
            if dark {
                iced_palette::lighten(background, amount)
            } else {
                iced_palette::darken(background, amount)
            }
        };

        let (surface, card_hover, card_pressed, border, border_hover) = if dark {
            (
                background,
                shade(0.10),
                shade(0.15),
                shade(0.12),
                shade(0.20),
            )
        } else {
            (
                shade(0.06),
                shade(0.04),
                shade(0.09),
                shade(0.11),
                shade(0.17),
            )
        };

        Self {
            surface,
            card_hover,
            card_pressed,
            card_selected: extended.primary.weak.color.scale_alpha(SURFACE_ALPHA),
            border,
            border_hover,
            accent: base.primary,
            text: base.text,
        }
    }
}

pub fn resolve(mode: ThemeMode, system: Mode) -> (Option<Theme>, Metrics) {
    let system_theme = native_theme_iced::SystemTheme::from_system().ok();

    let color = match mode {
        ThemeMode::Light => ColorMode::Light,
        ThemeMode::Dark => ColorMode::Dark,
        ThemeMode::System => match system {
            Mode::Dark => ColorMode::Dark,
            Mode::Light => ColorMode::Light,
            Mode::None => match &system_theme {
                Some(system_theme) => system_theme.mode,
                None => ColorMode::Light,
            },
        },
    };

    match &system_theme {
        Some(system_theme) => {
            let resolved = system_theme.pick(color);

            (
                Some(native_theme_iced::to_theme(resolved, &system_theme.name)),
                Metrics::of(resolved),
            )
        }
        None => {
            let theme = match color {
                ColorMode::Dark => Theme::Dark,
                _ => Theme::Light,
            };

            (Some(theme), Metrics::default())
        }
    }
}

pub fn surface(theme: &Theme) -> container::Style {
    let colors = Colors::of(theme);

    container::Style {
        background: Some(Background::Color(colors.surface)),
        border: Border {
            color: colors.border,
            width: 1.0,
            radius: WINDOW_RADIUS.into(),
        },
        ..container::Style::default()
    }
}

pub fn tooltip(theme: &Theme) -> container::Style {
    let colors = Colors::of(theme);

    container::Style {
        background: Some(Background::Color(theme.palette().background)),
        border: Border {
            color: colors.border_hover,
            width: 1.0,
            radius: WINDOW_RADIUS.into(),
        },
        text_color: Some(colors.text),
        ..container::Style::default()
    }
}
