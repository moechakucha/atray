use std::{
    ffi::c_void,
    mem::{ManuallyDrop, size_of},
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
    ptr,
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

use windows::{
    Win32::{
        Foundation::{
            CloseHandle, DRAGDROP_S_CANCEL, DRAGDROP_S_DROP, DRAGDROP_S_USEDEFAULTCURSORS,
            DV_E_FORMATETC, E_INVALIDARG, E_NOTIMPL, GlobalFree, HGLOBAL, HWND, LPARAM,
            OLE_E_ADVISENOTSUPPORTED, OLE_E_NOCONNECTION, POINT, S_FALSE, S_OK, SIZE, WPARAM,
        },
        Graphics::{
            Dwm::{
                DWMSBT_MAINWINDOW, DWMSBT_TRANSIENTWINDOW, DWMWA_SYSTEMBACKDROP_TYPE,
                DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
                DWMWINDOWATTRIBUTE, DwmSetWindowAttribute,
            },
            Gdi::{
                BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, DIB_RGB_COLORS,
                DeleteDC, DeleteObject, GetDIBits, GetObjectW, HBITMAP, HGDIOBJ,
            },
        },
        System::{
            Com::{
                DATADIR_GET, DVASPECT_CONTENT, FORMATETC, IAdviseSink, IDataObject,
                IDataObject_Impl, IEnumFORMATETC, IEnumFORMATETC_Impl, IEnumSTATDATA, STGMEDIUM,
                STGMEDIUM_0, TYMED_HGLOBAL,
            },
            Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock},
            Ole::{
                CF_HDROP, DROPEFFECT, DROPEFFECT_COPY, DROPEFFECT_MOVE, DROPEFFECT_NONE,
                DoDragDrop, IDropSource, IDropSource_Impl, OleInitialize,
            },
            SystemServices::{MK_LBUTTON, MK_RBUTTON, MODIFIERKEYS_FLAGS},
            Threading::{
                OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
                QueryFullProcessImageNameW,
            },
        },
        UI::{
            Input::KeyboardAndMouse::{
                GetAsyncKeyState, VIRTUAL_KEY, VK_LBUTTON, VK_LCONTROL, VK_LMENU, VK_LSHIFT,
                VK_LWIN, VK_RBUTTON, VK_RCONTROL, VK_RMENU, VK_RSHIFT, VK_RWIN,
            },
            Shell::{
                DROPFILES, IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF,
                SIIGBF_BIGGERSIZEOK, SIIGBF_ICONONLY, SIIGBF_THUMBNAILONLY,
            },
            WindowsAndMessaging::{
                FindWindowW, GetForegroundWindow, GetWindowThreadProcessId, SMTO_ABORTIFHUNG,
                SendMessageTimeoutW, WM_GETTEXT,
            },
        },
    },
    core::{BOOL, Error, HRESULT, PCWSTR, PWSTR, Ref, Result, implement, w},
};

use raw_window_handle::RawWindowHandle;

use crate::input::{InputHandler, InputSink, Modifier, Modifiers};
use crate::platform::{
    DragEffect, DragHandler, DragSource, FileIcon, RdevInputHandler, WindowMaterial,
};

const WINDOW_RADIUS: f32 = 8.0;

static DRAGGING: AtomicBool = AtomicBool::new(false);

pub fn app_init() -> anyhow::Result<()> {
    unsafe { OleInitialize(None)? };

    Ok(())
}

pub fn platform_window_settings() -> iced::window::settings::PlatformSpecific {
    iced::window::settings::PlatformSpecific {
        drag_and_drop: true,
        skip_taskbar: true,
        ..Default::default()
    }
}

pub fn window_radius() -> f32 {
    WINDOW_RADIUS
}

