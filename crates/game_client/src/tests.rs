//! End-to-end client/server sessions over the in-process network.

use super::*;
use bevy::time::TimeUpdateStrategy;
use game_core::input::PlayerInputState;
use game_core::protocol::{InputActions, PlayerInputPacket};
use game_core::time::{NetworkTick, FIXED_TIMESTEP, FIXED_TIMESTEP_SECS};
use game_server::{
    player_network_id, LoopbackNetwork, NetworkConditions, ServerNetworkState, ServerPlugin,
    ServerTransportResource, SPRINT_SPEED,
};

const STEP: f32 = SPRINT_SPEED * FIXED_TIMESTEP_SECS;

struct Session {
    network: LoopbackNetwork,
    server: App,
    clients: Vec<App>,
}

fn headless(app: &mut App) {
    app.add_plugins(MinimalPlugins)
        .insert_resource(TimeUpdateStrategy::ManualDuration(FIXED_TIMESTEP));
}

impl Session {
    fn new(network: LoopbackNetwork, clients: usize) -> Self {
        let mut server = App::new();
        headless(&mut server);
        server
            .add_plugins(ServerPlugin)
            .insert_resource(ServerTransportResource::new(network.server_transport()));
        server.update();
        let clients = (0..clients)
            .map(|_| {
                let mut client = App::new();
                headless(&mut client);
                client
                    .add_plugins(ClientNetworkPlugin)
                    .insert_resource(ClientTransportResource::new(network.connect()));
                client.update();
                client
            })
            .collect();
        Self {
            network,
            server,
            clients,
        }
    }

    fn step(&mut self) {
        self.server.update();
        for client in &mut self.clients {
            client.update();
        }
    }

    fn state(&self, client: usize) -> &ClientNetworkState {
        self.clients[client]
            .world()
            .resource::<ClientNetworkState>()
    }

    /// Steps until every client has its predicted character.
    fn connect_all(&mut self) -> Vec<Entity> {
        for _ in 0..400 {
            if (0..self.clients.len()).all(|i| self.state(i).local_entity.is_some()) {
                break;
            }
            self.step();
        }
        // One more step so spawn commands have been applied everywhere.
        self.step();
        (0..self.clients.len())
            .map(|i| self.state(i).local_entity.expect("connected"))
            .collect()
    }

    fn set_input(&mut self, client: usize, packet: PlayerInputPacket) {
        let entity = self.state(client).local_entity.unwrap();
        self.clients[client]
            .world_mut()
            .get_mut::<PlayerInputState>(entity)
            .unwrap()
            .push(packet);
    }

    fn prediction(&self, client: usize) -> ClientPredictionState {
        let entity = self.state(client).local_entity.unwrap();
        self.clients[client]
            .world()
            .get::<ClientPredictionState>(entity)
            .unwrap()
            .clone()
    }

    fn simulated(&self, client: usize) -> Vec3 {
        let entity = self.state(client).local_entity.unwrap();
        self.clients[client]
            .world()
            .get::<Transform>(entity)
            .unwrap()
            .translation
    }

    fn visual(&self, client: usize) -> Vec3 {
        self.prediction(client)
            .visual_position(self.simulated(client))
    }

    /// Server-side position and last applied input tick for a client.
    fn authoritative(&self, client: usize) -> (Vec3, NetworkTick) {
        let player = self.state(client).player_id.unwrap();
        let server = self.server.world().resource::<ServerNetworkState>();
        let connected = &server.clients[&player];
        let pos = self
            .server
            .world()
            .get::<Transform>(connected.entity)
            .unwrap()
            .translation;
        (pos, connected.last_processed_tick.unwrap_or_default())
    }
}

fn sprint(yaw: f32) -> PlayerInputPacket {
    PlayerInputPacket {
        movement_vector: Vec2::Y,
        yaw,
        actions: InputActions::SPRINT,
        ..Default::default()
    }
}

#[test]
fn offline_loopback_session_connects_and_predicts() {
    let mut session = Session::new(LoopbackNetwork::perfect(), 1);
    let entity = session.connect_all()[0];
    let state = session.state(0);
    assert_eq!(state.status, ConnectionStatus::Connected);
    let player = state.player_id.unwrap();
    let local = session.clients[0]
        .world()
        .get::<LocalPredictedPlayer>(entity)
        .unwrap();
    assert_eq!(local.network_id, player_network_id(player));
    assert!(session
        .server
        .world()
        .resource::<ServerNetworkState>()
        .clients
        .contains_key(&player));

    session.set_input(0, sprint(0.0));
    for _ in 0..32 {
        session.step();
    }
    let (server_pos, acked) = session.authoritative(0);
    let predicted = session.prediction(0).predicted_position_at(acked);
    // Zero latency: the server is at most one tick behind and agrees exactly.
    assert!(predicted.is_some_and(|p| p.distance(server_pos) < 1e-5));
    assert!(session.prediction(0).input_history.len() <= 2);
    assert_eq!(session.prediction(0).corrections, 0);
}

