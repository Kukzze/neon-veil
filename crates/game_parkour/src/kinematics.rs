//! Kinematic locomotion math: ground movement, sliding, jumping and gravity.
//!
//! All integrators take `dt` in seconds, treat non-finite or negative `dt` as
//! zero, and leave the velocity finite.

use crate::state_machine::ParkourState;
use bevy::prelude::*;
use game_core::math::{safe_normalize, sanitize_f32, sanitize_vec3};

/// Slopes steeper than this accelerate a slide; shallower ones damp it.
pub const SLIDE_SLOPE_THRESHOLD_DEGREES: f32 = 5.0;

/// Current character velocity in m/s.
#[derive(Component, Reflect, Debug, Default, Clone, Copy, PartialEq)]
#[reflect(Component, Default, Debug, PartialEq)]
pub struct KinematicVelocity(pub Vec3);

impl KinematicVelocity {
    pub fn new(velocity: Vec3) -> Self {
        Self(sanitize_vec3(velocity))
    }

    /// Replaces the velocity, zeroing any non-finite component.
    pub fn set(&mut self, velocity: Vec3) {
        self.0 = sanitize_vec3(velocity);
    }

    /// Horizontal (XZ) part of the velocity.
    pub fn planar(&self) -> Vec3 {
        Vec3::new(self.0.x, 0.0, self.0.z)
    }

    pub fn planar_speed(&self) -> f32 {
        self.planar().length()
    }
}

/// Movement tuning shared by client prediction and the server.
#[derive(Resource, Reflect, Debug, Clone, PartialEq)]
#[reflect(Resource, Default, Debug, PartialEq)]
pub struct ParkourPhysicsConstants {
    /// m/s
    pub walk_speed: f32,
    /// m/s
    pub sprint_speed: f32,
    /// m/s
    pub max_flow_speed: f32,
    /// Slides end below this speed, m/s.
    pub slide_min_speed: f32,
    /// Terminal slide speed, m/s.
    pub slide_max_speed: f32,
    /// Exponential damping rate on flat ground, 1/s.
    pub slide_flat_decay: f32,
    /// Kinetic friction coefficient on slopes (dimensionless).
    pub slide_kinetic_friction: f32,
    /// m/s
    pub wall_run_min_speed: f32,
    /// s
    pub wall_run_max_duration: f32,
    /// m/s
    pub wall_run_sink_speed: f32,
    pub wall_kick_normal_impulse: f32,
    pub wall_kick_forward_impulse: f32,
    pub wall_kick_up_impulse: f32,
    /// Upward velocity set by a jump, m/s.
    pub jump_vertical_impulse: f32,
    /// m/s^2
    pub gravity_acceleration: f32,
    /// s
    pub coyote_time_duration: f32,
    /// s
    pub jump_buffer_duration: f32,
    /// m/s
    pub mantle_snap_speed: f32,
    /// Rate at which planar ground velocity approaches the input target, m/s^2.
    pub ground_acceleration: f32,
    /// Time constant of the wall-run sink ramp `-v_sink (1 - e^(-t/tau))`, s.
    pub wall_run_sink_time_constant: f32,
    /// Scale of the centrifugal stick acceleration on curved walls.
    pub wall_run_adhesion: f32,
    /// Landing faster than this (downward, m/s) without a roll is a hard landing.
    pub hard_landing_speed: f32,
    /// Momentum lock after a hard landing, s.
    pub hard_landing_lock_duration: f32,
    /// Duration of a landing roll, s.
    pub landing_roll_duration: f32,
    /// Time spent in `WallKick` before normal airborne control resumes, s.
    pub wall_kick_duration: f32,
    /// Stamina spent on a wall kick (waived in Flow Overdrive).
    pub wall_kick_stamina_cost: f32,
    /// After leaving a wall, wall runs are blocked for this long, s.
    pub wall_reattach_cooldown: f32,
    /// Extra height the feet clear above a vault obstacle, m.
    pub vault_clearance: f32,
    /// Speed multiplier at Flow tier 3 (Overdrive).
    pub flow_overdrive_multiplier: f32,
}

impl Default for ParkourPhysicsConstants {
    fn default() -> Self {
        Self {
            walk_speed: 5.2,
            sprint_speed: 12.8,
            max_flow_speed: 17.5,
            slide_min_speed: 7.0,
            slide_max_speed: 25.0,
            slide_flat_decay: 1.45,
            slide_kinetic_friction: 0.035,
            wall_run_min_speed: 8.5,
            wall_run_max_duration: 2.2,
            wall_run_sink_speed: 1.25,
            wall_kick_normal_impulse: 9.2,
            wall_kick_forward_impulse: 11.5,
            wall_kick_up_impulse: 7.8,
            jump_vertical_impulse: 9.4,
            gravity_acceleration: 24.5,
            coyote_time_duration: 0.15,
            jump_buffer_duration: 0.12,
            mantle_snap_speed: 14.0,
            ground_acceleration: 80.0,
            wall_run_sink_time_constant: 0.35,
            wall_run_adhesion: 1.0,
            hard_landing_speed: 14.0,
            hard_landing_lock_duration: 0.35,
            landing_roll_duration: 0.45,
            wall_kick_duration: 0.15,
            wall_kick_stamina_cost: 15.0,
            wall_reattach_cooldown: 0.3,
            vault_clearance: 0.15,
            flow_overdrive_multiplier: 1.25,
        }
    }
}

