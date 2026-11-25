//! The city: a 10 km x 10 km grid of blocks around the Kuroshio Avenue
//! district, generated on demand from a seed. Everything solid is an
//! axis-aligned box, so the shared movement model (server and client
//! prediction) collides against exactly the same geometry the renderer
//! draws, without either side storing the whole city.
//!
//! Layout uses integer hashing and plain arithmetic only, so every
//! platform builds bit-identical boxes.

use crate::neon_district::{
    generate_district_layout, DistrictLayout, AVENUE_END_Z, AVENUE_START_Z, DISTRICT_SEED,
    FACADE_OFFSET, ROAD_HALF_WIDTH, SIDEWALK_HEIGHT,
};
use crate::procedural_streets::SKYBRIDGE_DECK_THICKNESS;
use bevy::prelude::*;
use std::sync::OnceLock;

/// Half the side of the playable square, metres (the map is 10 km wide).
pub const MAP_HALF: f32 = 5000.0;
/// Distance between block centres: a block plus one road.
pub const BLOCK_PITCH: f32 = 120.0;
/// Road width between blocks.
pub const ROAD_WIDTH: f32 = 22.0;
/// Height of the boundary barrier.
pub const BARRIER_HEIGHT: f32 = 60.0;
const CITY_SEED: u64 = 0x5A45_524F_4349_5459;

/// What a solid box is, for drawing it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SolidKind {
    /// A tower; `tone` picks its facade and window colours.
    Building { tone: u8 },
    /// Raised pavement a block stands on.
    Sidewalk,
    /// Street clutter (most of it vaultable).
    Prop(PropKind),
    /// Rooftop plant: AC units, vents, water tanks.
    Rooftop,
    /// A walkway between towers.
    Bridge,
    /// The edge of the map.
    Barrier,
}

/// Street props, each sized to its model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PropKind {
    TrashBags,
    WasteBin,
    ABoard,
    Fence,
}

impl PropKind {
    pub const ALL: [Self; 4] = [Self::TrashBags, Self::WasteBin, Self::ABoard, Self::Fence];

    /// Width (X), height, depth (Z) in metres, matching the model.
    pub fn size(self) -> Vec3 {
        match self {
            Self::TrashBags => Vec3::new(0.69, 0.9, 0.83),
            Self::WasteBin => Vec3::new(0.62, 1.1, 0.62),
            Self::ABoard => Vec3::new(0.56, 1.0, 0.62),
            Self::Fence => Vec3::new(2.12, 2.4, 0.11),
        }
    }
}

/// An axis-aligned solid box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Solid {
    pub min: Vec3,
    pub max: Vec3,
    pub kind: SolidKind,
}

impl Solid {
    pub fn new(center: Vec3, size: Vec3, kind: SolidKind) -> Self {
        Self {
            min: center - size * 0.5,
            max: center + size * 0.5,
            kind,
        }
    }

    pub fn center(&self) -> Vec3 {
        (self.min + self.max) * 0.5
    }

    pub fn size(&self) -> Vec3 {
        self.max - self.min
    }

    /// Planar distance from `p` to the box footprint (0 inside).
    pub fn planar_distance(&self, p: Vec3) -> f32 {
        let dx = (self.min.x - p.x).max(p.x - self.max.x).max(0.0);
        let dz = (self.min.z - p.z).max(p.z - self.max.z).max(0.0);
        (dx * dx + dz * dz).sqrt()
    }
}

/// SplitMix64: a fast, portable integer hash.
fn mix(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

struct BlockRng(u64);

impl BlockRng {
    fn new(cx: i32, cz: i32) -> Self {
        let key = (u64::from(cx as u32) << 32) | u64::from(cz as u32);
        Self(mix(CITY_SEED ^ mix(key)))
    }

    fn unit(&mut self) -> f32 {
        self.0 = mix(self.0);
        (self.0 >> 40) as f32 / (1u64 << 24) as f32
    }

    fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.unit()
    }

    fn chance(&mut self, p: f32) -> bool {
        self.unit() < p
    }
}

