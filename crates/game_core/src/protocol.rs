//! Wire protocol shared by client and dedicated server.
//!
//! Messages are serde types encoded with `bitcode` via [`encode`] / [`decode`].

use crate::identifiers::{NetworkEntityId, PlayerId};
use crate::math::{sanitize_f32, sanitize_vec3, wrap_angle_radians};
use bevy::math::{Vec2, Vec3};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::f32::consts::FRAC_PI_2;
use thiserror::Error;

pub use crate::time::NetworkTick;

/// Bumped on every incompatible change to the message layout.
/// v2: added `InputActions::MELEE_HEAVY`. v3: added `InputActions::RELOAD`.
pub const PROTOCOL_VERSION: u32 = 5;

/// Largest player profile (look and player card) a client may send, bytes.
pub const MAX_PROFILE_BYTES: usize = 8 * 1024;

/// Places the server may respawn a player at, by index: the avenue start
/// and the checkpoint before each Night One encounter.
pub const RESPAWN_CHECKPOINTS: [Vec3; 5] = [
    Vec3::new(0.0, 0.0, 0.0),
    Vec3::new(0.0, 0.0, -40.0),
    Vec3::new(0.0, 0.0, -108.0),
    Vec3::new(0.0, 0.0, -172.0),
    Vec3::new(0.0, 0.0, -208.0),
];

/// Bit flags carried in [`PlayerInputPacket::actions`].
pub struct InputActions;

impl InputActions {
    pub const JUMP: u32 = 1 << 0;
    pub const SLIDE: u32 = 1 << 1;
    pub const WALL_RUN: u32 = 1 << 2;
    pub const SPRINT: u32 = 1 << 3;
    pub const FIRE: u32 = 1 << 4;
    pub const PARRY: u32 = 1 << 5;
    pub const MELEE_HEAVY: u32 = 1 << 6;
    pub const RELOAD: u32 = 1 << 7;
    /// Union of every defined flag; undefined bits are stripped on sanitize.
    pub const ALL: u32 = Self::JUMP
        | Self::SLIDE
        | Self::WALL_RUN
        | Self::SPRINT
        | Self::FIRE
        | Self::PARRY
        | Self::MELEE_HEAVY
        | Self::RELOAD;
}

/// One tick of player input, sent client -> server.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct PlayerInputPacket {
    pub tick: NetworkTick,
    /// Planar movement intent; magnitude at most `1.0` after sanitizing.
    pub movement_vector: Vec2,
    /// Camera pitch in radians, within `[-PI/2, PI/2]` after sanitizing.
    pub pitch: f32,
    /// Camera yaw in radians, within `[-PI, PI)` after sanitizing.
    pub yaw: f32,
    /// [`InputActions`] bit flags.
    pub actions: u32,
}

impl PlayerInputPacket {
    #[inline]
    pub const fn has_action(&self, flag: u32) -> bool {
        self.actions & flag != 0
    }

    /// Returns a copy that is safe to feed into the simulation: non-finite
    /// values zeroed, movement clamped to unit length, yaw wrapped, pitch
    /// clamped and unknown action bits removed. Servers must apply this to
    /// every packet received from a client.
    #[must_use]
    pub fn sanitized(&self) -> Self {
        let movement = Vec2::new(
            sanitize_f32(self.movement_vector.x),
            sanitize_f32(self.movement_vector.y),
        )
        .clamp_length_max(1.0);
        Self {
            tick: self.tick,
            movement_vector: movement,
            pitch: sanitize_f32(self.pitch).clamp(-FRAC_PI_2, FRAC_PI_2),
            yaw: wrap_angle_radians(self.yaw),
            actions: self.actions & InputActions::ALL,
        }
    }
}

/// Replicated state of a single entity at one server tick.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct EntityStateSnapshot {
    pub entity_id: NetworkEntityId,
    pub position: Vec3,
    pub velocity: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub state_flags: u32,
}

impl EntityStateSnapshot {
    /// Returns a copy with every spatial field made finite and angles normalized.
    #[must_use]
    pub fn sanitized(&self) -> Self {
        Self {
            position: sanitize_vec3(self.position),
            velocity: sanitize_vec3(self.velocity),
            yaw: wrap_angle_radians(self.yaw),
            pitch: sanitize_f32(self.pitch).clamp(-FRAC_PI_2, FRAC_PI_2),
            ..*self
        }
    }
}

