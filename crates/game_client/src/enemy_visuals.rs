//! Bodies, health bars and death bursts for Syndicate enemies and the
//! Gorgon. Enemies are simulation entities (capsule centre at their
//! transform); this module dresses them when they appear.

use crate::character::{spawn_operative_body_at, CharacterAppearance, MorphLibrary, Preset};
use bevy::pbr::NotShadowCaster;
use bevy::prelude::*;
use game_ai::{BossGorgon, Enemy, EnemyArchetype, EnemyDefeatedEvent};
use game_rpg::Health;

const BAR_WIDTH: f32 = 1.1;
const BURST_SECONDS: f32 = 0.6;

/// A health bar that floats above `owner` and faces the camera.
#[derive(Component, Debug, Clone, Copy)]
pub struct EnemyHealthBar {
    pub owner: Entity,
    pub height: f32,
}

/// The fill part of a health bar.
#[derive(Component, Debug, Clone, Copy)]
pub struct HealthBarFill;

/// A short flash of light where an enemy fell.
#[derive(Component, Debug, Clone, Copy)]
pub struct DeathBurst {
    pub age: f32,
}

/// Shared meshes and materials for every enemy body.
#[derive(Resource)]
pub struct EnemyAssets {
    cube: Handle<Mesh>,
    sphere: Handle<Mesh>,
    ring: Handle<Mesh>,
    plate: Handle<StandardMaterial>,
    suit: Handle<StandardMaterial>,
    red: Handle<StandardMaterial>,
    magenta: Handle<StandardMaterial>,
    amber: Handle<StandardMaterial>,
    violet: Handle<StandardMaterial>,
    bar_back: Handle<StandardMaterial>,
    bar_fill: Handle<StandardMaterial>,
    burst: Handle<StandardMaterial>,
}

impl FromWorld for EnemyAssets {
    fn from_world(world: &mut World) -> Self {
        let (cube, sphere, ring) = {
            let mut meshes = world.resource_mut::<Assets<Mesh>>();
            (
                meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
                meshes.add(Sphere::new(1.0).mesh().uv(24, 16)),
                meshes.add(Torus::new(0.85, 1.0)),
            )
        };
        let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
        let mut lit = |base: Color, roughness: f32, metallic: f32| {
            materials.add(StandardMaterial {
                base_color: base,
                perceptual_roughness: roughness,
                metallic,
                ..default()
            })
        };
        let plate = lit(Color::srgb(0.3, 0.27, 0.28), 0.4, 0.3);
        let suit = lit(Color::srgb(0.08, 0.075, 0.08), 0.7, 0.05);
        let mut glow = |r: f32, g: f32, b: f32| {
            materials.add(StandardMaterial {
                base_color: Color::srgb(r * 0.1, g * 0.1, b * 0.1),
                emissive: LinearRgba::rgb(r, g, b),
                ..default()
            })
        };
        let red = glow(7.0, 0.5, 0.8);
        let magenta = glow(5.5, 0.5, 4.5);
        let amber = glow(6.0, 2.5, 0.1);
        let violet = glow(2.5, 0.8, 6.5);
        let unlit = |materials: &mut Assets<StandardMaterial>, color: Color| {
            materials.add(StandardMaterial {
                base_color: color,
                unlit: true,
                alpha_mode: AlphaMode::Blend,
                ..default()
            })
        };
        let bar_back = unlit(&mut materials, Color::srgba(0.0, 0.0, 0.0, 0.6));
        let bar_fill = unlit(&mut materials, Color::srgb(1.0, 0.2, 0.28));
        let burst = materials.add(StandardMaterial {
            base_color: Color::srgba(1.0, 0.4, 0.3, 0.7),
            emissive: LinearRgba::rgb(30.0, 6.0, 4.0),
            unlit: true,
            alpha_mode: AlphaMode::Add,
            ..default()
        });
        Self {
            cube,
            sphere,
            ring,
            plate,
            suit,
            red,
            magenta,
            amber,
            violet,
            bar_back,
            bar_fill,
            burst,
        }
    }
}

/// Body proportions and accent for each archetype.
struct Build {
    scale: f32,
    bulk: f32,
    accent: fn(&EnemyAssets) -> Handle<StandardMaterial>,
    shield: bool,
    blade: bool,
}

