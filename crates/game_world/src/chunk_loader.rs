//! Cubic 64 m chunk streaming around the player.
//!
//! Rings by Chebyshev chunk distance from the player's chunk:
//! - 0-2 `Active`: spawned, full physics and AI.
//! - 3-4 `Rendered`: spawned, static meshes only.
//! - 5-6 `Cached`: entity despawned, scene asset handle kept so the data
//!   stays in RAM and re-entry is instant.
//! - 7+ `Evicted`: handle dropped, nothing kept.
//!
//! Chunks exist in a vertical band of one layer below to two above the
//! player. Spawns are budgeted per frame, nearest first, so crossing a
//! boundary at sprint speed never spawns a whole shell in one frame.

use bevy::prelude::*;
use game_core::identifiers::PlayerCharacterMarker;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

pub const CHUNK_SIZE_METERS: f32 = 64.0;
/// 0-128 m: full 64 Hz physics and AI.
pub const SIMULATION_DISTANCE_CHUNKS: i32 = 2;
/// 128-256 m: rendered static meshes.
pub const RENDER_DISTANCE_CHUNKS: i32 = 4;
/// 256-384 m: cached in RAM, meshes unloaded.
pub const CACHE_DISTANCE_CHUNKS: i32 = 6;
/// Vertical chunk layers streamed relative to the player's layer.
pub const VERTICAL_CHUNK_RANGE: std::ops::RangeInclusive<i32> = -1..=2;
/// Chunk entities spawned per frame at most.
pub const DEFAULT_SPAWNS_PER_FRAME: usize = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, Reflect)]
#[reflect(Debug, PartialEq, Hash)]
pub struct ChunkCoordinate {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl ChunkCoordinate {
    pub const fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }

    pub fn from_world_position(pos: Vec3) -> Self {
        let c = (pos / CHUNK_SIZE_METERS).floor();
        Self::new(c.x as i32, c.y as i32, c.z as i32)
    }

    fn as_vec3(self) -> Vec3 {
        Vec3::new(self.x as f32, self.y as f32, self.z as f32)
    }

    pub fn to_world_center(&self) -> Vec3 {
        (self.as_vec3() + 0.5) * CHUNK_SIZE_METERS
    }

    pub fn to_world_min(&self) -> Vec3 {
        self.as_vec3() * CHUNK_SIZE_METERS
    }

    pub fn to_world_max(&self) -> Vec3 {
        (self.as_vec3() + 1.0) * CHUNK_SIZE_METERS
    }

    pub fn chebyshev_distance(&self, other: &ChunkCoordinate) -> i32 {
        (self.x - other.x)
            .abs()
            .max((self.y - other.y).abs())
            .max((self.z - other.z).abs())
    }

    /// In chunk units.
    pub fn euclidean_distance(&self, other: &ChunkCoordinate) -> f32 {
        self.as_vec3().distance(other.as_vec3())
    }

