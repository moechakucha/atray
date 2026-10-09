use std::{
    collections::HashMap,
    ffi::OsStr,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant, UNIX_EPOCH},
};

use x11rb::{
    COPY_DEPTH_FROM_PARENT, COPY_FROM_PARENT, CURRENT_TIME, NONE,
    connection::Connection,
    protocol::{
        Event,
        xproto::{
            Atom, AtomEnum, ClientMessageEvent, ConnectionExt as _, CreateWindowAux, EventMask,
            KeyButMask, Keycode, PropMode, SELECTION_NOTIFY_EVENT, SelectionNotifyEvent,
            SelectionRequestEvent, Timestamp, Window, WindowClass,
        },
    },
    rust_connection::RustConnection,
    wrapper::ConnectionExt as _,
};

use crate::platform::{DragEffect, DragHandler, DragSource, FileIcon};

const APPLICATION_ID: &str = "moe.prwk.atray";
const XDND_VERSION: u8 = 5;
const ESCAPE_KEYSYM: u32 = 0xff1b;
const WATCH_TICK: Duration = Duration::from_millis(40);
const DRAG_TICK: Duration = Duration::from_millis(25);
const TARGETS_TIMEOUT: Duration = Duration::from_millis(250);
const SETTLE_TIMEOUT: Duration = Duration::from_millis(120);
const TRANSFER_TIMEOUT: Duration = Duration::from_secs(10);
const PREVIEW_LIMIT: u64 = 64 * 1024 * 1024;
const THUMBNAIL_DIRS: [&str; 4] = ["normal", "large", "x-large", "xx-large"];

static DRAGGING: AtomicBool = AtomicBool::new(false);
static SOURCE: Mutex<Option<DragSource>> = Mutex::new(None);
static RESULT: Mutex<Option<bool>> = Mutex::new(None);
static WATCHER: OnceLock<()> = OnceLock::new();

pub fn app_init() -> anyhow::Result<()> {
    prefer_x11();

    WATCHER.get_or_init(|| {
        std::thread::spawn(watch);
    });

    Ok(())
}

fn prefer_x11() {
    if std::env::var_os("WAYLAND_DISPLAY").is_none() || std::env::var_os("DISPLAY").is_none() {
        return;
    }

    log::info!("preferring x11 over wayland for this session");

    unsafe { std::env::remove_var("WAYLAND_DISPLAY") };
}

pub fn platform_window_settings() -> iced::window::settings::PlatformSpecific {
    iced::window::settings::PlatformSpecific {
        application_id: APPLICATION_ID.to_string(),
        override_redirect: true,
    }
}

pub fn settings_window_settings() -> iced::window::settings::PlatformSpecific {
    iced::window::settings::PlatformSpecific {
        application_id: APPLICATION_ID.to_string(),
        override_redirect: false,
    }
}

pub fn window_background(_background: iced::Color) -> iced::Color {
    iced::Color::TRANSPARENT
}

pub fn preferred_languages() -> Vec<String> {
    let value = std::env::var("LANGUAGE").unwrap_or_default();

    value
        .split(':')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(|entry| {
            entry
                .split(['.', '@'])
                .next()
                .unwrap_or_default()
                .replace('_', "-")
        })
        .filter(|language| !language.is_empty())
        .collect()
}

pub fn drag_source() -> Option<DragSource> {
    SOURCE.lock().unwrap().clone()
}

pub struct X11DragHandler;

impl DragHandler for X11DragHandler {
    fn new() -> Self {
        Self
    }

    fn is_dragging(&self) -> bool {
        DRAGGING.load(Ordering::Relaxed)
    }

    fn start_drag(&self, paths: &[PathBuf], effect: DragEffect) -> bool {
        if paths.is_empty() {
            return false;
        }

        match Session::start(paths.to_vec(), effect) {
            Ok(()) => true,
            Err(error) => {
                log::error!("failed to start the drag session: {error}");
                false
            }
        }
    }

    fn cancel_pending_drag(&self) {}

    fn take_drag_result(&self) -> Option<bool> {
        RESULT.lock().unwrap().take()
    }
}

struct Atoms {
    xdnd_aware: Atom,
    xdnd_proxy: Atom,
    xdnd_selection: Atom,
    xdnd_enter: Atom,
    xdnd_position: Atom,
    xdnd_status: Atom,
    xdnd_leave: Atom,
    xdnd_drop: Atom,
    xdnd_finished: Atom,
    xdnd_action_copy: Atom,
    xdnd_action_move: Atom,
    uri_list: Atom,
    targets: Atom,
    delete: Atom,
    scratch: Atom,
    net_wm_pid: Atom,
    net_wm_name: Atom,
    wm_name: Atom,
    utf8_string: Atom,
}

impl Atoms {
    fn new(conn: &RustConnection) -> Result<Self, String> {
        Ok(Self {
            xdnd_aware: intern(conn, "XdndAware")?,
            xdnd_proxy: intern(conn, "XdndProxy")?,
            xdnd_selection: intern(conn, "XdndSelection")?,
            xdnd_enter: intern(conn, "XdndEnter")?,
            xdnd_position: intern(conn, "XdndPosition")?,
            xdnd_status: intern(conn, "XdndStatus")?,
            xdnd_leave: intern(conn, "XdndLeave")?,
            xdnd_drop: intern(conn, "XdndDrop")?,
            xdnd_finished: intern(conn, "XdndFinished")?,
            xdnd_action_copy: intern(conn, "XdndActionCopy")?,
            xdnd_action_move: intern(conn, "XdndActionMove")?,
            uri_list: intern(conn, "text/uri-list")?,
            targets: intern(conn, "TARGETS")?,
            delete: intern(conn, "DELETE")?,
            scratch: intern(conn, "ATRAY_SCRATCH")?,
            net_wm_pid: intern(conn, "_NET_WM_PID")?,
            net_wm_name: intern(conn, "_NET_WM_NAME")?,
            wm_name: intern(conn, "WM_NAME")?,
            utf8_string: intern(conn, "UTF8_STRING")?,
        })
    }
}