/// Authoritative world state broadcast server -> client.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ServerStateSnapshot {
    pub server_tick: NetworkTick,
    /// Latest client input tick the server applied; clients replay inputs
    /// after this tick during reconciliation.
    pub last_processed_client_tick: NetworkTick,
    pub entities: Vec<EntityStateSnapshot>,
}

/// Messages sent client -> server.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ClientMessage {
    Handshake {
        protocol_version: u32,
    },
    Input(PlayerInputPacket),
    /// Request to use the interactable with the given id.
    Interact(u32),
    /// Client timestamp echoed back in [`ServerMessage::Pong`] for RTT.
    Ping(u64),
    /// Respawn at [`RESPAWN_CHECKPOINTS`]`[index]`; other indices are ignored.
    Respawn(u8),
    /// The sender's profile (opaque to the server, at most
    /// [`MAX_PROFILE_BYTES`]), relayed to every player.
    Profile(Vec<u8>),
}

/// Messages sent server -> client.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ServerMessage {
    HandshakeAck {
        player_id: PlayerId,
        server_tick: NetworkTick,
    },
    Snapshot(ServerStateSnapshot),
    Pong(u64),
    /// Disconnect with a human-readable reason.
    Disconnect(String),
    /// A player's profile, as they sent it.
    Profile {
        player_id: PlayerId,
        data: Vec<u8>,
    },
}

