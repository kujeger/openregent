# `TILE` and `IMAG` — pixels and animation

`TILE`: sprite pixels, three versions. `IMAG`: which tiles make a pose.

> Part of [README.md](README.md).

## `TILE` — sprite pixels

All three versions decode. HD: 29,304 version 3, 2,922 version 1, 50
version 2 (`tile_versions_are_the_three_known_ones`).

```
0x00  u16      version, 3 for the row-RLE form
0x02  u16      height, rows
0x04  u16      width, the canvas in columns
0x06  u16      row stride in versions 1 and 2; unknown in version 3 (0 on most
               tiles)
0x08  u16      flags, below
0x0a  i16      anchor x    where the unit stands
0x0c  i16      anchor y
0x0e  u16      draw mode: 0, 1, 2, 5, 7, 8
0x10  u32      copied, never read
0x14  u16      embedded palette count, 1032 bytes each; 0 or 1
0x16  u32      palette: an SPLT record index, or a byte offset when embedded
0x1a  u32[h]   row offsets, relative to 0x1a
...            rows, then the embedded palette if any
```

**Binary**, `MajestyHD.exe`: the loader reads the header, the pixels whole,
then the embedded palettes or, with none, the `SPLT` record. It decodes
nothing: the version and mode are read at draw time.

Flags word (**Binary**):

| Bits | Meaning |
|---|---|
| 0–1 | pixel format; zero is 8-bit indices |
| 2–3 | 16-bit packing: 0 555, 4 565, 8 4444, 12 a table |
| 4 | pixels stored mirrored |
| 5 | palette is the `SPLT` record `0x16` names |
| 8–12 | level for modes 5 and 8 |
| 13 | anchor at the canvas centre; set on no tile |

`tile::MIRRORED`, `tile::SPLT_PALETTE`, `Tile::level`,
`tile::Mode`. **Corpus**: bit 5 is set on exactly the indexed tiles without
an embedded palette (`an_indexed_tile_names_its_palette_one_way`); bit 4 on
22 tiles, exactly those the 22 mirroring frames name
(`a_mirrored_frame_names_a_tile_stored_mirrored`), so each draws as stored.

