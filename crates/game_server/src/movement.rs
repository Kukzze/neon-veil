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

pub const WALK_SPEED: f32 = 5.5;
pub const SPRINT_SPEED: f32 = 13.5;
pub const CROUCH_SPEED: f32 = 2.8;
/// Planar speed a slide starts at (or keeps, when already faster).
pub const SLIDE_START_SPEED: f32 = 16.5;
/// Exponential slide friction, 1/s.
pub const SLIDE_FRICTION: f32 = 1.4;
/// A slide ends once it decays below this planar speed.
pub const SLIDE_EXIT_SPEED: f32 = 6.0;
/// Takeoff speed: a 1.4 m apex under [`GRAVITY`].
pub const JUMP_SPEED: f32 = 8.2;
pub const GRAVITY: f32 = 24.0;
/// Planar acceleration available in the air, m/s².
pub const AIR_ACCELERATION: f32 = 22.0;
/// Ticks after leaving the ground during which a jump still works.
pub const COYOTE_TICKS: u8 = 8;
/// Ticks a jump press is remembered before landing.
pub const JUMP_BUFFER_TICKS: u8 = 8;
/// Height of the walkable ground plane.
pub const GROUND_HEIGHT: f32 = 0.0;

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
            planar = Vec2::new(direction.x, direction.z) * speed;
        }
    } else {
        let top = if sprint { SPRINT_SPEED } else { WALK_SPEED };
        let target = Vec2::new(direction.x, direction.z) * top.max(planar.length());
        planar += (target - planar).clamp_length_max(AIR_ACCELERATION * dt);
    }

    if state.jump_buffer_ticks > 0 && state.coyote_ticks > 0 {
        velocity.y = JUMP_SPEED;
        state.jump_buffer_ticks = 0;
        state.coyote_ticks = 0;
        state.grounded = false;
        state.sliding = false;
    }
    if !state.grounded {
        velocity.y -= GRAVITY * dt;
    }
    velocity.x = planar.x;
    velocity.z = planar.y;
    *position += *velocity * dt;
    if position.y <= GROUND_HEIGHT && velocity.y <= 0.0 {
        position.y = GROUND_HEIGHT;
        velocity.y = 0.0;
        state.grounded = true;
    } else {
        state.grounded = false;
    }
    state.jump_held = jump;
    state.crouch_held = crouch;
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

    #[test]
    fn forward_is_negative_z_and_yaw_turns_left() {
        assert!(movement_direction(&packet(Vec2::Y, 0.0, 0)).distance(Vec3::NEG_Z) < 1e-6);
        assert!(movement_direction(&packet(Vec2::X, 0.0, 0)).distance(Vec3::X) < 1e-6);
        assert!(movement_direction(&packet(Vec2::Y, FRAC_PI_2, 0)).distance(Vec3::NEG_X) < 1e-6);
    }

    #[test]
    fn sprint_and_walk_distances_per_tick() {
        let mut body = Body::new();
        body.step(&packet(Vec2::Y, 0.0, InputActions::SPRINT));
        assert!((body.pos.z + SPRINT_SPEED / 64.0).abs() < 1e-6);
        assert_eq!(body.vel, Vec3::new(0.0, 0.0, -SPRINT_SPEED));
        let mut body = Body::new();
        body.step(&packet(Vec2::new(0.0, 0.5), 0.0, 0));
        assert!(
            (body.pos.z + 0.5 * WALK_SPEED / 64.0).abs() < 1e-6,
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
        body.run(&packet(Vec2::Y, 0.0, InputActions::SPRINT), 4);
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
