use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime},
};

use iced::advanced::image;
use iced::advanced::text::Wrapping;
use iced::widget::{
    column, container, mouse_area, row, scrollable,
    scrollable::{Direction, Scrollbar},
    text,
};
use iced::{
    Alignment, Element, Length, Point, Task, Theme,
    alignment::{Horizontal, Vertical},
    window,
};
use serde::{Deserialize, Serialize};

use crate::autostart;
use crate::config::{self, Side, TRAY_LENGTH, TRAY_THICKNESS, screen};
use crate::i18n;
use crate::input::{self, InputEvent};
use crate::platform::{DragEffect, DragHandler, WindowMaterial};
use crate::theme;
use crate::tray;
use crate::widget::FileChip;

const CACHE_METADATA_SUFFIX: &str = ".atray.toml";
const CACHE_SESSION_FILE: &str = ".session.toml";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum SessionKind {
    Referenced,
    Owned,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SessionEntry {
    kind: SessionKind,
    path: PathBuf,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct CacheSession {
    #[serde(default)]
    files: Vec<SessionEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheMetadata {
    #[serde(default)]
    name: String,
    #[serde(default)]
    original: Option<PathBuf>,
}

impl CacheMetadata {
    fn display_name(&self, path: &Path) -> String {
        if !self.name.is_empty() {
            return self.name.clone();
        }

        self.original
            .as_ref()
            .and_then(|original| original.file_name())
            .and_then(|name| name.to_str())
            .map(str::to_string)
            .unwrap_or_else(|| file_name_of(path))
    }
}

#[derive(Debug)]
pub struct DeferredFile {
    ownership: Ownership,
    pub file_name: String,
    pub file_size: u64,
    pub last_modified: SystemTime,
    pub icon: Option<image::Handle>,
}

#[derive(Debug)]
enum Ownership {
    Referenced(PathBuf),
    Owned(OwnedPath),
}

impl Ownership {
    fn path(&self) -> &PathBuf {
        match self {
            Ownership::Referenced(path) => path,
            Ownership::Owned(owned_path) => owned_path.path(),
        }
    }
}

#[derive(Debug)]
enum OwnedPath {
    Temp(#[allow(dead_code)] tempfile::TempDir, PathBuf),
    Configured(PathBuf),
}

impl OwnedPath {
    pub fn path(&self) -> &PathBuf {
        match self {
            OwnedPath::Temp(_, path) => path,
            OwnedPath::Configured(path) => path,
        }
    }
}

impl DeferredFile {
    pub fn new<P: AsRef<Path>>(
        path: PathBuf,
        cache_path: Option<P>,
        should_move: bool,
    ) -> std::io::Result<Self> {
        let metadata = std::fs::metadata(&path)?;
        let file_name = file_name_of(&path);

        let original = path;

        let ownership = if should_move {
            match cache_path {
                Some(cache_path) => {
                    let cache_dir = cache_path.as_ref();
                    std::fs::create_dir_all(cache_dir)?;

                    let stored = unique_path(cache_dir, &file_name);
                    std::fs::copy(&original, &stored)?;
                    write_metadata(
                        &stored,
                        &CacheMetadata {
                            name: file_name_of(&stored),
                            original: Some(original.clone()),
                        },
                    );

                    Ownership::Owned(OwnedPath::Configured(stored))
                }
                None => {
                    let dir = tempfile::tempdir()?;
                    let stored = dir.path().join(&file_name);
                    std::fs::copy(&original, &stored)?;
                    Ownership::Owned(OwnedPath::Temp(dir, stored))
                }
            }
        } else {
            Ownership::Referenced(original.clone())
        };

        if should_move {
            if let Err(err) = std::fs::remove_file(&original) {
                log::error!("failed to remove original {}: {err}", original.display());
            }
        }

        Ok(Self {
            file_name: file_name_of(ownership.path()),
            ownership,
            file_size: metadata.len(),
            last_modified: metadata.modified()?,
            icon: None,
        })
    }

    fn from_cache(path: PathBuf, metadata: CacheMetadata) -> std::io::Result<Self> {
        let file_metadata = std::fs::metadata(&path)?;

        Ok(Self {
            file_name: metadata.display_name(&path),
            ownership: Ownership::Owned(OwnedPath::Configured(path)),
            file_size: file_metadata.len(),
            last_modified: file_metadata.modified()?,
            icon: None,
        })
    }

    fn discard(&self) {
        if let Ownership::Owned(OwnedPath::Configured(path)) = &self.ownership {
            let _ = std::fs::remove_file(path);
            let _ = std::fs::remove_file(metadata_path(path));
        }
    }

    fn exists(&self) -> bool {
        self.path().exists()
    }

    pub fn path(&self) -> &PathBuf {
        self.ownership.path()
    }
}

fn file_name_of(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(str::to_string)
        .unwrap_or_else(|| i18n::t("file-unknown-name"))
}

fn icon_task(path: PathBuf) -> Task<Message> {
    let size = (theme::ICON_SIZE * 2.0).round() as u32;

    crate::platform::file_icon_task(path.clone(), size)
        .map(move |icon| Message::FileIconLoaded(path.clone(), icon_handle(icon)))
}

fn icon_handle(icon: Option<crate::platform::FileIcon>) -> Option<image::Handle> {
    icon.map(|icon| image::Handle::from_rgba(icon.width, icon.height, icon.rgba))
}

fn metadata_path(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .map(|name| name.to_os_string())
        .unwrap_or_default();
    name.push(CACHE_METADATA_SUFFIX);

    path.with_file_name(name)
}

fn is_metadata_path(path: &Path, entries: &[PathBuf]) -> bool {
    let Some(annotated) = path
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_suffix(CACHE_METADATA_SUFFIX))
    else {
        return false;
    };

    entries
        .iter()
        .any(|entry| entry.as_path() == path.with_file_name(annotated))
}

fn write_metadata(path: &Path, metadata: &CacheMetadata) {
    let content = match toml::to_string_pretty(metadata) {
        Ok(content) => content,
        Err(err) => {
            log::error!(
                "failed to serialize cache metadata for {}: {err}",
                path.display()
            );
            return;
        }
    };

    if let Err(err) = std::fs::write(metadata_path(path), content) {
        log::error!(
            "failed to write cache metadata for {}: {err}",
            path.display()
        );
    }
}

fn read_metadata(path: &Path) -> Option<CacheMetadata> {
    let content = std::fs::read_to_string(metadata_path(path)).ok()?;

    toml::from_str(&content).ok()
}

fn session_path(config_path: &Path) -> PathBuf {
    config_path.with_file_name(CACHE_SESSION_FILE)
}

fn read_session(config_path: &Path) -> Vec<SessionEntry> {
    let Ok(content) = std::fs::read_to_string(session_path(config_path)) else {
        return Vec::new();
    };

    toml::from_str::<CacheSession>(&content)
        .map(|session| session.files)
        .unwrap_or_default()
}

fn write_session(config_path: &Path, files: &[SessionEntry]) {
    let content = match toml::to_string_pretty(&CacheSession {
        files: files.to_vec(),
    }) {
        Ok(content) => content,
        Err(err) => {
            log::error!("failed to serialize session: {err}");
            return;
        }
    };

    if let Err(err) = std::fs::write(session_path(config_path), content) {
        log::error!("failed to write session: {err}");
    }
}

fn load_session(config_path: &Path, cache_dir: &Path) -> Vec<DeferredFile> {
    let mut files: Vec<DeferredFile> = Vec::new();
    let mut seen: HashSet<PathBuf> = HashSet::new();

    for entry in read_session(config_path) {
        let file = match entry.kind {
            SessionKind::Owned => read_metadata(&entry.path)
                .and_then(|metadata| DeferredFile::from_cache(entry.path.clone(), metadata).ok()),
            SessionKind::Referenced => {
                DeferredFile::new(entry.path.clone(), None::<&Path>, false).ok()
            }
        };

        if let Some(file) = file {
            seen.insert(entry.path);
            files.push(file);
        }
    }

    for file in load_cache(cache_dir) {
        if seen.insert(file.path().clone()) {
            files.push(file);
        }
    }

    files
}

fn unique_path(dir: &Path, file_name: &str) -> PathBuf {
    let mut candidate = dir.join(file_name);

    if !candidate.exists() {
        return candidate;
    }

    let path = Path::new(file_name);
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(file_name);
    let extension = path.extension().and_then(|extension| extension.to_str());
    let mut index = 2;

    while candidate.exists() {
        let name = match extension {
            Some(extension) => format!("{stem} {index}.{extension}"),
            None => format!("{stem} {index}"),
        };

        candidate = dir.join(name);
        index += 1;
    }

    candidate
}

fn load_cache(cache_dir: &Path) -> Vec<DeferredFile> {
    let Ok(entries) = std::fs::read_dir(cache_dir) else {
        return Vec::new();
    };

    let paths: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .collect();

    let mut files: Vec<DeferredFile> = paths
        .iter()
        .filter(|path| !is_metadata_path(path, &paths))
        .filter_map(|path| {
            let metadata = read_metadata(path)?;

            DeferredFile::from_cache(path.clone(), metadata).ok()
        })
        .collect();

    files.sort_by(|left, right| left.file_name.cmp(&right.file_name));

    files
}

#[derive(Clone)]
pub enum Message {
    PointerPressed,
    PointerReleased,
    PollDragState,
    FileHovered,
    FileHoveredLeft,
    FileDropped(PathBuf),
    FileSelectionToggled(usize),
    FileTakenOut(usize),
    FileIconLoaded(PathBuf, Option<image::Handle>),
    PollDragResult,
    TrayMenuClicked(String),
    SystemThemeChanged(iced::theme::Mode),
    RelayOpened(window::Id),
    RelayPositioned(Option<iced::Point>),
    RelayScrolled(f32),
    ChipHovered {
        index: usize,
        position: Point,
        truncated: bool,
    },
    ChipHoverLeft(usize),
    TrayEntered,
    TrayExited,
    TooltipTick,
    SlideTick(Instant),
    Screen(screen::Message),
    WindowCloseRequested(window::Id),
    WindowClosed(window::Id),
    Placeholder,
}

const SLIDE_DURATION: Duration = Duration::from_millis(220);
const TRAY_INSET: f32 = 4.0;
const TRAY_PEEK_WIDTH: f32 = 16.0;
const TOOLTIP_WIDTH: f32 = 320.0;
const TOOLTIP_HEIGHT: f32 = 240.0;
const TOOLTIP_GAP: f32 = 4.0;
const TOOLTIP_DELAY: Duration = Duration::from_millis(350);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TrayState {
    Hidden,
    Peek,
    Open,
}

#[derive(Clone, Copy)]
struct Slide {
    started: Instant,
    from: Point,
    to: Point,
    hide: bool,
}

#[derive(Clone, Copy)]
struct TooltipHover {
    index: usize,
    position: Point,
    since: Instant,
}

pub struct App {
    config: config::Config,
    config_dirty: bool,
    file_relay: Vec<DeferredFile>,
    pending_icons: HashSet<PathBuf>,
    window_id: Option<window::Id>,
    drag_handler: Box<dyn DragHandler>,
    dragging: Option<Vec<usize>>,
    lingering: Vec<DeferredFile>,
    selected: HashSet<usize>,
    hovered: bool,
    theme: Option<Theme>,
    metrics: theme::Metrics,
    system_mode: iced::theme::Mode,
    slide: Option<Slide>,
    anchor: Option<Point>,
    position: Point,
    tray_hovered: bool,
    scroll_offset: f32,
    tooltip_hover: Option<TooltipHover>,
    tooltip_window: Option<window::Id>,
    config_window: Option<window::Id>,
    screen: Option<screen::Screen>,
}

impl App {
    pub fn new(config: config::Config) -> (Self, Task<Message>) {
        let system_mode = detected_system_mode();
        let (theme, metrics) = theme::resolve(config.appearance.theme, system_mode);

        let file_relay = load_session(config.path(), Path::new(&config.advanced.cache_dir));

        let mut app = Self {
            config,
            config_dirty: false,
            file_relay,
            pending_icons: HashSet::new(),
            window_id: None,
            drag_handler: crate::platform::get_drag_handler(),
            dragging: None,
            lingering: Vec::new(),
            selected: HashSet::new(),
            hovered: false,
            theme,
            metrics,
            system_mode,
            slide: None,
            anchor: None,
            position: Point::ORIGIN,
            tray_hovered: false,
            scroll_offset: 0.0,
            tooltip_hover: None,
            tooltip_window: None,
            config_window: None,
            screen: None,
        };

        app.read_autostart();

        log::info!("session: restored {} file(s)", app.file_relay.len());

        let icons = app.load_icons();

        let task = if app.file_relay.is_empty() {
            icons
        } else {
            Task::batch([icons, app.open_tray()])
        };

        (app, task)
    }

    pub fn theme(&self, _id: window::Id) -> Option<Theme> {
        self.theme.clone()
    }

    pub fn title(&self, id: window::Id) -> String {
        if self.config_window == Some(id) {
            i18n::t("settings-window-title")
        } else {
            String::new()
        }
    }

    fn relocalize(&self) {
        i18n::init(self.config.appearance.language.as_deref());
        tray::relocalize();
    }

    fn refresh_theme(&mut self) {
        let (theme, metrics) = theme::resolve(self.config.appearance.theme, self.system_mode);
        self.theme = theme;
        self.metrics = metrics;
    }

    fn material(&self, material: WindowMaterial) -> Task<Message> {
        let (window, radius) = match material {
            WindowMaterial::Tray => (self.window_id, Some(self.metrics.large_radius)),
            WindowMaterial::Settings => (self.config_window, None),
        };

        let Some(window) = window else {
            return Task::none();
        };

        let dark = self
            .theme
            .as_ref()
            .is_some_and(|theme| theme.extended_palette().is_dark);

        crate::platform::apply_window_material(window, material, radius, dark)
            .map(|()| Message::Placeholder)
    }

    fn materials(&self) -> Task<Message> {
        Task::batch([
            self.material(WindowMaterial::Tray),
            self.material(WindowMaterial::Settings),
        ])
    }

    fn load_icons(&mut self) -> Task<Message> {
        let paths: Vec<PathBuf> = self
            .file_relay
            .iter()
            .filter(|file| file.icon.is_none())
            .map(|file| file.path().clone())
            .collect();

        let tasks: Vec<Task<Message>> = paths
            .into_iter()
            .filter(|path| self.pending_icons.insert(path.clone()))
            .map(icon_task)
            .collect();

        if tasks.is_empty() {
            return Task::none();
        }

        Task::batch(tasks)
    }

    pub fn add_from_location<P: AsRef<Path>>(
        &mut self,
        path: PathBuf,
        cache_path: Option<P>,
        should_move: bool,
    ) -> std::io::Result<()> {
        let file = DeferredFile::new(path, cache_path.as_ref(), should_move)?;

        log::info!(
            "added {} ({} bytes), {} in the tray",
            file.path().display(),
            file.file_size,
            self.file_relay.len() + 1
        );

        self.file_relay.push(file);
        self.save_session();
        Ok(())
    }

    fn save_session(&self) {
        let files: Vec<SessionEntry> = self
            .file_relay
            .iter()
            .filter_map(|file| {
                let entry = match &file.ownership {
                    Ownership::Referenced(path) => Some(SessionEntry {
                        kind: SessionKind::Referenced,
                        path: path.clone(),
                    }),
                    Ownership::Owned(OwnedPath::Configured(path)) => Some(SessionEntry {
                        kind: SessionKind::Owned,
                        path: path.clone(),
                    }),
                    Ownership::Owned(OwnedPath::Temp(..)) => None,
                }?;

                if entry.path.to_str().is_none() {
                    log::warn!(
                        "session: skipping {} because its path is not valid UTF-8",
                        entry.path.display()
                    );
                    return None;
                }

                Some(entry)
            })
            .collect();

        log::debug!("session: saving {} file(s)", files.len());

        write_session(self.config.path(), &files);
    }

    fn should_move(&self) -> bool {
        input::modifiers().contains(self.config.behavior.move_modifier)
            ^ self.config.behavior.invert_copy_and_move
    }

    pub fn take_out(&mut self, index: usize) -> bool {
        if index >= self.file_relay.len() {
            return false;
        }

        let mut indices: Vec<usize> = if self.selected.contains(&index) {
            self.selected
                .iter()
                .copied()
                .filter(|i| *i < self.file_relay.len())
                .collect()
        } else {
            vec![index]
        };

        indices.sort_unstable();
        indices.dedup();

        self.prune_missing(&mut indices);

        if indices.is_empty() {
            return false;
        }

        let paths: Vec<PathBuf> = indices
            .iter()
            .map(|&i| self.file_relay[i].path().clone())
            .collect();

        let effect = if self.should_move() {
            DragEffect::Move
        } else {
            DragEffect::Copy
        };

        if !self.drag_handler.start_drag(&paths, effect) {
            log::warn!("failed to start the drag session");
            return false;
        }

        log::info!("dragging out {} file(s) ({effect:?})", paths.len());

        self.discard_lingering();
        self.dragging = Some(indices);

        true
    }

    fn prune_missing(&mut self, indices: &mut Vec<usize>) {
        let len = self.file_relay.len();
        let missing: Vec<usize> = (0..len)
            .filter(|&index| !self.file_relay[index].exists())
            .collect();

        if missing.is_empty() {
            return;
        }

        let shift =
            |index: usize| index - missing.iter().filter(|&&removed| removed < index).count();

        for &index in missing.iter().rev() {
            log::info!(
                "dropping missing file: {}",
                self.file_relay[index].path().display()
            );
            self.file_relay.remove(index).discard();
        }

        indices.retain(|index| !missing.contains(index));
        *indices = indices.iter().map(|&index| shift(index)).collect();

        self.selected = self
            .selected
            .iter()
            .filter(|index| **index < len && !missing.contains(index))
            .map(|&index| shift(index))
            .collect();

        self.save_session();
    }

    fn finish_drag(&mut self, success: bool) {
        let Some(indices) = self.dragging.take() else {
            return;
        };

        log::info!("drag ended: success={success}, {} item(s)", indices.len());

        if !success {
            return;
        }

        for &index in indices.iter().rev() {
            if index < self.file_relay.len() {
                self.lingering.push(self.file_relay.remove(index));
            }
        }

        self.selected.clear();
        self.save_session();
    }

    fn open_tray(&mut self) -> Task<Message> {
        if self.window_id.is_some() {
            return Task::none();
        }

        self.refresh_theme();

        let (id, task) = window::open(self.config.appearance.into_settings());
        self.window_id = Some(id);
        self.anchor = None;
        self.position = Point::ORIGIN;
        self.scroll_offset = 0.0;

        log::info!("tray window opened: {id:?}");

        task.discard()
    }

    fn discard_lingering(&mut self) {
        for file in self.lingering.drain(..) {
            file.discard();
        }
    }

    fn restart_tray(&mut self) -> Task<Message> {
        log::debug!("restarting the tray window");

        let tooltip = self.close_tooltip();
        self.tooltip_hover = None;

        let close = match self.window_id.take() {
            Some(id) => window::close(id),
            None => Task::none(),
        };

        self.anchor = None;
        self.position = Point::ORIGIN;
        self.slide = None;
        self.scroll_offset = 0.0;
        self.tray_hovered = false;

        let open = if self.dragging.is_some()
            || self.drag_handler.is_dragging()
            || !self.file_relay.is_empty()
        {
            self.open_tray()
        } else {
            Task::none()
        };

        Task::batch([tooltip, close, open])
    }

    fn try_open_relay(&mut self) -> Task<Message> {
        if !self.drag_handler.is_dragging() {
            return Task::none();
        }

        if !self.source_allowed() {
            log::debug!("ignoring drag: source does not match the filter");
            return Task::none();
        }

        log::debug!("drag detected from {:?}", crate::platform::drag_source());

        self.open_tray()
    }

    fn source_allowed(&self) -> bool {
        self.config
            .behavior
            .allows(crate::platform::drag_source().as_ref())
    }

    fn tray_state(&self) -> TrayState {
        if self.dragging.is_some() || self.tray_hovered {
            TrayState::Open
        } else if self.drag_handler.is_dragging() && self.source_allowed() {
            TrayState::Open
        } else if self.file_relay.is_empty() {
            TrayState::Hidden
        } else {
            TrayState::Peek
        }
    }

    fn tray_point(&self, state: TrayState) -> Option<Point> {
        let anchor = self.anchor?;

        let point = match self.config.appearance.side {
            Side::Left => Point::new(
                match state {
                    TrayState::Hidden => -TRAY_THICKNESS,
                    TrayState::Peek => TRAY_PEEK_WIDTH - TRAY_THICKNESS,
                    TrayState::Open => TRAY_INSET,
                },
                anchor.y,
            ),
            Side::Right => Point::new(
                match state {
                    TrayState::Hidden => anchor.x,
                    TrayState::Peek => anchor.x - TRAY_PEEK_WIDTH,
                    TrayState::Open => anchor.x - TRAY_THICKNESS - TRAY_INSET,
                },
                anchor.y,
            ),
            Side::Top => Point::new(
                anchor.x,
                match state {
                    TrayState::Hidden => -TRAY_THICKNESS,
                    TrayState::Peek => TRAY_PEEK_WIDTH - TRAY_THICKNESS,
                    TrayState::Open => TRAY_INSET,
                },
            ),
            Side::Bottom => Point::new(
                anchor.x,
                match state {
                    TrayState::Hidden => anchor.y,
                    TrayState::Peek => anchor.y - TRAY_PEEK_WIDTH,
                    TrayState::Open => anchor.y - TRAY_THICKNESS - TRAY_INSET,
                },
            ),
        };

        Some(point)
    }

    fn sync_tray(&mut self) -> Task<Message> {
        if self.window_id.is_none() {
            return Task::none();
        }

        let state = self.tray_state();

        log::debug!("tray state: {state:?}");

        let Some(target) = self.tray_point(state) else {
            return Task::none();
        };

        if self.slide.is_some_and(|slide| slide.to == target) {
            return Task::none();
        }

        self.slide = Some(Slide {
            started: Instant::now(),
            from: self.position,
            to: target,
            hide: state == TrayState::Hidden,
        });

        Task::none()
    }

    fn tooltip_origin(&self, chip: Point) -> Option<Point> {
        let open = self.tray_point(TrayState::Open)?;

        let origin = if self.config.appearance.side.horizontal() {
            let x = open.x + chip.x - self.scroll_offset;
            let x = x.clamp(open.x, open.x + (TRAY_LENGTH - TOOLTIP_WIDTH).max(0.0));
            let y = if self.config.appearance.side == Side::Bottom {
                open.y - TOOLTIP_HEIGHT - TOOLTIP_GAP
            } else {
                open.y + TRAY_THICKNESS + TOOLTIP_GAP
            };

            Point::new(x, y)
        } else {
            let y = open.y + chip.y - self.scroll_offset;
            let y = y.clamp(open.y, open.y + (TRAY_LENGTH - TOOLTIP_HEIGHT).max(0.0));
            let x = if self.config.appearance.side == Side::Right {
                open.x - TOOLTIP_WIDTH - TOOLTIP_GAP
            } else {
                open.x + TRAY_THICKNESS + TOOLTIP_GAP
            };

            Point::new(x, y)
        };

        Some(origin)
    }

    fn open_tooltip(&mut self, chip: Point) -> Task<Message> {
        let Some(position) = self.tooltip_origin(chip) else {
            return Task::none();
        };

        let settings = window::Settings {
            size: (TOOLTIP_WIDTH, TOOLTIP_HEIGHT).into(),
            position: window::Position::Specific(position),
            decorations: false,
            closeable: false,
            resizable: false,
            transparent: true,
            minimizable: false,
            level: window::Level::AlwaysOnTop,
            exit_on_close_request: false,
            platform_specific: crate::platform::platform_window_settings(),
            ..Default::default()
        };

        let (id, task) = window::open(settings);
        self.tooltip_window = Some(id);

        task.discard()
    }

    fn move_tooltip(&mut self, chip: Point) -> Task<Message> {
        let Some(id) = self.tooltip_window else {
            return Task::none();
        };

        match self.tooltip_origin(chip) {
            Some(position) => window::move_to(id, position),
            None => Task::none(),
        }
    }

    fn close_tooltip(&mut self) -> Task<Message> {
        match self.tooltip_window.take() {
            Some(id) => window::close(id),
            None => Task::none(),
        }
    }

    fn open_config(&mut self) -> Task<Message> {
        if let Some(id) = self.config_window {
            return window::gain_focus::<Message>(id).discard();
        }

        self.screen = Some(screen::Screen::from_config(&self.config));

        let (id, task) = window::open(screen::window_settings());
        self.config_window = Some(id.clone());

        log::info!("settings window opened: {id:?}");

        task.chain(window::gain_focus(id)).discard()
    }

    fn screen_message(&mut self, message: screen::Message) -> Task<Message> {
        let previous_side = self.config.appearance.side;
        let previous_theme = self.config.appearance.theme;
        let previous_language = self.config.appearance.language.clone();
        let previous_autostart = self.config.advanced.launch_at_login;

        let changed = match &mut self.screen {
            Some(screen) => screen.update(message, &mut self.config),
            None => return Task::none(),
        };

        if !changed {
            return Task::none();
        }

        self.config_dirty = true;

        let mut tasks = Vec::new();

        if self.config.appearance.theme != previous_theme {
            self.refresh_theme();
            tasks.push(self.materials());
        }

        if self.config.appearance.language != previous_language {
            self.relocalize();
        }

        if self.config.advanced.launch_at_login != previous_autostart
            && let Some(error) = self.apply_autostart()
            && let Some(screen) = &mut self.screen
        {
            screen.set_error(error);
        }

        if self.config.appearance.side != previous_side {
            tasks.push(self.restart_tray());
        }

        Task::batch(tasks)
    }

    fn save_config(&mut self) {
        if !self.config_dirty {
            return;
        }

        match self.config.save_to_original() {
            Ok(()) => {
                self.config_dirty = false;
                log::debug!("config saved to {}", self.config.path().display());
            }
            Err(error) => log::error!("failed to save config: {error}"),
        }
    }

    fn read_autostart(&mut self) {
        let Ok(enabled) = autostart::is_enabled() else {
            return;
        };

        if enabled == self.config.advanced.launch_at_login {
            return;
        }

        self.config.advanced.launch_at_login = enabled;
        self.config_dirty = true;

        log::info!("launch at login: {enabled}");
    }

    fn apply_autostart(&self) -> Option<String> {
        autostart::set(self.config.advanced.launch_at_login)
            .err()
            .map(|err| {
                i18n::t_args(
                    "error-launch-at-login",
                    &[("error", err.to_string().into())],
                )
            })
    }

    pub fn view(&self, id: window::Id) -> Element<'_, Message> {
        if self.config_window == Some(id) {
            return self.screen_view();
        }

        if self.tooltip_window == Some(id) {
            return self.tooltip_view();
        }

        self.relay_view()
    }

    fn screen_view(&self) -> Element<'_, Message> {
        let theme = self.theme.clone().unwrap_or(Theme::Light);
        let colors = theme::Colors::of(&theme);

        match &self.screen {
            Some(screen) => screen
                .view(&self.config, colors, self.metrics)
                .map(Message::Screen),
            None => container(text("")).into(),
        }
    }

    fn tooltip_view(&self) -> Element<'_, Message> {
        let name = self
            .tooltip_hover
            .and_then(|hover| self.file_relay.get(hover.index))
            .map(|file| file.file_name.as_str())
            .unwrap_or_default();

        let card = container(
            text(name)
                .size(self.metrics.name_size)
                .wrapping(Wrapping::WordOrGlyph),
        )
        .style(move |theme| theme::tooltip(theme, self.metrics))
        .padding(theme::CARD_PADDING)
        .width(Length::Shrink)
        .height(Length::Shrink);

        let (align_x, align_y) = match self.config.appearance.side {
            Side::Right => (Horizontal::Right, Vertical::Top),
            Side::Bottom => (Horizontal::Left, Vertical::Bottom),
            Side::Left | Side::Top => (Horizontal::Left, Vertical::Top),
        };

        container(card)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(align_x)
            .align_y(align_y)
            .into()
    }

    fn relay_view(&self) -> Element<'_, Message> {
        let files: Vec<Element<Message>> = self
            .file_relay
            .iter()
            .enumerate()
            .map(|(index, file)| {
                FileChip::new(index, file, self.selected.contains(&index), self.metrics).into()
            })
            .collect();

        let horizontal = self.config.appearance.side.horizontal();

        let content: Element<'_, Message> = if horizontal {
            row(files)
                .spacing(theme::SPACING)
                .align_y(Alignment::Center)
                .height(Length::Fill)
                .into()
        } else {
            column(files)
                .spacing(theme::SPACING)
                .align_x(Alignment::Center)
                .width(Length::Fill)
                .into()
        };

        let scrollable = if horizontal {
            scrollable(content).direction(Direction::Horizontal(Scrollbar::default()))
        } else {
            scrollable(content)
        };

        mouse_area(
            container(
                scrollable
                    .height(Length::Fill)
                    .width(Length::Fill)
                    .on_scroll(move |viewport| {
                        let offset = viewport.absolute_offset();
                        Message::RelayScrolled(if horizontal { offset.x } else { offset.y })
                    }),
            )
            .style(move |theme| theme::surface(theme, self.metrics))
            .padding(theme::VIEW_PADDING)
            .width(Length::Fill)
            .height(Length::Fill),
        )
        .on_enter(Message::TrayEntered)
        .on_exit(Message::TrayExited)
        .into()
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        if let Some(success) = self.drag_handler.take_drag_result() {
            self.finish_drag(success);
        }

        match message {
            Message::PointerPressed => {
                log::trace!(
                    "pointer pressed (window={}, dragging={}, hovered={})",
                    self.window_id.is_some(),
                    self.dragging.is_some(),
                    self.hovered
                );

                if self.dragging.is_some() {
                    self.finish_drag(false);
                    self.drag_handler.cancel_pending_drag();
                }

                self.tooltip_hover = None;
                self.hovered = false;
                let tooltip = self.close_tooltip();

                Task::batch([tooltip, self.try_open_relay()])
            }
            Message::PointerReleased => {
                log::trace!("pointer released");

                self.drag_handler.cancel_pending_drag();
                Task::none()
            }
            Message::PollDragState => {
                if self.window_id.is_none() {
                    return self.try_open_relay();
                }

                self.sync_tray()
            }
            Message::FileTakenOut(idx) => {
                self.take_out(idx);
                Task::none()
            }
            Message::FileSelectionToggled(index) => {
                if index < self.file_relay.len() && !self.selected.remove(&index) {
                    self.selected.insert(index);
                }

                Task::none()
            }
            Message::FileIconLoaded(path, icon) => {
                self.pending_icons.remove(&path);

                log::debug!("icon loaded for {}: {}", path.display(), icon.is_some());

                for file in &mut self.file_relay {
                    if file.path() == &path {
                        file.icon = icon.clone();
                    }
                }

                Task::none()
            }
            Message::PollDragResult => Task::none(),
            Message::FileHovered => {
                log::debug!("files hovered over the tray");

                self.hovered = true;
                Task::none()
            }
            Message::FileHoveredLeft => {
                log::debug!("files left the tray");

                self.hovered = false;
                Task::none()
            }
            Message::FileDropped(path) => {
                self.hovered = false;

                if let Some(hover) = &mut self.tooltip_hover {
                    hover.since = Instant::now();
                }

                if let Some(index) = self.lingering.iter().position(|file| file.path() == &path) {
                    log::info!("{} was dropped back onto the tray", path.display());

                    self.file_relay.push(self.lingering.remove(index));
                    self.save_session();
                    return Task::none();
                }

                let is_our_file = self.dragging.as_ref().is_some_and(|indices| {
                    indices.iter().any(|&index| {
                        self.file_relay
                            .get(index)
                            .is_some_and(|file| file.path() == &path)
                    })
                });

                if is_our_file {
                    self.finish_drag(false);
                    self.drag_handler.cancel_pending_drag();
                    return Task::none();
                }

                if !self.source_allowed() {
                    log::info!(
                        "ignoring {}: source does not match the filter",
                        path.display()
                    );
                    return Task::none();
                }

                let should_move = self.should_move();

                let cache_dir = self.config.advanced.cache_dir.clone();
                let cache_path = if cache_dir.is_empty() {
                    None
                } else {
                    Some(PathBuf::from(cache_dir))
                };

                match self.add_from_location(path.clone(), cache_path, should_move) {
                    Ok(()) => self.load_icons(),
                    Err(err) => {
                        log::error!("failed to add {}: {err}", path.display());
                        Task::none()
                    }
                }
            }
            Message::TrayMenuClicked(id) => {
                log::info!("tray menu: {id}");

                return match id.as_str() {
                    "quit" => {
                        self.save_config();
                        self.discard_lingering();
                        iced::exit()
                    }
                    "config_file" => {
                        open::that(&self.config.path()).ok();
                        Task::none()
                    }
                    "reload_config_file" => {
                        let previous_side = self.config.appearance.side;
                        let previous_theme = self.config.appearance.theme;
                        let previous_language = self.config.appearance.language.clone();
                        self.config = config::Config::load(self.config.path()).unwrap_or_default();
                        self.config_dirty = false;

                        if self.config_window.is_some() {
                            self.screen = Some(screen::Screen::from_config(&self.config));
                        }

                        let mut tasks = Vec::new();

                        if self.config.appearance.theme != previous_theme {
                            self.refresh_theme();
                            tasks.push(self.materials());
                        }

                        if self.config.appearance.language != previous_language {
                            self.relocalize();
                        }

                        if let Some(error) = self.apply_autostart() {
                            log::error!("{error}");
                        }

                        if self.config.appearance.side != previous_side {
                            tasks.push(self.restart_tray());
                        }

                        Task::batch(tasks)
                    }
                    "settings" => self.open_config(),
                    _ => Task::none(),
                };
            }
            Message::SystemThemeChanged(mode) => {
                log::info!("system theme changed: {mode:?}");

                self.system_mode = mode;
                self.refresh_theme();

                self.materials()
            }
            Message::RelayOpened(id) => {
                if self.tooltip_window == Some(id) {
                    return window::enable_mouse_passthrough(id);
                }

                if self.config_window == Some(id) {
                    return self.material(WindowMaterial::Settings);
                }

                if self.window_id == Some(id) {
                    self.refresh_theme();
                    return Task::batch([
                        self.material(WindowMaterial::Tray),
                        window::position(id).map(Message::RelayPositioned),
                    ]);
                }

                Task::none()
            }
            Message::RelayPositioned(position) => {
                let Some(position) = position else {
                    if let Some(id) = self.window_id.take() {
                        return window::close(id);
                    }

                    return Task::none();
                };

                if self.window_id.is_some() {
                    self.anchor = Some(position);
                    self.position = position;
                    return self.sync_tray();
                }

                Task::none()
            }
            Message::SlideTick(now) => {
                let Some(slide) = self.slide else {
                    return Task::none();
                };

                let Some(id) = self.window_id else {
                    self.slide = None;
                    return Task::none();
                };

                let elapsed = now.saturating_duration_since(slide.started).as_secs_f32();
                let progress = (elapsed / SLIDE_DURATION.as_secs_f32()).clamp(0.0, 1.0);
                let eased = ease_out_cubic(progress);
                let point = Point::new(
                    slide.from.x + (slide.to.x - slide.from.x) * eased,
                    slide.from.y + (slide.to.y - slide.from.y) * eased,
                );

                self.position = point;

                if progress < 1.0 {
                    return window::move_to(id, point);
                }

                self.slide = None;

                if slide.hide {
                    self.window_id = None;
                    self.tooltip_hover = None;
                    let tooltip = self.close_tooltip();

                    return Task::batch([tooltip, window::close(id)]);
                }

                window::move_to(id, slide.to)
            }
            Message::TrayEntered => {
                self.tray_hovered = true;
                self.sync_tray()
            }
            Message::TrayExited => {
                self.tray_hovered = false;
                self.sync_tray()
            }
            Message::Screen(message) => return self.screen_message(message),
            Message::WindowCloseRequested(id) => {
                if self.config_window == Some(id) {
                    return window::close(id);
                }

                Task::none()
            }
            Message::WindowClosed(id) => {
                log::debug!("window closed: {id:?}");

                if self.config_window == Some(id) {
                    self.config_window = None;
                    self.screen = None;
                    self.save_config();
                }

                Task::none()
            }
            Message::RelayScrolled(offset) => {
                self.scroll_offset = offset;
                Task::none()
            }
            Message::ChipHovered {
                index,
                position,
                truncated,
            } => {
                if !truncated {
                    return Task::none();
                }

                self.tooltip_hover = Some(TooltipHover {
                    index,
                    position,
                    since: Instant::now(),
                });

                self.move_tooltip(position)
            }
            Message::ChipHoverLeft(index) => {
                if self.tooltip_hover.is_some_and(|hover| hover.index == index) {
                    self.tooltip_hover = None;
                    self.close_tooltip()
                } else {
                    Task::none()
                }
            }
            Message::TooltipTick => {
                let Some(hover) = self.tooltip_hover else {
                    return Task::none();
                };

                if self.tooltip_window.is_some()
                    || self.hovered
                    || self.dragging.is_some()
                    || hover.since.elapsed() < TOOLTIP_DELAY
                {
                    return Task::none();
                }

                self.open_tooltip(hover.position)
            }
            Message::Placeholder => Task::none(),
        }
    }

    pub fn subscription(&self) -> iced::Subscription<Message> {
        let system_theme = if self.config.appearance.theme == config::ThemeMode::System {
            iced::Subscription::run(system_theme_stream)
        } else {
            iced::Subscription::none()
        };

        iced::Subscription::batch(vec![
            self.listen_to_drag(),
            self.listen_to_input(),
            self.listen_for_drag_state(),
            self.listen_for_drag_completion(),
            self.listen_to_tray_menu(),
            self.listen_to_window_opens(),
            self.listen_to_window_events(),
            self.listen_for_tooltip(),
            self.listen_to_slide(),
            iced::system::theme_changes().map(Message::SystemThemeChanged),
            system_theme,
        ])
    }

    fn listen_to_window_opens(&self) -> iced::Subscription<Message> {
        window::open_events().map(Message::RelayOpened)
    }

    fn listen_to_window_events(&self) -> iced::Subscription<Message> {
        window::events().filter_map(|(id, event)| match event {
            window::Event::CloseRequested => Some(Message::WindowCloseRequested(id)),
            window::Event::Closed => Some(Message::WindowClosed(id)),
            _ => None,
        })
    }

    fn listen_for_tooltip(&self) -> iced::Subscription<Message> {
        if self.tooltip_hover.is_none() || self.tooltip_window.is_some() {
            return iced::Subscription::none();
        }

        iced::Subscription::run(tooltip_ticks)
    }

    fn listen_to_slide(&self) -> iced::Subscription<Message> {
        if self.slide.is_none() {
            return iced::Subscription::none();
        }

        iced::Subscription::run(slide_ticks)
    }

    fn listen_to_tray_menu(&self) -> iced::Subscription<Message> {
        use iced::futures::channel::{mpsc, oneshot};
        use tray_icon::menu::MenuEvent;

        iced::Subscription::run(|| {
            iced::stream::channel(100, move |mut output: mpsc::Sender<Message>| async move {
                let (exit_tx, exit_rx) = oneshot::channel();

                std::thread::spawn(move || {
                    let receiver = MenuEvent::receiver();

                    while let Ok(event) = receiver.recv() {
                        match output.try_send(Message::TrayMenuClicked(event.id.0.clone())) {
                            Ok(()) => {}
                            Err(err) if err.is_disconnected() => break,
                            Err(_) => {}
                        }
                    }

                    let _ = exit_tx.send(());
                });

                let _ = exit_rx.await;
            })
        })
    }

    fn listen_for_drag_state(&self) -> iced::Subscription<Message> {
        iced::Subscription::run(drag_state_ticks)
    }

    fn listen_for_drag_completion(&self) -> iced::Subscription<Message> {
        if self.dragging.is_none() {
            return iced::Subscription::none();
        }

        iced::Subscription::run(drag_completion_ticks)
    }

    fn listen_to_drag(&self) -> iced::Subscription<Message> {
        window::events().filter_map(|(_, event)| match event {
            window::Event::FileHovered(_) => Some(Message::FileHovered),
            window::Event::FileDropped(path) => Some(Message::FileDropped(path)),
            window::Event::FilesHoveredLeft => Some(Message::FileHoveredLeft),
            _ => None,
        })
    }

    fn listen_to_input(&self) -> iced::Subscription<Message> {
        use iced::futures::channel::mpsc;

        iced::Subscription::run(|| {
            iced::stream::channel(100, move |output: mpsc::Sender<Message>| async move {
                let output = std::sync::Mutex::new(output);

                input::listen_input(Box::new(move |event| {
                    let message = match event {
                        InputEvent::PointerPressed => Message::PointerPressed,
                        InputEvent::PointerReleased => Message::PointerReleased,
                    };

                    output.lock().unwrap().try_send(message).ok();
                }));

                std::future::pending::<()>().await;
            })
        })
    }
}

