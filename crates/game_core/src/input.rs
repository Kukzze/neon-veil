//! Input state shared between the input, camera and simulation crates.
//!
//! [`ViewAngles`] is the single owner of where a player is looking: the input
//! crate writes it, the camera reads it, and it is copied into every
//! [`PlayerInputPacket`]. [`PlayerInputState`] carries packets from whoever
//! produces them (local input or the network) to the simulation, so local
//! and remote characters consume identical input.

use crate::math::{sanitize_f32, wrap_angle_radians};
use crate::protocol::PlayerInputPacket;
use bevy::prelude::*;

/// Pitch limit, just short of straight up/down to avoid gimbal inversion.
pub const MAX_PITCH_RADIANS: f32 = 89.0 * std::f32::consts::PI / 180.0;

/// Clamps pitch to `[-89, 89]` degrees; non-finite input becomes `0`.
#[inline]
pub fn clamp_pitch(pitch: f32) -> f32 {
    sanitize_f32(pitch).clamp(-MAX_PITCH_RADIANS, MAX_PITCH_RADIANS)
}

/// Where a player is looking, in radians. Yaw 0 faces `-Z`; positive pitch
/// looks up.
#[derive(Component, Reflect, Debug, Default, Clone, Copy, PartialEq)]
#[reflect(Component, Default, Debug, PartialEq)]
pub struct ViewAngles {
    pub yaw: f32,
    pub pitch: f32,
}

impl ViewAngles {
    pub fn new(yaw: f32, pitch: f32) -> Self {
        Self {
            yaw: wrap_angle_radians(yaw),
            pitch: clamp_pitch(pitch),
        }
    }

    /// Rotates the view, wrapping yaw and clamping pitch.
    pub fn rotate(&mut self, delta_yaw: f32, delta_pitch: f32) {
        *self = Self::new(
            self.yaw + sanitize_f32(delta_yaw),
            self.pitch + sanitize_f32(delta_pitch),
        );
    }

    /// Yaw-then-pitch rotation (`EulerRot::YXZ`, no roll).
    pub fn rotation(&self) -> Quat {
        Quat::from_euler(EulerRot::YXZ, self.yaw, self.pitch, 0.0)
    }

    /// Unit look direction.
    pub fn forward(&self) -> Vec3 {
        self.rotation() * Vec3::NEG_Z
    }

    /// View angles that look along `direction` (zero direction -> default).
    pub fn looking_along(direction: Vec3) -> Self {
        let Some(d) = direction.try_normalize() else {
            return Self::default();
        };
        Self::new((-d.x).atan2(-d.z), d.y.clamp(-1.0, 1.0).asin())
    }
}

/// Latest input packets for one character.
#[derive(Component, Debug, Default, Clone, PartialEq)]
pub struct PlayerInputState {
    latest: Option<PlayerInputPacket>,
    previous: Option<PlayerInputPacket>,
    fresh: bool,
}

impl PlayerInputState {
    /// Records a new packet; the previous latest becomes `previous`.
    pub fn push(&mut self, packet: PlayerInputPacket) {
        self.previous = self.latest.replace(packet);
        self.fresh = true;
    }

    pub fn latest(&self) -> Option<&PlayerInputPacket> {
        self.latest.as_ref()
    }

    /// Returns the newest packet and its predecessor once per push, so a
    /// press edge is never applied twice when no new packet arrives.
    pub fn take_fresh(&mut self) -> Option<(PlayerInputPacket, Option<PlayerInputPacket>)> {
        if !self.fresh {
            return None;
        }
        self.fresh = false;
        self.latest.map(|latest| (latest, self.previous))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{InputActions, NetworkTick};
    use std::f32::consts::{FRAC_PI_2, PI};

    #[test]
    fn pitch_is_clamped_short_of_vertical() {
        assert!((clamp_pitch(PI) - MAX_PITCH_RADIANS).abs() < 1e-6);
        assert!((clamp_pitch(-PI) + MAX_PITCH_RADIANS).abs() < 1e-6);
        assert_eq!(clamp_pitch(0.3), 0.3);
        assert_eq!(clamp_pitch(f32::NAN), 0.0);
        assert!((MAX_PITCH_RADIANS.to_degrees() - 89.0).abs() < 1e-4);
    }

    #[test]
    fn rotate_wraps_yaw_and_clamps_pitch() {
        let mut v = ViewAngles::default();
        v.rotate(3.0 * PI / 2.0, 10.0);
        assert!((v.yaw - -FRAC_PI_2).abs() < 1e-5);
        assert!((v.pitch - MAX_PITCH_RADIANS).abs() < 1e-6);
        v.rotate(f32::NAN, f32::INFINITY);
        assert!(v.yaw.is_finite() && v.pitch.is_finite());
    }

    #[test]
    fn forward_and_looking_along_are_inverse() {
        assert!(ViewAngles::default()
            .forward()
            .abs_diff_eq(Vec3::NEG_Z, 1e-6));
        let left = ViewAngles::new(FRAC_PI_2, 0.0);
        assert!(left.forward().abs_diff_eq(Vec3::NEG_X, 1e-6));
        let up = ViewAngles::new(0.0, 0.5);
        assert!(up.forward().y > 0.0);

        for v in [
            ViewAngles::new(0.7, -0.3),
            ViewAngles::new(-2.5, 1.2),
            ViewAngles::new(3.0, 0.0),
        ] {
            let back = ViewAngles::looking_along(v.forward());
            assert!((back.yaw - v.yaw).abs() < 1e-4 && (back.pitch - v.pitch).abs() < 1e-4);
        }
        assert_eq!(ViewAngles::looking_along(Vec3::ZERO), ViewAngles::default());
    }

    #[test]
    fn input_state_hands_out_each_packet_once() {
        let mut state = PlayerInputState::default();
        assert!(state.take_fresh().is_none());

        let first = PlayerInputPacket {
            tick: NetworkTick(1),
            actions: InputActions::JUMP,
            ..Default::default()
        };
        state.push(first);
        assert_eq!(state.take_fresh(), Some((first, None)));
        assert!(state.take_fresh().is_none(), "not fresh twice");

        let second = PlayerInputPacket {
            tick: NetworkTick(2),
            ..Default::default()
        };
        state.push(second);
        assert_eq!(state.take_fresh(), Some((second, Some(first))));
        assert_eq!(state.latest(), Some(&second));
    }
}