#[inline]
fn sanitize_dt(dt: f32) -> f32 {
    sanitize_f32(dt).max(0.0)
}

/// Removes the component of `velocity` along `normal`, leaving motion in the
/// surface's tangent plane. A degenerate normal leaves `velocity` unchanged.
pub fn project_velocity_on_surface(velocity: Vec3, normal: Vec3) -> Vec3 {
    let velocity = sanitize_vec3(velocity);
    match normal.try_normalize() {
        Some(n) => sanitize_vec3(velocity - n * velocity.dot(n)),
        None => velocity,
    }
}

/// Slope angle of a surface with the given normal, in radians (`0` = flat).
/// A degenerate normal is treated as flat.
pub fn slope_angle_radians(normal: Vec3) -> f32 {
    safe_normalize(normal, Vec3::Y)
        .dot(Vec3::Y)
        .clamp(-1.0, 1.0)
        .acos()
}

/// Downslope unit direction and net acceleration `g(sin θ - μ cos θ)` for a
/// body sliding downhill on a surface with normal `normal`.
///
/// The direction is zero on flat ground. The acceleration is negative when
/// friction outweighs the slope.
pub fn compute_slope_slide_acceleration(normal: Vec3, gravity: f32, friction: f32) -> (Vec3, f32) {
    let n = safe_normalize(normal, Vec3::Y);
    let down = Vec3::NEG_Y;
    let downslope = (down - n * down.dot(n)).normalize_or_zero();
    let cos_theta = n.dot(Vec3::Y).clamp(-1.0, 1.0);
    let sin_theta = (1.0 - cos_theta * cos_theta).max(0.0).sqrt();
    let acceleration = sanitize_f32(gravity) * (sin_theta - sanitize_f32(friction) * cos_theta);
    (downslope, sanitize_f32(acceleration))
}

/// Whether a character in `state` moving at `velocity` may start a slide.
pub fn can_begin_slide(
    state: ParkourState,
    velocity: Vec3,
    constants: &ParkourPhysicsConstants,
) -> bool {
    let planar = Vec3::new(velocity.x, 0.0, velocity.z);
    state.can_slide() && planar.length() >= constants.slide_min_speed
}

/// Whether a slide at `velocity` has lost too much speed to continue.
pub fn slide_should_end(velocity: Vec3, constants: &ParkourPhysicsConstants) -> bool {
    sanitize_vec3(velocity).length() < constants.slide_min_speed
}

/// Advances a slide by `dt` on a surface with normal `normal`.
///
/// On slopes steeper than [`SLIDE_SLOPE_THRESHOLD_DEGREES`], gravity's
/// along-slope component accelerates the body downhill while kinetic friction
/// opposes its motion. Moving downhill this is exactly
/// `g(sin θ - μ cos θ)`; moving uphill both terms decelerate. Speed is capped at
/// `slide_max_speed`. On flatter ground velocity decays as `v * e^(-λ dt)`.
pub fn integrate_slide(
    velocity: &mut Vec3,
    normal: Vec3,
    dt: f32,
    constants: &ParkourPhysicsConstants,
) {
    let dt = sanitize_dt(dt);
    let n = safe_normalize(normal, Vec3::Y);
    let mut v = project_velocity_on_surface(*velocity, n);

    if slope_angle_radians(n) > SLIDE_SLOPE_THRESHOLD_DEGREES.to_radians() {
        let g = sanitize_f32(constants.gravity_acceleration);
        let mu = sanitize_f32(constants.slide_kinetic_friction).max(0.0);
        let cos_theta = n.dot(Vec3::Y).clamp(-1.0, 1.0);
        let sin_theta = (1.0 - cos_theta * cos_theta).max(0.0).sqrt();
        let (downslope, _) = compute_slope_slide_acceleration(n, g, mu);

        v += downslope * (g * sin_theta * dt);

        let speed = v.length();
        let friction_loss = g * mu * cos_theta.abs() * dt;
        v = if speed > friction_loss {
            v * ((speed - friction_loss) / speed)
        } else {
            Vec3::ZERO
        };
        v = v.clamp_length_max(sanitize_f32(constants.slide_max_speed).max(0.0));
    } else {
        let decay = sanitize_f32(constants.slide_flat_decay).max(0.0);
        v *= (-decay * dt).exp();
    }

    *velocity = sanitize_vec3(v);
}

/// Moves planar velocity toward the input target at `ground_acceleration`,
/// preserving vertical velocity.
///
/// `input_dir` is the world-space XZ movement intent (`x` -> world X,
/// `y` -> world Z), already rotated by camera yaw; its length is clamped to 1.
pub fn integrate_ground_locomotion(
    velocity: &mut Vec3,
    input_dir: Vec2,
    is_sprinting: bool,
    dt: f32,
    constants: &ParkourPhysicsConstants,
) {
    let dt = sanitize_dt(dt);
    let input =
        Vec2::new(sanitize_f32(input_dir.x), sanitize_f32(input_dir.y)).clamp_length_max(1.0);
    let target_speed = if is_sprinting {
        constants.sprint_speed
    } else {
        constants.walk_speed
    };
    let target = Vec3::new(input.x, 0.0, input.y) * sanitize_f32(target_speed).max(0.0);

    let v = sanitize_vec3(*velocity);
    let planar = Vec3::new(v.x, 0.0, v.z);
    let max_delta = sanitize_f32(constants.ground_acceleration).max(0.0) * dt;
    let to_target = target - planar;
    let distance = to_target.length();
    let new_planar = if distance <= max_delta || distance <= f32::EPSILON {
        target
    } else {
        planar + to_target * (max_delta / distance)
    };

    *velocity = sanitize_vec3(Vec3::new(new_planar.x, v.y, new_planar.z));
}

