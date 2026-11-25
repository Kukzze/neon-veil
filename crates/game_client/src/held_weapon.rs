//! The equipped weapon as a real 3D model in the operative's hand. Each
//! class is built from parts (grip, guard, blade, shaft, head...) that
//! ride the hand joints through gear mounts, so the weapon follows every
//! step and swing. Metal parts are dark steel; accents glow in the
//! weapon's element colour.

use crate::armory::Armory;
use crate::character::OperativeBody;
use crate::client_network::LocalPredictedPlayer;
use crate::enemy_visuals::GearMount;
use crate::visuals::OperativeAvatar;
use crate::weapon_look::element_glow;
use bevy::pbr::NotShadowCaster;
use bevy::prelude::*;
use game_rpg::{Element, WeaponClass};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hand {
    Right,
    Left,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Finish {
    Steel,
    Grip,
    Glow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    Box,
    Rod,
}

/// One part of a weapon model, placed along the line of the forearm past
/// the fist (+Y of the part runs along the weapon).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Part {
    pub hand: Hand,
    pub shape: Shape,
    pub finish: Finish,
    /// Metres from the hand joint along the weapon to the part's centre.
    pub reach: f32,
    pub size: Vec3,
    /// Extra roll of the part about the weapon line.
    pub roll: f32,
}

const fn part(hand: Hand, shape: Shape, finish: Finish, reach: f32, size: Vec3) -> Part {
    Part {
        hand,
        shape,
        finish,
        reach,
        size,
        roll: 0.0,
    }
}

/// A one-handed blade: grip, guard, steel blade with a glowing core.
fn blade(hand: Hand, grip: f32, guard: Vec3, length: f32, width: f32) -> Vec<Part> {
    use Finish::*;
    use Shape::*;
    let blade_at = grip * 0.5 + guard.y + length * 0.5;
    vec![
        part(hand, Rod, Grip, 0.02, Vec3::new(0.034, grip, 0.034)),
        part(
            hand,
            Box,
            Steel,
            -grip * 0.5 + 0.03,
            Vec3::new(0.045, 0.035, 0.045),
        ),
        part(hand, Box, Steel, grip * 0.5 + guard.y * 0.5, guard),
        part(hand, Box, Steel, blade_at, Vec3::new(width, length, 0.009)),
        part(
            hand,
            Box,
            Glow,
            blade_at,
            Vec3::new(width * 0.35, length * 0.97, 0.012),
        ),
    ]
}

/// How far a class leans ahead of the wrist (longer weapons more, so they
/// never dig into the ground from a hanging arm).
pub fn carry_lean(class: WeaponClass) -> f32 {
    match class {
        WeaponClass::Gauntlet => 0.0,
        WeaponClass::Whip => 0.6,
        WeaponClass::Katana | WeaponClass::DualBlades | WeaponClass::Baton => 0.9,
        WeaponClass::Axe => 1.2,
        WeaponClass::Claymore | WeaponClass::Hammer => 1.6,
        WeaponClass::Spear | WeaponClass::Scythe => 2.4,
    }
}

