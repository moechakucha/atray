use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::LazyLock,
    time::{Duration, Instant, SystemTime},
};

use iced::advanced::image;
use iced::advanced::text::Wrapping;
use iced::widget::{column, container, mouse_area, scrollable, text};
use iced::{Alignment, Element, Length, Task, Theme, window};

use crate::platform::{DragHandler, InputEvent, Modifier};
use crate::theme;
use crate::widget::FileChip;

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
    pub fn new(
        path: PathBuf,
        cache_path: Option<&Path>,
        should_move: bool,
    ) -> std::io::Result<Self> {
        let metadata = std::fs::metadata(&path)?;
        let file_name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown file")
            .to_string();

        let original = path;

        let icon = crate::platform::file_icon(&original, (theme::ICON_SIZE * 2.0).round() as u32)
            .map(|icon| image::Handle::from_rgba(icon.width, icon.height, icon.rgba));

        let ownership = if should_move {
            match cache_path {
                Some(cache_path) => {
                    let stored = cache_path.join(&file_name);
                    std::fs::copy(&original, &stored)?;
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
                eprintln!("failed to remove original {original:?}: {err}");
            }
        }

        Ok(Self {
            ownership,
            file_name,
            file_size: metadata.len(),
            last_modified: metadata.modified()?,
            icon,
        })
    }

    pub fn path(&self) -> &PathBuf {
        match &self.ownership {
            Ownership::Referenced(path) => path,
            Ownership::Owned(owned_path) => owned_path.path(),
        }
    }
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
    PollDragResult,
    TrayMenuClicked(String),
    SystemThemeChanged,
    RelayOpened(window::Id),
    RelayPositioned(Option<iced::Point>),
    RelayScrolled(f32),
    ChipHovered {
        index: usize,
        y: f32,
        truncated: bool,
    },
    ChipHoverLeft(usize),
    TrayEntered,
    TrayExited,
    TooltipTick,
    SlideTick(Instant),
}

const WINDOW_WIDTH: f32 = 150.0;
const WINDOW_HEIGHT: f32 = 430.0;
const SLIDE_DURATION: Duration = Duration::from_millis(220);
const TRAY_HIDDEN_X: f32 = -WINDOW_WIDTH;
const TRAY_PEEK_WIDTH: f32 = 16.0;
const TRAY_PEEK_X: f32 = TRAY_PEEK_WIDTH - WINDOW_WIDTH;
const TRAY_OPEN_X: f32 = 0.0;
const TOOLTIP_WIDTH: f32 = 320.0;
const TOOLTIP_HEIGHT: f32 = 240.0;
const TOOLTIP_GAP: f32 = 4.0;
const TOOLTIP_DELAY: Duration = Duration::from_millis(350);

static WINDOW_SETTINGS: LazyLock<window::Settings> = LazyLock::new(|| window::Settings {
    size: (WINDOW_WIDTH, WINDOW_HEIGHT).into(),
    position: window::Position::SpecificWith(|window, resolution| {
        let y = (resolution.height - window.height) / 2.0;
        iced::Point::new(-window.width, y)
    }),
    decorations: false,
    closeable: false,
    resizable: false,
    transparent: true,
    minimizable: false,
    blur: true,
    level: window::Level::AlwaysOnTop,
    exit_on_close_request: false,
    platform_specific: crate::platform::platform_window_settings(),
    ..Default::default()
});

#[derive(Clone, Copy)]
struct Slide {
    started: Instant,
    from: f32,
    to: f32,
}

#[derive(Clone, Copy)]
struct TooltipHover {
    index: usize,
    y: f32,
    since: Instant,
}

pub struct App {
    file_relay: Vec<DeferredFile>,
    window_id: Option<window::Id>,
    drag_handler: Box<dyn DragHandler>,
    dragging: Option<Vec<usize>>,
    lingering: Vec<DeferredFile>,
    selected: HashSet<usize>,
    hovered: bool,
    theme: Option<Theme>,
    metrics: theme::Metrics,
    slide: Option<Slide>,
    resting_y: Option<f32>,
    window_x: f32,
    tray_hovered: bool,
    scroll_offset: f32,
    tooltip_hover: Option<TooltipHover>,
    tooltip_window: Option<window::Id>,
}