/// Starts a jump: sets vertical velocity to `jump_vertical_impulse` and keeps
/// planar momentum.
pub fn integrate_jump(velocity: &mut Vec3, constants: &ParkourPhysicsConstants) {
    let mut v = sanitize_vec3(*velocity);
    v.y = sanitize_f32(constants.jump_vertical_impulse);
    *velocity = v;
}

/// Applies downward gravity for `dt`.
pub fn integrate_gravity(velocity: &mut Vec3, dt: f32, constants: &ParkourPhysicsConstants) {
    let mut v = sanitize_vec3(*velocity);
    v.y -= sanitize_f32(constants.gravity_acceleration) * sanitize_dt(dt);
    *velocity = sanitize_vec3(v);
}

/// Repairs any non-finite velocity before it reaches transforms or the
/// network. Only writes when a repair is needed, so change detection stays
/// meaningful.
pub fn sanitize_kinematic_velocities(mut query: Query<&mut KinematicVelocity>) {
    for mut velocity in &mut query {
        if !velocity.0.is_finite() {
            warn!("non-finite KinematicVelocity {:?} sanitized", velocity.0);
            let repaired = sanitize_vec3(velocity.0);
            velocity.0 = repaired;
        }
    }
}

// --- Acrobatics ----------------------------------------------------------------

/// Small radius offset that keeps the stick term finite on sharp corners.
pub const WALL_CURVATURE_EPSILON: f32 = 0.01;
/// Upper bound on the centrifugal stick acceleration, m/s^2.
pub const MAX_WALL_STICK_ACCELERATION: f32 = 60.0;
/// Squared distance at which a mantle snaps onto its target (0.2 m).
pub const MANTLE_ARRIVAL_DISTANCE_SQUARED: f32 = 0.04;
/// Vertical speed band treated as the jump apex, m/s.
pub const JUMP_APEX_SPEED_BAND: f32 = 1.0;

#[inline]
fn planar(v: Vec3) -> Vec3 {
    Vec3::new(v.x, 0.0, v.z)
}

/// Unit direction along the wall in the direction of planar travel:
/// `normalize(v_planar - (v_planar . n) n)`. Zero when moving straight into
/// or away from the wall.
pub fn wall_run_tangent(planar_velocity: Vec3, wall_normal: Vec3) -> Vec3 {
    let n = planar(safe_normalize(wall_normal, Vec3::ZERO)).normalize_or_zero();
    let v = planar(sanitize_vec3(planar_velocity));
    (v - n * v.dot(n)).normalize_or_zero()
}

/// Planar wall-run velocity: the tangent direction at
/// `max(entry_speed, min_speed) * flow_multiplier`, capped at
/// `max(max_speed, max(entry_speed, min_speed))` so the multiplier cannot
/// compound over successive ticks.
pub fn wall_run_velocity(
    planar_velocity: Vec3,
    wall_normal: Vec3,
    entry_speed: f32,
    flow_multiplier: f32,
    constants: &ParkourPhysicsConstants,
) -> Vec3 {
    let tangent = wall_run_tangent(planar_velocity, wall_normal);
    let base = sanitize_f32(entry_speed).max(constants.wall_run_min_speed);
    let multiplier = sanitize_f32(flow_multiplier).max(0.0);
    let speed = (base * multiplier).min(constants.max_flow_speed.max(base));
    tangent * speed
}

/// Curvature radius of a **convex** wall (one that curves away from the
/// runner, like a pillar), estimated from how far the wall normal turned
/// while travelling `distance_travelled` along `travel_direction`.
///
/// The normal of a convex surface rotates toward the direction of travel;
/// on a concave surface it rotates against it, and the wall's own collision
/// response keeps the runner attached, so concave and flat walls return
/// `f32::INFINITY` (no stick force).
pub fn estimate_wall_radius(
    previous_normal: Vec3,
    normal: Vec3,
    travel_direction: Vec3,
    distance_travelled: f32,
) -> f32 {
    let (Some(a), Some(b)) = (previous_normal.try_normalize(), normal.try_normalize()) else {
        return f32::INFINITY;
    };
    let distance = sanitize_f32(distance_travelled).max(0.0);
    let angle = a.angle_between(b);
    let turning_toward_travel = (b - a).dot(sanitize_vec3(travel_direction)) > 0.0;
    if !angle.is_finite() || angle < 1e-4 || distance <= 0.0 || !turning_toward_travel {
        f32::INFINITY
    } else {
        distance / angle
    }
}

/// Centrifugal stick acceleration `-n * v^2 / (R + eps) * k` that keeps a
/// wall runner on convex walls. Zero for flat walls (`R = inf`).
pub fn wall_stick_acceleration(wall_normal: Vec3, speed: f32, radius: f32, adhesion: f32) -> Vec3 {
    let n = safe_normalize(wall_normal, Vec3::ZERO);
    let radius = if radius.is_nan() {
        f32::INFINITY
    } else {
        radius.max(0.0)
    };
    if radius.is_infinite() {
        return Vec3::ZERO;
    }
    let speed = sanitize_f32(speed);
    let magnitude = (speed * speed / (radius + WALL_CURVATURE_EPSILON) * sanitize_f32(adhesion))
        .clamp(0.0, MAX_WALL_STICK_ACCELERATION);
    -n * magnitude
}

