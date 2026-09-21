#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

use anyhow::Context;

use crate::app::App;

mod app;
mod autostart;
mod config;
mod font;
mod i18n;
mod input;
mod logging;
mod platform;
mod theme;
mod tray;
mod widget;

fn main() {
    if let Err(error) = run() {
        log::error!("{error:?}");
        std::process::exit(1);
    }
}

fn run() -> anyhow::Result<()> {
    let config_dir = dirs::config_dir()
        .context("failed to retrieve system config dir")?
        .join("atray");
    if !config_dir.exists() {
        std::fs::create_dir_all(&config_dir)?;
    }

    logging::init(&config_dir);
    logging::install_panic_hook();

    log::info!(
        "{} v{} starting ({}, {})",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH
    );

    platform::app_init()?;
    input::init_input();

    let config_path = config_dir.join("config.toml");
    log::info!("config: {}", config_path.display());

    let config = config::Config::load(&config_path)?;
    config.save_to_original()?;

    log::info!(
        "theme: {:?}, side: {:?}, cache: {}",
        config.appearance.theme,
        config.appearance.side,
        config.advanced.cache_dir
    );

    i18n::init(config.appearance.language.as_deref());

    let _tray = tray::build().context("failed to build tray icon")?;

    let daemon = iced::daemon(move || App::new(config.clone()), App::update, App::view)
        .theme(App::theme)
        .title(App::title)
        .style(|_, theme| iced::theme::Style {
            background_color: crate::platform::window_background(theme.palette().background),
            ..iced::theme::Base::base(theme)
        })
        .subscription(App::subscription);

    match font::system() {
        Some(font) => daemon.default_font(font).run(),
        None => {
            log::warn!("no system font available, falling back to the bundled default");
            daemon.run()
        }
    }
    .context("failed to run daemon")
}