pub fn apply_window_material(
    window: iced::window::Id,
    material: WindowMaterial,
    _radius: Option<f32>,
    dark: bool,
) -> iced::Task<()> {
    iced::window::run(window, move |window| {
        let Ok(handle) = window.window_handle() else {
            return;
        };

        let RawWindowHandle::Win32(handle) = handle.as_raw() else {
            return;
        };

        let hwnd = HWND(handle.hwnd.get() as *mut c_void);
        let backdrop = match material {
            WindowMaterial::Tray => DWMSBT_TRANSIENTWINDOW,
            WindowMaterial::Settings => DWMSBT_MAINWINDOW,
        };

        unsafe {
            set_window_attribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, &BOOL::from(dark));
            set_window_attribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, &DWMWCP_ROUND);
            set_window_attribute(hwnd, DWMWA_SYSTEMBACKDROP_TYPE, &backdrop);
        }
    })
}

unsafe fn set_window_attribute<T>(hwnd: HWND, attribute: DWMWINDOWATTRIBUTE, value: &T) {
    let _ = unsafe {
        DwmSetWindowAttribute(
            hwnd,
            attribute,
            (value as *const T).cast(),
            size_of::<T>() as u32,
        )
    };
}

pub struct WindowsDragHandler {
    result: Mutex<Option<bool>>,
}

impl DragHandler for WindowsDragHandler {
    fn new() -> Self {
        Self {
            result: Mutex::new(None),
        }
    }

    fn is_dragging(&self) -> bool {
        if !mouse_button_down() {
            DRAGGING.store(false, Ordering::Relaxed);
        } else if !DRAGGING.load(Ordering::Relaxed) && drag_image_present() {
            DRAGGING.store(true, Ordering::Relaxed);
        }

        let dragging = DRAGGING.load(Ordering::Relaxed);

        dragging
    }

    fn start_drag(&self, paths: &[PathBuf], effect: DragEffect) -> bool {
        if paths.is_empty() {
            return false;
        }

        let data: IDataObject = AtrayDataObject {
            paths: paths.to_vec(),
        }
        .into();
        let source: IDropSource = AtrayDropSource.into();
        let allowed = match effect {
            DragEffect::Copy => DROPEFFECT_COPY,
            DragEffect::Move => DROPEFFECT_MOVE,
        };
        let mut performed = DROPEFFECT_NONE;

        let hr = unsafe { DoDragDrop(&data, &source, allowed, &mut performed) };

        *self.result.lock().unwrap() = Some(hr.is_ok() && performed != DROPEFFECT_NONE);

        true
    }

    fn cancel_pending_drag(&self) {}

    fn take_drag_result(&self) -> Option<bool> {
        self.result.lock().unwrap().take()
    }
}

fn drag_image_present() -> bool {
    drag_image_window().is_some()
}

fn drag_image_window() -> Option<HWND> {
    unsafe { FindWindowW(w!("SysDragImage"), None).ok() }
}

pub fn drag_source() -> Option<DragSource> {
    let window = drag_image_window()?;
    let mut pid = 0u32;

    unsafe { GetWindowThreadProcessId(window, Some(&mut pid)) };

    let app_name = process_name(pid);
    let window_title = app_name.as_deref().and_then(|_| foreground_title(pid));

    Some(DragSource {
        app_name,
        window_title,
    })
}

fn process_name(pid: u32) -> Option<String> {
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buffer = [0u16; 1024];
        let mut length = buffer.len() as u32;

        let result = QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut length,
        );

        let _ = CloseHandle(process);

        result.ok()?;

        let path = String::from_utf16_lossy(&buffer[..length as usize]);

        Path::new(&path)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
    }
}

fn foreground_title(pid: u32) -> Option<String> {
    unsafe {
        let window = GetForegroundWindow();

        if window.is_invalid() {
            return None;
        }

        let mut owner = 0u32;

        GetWindowThreadProcessId(window, Some(&mut owner));

        if owner != pid {
            return None;
        }

        window_title(window)
    }
}