fn intern(conn: &RustConnection, name: &str) -> Result<Atom, String> {
    conn.intern_atom(false, name.as_bytes())
        .map_err(|error| error.to_string())?
        .reply()
        .map_err(|error| error.to_string())
        .map(|reply| reply.atom)
}

fn watch() {
    let (conn, screen) = match x11rb::connect(None) {
        Ok(connection) => connection,
        Err(error) => {
            log::warn!("x11 drag detection unavailable: {error}");
            return;
        }
    };

    let root = conn.setup().roots[screen].root;

    let atoms = match Atoms::new(&conn) {
        Ok(atoms) => atoms,
        Err(error) => {
            log::warn!("x11 drag detection unavailable: {error}");
            return;
        }
    };

    let window = match conn.generate_id() {
        Ok(window) => window,
        Err(error) => {
            log::warn!("x11 drag detection unavailable: {error}");
            return;
        }
    };

    let aux = CreateWindowAux::new().event_mask(EventMask::PROPERTY_CHANGE);

    if let Err(error) = conn.create_window(
        COPY_DEPTH_FROM_PARENT,
        window,
        root,
        0,
        0,
        1,
        1,
        0,
        WindowClass::INPUT_OUTPUT,
        COPY_FROM_PARENT,
        &aux,
    ) {
        log::warn!("x11 drag detection unavailable: {error}");
        return;
    }

    let _ = conn.flush();

    log::info!("x11 drag detection ready on screen {screen}");

    let mut owner: Window = NONE;
    let mut confirmed = false;
    let mut attempts = 0u32;

    loop {
        std::thread::sleep(WATCH_TICK);

        let pointer = conn
            .query_pointer(root)
            .ok()
            .and_then(|cookie| cookie.reply().ok());
        let pressed = pointer
            .as_ref()
            .is_some_and(|reply| buttons_pressed(reply.mask));
        let current = conn
            .get_selection_owner(atoms.xdnd_selection)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .map(|reply| reply.owner)
            .unwrap_or(NONE);

        if current != owner {
            if current == NONE && DRAGGING.swap(false, Ordering::Relaxed) {
                log::debug!("external drag ended");
            }

            owner = current;
            confirmed = false;
            attempts = 0;
            *SOURCE.lock().unwrap() = None;
        }

        if owner == NONE {
            continue;
        }

        if !confirmed && attempts < 3 {
            attempts += 1;

            let time = server_time(&conn, window, atoms.scratch).unwrap_or(CURRENT_TIME);

            if offers(&conn, window, &atoms, time, TARGETS_TIMEOUT) {
                confirmed = true;

                let child = pointer
                    .as_ref()
                    .map(|reply| reply.child)
                    .filter(|child| *child != NONE);
                let source = source_information(&conn, &atoms, root, owner, child);

                log::info!("file drag detected from {source:?}");

                *SOURCE.lock().unwrap() = source;
            }
        }

        DRAGGING.store(confirmed && pressed, Ordering::Relaxed);
    }
}

fn buttons_pressed(mask: KeyButMask) -> bool {
    u16::from(
        mask & (KeyButMask::BUTTON1
            | KeyButMask::BUTTON2
            | KeyButMask::BUTTON3
            | KeyButMask::BUTTON4
            | KeyButMask::BUTTON5),
    ) != 0
}

fn server_time(conn: &RustConnection, window: Window, scratch: Atom) -> Option<Timestamp> {
    conn.change_property32(PropMode::REPLACE, window, scratch, AtomEnum::CARDINAL, &[0])
        .ok()?;
    conn.flush().ok()?;

    let deadline = Instant::now() + Duration::from_millis(100);

    while Instant::now() < deadline {
        match conn.poll_for_event() {
            Ok(Some(Event::PropertyNotify(event)))
                if event.window == window && event.atom == scratch =>
            {
                return Some(event.time);
            }
            Ok(Some(_)) => continue,
            Ok(None) => std::thread::sleep(Duration::from_millis(2)),
            Err(_) => return None,
        }
    }

    None
}

fn offers(
    conn: &RustConnection,
    window: Window,
    atoms: &Atoms,
    time: Timestamp,
    timeout: Duration,
) -> bool {
    if conn
        .convert_selection(
            window,
            atoms.xdnd_selection,
            atoms.targets,
            atoms.scratch,
            time,
        )
        .is_err()
    {
        return false;
    }

    if conn.flush().is_err() {
        return false;
    }

    let deadline = Instant::now() + timeout;

    while Instant::now() < deadline {
        match conn.poll_for_event() {
            Ok(Some(Event::SelectionNotify(event)))
                if event.requestor == window
                    && event.selection == atoms.xdnd_selection
                    && event.target == atoms.targets =>
            {
                if event.property == NONE {
                    return false;
                }

                let targets = conn
                    .get_property(false, window, event.property, AtomEnum::ATOM, 0, 256)
                    .ok()
                    .and_then(|cookie| cookie.reply().ok());

                let _ = conn.delete_property(window, event.property);

                return targets.is_some_and(|reply| {
                    reply
                        .value32()
                        .is_some_and(|mut values| values.any(|atom| atom == atoms.uri_list))
                });
            }
            Ok(Some(_)) => continue,
            Ok(None) => std::thread::sleep(Duration::from_millis(2)),
            Err(_) => return false,
        }
    }

    false
}