    /// Asset path of this chunk's authored scene.
    pub fn asset_path(&self) -> String {
        format!(
            "models/districts/chunk_{}_{}_{}.glb#Scene0",
            self.x, self.y, self.z
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Reflect)]
#[reflect(Debug, PartialEq, Hash)]
pub enum ChunkState {
    /// Full physics colliders, NPC AI, 64 Hz ticks.
    Active,
    /// Static PBR meshes on the GPU, LOD 2/3.
    Rendered,
    /// Asset data cached in RAM, meshes off the GPU.
    Cached,
    /// Memory released, colliders unmapped.
    #[default]
    Evicted,
}

impl ChunkState {
    /// Whether the chunk has a live entity.
    pub fn is_spawned(self) -> bool {
        matches!(self, Self::Active | Self::Rendered)
    }
}

/// Marks a streamed chunk root.
#[derive(Component, Debug, Clone, Copy, PartialEq, Reflect)]
#[reflect(Component, Debug, PartialEq)]
pub struct WorldChunk {
    pub coord: ChunkCoordinate,
    pub state: ChunkState,
}

/// Chunks that have an authored scene. Only these request an asset load;
/// every other chunk still gets an (empty) streamed root.
#[derive(Resource, Debug, Clone, Default)]
pub struct ChunkAssetManifest {
    pub authored_chunks: HashSet<ChunkCoordinate>,
}

/// Number of chunks `chunks_in_range` yields for `distance`.
pub fn render_volume_len(distance: i32) -> usize {
    let side = (2 * distance + 1) as usize;
    let layers = VERTICAL_CHUNK_RANGE
        .filter(|dy| dy.abs() <= distance)
        .count();
    side * side * layers
}

/// All chunk coordinates within `distance` (Chebyshev) of `center`, inside
/// the vertical streaming band.
pub fn chunks_in_range(center: ChunkCoordinate, distance: i32) -> HashSet<ChunkCoordinate> {
    let mut out = HashSet::new();
    for dx in -distance..=distance {
        for dy in VERTICAL_CHUNK_RANGE {
            if dy.abs() > distance {
                continue;
            }
            for dz in -distance..=distance {
                out.insert(ChunkCoordinate::new(
                    center.x + dx,
                    center.y + dy,
                    center.z + dz,
                ));
            }
        }
    }
    out
}

#[derive(Resource, Debug)]
pub struct WorldChunkStreamer {
    pub loaded_chunks: HashMap<ChunkCoordinate, Entity>,
    /// State of every chunk within cache distance; absent means evicted.
    pub chunk_states: HashMap<ChunkCoordinate, ChunkState>,
    /// Chunks whose scene asset has been requested but not finished loading.
    pub pending_loads: HashSet<ChunkCoordinate>,
    /// Scene handles kept alive for spawned and cached chunks.
    pub scene_handles: HashMap<ChunkCoordinate, Handle<Scene>>,
    pub spawns_per_frame: usize,
    /// Player chunk at the last full update; `None` forces one.
    pub last_center: Option<ChunkCoordinate>,
}

impl Default for WorldChunkStreamer {
    fn default() -> Self {
        Self {
            loaded_chunks: HashMap::new(),
            chunk_states: HashMap::new(),
            pending_loads: HashSet::new(),
            scene_handles: HashMap::new(),
            spawns_per_frame: DEFAULT_SPAWNS_PER_FRAME,
            last_center: None,
        }
    }
}

impl WorldChunkStreamer {
    /// Chunks that should be spawned around `player_coord`, and the loaded
    /// chunks that fall outside that set.
    pub fn compute_streaming_delta(
        &self,
        player_coord: ChunkCoordinate,
        render_dist: i32,
    ) -> (HashSet<ChunkCoordinate>, Vec<ChunkCoordinate>) {
        let desired = chunks_in_range(player_coord, render_dist);
        let mut evict: Vec<ChunkCoordinate> = self
            .loaded_chunks
            .keys()
            .filter(|c| !desired.contains(c))
            .copied()
            .collect();
        evict.sort_by_key(|c| (c.x, c.y, c.z));
        (desired, evict)
    }

    pub fn classify_chunk_state(dist: i32) -> ChunkState {
        match dist {
            d if d <= SIMULATION_DISTANCE_CHUNKS => ChunkState::Active,
            d if d <= RENDER_DISTANCE_CHUNKS => ChunkState::Rendered,
            d if d <= CACHE_DISTANCE_CHUNKS => ChunkState::Cached,
            _ => ChunkState::Evicted,
        }
    }

    pub fn state_of(&self, coord: ChunkCoordinate) -> ChunkState {
        self.chunk_states.get(&coord).copied().unwrap_or_default()
    }

