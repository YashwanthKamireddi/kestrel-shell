//! Event sounds through PulseAudio's sample cache: each sound uploaded once, then played by
//! name at the volume asked.

use super::*;

/// Event sounds in the server's sample cache, as libcanberra keeps them: uploaded once per
/// connection, then played by name.
#[derive(Default)]
pub(super) struct Samples {
    pub(super) cached: HashSet<String>,
    pub(super) uploading: HashMap<String, Upload>,
}

/// An upload under way: its stream, the plays waiting for it (their levels), and whether the
/// sound stays cached afterwards.
pub(super) struct Upload {
    pub(super) stream: Rc<RefCell<pulse::stream::Stream>>,
    pub(super) waiting: Vec<f32>,
    pub(super) keep: bool,
}

pub(super) fn sample_name(pcm: &crate::sounds::Pcm) -> String {
    format!("kestrel:{}", pcm.file.display())
}

/// Plays a cached sample as an event sound (the role the session's policy knows).
pub(super) fn play_sample(ctx: &mut Context, name: &str, volume: f32, tx: &mpsc::Sender<Command>) {
    use pulse::proplist::{Proplist, properties};
    let Some(mut props) = Proplist::new() else {
        return;
    };
    let _ = props.set_str(properties::MEDIA_ROLE, "event");
    let _ = props.set_str(properties::EVENT_ID, name);
    let volume = Volume::from(pulse::volume::VolumeLinear(f64::from(
        volume.clamp(0.0, 1.0),
    )));
    let (tx, gone) = (tx.clone(), name.to_string());
    ctx.play_sample_with_proplist(
        name,
        None,
        Some(volume),
        &props,
        Some(Box::new(move |r| {
            if r.is_err() {
                let _ = tx.send(Command::Forget(gone.clone()));
            }
        })),
    );
}

/// Starts uploading `pcm` into the server's sample cache; `Uploaded` follows.
pub(super) fn upload(
    ctx: &mut Context,
    pcm: &crate::sounds::Pcm,
    tx: &mpsc::Sender<Command>,
) -> Option<Rc<RefCell<pulse::stream::Stream>>> {
    use pulse::stream::{SeekMode, State as StreamState, Stream};
    let name = sample_name(pcm);
    let spec = pulse::sample::Spec {
        format: pulse::sample::Format::S16le,
        channels: pcm.channels,
        rate: pcm.rate,
    };
    if !spec.is_valid() {
        log::warn!("sound {name}: unplayable format");
        return None;
    }
    let st = Rc::new(RefCell::new(Stream::new(ctx, &name, &spec, None)?));
    let data = pcm.bytes();
    let len = data.len();
    // The data as the server asks for it; then the upload is finished.
    let (w, mut at) = (st.clone(), 0usize);
    st.borrow_mut().set_write_callback(Some(Box::new(move |n| {
        let Ok(mut s) = w.try_borrow_mut() else {
            return;
        };
        if at >= data.len() {
            return;
        }
        let end = (at + n).min(data.len());
        if s.write_copy(&data[at..end], 0, SeekMode::Relative).is_ok() {
            at = end;
        }
        if at >= data.len() {
            let _ = s.finish_upload();
        }
    })));
    let (c, tx, done) = (st.clone(), tx.clone(), name.clone());
    st.borrow_mut().set_state_callback(Some(Box::new(move || {
        let Ok(s) = c.try_borrow() else {
            return;
        };
        match s.get_state() {
            // A finished upload ends its stream.
            StreamState::Terminated => {
                let _ = tx.send(Command::Uploaded(done.clone(), true));
            }
            StreamState::Failed => {
                let _ = tx.send(Command::Uploaded(done.clone(), false));
            }
            _ => {}
        }
    })));
    st.borrow_mut().connect_upload(len).ok()?;
    Some(st)
}

impl Samples {
    /// Plays a sound: at once when the server has it cached, after its upload otherwise (plays
    /// asked while it uploads wait for it).
    pub(super) fn play(
        &mut self,
        ctx: &mut Context,
        pcm: &crate::sounds::Pcm,
        volume: f32,
        tx: &mpsc::Sender<Command>,
    ) {
        let name = sample_name(pcm);
        if pcm.keep && self.cached.contains(&name) {
            play_sample(ctx, &name, volume, tx);
        } else if let Some(u) = self.uploading.get_mut(&name) {
            u.waiting.push(volume);
        } else if let Some(stream) = upload(ctx, pcm, tx) {
            let u = Upload {
                stream,
                waiting: vec![volume],
                keep: pcm.keep,
            };
            self.uploading.insert(name, u);
        }
    }

    /// An upload finished: the plays waiting for it go, and a sound to keep is cached.
    pub(super) fn uploaded(
        &mut self,
        ctx: &mut Context,
        name: String,
        ok: bool,
        tx: &mpsc::Sender<Command>,
    ) {
        if let Some(u) = self.uploading.remove(&name) {
            // Its callbacks hold the stream: cleared, so it is freed.
            let mut s = u.stream.borrow_mut();
            s.set_write_callback(None);
            s.set_state_callback(None);
            drop(s);
            if ok {
                for v in u.waiting {
                    play_sample(ctx, &name, v, tx);
                }
                // A recording is uploaded afresh each time (same name, replaced).
                if u.keep {
                    self.cached.insert(name);
                }
            } else {
                log::warn!("sound {name}: the server did not take it");
            }
        }
    }
}
