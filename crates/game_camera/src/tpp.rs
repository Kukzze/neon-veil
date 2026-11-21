//! Third-person spring arm with spherecast occlusion.
//!
//! The camera hangs off a shoulder pivot on a boom pointing backwards
//! (`rotation * +Z`). A sphere swept along the boom shortens it in front of
//! geometry. The pivot itself is first swept out from the head so a shoulder
//! pressed into a wall never puts the pivot inside it.

use crate::{CameraPerspectiveMode, CameraRigController};
use avian3d::prelude::*;
use bevy::prelude::*;

/// Gap kept between the camera sphere and the surface it hit.
pub const SURFACE_CLEARANCE: f32 = 0.15;
/// Rate at which the arm extends back out after an occlusion clears.
pub const ARM_SMOOTHING_RATE: f32 = 18.0;
/// Head height above the feet: origin of the pivot sweep.
pub const HEAD_HEIGHT: f32 = 1.55;
pub const WIDE_PIVOT_HEIGHT: f32 = 1.8;
pub const WIDE_ARM_LENGTH: f32 = 4.5;

/// Something that answers sphere sweeps.
pub trait SphereCaster {
    /// Distance the sphere travels before touching geometry, if it does
    /// within `max_distance`.
    fn cast_sphere(
        &self,
        origin: Vec3,
        direction: Dir3,
        max_distance: f32,
        radius: f32,
    ) -> Option<f32>;
}

/// A caster for empty space.
pub struct NoObstacles;

impl SphereCaster for NoObstacles {
    fn cast_sphere(&self, _: Vec3, _: Dir3, _: f32, _: f32) -> Option<f32> {
        None
    }
}

/// [`SphereCaster`] backed by Avian's spatial query pipeline.
pub struct SpatialSphereCaster<'a, 'w, 's> {
    pub query: &'a SpatialQuery<'w, 's>,
    pub filter: SpatialQueryFilter,
}

impl SphereCaster for SpatialSphereCaster<'_, '_, '_> {
    fn cast_sphere(
        &self,
        origin: Vec3,
        direction: Dir3,
        max_distance: f32,
        radius: f32,
    ) -> Option<f32> {
        let config = ShapeCastConfig {
            max_distance,
            ignore_origin_penetration: true,
            ..ShapeCastConfig::DEFAULT
        };
        self.query
            .cast_shape(
                &Collider::sphere(radius),
                origin,
                Quat::IDENTITY,
                direction,
                &config,
                &self.filter,
            )
            .map(|hit| hit.distance)
    }
}

/// Shoulder pivot for a third-person mode, from the character's feet.
pub fn shoulder_pivot(feet: Vec3, yaw: f32, mode: CameraPerspectiveMode, offset: Vec3) -> Vec3 {
    let yaw_rotation = Quat::from_rotation_y(yaw);
    match mode {
        CameraPerspectiveMode::ThirdPersonLeftShoulder => {
            feet + yaw_rotation * Vec3::new(-offset.x, offset.y, offset.z)
        }
        CameraPerspectiveMode::ThirdPersonWide => feet + Vec3::Y * WIDE_PIVOT_HEIGHT,
        _ => feet + yaw_rotation * offset,
    }
}

/// Unobstructed boom length for `mode`.
pub fn desired_arm_length(mode: CameraPerspectiveMode, rig: &CameraRigController) -> f32 {
    if mode == CameraPerspectiveMode::ThirdPersonWide {
        WIDE_ARM_LENGTH
    } else {
        rig.tpp_arm_length
    }
}

/// Moves `pivot` toward `head` if the sweep from the head hits geometry first.
pub fn clamp_pivot(caster: &impl SphereCaster, head: Vec3, pivot: Vec3, radius: f32) -> Vec3 {
    let offset = pivot - head;
    let distance = offset.length();
    let Ok(direction) = Dir3::new(offset) else {
        return pivot;
    };
    match caster.cast_sphere(head, direction, distance, radius) {
        Some(hit) => head + *direction * (hit - SURFACE_CLEARANCE).max(0.0),
        None => pivot,
    }
}

/// Boom length after occlusion: `(hit - 0.15).max(min)` capped at `desired`,
/// or `desired` when clear.
pub fn occluded_arm_length(
    caster: &impl SphereCaster,
    pivot: Vec3,
    boom_direction: Vec3,
    desired: f32,
    radius: f32,
    min_length: f32,
) -> f32 {
    let Ok(direction) = Dir3::new(boom_direction) else {
        return desired;
    };
    match caster.cast_sphere(pivot, direction, desired, radius) {
        Some(hit) => (hit - SURFACE_CLEARANCE).max(min_length).min(desired),
        None => desired,
    }
}

/// Retracts instantly (the camera must never sit inside geometry) and
/// extends smoothly: `current + (target - current) * min(18 dt, 1)`.
pub fn smooth_arm_length(current: f32, target: f32, dt: f32) -> f32 {
    if target <= current {
        target
    } else {
        current + (target - current) * (ARM_SMOOTHING_RATE * dt.max(0.0)).min(1.0)
    }
}

