//! What the equipped weapon and elemental statuses look like: the blade
//! and its slash trail take the weapon's element colour, the blade's
//! proportions follow its class (a hammer is a heavy block, a whip a long
//! thread), and enemies under a status glow in its colour.

use crate::armory::Armory;
use crate::client_network::LocalPredictedPlayer;
use crate::combat_fx::{DrawnKatana, SlashTrail};
use crate::visuals::{OperativeAvatar, SheathedBlade};
use bevy::pbr::NotShadowCaster;
use bevy::prelude::*;
use game_ai::Enemy;
use game_combat::status_effects::{ActiveStatusEffects, StatusEffectType};
use game_rpg::{Element, WeaponClass};

/// Blade proportions per class, as a scale of the katana blade (width,
/// length, thickness).
pub fn class_shape(class: WeaponClass) -> Vec3 {
    match class {
        WeaponClass::Katana => Vec3::new(1.0, 1.0, 1.0),
        WeaponClass::DualBlades => Vec3::new(1.4, 0.62, 1.2),
        WeaponClass::Baton => Vec3::new(3.0, 0.8, 3.0),
        WeaponClass::Gauntlet => Vec3::new(7.0, 0.32, 7.0),
        WeaponClass::Whip => Vec3::new(0.5, 1.7, 0.5),
        WeaponClass::Claymore => Vec3::new(3.2, 1.45, 1.6),
        WeaponClass::Spear => Vec3::new(0.9, 1.75, 0.9),
        WeaponClass::Scythe => Vec3::new(2.6, 1.35, 1.0),
        WeaponClass::Axe => Vec3::new(4.2, 0.95, 1.6),
        WeaponClass::Hammer => Vec3::new(5.5, 0.9, 5.5),
    }
}

/// Emissive colour of an element (linear), bright enough to bloom.
pub fn element_glow(element: Element) -> LinearRgba {
    match element {
        Element::Physical => LinearRgba::rgb(0.0, 4.0, 6.0),
        Element::Fire => LinearRgba::rgb(9.0, 2.2, 0.3),
        Element::Frost => LinearRgba::rgb(1.2, 4.5, 9.0),
        Element::Shock => LinearRgba::rgb(8.0, 7.0, 1.0),
        Element::Toxin => LinearRgba::rgb(2.5, 8.0, 0.8),
        Element::Void => LinearRgba::rgb(5.0, 1.2, 9.0),
    }
}

fn status_element(kind: StatusEffectType) -> Element {
    match kind {
        StatusEffectType::ThermalBurn => Element::Fire,
        StatusEffectType::CryoGlitch => Element::Frost,
        StatusEffectType::ShockEmp => Element::Shock,
        StatusEffectType::CorrosiveAcid => Element::Toxin,
    }
}

#[derive(Resource)]
pub struct WeaponLookAssets {
    blades: Vec<Handle<StandardMaterial>>,
    trails: Vec<Handle<StandardMaterial>>,
    auras: Vec<Handle<StandardMaterial>>,
    sphere: Handle<Mesh>,
}

impl FromWorld for WeaponLookAssets {
    fn from_world(world: &mut World) -> Self {
        let sphere = world
            .resource_mut::<Assets<Mesh>>()
            .add(Sphere::new(1.0).mesh().ico(2).expect("icosphere"));
        let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
        let mut blades = Vec::new();
        let mut trails = Vec::new();
        let mut auras = Vec::new();
        for element in Element::ALL {
            let glow = element_glow(element);
            blades.push(materials.add(StandardMaterial {
                base_color: Color::LinearRgba(glow * 0.04),
                perceptual_roughness: 0.3,
                emissive: glow,
                ..default()
            }));
            trails.push(materials.add(StandardMaterial {
                base_color: Color::LinearRgba((glow * 0.1).with_alpha(0.55)),
                emissive: glow * 1.5,
                alpha_mode: AlphaMode::Add,
                unlit: true,
                double_sided: true,
                cull_mode: None,
                ..default()
            }));
            auras.push(materials.add(StandardMaterial {
                base_color: Color::LinearRgba((glow * 0.05).with_alpha(0.25)),
                emissive: glow * 0.35,
                alpha_mode: AlphaMode::Add,
                unlit: true,
                ..default()
            }));
        }
        Self {
            blades,
            trails,
            auras,
            sphere,
        }
    }
}

