# `DTRN` and `DTPT` — the tileset and the blend cases

> Part of [README.md](README.md).

## The tileset and its classes

**Corpus.** `DTRN` holds a table naming every terrain class; the tileset checks
against it.

`terrtype.cam`: one `DTRN`, ten `DTPT`. `mx_terrtype.cam`: one and eleven. A
`DTRN`'s `PRIM` is 173 bytes base, 221 expansion:

```
PRIM  len 173 / 221
0x00  char[4]  "GRIT", then "Gritty", then zero padding      61 bytes
0x3d  ATCH     len 0x60 / 0x90   version 0   children 1
0x4d    TPRT   len 0x50 / 0x80   version 0   slots 10 / 16
          slots x 8: [u32 0][char[4] DTPT id]
```

`len == slots * 8` on both, as for `DUNT`'s `MOVE`, `EXPL` and `WEAP`. The
fourth word counts slots, not children, so `Part::parse` does not walk an
`ATCH`; `data::desc::atch` reads both. Test:
`the_terrain_descriptor_names_one_texture_class_per_letter`.

A slot's `TPRT` index is its `MapTexture` letter: slot 0 `A`, slot 10 `K`. That
joins classes to art (`the_tileset_and_the_terrain_table_agree_on_the_letters`,
`the_tileset_descriptor_shares_its_id_with_its_art`):

```
terrtype.cam                       tilesetdata.cam
  DTRN "GRIT" ──same id──────────────> IMAG "GRIT"
    └─ TPRT slot 1 = "GR01" ─┐           └─ MapTextureBtoB, MapTextureAtoB
                             │              ^
  DTPT "GR01" Plains_parent <┘              slot 1 is B
```

Base classes: `GR00 Dirt`, `GR01 Plains_parent`, `GR02 Plains_child`,
`GR03 Arid_parent`, `GR04 Arid_child`, `GR05 Scorch_parent`,
`GR06 Scorch_child`, `GR07 Swamp_parent`, `GR08 Swamp_child`, `GR09 Snow`. The
expansion renames `GR09` to `Snow_parent` and adds `GR10 Snow_child`: 16 slots,
11 filled.

16 is `MAP_TEXTURE_CLASSES`, the width of the `MapTexture` id grid.

Take the letter from the table, never from record order. In `mx_terrtype.cam`
the `DTPT` records run `GR00, GR10, GR01, GR02, …`: `Snow_child` is record 1,
table slot 1 is `Plains_parent`. Base records are in table order, so record
order works there and fails silently on the expansion:
`the_expansion_table_order_is_not_the_archive_record_order`.

### The classes form a tree, and the art agrees

Every blend set joins a class to a tree neighbour: `Dirt` to a `*_parent`, or a
`*_parent` to its `*_child`. Eight blends base, ten expansion
(`every_blend_joins_a_terrain_class_to_its_tree_neighbour`).

```
   Dirt ── biome_parent ── biome_child

    A          B                C          Plains
    A          D                E          Arid
    A          F                G          Scorch
    A          H                I          Swamp
    A          J                K          Snow, expansion only
```

Five three-step ladders share `Dirt`. A map may hold several biomes (Day of
Reckoning has four), but the generator separates them with `Dirt`, so every
boundary is `Dirt`-to-parent, and within a biome parent-to-child. No
plains-to-swamp blend exists because none can occur.

`constants.rgs` agrees:

```
Plains_All_Parent     GR01:100
Plains_Mostly_Parent  GR01:78  GR02:30
Plains_Mostly_Child   GR02:76  GR01:24
Plains_All_Child      GR02:100
Plains_Dirt           GR00:26  GR01:74
```

Likewise for Arrid, Scorched and Swamp; Snow's mixes are only in the
expansion's chart. No mix crosses biomes, and none pairs `Dirt` with a child,
so no `AtoC` blend exists.

The rule is `rgs::blendable`. Tests: every `MapTexture` set in both tilesets
joins tree neighbours (`every_blend_joins_a_terrain_class_to_its_tree_neighbour`,
`the_terrain_classes_are_ladders_of_three_sharing_dirt`); every cell pair of the
corpus and every generated map obeys it
(**Measured**). A map
breaking it loads and draws holes. **Binary**: the original finds a pair's
blend by asking the tileset, not a table.

The base game names ten classes and draws nine: `GR09 Snow` has no body set
until the expansion, and `constants.rgs` references `GR00`–`GR08`, never `GR09`.

### Two negatives

- A `DTPT`'s `PRIM` is 29 zero bytes in all 21 records
  (`a_terrain_class_prim_is_twenty_nine_zero_bytes`). A `DTPT` is only its
  `HEAD` name and table position.
- The 61 bytes before `ATCH` are id, name and padding. Both datasets' single
  `DTRN` is `GRIT Gritty`, so there is nothing to compare.

## The seventeen blend cases

