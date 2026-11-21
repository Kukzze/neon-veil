//! Procedural recoil on a critically damped harmonic oscillator.
//!
//! `x'' + 2 zeta w0 x' + w0^2 x = 0` with `zeta = 1` is integrated with its
//! exact closed-form step, so the result is stable for any `dt` and never
//! overshoots past rest: starting displaced and/or moving away from rest, the
//! offset decays monotonically back toward zero once it peaks.

use bevy::prelude::*;
use game_core::math::sanitize_f32;

pub const DEFAULT_RECOIL_OMEGA: f32 = 22.0;
pub const DEFAULT_RECOIL_DAMPING_RATIO: f32 = 1.0;

/// Deterministic value in `[0, 1)` from a 32-bit seed (splitmix-style
/// integer hash, identical on every platform).
pub fn hash_unit(seed: u32) -> f32 {
    let mut x = seed.wrapping_add(0x9E37_79B9);
    x = (x ^ (x >> 16)).wrapping_mul(0x85EB_CA6B);
    x = (x ^ (x >> 13)).wrapping_mul(0xC2B2_AE35);
    x ^= x >> 16;
    (x >> 8) as f32 / (1u32 << 24) as f32
}

/// Deterministic value in `[-1, 1)` from a seed.
pub fn hash_signed(seed: u32) -> f32 {
    hash_unit(seed) * 2.0 - 1.0
}

/// Camera / weapon kick offset, in degrees (`x` = pitch up, `y` = yaw left).
#[derive(Component, Reflect, Debug, Clone, Copy, PartialEq)]
#[reflect(Component, Default, Debug, PartialEq)]
pub struct RecoilSpring {
    pub current_offset: Vec2,
    pub velocity: Vec2,
    pub omega: f32,
    /// Only `1.0` (critical damping) is supported by the closed-form step.
    pub damping_ratio: f32,
}

impl Default for RecoilSpring {
    fn default() -> Self {
        Self {
            current_offset: Vec2::ZERO,
            velocity: Vec2::ZERO,
            omega: DEFAULT_RECOIL_OMEGA,
            damping_ratio: DEFAULT_RECOIL_DAMPING_RATIO,
        }
    }
}

impl RecoilSpring {
    /// Kicks the spring. The impulse is scaled so that, from rest, the pitch
    /// offset peaks at exactly `pitch_degrees` (a critically damped spring
    /// kicked with velocity `v` peaks at `v / (w0 e)` after `1 / w0` s). Yaw
    /// kicks by up to `yaw_random_degrees` left or right, chosen
    /// deterministically from `rng_seed`.
    pub fn punch(&mut self, pitch_degrees: f32, yaw_random_degrees: f32, rng_seed: f32) {
        let gain = self.omega.max(1e-3) * std::f32::consts::E;
        let yaw = yaw_random_degrees * hash_signed(rng_seed.to_bits());
        self.velocity += Vec2::new(sanitize_f32(pitch_degrees), sanitize_f32(yaw)) * gain;
    }

    /// Advances the spring toward rest by `dt` with the exact critically
    /// damped solution:
    /// `x(t+dt) = (c1 + c2 dt) e^(-w dt)`,
    /// `v(t+dt) = (c2 - w (c1 + c2 dt)) e^(-w dt)`,
    /// with `c1 = x`, `c2 = v + w x`.
    pub fn update(&mut self, dt: f32) {
        let dt = sanitize_f32(dt).max(0.0);
        let w = sanitize_f32(self.omega).max(0.0);
        let decay = (-w * dt).exp();
        let c1 = self.current_offset;
        let c2 = self.velocity + c1 * w;
        let position = (c1 + c2 * dt) * decay;
        let velocity = (c2 - (c1 + c2 * dt) * w) * decay;
        self.current_offset = sanitize_vec2(position);
        self.velocity = sanitize_vec2(velocity);
    }

    /// Kick as a rotation to compose with the view (`EulerRot::YXZ`).
    pub fn rotation(&self) -> Quat {
        Quat::from_euler(
            EulerRot::YXZ,
            self.current_offset.y.to_radians(),
            self.current_offset.x.to_radians(),
            0.0,
        )
    }
}

fn sanitize_vec2(v: Vec2) -> Vec2 {
    Vec2::new(sanitize_f32(v.x), sanitize_f32(v.y))
}

