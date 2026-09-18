use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Quad};
use iced::advanced::text::{self, Text};
use iced::advanced::widget::{Tree, Widget, tree};
use iced::advanced::{Clipboard, Shell, mouse};
use iced::{
    Background, Border, Element, Event, Length, Pixels, Point, Rectangle, Shadow, Size, Theme,
    alignment::Vertical,
};

use crate::app::{DeferredFile, Message};
use crate::theme::{self, Metrics};

const DRAG_THRESHOLD: f32 = 4.0;

pub struct FileChip<'a> {
    file: &'a DeferredFile,
    index: usize,
    selected: bool,
    metrics: Metrics,
}

impl<'a> FileChip<'a> {
    pub fn new(index: usize, file: &'a DeferredFile, selected: bool, metrics: Metrics) -> Self {
        Self {
            file,
            index,
            selected,
            metrics,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct State {
    is_pressed: bool,
    press_origin: Option<Point>,
    dragging: bool,
}

impl<Renderer> Widget<Message, Theme, Renderer> for FileChip<'_>
where
    Renderer: text::Renderer,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn size(&self) -> Size<Length> {
        Size {
            width: Length::Fill,
            height: Length::Fixed(self.metrics.chip_height),
        }
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::Node::new(limits.resolve(
            Length::Fill,
            Length::Fixed(self.metrics.chip_height),
            Size::ZERO,
        ))
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let state = tree.state.downcast_mut::<State>();

        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if cursor.is_over(bounds) {
                    state.is_pressed = true;
                    state.dragging = false;
                    state.press_origin = cursor.position();

                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                if state.is_pressed && !state.dragging {
                    if let Some(origin) = state.press_origin {
                        if origin.distance(*position) > DRAG_THRESHOLD {
                            state.dragging = true;

                            shell.publish(Message::FileTakenOut(self.index));
                            shell.capture_event();
                        }
                    }
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if state.is_pressed {
                    let was_dragging = state.dragging;

                    state.is_pressed = false;
                    state.dragging = false;
                    state.press_origin = None;

                    if !was_dragging && cursor.is_over(bounds) {
                        shell.publish(Message::FileSelectionToggled(self.index));
                    }

                    shell.capture_event();
                }
            }
            _ => {}
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        if !cursor.is_over(layout.bounds()) {
            return mouse::Interaction::None;
        }

        if tree.state.downcast_ref::<State>().is_pressed {
            mouse::Interaction::Grabbing
        } else {
            mouse::Interaction::Grab
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();

        if bounds.intersection(viewport).is_none() {
            return;
        }

        let state = tree.state.downcast_ref::<State>();
        let hovered = cursor.is_over(bounds);
        let palette = theme::Colors::of(theme);
        let metrics = self.metrics;

        let background = if state.is_pressed {
            palette.card_pressed
        } else if self.selected {
            palette.card_selected
        } else if hovered {
            palette.card_hover
        } else {
            palette.card
        };

        let border_color = if self.selected {
            palette.accent
        } else if hovered {
            palette.border_hover
        } else {
            palette.border
        };

        renderer.fill_quad(
            Quad {
                bounds,
                border: Border {
                    color: border_color,
                    width: 1.0,
                    radius: metrics.card_radius.into(),
                },
                shadow: Shadow::default(),
                snap: true,
            },
            Background::Color(background),
        );

        let accent = Rectangle {
            x: bounds.x + metrics.card_padding,
            y: bounds.y + metrics.card_padding,
            width: metrics.accent_width,
            height: (bounds.height - metrics.card_padding * 2.0).max(0.0),
        };

        renderer.fill_quad(
            Quad {
                bounds: accent,
                border: Border {
                    radius: (metrics.accent_width / 2.0).into(),
                    ..Border::default()
                },
                shadow: Shadow::default(),
                snap: true,
            },
            Background::Color(theme::accent(theme, &extension(&self.file.file_name))),
        );

        let text_x = bounds.x + metrics.card_padding + metrics.accent_width + metrics.accent_gap;
        let text_width = (bounds.x + bounds.width - metrics.card_padding - text_x).max(0.0);

        renderer.fill_text(
            Text {
                content: self.file.file_name.clone(),
                bounds: Size::new(text_width, metrics.name_size * metrics.line_height),
                size: Pixels(metrics.name_size),
                line_height: text::LineHeight::Relative(metrics.line_height),
                font: renderer.default_font(),
                align_x: text::Alignment::Left,
                align_y: Vertical::Top,
                shaping: text::Shaping::default(),
                wrapping: text::Wrapping::None,
            },
            Point::new(text_x, bounds.y + metrics.card_padding),
            palette.text,
            bounds,
        );

        renderer.fill_text(
            Text {
                content: detail(&self.file.file_name, self.file.file_size),
                bounds: Size::new(text_width, metrics.detail_size * metrics.line_height),
                size: Pixels(metrics.detail_size),
                line_height: text::LineHeight::Relative(metrics.line_height),
                font: renderer.default_font(),
                align_x: text::Alignment::Left,
                align_y: Vertical::Top,
                shaping: text::Shaping::default(),
                wrapping: text::Wrapping::None,
            },
            Point::new(
                text_x,
                bounds.y + bounds.height
                    - metrics.card_padding
                    - metrics.detail_size * metrics.line_height,
            ),
            palette.text_muted,
            bounds,
        );
    }
}

impl<'a> From<FileChip<'a>> for Element<'a, Message> {
    fn from(chip: FileChip<'a>) -> Self {
        Element::new(chip)
    }
}

fn extension(file_name: &str) -> String {
    file_name
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_uppercase())
        .unwrap_or_default()
}

fn detail(file_name: &str, file_size: u64) -> String {
    let extension = extension(file_name);

    if extension.is_empty() {
        format_size(file_size)
    } else {
        format!("{extension}  {}", format_size(file_size))
    }
}

fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];

    let mut value = bytes as f64;
    let mut unit = 0;

    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }

    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}
