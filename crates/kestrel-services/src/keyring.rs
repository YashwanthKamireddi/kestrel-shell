//! The keyring's password prompts, answered by kestrel: the system prompter of GNOME Keyring
//! (`org.gnome.keyring.SystemPrompter`), so unlocking a keyring, or making one, asks in
//! kestrel's own window instead of GTK's `gcr-prompter`. GNOME Shell does the same: it owns
//! that name and draws the prompts itself.
//!
//! The protocol is gcr's (gcr/gcr-system-prompter.c, gcr-dbus-constants.h): the keyring daemon
//! calls `BeginPrompting(callback)`; the prompter answers on the callback with
//! `PromptReady("", {}, exchange)`; the daemon asks with `PerformPrompt(callback, type,
//! properties, exchange)`; the prompter shows it and answers `PromptReady(reply, changed
//! properties, exchange)` (the password inside the exchange, encrypted); again on a retry, until
//! `StopPrompting(callback)`, answered with `PromptDone()`.
//!
//! The password never crosses the bus in the clear: it travels in gcr's secret exchange
//! ("sx-aes-1", gcr/gcr-secret-exchange.c): Diffie-Hellman over the 1536-bit IKE group, the
//! shared secret (big-endian, no leading zeros) through HKDF-SHA256 with no salt
//! and no info into a 16-byte key, then AES-128-CBC with PKCS#7 padding and a fresh 16-byte IV
//! per message, all base64 in a key file `[sx-aes-1]` with `public`, `secret` and `iv`.

use crypto_bigint::{
    Odd, U1536,
    modular::{FixedMontyForm, FixedMontyParams},
};
use futures::{SinkExt, Stream};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use zbus::{
    Connection,
    message::Header,
    zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value},
};

const NAME: &str = "org.gnome.keyring.SystemPrompter";
const PATH: &str = "/org/gnome/keyring/Prompter";
const CALLBACK: &str = "org.gnome.keyring.internal.Prompter.Callback";

// --- The secret exchange (sx-aes-1) ----------------------------------------------------------

/// The 1536-bit MODP group of RFC 3526 (group 5): "ietf-ike-grp-modp-1536", generator 2.
const PRIME: &str = "FFFFFFFFFFFFFFFFC90FDAA22168C234C4C6628B80DC1CD129024E088A67CC74020BBEA63B139B22514A08798E3404DDEF9519B3CD3A431B302B0A6DF25F14374FE1356D6D51C245E485B576625E7EC6F44C42E9A637ED6B0BFF5CB6F406B7EDEE386BFB5A899FA5AE9F24117C4B1FE649286651ECE45B3DC2007CB8A163BF0598DA48361C55D39A69163FA8FD24CF5F83655D23DCA3AD961C62F356208552BB9ED529077096966D670C354E4ABC9804F1746C08CA237327FFFFFFFFFFFFFFFF";
const GROUP: &str = "sx-aes-1";

/// One side of a secret exchange: a key pair, and the key shared with the other side once its
/// public key has arrived.
pub struct Exchange {
    private: U1536,
    public: U1536,
    key: Option<[u8; 16]>,
    #[cfg(test)]
    seen: (usize, u8),
}

impl Exchange {
    pub fn new() -> Option<Self> {
        let params = params();
        // A random exponent below p / 2 (as gcr's: the prime's bits, the top one cleared).
        let mut bytes = [0u8; 192];
        getrandom::fill(&mut bytes).ok()?;
        bytes[0] &= 0x7f;
        let private = U1536::from_be_slice(&bytes);
        if private == U1536::ZERO {
            return None;
        }
        let g = FixedMontyForm::new(&U1536::from_u8(2), &params);
        let public = g.pow(&private).retrieve();
        Some(Self {
            private,
            public,
            key: None,
            #[cfg(test)]
            seen: (0, 0),
        })
    }

    /// The first message: this side's public key only.
    pub fn begin(&self) -> String {
        render(&[("public", &unpadded(&self.public))])
    }

