Shadow proxy template: {CODE} ({SET} set)

Units: one Blender unit is one art pixel. The origin is the sprite's
anchor on the ground; X and Y are the building's ground axes, Z is up.

The view looks through the art camera (orthographic, 45 / 35 degrees) with
the sprite behind the model. Camera > Background Images: fade or hide the
sprite and the red painted-shadow layer. Numpad 0 returns to the camera.

Collections
  model                    build here; every mesh in it is the proxy
  reference (old model)    the building's model in local/models, if any,
                           hidden: unhide to compare or copy parts from
  helpers                  camera, sun, ground; not exported

Modelling tips
  - Keep it low: round parts as 6-sided cylinders (Add > Mesh > Cylinder,
    Vertices 6), no detail the outline or shadow cannot show.
  - Name objects meaningfully (tower, keep, merlon_n_1, ...): the names
    become glTF node names.
  - Doorways: a Boolean modifier (Difference) with a cutter object kept
    outside "model". Merlon rows: an Array modifier. Modifiers are applied
    on export.
  - Closed solids work best for shadows.

Check and export
  Open check.py in the Text Editor and press Run Script (or Alt+P). It
  prints the triangle count, the outline IoU against the sprite and the
  shadow IoU against the painted shadow into the "report" text, writes
  {CODE}.glb beside this file, and adds a "Proxy" tab (N sidebar) with a
  button to do it again.

  Reference model: {REFERENCE}
  The scene's sun is phi 29, elevation 50; rotate the "sun" object to try
  others.

Making another template
  tools/proxy/template.sh <CODE> [--set active] [--out DIR] [--reference MODEL.glb]
