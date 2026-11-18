//! "Open with…", as the desktop portal's AppChooser backend: when an app asks which program
//! should open a file or link (GTK's Open With, a file type with no default), the portal
//! forwards it here (served by crate::portal) and the shell shows its own list. The call waits
//! until it is answered: the chosen app, or cancelled.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};
use tokio::sync::{mpsc, oneshot};
use zbus::interface;
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

/// A question for the shell: which of `choices` (desktop entry ids, without ".desktop")
/// should open the thing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ask {
    pub id: u64,
    /// The asking app's id ("org.gnome.Nautilus"), empty when unknown.
    pub app_id: String,
    pub choices: Vec<String>,
    /// What was chosen last time for this kind of file.
    pub last: Option<String>,
    /// The file's name (no path) or the link, to say what is being opened.
    pub what: String,
    pub content_type: String,
}

/// What the portal hands the shell.
#[derive(Clone, Debug)]
pub enum Event {
    Ask(Ask),
    /// The app offers other choices for the question being shown (`UpdateChoices`).
    Update(u64, Vec<String>),
}

static NEXT: AtomicU64 = AtomicU64::new(1);
static WAITING: LazyLock<Mutex<HashMap<u64, oneshot::Sender<Option<String>>>>> =
    LazyLock::new(Default::default);
/// Each request's handle → its question (UpdateChoices names the handle).
static HANDLES: LazyLock<Mutex<HashMap<String, u64>>> = LazyLock::new(Default::default);

/// The person chose `choice` for question `id` (None: cancelled).
pub fn answer(id: u64, choice: Option<String>) {
    if let Some(tx) = WAITING.lock().ok().and_then(|mut w| w.remove(&id)) {
        let _ = tx.send(choice);
    }
}

/// The AppChooser interface; questions go to `tx` (served by crate::portal).
pub(crate) struct Chooser {
    tx: mpsc::UnboundedSender<Event>,
}

impl Chooser {
    pub(crate) fn new(tx: mpsc::UnboundedSender<Event>) -> Self {
        Self { tx }
    }
}

fn text(o: &HashMap<String, OwnedValue>, k: &str) -> Option<String> {
    o.get(k)
        .and_then(|v| String::try_from(v.try_clone().ok()?).ok())
        .filter(|s| !s.is_empty())
}

#[interface(name = "org.freedesktop.impl.portal.AppChooser")]
impl Chooser {
    #[zbus(property)]
    fn version(&self) -> u32 {
        2
    }

    async fn choose_application(
        &self,
        handle: OwnedObjectPath,
        app_id: String,
        _parent_window: String,
        choices: Vec<String>,
        options: HashMap<String, OwnedValue>,
    ) -> (u32, HashMap<String, OwnedValue>) {
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        if let Ok(mut w) = WAITING.lock() {
            w.insert(id, tx);
        }
        if let Ok(mut h) = HANDLES.lock() {
            h.insert(handle.to_string(), id);
        }
        let what = text(&options, "filename")
            .or_else(|| text(&options, "uri"))
            .unwrap_or_default();
        let ask = Ask {
            id,
            app_id,
            choices,
            last: text(&options, "last_choice"),
            what,
            content_type: text(&options, "content_type").unwrap_or_default(),
        };
        let _ = self.tx.send(Event::Ask(ask));
        let choice = rx.await.ok().flatten();
        if let Ok(mut h) = HANDLES.lock() {
            h.remove(handle.as_str());
        }
        match choice {
            Some(c) => {
                let mut out = HashMap::new();
                if let Ok(v) = OwnedValue::try_from(zbus::zvariant::Value::from(c)) {
                    out.insert("choice".to_string(), v);
                }
                (0, out)
            }
            None => (1, HashMap::new()),
        }
    }

    fn update_choices(&self, handle: OwnedObjectPath, choices: Vec<String>) {
        let id = HANDLES
            .lock()
            .ok()
            .and_then(|h| h.get(handle.as_str()).copied());
        if let Some(id) = id {
            let _ = self.tx.send(Event::Update(id, choices));
        }
    }
}
