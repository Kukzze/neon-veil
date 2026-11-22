"""Packs MakeHuman proxy assets (hair, eyebrows, eyelashes, clothes) for the
game: copies each mesh (.obj) and its body binding (.mhclo), and rewrites
textures. Hair, brows and lashes become tintable luminance + alpha maps;
clothes keep their colour. Textures are downscaled to keep the repo small.

    blender -b --factory-startup --python tools/character/build_proxies.py -- <assets_dir> <output_dir>

``assets_dir`` holds the ``hair``, ``eyebrows``, ``eyelashes`` and
``clothes`` folders of the MakeHuman system assets (CC0).
"""

import os
import shutil
import sys

import bpy
import numpy as np

# category -> (source folder, texture size, tintable, asset names or None for all)
CATEGORIES = {
    "hair": ("hair", 1024, True, None),
    "brows": ("eyebrows", 256, True, None),
    "lashes": ("eyelashes", 256, True, ["eyelashes01", "eyelashes02"]),
    "clothes": ("clothes", 1024, False, [
        "male_casualsuit01", "male_casualsuit04", "male_worksuit01",
        "female_casualsuit01", "female_sportsuit01", "female_elegantsuit01",
        "shoes01", "shoes02", "shoes06",
    ]),
}
TINT_MEAN = 0.75


def diffuse_texture(folder, name):
    """The diffuse texture named by the asset's .mhmat, if any."""
    mhmat = os.path.join(folder, name + ".mhmat")
    if os.path.exists(mhmat):
        with open(mhmat, encoding="utf-8", errors="replace") as f:
            for line in f:
                parts = line.split()
                if len(parts) >= 2 and parts[0] == "diffuseTexture":
                    path = os.path.join(folder, os.path.basename(parts[1]))
                    if os.path.exists(path):
                        return path
    pngs = [p for p in os.listdir(folder) if p.endswith(".png") and "normal" not in p]
    return os.path.join(folder, pngs[0]) if pngs else None


def save(pixels, size, target):
    out = bpy.data.images.new("out", width=size, height=size, alpha=True)
    out.colorspace_settings.name = "sRGB"
    out.pixels[:] = pixels.ravel()
    scene = bpy.context.scene
    scene.render.image_settings.file_format = "PNG"
    scene.render.image_settings.color_mode = "RGBA"
    scene.render.image_settings.color_depth = "8"
    scene.render.image_settings.compression = 100
    out.save_render(target, scene=scene)
    bpy.data.images.remove(out)


def convert(source, target, size, tintable):
    image = bpy.data.images.load(source)
    image.scale(size, size)
    px = np.array(image.pixels[:], dtype=np.float32).reshape(size, size, 4)
    bpy.data.images.remove(image)
    if tintable:
        lum = px[..., 0] * 0.299 + px[..., 1] * 0.587 + px[..., 2] * 0.114
        alpha = px[..., 3]
        visible = alpha > 0.5
        mean = float(lum[visible].mean()) if visible.any() else 0.5
        lum = np.clip(lum / max(mean, 1e-3) * TINT_MEAN, 0.0, 1.0)
        px[..., 0] = px[..., 1] = px[..., 2] = lum
    save(px, size, target)


def main(src_root, out_root):
    for category, (folder, size, tintable, names) in CATEGORIES.items():
        src_dir = os.path.join(src_root, folder)
        for name in names or sorted(os.listdir(src_dir)):
            src = os.path.join(src_dir, name)
            if not os.path.isdir(src):
                continue
            dst = os.path.join(out_root, category, name)
            os.makedirs(dst, exist_ok=True)
            for ext in (".obj", ".mhclo"):
                shutil.copyfile(os.path.join(src, name + ext), os.path.join(dst, name + ext))
            texture = diffuse_texture(src, name)
            if texture:
                convert(texture, os.path.join(dst, "texture.png"), size, tintable)
            print(f"PROXY {category}/{name}")


if __name__ == "__main__":
    args = sys.argv[sys.argv.index("--") + 1:]
    main(args[0], args[1])
