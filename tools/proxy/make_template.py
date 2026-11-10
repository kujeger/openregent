"""Make a Blender modelling template for one building's shadow proxy,
from its descriptor code alone.

    blender -b -P make_template.py -- <CODE> [--set inactive] [--out DIR]
                                     [--reference MODEL.glb|.obj]
    or: ./template.sh <CODE> [options]

Fetches what it needs from the game data:
    art.rs (a cargo script on the regent_core library, nightly cargo) composes
    the set's layers, frame 0, in anchor coordinates: the picture, a mask
    of body and painted-shadow pixels, the anchor's pixel and the base
    polygon.
The game data is MAJESTY_HD, else the repo's .env.

Writes <DIR>/<CODE>_template.blend (DIR default: ./<CODE>), with the
picture and masks packed in, check.py and a README as texts, and the
building's model in local/models, if it has one (or --reference:
a glTF or OBJ), as a hidden reference.

One unit is one art pixel, about the sprite's anchor on the ground: X, Y
the building's ground axes, Z up. The art camera (orthographic, yaw 45,
elevation 35, Inferred in docs/original/art-camera.md) shows the picture behind
the model; the sun is phi 29, elevation 50.
"""
import argparse
import glob
import json
import math
import os
import subprocess
import sys

import bpy
import numpy as np
from mathutils import Vector

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))
MODELS = os.path.join(REPO, "local/models")
SUN_PHI, SUN_EL = 29.0, 50.0
GROUND = 3000            # ground plane side, art pixels
FAR = 2000               # camera distance from the anchor
BODY, SHADOW = 128, 255  # art.rs's mask values; 0 is a hole


def arguments():
    parser = argparse.ArgumentParser(prog="make_template.py")
    parser.add_argument("code", help="descriptor code, e.g. ABV1")
    parser.add_argument("--set", default="inactive", help="set kind, e.g. inactive, active")
    parser.add_argument("--out", help="output directory (default ./<CODE>)")
    parser.add_argument("--reference", help="an earlier model, glTF or OBJ (default: the repo's, by code)")
    return parser.parse_args(sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else [])


def game_dirs():
    """The HD install's Data and DataMX, from MAJESTY_HD or .env."""
    hd = os.environ.get("MAJESTY_HD")
    if not hd:
        for line in open(os.path.join(REPO, ".env")):
            key, _, value = line.strip().partition("=")
            if key == "MAJESTY_HD":
                hd = value if os.path.isabs(value) else os.path.join(REPO, value)
    return [os.path.join(hd, d) for d in ("Data", "DataMX") if os.path.isdir(os.path.join(hd, d))]


def fetch(code, kind, work):
    """Run art.rs: the picture, mask, anchor and base polygon of pose 0."""
    run = subprocess.run(
        [os.path.join(HERE, "art.rs"), ":".join(game_dirs()), work, code, kind],
        capture_output=True, text=True, check=True,
    )
    rows = [json.loads(line) for line in run.stdout.splitlines() if line.startswith("{")]
    pose = next(r for r in rows if r.get("pose") == 0)
    polygon = next((r["polygon"] for r in rows if "polygon" in r), None)
    return (
        netpbm(os.path.join(work, f"{code}_{kind}_p0.ppm")),
        netpbm(os.path.join(work, f"{code}_{kind}_p0.pgm")),
        tuple(pose["origin"]),
        polygon,
    )


def netpbm(path):
    """A binary PGM or PPM as an array, rows top down."""
    data = open(path, "rb").read()
    fields, at = [], 0
    while len(fields) < 4:
        while data[at:at + 1].isspace():
            at += 1
        start = at
        while not data[at:at + 1].isspace():
            at += 1
        fields.append(data[start:at])
    width, height = int(fields[1]), int(fields[2])
    channels = 3 if fields[0] == b"P6" else 1
    pixels = np.frombuffer(data[at + 1:], np.uint8, width * height * channels)
    return pixels.reshape(height, width, channels)


def packed(name, rgba):
    """`rgba` (rows top down, 0..1) as a packed image named `name`."""
    height, width = rgba.shape[:2]
    image = bpy.data.images.new(name, width, height, alpha=True)
    image.pixels.foreach_set(rgba[::-1].astype(np.float32).ravel())
    image.pack()
    image.use_fake_user = True
    return image


def reference_model(code, given):
    """An earlier model, glTF or OBJ: given, or the repo's for this building."""
    shipped = os.path.join(MODELS, f"{code}.glb")
    return given or (shipped if os.path.exists(shipped) else None)


