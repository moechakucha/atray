use anyhow::Context;
use iced::Color;
use tray_icon::{
    Icon,
    menu::{Menu, MenuItem},
};

use crate::app::App;

mod app;
mod font;
mod platform;
mod theme;
mod widget;

fn load_icon(bytes: &[u8]) -> anyhow::Result<Icon> {
    let image = image::load_from_memory(bytes)?.into_rgba8();
    let (width, height) = image.dimensions();
    let rgba = image.into_raw();
    Ok(Icon::from_rgba(rgba, width, height)?)
}

fn main() -> anyhow::Result<()> {
    platform::app_init()?;
    platform::init_input();

    let version = env!("CARGO_PKG_VERSION");
    let icon_bytes = include_bytes!("../assets/icon.png");

    let version_label = MenuItem::new(format!("Version {version}"), false, None);
    let quit_button = MenuItem::with_id("quit", "Quit", true, None);

    let menu = Menu::with_items(&[&version_label, &quit_button])?;

    let _tray_icon = tray_icon::TrayIconBuilder::new()
        .with_icon(load_icon(icon_bytes)?)
        .with_icon_as_template(true)
        .with_tooltip("atray")
        .with_menu(Box::new(menu))
        .build()
        .context("failed to build tray icon")?;

    let daemon = iced::daemon(App::new, App::update, App::view)
        .theme(App::theme)
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
