use std::{
    path::{Path, PathBuf},
    ptr::NonNull,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicIsize, Ordering},
    },
    time::{Duration, Instant},
};

use block2::RcBlock;
use objc2::{
    AnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send,
    rc::Retained,
    runtime::{NSObject, NSObjectProtocol, ProtocolObject},
};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSDragOperation, NSDraggingContext,
    NSDraggingItem, NSDraggingSession, NSDraggingSource, NSEvent, NSEventMask, NSImage,
    NSPasteboard, NSPasteboardNameDrag, NSPasteboardTypeFileURL, NSWorkspace,
};

use objc2_core_foundation::{CFArray, CFDictionary, CFNumber, CFRetained, CFString, CFType};
use objc2_core_graphics::{
    CGEventFlags, CGEventSource, CGEventSourceStateID, CGWindowListCopyWindowInfo,
    CGWindowListOption, kCGNullWindowID, kCGWindowLayer, kCGWindowName, kCGWindowOwnerName,
};
use objc2_foundation::{
    NSArray, NSDate, NSError, NSPoint, NSRect, NSRunLoop, NSSize, NSString, NSURL,
};
use objc2_quick_look_thumbnailing::{
    QLThumbnailGenerationRequest, QLThumbnailGenerationRequestRepresentationTypes,
    QLThumbnailGenerator, QLThumbnailRepresentation,
};

use crate::input::{InputEvent, InputHandler, InputSink, Modifier, Modifiers, dispatch};
use crate::platform::{DragHandler, DragSource, FileIcon};

type DragSourceHandle = Retained<ProtocolObject<dyn NSDraggingSource>>;

struct DragSourceIvars {
    result: Arc<Mutex<Option<bool>>>,
}

#[derive(Clone)]
pub struct MacosDragHandler {
    is_dragging: Arc<AtomicBool>,
    pending: Arc<Mutex<Option<Vec<PathBuf>>>>,
    result: Arc<Mutex<Option<bool>>>,
    source: Arc<Mutex<Option<DragSourceHandle>>>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = DragSourceIvars]
    #[name = "AtrayDragSource"]
    struct AtrayDragSource;

    unsafe impl NSObjectProtocol for AtrayDragSource {}

    #[allow(non_snake_case)]
    unsafe impl NSDraggingSource for AtrayDragSource {
        #[unsafe(method(draggingSession:sourceOperationMaskForDraggingContext:))]
        fn draggingSession_sourceOperationMaskForDraggingContext(
            &self,
            _session: &NSDraggingSession,
            _context: NSDraggingContext,
        ) -> NSDragOperation {
            NSDragOperation::Copy
        }

        #[unsafe(method(draggingSession:willBeginAtPoint:))]
        fn draggingSession_willBeginAtPoint(&self, _session: &NSDraggingSession, _point: NSPoint) {}

        #[unsafe(method(draggingSession:movedToPoint:))]
        fn draggingSession_movedToPoint(&self, _session: &NSDraggingSession, _point: NSPoint) {}

        #[unsafe(method(draggingSession:endedAtPoint:operation:))]
        fn draggingSession_endedAtPoint_operation(
            &self,
            _session: &NSDraggingSession,
            _point: NSPoint,
            operation: NSDragOperation,
        ) {
            *self.ivars().result.lock().unwrap() = Some(operation.0 != 0);
        }

        #[unsafe(method(ignoreModifierKeysForDraggingSession:))]
        fn ignoreModifierKeysForDraggingSession(&self, _session: &NSDraggingSession) -> bool {
            false
        }
    }
);

impl AtrayDragSource {
    fn new(mtm: MainThreadMarker, result: Arc<Mutex<Option<bool>>>) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(DragSourceIvars { result });
        unsafe { msg_send![super(this), init] }
    }
}

