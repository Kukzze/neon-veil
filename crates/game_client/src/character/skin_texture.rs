//! Per-character albedo: skin detail tinted by the chosen tone, with
//! garments painted in texture space so their edges stay crisp no matter
//! how coarse the mesh is.
//!
//! Garment shapes are analytic functions of the neutral body, evaluated
//! once per texel by rasterizing every body triangle into the UV atlas.

use super::appearance::{CharacterAppearance, Outfit};
use super::library::MorphLibrary;
use bevy::prelude::*;

pub const ATLAS_SIZE: usize = 2048;
/// Body vertices (MakeHuman indices below this) carry the skin atlas.
const BODY_VERTEX_COUNT: u32 = 13_380;
/// Garment edges fade over this distance, metres.
const EDGE: f32 = 0.0025;

/// Landmarks the garments are fitted to, on the neutral body.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GarmentFit {
    pub crotch: f32,
    pub waist: f32,
    pub nipples: [Vec3; 2],
    /// Eye centres, for painting brows.
    pub eyes: [Vec3; 2],
}

impl GarmentFit {
    pub fn from_library(library: &MorphLibrary) -> Option<Self> {
        let neutral = &library.neutral;
        let crotch = neutral[..BODY_VERTEX_COUNT as usize]
            .iter()
            .filter(|p| p.x.abs() < 0.008 && p.y > 0.6 && p.y < 1.05)
            .map(|p| p.y)
            .fold(f32::MAX, f32::min);
        let nipple = |bone: &str| {
            library
                .bones
                .get(bone)
                .and_then(|b| MorphLibrary::joint_position(neutral, &b.tail))
        };
        let eye = |name: &str| {
            library
                .regions
                .get(name)
                .and_then(|list| MorphLibrary::joint_position(neutral, list))
        };
        Some(Self {
            crotch: crotch.is_finite().then_some(crotch)?,
            eyes: [eye("helper-l-eye")?, eye("helper-r-eye")?],
            waist: crotch + 0.155,
            nipples: [nipple("breast_l")?, nipple("breast_r")?],
        })
    }
}

fn inside(distance_inside: f32) -> f32 {
    (distance_inside / EDGE * 0.5 + 0.5).clamp(0.0, 1.0)
}

/// Coverage (0..1) of briefs, bra and trim at a neutral body point.
pub fn garment_at(p: Vec3, fit: &GarmentFit) -> [f32; 3] {
    let side = p.x.abs();
    let opening = fit.crotch - 0.012 + ((side - 0.035) / 0.11).clamp(0.0, 1.0) * 0.085;
    let in_briefs = inside((p.y - opening).min(fit.waist - p.y)).min(inside(0.2 - side));
    let briefs_trim = in_briefs * inside(p.y - (fit.waist - 0.016));
    let mut cup: f32 = 0.0;
    let mut bra_trim: f32 = 0.0;
    for n in fit.nipples {
        let under = n.y - 0.072;
        let cup_here = inside(0.08 - p.distance(n))
            .min(inside(p.z - (n.z - 0.06)))
            .min(inside(n.y + 0.055 - p.y));
        let band =
            inside(0.19 - side).min(inside(p.y - (under - 0.024)).min(inside(under + 0.004 - p.y)));
        // Straps run over the shoulder from the cup top (front) and down to
        // the band (back); in x they drift from the cup toward the neck.
        let top = n.y + 0.05;
        let peak = n.y + 0.27;
        let t = ((p.y - top) / (peak - top)).clamp(0.0, 1.0);
        let strap_x = n.x + (n.x.signum() * 0.075 - n.x) * t;
        let front = p.z > n.z - 0.07;
        let span = if front {
            inside(p.y - top)
        } else {
            inside(p.y - under)
        };
        let strap = inside(0.0085 - (p.x - strap_x).abs())
            .min(span)
            .min(inside(peak - p.y));
        // A thin binding along the top of the cup.
        let binding = cup_here * inside(p.y - (n.y + 0.045));
        cup = cup.max(cup_here);
        bra_trim = bra_trim.max(band).max(strap).max(binding);
    }
    [in_briefs, cup.max(bra_trim), briefs_trim.max(bra_trim)]
}

