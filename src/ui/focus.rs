//! The keyboard focus moved by direction (the arrow keys): from the control with the ring to
//! the nearest one that way, preferring the ones in line with it, as GTK moves focus within a
//! group. Built from iced's own operations: one pass reads where every focusable control is
//! and which has the focus, the next focuses the one chosen. Scoped by the caller to a
//! panel's or window's root (`operation::scope`), like Tab.
//!
//! `keep` holds the keyboard's place when the control that had it goes (a row's ✕ pressed with
//! Enter removed the row): the same kind of control nearest to where it was takes the focus,
//! which is the next row slid into its place, or the one before when the last went.

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::operation::{Focusable, Operation, Outcome, then};
use iced::advanced::widget::{Tree, Widget, tree};
use iced::advanced::{Clipboard, Shell, mouse};
use iced::event::Event;
use iced::widget::Id;
use iced::{Element, Length, Rectangle, Size};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    Up,
    Down,
    Left,
    Right,
}

/// Where the focusable controls are (in order) and which has the focus.
type Found = (Dir, Vec<Rectangle>, Option<usize>);

struct Scan {
    dir: Dir,
    rects: Vec<Rectangle>,
    focused: Option<usize>,
}

impl Operation<Found> for Scan {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<Found>)) {
        operate(self);
    }

    fn focusable(&mut self, _id: Option<&Id>, bounds: Rectangle, state: &mut dyn Focusable) {
        if state.is_focused() {
            self.focused = Some(self.rects.len());
        }
        self.rects.push(bounds);
    }

    fn finish(&self) -> Outcome<Found> {
        Outcome::Some((self.dir, self.rects.clone(), self.focused))
    }
}

struct Pick {
    target: Option<usize>,
    current: usize,
}

impl<T> Operation<T> for Pick {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<T>)) {
        operate(self);
    }

    fn focusable(&mut self, _id: Option<&Id>, _bounds: Rectangle, state: &mut dyn Focusable) {
        if let Some(t) = self.target {
            if self.current == t {
                state.focus();
            } else if state.is_focused() {
                state.unfocus();
            }
        }
        self.current += 1;
    }
}

/// The control to move to from `focused` going `dir`: in that direction, the nearest along
/// it, those overlapping in line first. None when there is none that way (the focus stays).
/// With nothing focused yet, the first control.
pub fn choose(dir: Dir, rects: &[Rectangle], focused: Option<usize>) -> Option<usize> {
    let Some(f) = focused else {
        return (!rects.is_empty()).then_some(0);
    };
    let from = rects.get(f)?;
    let c = from.center();
    rects
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != f)
        .filter_map(|(i, r)| {
            let rc = r.center();
            // Distance along the direction (edge to edge) and across it (centre to centre);
            // `overlap`: in line with the focused one across the direction.
            let (along, across, overlap) = match dir {
                Dir::Down => (
                    r.y - (from.y + from.height),
                    (rc.x - c.x).abs(),
                    r.x < from.x + from.width && from.x < r.x + r.width,
                ),
                Dir::Up => (
                    from.y - (r.y + r.height),
                    (rc.x - c.x).abs(),
                    r.x < from.x + from.width && from.x < r.x + r.width,
                ),
                Dir::Right => (
                    r.x - (from.x + from.width),
                    (rc.y - c.y).abs(),
                    r.y < from.y + from.height && from.y < r.y + r.height,
                ),
                Dir::Left => (
                    from.x - (r.x + r.width),
                    (rc.y - c.y).abs(),
                    r.y < from.y + from.height && from.y < r.y + r.height,
                ),
            };
            // That way at all (a little overlap allowed: rows touch).
            let ahead = match dir {
                Dir::Down => rc.y > c.y + 1.0,
                Dir::Up => rc.y < c.y - 1.0,
                Dir::Right => rc.x > c.x + 1.0,
                Dir::Left => rc.x < c.x - 1.0,
            };
            ahead.then_some((i, overlap, along.max(0.0), across))
        })
        .min_by(|a, b| {
            // In line first; then nearest along; then nearest across.
            b.1.cmp(&a.1)
                .then(a.2.total_cmp(&b.2))
                .then(a.3.total_cmp(&b.3))
        })
        .map(|(i, ..)| i)
}

