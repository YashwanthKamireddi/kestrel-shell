//! `kestrel <command>` talks to the running shell over `$XDG_RUNTIME_DIR/kestrel.sock`.
//! Commands are single lines: `toggle <panel>`, `close`, `reload`.

use iced::Subscription;
use iced::futures::{SinkExt, Stream};
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};

pub fn socket() -> PathBuf {
    if let Some(p) = std::env::var_os("KESTREL_SOCKET") {
        return PathBuf::from(p);
    }
    let dir = std::env::var_os("XDG_RUNTIME_DIR").map_or_else(std::env::temp_dir, PathBuf::from);
    dir.join("kestrel.sock")
}

/// Sends a command to a running kestrel; returns false if none is running.
pub fn send(cmd: &str) -> bool {
    use std::io::{Read, Write};
    let Ok(mut s) = std::os::unix::net::UnixStream::connect(socket()) else {
        return false;
    };
    let _ = s.write_all(format!("{cmd}\n").as_bytes());
    let _ = s.shutdown(std::net::Shutdown::Write);
    let mut reply = String::new();
    let _ = s.read_to_string(&mut reply);
    if !reply.trim().is_empty() && reply.trim() != "ok" {
        eprintln!("{}", reply.trim());
    }
    true
}

fn listen() -> impl Stream<Item = String> {
    iced::stream::channel(8, async |mut out| {
        let path = socket();
        // A stale socket from a crashed run is removed; a live one means another instance.
        if UnixStream::connect(&path).await.is_ok() {
            log::error!("another kestrel is running; IPC disabled for this one");
            std::future::pending::<()>().await;
        }
        let _ = std::fs::remove_file(&path);
        let listener = match UnixListener::bind(&path) {
            Ok(l) => l,
            Err(e) => {
                log::error!("ipc {}: {e}", path.display());
                std::future::pending::<()>().await;
                return;
            }
        };
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                continue;
            };
            let (r, mut w) = stream.into_split();
            let mut line = String::new();
            if BufReader::new(r).read_line(&mut line).await.is_ok() {
                let cmd = line.trim().to_string();
                if !cmd.is_empty() {
                    let _ = out.send(cmd).await;
                }
            }
            let _ = w.write_all(b"ok\n").await;
        }
    })
}

pub fn subscription() -> Subscription<String> {
    Subscription::run(listen)
}
