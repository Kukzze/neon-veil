//! Game client: app assembly, transports and client-side prediction
//! against the authoritative server (in-process for offline play, UDP
//! otherwise).

pub mod app;
pub mod character;
pub mod client_network;
pub mod combat_fx;
pub mod creator;
pub mod enemy_visuals;
pub mod gameplay_audio;
pub mod mission;
pub mod transport;
pub mod tutorial;
pub mod visuals;

pub use app::{
    add_gameplay_plugins, build_client_app, build_headless_client_app, build_windowed_client_app,
    parse_client_args, ClientOptions, OfflineSession, SessionMode,
};

pub use client_network::{
    reconcile_with_server_state, ClientNetworkState, ClientPredictionState,
    ClientTransportResource, ConnectedToServerEvent, ConnectionStatus, LocalPredictedPlayer,
    PredictedVisual, ReconcileOutcome, RemoteEntity, ERROR_CORRECTION_THRESHOLD_METERS,
    INPUT_BUFFER_CAPACITY, VISUAL_DECAY_RATE,
};
pub use transport::RenetClientTransport;

use bevy::prelude::*;
use game_core::{CoreGameplayPlugin, CoreSimulationSet};

/// Client network loop. Requires a [`ClientTransportResource`]; without
/// one the loop is idle.
pub struct ClientNetworkPlugin;

impl Plugin for ClientNetworkPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<CoreGameplayPlugin>() {
            app.add_plugins(CoreGameplayPlugin);
        }
        let has_transport = resource_exists::<ClientTransportResource>;
        app.init_resource::<ClientNetworkState>()
            .add_event::<ConnectedToServerEvent>()
            .add_systems(
                FixedUpdate,
                (
                    (
                        client_network::pump_client_transport,
                        client_network::reconcile_server_snapshot,
                        client_network::sync_remote_entities,
                        client_network::sample_and_record_input,
                    )
                        .chain()
                        .in_set(CoreSimulationSet::Input)
                        .run_if(has_transport),
                    client_network::predict_local_movement.in_set(CoreSimulationSet::Simulate),
                    client_network::flush_client_transport
                        .in_set(CoreSimulationSet::Sanitize)
                        .run_if(has_transport),
                ),
            )
            .add_systems(Update, client_network::apply_visual_error_decay);
    }
}

#[cfg(test)]
mod tests;
