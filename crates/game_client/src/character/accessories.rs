//! Hair, brows, lashes and clothes on operative bodies: each worn proxy is
//! a skinned mesh sharing the body's skeleton, refitted whenever the body
//! is reshaped, and garments hide the skin beneath them.

use super::appearance::CharacterAppearance;
use super::body::BodyInstance;
use super::proxy::{parse_proxy, ProxyAsset};
use bevy::prelude::*;
use bevy::render::mesh::skinning::SkinnedMesh;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::view::NoFrustumCulling;
use std::collections::{HashMap, HashSet};
use std::path::Path;

pub const PROXY_DIR: &str = "characters/proxies";
/// Tintable textures average this luminance (see build_proxies.py).
const TINT_MEAN: f32 = 0.75;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProxyKind {
    Hair,
    Brows,
    Lashes,
    Clothes,
}

impl ProxyKind {
    fn folder(self) -> &'static str {
        match self {
            Self::Hair => "hair",
            Self::Brows => "brows",
            Self::Lashes => "lashes",
            Self::Clothes => "clothes",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProxyEntry {
    pub kind: ProxyKind,
    pub asset: ProxyAsset,
    pub texture: Option<Handle<Image>>,
}

/// Every proxy shipped with the game, by name.
#[derive(Resource, Debug, Default, Clone)]
pub struct ProxyLibrary {
    pub entries: HashMap<String, ProxyEntry>,
}

impl ProxyLibrary {
    pub fn names(&self, kind: ProxyKind) -> Vec<&str> {
        let mut names: Vec<&str> = self
            .entries
            .iter()
            .filter(|(_, e)| e.kind == kind)
            .map(|(n, _)| n.as_str())
            .collect();
        names.sort_unstable();
        names
    }
}

/// Reads every proxy's mesh and binding (textures load through assets).
pub fn load_proxy_library(root: &Path, assets: Option<&AssetServer>) -> ProxyLibrary {
    let mut library = ProxyLibrary::default();
    for kind in [
        ProxyKind::Hair,
        ProxyKind::Brows,
        ProxyKind::Lashes,
        ProxyKind::Clothes,
    ] {
        let dir = root.join(PROXY_DIR).join(kind.folder());
        let Ok(read) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in read.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let obj = std::fs::read_to_string(entry.path().join(format!("{name}.obj")));
            let mhclo = std::fs::read_to_string(entry.path().join(format!("{name}.mhclo")));
            let (Ok(obj), Ok(mhclo)) = (obj, mhclo) else {
                continue;
            };
            match parse_proxy(&name, &obj, &mhclo) {
                Ok(asset) => {
                    let texture = assets.map(|a| {
                        a.load(format!("{PROXY_DIR}/{}/{name}/texture.png", kind.folder()))
                    });
                    library.entries.insert(
                        name,
                        ProxyEntry {
                            kind,
                            asset,
                            texture,
                        },
                    );
                }
                Err(error) => warn!("proxy {name} skipped: {error}"),
            }
        }
    }
    library
}

/// The proxies `look` wears, in drawing order.
pub fn worn(look: &CharacterAppearance) -> Vec<String> {
    let mut out = Vec::new();
    if !look.hair_mesh.is_empty() {
        out.push(look.hair_mesh.clone());
    }
    if !look.brow_mesh.is_empty() {
        out.push(look.brow_mesh.clone());
    }
    if look.lashes {
        out.push(
            if look.slider("gender") > 0.15 {
                "eyelashes02"
            } else {
                "eyelashes01"
            }
            .to_owned(),
        );
    }
    out.extend(look.clothing.iter().cloned());
    out
}

/// One worn proxy on one body.
#[derive(Component, Debug, Clone)]
pub struct ProxyPiece {
    pub body: Entity,
    pub name: String,
    /// Render vertex -> (obj vertex, uv), and triangle indices.
    pub layout: Vec<(u32, u32)>,
    pub indices: Vec<u32>,
    pub fitted: Option<CharacterAppearance>,
    pub tint: Option<[f32; 3]>,
}

fn linear(c: [f32; 3]) -> Vec3 {
    let l = Color::srgb(c[0], c[1], c[2]).to_linear();
    Vec3::new(l.red, l.green, l.blue)
}

fn proxy_material(entry: &ProxyEntry, look: &CharacterAppearance) -> StandardMaterial {
    let tinted = entry.kind != ProxyKind::Clothes;
    let base = if tinted {
        let c = linear(look.hair_color) / TINT_MEAN;
        Color::linear_rgb(c.x, c.y, c.z)
    } else {
        Color::WHITE
    };
    StandardMaterial {
        base_color: base,
        base_color_texture: entry.texture.clone(),
        alpha_mode: if tinted {
            AlphaMode::Mask(0.35)
        } else {
            AlphaMode::Opaque
        },
        double_sided: true,
        cull_mode: None,
        perceptual_roughness: if tinted { 0.55 } else { 0.8 },
        reflectance: if tinted { 0.4 } else { 0.3 },
        ..default()
    }
}

/// Spawns and removes proxy pieces so each body wears exactly its look.
pub fn sync_worn_proxies(
    mut commands: Commands,
    library: Option<Res<ProxyLibrary>>,
    bodies: Query<(Entity, &BodyInstance, &SkinnedMesh)>,
    looks: Query<&CharacterAppearance>,
    pieces: Query<(Entity, &ProxyPiece)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Some(library) = library else {
        return;
    };
    let mut have: HashMap<Entity, Vec<(Entity, String)>> = HashMap::new();
    for (entity, piece) in &pieces {
        have.entry(piece.body)
            .or_default()
            .push((entity, piece.name.clone()));
    }
    for (body_entity, body, skinned) in &bodies {
        if body.shaped.is_empty() {
            continue;
        }
        let Ok(look) = looks.get(body.root) else {
            continue;
        };
        let wanted: Vec<String> = worn(look)
            .into_iter()
            .filter(|n| library.entries.contains_key(n))
            .collect();
        let current = have.remove(&body_entity).unwrap_or_default();
        for (entity, name) in &current {
            if !wanted.contains(name) {
                commands.entity(*entity).despawn_recursive();
            }
        }
        for name in wanted {
            if current.iter().any(|(_, n)| *n == name) {
                continue;
            }
            let entry = &library.entries[&name];
            if entry.asset.max_body_vertex() as usize >= body.shaped.len() {
                warn!("proxy {name} binds outside the body");
                continue;
            }
            let (layout, indices) = entry.asset.render_layout();
            let weights = entry.asset.skin_weights(&|v| weights_or_nearest(body, v));
            let mesh = build_mesh(&entry.asset, &layout, &indices, &body.shaped, &weights);
            let piece = commands
                .spawn((
                    Name::new(format!("Proxy {name}")),
                    Mesh3d(meshes.add(mesh)),
                    MeshMaterial3d(materials.add(proxy_material(entry, look))),
                    SkinnedMesh {
                        inverse_bindposes: skinned.inverse_bindposes.clone(),
                        joints: skinned.joints.clone(),
                    },
                    Transform::default(),
                    NoFrustumCulling,
                    ProxyPiece {
                        body: body_entity,
                        name: name.clone(),
                        layout,
                        indices,
                        fitted: Some(look.clone()),
                        tint: Some(look.hair_color),
                    },
                ))
                .id();
            commands.entity(body_entity).add_child(piece);
        }
    }
    // Pieces whose body is gone.
    for (_, orphans) in have {
        for (entity, _) in orphans {
            commands.entity(entity).despawn_recursive();
        }
    }
}

/// Skin weights of a body vertex, or of the nearest weighted one (helper
/// vertices such as the hair shell are not in the rendered mesh).
fn weights_or_nearest(body: &BodyInstance, v: u32) -> Option<([u16; 4], [f32; 4])> {
    if let Some(w) = body.vid_weights.get(v as usize).copied().flatten() {
        return Some(w);
    }
    let at = *body.shaped.get(v as usize)?;
    body.vid_weights
        .iter()
        .enumerate()
        .filter_map(|(i, w)| w.map(|w| (i, w)))
        .min_by(|a, b| {
            body.shaped[a.0]
                .distance_squared(at)
                .total_cmp(&body.shaped[b.0].distance_squared(at))
        })
        .map(|(_, w)| w)
}

fn build_mesh(
    asset: &ProxyAsset,
    layout: &[(u32, u32)],
    indices: &[u32],
    shaped: &[Vec3],
    weights: &[([u16; 4], [f32; 4])],
) -> Mesh {
    let fitted = asset.fit(shaped);
    let positions: Vec<[f32; 3]> = layout
        .iter()
        .map(|(v, _)| fitted[*v as usize].to_array())
        .collect();
    let uvs: Vec<[f32; 2]> = layout
        .iter()
        .map(|(_, t)| asset.uvs.get(*t as usize).copied().unwrap_or([0.0, 0.0]))
        .collect();
    let joints: Vec<[u16; 4]> = layout.iter().map(|(v, _)| weights[*v as usize].0).collect();
    let joint_weights: Vec<[f32; 4]> = layout.iter().map(|(v, _)| weights[*v as usize].1).collect();
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_JOINT_INDEX,
        bevy::render::mesh::VertexAttributeValues::Uint16x4(joints),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT, joint_weights)
    .with_inserted_indices(Indices::U32(indices.to_vec()));
    mesh.compute_smooth_normals();
    mesh
}

