//! PipeWire, through its own API (no `pw-dump` or `wpctl` processes): one connection on its own
//! thread, kept for the session, reconnecting when PipeWire restarts.
//!
//! What kestrel reads from it is rebuilt from PipeWire's own picture, never from a chain of
//! start/stop events: the registry announces every object on connecting, each bound object
//! sends its whole state first, and a removal drops the object. Two sync round trips mark the
//! moment that picture is complete (`State::ready`).
//!
//! - The camera: video input streams (`Stream/Input/Video`) whose node is running, by app.
//! - WirePlumber's settings in force (the `sm-settings` metadata) and setting one, as
//!   `wpctl settings --save` does: the value in `sm-settings` and `persistent-sm-settings`
//!   (WirePlumber's lib/wp/settings.c, wp_settings_set and wp_settings_save).

use pipewire as pw;
use pw::metadata::Metadata;
use pw::node::{Node, NodeListener, NodeState};
use pw::types::ObjectType;
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;
use std::sync::{Mutex, OnceLock};
use tokio::sync::watch;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct State {
    /// Connected, and the first complete picture has arrived.
    pub ready: bool,
    /// The apps whose camera stream is running.
    pub camera: Vec<String>,
    /// WirePlumber's settings in force: key → JSON value.
    pub settings: BTreeMap<String, String>,
}

enum Cmd {
    /// A WirePlumber setting, kept across restarts (key, JSON value).
    Setting(String, String),
}

fn state() -> &'static watch::Sender<State> {
    static S: OnceLock<watch::Sender<State>> = OnceLock::new();
    S.get_or_init(|| {
        let (tx, _) = watch::channel(State::default());
        let _ = std::thread::Builder::new()
            .name("kestrel-pipewire".into())
            .spawn(|| {
                pw::init();
                loop {
                    if let Err(e) = run() {
                        log::warn!("pipewire: {e}");
                    }
                    *CMDS
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
                    // Nothing is known while it reconnects: said, not the old answer kept.
                    state().send_replace(State::default());
                    std::thread::sleep(std::time::Duration::from_secs(1));
                }
            });
        tx
    })
}

static CMDS: Mutex<Option<pw::channel::Sender<Cmd>>> = Mutex::new(None);

/// PipeWire's state, now and as it changes (the connection starts on first use).
pub fn watch() -> watch::Receiver<State> {
    state().subscribe()
}

/// A WirePlumber setting in force (its JSON), once the picture is complete.
pub async fn setting(key: &str) -> Option<String> {
    let mut rx = watch();
    let wait = rx.wait_for(|s| s.ready);
    let s = tokio::time::timeout(std::time::Duration::from_secs(3), wait)
        .await
        .ok()?
        .ok()?;
    s.settings.get(key).cloned()
}

/// Sets a WirePlumber setting and keeps it across restarts.
pub fn set_setting(key: &str, json: &str) {
    let _ = state();
    if let Some(tx) = CMDS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .as_ref()
    {
        let _ = tx.send(Cmd::Setting(key.into(), json.into()));
    }
}

struct Stream {
    app: Option<String>,
    running: bool,
}

#[derive(Default)]
struct Model {
    streams: HashMap<u32, Stream>,
    /// The bound camera stream nodes (dropped with their object).
    nodes: HashMap<u32, (Node, NodeListener)>,
    settings: BTreeMap<String, String>,
    /// (`sm-settings`, `persistent-sm-settings`) with their global ids.
    live: Option<(u32, Metadata, pw::metadata::MetadataListener)>,
    persistent: Option<(u32, Metadata)>,
    ready: bool,
}

/// The apps whose camera stream is running (an idle or paused stream is not a camera in use).
fn camera_apps<'a>(streams: impl Iterator<Item = &'a Stream>) -> Vec<String> {
    let mut v: Vec<String> = streams
        .filter(|s| s.running)
        .filter_map(|s| s.app.clone())
        .collect();
    v.sort();
    v.dedup();
    v
}

impl Model {
    fn publish(&self) {
        let next = State {
            ready: self.ready,
            camera: camera_apps(self.streams.values()),
            settings: self.settings.clone(),
        };
        state().send_if_modified(|s| {
            let changed = *s != next;
            *s = next;
            changed
        });
    }
}

/// The app a stream belongs to (PipeWire's and WirePlumber's own are not apps).
fn app_of(props: &pw::spa::utils::dict::DictRef) -> Option<String> {
    ["application.name", "application.process.binary"]
        .iter()
        .find_map(|k| props.get(k).filter(|s| !s.is_empty()))
        .filter(|n| !matches!(*n, "pipewire" | "wireplumber"))
        .map(crate::apps::name_for_class)
}

