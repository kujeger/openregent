# `.rgs` and `.q` — the generator charts and the quests

A quest is a recipe; its ground is generated at play time. See
[saves.md](saves.md).

> Part of [README.md](README.md).

## Framing

**Corpus.** Both open with a four-character tag, repeated at offset 8:

```
0x00  char[4]  tag
0x04  u32      0
0x08  char[4]  tag, repeated
0x0c  u32      0
```

The fourth character is a version; versions ship side by side:

| Extension | Tag | Files |
|---|---|---|
| `.q` | `RGM6` | 19 |
| `.q` | `RGM9` | 13 |
| `.q` | `RGMa` | 4 |
| `.rgs` | `RGCA` | 1 |
| `.rgs` | `RGCB` | 3 |

`RGM` is a quest map, `RGC` a generator chart. A decoder must handle all three
map versions. `quest_and_chart_files_are_recognised`.

Both belong to the `RGS*` classes; one module reads both.

## The chart body — `RGCA` and `RGCB`

**Corpus**, in `data::rgs::chart`. All four shipped charts parse to their last
byte.

A `.q` names a pattern; the pattern names terrain classes and objects, sometimes
through another pattern.

```
[16-byte framing]  [84-byte preamble]
  [u32 n] record x n     terrain    -> DTPT classes, weighted
  [u32 n] record x n     surface    -> terrain patterns
  [u32 n] record x n     object     -> DUNT ids: trees, rocks, ponds, ruins
  [u32 n] record x n     landscape  -> object patterns
  [u32 n] profile x n    fractal    -> named height profiles
```

| File | Tag | terrain | surface | object | landscape | profiles |
|---|---|---|---|---|---|---|
| `data/constants.rgs` | `RGCA` | 27 | 21 | 73 | 104 | 9 |
| `data/constants_down.rgs` | `RGCB` | 27 | 21 | 73 | 104 | 9 |
| `datamx/mx_constants.rgs` | `RGCB` | 31 | 33 | 150 | 182 | 14 |
| `datamx/mx_constants_down.rgs` | `RGCB` | 31 | 33 | 149 | 182 | 14 |

`every_shipped_chart_parses_and_fills_its_file`.

A record:

```
char[4] key         what a reference uses, and the map's key
char[4] code        the same four characters again
cstr    name        `#Plains_Mostly_Parent`, `$Mayhew_Forest`, `Rocky`
u32     0           a flag
u32     0           a flag
u32     count
  ( char[4] id  u32 weight  u32 ?  u32 ?  u32 ?  u32 ? ) x count
```

**Binary**: the game reads the body in that order; the two flag words become
booleans. Records whose key or code is `NONE` are dropped on load.

Preamble: one dword, 4,000 in every shipped chart, then five 16-byte records of two
ranges each, 0–100 and 0–20. **Binary**: only record 2's second bound is read,
by the surface blotch pass. `RGCB` adds a four-byte version stamp, `"Rel@"` in
`mx_constants.rgs`.

Key records by the repeated code, not the name prefix: `#` on most, `$` on a
few named for game places, `%` on the expansion's, none on `Rocky` or `Dirt`.
Keying on `#` loses records.

Only the first word after an id is read, as a weight. Mixes match names:
`*_All_*` one entry at 100, `*_Mostly_*` roughly 78 against 30; weights need not
sum to 100. The other four words are undecoded, except `rest[0]` and `rest[2]`
(below).

A profile has no entries and three words instead. Profile names are the `Bump`,
`Roll`, `Stee`, `Moun`, `FS01` height codes third-party notes call a quest's
`ref3`.

### The five sections are the five `RGS*Chart` classes

`RGSTerrainChart`, `RGSSurfaceChart`, `RGSMobjectChart`, `RGSLandscapeChart`,
`RGSFractalChart` (RTTI). The order comes from what each section references:

| Layer | References |
|---|---|
| terrain | `DTPT` ids and nulls, nothing else |
| surface | terrain pattern codes only |
| object | `DUNT` ids: `IG02 pine_trees_big`, `PG04 grey_rocks_med`, `CG01 scorch_ponds_big`, `BGK2 ruined_altar2` |
| landscape | object pattern codes only |

`every_chart_reference_resolves_to_a_record_or_a_pattern`, every chart.

One id resolves to nothing: `CLER`, in six object patterns, all `#DEMO_*`,
always last with a substantial weight. **Inferred**: a "leave empty" sentinel
(the object-layer `NONE`), or a dangling reference to cut content. Pinned at six:
`only_the_demo_patterns_reference_the_unexplained_clear`.

### A weighted pick takes its bucket on `>=`

**Binary.** A weighted pick sums the weights, draws `rng_next(total)` and
accumulates, stopping when the sum reaches the draw. So weights 76 and 25 give
the first entry draws 0–76 of 0–100: 77 of 101, not 76. The landscape layer's
pick returns the index instead of the code, same rule. `sim::mapgen::pick`.

Reading it as `>` puts one draw in a hundred in the wrong zone and shifts every
later draw: the region chart then matches the zone mask far worse.

### The editor's terrain names disagree with the descriptors'

**Corpus**, unexplained. The terrain patterns with no prefix do not match
`DTPT`:

| Pattern | Points at |
|---|---|
| `Grass` | `GR00 Dirt` |
| `Dirt` | `GR01 Plains_parent` |
| `Soil` | `GR02 Plains_child` |
| `Rocky` | `GR03 Arid_parent` |
| `Scorched` | `GR04 Arid_child` |
| `Swamp` | `GR05 Scorch_parent` |

Each is one rung off. **Inferred**: a vocabulary from before a renumbering. The
`#` patterns quests use agree with `DTPT` throughout.

## The map body — `RGM6`, `RGM9` and `RGMa`

**Corpus**, in `data::rgs::map`. All 36 shipped quests, three versions, read to
their last byte (`every_shipped_quest_reads_to_its_last_byte`).

A `.q` holds the recipe: map size, allowed themed landscapes, faction starts,
what lairs release. No tile grid, no heightfield.

