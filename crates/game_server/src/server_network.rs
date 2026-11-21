//! Authoritative 64 Hz server: connections, input processing and snapshots.
//!
//! Per fixed tick: pump the transport and buffer messages (Input set), apply
//! each client's inputs in tick order (Simulate), then record lag
//! compensation history and broadcast snapshots (Sanitize).
//!
//! Inputs are applied one step per client tick. A missing tick (all copies
//! lost) is filled with the previous input so server and client stay in
//! step; at most `max_input_steps_per_tick` steps run per client per server
//! tick, and inputs too far ahead are dropped, which bounds speed hacks.

use crate::movement::{simulate_movement_step, NetworkMovement};
use crate::transport::{ConnectionId, ServerTransport};
use bevy::prelude::*;
use game_core::identifiers::{NetworkEntityId, PlayerCharacterMarker, PlayerId};
use game_core::input::PlayerInputState;
use game_core::protocol::{
    ClientMessage, EntityStateSnapshot, PlayerInputPacket, ServerMessage, ServerStateSnapshot,
    PROTOCOL_VERSION,
};
use game_core::time::{NetworkTick, FIXED_TIMESTEP_SECS, NETWORK_TICK_RATE_HZ};
use game_core::CurrentNetworkTick;
use game_parkour::KinematicVelocity;
use std::collections::{BTreeMap, HashMap};

/// Bit set on every player's network id; other entities count up from 1.
pub const PLAYER_NETWORK_ID_BIT: u64 = 1 << 63;
/// RTT smoothing factor per sample.
const RTT_SMOOTHING: f32 = 0.1;

/// Network id of `player`'s character, derivable on the client.
pub fn player_network_id(player: PlayerId) -> NetworkEntityId {
    let low = player.0.as_u64_pair().1;
    NetworkEntityId(low | PLAYER_NETWORK_ID_BIT)
}

/// The installed server transport.
#[derive(Resource)]
pub struct ServerTransportResource(pub Box<dyn ServerTransport>);

impl ServerTransportResource {
    pub fn new(transport: impl ServerTransport) -> Self {
        Self(Box::new(transport))
    }
}

/// An entity included in snapshots and lag compensation.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Replicated {
    pub id: NetworkEntityId,
}

/// A connected player's character.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct NetworkedPlayer {
    pub player_id: PlayerId,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConnectedClient {
    pub player_id: PlayerId,
    pub network_entity_id: NetworkEntityId,
    /// Newest input tick received.
    pub last_received_tick: NetworkTick,
    /// Smoothed round trip, from how far behind the server clock inputs arrive.
    pub rtt_ms: f32,
    pub connection: ConnectionId,
    pub entity: Entity,
    /// Newest input tick applied to the simulation.
    pub last_processed_tick: Option<NetworkTick>,
    pub pending_inputs: BTreeMap<u64, PlayerInputPacket>,
    pub last_input: PlayerInputPacket,
}

impl ConnectedClient {
    fn record_rtt_sample(&mut self, server_tick: NetworkTick, input_tick: NetworkTick) {
        let lead = server_tick.diff(input_tick);
        if lead < 0 {
            return;
        }
        let sample = lead as f32 * FIXED_TIMESTEP_SECS * 1000.0;
        self.rtt_ms = if self.rtt_ms == 0.0 {
            sample
        } else {
            self.rtt_ms + (sample - self.rtt_ms) * RTT_SMOOTHING
        };
    }

    /// Takes the next input to simulate, filling a gap with the previous
    /// input when later ticks have already arrived.
    fn next_input(&mut self) -> Option<PlayerInputPacket> {
        let next = match self.last_processed_tick {
            Some(t) => t.next().0,
            None => *self.pending_inputs.keys().next()?,
        };
        // Late copies of already simulated ticks.
        self.pending_inputs.retain(|&tick, _| tick >= next);
        if let Some(packet) = self.pending_inputs.remove(&next) {
            return Some(packet);
        }
        if self.pending_inputs.is_empty() {
            return None;
        }
        Some(PlayerInputPacket {
            tick: NetworkTick(next),
            ..self.last_input
        })
    }
}

#[derive(Resource, Debug, Clone)]
pub struct ServerNetworkState {
    pub clients: HashMap<PlayerId, ConnectedClient>,
    pub connections: HashMap<ConnectionId, PlayerId>,
    pub current_tick: NetworkTick,
    pub snapshot_rate_hz: u32,
    pub spawn_point: Vec3,
    pub max_input_steps_per_tick: usize,
    /// Inputs more than this many ticks past the last applied one are dropped.
    pub max_input_lead_ticks: u64,
    pub next_entity_id: u64,
}

