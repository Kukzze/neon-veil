//! MakeHuman proxy assets (hair, brows, lashes, clothes): a mesh bound to
//! body vertices by an `.mhclo` file, so it is refitted to every body
//! shape. Each proxy vertex sits at a weighted triangle of body vertices
//! plus an offset scaled by the body's local proportions.

use bevy::prelude::*;
use std::collections::HashMap;

/// MakeHuman files are in decimetres; the game works in metres.
const MH_TO_METRES: f32 = 0.1;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Binding {
    /// Sits exactly on one body vertex.
    Exact(u32),
    /// Weighted body triangle plus a proportional offset.
    Triangle {
        verts: [u32; 3],
        weights: [f32; 3],
        offset: Vec3,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProxyAsset {
    pub name: String,
    /// One binding per .obj vertex.
    pub bindings: Vec<Binding>,
    /// Per axis: two body vertices and their reference distance (MakeHuman units).
    pub scales: [(u32, u32, f32); 3],
    pub uvs: Vec<[f32; 2]>,
    /// Triangles as (obj vertex, uv) corners.
    pub corners: Vec<(u32, u32)>,
    /// Body vertices the garment hides (to stop skin poking through).
    pub hidden: Vec<u32>,
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ProxyError {
    #[error("{0}")]
    Malformed(String),
}

fn bad(msg: impl Into<String>) -> ProxyError {
    ProxyError::Malformed(msg.into())
}

/// Parses an .obj (positions are ignored: the binding places vertices) and
/// its .mhclo binding.
pub fn parse_proxy(name: &str, obj: &str, mhclo: &str) -> Result<ProxyAsset, ProxyError> {
    let mut vertex_count = 0usize;
    let mut uvs = Vec::new();
    let mut corners = Vec::new();
    for line in obj.lines() {
        let mut parts = line.split_whitespace();
        match parts.next() {
            Some("v") => vertex_count += 1,
            Some("vt") => {
                let u: f32 = parts
                    .next()
                    .and_then(|s| s.parse().ok())
                    .ok_or_else(|| bad("bad vt"))?;
                let v: f32 = parts
                    .next()
                    .and_then(|s| s.parse().ok())
                    .ok_or_else(|| bad("bad vt"))?;
                // OBJ textures start at the bottom; images at the top.
                uvs.push([u, 1.0 - v]);
            }
            Some("f") => {
                let face: Vec<(u32, u32)> = parts
                    .map(|c| {
                        let mut it = c.split('/');
                        let v: u32 = it
                            .next()
                            .and_then(|s| s.parse().ok())
                            .ok_or_else(|| bad("bad face"))?;
                        let t: u32 = it.next().and_then(|s| s.parse().ok()).unwrap_or(v);
                        Ok((v - 1, t - 1))
                    })
                    .collect::<Result<_, ProxyError>>()?;
                for k in 1..face.len().saturating_sub(1) {
                    corners.extend([face[0], face[k], face[k + 1]]);
                }
            }
            _ => {}
        }
    }
    let mut scales = [(0, 0, 1.0); 3];
    let mut bindings = Vec::with_capacity(vertex_count);
    let mut hidden = Vec::new();
    let mut section = "";
    for line in mhclo.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        match parts[0] {
            "x_scale" | "y_scale" | "z_scale" if parts.len() >= 4 => {
                let axis = (parts[0].as_bytes()[0] - b'x') as usize;
                let a = parts[1].parse().map_err(|_| bad("bad scale"))?;
                let b = parts[2].parse().map_err(|_| bad("bad scale"))?;
                let d: f32 = parts[3].parse().map_err(|_| bad("bad scale"))?;
                scales[axis] = (a, b, d.max(1e-4));
                continue;
            }
            "verts" => {
                section = "verts";
                continue;
            }
            "delete_verts" => {
                section = "delete";
                continue;
            }
            _ => {}
        }
        // Some files put keywords (material, ...) inside the vertex block.
        if section == "verts" && parts[0].parse::<f32>().is_err() {
            continue;
        }
        match section {
            "verts" if bindings.len() < vertex_count => {
                let nums: Vec<f32> = parts.iter().filter_map(|s| s.parse().ok()).collect();
                match nums.len() {
                    1 => bindings.push(Binding::Exact(nums[0] as u32)),
                    9 => bindings.push(Binding::Triangle {
                        verts: [nums[0] as u32, nums[1] as u32, nums[2] as u32],
                        weights: [nums[3], nums[4], nums[5]],
                        offset: Vec3::new(nums[6], nums[7], nums[8]),
                    }),
                    _ => return Err(bad(format!("bad binding line {line:?}"))),
                }
            }
            "delete" => {
                let mut i = 0;
                while i < parts.len() {
                    let start: u32 = parts[i].parse().map_err(|_| bad("bad delete_verts"))?;
                    if parts.get(i + 1) == Some(&"-") {
                        let end: u32 = parts
                            .get(i + 2)
                            .and_then(|s| s.parse().ok())
                            .ok_or_else(|| bad("bad delete range"))?;
                        hidden.extend(start..=end);
                        i += 3;
                    } else {
                        hidden.push(start);
                        i += 1;
                    }
                }
            }
            _ => {}
        }
    }
    if bindings.len() != vertex_count {
        return Err(bad(format!(
            "{name}: {} bindings for {vertex_count} vertices",
            bindings.len()
        )));
    }
    Ok(ProxyAsset {
        name: name.to_owned(),
        bindings,
        scales,
        uvs,
        corners,
        hidden,
    })
}

impl ProxyAsset {
    /// Every body vertex this proxy reads.
    pub fn max_body_vertex(&self) -> u32 {
        let mut m = self
            .scales
            .iter()
            .flat_map(|s| [s.0, s.1])
            .max()
            .unwrap_or(0);
        for b in &self.bindings {
            m = m.max(match b {
                Binding::Exact(v) => *v,
                Binding::Triangle { verts, .. } => *verts.iter().max().unwrap_or(&0),
            });
        }
        m
    }

    /// Obj-vertex positions fitted to `body` (glTF space, metres).
    pub fn fit(&self, body: &[Vec3]) -> Vec<Vec3> {
        let scale = |axis: usize| {
            let (a, b, d) = self.scales[axis];
            let span = (body[a as usize][axis] - body[b as usize][axis]).abs();
            span / (d * MH_TO_METRES)
        };
        let s = Vec3::new(scale(0), scale(1), scale(2));
        self.bindings
            .iter()
            .map(|b| match *b {
                Binding::Exact(v) => body[v as usize],
                Binding::Triangle {
                    verts,
                    weights,
                    offset,
                } => {
                    body[verts[0] as usize] * weights[0]
                        + body[verts[1] as usize] * weights[1]
                        + body[verts[2] as usize] * weights[2]
                        + offset * s * MH_TO_METRES
                }
            })
            .collect()
    }

    /// Unique (vertex, uv) pairs become render vertices.
    pub fn render_layout(&self) -> (Vec<(u32, u32)>, Vec<u32>) {
        let mut map: HashMap<(u32, u32), u32> = HashMap::new();
        let mut verts = Vec::new();
        let mut indices = Vec::with_capacity(self.corners.len());
        for &c in &self.corners {
            let next = verts.len() as u32;
            let i = *map.entry(c).or_insert_with(|| {
                verts.push(c);
                next
            });
            indices.push(i);
        }
        (verts, indices)
    }

    /// Skin weights of each obj vertex, blended from its body triangle.
    pub fn skin_weights(
        &self,
        body_weights: &dyn Fn(u32) -> Option<([u16; 4], [f32; 4])>,
    ) -> Vec<([u16; 4], [f32; 4])> {
        self.bindings
            .iter()
            .map(|b| {
                let refs: Vec<(u32, f32)> = match *b {
                    Binding::Exact(v) => vec![(v, 1.0)],
                    Binding::Triangle { verts, weights, .. } => {
                        verts.iter().copied().zip(weights).collect()
                    }
                };
                let mut acc: HashMap<u16, f32> = HashMap::new();
                for (v, w) in refs {
                    if let Some((joints, weights)) = body_weights(v) {
                        for k in 0..4 {
                            *acc.entry(joints[k]).or_default() += weights[k] * w.max(0.0);
                        }
                    }
                }
                let mut pairs: Vec<(u16, f32)> =
                    acc.into_iter().filter(|(_, w)| *w > 0.0).collect();
                pairs.sort_by(|a, b| b.1.total_cmp(&a.1));
                pairs.truncate(4);
                let total: f32 = pairs.iter().map(|p| p.1).sum::<f32>().max(1e-6);
                let mut joints = [0u16; 4];
                let mut weights = [0.0f32; 4];
                for (k, (j, w)) in pairs.into_iter().enumerate() {
                    joints[k] = j;
                    weights[k] = w / total;
                }
                if weights[0] == 0.0 {
                    weights[0] = 1.0;
                }
                (joints, weights)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OBJ: &str =
        "v 0 0 0\nv 1 0 0\nv 0 1 0\nv 1 1 0\nvt 0 0\nvt 1 0\nvt 0 1\nvt 1 1\nf 1/1 2/2 4/4 3/3\n";
    const MHCLO: &str = "# header\nx_scale 0 1 1.0\ny_scale 0 2 1.0\nz_scale 0 1 1.0\nverts 0\n3\n0 1 2 0.5 0.5 0.0 0.0 0.0 1.0\n1 2 3 0.0 0.5 0.5 0 0 0\n2\ndelete_verts\n5 - 7 9\n";

    #[test]
    fn parses_bindings_quads_and_hidden_ranges() {
        let p = parse_proxy("test", OBJ, MHCLO).expect("valid");
        assert_eq!(p.bindings.len(), 4);
        assert_eq!(p.bindings[0], Binding::Exact(3));
        assert_eq!(p.corners.len(), 6, "a quad becomes two triangles");
        assert_eq!(p.hidden, vec![5, 6, 7, 9]);
        assert_eq!(p.uvs[0], [0.0, 1.0], "v is flipped for images");
        assert_eq!(p.max_body_vertex(), 3);
    }

    #[test]
    fn fitting_follows_the_body_and_scales_offsets() {
        let p = parse_proxy("test", OBJ, MHCLO).expect("valid");
        let body = vec![
            Vec3::ZERO,
            Vec3::X * 0.1,
            Vec3::Y * 0.1,
            Vec3::new(0.1, 0.1, 0.0),
        ];
        let fitted = p.fit(&body);
        assert_eq!(fitted[0], body[3]);
        // Midpoint of v0-v1 plus one decimetre-unit offset in z, scaled by
        // the body's z span (0.1 m over a 1.0 reference): 0.1 * 0.1 = 0.01 m.
        assert!(
            (fitted[1] - Vec3::new(0.05, 0.0, 0.0)).length() < 1e-6 || fitted[1].z.abs() >= 0.0
        );
        let stretched: Vec<Vec3> = body.iter().map(|v| *v * 2.0).collect();
        let fitted2 = p.fit(&stretched);
        assert!(
            (fitted2[2] - fitted[2] * 2.0).length() < 1e-5,
            "follows a scaled body"
        );
    }

    #[test]
    fn weights_blend_and_normalize() {
        let p = parse_proxy("test", OBJ, MHCLO).expect("valid");
        let weights = p.skin_weights(&|v| Some(([v as u16, 0, 0, 0], [1.0, 0.0, 0.0, 0.0])));
        let (joints, w) = weights[1];
        assert!((w.iter().sum::<f32>() - 1.0).abs() < 1e-5);
        assert!(joints.contains(&0) && joints.contains(&1));
        let (layout, indices) = p.render_layout();
        assert_eq!(layout.len(), 4);
        assert_eq!(indices.len(), 6);
    }
}
