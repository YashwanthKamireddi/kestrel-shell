//! The bar from the keyboard (`kestrel focus bar`), as GNOME's Ctrl+Alt+Tab and KDE's
//! "Activate panel" do it: the bar takes the keyboard with its first button lit, the arrows
//! (and Tab) move along it, Enter or Space presses the one lit (a panel opens and takes the
//! keyboard), Esc gives the keyboard back to the app in use. The bar's buttons are the kit's
//! controls, so this is the same focus as in the panels: `ui::focus`, scoped to the bar.

use super::*;

impl App {
    /// `kestrel focus bar`: the bar on the focused screen takes the keyboard; asked again, it
    /// gives it back. An open panel closes first (the bar is what the keys move along now).
    pub(crate) fn focus_bar(&mut self) -> Task<Message> {
        if self.bar_keys.is_some() {
            return self.leave_bar();
        }
        let Some(id) = self.focused_output() else {
            return Task::none();
        };
        let closed = if self.panel.as_ref().is_some_and(|p| p.open) {
            self.close()
        } else {
            Task::none()
        };
        self.bar_keys = Some(id);
        let Some(o) = self.output(id) else {
            return closed;
        };
        let first = Task::Iced(iced_runtime::task::effect(iced_runtime::Action::widget(
            iced::advanced::widget::operation::scope(
                shell::bar::root(o),
                crate::ui::focus::first(),
            ),
        )));
        Task::batch([closed, first])
    }

    /// The bar gives the keyboard back; its lit button goes out.
    pub(crate) fn leave_bar(&mut self) -> Task<Message> {
        let Some(o) = self.bar_keys.take().and_then(|id| self.output(id)) else {
            return Task::none();
        };
        Task::Iced(iced_runtime::task::effect(iced_runtime::Action::widget(
            iced::advanced::widget::operation::scope(
                shell::bar::root(o),
                iced::advanced::widget::operation::focusable::unfocus::<()>(),
            ),
        )))
    }

    /// A key while the bar has the keyboard (None: Esc, or a click). Along the bar only: Tab
    /// and the arrows all move to the next button that way on screen (Tab's own order is the
    /// bar's build order, sides before the centre).
    pub(crate) fn bar_key(&mut self, k: Option<panels::Key>) -> Task<Message> {
        use panels::Key;
        let Some(k) = k else {
            return self.leave_bar();
        };
        let Some(o) = self.bar_keys.and_then(|id| self.output(id)) else {
            return Task::none();
        };
        let k = match k {
            Key::Tab | Key::Right | Key::Down => Key::Right,
            Key::BackTab | Key::Left | Key::Up => Key::Left,
            _ => return Task::none(),
        };
        panels::focus_step(shell::bar::root(o), k)
    }

    /// The bar surface has the keyboard exactly while the bar is being used from it and no
    /// panel is open (one opened from it took the keyboard and ended this, as did a panel
    /// opened any other way, or the screen going).
    pub(crate) fn sync_bar_keyboard(&mut self) -> Task<Message> {
        let mut tasks = Vec::new();
        if self.bar_keys.is_some() && self.panel.as_ref().is_some_and(|p| p.open) {
            tasks.push(self.leave_bar());
        }
        if self
            .bar_keys
            .is_some_and(|id| !self.outputs.iter().any(|o| o.id == id))
        {
            self.bar_keys = None;
        }
        let want = self.bar_keys;
        for o in &mut self.outputs {
            let on = want == Some(o.id);
            if o.bar_keyboard != on {
                o.bar_keyboard = on;
                tasks.push(set_keyboard_interactivity(
                    o.bar,
                    if on {
                        KeyboardInteractivity::Exclusive
                    } else {
                        KeyboardInteractivity::None
                    },
                ));
            }
        }
        Task::batch(tasks)
    }
}