#[test]
fn prediction_stays_smooth_at_150_ms_rtt() {
    let network = LoopbackNetwork::simulated(NetworkConditions::with_rtt_ms(150.0), 3);
    let mut session = Session::new(network, 1);
    session.connect_all();
    session.set_input(0, sprint(0.0));

    let mut visual = vec![session.visual(0)];
    for _ in 0..60 {
        session.step();
        visual.push(session.visual(0));
    }
    // Input feels instant even though the server hears about it ~75 ms
    // later: the body speeds up every frame, then holds one sprint step.
    let mut last = 0.0;
    for (i, pair) in visual.windows(2).enumerate() {
        let moved = pair[1].distance(pair[0]);
        assert!(
            moved > 0.0 && moved >= last - 1e-5,
            "frame {i}: moved {moved}"
        );
        assert!(moved <= STEP + 1e-4, "frame {i}: moved {moved}");
        if i >= 16 {
            assert!((moved - STEP).abs() < 1e-4, "frame {i}: moved {moved}");
        }
        last = moved;
    }
    let prediction = session.prediction(0);
    assert_eq!(
        prediction.corrections, 0,
        "deterministic model never mispredicts"
    );
    let unconfirmed = prediction.input_history.len();
    assert!(
        (8..=14).contains(&unconfirmed),
        "about one RTT of inputs in flight: {unconfirmed}"
    );

    // The server agrees with what the client predicted for the same tick.
    let (server_pos, acked) = session.authoritative(0);
    assert!(acked > NetworkTick::ZERO);
    let client_view = prediction.predicted_position_at(acked);
    assert!(client_view.is_some_and(|p| p.distance(server_pos) < 1e-4));

    // Ping measures the configured round trip (within a tick of jitter
    // from update order), and the server's estimate agrees.
    session.step();
    let rtt = session.state(0).rtt_ms.expect("pong received");
    assert!(
        (rtt - 150.0).abs() <= 2.0 * 15.625 + 1.0,
        "client rtt {rtt}"
    );
    let player = session.state(0).player_id.unwrap();
    let server_rtt = session
        .server
        .world()
        .resource::<ServerNetworkState>()
        .clients[&player]
        .rtt_ms;
    assert!(
        (server_rtt - 150.0).abs() <= 2.0 * 15.625 + 1.0,
        "server rtt {server_rtt}"
    );
}

#[test]
fn two_percent_loss_does_not_cause_visible_jumps() {
    let conditions = NetworkConditions::with_rtt_ms(150.0).with_drop_rate(0.02);
    let mut session = Session::new(LoopbackNetwork::simulated(conditions, 0xBEEF), 1);
    session.connect_all();
    let mut visual = vec![session.visual(0)];
    for step in 0..400 {
        // Turn a quarter every 40 ticks so lost inputs actually matter.
        if step % 40 == 0 {
            session.set_input(0, sprint(std::f32::consts::FRAC_PI_2 * (step / 40) as f32));
        }
        session.step();
        visual.push(session.visual(0));
    }
    assert!(
        session.network.stats().dropped > 0,
        "the link really lost packets"
    );
    let worst = visual
        .windows(2)
        .map(|w| w[1].distance(w[0]))
        .fold(0.0f32, f32::max);
    assert!(
        worst <= STEP + 0.05,
        "largest per-frame visual move {worst} m"
    );
    let (server_pos, acked) = session.authoritative(0);
    let prediction = session.prediction(0);
    let client_view = prediction
        .predicted_position_at(acked)
        .expect("acked tick in history");
    assert!(client_view.distance(server_pos) < ERROR_CORRECTION_THRESHOLD_METERS);
}

