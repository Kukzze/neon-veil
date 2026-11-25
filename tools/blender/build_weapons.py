"""Builds the fifty melee weapon models from code and exports them for the game.

Run headless:

    blender --background --factory-startup --python-exit-code 1 \
        --python tools/blender/build_weapons.py -- <assets/weapons dir> [<preview dir>]

Writes one GLB per model (katana_0.glb .. hammer_4.glb: five models per
class, in the order the arsenal names them), the editable source
(source/weapons.blend, one collection per model) and, when a preview
directory is given, a side-view render of each model.

Conventions (Blender axes): the hand grips the origin, the weapon runs up +Z
(the game's +Y after export), its broad face looks along Y and its edge or
width runs along X. Materials: steel, dark steel, brass, wrap, skin and one
emissive "glow" material; the game recolours the glow per element and
scrolls it along the weapon, using the glow mesh's UV V (0 at the hand end,
1 at the far end).
"""

import math
import os
import sys

import bmesh
import bpy
from mathutils import Euler, Matrix, Vector

CLASSES = [
    "katana",
    "dual_blades",
    "baton",
    "gauntlet",
    "whip",
    "claymore",
    "spear",
    "scythe",
    "axe",
    "hammer",
]


# ---------------------------------------------------------------- materials


def make_material(name, color, metallic, roughness, emission=0.0):
    mat = bpy.data.materials.new(name)
    mat.use_nodes = True
    bsdf = next(n for n in mat.node_tree.nodes if n.type == "BSDF_PRINCIPLED")
    bsdf.inputs["Base Color"].default_value = (*color, 1.0)
    bsdf.inputs["Metallic"].default_value = metallic
    bsdf.inputs["Roughness"].default_value = roughness
    if emission:
        bsdf.inputs["Emission Color"].default_value = (*color, 1.0)
        bsdf.inputs["Emission Strength"].default_value = emission
    return mat


M = {}


def setup_materials():
    M["steel"] = make_material("steel", (0.46, 0.48, 0.51), 0.95, 0.22)
    M["dark"] = make_material("dark_steel", (0.07, 0.075, 0.08), 0.9, 0.38)
    M["brass"] = make_material("brass", (0.62, 0.44, 0.19), 1.0, 0.3)
    M["wrap"] = make_material("wrap", (0.018, 0.018, 0.02), 0.0, 0.8)
    M["skin"] = make_material("skin", (0.52, 0.5, 0.45), 0.0, 0.55)
    M["glow"] = make_material("glow", (1.0, 1.0, 1.0), 0.0, 0.5, emission=6.0)


# ------------------------------------------------------------- mesh helpers


