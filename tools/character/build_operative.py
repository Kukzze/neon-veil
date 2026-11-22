"""Builds the customizable operative from MakeHuman (MPFB) data.

Run headless with the MPFB extension enabled in the Blender profile:

    blender -b --python tools/character/build_operative.py -- <output_dir>

Outputs, all in glTF space (Y up, metres):

* ``operative.glb``: the neutral body, skinned to the game_engine rig,
  with a ``_VID`` vertex attribute holding each vertex's MakeHuman index.
* ``operative_morphs.bin``: neutral positions of every MakeHuman vertex
  (helpers included, for joint refitting) and sparse per-slider deltas.
* ``operative_rig.json``: slider metadata and, per bone, the vertices whose
  mean places its head and tail.

The MakeHuman base mesh and targets are CC0.
"""

import gzip
import json
import os
import struct
import sys

import bpy

MPFB = "bl_ext.user_default.mpfb"
HumanService = __import__(f"{MPFB}.services.humanservice", fromlist=["HumanService"]).HumanService
TargetService = __import__(f"{MPFB}.services.targetservice", fromlist=["TargetService"]).TargetService
HumanObjectProperties = __import__(
    f"{MPFB}.entities.objectproperties", fromlist=["HumanObjectProperties"]
).HumanObjectProperties

MPFB_DIR = os.path.dirname(sys.modules[MPFB].__file__)
TARGETS = os.path.join(MPFB_DIR, "data", "targets")
RIG_JSON = os.path.join(MPFB_DIR, "data", "rigs", "standard", "rig.game_engine_with_breast.json")
GROUPS_JSON = os.path.join(MPFB_DIR, "data", "mesh_metadata", "basemesh_vertex_groups.json")

# Helper groups removed from the rendered mesh.
DROPPED_GROUPS = ("JointCubes", "helper-genital", "helper-skirt", "helper-tights", "helper-hair")

# Quantization of deltas: 0.1 mm steps, +-3.2 m range.
DELTA_SCALE = 10000.0

# Macro sliders: (id, label, category, property, low, high).
MACROS = [
    # MakeHuman gender runs 0 = female to 1 = male; the slider reads masculine
    # (-1) to feminine (+1).
    ("gender", "Masculine / Feminine", "Body", "gender", 1.0, 0.0),
    ("age", "Age", "Body", "age", 0.39, 0.8),
    ("muscle", "Muscle", "Body", "muscle", 0.0, 1.0),
    ("weight", "Weight", "Body", "weight", 0.0, 1.0),
    ("height", "Height", "Body", "height", 0.0, 1.0),
    ("proportions", "Proportions", "Body", "proportions", 0.0, 1.0),
    ("cupsize", "Chest size", "Chest", "cupsize", 0.0, 1.0),
    ("firmness", "Chest firmness", "Chest", "firmness", 0.0, 1.0),
]

