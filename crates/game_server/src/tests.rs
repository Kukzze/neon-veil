//! Server loop tests over the in-process transport.

use super::*;
use bevy::time::TimeUpdateStrategy;
use game_core::identifiers::PlayerId;
use game_core::protocol::{
    ClientMessage, InputActions, PlayerInputPacket, ServerMessage, ServerStateSnapshot,
    PROTOCOL_VERSION,
};
use game_core::time::{NetworkTick, FIXED_TIMESTEP_SECS};
use game_parkour::KinematicVelocity;
use std::time::{Duration, Instant};

const SPRINT_STEP: f32 = SPRINT_SPEED * FIXED_TIMESTEP_SECS;

fn server_app(network: &LoopbackNetwork) -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, ServerPlugin))
        .insert_resource(TimeUpdateStrategy::ManualDuration(FIXED_TIMESTEP))
        .insert_resource(ServerTransportResource::new(network.server_transport()));
    // Let the fixed clock start so every later update runs exactly one tick.
    app.update();
    app
}

fn sprint(tick: u64) -> ClientMessage {
    ClientMessage::Input(PlayerInputPacket {
        tick: NetworkTick(tick),
        movement_vector: Vec2::Y,
        actions: InputActions::SPRINT,
        ..Default::default()
    })
}

fn handshake() -> ClientMessage {
    ClientMessage::Handshake {
        protocol_version: PROTOCOL_VERSION,
    }
}

/// Connects a client and returns it with its player id.
fn join(app: &mut App, network: &LoopbackNetwork) -> (LoopbackClientTransport, PlayerId) {
    let mut client = network.connect();
    client.send(handshake());
    app.update();
    let ack = client.receive().into_iter().find_map(|m| match m {
        ServerMessage::HandshakeAck { player_id, .. } => Some(player_id),
        _ => None,
    });
    (client, ack.expect("handshake acknowledged"))
}

fn client_state(app: &App, player: PlayerId) -> ConnectedClient {
    app.world().resource::<ServerNetworkState>().clients[&player].clone()
}

fn position(app: &App, player: PlayerId) -> Vec3 {
    let entity = client_state(app, player).entity;
    app.world().get::<Transform>(entity).unwrap().translation
}

fn snapshots(client: &mut LoopbackClientTransport) -> Vec<ServerStateSnapshot> {
    client
        .receive()
        .into_iter()
        .filter_map(|m| match m {
            ServerMessage::Snapshot(s) => Some(s),
            _ => None,
        })
        .collect()
}

#[test]
fn handshake_allocates_player_and_spawns_character() {
    let network = LoopbackNetwork::perfect();
    let mut app = server_app(&network);
    let mut client = network.connect();
    client.send(handshake());
    app.update();
    let messages = client.receive();
    let (player_id, server_tick) = messages
        .iter()
        .find_map(|m| match m {
            ServerMessage::HandshakeAck {
                player_id,
                server_tick,
            } => Some((*player_id, *server_tick)),
            _ => None,
        })
        .expect("ack");
    let state = app.world().resource::<ServerNetworkState>();
    assert_eq!(server_tick, state.current_tick);
    assert!(server_tick > NetworkTick::ZERO);
    let connected = &state.clients[&player_id];
    assert_eq!(connected.network_entity_id, player_network_id(player_id));
    assert_eq!(connected.connection, client.connection_id());
    let replicated = app.world().get::<Replicated>(connected.entity).unwrap();
    assert_eq!(replicated.id, player_network_id(player_id));
    assert!(replicated.id.0 & server_network::PLAYER_NETWORK_ID_BIT != 0);
    let events = app.world().resource::<Events<ClientConnectedEvent>>();
    assert_eq!(events.len(), 1);
    // The first snapshot already lists the new character.
    assert!(
        matches!(messages.last(), Some(ServerMessage::Snapshot(s)) if s.entities.iter().any(|e| e.entity_id == replicated.id))
    );
}

#[test]
fn repeated_handshake_keeps_the_same_player() {
    let network = LoopbackNetwork::perfect();
    let mut app = server_app(&network);
    let (mut client, player) = join(&mut app, &network);
    client.send(handshake());
    app.update();
    let again = client.receive().into_iter().find_map(|m| match m {
        ServerMessage::HandshakeAck { player_id, .. } => Some(player_id),
        _ => None,
    });
    assert_eq!(again, Some(player));
    assert_eq!(
        app.world().resource::<ServerNetworkState>().clients.len(),
        1
    );
}