impl App {
    pub fn new() -> (Self, Task<Message>) {
        let (theme, metrics) = match theme::system() {
            Some((theme, metrics)) => (Some(theme), metrics),
            None => (None, theme::Metrics::default()),
        };

        (
            Self {
                file_relay: Vec::new(),
                window_id: None,
                drag_handler: crate::platform::get_drag_handler(),
                dragging: None,
                lingering: Vec::new(),
                selected: HashSet::new(),
                hovered: false,
                theme,
                metrics,
                slide: None,
                resting_y: None,
                window_x: TRAY_HIDDEN_X,
                tray_hovered: false,
                scroll_offset: 0.0,
                tooltip_hover: None,
                tooltip_window: None,
            },
            Task::none(),
        )
    }

    pub fn theme(&self, _id: window::Id) -> Option<Theme> {
        self.theme.clone()
    }

    pub fn add_from_location(
        &mut self,
        path: PathBuf,
        cache_path: Option<&Path>,
        should_move: bool,
    ) -> std::io::Result<()> {
        let file = DeferredFile::new(path, cache_path, should_move)?;
        self.file_relay.push(file);
        Ok(())
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

        let paths: Vec<PathBuf> = indices
            .iter()
            .map(|&i| self.file_relay[i].path().clone())
            .collect();

        if !self.drag_handler.start_drag(&paths) {
            return false;
        }

        self.lingering.clear();
        self.dragging = Some(indices);

        true
    }

    fn finish_drag(&mut self, success: bool) {
        let Some(indices) = self.dragging.take() else {
            return;
        };

        if !success {
            return;
        }

        for &index in indices.iter().rev() {
            if index < self.file_relay.len() {
                self.lingering.push(self.file_relay.remove(index));
            }
        }

        self.selected.clear();
    }

    fn try_open_relay(&mut self) -> Task<Message> {
        if self.window_id.is_some() || !self.drag_handler.is_dragging() {
            return Task::none();
        }

        let (id, task) = window::open(WINDOW_SETTINGS.clone());
        self.window_id = Some(id);
        self.window_x = TRAY_HIDDEN_X;
        self.scroll_offset = 0.0;

        task.discard()
    }

    fn tray_target(&self) -> f32 {
        if self.dragging.is_some() || self.drag_handler.is_dragging() || self.tray_hovered {
            TRAY_OPEN_X
        } else if self.file_relay.is_empty() {
            TRAY_HIDDEN_X
        } else {
            TRAY_PEEK_X
        }
    }

    fn sync_tray(&mut self) -> Task<Message> {
        if self.window_id.is_none() || self.resting_y.is_none() {
            return Task::none();
        }

        let target = self.tray_target();

        if self.slide.is_some_and(|slide| slide.to == target) {
            return Task::none();
        }

        self.slide = Some(Slide {
            started: Instant::now(),
            from: self.window_x,
            to: target,
        });

        Task::none()
    }

    fn tooltip_anchor(&self, y: f32) -> f32 {
        let resting_y = self.resting_y.unwrap_or(0.0);
        let top = resting_y + y - self.scroll_offset;

        top.clamp(
            resting_y,
            resting_y + (WINDOW_HEIGHT - TOOLTIP_HEIGHT).max(0.0),
        )
    }

