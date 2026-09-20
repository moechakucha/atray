use iced::theme::Mode;
use iced::theme::palette as iced_palette;
use iced::widget::{button, container, pick_list, text_input};
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
    pub danger: Color,
}

#[derive(Debug, Clone, Copy)]
pub struct Metrics {
    pub chip_size: f32,
    pub small_radius: f32,
    pub large_radius: f32,
    pub line_width: f32,
    pub card_padding: f32,
    pub icon_size: f32,
    pub icon_gap: f32,
    pub name_size: f32,
    pub line_height: f32,
}

pub const SPACING: f32 = 6.0;
pub const VIEW_PADDING: f32 = 8.0;

const SURFACE_ALPHA: f32 = 0.4;
const TINT_ALPHA: f32 = 0.72;
pub const CARD_PADDING: f32 = 8.0;
pub const ICON_SIZE: f32 = 64.0;
const ICON_GAP: f32 = 4.0;

fn chip_size(icon_size: f32, name_size: f32, line_height: f32) -> f32 {
    (CARD_PADDING * 2.0 + icon_size + ICON_GAP + name_size * line_height).ceil()
}

fn large_radius(fallback: f32) -> f32 {
    match crate::platform::window_radius() {
        0.0 => fallback,
        radius => radius,
    }
}

impl Default for Metrics {
    fn default() -> Self {
        Self {
            chip_size: chip_size(ICON_SIZE, 12.5, 1.3),
            small_radius: 10.0,
            large_radius: large_radius(10.0),
            line_width: 1.0,
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
        let small_radius = native_theme_iced::border_radius(resolved).clamp(4.0, 16.0);

        Self {
            chip_size: chip_size(ICON_SIZE, name_size, line_height),
            small_radius,
            large_radius: large_radius(
                native_theme_iced::border_radius_lg(resolved).clamp(small_radius, 24.0),
            ),
            line_width: resolved.defaults.border.line_width.clamp(0.5, 2.0),
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
        let opaque = if dark {
            base.background
        } else {
            shade(base.background, theme, 0.06)
        };

        let surface = opaque.scale_alpha(SURFACE_ALPHA);
        let tint = |amount: f32| shade(opaque, theme, amount).scale_alpha(TINT_ALPHA);

        let (card_hover, card_pressed, border, border_hover) = if dark {
            (tint(0.10), tint(0.15), tint(0.12), tint(0.20))
        } else {
            (tint(0.04), tint(0.09), tint(0.11), tint(0.17))
        };

        Self {
            surface,
            card_hover,
            card_pressed,
            card_selected: extended.primary.weak.color.scale_alpha(TINT_ALPHA),
            border,
            border_hover,
            accent: base.primary,
            text: base.text,
            danger: base.danger,
        }
    }

    pub fn note(&self) -> Color {
        self.text.scale_alpha(0.6)
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

pub fn surface(theme: &Theme, metrics: Metrics) -> container::Style {
    let colors = Colors::of(theme);

    container::Style {
        background: Some(Background::Color(colors.surface)),
        border: Border {
            color: colors.border,
            width: metrics.line_width,
            radius: metrics.large_radius.into(),
        },
        ..container::Style::default()
    }
}

pub fn tooltip(theme: &Theme, metrics: Metrics) -> container::Style {
    let colors = Colors::of(theme);

    container::Style {
        background: Some(Background::Color(theme.palette().background)),
        border: Border {
            color: colors.border_hover,
            width: metrics.line_width,
            radius: metrics.large_radius.into(),
        },
        text_color: Some(colors.text),
        ..container::Style::default()
    }
}

pub fn settings_window(theme: &Theme) -> container::Style {
    let colors = Colors::of(theme);

    container::Style {
        background: Some(Background::Color(colors.surface)),
        text_color: Some(colors.text),
        ..container::Style::default()
    }
}

pub fn settings_sidebar(theme: &Theme) -> container::Style {
    let colors = Colors::of(theme);
    let background = shade(colors.surface, theme, 0.05);

    container::Style {
        background: Some(Background::Color(background)),
        text_color: Some(colors.text),
        ..container::Style::default()
    }
}

pub fn settings_card(theme: &Theme, metrics: Metrics) -> container::Style {
    let colors = Colors::of(theme);
    let background = shade(colors.surface, theme, 0.08);

    container::Style {
        background: Some(Background::Color(background)),
        border: Border {
            color: colors.border,
            width: metrics.line_width,
            radius: metrics.large_radius.into(),
        },
        text_color: Some(colors.text),
        ..container::Style::default()
    }
}

pub fn settings_input(
    theme: &Theme,
    status: text_input::Status,
    metrics: Metrics,
) -> text_input::Style {
    let mut style = text_input::default(theme, status);
    style.border.radius = metrics.small_radius.into();
    style.border.width = metrics.line_width;
    style
}

pub fn settings_pick_list(
    theme: &Theme,
    status: pick_list::Status,
    metrics: Metrics,
) -> pick_list::Style {
    let mut style = pick_list::default(theme, status);
    style.border.radius = metrics.small_radius.into();
    style.border.width = metrics.line_width;
    style
}

pub fn settings_tab(
    theme: &Theme,
    status: button::Status,
    selected: bool,
    metrics: Metrics,
) -> button::Style {
    let colors = Colors::of(theme);

    let background = if selected {
        Some(Background::Color(colors.card_selected))
    } else if status == button::Status::Hovered {
        Some(Background::Color(colors.card_hover))
    } else {
        None
    };

    button::Style {
        background,
        text_color: colors.text,
        border: Border {
            radius: metrics.small_radius.into(),
            ..Border::default()
        },
        shadow: Default::default(),
        snap: true,
    }
}

pub fn settings_button(theme: &Theme, status: button::Status, metrics: Metrics) -> button::Style {
    let colors = Colors::of(theme);

    let background = match status {
        button::Status::Hovered => colors.card_hover,
        button::Status::Pressed => colors.card_pressed,
        _ => colors.surface,
    };

    button::Style {
        background: Some(Background::Color(background)),
        text_color: colors.text,
        border: Border {
            color: colors.border,
            width: metrics.line_width,
            radius: metrics.small_radius.into(),
        },
        shadow: Default::default(),
        snap: true,
    }
}

pub fn settings_primary_button(
    theme: &Theme,
    status: button::Status,
    metrics: Metrics,
) -> button::Style {
    let colors = Colors::of(theme);
    let extended = theme.extended_palette();

    let background = match status {
        button::Status::Hovered => iced_palette::lighten(colors.accent, 0.08),
        button::Status::Pressed => iced_palette::darken(colors.accent, 0.08),
        _ => colors.accent,
    };

    button::Style {
        background: Some(Background::Color(background)),
        text_color: extended.primary.base.text,
        border: Border {
            radius: metrics.small_radius.into(),
            ..Border::default()
        },
        shadow: Default::default(),
        snap: true,
    }
}

fn shade(color: Color, theme: &Theme, amount: f32) -> Color {
    if theme.extended_palette().is_dark {
        iced_palette::lighten(color, amount)
    } else {
        iced_palette::darken(color, amount)
    }
}
