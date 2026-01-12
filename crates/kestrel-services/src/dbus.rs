//! Small D-Bus helpers shared by the services: uncached property reads and a "something
//! changed" stream for a bus name, which the services answer by re-reading their state.

use futures::{Stream, StreamExt};
use std::time::Duration;
use zbus::zvariant::{OwnedValue, Value};
use zbus::{Connection, MatchRule, MessageStream, proxy::CacheProperties};

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
    Ok(Box::pin(debounce(
        stream.map(|_| ()),
        Duration::from_millis(80),
    )))
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
