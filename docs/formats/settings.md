# `DATA` and the engine's settings

> Part of [README.md](README.md).

| Reads | Code |
|---|---|
| `GOPT` | [`data::settings`](../../crates/regent_core/src/data/settings/mod.rs) |
| `BDEP`, `QDEP` | [`data::deps`](../../crates/regent_core/src/data/deps.rs) |

Tests: `crates/regent_core/tests/settings.rs`. Print with `regent_cli deps`.

## `DATA` — four plain-text records

One `DATA` archive per dataset (`miscdata.cam`, `mx_miscdata.cam`), each with
the same four plain-text records (**Corpus**; read by
`every_shipped_dependency_table_parses`,
`the_shipped_settings_hold_the_lighting_constants`):

| Record | Holds |
|---|---|
| `BDEP` | Building dependency table |
| `QDEP` | Quest dependency table, in a different notation |
| `GOPT` | Engine settings file |
| `XML1` | `datasets.xml` ([README.md](README.md)), byte for byte |

## `BDEP` and `QDEP` — a postfix boolean language

The header comment says the tables use postfix notation. One rule per line:
four-character id, colon, stack expression over `&&`, `||`, `NOT`.
`BDEP` gates the build menu and the crown's own buildings, so a save
cannot refute a rule.

```
ABQ1 : ABJ2 ABJ3 ||          Temple to Fervus needs Palace level 2 or 3
PHG1 : ABJ2 ABJ3 NOT NOT ||  a mod's rule, same shape
```

- `ABJ2`, `ABJ3`: Palace levels. `#` starts a comment. Lines are CRLF.
- The header says the last line must be blank or the statue prerequisites fail
  to parse. This reader does not need it:
  `the_dependency_tables_do_not_need_their_blank_last_line`.
- Parsed as a stack: every operator finds its operands, one value left. Holds on
  all 160 rules of the three shipped tables
  (`every_shipped_dependency_table_parses`; the count,
  `every_building_dependency_names_a_building`).
- **Binary**: per owner, a code is absent, placed (`CurrentStageBuilt` 0) or
  finished, ruins left out. An id directly under `NOT` asks placed, any other id
  finished; operators give 0 or 1; no rule is met. So `x NOT` is "none
  placed", `x NOT NOT` "one placed". Upgrades ask no rule: no level 2 or
  3 line has one, and the upgrade button reads the `BTDT` cap table
  instead. `data::deps::Expr::holds`;
  `an_id_under_not_asks_placed_and_a_bare_id_finished`.
- Every id a rule names, its own included, resolves to a `DUNT` record — 60
  distinct (`every_building_dependency_names_a_building`). Exception: `PHG1`
  (Phantoms Haunt guild), declared in `phantom_units.xml` and compiled by the
  HD toolchain at load; the mod ships no `DUNT`.

`QDEP` adds a difficulty field:

```
QU06 : HARD QU01 QU02 && QU03 && …   difficulty, then expression
QU01 : EASY NONE                     NONE = no rule
```

- `EASY` and `NONE` are four characters, so a `BDEP` reader takes them for ids
  (`a_quest_rule_read_as_a_building_rule_swallows_the_difficulty`).
- Difficulties: `EASY`, `MEDM`, `HARD`, `MSTR`. `MSTR` only on the expansion
  finales `XQ08` and `XQ12`, each needing every other expansion quest
  (`the_expansion_adds_a_difficulty_the_base_game_never_uses`).
- The expression lists quests to finish first. **Game**: Elven Treachery needs
  Bell, Book and Candle, The Forsaken Land, Rescue the Prince:

```
QU10 Elven Treachery  Medium  ((QU02 Bell, Book, Candle && QU12 Forsaken Land) && QU15 Rescue the Prince)
```

Quest names: `STRT QUES`, keyed by the `QDEP` codes. `gpltext.cam` has 20 (the
base game's 20 rules); `mx_gpltext.cam` more (adds `XQ01`…`XQ12` and bonus
quests no rule names). `every_quest_id_is_named_by_the_ques_string_table`.
`regent_cli deps --resolve` prints rules in English.