    pub fn count_in_state(&self, state: ChunkState) -> usize {
        self.chunk_states.values().filter(|s| **s == state).count()
    }
}

fn streaming_center(
    players: &Query<&Transform, With<PlayerCharacterMarker>>,
    cameras: &Query<&Transform, (With<Camera3d>, Without<PlayerCharacterMarker>)>,
) -> Option<Vec3> {
    players
        .iter()
        .next()
        .or_else(|| cameras.iter().next())
        .map(|t| t.translation)
}

/// Streams chunks around the player (or the first 3D camera when there is
/// no player): despawns and caches or evicts chunks that left render
/// distance, re-classifies spawned chunks, and spawns newly desired chunks
/// nearest first within the per-frame budget.
#[allow(clippy::too_many_arguments)]
pub fn update_chunk_streaming(
    mut commands: Commands,
    asset_server: Option<Res<AssetServer>>,
    manifest: Option<Res<ChunkAssetManifest>>,
    mut streamer: ResMut<WorldChunkStreamer>,
    players: Query<&Transform, With<PlayerCharacterMarker>>,
    cameras: Query<&Transform, (With<Camera3d>, Without<PlayerCharacterMarker>)>,
    mut chunks: Query<&mut WorldChunk>,
) {
    let Some(position) = streaming_center(&players, &cameras) else {
        return;
    };
    let center = ChunkCoordinate::from_world_position(position);
    let streamer = streamer.as_mut();

    if let Some(server) = asset_server.as_deref() {
        streamer.pending_loads.retain(|coord| {
            streamer.scene_handles.get(coord).is_some_and(|handle| {
                !server.is_loaded_with_dependencies(handle)
                    && !matches!(
                        server.get_load_state(handle),
                        Some(bevy::asset::LoadState::Failed(_))
                    )
            })
        });
    }

    let backlog = streamer.loaded_chunks.len() < render_volume_len(RENDER_DISTANCE_CHUNKS);
    if streamer.last_center == Some(center) && !backlog {
        return;
    }
    streamer.last_center = Some(center);

    let (desired, evict) = streamer.compute_streaming_delta(center, RENDER_DISTANCE_CHUNKS);

    for coord in evict {
        if let Some(entity) = streamer.loaded_chunks.remove(&coord) {
            commands.entity(entity).despawn_recursive();
        }
        streamer.pending_loads.remove(&coord);
        streamer.chunk_states.insert(coord, ChunkState::Cached);
    }

    // Cached chunks that drifted past cache distance release their data.
    let evicted: Vec<ChunkCoordinate> = streamer
        .chunk_states
        .keys()
        .filter(|c| {
            WorldChunkStreamer::classify_chunk_state(center.chebyshev_distance(c))
                == ChunkState::Evicted
        })
        .copied()
        .collect();
    for coord in evicted {
        streamer.chunk_states.remove(&coord);
        streamer.scene_handles.remove(&coord);
        streamer.pending_loads.remove(&coord);
    }

    for (&coord, &entity) in &streamer.loaded_chunks {
        let state = WorldChunkStreamer::classify_chunk_state(center.chebyshev_distance(&coord));
        streamer.chunk_states.insert(coord, state);
        if let Ok(mut chunk) = chunks.get_mut(entity) {
            if chunk.state != state {
                chunk.state = state;
            }
        }
    }

    let mut to_spawn: Vec<ChunkCoordinate> = desired
        .into_iter()
        .filter(|c| !streamer.loaded_chunks.contains_key(c))
        .collect();
    to_spawn.sort_by(|a, b| {
        center
            .euclidean_distance(a)
            .total_cmp(&center.euclidean_distance(b))
            .then((a.x, a.y, a.z).cmp(&(b.x, b.y, b.z)))
    });
    for coord in to_spawn.into_iter().take(streamer.spawns_per_frame) {
        let state = WorldChunkStreamer::classify_chunk_state(center.chebyshev_distance(&coord));
        let mut entity = commands.spawn((
            Name::new(format!("Chunk ({}, {}, {})", coord.x, coord.y, coord.z)),
            WorldChunk { coord, state },
            Transform::from_translation(coord.to_world_center()),
            Visibility::default(),
        ));
        let authored = manifest
            .as_deref()
            .is_some_and(|m| m.authored_chunks.contains(&coord));
        if let (true, Some(server)) = (authored, asset_server.as_deref()) {
            let handle = streamer
                .scene_handles
                .entry(coord)
                .or_insert_with(|| server.load(coord.asset_path()))
                .clone();
            if !server.is_loaded_with_dependencies(&handle) {
                streamer.pending_loads.insert(coord);
            }
            entity.insert(SceneRoot(handle));
        }
        streamer.loaded_chunks.insert(coord, entity.id());
        streamer.chunk_states.insert(coord, state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(x: i32, y: i32, z: i32) -> ChunkCoordinate {
        ChunkCoordinate::new(x, y, z)
    }

    #[test]
    fn world_to_chunk_floors_toward_negative_infinity() {
        assert_eq!(ChunkCoordinate::from_world_position(Vec3::ZERO), c(0, 0, 0));
        assert_eq!(
            ChunkCoordinate::from_world_position(Vec3::splat(63.99)),
            c(0, 0, 0)
        );
        assert_eq!(
            ChunkCoordinate::from_world_position(Vec3::new(65.0, 64.0, 128.0)),
            c(1, 1, 2)
        );
        assert_eq!(
            ChunkCoordinate::from_world_position(Vec3::new(-10.0, -64.0, -64.01)),
            c(-1, -1, -2)
        );
    }

    #[test]
    fn chunk_bounds_and_center_roundtrip() {
        for coord in [c(0, 0, 0), c(3, -2, 7), c(-5, 1, -9), c(-1, -1, -1)] {
            assert_eq!(
                ChunkCoordinate::from_world_position(coord.to_world_center()),
                coord
            );
            assert_eq!(
                ChunkCoordinate::from_world_position(coord.to_world_min()),
                coord
            );
            assert_eq!(
                coord.to_world_max() - coord.to_world_min(),
                Vec3::splat(64.0)
            );
            assert_eq!(
                (coord.to_world_min() + coord.to_world_max()) * 0.5,
                coord.to_world_center()
            );
        }
        assert_eq!(c(-1, 0, 2).to_world_center(), Vec3::new(-32.0, 32.0, 160.0));
    }

    #[test]
    fn distances() {
        let origin = c(0, 0, 0);
        assert_eq!(origin.chebyshev_distance(&c(3, -1, 2)), 3);
        assert_eq!(origin.chebyshev_distance(&c(-4, 4, 0)), 4);
        assert_eq!(c(2, 2, 2).chebyshev_distance(&c(2, 2, 2)), 0);
        assert_eq!(origin.euclidean_distance(&c(3, 4, 0)), 5.0);
        // The Chebyshev ball is the cube around the center.
        let cube = chunks_in_range(origin, 2);
        assert!(cube.iter().all(|k| origin.chebyshev_distance(k) <= 2));
        assert_eq!(cube.len(), 5 * 4 * 5, "x/z -2..=2, vertical band -1..=2");
        for d in 0..=6 {
            assert_eq!(
                chunks_in_range(origin, d).len(),
                render_volume_len(d),
                "{d}"
            );
        }
    }

    #[test]
    fn state_rings() {
        for d in 0..=2 {
            assert_eq!(
                WorldChunkStreamer::classify_chunk_state(d),
                ChunkState::Active
            );
        }
        for d in 3..=4 {
            assert_eq!(
                WorldChunkStreamer::classify_chunk_state(d),
                ChunkState::Rendered
            );
        }
        for d in 5..=6 {
            assert_eq!(
                WorldChunkStreamer::classify_chunk_state(d),
                ChunkState::Cached
            );
        }
        for d in [7, 8, 50] {
            assert_eq!(
                WorldChunkStreamer::classify_chunk_state(d),
                ChunkState::Evicted
            );
        }
    }

    #[test]
    fn delta_evicts_everything_left_behind() {
        let mut streamer = WorldChunkStreamer::default();
        let (desired, evict) = streamer.compute_streaming_delta(c(0, 0, 0), RENDER_DISTANCE_CHUNKS);
        assert_eq!(desired.len(), 9 * 4 * 9);
        assert!(evict.is_empty());
        for (i, coord) in desired.iter().enumerate() {
            streamer
                .loaded_chunks
                .insert(*coord, Entity::from_raw(i as u32));
        }
        let (desired, evict) =
            streamer.compute_streaming_delta(c(10, 0, 0), RENDER_DISTANCE_CHUNKS);
        assert!(desired.iter().all(|k| (6..=14).contains(&k.x)));
        assert_eq!(evict.len(), 9 * 4 * 9, "no overlap after a 10-chunk move");
        let (_, evict) = streamer.compute_streaming_delta(c(1, 0, 0), RENDER_DISTANCE_CHUNKS);
        assert_eq!(evict.len(), 4 * 9, "one slab of x = -4");
        assert!(evict.iter().all(|k| k.x == -4));
    }

    struct Sim {
        app: App,
        player: Entity,
    }

    impl Sim {
        fn new() -> Self {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, TransformPlugin, HierarchyPlugin))
                .init_resource::<WorldChunkStreamer>()
                .add_systems(Update, update_chunk_streaming);
            app.world_mut()
                .resource_mut::<WorldChunkStreamer>()
                .spawns_per_frame = usize::MAX;
            let player = app
                .world_mut()
                .spawn((PlayerCharacterMarker, Transform::from_xyz(10.0, 5.0, 10.0)))
                .id();
            Self { app, player }
        }

        fn move_to(&mut self, pos: Vec3) {
            self.app
                .world_mut()
                .entity_mut(self.player)
                .insert(Transform::from_translation(pos));
            self.app.update();
        }

        fn streamer(&self) -> &WorldChunkStreamer {
            self.app.world().resource::<WorldChunkStreamer>()
        }

        fn chunk_entities(&mut self) -> Vec<WorldChunk> {
            let world = self.app.world_mut();
            world.query::<&WorldChunk>().iter(world).copied().collect()
        }
    }

    #[test]
    fn system_spawns_full_render_volume_with_rings() {
        let mut sim = Sim::new();
        sim.app.update();
        let chunks = sim.chunk_entities();
        assert_eq!(chunks.len(), 9 * 4 * 9);
        assert_eq!(sim.streamer().loaded_chunks.len(), chunks.len());
        let active = chunks
            .iter()
            .filter(|k| k.state == ChunkState::Active)
            .count();
        assert_eq!(active, 5 * 4 * 5);
        assert_eq!(
            chunks.len() - active,
            sim.streamer().count_in_state(ChunkState::Rendered)
        );
        assert!(
            sim.streamer().pending_loads.is_empty(),
            "no authored chunks, no loads"
        );
    }

    #[test]
    fn moving_away_despawns_caches_then_evicts() {
        let mut sim = Sim::new();
        sim.app.update();
        let origin = c(0, 0, 0);
        // Six chunks east: the origin is now in the cache ring.
        sim.move_to(Vec3::new(6.0 * 64.0 + 10.0, 5.0, 10.0));
        assert!(!sim.streamer().loaded_chunks.contains_key(&origin));
        assert_eq!(sim.streamer().state_of(origin), ChunkState::Cached);
        assert!(sim.chunk_entities().iter().all(|k| k.coord.x >= 2));
        assert_eq!(
            sim.chunk_entities().len(),
            9 * 4 * 9,
            "despawned entities are gone"
        );
        // Ten chunks east: everything around the origin is released.
        sim.move_to(Vec3::new(10.0 * 64.0 + 10.0, 5.0, 10.0));
        assert_eq!(sim.streamer().state_of(origin), ChunkState::Evicted);
        assert!(sim.streamer().loaded_chunks.keys().all(|k| k.x >= 6));
        assert!(sim.streamer().chunk_states.keys().all(|k| k.x >= 4));
        assert!(sim.chunk_entities().iter().all(|k| k.coord.x >= 6));
    }

    #[test]
    fn crossing_a_ring_boundary_reclassifies_spawned_chunks() {
        let mut sim = Sim::new();
        sim.app.update();
        let east = c(2, 0, 0);
        assert_eq!(sim.streamer().state_of(east), ChunkState::Active);
        sim.move_to(Vec3::new(-64.0 + 10.0, 5.0, 10.0));
        assert_eq!(sim.streamer().state_of(east), ChunkState::Rendered);
        let entity = sim.streamer().loaded_chunks[&east];
        assert_eq!(
            sim.app.world().get::<WorldChunk>(entity).unwrap().state,
            ChunkState::Rendered
        );
    }

    #[test]
    fn spawn_budget_fills_in_over_frames_nearest_first() {
        let mut sim = Sim::new();
        sim.app
            .world_mut()
            .resource_mut::<WorldChunkStreamer>()
            .spawns_per_frame = 40;
        sim.app.update();
        let first = sim.chunk_entities();
        assert_eq!(first.len(), 40);
        let player_chunk = c(0, 0, 0);
        assert!(first
            .iter()
            .all(|k| player_chunk.chebyshev_distance(&k.coord) <= 2));
        for _ in 0..10 {
            sim.app.update();
        }
        assert_eq!(
            sim.chunk_entities().len(),
            9 * 4 * 9,
            "backlog drains while standing still"
        );
    }

    #[test]
    fn no_anchor_means_no_streaming() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<WorldChunkStreamer>()
            .add_systems(Update, update_chunk_streaming);
        app.update();
        assert!(app
            .world()
            .resource::<WorldChunkStreamer>()
            .loaded_chunks
            .is_empty());
    }
}
