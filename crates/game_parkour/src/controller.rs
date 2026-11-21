//! Per-tick parkour character controller.
//!
//! [`step_parkour`] is a pure state-transition and velocity function over the
//! sensor readings and input; [`update_parkour_controller`] applies its
//! result to the transform through capsule collide-and-slide.

use crate::flow_state::{FlowState, FLOW_LANDING_ROLL_REWARD, FLOW_WALL_KICK_REWARD};
use crate::input_buffers::LocomotionBuffer;
use crate::kinematics::{
    airborne_phase, can_begin_slide, classify_landing, estimate_wall_radius, integrate_gravity,
    integrate_ground_locomotion, integrate_jump, integrate_slide, mantle_step,
    project_velocity_on_surface, slide_should_end, vault_height_offset, wall_kick_impulse,
    wall_run_sink_velocity, wall_run_tangent, wall_run_velocity, wall_stick_acceleration,
    KinematicVelocity, LandingOutcome, ParkourPhysicsConstants,
};
use crate::shapecasts::{
    ParkourSensorReadings, CHARACTER_HEIGHT, CHARACTER_RADIUS, GROUND_PROBE_HEIGHT,
};
use crate::state_machine::ParkourState;
use avian3d::prelude::*;
use bevy::prelude::*;
use game_core::input::PlayerInputState;
use game_core::math::{sanitize_f32, sanitize_vec3, wrap_angle_radians};
use game_core::protocol::{InputActions, PlayerInputPacket};

/// Gap kept between the feet and the ground so horizontal casts do not snag.
pub const GROUND_HOVER: f32 = 0.03;
/// Collision skin: the cast capsule is this much thinner than the character.
pub const COLLISION_SKIN: f32 = 0.02;
pub const MAX_SLIDE_ITERATIONS: usize = 4;
/// Largest per-tick vertical ground-snap correction, m.
pub const MAX_GROUND_SNAP: f32 = 0.3;
/// Crouch-walk target speed as a fraction of walk speed.
pub const CROUCH_SPEED_FACTOR: f32 = 0.5;
/// Wall runs start only near the apex or while falling, m/s.
pub const WALL_RUN_MAX_ENTRY_VERTICAL_SPEED: f32 = 2.0;
/// A wall run needs the motion to be at least this far from head-on
/// (cosine between travel direction and the inward wall normal).
pub const WALL_RUN_MAX_HEAD_ON_COS: f32 = 0.8;
/// Lateral push applied when a wall kick is denied for lack of stamina, m/s.
pub const WALL_DETACH_PUSH: f32 = 2.0;
/// Input must point within ~45 degrees of the obstacle to start a vault.
pub const VAULT_MIN_INPUT_ALIGNMENT: f32 = 0.7;
/// Extra horizontal travel past the obstacle before a vault ends, m.
pub const VAULT_EXIT_MARGIN: f32 = 0.1;

/// Movement intent for one fixed tick.
#[derive(Component, Reflect, Debug, Default, Clone, Copy, PartialEq)]
#[reflect(Component, Default, Debug, PartialEq)]
pub struct ParkourInput {
    /// World-space XZ movement intent (`x` -> X, `y` -> Z), length <= 1.
    pub move_direction: Vec2,
    /// Camera yaw in radians; the character faces `-Z` rotated by it.
    pub yaw: f32,
    pub sprint: bool,
    /// Held crouch / slide button.
    pub crouch: bool,
    /// Jump pressed this tick (edge). Cleared by the controller once read.
    pub jump_pressed: bool,
    /// Slide pressed this tick (edge). Cleared by the controller once read.
    pub slide_pressed: bool,
}

impl ParkourInput {
    /// Planar camera forward from `yaw`.
    pub fn facing_forward(&self) -> Vec3 {
        Quat::from_rotation_y(sanitize_f32(self.yaw)) * Vec3::NEG_Z
    }

    /// Builds input from a network packet. `previous` is the prior tick's
    /// packet, used to turn held action bits into press edges.
    pub fn from_packet(packet: &PlayerInputPacket, previous: Option<&PlayerInputPacket>) -> Self {
        let p = packet.sanitized();
        let held_before = |flag: u32| previous.is_some_and(|prev| prev.has_action(flag));
        let local = Vec3::new(p.movement_vector.x, 0.0, -p.movement_vector.y);
        let world = Quat::from_rotation_y(p.yaw) * local;
        Self {
            move_direction: Vec2::new(world.x, world.z),
            yaw: p.yaw,
            sprint: p.has_action(InputActions::SPRINT),
            crouch: p.has_action(InputActions::SLIDE),
            jump_pressed: p.has_action(InputActions::JUMP) && !held_before(InputActions::JUMP),
            slide_pressed: p.has_action(InputActions::SLIDE) && !held_before(InputActions::SLIDE),
        }
    }

    fn is_moving(&self) -> bool {
        self.move_direction.length_squared() > 0.01
    }
}

/// Controller bookkeeping. Adding it to an entity (with a `Transform`) makes
/// it a parkour character; the other parkour components are required and
/// inserted automatically.
#[derive(Component, Reflect, Debug, Default, Clone, PartialEq)]
#[reflect(Component, Default, Debug)]
#[require(
    Transform,
    KinematicVelocity,
    ParkourState,
    LocomotionBuffer,
    FlowState,
    ParkourSensorReadings,
    ParkourInput
)]
pub struct ParkourController {
    /// Time spent in the current state, s.
    pub state_time: f32,
    pub wall_run_time: f32,
    pub wall_run_entry_speed: f32,
    /// Wall normal seen on the previous wall-run tick.
    pub wall_normal: Vec3,
    pub wall_reattach_cooldown: f32,
    pub wall_kick_elapsed: f32,
    pub mantle_target: Vec3,
    pub vault_elapsed: f32,
    pub vault_duration: f32,
    /// Capsule-center height at vault take-off.
    pub vault_start_height: f32,
    /// Peak height of the vault arc above take-off.
    pub vault_peak_height: f32,
    /// Remaining hard-landing lock or landing-roll time, s.
    pub landing_timer: f32,
    /// Vertical velocity before collision clipping, kept so landings see the
    /// true impact speed.
    pub airborne_vertical_velocity: f32,
}

