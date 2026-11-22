//! Client app assembly: the full gameplay plugin stack, the session
//! transport (in-process offline server or UDP), and the windowed
//! presentation layer.

use crate::client_network::ClientTransportResource;
use crate::transport::RenetClientTransport;
use crate::ClientNetworkPlugin;
use avian3d::prelude::PhysicsPlugins;
use bevy::prelude::*;
use bevy::window::{PresentMode, WindowResolution};
use game_ai::GameAiPlugin;
use game_audio::AudioPlugin;
use game_camera::CameraRigPlugin;
use game_combat::CombatPlugin;
use game_core::CoreGameplayPlugin;
use game_economy::EconomyPlugin;
use game_input::GameInputPlugin;
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

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ClientOptions {
    pub session: SessionMode,
    /// Load the greybox test gym and developer tools.
    pub greybox: bool,
    /// Save one rendered frame here after the scene settles, then exit
    /// (visual QA and store screenshots).
    pub capture: Option<std::path::PathBuf>,
    /// Capture view as whole degrees (yaw, pitch); defaults to [`CAPTURE_VIEW`].
    pub capture_view: Option<(i32, i32)>,
    /// UI panel to show open in the capture.
    pub open_panel: Option<UiPanel>,
    /// Start Night One at this encounter (1-3, 4 for the Gorgon).
    pub checkpoint: Option<u8>,
    /// Seconds to wait before the capture (default 6).
    pub capture_after: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiPanel {
    Skills,
    Inventory,
    Settings,
    Pause,
    Controls,
}

/// Parses `[--connect <address>] [--greybox] [--capture <file.png>]
/// [--capture-view <yaw>,<pitch>] [--open-panel skills|inventory|settings]`.
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
            "--capture" => {
                let path = args
                    .next()
                    .ok_or("--capture needs a file path, e.g. shot.png")?;
                options.capture = Some(path.as_ref().into());
            }
            "--capture-view" => {
                let spec = args
                    .next()
                    .ok_or("--capture-view needs <yaw>,<pitch> in degrees")?;
                let (yaw, pitch) = spec
                    .as_ref()
                    .split_once(',')
                    .and_then(|(y, p)| Some((y.trim().parse().ok()?, p.trim().parse().ok()?)))
                    .ok_or_else(|| {
                        format!(
                            "invalid --capture-view {:?}, expected e.g. 20,-5",
                            spec.as_ref()
                        )
                    })?;
                options.capture_view = Some((yaw, pitch));
            }
            "--capture-after" => {
                let secs = args
                    .next()
                    .and_then(|s| s.as_ref().parse().ok())
                    .filter(|s| (1..=600).contains(s))
                    .ok_or("--capture-after needs whole seconds from 1 to 600")?;
                options.capture_after = Some(secs);
            }
            "--checkpoint" => {
                let n: u8 = args
                    .next()
                    .and_then(|n| n.as_ref().parse().ok())
                    .filter(|n| (1..=4).contains(n))
                    .ok_or("--checkpoint needs an encounter number from 1 to 4")?;
                options.checkpoint = Some(n);
            }
            "--open-panel" => {
                options.open_panel = Some(match args.next().as_ref().map(AsRef::as_ref) {
                    Some("skills") => UiPanel::Skills,
                    Some("inventory") => UiPanel::Inventory,
                    Some("settings") => UiPanel::Settings,
                    Some("pause") => UiPanel::Pause,
                    Some("controls") => UiPanel::Controls,
                    _ => {
                        return Err(
                            "--open-panel needs skills, inventory, settings, pause or controls"
                                .into(),
                        )
                    }
                });
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

/// Where game assets live: `assets/` next to the executable (shipped
/// builds), else the workspace `assets/` (development runs).
pub fn asset_root() -> std::path::PathBuf {
    let beside_exe = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("assets")));
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    beside_exe
        .filter(|p| p.is_dir())
        .or_else(|| workspace.is_dir().then_some(workspace))
        .unwrap_or_else(|| "assets".into())
}

