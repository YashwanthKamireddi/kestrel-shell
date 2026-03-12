//! kestrel's own authentication agent: when an app asks for administrator rights (Timeshift,
//! a package tool, mounting a disk), polkit asks the session's agent to check the user's
//! password. This is that agent, so kestrel shows the prompt itself (no KDE agent).
//!
//! Registered with polkit for this login session on the system bus. A request becomes a
//! `Request` for the shell; the shell answers with the password (`respond`) or a cancel.
//! The password is checked by polkit's own helper (`/run/polkit/agent-helper.socket`, the
//! socket-activated `polkit-agent-helper-1`): kestrel never sees whether a password is right
//! except through it, and keeps nothing.

use futures::{SinkExt, Stream};
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;
use zbus::interface;
use zbus::zvariant::{OwnedValue, Value};

const AGENT_PATH: &str = "/org/kestrel/PolicyKit1/AuthenticationAgent";
const HELPER: &str = "/run/polkit/agent-helper.socket";

/// Something asking for the password.
#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    pub cookie: String,
    /// polkit's sentence ("Authentication is required to …").
    pub message: String,
    pub action: String,
    /// Whose password is asked for.
    pub user: String,
    /// The previous try was wrong.
    pub retry: bool,
}

/// What the shell hears: a request, or that one is over (done or cancelled elsewhere).
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Ask(Request),
    Done(String),
}

enum Answer {
    Password(String),
    Cancel,
}

static WAITING: LazyLock<Mutex<HashMap<String, mpsc::UnboundedSender<Answer>>>> =
    LazyLock::new(Default::default);

/// The user typed the password for request `cookie`.
pub fn respond(cookie: &str, password: String) {
    if let Some(tx) = WAITING.lock().ok().and_then(|w| w.get(cookie).cloned()) {
        let _ = tx.send(Answer::Password(password));
    }
}

/// The user cancelled request `cookie`.
pub fn cancel(cookie: &str) {
    if let Some(tx) = WAITING.lock().ok().and_then(|w| w.get(cookie).cloned()) {
        let _ = tx.send(Answer::Cancel);
    }
}

/// The user name of a uid.
fn user_name(uid: u32) -> Option<String> {
    // Safety: getpwuid returns a pointer into static storage or null; it is read at once.
    unsafe {
        let pw = libc::getpwuid(uid);
        if pw.is_null() {
            return None;
        }
        Some(
            std::ffi::CStr::from_ptr((*pw).pw_name)
                .to_string_lossy()
                .into_owned(),
        )
    }
}

/// Checks `password` for `user` with polkit's helper; true when accepted (the helper then
/// tells polkit itself).
async fn check(user: &str, cookie: &str, password: &str) -> std::io::Result<bool> {
    let s = tokio::net::UnixStream::connect(HELPER).await?;
    let (r, mut w) = s.into_split();
    w.write_all(format!("{user}\n{cookie}\n").as_bytes())
        .await?;
    let mut lines = BufReader::new(r).lines();
    while let Some(line) = lines.next_line().await? {
        if line.starts_with("PAM_PROMPT_ECHO_OFF") || line.starts_with("PAM_PROMPT_ECHO_ON") {
            w.write_all(format!("{password}\n").as_bytes()).await?;
        } else if line.starts_with("SUCCESS") {
            return Ok(true);
        } else if line.starts_with("FAILURE") {
            return Ok(false);
        }
        // PAM_TEXT_INFO / PAM_ERROR_MSG: nothing to answer.
    }
    Ok(false)
}

struct Agent {
    tx: mpsc::UnboundedSender<Event>,
}

#[interface(name = "org.freedesktop.PolicyKit1.AuthenticationAgent")]
impl Agent {
    async fn begin_authentication(
        &self,
        action_id: String,
        message: String,
        _icon_name: String,
        _details: HashMap<String, String>,
        cookie: String,
        identities: Vec<(String, HashMap<String, OwnedValue>)>,
    ) -> zbus::fdo::Result<()> {
        // The identity to ask for: the user themself when they are one of them (an admin in
        // wheel), else the first user listed.
        let me = unsafe { libc::getuid() };
        let uids: Vec<u32> = identities
            .iter()
            .filter(|(kind, _)| kind == "unix-user")
            .filter_map(|(_, d)| d.get("uid").and_then(|v| u32::try_from(v.clone()).ok()))
            .collect();
        let uid = if uids.contains(&me) {
            me
        } else {
            *uids
                .first()
                .ok_or_else(|| zbus::fdo::Error::Failed("no user to authenticate as".into()))?
        };
        let user = user_name(uid).ok_or_else(|| zbus::fdo::Error::Failed("unknown user".into()))?;
        let (atx, mut arx) = mpsc::unbounded_channel();
        if let Ok(mut w) = WAITING.lock() {
            w.insert(cookie.clone(), atx);
        }
        let mut retry = false;
        let result = loop {
            let _ = self.tx.send(Event::Ask(Request {
                cookie: cookie.clone(),
                message: message.clone(),
                action: action_id.clone(),
                user: user.clone(),
                retry,
            }));
            match arx.recv().await {
                Some(Answer::Password(p)) => match check(&user, &cookie, &p).await {
                    Ok(true) => break Ok(()),
                    Ok(false) => retry = true,
                    Err(e) => {
                        log::warn!("polkit: helper: {e}");
                        break Err(zbus::fdo::Error::Failed(format!("helper: {e}")));
                    }
                },
                _ => {
                    break Err(zbus::fdo::Error::Failed(
                        "org.freedesktop.PolicyKit1.Error.Cancelled".into(),
                    ));
                }
            }
        };
        if let Ok(mut w) = WAITING.lock() {
            w.remove(&cookie);
        }
        let _ = self.tx.send(Event::Done(cookie));
        result
    }

    async fn cancel_authentication(&self, cookie: String) {
        cancel(&cookie);
    }
}

/// Registers the agent for this session and streams its requests. Fails quietly (logged)
/// when another agent already holds the session.
pub fn stream() -> impl Stream<Item = Event> {
    crate::stream::channel(4, async |mut out| {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let conn = async {
            let c = zbus::connection::Builder::system()?
                .serve_at(AGENT_PATH, Agent { tx })?
                .build()
                .await?;
            let session = std::env::var("XDG_SESSION_ID").unwrap_or_default();
            let mut details: HashMap<&str, Value> = HashMap::new();
            details.insert("session-id", Value::from(session.as_str()));
            let subject = ("unix-session", details);
            let locale = std::env::var("LANG").unwrap_or_else(|_| "en_US.UTF-8".into());
            c.call_method(
                Some("org.freedesktop.PolicyKit1"),
                "/org/freedesktop/PolicyKit1/Authority",
                Some("org.freedesktop.PolicyKit1.Authority"),
                "RegisterAuthenticationAgent",
                &(subject, locale.as_str(), AGENT_PATH),
            )
            .await?;
            Ok::<_, zbus::Error>(c)
        }
        .await;
        let _conn = match conn {
            Ok(c) => {
                log::info!("polkit: kestrel is the authentication agent");
                c
            }
            Err(e) => {
                log::warn!("polkit: not registered ({e})");
                return;
            }
        };
        while let Some(e) = rx.recv().await {
            if out.send(e).await.is_err() {
                return;
            }
        }
    })
}