fn ease_out_cubic(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

fn slide_ticks() -> impl iced::futures::Stream<Item = Message> {
    ticks(5, || Message::SlideTick(Instant::now()))
}

fn drag_completion_ticks() -> impl iced::futures::Stream<Item = Message> {
    ticks(100, || Message::PollDragResult)
}

fn drag_state_ticks() -> impl iced::futures::Stream<Item = Message> {
    ticks(100, || Message::PollDragState)
}

fn tooltip_ticks() -> impl iced::futures::Stream<Item = Message> {
    ticks(50, || Message::TooltipTick)
}

fn ticks(interval_ms: u64, message: fn() -> Message) -> impl iced::futures::Stream<Item = Message> {
    use std::time::Duration;

    use iced::futures::channel::{mpsc, oneshot};

    iced::stream::channel(1, move |mut output: mpsc::Sender<Message>| async move {
        let (exit_tx, exit_rx) = oneshot::channel();

        std::thread::spawn(move || {
            loop {
                std::thread::sleep(Duration::from_millis(interval_ms));

                match output.try_send(message()) {
                    Ok(()) => {}
                    Err(err) if err.is_disconnected() => break,
                    Err(_) => {}
                }
            }

            let _ = exit_tx.send(());
        });

        let _ = exit_rx.await;
    })
}

fn detected_system_mode() -> iced::theme::Mode {
    match dark_light::detect() {
        Ok(dark_light::Mode::Dark) => iced::theme::Mode::Dark,
        Ok(dark_light::Mode::Light) => iced::theme::Mode::Light,
        _ => iced::theme::Mode::None,
    }
}

fn system_theme_stream() -> impl iced::futures::Stream<Item = Message> {
    use iced::futures::channel::{mpsc, oneshot};

    iced::stream::channel(10, move |mut output: mpsc::Sender<Message>| async move {
        let (exit_tx, exit_rx) = oneshot::channel();

        std::thread::spawn(move || {
            if let Ok(watcher) = dark_light::subscribe() {
                for mode in watcher.iter() {
                    let mode = match mode {
                        dark_light::Mode::Dark => iced::theme::Mode::Dark,
                        dark_light::Mode::Light => iced::theme::Mode::Light,
                        _ => iced::theme::Mode::None,
                    };

                    if output.try_send(Message::SystemThemeChanged(mode)).is_err() {
                        break;
                    }
                }
            }

            let _ = exit_tx.send(());
        });

        let _ = exit_rx.await;
    })
}
