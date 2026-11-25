//! Imported rigged characters (the cyber armour) animated by the same
//! procedural pose as the operative. Any skinned glTF whose skeleton uses
//! the game's bone names (pelvis, spine_01..03, thigh_l, upperarm_r, ...)
//! works: its facing and height are read from the rest pose, so it is
//! turned to face the game's forward and scaled to a set height.

use super::body::{BodyInstance, JointRest, Skeleton};
use bevy::prelude::*;
use std::collections::HashMap;

/// The cyber armour, shipped only in local builds (it is a store asset
/// that may not be redistributed as a file).
pub const ARMOR_SCENE: &str = "licensed/cyber_armor.glb";

/// A rigged model to pose: `height` is its standing height in metres and
/// `at` places its feet and facing in the parent (like an operative body).
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct RiggedModel {
    pub height: f32,
    pub at: Transform,
    /// Glow of the bright panel seams in the albedo (emissive tint).
    pub trim: LinearRgba,
}

/// Whether the armour asset is present in this build.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ArmorAvailable(pub bool);

impl ArmorAvailable {
    pub fn detect(asset_root: &std::path::Path) -> Self {
        Self(asset_root.join(ARMOR_SCENE).exists())
    }
}

/// Spawns the armour as a child of `parent` (feet on the parent's origin).
pub fn spawn_armor(
    commands: &mut Commands,
    assets: &AssetServer,
    parent: Entity,
    height: f32,
    at: Transform,
    trim: LinearRgba,
) -> Entity {
    let armor = commands
        .spawn((
            Name::new("Cyber Armour"),
            RiggedModel { height, at, trim },
            SceneRoot(assets.load(GltfAssetLabel::Scene(0).from_asset(ARMOR_SCENE))),
            // Turned and scaled once its rest pose is known.
            Transform::default(),
            Visibility::Hidden,
        ))
        .id();
    commands.entity(parent).add_child(armor);
    armor
}

/// The rotation taking a rig whose rest pose has `left` and `up` into the
/// game's rig space (faces +Z, left is +X).
pub fn rig_frame(left: Vec3, up: Vec3) -> Quat {
    let up = up.normalize_or(Vec3::Y);
    let forward = left.cross(up).normalize_or(Vec3::Z);
    let left = up.cross(forward).normalize_or(Vec3::X);
    Quat::from_mat3(&Mat3::from_cols(left, up, forward)).inverse()
}

