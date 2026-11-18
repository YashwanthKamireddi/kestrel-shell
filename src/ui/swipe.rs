//! `Swipe`: a card that is swiped sideways to dismiss it, as Android's notifications and DMS's
//! (`Modules/Notifications/NotificationSwipeRow.qml`): it follows a held drag or a two-finger
//! sideways scroll; let go past 35 % of its width (or flung that far) it slides out and
//! `on_dismiss` is sent at once (the list closes up on that frame: a slide-out first left an
//! empty slot that read as lag), short of that it springs back. A click that does not move is a tap
//! (`on_tap`, on release so a drag never taps). Buttons inside keep their own clicks.

use crate::motion::Spring;
use crate::theme::tok;
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::{Tree, Widget, tree};
use iced::advanced::{Clipboard, Shell, mouse};
use iced::event::Event;
use iced::{Element, Length, Rectangle, Size, Vector, window};
use std::time::Instant;

/// How far (share of the width) a release must carry the card to dismiss it (DMS 0.35).
const THRESHOLD: f32 = 0.35;
/// Movement before a press becomes a drag rather than a tap.
const SLOP: f32 = 6.0;

pub struct Swipe<'a, M> {
    content: Element<'a, M>,
    identity: Option<u64>,
    on_tap: Option<M>,
    on_dismiss: Option<M>,
}

pub fn swipe<'a, M>(content: impl Into<Element<'a, M>>) -> Swipe<'a, M> {
    Swipe {
        content: content.into(),
        identity: None,
        on_tap: None,
        on_dismiss: None,
    }
}

impl<M> Swipe<'_, M> {
    /// What this card shows (a notification's id): another one in its place starts at rest.
    pub fn identity(mut self, id: u64) -> Self {
        self.identity = Some(id);
        self
    }
    pub fn on_tap_maybe(mut self, m: Option<M>) -> Self {
        self.on_tap = m;
        self
    }
    pub fn on_dismiss(mut self, m: M) -> Self {
        self.on_dismiss = Some(m);
        self
    }
}

enum Grab {
    None,
    /// Pressed at x, not yet moved past the slop.
    Pressed(f32),
    /// Dragging: the press x and the offset when it began.
    Dragging(f32),
    /// Following a sideways scroll; when the last one came.
    Wheel(Instant),
}

struct State {
    identity: Option<u64>,
    offset: Spring,
    grab: Grab,
    /// Recent movement speed (px/s), for flings.
    speed: f32,
    moved_at: Option<(Instant, f32)>,
    leaving: bool,
    last: Option<Instant>,
}

impl State {
    fn new(identity: Option<u64>) -> Self {
        Self {
            identity,
            offset: Spring::new(0.0),
            grab: Grab::None,
            speed: 0.0,
            moved_at: None,
            leaving: false,
            last: None,
        }
    }

    fn track(&mut self, x: f32) {
        let now = Instant::now();
        if let Some((t, px)) = self.moved_at {
            let dt = now.saturating_duration_since(t).as_secs_f32();
            if dt > 0.0 {
                self.speed = (x - px) / dt;
            }
        }
        self.moved_at = Some((now, x));
    }

    /// Let go: past the threshold (with its fling) it goes, and true is returned; else back.
    fn release(&mut self, width: f32) -> bool {
        self.grab = Grab::None;
        self.last = None;
        let projected = self.offset.value + self.speed * tok::FLING;
        self.speed = 0.0;
        self.moved_at = None;
        if projected.abs() > width * THRESHOLD {
            self.leaving = true;
            true
        } else {
            self.offset.to(0.0, tok::SPRING_SNAP);
            false
        }
    }
}

