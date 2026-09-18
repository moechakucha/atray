use iced::theme::palette as iced_palette;
use iced::widget::container;
use iced::{Background, Border, Color, Theme};
use native_theme_iced::ResolvedTheme;

#[derive(Debug, Clone, Copy)]
pub struct Colors {
    pub surface: Color,
    pub card: Color,
    pub card_hover: Color,
    pub card_pressed: Color,
    pub card_selected: Color,
    pub border: Color,
    pub border_hover: Color,
    pub accent: Color,
    pub text: Color,
    pub text_muted: Color,
}

#[derive(Debug, Clone, Copy)]
pub struct Metrics {
    pub chip_height: f32,
    pub card_radius: f32,
    pub card_padding: f32,
    pub accent_width: f32,
    pub accent_gap: f32,
    pub name_size: f32,
    pub detail_size: f32,
    pub line_height: f32,
}

pub const SPACING: f32 = 6.0;
pub const VIEW_PADDING: f32 = 8.0;
pub const WINDOW_RADIUS: f32 = 12.0;

const SURFACE_ALPHA: f32 = 0.72;
pub const CARD_PADDING: f32 = 10.0;
pub const ACCENT_WIDTH: f32 = 3.0;
pub const ACCENT_GAP: f32 = 6.0;

impl Default for Metrics {
    fn default() -> Self {
        Self {
            chip_height: 54.0,
            card_radius: 10.0,
            card_padding: CARD_PADDING,
            accent_width: ACCENT_WIDTH,
            accent_gap: ACCENT_GAP,
            name_size: 12.5,
            detail_size: 10.0,
            line_height: 1.3,
        }
    }
}

impl Metrics {
    pub fn of(resolved: &ResolvedTheme) -> Self {
        let name_size = native_theme_iced::font_size(resolved).clamp(10.0, 20.0);
        let line_height = native_theme_iced::line_height_multiplier(resolved).max(1.0);
        let detail_size = (name_size * 0.8).max(9.0);
        let line_span = (name_size + detail_size) * line_height;

        Self {
            chip_height: (line_span + CARD_PADDING * 2.0).ceil(),
            card_radius: native_theme_iced::border_radius(resolved).clamp(4.0, 16.0),
            card_padding: CARD_PADDING,
            accent_width: ACCENT_WIDTH,
            accent_gap: ACCENT_GAP,
            name_size,
            detail_size,
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

        let (surface, card, card_hover, card_pressed, border, border_hover) = if dark {
            (
                background,
                shade(0.05),
                shade(0.10),
                shade(0.15),
                shade(0.12),
                shade(0.20),
            )
        } else {
            (
                shade(0.06),
                background,
                shade(0.04),
                shade(0.09),
                shade(0.11),
                shade(0.17),
            )
        };

        Self {
            surface,
            card,
            card_hover,
            card_pressed,
            card_selected: extended.primary.weak.color.scale_alpha(SURFACE_ALPHA),
            border,
            border_hover,
            accent: base.primary,
            text: base.text,
            text_muted: iced_palette::mix(base.text, base.background, 0.4),
        }
    }
}

pub fn system() -> Option<(Theme, Metrics)> {
    native_theme_iced::from_system()
        .ok()
        .map(|(theme, resolved, _)| (theme, Metrics::of(&resolved)))
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

pub fn accent(theme: &Theme, seed: &str) -> Color {
    let base = theme.palette();
    let extended = theme.extended_palette();

    let accents = [
        base.primary,
        base.success,
        base.warning,
        base.danger,
        extended.secondary.strong.color,
        extended.primary.strong.color,
    ];

    accents[(hash(seed) % accents.len() as u64) as usize]
}

fn hash(seed: &str) -> u64 {
    let mut hash: u64 = 1469598103934665603;

    for byte in seed.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(1099511628211);
    }

    hash
}
