//! The keyboard focus moved by direction (the arrow keys): from the control with the ring to
//! the nearest one that way, preferring the ones in line with it, as GTK moves focus within a
//! group. Built from iced's own operations: one pass reads where every focusable control is
//! and which has the focus, the next focuses the one chosen. Scoped by the caller to a
//! panel's or window's root (`operation::scope`), like Tab.

use iced::Rectangle;
use iced::advanced::widget::operation::{Focusable, Operation, Outcome, then};
use iced::widget::Id;

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
}
