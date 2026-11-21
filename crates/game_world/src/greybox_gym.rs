//! Procedural greybox gym: an asset-free blockout course for tuning parkour.
//!
//! The course is described as plain data by [`greybox_gym_layout`] and turned
//! into entities by [`setup_greybox_gym`], so geometry can be verified
//! without a renderer.
//!
//! Coordinate conventions: `+Y` is up, the ground's top surface is `y = 0`,
//! and the main runway runs toward `-Z` from the origin.

use avian3d::prelude::*;
use bevy::prelude::*;

/// Marks every entity spawned by the greybox gym.
#[derive(Component, Reflect, Debug, Default, Clone, Copy, PartialEq, Eq)]
#[reflect(Component, Default, Debug)]
pub struct GreyboxGymMarker;

/// Parkour interaction category of a gym obstacle.
#[derive(Reflect, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[reflect(Debug, PartialEq, Hash)]
pub enum ObstacleType {
    LowVault,
    MidVault,
    HighVault,
    WallRunFlat,
    WallRunCurved,
    LedgeMantle,
    SlideSlope,
    GapJump,
    Zipline,
    DropTower,
}

/// Tags a gym entity with the parkour move it is built to exercise.
#[derive(Component, Reflect, Debug, Clone, Copy, PartialEq)]
#[reflect(Component, Debug, PartialEq)]
pub struct ParkourObstacle {
    pub obstacle_type: ObstacleType,
    /// Height of the obstacle's top surface above the ground.
    pub height_meters: f32,
}

/// Trigger volume at the foot of the drop tower; landing inside it should
/// convert a hard landing into a roll.
#[derive(Component, Reflect, Debug, Default, Clone, Copy, PartialEq, Eq)]
#[reflect(Component, Default, Debug)]
pub struct LandingCushion;

/// Shared PBR materials for the greybox gym.
#[derive(Resource, Debug, Clone)]
pub struct GreyboxMaterials {
    pub grid_floor: Handle<StandardMaterial>,
    pub wall_run: Handle<StandardMaterial>,
    pub vault_accent: Handle<StandardMaterial>,
    pub hazard_red: Handle<StandardMaterial>,
    pub target_emissive: Handle<StandardMaterial>,
}

impl GreyboxMaterials {
    pub fn new(materials: &mut Assets<StandardMaterial>) -> Self {
        Self {
            grid_floor: materials.add(StandardMaterial {
                base_color: Color::srgb(0.15, 0.15, 0.17),
                perceptual_roughness: 0.8,
                metallic: 0.1,
                ..default()
            }),
            wall_run: materials.add(StandardMaterial {
                base_color: Color::srgb(0.1, 0.4, 0.6),
                perceptual_roughness: 0.4,
                metallic: 0.5,
                ..default()
            }),
            vault_accent: materials.add(StandardMaterial {
                base_color: Color::srgb(0.9, 0.6, 0.1),
                perceptual_roughness: 0.5,
                metallic: 0.2,
                ..default()
            }),
            hazard_red: materials.add(StandardMaterial {
                base_color: Color::srgb(0.8, 0.1, 0.1),
                perceptual_roughness: 0.6,
                ..default()
            }),
            target_emissive: materials.add(StandardMaterial {
                base_color: Color::srgb(0.0, 1.0, 0.8),
                emissive: LinearRgba::rgb(0.0, 5.0, 4.0),
                ..default()
            }),
        }
    }

    fn get(&self, slot: GymMaterial) -> Handle<StandardMaterial> {
        match slot {
            GymMaterial::GridFloor => self.grid_floor.clone(),
            GymMaterial::WallRun => self.wall_run.clone(),
            GymMaterial::VaultAccent => self.vault_accent.clone(),
            GymMaterial::HazardRed => self.hazard_red.clone(),
            GymMaterial::TargetEmissive => self.target_emissive.clone(),
        }
    }
}

// --- Layout dimensions (meters) ---------------------------------------------

pub const GROUND_SIZE: f32 = 200.0;
pub const GROUND_THICKNESS: f32 = 0.2;

pub const VAULT_WIDTH: f32 = 4.0;
pub const VAULT_DEPTH: f32 = 0.6;
/// `(type, height, z)` for each vault barrier on the runway.
pub const VAULTS: [(ObstacleType, f32, f32); 3] = [
    (ObstacleType::LowVault, 0.75, -10.0),
    (ObstacleType::MidVault, 1.25, -20.0),
    (ObstacleType::HighVault, 1.80, -30.0),
];