fn source_information(
    conn: &RustConnection,
    atoms: &Atoms,
    root: Window,
    owner: Window,
    child: Option<Window>,
) -> Option<DragSource> {
    let mut pid = None;
    let mut title = None;

    if let Some(child) = child {
        for window in ancestors(conn, child, root) {
            if pid.is_none() {
                pid = window_pid(conn, atoms, window);
            }

            if title.is_none() {
                title = window_title(conn, atoms, window);
            }

            if pid.is_some() && title.is_some() {
                break;
            }
        }
    }

    if pid.is_none() || title.is_none() {
        for window in ancestors(conn, owner, root) {
            if pid.is_none() {
                pid = window_pid(conn, atoms, window);
            }

            if title.is_none() {
                title = window_title(conn, atoms, window);
            }

            if pid.is_some() && title.is_some() {
                break;
            }
        }
    }

    let app_name = pid.and_then(process_name);

    if app_name.is_none() && title.is_none() {
        return None;
    }

    Some(DragSource {
        app_name,
        window_title: title,
    })
}

fn ancestors(conn: &RustConnection, window: Window, root: Window) -> Vec<Window> {
    let mut chain = Vec::new();
    let mut current = window;

    for _ in 0..16 {
        if current == NONE {
            break;
        }

        chain.push(current);

        if current == root {
            break;
        }

        let Ok(cookie) = conn.query_tree(current) else {
            break;
        };

        let Ok(reply) = cookie.reply() else {
            break;
        };

        if reply.parent == NONE || reply.parent == current {
            break;
        }

        current = reply.parent;
    }

    chain
}

fn window_pid(conn: &RustConnection, atoms: &Atoms, window: Window) -> Option<u32> {
    conn.get_property(false, window, atoms.net_wm_pid, AtomEnum::CARDINAL, 0, 1)
        .ok()
        .and_then(|cookie| cookie.reply().ok())
        .and_then(|reply| reply.value32().and_then(|mut values| values.next()))
}

fn window_title(conn: &RustConnection, atoms: &Atoms, window: Window) -> Option<String> {
    let candidates = [
        (atoms.net_wm_name, atoms.utf8_string),
        (atoms.wm_name, u32::from(AtomEnum::STRING)),
    ];

    for (property, kind) in candidates {
        let text = conn
            .get_property(false, window, property, kind, 0, 1024)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .filter(|reply| reply.format == 8 && !reply.value.is_empty())
            .map(|reply| String::from_utf8_lossy(&reply.value).trim().to_string())
            .filter(|text| !text.is_empty());

        if text.is_some() {
            return text;
        }
    }

    None
}

fn process_name(pid: u32) -> Option<String> {
    let base = PathBuf::from(format!("/proc/{pid}"));

    if let Ok(cmdline) = std::fs::read(base.join("cmdline"))
        && let Some(first) = cmdline
            .split(|byte| *byte == 0)
            .next()
            .filter(|first| !first.is_empty())
        && let Some(name) = Path::new(OsStr::from_bytes(first))
            .file_name()
            .and_then(OsStr::to_str)
    {
        return Some(name.to_string());
    }

    if let Ok(exe) = std::fs::read_link(base.join("exe"))
        && let Some(name) = exe.file_name().and_then(OsStr::to_str)
    {
        return Some(name.to_string());
    }

    if let Ok(comm) = std::fs::read_to_string(base.join("comm")) {
        let comm = comm.trim();

        if !comm.is_empty() {
            return Some(comm.to_string());
        }
    }

    None
}

struct Target {
    window: Window,
    dest: Window,
    version: u8,
    accepted: bool,
}

struct Session {
    conn: RustConnection,
    root: Window,
    window: Window,
    atoms: Atoms,
    time: Timestamp,
    payload: Vec<u8>,
    requested: Atom,
    escape: Option<Keycode>,
    target: Option<Target>,
    pointer: Option<(i16, i16)>,
    pending: bool,
    dropped: bool,
    deadline: Instant,
}

impl Session {
    fn start(paths: Vec<PathBuf>, effect: DragEffect) -> Result<(), String> {
        let (conn, screen) = x11rb::connect(None).map_err(|error| error.to_string())?;
        let root = conn.setup().roots[screen].root;
        let atoms = Atoms::new(&conn)?;
        let window = conn.generate_id().map_err(|error| error.to_string())?;

        let aux = CreateWindowAux::new().event_mask(EventMask::PROPERTY_CHANGE);

        conn.create_window(
            COPY_DEPTH_FROM_PARENT,
            window,
            root,
            0,
            0,
            1,
            1,
            0,
            WindowClass::INPUT_OUTPUT,
            COPY_FROM_PARENT,
            &aux,
        )
        .map_err(|error| error.to_string())?;
        conn.flush().map_err(|error| error.to_string())?;

        let time = server_time(&conn, window, atoms.scratch).unwrap_or(CURRENT_TIME);

        conn.set_selection_owner(window, atoms.xdnd_selection, time)
            .map_err(|error| error.to_string())?;
        conn.flush().map_err(|error| error.to_string())?;

        let owner = conn
            .get_selection_owner(atoms.xdnd_selection)
            .map_err(|error| error.to_string())?
            .reply()
            .map_err(|error| error.to_string())?
            .owner;

        if owner != window {
            return Err("another drag session owns the selection".to_string());
        }

        let requested = match effect {
            DragEffect::Copy => atoms.xdnd_action_copy,
            DragEffect::Move => atoms.xdnd_action_move,
        };
        let escape = escape_keycode(&conn);

        let session = Self {
            conn,
            root,
            window,
            atoms,
            time,
            payload: uri_list(&paths),
            requested,
            escape,
            target: None,
            pointer: None,
            pending: false,
            dropped: false,
            deadline: Instant::now() + TRANSFER_TIMEOUT,
        };

        std::thread::spawn(move || session.run());

        Ok(())
    }

    fn run(mut self) {
        let result = self.run_loop();

        log::debug!("drag session finished: success={result}");

        *RESULT.lock().unwrap() = Some(result);
    }