impl<M: Clone> Widget<M, iced::Theme, iced::Renderer> for Swipe<'_, M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }
    fn state(&self) -> tree::State {
        tree::State::new(State::new(self.identity))
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
        let bounds = layout.bounds();
        let width = bounds.width.max(1.0);
        {
            let state = tree.state.downcast_mut::<State>();
            if state.identity != self.identity {
                *state = State::new(self.identity);
            }
        }
        let dx = tree.state.downcast_ref::<State>().offset.value;
        let moved = dx.abs() > 0.5;
        // The content sees the pointer where it is drawn; while the card moves its buttons
        // do not take clicks.
        let inner = if moved {
            mouse::Cursor::Unavailable
        } else {
            cursor
        };
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout.children().next().unwrap(),
            inner,
            renderer,
            clipboard,
            shell,
            viewport,
        );
        let captured = shell.is_event_captured();
        let state = tree.state.downcast_mut::<State>();
        // Gone: it waits, where it was let go, for the list to drop it.
        if state.leaving {
            return;
        }
        let over = cursor.is_over(bounds);
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) if over && !captured => {
                if let Some(p) = cursor.position() {
                    state.grab = Grab::Pressed(p.x);
                    state.track(p.x);
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) => match state.grab {
                Grab::Pressed(x0) if (position.x - x0).abs() > SLOP => {
                    state.grab = Grab::Dragging(x0);
                    state.offset.snap(position.x - x0);
                    state.track(position.x);
                    shell.request_redraw();
                }
                Grab::Dragging(x0) => {
                    state.offset.snap(position.x - x0);
                    state.track(position.x);
                    shell.request_redraw();
                }
                _ => {}
            },
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => match state.grab {
                Grab::Pressed(_) => {
                    state.grab = Grab::None;
                    if over && let Some(m) = &self.on_tap {
                        shell.publish(m.clone());
                    }
                }
                Grab::Dragging(_) => {
                    if state.release(width)
                        && let Some(m) = &self.on_dismiss
                    {
                        shell.publish(m.clone());
                    }
                    shell.request_redraw();
                    shell.capture_event();
                }
                _ => {}
            },
            Event::Mouse(mouse::Event::CursorLeft) => {
                if matches!(state.grab, Grab::Dragging(_)) {
                    if state.release(width)
                        && let Some(m) = &self.on_dismiss
                    {
                        shell.publish(m.clone());
                    }
                    shell.request_redraw();
                } else {
                    state.grab = Grab::None;
                }
            }
            // A two-finger sideways swipe on the touchpad (mostly sideways, so a list still
            // scrolls under a vertical one).
            Event::Mouse(mouse::Event::WheelScrolled {
                delta: mouse::ScrollDelta::Pixels { x, y },
            }) if over
                && !captured
                && (matches!(state.grab, Grab::Wheel(_))
                    || (x.abs() > y.abs() * 1.5 && x.abs() > 0.5)) =>
            {
                let now = Instant::now();
                let v = state.offset.value - x;
                state.offset.snap(v);
                state.track(v);
                state.grab = Grab::Wheel(now);
                shell.capture_event();
                shell.request_redraw();
            }
            Event::Window(window::Event::RedrawRequested(now)) => {
                if let Grab::Wheel(t) = state.grab
                    && now.saturating_duration_since(t).as_secs_f32() > tok::WHEEL_IDLE
                    && state.release(width)
                    && let Some(m) = &self.on_dismiss
                {
                    shell.publish(m.clone());
                }
                step(state, *now);
                if !state.offset.settled() || matches!(state.grab, Grab::Wheel(_)) {
                    shell.request_redraw();
                }
            }
            _ => {}
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
        let dx = tree.state.downcast_ref::<State>().offset.value;
        if dx.abs() < 0.5 {
            self.content.as_widget().draw(
                &tree.children[0],
                renderer,
                theme,
                style,
                layout.children().next().unwrap(),
                cursor,
                viewport,
            );
            return;
        }
        // Cut at its own slot: the card slides out of the list, never across the screen.
        let Some(clip) = layout.bounds().intersection(viewport) else {
            return;
        };
        // Dragged wholly out of its slot it shows nothing (drawn, the controls inside found no
        // visible part and stopped clipping, so the card was painted across the screen).
        if dx.abs() >= layout.bounds().width {
            return;
        }
        renderer.with_layer(clip, |renderer| {
            renderer.with_translation(Vector::new(dx.round(), 0.0), |renderer| {
                self.content.as_widget().draw(
                    &tree.children[0],
                    renderer,
                    theme,
                    style,
                    layout.children().next().unwrap(),
                    mouse::Cursor::Unavailable,
                    &Rectangle {
                        x: clip.x - dx,
                        ..clip
                    },
                );
            });
        });
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        let state = tree.state.downcast_ref::<State>();
        if matches!(state.grab, Grab::Dragging(_)) {
            return mouse::Interaction::Grabbing;
        }
        let i = self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout.children().next().unwrap(),
            cursor,
            viewport,
            renderer,
        );
        if i == mouse::Interaction::None && self.on_tap.is_some() && cursor.is_over(layout.bounds())
        {
            mouse::Interaction::Pointer
        } else {
            i
        }
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
}

fn step(state: &mut State, now: Instant) {
    let dt = state.last.map_or(1.0 / 60.0, |t| {
        now.saturating_duration_since(t).as_secs_f32()
    });
    state.last = Some(now);
    state.offset.step(dt);
}

impl<'a, M: Clone + 'a> From<Swipe<'a, M>> for Element<'a, M> {
    fn from(s: Swipe<'a, M>) -> Self {
        Element::new(s)
    }
}