class Builder:
    """Collects the parts of one weapon into its own collection."""

    def __init__(self, name):
        self.coll = bpy.data.collections.new(name)
        bpy.context.scene.collection.children.link(self.coll)
        self.count = 0

    def finish(self, bm, mat, smooth_sides=False):
        self.count += 1
        name = f"{self.coll.name}_{self.count:02d}"
        mesh = bpy.data.meshes.new(name)
        bm.normal_update()
        bm.to_mesh(mesh)
        bm.free()
        mesh.materials.append(M[mat])
        for poly in mesh.polygons:
            poly.use_smooth = smooth_sides and abs(poly.normal.z) < 0.7
        obj = bpy.data.objects.new(name, mesh)
        self.coll.objects.link(obj)
        return obj

    def box(self, size, at, mat, rot=(0.0, 0.0, 0.0), bevel=0.0):
        bm = bmesh.new()
        bmesh.ops.create_cube(bm, size=1.0)
        bmesh.ops.scale(bm, vec=Vector(size), verts=bm.verts)
        if bevel > 0.0:
            bmesh.ops.bevel(
                bm,
                geom=list(bm.edges),
                offset=min(bevel, min(size) * 0.45),
                segments=2,
                affect="EDGES",
                clamp_overlap=True,
            )
        matrix = Matrix.Translation(Vector(at)) @ Euler(rot).to_matrix().to_4x4()
        bmesh.ops.transform(bm, matrix=matrix, verts=bm.verts)
        return self.finish(bm, mat)

    def cyl(self, r, z0, z1, mat, r2=None, segs=24, x=0.0, y=0.0, sx=1.0, sy=1.0):
        bm = bmesh.new()
        bmesh.ops.create_cone(
            bm,
            cap_ends=True,
            cap_tris=False,
            segments=segs,
            radius1=r,
            radius2=r if r2 is None else r2,
            depth=z1 - z0,
        )
        mid = (z0 + z1) * 0.5
        for v in bm.verts:
            v.co = Vector((v.co.x * sx + x, v.co.y * sy + y, v.co.z + mid))
        return self.finish(bm, mat, smooth_sides=True)

    def rod_between(self, a, b, r, mat, segs=12):
        """A cylinder from point a to point b (x, y, z)."""
        a, b = Vector(a), Vector(b)
        bm = bmesh.new()
        length = (b - a).length
        bmesh.ops.create_cone(
            bm, cap_ends=True, cap_tris=False, segments=segs, radius1=r, radius2=r, depth=length
        )
        rot = Vector((0, 0, 1)).rotation_difference((b - a).normalized())
        matrix = Matrix.Translation((a + b) * 0.5) @ rot.to_matrix().to_4x4()
        bmesh.ops.transform(bm, matrix=matrix, verts=bm.verts)
        return self.finish(bm, mat, smooth_sides=True)

    def sphere(self, r, at, mat, sx=1.0, sy=1.0, sz=1.0):
        bm = bmesh.new()
        bmesh.ops.create_uvsphere(bm, u_segments=16, v_segments=10, radius=r)
        for v in bm.verts:
            v.co = Vector((v.co.x * sx + at[0], v.co.y * sy + at[1], v.co.z * sz + at[2]))
        obj = self.finish(bm, mat)
        for poly in obj.data.polygons:
            poly.use_smooth = True
        return obj

    def prism(self, pts, mat):
        """An outline in the XZ plane, given as (x, z, half thickness)."""
        bm = bmesh.new()
        front = [bm.verts.new((x, -h, z)) for x, z, h in pts]
        back = [bm.verts.new((x, h, z)) for x, z, h in pts]
        bm.faces.new(front)
        bm.faces.new(list(reversed(back)))
        n = len(pts)
        for i in range(n):
            j = (i + 1) % n
            bm.faces.new((front[i], back[i], back[j], front[j]))
        bmesh.ops.recalc_face_normals(bm, faces=bm.faces[:])
        return self.finish(bm, mat)

    def ribbon(self, rows, mat):
        """A band built from rows of (a, b) points, each (x, z, half thickness).

        Faces are quads between consecutive rows (triangles where a row
        closes to a point), so tapering thickness never leaves a bent ngon
        to be split unpredictably on export.
        """
        bm = bmesh.new()
        front, back = [], []
        for a, b in rows:
            fa = bm.verts.new((a[0], -a[2], a[1]))
            ba = bm.verts.new((a[0], a[2], a[1]))
            closed = abs(a[0] - b[0]) < 1e-7 and abs(a[1] - b[1]) < 1e-7
            fb = fa if closed else bm.verts.new((b[0], -b[2], b[1]))
            bb = ba if closed else bm.verts.new((b[0], b[2], b[1]))
            front.append((fa, fb))
            back.append((ba, bb))

        def face(verts):
            unique = []
            for v in verts:
                if v not in unique:
                    unique.append(v)
            if len(unique) >= 3:
                bm.faces.new(unique)

        for i in range(len(rows) - 1):
            (fa0, fb0), (fa1, fb1) = front[i], front[i + 1]
            (ba0, bb0), (ba1, bb1) = back[i], back[i + 1]
            face((fa0, fb0, fb1, fa1))
            face((ba1, bb1, bb0, ba0))
            face((fa0, fa1, ba1, ba0))
            face((fb1, fb0, bb0, bb1))
        for i in (0, len(rows) - 1):
            (fa, fb), (ba, bb) = front[i], back[i]
            face((fa, ba, bb, fb))
        bmesh.ops.recalc_face_normals(bm, faces=bm.faces[:])
        return self.finish(bm, mat)

    def wedge(self, spine, edge, spine_half, edge_half, mat):
        """A blade section, thick along `spine` and sharp along `edge`.

        Both run base to tip as (x, z) and share their last point (the tip).
        """
        n = len(spine)
        rows = []
        for i in range(n):
            t = i / (n - 1)
            hs = edge_half if i == n - 1 else spine_half * (1.0 - 0.55 * t)
            rows.append(((spine[i][0], spine[i][1], hs), (edge[i][0], edge[i][1], edge_half)))
        return self.ribbon(rows, mat)

    def edge_glow(self, spine, edge, spine_half, edge_half, inset=(0.0012, 0.0068), stop=1.0):
        """A glowing band over a wedge's edge, proud of both faces.

        It wraps the very edge as a bead a few millimetres thick, so the
        glow stays a solid line even when the blade is seen edge-on (a band
        only on the faces turns sub-pixel there and breaks into dashes).
        """
        n = len(edge)
        inner, outer = [], []
        last = max(2, int((n - 1) * stop))
        reach = inset[1] * 1.25
        for i in range(1, last):
            t = i / (n - 1)
            ex, ez = edge[i]
            sx, sz = spine[i]
            width = math.hypot(sx - ex, sz - ez)
            if width < reach * 1.4:
                continue
            dx, dz = (sx - ex) / width, (sz - ez) / width
            spine_h = spine_half * (1.0 - 0.55 * t)
            inner_h = edge_half + (spine_h - edge_half) * reach / width + 0.0008
            outer.append((ex, ez, edge_half + 0.0013))
            inner.append((ex + dx * reach, ez + dz * reach, inner_h))
        if len(outer) < 2:
            return None
        return self.ribbon(list(zip(outer, inner)), "glow")

    def wrap(self, z0, z1, half_w, half_t, step=0.026, strap=0.0065):
        """Crossed grip wrap on both broad faces between z0 and z1."""
        z = z0 + step * 0.5
        while z < z1 - step * 0.25:
            for face in (-1.0, 1.0):
                for tilt in (-0.8, 0.8):
                    self.box(
                        (half_w * 2.5, 0.0016, strap),
                        (0.0, face * (half_t + 0.0009), z),
                        "wrap",
                        rot=(0.0, tilt, 0.0),
                    )
            z += step

    def rings(self, z0, z1, r, mat="wrap", step=0.018, width=0.006):
        z = z0 + step * 0.5
        while z < z1:
            self.cyl(r, z - width * 0.5, z + width * 0.5, mat, segs=20)
            z += step


def curve(n, fn):
    return [fn(i / (n - 1)) for i in range(n)]


# ------------------------------------------------------------------ weapons
#
# Every class has five models, in the order the arsenal names them; each
# builder reads its model's shape from VARIANTS.


def spike(b, a, tip, r, mat="steel", segs=10):
    """A cone from base point `a` (radius r) to `tip`."""
    a, tip = Vector(a), Vector(tip)
    bm = bmesh.new()
    bmesh.ops.create_cone(
        bm, cap_ends=True, cap_tris=False, segments=segs, radius1=r, radius2=0.0005,
        depth=(tip - a).length,
    )
    rot = Vector((0, 0, 1)).rotation_difference((tip - a).normalized())
    matrix = Matrix.Translation((a + tip) * 0.5) @ rot.to_matrix().to_4x4()
    bmesh.ops.transform(bm, matrix=matrix, verts=bm.verts)
    return b.finish(bm, mat, smooth_sides=True)