```
[16-byte framing]
cstr    quest          `DAY_OF_RECKONING`, the id `.qdd` also uses
char[4] code           `Dayx`
char[4] pattern
u32 u32                two words: "word 1" is the map number, the other not understood
u32 u32                the map's size
u32 n  ( u32 key  spawner ) x n
u32 n  ( u32 key  slot ) x n
u32 u32 u32            three words, one a quest-type code
u32 n  ( region ) x n
u32 n  ( zone ) x n
unit pattern set x 2
char[4]                `Rel@`, later versions only
```

Sequential, no offsets, some fields version-gated; one wrong field loses the
rest, so ending exactly on 36 files is the evidence.
`every_shipped_quest_reads_to_its_last_byte`.

Sizes: 128 (17 quests), 256 (19) (`every_quest_is_a_square_map_of_a_offered_size`).
None at the editor's 512, though saves show the engine handles it. 512 needs
the expansion: the freestyle dialog puts the size letter in a scenario's name,
the base list holding S and L but no H, the expansion's all three. Every 512 save in the corpus is expansion.

### A unit pattern's fields are placed by the engine's reader

**Binary.** A unit pattern set is a count, then per record a `u32` key and an
`RGSLayoutPattern`, in file order:

```
u32     faction      the map key
char[4] code
cstr    name
u32     alternate    a byte: word == 1
u32     words[0]     the kind, kept if <= 10
char[4] ground
u32     words[1]     the placement grid step
u32 n  placement x n char[4] id, u32 word, cstr, u32 n, n bytes
u32 n  lair x n      keyed 1000 * placement + unit
u32     sizes        from RGC5, else 3
u32 x 3              from RGC6
u32 x 4              bytes: word == 1, the player-count flags
u32     flag         from RGC8
```

The gates compare a version the map tag selects (`RGC8` for `RGM9` and
`RGMa`, `RGC7` below). The field the block phase stamps is the one after
`words[0]`; `rgs::map` calls it `ground` (**Binary**).

`flag` keeps a pattern out of Random: the custom game's Random settlement and
Enemy Forces picks skip it, as the difficulty-band filter does. A quest never
runs the freestyle setup, so its own patterns' flag (Urban Renewal's) changes
nothing in its map.

`sizes` is the scenario's map size: bit `0x02` gives 256, then `0x04` 512, else
128; the first set bit wins (**Binary**). **Corpus**: 53 of 55 sized Enemy Forces entries
agree with their name's letter. `niuR` "Ruinous Domain (Expert) - S" and `Gobg`
"Goblin Outpost (Beginner) - S" carry 3, so the engine makes them 256.
**Game**: Goblin Outpost's map is not small. Ruinous Domain's one-player flag
is clear, so the dialog does not offer it (not in
`local/game-text/enemy_forces.txt`); multiplayer only.
`UnitPattern::side`, `a_scenarios_map_size_is_the_first_size_bit_it_carries`.

### A zone is the join to the chart

Same `code, code, cstr` header as a chart record:

```
"Day " "Day " "Day #1\0"    code, code, name
"xDay"                      a landscape pattern: what to scatter
"#Arr"                      a surface pattern:   what ground to lay
"Bump"                      a height profile:    what shape to give it
u32 u32
```

`every_zone_names_chart_patterns_that_exist`: every reference lands (over 900);
the only unresolved id is `NONE`.

`day_reckoning.q`:

| Zone | Landscape | Surface | Height |
|---|---|---|---|
| Day #1 | `#Day_Arid` | `#Arrid_Mixed_Surface` | Bumpy Flatlands |
| Day #2 | `#Day_Plains` | `#Plains_All_Parent` | Rolling Flats |
| Day #3 | `#Day_Mountains` | `#Scorched_Mostly_Parent` | Mountainous |
| Day #4 | `#Day_Swamp` | `#Swamp_Mostly_Child` | Flat |

A zone's parts share a biome, with two exceptions:

- `#All_Dirt` goes with everything; Dirt is every biome's base.
- Eight zones scatter a `#DEMO_` pattern (the ones naming `CLER`) over an
  unrelated surface (`a_zones_landscape_and_surface_agree_on_a_biome`).

A zone is a map region; a quest uses all its zones. **Game**: Day of Reckoning
has arid, plains, mountains and swamp on one map. Zones never touch: Dirt
separates them, and the tileset is cut for that. Each playthrough regenerates
the map, so zones move. See [terrain.md](terrain.md).

The freestyle quests have the most zones; story quests two to nine.

### A height profile is the fractal's settings, and the binary names them

**Binary.** `RGSFractalSettings` has a debug dump naming its fields: id, hurst
exponent, scale. The three 16.16 words, read by `chart::Profile`:

| Word | What | Range over the 14 profiles |
|---|---|---|
| 0 | hurst exponent | 0.15 to 0.90 |
| 1 | per-step falloff | 0.73 to 0.95 |
| 2 | scale, an amplitude | 2.0 to 8.0 |

Several expansion profiles share one scale, differing only in hurst and named
for it: `%flat` 0.90, `%rough_hilly` 0.15. Higher exponent, flatter ground.

Word 1 is within 0.4% of `2^(-hurst/2)` on all fourteen (midpoint displacement
falloff in half steps). Close, not exact, so **Inferred**: authored rather than
computed. `a_profiles_falloff_follows_from_its_hurst`.

Scale is the only chart number in the units of a save's heights:

| Land | Tallest zone scale | Highest vertex over its saves |
|---|---|---|
| K'Hal-Kratis | 2.5 | 2.01 |
| East Cheswick | 5.0 | 8.69 |
| Ravenswood | 7.0 | 10.91 |
| Valmorgen | 7.0 | 8.67 |

Every save of the land whose zones are all 2.5 stays under 4; every save of a
land with a taller zone exceeds 4. A scale, not a ceiling: zones blend, and
how they add is unread. **Measured**.

A region's debug dump agrees: id, name, fractal, landscape, terrain.

### A quest is generated from its own `.q`, on the clock

**Binary**: a quest start runs the chart generator on the world its `.q` made;
the freestyle setup never runs.

