use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "macos")]
pub use macos::*;

#[cfg(not(target_os = "macos"))]
mod rdev_input;

#[cfg(not(target_os = "macos"))]
use rdev_input::RdevInputHandler;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Modifier {
    Alt,
    Control,
    Shift,
    Super,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Modifiers(u8);

impl Modifiers {
    pub fn contains(self, modifier: Modifier) -> bool {
        self.0 & (1 << modifier as u8) != 0
    }

    pub(crate) fn set(&mut self, modifier: Modifier, pressed: bool) {
        if pressed {
            self.0 |= 1 << modifier as u8;
        } else {
            self.0 &= !(1 << modifier as u8);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InputEvent {
    PointerPressed,
    PointerReleased,
}

pub type InputSink = Box<dyn Fn(InputEvent) + Send + 'static>;

pub trait InputHandler: Send + Sync {
    fn modifiers(&self) -> Modifiers;

    fn listen(&self, sink: InputSink);
}

fn input_handler() -> &'static dyn InputHandler {
    static HANDLER: OnceLock<Box<dyn InputHandler>> = OnceLock::new();

    &**HANDLER.get_or_init(|| {
        #[cfg(target_os = "macos")]
        let handler: Box<dyn InputHandler> = Box::new(MacosInputHandler::init());

        #[cfg(not(target_os = "macos"))]
        let handler: Box<dyn InputHandler> = Box::new(RdevInputHandler::init());

        handler
    })
}

pub(crate) fn dispatch(sink: &Mutex<Option<InputSink>>, event: InputEvent) {
    if let Some(sink) = sink.lock().unwrap().as_ref() {
        sink(event);
    }
}

pub fn init_input() {
    let _ = input_handler();
}

pub fn modifiers() -> Modifiers {
    input_handler().modifiers()
}

pub fn listen_input(sink: InputSink) {
    input_handler().listen(sink);
}
