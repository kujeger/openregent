# A shadow needs a destination

Indices 247–254 are the control band: 247–250 darken, 251–254 lighten. They
operate on the pixel already on screen (see [formats/art.md](../formats/art.md)),
so a sprite drawn alone shows the palette's magenta placeholder.

`regent_core::view::Sprite::ink` is the blitter's branch: a hole, a colour, or a
move along the destination's value (HSV, as HD's tables:
[formats/terrain.md](../formats/terrain.md#the-colour-on-screen)).

- **Which blitter.** The branch is the world blitter's. Interface art uses those
  indices as colours; applying it there punches holes in every panel picture
  and every unit icon. `Blit` selects: the browser takes it from the set's
  kind, the map's icons use `Blit::Interface`. The engine instead reads the
  tile's draw mode, `tile::Mode`; the map reads it for the translucent
  modes 5 and 8, and for mode 1's ramps (`soft::Ramps`). See
  [formats/art.md](../formats/art.md#the-draw-mode-picks-the-blit).
- **The browser draws on a backdrop.** `Sheet::draw_with` composites on the
  CPU over a swatch, a shadow by its soft factor (`soft::shaded`), default
  lit dirt measured in an HD capture.
  It is also the camera's clear colour. It reads every world tile as mode 7.
- **The map flattens nothing**: shadows land on whatever is drawn beneath. It
  sends the tile as stored — one byte a pixel, palette beside it — and draws
  the band in a second pass.

## Two modes

The engine moves each pixel's HSV value through tables as it draws, so
shadows stack ([formats/art.md](../formats/art.md#what-the-band-does-from-the-blitter),
`view::raster::stack`). A fragment shader cannot read the pixel beneath.
`[enhancements] shadows` picks how the band reaches the picture
(`regent_draw::soft`, `material::Band`):

```text
 soft (default)  band darken  alpha = min(dst, 255 − cover × 254)   blend op Min
                 band glow    colour × factor                        in draw order
                 resolve      colour × (1 − c (1 − STRENGTH))        c = (255 − alpha) / 254
 blend           band         colour × factor                        in draw order
                 resolve      none
```

Both then put every pixel on the 565 grid (`shadow.wesl`). Each index's
factor is one of `soft::DARK`, the `(50, 6)` family's eight percents
fitted on linear colour, within two of 64 value steps of `Shade::over`
over every colour (`the_factors_keep_the_engines_value`); mode 7's
glows `soft::GLOW`. A tile's draw mode picks the indices' factors
(`soft::Ramps`, `each_mode_takes_its_own_factors`): mode 7's 247–250 are
50, 62, 74, 86 %, 251–254 glow; mode 1's eight darken, 50 to 92 %. Every
other mode takes mode 7's.

`soft`: Min, not a sum, so overlaps do not stack; hue kept, no HSV
tables. `blend`: shadows stack in draw order, and things drawn after a
shadow stay unshaded; a glow is a gain capped at twice, so black stays
black. Neither rounds hue and saturation to five bits as the engine's
tables do.

The glow is weaker than the engine's, accepted. The engine adds 9, 13,
17, 21 of 64 value steps (**Binary**), so black turns grey; the
multiply caps at twice. On HD captures, over half their pixels dark,
the mean error grows from 251 to 254, and is worst on dark ground. A one-draw blend on the `Rgba8UnormSrgb`
target gives a gain up to twice or `m × dst + c` with `m` ≤ 1, not
both; a second, additive draw per glow tile would fit within 1.4.

Soft coverage resolves last, under everything, so a translucent thing
lifts it where at least half drawn (`Pass::Lift`, `soft::lifts`,
**Inferred**); under `blend` the lift writes an alpha nothing reads.
Hit point bars sit on `present::OVERLAY` past the
resolve, or, with nearest, in the world camera, where their alpha blend
raises alpha: coverage lifted by their opacity (**Inferred**, from the
blend state). `soft::over` models the arithmetic for the tests.

## Where a soft shadow lies

The engine sorts a shadow pixel with its caster, at the caster's row
(**Binary**). 87 % of the Palace's darkening pixels lie in front
of its anchor (tile 1036, `the_palaces_shadow_lies_in_front_of_it`), so a unit standing in its
shadow is drawn after it and never darkened. `soft` alone
(`regent_draw::lie`) tests each darkening pixel at

```text
 depth = max(caster's, Depth::at(row of its run's foot))
 run's foot   the lowest darkening pixel below it, same column, before a hole
 its row      the quad's top row (MeshTag) + tile rows / (32 · sin 55°)
```

- **The foot, not the pixel.** A body is upright: the shadow pixels over a
  unit's head lie on ground behind it. Tested at the pixel's own ground, a
  unit standing in a shadow would take none of it; at the run's foot it
  takes it where it stands between caster and foot.
- **The caster's depth is a floor.** A shadow never darkens less than the
  engine's: shadows cast behind (trees, the Wizard Guild) look as the engine's,
  and a set's later layer still darkens its earlier ones (28,618 pixels
  of HD's sets, frame 0, slot 0, where a later layer's shadow behind the
  anchor covers an earlier layer). Cost: a layer's shadow cast in front
  darkens a later layer of its set where they overlap, 5,190 pixels in
  all, a few a set (`a_sets_own_layers_take_its_shadow_mostly_as_the_engines`). A body cannot be told from its caster's in a depth
  test, and the 2D pipeline has no stencil.
- **Flat ground, the caster's height** (**Inferred**: the art paints shadows
  for flat ground). The row is the caster's (`lie::Footing`: its cell's row
  and its ground point's height) plus the screen distance below that
  point, so a flier's shadow part, drawn on the ground, lies about its
  ground point, and a body drawn aloft about its own.
- **A table, not a formula.** `Depth::at` is ranks, not linear in the row,
  so `lie::ROWS` holds it 16 samples a cell, rebuilt as the placed rows
  change; the shader writes `frag_depth`. Pieces between two placed rows
  sort by row within them (`Depth::at`), so the comparison holds between
  rows too.
- **Glows and untagged bands** test at their caster's depth: a band with
  no `Footing` (effects, trails, the placing preview) keeps no `MeshTag`.

## Passes

1. **Bodies**, alpha-masked, in `AlphaMask3d`, writing depth. The art has binary
   transparency.
2. **Shadows**, in the later `Transparent3d`, depth-tested against bodies,
   writing no depth.
3. **One full-screen pass** (`shadow::resolve`), straight into the camera's
   output (window or world image), cleared to the clear colour and cut to
   its viewport: Bevy's own copy (`upscaling`) is skipped for that camera
   (`CameraOutputMode::Skip`), and `shadow::attach` gives it the output
   Bevy gives only a camera that copies. On the menus the world camera is
   off (`draw::world_shown`); egui's clears the window.

Order comes from the depth test, not a sort key: a shadow is rejected where
something nearer stands, nearer than its caster or, under `soft`, than
the ground it lies on ([Where a soft shadow lies](#where-a-soft-shadow-lies)).

Every shader compiles, headless, with the defs each material's
`specialize` gives (`regent_draw::compile`, the veil's too): a WESL error
otherwise shows only in the log.

Both materials in `AlphaMask3d` (or 2D's `AlphaMask2d`) would fail. That phase is binned (grouped by pipeline
and texture, not depth order), so a body can draw after a shadow and overwrite it.
Shadows would land on ground but not on buildings.

## Three things the data forced

- **Depth by rank, not position.** The mesh pipeline compares with `GreaterEqual`,
  so equal depths pass and two objects on one row tie, letting a shadow paint
  over a body. Sort once and number. The depth band is 1,000 units wide:
  orthographic projection maps z linearly, so 9,000 objects within a fraction
  of a unit quantise back into ties.
- **One ruin is direct colour.** 47 of 73,219 frame references carry no palette
  index: 38 `Interface` portraits (never on this path) and nine frames of the
  Fairgrounds collapsing (`a_units_frames_are_indexed_bar_the_fairgrounds_ruin`). A third material, `plain::Plain` (2D's
  `ColorMaterial`, ported) in the same phase, draws them; with no index they cannot hold a shadow.
- **Both cameras must agree on MSAA.** They share the window's colour target but
  allocate separate depth attachments; 1 vs 4 samples fails validation at
  startup. Off on both: right for nearest-sampled art, and needed since
  multisampling would average one ramp index into another.