fn run() -> Result<(), pw::Error> {
    let ml = pw::main_loop::MainLoopRc::new(None)?;
    let ctx = pw::context::ContextRc::new(&ml, None)?;
    let core = ctx.connect_rc(None)?;
    let registry = core.get_registry_rc()?;
    let model = Rc::new(RefCell::new(Model::default()));

    // The connection lost (PipeWire restarted): the loop ends and is started again.
    let quit = ml.downgrade();
    // Ready after two round trips: the first returns once every object is announced (and the
    // wanted ones bound), the second once those bound objects have sent their state.
    let first = Rc::new(RefCell::new(core.sync(0).ok()));
    let second: Rc<RefCell<Option<pw::spa::utils::result::AsyncSeq>>> = Rc::new(RefCell::new(None));
    let (m, c, f, s2) = (model.clone(), core.clone(), first.clone(), second.clone());
    let _core_listener = core
        .add_listener_local()
        .done(move |id, seq| {
            if id != pw::core::PW_ID_CORE {
                return;
            }
            if f.borrow().as_ref() == Some(&seq) {
                *f.borrow_mut() = None;
                *s2.borrow_mut() = c.sync(0).ok();
            } else if s2.borrow().as_ref() == Some(&seq) {
                *s2.borrow_mut() = None;
                let mut m = m.borrow_mut();
                m.ready = true;
                m.publish();
            }
        })
        .error(move |id, _seq, res, message| {
            if id == pw::core::PW_ID_CORE {
                log::warn!("pipewire: connection error {res}: {message}");
                if let Some(ml) = quit.upgrade() {
                    ml.quit();
                }
            }
        })
        .register();

    let (m, reg) = (model.clone(), registry.downgrade());
    let m_remove = model.clone();
    let _registry_listener = registry
        .add_listener_local()
        .global(move |g| {
            let Some(registry) = reg.upgrade() else {
                return;
            };
            let props = g.props.as_ref();
            match g.type_ {
                ObjectType::Node
                    if props.and_then(|p| p.get("media.class")) == Some("Stream/Input/Video") =>
                {
                    let Ok(node) = registry.bind::<Node, _>(g) else {
                        return;
                    };
                    let id = g.id;
                    let app = props.and_then(|p| app_of(p));
                    m.borrow_mut().streams.insert(
                        id,
                        Stream {
                            app,
                            running: false,
                        },
                    );
                    let m2 = m.clone();
                    let listener = node
                        .add_listener_local()
                        .info(move |info| {
                            let mut m = m2.borrow_mut();
                            if let Some(s) = m.streams.get_mut(&id) {
                                s.running = matches!(info.state(), NodeState::Running);
                                if let Some(app) = info.props().and_then(app_of) {
                                    s.app = Some(app);
                                }
                            }
                            m.publish();
                        })
                        .register();
                    m.borrow_mut().nodes.insert(id, (node, listener));
                }
                ObjectType::Metadata => {
                    let name = props.and_then(|p| p.get("metadata.name"));
                    match name {
                        Some("sm-settings") => {
                            let Ok(md) = registry.bind::<Metadata, _>(g) else {
                                return;
                            };
                            let m2 = m.clone();
                            let listener = md
                                .add_listener_local()
                                .property(move |subject, key, _type, value| {
                                    if subject == 0 {
                                        let mut m = m2.borrow_mut();
                                        match (key, value) {
                                            // No key: everything cleared.
                                            (None, _) => m.settings.clear(),
                                            (Some(k), None) => {
                                                m.settings.remove(k);
                                            }
                                            (Some(k), Some(v)) => {
                                                m.settings.insert(k.into(), v.into());
                                            }
                                        }
                                        m.publish();
                                    }
                                    0
                                })
                                .register();
                            m.borrow_mut().live = Some((g.id, md, listener));
                        }
                        Some("persistent-sm-settings") => {
                            if let Ok(md) = registry.bind::<Metadata, _>(g) {
                                m.borrow_mut().persistent = Some((g.id, md));
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        })
        .global_remove(move |id| {
            let mut m = m_remove.borrow_mut();
            m.streams.remove(&id);
            m.nodes.remove(&id);
            if m.live.as_ref().is_some_and(|l| l.0 == id) {
                m.live = None;
                m.settings.clear();
            }
            if m.persistent.as_ref().is_some_and(|p| p.0 == id) {
                m.persistent = None;
            }
            m.publish();
        })
        .register();

    let (tx, rx) = pw::channel::channel::<Cmd>();
    let m = model.clone();
    let _commands = rx.attach(ml.loop_(), move |cmd| match cmd {
        Cmd::Setting(key, json) => {
            let m = m.borrow();
            for md in [
                m.live.as_ref().map(|l| &l.1),
                m.persistent.as_ref().map(|p| &p.1),
            ]
            .into_iter()
            .flatten()
            {
                md.set_property(0, &key, Some("Spa:String:JSON"), Some(&json));
            }
        }
    });
    *CMDS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(tx);

    ml.run();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Stream, camera_apps};

    #[test]
    fn only_running_app_streams_count() {
        let s = |app: Option<&str>, running| Stream {
            app: app.map(str::to_string),
            running,
        };
        let all = [
            s(Some("Chromium"), true),
            s(Some("Snapshot"), false),
            s(Some("Chromium"), true),
            // PipeWire's own (no app name kept for it).
            s(None, true),
        ];
        assert_eq!(camera_apps(all.iter()), vec!["Chromium".to_string()]);
    }
}

#[cfg(test)]
mod live {
    /// Connects to the running PipeWire and prints its picture (by hand: `--ignored`).
    #[tokio::test]
    #[ignore]
    async fn reads_the_live_picture() {
        let mut rx = super::watch();
        let s = tokio::time::timeout(std::time::Duration::from_secs(5), rx.wait_for(|s| s.ready))
            .await
            .expect("ready in time")
            .unwrap()
            .clone();
        println!("camera: {:?}", s.camera);
        println!("mono: {:?}", s.settings.get("node.features.audio.mono"));
        println!("{} settings", s.settings.len());
    }
}

#[cfg(test)]
mod live_write {
    /// Sets mono to its current value through PipeWire (by hand: `--ignored`).
    #[tokio::test]
    #[ignore]
    async fn sets_a_setting() {
        let now = super::setting("node.features.audio.mono").await.unwrap();
        super::set_setting("node.features.audio.mono", &now);
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
}