/// How the controller wants the transform moved this tick.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ParkourMotion {
    /// Move by `velocity * dt` with collide-and-slide, after an optional
    /// vertical ground-snap correction.
    Move { ground_snap: Option<f32> },
    /// Place the capsule center exactly here (scripted mantle / vault).
    Place(Vec3),
    /// State is driven by another system; leave the transform alone.
    Hold,
}

/// Mutable character state advanced by [`step_parkour`].
pub struct ParkourCharacter<'a> {
    pub state: &'a mut ParkourState,
    pub velocity: &'a mut Vec3,
    pub buffer: &'a mut LocomotionBuffer,
    pub flow: &'a mut FlowState,
    pub controller: &'a mut ParkourController,
}

/// Read-only inputs to [`step_parkour`].
pub struct StepContext<'a> {
    pub readings: &'a ParkourSensorReadings,
    pub input: ParkourInput,
    pub constants: &'a ParkourPhysicsConstants,
    pub dt: f32,
    /// Capsule center.
    pub position: Vec3,
}

#[inline]
fn planar(v: Vec3) -> Vec3 {
    Vec3::new(v.x, 0.0, v.z)
}

fn ground_snap(readings: &ParkourSensorReadings) -> Option<f32> {
    Some(GROUND_PROBE_HEIGHT + GROUND_HOVER - readings.ground_distance)
}

fn ground_state_for_input(input: &ParkourInput, planar_speed: f32) -> ParkourState {
    if input.crouch {
        ParkourState::Crouching
    } else if !input.is_moving() && planar_speed < 0.1 {
        ParkourState::Idle
    } else if input.sprint && input.is_moving() {
        ParkourState::Sprinting
    } else {
        ParkourState::Walking
    }
}

fn start_jump(ch: &mut ParkourCharacter, constants: &ParkourPhysicsConstants) -> ParkourMotion {
    ch.buffer.consume_coyote_jump();
    integrate_jump(ch.velocity, constants);
    ch.controller.airborne_vertical_velocity = ch.velocity.y;
    *ch.state = ParkourState::JumpAscent;
    ParkourMotion::Move { ground_snap: None }
}

fn fall(ch: &mut ParkourCharacter, ctx: &StepContext) -> ParkourMotion {
    *ch.state = ParkourState::JumpDescent;
    integrate_gravity(ch.velocity, ctx.dt, ctx.constants);
    ch.controller.airborne_vertical_velocity = ch.velocity.y;
    ParkourMotion::Move { ground_snap: None }
}

fn detach_from_wall(ch: &mut ParkourCharacter, ctx: &StepContext) -> ParkourMotion {
    ch.controller.wall_reattach_cooldown = ctx.constants.wall_reattach_cooldown;
    fall(ch, ctx)
}

fn grounded_step(ch: &mut ParkourCharacter, ctx: &StepContext) -> ParkourMotion {
    let (r, c, input) = (ctx.readings, ctx.constants, &ctx.input);
    if !r.is_ground_detected {
        return fall(ch, ctx);
    }
    if ch.buffer.consume_jump_buffer() {
        return start_jump(ch, c);
    }
    let wants_slide = input.crouch || ch.buffer.slide_buffer_timer > 0.0;
    if wants_slide && can_begin_slide(*ch.state, *ch.velocity, c) {
        ch.buffer.consume_slide_buffer();
        *ch.velocity = project_velocity_on_surface(*ch.velocity, r.ground_normal);
        *ch.state = ParkourState::Sliding;
        return ParkourMotion::Move {
            ground_snap: ground_snap(r),
        };
    }
    if let Some(motion) = try_start_vault(ch, ctx) {
        return motion;
    }

    let direction = if input.crouch {
        input.move_direction.clamp_length_max(1.0) * CROUCH_SPEED_FACTOR
    } else {
        input.move_direction
    };
    integrate_ground_locomotion(
        ch.velocity,
        direction,
        input.sprint && !input.crouch,
        ctx.dt,
        c,
    );
    ch.velocity.y = 0.0;
    *ch.state = ground_state_for_input(input, planar(*ch.velocity).length());
    ParkourMotion::Move {
        ground_snap: ground_snap(r),
    }
}

fn sliding_step(ch: &mut ParkourCharacter, ctx: &StepContext) -> ParkourMotion {
    let (r, c) = (ctx.readings, ctx.constants);
    if !r.is_ground_detected {
        return fall(ch, ctx);
    }
    if ch.buffer.consume_jump_buffer() {
        return start_jump(ch, c);
    }
    integrate_slide(ch.velocity, r.ground_normal, ctx.dt, c);
    if slide_should_end(*ch.velocity, c) {
        *ch.state = ground_state_for_input(&ctx.input, planar(*ch.velocity).length());
    }
    ParkourMotion::Move {
        ground_snap: ground_snap(r),
    }
}