| `.q` field | Role |
|---|---|
| header `code` | the land: the region the region chart grows |
| header `pattern` | the template: that set 1 entry, alone |
| header word 1 | the map number: 0 in all 36, so a clock seed |
| size | the side |
| spawners | the unit chart's lairs, by key |
| slots | each live one a player, its gold `APP:0`; slot 7 always |
| trailer words 0, 1 | the victory index and its goal |

- No off-map template, so no off-map lair. **Corpus**: no Day of Reckoning save
  holds its `Anim` den.
- Every play is a new map: three Day of Reckoning starts, three maps. A
  quest save keeps no seed; a search found it (the tool is retired): force
  chart, then unit chart and region seeds, then heights. **Corpus**: the
  two Day of Reckoning seeds are clock words two minutes apart.
- A clock word is `t | (t & 0x3f) << 21`, so each low bit set is set again at
  bit 21 up (every found word holds it). The search ranked
  the force chart's fits by their grown region chart against the save's cells,
  past 200,000 fits only the clock words, then built the
  best's whole maps (**Measured**).
  - **Corpus**, Urban Renewal's day-0 save: many words fit its force chart,
    and thousands beat `0x0be833de` on units and seeds (the place search moves
    a few crowded buildings 5 to 8 cells), but its regions hold most cells, well
    ahead of the next clock word (`0xb84ab3de`, not a clock word, makes the
    same map), and its map most heights, the rest levelled ground.
  - **Corpus**, The Barren Waste's day-0 save: millions of words fit its
    one-letter force chart, all but one unit and seed; among the clock words
    `0x0725f9f9`'s regions hold most cells, well ahead of the next, and its map
    nearly every height, the rest levelled ground.
- **Binary**: map number 0 seeds from the clock, not from the session seed, so
  `--seed` does not fix a quest's map. **Game**: the Barren Waste played twice
  with the same `--seed` gives two maps (nearly every cell differs at day 6). Base and expansion quests take the same path; every HD `.q` has map
  number 0 (`every_hd_quests_map_number_is_0`).
- Paint stays on the clock.
- Its script is its `.mqxml`'s `<Name>`, run at setup; scripts do the rest
  (music, victory). Only `Freestyle` reads the victory index and the
  special events.

`sim::mapgen::quest`, `regent_core/tests/quest_start.rs`.

### `Freestyle.q` is the custom-game dialog

The dialog's lists are unit patterns in the freestyle quest, keyed by the codes
a save writes into its setup block.
All three codes of every freestyle save are checked (**Measured**):

| Setup slot | Dialog list | Where |
|---|---|---|
| 0, settlement | Player Forces | a set 1 unit pattern |
| 1, scenario | Enemy Forces | a set 1 unit pattern |
| 3, land | Landscape | a region |

- Set 1 is the dialog: 81 records base, 137 expansion (same test), each with
  the exact dialog string, size letter included.
- Set 0 is the blocks those entries place: "Goblin Camp 1", "Animal Dens (3)",
  "Ruined Shrine". Set-1 placements name set-0 patterns.

One code can mean different things per set: `Ruib` is "Ruined Shrine" in set 0,
"Ruins of Esterhausen (Advanced) - S" in set 1.

**Binary**: a set-1 record's `words[0]` picks its list:

- 1 to 4: Player Forces, and the value is the player count ("Frontier
  Settlement" 1, "Frontier Towns" 4, "Fortified Palace" 1, "Fortified Palaces"
  2). Single-player offers only the 1s.
- 9: Enemy Forces, every entry with a size letter among them.
- 10: Wandering Monsters.

Lands are regions, chosen by another field; the dialog offers those with
`active` and `expanding` set: 15 of HD's regions.
`a_player_forces_entry_says_how_many_players_it_is_for`,
`the_monsters_pick_is_a_third_force_that_places_no_block`.

Single-player, an Enemy Forces or Wandering Monsters entry is offered only
with trailer flag 3 set (`rgs::map::ONE_PLAYER_FLAG`; **Game** for the sized
Enemy Forces, `local/game-text/enemy_forces.txt`). From HD's
`mx_Freestyle.q`, `freestyle::offers` lists 15 lands, 18 settlements, 25
scenarios and 15 Wandering Monsters
(`the_dialog_offers_each_list_with_its_names`).

`regent_cli quest --units --placements` prints all of this.

### A placement names a block of a five by five grid

