use std::cell::RefCell;

use tray_icon::{
    Icon, TrayIcon, TrayIconBuilder,
    menu::{
        Menu, MenuItem, PredefinedMenuItem,
        accelerator::{Accelerator, Modifiers},
    },
};

use crate::i18n;

const ICON: &[u8] = include_bytes!("../assets/tray_icon.png");
const VERSION: &str = env!("CARGO_PKG_VERSION");

thread_local! {
    static TRAY: RefCell<Option<TrayIcon>> = const { RefCell::new(None) };
}

pub fn build() -> anyhow::Result<TrayIcon> {
    let tray = TrayIconBuilder::new()
        .with_icon(icon()?)
        .with_icon_as_template(true)
        .with_tooltip("atray")
        .with_menu(Box::new(menu()?))
        .build()?;

    TRAY.with(|slot| *slot.borrow_mut() = Some(tray.clone()));

    Ok(tray)
}

pub fn relocalize() {
    let menu = match menu() {
        Ok(menu) => menu,
        Err(err) => {
            eprintln!("failed to build tray menu: {err}");
            return;
        }
    };

    TRAY.with(|slot| {
        if let Some(tray) = slot.borrow().as_ref() {
            tray.set_menu(Some(Box::new(menu)));
        }
    });
}

fn menu() -> anyhow::Result<Menu> {
    let modifier = if cfg!(target_os = "windows") {
        Modifiers::CONTROL
    } else {
        Modifiers::META
    };

    let version = MenuItem::new(
        i18n::t_args("menu-version", &[("version", VERSION.into())]),
        false,
        None,
    );
    let settings = MenuItem::with_id("settings", i18n::t("menu-settings"), true, None);
    let config_file = MenuItem::with_id("config_file", i18n::t("menu-open-config"), true, None);
    let reload_config_file = MenuItem::with_id(
        "reload_config_file",
        i18n::t("menu-reload-config"),
        true,
        None,
    );
    let quit = MenuItem::with_id("quit", i18n::t("menu-quit"), true, None);

    settings.set_accelerator(Some(Accelerator::new(
        modifier.clone(),
        tray_icon::menu::accelerator::Code::Comma,
    )))?;
    quit.set_accelerator(Some(Accelerator::new(
        modifier.clone(),
        tray_icon::menu::accelerator::Code::KeyQ,
    )))?;

    Ok(Menu::with_items(&[
        &version,
        &PredefinedMenuItem::separator(),
        &settings,
        &PredefinedMenuItem::separator(),
        &config_file,
        &reload_config_file,
        &PredefinedMenuItem::separator(),
        &quit,
    ])?)
}

fn icon() -> anyhow::Result<Icon> {
    let image = image::load_from_memory(ICON)?.into_rgba8();
    let (width, height) = image.dimensions();

    Ok(Icon::from_rgba(image.into_raw(), width, height)?)
}
