//! Client-side prediction and server reconciliation.
//!
//! Each fixed tick the client applies the newest snapshot, samples input
//! for its current tick, sends it (with the previous ticks repeated, so a
//! lost datagram costs nothing) and predicts that tick with the same
//! movement model the server runs. Predicted positions are kept per tick;
//! when the server's state for its last processed tick differs from what
//! was predicted for that tick by more than 5 cm, the client rolls back to
//! the server state and replays the unconfirmed inputs.
//!
//! The simulated `Transform` always holds the true predicted position. The
//! correction is absorbed by `visual_error_offset`, which decays over a few
//! frames and is applied only to [`PredictedVisual`] entities (mesh,
//! camera), so a correction never shows up as a snap.

use crate::transport::ClientTransport;
use bevy::prelude::*;
use game_core::identifiers::{NetworkEntityId, PlayerCharacterMarker, PlayerId};
use game_core::input::PlayerInputState;
use game_core::protocol::{
    ClientMessage, EntityStateSnapshot, PlayerInputPacket, ServerMessage, ServerStateSnapshot,
    PROTOCOL_VERSION,
};
use game_core::time::NetworkTick;
use game_parkour::KinematicVelocity;
use game_server::movement::{simulate_movement_step, MovementState};
use game_server::player_network_id;
use std::collections::{HashMap, VecDeque};

/// 2 s of input history at 64 Hz.
pub const INPUT_BUFFER_CAPACITY: usize = 128;
/// Divergence that triggers a rollback.
pub const ERROR_CORRECTION_THRESHOLD_METERS: f32 = 0.05;
/// Corrections beyond this are treated as teleports and not smoothed.
pub const TELEPORT_SNAP_METERS: f32 = 4.0;
/// Exponential decay rate of the visual correction, 1/s.
pub const VISUAL_DECAY_RATE: f32 = 20.0;
/// Inputs sent per tick: the current one plus the previous ones again.
pub const INPUT_REDUNDANCY: usize = 3;
pub const HANDSHAKE_RETRY_SECS: f64 = 1.0;
pub const PING_INTERVAL_SECS: f64 = 1.0;

/// The installed client transport.
#[derive(Resource)]
pub struct ClientTransportResource(pub Box<dyn ClientTransport>);

impl ClientTransportResource {
    pub fn new(transport: impl ClientTransport) -> Self {
        Self(Box::new(transport))
    }
}

/// The locally controlled, predicted character.
#[derive(Component, Debug, Clone, Copy)]
pub struct LocalPredictedPlayer {
    pub network_id: NetworkEntityId,
}

/// A render-side entity that follows `source` plus its visual correction.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PredictedVisual {
    pub source: Entity,
}

/// Another replicated entity, placed from the newest snapshot.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemoteEntity {
    pub network_id: NetworkEntityId,
}

#[derive(Component, Debug, Clone, Default, PartialEq)]
pub struct ClientPredictionState {
    /// Tick the next sampled input is stamped with.
    pub current_predicted_tick: NetworkTick,
    /// Inputs the server has not confirmed yet, oldest first.
    pub input_history: VecDeque<PlayerInputPacket>,
    /// Predicted position after each tick in `input_history`.
    pub predicted_positions: VecDeque<(NetworkTick, Vec3)>,
    pub visual_error_offset: Vec3,
    /// Newest snapshot tick applied.
    pub last_server_tick: NetworkTick,
    pub first_input_tick: Option<NetworkTick>,
    pub corrections: u32,
    /// Movement state after the newest predicted tick.
    pub movement: MovementState,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ReconcileOutcome {
    /// The server has not applied any of our inputs yet.
    Pending,
    /// Prediction matched within the threshold.
    Verified { error: f32 },
    /// Rolled back and replayed this many inputs.
    Corrected { error: f32, replayed: usize },
}

impl ClientPredictionState {
    pub fn new(start_tick: NetworkTick) -> Self {
        Self {
            current_predicted_tick: start_tick,
            ..Default::default()
        }
    }

