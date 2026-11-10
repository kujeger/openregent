# Notes for an artist modelling buildings

Model a shadow proxy in a template from `tools/proxy` (`template.sh CODE`;
its README tells the rest). The template sets the units, camera and sun:

| | |
|---|---|
| Units | 1 art pixel; origin the sprite's anchor on the ground; Z up |
| Camera | Orthographic, yawed 45°, tilted 35° (**Inferred**) |
| Sun | φ 29°, elevation 50° (**Game**, by eye) |

Evidence: [original/art-camera.md](../original/art-camera.md). The art was not
drawn through the engine's camera or under its terrain sun. The rest of this
page is the engine's terms; do not model to them. All values are read from the
game; derived ones say so.

Constants: [`data::settings`](../../crates/regent_core/src/data/settings/mod.rs). Workings:
[formats/saves.md](../formats/saves.md). Art: [formats/art.md](../formats/art.md).

## The engine's units

1 unit = 1 terrain cell = 32 px at zoom 1 (`settings::TILE_PIXELS`). Height
uses the same unit as X and Y.

A sprite's origin is the tile anchor at `+0x0a`: where the unit stands, the
middle of the footprint on the ground, not the image corner.

## The engine's camera

How the engine draws the ground, not how the art was drawn.

| | |
|---|---|
| Projection | Orthographic — no perspective divide in the setup or its inverse |
| Pitch | 55° above the horizon |
| Yaw | None |
| Cell on screen | 32 px wide, 26.21 px tall |

Despite the `Iso*` class names this is not a 45°-rotated diamond view: world X
maps straight to screen X.

```text
screen_x =  32*scale * world_x
screen_y =  sin(55°) * 32*scale * world_y   ; depth, +y toward the viewer
         -  cos(55°) * 32*scale * world_z   ; height, lifts up the screen

sin(55°) * 32 = 26.21 px per cell of depth
cos(55°) * 32 = 18.35 px per unit of height
```

**Binary.** `IsoCameraAngle` (case 34 in `MajestyHD.exe`) has static
initialiser 55 and no shipped file sets it. `SetCameraAngle` and `SetScale`
build the two terms. The height term is settled by elimination:
`sin(a)*32*scale` divides screen distances at four call sites;
`cos(a)*32*scale` has no forward use found.

**Viewer.** `scale` is 1 at the regular zoom: 83 cells measure 2,656 px on a
stitched screenshot, and a free fit gives 32.00 px per cell at 55.0°, 0.00 px
rms, lift included (**Measured**).
The art is drawn for this zoom; the game has a second.

## The terrain's sun

Lights the terrain; the art's sun is another
([original/art-camera.md](../original/art-camera.md#what-the-numbers-say)).
`GOPT` stores an unnormalised direction.

| | |
|---|---|
| Direction | `(0.90, 0.90, 1.0)` — `LightX`, `LightY`, `LightZ` |
| Azimuth | 45°, toward the screen's lower right |
| Elevation | 38.1° — `atan(1.0 / sqrt(0.9² + 0.9²))` |
| Diffuse | `Contrast` 1.5, `Ambient` 0.35 |

**Binary.** The level normalises the vector and scales it by `Contrast`; the
map bakes the vertex shade from it. `regent_core::sim::light` reproduces 99.97%
of baked bytes over the Windows saves from that vector alone, nothing fitted
(**Measured**).

**Corpus.** The azimuth predicts the drawn shadows: the darkening band lies left
of the anchor on all 57 characters that have one
(`a_characters_shadow_falls_to_the_left_of_its_anchor`).

## How the terrain sun's shadow lands on screen

Under the engine's camera and the terrain sun; the art's shadows fall
otherwise.

Direction does not depend on elevation: a shadow runs up and left at 39.3° from
horizontal (`atan(26.21 / 32)`, the ground direction away from the sun,
foreshortened).

Length does. Offset of the shadow of a point one cell (32 px) above ground,
derived, not measured off the art:

| Sun elevation | Offset |
|---|---|
| 45° | 29 px |
| 38° | 37 px |

The stored vector gives 38°, so 37 px per cell of height.

## What the art already gives you

- **A footprint polygon.** Many `IMAG` slots carry point lists of `(i32, i32)`
  pairs about the anchor (`Slot::point_lists` in `data::imag`). A building's
  list 0 is the closed polygon of its base; the engine paints bare ground over
  exactly that shape when the building is placed (**Binary**). `ABY1 Wizards_Guild1`: a closed hexagon about 236×145;
  `ABJ1 Palace1`: 19 points. In HD's `Data`, the lists account for every byte of
  168 of 168 slots that have them, 195 lists in all
  (`a_slots_point_run_is_a_chain_of_lists`).
- **A silhouette.** The opaque extent bounds height but not depth: from one
  view, a spire and a distant tall wall project alike.
- **A baked shadow.** Indices 247–250 (the darkening half of the control band;
  251–254 lighten) hold the building's own mass projected along the sun. A
  second view from a known angle, and the best check of a proxy mesh's shape.

Buildings flatten the ground under them, so model the base flat.

## Not settled

- The art's camera and sun: fitted, not read from the game
  ([original/art-camera.md](../original/art-camera.md#open)).
