//! Seven-probe spatial sensing for the parkour controller.
//!
//! Probes are measured from the character's feet (the capsule center is
//! [`CHARACTER_HALF_HEIGHT`] above them):
//!
//! 1. Ground: down from 0.2 m, 0.45 m long.
//! 2. / 3. Left / right walls: sideways from waist height (0.9 m), 0.85 m.
//! 4. / 5. / 6. Knee (0.45 m), chest (1.15 m) and eye (1.70 m) forward sweeps.
//! 7. Top sweep: down from 2.2 m just past the obstacle's front face to find
//!    its top, plus a return ray from beyond the obstacle to measure depth.
//!
//! The probe logic is written against [`ProbeWorld`] so it can run against
//! Avian's `SpatialQuery` in game and against simple box worlds in tests.

use crate::controller::ParkourInput;
use crate::kinematics::KinematicVelocity;
use avian3d::prelude::*;
use bevy::prelude::*;
use game_core::debug_draw::DebugShapecastQueue;
use game_core::math::safe_normalize;

pub const CHARACTER_HEIGHT: f32 = 1.8;
pub const CHARACTER_RADIUS: f32 = 0.4;
pub const CHARACTER_HALF_HEIGHT: f32 = CHARACTER_HEIGHT * 0.5;

pub const GROUND_PROBE_HEIGHT: f32 = 0.2;
pub const GROUND_PROBE_LENGTH: f32 = 0.45;
pub const WALL_PROBE_HEIGHT: f32 = 0.9;
pub const WALL_PROBE_LENGTH: f32 = 0.85;
pub const KNEE_PROBE_HEIGHT: f32 = 0.45;
pub const KNEE_PROBE_LENGTH: f32 = 1.0;
pub const CHEST_PROBE_HEIGHT: f32 = 1.15;
pub const CHEST_PROBE_LENGTH: f32 = 1.0;
pub const EYE_PROBE_HEIGHT: f32 = 1.70;
pub const EYE_PROBE_LENGTH: f32 = 1.1;
pub const TOP_SWEEP_HEIGHT: f32 = 2.2;
/// How far past the front face the top sweep starts.
pub const TOP_PROBE_INSET: f32 = 0.1;

pub const VAULT_MIN_HEIGHT: f32 = 0.3;
pub const VAULT_LOW_MAX_HEIGHT: f32 = 0.85;
pub const VAULT_MID_MAX_HEIGHT: f32 = 1.35;
pub const VAULT_HIGH_MAX_HEIGHT: f32 = 1.85;
/// Obstacles deeper than this are platforms: mantle, don't vault.
pub const MAX_VAULT_DEPTH: f32 = 1.5;
pub const MANTLE_MIN_HEIGHT: f32 = 1.0;
/// How far onto the ledge the mantle target is placed (capped at half depth).
pub const MANTLE_FORWARD_INSET: f32 = 0.5;

/// Surfaces whose normal is within 50 degrees of up are walkable.
pub const MIN_WALKABLE_NORMAL_Y: f32 = 0.642_787_6;
/// Wall-run surfaces must be within 15 degrees of vertical.
pub const MAX_WALL_NORMAL_Y: f32 = 0.258_819;
/// Ray hits closer than this are treated as starting inside a solid.
const INSIDE_EPSILON: f32 = 1e-3;

/// What the probes found this tick.
#[derive(Component, Debug, Clone, Default, Reflect, PartialEq)]
#[reflect(Component, Default, Debug)]
pub struct ParkourSensorReadings {
    pub is_ground_detected: bool,
    /// Distance from the ground probe origin (feet + 0.2 m) to the ground.
    pub ground_distance: f32,
    pub ground_normal: Vec3,
    pub is_left_wall_detected: bool,
    pub left_wall_normal: Vec3,
    pub is_right_wall_detected: bool,
    pub right_wall_normal: Vec3,
    pub can_vault_low: bool,
    pub can_vault_mid: bool,
    pub can_vault_high: bool,
    pub vault_depth: f32,
    pub can_ledge_mantle: bool,
    /// Capsule-center position standing on the ledge.
    pub mantle_target_position: Vec3,
    /// Planar direction the forward probes were cast in.
    pub forward: Vec3,
    /// Distance from the character's center axis to the obstacle's front face.
    pub obstacle_distance: f32,
    /// Obstacle top height above the feet.
    pub obstacle_top_height: f32,
}

/// One ray hit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProbeHit {
    pub distance: f32,
    /// Outward surface normal at the hit.
    pub normal: Vec3,
}