/// Vertical wall-run velocity after `elapsed` seconds on the wall:
/// `-v_sink (1 - e^(-t / tau))`, replacing gravity while attached.
pub fn wall_run_sink_velocity(elapsed: f32, constants: &ParkourPhysicsConstants) -> f32 {
    let t = sanitize_f32(elapsed).max(0.0);
    let tau = sanitize_f32(constants.wall_run_sink_time_constant).max(1e-3);
    -constants.wall_run_sink_speed * (1.0 - (-t / tau).exp())
}

/// Composite wall-kick velocity `a n + b f + g up`.
///
/// `camera_forward` is flattened to the horizontal plane. If it points into
/// the wall, its into-wall component is removed so the kick never drives the
/// character back into the surface.
pub fn wall_kick_impulse(
    wall_normal: Vec3,
    camera_forward: Vec3,
    constants: &ParkourPhysicsConstants,
) -> Vec3 {
    let n = planar(safe_normalize(wall_normal, Vec3::ZERO)).normalize_or_zero();
    let mut forward = planar(sanitize_vec3(camera_forward)).normalize_or_zero();
    let into_wall = forward.dot(n);
    if into_wall < 0.0 {
        forward = (forward - n * into_wall).normalize_or_zero();
    }
    sanitize_vec3(
        n * constants.wall_kick_normal_impulse
            + forward * constants.wall_kick_forward_impulse
            + Vec3::Y * constants.wall_kick_up_impulse,
    )
}

/// Moves `current` toward `target` at `speed` m/s. Returns the new position
/// and whether the mantle has arrived (within 0.2 m, then snapped exactly).
pub fn mantle_step(current: Vec3, target: Vec3, speed: f32, dt: f32) -> (Vec3, bool) {
    let current = sanitize_vec3(current);
    let target = sanitize_vec3(target);
    let step = sanitize_f32(speed).max(0.0) * sanitize_f32(dt).max(0.0);
    let offset = target - current;
    let distance = offset.length();
    let next = if distance <= step || distance <= f32::EPSILON {
        target
    } else {
        current + offset * (step / distance)
    };
    if next.distance_squared(target) < MANTLE_ARRIVAL_DISTANCE_SQUARED {
        (target, true)
    } else {
        (next, false)
    }
}

/// Height of the vault arc above the take-off height at `progress` in
/// `[0, 1]`: a half-sine peaking at `peak_height`.
pub fn vault_height_offset(progress: f32, peak_height: f32) -> f32 {
    let p = sanitize_f32(progress).clamp(0.0, 1.0);
    (std::f32::consts::PI * p).sin() * sanitize_f32(peak_height).max(0.0)
}

/// How a character touches down.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LandingOutcome {
    Normal,
    /// Fast landing without a roll: brief momentum lock.
    Hard,
    /// Slide was buffered before touchdown: keep momentum, gain Flow.
    Roll,
}

/// Classifies a landing from the downward velocity at touchdown.
pub fn classify_landing(
    vertical_velocity: f32,
    slide_buffered: bool,
    constants: &ParkourPhysicsConstants,
) -> LandingOutcome {
    if slide_buffered {
        LandingOutcome::Roll
    } else if sanitize_f32(vertical_velocity) < -constants.hard_landing_speed {
        LandingOutcome::Hard
    } else {
        LandingOutcome::Normal
    }
}

/// Airborne sub-state from vertical velocity.
pub fn airborne_phase(vertical_velocity: f32) -> ParkourState {
    let vy = sanitize_f32(vertical_velocity);
    if vy > JUMP_APEX_SPEED_BAND {
        ParkourState::JumpAscent
    } else if vy < -JUMP_APEX_SPEED_BAND {
        ParkourState::JumpDescent
    } else {
        ParkourState::JumpApex
    }
}

#[cfg(test)]
mod acrobatics_tests {
    use super::*;
    use game_core::time::FIXED_TIMESTEP_SECS as DT;

    const EPS: f32 = 1e-4;

    fn c() -> ParkourPhysicsConstants {
        ParkourPhysicsConstants::default()
    }

    #[test]
    fn tangent_projects_out_wall_normal() {
        // Running mostly forward (-Z) with a left wall whose normal is +X.
        let t = wall_run_tangent(Vec3::new(-3.0, 5.0, -12.0), Vec3::X);
        assert!(t.abs_diff_eq(Vec3::NEG_Z, EPS), "{t:?}");
        // Diagonal approach keeps only the along-wall component.
        let t = wall_run_tangent(Vec3::new(-4.0, 0.0, -4.0), Vec3::X);
        assert!(t.abs_diff_eq(Vec3::NEG_Z, EPS));
        // Head-on: no tangent.
        assert_eq!(
            wall_run_tangent(Vec3::new(-10.0, 0.0, 0.0), Vec3::X),
            Vec3::ZERO
        );
        // A slightly tilted normal is flattened first.
        let t = wall_run_tangent(Vec3::new(0.0, 0.0, -10.0), Vec3::new(1.0, 0.2, 0.0));
        assert!(t.abs_diff_eq(Vec3::NEG_Z, EPS));
    }