def curved_blade(b, base, length, w0, w1, sori, tip, ridge=0.0036, mat="steel", spine_glow=False):
    """A single-edged blade: spine on +X, edge on -X, bowed by `sori`."""
    n = 28

    def center(t):
        return sori * t * t

    def width(t):
        return w0 + (w1 - w0) * t

    spine = curve(n, lambda t: (center(t) + width(t) * 0.5, base + length * t))
    edge = []
    for i in range(n):
        t = i / (n - 1)
        ex = center(t) - width(t) * 0.5
        if t > 1.0 - tip:
            k = ((t - (1.0 - tip)) / tip) ** 1.7
            ex = ex + (spine[i][0] - ex) * k
        edge.append((ex, base + length * t))
    edge[-1] = spine[-1]
    b.wedge(spine, edge, ridge, 0.0006, mat)
    b.edge_glow(spine, edge, ridge, 0.0006, stop=0.97)
    if spine_glow:
        rows = []
        for i in range(1, n - 4):
            t = i / (n - 1)
            sx, sz = spine[i]
            h = ridge * (1.0 - 0.55 * t) + 0.0006
            rows.append(((sx - 0.0012, sz, h), (sx - 0.0045, sz, h)))
        b.ribbon(rows, "glow")
    return spine, edge


def straight_blade(b, base, length, w0, w1, tip, double, ridge=0.0035, glow="edge", curve_x=0.0):
    """A straight (or gently bent, `curve_x`) blade from `base`; `tip` is the
    share of its length in the point."""
    n = 16
    half_widths = [w0 * 0.5 + (w1 - w0) * 0.5 * (i / (n - 1)) for i in range(n)]

    def bend(i):
        t = i / (n - 1)
        return curve_x * t * t

    def taper(i):
        t = i / (n - 1)
        if t <= 1.0 - tip:
            return 1.0
        return max(0.0, 1.0 - ((t - (1.0 - tip)) / tip) ** 1.25)

    z = [base + length * i / (n - 1) for i in range(n)]
    if double:
        center = [(bend(i), z[i]) for i in range(n)]
        for side in (-1.0, 1.0):
            edge = [(bend(i) + side * half_widths[i] * taper(i), z[i]) for i in range(n)]
            edge[-1] = center[-1]
            b.wedge(center, edge, ridge, 0.0006, "steel")
            if glow == "edge":
                b.edge_glow(center, edge, ridge, 0.0006, inset=(0.001, 0.004), stop=0.95)
        if glow == "fuller":
            top = base + length * (1.0 - tip) * 0.92
            b.prism(
                [(-0.004, base + 0.01, ridge + 0.0003), (0.004, base + 0.01, ridge + 0.0003),
                 (0.004, top, ridge * 0.6 + 0.0003), (0.0, top + 0.02, ridge * 0.5 + 0.0003),
                 (-0.004, top, ridge * 0.6 + 0.0003)],
                "glow",
            )
    else:
        spine = [(bend(i) + half_widths[i], z[i]) for i in range(n)]
        edge = []
        for i in range(n):
            t = i / (n - 1)
            ex = bend(i) - half_widths[i]
            if t > 1.0 - tip:
                k = (t - (1.0 - tip)) / tip
                ex = ex + (spine[i][0] - ex) * k
            edge.append((ex, z[i]))
        edge[-1] = spine[-1]
        b.wedge(spine, edge, ridge, 0.0006, "steel")
        b.edge_glow(spine, edge, ridge, 0.0006, inset=(0.0012, 0.005), stop=0.98)
        if curve_x == 0.0:
            b.box((0.003, ridge * 2.2, length * (1.0 - tip) * 0.85),
                  (half_widths[0] - 0.0015, 0.0, base + length * (1.0 - tip) * 0.45), "dark")


def tsuka(b, grip, top=0.13):
    """Ray-skin grip with crossed wrap from `top` down `grip` metres."""
    bottom = top - grip
    b.box((0.03, 0.022, grip), (0.0, 0.0, (top + bottom) * 0.5), "skin", bevel=0.006)
    b.wrap(bottom + 0.012, top - 0.01, 0.015, 0.011)
    for face in (-1.0, 1.0):
        b.box((0.012, 0.004, 0.03), (0.003, face * 0.0128, top - grip * 0.42), "brass", bevel=0.0015)
    b.cyl(0.019, top - 0.012, top + 0.001, "dark", sy=0.76)
    b.box((0.034, 0.026, 0.02), (0.0, 0.0, bottom - 0.008), "dark", bevel=0.006)


def katana(b, p):
    curved_blade(b, 0.175, p["length"], p["width"], p["width"] * 0.77, p["sori"], 0.1,
                 mat=p["blade"], spine_glow=p["spine_glow"])
    b.box((0.038, 0.014, 0.024), (0.0, 0.0, 0.163), "brass", bevel=0.002)
    b.cyl(0.026, 0.146, 0.150, "brass", sy=0.75)
    if p["guard"] == "round":
        b.cyl(0.044, 0.139, 0.146, "brass", sy=0.82, segs=40)
        b.cyl(0.041, 0.137, 0.148, "dark", sy=0.8, segs=40)
        for side in (-1.0, 1.0):
            b.cyl(0.006, 0.1365, 0.1485, "glow", x=side * 0.026, segs=12)
    elif p["guard"] == "square":
        b.box((0.074, 0.058, 0.009), (0.0, 0.0, 0.1425), "dark", bevel=0.004)
        b.box((0.078, 0.062, 0.004), (0.0, 0.0, 0.1425), "brass", bevel=0.0015)
        b.box((0.05, 0.064, 0.003), (0.0, 0.0, 0.1425), "glow")
    b.cyl(0.026, 0.131, 0.137, "brass", sy=0.75)
    tsuka(b, p["grip"])


