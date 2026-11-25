//! Night-city presentation for the windowed client: HDR camera with bloom
//! and filmic tonemapping, district-driven fog and rain, moonlight, the
//! operative avatar, and the local player's starting kit.

use crate::character::{spawn_operative_body, CharacterAppearance};
use crate::client_network::{apply_visual_error_decay, LocalPredictedPlayer, PredictedVisual};
use avian3d::prelude::{Collider, RigidBody};
use bevy::core_pipeline::bloom::Bloom;
use bevy::core_pipeline::tonemapping::{DebandDither, Tonemapping};
use bevy::pbr::{
    CascadeShadowConfigBuilder, DefaultOpaqueRendererMethod, DistanceFog, FogFalloff,
    NotShadowCaster, ScreenSpaceReflections,
};
use bevy::prelude::*;
use game_camera::CameraRigController;
use game_combat::{AttackPhase, MeleeCombatState};
use game_economy::PlayerInventory;
use game_input::{default_input_map, local_player_input_bundle};
use game_parkour::{FlowState, KinematicVelocity};
use game_rpg::{CharacterSkills, Health, PlayerQuestLog, Shields};
use game_server::SPRINT_SPEED;
use game_ui::HudFont;
use game_world::district_manifest::get_district_at_position;

pub const HUD_FONT_PATH: &str = "fonts/JetBrainsMono-Medium.ttf";
pub const HUD_TITLE_FONT_PATH: &str = "fonts/Rajdhani-Bold.ttf";
/// Height of the avatar's hips above its feet; the simulated position is
/// the feet.
pub const AVATAR_HIP_HEIGHT: f32 = 0.83;
/// Height of the player hurtbox centre above the feet.
pub const PLAYER_HURTBOX_CENTER: f32 = 0.93;
/// Brightness of the fill light carried by the camera.
const CAMERA_FILL_LUMENS: f32 = 90_000.0;
/// Operative health; shields absorb the first 100 points.
pub const PLAYER_MAX_HEALTH: f32 = 150.0;
const AVATAR_TURN_RATE: f32 = 12.0;
const AVATAR_MAX_LEAN: f32 = 0.2;
const ATMOSPHERE_EASE_RATE: f32 = 1.5;
pub const RAIN_DROP_COUNT: usize = 900;
const RAIN_BOX_HALF: Vec3 = Vec3::new(22.0, 14.0, 22.0);
const RAIN_VELOCITY: Vec3 = Vec3::new(2.5, -24.0, 1.0);

/// Fog and rain outside every district.
const OPEN_AIR_FOG: Color = Color::srgb(0.035, 0.05, 0.085);
const OPEN_AIR_FOG_DENSITY: f32 = 0.0055;
/// sRGB haze added to each district's tint.
const HAZE_BASE: (f32, f32, f32) = (0.025, 0.035, 0.06);

/// Starting kit so the HUD, skills and inventory show real data.
pub const STARTER_ITEMS: [(u32, u32); 4] = [(2001, 1), (2002, 1), (1021, 5), (1001, 24)];

/// The visual body following a predicted player.
#[derive(Component, Debug, Clone, Copy)]
pub struct OperativeAvatar {
    pub source: Entity,
    pub yaw: f32,
}

/// The avatar part that pivots at the hips (leans, crouches into slides).
#[derive(Component, Debug, Clone, Copy)]
pub struct AvatarBody;

/// The katana resting in its sheath; hidden while the blade is drawn.
#[derive(Component, Debug, Clone, Copy)]
pub struct SheathedBlade;

#[derive(Component, Debug, Clone, Copy)]
pub struct RainDrop;

/// What the atmosphere is easing toward, from the camera's district.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct AtmosphereState {
    pub fog_color: Color,
    pub fog_density: f32,
    pub rain_intensity: f32,
}

impl Default for AtmosphereState {
    fn default() -> Self {
        Self {
            fog_color: OPEN_AIR_FOG,
            fog_density: OPEN_AIR_FOG_DENSITY,
            rain_intensity: 0.3,
        }
    }
}

