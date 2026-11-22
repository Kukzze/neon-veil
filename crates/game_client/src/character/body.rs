//! The operative body in the world: each instance gets its own copy of the
//! MakeHuman mesh, shaped on the CPU from its [`CharacterAppearance`]
//! (positions, seamless normals, per-region colours), and its skeleton is
//! refitted so joints sit where the reshaped body needs them.

use super::appearance::{CharacterAppearance, HairStyle};
use super::library::MorphLibrary;
use bevy::prelude::*;
use bevy::render::mesh::skinning::{SkinnedMesh, SkinnedMeshInverseBindposes};
use bevy::render::mesh::{Indices, MeshVertexAttribute, VertexAttributeValues};
use bevy::render::render_resource::VertexFormat;
use bevy::render::view::NoFrustumCulling;
use std::collections::HashMap;

/// MakeHuman vertex index of each glTF vertex, exported as `_VID` (the
/// glTF loader registers it without the underscore).
pub const ATTRIBUTE_VID: MeshVertexAttribute =
    MeshVertexAttribute::new("Vid", 0x4E56_5644, VertexFormat::Float32);

/// Bones whose skinning marks bare skin (head, neck, hands); everything
/// else wears the suit.
const SKIN_BONES: [&str; 2] = ["head", "neck_01"];
const SKIN_BONE_PREFIXES: [&str; 6] = ["hand_", "thumb_", "index_", "middle_", "ring_", "pinky_"];
const BODY_VERTEX_COUNT: u32 = 13_380;

/// Marks a spawned operative scene; carries the appearance to apply.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct OperativeBody;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Region {
    Skin,
    Suit,
    Scalp,
    Lips,
    Nails,
    Sclera,
    Iris,
    Pupil,
    Teeth,
    Tongue,
    Lashes,
}

/// Rest pose of one joint after refitting; animation poses relative to it.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct JointRest {
    pub translation: Vec3,
    pub rotation: Quat,
}

/// Skeleton data of one body, joints in skin order.
#[derive(Debug, Clone)]
pub struct Skeleton {
    pub joints: Vec<Entity>,
    pub names: Vec<String>,
    pub parents: Vec<Option<usize>>,
    pub rest_local_rotation: Vec<Quat>,
    pub rest_world_rotation: Vec<Quat>,
    pub index: HashMap<String, usize>,
}

/// One body instance's mesh, ready to be reshaped.
#[derive(Component, Debug, Clone)]
pub struct BodyInstance {
    pub root: Entity,
    pub vids: Vec<u32>,
    pub indices: Vec<u32>,
    pub regions: Vec<Region>,
    /// For skin-or-suit vertices: 0 is suit, 1 is bare skin.
    pub skin_blend: Vec<f32>,
    pub uvs: Vec<[f32; 2]>,
    /// The composed albedo and the material showing it, with the look
    /// they were made for.
    pub albedo: Option<(AlbedoKey, Handle<Image>, Handle<StandardMaterial>)>,
    pub skeleton: Skeleton,
    /// Joint head positions (rig space) after the last refit.
    pub rest_heads: Vec<Vec3>,
    pub applied: Option<CharacterAppearance>,
}

fn find_root(
    mut entity: Entity,
    parents: &Query<&Parent>,
    bodies: &Query<(), With<OperativeBody>>,
) -> Option<Entity> {
    for _ in 0..32 {
        if bodies.contains(entity) {
            return Some(entity);
        }
        entity = parents.get(entity).ok()?.get();
    }
    None
}

/// Region of each MakeHuman vertex from the library's region lists, eye
/// geometry split into sclera, iris and pupil by facing.
pub fn vertex_regions(library: &MorphLibrary) -> Vec<Option<Region>> {
    let mut out = vec![None; library.neutral.len()];
    let mark = |name: &str, region: Region, out: &mut Vec<Option<Region>>| {
        if let Some(list) = library.regions.get(name) {
            for &v in list {
                if let Some(slot) = out.get_mut(v as usize) {
                    *slot = Some(region);
                }
            }
        }
    };
    mark("scalp", Region::Scalp, &mut out);
    mark("lips", Region::Lips, &mut out);
    mark("fingernails", Region::Nails, &mut out);
    for teeth in ["helper-upper-teeth", "helper-lower-teeth"] {
        mark(teeth, Region::Teeth, &mut out);
    }
    mark("helper-tongue", Region::Tongue, &mut out);
    for lashes in [
        "helper-l-eyelashes-1",
        "helper-l-eyelashes-2",
        "helper-r-eyelashes-1",
        "helper-r-eyelashes-2",
    ] {
        mark(lashes, Region::Lashes, &mut out);
    }
    for eye in ["helper-l-eye", "helper-r-eye"] {
        let Some(list) = library.regions.get(eye) else {
            continue;
        };
        let Some(center) = MorphLibrary::joint_position(&library.neutral, list) else {
            continue;
        };
        for &v in list {
            let dir = (library.neutral[v as usize] - center).normalize_or_zero();
            // The body faces +Z in its own space.
            out[v as usize] = Some(if dir.z > 0.93 {
                Region::Pupil
            } else if dir.z > 0.78 {
                Region::Iris
            } else {
                Region::Sclera
            });
        }
    }
    out
}

