//! Deterministic movement model shared by server authority and client
//! prediction. Both sides run exactly this step per input, so a client that
//! received the same inputs reaches the same position bit for bit.
//!
//! Walk / sprint on the input heading, jumps with gravity, coyote time and
//! a jump buffer, and crouch slides that decay with friction. The small
//! amount of state this needs travels in snapshot `state_flags` (see
//! [`MovementState::pack`]) so reconciliation replays from the same state.
//! Characters driven by the full parkour controller are not moved by it.

use bevy::prelude::*;
use game_core::protocol::{InputActions, PlayerInputPacket};
use game_core::time::FIXED_TIMESTEP_SECS;
use game_world::city::{self, Solid};

pub const WALK_SPEED: f32 = 5.5;
pub const SPRINT_SPEED: f32 = 9.0;
pub const CROUCH_SPEED: f32 = 2.8;
/// Planar speed a slide starts at (or keeps, when already faster).
pub const SLIDE_START_SPEED: f32 = 11.5;
/// Exponential slide friction, 1/s.
pub const SLIDE_FRICTION: f32 = 1.4;
/// A slide ends once it decays below this planar speed.
pub const SLIDE_EXIT_SPEED: f32 = 4.5;
/// Takeoff speed: a 1.4 m apex under [`GRAVITY`].
pub const JUMP_SPEED: f32 = 8.2;
pub const GRAVITY: f32 = 24.0;
/// Ground acceleration towards the wanted speed, m/s² (walk in ~0.08 s).
pub const GROUND_ACCELERATION: f32 = 70.0;
/// Ground braking when slowing or turning, m/s².
pub const GROUND_BRAKING: f32 = 95.0;
/// Planar acceleration available in the air, m/s².
pub const AIR_ACCELERATION: f32 = 22.0;
/// Ticks after leaving the ground during which a jump still works.
pub const COYOTE_TICKS: u8 = 8;
/// Ticks a jump press is remembered before landing.
pub const JUMP_BUFFER_TICKS: u8 = 8;
/// Height of the walkable ground plane.
pub const GROUND_HEIGHT: f32 = 0.0;
/// The body as an upright cylinder: radius and height, metres.
pub const PLAYER_RADIUS: f32 = 0.35;
pub const PLAYER_HEIGHT: f32 = 1.8;
/// Edges this low are walked onto; this far down is walked off.
pub const STEP_HEIGHT: f32 = 0.5;
pub const STEP_DOWN: f32 = 0.4;
/// Highest obstacle a sprint vaults, and a ledge reachable in the air,
/// above the feet.
pub const VAULT_HEIGHT: f32 = 1.4;
pub const LEDGE_REACH: f32 = 1.8;
/// Extra height a mantle clears the edge by, and its forward speed.
const MANTLE_CLEARANCE: f32 = 0.35;
const MANTLE_FORWARD_SPEED: f32 = 3.5;
/// Ticks a mantle carries over the edge without air steering.
const MANTLE_TICKS: u8 = 14;
/// Ticks of wall running per airborne stretch (about 1.1 s).
pub const WALLRUN_TICKS: u8 = 70;
const WALLRUN_MIN_SPEED: f32 = 4.5;
const WALLRUN_SPEED: f32 = 8.0;
const WALLRUN_STICK: f32 = 0.6;
const WALLRUN_MAX_SINK: f32 = -1.5;
const WALLRUN_GRAVITY_SCALE: f32 = 0.25;
/// Wall jump: push off the wall and upward speed; ticks of wall run it costs.
const WALL_JUMP_PUSH: f32 = 6.5;
const WALL_JUMP_SPEED: f32 = 7.8;
const WALL_JUMP_COST: u8 = 20;
/// How close a wall counts as touched, beyond the body radius.
const WALL_TOUCH: f32 = 0.12;

/// Entities moved by [`simulate_movement_step`] on the server.
#[derive(Component, Debug, Clone, Copy, Default, Reflect)]
#[reflect(Component, Default, Debug)]
pub struct NetworkMovement {
    #[reflect(ignore)]
    pub state: MovementState,
}

