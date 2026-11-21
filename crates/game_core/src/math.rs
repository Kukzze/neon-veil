//! Deterministic, NaN-safe spatial math shared by client and server.
//!
//! Every helper in this module is total: it never panics and never returns a
//! non-finite value, so its output can be written straight into a `Transform`
//! or a network packet without risking a physics crash or a desync.

use bevy::math::{Quat, Vec3};
use std::f32::consts::{PI, TAU};

/// Returns `x` if it is finite, otherwise `0.0`.
#[inline]
pub fn sanitize_f32(x: f32) -> f32 {
    if x.is_finite() {
        x
    } else {
        0.0
    }
}

/// Replaces every `NaN`, `+INFINITY` or `-INFINITY` component with `0.0`,
/// keeping the finite components untouched.
#[inline]
pub fn sanitize_vec3(v: Vec3) -> Vec3 {
    Vec3::new(sanitize_f32(v.x), sanitize_f32(v.y), sanitize_f32(v.z))
}

/// Returns `v` if all components are finite, otherwise the whole `fallback`.
///
/// Use this for positions, where zeroing a single axis would teleport the
/// entity; pass the last verified position as `fallback`. A non-finite
/// `fallback` is itself sanitized.
#[inline]
pub fn sanitize_vec3_or(v: Vec3, fallback: Vec3) -> Vec3 {
    if v.is_finite() {
        v
    } else {
        sanitize_vec3(fallback)
    }
}

/// Returns a unit-length copy of `q`, or `fallback` if `q` is non-finite or
/// too close to zero length to normalize.
#[inline]
pub fn sanitize_quat(q: Quat, fallback: Quat) -> Quat {
    let length_squared = q.length_squared();
    if q.is_finite() && length_squared.is_finite() && length_squared > f32::EPSILON {
        q.normalize()
    } else {
        fallback
    }
}

/// Normalizes `v`, returning `fallback` when `v` is zero-length or non-finite.
#[inline]
pub fn safe_normalize(v: Vec3, fallback: Vec3) -> Vec3 {
    v.try_normalize().unwrap_or(fallback)
}

/// Sanitizes `velocity` and limits its magnitude to `max_speed`.
///
/// A negative or non-finite `max_speed` is treated as `0.0`.
#[inline]
pub fn clamp_velocity(velocity: Vec3, max_speed: f32) -> Vec3 {
    let max_speed = sanitize_f32(max_speed).max(0.0);
    sanitize_vec3(velocity).clamp_length_max(max_speed)
}

/// Wraps `angle` (radians) into the half-open interval `[-PI, PI)`.
///
/// Non-finite input yields `0.0`.
#[inline]
pub fn wrap_angle_radians(angle: f32) -> f32 {
    if !angle.is_finite() {
        return 0.0;
    }
    let mut shifted = (angle + PI).rem_euclid(TAU);
    // `rem_euclid` may round up to exactly `TAU` for tiny negative inputs.
    if shifted >= TAU {
        shifted = 0.0;
    }
    shifted - PI
}

