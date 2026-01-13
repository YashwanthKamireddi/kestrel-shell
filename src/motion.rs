//! Motion primitives. Springs give panels their weight; tweens handle simple fades.
//! Both are stepped with real elapsed time, so a slow frame never makes motion slower.

use std::time::Instant;

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

impl Spring {
    /// Panel opening: ~340 ms with a slight settle.
    pub const OPEN: (f32, f32) = (17.0, 0.82);
    /// Closing and quick moves: no overshoot, ~200 ms.
    pub const CLOSE: (f32, f32) = (26.0, 1.0);
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
        while !s.settled() && t < 2.0 {
            s.step(1.0 / 60.0);
            peak = peak.max(s.value);
            t += 1.0 / 60.0;
        }
        assert!(t < 0.6, "took {t}s");
        assert!(peak > 1.0 && peak < 1.05, "peak {peak}");
    }

    #[test]
    fn close_spring_never_overshoots() {
        let mut s = Spring::new(1.0);
        s.to(0.0, Spring::CLOSE);
        for _ in 0..60 {
            s.step(1.0 / 60.0);
            assert!(s.value >= -1e-3);
        }
        assert!(s.settled());
    }
}