    #[test]
    fn wall_run_speed_uses_floor_and_flow_without_compounding() {
        let k = c();
        let slow = wall_run_velocity(Vec3::new(0.0, 0.0, -5.0), Vec3::X, 5.0, 1.0, &k);
        assert!((slow.length() - k.wall_run_min_speed).abs() < EPS);

        let boosted = wall_run_velocity(Vec3::new(0.0, 0.0, -12.8), Vec3::X, 12.8, 1.25, &k);
        assert!((boosted.length() - 16.0).abs() < EPS);

        // Feeding the output back in for many ticks never grows past the cap.
        let mut v = Vec3::new(0.0, 0.0, -12.8);
        for _ in 0..200 {
            v = wall_run_velocity(v, Vec3::X, 12.8, 1.25, &k);
        }
        assert!((v.length() - 16.0).abs() < EPS);

        let capped = wall_run_velocity(Vec3::new(0.0, 0.0, -16.0), Vec3::X, 16.0, 1.25, &k);
        assert!((capped.length() - k.max_flow_speed).abs() < EPS);
    }

    #[test]
    fn curvature_radius_from_normal_rotation() {
        // Normal +X rotating toward -Z while travelling -Z: convex.
        let turned = Quat::from_rotation_y(0.1) * Vec3::X;
        assert!(turned.z < 0.0);
        let r = estimate_wall_radius(Vec3::X, turned, Vec3::NEG_Z, 0.5);
        assert!((r - 5.0).abs() < 1e-2, "radius {r}");
        // Same rotation while travelling +Z: concave, no stick.
        assert!(estimate_wall_radius(Vec3::X, turned, Vec3::Z, 0.5).is_infinite());
        assert!(estimate_wall_radius(Vec3::X, Vec3::X, Vec3::NEG_Z, 0.5).is_infinite());
        assert!(estimate_wall_radius(Vec3::ZERO, Vec3::X, Vec3::NEG_Z, 0.5).is_infinite());
        assert!(estimate_wall_radius(Vec3::X, turned, Vec3::NEG_Z, 0.0).is_infinite());
    }

    #[test]
    fn curved_wall_adhesion_points_into_wall() {
        // 8 m radius wall at 12 m/s: v^2 / R = 18 m/s^2.
        let a = wall_stick_acceleration(Vec3::X, 12.0, 8.0, 1.0);
        assert!(a.normalize().abs_diff_eq(Vec3::NEG_X, EPS));
        assert!((a.length() - 144.0 / (8.0 + WALL_CURVATURE_EPSILON)).abs() < 1e-3);

        assert_eq!(
            wall_stick_acceleration(Vec3::X, 12.0, f32::INFINITY, 1.0),
            Vec3::ZERO
        );
        assert_eq!(
            wall_stick_acceleration(Vec3::X, 12.0, f32::NAN, 1.0),
            Vec3::ZERO
        );
        let sharp = wall_stick_acceleration(Vec3::X, 30.0, 0.0, 1.0);
        assert!((sharp.length() - MAX_WALL_STICK_ACCELERATION).abs() < EPS);
    }

    /// Runs one second around a pillar of radius 8 m (runner path radius
    /// 8.5 m, wall normal pointing away from the center) and returns the
    /// final distance from the pillar's center.
    fn run_around_pillar(with_stick: bool) -> f32 {
        let k = c();
        let mut position = Vec3::new(8.5, 0.0, 0.0);
        let mut previous_normal = position.normalize();
        let mut v = Vec3::new(0.0, 0.0, -12.0);
        for _ in 0..64 {
            let normal = Vec3::new(position.x, 0.0, position.z).normalize();
            let travelled = v.length() * DT;
            let r = estimate_wall_radius(previous_normal, normal, v, travelled);
            previous_normal = normal;
            let mut planar_v = wall_run_velocity(v, normal, 12.0, 1.0, &k);
            if with_stick {
                planar_v +=
                    wall_stick_acceleration(normal, planar_v.length(), r, k.wall_run_adhesion) * DT;
            }
            v = planar_v;
            position += v * DT;
        }
        Vec3::new(position.x, 0.0, position.z).length()
    }

    #[test]
    fn adhesion_keeps_runner_on_convex_wall() {
        let without = run_around_pillar(false);
        let with = run_around_pillar(true);
        println!("pillar run: without stick {without:.3} m, with stick {with:.3} m");
        assert!(
            without > 8.6,
            "without stick the runner should drift off ({without})"
        );
        assert!(
            with <= 8.5 + 0.05,
            "stick must keep the runner on the wall ({with})"
        );
        assert!(
            with > 8.0,
            "stick must not pull through the pillar ({with})"
        );
    }

    #[test]
    fn concave_wall_gets_no_stick_force() {
        // Inside a cylinder the normal points toward the center and rotates
        // against the direction of travel.
        let position = Vec3::new(7.5, 0.0, 0.0);
        let n0 = (-position).normalize();
        let next = Quat::from_rotation_y(0.02) * position;
        let n1 = (-next).normalize();
        let travel = next - position;
        let r = estimate_wall_radius(n0, n1, travel, travel.length());
        assert!(r.is_infinite());
    }