def dual_blades(b, p):
    if p["curve"]:
        curved_blade(b, 0.098, p["length"], p["width"], p["width"] * 0.8, p["curve"], p["tip"],
                     ridge=0.0034)
    else:
        straight_blade(b, 0.098, p["length"], p["width"], p["width"] * 0.85, p["tip"],
                       double=p["double"], ridge=0.0034)
    b.box((p["guard"], 0.022, 0.012), (0.0, 0.0, 0.09), "dark", bevel=0.003)
    b.box((0.012, 0.024, 0.016), (-p["guard"] * 0.45, 0.0, 0.096), "steel", rot=(0, -0.5, 0), bevel=0.002)
    b.box((0.028, 0.02, 0.15), (0.0, 0.0, 0.012), "skin", bevel=0.005)
    b.wrap(-0.058, 0.08, 0.014, 0.01, step=0.024)
    b.box((0.032, 0.024, 0.016), (0.0, 0.0, -0.066), "dark", bevel=0.005)
    b.cyl(0.006, -0.08, -0.072, "glow", segs=12)
    if p["ring"]:
        # A finger ring past the pommel (karambit style).
        b.cyl(0.024, -0.1, -0.09, "steel", segs=24)
        b.cyl(0.016, -0.101, -0.089, "dark", segs=24)


def baton(b, p):
    top = 0.17 + p["length"]
    b.cyl(0.021, -0.11, 0.14, "wrap", segs=20)
    b.rings(-0.1, 0.13, 0.0225, mat="dark", step=0.024, width=0.008)
    b.cyl(0.025, -0.13, -0.11, "dark", segs=20)
    b.cyl(0.024, 0.14, 0.17, "steel", r2=0.019, segs=20)
    b.cyl(p["radius"], 0.17, top, "dark", segs=20)
    for k in range(p["rings"]):
        z = top - 0.32 + k * (0.28 / max(1, p["rings"] - 1)) if p["rings"] > 1 else top - 0.18
        b.cyl(p["radius"] + 0.0015, z, z + 0.022, "glow", segs=20)
        b.cyl(p["radius"] + 0.0025, z - 0.006, z, "steel", segs=20)
    if p["tonfa"]:
        b.cyl(0.016, -0.0, 0.11, "wrap", segs=16, x=0.0)
        b.rod_between((0.0, 0.0, 0.17), (0.11, 0.0, 0.17), 0.015, "dark", segs=16)
        b.cyl(0.017, 0.12, 0.2, "wrap", segs=16, x=0.11)
    head = p["head"]
    if head == "prongs":
        b.cyl(p["radius"] + 0.005, top, top + 0.03, "steel", segs=20)
        for side in (-1.0, 1.0):
            b.box((0.006, 0.01, 0.06), (side * 0.013, 0.0, top + 0.06), "steel", bevel=0.0015)
        b.box((0.016, 0.004, 0.05), (0.0, 0.0, top + 0.06), "glow")
    elif head == "pike":
        b.cyl(p["radius"] + 0.004, top, top + 0.025, "steel", segs=20)
        spike(b, (0, 0, top + 0.025), (0, 0, top + 0.16), 0.016)
        b.cyl(0.0175, top + 0.03, top + 0.045, "glow", r2=0.014, segs=16)
    elif head == "mace":
        b.sphere(0.042, (0, 0, top + 0.03), "dark")
        for k in range(8):
            a = k * math.pi / 4
            d = Vector((math.cos(a), math.sin(a), 0.35)).normalized()
            base = Vector((0, 0, top + 0.03)) + d * 0.036
            spike(b, base, base + d * 0.045, 0.009)
        b.sphere(0.02, (0, 0, top + 0.075), "glow")
    else:
        b.cyl(p["radius"] + 0.004, top, top + 0.03, "steel", segs=20)
        b.cyl(p["radius"] * 0.6, top + 0.03, top + 0.036, "glow", segs=16)


def gauntlet(b, p):
    bulk = p["bulk"]
    b.cyl(0.058 * bulk, -0.27, -0.05, "dark", r2=0.054 * bulk, segs=8)
    for z in (-0.25, -0.18, -0.11):
        b.cyl(0.061 * bulk, z, z + 0.024, "steel", segs=8)
    for side in (-1.0, 1.0):
        b.box((0.006, 0.004, 0.19), (side * 0.04 * bulk, -0.056 * bulk, -0.155), "glow")
    b.cyl(0.062 * bulk, -0.06, -0.03, "steel", r2=0.066 * bulk, segs=8)
    b.box((0.11 * bulk, 0.05 * bulk, 0.1), (0.0, 0.0, 0.03), "dark", bevel=0.008)
    b.box((0.1 * bulk, 0.012, 0.08), (0.0, -0.03 * bulk, 0.03), "steel", bevel=0.004)
    for i in range(4):
        x = (-0.039 + i * 0.026) * bulk
        b.box((0.024 * bulk, 0.04, 0.03), (x, -0.004, 0.096), "steel", bevel=0.005)
        for k, z in enumerate((0.125, 0.15)):
            b.box((0.021, 0.034, 0.024), (x, 0.004 + k * 0.01, z), "dark", bevel=0.004)
        if p["spikes"]:
            spike(b, (x, -0.02, 0.105), (x, -0.06, 0.12), 0.007)
    b.box((0.11 * bulk, 0.008, 0.012), (0.0, -0.026 * bulk, 0.095), "glow")
    b.box((0.03, 0.04, 0.06), (0.065 * bulk, 0.004, 0.05), "dark", rot=(0, 0.3, 0), bevel=0.006)
    if p["pistons"]:
        for side in (-1.0, 1.0):
            x = side * 0.03 * bulk
            b.cyl(0.011, -0.22, -0.04, "steel", x=x, y=-0.064 * bulk, segs=12)
            b.cyl(0.007, -0.04, 0.03, "dark", x=x, y=-0.064 * bulk, segs=12)
            b.cyl(0.012, -0.12, -0.1, "glow", x=x, y=-0.064 * bulk, segs=12)
    if p["ram"]:
        b.box((0.12 * bulk, 0.07, 0.03), (0.0, -0.01, 0.175), "steel", bevel=0.006)
        b.box((0.1 * bulk, 0.072, 0.006), (0.0, -0.01, 0.19), "glow")


