//! Neon Veil native client.
//!
//! Offline by default: an in-process server runs on its own thread over the
//! loopback transport. `game_client --connect <address>` joins a dedicated
//! server over UDP instead.

use bevy::prelude::*;
use game_client::{
    ClientNetworkPlugin, ClientTransportResource, LocalPredictedPlayer, PredictedVisual,
    RenetClientTransport,
};
use game_input::{default_input_map, local_player_input_bundle, GameInputPlugin};
use game_server::{spawn_local_server, LoopbackNetwork};
use std::net::SocketAddr;

fn connect_address() -> anyhow::Result<Option<SocketAddr>> {
    let args: Vec<String> = std::env::args().collect();
    match args.iter().position(|a| a == "--connect") {
        Some(i) => {
            let addr = args
                .get(i + 1)
                .ok_or_else(|| anyhow::anyhow!("--connect needs an address"))?;
            Ok(Some(addr.parse()?))
        }
        None => Ok(None),
    }
}

/// Gives the predicted player keyboard / gamepad input and a visible body.
fn attach_local_player(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    added: Query<Entity, Added<LocalPredictedPlayer>>,
) {
    for entity in &added {
        commands
            .entity(entity)
            .insert(local_player_input_bundle(default_input_map()));
        commands.spawn((
            PredictedVisual { source: entity },
            Mesh3d(meshes.add(Capsule3d::new(0.4, 1.0))),
            MeshMaterial3d(materials.add(Color::srgb(0.85, 0.2, 0.45))),
            Transform::default(),
        ));
    }
}

fn setup_scene(mut commands: Commands) {
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 6.0, 12.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        DirectionalLight::default(),
        Transform::from_xyz(4.0, 10.0, 4.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_target(false).init();
    let mut app = App::new();
    app.add_plugins((DefaultPlugins, GameInputPlugin, ClientNetworkPlugin))
        .add_systems(Startup, setup_scene)
        .add_systems(Update, attach_local_player);

    // Keeps the offline server alive until the window closes.
    let _local_server = match connect_address()? {
        Some(addr) => {
            let client_id = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos() as u64;
            app.insert_resource(ClientTransportResource::new(RenetClientTransport::connect(
                addr, client_id,
            )?));
            None
        }
        None => {
            let network = LoopbackNetwork::perfect();
            let server = spawn_local_server(&network);
            app.insert_resource(ClientTransportResource::new(network.connect()));
            Some(server)
        }
    };
    app.run();
    Ok(())
}
