use auto_launch::{AutoLaunch, AutoLaunchBuilder};

#[cfg(target_os = "linux")]
use auto_launch::LinuxLaunchMode;
#[cfg(target_os = "macos")]
use auto_launch::MacOSLaunchMode;
#[cfg(target_os = "windows")]
use auto_launch::WindowsEnableMode;

const APP_NAME: &str = "atray";

fn launcher() -> anyhow::Result<AutoLaunch> {
    let path = std::env::current_exe()?;
    let path = path.to_string_lossy();

    let mut builder = AutoLaunchBuilder::new();

    builder.set_app_name(APP_NAME).set_app_path(&path);

    #[cfg(target_os = "macos")]
    builder.set_macos_launch_mode(MacOSLaunchMode::SMAppService);

    #[cfg(target_os = "linux")]
    builder.set_linux_launch_mode(LinuxLaunchMode::XdgAutostart);

    #[cfg(target_os = "windows")]
    builder.set_windows_enable_mode(WindowsEnableMode::CurrentUser);

    Ok(builder.build()?)
}

pub fn set(enabled: bool) -> anyhow::Result<()> {
    let launcher = launcher()?;

    if launcher.is_enabled()? == enabled {
        return Ok(());
    }

    if enabled {
        launcher.enable()?;
    } else {
        launcher.disable()?;
    }

    Ok(())
}

pub fn is_enabled() -> anyhow::Result<bool> {
    Ok(launcher()?.is_enabled()?)
}
