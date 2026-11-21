//! Gamepad aim assist: look slowdown near targets and rotational magnetism.

use bevy::prelude::*;
use game_core::input::ViewAngles;

/// How strongly magnetism closes the angular gap per second at full strength.
pub const MAGNETISM_RATE_PER_SECOND: f32 = 10.0;

/// Tunable aim assist.
#[derive(Resource, Reflect, Debug, Clone, PartialEq)]
#[reflect(Resource, Default, Debug)]
pub struct AimAssistSettings {
    pub enabled: bool,
    /// Inside this cone the view is pulled toward the target.
    pub inner_cone_angle_degrees: f32,
    /// Inside this cone stick sensitivity is scaled by `slowdown_factor`.
    pub outer_cone_angle_degrees: f32,
    pub slowdown_factor: f32,
    pub magnetism_strength: f32,
}

impl Default for AimAssistSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            inner_cone_angle_degrees: 5.0,
            outer_cone_angle_degrees: 12.0,
            slowdown_factor: 0.40,
            magnetism_strength: 0.45,
        }
    }
}

/// Marks an entity that aim assist may lock onto. The target point is the
/// entity's `GlobalTransform` translation plus `offset`.
#[derive(Component, Reflect, Debug, Default, Clone, Copy, PartialEq)]
#[reflect(Component, Default, Debug)]
pub struct AimAssistTarget {
    pub offset: Vec3,
}

/// Result of evaluating aim assist for one view.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AimAssistResult {
    /// Multiplier for stick look sensitivity (1.0 = no slowdown).
    pub sensitivity_scale: f32,
    /// View that looks straight at the target inside the inner cone.
    pub magnetism_target: Option<ViewAngles>,
    /// Angle to the best target, in degrees.
    pub target_angle_degrees: Option<f32>,
}

impl AimAssistResult {
    pub const NONE: Self = Self {
        sensitivity_scale: 1.0,
        magnetism_target: None,
        target_angle_degrees: None,
    };
}

/// Angle in degrees between `forward` and the direction from `eye` to
/// `target`, or `None` when the target is at the eye.
pub fn angle_to_target_degrees(eye: Vec3, forward: Vec3, target: Vec3) -> Option<f32> {
    let to_target = (target - eye).try_normalize()?;
    let forward = forward.try_normalize()?;
    Some(forward.angle_between(to_target).to_degrees())
}

/// Picks the target closest to the view direction and computes slowdown and
/// magnetism for it.
pub fn evaluate_aim_assist(
    eye: Vec3,
    forward: Vec3,
    targets: impl IntoIterator<Item = Vec3>,
    settings: &AimAssistSettings,
) -> AimAssistResult {
    if !settings.enabled {
        return AimAssistResult::NONE;
    }
    let best = targets
        .into_iter()
        .filter_map(|t| angle_to_target_degrees(eye, forward, t).map(|a| (a, t)))
        .filter(|(angle, _)| *angle <= settings.outer_cone_angle_degrees)
        .min_by(|a, b| a.0.total_cmp(&b.0));
    let Some((angle, target)) = best else {
        return AimAssistResult::NONE;
    };
    let magnetism_target = (angle <= settings.inner_cone_angle_degrees)
        .then(|| ViewAngles::looking_along(target - eye));
    AimAssistResult {
        sensitivity_scale: settings.slowdown_factor.clamp(0.0, 1.0),
        magnetism_target,
        target_angle_degrees: Some(angle),
    }
}

