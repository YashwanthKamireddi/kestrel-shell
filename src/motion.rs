//! Motion primitives. Springs give panels their weight; tweens handle simple fades.
//! Both are stepped with real elapsed time, so a slow frame never makes motion slower.
//!
//! The motion tokens (`theme::tok`, SPRING_*, HOVER_*, GLIDE_*, BREATH) are read only through
//! here: `Spring::to`, `secs` and `glide` apply `[motion] speed` and `reduce` in one place.

use crate::theme::tok;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Instant;

/// Game mode: no motion anywhere in the shell. Every spring arrives at once, so nothing asks
/// for animation frames and no CPU goes to movement while a game runs.
static STILL: AtomicBool = AtomicBool::new(false);
/// `[motion] reduce`: every transition arrives at once (the visualiser, a level meter rather
/// than a transition, keeps moving; game mode stops that too).
static REDUCE: AtomicBool = AtomicBool::new(false);
/// `[motion] speed`: 2 plays every animation twice as fast, 0.5 half as fast.
static SPEED: AtomicU32 = AtomicU32::new(1.0f32.to_bits());

pub fn set_still(on: bool) {
    STILL.store(on, Ordering::Relaxed);
}

pub fn still() -> bool {
    STILL.load(Ordering::Relaxed)
}

/// The config's `[motion]`. A speed of 0 or less (or not a number) is read as no motion.
pub fn set_tuning(speed: f32, reduce: bool) {
    let t = Tuning::new(speed, reduce);
    SPEED.store(t.speed.to_bits(), Ordering::Relaxed);
    REDUCE.store(t.off, Ordering::Relaxed);
}

/// No transitions now: game mode or `[motion] reduce`.
pub fn instant() -> bool {
    still() || REDUCE.load(Ordering::Relaxed)
}

/// How motion plays now.
fn now() -> Tuning {
    Tuning {
        speed: f32::from_bits(SPEED.load(Ordering::Relaxed)),
        off: instant(),
    }
}

/// A duration token as it plays now, in seconds (0: arrive at once).
pub fn secs(token: f32) -> f32 {
    now().secs(token)
}

/// The share of what is left that an exponential glide token covers in `dt` seconds now.
pub fn glide(dt: f32, rate: f32) -> f32 {
    now().glide(dt, rate)
}

/// `[motion]` as the tokens are read: the one place speed and reduce change them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tuning {
    /// 2 plays every animation twice as fast, 0.5 half as fast.
    pub speed: f32,
    /// Every transition arrives at once.
    pub off: bool,
}

impl Default for Tuning {
    fn default() -> Self {
        Self::new(1.0, false)
    }
}

impl Tuning {
    pub fn new(speed: f32, reduce: bool) -> Self {
        let usable = speed.is_finite() && speed > 0.0;
        Self {
            speed: if usable { speed } else { 1.0 },
            off: reduce || !usable,
        }
    }

    /// A duration: shorter at a higher speed, 0 when motion is off.
    pub fn secs(self, token: f32) -> f32 {
        if self.off { 0.0 } else { token / self.speed }
    }

    /// An exponential glide at `rate` per second: the share of what is left covered in `dt`
    /// seconds; all of it when motion is off.
    pub fn glide(self, dt: f32, rate: f32) -> f32 {
        if self.off {
            1.0
        } else {
            1.0 - (-dt * rate * self.speed).exp()
        }
    }

    /// A spring: stiffer at a higher speed with the same damping, so the same shape in less
    /// time. None when motion is off (it arrives at once).
    pub fn spring(self, (omega, zeta): (f32, f32)) -> Option<(f32, f32)> {
        (!self.off).then_some((omega * self.speed, zeta))
    }
}

/// A damped spring that chases `target`.
#[derive(Clone, Copy, Debug)]
pub struct Spring {
    pub value: f32,
    pub velocity: f32,
    pub target: f32,
    /// Angular frequency (rad/s): higher is faster.
    pub omega: f32,
    /// Damping ratio: 1 is critical (no overshoot), below 1 settles with a slight overshoot.
    pub zeta: f32,
}