impl MacosDragHandler {
    pub fn init() -> Self {
        let is_dragging = Arc::new(AtomicBool::new(false));
        let pending: Arc<Mutex<Option<Vec<PathBuf>>>> = Arc::new(Mutex::new(None));
        let result: Arc<Mutex<Option<bool>>> = Arc::new(Mutex::new(None));
        let source: Arc<Mutex<Option<DragSourceHandle>>> = Arc::new(Mutex::new(None));

        let flag = Arc::clone(&is_dragging);
        let baseline = Arc::new(AtomicIsize::new(drag_change_count()));

        let baseline_for_down = Arc::clone(&baseline);
        let down_monitor = NSEvent::addGlobalMonitorForEventsMatchingMask_handler(
            NSEventMask::LeftMouseDown,
            &RcBlock::new(move |_event: NonNull<NSEvent>| {
                baseline_for_down.store(drag_change_count(), Ordering::Relaxed);
                flag.store(false, Ordering::Relaxed);
            }),
        );

        let flag = Arc::clone(&is_dragging);
        let baseline_for_drag = Arc::clone(&baseline);
        let drag_monitor = NSEvent::addGlobalMonitorForEventsMatchingMask_handler(
            NSEventMask::LeftMouseDragged,
            &RcBlock::new(move |_event: NonNull<NSEvent>| {
                if drag_change_count() != baseline_for_drag.load(Ordering::Relaxed)
                    && is_file_drag()
                {
                    flag.store(true, Ordering::Relaxed);
                }
            }),
        );

        let flag = Arc::clone(&is_dragging);
        let release_monitor = NSEvent::addGlobalMonitorForEventsMatchingMask_handler(
            NSEventMask::LeftMouseUp,
            &RcBlock::new(move |_event: NonNull<NSEvent>| {
                flag.store(false, Ordering::Relaxed);
            }),
        );

        let pending_for_monitor = Arc::clone(&pending);
        let result_for_monitor = Arc::clone(&result);
        let source_for_monitor = Arc::clone(&source);
        let local_monitor = unsafe {
            NSEvent::addLocalMonitorForEventsMatchingMask_handler(
                NSEventMask::LeftMouseDragged,
                &RcBlock::new(move |event: NonNull<NSEvent>| {
                    if let Some(paths) = pending_for_monitor.lock().unwrap().take() {
                        begin_drag(
                            event.as_ref(),
                            &paths,
                            &result_for_monitor,
                            &source_for_monitor,
                        );
                    }

                    event.as_ptr()
                }),
            )
        };

        let flag = Arc::clone(&is_dragging);
        let local_release = unsafe {
            NSEvent::addLocalMonitorForEventsMatchingMask_handler(
                NSEventMask::LeftMouseUp,
                &RcBlock::new(move |event: NonNull<NSEvent>| {
                    flag.store(false, Ordering::Relaxed);
                    event.as_ptr()
                }),
            )
        };

        std::mem::forget(down_monitor);
        std::mem::forget(drag_monitor);
        std::mem::forget(release_monitor);
        std::mem::forget(local_monitor);
        std::mem::forget(local_release);

        Self {
            is_dragging,
            pending,
            result,
            source,
        }
    }

    pub fn is_dragging(&self) -> bool {
        self.is_dragging.load(Ordering::Relaxed)
    }

    pub fn take_drag_result(&self) -> Option<bool> {
        let result = self.result.lock().unwrap().take();

        if result.is_some() {
            self.source.lock().unwrap().take();
        }

        result
    }
}

impl DragHandler for MacosDragHandler {
    fn new() -> Self {
        MacosDragHandler::init()
    }

    fn is_dragging(&self) -> bool {
        MacosDragHandler::is_dragging(self)
    }

    fn start_drag(&self, paths: &[PathBuf]) -> bool {
        if paths.is_empty() {
            return false;
        }

        *self.pending.lock().unwrap() = Some(paths.to_vec());
        true
    }

    fn cancel_pending_drag(&self) {
        self.pending.lock().unwrap().take();
    }

    fn take_drag_result(&self) -> Option<bool> {
        MacosDragHandler::take_drag_result(self)
    }
}

