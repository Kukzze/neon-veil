//! The equipped weapon as a real 3D model in the operative's hand. Each
//! class is built from parts (grip, guard, blade, shaft, head...) that
//! ride the hand joints through gear mounts, so the weapon follows every
//! step and swing. Metal parts are dark steel; accents glow in the
//! weapon's element colour.

use crate::armory::Armory;
use crate::character::{BodyInstance, OperativeBody};
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

/// Where the fist closes on a gripped model: metres up the model from its
/// origin. `None` for worn weapons (the gauntlet), which ride the forearm.
pub fn model_hold(class: WeaponClass) -> Option<f32> {
    match class {
        WeaponClass::Gauntlet => None,
        WeaponClass::Katana => Some(0.075),
        WeaponClass::DualBlades => Some(0.025),
        WeaponClass::Baton => Some(0.03),
        WeaponClass::Whip => Some(0.03),
        WeaponClass::Claymore => Some(0.11),
        WeaponClass::Spear => Some(0.08),
        WeaponClass::Scythe => Some(0.0),
        WeaponClass::Axe => Some(0.0),
        WeaponClass::Hammer => Some(0.02),
    }
}

/// Finger chains of a hand, base to tip.
const FINGERS: [&str; 4] = ["index", "middle", "ring", "pinky"];

/// Curl of each finger joint (01, 02, 03) around a grip, and of the thumb.
const FINGER_CURL: [f32; 3] = [1.15, 1.35, 0.9];
const THUMB_CURL: [f32; 3] = [0.35, 0.55, 0.45];

fn joint_name(part: &str, segment: usize, hand: Hand) -> String {
    let side = match hand {
        Hand::Right => "r",
        Hand::Left => "l",
    };
    format!("{part}_0{segment}_{side}")
}

fn hand_name(hand: Hand) -> &'static str {
    match hand {
        Hand::Right => "hand_r",
        Hand::Left => "hand_l",
    }
}

/// +1 or -1: which way around the knuckle line (pinky to index) the
/// fingers close, read from the slight bend they have at rest.
pub fn closing_sign(body: &BodyInstance, hand: Hand) -> Option<f32> {
    let skeleton = &body.skeleton;
    let head = |part: &str, segment| {
        let i = *skeleton.index.get(&joint_name(part, segment, hand))?;
        body.rest_heads.get(i).copied()
    };
    let knuckles = head("index", 1)? - head("pinky", 1)?;
    let mut bend = Vec3::ZERO;
    for finger in FINGERS {
        let (a, b, c) = (head(finger, 1)?, head(finger, 2)?, head(finger, 3)?);
        bend += (b - a).cross(c - b);
    }
    let along = bend.dot(knuckles.normalize_or_zero());
    if along.abs() > 1e-9 {
        return Some(along.signum());
    }
    // Straight fingers: the thumb sits on the palm side.
    let middle = head("middle", 3)? - head("middle", 1)?;
    let palm = knuckles.cross(middle);
    let thumb = head("thumb", 3)? - head("middle", 1)?;
    Some(if palm.dot(thumb) >= 0.0 { 1.0 } else { -1.0 })
}