fn is_skin_bone(name: &str) -> bool {
    SKIN_BONES.contains(&name) || SKIN_BONE_PREFIXES.iter().any(|p| name.starts_with(p))
}

fn linear(c: [f32; 3]) -> Vec3 {
    let l = Color::srgb(c[0], c[1], c[2]).to_linear();
    Vec3::new(l.red, l.green, l.blue)
}

/// Linear colour of a region for an appearance.
pub fn region_color(region: Region, look: &CharacterAppearance) -> Vec3 {
    let skin = linear(look.skin_tone);
    match region {
        Region::Skin => skin,
        Region::Suit => linear(look.suit),
        Region::Scalp => match look.hair {
            HairStyle::Buzz => linear(look.hair_color),
            HairStyle::Shaved => skin * 0.92,
        },
        Region::Lips => skin * Vec3::new(0.9, 0.74, 0.72),
        Region::Nails => skin * 1.08,
        Region::Sclera => Vec3::new(0.82, 0.8, 0.78),
        Region::Iris => linear(look.eye_color),
        Region::Pupil => Vec3::splat(0.01),
        Region::Teeth => Vec3::new(0.78, 0.76, 0.68),
        Region::Tongue => Vec3::new(0.5, 0.16, 0.18),
        Region::Lashes => linear(look.hair_color) * 0.4,
    }
}

/// Vertex colour: skin vertices stay white (the albedo texture carries skin
/// and clothes); eyes, teeth, tongue and lashes are coloured here.
pub fn vertex_color(region: Region, look: &CharacterAppearance) -> Vec3 {
    match region {
        Region::Skin | Region::Suit => Vec3::ONE,
        // These multiply the skin-toned albedo, so they are relative tints.
        Region::Lips => Vec3::new(1.0, 0.8, 0.8),
        Region::Nails => Vec3::new(1.06, 1.02, 1.0),
        Region::Scalp => match look.hair {
            HairStyle::Buzz => {
                let skin = linear(look.skin_tone).max(Vec3::splat(0.05));
                (linear(look.hair_color) / skin).min(Vec3::ONE)
            }
            HairStyle::Shaved => Vec3::splat(0.92),
        },
        other => region_color(other, look),
    }
}

/// Area-weighted normals accumulated per MakeHuman vertex, so UV seams
/// (split glTF vertices) shade without visible edges.
pub fn seamless_normals(
    positions: &[Vec3],
    vids: &[u32],
    indices: &[u32],
    vid_count: usize,
) -> Vec<[f32; 3]> {
    let mut acc = vec![Vec3::ZERO; vid_count];
    for tri in indices.chunks_exact(3) {
        let (a, b, c) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);
        let n = (positions[b] - positions[a]).cross(positions[c] - positions[a]);
        for &k in &[a, b, c] {
            acc[vids[k] as usize] += n;
        }
    }
    vids.iter()
        .map(|&v| acc[v as usize].normalize_or(Vec3::Y).to_array())
        .collect()
}