def whip(b, p):
    b.cyl(0.019, -0.1, 0.14, "wrap", segs=20)
    b.rings(-0.09, 0.13, 0.0205, mat="dark", step=0.02, width=0.006)
    b.cyl(0.024, -0.125, -0.1, "dark", segs=20)
    b.cyl(0.006, -0.13, -0.124, "glow", segs=12)
    b.cyl(0.022, 0.14, 0.175, "steel", r2=0.012, segs=20)
    n = p["segments"]
    pts = []
    for i in range(n):
        t = i / (n - 1)
        x = p["sag"] * math.sin(t * math.pi * p["waves"]) - 0.03 * t
        pts.append(Vector((x, 0.0, 0.175 + p["length"] * t)))
    for i in range(len(pts) - 1):
        a, c = pts[i], pts[i + 1]
        mid = (a + c) * 0.5
        r = p["radius"] * (1.0 - 0.6 * i / len(pts))
        b.rod_between(a.lerp(mid, 0.15), c.lerp(mid, 0.15), r, "dark", segs=8)
        b.rod_between(a, c, r * 0.55, "glow", segs=6)
        if p["barbs"] and i % 2 == 0:
            spike(b, mid, mid + Vector((r * 3.0, 0.0, -r * 1.5)), r * 0.7)
    if p["end_handle"]:
        end = pts[-1]
        b.rod_between(end + Vector((-0.06, 0, 0)), end + Vector((0.06, 0, 0)), 0.014, "wrap", segs=12)


def claymore(b, p):
    base = 0.215
    straight_blade(b, base, p["length"], p["width"], p["width"] * p["taper"], p["tip"],
                   double=p["double"], ridge=0.0045, glow="fuller" if p["double"] else "edge")
    g = p["guard"]
    b.box((g, 0.034, 0.03), (0.0, 0.0, 0.2), "dark", bevel=0.006)
    for side in (-1.0, 1.0):
        b.box((0.07, 0.028, 0.024), (side * (g * 0.5 + 0.025), 0.0, 0.218), "steel",
              rot=(0.0, -side * p["quillon"], 0.0), bevel=0.005)
        b.sphere(0.014, (side * (g * 0.5 + 0.058), 0.0, 0.218 + 0.07 * math.sin(p["quillon"])), "brass")
    b.box((0.05, 0.03, 0.04), (0.0, 0.0, 0.233), "steel", bevel=0.004)
    b.cyl(0.006, 0.226, 0.24, "glow", y=-0.016, segs=12)
    if p["ribs"]:
        for k in range(4):
            z = base + 0.12 + k * 0.12
            b.box((p["width"] * 0.6, 0.012, 0.012), (0.0, 0.0, z), "dark", bevel=0.003)
    b.cyl(0.019, -0.2, 0.186, "skin", sy=0.85)
    b.rings(-0.19, 0.18, 0.0205, step=0.022)
    b.cyl(0.024, 0.17, 0.188, "dark", sy=0.85)
    b.sphere(0.034, (0.0, 0.0, -0.225), "dark", sy=0.75, sz=0.85)
    b.cyl(0.012, -0.236, -0.214, "glow", y=-0.024, segs=16)


def leaf_head(b, base, length, half_width, ridge=0.0045, peak=0.33):
    n = 18
    center = [(0.0, base + length * i / (n - 1)) for i in range(n)]
    for side in (-1.0, 1.0):
        edge = []
        for i in range(n):
            t = i / (n - 1)
            w = half_width * math.sin(math.pi * min(1.0, t / peak) * 0.5) * (1.0 - max(0.0, t - peak) / (1.0 - peak)) ** 0.9
            edge.append((side * max(w, 0.004 * (1 - t)), center[i][1]))
        edge[-1] = center[-1]
        b.wedge(center, edge, ridge, 0.0006, "steel")
        b.edge_glow(center, edge, ridge, 0.0006, inset=(0.001, 0.0045), stop=0.92)


def spear(b, p):
    top = p["shaft"]
    bottom = -0.63 if top > 1.0 else -0.4
    b.cyl(0.016, bottom, top, "dark", segs=16)
    b.rings(-0.08, 0.22, 0.0172, step=0.02)
    if top > 1.0:
        b.rings(0.62, 0.8, 0.0172, step=0.02)
    for z in (-0.11, 0.24, top * 0.5, top * 0.65):
        b.cyl(0.019, z, z + 0.014, "steel", segs=16)
    b.cyl(0.017, bottom - 0.05, bottom, "steel", r2=0.019, segs=16)
    spike(b, (0, 0, bottom - 0.05), (0, 0, bottom - 0.11), 0.016)
    b.cyl(0.02, top - 0.02, top + 0.04, "steel", r2=0.016, segs=16)
    b.cyl(0.021, top + 0.002, top + 0.014, "glow", segs=16)
    head = p["head"]
    if head == "glaive":
        curved_blade(b, top + 0.03, 0.5, 0.06, 0.05, 0.05, 0.16, ridge=0.0045)
    else:
        leaf_head(b, top + 0.03, p["head_length"], p["head_width"], peak=p.get("peak", 0.33))
        if head == "harpoon":
            for side in (-1.0, 1.0):
                b.prism([(side * 0.006, top + 0.06, 0.004), (side * 0.05, top + 0.01, 0.001),
                         (side * 0.012, top + 0.09, 0.004)], "steel")
    for side in (-1.0, 1.0):
        b.box((0.008, 0.004, 0.09), (0.0, side * 0.018, top - 0.04), "steel", bevel=0.0015)


