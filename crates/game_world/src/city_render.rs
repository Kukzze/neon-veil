//! Draws the generated city around the camera: blocks stream in nearest
//! first and stream out once far behind, so the 10 km map costs only what
//! is in view. Towers are one mesh each with a tiling emissive window
//! texture; the boundary wall is built once.

use crate::city::{self, Solid, SolidKind};
use crate::mipmaps::{mip_chain, tiling_sampler};
use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use std::collections::HashMap;

/// Blocks drawn within this many block pitches of the camera.
pub const STREAM_RADIUS_BLOCKS: i32 = 6;
/// Blocks further than this are removed.
const UNLOAD_RADIUS_BLOCKS: i32 = 8;
/// Blocks built per frame, so streaming never hitches.
const BLOCKS_PER_FRAME: usize = 3;
/// One window texture tile covers this much facade (16 x 16 windows).
const WINDOW_TILE: Vec2 = Vec2::new(40.0, 54.4);
const WINDOW_TEXTURE_SIZE: usize = 512;
const WINDOW_CELLS: usize = 16;

// Unloading further out than loading keeps blocks from flickering at the edge.
const _: () = assert!(UNLOAD_RADIUS_BLOCKS > STREAM_RADIUS_BLOCKS);

/// Root of one drawn block.
#[derive(Component, Debug, Clone, Copy)]
pub struct CityBlock(pub i32, pub i32);

/// Marks city geometry (blocks and the boundary).
#[derive(Component, Debug, Clone, Copy)]
pub struct CityPiece;

/// Shared meshes and materials for the city.
#[derive(Resource)]
pub struct CityPalette {
    cube: Handle<Mesh>,
    towers: Vec<Handle<StandardMaterial>>,
    sidewalk: Handle<StandardMaterial>,
    prop: Handle<StandardMaterial>,
    rooftop: Handle<StandardMaterial>,
    barrier: Handle<StandardMaterial>,
    barrier_glow: Handle<StandardMaterial>,
    neon: Vec<Handle<StandardMaterial>>,
}

/// Which blocks are drawn.
#[derive(Resource, Debug, Default)]
pub struct CityStream {
    pub loaded: HashMap<(i32, i32), Entity>,
}

/// Window texture: a grid of lit, dim and dark windows on a black wall.
/// `lit` is the fraction of lit windows, `warm` of those lit warm.
fn window_texture(seed: u64, lit: f32, warm: f32) -> Vec<u8> {
    let size = WINDOW_TEXTURE_SIZE;
    let cell = size / WINDOW_CELLS;
    let mut data = vec![0u8; size * size * 4];
    let mut state = seed | 1;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state >> 40) as f32 / (1u64 << 24) as f32
    };
    for cy in 0..WINDOW_CELLS {
        for cx in 0..WINDOW_CELLS {
            let roll = next();
            let tint = next();
            let level = 0.35 + 0.65 * next();
            let color: [f32; 3] = if roll < lit {
                if tint < warm {
                    [1.0, 0.72, 0.42]
                } else {
                    [0.55, 0.8, 1.0]
                }
            } else {
                // Unlit glass catches a little city glow.
                [0.03, 0.04, 0.06]
            };
            let level = if roll < lit { level } else { 1.0 };
            // Window pane inset in its cell: wall stays black around it.
            let (x0, x1) = (cx * cell + cell / 5, cx * cell + cell - cell / 5);
            let (y0, y1) = (cy * cell + cell / 4, cy * cell + cell - cell / 6);
            for y in y0..y1 {
                for x in x0..x1 {
                    let o = (y * size + x) * 4;
                    for c in 0..3 {
                        data[o + c] = (color[c] * level * 255.0) as u8;
                    }
                    data[o + 3] = 255;
                }
            }
        }
    }
    // Pixel (0, 0) is wall: roofs sample it.
    data
}

