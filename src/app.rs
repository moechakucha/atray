use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::LazyLock,
    time::{Duration, Instant, SystemTime},
};

use iced::widget::{column, container, scrollable};
use iced::{Element, Length, Task, Theme, window};
use rdev::Key;

use crate::platform::DragHandler;
use crate::theme;
use crate::widget::FileChip;

#[derive(Debug)]
pub struct DeferredFile {
    ownership: Ownership,
    pub file_name: String,
    pub file_size: u64,
    pub last_modified: SystemTime,
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
        })
    }

    pub fn path(&self) -> &PathBuf {
        match &self.ownership {
            Ownership::Referenced(path) => path,
            Ownership::Owned(owned_path) => owned_path.path(),
        }
    }
}

pub enum Message {
    PointerPressed,
    PointerReleased,
    PollDragState,
    FileHovered,
    FileHoveredLeft,
    FileDropped(PathBuf),
    FileSelectionToggled(usize),
    FileTakenOut(usize),
    KeyPressed(Key),
    KeyReleased(Key),
    PollDragResult,
    TrayMenuClicked(String),
    SystemThemeChanged,
    RelayOpened(window::Id),
    RelayPositioned(Option<iced::Point>),
    SlideTick(Instant),
}

const WINDOW_WIDTH: f32 = 150.0;
const WINDOW_HEIGHT: f32 = 430.0;
const SLIDE_DURATION: Duration = Duration::from_millis(220);

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
    opening: bool,
}

pub struct App {
    file_relay: Vec<DeferredFile>,
    window_id: Option<window::Id>,
    pressed_keys: HashSet<Key>,
    drag_handler: Box<dyn DragHandler>,
    dragging: Option<Vec<usize>>,
    lingering: Vec<DeferredFile>,
    selected: HashSet<usize>,
    hovered: bool,
    theme: Option<Theme>,
    metrics: theme::Metrics,
    slide: Option<Slide>,
    resting_y: Option<f32>,
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
                pressed_keys: HashSet::new(),
                drag_handler: crate::platform::get_drag_handler(),
                dragging: None,
                lingering: Vec::new(),
                selected: HashSet::new(),
                hovered: false,
                theme,
                metrics,
                slide: None,
                resting_y: None,
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

        task.discard()
    }

    fn slide_out(&mut self) -> Task<Message> {
        if self.slide.is_some_and(|slide| !slide.opening) {
            return Task::none();
        }

        let Some(id) = self.window_id else {
            return Task::none();
        };

        if self.resting_y.is_none() {
            self.window_id = None;
            return window::close(id);
        }

        self.slide = Some(Slide {
            started: Instant::now(),
            opening: false,
        });

        Task::none()
    }

    fn slide_in(&mut self) -> Task<Message> {
        if !self.slide.is_some_and(|slide| !slide.opening) {
            return Task::none();
        }

        self.slide = Some(Slide {
            started: Instant::now(),
            opening: true,
        });

        Task::none()
    }

    pub fn view(&self, _id: window::Id) -> Element<'_, Message> {
        let files: Vec<Element<Message>> = self
            .file_relay
            .iter()
            .enumerate()
            .map(|(index, file)| {
                FileChip::new(index, file, self.selected.contains(&index), self.metrics).into()
            })
            .collect();

        container(scrollable(column(files).spacing(theme::SPACING)).height(Length::Fill))
            .style(theme::surface)
            .padding(theme::VIEW_PADDING)
            .width(Length::Fill)
            .height(Length::Fill)
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

                self.try_open_relay()
            }
            Message::PointerReleased => {
                self.drag_handler.cancel_pending_drag();
                Task::none()
            }
            Message::PollDragState => {
                if self.window_id.is_none() {
                    return self.try_open_relay();
                }

                let idle = self.dragging.is_none()
                    && self.file_relay.is_empty()
                    && !self.drag_handler.is_dragging();

                if idle {
                    self.slide_out()
                } else {
                    self.slide_in()
                }
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
            Message::KeyPressed(key) => {
                self.pressed_keys.insert(key);
                Task::none()
            }
            Message::KeyReleased(key) => {
                self.pressed_keys.remove(&key);
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

                let should_move = self.pressed_keys.contains(&Key::Alt)
                    || self.pressed_keys.contains(&Key::AltGr);

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
                if self.window_id == Some(id) {
                    window::position(id).map(Message::RelayPositioned)
                } else {
                    Task::none()
                }
            }
            Message::RelayPositioned(position) => {
                if let Some(position) = position {
                    self.resting_y = Some(position.y);
                    self.slide = Some(Slide {
                        started: Instant::now(),
                        opening: true,
                    });
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
                let travel = if slide.opening { 1.0 - eased } else { eased };

                if progress < 1.0 {
                    return window::move_to(id, iced::Point::new(-WINDOW_WIDTH * travel, y));
                }

                self.slide = None;

                if slide.opening {
                    window::move_to(id, iced::Point::new(0.0, y))
                } else {
                    self.window_id = None;
                    window::close(id)
                }
            }
        }
    }

    pub fn subscription(&self) -> iced::Subscription<Message> {
        iced::Subscription::batch(vec![
            self.listen_to_drag(),
            self.listen_to_rdev(),
            self.listen_for_drag_state(),
            self.listen_for_drag_completion(),
            self.listen_to_tray_menu(),
            self.listen_to_window_opens(),
            self.listen_to_slide(),
            iced::system::theme_changes().map(|_| Message::SystemThemeChanged),
        ])
    }

    fn listen_to_window_opens(&self) -> iced::Subscription<Message> {
        window::open_events().map(Message::RelayOpened)
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

    fn listen_to_rdev(&self) -> iced::Subscription<Message> {
        use iced::futures::channel::{mpsc, oneshot};
        use rdev::{Button, EventType};

        iced::Subscription::run(|| {
            iced::stream::channel(100, move |mut output: mpsc::Sender<Message>| async move {
                let (exit_tx, exit_rx) = oneshot::channel();
                std::thread::spawn(move || {
                    if let Err(err) = rdev::listen(move |event| match event.event_type {
                        EventType::KeyPress(key) => {
                            output.try_send(Message::KeyPressed(key)).ok();
                        }
                        EventType::KeyRelease(key) => {
                            output.try_send(Message::KeyReleased(key)).ok();
                        }
                        EventType::ButtonPress(btn) => {
                            if matches!(btn, Button::Left) {
                                output.try_send(Message::PointerPressed).ok();
                            }
                        }
                        EventType::ButtonRelease(btn) => {
                            if matches!(btn, Button::Left) {
                                output.try_send(Message::PointerReleased).ok();
                            }
                        }
                        _ => {}
                    }) {
                        eprintln!("Error when listening rdev events: {:?}", err);
                    }

                    let _ = exit_tx.send(());
                });

                let _ = exit_rx.await;
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