### `DPTX` says the same rules in English

`STRT DPTX` in `gpltext.cam` is `BDEP` in prose, keyed by `DUNT` id: `ABB1`
"requires a Dwarven settlement" beside `ABB1 : ABb1`.

| `DPTX` says | Means |
|---|---|
| no prerequisites | Always buildable |
| "none (not user built)", "none (Can't be built)" | Never in the build menu |
| anything else | The condition in English |

- Unbuildable: Palace, Sewers, Brothel, Gambling Hall, General Housing. None has
  a `BDEP` condition.
- One disagreement: `ABH1 Marketplace1` needs a Palace by rule, no
  prerequisites in prose. Every game has a Palace, so the condition is vacuous.
  `the_prose_agrees_with_the_rules_and_names_what_is_never_built`.

### A rule governs the build menu, and nothing else

A file comment says `ABJ3 NOT NOT` means the building exists, finished or not.
Three rules name themselves (`regent_cli deps`, infix):

```
ABf1 needs (((ABJ1 && !ABb1) && !ABc1) && !ABf1)     Gnome Hovel: no Gnome Hovel
ABo1 needs ((ABJ3 || ABm1) && !ABo1)                 expansion
ABq1 needs …
```

- `NOT` is a plain complement over "is this present". Temples resolve to the
  gods rule (Agrela needs no Fervus, Krypta or Krolm); `ABf1` naming itself
  means one per kingdom.
- **Game**: the player builds one Gnome Hovel; two more appear once it stands
  and rebuild after destruction.
- A rule is a condition at build time, not a map invariant. Starting scenarios
  grant buildings the rules forbid together, and a prerequisite can be knocked
  down later. `BDEP` guards the crown's menu only.
- No cap by default: each extra building costs more. **Corpus**, no test:
  only two caps exist in the data: `ABf1`'s self-reference and `BTDT`'s *One of a Kind* variant. Dwarven
  Settlement and Elven Bungalow exclude each other, not themselves, so are not
  unique.

