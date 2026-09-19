use std::path::PathBuf;

#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "macos")]
pub use macos::*;

#[cfg(not(target_os = "macos"))]
mod rdev_input;

#[cfg(not(target_os = "macos"))]
pub use rdev_input::*;

pub trait DragHandler {
    fn new() -> Self
    where
        Self: Sized;

    fn is_dragging(&self) -> bool;

    fn start_drag(&self, paths: &[PathBuf]) -> bool;

    fn cancel_pending_drag(&self);

    fn take_drag_result(&self) -> Option<bool>;
}

#[cfg(target_os = "macos")]
pub fn get_drag_handler() -> Box<dyn DragHandler> {
    Box::new(MacosDragHandler::new())
}

#[cfg(not(target_os = "macos"))]
pub fn get_drag_handler() -> Box<dyn DragHandler> {
    todo!("implement a drag handler for this platform")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileIcon {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

#[cfg(not(target_os = "macos"))]
pub fn file_icon(_path: &std::path::Path, _size: u32) -> Option<FileIcon> {
    None
}