/// Block grid index containing a world position.
pub fn block_of(p: Vec3) -> (i32, i32) {
    (
        (p.x / BLOCK_PITCH).round() as i32,
        (p.z / BLOCK_PITCH).round() as i32,
    )
}

/// Centre of a block.
pub fn block_center(cx: i32, cz: i32) -> Vec3 {
    Vec3::new(cx as f32 * BLOCK_PITCH, 0.0, cz as f32 * BLOCK_PITCH)
}

/// Blocks inside the map edge.
pub fn block_in_map(cx: i32, cz: i32) -> bool {
    let c = block_center(cx, cz);
    let half = (BLOCK_PITCH - ROAD_WIDTH) * 0.5;
    c.x.abs() + half < MAP_HALF - 20.0 && c.z.abs() + half < MAP_HALF - 20.0
}

/// The hand-built district occupies these blocks; the generator leaves
/// them (and the avenue between) to it.
pub fn block_is_district(cx: i32, cz: i32) -> bool {
    let c = block_center(cx, cz);
    let half = BLOCK_PITCH * 0.5;
    let reach = FACADE_OFFSET + 30.0;
    c.x - half < reach
        && c.x + half > -reach
        && c.z - half < AVENUE_START_Z + 20.0
        && c.z + half > AVENUE_END_Z - 20.0
}

/// The operative creator's studio stands in an open plaza here.
pub const STUDIO_CLEARING: Vec3 = Vec3::new(0.0, 0.0, 420.0);
const STUDIO_CLEARING_RADIUS: f32 = 160.0;

/// Blocks kept empty: the district and the studio plaza.
pub fn block_is_reserved(cx: i32, cz: i32) -> bool {
    let c = block_center(cx, cz);
    block_is_district(cx, cz) || c.distance(STUDIO_CLEARING) < STUDIO_CLEARING_RADIUS
}