fn build_for(archetype: EnemyArchetype) -> Build {
    use EnemyArchetype::*;
    let accent: fn(&EnemyAssets) -> Handle<StandardMaterial> = match archetype {
        CyberNinja => |a| a.magenta.clone(),
        ShieldedHeavy | MechaEnforcer => |a| a.amber.clone(),
        Netrunner => |a| a.violet.clone(),
        _ => |a| a.red.clone(),
    };
    Build {
        scale: match archetype {
            MechaEnforcer => 1.35,
            ShieldedHeavy => 1.12,
            _ => 1.0,
        },
        bulk: match archetype {
            CyberNinja => 0.82,
            ShieldedHeavy | MechaEnforcer => 1.35,
            _ => 1.0,
        },
        accent,
        shield: archetype == ShieldedHeavy,
        blade: archetype == CyberNinja,
    }
}

fn part(
    mesh: &Handle<Mesh>,
    material: &Handle<StandardMaterial>,
    at: Vec3,
    size: Vec3,
) -> (Mesh3d, MeshMaterial3d<StandardMaterial>, Transform) {
    (
        Mesh3d(mesh.clone()),
        MeshMaterial3d(material.clone()),
        Transform::from_translation(at).with_scale(size),
    )
}

/// A humanoid built around the capsule centre (feet at -0.93).
/// The capsule centre sits this far above the feet.
const ENEMY_FEET: f32 = 0.93;

