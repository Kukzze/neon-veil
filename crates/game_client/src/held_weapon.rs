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

#[derive(Resource)]
pub struct HeldWeaponAssets {
    cube: Handle<Mesh>,
    rod: Handle<Mesh>,
    steel: Handle<StandardMaterial>,
    grip: Handle<StandardMaterial>,
    glows: Vec<Handle<StandardMaterial>>,
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
        let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
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
pub fn hold_weapon(
    mut commands: Commands,
    armory: Res<Armory>,
    assets: Res<HeldWeaponAssets>,
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
            .add_systems(Update, (hold_weapon, hide_body_in_first_person));
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
}