#[test]
fn wrong_protocol_version_is_refused() {
    let network = LoopbackNetwork::perfect();
    let mut app = server_app(&network);
    let mut client = network.connect();
    client.send(ClientMessage::Handshake {
        protocol_version: PROTOCOL_VERSION + 1,
    });
    app.update();
    let messages = client.receive();
    assert!(
        matches!(messages.as_slice(), [ServerMessage::Disconnect(reason)] if reason.contains("not supported"))
    );
    assert!(!client.is_connected());
    assert!(app
        .world()
        .resource::<ServerNetworkState>()
        .clients
        .is_empty());
}

#[test]
fn ping_is_echoed() {
    let network = LoopbackNetwork::perfect();
    let mut app = server_app(&network);
    let mut client = network.connect();
    client.send(ClientMessage::Ping(123_456));
    app.update();
    assert!(client.receive().contains(&ServerMessage::Pong(123_456)));
}

#[test]
fn inputs_drive_the_character_one_step_per_tick() {
    let network = LoopbackNetwork::perfect();
    let mut app = server_app(&network);
    let (mut client, player) = join(&mut app, &network);
    for tick in 1..=10 {
        client.send(sprint(tick));
        app.update();
    }
    let state = client_state(&app, player);
    assert_eq!(state.last_processed_tick, Some(NetworkTick(10)));
    assert_eq!(state.last_received_tick, NetworkTick(10));
    let pos = position(&app, player);
    assert!((pos.z + 10.0 * SPRINT_STEP).abs() < 1e-4, "{pos}");
    let last = snapshots(&mut client).pop().expect("snapshot each tick");
    assert_eq!(last.last_processed_client_tick, NetworkTick(10));
    let me = last
        .entities
        .iter()
        .find(|e| e.entity_id == player_network_id(player))
        .unwrap();
    assert_eq!(me.position, pos);
    assert_eq!(me.velocity, Vec3::new(0.0, 0.0, -SPRINT_SPEED));
}

#[test]
fn lost_input_is_filled_with_the_previous_one() {
    let network = LoopbackNetwork::perfect();
    let mut app = server_app(&network);
    let (mut client, player) = join(&mut app, &network);
    client.send(sprint(1));
    client.send(sprint(2));
    client.send(sprint(4)); // tick 3 lost
    app.update();
    assert_eq!(
        client_state(&app, player).last_processed_tick,
        Some(NetworkTick(4))
    );
    assert!(
        (position(&app, player).z + 4.0 * SPRINT_STEP).abs() < 1e-4,
        "gap filled, not skipped"
    );
    // A late copy of tick 3 is ignored.
    client.send(sprint(3));
    app.update();
    assert!((position(&app, player).z + 4.0 * SPRINT_STEP).abs() < 1e-4);
}

#[test]
fn input_bursts_are_rate_limited() {
    let network = LoopbackNetwork::perfect();
    let mut app = server_app(&network);
    let (mut client, player) = join(&mut app, &network);
    for tick in 1..=20 {
        client.send(sprint(tick));
    }
    app.update();
    let max = app
        .world()
        .resource::<ServerNetworkState>()
        .max_input_steps_per_tick as u64;
    assert_eq!(
        client_state(&app, player).last_processed_tick,
        Some(NetworkTick(max))
    );
    for _ in 0..10 {
        app.update();
    }
    assert_eq!(
        client_state(&app, player).last_processed_tick,
        Some(NetworkTick(20)),
        "backlog drains"
    );
    // Far-future ticks are dropped instead of being simulated.
    client.send(sprint(20 + 10_000));
    app.update();
    let state = client_state(&app, player);
    assert_eq!(state.last_processed_tick, Some(NetworkTick(20)));
    assert!(state.pending_inputs.is_empty());
}

#[test]
fn malformed_input_is_sanitized_before_simulation() {
    let network = LoopbackNetwork::perfect();
    let mut app = server_app(&network);
    let (mut client, player) = join(&mut app, &network);
    client.send(ClientMessage::Input(PlayerInputPacket {
        tick: NetworkTick(1),
        movement_vector: Vec2::new(f32::NAN, 1e9),
        yaw: f32::INFINITY,
        actions: u32::MAX,
        ..Default::default()
    }));
    app.update();
    let entity = client_state(&app, player).entity;
    let velocity = app.world().get::<KinematicVelocity>(entity).unwrap().0;
    assert!(position(&app, player).is_finite());
    assert!(velocity.length() <= SPRINT_SPEED + 1e-3);
}