/// Builds the skeleton of each newly loaded rigged model, then turns and
/// scales it and shows it.
#[allow(clippy::type_complexity)]
#[allow(clippy::too_many_arguments)]
pub fn prepare_rigged(
    mut commands: Commands,
    mut roots: Query<
        (Entity, &RiggedModel, &mut Transform, &mut Visibility),
        Without<BodyInstance>,
    >,
    children: Query<&Children>,
    skins: Query<&bevy::render::mesh::skinning::SkinnedMesh>,
    names: Query<&Name>,
    parents: Query<&Parent>,
    transforms: Query<&Transform, Without<RiggedModel>>,
    skin_materials: Query<&MeshMaterial3d<StandardMaterial>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (root, rigged, mut root_transform, mut visibility) in &mut roots {
        let Some((skin_entity, skin)) = children
            .iter_descendants(root)
            .find_map(|e| skins.get(e).ok().map(|s| (e, s)))
        else {
            continue;
        };
        let joints = skin.joints.clone();
        if joints.iter().any(|j| transforms.get(*j).is_err()) {
            continue;
        }
        let names: Vec<String> = joints
            .iter()
            .map(|j| {
                names
                    .get(*j)
                    .map(|n| n.as_str().to_owned())
                    .unwrap_or_default()
            })
            .collect();
        let index: HashMap<String, usize> = names
            .iter()
            .enumerate()
            .map(|(i, n)| (n.clone(), i))
            .collect();
        let joint_parents: Vec<Option<usize>> = joints
            .iter()
            .map(|j| {
                parents
                    .get(*j)
                    .ok()
                    .and_then(|p| joints.iter().position(|x| *x == p.get()))
            })
            .collect();
        // Everything between the model root and the skeleton (armature
        // nodes): the frame the root joints hang from.
        let Some(first_root) = joint_parents.iter().position(Option::is_none) else {
            continue;
        };
        let mut pre = Transform::IDENTITY;
        let mut at = parents.get(joints[first_root]).ok().map(|p| p.get());
        while let Some(node) = at {
            if node == root {
                break;
            }
            pre = transforms.get(node).copied().unwrap_or_default() * pre;
            at = parents.get(node).ok().map(|p| p.get());
        }
        // Rest transforms in the model's own space.
        let local: Vec<Transform> = joints
            .iter()
            .map(|j| transforms.get(*j).copied().unwrap_or_default())
            .collect();
        let mut world: Vec<Option<Transform>> = vec![None; joints.len()];
        fn resolve(
            i: usize,
            parents: &[Option<usize>],
            local: &[Transform],
            pre: Transform,
            memo: &mut [Option<Transform>],
        ) -> Transform {
            if let Some(t) = memo[i] {
                return t;
            }
            let parent = match parents[i] {
                Some(p) => resolve(p, parents, local, pre, memo),
                None => pre,
            };
            let t = parent * local[i];
            memo[i] = Some(t);
            t
        }
        for i in 0..joints.len() {
            resolve(i, &joint_parents, &local, pre, &mut world);
        }
        let world: Vec<Transform> = world.into_iter().map(Option::unwrap_or_default).collect();
        let head = |name: &str| index.get(name).map(|&i| world[i].translation);
        let (Some(thigh_l), Some(thigh_r), Some(top)) =
            (head("thigh_l"), head("thigh_r"), head("head"))
        else {
            warn!("rigged model without thigh_l / thigh_r / head bones");
            commands.entity(root).insert(Visibility::Hidden);
            continue;
        };
        // Into the game's rig space: facing +Z, left +X, up +Y.
        let frame = rig_frame(thigh_l - thigh_r, Vec3::Y);
        let rest_heads: Vec<Vec3> = world.iter().map(|t| frame * t.translation).collect();
        let rest_world_rotation: Vec<Quat> = world.iter().map(|t| frame * t.rotation).collect();
        let feet = world
            .iter()
            .map(|t| t.translation.y)
            .fold(f32::INFINITY, f32::min);
        let tall = (top.y - feet).max(0.1) * 1.08;
        let skeleton_len = joints.len();
        let skeleton = Skeleton {
            joints: joints.clone(),
            names,
            parents: joint_parents,
            rest_local_rotation: local.iter().map(|t| t.rotation).collect(),
            rest_world_rotation,
            index,
            base_rotation: frame * pre.rotation,
            units_per_metre: tall / rigged.height,
        };
        for (i, joint) in joints.iter().enumerate() {
            commands.entity(*joint).insert(JointRest {
                translation: local[i].translation,
                rotation: local[i].rotation,
            });
        }
        commands.entity(root).insert(BodyInstance {
            root,
            vids: Vec::new(),
            indices: Vec::new(),
            regions: Vec::new(),
            skin_blend: Vec::new(),
            uvs: Vec::new(),
            vid_weights: Vec::new(),
            shaped: Vec::new(),
            albedo: None,
            skeleton,
            rest_heads,
            applied: None,
        });
        // Into rig space, scaled to height, feet on the origin, then placed.
        let scale = rigged.height / tall;
        let fit = Transform {
            translation: Vec3::new(0.0, -feet * scale, 0.0),
            rotation: frame,
            scale: Vec3::splat(scale),
        };
        *root_transform = rigged.at * fit;
        // The albedo is near black: lift it to gunmetal, keep the metal low
        // (there is no sky to reflect) and light the seams in the trim colour.
        if let Some(source) = skin_materials
            .get(skin_entity)
            .ok()
            .and_then(|m| materials.get(&m.0))
            .cloned()
        {
            let material = StandardMaterial {
                base_color: Color::linear_rgb(2.4, 2.4, 2.7),
                metallic: 0.45,
                perceptual_roughness: 0.55,
                emissive: rigged.trim,
                emissive_texture: source.base_color_texture.clone(),
                ..source
            };
            commands
                .entity(skin_entity)
                .insert(MeshMaterial3d(materials.add(material)));
        }
        // Skinned bounds are the bind pose; the posed body moves out of them.
        commands
            .entity(skin_entity)
            .insert(bevy::render::view::NoFrustumCulling);
        info!(
            "rigged model ready: {} joints, scale {scale:.2}",
            skeleton_len
        );
        *visibility = Visibility::Inherited;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rig_frame_turns_any_facing_into_plus_z() {
        // A rig facing +X: its left is -Z.
        let frame = rig_frame(Vec3::NEG_Z, Vec3::Y);
        assert!((frame * Vec3::X - Vec3::Z).length() < 1e-5, "forward");
        assert!((frame * Vec3::NEG_Z - Vec3::X).length() < 1e-5, "left");
        assert!((frame * Vec3::Y - Vec3::Y).length() < 1e-5, "up");
        // The MakeHuman frame is already the game's.
        let identity = rig_frame(Vec3::X, Vec3::Y);
        assert!(identity.angle_between(Quat::IDENTITY) < 1e-5);
    }
}