pub const WALL_RUN_LENGTH: f32 = 25.0;
pub const WALL_RUN_HEIGHT: f32 = 6.0;
pub const WALL_RUN_THICKNESS: f32 = 0.5;
pub const WALL_RUN_OFFSET_X: f32 = 3.0;
pub const WALL_RUN_CENTER_Z: f32 = -55.0;

pub const RAMP_ANGLE_DEGREES: f32 = 35.0;
pub const RAMP_WIDTH: f32 = 6.0;
pub const RAMP_LENGTH: f32 = 15.0;
pub const RAMP_THICKNESS: f32 = 0.5;
pub const RAMP_CENTER: Vec3 = Vec3::new(15.0, 4.0, -25.0);

pub const DROP_TOWER_HEIGHT: f32 = 40.0;
pub const DROP_TOWER_X: f32 = -20.0;
pub const DROP_TOWER_Z: f32 = 0.0;
pub const DROP_TOWER_COLUMN_SIZE: f32 = 3.0;
/// The platform extends this far past the column on the +X side so a player
/// can step off into open air above the cushion.
pub const DROP_PLATFORM_OVERHANG: f32 = 4.0;
pub const DROP_PLATFORM_THICKNESS: f32 = 0.5;
pub const CUSHION_SIZE: Vec3 = Vec3::new(5.0, 1.0, 5.0);

pub const GAP_JUMP_DISTANCES: [f32; 5] = [3.0, 5.0, 8.0, 12.0, 16.0];
pub const GAP_PAD_SIZE: Vec3 = Vec3::new(4.0, 3.0, 6.0);
pub const GAP_BED_X: f32 = 40.0;
/// Z of the leading (+Z) edge of the first pad.
pub const GAP_BED_START_Z: f32 = 0.0;

// --- Layout data -------------------------------------------------------------

/// Material slot used by a [`GymPiece`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GymMaterial {
    GridFloor,
    WallRun,
    VaultAccent,
    HazardRed,
    TargetEmissive,
}

/// Physical behaviour of a [`GymPiece`]'s collider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GymColliderKind {
    /// Static, solid geometry.
    Solid,
    /// Static trigger volume that reports overlaps but does not block.
    Sensor,
}

/// One axis-aligned (before rotation) box in the gym.
#[derive(Debug, Clone, PartialEq)]
pub struct GymPiece {
    pub name: &'static str,
    /// Full extents along local X, Y, Z.
    pub size: Vec3,
    pub transform: Transform,
    pub material: GymMaterial,
    pub collider: GymColliderKind,
    pub obstacle: Option<ParkourObstacle>,
    pub landing_cushion: bool,
}

impl GymPiece {
    fn solid(name: &'static str, size: Vec3, translation: Vec3, material: GymMaterial) -> Self {
        Self {
            name,
            size,
            transform: Transform::from_translation(translation),
            material,
            collider: GymColliderKind::Solid,
            obstacle: None,
            landing_cushion: false,
        }
    }

    fn with_obstacle(mut self, obstacle_type: ObstacleType, height_meters: f32) -> Self {
        self.obstacle = Some(ParkourObstacle {
            obstacle_type,
            height_meters,
        });
        self
    }

    /// World-space Y of the piece's highest point.
    pub fn top_y(&self) -> f32 {
        let half = self.size * 0.5;
        let mut top = f32::NEG_INFINITY;
        for corner in [
            Vec3::new(-half.x, -half.y, -half.z),
            Vec3::new(-half.x, -half.y, half.z),
            Vec3::new(-half.x, half.y, -half.z),
            Vec3::new(-half.x, half.y, half.z),
            Vec3::new(half.x, -half.y, -half.z),
            Vec3::new(half.x, -half.y, half.z),
            Vec3::new(half.x, half.y, -half.z),
            Vec3::new(half.x, half.y, half.z),
        ] {
            top = top.max(self.transform.transform_point(corner).y);
        }
        top
    }
}