fn begin_drag(
    event: &NSEvent,
    paths: &[PathBuf],
    result: &Arc<Mutex<Option<bool>>>,
    source_slot: &Arc<Mutex<Option<DragSourceHandle>>>,
) {
    let Some(mtm) = MainThreadMarker::new() else {
        *result.lock().unwrap() = Some(false);
        return;
    };

    let Some(window) = event.window(mtm) else {
        *result.lock().unwrap() = Some(false);
        return;
    };

    let location = event.locationInWindow();
    let frame = NSRect::new(location, NSSize::new(1.0, 1.0));
    let workspace = NSWorkspace::sharedWorkspace();

    let mut items = Vec::new();

    for path in paths {
        let Some(url) = NSURL::from_file_path(path) else {
            continue;
        };

        let writer = ProtocolObject::from_retained(url);
        let item = NSDraggingItem::initWithPasteboardWriter(NSDraggingItem::alloc(), &writer);
        let icon = workspace.iconForFile(&NSString::from_str(&path.to_string_lossy()));

        unsafe { item.setDraggingFrame_contents(frame, Some(&icon)) };

        items.push(item);
    }

    if items.is_empty() {
        *result.lock().unwrap() = Some(false);
        return;
    }

    let items = NSArray::from_retained_slice(&items);
    let source = AtrayDragSource::new(mtm, Arc::clone(result));
    let source: DragSourceHandle = ProtocolObject::from_retained(source);

    window.beginDraggingSessionWithItems_event_source(&items, event, &source);

    *source_slot.lock().unwrap() = Some(source);
}

fn drag_pasteboard() -> Retained<NSPasteboard> {
    NSPasteboard::pasteboardWithName(unsafe { NSPasteboardNameDrag })
}

fn drag_change_count() -> isize {
    drag_pasteboard().changeCount() as isize
}

fn is_file_drag() -> bool {
    let pasteboard = drag_pasteboard();

    let Some(types) = pasteboard.types() else {
        return false;
    };

    types.containsObject(unsafe { NSPasteboardTypeFileURL })
}

pub fn app_init() -> anyhow::Result<()> {
    let Some(mtm) = MainThreadMarker::new() else {
        return Ok(());
    };

    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);

    Ok(())
}

pub fn platform_window_settings() -> iced::window::settings::PlatformSpecific {
    iced::window::settings::PlatformSpecific {
        title_hidden: true,
        ..Default::default()
    }
}

pub fn drag_source() -> Option<DragSource> {
    let app = NSWorkspace::sharedWorkspace().frontmostApplication()?;
    let app_name = app.localizedName().map(|name| name.to_string());
    let window_title = app_name.as_deref().and_then(front_window_title);

    Some(DragSource {
        app_name,
        window_title,
    })
}

fn front_window_title(owner: &str) -> Option<String> {
    let windows = CGWindowListCopyWindowInfo(
        CGWindowListOption::OptionOnScreenOnly | CGWindowListOption::ExcludeDesktopElements,
        kCGNullWindowID,
    )?;

    let windows: CFRetained<CFArray<CFDictionary<CFString, CFType>>> =
        unsafe { CFRetained::cast_unchecked(windows) };

    for window in windows.iter() {
        let layer = window
            .get(unsafe { kCGWindowLayer })
            .and_then(|value| value.downcast::<CFNumber>().ok())
            .and_then(|number| number.as_i32());

        if layer != Some(0) {
            continue;
        }

        let window_owner = window
            .get(unsafe { kCGWindowOwnerName })
            .and_then(|value| value.downcast::<CFString>().ok())
            .map(|name| name.to_string());

        if window_owner.as_deref() != Some(owner) {
            continue;
        }

        if let Some(title) = window.get(unsafe { kCGWindowName })
            && let Ok(title) = title.downcast::<CFString>()
        {
            return Some(title.to_string());
        }
    }

    None
}

pub fn file_icon(path: &Path, size: u32) -> Option<FileIcon> {
    let size = size.max(1);

    quicklook_thumbnail(path, size).or_else(|| workspace_icon(path, size))
}

fn workspace_icon(path: &Path, size: u32) -> Option<FileIcon> {
    let workspace = NSWorkspace::sharedWorkspace();
    let image = workspace.iconForFile(&NSString::from_str(&path.to_string_lossy()));

    decode_image(&image, size, false)
}