    /// Reads the other side's message: its public key (the shared key is made from the first
    /// one) and, when it carries one, the secret. Err when it is not a valid exchange.
    pub fn receive(&mut self, message: &str) -> Result<Option<Vec<u8>>, &'static str> {
        let fields = parse(message).ok_or("not a secret exchange")?;
        if self.key.is_none() {
            let peer = fields.get("public").ok_or("no public key")?;
            // Leading zeros are the same number (gcr 4's GnuTLS writes a sign byte: 193 bytes).
            let peer = &peer[peer.iter().position(|&b| b != 0).unwrap_or(peer.len())..];
            if peer.is_empty() || peer.len() > 192 {
                return Err("bad public key");
            }
            let mut b = [0u8; 192];
            b[192 - peer.len()..].copy_from_slice(peer);
            let peer = U1536::from_be_slice(&b);
            let p = U1536::from_be_hex(PRIME);
            // 1 < peer < p - 1 (a small-subgroup value would make the key guessable).
            if peer <= U1536::ONE || peer >= p.wrapping_sub(&U1536::ONE) {
                return Err("bad public key");
            }
            let shared = FixedMontyForm::new(&peer, &params())
                .pow(&self.private)
                .retrieve();
            // Without leading zeros: what gcr 3.41 (libgcrypt, the keyring daemon's) and gcr 4.4
            // (GnuTLS, GNOME Shell's prompter) both derive from, measured against both
            // (`interoperates_with_gcr`); gcr's newest source pads to the prime's length, which
            // no installed gcr does (a padded key failed 1 exchange in 256 against both).
            let ikm = unpadded(&shared);
            #[cfg(test)]
            {
                self.seen = (
                    fields.get("public").map_or(0, Vec::len),
                    shared.to_be_bytes().as_ref()[0],
                );
            }
            let hk = hkdf::Hkdf::<sha2::Sha256>::new(None, &ikm);
            let mut key = [0u8; 16];
            hk.expand(&[], &mut key).map_err(|_| "key")?;
            self.key = Some(key);
        }
        let Some(secret) = fields.get("secret") else {
            return Ok(None);
        };
        let iv = fields.get("iv").ok_or("no iv")?;
        if iv.len() != 16 || secret.is_empty() || secret.len() % 16 != 0 {
            return Err("bad secret");
        }
        let key = self.key.ok_or("no key")?;
        use cbc::cipher::{BlockModeDecrypt, KeyIvInit, block_padding::Pkcs7};
        let iv: [u8; 16] = iv.as_slice().try_into().map_err(|_| "bad iv")?;
        cbc::Decryptor::<aes::Aes128>::new(&key.into(), &iv.into())
            .decrypt_padded_vec::<Pkcs7>(secret)
            .map(Some)
            .map_err(|_| "bad secret")
    }

    /// A message to the other side: this side's public key, and `secret` encrypted when given.
    /// None before the other side's key has arrived.
    pub fn send(&self, secret: Option<&[u8]>) -> Option<String> {
        let key = self.key?;
        let public = unpadded(&self.public);
        let Some(secret) = secret else {
            return Some(render(&[("public", &public)]));
        };
        let mut iv = [0u8; 16];
        getrandom::fill(&mut iv).ok()?;
        use cbc::cipher::{BlockModeEncrypt, KeyIvInit, block_padding::Pkcs7};
        let sealed = cbc::Encryptor::<aes::Aes128>::new(&key.into(), &iv.into())
            .encrypt_padded_vec::<Pkcs7>(secret);
        Some(render(&[
            ("public", &public),
            ("secret", &sealed),
            ("iv", &iv),
        ]))
    }
}

impl Exchange {
    #[cfg(test)]
    fn debug_shared_top(&self) -> (usize, u8) {
        self.seen
    }
}

fn params() -> FixedMontyParams<{ U1536::LIMBS }> {
    FixedMontyParams::new(Odd::<U1536>::from_be_hex(PRIME))
}

/// A number's big-endian bytes without leading zeros (libgcrypt's unsigned format).
fn unpadded(n: &U1536) -> Vec<u8> {
    let b = n.to_be_bytes();
    let b = b.as_ref();
    let first = b.iter().position(|&x| x != 0).unwrap_or(b.len() - 1);
    b[first..].to_vec()
}