/// Eyebrow coverage: a tapered arch above each eye on the brow ridge.
pub fn brow_at(p: Vec3, fit: &GarmentFit) -> f32 {
    let mut best: f32 = 0.0;
    for eye in fit.eyes {
        // Outward from the nose is +1 along x for this eye.
        let out = (p.x - eye.x) * eye.x.signum();
        let t = (out + 0.012) / 0.042;
        if !(0.0..=1.0).contains(&t) || p.z < eye.z - 0.005 {
            continue;
        }
        let centre = eye.y + 0.0125 + 0.005 * (1.0 - (2.0 * t - 0.7).powi(2)).max(0.0) - 0.004 * t;
        let half = 0.0068 * (1.0 - 0.55 * t);
        let along = inside(t * 0.04).min(inside((1.0 - t) * 0.03));
        best = best.max(inside(half - (p.y - centre).abs()).min(along));
    }
    best
}

/// Garment and suit coverage for every texel of the atlas.
#[derive(Debug, Clone)]
pub struct GarmentAtlas {
    pub briefs: Vec<u8>,
    pub bra: Vec<u8>,
    pub trim: Vec<u8>,
    pub briefs_trim: Vec<u8>,
    pub brows: Vec<u8>,
    /// 255 where a body triangle owns the texel (eyes, teeth and the
    /// like keep a neutral texture).
    pub body: Vec<u8>,
    /// Skin amount under the full-body suit (255 bare skin).
    pub suit_skin: Vec<u8>,
}

/// Rasterizes body triangles in UV space; each texel samples the garment
/// functions at its interpolated neutral position.
#[allow(clippy::too_many_arguments)]
pub fn build_atlas(
    size: usize,
    uvs: &[[f32; 2]],
    indices: &[u32],
    vids: &[u32],
    neutral: &[Vec3],
    skin_blend: &[f32],
    fit: &GarmentFit,
) -> GarmentAtlas {
    let n = size * size;
    let mut atlas = GarmentAtlas {
        briefs: vec![0; n],
        bra: vec![0; n],
        trim: vec![0; n],
        briefs_trim: vec![0; n],
        brows: vec![0; n],
        body: vec![0; n],
        suit_skin: vec![255; n],
    };
    let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    for tri in indices.chunks_exact(3) {
        let [a, b, c] = [tri[0] as usize, tri[1] as usize, tri[2] as usize];
        if [a, b, c].iter().any(|&k| vids[k] >= BODY_VERTEX_COUNT) {
            continue;
        }
        let uv = |k: usize| Vec2::new(uvs[k][0], uvs[k][1]) * size as f32;
        let (ua, ub, uc) = (uv(a), uv(b), uv(c));
        let area = (ub - ua).perp_dot(uc - ua);
        if area.abs() < 1e-9 {
            continue;
        }
        let min = ua.min(ub).min(uc).floor().max(Vec2::ZERO);
        let max = ua
            .max(ub)
            .max(uc)
            .ceil()
            .min(Vec2::splat(size as f32 - 1.0));
        let (pa, pb, pc) = (
            neutral[vids[a] as usize],
            neutral[vids[b] as usize],
            neutral[vids[c] as usize],
        );
        for y in min.y as usize..=max.y as usize {
            for x in min.x as usize..=max.x as usize {
                let q = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
                let w0 = (ub - q).perp_dot(uc - q) / area;
                let w1 = (uc - q).perp_dot(ua - q) / area;
                let w2 = 1.0 - w0 - w1;
                // A little slack fills cracks between neighbouring triangles.
                if w0 < -0.02 || w1 < -0.02 || w2 < -0.02 {
                    continue;
                }
                let p = pa * w0 + pb * w1 + pc * w2;
                let [briefs, bra, trim] = garment_at(p, fit);
                let i = y * size + x;
                atlas.briefs[i] = byte(briefs);
                atlas.bra[i] = byte(bra);
                atlas.trim[i] = byte(trim);
                atlas.briefs_trim[i] = byte(briefs * inside(p.y - (fit.waist - 0.016)));
                atlas.brows[i] = byte(brow_at(p, fit));
                atlas.body[i] = 255;
                atlas.suit_skin[i] =
                    byte(skin_blend[a] * w0 + skin_blend[b] * w1 + skin_blend[c] * w2);
            }
        }
    }
    atlas
}

/// Linear to sRGB byte, through a lookup table.
pub struct SrgbTable([u8; 4096]);