/// Cubic Hermite interpolation between `p0` and `p1`.
///
/// `v0` and `v1` are the tangents at each end expressed per unit of `t`;
/// when interpolating between two snapshots `dt` seconds apart, pass
/// `velocity * dt`. `t` is clamped to `[0, 1]` (non-finite `t` is treated as
/// `0.0`) and the result is sanitized.
#[inline]
pub fn hermite_interpolate(p0: Vec3, v0: Vec3, p1: Vec3, v1: Vec3, t: f32) -> Vec3 {
    let t = sanitize_f32(t).clamp(0.0, 1.0);
    let t2 = t * t;
    let t3 = t2 * t;

    let h00 = 2.0 * t3 - 3.0 * t2 + 1.0;
    let h10 = t3 - 2.0 * t2 + t;
    let h01 = -2.0 * t3 + 3.0 * t2;
    let h11 = t3 - t2;

    sanitize_vec3(p0 * h00 + v0 * h10 + p1 * h01 + v1 * h11)
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f32 = 1e-5;

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() <= EPS
    }

    fn approx_vec(a: Vec3, b: Vec3) -> bool {
        approx(a.x, b.x) && approx(a.y, b.y) && approx(a.z, b.z)
    }

    #[test]
    fn sanitize_vec3_zeroes_each_non_finite_component() {
        let v = Vec3::new(f32::NAN, 2.5, f32::INFINITY);
        assert_eq!(sanitize_vec3(v), Vec3::new(0.0, 2.5, 0.0));

        let v = Vec3::new(-7.0, f32::NEG_INFINITY, f32::NAN);
        assert_eq!(sanitize_vec3(v), Vec3::new(-7.0, 0.0, 0.0));

        let all_bad = Vec3::new(f32::NAN, f32::INFINITY, f32::NEG_INFINITY);
        assert_eq!(sanitize_vec3(all_bad), Vec3::ZERO);
    }

    #[test]
    fn sanitize_vec3_preserves_finite_extremes() {
        let v = Vec3::new(f32::MAX, f32::MIN, f32::MIN_POSITIVE);
        assert_eq!(sanitize_vec3(v), v);
        assert_eq!(sanitize_vec3(Vec3::splat(-0.0)), Vec3::splat(-0.0));
    }

    #[test]
    fn sanitize_vec3_or_reverts_whole_vector_to_fallback() {
        let fallback = Vec3::new(1.0, 2.0, 3.0);
        let v = Vec3::new(10.0, f32::NAN, 10.0);
        assert_eq!(sanitize_vec3_or(v, fallback), fallback);

        let good = Vec3::new(4.0, 5.0, 6.0);
        assert_eq!(sanitize_vec3_or(good, fallback), good);

        let bad_fallback = Vec3::new(f32::INFINITY, 1.0, f32::NAN);
        assert_eq!(
            sanitize_vec3_or(Vec3::NAN, bad_fallback),
            Vec3::new(0.0, 1.0, 0.0)
        );
    }

    #[test]
    fn sanitize_quat_normalizes_or_falls_back() {
        let q = Quat::from_xyzw(0.0, 0.0, 0.0, 2.0);
        assert!(approx(sanitize_quat(q, Quat::IDENTITY).length(), 1.0));

        let nan = Quat::from_xyzw(f32::NAN, 0.0, 0.0, 1.0);
        assert_eq!(sanitize_quat(nan, Quat::IDENTITY), Quat::IDENTITY);

        let zero = Quat::from_xyzw(0.0, 0.0, 0.0, 0.0);
        assert_eq!(sanitize_quat(zero, Quat::IDENTITY), Quat::IDENTITY);

        let huge = Quat::from_xyzw(f32::MAX, f32::MAX, 0.0, 0.0);
        assert_eq!(sanitize_quat(huge, Quat::IDENTITY), Quat::IDENTITY);
    }

    #[test]
    fn safe_normalize_handles_degenerate_input() {
        assert_eq!(safe_normalize(Vec3::ZERO, Vec3::Y), Vec3::Y);
        assert_eq!(safe_normalize(Vec3::NAN, Vec3::Y), Vec3::Y);
        assert_eq!(safe_normalize(Vec3::INFINITY, Vec3::Y), Vec3::Y);
        assert!(approx_vec(
            safe_normalize(Vec3::new(3.0, 0.0, 4.0), Vec3::Y),
            Vec3::new(0.6, 0.0, 0.8)
        ));
    }

    #[test]
    fn clamp_velocity_limits_magnitude_and_strips_nan() {
        let clamped = clamp_velocity(Vec3::new(30.0, 0.0, 40.0), 25.0);
        assert!(approx(clamped.length(), 25.0));
        assert!(approx_vec(clamped.normalize(), Vec3::new(0.6, 0.0, 0.8)));

        let slow = Vec3::new(1.0, 2.0, 2.0);
        assert_eq!(clamp_velocity(slow, 25.0), slow);

        let injected = clamp_velocity(Vec3::new(f32::NAN, 0.0, f32::INFINITY), 25.0);
        assert_eq!(injected, Vec3::ZERO);

        assert_eq!(clamp_velocity(slow, -5.0), Vec3::ZERO);
        assert_eq!(clamp_velocity(slow, f32::NAN), Vec3::ZERO);
        assert_eq!(clamp_velocity(slow, f32::INFINITY), Vec3::ZERO);
    }

    #[test]
    fn wrap_angle_stays_in_half_open_range() {
        assert!(approx(wrap_angle_radians(0.0), 0.0));
        assert!(approx(wrap_angle_radians(PI), -PI));
        assert!(approx(wrap_angle_radians(-PI), -PI));
        assert!(approx(wrap_angle_radians(TAU), 0.0));
        assert!(approx(wrap_angle_radians(3.0 * PI / 2.0), -PI / 2.0));
        assert!(approx(wrap_angle_radians(-3.0 * PI / 2.0), PI / 2.0));
        assert!(approx(wrap_angle_radians(7.0 * TAU + 0.5), 0.5));
        assert!(approx(wrap_angle_radians(-7.0 * TAU - 0.5), -0.5));

        for &angle in &[
            -f32::EPSILON,
            -1e-30,
            PI - 1e-6,
            -PI + 1e-6,
            1.0e6,
            -1.0e6,
            f32::MAX,
            f32::MIN,
        ] {
            let wrapped = wrap_angle_radians(angle);
            assert!(
                (-PI..PI).contains(&wrapped),
                "{angle} wrapped to {wrapped}, outside [-PI, PI)"
            );
        }
    }

    #[test]
    fn wrap_angle_rejects_non_finite() {
        assert_eq!(wrap_angle_radians(f32::NAN), 0.0);
        assert_eq!(wrap_angle_radians(f32::INFINITY), 0.0);
        assert_eq!(wrap_angle_radians(f32::NEG_INFINITY), 0.0);
    }

    #[test]
    fn hermite_hits_endpoints_and_clamps_t() {
        let p0 = Vec3::new(0.0, 1.0, 2.0);
        let p1 = Vec3::new(10.0, -1.0, 5.0);
        let v0 = Vec3::new(3.0, 0.0, 0.0);
        let v1 = Vec3::new(0.0, 4.0, 0.0);

        assert!(approx_vec(hermite_interpolate(p0, v0, p1, v1, 0.0), p0));
        assert!(approx_vec(hermite_interpolate(p0, v0, p1, v1, 1.0), p1));
        assert!(approx_vec(hermite_interpolate(p0, v0, p1, v1, -3.0), p0));
        assert!(approx_vec(hermite_interpolate(p0, v0, p1, v1, 9.0), p1));
        assert!(approx_vec(
            hermite_interpolate(p0, v0, p1, v1, f32::NAN),
            p0
        ));
    }

    #[test]
    fn hermite_with_linear_tangents_is_linear() {
        let p0 = Vec3::ZERO;
        let p1 = Vec3::new(8.0, 0.0, -4.0);
        let tangent = p1 - p0;
        for i in 0..=10 {
            let t = i as f32 / 10.0;
            let expected = p0.lerp(p1, t);
            assert!(approx_vec(
                hermite_interpolate(p0, tangent, p1, tangent, t),
                expected
            ));
        }
    }

    #[test]
    fn hermite_matches_tangent_at_start() {
        let p0 = Vec3::ZERO;
        let p1 = Vec3::new(1.0, 0.0, 0.0);
        let v0 = Vec3::new(0.0, 2.0, 0.0);
        let h = 1e-3;
        let slope = (hermite_interpolate(p0, v0, p1, Vec3::ZERO, h) - p0) / h;
        assert!((slope - v0).length() < 1e-2, "slope {slope:?}");
    }

    #[test]
    fn hermite_sanitizes_poisoned_inputs() {
        let out = hermite_interpolate(Vec3::NAN, Vec3::ZERO, Vec3::INFINITY, Vec3::ZERO, 0.5);
        assert!(out.is_finite());
    }
}
