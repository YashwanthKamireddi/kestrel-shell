//! The secret agent. NetworkManager asks a secret agent when a connection needs a password it
//! does not have: a saved network whose password changed (it asks again, "request new"), or
//! one saved without its password. With no agent it gives up at once and the attempt fails
//! ("no secrets"); with kestrel registered, the person is asked and NetworkManager saves the
//! answer.

use super::*;

pub(super) const AGENT_PATH: &str = "/org/freedesktop/NetworkManager/SecretAgent";

/// A password NetworkManager needs.
#[derive(Clone, Debug, PartialEq)]
pub enum Ask {
    /// The Wi-Fi password for `ssid`; `wrong`: the one it had did not work.
    Password { id: u64, ssid: String, wrong: bool },
    /// The question is over (answered, or NetworkManager stopped waiting).
    Done(u64),
}

impl Ask {
    pub fn id(&self) -> u64 {
        match self {
            Self::Password { id, .. } | Self::Done(id) => *id,
        }
    }
}

pub(super) type Waiting =
    std::sync::Mutex<HashMap<u64, (String, tokio::sync::oneshot::Sender<Option<String>>)>>;
pub(super) static WAITING: std::sync::LazyLock<Waiting> =
    std::sync::LazyLock::new(Default::default);

pub(super) static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// The person typed `password` for question `id` (None: cancelled).
pub fn answer(id: u64, password: Option<String>) {
    if let Some((_, tx)) = WAITING.lock().ok().and_then(|mut w| w.remove(&id)) {
        let _ = tx.send(password);
    }
}

#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.freedesktop.NetworkManager.SecretAgent")]
pub(super) enum AgentError {
    #[zbus(error)]
    ZBus(zbus::Error),
    UserCanceled(String),
    NoSecrets(String),
}

pub(super) struct SecretAgent {
    tx: tokio::sync::mpsc::UnboundedSender<Ask>,
}

pub(super) type Settings = HashMap<String, HashMap<String, OwnedValue>>;

#[zbus::interface(name = "org.freedesktop.NetworkManager.SecretAgent")]
impl SecretAgent {
    async fn get_secrets(
        &self,
        connection: Settings,
        connection_path: OwnedObjectPath,
        setting_name: String,
        _hints: Vec<String>,
        flags: u32,
    ) -> Result<Settings, AgentError> {
        // ALLOW_INTERACTION 0x1, REQUEST_NEW 0x2.
        const ALLOW: u32 = 0x1;
        const NEW: u32 = 0x2;
        let wifi = connection.get("802-11-wireless");
        let mgmt = connection
            .get("802-11-wireless-security")
            .and_then(|s| prop::<String>(s, "key-mgmt"))
            .unwrap_or_default();
        // A personal Wi-Fi password (WPA/WPA2/WPA3); enterprise logins and VPNs are not asked
        // for here.
        if setting_name != "802-11-wireless-security"
            || !matches!(mgmt.as_str(), "wpa-psk" | "sae")
            || flags & ALLOW == 0
        {
            return Err(AgentError::NoSecrets("not asked by kestrel".into()));
        }
        let ssid = wifi
            .and_then(|w| prop::<Vec<u8>>(w, "ssid"))
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .unwrap_or_default();
        let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let (tx, rx) = tokio::sync::oneshot::channel();
        if let Ok(mut w) = WAITING.lock() {
            w.insert(id, (connection_path.to_string(), tx));
        }
        let _ = self.tx.send(Ask::Password {
            id,
            ssid,
            wrong: flags & NEW != 0,
        });
        let reply = rx.await.ok().flatten();
        let _ = self.tx.send(Ask::Done(id));
        match reply {
            Some(psk) => {
                let mut sec = HashMap::new();
                sec.insert(
                    "psk".to_string(),
                    OwnedValue::try_from(Value::from(psk)).map_err(zbus::Error::from)?,
                );
                let mut out = HashMap::new();
                out.insert("802-11-wireless-security".to_string(), sec);
                Ok(out)
            }
            None => Err(AgentError::UserCanceled("cancelled".into())),
        }
    }

    /// NetworkManager stopped waiting (the attempt was cancelled or timed out): the question
    /// goes.
    async fn cancel_get_secrets(&self, connection_path: OwnedObjectPath, _setting_name: String) {
        let ids: Vec<u64> = WAITING
            .lock()
            .map(|w| {
                w.iter()
                    .filter(|(_, (p, _))| p == connection_path.as_str())
                    .map(|(id, _)| *id)
                    .collect()
            })
            .unwrap_or_default();
        for id in ids {
            answer(id, None);
        }
    }

    /// NetworkManager keeps system connections' passwords itself: nothing to store here.
    async fn save_secrets(&self, _connection: Settings, _connection_path: OwnedObjectPath) {}

    async fn delete_secrets(&self, _connection: Settings, _connection_path: OwnedObjectPath) {}
}

/// Registers kestrel as NetworkManager's secret agent and streams the passwords it needs.
/// Registered again whenever NetworkManager restarts (it forgets its agents).
pub fn agent() -> impl Stream<Item = Ask> {
    crate::stream::channel(8, async |mut out| {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let Ok(c) = Connection::system().await else {
            return;
        };
        if let Err(e) = c.object_server().at(AGENT_PATH, SecretAgent { tx }).await {
            log::warn!("network: no secret agent ({e})");
            return;
        }
        // The test rig's shell shares the system bus: it serves the agent (to be called
        // directly) but does not take the desktop's prompts.
        let register = std::env::var_os("KESTREL_NO_BT_AGENT").is_none();
        let reg = |c: Connection| async move {
            let r: zbus::Result<()> = dbus::proxy(
                &c,
                NM,
                "/org/freedesktop/NetworkManager/AgentManager",
                "org.freedesktop.NetworkManager.AgentManager",
            )
            .await?
            .call("RegisterWithCapabilities", &("dev.kestrel.shell", 0u32))
            .await;
            r
        };
        if register {
            match reg(c.clone()).await {
                Ok(()) => log::info!("network: kestrel is the secret agent"),
                Err(e) => log::warn!("network: secret agent not registered ({e})"),
            }
        }
        // NetworkManager restarting drops every agent: registered again when its name comes
        // back.
        let owner = async {
            let Ok(dbus) = zbus::fdo::DBusProxy::new(&c).await else {
                return std::future::pending::<()>().await;
            };
            let Ok(mut changes) = dbus.receive_name_owner_changed_with_args(&[(0, NM)]).await
            else {
                return std::future::pending::<()>().await;
            };
            while let Some(ch) = changes.next().await {
                if register
                    && let Ok(a) = ch.args()
                    && a.new_owner().is_some()
                {
                    let _ = reg(c.clone()).await;
                }
            }
        };
        let forward = async {
            while let Some(a) = rx.recv().await {
                if out.send(a).await.is_err() {
                    return;
                }
            }
        };
        tokio::select! {
            () = owner => {}
            () = forward => {}
        }
    })
}
