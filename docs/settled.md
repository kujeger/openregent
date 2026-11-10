# What is settled
One line per closed question, with a pointer. Read before calling something
unknown. Numbers and detail live in the linked document and
the code. Open: [status/open-questions.md](status/open-questions.md). Evidence markers:
[README.md](README.md#evidence).

## What the game is

**Game**: the player never controls a character; heroes are autonomous. The whole input vocabulary:
build and upgrade buildings, research, recruit and dismiss heroes, cast unlocked spells, place rewards, set tax
and repair and a tax collector's amounts. Anything like a per-unit order is the simulation's own
decision. Player intent is written in `RWRD`, the reward flags, and those settings' attributes.

## The binary

| Fact | Where |
|---|---|
| RTTI names the classes: MSVC in the Windows PEs, with a hierarchy | **Binary**, [original/architecture.md](original/architecture.md) |
| 118 settings keys in three tables; a parser pairs each key with its case. HD reads only the expansion's `GOPT` | [formats/settings.md](formats/settings.md), `data::settings::keys` |
| The opcode table holds names only; the VM runs 43 opcodes, 39 as `opNop`, 40–42 inlined attribute access no `.bcd` uses | [formats/bytecode.md](formats/bytecode.md), `opcode::INLINE_ATTRIBUTE` |
| Each recorded struct field cites an instruction that reads it. A `_purecall` slot is a pure virtual | **Binary** |

## Containers

| Fact | Where |
|---|---|
| `.cam` is read whole; tile numbers are archive-local; a later archive overrides by record name, an empty record too | [formats/README.md](formats/README.md#no-archive-is-sparse), `data::cam` |
| `.gmp`: one chunk shape, fixed order, a `PLYR` per player; `SVGD` names the save; `GSGL` reads to the last byte | [formats/saves.md](formats/saves.md) |
| `.bcd` framing and its three sections; `.rgs` and `.q` hold three coexisting quest-map versions | [formats/bytecode.md](formats/bytecode.md), [formats/maps.md](formats/maps.md) |
| Mods: an `.mmxml` holds `<Mod>`s by GUID, composed after the quest's lists if the base fits; a save lists them; network games play none; XML descriptions become archive records | [formats/mods.md](formats/mods.md), `content::mods`, `desc::xml` |
| `.qdd`, `datasets.xml`, `DATA` are text. `BDEP`/`QDEP` are postfix boolean, gating the build menu; no default building cap | [formats/settings.md](formats/settings.md), `data::deps` |
| A keyed `STRT` names what a save numbers; text markup is a colour only; `FONT` a bitmap; music MP3; `PICT` Bink videos decode bit-exact | [formats/payloads.md](formats/payloads.md), `data::text::Text`, the `bink` crate |
| `SMNU` panels are blocks of keys, each key's width from the binary's skip table; key 33 is the tooltip | [formats/panels.md](formats/panels.md#the-front-end), [tooltip](formats/panels.md#the-tooltip-line), `data::smnu` |

## Art

| Fact | Where |
|---|---|
| `TILE` 1–3 and `IMAG` decode everywhere; `+0x0e` is the draw mode; indices 247–254 shade what is under, so shadows stack | [formats/art.md](formats/art.md), `tile::Mode`, `view::raster::stack` |
| Draw modes 5 and 8 are translucent; `SetDrawEffects` draws mode 5 or a palette tint | [formats/art.md](formats/art.md#modes-5-and-8-are-translucent), `view::raster::blend` |
| A tile's anchor is where the unit stands; attachment points, footprint block, pass, kind and cue bits are read; dragons fly by code | [formats/art.md](formats/art.md), `imag::Pass`, `Shown::aloft` |
| A repeated set is a stack of layers, the id's top byte its counter; only the executable says what a picture is for | [formats/art.md](formats/art.md), `a_pose_offset_places_a_stacked_layer` |
| HD's quest map is `SMNU` `APdb`: 34 slots, `QDEP` locks, victories in one `questdata_majx.sav` | [formats/panels.md](formats/panels.md#the-quest-map), `view::menu::quest_map` |
| The selection panel and unit names (`NumberedName`, by owner) are read; a start selects the palace, a load nothing | [formats/panels.md](formats/panels.md#the-selection-panel), `view::panel::portrait` |
| Selection ring, hit point bar, flag and name labels, cursor `CUR1`, the pick | [formats/art.md](formats/art.md#the-selection-ring-is-a-sprite), `view::map::ring`, `view::map::bar`, `view::map::label`, `view::map::hit` |
| A game day is 60,000 ms; the top bar counts days and seconds; the save and load dialogs are read | [day](formats/panels.md#the-day-counter), [dialogs](formats/panels.md#the-save-and-load-dialogs), `view::panel::day` |
| A frame shows its set's playback ms, at least 50; game speed holds a frame 10–250 real ms, default 50 | [formats/art.md](formats/art.md), [a step](original/movement.md#a-step), `runtime::PACES` |
| The spell bar `AP92`: 22 buttons, 28 in the expansion; only the Abode's spells are the expansion's; gold greys none, a short map click says `GMTX` 34 | [openregent/game.md](openregent/game.md#the-game), `view::panel::spell_bar`, `orders::cast` |

## Descriptors

| Fact | Where |
|---|---|
| The `D*` tree and every unit, building and action field are read; `Flags` differs per subtype; `CanUse` is filled at load | [formats/payloads.md](formats/payloads.md), `tests/game_files.rs`, `desc::Unit::can_use` |
| `PRIM`'s tail, `DUNT` populations, `DSND`, `ParticleSystem`, `BTDT` are read; a `.BTD` file is a `BTDT` record | [formats/payloads.md](formats/payloads.md), `data::build_tree` |
| `DTRN`'s `TPRT` maps terrain classes to `MapTexture` letters | [formats/terrain.md](formats/terrain.md) |

## The map

| Fact | Where | In code |
|---|---|---|
| Terrain lives on vertices; 17 blend cases; a save's cells invert to that field | [formats/terrain.md](formats/terrain.md) | `content::terrain::schema`, `terrain::ground::Ground` |
| `MTER` sizes and class ladders; vertex bit 0 is explored; the fog draws unexplored black, unseen units hidden | [formats/terrain.md](formats/terrain.md), [bytecode.md](gpl/vm.md#explored-ground) | `terrain::grid::Terrain`, `view::map::fog` |
| Vertex shade is baked from heights; the ground is lit by table and matches captures pixel for pixel | [terrain.md](formats/terrain.md#the-colour-on-screen), [formats/saves.md](formats/saves.md) | `sim::light`, `view::raster::lit` |
| The view is orthographic at 55° pitch, a cell 32 px; a ground cell is two triangles cut by `(x ^ y)` parity | [formats/saves.md](formats/saves.md) | `view::map::pick`, `view::map::cut` |
| The minimap `APMK` matches the captures pixel for pixel; the tracking window is read | [minimap.md](original/minimap.md), [tracking](formats/panels.md#the-tracking-window) | `view::panel::minimap` |
| `MaxSlope` is a slope clamp flooded before the rim is pinned; 45° gives `tan(43.59°)` | [formats/saves.md](formats/saves.md) | `sim::slope`, `rim::tan_of` |
| The generator is diamond-square, filling sentinel vertices only; `sim::mapgen` reproduces all eight charts from the picks | [formats/maps.md](formats/maps.md#the-generators-fidelity) | `sim::mapgen::world`, `sim::mapgen::field` |
| Zones are carved, not tiled: one fractal per height profile, re-seeded; codes resolve in the quest's zone map | [formats/maps.md](formats/maps.md) | `rim::pin`, **Measured** |
| The generator's RNG is a masked LCG with a scramble: a rho, not a cycle | [formats/maps.md](formats/maps.md#the-rngs-orbits) | `base::rng` |
| `Freestyle.q` is the custom-game dialog; Wandering Monsters places nothing | [formats/maps.md](formats/maps.md) | `the_monsters_pick_is_a_third_force_that_places_no_block` |
| The freestyle config lives the session; the registry keeps it but the Map Number; its dialogs are read | [panels.md](formats/panels.md#the-freestyle-pages) | `menu::freestyle::Custom::kept` |
| Map Number is the seed (0 and 1 one map); Enemy Forces sets map size; Random picks stay tags | [formats/maps.md](formats/maps.md), [panels.md](formats/panels.md#the-freestyle-pages) | `sim::freestyle::random`, `sim::mapgen::preset` |
| Placements are blocks of a 5×5 grid turned by the first draw; off-map lairs enter by an edge | [formats/maps.md](formats/maps.md) | `rgs::map::Edge` |
| The region chart grows by neighbour runs; a map edge is easier | [formats/maps.md](formats/maps.md) | `sim::mapgen::region` |
| Ground takes two picks: surface pattern, then terrain pattern; a weighted pick takes `>=` | [formats/maps.md](formats/maps.md) | `sim::mapgen::surface`, `sim::mapgen::pick` |
| A scatter object's grid is `rest[2]`, its walk stopping at its cap; the landscape chart is exact, pinned by inverting a save | [formats/maps.md](formats/maps.md) | `sim::mapgen::mobject`, `shut_gate` |
| Dirt is a fallback, not a band: a pair with no blend tile cascades `GR00` | [formats/maps.md](formats/maps.md) | `sim::mapgen::paint` |
| A footprint is read from the art. A placed building paints bare ground over its art's polygon; destruction never restores it | [formats/maps.md](formats/maps.md) | `sim::footprint`, `sim::placement`, `world::building_ground` |
| The unit chart's start places, draws and levels every corpus start building | [formats/maps.md](formats/maps.md#the-unit-chart-names-the-buildings-a-save-stands-up-and-the-start-places-them) | `sim::start` |
| HD played a generated map written over a template save; openregent writes no `.gmp`; a map read into arrays reads as its bytes | [formats/saves.md](formats/saves.md) | **Measured** |
| A seed fixes the ground, not its paint: variant and quarter take a clock-seeded RNG | [formats/saves.md](formats/saves.md) | `sim::mapgen::animate` |
| A model's expected bar is (100 + twins) / 2 = 99.40%, not the twins' share; each layer's bar lives in code; no save is raw generator output | [formats/saves.md](formats/saves.md) | `Tier::bar`, **Measured** |

## Placed things

| Fact | Where | In code |
|---|---|---|
| `BLDG`, `VHCL`, `GRND`, `RWRD`, `MSGF` share one record shape: a C++ object dump plus keys | [formats/saves.md](formats/saves.md) | `world::building_ground::Stand` |
| Every attribute key, activity and item is named from the binary's registrations | [formats/saves.md](formats/saves.md) | `data::attrib`, `data::stat::Stat::ALL` |
| A freestyle save carries its map seed; a quest seeds its map from the clock | [formats/saves.md](formats/saves.md), [maps.md](formats/maps.md#a-quest-is-generated-from-its-own-q-on-the-clock) | `sim::freestyle::Setup::seed`, `sim::mapgen::quest` |
| A coffer is a hero's purse key; a stat is the total; a spell's second word its timeout | [formats/saves.md](formats/saves.md) | `data::stat::PRIMARY_STATS` |
| Every object has a unique handle and an `AgentID`; word 5 is the owner; slots 0–6 are kingdoms, 7 not | [formats/saves.md](formats/saves.md) |
| `ORDR`, `CONT`, `ORLS`, `OFMP` and the held, selected and ruined bits are read | [formats/saves.md](formats/saves.md) | `world::piece_state::HELD` |
| A save names each object's `IMAG` set and layers; a mobile's slot word is a direction of 32 | [formats/saves.md](formats/saves.md) | `world::scatter::Drawn` |

## GPL

| Fact | Where |
|---|---|
| The front end parses all 306 shipped and every mod file; eight types; `float` is fixed point | [gpl/language.md](gpl/language.md), [gpl/compiler.md](gpl/compiler.md), `every_file_lexes_without_error` |
| Section 2 is a scope table, section 1 the code; an entry's `u16` is the low half of its record count | [formats/bytecode.md](formats/bytecode.md), `bcd::walk`, `gpl::bcd::Scope` |
| Three-address code, numbered post-order: a record's index is its slot | [formats/bytecode.md](formats/bytecode.md), `a_records_index_is_its_expression_slot` |
| 39 opcode names, 43 run; all three sections re-encode byte-identically | [formats/bytecode.md](formats/bytecode.md), `bcd::encode` |
| A record's header, marks and prologue are read; nothing in a `.bcd` record is unread | [formats/bytecode.md](formats/bytecode.md), `a_records_mark_is_decided_by_its_opcode` |
| Builtins are one case-insensitive map, 166 registrations; signatures come from an x86 walk | [formats/bytecode.md](formats/bytecode.md#builtins-one-map-of-names), `gpl::builtins` |
| The `GPL ` save chunk is read to the last byte | [formats/saves.md](formats/saves.md) |
| Operators run in the left operand's class; integers are fixed point, wrapping past 2^21; strings compare case-insensitively | [formats/bytecode.md](formats/bytecode.md#what-an-operator-does), `gpl::ops` |
| Records decode once; the last listed image owns a name; every call resolves | [formats/bytecode.md](formats/bytecode.md#the-decoded-form-and-the-load), `gpl::load` |
| One activation per function; threads on game ms, 50 a frame; two FIFO queues and a budget | [gpl/runtime.md](gpl/runtime.md#how-the-engine-runs-a-function), `gpl::runtime` |
| Play is a function of setup, session word and input; every RNG and its seed are known; checked by digest | [RNGs](gpl/runtime.md#every-rng-and-its-seed), [determinism](openregent/determinism.md#determinism), `base::digest` |
| A new game's setup order before frame 1 is read | [gpl/runtime.md](gpl/runtime.md#starting-a-game), `gpl::startup` |
| A save loads as the single-player load does; every corpus and trace save plays a minute without error | [formats/saves.md](formats/saves.md#what-a-load-restores), **Measured** |
| `gpl::vm` runs as the engine does, `foreach` cursor and errors included; six whole traces replayed every line | [gpl/vm.md](gpl/vm.md#the-vm), `gpl::vm` |
| HD's traces call 56 world builtins in two days, 17 at start-up | [gpl/vm.md](gpl/vm.md#world-builtins), **Measured** |
| Every world builtin answers from `sim::world`, nested script asks included | [formats/bytecode.md](gpl/vm.md#world-builtins), `sim::world`, `every_builtin_has_an_answer` |
| Spawns, searches, sight and reveal, items, limits, message flags, the advisor are the world's | [formats/bytecode.md](gpl/vm.md#world-builtins), `world::spawn` |
| The player's orders decode between passes as the log places them, by player, each as given | [formats/bytecode.md](gpl/vm.md#world-builtins), `game::Order`, `game::check` |
| A game runs on the world alone (`Game::free`); the traced games, quest and freestyle, played their logs | [gpl/vm.md](gpl/vm.md#a-game), `sim::game`, **Measured** |
| A game's end: victory, loss, the stand-still, high scores; which videos play, where, how stopped | [end](formats/panels.md#the-games-end), [videos](formats/panels.md#credits), `world::defeat` |
| Lairs take their spawner's settings and pick by weight on the world RNG; start names draw first | [formats/bytecode.md](gpl/vm.md#world-builtins), `world::new_map::Settings`, `World::seed_names` |
| A new game's world needs no save (`World::fresh`); quest starts and quest traces come out | [gpl/runtime.md](gpl/runtime.md#with-no-save), `sim::mapgen::fresh`, `sim::start` |
| A spell unit of sets 0x50 and 0x40, no 0x60 (wind storm): 0x50 once, then 0x40 looped; its timeout ends that loop's run | [gpl/vm.md](gpl/vm.md#world-builtins), `spell::end_at` |
| Combat, death, spells and hero homes are the scripts'; the engine writes hit points, ticks, and flies missiles | [formats/bytecode.md](gpl/vm.md#combat-death-and-spells), [homes](formats/panels.md#what-the-panels-say-beyond-hit-points) |
| A building falls by script, tracks or none, deselected; its insiders and its recruit go with it; its damage set is read | [formats/bytecode.md](gpl/vm.md#a-building-falls), [art.md](formats/art.md#which-set-a-building-shows), `world::ruin` |
| Particle systems are units no script makes; particles take a clock-seeded LCG; no rule reads them, no save holds them | [formats/bytecode.md](gpl/vm.md#particle-systems), `world::particles` |
| Effectors and speed trails: place, sets, draw order and deletion are read | [formats/bytecode.md](gpl/vm.md#what-an-effect-shows), `world::effectors` |
| Research, prices, rewards and a flag's seekers are read; a price counts no ruin or agentless building | [formats/bytecode.md](gpl/vm.md#prices-and-research), `world::research`, `world::price` |
| The build menu, preview verdict, guilds, research and spell targets: what the panels offer | [formats/panels.md](formats/panels.md#what-the-panels-offer), `world::offer`, `World::affords` |
| Guild actions (Market Day, Call to Arms, extortion, bounties, Rage, the gamble, resurrect, dismissal, rename, contests) and what each sends | [formats/panels.md](formats/panels.md#what-the-panels-offer), `sim::game::orders` |
| The palace's counts, statistics, rosters; game messages, the message scroll, 19 cheats | [panels.md](formats/panels.md#game-messages), [original/cheats.md](original/cheats.md), `world::ledger`, `world::cheat` |
| Walking and the planner: orders before scripts, A* on half cells; a five-day trace's every answer is the engine's | [original/movement.md](original/movement.md), `sim::movement` |
| The Liche Queen is on no collision layer: she walks through buildings and units | [original/movement.md](original/movement.md) | `the_liche_queen_walks_on_no_collision_layer` |
| Actions, timed orders, lying dead and gravestones; dead heroes' lists and graveyards | [original/movement.md](original/movement.md), [bytecode.md](gpl/vm.md#world-builtins), `movement::action`, `world::timed`, `world::graveyard` |
| A clock probe names each run's frame; the scheduler's model places every probe run | [gpl/vm.md](gpl/vm.md#what-the-probe-showed), **Measured** |
| `regent_cli compile` compiles every function back byte for byte; HD runs its image; Workshop sources rebuild their `.bcd` | [gpl/compiler.md](gpl/compiler.md), `a_whole_bcd_is_compiled_from_its_project` |

## Sound

| Fact | Where |
|---|---|
| HD `WAVE` is 16-bit PCM; `DSDG`, `DACT` phases and `DSND` are read | [formats/payloads.md](formats/payloads.md), `desc::SoundGroup` |
| What plays, how loud and where: cues, sight, `SoundViewRadius` | [original/sound.md](original/sound.md), `world::sound`, `view::sound` |
| `PlayMusic(n)` is a line of `MusicTracks.txt`; menu and end music are read | [original/sound.md](original/sound.md#music), `view::menu::music` |

## Editions

| Fact | Where |
|---|---|
| HD adds one attribute and eight builtins to the base engine | [original/editions.md](original/editions.md) |