/// Per-character movement state carried between ticks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MovementState {
    pub grounded: bool,
    pub sliding: bool,
    /// Jump and crouch were held on the previous tick (edge detection).
    pub jump_held: bool,
    pub crouch_held: bool,
    pub coyote_ticks: u8,
    pub jump_buffer_ticks: u8,
    /// Ticks left carrying a mantle over an edge.
    pub mantle_ticks: u8,
    /// Wall-run ticks used since last on the ground.
    pub wall_ticks: u8,
    /// Running along a wall this tick.
    pub on_wall: bool,
}

impl Default for MovementState {
    fn default() -> Self {
        Self {
            grounded: true,
            sliding: false,
            jump_held: false,
            crouch_held: false,
            coyote_ticks: COYOTE_TICKS,
            jump_buffer_ticks: 0,
            mantle_ticks: 0,
            wall_ticks: 0,
            on_wall: false,
        }
    }
}

impl MovementState {
    const GROUNDED: u32 = 1 << 0;
    const SLIDING: u32 = 1 << 1;
    const JUMP_HELD: u32 = 1 << 2;
    const CROUCH_HELD: u32 = 1 << 3;

    /// Packs the state into snapshot `state_flags`.
    pub fn pack(&self) -> u32 {
        let flag = |on: bool, bit: u32| if on { bit } else { 0 };
        flag(self.grounded, Self::GROUNDED)
            | flag(self.sliding, Self::SLIDING)
            | flag(self.jump_held, Self::JUMP_HELD)
            | flag(self.crouch_held, Self::CROUCH_HELD)
            | u32::from(self.coyote_ticks.min(COYOTE_TICKS)) << 8
            | u32::from(self.jump_buffer_ticks.min(JUMP_BUFFER_TICKS)) << 16
            | u32::from(self.mantle_ticks.min(15)) << 4
            | u32::from(self.wall_ticks.min(127)) << 24
            | flag(self.on_wall, 1 << 31)
    }

    /// Inverse of [`Self::pack`]; out-of-range counters are clamped.
    pub fn unpack(bits: u32) -> Self {
        Self {
            grounded: bits & Self::GROUNDED != 0,
            sliding: bits & Self::SLIDING != 0,
            jump_held: bits & Self::JUMP_HELD != 0,
            crouch_held: bits & Self::CROUCH_HELD != 0,
            coyote_ticks: ((bits >> 8) as u8).min(COYOTE_TICKS),
            jump_buffer_ticks: ((bits >> 16) as u8).min(JUMP_BUFFER_TICKS),
            mantle_ticks: ((bits >> 4) & 0xF) as u8,
            wall_ticks: ((bits >> 24) & 0x7F) as u8,
            on_wall: bits & (1 << 31) != 0,
        }
    }
}

/// World-space planar direction of `input` (magnitude <= 1, analog kept).
pub fn movement_direction(input: &PlayerInputPacket) -> Vec3 {
    let p = input.sanitized();
    let local = Vec3::new(p.movement_vector.x, 0.0, -p.movement_vector.y);
    Quat::from_rotation_y(p.yaw) * local
}

