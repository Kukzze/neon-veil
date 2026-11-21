//! First-person eye placement and procedural head bob.
//!
//! The bob phase is integrated (`phase += f_stride * dt`, in stride cycles)
//! rather than computed as `f_stride * t`, so changing speed changes the
//! bob frequency without jumping the camera.

use bevy::prelude::*;
use std::f32::consts::{PI, TAU};

/// Distance covered per vertical bob cycle, m.
pub const STRIDE_LENGTH_METERS: f32 = 1.85;
pub const BOB_AMPLITUDE_Y: f32 = 0.045;
pub const BOB_AMPLITUDE_X: f32 = 0.022;
/// Rate at which the bob fades in and out, 1/s.
pub const BOB_BLEND_RATE: f32 = 10.0;
/// Below this planar speed the character counts as stationary.
pub const MIN_BOB_SPEED: f32 = 0.1;

/// Vertical bob cycles per second at `speed` m/s.
pub fn stride_frequency(speed: f32) -> f32 {
    speed.max(0.0) / STRIDE_LENGTH_METERS
}

/// Advances the bob phase and fade weight. The phase stays in `[0, 2)`,
/// the period of the horizontal sway.
pub fn advance_head_bob(
    phase: f32,
    weight: f32,
    speed: f32,
    grounded: bool,
    dt: f32,
) -> (f32, f32) {
    let dt = dt.max(0.0);
    let active = grounded && speed > MIN_BOB_SPEED;
    let target_weight = if active { 1.0 } else { 0.0 };
    let blend = 1.0 - (-BOB_BLEND_RATE * dt).exp();
    let weight = weight + (target_weight - weight) * blend;
    let phase = if active {
        (phase + stride_frequency(speed) * dt).rem_euclid(2.0)
    } else {
        phase
    };
    (phase, weight)
}

/// Local-space bob offset: `x = A_x cos(pi phase)`, `y = A_y sin(2 pi phase)`,
/// scaled by the fade weight.
pub fn head_bob_offset(phase: f32, weight: f32) -> Vec3 {
    let weight = weight.clamp(0.0, 1.0);
    Vec3::new(
        BOB_AMPLITUDE_X * (PI * phase).cos(),
        BOB_AMPLITUDE_Y * (TAU * phase).sin(),
        0.0,
    ) * weight
}

/// Eye position: feet plus the yaw-rotated eye offset and bob.
pub fn eye_position(feet: Vec3, yaw: f32, eye_offset: Vec3, bob: Vec3) -> Vec3 {
    feet + Quat::from_rotation_y(yaw) * (eye_offset + bob)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 60.0;

    #[test]
    fn no_bob_when_stationary() {
        let (mut phase, mut weight) = (0.0, 0.0);
        for _ in 0..120 {
            (phase, weight) = advance_head_bob(phase, weight, 0.0, true, DT);
        }
        assert_eq!(phase, 0.0);
        assert_eq!(weight, 0.0);
        assert_eq!(head_bob_offset(phase, weight), Vec3::ZERO);
    }

    #[test]
    fn frequency_scales_with_speed() {
        assert_eq!(stride_frequency(0.0), 0.0);
        assert!((stride_frequency(3.7) - 2.0).abs() < 1e-6);
        assert!((stride_frequency(7.4) - 4.0).abs() < 1e-6);

        // 0.25 s at 3.7 m/s is half a vertical cycle.
        let mut phase = 0.0;
        let mut weight = 1.0;
        for _ in 0..15 {
            (phase, weight) = advance_head_bob(phase, weight, 3.7, true, DT);
        }
        assert!((phase - 0.5).abs() < 1e-4, "phase {phase}");
        assert!(weight > 0.99);
    }

    #[test]
    fn bob_shape_and_amplitudes() {
        assert!(head_bob_offset(0.25, 1.0).abs_diff_eq(
            Vec3::new(BOB_AMPLITUDE_X * (PI * 0.25).cos(), BOB_AMPLITUDE_Y, 0.0),
            1e-6
        ));
        // Horizontal sway has half the vertical frequency.
        assert!(head_bob_offset(0.0, 1.0).abs_diff_eq(Vec3::new(BOB_AMPLITUDE_X, 0.0, 0.0), 1e-6));
        assert!(head_bob_offset(1.0, 1.0).abs_diff_eq(Vec3::new(-BOB_AMPLITUDE_X, 0.0, 0.0), 1e-5));
        for i in 0..100 {
            let o = head_bob_offset(i as f32 * 0.02, 1.0);
            assert!(o.y.abs() <= BOB_AMPLITUDE_Y + 1e-6 && o.x.abs() <= BOB_AMPLITUDE_X + 1e-6);
        }
    }

    #[test]
    fn bob_fades_out_in_the_air() {
        let (mut phase, mut weight) = (0.3, 1.0);
        for _ in 0..60 {
            (phase, weight) = advance_head_bob(phase, weight, 12.8, false, DT);
        }
        assert!(weight < 1e-3, "weight {weight}");
        assert!(head_bob_offset(phase, weight).length() < 1e-4);
        assert_eq!(phase, 0.3, "phase frozen while airborne");
    }

    #[test]
    fn eye_follows_yaw() {
        let offset = Vec3::new(0.0, 1.68, -0.12);
        assert!(eye_position(Vec3::ZERO, 0.0, offset, Vec3::ZERO).abs_diff_eq(offset, 1e-6));
        let turned = eye_position(Vec3::ZERO, std::f32::consts::FRAC_PI_2, offset, Vec3::ZERO);
        assert!(
            turned.abs_diff_eq(Vec3::new(-0.12, 1.68, 0.0), 1e-5),
            "{turned:?}"
        );
    }
}