/// Something that can answer solid ray casts.
pub trait ProbeWorld {
    /// Casts a solid ray: an origin inside geometry hits at distance 0.
    fn ray(&self, origin: Vec3, direction: Dir3, max_distance: f32) -> Option<ProbeHit>;
}

/// [`ProbeWorld`] backed by Avian's spatial query pipeline.
pub struct SpatialProbeWorld<'a, 'w, 's> {
    pub query: &'a SpatialQuery<'w, 's>,
    pub filter: SpatialQueryFilter,
}

impl ProbeWorld for SpatialProbeWorld<'_, '_, '_> {
    fn ray(&self, origin: Vec3, direction: Dir3, max_distance: f32) -> Option<ProbeHit> {
        self.query
            .cast_ray(origin, direction, max_distance, true, &self.filter)
            .map(|hit| ProbeHit {
                distance: hit.distance,
                normal: hit.normal,
            })
    }
}

/// Runs all probes for a character whose feet are at `feet`, sweeping along
/// the planar `forward` direction. `record` receives every ray as
/// `(origin, direction, length, hit_distance)` for debug drawing.
pub fn sense_surroundings(
    world: &impl ProbeWorld,
    feet: Vec3,
    forward: Vec3,
    mut record: impl FnMut(Vec3, Vec3, f32, Option<f32>),
) -> ParkourSensorReadings {
    let forward = Vec3::new(forward.x, 0.0, forward.z)
        .try_normalize()
        .unwrap_or(Vec3::NEG_Z);
    let right = forward.cross(Vec3::Y);
    let up = Vec3::Y;

    let mut cast = |origin: Vec3, direction: Vec3, length: f32| -> Option<ProbeHit> {
        let dir = Dir3::new(direction).ok()?;
        let hit = world.ray(origin, dir, length);
        record(origin, direction, length, hit.map(|h| h.distance));
        hit
    };

    let mut readings = ParkourSensorReadings {
        ground_distance: GROUND_PROBE_LENGTH,
        ground_normal: Vec3::Y,
        forward,
        ..Default::default()
    };

    // 1. Ground.
    if let Some(hit) = cast(
        feet + up * GROUND_PROBE_HEIGHT,
        Vec3::NEG_Y,
        GROUND_PROBE_LENGTH,
    ) {
        let normal = safe_normalize(hit.normal, Vec3::Y);
        if normal.y >= MIN_WALKABLE_NORMAL_Y {
            readings.is_ground_detected = true;
            readings.ground_distance = hit.distance;
            readings.ground_normal = normal;
        }
    }

    // 2. / 3. Lateral walls.
    let waist = feet + up * WALL_PROBE_HEIGHT;
    let is_wall = |hit: &ProbeHit| hit.normal.y.abs() <= MAX_WALL_NORMAL_Y && hit.distance > 0.0;
    if let Some(hit) = cast(waist, -right, WALL_PROBE_LENGTH).filter(is_wall) {
        readings.is_left_wall_detected = true;
        readings.left_wall_normal = safe_normalize(hit.normal, right);
    }
    if let Some(hit) = cast(waist, right, WALL_PROBE_LENGTH).filter(is_wall) {
        readings.is_right_wall_detected = true;
        readings.right_wall_normal = safe_normalize(hit.normal, -right);
    }

    // 4. / 5. / 6. Forward sweeps. Only faces turned toward us count.
    let facing_us = |hit: &ProbeHit| hit.normal.dot(forward) <= -0.5;
    let knee = cast(feet + up * KNEE_PROBE_HEIGHT, forward, KNEE_PROBE_LENGTH).filter(facing_us);
    let chest = cast(feet + up * CHEST_PROBE_HEIGHT, forward, CHEST_PROBE_LENGTH).filter(facing_us);
    let eye = cast(feet + up * EYE_PROBE_HEIGHT, forward, EYE_PROBE_LENGTH);

    let front_distance = match (knee, chest) {
        (Some(k), Some(c)) => k.distance.min(c.distance),
        (Some(k), None) => k.distance,
        (None, Some(c)) => c.distance,
        (None, None) => return readings,
    };

    // 7. Top sweep just past the front face.
    let probe_x = front_distance + TOP_PROBE_INSET;
    let top_origin = feet + up * TOP_SWEEP_HEIGHT + forward * probe_x;
    let Some(top_hit) = cast(top_origin, Vec3::NEG_Y, TOP_SWEEP_HEIGHT) else {
        return readings;
    };
    if top_hit.distance <= INSIDE_EPSILON || top_hit.normal.y < MIN_WALKABLE_NORMAL_Y {
        // Taller than the sweep, or no standable top.
        return readings;
    }
    let top_height = TOP_SWEEP_HEIGHT - top_hit.distance;

    // Depth: cast back toward the character from beyond the deepest
    // vaultable obstacle, just below the top surface.
    let back_reach = MAX_VAULT_DEPTH + TOP_PROBE_INSET * 2.0;
    let back_origin =
        feet + up * (top_height - 0.05).max(0.01) + forward * (front_distance + back_reach);
    let depth = match cast(back_origin, -forward, back_reach) {
        Some(hit) if hit.distance > INSIDE_EPSILON => back_reach - hit.distance,
        _ => f32::INFINITY,
    };

    // Something at eye level closer than the obstacle's own face only
    // matters when the obstacle's top is below the eye probe.
    let head_clear = eye.is_none() || top_height >= EYE_PROBE_HEIGHT - 0.05;

    readings.obstacle_distance = front_distance;
    readings.obstacle_top_height = top_height;
    readings.vault_depth = depth;

    if head_clear && depth <= MAX_VAULT_DEPTH && top_height >= VAULT_MIN_HEIGHT {
        readings.can_vault_low = top_height <= VAULT_LOW_MAX_HEIGHT;
        readings.can_vault_mid = !readings.can_vault_low && top_height <= VAULT_MID_MAX_HEIGHT;
        readings.can_vault_high = !readings.can_vault_low
            && !readings.can_vault_mid
            && top_height <= VAULT_HIGH_MAX_HEIGHT;
    }

    let vaultable = readings.can_vault_low || readings.can_vault_mid || readings.can_vault_high;
    if chest.is_some() && head_clear && !vaultable && top_height >= MANTLE_MIN_HEIGHT {
        let inset = if depth.is_finite() {
            MANTLE_FORWARD_INSET.min(depth * 0.5)
        } else {
            MANTLE_FORWARD_INSET
        };
        readings.can_ledge_mantle = true;
        readings.mantle_target_position =
            feet + forward * (front_distance + inset) + up * (top_height + CHARACTER_HALF_HEIGHT);
    }

    readings
}