#[test]
fn server_correction_is_absorbed_visually() {
    let mut session = Session::new(LoopbackNetwork::perfect(), 1);
    let entity = session.connect_all()[0];
    let visual_entity = session.clients[0]
        .world_mut()
        .spawn((PredictedVisual { source: entity }, Transform::default()))
        .id();
    session.set_input(0, sprint(0.0));
    for _ in 0..10 {
        session.step();
    }
    // Knock the authoritative character half a metre sideways.
    let player = session.state(0).player_id.unwrap();
    let server_entity = session
        .server
        .world()
        .resource::<ServerNetworkState>()
        .clients[&player]
        .entity;
    session
        .server
        .world_mut()
        .get_mut::<Transform>(server_entity)
        .unwrap()
        .translation
        .x += 0.5;

    let shown = |s: &Session| {
        s.clients[0]
            .world()
            .get::<Transform>(visual_entity)
            .unwrap()
            .translation
    };
    let mut frames = vec![shown(&session)];
    for _ in 0..40 {
        session.step();
        frames.push(shown(&session));
    }
    let prediction = session.prediction(0);
    assert_eq!(prediction.corrections, 1);
    assert!(
        (session.simulated(0).x - 0.5).abs() < 1e-4,
        "simulation snapped to the server"
    );
    let worst = frames
        .windows(2)
        .map(|w| w[1].distance(w[0]))
        .fold(0.0f32, f32::max);
    assert!(
        worst < STEP + 0.5 * 0.4,
        "the 0.5 m correction is spread over frames ({worst})"
    );
    assert!(
        (frames.last().unwrap().x - 0.5).abs() < 1e-3,
        "visual converged to the corrected path"
    );
    assert!(prediction.visual_error_offset.length() < 1e-3);
}

#[test]
fn handshake_is_retried_until_the_link_works() {
    let network = LoopbackNetwork::simulated(NetworkConditions::PERFECT.with_drop_rate(1.0), 1);
    let mut session = Session::new(network.clone(), 1);
    for _ in 0..30 {
        session.step();
    }
    assert_eq!(session.state(0).status, ConnectionStatus::Connecting);
    network.set_conditions(NetworkConditions::PERFECT);
    for _ in 0..70 {
        session.step();
    }
    assert_eq!(
        session.state(0).status,
        ConnectionStatus::Connected,
        "retried within a second"
    );
}

#[test]
fn players_see_each_other_as_remote_entities() {
    let mut session = Session::new(
        LoopbackNetwork::simulated(NetworkConditions::with_rtt_ms(60.0), 5),
        2,
    );
    session.connect_all();
    session.set_input(1, sprint(0.0));
    for _ in 0..40 {
        session.step();
    }
    let other = player_network_id(session.state(1).player_id.unwrap());
    let world = session.clients[0].world_mut();
    let remotes: Vec<(RemoteEntity, Vec3)> = world
        .query::<(&RemoteEntity, &Transform)>()
        .iter(world)
        .map(|(r, t)| (*r, t.translation))
        .collect();
    assert_eq!(remotes.len(), 1, "only the other player, not ourselves");
    assert_eq!(remotes[0].0.network_id, other);
    assert!(remotes[0].1.z < -1.0, "remote player seen moving");
    let (server_pos, _) = session.authoritative(1);
    assert!(
        remotes[0].1.z > server_pos.z - 1e-4,
        "remote view trails the server by the latency"
    );
}

#[test]
fn full_client_boots_headless_and_joins_its_offline_server() {
    use std::time::{Duration, Instant};
    let mut app = build_client_app();
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        app.update();
        let state = app.world().resource::<ClientNetworkState>();
        if state.status == ConnectionStatus::Connected && state.local_entity.is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    for _ in 0..30 {
        app.update();
    }
    let world = app.world_mut();
    let state = world.resource::<ClientNetworkState>();
    assert_eq!(
        state.status,
        ConnectionStatus::Connected,
        "offline handshake"
    );
    assert!(
        state.snapshots_received > 0,
        "snapshots arriving from the local server"
    );
    assert!(world.contains_resource::<OfflineSession>());
    assert!(world.contains_resource::<game_world::WorldChunkStreamer>());
    assert!(world.contains_resource::<game_audio::AudioMixerSettings>());
    assert!(world.contains_resource::<game_ui::SkillsModalState>());
    assert!(world.contains_resource::<game_economy::GrandExchangeResource>());
    assert!(world.contains_resource::<game_steam::PlatformServiceResource>());
    assert_eq!(
        world.query::<&game_ui::HudRootNode>().iter(world).count(),
        1
    );
    assert!(
        !world
            .resource::<game_world::WorldChunkStreamer>()
            .loaded_chunks
            .is_empty(),
        "chunks stream around the local player"
    );
}