/// Advances one fixed tick of movement for `input`.
pub fn simulate_movement_step(
    input: &PlayerInputPacket,
    position: &mut Vec3,
    velocity: &mut Vec3,
    state: &mut MovementState,
) {
    let dt = FIXED_TIMESTEP_SECS;
    let jump = input.has_action(InputActions::JUMP);
    let crouch = input.has_action(InputActions::SLIDE);
    let sprint = input.has_action(InputActions::SPRINT);

    state.jump_buffer_ticks = if jump && !state.jump_held {
        JUMP_BUFFER_TICKS
    } else {
        state.jump_buffer_ticks.saturating_sub(1)
    };
    state.coyote_ticks = if state.grounded {
        COYOTE_TICKS
    } else {
        state.coyote_ticks.saturating_sub(1)
    };

    let direction = movement_direction(input);
    let mut planar = Vec2::new(velocity.x, velocity.z);
    if state.grounded {
        let speed = planar.length();
        let crouch_pressed = crouch && !state.crouch_held;
        if crouch_pressed && !state.sliding && speed > WALK_SPEED + 0.5 {
            state.sliding = true;
            planar = planar.normalize_or_zero() * speed.max(SLIDE_START_SPEED);
        } else if state.sliding {
            if crouch && speed > SLIDE_EXIT_SPEED {
                planar *= (-SLIDE_FRICTION * dt).exp();
            } else {
                state.sliding = false;
            }
        }
        if !state.sliding {
            let speed = if crouch {
                CROUCH_SPEED
            } else if sprint {
                SPRINT_SPEED
            } else {
                WALK_SPEED
            };
            let target = Vec2::new(direction.x, direction.z) * speed;
            // Speeding up uses acceleration, slowing or turning brakes harder.
            let rate = if target.length_squared() > planar.length_squared() + 1e-4
                && target.dot(planar) >= 0.0
            {
                GROUND_ACCELERATION
            } else {
                GROUND_BRAKING
            };
            planar += (target - planar).clamp_length_max(rate * dt);
        }
    } else if state.mantle_ticks == 0 {
        let top = if sprint { SPRINT_SPEED } else { WALK_SPEED };
        let target = Vec2::new(direction.x, direction.z) * top.max(planar.length());
        planar += (target - planar).clamp_length_max(AIR_ACCELERATION * dt);
    }

    let mut solids = Vec::new();
    let reach = 3.0 + Vec2::new(velocity.x, velocity.z).length() * dt;
    city::solids_near(*position, reach, &mut solids);
    let contact = wall_contact(*position, &solids);
    let fresh_jump = jump && !state.jump_held;
    let input_dir = Vec2::new(direction.x, direction.z);

    if state.jump_buffer_ticks > 0 && state.coyote_ticks > 0 {
        velocity.y = JUMP_SPEED;
        state.jump_buffer_ticks = 0;
        state.coyote_ticks = 0;
        state.grounded = false;
        state.sliding = false;
    } else if let (true, false, Some(wall), true) = (
        fresh_jump,
        state.grounded,
        contact,
        state.wall_ticks < WALLRUN_TICKS,
    ) {
        // Wall jump: kick off the wall, keeping speed along it.
        let along = planar - wall.normal * planar.dot(wall.normal);
        planar = along + wall.normal * WALL_JUMP_PUSH;
        velocity.y = WALL_JUMP_SPEED;
        state.jump_buffer_ticks = 0;
        state.mantle_ticks = 0;
        state.wall_ticks = state.wall_ticks.saturating_add(WALL_JUMP_COST);
    }

    // Mantle: sprint into a low obstacle, or reach a ledge in the air.
    let mut wallrunning = false;
    if let (0, Some(wall)) = (state.mantle_ticks, contact) {
        let rise = wall.top - position.y;
        let pushing = input_dir.dot(-wall.normal) > 0.4;
        let vault = state.grounded && sprint && rise <= VAULT_HEIGHT;
        let grab = !state.grounded && rise <= LEDGE_REACH && velocity.y > -6.0;
        if pushing && rise > STEP_HEIGHT && (vault || grab) {
            velocity.y = (2.0 * GRAVITY * (rise + MANTLE_CLEARANCE)).sqrt();
            let forward = planar.dot(-wall.normal).max(MANTLE_FORWARD_SPEED);
            planar = -wall.normal * forward;
            state.mantle_ticks = MANTLE_TICKS;
            // No jump on top of a vault.
            state.coyote_ticks = 0;
            state.jump_buffer_ticks = 0;
            state.grounded = false;
            state.sliding = false;
        } else if !state.grounded
            && sprint
            && state.wall_ticks < WALLRUN_TICKS
            && input_dir.dot(-wall.normal) < 0.9
        {
            // Wall run: glide along the wall with most of gravity held off.
            let along = planar - wall.normal * planar.dot(wall.normal);
            if along.length() > WALLRUN_MIN_SPEED {
                wallrunning = true;
                planar = along.normalize() * along.length().max(WALLRUN_SPEED)
                    - wall.normal * WALLRUN_STICK;
                velocity.y = velocity.y.max(WALLRUN_MAX_SINK);
                state.wall_ticks += 1;
            }
        }
    }

    if !state.grounded {
        let scale = if wallrunning {
            WALLRUN_GRAVITY_SCALE
        } else {
            1.0
        };
        velocity.y -= GRAVITY * scale * dt;
    }
    velocity.x = planar.x;
    velocity.z = planar.y;
    resolve_motion(position, velocity, state, &solids);
    state.mantle_ticks = state.mantle_ticks.saturating_sub(1);
    if state.grounded {
        state.wall_ticks = 0;
        state.mantle_ticks = 0;
    }
    state.on_wall = wallrunning;
    state.jump_held = jump;
    state.crouch_held = crouch;
}

