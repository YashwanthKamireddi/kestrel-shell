//! Live sound levels for the Sound page, like Windows' volume mixer: the output, the microphone
//! and each app playing sound, each a peak level 0 to 1, about 25 times a second.
//!
//! The sound server measures them (peak-detect record streams: it sends one number per window,
//! not the sound itself), so this costs next to nothing. The streams exist only while the page
//! is open (while this stream is polled); the app streams are re-made when the apps change.
//! They are named "kestrel meters", which the audio service leaves out of "recording".

use futures::Stream;
use libpulse_binding as pulse;
use pulse::context::{Context, FlagSet as CtxFlags, State as CtxState};
use pulse::def::BufferAttr;
use pulse::mainloop::standard::{IterateResult, Mainloop};
use pulse::sample::{Format, Spec};
use pulse::stream::{FlagSet as StreamFlags, PeekResult, State as StreamState, Stream as PaStream};

/// Peak levels, 0 to 1.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Meters {
    pub output: f32,
    pub input: f32,
    /// (stream index, level) for each app stream asked for.
    pub apps: Vec<(u32, f32)>,
}

/// Readings a second.
const RATE: u32 = 25;

fn peak_stream(ctx: &mut Context, device: &str, monitor_of: Option<u32>) -> Option<PaStream> {
    let spec = Spec {
        format: Format::FLOAT32NE,
        channels: 1,
        rate: RATE,
    };
    let mut s = PaStream::new(ctx, "level", &spec, None)?;
    if let Some(index) = monitor_of {
        s.set_monitor_stream(index).ok()?;
    }
    let attr = BufferAttr {
        maxlength: u32::MAX,
        tlength: u32::MAX,
        prebuf: u32::MAX,
        minreq: u32::MAX,
        // One sample per fragment: every reading arrives as it is made.
        fragsize: 4,
    };
    s.connect_record(
        Some(device),
        Some(&attr),
        StreamFlags::PEAK_DETECT
            | StreamFlags::ADJUST_LATENCY
            | StreamFlags::DONT_INHIBIT_AUTO_SUSPEND
            | StreamFlags::DONT_MOVE,
    )
    .ok()?;
    Some(s)
}

/// A peak as a meter shows it: decibels, -60 dB (silence) to 0 dB (full) over 0 to 1, as
/// sound meters read. Linear, speech into a laptop microphone (a few hundredths) was a sliver
/// and the microphone looked dead.
fn shown(peak: f32) -> f32 {
    if peak <= 0.001 {
        return 0.0;
    }
    ((20.0 * peak.log10() + 60.0) / 60.0).clamp(0.0, 1.0)
}

/// The newest reading waiting on a stream, if any.
fn latest(s: &mut PaStream) -> Option<f32> {
    if s.get_state() != StreamState::Ready {
        return None;
    }
    let mut last = None;
    while s.readable_size().unwrap_or(0) > 0 {
        match s.peek() {
            Ok(PeekResult::Data(d)) => {
                if let Some(c) = d.as_chunks::<4>().0.last() {
                    last = Some(f32::from_ne_bytes(*c).clamp(0.0, 1.0));
                }
                let _ = s.discard();
            }
            Ok(PeekResult::Hole(_)) => {
                let _ = s.discard();
            }
            _ => break,
        }
    }
    last
}

/// `apps`: each app stream to measure, with the output (sink name) it plays on.
fn run(
    apps: &[(u32, String)],
    out: &futures::channel::mpsc::UnboundedSender<Meters>,
) -> Option<()> {
    let mut ml = Mainloop::new()?;
    let mut ctx = Context::new(&ml, "kestrel meters")?;
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
    let mut output = peak_stream(&mut ctx, "@DEFAULT_MONITOR@", None);
    let mut input = peak_stream(&mut ctx, "@DEFAULT_SOURCE@", None);
    let mut per_app: Vec<(u32, Option<PaStream>)> = apps
        .iter()
        .map(|(i, sink)| {
            (
                *i,
                peak_stream(&mut ctx, &format!("{sink}.monitor"), Some(*i)),
            )
        })
        .collect();
    let mut m = Meters {
        apps: apps.iter().map(|(i, _)| (*i, 0.0)).collect(),
        ..Meters::default()
    };
    let mut sent = None;
    loop {
        if let IterateResult::Quit(_) | IterateResult::Err(_) = ml.iterate(true) {
            return None;
        }
        if let Some(v) = output.as_mut().and_then(latest) {
            m.output = shown(v);
        }
        if let Some(v) = input.as_mut().and_then(latest) {
            m.input = shown(v);
        }
        for (k, (_, s)) in per_app.iter_mut().enumerate() {
            if let Some(v) = s.as_mut().and_then(latest) {
                m.apps[k].1 = shown(v);
            }
        }
        if sent.as_ref() != Some(&m) {
            sent = Some(m.clone());
            // Nobody listening any more (the page closed): stop measuring.
            if out.unbounded_send(m.clone()).is_err() {
                for s in [&mut output, &mut input]
                    .into_iter()
                    .flatten()
                    .chain(per_app.iter_mut().filter_map(|(_, s)| s.as_mut()))
                {
                    let _ = s.disconnect();
                }
                return Some(());
            }
        }
    }
}

/// The levels while polled; `apps` are the app streams to measure, each with its output.
pub fn stream(apps: Vec<(u32, String)>) -> impl Stream<Item = Meters> {
    crate::stream::channel(4, async move |mut out| {
        use futures::{SinkExt, StreamExt};
        let (tx, mut rx) = futures::channel::mpsc::unbounded();
        let spawned = std::thread::Builder::new()
            .name("kestrel-meters".into())
            .spawn(move || {
                let _ = run(&apps, &tx);
            });
        if spawned.is_err() {
            return;
        }
        while let Some(v) = rx.next().await {
            if out.send(v).await.is_err() {
                return;
            }
        }
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn decibel_scale() {
        assert_eq!(super::shown(0.0), 0.0);
        assert!((super::shown(1.0) - 1.0).abs() < 1e-6);
        // -20 dB (0.1, speech) reads two thirds of the way, not a tenth.
        assert!((super::shown(0.1) - 2.0 / 3.0).abs() < 1e-3);
    }
}
