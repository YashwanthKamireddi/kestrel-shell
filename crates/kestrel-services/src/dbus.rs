//! Small D-Bus helpers shared by the services: uncached property reads and a "something
//! changed" stream for a bus name, which the services answer by re-reading their state.

use futures::{Stream, StreamExt};
use std::time::Duration;
use zbus::zvariant::{OwnedValue, Value};
use zbus::{Connection, MatchRule, MessageStream, proxy::CacheProperties};

/// The session bus, one connection kept for the shell's one-off calls (a media key, a tray
/// click, the media position, a unit restarted). A connection per call cost a handshake each
/// time and told every program watching the bus about a name coming and going.
pub async fn session() -> zbus::Result<Connection> {
    static CONN: tokio::sync::OnceCell<Connection> = tokio::sync::OnceCell::const_new();
    CONN.get_or_try_init(Connection::session).await.cloned()
}

pub async fn proxy<'a>(
    c: &Connection,
    dest: &'a str,
    path: &'a str,
    iface: &'a str,
) -> zbus::Result<zbus::Proxy<'a>> {
    zbus::proxy::Builder::new(c)
        .destination(dest)?
        .path(path)?
        .interface(iface)?
        .cache_properties(CacheProperties::No)
        .build()
        .await
}

pub async fn get<T>(
    c: &Connection,
    dest: &str,
    path: &str,
    iface: &str,
    name: &str,
) -> zbus::Result<T>
where
    T: TryFrom<OwnedValue>,
    T::Error: Into<zbus::Error>,
{
    proxy(c, dest, path, iface).await?.get_property(name).await
}

pub async fn set<'v, T>(
    c: &Connection,
    dest: &str,
    path: &str,
    iface: &str,
    name: &str,
    value: T,
) -> zbus::Result<()>
where
    T: 'v + Into<Value<'v>> + serde::Serialize + zbus::zvariant::DynamicType,
{
    proxy(c, dest, path, iface)
        .await?
        .set_property(name, value)
        .await
        .map_err(zbus::Error::from)
}

/// Every property of `iface` at `path` in one call (instead of one round trip per property).
pub async fn get_all(
    c: &Connection,
    dest: &str,
    path: &str,
    iface: &str,
) -> zbus::Result<std::collections::HashMap<String, OwnedValue>> {
    let props = zbus::fdo::PropertiesProxy::builder(c)
        .destination(dest.to_string())?
        .path(path.to_string())?
        .cache_properties(CacheProperties::No)
        .build()
        .await?;
    let iface = zbus::names::InterfaceName::try_from(iface.to_string())?;
    Ok(props.get_all(iface).await?)
}

/// Fires (debounced) whenever `sender` emits any signal.
pub async fn changes(
    c: &Connection,
    sender: &str,
) -> zbus::Result<std::pin::Pin<Box<dyn Stream<Item = ()> + Send>>> {
    let rule = MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender(sender.to_string())?
        .build();
    let stream = MessageStream::for_match_rule(rule, c, Some(64)).await?;
    // Also "look again" after the machine wakes (changes during sleep can be missed) and when
    // the service itself restarts (its old signals are gone, its state is new): the state is
    // always read whole again, never patched from events (docs/ROADMAP-v2.md, rule 1).
    let resume = MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender("org.freedesktop.login1")?
        .interface("org.freedesktop.login1.Manager")?
        .member("PrepareForSleep")?
        .build();
    let owner = MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender("org.freedesktop.DBus")?
        .interface("org.freedesktop.DBus")?
        .member("NameOwnerChanged")?
        .arg(0, sender.to_string())?
        .build();
    let resume = MessageStream::for_match_rule(resume, c, Some(4)).await?;
    let owner = MessageStream::for_match_rule(owner, c, Some(4)).await?;
    let all = futures::stream::select(
        stream.map(|_| ()),
        futures::stream::select(resume.map(|_| ()), owner.map(|_| ())),
    );
    Ok(Box::pin(debounce(all, Duration::from_millis(80))))
}

/// Emits once after a burst of items has been quiet for `quiet`.
pub fn debounce<S: Stream<Item = ()> + Unpin>(s: S, quiet: Duration) -> impl Stream<Item = ()> {
    futures::stream::unfold(s, move |mut s| async move {
        s.next().await?;
        loop {
            match tokio::time::timeout(quiet, s.next()).await {
                Ok(Some(())) => continue,
                Ok(None) => return None,
                Err(_) => return Some(((), s)),
            }
        }
    })
}
