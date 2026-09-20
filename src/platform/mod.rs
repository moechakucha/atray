use std::path::PathBuf;

#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "macos")]
pub use macos::*;

#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "windows")]
pub use windows::*;

#[cfg(not(target_os = "macos"))]
mod rdev_input;

#[cfg(not(target_os = "macos"))]
pub use rdev_input::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragEffect {
    Copy,
    Move,
}

pub trait DragHandler {
    fn new() -> Self
    where
        Self: Sized;

    fn is_dragging(&self) -> bool;

    fn start_drag(&self, paths: &[PathBuf], effect: DragEffect) -> bool;

    fn cancel_pending_drag(&self);

    fn take_drag_result(&self) -> Option<bool>;
}

#[cfg(target_os = "macos")]
pub fn get_drag_handler() -> Box<dyn DragHandler> {
    Box::new(MacosDragHandler::new())
}

#[cfg(target_os = "windows")]
pub fn get_drag_handler() -> Box<dyn DragHandler> {
    Box::new(WindowsDragHandler::new())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn get_drag_handler() -> Box<dyn DragHandler> {
    todo!("implement a drag handler for this platform")
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn app_init() -> anyhow::Result<()> {
    Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn platform_window_settings() -> iced::window::settings::PlatformSpecific {
    iced::window::settings::PlatformSpecific::default()
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DragSource {
    pub app_name: Option<String>,
    pub window_title: Option<String>,
}

#[cfg(not(target_os = "macos"))]
pub fn drag_source() -> Option<DragSource> {
    None
}

#[cfg(not(target_os = "macos"))]
pub fn titlebar_inset() -> f32 {
    0.0
}

#[cfg(not(target_os = "macos"))]
pub fn window_radius() -> f32 {
    0.0
}

#[cfg(not(target_os = "macos"))]
pub fn preferred_languages() -> Vec<String> {
    Vec::new()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileIcon {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn file_icon(_path: &std::path::Path, _size: u32) -> Option<FileIcon> {
    None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowMaterial {
    Tray,
    Settings,
}

#[cfg(not(target_os = "macos"))]
pub fn apply_window_material(
    _window: iced::window::Id,
    _material: WindowMaterial,
    _radius: Option<f32>,
    _dark: bool,
) -> iced::Task<()> {
    iced::Task::none()
}