/// Local rotations that close a hand around a grip: (joint index, rotation
/// to use instead of the joint's rest rotation).
pub fn grip_curls(body: &BodyInstance, hand: Hand) -> Vec<(usize, Quat)> {
    let skeleton = &body.skeleton;
    let Some(sign) = closing_sign(body, hand) else {
        return Vec::new();
    };
    let index = |part: &str, segment| {
        skeleton
            .index
            .get(&joint_name(part, segment, hand))
            .copied()
    };
    let (Some(i1), Some(p1)) = (index("index", 1), index("pinky", 1)) else {
        return Vec::new();
    };
    let knuckles = (body.rest_heads[i1] - body.rest_heads[p1]).normalize_or_zero() * sign;
    let mut out = Vec::new();
    let mut curl = |joint: usize, axis_rig: Vec3, angle: f32| {
        let Some(parent) = skeleton.parents[joint] else {
            return;
        };
        let axis = (skeleton.rest_world_rotation[parent].inverse() * axis_rig).normalize_or_zero();
        if axis != Vec3::ZERO {
            let delta = Quat::from_axis_angle(axis, angle);
            out.push((joint, delta * skeleton.rest_local_rotation[joint]));
        }
    };
    for finger in FINGERS {
        for (k, angle) in FINGER_CURL.iter().enumerate() {
            if let Some(j) = index(finger, k + 1) {
                curl(j, knuckles, *angle);
            }
        }
    }
    // The thumb folds over the fingers, towards the middle knuckle.
    if let (Some(t1), Some(t3), Some(m2)) =
        (index("thumb", 1), index("thumb", 3), index("middle", 2))
    {
        let (base, tip, target) = (
            body.rest_heads[t1],
            body.rest_heads[t3],
            body.rest_heads[m2],
        );
        let axis = (tip - base).cross(target - base).normalize_or_zero();
        for (k, angle) in THUMB_CURL.iter().enumerate() {
            if let Some(j) = index("thumb", k + 1) {
                curl(j, axis, *angle);
            }
        }
    }
    out
}

/// How far the grip line leans from the knuckle line towards the fingers.
const GRIP_TILT: f32 = 0.28;

/// The roll about the forearm that turns a hand's knuckle line (pinky to
/// index) towards `target`, both taken across the forearm `axis`.
pub fn wrist_roll(axis: Vec3, knuckles: Vec3, target: Vec3) -> f32 {
    let across = |v: Vec3| (v - axis * axis.dot(v)).normalize_or_zero();
    let (k, t) = (across(knuckles), across(target));
    if k == Vec3::ZERO || t == Vec3::ZERO {
        return 0.0;
    }
    axis.dot(k.cross(t)).atan2(k.dot(t))
}