/// The exchange's key file: `[sx-aes-1]`, then `key=base64` lines (as GKeyFile writes it).
fn render(fields: &[(&str, &[u8])]) -> String {
    use base64::Engine;
    let mut out = format!("[{GROUP}]\n");
    for (k, v) in fields {
        out.push_str(k);
        out.push('=');
        out.push_str(&base64::engine::general_purpose::STANDARD.encode(v));
        out.push('\n');
    }
    out
}

/// Reads a key file's `[sx-aes-1]` group: its fields, base64-decoded.
fn parse(message: &str) -> Option<HashMap<String, Vec<u8>>> {
    use base64::Engine;
    let mut lines = message.lines().map(str::trim).filter(|l| !l.is_empty());
    if lines.next()? != format!("[{GROUP}]") {
        return None;
    }
    let mut out = HashMap::new();
    for l in lines {
        if l.starts_with('[') {
            break;
        }
        if l.starts_with('#') {
            continue;
        }
        let (k, v) = l.split_once('=')?;
        let v = base64::engine::general_purpose::STANDARD
            .decode(v.trim())
            .ok()?;
        out.insert(k.trim().to_string(), v);
    }
    Some(out)
}

// --- The prompter ------------------------------------------------------------------------------

/// What a prompt asks: a password (unlock, or a new one) or a yes/no.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    Password,
    Confirm,
}

/// A question for the window, and its end.
#[derive(Clone, Debug)]
pub enum Ask {
    Prompt(Prompt),
    /// The question is over (the keyring stopped prompting, or the asker went away).
    Done(u64),
}

/// What the keyring asks, in its own words (gcr's prompt properties).
#[derive(Clone, Debug, Default)]
pub struct Prompt {
    pub id: u64,
    pub kind: Option<Kind>,
    pub title: String,
    pub message: String,
    pub description: String,
    /// Why it asks again ("The unlock password was incorrect").
    pub warning: String,
    /// A new password is being chosen (asked twice).
    pub new_password: bool,
    /// A check box ("Automatically unlock this keyring whenever I'm logged in"), and its state.
    pub choice: Option<(String, bool)>,
    pub continue_label: String,
    pub cancel_label: String,
}

/// The answer: the password (or "yes") and the check box, or None to cancel.
pub struct Reply {
    pub password: String,
    pub choice: bool,
}

type Answer = tokio::sync::oneshot::Sender<Option<Reply>>;
static WAITING: std::sync::LazyLock<Mutex<HashMap<u64, Answer>>> =
    std::sync::LazyLock::new(Default::default);
static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// The person answered question `id` (None: cancelled).
pub fn answer(id: u64, reply: Option<Reply>) {
    if let Some(tx) = WAITING.lock().ok().and_then(|mut w| w.remove(&id)) {
        let _ = tx.send(reply);
    }
}

/// One asker's prompt (a keyring daemon's callback object).
struct Active {
    sender: String,
    callback: OwnedObjectPath,
    exchange: Exchange,
    prompt: Prompt,
    /// Ready for the next PerformPrompt (gcr: one at a time).
    ready: bool,
}

#[derive(Default)]
struct Inner {
    active: Option<Active>,
    /// Askers waiting their turn (one prompt is shown at a time, as gcr's single mode).
    waiting: std::collections::VecDeque<(String, OwnedObjectPath)>,
}

struct Prompter {
    inner: Arc<Mutex<Inner>>,
    tx: tokio::sync::mpsc::UnboundedSender<Ask>,
}

#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.gnome.keyring.Prompter")]
enum PromptError {
    #[zbus(error)]
    ZBus(zbus::Error),
    InProgress(String),
    Failed(String),
}

#[zbus::interface(name = "org.gnome.keyring.internal.Prompter")]
impl Prompter {
    async fn begin_prompting(
        &self,
        #[zbus(header)] hdr: Header<'_>,
        #[zbus(connection)] conn: &Connection,
        callback: OwnedObjectPath,
    ) -> Result<(), PromptError> {
        let sender = sender(&hdr)?;
        {
            let mut g = self.inner.lock().map_err(|_| failed("busy"))?;
            let same = |s: &str, c: &OwnedObjectPath| s == sender && *c == callback;
            if g.active
                .as_ref()
                .is_some_and(|a| same(&a.sender, &a.callback))
                || g.waiting.iter().any(|(s, c)| same(s, c))
            {
                return Err(PromptError::InProgress(
                    "Already begun prompting for this prompt callback".into(),
                ));
            }
            g.waiting.push_back((sender, callback));
        }
        next(self.inner.clone(), conn.clone());
        Ok(())
    }

