# The minimap

The original's `GSOverheadGameView`, a subclass of `OverheadGameView`.
**Binary** unless marked. Code: `regent_core::view::panel::minimap`.
`minimap::Cache` keeps the ground, a cell redrawn as its vertex is explored
(**Measured**);
`minimap::draw` puts a world's units, marks, reach circles and frames over
it. Drawn whole for a save of a hilly map, it matched both captures of it
pixel for pixel (**Measured**).

```text
set-up: control 2001's rect -> view id 5, cursor mode Mini
  scale = rect width / map cells, 16.16
  frames: the main view (white), mini view 1 (light grey)
a frame, per dirty rect:
  the ground, blitted from a cache
  reach circles; buildings, vehicles, reward flags, message flags
  the frames, 1 px
```

## Size

- The rect is `APMK` control 2001: `(4, 3)`, 176 × 176 in every HD
  `UIData` from 1024 × 768 up. The view draws no border. Test:
  `every_main_screen_puts_the_minimap_at_one_rect`.
- Scale = width / map cells in 16.16; height plays no part, so a map is
  square on the width's scale. Block = ceil(scale): 2 at 128 cells
  (1.375), 1 at 256 and 512. Picture ceil(scale × cells) each way: 176.
- A point in 32nds of a cell maps by `fixed_div(v, 32 / scale)`, the
  divisor truncated in 16.16.

## The ground

- Built per map into a cache: redrawn whole when marked dirty, else per
  queued rect. **Inferred**: the map's change notices (exploring, paint)
  queue them.
- Every tileset tile is sampled into a block × block entry: pixel `(x, y)`
  at steps ceil(32 / block) from the top left — `(0, 0)`, `(16, 0)`,
  `(0, 16)`, `(16, 16)` at block 2; `(0, 0)` at block 1. Raw 565 words: no
  light, no shade, no water.
- Rows outer, cells inner, each square at floor(cell × scale); a cell
  starting on the pixel the last one did is skipped (big maps: the first
  cell wins). At 1.375 squares overlap and the later cell writes over. The
  tile is the cell's own (set `5000 + pair`, case, variant, rotation;
  `Tileset::picture`).
- Fog: a cell whose top-left vertex the local player has not explored is
  black. Units need a seen level above 0.
- Filter bit 2 clear (a main-menu option, saved in the prefs): explored
  cells flat grey `0x3f3f3f` instead. Default all bits on (`0x7fffffff`).

## Units

- Lists in order: buildings, vehicles, reward flags, message flags
  (`MSGF`), each in its own order.
- Shown: seen above 0, not held, `ShowMinimap` bit clear. A building needs
  filter bit 4, `NotInMiniMap` clear, standing; a mobile bit 3, a character
  part (missiles never), `NotInMiniMap` clear, not `APV:37` invisible,
  `APV:45` camouflaged or a speed-trail clone (`APU:8`). Other kinds
  always.
- A `CombatSignal` flash skips the unit on an odd count: it blinks every
  200 ms (`blinked`, `World::flash_count`).
- With set 300 in its art: that set, slot 0, the unit's current frame, at
  its point by the tile anchor; palette indices 0–15 take the owner's
  colour ramp (none for owner −1). Heroes are a 3 × 3 icon, the palace a
  10 × 10 cross.
- Else a filled square about the point: building 5 × 5 (x − 2 … x + 2),
  character 3 × 3, `BBi2` 3 × 3.
