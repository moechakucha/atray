use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};

#[cfg(target_os = "macos")]
use crate::platform::MacosInputHandler;

#[cfg(target_os = "windows")]
use crate::platform::WindowsInputHandler;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
use crate::platform::RdevInputHandler;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Modifier {
    #[serde(rename = "alt")]
    Alt,
    #[serde(rename = "control")]
    Control,
    #[serde(rename = "shift")]
    Shift,
    #[serde(rename = "super")]
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

        #[cfg(target_os = "windows")]
        let handler: Box<dyn InputHandler> = Box::new(WindowsInputHandler::init());

        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
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
