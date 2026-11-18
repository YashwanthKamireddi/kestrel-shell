//! `Control`: the one interactive element. Every clickable thing in kestrel (bar buttons, tiles,
//! rows, menu items) is a `Control` around some content, so they all respond the same way:
//! the background fades in on hover, the whole control dips to 0.96 on press and springs back.

use crate::motion::Spring;
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::{Tree, Widget, tree};
use iced::advanced::{Clipboard, Shell, mouse};
use iced::event::Event;
use iced::{Border, Color, Element, Length, Padding, Rectangle, Size, Transformation, window};
use std::time::Instant;

/// How long a hover highlight takes to leave.
/// The state layer eases in and out (docs/DESIGN.md): quick enough to read as instant, slow
/// enough that a clear-at-rest button grows its shape instead of flashing it.
const HOVER_IN_SECS: f32 = crate::theme::tok::HOVER_IN;
const HOVER_OUT_SECS: f32 = crate::theme::tok::HOVER_OUT;
const PRESSED_SCALE: f32 = 0.96;

/// What kind of small pill button (`Control::pill`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pill {
    Plain,
    Quiet,
    Primary,
}

/// The state-layer styles every control is drawn in (docs/DESIGN.md): the container at rest,
/// with the same ink laid over it on hover and press everywhere.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Clear at rest (text and icon buttons, rows, chips that are off).
    Ghost,
    /// A faint ink fill at rest (secondary buttons, cards that click).
    Tonal,
    /// Chosen: the selected row, the chip that is on, the open place.
    Chosen,
    /// The accent fill (the main action, a choice that is on).
    Primary,
    /// The danger fill (a destructive action asked to confirm).
    Danger,
    /// No fill of its own: something else shows the selection (the launcher's highlight that
    /// moves under the rows, a picture that is its own button). Press still answers.
    Bare,
    /// No fill ever, not even pressed (a mark inside another control: the island's count);
    /// pair with `press_scale` for a tactile press.
    Still,
    /// A clear button on an accent fill (the icon at the start of a filled slider).
    OnAccent,
}

pub struct Control<'a, M> {
    content: Element<'a, M>,
    on_press: Option<M>,
    on_press_at: Option<Box<dyn Fn(Rectangle) -> M + 'a>>,
    on_right_press: Option<M>,
    /// Sent when the pointer comes onto it (lists: the pointer moves the selection).
    on_enter: Option<M>,
    on_exit: Option<M>,
    on_scroll: Option<Box<dyn Fn(f32) -> M + 'a>>,
    padding: Padding,
    width: Length,
    height: Length,
    radius: f32,
    /// Top and bottom corners apart (cards in a grouped list: round only at its ends).
    corners: Option<(f32, f32)>,
    rest: Color,
    hover: Color,
    press: Color,
    border: Option<Color>,
    press_scale: bool,
    center_x: bool,
    /// Bar style: drawn over the content at rest and lifted on hover, so the pointer brightens
    /// what is there instead of laying a grey block over it.
    veil: Color,
    /// What this control stands for in a list (a notification's id). iced keeps a widget's
    /// state by its place, so when one goes the next slides into its place with its state:
    /// a changed identity drops the hover it inherited, and the pointer resting there is
    /// heard entering the new one (closing a notification left the next, now under the
    /// pointer, showing its time instead of its close button).
    identity: Option<u64>,
    /// Its name for iced's focus (`widget::focus(id)` puts the keyboard here: a list's first
    /// item when it opens).
    id: Option<iced::widget::Id>,
}

pub fn control<'a, M>(content: impl Into<Element<'a, M>>) -> Control<'a, M> {
    Control {
        content: content.into(),
        on_press: None,
        on_press_at: None,
        on_right_press: None,
        on_enter: None,
        on_exit: None,
        on_scroll: None,
        padding: Padding::ZERO,
        width: Length::Shrink,
        height: Length::Shrink,
        radius: crate::theme::tok::R_CONTROL,
        corners: None,
        rest: Color::TRANSPARENT,
        hover: Color::TRANSPARENT,
        press: Color::TRANSPARENT,
        border: None,
        press_scale: false,
        center_x: false,
        veil: Color::TRANSPARENT,
        identity: None,
        id: None,
    }
}