/// Fog colour, exponential-squared density and rain amount at `position`.
pub fn district_atmosphere(position: Vec3) -> AtmosphereState {
    match get_district_at_position(position) {
        Some(zone) => {
            // City light scattered in the rain: the district tint over a
            // base haze, so towers silhouette against a glowing sky.
            let tint = zone.ambient_color.to_srgba();
            AtmosphereState {
                fog_color: Color::srgb(
                    tint.red + HAZE_BASE.0,
                    tint.green + HAZE_BASE.1,
                    tint.blue + HAZE_BASE.2,
                ),
                fog_density: zone.fog_density * 0.2,
                rain_intensity: zone.rain_intensity,
            }
        }
        None => AtmosphereState::default(),
    }
}

/// Meshes and materials for the operative's katana and slash trail.
#[derive(Resource)]
pub struct AvatarAssets {
    slash_arc: Handle<Mesh>,
    slash: Handle<StandardMaterial>,
}

impl AvatarAssets {
    fn new(meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>) -> Self {
        Self {
            slash_arc: meshes.add(crate::combat_fx::slash_arc_mesh(
                0.55,
                1.45,
                crate::combat_fx::SWING_HALF_ANGLE,
                24,
            )),
            slash: materials.add(StandardMaterial {
                base_color: Color::srgba(0.35, 0.95, 1.0, 0.55),
                emissive: LinearRgba::rgb(0.0, 6.0, 9.0),
                alpha_mode: AlphaMode::Add,
                unlit: true,
                cull_mode: None,
                ..default()
            }),
        }
    }
}

fn load_hud_font(mut commands: Commands, assets: Res<AssetServer>) {
    commands.insert_resource(HudFont(assets.load(HUD_FONT_PATH)));
    commands.insert_resource(game_ui::HudTitleFont(assets.load(HUD_TITLE_FONT_PATH)));
}

