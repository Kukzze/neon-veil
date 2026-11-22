//! Mip chains for tiling world textures. PNGs load with one level, which
//! shimmers and smears at a distance; this builds the full chain once the
//! image arrives so trilinear, anisotropic sampling works.

use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use std::collections::HashSet;

/// Images still waiting for their mip chain.
#[derive(Resource, Debug, Default)]
pub struct MipmapTargets(pub HashSet<AssetId<Image>>);

/// Repeating, trilinear, anisotropic sampling for tiling surfaces.
pub fn tiling_sampler() -> ImageSampler {
    ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        anisotropy_clamp: 16,
        ..default()
    })
}

fn bytes_per_pixel(format: TextureFormat) -> Option<usize> {
    match format {
        TextureFormat::Rgba8Unorm | TextureFormat::Rgba8UnormSrgb => Some(4),
        TextureFormat::Rg8Unorm => Some(2),
        TextureFormat::R8Unorm => Some(1),
        _ => None,
    }
}

/// Box-filtered mip levels appended after level 0. Returns the level count.
pub fn mip_chain(base: &[u8], width: usize, height: usize, bpp: usize) -> (Vec<u8>, u32) {
    let mut data = base.to_vec();
    let mut levels = 1;
    let (mut w, mut h) = (width, height);
    let mut prev = base.to_vec();
    while w > 1 || h > 1 {
        let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
        let mut next = vec![0u8; nw * nh * bpp];
        for y in 0..nh {
            for x in 0..nw {
                for c in 0..bpp {
                    let mut sum = 0u32;
                    for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                        let sx = (x * 2 + dx).min(w - 1);
                        let sy = (y * 2 + dy).min(h - 1);
                        sum += u32::from(prev[(sy * w + sx) * bpp + c]);
                    }
                    next[(y * nw + x) * bpp + c] = ((sum + 2) / 4) as u8;
                }
            }
        }
        data.extend_from_slice(&next);
        prev = next;
        w = nw;
        h = nh;
        levels += 1;
    }
    (data, levels)
}

pub fn generate_mipmaps(
    mut events: EventReader<AssetEvent<Image>>,
    mut targets: ResMut<MipmapTargets>,
    mut images: ResMut<Assets<Image>>,
) {
    for event in events.read() {
        let AssetEvent::LoadedWithDependencies { id } = event else {
            continue;
        };
        if !targets.0.remove(id) {
            continue;
        }
        let Some(image) = images.get_mut(*id) else {
            continue;
        };
        let desc = &image.texture_descriptor;
        let Some(bpp) = bytes_per_pixel(desc.format) else {
            continue;
        };
        if desc.mip_level_count > 1 {
            continue;
        }
        let (w, h) = (desc.size.width as usize, desc.size.height as usize);
        if image.data.len() != w * h * bpp {
            continue;
        }
        let (data, levels) = mip_chain(&image.data, w, h, bpp);
        image.data = data;
        image.texture_descriptor.mip_level_count = levels;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chain_halves_to_one_pixel_and_averages() {
        let base = vec![0u8, 255, 255, 0];
        let (data, levels) = mip_chain(&base, 2, 2, 1);
        assert_eq!(levels, 2);
        assert_eq!(data.len(), 5);
        assert_eq!(data[4], 128);
        let (data, levels) = mip_chain(&[10u8; 8 * 4 * 4], 8, 4, 4);
        assert_eq!(levels, 4, "8x4 -> 4x2 -> 2x1 -> 1x1");
        assert_eq!(data.len(), (32 + 8 + 2 + 1) * 4);
        assert!(data.iter().all(|&b| b == 10));
    }
}
