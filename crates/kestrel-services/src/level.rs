//! How loud the sound playing right now is, about 30 times a second, for the music visualiser.
//! The sound server measures it: a record stream on the default output's monitor with peak
//! detection (what volume meters use), so kestrel receives one number per reading, not audio.
//! It reads the speakers, never a microphone, so it does not count as "recording" in the
//! privacy indicator. The stream exists only while someone listens to this.

use futures::Stream;
use libpulse_binding as pulse;
use pulse::context::{Context, FlagSet as CtxFlags, State as CtxState};
use pulse::def::BufferAttr;
use pulse::mainloop::standard::{IterateResult, Mainloop};
use pulse::sample::{Format, Spec};
use pulse::stream::{FlagSet as StreamFlags, PeekResult, State as StreamState, Stream as PaStream};

/// Readings per second.
const RATE: u32 = 30;

fn run(out: &futures::channel::mpsc::UnboundedSender<f32>) -> Option<()> {
    let mut ml = Mainloop::new()?;
    let mut ctx = Context::new(&ml, "kestrel visualiser")?;
    ctx.connect(None, CtxFlags::NOFAIL, None).ok()?;
    loop {
        if let IterateResult::Quit(_) | IterateResult::Err(_) = ml.iterate(true) {
            return None;
        }
        match ctx.get_state() {
            CtxState::Ready => break,
            CtxState::Failed | CtxState::Terminated => return None,
            _ => {}
        }
    }
    let spec = Spec {
        format: Format::FLOAT32NE,
        channels: 1,
        rate: RATE,
    };
    let mut stream = PaStream::new(&mut ctx, "music level", &spec, None)?;
    let attr = BufferAttr {
        maxlength: u32::MAX,
        tlength: u32::MAX,
        prebuf: u32::MAX,
        minreq: u32::MAX,
        fragsize: 4,
    };
    stream
        .connect_record(
            Some("@DEFAULT_MONITOR@"),
            Some(&attr),
            StreamFlags::PEAK_DETECT
                | StreamFlags::ADJUST_LATENCY
                | StreamFlags::DONT_INHIBIT_AUTO_SUSPEND,
        )
        .ok()?;
    loop {
        if let IterateResult::Quit(_) | IterateResult::Err(_) = ml.iterate(true) {
            return None;
        }
        match stream.get_state() {
            StreamState::Ready => {}
            StreamState::Failed | StreamState::Terminated => return None,
            _ => continue,
        }
        while stream.readable_size().unwrap_or(0) > 0 {
            match stream.peek() {
                Ok(PeekResult::Data(d)) => {
                    if let Some(chunk) = d.get(d.len().saturating_sub(4)..)
                        && let Ok(b) = <[u8; 4]>::try_from(chunk)
                    {
                        // Nobody listening any more: stop measuring.
                        if out
                            .unbounded_send(f32::from_ne_bytes(b).clamp(0.0, 1.0))
                            .is_err()
                        {
                            let _ = stream.disconnect();
                            return Some(());
                        }
                    }
                    let _ = stream.discard();
                }
                Ok(PeekResult::Hole(_)) => {
                    let _ = stream.discard();
                }
                _ => break,
            }
        }
    }
}

/// Loudness, 0 to 1, about 30 readings a second, while the stream is polled.
pub fn stream() -> impl Stream<Item = f32> {
    crate::stream::channel(4, async |mut out| {
        use futures::{SinkExt, StreamExt};
        let (tx, mut rx) = futures::channel::mpsc::unbounded();
        let spawned = std::thread::Builder::new()
            .name("kestrel-level".into())
            .spawn(move || {
                let _ = run(&tx);
            });
        if spawned.is_err() {
            return;
        }
        while let Some(v) = rx.next().await {
            if out.send(v).await.is_err() {
                return; // Dropping rx makes the thread's next send fail, and it stops.
            }
        }
    })
}