    async fn perform_prompt(
        &self,
        #[zbus(header)] hdr: Header<'_>,
        #[zbus(connection)] conn: &Connection,
        callback: OwnedObjectPath,
        kind: String,
        properties: HashMap<String, OwnedValue>,
        exchange: String,
    ) -> Result<(), PromptError> {
        let sender = sender(&hdr)?;
        let kind = match kind.as_str() {
            "password" => Kind::Password,
            "confirm" => Kind::Confirm,
            _ => return Err(failed("Invalid type argument")),
        };
        let (prompt, rx) = {
            let mut g = self.inner.lock().map_err(|_| failed("busy"))?;
            let a = g
                .active
                .as_mut()
                .filter(|a| a.sender == sender && a.callback == callback)
                .ok_or_else(|| failed("Not begun prompting for this prompt callback"))?;
            if !a.ready {
                return Err(failed(
                    "Already performing a prompt for this prompt callback",
                ));
            }
            a.exchange
                .receive(&exchange)
                .map_err(|_| failed("Invalid secret exchange"))?;
            apply(&mut a.prompt, &properties);
            a.prompt.kind = Some(kind);
            a.prompt.id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            a.ready = false;
            let (tx, rx) = tokio::sync::oneshot::channel();
            if let Ok(mut w) = WAITING.lock() {
                w.insert(a.prompt.id, tx);
            }
            (a.prompt.clone(), rx)
        };
        let _ = self.tx.send(Ask::Prompt(prompt.clone()));
        // Answered later on the callback (PerformPrompt itself returns at once, as gcr's).
        // The window stays (showing it is being checked) until the keyring stops prompting
        // or asks again (a wrong password: the next PerformPrompt, with its warning).
        let inner = self.inner.clone();
        let conn = conn.clone();
        tokio::spawn(async move {
            let reply = rx.await.ok().flatten();
            reply_ready(&inner, &conn, prompt.id, reply).await;
        });
        Ok(())
    }

    async fn stop_prompting(
        &self,
        #[zbus(header)] hdr: Header<'_>,
        #[zbus(connection)] conn: &Connection,
        callback: OwnedObjectPath,
    ) -> Result<(), PromptError> {
        let sender = sender(&hdr)?;
        let stopped = {
            let mut g = self.inner.lock().map_err(|_| failed("busy"))?;
            g.waiting.retain(|(s, c)| !(*s == sender && *c == callback));
            match &g.active {
                Some(a) if a.sender == sender && a.callback == callback => g.active.take(),
                _ => None,
            }
        };
        if let Some(a) = stopped {
            // A question still shown goes (the keyring gave up on it).
            answer(a.prompt.id, None);
            let _ = self.tx.send(Ask::Done(a.prompt.id));
            let _ = call(&conn.clone(), &a.sender, &a.callback, "PromptDone", &()).await;
        }
        next(self.inner.clone(), conn.clone());
        Ok(())
    }
}

fn failed(why: &str) -> PromptError {
    PromptError::Failed(why.into())
}

fn sender(hdr: &Header<'_>) -> Result<String, PromptError> {
    hdr.sender()
        .map(ToString::to_string)
        .ok_or_else(|| failed("no sender"))
}

/// Calls the asker's callback object.
async fn call<B>(
    conn: &Connection,
    to: &str,
    path: &OwnedObjectPath,
    method: &str,
    body: &B,
) -> zbus::Result<()>
where
    B: serde::Serialize + zbus::zvariant::DynamicType,
{
    conn.call_method(
        Some(to),
        ObjectPath::from(path.clone()),
        Some(CALLBACK),
        method,
        body,
    )
    .await
    .map(|_| ())
}