fn try_start_vault(ch: &mut ParkourCharacter, ctx: &StepContext) -> Option<ParkourMotion> {
    let (r, c, input) = (ctx.readings, ctx.constants, &ctx.input);
    let vault_state = if r.can_vault_low {
        ParkourState::VaultLow
    } else if r.can_vault_mid {
        ParkourState::VaultMid
    } else if r.can_vault_high {
        ParkourState::VaultHigh
    } else {
        return None;
    };
    let intent = Vec3::new(input.move_direction.x, 0.0, input.move_direction.y);
    if !input.is_moving() || intent.normalize().dot(r.forward) < VAULT_MIN_INPUT_ALIGNMENT {
        return None;
    }

    let speed = planar(*ch.velocity).length().max(c.walk_speed);
    let travel = r.obstacle_distance + r.vault_depth + 2.0 * CHARACTER_RADIUS + VAULT_EXIT_MARGIN;
    let ctrl = &mut *ch.controller;
    ctrl.vault_elapsed = 0.0;
    ctrl.vault_duration = travel / speed;
    ctrl.vault_start_height = ctx.position.y;
    ctrl.vault_peak_height = r.obstacle_top_height + c.vault_clearance;
    *ch.velocity = r.forward * speed;
    *ch.state = vault_state;
    Some(vault_step(ch, ctx))
}

fn vault_step(ch: &mut ParkourCharacter, ctx: &StepContext) -> ParkourMotion {
    let ctrl = &mut *ch.controller;
    ctrl.vault_elapsed += ctx.dt;
    let progress = (ctrl.vault_elapsed / ctrl.vault_duration.max(1e-3)).min(1.0);
    ch.velocity.y = 0.0;
    let mut position = ctx.position + planar(*ch.velocity) * ctx.dt;
    position.y = ctrl.vault_start_height + vault_height_offset(progress, ctrl.vault_peak_height);
    if progress >= 1.0 {
        *ch.state = ground_state_for_input(&ctx.input, planar(*ch.velocity).length());
    }
    ParkourMotion::Place(position)
}

fn try_start_wall_run(ch: &mut ParkourCharacter, ctx: &StepContext) -> Option<ParkourMotion> {
    let (r, c) = (ctx.readings, ctx.constants);
    if ch.controller.wall_reattach_cooldown > 0.0
        || r.is_ground_detected
        || ch.velocity.y > WALL_RUN_MAX_ENTRY_VERTICAL_SPEED
    {
        return None;
    }
    let travel = planar(*ch.velocity);
    let speed = travel.length();
    if speed < c.wall_run_min_speed {
        return None;
    }
    let (state, normal) = if r.is_left_wall_detected {
        (ParkourState::WallRunLeft, r.left_wall_normal)
    } else if r.is_right_wall_detected {
        (ParkourState::WallRunRight, r.right_wall_normal)
    } else {
        return None;
    };
    if (travel / speed).dot(-normal) > WALL_RUN_MAX_HEAD_ON_COS
        || wall_run_tangent(travel, normal) == Vec3::ZERO
    {
        return None;
    }
    let ctrl = &mut *ch.controller;
    ctrl.wall_run_time = 0.0;
    ctrl.wall_run_entry_speed = speed;
    ctrl.wall_normal = normal;
    *ch.state = state;
    Some(wall_run_step(ch, ctx))
}

fn wall_run_step(ch: &mut ParkourCharacter, ctx: &StepContext) -> ParkourMotion {
    let (r, c) = (ctx.readings, ctx.constants);
    ch.controller.wall_run_time += ctx.dt;
    let (detected, normal) = if *ch.state == ParkourState::WallRunLeft {
        (r.is_left_wall_detected, r.left_wall_normal)
    } else {
        (r.is_right_wall_detected, r.right_wall_normal)
    };
    if !detected || r.is_ground_detected || ch.controller.wall_run_time >= c.wall_run_max_duration {
        return detach_from_wall(ch, ctx);
    }

    if ch.buffer.consume_jump_buffer() {
        if ch.flow.consume_stamina(c.wall_kick_stamina_cost) {
            *ch.velocity = wall_kick_impulse(normal, ctx.input.facing_forward(), c);
            ch.flow.add_flow(FLOW_WALL_KICK_REWARD);
            ch.controller.wall_kick_elapsed = 0.0;
            ch.controller.wall_reattach_cooldown = c.wall_reattach_cooldown;
            ch.controller.airborne_vertical_velocity = ch.velocity.y;
            *ch.state = ParkourState::WallKick;
            return ParkourMotion::Move { ground_snap: None };
        }
        *ch.velocity += normal * WALL_DETACH_PUSH;
        return detach_from_wall(ch, ctx);
    }

    let travel = planar(*ch.velocity);
    let radius = estimate_wall_radius(
        ch.controller.wall_normal,
        normal,
        travel,
        travel.length() * ctx.dt,
    );
    ch.controller.wall_normal = normal;
    let mut along_wall = wall_run_velocity(
        travel,
        normal,
        ch.controller.wall_run_entry_speed,
        ch.flow.current_multiplier,
        c,
    );
    if along_wall == Vec3::ZERO {
        return detach_from_wall(ch, ctx);
    }
    along_wall +=
        wall_stick_acceleration(normal, along_wall.length(), radius, c.wall_run_adhesion) * ctx.dt;
    *ch.velocity = along_wall + Vec3::Y * wall_run_sink_velocity(ch.controller.wall_run_time, c);
    ParkourMotion::Move { ground_snap: None }
}

fn land(ch: &mut ParkourCharacter, ctx: &StepContext) -> ParkourMotion {
    let c = ctx.constants;
    let impact = ch.controller.airborne_vertical_velocity.min(ch.velocity.y);
    let outcome = classify_landing(impact, ch.buffer.slide_buffer_timer > 0.0, c);
    ch.velocity.y = 0.0;
    ch.controller.airborne_vertical_velocity = 0.0;
    match outcome {
        LandingOutcome::Roll => {
            ch.buffer.consume_slide_buffer();
            ch.flow.add_flow(FLOW_LANDING_ROLL_REWARD);
            ch.controller.landing_timer = c.landing_roll_duration;
            *ch.state = ParkourState::LandingRoll;
        }
        LandingOutcome::Hard => {
            *ch.velocity = Vec3::ZERO;
            ch.controller.landing_timer = c.hard_landing_lock_duration;
            *ch.state = ParkourState::HardLanding;
        }
        LandingOutcome::Normal => {
            *ch.state = ground_state_for_input(&ctx.input, planar(*ch.velocity).length());
        }
    }
    ParkourMotion::Move {
        ground_snap: ground_snap(ctx.readings),
    }
}