/// A Syndicate look for `archetype`; `seed` varies the face and build so
/// no two enemies in a wave are twins.
pub fn enemy_look(
    archetype: EnemyArchetype,
    seed: u64,
    face_sliders: &[&str],
) -> CharacterAppearance {
    use crate::character::appearance::{HAIR_COLORS, SKIN_TONES, SUIT_COLORS};
    use EnemyArchetype::*;
    let feminine = seed % 3 == 0;
    let preset = match archetype {
        ShieldedHeavy | MechaEnforcer => Preset::Bruiser,
        CyberNinja if feminine => Preset::Wraith,
        CyberNinja => Preset::Jun,
        Netrunner if feminine => Preset::Hana,
        Netrunner => Preset::Jun,
        _ if feminine => Preset::Runner,
        _ => Preset::Vanguard,
    };
    let mut look = CharacterAppearance::preset(preset);
    look.randomize(face_sliders, seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    look.set_slider("muscle", look.slider("muscle").max(0.3));
    look.skin_tone = SKIN_TONES[(seed as usize / 7) % SKIN_TONES.len()];
    look.hair_color = HAIR_COLORS[(seed as usize / 3) % 3];
    look.suit = SUIT_COLORS[0];
    look.lashes = true;
    let (clothes, hair): (&[&str], &[&str]) = match archetype {
        ShieldedHeavy | MechaEnforcer => (&["male_worksuit01", "shoes06"], &["short02", "short01"]),
        CyberNinja => (
            if feminine {
                &["female_sportsuit01", "shoes02"]
            } else {
                &["male_casualsuit01", "shoes02"]
            },
            if feminine {
                &["ponytail01", "braid01"]
            } else {
                &["short04", "short03"]
            },
        ),
        Netrunner => (
            if feminine {
                &["female_elegantsuit01", "shoes02"]
            } else {
                &["male_casualsuit04", "shoes01"]
            },
            if feminine {
                &["bob01", "bob02"]
            } else {
                &["short03"]
            },
        ),
        _ => (
            if feminine {
                &["female_casualsuit01", "shoes02"]
            } else {
                &["male_casualsuit04", "shoes01"]
            },
            if feminine {
                &["bob02", "long01"]
            } else {
                &["short01", "short03", "short04"]
            },
        ),
    };
    look.clothing = clothes.iter().map(|s| (*s).to_owned()).collect();
    look.hair_mesh = hair[(seed as usize) % hair.len()].to_owned();
    look
}

/// Weapons and gear carried on top of the body.
fn spawn_enemy_gear(parent: &mut ChildBuilder, a: &EnemyAssets, build: &Build) {
    let accent = (build.accent)(a);
    if build.shield {
        parent.spawn(part(
            &a.cube,
            &a.plate,
            Vec3::new(0.0, 0.05, -0.5),
            Vec3::new(0.95, 1.35, 0.08),
        ));
        parent.spawn(part(
            &a.cube,
            &accent,
            Vec3::new(0.0, 0.05, -0.55),
            Vec3::new(0.7, 0.05, 0.02),
        ));
    }
    if build.blade {
        parent.spawn(part(
            &a.cube,
            &accent,
            Vec3::new(0.32, -0.05, -0.4),
            Vec3::new(0.025, 0.02, 0.9),
        ));
    }
    // A glowing Syndicate collar so enemies read as enemies at night.
    parent.spawn((
        part(
            &a.cube,
            &accent,
            Vec3::new(0.0, 0.52, -0.02),
            Vec3::new(0.22, 0.025, 0.22),
        ),
        NotShadowCaster,
    ));
}

fn spawn_humanoid(parent: &mut ChildBuilder, a: &EnemyAssets, build: &Build) {
    let accent = (build.accent)(a);
    let w = build.bulk;
    parent
        .spawn((
            Transform::from_scale(Vec3::splat(build.scale)),
            Visibility::default(),
        ))
        .with_children(|b| {
            b.spawn(part(
                &a.cube,
                &a.plate,
                Vec3::new(0.0, 0.28, 0.0),
                Vec3::new(0.5 * w, 0.6, 0.3 * w),
            ));
            b.spawn(part(
                &a.cube,
                &accent,
                Vec3::new(0.0, 0.34, -0.16 * w),
                Vec3::new(0.1, 0.22, 0.02),
            ));
            b.spawn(part(
                &a.cube,
                &a.plate,
                Vec3::new(0.0, 0.76, 0.0),
                Vec3::new(0.27, 0.3, 0.29),
            ));
            b.spawn(part(
                &a.cube,
                &accent,
                Vec3::new(0.0, 0.78, -0.15),
                Vec3::new(0.24, 0.06, 0.02),
            ));
            for x in [-1.0f32, 1.0] {
                b.spawn(part(
                    &a.cube,
                    &a.suit,
                    Vec3::new(x * 0.33 * w, 0.18, 0.0),
                    Vec3::new(0.12, 0.55, 0.14),
                ));
                b.spawn(part(
                    &a.cube,
                    &a.plate,
                    Vec3::new(x * 0.3 * w, 0.52, 0.0),
                    Vec3::new(0.16 * w, 0.12, 0.24),
                ));
                b.spawn(part(
                    &a.cube,
                    &a.suit,
                    Vec3::new(x * 0.13, -0.5, 0.0),
                    Vec3::new(0.16, 0.8, 0.18),
                ));
                b.spawn(part(
                    &a.cube,
                    &accent,
                    Vec3::new(x * 0.13, -0.9, -0.02),
                    Vec3::new(0.15, 0.04, 0.2),
                ));
            }
            if build.shield {
                b.spawn(part(
                    &a.cube,
                    &a.plate,
                    Vec3::new(0.0, 0.15, -0.42),
                    Vec3::new(0.95, 1.3, 0.08),
                ));
                b.spawn(part(
                    &a.cube,
                    &accent,
                    Vec3::new(0.0, 0.15, -0.47),
                    Vec3::new(0.7, 0.05, 0.02),
                ));
            }
            if build.blade {
                b.spawn(part(
                    &a.cube,
                    &accent,
                    Vec3::new(0.36, 0.0, -0.35),
                    Vec3::new(0.025, 0.02, 0.9),
                ));
            }
        });
}

fn spawn_drone(parent: &mut ChildBuilder, a: &EnemyAssets) {
    parent.spawn(part(&a.sphere, &a.plate, Vec3::ZERO, Vec3::splat(0.38)));
    parent.spawn(part(
        &a.sphere,
        &a.red,
        Vec3::new(0.0, 0.0, -0.33),
        Vec3::splat(0.1),
    ));
    parent.spawn((
        part(&a.ring, &a.red, Vec3::ZERO, Vec3::new(0.62, 0.3, 0.62)),
        NotShadowCaster,
    ));
}

fn spawn_gorgon(parent: &mut ChildBuilder, a: &EnemyAssets) {
    parent
        .spawn((
            Transform::from_scale(Vec3::splat(1.7)),
            Visibility::default(),
        ))
        .with_children(|b| {
            b.spawn(part(
                &a.cube,
                &a.plate,
                Vec3::new(0.0, 0.3, 0.0),
                Vec3::new(0.95, 0.75, 0.6),
            ));
            b.spawn(part(
                &a.sphere,
                &a.red,
                Vec3::new(0.0, 0.35, -0.31),
                Vec3::splat(0.12),
            ));
            b.spawn(part(
                &a.cube,
                &a.plate,
                Vec3::new(0.0, 0.82, -0.05),
                Vec3::new(0.36, 0.26, 0.4),
            ));
            b.spawn(part(
                &a.cube,
                &a.red,
                Vec3::new(0.0, 0.84, -0.26),
                Vec3::new(0.3, 0.05, 0.02),
            ));
            for x in [-1.0f32, 1.0] {
                b.spawn(part(
                    &a.cube,
                    &a.plate,
                    Vec3::new(x * 0.62, 0.55, 0.0),
                    Vec3::new(0.34, 0.3, 0.5),
                ));
                b.spawn(part(
                    &a.cube,
                    &a.suit,
                    Vec3::new(x * 0.66, 0.05, -0.12),
                    Vec3::new(0.24, 0.55, 0.3),
                ));
                // Chaingun barrels.
                b.spawn(part(
                    &a.cube,
                    &a.plate,
                    Vec3::new(x * 0.66, -0.15, -0.5),
                    Vec3::new(0.16, 0.16, 0.7),
                ));
                b.spawn(part(
                    &a.cube,
                    &a.amber,
                    Vec3::new(x * 0.66, -0.15, -0.86),
                    Vec3::new(0.1, 0.1, 0.03),
                ));
                b.spawn(part(
                    &a.cube,
                    &a.suit,
                    Vec3::new(x * 0.25, -0.5, 0.0),
                    Vec3::new(0.28, 0.85, 0.34),
                ));
                b.spawn(part(
                    &a.cube,
                    &a.red,
                    Vec3::new(x * 0.25, -0.94, -0.05),
                    Vec3::new(0.3, 0.05, 0.4),
                ));
            }
        });
}

fn spawn_health_bar(commands: &mut Commands, a: &EnemyAssets, owner: Entity, height: f32) {
    commands
        .spawn((
            EnemyHealthBar { owner, height },
            Transform::default(),
            Visibility::default(),
        ))
        .with_children(|bar| {
            bar.spawn((
                part(
                    &a.cube,
                    &a.bar_back,
                    Vec3::ZERO,
                    Vec3::new(BAR_WIDTH + 0.06, 0.12, 0.01),
                ),
                NotShadowCaster,
            ));
            bar.spawn((
                HealthBarFill,
                part(
                    &a.cube,
                    &a.bar_fill,
                    Vec3::new(0.0, 0.0, -0.01),
                    Vec3::new(BAR_WIDTH, 0.07, 0.01),
                ),
                NotShadowCaster,
            ));
        });
}

pub fn dress_new_enemies(
    mut commands: Commands,
    assets: Res<EnemyAssets>,
    asset_server: Res<AssetServer>,
    library: Option<Res<MorphLibrary>>,
    enemies: Query<(Entity, &Enemy), Added<Enemy>>,
    bosses: Query<Entity, Added<BossGorgon>>,
) {
    let face_sliders: Vec<&str> = library.as_deref().map_or_else(Vec::new, |l| {
        l.sliders
            .iter()
            .filter(|s| {
                matches!(
                    s.category.as_str(),
                    "Head" | "Jaw" | "Face" | "Eyes" | "Nose" | "Mouth" | "Ears"
                )
            })
            .map(|s| s.id.as_str())
            .collect()
    });
    for (entity, enemy) in &enemies {
        let flying = enemy.archetype == EnemyArchetype::SwarmDrone;
        let build = build_for(enemy.archetype);
        commands
            .entity(entity)
            .insert(Visibility::default())
            .with_children(|root| {
                if flying {
                    spawn_drone(root, &assets);
                } else if library.is_some() {
                    spawn_enemy_gear(root, &assets, &build);
                } else {
                    spawn_humanoid(root, &assets, &build);
                }
            });
        if !flying && library.is_some() {
            // A real human body: feet on the ground, facing the enemy's forward.
            let look = enemy_look(enemy.archetype, entity.to_bits(), &face_sliders);
            let at = Transform::from_xyz(0.0, -ENEMY_FEET, 0.0)
                .with_rotation(Quat::from_rotation_y(std::f32::consts::PI))
                .with_scale(Vec3::splat(build.scale));
            spawn_operative_body_at(&mut commands, &asset_server, entity, look, at);
        }
        let height = if flying {
            0.7
        } else {
            1.25 * build_for(enemy.archetype).scale
        };
        spawn_health_bar(&mut commands, &assets, entity, height);
    }
    for entity in &bosses {
        commands
            .entity(entity)
            .insert(Visibility::default())
            .with_children(|root| spawn_gorgon(root, &assets));
        spawn_health_bar(&mut commands, &assets, entity, 2.6);
    }
}

/// Health as a fraction, from [`Health`] or the Gorgon's own pool.
pub fn health_fraction(health: Option<&Health>, boss: Option<&BossGorgon>) -> f32 {
    let (current, max) = match (boss, health) {
        (Some(b), _) => (b.health_current, b.health_max),
        (None, Some(h)) => (h.current, h.max),
        (None, None) => return 1.0,
    };
    if max > 0.0 {
        (current / max).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

#[allow(clippy::type_complexity)]
pub fn update_health_bars(
    mut commands: Commands,
    owners: Query<(&GlobalTransform, Option<&Health>, Option<&BossGorgon>)>,
    cameras: Query<&GlobalTransform, With<Camera3d>>,
    mut bars: Query<(Entity, &EnemyHealthBar, &mut Transform, &Children), Without<HealthBarFill>>,
    mut fills: Query<&mut Transform, With<HealthBarFill>>,
) {
    let camera = cameras.iter().next().map(GlobalTransform::translation);
    for (entity, bar, mut transform, children) in &mut bars {
        let Ok((owner, health, boss)) = owners.get(bar.owner) else {
            commands.entity(entity).despawn_recursive();
            continue;
        };
        let at = owner.translation() + Vec3::Y * bar.height;
        transform.translation = at;
        if let Some(camera) = camera {
            let to_camera = Vec3::new(camera.x - at.x, 0.0, camera.z - at.z);
            if to_camera.length_squared() > 1e-4 {
                transform.rotation = Quat::from_rotation_y(to_camera.x.atan2(to_camera.z));
            }
        }
        let fraction = health_fraction(health, boss);
        for child in children.iter() {
            if let Ok(mut fill) = fills.get_mut(*child) {
                fill.scale.x = (BAR_WIDTH * fraction).max(0.001);
                fill.translation.x = -BAR_WIDTH * (1.0 - fraction) * 0.5;
            }
        }
    }
}

pub fn spawn_death_bursts(
    mut commands: Commands,
    assets: Res<EnemyAssets>,
    mut defeated: EventReader<EnemyDefeatedEvent>,
) {
    for event in defeated.read() {
        commands
            .spawn((
                DeathBurst { age: 0.0 },
                part(
                    &assets.sphere,
                    &assets.burst,
                    event.position,
                    Vec3::splat(0.2),
                ),
                NotShadowCaster,
            ))
            .with_children(|burst| {
                burst.spawn(PointLight {
                    color: Color::srgb(1.0, 0.35, 0.25),
                    intensity: 600_000.0,
                    range: 14.0,
                    shadows_enabled: false,
                    ..default()
                });
            });
    }
}

pub fn age_death_bursts(
    time: Res<Time>,
    mut commands: Commands,
    mut bursts: Query<(Entity, &mut DeathBurst, &mut Transform, &Children)>,
    mut lights: Query<&mut PointLight>,
) {
    for (entity, mut burst, mut transform, children) in &mut bursts {
        burst.age += time.delta_secs();
        let t = burst.age / BURST_SECONDS;
        if t >= 1.0 {
            commands.entity(entity).despawn_recursive();
            continue;
        }
        transform.scale = Vec3::splat(0.2 + 1.6 * t);
        for child in children.iter() {
            if let Ok(mut light) = lights.get_mut(*child) {
                light.intensity = 600_000.0 * (1.0 - t).powi(2);
            }
        }
    }
}

/// Enemy presentation for the windowed client.
pub struct EnemyVisualsPlugin;

impl Plugin for EnemyVisualsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<EnemyAssets>().add_systems(
            Update,
            (
                dress_new_enemies,
                update_health_bars,
                spawn_death_bursts,
                age_death_bursts,
            ),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_fraction_prefers_the_boss_pool_and_clamps() {
        let mut health = Health::new(100.0);
        health.current = 25.0;
        assert!((health_fraction(Some(&health), None) - 0.25).abs() < 1e-6);
        assert_eq!(health_fraction(None, None), 1.0);
        health.current = -5.0;
        assert_eq!(health_fraction(Some(&health), None), 0.0);
    }

    #[test]
    fn builds_differ_by_archetype() {
        assert!(build_for(EnemyArchetype::ShieldedHeavy).shield);
        assert!(build_for(EnemyArchetype::CyberNinja).blade);
        assert!(build_for(EnemyArchetype::MechaEnforcer).scale > 1.0);
        assert!(!build_for(EnemyArchetype::StreetGrunt).shield);
    }
}