fn window_image(seed: u64, lit: f32, warm: f32) -> Image {
    let base = window_texture(seed, lit, warm);
    let (data, levels) = mip_chain(&base, WINDOW_TEXTURE_SIZE, WINDOW_TEXTURE_SIZE, 4);
    let mut image = Image::new(
        Extent3d {
            width: WINDOW_TEXTURE_SIZE as u32,
            height: WINDOW_TEXTURE_SIZE as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        base,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    // Level 0 is checked on creation; the chain goes in afterwards.
    image.data = data;
    image.texture_descriptor.mip_level_count = levels;
    image.sampler = tiling_sampler();
    image
}

/// A box with world-scale facade UVs on its four sides; the roof samples
/// the texture's wall corner (unlit).
pub fn facade_mesh(size: Vec3) -> Mesh {
    let h = size * 0.5;
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();
    let mut face = |corners: [Vec3; 4], normal: Vec3, uv: [[f32; 2]; 4]| {
        let base = positions.len() as u32;
        for (c, t) in corners.iter().zip(uv) {
            positions.push(c.to_array());
            normals.push(normal.to_array());
            uvs.push(t);
        }
        indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    };
    let (u_x, u_z, v_y) = (
        size.x / WINDOW_TILE.x,
        size.z / WINDOW_TILE.x,
        size.y / WINDOW_TILE.y,
    );
    // +Z and -Z faces.
    face(
        [
            Vec3::new(-h.x, -h.y, h.z),
            Vec3::new(h.x, -h.y, h.z),
            Vec3::new(h.x, h.y, h.z),
            Vec3::new(-h.x, h.y, h.z),
        ],
        Vec3::Z,
        [[0.0, v_y], [u_x, v_y], [u_x, 0.0], [0.0, 0.0]],
    );
    face(
        [
            Vec3::new(h.x, -h.y, -h.z),
            Vec3::new(-h.x, -h.y, -h.z),
            Vec3::new(-h.x, h.y, -h.z),
            Vec3::new(h.x, h.y, -h.z),
        ],
        Vec3::NEG_Z,
        [[0.0, v_y], [u_x, v_y], [u_x, 0.0], [0.0, 0.0]],
    );
    // +X and -X faces.
    face(
        [
            Vec3::new(h.x, -h.y, h.z),
            Vec3::new(h.x, -h.y, -h.z),
            Vec3::new(h.x, h.y, -h.z),
            Vec3::new(h.x, h.y, h.z),
        ],
        Vec3::X,
        [[0.0, v_y], [u_z, v_y], [u_z, 0.0], [0.0, 0.0]],
    );
    face(
        [
            Vec3::new(-h.x, -h.y, -h.z),
            Vec3::new(-h.x, -h.y, h.z),
            Vec3::new(-h.x, h.y, h.z),
            Vec3::new(-h.x, h.y, -h.z),
        ],
        Vec3::NEG_X,
        [[0.0, v_y], [u_z, v_y], [u_z, 0.0], [0.0, 0.0]],
    );
    // Roof.
    face(
        [
            Vec3::new(-h.x, h.y, h.z),
            Vec3::new(h.x, h.y, h.z),
            Vec3::new(h.x, h.y, -h.z),
            Vec3::new(-h.x, h.y, -h.z),
        ],
        Vec3::Y,
        [[0.001, 0.001]; 4],
    );
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_indices(Indices::U32(indices))
}

pub fn setup_city_palette(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: Option<ResMut<Assets<Image>>>,
) {
    let towers = [
        (0x11, 0.32, 0.7, [0.045, 0.047, 0.055]),
        (0x22, 0.22, 0.3, [0.05, 0.05, 0.06]),
        (0x33, 0.45, 0.55, [0.04, 0.042, 0.05]),
        (0x44, 0.15, 0.8, [0.06, 0.058, 0.06]),
        (0x55, 0.38, 0.2, [0.035, 0.04, 0.05]),
        (0x66, 0.28, 0.5, [0.055, 0.05, 0.05]),
    ]
    .map(|(seed, lit, warm, base)| {
        // Headless runs have no image assets: towers just go untextured.
        let windows = images
            .as_mut()
            .map(|i| i.add(window_image(seed, lit, warm)));
        materials.add(StandardMaterial {
            base_color: Color::srgb(base[0], base[1], base[2]),
            perceptual_roughness: 0.55,
            metallic: 0.25,
            emissive: LinearRgba::rgb(3.0, 3.0, 3.0),
            emissive_texture: windows,
            ..default()
        })
    });
    let lit = |materials: &mut Assets<StandardMaterial>, c: [f32; 3], rough: f32, metal: f32| {
        materials.add(StandardMaterial {
            base_color: Color::srgb(c[0], c[1], c[2]),
            perceptual_roughness: rough,
            metallic: metal,
            ..default()
        })
    };
    let glow = |materials: &mut Assets<StandardMaterial>, c: LinearRgba| {
        materials.add(StandardMaterial {
            base_color: Color::BLACK,
            emissive: c,
            ..default()
        })
    };
    let neon = [
        LinearRgba::rgb(0.2, 3.5, 4.0),
        LinearRgba::rgb(4.5, 0.4, 2.8),
        LinearRgba::rgb(4.0, 2.2, 0.3),
        LinearRgba::rgb(1.2, 4.0, 1.0),
    ]
    .map(|c| glow(&mut materials, c))
    .to_vec();
    let palette = CityPalette {
        cube: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
        towers: towers.to_vec(),
        sidewalk: lit(&mut materials, [0.07, 0.075, 0.085], 0.45, 0.0),
        prop: lit(&mut materials, [0.12, 0.14, 0.13], 0.5, 0.5),
        rooftop: lit(&mut materials, [0.14, 0.15, 0.15], 0.45, 0.6),
        barrier: lit(&mut materials, [0.03, 0.03, 0.035], 0.3, 0.7),
        barrier_glow: glow(&mut materials, LinearRgba::rgb(6.0, 0.6, 0.4)),
        neon,
    };
    spawn_boundary(&mut commands, &palette);
    commands.insert_resource(palette);
}

fn spawn_boundary(commands: &mut Commands, palette: &CityPalette) {
    for wall in city::boundary_solids() {
        let size = wall.size();
        commands.spawn((
            CityPiece,
            Mesh3d(palette.cube.clone()),
            MeshMaterial3d(palette.barrier.clone()),
            Transform::from_translation(wall.center()).with_scale(size),
        ));
        // A red warning band at head height and one along the top.
        for y in [2.2, wall.max.y - 1.0] {
            let inward =
                -wall.center().with_y(0.0).normalize_or_zero() * (size.x.min(size.z) * 0.5 + 0.05);
            commands.spawn((
                CityPiece,
                Mesh3d(palette.cube.clone()),
                MeshMaterial3d(palette.barrier_glow.clone()),
                Transform::from_translation(wall.center().with_y(y) + inward)
                    .with_scale(Vec3::new(size.x.max(0.1), 0.35, size.z.max(0.1))),
            ));
        }
    }
}

fn spawn_solid(
    parent: &mut ChildBuilder,
    meshes: &mut Assets<Mesh>,
    palette: &CityPalette,
    solid: &Solid,
    index: usize,
) {
    let size = solid.size();
    let center = solid.center();
    match solid.kind {
        SolidKind::Building { tone } => {
            let material = palette.towers[usize::from(tone) % palette.towers.len()].clone();
            parent.spawn((
                CityPiece,
                Mesh3d(meshes.add(facade_mesh(size))),
                MeshMaterial3d(material),
                Transform::from_translation(center),
            ));
            // Some towers carry a vertical neon strip on a corner.
            if index % 3 == 0 && size.y > 20.0 {
                let neon = palette.neon[index % palette.neon.len()].clone();
                let corner = Vec3::new(solid.max.x + 0.1, center.y, solid.max.z + 0.1);
                parent.spawn((
                    CityPiece,
                    Mesh3d(palette.cube.clone()),
                    MeshMaterial3d(neon),
                    Transform::from_translation(corner).with_scale(Vec3::new(
                        0.4,
                        size.y * 0.8,
                        0.4,
                    )),
                ));
            }
        }
        SolidKind::Sidewalk => {
            parent.spawn((
                CityPiece,
                Mesh3d(palette.cube.clone()),
                MeshMaterial3d(palette.sidewalk.clone()),
                Transform::from_translation(center).with_scale(size),
            ));
        }
        SolidKind::Prop | SolidKind::Rooftop | SolidKind::Bridge => {
            let material = if solid.kind == SolidKind::Prop {
                palette.prop.clone()
            } else {
                palette.rooftop.clone()
            };
            parent.spawn((
                CityPiece,
                Mesh3d(palette.cube.clone()),
                MeshMaterial3d(material),
                Transform::from_translation(center).with_scale(size),
            ));
        }
        SolidKind::Barrier => {}
    }
}

/// Builds blocks near the camera (nearest first) and drops far ones.
pub fn stream_city(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    palette: Option<Res<CityPalette>>,
    mut stream: ResMut<CityStream>,
    cameras: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
) {
    let Some(palette) = palette else {
        return;
    };
    let Some((_, eye)) = cameras.iter().find(|(c, _)| c.is_active) else {
        return;
    };
    let eye = eye.translation();
    let (bx, bz) = city::block_of(eye);
    stream.loaded.retain(|&(cx, cz), entity| {
        let keep =
            (cx - bx).abs() <= UNLOAD_RADIUS_BLOCKS && (cz - bz).abs() <= UNLOAD_RADIUS_BLOCKS;
        if !keep {
            commands.entity(*entity).despawn_recursive();
        }
        keep
    });
    let r = STREAM_RADIUS_BLOCKS;
    let mut wanted: Vec<(i32, i32)> = (bx - r..=bx + r)
        .flat_map(|cx| (bz - r..=bz + r).map(move |cz| (cx, cz)))
        .filter(|&(cx, cz)| {
            let d = Vec2::new((cx - bx) as f32, (cz - bz) as f32);
            d.length() <= r as f32 + 0.5 && !stream.loaded.contains_key(&(cx, cz))
        })
        .collect();
    wanted.sort_by_key(|&(cx, cz)| (cx - bx).pow(2) + (cz - bz).pow(2));
    for (cx, cz) in wanted.into_iter().take(BLOCKS_PER_FRAME) {
        let solids = city::block_solids(cx, cz);
        let root = commands
            .spawn((
                Name::new("City Block"),
                CityBlock(cx, cz),
                Transform::default(),
                Visibility::default(),
            ))
            .with_children(|parent| {
                for (i, solid) in solids.iter().enumerate() {
                    spawn_solid(
                        parent,
                        &mut meshes,
                        &palette,
                        solid,
                        i + (cx * 7 + cz * 13).unsigned_abs() as usize,
                    );
                }
            })
            .id();
        stream.loaded.insert((cx, cz), root);
    }
}

/// Streams the city in around whichever 3D camera is active.
pub struct CityPlugin;

impl Plugin for CityPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CityStream>()
            .add_systems(Startup, setup_city_palette)
            .add_systems(Update, stream_city);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::city::BLOCK_PITCH;

    #[test]
    fn facade_uvs_tile_in_metres_and_roofs_stay_dark() {
        let mesh = facade_mesh(Vec3::new(40.0, 54.4, 20.0));
        let Some(bevy::render::mesh::VertexAttributeValues::Float32x2(uvs)) =
            mesh.attribute(Mesh::ATTRIBUTE_UV_0)
        else {
            panic!("uvs");
        };
        assert_eq!(uvs.len(), 20);
        assert!(uvs[..16]
            .iter()
            .any(|uv| (uv[0] - 1.0).abs() < 1e-5 && (uv[1] - 1.0).abs() < 1e-5));
        assert!(uvs[16..].iter().all(|uv| uv[0] < 0.01 && uv[1] < 0.01));
        let texture = window_texture(7, 0.5, 0.5);
        assert_eq!(
            &texture[..3],
            &[0, 0, 0],
            "the corner the roof samples is wall"
        );
    }

    #[test]
    fn streaming_radius_covers_the_fog() {
        assert!(STREAM_RADIUS_BLOCKS as f32 * BLOCK_PITCH >= 600.0);
    }
}
