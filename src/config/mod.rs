use std::path::{Path, PathBuf};

use iced::window;
use iced::{Point, Size};
use regex::Regex;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub const TRAY_THICKNESS: f32 = 150.0;
pub const TRAY_LENGTH: f32 = 430.0;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(skip)]
    path: PathBuf,
    #[serde(default)]
    pub behavior: BehaviorConfig,
    #[serde(default)]
    pub appearance: AppearanceConfig,
    #[serde(default)]
    pub advanced: AdvancedConfig,
}

impl Config {
    pub fn load<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let path = path.as_ref();
        if !path.exists() {
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(path)?;
        let mut config: Self = toml::from_str(&content)?;
        config.path = path.to_path_buf();
        Ok(config)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    #[allow(unused)]
    pub fn save<P: AsRef<Path>>(&self, path: P) -> anyhow::Result<()> {
        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    pub fn save_to_original(&self) -> anyhow::Result<()> {
        self.save(&self.path)
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            path: dirs::config_dir()
                .unwrap_or("~/.config".into())
                .join("atray")
                .join("config.toml")
                .to_path_buf(),
            advanced: AdvancedConfig::default(),
            behavior: BehaviorConfig::default(),
            appearance: AppearanceConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdvancedConfig {
    pub cache_dir: String,
}

impl Default for AdvancedConfig {
    fn default() -> Self {
        Self {
            cache_dir: dirs::cache_dir()
                .unwrap_or("~/.cache".into())
                .join("atray")
                .to_string_lossy()
                .into_owned(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehaviorConfig {
    pub move_modifier: crate::input::Modifier,
    pub invert_copy_and_move: bool,
    #[serde(default)]
    pub filter: Vec<FilterRule>,
}

impl BehaviorConfig {
    pub fn allows(&self, source: Option<&crate::platform::DragSource>) -> bool {
        self.filter
            .iter()
            .find(|rule| rule.matches(source))
            .is_none_or(|rule| rule.action == FilterAction::Allow)
    }
}

impl Default for BehaviorConfig {
    fn default() -> Self {
        Self {
            move_modifier: crate::input::Modifier::Shift,
            invert_copy_and_move: false,
            filter: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FilterRule {
    #[serde(default)]
    pub app: Option<Pattern>,
    #[serde(default)]
    pub title: Option<Pattern>,
    #[serde(default)]
    pub action: FilterAction,
}

impl FilterRule {
    pub fn matches(&self, source: Option<&crate::platform::DragSource>) -> bool {
        let app = source.and_then(|source| source.app_name.as_deref());
        let title = source.and_then(|source| source.window_title.as_deref());

        self.app.as_ref().is_none_or(|pattern| pattern.matches(app))
            && self
                .title
                .as_ref()
                .is_none_or(|pattern| pattern.matches(title))
    }
}

#[derive(Debug, Clone)]
pub struct Pattern(Regex);

impl Pattern {
    pub fn parse(pattern: &str) -> Result<Self, regex::Error> {
        Regex::new(pattern).map(Pattern)
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    fn matches(&self, value: Option<&str>) -> bool {
        value.is_some_and(|value| self.0.is_match(value))
    }
}

impl PartialEq for Pattern {
    fn eq(&self, other: &Self) -> bool {
        self.0.as_str() == other.0.as_str()
    }
}

impl Serialize for Pattern {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.0.as_str())
    }
}

impl<'de> Deserialize<'de> for Pattern {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let pattern = String::deserialize(deserializer)?;
        Pattern::parse(&pattern).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FilterAction {
    Allow,
    #[default]
    Deny,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppearanceConfig {
    #[serde(default)]
    pub side: Side,
    #[serde(default)]
    pub theme: ThemeMode,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    #[default]
    System,
    Light,
    Dark,
}

impl AppearanceConfig {
    pub fn into_settings(&self) -> window::Settings {
        window::Settings {
            size: window_size(self.side).into(),
            position: window::Position::SpecificWith(match self.side {
                Side::Left => offscreen_left,
                Side::Right => offscreen_right,
                Side::Top => offscreen_top,
                Side::Bottom => offscreen_bottom,
            }),
            decorations: false,
            closeable: false,
            resizable: false,
            transparent: true,
            minimizable: false,
            blur: true,
            level: window::Level::AlwaysOnTop,
            exit_on_close_request: false,
            platform_specific: crate::platform::platform_window_settings(),
            ..Default::default()
        }
    }
}

fn offscreen_left(window: Size, resolution: Size) -> Point {
    Point::new(-window.width, (resolution.height - window.height) / 2.0)
}

fn offscreen_right(window: Size, resolution: Size) -> Point {
    Point::new(resolution.width, (resolution.height - window.height) / 2.0)
}

fn offscreen_top(window: Size, resolution: Size) -> Point {
    Point::new((resolution.width - window.width) / 2.0, -window.height)
}

fn offscreen_bottom(window: Size, resolution: Size) -> Point {
    Point::new((resolution.width - window.width) / 2.0, resolution.height)
}

pub fn window_size(side: Side) -> Size {
    match side {
        Side::Left | Side::Right => Size::new(TRAY_THICKNESS, TRAY_LENGTH),
        Side::Top | Side::Bottom => Size::new(TRAY_LENGTH, TRAY_THICKNESS),
    }
}

impl Default for AppearanceConfig {
    fn default() -> Self {
        Self {
            side: Side::default(),
            theme: ThemeMode::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Side {
    #[serde(rename = "left")]
    Left,
    #[serde(rename = "right")]
    Right,
    #[serde(rename = "bottom")]
    Bottom,
    #[serde(rename = "top")]
    Top,
}

impl Default for Side {
    fn default() -> Self {
        Self::Left
    }
}

impl Side {
    pub fn horizontal(self) -> bool {
        matches!(self, Self::Top | Self::Bottom)
    }
}

pub mod screen;