fn window_title(window: HWND) -> Option<String> {
    let mut buffer = [0u16; 512];
    let mut length = 0usize;

    let result = unsafe {
        SendMessageTimeoutW(
            window,
            WM_GETTEXT,
            WPARAM(buffer.len()),
            LPARAM(buffer.as_mut_ptr() as isize),
            SMTO_ABORTIFHUNG,
            100,
            Some(&mut length),
        )
    };

    if result.0 == 0 {
        return None;
    }

    let length = length.min(buffer.len());

    if length == 0 {
        return None;
    }

    let title = String::from_utf16_lossy(&buffer[..length]);

    Some(title.trim_end_matches('\0').to_owned())
}

pub struct WindowsInputHandler {
    inner: RdevInputHandler,
}

impl WindowsInputHandler {
    pub fn init() -> Self {
        Self {
            inner: RdevInputHandler::init(),
        }
    }
}

impl InputHandler for WindowsInputHandler {
    fn modifiers(&self) -> Modifiers {
        let mut modifiers = Modifiers::default();

        modifiers.set(Modifier::Alt, key_down(VK_LMENU) || key_down(VK_RMENU));
        modifiers.set(
            Modifier::Control,
            key_down(VK_LCONTROL) || key_down(VK_RCONTROL),
        );
        modifiers.set(Modifier::Shift, key_down(VK_LSHIFT) || key_down(VK_RSHIFT));
        modifiers.set(Modifier::Super, key_down(VK_LWIN) || key_down(VK_RWIN));

        modifiers
    }

    fn listen(&self, sink: InputSink) {
        self.inner.listen(sink);
    }
}

fn key_down(key: VIRTUAL_KEY) -> bool {
    unsafe { GetAsyncKeyState(key.0 as i32) < 0 }
}

pub fn file_icon(path: &Path, size: u32) -> Option<FileIcon> {
    let size = size.max(1);

    let image = shell_image(path, size, SIIGBF_THUMBNAILONLY)
        .or_else(|| shell_image(path, size, SIIGBF_ICONONLY))?;

    Some(icon_canvas(image, size))
}

fn shell_image(path: &Path, size: u32, kind: SIIGBF) -> Option<image::RgbaImage> {
    let mut name: Vec<u16> = path.as_os_str().encode_wide().collect();
    name.push(0);

    unsafe {
        let item: IShellItemImageFactory =
            SHCreateItemFromParsingName(PCWSTR(name.as_ptr()), None).ok()?;
        let flags = SIIGBF(kind.0 | SIIGBF_BIGGERSIZEOK.0);
        let bitmap = item
            .GetImage(
                SIZE {
                    cx: size as i32,
                    cy: size as i32,
                },
                flags,
            )
            .ok()?;
        let image = bitmap_image(&bitmap);

        let _ = DeleteObject(HGDIOBJ(bitmap.0));

        image
    }
}

fn bitmap_image(bitmap: &HBITMAP) -> Option<image::RgbaImage> {
    unsafe {
        let mut basic = BITMAP::default();
        let object = GetObjectW(
            HGDIOBJ(bitmap.0),
            size_of::<BITMAP>() as i32,
            Some(&mut basic as *mut BITMAP as *mut c_void),
        );

        if object == 0 {
            return None;
        }

        let width = basic.bmWidth;
        let height = basic.bmHeight.abs();

        if width <= 0 || height <= 0 {
            return None;
        }

        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..BITMAPINFOHEADER::default()
            },
            ..BITMAPINFO::default()
        };
        let mut pixels = vec![0u8; width as usize * height as usize * 4];
        let dc = CreateCompatibleDC(None);

        if dc.is_invalid() {
            return None;
        }

        let rows = GetDIBits(
            dc,
            *bitmap,
            0,
            height as u32,
            Some(pixels.as_mut_ptr().cast()),
            &mut info,
            DIB_RGB_COLORS,
        );

        let _ = DeleteDC(dc);

        if rows == 0 {
            return None;
        }

        let mut alpha_unset = true;

        for pixel in pixels.chunks_exact_mut(4) {
            pixel.swap(0, 2);
            alpha_unset &= pixel[3] == 0;
        }

        if alpha_unset {
            for pixel in pixels.chunks_exact_mut(4) {
                pixel[3] = 255;
            }
        }

        image::RgbaImage::from_raw(width as u32, height as u32, pixels)
    }
}