impl Default for Spring {
    fn default() -> Self {
        Self::new(0.0)
    }
}

impl Spring {
    pub fn new(value: f32) -> Self {
        Self {
            value,
            velocity: 0.0,
            target: value,
            omega: tok::SPRING_OPEN.0,
            zeta: tok::SPRING_OPEN.1,
        }
    }

    /// Chase `target` with a spring token (`tok::SPRING_*`) as `[motion]` plays it.
    pub fn to(&mut self, target: f32, spring: (f32, f32)) {
        self.target = target;
        match now().spring(spring) {
            Some((omega, zeta)) => {
                self.omega = omega;
                self.zeta = zeta;
            }
            None => self.snap(target),
        }
    }

    pub fn snap(&mut self, value: f32) {
        self.value = value;
        self.target = value;
        self.velocity = 0.0;
    }

    pub fn settled(&self) -> bool {
        (self.value - self.target).abs() < 0.001 * self.scale()
            && self.velocity.abs() < 0.01 * self.scale()
    }

    fn scale(&self) -> f32 {
        self.target.abs().max(1.0)
    }

    /// Advance by `dt` seconds (semi-implicit Euler in small sub-steps; stable at any frame rate).
    pub fn step(&mut self, dt: f32) {
        if self.settled() {
            self.snap(self.target);
            return;
        }
        let mut left = dt.min(0.1);
        // Sub-steps of 1/240 s, shorter for a spring stiffer than the tokens' (a high
        // `[motion] speed`) so it stays stable.
        let most = (1.0 / 240.0_f32).min(0.5 / self.omega.max(1.0));
        while left > 0.0 {
            let h = left.min(most);
            let accel = -2.0 * self.zeta * self.omega * self.velocity
                - self.omega * self.omega * (self.value - self.target);
            self.velocity += accel * h;
            self.value += self.velocity * h;
            left -= h;
        }
    }
}

/// Wall-clock driver: returns the seconds since the last call.
#[derive(Clone, Copy, Debug)]
pub struct Clock(Option<Instant>);

impl Clock {
    pub const fn new() -> Self {
        Self(None)
    }

    pub fn tick(&mut self, now: Instant) -> f32 {
        let dt = self.0.map_or(1.0 / 60.0, |t| (now - t).as_secs_f32());
        self.0 = Some(now);
        dt
    }

    pub fn reset(&mut self) {
        self.0 = None;
    }
}

pub fn ease_out_cubic(t: f32) -> f32 {
    1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(3)
}