# Detail sliders: (id, label, category, low target, high target). Targets
# are paths under data/targets without extension; a "{s}" is expanded to
# both "l" and "r" so paired limbs move together.
DETAILS = [
    ("torso_width", "Torso width", "Torso", "torso/torso-scale-horiz-decr", "torso/torso-scale-horiz-incr"),
    ("torso_depth", "Torso depth", "Torso", "torso/torso-scale-depth-decr", "torso/torso-scale-depth-incr"),
    ("torso_length", "Torso length", "Torso", "torso/torso-scale-vert-decr", "torso/torso-scale-vert-incr"),
    ("torso_vshape", "V-shape", "Torso", "torso/torso-vshape-decr", "torso/torso-vshape-incr"),
    ("pectorals", "Pectorals", "Torso", "torso/torso-muscle-pectoral-decr", "torso/torso-muscle-pectoral-incr"),
    ("lats", "Back width", "Torso", "torso/torso-muscle-dorsi-decr", "torso/torso-muscle-dorsi-incr"),
    ("shoulders", "Shoulder width", "Torso", "torso/measure-shoulder-dist-decr", "torso/measure-shoulder-dist-incr"),
    ("bust", "Bust", "Chest", "torso/measure-bust-circ-decr", "torso/measure-bust-circ-incr"),
    ("underbust", "Ribcage", "Chest", "torso/measure-underbust-circ-decr", "torso/measure-underbust-circ-incr"),
    ("chest_lift", "Chest lift", "Chest", "breast/breast-volume-vert-down", "breast/breast-volume-vert-up"),
    ("chest_spacing", "Chest spacing", "Chest", "breast/breast-dist-decr", "breast/breast-dist-incr"),
    ("chest_projection", "Chest projection", "Chest", "breast/breast-point-decr", "breast/breast-point-incr"),
    ("waist", "Waist", "Torso", "torso/measure-waist-circ-decr", "torso/measure-waist-circ-incr"),
    ("stomach", "Stomach", "Torso", "stomach/stomach-tone-decr", "stomach/stomach-tone-incr"),
    ("hips", "Hips", "Torso", "torso/measure-hips-circ-decr", "torso/measure-hips-circ-incr"),
    ("hip_width", "Hip width", "Torso", "hip/hip-scale-horiz-decr", "hip/hip-scale-horiz-incr"),
    ("glutes", "Glutes", "Torso", "buttocks/buttocks-volume-decr", "buttocks/buttocks-volume-incr"),
    ("neck_width", "Neck thickness", "Torso", "neck/neck-scale-horiz-decr", "neck/neck-scale-horiz-incr"),
    ("neck_length", "Neck length", "Torso", "neck/measure-neck-height-decr", "neck/measure-neck-height-incr"),
    ("upperarm_muscle", "Biceps", "Arms", "arms/{s}-upperarm-muscle-decr", "arms/{s}-upperarm-muscle-incr"),
    ("upperarm_fat", "Upper arm fat", "Arms", "arms/{s}-upperarm-fat-decr", "arms/{s}-upperarm-fat-incr"),
    ("deltoids", "Deltoids", "Arms", "arms/{s}-upperarm-shoulder-muscle-decr", "arms/{s}-upperarm-shoulder-muscle-incr"),
    ("forearm_muscle", "Forearms", "Arms", "arms/{s}-lowerarm-muscle-decr", "arms/{s}-lowerarm-muscle-incr"),
    ("upperarm_length", "Upper arm length", "Arms", "arms/measure-upperarm-length-decr", "arms/measure-upperarm-length-incr"),
    ("forearm_length", "Forearm length", "Arms", "arms/measure-lowerarm-length-decr", "arms/measure-lowerarm-length-incr"),
    ("hand_size", "Hand size", "Hands", "hands/{s}-hand-scale-decr", "hands/{s}-hand-scale-incr"),
    ("finger_length", "Finger length", "Hands", "hands/{s}-hand-fingers-length-decr", "hands/{s}-hand-fingers-length-incr"),
    ("finger_width", "Finger width", "Hands", "hands/{s}-hand-fingers-diameter-decr", "hands/{s}-hand-fingers-diameter-incr"),
    ("wrist", "Wrists", "Hands", "hands/measure-wrist-circ-decr", "hands/measure-wrist-circ-incr"),
    ("thigh_muscle", "Thigh muscle", "Legs", "legs/{s}-upperleg-muscle-decr", "legs/{s}-upperleg-muscle-incr"),
    ("thigh_fat", "Thigh fat", "Legs", "legs/{s}-upperleg-fat-decr", "legs/{s}-upperleg-fat-incr"),
    ("thigh_girth", "Thigh girth", "Legs", "legs/measure-thigh-circ-decr", "legs/measure-thigh-circ-incr"),
    ("calf_muscle", "Calves", "Legs", "legs/{s}-lowerleg-muscle-decr", "legs/{s}-lowerleg-muscle-incr"),
    ("upperleg_length", "Thigh length", "Legs", "legs/measure-upperleg-height-decr", "legs/measure-upperleg-height-incr"),
    ("lowerleg_length", "Shin length", "Legs", "legs/measure-lowerleg-height-decr", "legs/measure-lowerleg-height-incr"),
    ("foot_size", "Foot size", "Legs", "feet/{s}-foot-scale-decr", "feet/{s}-foot-scale-incr"),
    ("head_width", "Head width", "Head", "head/head-scale-horiz-decr", "head/head-scale-horiz-incr"),
    ("head_height", "Head height", "Head", "head/head-scale-vert-decr", "head/head-scale-vert-incr"),
    ("head_fat", "Face fullness", "Head", "head/head-fat-decr", "head/head-fat-incr"),
    ("head_oval", "Oval face", "Head", None, "head/head-oval"),
    ("head_square", "Square face", "Head", None, "head/head-square"),
    ("head_round", "Round face", "Head", None, "head/head-round"),
    ("jaw_width", "Jaw width", "Jaw", "chin/chin-width-decr", "chin/chin-width-incr"),
    ("jaw_bones", "Jaw definition", "Jaw", "chin/chin-bones-decr", "chin/chin-bones-incr"),
    ("chin_height", "Chin height", "Jaw", "chin/chin-height-decr", "chin/chin-height-incr"),
    ("chin_prominence", "Chin prominence", "Jaw", "chin/chin-prominent-decr", "chin/chin-prominent-incr"),
    ("chin_cleft", "Chin cleft", "Jaw", "chin/chin-cleft-decr", "chin/chin-cleft-incr"),
    ("cheekbones", "Cheekbones", "Face", "cheek/{s}-cheek-bones-decr", "cheek/{s}-cheek-bones-incr"),
    ("cheek_volume", "Cheek volume", "Face", "cheek/{s}-cheek-volume-decr", "cheek/{s}-cheek-volume-incr"),
    ("forehead_height", "Forehead height", "Face", "forehead/forehead-scale-vert-decr", "forehead/forehead-scale-vert-incr"),
    ("temples", "Temples", "Face", "forehead/forehead-temple-decr", "forehead/forehead-temple-incr"),
    ("brow_angle", "Brow angle", "Eyes", "eyebrows/eyebrows-angle-down", "eyebrows/eyebrows-angle-up"),
    ("brow_height", "Brow height", "Eyes", "eyebrows/eyebrows-trans-down", "eyebrows/eyebrows-trans-up"),
    ("eye_size", "Eye size", "Eyes", "eyes/{s}-eye-scale-decr", "eyes/{s}-eye-scale-incr"),
    ("eye_height", "Eye height", "Eyes", "eyes/{s}-eye-trans-down", "eyes/{s}-eye-trans-up"),
    ("eye_spacing", "Eye spacing", "Eyes", "eyes/{s}-eye-trans-in", "eyes/{s}-eye-trans-out"),
    ("eye_tilt", "Eye tilt", "Eyes", "eyes/{s}-eye-corner1-down", "eyes/{s}-eye-corner1-up"),
    ("eye_bags", "Eye bags", "Eyes", "eyes/{s}-eye-bag-decr", "eyes/{s}-eye-bag-incr"),
    ("nose_width", "Nose width", "Nose", "nose/nose-scale-horiz-decr", "nose/nose-scale-horiz-incr"),
    ("nose_length", "Nose length", "Nose", "nose/nose-scale-vert-decr", "nose/nose-scale-vert-incr"),
    ("nose_depth", "Nose depth", "Nose", "nose/nose-scale-depth-decr", "nose/nose-scale-depth-incr"),
    ("nose_bridge", "Nose bridge", "Nose", "nose/nose-hump-decr", "nose/nose-hump-incr"),
    ("nose_tip", "Nose tip width", "Nose", "nose/nose-point-width-decr", "nose/nose-point-width-incr"),
    ("nose_tip_angle", "Nose tip angle", "Nose", "nose/nose-point-down", "nose/nose-point-up"),
    ("nostrils", "Nostril width", "Nose", "nose/nose-nostrils-width-decr", "nose/nose-nostrils-width-incr"),
    ("nose_curve", "Nose curve", "Nose", "nose/nose-curve-concave", "nose/nose-curve-convex"),
    ("mouth_width", "Mouth width", "Mouth", "mouth/mouth-scale-horiz-decr", "mouth/mouth-scale-horiz-incr"),
    ("upper_lip", "Upper lip", "Mouth", "mouth/mouth-upperlip-volume-decr", "mouth/mouth-upperlip-volume-incr"),
    ("lower_lip", "Lower lip", "Mouth", "mouth/mouth-lowerlip-volume-decr", "mouth/mouth-lowerlip-volume-incr"),
    ("mouth_corners", "Mouth corners", "Mouth", "mouth/mouth-angles-down", "mouth/mouth-angles-up"),
    ("cupids_bow", "Cupid's bow", "Mouth", "mouth/mouth-cupidsbow-decr", "mouth/mouth-cupidsbow-incr"),
    ("ear_size", "Ear size", "Ears", "ears/{s}-ear-scale-decr", "ears/{s}-ear-scale-incr"),
    ("ear_angle", "Ear angle", "Ears", "ears/{s}-ear-rot-backward", "ears/{s}-ear-rot-forward"),
    ("ear_lobes", "Ear lobes", "Ears", "ears/{s}-ear-lobe-decr", "ears/{s}-ear-lobe-incr"),
    ("ear_point", "Pointed ears", "Ears", None, "ears/{s}-ear-shape-pointed"),
]