#[test]
fn inputs_before_handshake_are_ignored() {
    let network = LoopbackNetwork::perfect();
    let mut app = server_app(&network);
    let mut stranger = network.connect();
    stranger.send(sprint(1));
    app.update();
    assert!(app
        .world()
        .resource::<ServerNetworkState>()
        .clients
        .is_empty());
    assert!(
        stranger.receive().is_empty(),
        "no snapshots for unknown connections"
    );
}

#[test]
fn rtt_is_estimated_from_input_lag() {
    let network = LoopbackNetwork::perfect();
    let mut app = server_app(&network);
    let (mut client, player) = join(&mut app, &network);
    for _ in 0..20 {
        app.update();
    }
    let now = app.world().resource::<ServerNetworkState>().current_tick;
    // An input stamped 10 ticks behind the clock arrives on the next tick.
    client.send(sprint(now.0 - 10));
    app.update();
    let rtt = client_state(&app, player).rtt_ms;
    assert!((rtt - 11.0 * 15.625).abs() < 1e-3, "{rtt}");
}

#[test]
fn snapshot_rate_can_be_lowered() {
    let network = LoopbackNetwork::perfect();
    let mut app = server_app(&network);
    app.world_mut()
        .resource_mut::<ServerNetworkState>()
        .snapshot_rate_hz = 16;
    let (mut client, _) = join(&mut app, &network);
    client.receive();
    for _ in 0..16 {
        app.update();
    }
    let got = snapshots(&mut client);
    assert_eq!(got.len(), 4, "every 4th tick");
    assert!(got.iter().all(|s| s.server_tick.0 % 4 == 0));
}

#[test]
fn every_client_gets_its_own_ack_tick_and_sees_everyone() {
    let network = LoopbackNetwork::perfect();
    let mut app = server_app(&network);
    let (mut a, pa) = join(&mut app, &network);
    let (mut b, pb) = join(&mut app, &network);
    a.send(sprint(1));
    a.send(sprint(2));
    b.send(sprint(1));
    app.update();
    let sa = snapshots(&mut a).pop().unwrap();
    let sb = snapshots(&mut b).pop().unwrap();
    assert_eq!(
        (sa.last_processed_client_tick, sb.last_processed_client_tick),
        (NetworkTick(2), NetworkTick(1))
    );
    for s in [&sa, &sb] {
        let ids: Vec<_> = s.entities.iter().map(|e| e.entity_id).collect();
        assert!(ids.contains(&player_network_id(pa)) && ids.contains(&player_network_id(pb)));
    }
}

#[test]
fn disconnect_despawns_the_character() {
    let network = LoopbackNetwork::perfect();
    let mut app = server_app(&network);
    let (client, player) = join(&mut app, &network);
    let entity = client_state(&app, player).entity;
    drop(client);
    app.update();
    assert!(app.world().get_entity(entity).is_err());
    assert!(app
        .world()
        .resource::<ServerNetworkState>()
        .clients
        .is_empty());
    assert_eq!(
        app.world()
            .resource::<Events<ClientDisconnectedEvent>>()
            .len(),
        1
    );
}

#[test]
fn lag_compensation_records_every_tick() {
    let network = LoopbackNetwork::perfect();
    let mut app = server_app(&network);
    let (mut client, player) = join(&mut app, &network);
    for tick in 1..=80 {
        client.send(sprint(tick));
        app.update();
    }
    let buffer = app.world().resource::<ServerLagCompensationBuffer>();
    assert_eq!(buffer.history.len(), HISTORY_BUFFER_TICKS);
    let newest = buffer.history.back().unwrap();
    assert_eq!(
        newest.tick,
        app.world().resource::<ServerNetworkState>().current_tick
    );
    let me = player_network_id(player);
    let recorded = newest
        .entity_positions
        .iter()
        .find(|(id, _, _)| *id == me)
        .unwrap()
        .1;
    assert_eq!(recorded, position(&app, player));
}

#[test]
fn local_server_thread_answers_a_client() {
    let network = LoopbackNetwork::perfect();
    let server = spawn_local_server(&network);
    assert!(server.is_running());
    let mut client = network.connect();
    client.send(handshake());
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut acked = false;
    while Instant::now() < deadline && !acked {
        acked = client
            .receive()
            .iter()
            .any(|m| matches!(m, ServerMessage::HandshakeAck { .. }));
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(acked, "in-process server acknowledged within 5 s");
    server.stop();
}