/// Refits pieces after a reshape and keeps their skins and tints current.
pub fn refit_worn_proxies(
    library: Option<Res<ProxyLibrary>>,
    bodies: Query<(&BodyInstance, &SkinnedMesh), Without<ProxyPiece>>,
    looks: Query<&CharacterAppearance>,
    mut pieces: Query<(
        &mut ProxyPiece,
        &Mesh3d,
        &mut SkinnedMesh,
        &MeshMaterial3d<StandardMaterial>,
    )>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Some(library) = library else {
        return;
    };
    for (mut piece, mesh3d, mut skin, material) in &mut pieces {
        let Ok((body, body_skin)) = bodies.get(piece.body) else {
            continue;
        };
        if skin.inverse_bindposes != body_skin.inverse_bindposes {
            skin.inverse_bindposes = body_skin.inverse_bindposes.clone();
        }
        let (Some(look), Some(entry)) = (body.applied.as_ref(), library.entries.get(&piece.name))
        else {
            continue;
        };
        let _ = looks;
        if piece.tint != Some(look.hair_color) {
            if let Some(m) = materials.get_mut(&material.0) {
                *m = proxy_material(entry, look);
            }
            piece.tint = Some(look.hair_color);
        }
        if piece.fitted.as_ref() == Some(look) || body.shaped.is_empty() {
            continue;
        }
        let Some(mesh) = meshes.get_mut(&mesh3d.0) else {
            continue;
        };
        let fitted = entry.asset.fit(&body.shaped);
        let positions: Vec<[f32; 3]> = piece
            .layout
            .iter()
            .map(|(v, _)| fitted[*v as usize].to_array())
            .collect();
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
        mesh.compute_smooth_normals();
        piece.fitted = Some(look.clone());
    }
}