def to_gltf(co):
    """Blender Z-up to glTF Y-up."""
    return (co[0], co[2], -co[1])


def mix_positions(basemesh):
    key = basemesh.shape_key_add(name="__mix", from_mix=True)
    coords = [to_gltf(p.co) for p in key.data]
    basemesh.shape_key_remove(key)
    return coords


def target_deltas(basemesh, pattern):
    """Deltas (glTF space) of one target, both sides when paired."""
    paths = [pattern.replace("{s}", s) for s in ("l", "r")] if "{s}" in pattern else [pattern]
    total = {}
    basis = basemesh.data.shape_keys.key_blocks["Basis"].data
    for rel in paths:
        full = os.path.join(TARGETS, rel + ".target.gz")
        if not os.path.exists(full):
            raise FileNotFoundError(full)
        key = TargetService.load_target(basemesh, full, weight=1.0, name="__t")
        for i, (p, b) in enumerate(zip(key.data, basis)):
            d = p.co - b.co
            if d.length_squared > 1e-12:
                g = to_gltf(d)
                acc = total.get(i, (0.0, 0.0, 0.0))
                total[i] = (acc[0] + g[0], acc[1] + g[1], acc[2] + g[2])
        basemesh.shape_key_remove(key)
    return total


def macro_deltas(basemesh, prop, value, neutral):
    original = HumanObjectProperties.get_value(prop, entity_reference=basemesh)
    HumanObjectProperties.set_value(prop, value, entity_reference=basemesh)
    TargetService.reapply_macro_details(basemesh)
    shaped = mix_positions(basemesh)
    HumanObjectProperties.set_value(prop, original, entity_reference=basemesh)
    TargetService.reapply_macro_details(basemesh)
    out = {}
    for i, (a, b) in enumerate(zip(shaped, neutral)):
        d = (a[0] - b[0], a[1] - b[1], a[2] - b[2])
        if d[0] * d[0] + d[1] * d[1] + d[2] * d[2] > 1e-12:
            out[i] = d
    return out


