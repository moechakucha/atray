use std::{
    mem::{ManuallyDrop, size_of},
    os::windows::ffi::OsStrExt,
    path::PathBuf,
    ptr,
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

use windows::{
    Win32::{
        Foundation::{
            DRAGDROP_S_CANCEL, DRAGDROP_S_DROP, DRAGDROP_S_USEDEFAULTCURSORS, DV_E_FORMATETC,
            E_INVALIDARG, E_NOTIMPL, GlobalFree, HGLOBAL, OLE_E_ADVISENOTSUPPORTED,
            OLE_E_NOCONNECTION, POINT, S_FALSE, S_OK,
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
        },
        UI::{
            Input::KeyboardAndMouse::{
                GetAsyncKeyState, VIRTUAL_KEY, VK_LBUTTON, VK_LCONTROL, VK_LMENU, VK_LSHIFT,
                VK_LWIN, VK_RBUTTON, VK_RCONTROL, VK_RMENU, VK_RSHIFT, VK_RWIN,
            },
            Shell::DROPFILES,
            WindowsAndMessaging::FindWindowW,
        },
    },
    core::{BOOL, Error, HRESULT, Ref, Result, implement, w},
};

use crate::input::{InputHandler, InputSink, Modifier, Modifiers};
use crate::platform::{DragEffect, DragHandler, RdevInputHandler};

static DRAGGING: AtomicBool = AtomicBool::new(false);
static LAST_LOGGED: AtomicBool = AtomicBool::new(false);

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

        if LAST_LOGGED.swap(dragging, Ordering::Relaxed) != dragging {
            eprintln!("[atray] dragging -> {dragging}");
        }

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

        eprintln!("[atray] drag out: {hr:?}, effect {}", performed.0);

        true
    }

    fn cancel_pending_drag(&self) {}

    fn take_drag_result(&self) -> Option<bool> {
        self.result.lock().unwrap().take()
    }
}

fn drag_image_present() -> bool {
    unsafe { FindWindowW(w!("SysDragImage"), None).is_ok() }
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
