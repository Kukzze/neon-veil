//! Client app assembly: the full gameplay plugin stack, the session
//! transport (in-process offline server or UDP), and the windowed
//! presentation layer.

use crate::client_network::{ClientTransportResource, LocalPredictedPlayer, PredictedVisual};
use crate::transport::RenetClientTransport;
use crate::ClientNetworkPlugin;
use avian3d::prelude::PhysicsPlugins;
use bevy::prelude::*;
use bevy::window::{PresentMode, WindowResolution};
use game_ai::GameAiPlugin;
use game_audio::AudioPlugin;
use game_camera::{CameraRigController, CameraRigPlugin};
use game_combat::CombatPlugin;
use game_core::CoreGameplayPlugin;
use game_economy::EconomyPlugin;
use game_input::{default_input_map, local_player_input_bundle, GameInputPlugin};
use game_parkour::ParkourPlugin;
use game_rpg::RpgPlugin;
use game_server::{spawn_local_server, LocalServerHandle, LoopbackNetwork};
use game_steam::SteamworksPlugin;
use game_ui::UiPlugin;
use game_world::{GreyboxGymPlugin, WorldPlugin};
use std::net::SocketAddr;

pub const WINDOW_TITLE: &str = "Neon Veil: Zero Protocol";
pub const WINDOW_RESOLUTION: (f32, f32) = (1920.0, 1080.0);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SessionMode {
    /// Single player: an in-process server over the loopback transport.
    #[default]
    Offline,
    /// Join a dedicated server over UDP.
    Connect(SocketAddr),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ClientOptions {
    pub session: SessionMode,
    /// Load the greybox test gym and developer tools.
    pub greybox: bool,
}

/// Parses `[--connect <address>] [--greybox]`.
pub fn parse_client_args<I, S>(args: I) -> Result<ClientOptions, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut options = ClientOptions::default();
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_ref() {
            "--greybox" => options.greybox = true,
            "--connect" => {
                let addr = args
                    .next()
                    .ok_or("--connect needs an address, e.g. 127.0.0.1:5000")?;
                let addr = addr
                    .as_ref()
                    .parse()
                    .map_err(|e| format!("invalid server address {:?}: {e}", addr.as_ref()))?;
                options.session = SessionMode::Connect(addr);
            }
            other => return Err(format!("unknown argument {other:?}")),
        }
    }
    Ok(options)
}

/// Keeps the in-process server alive for the app's lifetime; dropping the
/// app stops it.
#[derive(Resource)]
pub struct OfflineSession {
    pub network: LoopbackNetwork,
    pub server: LocalServerHandle,
}

fn add_once<P: Plugin>(app: &mut App, plugin: P) {
    if !app.is_plugin_added::<P>() {
        app.add_plugins(plugin);
    }
}

/// Every gameplay plugin, dependencies first. Expects the platform layer
/// (`DefaultPlugins`, or the headless subset) to be present already.
pub fn add_gameplay_plugins(app: &mut App) {
    add_once(app, CoreGameplayPlugin);
    app.add_plugins(PhysicsPlugins::default());
    add_once(app, ParkourPlugin);
    add_once(app, CombatPlugin);
    add_once(app, RpgPlugin);
    add_once(app, GameAiPlugin);
    add_once(app, EconomyPlugin);
    add_once(app, WorldPlugin);
    add_once(app, CameraRigPlugin);
    add_once(app, GameInputPlugin);
    add_once(app, AudioPlugin);
    add_once(app, UiPlugin);
    add_once(app, SteamworksPlugin);
    add_once(app, ClientNetworkPlugin);
}

/// Installs the transport for `session`; offline starts the local server.
pub fn install_session(app: &mut App, session: SessionMode) -> anyhow::Result<()> {
    match session {
        SessionMode::Offline => {
            let network = LoopbackNetwork::perfect();
            let server = spawn_local_server(&network);
            app.insert_resource(ClientTransportResource::new(network.connect()))
                .insert_resource(OfflineSession { network, server });
        }
        SessionMode::Connect(addr) => {
            let client_id = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos() as u64;
            app.insert_resource(ClientTransportResource::new(RenetClientTransport::connect(
                addr, client_id,
            )?));
        }
    }
    Ok(())
}