/// Of the controls `rects`, the one to take the keyboard when the control at `at` that had it
/// went: the same size first (a list's next or previous row, another row's ✕), then those in
/// line with it up and down, then the nearest. None when there is none at all.
pub fn nearest(at: Rectangle, rects: &[Rectangle]) -> Option<usize> {
    let same =
        |r: &Rectangle| (r.width - at.width).abs() < 1.0 && (r.height - at.height).abs() < 1.0;
    let in_line = |r: &Rectangle| r.x < at.x + at.width && at.x < r.x + r.width;
    let c = at.center();
    let dist = |r: &Rectangle| {
        let rc = r.center();
        (rc.x - c.x).hypot(rc.y - c.y)
    };
    rects
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| {
            same(b)
                .cmp(&same(a))
                .then(in_line(b).cmp(&in_line(a)))
                .then(dist(a).total_cmp(&dist(b)))
        })
        .map(|(i, _)| i)
}

/// Where the focusable controls are and which has the focus, read over a widget's content.
#[derive(Default)]
struct Look {
    rects: Vec<Rectangle>,
    focused: Option<usize>,
}

impl Operation for Look {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }

    fn focusable(&mut self, _id: Option<&Id>, bounds: Rectangle, state: &mut dyn Focusable) {
        if state.is_focused() {
            self.focused = Some(self.rects.len());
        }
        self.rects.push(bounds);
    }
}

/// `content` keeping the keyboard's place (see the module's note). `key` names what it shows
/// (a panel and its page): when that changes the place is forgotten, so a new page never
/// starts with a control lit that the keyboard did not choose.
pub fn keep<'a, M>(content: impl Into<Element<'a, M>>, key: u64) -> Keep<'a, M> {
    Keep {
        content: content.into(),
        key,
    }
}

pub struct Keep<'a, M> {
    content: Element<'a, M>,
    key: u64,
}

struct KeepState {
    key: u64,
    /// Where the control with the keyboard was at the last frame; None when the keyboard has
    /// no place here (never used, or a click took it away).
    place: Option<Rectangle>,
}

impl<M> Widget<M, iced::Theme, iced::Renderer> for Keep<'_, M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<KeepState>()
    }
    fn state(&self) -> tree::State {
        tree::State::new(KeepState {
            key: self.key,
            place: None,
        })
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
    fn size_hint(&self) -> Size<Length> {
        self.content.as_widget().size_hint()
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
        // A click takes the keyboard's place away (each control drops its focus on a click).
        if matches!(event, Event::Mouse(mouse::Event::ButtonPressed(_))) {
            tree.state.downcast_mut::<KeepState>().place = None;
        }
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            child,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
        // Each frame, before it is drawn: where the keyboard is; or, when its control went,
        // the focus given to the one taking its place (drawn in this same frame).
        if !matches!(
            event,
            Event::Window(iced::window::Event::RedrawRequested(_))
        ) {
            return;
        }
        let key = self.key;
        let st = tree.state.downcast_mut::<KeepState>();
        if st.key != key {
            st.key = key;
            st.place = None;
        }
        let mut look = Look::default();
        self.content
            .as_widget_mut()
            .operate(&mut tree.children[0], child, renderer, &mut look);
        let st = tree.state.downcast_mut::<KeepState>();
        match (look.focused, st.place) {
            (Some(i), _) => st.place = look.rects.get(i).copied(),
            (None, Some(at)) => {
                let target = nearest(at, &look.rects);
                st.place = target.map(|i| look.rects[i]);
                if target.is_some() {
                    let mut pick = Pick { target, current: 0 };
                    self.content.as_widget_mut().operate(
                        &mut tree.children[0],
                        child,
                        renderer,
                        &mut pick,
                    );
                }
            }
            (None, None) => {}
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
        operation: &mut dyn Operation,
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

impl<'a, M: 'a> From<Keep<'a, M>> for Element<'a, M> {
    fn from(k: Keep<'a, M>) -> Self {
        Element::new(k)
    }
}

/// The control that starts a row on screen: the leftmost (the topmost of those).
pub fn leftmost(rects: &[Rectangle]) -> Option<usize> {
    rects
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)))
        .map(|(i, _)| i)
}