/// Parts of a weapon class.
pub fn weapon_parts(class: WeaponClass) -> Vec<Part> {
    use Finish::*;
    use Hand::*;
    use Shape::*;
    match class {
        WeaponClass::Katana => blade(Right, 0.26, Vec3::new(0.1, 0.018, 0.07), 0.84, 0.028),
        WeaponClass::DualBlades => {
            let mut parts = blade(Right, 0.14, Vec3::new(0.07, 0.015, 0.05), 0.46, 0.034);
            parts.extend(blade(Left, 0.14, Vec3::new(0.07, 0.015, 0.05), 0.46, 0.034));
            parts
        }
        WeaponClass::Claymore => blade(Right, 0.4, Vec3::new(0.26, 0.03, 0.08), 1.25, 0.072),
        WeaponClass::Baton => vec![
            part(Right, Rod, Grip, 0.02, Vec3::new(0.04, 0.24, 0.04)),
            part(Right, Rod, Steel, 0.42, Vec3::new(0.036, 0.6, 0.036)),
            part(Right, Box, Glow, 0.5, Vec3::new(0.05, 0.035, 0.05)),
            part(Right, Box, Glow, 0.62, Vec3::new(0.05, 0.035, 0.05)),
            part(Right, Box, Glow, 0.73, Vec3::new(0.046, 0.06, 0.046)),
        ],
        WeaponClass::Gauntlet => {
            let mut parts = Vec::new();
            for hand in [Right, Left] {
                parts.push(part(hand, Box, Steel, 0.03, Vec3::new(0.13, 0.17, 0.14)));
                parts.push(part(hand, Box, Glow, 0.11, Vec3::new(0.135, 0.03, 0.145)));
                parts.push(part(hand, Box, Steel, -0.15, Vec3::new(0.115, 0.22, 0.115)));
            }
            parts
        }
        WeaponClass::Whip => vec![
            part(Right, Rod, Grip, 0.02, Vec3::new(0.036, 0.24, 0.036)),
            part(Right, Box, Steel, 0.16, Vec3::new(0.05, 0.04, 0.05)),
            part(Right, Rod, Glow, 1.0, Vec3::new(0.01, 1.66, 0.01)),
        ],
        WeaponClass::Spear => vec![
            part(Right, Rod, Grip, 0.35, Vec3::new(0.032, 1.95, 0.032)),
            part(Right, Box, Steel, -0.63, Vec3::new(0.045, 0.06, 0.045)),
            part(Right, Box, Steel, 1.44, Vec3::new(0.065, 0.3, 0.012)),
            part(Right, Box, Glow, 1.44, Vec3::new(0.022, 0.28, 0.015)),
        ],
        WeaponClass::Scythe => vec![
            part(Right, Rod, Grip, 0.45, Vec3::new(0.034, 1.55, 0.034)),
            Part {
                roll: 0.35,
                ..part(Right, Box, Steel, 1.17, Vec3::new(0.72, 0.06, 0.012))
            },
            Part {
                roll: 0.35,
                ..part(Right, Box, Glow, 1.15, Vec3::new(0.7, 0.018, 0.016))
            },
        ],
        WeaponClass::Axe => vec![
            part(Right, Rod, Grip, 0.25, Vec3::new(0.036, 0.8, 0.036)),
            part(Right, Box, Steel, 0.6, Vec3::new(0.27, 0.2, 0.035)),
            part(Right, Box, Glow, 0.6, Vec3::new(0.28, 0.04, 0.04)),
        ],
        WeaponClass::Hammer => vec![
            part(Right, Rod, Grip, 0.3, Vec3::new(0.042, 0.95, 0.042)),
            part(Right, Box, Steel, 0.78, Vec3::new(0.34, 0.2, 0.2)),
            part(Right, Box, Glow, 0.78, Vec3::new(0.345, 0.045, 0.205)),
        ],
    }
}

/// File stem of each class's model in `assets/weapons` (built by
/// `tools/blender/build_weapons.py`).
pub fn model_file(class: WeaponClass) -> &'static str {
    match class {
        WeaponClass::Katana => "katana",
        WeaponClass::DualBlades => "dual_blades",
        WeaponClass::Baton => "baton",
        WeaponClass::Gauntlet => "gauntlet",
        WeaponClass::Whip => "whip",
        WeaponClass::Claymore => "claymore",
        WeaponClass::Spear => "spear",
        WeaponClass::Scythe => "scythe",
        WeaponClass::Axe => "axe",
        WeaponClass::Hammer => "hammer",
    }
}

/// The hands that carry a class's model (paired weapons take both).
pub fn model_hands(class: WeaponClass) -> &'static [Hand] {
    match class {
        WeaponClass::DualBlades | WeaponClass::Gauntlet => &[Hand::Right, Hand::Left],
        _ => &[Hand::Right],
    }
}

/// Metres past the wrist joint to the model's grip point.
fn model_reach(class: WeaponClass) -> f32 {
    if class == WeaponClass::Gauntlet {
        0.0
    } else {
        0.02
    }
}

/// The weapon models, when the asset files are present (headless runs and
/// stripped builds fall back to the part models).
#[derive(Resource)]
pub struct WeaponModels {
    scenes: Option<Vec<Handle<Scene>>>,
}

impl WeaponModels {
    pub fn scene(&self, class: WeaponClass) -> Option<Handle<Scene>> {
        let index = WeaponClass::ALL.iter().position(|c| *c == class)?;
        self.scenes.as_ref()?.get(index).cloned()
    }
}