    fn run_loop(&mut self) -> bool {
        let mut finished = None;

        loop {
            self.receive(&mut finished);

            if let Some(result) = finished {
                return result;
            }

            if self.dropped {
                if Instant::now() >= self.deadline {
                    return false;
                }

                std::thread::sleep(DRAG_TICK);
                continue;
            }

            let Some(pointer) = self
                .conn
                .query_pointer(self.root)
                .ok()
                .and_then(|cookie| cookie.reply().ok())
            else {
                return false;
            };

            if self.escape_pressed() {
                self.leave();
                log::debug!("drag session cancelled");

                return false;
            }

            if !buttons_pressed(pointer.mask) {
                self.settle(&mut finished);

                if let Some(result) = finished {
                    return result;
                }

                self.drop();

                if !self.dropped {
                    return false;
                }

                continue;
            }

            let position = (pointer.root_x, pointer.root_y);

            if self.pointer != Some(position) {
                self.pointer = Some(position);
                self.update_target(pointer.child, position);
            }

            let _ = self.conn.flush();
            std::thread::sleep(DRAG_TICK);
        }
    }

    fn receive(&mut self, finished: &mut Option<bool>) {
        while let Ok(Some(event)) = self.conn.poll_for_event() {
            match event {
                Event::ClientMessage(message) => {
                    let data = message.data.as_data32();

                    if message.type_ == self.atoms.xdnd_status {
                        self.status(data);
                    } else if message.type_ == self.atoms.xdnd_finished {
                        self.finished(data, finished);
                    }
                }
                Event::SelectionRequest(request) => self.serve(request),
                _ => {}
            }
        }
    }

    fn status(&mut self, data: [u32; 5]) {
        let Some(target) = &mut self.target else {
            return;
        };

        if data[0] != target.window && data[0] != target.dest {
            return;
        }

        self.pending = false;
        target.accepted = data[1] & 1 != 0;
    }

    fn finished(&mut self, data: [u32; 5], finished: &mut Option<bool>) {
        let Some(target) = &self.target else {
            return;
        };

        if data[0] != target.window && data[0] != target.dest {
            return;
        }

        let accepted = data[1] & 1 != 0;

        *finished = Some(if target.version >= 5 {
            accepted
        } else {
            target.accepted
        });
    }

    fn serve(&mut self, request: SelectionRequestEvent) {
        let mut property = request.property;

        if request.selection == self.atoms.xdnd_selection && request.target == self.atoms.uri_list {
            if request.property == NONE {
                let _ = self.conn.flush();
            } else {
                let stored = self.conn.change_property8(
                    PropMode::REPLACE,
                    request.requestor,
                    request.property,
                    self.atoms.uri_list,
                    &self.payload,
                );

                if stored.is_err() {
                    property = NONE;
                }
            }
        } else if request.selection == self.atoms.xdnd_selection
            && request.target == self.atoms.targets
        {
            if request.property == NONE {
                property = NONE;
            } else {
                let targets = [self.atoms.uri_list, self.atoms.targets];

                let stored = self.conn.change_property32(
                    PropMode::REPLACE,
                    request.requestor,
                    request.property,
                    AtomEnum::ATOM,
                    &targets,
                );

                if stored.is_err() {
                    property = NONE;
                }
            }
        } else if request.selection == self.atoms.xdnd_selection
            && request.target == self.atoms.delete
        {
            property = NONE;
        } else {
            property = NONE;
        }

        let event = SelectionNotifyEvent {
            response_type: SELECTION_NOTIFY_EVENT,
            sequence: 0,
            time: request.time,
            requestor: request.requestor,
            selection: request.selection,
            target: request.target,
            property,
        };

        let _ = self
            .conn
            .send_event(false, request.requestor, EventMask::NO_EVENT, event);
        let _ = self.conn.flush();
    }

    fn settle(&mut self, finished: &mut Option<bool>) {
        let deadline = Instant::now() + SETTLE_TIMEOUT;

        while self.pending && Instant::now() < deadline {
            self.receive(finished);

            if !self.pending || finished.is_some() {
                break;
            }

            std::thread::sleep(Duration::from_millis(4));
        }
    }

    fn escape_pressed(&self) -> bool {
        let Some(escape) = self.escape else {
            return false;
        };

        self.conn
            .query_keymap()
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .is_some_and(|reply| {
                let index = (escape / 8) as usize;
                reply.keys[index] & (1 << (escape % 8)) != 0
            })
    }

    fn update_target(&mut self, child: Window, position: (i16, i16)) {
        let found = self.deepest_window(child).and_then(|window| {
            let (real, dest, version) = self.find_target(window)?;

            Some((real, dest, version))
        });

        let current = self.target.as_ref().map(|target| target.dest);

        match found {
            Some((real, dest, version)) if current != Some(dest) => {
                self.leave();
                self.enter(real, dest, version);
                self.position(position);
            }
            Some(_) => self.position(position),
            None => self.leave(),
        }
    }

    fn deepest_window(&self, child: Window) -> Option<Window> {
        if child == NONE {
            return Some(self.root);
        }

        let mut window = child;

        loop {
            let reply = self
                .conn
                .query_pointer(window)
                .ok()
                .and_then(|cookie| cookie.reply().ok())?;

            if !reply.same_screen || reply.child == NONE {
                return Some(window);
            }

            window = reply.child;
        }
    }

    fn find_target(&self, mut window: Window) -> Option<(Window, Window, u8)> {
        for _ in 0..16 {
            if window == NONE {
                return None;
            }

            let real = window;

            if let Some(proxy) = self.proxy(window)
                && let Some(version) = self.aware_version(proxy)
            {
                return Some((real, proxy, version));
            }

            if let Some(version) = self.aware_version(window) {
                return Some((real, window, version));
            }

            if window == self.root {
                return None;
            }

            let reply = self
                .conn
                .query_tree(window)
                .ok()
                .and_then(|cookie| cookie.reply().ok())?;

            if reply.parent == NONE || reply.parent == window {
                return None;
            }

            window = reply.parent;
        }

        None
    }