fn quicklook_thumbnail(path: &Path, size: u32) -> Option<FileIcon> {
    let url = NSURL::from_file_path(path)?;

    let tiff: Arc<Mutex<Option<Vec<u8>>>> = Arc::new(Mutex::new(None));
    let done = Arc::new(AtomicBool::new(false));

    let handler = {
        let tiff = Arc::clone(&tiff);
        let done = Arc::clone(&done);

        RcBlock::new(
            move |thumbnail: *mut QLThumbnailRepresentation, _error: *mut NSError| {
                if let Some(thumbnail) = unsafe { thumbnail.as_ref() } {
                    let image = unsafe { thumbnail.NSImage() };

                    *tiff.lock().unwrap() = image.TIFFRepresentation().map(|tiff| tiff.to_vec());
                }

                done.store(true, Ordering::Release);
            },
        )
    };

    let request = unsafe {
        QLThumbnailGenerationRequest::initWithFileAtURL_size_scale_representationTypes(
            QLThumbnailGenerationRequest::alloc(),
            &url,
            NSSize::new(size as f64, size as f64),
            1.0,
            QLThumbnailGenerationRequestRepresentationTypes::All,
        )
    };

    unsafe {
        QLThumbnailGenerator::sharedGenerator()
            .generateBestRepresentationForRequest_completionHandler(&request, &handler);
    }

    let deadline = Instant::now() + Duration::from_millis(500);

    while !done.load(Ordering::Acquire) && Instant::now() < deadline {
        NSRunLoop::currentRunLoop().runUntilDate(&NSDate::dateWithTimeIntervalSinceNow(0.005));
    }

    let tiff = tiff.lock().unwrap().take()?;

    decode_tiff(&tiff, size, true)
}

fn decode_image(image: &NSImage, size: u32, fit: bool) -> Option<FileIcon> {
    let representation = image.TIFFRepresentation()?;

    decode_tiff(&representation.to_vec(), size, fit)
}

fn decode_tiff(tiff: &[u8], size: u32, fit: bool) -> Option<FileIcon> {
    let source = image::load_from_memory_with_format(tiff, image::ImageFormat::Tiff).ok()?;

    let icon = if fit {
        let scaled = source
            .resize(size, size, image::imageops::FilterType::Lanczos3)
            .to_rgba8();
        let mut canvas = image::RgbaImage::new(size, size);
        let x = ((size - scaled.width()) / 2) as i64;
        let y = ((size - scaled.height()) / 2) as i64;

        image::imageops::overlay(&mut canvas, &scaled, x, y);

        canvas
    } else {
        source
            .resize_exact(size, size, image::imageops::FilterType::Lanczos3)
            .to_rgba8()
    };

    Some(FileIcon {
        width: icon.width(),
        height: icon.height(),
        rgba: icon.into_raw(),
    })
}

pub struct MacosInputHandler {
    sink: Arc<Mutex<Option<InputSink>>>,
}

impl MacosInputHandler {
    pub fn init() -> Self {
        let sink: Arc<Mutex<Option<InputSink>>> = Arc::new(Mutex::new(None));

        let sink_for_down = Arc::clone(&sink);
        let down_monitor = NSEvent::addGlobalMonitorForEventsMatchingMask_handler(
            NSEventMask::LeftMouseDown,
            &RcBlock::new(move |_event: NonNull<NSEvent>| {
                dispatch(&sink_for_down, InputEvent::PointerPressed);
            }),
        );

        let sink_for_up = Arc::clone(&sink);
        let up_monitor = NSEvent::addGlobalMonitorForEventsMatchingMask_handler(
            NSEventMask::LeftMouseUp,
            &RcBlock::new(move |_event: NonNull<NSEvent>| {
                dispatch(&sink_for_up, InputEvent::PointerReleased);
            }),
        );

        std::mem::forget(down_monitor);
        std::mem::forget(up_monitor);

        Self { sink }
    }
}

impl InputHandler for MacosInputHandler {
    fn modifiers(&self) -> Modifiers {
        let flags = CGEventSource::flags_state(CGEventSourceStateID::CombinedSessionState);
        let mut modifiers = Modifiers::default();

        modifiers.set(Modifier::Alt, flags.contains(CGEventFlags::MaskAlternate));
        modifiers.set(Modifier::Control, flags.contains(CGEventFlags::MaskControl));
        modifiers.set(Modifier::Shift, flags.contains(CGEventFlags::MaskShift));
        modifiers.set(Modifier::Super, flags.contains(CGEventFlags::MaskCommand));

        modifiers
    }

    fn listen(&self, sink: InputSink) {
        *self.sink.lock().unwrap() = Some(sink);
    }
}
