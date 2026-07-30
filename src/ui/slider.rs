//! `Slider`: a pill-shaped level control. The accent fill always covers at least a circle at
//! the left end, where the icon sits, so the icon is always on the fill. Drag, click or scroll.

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::{Tree, Widget, tree};
use iced::advanced::{Clipboard, Shell, mouse};
use iced::event::Event;
use iced::{Border, Color, Element, Length, Rectangle, Size, window};
use std::time::Instant;

pub struct Slider<'a, M> {
    content: Element<'a, M>,
    value: f32,
    height: f32,
    track: Color,
    fill: Color,
    on_change: Box<dyn Fn(f32) -> M + 'a>,
    on_release: Option<M>,
    /// Over-amplification: where 100 % sits on the track (0–1) and the colour of the part past
    /// it, in the danger colour (no notch: the colour change marks 100 %).
    boost: Option<(f32, Color)>,
}

/// `content` is drawn over the track (icon on the left, value on the right).
pub fn slider<'a, M>(
    content: impl Into<Element<'a, M>>,
    value: f32,
    height: f32,
    (track, fill): (Color, Color),
    on_change: impl Fn(f32) -> M + 'a,
) -> Slider<'a, M> {
    Slider {
        content: content.into(),
        value: value.clamp(0.0, 1.0),
        height,
        track,
        fill,
        on_change: Box::new(on_change),
        on_release: None,
        boost: None,
    }
}

impl<M> Slider<'_, M> {
    pub fn on_release(mut self, m: M) -> Self {
        self.on_release = Some(m);
        self
    }
    /// The track runs past 100 %: `mark` is where 100 % sits (0–1); past it the fill is `over`.
    pub fn boost(mut self, mark: f32, over: Color) -> Self {
        if mark < 1.0 {
            self.boost = Some((mark.clamp(0.0, 1.0), over));
        }
        self
    }
}

struct State {
    dragging: bool,
    /// The value shown, eased towards the real one so jumps glide.
    shown: f32,
    last: Option<Instant>,
}

impl<M: Clone> Widget<M, iced::Theme, iced::Renderer> for Slider<'_, M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }
    fn state(&self) -> tree::State {
        tree::State::new(State {
            dragging: false,
            shown: self.value,
            last: None,
        })
    }
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }
    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fixed(self.height))
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let size = limits.resolve(Length::Fill, Length::Fixed(self.height), Size::ZERO);
        let child_limits = layout::Limits::new(Size::ZERO, size);
        let child =
            self.content
                .as_widget_mut()
                .layout(&mut tree.children[0], renderer, &child_limits);
        let dy = (size.height - child.size().height) / 2.0;
        let child = child.move_to((0.0, dy));
        layout::Node::with_children(size, vec![child])
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &iced::Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, M>,
        _viewport: &Rectangle,
    ) {
        let b = layout.bounds();
        let state = tree.state.downcast_mut::<State>();
        let value_at = |x: f32| {
            let knob = b.height / 2.0;
            ((x - b.x - knob) / (b.width - 2.0 * knob)).clamp(0.0, 1.0)
        };
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(p) = cursor.position_over(b) {
                    state.dragging = true;
                    shell.publish((self.on_change)(value_at(p.x)));
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) if state.dragging => {
                shell.publish((self.on_change)(value_at(position.x)));
                shell.capture_event();
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) if state.dragging => {
                state.dragging = false;
                if let Some(m) = &self.on_release {
                    shell.publish(m.clone());
                }
                shell.capture_event();
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) if cursor.is_over(b) => {
                let lines = match delta {
                    mouse::ScrollDelta::Lines { y, .. } => *y,
                    mouse::ScrollDelta::Pixels { y, .. } => *y / 40.0,
                };
                if lines != 0.0 {
                    let v = (self.value + lines.signum() * 0.05).clamp(0.0, 1.0);
                    shell.publish((self.on_change)(v));
                    if let Some(m) = &self.on_release {
                        shell.publish(m.clone());
                    }
                    shell.capture_event();
                }
            }
            Event::Window(window::Event::RedrawRequested(now)) => {
                let dt = state.last.map_or(1.0 / 60.0, |t| {
                    now.saturating_duration_since(t).as_secs_f32()
                });
                if state.dragging {
                    state.shown = self.value;
                } else {
                    // Exponential approach: ~90 % of the way in 120 ms.
                    let k = 1.0 - (-dt * 19.0).exp();
                    state.shown += (self.value - state.shown) * k;
                }
                if (state.shown - self.value).abs() > 1e-3 {
                    state.last = Some(*now);
                    shell.request_redraw();
                } else {
                    state.shown = self.value;
                    state.last = None;
                }
            }
            _ => {}
        }
        if (state.shown - self.value).abs() > 1e-3 && state.last.is_none() {
            shell.request_redraw();
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &iced::Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        use iced::advanced::Renderer as _;
        let b = layout.bounds();
        let state = tree.state.downcast_ref::<State>();
        let r = b.height / 2.0;
        renderer.fill_quad(
            renderer::Quad {
                bounds: b,
                border: Border::default().rounded(r),
                ..renderer::Quad::default()
            },
            self.track,
        );
        let at = |v: f32| b.height + (b.width - b.height) * v;
        let fill_w = at(state.shown);
        // Past 100 % the whole fill is the danger colour, one piece (a red end stuck onto the
        // blue read as misaligned, design call 2026-11-16).
        let colour = match self.boost {
            Some((mark, over)) if state.shown > mark + 1e-4 => over,
            _ => self.fill,
        };
        renderer.fill_quad(
            renderer::Quad {
                bounds: Rectangle { width: fill_w, ..b },
                border: Border::default().rounded(r),
                ..renderer::Quad::default()
            },
            colour,
        );
        renderer.with_layer(b, |renderer| {
            self.content.as_widget().draw(
                &tree.children[0],
                renderer,
                theme,
                style,
                layout.children().next().unwrap(),
                cursor,
                viewport,
            );
        });
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        let state = tree.state.downcast_ref::<State>();
        if state.dragging {
            mouse::Interaction::Grabbing
        } else if cursor.is_over(layout.bounds()) {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::default()
        }
    }
}

impl<'a, M: Clone + 'a> From<Slider<'a, M>> for Element<'a, M> {
    fn from(s: Slider<'a, M>) -> Self {
        Element::new(s)
    }
}