impl<'a, M> Control<'a, M> {
    pub fn on_press(mut self, m: M) -> Self {
        self.on_press = Some(m);
        self
    }
    pub fn on_enter(mut self, m: M) -> Self {
        self.on_enter = Some(m);
        self
    }
    pub fn on_exit(mut self, m: M) -> Self {
        self.on_exit = Some(m);
        self
    }
    /// What it stands for in a list (see `identity`).
    pub fn identity(mut self, id: u64) -> Self {
        self.identity = Some(id);
        self
    }

    pub fn on_press_maybe(mut self, m: Option<M>) -> Self {
        self.on_press = m;
        self
    }
    /// Press handler that receives the control's bounds (to place a panel under it).
    pub fn on_press_at(mut self, f: impl Fn(Rectangle) -> M + 'a) -> Self {
        self.on_press_at = Some(Box::new(f));
        self
    }
    pub fn on_right_press(mut self, m: M) -> Self {
        self.on_right_press = Some(m);
        self
    }
    /// Scroll handler: receives lines scrolled, positive = up.
    pub fn on_scroll(mut self, f: impl Fn(f32) -> M + 'a) -> Self {
        self.on_scroll = Some(Box::new(f));
        self
    }
    pub fn padding(mut self, p: impl Into<Padding>) -> Self {
        self.padding = p.into();
        self
    }
    pub fn width(mut self, w: impl Into<Length>) -> Self {
        self.width = w.into();
        self
    }
    pub fn height(mut self, h: impl Into<Length>) -> Self {
        self.height = h.into();
        self
    }
    /// Different top and bottom corners (a card in a grouped list).
    pub fn corners(mut self, top: f32, bottom: f32) -> Self {
        self.corners = Some((top, bottom));
        self
    }

    fn shape(&self, h: f32) -> iced::border::Radius {
        match self.corners {
            Some((t, b)) => iced::border::Radius {
                top_left: t.min(h / 2.0),
                top_right: t.min(h / 2.0),
                bottom_right: b.min(h / 2.0),
                bottom_left: b.min(h / 2.0),
            },
            None => self.radius.min(h / 2.0).into(),
        }
    }

    pub fn radius(mut self, r: f32) -> Self {
        self.radius = r;
        self
    }
    /// Background at rest, on hover and while pressed.
    pub fn colors(mut self, rest: Color, hover: Color, press: Color) -> Self {
        self.rest = rest;
        self.hover = hover;
        self.press = press;
        self
    }
    /// A bar item: never a fill (his call 2026-11-13: grey highlights looked cheap). At rest
    /// the content sits a little dimmed under a veil of the bar's own black; under the pointer,
    /// and while its panel is open, the veil is gone and it shows at full white. A press
    /// changes nothing (dimming on press flickered as the panel opened).
    pub fn bar(mut self, s: crate::theme::Scheme, active: bool) -> Self {
        // Frosted: no film and no boxes at all (a dark film showed as a patch on the frost,
        // and light boxes looked random inside the pills, his call 2026-11-13).
        if crate::theme::glass() {
            self.rest = Color::TRANSPARENT;
            self.hover = Color::TRANSPARENT;
            self.press = Color::TRANSPARENT;
            self.veil = Color::TRANSPARENT;
            let _ = active;
            return self;
        }
        self.rest = Color::TRANSPARENT;
        self.hover = Color::TRANSPARENT;
        self.press = Color::TRANSPARENT;
        self.veil = if active {
            Color::TRANSPARENT
        } else {
            s.base.scale_alpha(0.16 * s.opacity)
        };
        self
    }
    /// The one small pill button, the same everywhere (26 px high, round ends): `Plain` a
    /// soft fill, `Quiet` only text until hovered, `Primary` the accent.
    pub fn pill(mut self, s: crate::theme::Scheme, kind: Pill) -> Self {
        self.padding = Padding::from([5, 12]);
        self.radius = 13.0;
        self.kind(
            s,
            match kind {
                Pill::Plain => Kind::Tonal,
                Pill::Quiet => Kind::Ghost,
                Pill::Primary => Kind::Primary,
            },
        )
    }
    /// Draws it in one of the kit's styles (rest, hover and press from the one state layer).
    pub fn kind(mut self, s: crate::theme::Scheme, k: Kind) -> Self {
        use crate::theme::tok::{HOVER, PRESS, SELECTED, TONAL};
        let ink = |x: f32| s.ink.scale_alpha(x * s.opacity);
        let over = |base: Color, x: f32| mix(base, s.on_accent, x);
        let (rest, hover, press) = match k {
            Kind::Ghost => (Color::TRANSPARENT, ink(HOVER), ink(PRESS)),
            Kind::Tonal => (ink(TONAL), ink(TONAL + HOVER), ink(TONAL + PRESS)),
            Kind::Chosen => (
                ink(SELECTED),
                ink(SELECTED + HOVER / 2.0),
                ink(SELECTED + PRESS / 2.0),
            ),
            Kind::Primary => (s.accent, over(s.accent, HOVER), over(s.accent, PRESS)),
            Kind::Danger => (s.bad, over(s.bad, HOVER), over(s.bad, PRESS)),
            Kind::Bare => (Color::TRANSPARENT, Color::TRANSPARENT, ink(PRESS)),
            Kind::Still => (Color::TRANSPARENT, Color::TRANSPARENT, Color::TRANSPARENT),
            Kind::OnAccent => (
                Color::TRANSPARENT,
                s.on_accent.scale_alpha(1.5 * HOVER),
                s.on_accent.scale_alpha(1.5 * PRESS),
            ),
        };
        self.rest = rest;
        self.hover = hover;
        self.press = press;
        self
    }
    /// Names it for iced's focus (see `id`).
    pub fn id(mut self, id: iced::widget::Id) -> Self {
        self.id = Some(id);
        self
    }
    pub fn border(mut self, c: Color) -> Self {
        self.border = Some(c);
        self
    }
    pub fn center_x(mut self) -> Self {
        self.center_x = true;
        self
    }
    /// Large surfaces (rows in a list) should not shrink on press.
    /// Shrinks a little while pressed (a tactile press with no fill flashing).
    pub fn press_scale(mut self) -> Self {
        self.press_scale = true;
        self
    }

