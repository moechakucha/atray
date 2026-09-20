use auto_launch::{
    AutoLaunch, AutoLaunchBuilder, LinuxLaunchMode, MacOSLaunchMode, WindowsEnableMode,
};

const APP_NAME: &str = "atray";

fn launcher() -> anyhow::Result<AutoLaunch> {
    let path = std::env::current_exe()?;
    let path = path.to_string_lossy();

    Ok(AutoLaunchBuilder::new()
        .set_app_name(APP_NAME)
        .set_app_path(&path)
        .set_macos_launch_mode(MacOSLaunchMode::SMAppService)
        .set_linux_launch_mode(LinuxLaunchMode::XdgAutostart)
        .set_windows_enable_mode(WindowsEnableMode::CurrentUser)
        .build()?)
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
