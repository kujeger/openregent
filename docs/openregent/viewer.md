# The art browser, and the resolution layer under it

`regent_core::data` joins the formats and `regent_core::view` resolves them for the
screen; neither has a user interface. `regent_viewer` is a
Bevy binary that browses units' art and the tileset, on a drawing layer, `regent_draw`, that
[`openregent`](game.md#the-game) draws with too.

## Running it

```sh
cargo run -p regent_viewer -- ~/Games/majesty/data ~/Games/majesty/datamx
```

Takes archives and directories, like `regent_cli units`. Opens on the unit
browser.

| Input | Does |
|---|---|
| Middle drag, or left drag with Left Shift | Pan |
| Wheel | Zoom about the pointer |

`regent_draw`, `regent_viewer` and `openregent` are workspace members, so root
`cargo build`, `cargo test` and `cargo clippy --all-targets` compile Bevy.

Where the code lives:

```text
 regent_draw    data (Corpus), map (redraw, Paint, Maps), stage (View, camera,
             viewport), material, texture, terrain (Ground), shadow,
             shaders, art (the art service), log
 regent_viewer  main, browser (the panels), units (the unit lists, Selection),
             sprites (the unit stage), tiles (the tileset browser), grid
 openregent    the game: sim, menus, panels, audio, saves, settings
 bink        the videos' decoder, vendored; openregent plays them with it
```

Bevy gives a `.cam` `AssetReader`, `TILE` to GPU texture, and `IMAG` to
animation clips with pivots and playback order. The game's interface is skinned
sprites from `interfacedata.cam`, not widgets; the viewer's panels use
`bevy_egui`.

## The resolution layer

```
DUNT ──ImageIDBase──> IMAG ──frame refs──> TILE ──palette──> SPLT
  │                     └── sets and slots: pose, facing, playback order
  ├──DefaultSound────> DSND ──phases──> WAVE
  ├──SoundOverride───> DACT
  ├──MOVE slot───────> DMOV
  ├──HelpID, DialogID> STRT
  └──GPL callback────> .bcd section 1
```

Three facts fix its shape (**Corpus**, `tests/view.rs`):

- Every join crosses an archive: no `DUNT` record names an `IMAG` in its own
  archive. Units sit in `unittype.cam`, art in `maindata.cam`.
- Ids are not unique across archives: 12 ids appear in more than one, so
  lookup needs a scope (`repeated_ids_are_recorded_not_hidden`).
- Most units resolve; the viewer shows those that do not.

`MajestyDatasetDefinitions.xml` gives the scope: which archives load together,
in what order. See [formats/README.md](../formats/README.md).

```
crates/regent_core/src/data/
├── catalogue.rs  the loaded archives and the (tag, id) index over them
└── text.rs       the STRT tables that name things
crates/regent_core/src/content/
├── dataset.rs    which archives, in order
├── imagery.rs    IMAG sets and slots, resolved
├── unit.rs       DUNT -> stats, animations, sounds — as locations
└── terrain.rs    the tileset and the DTRN that names its classes
crates/regent_core/src/sim/
├── paint.rs      painting a class onto a map, the engine's way
├── placement.rs  the ground a building takes when it is put down
└── footprint.rs  the rect the engine asks an object's art for
crates/regent_core/src/view/
├── sprite.rs     TILE + palette -> indices, coverage and colours
├── schema.rs     the 17 blend cases
├── board.rs      a map assembled from them
├── ground.rs     a map's terrain classes, one per vertex
├── cut.rs        a ground cell's two triangles, values Gouraud over them
├── compass.rs    north, in GPL's terms and on the screen
└── raster/       the software renderer's rules: 565 colour, the
                  247–254 band, lighting, translucency, a CPU sheet
```

The library's layers and their order are in `lib.rs`; "When to split this
crate" there says what each depends on.

### Rules

- **Report ambiguity; do not resolve it silently.** The catalogue records both
  matches and applies the dataset's load order. The order's direction is
  unknown, so only the cases are known, not the rule: 12 (**Corpus**, no
  test). They come from composition: neither dataset alone repeats an id.
- Flagged chunks stay out of the id index. Their record id is their position,
  so every tile archive has a tile 0.
- **Locations, not pixels.** A frame resolves to an archive and a tile number;
  `sprite.rs` makes colours on request. A sweep checks every reference without
  rasterising every tile.
- A `Sprite` keeps indices, coverage and palette apart; `Sprite::rgba` flattens
  on request. Player colours and the GPU material need the indices. Coverage is
  its own plane: index 0 is a colour and an RLE gap is the only transparency,
  so no spare index can mark it.
- Model only holes the corpus has. `Colour::Indexed` has no `Option<Palette>`:
  no indexed tile names a missing palette, in HD and the Workshop
  mods (**Corpus**; HD in `every_tile_in_a_dataset_resolves_to_pixels`), so a
  missing palette is an error. `FrameSource::Missing` stays: a tile number past
  a chunk's end is bounded by a computed count, not a measurement.
- `Sprite::rgba` widens 565 by bit replication. The screen does not: the
  original draws a 16-bit frame and shifts each channel up, low bits zero
  (**Binary**, [formats/terrain.md](../formats/terrain.md#the-colour-on-screen)).
  The game's shadow resolve pass puts every pixel on that grid
  (`lit::present`); `lit` lights the ground before it.
- Absent and empty archives are separate lists. `cinedata1.cam` is present
  and zero bytes.
- The two datasets partition. `Dataset::library` loads only archives a definition
  names and ignores `base="Majesty"`, so the expansion is its own 17 `mx_`
  archives. Direct-colour tiles: 1,140 + 1,140 = 2,280, every one in the
  install, once (`the_datasets_account_for_the_shipped_archives`,
  `every_tile_in_a_dataset_resolves_to_pixels`).

### What it asserts

All in `tests/view.rs` (**Corpus**).

Base dataset: 394 units, 385 with art, 3,428 animation sets, 6,540 poses, 59,937
frames each naming an existing tile; 93 of 93 `DMOV`, 195 of 195 `DSND`, 5 of 5
per-action overrides (`every_unit_resolves_what_it_names`). Every tile in both
datasets resolves (20,738 and 10,873, `every_tile_in_a_dataset_resolves_to_pixels`) and every indexed tile finds its palette.

On all 59,937 frames: a four-character id is looked up across the dataset, a
tile number is counted inside one archive, and that archive holds the `IMAG`,
never the unit.

26 frames draw nothing: five distinct tiles, all but two on buildings. A tile of
bare row terminators is legal, so the test names them as content
(`every_unit_frame_composes_to_pixels`).

`Unit::at` passes on a failed `PRIM` tail read. The tail parses on all 583
shipped records (`unit_prim_tails_account_for_every_byte`); swallowing the error would silently cost a unit its sounds.

### Two `regent_cli` commands

```
regent_cli units [--kind <k>] <file.cam|dir>...   one line per unit
regent_cli unit  --record <ID> <file.cam|dir>...  one unit, everything it names
```

A whole directory also loads the `mdl1_` archives no dataset names, adding
ambiguous ids. The header prints the load order, empty-archive
count and ambiguous count.

## The unit browser

Unit list with subtype and text filters; resolved data on the right; the
selected frame in the middle, with set, pose, stream and frame sliders, zoom and
toggles.

- **A sprite is placed by its anchor.** Canvases in one set differ in size and
  figure position, so centring makes a walk cycle slide. Bevy's `Anchor`
  carries it, so gizmos need no correction.
- A character opens on `Walk`, slot 5 (lower left, as the game shows heroes). A
  Ranger's `Stand` is eight single-frame poses, so nothing would play. Anything
  else opens on its first set.
- **Interface** lists `IMAG` records no subtype covers: every one in an
  archive named `*interface*`, plus the
  four ring images in `maindata.cam`. One row per archive. A row names the
  use the library knows: `INDe` "crown, nothing selected",
  `IX72` the expansion's, `INBc` health bars, `AVn1`/`2`/`8`/`9` rings
  (`view::panel::portrait`, `view::map::ring`). The filter searches these words too, so
  "crown" finds `INDe`. A picture opens on its known set
  (`interface_archives_and_the_rings_make_the_interface_list`,
  `the_interface_filter_shows_pictures_and_no_units`).
- Each set row in the detail panel is clickable and marks its use. A unit's
  `Interface` sets show as buttons: portrait (1000), hero panel backdrop
  (1001), list icon (1002) (`a_units_interface_sets_are_named_by_id`).
- Blit is per set, as for units: `Interface` sets flatten with
  `Blit::Interface`; a ring's `Stand` is world glow, drawn over the
  backdrop colour (dirt).
- Playback shows each frame the set's time, `action::frame_ms` (**Binary**,
  [art.md](../formats/art.md#the-slot-trailer-is-playback-and-version-5-adds-effects)),
  looping unless toggled off. `IMAG` also holds a frame by repeating its
  reference.
- **The ring** draws every pose of a set on a circle, slot 0 up, the rest
  clockwise, each labelled with its slot number. Art that disagrees shows the
  real order. The status line counts distinct tiles against poses (the
  mirroring question). Radius is a multiple of the widest sprite; `spread` below
  1 overlaps neighbours. Two passes: the radius needs every frame decoded.
- **Attachment marks**: circle and cross per point, two per block a pose
  carries — red bit 0, green bit 1, blue bit 2 — also in the ring, so a
  building's eight firing positions show against its eight pictures.
  Measured from the tile's anchor (**Viewer**: from it the Guardhouse's six
  firing points land one per face). Gizmos, not entities.

Format facts: [formats/art.md](../formats/art.md).

## The tileset browser

Every set of the tileset as a labelled sheet: a body set as its
`frames x 4` grid, a blend set as seventeen rows, one per case. Each sheet
is one composited texture: per-cell sprites at fractional zoom round
apart, leaving gaps. Format facts: [formats/terrain.md](../formats/terrain.md).