fn setup_night_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let state = AtmosphereState::default();
    commands.insert_resource(ClearColor(state.fog_color));
    commands.insert_resource(AmbientLight {
        color: Color::srgb(0.05, 0.08, 0.16),
        brightness: 85.0,
    });

    // Cool moonlight with cascaded shadows sized for the street.
    commands.spawn((
        DirectionalLight {
            illuminance: 2_800.0,
            color: Color::srgb(0.45, 0.6, 0.95),
            shadows_enabled: true,
            ..default()
        },
        CascadeShadowConfigBuilder {
            num_cascades: 3,
            maximum_distance: 140.0,
            first_cascade_far_bound: 18.0,
            ..default()
        }
        .build(),
        Transform::from_xyz(30.0, 75.0, 20.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // The camera exists from the first frame (the HUD renders through it)
    // and follows the player once the session spawns them.
    commands
        .spawn((
            Camera3d::default(),
            Camera {
                hdr: true,
                ..default()
            },
            Tonemapping::AgX,
            DebandDither::Enabled,
            // Deferred shading with screen-space reflections: the wet asphalt
            // mirrors the neon. Deferred rendering requires MSAA off, so FXAA
            // smooths the edges instead.
            Msaa::Off,
            bevy::core_pipeline::fxaa::Fxaa {
                enabled: true,
                edge_threshold: bevy::core_pipeline::fxaa::Sensitivity::High,
                edge_threshold_min: bevy::core_pipeline::fxaa::Sensitivity::High,
            },
            ScreenSpaceReflections {
                perceptual_roughness_threshold: 0.35,
                thickness: 0.3,
                linear_steps: 24,
                ..default()
            },
            Bloom {
                intensity: 0.12,
                ..Bloom::NATURAL
            },
            DistanceFog {
                color: state.fog_color,
                falloff: FogFalloff::ExponentialSquared {
                    density: state.fog_density,
                },
                ..default()
            },
            CameraRigController::default(),
            game_audio::AudioListener,
            Transform::from_xyz(0.0, 3.0, 6.0).looking_at(Vec3::new(0.0, 1.5, -20.0), Vec3::Y),
        ))
        // Soft fill that travels with the camera so the operative always reads
        // against the night, whatever neon is nearby.
        .with_child((
            PointLight {
                color: Color::srgb(0.85, 0.92, 1.0),
                intensity: CAMERA_FILL_LUMENS,
                range: 10.0,
                radius: 0.5,
                shadows_enabled: false,
                ..default()
            },
            Transform::from_xyz(0.6, 0.8, 0.0),
        ));
    commands.insert_resource(state);

    // Rain: thin additive streaks recycled inside a box around the camera.
    let streak = meshes.add(Cuboid::new(0.012, 0.6, 0.012));
    let water = materials.add(StandardMaterial {
        base_color: Color::srgba(0.55, 0.68, 0.9, 0.22),
        alpha_mode: AlphaMode::Add,
        unlit: true,
        ..default()
    });
    let tilt = Quat::from_rotation_arc(Vec3::NEG_Y, RAIN_VELOCITY.normalize());
    for i in 0..RAIN_DROP_COUNT {
        commands.spawn((
            RainDrop,
            Mesh3d(streak.clone()),
            MeshMaterial3d(water.clone()),
            NotShadowCaster,
            Transform::from_translation(scatter(i, Vec3::ZERO)).with_rotation(tilt),
        ));
    }

    commands.insert_resource(AvatarAssets::new(&mut meshes, &mut materials));
}

/// Deterministic spread of drop `i` inside the rain box around `center`.
fn scatter(i: usize, center: Vec3) -> Vec3 {
    let h = |k: u32| {
        let mut x = (i as u32).wrapping_mul(0x9E37_79B1) ^ k.wrapping_mul(0x85EB_CA77);
        x ^= x >> 15;
        x = x.wrapping_mul(0x2C1B_3C6D);
        x ^= x >> 12;
        (x as f32 / u32::MAX as f32) * 2.0 - 1.0
    };
    center + Vec3::new(h(1), h(2), h(3)) * RAIN_BOX_HALF
}

/// Wraps `value` into `[center - half, center + half)`.
fn wrap(value: f32, center: f32, half: f32) -> f32 {
    center - half + (value - (center - half)).rem_euclid(2.0 * half)
}

fn update_rain(
    time: Res<Time>,
    atmosphere: Res<AtmosphereState>,
    cameras: Query<&GlobalTransform, (With<Camera3d>, With<CameraRigController>)>,
    mut drops: Query<(&mut Transform, &mut Visibility), With<RainDrop>>,
) {
    let Some(camera) = cameras.iter().next() else {
        return;
    };
    let center = camera.translation();
    let shown = (atmosphere.rain_intensity.clamp(0.0, 1.0) * RAIN_DROP_COUNT as f32) as usize;
    let step = RAIN_VELOCITY * time.delta_secs();
    for (i, (mut transform, mut visibility)) in drops.iter_mut().enumerate() {
        let wanted = if i < shown {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *visibility != wanted {
            *visibility = wanted;
        }
        if wanted == Visibility::Hidden {
            continue;
        }
        let p = transform.translation + step;
        transform.translation = Vec3::new(
            wrap(p.x, center.x, RAIN_BOX_HALF.x),
            wrap(p.y, center.y, RAIN_BOX_HALF.y),
            wrap(p.z, center.z, RAIN_BOX_HALF.z),
        );
    }
}

fn update_district_atmosphere(
    time: Res<Time>,
    mut clear: ResMut<ClearColor>,
    mut atmosphere: ResMut<AtmosphereState>,
    mut cameras: Query<(&GlobalTransform, &mut DistanceFog)>,
) {
    let Some((camera, mut fog)) = cameras.iter_mut().next() else {
        return;
    };
    let target = district_atmosphere(camera.translation());
    let t = 1.0 - (-ATMOSPHERE_EASE_RATE * time.delta_secs()).exp();
    let current = atmosphere.fog_color.to_linear();
    let goal = target.fog_color.to_linear();
    let next = AtmosphereState {
        fog_color: Color::linear_rgb(
            current.red + (goal.red - current.red) * t,
            current.green + (goal.green - current.green) * t,
            current.blue + (goal.blue - current.blue) * t,
        ),
        fog_density: atmosphere.fog_density + (target.fog_density - atmosphere.fog_density) * t,
        rain_intensity: atmosphere.rain_intensity
            + (target.rain_intensity - atmosphere.rain_intensity) * t,
    };
    *atmosphere = next;
    fog.color = next.fog_color;
    // The sky is the haze, so the horizon dissolves into the city glow.
    clear.0 = next.fog_color;
    fog.falloff = FogFalloff::ExponentialSquared {
        density: next.fog_density,
    };
}

fn starter_inventory() -> PlayerInventory {
    let mut inventory = PlayerInventory::new();
    for (item, quantity) in STARTER_ITEMS {
        let _ = inventory.add_item(item, quantity);
    }
    inventory
}

/// Gives the newly spawned local player controls, vitals, a starting kit
/// and an avatar, and points the camera at them.
fn attach_local_player_presentation(
    mut commands: Commands,
    assets: Res<AvatarAssets>,
    asset_server: Res<AssetServer>,
    appearance: Option<Res<CharacterAppearance>>,
    added: Query<Entity, Added<LocalPredictedPlayer>>,
    mut rigs: Query<&mut CameraRigController>,
) {
    for player in &added {
        for mut rig in &mut rigs {
            rig.target = Some(player);
        }
        commands.entity(player).insert((
            // Simulated positions are the feet.
            game_camera::OriginAtFeet,
            local_player_input_bundle(default_input_map()),
            Health::new(PLAYER_MAX_HEALTH),
            Shields::default(),
            FlowState::default(),
            CharacterSkills::default(),
            PlayerQuestLog::default(),
            starter_inventory(),
            MeleeCombatState::default(),
            // Hurtbox for enemy attacks; the simulated position is the feet.
            RigidBody::Kinematic,
            Collider::compound(vec![(
                Vec3::Y * PLAYER_HURTBOX_CENTER,
                Quat::IDENTITY,
                Collider::capsule(0.35, 1.1),
            )]),
        ));
        let look = appearance.as_deref().cloned().unwrap_or_default();
        spawn_operative_avatar(&mut commands, &assets, &asset_server, look, player);
    }
}

pub fn spawn_operative_avatar(
    commands: &mut Commands,
    a: &AvatarAssets,
    asset_server: &AssetServer,
    appearance: CharacterAppearance,
    source: Entity,
) -> Entity {
    let part = |mesh: &Handle<Mesh>, material: &Handle<StandardMaterial>, at: Transform| {
        (Mesh3d(mesh.clone()), MeshMaterial3d(material.clone()), at)
    };
    let root = commands
        .spawn((
            Name::new("Operative Avatar"),
            PredictedVisual { source },
            OperativeAvatar { source, yaw: 0.0 },
            Transform::default(),
            Visibility::default(),
        ))
        .with_children(|root| {
            // Gear pivots at the hips so it leans and crouches with the body.
            root.spawn((
                AvatarBody,
                Transform::from_xyz(0.0, AVATAR_HIP_HEIGHT, 0.0),
                Visibility::default(),
            ))
            .with_children(|body| {
                body.spawn((
                    crate::combat_fx::SlashTrail { source },
                    part(&a.slash_arc, &a.slash, Transform::default()),
                    Visibility::Hidden,
                    NotShadowCaster,
                ));
            });
        })
        .id();
    spawn_operative_body(commands, asset_server, root, appearance);
    root
}

/// Yaw facing a planar velocity (forward is -Z), if moving.
pub fn facing_yaw(velocity: Vec3) -> Option<f32> {
    let planar = Vec2::new(velocity.x, velocity.z);
    (planar.length_squared() > 0.25).then(|| (-planar.x).atan2(-planar.y))
}

/// Turns each avatar toward where it is going and leans it into speed.
/// Runs after the visual-offset pass, which positions the avatar.
fn orient_avatars(
    time: Res<Time>,
    sources: Query<(
        &KinematicVelocity,
        Option<&MeleeCombatState>,
        Option<&game_core::input::ViewAngles>,
    )>,
    mut avatars: Query<(&mut OperativeAvatar, &mut Transform)>,
) {
    let t = 1.0 - (-AVATAR_TURN_RATE * time.delta_secs()).exp();
    for (mut avatar, mut transform) in &mut avatars {
        let (velocity, melee, view) = sources
            .get(avatar.source)
            .map_or((Vec3::ZERO, None, None), |(v, m, view)| (v.0, m, view));
        // Fighting faces the camera so swings go where the player aims.
        let fighting = melee.is_some_and(|m| m.attack_phase != AttackPhase::Idle || m.is_parrying);
        let target = match view {
            Some(view) if fighting => Some(view.yaw),
            _ => facing_yaw(velocity),
        };
        if let Some(target) = target {
            let delta = (target - avatar.yaw + std::f32::consts::PI)
                .rem_euclid(std::f32::consts::TAU)
                - std::f32::consts::PI;
            avatar.yaw += delta * t;
        }
        let speed = Vec2::new(velocity.x, velocity.z).length();
        let lean = (speed / SPRINT_SPEED).min(1.0) * AVATAR_MAX_LEAN;
        transform.rotation = Quat::from_rotation_y(avatar.yaw) * Quat::from_rotation_x(-lean);
    }
}

/// Windowed-client presentation.
pub struct VisualsPlugin;

impl Plugin for VisualsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(DefaultOpaqueRendererMethod::deferred())
            .add_systems(PreStartup, load_hud_font)
            .add_systems(Startup, setup_night_scene)
            .add_systems(
                Update,
                (
                    attach_local_player_presentation,
                    orient_avatars.after(apply_visual_error_decay),
                    crate::combat_fx::face_local_player_to_camera,
                    crate::combat_fx::animate_katana,
                    crate::combat_fx::pose_avatar_bodies,
                    crate::gameplay_audio::emit_movement_sounds,
                    crate::gameplay_audio::emit_combat_sounds,
                    update_district_atmosphere,
                    update_rain,
                ),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::FRAC_PI_2;

    #[test]
    fn avatar_faces_its_velocity() {
        assert!(
            facing_yaw(Vec3::ZERO).is_none(),
            "standing still keeps the facing"
        );
        assert!(
            facing_yaw(Vec3::new(0.0, 0.0, -5.0)).unwrap().abs() < 1e-6,
            "north is yaw 0"
        );
        assert!(
            (facing_yaw(Vec3::new(-5.0, 0.0, 0.0)).unwrap() - FRAC_PI_2).abs() < 1e-6,
            "west is +pi/2 (left turn)"
        );
        let east = facing_yaw(Vec3::new(5.0, 0.0, 0.0)).unwrap();
        assert!((Quat::from_rotation_y(east) * Vec3::NEG_Z).distance(Vec3::X) < 1e-5);
    }

    #[test]
    fn rain_wraps_inside_its_box() {
        assert!((wrap(23.0, 0.0, 22.0) - (-21.0)).abs() < 1e-5);
        assert!((wrap(-22.5, 0.0, 22.0) - 21.5).abs() < 1e-5);
        assert!(
            (wrap(5.0, 100.0, 22.0) - 93.0).abs() < 1e-4,
            "lands inside [78, 122)"
        );
        for i in 0..RAIN_DROP_COUNT {
            let p = scatter(i, Vec3::new(10.0, 5.0, -3.0)) - Vec3::new(10.0, 5.0, -3.0);
            assert!(p.abs().cmple(RAIN_BOX_HALF).all());
        }
    }

    #[test]
    fn atmosphere_follows_districts() {
        let slums = district_atmosphere(Vec3::new(0.0, 5.0, 0.0));
        assert!((slums.rain_intensity - 0.85).abs() < 1e-6);
        let spire = district_atmosphere(Vec3::new(0.0, 600.0, 0.0));
        assert!(
            spire.fog_density < slums.fog_density,
            "thinner air up the Spire"
        );
        assert_eq!(
            district_atmosphere(Vec3::new(0.0, 150.0, 0.0)),
            AtmosphereState::default()
        );
    }

    #[test]
    fn starter_kit_fits_and_uses_real_items() {
        let inventory = starter_inventory();
        assert_eq!(inventory.slot_count(), STARTER_ITEMS.len());
        for (id, quantity) in STARTER_ITEMS {
            assert!(game_economy::find_item(id).is_some(), "{id}");
            assert_eq!(inventory.item_count(id), quantity);
        }
    }
}