    fn open_tooltip(&mut self, y: f32) -> Task<Message> {
        let settings = window::Settings {
            size: (TOOLTIP_WIDTH, TOOLTIP_HEIGHT).into(),
            position: window::Position::Specific(iced::Point::new(
                WINDOW_WIDTH + TOOLTIP_GAP,
                self.tooltip_anchor(y),
            )),
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

    fn move_tooltip(&mut self, y: f32) -> Task<Message> {
        let Some(id) = self.tooltip_window else {
            return Task::none();
        };

        window::move_to(
            id,
            iced::Point::new(WINDOW_WIDTH + TOOLTIP_GAP, self.tooltip_anchor(y)),
        )
    }

    fn close_tooltip(&mut self) -> Task<Message> {
        match self.tooltip_window.take() {
            Some(id) => window::close(id),
            None => Task::none(),
        }
    }

    pub fn view(&self, id: window::Id) -> Element<'_, Message> {
        if self.tooltip_window == Some(id) {
            return self.tooltip_view();
        }

        self.relay_view()
    }

    fn tooltip_view(&self) -> Element<'_, Message> {
        let name = self
            .tooltip_hover
            .and_then(|hover| self.file_relay.get(hover.index))
            .map(|file| file.file_name.as_str())
            .unwrap_or_default();

        container(
            text(name)
                .size(self.metrics.name_size)
                .wrapping(Wrapping::WordOrGlyph),
        )
        .style(theme::tooltip)
        .padding(theme::CARD_PADDING)
        .width(Length::Shrink)
        .height(Length::Shrink)
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

        mouse_area(
            container(
                scrollable(
                    column(files)
                        .spacing(theme::SPACING)
                        .align_x(Alignment::Center)
                        .width(Length::Fill),
                )
                .height(Length::Fill)
                .on_scroll(|viewport| Message::RelayScrolled(viewport.absolute_offset().y)),
            )
            .style(theme::surface)
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
            Message::PollDragResult => Task::none(),
            Message::FileHovered => {
                self.hovered = true;
                Task::none()
            }
            Message::FileHoveredLeft => {
                self.hovered = false;
                Task::none()
            }
            Message::FileDropped(path) => {
                self.hovered = false;

                if let Some(hover) = &mut self.tooltip_hover {
                    hover.since = Instant::now();
                }

                if let Some(index) = self.lingering.iter().position(|file| file.path() == &path) {
                    self.file_relay.push(self.lingering.remove(index));
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

                let should_move = crate::platform::modifiers().contains(Modifier::Alt);

                if let Err(err) = self.add_from_location(path.clone(), None, should_move) {
                    eprintln!("failed to add {path:?}: {err}");
                }

                Task::none()
            }
            Message::TrayMenuClicked(id) => {
                if id == "quit" {
                    return iced::exit();
                }
                Task::none()
            }
            Message::SystemThemeChanged => {
                if let Some((theme, metrics)) = theme::system() {
                    self.theme = Some(theme);
                    self.metrics = metrics;
                }

                Task::none()
            }
            Message::RelayOpened(id) => {
                if self.tooltip_window == Some(id) {
                    return window::enable_mouse_passthrough(id);
                }

                if self.window_id == Some(id) {
                    window::position(id).map(Message::RelayPositioned)
                } else {
                    Task::none()
                }
            }
            Message::RelayPositioned(position) => {
                let Some(position) = position else {
                    if let Some(id) = self.window_id.take() {
                        return window::close(id);
                    }

                    return Task::none();
                };

                if self.window_id.is_some() {
                    self.window_x = position.x;
                    self.resting_y = Some(position.y);
                    return self.sync_tray();
                }

                Task::none()
            }
            Message::SlideTick(now) => {
                let Some(slide) = self.slide else {
                    return Task::none();
                };

                let (Some(id), Some(y)) = (self.window_id, self.resting_y) else {
                    self.slide = None;
                    return Task::none();
                };

                let elapsed = now.saturating_duration_since(slide.started).as_secs_f32();
                let progress = (elapsed / SLIDE_DURATION.as_secs_f32()).clamp(0.0, 1.0);
                let eased = ease_out_cubic(progress);
                let x = slide.from + (slide.to - slide.from) * eased;

                self.window_x = x;

                if progress < 1.0 {
                    return window::move_to(id, iced::Point::new(x, y));
                }

                self.slide = None;

                if slide.to == TRAY_HIDDEN_X {
                    self.window_id = None;
                    self.tooltip_hover = None;
                    let tooltip = self.close_tooltip();

                    return Task::batch([tooltip, window::close(id)]);
                }

                window::move_to(id, iced::Point::new(slide.to, y))
            }
            Message::TrayEntered => {
                self.tray_hovered = true;
                self.sync_tray()
            }
            Message::TrayExited => {
                self.tray_hovered = false;
                self.sync_tray()
            }
            Message::RelayScrolled(offset) => {
                self.scroll_offset = offset;
                Task::none()
            }
            Message::ChipHovered {
                index,
                y,
                truncated,
            } => {
                if !truncated {
                    return Task::none();
                }

                self.tooltip_hover = Some(TooltipHover {
                    index,
                    y,
                    since: Instant::now(),
                });

                self.move_tooltip(y)
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

                self.open_tooltip(hover.y)
            }
        }
    }

    pub fn subscription(&self) -> iced::Subscription<Message> {
        iced::Subscription::batch(vec![
            self.listen_to_drag(),
            self.listen_to_input(),
            self.listen_for_drag_state(),
            self.listen_for_drag_completion(),
            self.listen_to_tray_menu(),
            self.listen_to_window_opens(),
            self.listen_for_tooltip(),
            self.listen_to_slide(),
            iced::system::theme_changes().map(|_| Message::SystemThemeChanged),
        ])
    }

    fn listen_to_window_opens(&self) -> iced::Subscription<Message> {
        window::open_events().map(Message::RelayOpened)
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

                crate::platform::listen_input(Box::new(move |event| {
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
    ticks(16, || Message::SlideTick(Instant::now()))
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