fn icon_canvas(source: image::RgbaImage, size: u32) -> FileIcon {
    let scaled =
        image::imageops::resize(&source, size, size, image::imageops::FilterType::Lanczos3);
    let mut canvas = image::RgbaImage::new(size, size);
    let x = ((size - scaled.width()) / 2) as i64;
    let y = ((size - scaled.height()) / 2) as i64;

    image::imageops::overlay(&mut canvas, &scaled, x, y);

    FileIcon {
        width: canvas.width(),
        height: canvas.height(),
        rgba: canvas.into_raw(),
    }
}

fn mouse_button_down() -> bool {
    unsafe {
        GetAsyncKeyState(VK_LBUTTON.0 as i32) < 0 || GetAsyncKeyState(VK_RBUTTON.0 as i32) < 0
    }
}

#[implement(IDropSource)]
struct AtrayDropSource;

impl IDropSource_Impl for AtrayDropSource_Impl {
    fn QueryContinueDrag(&self, fescapepressed: BOOL, grfkeystate: MODIFIERKEYS_FLAGS) -> HRESULT {
        if fescapepressed.as_bool() {
            DRAGDROP_S_CANCEL
        } else if grfkeystate.contains(MK_LBUTTON) || grfkeystate.contains(MK_RBUTTON) {
            S_OK
        } else {
            DRAGDROP_S_DROP
        }
    }

    fn GiveFeedback(&self, _dweffect: DROPEFFECT) -> HRESULT {
        DRAGDROP_S_USEDEFAULTCURSORS
    }
}

#[implement(IDataObject)]
struct AtrayDataObject {
    paths: Vec<PathBuf>,
}

impl IDataObject_Impl for AtrayDataObject_Impl {
    fn GetData(&self, pformatetcin: *const FORMATETC) -> Result<STGMEDIUM> {
        if pformatetcin.is_null() {
            return Err(E_INVALIDARG.into());
        }

        if !hdrop_format_match(unsafe { *pformatetcin }) {
            return Err(DV_E_FORMATETC.into());
        }

        let hglobal = hdrop(&self.paths)?;

        Ok(STGMEDIUM {
            tymed: TYMED_HGLOBAL.0 as u32,
            u: STGMEDIUM_0 { hGlobal: hglobal },
            pUnkForRelease: ManuallyDrop::new(None),
        })
    }

    fn GetDataHere(&self, _pformatetc: *const FORMATETC, _pmedium: *mut STGMEDIUM) -> Result<()> {
        Err(E_NOTIMPL.into())
    }

    fn QueryGetData(&self, pformatetc: *const FORMATETC) -> HRESULT {
        if pformatetc.is_null() {
            return E_INVALIDARG;
        }

        if hdrop_format_match(unsafe { *pformatetc }) {
            S_OK
        } else {
            DV_E_FORMATETC
        }
    }

    fn GetCanonicalFormatEtc(
        &self,
        _pformatectin: *const FORMATETC,
        pformatetcout: *mut FORMATETC,
    ) -> HRESULT {
        if pformatetcout.is_null() {
            return E_INVALIDARG;
        }

        unsafe { (*pformatetcout).ptd = ptr::null_mut() };

        E_NOTIMPL
    }

    fn SetData(
        &self,
        _pformatetc: *const FORMATETC,
        _pmedium: *const STGMEDIUM,
        _frelease: BOOL,
    ) -> Result<()> {
        Err(E_NOTIMPL.into())
    }

    fn EnumFormatEtc(&self, dwdirection: u32) -> Result<IEnumFORMATETC> {
        if dwdirection != DATADIR_GET.0 as u32 {
            return Err(E_NOTIMPL.into());
        }

        Ok(AtrayFormatEnumerator::new().into())
    }

    fn DAdvise(
        &self,
        _pformatetc: *const FORMATETC,
        _advf: u32,
        _padvsink: Ref<IAdviseSink>,
    ) -> Result<u32> {
        Err(OLE_E_ADVISENOTSUPPORTED.into())
    }