/// Copies the shared body mesh for each new instance and records what
/// reshaping needs.
#[allow(clippy::too_many_arguments)]
pub fn prepare_bodies(
    mut commands: Commands,
    library: Option<Res<MorphLibrary>>,
    mut meshes: ResMut<Assets<Mesh>>,
    candidates: Query<(Entity, &Mesh3d, &SkinnedMesh), Without<BodyInstance>>,
    parents: Query<&Parent>,
    bodies: Query<(), With<OperativeBody>>,
    names: Query<&Name>,
    transforms: Query<&Transform>,
) {
    let Some(library) = library else {
        return;
    };
    let regions = std::sync::OnceLock::new();
    for (entity, mesh3d, skinned) in &candidates {
        let Some(root) = find_root(entity, &parents, &bodies) else {
            continue;
        };
        let Some(mesh) = meshes.get(&mesh3d.0) else {
            continue;
        };
        let Some(VertexAttributeValues::Float32(raw_vids)) = mesh.attribute(ATTRIBUTE_VID) else {
            continue;
        };
        let vids: Vec<u32> = raw_vids.iter().map(|v| v.round().max(0.0) as u32).collect();
        if vids.iter().any(|&v| v as usize >= library.neutral.len()) {
            warn!("operative mesh references vertices outside the morph library");
            continue;
        }
        let indices: Vec<u32> = match mesh.indices() {
            Some(Indices::U16(i)) => i.iter().map(|&x| u32::from(x)).collect(),
            Some(Indices::U32(i)) => i.clone(),
            None => continue,
        };

        // Skeleton, in skin order.
        let joint_names: Vec<String> = skinned
            .joints
            .iter()
            .map(|j| {
                names
                    .get(*j)
                    .map(|n| n.as_str().to_owned())
                    .unwrap_or_default()
            })
            .collect();
        let index: HashMap<String, usize> = joint_names
            .iter()
            .enumerate()
            .map(|(i, n)| (n.clone(), i))
            .collect();
        let joint_parents: Vec<Option<usize>> = skinned
            .joints
            .iter()
            .map(|j| {
                parents
                    .get(*j)
                    .ok()
                    .and_then(|p| skinned.joints.iter().position(|x| *x == p.get()))
            })
            .collect();
        let rest_local_rotation: Vec<Quat> = skinned
            .joints
            .iter()
            .map(|j| transforms.get(*j).map_or(Quat::IDENTITY, |t| t.rotation))
            .collect();
        let mut rest_world_rotation = vec![None; skinned.joints.len()];
        fn world_rot(
            i: usize,
            parents: &[Option<usize>],
            local: &[Quat],
            memo: &mut [Option<Quat>],
        ) -> Quat {
            if let Some(q) = memo[i] {
                return q;
            }
            let q = match parents[i] {
                Some(p) => world_rot(p, parents, local, memo) * local[i],
                None => local[i],
            };
            memo[i] = Some(q);
            q
        }
        for i in 0..skinned.joints.len() {
            world_rot(
                i,
                &joint_parents,
                &rest_local_rotation,
                &mut rest_world_rotation,
            );
        }
        let skeleton = Skeleton {
            joints: skinned.joints.clone(),
            names: joint_names.clone(),
            parents: joint_parents,
            rest_local_rotation,
            rest_world_rotation: rest_world_rotation
                .into_iter()
                .map(|q| q.unwrap_or_default())
                .collect(),
            index,
        };

        // Regions: special areas by vertex, then skin or suit by bone.
        let by_vid = regions.get_or_init(|| vertex_regions(&library));
        let joints_attr = mesh.attribute(Mesh::ATTRIBUTE_JOINT_INDEX);
        let weights_attr = mesh.attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT);
        let (vertex_regions, skin_blend): (Vec<Region>, Vec<f32>) = vids
            .iter()
            .enumerate()
            .map(|(i, &vid)| {
                if let Some(region) = by_vid[vid as usize] {
                    return (region, 1.0);
                }
                if vid >= BODY_VERTEX_COUNT {
                    return (Region::Skin, 1.0);
                }
                let skin_weight = match (joints_attr, weights_attr) {
                    (
                        Some(VertexAttributeValues::Uint16x4(j)),
                        Some(VertexAttributeValues::Float32x4(w)),
                    ) => (0..4)
                        .filter(|&k| {
                            joint_names
                                .get(usize::from(j[i][k]))
                                .is_some_and(|n| is_skin_bone(n))
                        })
                        .map(|k| w[i][k])
                        .sum::<f32>(),
                    _ => 0.0,
                };
                // A soft edge where the suit meets skin (neck, wrists).
                let t = ((skin_weight - 0.3) / 0.4).clamp(0.0, 1.0);
                (Region::Skin, t * t * (3.0 - 2.0 * t))
            })
            .unzip();

        let uvs: Vec<[f32; 2]> = match mesh.attribute(Mesh::ATTRIBUTE_UV_0) {
            Some(VertexAttributeValues::Float32x2(uv)) => uv.clone(),
            _ => continue,
        };
        let cloned = mesh.clone();
        let copy = meshes.add(cloned);
        commands.entity(entity).insert((
            Mesh3d(copy),
            NoFrustumCulling,
            BodyInstance {
                root,
                vids,
                indices,
                regions: vertex_regions,
                skin_blend,
                uvs,
                albedo: None,
                skeleton,
                rest_heads: Vec::new(),
                applied: None,
            },
        ));
    }
}