    fn proxy(&self, window: Window) -> Option<Window> {
        let proxy = property32(&self.conn, window, self.atoms.xdnd_proxy, AtomEnum::WINDOW)?;

        if property32(&self.conn, proxy, self.atoms.xdnd_proxy, AtomEnum::WINDOW) == Some(proxy) {
            Some(proxy)
        } else {
            None
        }
    }

    fn aware_version(&self, window: Window) -> Option<u8> {
        property32(&self.conn, window, self.atoms.xdnd_aware, AtomEnum::ATOM)
            .map(|version| version.min(u32::from(u8::MAX)) as u8)
    }

    fn enter(&mut self, window: Window, dest: Window, version: u8) {
        let version = version.min(XDND_VERSION);
        let flags = u32::from(version) << 24;
        let data = [self.window, flags, self.atoms.uri_list, NONE, NONE];

        self.send(dest, window, self.atoms.xdnd_enter, data);

        log::debug!("entering xdnd target {dest:#x} (version {version})");

        self.target = Some(Target {
            window,
            dest,
            version,
            accepted: false,
        });
    }

    fn position(&mut self, (x, y): (i16, i16)) {
        let Some(target) = &self.target else {
            return;
        };

        let mut data = [
            self.window,
            0,
            (u32::from(x as u16) << 16) | u32::from(y as u16),
            0,
            0,
        ];

        if target.version >= 1 {
            data[3] = self.time;
        }

        if target.version >= 2 {
            data[4] = self.requested;
        }

        self.send(target.dest, target.window, self.atoms.xdnd_position, data);

        self.pending = true;
    }

    fn leave(&mut self) {
        let Some(target) = self.target.take() else {
            return;
        };

        self.send(
            target.dest,
            target.window,
            self.atoms.xdnd_leave,
            [self.window, 0, 0, 0, 0],
        );
    }

    fn drop(&mut self) {
        let Some(target) = self.target.take() else {
            return;
        };

        if !target.accepted {
            self.send(
                target.dest,
                target.window,
                self.atoms.xdnd_leave,
                [self.window, 0, 0, 0, 0],
            );

            return;
        }

        let mut data = [self.window, 0, 0, 0, 0];

        if target.version >= 1 {
            data[2] = self.time;
        }

        self.send(target.dest, target.window, self.atoms.xdnd_drop, data);

        log::debug!("dropped on xdnd target {:#x}", target.dest);

        self.target = Some(target);
        self.dropped = true;
        self.deadline = Instant::now() + TRANSFER_TIMEOUT;
    }

    fn send(&self, dest: Window, window: Window, kind: Atom, data: [u32; 5]) {
        let event = ClientMessageEvent::new(32, window, kind, data);

        let _ = self
            .conn
            .send_event(false, dest, EventMask::NO_EVENT, event);
    }
}

fn property32(
    conn: &RustConnection,
    window: Window,
    property: Atom,
    kind: AtomEnum,
) -> Option<u32> {
    conn.get_property(false, window, property, kind, 0, 1)
        .ok()
        .and_then(|cookie| cookie.reply().ok())
        .filter(|reply| reply.format == 32 && reply.value_len >= 1)
        .and_then(|reply| reply.value32().and_then(|mut values| values.next()))
}

fn escape_keycode(conn: &RustConnection) -> Option<Keycode> {
    let setup = conn.setup();
    let first = setup.min_keycode;
    let count = setup.max_keycode.saturating_sub(first).saturating_add(1);

    if count == 0 {
        return None;
    }

    let reply = conn.get_keyboard_mapping(first, count).ok()?.reply().ok()?;
    let per_keycode = reply.keysyms_per_keycode as usize;

    if per_keycode == 0 {
        return None;
    }

    reply
        .keysyms
        .iter()
        .position(|keysym| *keysym == ESCAPE_KEYSYM)
        .map(|index| first + (index / per_keycode) as u8)
}

fn uri_list(paths: &[PathBuf]) -> Vec<u8> {
    let mut payload = Vec::new();

    for path in paths {
        let Some(uri) = file_uri(path) else {
            continue;
        };

        payload.extend_from_slice(uri.as_bytes());
        payload.extend_from_slice(b"\r\n");
    }

    payload
}

fn file_uri(path: &Path) -> Option<String> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().ok()?.join(path)
    };

    let mut uri = String::from("file://");

    for byte in absolute.as_os_str().as_bytes() {
        uri.push_str(&escape_byte(*byte));
    }

    Some(uri)
}

fn escape_byte(byte: u8) -> String {
    if byte.is_ascii_alphanumeric()
        || matches!(
            byte,
            b'-' | b'.'
                | b'_'
                | b'~'
                | b'!'
                | b'$'
                | b'&'
                | b'\''
                | b'('
                | b')'
                | b'*'
                | b'+'
                | b','
                | b';'
                | b'='
                | b':'
                | b'@'
                | b'/'
        )
    {
        (byte as char).to_string()
    } else {
        format!("%{byte:02X}")
    }
}

pub fn file_icon(path: &Path, size: u32) -> Option<FileIcon> {
    let size = size.max(1);

    let image = thumbnail(path, size)
        .or_else(|| preview(path))
        .or_else(|| themed_icon(path, size))?;

    Some(canvas(image, size))
}

pub fn file_icon_task(path: PathBuf, size: u32) -> iced::Task<Option<FileIcon>> {
    iced::Task::perform(async move { file_icon(&path, size) }, |icon| icon)
}