/// Headless client (no window, renderer or audio device) with the full
/// gameplay stack and an offline session, for smoke tests and bots.
pub fn build_client_app() -> App {
    build_headless_client_app(ClientOptions::default()).expect("offline session needs no I/O")
}

pub fn build_headless_client_app(options: ClientOptions) -> anyhow::Result<App> {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        TransformPlugin,
        HierarchyPlugin,
        bevy::input::InputPlugin,
        bevy::asset::AssetPlugin::default(),
        bevy::scene::ScenePlugin,
    ))
    .init_asset::<Mesh>()
    .init_asset::<StandardMaterial>();
    add_gameplay_plugins(&mut app);
    if options.greybox {
        add_once(&mut app, GreyboxGymPlugin);
    }
    install_session(&mut app, options.session)?;
    Ok(app)
}

/// The shipping client: window, renderer, egui panels and presentation.
pub fn build_windowed_client_app(options: ClientOptions) -> anyhow::Result<App> {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: WINDOW_TITLE.into(),
            resolution: WindowResolution::new(WINDOW_RESOLUTION.0, WINDOW_RESOLUTION.1),
            present_mode: PresentMode::AutoVsync,
            ..default()
        }),
        ..default()
    }))
    .add_plugins(bevy_egui::EguiPlugin);
    add_gameplay_plugins(&mut app);
    if options.greybox {
        add_once(&mut app, GreyboxGymPlugin);
        add_once(&mut app, game_dev_tools::DevToolsPlugin);
    }
    app.add_systems(Startup, spawn_world_lighting)
        .add_systems(Update, attach_local_player_presentation);
    install_session(&mut app, options.session)?;
    Ok(app)
}

fn spawn_world_lighting(mut commands: Commands) {
    commands.spawn((
        DirectionalLight {
            illuminance: 12_000.0,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_xyz(30.0, 60.0, 20.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.insert_resource(AmbientLight {
        color: Color::srgb(0.55, 0.7, 0.8),
        brightness: 180.0,
    });
    // The camera exists from the first frame (the HUD renders through it)
    // and starts following the player once the session spawns them.
    commands.spawn((
        Camera3d::default(),
        CameraRigController::default(),
        Transform::from_xyz(0.0, 3.0, 6.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

/// Gives the predicted player controls and a visible body, and points the
/// camera rig at them.
pub fn attach_local_player_presentation(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    added: Query<Entity, Added<LocalPredictedPlayer>>,
    mut rigs: Query<&mut CameraRigController>,
) {
    for player in &added {
        for mut rig in &mut rigs {
            rig.target = Some(player);
        }
        commands
            .entity(player)
            .insert(local_player_input_bundle(default_input_map()));
        commands.spawn((
            PredictedVisual { source: player },
            Mesh3d(meshes.add(Capsule3d::new(0.4, 1.0))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgb(0.85, 0.2, 0.45),
                perceptual_roughness: 0.4,
                ..default()
            })),
            Transform::default(),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments() {
        assert_eq!(
            parse_client_args(Vec::<String>::new()),
            Ok(ClientOptions::default())
        );
        assert_eq!(
            parse_client_args(["--greybox"]),
            Ok(ClientOptions {
                greybox: true,
                ..default()
            })
        );
        let addr: SocketAddr = "10.0.0.5:5000".parse().unwrap();
        assert_eq!(
            parse_client_args(["--connect", "10.0.0.5:5000", "--greybox"]),
            Ok(ClientOptions {
                session: SessionMode::Connect(addr),
                greybox: true
            })
        );
        assert!(parse_client_args(["--connect"]).is_err());
        assert!(parse_client_args(["--connect", "not-an-address"]).is_err());
        assert!(parse_client_args(["--fullscreen"]).is_err());
    }
}