/// Planar probing direction: the direction of travel when moving, otherwise
/// the facing direction from the input yaw.
pub fn probe_forward(velocity: Vec3, input: &ParkourInput) -> Vec3 {
    let planar = Vec3::new(velocity.x, 0.0, velocity.z);
    if planar.length_squared() > 1.0 {
        planar.normalize()
    } else {
        input.facing_forward()
    }
}

/// Runs the probe array for every parkour character.
pub fn execute_parkour_spatial_probes(
    spatial: SpatialQuery,
    mut debug_queue: Option<ResMut<DebugShapecastQueue>>,
    mut characters: Query<(
        Entity,
        &Transform,
        &KinematicVelocity,
        &ParkourInput,
        &mut ParkourSensorReadings,
    )>,
) {
    for (entity, transform, velocity, input, mut readings) in &mut characters {
        let world = SpatialProbeWorld {
            query: &spatial,
            filter: SpatialQueryFilter::default().with_excluded_entities([entity]),
        };
        let feet = transform.translation - Vec3::Y * CHARACTER_HALF_HEIGHT;
        let forward = probe_forward(velocity.0, input);
        let queue = debug_queue.as_deref_mut().filter(|q| q.is_recording());
        let new_readings = match queue {
            Some(queue) => sense_surroundings(&world, feet, forward, |o, d, l, h| {
                queue.push(o, d, l, 0.0, h);
            }),
            None => sense_surroundings(&world, feet, forward, |_, _, _, _| {}),
        };
        *readings = new_readings;
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Axis-aligned boxes for exercising probe logic without a physics world.
    pub(crate) struct BoxWorld(pub Vec<(Vec3, Vec3)>);

    impl BoxWorld {
        /// Box from center and full size.
        pub(crate) fn with(mut self, center: Vec3, size: Vec3) -> Self {
            self.0.push((center - size * 0.5, center + size * 0.5));
            self
        }

        pub(crate) fn ground() -> Self {
            Self(Vec::new()).with(Vec3::new(0.0, -0.1, 0.0), Vec3::new(200.0, 0.2, 200.0))
        }
    }

    fn ray_box(origin: Vec3, dir: Vec3, max: f32, min_b: Vec3, max_b: Vec3) -> Option<ProbeHit> {
        let inside = (0..3).all(|a| origin[a] > min_b[a] && origin[a] < max_b[a]);
        if inside {
            return Some(ProbeHit {
                distance: 0.0,
                normal: -dir,
            });
        }
        let mut t_enter = 0.0_f32;
        let mut t_exit = max;
        let mut normal = Vec3::ZERO;
        for axis in 0..3 {
            if dir[axis].abs() < 1e-8 {
                if origin[axis] < min_b[axis] || origin[axis] > max_b[axis] {
                    return None;
                }
                continue;
            }
            let inv = 1.0 / dir[axis];
            let mut t_near = (min_b[axis] - origin[axis]) * inv;
            let mut t_far = (max_b[axis] - origin[axis]) * inv;
            let mut sign = -1.0;
            if t_near > t_far {
                std::mem::swap(&mut t_near, &mut t_far);
                sign = 1.0;
            }
            if t_near > t_enter {
                t_enter = t_near;
                normal = Vec3::ZERO;
                normal[axis] = sign;
            }
            t_exit = t_exit.min(t_far);
            if t_enter > t_exit {
                return None;
            }
        }
        Some(ProbeHit {
            distance: t_enter,
            normal,
        })
    }

    impl ProbeWorld for BoxWorld {
        fn ray(&self, origin: Vec3, direction: Dir3, max_distance: f32) -> Option<ProbeHit> {
            self.0
                .iter()
                .filter_map(|(min, max)| ray_box(origin, *direction, max_distance, *min, *max))
                .min_by(|a, b| a.distance.total_cmp(&b.distance))
        }
    }

    fn sense(world: &BoxWorld, feet: Vec3) -> ParkourSensorReadings {
        sense_surroundings(world, feet, Vec3::NEG_Z, |_, _, _, _| {})
    }

    /// A box sitting on the ground whose front face is `gap` meters in front
    /// of the character's axis (character at the origin facing -Z).
    fn obstacle(height: f32, depth: f32, gap: f32) -> BoxWorld {
        BoxWorld::ground().with(
            Vec3::new(0.0, height * 0.5, -(gap + depth * 0.5)),
            Vec3::new(4.0, height, depth),
        )
    }

    #[test]
    fn ground_probe_on_flat_ground() {
        let r = sense(&BoxWorld::ground(), Vec3::new(0.0, 0.03, 0.0));
        assert!(r.is_ground_detected);
        assert!((r.ground_distance - 0.23).abs() < 1e-4);
        assert_eq!(r.ground_normal, Vec3::Y);
        assert!(!r.is_left_wall_detected && !r.is_right_wall_detected);
        assert!(!r.can_vault_low && !r.can_ledge_mantle);
    }

    #[test]
    fn ground_probe_misses_when_airborne() {
        let r = sense(&BoxWorld::ground(), Vec3::new(0.0, 0.5, 0.0));
        assert!(!r.is_ground_detected);
    }

    #[test]
    fn walls_detected_on_each_side_with_inward_normals() {
        let world = BoxWorld::ground()
            .with(Vec3::new(-1.0, 3.0, 0.0), Vec3::new(0.5, 6.0, 20.0))
            .with(Vec3::new(1.0, 3.0, 0.0), Vec3::new(0.5, 6.0, 20.0));
        let r = sense(&world, Vec3::ZERO);
        assert!(r.is_left_wall_detected && r.is_right_wall_detected);
        assert_eq!(r.left_wall_normal, Vec3::X);
        assert_eq!(r.right_wall_normal, Vec3::NEG_X);
    }

    #[test]
    fn wall_out_of_reach_is_not_detected() {
        let world = BoxWorld::ground().with(Vec3::new(1.5, 3.0, 0.0), Vec3::new(0.5, 6.0, 20.0));
        let r = sense(&world, Vec3::ZERO);
        assert!(!r.is_right_wall_detected);
    }

    #[test]
    fn vault_heights_classify_low_mid_high() {
        for (height, low, mid, high) in [
            (0.75, true, false, false),
            (1.25, false, true, false),
            (1.80, false, false, true),
        ] {
            let r = sense(&obstacle(height, 0.6, 0.6), Vec3::ZERO);
            assert_eq!(
                (r.can_vault_low, r.can_vault_mid, r.can_vault_high),
                (low, mid, high),
                "height {height}"
            );
            assert!((r.obstacle_top_height - height).abs() < 1e-4);
            assert!(
                (r.vault_depth - 0.6).abs() < 1e-4,
                "depth {}",
                r.vault_depth
            );
            assert!((r.obstacle_distance - 0.6).abs() < 1e-4);
            assert!(!r.can_ledge_mantle);
        }
    }

    #[test]
    fn too_short_or_out_of_range_is_not_a_vault() {
        let curb = sense(&obstacle(0.2, 0.6, 0.6), Vec3::ZERO);
        assert!(!curb.can_vault_low, "below knee probe");
        let far = sense(&obstacle(0.75, 0.6, 1.5), Vec3::ZERO);
        assert!(!far.can_vault_low, "beyond forward probe reach");
    }

    #[test]
    fn deep_ledge_is_a_mantle_not_a_vault() {
        let r = sense(&obstacle(1.6, 4.0, 0.6), Vec3::ZERO);
        assert!(!r.can_vault_low && !r.can_vault_mid && !r.can_vault_high);
        assert!(r.vault_depth > MAX_VAULT_DEPTH);
        assert!(r.can_ledge_mantle);
        let target = r.mantle_target_position;
        assert!((target.y - (1.6 + CHARACTER_HALF_HEIGHT)).abs() < 1e-4);
        assert!((target.z - -(0.6 + MANTLE_FORWARD_INSET)).abs() < 1e-4);
    }

    #[test]
    fn ledge_above_eye_level_can_be_mantled() {
        let r = sense(&obstacle(2.1, 4.0, 0.6), Vec3::ZERO);
        assert!(
            r.can_ledge_mantle,
            "eye ray hitting the ledge face is expected"
        );
    }

    #[test]
    fn tall_wall_offers_nothing() {
        let r = sense(&obstacle(6.0, 0.5, 0.6), Vec3::ZERO);
        assert!(!r.can_vault_low && !r.can_vault_mid && !r.can_vault_high);
        assert!(!r.can_ledge_mantle);
    }

    #[test]
    fn wall_behind_low_box_blocks_vault() {
        // Box spans z -0.6..-1.0; the wall spans -1.05..-1.35, inside the
        // 1.1 m eye probe.
        let world =
            obstacle(0.75, 0.4, 0.6).with(Vec3::new(0.0, 3.0, -1.2), Vec3::new(4.0, 6.0, 0.3));
        let r = sense(&world, Vec3::ZERO);
        assert!(!r.can_vault_low, "eye probe hits the wall behind the box");
    }

    #[test]
    fn probes_report_every_ray_to_the_recorder() {
        let mut rays = Vec::new();
        sense_surroundings(
            &obstacle(0.75, 0.6, 0.6),
            Vec3::ZERO,
            Vec3::NEG_Z,
            |o, d, l, h| {
                rays.push((o, d, l, h));
            },
        );
        // ground, left, right, knee, chest, eye, top sweep, depth return.
        assert_eq!(rays.len(), 8);
        assert!(rays[0].3.is_some(), "ground contact");
        assert!(rays[1].3.is_none() && rays[2].3.is_none(), "no walls");
        assert!(rays[3].3.is_some(), "knee contact");
        assert!(rays[4].3.is_none(), "chest clear above a low vault");
    }

    #[test]
    fn no_obstacle_skips_top_sweep() {
        let mut count = 0;
        sense_surroundings(
            &BoxWorld::ground(),
            Vec3::ZERO,
            Vec3::NEG_Z,
            |_, _, _, _| count += 1,
        );
        assert_eq!(count, 6);
    }

    #[test]
    fn steep_surface_is_not_ground() {
        struct Steep;
        impl ProbeWorld for Steep {
            fn ray(&self, _: Vec3, _: Dir3, _: f32) -> Option<ProbeHit> {
                Some(ProbeHit {
                    distance: 0.2,
                    normal: Vec3::new(0.9, 0.43, 0.0).normalize(),
                })
            }
        }
        assert!(
            !sense_surroundings(&Steep, Vec3::ZERO, Vec3::NEG_Z, |_, _, _, _| {})
                .is_ground_detected
        );
    }

    #[test]
    fn forward_follows_motion_or_facing() {
        let input = ParkourInput {
            yaw: std::f32::consts::FRAC_PI_2,
            ..Default::default()
        };
        assert!(probe_forward(Vec3::new(0.0, 5.0, 0.0), &input).abs_diff_eq(Vec3::NEG_X, 1e-5));
        assert!(probe_forward(Vec3::new(3.0, 0.0, 0.0), &input).abs_diff_eq(Vec3::X, 1e-5));
    }
}
