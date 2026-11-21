//! Deterministic movement model shared by server authority and client
//! prediction. Both sides run exactly this step per input, so a client that
//! received the same inputs reaches the same position bit for bit.
//!
//! The model is planar (walk / sprint on the input heading). Characters
//! driven by the full parkour controller are not moved by it.

use bevy::prelude::*;
use game_core::protocol::{InputActions, PlayerInputPacket};
use game_core::time::FIXED_TIMESTEP_SECS;

pub const WALK_SPEED: f32 = 5.5;
pub const SPRINT_SPEED: f32 = 13.5;

/// Entities moved by [`simulate_movement_step`] on the server.
#[derive(Component, Debug, Clone, Copy, Default, Reflect)]
#[reflect(Component, Default, Debug)]
pub struct NetworkMovement;

/// World-space planar direction of `input` (magnitude <= 1, analog kept).
pub fn movement_direction(input: &PlayerInputPacket) -> Vec3 {
    let p = input.sanitized();
    let local = Vec3::new(p.movement_vector.x, 0.0, -p.movement_vector.y);
    Quat::from_rotation_y(p.yaw) * local
}

/// Advances one fixed tick of movement for `input`.
pub fn simulate_movement_step(input: &PlayerInputPacket, position: &mut Vec3, velocity: &mut Vec3) {
    let speed = if input.has_action(InputActions::SPRINT) {
        SPRINT_SPEED
    } else {
        WALK_SPEED
    };
    let planar = movement_direction(input) * speed;
    velocity.x = planar.x;
    velocity.z = planar.z;
    velocity.y = 0.0;
    *position += *velocity * FIXED_TIMESTEP_SECS;
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

    #[test]
    fn forward_is_negative_z_and_yaw_turns_left() {
        assert!(movement_direction(&packet(Vec2::Y, 0.0, 0)).distance(Vec3::NEG_Z) < 1e-6);
        assert!(movement_direction(&packet(Vec2::X, 0.0, 0)).distance(Vec3::X) < 1e-6);
        assert!(movement_direction(&packet(Vec2::Y, FRAC_PI_2, 0)).distance(Vec3::NEG_X) < 1e-6);
    }

    #[test]
    fn sprint_and_walk_distances_per_tick() {
        let (mut pos, mut vel) = (Vec3::ZERO, Vec3::ZERO);
        simulate_movement_step(
            &packet(Vec2::Y, 0.0, InputActions::SPRINT),
            &mut pos,
            &mut vel,
        );
        assert!((pos.z + SPRINT_SPEED / 64.0).abs() < 1e-6);
        assert_eq!(vel, Vec3::new(0.0, 0.0, -SPRINT_SPEED));
        let (mut pos, mut vel) = (Vec3::ZERO, Vec3::ZERO);
        simulate_movement_step(&packet(Vec2::new(0.0, 0.5), 0.0, 0), &mut pos, &mut vel);
        assert!(
            (pos.z + 0.5 * WALK_SPEED / 64.0).abs() < 1e-6,
            "analog stick keeps magnitude"
        );
    }

    #[test]
    fn hostile_input_is_sanitized() {
        let (mut pos, mut vel) = (Vec3::ZERO, Vec3::ZERO);
        simulate_movement_step(
            &packet(Vec2::new(f32::NAN, 50.0), f32::INFINITY, u32::MAX),
            &mut pos,
            &mut vel,
        );
        assert!(pos.is_finite());
        assert!(
            vel.length() <= SPRINT_SPEED + 1e-4,
            "magnitude clamped to 1"
        );
    }
}