- `0x08` low byte takes only 0, 1, 5, 32, 48. Bit 2:
  [below](#bit-2-of-the-same-word-says-555-or-565).
- `0x0e` picks the blit: [draw modes](#the-draw-mode-picks-the-blit).
- `0x0a`, `0x0c`: where the unit stands — a character's feet, a building's
  middle. **Viewer**: firing points measured from it land on their towers and
  bowstaves ([attachment points](#the-attachment-blocks-are-points)).
  **Corpus**: not the art's centre: over the indexed version 3 tiles `x`
  matches the drawn centre on median, but few tiles within two pixels.
- `0x10` is not the transparent index ([Index 255 draws
  nothing](#index-255-draws-nothing)). **Binary**: only copied; no blit,
  converter or draw path reads it.

### The row encoding

A row is a sequence of segments; the gaps are the only transparency.

```
[u16 x_end] [u16 count_flags] [count x pixel]
```

- `x_end` is the exclusive end column: the run covers `[x_end - count, x_end)`
  ([Phantomstar721][tile-rle]). Read as a start column, every multi-run row
  shears.
- `count` = low 11 bits of `count_flags`; bit 15 marks the row's last segment.
  Not a byte: many version 3 runs exceed 255 (the Phantoms Haunt mod's `[u8 count][u8 flags]` with runs capped at 80
  is wrong).
- Index 0 is an ordinary colour.

[tile-rle]: https://github.com/Phantomstar721/majesty-gold-hd-art-asset-extractor/blob/main/docs/TILE_V3_RLE_ROOT_CAUSE.md

**Corpus**, over the version 3 tiles of HD and the Workshop mods:

- Bit 0 of `0x08`: set 16-bit colour, clear 8-bit indices. It never disagrees
  with a working width and settles the tiles that parse either way, 10 in HD
  (`the_pixel_format_flag_is_never_wrong`; published tools decode twice and
  keep whichever parses).
- An empty run's `x_end` is meaningless. Some tiles end a row
  `[0xcdcd] [0x8000]` (MSVC's uninitialised-heap fill); do not range-check it.

With both, every tile parses (`every_v3_tile_decodes`, `every_mod_tile_decodes`).
In HD `max(x_end) == width` on 28,733 of 29,304; 571 fall short (empty
rightmost columns); none exceeds (`tile_width_bounds_every_run`).
The indexed tiles' canvases are nearly their drawn boxes: the boxes hold
98.7% of the canvas texels; ten tiles draw nothing
(`a_tiles_canvas_is_nearly_its_drawn_box`).

### Bit 2 of the same word says 555 or 565

**Binary**: the converter masks the flags with `0x0c` and converts to the
screen's format, 565 to 555 or back. Only 0 and 4 ship. **Corpus** agrees:

Of 2,280 direct-colour tiles, the 150 without bit 2 set bit 15 in only 2,676
of 4,340,967 words (`0rrrrrgg gggbbbbb` cannot). The 2,130 with it set bit 15
in over one word in ten (`rrrrrggg gggbbbbb`).

Read 555 as 565 and red's low bit lands on green: green swings half its range
wherever red crosses an even value.

| Mean green step between neighbours | as 565 | as 555 |
|---|---:|---:|
| 150 tiles without bit 2 | 22.8 | 5.4 |
| 2,130 with it | 6.0 | 14.4 |

Per tile: 2,277 of 2,280; the three exceptions are slivers.
Specimen: `BBA1 Animal_Den`'s portrait, green noise under 565.
`tile::Packing`, `the_packing_bit_agrees_with_the_pixels_it_describes`.

### Palette indices 247 to 254 are a control band

**Corpus**: `247-250` darken, `251-254` lighten. The Phantoms Haunt mod's
shadow note agrees on the darken half: 247 a transition seam, 248–250 three
shadow bands, never mixed with ordinary colours in one run.

Green tells the ramps apart: darken holds green at 0 with red and blue rising;
lighten pins red and blue near full with green rising to white. `BVE1 Dryad`'s
palette 564 has both:

```
247 (184,   0, 184)      251 (255,  42, 255)
248 (211,   0, 211)      252 (255,  85, 255)
249 (237,   0, 237)      253 (255, 127, 255)
250 (254,   0, 254)      254 (255, 170, 255)
```

`level_up`'s palette 800: `247-250` black and unused (a glow casts no shadow);
lighten runs to `(253, 253, 253)`. A ramp topping out at white is a brightness
ramp.

False-coloured Dryad frame: `247-250` a blob beside her, where a shadow falls;
`251-254` a one-pixel border on her silhouette. Direction:
[where a shadow falls](#where-a-shadow-falls).

Glow ramp users: `Dryad`, `level_up`, `spawneffect_in` / `_out` / `_new`,
`wound_crit`, `energy`, `selection_ring`, expansion `invis_brew_eff` and
`All_Damged_Fire`.

Neither range is reserved. `maindata`: 279 version 3 tiles use `251-254` with a
magenta palette entry, 204 as ordinary colours; `mx_maindata`: 207 and 49. No
world sprite uses index 255 (`the_palette_control_band_darkens_below_251_and_lightens_above`);
some `interfacedata` tiles do.

The mixing rule holds for world sprites only. **Corpus**:
few world-tile runs mix the two, nearly all
in draw mode 0 tiles, where 247–250 are colours
([draw modes](#the-draw-mode-picks-the-blit)), the rest in mode 1. Interface
and intro tiles mix them often: interface art uses high indices as colours.

The mod's reference colours `(178, 0, 178)`, `(204, 0, 204)`, `(229, 0, 229)`
are the magenta ramp in `SPLT`: per-sprite defaults the renderer never draws, so
no index holds a constant colour across the corpus.

`the_palette_control_band_darkens_below_251_and_lightens_above`.

### What the band does, from the blitter

**Binary**: the world blitter
tests the index before the palette. 255 draws nothing; 247 and above never
reach the palette but shade the destination pixel; the rest are colours.

The shade path takes the destination pixel (RGB565) through a 64K table to
HSV packed `H5 S5 V6`, replaces the 6-bit value by ramp *n*'s entry for it,
and turns it back through a second 64K table. Hue and saturation survive.
The pixel changed is whatever was drawn first, so draw order decides what a
shadow falls on: terrain or the object behind.

**Shadows stack.** **Binary**: the mode 1 and 7 run blits read the screen
word and write it back; nothing marks a pixel as shaded, so a shadow over a
shadow shades it again. **Game**: in a native capture of a hilly map, the
pixels no one ramp explains are the lit ground through two mode 7 ramps in
turn; most fill the overlap of two trees' shadows. `view::raster::stack`,
**Measured**. regent_draw draws
shadows as factors fitted to this arithmetic, stacking (`"blend"`) or not
(`"soft"`).

**Binary**: video init builds the two 128 KB tables as HSV: each 565 channel
scaled to 0–1, value the brightest channel, each field times 255 chopped;
back by the six sectors, low bits dropped. A textbook HLS builder also
exists but HD never picks it.

**Game**: a shadow on snow is grey, as scaling the value keeps it; HLS
would turn near-white purple. Over a native capture of a hilly map the HSV
tables put every shadow pixel exactly where the capture has them,
over the lit ground ([terrain.md](terrain.md#the-colour-on-screen),
**Measured**).

`n` = `(index + 1) & 7`: 247 is ramp 0, 254 ramp 7. Draw mode 7:

| Index | Value, of 64 steps |
|---|---|
| 247 | × 50 % |
| 248 | × 62 % |
| 249 | × 74 % |
| 250 | × 86 % |
| 251 | + 9 |
| 252 | + 13 |
| 253 | + 17 |
| 254 | + 21, clamped at 63 |

**Binary**: one call site builds four ramp families from four literal
pairs. Table *n* of a family uses `first + n * step` percent:

| Pair | Tables | Rule | Used by |
|---|---|---|---|
| `(50, 6)` | 8 | multiplicative, of 64 | draw mode 1 |
| `(50, 12)` | 4 | multiplicative, of 64 | draw mode 7, 247–250 |
| `(10, 6)` | 8 | additive, scaled by 32 | unread |
| `(30, 12)` | 4 | additive, scaled by 32 | draw mode 7, 251–254 |

The two four-table families are adjacent in memory, so one `(index + 1) & 7`
reaches both. Mode 1 darkens all eight indices through the eight-table
family, 50 to 92 %.

Every channel of every capture is on the 565 grid (red and blue in eights,
green in fours): the D3D present shifts the 16-bit frame up
([terrain.md](terrain.md#the-colour-on-screen)).

`regent_core::view::raster::shade`; the four pairs (**Binary**),
**Measured** (the capture).

### The draw mode picks the blit

**Binary**: header `0x0e` is the column of a blit table; the version picks
the table (one of three listed at sprite blit tables), the screen and pixel formats the row. Nine
columns:

| Version | Modes filled | 8-bit blit, mode 0 |
|---|---|---|
| 1 | 0 only | whole rows, 255 a colour |
| 2 | 0 to 8 | skips 255 |
| 3 | 0, 1, 3, 5, 7, 8 | runs, no index compared |

| Mode | Indices 247–254 |
|---|---|
| 0 | colours |
| 1 | darken, `(50, 6)`, eight steps |
| 7 | the control band above |
| 5 | translucent at the level in flags bits 8–12 |

The texture path says the same in alpha: mode 1 gives 247–254 black at
alpha 200 falling by 25; mode 7 four black at 150, 100, 75, 50 and four
white at 50, 75, 100, 125; mode 5 alpha `(10 - level) × 25`. Mode 2:
unread; mode 8 [below](#modes-5-and-8-are-translucent).

**Corpus**: version 1 is mode 0 on every tile; version 2 takes 0, 1, 7;
version 3 0, 1, 2, 5, 7, 8. `tile::Mode`;
`a_tiles_mode_is_one_its_version_can_draw` (`tests/art.rs`). The blit tables and ramps:
**Binary**.

So the tile, not the caller, says whether 247–254 are an operation. The
viewer's map draws mode 1's ramps for a mode 1 tile (`regent_draw::soft::Ramps`)
and mode 7's for every other; modes 5 and 8 translucent.

### Modes 5 and 8 are translucent

**Binary**: both blits keep `L` tenths of what is under, from eleven
tables `d → ⌊d × L / 10⌋`, and draw nothing from 247 up:

| Mode | `L` |
|---|---|
| 5 | the level less 1, 0 to 8, where set |
| 8 | per index: the level less 10 where set, plus `⌊10 (256 − m) / 256⌋`, `m` the colour's brightest channel; 0 to 10 |

So mode 8 shows a pixel as much as it is bright: a glow. **Corpus**, HD
`maindata.cam`: many mode 8 tiles, among them spell effects' (`healing_e`,
`vigilance_e`, …) and the red selection ring's; some mode 5. The 555 mode 5
blit blends blue the other way round, `dst + (src − dst) L / 10`; the viewer
blends all three as red and green. `view::raster::blend`, `tests/blend.rs` (**Binary**).

**Binary**: a unit's draw effects override the tile's mode.
`SetDrawEffects`' transparency is mode 5 at that level as given, every
tile, so its shadow goes; its colour, 1 to 3, is a palette, `PALT` `RED `,
`GRAY`, `GRN ` in `interfacedata.cam`, and every colour of the tile's
palette becomes the tint's entry nearest it: a 16-level-a-channel lookup
over the tint's first 247 entries, the lowest index on a
tie.
`ClearDrawEffects` puts the tile's mode back. `blend::tinted`,
`blend::see_through`.

### Two blits, and which one is drawing

Applied to interface tiles, the world branch draws them wrong:

- `AVL1 Warrior`'s `Interface` set 1001 (hero panel backdrop): 200 by
  245 of stonework, palette 247–255 muted colours, `247` = `(228, 208, 218)`.
  The world reading shades 1,786 of its pixels and punches 45 holes.
- `INDa` set 1000 is `interfacedata` tile 912: 800 by 600, 407,865 pixels of
  index 247 with palette `(128, 0, 128)` — a dialog frame over a scrim, the
  screen behind at half lightness. It is mode 7, so it shades under either
  reading.

**Binary**: the tile's draw mode picks the blit
([above](#the-draw-mode-picks-the-blit)). `regent_core::data::imag::Blit`
selects by `IMAG` set kind instead (in the world the index decides, in the
interface the palette), with a palette marker: both reference ramps put red
and blue equal and green apart (magenta for shadow, green for glow).
**Inferred**: the marker predicate; no engine path consults the palette.

**Corpus**: nearly every world-set tile using index 247 has the marker in
its palette; a minority of `Interface`-set tiles do.

Of 31,322 tiles the base dataset's sets reference, 3,604 are in an
`Interface` set alone, 27,715 in a world set alone, three in both. 1,394
interface tiles differ between readings, every 25 by 25 panel icon among them.
`an_interface_picture_reads_its_colours_from_the_palette`,
`a_dialogs_scrim_is_a_control_band_under_either_blit`,
`the_two_blits_barely_share_a_tile`.

The menus and panels (openregent, `regent_cli ui-dump`) go by the mode
(`Sprite::as_drawn`, `shade::by_mode`). Where the marker and the mode
differ: the quest map frame `INx2` 1000–1011 is mode 1 on a black palette
(a shadow, not solid black); the hero frame `INDe` 1001 and building frames
`INBg` 1024–1027 are mode 1 on magenta, so 251–254 darken there.
`an_interface_tiles_mode_says_what_its_band_does`.

### Where a shadow falls

**Corpus**: every character's shadow falls left of its anchor: all 57
base-dataset characters with a shadow in a standing or walking frame
(centroid of the darkening band). The sun is to the right, where
[settings.md](settings.md)'s `LIGHT` lights terrain from.

Only the horizontal is tested (`a_characters_shadow_falls_to_the_left_of_its_anchor`):
the anchor is at the feet, and a rat or spider spreads its shadow around it.
**Viewer**, one building: in a native capture of a hilly map the Wizards
Tower's shadow falls up and left.

**Corpus**: a building's shadow is 247–250 alone, in a mode 1 or 7 tile
(`a_buildings_shadow_is_indices_247_to_250`). Shade painted in ordinary
colours is not shadow to the draw: the Palace's west walls are dark
colours, and the dirt at their foot is painted darker (**Inferred**, by
eye); its 5,264 shadow pixels are the two lobes its towers throw on the
grass and a dithered rim. `regent_cli shadows --record ID` draws which
pixel is which.

**Corpus**: not every darkening pixel is a ground shadow. The level 2
Guardhouse's flag (`ABE2` `Active`) is drawn in 247–250, 94 to 98 pixels a
frame, in the same mode 7 tile as its 3,077 to 3,086 ground shadow pixels.
Mode, layer and index cannot tell them apart; place can. A ground shadow
rises above the footprint's top by no more than it lies left of the
footprint's right edge (**Inferred**, from the fitted suns,
[original/art-camera.md](../original/art-camera.md#the-sun)); the flag rises over
90 pixels, just right of the anchor. `Sprite::aloft`,
`a_guardhouses_flag_is_aloft_and_its_shadow_not`. Most other hits are a
rim on a roof (Wizards Tower). A damage stage's layers over
the building (flames, smoke) also darken with these indices. A chimney's
smoke drawn as a layer of darkening pixels only (Blacksmith `ABC1` layer 1)
is not told apart by this rule.

### The anchor is the ground, and four characters are off it

**Corpus**: of 59 characters with a standing frame, 55 put their lowest body
pixel at or below the anchor row, median six rows below: feet form an
ellipse, the anchor is its middle. The four above it:

| Unit | Body above the anchor |
|---|---:|
| Harpy | 39 |
| Giant Eagle | 37 |
| Black Phantom | 6 |
| Evil Oculus | 5 |

**Game** (user): the Evil Oculus hovers; Harpy and Giant Eagle fly; Black
Phantom is a ghost. The Dryad also hovers and measures exactly 0 (one foot
lifted). So flight height is readable off the art.
`a_character_stands_on_its_anchor_unless_it_flies`.

The Evil Oculus throws its shadow 63 pixels from its anchor, furthest in the
dataset (`a_characters_shadow_falls_to_the_left_of_its_anchor`). What sets the distance is unread: height and size both fit; grounded
Rock Golem and Dirgo throw nearly as far.

### Index 255 draws nothing

**Binary**: not in every blit. Version 1 copies 255 as a colour, version 3's
mode 0 blit compares no index, version 2 skips it in every mode
([draw modes](#the-draw-mode-picks-the-blit)). Run gaps are version 3's
transparency.

Header `0x10` is not the transparent index: on the interface icons where
it holds the index of the icon's one-pixel frame, the game draws the frame
(**Game**: school colours around every spell bar icon in
a native capture).

**Corpus**: some uncompressed indexed tiles hold index 255, sparsely; nearly
all are version 1, where HD draws it as a colour.
Specimen `maindata` tile 4377, a one-pixel ellipse outline whose
`palette[255]` nearly matches its neighbours.

### Which archives use which format

**Corpus**, every `TILE` header in the install: version 3, 28,927 indexed
(`tile_rasterises_exactly_its_runs`) and 377 direct colour
(`every_v3_tile_decodes`); versions 1 and 2, 2,972
(`every_uncompressed_tile_decodes`), 1,903 of them RGB565
(`no_animated_sprite_is_stored_in_direct_colour`). The world archives hold
indexed version 3 and some versions 1 and 2; the interface archives every kind; the
tileset archives only version 1 RGB565.

- Every version 3 tile outside the interface is indexed; all 377 direct-colour
  version 3 tiles are interface art. Direct colour cannot take player colours,
  and only version 3 has transparency, so only it can be an animation frame:
  nothing that moves is unrecolourable. `no_animated_sprite_is_stored_in_direct_colour`.
- Terrain is all uncompressed direct colour: 1,860 version 1 RGB565 tiles,
  32 by 32, no palette (`no_animated_sprite_is_stored_in_direct_colour`).

Direct colour is most of version 1, not only interface art. Outside terrain,
a few version 1 RGB565 tiles sit in the interface archives, and more in
`maindata.cam`. Those are each 100 by 100 and named for buildings (`Ballista Tower`, `Guardhouse`, `Inn`, temples) — **Inferred** to
be interface pictures of buildings, not sprites.

### Versions 1 and 2 are the same format, uncompressed

**Corpus**, every version 1 and 2 tile (HD, Workshop mods):

```
0x00 .. 0x19  the same header as version 3
0x1a          height x stride bytes of pixels, row-major, no runs, no gaps
              stride is 0x06, and equals width x bytes-per-pixel
then          the 1032-byte palette, when the count at 0x14 is 1
```

- Bit 0 of the flags still selects the pixel format (in HD 1,903 of 2,972
  RGB565, the rest 8-bit); `stride == width x bpp` holds on every tile.
- With an embedded palette, `0x16` equals `0x1a + height x stride`.
- Payload length fully accounted for.

Every version 2 tile across every corpus fits the version 1 layout. The
blit tells them apart: version 2 is the uncompressed form with index 255
transparent (`tile::VERSION_KEYED`), version 1 opaque
([draw modes](#the-draw-mode-picks-the-blit)). **Corpus**: some of HD's
version 2 tiles hold 255.

**Inferred** purpose of version 1: a raw block has no gaps, so no
transparency; that fits its users, terrain and full-rectangle interface art.

`data::tile` reads all three into one `Pixels` enum; `every_uncompressed_tile_decodes`
and `every_decodes_its_tiles` (`tests/editions.rs`) hold the counts;

## `IMAG` — animation metadata

No pixels: which `TILE`s make which animation, for which pose and facing.
Every record decodes (`every_decodes_its_imag_records`,
`tests/editions.rs`).

```
record  +0x00  u32       version: 2, 3, 4 or 5
        +0x14  u32       set count
        +0x18  8 x n     set table: { u32 set_id, u32 offset }

set, at the table's offset, ending where the next entry begins
        +0x00  u32       slot count
        +0x04  60 by     control; +0x0c layer flags, +0x14 to +0x28 playback,
                         +0x3c sort nudge (layer flags bit 0; Set::sort_nudge)
        +0x40  4 x n     slot offsets, relative to the set

slot, versions 3 to 5; HEADER = 0x10, 0x14, 0x18
        +0x00  i16 i16   x, y — this layer against layer 0
        +0x04  u16 u16   streams, frames per stream
        +0x08  u32       block mask
        +0x0c  u32       point-list mask
        +0x10  u16       playback follows the frames (4, 5)
        +0x12  u16       frame list count, 1 on every slot (4, 5)
        +0x14  u32       effect-list mask (5)
        HEADER           8 bytes per set bit of the block mask
                         per set bit of the point-list mask: [u32 n], n (i32, i32)
                         [u32 0] (4, 5), then the frame table:
                         frames x streams x { u32 metadata, u32 reference }
                         20-byte playback, if +0x10
                         per set bit of the effect mask (5): [u32 n], n x 32 bytes

slot, version 2
        +0x00  3 x 8     three blocks, always
        +0x18  i16 i16   x, y
        +0x1c  u16 u16   streams, frames per stream
        +0x20            frame table
```

**Binary**: one slot reader per version. Walked this way every version 3 to
5 slot in HD and the Workshop mods ends where the next begins, bar one mod
slot. The loader seeks to each slot by its offset, so seven version 2 slots
in HD `Data` carry slack bytes nothing reads
(`a_slots_trailer_is_its_playback_and_effects`).

**Corpus**: every record decodes — HD's 649, with 13,439 slots and 86,868
frames (`every_imag_record_decodes_to_frames`), and the Workshop mods' (at
least 35, `every_mod_imag_record_decodes`).

The overall shape is [Phantomstar721's][imag]. Three parts are needed
beyond it:

[imag]: https://github.com/Phantomstar721/majesty-gold-hd-art-asset-extractor/blob/main/docs/IMAG_DESCRIPTOR_ROOT_CAUSE.md

1. `+0x00` is a version word. It sets where the count word lives and the
   slot header length. Versions 2 and 3 number exactly 98 and 28
   (`imag_versions_are_the_four_known_ones`); 4 and 5 grow per release. Version 5 ships only in expansion
   archives.
2. The frame table position depends on `+0x0c`, a mask: one point list per
   set bit between blocks and frames. Neither fixed anchoring works:
   `ABo1 HallOfChampions` has one slot of each kind, and end anchoring misreads
   a slot with a trailer (`SpireOfDeath`'s one version 5 slot with both, 68
   bytes off).
3. Every reference is a tile. Five flag bits are ever set (`0x8000`,
   `0x4000`, `0x2000`, `0x0800`, `0x0100`, in the high half); the low 16 bits
   index the archive's own `TILE` chunk regardless. Negative references are
   a sign of wrong anchoring.

### What the reference bits do

**Binary**, read by the unit draw, the hit test and the preload:

| Bit | Effect |
|---|---|
| 24 | drawn only in the layer's second pass; never hit-tested |
| 27–28 | a field: 0 a `TILE`, `0x10000000` a `CY3D` model, `0x08000000` a renderer object |
| 29 | draw mirrored; see flags bit 4 |
| 30 | not drawn, loaded or hit-tested |
| 31 | an action's cue: its `InitAction` fires its callback on this frame, not its last |

Bits 27 and 28 are one field; 24, 29 and 30 are read independently. Kind
`0x08000000` is `Got_Gold`'s amount, drawn as text
([bytecode.md](../gpl/vm.md#what-an-effect-shows)); a reward flag's number
is not one: no flag frame carries it, it is the unit's label
([below](#a-reward-flags-reward-hangs-over-it)).

**Binary**: a panel's picture draws only where a frame box is found, which
skips bit 30 and kinds 27–28, so a kind `0x08000000` frame there is no
picture. Interface sets lit only on hover use it for their other slots:
base `INTN` 1039 (`AP92`'s overlays 6401–6428), `INPq` 1020–1038,
expansion `IX23` 1039, `IX33` 1001–1031. Its low bits are 0; read as a
tile it is the archive's tile 0, `CUR1`'s cursor in `interfacedata.cam`.
`a_frame_of_another_kind_is_no_picture`.

**Binary**: at a part's frame the draw blits every stream of the slot, in
order, each frame at its nudge on the layer's pose: streams are parallel
pictures, not alternatives (`energy_blast_effector`'s burst and its streak).
A second stream's blank frames are bit 30; bit 24 frames are the second
pass, below. `every_stream_blits_at_its_frames_nudge`,
`got_golds_amount_layer_blits_no_tile`. **Corpus**: every frame carries at
most one bit, and no model reference ships. Bit 31 is read by the animation
order, not the draw ([movement.md](../original/movement.md#track-2));
**Corpus**: HD's casts and some attacks carry it. `imag::PASS`, `KIND`,
`MODEL`, `TEXT`, `MIRROR`, `HIDDEN`, `CUE`;
`a_reference_carries_one_flag_at_most`.

### The second pass is a flier's shadow

**Binary**: a layer makes a pass 0 part for layer flags bit 8 and a pass 1
part for bit 9, in that order. The draw takes a frame whose bit 24 is the
part's pass; a pass 1 part stands at the unit's map x and y and the ground
height there, not at the unit's own place. Same slot, frame, streams and
nudges as pass 0. It draws with part 0 in part order, so right after its
body, at the unit's place in the draw list. Any part but 0 takes its slot's
pose offset, so `WPm1`'s shadow moves 3 pixels on some slots, its body not.

**Corpus** (`every_second_pass_frame_is_a_fliers_shadow`): 26 sets of 15
records carry bit 9 — arrows and bolts, the wizard blasts (`APB2`, `WPa2`),
`Firespit`, and the fliers `dragon`, `Vendral` and `IceDragon` (`BVD1`,
`BVg1`, `BVm1`). Every bit 24 frame sits in such a set, 1,591 in a second
stream, 2 in stream 0 (`APA5` and `WPl1` slot 5, which so draw two shadows
and no body: game data). All are mode 1 or 7 tiles: shadows, not glows.
`imag::Pass`, `Pose::blits` (**Binary**),
`a_missiles_second_part_draws_its_bit_24_frames`. **Inferred**: a set with
neither flag (in HD, interface art and some effect and missile sets)
would make no part; the viewer draws it as one pass 0 part.

### The point run is a chain of lists, and list 0 is a building's base

The first list's count is the slot header's last word, so the run begins one
word after its count:

```text
...slot header, last dword = n0
n0 x (i32, i32)        list 0
n1, n1 x (i32, i32)    list 1
0                      a zero count closes the chain
```

**Corpus**: some slots carry a second list. HD `Data`: 168 of 168 slots
with a run account for every byte, 195 lists —
`a_slots_point_run_is_a_chain_of_lists`. **Binary**: the engine keeps two
lists.

List 0 of a building is its base polygon, screen pixels about the anchor. The
engine paints bare ground over it when the building is placed — see
[maps.md](maps.md#a-building-paints-bare-ground-over-its-own-polygon).
`ABY1 Wizards_Guild1`'s `Inactive` slot:

```text
(92, 99) (52, -10) (-72, -9) (-144, 84) (-25, 130) (53, 135)
```

a closed hexagon about 236 by 145. `ABJ1 Palace1` has 19 points.

**Binary**: list 1 is read by nothing; the one function that caches it has
no caller. HD's 27 second lists are unused.

A slot is not always a direction. The format allows 256 slots; `APA2 Ranger
Arrow` has 32. Eight `Walk` slots are facings, ten on a rock are variants,
eight on a Ballista Tower are firing positions.

### A repeated set is a stack of layers

`set_id` is packed: low 16 bits the kind id, high byte a counter separating
repeats. Bits 16–23 are zero on every set, so reading a `u16` there gives
256 where the file means 1. `a_sets_variant_is_the_top_byte_of_its_id`.

Repeats of one kind are layers drawn together, not alternatives:

| Record | Kind | Layer 0 | Layer 1 |
|---|---|---|---|
| `ABJ1 Palace1` | `Active` | the palace, tile 1036 at 501×339 | waving banner, tile 1041, 6 frames at 46×49 |
| `ABJ1 Palace1` | `Inactive` | tile 1036 again | the same banner |
| `ABY1 Wizards_Guild1` | `Build` | the finished building, 291×274 | scaffolding, 32 frames at 183×143 |
| `ABH3 Marketplace3` | `Active` | the market, 259×205 | awnings and pennants, 6 frames |

Drawing one set draws half a building. The Palace banner is also on every `Die`
stage, so it says nothing about activity.

**Binary**: layer order is lowest first; parts are made layer by layer and
part 0's draw takes them in order. Counterexample `BB01 Krolm_Altar`:
`Inactive` has `All_Damged_Fire` on layer 0, the altar on layer 1;
unexplained.

**Binary**: a higher layer whose set has layer flags bit 0 and a sort nudge
is a draw-list node of its own at its nudge, which part 0's draw skips; any
other draws with part 0. Seven expansion buildings carry a ground layer so,
at -1,000: the Mausoleum's dirt diamond (`Inactive` layer 1, tile 533,
200×139), the three Magic Bazaars, Fortress of Ixmil, Ancient Graveyard,
Goblin Fortress. It sorts 38 cells further, so under the building.
`Animation::listed`, `a_buildings_ground_layer_sorts_under_it`.

### A building has two pictures, and the windows are the difference

`ABY1 Wizards_Guild1`: tile 1781 windows dark; tile 1801 windows lit green, a
lantern glowing.

| Picture | Set | Coverage |
|---|---|---|
| Windows dark, idle | `Hotspot`, id 400 | most buildings with art |
| Lit, animated, with overlay layers | `Active`, id 192 | most buildings with art |

**Corpus**: `Hotspot` is the building's picture, not a click mask: `Die`
layer 0 (the damage sequence) names the same tile on most buildings with
both.

`Inactive` is the building too: all buildings but one have one, matching
`Hotspot` on most. The save names the set each object draws, so a renderer never
chooses — see [saves.md](saves.md).

**Corpus**: an upgraded building names its own `IMAG` record: all 25
with an `UpgradeFrom` in the expansion dataset's archives
(`an_upgrade_draws_its_own_art`).

54 of 60 records with both draw two different pictures. The six that do not
include all three Palaces. `ABH3 Marketplace3`: `Active` adds stall awnings and
pennants. `a_buildings_two_pictures_are_two_pictures_bar_six`.

On characters `Hotspot` means something else: every one with one names a tile
matching neither `Die` layer 0 nor `Active`.

### The three `Interface` sets, and how the engine tells them apart

`SetKind::Interface` covers ids 1000–1040; a record has up to three, and
position does not identify them (the small icon is set 8 on
`AVb0 Veteran_City_Guard`, set 3 on `AVL1 Warrior`). The id does:

| Id | Size | Is | On |
|---|---|---|---|
| 1000 | 100 × 100 | portrait | 146 records: buildings, characters, a few scatter and overlays |
| 1001 | 200 × 245 | hero panel backdrop | 20 characters, no building |
| 1002 | 25 × 25 | small list icon | 76 records: buildings and characters |

Counts: `a_units_interface_sets_are_three_sizes_of_one_picture`. The
characters with a small icon are those a panel lists: sixteen classes,
peasants, guards, tax collector.

**Corpus**: a character's icon is its portrait shrunk, whole frame: grey
levels of the portrait's 4 × 4 means against the icon correlate above 0.5,
the lowest the veteran guard's, retoned. A building's icon is another
drawing, a symbol in a border (below 0.5).
`a_characters_list_icon_is_its_portrait_shrunk`, `portrait::icon_source`.

The roles come from a working mod, not the sizes. [Custom Guild — Phantom's
Haunt][haunt] adds a hero to the retail game and names all three: 1000 portrait,
1001 selected-unit panel art, 1002 small hero icon. Its `PHM1Phantom` clones the
stock Priestess `IMAG`, art replaced, set ids unchanged.
`a_workshop_hero_uses_the_stock_interface_ids`.

[haunt]: https://github.com/Phantomstar721/majesty-gold-hd-custom-guild-phantoms-haunt

No data file says what an id is for. HD's `Data/ImageSetIDXRef.xml` calls them
`Interface-00` to `Interface-40`, an ordinal. **Binary**: the string
`Interface` is in neither executable, so the engine never resolves one by
name as it does `<ImageSet value="Cast"/>`; a mod adding a hero must reuse
stock ids.

**Binary**: the spell book's icons come from one switch on a spell's
descriptor code: 23 spells, `INTn` sets 1001–1023 in guild order (`WRa1`…`WRg1`,
then `HR`, `MR`, `CR`, `PR`, three singletons); 1023 is the fallback. `INTn`
holds exactly `Interface` 1001–1024, so every id the switch yields exists.
[`view::panel::icons`](../../crates/regent_core/src/view/panel/icons.rs),
`tests/icons.rs`. A second table maps a small integer key to set ids
1000–1012; unread.

**Binary**, HD, what the selection panel draws:

- Every selection panel gives its portrait control, 8011, the unit's `IMAG`
  and set 1000. Heroes, henchmen, monsters and buildings alike: every
  character with a `Walk` set has one.
- The hero panel lays the unit's set 1001 under the whole panel:
  stonework, coloured per class. The portrait sits on it.
- Nothing selected: the panel slot shows `INDe` set 1000, a gold crown, or
  the expansion's `IX72` set 1001; [panels.md](panels.md#the-selection-panel).

[`view::panel::portrait`](../../crates/regent_core/src/view/panel/portrait.rs),
`tests/portrait.rs`.

So an `IMAG` says which pictures exist; only the executable says what each is
for. Sizes hold on every record.
`a_units_interface_sets_are_three_sizes_of_one_picture`, read by
`content::unit::Unit::interface`.

### The selection ring is a sprite

**Binary**: not a drawn ellipse but a level image's `Stand` slot, drawn for
buildings, mobiles, reward flags and the build preview. Scatter gets none.
Per unit in the board-y draw list: the ring, then the picture, then bar and
label, so the ring lies under the body.

| Image | Record | Drawn for |
|---|---|---|
| 1 | `AVn1 selection_ring` | the selected unit |
| 2 | `AVn2 flag_brackets` | a reward flag, tested first: selected, as no hover takes a flag |
| 3 | `AVn8`, green | the build preview on free ground; the unit under the cursor |
| 4 | `AVn9`, red | the build preview on blocked ground; the cursor's, by a flag |

Each holds 13 slots of ellipses, 39×21 to 750×436. The slot is the first
whose block 0 is at least as wide as a rect R, else the last. Its anchor
goes on the unit's anchor plus R's centre; a flier's (movement layer 4) on
the ground under it, as its shadow (**Game**: a dragon's ring is at its
shadow). R is the caller's (`ring::Fit`):

| Drawn for | R |
|---|---|
| a selected or hovered unit | its own rect: set 8 block 0, else `0xd0` block 1, else `0x40` block 2, else frame boxes; with none, the box it was first drawn in |
| a standing building in build mode | its pad, ground pixels |
| the build preview | its pad grown by `ConstructData1`, 8 half cells, times 16: the clearing reach |

A warrior takes slot 0; the palace, fitted to its `Inactive` block 1,
slot 9, round its walls (**Viewer**). An inn's preview takes slot 8, 465
pixels wide; selected, slot 3 (`the_previews_ring_fits_its_clearing_reach`).
In build mode every standing building draws a ring, `AVn8` or `AVn9`.

A unit rect with none of its sources stays empty, and the piece's screen
box fills it at the next draw, less the anchor: the frames' tiles with the
anchor, grown by the unit's extra rect where that has area, the hit point
bar's outer rect while shown, then the name's box while `DisplayFlagNames`
is set. Drawn before any selection, so the ring's own rect is not in it;
the rect stays until the unit changes type. Eight units (`RECTLESS`): the
Creature Den `BBA1`, the Hidden Chalice Site `BBf1`, `ABA0`, markers
`MVa1`–`MVa4`, `WVe2`. The den in `Inactive` slot 0: tiles
`[-63, -3, 56, 102]`, bar to 112, name to 121, so slot 3 at (-4, 59).
**Game** (a capture, the den selected): its tile at (162, 118), anchor
(225, 121), the ring slot 3 at (221, 180)
(`the_creature_dens_ring_fits_its_drawn_box`,
`a_rectless_units_ring_fits_its_drawn_box`). **Inferred**: the box is
taken at view zoom 1.

`AVn1`'s tiles are indices 251–254 in draw mode 7 and nothing else: a glow
the world blit adds to what is beneath.
[`view::map::ring`](../../crates/regent_core/src/view/map/ring.rs),
`the_selection_ring_is_all_glow`. The clearing ring (`sim::clearing`) fits
`AVn8` the same way.

**Corpus**, fitted: from slot 3 up, `AVn1`, `AVn8` and `AVn9` draw one
ellipse a slot (their middle lines' axes agree within a pixel), about
0.557 high to 1 wide; each look its own width and fall-off. `AVn1` is flat at its
peak in the middle 45 % of its width; `AVn8` and `AVn9` fade to a
fifth or less at the edge. `AVn8` is level 11 in slots 0 to 2, 12
above (alpha 0.7, then 0.6 at its brightest); `AVn9` level 14.
[`view::map::ring::band`](../../crates/regent_core/src/view/map/ring/band.rs),
`a_native_ring_covers_its_art`.

**Binary**, the hover ring: the unit under the cursor draws `AVn8`, a mode
8 glow, fitted to its own rect; selected, `AVn1` instead: one ring a unit.
`AVn9` never shows for a hovered unit (its word is always 0). Only the
select and spell modes name the unit ([below](#the-cursor-is-cur1)), and
only one with `CanHighlight` on a standing building or a character. Every
walking character carries it; resource piles, markers and spell effects
do not. Two buildings do: the Balance of Twilight shrines
`CB01 Shrine_of_Darkness` and `CB02 Shrine_of_Light` (**Game**: the user
saw rings on them). A spell mode names only a unit its spell may take.
`ring::HOVER`, `ring::highlights`,
`the_hover_ring_takes_every_walker_and_two_buildings`.

### The hit point bar hangs off the body

**Binary**: every unit draws one, selected or not, when display flag bit 2
is set (`DisplayFlagStats`, registry default 1, the main frame's control
18), it has an owner, `APU:0` is at least 1, and it has `HasHPBar`. The bar
is placed about the unit's screen place, the body at its own height, with
no movement layer test.

```text
 R = PRIM +0x04..+0x10, ground pixels (y over sin 55°)
 w = 60 for a building, else 30;  x0 = (R.x0 + R.x1) / 2 − w / 2
 black   [x0 − 1, R.y1 − 4) .. [x0 + w + 1, R.y1 + 1)
 shade   below and right, 2 pixels: HSV value to 68% (table 3)
 fill    [x0, R.y1 − 3) .. [x0 + w · hp / max, R.y1), the owner's colour
```

R's bottom is a screen rect's read as ground pixels, so the bar sits
lower than the art's footprint. **Game** (captures, hovered and
selected): an Ice Dragon hovering, body anchor found by its
frame's pixels; shadow 34 pixels lower, its lift; fill 61 to 64 pixels
under the body, inside its ring, frame black, pixel for pixel. So a
flier's bar looks to be at its shadow only because the dragon flies low.
[`view::map::bar`](../../crates/regent_core/src/view/map/bar.rs), **Measured**.

`AVn2` has one slot, four bracket corners in indices 251 and yellow, so
every flag takes it, anchored on its own rect's centre: an attack flag's
(set `0x40`'s frame boxes, `[-18, -19, 17, 23]`) at `(-1, 2)`.
**Game** (a capture, a flag selected): every colour pixel there.
`ring::Look::of`, **Measured**.

### A reward flag's reward hangs over it

**Binary**: a label draws after the bar, at view zoom 1.0 alone, for a
reward flag, or any unit under `DisplayFlagNames` (display flag bit 0,
[below](#a-units-name-hangs-under-its-bar)). The flag's is made once and
remade when `APR:1` is written, so a raise shows. No owner or fog test but
the unit's own draw.

```text
 text   "%d" of APR:1, the reward with its raises; 0 where unset
 font   fnt7: ascent 7, descent 3, spacing 1 (slot 0 blocks 1 and 2),
        a digit 6 wide; colour 0x80ffffff, on screen 0xe0e0e0 (Game)
 box    w = Σ (advance + spacing), h = 7 + 3 + 1, the shadow's drop
        x0 = -(w / 2), y1 = R.y0, R the pad's pixel rect (PRIM +0x04),
        about the screen place; the baseline 7 below the top
 shadow each glyph first at +1, +1 in blit mode 3: HSV value to 68%
```

An attack flag's 500: box `[-10, -34, 11, -23]`. **Game**
(the same capture): its glyphs land on every `0xe0e0e0` pixel and no
other. [`view::map::label`](../../crates/regent_core/src/view/map/label.rs),
**Measured**.

### A unit's name hangs under its bar

**Binary**: under `DisplayFlagNames` (registry record 2, default 1: on;
`APMK` control 19, F9) every other unit's label is its name, in the flag's
font, colour, flags and shadow.

```text
 text   a building (chests and signs too): its own name (AP:1 spelled,
        "Library #1"), else its HEAD title, as is, not the panel's
        capitals; anyone else: its name, then " (n)" with APV:11 from 1
        ("%s (%d)"); nameless scatter, effects and message flags an empty
        text
 box    x0 = (R.x0 + R.x1) / 2 - w / 2, y1 = R.y1 + 12: under the bar
 when   a building standing; a character lying dead too; never the build
        preview; not held
 remade on an APV:11 write
```

**Game** (a capture): Wizards Guild, Wizards Tower #1 and #2,
Library #1 and Palace, each box placed from its bar's fill, every glyph
on `0xe0e0e0` and no other pixel. Names show in every capture taken with
the check set; those with it clear show none. `label::name`, `label::under`,
**Measured**.

### The cursor is `CUR1`

**Binary**, `GE_Cursor`/`GS_Cursor`: the cursor is `CUR1 Tactical Cursor`,
the expansion's (`mx_interfacedata.cam`) over the base game's. A mode (a
view's, or the dialog manager's) names a row of a cursor table: set
`1000 + n`, slot 0 on entering; each update the row's state callback
returns the slot. The hotspot is the tile anchor. No slot has two frames,
so nothing animates.

| Mode | Set | Slots |
|---|---|---|
| `Sele`, the map | 1000 | 0 an open hand; 2 the pointing hand over a piece it may select, seen, hit on its picture or a character ([the pick](#the-pick)) |
| `Dial`, a dialog | 1002 | 0 the pointing hand: over a panel's control, SMNU key 38 not 0 (`CYDialogItem` message 0x5a) |
| `Mini`, a mini view | 1000 | 0 |
| `Drag`, placing a building | 1007 | 0 a fist hanging below the mouse; 2 the treasury short of the price |
| `Move`, a camp move | 1007 | 0 |
| `Sp10`–`Sp37` but Sp15 | 1010–1037 | 0 the spell's icon, a valid aim; 1 with a red bar, not; 2 with coins, short of the price |
| `Fl00`, attack flag | 1005 | 0 a target, 1 none, 2 short of the reward |
| `Fl01`, explore flag | 1006 | 0; 1 short of the reward |
| busy (loading, saving) | 1001 | 0 |

The selection panel's portrait 8011 keeps an aimed spell's cursor: its slot
by the panel's unit as a unit spell's target, the ring on that unit valid or
not, not when short (**Binary**).

A point spell's aim is its spot: Farseeing unexplored ground in reach,
Lightning Storm explored ground in reach, the other four explored ground.
The wizards' guild's six spells' sets (1010–1015, Sp15's unused) have a
slot 3, the icon under a red bar and spyglass, that no callback returns. A
row with its price byte writes a nonzero amount (price, reward) at (0, 20)
from the hotspot in font `SM09`. Edge scrolling changes no cursor.
[`view::map::cursor`](../../crates/regent_core/src/view/map/cursor.rs), `tests/cursor.rs`
(the mode table: **Binary**).
**Inferred**: with nothing under a hardware cursor, its shadow indices
draw black at their darkening.

### The pick

**Binary**: the select mode's cursor, hover and click, a unit spell's aim
and an attack flag's target ask one pick: mask `0x80000030`, reach 4.

| Step | Rule |
|---|---|
| Pieces | the view's draw list, front to back: seen, no scatter; **Inferred**: no held piece |
| Box | the current frame's tiles over every layer and the anchor; the ring's rect while selected or hovered, then the bar's outer rect while shown and the label's box under DisplayFlagNames or on a reward flag; at scale 0.5 halved, a pixel grown; grown by the reach |
| Picture | each layer's frame, every stream; a tile pixel stored under a run, the column past a run's end too; on an 8-bit tile index 247–255 misses in any draw mode; a text frame always meets; mirroring unread |
| Answer | the first piece met on its picture, else the last met in its box |
| Then | a speed trail clone's leader (`APU:8`), or a hung piece's holder but a reward flag's |

The pointing hand and a selecting click want a picture hit or a
character; the hover ring any hit `CanHighlight` takes; a unit spell any
hit; an attack flag a picture hit or a character. So a building is picked
by its pixels, its shadow not; a character by its box.
[`view::map::hit`](../../crates/regent_core/src/view/map/hit.rs),
`view::map::tile_hit::picks`, `the_pick_meets_a_buildings_body_not_its_gaps_or_shadow`,
`a_character_is_met_in_its_box`; the pick sites (**Binary**).

**Binary**: a character takes a click only with a panel, and spell effects
carry no `DialogID` and no `CanUse` `HumanPlayer`. The original's pick
still stops at them, and shows the pointing hand. openregent's goes through
them ([openregent/enhancements.md](../openregent/enhancements.md)):
`view::map::hit::solid`, `a_spell_effect_lets_the_pick_through`.

### `Die` means two things, and the pose offset places a layer

| Subtype | `Die` |
|---|---|
| Character | one set, eight slots: death animation per facing, all at the origin |
| Building | many sets, nearly all one slot, most offset |

**Game**: on a building they are the fires of a damaged building, one set per
fire. `SetKind::Die` is wrong for buildings, where ids `0x60`–`0x67` are damage
overlays (like `Flags`, meaning depends on subtype); which one shows is
[below](#which-set-a-building-shows).

Layer 0 of each `Die` id is the building at that damage stage; layers 1+ are
fires. The Guild's eight ids give four tiles twice: 1781 undamaged, then 1782,
1783, 1784 increasingly wrecked. The save names the set (a played save's Guardhouse
asks for `Die-4`, three fires), so hit points need not decide the
stage. See [saves.md](saves.md).

### Which set a building shows

`Die-0` and `Die-1` draw the finished tile, so up to 25% of hit points lost
only fires show; `Die-2` on are wrecked tiles. **Corpus**: Guardhouse 1 and 2,
Palace 1, Warriors Guild, Wizards Guild 1 and 2 (`damage::UNMARKED`,
**Measured**).

**Binary**: the set is picked on every attribute write unless an action's
set plays, and as one ends. `sim::damage`.

| Condition | Set |
|---|---|
| hit points at most 0 | `Dead` |
| short of `MaxHP`, `CurrentStageBuilt` 0 | `Build` stage by hit points built (an upgrade's from its `UpgradeFrom`'s `MaxHP`) |
| short of `MaxHP`, `CurrentEvent` 0 | `Die` stage by hit points lost |
| lit: someone inside, or `ForceBuildingState` 2 | `Active` + n |
| unlit, the art has `Inactive`, no Sorcerer's spell | `Inactive` + n |

A stage: the art's sets from the block's first, while each is there, up to
ten, split 0–100 evenly; n is 1, 2, 3, 4 for `CurrentEvent` 1, 2, 4, 3.
**Game**: the defeat save's palace, 95 of 550 hit points, 82% lost of
eight sets, `Die-7`. Falling, a building plays `Crumble`
([bytecode.md](../gpl/vm.md#a-building-falls)).

As an action's set ends, the pick replaces its last frame. So a treasure
chest: `Open_Chest` plays `chest_open` (set `Special`), sets
`ForceBuildingState` 2 and hit points 0; at the action's end it shows
`Dead`, the empty chest (**Corpus**: an emptied chest's record draws
`Dead`). **Measured**.

A one-frame action set ends as it is pushed, inside `PerformAction`, so the
pick replaces it before a frame is drawn. The guardhouse's and the Dwarven
Settlement's `Attack` are one frame of the unlit picture (Guardhouse tile
632); a lit one shooting keeps its lit look (**Game**, the user). The
Ballista Tower's seven frames show.
**Measured**.

`Pose::x`/`y` places a stacked layer against its base. HD `Data`: 1,479 of
10,024 slots have a non-zero offset (`a_pose_offset_places_a_stacked_layer`):

| Kind | layer 0 flat | layer 0 offset | layer *n* flat | layer *n* offset |
|---|---|---|---|---|
| `Die` | 1,081 | 0 | 12 | 1,451 |
| `Active` | 86 | 0 | 39 | 4 |
| `Inactive` | 406 | 0 | 36 | 6 |
| `Dead` | 153 | 2 | 0 | 0 |
| `Walk` | 704 | 8 | 0 | 0 |
| `Interface` | 3,489 | 6 | 0 | 1 |
| `Special` | 616 | 0 | 3 | 1 |

All 1,451 `Die` offsets are fires on layers above the building: the
Marketplace's four at `(-73, 6)`, `(19, 2)`, `(103, 37)`, `(-36, 95)` round a
105 by 116 picture. Unapplied, they pile on the anchor. The four `Active`
offsets are the same (**Viewer**: the Rogues' Guild smoke, `Active` layer
1, lifted to its pipe top).

No `Die` layer 0 has an offset on any subtype; `Attack`, `Build`, `Carry`,
`Cast`, `Crumble` have none, so death animations do not move off the spot.

A version 2 slot opens with its footprint and keeps the offset at `+0x18`,
zero on every one shipped (read at `+0x00`, false offsets appear). The 16
real layer 0 offsets (two `Dead`, eight `Walk` on `BPA5`, six `Interface`)
are not applied; none is scatter. `a_pose_offset_places_a_stacked_layer`.

**Binary**: the engine never reads a layer 0 slot's offset: the draw, the
hit test and the frame box union read it only for a layer above 0, from
that layer's slot. So the 16 are dead data. Strictly the draw reads it for
any part but 0: a layer 0 with flags bit 9 makes part 1, its shadow, which
takes the offset — `WPm1` in `DataMX`
([the second pass](#the-second-pass-is-a-fliers-shadow)).

### A set names the particle system its parts carry

**Binary**: set header `+0x08`, versions 4 and 5, is a `DUNT` id. A set
change first stops every system of an old part's code on the unit, then
makes one per new part, hung at the part's offset. HD: 947 sets; `XLSL`
763, `XLSH` 166 — a building's `Die` stages, one per fire layer — and
projectiles' and effects' own (`XL23` on the ratapult ball's flight, `XL31`
on an ice breath's impact). `Image::particle_system`,
`a_set_names_the_particle_system_its_parts_carry`; the units,
[bytecode.md](../gpl/vm.md#particle-systems).

**Binary**: a particle is a frame of its system's own art (`ImageIDBase`),
in the set the system's unit plays, its slot drawn at random where the set
has several; its frames step by the slot's playback word 2, else the
set's. It is blitted at its own place as any unit's frame: the tile's draw
mode decides, with no scale or tint. Its alpha below exactly 1 draws mode 8
instead, at level `(1 − alpha) × 9` truncated: `Smoke_Large`'s tiles are
mode 0, yet it glows in and out. `blend::fade`,
`a_particles_alpha_is_mode_8_at_nine_tenths_of_its_lack`. `Sprite::of`,
`a_burning_buildings_smoke_reads_its_descriptor`; the motion,
[bytecode.md](../gpl/vm.md#particles).

### The slot trailer is playback, and version 5 adds effects

**Binary**: after the frame table, versions 4 and 5 carry a 20-byte playback
when slot header `+0x10` is non-zero; every set has the same five words at
set `+0x14`, kept unless all zero. The slot's is taken, else the set's: word
0 1 gives animator mode 1, 2 gives 4 or 2 and 4 gives 5 or 3 by word 1 bit 0,
anything else 2; word 2 is the frame time.

| Word | HD `Data` and `DataMX`, slots |
|---|---|
| 0 | 0, 1, 2, 4 |
| 1 | 0 or 1 |
| 2 | 0 to 500, mostly in tens; game ms, below |
| 3, 4 | 0 |

**Binary**: an `Animate` or `InitAction` takes the set's word 2 as rate (0
gives 100, rounded to whole frames), steps a frame a run and re-arms with
it; an order runs at most once a 50 ms frame. So a frame shows
`action::frame_ms`: none 100 ms, 150 150, under a frame 50. Negative words
(`APB2`, `PPa1`, `PRc1`, one `Die`: −500 to −50) give rate 1, so 50. HD sets
by frame time: 50 ms 80, 100 ms 5,116, 150 93, 200 24, 250 4, 300 45
(`a_sets_frame_time_is_its_playback_as_an_order_plays_it`). `Walk` while
walking is not this: a step moves the frame on
([movement.md](../original/movement.md#a-step), `Walker::stride_ms`).

Version 5 then reads one list per set bit of slot header `+0x14`, `[u32 n]`
and `n` 32-byte entries, each list replacing the last. An entry names
another `IMAG` record, set and slot, with an offset: word 0 flags (bit 0
used, bit 2 fixed slot, top byte the index), 2 and 3 x and y (screen
pixels, y down), 5 the record, 6 the set (`0x40` on all), 7 the slot; 1 and
4 zero.

**Binary**: each entry is an extra part (layer) of the unit, not a unit nor
a particle system. After the layer parts, a part is added per used entry of
each first-pass part's slot, last entry first, where the record and set
exist and the set has layer flag `0x100`. It plays with the set from frame
0, at its set's frame time (100 ms: none has a playback). Draw, place and
pick add the entry's offset. **Inferred** from the part order: entry 0
draws on top.

| Record | Name | Art | Carried on |
|---|---|---|---|
| `FXa1` | `All_Damged_Fire` | 23 slots, 20 or 24 frames, mode 8 fire | `Die-1`..`Die-6` |
| `XR47` | `DustofDeth` | 1 slot, 21 frames, mode 8 dust | `Crumble` |
| `XR46` | `firebalm_part` | 32 slots, 4 frames, a trail per direction | `WPl1`, `WPm1` walk |

**Corpus**, HD (`every_slot_effect_resolves_to_a_part`): 25 records, 415
entries: 22 expansion buildings and lairs in
`DataMX`, `BB0t Krolm_Altar`, and the two firebalm missiles. Only on layer
0 sets; entries grow with damage (`ABl1`: 1, 2, 2, 3, 3, 5). Base buildings
carry fire and dust in their own layers; these expansion buildings only
here. Every `XR47` carrier's `Crumble` slot has 21 frames, bar `BBx1`'s
second (20).

`Slot::playback`, `Set::playback`, `Slot::effects`, `imag::Effect`,
`imagery::effect`, drawn by `regent_draw::map::cel::Drawing::layers`;
`a_slots_trailer_is_its_playback_and_effects`,
`the_bazaars_last_damage_stage_carries_five_fires`,
`every_slot_effect_resolves_to_a_part`.

### A slot's block 0 is a footprint

**Binary**: the footprint is a slot's block 0 as `left`, `top`, `right`,
`bottom`, pixels about the anchor, when `left < right` and `top < bottom`.
Otherwise the frames' tile boxes are unioned — an outline, not a footprint.

Order: block 0 of slot 0 of set 8, then `0xd0`; failing both, the union on
set 8, `0xd0`, then `0x40`. Set `0x40`'s slot rect is never tried, so a
record with only a `Special` set is measured by its pictures — six in HD:
`ARB2`, `ARE1`, `NGe1`, `NGf1`, `NGl1`, `YG02` (three are scatter).
`a_special_sets_slot_rect_is_never_the_footprint`.

The union is narrower than the set's pictures. Three rules, each changing real
art:

* Frame 0 of each stream of slot 0 only, so a 23-frame animation contributes
  one picture.
* The frame's own nudge: two `i16` in the frame's metadata word
  (`Frame::metadata`). 2,895 of HD's 86,868 frames carry one, `(-196, -149)`
  to `(110, 130)`. `a_frames_metadata_word_is_a_nudge`.
* Every layer of the set (matching the id's low 24 bits), each layer above
  the first offset by its own pose offset.

Frames whose reference has bit 30, 27 or 28 are skipped. A
degenerate box counts as none.

In the file block 0 starts a version 2 slot; in later versions it is the
first block after the header, present only when mask bit 0 is set (a missing
block reads as zero). Taking `+0x14` in every version is right only for
version 4 with bit 0 set. HD `Data`: 3,320 of 10,024 slots hold a proper
rect, 6,494 nothing, 210 a point or line (rejected like empty;
`a_slots_footprint_is_where_the_thing_stands`).
`CG07 scorch_dead_tree` `(-15, -21, 14, 27)`, `NGa1 SnowPine`
`(-32, -19, 22, 22)`.

The reader is the generator's scatter pass: rect divided by a cell, spot refused
unless all its cells are free — see
[maps.md](maps.md#the-mobject-chart-scatters-in-clumps-and-a-save-holds-its-output).
Top and bottom are first scaled by `32 / (32 sin(IsoCameraAngle))`: a cell is
32 pixels across the screen, 26 deep.
`a_slots_footprint_is_where_the_thing_stands`.

The Phantoms Haunt mod writes the same field: to make a new building burn it
writes `<hh>` at slot `+0x00` of sets `0x03000063`–`0x03000067`. Its reader
also takes the slot table from set `+0x40`, frame count from slot `+0x04`'s
high half, the tile from a frame pair's second word.

`a_pose_offset_places_a_piece_and_die_means_two_things` in
`crates/regent_core/tests/view.rs` uses Barbarian as control: every slot of
every set at the origin. Three shape checks, from the bytes:

| Record | Shape |
|---|---|
| Barbarian `Walk` | eight facings of eight frames |
| Healer `Cast` | eight of nine frames, two streams |
| Ranger `Die` | sixteen frames in seven facings, fifteen in the eighth |

Ranger `Die` is the archive's only asymmetric one: the canary for a parser that
derives frame counts from pointer spacing.

### Which way a slot faces

| Link | State |
|---|---|
| compass → board coordinates | **Game**, from the game's own sources |
| board coordinates → screen | **Viewer**. See [saves.md](saves.md#north-is-up-the-screen) |
| screen angle → slot | **Viewer**, by looking |

All three in [`content::terrain::compass`](../../crates/regent_core/src/content/terrain/compass.rs).

`OriginalQuests/GPL/LowLevel.gpl`'s `randomedgeCoord` (unchanged in
`mx_LowLevel.gpl`) spawns monsters 30 cells in from a map edge: North at
y 30, South at `y_size - 30`, East at `x_size - 30`, West at x 30. So on
the board north is −y, south +y, east +x, west −x, origin north-west.

Slot *i* of *n* faces *i*/*n* of a turn clockwise from up — a fraction, not
eight compass points. **Viewer**, with the ring: `AVB1 Barbarian`'s eight
`Walk` slots (0 up, 2 right, 4 down, 6 left), `APA2 Ranger Arrow`'s 32 (8, 16,
24 at right, down, left), and other characters. Only characters and projectiles
are evidence; building slots are emplacements (below).

The engine counts 32 directions, not slots. The SDK's `SetEffectorDirection`
takes 0 to 31, 0 north, rising clockwise; every mobile record in a save
stores that. A slot is that direction spread over the set's count. See
[saves.md](saves.md#north-is-up-the-screen).

No mirroring: `AVB1 Barbarian`'s eight `Walk` slots hold 64 distinct tiles, the
Ranger Arrow's 32 slots 32, no tile repeated. Every facing is drawn.

### The attachment blocks are points

The mask at slot `+0x08` adds an 8-byte block per set bit. **Binary**: a
block is two points, `(x, y)` twice; the engine keeps three blocks, six
points, point `k` in block `k / 2` (`Slot::points`). Only bits 0–2 are ever
set. **Corpus**, every install slot: most have no block, many all three,
the rest one or two. In multi-slot sets a block is mostly fixed to the unit;
it varies across slots most on `Cast`, `Hotspot` and `Attack` sets, and a few
`Attack` sets draw identical frames and still move a block (below).
`regent_cli imag --record` prints a
slot's blocks as point pairs.

What a point means depends on the set:

- `Hotspot` (400) slot 0: points 0–2 are attachment points 1–3, point 5 their
  base. **Binary**: id 1, 2, 3 maps to point 0, 1, 2; any other id fails and
  the overlay falls back to set `0xd0`. An overlay's `AttachmentPointID` of
  0 is taken as 1. So the id is one-based with 0 the default, and names a
  point, not a mask bit. `imag::attachment_point`.
- Sets 8, `0xd0`, `0x40`: block 0, 1, 2 respectively are rects, the unit's
  bounds (**Binary**); set 8's block 0 is also the footprint.
- `Attack` on a building: points 4 and 5 are the firing position (below).

A point is measured from the tile anchor (`0x0a`): tile pixel
`(anchor.x + x, anchor.y + y)`, `y` downward. **Viewer**, every visible case:

| Record | Points |
|---|---|
| `ABE1 Guardhouse` | six, one at the outer edge of each face, parapet height |
| `ABb1 Dwarf_Settlement` | four, one on each corner tower |
| `ABB1 Ballista_Tower` | eight round the roofline |
| `AVH1 Ranger` | per facing, on the bow stave (art changes per facing) |

Marked on the tile (tile 2874 of `maindata.cam`: `97,-8 100,90 -91,94
-86,-9`), each pair lands on the drawn feature. This also settles header field `0x0a`, which no corpus statistic can.

Oddity: the Guardhouse's slot 0 lands six pixels above the canvas (`y` −57,
anchor `y` 51). All other points of the three buildings are inside the picture.

### The points are emplacements, and the buildings say so

Five buildings' `Attack` slots draw identical frames:

| Record | `Attack` slots | Frames per slot | What differs |
|---|---:|---|---|
| `ABB1 Ballista_Tower` | 8 | the same seven tiles | points 4 and 5 |
| `ABE1 Guardhouse` | 6 | the same one tile | points 4 and 5 |
| `ABE2 Guardhouse L2` | 6 | the same one tile | points 4 and 5 |
| `ABb1 Dwarf_Settlement` | 4 | the same one tile | points 4 and 5 |
| `ABm1 Outpost`, expansion | 8 | the same art | points 4 and 5 |

Only bit 2's block changes, its two points a few pixels apart: the slots are
firing positions. **Game**: the Ballista Tower's art ignores firing direction;
its portrait shows the ballista hidden in the tower.

Counts follow the architecture, not a compass. Tile 632 is a hexagonal tower
with a slit per face; tile 2874 a compound with four corner towers. The Dwarf
Settlement's points span about 190 by 100 tile pixels, its towers about 187
apart across and 110 down. `ABb1`'s four begin at north-east; `ABB1`'s eight are
up to 35° off at east and west (arm above the roof, points placed in 3D).
`a_facing_can_carry_a_firing_point_when_the_art_does_not_change`.

Not `ATCH`: `PRIM`'s `ATCH` subtree has a `WEAP` child with two slots, zero in
every record — a descriptor-side weapon attachment, unused.

Not a damage ladder: building set counts per `Die` id rise and fall.

### The set ids have an oracle

HD's `Data/ImageSetIDXRef.xml` is a `NameToIDTable` of 367 distinct ids
(`image_set_kinds_match_the_cross_reference`);
`data::imag`'s kind table is checked against it. What a corpus-only
reconstruction gets wrong:

- `MapTexture` is its own family: a 16 by 16 grid of terrain blends at
  `5000 + 16 * from + to`, named `MapTexture{A..P}to{A..P}`, up to 5255.
  `UnitTexture` is 4000–4004 only.
- Five families: `High-Power-Active`, `High-Power-Idle`, `Low-Power-Active`,
  `Low-Power-Idle`, `Unpowered`.

Two ids used in the install are in no table: 196 and 212, each one past the
end of the `Active` and `Inactive` blocks. They stay unnamed and are pinned
as the only two. **Binary**: no code immediate names either.
