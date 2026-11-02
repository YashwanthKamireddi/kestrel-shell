//! The Lua user layer: `~/.config/kestrel/init.lua` and plugins in
//! `~/.config/kestrel/plugins/<id>/` (see `Plugin`).
//!
//! The script runs on its own thread with its own Lua state, so a slow or broken script can
//! never stall or crash the shell: every call has a time limit, errors come back as events
//! (shown as a notification), and a panic on the thread only ends the Lua layer. Saving the
//! file reloads it with a fresh state.
//!
//! ```lua
//! kestrel.bar { style = "pills", right = { "disk", "tray", "network", "volume", "battery", "power" } }
//!
//! kestrel.widget("disk", {
//!   interval = 60,                       -- seconds between updates (default 10)
//!   update = function()                  -- returns { text, icon } / a string / nil to hide
//!     return { icon = "hard-drive", text = kestrel.read("df --output=pcent / | tail -1") }
//!   end,
//!   click = "kitty -e btop",             -- a command, or a function
//! })
//! ```
//!
//! Also: `kestrel.exec(cmd)` starts a program, `kestrel.read(cmd)` runs one and returns its
//! output (2 s limit), `kestrel.notify(title, body)` shows a notification,
//! `kestrel.island{ icon, title, detail }` shows a moment in the island, and
//! `kestrel.on(event, fn)` runs `fn(table)` when something happens:
//! workspace {id}, window {class, title}, notification {app, summary, body},
//! media {title, artist, playing}, device {name, connected}, power {charging, percent},
//! battery {percent}.
//!
//! A widget's `icon` is a name from kestrel's icon set (src/ui/icon.rs), else an icon theme
//! name ("drive-harddisk") or a file path.

use crate::config::BarStyle;
use futures::{SinkExt, Stream};
use mlua::{Function, HookTriggers, Lua, Table, Value, VmState};
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// How long one call into the script may run.
const CALL_LIMIT: Duration = Duration::from_secs(1);
/// How long `kestrel.read` waits for its command.
const READ_LIMIT: Duration = Duration::from_secs(2);

/// What a widget shows in the bar.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Chip {
    pub text: String,
    /// An icon name from kestrel's set.
    pub icon: Option<String>,
}

/// `kestrel.bar{}`: sections the script sets replace the config file's.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Bar {
    pub style: Option<BarStyle>,
    pub left: Option<Vec<String>>,
    pub center: Option<Vec<String>>,
    pub right: Option<Vec<String>>,
}

impl Bar {
    pub fn apply(&self, bar: &mut crate::config::BarConfig) {
        if let Some(s) = self.style {
            bar.style = s;
        }
        for (mine, theirs) in [
            (&self.left, &mut bar.left),
            (&self.center, &mut bar.center),
            (&self.right, &mut bar.right),
        ] {
            if let Some(v) = mine {
                theirs.clone_from(v);
            }
        }
    }
}

#[derive(Clone, Debug)]
pub enum Event {
    /// The script (re)loaded: its bar layout and the widget names it defines.
    Loaded {
        bar: Bar,
        widgets: Vec<String>,
    },
    /// A widget's new content (None hides it).
    Widget(String, Option<Chip>),
    Error(String),
    Notify(String, String),
    /// `kestrel.exec`: a command for Hyprland to start.
    Exec(String),
    /// `kestrel.island{}`: a moment in the island (icon name, title, detail).
    Island(Option<String>, String, String),
}

/// A value in an event's table.
#[derive(Clone, Debug)]
pub enum Val {
    Str(String),
    Num(f64),
    Bool(bool),
}

enum Command {
    Reload,
    Click(String),
    Event(&'static str, Vec<(&'static str, Val)>),
}

static COMMANDS: OnceLock<Mutex<mpsc::Sender<Command>>> = OnceLock::new();

fn send(c: Command) {
    if let Some(tx) = COMMANDS.get()
        && let Ok(tx) = tx.lock()
    {
        let _ = tx.send(c);
    }
}

/// Re-read init.lua.
pub fn reload() {
    send(Command::Reload);
}

/// Something happened that scripts can react to (`kestrel.on(name, fn)`); `fields` become
/// the table the handler receives.
pub fn event(name: &'static str, fields: Vec<(&'static str, Val)>) {
    send(Command::Event(name, fields));
}

/// A widget was clicked.
pub fn click(name: &str) {
    send(Command::Click(name.to_string()));
}

pub fn path() -> PathBuf {
    crate::config::path().with_file_name("init.lua")
}