    pub fn no_press_scale(mut self) -> Self {
        self.press_scale = false;
        self
    }
}

struct State {
    identity: Option<u64>,
    hovered: bool,
    pressed: bool,
    /// 0 = rest, 1 = hovered.
    hover_t: f32,
    scale: Spring,
    last: Option<Instant>,
    /// Has the keyboard: Tab or an arrow brought it here. It shows lit as under the pointer;
    /// a click never focuses it, and any click takes the focus away (GTK's focus-visible).
    focused: bool,
}

impl iced::advanced::widget::operation::Focusable for State {
    fn is_focused(&self) -> bool {
        self.focused
    }
    fn focus(&mut self) {
        self.focused = true;
    }
    fn unfocus(&mut self) {
        self.focused = false;
    }
}

impl State {
    fn animating(&self) -> bool {
        let want = if self.hovered { 1.0 } else { 0.0 };
        (self.hover_t - want).abs() > 1e-3 || !self.scale.settled()
    }
}

fn mix(a: Color, b: Color, t: f32) -> Color {
    Color {
        r: a.r + (b.r - a.r) * t,
        g: a.g + (b.g - a.g) * t,
        b: a.b + (b.b - a.b) * t,
        a: a.a + (b.a - a.a) * t,
    }
}

/// A transparent colour takes the other colour's RGB so fades don't pass through black.
fn fade_pair(a: Color, b: Color) -> (Color, Color) {
    match (a.a == 0.0, b.a == 0.0) {
        (true, false) => (Color { a: 0.0, ..b }, b),
        (false, true) => (a, Color { a: 0.0, ..a }),
        _ => (a, b),
    }
}