fn canvas(source: image::RgbaImage, size: u32) -> FileIcon {
    let scaled = image::DynamicImage::ImageRgba8(source)
        .resize(size, size, image::imageops::FilterType::Lanczos3)
        .to_rgba8();
    let mut canvas = image::RgbaImage::new(size, size);
    let x = ((size - scaled.width()) / 2) as i64;
    let y = ((size - scaled.height()) / 2) as i64;

    image::imageops::overlay(&mut canvas, &scaled, x, y);

    FileIcon {
        width: size,
        height: size,
        rgba: canvas.into_raw(),
    }
}

struct ThumbnailText {
    uri: Option<String>,
    mtime: Option<u64>,
}

fn thumbnail(path: &Path, size: u32) -> Option<image::RgbaImage> {
    let uri = file_uri(path)?;
    let seconds = std::fs::metadata(path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_secs();
    let hash = md5_hex(uri.as_bytes());
    let root = dirs::cache_dir()?.join("thumbnails");
    let start = if size <= 128 {
        0
    } else if size <= 256 {
        1
    } else if size <= 512 {
        2
    } else {
        3
    };

    for dir in &THUMBNAIL_DIRS[start..] {
        let Ok(data) = std::fs::read(root.join(dir).join(format!("{hash}.png"))) else {
            continue;
        };

        let Some(text) = png_text(&data) else {
            continue;
        };

        if text.mtime != Some(seconds) {
            continue;
        }

        if text.uri.as_deref().is_some_and(|stored| stored != uri) {
            continue;
        }

        if let Ok(image) = image::load_from_memory(&data) {
            return Some(image.into_rgba8());
        }
    }

    None
}

fn preview(path: &Path) -> Option<image::RgbaImage> {
    let mime = mime_type(path)?;

    if !mime.starts_with("image/") || mime == "image/svg+xml" {
        return None;
    }

    if std::fs::metadata(path).ok()?.len() > PREVIEW_LIMIT {
        return None;
    }

    image::open(path).ok().map(|image| image.into_rgba8())
}

fn themed_icon(path: &Path, size: u32) -> Option<image::RgbaImage> {
    let names = icon_names(path);
    let roots = icon_roots();
    let themes = theme_chain();

    for name in &names {
        for theme in &themes {
            for root in &roots {
                let base = root.join(theme);

                if !base.is_dir() {
                    continue;
                }

                let entries = theme_entries(&base);

                for entry in order_entries(&entries, size) {
                    let dir = base.join(&entry.path);

                    for extension in ["png", "svg"] {
                        let candidate = dir.join(format!("{name}.{extension}"));

                        if candidate.is_file()
                            && let Some(image) = decode_icon(&candidate, size)
                        {
                            return Some(image);
                        }
                    }
                }
            }
        }
    }

    None
}

fn icon_names(path: &Path) -> Vec<String> {
    if path.is_dir() {
        return vec![
            "folder".to_string(),
            "inode-directory".to_string(),
            "application-x-generic".to_string(),
        ];
    }

    let mut names = Vec::new();

    if let Some(mime) = mime_type(path) {
        for name in mime_icons(&mime) {
            push_unique(&mut names, name);
        }

        push_unique(&mut names, mime.replace('/', "-"));

        if let Some((media, _)) = mime.split_once('/') {
            let generic = match media {
                "text" => "text-x-generic",
                "image" => "image-x-generic",
                "video" => "video-x-generic",
                "audio" => "audio-x-generic",
                "font" => "font-x-generic",
                _ => "application-x-generic",
            };

            push_unique(&mut names, generic.to_string());
        }
    }

    push_unique(&mut names, "application-x-generic".to_string());

    names
}

fn push_unique(names: &mut Vec<String>, name: String) {
    if !name.is_empty() && !names.contains(&name) {
        names.push(name);
    }
}

fn mime_icons(mime: &str) -> Vec<String> {
    let mut names = Vec::new();

    for dir in data_dirs() {
        let Ok(content) = std::fs::read_to_string(dir.join("mime").join(format!("{mime}.xml")))
        else {
            continue;
        };

        for tag in ["<icon name=\"", "<generic-icon name=\""] {
            let mut rest = content.as_str();

            while let Some(index) = rest.find(tag) {
                let after = &rest[index + tag.len()..];

                if let Some(end) = after.find('"') {
                    push_unique(&mut names, after[..end].to_string());
                }

                rest = after;
            }
        }

        break;
    }

    names
}

fn mime_type(path: &Path) -> Option<String> {
    static GLOBS: OnceLock<HashMap<String, String>> = OnceLock::new();

    let globs = GLOBS.get_or_init(load_globs);
    let extension = path.extension()?.to_str()?.to_lowercase();

    globs.get(&extension).cloned()
}

fn load_globs() -> HashMap<String, String> {
    let mut globs: HashMap<String, (u32, String)> = HashMap::new();

    for dir in data_dirs() {
        let mime = dir.join("mime");

        for name in ["globs2", "globs"] {
            let Ok(content) = std::fs::read_to_string(mime.join(name)) else {
                continue;
            };

            for line in content.lines() {
                let line = line.trim();

                if line.is_empty() || line.starts_with('#') {
                    continue;
                }

                let fields: Vec<&str> = line.splitn(4, ':').collect();

                if fields.len() < 2 {
                    continue;
                }

                let (weight, mime, glob, flags) = match fields[0].parse::<u32>() {
                    Ok(weight) => {
                        if fields.len() < 3 {
                            continue;
                        }

                        (
                            weight,
                            fields[1],
                            fields[2],
                            fields.get(3).copied().unwrap_or(""),
                        )
                    }
                    Err(_) => (
                        50,
                        fields[0],
                        fields[1],
                        fields.get(2).copied().unwrap_or(""),
                    ),
                };

                if flags.split(',').any(|flag| flag.trim() == "cs") {
                    continue;
                }

                let Some(suffix) = glob.strip_prefix("*.") else {
                    continue;
                };

                if suffix.is_empty() || suffix.contains(['*', '?', '[', ']', '/']) || suffix == "*"
                {
                    continue;
                }

                let key = suffix.to_lowercase();
                let candidate = (weight, mime.to_string());

                match globs.get(&key) {
                    Some((current, _)) if *current >= weight => {}
                    _ => {
                        globs.insert(key, candidate);
                    }
                }
            }
        }
    }

    globs
        .into_iter()
        .map(|(extension, (_, mime))| (extension, mime))
        .collect()
}

struct ThemeEntry {
    path: PathBuf,
    size: u32,
    scale: u32,
    scalable: bool,
}

impl ThemeEntry {
    fn pixels(&self) -> u32 {
        self.size.saturating_mul(self.scale)
    }
}

fn order_entries(entries: &[ThemeEntry], size: u32) -> Vec<&ThemeEntry> {
    let mut larger: Vec<&ThemeEntry> = entries
        .iter()
        .filter(|entry| !entry.scalable && entry.pixels() >= size)
        .collect();
    larger.sort_by_key(|entry| entry.pixels());

    let scalable: Vec<&ThemeEntry> = entries.iter().filter(|entry| entry.scalable).collect();

    let mut smaller: Vec<&ThemeEntry> = entries
        .iter()
        .filter(|entry| !entry.scalable && entry.pixels() < size)
        .collect();
    smaller.sort_by_key(|entry| std::cmp::Reverse(entry.pixels()));

    larger.into_iter().chain(scalable).chain(smaller).collect()
}

fn theme_entries(base: &Path) -> Arc<Vec<ThemeEntry>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, Arc<Vec<ThemeEntry>>>>> = OnceLock::new();

    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut cache = cache.lock().unwrap();

    if let Some(entries) = cache.get(base) {
        return Arc::clone(entries);
    }

    let entries = Arc::new(load_theme_entries(base));
    cache.insert(base.to_path_buf(), Arc::clone(&entries));

    entries
}