/// The operation: the leftmost control takes the focus (the bar's first button), whatever had
/// it before. Scope it to a root.
pub fn first() -> impl Operation<()> {
    then(
        Scan {
            dir: Dir::Right,
            rects: Vec::new(),
            focused: None,
        },
        |(_, rects, _)| Pick {
            target: leftmost(&rects),
            current: 0,
        },
    )
}

/// The operation: focus moves `dir` (scope it to a root with `operation::scope`).
pub fn toward(dir: Dir) -> impl Operation<()> {
    then(
        Scan {
            dir,
            rects: Vec::new(),
            focused: None,
        },
        |(dir, rects, focused)| Pick {
            target: choose(dir, &rects, focused),
            current: 0,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(x: f32, y: f32) -> Rectangle {
        Rectangle::new(iced::Point::new(x, y), iced::Size::new(100.0, 40.0))
    }

    /// A grid of tiles, two a row (the control centre's): arrows move as the eye expects.
    #[test]
    fn moves_in_a_grid() {
        let g = [
            r(0.0, 0.0),
            r(110.0, 0.0),
            r(0.0, 50.0),
            r(110.0, 50.0),
            r(0.0, 100.0),
        ];
        assert_eq!(choose(Dir::Right, &g, Some(0)), Some(1));
        assert_eq!(choose(Dir::Down, &g, Some(1)), Some(3));
        assert_eq!(choose(Dir::Left, &g, Some(3)), Some(2));
        assert_eq!(
            choose(Dir::Down, &g, Some(3)),
            Some(4),
            "the last row's only tile"
        );
        assert_eq!(
            choose(Dir::Up, &g, Some(0)),
            None,
            "nothing above: it stays"
        );
        assert_eq!(
            choose(Dir::Down, &g, None),
            Some(0),
            "nothing focused: the first"
        );
    }

    /// A list with a small button at a row's end (a notification's ✕): Down from the row goes
    /// to the next row, not to the far button.
    #[test]
    fn prefers_what_is_in_line() {
        let rows = [
            Rectangle::new(iced::Point::new(0.0, 0.0), iced::Size::new(300.0, 40.0)),
            Rectangle::new(iced::Point::new(310.0, 60.0), iced::Size::new(20.0, 20.0)),
            Rectangle::new(iced::Point::new(0.0, 100.0), iced::Size::new(300.0, 40.0)),
        ];
        assert_eq!(choose(Dir::Down, &rows, Some(0)), Some(2));
    }

    /// A list of `n` cards, each with a ✕ at its top right, and a "Clear all" button under
    /// them: card i at 2i, its ✕ at 2i + 1, the button last.
    fn cards(n: usize) -> Vec<Rectangle> {
        let mut v = Vec::new();
        for i in 0..n {
            let y = i as f32 * 90.0;
            v.push(Rectangle::new(
                iced::Point::new(0.0, y),
                iced::Size::new(300.0, 80.0),
            ));
            v.push(Rectangle::new(
                iced::Point::new(270.0, y + 8.0),
                iced::Size::new(22.0, 22.0),
            ));
        }
        v.push(Rectangle::new(
            iced::Point::new(200.0, n as f32 * 90.0),
            iced::Size::new(100.0, 30.0),
        ));
        v
    }

    /// The second card's ✕ pressed: the third card slid into its place, and its ✕ takes the
    /// keyboard.
    #[test]
    fn the_next_takes_the_place() {
        let gone = cards(3)[3];
        assert_eq!(nearest(gone, &cards(2)), Some(3), "the next card's ✕");
    }

    /// The last card's ✕ pressed: the one before it takes the keyboard, not "Clear all",
    /// which moved up into its place.
    #[test]
    fn the_previous_when_the_last_went() {
        let before = cards(3);
        let after = cards(2);
        assert_eq!(nearest(before[5], &after), Some(3), "the previous card's ✕");
        // A whole row the same: the row before.
        assert_eq!(nearest(before[4], &after), Some(2));
    }

    /// A list as the panels draw it: each item a card with a ✕, every second one with a
    /// button under its text too (so items differ in shape, as notifications do and iced's
    /// state by place cannot carry the focus over).
    fn list(items: &[u32]) -> Element<'static, u32> {
        use crate::ui::control;
        use iced::widget::{column, row, text};
        let mut col = column![].spacing(8);
        for &i in items {
            let close = control(text("x")).padding(4).on_press(i);
            let head = row![text(format!("item {i}")).width(Length::Fill), close];
            let card: Element<'static, u32> = if i % 2 == 0 {
                column![head, control(text("Reply")).on_press(100 + i)].into()
            } else {
                head.into()
            };
            col = col.push(card);
        }
        keep(col.width(Length::Fixed(300.0)), 0).into()
    }

    /// The list with `items`, built on the state of the last (`cache`), given one frame:
    /// where the keyboard is after it (the focused control's bounds and every control's).
    fn frame(
        items: &[u32],
        cache: iced_runtime::user_interface::Cache,
        focus: Option<usize>,
    ) -> (
        Option<Rectangle>,
        Vec<Rectangle>,
        iced_runtime::user_interface::Cache,
    ) {
        let mut r = iced::Renderer::Secondary(iced_tiny_skia::Renderer::new(
            crate::theme::FONT,
            iced::Pixels(14.0),
        ));
        let mut ui = iced_runtime::UserInterface::build(
            list(items),
            Size::new(400.0, 1000.0),
            cache,
            &mut r,
        );
        if focus.is_some() {
            ui.operate(
                &r,
                &mut Pick {
                    target: focus,
                    current: 0,
                },
            );
        }
        let mut msgs = Vec::new();
        let _ = ui.update(
            &[Event::Window(iced::window::Event::RedrawRequested(
                std::time::Instant::now(),
            ))],
            mouse::Cursor::Unavailable,
            &mut r,
            &mut iced::advanced::clipboard::Null,
            &mut msgs,
        );
        let mut look = Look::default();
        ui.operate(&r, &mut look);
        let at = look.focused.map(|i| look.rects[i]);
        (at, look.rects, ui.into_cache())
    }

    /// Enter on a ✕ removes its item: the keyboard goes to the next item's ✕, or the one
    /// before when it was the last, through the real widgets.
    #[test]
    fn keep_moves_the_keyboard_on() {
        // Controls in order: 1 ✕; 2 ✕, 2 Reply; 3 ✕; 4 ✕, 4 Reply.
        let (_, rects, cache) = frame(&[1, 2, 3, 4], Default::default(), None);
        // The keyboard on item 2's ✕; item 2 goes.
        let (at, _, cache) = frame(&[1, 2, 3, 4], cache, Some(1));
        assert_eq!(at, Some(rects[1]));
        let (at, after, cache) = frame(&[1, 3, 4], cache, None);
        assert_eq!(at, Some(after[1]), "item 3's ✕, now second");
        // Item 3 goes (focused), then item 4, the last: the one before takes it.
        let (_, _, cache) = frame(&[1, 4], cache, None);
        let (at, after, cache) = frame(&[1, 4], cache, Some(1));
        assert_eq!(at, Some(after[1]));
        let (at, after, _) = frame(&[1], cache, None);
        assert_eq!(at, Some(after[0]), "item 1's ✕");
    }

    #[test]
    fn nothing_left_nothing_taken() {
        assert_eq!(nearest(r(0.0, 0.0), &[]), None);
    }
}