/// Maps `t` from `[a, b]` to `[0, 1]`, clamped.
pub fn range(t: f32, a: f32, b: f32) -> f32 {
    ((t - a) / (b - a)).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_spring_settles_with_small_overshoot() {
        let mut s = Spring::new(0.0);
        s.to(1.0, tok::SPRING_OPEN);
        let mut peak: f32 = 0.0;
        let mut t = 0.0;
        // When it last was more than 1 % away: what the eye reads as "arrived".
        let mut arrived = 0.0;
        while !s.settled() && t < 2.0 {
            s.step(1.0 / 60.0);
            peak = peak.max(s.value);
            t += 1.0 / 60.0;
            if (s.value - 1.0).abs() > 0.01 {
                arrived = t;
            }
        }
        assert!(arrived < 0.32, "arrived after {arrived}s");
        assert!(peak > 1.0 && peak < 1.02, "peak {peak}");
    }

    #[test]
    fn close_spring_never_overshoots() {
        let mut s = Spring::new(1.0);
        s.to(0.0, tok::SPRING_CLOSE);
        for _ in 0..60 {
            s.step(1.0 / 60.0);
            assert!(s.value >= -2e-3, "overshoot {}", s.value);
        }
        assert!(s.settled());
    }

    /// When a spring stepped at 240 Hz was last more than 1 % away: what the eye reads as
    /// "arrived".
    fn settle_time(spring: (f32, f32)) -> f32 {
        let mut s = Spring::new(0.0);
        s.target = 1.0;
        s.omega = spring.0;
        s.zeta = spring.1;
        let (mut t, mut arrived) = (0.0, 0.0);
        while t < 2.0 {
            s.step(1.0 / 240.0);
            t += 1.0 / 240.0;
            if (s.value - 1.0).abs() > 0.01 {
                arrived = t;
            }
        }
        arrived
    }

    #[test]
    fn default_tuning_leaves_every_token_as_it_is() {
        let t = Tuning::default();
        let c = crate::config::MotionConfig::default();
        assert_eq!(Tuning::new(c.speed, c.reduce), t);
        // No test changes the globals: unset, they are the defaults too.
        assert_eq!(now(), t);
        for spring in [
            tok::SPRING_OPEN,
            tok::SPRING_CLOSE,
            tok::SPRING_SHOW,
            tok::SPRING_SNAP,
            tok::SPRING_LEAD,
            tok::SPRING_TRAIL,
        ] {
            assert_eq!(t.spring(spring), Some(spring));
        }
        for d in [tok::HOVER_IN, tok::HOVER_OUT, tok::BREATH] {
            assert_eq!(t.secs(d), d);
        }
        // The glide exactly as the slider and smooth scrolling computed it before.
        for dt in [0.0, 1.0 / 144.0, 1.0 / 60.0, 0.05] {
            assert_eq!(
                t.glide(dt, tok::GLIDE_SLIDER),
                1.0 - (-dt * tok::GLIDE_SLIDER).exp()
            );
            assert_eq!(
                t.glide(dt, tok::GLIDE_SCROLL),
                1.0 - (-dt * tok::GLIDE_SCROLL).exp()
            );
        }
    }

    #[test]
    fn speed_scales_every_kind_of_token() {
        let fast = Tuning::new(2.0, false);
        assert_eq!(fast.secs(tok::HOVER_OUT), tok::HOVER_OUT / 2.0);
        assert_eq!(Tuning::new(0.5, false).secs(tok::BREATH), tok::BREATH * 2.0);
        // A glide at twice the speed covers in one frame what it covered in two.
        let dt = 1.0 / 60.0;
        let one = Tuning::default().glide(dt, tok::GLIDE_SLIDER);
        let two_frames = 1.0 - (1.0 - one) * (1.0 - one);
        assert!((fast.glide(dt, tok::GLIDE_SLIDER) - two_frames).abs() < 1e-5);
        // A spring at twice the speed settles in about half the time, with the same damping.
        let (omega, zeta) = fast.spring(tok::SPRING_OPEN).unwrap();
        assert_eq!(
            (omega, zeta),
            (tok::SPRING_OPEN.0 * 2.0, tok::SPRING_OPEN.1)
        );
        let (slow, quick) = (settle_time(tok::SPRING_OPEN), settle_time((omega, zeta)));
        assert!((quick / slow - 0.5).abs() < 0.1, "{slow}s vs {quick}s");
    }

    #[test]
    fn reduce_makes_everything_arrive_at_once() {
        for t in [
            Tuning::new(1.0, true),
            Tuning::new(0.0, false),
            Tuning::new(-1.0, false),
            Tuning::new(f32::NAN, false),
        ] {
            assert!(t.off);
            assert_eq!(t.secs(tok::HOVER_IN), 0.0);
            assert_eq!(t.glide(0.0, tok::GLIDE_SCROLL), 1.0);
            assert_eq!(t.spring(tok::SPRING_OPEN), None);
        }
    }

    #[test]
    fn a_very_fast_spring_stays_stable() {
        // At ten times the speed SHOW is ω 600: still settles, never blows up.
        let (omega, zeta) = Tuning::new(10.0, false).spring(tok::SPRING_SHOW).unwrap();
        let mut s = Spring::new(0.0);
        s.target = 1.0;
        s.omega = omega;
        s.zeta = zeta;
        for _ in 0..30 {
            s.step(1.0 / 60.0);
            assert!(s.value.is_finite() && s.value < 1.01, "{}", s.value);
        }
        assert!(s.settled());
    }
}
