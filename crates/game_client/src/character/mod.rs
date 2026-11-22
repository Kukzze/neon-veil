//! The customizable operative: a MakeHuman body shaped by 80+ sliders,
//! coloured per region and refitted to a game skeleton.

pub mod appearance;
pub mod body;
pub mod library;
pub mod pose;

pub use appearance::{
    load_appearance, save_appearance, AppearancePath, CharacterAppearance, HairStyle, PlayerCard,
    Preset,
};
pub use body::{BodyInstance, JointRest, OperativeBody, ATTRIBUTE_VID};
pub use library::{MorphLibrary, SliderInfo, BODY_SCENE};

use bevy::prelude::*;

/// Body material: vertex colours carry skin, suit and eye tones.
#[derive(Resource, Debug, Clone)]
pub struct BodyMaterial(pub Handle<StandardMaterial>);

impl FromWorld for BodyMaterial {
    fn from_world(world: &mut World) -> Self {
        let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
        Self(materials.add(StandardMaterial {
            base_color: Color::WHITE,
            perceptual_roughness: 0.58,
            reflectance: 0.35,
            ..default()
        }))
    }
}

fn load_library(mut commands: Commands) {
    match MorphLibrary::load(&crate::app::asset_root()) {
        Ok(library) => {
            info!(
                "operative library: {} sliders, {} morphs",
                library.sliders.len(),
                library.morphs.len()
            );
            commands.insert_resource(library);
        }
        Err(error) => warn!("operative library unavailable: {error}"),
    }
}

/// Swaps the glTF default material for the body material.
fn use_body_material(
    material: Res<BodyMaterial>,
    mut bodies: Query<&mut MeshMaterial3d<StandardMaterial>, Added<BodyInstance>>,
) {
    for mut handle in &mut bodies {
        handle.0 = material.0.clone();
    }
}

/// Loads appearance data and shapes operative bodies.
pub struct CharacterPlugin;

impl Plugin for CharacterPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BodyMaterial>()
            .init_resource::<AppearancePath>()
            .add_systems(PreStartup, load_library)
            .add_systems(
                Update,
                (
                    body::prepare_bodies,
                    use_body_material,
                    body::apply_appearances,
                    pose::animate_bodies,
                )
                    .chain(),
            );
    }
}

/// Spawns an operative body scene under `parent`, facing -Z.
pub fn spawn_operative_body(
    commands: &mut Commands,
    assets: &AssetServer,
    parent: Entity,
    appearance: CharacterAppearance,
) -> Entity {
    let body = commands
        .spawn((
            Name::new("Operative Body"),
            OperativeBody,
            appearance,
            SceneRoot(assets.load(BODY_SCENE)),
            // The MakeHuman body faces +Z; the game's forward is -Z.
            Transform::from_rotation(Quat::from_rotation_y(std::f32::consts::PI)),
        ))
        .id();
    commands.entity(parent).add_child(body);
    body
}