/// Camera position for a third-person mode. Updates `rig.current_arm_length`.
pub fn third_person_position(
    caster: &impl SphereCaster,
    feet: Vec3,
    rig: &mut CameraRigController,
    rotation: Quat,
    dt: f32,
) -> Vec3 {
    let mode = rig.current_mode;
    let head = feet + Vec3::Y * HEAD_HEIGHT;
    let pivot = clamp_pivot(
        caster,
        head,
        shoulder_pivot(feet, rig.yaw, mode, rig.tpp_shoulder_offset),
        rig.spherecast_radius,
    );
    let boom = rotation * Vec3::Z;
    let target = occluded_arm_length(
        caster,
        pivot,
        boom,
        desired_arm_length(mode, rig),
        rig.spherecast_radius,
        rig.min_arm_length,
    );
    rig.current_arm_length = smooth_arm_length(rig.current_arm_length, target, dt);
    pivot + boom * rig.current_arm_length
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reports a hit at a fixed distance for every sweep.
    struct WallAt(f32);

    impl SphereCaster for WallAt {
        fn cast_sphere(&self, _: Vec3, _: Dir3, max_distance: f32, _: f32) -> Option<f32> {
            (self.0 <= max_distance).then_some(self.0)
        }
    }

    fn rig() -> CameraRigController {
        CameraRigController::default()
    }

    #[test]
    fn shoulders_mirror_and_wide_is_centered() {
        let offset = Vec3::new(0.65, 1.55, 0.0);
        let right = shoulder_pivot(
            Vec3::ZERO,
            0.0,
            CameraPerspectiveMode::ThirdPersonRightShoulder,
            offset,
        );
        let left = shoulder_pivot(
            Vec3::ZERO,
            0.0,
            CameraPerspectiveMode::ThirdPersonLeftShoulder,
            offset,
        );
        assert_eq!(right, Vec3::new(0.65, 1.55, 0.0));
        assert_eq!(left, Vec3::new(-0.65, 1.55, 0.0));
        let wide = shoulder_pivot(
            Vec3::ONE,
            1.0,
            CameraPerspectiveMode::ThirdPersonWide,
            offset,
        );
        assert_eq!(wide, Vec3::new(1.0, 2.8, 1.0));

        // Facing +X (yaw -90 deg): the right shoulder is toward +Z.
        let turned = shoulder_pivot(
            Vec3::ZERO,
            -std::f32::consts::FRAC_PI_2,
            CameraPerspectiveMode::ThirdPersonRightShoulder,
            offset,
        );
        assert!(turned.abs_diff_eq(Vec3::new(0.0, 1.55, 0.65), 1e-5));
    }

    #[test]
    fn arm_lengths_per_mode() {
        let r = rig();
        assert_eq!(
            desired_arm_length(CameraPerspectiveMode::ThirdPersonRightShoulder, &r),
            2.8
        );
        assert_eq!(
            desired_arm_length(CameraPerspectiveMode::ThirdPersonWide, &r),
            4.5
        );
    }

    #[test]
    fn occlusion_clamps_with_clearance_and_minimum() {
        let r = rig();
        let clear = occluded_arm_length(
            &NoObstacles,
            Vec3::ZERO,
            Vec3::Z,
            2.8,
            0.22,
            r.min_arm_length,
        );
        assert_eq!(clear, 2.8);

        let wall = occluded_arm_length(
            &WallAt(1.5),
            Vec3::ZERO,
            Vec3::Z,
            2.8,
            0.22,
            r.min_arm_length,
        );
        assert!((wall - 1.35).abs() < 1e-6, "hit - 0.15: {wall}");

        let tight = occluded_arm_length(
            &WallAt(0.3),
            Vec3::ZERO,
            Vec3::Z,
            2.8,
            0.22,
            r.min_arm_length,
        );
        assert_eq!(tight, 0.4, "never shorter than min_arm_length");

        let beyond = occluded_arm_length(
            &WallAt(5.0),
            Vec3::ZERO,
            Vec3::Z,
            2.8,
            0.22,
            r.min_arm_length,
        );
        assert_eq!(beyond, 2.8, "hits past the boom are ignored");
    }

    #[test]
    fn arm_snaps_in_and_eases_out() {
        assert_eq!(
            smooth_arm_length(2.8, 1.0, 1.0 / 60.0),
            1.0,
            "retract instantly"
        );
        let mut length = 1.0;
        let mut previous = length;
        for _ in 0..30 {
            length = smooth_arm_length(length, 2.8, 1.0 / 60.0);
            assert!(length > previous && length <= 2.8);
            previous = length;
        }
        assert!((length - 2.8).abs() < 1e-3, "extended to {length}");
        assert_eq!(
            smooth_arm_length(1.0, 2.8, 1.0),
            2.8,
            "large dt does not overshoot"
        );
    }

    #[test]
    fn pivot_is_pulled_out_of_walls() {
        let head = Vec3::new(0.0, 1.55, 0.0);
        let pivot = Vec3::new(0.65, 1.55, 0.0);
        assert_eq!(clamp_pivot(&NoObstacles, head, pivot, 0.22), pivot);
        let pulled = clamp_pivot(&WallAt(0.4), head, pivot, 0.22);
        assert!((pulled.x - 0.25).abs() < 1e-6);
        let flush = clamp_pivot(&WallAt(0.05), head, pivot, 0.22);
        assert_eq!(flush, head);
    }

    #[test]
    fn camera_sits_behind_and_above_the_shoulder() {
        let mut r = rig();
        r.current_arm_length = r.tpp_arm_length;
        let rotation = Quat::from_euler(EulerRot::YXZ, 0.0, 0.0, 0.0);
        let position =
            third_person_position(&NoObstacles, Vec3::ZERO, &mut r, rotation, 1.0 / 60.0);
        assert!(
            position.abs_diff_eq(Vec3::new(0.65, 1.55, 2.8), 1e-5),
            "{position:?}"
        );

        // Looking down raises the camera over the shoulder.
        let down = Quat::from_euler(EulerRot::YXZ, 0.0, -0.5, 0.0);
        let raised = third_person_position(&NoObstacles, Vec3::ZERO, &mut r, down, 1.0 / 60.0);
        assert!(raised.y > 1.55);
    }
}
