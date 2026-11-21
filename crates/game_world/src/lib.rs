//! Megacity world: districts, chunk streaming, spatial index, mesh LOD,
//! procedural traversal props and the greybox gym.

pub mod chunk_loader;
pub mod district_manifest;
pub mod greybox_gym;
pub mod lod_system;
pub mod procedural_streets;
pub mod spatial_index;

pub use chunk_loader::{
    ChunkAssetManifest, ChunkCoordinate, ChunkState, WorldChunk, WorldChunkStreamer,
    CACHE_DISTANCE_CHUNKS, CHUNK_SIZE_METERS, RENDER_DISTANCE_CHUNKS, SIMULATION_DISTANCE_CHUNKS,
};
pub use district_manifest::{get_district_at_position, DistrictId, DistrictZone, DISTRICT_CATALOG};
pub use greybox_gym::GreyboxGymPlugin;
pub use lod_system::{LODComponent, LODLevel, LODQuality, LODSettings};
pub use procedural_streets::{
    BounceAwning, ConveyorBelt, Skybridge, SkybridgeLayout, SteamVentGeyser, TraversalVolume,
};
pub use spatial_index::{OctreeAABB, OctreeNode, SpatialIndexed, WorldSpatialIndex};

use bevy::prelude::*;
use game_core::CoreSimulationSet;
use game_parkour::controller::update_parkour_controller;

/// Streaming and LOD run every frame; the spatial index rebuild and the
/// traversal props run in the fixed simulation tick.
pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<chunk_loader::WorldChunkStreamer>()
            .init_resource::<chunk_loader::ChunkAssetManifest>()
            .init_resource::<spatial_index::WorldSpatialIndex>()
            .init_resource::<lod_system::LODSettings>()
            .register_type::<DistrictId>()
            .register_type::<ChunkCoordinate>()
            .register_type::<ChunkState>()
            .register_type::<WorldChunk>()
            .register_type::<SpatialIndexed>()
            .register_type::<LODLevel>()
            .register_type::<LODQuality>()
            .register_type::<LODComponent>()
            .register_type::<LODSettings>()
            .register_type::<TraversalVolume>()
            .register_type::<Skybridge>()
            .register_type::<BounceAwning>()
            .register_type::<SteamVentGeyser>()
            .register_type::<ConveyorBelt>()
            .add_systems(
                Update,
                (
                    chunk_loader::update_chunk_streaming,
                    lod_system::update_lod_system,
                ),
            )
            .add_systems(
                FixedUpdate,
                (
                    spatial_index::rebuild_world_spatial_index.in_set(CoreSimulationSet::Input),
                    (
                        procedural_streets::advance_steam_vents,
                        procedural_streets::apply_launch_surfaces,
                    )
                        .chain()
                        .before(update_parkour_controller)
                        .in_set(CoreSimulationSet::Simulate),
                    procedural_streets::apply_conveyor_belts
                        .after(update_parkour_controller)
                        .in_set(CoreSimulationSet::Simulate),
                ),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_registers_resources_and_runs_headless() {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            TransformPlugin,
            HierarchyPlugin,
            WorldPlugin,
        ));
        for _ in 0..3 {
            app.update();
        }
        let world = app.world();
        assert!(world.contains_resource::<WorldChunkStreamer>());
        assert!(world.contains_resource::<ChunkAssetManifest>());
        assert!(world.contains_resource::<WorldSpatialIndex>());
        assert_eq!(world.resource::<LODSettings>().distance_factor, 1.0);
        assert!(
            world
                .resource::<WorldChunkStreamer>()
                .loaded_chunks
                .is_empty(),
            "no player yet"
        );
    }

    #[test]
    fn plugin_streams_around_a_player() {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            TransformPlugin,
            HierarchyPlugin,
            WorldPlugin,
        ));
        app.world_mut()
            .resource_mut::<WorldChunkStreamer>()
            .spawns_per_frame = usize::MAX;
        app.world_mut().spawn((
            game_core::identifiers::PlayerCharacterMarker,
            Transform::from_xyz(0.0, 300.0, 0.0),
        ));
        app.update();
        let streamer = app.world().resource::<WorldChunkStreamer>();
        assert_eq!(
            streamer.loaded_chunks.len(),
            chunk_loader::render_volume_len(RENDER_DISTANCE_CHUNKS)
        );
        let center = ChunkCoordinate::from_world_position(Vec3::new(0.0, 300.0, 0.0));
        assert_eq!(streamer.state_of(center), ChunkState::Active);
        assert_eq!(
            get_district_at_position(center.to_world_center()).map(|d| d.id),
            Some(DistrictId::KuroshioCommercial)
        );
    }
}