/// Builds the complete gym description.
pub fn greybox_gym_layout() -> Vec<GymPiece> {
    let mut pieces = Vec::with_capacity(16);

    // Ground: a finite slab whose top surface sits exactly at y = 0.
    pieces.push(GymPiece::solid(
        "ground",
        Vec3::new(GROUND_SIZE, GROUND_THICKNESS, GROUND_SIZE),
        Vec3::new(0.0, -GROUND_THICKNESS * 0.5, 0.0),
        GymMaterial::GridFloor,
    ));

    for (obstacle_type, height, z) in VAULTS {
        pieces.push(
            GymPiece::solid(
                "vault",
                Vec3::new(VAULT_WIDTH, height, VAULT_DEPTH),
                Vec3::new(0.0, height * 0.5, z),
                GymMaterial::VaultAccent,
            )
            .with_obstacle(obstacle_type, height),
        );
    }

    let wall_size = Vec3::new(WALL_RUN_THICKNESS, WALL_RUN_HEIGHT, WALL_RUN_LENGTH);
    for (name, x) in [
        ("wall_run_left", -WALL_RUN_OFFSET_X),
        ("wall_run_right", WALL_RUN_OFFSET_X),
    ] {
        pieces.push(
            GymPiece::solid(
                name,
                wall_size,
                Vec3::new(x, WALL_RUN_HEIGHT * 0.5, WALL_RUN_CENTER_Z),
                GymMaterial::WallRun,
            )
            .with_obstacle(ObstacleType::WallRunFlat, WALL_RUN_HEIGHT),
        );
    }

    let mut ramp = GymPiece::solid(
        "slide_ramp",
        Vec3::new(RAMP_WIDTH, RAMP_THICKNESS, RAMP_LENGTH),
        RAMP_CENTER,
        GymMaterial::GridFloor,
    );
    ramp.transform.rotation = Quat::from_rotation_x(RAMP_ANGLE_DEGREES.to_radians());
    let ramp_top = ramp.top_y();
    pieces.push(ramp.with_obstacle(ObstacleType::SlideSlope, ramp_top));

    // Drop tower: a column topped by a platform that overhangs toward +X.
    pieces.push(
        GymPiece::solid(
            "drop_tower_column",
            Vec3::new(
                DROP_TOWER_COLUMN_SIZE,
                DROP_TOWER_HEIGHT - DROP_PLATFORM_THICKNESS,
                DROP_TOWER_COLUMN_SIZE,
            ),
            Vec3::new(
                DROP_TOWER_X,
                (DROP_TOWER_HEIGHT - DROP_PLATFORM_THICKNESS) * 0.5,
                DROP_TOWER_Z,
            ),
            GymMaterial::HazardRed,
        )
        .with_obstacle(ObstacleType::DropTower, DROP_TOWER_HEIGHT),
    );
    let platform_length = DROP_TOWER_COLUMN_SIZE + DROP_PLATFORM_OVERHANG;
    pieces.push(
        GymPiece::solid(
            "drop_tower_platform",
            Vec3::new(
                platform_length,
                DROP_PLATFORM_THICKNESS,
                DROP_TOWER_COLUMN_SIZE,
            ),
            Vec3::new(
                DROP_TOWER_X + DROP_PLATFORM_OVERHANG * 0.5,
                DROP_TOWER_HEIGHT - DROP_PLATFORM_THICKNESS * 0.5,
                DROP_TOWER_Z,
            ),
            GymMaterial::HazardRed,
        )
        .with_obstacle(ObstacleType::DropTower, DROP_TOWER_HEIGHT),
    );
    let platform_edge_x = DROP_TOWER_X - DROP_TOWER_COLUMN_SIZE * 0.5 + platform_length;
    pieces.push(GymPiece {
        name: "drop_cushion",
        size: CUSHION_SIZE,
        transform: Transform::from_xyz(
            platform_edge_x - CUSHION_SIZE.x * 0.5 + 1.0,
            CUSHION_SIZE.y * 0.5,
            DROP_TOWER_Z,
        ),
        material: GymMaterial::TargetEmissive,
        collider: GymColliderKind::Sensor,
        obstacle: None,
        landing_cushion: true,
    });

    // Gap jump bed: raised pads separated by increasing chasms toward -Z.
    let mut leading_edge_z = GAP_BED_START_Z;
    let pad_count = GAP_JUMP_DISTANCES.len() + 1;
    for i in 0..pad_count {
        let center_z = leading_edge_z - GAP_PAD_SIZE.z * 0.5;
        pieces.push(
            GymPiece::solid(
                "gap_pad",
                GAP_PAD_SIZE,
                Vec3::new(GAP_BED_X, GAP_PAD_SIZE.y * 0.5, center_z),
                GymMaterial::VaultAccent,
            )
            .with_obstacle(ObstacleType::GapJump, GAP_PAD_SIZE.y),
        );
        leading_edge_z -= GAP_PAD_SIZE.z;
        if let Some(gap) = GAP_JUMP_DISTANCES.get(i) {
            leading_edge_z -= gap;
        }
    }

    pieces
}

