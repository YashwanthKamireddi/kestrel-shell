//! Motion primitives. Springs give panels their weight; tweens handle simple fades.
//! Both are stepped with real elapsed time, so a slow frame never makes motion slower.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

/// Game mode: no motion anywhere in the shell. Every spring arrives at once, so nothing asks
/// for animation frames and no CPU goes to movement while a game runs.
static STILL: AtomicBool = AtomicBool::new(false);

pub fn set_still(on: bool) {
    STILL.store(on, Ordering::Relaxed);
}

pub fn still() -> bool {
    STILL.load(Ordering::Relaxed)
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
    /// Panel opening: settles in ~280 ms with 0.6 % overshoot (Material 3 expressive stiffness,
    /// damped further; docs/muse-ux report, 2026-11-12). Visible growth, no bounce.
    pub const OPEN: (f32, f32) = (19.5, 0.85);
    /// Closing and quick moves: ~160 ms, no visible overshoot. Exits run faster than entries.
    pub const CLOSE: (f32, f32) = (30.0, 0.95);
    /// Small UI elements (workspace pill): snappy, barely any overshoot.
    pub const SNAP: (f32, f32) = (24.0, 0.86);

    pub fn new(value: f32) -> Self {
        Self {
            value,
            velocity: 0.0,
            target: value,
            omega: Self::OPEN.0,
            zeta: Self::OPEN.1,
        }
    }

    pub fn to(&mut self, target: f32, (omega, zeta): (f32, f32)) {
        self.target = target;
        self.omega = omega;
        self.zeta = zeta;
        if still() {
            self.snap(target);
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
        while left > 0.0 {
            let h = left.min(1.0 / 240.0);
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
        s.to(1.0, Spring::OPEN);
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
        s.to(0.0, Spring::CLOSE);
        for _ in 0..60 {
            s.step(1.0 / 60.0);
            assert!(s.value >= -2e-3, "overshoot {}", s.value);
        }
        assert!(s.settled());
    }
}