impl FromWorld for WeaponModels {
    fn from_world(world: &mut World) -> Self {
        let root = crate::app::asset_root();
        let present = WeaponClass::ALL.iter().all(|c| {
            root.join(format!("weapons/{}.glb", model_file(*c)))
                .exists()
        });
        let Some(assets) = world.get_resource::<AssetServer>().filter(|_| present) else {
            return Self { scenes: None };
        };
        let scenes = WeaponClass::ALL
            .iter()
            .map(|c| {
                assets.load(
                    GltfAssetLabel::Scene(0).from_asset(format!("weapons/{}.glb", model_file(*c))),
                )
            })
            .collect();
        Self {
            scenes: Some(scenes),
        }
    }
}

/// A held weapon model; its glow takes the element's colour.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeldWeaponModel {
    pub element: usize,
}

/// Marks a model mesh whose material has been checked (and the glow swapped).
#[derive(Component)]
pub struct WeaponMaterialChecked;

/// Repeats of the energy pulse along a weapon, and how fast it travels
/// (weapon lengths per second).
const FLOW_PULSES: f32 = 2.0;
const FLOW_SPEED: f32 = 0.7;

/// Brightness of the flowing energy along the glow (0 at the hand end, 1 at
/// the tip): a steady core with soft pulses riding on it.
pub fn flow_brightness(v: f32) -> f32 {
    let phase = (v * FLOW_PULSES).rem_euclid(1.0) - 0.5;
    0.35 + 0.65 * (-(phase / 0.13).powi(2)).exp()
}

/// A tiling strip of [`flow_brightness`] for the glow's emissive texture.
fn flow_texture() -> Image {
    use bevy::image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor};
    use bevy::render::render_asset::RenderAssetUsages;
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
    const HEIGHT: u32 = 128;
    let mut data = Vec::with_capacity((HEIGHT * 4 * 4) as usize);
    for row in 0..HEIGHT {
        let v = (row as f32 + 0.5) / HEIGHT as f32 / FLOW_PULSES;
        let level = (flow_brightness(v) * 255.0).round() as u8;
        for _ in 0..4 {
            data.extend_from_slice(&[level, level, level, 255]);
        }
    }
    let mut image = Image::new(
        Extent3d {
            width: 4,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        ..ImageSamplerDescriptor::linear()
    });
    image
}

#[derive(Resource)]
pub struct HeldWeaponAssets {
    cube: Handle<Mesh>,
    rod: Handle<Mesh>,
    steel: Handle<StandardMaterial>,
    grip: Handle<StandardMaterial>,
    glows: Vec<Handle<StandardMaterial>>,
    /// Per element: the model glow, with energy flowing along it.
    flows: Vec<Handle<StandardMaterial>>,
}

impl FromWorld for HeldWeaponAssets {
    fn from_world(world: &mut World) -> Self {
        let (cube, rod) = {
            let mut meshes = world.resource_mut::<Assets<Mesh>>();
            (
                meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
                meshes.add(Cylinder::new(0.5, 1.0)),
            )
        };
        let flow = world
            .get_resource_mut::<Assets<Image>>()
            .map(|mut images| images.add(flow_texture()));
        let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
        let flows = Element::ALL
            .iter()
            .map(|e| {
                let glow = element_glow(*e);
                materials.add(StandardMaterial {
                    base_color: Color::LinearRgba(glow * 0.05),
                    emissive: glow * 2.4,
                    emissive_texture: flow.clone(),
                    ..default()
                })
            })
            .collect();
        let steel = materials.add(StandardMaterial {
            base_color: Color::srgb(0.32, 0.34, 0.37),
            perceptual_roughness: 0.22,
            metallic: 0.95,
            ..default()
        });
        let grip = materials.add(StandardMaterial {
            base_color: Color::srgb(0.03, 0.03, 0.035),
            perceptual_roughness: 0.7,
            ..default()
        });
        let glows = Element::ALL
            .iter()
            .map(|e| {
                let glow = element_glow(*e);
                materials.add(StandardMaterial {
                    base_color: Color::LinearRgba(glow * 0.05),
                    emissive: glow,
                    ..default()
                })
            })
            .collect();
        Self {
            cube,
            rod,
            steel,
            grip,
            glows,
            flows,
        }
    }
}

/// A part of the weapon in the operative's hand, and which weapon base it
/// belongs to.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeldWeaponPart {
    pub base: u16,
}