/// A wall the body is touching: outward normal (planar) and its top.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WallContact {
    pub normal: Vec2,
    pub top: f32,
}

/// Whether `solid` blocks a body whose feet are at `feet` (too tall to
/// step onto, and not entirely overhead).
fn blocks(solid: &Solid, feet: f32, step: f32) -> bool {
    solid.max.y > feet + step && solid.min.y < feet + PLAYER_HEIGHT
}

/// How high an edge the body steps or lands onto: a full step on the
/// ground and while falling (a jump that just reaches a ledge lands on
/// it), almost nothing while rising.
fn step_allowance(grounded: bool, vertical: f32) -> f32 {
    if grounded || vertical <= 0.0 {
        STEP_HEIGHT
    } else {
        0.02
    }
}

/// The nearest wall within touching distance of the body.
pub fn wall_contact(position: Vec3, solids: &[Solid]) -> Option<WallContact> {
    let mut best: Option<(f32, WallContact)> = None;
    for s in solids.iter().filter(|s| blocks(s, position.y, STEP_HEIGHT)) {
        let d = s.planar_distance(position);
        if d <= 0.0 || d > PLAYER_RADIUS + WALL_TOUCH {
            continue;
        }
        let nearest = Vec2::new(
            position.x.clamp(s.min.x, s.max.x),
            position.z.clamp(s.min.z, s.max.z),
        );
        let normal = (Vec2::new(position.x, position.z) - nearest).normalize_or_zero();
        if normal != Vec2::ZERO && best.is_none_or(|(b, _)| d < b) {
            best = Some((
                d,
                WallContact {
                    normal,
                    top: s.max.y,
                },
            ));
        }
    }
    best.map(|(_, c)| c)
}