/// Settles every recoil spring on the fixed clock.
pub fn update_recoil_springs(time: Res<Time<Fixed>>, mut springs: Query<&mut RecoilSpring>) {
    let dt = time.delta_secs();
    for mut spring in &mut springs {
        if spring.current_offset != Vec2::ZERO || spring.velocity != Vec2::ZERO {
            spring.update(dt);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_core::time::FIXED_TIMESTEP_SECS as DT;

    /// Punches and returns (peak pitch, pitch after 0.35 s, minimum pitch).
    fn kick_profile(pitch: f32) -> (f32, f32, f32) {
        let mut s = RecoilSpring::default();
        s.punch(pitch, 0.0, 1.0);
        let mut peak = 0.0_f32;
        let mut min = f32::INFINITY;
        let mut at_350ms = 0.0;
        // First fixed tick at or after 0.35 s (tick 23 = 0.359 s).
        let ticks_350 = (0.35 / DT).ceil() as u32;
        for tick in 1..=256 {
            s.update(DT);
            peak = peak.max(s.current_offset.x);
            min = min.min(s.current_offset.x);
            if tick == ticks_350 {
                at_350ms = s.current_offset.x;
            }
        }
        (peak, at_350ms, min)
    }

    #[test]
    fn punch_kicks_pitch_upward_to_the_requested_peak() {
        let (peak, _, _) = kick_profile(0.6);
        // Sampled at 64 Hz the analytic peak (at t = 1/22 s) is just missed.
        assert!(peak > 0.55 && peak <= 0.6 + 1e-4, "peak {peak}");
    }

    #[test]
    fn smg_kick_settles_within_001_degrees_in_350ms() {
        let (_, at_350ms, _) = kick_profile(0.6);
        assert!(
            at_350ms.abs() < 0.01,
            "0.6 deg kick: {at_350ms} after 0.35 s"
        );
    }

    #[test]
    fn every_kick_settles_below_one_percent_of_peak_in_350ms() {
        // x(0.35) / peak = 0.35 w e e^(-0.35 w) = 0.95 % at w = 22, so the
        // absolute 0.01 degree bound only holds for kicks up to ~1.05 deg.
        for pitch in [0.6, 1.1, 1.4, 4.2, 5.2, 6.5] {
            let (peak, at_350ms, _) = kick_profile(pitch);
            assert!(
                at_350ms / peak < 0.01,
                "{pitch} deg kick: {at_350ms} of {peak}"
            );
        }
    }

    #[test]
    fn no_weapon_overshoots_below_zero() {
        for pitch in [0.6, 1.1, 1.4, 4.2, 5.2, 6.5] {
            let (_, _, min) = kick_profile(pitch);
            assert!(min >= 0.0, "{pitch} deg kick dipped to {min}");
        }
    }

    #[test]
    fn rapid_fire_punches_never_overshoot_and_settle() {
        let mut s = RecoilSpring::default();
        for i in 0..20 {
            if i % 4 == 0 {
                s.punch(0.6, 0.0, i as f32);
            }
            s.update(DT);
            assert!(s.current_offset.x >= 0.0);
        }
        for _ in 0..64 {
            let before = s.current_offset.x;
            s.update(DT);
            assert!(s.current_offset.x >= 0.0 && s.current_offset.x <= before + 1e-6);
        }
        assert!(s.current_offset.x < 0.01);
    }

    #[test]
    fn closed_form_step_is_dt_independent() {
        let mut fine = RecoilSpring::default();
        let mut coarse = RecoilSpring::default();
        fine.punch(2.0, 0.0, 0.0);
        coarse.punch(2.0, 0.0, 0.0);
        for _ in 0..64 {
            fine.update(1.0 / 64.0);
        }
        for _ in 0..4 {
            coarse.update(0.25);
        }
        assert!((fine.current_offset.x - coarse.current_offset.x).abs() < 1e-5);
    }

    #[test]
    fn yaw_kick_is_deterministic_and_bounded() {
        let mut a = RecoilSpring::default();
        let mut b = RecoilSpring::default();
        a.punch(1.0, 0.45, 17.0);
        b.punch(1.0, 0.45, 17.0);
        assert_eq!(a, b, "same seed, same kick");
        let mut c = RecoilSpring::default();
        c.punch(1.0, 0.45, 18.0);
        assert_ne!(a.velocity.y, c.velocity.y, "different seed, different yaw");
        for seed in 0..1000_u32 {
            let v = hash_signed(seed);
            assert!((-1.0..1.0).contains(&v));
        }
    }

    #[test]
    fn hostile_inputs_are_sanitized() {
        let mut s = RecoilSpring::default();
        s.punch(f32::NAN, f32::INFINITY, f32::NAN);
        s.update(f32::NAN);
        assert!(s.current_offset.is_finite() && s.velocity.is_finite());
    }
}