fn start_mantle(ch: &mut ParkourCharacter, ctx: &StepContext) -> ParkourMotion {
    ch.controller.mantle_target = ctx.readings.mantle_target_position;
    *ch.state = ParkourState::LedgeMantle;
    mantle_motion(ch, ctx)
}

fn mantle_motion(ch: &mut ParkourCharacter, ctx: &StepContext) -> ParkourMotion {
    let (position, arrived) = mantle_step(
        ctx.position,
        ch.controller.mantle_target,
        ctx.constants.mantle_snap_speed,
        ctx.dt,
    );
    *ch.velocity = Vec3::ZERO;
    if arrived {
        *ch.state = ground_state_for_input(&ctx.input, 0.0);
    }
    ParkourMotion::Place(position)
}

fn airborne_step(ch: &mut ParkourCharacter, ctx: &StepContext) -> ParkourMotion {
    let (r, c) = (ctx.readings, ctx.constants);
    if ch.buffer.jump_buffer_timer > 0.0 && ch.buffer.can_coyote_jump() {
        ch.buffer.consume_jump_buffer();
        return start_jump(ch, c);
    }
    if r.can_ledge_mantle {
        return start_mantle(ch, ctx);
    }
    if let Some(motion) = try_start_wall_run(ch, ctx) {
        return motion;
    }
    if r.is_ground_detected && ch.velocity.y <= 0.0 {
        return land(ch, ctx);
    }
    integrate_gravity(ch.velocity, ctx.dt, c);
    ch.controller.airborne_vertical_velocity = ch.velocity.y;
    *ch.state = airborne_phase(ch.velocity.y);
    ParkourMotion::Move { ground_snap: None }
}

fn wall_kick_step(ch: &mut ParkourCharacter, ctx: &StepContext) -> ParkourMotion {
    ch.controller.wall_kick_elapsed += ctx.dt;
    if ctx.readings.is_ground_detected && ch.velocity.y <= 0.0 {
        return land(ch, ctx);
    }
    integrate_gravity(ch.velocity, ctx.dt, ctx.constants);
    ch.controller.airborne_vertical_velocity = ch.velocity.y;
    if ch.controller.wall_kick_elapsed >= ctx.constants.wall_kick_duration {
        *ch.state = airborne_phase(ch.velocity.y);
    }
    ParkourMotion::Move { ground_snap: None }
}

fn hard_landing_step(ch: &mut ParkourCharacter, ctx: &StepContext) -> ParkourMotion {
    if !ctx.readings.is_ground_detected {
        return fall(ch, ctx);
    }
    ch.controller.landing_timer -= ctx.dt;
    *ch.velocity = Vec3::ZERO;
    if ch.controller.landing_timer <= 0.0 {
        *ch.state = ground_state_for_input(&ctx.input, 0.0);
    }
    ParkourMotion::Move {
        ground_snap: ground_snap(ctx.readings),
    }
}

fn landing_roll_step(ch: &mut ParkourCharacter, ctx: &StepContext) -> ParkourMotion {
    if !ctx.readings.is_ground_detected {
        return fall(ch, ctx);
    }
    if ch.buffer.consume_jump_buffer() {
        return start_jump(ch, ctx.constants);
    }
    ch.controller.landing_timer -= ctx.dt;
    ch.velocity.y = 0.0;
    if ch.controller.landing_timer <= 0.0 {
        *ch.state = ground_state_for_input(&ctx.input, planar(*ch.velocity).length());
    }
    ParkourMotion::Move {
        ground_snap: ground_snap(ctx.readings),
    }
}

/// Advances one character by one fixed tick: buffers this tick's presses,
/// evaluates state transitions in priority order and integrates velocity.
///
/// `WallClimb`, `LedgeGrab`, `ZiplineTraverse`, `GrapplePull` and `Ragdoll`
/// are driven by other systems; for them this returns [`ParkourMotion::Hold`].
pub fn step_parkour(ch: &mut ParkourCharacter, ctx: &StepContext) -> ParkourMotion {
    let c = ctx.constants;
    if ctx.input.jump_pressed {
        ch.buffer.buffer_jump(c.jump_buffer_duration);
    }
    if ctx.input.slide_pressed {
        ch.buffer.buffer_slide(c.jump_buffer_duration);
    }
    ch.controller.wall_reattach_cooldown = (ch.controller.wall_reattach_cooldown - ctx.dt).max(0.0);

    let previous = *ch.state;
    let motion = match previous {
        ParkourState::Idle
        | ParkourState::Walking
        | ParkourState::Sprinting
        | ParkourState::Crouching => grounded_step(ch, ctx),
        ParkourState::Sliding => sliding_step(ch, ctx),
        ParkourState::JumpAscent | ParkourState::JumpApex | ParkourState::JumpDescent => {
            airborne_step(ch, ctx)
        }
        ParkourState::WallRunLeft | ParkourState::WallRunRight => wall_run_step(ch, ctx),
        ParkourState::WallKick => wall_kick_step(ch, ctx),
        ParkourState::LedgeMantle => mantle_motion(ch, ctx),
        ParkourState::VaultLow | ParkourState::VaultMid | ParkourState::VaultHigh => {
            vault_step(ch, ctx)
        }
        ParkourState::HardLanding => hard_landing_step(ch, ctx),
        ParkourState::LandingRoll => landing_roll_step(ch, ctx),
        ParkourState::WallClimb
        | ParkourState::LedgeGrab
        | ParkourState::ZiplineTraverse
        | ParkourState::GrapplePull
        | ParkourState::Ragdoll => ParkourMotion::Hold,
    };

    if *ch.state == previous {
        ch.controller.state_time += ctx.dt;
    } else {
        ch.controller.state_time = 0.0;
    }
    *ch.velocity = sanitize_vec3(*ch.velocity);
    motion
}

