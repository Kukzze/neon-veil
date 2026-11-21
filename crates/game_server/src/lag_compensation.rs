//! One-second hitbox history for rewinding hitscan and melee checks to the
//! world the shooter saw.
//!
//! A client's input tick lines up with the server tick of the newest
//! snapshot it had when it pressed fire (its clock starts at the server tick
//! from the handshake), so `target_tick` for a shot is the shot's input tick.

use crate::server_network::{Replicated, ServerNetworkState};
use bevy::prelude::*;
use game_core::identifiers::NetworkEntityId;
use game_core::time::NetworkTick;
use std::collections::VecDeque;

/// 1.0 s at 64 Hz.
pub const HISTORY_BUFFER_TICKS: usize = 64;

#[derive(Debug, Clone, PartialEq)]
pub struct HistoricalHitboxFrame {
    pub tick: NetworkTick,
    pub timestamp_seconds: f64,
    pub entity_positions: Vec<(NetworkEntityId, Vec3, Quat)>,
}

#[derive(Resource, Debug, Clone, Default)]
pub struct ServerLagCompensationBuffer {
    /// Oldest first.
    pub history: VecDeque<HistoricalHitboxFrame>,
}

/// Distance along the ray to the first intersection with the sphere, if
/// any within `max_range`. A ray starting inside reports the exit point.
pub fn ray_sphere_distance(
    origin: Vec3,
    direction: Vec3,
    max_range: f32,
    center: Vec3,
    radius: f32,
) -> Option<f32> {
    let dir = direction.normalize_or_zero();
    if dir == Vec3::ZERO {
        return None;
    }
    let oc = origin - center;
    let b = oc.dot(dir);
    let c = oc.length_squared() - radius * radius;
    let discriminant = b * b - c;
    if discriminant < 0.0 {
        return None;
    }
    let root = discriminant.sqrt();
    let near = -b - root;
    let t = if near >= 0.0 { near } else { -b + root };
    (t >= 0.0 && t <= max_range).then_some(t)
}

impl ServerLagCompensationBuffer {
    /// Appends a frame, dropping the oldest beyond one second. A frame for a
    /// tick already recorded replaces it.
    pub fn record_frame(
        &mut self,
        tick: NetworkTick,
        timestamp: f64,
        entities: Vec<(NetworkEntityId, Vec3, Quat)>,
    ) {
        let frame = HistoricalHitboxFrame {
            tick,
            timestamp_seconds: timestamp,
            entity_positions: entities,
        };
        match self.history.back_mut() {
            Some(last) if last.tick == tick => *last = frame,
            _ => self.history.push_back(frame),
        }
        while self.history.len() > HISTORY_BUFFER_TICKS {
            self.history.pop_front();
        }
    }

    pub fn oldest_tick(&self) -> Option<NetworkTick> {
        self.history.front().map(|f| f.tick)
    }

    pub fn newest_tick(&self) -> Option<NetworkTick> {
        self.history.back().map(|f| f.tick)
    }

    /// Frame closest in time to `target_time` (earlier frame on ties).
    pub fn rewind_to_timestamp(&self, target_time: f64) -> Option<&HistoricalHitboxFrame> {
        self.history.iter().min_by(|a, b| {
            (a.timestamp_seconds - target_time)
                .abs()
                .total_cmp(&(b.timestamp_seconds - target_time).abs())
        })
    }

    /// The frame for `target_tick`, or the nearest recorded one when it is
    /// outside the window (older shots clamp to the oldest frame).
    pub fn rewind_to_tick(&self, target_tick: NetworkTick) -> Option<&HistoricalHitboxFrame> {
        self.history
            .iter()
            .min_by_key(|f| f.tick.diff(target_tick).unsigned_abs())
    }

    /// Nearest entity whose hitbox sphere the ray hits at `target_tick`,
    /// with the hit distance.
    pub fn raycast_lag_compensated(
        &self,
        origin: Vec3,
        direction: Vec3,
        max_range: f32,
        target_tick: NetworkTick,
        hitbox_radius: f32,
    ) -> Option<(NetworkEntityId, f32)> {
        self.raycast_lag_compensated_excluding(
            origin,
            direction,
            max_range,
            target_tick,
            hitbox_radius,
            None,
        )
    }

