"""Builds the ten melee weapon models from code and exports them for the game.

Run headless:

    blender --background --factory-startup --python-exit-code 1 \
        --python tools/blender/build_weapons.py -- <assets/weapons dir> [<preview dir>]

Writes one GLB per weapon class (katana.glb, dual_blades.glb, ...), the
editable source (source/weapons.blend, one collection per class) and, when a
preview directory is given, a side-view render of each class.

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


def katana(b):
    base, length, sori = 0.175, 0.84, 0.026

    def center(t):
        return sori * t * t

    def width(t):
        return 0.031 - 0.007 * t

    n = 28
    spine = curve(n, lambda t: (center(t) + width(t) * 0.5, base + length * t))
    edge = []
    for i in range(n):
        t = i / (n - 1)
        ex = center(t) - width(t) * 0.5
        sx = spine[i][0]
        if t > 0.9:
            k = ((t - 0.9) / 0.1) ** 1.7
            ex = ex + (sx - ex) * k
        edge.append((ex, base + length * t))
    edge[-1] = spine[-1]
    b.wedge(spine, edge, 0.0036, 0.0006, "steel")
    b.edge_glow(spine, edge, 0.0036, 0.0006, stop=0.97)
    # Habaki collar, seppa washers and the tsuba.
    b.box((0.038, 0.014, 0.024), (0.0, 0.0, 0.163), "brass", bevel=0.002)
    b.cyl(0.026, 0.146, 0.150, "brass", sy=0.75)
    b.cyl(0.044, 0.139, 0.146, "brass", sy=0.82, segs=40)
    b.cyl(0.041, 0.137, 0.148, "dark", sy=0.8, segs=40)
    for side in (-1.0, 1.0):
        b.cyl(0.006, 0.1365, 0.1485, "glow", x=side * 0.026, segs=12)
    b.cyl(0.026, 0.131, 0.137, "brass", sy=0.75)
    # Tsuka: ray-skin core, crossed wrap, menuki, fuchi and kashira.
    b.box((0.03, 0.022, 0.25), (0.0, 0.0, 0.003), "skin", bevel=0.006)
    b.wrap(-0.115, 0.12, 0.015, 0.011)
    for face in (-1.0, 1.0):
        b.box((0.012, 0.004, 0.03), (0.003, face * 0.0128, 0.02), "brass", bevel=0.0015)
    b.cyl(0.019, 0.118, 0.131, "dark", sy=0.76)
    b.box((0.034, 0.026, 0.02), (0.0, 0.0, -0.128), "dark", bevel=0.006)


def straight_blade(b, base, length, w0, w1, tip, double, ridge=0.0035, glow="edge"):
    """A straight blade from `base`; `tip` is the share of length in the point."""
    n = 16
    half_widths = [w0 * 0.5 + (w1 - w0) * 0.5 * (i / (n - 1)) for i in range(n)]

    def taper(i):
        t = i / (n - 1)
        if t <= 1.0 - tip:
            return 1.0
        return max(0.0, 1.0 - ((t - (1.0 - tip)) / tip) ** 1.25)

    z = [base + length * i / (n - 1) for i in range(n)]
    if double:
        center = [(0.0, z[i]) for i in range(n)]
        for side in (-1.0, 1.0):
            edge = [(side * half_widths[i] * taper(i), z[i]) for i in range(n)]
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
        spine = [(half_widths[i], z[i]) for i in range(n)]
        edge = []
        for i in range(n):
            t = i / (n - 1)
            ex = -half_widths[i]
            if t > 1.0 - tip:
                # Angular, tanto-style point rising to the spine.
                k = (t - (1.0 - tip)) / tip
                ex = ex + (spine[i][0] - ex) * k
            edge.append((ex, z[i]))
        edge[-1] = spine[-1]
        b.wedge(spine, edge, ridge, 0.0006, "steel")
        b.edge_glow(spine, edge, ridge, 0.0006, inset=(0.0012, 0.005), stop=0.98)
        # A dark spine bar for the tactical look.
        b.box((0.003, ridge * 2.2, length * (1.0 - tip) * 0.85),
              (half_widths[0] - 0.0015, 0.0, base + length * (1.0 - tip) * 0.45), "dark")


def dual_blades(b):
    straight_blade(b, 0.098, 0.46, 0.036, 0.03, 0.12, double=False, ridge=0.0034)
    b.box((0.07, 0.022, 0.012), (0.0, 0.0, 0.09), "dark", bevel=0.003)
    b.box((0.012, 0.024, 0.016), (-0.032, 0.0, 0.096), "steel", rot=(0, -0.5, 0), bevel=0.002)
    b.box((0.028, 0.02, 0.15), (0.0, 0.0, 0.012), "skin", bevel=0.005)
    b.wrap(-0.058, 0.08, 0.014, 0.01, step=0.024)
    b.box((0.032, 0.024, 0.016), (0.0, 0.0, -0.066), "dark", bevel=0.005)
    b.cyl(0.006, -0.08, -0.072, "glow", segs=12)


def claymore(b):
    straight_blade(b, 0.215, 1.25, 0.074, 0.05, 0.1, double=True, ridge=0.0045, glow="fuller")
    # Cross guard with angled quillons and a ricasso block.
    b.box((0.2, 0.034, 0.03), (0.0, 0.0, 0.2), "dark", bevel=0.006)
    for side in (-1.0, 1.0):
        b.box((0.07, 0.028, 0.024), (side * 0.125, 0.0, 0.218), "steel",
              rot=(0.0, -side * 0.45, 0.0), bevel=0.005)
        b.sphere(0.014, (side * 0.158, 0.0, 0.238), "brass")
    b.box((0.05, 0.03, 0.04), (0.0, 0.0, 0.233), "steel", bevel=0.004)
    b.cyl(0.006, 0.226, 0.24, "glow", y=-0.016, segs=12)
    # Long two-hand grip with leather rings, collar and pommel.
    b.cyl(0.019, -0.2, 0.186, "skin", sy=0.85)
    b.rings(-0.19, 0.18, 0.0205, step=0.022)
    b.cyl(0.024, 0.17, 0.188, "dark", sy=0.85)
    b.sphere(0.034, (0.0, 0.0, -0.225), "dark", sy=0.75, sz=0.85)
    b.cyl(0.012, -0.236, -0.214, "glow", y=-0.024, segs=16)


def baton(b):
    b.cyl(0.021, -0.11, 0.14, "wrap", segs=20)
    b.rings(-0.1, 0.13, 0.0225, mat="dark", step=0.024, width=0.008)
    b.cyl(0.025, -0.13, -0.11, "dark", segs=20)
    b.cyl(0.024, 0.14, 0.17, "steel", r2=0.019, segs=20)
    b.cyl(0.017, 0.17, 0.74, "dark", segs=20)
    for z in (0.42, 0.5, 0.58, 0.66):
        b.cyl(0.0185, z, z + 0.022, "glow", segs=20)
        b.cyl(0.0195, z - 0.006, z, "steel", segs=20)
    b.cyl(0.022, 0.74, 0.77, "steel", segs=20)
    for side in (-1.0, 1.0):
        b.box((0.006, 0.01, 0.06), (side * 0.013, 0.0, 0.8), "steel", bevel=0.0015)
    b.box((0.016, 0.004, 0.05), (0.0, 0.0, 0.8), "glow")


def gauntlet(b):
    # Bracer up the forearm (-Z), banded, with glowing seams.
    b.cyl(0.058, -0.27, -0.05, "dark", r2=0.054, segs=8)
    for z in (-0.25, -0.18, -0.11):
        b.cyl(0.061, z, z + 0.024, "steel", segs=8)
    for side in (-1.0, 1.0):
        b.box((0.006, 0.004, 0.19), (side * 0.04, -0.056, -0.155), "glow")
    b.cyl(0.062, -0.06, -0.03, "steel", r2=0.066, segs=8)
    # Back of the hand, knuckle plates and a glowing knuckle bar.
    b.box((0.11, 0.05, 0.1), (0.0, 0.0, 0.03), "dark", bevel=0.008)
    b.box((0.1, 0.012, 0.08), (0.0, -0.03, 0.03), "steel", bevel=0.004)
    for i in range(4):
        x = -0.039 + i * 0.026
        b.box((0.024, 0.04, 0.03), (x, -0.004, 0.096), "steel", bevel=0.005)
        for k, z in enumerate((0.125, 0.15)):
            b.box((0.021, 0.034, 0.024), (x, 0.004 + k * 0.01, z), "dark", bevel=0.004)
    b.box((0.11, 0.008, 0.012), (0.0, -0.026, 0.095), "glow")
    b.box((0.03, 0.04, 0.06), (0.065, 0.004, 0.05), "dark", rot=(0, 0.3, 0), bevel=0.006)


def whip(b):
    b.cyl(0.019, -0.1, 0.14, "wrap", segs=20)
    b.rings(-0.09, 0.13, 0.0205, mat="dark", step=0.02, width=0.006)
    b.cyl(0.024, -0.125, -0.1, "dark", segs=20)
    b.cyl(0.006, -0.13, -0.124, "glow", segs=12)
    b.cyl(0.022, 0.14, 0.175, "steel", r2=0.012, segs=20)
    # A segmented lash drooping in a gentle S, glowing core through it.
    pts = []
    for i in range(30):
        t = i / 29
        pts.append(Vector((0.05 * math.sin(t * math.pi * 1.3) - 0.03 * t, 0.0, 0.175 + 1.65 * t)))
    for i in range(len(pts) - 1):
        a, c = pts[i], pts[i + 1]
        mid = (a + c) * 0.5
        r = 0.011 * (1.0 - 0.6 * i / len(pts))
        b.rod_between(a.lerp(mid, 0.15), c.lerp(mid, 0.15), r, "dark", segs=8)
        b.rod_between(a, c, r * 0.55, "glow", segs=6)


def spear(b):
    b.cyl(0.016, -0.63, 1.27, "dark", segs=16)
    b.rings(-0.08, 0.22, 0.0172, step=0.02)
    b.rings(0.62, 0.8, 0.0172, step=0.02)
    for z in (-0.11, 0.24, 0.6, 0.82):
        b.cyl(0.019, z, z + 0.014, "steel", segs=16)
    b.cyl(0.017, -0.68, -0.63, "steel", r2=0.019, segs=16)
    b.cyl(0.001, -0.74, -0.68, "steel", r2=0.016, segs=16)
    b.cyl(0.02, 1.25, 1.31, "steel", r2=0.016, segs=16)
    b.cyl(0.021, 1.272, 1.284, "glow", segs=16)
    # Leaf head: widest a third of the way up.
    n, base, length = 18, 1.3, 0.3
    center = [(0.0, base + length * i / (n - 1)) for i in range(n)]
    for side in (-1.0, 1.0):
        edge = []
        for i in range(n):
            t = i / (n - 1)
            w = 0.034 * math.sin(math.pi * min(1.0, t / 0.33) * 0.5) * (1.0 - max(0.0, t - 0.33) / 0.67) ** 0.9
            edge.append((side * max(w, 0.004 * (1 - t)), center[i][1]))
        edge[-1] = center[-1]
        b.wedge(center, edge, 0.0045, 0.0006, "steel")
        b.edge_glow(center, edge, 0.0045, 0.0006, inset=(0.001, 0.0045), stop=0.92)
    for side in (-1.0, 1.0):
        b.box((0.008, 0.004, 0.09), (0.0, side * 0.018, 1.23), "steel", bevel=0.0015)


def scythe(b):
    b.cyl(0.017, -0.33, 1.2, "dark", segs=16)
    b.rings(-0.2, 0.15, 0.0182, step=0.02)
    b.rings(0.55, 0.78, 0.0182, step=0.02)
    for z in (-0.33, 0.16, 0.54, 0.79):
        b.cyl(0.02, z, z + 0.016, "steel", segs=16)
    b.box((0.05, 0.034, 0.07), (0.0, 0.0, 1.2), "steel", bevel=0.006)
    b.box((0.03, 0.03, 0.05), (0.03, 0.0, 1.22), "dark", rot=(0, -0.6, 0), bevel=0.004)
    # Crescent blade sweeping out to -X, edge on its lower, inner curve.
    n = 26
    spine = curve(n, lambda t: (-0.025 - 0.68 * t, 1.215 + 0.09 * math.sin(math.pi * t * 0.85) - 0.17 * t * t))
    edge = []
    for i in range(n):
        t = i / (n - 1)
        x, z = spine[i]
        w = 0.085 * (1.0 - t) ** 0.75 + 0.0005
        edge.append((x + 0.012 * t, z - w))
    edge[-1] = spine[-1]
    b.wedge(spine, edge, 0.0045, 0.0006, "steel")
    b.edge_glow(spine, edge, 0.0045, 0.0006, inset=(0.0015, 0.008), stop=0.96)


def axe(b):
    b.cyl(0.018, -0.16, 0.66, "dark", segs=16)
    b.wrap(-0.14, 0.2, 0.018, 0.018, step=0.026)
    b.cyl(0.022, -0.18, -0.15, "steel", segs=16)
    b.cyl(0.02, 0.2, 0.215, "steel", segs=16)
    b.box((0.05, 0.04, 0.13), (0.0, 0.0, 0.6), "steel", bevel=0.006)
    b.box((0.008, 0.042, 0.1), (0.0, 0.0, 0.6), "glow")
    # Bearded crescent bit on -X.
    n = 22
    spine = [(-0.024, 0.55 + 0.11 * i / (n - 1)) for i in range(n)]
    edge = []
    for i in range(n):
        t = i / (n - 1)
        a = math.pi * (t - 0.5) * 0.95
        edge.append((-0.03 - 0.15 * math.cos(a) ** 0.8, 0.6 + 0.12 * math.sin(a)))
    b.ribbon([((sx, sz, 0.011), (ex, ez, 0.0008)) for (sx, sz), (ex, ez) in zip(spine, edge)], "steel")
    b.ribbon([((x + 0.002, z, 0.0016), (x + 0.012, z, 0.0034)) for x, z in edge[1:-1]], "glow")
    # Back spike on +X.
    b.prism([(0.024, 0.57, 0.01), (0.11, 0.6, 0.002), (0.024, 0.635, 0.01)], "dark")


def hammer(b):
    b.cyl(0.02, -0.18, 0.7, "dark", segs=16)
    b.wrap(-0.16, 0.22, 0.02, 0.02, step=0.026)
    for z in (-0.19, 0.23, 0.68):
        b.cyl(0.024, z, z + 0.02, "steel", segs=16)
    b.box((0.26, 0.17, 0.17), (0.0, 0.0, 0.79), "dark", bevel=0.014)
    for side in (-1.0, 1.0):
        b.box((0.035, 0.19, 0.19), (side * 0.145, 0.0, 0.79), "steel", bevel=0.012)
        b.box((0.012, 0.15, 0.15), (side * 0.168, 0.0, 0.79), "dark", bevel=0.006)
        for k in range(3):
            b.box((0.12, 0.004, 0.012), (0.0, side * 0.086, 0.75 + k * 0.04), "glow")
    b.box((0.06, 0.05, 0.05), (0.0, 0.0, 0.9), "steel", bevel=0.008)
    b.cyl(0.001, 0.92, 0.99, "steel", r2=0.022, segs=12)


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
    scene.render.resolution_x, scene.render.resolution_y = 420, 900
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
    for name in CLASSES:
        for other in CLASSES:
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
    for name in CLASSES:
        builder = Builder(name)
        BUILDERS[name](builder)
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