/// Spawns every [`GymPiece`] and inserts [`GreyboxMaterials`].
pub fn setup_greybox_gym(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let greybox_materials = GreyboxMaterials::new(&mut materials);

    for piece in greybox_gym_layout() {
        let mut entity = commands.spawn((
            Name::new(piece.name),
            Mesh3d(meshes.add(Cuboid::from_size(piece.size))),
            MeshMaterial3d(greybox_materials.get(piece.material)),
            piece.transform,
            RigidBody::Static,
            Collider::cuboid(piece.size.x, piece.size.y, piece.size.z),
            GreyboxGymMarker,
        ));
        if piece.collider == GymColliderKind::Sensor {
            entity.insert(Sensor);
        }
        if let Some(obstacle) = piece.obstacle {
            entity.insert(obstacle);
        }
        if piece.landing_cushion {
            entity.insert(LandingCushion);
        }
    }

    commands.insert_resource(greybox_materials);
}

/// Spawns the greybox gym at startup. Add only when greybox mode is active.
pub struct GreyboxGymPlugin;

impl Plugin for GreyboxGymPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<GreyboxGymMarker>()
            .register_type::<ParkourObstacle>()
            .register_type::<ObstacleType>()
            .register_type::<LandingCushion>()
            .add_systems(Startup, setup_greybox_gym);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f32 = 1e-4;

    fn piece<'a>(layout: &'a [GymPiece], name: &str) -> &'a GymPiece {
        layout
            .iter()
            .find(|p| p.name == name)
            .unwrap_or_else(|| panic!("missing gym piece {name}"))
    }

    fn bottom_y(p: &GymPiece) -> f32 {
        p.transform.translation.y - p.size.y * 0.5
    }

    #[test]
    fn ground_top_surface_is_at_zero() {
        let layout = greybox_gym_layout();
        let ground = piece(&layout, "ground");
        assert_eq!(
            ground.size,
            Vec3::new(GROUND_SIZE, GROUND_THICKNESS, GROUND_SIZE)
        );
        assert!(ground.top_y().abs() < EPS);
    }

    #[test]
    fn vaults_match_spec_heights_and_rest_on_ground() {
        let layout = greybox_gym_layout();
        let vaults: Vec<_> = layout.iter().filter(|p| p.name == "vault").collect();
        assert_eq!(vaults.len(), 3);
        for (vault, (expected_type, height, z)) in vaults.iter().zip(VAULTS) {
            let obstacle = vault.obstacle.expect("vault obstacle");
            assert_eq!(obstacle.obstacle_type, expected_type);
            assert!((obstacle.height_meters - height).abs() < EPS);
            assert_eq!(vault.size, Vec3::new(4.0, height, 0.6));
            assert!(bottom_y(vault).abs() < EPS);
            assert!((vault.top_y() - height).abs() < EPS);
            assert!((vault.transform.translation.z - z).abs() < EPS);
        }
    }

    #[test]
    fn wall_run_corridor_dimensions() {
        let layout = greybox_gym_layout();
        for (name, x) in [("wall_run_left", -3.0), ("wall_run_right", 3.0)] {
            let wall = piece(&layout, name);
            assert_eq!(wall.size, Vec3::new(0.5, 6.0, 25.0));
            assert_eq!(wall.transform.translation, Vec3::new(x, 3.0, -55.0));
            assert_eq!(
                wall.obstacle.map(|o| o.obstacle_type),
                Some(ObstacleType::WallRunFlat)
            );
        }
    }

    #[test]
    fn slide_ramp_is_inclined_35_degrees() {
        let layout = greybox_gym_layout();
        let ramp = piece(&layout, "slide_ramp");
        let surface_normal = ramp.transform.rotation * Vec3::Y;
        let incline = surface_normal.angle_between(Vec3::Y).to_degrees();
        assert!((incline - 35.0).abs() < 1e-3, "incline {incline}");

        let downhill = ramp.transform.rotation * Vec3::Z;
        assert!(downhill.y < 0.0, "ramp should descend toward +Z");
        assert_eq!(ramp.size, Vec3::new(6.0, 0.5, 15.0));
        assert_eq!(ramp.transform.translation, RAMP_CENTER);

        let obstacle = ramp.obstacle.expect("ramp obstacle");
        assert_eq!(obstacle.obstacle_type, ObstacleType::SlideSlope);
        assert!((obstacle.height_meters - ramp.top_y()).abs() < EPS);
    }

    #[test]
    fn drop_tower_platform_is_40m_with_cushion_below_overhang() {
        let layout = greybox_gym_layout();
        let column = piece(&layout, "drop_tower_column");
        let platform = piece(&layout, "drop_tower_platform");
        let cushion = piece(&layout, "drop_cushion");

        assert!(bottom_y(column).abs() < EPS);
        assert!((column.top_y() - bottom_y(platform)).abs() < EPS);
        assert!((platform.top_y() - DROP_TOWER_HEIGHT).abs() < EPS);

        let column_max_x = column.transform.translation.x + column.size.x * 0.5;
        let platform_max_x = platform.transform.translation.x + platform.size.x * 0.5;
        let cushion_x = cushion.transform.translation.x;
        assert!(cushion_x > column_max_x && cushion_x < platform_max_x);

        assert_eq!(cushion.collider, GymColliderKind::Sensor);
        assert!(cushion.landing_cushion);
        assert!(bottom_y(cushion).abs() < EPS);
    }

    #[test]
    fn gap_bed_has_requested_chasm_distances() {
        let layout = greybox_gym_layout();
        let pads: Vec<_> = layout.iter().filter(|p| p.name == "gap_pad").collect();
        assert_eq!(pads.len(), GAP_JUMP_DISTANCES.len() + 1);

        for (pair, expected_gap) in pads.windows(2).zip(GAP_JUMP_DISTANCES) {
            let near_trailing_edge = pair[0].transform.translation.z - pair[0].size.z * 0.5;
            let far_leading_edge = pair[1].transform.translation.z + pair[1].size.z * 0.5;
            let gap = near_trailing_edge - far_leading_edge;
            assert!(
                (gap - expected_gap).abs() < EPS,
                "gap {gap} != {expected_gap}"
            );
            assert!((pair[1].top_y() - GAP_PAD_SIZE.y).abs() < EPS);
        }
    }

    #[test]
    fn every_piece_is_finite_and_sized() {
        for p in greybox_gym_layout() {
            assert!(
                p.size.is_finite() && p.size.min_element() > 0.0,
                "{}",
                p.name
            );
            assert!(p.transform.is_finite(), "{}", p.name);
        }
    }

    fn spawned_app() -> App {
        let mut app = App::new();
        app.init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>()
            .add_plugins(GreyboxGymPlugin);
        app.update();
        app
    }

    #[test]
    fn plugin_spawns_full_layout_with_matching_colliders() {
        let mut app = spawned_app();
        let expected = greybox_gym_layout();

        let world = app.world_mut();
        let mut query =
            world.query_filtered::<(&Name, &Collider, &RigidBody), With<GreyboxGymMarker>>();
        let spawned: Vec<_> = query.iter(world).collect();
        assert_eq!(spawned.len(), expected.len());

        for (name, collider, body) in spawned {
            assert_eq!(*body, RigidBody::Static);
            let cuboid = collider
                .shape()
                .as_cuboid()
                .unwrap_or_else(|| panic!("{name} collider is not a cuboid"));
            let size = Vec3::from(cuboid.half_extents) * 2.0;
            assert!(
                expected
                    .iter()
                    .any(|p| p.name == name.as_str() && (p.size - size).length() < EPS),
                "{name} collider size {size:?} not in layout"
            );
        }

        let mut sensors = world.query_filtered::<(), (With<Sensor>, With<LandingCushion>)>();
        assert_eq!(sensors.iter(world).count(), 1);
        let mut obstacles = world.query::<&ParkourObstacle>();
        assert_eq!(
            obstacles.iter(world).count(),
            expected.iter().filter(|p| p.obstacle.is_some()).count()
        );
    }

    #[test]
    fn plugin_registers_materials() {
        let app = spawned_app();
        let world = app.world();
        let mats = world.resource::<GreyboxMaterials>();
        let assets = world.resource::<Assets<StandardMaterial>>();
        for handle in [
            &mats.grid_floor,
            &mats.wall_run,
            &mats.vault_accent,
            &mats.hazard_red,
            &mats.target_emissive,
        ] {
            assert!(assets.get(handle).is_some());
        }
        assert_eq!(assets.len(), 5);
        let emissive = assets.get(&mats.target_emissive).expect("emissive");
        assert!(emissive.emissive.green > 1.0);
    }
}