    /// Stores `packet` (re-stamped with the current tick) and returns it.
    pub fn record_input(&mut self, mut packet: PlayerInputPacket) -> PlayerInputPacket {
        packet.tick = self.current_predicted_tick;
        if self
            .input_history
            .back()
            .is_some_and(|p| p.tick == packet.tick)
        {
            self.input_history.pop_back();
        }
        self.input_history.push_back(packet);
        while self.input_history.len() > INPUT_BUFFER_CAPACITY {
            self.input_history.pop_front();
        }
        self.first_input_tick.get_or_insert(packet.tick);
        packet
    }

    /// Predicts the current tick from its recorded input and advances.
    pub fn predict_step(&mut self, position: &mut Vec3, velocity: &mut Vec3) {
        let tick = self.current_predicted_tick;
        if let Some(input) = self
            .input_history
            .back()
            .filter(|p| p.tick == tick)
            .copied()
        {
            simulate_movement_step(&input, position, velocity, &mut self.movement);
            self.predicted_positions.retain(|(t, _)| *t != tick);
            self.predicted_positions.push_back((tick, *position));
            while self.predicted_positions.len() > INPUT_BUFFER_CAPACITY {
                self.predicted_positions.pop_front();
            }
        }
        self.current_predicted_tick = tick.next();
    }

    pub fn predicted_position_at(&self, tick: NetworkTick) -> Option<Vec3> {
        self.predicted_positions
            .iter()
            .find(|(t, _)| *t == tick)
            .map(|(_, p)| *p)
    }

    /// Where to draw the character whose simulated position is `simulated`.
    pub fn visual_position(&self, simulated: Vec3) -> Vec3 {
        simulated + self.visual_error_offset
    }

    pub fn decay_visual_error(&mut self, dt: f32) {
        self.visual_error_offset *= 1.0 - (VISUAL_DECAY_RATE * dt).min(1.0);
        if self.visual_error_offset.length_squared() < 1e-10 {
            self.visual_error_offset = Vec3::ZERO;
        }
    }