/// Puts the equipped weapon in the local operative's hands and swaps it
/// when another is equipped.
#[allow(clippy::too_many_arguments)]
pub fn hold_weapon(
    mut commands: Commands,
    armory: Res<Armory>,
    assets: Res<HeldWeaponAssets>,
    models: Res<WeaponModels>,
    players: Query<Entity, With<LocalPredictedPlayer>>,
    avatars: Query<&OperativeAvatar>,
    bodies: Query<(Entity, &Parent), With<OperativeBody>>,
    held: Query<(Entity, &HeldWeaponPart, &GearMount)>,
) {
    let Ok(player) = players.get_single() else {
        return;
    };
    let Some(body) = bodies
        .iter()
        .find(|(_, parent)| avatars.get(parent.get()).is_ok_and(|a| a.source == player))
        .map(|(e, _)| e)
    else {
        return;
    };
    let item = armory.data.equipped();
    let current = held
        .iter()
        .any(|(_, part, mount)| part.base == item.base && mount.body == body);
    if current {
        return;
    }
    for (entity, _, _) in &held {
        commands.entity(entity).despawn_recursive();
    }
    let base = item.base();
    if let Some(scene) = models.scene(base.class) {
        for hand in model_hands(base.class) {
            let (from, to) = match hand {
                Hand::Right => ("lowerarm_r", "hand_r"),
                Hand::Left => ("lowerarm_l", "hand_l"),
            };
            commands.spawn((
                Name::new("Held Weapon"),
                HeldWeaponPart { base: item.base },
                HeldWeaponModel {
                    element: base.element.index(),
                },
                SceneRoot(scene.clone()),
                Transform::default(),
                Visibility::Hidden,
                GearMount::new(body, from, to, 1.0)
                    .aligned()
                    .lean(carry_lean(base.class))
                    .reach(model_reach(base.class)),
            ));
        }
        return;
    }
    let glow = assets.glows[base.element.index()].clone();
    for p in weapon_parts(base.class) {
        let (from, to) = match p.hand {
            Hand::Right => ("lowerarm_r", "hand_r"),
            Hand::Left => ("lowerarm_l", "hand_l"),
        };
        let mount = GearMount::new(body, from, to, 1.0)
            .aligned()
            .lean(carry_lean(base.class))
            .reach(p.reach)
            .shape(p.size)
            .turned(Quat::from_rotation_z(p.roll));
        let mesh = match p.shape {
            Shape::Box => assets.cube.clone(),
            Shape::Rod => assets.rod.clone(),
        };
        let material = match p.finish {
            Finish::Steel => assets.steel.clone(),
            Finish::Grip => assets.grip.clone(),
            Finish::Glow => glow.clone(),
        };
        let mut part = commands.spawn((
            Name::new("Held Weapon"),
            HeldWeaponPart { base: item.base },
            Mesh3d(mesh),
            MeshMaterial3d(material),
            Transform::default(),
            Visibility::Hidden,
            mount,
        ));
        if p.finish == Finish::Glow {
            part.insert(NotShadowCaster);
        }
    }
}

/// Swaps each model's emissive material for the element's flowing glow
/// (other materials keep the model's own steel, brass and wrap).
pub fn tint_weapon_models(
    mut commands: Commands,
    assets: Res<HeldWeaponAssets>,
    materials: Res<Assets<StandardMaterial>>,
    models: Query<(Entity, &HeldWeaponModel)>,
    children: Query<&Children>,
    mut meshes: Query<&mut MeshMaterial3d<StandardMaterial>, Without<WeaponMaterialChecked>>,
) {
    for (root, model) in &models {
        for entity in children.iter_descendants(root) {
            let Ok(mut material) = meshes.get_mut(entity) else {
                continue;
            };
            let Some(source) = materials.get(&material.0) else {
                continue;
            };
            let e = source.emissive;
            let mut checked = commands.entity(entity);
            checked.insert(WeaponMaterialChecked);
            if e.red + e.green + e.blue > 0.01 {
                material.0 = assets.flows[model.element].clone();
                checked.insert(NotShadowCaster);
            }
        }
    }
}

/// Sends the energy pulses up every glow, hand to tip.
pub fn flow_weapon_glow(
    time: Res<Time>,
    assets: Res<HeldWeaponAssets>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let shift = (time.elapsed_secs() * FLOW_SPEED).rem_euclid(1.0);
    for handle in &assets.flows {
        if let Some(material) = materials.get_mut(handle) {
            material.uv_transform = bevy::math::Affine2::from_translation(Vec2::new(0.0, -shift));
        }
    }
}

