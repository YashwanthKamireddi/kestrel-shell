//! `Smooth`: wraps a scrollable so a mouse wheel's notches glide instead of jumping. Each
//! notch adds its distance to what is still to go; every frame passes a share of it on as
//! pixel scrolling (most of it within ~120 ms, so it keeps up with a fast wheel and never
//! trails). Touchpads already scroll in pixels: those pass straight through.

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::{Tree, Widget, tree};
use iced::advanced::{Clipboard, Shell, mouse};
use iced::event::Event;
use iced::{Element, Length, Rectangle, Size};

/// One wheel notch, in pixels (iced's own line height for scrolling).
const LINE: f32 = 60.0;
/// How fast the rest is caught up (per second).
const RATE: f32 = 26.0;

pub struct Smooth<'a, M> {
    content: Element<'a, M>,
}

pub fn smooth<'a, M>(content: impl Into<Element<'a, M>>) -> Smooth<'a, M> {
    Smooth {
        content: content.into(),
    }
}

#[derive(Default)]
struct State {
    /// Pixels still to scroll (x, y).
    left: (f32, f32),
    last: Option<std::time::Instant>,
}

impl<M> Widget<M, iced::Theme, iced::Renderer> for Smooth<'_, M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }
    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }
    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }
    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }
    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let child = self
            .content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits);
        layout::Node::with_children(child.size(), vec![child])
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, M>,
        viewport: &Rectangle,
    ) {
        let child = layout.children().next().unwrap();
        let state = tree.state.downcast_mut::<State>();
        let mut pass = |tree: &mut Tree, e: &Event, shell: &mut Shell<'_, M>| {
            self.content.as_widget_mut().update(
                &mut tree.children[0],
                e,
                child,
                cursor,
                renderer,
                clipboard,
                shell,
                viewport,
            );
        };
        match event {
            Event::Mouse(mouse::Event::WheelScrolled {
                delta: mouse::ScrollDelta::Lines { x, y },
            }) if cursor.is_over(layout.bounds()) => {
                state.left.0 += x * LINE;
                state.left.1 += y * LINE;
                shell.capture_event();
                shell.request_redraw();
            }
            Event::Window(iced::window::Event::RedrawRequested(now)) => {
                let st = tree.state.downcast_mut::<State>();
                // A redraw of a frame already stepped (same instant) moves nothing.
                let stepped = st.last == Some(*now);
                if !stepped && (st.left.0.abs() > 0.5 || st.left.1.abs() > 0.5) {
                    let dt = st
                        .last
                        .map_or(1.0 / 60.0, |t| {
                            now.saturating_duration_since(t).as_secs_f32()
                        })
                        .clamp(0.0, 0.05);
                    st.last = Some(*now);
                    let k = 1.0 - (-dt * RATE).exp();
                    let mut step = (st.left.0 * k, st.left.1 * k);
                    // The tail goes in one piece (no endless sub-pixel creep).
                    if (st.left.0 - step.0).abs() < 0.5 && (st.left.1 - step.1).abs() < 0.5 {
                        step = st.left;
                    }
                    st.left.0 -= step.0;
                    st.left.1 -= step.1;
                    let scroll = Event::Mouse(mouse::Event::WheelScrolled {
                        delta: mouse::ScrollDelta::Pixels {
                            x: step.0,
                            y: step.1,
                        },
                    });
                    pass(tree, &scroll, shell);
                    if st_left_remaining(tree) {
                        shell.request_redraw();
                    } else {
                        tree.state.downcast_mut::<State>().last = None;
                    }
                }
                pass(tree, event, shell);
            }
            _ => pass(tree, event, shell),
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
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout.children().next().unwrap(),
            cursor,
            viewport,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout.children().next().unwrap(),
            cursor,
            viewport,
            renderer,
        )
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn iced::advanced::widget::Operation,
    ) {
        self.content.as_widget_mut().operate(
            &mut tree.children[0],
            layout.children().next().unwrap(),
            renderer,
            operation,
        );
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: iced::Vector,
    ) -> Option<iced::advanced::overlay::Element<'b, M, iced::Theme, iced::Renderer>> {
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout.children().next().unwrap(),
            renderer,
            viewport,
            translation,
        )
    }
}

fn st_left_remaining(tree: &mut Tree) -> bool {
    let st = tree.state.downcast_mut::<State>();
    st.left.0.abs() > 0.5 || st.left.1.abs() > 0.5
}

impl<'a, M: 'a> From<Smooth<'a, M>> for Element<'a, M> {
    fn from(s: Smooth<'a, M>) -> Self {
        Element::new(s)
    }
}
