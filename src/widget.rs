use iced::advanced::image::{self, Renderer as ImageRenderer};
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Quad};
use iced::advanced::text::{self, Paragraph, Text};
use iced::advanced::widget::{Tree, Widget, tree};
use iced::advanced::{Clipboard, Shell, mouse};
use iced::{
    Background, Border, Element, Event, Length, Pixels, Point, Radians, Rectangle, Shadow, Size,
    Theme, alignment::Vertical,
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

#[derive(Debug, Clone, Default)]
struct State {
    is_pressed: bool,
    press_origin: Option<Point>,
    dragging: bool,
    hovered: bool,
    truncated: bool,
    label: String,
}

impl<Renderer> Widget<Message, Theme, Renderer> for FileChip<'_>
where
    Renderer: text::Renderer + ImageRenderer<Handle = image::Handle>,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn size(&self) -> Size<Length> {
        Size {
            width: Length::Fixed(self.metrics.chip_size),
            height: Length::Fixed(self.metrics.chip_size),
        }
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let available = limits.max();
        let size = self
            .metrics
            .chip_size
            .min(available.width)
            .min(available.height);
        let label_width = (size - self.metrics.card_padding * 2.0).max(0.0);

        let label = truncate::<Renderer>(
            &self.file.file_name,
            label_width,
            &self.metrics,
            renderer.default_font(),
        );

        let state = tree.state.downcast_mut::<State>();
        state.truncated = label != self.file.file_name;
        state.label = label;

        layout::Node::new(Size::new(size, size))
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

        if matches!(event, Event::Mouse(_)) {
            let hovered = cursor.is_over(bounds);

            if hovered != state.hovered {
                state.hovered = hovered;

                shell.publish(if hovered {
                    Message::ChipHovered {
                        index: self.index,
                        y: bounds.y,
                        truncated: state.truncated,
                    }
                } else {
                    Message::ChipHoverLeft(self.index)
                });
            }
        }

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
            Some(palette.card_pressed)
        } else if self.selected {
            Some(palette.card_selected)
        } else if hovered {
            Some(palette.card_hover)
        } else {
            None
        };

        if let Some(background) = background {
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
        }

        if let Some(handle) = &self.file.icon {
            renderer.draw_image(
                image::Image {
                    handle: handle.clone(),
                    filter_method: image::FilterMethod::Linear,
                    rotation: Radians(0.0),
                    border_radius: Default::default(),
                    opacity: 1.0,
                    snap: true,
                },
                Rectangle {
                    x: bounds.x + (bounds.width - metrics.icon_size) / 2.0,
                    y: bounds.y + metrics.card_padding,
                    width: metrics.icon_size,
                    height: metrics.icon_size,
                },
                bounds,
            );
        }

        let line_height = metrics.name_size * metrics.line_height;

        renderer.fill_text(
            Text {
                content: state.label.clone(),
                bounds: Size::new(
                    (bounds.width - metrics.card_padding * 2.0).max(0.0),
                    line_height,
                ),
                size: Pixels(metrics.name_size),
                line_height: text::LineHeight::Relative(metrics.line_height),
                font: renderer.default_font(),
                align_x: text::Alignment::Center,
                align_y: Vertical::Top,
                shaping: text::Shaping::default(),
                wrapping: text::Wrapping::None,
            },
            Point::new(
                bounds.center().x,
                bounds.y + metrics.card_padding + metrics.icon_size + metrics.icon_gap,
            ),
            palette.text,
            bounds,
        );
    }
}

impl<'a> From<FileChip<'a>> for Element<'a, Message> {
    fn from(chip: FileChip<'a>) -> Self {
        Element::new(chip)
    }
}

fn truncate<Renderer>(
    name: &str,
    width: f32,
    metrics: &Metrics,
    font: <Renderer as text::Renderer>::Font,
) -> String
where
    Renderer: text::Renderer,
{
    let measure = |content: &str| {
        <Renderer::Paragraph as text::Paragraph>::with_text(Text {
            content,
            bounds: Size::new(f32::INFINITY, metrics.name_size * metrics.line_height),
            size: Pixels(metrics.name_size),
            line_height: text::LineHeight::Relative(metrics.line_height),
            font,
            align_x: text::Alignment::Left,
            align_y: Vertical::Top,
            shaping: text::Shaping::default(),
            wrapping: text::Wrapping::None,
        })
        .min_bounds()
        .width
    };

    if measure(name) <= width {
        return name.to_string();
    }

    let ellipsis = '…';
    let budget = (width - measure(&ellipsis.to_string())).max(0.0);
    let characters: Vec<char> = name.chars().collect();
    let mut low = 0;
    let mut high = characters.len();

    while low < high {
        let middle = (low + high + 1) / 2;
        let candidate: String = characters[..middle].iter().collect();

        if measure(&candidate) <= budget {
            low = middle;
        } else {
            high = middle - 1;
        }
    }

    let mut truncated: String = characters[..low].iter().collect();
    truncated.push(ellipsis);
    truncated
}