    #[test]
    fn sink_ramps_toward_sink_speed() {
        let k = c();
        assert_eq!(wall_run_sink_velocity(0.0, &k), 0.0);
        let at_tau = wall_run_sink_velocity(k.wall_run_sink_time_constant, &k);
        assert!((at_tau - -1.25 * (1.0 - (-1.0f32).exp())).abs() < EPS);
        assert!((wall_run_sink_velocity(10.0, &k) - -1.25).abs() < EPS);
        assert!(wall_run_sink_velocity(f32::NAN, &k).is_finite());
    }

    #[test]
    fn wall_kick_composite_impulse() {
        let k = c();
        // Left wall (normal +X), camera looking down the corridor (-Z).
        let j = wall_kick_impulse(Vec3::X, Vec3::NEG_Z, &k);
        assert!(j.abs_diff_eq(Vec3::new(9.2, 7.8, -11.5), EPS), "{j:?}");

        // Camera pitch does not leak into the forward term.
        let pitched = wall_kick_impulse(Vec3::X, Vec3::new(0.0, -0.8, -0.6), &k);
        assert!(pitched.abs_diff_eq(Vec3::new(9.2, 7.8, -11.5), EPS));

        // Looking into the wall: the into-wall part is removed.
        let into = wall_kick_impulse(Vec3::X, Vec3::new(-1.0, 0.0, -1.0), &k);
        assert!(into.x > 0.0 && (into.x - 9.2).abs() < EPS, "{into:?}");

        // Looking straight at the wall: pure push-off and lift.
        let head_on = wall_kick_impulse(Vec3::X, Vec3::NEG_X, &k);
        assert!(head_on.abs_diff_eq(Vec3::new(9.2, 7.8, 0.0), EPS));
    }

    #[test]
    fn mantle_converges_and_snaps() {
        let target = Vec3::new(0.0, 2.9, -1.0);
        let mut position = Vec3::new(0.0, 1.0, 0.0);
        let mut ticks = 0;
        loop {
            let (next, done) = mantle_step(position, target, 14.0, DT);
            ticks += 1;
            assert!(next.distance(target) <= position.distance(target) + EPS);
            position = next;
            if done {
                break;
            }
            assert!(ticks < 64, "mantle never arrived");
        }
        assert_eq!(position, target);
        // 2.15 m at 14 m/s ~= 0.154 s; the 0.2 m snap saves a tick.
        assert!((9..=11).contains(&ticks), "ticks {ticks}");
    }

    #[test]
    fn vault_arc_peaks_mid_way() {
        assert_eq!(vault_height_offset(0.0, 1.0), 0.0);
        assert!((vault_height_offset(0.5, 1.0) - 1.0).abs() < EPS);
        assert!(vault_height_offset(1.0, 1.0).abs() < EPS);
        assert_eq!(vault_height_offset(2.0, 1.0), vault_height_offset(1.0, 1.0));
    }

    #[test]
    fn landing_classification() {
        let k = c();
        assert_eq!(classify_landing(-5.0, false, &k), LandingOutcome::Normal);
        assert_eq!(classify_landing(-14.0, false, &k), LandingOutcome::Normal);
        assert_eq!(classify_landing(-14.5, false, &k), LandingOutcome::Hard);
        assert_eq!(classify_landing(-30.0, true, &k), LandingOutcome::Roll);
        assert_eq!(
            classify_landing(f32::NAN, false, &k),
            LandingOutcome::Normal
        );
    }