/// The Lua layer's events. Starts the script thread; there is one per process.
pub fn stream() -> impl Stream<Item = Event> {
    crate::services::stream::channel(32, async |mut out| {
        let (tx, rx) = mpsc::channel();
        let (etx, mut erx) = futures::channel::mpsc::unbounded();
        if COMMANDS.set(Mutex::new(tx.clone())).is_err() {
            // A second subscription: hand the thread over to the new sender.
            if let Some(m) = COMMANDS.get()
                && let Ok(mut g) = m.lock()
            {
                *g = tx;
            }
        }
        let spawned = std::thread::Builder::new()
            .name("kestrel-lua".into())
            .spawn(move || {
                let emit = move |e: Event| {
                    let _ = etx.unbounded_send(e);
                };
                let fail = emit.clone();
                if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(&rx, &emit)))
                    .is_err()
                {
                    fail(Event::Error(
                        "the Lua layer stopped after an internal error".into(),
                    ));
                }
            });
        if let Err(e) = spawned {
            log::warn!("lua: starting the script thread: {e}");
            return;
        }
        use futures::StreamExt;
        while let Some(e) = erx.next().await {
            if out.send(e).await.is_err() {
                break;
            }
        }
    })
}

struct Widget {
    name: String,
    interval: Duration,
    update: Option<Function>,
    click: Option<Value>,
    due: Instant,
    /// The last error was reported; stay quiet until it works again.
    failing: bool,
}

