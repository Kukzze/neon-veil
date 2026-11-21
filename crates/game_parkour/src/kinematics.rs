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