    #[test]
    fn airborne_phases() {
        assert_eq!(airborne_phase(9.4), ParkourState::JumpAscent);
        assert_eq!(airborne_phase(0.5), ParkourState::JumpApex);
        assert_eq!(airborne_phase(-0.5), ParkourState::JumpApex);
        assert_eq!(airborne_phase(-3.0), ParkourState::JumpDescent);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_core::time::FIXED_TIMESTEP_SECS as DT;

    const EPS: f32 = 1e-3;

    fn constants() -> ParkourPhysicsConstants {
        ParkourPhysicsConstants::default()
    }

    /// Normal of a slope inclined `degrees`, descending toward +Z.
    fn slope_normal(degrees: f32) -> Vec3 {
        Quat::from_rotation_x(degrees.to_radians()) * Vec3::Y
    }

    #[test]
    fn defaults_match_spec() {
        let c = constants();
        assert_eq!(c.walk_speed, 5.2);
        assert_eq!(c.sprint_speed, 12.8);
        assert_eq!(c.max_flow_speed, 17.5);
        assert_eq!(c.slide_min_speed, 7.0);
        assert_eq!(c.slide_max_speed, 25.0);
        assert_eq!(c.slide_flat_decay, 1.45);
        assert_eq!(c.slide_kinetic_friction, 0.035);
        assert_eq!(c.wall_run_min_speed, 8.5);
        assert_eq!(c.wall_run_max_duration, 2.2);
        assert_eq!(c.wall_run_sink_speed, 1.25);
        assert_eq!(c.wall_kick_normal_impulse, 9.2);
        assert_eq!(c.wall_kick_forward_impulse, 11.5);
        assert_eq!(c.wall_kick_up_impulse, 7.8);
        assert_eq!(c.jump_vertical_impulse, 9.4);
        assert_eq!(c.gravity_acceleration, 24.5);
        assert_eq!(c.coyote_time_duration, 0.15);
        assert_eq!(c.jump_buffer_duration, 0.12);
        assert_eq!(c.mantle_snap_speed, 14.0);
    }

    #[test]
    fn projection_removes_normal_component() {
        let v = Vec3::new(3.0, -4.0, 5.0);
        assert_eq!(
            project_velocity_on_surface(v, Vec3::Y),
            Vec3::new(3.0, 0.0, 5.0)
        );

        let n = slope_normal(35.0);
        let projected = project_velocity_on_surface(v, n);
        assert!(projected.dot(n).abs() < EPS);

        assert_eq!(project_velocity_on_surface(v, Vec3::ZERO), v);
        assert_eq!(project_velocity_on_surface(v, Vec3::NAN), v);
        // An unnormalized normal gives the same result.
        assert!((project_velocity_on_surface(v, n * 7.0) - projected).length() < EPS);
    }

    #[test]
    fn slope_acceleration_formula() {
        let (dir, a) = compute_slope_slide_acceleration(slope_normal(35.0), 24.5, 0.035);
        let theta = 35.0_f32.to_radians();
        let expected = 24.5 * (theta.sin() - 0.035 * theta.cos());
        assert!((a - expected).abs() < EPS, "a {a} expected {expected}");
        assert!((dir.length() - 1.0).abs() < EPS);
        assert!(dir.y < 0.0 && dir.z > 0.0, "downslope {dir:?}");

        let (flat_dir, flat_a) = compute_slope_slide_acceleration(Vec3::Y, 24.5, 0.035);
        assert_eq!(flat_dir, Vec3::ZERO);
        assert!((flat_a - -24.5 * 0.035).abs() < EPS);
    }

    #[test]
    fn slide_down_35_degrees_accelerates_to_terminal_speed() {
        let c = constants();
        let n = slope_normal(35.0);
        let (downslope, _) = compute_slope_slide_acceleration(n, c.gravity_acceleration, 0.0);
        let mut v = downslope * c.sprint_speed;

        let mut previous = v.length();
        assert!((previous - 12.8).abs() < EPS);
        let mut reached_terminal_at = None;
        for tick in 0..256 {
            integrate_slide(&mut v, n, DT, &c);
            let speed = v.length();
            assert!(speed >= previous - EPS, "slowed at tick {tick}");
            assert!(speed <= c.slide_max_speed + EPS, "exceeded terminal");
            if reached_terminal_at.is_none() && (speed - c.slide_max_speed).abs() < EPS {
                reached_terminal_at = Some(tick);
            }
            previous = speed;
        }
        let tick = reached_terminal_at.expect("never reached 25 m/s");
        // (25 - 12.8) / 13.35 m/s^2 ~= 0.91 s ~= 59 ticks.
        assert!((50..70).contains(&tick), "terminal at tick {tick}");
        assert!((v.length() - 25.0).abs() < EPS);
        assert!(v.dot(n).abs() < EPS, "velocity left the surface");
    }

    #[test]
    fn integrated_slope_acceleration_matches_formula() {
        let c = constants();
        let n = slope_normal(35.0);
        let (downslope, expected) =
            compute_slope_slide_acceleration(n, c.gravity_acceleration, c.slide_kinetic_friction);
        let mut v = downslope * 10.0;
        integrate_slide(&mut v, n, DT, &c);
        let measured = (v.length() - 10.0) / DT;
        assert!(
            (measured - expected).abs() < 0.05,
            "measured {measured} expected {expected}"
        );
    }

    #[test]
    fn uphill_slide_decelerates() {
        let c = constants();
        let n = slope_normal(35.0);
        let (downslope, _) = compute_slope_slide_acceleration(n, c.gravity_acceleration, 0.0);
        let mut v = -downslope * 15.0;
        integrate_slide(&mut v, n, DT, &c);
        assert!(v.length() < 15.0);
        assert!(v.dot(downslope) < 0.0, "still moving uphill after one tick");
    }

    #[test]
    fn flat_slide_decays_exponentially_below_min_speed() {
        let c = constants();
        let mut v = Vec3::new(0.0, 0.0, -12.8);
        let mut crossed_at = None;
        for tick in 1..=128 {
            integrate_slide(&mut v, Vec3::Y, DT, &c);
            let expected = 12.8 * (-c.slide_flat_decay * DT * tick as f32).exp();
            assert!((v.length() - expected).abs() < EPS, "tick {tick}");
            if crossed_at.is_none() && slide_should_end(v, &c) {
                crossed_at = Some(tick);
            }
        }
        // ln(12.8 / 7.0) / 1.45 = 0.416 s -> tick 27 at 64 Hz.
        assert_eq!(crossed_at, Some(27));
        assert!(
            v.normalize().abs_diff_eq(Vec3::NEG_Z, EPS),
            "direction changed"
        );
    }

    #[test]
    fn gentle_slope_counts_as_flat() {
        let c = constants();
        let mut on_gentle = Vec3::new(0.0, 0.0, 10.0);
        integrate_slide(&mut on_gentle, slope_normal(4.0), DT, &c);
        assert!(on_gentle.length() < 10.0);
    }

    #[test]
    fn slide_entry_requires_sprint_and_speed() {
        let c = constants();
        let fast = Vec3::new(0.0, -3.0, -12.8);
        assert!(can_begin_slide(ParkourState::Sprinting, fast, &c));
        assert!(!can_begin_slide(ParkourState::Walking, fast, &c));
        assert!(!can_begin_slide(
            ParkourState::Sprinting,
            Vec3::new(0.0, -20.0, -5.0),
            &c
        ));
    }

    #[test]
    fn jump_sets_vertical_and_preserves_planar() {
        let c = constants();
        let mut v = Vec3::new(6.0, -3.0, -11.0);
        integrate_jump(&mut v, &c);
        assert_eq!(v, Vec3::new(6.0, 9.4, -11.0));

        let mut rising = Vec3::new(0.0, 20.0, 0.0);
        integrate_jump(&mut rising, &c);
        assert_eq!(rising.y, 9.4, "jump sets, does not add");
    }

    #[test]
    fn gravity_reduces_vertical_velocity() {
        let c = constants();
        let mut v = Vec3::new(1.0, 9.4, 0.0);
        let mut apex_tick = None;
        for tick in 1..=64 {
            integrate_gravity(&mut v, DT, &c);
            if apex_tick.is_none() && v.y <= 0.0 {
                apex_tick = Some(tick);
            }
        }
        // 9.4 / 24.5 = 0.384 s -> tick 25.
        assert_eq!(apex_tick, Some(25));
        assert_eq!(v.x, 1.0);
    }

    #[test]
    fn ground_locomotion_reaches_and_holds_sprint_speed() {
        let c = constants();
        let mut v = Vec3::new(0.0, -0.5, 0.0);
        for _ in 0..32 {
            integrate_ground_locomotion(&mut v, Vec2::new(0.0, -1.0), true, DT, &c);
        }
        assert_eq!(v, Vec3::new(0.0, -0.5, -12.8));

        // Matches the spec determinism check: 500 ticks at sprint = 100 m.
        let mut z = 0.0;
        for _ in 0..500 {
            integrate_ground_locomotion(&mut v, Vec2::new(0.0, -1.0), true, DT, &c);
            z += v.z * DT;
        }
        assert!((z - -100.0).abs() < EPS, "z {z}");
    }

    #[test]
    fn ground_locomotion_walk_stop_and_overspeed() {
        let c = constants();
        let mut v = Vec3::ZERO;
        for _ in 0..32 {
            integrate_ground_locomotion(&mut v, Vec2::new(5.0, 0.0), false, DT, &c);
        }
        assert_eq!(v, Vec3::new(5.2, 0.0, 0.0), "input clamped, walk speed");

        let step = c.ground_acceleration * DT;
        integrate_ground_locomotion(&mut v, Vec2::ZERO, false, DT, &c);
        assert!(
            (v.x - (5.2 - step)).abs() < EPS,
            "decelerates at ground rate"
        );

        let mut fast = Vec3::new(0.0, 0.0, -20.0);
        integrate_ground_locomotion(&mut fast, Vec2::new(0.0, -1.0), true, DT, &c);
        assert!(
            fast.z > -20.0 && fast.z < -12.8,
            "bleeds toward sprint speed"
        );
    }

    #[test]
    fn integrators_sanitize_poisoned_input() {
        let c = constants();
        let poisoned = Vec3::new(f32::NAN, f32::INFINITY, f32::NEG_INFINITY);

        let mut v = poisoned;
        integrate_slide(&mut v, Vec3::Y, DT, &c);
        assert!(v.is_finite());

        let mut v = poisoned;
        integrate_slide(&mut v, Vec3::NAN, f32::NAN, &c);
        assert!(v.is_finite());

        let mut v = poisoned;
        integrate_ground_locomotion(&mut v, Vec2::new(f32::NAN, f32::INFINITY), true, DT, &c);
        assert!(v.is_finite());

        let mut v = poisoned;
        integrate_jump(&mut v, &c);
        assert_eq!(v, Vec3::new(0.0, 9.4, 0.0));

        let mut v = poisoned;
        integrate_gravity(&mut v, f32::INFINITY, &c);
        assert!(v.is_finite());

        let mut v = Vec3::new(1.0, 2.0, 3.0);
        integrate_gravity(&mut v, -1.0, &c);
        assert_eq!(v, Vec3::new(1.0, 2.0, 3.0), "negative dt ignored");

        let mut k = KinematicVelocity::new(poisoned);
        assert_eq!(k.0, Vec3::ZERO);
        k.set(Vec3::new(f32::NAN, 4.0, 0.0));
        assert_eq!(k.0, Vec3::new(0.0, 4.0, 0.0));
        assert_eq!(
            KinematicVelocity(Vec3::new(3.0, 9.0, 4.0)).planar_speed(),
            5.0
        );
    }

    #[test]
    fn sanitize_system_repairs_only_bad_velocities() {
        let mut app = App::new();
        app.add_systems(Update, sanitize_kinematic_velocities);
        let bad = app
            .world_mut()
            .spawn(KinematicVelocity(Vec3::new(f32::NAN, 1.0, f32::INFINITY)))
            .id();
        let good = app.world_mut().spawn(KinematicVelocity(Vec3::X)).id();
        app.update();
        app.update();

        let world = app.world();
        assert_eq!(
            world.get::<KinematicVelocity>(bad).map(|v| v.0),
            Some(Vec3::new(0.0, 1.0, 0.0))
        );
        assert_eq!(
            world.get::<KinematicVelocity>(good).map(|v| v.0),
            Some(Vec3::X)
        );
    }
}
