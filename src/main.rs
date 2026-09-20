use anyhow::Context;
use iced::Color;

use crate::app::App;

mod app;
mod autostart;
mod config;
mod font;
mod i18n;
mod input;
mod platform;
mod theme;
mod tray;
mod widget;

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

    i18n::init(config.appearance.language.as_deref());

    let _tray = tray::build().context("failed to build tray icon")?;

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