/// Every solid box of one generated block (empty for district blocks and
/// blocks past the map edge).
pub fn block_solids(cx: i32, cz: i32) -> Vec<Solid> {
    let mut out = Vec::new();
    if !block_in_map(cx, cz) || block_is_reserved(cx, cz) {
        return out;
    }
    let mut rng = BlockRng::new(cx, cz);
    let c = block_center(cx, cz);
    let side = BLOCK_PITCH - ROAD_WIDTH;
    out.push(Solid::new(
        c + Vec3::Y * SIDEWALK_HEIGHT * 0.5,
        Vec3::new(side, SIDEWALK_HEIGHT, side),
        SolidKind::Sidewalk,
    ));
    let floor = SIDEWALK_HEIGHT;
    // Downtown is tall, the outskirts low (rational falloff: no exp).
    let distance = (c.x * c.x + c.z * c.z).sqrt();
    let downtown = 1.0 / (1.0 + distance / 1400.0);
    let plaza = rng.chance(0.08);
    if !plaza {
        // Lots: one tower, two halves or four quadrants.
        let split = rng.unit();
        let lots: Vec<(Vec2, Vec2)> = if split < 0.3 {
            vec![(Vec2::ZERO, Vec2::splat(side - 12.0))]
        } else if split < 0.6 {
            let w = (side - 14.0) * 0.5;
            if rng.chance(0.5) {
                vec![
                    (Vec2::new(-w * 0.5 - 1.0, 0.0), Vec2::new(w, side - 12.0)),
                    (Vec2::new(w * 0.5 + 1.0, 0.0), Vec2::new(w, side - 12.0)),
                ]
            } else {
                vec![
                    (Vec2::new(0.0, -w * 0.5 - 1.0), Vec2::new(side - 12.0, w)),
                    (Vec2::new(0.0, w * 0.5 + 1.0), Vec2::new(side - 12.0, w)),
                ]
            }
        } else {
            let w = (side - 16.0) * 0.5;
            [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)]
                .iter()
                .map(|&(sx, sz)| {
                    (
                        Vec2::new(sx * (w * 0.5 + 2.0), sz * (w * 0.5 + 2.0)),
                        Vec2::splat(w),
                    )
                })
                .collect()
        };
        for (offset, area) in lots {
            let footprint = Vec2::new(
                area.x * rng.range(0.62, 0.96),
                area.y * rng.range(0.62, 0.96),
            );
            let height = if rng.chance(0.2) {
                // Low rooftops: a parkour route above the street.
                rng.range(5.0, 12.0)
            } else {
                rng.range(12.0, 34.0) + downtown * rng.range(20.0, 190.0)
            };
            let base = Vec3::new(c.x + offset.x, floor, c.z + offset.y);
            let tone = (rng.unit() * 6.0) as u8;
            out.push(Solid::new(
                base + Vec3::Y * height * 0.5,
                Vec3::new(footprint.x, height, footprint.y),
                SolidKind::Building { tone },
            ));
            // Rooftop plant to climb over.
            let ac = Vec3::new(
                rng.range(-0.3, 0.3) * footprint.x,
                0.0,
                rng.range(-0.3, 0.3) * footprint.y,
            );
            out.push(Solid::new(
                base + ac + Vec3::Y * (height + 0.9),
                Vec3::new(3.2, 1.8, 2.4),
                SolidKind::Rooftop,
            ));
        }
    }
    // Street clutter along the pavement edge: vaultable boxes.
    let props = 3 + (rng.unit() * 4.0) as usize;
    for _ in 0..props {
        let along = rng.range(-0.45, 0.45) * side;
        let edge = side * 0.5 - 2.2;
        let (x, z) = match (rng.unit() * 4.0) as u8 {
            0 => (along, edge),
            1 => (along, -edge),
            2 => (edge, along),
            _ => (-edge, along),
        };
        let kind = match (rng.unit() * 10.0) as u8 {
            0..=3 => PropKind::TrashBags,
            4..=6 => PropKind::WasteBin,
            7..=8 => PropKind::ABoard,
            _ => PropKind::Fence,
        };
        let size = kind.size();
        out.push(Solid::new(
            Vec3::new(c.x + x, floor + size.y * 0.5, c.z + z),
            size,
            SolidKind::Prop(kind),
        ));
    }
    out
}

/// Solid boxes of the hand-built district (towers, pavements, bridges).
pub fn district_solids() -> &'static [Solid] {
    static SOLIDS: OnceLock<Vec<Solid>> = OnceLock::new();
    SOLIDS.get_or_init(|| solids_from_district(&generate_district_layout(DISTRICT_SEED)))
}

fn solids_from_district(layout: &DistrictLayout) -> Vec<Solid> {
    let mut out = Vec::new();
    for b in &layout.buildings {
        out.push(Solid::new(
            b.base_center + Vec3::Y * b.size.y * 0.5,
            b.size,
            SolidKind::Building { tone: 0 },
        ));
    }
    let length = AVENUE_START_Z - AVENUE_END_Z;
    let mid = (AVENUE_START_Z + AVENUE_END_Z) * 0.5;
    let width = FACADE_OFFSET - ROAD_HALF_WIDTH;
    for side in [-1.0f32, 1.0] {
        out.push(Solid::new(
            Vec3::new(
                side * (ROAD_HALF_WIDTH + width * 0.5),
                SIDEWALK_HEIGHT * 0.5,
                mid,
            ),
            Vec3::new(width, SIDEWALK_HEIGHT, length),
            SolidKind::Sidewalk,
        ));
    }
    for &(start, end, width) in &layout.skybridges {
        let center = (start + end) * 0.5;
        let size = Vec3::new(
            (end.x - start.x).abs().max(width),
            SKYBRIDGE_DECK_THICKNESS,
            (end.z - start.z).abs().max(width),
        );
        out.push(Solid::new(center, size, SolidKind::Bridge));
    }
    out
}