/// Body vertices hidden by the clothes a look wears.
pub fn hidden_vertices(library: &ProxyLibrary, look: &CharacterAppearance) -> HashSet<u32> {
    look.clothing
        .iter()
        .filter_map(|n| library.entries.get(n))
        .flat_map(|e| e.asset.hidden.iter().copied())
        .collect()
}

/// Drops body triangles under worn clothes so skin never pokes through.
pub fn hide_covered_skin(
    library: Option<Res<ProxyLibrary>>,
    mut bodies: Query<(&BodyInstance, &Mesh3d, &mut HiddenSkin)>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let Some(library) = library else {
        return;
    };
    for (body, mesh3d, mut hidden) in &mut bodies {
        let Some(look) = body.applied.as_ref() else {
            continue;
        };
        let key: Vec<String> = look.clothing.clone();
        if hidden.0.as_ref() == Some(&key) {
            continue;
        }
        let Some(mesh) = meshes.get_mut(&mesh3d.0) else {
            continue;
        };
        let covered = hidden_vertices(&library, look);
        let indices: Vec<u32> = body
            .indices
            .chunks_exact(3)
            .filter(|t| !t.iter().any(|&i| covered.contains(&body.vids[i as usize])))
            .flatten()
            .copied()
            .collect();
        mesh.insert_indices(Indices::U32(indices));
        hidden.0 = Some(key);
    }
}

/// The clothing set the body's index buffer currently hides.
#[derive(Component, Debug, Default, Clone)]
pub struct HiddenSkin(pub Option<Vec<String>>);

pub fn add_hidden_skin(
    mut commands: Commands,
    bodies: Query<Entity, (With<BodyInstance>, Without<HiddenSkin>)>,
) {
    for entity in &bodies {
        commands.entity(entity).insert(HiddenSkin::default());
    }
}

/// Loads the proxy library at startup.
pub fn setup_proxy_library(mut commands: Commands, assets: Res<AssetServer>) {
    let library = load_proxy_library(&crate::app::asset_root(), Some(&assets));
    info!("proxy library: {} assets", library.entries.len());
    commands.insert_resource(library);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_proxies_parse_and_bind_inside_the_body() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let library = load_proxy_library(&root, None);
        let body = super::super::MorphLibrary::load(&root).expect("body");
        assert!(library.names(ProxyKind::Hair).len() >= 8);
        assert!(library.names(ProxyKind::Brows).len() >= 8);
        assert!(!library.names(ProxyKind::Clothes).is_empty());
        for (name, entry) in &library.entries {
            assert!(
                (entry.asset.max_body_vertex() as usize) < body.neutral.len(),
                "{name}"
            );
            let fitted = entry.asset.fit(&body.neutral);
            assert!(fitted.iter().all(|p| p.is_finite()), "{name}");
        }
        // Hair sits on the head.
        let hair = &library.entries["short02"].asset;
        let top = hair
            .fit(&body.neutral)
            .iter()
            .map(|p| p.y)
            .fold(f32::MIN, f32::max);
        assert!(top > 1.5, "hair top at {top}");
    }

    #[test]
    fn looks_wear_hair_brows_lashes_and_clothes() {
        let look = CharacterAppearance {
            clothing: vec!["shoes01".into()],
            ..default()
        };
        let names = worn(&look);
        assert!(names.contains(&look.hair_mesh));
        assert!(names.contains(&"eyelashes01".to_string()));
        assert!(names.contains(&"shoes01".to_string()));
    }
}