fn load_theme_entries(base: &Path) -> Vec<ThemeEntry> {
    if let Ok(content) = std::fs::read_to_string(base.join("index.theme")) {
        let entries = parse_index_theme(&content);

        if !entries.is_empty() {
            return entries;
        }
    }

    scan_theme(base)
}

fn parse_index_theme(content: &str) -> Vec<ThemeEntry> {
    let sections = parse_ini(content);
    let directories: Vec<String> = sections
        .iter()
        .find(|(name, _)| name == "Icon Theme")
        .and_then(|(_, values)| values.get("Directories"))
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|entry| !entry.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();

    directories
        .into_iter()
        .filter_map(|directory| {
            let (_, values) = sections.iter().find(|(name, _)| name == &directory)?;
            let size = values.get("Size").and_then(|value| value.parse().ok())?;
            let scale = values
                .get("Scale")
                .and_then(|value| value.parse::<u32>().ok())
                .unwrap_or(1)
                .max(1);
            let scalable = values
                .get("Type")
                .is_some_and(|value| value.eq_ignore_ascii_case("scalable"));

            Some(ThemeEntry {
                path: PathBuf::from(directory),
                size,
                scale,
                scalable,
            })
        })
        .collect()
}

fn scan_theme(base: &Path) -> Vec<ThemeEntry> {
    let Ok(listing) = std::fs::read_dir(base) else {
        return Vec::new();
    };

    let mut entries = Vec::new();

    for entry in listing.filter_map(Result::ok) {
        let name = entry.file_name();
        let Some((size, scale, scalable)) = parse_size_dir(&name) else {
            continue;
        };

        let Ok(contexts) = std::fs::read_dir(entry.path()) else {
            continue;
        };

        for context in contexts.filter_map(Result::ok) {
            if !context.path().is_dir() {
                continue;
            }

            entries.push(ThemeEntry {
                path: Path::new(&name).join(context.file_name()),
                size,
                scale,
                scalable,
            });
        }
    }

    entries
}

fn parse_size_dir(name: &OsStr) -> Option<(u32, u32, bool)> {
    let name = name.to_str()?;

    if name == "scalable" {
        return Some((0, 1, true));
    }

    let (size, scale) = match name.split_once('@') {
        Some((size, scale)) => (size, scale.parse::<u32>().ok()?),
        None => (name, 1),
    };

    let size = size.split_once('x')?.0.parse::<u32>().ok()?;

    Some((size, scale.max(1), false))
}

fn parse_ini(content: &str) -> Vec<(String, HashMap<String, String>)> {
    let mut sections: Vec<(String, HashMap<String, String>)> = Vec::new();

    for line in content.lines() {
        let line = line.trim();

        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if let Some(name) = line
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
        {
            sections.push((name.trim().to_string(), HashMap::new()));
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            continue;
        };

        if let Some((_, values)) = sections.last_mut() {
            values.insert(key.trim().to_string(), value.trim().to_string());
        }
    }

    sections
}

fn ini_value(content: &str, section: &str, key: &str) -> Option<String> {
    parse_ini(content)
        .into_iter()
        .find(|(name, _)| name == section)
        .and_then(|(_, values)| values.get(key).cloned())
}

fn theme_chain() -> Vec<String> {
    let mut chain: Vec<String> = Vec::new();
    let mut current = gtk_icon_theme().unwrap_or_else(|| "hicolor".to_string());

    while chain.len() < 8 && !current.is_empty() && !chain.contains(&current) {
        let name = current.clone();
        chain.push(name.clone());

        current = icon_roots()
            .iter()
            .find_map(|root| {
                std::fs::read_to_string(root.join(&name).join("index.theme"))
                    .ok()
                    .and_then(|content| ini_value(&content, "Icon Theme", "Inherits"))
            })
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .find(|entry| !entry.is_empty())
            .unwrap_or_default()
            .to_string();
    }

    push_unique(&mut chain, "hicolor".to_string());

    chain
}