/// The four walls at the edge of the map.
pub fn boundary_solids() -> [Solid; 4] {
    let t = 4.0;
    let long = MAP_HALF * 2.0 + t * 2.0;
    let h = BARRIER_HEIGHT;
    [
        Solid::new(
            Vec3::new(0.0, h * 0.5, MAP_HALF + t * 0.5),
            Vec3::new(long, h, t),
            SolidKind::Barrier,
        ),
        Solid::new(
            Vec3::new(0.0, h * 0.5, -MAP_HALF - t * 0.5),
            Vec3::new(long, h, t),
            SolidKind::Barrier,
        ),
        Solid::new(
            Vec3::new(MAP_HALF + t * 0.5, h * 0.5, 0.0),
            Vec3::new(t, h, long),
            SolidKind::Barrier,
        ),
        Solid::new(
            Vec3::new(-MAP_HALF - t * 0.5, h * 0.5, 0.0),
            Vec3::new(t, h, long),
            SolidKind::Barrier,
        ),
    ]
}

/// Appends every solid whose footprint comes within `radius` of `p`.
pub fn solids_near(p: Vec3, radius: f32, out: &mut Vec<Solid>) {
    let near = |s: &Solid| s.planar_distance(p) <= radius;
    out.extend(district_solids().iter().filter(|s| near(s)).copied());
    let (bx, bz) = block_of(p);
    let reach = (radius / BLOCK_PITCH).ceil() as i32 + 1;
    for cx in bx - reach..=bx + reach {
        for cz in bz - reach..=bz + reach {
            let c = block_center(cx, cz);
            let half = BLOCK_PITCH * 0.5;
            if (p.x - c.x).abs() > half + radius || (p.z - c.z).abs() > half + radius {
                continue;
            }
            out.extend(block_solids(cx, cz).into_iter().filter(&near));
        }
    }
    out.extend(boundary_solids().into_iter().filter(near));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_are_deterministic_and_vary() {
        assert_eq!(block_solids(3, -7), block_solids(3, -7));
        assert_ne!(block_solids(3, -7), block_solids(4, -7));
    }

    #[test]
    fn district_and_far_edge_blocks_are_left_empty() {
        assert!(block_is_district(0, 0));
        assert!(block_is_district(0, -1));
        assert!(block_solids(0, -1).is_empty());
        assert!(!block_solids(5, 5).is_empty());
        assert!(block_solids(60, 0).is_empty(), "past the edge");
    }

    #[test]
    fn generated_towers_stay_off_the_roads() {
        for cx in -4..4 {
            for cz in -4..4 {
                let c = block_center(cx, cz);
                for s in block_solids(cx, cz) {
                    let half = (BLOCK_PITCH - ROAD_WIDTH) * 0.5 + 1e-3;
                    assert!(s.min.x >= c.x - half && s.max.x <= c.x + half, "{s:?}");
                    assert!(s.min.z >= c.z - half && s.max.z <= c.z + half, "{s:?}");
                }
            }
        }
    }

    #[test]
    fn downtown_is_taller_than_the_outskirts() {
        let tallest = |range: std::ops::Range<i32>| {
            let mut best: f32 = 0.0;
            for cx in range.clone() {
                for cz in range.clone() {
                    for s in block_solids(cx, cz) {
                        if matches!(s.kind, SolidKind::Building { .. }) {
                            best = best.max(s.max.y);
                        }
                    }
                }
            }
            best
        };
        assert!(tallest(-5..5) > tallest(30..38));
    }

    #[test]
    fn nearby_query_finds_district_towers_and_the_barrier() {
        let mut out = Vec::new();
        solids_near(Vec3::new(FACADE_OFFSET - 1.0, 0.0, -40.0), 4.0, &mut out);
        assert!(out
            .iter()
            .any(|s| matches!(s.kind, SolidKind::Building { .. })));
        out.clear();
        solids_near(Vec3::new(MAP_HALF - 1.0, 0.0, 0.0), 3.0, &mut out);
        assert!(out.iter().any(|s| s.kind == SolidKind::Barrier));
    }
}
