use std::sync::{Arc, Mutex};

use rdev::{Button, EventType, Key};

use crate::input::{InputEvent, InputHandler, InputSink, Modifier, Modifiers, dispatch};

pub struct RdevInputHandler {
    pressed: Arc<Mutex<Modifiers>>,
    sink: Arc<Mutex<Option<InputSink>>>,
}

impl RdevInputHandler {
    pub fn init() -> Self {
        let pressed: Arc<Mutex<Modifiers>> = Arc::new(Mutex::new(Modifiers::default()));
        let sink: Arc<Mutex<Option<InputSink>>> = Arc::new(Mutex::new(None));

        let pressed_for_listen = Arc::clone(&pressed);
        let sink_for_listen = Arc::clone(&sink);

        std::thread::spawn(move || {
            let result = rdev::listen(move |event| match event.event_type {
                EventType::KeyPress(key) => set_pressed(&pressed_for_listen, key, true),
                EventType::KeyRelease(key) => set_pressed(&pressed_for_listen, key, false),
                EventType::ButtonPress(Button::Left) => {
                    dispatch(&sink_for_listen, InputEvent::PointerPressed);
                }
                EventType::ButtonRelease(Button::Left) => {
                    dispatch(&sink_for_listen, InputEvent::PointerReleased);
                }
                _ => {}
            });

            if let Err(err) = result {
                log::error!("failed to listen for rdev events: {err:?}");
            }
        });

        Self { pressed, sink }
    }
}

impl InputHandler for RdevInputHandler {
    fn modifiers(&self) -> Modifiers {
        *self.pressed.lock().unwrap()
    }

    fn listen(&self, sink: InputSink) {
        *self.sink.lock().unwrap() = Some(sink);
    }
}

fn set_pressed(pressed: &Mutex<Modifiers>, key: Key, down: bool) {
    let Some(modifier) = modifier_of(key) else {
        return;
    };

    let mut pressed = pressed.lock().unwrap();
    pressed.set(modifier, down);
}

fn modifier_of(key: Key) -> Option<Modifier> {
    match key {
        Key::Alt | Key::AltGr => Some(Modifier::Alt),
        Key::ControlLeft | Key::ControlRight => Some(Modifier::Control),
        Key::ShiftLeft | Key::ShiftRight => Some(Modifier::Shift),
        Key::MetaLeft | Key::MetaRight => Some(Modifier::Super),
        _ => None,
    }
}
