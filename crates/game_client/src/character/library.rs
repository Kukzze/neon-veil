//! The operative's morph library: neutral MakeHuman vertex positions, the
//! sparse delta of every slider end, slider metadata and the vertex sets
//! that place each bone. Built by `tools/character/build_operative.py`.

use super::appearance::CharacterAppearance;
use bevy::prelude::*;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

pub const MORPHS_FILE: &str = "characters/operative_morphs.bin";
pub const RIG_FILE: &str = "characters/operative_rig.json";
pub const BODY_SCENE: &str = "characters/operative.glb#Scene0";

/// One slider end: sparse `(vertex, delta)` pairs, deltas quantized.
#[derive(Debug, Clone, PartialEq)]
pub struct MorphTarget {
    pub name: String,
    pub entries: Vec<(u16, [i16; 3])>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SliderInfo {
    pub id: String,
    pub label: String,
    pub category: String,
    pub low: Option<String>,
    pub high: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct BoneJoints {
    pub parent: Option<String>,
    pub head: Vec<u32>,
    pub tail: Vec<u32>,
}

#[derive(Debug, Deserialize)]
struct RigFile {
    sliders: Vec<SliderInfo>,
    bones: HashMap<String, BoneJoints>,
    #[serde(default)]
    regions: HashMap<String, Vec<u32>>,
}

#[derive(Resource, Debug, Clone)]
pub struct MorphLibrary {
    pub neutral: Vec<Vec3>,
    pub scale: f32,
    pub morphs: Vec<MorphTarget>,
    pub sliders: Vec<SliderInfo>,
    pub bones: HashMap<String, BoneJoints>,
    /// Named vertex sets (scalp, lips, eyes, teeth...).
    pub regions: HashMap<String, Vec<u32>>,
    index: HashMap<String, usize>,
}

#[derive(Debug, thiserror::Error)]
pub enum LibraryError {
    #[error("morph file is truncated or malformed")]
    Malformed,
    #[error("unsupported morph file version {0}")]
    Version(u32),
    #[error("i/o: {0}")]
    Io(#[from] std::io::Error),
    #[error("rig metadata: {0}")]
    Json(#[from] serde_json::Error),
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], LibraryError> {
        let slice = self
            .bytes
            .get(self.at..self.at + n)
            .ok_or(LibraryError::Malformed)?;
        self.at += n;
        Ok(slice)
    }

    fn u16(&mut self) -> Result<u16, LibraryError> {
        Ok(u16::from_le_bytes(
            self.take(2)?.try_into().expect("2 bytes"),
        ))
    }

    fn i16(&mut self) -> Result<i16, LibraryError> {
        Ok(i16::from_le_bytes(
            self.take(2)?.try_into().expect("2 bytes"),
        ))
    }

    fn u32(&mut self) -> Result<u32, LibraryError> {
        Ok(u32::from_le_bytes(
            self.take(4)?.try_into().expect("4 bytes"),
        ))
    }

    fn f32(&mut self) -> Result<f32, LibraryError> {
        Ok(f32::from_le_bytes(
            self.take(4)?.try_into().expect("4 bytes"),
        ))
    }
}

/// Parses `operative_morphs.bin`: neutral positions, scale and targets.
pub fn parse_morphs(bytes: &[u8]) -> Result<(Vec<Vec3>, f32, Vec<MorphTarget>), LibraryError> {
    let mut r = Reader { bytes, at: 0 };
    if r.take(4)? != b"NVMH" {
        return Err(LibraryError::Malformed);
    }
    let version = r.u32()?;
    if version != 1 {
        return Err(LibraryError::Version(version));
    }
    let vertex_count = r.u32()? as usize;
    let morph_count = r.u32()? as usize;
    let scale = r.f32()?;
    if !(scale.is_finite() && scale > 0.0) || vertex_count > u16::MAX as usize + 1 {
        return Err(LibraryError::Malformed);
    }
    let mut neutral = Vec::with_capacity(vertex_count);
    for _ in 0..vertex_count {
        neutral.push(Vec3::new(r.f32()?, r.f32()?, r.f32()?));
    }
    let mut morphs = Vec::with_capacity(morph_count);
    for _ in 0..morph_count {
        let len = r.u16()? as usize;
        let name = String::from_utf8(r.take(len)?.to_vec()).map_err(|_| LibraryError::Malformed)?;
        let count = r.u32()? as usize;
        let mut entries = Vec::with_capacity(count);
        for _ in 0..count {
            let index = r.u16()?;
            if usize::from(index) >= vertex_count {
                return Err(LibraryError::Malformed);
            }
            entries.push((index, [r.i16()?, r.i16()?, r.i16()?]));
        }
        morphs.push(MorphTarget { name, entries });
    }
    Ok((neutral, scale, morphs))
}

impl MorphLibrary {
    pub fn from_parts(morph_bytes: &[u8], rig_json: &str) -> Result<Self, LibraryError> {
        let (neutral, scale, morphs) = parse_morphs(morph_bytes)?;
        let rig: RigFile = serde_json::from_str(rig_json)?;
        let index = morphs
            .iter()
            .enumerate()
            .map(|(i, m)| (m.name.clone(), i))
            .collect();
        Ok(Self {
            neutral,
            scale,
            morphs,
            sliders: rig.sliders,
            bones: rig.bones,
            regions: rig.regions,
            index,
        })
    }

    pub fn load(asset_root: &Path) -> Result<Self, LibraryError> {
        let bytes = std::fs::read(asset_root.join(MORPHS_FILE))?;
        let json = std::fs::read_to_string(asset_root.join(RIG_FILE))?;
        Self::from_parts(&bytes, &json)
    }

    pub fn morph(&self, name: &str) -> Option<&MorphTarget> {
        self.index.get(name).map(|&i| &self.morphs[i])
    }

    /// Every MakeHuman vertex position for `appearance`: sliders run from
    /// -1 (their low end) through 0 (neutral) to 1 (their high end).
    pub fn shape(&self, appearance: &CharacterAppearance) -> Vec<Vec3> {
        let mut out = self.neutral.clone();
        let inv = 1.0 / self.scale;
        for slider in &self.sliders {
            let value = appearance.slider(&slider.id);
            let (morph, weight) = if value < 0.0 {
                (slider.low.as_deref(), -value)
            } else {
                (slider.high.as_deref(), value)
            };
            let Some(target) = morph.and_then(|m| self.morph(m)) else {
                continue;
            };
            if weight <= 1e-4 {
                continue;
            }
            let w = weight * inv;
            for &(i, d) in &target.entries {
                out[usize::from(i)] +=
                    Vec3::new(f32::from(d[0]), f32::from(d[1]), f32::from(d[2])) * w;
            }
        }
        out
    }

    /// Mean of `verts` in `positions`, or `None` for an empty set.
    pub fn joint_position(positions: &[Vec3], verts: &[u32]) -> Option<Vec3> {
        let mut sum = Vec3::ZERO;
        let mut n = 0.0;
        for &v in verts {
            if let Some(p) = positions.get(v as usize) {
                sum += *p;
                n += 1.0;
            }
        }
        (n > 0.0).then(|| sum / n)
    }

    /// Slider categories in display order.
    pub fn categories(&self) -> Vec<&str> {
        let mut out: Vec<&str> = Vec::new();
        for s in &self.sliders {
            if !out.contains(&s.category.as_str()) {
                out.push(&s.category);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tiny_file() -> Vec<u8> {
        let mut b = b"NVMH".to_vec();
        for v in [1u32, 2, 2] {
            b.extend(v.to_le_bytes());
        }
        b.extend(1000.0f32.to_le_bytes());
        for p in [[0.0f32, 1.0, 0.0], [1.0, 0.0, 0.0]] {
            for c in p {
                b.extend(c.to_le_bytes());
            }
        }
        for (name, entries) in [
            ("w-", vec![(1u16, [-100i16, 0, 0])]),
            ("w+", vec![(1, [500, 0, 0])]),
        ] {
            b.extend((name.len() as u16).to_le_bytes());
            b.extend(name.as_bytes());
            b.extend((entries.len() as u32).to_le_bytes());
            for (i, d) in entries {
                b.extend(i.to_le_bytes());
                for c in d {
                    b.extend(c.to_le_bytes());
                }
            }
        }
        b
    }

    const RIG: &str = r#"{"sliders":[{"id":"w","label":"Width","category":"Body","low":"w-","high":"w+"}],
        "bones":{"Root":{"parent":null,"head":[0],"tail":[1]}}}"#;

    #[test]
    fn parses_and_shapes_both_directions() {
        let lib = MorphLibrary::from_parts(&tiny_file(), RIG).expect("valid");
        assert_eq!(lib.neutral.len(), 2);
        let mut look = CharacterAppearance::default();
        look.sliders.clear();
        assert_eq!(lib.shape(&look), lib.neutral, "neutral at zero");
        look.set_slider("w", 1.0);
        assert!((lib.shape(&look)[1].x - 1.5).abs() < 1e-5);
        look.set_slider("w", -0.5);
        assert!((lib.shape(&look)[1].x - 0.95).abs() < 1e-5);
        assert_eq!(lib.categories(), vec!["Body"]);
    }

    #[test]
    fn rejects_malformed_files() {
        assert!(parse_morphs(b"NOPE").is_err());
        let mut bytes = tiny_file();
        bytes.truncate(bytes.len() - 3);
        assert!(parse_morphs(&bytes).is_err());
        let mut bad_index = tiny_file();
        let at = bad_index.len() - 8;
        bad_index[at..at + 2].copy_from_slice(&9u16.to_le_bytes());
        assert!(parse_morphs(&bad_index).is_err());
    }

    #[test]
    fn joint_position_is_the_mean() {
        let pts = [Vec3::ZERO, Vec3::X * 2.0, Vec3::Y * 4.0];
        assert_eq!(MorphLibrary::joint_position(&pts, &[0, 1]), Some(Vec3::X));
        assert_eq!(MorphLibrary::joint_position(&pts, &[]), None);
    }

    #[test]
    fn shipped_library_loads_with_every_slider_backed() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let lib = MorphLibrary::load(&root).expect("shipped character data");
        assert!(lib.sliders.len() >= 80, "{}", lib.sliders.len());
        for s in &lib.sliders {
            for end in [&s.low, &s.high].into_iter().flatten() {
                assert!(lib.morph(end).is_some(), "{} missing {end}", s.id);
            }
        }
        for bone in ["pelvis", "head", "hand_r", "foot_l"] {
            let joints = &lib.bones[bone];
            assert!(MorphLibrary::joint_position(&lib.neutral, &joints.head).is_some());
        }
        // A tall, muscular body differs from the neutral one.
        let mut look = CharacterAppearance::default();
        look.sliders.clear();
        look.set_slider("height", 1.0);
        let tall = lib.shape(&look);
        let top = |p: &[Vec3]| p.iter().map(|v| v.y).fold(f32::MIN, f32::max);
        assert!(top(&tall) > top(&lib.neutral) + 0.05);
    }
}