/// Mounts are placed after transforms propagate, so carry their new pose
/// down to the model's meshes the same frame (else the weapon trails the
/// hand by a frame mid-swing).
pub fn propagate_weapon_models(
    roots: Query<(Entity, &GlobalTransform), With<HeldWeaponModel>>,
    children: Query<&Children>,
    mut nodes: Query<(&Transform, &mut GlobalTransform), Without<HeldWeaponModel>>,
) {
    for (root, global) in &roots {
        let mut stack = vec![(root, *global)];
        while let Some((entity, parent)) = stack.pop() {
            let Ok(kids) = children.get(entity) else {
                continue;
            };
            for &child in kids {
                let Ok((local, mut child_global)) = nodes.get_mut(child) else {
                    continue;
                };
                let next = parent.mul_transform(*local);
                *child_global = next;
                stack.push((child, next));
            }
        }
    }
}

/// In first person the operative's own body is hidden (the camera sits
/// in its head); the weapon in hand stays visible.
pub fn hide_body_in_first_person(
    rigs: Query<&game_camera::CameraRigController>,
    players: Query<Entity, With<LocalPredictedPlayer>>,
    avatars: Query<&OperativeAvatar>,
    mut bodies: Query<(&Parent, &mut Visibility), With<OperativeBody>>,
) {
    let first_person = rigs
        .iter()
        .any(|r| r.current_mode == game_camera::CameraPerspectiveMode::FirstPerson);
    let Ok(player) = players.get_single() else {
        return;
    };
    for (parent, mut visibility) in &mut bodies {
        if !avatars.get(parent.get()).is_ok_and(|a| a.source == player) {
            continue;
        }
        let wanted = if first_person {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
        if *visibility != wanted {
            *visibility = wanted;
        }
    }
}

pub struct HeldWeaponPlugin;

impl Plugin for HeldWeaponPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HeldWeaponAssets>()
            .init_resource::<WeaponModels>()
            .add_systems(
                Update,
                (
                    hold_weapon,
                    tint_weapon_models,
                    flow_weapon_glow,
                    hide_body_in_first_person,
                ),
            )
            .add_systems(
                PostUpdate,
                propagate_weapon_models.after(crate::enemy_visuals::follow_gear_mounts),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_class_has_a_model_with_a_grip_or_fist_and_a_glow() {
        for class in WeaponClass::ALL {
            let parts = weapon_parts(class);
            assert!(parts.len() >= 3, "{class:?}");
            assert!(parts.iter().any(|p| p.finish == Finish::Glow), "{class:?}");
            assert!(parts.iter().all(|p| p.size.min_element() > 0.0));
            // Held at the fist: some part spans the hand.
            assert!(
                parts.iter().any(|p| p.reach.abs() < p.size.y * 0.5 + 0.05),
                "{class:?}"
            );
        }
        let two_handed = |c| weapon_parts(c).iter().any(|p| p.hand == Hand::Left);
        assert!(two_handed(WeaponClass::DualBlades) && two_handed(WeaponClass::Gauntlet));
        assert!(!two_handed(WeaponClass::Katana));
        let reach = |c| {
            weapon_parts(c)
                .iter()
                .map(|p| p.reach + p.size.y * 0.5)
                .fold(0.0, f32::max)
        };
        assert!(reach(WeaponClass::Spear) > reach(WeaponClass::Katana));
        assert!(reach(WeaponClass::Katana) > reach(WeaponClass::DualBlades));
    }

    #[test]
    fn every_class_has_a_built_model_file() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/weapons");
        let mut stems: Vec<_> = WeaponClass::ALL.iter().map(|c| model_file(*c)).collect();
        for stem in &stems {
            assert!(dir.join(format!("{stem}.glb")).exists(), "{stem}.glb");
        }
        stems.dedup();
        assert_eq!(stems.len(), WeaponClass::ALL.len());
        assert_eq!(model_hands(WeaponClass::DualBlades).len(), 2);
        assert_eq!(model_hands(WeaponClass::Katana), &[Hand::Right]);
    }

    #[test]
    fn energy_pulses_ride_a_steady_core() {
        let samples: Vec<f32> = (0..200)
            .map(|i| flow_brightness(i as f32 / 200.0))
            .collect();
        let lo = samples.iter().copied().fold(f32::MAX, f32::min);
        let hi = samples.iter().copied().fold(f32::MIN, f32::max);
        assert!(lo > 0.3 && lo < 0.4, "core {lo}");
        assert!(hi > 0.95 && hi <= 1.0, "pulse {hi}");
        // The strip tiles: brightness repeats every 1 / FLOW_PULSES.
        assert!((flow_brightness(0.1) - flow_brightness(0.1 + 1.0 / FLOW_PULSES)).abs() < 1e-4);
    }
}