/// Moves a capsule from `center` by `velocity * dt`, sliding along whatever
/// it hits. Velocity components into hit surfaces are removed.
pub fn move_and_slide(
    spatial: &SpatialQuery,
    shape: &Collider,
    filter: &SpatialQueryFilter,
    center: Vec3,
    velocity: &mut Vec3,
    dt: f32,
) -> Vec3 {
    let mut position = center;
    let mut remaining = *velocity * sanitize_f32(dt).max(0.0);
    for _ in 0..MAX_SLIDE_ITERATIONS {
        let distance = remaining.length();
        if distance < 1e-5 {
            break;
        }
        let Ok(direction) = Dir3::new(remaining) else {
            break;
        };
        let config = ShapeCastConfig {
            max_distance: distance + COLLISION_SKIN,
            ignore_origin_penetration: true,
            ..ShapeCastConfig::DEFAULT
        };
        let Some(hit) =
            spatial.cast_shape(shape, position, Quat::IDENTITY, direction, &config, filter)
        else {
            position += remaining;
            break;
        };
        let travel = (hit.distance - COLLISION_SKIN).clamp(0.0, distance);
        position += *direction * travel;
        let normal = hit.normal1.try_normalize().unwrap_or(-*direction);
        remaining -= *direction * travel;
        let into = remaining.dot(normal);
        if into < 0.0 {
            remaining -= normal * into;
        }
        let velocity_into = velocity.dot(normal);
        if velocity_into < 0.0 {
            *velocity -= normal * velocity_into;
        }
    }
    sanitize_vec3(position)
}

/// The collision capsule used for character movement casts.
pub fn character_cast_shape() -> Collider {
    let radius = CHARACTER_RADIUS - COLLISION_SKIN;
    Collider::capsule(radius, CHARACTER_HEIGHT - 2.0 * CHARACTER_RADIUS)
}

/// Converts each fresh packet in [`PlayerInputState`] into [`ParkourInput`].
/// Local and remote characters both receive input this way. Without a new
/// packet the previous held input is kept (the controller already cleared
/// its press edges).
pub fn apply_player_input_packets(
    mut characters: Query<(&mut PlayerInputState, &mut ParkourInput)>,
) {
    for (mut state, mut input) in &mut characters {
        if let Some((packet, previous)) = state.take_fresh() {
            *input = ParkourInput::from_packet(&packet, previous.as_ref());
        }
    }
}

