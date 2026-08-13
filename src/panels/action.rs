//! What a panel asks the shell to do: the answer of a panel's `update`, as iced's guide has a
//! screen return an `Action` to its parent (docs.rs/iced, "Pocket Guide"). A panel changes only
//! its own state; everything beyond it is one of these, carried out by the shell in `perform`,
//! the one place it happens. The list is the whole contract: a panel cannot do anything else.

use crate::app::Message as AppMessage;
use crate::services::compositor;
use iced::Task;

pub enum Action<M> {
    None,
    /// Work for the panel itself (its answers come back as its own messages).
    Run(Task<M>),
    /// Close the open panel.
    Close,
    /// Ask the compositor (focus a window, start a program, open a file).
    #[allow(dead_code)] // the launcher's and overview's, as they move onto actions (Phase 9)
    Dispatch(compositor::Action),
    /// Connect (true) or disconnect a Bluetooth device, shown in progress until it is so.
    Device {
        path: String,
        connect: bool,
    },
    /// Several, in order.
    Many(Vec<Action<M>>),
}

impl<M> Action<M> {
    pub fn and(self, next: Action<M>) -> Action<M> {
        match self {
            Action::None => next,
            Action::Many(mut v) => {
                v.push(next);
                Action::Many(v)
            }
            a => Action::Many(vec![a, next]),
        }
    }
}

/// Carries out a panel's action. `wrap` turns the panel's own messages into the shell's.
pub fn perform<M: Send + 'static>(
    app: &mut crate::app::App,
    action: Action<M>,
    wrap: fn(M) -> AppMessage,
) -> Task<AppMessage> {
    match action {
        Action::None => Task::none(),
        Action::Run(t) => t.map(wrap),
        Action::Close => Task::done(AppMessage::Close),
        Action::Dispatch(a) => Task::done(AppMessage::Dispatch(a)),
        Action::Device { path, connect } => {
            use crate::services::bluetooth;
            app.connecting = Some(super::cx::Connecting::new(path.clone(), connect));
            app.device_error = None;
            let cmd = if connect {
                bluetooth::Command::Connect(path.clone())
            } else {
                bluetooth::Command::Disconnect(path.clone())
            };
            Task::perform(bluetooth::command(cmd), move |r| {
                AppMessage::DeviceDone(path.clone(), r)
            })
        }
        Action::Many(v) => Task::batch(v.into_iter().map(|a| perform(app, a, wrap))),
    }
}