def race_deltas(basemesh, mix, neutral):
    """Deltas for a heritage mix like {"asian": 1.0, "african": 0.0, "caucasian": 0.0}."""
    names = ("asian", "african", "caucasian")
    original = {n: HumanObjectProperties.get_value(n, entity_reference=basemesh) for n in names}
    for n in names:
        HumanObjectProperties.set_value(n, mix[n], entity_reference=basemesh)
    TargetService.reapply_macro_details(basemesh)
    shaped = mix_positions(basemesh)
    for n in names:
        HumanObjectProperties.set_value(n, original[n], entity_reference=basemesh)
    TargetService.reapply_macro_details(basemesh)
    out = {}
    for i, (a, b) in enumerate(zip(shaped, neutral)):
        d = (a[0] - b[0], a[1] - b[1], a[2] - b[2])
        if d[0] * d[0] + d[1] * d[1] + d[2] * d[2] > 1e-12:
            out[i] = d
    return out


def quantize(v):
    return max(-32767, min(32767, round(v * DELTA_SCALE)))


def main(out_dir):
    os.makedirs(out_dir, exist_ok=True)
    bpy.ops.wm.read_factory_settings(use_empty=True)
    basemesh = HumanService.create_human(
        mask_helpers=False, detailed_helpers=False, extra_vertex_groups=True,
        feet_on_ground=True, scale=0.1,
    )
    TargetService.reapply_macro_details(basemesh)
    neutral = mix_positions(basemesh)

    morphs = []  # (name, {index: delta})
    sliders = []
    for sid, label, category, prop, low, high in MACROS:
        morphs.append((f"{sid}-", macro_deltas(basemesh, prop, low, neutral)))
        morphs.append((f"{sid}+", macro_deltas(basemesh, prop, high, neutral)))
        sliders.append({"id": sid, "label": label, "category": category,
                        "low": f"{sid}-", "high": f"{sid}+"})
    # Heritage: face and body structure blended toward each MakeHuman set.
    for heritage, label in (("asian", "East Asian"), ("african", "African"), ("caucasian", "European")):
        mix = {n: (1.0 if n == heritage else 0.0) for n in ("asian", "african", "caucasian")}
        sid = f"heritage_{heritage}"
        morphs.append((f"{sid}+", race_deltas(basemesh, mix, neutral)))
        sliders.append({"id": sid, "label": label, "category": "Heritage", "low": None, "high": f"{sid}+"})
    for sid, label, category, low, high in DETAILS:
        entry = {"id": sid, "label": label, "category": category, "low": None, "high": None}
        if low:
            morphs.append((f"{sid}-", target_deltas(basemesh, low)))
            entry["low"] = f"{sid}-"
        morphs.append((f"{sid}+", target_deltas(basemesh, high)))
        entry["high"] = f"{sid}+"
        sliders.append(entry)

    # Joints: each bone head and tail is the mean of a vertex set.
    with open(GROUPS_JSON, encoding="utf-8") as f:
        groups = json.load(f)
    with open(RIG_JSON, encoding="utf-8") as f:
        rig = json.load(f)

    def joint_vertices(info):
        if info["strategy"] == "CUBE":
            out = []
            for start, end in groups[info["cube_name"]]:
                out.extend(range(start, end + 1))
            return out
        if info["strategy"] == "VERTEX":
            return [info["vertex_index"]]
        return list(info["vertex_indices"])

    bones = {name: {"parent": b["parent"] or None,
                    "head": joint_vertices(b["head"]),
                    "tail": joint_vertices(b["tail"])}
             for name, b in rig.items()}

    # Neutral positions and morph deltas.
    with open(os.path.join(out_dir, "operative_morphs.bin"), "wb") as f:
        f.write(b"NVMH")
        f.write(struct.pack("<III", 1, len(neutral), len(morphs)))
        f.write(struct.pack("<f", DELTA_SCALE))
        for p in neutral:
            f.write(struct.pack("<fff", *p))
        for name, deltas in morphs:
            raw = name.encode("utf-8")
            f.write(struct.pack("<H", len(raw)))
            f.write(raw)
            f.write(struct.pack("<I", len(deltas)))
            for index in sorted(deltas):
                d = deltas[index]
                f.write(struct.pack("<Hhhh", index, quantize(d[0]), quantize(d[1]), quantize(d[2])))

    # Regions the game colours separately (MakeHuman vertex indices).
    regions = {}
    for name in ("scalp", "lips", "fingernails", "toenails"):
        group = basemesh.vertex_groups[name].index
        regions[name] = [
            v.index for v in basemesh.data.vertices if any(g.group == group for g in v.groups)
        ]
    for name in (
        "helper-l-eye", "helper-r-eye", "helper-upper-teeth", "helper-lower-teeth", "helper-tongue",
        "helper-l-eyelashes-1", "helper-l-eyelashes-2", "helper-r-eyelashes-1", "helper-r-eyelashes-2",
    ):
        regions[name] = [i for start, end in groups[name] for i in range(start, end + 1)]

    with open(os.path.join(out_dir, "operative_rig.json"), "w", encoding="utf-8", newline="\n") as f:
        json.dump({"sliders": sliders, "bones": bones, "regions": regions}, f, separators=(",", ":"))

    # Skinned neutral body for the game.
    HumanService.add_builtin_rig(basemesh, "game_engine_with_breast")
    mesh = basemesh.data
    vid = mesh.attributes.new("_VID", "FLOAT", "POINT")
    for i, v in enumerate(vid.data):
        v.value = float(i)
    bpy.context.view_layer.objects.active = basemesh
    basemesh.select_set(True)
    bpy.ops.object.shape_key_remove(all=True, apply_mix=True)
    # Keep the eyes, teeth, tongue and lashes (they follow every morph); drop
    # joint cubes and the clothing-fit shells.
    drop = set()
    for name in DROPPED_GROUPS:
        for start, end in groups[name]:
            drop.update(range(start, end + 1))
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="DESELECT")
    bpy.ops.object.mode_set(mode="OBJECT")
    for v in mesh.vertices:
        v.select = v.index in drop
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.delete(type="VERT")
    bpy.ops.object.mode_set(mode="OBJECT")
    for mod in list(basemesh.modifiers):
        if mod.type != "ARMATURE":
            basemesh.modifiers.remove(mod)
    bpy.ops.object.select_all(action="SELECT")
    bpy.ops.export_scene.gltf(
        filepath=os.path.join(out_dir, "operative.glb"),
        export_format="GLB",
        use_selection=True,
        export_yup=True,
        export_skins=True,
        export_morph=False,
        export_animations=False,
        export_attributes=True,
        export_materials="NONE",
    )
    print(f"OPERATIVE built: {len(sliders)} sliders, {len(morphs)} morphs, {len(bones)} bones")


if __name__ == "__main__":
    argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    main(argv[0] if argv else os.path.join(os.getcwd(), "assets", "characters"))
