use std::{
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
};

use windows::{
    Win32::UI::{
        Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON, VK_RBUTTON},
        WindowsAndMessaging::FindWindowW,
    },
    core::w,
};

use crate::platform::DragHandler;

static DRAGGING: AtomicBool = AtomicBool::new(false);
static LAST_LOGGED: AtomicBool = AtomicBool::new(false);

pub fn app_init() -> anyhow::Result<()> {
    Ok(())
}

pub fn platform_window_settings() -> iced::window::settings::PlatformSpecific {
    iced::window::settings::PlatformSpecific {
        drag_and_drop: true,
        skip_taskbar: true,
        ..Default::default()
    }
}

pub struct WindowsDragHandler;

impl DragHandler for WindowsDragHandler {
    fn new() -> Self {
        Self
    }

    fn is_dragging(&self) -> bool {
        if !mouse_button_down() {
            DRAGGING.store(false, Ordering::Relaxed);
        } else if !DRAGGING.load(Ordering::Relaxed) && drag_image_present() {
            DRAGGING.store(true, Ordering::Relaxed);
        }

        let dragging = DRAGGING.load(Ordering::Relaxed);

        if LAST_LOGGED.swap(dragging, Ordering::Relaxed) != dragging {
            eprintln!("[atray] dragging -> {dragging}");
        }

        dragging
    }

    fn start_drag(&self, _paths: &[PathBuf]) -> bool {
        false
    }

    fn cancel_pending_drag(&self) {}

    fn take_drag_result(&self) -> Option<bool> {
        None
    }
}

fn drag_image_present() -> bool {
    unsafe { FindWindowW(w!("SysDragImage"), None).is_ok() }
}

fn mouse_button_down() -> bool {
    unsafe {
        GetAsyncKeyState(VK_LBUTTON.0 as i32) < 0 || GetAsyncKeyState(VK_RBUTTON.0 as i32) < 0
    }
}