/// Errors produced while encoding or decoding protocol messages.
#[derive(Debug, Error)]
pub enum ProtocolError {
    #[error("failed to encode protocol message: {0}")]
    Encode(#[source] bitcode::Error),
    #[error("failed to decode protocol message: {0}")]
    Decode(#[source] bitcode::Error),
}

/// Encodes a protocol message into its wire representation.
pub fn encode<T: Serialize>(message: &T) -> Result<Vec<u8>, ProtocolError> {
    bitcode::serialize(message).map_err(ProtocolError::Encode)
}

/// Decodes a protocol message from untrusted bytes.
pub fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, ProtocolError> {
    bitcode::deserialize(bytes).map_err(ProtocolError::Decode)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;
    use std::fmt::Debug;
    use uuid::Uuid;

    fn roundtrip<T: Serialize + DeserializeOwned + PartialEq + Debug>(value: &T) {
        let bytes = encode(value).expect("encode");
        let decoded: T = decode(&bytes).expect("decode");
        assert_eq!(&decoded, value, "bitcode roundtrip lost data");

        let json = serde_json::to_string(value).expect("json encode");
        let decoded: T = serde_json::from_str(&json).expect("json decode");
        assert_eq!(&decoded, value, "json roundtrip lost data");
    }

    fn sample_input() -> PlayerInputPacket {
        PlayerInputPacket {
            tick: NetworkTick(9_001),
            movement_vector: Vec2::new(0.6, -0.8),
            pitch: -0.25,
            yaw: 1.75,
            actions: InputActions::JUMP | InputActions::SPRINT | InputActions::PARRY,
        }
    }

    fn sample_snapshot() -> ServerStateSnapshot {
        ServerStateSnapshot {
            server_tick: NetworkTick(9_010),
            last_processed_client_tick: NetworkTick(9_001),
            entities: vec![
                EntityStateSnapshot {
                    entity_id: NetworkEntityId(1),
                    position: Vec3::new(12.5, 3.0, -40.125),
                    velocity: Vec3::new(0.0, -9.81, 25.0),
                    yaw: -3.0,
                    pitch: 0.5,
                    state_flags: 0b1010,
                },
                EntityStateSnapshot {
                    entity_id: NetworkEntityId(u64::MAX),
                    position: Vec3::new(f32::MAX, f32::MIN, f32::MIN_POSITIVE),
                    velocity: Vec3::ZERO,
                    yaw: 0.0,
                    pitch: 0.0,
                    state_flags: u32::MAX,
                },
            ],
        }
    }

    #[test]
    fn input_packet_roundtrips() {
        roundtrip(&sample_input());
    }

    #[test]
    fn snapshot_roundtrips_bit_exact() {
        let snapshot = sample_snapshot();
        roundtrip(&snapshot);

        let decoded: ServerStateSnapshot =
            decode(&encode(&snapshot).expect("encode")).expect("decode");
        for (a, b) in decoded.entities.iter().zip(&snapshot.entities) {
            assert_eq!(a.position.x.to_bits(), b.position.x.to_bits());
            assert_eq!(a.position.y.to_bits(), b.position.y.to_bits());
            assert_eq!(a.position.z.to_bits(), b.position.z.to_bits());
        }
    }

    #[test]
    fn empty_snapshot_roundtrips() {
        roundtrip(&ServerStateSnapshot::default());
    }

    #[test]
    fn every_client_message_roundtrips() {
        for message in [
            ClientMessage::Handshake {
                protocol_version: PROTOCOL_VERSION,
            },
            ClientMessage::Input(sample_input()),
            ClientMessage::Interact(77),
            ClientMessage::Ping(u64::MAX),
        ] {
            roundtrip(&message);
        }
    }

    #[test]
    fn every_server_message_roundtrips() {
        for message in [
            ServerMessage::HandshakeAck {
                player_id: PlayerId(Uuid::new_v4()),
                server_tick: NetworkTick(64),
            },
            ServerMessage::Snapshot(sample_snapshot()),
            ServerMessage::Pong(123),
            ServerMessage::Disconnect("server shutting down".to_owned()),
            ServerMessage::Disconnect(String::new()),
        ] {
            roundtrip(&message);
        }
    }

    #[test]
    fn decode_rejects_garbage_without_panicking() {
        assert!(decode::<ServerMessage>(&[]).is_err());
        assert!(decode::<ServerMessage>(&[0xFF; 3]).is_err());

        let bytes = encode(&ServerMessage::Snapshot(sample_snapshot())).expect("encode");
        assert!(decode::<ServerMessage>(&bytes[..bytes.len() / 2]).is_err());
    }

    #[test]
    fn action_flags_are_distinct_bits() {
        let flags = [
            InputActions::JUMP,
            InputActions::SLIDE,
            InputActions::WALL_RUN,
            InputActions::SPRINT,
            InputActions::FIRE,
            InputActions::PARRY,
            InputActions::MELEE_HEAVY,
            InputActions::RELOAD,
        ];
        let mut union = 0;
        for flag in flags {
            assert_eq!(flag.count_ones(), 1);
            assert_eq!(union & flag, 0, "flag {flag:#b} overlaps");
            union |= flag;
        }
        assert_eq!(union, InputActions::ALL);

        let input = sample_input();
        assert!(input.has_action(InputActions::JUMP));
        assert!(!input.has_action(InputActions::FIRE));
    }

    #[test]
    fn sanitized_input_is_simulation_safe() {
        let hostile = PlayerInputPacket {
            tick: NetworkTick(5),
            movement_vector: Vec2::new(f32::NAN, 50.0),
            pitch: f32::INFINITY,
            yaw: 3.0 * PI,
            actions: u32::MAX,
        };
        let clean = hostile.sanitized();
        assert_eq!(clean.tick, NetworkTick(5));
        assert_eq!(clean.movement_vector, Vec2::new(0.0, 1.0));
        assert_eq!(clean.pitch, 0.0);
        assert!((clean.yaw - -PI).abs() < 1e-5);
        assert_eq!(clean.actions, InputActions::ALL);

        let overspeed = PlayerInputPacket {
            movement_vector: Vec2::new(3.0, 4.0),
            pitch: -10.0,
            ..sample_input()
        }
        .sanitized();
        assert!((overspeed.movement_vector.length() - 1.0).abs() < 1e-5);
        assert_eq!(overspeed.pitch, -FRAC_PI_2);

        let valid = sample_input();
        let resanitized = valid.sanitized();
        assert_eq!(resanitized.movement_vector, valid.movement_vector);
        assert_eq!(resanitized.pitch, valid.pitch);
        assert!((resanitized.yaw - valid.yaw).abs() < 1e-5);
        assert_eq!(resanitized.actions, valid.actions);
    }

    #[test]
    fn sanitized_entity_snapshot_is_finite() {
        let poisoned = EntityStateSnapshot {
            entity_id: NetworkEntityId(3),
            position: Vec3::new(1.0, f32::NAN, 2.0),
            velocity: Vec3::INFINITY,
            yaw: f32::NEG_INFINITY,
            pitch: f32::NAN,
            state_flags: 9,
        };
        let clean = poisoned.sanitized();
        assert_eq!(clean.position, Vec3::new(1.0, 0.0, 2.0));
        assert_eq!(clean.velocity, Vec3::ZERO);
        assert_eq!(clean.yaw, 0.0);
        assert_eq!(clean.pitch, 0.0);
        assert_eq!(clean.entity_id, NetworkEntityId(3));
        assert_eq!(clean.state_flags, 9);
    }
}
