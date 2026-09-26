//! kestrel's file picker, as the desktop portal's FileChooser backend: when an app asks to
//! open or save a file (a browser's Upload, Save as), xdg-desktop-portal forwards the request
//! to kestrel's portal backend (crate::portal, which serves this interface) and the shell
//! shows its own picker.
//! The call waits until the picker answers: chosen files, or cancelled.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};
use tokio::sync::{mpsc, oneshot};
use zbus::interface;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

/// A file type an app offers to filter by: its name and glob patterns ("*.png").
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Filter {
    pub name: String,
    pub globs: Vec<String>,
    /// MIME types ("image/png", "image/*"), matched by a file's type (crate::open::is_a).
    pub mimes: Vec<String>,
}

/// What an app asked for.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Request {
    pub id: u64,
    /// Saving (a name to type) rather than opening.
    pub save: bool,
    pub title: String,
    pub multiple: bool,
    /// Choose a folder, not a file.
    pub directory: bool,
    pub filters: Vec<Filter>,
    /// The filter the app wants chosen first (`current_filter`), as an index into `filters`.
    pub current: Option<usize>,
    /// Saving: the suggested name.
    pub name: String,
    /// Where to start.
    pub folder: Option<PathBuf>,
    /// The confirm button's label ("Upload", "Save").
    pub accept: String,
}

pub enum Answer {
    Cancel,
    Chosen(Vec<PathBuf>),
}

/// The id of a request kestrel makes for itself (choosing the wallpaper folder): answered
/// inside the shell, never sent to an app. Apps' requests count from 1.
pub const OWN: u64 = 0;

static NEXT: AtomicU64 = AtomicU64::new(1);
static WAITING: LazyLock<Mutex<HashMap<u64, oneshot::Sender<Answer>>>> =
    LazyLock::new(Default::default);

/// A request from the shell itself (a picture for the account): the same dialog apps get,
/// answered here instead of over the portal.
pub fn local(
    title: &str,
    filter: Option<Filter>,
    folder: Option<PathBuf>,
) -> (Request, oneshot::Receiver<Answer>) {
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let (tx, rx) = oneshot::channel();
    if let Ok(mut w) = WAITING.lock() {
        w.insert(id, tx);
    }
    (
        Request {
            id,
            save: false,
            title: title.to_string(),
            multiple: false,
            directory: false,
            filters: filter.into_iter().collect(),
            current: None,
            name: String::new(),
            folder,
            accept: String::new(),
        },
        rx,
    )
}

/// Answers request `id` (the app waiting on it gets the files, or a cancel).
pub fn answer(id: u64, a: Answer) {
    if let Some(tx) = WAITING.lock().ok().and_then(|mut w| w.remove(&id)) {
        let _ = tx.send(a);
    }
}

/// The FileChooser interface; its requests go to `tx` (served by crate::portal).
pub(crate) struct Chooser {
    tx: mpsc::UnboundedSender<Request>,
}

impl Chooser {
    pub(crate) fn new(tx: mpsc::UnboundedSender<Request>) -> Self {
        Self { tx }
    }
}

fn get<T: TryFrom<OwnedValue>>(o: &HashMap<String, OwnedValue>, k: &str) -> Option<T> {
    o.get(k).and_then(|v| T::try_from(v.clone()).ok())
}

/// A path as a file:// URI.
pub fn uri(p: &std::path::Path) -> String {
    use std::os::unix::ffi::OsStrExt;
    let mut s = String::from("file://");
    for &b in p.as_os_str().as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                s.push(b as char);
            }
            _ => s.push_str(&format!("%{b:02X}")),
        }
    }
    s
}

impl Chooser {
    async fn ask(
        &self,
        save: bool,
        title: String,
        o: HashMap<String, OwnedValue>,
    ) -> (u32, HashMap<String, OwnedValue>) {
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let filter = |(name, rules): (String, Vec<(u32, String)>)| Filter {
            name,
            globs: rules
                .iter()
                .filter(|(k, _)| *k == 0)
                .map(|(_, g)| g.clone())
                .collect(),
            mimes: rules
                .iter()
                .filter(|(k, _)| *k == 1)
                .map(|(_, m)| m.clone())
                .collect(),
        };
        let mut filters: Vec<Filter> = get::<Vec<(String, Vec<(u32, String)>)>>(&o, "filters")
            .unwrap_or_default()
            .into_iter()
            .map(filter)
            .collect();
        // The one the app wants chosen first: one of its filters, or (the spec allows it) one
        // of its own, added to the list.
        let current = get::<(String, Vec<(u32, String)>)>(&o, "current_filter")
            .map(filter)
            .map(|c| match filters.iter().position(|f| *f == c) {
                Some(i) => i,
                None => {
                    filters.push(c);
                    filters.len() - 1
                }
            });
        let folder = get::<Vec<u8>>(&o, "current_folder").and_then(|b| {
            let b: Vec<u8> = b.into_iter().take_while(|&c| c != 0).collect();
            use std::os::unix::ffi::OsStringExt;
            let p = PathBuf::from(std::ffi::OsString::from_vec(b));
            p.is_dir().then_some(p)
        });
        let req = Request {
            id,
            save,
            title,
            multiple: get(&o, "multiple").unwrap_or(false),
            directory: get(&o, "directory").unwrap_or(false),
            filters,
            current,
            name: get(&o, "current_name").unwrap_or_default(),
            folder,
            accept: get(&o, "accept_label").unwrap_or_default(),
        };
        let (tx, rx) = oneshot::channel();
        if let Ok(mut w) = WAITING.lock() {
            w.insert(id, tx);
        }
        if self.tx.send(req).is_err() {
            return (2, HashMap::new());
        }
        match rx.await {
            Ok(Answer::Chosen(paths)) if !paths.is_empty() => {
                let uris: Vec<String> = paths.iter().map(|p| uri(p)).collect();
                let mut out = HashMap::new();
                if let Ok(v) = OwnedValue::try_from(Value::from(uris)) {
                    out.insert("uris".to_string(), v);
                }
                (0, out)
            }
            _ => (1, HashMap::new()),
        }
    }
}

#[interface(name = "org.freedesktop.impl.portal.FileChooser")]
impl Chooser {
    async fn open_file(
        &self,
        _handle: OwnedObjectPath,
        _app_id: String,
        _parent_window: String,
        title: String,
        options: HashMap<String, OwnedValue>,
    ) -> (u32, HashMap<String, OwnedValue>) {
        self.ask(false, title, options).await
    }

    async fn save_file(
        &self,
        _handle: OwnedObjectPath,
        _app_id: String,
        _parent_window: String,
        title: String,
        options: HashMap<String, OwnedValue>,
    ) -> (u32, HashMap<String, OwnedValue>) {
        self.ask(true, title, options).await
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn paths_become_uris() {
        assert_eq!(
            super::uri(std::path::Path::new("/home/a/My File#1.png")),
            "file:///home/a/My%20File%231.png"
        );
    }
}

#[cfg(test)]
mod folder_tests {
    use super::*;
    use zbus::zvariant::{OwnedValue, Value};

    /// `current_folder` as apps send it: a byte array ending in a NUL.
    #[test]
    fn reads_current_folder_bytes() {
        let bytes = b"/tmp\0".to_vec();
        let v = OwnedValue::try_from(Value::from(bytes)).unwrap();
        let mut o = HashMap::new();
        o.insert("current_folder".to_string(), v);
        let got = get::<Vec<u8>>(&o, "current_folder");
        assert_eq!(got.as_deref(), Some(&b"/tmp\0"[..]));
    }
}