def scythe(b, p):
    top = p["shaft"]
    b.cyl(0.017, -0.33, top, "dark", segs=16)
    b.rings(-0.2, 0.15, 0.0182, step=0.02)
    b.rings(top * 0.46, top * 0.65, 0.0182, step=0.02)
    for z in (-0.33, 0.16, top * 0.45, top * 0.66):
        b.cyl(0.02, z, z + 0.016, "steel", segs=16)
    b.box((0.05, 0.034, 0.07), (0.0, 0.0, top), "steel", bevel=0.006)
    b.box((0.03, 0.03, 0.05), (0.03, 0.0, top + 0.02), "dark", rot=(0, -0.6, 0), bevel=0.004)

    def crescent(direction, length, width, arc, drop):
        n = 26
        spine = curve(n, lambda t: (direction * (0.025 + length * t),
                                    top + 0.015 + arc * math.sin(math.pi * t * 0.85) - drop * t * t))
        edge = []
        for i in range(n):
            t = i / (n - 1)
            x, z = spine[i]
            w = width * (1.0 - t) ** 0.75 + 0.0005
            edge.append((x - direction * 0.012 * t, z - w))
        edge[-1] = spine[-1]
        b.wedge(spine, edge, 0.0045, 0.0006, "steel")
        b.edge_glow(spine, edge, 0.0045, 0.0006, inset=(0.0015, 0.008), stop=0.96)
        return spine

    spine = crescent(-1.0, p["length"], p["width"], p["arc"], p["drop"])
    if p["double"]:
        crescent(1.0, p["length"] * 0.45, p["width"] * 0.7, p["arc"] * 0.6, p["drop"] * 0.5)
    if p["spikes"]:
        for t in (0.25, 0.5, 0.75):
            x, z = spine[int(t * (len(spine) - 1))]
            spike(b, (x, 0, z - 0.004), (x + 0.01, 0, z + 0.04), 0.006, mat="dark")


def axe(b, p):
    top = p["haft"]
    head_z = top - 0.06
    b.cyl(0.018, -0.16, top, "dark", segs=16)
    b.wrap(-0.14, 0.2, 0.018, 0.018, step=0.026)
    b.cyl(0.022, -0.18, -0.15, "steel", segs=16)
    b.cyl(0.02, 0.2, 0.215, "steel", segs=16)
    b.box((0.05, 0.04, 0.13), (0.0, 0.0, head_z), "steel", bevel=0.006)
    b.box((0.008, 0.042, 0.1), (0.0, 0.0, head_z), "glow")

    def bit(direction, size, beard):
        n = 22
        spine = [(direction * 0.024, head_z - 0.05 + 0.11 * i / (n - 1)) for i in range(n)]
        edge = []
        for i in range(n):
            t = i / (n - 1)
            a = math.pi * (t - 0.5) * 0.95
            dz = 0.12 * math.sin(a) - (beard * (1.0 - t) ** 2)
            edge.append((direction * (0.03 + size * math.cos(a) ** 0.8), head_z + dz))
        b.ribbon([((sx, sz, 0.011), (ex, ez, 0.0008)) for (sx, sz), (ex, ez) in zip(spine, edge)], "steel")
        b.ribbon([((x - direction * 0.002, z, 0.0016), (x - direction * 0.012, z, 0.0034))
                  for x, z in edge[1:-1]], "glow")

    bit(-1.0, p["bit"], p["beard"])
    if p["double"]:
        bit(1.0, p["bit"], p["beard"])
    elif p["spike"] > 0:
        b.prism([(0.024, head_z - 0.03, 0.01), (0.024 + p["spike"], head_z, 0.002),
                 (0.024, head_z + 0.035, 0.01)], "dark")


def hammer(b, p):
    top = p["haft"]
    hz = top + 0.09
    w, h, d = p["head"]
    b.cyl(0.02, -0.18, top, "dark", segs=16)
    b.wrap(-0.16, 0.22, 0.02, 0.02, step=0.026)
    for z in (-0.19, 0.23, top - 0.02):
        b.cyl(0.024, z, z + 0.02, "steel", segs=16)
    if p["shape"] == "cyl":
        b.rod_between((-w * 0.5, 0, hz), (w * 0.5, 0, hz), h * 0.5, "dark", segs=24)
        for k in (-1.0, 0.0, 1.0):
            x = k * w * 0.3
            b.rod_between((x - 0.012, 0, hz), (x + 0.012, 0, hz), h * 0.52, "glow", segs=24)
        for side in (-1.0, 1.0):
            b.rod_between((side * w * 0.5, 0, hz), (side * (w * 0.5 + 0.03), 0, hz), h * 0.56, "steel", segs=24)
    else:
        b.box((w, d, h), (0.0, 0.0, hz), "dark", bevel=0.014)
        for side in (-1.0, 1.0):
            b.box((0.035, d + 0.02, h + 0.02), (side * (w * 0.5 + 0.0175), 0.0, hz), "steel", bevel=0.012)
            b.box((0.012, d - 0.02, h - 0.02), (side * (w * 0.5 + 0.038), 0.0, hz), "dark", bevel=0.006)
            for k in range(3):
                b.box((w * 0.45, 0.004, 0.012), (0.0, side * (d * 0.5 + 0.001), hz - h * 0.25 + k * h * 0.25), "glow")
    b.box((0.06, 0.05, 0.05), (0.0, 0.0, hz + h * 0.5 + 0.02), "steel", bevel=0.008)
    if p["top_spike"]:
        spike(b, (0, 0, hz + h * 0.5 + 0.045), (0, 0, hz + h * 0.5 + 0.13), 0.022)
    if p["back_spike"]:
        spike(b, (w * 0.5 + 0.035, 0, hz), (w * 0.5 + 0.15, 0, hz - 0.03), 0.03)


BUILDERS = {
    "katana": katana,
    "dual_blades": dual_blades,
    "baton": baton,
    "gauntlet": gauntlet,
    "whip": whip,
    "claymore": claymore,
    "spear": spear,
    "scythe": scythe,
    "axe": axe,
    "hammer": hammer,
}

