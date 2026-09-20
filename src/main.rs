use anyhow::Context;
use iced::Color;
use tray_icon::{
    Icon,
    menu::{Menu, MenuItem, PredefinedMenuItem},
};

use crate::app::App;

mod app;
mod config;
mod font;
mod input;
mod platform;
mod theme;
mod widget;

fn load_icon(bytes: &[u8]) -> anyhow::Result<Icon> {
    let image = image::load_from_memory(bytes)?.into_rgba8();
    let (width, height) = image.dimensions();
    let rgba = image.into_raw();
    Ok(Icon::from_rgba(rgba, width, height)?)
}

fn construct_menu(version: &str) -> anyhow::Result<Menu> {
    let version_label = MenuItem::new(format!("Version {version}"), false, None);
    let settings_button = MenuItem::with_id("settings", "Settings...", true, None);
    let config_file_button = MenuItem::with_id("config_file", "Open Config File", true, None);
    let reload_config_file_button =
        MenuItem::with_id("reload_config_file", "Reload Config File", true, None);
    let quit_button = MenuItem::with_id("quit", "Quit", true, None);

    Ok(Menu::with_items(&[
        &version_label,
        &PredefinedMenuItem::separator(),
        &settings_button,
        &PredefinedMenuItem::separator(),
        &config_file_button,
        &reload_config_file_button,
        &PredefinedMenuItem::separator(),
        &quit_button,
    ])?)
}

fn main() -> anyhow::Result<()> {
    platform::app_init()?;
    input::init_input();

    let config_dir = dirs::config_dir()
        .context("failed to retrieve system config dir")?
        .join("atray");
    if !config_dir.exists() {
        std::fs::create_dir_all(&config_dir)?;
    }

    let config_path = config_dir.join("config.toml");
    let config = config::Config::load(&config_path)?;
    config.save_to_original()?;

    let version = env!("CARGO_PKG_VERSION");
    let tray_icon_bytes = include_bytes!("../assets/tray_icon.png");

    let menu = construct_menu(&version)?;

    let _tray_icon = tray_icon::TrayIconBuilder::new()
        .with_icon(load_icon(tray_icon_bytes)?)
        .with_icon_as_template(true)
        .with_tooltip("atray")
        .with_menu(Box::new(menu))
        .build()
        .context("failed to build tray icon")?;

    let daemon = iced::daemon(move || App::new(config.clone()), App::update, App::view)
        .theme(App::theme)
        .title(App::title)
        .style(|_, theme| iced::theme::Style {
            background_color: Color::TRANSPARENT,
            ..iced::theme::Base::base(theme)
        })
        .subscription(App::subscription);

    match font::system() {
        Some(font) => daemon.default_font(font).run(),
        None => {
            eprintln!("failed to load system font, falling back to default");
            daemon.run()
        }
    }
    .context("failed to run daemon")
}
