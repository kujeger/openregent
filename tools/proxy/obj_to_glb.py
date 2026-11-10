"""An OBJ shadow proxy as glTF binary, as check.py exports one: one node
per object, Blender's X, Y, Z as glTF X, Z, -Y, one unit an art pixel.

    blender -b --factory-startup --python tools/proxy/obj_to_glb.py -- IN.obj OUT.glb

The OBJ is Z-up, as local/models wrote them.
"""
import sys

import bpy

source, target = sys.argv[sys.argv.index("--") + 1:]

bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.ops.wm.obj_import(filepath=source, forward_axis="Y", up_axis="Z")
bpy.ops.export_scene.gltf(filepath=target, export_format="GLB", export_materials="NONE")