- Colour index: another player's unit its owner, grey 9 when down; the
  local player's white 8 when selected, when a `HOME` or roster member is
  selected (not the palace), or a caravan while a Marketplace or Trading
  Post is selected; else its owner, 9 when down. So a selected reward flag
  lights its seekers, the heroes whose `HOME` holds it
  ([panels.md](../formats/panels.md#a-flags-seekers)): white, same size, no
  blink; a selected hero lights its flag.
- Colours: players 0–7 are `PALT PLYR` entries 1–8 in `interfacedata.cam`
  — blue, red, teal, purple, light grey, brown, dark green, yellow; 8
  white, 9 `(90, 90, 90)`. Slot 7's yellow is the monsters' (**Game**:
  small yellow squares are monsters, large ones their lairs, as the 3 × 3
  and 5 × 5 rule gives). A square takes the colour; a ramp is black →
  colour at index 7 → white at 15, index 0 30% colour (`ramp`). **Game**:
  the palace cross's 4, 5, 7 read `(0, 0, 104)`, `(0, 0, 136)`,
  `(0, 0, 216)`.

## Frames

- The main view's frame is `0x80ffffff`; mini view 1's, when present,
  `0x80cccccc`. Drawn last, in that order, as 1 px outlines inside the
  rect. **Game**: the smaller frame is the follow-camera panel's; the
  captures read it `(200, 204, 200)`, `0xcccccc` in 565, not white.
- A frame is the view's rect on flat ground in 32nds (x / zoom,
  y × 32 / (32 zoom sin 55°)), lowered by the ground's lift at
  `((right − left) / 2, bottom)` — half the width, not the middle — times
  the camera's height scale, kept within the map; to pixels with width and
  height converted apart. Updated as the view moves (**Inferred**).
- **Game**: width 73 for the 1,710 px main view, as predicted
  (`the_main_views_frame_is_as_wide_as_the_view`); height 58 where
  1,136 px predict 59 (open).

## Input

Cursor row `Mini`, playing only:

- Only a point inside the minimap counts.
- Left press or drag: the main view centred on the point lifted to the
  ground.
- Right press or drag: mini view 1 centred there; it stops following.
- Left double click: an explore flag at the point, the panel's reward;
  sent so the flag is selected.
- Map modes set views 0–4 only, so the minimap stays `Mini`: no spell or
  attack flag is aimed on it.

## Reach circles

While a reach spell's cursor row (`Sp10`–`Sp14`) is entered, white
midpoint circles about each local Wizards Tower and Guild with hit points
and `APB:1`: radius the spell range in pixels, Guild 2 one bonus more,
Guild 3 two.

Only those five cursor rows turn the circles on and off; no other code
touches the switch. So placing a building, a Wizards' Guild or Tower
included, shows no circles, and none is drawn for the preview. openregent
shows both as an enhancement (`World::placing_reach`,
[openregent/enhancements.md](../openregent/enhancements.md)); test
**Measured**.

## Marks

A mark is the beacon a script flashes on the minimap: a unit only the
minimap draws, flashing its set 300 twice and then deleted. Code:
`regent_core::sim::world::minimap_mark`, `World::minimap_marks`; tests
`tests/marks.rs`.

### The descriptors

**Binary**, read by `regent_cli unit` and `regent_cli imag`: four `Character`
descriptors in `unittype.cam`, art in `maindata.cam`, each one slot of set
300 (`SetKind::Minimap`) and no other set. Every frame is draw mode 0, one
colour, anchored at its centre. Test: `the_shipped_marks_flash_their_set_twice`.

| Code | Name | Frames | Playback | Colour | Sizes | Made by |
|---|---|---|---|---|---|---|
| `MVa1` | `event_beacon` | 8 | 100 ms | (255, 255, 152) | a ring, 3×3 to 23×21 | monster births, quest events |
| `MVa2` | `hero_death` | 20 | 100 ms | (137, 186, 255) | 15×15 squeezed to a 27×1 line, then back to 15×20 | `gravestone` |
| `MVa3` | `victory_star` | 8 | 100 ms | (255, 255, 152) | a star, 1×1 to 17×19 | the won game's end |
| `MVa4` | `treasure_find` | 10 | 150 ms | (255, 255, 152), brown in frame 0 | at most 8×14 | `Treasure.gpl` |

- `Info`: `Static`, `Directionless`, `DontBlock`, `NotVisibleInISOView`, so
  the main view never draws one. `hero_death` and `treasure_find` add
  `NotVisibleToOpponent`: their owner alone sees them.
- `DefaultSound` 0 (SDK `M_Characters.xml`), and `MiniMapAnimation` plays
  none: a mark is silent.
- The colours are `SPLT` 25, 26, 27 of `maindata.cam` (**Corpus**, read
  by `view::Sprite`). **Inferred** from frame sizes and pixel counts: the
  beacon a growing ring, the death a square folding to a line and back,
  the star growing.

### Its life

**Binary**:

- Made at a coordinate (owner −1) or at an agent's unit's point (its
  owner); none: nothing; an unknown name throws
  (**Measured**). The next handle, kind 1, no
  agent, no place search, no world draw. It never moves.
- An `OrderCounter` on track 1, run on push: frame 0 to the slot's last,
  a frame a run, every `interval(rate)` ms; the rate is the set's
  playback, else 100. Each run sets the frame the minimap draws.
- Loop count 2: the second time the counter reaches the end, the order
  ends and the unit is deleted after that frame's orders. So it shows
  frames 0 to n − 1, then 0 to n − 2, and stands `(2n − 1) × interval`:
  `event_beacon` 1,500 ms, `hero_death` 3,900, `treasure_find` 2,850.
  **Measured**.
- Message and reward flags take the same order, its info word set so their
  set 300 loops for good.
- The victory star is made in app state 8, which runs no orders: it stays
  on frame 0 (`World::end_point` gives its point; not a `Mark`).

### Drawing

**Binary**, as for [units](#units):

- A unit whose art has set 300 draws that set's current frame at its
  minimap point; any other unit a filled rect in its owner's colour. So
  heroes, palaces and flags draw their own one-frame set 300 icon.
- A mark's owner always sees it; others where its cell is explored, unless
  `NotVisibleToOpponent` (`World::mark_seen`;
  **Measured**).
- A kind 1 unit, a mark too, needs filter bit 3 on; buildings need bit 4.