    fn DUnadvise(&self, _dwconnection: u32) -> Result<()> {
        Err(OLE_E_NOCONNECTION.into())
    }

    fn EnumDAdvise(&self) -> Result<IEnumSTATDATA> {
        Err(OLE_E_ADVISENOTSUPPORTED.into())
    }
}

#[implement(IEnumFORMATETC)]
struct AtrayFormatEnumerator {
    index: AtomicUsize,
}

impl AtrayFormatEnumerator {
    fn new() -> Self {
        Self {
            index: AtomicUsize::new(0),
        }
    }
}

impl IEnumFORMATETC_Impl for AtrayFormatEnumerator_Impl {
    fn Next(&self, celt: u32, rgelt: *mut FORMATETC, pceltfetched: *mut u32) -> HRESULT {
        if rgelt.is_null() {
            return E_INVALIDARG;
        }

        if celt == 0 || self.index.load(Ordering::Relaxed) > 0 {
            if !pceltfetched.is_null() {
                unsafe { *pceltfetched = 0 };
            }

            return S_FALSE;
        }

        unsafe { *rgelt = hdrop_format() };

        self.index.store(1, Ordering::Relaxed);

        if !pceltfetched.is_null() {
            unsafe { *pceltfetched = 1 };
        }

        if celt == 1 { S_OK } else { S_FALSE }
    }

    fn Skip(&self, celt: u32) -> Result<()> {
        let remaining = 1usize.saturating_sub(self.index.load(Ordering::Relaxed));

        self.index.store(1, Ordering::Relaxed);

        if remaining >= celt as usize {
            Ok(())
        } else {
            Err(S_FALSE.into())
        }
    }

    fn Reset(&self) -> Result<()> {
        self.index.store(0, Ordering::Relaxed);

        Ok(())
    }

    fn Clone(&self) -> Result<IEnumFORMATETC> {
        let clone = AtrayFormatEnumerator {
            index: AtomicUsize::new(self.index.load(Ordering::Relaxed)),
        };

        Ok(clone.into())
    }
}

const fn hdrop_format() -> FORMATETC {
    FORMATETC {
        cfFormat: CF_HDROP.0,
        ptd: ptr::null_mut(),
        dwAspect: DVASPECT_CONTENT.0,
        lindex: -1,
        tymed: TYMED_HGLOBAL.0 as u32,
    }
}

fn hdrop_format_match(format: FORMATETC) -> bool {
    format.cfFormat == CF_HDROP.0
        && format.dwAspect == DVASPECT_CONTENT.0
        && format.tymed & (TYMED_HGLOBAL.0 as u32) != 0
}

fn hdrop(paths: &[PathBuf]) -> Result<HGLOBAL> {
    let mut wide: Vec<u16> = Vec::new();

    for path in paths {
        wide.extend(path.as_os_str().encode_wide());
        wide.push(0);
    }

    if wide.is_empty() {
        return Err(E_INVALIDARG.into());
    }

    wide.push(0);

    let header = size_of::<DROPFILES>();
    let files = header + wide.len() * size_of::<u16>();
    let handle = unsafe { GlobalAlloc(GMEM_MOVEABLE, files)? };
    let buffer = unsafe { GlobalLock(handle) }.cast::<u8>();

    if buffer.is_null() {
        unsafe {
            let _ = GlobalFree(Some(handle));
        }

        return Err(Error::from_thread());
    }

    unsafe {
        let dropfiles = DROPFILES {
            pFiles: header as u32,
            pt: POINT { x: 0, y: 0 },
            fNC: BOOL::from(false),
            fWide: BOOL::from(true),
        };

        ptr::copy_nonoverlapping(
            (&dropfiles as *const DROPFILES).cast::<u8>(),
            buffer,
            header,
        );
        ptr::copy_nonoverlapping(
            wide.as_ptr().cast::<u8>(),
            buffer.add(header),
            wide.len() * size_of::<u16>(),
        );

        let _ = GlobalUnlock(handle);
    }

    Ok(handle)
}