# Shapes of each class's five models (arsenal order).
VARIANTS = {
    "katana": [  # Shiden, Kagerou, Tsukikage, Raimei, Hototogisu
        dict(length=0.84, width=0.031, sori=0.026, blade="steel", guard="round", grip=0.25, spine_glow=False),
        dict(length=0.9, width=0.026, sori=0.034, blade="steel", guard="square", grip=0.25, spine_glow=True),
        dict(length=0.78, width=0.035, sori=0.02, blade="dark", guard="round", grip=0.23, spine_glow=False),
        dict(length=0.8, width=0.03, sori=0.004, blade="steel", guard="none", grip=0.24, spine_glow=True),
        dict(length=1.08, width=0.033, sori=0.042, blade="steel", guard="round", grip=0.34, spine_glow=False),
    ],
    "dual_blades": [  # Mantis, Twin Fang, Sickle Pair, Scissor, Wasp
        dict(length=0.44, width=0.036, tip=0.14, curve=-0.05, double=False, guard=0.07, ring=True),
        dict(length=0.5, width=0.032, tip=0.25, curve=0.0, double=True, guard=0.08, ring=False),
        dict(length=0.42, width=0.042, tip=0.1, curve=-0.13, double=False, guard=0.06, ring=True),
        dict(length=0.48, width=0.054, tip=0.06, curve=0.0, double=False, guard=0.08, ring=False),
        dict(length=0.38, width=0.022, tip=0.35, curve=0.0, double=True, guard=0.05, ring=False),
    ],
    "baton": [  # Riot Rod, Lineman, Stun Pike, Night Stick, Arc Mace
        dict(length=0.57, radius=0.017, rings=4, head="prongs", tonfa=False),
        dict(length=0.5, radius=0.021, rings=3, head="cap", tonfa=False),
        dict(length=0.82, radius=0.016, rings=5, head="pike", tonfa=False),
        dict(length=0.52, radius=0.019, rings=2, head="cap", tonfa=True),
        dict(length=0.46, radius=0.018, rings=3, head="mace", tonfa=False),
    ],
    "gauntlet": [  # Piston Fist, Breaker, Knuckle Drive, Ram, Jackhammer
        dict(bulk=1.0, spikes=False, pistons=True, ram=False),
        dict(bulk=1.05, spikes=True, pistons=False, ram=False),
        dict(bulk=0.95, spikes=False, pistons=False, ram=False),
        dict(bulk=1.2, spikes=False, pistons=False, ram=True),
        dict(bulk=1.1, spikes=True, pistons=True, ram=False),
    ],
    "whip": [  # Monofilament, Lash, Garrote, Coil, Razor Line
        dict(length=1.9, segments=40, sag=0.03, waves=1.0, radius=0.006, barbs=False, end_handle=False),
        dict(length=1.65, segments=30, sag=0.05, waves=1.3, radius=0.011, barbs=False, end_handle=False),
        dict(length=1.2, segments=24, sag=0.07, waves=1.0, radius=0.008, barbs=False, end_handle=True),
        dict(length=1.6, segments=36, sag=0.09, waves=3.0, radius=0.01, barbs=False, end_handle=False),
        dict(length=1.7, segments=30, sag=0.05, waves=1.6, radius=0.01, barbs=True, end_handle=False),
    ],
    "claymore": [  # Greatblade, Executioner, Bulkhead, Cleaver, Longsword
        dict(length=1.25, width=0.074, taper=0.68, tip=0.1, double=True, guard=0.2, quillon=0.45, ribs=False),
        dict(length=1.1, width=0.095, taper=0.95, tip=0.03, double=True, guard=0.18, quillon=0.1, ribs=False),
        dict(length=1.0, width=0.11, taper=0.85, tip=0.08, double=True, guard=0.28, quillon=0.0, ribs=True),
        dict(length=1.05, width=0.1, taper=1.05, tip=0.1, double=False, guard=0.16, quillon=0.3, ribs=False),
        dict(length=1.15, width=0.05, taper=0.7, tip=0.2, double=True, guard=0.22, quillon=0.6, ribs=False),
    ],
    "spear": [  # Lance, Harpoon, Pike, Glaive, Javelin
        dict(shaft=1.27, head="leaf", head_length=0.3, head_width=0.034),
        dict(shaft=1.2, head="harpoon", head_length=0.24, head_width=0.03),
        dict(shaft=1.6, head="leaf", head_length=0.36, head_width=0.02, peak=0.15),
        dict(shaft=1.15, head="glaive", head_length=0.5, head_width=0.03),
        dict(shaft=0.95, head="leaf", head_length=0.2, head_width=0.028),
    ],
    "scythe": [  # Reaper, Harvester, Crescent, Mourner, Grim Arc
        dict(shaft=1.2, length=0.68, width=0.085, arc=0.09, drop=0.17, double=False, spikes=False),
        dict(shaft=1.3, length=0.85, width=0.1, arc=0.07, drop=0.12, double=False, spikes=False),
        dict(shaft=1.1, length=0.6, width=0.075, arc=0.18, drop=0.3, double=False, spikes=False),
        dict(shaft=1.2, length=0.62, width=0.08, arc=0.09, drop=0.17, double=True, spikes=False),
        dict(shaft=1.25, length=0.75, width=0.09, arc=0.11, drop=0.2, double=False, spikes=True),
    ],
    "axe": [  # Hatchet, Bearded Axe, Splitter, Breacher, Tomahawk
        dict(haft=0.5, bit=0.11, beard=0.0, double=False, spike=0.05),
        dict(haft=0.66, bit=0.15, beard=0.07, double=False, spike=0.086),
        dict(haft=0.7, bit=0.14, beard=0.0, double=True, spike=0.0),
        dict(haft=0.68, bit=0.13, beard=0.02, double=False, spike=0.15),
        dict(haft=0.48, bit=0.09, beard=0.03, double=False, spike=0.11),
    ],
    "hammer": [  # Maul, Sledge, Pile Driver, Anvil, Wrecker
        dict(haft=0.7, head=(0.26, 0.17, 0.17), shape="box", top_spike=True, back_spike=False),
        dict(haft=0.62, head=(0.22, 0.13, 0.13), shape="box", top_spike=False, back_spike=False),
        dict(haft=0.72, head=(0.3, 0.18, 0.18), shape="cyl", top_spike=False, back_spike=False),
        dict(haft=0.66, head=(0.34, 0.12, 0.2), shape="box", top_spike=True, back_spike=False),
        dict(haft=0.74, head=(0.24, 0.16, 0.16), shape="box", top_spike=False, back_spike=True),
    ],
}