impl Default for ServerNetworkState {
    fn default() -> Self {
        Self {
            clients: HashMap::new(),
            connections: HashMap::new(),
            current_tick: NetworkTick::ZERO,
            snapshot_rate_hz: NETWORK_TICK_RATE_HZ,
            spawn_point: Vec3::ZERO,
            max_input_steps_per_tick: 4,
            max_input_lead_ticks: 128,
            next_entity_id: 1,
        }
    }
}

impl ServerNetworkState {
    pub fn allocate_entity_id(&mut self) -> NetworkEntityId {
        let id = NetworkEntityId(self.next_entity_id);
        self.next_entity_id = (self.next_entity_id + 1) & !PLAYER_NETWORK_ID_BIT;
        id
    }

    pub fn client_by_connection(&self, connection: ConnectionId) -> Option<&ConnectedClient> {
        self.connections
            .get(&connection)
            .and_then(|p| self.clients.get(p))
    }

    /// Snapshot cadence in ticks (1 = every tick).
    pub fn snapshot_interval_ticks(&self) -> u64 {
        u64::from(NETWORK_TICK_RATE_HZ / self.snapshot_rate_hz.clamp(1, NETWORK_TICK_RATE_HZ))
    }
}

#[derive(Event, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientConnectedEvent {
    pub player_id: PlayerId,
    pub entity: Entity,
}

#[derive(Event, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientDisconnectedEvent {
    pub player_id: PlayerId,
}

pub fn sync_server_tick(tick: Res<CurrentNetworkTick>, mut state: ResMut<ServerNetworkState>) {
    state.current_tick = tick.0;
}

/// Pumps the transport and handles every message that arrived.
pub fn process_incoming_packets(
    mut commands: Commands,
    time: Res<Time<Fixed>>,
    mut transport: ResMut<ServerTransportResource>,
    mut state: ResMut<ServerNetworkState>,
    mut connected: EventWriter<ClientConnectedEvent>,
) {
    let transport = &mut transport.0;
    transport.update(time.delta());
    let state = state.as_mut();
    for (connection, message) in transport.receive() {
        match message {
            ClientMessage::Handshake { protocol_version } => {
                if let Some(client) = state.client_by_connection(connection) {
                    transport.send(
                        connection,
                        ServerMessage::HandshakeAck {
                            player_id: client.player_id,
                            server_tick: state.current_tick,
                        },
                    );
                    continue;
                }
                if protocol_version != PROTOCOL_VERSION {
                    transport.send(
                        connection,
                        ServerMessage::Disconnect(format!(
                            "protocol version {protocol_version} is not supported (server speaks {PROTOCOL_VERSION})"
                        )),
                    );
                    transport.flush();
                    transport.disconnect(connection);
                    continue;
                }
                let mut player_id = PlayerId::new_random();
                while state
                    .clients
                    .values()
                    .any(|c| c.network_entity_id == player_network_id(player_id))
                {
                    player_id = PlayerId::new_random();
                }
                let network_entity_id = player_network_id(player_id);
                let entity = commands
                    .spawn((
                        Name::new("Networked Player"),
                        PlayerCharacterMarker,
                        NetworkedPlayer { player_id },
                        Replicated {
                            id: network_entity_id,
                        },
                        NetworkMovement,
                        Transform::from_translation(state.spawn_point),
                        KinematicVelocity::default(),
                        PlayerInputState::default(),
                    ))
                    .id();
                state.clients.insert(
                    player_id,
                    ConnectedClient {
                        player_id,
                        network_entity_id,
                        last_received_tick: NetworkTick::ZERO,
                        rtt_ms: 0.0,
                        connection,
                        entity,
                        last_processed_tick: None,
                        pending_inputs: BTreeMap::new(),
                        last_input: PlayerInputPacket::default(),
                    },
                );
                state.connections.insert(connection, player_id);
                transport.send(
                    connection,
                    ServerMessage::HandshakeAck {
                        player_id,
                        server_tick: state.current_tick,
                    },
                );
                connected.send(ClientConnectedEvent { player_id, entity });
            }
            ClientMessage::Input(packet) => {
                let Some(player_id) = state.connections.get(&connection).copied() else {
                    continue;
                };
                let current_tick = state.current_tick;
                let lead_limit = state.max_input_lead_ticks;
                let Some(client) = state.clients.get_mut(&player_id) else {
                    continue;
                };
                let packet = packet.sanitized();
                let tick = packet.tick;
                if client.last_processed_tick.is_some_and(|t| tick <= t) {
                    continue;
                }
                let base = client
                    .last_processed_tick
                    .unwrap_or(client.last_received_tick);
                if client.last_processed_tick.is_some() && tick.diff(base) > lead_limit as i64 {
                    continue;
                }
                if tick > client.last_received_tick {
                    client.last_received_tick = tick;
                    client.record_rtt_sample(current_tick, tick);
                }
                client.pending_inputs.insert(tick.0, packet);
            }
            ClientMessage::Ping(timestamp) => {
                transport.send(connection, ServerMessage::Pong(timestamp));
            }
            ClientMessage::Interact(_) => {}
        }
    }
}