/// The next asker's turn, if none is being asked: its exchange begins (an empty PromptReady).
fn next(inner: Arc<Mutex<Inner>>, conn: Connection) {
    let start = {
        let Ok(mut g) = inner.lock() else { return };
        if g.active.is_some() {
            return;
        }
        let Some((sender, callback)) = g.waiting.pop_front() else {
            return;
        };
        let Some(exchange) = Exchange::new() else {
            log::warn!("keyring: no random numbers for the exchange");
            return;
        };
        let begin = exchange.begin();
        g.active = Some(Active {
            sender: sender.clone(),
            callback: callback.clone(),
            exchange,
            prompt: Prompt::default(),
            ready: true,
        });
        (sender, callback, begin)
    };
    tokio::spawn(async move {
        let (sender, callback, begin) = start;
        let props: HashMap<&str, Value<'_>> = HashMap::new();
        if let Err(e) = call(
            &conn,
            &sender,
            &callback,
            "PromptReady",
            &("", props, begin),
        )
        .await
        {
            // The asker went away: its turn ends, the next one's begins.
            log::warn!("keyring: {e}");
            if let Ok(mut g) = inner.lock()
                && g.active.as_ref().is_some_and(|a| a.callback == callback)
            {
                g.active = None;
            }
            next(inner, conn);
        }
    });
}

/// The person answered: the reply goes to the asker, the password inside the exchange.
async fn reply_ready(inner: &Arc<Mutex<Inner>>, conn: &Connection, id: u64, reply: Option<Reply>) {
    let msg = {
        let Ok(mut g) = inner.lock() else { return };
        let Some(a) = g.active.as_mut().filter(|a| a.prompt.id == id) else {
            return;
        };
        let mut changed: HashMap<&str, Value<'_>> = HashMap::new();
        let (word, secret) = match (&reply, &a.prompt.kind) {
            (None, _) => ("no", None),
            (Some(r), Some(Kind::Password)) => ("yes", Some(r.password.as_bytes().to_vec())),
            (Some(_), _) => ("yes", None),
        };
        if let (Some(r), Some((_, was))) = (&reply, &a.prompt.choice)
            && r.choice != *was
        {
            // Wrapped twice, as gcr's prompter sends every property (g_variant_new_variant
            // inside the a{sv}; its client unwraps twice: gcr-system-prompter.c,
            // prompt_build_properties).
            changed.insert(
                "choice-chosen",
                Value::Value(Box::new(Value::from(r.choice))),
            );
        }
        let Some(exchange) = a.exchange.send(secret.as_deref()) else {
            return;
        };
        a.ready = true;
        (
            a.sender.clone(),
            a.callback.clone(),
            word,
            changed
                .into_iter()
                .map(|(k, v)| (k.to_string(), v.try_to_owned().ok()))
                .filter_map(|(k, v)| Some((k, v?)))
                .collect::<HashMap<String, OwnedValue>>(),
            exchange,
        )
    };
    let (sender, callback, word, changed, exchange) = msg;
    if let Err(e) = call(
        conn,
        &sender,
        &callback,
        "PromptReady",
        &(word, changed, exchange),
    )
    .await
    {
        log::warn!("keyring: {e}");
    }
}

/// The keyring's prompt properties (gcr/gcr-prompt.c), as they change.
fn apply(p: &mut Prompt, props: &HashMap<String, OwnedValue>) {
    // A value as sent, or wrapped once more (gcr's own prompter wraps twice).
    let get = |k: &str| -> Option<Value<'static>> {
        let mut v: Value<'static> = props.get(k)?.try_clone().ok()?.into();
        while let Value::Value(inner) = v {
            v = *inner;
        }
        Some(v)
    };
    let text = |k: &str| match get(k)? {
        Value::Str(s) => Some(s.to_string()),
        _ => None,
    };
    let flag = |k: &str| match get(k)? {
        Value::Bool(b) => Some(b),
        _ => None,
    };
    for (k, slot) in [
        ("title", &mut p.title),
        ("message", &mut p.message),
        ("description", &mut p.description),
        ("warning", &mut p.warning),
        ("continue-label", &mut p.continue_label),
        ("cancel-label", &mut p.cancel_label),
    ] {
        if let Some(v) = text(k) {
            *slot = v;
        }
    }
    if let Some(v) = flag("password-new") {
        p.new_password = v;
    }
    let label = text("choice-label").or_else(|| p.choice.as_ref().map(|c| c.0.clone()));
    let chosen = flag("choice-chosen").or_else(|| p.choice.as_ref().map(|c| c.1));
    p.choice = label
        .filter(|l| !l.is_empty())
        .map(|l| (l, chosen.unwrap_or(false)));
}