/// Pulls `view` toward `target` by a fraction of the remaining gap scaled by
/// `strength` and `dt`. Yaw takes the short way around.
pub fn apply_magnetism(view: &mut ViewAngles, target: ViewAngles, strength: f32, dt: f32) {
    let fraction = (strength.max(0.0) * MAGNETISM_RATE_PER_SECOND * dt.max(0.0)).min(1.0);
    let yaw_gap = game_core::math::wrap_angle_radians(target.yaw - view.yaw);
    let pitch_gap = target.pitch - view.pitch;
    view.rotate(yaw_gap * fraction, pitch_gap * fraction);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target_at_degrees(degrees: f32) -> Vec3 {
        // 10 m ahead of an eye at the origin looking down -Z, rotated
        // `degrees` to the left.
        Quat::from_rotation_y(degrees.to_radians()) * Vec3::new(0.0, 0.0, -10.0)
    }

    #[test]
    fn angle_math() {
        let a = angle_to_target_degrees(Vec3::ZERO, Vec3::NEG_Z, target_at_degrees(30.0));
        assert!((a.expect("angle") - 30.0).abs() < 1e-3);
        assert_eq!(
            angle_to_target_degrees(Vec3::ZERO, Vec3::NEG_Z, Vec3::ZERO),
            None
        );
    }

    #[test]
    fn inside_inner_cone_slows_and_pulls() {
        let s = AimAssistSettings::default();
        let r = evaluate_aim_assist(Vec3::ZERO, Vec3::NEG_Z, [target_at_degrees(3.0)], &s);
        assert_eq!(r.sensitivity_scale, 0.40);
        let target = r.magnetism_target.expect("magnetism inside inner cone");
        assert!((target.yaw.to_degrees() - 3.0).abs() < 1e-3);
    }

    #[test]
    fn inside_outer_cone_only_slows() {
        let s = AimAssistSettings::default();
        let r = evaluate_aim_assist(Vec3::ZERO, Vec3::NEG_Z, [target_at_degrees(8.0)], &s);
        assert_eq!(r.sensitivity_scale, 0.40);
        assert!(r.magnetism_target.is_none());
    }

    #[test]
    fn outside_cones_does_nothing() {
        let s = AimAssistSettings::default();
        let r = evaluate_aim_assist(Vec3::ZERO, Vec3::NEG_Z, [target_at_degrees(20.0)], &s);
        assert_eq!(r, AimAssistResult::NONE);
        let behind = evaluate_aim_assist(Vec3::ZERO, Vec3::NEG_Z, [Vec3::new(0.0, 0.0, 10.0)], &s);
        assert_eq!(behind, AimAssistResult::NONE);
    }

    #[test]
    fn disabled_does_nothing() {
        let s = AimAssistSettings {
            enabled: false,
            ..Default::default()
        };
        let r = evaluate_aim_assist(Vec3::ZERO, Vec3::NEG_Z, [target_at_degrees(1.0)], &s);
        assert_eq!(r, AimAssistResult::NONE);
    }

    #[test]
    fn closest_target_wins() {
        let s = AimAssistSettings::default();
        let r = evaluate_aim_assist(
            Vec3::ZERO,
            Vec3::NEG_Z,
            [
                target_at_degrees(10.0),
                target_at_degrees(-2.0),
                target_at_degrees(4.0),
            ],
            &s,
        );
        assert!((r.target_angle_degrees.expect("angle") - 2.0).abs() < 1e-3);
    }

    #[test]
    fn magnetism_converges_without_overshoot() {
        let target = ViewAngles::new(0.05, 0.02);
        let mut view = ViewAngles::default();
        let mut previous_gap = f32::INFINITY;
        for _ in 0..120 {
            apply_magnetism(&mut view, target, 0.45, 1.0 / 60.0);
            let gap = (target.yaw - view.yaw).abs() + (target.pitch - view.pitch).abs();
            assert!(gap <= previous_gap + 1e-7);
            previous_gap = gap;
        }
        assert!(previous_gap < 1e-4);
        // Yaw pulls across the +-PI seam the short way.
        let mut seam = ViewAngles::new(3.1, 0.0);
        apply_magnetism(&mut seam, ViewAngles::new(-3.1, 0.0), 1.0, 0.05);
        assert!(
            seam.yaw > 3.1 || seam.yaw < -3.0,
            "went the long way: {}",
            seam.yaw
        );
    }
}