    /// As [`Self::raycast_lag_compensated`], ignoring the shooter.
    pub fn raycast_lag_compensated_excluding(
        &self,
        origin: Vec3,
        direction: Vec3,
        max_range: f32,
        target_tick: NetworkTick,
        hitbox_radius: f32,
        shooter: Option<NetworkEntityId>,
    ) -> Option<(NetworkEntityId, f32)> {
        let frame = self.rewind_to_tick(target_tick)?;
        frame
            .entity_positions
            .iter()
            .filter(|(id, _, _)| Some(*id) != shooter)
            .filter_map(|&(id, center, _)| {
                ray_sphere_distance(origin, direction, max_range, center, hitbox_radius)
                    .map(|d| (id, d))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
    }
}

/// Records every replicated entity's pose for the current tick.
pub fn record_lag_compensation_frame(
    time: Res<Time<Fixed>>,
    state: Res<ServerNetworkState>,
    mut buffer: ResMut<ServerLagCompensationBuffer>,
    replicated: Query<(&Replicated, &Transform)>,
) {
    let mut entities: Vec<(NetworkEntityId, Vec3, Quat)> = replicated
        .iter()
        .map(|(r, t)| (r.id, t.translation, t.rotation))
        .collect();
    entities.sort_by_key(|(id, _, _)| id.0);
    buffer.record_frame(state.current_tick, time.elapsed_secs_f64(), entities);
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_core::time::FIXED_TIMESTEP_SECS;

    const TARGET: NetworkEntityId = NetworkEntityId(7);
    const BYSTANDER: NetworkEntityId = NetworkEntityId(8);

    /// Target runs along +X at 10 m/s for 30 ticks, 20 m in front of the
    /// shooter at the origin; a bystander stands still off to the side.
    fn moving_target_history() -> ServerLagCompensationBuffer {
        let mut buffer = ServerLagCompensationBuffer::default();
        for tick in 0..30u64 {
            let x = 10.0 * tick as f32 * FIXED_TIMESTEP_SECS;
            buffer.record_frame(
                NetworkTick(tick),
                tick as f64 / 64.0,
                vec![
                    (TARGET, Vec3::new(x, 1.0, -20.0), Quat::IDENTITY),
                    (BYSTANDER, Vec3::new(-30.0, 1.0, -20.0), Quat::IDENTITY),
                ],
            );
        }
        buffer
    }

    #[test]
    fn ray_sphere_cases() {
        let c = Vec3::new(0.0, 0.0, -10.0);
        assert_eq!(
            ray_sphere_distance(Vec3::ZERO, Vec3::NEG_Z, 100.0, c, 1.0),
            Some(9.0)
        );
        assert_eq!(
            ray_sphere_distance(Vec3::ZERO, Vec3::NEG_Z * 5.0, 100.0, c, 1.0),
            Some(9.0),
            "direction need not be unit"
        );
        assert_eq!(
            ray_sphere_distance(Vec3::ZERO, Vec3::Z, 100.0, c, 1.0),
            None,
            "behind the shooter"
        );
        assert_eq!(
            ray_sphere_distance(Vec3::ZERO, Vec3::NEG_Z, 5.0, c, 1.0),
            None,
            "out of range"
        );
        assert_eq!(
            ray_sphere_distance(Vec3::new(1.5, 0.0, 0.0), Vec3::NEG_Z, 100.0, c, 1.0),
            None,
            "passes beside"
        );
        assert_eq!(
            ray_sphere_distance(c, Vec3::NEG_Z, 100.0, c, 1.0),
            Some(1.0),
            "starting inside"
        );
        assert_eq!(
            ray_sphere_distance(Vec3::ZERO, Vec3::ZERO, 100.0, c, 1.0),
            None
        );
    }

    #[test]
    fn buffer_keeps_one_second() {
        let mut buffer = ServerLagCompensationBuffer::default();
        for tick in 0..100u64 {
            buffer.record_frame(NetworkTick(tick), tick as f64 / 64.0, Vec::new());
        }
        assert_eq!(buffer.history.len(), HISTORY_BUFFER_TICKS);
        assert_eq!(buffer.oldest_tick(), Some(NetworkTick(36)));
        assert_eq!(buffer.newest_tick(), Some(NetworkTick(99)));
        buffer.record_frame(
            NetworkTick(99),
            9.9,
            vec![(TARGET, Vec3::ONE, Quat::IDENTITY)],
        );
        assert_eq!(
            buffer.history.len(),
            HISTORY_BUFFER_TICKS,
            "same tick replaces"
        );
        assert_eq!(buffer.history.back().unwrap().entity_positions.len(), 1);
    }

    #[test]
    fn rewind_lookups() {
        let buffer = moving_target_history();
        assert_eq!(
            buffer.rewind_to_tick(NetworkTick(12)).unwrap().tick,
            NetworkTick(12)
        );
        assert_eq!(
            buffer.rewind_to_tick(NetworkTick(500)).unwrap().tick,
            NetworkTick(29),
            "future clamps to newest"
        );
        assert_eq!(
            buffer.rewind_to_timestamp(10.2 / 64.0).unwrap().tick,
            NetworkTick(10)
        );
        assert_eq!(
            buffer.rewind_to_timestamp(-1.0).unwrap().tick,
            NetworkTick(0)
        );
        assert!(ServerLagCompensationBuffer::default()
            .rewind_to_tick(NetworkTick(1))
            .is_none());
    }

    #[test]
    fn shot_at_the_past_position_hits_only_when_rewound() {
        let buffer = moving_target_history();
        let now = NetworkTick(29);
        let shot_tick = NetworkTick(19); // 10 ticks = 156 ms in the past
        let seen = buffer.rewind_to_tick(shot_tick).unwrap().entity_positions[0].1;
        let aim = seen - Vec3::ZERO;
        let rewound = buffer.raycast_lag_compensated(Vec3::ZERO, aim, 100.0, shot_tick, 0.5);
        let (id, distance) = rewound.expect("hit at the rewound position");
        assert_eq!(id, TARGET);
        assert!((distance - (aim.length() - 0.5)).abs() < 1e-3);
        // The target has moved 1.56 m since: the same ray misses the present.
        assert!(buffer
            .raycast_lag_compensated(Vec3::ZERO, aim, 100.0, now, 0.5)
            .is_none());
        let current = buffer.rewind_to_tick(now).unwrap().entity_positions[0].1;
        assert!((current.x - seen.x - 1.5625).abs() < 1e-4);
    }

    #[test]
    fn nearest_hit_wins_and_shooter_is_ignored() {
        let mut buffer = ServerLagCompensationBuffer::default();
        let near = NetworkEntityId(1);
        let far = NetworkEntityId(2);
        let shooter = NetworkEntityId(3);
        buffer.record_frame(
            NetworkTick(5),
            0.0,
            vec![
                (far, Vec3::new(0.0, 0.0, -30.0), Quat::IDENTITY),
                (near, Vec3::new(0.0, 0.0, -10.0), Quat::IDENTITY),
                (shooter, Vec3::ZERO, Quat::IDENTITY),
            ],
        );
        let hit =
            buffer.raycast_lag_compensated(Vec3::ZERO, Vec3::NEG_Z, 100.0, NetworkTick(5), 0.5);
        assert_eq!(
            hit.map(|h| h.0),
            Some(shooter),
            "a ray from inside a hitbox hits it"
        );
        let hit = buffer.raycast_lag_compensated_excluding(
            Vec3::ZERO,
            Vec3::NEG_Z,
            100.0,
            NetworkTick(5),
            0.5,
            Some(shooter),
        );
        assert_eq!(hit, Some((near, 9.5)));
    }
}