Standing buildings whose own rule fails, over the corpus, by owner
(**Measured**; owners in
[saves.md](saves.md#word-5-is-the-owning-player)):

| Owner | Kinds |
|---|---|
| 0 (crown) | `ABO1 Temple_Agrela1`, `ABY1 Wizards_Guild1` — prerequisite no longer standing |
| 7 (neutral) | `ABS1 Temple_Krolm`, `ABb1 Dwarven_Settlem`, `ABf1 Gnome_Hovel` |
| 1 and 7 | `ABc1 Elven_Bungalow` |
| 1 (rival, The Siege) | `ABD1 Fairgrounds`, `ABQ1 Temple_Fervus1`, `ABh1 Royal_Gardens`, `ABk1 Statue` |

`ABb1`, `ABc1`, `ABf1` exclude one another and
are granted together at a start.

## `GOPT` — the settings file

```
LoadCAM = ACTION.CAM ... VOICES.CAM      14 archives, in load order
GplPath = bytecode.bcd
GplsPerFrame = 30
GplPseudorandom = 1
UsePlayerColorPaletteRange = FALSE
ScrollSpeedLow = 32          ScrollSpeedHigh = 64
PixDepth = 16   PixWidth = 800   PixHeight = 600   PixExclusive = TRUE
LightX = 0.90   LightY = 0.90   LightZ = 1.0
Contrast = 1.5  Ambient = 0.35  MaxSlope = 45
```

Tests: `the_shipped_settings_hold_the_lighting_constants`,
`every_archive_the_settings_load_is_on_disk`.

- Player-colour palette range is off, so the data cannot place it
  ([payloads.md](payloads.md)).
- `LightX/Y/Z`, `Contrast`, `Ambient`: the terrain renderer's whole lighting
  configuration.
- `MaxSlope` is geometry, not lighting: a ground slope clamp. A coarse trig
  table makes 45° a tangent of 0.9520569, not 1.0. See
  [saves.md](saves.md#maxslope-is-a-ground-clamp-not-a-lighting-value).

### The expansion's copy

`datamx/mx_miscdata.cam`: `LoadCAM` reduced to `XQD1_INTRO.CAM`, `GplPath`
dropped, paths lowercased, four keys added, one changed. 25 distinct keys in the
base game's, 28 here (`every_key_a_shipped_file_sets_is_one_the_engine_knows`).

| Key | Value | Bears on |
|---|---|---|
| `BuildTreePath` | `buildtree` | Directory scanned for `*.BTD` build-tree files |
| `FreeStyleIOPath` | `freestyle` | Where the freestyle settings' `free<name>.STY` files are read and written ([panels.md](panels.md#load-and-save-freestyle-options)) |
| `ParabolaAngle` | `75` | Lobbed projectile's launch angle; binary default 60 |
| `ParabolaGravity` | `4` | Its gravity; binary default 8 |
| `GplPseudorandom` | `0` (base: `1`) | Per-frame reseed of the world RNG |

Tests: `only_the_expansion_configures_a_build_tree_and_a_freestyle_path`,
`the_expansion_turns_gpl_pseudorandom_off_and_the_base_game_leaves_it_on`.

- **Binary**: HD reads only this `GOPT`, so HD runs with `GplPseudorandom` 0
  and the parabola values above (`settings::PARABOLA_ANGLE`).
- **Binary**: with `GplPseudorandom` set, the frame loop reseeds the world RNG
  with frame + 1 every frame, and the GPL RNG is seeded from the frame too, and
  with 1 at a new game. Name generators keep the session's clock seed
  ([gpl/runtime.md](../gpl/runtime.md#every-rng-and-its-seed)). One byte of
  `mx_miscdata.cam` turns it on.
- No `buildtree` directory in the install. **Binary**: the game lists `*.BTD`
  there, under the user directory, for the build-tree editor and freestyle
  options; each is a `BTDT` record
  ([payloads.md](payloads.md#btdt--build-tree-variants)).

### The keys that exist, against the keys anybody set

**Binary.** A `GOPT` lists keys somebody set; the executable lists all keys.
Each settings class's keys are one contiguous string run before its RTTI name.
The tables are in
[`regent_core::data::settings::keys`](../../crates/regent_core/src/data/settings/keys.rs),
checked against the shipped files by
`every_key_a_shipped_file_sets_is_one_the_engine_knows`.

| Run | Read by | Keys |
|---|---|---|
| 1 (`GAME`) | `GSINIFile` | 71 |
| 2 (`DATASET`) | dataset XML reader | 8 |
| 3 (`ENGINE`) | `GEINIFile` | 39 |

The base `GOPT` sets most of its keys in run 3.

- 118 entries, 116 distinct. In two runs: `CDVolume` (`GSINIFile`,
  `GEINIFile`); `LoadCam` (dataset, and `GEINIFile` as `LOADCAM`).
- Run 1 is two tables: the first 37 keys have parser cases; the rest are
  persistent settings with defaults and no case.
- Growing a run outward overshoots by three runtime strings and two parser
  values, so the tables are pinned, not grown.
- The expansion (`EXPANSION_GAME`, 77) = base + 6: `MajestyInstallPath`,
  `BuildTreePath`, `FreeStyleIOPath`, `ExpansionDataPath` after
  `CYLibGarbageCollectionLRULimit`; `RGS_SpecialEvent1`/`2` appended.
- HD appends 13 (`HD_EXTRA`), in binary order: `PixWidth`, `PixHeight`,
  `PixDepth`, `Windowed`, `DebugFlags`, `VSync`, `BlitMode`, `IntroVideo`,
  `UseD3D`, `UseGPLDebugger`, `UseDebugWindow`, `SDKPath`, `MapDrag`. The three
  `Pix*` are `GEINIFile` keys, here given a second home in HD's
  game-side table.
- Both HD builds also insert two parser keys, `DirectShowDecodingMethod` and
  `LocalPath` (`keys::HD_INSERTED`), after `CYLibGarbageCollectionLRULimit`.
- The game-side table serialises itself, so HD writes resolution back.
- `MajestyHD - Old.exe` has the first eight of the 13, none of the last five:
  it predates the SDK, GPL debugger and Direct3D path.

The two shipped `GOPT`s set few of the keys; most are set by nothing.
`GAMEOPT.INI` is a filename the engine knows; nothing writes it.

### What each key does in HD

**Binary.** Per key, in
[`settings::cases`](../../crates/regent_core/src/data/settings/cases.rs): parser case,
the global or function it writes, and the file's value for that global.

```text
GOPT (mx_miscdata.cam only) ─► game table ─ no pair ─► engine table
                                  │ case                  │ case
                                  ▼                       ▼
                              game globals           engine globals
registry records 0–37   ─► globals
MajXPrefs records 38–50 ─► override Pix*, set switches
```

- **A key's case is paired with it.** `(key, case)` pairs, not positions:
  `IsoCameraAngle` is position 18, case 34, in HD. **Inferred**: the two pitch
  keys were numbered on the end and listed beside the lighting. Four game keys
  are swapped the same way.
- **A key in both tables stops in the game table.** HD's `GOPT` `LocalPath`
  hits the game case, which drops it; `QuestDirectory` has no case;
  `SaveGamePath` is dropped. So those three `GOPT` lines do nothing in HD.
- **No case in HD:** `HeightThreshold`, `OdbcPath`, `ConstructBuffer` (engine),
  `QuestDirectory`, `ReportIfFrameLongerThan` (game). Parsed and thrown away:
  `MaxCYLibs`, `CDAudio`, `MajestyInstallPath`, `ExpansionDataPath` (logged).
- **Set, never read:** `PixAlloc`. No reader found: the three
  `Default*Volume` and the `GOPT` copies of `AudibleVolumeLimit`,
  `MaxConcurrentSounds`. The live volumes are registry records.
- **Binary defaults a `GOPT` overrides:** `MaxSlope` 55 (set 45),
  `UsePlayerColorPaletteRange` 1 (set `FALSE`), `Ambient` 0.4 (set 0.35),
  `PixWidth`×`PixHeight` 640×480 (set 800×600).
- **Unset, so the binary decides:** `IsoCameraAngle` 55
  (`no_shipped_settings_file_sets_the_camera_pitch`);
  `DifficultyLimit*` 0–39, 40–79, 80–100, the bands a Random map pick accepts
  (`settings::DIFFICULTY_BANDS`, `the_difficulty_bands_are_the_limits_file_values`);
  `PathHorizon` 1024, `RepathLength` 10; `CollisionDelay` 1000, `ExpandCount` 4
  (pathing); `GPLLinesPerFrame` 1500; `FramesPerChecksum` 200; `TimePerFrame`
  50, `ExecutionWindow` 10, `BundleFactor` 2, `TossPlayerAfter` 60000
  (`GSMessageManager`); `SoundViewRadius` 1.5.
- **`PlayerColorPaletteRangeStart`/`End`**: fields of the `GEPlayerColors`
  singleton, zero unless set. The range is `End - Start + 1` palette indices,
  ramped from the player's colour. **Inferred**: the indices a player colour
  recolours.
- **`UseGPLDebugger`** equals `-gpldebug`; with it the loader mounts the SDK's
  `OriginalQuests/Data` and `DataMX`. `UseDebugWindow` = `-debugoutwindow`,
  `UseD3D` = `-d3d9`, `Windowed` = `-w`, `BlitMode` 0/1/2 =
  `-useglblit`/`-useddblit`/`-dib`.
- The dataset reader has no case table: it compares XML element names in turn.