/// Reshapes every body whose appearance changed: mesh, colours, skeleton.
#[allow(clippy::too_many_arguments)]
pub fn apply_appearances(
    library: Option<Res<MorphLibrary>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut bindposes: ResMut<Assets<SkinnedMeshInverseBindposes>>,
    mut bodies: Query<(&Mesh3d, &mut SkinnedMesh, &mut BodyInstance)>,
    looks: Query<&CharacterAppearance>,
    mut joints: Query<(&mut Transform, Option<&mut JointRest>)>,
    mut commands: Commands,
) {
    let Some(library) = library else {
        return;
    };
    for (mesh3d, mut skinned, mut body) in &mut bodies {
        let Ok(look) = looks.get(body.root) else {
            continue;
        };
        if body.applied.as_ref() == Some(look) {
            continue;
        }
        let Some(mesh) = meshes.get_mut(&mesh3d.0) else {
            continue;
        };
        let shaped = library.shape(look);
        let positions: Vec<Vec3> = body.vids.iter().map(|&v| shaped[v as usize]).collect();
        let normals = seamless_normals(&positions, &body.vids, &body.indices, shaped.len());
        let colors: Vec<[f32; 4]> = body
            .regions
            .iter()
            .map(|r| {
                let c = vertex_color(*r, look);
                [c.x, c.y, c.z, 1.0]
            })
            .collect();
        mesh.insert_attribute(
            Mesh::ATTRIBUTE_POSITION,
            positions.iter().map(|p| p.to_array()).collect::<Vec<_>>(),
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
        // The skin normal map needs tangents for the reshaped surface.
        if let Err(error) = mesh.generate_tangents() {
            warn!("operative tangents: {error}");
        }

        // Refit: joints move to where the reshaped body puts them; rest
        // rotations stay, so animation behaves the same on every body.
        let skeleton = &body.skeleton;
        let count = skeleton.joints.len();
        let mut world_t = vec![Vec3::ZERO; count];
        for (i, name) in skeleton.names.iter().enumerate() {
            world_t[i] = library
                .bones
                .get(name)
                .and_then(|b| MorphLibrary::joint_position(&shaped, &b.head))
                .unwrap_or(Vec3::ZERO);
        }
        let mut inverse = Vec::with_capacity(count);
        for i in 0..count {
            let world =
                Mat4::from_rotation_translation(skeleton.rest_world_rotation[i], world_t[i]);
            inverse.push(world.inverse());
            let local_t = match skeleton.parents[i] {
                Some(p) => skeleton.rest_world_rotation[p].inverse() * (world_t[i] - world_t[p]),
                None => world_t[i],
            };
            let rest = JointRest {
                translation: local_t,
                rotation: skeleton.rest_local_rotation[i],
            };
            if let Ok((mut transform, existing)) = joints.get_mut(skeleton.joints[i]) {
                transform.translation = local_t;
                match existing {
                    Some(mut r) => *r = rest,
                    None => {
                        commands.entity(skeleton.joints[i]).insert(rest);
                    }
                }
            }
        }
        skinned.inverse_bindposes = bindposes.add(SkinnedMeshInverseBindposes::from(inverse));
        body.rest_heads = world_t;
        body.applied = Some(look.clone());
    }
}

/// What the albedo depends on; slider changes alone do not recompose it.
#[derive(Debug, Clone, PartialEq)]
pub struct AlbedoKey {
    skin: [f32; 3],
    suit: [f32; 3],
    accent: [f32; 3],
    outfit: super::appearance::Outfit,
    feminine: bool,
}

impl AlbedoKey {
    pub fn of(look: &CharacterAppearance) -> Self {
        Self {
            skin: look.skin_tone,
            suit: look.suit,
            accent: look.accent,
            outfit: look.outfit,
            feminine: look.slider("gender") > 0.15,
        }
    }
}

/// Shared atlas data, built from the first prepared body.
#[derive(Resource)]
pub struct SkinAtlas {
    pub garments: super::skin_texture::GarmentAtlas,
    pub table: super::skin_texture::SrgbTable,
}

/// Composes each body's albedo when its colours, outfit or gender side
/// change, and gives it a material showing it.
#[allow(clippy::too_many_arguments)]
pub fn compose_albedos(
    mut commands: Commands,
    library: Option<Res<MorphLibrary>>,
    atlas: Option<Res<SkinAtlas>>,
    base: Res<super::BodyMaterial>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut bodies: Query<(&mut BodyInstance, &mut MeshMaterial3d<StandardMaterial>)>,
    looks: Query<&CharacterAppearance>,
) {
    use super::skin_texture::{build_atlas, compose_albedo, GarmentFit, SrgbTable, ATLAS_SIZE};
    use bevy::render::render_asset::RenderAssetUsages;
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
    let Some(library) = library else {
        return;
    };
    let Some(atlas) = atlas else {
        // Build once from any prepared body (all share topology and UVs).
        if let Some((body, _)) = bodies.iter().next() {
            if let Some(fit) = GarmentFit::from_library(&library) {
                let garments = build_atlas(
                    ATLAS_SIZE,
                    &body.uvs,
                    &body.indices,
                    &body.vids,
                    &library.neutral,
                    &body.skin_blend,
                    &fit,
                );
                commands.insert_resource(SkinAtlas {
                    garments,
                    table: SrgbTable::default(),
                });
            }
        }
        return;
    };
    for (mut body, mut material) in &mut bodies {
        let Ok(look) = looks.get(body.root) else {
            continue;
        };
        let key = AlbedoKey::of(look);
        if body.albedo.as_ref().is_some_and(|(k, _, _)| *k == key) {
            continue;
        }
        let detail_handle = if key.feminine {
            &base.detail_female
        } else {
            &base.detail_male
        };
        let Some(detail) = images.get(detail_handle) else {
            continue;
        };
        // Grayscale maps load as one byte per texel; take red otherwise.
        let stride = (detail.data.len() / (ATLAS_SIZE * ATLAS_SIZE)).max(1);
        if detail.data.len() < ATLAS_SIZE * ATLAS_SIZE {
            continue;
        }
        let gray: Vec<u8> = detail
            .data
            .iter()
            .step_by(stride)
            .copied()
            .take(ATLAS_SIZE * ATLAS_SIZE)
            .collect();
        let rgba = compose_albedo(&atlas.garments, &gray, look, &atlas.table);
        let image = Image::new(
            Extent3d {
                width: ATLAS_SIZE as u32,
                height: ATLAS_SIZE as u32,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            rgba,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::RENDER_WORLD,
        );
        let image = images.add(image);
        let handle = match &body.albedo {
            Some((_, _, existing)) => {
                if let Some(m) = materials.get_mut(existing) {
                    m.base_color_texture = Some(image.clone());
                }
                existing.clone()
            }
            None => {
                let mut m = materials.get(&base.template).cloned().unwrap_or_default();
                m.base_color_texture = Some(image.clone());
                materials.add(m)
            }
        };
        material.0 = handle.clone();
        body.albedo = Some((key, image, handle));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seams_share_normals() {
        // Two triangles forming a quad; vertex 3 duplicates vertex 0 (a seam).
        let positions = [
            Vec3::ZERO,
            Vec3::X,
            Vec3::Y,
            Vec3::ZERO,
            Vec3::new(1.0, 1.0, 0.0),
        ];
        let vids = [0, 1, 2, 0, 3];
        let indices = [0, 1, 2, 3, 1, 4];
        let n = seamless_normals(&positions, &vids, &indices, 4);
        assert_eq!(n[0], n[3], "split vertices share a normal");
        assert!((Vec3::from(n[0]) - Vec3::Z).length() < 1e-5);
    }

    #[test]
    fn regions_colour_from_the_appearance() {
        let mut look = CharacterAppearance {
            hair: HairStyle::Buzz,
            ..default()
        };
        assert_eq!(region_color(Region::Scalp, &look), linear(look.hair_color));
        look.hair = HairStyle::Shaved;
        assert_ne!(region_color(Region::Scalp, &look), linear(look.hair_color));
        assert_eq!(region_color(Region::Iris, &look), linear(look.eye_color));
        assert!(is_skin_bone("hand_r") && is_skin_bone("index_02_l") && !is_skin_bone("spine_02"));
    }

    #[test]
    fn shipped_eyes_have_pupils_irises_and_sclera() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let library = MorphLibrary::load(&root).expect("library");
        let regions = vertex_regions(&library);
        for wanted in [
            Region::Pupil,
            Region::Iris,
            Region::Sclera,
            Region::Teeth,
            Region::Scalp,
        ] {
            assert!(regions.contains(&Some(wanted)), "{wanted:?}");
        }
    }
}