impl<M: Clone> Widget<M, iced::Theme, iced::Renderer> for Control<'_, M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State {
            identity: self.identity,
            hovered: false,
            pressed: false,
            hover_t: 0.0,
            scale: Spring::new(1.0),
            last: None,
            focused: false,
        })
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }

    fn size(&self) -> Size<Length> {
        Size::new(self.width, self.height)
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        // Content is laid out at its natural height (rows and columns would otherwise
        // stretch to a fixed control height and pin their children to the top).
        let node = layout::padded(limits, self.width, self.height, self.padding, |l| {
            let natural = layout::Limits::new(Size::ZERO, Size::new(l.max().width, f32::INFINITY));
            self.content
                .as_widget_mut()
                .layout(&mut tree.children[0], renderer, &natural)
        });
        // Content is centred vertically (and horizontally when asked) inside the padding.
        let size = node.size();
        let child = node.children()[0].clone();
        let inner_h = size.height - self.padding.top - self.padding.bottom;
        let inner_w = size.width - self.padding.left - self.padding.right;
        let x = if self.center_x {
            self.padding.left + (inner_w - child.size().width) / 2.0
        } else {
            self.padding.left
        };
        let y = self.padding.top + (inner_h - child.size().height) / 2.0;
        layout::Node::with_children(size, vec![child.move_to((x.max(0.0), y.max(0.0)))])
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
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout.children().next().unwrap(),
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
        let bounds = layout.bounds();
        let pressable = self.on_press.is_some() || self.on_press_at.is_some();
        // Hover is tracked for anything that acts on it or asks to hear it (a notification card
        // with no default action still swaps its time for a close button under the pointer).
        let interactive = pressable
            || self.on_right_press.is_some()
            || self.on_scroll.is_some()
            || self.on_enter.is_some()
            || self.on_exit.is_some();
        let state = tree.state.downcast_mut::<State>();
        let over = interactive && cursor.is_over(bounds);

        // Any click takes the keyboard focus away (it is the keyboard's place only).
        if state.focused && matches!(event, Event::Mouse(mouse::Event::ButtonPressed(_))) {
            state.focused = false;
            shell.request_redraw();
        }
        // With the keyboard: Enter or Space presses it, as a click would.
        if state.focused
            && pressable
            && !shell.is_event_captured()
            && let Event::Keyboard(iced::keyboard::Event::KeyPressed {
                key: iced::keyboard::Key::Named(k),
                modifiers,
                ..
            }) = event
            && matches!(
                k,
                iced::keyboard::key::Named::Enter | iced::keyboard::key::Named::Space
            )
            && modifiers.is_empty()
        {
            if let Some(f) = &self.on_press_at {
                shell.publish(f(bounds));
            } else if let Some(m) = &self.on_press {
                shell.publish(m.clone());
            }
            shell.capture_event();
        }
        if !shell.is_event_captured() {
            match event {
                // Acts on press, not release: the shell answers the moment the button goes
                // down (as menus do), instead of a click's length later.
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) if over => {
                    if pressable {
                        state.pressed = true;
                        shell.request_redraw();
                        if self.press_scale {
                            state.scale.to(PRESSED_SCALE, Spring::CLOSE);
                        }
                        if let Some(f) = &self.on_press_at {
                            shell.publish(f(bounds));
                        } else if let Some(m) = &self.on_press {
                            shell.publish(m.clone());
                        }
                        shell.capture_event();
                    }
                }
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) if over => {
                    if let Some(m) = &self.on_right_press {
                        shell.publish(m.clone());
                        shell.capture_event();
                    }
                }
                Event::Mouse(mouse::Event::WheelScrolled { delta }) if over => {
                    if let Some(f) = &self.on_scroll {
                        let lines = match delta {
                            mouse::ScrollDelta::Lines { y, .. } => *y,
                            mouse::ScrollDelta::Pixels { y, .. } => *y / 40.0,
                        };
                        if lines != 0.0 {
                            shell.publish(f(lines));
                            shell.capture_event();
                        }
                    }
                }
                _ => {}
            }
        }

        // The pressed look ends with the button, wherever the release lands. Seen by every
        // control, captured or not, and never captured here: each pressed control used to take
        // the release for itself, so one left pressed (its release landed after the view
        // changed) kept the next one from ever seeing its own, and every click left another
        // item lit. A press anywhere else also ends it (the button must have come up).
        let up = matches!(
            event,
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
        ) || (matches!(
            event,
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
        ) && !over)
            || matches!(event, Event::Mouse(mouse::Event::CursorLeft));
        if up && state.pressed {
            state.pressed = false;
            state.scale.to(1.0, Spring::CLOSE);
            // Redrawn now: without this the pressed look stayed on screen until something
            // else redrew, seconds later.
            shell.request_redraw();
        }

        // Another item now in this place: it starts unhovered, so the pointer already over it
        // is heard entering it.
        if state.identity != self.identity {
            state.identity = self.identity;
            state.hovered = false;
            state.pressed = false;
            state.hover_t = 0.0;
        }
        if state.hovered != over {
            state.hovered = over;
            // The redraw below starts the ease (every surface a control can be on redraws while
            // it shows anything: `redraw_key` is None for the bar and a busy overlay).
            if over && let Some(m) = &self.on_enter {
                shell.publish(m.clone());
            }
            if !over && let Some(m) = &self.on_exit {
                shell.publish(m.clone());
            }
            if !over && state.pressed {
                state.scale.to(1.0, Spring::SNAP);
            }
            shell.request_redraw();
        }

        if let Event::Window(window::Event::RedrawRequested(now)) = event {
            let dt = state.last.map_or(1.0 / 60.0, |t| {
                now.saturating_duration_since(t).as_secs_f32()
            });
            state.last = Some(*now);
            let want = if state.hovered { 1.0 } else { 0.0 };
            // In over 70 ms, out over 120 ms (eased in `draw`). Game mode: no fade at all.
            state.hover_t = if crate::motion::still() {
                want
            } else if state.hover_t < want {
                (state.hover_t + dt / HOVER_IN_SECS).min(want)
            } else {
                (state.hover_t - dt / HOVER_OUT_SECS).max(want)
            };
            state.scale.step(dt);
            if state.animating() {
                shell.request_redraw();
            } else {
                state.last = None;
            }
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
        let state = tree.state.downcast_ref::<State>();
        let bounds = layout.bounds();
        // Only what can be pressed lifts under the pointer.
        let lifts = self.on_press.is_some() || self.on_press_at.is_some();
        let (rest, hover) = fade_pair(self.rest, if lifts { self.hover } else { self.rest });
        // The keyboard's place: a fill one step above hover (the pressed tone), shown at once:
        // keyboard moves never animate (Raycast), and keyboard selection must read at a glance
        // even on a small button (Material's focus layer sits above hover). The pointer's
        // hover keeps its ease.
        let keyboard = state.focused && lifts;
        let lit = if keyboard { 1.0 } else { ease(state.hover_t) };
        let mut bg = if keyboard {
            self.press
        } else {
            mix(rest, hover, lit)
        };
        if state.pressed && state.hovered {
            bg = self.press;
        }
        let s = state.scale.value;
        let center = bounds.center();
        let draw_all = |renderer: &mut iced::Renderer| {
            if bg.a > 0.0 || self.border.is_some() {
                renderer.fill_quad(
                    renderer::Quad {
                        bounds,
                        border: Border {
                            radius: self.shape(bounds.height),
                            width: if self.border.is_some() { 1.0 } else { 0.0 },
                            color: self.border.unwrap_or(Color::TRANSPARENT),
                        },
                        ..renderer::Quad::default()
                    },
                    bg,
                );
            }
            // Content never paints outside the control: long text or a big image is cut at
            // the control's edge instead of spilling over its neighbours.
            let clip = bounds.intersection(viewport).unwrap_or(bounds);
            renderer.with_layer(clip, |renderer| {
                self.content.as_widget().draw(
                    &tree.children[0],
                    renderer,
                    theme,
                    style,
                    layout.children().next().unwrap(),
                    cursor,
                    &clip,
                );
            });
            let veil = if state.pressed && state.hovered {
                0.0
            } else {
                self.veil.a * (1.0 - lit)
            };
            // Its own layer, so it lies over the content's layer rather than under it.
            if veil > 0.0 {
                renderer.with_layer(clip, |renderer| {
                    renderer.fill_quad(
                        renderer::Quad {
                            bounds,
                            border: Border::default().rounded(self.shape(bounds.height)),
                            ..renderer::Quad::default()
                        },
                        Color {
                            a: veil,
                            ..self.veil
                        },
                    );
                });
            }
        };
        if (s - 1.0).abs() > 1e-3 {
            let t = Transformation::translate(center.x, center.y)
                * Transformation::scale(s)
                * Transformation::translate(-center.x, -center.y);
            renderer.with_transformation(t, draw_all);
        } else {
            draw_all(renderer);
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        let inner = self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout.children().next().unwrap(),
            cursor,
            viewport,
            renderer,
        );
        if inner != mouse::Interaction::default() {
            inner
        } else if (self.on_press.is_some() || self.on_press_at.is_some())
            && cursor.is_over(layout.bounds())
        {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::default()
        }
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn iced::advanced::widget::Operation,
    ) {
        // Only what can be pressed takes the keyboard (Tab passes over labels and cards).
        if self.on_press.is_some() || self.on_press_at.is_some() {
            let state = tree.state.downcast_mut::<State>();
            operation.focusable(self.id.as_ref(), layout.bounds(), state);
        }
        self.content.as_widget_mut().operate(
            &mut tree.children[0],
            layout.children().next().unwrap(),
            renderer,
            operation,
        );
    }
}

fn ease(t: f32) -> f32 {
    crate::motion::ease_out_cubic(t)
}

impl<'a, M: Clone + 'a> From<Control<'a, M>> for Element<'a, M> {
    fn from(c: Control<'a, M>) -> Self {
        Element::new(c)
    }
}