/// Where a gripped model sits, from the hand's posed joints: the grip runs
/// across the palm (index side out, leaning by [`GRIP_TILT`]), held inside
/// the closed fingers, with the edge (model -X) facing the knuckles.
pub fn grip_transform(
    wrist: Vec3,
    [index, middle, ring, pinky]: [Vec3; 4],
    sign: f32,
    hold: f32,
    scale: f32,
) -> Option<Transform> {
    let knuckles = (index + middle + ring + pinky) * 0.25;
    let line = (index - pinky).normalize_or_zero();
    let out = (knuckles - wrist).normalize_or_zero();
    let palm = (line.cross(out) * sign).normalize_or_zero();
    // A real grip crosses the palm diagonally, from the index knuckle to the
    // heel of the hand, so the weapon leans out past the fingers.
    let along = (line + out * GRIP_TILT.tan()).normalize_or_zero();
    let front = (out - along * along.dot(out)).normalize_or_zero();
    if line == Vec3::ZERO || front == Vec3::ZERO || palm == Vec3::ZERO {
        return None;
    }
    let x = -front;
    let rotation = Quat::from_mat3(&Mat3::from_cols(x, along, x.cross(along)));
    let grip = wrist.lerp(knuckles, 0.62) + palm * 0.024 * scale;
    Some(Transform {
        translation: grip - rotation * Vec3::Y * hold * scale,
        rotation,
        scale: Vec3::splat(scale),
    })
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
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct HeldWeaponModel {
    pub element: usize,
    /// The operative body holding it.
    pub body: Entity,
    pub hand: Hand,
    /// See [`model_hold`]; `None` rides a gear mount instead.
    pub hold: Option<f32>,
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
    /// The operative body carrying it.
    pub body: Entity,
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
    held: Query<(Entity, &HeldWeaponPart)>,
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
        .any(|(_, part)| part.base == item.base && part.body == body);
    if current {
        return;
    }
    for (entity, _) in &held {
        commands.entity(entity).despawn_recursive();
    }
    let base = item.base();
    if let Some(scene) = models.scene(base.class) {
        for &hand in model_hands(base.class) {
            let hold = model_hold(base.class);
            let mut model = commands.spawn((
                Name::new("Held Weapon"),
                HeldWeaponPart {
                    base: item.base,
                    body,
                },
                HeldWeaponModel {
                    element: base.element.index(),
                    body,
                    hand,
                    hold,
                },
                SceneRoot(scene.clone()),
                Transform::default(),
                Visibility::Hidden,
            ));
            if hold.is_none() {
                // Worn, not gripped: rides the forearm into the fist.
                let (from, to) = match hand {
                    Hand::Right => ("lowerarm_r", "hand_r"),
                    Hand::Left => ("lowerarm_l", "hand_l"),
                };
                model.insert(GearMount::new(body, from, to, 1.0).aligned());
            }
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
            HeldWeaponPart {
                base: item.base,
                body,
            },
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

/// Closes the fingers of every hand holding a model around its grip (after
/// the body pose, which leaves the fingers at rest).
pub fn curl_gripping_fingers(
    models: Query<&HeldWeaponModel>,
    bodies: Query<&BodyInstance>,
    mut joints: Query<&mut Transform, Without<HeldWeaponModel>>,
) {
    for model in &models {
        let Some(body) = bodies.iter().find(|b| b.root == model.body) else {
            continue;
        };
        if body.rest_heads.is_empty() {
            continue;
        }
        for (joint, rotation) in grip_curls(body, model.hand) {
            if let Ok(mut transform) = joints.get_mut(body.skeleton.joints[joint]) {
                transform.rotation = rotation;
            }
        }
    }
}

/// Puts each gripped model in its hand, from the posed hand joints (after
/// transforms propagate, so this frame's pose and finger curl are in).
pub fn place_gripped_weapons(
    bodies: Query<&BodyInstance>,
    children: Query<&Children>,
    mut joints: Query<(&mut Transform, &mut GlobalTransform), Without<HeldWeaponModel>>,
    mut models: Query<(
        &HeldWeaponModel,
        &mut Transform,
        &mut GlobalTransform,
        &mut Visibility,
    )>,
) {
    for (model, mut transform, mut global, mut visibility) in &mut models {
        let Some(hold) = model.hold else {
            continue;
        };
        let placed = (|| {
            let body = bodies.iter().find(|b| b.root == model.body)?;
            let entity = |name: &str| {
                let i = *body.skeleton.index.get(name)?;
                body.skeleton.joints.get(i).copied()
            };
            let (hand, forearm) = match model.hand {
                Hand::Right => (entity("hand_r")?, entity("lowerarm_r")?),
                Hand::Left => (entity("hand_l")?, entity("lowerarm_l")?),
            };
            let body_global = *joints.get(model.body).ok()?.1;
            roll_wrist(
                &mut joints,
                &children,
                hand,
                forearm,
                [
                    entity(&joint_name("index", 1, model.hand))?,
                    entity(&joint_name("pinky", 1, model.hand))?,
                ],
                &body_global,
            )?;
            let joint = |name: &str| Some(joints.get(entity(name)?).ok()?.1.translation());
            let knuckle = |finger| joint(&joint_name(finger, 1, model.hand));
            let scale = body_global.to_scale_rotation_translation().0.y;
            grip_transform(
                joint(hand_name(model.hand))?,
                [
                    knuckle("index")?,
                    knuckle("middle")?,
                    knuckle("ring")?,
                    knuckle("pinky")?,
                ],
                closing_sign(body, model.hand)?,
                hold,
                scale,
            )
        })();
        match placed {
            Some(t) => {
                *transform = t;
                *global = GlobalTransform::from(t);
                visibility.set_if_neq(Visibility::Inherited);
            }
            None => {
                visibility.set_if_neq(Visibility::Hidden);
            }
        }
    }
}

/// Rolls a gripping hand about its forearm so the knuckles face the body's
/// forward (or up, when the forearm itself points forward), then carries
/// the new pose down the hand's joints the same frame.
fn roll_wrist(
    joints: &mut Query<(&mut Transform, &mut GlobalTransform), Without<HeldWeaponModel>>,
    children: &Query<&Children>,
    hand: Entity,
    forearm: Entity,
    [index, pinky]: [Entity; 2],
    body: &GlobalTransform,
) -> Option<()> {
    let hand_global = *joints.get(hand).ok()?.1;
    let forearm_global = *joints.get(forearm).ok()?.1;
    let axis = (hand_global.translation() - forearm_global.translation()).normalize_or_zero();
    let knuckles =
        joints.get(index).ok()?.1.translation() - joints.get(pinky).ok()?.1.translation();
    let (_, facing, _) = body.to_scale_rotation_translation();
    // The operative body faces -Z in its own frame.
    let target = facing * Vec3::NEG_Z + Vec3::Y * 0.35;
    let roll = wrist_roll(axis, knuckles, target);
    if axis == Vec3::ZERO || roll.abs() < 1e-3 {
        return Some(());
    }
    let (_, hand_rotation, _) = hand_global.to_scale_rotation_translation();
    let (_, forearm_rotation, _) = forearm_global.to_scale_rotation_translation();
    let turned = Quat::from_axis_angle(axis, roll) * hand_rotation;
    {
        let (mut local, mut global) = joints.get_mut(hand).ok()?;
        local.rotation = forearm_rotation.inverse() * turned;
        *global = forearm_global.mul_transform(*local);
    }
    let mut stack = vec![hand];
    while let Some(entity) = stack.pop() {
        let parent = *joints.get(entity).ok()?.1;
        let Ok(kids) = children.get(entity) else {
            continue;
        };
        for &child in kids {
            if let Ok((local, mut global)) = joints.get_mut(child) {
                *global = parent.mul_transform(*local);
                stack.push(child);
            }
        }
    }
    Some(())
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
                (
                    place_gripped_weapons
                        .after(bevy::transform::TransformSystem::TransformPropagate),
                    propagate_weapon_models
                        .after(crate::enemy_visuals::follow_gear_mounts)
                        .after(place_gripped_weapons),
                ),
            )
            .add_systems(
                Update,
                curl_gripping_fingers.after(crate::character::pose::animate_bodies),
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
    fn the_grip_sits_in_the_fist_and_points_out_the_index_side() {
        // A right hand hanging down, knuckles forward (-Z), palm inward (+X).
        let wrist = Vec3::new(0.0, 0.8, 0.0);
        let knuckles = [
            Vec3::new(0.0, 0.71, -0.03),
            Vec3::new(0.0, 0.7, -0.01),
            Vec3::new(0.0, 0.7, 0.01),
            Vec3::new(0.0, 0.71, 0.03),
        ];
        let sign = if Vec3::NEG_Z.cross(Vec3::NEG_Y).dot(Vec3::X) > 0.0 {
            1.0
        } else {
            -1.0
        };
        let t = grip_transform(wrist, knuckles, sign, 0.0, 1.0).expect("a grip");
        let along = t.rotation * Vec3::Y;
        // Out of the index side (forward), leaning down past the fingers.
        assert!(along.z < -0.9 && along.y < -0.1, "{along:?}");
        // The grip point is in the palm, a little inside the hand.
        let palm = (t.translation - wrist).length();
        assert!(palm > 0.04 && palm < 0.1, "{palm}");
        assert!(t.translation.x > 0.01, "{:?}", t.translation);
        // The hold offset slides the model back along its own axis.
        let held = grip_transform(wrist, knuckles, sign, 0.1, 1.0).unwrap();
        assert!((held.translation - (t.translation - along * 0.1)).length() < 1e-5);
    }

    #[test]
    fn the_wrist_rolls_the_knuckles_towards_the_target() {
        let axis = Vec3::NEG_Y;
        let knuckles = Vec3::X;
        let roll = wrist_roll(axis, knuckles, Vec3::NEG_Z);
        let turned = Quat::from_axis_angle(axis, roll) * knuckles;
        assert!((turned - Vec3::NEG_Z).length() < 1e-5, "{turned:?}");
        assert_eq!(wrist_roll(axis, Vec3::NEG_Y, Vec3::NEG_Z), 0.0);
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