`Placement::cells` is a string of letters `A`–`Y`. The map is a five by five
grid of blocks (`RGSForceChart`'s fixed `5, 5`, whatever the map size); a
letter names one in reading order:

```text
   A  B  C  D  E
   F  G  H  I  J
   K  L  M  N  O
   P  Q  R  S  T
   U  V  W  X  Y
```

Several letters: the generator picks one.

**Binary**: the grid is turned. A letter gives `((c - 'A') % 5, (c - 'A') / 5)`,
then one of the square's eight symmetries, then the block size scales it. The
symmetry is `rng_next(8)`, the first draw after the map number seeds the
generator, so the turn depends on the map number alone: map 1 is orientation 7,
map 4 identity.

**Corpus**, checked on the ground:

- Wild Expanse places `Icex` "Ice Cave" (zone `$Dtar_Snow`, surface `%snow`) at
  `M`. Such saves, on lands that cannot paint snow, each have a square of
  snow over cells 204–306 of 512 on both axes: block 2 of 5 is
  `2 × (512/5) = 204` to `306`, exact
  (`a_placement_names_blocks_of_a_five_by_five_grid`); centroid mid-map. `M` is fixed
  under every turn.
- Turn: Frontier Settlement places `Fron` at `A` and lands bottom left; Winter
  Behemoths places `Icex` at `X`, one of `EVY`, one of `JPT`, and on Tholaria
  snow fills exactly three blocks. Scoring all eight turns puts 7 first by a
  clear margin, as `rng_next(8)` gives at map 1. At map 4 (orientation 0) the
  same Frontier Settlement puts its plains top left, more of `A` plains at 512
  cells than at 128, where the map 1 save has that corner empty.

**Measured**.

So a set 0 pattern's `ground` is its block's ground, and placing a block with
`alternate` 1 stamps it (**Binary**, `UnitPattern::stamps`, see
[the region chart](#the-region-chart-grows-blobs-on-a-circle)). In the
freestyle quests only `Fron` and, in the expansion's, `Icex` carry it;
neighbouring `Quea` and `Dara` stamp nothing.
`two_freestyle_blocks_stamp_their_ground`.

#### A bare number is a map edge

Most of `mx_freestyle.q`'s placements are letters; the rest are a bare byte 1,
3, 5 or 7, alone, with `word` 5, all in Wandering Monsters records (130 over
both freestyle quests). Over all 36 quests: 170, all 1, 3, 5 or 7 but two 0s in
`clash_empires.q` (`a_bare_number_is_the_edge_wandering_monsters_enter_by`).

**Binary.** The force chart skips a placement whose first cell is not `A`–`Y`.
Values with `c - 1 < 8` (so 0 places nothing) make the block's buildings off
the map, at `x = -1000 - (c - 1) / 2`, `y = -1000` thirty-seconds of a cell,
with no pad and no flattening. `SpawnUnit` spawns from a lair at such an `x` on
a map edge:

| Cell | Lair `x` | Spawn point | Edge |
|---|---|---|---|
| 1 | -1000 | `(rng(w - 1), 0)` | north |
| 3 | -1001 | `(w - 1, rng(h - 1))` | east |
| 5 | -1002 | `(rng(w - 1), h - 1)` | south |
| 7 | -1003 | `(0, rng(h - 1))` | west |

Clockwise from north. An even cell would leave `x` unset; none ships. A lair's
settings are keyed `1000 * placement + unit` (`Harb`'s run 0, 1, 2, 1000, …).
`rgs::map::Edge`, `a_bare_number_is_the_edge_wandering_monsters_enter_by`.
**Game**: Wandering Monsters travel in from the map's edges; which edge each
lair uses is not checked.

### A scenario's trailer says who may play it

A sized Enemy Forces record's trailer is three difficulty numbers then four
flags, one per player count 1–4.

`mx_freestyle.q` has 55 sized Enemy Forces records; the dialog shows 25
(`the_dialog_offers_a_scenario_when_its_trailer_says_one_player`). Every
shown one has `trailer[3] == 1` (the one-player flag), every hidden one `0`.
`local/game-text/enemy_forces.txt` is the dialog's list, copied
from the game; its one mismatch is a game typo, "Shadow's Guardian" for
"Shadow's Guardians".
`the_dialog_offers_a_scenario_when_its_trailer_says_one_player`.

No record has all four flags clear, so the 30 hidden ones are reachable in
multiplayer. Eight of sixteen masks occur
(`the_trailers_last_four_words_are_a_player_count_mask`):

| Mask | Records | Mask | Records |
|---|---|---|---|
| `1 0 0 0` | 15 | `0 1 0 0` | 9 |
| `1 1 0 0` | 7 | `0 0 0 1` | 9 |
| `1 1 1 1` | 2 | `0 0 1 0` | 7 |
| `1 0 1 0` | 1 | `0 1 1 1` | 5 |

Not map size; that only correlates (bigger maps, more players).

### Which slot owns a freestyle placement

**Binary.** The player count is 1 alone, the session's player count in a
network game. The dialog offers Player Forces entries of that kind, and
scenarios and monsters with that count's trailer flag.

The freestyle setup clears the quest's force lists and builds eight player
entries of its own:

| Slot | Alone | Network game of n |
|---|---|---|
| 0 | "Human Player", live | session player 0, live |
| 1 to n - 1 | "Dummy Player" | session players, live, in the session list's order |
| n to 6 | "Dummy Player" | "Dummy Player" |
| 7 | "Monster Player", not live, gold 0 | the same |

Only a live entry gets a `Player`. Each template placement then goes to a list
by the set 0 block its code names:

```text
for p in template (settlement, scenario, monsters):
    if units[0][p.id].words[0] == 1: list[next] += p
        next += 1 in a network game only
    else: list[7] += p
```

So a network game's n-th player block is slot n; alone, every one is slot 0.
**Corpus**, both HD freestyle quests: a Player Forces entry for n players
holds exactly n placements, all player blocks; no Enemy Forces or Wandering
Monsters entry holds one. So in a network game of n the settlement's
placements are slots 0 to n - 1, in file order; everything else is slot 7.
Alone, the result equals the quest's own slot lists (0 under slot 0, 1 and 2
under slot 7) on every one-player pair, so the single-player start is
unchanged. `freestyle::seats`, `Play::humans`,
`a_freestyle_placements_slot_counts_player_blocks`; the start's owners
(`start::stand`, `Recipe::Freestyle`), `Seat::freestyle`. The lists for n
players: `freestyle::offers`, `randoms` (`Harb`: three and four players).

The lairs merge keeps the template's indices, so its "entry 0 is the
settlement" holds for keys only; owners follow the lists above.

Not settled: which session player is the local machine, and the session list's
order (**Inferred**: join order).

### The scenario hands the generator a position in the RNG stream

The Enemy Forces choice moves the heightfield (see
[saves.md](saves.md#the-enemy-forces-choice-reaches-the-generator)) without
naming any class, pattern or profile. It shifts *when* the generator draws.

The generator seeds the RNG with the map number, then builds the force chart
and unit chart before the region chart. The region chart starts by shuffling a
`w/10` by `h/10` lattice, walking it and planting each zone's seeds on free
points. A finished map tells which zone owns lattice point `i`, so the only
unknown between map number and shuffle is what the two charts drew. Scoring
every offset below 2²⁰ gives one winner per scenario:

| Draws | Scenarios |
|---|---|
| 1 | `Ench` Enchanted Forest, `Skea` Skeletal Hordes, `dliW` Wild Expanse |
| 2 | `Cona` Conclave of Shamans, `Gorg` Gorgon Realm |
| 3 | `Ruib` Ruins of Esterhausen, `Ratx` Rat Scouts, `Gobf` Goblin Stronghold, `Wint` Winter Behemoths |
| 7 | `evaR` Ravenous |
| 8 | `Shad` Shadow's Guardians |
| 13 | `Gobe` Goblin Caves |

The offset depends on the scenario only:

- `Cona`, `Skea`, `dliW` and `Ruib`, each on several lands: zones, seed
  counts and ground differ; the offset does not.
- Not map size: `Ench` at 128 and `dliW` at 512 both spend 1; `Ruib` and `Gobe`
  differ at 128 and 256.
- Not the settlement: a Tholaria save with Frontier Settlement instead of
  Wizard Hold also spends 3.
- **Corpus**, no test: Rat Scouts and Ruins of Esterhausen (3 each) give the
  same ground; Enchanted Forest (1) does not.

`sim::mapgen::force` and `sim::mapgen::unit` on a save's setup reproduce the
offset of every Windows freestyle save —
**Measured**. Cost:

```text
1                        -- the grid's turn, rng_next(8)
+ one per template placement that still had a free block
+ one per placement of each block it placed that still had a free cell
```

Both charts place alike: collect the placement's letters whose cell is free;
two or more cost one `rng_next`, one costs nothing. Grids:

- Force chart: the five by five.
- Unit chart: inside each placed block, its set 0 record's lettering at a step
  of the record's `words[1]` (the `.q` word after the tag, not `resolution`,
  which reads 3 on every freestyle set-0 record), centred.

So `Gobe`'s 13 = 1 turn + four multi-letter placements of its own + eight from
its two `Gobf` blocks.

**Binary**: the template is the settlement's placements then the scenario's.
A settlement shifts the offset only where one of its blocks has a choice;
neither corpus settlement does. `Shaa` "Shanty Town" places `Shan`, whose four
placements each name two cells, so it is predicted to spend four more —
`the_settlement_moves_the_offset_where_its_block_has_a_choice`, unconfirmed by
a save.

Not the trailer: 20 pairs `Ruib` and `Ratx`, but `Gobf` spends 3 at 95; 25 for
`Ench` and `Skea` fits, 30 for `Gobe` and 45 for `dliW` do not.

### The RNG's orbits

The mask drops bit 31, so the transition is not a permutation: a seed's
stream is a tail into a cycle, a rho (`base::rng`). Brent's algorithm over
the corpus's seeds (**Measured**):

| Seed | Tail | Cycle |
|---|---|---|
| 1 | 56,611 | 49,271 |
| 2 | 44,234 | 15,847 |
| 3 | 27,057 | 15,847 |
| 4 | 18,790 | 49,271 |
| 5 | 5,265 | 49,271 |
| 6 | 53,878 | 49,271 |

Two cycles appear, so the cycle belongs to the seed. Past the tail a state
gives a draw count only modulo the cycle: the heightfield offsets measured
on seed 1, 100,182 at 128 cells and 66,906 at 512, are states, each the
smallest count reaching it.

### The region chart grows blobs on a circle

**Binary**, rebuilt in `sim::mapgen::region`. Decides which zone owns each
cell; heightfield, classes and scatter are painted through it.

```text
fill every cell NONE
seed placement           -- the w/10 lattice shuffle, a plus of five per seed
                         -- then the 5 by 5 block phase, which draws nothing
shuffled_point_list(w, h)   -- w * h draws, the order a stuck region rehomes in
while some cell is still NONE:
    if no region record is left:
        give every NONE cell its first non-NONE neighbour, and repeat
    code = weighted pick                     -- one draw
    if no record carries that code: drop code from the pick
    else:
        record = candidates[rng_next(len)]   -- one draw, even at one candidate
        grow record by one cell
        if it had nowhere left to go: rehome it, or erase it
```

Two draws per iteration; nothing else is random.

A blob grows on a ring, not along its border. A region record holds its seed
centre and a pass count. Its frontier is the circle of that radius: the point
at angle 0, then `7 * (r + 1)` evenly spaced samples of the engine's 256-entry
sine and cosine tables scaled by `r`, dropping repeats and off-map points. The
ring has gaps (some steps move two cells), so a blob can leak through it.

The record walks the ring once, claiming what it may, then the radius grows by
one. The region is spent when the radius passes a third of the map width.

The claim rule tightens with age. It thresholds a count of a consecutive run of
neighbours, not a total:

| Pass count | A cell needs |
|---|---|
| 0 or 1 | one neighbour with the code, and at least one without |
| 2 and up | two adjacent neighbours with it |
| 2 and up, on the first or last row or column | one |

"At least one without" is a side effect: the run starts at 1 and is tested only
after a step, so a cell whose eight neighbours all carry the code fails a
threshold of one.

If the region's own code may not claim the cell, the grower walks the whole
zone table without early exit, so the last zone that may claim it wins —
possibly not the one drawn.

A placed block seeds its own region. The seed placement's second phase walks
the five by five grid; for each block whose unit pattern has `alternate` set it:

- writes the pattern's `ground` over every free cell of the block,
- adds that code to the zone table at weight 20 unless the land names it,
- plants a record at the block centre, pass count starting at half a block,
  erased, not rehomed, when spent.

That matches [the reader](#a-unit-patterns-fields-are-placed-by-the-engines-reader)
and [the stamped blocks](#a-placement-names-a-block-of-a-five-by-five-grid).

#### How far it is checked

By the heightfield, the only instrument that sees the carve: it depends on the
stream position and on zone ownership, so an exact field on a land with several
profiles (Valmorgen, Tholaria) tests the carve. All 60 Windows
freestyle saves come out at the model's draw count, the carve off by at most
six vertices, and no offset within 3,000 either side does better:
**Measured**.

Masks cannot check it. A save records classes per vertex, not zones, so a class
mask attributes a cell only where its class belongs to one zone; and a per-cell
chart against per-vertex classes is the wrong grid, shifted differently per
land. A heights mask fails too: the
[slope clamp](saves.md#maxslope-is-a-ground-clamp-not-a-lighting-value) floods
the whole map after the passes. Nor can zone cells be recovered from the
classes past the first zone: one wrong cell shifts every later zone's draws
(K'Hal nearly right at 128 cells, the bog zone far off at 512); the chart is
generated instead.

### The ground is two picks deep, and the terrain chart is the first

**Binary**, rebuilt in `sim::mapgen::terrain`. Zone → surface pattern →
terrain patterns → `DTPT` classes. Each chart is named for the layer it
*reads*.

The terrain chart, the first hop:

```text
shuffled_point_list(w, h)              -- w * h draws
for each point in that order:
    zone = region_chart.at(point)
    pattern = the zone's surface pattern
    zone or pattern unknown  -> leave the cell alone, no draw
    pattern with no entries  -> write NONE, no draw
    otherwise                -> one weighted pick, write what it picked
```

A cell holds a terrain pattern code. The surface chart turns them into classes
in three passes, all in `sim::mapgen::surface`:

1. No draws: each cell takes its pattern's entry 0.
2. Shuffle a `w/3` by `h/3` grid; draw `rng_next(3)` twice for a cell in each
   block; where that cell's pattern has two entries and entry 1 is not `NONE`,
   one weighted pick. Landing on entry 1 blotches: that class is written over
   the cell and up to eight neighbours (so a one-entry cell next to a two-entry
   one can change). The neighbour count starts at `k`, from a chart entry's
   first unread word:

   ```text
   k = entry0.rest[0] / 5        -- 5 is preamble record 2's second bound, 20, over 4
   n = rng_next(8 - k) + k       -- then n times rng_next(8), skipping the taken
   ```

   A cell is written only where all eight neighbours already hold the majority
   or minority class, keeping speckle off biome boundaries.
3. No draws: flip any cell whose class is on none of its four orthogonal
   neighbours to the other class of its pair, walking `x` then `y`, seeing its
   own writes. Lone speckles vanish; pairs survive.

#### What it scores

A save records a class per cell, so no mask is needed. Over the uniform cells
(a blend cell has two classes) of every Windows freestyle save, charts only, no
paint, the three passes get most cells right: the base pass alone about as
many, saves that stamp no block more, East Cheswick at 512 best, Ravenswood
(every terrain pattern two entries) worst.

With the engine's paint added, the finished layer reads 99.25% of uniform
cells against a bar of 99.40 (`Tier::bar`) —
**Measured**, the class row of
[the generator's fidelity](#the-generators-fidelity).

The blotch pass helps where a land has a minority class (Ravenswood,
K'Hal-Kratis) and changes nothing where no pattern has two entries. Side by side, model and save agree on every blob; they differ in
one-to-three-cell speckle within the *same* ladder, never on biome.

The game does not reproduce its own paint: two fresh games on one seed have
bit-identical heights and differ on a few per cent of cell classes (see
[saves.md](saves.md#what-a-seed-fixes-and-what-it-does-not)). Twins agree on
98.79% of uniform cells (`Tier::Ceiling`, **Measured**); a base-only model's
bar is `(100 + c) / 2` = 99.40 (`Tier::bar`). No save is raw generator output: the engine
places the settlement before writing, so every save has flattened ground,
relit vertices and cleared scatter.

That difference is the blotch pass; the zone layout is reproducible. On two
lands whose terrain patterns have one entry each (no minority class), runs over
two setups agree on all but a fraction of a per cent of uniform cells,
heightfields bit-identical across a restart. **Corpus**, no test.

Many of the cells the three passes get wrong are Dirt in the save — laid by the
engine's paint, below.

This also checks everything upstream: the terrain chart's shuffle continues the
region chart's RNG, so a wrong draw count there would scramble every pick.

### The landscape chart

**Binary**, rebuilt in `sim::mapgen::landscape`. Shaped like the terrain chart,
with two differences, both affecting the stream: it stores the picked entry's
index, not its code, and an empty table still draws. A cell whose zone or
pattern does not resolve spends nothing and keeps the last good pick.

#### A save pins the index, by inversion

No save field is this chart, and it costs one draw per cell whatever it picks,
so draw counts cannot see it. Pinned backwards: a `GRND` record names an object
code, a landscape entry names an object pattern, an object pattern names the
codes it may place. Where only one entry of a zone's landscape pattern can make
a code the save holds, the index at that clump's seed is fixed; where that
clump holds one object, the seed is the object's cell (the clump loop does not
run for size 0 or 1).

16,229 pinned cells over 60 Windows freestyle saves, all right, against 1,992
expected by chance (`PINS`, `BY_CHANCE`). The pin uses only the save's `GRND` codes and the chart's
entry lists. **Measured**.

**Binary**: the entry list is the file's, in file order: no filter, no sort.

Do not score the pin over bigger clumps: it then only says some cell of the 9
by 9 ring holds the index, true 52,024 times against 51,844 by chance (`RING`,
same test).

#### What the mobject chart costs

K'Hal-Kratis at 128 cells: the seven charts spend 149,453 draws, the mobject
chart's `w * h` shuffle of 16,384 among them; the model spends the same
(**Measured**) and reaches every save's
field (**Measured**). K'Hal-Kratis converges on an
inexact region map because its two profiles barely differ, so it cannot judge
one — **Measured**. Ravenswood (`Moun`
beside `Subt`) matches only on an exact region map.

### The unit chart names the buildings a save stands up, and the start places them

**Binary**, held on the corpus by `regent_core::sim::start`. The second chart's
cells are the start's units, kind for kind
(**Measured**). They are made after
the scatter, on the charts' stream:

```text
off-map lairs               one draw each (the unit's slot)
scatter                     one slot draw each
unit chart
  shuffled_point_list(w, h)          w * h draws: the creation order
  per cell not NONE or FOOT:
    at = cell << 5                   32nds
    character or general: jitter     two draws
    create                           owner from the block, below
    slot                             one draw
    place                            the search, margin 8
```

The descriptor's kind picks the record class:

| Kind | Made as | Search | Blocked by |
|---|---|---|---|
| building, and anything else | `BuildingRec` | its clearing ring | units made before it |
| 3, character | `VehicleRec` | its own pad | units and scatter |
| 0, general (`BGL2`, `BGD1`, `PG03`) | `TerrainFeatureRec`, as scatter | none | — |

- A building's search runs from its cell to the nearest half cell where its
  ring (`sim::clearing`, margin `ConstructData1`) is clear: a flood of the
  half-cell grid that spends no draw. Scatter and terrain features do not
  block it. A ring cell off the map blocks; a failed search leaves it unmoved
  and unlevelled (not in the corpus).
- So what moves a building is the order: the shuffle runs on the stream the
  scatter leaves, so two lands with one chart place differently
  (**Measured**).
- A terrain feature stays at its jittered point; it blocks mobiles, and a later
  ring clears it. **Corpus**: most are `GRND` records exactly there, the
  rest gone.
- Owner: the cell's second word is its force block's owner, the player slot
  listing the block's template placement index, else 7; `AVk1`-`AVk3` get -1.
  A freestyle game's lists are the freestyle setup's
  ([which slot owns a freestyle placement](#which-slot-owns-a-freestyle-placement));
  alone, they match the quest's slot items. **Inferred**: a quest's lists
  are its slot items.
- Handles and agents follow creation order: off-map lairs, scatter, then the
  shuffle.
- The one draw is the unit's slot, over the slots of the set it is made in
  (`Unit::initial`: its first `Inactive`, else its first). **Corpus**: all
  1,212 start buildings and terrain features found at their place hold it
  (**Measured**); every scatter record names that set
  (**Measured**).

**Corpus**: every start building of the Windows freestyle saves and of
played games' saves stands where the record holding its handle does, owner and
code its own, some upgraded, a few razed with no record left (a defeat game's
`ABM1`, a Magic Bazaar), two ruins their collapse moved
(**Measured**). Both are the united winter
games' `ABP1`: standing, its `LocationOf` is the model's (256, 3168); its ruin
is at (304, 3168). **Binary**: a collapse runs the same place search a dying
hero's peak runs, so a unit on its pad moves the ruin
([vm.md](../gpl/vm.md#a-building-falls)). Fresh games on one setup agree record
for record (**Measured**). Off-map lairs come first
(**Measured**); the united winter game's six Minotaur lairs
are its six `OFMP` records, code, place, owner and handles 1 to 6
(**Measured**).

### The mobject chart scatters in clumps, and a save holds its output

**Binary**, rebuilt in `sim::mapgen::mobject`. The seventh chart, the only one
a save keeps: after the fractal the game walks its grid and writes one `GRND`
record per cell holding a code. The chart shuffles the grid and places a clump
per point until the placed count reaches the object limit less 200 — 550 at
128 cells.

Six free gates: the cell is still `NONE`; the region code names a zone; the zone
names a landscape pattern; the landscape chart's index names an entry; that
entry's code is neither `NONE` nor `Empt`; the code names an object pattern.
Then:

```
kind  = weighted pick (object pattern)           -- one draw, CLER means none
if the footprint fits: stamp it, and grow a clump
while the clump holds fewer than the landscape entry's rest[0]:
    direction     = rng_next(8) + 1              -- one draw
    first, second = rng_next(4) + 1 each         -- two more
    the step is clamped onto the map
    if it lands in the same zone:
        one more draw for what goes there, and it is placed if it fits
    stop at 256 tries, or at the object limit
```

A clump try costs three or four draws.

Clump size comes from the landscape entry, not the object pattern. Data
agrees: landscape entries ask for 1–9, object-layer entries 0 throughout.

The two step draws are not `x` and `y`: each of the eight directions uses them
its own way. Read as `(dx, dy)`, a member lands one cell off.

A clump stamps one of two grids, per entry. A placed object marks its cell and
four cells each way `FOOT`; the first gate tests that mark, keeping clumps five
cells apart. The picked entry's `rest[2]` picks the grid (1 if unset):

* Nonzero: marks the clump's private 41 by 41 window about its first point,
  only where still `NONE`. The fit test reads the map, not the window; the
  window is copied to the map when the clump is done. So members can be
  adjacent: one save has many objects with a neighbour one cell away, and no
  two clumps closer than five.
* Zero: marks the chart's grid at once, unconditionally. The fit test then sees
  the halo, and a step reaches only four cells (the halo width), so such a
  clump rarely grows past one object and still spends its 256 tries. It can
  also write `FOOT` over an object already placed, removing it.

About half the object-layer entries have zero; the byte is per entry, not per
code (`%sno_g-tree_02` names `NGa1` twice). Not a "stands alone" flag.

Footprint: a rect per object code, each word divided by 32 and added to the
point; the fit test refuses unless every cell is free. The rect is the art's:
first four `i16` of the drawn slot, pixels about the anchor (see
[art.md](art.md#a-slots-block-0-is-a-footprint)), top and bottom first scaled
by `32 / (32 sin(IsoCameraAngle))` (a cell is 32 pixels across, 26 deep).
`NGa1 SnowPine`: one cell wide to the left, nothing else. `NGo1`, art floating
above its anchor: four cells *below* the point.

Both conversions truncate toward zero. `PG07` is the only code of the fourteen
K'Hal-Kratis scatters (`FOOTPRINTS`,
`the_footprint_reader_agrees_with_the_rects_that_were_measured`) where rounding differs (26 pixels deep is 31.74 cells'
worth: truncates to 0, rounds to 1); that one cell was the chart's whole
residual.

An empty rect falls back to the union of the frame boxes of sets 8, `0xd0` and
`0x40`. `WG12` is that land's one code with an empty rect; its frame union
`[-19, -2, 16, 21]` divides to the same single cell. The other thirteen codes'
unions match their slot rects. Art drawing none of the three sets gives a zero
rect: a unit's pad is then a point at its half cell
(`placement::pixel_rect`; the `fire_ball` spell unit, `XL02`, only `Die`).

A code with no `IMAG` would read an uninitialised rect. Of the object layer's
79 codes only `CLER` has none, and it is refused first —
`every_code_the_scatter_can_pick_has_a_footprint_to_read`.

The fit test:

- Tests occupancy only: no class, slope, zone, distance or edge rule beyond four
  bounds.
- Bounds first: `top < 0`, `left < 0`, `bottom >= height`, `right >= width`
  refuse; `left > right` accepts. Then both ranges inclusive, x outer.
- Bounds are the map grid at cell resolution: not half cells, not offset by
  the footprint origin, not the clump window.

The cap stops the chart, not the shuffle. On the measured 128-cell save it walks
about a third of the 16,384 points and stops at `placed >= 550` (the limit of
750 less 200). The map fills from the shuffle's front; the draws the
heightfield measures are exactly those up to the 550th stamp. 550 stamps at 149,453 draws
is the chart's sharpest check; the model hits both.

A stamp is not an object: nothing stops a later step landing on an earlier one
(one clump stamps `(1, 53)` three times). The `GRND` pass reads the finished
grid, where the last write stands.

The chart is exact: all 501 of the save's objects in the same cell with the same
descriptor, at the measured draw count —
**Measured**. The heightfield after it is
off by 230 of 16,641 vertices (construction flattening only), from the map
number alone — **Measured**.

The objects the save lacks are the clearance pass: every chart unit, and
every building the player places, deletes the scatter under a selection-ring
ellipse sized to its pad grown by 8 half cells (`regent_core::sim::clearing`;
[vm.md](../gpl/vm.md#placing-a-building)). Over the saves whose field
the model reproduces, the rings of the units standing in each save cover nearly
every object the chart placed and the save lacks; most saves agree exactly. The
few objects lost or kept otherwise all lie within 8.2 cells of a chart monster,
which has walked since it cleared —
**Measured**. A fresh world keeps
the scatter the library's world kept as it stood the start up
(`sim::mapgen::fresh::stood`).

Do not override a region cell to close a miss: it makes the model worse.

### The Dirt between two biomes is a fallback, not a band

**Binary.** The classes are five ladders sharing Dirt, with no blend between
biomes. The engine separates them per pair, not per seam: a pair with no blend
takes the lowest class drawable against both, `GR00`, and that cascades
outward — eight ways from a uniform cell, two from an edge, three from a
corner, none from the four diagonals. `sim::mapgen::paint` lays it;
`regent_core::sim::paint` holds it.

The chart is copied into the map cell by cell, x outer, y inner; the surface
chart's class becomes a letter via `DTRN GRIT`'s `TPRT` at run time and is
painted with a full corner mask, so the cell ends uniform. Walk order decides
which blends survive. Building pads use the same paint.

Cells do not start as Dirt: the map is first painted from the heightfield. The
charts lay Dirt through a surface pattern reaching a `#All` terrain pattern —
most of a save's Dirt over the corpus. The rest is on lands whose zones name
no Dirt (Ravenswood, Tholaria, Valmorgen): charts lay none, saves have some.

A fitted band (dilating and mending the seam) does not match. The tileset draws
only `Dirt`-to-parent and parent-to-child edges (see
[terrain.md](terrain.md#the-classes-form-a-tree-and-the-art-agrees)) —
`rgs::blendable`, held against both tilesets by
`every_blend_joins_a_terrain_class_to_its_tree_neighbour`.

### A building paints bare ground over its own polygon

**Binary**, gated on `Info` bit 8 (95 descriptors, base and expansion:
`every_building_that_lays_ground_has_a_polygon_to_lay_it_from`). Held by
`regent_core::sim::placement`.

- Shape: point list 0 of slot 0 of set 8, or of set `0xd0` where set 8 has
  none, not the footprint rect. Layer 0 only: the whole set id must match.
- Unprojected by `sin(IsoCameraAngle)`; floored and ceiled at 16 ground pixels
  into half cells; rasterised by testing each half cell's four corners.
- Same Dirt cascade; `GR00` is resolved in the live `TPRT`, so a mod tileset
  without `GR00` gets no paint.
- At least 97.9% of predicted cells are wholly Dirt in a day-zero save,
  against at most 12% overall (**Measured**).

One path for every building: chart, quest and scripted (`SpawnUnit`) buildings
all run it. The mask is cached in the descriptor, so a descriptor has one pad
whoever places it. An agent whose pad fails is destroyed, so every building a
save holds ran it. Only the off-map lairs of
[a bare-number placement](#a-bare-number-is-a-map-edge) skip it.

**Corpus**: all 29 well-sampled pad boxes match, the two scenario lairs
included (`a_pads_bounding_box_is_the_one_the_corpus_measures`). Per instance
the model's leftmost column is mostly Dirt for `BBB1 Dark_Castle1` and
`BBL1 Ruined_Keep1`, the column beyond it seldom.
**Measured**.

**Game**: runs at construction start; destroying the building never restores
the ground. **Binary**: at placement, after the clearing, paint per covered
half cell, row by row, cell `(x / 2, y / 2)`. The world does it
(`world::building_ground`); the inn save's 32 repainted cells come out
(**Measured**).

### The generator's fidelity

`sim::mapgen` against the 60 Windows freestyle saves, by layer.
**Measured**, on an older HD data version; no test holds it now.

The game's own bar comes from twin games on one setup. Where twins agree on
share `c`, a model with no re-roll can expect `(100 + c) / 2`.

| Layer | The game | The model | Over | Left |
|---|---|---|---|---|
| height | exact | 100% | 60 heightfields, each whole | none; 1, 2 and 6 vertices near buildings on three saves are the flatten's |
| shade | exact | 99.97% | vertices of saves whose heights match | 0.028%, under a pad's bottom edge: a rerun's stale buffer |
| class | 98.8% ceiling | 99.25%, bar 99.40 | 4.65M uniform cells | the paint on zone boundaries, mostly Dirt |
| case | 98.8% ceiling | follows class | | none: a save's cells rebuild from their field on 6,846,536 of 6,848,512 |
| variant, rotation | re-rolled | invented | | none possible: a clock-seeded RNG draws them |
| scatter placement | exact | 99.91% | 197,945 objects | 173 jittered records, off their cell |
| scatter slot | exact | 100% | 22,140 objects, five saves | none |
| start buildings | exact | 100% | 1,026 buildings, plus 19 traces' saves | none; a failing place search is untested |

A save is written after the settlement stands, so near buildings it is not
the generator's output: a building flattens, clears, paints bare and
relights its ground, and reveals what it sees (`sim::placement`,
`sim::clearing`, `sim::world::reveal`).

### What is not read

The four words after a spawner's settings. Third-party decompilation of
`RGSeditor.exe` calls them max hit points, base spawn rate (ms), dispersion
(pixels), and one the editor labels "not implemented", plus a fifth for
hit-rate reduction. **Inferred**: nothing in this corpus checks it.