def build(code, kind, out, reference):
    work = os.path.join(out, "art")
    os.makedirs(work, exist_ok=True)
    picture, mask, anchor, polygon = fetch(code, kind, work)
    height, width = mask.shape[:2]
    body, painted = mask[..., 0] == BODY, mask[..., 0] == SHADOW

    bpy.ops.wm.read_factory_settings(use_empty=True)
    scene = bpy.context.scene
    scene["building"] = code
    scene["tile"] = (width, height)
    scene["anchor"] = anchor
    if polygon:
        scene["polygon"] = [c for p in polygon for c in p]

    # Plain units: a typed 12 is 12 art pixels.
    scene.unit_settings.system = "NONE"

    model = bpy.data.collections.new("model")
    old = bpy.data.collections.new("reference (old model)")
    helpers = bpy.data.collections.new("helpers")
    for c in (model, old, helpers):
        scene.collection.children.link(c)

    if reference:
        # Into the reference collection, axes as they are: Z up, no turn.
        layer = bpy.context.view_layer
        layer.active_layer_collection = layer.layer_collection.children[old.name]
        if reference.lower().endswith(".obj"):
            bpy.ops.wm.obj_import(filepath=reference, forward_axis="Y", up_axis="Z")
        else:
            # glTF is Y up; Blender's importer turns it Z up.
            bpy.ops.import_scene.gltf(filepath=reference)
        tint = bpy.data.materials.new("reference")
        tint.diffuse_color = (0.8, 0.5, 0.3, 1)
        for obj in old.objects:
            obj.data.materials.clear()
            obj.data.materials.append(tint)

    h = GROUND / 2
    ground = bpy.data.meshes.new("ground")
    ground.from_pydata([(-h, -h, 0), (h, -h, 0), (h, h, 0), (-h, h, 0)], [], [(0, 1, 2, 3)])
    helpers.objects.link(bpy.data.objects.new("ground", ground))

    # Art camera: right (1, -1, 0)/sqrt2, up (S, S, C sqrt2)/sqrt2; the
    # picture's middle in the middle of the frame.
    s, c = math.sin(math.radians(35)), math.cos(math.radians(35))
    right = Vector((1, -1, 0)).normalized()
    up = Vector((s, s, c * math.sqrt(2))).normalized()
    forward = up.cross(right)
    cam_data = bpy.data.cameras.new("art camera")
    cam_data.type = "ORTHO"
    cam_data.ortho_scale = max(width, height)
    cam_data.clip_start = 1
    cam_data.clip_end = FAR * 2
    cam = bpy.data.objects.new("art camera", cam_data)
    helpers.objects.link(cam)
    scene.camera = cam
    dx, dy = width / 2 - anchor[0], height / 2 - anchor[1]
    cam.location = right * dx - up * dy - forward * FAR
    cam.rotation_euler = forward.to_track_quat("-Z", "Y").to_euler()
    scene.render.resolution_x, scene.render.resolution_y = width * 4, height * 4

    # The picture (holes clear) and its painted shadow in red, behind the
    # model in camera view: Camera > Background Images.
    seen = (mask[..., 0] > 0)[..., None]
    sprite = np.concatenate([picture / 255.0, seen], axis=2)
    red = np.zeros((height, width, 4))
    red[painted] = (1.0, 0.1, 0.1, 1.0)
    cam_data.show_background_images = True
    for image, alpha in ((packed(f"sprite_{code}", sprite), 0.6), (packed(f"shadow_{code}", red), 0.5)):
        bg = cam_data.background_images.new()
        bg.image = image
        bg.alpha = alpha
        bg.display_depth = "FRONT"

    # The masks the check compares against.
    for name, m in (("mask_body", body), ("mask_shadow", painted)):
        packed(name, np.repeat(m[..., None], 4, axis=2).astype(float))

    phi, el = math.radians(SUN_PHI), math.radians(SUN_EL)
    az = math.atan2(-math.cos(phi) - math.sin(phi), math.cos(phi) - math.sin(phi))
    toward = Vector((math.cos(el) * math.cos(az), math.cos(el) * math.sin(az), math.sin(el)))
    sun_data = bpy.data.lights.new("sun", "SUN")
    sun_data.energy = 3
    sun_data.angle = math.radians(1)
    sun = bpy.data.objects.new("sun", sun_data)
    helpers.objects.link(sun)
    sun.rotation_euler = toward.to_track_quat("Z", "Y").to_euler()
    sun.location = toward * 300

    scene.world = bpy.data.worlds.new("world")
    scene.world.color = (0.05, 0.05, 0.05)

    layer = bpy.context.view_layer
    layer.layer_collection.children[old.name].hide_viewport = True
    layer.active_layer_collection = layer.layer_collection.children[model.name]

    readme = open(os.path.join(HERE, "README.txt")).read()
    readme = readme.replace("{CODE}", code).replace("{SET}", kind)
    readme = readme.replace("{REFERENCE}", reference or "none: no earlier model for this building")
    bpy.data.texts.new("README").from_string(readme)
    bpy.data.texts.new("check.py").from_string(open(os.path.join(HERE, "check.py")).read())

    for screen in bpy.data.screens:
        for area in screen.areas:
            if area.type == "VIEW_3D":
                space = area.spaces.active
                space.region_3d.view_perspective = "CAMERA"
                space.clip_end = FAR * 4
                space.overlay.show_floor = False
                space.overlay.show_axis_x = space.overlay.show_axis_y = False
                space.shading.type = "MATERIAL"
                space.shading.use_scene_lights = True
                space.shading.use_scene_world = True

    path = os.path.join(out, f"{code}_template.blend")
    bpy.ops.wm.save_as_mainfile(filepath=path)
    print(f"wrote {path}: {width}x{height}, anchor {anchor}, reference {reference}")


args = arguments()
out = os.path.abspath(args.out or args.code)
build(args.code, args.set, out, reference_model(args.code, args.reference))