    /// The most recent inputs, oldest first, for redundant sending.
    pub fn recent_inputs(&self, count: usize) -> impl Iterator<Item = &PlayerInputPacket> {
        self.input_history
            .iter()
            .skip(self.input_history.len().saturating_sub(count))
    }
}

/// Compares the server's state after `last_processed` with what was
/// predicted for that tick; rolls back and replays on divergence.
pub fn reconcile_with_server_state(
    prediction: &mut ClientPredictionState,
    position: &mut Vec3,
    velocity: &mut Vec3,
    server_state: &EntityStateSnapshot,
    last_processed: NetworkTick,
) -> ReconcileOutcome {
    if prediction
        .first_input_tick
        .is_none_or(|first| last_processed < first)
    {
        return ReconcileOutcome::Pending;
    }
    let predicted = prediction.predicted_position_at(last_processed);
    prediction.input_history.retain(|p| p.tick > last_processed);
    // The confirmed tick's entry is kept as the newest known-good state.
    prediction
        .predicted_positions
        .retain(|(t, _)| *t >= last_processed);
    let server = server_state.sanitized();
    // Without a prediction for that tick (history overflowed) we cannot
    // verify, so trust the server.
    let error = predicted.map_or(f32::INFINITY, |p| p.distance(server.position));
    if error <= ERROR_CORRECTION_THRESHOLD_METERS {
        return ReconcileOutcome::Verified { error };
    }
    let shown_before = prediction.visual_position(*position);
    *position = server.position;
    *velocity = server.velocity;
    prediction.movement = MovementState::unpack(server.state_flags);
    let replay: Vec<PlayerInputPacket> = prediction.input_history.iter().copied().collect();
    prediction.predicted_positions.clear();
    prediction
        .predicted_positions
        .push_back((last_processed, server.position));
    for input in &replay {
        simulate_movement_step(input, position, velocity, &mut prediction.movement);
        prediction
            .predicted_positions
            .push_back((input.tick, *position));
    }
    // Large corrections are teleports (respawns): snap instead of gliding.
    prediction.visual_error_offset = if error > TELEPORT_SNAP_METERS {
        Vec3::ZERO
    } else {
        shown_before - *position
    };
    prediction.corrections += 1;
    ReconcileOutcome::Corrected {
        error,
        replayed: replay.len(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ConnectionStatus {
    #[default]
    Disconnected,
    Connecting,
    Connected,
    /// The server refused or closed the session.
    Rejected(String),
}

#[derive(Resource, Debug, Clone, Default)]
pub struct ClientNetworkState {
    pub status: ConnectionStatus,
    pub player_id: Option<PlayerId>,
    pub local_entity: Option<Entity>,
    /// Round trip measured with pings.
    pub rtt_ms: Option<f32>,
    pub server_tick: NetworkTick,
    /// Newest snapshot not yet reconciled.
    pub pending_snapshot: Option<ServerStateSnapshot>,
    /// Newest snapshot applied.
    pub latest_snapshot: Option<ServerStateSnapshot>,
    pub snapshots_received: u64,
    /// Fixed-tick clock, s.
    pub clock_secs: f64,
    pub last_handshake_sent: Option<f64>,
    pub last_ping_sent: Option<f64>,
    /// Where the local character starts; must match the server's spawn.
    pub spawn_point: Vec3,
    pub last_outcome: Option<ReconcileOutcome>,
    /// Player profiles received from the server, by player.
    pub profiles: HashMap<PlayerId, Vec<u8>>,
}

#[derive(Event, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConnectedToServerEvent {
    pub player_id: PlayerId,
    pub entity: Entity,
}

fn clock_ms(state: &ClientNetworkState) -> u64 {
    (state.clock_secs * 1000.0) as u64
}

/// Pumps the transport, keeps the session alive and handles messages.
pub fn pump_client_transport(
    mut commands: Commands,
    time: Res<Time<Fixed>>,
    mut transport: ResMut<ClientTransportResource>,
    mut state: ResMut<ClientNetworkState>,
    mut connected: EventWriter<ConnectedToServerEvent>,
) {
    let transport = &mut transport.0;
    transport.update(time.delta());
    let state = state.as_mut();
    state.clock_secs += time.delta_secs_f64();
    let now = state.clock_secs;

    if !transport.is_connected() && state.status == ConnectionStatus::Connected {
        state.status = ConnectionStatus::Disconnected;
    }
    if matches!(
        state.status,
        ConnectionStatus::Disconnected | ConnectionStatus::Connecting
    ) && state
        .last_handshake_sent
        .is_none_or(|t| now - t >= HANDSHAKE_RETRY_SECS)
    {
        transport.send(ClientMessage::Handshake {
            protocol_version: PROTOCOL_VERSION,
        });
        state.last_handshake_sent = Some(now);
        state.status = ConnectionStatus::Connecting;
    }
    if state.status == ConnectionStatus::Connected
        && state
            .last_ping_sent
            .is_none_or(|t| now - t >= PING_INTERVAL_SECS)
    {
        transport.send(ClientMessage::Ping(clock_ms(state)));
        state.last_ping_sent = Some(now);
    }

    for message in transport.receive() {
        match message {
            ServerMessage::HandshakeAck {
                player_id,
                server_tick,
            } => {
                if state.player_id.is_some() {
                    continue;
                }
                let network_id = player_network_id(player_id);
                // Start one tick ahead so tick 0 always means "no input yet".
                let entity = commands
                    .spawn((
                        Name::new("Local Player"),
                        LocalPredictedPlayer { network_id },
                        PlayerCharacterMarker,
                        ClientPredictionState::new(server_tick.next()),
                        Transform::from_translation(state.spawn_point),
                        KinematicVelocity::default(),
                        PlayerInputState::default(),
                    ))
                    .id();
                state.player_id = Some(player_id);
                state.local_entity = Some(entity);
                state.server_tick = server_tick;
                state.status = ConnectionStatus::Connected;
                connected.send(ConnectedToServerEvent { player_id, entity });
            }
            ServerMessage::Snapshot(snapshot) => {
                state.snapshots_received += 1;
                let newest = state
                    .pending_snapshot
                    .as_ref()
                    .or(state.latest_snapshot.as_ref())
                    .map(|s| s.server_tick);
                if newest.is_none_or(|t| snapshot.server_tick > t) {
                    state.server_tick = snapshot.server_tick;
                    state.pending_snapshot = Some(snapshot);
                }
            }
            ServerMessage::Pong(sent_ms) => {
                state.rtt_ms = Some(clock_ms(state).saturating_sub(sent_ms) as f32);
            }
            ServerMessage::Profile { player_id, data } => {
                state.profiles.insert(player_id, data);
            }
            ServerMessage::Disconnect(reason) => {
                state.status = ConnectionStatus::Rejected(reason);
            }
        }
    }
}

/// Applies the newest snapshot to the local prediction.
pub fn reconcile_server_snapshot(
    mut state: ResMut<ClientNetworkState>,
    mut local: Query<(
        &LocalPredictedPlayer,
        &mut ClientPredictionState,
        &mut Transform,
        &mut KinematicVelocity,
    )>,
) {
    let Some(snapshot) = state.pending_snapshot.take() else {
        return;
    };
    if let Some((player, mut prediction, mut transform, mut velocity)) =
        state.local_entity.and_then(|e| local.get_mut(e).ok())
    {
        if let Some(server_state) = snapshot
            .entities
            .iter()
            .find(|e| e.entity_id == player.network_id)
        {
            let outcome = reconcile_with_server_state(
                &mut prediction,
                &mut transform.translation,
                &mut velocity.0,
                server_state,
                snapshot.last_processed_client_tick,
            );
            state.last_outcome = Some(outcome);
        }
        prediction.last_server_tick = snapshot.server_tick;
    }
    state.latest_snapshot = Some(snapshot);
}

/// Places other replicated entities at their newest snapshot state.
pub fn sync_remote_entities(
    mut commands: Commands,
    state: Res<ClientNetworkState>,
    mut remotes: Query<(Entity, &RemoteEntity, &mut Transform)>,
) {
    let Some(snapshot) = state.latest_snapshot.as_ref() else {
        return;
    };
    let local_id = state.player_id.map(player_network_id);
    let wanted: HashMap<NetworkEntityId, &EntityStateSnapshot> = snapshot
        .entities
        .iter()
        .filter(|e| Some(e.entity_id) != local_id)
        .map(|e| (e.entity_id, e))
        .collect();
    let mut present = Vec::new();
    for (entity, remote, mut transform) in &mut remotes {
        match wanted.get(&remote.network_id) {
            Some(s) => {
                let s = s.sanitized();
                transform.translation = s.position;
                transform.rotation = Quat::from_rotation_y(s.yaw);
                present.push(remote.network_id);
            }
            None => commands.entity(entity).despawn_recursive(),
        }
    }
    for (id, s) in wanted {
        if !present.contains(&id) {
            let s = s.sanitized();
            commands.spawn((
                RemoteEntity { network_id: id },
                Transform::from_translation(s.position).with_rotation(Quat::from_rotation_y(s.yaw)),
            ));
        }
    }
}

/// Samples this tick's input, records it and sends it.
pub fn sample_and_record_input(
    state: Res<ClientNetworkState>,
    mut transport: ResMut<ClientTransportResource>,
    mut local: Query<
        (&mut ClientPredictionState, Option<&PlayerInputState>),
        With<LocalPredictedPlayer>,
    >,
) {
    if state.status != ConnectionStatus::Connected {
        return;
    }
    let Some((mut prediction, input)) = state.local_entity.and_then(|e| local.get_mut(e).ok())
    else {
        return;
    };
    let sampled = input
        .and_then(PlayerInputState::latest)
        .copied()
        .unwrap_or_default()
        .sanitized();
    prediction.record_input(sampled);
    for packet in prediction.recent_inputs(INPUT_REDUNDANCY) {
        transport.0.send(ClientMessage::Input(*packet));
    }
}

/// Predicts the local character for this tick.
pub fn predict_local_movement(
    mut local: Query<
        (
            &mut ClientPredictionState,
            &mut Transform,
            &mut KinematicVelocity,
        ),
        With<LocalPredictedPlayer>,
    >,
) {
    for (mut prediction, mut transform, mut velocity) in &mut local {
        prediction.predict_step(&mut transform.translation, &mut velocity.0);
    }
}

pub fn flush_client_transport(mut transport: ResMut<ClientTransportResource>) {
    transport.0.flush();
}

/// Decays the correction offset and positions [`PredictedVisual`] entities.
pub fn apply_visual_error_decay(
    time: Res<Time>,
    mut predicted: Query<(&mut ClientPredictionState, &Transform), Without<PredictedVisual>>,
    mut visuals: Query<(&PredictedVisual, &mut Transform)>,
) {
    let dt = time.delta_secs();
    for (mut prediction, _) in &mut predicted {
        prediction.decay_visual_error(dt);
    }
    for (visual, mut transform) in &mut visuals {
        if let Ok((prediction, source)) = predicted.get(visual.source) {
            transform.translation = prediction.visual_position(source.translation);
            transform.rotation = source.rotation;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_core::protocol::InputActions;
    use game_server::SPRINT_SPEED;

    /// Velocity after `ticks` sprinting ticks from a standstill.
    fn reference_velocity(ticks: usize) -> Vec3 {
        let (mut pos, mut vel) = (Vec3::ZERO, Vec3::ZERO);
        let mut state = MovementState::default();
        for _ in 0..ticks {
            simulate_movement_step(&sprint(), &mut pos, &mut vel, &mut state);
        }
        vel
    }

    /// Distance the shared movement step covers sprinting from a standstill.
    fn reference_distance(ticks: usize) -> f32 {
        let (mut pos, mut vel) = (Vec3::ZERO, Vec3::ZERO);
        let mut state = MovementState::default();
        for _ in 0..ticks {
            simulate_movement_step(&sprint(), &mut pos, &mut vel, &mut state);
        }
        -pos.z
    }

    fn sprint() -> PlayerInputPacket {
        PlayerInputPacket {
            movement_vector: Vec2::Y,
            actions: InputActions::SPRINT,
            ..Default::default()
        }
    }

    fn server_state(position: Vec3) -> EntityStateSnapshot {
        EntityStateSnapshot {
            entity_id: NetworkEntityId(1),
            position,
            velocity: Vec3::new(0.0, 0.0, -SPRINT_SPEED),
            ..Default::default()
        }
    }

    /// Predicts `ticks` sprint inputs from `start`, beginning at tick 1.
    fn predicted(start: Vec3, ticks: u64) -> (ClientPredictionState, Vec3, Vec3) {
        let mut prediction = ClientPredictionState::new(NetworkTick(1));
        let (mut pos, mut vel) = (start, Vec3::ZERO);
        for _ in 0..ticks {
            prediction.record_input(sprint());
            prediction.predict_step(&mut pos, &mut vel);
        }
        (prediction, pos, vel)
    }

    #[test]
    fn ten_sprint_inputs_predict_kinematic_distance() {
        let (prediction, pos, vel) = predicted(Vec3::ZERO, 10);
        assert_eq!(prediction.input_history.len(), 10);
        assert_eq!(prediction.predicted_positions.len(), 10);
        assert_eq!(prediction.current_predicted_tick, NetworkTick(11));
        let ticks: Vec<u64> = prediction.input_history.iter().map(|p| p.tick.0).collect();
        assert_eq!(ticks, (1..=10).collect::<Vec<_>>());
        assert!((pos.z + reference_distance(10)).abs() < 1e-5, "{pos}");
        assert_eq!(vel, Vec3::new(0.0, 0.0, -SPRINT_SPEED));
    }

    #[test]
    fn history_is_capped_at_two_seconds() {
        let (prediction, _, _) = predicted(Vec3::ZERO, 300);
        assert_eq!(prediction.input_history.len(), INPUT_BUFFER_CAPACITY);
        assert_eq!(
            prediction.input_history.front().unwrap().tick,
            NetworkTick(300 - 127)
        );
        assert_eq!(prediction.predicted_positions.len(), INPUT_BUFFER_CAPACITY);
    }

    #[test]
    fn matching_server_state_only_trims_history() {
        let (mut prediction, mut pos, mut vel) = predicted(Vec3::ZERO, 10);
        let at_5 = prediction.predicted_position_at(NetworkTick(5)).unwrap();
        let before = pos;
        let outcome = reconcile_with_server_state(
            &mut prediction,
            &mut pos,
            &mut vel,
            &server_state(at_5 + Vec3::X * 0.03),
            NetworkTick(5),
        );
        assert!(
            matches!(outcome, ReconcileOutcome::Verified { error } if (error - 0.03).abs() < 1e-5)
        );
        assert_eq!(pos, before, "within 5 cm nothing moves");
        assert_eq!(prediction.input_history.len(), 5, "ticks 6..=10 remain");
        assert_eq!(prediction.visual_error_offset, Vec3::ZERO);
    }

    #[test]
    fn divergence_rolls_back_replays_and_smooths() {
        // Start so that the prediction for tick 5 is exactly (0, 0, -10).
        let start = Vec3::new(0.0, 0.0, -10.0 + reference_distance(5));
        let (mut prediction, mut pos, mut vel) = predicted(start, 10);
        assert!(
            prediction
                .predicted_position_at(NetworkTick(5))
                .unwrap()
                .distance(Vec3::new(0.0, 0.0, -10.0))
                < 1e-5
        );
        let shown_before = prediction.visual_position(pos);

        let outcome = reconcile_with_server_state(
            &mut prediction,
            &mut pos,
            &mut vel,
            &EntityStateSnapshot {
                velocity: reference_velocity(5),
                // The server saw the body grounded, as the prediction did.
                state_flags: MovementState::default().pack(),
                ..server_state(Vec3::new(0.0, 0.0, -9.5))
            },
            NetworkTick(5),
        );
        let ReconcileOutcome::Corrected { error, replayed } = outcome else {
            panic!("expected a rollback, got {outcome:?}");
        };
        assert!((error - 0.5).abs() < 1e-4);
        assert_eq!(replayed, 5, "inputs 6..=10 replayed");
        let expected = Vec3::new(
            0.0,
            0.0,
            -9.5 - (reference_distance(10) - reference_distance(5)),
        );
        assert!(pos.distance(expected) < 1e-4, "{pos}");
        assert!(
            prediction
                .visual_error_offset
                .distance(Vec3::new(0.0, 0.0, -0.5))
                < 1e-4
        );
        assert!(
            prediction.visual_position(pos).distance(shown_before) < 1e-5,
            "no visible jump"
        );
        assert_eq!(prediction.corrections, 1);
        assert!(
            prediction
                .predicted_position_at(NetworkTick(10))
                .unwrap()
                .distance(expected)
                < 1e-4
        );

        // The offset fades over a few frames at 60 FPS.
        let mut sizes = vec![prediction.visual_error_offset.length()];
        for _ in 0..30 {
            prediction.decay_visual_error(1.0 / 60.0);
            sizes.push(prediction.visual_error_offset.length());
        }
        assert!(sizes.windows(2).all(|w| w[1] <= w[0]), "monotonic decay");
        assert!(sizes[6] < 0.5 * 0.2, "under 20% after 0.1 s");
        assert!(sizes[30] < 1e-4, "gone after half a second");
    }

    #[test]
    fn snapshots_before_our_first_input_are_pending() {
        let mut prediction = ClientPredictionState::new(NetworkTick(50));
        let (mut pos, mut vel) = (Vec3::ZERO, Vec3::ZERO);
        let outcome = reconcile_with_server_state(
            &mut prediction,
            &mut pos,
            &mut vel,
            &server_state(Vec3::ONE),
            NetworkTick(0),
        );
        assert_eq!(outcome, ReconcileOutcome::Pending);
        prediction.record_input(sprint());
        prediction.predict_step(&mut pos, &mut vel);
        let outcome = reconcile_with_server_state(
            &mut prediction,
            &mut pos,
            &mut vel,
            &server_state(Vec3::ONE),
            NetworkTick(49),
        );
        assert_eq!(
            outcome,
            ReconcileOutcome::Pending,
            "server is still behind our first input"
        );
        assert_eq!(prediction.input_history.len(), 1);
    }

    #[test]
    fn redundant_inputs_are_the_newest_three() {
        let (prediction, _, _) = predicted(Vec3::ZERO, 10);
        let ticks: Vec<u64> = prediction
            .recent_inputs(INPUT_REDUNDANCY)
            .map(|p| p.tick.0)
            .collect();
        assert_eq!(ticks, vec![8, 9, 10]);
        let (short, _, _) = predicted(Vec3::ZERO, 2);
        assert_eq!(short.recent_inputs(INPUT_REDUNDANCY).count(), 2);
    }
}
