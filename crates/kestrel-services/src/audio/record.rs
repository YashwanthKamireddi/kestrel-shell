//! Recording from a microphone for a few seconds (Settings > Sound's microphone test).

use super::*;

/// A recording under way: its stream, what it has heard, how much it wants (bytes), and who
/// is waiting for it.
pub(super) struct Recording {
    pub(super) stream: Rc<RefCell<pulse::stream::Stream>>,
    pub(super) heard: Rc<RefCell<Vec<u8>>>,
    pub(super) wanted: usize,
    pub(super) reply: Reply<Result<Arc<crate::sounds::Pcm>, String>>,
}

pub(super) const RECORD_RATE: u32 = 48_000;

/// Starts recording `secs` seconds from the default microphone; `RecordEnd` comes when that
/// much sound has arrived (counted in sound, not on a clock: the microphone takes a moment
/// to start), or two seconds late if the microphone stalls.
pub(super) fn record(
    ctx: &mut Context,
    secs: u32,
    reply: Reply<Result<Arc<crate::sounds::Pcm>, String>>,
    tx: &mpsc::Sender<Command>,
) -> Option<Recording> {
    use pulse::stream::{FlagSet as StreamFlags, PeekResult, State as StreamState, Stream};
    let spec = pulse::sample::Spec {
        format: pulse::sample::Format::S16le,
        channels: 1,
        rate: RECORD_RATE,
    };
    let wanted = spec.usec_to_bytes(pulse::time::MicroSeconds(u64::from(secs) * 1_000_000));
    let stream = Rc::new(RefCell::new(Stream::new(
        ctx,
        "kestrel microphone test",
        &spec,
        None,
    )?));
    let heard = Rc::new(RefCell::new(Vec::with_capacity(wanted)));
    let (s, h, full) = (stream.clone(), heard.clone(), tx.clone());
    stream
        .borrow_mut()
        .set_read_callback(Some(Box::new(move |_| {
            let Ok(mut s) = s.try_borrow_mut() else {
                return;
            };
            loop {
                match s.peek() {
                    Ok(PeekResult::Data(d)) => h.borrow_mut().extend_from_slice(d),
                    // A gap (nothing captured for a moment): silence in its place.
                    Ok(PeekResult::Hole(n)) => {
                        let len = h.borrow().len();
                        h.borrow_mut().resize(len + n, 0);
                    }
                    Ok(PeekResult::Empty) | Err(_) => break,
                }
                if s.discard().is_err() {
                    break;
                }
            }
            if h.borrow().len() >= wanted {
                let _ = full.send(Command::RecordEnd);
            }
        })));
    let (s, late) = (stream.clone(), tx.clone());
    stream
        .borrow_mut()
        .set_state_callback(Some(Box::new(move || {
            match s.try_borrow().map(|s| s.get_state()) {
                Ok(StreamState::Ready) => {
                    let late = late.clone();
                    let _ = std::thread::Builder::new()
                        .name("kestrel-mic-test".into())
                        .spawn(move || {
                            let wait = u64::from(secs) + 2;
                            std::thread::sleep(std::time::Duration::from_secs(wait));
                            let _ = late.send(Command::RecordEnd);
                        });
                }
                Ok(StreamState::Failed) => {
                    let _ = late.send(Command::RecordEnd);
                }
                _ => {}
            }
        })));
    // Delivered every 50 ms (left to the server, a recording may arrive in 2 s pieces).
    let fragment = spec.usec_to_bytes(pulse::time::MicroSeconds(50_000));
    let attr = pulse::def::BufferAttr {
        maxlength: u32::MAX,
        tlength: u32::MAX,
        prebuf: u32::MAX,
        minreq: u32::MAX,
        fragsize: u32::try_from(fragment).unwrap_or(u32::MAX),
    };
    if stream
        .borrow_mut()
        .connect_record(None, Some(&attr), StreamFlags::ADJUST_LATENCY)
        .is_err()
    {
        reply.send(Err("The microphone could not be opened".into()));
        return None;
    }
    Some(Recording {
        stream,
        heard,
        wanted,
        reply,
    })
}
