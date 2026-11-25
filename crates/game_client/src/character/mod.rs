//! The customizable operative: a MakeHuman body shaped by 80+ sliders,
//! coloured per region and refitted to a game skeleton.

pub mod accessories;
pub mod appearance;
pub mod body;
pub mod library;
pub mod pose;
pub mod proxy;
pub mod rigged;
pub mod skin_texture;

pub use appearance::{
    load_appearance, save_appearance, AppearancePath, CharacterAppearance, HairStyle, Outfit,
    PlayerCard, Preset,
};
pub use body::{BodyInstance, JointRest, OperativeBody, ATTRIBUTE_VID};
pub use library::{MorphLibrary, SliderInfo, BODY_SCENE};

use bevy::prelude::*;

/// Skin detail maps (one leaning masculine, one feminine) and the template
/// each body's material is cloned from; bodies add their composed albedo.
#[derive(Resource, Debug, Clone)]
pub struct BodyMaterial {
    pub detail_male: Handle<Image>,
    pub detail_female: Handle<Image>,
    pub template: Handle<StandardMaterial>,
}

pub const SKIN_DETAIL_MALE: &str = "characters/skin_detail_male.png";
pub const SKIN_DETAIL_FEMALE: &str = "characters/skin_detail_female.png";
pub const SKIN_NORMAL: &str = "characters/skin_normal.png";

impl FromWorld for BodyMaterial {
    fn from_world(world: &mut World) -> Self {
        let assets = world.resource::<AssetServer>().clone();
        let linear = |path: &'static str| {
            assets.load_with_settings(path, |s: &mut bevy::image::ImageLoaderSettings| {
                s.is_srgb = false;
            })
        };
        let detail_male = linear(SKIN_DETAIL_MALE);
        let detail_female = linear(SKIN_DETAIL_FEMALE);
        let normal = linear(SKIN_NORMAL);
        let template = world
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial {
                base_color: Color::WHITE,
                normal_map_texture: Some(normal),
                perceptual_roughness: 0.5,
                reflectance: 0.4,
                // A soft dewy sheen over the skin.
                clearcoat: 0.18,
                clearcoat_perceptual_roughness: 0.32,
                // Clearcoat needs the forward path (the deferred G-buffer drops it).
                opaque_render_method: bevy::pbr::OpaqueRendererMethod::Forward,
                ..default()
            });
        Self {
            detail_male,
            detail_female,
            template,
        }
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

/// Loads appearance data and shapes operative bodies.
pub struct CharacterPlugin;

impl Plugin for CharacterPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BodyMaterial>()
            .init_resource::<AppearancePath>()
            .add_systems(PreStartup, load_library)
            .add_systems(Startup, accessories::setup_proxy_library)
            .add_systems(
                Update,
                (
                    body::prepare_bodies,
                    rigged::prewarm_armor,
                    rigged::prepare_rigged,
                    body::apply_appearances,
                    body::compose_albedos,
                    accessories::add_hidden_skin,
                    accessories::sync_worn_proxies,
                    accessories::refit_worn_proxies,
                    accessories::hide_covered_skin,
                    pose::animate_bodies,
                )
                    .chain(),
            )
            .add_systems(Update, rigged::end_armor_prewarm);
    }
}

/// Spawns an operative body scene under `parent`, facing -Z.
pub fn spawn_operative_body(
    commands: &mut Commands,
    assets: &AssetServer,
    parent: Entity,
    appearance: CharacterAppearance,
) -> Entity {
    // The MakeHuman body faces +Z; the game's forward is -Z.
    let facing = Transform::from_rotation(Quat::from_rotation_y(std::f32::consts::PI));
    spawn_operative_body_at(commands, assets, parent, appearance, facing)
}

/// Like [`spawn_operative_body`], with an explicit local transform (for
/// parents whose origin is not at the feet).
pub fn spawn_operative_body_at(
    commands: &mut Commands,
    assets: &AssetServer,
    parent: Entity,
    appearance: CharacterAppearance,
    transform: Transform,
) -> Entity {
    let body = commands
        .spawn((
            Name::new("Operative Body"),
            OperativeBody,
            appearance,
            SceneRoot(assets.load(BODY_SCENE)),
            transform,
        ))
        .id();
    commands.entity(parent).add_child(body);
    body
}
