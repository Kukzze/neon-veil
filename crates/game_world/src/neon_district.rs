//! Procedural neon street around the spawn point: an avenue running north
//! (-Z) lined with towers, lit window bands, blade signs, storefront strips,
//! street lamps, bounce awnings and a skybridge, with a far skyline for
//! depth.
//!
//! The layout is generated as plain data from a fixed seed (so it is the
//! same every run and testable), then spawned with a handful of shared
//! meshes and materials to keep draw cost low. Emissive values are in HDR
//! units and rely on bloom to glow.

use crate::procedural_streets::{spawn_bounce_awning, spawn_skybridge, BounceAwning};
use avian3d::prelude::*;
use bevy::prelude::*;

pub const DISTRICT_SEED: u64 = 0x4E56_5A50;
/// Half width of the road; sidewalks run from here to the facades.
pub const ROAD_HALF_WIDTH: f32 = 9.0;
/// Distance from the avenue centre line to the building facades.
pub const FACADE_OFFSET: f32 = 13.0;
pub const SIDEWALK_HEIGHT: f32 = 0.2;
/// The avenue spans this range of Z (north is -Z).
pub const AVENUE_START_Z: f32 = 30.0;
pub const AVENUE_END_Z: f32 = -260.0;
pub const FLOOR_HEIGHT: f32 = 3.4;
/// Rain puddles on the avenue; only these mirror the neon.
pub const PUDDLE_COUNT: usize = 220;
const STREET_LIGHT_SPACING: f32 = 24.0;

/// Marks every entity spawned for the district.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct NeonDistrictPiece;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NeonTone {
    Cyan,
    Magenta,
    Amber,
    Acid,
}

impl NeonTone {
    pub const ALL: [Self; 4] = [Self::Cyan, Self::Magenta, Self::Amber, Self::Acid];

    /// Linear HDR emissive colour.
    pub fn emissive(self, strength: f32) -> LinearRgba {
        let (r, g, b) = match self {
            Self::Cyan => (0.0, 0.85, 1.0),
            Self::Magenta => (1.0, 0.1, 0.55),
            Self::Amber => (1.0, 0.55, 0.05),
            Self::Acid => (0.45, 1.0, 0.15),
        };
        LinearRgba::rgb(r * strength, g * strength, b * strength)
    }