/// The shipping client: window, renderer, egui panels and presentation.
pub fn build_windowed_client_app(options: ClientOptions) -> anyhow::Result<App> {
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(bevy::asset::AssetPlugin {
                file_path: asset_root().to_string_lossy().into_owned(),
                ..default()
            })
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: WINDOW_TITLE.into(),
                    resolution: WindowResolution::new(WINDOW_RESOLUTION.0, WINDOW_RESOLUTION.1),
                    present_mode: PresentMode::AutoVsync,
                    ..default()
                }),
                ..default()
            }),
    )
    .add_plugins(bevy_egui::EguiPlugin);
    add_gameplay_plugins(&mut app);
    if options.greybox {
        add_once(&mut app, GreyboxGymPlugin);
        add_once(&mut app, game_dev_tools::DevToolsPlugin);
    } else {
        add_once(&mut app, game_world::NeonDistrictPlugin);
        // The story mission runs its encounters locally, so it is offline only.
        if options.session == SessionMode::Offline {
            app.add_plugins(crate::mission::MissionPlugin);
            if let Some(n) = options.checkpoint {
                app.insert_resource(crate::mission::StartAtEncounter(n));
            }
        }
    }
    app.add_plugins((
        crate::visuals::VisualsPlugin,
        crate::enemy_visuals::EnemyVisualsPlugin,
        game_audio::AudioOutputPlugin,
    ))
    .insert_resource(game_audio::VoiceAssetRoot(asset_root()));
    if let Some(path) = options.capture {
        let view = options.capture_view.map_or(CAPTURE_VIEW, |(yaw, pitch)| {
            ((yaw as f32).to_radians(), (pitch as f32).to_radians())
        });
        let mut capture = FrameCapture::new(path).with_view(view);
        capture.open_panel = options.open_panel;
        if let Some(secs) = options.capture_after {
            capture.after_seconds = secs as f32;
        }
        app.insert_resource(capture)
            .add_systems(Update, capture_frame_and_exit);
    }
    install_session(&mut app, options.session)?;
    Ok(app)
}

/// View yaw and pitch used for captures.
pub const CAPTURE_VIEW: (f32, f32) = (0.0, -0.06);

/// Saves one frame after the scene settles, then exits.
#[derive(Resource, Debug)]
pub struct FrameCapture {
    pub path: std::path::PathBuf,
    pub after_seconds: f32,
    pub requested_at: Option<f32>,
    /// View yaw and pitch held during the capture, radians.
    pub view: (f32, f32),
    /// Panel opened shortly before the shot (opening it at launch would
    /// pause the game before the session connects).
    pub open_panel: Option<UiPanel>,
}

fn open_ui_panel(world: &mut World, panel: UiPanel) {
    match panel {
        UiPanel::Skills => world.resource_mut::<game_ui::SkillsModalState>().is_open = true,
        UiPanel::Inventory => world.resource_mut::<game_ui::InventoryModalState>().is_open = true,
        UiPanel::Settings => world.resource_mut::<game_ui::SettingsModalState>().is_open = true,
        UiPanel::Pause => world.resource_mut::<game_ui::PauseMenuState>().open(),
        UiPanel::Controls => {
            let mut pause = world.resource_mut::<game_ui::PauseMenuState>();
            pause.open();
            pause.page = game_ui::PausePage::Controls;
        }
    }
}

impl FrameCapture {
    pub fn new(path: std::path::PathBuf) -> Self {
        Self {
            path,
            after_seconds: 6.0,
            requested_at: None,
            view: CAPTURE_VIEW,
            open_panel: None,
        }
    }

    pub fn with_view(mut self, view: (f32, f32)) -> Self {
        self.view = view;
        self
    }
}

fn capture_frame_and_exit(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut capture: ResMut<FrameCapture>,
    mut exit: EventWriter<AppExit>,
    mut views: Query<
        &mut game_core::input::ViewAngles,
        With<crate::client_network::LocalPredictedPlayer>,
    >,
) {
    use bevy::render::view::screenshot::{save_to_disk, Screenshot};
    // Repeatable framing: look straight up the avenue, slightly down.
    for mut view in &mut views {
        *view = game_core::input::ViewAngles::new(capture.view.0, capture.view.1);
    }
    let now = time.elapsed_secs();
    if now >= capture.after_seconds - 1.5 {
        if let Some(panel) = capture.open_panel.take() {
            commands.queue(move |world: &mut World| open_ui_panel(world, panel));
        }
    }
    match capture.requested_at {
        None if now >= capture.after_seconds => {
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(capture.path.clone()));
            capture.requested_at = Some(now);
        }
        Some(at) if now - at > 2.0 => {
            exit.send(AppExit::Success);
        }
        _ => {}
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
                greybox: true,
                capture: None,
                capture_view: None,
                open_panel: None,
                checkpoint: None,
                capture_after: None
            })
        );
        assert_eq!(
            parse_client_args(["--capture", "shot.png"]).map(|o| o.capture),
            Ok(Some("shot.png".into()))
        );
        assert!(parse_client_args(["--capture"]).is_err());
        assert!(parse_client_args(["--connect"]).is_err());
        assert!(parse_client_args(["--connect", "not-an-address"]).is_err());
        assert!(parse_client_args(["--fullscreen"]).is_err());
    }
}