/// The prompts asked of kestrel: serves the system prompter while the stream is read.
pub fn stream() -> impl Stream<Item = Ask> {
    crate::stream::channel(8, async |mut out| {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let Ok(conn) = Connection::session().await else {
            return;
        };
        let prompter = Prompter {
            inner: Arc::default(),
            tx,
        };
        if let Err(e) = conn.object_server().at(PATH, prompter).await {
            log::warn!("keyring: no prompter ({e})");
            return;
        }
        // Taken from gcr-prompter if it runs (it may not hand it over: then once it exits).
        use zbus::fdo::RequestNameFlags as F;
        if let Err(e) = conn
            .request_name_with_flags(NAME, F::ReplaceExisting | F::AllowReplacement)
            .await
        {
            log::warn!("keyring: {NAME} not taken ({e})");
            return;
        }
        while let Some(a) = rx.recv().await {
            if out.send(a).await.is_err() {
                return;
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two sides of the exchange agree on the key and pass a secret both ways.
    #[test]
    fn exchange_round_trip() {
        for _ in 0..20 {
            let mut a = Exchange::new().unwrap();
            let mut b = Exchange::new().unwrap();
            assert_eq!(b.receive(&a.begin()).unwrap(), None);
            let m = b.send(None).unwrap();
            assert_eq!(a.receive(&m).unwrap(), None);
            let sealed = a.send(Some(b"hunter2 \xe2\x9c\x93")).unwrap();
            assert_eq!(
                b.receive(&sealed).unwrap().unwrap(),
                b"hunter2 \xe2\x9c\x93"
            );
            let back = b.send(Some(b"")).unwrap();
            assert_eq!(a.receive(&back).unwrap().unwrap(), b"");
        }
    }

    #[test]
    fn rejects_bad_keys() {
        let mut a = Exchange::new().unwrap();
        assert!(a.receive("[sx-aes-1]\npublic=AQ==\n").is_err());
        assert!(a.receive("[other]\npublic=AQ==\n").is_err());
    }

    /// Against gcr's own library (the keyring daemon's side, gcr 3 and gcr 4 through
    /// GObject introspection): thousands of exchanges, so a padding difference (1 in 256)
    /// would show. Run with `cargo test -- --ignored gcr`.
    #[test]
    #[ignore]
    fn interoperates_with_gcr() {
        use std::io::{BufRead, Write};
        for version in ["4", "3"] {
            let script = format!(
                r#"
import sys, ctypes, gi
gi.require_version('Gcr', '{version}')
from gi.repository import Gcr
# get_secret is mis-mapped by introspection: called through C.
lib = ctypes.CDLL('libgcr-base-3.so.1' if '{version}' == '3' else 'libgcr-4.so.4')
lib.gcr_secret_exchange_get_secret.restype = ctypes.c_char_p
lib.gcr_secret_exchange_get_secret.argtypes = [ctypes.c_void_p, ctypes.POINTER(ctypes.c_size_t)]
ctypes.pythonapi.PyCapsule_GetPointer.restype = ctypes.c_void_p
ctypes.pythonapi.PyCapsule_GetPointer.argtypes = [ctypes.py_object, ctypes.c_char_p]
for line in sys.stdin:
    begin = line.strip().replace('\\n', '\n')
    ex = Gcr.SecretExchange.new(None)
    ex.receive(begin)
    print(ex.send(None, -1).replace('\n', '\\n'), flush=True)
    sealed = sys.stdin.readline().strip().replace('\\n', '\n')
    ok = ex.receive(sealed)
    ptr = ctypes.pythonapi.PyCapsule_GetPointer(ex.__gpointer__, None)
    n = ctypes.c_size_t(0)
    got = lib.gcr_secret_exchange_get_secret(ptr, ctypes.byref(n)) if ok else None
    print((got[:n.value].decode() if got is not None else 'FAILED').replace('\n', '\\n'), flush=True)
    # The other way: gcr seals one for us.
    print(ex.send('back ' + str(n.value), -1).replace('\n', '\\n'), flush=True)
"#
            );
            let mut py = std::process::Command::new("python3")
                .args(["-I", "-c", &script])
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::inherit())
                .spawn()
                .unwrap();
            let mut input = py.stdin.take().unwrap();
            let mut output = std::io::BufReader::new(py.stdout.take().unwrap());
            for i in 0..3000 {
                let mut ours = Exchange::new().unwrap();
                writeln!(input, "{}", ours.begin().replace('\n', "\\n")).unwrap();
                let mut line = String::new();
                output.read_line(&mut line).unwrap();
                let got = ours.receive(&line.trim().replace("\\n", "\n"));
                assert_eq!(
                    got,
                    Ok(None),
                    "gcr {version}, exchange {i}, from gcr: {line:?}"
                );
                let secret = format!("password {i} ✓");
                let sealed = ours.send(Some(secret.as_bytes())).unwrap();
                writeln!(input, "{}", sealed.replace('\n', "\\n")).unwrap();
                line.clear();
                output.read_line(&mut line).unwrap();
                assert_eq!(
                    line.trim(),
                    secret,
                    "gcr {version}, exchange {i}: ours {} bytes, gcr key bytes and shared top byte {:?}",
                    unpadded(&ours.public).len(),
                    ours.debug_shared_top()
                );
                line.clear();
                output.read_line(&mut line).unwrap();
                let back = ours.receive(&line.trim().replace("\\n", "\n")).unwrap();
                assert_eq!(
                    back.as_deref(),
                    Some(format!("back {}", secret.len()).as_bytes()),
                    "gcr {version} to us, exchange {i}"
                );
            }
            drop(input);
            let _ = py.wait();
        }
    }

    /// The whole conversation with gcr's real client (what GNOME Keyring uses to ask), on a
    /// private bus: `dbus-run-session -- cargo test -p kestrel-services -- --ignored
    /// keyring::tests::prompts_like_gcr`. A wrong password asked again, then the right one.
    #[test]
    #[ignore]
    fn prompts_like_gcr() {
        if std::env::var_os("KESTREL_TEST_PRIVATE_BUS").is_none() {
            eprintln!("run inside dbus-run-session with KESTREL_TEST_PRIVATE_BUS=1");
            return;
        }
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            use futures::StreamExt;
            let mut asks = Box::pin(stream());
            // gcr's client, as the keyring daemon drives it: ask, be told it was wrong, ask
            // again, close.
            let script = r#"
import gi
gi.require_version('Gcr', '3')
from gi.repository import Gcr
p = Gcr.SystemPrompt.open_for_prompter('org.gnome.keyring.SystemPrompter', 10, None)
p.set_title('Unlock Login Keyring')
p.set_message('Authentication required')
p.set_choice_label('Automatically unlock')
first = p.password(None)
p.set_warning('The unlock password was incorrect')
second = p.password(None)
print(first, second, p.get_choice_chosen(), flush=True)
p.close()
"#;
            let client = tokio::process::Command::new("python3")
                .args(["-I", "-c", script])
                .stdout(std::process::Stdio::piped())
                .spawn()
                .unwrap();
            let mut seen = Vec::new();
            let answers = ["wrong ✗", "right ✓"];
            while seen.len() < 2 {
                match asks.next().await {
                    Some(Ask::Prompt(p)) => {
                        let n = seen.len();
                        seen.push(p.clone());
                        answer(
                            p.id,
                            Some(Reply {
                                password: answers[n].into(),
                                choice: n == 1,
                            }),
                        );
                    }
                    Some(Ask::Done(_)) => {}
                    None => panic!("prompter stopped"),
                }
            }
            // The prompt ends: Done arrives.
            let mut done = false;
            while !done {
                done = matches!(asks.next().await, Some(Ask::Done(_)));
            }
            let out = client.wait_with_output().await.unwrap();
            let out = String::from_utf8_lossy(&out.stdout);
            assert_eq!(out.trim(), "wrong ✗ right ✓ True");
            assert_eq!(seen[0].title, "Unlock Login Keyring");
            assert_eq!(seen[0].message, "Authentication required");
            assert!(seen[0].warning.is_empty());
            assert_eq!(seen[1].warning, "The unlock password was incorrect");
            assert_eq!(
                seen[1].choice.as_ref().map(|c| c.0.as_str()),
                Some("Automatically unlock")
            );
        });
    }
}