impl Default for SrgbTable {
    fn default() -> Self {
        let mut t = [0u8; 4096];
        for (i, v) in t.iter_mut().enumerate() {
            let l = i as f32 / 4095.0;
            let s = if l <= 0.003_130_8 {
                l * 12.92
            } else {
                1.055 * l.powf(1.0 / 2.4) - 0.055
            };
            *v = (s.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
        }
        Self(t)
    }
}

impl SrgbTable {
    fn encode(&self, linear: f32) -> u8 {
        self.0[(linear.clamp(0.0, 1.0) * 4095.0) as usize]
    }
}

fn linear(c: [f32; 3]) -> Vec3 {
    let l = Color::srgb(c[0], c[1], c[2]).to_linear();
    Vec3::new(l.red, l.green, l.blue)
}

/// Composes the RGBA8 sRGB albedo for `look` from a grayscale skin
/// detail map (same size as the atlas, average about 0.9).
pub fn compose_albedo(
    atlas: &GarmentAtlas,
    detail: &[u8],
    look: &CharacterAppearance,
    table: &SrgbTable,
) -> Vec<u8> {
    let tone = linear(look.skin_tone) / 0.9;
    let cloth = linear(look.suit);
    let trim = linear(look.accent) * 0.55 + Vec3::splat(0.02);
    let feminine = look.slider("gender") > 0.15;
    let brow = linear(look.hair_color) * 0.4;
    let mut out = vec![0u8; detail.len() * 4];
    for (i, &d) in detail.iter().enumerate() {
        let d = f32::from(d) / 255.0;
        let skin = tone * d;
        let fabric = cloth * (0.82 + 0.2 * d);
        let c = match look.outfit {
            Outfit::OperativeSuit => fabric.lerp(skin, f32::from(atlas.suit_skin[i]) / 255.0),
            Outfit::Underwear => {
                let (cover, edge) = if feminine {
                    (atlas.briefs[i].max(atlas.bra[i]), atlas.trim[i])
                } else {
                    (atlas.briefs[i], atlas.briefs_trim[i])
                };
                skin.lerp(fabric, f32::from(cover) / 255.0)
                    .lerp(trim, f32::from(edge) / 255.0)
            }
        };
        // Brows sit over skin and are never covered.
        let brow_amount = if look.brow_mesh.is_empty() {
            f32::from(atlas.brows[i]) / 255.0
        } else {
            0.0
        };
        let c = c.lerp(brow * (0.6 + 0.4 * d), brow_amount);
        // Eyes, teeth and tongue take their colour from the vertices.
        let c = if atlas.body[i] == 0 {
            Vec3::splat(d / 0.9)
        } else {
            c
        };
        let o = i * 4;
        out[o] = table.encode(c.x);
        out[o + 1] = table.encode(c.y);
        out[o + 2] = table.encode(c.z);
        out[o + 3] = 255;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fit() -> GarmentFit {
        GarmentFit {
            crotch: 0.8,
            waist: 0.955,
            nipples: [Vec3::new(0.09, 1.25, 0.12), Vec3::new(-0.09, 1.25, 0.12)],
            eyes: [Vec3::new(0.032, 1.55, 0.09), Vec3::new(-0.032, 1.55, 0.09)],
        }
    }

    #[test]
    fn garments_cover_the_right_places() {
        let f = fit();
        assert!(
            garment_at(Vec3::new(0.0, 0.85, 0.1), &f)[0] > 0.99,
            "briefs at the hips"
        );
        assert!(
            garment_at(Vec3::new(0.0, 1.1, 0.1), &f)[0] < 0.01,
            "bare stomach"
        );
        assert!(
            garment_at(Vec3::new(0.09, 1.25, 0.13), &f)[1] > 0.99,
            "cup over the chest"
        );
        assert!(
            garment_at(Vec3::new(0.0, 1.5, 0.05), &f)[1] < 0.01,
            "bare neck"
        );
        let edge = garment_at(Vec3::new(0.0, 0.955, 0.1), &f)[0];
        assert!(edge > 0.2 && edge < 0.8, "soft edge at the waistband");
    }

    #[test]
    fn atlas_rasterizes_a_triangle_and_albedo_tints_skin() {
        let uvs = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]];
        let neutral = vec![Vec3::new(0.0, 0.85, 0.1); 3];
        let atlas = build_atlas(8, &uvs, &[0, 1, 2], &[0, 1, 2], &neutral, &[1.0; 3], &fit());
        assert!(atlas.briefs[8 + 1] > 250);
        let look = CharacterAppearance {
            outfit: Outfit::OperativeSuit,
            ..default()
        };
        let detail = vec![230u8; 64];
        let rgba = compose_albedo(&atlas, &detail, &look, &SrgbTable::default());
        assert_eq!(rgba.len(), 256);
        assert_eq!(rgba[3], 255);
    }
}