    pub fn light_color(self) -> Color {
        let e = self.emissive(1.0);
        Color::linear_rgb(e.red, e.green, e.blue)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowTone {
    Warm,
    Cool,
}

/// A lit band of windows on one floor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WindowBand {
    pub height: f32,
    /// Fraction of the facade width that is lit.
    pub width_fraction: f32,
    /// Offset of the band's centre along the facade, as a fraction of width.
    pub offset_fraction: f32,
    pub tone: WindowTone,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BuildingSpec {
    /// Centre of the footprint at ground level.
    pub base_center: Vec3,
    /// Width (X), height (Y), length along the avenue (Z).
    pub size: Vec3,
    /// -1 for the west side of the avenue, +1 for the east side.
    pub side: f32,
    pub windows: Vec<WindowBand>,
    pub sign: Option<(NeonTone, f32, f32)>,
    pub storefront: NeonTone,
    pub awning: bool,
}

impl BuildingSpec {
    /// X of the street-facing wall.
    pub fn facade_x(&self) -> f32 {
        self.side * FACADE_OFFSET
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StreetLight {
    pub base: Vec3,
    pub lit: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkylineTower {
    pub base_center: Vec3,
    pub size: Vec3,
    pub beacon: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct DistrictLayout {
    pub buildings: Vec<BuildingSpec>,
    pub street_lights: Vec<StreetLight>,
    pub skyline: Vec<SkylineTower>,
    /// Skybridges as (start, end, width).
    pub skybridges: Vec<(Vec3, Vec3, f32)>,
    /// Rain puddles as (centre on the ground, footprint X / Z, yaw).
    pub puddles: Vec<(Vec3, Vec2, f32)>,
}

/// Deterministic xorshift so the street is identical every run.
struct Rng(u64);

impl Rng {
    fn next_f32(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 40) as f32 / (1u64 << 24) as f32
    }

    fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.next_f32()
    }

    fn chance(&mut self, p: f32) -> bool {
        self.next_f32() < p
    }

    fn pick<T: Copy>(&mut self, items: &[T]) -> T {
        items[((self.next_f32() * items.len() as f32) as usize).min(items.len() - 1)]
    }
}

pub fn generate_district_layout(seed: u64) -> DistrictLayout {
    let mut rng = Rng(seed.max(1));
    let mut layout = DistrictLayout::default();

    for side in [-1.0f32, 1.0] {
        let mut z = AVENUE_START_Z;
        while z > AVENUE_END_Z {
            let length = rng.range(10.0, 20.0);
            let depth = rng.range(14.0, 26.0);
            // Taller towers further up the avenue for a rising skyline.
            let distance = (AVENUE_START_Z - z) / (AVENUE_START_Z - AVENUE_END_Z);
            let height = rng.range(16.0, 48.0) + distance * rng.range(10.0, 60.0);
            let center_z = z - length * 0.5;
            let mut windows = Vec::new();
            let mut y = 5.0;
            while y < height - 2.0 {
                if rng.chance(0.55) {
                    let width_fraction = rng.range(0.35, 0.95);
                    windows.push(WindowBand {
                        height: y,
                        width_fraction,
                        offset_fraction: rng
                            .range(-(1.0 - width_fraction) * 0.5, (1.0 - width_fraction) * 0.5),
                        tone: if rng.chance(0.62) {
                            WindowTone::Warm
                        } else {
                            WindowTone::Cool
                        },
                    });
                }
                y += FLOOR_HEIGHT;
            }
            let sign = rng.chance(0.5).then(|| {
                (
                    rng.pick(&NeonTone::ALL),
                    rng.range(4.0, 10.0),
                    rng.range(5.0, 9.0),
                )
            });
            layout.buildings.push(BuildingSpec {
                base_center: Vec3::new(side * (FACADE_OFFSET + depth * 0.5), 0.0, center_z),
                size: Vec3::new(depth, height, length),
                side,
                windows,
                sign,
                storefront: rng.pick(&NeonTone::ALL),
                awning: rng.chance(0.3),
            });
            z -= length + rng.range(1.0, 3.5);
        }

        let mut light_z = AVENUE_START_Z - 6.0;
        let mut index = 0;
        while light_z > AVENUE_END_Z {
            layout.street_lights.push(StreetLight {
                base: Vec3::new(side * (ROAD_HALF_WIDTH + 1.0), SIDEWALK_HEIGHT, light_z),
                // Alternate sides so the lit pools zigzag down the avenue.
                lit: (index % 2 == 0) == (side < 0.0),
            });
            light_z -= STREET_LIGHT_SPACING;
            index += 1;
        }
    }

    // Skybridges between facing towers that are both tall enough.
    for target_z in [-70.0f32, -165.0] {
        let tallest_at = |side: f32| {
            layout
                .buildings
                .iter()
                .filter(|b| b.side == side && (b.base_center.z - target_z).abs() < b.size.z * 0.5)
                .map(|b| b.size.y)
                .next()
        };
        if let (Some(west), Some(east)) = (tallest_at(-1.0), tallest_at(1.0)) {
            let y = (west.min(east) * 0.6).clamp(14.0, 34.0);
            layout.skybridges.push((
                Vec3::new(-FACADE_OFFSET, y, target_z),
                Vec3::new(FACADE_OFFSET, y, target_z),
                4.0,
            ));
        }
    }

    // Puddles: mostly on the road, some on the sidewalks.
    for _ in 0..PUDDLE_COUNT {
        let on_road = rng.chance(0.8);
        let x = if on_road {
            rng.range(-ROAD_HALF_WIDTH + 1.0, ROAD_HALF_WIDTH - 1.0)
        } else {
            let side = if rng.chance(0.5) { -1.0 } else { 1.0 };
            side * rng.range(ROAD_HALF_WIDTH + 0.8, FACADE_OFFSET - 0.8)
        };
        let y = if on_road {
            0.004
        } else {
            SIDEWALK_HEIGHT + 0.004
        };
        let size = Vec2::new(rng.range(1.5, 5.0), rng.range(2.0, 8.0));
        layout.puddles.push((
            Vec3::new(x, y, rng.range(AVENUE_END_Z + 4.0, AVENUE_START_Z - 4.0)),
            size,
            rng.range(-0.6, 0.6),
        ));
    }

    // Distant skyline ring, kept clear of the avenue corridor.
    for i in 0..56 {
        let angle = i as f32 / 56.0 * std::f32::consts::TAU + rng.range(-0.04, 0.04);
        let radius = rng.range(300.0, 520.0);
        let base = Vec3::new(angle.cos() * radius, 0.0, angle.sin() * radius - 120.0);
        let width = rng.range(18.0, 46.0);
        layout.skyline.push(SkylineTower {
            base_center: base,
            size: Vec3::new(width, rng.range(70.0, 260.0), rng.range(18.0, 46.0)),
            beacon: rng.chance(0.6),
        });
    }
    layout
}

/// Shared meshes and materials for the district.
struct Palette {
    cube: Handle<Mesh>,
    sphere: Handle<Mesh>,
    concrete: [Handle<StandardMaterial>; 3],
    asphalt: Handle<StandardMaterial>,
    puddle: Handle<StandardMaterial>,
    sidewalk: Handle<StandardMaterial>,
    lane_marking: Handle<StandardMaterial>,
    window_warm: Handle<StandardMaterial>,
    window_cool: Handle<StandardMaterial>,
    neon: [Handle<StandardMaterial>; 4],
    strip: [Handle<StandardMaterial>; 4],
    lamp_pole: Handle<StandardMaterial>,
    lamp_head: Handle<StandardMaterial>,
    skyline: Handle<StandardMaterial>,
    beacon: Handle<StandardMaterial>,
}

impl Palette {
    fn new(meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>) -> Self {
        let mut surface = |r: f32, g: f32, b: f32, roughness: f32, metallic: f32| {
            materials.add(StandardMaterial {
                base_color: Color::srgb(r, g, b),
                perceptual_roughness: roughness,
                metallic,
                ..default()
            })
        };
        let concrete = [
            surface(0.055, 0.06, 0.075, 0.85, 0.1),
            surface(0.07, 0.065, 0.075, 0.75, 0.2),
            surface(0.04, 0.05, 0.06, 0.6, 0.45),
        ];
        // Matte asphalt (above the reflection threshold); puddles are
        // near-mirrors, so the street reads wet in patches, not as glass.
        let asphalt = surface(0.035, 0.038, 0.046, 0.5, 0.0);
        let puddle = surface(0.012, 0.014, 0.02, 0.04, 0.0);
        let sidewalk = surface(0.07, 0.075, 0.085, 0.45, 0.0);
        let lamp_pole = surface(0.05, 0.05, 0.055, 0.4, 0.8);
        let skyline = surface(0.015, 0.018, 0.026, 0.9, 0.0);
        let mut glow = |color: LinearRgba| {
            materials.add(StandardMaterial {
                base_color: Color::BLACK,
                emissive: color,
                ..default()
            })
        };
        Self {
            cube: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
            sphere: meshes.add(Sphere::new(0.5).mesh().ico(2).expect("valid icosphere")),
            concrete,
            asphalt,
            puddle,
            sidewalk,
            lane_marking: glow(LinearRgba::rgb(0.05, 0.048, 0.034)),
            window_warm: glow(LinearRgba::rgb(2.2, 1.35, 0.6)),
            window_cool: glow(LinearRgba::rgb(0.55, 1.4, 2.4)),
            neon: NeonTone::ALL.map(|t| glow(t.emissive(9.0))),
            strip: NeonTone::ALL.map(|t| glow(t.emissive(4.0))),
            lamp_pole,
            lamp_head: glow(LinearRgba::rgb(6.0, 5.2, 4.2)),
            skyline,
            beacon: glow(LinearRgba::rgb(12.0, 0.3, 0.2)),
        }
    }

    fn neon(&self, tone: NeonTone) -> Handle<StandardMaterial> {
        self.neon[tone as usize].clone()
    }

    fn strip(&self, tone: NeonTone) -> Handle<StandardMaterial> {
        self.strip[tone as usize].clone()
    }
}

fn block(
    commands: &mut Commands,
    palette: &Palette,
    material: Handle<StandardMaterial>,
    center: Vec3,
    size: Vec3,
    solid: bool,
) -> Entity {
    let mut entity = commands.spawn((
        NeonDistrictPiece,
        Mesh3d(palette.cube.clone()),
        MeshMaterial3d(material),
        Transform::from_translation(center).with_scale(size),
    ));
    if solid {
        // Unit-cube collider scaled with the transform.
        entity.insert((RigidBody::Static, Collider::cuboid(1.0, 1.0, 1.0)));
    }
    entity.id()
}

pub fn spawn_neon_district(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let layout = generate_district_layout(DISTRICT_SEED);
    let palette = Palette::new(&mut meshes, &mut materials);
    let avenue_len = AVENUE_START_Z - AVENUE_END_Z;
    let avenue_mid = (AVENUE_START_Z + AVENUE_END_Z) * 0.5;

    // Ground: wet asphalt far beyond the avenue, raised sidewalks, lane dashes.
    block(
        &mut commands,
        &palette,
        palette.asphalt.clone(),
        Vec3::new(0.0, -0.5, -120.0),
        Vec3::new(1400.0, 1.0, 1400.0),
        true,
    );
    for side in [-1.0f32, 1.0] {
        let width = FACADE_OFFSET - ROAD_HALF_WIDTH;
        block(
            &mut commands,
            &palette,
            palette.sidewalk.clone(),
            Vec3::new(
                side * (ROAD_HALF_WIDTH + width * 0.5),
                SIDEWALK_HEIGHT * 0.5,
                avenue_mid,
            ),
            Vec3::new(width, SIDEWALK_HEIGHT, avenue_len),
            true,
        );
    }
    for &(center, size, yaw) in &layout.puddles {
        commands.spawn((
            NeonDistrictPiece,
            Mesh3d(palette.cube.clone()),
            MeshMaterial3d(palette.puddle.clone()),
            Transform::from_translation(center)
                .with_rotation(Quat::from_rotation_y(yaw))
                .with_scale(Vec3::new(size.x, 0.006, size.y)),
        ));
    }
    let mut dash_z = AVENUE_START_Z - 2.0;
    while dash_z > AVENUE_END_Z {
        block(
            &mut commands,
            &palette,
            palette.lane_marking.clone(),
            Vec3::new(0.0, 0.01, dash_z),
            Vec3::new(0.12, 0.02, 2.4),
            false,
        );
        dash_z -= 7.0;
    }

    for (i, b) in layout.buildings.iter().enumerate() {
        let body = palette.concrete[i % palette.concrete.len()].clone();
        block(
            &mut commands,
            &palette,
            body,
            b.base_center + Vec3::Y * b.size.y * 0.5,
            b.size,
            true,
        );

        // Window bands sit just proud of the street-facing wall.
        let face_x = b.facade_x() - b.side * 0.06;
        for w in &b.windows {
            let material = match w.tone {
                WindowTone::Warm => palette.window_warm.clone(),
                WindowTone::Cool => palette.window_cool.clone(),
            };
            block(
                &mut commands,
                &palette,
                material,
                Vec3::new(
                    face_x,
                    w.height,
                    b.base_center.z + w.offset_fraction * b.size.z,
                ),
                Vec3::new(0.08, 1.1, b.size.z * w.width_fraction),
                false,
            );
        }

        // Storefront strip just above the doors.
        block(
            &mut commands,
            &palette,
            palette.strip(b.storefront),
            Vec3::new(face_x, 3.6, b.base_center.z),
            Vec3::new(0.1, 0.16, b.size.z * 0.86),
            false,
        );
        // Its glow spills onto the sidewalk and the wet road.
        commands.spawn((
            NeonDistrictPiece,
            PointLight {
                color: b.storefront.light_color(),
                intensity: 140_000.0,
                range: 12.0,
                radius: 0.5,
                shadows_enabled: false,
                ..default()
            },
            Transform::from_xyz(b.facade_x() - b.side * 1.6, 3.0, b.base_center.z),
        ));

        // Blade sign standing out over the sidewalk, with a coloured light.
        if let Some((tone, height, base_y)) = b.sign {
            let center = Vec3::new(
                b.facade_x() - b.side * 0.9,
                base_y + height * 0.5,
                b.base_center.z + b.size.z * 0.3,
            );
            block(
                &mut commands,
                &palette,
                palette.neon(tone),
                center,
                Vec3::new(1.6, height, 0.28),
                false,
            );
            commands.spawn((
                NeonDistrictPiece,
                PointLight {
                    color: tone.light_color(),
                    intensity: 220_000.0,
                    range: 16.0,
                    radius: 0.4,
                    shadows_enabled: false,
                    ..default()
                },
                Transform::from_translation(center - Vec3::X * b.side * 1.6),
            ));
        }

        if b.awning {
            let transform = Transform::from_xyz(
                b.facade_x() - b.side * 1.3,
                4.2,
                b.base_center.z - b.size.z * 0.2,
            )
            .with_rotation(Quat::from_rotation_z(b.side * 0.18));
            let awning = spawn_bounce_awning(
                &mut commands,
                &mut meshes,
                &mut materials,
                transform,
                Vec2::new(2.6, (b.size.z * 0.4).max(3.0)),
                BounceAwning::default(),
            );
            commands.entity(awning).insert(NeonDistrictPiece);
        }
    }

    for light in &layout.street_lights {
        let side = light.base.x.signum();
        block(
            &mut commands,
            &palette,
            palette.lamp_pole.clone(),
            light.base + Vec3::Y * 3.5,
            Vec3::new(0.18, 7.0, 0.18),
            false,
        );
        let head = light.base + Vec3::new(-side * 1.2, 7.0, 0.0);
        block(
            &mut commands,
            &palette,
            palette.lamp_pole.clone(),
            light.base + Vec3::new(-side * 0.6, 7.05, 0.0),
            Vec3::new(1.4, 0.12, 0.12),
            false,
        );
        block(
            &mut commands,
            &palette,
            palette.lamp_head.clone(),
            head - Vec3::Y * 0.1,
            Vec3::new(0.9, 0.1, 0.35),
            false,
        );
        if light.lit {
            commands.spawn((
                NeonDistrictPiece,
                PointLight {
                    color: Color::srgb(1.0, 0.86, 0.68),
                    intensity: 900_000.0,
                    range: 26.0,
                    radius: 0.3,
                    shadows_enabled: false,
                    ..default()
                },
                Transform::from_translation(head - Vec3::Y * 0.4),
            ));
        }
    }

    for &(start, end, width) in &layout.skybridges {
        if let Some(bridge) = spawn_skybridge(
            &mut commands,
            &mut meshes,
            &mut materials,
            start,
            end,
            width,
        ) {
            commands.entity(bridge).insert(NeonDistrictPiece);
        }
        // Cyan light line along the underside.
        block(
            &mut commands,
            &palette,
            palette.strip(NeonTone::Cyan),
            (start + end) * 0.5 - Vec3::Y * 0.25,
            Vec3::new((end - start).length(), 0.08, 0.25),
            false,
        );
    }

    for tower in &layout.skyline {
        block(
            &mut commands,
            &palette,
            palette.skyline.clone(),
            tower.base_center + Vec3::Y * tower.size.y * 0.5,
            tower.size,
            false,
        );
        if tower.beacon {
            commands.spawn((
                NeonDistrictPiece,
                Mesh3d(palette.sphere.clone()),
                MeshMaterial3d(palette.beacon.clone()),
                Transform::from_translation(tower.base_center + Vec3::Y * (tower.size.y + 1.0))
                    .with_scale(Vec3::splat(2.2)),
            ));
        }
    }
}

/// Spawns the neon district once at startup.
pub struct NeonDistrictPlugin;

impl Plugin for NeonDistrictPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_neon_district);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_is_deterministic() {
        assert_eq!(
            generate_district_layout(DISTRICT_SEED),
            generate_district_layout(DISTRICT_SEED)
        );
        assert_ne!(
            generate_district_layout(DISTRICT_SEED),
            generate_district_layout(DISTRICT_SEED + 1)
        );
    }

    #[test]
    fn buildings_line_both_sides_without_blocking_the_avenue() {
        let layout = generate_district_layout(DISTRICT_SEED);
        let west = layout.buildings.iter().filter(|b| b.side < 0.0).count();
        let east = layout.buildings.iter().filter(|b| b.side > 0.0).count();
        assert!(west >= 10 && east >= 10, "{west} / {east}");
        for b in &layout.buildings {
            let inner_x = b.base_center.x.abs() - b.size.x * 0.5;
            assert!(
                (inner_x - FACADE_OFFSET).abs() < 1e-3,
                "facade on the building line"
            );
            assert!(b.size.y >= 16.0 && b.size.y <= 110.0);
            for w in &b.windows {
                assert!(w.height < b.size.y, "windows inside the building");
                assert!(
                    w.offset_fraction.abs() + w.width_fraction * 0.5 <= 0.5 + 1e-4,
                    "band within the facade"
                );
            }
        }
        // No two buildings on a side overlap along the avenue.
        for side in [-1.0f32, 1.0] {
            let mut spans: Vec<(f32, f32)> = layout
                .buildings
                .iter()
                .filter(|b| b.side == side)
                .map(|b| {
                    (
                        b.base_center.z - b.size.z * 0.5,
                        b.base_center.z + b.size.z * 0.5,
                    )
                })
                .collect();
            spans.sort_by(|a, b| a.0.total_cmp(&b.0));
            assert!(spans.windows(2).all(|w| w[0].1 <= w[1].0 + 1e-3));
        }
    }

    #[test]
    fn spawn_point_is_on_open_road() {
        let layout = generate_district_layout(DISTRICT_SEED);
        let spawn = Vec3::ZERO;
        assert!(spawn.x.abs() < ROAD_HALF_WIDTH);
        for b in &layout.buildings {
            let half = b.size * 0.5;
            let local = spawn - b.base_center;
            assert!(
                local.x.abs() > half.x || local.z.abs() > half.z,
                "spawn inside a building"
            );
        }
    }

    #[test]
    fn puddles_lie_on_the_avenue() {
        let layout = generate_district_layout(DISTRICT_SEED);
        assert_eq!(layout.puddles.len(), PUDDLE_COUNT);
        for (center, size, _) in &layout.puddles {
            assert!(center.x.abs() < FACADE_OFFSET, "{center}");
            assert!(center.z <= AVENUE_START_Z && center.z >= AVENUE_END_Z);
            assert!(center.y < SIDEWALK_HEIGHT + 0.01);
            assert!(size.min_element() > 0.5);
        }
        let on_road = layout
            .puddles
            .iter()
            .filter(|(c, _, _)| c.x.abs() < ROAD_HALF_WIDTH)
            .count();
        assert!(on_road > PUDDLE_COUNT / 2);
    }

    #[test]
    fn light_budget_stays_bounded() {
        let layout = generate_district_layout(DISTRICT_SEED);
        let lamps = layout.street_lights.iter().filter(|l| l.lit).count();
        let signs = layout.buildings.iter().filter(|b| b.sign.is_some()).count();
        assert!(lamps + signs <= 48, "{lamps} lamps + {signs} signs");
        assert!(lamps >= 10 && signs >= 8);
        assert!(!layout.skybridges.is_empty());
        assert!(layout
            .skyline
            .iter()
            .all(|t| t.base_center.length() > 150.0));
    }

    #[test]
    fn spawning_builds_a_lit_street() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>()
            .add_plugins(NeonDistrictPlugin);
        app.update();
        let world = app.world_mut();
        let pieces = world.query::<&NeonDistrictPiece>().iter(world).count();
        let lights = world.query::<&PointLight>().iter(world).count();
        let solids = world
            .query_filtered::<&Collider, With<NeonDistrictPiece>>()
            .iter(world)
            .count();
        assert!(pieces > 300 && pieces < 2_000, "{pieces} entities");
        // Lamps + signs + one storefront spill light per building.
        assert!((40..=110).contains(&lights), "{lights} lights");
        assert!(solids > 20);
        // Shared handles keep unique mesh / material counts tiny.
        assert!(world.resource::<Assets<Mesh>>().len() < 20);
        assert!(world.resource::<Assets<StandardMaterial>>().len() < 40);
    }
}