fn gtk_icon_theme() -> Option<String> {
    let config = dirs::config_dir()?;

    for version in ["gtk-3.0", "gtk-4.0"] {
        let Ok(content) = std::fs::read_to_string(config.join(version).join("settings.ini")) else {
            continue;
        };

        if let Some(name) =
            ini_value(&content, "Settings", "gtk-icon-theme-name").filter(|name| !name.is_empty())
        {
            return Some(name);
        }
    }

    None
}

fn icon_roots() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = data_dirs()
        .into_iter()
        .map(|dir| dir.join("icons"))
        .collect();

    if let Some(home) = dirs::home_dir() {
        roots.push(home.join(".icons"));
    }

    roots
}

fn data_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();

    if let Some(home) = dirs::data_dir() {
        dirs.push(home);
    }

    let value = std::env::var("XDG_DATA_DIRS")
        .unwrap_or_else(|_| "/usr/local/share:/usr/share".to_string());

    for entry in value.split(':') {
        if entry.is_empty() {
            continue;
        }

        let path = PathBuf::from(entry);

        if path.is_absolute() && !dirs.contains(&path) {
            dirs.push(path);
        }
    }

    dirs
}

fn decode_icon(path: &Path, size: u32) -> Option<image::RgbaImage> {
    if path
        .extension()
        .and_then(OsStr::to_str)
        .is_some_and(|extension| extension.eq_ignore_ascii_case("svg"))
    {
        return rasterize(&std::fs::read(path).ok()?, size);
    }

    image::open(path).ok().map(|image| image.into_rgba8())
}

fn rasterize(data: &[u8], size: u32) -> Option<image::RgbaImage> {
    let tree = resvg::usvg::Tree::from_data(data, &resvg::usvg::Options::default()).ok()?;
    let source = tree.size();

    if source.width() <= 0.0 || source.height() <= 0.0 {
        return None;
    }

    let target = size as f32;
    let scale = (target / source.width()).min(target / source.height());
    let mut pixmap = resvg::tiny_skia::Pixmap::new(size, size)?;
    let transform = resvg::tiny_skia::Transform::from_scale(scale, scale).post_translate(
        (target - source.width() * scale) / 2.0,
        (target - source.height() * scale) / 2.0,
    );

    resvg::render(&tree, transform, &mut pixmap.as_mut());

    let mut rgba = Vec::with_capacity((size as usize).pow(2) * 4);

    for pixel in pixmap.pixels() {
        let color = pixel.demultiply();
        rgba.extend_from_slice(&[color.red(), color.green(), color.blue(), color.alpha()]);
    }

    image::RgbaImage::from_raw(size, size, rgba)
}

fn png_text(data: &[u8]) -> Option<ThumbnailText> {
    if data.len() < 8 || data[..8] != *b"\x89PNG\r\n\x1a\n" {
        return None;
    }

    let mut text = ThumbnailText {
        uri: None,
        mtime: None,
    };
    let mut cursor = 8usize;

    while cursor + 12 <= data.len() {
        let length = u32::from_be_bytes(data[cursor..cursor + 4].try_into().ok()?) as usize;
        let kind = &data[cursor + 4..cursor + 8];
        let start = cursor + 8;
        let end = start.checked_add(length)?;

        if end + 4 > data.len() {
            break;
        }

        match kind {
            b"tEXt" => {
                let chunk = &data[start..end];

                if let Some(separator) = chunk.iter().position(|byte| *byte == 0) {
                    let keyword = &chunk[..separator];
                    let value = &chunk[separator + 1..];

                    match keyword {
                        b"Thumb::URI" => {
                            text.uri = Some(String::from_utf8_lossy(value).into_owned());
                        }
                        b"Thumb::MTime" => {
                            text.mtime = std::str::from_utf8(value)
                                .ok()
                                .and_then(|value| value.trim().parse().ok());
                        }
                        _ => {}
                    }
                }
            }
            b"IDAT" | b"IEND" => break,
            _ => {}
        }

        cursor = end + 4;
    }

    Some(text)
}

fn md5_hex(data: &[u8]) -> String {
    const SHIFTS: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5,
        9, 14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10,
        15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
    ];

    let mut state: [u32; 4] = [0x6745_2301, 0xefcd_ab89, 0x98ba_dcfe, 0x1032_5476];
    let mut message = data.to_vec();
    let bit_length = (data.len() as u64).wrapping_mul(8);

    message.push(0x80);

    while message.len() % 64 != 56 {
        message.push(0);
    }

    message.extend_from_slice(&bit_length.to_le_bytes());

    for chunk in message.chunks_exact(64) {
        let mut words = [0u32; 16];

        for (index, word) in words.iter_mut().enumerate() {
            *word = u32::from_le_bytes(chunk[index * 4..index * 4 + 4].try_into().unwrap());
        }

        let [mut a, mut b, mut c, mut d] = state;

        for index in 0..64 {
            let (mut value, slot) = match index {
                0..=15 => ((b & c) | (!b & d), index),
                16..=31 => ((d & b) | (!d & c), (5 * index + 1) % 16),
                32..=47 => (b ^ c ^ d, (3 * index + 5) % 16),
                _ => (c ^ (b | !d), (7 * index) % 16),
            };

            value = value
                .wrapping_add(a)
                .wrapping_add(((index as f64 + 1.0).sin().abs() * 4_294_967_296.0) as u32)
                .wrapping_add(words[slot]);

            a = d;
            d = c;
            c = b;
            b = b.wrapping_add(value.rotate_left(SHIFTS[index]));
        }

        state[0] = state[0].wrapping_add(a);
        state[1] = state[1].wrapping_add(b);
        state[2] = state[2].wrapping_add(c);
        state[3] = state[3].wrapping_add(d);
    }

    let mut digest = String::with_capacity(32);

    for word in state {
        for byte in word.to_le_bytes() {
            digest.push_str(&format!("{byte:02x}"));
        }
    }

    digest
}
