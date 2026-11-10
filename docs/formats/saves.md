# `.gmp` — save games

openregent's own saves are another format: see
[openregent/saves.md](../openregent/saves.md#saves).

> Part of [README.md](README.md).

Container, map, objects and script state are solved. The project holds no
reader.

A `.q` is a recipe; its ground is generated at quest start. A save is the only
written record of a generated map. No saves are in the repository, and no test
reads one, so claims over saves are **Measured** unless marked.

The corpus spans Day of Reckoning, freestyle games of every land, size and
scenario, quests at day 0, a hilly map, and single heroes saved over several
days. Two saves of one quest are two different maps.

**Measured**: 65 saves hold 208,818 objects (1,165 `BLDG`, 556 `VHCL`, 207,097
`GRND`, no `RWRD`) and 234,802 property entries. `SVGD` was read on 88 saves,
74 freestyle and 14 quest.

## Framing

Every chunk:

```text
0x00  char[4]  tag
0x04  u32      size — content only, excluding this 16-byte header
0x08  u32      version
0x0c  u32      count
0x10  ...      content: size bytes, nested chunks or an array
```

- A chunk occupies `16 + size`. Walks every corpus save to the last byte.
- `version` 0 = nested chunks, else payload. Holds on all 486,769 non-empty corpus
  chunks. Do not guess from content: a payload can open with four
  printable bytes and a plausible length.
- `count` = children or records. `MTER` 2 (`MVTX`, `MCEL`); `GRND` 501 → 501
  `UNIT`s; `MVTX`'s `PRIM` 16,641 = 129². **Measured**.
- Reading an array from byte 12 instead of 16 still walks the file, but rotates
  every field of a fixed-stride record by four.

Top level, same order in every save (18 chunks; more with
extra players, see [below](#what-a-second-kingdom-in-the-corpus-shows)):

```text
VERS SVGD MAP  PLYR PLYR MTER GPL  BLDG VHCL
GRND RWRD MSGF OFMP NMGM ORDR CONT GSGL ORLS
```

- `MTER`, `MAP `, `PLYR`, `BLDG`, `VHCL`, `GRND`, `RWRD` hold chunks;
  `MSGF` too where a message flag stands.
- `NMGM` is the 17 hero-name generators, each its seed and how many names
  it has given (136 bytes): the seed the game's GPL
  seed, the counts rising with play
  ([gpl/runtime.md](../gpl/runtime.md#every-rng-and-its-seed)). **Binary**: the
  chunk gives each generator two words, the seed, which reseeds its stream,
  then the count; a new game seeds all 17 with the session word.
- `OFMP` holds the off-map lairs, one record each: the united winter save's
  six Minotaur lairs (**Measured**); zero
  bytes in every save without one. A ranger away in distant lands is not in
  it: its `VHCL` record stands 250 32nds past the map's edge, unheld, activity
  15, its agent's type `Hidden` and `activescript` `Return_To_Map` every
  90,000 ms (**Measured**).

## `SVGD` — what the save is

Version 10 on every save. **Binary**, as the original reads it:

```text
cstr      save name, as typed (version 10 on)
cstr      quest name, in the game's words
u32       unread
u16[8]    Windows SYSTEMTIME, year to milliseconds
u32 × 2   unread
u32       the world RNG state (version 6 on)
u32       players, then each one's name (cstr): one on every save
u32       game kind: 0 or 2 freestyle, 1 a quest
char[4]   quest key (version 8 on) — QU06 for Day of Reckoning
u32       50 on every save
```

- The key and the next word's low byte read as text, `QU062` or `FRE12`.
  The original reads no quest id string, only the key word. A mod's quest's
  key is a CRC of its id, any four bytes ([mods.md](mods.md#quests)):
  `Metadata::key` reads it raw.
- Check: a misread `SYSTEMTIME` gives a year in the thousands; every save gives
  a real date. **Measured**.
- The key agrees with the quest name on every quest save.
- The word 50: a load restores it to the message manager, which a new game
  sets to 50. **Inferred**: the game speed, a frame's real ms.
- The world RNG state is the word after the timestamp's two unread words. A
  single-player load (mode 1) does not seed from it ([below](#what-a-load-restores)); a
  network game's (mode 2) does.
- Fields arrive by version: the save's name from 10, the world RNG from 6, the
  quest key from 8 (before it, a quest slot), the cheats word from 2, the eight
  flag words from 3, the dataset tag from 7, the mod list from 11. HD reads up to
  11.
- The first player name is the save's player. The display name is the quest's
  ("The Day of Reckoning", "Freestyle Game"). `SYSTEMTIME` is Windows', with
  no timezone; weekday 0 is Sunday.

### The setup, and the seed

After the quest key, word-aligned:

```text
u32       1
u32       2
u32       1     the cheats word: 1 allows cheats (cheats.md)
u32       mode — 6 freestyle, 1 quest
u32[13]   freestyle setup, mode 6 only
```

[cheats.md](../original/cheats.md) has what the cheats word allows.

Mode 6 on exactly the saves with key `FRE1`; 1 on the quest
saves; the meaning of 6 is unknown. The mode word is also the freestyle config's version
(below) (**Measured**; no setup block: a quest seeds its map from the clock and keeps no word,
[maps.md](maps.md#a-quest-is-generated-from-its-own-q-on-the-clock)). The words
are the registry's `RGS*` values plus the seed:

| Slot | Reads as |
|---|---|
| 0, 1, 3 | Settlement, scenario, land — `Wizb`, `Ruib`, `KxHa` — keyed into `mx_rgstext.cam` as `RGS1`, `RGS2`, `RGS4` |
| 2 | Wandering Monsters pick, `RGS3`; `NONE` on every one |
| 4, 5 | The picks' difficulty: `(3e + s + 40) / 6` of the scenario's and settlement's `trailer[0]`, then its band, 0 to 2 (**Measured**); no reader found |
| 6 | Starting gold, `RGS5` — 30,000 throughout |
| 7, 8 | 0, as `RGS6`, `RGS7`: the victory condition, `GetVictoryConditionIndex`'s answer, and its goal, `GetVictoryConditionModifier`'s: gold, or days ([gpl/vm.md](../gpl/vm.md#world-builtins), `freestyle::Victory`) |
| 9, 10 | Stale dialog state, read by nothing |
| 11 | Tileset, `RGS8` — `GRIT` (`terrtype.cam`: `GRIT Gritty`) |
| 12 | Map number = seed |

- The scenario gives the map size: its name ends in the size letter ("Ruins of
  Esterhausen (Advanced) - S"), matching the map on every freestyle save. No other word does.
  S 128, L 256, H 512.
  **Measured**. Freestyle picks size
  through the Enemy Forces list.
- Slot 2: the dialog's third force list. **Binary**: the freestyle dialog
  fills its three lists from the quest's set 1, bucketed by `words[0]` (player
  count for Player Forces, 9 Enemy Forces, 10 this list); only this list has
  "None". The config is saved in `RGS1`…`RGS9` order. A pick other than `NONE`
  merges its placements into the force chart's template, shifting every later
  draw.
- Slots 4, 5 read 19, 21, 24, 30, 31 or 34, and 0 or 1. Unread; they track the
  scenario, not the map size.
- Slot 6 is named from prefs: `RGS5` reads 30,000 in a profile saved as `30Kgold`.
- Slots 9, 10 are the dialog's size boxes (`RGS9` the first); the setup
  overwrites both from the scenario.
- Slots 9, 10 are not the size: they read 128 on 512-cell maps.
  Two games back to back, all settings held, read `512, 512` and
  `128, 128` and have bit-identical heightfields.
  With them masked, the other eleven match (**Measured**).

### Mods

**Binary**. After the cheats word: a freestyle game's config (HD writes
version 6), eight flag words, the dataset tag (`Rel\0` base, `Rel@`
expansion) — the last word on every save. HD writes version 11 only with a mod
applied, and then:

```text
u8[16]    the quest's GUID, Windows layout
u32       mods
          each: u8[16] GUID, cstr name
```

The quest id is **Inferred** zero for a freestyle game: it is cleared and no
quest record is picked. Mods keep the order saved, which is the order applied;
a name is narrow text, read lossily. A GUID listed twice is read once. A load finds each mod, refuses the save
naming the missing ones, else plays with those mods only.
**Measured** (a list spliced
into corpus saves: no modded HD save was at hand).

### The freestyle config

**Binary.** After the cheats word, a freestyle game's config, whose version is
the mode word (**Inferred**, from where the reader starts):

```text
u32      version — 6 on all 74 corpus freestyle saves (7 is never written)
u32[13]  the setup words above (12 from version 1, the Map Number from 2)
cstr     the profile, `Rel@default`: its first four bytes are the stamp (**Inferred**)
cstr     the build tree's name, `Default Settings` (version 5 on)
u32 × 2  0 and 1 (version 5 on); **Inferred** framing
u32 × 2  the special events (version 6 on): `EVSC` keys or `NONE`
```

The name and tree follow only after a named stamp: the expansion's `Rel@` or an
older one. **Corpus**: the events are `NONE` twice on every freestyle save but the
united winter trace's, `EV04` (Cannon Fodder) and `NONE`, as its `SETUP.txt`
says. The eight flag words follow the config, then the dataset tag.

### The map number is the seed, and it clamps at 1

- Map Number 1 to 5 → slot 12 reads 1 to 5, nothing else differs. The labelled
  saves read 1.
- Map Number 0 → writes 1 and generates the seed-1 map (two games at 0, one at
  1: identical on all 16,641 vertices). **Measured**.
- Unchosen numbers are large (729,593,286) and constant within one game.

A save holds the generator's input and output.
**Measured**.

### What a seed fixes, and what it does not

Four fresh games, two per landscape, all settings held within a pair:

| | Differs within a pair |
|---|---|
| Vertex heights | none |
| Scatter and buildings | none, by name and position |
| Cell terrain pair or blend case | under 4% |
| Cell art variant | over half |

- Heightfield, class layout and object placement are a function of the setup.
  The art variant and the quarter (`rotation`) are not: they come from a
  second, clock-seeded RNG ([gpl/runtime.md](../gpl/runtime.md#every-rng-and-its-seed)).
- The layout difference is a wobbling class boundary: every differing cell
  touches another. **Measured**.

### The Enemy Forces choice reaches the generator

Five games, seed 1, one landscape, only that choice varied:

- "Ruins of Esterhausen - S" vs "Rat Scouts - S": 149 of 16,641 vertices differ,
  all within eight cells of a building —
  [construction flattening](#the-ground-is-edited-by-construction).
- "Enchanted Forest - S": a different heightfield on the same seed, size and
  landscape.

The scenario sets a position in the RNG stream
([maps.md](maps.md#the-scenario-hands-the-generator-a-position-in-the-rng-stream));
two scenarios can spend the same draws.
**Measured**. Slots 4, 5 are the only other
words that move with the scenario.

### A map size is not a window onto one fractal

Seed 1, one landscape, 128 vs 512 cells: the small field is not in the large one
— not decimated by four at any phase, not its top-left corner. The best match
is about the shared pinned rim. Each size is its own field.
**Measured**.

## `MTER` — the map

```text
MTER
  MVTX / PRIM   (n + 1)^2 vertices
  MCEL / PRIM       n^2   cells
```

`n` = 128, 256, 512. Vertices are cell corners: terrain lives on vertices, a
cell draws the four around it ([terrain.md](terrain.md)).

Cell: 5 bytes.

| Offset | Field | Holds |
|---|---|---|
| 0 | `pair` | Two terrain classes, one per nibble |
| 1 | `case` | Blend case, `0..=16` |
| 2 | `variant` | `u16`, `0..=33` — art column |
| 4 | `rotation` | Quarter of a 2×2 group tile; `u8` |

Strides are HD's; the vertex flag byte is the one offset that moves between
editions. A cell is uniform, a single terrain rather than a blend, exactly
when `case` is 0 (every corpus cell).

Vertex: 9 bytes.

| Offset | Field | Holds |
|---|---|---|
| 0 | `height` | `f32`, observed −0.05 to 7.50 |
| 4 | `shade` | Baked light level |
| 5 | `flags` | Bit *p*: explored by player *p* |

- Explored bit: a small patch at a freestyle start (726 of 16,641 vertices);
  every vertex with the map revealed (**Corpus**).
- One bit per player; **Binary**: bit *p* is tested for player *p*'s sight
  of a unit. Bit 0 is the crown's. Slot 7's is on every vertex of every corpus
  save (**Measured**), so the monsters see
  everything. **Binary**: a new map reveals every vertex to mask `0x80`
  (`world::new_map::NEUTRAL_SEES`).

## The ground is edited by construction

Saves of one freestyle game, seed 1, size S: at the start, 30 s later idle,
day 2, day 4.

- Start → idle: `MTER` unchanged to the byte. Nothing in terrain is a clock.
- Start → day 2: hundreds of cells and vertex heights change, in two clusters,
  while `BLDG` grows. Construction re-tiles and flattens.
- `GRND` shrinks. Most scatter objects removed stand on re-tiled cells, far
  above chance; the rest just outside.
- **Binary**: an object keeps its own height and is drawn there. The flatten
  reseats only what stands still on the grid in the pad's cell rect grown by
  one; outside it, ground the slope clamp moved leaves the object at its old
  height. **Corpus**: never seen — every `GRND` height is the ground under
  it, the clearing ring reaching past the rect. `Board::level`,
  `placement::reseated`.

`GRND` is simulation state, not map data.

## The object chunks

| Chunk | Holds | Examples |
|---|---|---|
| `BLDG` | `UNIT` per building | `ABJ1 Palace1`, `ABG1 Library1`, `ABY1 Wizards_Guild1`, `BBx1 RatsNest` |
| `VHCL` | `UNIT` per mobile | `AVH1 Ranger`, `AVN1 Wizard`, `ARG1 thought_bubble_`, `XR13 Damage_Shield` |
| `GRND` | `UNIT` per scatter object | `RG08 rocky_trees_med`, `CG07 scorch_trees_big` |
| `RWRD` | `UNIT` per standing reward flag | `ARA2 Flag_Attack`, `ARA4 Flag_Explore` |
| `MSGF` | `UNIT` per standing message flag | `ARE1 message_flag` |

- `VHCL` = every moving thing (heroes, monsters, projectiles, overlays, thought
  bubbles); "vehicle" is the engine's word (`VehicleList`, `VehicleDesc`,
  `CreateVehicleMessage`).
- Lairs are `BLDG`: `RatsNest`, `Animal_Den`, `Goblin_Camp`, `Dragon_Lair`,
  `Liche_Queen_Lair` are `Building` subtypes.
- Upgrades edit in place: `ABJ1` → `ABJ2` → `ABJ3` at the same coordinates.
- A `RWRD` record is a flag the player placed and no hero has claimed:
  unit kind 4, `AE:0`, `APR:0` `FlagSeekers`, `APR:1` `RewardCost`,
  `APR:2` `TargetID` (an attack flag's target's `AE:0`). **Binary**: the
  treasury pays `RewardCost` as it is set and raised
  ([gpl/vm.md](../gpl/vm.md#prices-and-research)). **Game**: a played save's one
  holds the words the world gives it.
- A `MSGF` record is a message flag (`world::message`): `ARE1`, no agent
  (`AE:0` 0), `APM:0` its text, held by the unit it hangs on. None in the
  corpus saves; **Game**: The Barren Waste's saves hold one each, the
  world's in place, set, frame and text (**Measured**).

Each record is a `UNIT` wrapping a `PRIM`; on every one:

```text
+0x00  u32      kind — the chunk's: 0 BLDG, 1 VHCL, 2 GRND, 4 RWRD
+0x04  u32      handle — unique in the save
+0x08  u32      state bitfield
+0x10  char[4]  descriptor id — a DUNT record
+0x14  u32      owning player (word 5)
+0x18  u32      holder's handle, 0 for none (word 6)
+0x1c  i32      x, cells, 16.16 fixed point
+0x20  i32      y
+0x24  i32      height (word 9)
+0x28  i32      half cell x the engine measures from (word 10)
+0x2c  i32      half cell y
+0x38  u32      vtable pointer, the same on every record
+0x40  u32      property count — +0x50 on a Character
```

then the property list, then the drawing tail.

- The record is a dumped C++ object; offsets are member offsets.
- Read properties by key, not offset: `APU:0` is at `+0x58` on a building, but a
  character's list starts sixteen bytes later.
- `VHCL` holds every mobile thing, as the class map's `VehicleList`.
- `HP` (`APU:0`) is negative on something just killed. An undamaged building holds
  its descriptor's maximum exactly (`ABJ3 Palace3` 1000); mobiles stay under a
  maximum that levelling raises. A scatter object has none (**Measured**).
- Position is signed. Nearly every object stands on the map; the rest hang on a
  host (overlays and particle systems positioned relative to it) or are
  rangers away in distant lands, 250 32nds past an edge, activity 15. A few hold
  a negative coordinate (unsigned: 65,535 cells).
  **Measured**.
- Buildings and scatter sit on half-cells; mobiles anywhere.

### Where a unit stands

**Binary**: word 0 is the kind the unit lists sort on; word 6 the holder.

| | What |
|---|---|
| words 7, 8 | position, 16.16 cells |
| — | GPL coordinate: position `>> 11`, 32nds of a cell |
| words 10, 11 | half cell: the coordinate over 16, toward zero; a held unit's holder's |

A unit is held when word 6 names a holder and state bit 7 is clear.
**Measured**: word 0 is the
chunk's kind on every record; words 10 and 11 are the position's half
cell on every building, scatter object and unheld flag,
the holder's on every held mobile and held flag, and the position's
on about half the unheld mobiles: a walking mobile's half cell is set as it
moves.

Distances, searches and sight: [gpl/vm.md](../gpl/vm.md#positions).

### The handle

`+0x04` is unique across all four chunks within a save, on every corpus save — unlike any
kind, flag or timer field. Not the agent id; an object has both.

The `MAP ` record's ninth word (byte `0x20`) is the last handle issued:
highest object handle on most saves, one higher on a few. A load takes it as
the handle counter; without it the first agent created
after load collides with scatter. **Game**: the Liche Queen's 4-day save holds
1835, its last unit 1834.
**Measured**.

### State bits

| Bit | Means |
|---|---|
| `0x10` `RUINED` | Structure in ruins |
| `0x01` | Selected by the player |
| `0x02` | Held by another object, so not drawn; an overlay or flag hung on a unit is not held, unless its holder is |
| `0x20` `DONT_BLOCK` | Its descriptor's `Info` `DontBlock`: never on the collision grid (**Measured**) |
| `0x40`, `0x80` | Unread. `0x40` on goblins `BVL1`–`BVL3`, `APA3`, `BPA7`, `MVa1`, `MVa2`; `0x80` on goblins only, which carry all three high bits |

- `0x10`: killed lairs, `ABe1 General_Housing` burned out at −2 to −16 hp,
  `BBM1 Ruined_Shrine1`. Not "hp gone": chests `BBt1`–`BBt4` sit at 0 hp (max
  20) without it; keying on `hp <= 0` calls fourteen chests rubble.
- `0x01`: read back raw, then dropped at every game start
  (**Binary**): a loaded save selects nothing (**Game**). Most "0 Days" saves
  select the Palace (as the game does at founding); saves taken to follow a hero
  select him. One hero reads `0x03`:
  selected, inside his guild. **Measured**.
  Not "building working": one played save draws lit buildings while nearly every
  object reads 0 (see the [set id](#the-set-id)).
- `0x02`: hero in a guild, peasant in the Palace, overlay on its unit, goblin
  folded into a warband. Position is last seen, so drawing it puts a hero in a
  wall. Every one is named by another object's `CONT` list; the converse fails.
  **Measured**.

### Two ways a building leaves `BLDG`

One game: intact; `ABY1 Wizards_Guild1` destroyed; `ABV1 Warriors_Guild` built
on its spot.

| | Guild at (115.0, 105.0) |
|---|---|
| intact | 350 hp, state `0x00` |
| destroyed | 0 hp, state `0x10`, same position |
| built over | no record |

A ruin is a building with a bit set; building over removes it
(**Corpus**; `world::building_ground`).

**Binary**, **Game** (the defeat save): a ruin from play also has `AE:0`
0 (`DeleteAgent` wrote it, the unit stays), `APB:2` 0, set `Dead` at frame
0, and an `ORDR` entry: kind `0x4003`, 60,000 ms (`#RubbleCleanupPeriod`)
from its collapse, callback `0x2007`, which deletes it. So a ruin leaves on
its own a minute on ([gpl/vm.md](../gpl/vm.md#a-building-falls),
`world::ruin`, `a_ruin_stands_its_rubble_period`).

## An object record is a keyed property list

```text
+0x40   u32       property count — +0x50 on a Character
        char[3]   bank, zero-padded
        u8        index
        i32       value                ... count times
```

- Every corpus record parses; hence variable record
  size (`BLDG` 180–276 bytes). The corpus holds ten banks
  (**Measured**); flags add `APM` and `APR`.
- Bank = namespace, index = field: `APU` living things, `APB` buildings, `APV`
  characters. Full set: `ABB`, `AE`, `AP`, `APB`, `APM`, `APR`, `APU`, `APV`,
  `ARB`, `AZ0`, `INV`, `LUC`.
- Two possible offsets. A `Character` has 16 more bytes first. Pick the first
  candidate whose count is followed by that many well-formed keys; on the wrong
  one the count is a pointer. **Measured**.

### The engine names every key, bank and index

**Binary.** One function registers each attribute with its name, bank and
index as constants: 178 in the base game, 207 in the expansion, one call
each. The table is [`data::attrib`](../../crates/regent_core/src/data/attrib.rs)
(`EXPANSION`, the base game's 178 and 29 more, no index moved);
`the_engines_names_agree_with_the_joins`. `AE:0` is `AgentID`.

- A bank is a class: `APB` building, `APU` unit, `APV` vehicle (hero stats),
  `LUC` lair, `APP` player, `XSC` GPL script callbacks; expansion `AZ0`, `ABB`,
  `ARB`, `AEM`, `ASP`, `ASS` — zoo, bazaar, embassy, Sorcerer's Abode.
- `AP` is the project's initials: the original's source paths are
  `Source\AP\`, and `SMNU` panels are `AP##`.
- String position ≠ index outside four contiguous banks:
  - Banks have gaps: `APB` skips 38, `APM` is 0, 32, 64, `AP` has no 0. No save
    writes `APB:38` (`a_banks_gaps_have_no_name`).
  - `APB:56` is `ButtonStartTime`, last of the building bank; position-as-index
    puts it on `Strength` (`the_building_bank_reaches_its_last_entry`).
  - Indices are stable: all 178 base keys mean the same in the
    expansion, whose eight new names fill the gap `APV:54`–`APV:61`.
- The keys derived independently from data (below) all land, including
  `APB:0` `MaxGuildMembers`.

Engine names that correct derived ones:

| Derived | Engine |
|---|---|
| `Attack` | `HtoH` |
| `ExperienceNeeded` | `LevelXP` |
| `TaxedGold` | `StoredGold` |
| `Activity` | `AIIntentionString` — a hero has an intention, not an order |
| `Enchantments` | `HasEffectStoneSkin` — one of fifteen `HasEffect*` booleans, not a count |

- `MagDelta*` = `APV:100`–`APV:109`, same order as the ten stats at
  `APV:0`–`APV:9`. Only those ten can carry a modifier
  (`a_modifier_is_named_for_what_it_modifies`).
- Every key in the corpus is named except `INV:n`, an inventory slot whose
  index is a `STRT QITM` item. Clone check: `CloneParentID` and `WhichClone` on
  the same records, `WhichClone` 0 or 1 equally often, `NumClones` 2.

### What the keys mean, by joining to the descriptor tree

Joins against the `DUNT` value over hundreds of objects. `APV:0`–`APV:9` are the
descriptor's ten stats permuted, each the other's unique best match, agreeing
on over half (**Measured**):

| key | stat | | key | stat |
|---|---|---|---|---|
| `APV:0` | Strength | | `APV:5` | Attack |
| `APV:1` | Intelligence | | `APV:6` | RangedAttack |
| `APV:2` | Artifice | | `APV:7` | Parry |
| `APV:3` | Vitality | | `APV:8` | MagicResistance |
| `APV:4` | WillPower | | `APV:9` | Dodge |

Disagreement is levelling; only `WillPower` agrees everywhere. A misread gives
no bijection.

**Corpus**, by join, not test-pinned:

| key | means | agrees |
|---|---|---|
| `APU:0` | current hp | — |
| `APU:1` | max hp | nearly all buildings |
| `APU:3` | sight range | all buildings, nearly all characters |
| `APB:0` | guild roster limit | nearly all |
| `APV:10` | experience a level costs | all |
| `APV:12` | speed | all |
| `APV:13` | attack range, min | all |
| `APV:14` | attack range, max | nearly all |
| `APV:18` | weapon damage | all |
| `APV:21` | armour damage | nearly all |
| `APV:47` | primary stat | all |

- `PLYR` uses the same list with one entry, `APP:0`, the treasury: 30000 at a
  freestyle start, 2690 at day 14. **Binary**: a new game's is the setup's
  slot 6, for each live player; none for 0
  ([gpl/runtime.md](../gpl/runtime.md#with-no-save)). A load reads every
  `PLYR`'s; **Corpus**: one other than the crown's, The Siege's slot 1
  (**Measured**).
- Undamaged buildings hold exactly the descriptor's max hp (`ABJ3` 1000, `ABV1`
  700, `ABY2` 500, `ABF1 Inn` 120, `ABe1` 75, `BBx1` 150); damaged below, killed
  negative.
- On every character: saved max ≥ descriptor max (levelling only raises
  it), current ≤ saved max. **Measured**.

### What a character sheet names that no descriptor can

Two saves of `AVN1 Wizard` "Rendi the Amazing", sheet transcribed:

| key | means | early | later | sheet |
|---|---|---|---|---|
| `APV:11` | level | 21 | 21 | 21 |
| `APV:16` | experience earned | 347 | 430 | "347 of 2000", "430 of 2000" |
| `APV:10` | experience a level costs | 2000 | 2000 | "of 2000" |
| `APV:24` | taxed gold | 93 | 1528 | 93, 1528 |
| `APU:2` | untaxed gold | 2600 | 0 | 2600, 0 |
| `APV:35` | healing potions | *absent* | 5 | 0, 5 |

- Keys come and go: a missing key is not an error.
- `APV:10` matches its descriptor everywhere because it is the threshold, not the
  earned amount (`APV:16`). A perfect join does not name a key.
- The ten stats match the sheet. Attack/Dodge (both 45) and Parry/Resist (both
  35) are indistinguishable in this file; the bijection separates them and the
  sheet agrees.

Four saves of `AVL1 Warrior` "Sir Lucas Silveredge":

| key | means | across four | sheet |
|---|---|---|---|
| `APV:30` | activity | 45, 36, 4, 36 | purchasing armour, berserking, going home, berserking |
| `APV:19` | weapon upgrades | 1, 2, 2, 2 | "level 2", then "level 3" |
| `APV:22` | armour upgrades | *absent*, 1, 1, 1 | "level 1", then "level 2" |
| `APV:48` | tax collector's minimum return | 275 | 275 |
| `APV:49` | tax collector's minimum pickup | 300 | 300 |

- `APV:30` = 4 for "Going Home" on a warrior and a wizard. It indexes
  `STRT AITX` (177 entries, `the_labelled_activities_are_the_ones_the_game_ships`;
  [payloads.md](payloads.md)).
- Upgrade counters count upgrades, not levels: one less than shown, absent at 0.

### A stat already includes what an item gave it

Save four adds a Ring of Protection and Stoneskin: Parry, Magic Resistance and
Dodge rise by 10; `APV:107`, `APV:108`, `APV:109` appear at 10 each. A modifier
sits 100 above its stat; the stat is the total.

- Corpus: every `APV:100+n` entry has its base present. Bases used:
  Strength, Intelligence, Attack, Parry, Magic Resistance, Dodge. Values −10 to
  +35, so curses too.
- Save five (ring on, Stoneskin expired):

| | armour | `APV:40` |
|---|---|---|
| neither | 9 | absent |
| ring + Stoneskin | 17 | 1 |
| ring | 11 | 0 |

  Ring = 2 armour, Stoneskin = 6. `APV:40` follows the enchantment.
- `INV:11` appeared with the ring. Every `INV:n` value is 0, so the index is
  the item: `STRT QITM` 11 = "Ring of Protection" (+10 dodge, parry and
  resist, +2 armour). No `APV:121` for armour: modifiers cover the ten stats
  only. **Measured**.

### `APU:2` is the coffer, the purse and the bag

One key: a building's coffer, a hero's untaxed purse, a tax collector's load.
Many buildings hold some, the most in the Fortress of Ixmil
(**Measured**).

Fills over a game: a Wizards' Guild 0, 0, 0, 341, 343 over five saves in 15
days; `BBx1 RatsNest` 0, 2, 8, 29, 29.

| Holds gold | Never |
|---|---|
| `BBQ1 FortressofIxmil`, `BBB2 Dark_Castle2`, `BBz1 GoblinFortress`, `BBw1 Ice_Cave`, `BBu1 Ancient_Graveyard` | many kinds, including wells and towers |
| `ABH3 Marketplace3`, `ABF1 Inn`, `ABC2 Blacksmith2`, `ABe1 General_Housing`, guilds | |

### `PrimaryStat`, settled against all sixteen classes

`APV:47` mirrors the descriptor's `primary_stat`. **Game**, per class:

| value | stat | classes |
|---|---|---|
| 0 | Artifice | Rogue, Cultist, Elf |
| 1 | — | none |
| 2 | Intelligence | Healer, Paladin, Priestess, Wizard |
| 3 | Strength | Adept, Ranger, Solarus, Warrior, Dwarf |
| 4 | Vitality | Barbarian, Warrior of Discord |
| 5 | Will Power | Monk, Gnome, Gnomechamp |

- The five attributes alphabetically, 1 skipped (unused). Values used: 0, 2,
  3, 4, 5.
- 0 is also the default. A price separates them: in the base dataset the
  `Character` descriptors that cost gold are exactly the sixteen classes.

Three gold-costing `Character`s that are not player-picked classes:

| id | what | primary | shipped? |
|---|---|---|---|
| `AVO1 Gnomechamp` | Gnome variant (same attributes, price, hp, delay). A level-8 Gnome becomes one via `$ChangeUnitType(thisagent, "GnomeChamp")` in `mx_give_exp.gpl`; the 100 gold is inherited, never charged | 5 | expansion |
| `XVN1 Sage` | Wizard with 68 hp, `NotHero` | 2 | only `xqd1_unittype.cam` |
| `BV02 Enemy_Barbarian` | monster Barbarian: attributes 20, 9, 8, 5, 25 as the player's, primary 0 not 4 — 0 is "not applicable" | 0 | only `mdl1_unittype.cam` |

Not the test: `NotHero` (the Gnome has it and a real primary); "recruitable"
(Gnomechamp is promoted into). `every_character_that_costs_gold_is_a_hero_class`,
`desc::Character::costs_gold`,
`every_hero_classs_primary_stat_is_the_one_the_game_names`.

## `AE:0` is the agent id

The engine registers `AE:0` as `AgentID`; `GplAgentRef` is a GPL value type.

**Corpus**: scatter never carries one, and a few non-scatter objects read 0
(**Measured**).
Buildings and characters mostly have one; overlays rarely; particle systems,
projectiles and scatter never.

- One counter across chunks: in one save `BLDG` runs 2..104, `VHCL` 45..129, no
  value shared.
- Never the handle. `ORDR` and `CONT` key on the handle.
- Every object carries the key; the value is the signal.
- The non-scatter zeros are ruined buildings (`DeleteAgent` zeroed
  them), goblins `BVL1`–`BVL3` with state bits `0xe0`, and units with
  `DONT_BLOCK`: overlays, effectors, particle systems, missiles, minimap
  marks, which the engine makes with no agent
  (**Measured**). Median 14
  properties with an id, 1 without.

**Measured**;
`a_banks_gaps_have_no_name` says the rest of `AE` is empty.

## Word 5 is the owning player

**Binary.** The record writer copies the unit's owner into word 5. The same
field is what `GetUnitPlayerNumber` returns (SDK: "the owning player
number") and `SetUnitPlayerNumber` sets.

- A player, not a team: `GetPlayerTeamNumber` is separate.
- Dead ends: `APV:15 Team` (0 on every object); `AP:3
  Controller` (registered, never written).

### What a second kingdom in the corpus shows

The Siege (256 cells) and Urban Renewal (128) hold a rival.

- One `PLYR` per player, so a three-player save has 19 top-level chunks.
- The Siege holds `ABJ4 EvilPalace` at owner 1; the selection bit still lands on
  the crown's Palace in both.
- Four `ABA1 Siege_Marker` name set `Stand` with no animation sets on the
  descriptor, and draw a scatter object's picture.
- `BDEP` guards the crown's menu only: most building kinds standing against
  their own rule are never the crown's (rival or slot 7;
  **Measured**)
  ([settings.md](settings.md#a-rule-governs-the-build-menu-and-nothing-else)).
- The rival's `ABl2 MagicBazaar2` carries `ARB:0`, `ARB:1`, `ABB:2`, `ABB:3` —
  the `ResearchBazaar` split across two banks that `attrib::EXPANSION` has from
  the registration table.

### Slots 0 to 6 are kingdoms; slot 7 is not

**Binary.** Player setup marks slots 0–6 as kingdoms and slot 7 not; the
`PLYR` writer emits one chunk per occupied slot of eight, its first word the
slot number (so the treasury, `APP:0`, is at byte `0x10`), its third the
kingdom mark. Teams are a side table of eight player bitmasks on the level,
in no `PLYR` record; `GetPlayerTeamNumber` reads it.

Slots present (**Measured**):

| Players | Saves |
|---|---|
| `[0, 7]` | all others |
| `[0, 1, 7]` | The Siege, Urban Renewal |

- Unknown: whether kingdoms fill in order or a quest names its rival's number.
  No save uses slots 2–6.
- `PLYR` word 2 is 0 on every player; undecoded, not the team
  (**Measured**).
- The first `PLYR`'s `APP:0` is the crown's treasury. **Game**: it matches two
  saves' on-screen gold, 22,713 and 15,101. Player lists have the object
  list's shape, at `+0x10`.

| Owner | In |
|---|---|
| 0, crown | `BLDG`, `VHCL` |
| 1, rival | `BLDG`, `VHCL` |
| 7, no kingdom | `BLDG`, `VHCL`, `GRND` |
| none (`0xFFFFFFFF`) | `GRND` |

**Measured**. Objects by owner: 0 on 1,361, 1 on 61, 7 on 1,482, none on 238,666.
The descriptor does not set it (`ABf1 Gnome_Hovel` reads 0 twelve times, 7 nine;
eighteen codes have several owners) nor does the `A`/`B` prefix (`BBJ1` is the
crown's). 175 `GRND` records read 7 (`BGD1`, `BGD2`, `BGK2`, `BGL2` and three
others). All 833 crown buildings stand on explored ground; 53 of the rival's 57
do not.

### What slot 7 holds, and what it does not

Not "the monsters". The eleven `A` codes at 7 are the same in every Day of
Reckoning save, one record each: the temples of Lunord, Krolm, Agrela,
Krypta, Helia, Dauros and Fervus, the Gnome Hovel, Elven Bungalow, Dwarven
Settlement and Royal Gardens.

- **Game**: that quest scatters neutral buildings the player discovers and
  adopts. **Corpus**: its `BLDG` holds the crown's at 0, those 11 at 7, no
  monster building; all 11 on unexplored ground.
- Not "everyone but you": the rival has its own number, and Urban Renewal's
  rival owns a Goblin_Camp, a Goblin_Hovel and four Sewers.
- **Inferred**: the owner changes when a neutral joins. No save pair spans
  a discovery.

### It explains the code prefix rather than repeating it

- Every crown building is on explored ground. The rival's are mostly dark,
  and slot 7's nearly all dark, the few lit ones in games under way
  (**Measured**).
- `BBJ1 Graveyard` is the crown's. Several codes have more than one owner, mostly
  crown and rival (**Measured**).
- The crown's only spawner is `ABN1 Sewers`.
- `ABA1 Siege_Marker` is a `BLDG` record but not a building: no hp, cost or
  sight range, draws a rock.

**Measured**.

## `SECD` — the name and the spell list

```text
char[]   name, NUL-terminated — empty if none
u32      spell count                 \ only if anything follows the name
         char[4] id, u32, u32        / count times, 12 bytes each
```

- Parses exactly on every corpus save: **Measured**.
  No `SECD` = no name, no spells.
- Example: "Rendi the Amazing" (17 + NUL) + 4 + 6 × 12 = 94 bytes; 106 three days
  later with a seventh spell.
- Names are generated ("Agilla the Pensive One",
  "Loo Pilferpocket"); numbered kinds too ("City Guard #1", "Caravan #8").
- A numbered name is the engine's, not the scripts': **Binary**, `GMTX` 184
  `%s #%d` for a `NumberedName` unit with no name and no `NameGenType`. A
  building takes the title its `UpgradeFrom` chain starts at, a character
  its own; the number is the owner's count of the key, `Peasant_female`
  counted as `Peasant`. Counted as bought (placed, a recruit at its decode,
  a henchman made by `SpawnUnit`, a typed `AdjustPlayerData` spend) or
  gained (from the chart or given), before the unit is numbered, never given
  back; an upgrade keeps the name. Numbered characters: peasants, tax
  collectors, city, veteran and palace guards, caravans. `GSGL` saves both
  counts per player (`gsgl::Record::counts`). **Corpus**: every numbered
  building and character reads so
  (**Measured**); the world
  numbers and counts as played games' saves do (`world::numbered`).
- Spell ids: `WRa1 energy_blast_effector`, `WRb1 fire_shield_effector`, `WRc1
  fire_blast_effector`, `WRd1 teleport_effector`, `WRf1 resist_magic_effector`
  are `DUNT` overlays; `WRe1 fire_ball`, `WRg1 meteor_storm` are `DACT` actions.
  Entries keep the game's listing order: **Game**, a wizard's six listed spells
  match six entries in order, and a later save adds `meteor_storm`, newly learned.
- First word: **Inferred**, rises with play, shared by spells learned together,
  highest on running timeouts. Its unit and zero point are unknown.
- All 97 non-zero timeouts of 308 entries equal the `DACT`'s `timeout duration`:
  a copy, not a remaining count.
- Second word: a running timeout. Every non-zero one is
  equal to the descriptor's `timeout duration` (Teleport 36,500, Resist Magic
  41,500, Fire Ball 4,500). Zero = elapsed, not "no timeout" (`WRa1` declares
  1,000, reads 0 on heroes). **Measured**.

## An object says which picture of itself it is

`NGa1 SnowPine` has fifteen slots on its `Inactive` set; the record says which.
**Binary**: the record ends, after the property list, with the unit's
animation object, one entry per drawn layer:

```text
u32  layers
u32  flags — bit 0: a particle system follows; bit 1: a u32 follows
per layer:
  u32  set id — the IMAG set being drawn
  u32  part flags
  u16  slot of that set, or a direction   <- last layer: len-8 (Gold HD: len-6)
  u16  frame of that slot
  u32  link — 0 alone, 1 atop a stack     <- u16 in Gold HD
then the particle system, then the u32
```

Only the link differs by edition, widened like `rotation`.
The set id, slot and frame are read from the back, so they are the last
layer's, and hold where the flags are zero.

The entries are the unit's parts, so a version 5 slot effect's part
(part flags kind `0x2000`) follows the layers; the reader passes over it for
the set shown. No corpus save holds one. The list holds room for the most
effects a slot of the set has: a part left unused has flags bit 3, as do a
particle system's entries, whose last is its set.

| Chunk | Layers |
|---|---|
| `BLDG` | 1 to 4, mostly 1 or 2 |
| `GRND` | 1, a few 2 |

- Part flags: 2 everywhere but some buildings with 18. Bit 1 is set when the
  set's header byte lacks bit 0 (**Binary**); bit 4 is unread.
- The flags are set only on projectiles, overlays and effects in `VHCL`, and
  one building. **Measured**.

### A particle system's record

**Binary**. A particle-system unit (`XLSL`, `XL23`, …) has one tail entry
per particle (`MaxParticles`), then its system, offsets from the payload's
start:

```text
+0   u32   version, 3
+4   6×f32 bounds, min then max
+28  3×f32 emitter words
+40  u32   window, ms: -1 endless
+44  u32   emitter lifespan, +48 the same
+52  f32   rate
+56  u32   window start, game ms: made, or stopped
+68  u32   live particles, +72 the most
+76  u32   last update, game ms: the save's clock
then the image's set and attachment, each particle
```

- Held, it stands about its holder, at the offset its part's slot gives:
  x over 32, y over `32 × 0xd14d`. Stopped: no holder, where it stood.
- Its window past, its system is gone: no payload, tail flag bit 0 clear.
  A held one stays so; a stopped one is queued for deletion (state bit
  `0x04`). **Corpus**: every stopped system was updated at most its window
  after its stop; one queued (united's `XL31` #1037).
- `world::plume::Plume`; it matched every such record of the defeat,
  dwarves and united saves (**Measured**).

### The set id

The packed `IMAG` set. Taken for a constant it reads `0xd0` = 208 = `Inactive`
(most things stand still), so a wrong reading looks right. Checked by joining to
the record's own `IMAG`:

| Chunk | Set present | Family only | No art |
|---|---|---|---|
| `BLDG` (1,707) | 1,703 | none | four |
| `GRND` (207,097) | all | none | none |
| `RWRD` (2) | both | none | none |

- No art: the four `ABA1 Siege_Marker`.
- A record with tail flag bit 1 carries a trailing value; read from the back
  without it, the words sit four bytes off (`BBQ1` seems to play `Attack-3`,
  `AVD1` `Walk-2`). Read forward, every record names a set it has. No
  fallback within a family is evidenced.
- `VHCL` excluded: projectiles, overlays, thought bubbles have another tail
  (`XLSL` reads 61,350 where a character has a direction: tail flag bit 0, a
  particle system after the entries). Characters are in the next section.
- Top byte = layer, always the last of its id. A repeated id is a stack drawn
  together: draw every layer of the low half.

What it says:

- Lit = `Active`. In one played save the occupied Palace, Warriors' Guild, Rogues'
  Guild, Marketplace, Temple of Dauros read `Active`; empty Blacksmith and house
  `Inactive`; the burning Guardhouse `Die-4`.
- `Die-1`…`Die-8`: death animations on a character, damage stages on a
  building. Each a stack: layer 0 the structure, higher layers fires.
  `ABH3 Marketplace3`: 1, 2, 2, 3, 3, 4 fires over `Die-1`…`Die-6`, layer 0
  stepping through three pictures.
- Ruin `Dead`; collapsing `Crumble`.
- Characters: `Stand`, `Walk-1`, `Attack-1`, `Cast`, `Dead`.

**Binary**: the stage is the hit points lost, split evenly over the `Die`
sets the art has; built, lit, unlit and ruined likewise
([art.md](art.md#which-set-a-building-shows), `sim::damage`). Every saved
building but four quest markers shows the set it picks
(**Measured**).

**Measured**.

### The slot, and the direction

Against the set the save names:

| Chunk | Slot | Frame |
|---|---|---|
| `GRND` | every one | every one |
| `BLDG` | all but four | all but four |
| `VHCL` | not a slot | every character |

(**Measured**.)

- On a mobile the word is a direction out of 32: most characters
  playing a set with facings exceed their set's slot count, up to 31 against eight-slot sets. SDK,
  `SetEffectorDirection`: 0 to 31, 0 north, rising clockwise. All characters
  read 0–31.
- Slot = direction spread over the set's slot count (8 or 16; `BVq1` has 16).
  One-slot sets (corpses) ignore it; so do one-picture buildings (two
  `ABE2 Guardhouse2` read 19 and 15 against single-slot sets).
- Slot 0 = direction 0 = north = up the screen
  ([below](#north-is-up-the-screen)).
- Four `ABf1 Gnome_Hovel` of three visible kinds read slots 0, 1, 2, 2. 58 of 64
  multi-slot descriptors reach their last slot. The frame is in range on all
  1,703 checkable buildings and 207,097 scatter objects.
- 491 characters exceed their set's slot count. The slot is rounded to the nearest
  and wraps: with `w = max(32 / slots, 1)`, slot `((w / 2 + direction) / w) mod
  slots`; with 8 slots, slot 0 covers directions 30 to 1. This matters for 62 of
  726 corpus characters against an unrounded facing. A direction past 31 (a
  projectile's 61,350) reads as 31.

**Measured**.

## The 17 blend cases, recovered from the data

[terrain.md](terrain.md) derives the table from the art. A save recovers it
independently:

1. `case == 0` cells have four corners of one class; that labels vertices
   (about half the cells).
2. Other cells: which labelled corners carry the high nibble is a four-bit mask.
   Same case must give same mask; neighbours share corners. Over-determined.

Whole corpus: no labelling clash, one mask per case, all 17 slots of
`content::terrain::schema` match as complements.

- Low nibble = far terrain, high nibble = body.
- Slot 0 = all corners far = `case == 0`.
- The empty mask never occurs (that is the body set).
- Slots 7, 8 (the `Through` diagonals) never occur; diagonals are always
  `Apart`.

### The cells invert to one class per vertex

Each cell hands its corners a class through the table, giving back the vertex
field: `terrain::ground::Ground`.

- Pairs are written low class first: `near` < `far` on every blend
  cell. Canonical, so generated and saved cells compare directly.
- A few vertices get two different classes, most of them
  where three classes meet (a cell names only two). The rest unexplained. Spread thinly
  over every save: an encoding property.

**Measured**.

### What the paint pass can be held to

Compare maps by vertex field. The twin pairs
([seed section](#what-a-seed-fixes-and-what-it-does-not)) agree on at least
98% of vertices and 95% of uniform cells, never all. The game does not
reproduce its own paint. Twin agreement is not a model's ceiling: a model's
expected bar is (100 + twins) / 2 ([maps.md](maps.md#the-generators-fidelity)).
**Measured**.

## Writing a `.gmp`

The project writes no `.gmp`: generated maps need not load in the original.
Vertex and cell bytes that `Terrain` does not read are zero on every record (**Measured**). A generated map's checks run on the generated world.

### A chart's cells become a map's vertices, and the rim repeats

The generator works in `n²` cells; the map stores `(n+1)²` vertices. The extra
row and column repeat their neighbour: last vs second-last differ on about 2%
of rim vertices; first vs second on about 12%.

A vertex takes its class from the same-index cell, clamped at the edge;
`Ground::cells` inverts. A real field round-trips losing a few vertices
in a million, all three-class corners.
**Measured**.

### The `MAP ` record says how big the map is

Words 0 and 1: width and height, equal to the terrain side on every save.
**Measured**.

### `GRND` is written in the walk that created it

Scatter is listed x outer, y inner — the landscape placement's walk
(**Binary**), not the mobject chart's shuffled order. Every record of the
128-cell measured save; all but the last two of the 512-cell one (those,
`BGL2` at fractional positions, are created elsewhere). Handles rise with the
walk. `sim::mapgen::fresh::stand` takes them in that order.

### The slot each object draws is a draw, not a re-roll

**Binary**: each object, in the walk above, spends one `rng_next` over its
set's slot count as its animation starts. So all slots are a function of the
map number.

Stream position: the fractal re-seeds per distinct height profile, so the draws
resume where the last profile's pass ended, not at the seven charts' total.
`sim::mapgen::generate_from` returns that state; `sim::mapgen::animate::slots`
spends it. Nothing draws between: player setup draws nothing, and bare
placement measures zero draws on every land checked.

Every slot of the saves checked (two lands, two map numbers, three sizes).
**Measured**.

### The fog is a sight range about each of the player's units

The engine restores the explored bits from the file; an all-unseen map starts
dark.

- Explored = union of the discs laid as each unit is placed (**Binary**):
  about the cell at the middle of its pad, `sight range / 32` cells plus half
  the pad's mean side, for its owner (`sim::world::reveal::pad_disc`,
  [gpl/vm.md](../gpl/vm.md#explored-ground)).
- **Corpus**, the HD freestyle saves' own building places: the discs light
  nearly every explored vertex, none extra, most saves exactly. **Inferred**:
  the rest is what else reveals (a mobile, a building's disc at its chart cell
  before its search). Discs of `sight / 32` cells about the building alone light
  far fewer, some extra, no save exactly.
- A fresh world's explored ground (`sim::mapgen::fresh::stood`) is the
  seed-7 save's exactly.
  **Measured**.
- A played save's explored ground is wider; the rest is where heroes walked.

### A scatter record restates its own position twice

Beyond position, handle and descriptor, three words move with the object:

| Word | What |
|---|---|
| `+0x24` | The ground height at its place, to the bit, on every one; on 235,515 the vertex height at its cell, the rest off a whole cell |
| `+0x28`, `+0x2c` | Position in half cells, as integers: `2x` and `2y` on 238,697 records |

The rest of a 98-byte scatter record is constant corpus-wide bar vtable pointer,
set id, slot and frame; two-layer records are one tail entry longer
(**Measured**).
**Measured**.

### A slot is fixed by the setup and a variant is not

Twin pairs compared by position:

| | Agree |
|---|---|
| Scatter slot | all |
| Cell `variant` | under a quarter |

**Measured**.

- Slot: reproduced by `sim::mapgen::animate::slots`
  ([above](#the-slot-each-object-draws-is-a-draw-not-a-re-roll)).
- `variant` and `rotation`: not reproducible. `sim::mapgen::dressing::spread`
  picks a variant by position; rotation stays 0.

## `variant` indexes the art, and the pair is always the authored way round

`variant` is a column of the set drawing the cell:

- blend cell: its case's variant list (1, 3 or 4 long);
- uniform cell: the body set's whole `frames × streams` grid, group columns
  included.

Against stream 0's plain columns only (`content::terrain`'s list) it overruns on
four body sets (Dirt offers 31, reaches 31; Snow offers 24, reaches 31).
Against the whole grid it fits every cell of the corpus, all sizes. And every pair names its blend the authored way:
`MapTexture{X}to{Y}` always forwards, never complemented.
**Measured**.

### Which column the engine picks

A body column's stream-0 nudge x (frame word `0x04`) is its class.
**Binary**: the paint pass gives a uniform cell a random column of class 0.
Other classes, group columns among them, come rarer; the path to them is not
read.

| Dirt columns | Class | Cells each (rotation 0) |
|---|---|---|
| 0–15 | 0 | many |
| 17–25, 16, 32, 33 | 1 | far fewer |
| 26–31 | 2 | far fewer |

**Measured**.

`sim::mapgen::custom::Art::dress` picks a plain column (not a group) by
position, each alike: the true column number, so Dirt reaches 31 and never
draws a lone quarter of 16. **Measured**.

## `rotation` is not a rotation

A body set's group columns (all four streams carry a tile) are one picture over
2×2 cells, a quarter per stream. `rotation` is the quarter:

```text
  variant = 26, one picture, four cells

  +--------+--------+     stream 0  stream 1
  | rot 0  | rot 1  |         TL       TR
  +--------+--------+
  | rot 2  | rot 3  |     stream 2  stream 3
  +--------+--------+         BL       BR
```

Uniform cell art = `grid[rotation * frames + variant]`, stream-major.

| | Result |
|---|---|
| `rotation` on plain and blend cells | 0 on all |
| `grid[rotation * frames + variant]` | A drawn tile on every uniform cell, never null |
| A turned cell's block corner | A rotation-0 cell of the same column, on over 99% of turned cells |

- Row two is decisive: only 3 of Dirt's 34 and 8 of Snow's 32 columns have a
  second stream, so another meaning would hit nulls quickly.
- The rest: **Inferred**, blocks part-overwritten by construction.
- Snow: a few percent of cells sit on a group column; drawing stream 0 four times looks
  blockier than the game.
- Not quarter-turns of one tile: turning stream 0 matches streams 1–3 no
  better than not, on every group column of three body sets.

**Measured**.

## Terrain class indices are global

Nibbles index one fixed table. Bell, Book and Candle: 0, 1, 2; Day of Reckoning
0–8; Ixmil and snow freestyles 0, 1, 2, 3, 9, 10.

Every pair in the corpus:

```text
0-1  1-2      0-3  3-4      0-5  5-6      0-7  7-8      0-9  9-10
```

Five ladders, eleven classes, ten blends, all rooted at 0 = Dirt (matches
`DTRN`/`TPRT`). 9, 10 only in the expansion quest and snow freestyles. No
biome-to-biome pair in the generated maps ([terrain.md](terrain.md)).

## The generator is deterministic, up to a point

Two games on one setup differ in cell terrain by under 4%
([above](#what-a-seed-fixes-and-what-it-does-not)).

- Not a redundant encoding: `(hi, lo, case)` ≡ `(lo, hi, complement)` matches
  none of the candidate cells.

Each layer's bar ([maps.md](maps.md#the-generators-fidelity)): height, shade,
scatter placement and slot exact; class and case share the twins' ceiling,
98.79% of uniform cells, case adding no error; `variant` and `rotation`
re-rolled. A model has no re-roll, so its bar is `(100 + c) / 2` = 99.40
([maps.md](maps.md#what-it-scores)).

## The vertex shade

`MVTX +4`: diffuse light baked from the heightfield against one fixed light,
updated as the ground changes. **Binary**: baked from unnormalised mean
triangle normals against the light, not a fit.

- Level ground (all eight neighbours at the same height) reads exactly 237,
  all sizes, no exception in the corpus (18,310 such vertices in one measure).
  Shade has 233 distinct values per map, `0xff` the commonest.
  **Measured** (with a floor on how many such vertices it sees).
- Construction, day 0 → day 15 of one game: more shades change than
  heights; a handful lie more than one vertex from a changed height, and
  some are reached only diagonally. So shade is computed over a 3×3 stencil,
  diagonals included; no blur. Level on its four axis neighbours only, a
  vertex does not always read 237 (**Measured**).
- Off level: Lambert, light 45° round and ~45° up toward `+x +y`, the same in
  every save within a couple of degrees.
- **Binary**: each cell splits into two triangles by `(x ^ y)` parity; a vertex
  averages the unit normals of the triangles touching it (eight or four) without
  renormalising, dots with the normalised `(LightX, LightY, LightZ)` times
  `Contrast`, and writes `trunc(dot * 256)`, clamped. Level ground gives
  `trunc(0.9267 * 256)` = 237. `Ambient` is applied at draw time. The last
  vertex row is never lit. 99.97% of the corpus matches.
- Array `+x` right, `+y` down (a save of a hilly map), so the sun is lower right and
  shadows fall upper left. `data::settings::LIGHT`.
- The arithmetic is open.

## The projection: an orthographic camera at 55 degrees

**Binary.** `IsoCameraAngle` is `GEINIFile` key 34 in `MajestyHD.exe`, its
static default 55; no shipped `GOPT` sets it. `MaxSlope` is key 35, read by
the same function. `no_shipped_settings_file_sets_the_camera_pitch`.

```text
SetCameraAngle(a):  cos(a deg), sin(a deg)
SetScale(s):        t = s * 32.0
                    depth  = sin(a deg) * t
                    height = cos(a deg) * t
```

- 32.0 = tile size. A parallel 16.16 path has its own π, 180 and a mod-256
  sine table. **Inferred**: drawing uses that, setup this.
- At 55°, scale 1: sin term 26.21 px, cos term 18.35 px. Ground rises
  `cos(55°)` = 0.5736 cells per height unit; foreshortened by `sin(55°)` =
  0.819. A cell is 32 × 26.2 px.
- `scale` is the zoom, two levels: 1 regular, 0.5 out, so no single pixel
  constant fits.
- The depth term is read at four sites, each dividing a screen distance by it
  to get cells: depth foreshortening. The other is the height term by
  elimination, read beside the first two by the unit and overhead paths. The
  ground uses a matrix
  ([below](#the-ground-is-a-triangle-mesh-cut-in-a-checkerboard)).
- Dividing by a constant = orthographic (perspective needs depth). The ground
  path has a homogeneous divide, so there ortho rests on measurement (under a
  pixel over 83 cells, below).
- Ground is real geometry: triangles at vertex heights through a rasteriser
  (`CYSoftwareShader`, `_555`, `_565`, `_AccurateColor`). Straight lines in a
  flat viewer bend in the game.
- Sprites are screen-aligned quads at a ground point lifted by height —
  billboards, since the camera never rotates. `TILE`/`IMAG` art per facing, not
  3D models. `IsoDrawList` depth-sorts; `IsoFootprintRectManager` places
  building bases.
- The minimap has its own projection.

`data::settings` (`ISO_CAMERA_ANGLE`, `TILE_PIXELS`),
`the_camera_pitch_gives_the_ground_its_relief`.

### North is up the screen

| Link | Source |
|---|---|
| compass → board | **Game.** `LowLevel.gpl` spawns monsters at a map edge; north is −y ([art.md](art.md#which-way-a-slot-faces)) |
| board → screen | **Viewer.** Below |
| direction → art slot | **Corpus.** Direction out of 32, spread over the set's slot count |

**Viewer.** A save of a hilly map and a native 1920×1200 capture of the same moment
(the `MajXPrefs` resolution). Five name plates:

| Building | Board | Plate (px) |
|---|---|---|
| `ABM1` Wizards Tower #2 | (11.0, 96.5) | (560, 258) |
| `ABY1` Wizards Guild | (8.5, 101.5) | (451, 535) |
| `ABG1` Library #1 | (6.0, 110.0) | (395, 726) |
| `ABJ1` Palace | (19.0, 110.0) | (836, 871) |
| `ABM1` Wizards Tower #1 | (10.0, 120.0) | (528, 943) |

- Screen order across and down equals board order: `+x` right, `+y` down. The
  other seven turns/mirrors swap a pair. Order only; plate offsets depend on
  building height.
- Check a capture is native: an untouched 565 capture has red and blue on
  multiples of 8, green on multiples of 4. Rescaling keeps order, not distance.
- The minimap agrees: viewport lower left with the camera over `x = 6..19`,
  `y = 96..120` of 128.

North is up; direction 0 is up. [`content::terrain::compass`](../../crates/regent_core/src/content/terrain/compass.rs),
**Measured**.

The `Info` bitfield names `NotVisibleInISOView` and `NotVisibleInOverheadView`;
this is the iso view. The overhead view's frame is unchecked.

### `scale` is 1, and a thing stands on the ground

Settles `scale` and the height coefficient. Pictures: the 1920×1200 screen,
and the whole 128-cell map stitched, 4,096 wide = 128 × 32 (a single screen
matches it pixel for pixel).

**Viewer.** Each specimen is one picture drawn twice, measured as a difference:
no origin, no anchor.

| Picture, drawn twice | Board apart | Screen apart | Lifted predicts | Flat predicts |
|---|---|---:|---:|---:|
| The two Wizards Towers | 23.5 cells, 3.754 height | 685 | 684.9 | 616 |
| Two `IG08 pine_trees_med`, max height gap | 26 cells, 2.041 height | 719 | 719.0 | 681.5 |
| Same pine, max distance across | 23 cells | 736 | 736.0 | — |
| `PG08 grey_rocks_med`, max across, stitched map | 83 cells, −1.790 height | 2,656 across, 1,093.3 down | 2,656.0 and 1,094.3 | 1,127.2 down |
| Same rock, max down | 81 cells, −0.903 height | 640 across, 2,138.8 down | 640.0 and 2,139.8 | 2,123.2 down |

- `scale` = 1 at normal zoom: 83 cells = 2,656 px exactly. Free fit over the
  pines: 32.00 px/cell at 55.0°, 0.00 px rms. Fourteen rock copies each within
  one pixel.
- The towers are the best pair: identical art, plateau heights (buildings
  flatten).
- A pair needs a height gap: the outermost pines (0.823 apart) put flat only
  15 px out, the towers 69.

**Measured**: the distances, not the fit.

### The ground is a triangle mesh, cut in a checkerboard

**Binary.** One function draws a ground cell, called for each visible cell:

```text
for corner in 0..4                       corner n is (x + n%2, y + n/2)
    h = vertex height                    16.16 in memory, times 1/65536
    clip = (X, Y, h, 1) * matrix
    sx = (clip.x/clip.w + 1) * vw * 0.5 + vx
    sy = (clip.y/clip.w - 1) * vh * -0.5 + vy
```

- World point `(x, y, height, 1)` in cells: one height unit = one cell width.
  The sprite path applies the same matrix, so sprites and ground share the
  height coefficient, and the measured lift
  (`regent_core::data::settings::RELIEF`) holds for the ground.
- The cut is chosen by bit 0 of `x ^ y`:

| Cell | Triangles | Shared edge |
|---|---|---|
| `(x ^ y)` even | `(0, 1, 3)`, `(0, 3, 2)` | `0`–`3`, main diagonal |
| `(x ^ y)` odd | `(0, 1, 2)`, `(1, 3, 2)` | `1`–`2`, anti-diagonal |

  A fixed diagonal is wrong on saddles (half the map), seen as patches of wrong
  steepness. `regent_core::view::map::cut::Cut`;
  `a_ground_cell_is_cut_on_the_diagonal_its_parity_picks`.
- Shading: each corner's shade is Gouraud interpolated over the same two
  triangles. Bilinear agrees only on the edges and shows a diagonal seam, even
  at 1:1. `regent_core::view::raster::lit::ground`;
  `a_cells_shade_is_gouraud_over_the_cut_its_parity_picks`.
- A cell with no explored corner (flag bit 0) has its shades halved, not
  blacked out.

### `MaxSlope` is a ground clamp, not a lighting value

Sits beside the five `GOPT` lighting values but is geometry.

**Binary.** The angle is looked up in 256-entry 16.16 sine and cosine
tables, and `tan(MaxSlope)` is kept on the camera.

- `MaxSlope` 45 does not give 1.0. Degrees convert to a table index that,
  for 45°, falls just under 32, so index 31: `sin[31]/cos[31]` = `0xf3ba` =
  `tan(43.59°)`. 46° reaches index 32 = 1.0. `sim::mapgen::rim::tan_of`,
  `the_slope_limit_is_forty_five_degrees_off_a_coarse_table`.
- The iso map adds the camera's `tan` to the base map's clamp: the limit acts
  on the ground, not shading.
- The clamp, `(x, y, tan, out rect, force)`, is a flood fill. It pops a
  queue, walks the 3×3 (x outer, y inner, diagonals included), pulls any
  neighbour more than `tan` away to exactly `tan`. A visited bit in the
  vertex flags keeps each vertex queued once: one wavefront. Touched
  vertices grow the returned rect.
- `force` clear → only moved neighbours are queued, the fill dies a step past
  the edit (building flatten, and per rim vertex); set → every unseen
  neighbour is queued, the whole grid.
- The building flatten averages the height under every covered half cell
  (a polygonless building's rect half open, one short of its paint), writes
  it back, and floods the clamp over the 3×3 neighbourhood; the average is
  truncated, so the model works in integers.
- The generator uses force once, at the end of the heightfield pass, before
  pinning the rim rows: the whole map is clamped from the corner while the
  corner holds the sentinel.
- Units: raw 16.16 height difference vs `tan` in 16.16, one cell of run —
  the drawer's `1 height unit = 1 cell`. Diagonals (√2 away) get the same
  limit, so the clamp is stricter diagonally.

**Corpus.** No adjacent pair in any save exceeds the limit:

| | |
|---|---|
| Steepest adjacent step | `0.9520569` (`0x3f73ba00`) = `tan(MaxSlope)` |
| Pairs above it | 0 |
| Pairs at it on a hilly map | some |

With the flood modelled, `sim::mapgen` leaves 0 over-limit vertices and every
converging heightfield is exact. `sim::slope::Spread`,
`RIM_STEP`, `RIM_MARK`,
**Measured**.

## The rim is pinned, and carries two marks

Every save, all sizes. The heightfield is a bowl (mean height
rises each ring inward) inside an asymmetric edge:

| Where | Height | Shade |
|---|---|---|
| Row 0 | 0 | varies (lit normally) |
| Row `side` | 0 | 0 — never produced by lighting, so never written |
| Columns 0 and `side` | ordinary, low | ordinary |

- `(0, 1)` and `(1, 1)` hold `RIM_MARK`, nowhere else on any map, across size,
  seed and landscape: written, not computed. Purpose unread.

| | Raw 16.16 | Float |
|---|---|---|
| `RIM_STEP` | 62,394 | `0.9520569` |
| `RIM_MARK` | −3,142 | `-0.0479431` |
| Sum of magnitudes | 65,536 | 1.0 |

- First rim rows are multiples of `RIM_STEP`, sometimes minus 3,142 (59,252,
  121,646, 124,788, 184,040, 187,182). Only on the rim (the interior is
  continuous): a rim construction, not quantisation.
  **Measured**.

## The chunks keyed by a handle

`ORDR` and `CONT`: `count` runs of a bare `u32` then a whole chunk.

```text
ORDR                    top level, count = n
  u32   handle          key, not a header
  ORDR                  entry
    PRIM  ...           u32 words
  u32   handle
  ORDR ...
```

**Corpus**: on both chunks of every save the walk consumes the content and
yields `count` entries.

The key is a handle. Handles are dense from 1, so any number resolves; the
evidence is the distribution. Scatter is nearly every object, yet `CONT`
names none, and `ORDR` keys far fewer than its share; buildings and mobiles
are over-represented.

- `CONT` names no scatter (96.5% of objects); `ORDR` keys all three kinds of object, 30:1
  towards buildings and mobiles.
  **Measured**. A few
  `ORDR` keys name no object (asserted as a ceiling).
- `ORLS`: `size == 4 + 4 * count`, a leading word then one `u32` per entry; the
  `ORDR` handles reordered, a permutation on nearly every save. Two played saves
  list one more: a goblin (`BVL1`, `BVL2`) with no `ORDR` entry,
  why unread (**Measured**).
- An `ORDR` entry is its unit's three order tracks
  (**Binary**, reader and writer): an entry holds three tracks, 0 the walk, 1
  every order, 2 an action's turn; an empty track is `1, 0`, written by the unit
  itself. A track is its orders front to back:

  ```text
  per track   u32 1, u32 n
  per order   u32 1, u32 type, then the order class's body:
    header    u32 2, type, rate, when, delta, info, subType, userData
    callback  cProcID, scriptID, 16 zero bytes, 32-byte name
    fields    per class, after the header
  ```

  Fields, by class: `MovePath` info (bits `0x8000` first, `0x400` any,
  `0x2000` after a wait), target handle, point, index, plans, waits,
  nodes; `Turn` the wanted facing; `Move` info (`0x4000` overshoot), goal,
  step; `Action` code, target handle, point, state (bit 2 armed);
  `InitAction` an `Animate`, code, target, bit 0 fired; `Animate` flags,
  part; `Counter` start, end, count, loops; `CombatSignal` count; `Delayed`
  a word, 1 armed; `Earthquake` min, max, radius, runs; `MoveMissile`
  target handle, goal, step, shooter, offset; `PhysicsMissile` start,
  velocity, gravity, bit 0 falling, the point it falls on, shooter.
  **Corpus and traces**: every entry parses to its end (**Measured**).
  Seen: 5 `Animate` (flags `0x50`), 9 `Wait` (`0x100d`), 10
  Action, 12 a flag's wave (`0x2002`), 13, and the GS orders `0x4001`
  `CombatSignal`, `0x4002` a recruit (`0x2006`), `0x4003` rubble
  (`0x2007`), `0x4005` research (`0x2009`). A building stands in `ORDR`
  while its set loops or a GS order runs: in every trace and corpus save
  a building's entry holds an `Animate` exactly where its set has a
  layer of more frames than one (**Measured**).

  Header fields: `rate` in game ms; `when` the game ms of the last run or of the
  push; `delta`: the order runs when `now - when >= delta`; `info` bit 2 runs it on
  push. The callback is 56 bytes. Every body opens with its own version word, 1
  (Move and Turn 2, MovePath 5); the entry wrapper is version 1, the header 2.
  The engine's own order types: `0x4000` `FollowClone` (a speed trail's),
  `0x4001` combat signal, `0x4002` recruit, `0x4003` rubble, `0x4004` market day,
  `0x4005` research, `0x4006` wizard-tower enchantment, `0x4007` gamble, `0x4008`
  earthquake, `0x4009` embassy recruit. Type 8 is built as 5.
  Bodies after the header: `Turn` a word, a point, the direction, four words;
  `MoveMissile` a move, the shooter's handle, a point; `Wait` none; `Counter` five
  words; `Fade` target, rate; `Tracker` its version alone, no header;
  `FollowClone` two words, a point, three words; a combat signal four words; a
  recruit, rubble, market-day, research, enchantment or gamble order one word;
  an earthquake five; an embassy recruit one. `Animate` six words (flags; the
  second 0 on an animate that steps frames; loops left, 0 on its last) and a
  callback; `FollowUnit` info, the followed handle, a word, the greatest distance,
  two words, a callback; `FollowPath` info, the next waypoint, a range, then the
  waypoints in 32nds; `MovePath` info (the avoid modes), target, point, avoid
  radius, a word, the node held, plans since the kept length last doubled, nodes a
  plan keeps, waits in a row, then the kept nodes in half cells.

### `CONT` is a bundle of lists, and one of them is who is indoors

```text
u32   1                 version
u32   lists
  u32   1               list version
  u32   count
  u32   id              which list
  u32   owner           always the entry's key
    u32  handle         count times
    u32  1
```

| Id | On | Names |
|---|---|---|
| 100 `ROSTER` | building | everything belonging to it, wherever |
| 101 `HOME` | mobile | its home |
| 2 `INSIDE` | building | what is in it |
| 1 `ATTACHED` | mobile | what hangs on it — overlay, or goblins it leads |

- Lists fill every payload exactly; each names its key as owner.
- `ROSTER` ≠ `INSIDE`: most indoors are on the host's roster; the rest are
  visits (**Measured**: heroes in `ABF1 Hynde Quarters Inn`, rangers in a Marketplace, a
  Healer and Paladin at the `ABC2` Blacksmith, tax collectors, peasants and a
  City Guard in a Guardhouse). A Palace
  rosters a City Guard fifteen cells away.

One save, resolved:

```text
ABJ3 Palace3 holds a Tax_Collector, four Peasants and a Palace_Guard
ABO1 Temple_Agrela1 holds three Healers
ABF1 Inn holds a Monk and two Rangers
ABC2 Blacksmith2 holds a Paladin and a Healer
```

**Measured**. Lists fill all 824 payloads (twenty lengths); each member's trailing
word is 1.

## `GPL ` — the script engine's state

**Measured**. Chunk
version 1, count 0. **Binary**, reader and writer. Every corpus save reads to the last
byte (**Measured**); bodies are version 3. Body versions 1 and 2 read the words in
another order and 1 has no next id; no corpus save has either.

Everything nests in blocks: `u32` size — the whole block, header and
terminator included — `u32` version, the content, then `0`, `-1`.

```text
block v3
  u32   GPL RNG state                   the state, not the seed; reseeded from it
  block v2                              the GplAIRoot
    u32 n, n × (cstr, u32)              named agents: GplAIRoot → 1, nothing else
    u32 n, n × (u32 id, agent)          every agent
    u32 n, n × u32                      scheduler queue
    u32 n, n × u32                      scheduler queue
    u8                                  1 if the first is the running one
  u32   next agent id
```

An agent, a `GplAgent`:

```text
block v2
  block v0: u32 n, n × (cstr name, u32 type, value)    attributes
  u32 n, n × cstr                     threads: its function attributes, in order
  u32   id                            equals its key
  cstr  name                          GplAIRoot on the root, else empty
  u32   handle                        the object it drives; 0 on the root
  u8    dead                          never set
  u32   unused                        0
  u8    suspended                     its threads are skipped
  u8    debug                         DebugOut while its threads run
```

A value is a `u32` type code, then its body: 0 null, nothing; 1 integer and 5
agent, an `i32`; 2 float, `i32` fixed point, 1.0 = 1024; 3 string, a cstr; 4
list, a v0 block of `u32 n` and n typed values; 6 boolean, a byte; 7 coord, two
`i32`; 8 function:

```text
u32 n, n × (u32 type, value)    call frame; slot 0 the return value
u32   interval                  clock units between runs
u32   last                      clock at the last run or the start
u32   mode                      0 idle, 1 once, 2 repeat, bit 7 suspended
i32   left                      time left when suspended
u8    running                   never set
u8 flag, cstr                   the function, if set; "" is the null function
u8 flag, cstr                   a function assigned while it ran
```

Nothing resolves at load: an agent reference is an agent id, looked up at run
time; a missing one is a run-time null. The type codes `reference`, `undefined`
and `unknowntype` never occur in a save. Attributes keep the map's order,
case-insensitive by name; an agent's threads run in the order of its thread
list. The root, built first, is id 1. Of floats, the commonest non-zero values
are 1024, 1228 and 921: 1.0, 1.2 and 0.9, truncated.

A function's mode: `NewThread` sets 2, `RunThread` and `RunThreadOnce` 1,
`KillThread` 0; `SuspendThread` sets bit 7, `ResumeThread` clears it. A thread
with mode 0 or bit 7 is skipped, and mode 1 drops to 0 after one run. A suspended
thread's `left` is `interval - (now - last)`; resuming makes `last` from it and
zeroes it. A function is run when `now - last >= interval`. Idle values are
pooled: release resets function, frame and mode, not interval and last. The
function `""` is the builtin registered under the empty name; the null record is
written `NullFunction` (**Game**: the bounty probe's embassy `ActiveScript`);
both load as that builtin. The agent's `dead` flag is never set in the corpus;
`unused` is 0.

What joins:

- Every attribute is a name some prototype declares, with that type.
  **Measured**.
- Every function value names a script function or the null one; no builtin (**Measured**).
- Every agent but the root drives an object of the same save, in `BLDG` or
  `VHCL` (**Measured**).
- An agent reference is held, none (`-1`), or gone (rare): an id
  below the next, missing at run time, which the engine resolves to null
  (**Measured**).
- The two queues hold every agent once. The next id is one above the highest
  held on nearly every save, two above on one
  (**Measured**). Each queue is written newest first; the
  scheduler takes the oldest (`runtime::Queues`,
  [gpl/runtime.md](../gpl/runtime.md#threads-and-the-scheduler)).
- The RNG state is the seed after every draw so far: played games' saves
  replay from their seeds ([gpl/runtime.md](../gpl/runtime.md#the-gpl-rng)).
- A function's `last` counts in the game clock, `GSGL` word 4
  (`GSGL_CLOCK_AT`). No active thread ran after it. Many idle ones did
  (**Measured**, 1,236 idle values hold a `last` after the save's clock):
  release resets function, frame and mode, not interval and last, so an idle
  value carries a pooled value's old times — **Inferred**, from an earlier
  game in the session.

A day-0 save — `(The Siege) 0 Days`: few of its threads are active; the
root's `KickoffFunction` has run, idle, `GplKickoff`; the quest's
`SpecialSpawnScript`, `VictoryCondition` and kin repeat, last 0. A start-up
model can match attributes, ids, queues, RNG state and active threads; an
idle function's interval and last may be a pool leftover.

## `GSGL` — the level's state

**Measured**. Chunk version 6, count 0. **Binary**, the
original's reader, which also takes versions 2 and 4: 2 stops after the seven words,
3 and 5 lack the tail; the tail is read only for 4 and 6. Words are four bytes, strings NUL-terminated and
unpadded. Every corpus save reads to the last byte (**Measured**).

```text
19 words             header: word 4 the clock, 16 the victory condition,
                     18 a quest code (the music callback)
u32 n, n × u32       a code set
u32 n, n × (u32 w)   the inventory register: key, unit handle
u32 n, n × list      hero lists: (cstr, code, level)
per player slot      version 5: pairs, palace handle, its dead heroes,
                     the record, two advisor timers, the team
u32 n, n × lair      handle, five words, a chart pattern,
                     u32 m, m × code
7 words, 3 words     level flags and words; the reward panel's
                     attack and explore settings, the music track
u32, cstr, pairs     the tail
```

- Versions: every corpus save is chunk version 6, its player blocks version 5,
  each player's record version 3 (which reads two strings; 1 and 2 two 128-byte
  buffers). An advisor timer is version 1. A player block is read for each slot 0
  to 7 the level holds, so one per `PLYR`.
- Header words: 4 the game clock, 16 the victory condition (`GetVictoryConditionIndex`'s
  answer), 18 a quest code the music callback is picked by. Words 2, 3 and 5 to 15
  are the tracking window's:

  ```text
  words 2, 3   its view's top left, pixels at its zoom
  5            the handle followed: 0 none, `none` told none
  6            the handle kept to go back to
  7            the category last scanned, 0 to 3
  8            Autoscan, 0 or 1
  9            following the palace a start gave it
  10           the filter bits
  11           Autoscan's clock
  12 to 15     each category's last scanned handle: unset words hold stale memory
  ```

  The rest are unread. The reward panel's attack and explore settings and the music
  track follow the seven level words; a music word of `-1` plays track 3.
- A hero list is one graveyard's dead heroes: a name, a guild code, a level.
  **Corpus**: one save, one group of 14. The code set is on quest saves only.
- A player block: pairs (**Corpus**: monster codes and small counts on the Warrior
  saves), the palace handle (the loader finds a palace when 0), its dead heroes not
  yet in a graveyard, the record, the elves' and dwarves' advisor timers
  (`Thread_ElvesAvailable`, `Thread_DwarvesAvailable`), and the team (**Corpus**: 0
  for a kingdom, 1 for slot 7). A record's strings are a hero's name and class
  (`Sir Lucas Silveredge`, `Warrior`) or empty; its two code lists count bought and
  gained units for numbered names.
- A lair: five words (**Corpus**: `550, 15000, 850, 5, 1000` on a size H lair;
  unread), the pattern's code and name, its two flag words, its entries (code,
  weight, four words), and the chart spawner's special codes
  (`FillSpecialLairList` names them; **Corpus**: every freestyle start's are the
  chart's). A lair with no data carries `NONE`; with no entries `SpawnUnit` takes
  the script's name. **Corpus**: the seed 1 size S game has no lair data at
  150 ms; play3's three spawners are defaults; the size H game's eight lairs have
  entries. The tail's name is the build tree's `BTDT` variant (`default` on 87
  saves), its pairs the building caps.
- A player's record: six words, two strings (a hero name and class, or
  empty), nine words, two code → count lists, bought then gained
  (`world::numbered`). The first six: built, destroyed, hired, killed. The
  nine: danger, vice (record `+0x34`), gold made, five spends, the
  initial gold; the score's peak is not saved (`world::ledger`).
- A player's advisors are the elves' and dwarves' timers, four words each:
  interval, 0, the clock last polled, on. The last run's clock is not saved.
- A player's pairs are its slain monsters, code word → count
  (`world::slain`). Its Hall of Champions bounty is in its `PLYR` words
  (`APP:2` the hall's code, 0 for none; `APP:3` the length; `APP:4` the clock
  it began); the hall's timer ends on callback `0x200c`.
- A hero list's entry spells the dead hero's name as a string, not as what
  spells it.
- A lair's data is keyed by the spawner's handle, and a lookup makes a
  default, so every unit that ever ran `SpawnUnit` is saved. Its pattern's
  entries are what `SpawnUnit` picks from, a world draw; with none it spawns
  the script's name. **Corpus**: most lairs have entries
  (**Measured**); a fresh small game has none
  at 150 ms, and a short played game's spawners are defaults.
- The lair record's shape follows a tag the reader is given (**Inferred**:
  the `MAP ` version): `RGC3` on every save.
- Level words 2–4, the hit point tick's period, poison and heal
  intervals, are `globals.gpl`'s constants, set at a new game; words 5 and
  6, the last tick's clock and the count, are 0 (`world::new_map::mend`,
  **Measured**).
- Music words 0 and 1 are the reward panel's attack and explore settings.
- The code set is the types scripts disabled (`DisableUnitType`); a quest
  save's is its quest's (**Measured**).
  The tail is the build tree: its `BTDT` variant's name and the building caps
  (`SetBuildingLimit`). A hero list is a graveyard's, by its `APB:8`
  `GravesIndex` (`SpawnGraveyard`; **Corpus**: one save's `BBJ1`, index 0).
  `world::limits::Limits` reads all three. A player block's named list is
  its dead heroes not yet in a graveyard (`gsgl::Player::dead`,
  `world::graveyard`).
- The inventory register: a pair per unit's `INV:n` key (**Corpus**,
  `tests/inventory.rs`).
- Unread meanings: most header words, a player's pairs, the record's
  words, the tail's first word.

## What a load restores

The original's single-player load. openregent does not load `.gmp` saves.
**Binary**, in order: the GPL engine is reset; the
quest's dataset is applied, then every chunk's reader runs; the level is
installed; the game starts in load mode, seeding the world and GPL RNGs from
the session word, clearing every level flag and the build cheat and taking
the saved cheats word; the view starts and deselects all.

Restored: every piece and attribute (`BLDG`, `VHCL`, `GRND`, `RWRD`,
`MSGF`, `OFMP`), keyed by agent id; the clock; the handle counter (`MAP `
word 8); the GPL chunk (agents, threads, queues, next id); `NMGM` (each
generator reseeded, its count kept); `ORDR` (walks, actions, turns,
falls, timers, missiles, quakes, a building's loop), ordered by `ORLS`;
`CONT` (who is inside); `GSGL` (palaces, teams, limits, lairs, the music
track and its stop callback, with no GPL dispatch).

Reset, as a new game: the world and GPL RNGs (the session word), the
selection (none), the two cheat flags. No setup runs: no dispatches, no
start frames.

**Measured**: a save loaded with its own RNG states, the log picked up
where the game from the start reaches the save's frame, played on to the
next save or the log's end, over a dozen traced games. Each next save's
world RNG is the loaded game's
([gpl/runtime.md](../gpl/runtime.md#every-rng-and-its-seed)). The cheat
allowance is the save's.

Not restored: speed-trail
clones (`FollowClone`), tints, the reward panel's settings, particle positions
(clock-seeded). **Inferred** not saved: the advisors' treasury and heroes
timers, sight words, GPL function activations.