/// The script thread: load, then run widget updates when due and clicks when asked.
fn run(rx: &mpsc::Receiver<Command>, emit: &(impl Fn(Event) + Clone + 'static)) {
    let mut session = Session::load(emit);
    loop {
        let wait = session
            .widgets
            .iter()
            .map(|w| w.due.saturating_duration_since(Instant::now()))
            .min()
            .unwrap_or(Duration::from_secs(3600));
        match rx.recv_timeout(wait) {
            Ok(Command::Reload) => {
                // Clear the old widgets' chips before the new script takes over.
                for w in &session.widgets {
                    emit(Event::Widget(w.name.clone(), None));
                }
                session = Session::load(emit);
            }
            Ok(Command::Click(name)) => session.click(&name, emit),
            Ok(Command::Event(name, fields)) => session.dispatch(name, &fields, emit),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
        }
        session.update_due(emit);
    }
}

struct Session {
    lua: Lua,
    deadline: Rc<Cell<Instant>>,
    widgets: Vec<Widget>,
    /// `kestrel.on` handlers: (event, function, failed already).
    handlers: Rc<RefCell<Vec<(String, Function, bool)>>>,
}

impl Session {
    fn load(emit: &(impl Fn(Event) + Clone + 'static)) -> Self {
        let file = path();
        let session = Self::start(emit, std::fs::read_to_string(&file).ok(), plugins());
        log::info!(
            "lua: loaded {} ({} widgets)",
            file.display(),
            session.widgets.len()
        );
        session
    }

    /// A fresh state running `init` (init.lua's text, if there is one), then `plugins`.
    fn start(
        emit: &(impl Fn(Event) + Clone + 'static),
        init: Option<String>,
        plugins: Vec<Plugin>,
    ) -> Self {
        let lua = Lua::new();
        let deadline = Rc::new(Cell::new(Instant::now() + CALL_LIMIT));
        let d = deadline.clone();
        let hooked = lua.set_hook(
            HookTriggers::new().every_nth_instruction(10_000),
            move |_, _| {
                if Instant::now() > d.get() {
                    Err(mlua::Error::runtime("took longer than a second"))
                } else {
                    Ok(VmState::Continue)
                }
            },
        );
        if let Err(e) = hooked {
            log::warn!("lua: no time limit for scripts: {e}");
        }
        let bar = Rc::new(RefCell::new(Bar::default()));
        let defined = Rc::new(RefCell::new(Vec::new()));
        let mut session = Self {
            lua,
            deadline,
            widgets: Vec::new(),
            handlers: Rc::new(RefCell::new(Vec::new())),
        };
        if let Err(e) = session.install_api(emit, bar.clone(), defined.clone()) {
            emit(Event::Error(format!("init.lua: {e}")));
            return session;
        }
        // init.lua first (it may set the layout), then every plugin. A failing script is
        // reported by name and the others still load.
        if let Some(source) = init {
            session.deadline.set(Instant::now() + CALL_LIMIT);
            let loaded = session.lua.load(source).set_name("@init.lua").exec();
            if let Err(e) = loaded {
                emit(Event::Error(short(&e)));
                *bar.borrow_mut() = Bar::default();
            }
        }
        for plugin in plugins {
            session.deadline.set(Instant::now() + CALL_LIMIT);
            if let Err(e) = session.load_plugin(&plugin) {
                emit(Event::Error(format!("plugin {}: {}", plugin.id, short(&e))));
            }
        }
        session.widgets = defined.take();
        emit(Event::Loaded {
            bar: bar.borrow().clone(),
            widgets: session.widgets.iter().map(|w| w.name.clone()).collect(),
        });
        session
    }

    /// Runs a plugin's script with its own globals (so plugins never collide) and its
    /// settings as `kestrel.config`... available as the global `config` in its environment.
    fn load_plugin(&self, p: &Plugin) -> mlua::Result<()> {
        let lua = &self.lua;
        let env = lua.create_table()?;
        let meta = lua.create_table()?;
        meta.set("__index", lua.globals())?;
        env.set_metatable(Some(meta))?;
        let config = lua.create_table()?;
        for (k, v) in &p.settings {
            config.set(k.as_str(), toml_to_lua(lua, v)?)?;
        }
        env.set("config", config)?;
        env.set("plugin_dir", p.dir.to_string_lossy().into_owned())?;
        let source = std::fs::read_to_string(p.dir.join(&p.main))
            .map_err(|e| mlua::Error::runtime(format!("{}: {e}", p.main)))?;
        lua.load(source)
            .set_name(format!("@{}/{}", p.id, p.main))
            .set_environment(env)
            .exec()
    }

    fn install_api(
        &self,
        emit: &(impl Fn(Event) + Clone + 'static),
        bar: Rc<RefCell<Bar>>,
        defined: Rc<RefCell<Vec<Widget>>>,
    ) -> mlua::Result<()> {
        let lua = &self.lua;
        let k = lua.create_table()?;

        k.set(
            "bar",
            lua.create_function(move |_, t: Table| {
                let list = |key: &str| -> mlua::Result<Option<Vec<String>>> { t.get(key) };
                let mut guard = bar.borrow_mut();
                let b = &mut *guard;
                b.style = match t.get::<Option<String>>("style")?.as_deref() {
                    None => b.style,
                    Some("full") => Some(BarStyle::Full),
                    Some("pills") => Some(BarStyle::Pills),
                    Some(other) => {
                        return Err(mlua::Error::runtime(format!(
                            "bar style {other:?}: use \"full\" or \"pills\""
                        )));
                    }
                };
                for (key, slot) in [
                    ("left", &mut b.left),
                    ("center", &mut b.center),
                    ("right", &mut b.right),
                ] {
                    if let Some(v) = list(key)? {
                        *slot = Some(v);
                    }
                }
                Ok(())
            })?,
        )?;

        k.set(
            "widget",
            lua.create_function(move |_, (name, t): (String, Table)| {
                if crate::modules::known(&name) {
                    return Err(mlua::Error::runtime(format!(
                        "widget {name:?}: that name belongs to a built-in module"
                    )));
                }
                let interval: f64 = t.get::<Option<f64>>("interval")?.unwrap_or(10.0);
                let mut d = defined.borrow_mut();
                d.retain(|w| w.name != name);
                d.push(Widget {
                    name,
                    interval: Duration::from_secs_f64(interval.max(0.5)),
                    update: t.get("update")?,
                    click: t.get::<Option<Value>>("click")?,
                    due: Instant::now(),
                    failing: false,
                });
                Ok(())
            })?,
        )?;

        let handlers = self.handlers.clone();
        k.set(
            "on",
            lua.create_function(move |_, (name, f): (String, Function)| {
                const EVENTS: &[&str] = &[
                    "workspace",
                    "window",
                    "notification",
                    "media",
                    "device",
                    "power",
                    "battery",
                ];
                if !EVENTS.contains(&name.as_str()) {
                    return Err(mlua::Error::runtime(format!(
                        "kestrel.on: no event {name:?} (events: {})",
                        EVENTS.join(", ")
                    )));
                }
                handlers.borrow_mut().push((name, f, false));
                Ok(())
            })?,
        )?;

        let e = emit.clone();
        k.set(
            "island",
            lua.create_function(move |_, t: Table| {
                e(Event::Island(
                    t.get::<Option<String>>("icon")?,
                    t.get::<Option<String>>("title")?.unwrap_or_default(),
                    t.get::<Option<String>>("detail")?.unwrap_or_default(),
                ));
                Ok(())
            })?,
        )?;

        let e = emit.clone();
        k.set(
            "exec",
            lua.create_function(move |_, cmd: String| {
                e(Event::Exec(cmd));
                Ok(())
            })?,
        )?;

        // Waiting for the command doesn't count against the script's time limit.
        let d = self.deadline.clone();
        k.set(
            "read",
            lua.create_function(move |_, cmd: String| {
                let start = Instant::now();
                let out = read(&cmd);
                d.set(d.get() + start.elapsed());
                Ok(out)
            })?,
        )?;

        let e = emit.clone();
        k.set(
            "notify",
            lua.create_function(move |_, (title, body): (String, Option<String>)| {
                e(Event::Notify(title, body.unwrap_or_default()));
                Ok(())
            })?,
        )?;

        lua.globals().set("kestrel", k)
    }

    /// Run every widget whose update is due.
    fn update_due(&mut self, emit: &impl Fn(Event)) {
        let now = Instant::now();
        for i in 0..self.widgets.len() {
            if self.widgets[i].due <= now {
                self.update(i, emit);
            }
        }
    }

    fn update(&mut self, i: usize, emit: &impl Fn(Event)) {
        self.deadline.set(Instant::now() + CALL_LIMIT);
        let w = &mut self.widgets[i];
        w.due = Instant::now() + w.interval;
        let Some(f) = &w.update else {
            return;
        };
        match f.call::<Value>(()).and_then(chip) {
            Ok(c) => {
                w.failing = false;
                emit(Event::Widget(w.name.clone(), c));
            }
            Err(e) => {
                if !w.failing {
                    w.failing = true;
                    emit(Event::Error(format!("widget {:?}: {}", w.name, short(&e))));
                }
            }
        }
    }

    /// Run every handler for `name` with the event's table. A failing handler is reported
    /// once, then quietly skipped until the script reloads.
    fn dispatch(&mut self, name: &str, fields: &[(&'static str, Val)], emit: &impl Fn(Event)) {
        let Ok(table) = self.lua.create_table() else {
            return;
        };
        for (k, v) in fields {
            let _ = match v {
                Val::Str(s) => table.set(*k, s.as_str()),
                Val::Num(n) => table.set(*k, *n),
                Val::Bool(b) => table.set(*k, *b),
            };
        }
        let mut handlers = self.handlers.borrow_mut();
        for (event, f, failed) in handlers.iter_mut() {
            if event != name || *failed {
                continue;
            }
            self.deadline.set(Instant::now() + CALL_LIMIT);
            if let Err(e) = f.call::<()>(table.clone()) {
                *failed = true;
                emit(Event::Error(format!("kestrel.on({name:?}): {}", short(&e))));
            }
        }
    }

    fn click(&mut self, name: &str, emit: &impl Fn(Event)) {
        let Some(i) = self.widgets.iter().position(|w| w.name == name) else {
            return;
        };
        self.deadline.set(Instant::now() + CALL_LIMIT);
        let result = match &self.widgets[i].click {
            Some(Value::String(cmd)) => {
                emit(Event::Exec(cmd.to_string_lossy()));
                Ok(())
            }
            Some(Value::Function(f)) => f.call::<()>(()),
            _ => Ok(()),
        };
        if let Err(e) = result {
            emit(Event::Error(format!(
                "widget {name:?} click: {}",
                short(&e)
            )));
        }
        // A click usually changes what the widget shows.
        self.update(i, emit);
    }
}

/// An update's return value as a chip: a table, a plain string, or nil.
/// A plugin: `~/.config/kestrel/plugins/<id>/plugin.toml` and its script.
///
/// ```toml
/// name = "Disk"
/// description = "Free space on /"
/// main = "main.lua"      # default
/// [settings]             # defaults; `[plugins.<id>]` in config.toml overrides them
/// path = "/"
/// ```
/// `[plugins.<id>] enabled = false` in config.toml turns a plugin off.
#[derive(Debug)]
struct Plugin {
    id: String,
    dir: PathBuf,
    main: String,
    settings: toml::Table,
}

/// An installed plugin as Settings shows it: (id, name, description, on).
pub fn installed() -> Vec<(String, String, String, bool)> {
    let overrides: toml::Table = std::fs::read_to_string(crate::config::path())
        .ok()
        .and_then(|s| s.parse::<toml::Table>().ok())
        .and_then(|t| t.get("plugins").and_then(|p| p.as_table()).cloned())
        .unwrap_or_default();
    let mut out: Vec<_> = std::fs::read_dir(plugins_dir())
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let id = e.file_name().to_string_lossy().into_owned();
            let m: toml::Table = std::fs::read_to_string(e.path().join("plugin.toml"))
                .ok()?
                .parse()
                .ok()?;
            let s = |k: &str| m.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
            let on = overrides
                .get(&id)
                .and_then(|u| u.get("enabled"))
                .and_then(toml::Value::as_bool)
                != Some(false);
            let name = if s("name").is_empty() {
                id.clone()
            } else {
                s("name")
            };
            Some((id, name, s("description"), on))
        })
        .collect();
    out.sort();
    out
}

pub fn plugins_dir() -> PathBuf {
    crate::config::path().with_file_name("plugins")
}

/// The installed plugins that are on, in name order, with their settings resolved.
fn plugins() -> Vec<Plugin> {
    let overrides: toml::Table = std::fs::read_to_string(crate::config::path())
        .ok()
        .and_then(|s| s.parse::<toml::Table>().ok())
        .and_then(|t| t.get("plugins").and_then(|p| p.as_table()).cloned())
        .unwrap_or_default();
    let Ok(rd) = std::fs::read_dir(plugins_dir()) else {
        return Vec::new();
    };
    let mut out: Vec<Plugin> = rd
        .flatten()
        .filter_map(|e| {
            let dir = e.path();
            let id = e.file_name().to_string_lossy().into_owned();
            let manifest: toml::Table = std::fs::read_to_string(dir.join("plugin.toml"))
                .ok()?
                .parse()
                .ok()?;
            let user = overrides.get(&id).and_then(|v| v.as_table());
            if user
                .and_then(|u| u.get("enabled"))
                .and_then(toml::Value::as_bool)
                == Some(false)
            {
                return None;
            }
            let mut settings = manifest
                .get("settings")
                .and_then(|s| s.as_table())
                .cloned()
                .unwrap_or_default();
            for (k, v) in user.into_iter().flatten() {
                if k != "enabled" {
                    settings.insert(k.clone(), v.clone());
                }
            }
            Some(Plugin {
                main: manifest
                    .get("main")
                    .and_then(|m| m.as_str())
                    .unwrap_or("main.lua")
                    .to_string(),
                id,
                dir,
                settings,
            })
        })
        .collect();
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

fn toml_to_lua(lua: &Lua, v: &toml::Value) -> mlua::Result<Value> {
    Ok(match v {
        toml::Value::String(s) => Value::String(lua.create_string(s)?),
        toml::Value::Integer(i) => Value::Integer(*i),
        toml::Value::Float(f) => Value::Number(*f),
        toml::Value::Boolean(b) => Value::Boolean(*b),
        toml::Value::Datetime(d) => Value::String(lua.create_string(d.to_string())?),
        toml::Value::Array(a) => {
            let t = lua.create_table()?;
            for (i, x) in a.iter().enumerate() {
                t.set(i + 1, toml_to_lua(lua, x)?)?;
            }
            Value::Table(t)
        }
        toml::Value::Table(m) => {
            let t = lua.create_table()?;
            for (k, x) in m {
                t.set(k.as_str(), toml_to_lua(lua, x)?)?;
            }
            Value::Table(t)
        }
    })
}

fn chip(v: Value) -> mlua::Result<Option<Chip>> {
    Ok(match v {
        Value::Nil => None,
        Value::String(s) => Some(Chip {
            text: s.to_string_lossy(),
            icon: None,
        }),
        Value::Table(t) => Some(Chip {
            text: t.get::<Option<String>>("text")?.unwrap_or_default(),
            icon: t.get("icon")?,
        }),
        other => {
            return Err(mlua::Error::runtime(format!(
                "update returned a {}; return a table, a string or nil",
                other.type_name()
            )));
        }
    })
}

/// The first line of an error, which is the useful part of a Lua traceback.
fn short(e: &mlua::Error) -> String {
    e.to_string()
        .lines()
        .next()
        .unwrap_or_default()
        .trim_start_matches("runtime error: ")
        .to_string()
}

/// Run `cmd` with `sh -c` and return its trimmed output; empty on failure or after the limit.
fn read(cmd: &str) -> String {
    use std::io::Read;
    use std::process::{Command, Stdio};
    let Ok(mut child) = Command::new("sh")
        .args(["-c", cmd])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return String::new();
    };
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if start.elapsed() < READ_LIMIT => {
                std::thread::sleep(Duration::from_millis(5));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return String::new();
            }
        }
    }
    let mut out = String::new();
    if let Some(mut s) = child.stdout.take() {
        let _ = s.read_to_string(&mut out);
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chip_accepts_tables_strings_and_nil() {
        let lua = Lua::new();
        let t: Value = lua.load("{ text = '42%', icon = 'cpu' }").eval().unwrap();
        assert_eq!(
            chip(t).unwrap(),
            Some(Chip {
                text: "42%".into(),
                icon: Some("cpu".into())
            })
        );
        let s: Value = lua.load("'hi'").eval().unwrap();
        assert_eq!(chip(s).unwrap().unwrap().text, "hi");
        assert_eq!(chip(Value::Nil).unwrap(), None);
        assert!(chip(Value::Integer(3)).is_err());
    }

    #[test]
    fn runaway_scripts_are_stopped() {
        let lua = Lua::new();
        let deadline = Instant::now() + Duration::from_millis(50);
        lua.set_hook(
            HookTriggers::new().every_nth_instruction(10_000),
            move |_, _| {
                if Instant::now() > deadline {
                    Err(mlua::Error::runtime("took longer than a second"))
                } else {
                    Ok(VmState::Continue)
                }
            },
        )
        .unwrap();
        assert!(lua.load("while true do end").exec().is_err());
    }

    #[test]
    fn read_returns_output_and_times_out() {
        assert_eq!(read("echo ' hello '"), "hello");
        let t = Instant::now();
        assert_eq!(read("sleep 5; echo late"), "");
        assert!(t.elapsed() < Duration::from_secs(4));
    }

    type Seen = Rc<RefCell<Vec<Event>>>;

    /// A session running `init`, and what it said.
    fn session(init: &str) -> (Session, Seen) {
        session_with(init, Vec::new())
    }

    fn session_with(init: &str, plugins: Vec<Plugin>) -> (Session, Seen) {
        let seen: Seen = Rc::default();
        let s = seen.clone();
        let emit = move |e: Event| s.borrow_mut().push(e);
        let session = Session::start(&emit, Some(init.to_string()), plugins);
        (session, seen)
    }

    fn emitter(seen: &Seen) -> impl Fn(Event) + Clone + 'static {
        let s = seen.clone();
        move |e: Event| s.borrow_mut().push(e)
    }

    fn errors(seen: &Seen) -> Vec<String> {
        seen.borrow()
            .iter()
            .filter_map(|e| match e {
                Event::Error(m) => Some(m.clone()),
                _ => None,
            })
            .collect()
    }

    fn loaded(seen: &Seen) -> (Bar, Vec<String>) {
        seen.borrow()
            .iter()
            .find_map(|e| match e {
                Event::Loaded { bar, widgets } => Some((bar.clone(), widgets.clone())),
                _ => None,
            })
            .expect("a Loaded event")
    }

    fn chips(seen: &Seen) -> Vec<(String, Option<Chip>)> {
        seen.borrow()
            .iter()
            .filter_map(|e| match e {
                Event::Widget(n, c) => Some((n.clone(), c.clone())),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_script_sets_the_bar() {
        let (_, seen) = session(r#"kestrel.bar { style = "pills", right = { "clock", "power" } }"#);
        let (bar, widgets) = loaded(&seen);
        assert_eq!(
            bar,
            Bar {
                style: Some(BarStyle::Pills),
                left: None,
                center: None,
                right: Some(vec!["clock".into(), "power".into()]),
            }
        );
        assert!(widgets.is_empty());
        assert!(errors(&seen).is_empty());
    }

    #[test]
    fn a_failing_script_leaves_the_bar_as_configured() {
        let (_, seen) = session(r#"kestrel.bar { left = { "clock" } } error("oops")"#);
        assert_eq!(loaded(&seen).0, Bar::default());
        let e = errors(&seen);
        assert_eq!(e.len(), 1);
        assert!(e[0].contains("oops"), "{e:?}");
    }

    #[test]
    fn an_unknown_bar_style_is_explained() {
        let (_, seen) = session(r#"kestrel.bar { style = "floating" }"#);
        let e = errors(&seen);
        assert!(
            e[0].contains("\"floating\"") && e[0].contains("pills"),
            "{e:?}"
        );
        assert_eq!(loaded(&seen).0, Bar::default());
    }

    #[test]
    fn a_script_that_never_ends_is_stopped_and_the_shell_goes_on() {
        let t = Instant::now();
        let (_, seen) = session("while true do end");
        assert!(t.elapsed() < CALL_LIMIT * 3);
        let e = errors(&seen);
        assert!(e[0].contains("took longer than a second"), "{e:?}");
        // It still reports in, with nothing from the script.
        assert_eq!(loaded(&seen), (Bar::default(), Vec::new()));
    }

    #[test]
    fn built_in_names_are_not_for_widgets() {
        let (_, seen) = session(r#"kestrel.widget("clock", { update = function() end })"#);
        assert!(errors(&seen)[0].contains("built-in module"));
        assert!(loaded(&seen).1.is_empty());
    }

    #[test]
    fn widgets_update_no_faster_than_twice_a_second() {
        let (s, _) = session(
            r#"kestrel.widget("fast", { interval = 0.01, update = function() return "x" end })
               kestrel.widget("slow", { update = function() return "y" end })"#,
        );
        assert_eq!(s.widgets[0].interval, Duration::from_millis(500));
        assert_eq!(
            s.widgets[1].interval,
            Duration::from_secs(10),
            "the default"
        );
    }

    #[test]
    fn a_widget_defined_again_replaces_the_first() {
        let (mut s, seen) = session(
            r#"kestrel.widget("w", { update = function() return "one" end })
               kestrel.widget("w", { update = function() return "two" end })"#,
        );
        assert_eq!(loaded(&seen).1, ["w"]);
        s.update_due(&emitter(&seen));
        assert_eq!(
            chips(&seen),
            [(
                "w".to_string(),
                Some(Chip {
                    text: "two".into(),
                    icon: None
                })
            )]
        );
    }

    #[test]
    fn what_an_update_returns_becomes_its_chip() {
        let (mut s, seen) = session(
            r#"kestrel.widget("t", { update = function() return { text = "42%", icon = "cpu" } end })
               kestrel.widget("n", { update = function() return nil end })
               kestrel.widget("none", {})"#,
        );
        s.update_due(&emitter(&seen));
        let c = chips(&seen);
        assert_eq!(c.len(), 2, "a widget without update says nothing: {c:?}");
        assert_eq!(c[0].1.as_ref().map(|c| c.text.as_str()), Some("42%"));
        assert_eq!(c[1], ("n".to_string(), None));
        // Not due again until its interval has passed.
        s.update_due(&emitter(&seen));
        assert_eq!(chips(&seen).len(), 2);
    }

    #[test]
    fn a_failing_widget_is_reported_once_until_it_works_again() {
        let (mut s, seen) = session(
            r#"broken = true
               kestrel.widget("w", { update = function()
                 if broken then error("no data") end
                 return "ok"
               end })"#,
        );
        let emit = emitter(&seen);
        s.update(0, &emit);
        s.update(0, &emit);
        assert_eq!(errors(&seen).len(), 1);
        assert!(errors(&seen)[0].starts_with("widget \"w\":"));
        s.lua.globals().set("broken", false).unwrap();
        s.update(0, &emit);
        assert_eq!(chips(&seen).last().unwrap().1.as_ref().unwrap().text, "ok");
        s.lua.globals().set("broken", true).unwrap();
        s.update(0, &emit);
        assert_eq!(errors(&seen).len(), 2, "failing again is news again");
    }

    #[test]
    fn a_widget_returning_a_number_is_told_what_to_return() {
        let (mut s, seen) = session(r#"kestrel.widget("w", { update = function() return 5 end })"#);
        s.update(0, &emitter(&seen));
        assert!(errors(&seen)[0].contains("return a table, a string or nil"));
    }

    #[test]
    fn a_runaway_update_is_stopped_and_the_next_call_gets_its_own_time() {
        let (mut s, seen) = session(
            r#"spin = true
               kestrel.widget("w", { update = function()
                 while spin do end
                 return "done"
               end })"#,
        );
        let emit = emitter(&seen);
        s.update(0, &emit);
        assert!(errors(&seen)[0].contains("took longer than a second"));
        s.lua.globals().set("spin", false).unwrap();
        s.update(0, &emit);
        assert_eq!(
            chips(&seen).last().unwrap().1.as_ref().unwrap().text,
            "done"
        );
    }

    #[test]
    fn handlers_get_the_events_fields() {
        let (mut s, seen) = session(
            r#"got = nil
               kestrel.on("notification", function(t) got = t.app .. ":" .. t.summary end)
               kestrel.on("battery", function(t) pct = t.percent end)"#,
        );
        let emit = emitter(&seen);
        s.dispatch(
            "notification",
            &[
                ("app", Val::Str("Mail".into())),
                ("summary", Val::Str("Hi".into())),
            ],
            &emit,
        );
        s.dispatch("battery", &[("percent", Val::Num(41.0))], &emit);
        let g = s.lua.globals();
        assert_eq!(g.get::<String>("got").unwrap(), "Mail:Hi");
        assert_eq!(g.get::<f64>("pct").unwrap(), 41.0);
        assert!(errors(&seen).is_empty());
    }

    #[test]
    fn only_known_events_can_be_listened_to() {
        let (_, seen) = session(r#"kestrel.on("keypress", function() end)"#);
        let e = errors(&seen);
        assert!(e[0].contains("no event \"keypress\""), "{e:?}");
        assert!(e[0].contains("workspace"), "the events are listed: {e:?}");
    }

    #[test]
    fn a_failing_handler_is_reported_once_and_the_others_still_run() {
        let (mut s, seen) = session(
            r#"n = 0
               kestrel.on("media", function() error("bad") end)
               kestrel.on("media", function() n = n + 1 end)"#,
        );
        let emit = emitter(&seen);
        for _ in 0..3 {
            s.dispatch("media", &[("playing", Val::Bool(true))], &emit);
        }
        assert_eq!(errors(&seen).len(), 1);
        assert!(errors(&seen)[0].starts_with("kestrel.on(\"media\")"));
        assert_eq!(s.lua.globals().get::<i64>("n").unwrap(), 3);
    }

    #[test]
    fn a_click_runs_its_command_or_function_and_updates_the_widget() {
        let (mut s, seen) = session(
            r#"clicks = 0
               kestrel.widget("cmd", { click = "kitty -e btop", update = function() return "c" end })
               kestrel.widget("fn", {
                 click = function() clicks = clicks + 1 end,
                 update = function() return tostring(clicks) end,
               })"#,
        );
        let emit = emitter(&seen);
        s.click("cmd", &emit);
        assert!(
            seen.borrow()
                .iter()
                .any(|e| matches!(e, Event::Exec(c) if c == "kitty -e btop"))
        );
        s.click("fn", &emit);
        assert_eq!(s.lua.globals().get::<i64>("clicks").unwrap(), 1);
        assert_eq!(chips(&seen).last().unwrap().1.as_ref().unwrap().text, "1");
        // A widget that is not there: nothing.
        let before = seen.borrow().len();
        s.click("gone", &emit);
        assert_eq!(seen.borrow().len(), before);
    }

    #[test]
    fn the_scripts_messages_reach_the_shell() {
        let (_, seen) = session(
            r#"kestrel.notify("Title")
               kestrel.notify("T2", "body")
               kestrel.island { icon = "cpu", title = "Hot" }
               kestrel.exec("kitty")"#,
        );
        let seen = seen.borrow();
        assert!(matches!(&seen[0], Event::Notify(t, b) if t == "Title" && b.is_empty()));
        assert!(matches!(&seen[1], Event::Notify(t, b) if t == "T2" && b == "body"));
        assert!(
            matches!(&seen[2], Event::Island(Some(i), t, d) if i == "cpu" && t == "Hot" && d.is_empty())
        );
        assert!(matches!(&seen[3], Event::Exec(c) if c == "kitty"));
    }

    /// A plugin in a folder of its own under the temporary directory (removed by the caller).
    fn plugin(id: &str, source: &str, settings: &str) -> Plugin {
        let dir =
            std::env::temp_dir().join(format!("kestrel-lua-test-{}-{id}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("main.lua"), source).unwrap();
        Plugin {
            id: id.into(),
            dir,
            main: "main.lua".into(),
            settings: settings.parse().unwrap(),
        }
    }

    #[test]
    fn plugins_keep_their_globals_to_themselves() {
        let a = plugin(
            "a",
            r#"mine = config.path .. #config.list
               kestrel.widget("a-w", { update = function() return mine end })"#,
            "path = \"/\"\nlist = [1, 2, 3]\n[nested]\nx = true",
        );
        let b = plugin(
            "b",
            r#"if mine ~= nil then error("saw a's global") end"#,
            "",
        );
        let dirs = [a.dir.clone(), b.dir.clone()];
        let (mut s, seen) = session_with("", vec![a, b]);
        for d in dirs {
            let _ = std::fs::remove_dir_all(d);
        }
        assert!(errors(&seen).is_empty(), "{:?}", errors(&seen));
        assert!(s.lua.globals().get::<Value>("mine").unwrap().is_nil());
        s.update_due(&emitter(&seen));
        assert_eq!(chips(&seen)[0].1.as_ref().unwrap().text, "/3");
    }

    #[test]
    fn a_broken_plugin_is_named_and_the_rest_still_load() {
        let bad = plugin("bad", "this is not lua", "");
        let good = plugin("good", r#"kestrel.widget("g", {})"#, "");
        let missing = Plugin {
            main: "absent.lua".into(),
            ..plugin("missing", "", "")
        };
        let dirs = [bad.dir.clone(), good.dir.clone(), missing.dir.clone()];
        let (_, seen) = session_with("", vec![bad, missing, good]);
        for d in dirs {
            let _ = std::fs::remove_dir_all(d);
        }
        let e = errors(&seen);
        assert_eq!(e.len(), 2, "{e:?}");
        assert!(e[0].starts_with("plugin bad:"));
        assert!(e[1].starts_with("plugin missing: absent.lua"));
        assert_eq!(loaded(&seen).1, ["g"]);
    }

    #[test]
    fn a_bar_replaces_only_the_sections_it_names() {
        let mut b = crate::config::BarConfig::default();
        let before = b.clone();
        Bar {
            center: Some(vec!["clock".into()]),
            ..Bar::default()
        }
        .apply(&mut b);
        assert_eq!(b.center, ["clock"]);
        assert_eq!(
            (b.left, b.right, b.style),
            (before.left, before.right, before.style)
        );
    }

    #[test]
    fn errors_are_cut_to_their_first_line() {
        let e = mlua::Error::runtime("bad thing\nstack traceback:\n  [C]: in ?");
        assert_eq!(short(&e), "bad thing");
    }
}
