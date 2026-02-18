//! What the sound playing right now is made of, about 30 times a second, for the music
//! visualiser: its energy in four frequency bands (bass, low mids, high mids, treble), like
//! cava. A small record stream on the default output's monitor (16 kHz mono) feeds a 512-sample
//! window; each band is the summed spectrum over its bins, auto-levelled so quiet and loud music
//! both fill the bars. It reads the speakers, never a microphone, so it does not count as
//! "recording" in the privacy indicator. The stream exists only while someone listens to this.

use futures::Stream;
use libpulse_binding as pulse;
use pulse::context::{Context, FlagSet as CtxFlags, State as CtxState};
use pulse::def::BufferAttr;
use pulse::mainloop::standard::{IterateResult, Mainloop};
use pulse::sample::{Format, Spec};
use pulse::stream::{FlagSet as StreamFlags, PeekResult, State as StreamState, Stream as PaStream};

/// Samples per second, and per analysis window (~32 ms).
const RATE: u32 = 16_000;
const WINDOW: usize = 512;
/// The bands' edges in Hz.
const BANDS: [(f32, f32); 4] = [
    (40.0, 200.0),
    (200.0, 800.0),
    (800.0, 2_500.0),
    (2_500.0, 7_000.0),
];

/// Energy per band, 0 to 1.
pub type Bands = [f32; 4];

/// The Hann window, computed once.
static HANN: std::sync::LazyLock<Vec<f32>> = std::sync::LazyLock::new(|| {
    (0..WINDOW)
        .map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / WINDOW as f32).cos())
        .collect()
});

/// The bands of one window of samples (Hann-windowed DFT over the bands' bins).
fn analyse(x: &[f32], gain: &mut [f32; 4]) -> Bands {
    let n = x.len() as f32;
    let hz = RATE as f32 / n;
    let mut out = [0f32; 4];
    for (b, &(lo, hi)) in BANDS.iter().enumerate() {
        let (k0, k1) = (
            (lo / hz).floor().max(1.0) as usize,
            (hi / hz).ceil() as usize,
        );
        let mut sum = 0f32;
        for k in k0..=k1 {
            // The bin's phasor advances by a fixed rotation per sample: one multiply each
            // instead of a sine and a cosine.
            let w = std::f32::consts::TAU * k as f32 / n;
            let (rc, rs) = (w.cos(), w.sin());
            let (mut c, mut s_) = (1f32, 0f32);
            let (mut re, mut im) = (0f32, 0f32);
            for (&v, &h) in x.iter().zip(HANN.iter()) {
                let v = v * h;
                re += v * c;
                im -= v * s_;
                (c, s_) = (c * rc - s_ * rs, c * rs + s_ * rc);
            }
            sum += (re * re + im * im).sqrt();
        }
        let e = sum / (k1 - k0 + 1) as f32;
        // Auto-level: each band remembers its recent peak (slowly forgotten) and is shown
        // against it, so every band moves whatever the music's loudness.
        gain[b] = (gain[b] * 0.995).max(e).max(0.02);
        out[b] = (e / gain[b]).clamp(0.0, 1.0);
    }
    out
}

fn run(out: &futures::channel::mpsc::UnboundedSender<Bands>) -> Option<()> {
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
        // About one window per fragment: low latency, few wakeups.
        fragsize: (WINDOW * 4) as u32,
    };
    stream
        .connect_record(
            Some("@DEFAULT_MONITOR@"),
            Some(&attr),
            StreamFlags::ADJUST_LATENCY | StreamFlags::DONT_INHIBIT_AUTO_SUSPEND,
        )
        .ok()?;
    let mut buf: Vec<f32> = Vec::with_capacity(WINDOW * 2);
    let mut gain = [0.02f32; 4];
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
                    buf.extend(
                        d.chunks_exact(4)
                            .filter_map(|c| <[u8; 4]>::try_from(c).ok())
                            .map(f32::from_ne_bytes),
                    );
                    let _ = stream.discard();
                }
                Ok(PeekResult::Hole(_)) => {
                    let _ = stream.discard();
                }
                _ => break,
            }
        }
        // The newest full window; older samples are dropped (the bars show now, not then).
        if buf.len() >= WINDOW {
            let window = &buf[buf.len() - WINDOW..];
            let bands = analyse(window, &mut gain);
            buf.clear();
            // Nobody listening any more: stop measuring.
            if out.unbounded_send(bands).is_err() {
                let _ = stream.disconnect();
                return Some(());
            }
        }
    }
}

/// The four bands, about 30 readings a second, while the stream is polled.
pub fn stream() -> impl Stream<Item = Bands> {
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