**Viewer** (drawn boards and seams) and **Corpus** (a save stores each cell's
slot number), independently. Solving corners out of a generated map reproduces
all 17 rows with nothing assumed. See
[saves.md](saves.md#the-17-blend-cases-recovered-from-the-data).

Every blend set has 17 slots
(`every_blend_set_has_seventeen_cases`). The tile's four corners pick one:

```
   TL ── TR      15 non-empty ways four corners can take the far terrain,
    │     │      plus a second slot for each of the two diagonals = 17
   BL ── BR
```

| Slot | Corners taking the far class | | Slot | Corners |
|---|---|---|---|---|
| 0 | all four | | 9 | all but TL |
| 1 | TR, BL — apart | | 10 | all but TR |
| 2 | TL, BR — apart | | 11 | all but BR |
| 3 | TR, BR — right edge | | 12 | all but BL |
| 4 | TL, TR — top edge | | 13 | TL |
| 5 | BL, BR — bottom edge | | 14 | TR |
| 6 | TL, BL — left edge | | 15 | BR |
| 7 | TL, BR — through | | 16 | BL |
| 8 | TR, BL — through | | | |

In code: `content::terrain::schema`'s `CORNER_SLOTS`.

So terrain lives on grid vertices, not cells: a `w` by `h` patch takes
`(w+1)(h+1)` classes, and `MTER` allocates exactly that many vertices.

### The board tests

Four boards, each predicting one family of cases. The predictions are tests in
`content::terrain::board`; whether the pictures join, only looking can tell.

| Board | Prediction | Result | Test |
|---|---|---|---|
| one far vertex alone | the four single-corner cases, closing into one patch | a closed patch | `a_speck_uses_the_four_single_corner_cases_and_no_others` |
| a straight horizontal boundary | one edge case, unbroken | case 5 on all 24 cells | `a_border_uses_one_edge_case_along_its_whole_run` |
| alternating vertices | the two diagonals only | the two diagonals | `a_checker_uses_the_diagonals_and_the_join_picks_which_pair` |
| a filled disc | every edge, corner and three-corner case, outline unbroken | unbroken | `a_blob_reaches_every_case_family` |

**Viewer**: the first also fixes orientation. Under the other seven rotations
and mirrors the four wedges point outward and leave a hole, so canvas corners
are board corners.

The spare slots are real: two opposite corners draw either as separate islands
or as a band through the tile; swapping slots 1/2 and 7/8 on a checkerboard
changes the picture that way. Hence seventeen, not fifteen.

### Slot 0

Never used. A cell whose four corners agree draws that class's body set, so
"all four far" cannot occur. Slot 0 is the destination's ground; on eight of the
ten shipped sets it is the same `TILE` as that body set's last variant
(`a_base_blend_slot_zero_is_its_destinations_last_body_tile`).

### What this says about the projection

The ground is an axis-aligned grid of square cells with terrain on corners.
Terrain tiles are 32 by 32, opaque, direct colour
(`every_terrain_tile_is_a_uniform_opaque_square`), so they cannot be isometric
diamonds: a diamond needs a silhouette, and version 1 cannot express one.

Board-to-screen: x right, y down, north up — see
[saves.md](saves.md#north-is-up-the-screen). **Inferred**: the `Info`
bitfield's two view flags suggest the overhead view may differ. The camera is
orthographic at 55° —
[saves.md](saves.md#the-projection-an-orthographic-camera-at-55-degrees).

## The colour on screen

**Binary**, `MajestyHD.exe`; **Game** where captures are named.

```text
 corner   shade/255 + Ambient (0.35), in f32
 pixel    Gouraud in 16.16
 level    ((s − 1) >> 12) & 31   k/16 < s ≤ (k+1)/16 gives k
 texel    565, three tables      min(top, trunc(level/16 × c + 0.5))
 shadow   on that frame          HSV: value × ramp, hue and saturation kept
 frame    16-bit 565             the D3D path draws into a 16-bit buffer
 screen   565 to 8888 by shifts  low bits zero, presented 1:1
```

- The ground is lit by table, not multiplied. 32 levels, level 16 the art as
  stored; shade 2.0 wraps to level 0 (`unity_keeps_a_colour_and_two_wraps`,
  `a_level_past_the_tables_wraps_both_ways`). Level ground (237) with the
  shipped `Ambient` is 1.279, level 20: every channel × 1.25, clamped
  (`level_ground_is_a_quarter_brighter`). So the game is a quarter brighter
  than its art, and bright texels saturate.
- `Ambient` comes from `GOPT`. A cell's byte 0, where not 0, replaces it;
  nothing fills that byte (**Inferred**: always the ambient).
- HD uses the accurate-colour 565 software shader and HSV shade tables. The
  HLS builder in [art.md](art.md#what-the-band-does-from-the-blitter) is never
  picked in HD.
- With `UseD3D` (the user's `MajXPrefs`) pixel depth is forced to 16; each
  frame is blitted 565 to `A8R8G8B8` and drawn as one texel-for-pixel quad,
  white diffuse. Every channel of every capture has its low bits clear.
- **Game** (the user): the reference captures were taken with
  `UseD3D=1`, `BlitMode` at its default; for the oldest, the user believes so
  but is not certain.
- **Game**, a capture of a hilly map against its save (**Measured**): 75% of ground
  pixels exact, against 3.5% for the art unlit and under 0.1% for a plain
  multiply. Of the rest, a fifth are shadows, each the lit ground
  through the HSV tables (**Measured**). The others
  are sprites and one texel column a cell, where float rounding differs from
  the x87's by one texel.
- Sprites are not lit. A palette colour reaches the frame by dropping low
  bits, so it shows `& 0xf8, 0xfc, 0xf8`.

`regent_core::view::raster::lit`, `regent_core::view::raster::shade`.