def model_names():
    """(class, variant, file stem) for all fifty models."""
    return [(c, v, f"{c}_{v}") for c in CLASSES for v in range(5)]


# ------------------------------------------------------------- post-process


def objects_with(coll, mat_name):
    return [o for o in coll.objects if o.type == "MESH" and o.data.materials[0].name == mat_name]


def join(objs):
    if len(objs) < 2:
        return objs[0] if objs else None
    bpy.ops.object.select_all(action="DESELECT")
    for o in objs:
        o.select_set(True)
    bpy.context.view_layer.objects.active = objs[0]
    bpy.ops.object.join()
    return bpy.context.view_layer.objects.active


def flow_uv(obj):
    """UV V runs 0 to 1 along the glow's longest extent, from the hand out."""
    mesh = obj.data
    layer = mesh.uv_layers.new(name="UVMap")
    co = [v.co for v in mesh.vertices]
    lo = [min(c[i] for c in co) for i in range(3)]
    hi = [max(c[i] for c in co) for i in range(3)]
    ext = [hi[i] - lo[i] for i in range(3)]
    axis = max(range(3), key=lambda i: ext[i])
    across = max((i for i in range(3) if i != axis), key=lambda i: ext[i])
    # Scythe blades run out along -X: the hand end is at the high X side.
    flip = axis == 0 and abs(hi[0]) < abs(lo[0])
    for loop in mesh.loops:
        c = mesh.vertices[loop.vertex_index].co
        v = (c[axis] - lo[axis]) / max(ext[axis], 1e-6)
        layer.data[loop.index].uv = ((c[across] - lo[across]) / max(ext[across], 1e-6), 1.0 - v if flip else v)


def finish_weapon(name, coll):
    for mat in ("steel", "dark_steel", "brass", "wrap", "skin"):
        merged = join(objects_with(coll, mat))
        if merged:
            merged.name = f"{name}_{mat}"
    glow = join(objects_with(coll, "glow"))
    glow.name = f"{name}_glow"
    flow_uv(glow)


def export(coll, path):
    bpy.ops.object.select_all(action="DESELECT")
    for o in coll.objects:
        o.select_set(True)
    bpy.context.view_layer.objects.active = coll.objects[0]
    bpy.ops.export_scene.gltf(
        filepath=path,
        export_format="GLB",
        use_selection=True,
        export_cameras=False,
        export_lights=False,
        export_animations=False,
    )


def render_previews(out_dir):
    scene = bpy.context.scene
    for engine in ("BLENDER_EEVEE_NEXT", "BLENDER_EEVEE", "BLENDER_WORKBENCH"):
        try:
            scene.render.engine = engine
            break
        except TypeError:
            continue
    scene.render.resolution_x, scene.render.resolution_y = 300, 640
    world = bpy.data.worlds.new("studio")
    world.use_nodes = True
    world.node_tree.nodes["Background"].inputs[0].default_value = (0.02, 0.022, 0.026, 1)
    scene.world = world
    sun = bpy.data.objects.new("key", bpy.data.lights.new("key", "SUN"))
    sun.data.energy = 4.0
    sun.rotation_euler = Euler((math.radians(60), 0, math.radians(-35)))
    scene.collection.objects.link(sun)
    cam = bpy.data.objects.new("cam", bpy.data.cameras.new("cam"))
    cam.data.type = "ORTHO"
    scene.collection.objects.link(cam)
    scene.camera = cam
    names = [n for _, _, n in model_names()]
    for name in names:
        for other in names:
            bpy.data.collections[other].hide_render = other != name
        coll = bpy.data.collections[name]
        pts = [o.matrix_world @ Vector(c) for o in coll.objects for c in o.bound_box]
        lo = Vector((min(p.x for p in pts), min(p.y for p in pts), min(p.z for p in pts)))
        hi = Vector((max(p.x for p in pts), max(p.y for p in pts), max(p.z for p in pts)))
        mid = (lo + hi) * 0.5
        cam.data.ortho_scale = max(hi.z - lo.z, (hi.x - lo.x) * 2.15) * 1.1
        cam.location = (mid.x, mid.y - 3.0, mid.z)
        cam.rotation_euler = Euler((math.radians(90), 0, 0))
        scene.render.filepath = os.path.join(out_dir, f"{name}.png")
        bpy.ops.render.render(write_still=True)


def main():
    args = sys.argv[sys.argv.index("--") + 1 :]
    out_dir = os.path.abspath(args[0])
    preview = os.path.abspath(args[1]) if len(args) > 1 else None
    os.makedirs(os.path.join(out_dir, "source"), exist_ok=True)
    bpy.ops.wm.read_factory_settings(use_empty=True)
    setup_materials()
    for cls, variant, name in model_names():
        builder = Builder(name)
        BUILDERS[cls](builder, VARIANTS[cls][variant])
        finish_weapon(name, builder.coll)
        path = os.path.join(out_dir, f"{name}.glb")
        export(builder.coll, path)
        faces = sum(len(o.data.polygons) for o in builder.coll.objects)
        print(f"WEAPON {name} faces={faces} bytes={os.path.getsize(path)}")
    bpy.ops.wm.save_as_mainfile(filepath=os.path.join(out_dir, "source", "weapons.blend"))
    if preview:
        os.makedirs(preview, exist_ok=True)
        render_previews(preview)
        print("PREVIEWS", preview)


if __name__ == "__main__":
    main()
