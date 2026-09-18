use std::path::PathBuf;

#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "macos")]
pub use macos::*;

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
