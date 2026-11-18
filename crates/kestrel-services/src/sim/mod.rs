//! Deterministic simulation tests : a service's pure state logic
//! (its `Tracker`) driven against a simulated bus by a seeded random sequence of events, with
//! the service's invariants checked after every step.
//!
//! Every run is a function of its seed alone, so a failure replays exactly:
//! `KESTREL_SIM_SEED=<seed> cargo test --release -p kestrel-services sim::<service>`.
//! `KESTREL_SIM_SEEDS=<count>` runs a longer hunt (`make check` runs the default count).

pub(crate) mod mpris;
pub(crate) mod tray;

/// xorshift64*: tiny, fast, and the same sequence for the same seed on every machine.
pub(crate) struct Rng(u64);

impl Rng {
    pub(crate) fn new(seed: u64) -> Rng {
        // splitmix64 of the seed, so nearby seeds start far apart (and never at 0).
        let mut z = seed.wrapping_add(0x9e37_79b9_7f4a_7c15);
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        Rng((z ^ (z >> 31)) | 1)
    }

    pub(crate) fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    /// 0..n (n > 0).
    pub(crate) fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    /// True `pct` times in a hundred.
    pub(crate) fn chance(&mut self, pct: u64) -> bool {
        self.next() % 100 < pct
    }

    pub(crate) fn pick<'a, T>(&mut self, v: &'a [T]) -> &'a T {
        &v[self.below(v.len())]
    }
}

/// With `KESTREL_SIM_TRACE` set (replaying one seed), every event handled is printed.
pub(crate) fn trace(what: impl FnOnce() -> String) {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if *ON.get_or_init(|| std::env::var_os("KESTREL_SIM_TRACE").is_some()) {
        eprintln!("{}", what());
    }
}

/// FNV-1a, to fold a run's observations into one number (two runs of a seed must agree).
#[derive(Clone, Copy)]
pub(crate) struct Digest(u64);

impl Digest {
    pub(crate) fn new() -> Digest {
        Digest(0xcbf2_9ce4_8422_2325)
    }

    pub(crate) fn add(&mut self, s: &str) {
        for b in s.bytes() {
            self.0 = (self.0 ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
        }
    }

    pub(crate) fn value(self) -> u64 {
        self.0
    }
}

/// Runs `run` for each seed (`KESTREL_SIM_SEED` for one, `KESTREL_SIM_SEEDS` for how many,
/// else `default`), each twice: it must not panic, must hold its invariants (an `Err` says
/// which broke, and when), and must give the same digest both times. A failure names its seed
/// and the command that replays it.
pub(crate) fn seeds(name: &str, default: u64, run: impl Fn(u64) -> Result<u64, String>) {
    let env = |k: &str| std::env::var(k).ok().and_then(|v| v.parse::<u64>().ok());
    let list: Vec<u64> = match (env("KESTREL_SIM_SEED"), env("KESTREL_SIM_SEEDS")) {
        (Some(s), _) => vec![s],
        (None, n) => (0..n.unwrap_or(default)).collect(),
    };
    let replay = |seed: u64| {
        format!(
            "replay: KESTREL_SIM_SEED={seed} cargo test --release -p kestrel-services \
             sim::{name} -- --nocapture"
        )
    };
    for seed in list {
        let once = || std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(seed)));
        let (a, b) = (once(), once());
        match (a, b) {
            (Ok(Ok(a)), Ok(Ok(b))) if a == b => {}
            (Ok(Ok(_)), Ok(Ok(_))) => panic!(
                "{name} sim, seed {seed}: two runs of the same seed differed (hidden \
                 nondeterminism)\n{}",
                replay(seed)
            ),
            (Ok(Err(e)), _) | (_, Ok(Err(e))) => {
                panic!("{name} sim, seed {seed}: {e}\n{}", replay(seed))
            }
            _ => panic!("{name} sim, seed {seed}: panicked\n{}", replay(seed)),
        }
    }
}

#[test]
fn the_rng_repeats_for_a_seed_and_differs_between_seeds() {
    let take = |seed| {
        let mut r = Rng::new(seed);
        (0..8).map(|_| r.next()).collect::<Vec<_>>()
    };
    assert_eq!(take(7), take(7));
    assert_ne!(take(7), take(8));
}