/// Colours the local operative's blade, sheath glow and trail by element.
#[allow(clippy::type_complexity)]
pub fn color_weapon(
    armory: Res<Armory>,
    looks: Res<WeaponLookAssets>,
    players: Query<Entity, With<LocalPredictedPlayer>>,
    avatars: Query<(Entity, &OperativeAvatar)>,
    children: Query<&Children>,
    mut parts: Query<
        (
            &mut MeshMaterial3d<StandardMaterial>,
            Option<&DrawnKatana>,
            Option<&SlashTrail>,
            Option<&SheathedBlade>,
        ),
        Or<(With<DrawnKatana>, With<SlashTrail>, With<SheathedBlade>)>,
    >,
) {
    let Ok(player) = players.get_single() else {
        return;
    };
    let element = armory.data.equipped().base().element.index();
    for (avatar, a) in &avatars {
        if a.source != player {
            continue;
        }
        for entity in children.iter_descendants(avatar) {
            let Ok((mut material, drawn, trail, sheathed)) = parts.get_mut(entity) else {
                continue;
            };
            let wanted = if trail.is_some() {
                &looks.trails[element]
            } else if drawn.is_some() || sheathed.is_some() {
                &looks.blades[element]
            } else {
                continue;
            };
            if material.0 != *wanted {
                material.0 = wanted.clone();
            }
        }
    }
}

/// Scales the drawn and sheathed blade to the weapon class (runs after the
/// swing pose, which resets the transform).
pub fn shape_weapon(
    armory: Res<Armory>,
    players: Query<Entity, With<LocalPredictedPlayer>>,
    mut drawn: Query<(&DrawnKatana, &mut Transform), Without<SheathedBlade>>,
    mut sheathed: Query<&mut Transform, (With<SheathedBlade>, Without<DrawnKatana>)>,
) {
    let Ok(player) = players.get_single() else {
        return;
    };
    let shape = class_shape(armory.data.equipped().base().class);
    for (blade, mut transform) in &mut drawn {
        if blade.source == player {
            transform.scale = shape;
        }
    }
    // Only the local operative carries a sheathed blade.
    for mut transform in &mut sheathed {
        if transform.scale != shape {
            transform.scale = shape;
        }
    }
}

/// Glow on an enemy under a status effect.
#[derive(Component, Debug, Clone, Copy)]
pub struct StatusAura {
    pub owner: Entity,
}

/// Adds, recolours, pulses and removes status auras on enemies.
#[allow(clippy::type_complexity)]
pub fn status_auras(
    mut commands: Commands,
    time: Res<Time>,
    looks: Res<WeaponLookAssets>,
    afflicted: Query<(Entity, &ActiveStatusEffects), With<Enemy>>,
    mut auras: Query<(
        Entity,
        &StatusAura,
        &mut Transform,
        &mut MeshMaterial3d<StandardMaterial>,
    )>,
) {
    // The strongest active status picks the colour.
    let strongest = |effects: &ActiveStatusEffects| {
        effects
            .effects
            .iter()
            .max_by_key(|e| e.stacks)
            .map(|e| status_element(e.effect_type))
    };
    let mut covered = Vec::new();
    let pulse = 1.0 + 0.08 * (time.elapsed_secs() * 6.0).sin();
    for (entity, aura, mut transform, mut material) in &mut auras {
        match afflicted
            .get(aura.owner)
            .ok()
            .and_then(|(_, e)| strongest(e))
        {
            Some(element) => {
                covered.push(aura.owner);
                transform.scale = Vec3::new(0.75, 1.15, 0.75) * pulse;
                let wanted = &looks.auras[element.index()];
                if material.0 != *wanted {
                    material.0 = wanted.clone();
                }
            }
            None => commands.entity(entity).despawn_recursive(),
        }
    }
    for (enemy, effects) in &afflicted {
        if covered.contains(&enemy) {
            continue;
        }
        let Some(element) = strongest(effects) else {
            continue;
        };
        let aura = commands
            .spawn((
                Name::new("Status Aura"),
                StatusAura { owner: enemy },
                Mesh3d(looks.sphere.clone()),
                MeshMaterial3d(looks.auras[element.index()].clone()),
                Transform::from_scale(Vec3::new(0.75, 1.15, 0.75)),
                NotShadowCaster,
            ))
            .id();
        commands.entity(enemy).add_child(aura);
    }
}

pub struct WeaponLookPlugin;

impl Plugin for WeaponLookPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WeaponLookAssets>()
            .add_systems(Update, (color_weapon, status_auras));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_class_has_its_own_silhouette_and_elements_their_own_colour() {
        let shapes: Vec<Vec3> = WeaponClass::ALL.iter().map(|c| class_shape(*c)).collect();
        for (i, a) in shapes.iter().enumerate() {
            for b in &shapes[i + 1..] {
                assert_ne!(a, b);
            }
        }
        assert_eq!(
            class_shape(WeaponClass::Katana),
            Vec3::ONE,
            "the katana is the reference"
        );
        let glows: Vec<LinearRgba> = Element::ALL.iter().map(|e| element_glow(*e)).collect();
        for (i, a) in glows.iter().enumerate() {
            for b in &glows[i + 1..] {
                assert_ne!(a, b);
            }
        }
        assert_eq!(status_element(StatusEffectType::ThermalBurn), Element::Fire);
    }
}
