"""Turns MakeHuman skin textures into neutral skin-detail maps.

The game tints skin per vertex, so it only needs the texture's light and
shade (pores, creases, lips, nails, stubble). Each map stores luminance
normalized so its average is 0.9; the body material multiplies it by the
chosen skin tone.

    blender -b --python tools/character/build_skin_detail.py -- <skins_dir> <output_dir>

``skins_dir`` is the ``skins`` folder of the MakeHuman system assets (CC0).
"""

import os
import sys

import bpy
import numpy as np

SOURCES = {
    "skin_detail_male.png": "young_caucasian_male/young_lightskinned_male_diffuse.png",
    "skin_detail_female.png": "young_caucasian_female/young_lightskinned_female_diffuse.png",
}
TARGET_MEAN = 0.9
# Detail contrast around the mean, and bump strength of the derived normals.
CONTRAST = 1.25
# Gentle bump from a blurred height field: smooth skin that still catches light.
NORMAL_STRENGTH = 0.9
NORMAL_BLUR = 3
NORMAL_MAP = "skin_normal.png"
NORMAL_SIZE = 1024


def build(source, target):
    image = bpy.data.images.load(source)
    w, h = image.size
    pixels = np.array(image.pixels[:], dtype=np.float32).reshape(h, w, 4)
    lum = pixels[..., 0] * 0.299 + pixels[..., 1] * 0.587 + pixels[..., 2] * 0.114
    lum = lum / max(float(lum.mean()), 1e-4) * TARGET_MEAN
    lum = np.clip(TARGET_MEAN + (lum - TARGET_MEAN) * CONTRAST, 0.0, 1.0)
    out = bpy.data.images.new(os.path.basename(target), width=w, height=h, alpha=False)
    out.colorspace_settings.name = "Non-Color"
    rgba = np.empty((h, w, 4), dtype=np.float32)
    rgba[..., 0] = rgba[..., 1] = rgba[..., 2] = lum
    rgba[..., 3] = 1.0
    out.pixels[:] = rgba.ravel()
    out.filepath_raw = target
    out.file_format = "PNG"
    scene = bpy.context.scene
    scene.render.image_settings.file_format = "PNG"
    scene.render.image_settings.color_mode = "BW"
    scene.render.image_settings.color_depth = "8"
    scene.render.image_settings.compression = 100
    out.save_render(target, scene=scene)
    print(f"SKIN {os.path.basename(target)} {w}x{h} mean {float(lum.mean()):.3f}")
    return lum


def save_normals(lum, target):
    """Tangent-space normals from luminance treated as a height field."""
    step = lum.shape[0] // NORMAL_SIZE
    height = lum[::step, ::step].copy()
    for _ in range(NORMAL_BLUR):
        height = (
            height
            + np.roll(height, 1, 0) + np.roll(height, -1, 0)
            + np.roll(height, 1, 1) + np.roll(height, -1, 1)
        ) / 5.0
    dx = np.roll(height, -1, axis=1) - np.roll(height, 1, axis=1)
    dy = np.roll(height, -1, axis=0) - np.roll(height, 1, axis=0)
    n = np.stack([-dx * NORMAL_STRENGTH, -dy * NORMAL_STRENGTH, np.ones_like(height)], axis=-1)
    n /= np.linalg.norm(n, axis=-1, keepdims=True)
    size = height.shape[0]
    out = bpy.data.images.new("normals", width=size, height=size, alpha=False)
    out.colorspace_settings.name = "Non-Color"
    rgba = np.ones((size, size, 4), dtype=np.float32)
    rgba[..., :3] = n * 0.5 + 0.5
    out.pixels[:] = rgba.ravel()
    scene = bpy.context.scene
    scene.render.image_settings.color_mode = "RGB"
    out.save_render(target, scene=scene)
    print(f"SKIN {os.path.basename(target)} {size}x{size}")


def main(skins_dir, out_dir):
    os.makedirs(out_dir, exist_ok=True)
    details = {}
    for name, rel in SOURCES.items():
        details[name] = build(os.path.join(skins_dir, rel), os.path.join(out_dir, name))
    save_normals(details["skin_detail_male.png"], os.path.join(out_dir, NORMAL_MAP))


if __name__ == "__main__":
    args = sys.argv[sys.argv.index("--") + 1:]
    main(args[0], args[1])