/// Applies buffered inputs to each client's character.
pub fn apply_client_inputs(
    mut state: ResMut<ServerNetworkState>,
    mut characters: Query<(
        &mut Transform,
        &mut KinematicVelocity,
        Option<&mut PlayerInputState>,
        Has<NetworkMovement>,
    )>,
) {
    let max_steps = state.max_input_steps_per_tick;
    for client in state.clients.values_mut() {
        let Ok((mut transform, mut velocity, mut input_state, moved_here)) =
            characters.get_mut(client.entity)
        else {
            continue;
        };
        for _ in 0..max_steps {
            let Some(packet) = client.next_input() else {
                break;
            };
            if moved_here {
                simulate_movement_step(&packet, &mut transform.translation, &mut velocity.0);
            }
            if let Some(input_state) = input_state.as_mut() {
                input_state.push(packet);
            }
            client.last_processed_tick = Some(packet.tick);
            client.last_input = packet;
        }
    }
}

/// Gives enemies and projectiles a network id so they replicate.
pub fn assign_network_ids(
    mut commands: Commands,
    mut state: ResMut<ServerNetworkState>,
    unassigned: Query<
        Entity,
        (
            Or<(
                With<game_ai::Enemy>,
                With<game_combat::projectiles::Projectile>,
            )>,
            Without<Replicated>,
        ),
    >,
) {
    for entity in &unassigned {
        let id = state.allocate_entity_id();
        commands.entity(entity).insert(Replicated { id });
    }
}

/// Builds the shared entity list for a snapshot.
pub fn collect_entity_snapshots(
    state: &ServerNetworkState,
    replicated: &Query<(
        &Replicated,
        &Transform,
        Option<&KinematicVelocity>,
        Option<&NetworkedPlayer>,
    )>,
) -> Vec<EntityStateSnapshot> {
    let mut entities: Vec<EntityStateSnapshot> = replicated
        .iter()
        .map(|(r, transform, velocity, player)| {
            let (yaw, pitch) = player
                .and_then(|p| state.clients.get(&p.player_id))
                .map_or_else(
                    || (transform.rotation.to_euler(EulerRot::YXZ).0, 0.0),
                    |c| (c.last_input.yaw, c.last_input.pitch),
                );
            EntityStateSnapshot {
                entity_id: r.id,
                position: transform.translation,
                velocity: velocity.map_or(Vec3::ZERO, |v| v.0),
                yaw,
                pitch,
                state_flags: 0,
            }
            .sanitized()
        })
        .collect();
    entities.sort_by_key(|e| e.entity_id.0);
    entities
}

/// Sends each client a snapshot with its own last processed input tick.
pub fn broadcast_world_snapshots(
    state: Res<ServerNetworkState>,
    mut transport: ResMut<ServerTransportResource>,
    replicated: Query<(
        &Replicated,
        &Transform,
        Option<&KinematicVelocity>,
        Option<&NetworkedPlayer>,
    )>,
) {
    if state.current_tick.0 % state.snapshot_interval_ticks() == 0 && !state.clients.is_empty() {
        let entities = collect_entity_snapshots(&state, &replicated);
        for client in state.clients.values() {
            let snapshot = ServerStateSnapshot {
                server_tick: state.current_tick,
                last_processed_client_tick: client.last_processed_tick.unwrap_or_default(),
                entities: entities.clone(),
            };
            transport
                .0
                .send(client.connection, ServerMessage::Snapshot(snapshot));
        }
    }
    transport.0.flush();
}

/// Removes clients whose connection closed and despawns their character.
pub fn handle_disconnections(
    mut commands: Commands,
    mut transport: ResMut<ServerTransportResource>,
    mut state: ResMut<ServerNetworkState>,
    mut disconnected: EventWriter<ClientDisconnectedEvent>,
) {
    for connection in transport.0.take_disconnections() {
        let Some(player_id) = state.connections.remove(&connection) else {
            continue;
        };
        if let Some(client) = state.clients.remove(&player_id) {
            if let Some(mut entity) = commands.get_entity(client.entity) {
                entity.despawn();
            }
            disconnected.send(ClientDisconnectedEvent { player_id });
        }
    }
}
