//! Keyboard focus inside the open panel, as DMS does it (every control reachable with Tab and
//! Shift+Tab, Enter or Space presses it, a ring shows which one): iced's own focus operations,
//! kept to the panel. iced's `focus_next` walks every surface, so Tab also landed on the bar's
//! buttons; these count only the controls inside the panel's `SCOPE` container.

use iced::advanced::widget::operation::{Focusable, Operation, Outcome};
use iced::advanced::widget::{Id, operation};
use iced::{Rectangle, Task};

/// The id of the container that holds an open panel's content.
pub const SCOPE: &str = "kestrel-panel";

/// Tracks whether a traversal is inside the scope: `container` names the next `traverse`.
#[derive(Default)]
struct Walk {
    next_is_scope: bool,
    inside: bool,
}

impl Walk {
    fn container(&mut self, id: Option<&Id>) {
        self.next_is_scope = id.is_some_and(|i| *i == Id::new(SCOPE));
    }
    fn enter(&mut self) -> bool {
        let was = self.inside;
        if self.next_is_scope {
            self.inside = true;
        }
        self.next_is_scope = false;
        was
    }
}

/// The panel's focusable controls: how many, and which one has focus.
#[derive(Clone, Copy, Default)]
pub struct Count {
    focused: Option<usize>,
    total: usize,
}

struct Counting {
    walk: Walk,
    count: Count,
}

impl Operation<Count> for Counting {
    fn container(&mut self, id: Option<&Id>, _bounds: Rectangle) {
        self.walk.container(id);
    }
    fn focusable(&mut self, _id: Option<&Id>, _bounds: Rectangle, state: &mut dyn Focusable) {
        if !self.walk.inside {
            return;
        }
        if state.is_focused() {
            self.count.focused = Some(self.count.total);
        }
        self.count.total += 1;
    }
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<Count>)) {
        let was = self.walk.enter();
        operate(self);
        self.walk.inside = was;
    }
    fn finish(&self) -> Outcome<Count> {
        Outcome::Some(self.count)
    }
}

/// Moves the focus to control `target` of the panel (unfocusing the one that had it).
struct Move {
    walk: Walk,
    target: usize,
    at: usize,
}

impl<T> Operation<T> for Move {
    fn container(&mut self, id: Option<&Id>, _bounds: Rectangle) {
        self.walk.container(id);
    }
    fn focusable(&mut self, _id: Option<&Id>, _bounds: Rectangle, state: &mut dyn Focusable) {
        if !self.walk.inside {
            return;
        }
        if self.at == self.target {
            state.focus();
        } else if state.is_focused() {
            state.unfocus();
        }
        self.at += 1;
    }
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<T>)) {
        let was = self.walk.enter();
        operate(self);
        self.walk.inside = was;
    }
}

fn counting() -> Counting {
    Counting {
        walk: Walk::default(),
        count: Count::default(),
    }
}

fn to(target: usize) -> Move {
    Move {
        walk: Walk::default(),
        target,
        at: 0,
    }
}

fn after(c: Count) -> Move {
    to(match c.focused {
        Some(i) if c.total > 0 => (i + 1) % c.total,
        _ => 0,
    })
}

fn before(c: Count) -> Move {
    to(match c.focused {
        Some(i) if c.total > 0 => (i + c.total - 1) % c.total,
        _ => c.total.saturating_sub(1),
    })
}

/// Tab: the panel's next control (the first when none has focus), wrapping.
pub fn next<M: Send + 'static>() -> Task<M> {
    Task::Iced(
        iced::widget::operate(operation::then::<Count, (), Move>(counting(), after)).discard(),
    )
}

/// Shift+Tab: the panel's previous control (the last when none has focus), wrapping.
pub fn previous<M: Send + 'static>() -> Task<M> {
    Task::Iced(
        iced::widget::operate(operation::then::<Count, (), Move>(counting(), before)).discard(),
    )
}
