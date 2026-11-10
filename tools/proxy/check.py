"""Shadow proxy: check and export. Run with the Text Editor's Run Script
button (or Alt+P); it also adds a "Proxy" tab to the 3D view's sidebar
(N) with the same button.

Measures the meshes in the "model" collection, modifiers applied, against
the sprite:

    triangles      what the game will draw into its shadow map
    outline IoU    the model drawn through the art camera against the
                   sprite's body pixels (1.0 = exact)
    shadow IoU     the model's ground shadow under the scene's sun, minus
                   the body, against the sprite's painted shadow pixels

then exports those meshes as <building>.glb, no materials beside the .blend: one node
per object, named as the object. glTF is Y-up: Blender's X, Y, Z become
glTF X, Z, -Y; one unit is one art pixel.

Headless: blender -b ABV1_template.blend --python-text check.py
"""
import math
import os

import bpy
import numpy as np
from mathutils import Vector

S35, C35 = math.sin(math.radians(35)), math.cos(math.radians(35))
R2 = math.sqrt(2)
MODEL = "model"
REPORT = "report"


def mask(name):
    """A packed mask image as a boolean array, rows top down."""
    image = bpy.data.images[name]
    w, h = image.size
    px = np.array(image.pixels[:], dtype=np.float32).reshape(h, w, image.channels)
    return px[::-1, :, 0] > 0.5


def triangles(collection):
    """Every evaluated triangle of the collection's meshes, world space."""
    depsgraph = bpy.context.evaluated_depsgraph_get()
    out = []
    for obj in collection.all_objects:
        if obj.type != "MESH" or obj.hide_get():
            continue
        evaluated = obj.evaluated_get(depsgraph)
        mesh = evaluated.to_mesh()
        mesh.calc_loop_triangles()
        m = np.array(obj.matrix_world)
        verts = np.array([(*v.co, 1.0) for v in mesh.vertices]) @ m.T
        out += [verts[list(t.vertices), :3] for t in mesh.loop_triangles]
        evaluated.to_mesh_clear()
    return out


def raster(tris, size, anchor):
    """Coverage of `tris` drawn through the art camera at sprite pixels."""
    w, h = size
    cover = np.zeros((h, w), bool)
    for t in tris:
        x, y, z = t[:, 0], t[:, 1], t[:, 2]
        sx = (x - y) / R2 + anchor[0]
        sy = -((x + y) / R2 * S35 + z * C35) + anchor[1]
        lo = np.floor([sx.min(), sy.min()]).astype(int).clip(0, [w - 1, h - 1])
        hi = np.ceil([sx.max(), sy.max()]).astype(int).clip(0, [w - 1, h - 1])
        ys, xs = np.mgrid[lo[1]:hi[1] + 1, lo[0]:hi[0] + 1] + 0.5
        area = (sx[1] - sx[0]) * (sy[2] - sy[0]) - (sx[2] - sx[0]) * (sy[1] - sy[0])
        if abs(area) < 1e-9:
            continue
        w0 = ((sx[1] - xs) * (sy[2] - ys) - (sx[2] - xs) * (sy[1] - ys)) / area
        w1 = ((sx[2] - xs) * (sy[0] - ys) - (sx[0] - xs) * (sy[2] - ys)) / area
        inside = (w0 >= 0) & (w1 >= 0) & (1 - w0 - w1 >= 0)
        cover[(ys[inside] - 0.5).astype(int), (xs[inside] - 0.5).astype(int)] = True
    return cover


def sun_toward():
    """Unit vector toward the scene's sun: its local +Z."""
    sun = next(o for o in bpy.data.objects if o.type == "LIGHT" and o.data.type == "SUN")
    v = sun.matrix_world.to_3x3() @ Vector((0, 0, 1))
    return np.array(v.normalized())


def iou(a, b):
    union = (a | b).sum()
    return float((a & b).sum() / union) if union else 0.0


def check():
    scene = bpy.context.scene
    collection = bpy.data.collections[MODEL]
    size, anchor = tuple(scene["tile"]), tuple(scene["anchor"])
    body, painted = mask("mask_body"), mask("mask_shadow")

    tris = triangles(collection)
    drawn = raster(tris, size, anchor) if tris else np.zeros(body.shape, bool)

    # Each triangle slid down the sun's ray onto the ground, Z 0.
    toward = sun_toward()
    flat = [t - np.outer(t[:, 2] / toward[2], toward) for t in tris]
    cast = raster(flat, size, anchor) if flat else np.zeros(body.shape, bool)

    out = os.path.join(bpy.path.abspath("//"), f"{scene['building']}.glb")
    bpy.ops.object.select_all(action="DESELECT")
    # What was measured, no more: hidden objects stay out.
    for obj in collection.all_objects:
        obj.select_set(obj.type == "MESH" and not obj.hide_get())
    bpy.ops.export_scene.gltf(
        filepath=out, use_selection=True, export_apply=True, export_materials="NONE"
    )

    lines = [
        f"{scene['building']}: {len(bpy.context.selected_objects)} objects, {len(tris)} triangles",
        f"outline IoU {iou(drawn, body):.4f}",
        f"shadow IoU  {iou(cast & ~drawn, painted & ~body):.4f}",
        f"exported {out}",
    ]
    text = bpy.data.texts.get(REPORT) or bpy.data.texts.new(REPORT)
    text.from_string("\n".join(lines) + "\n")
    print("\n".join(lines))
    return lines


class PROXY_OT_check(bpy.types.Operator):
    """Measure the model against the sprite and export it as glTF"""

    bl_idname = "proxy.check"
    bl_label = "Check and export"

    def execute(self, context):
        lines = check()
        self.report({"INFO"}, " | ".join(lines[:3]))
        return {"FINISHED"}


class PROXY_PT_panel(bpy.types.Panel):
    bl_label = "Shadow proxy"
    bl_space_type = "VIEW_3D"
    bl_region_type = "UI"
    bl_category = "Proxy"

    def draw(self, context):
        self.layout.operator("proxy.check")
        text = bpy.data.texts.get(REPORT)
        for line in (text.as_string().splitlines() if text else []):
            self.layout.label(text=line)


for cls in (PROXY_OT_check, PROXY_PT_panel):
    if not hasattr(bpy.types, cls.__name__):
        bpy.utils.register_class(cls)

check()