/// Moves the body by its velocity for one tick against the city: walls
/// push it out (and absorb the velocity into them, except mid-mantle),
/// low edges are stepped onto, roofs and ledges hold it up, ceilings stop
/// it, and the map edge keeps it in.
fn resolve_motion(
    position: &mut Vec3,
    velocity: &mut Vec3,
    state: &mut MovementState,
    solids: &[Solid],
) {
    let dt = FIXED_TIMESTEP_SECS;
    let r = PLAYER_RADIUS;
    let prev = *position;
    let allowance = step_allowance(state.grounded, velocity.y);
    let mut next = prev + Vec3::new(velocity.x, 0.0, velocity.z) * dt;
    for _ in 0..2 {
        for s in solids.iter().filter(|s| blocks(s, prev.y, allowance)) {
            let (x0, x1) = (s.min.x - r, s.max.x + r);
            let (z0, z1) = (s.min.z - r, s.max.z + r);
            if next.x <= x0 || next.x >= x1 || next.z <= z0 || next.z >= z1 {
                continue;
            }
            let exits = [
                (next.x - x0, Vec2::NEG_X),
                (x1 - next.x, Vec2::X),
                (next.z - z0, Vec2::NEG_Y),
                (z1 - next.z, Vec2::Y),
            ];
            let (depth, normal) = exits
                .into_iter()
                .fold(exits[0], |a, b| if b.0 < a.0 { b } else { a });
            next.x += normal.x * depth;
            next.z += normal.y * depth;
            if state.mantle_ticks == 0 {
                let planar = Vec2::new(velocity.x, velocity.z);
                let into = planar.dot(normal);
                if into < 0.0 {
                    velocity.x -= normal.x * into;
                    velocity.z -= normal.y * into;
                }
            }
        }
    }

    next.y = prev.y + velocity.y * dt;
    let feet_room = r * 0.5;
    let under = |s: &Solid| {
        next.x > s.min.x - feet_room
            && next.x < s.max.x + feet_room
            && next.z > s.min.z - feet_room
            && next.z < s.max.z + feet_room
    };
    let support = solids
        .iter()
        .filter(|s| under(s) && s.max.y <= prev.y + allowance)
        .map(|s| s.max.y)
        .fold(GROUND_HEIGHT, f32::max);
    if velocity.y > 0.0 {
        let ceiling = solids
            .iter()
            .filter(|s| under(s) && s.min.y >= prev.y + PLAYER_HEIGHT - 0.05)
            .map(|s| s.min.y)
            .fold(f32::INFINITY, f32::min);
        if next.y + PLAYER_HEIGHT > ceiling {
            next.y = ceiling - PLAYER_HEIGHT;
            velocity.y = 0.0;
        }
    }
    if next.y <= support && velocity.y <= 0.0 {
        next.y = support;
        velocity.y = 0.0;
        state.grounded = true;
    } else if state.grounded && velocity.y <= 0.0 && support >= prev.y - STEP_DOWN {
        // Walk down kerbs and steps instead of hopping off them.
        next.y = support;
        velocity.y = 0.0;
        state.grounded = true;
    } else {
        state.grounded = false;
    }

    // The map edge: nothing leaves the city.
    let limit = city::MAP_HALF - r;
    if next.x.abs() > limit {
        next.x = next.x.clamp(-limit, limit);
        velocity.x = 0.0;
    }
    if next.z.abs() > limit {
        next.z = next.z.clamp(-limit, limit);
        velocity.z = 0.0;
    }
    *position = next;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::FRAC_PI_2;

    fn packet(movement: Vec2, yaw: f32, actions: u32) -> PlayerInputPacket {
        PlayerInputPacket {
            movement_vector: movement,
            yaw,
            actions,
            ..Default::default()
        }
    }

    struct Body {
        pos: Vec3,
        vel: Vec3,
        state: MovementState,
    }

    impl Body {
        fn new() -> Self {
            Self {
                pos: Vec3::ZERO,
                vel: Vec3::ZERO,
                state: MovementState::default(),
            }
        }

        fn step(&mut self, input: &PlayerInputPacket) {
            simulate_movement_step(input, &mut self.pos, &mut self.vel, &mut self.state);
        }

        fn run(&mut self, input: &PlayerInputPacket, ticks: usize) {
            for _ in 0..ticks {
                self.step(input);
            }
        }
    }

    fn at(pos: Vec3) -> Body {
        Body {
            pos,
            vel: Vec3::ZERO,
            state: MovementState::default(),
        }
    }

    /// Yaw that faces `dir` on the ground plane.
    fn facing(dir: Vec3) -> f32 {
        f32::atan2(-dir.x, -dir.z)
    }

    fn east_district_tower() -> Solid {
        *city::district_solids()
            .iter()
            .filter(|s| matches!(s.kind, city::SolidKind::Building { .. }) && s.min.x > 0.0)
            .max_by(|a, b| a.size().z.total_cmp(&b.size().z))
            .expect("towers line the avenue")
    }

    #[test]
    fn walls_stop_the_body_and_kerbs_are_stepped_onto() {
        let tower = east_district_tower();
        let z = tower.center().z;
        let mut body = at(Vec3::new(0.0, 0.0, z));
        body.run(&packet(Vec2::Y, facing(Vec3::X), InputActions::SPRINT), 160);
        assert!(
            (body.pos.x - (tower.min.x - PLAYER_RADIUS)).abs() < 1e-3,
            "stopped at the facade: {}",
            body.pos
        );
        assert!((body.pos.y - 0.2).abs() < 1e-4, "standing on the pavement");
        assert!(body.state.grounded);
        assert!(body.vel.x.abs() < 1e-4, "no speed into the wall");
    }

    #[test]
    fn roofs_hold_the_body_up() {
        let roof = city::block_solids(5, 5)
            .into_iter()
            .find(|s| matches!(s.kind, city::SolidKind::Building { .. }))
            .expect("a tower");
        let mut body = at(Vec3::new(
            roof.min.x + 1.0,
            roof.max.y + 6.0,
            roof.min.z + 1.0,
        ));
        body.state.grounded = false;
        body.run(&packet(Vec2::ZERO, 0.0, 0), 120);
        assert!(body.state.grounded);
        assert!(
            (body.pos.y - roof.max.y).abs() < 1e-4,
            "{} vs {}",
            body.pos.y,
            roof.max.y
        );
    }

    #[test]
    fn a_jump_that_just_reaches_a_ledge_lands_on_it() {
        let roof = city::block_solids(5, 5)
            .into_iter()
            .find(|s| matches!(s.kind, city::SolidKind::Building { .. }))
            .expect("a tower");
        let mut body = at(Vec3::new(
            roof.min.x - 0.6,
            roof.max.y - 0.3,
            roof.center().z,
        ));
        body.state.grounded = false;
        body.vel = Vec3::new(6.0, -0.5, 0.0);
        body.run(&packet(Vec2::Y, facing(Vec3::X), 0), 12);
        assert!(body.state.grounded, "landed: {}", body.pos);
        assert!(
            (body.pos.y - roof.max.y).abs() < 1e-4,
            "on the roof: {}",
            body.pos
        );
    }

    #[test]
    fn walking_off_a_roof_falls() {
        let roof = city::block_solids(5, 5)
            .into_iter()
            .find(|s| matches!(s.kind, city::SolidKind::Building { .. }))
            .expect("a tower");
        let mut body = at(Vec3::new(roof.max.x - 1.0, roof.max.y, roof.center().z));
        body.run(&packet(Vec2::Y, facing(Vec3::X), 0), 40);
        assert!(body.pos.y < roof.max.y - 1.0, "fell off the edge");
    }

    #[test]
    fn sprinting_vaults_street_props() {
        let mut checked = 0;
        for (cx, cz) in [(5, 5), (6, 5), (5, 6), (-7, 3), (9, -9)] {
            for prop in city::block_solids(cx, cz)
                .into_iter()
                .filter(|s| s.kind == city::SolidKind::Prop && s.size().x > 1.5)
            {
                // Approach along X from the road side of the pavement.
                let start = Vec3::new(prop.min.x - 4.0, 0.2, prop.center().z);
                let mut solids = Vec::new();
                city::solids_near(start, 1.0, &mut solids);
                if solids
                    .iter()
                    .any(|s| s.kind != city::SolidKind::Sidewalk && s != &prop)
                {
                    continue;
                }
                let mut body = at(start);
                let mut highest: f32 = 0.0;
                for _ in 0..90 {
                    body.step(&packet(Vec2::Y, facing(Vec3::X), InputActions::SPRINT));
                    highest = highest.max(body.pos.y);
                }
                assert!(highest >= prop.max.y - 1e-3, "climbed over: {highest}");
                assert!(body.pos.x > prop.min.x, "kept going: {}", body.pos);
                checked += 1;
            }
        }
        assert!(checked > 0, "found props to vault");
    }

    #[test]
    fn wall_running_holds_off_the_fall() {
        let tower = east_district_tower();
        let start = Vec3::new(tower.min.x - PLAYER_RADIUS - 0.05, 3.0, tower.max.z - 1.0);
        let run = |wall: bool| {
            let x = if wall { start.x } else { 0.0 };
            let mut body = at(Vec3::new(x, start.y, start.z));
            body.state.grounded = false;
            body.vel = Vec3::new(0.0, 2.0, -SPRINT_SPEED);
            let mut ran = false;
            for _ in 0..25 {
                body.step(&packet(Vec2::Y, 0.0, InputActions::SPRINT));
                ran |= body.state.on_wall;
            }
            (body.pos.y, ran)
        };
        let (with_wall, ran) = run(true);
        let (free_fall, _) = run(false);
        assert!(ran, "ran along the wall");
        assert!(with_wall > free_fall + 0.5, "{with_wall} vs {free_fall}");
    }

    #[test]
    fn nothing_leaves_the_map() {
        let mut body = at(Vec3::new(city::MAP_HALF - 30.0, 0.0, 60.0));
        body.run(&packet(Vec2::Y, facing(Vec3::X), InputActions::SPRINT), 600);
        assert!(
            body.pos.x <= city::MAP_HALF - PLAYER_RADIUS + 1e-3,
            "{}",
            body.pos
        );
        assert!(body.pos.x > city::MAP_HALF - 2.0, "reached the barrier");
    }

    #[test]
    fn forward_is_negative_z_and_yaw_turns_left() {
        assert!(movement_direction(&packet(Vec2::Y, 0.0, 0)).distance(Vec3::NEG_Z) < 1e-6);
        assert!(movement_direction(&packet(Vec2::X, 0.0, 0)).distance(Vec3::X) < 1e-6);
        assert!(movement_direction(&packet(Vec2::Y, FRAC_PI_2, 0)).distance(Vec3::NEG_X) < 1e-6);
    }

    #[test]
    fn ground_speed_ramps_up_to_sprint_and_brakes_to_a_stop() {
        let sprint = packet(Vec2::Y, 0.0, InputActions::SPRINT);
        let mut body = Body::new();
        body.step(&sprint);
        assert!(
            (body.vel.z + GROUND_ACCELERATION / 64.0).abs() < 1e-5,
            "accelerates instead of snapping"
        );
        body.run(&sprint, 12);
        assert_eq!(
            body.vel,
            Vec3::new(0.0, 0.0, -SPRINT_SPEED),
            "top speed in ~0.2 s"
        );
        body.run(&packet(Vec2::ZERO, 0.0, 0), 7);
        assert_eq!(body.vel, Vec3::ZERO, "brakes to a stop in ~0.1 s");
        let mut body = Body::new();
        body.run(&packet(Vec2::new(0.0, 0.5), 0.0, 0), 12);
        assert!(
            (body.vel.z + 0.5 * WALK_SPEED).abs() < 1e-5,
            "analog stick keeps magnitude"
        );
    }

    #[test]
    fn jump_rises_lands_and_needs_a_fresh_press() {
        let mut body = Body::new();
        let hold_jump = packet(Vec2::ZERO, 0.0, InputActions::JUMP);
        body.step(&hold_jump);
        assert!(body.pos.y > 0.0 && !body.state.grounded);
        let mut apex: f32 = 0.0;
        for _ in 0..120 {
            body.step(&hold_jump);
            apex = apex.max(body.pos.y);
        }
        let expected = JUMP_SPEED * JUMP_SPEED / (2.0 * GRAVITY);
        assert!((apex - expected).abs() < 0.15, "apex {apex} vs {expected}");
        assert!(body.state.grounded && body.pos.y == GROUND_HEIGHT);
        body.step(&hold_jump);
        assert!(body.state.grounded, "holding jump does not bunny-hop");
    }

    #[test]
    fn coyote_time_and_jump_buffer() {
        // Buffer: a press a few ticks before landing still jumps on touchdown.
        let mut body = Body::new();
        body.step(&packet(Vec2::ZERO, 0.0, InputActions::JUMP));
        body.run(&packet(Vec2::ZERO, 0.0, 0), 200);
        let mut landed_with_buffer = false;
        body.step(&packet(Vec2::ZERO, 0.0, InputActions::JUMP));
        // Fall until just above the ground, then press jump before landing.
        for _ in 0..200 {
            if body.vel.y < 0.0 && body.pos.y < 0.5 {
                break;
            }
            body.step(&packet(Vec2::ZERO, 0.0, 0));
        }
        assert!(
            !body.state.grounded && body.vel.y < 0.0,
            "falling near the ground"
        );
        let press = packet(Vec2::ZERO, 0.0, InputActions::JUMP);
        body.step(&press);
        for _ in 0..JUMP_BUFFER_TICKS {
            body.step(&press);
            if body.vel.y > 0.0 {
                landed_with_buffer = true;
                break;
            }
        }
        assert!(landed_with_buffer, "buffered jump fires on landing");

        // Coyote: shortly after leaving the ground a jump still works.
        let mut state = MovementState {
            grounded: false,
            coyote_ticks: 3,
            ..MovementState::default()
        };
        let (mut pos, mut vel) = (Vec3::Y * 2.0, Vec3::ZERO);
        simulate_movement_step(
            &packet(Vec2::ZERO, 0.0, InputActions::JUMP),
            &mut pos,
            &mut vel,
            &mut state,
        );
        assert!(vel.y > 0.0, "coyote jump");
        let mut late = MovementState {
            grounded: false,
            coyote_ticks: 0,
            ..MovementState::default()
        };
        let (mut pos, mut vel) = (Vec3::Y * 2.0, Vec3::ZERO);
        simulate_movement_step(
            &packet(Vec2::ZERO, 0.0, InputActions::JUMP),
            &mut pos,
            &mut vel,
            &mut late,
        );
        assert!(vel.y < 0.0, "no jump after coyote time");
    }

    #[test]
    fn sprint_slide_boosts_then_decays_and_ends() {
        let mut body = Body::new();
        body.run(&packet(Vec2::Y, 0.0, InputActions::SPRINT), 16);
        let slide = packet(Vec2::Y, 0.0, InputActions::SPRINT | InputActions::SLIDE);
        body.step(&slide);
        assert!(body.state.sliding);
        let start = body.vel.length();
        assert!(start >= SLIDE_START_SPEED - 1e-3);
        body.run(&slide, 32);
        assert!(body.vel.length() < start, "friction decays the slide");
        body.run(&slide, 400);
        assert!(!body.state.sliding, "slide ends below exit speed");
        assert!((body.vel.length() - CROUCH_SPEED).abs() < 1e-3);
        // Crouching from a standstill walks slowly instead of sliding.
        let mut body = Body::new();
        body.run(&packet(Vec2::Y, 0.0, InputActions::SLIDE), 3);
        assert!(!body.state.sliding);
        assert!((body.vel.length() - CROUCH_SPEED).abs() < 1e-3);
    }

    #[test]
    fn state_round_trips_through_flags() {
        let state = MovementState {
            grounded: false,
            sliding: true,
            jump_held: true,
            crouch_held: false,
            coyote_ticks: 5,
            jump_buffer_ticks: 7,
            mantle_ticks: 9,
            wall_ticks: 66,
            on_wall: true,
        };
        assert_eq!(MovementState::unpack(state.pack()), state);
        let clamped = MovementState::unpack(u32::MAX);
        assert!(clamped.coyote_ticks <= COYOTE_TICKS);
        assert!(clamped.jump_buffer_ticks <= JUMP_BUFFER_TICKS);
    }

    #[test]
    fn hostile_input_is_sanitized() {
        let mut body = Body::new();
        body.step(&packet(Vec2::new(f32::NAN, 50.0), f32::INFINITY, u32::MAX));
        assert!(body.pos.is_finite());
        assert!(
            Vec2::new(body.vel.x, body.vel.z).length() <= SLIDE_START_SPEED + 1e-4,
            "magnitude clamped"
        );
    }
}
