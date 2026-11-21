//! Authoritative game server: transports, the 64 Hz network loop, lag
//! compensation and persistence. Runs as the dedicated server binary or
//! in-process for offline play ([`spawn_local_server`]).

pub mod lag_compensation;
pub mod movement;
pub mod persistence;
pub mod server_network;
pub mod transport;

pub use lag_compensation::{
    HistoricalHitboxFrame, ServerLagCompensationBuffer, HISTORY_BUFFER_TICKS,
};
pub use movement::{simulate_movement_step, NetworkMovement, SPRINT_SPEED, WALK_SPEED};
pub use persistence::{PersistenceEngine, PlayerSaveRecord};
pub use server_network::{
    player_network_id, ClientConnectedEvent, ClientDisconnectedEvent, ConnectedClient,
    NetworkedPlayer, Replicated, ServerNetworkState, ServerTransportResource,
};
pub use transport::{
    ClientTransport, ConnectionId, LoopbackClientTransport, LoopbackNetwork,
    LoopbackServerTransport, NetworkConditions, RenetServerTransport, ServerTransport,
};

use bevy::app::{AppExit, ScheduleRunnerPlugin};
use bevy::prelude::*;
use game_core::time::FIXED_TIMESTEP;
use game_core::{CoreGameplayPlugin, CoreSimulationSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

/// Server network loop. Requires a [`ServerTransportResource`]; without one
/// the loop is idle.
pub struct ServerPlugin;

impl Plugin for ServerPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<CoreGameplayPlugin>() {
            app.add_plugins(CoreGameplayPlugin);
        }
        let has_transport = resource_exists::<ServerTransportResource>;
        app.init_resource::<ServerNetworkState>()
            .init_resource::<ServerLagCompensationBuffer>()
            .register_type::<NetworkMovement>()
            .add_event::<ClientConnectedEvent>()
            .add_event::<ClientDisconnectedEvent>()
            .add_systems(
                FixedUpdate,
                (
                    (
                        server_network::sync_server_tick,
                        server_network::handle_disconnections.run_if(has_transport),
                        server_network::process_incoming_packets.run_if(has_transport),
                        server_network::assign_network_ids,
                    )
                        .chain()
                        .in_set(CoreSimulationSet::Input),
                    server_network::apply_client_inputs.in_set(CoreSimulationSet::Simulate),
                    (
                        lag_compensation::record_lag_compensation_frame,
                        server_network::broadcast_world_snapshots.run_if(has_transport),
                    )
                        .chain()
                        .in_set(CoreSimulationSet::Sanitize),
                ),
            );
    }
}

/// Headless server app around `transport`, stepping at 64 Hz in real time.
pub fn build_server_app(transport: impl ServerTransport) -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(FIXED_TIMESTEP / 4)),
        TransformPlugin,
        HierarchyPlugin,
        ServerPlugin,
    ))
    .insert_resource(ServerTransportResource::new(transport));
    app
}

#[derive(Resource)]
struct StopSignal(Arc<AtomicBool>);

fn exit_on_stop(signal: Res<StopSignal>, mut exit: EventWriter<AppExit>) {
    if signal.0.load(Ordering::Relaxed) {
        exit.send(AppExit::Success);
    }
}

/// An in-process server on its own thread. Dropping it stops the server.
pub struct LocalServerHandle {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl LocalServerHandle {
    pub fn is_running(&self) -> bool {
        self.thread.as_ref().is_some_and(|t| !t.is_finished())
    }

    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for LocalServerHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Starts the offline-mode server on `network`. Clients join with
/// `network.connect()`.
pub fn spawn_local_server(network: &LoopbackNetwork) -> LocalServerHandle {
    let stop = Arc::new(AtomicBool::new(false));
    let transport = network.server_transport();
    let signal = Arc::clone(&stop);
    let thread = std::thread::Builder::new()
        .name("neon-veil-local-server".into())
        .spawn(move || {
            let mut app = build_server_app(transport);
            app.insert_resource(StopSignal(signal))
                .add_systems(First, exit_on_stop);
            app.run();
        })
        .expect("spawn local server thread");
    LocalServerHandle {
        stop,
        thread: Some(thread),
    }
}

#[cfg(test)]
mod tests;