/// Runs [`step_parkour`] for every character and applies the motion.
#[allow(clippy::type_complexity)]
pub fn update_parkour_controller(
    time: Res<Time<Fixed>>,
    constants: Res<ParkourPhysicsConstants>,
    spatial: SpatialQuery,
    mut shape: Local<Option<Collider>>,
    mut characters: Query<(
        Entity,
        &mut Transform,
        &mut KinematicVelocity,
        &mut ParkourState,
        &ParkourSensorReadings,
        &mut LocomotionBuffer,
        &mut FlowState,
        &mut ParkourController,
        &mut ParkourInput,
    )>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }
    let shape = shape.get_or_insert_with(character_cast_shape);

    for (
        entity,
        mut transform,
        mut velocity,
        mut state,
        readings,
        mut buffer,
        mut flow,
        mut ctrl,
        mut input,
    ) in &mut characters
    {
        let mut tick_input = *input;
        tick_input.yaw = wrap_angle_radians(tick_input.yaw);
        input.jump_pressed = false;
        input.slide_pressed = false;

        let ctx = StepContext {
            readings,
            input: tick_input,
            constants: &constants,
            dt,
            position: transform.translation,
        };
        let mut v = velocity.0;
        let motion = step_parkour(
            &mut ParkourCharacter {
                state: &mut state,
                velocity: &mut v,
                buffer: &mut buffer,
                flow: &mut flow,
                controller: &mut ctrl,
            },
            &ctx,
        );

        match motion {
            ParkourMotion::Move { ground_snap } => {
                let mut start = transform.translation;
                if let Some(snap) = ground_snap {
                    start.y += snap.clamp(-MAX_GROUND_SNAP, MAX_GROUND_SNAP);
                }
                let filter = SpatialQueryFilter::default().with_excluded_entities([entity]);
                transform.translation = move_and_slide(&spatial, shape, &filter, start, &mut v, dt);
            }
            ParkourMotion::Place(position) => transform.translation = sanitize_vec3(position),
            ParkourMotion::Hold => {}
        }
        velocity.0 = sanitize_vec3(v);
        transform.rotation = Quat::from_rotation_y(tick_input.yaw);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flow_state::FLOW_OVERDRIVE_THRESHOLD;
    use game_core::time::FIXED_TIMESTEP_SECS as DT;

    /// Synthetic character for driving `step_parkour` without physics.
    #[derive(Default)]
    struct Sim {
        state: ParkourState,
        velocity: Vec3,
        buffer: LocomotionBuffer,
        flow: FlowState,
        controller: ParkourController,
        position: Vec3,
        constants: ParkourPhysicsConstants,
    }

    impl Sim {
        fn new(state: ParkourState, velocity: Vec3) -> Self {
            Self {
                state,
                velocity,
                position: Vec3::new(0.0, 0.93, 0.0),
                ..Default::default()
            }
        }

        fn step(&mut self, readings: &ParkourSensorReadings, input: ParkourInput) -> ParkourMotion {
            // Mirror the system: buffers tick in the Input set first.
            self.buffer.tick(
                self.state.is_grounded(),
                DT,
                self.constants.coyote_time_duration,
            );
            let ctx = StepContext {
                readings,
                input,
                constants: &self.constants,
                dt: DT,
                position: self.position,
            };
            let motion = step_parkour(
                &mut ParkourCharacter {
                    state: &mut self.state,
                    velocity: &mut self.velocity,
                    buffer: &mut self.buffer,
                    flow: &mut self.flow,
                    controller: &mut self.controller,
                },
                &ctx,
            );
            self.flow.tick(DT, self.constants.flow_overdrive_multiplier);
            match motion {
                ParkourMotion::Place(p) => self.position = p,
                ParkourMotion::Move { .. } => self.position += self.velocity * DT,
                ParkourMotion::Hold => {}
            }
            motion
        }
    }

    fn grounded() -> ParkourSensorReadings {
        ParkourSensorReadings {
            is_ground_detected: true,
            ground_distance: GROUND_PROBE_HEIGHT + GROUND_HOVER,
            ground_normal: Vec3::Y,
            forward: Vec3::NEG_Z,
            ..Default::default()
        }
    }

    fn airborne() -> ParkourSensorReadings {
        ParkourSensorReadings {
            ground_normal: Vec3::Y,
            forward: Vec3::NEG_Z,
            ..Default::default()
        }
    }

    fn run_forward(sprint: bool) -> ParkourInput {
        ParkourInput {
            move_direction: Vec2::new(0.0, -1.0),
            sprint,
            ..Default::default()
        }
    }

    fn press_jump(mut input: ParkourInput) -> ParkourInput {
        input.jump_pressed = true;
        input
    }

    #[test]
    fn input_from_packet_rotates_and_detects_edges() {
        let packet = PlayerInputPacket {
            movement_vector: Vec2::new(0.0, 1.0),
            yaw: std::f32::consts::FRAC_PI_2,
            actions: InputActions::JUMP | InputActions::SPRINT,
            ..Default::default()
        };
        let input = ParkourInput::from_packet(&packet, None);
        assert!(input.move_direction.abs_diff_eq(Vec2::new(-1.0, 0.0), 1e-5));
        assert!(input.sprint && input.jump_pressed && !input.crouch);
        assert!(input.facing_forward().abs_diff_eq(Vec3::NEG_X, 1e-5));

        let held = ParkourInput::from_packet(&packet, Some(&packet));
        assert!(!held.jump_pressed, "held jump is not a new press");
    }

    #[test]
    fn locomotion_states_follow_input() {
        let mut sim = Sim::new(ParkourState::Idle, Vec3::ZERO);
        sim.step(&grounded(), run_forward(false));
        assert_eq!(sim.state, ParkourState::Walking);
        for _ in 0..20 {
            sim.step(&grounded(), run_forward(true));
        }
        assert_eq!(sim.state, ParkourState::Sprinting);
        assert!((sim.velocity.z - -12.8).abs() < 1e-4);
        let crouch = ParkourInput {
            crouch: true,
            ..Default::default()
        };
        for _ in 0..20 {
            sim.step(&grounded(), crouch);
        }
        assert_eq!(
            sim.state,
            ParkourState::Sliding,
            "crouching at sprint speed slides"
        );
    }

    #[test]
    fn grounded_jump_then_land() {
        let mut sim = Sim::new(ParkourState::Walking, Vec3::new(0.0, 0.0, -5.0));
        let motion = sim.step(&grounded(), press_jump(run_forward(false)));
        assert_eq!(sim.state, ParkourState::JumpAscent);
        assert_eq!(motion, ParkourMotion::Move { ground_snap: None });
        assert_eq!(sim.velocity, Vec3::new(0.0, 9.4, -5.0));

        let mut saw_apex = false;
        for _ in 0..30 {
            sim.step(&airborne(), run_forward(false));
            saw_apex |= sim.state == ParkourState::JumpApex;
        }
        assert!(saw_apex);
        assert_eq!(sim.state, ParkourState::JumpDescent);
        sim.step(&grounded(), run_forward(false));
        assert_eq!(sim.state, ParkourState::Walking);
        assert_eq!(sim.velocity.y, 0.0);
    }

    #[test]
    fn walking_off_a_ledge_allows_coyote_jump() {
        let mut sim = Sim::new(ParkourState::Sprinting, Vec3::new(0.0, 0.0, -12.8));
        sim.step(&grounded(), run_forward(true));
        for _ in 0..5 {
            sim.step(&airborne(), run_forward(true));
        }
        assert_eq!(sim.state, ParkourState::JumpDescent);
        sim.step(&airborne(), press_jump(run_forward(true)));
        assert_eq!(
            sim.state,
            ParkourState::JumpAscent,
            "coyote jump within 150 ms"
        );

        // No second jump in the air.
        for _ in 0..3 {
            sim.step(&airborne(), run_forward(true));
        }
        sim.step(&airborne(), press_jump(run_forward(true)));
        assert_ne!(sim.velocity.y, 9.4);
    }

    #[test]
    fn late_jump_after_walking_off_is_rejected() {
        let mut sim = Sim::new(ParkourState::Sprinting, Vec3::new(0.0, 0.0, -12.8));
        sim.step(&grounded(), run_forward(true));
        for _ in 0..12 {
            sim.step(&airborne(), run_forward(true));
        }
        sim.step(&airborne(), press_jump(run_forward(true)));
        assert_ne!(sim.state, ParkourState::JumpAscent);
    }

    #[test]
    fn buffered_jump_fires_on_landing() {
        let mut sim = Sim::new(ParkourState::JumpDescent, Vec3::new(0.0, -6.0, 0.0));
        sim.controller.airborne_vertical_velocity = -6.0;
        sim.step(&airborne(), press_jump(ParkourInput::default()));
        for _ in 0..4 {
            sim.step(&airborne(), ParkourInput::default());
        }
        sim.step(&grounded(), ParkourInput::default());
        assert!(sim.state.is_grounded(), "landed");
        sim.step(&grounded(), ParkourInput::default());
        assert_eq!(
            sim.state,
            ParkourState::JumpAscent,
            "buffer consumed after landing"
        );
    }

    #[test]
    fn hard_landing_locks_momentum_then_recovers() {
        let mut sim = Sim::new(ParkourState::JumpDescent, Vec3::new(5.0, -20.0, 0.0));
        sim.controller.airborne_vertical_velocity = -20.0;
        sim.step(&grounded(), run_forward(true));
        assert_eq!(sim.state, ParkourState::HardLanding);
        assert_eq!(sim.velocity, Vec3::ZERO);
        for _ in 0..10 {
            sim.step(&grounded(), run_forward(true));
            assert_eq!(sim.velocity, Vec3::ZERO, "locked");
        }
        for _ in 0..20 {
            sim.step(&grounded(), run_forward(true));
        }
        assert!(matches!(
            sim.state,
            ParkourState::Sprinting | ParkourState::Walking
        ));
    }

    #[test]
    fn impact_speed_survives_collision_clipping() {
        // Collide-and-slide zeroed vy on touchdown; the stored airborne
        // velocity still drives the landing classification.
        let mut sim = Sim::new(ParkourState::JumpDescent, Vec3::new(0.0, 0.0, 0.0));
        sim.controller.airborne_vertical_velocity = -30.0;
        sim.step(&grounded(), ParkourInput::default());
        assert_eq!(sim.state, ParkourState::HardLanding);
    }

    #[test]
    fn buffered_slide_turns_landing_into_roll() {
        let mut sim = Sim::new(ParkourState::JumpDescent, Vec3::new(0.0, -20.0, -12.0));
        sim.controller.airborne_vertical_velocity = -20.0;
        let slide = ParkourInput {
            slide_pressed: true,
            ..run_forward(true)
        };
        sim.step(&airborne(), slide);
        sim.step(&grounded(), run_forward(true));
        assert_eq!(sim.state, ParkourState::LandingRoll);
        assert_eq!(sim.velocity, Vec3::new(0.0, 0.0, -12.0), "momentum kept");
        assert!(sim.flow.current_flow >= FLOW_LANDING_ROLL_REWARD - 1.0);
        for _ in 0..40 {
            sim.step(&grounded(), run_forward(true));
        }
        assert_eq!(sim.state, ParkourState::Sprinting);
    }

    fn right_wall() -> ParkourSensorReadings {
        ParkourSensorReadings {
            is_right_wall_detected: true,
            right_wall_normal: Vec3::NEG_X,
            ..airborne()
        }
    }

    #[test]
    fn wall_run_entry_rules() {
        // Too slow.
        let mut slow = Sim::new(ParkourState::JumpDescent, Vec3::new(0.0, -1.0, -6.0));
        slow.step(&right_wall(), ParkourInput::default());
        assert_ne!(slow.state, ParkourState::WallRunRight);

        // Still rising fast after a jump.
        let mut rising = Sim::new(ParkourState::JumpAscent, Vec3::new(0.0, 8.0, -12.0));
        rising.step(&right_wall(), ParkourInput::default());
        assert_ne!(rising.state, ParkourState::WallRunRight);

        // Running head-on into the wall.
        let mut head_on = Sim::new(ParkourState::JumpDescent, Vec3::new(12.0, -1.0, 0.0));
        head_on.step(&right_wall(), ParkourInput::default());
        assert_ne!(head_on.state, ParkourState::WallRunRight);

        // Fast, along the wall, near apex.
        let mut ok = Sim::new(ParkourState::JumpApex, Vec3::new(0.5, 0.5, -12.0));
        ok.step(&right_wall(), ParkourInput::default());
        assert_eq!(ok.state, ParkourState::WallRunRight);
        assert!(ok.velocity.x.abs() < 1e-4, "into-wall motion removed");
        assert!(ok.velocity.y <= 0.0, "gravity replaced by sink");
    }

    #[test]
    fn wall_run_sinks_slowly_and_expires() {
        let mut sim = Sim::new(ParkourState::JumpApex, Vec3::new(0.0, 0.0, -12.0));
        sim.step(&right_wall(), ParkourInput::default());
        assert_eq!(sim.state, ParkourState::WallRunRight);
        let mut ticks = 1;
        while sim.state == ParkourState::WallRunRight {
            sim.step(&right_wall(), ParkourInput::default());
            if sim.state == ParkourState::WallRunRight {
                assert!(sim.velocity.y >= -1.25 - 1e-4, "sink capped at 1.25 m/s");
            }
            ticks += 1;
            assert!(ticks < 400);
        }
        let seconds = ticks as f32 * DT;
        assert!((seconds - 2.2).abs() < 0.05, "detached after {seconds} s");
        assert_eq!(sim.state, ParkourState::JumpDescent);

        // Cooldown blocks immediate re-attachment.
        sim.step(&right_wall(), ParkourInput::default());
        assert_ne!(sim.state, ParkourState::WallRunRight);
    }

    #[test]
    fn wall_kick_impulse_flow_and_stamina() {
        let mut sim = Sim::new(ParkourState::JumpApex, Vec3::new(0.0, 0.0, -12.0));
        sim.step(&right_wall(), ParkourInput::default());
        sim.step(&right_wall(), press_jump(ParkourInput::default()));
        assert_eq!(sim.state, ParkourState::WallKick);
        assert!(
            sim.velocity.abs_diff_eq(Vec3::new(-9.2, 7.8, -11.5), 1e-4),
            "{:?}",
            sim.velocity
        );
        assert!((sim.flow.current_flow - 15.0).abs() < 1e-4);
        assert!((sim.flow.stamina_current - 85.0).abs() < 0.5);

        for _ in 0..12 {
            sim.step(&airborne(), ParkourInput::default());
        }
        assert!(
            sim.state.is_airborne(),
            "kick hands over to airborne states"
        );
    }

    #[test]
    fn wall_kick_denied_without_stamina_unless_overdrive() {
        let mut tired = Sim::new(ParkourState::JumpApex, Vec3::new(0.0, 0.0, -12.0));
        tired.flow.stamina_current = 0.0;
        tired.flow.stamina_regen_rate = 0.0;
        tired.step(&right_wall(), ParkourInput::default());
        tired.step(&right_wall(), press_jump(ParkourInput::default()));
        assert_eq!(tired.state, ParkourState::JumpDescent);
        assert_eq!(tired.flow.current_flow, 0.0);

        let mut flowing = Sim::new(ParkourState::JumpApex, Vec3::new(0.0, 0.0, -12.0));
        flowing.flow.stamina_current = 0.0;
        flowing.flow.add_flow(FLOW_OVERDRIVE_THRESHOLD);
        flowing.step(&right_wall(), ParkourInput::default());
        flowing.step(&right_wall(), press_jump(ParkourInput::default()));
        assert_eq!(flowing.state, ParkourState::WallKick);
    }

    #[test]
    fn flow_multiplier_speeds_up_wall_runs() {
        let mut sim = Sim::new(ParkourState::JumpApex, Vec3::new(0.0, 0.0, -12.8));
        sim.flow.add_flow(80.0);
        sim.flow.tick(DT, 1.25);
        sim.step(&right_wall(), ParkourInput::default());
        for _ in 0..30 {
            sim.step(&right_wall(), ParkourInput::default());
        }
        let planar_speed = planar(sim.velocity).length();
        assert!((planar_speed - 16.0).abs() < 1e-3, "speed {planar_speed}");
    }

    #[test]
    fn airborne_mantle_converges_to_ledge() {
        let target = Vec3::new(0.0, 2.5, -1.1);
        let ledge = ParkourSensorReadings {
            can_ledge_mantle: true,
            mantle_target_position: target,
            ..airborne()
        };
        let mut sim = Sim::new(ParkourState::JumpAscent, Vec3::new(0.0, 6.0, 0.0));
        sim.position = Vec3::new(0.0, 1.2, 0.0);
        sim.step(&ledge, ParkourInput::default());
        assert_eq!(sim.state, ParkourState::LedgeMantle);
        let mut ticks = 1;
        while sim.state == ParkourState::LedgeMantle {
            // Readings no longer matter once the mantle has started.
            sim.step(&airborne(), ParkourInput::default());
            ticks += 1;
            assert!(ticks < 64, "mantle never finished");
        }
        assert_eq!(sim.position, target);
        assert_eq!(sim.state, ParkourState::Idle);
        assert_eq!(sim.velocity, Vec3::ZERO);
    }

    #[test]
    fn vault_arcs_over_obstacle_and_preserves_speed() {
        let low_box = ParkourSensorReadings {
            can_vault_low: true,
            obstacle_distance: 0.6,
            obstacle_top_height: 0.75,
            vault_depth: 0.6,
            ..grounded()
        };
        let mut sim = Sim::new(ParkourState::Sprinting, Vec3::new(0.0, 0.0, -12.8));
        let start_y = sim.position.y;
        sim.step(&low_box, run_forward(true));
        assert_eq!(sim.state, ParkourState::VaultLow);
        let mut peak = start_y;
        let mut ticks = 1;
        while sim.state == ParkourState::VaultLow {
            sim.step(&grounded(), run_forward(true));
            peak = peak.max(sim.position.y);
            ticks += 1;
            assert!(ticks < 64);
        }
        assert!(
            peak - start_y >= 0.75 + 0.15 - 0.02,
            "cleared top: {}",
            peak - start_y
        );
        assert!(
            sim.position.z < -(0.6 + 0.6 + 0.4),
            "landed past the obstacle"
        );
        assert!((sim.velocity.z - -12.8).abs() < 1e-3, "momentum preserved");
        assert!(
            (sim.position.y - start_y).abs() < 1e-3,
            "back at take-off height"
        );
    }

    #[test]
    fn vault_needs_input_toward_obstacle() {
        let low_box = ParkourSensorReadings {
            can_vault_low: true,
            obstacle_distance: 0.6,
            obstacle_top_height: 0.75,
            vault_depth: 0.6,
            ..grounded()
        };
        let mut sim = Sim::new(ParkourState::Walking, Vec3::ZERO);
        sim.step(&low_box, ParkourInput::default());
        assert_ne!(sim.state, ParkourState::VaultLow);
    }

    #[test]
    fn externally_driven_states_hold() {
        for state in [
            ParkourState::WallClimb,
            ParkourState::LedgeGrab,
            ParkourState::ZiplineTraverse,
            ParkourState::GrapplePull,
            ParkourState::Ragdoll,
        ] {
            let mut sim = Sim::new(state, Vec3::new(1.0, 2.0, 3.0));
            assert_eq!(
                sim.step(&airborne(), ParkourInput::default()),
                ParkourMotion::Hold
            );
            assert_eq!(sim.state, state);
            assert_eq!(sim.velocity, Vec3::new(1.0, 2.0, 3.0));
        }
    }
}
