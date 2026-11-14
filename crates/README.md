# The workspace

Readers for Majesty's data files, and a compiler for its scripting language.

Windows Gold HD is the target, read with its mods and SDK — see
[docs/original/editions.md](../docs/original/editions.md).

Eight crates. The library is the artefact; `bink` is a vendored video
decoder; `regent_corpus` finds the tests' corpora; `regent_testgame` is a made-up
game for them; the other four consume the library, and are
separate so that what *they* depend on never reaches it. In one package,
`cargo test` on the library would build everything in `[dependencies]`, so
`regent_cli` could not take `clap` without the library taking it too.

| Crate | Holds |
|---|---|
| `regent_core/` | The library. Four dependencies: `logos`, `ariadne`, `serde`, `toml` |
| `regent_cli/` | Inspection tools and the GPL compiler. Binary of the same name; takes `clap` |
| `regent_corpus/` | Where the tests' corpora are (`.env`); every crate's tests take it |
| `regent_testgame/` | A small made-up game, all authored here, so tests with no game data run the real simulation |
| `regent_draw/` | The Bevy drawing layer `regent_viewer` and `openregent` share; a lib |
| `regent_viewer/` | A Bevy art browser: units and the tileset |
| `openregent/` | A new game run by the library, played and drawn with `regent_draw` |
| `bink/` | NihAV's Bink 1 decoder, vendored and trimmed, for the videos ([bink/README.md](bink/README.md)) |

The library is seven layers, lowest first; each uses only those below it
(`regent_core/tests/layers.rs` holds it):

- `regent_core/src/base/` — engine-wide primitives with no format or game
  knowledge: the RNG, fixed-order maps and the state digest, fixed point.
- `regent_core/src/data/` — how the game stores bytes: containers, payloads,
  framing. Strict and zero-copy: an unexpected value is an error rather than a
  guess, because the formats are only partly understood and a hard failure is a
  finding. Also the record index over a dataset's archives, and text.
- `regent_core/src/content/` — an install found and joined: where the files
  are, datasets, quests, mods, a unit and its art, the project's own TOML.
- `regent_core/src/save/` — openregent's save model, plain data.
- `regent_core/src/gpl/` — the language the engine runs: lexer, parser, resolver,
  back end, VM. Its input is text people still write, so it collects and
  recovers from errors instead of stopping at the first.
- `regent_core/src/sim/` — the game engine: the world GPL acts on, movement,
  placement, the rules it runs in play; the game driver and the live host.
- `regent_core/src/view/` — what the screen shows of data and sim.

The line between `data` and `gpl` is not binary versus text. `.qdd` is plain
text and lives in `data`; `.dat` is plain text and lives in `gpl`. It splits
`.bcd` down the middle: the block framing is a generic container, so `data`,
while the prototypes and bytecode inside it are `gpl`.

Background and findings are in the notes one directory up — start with
[../README.md](../README.md).

## Build and test

```sh
cargo check --workspace     # typecheck; fast
cargo nextest run           # test binaries in parallel; or cargo test
cargo nextest run --features install                  # adds the tests needing the game
cargo fmt --all
cargo clippy --all-targets
```

Plain `cargo` links Bevy statically. `--features dev` links it dynamically, for
fast rebuilds, and the `Justfile` passes it, except in `ci` and `release`. With
rust-analyzer, set `rust-analyzer.cargo.features` to `["dev"]`, so that it and
`just` keep to one Bevy build.

`[profile.test]` optimises, so `--release` gains nothing and costs a
slower rebuild.

`regent_draw`, `regent_viewer` and `openregent` are plain workspace members, so `cargo test` and a
bare `cargo build` compile Bevy. No `default-members` keeps them out.

The repo's `Justfile` wraps these; `just` lists its recipes.

Tests come in two tiers, by the data they need:

| Tier | Needs | Run with |
|---|---|---|
| code | Nothing | `cargo test --workspace`, `just test` |
| `install` | The game's files | `--features install`, `just test-install` |

A test file wholly in one tier has `required-features` in its crate's
`Cargo.toml`. A test in a mixed file has
`#[cfg_attr(not(feature = "…"), ignore = "…")]`. A new test that reads game
data needs this.

Nothing needs the game's assets to compile. The `install` tests
locate them by environment variable:

| Variable | Example | Holds |
|---|---|---|
| `MAJESTY_HD` | `~/.local/share/Steam/steamapps/common/Majesty HD` | The Windows HD edition, 1.5.2.28, GOG or Steam `beta2` — the target; its `SDK/` is the modding SDK |
| `MAJESTY_WORKSHOP` | `…/steamapps/workshop/content/73230` | Subscribed Workshop mods |

Each may instead go in `.env`, keeping a local install layout out of
the shell profile. `.env` is gitignored; copy
[.env.example](../.env.example).

```sh
MAJESTY_HD="/mnt/games/SteamLibrary/steamapps/common/Majesty HD"
```

`#` comments, quoted values and a leading `~/` all work. The process
environment wins, so a one-off `MAJESTY_HD=… cargo test` overrides the file.

Within a tier, a missing corpus fails the tests that read it, naming the
variable. Skipping would let the tier pass while verifying nothing.

### Tests on the test game

`regent_testgame` builds a game from its `fixtures/` text at test time: units as
a mod's XML, rules as GPL, agents as `.dat`, through the real readers and
compiler; `src/art.rs` writes the art and movement class. No game data, and no
retail text. A test takes a scene, seed, orders and frames, and asserts facts:

```rust
use regent_testgame::{HUT, KEEP, Run, TestGame, placed};

let scene = Scene::flat(128)
    .gold(1000)
    .unit(Placed::new(KEEP, [64.0, 64.0]))
    .unit(Placed::new(HUT, [40.0, 40.0]));
let run = Run::new(scene, 200).seed(7).input(2, Input::Demolish(placed(1)));
let played = TestGame::get().play(&run, |_frame, _world| ());

assert_eq!(played.errors, Vec::<String>::new());
assert_eq!(played.lost, vec![]);
assert_eq!(played.all(HUT), vec![]);
```

Facts and relationships, not stored digests: an intended rule change must not
break them. `regent_core/tests/test_game.rs` holds the rules, `openregent`'s
`game::test_game` recording, replay and saving. A new rule goes in
`fixtures/rules.gpl`, its unit in `fixtures/units.xml` and `agents.dat`; a new
code stays out of the retail ones (`the_games_codes_are_no_retail_records`).

## Binaries

`regent_cli` — inspection tools, spanning every domain. `regent_cli --help` lists
every command; `regent_cli <command> --help` its arguments.

```sh
cargo run --bin regent_cli -- tags       ~/Games/majesty/data/*.cam
cargo run --bin regent_cli -- tiles      ~/Games/majesty/data/interfacedata.cam
cargo run --bin regent_cli -- imag       --record AVH1 ~/Games/majesty/data/maindata.cam
cargo run --bin regent_cli -- desc       --record ABV1 ~/Games/majesty/data/unittype.cam
cargo run --bin regent_cli -- unit       --record AVB1 ~/Games/majesty/data
cargo run --bin regent_cli -- terrain    --record AtoB ~/Games/majesty/data/{terrtype,tilesetdata}.cam
cargo run --bin regent_cli -- deps       --resolve ~/Games/majesty/data ~/Games/majesty/data/miscdata.cam
cargo run --bin regent_cli -- ui-dump    ~/tmp/ui-art     # every UI picture as PNG, index.tsv
cargo run --bin regent_cli -- codesec    ~/Games/majesty/data/bytecode.bcd
cargo run --bin regent_cli -- lair-spawns                 # what each lair's spawn script can spawn
cargo run --bin regent_cli -- resolve    "$MAJESTY_HD/SDK/OriginalQuests"
cargo run --bin regent_cli -- check-data ~/Games/majesty/data ~/Games/majesty/datamx
```

The GPL commands take directories as well as files, walking them for the
extension they want. `check-data` and `check-gpl` parse everything given and
report failures: the fastest check that a change did not break a format.

`regent_cli compile` — the GPL compiler.

```sh
cargo run --bin regent_cli -- compile "$MAJESTY_HD/SDK/Adjust Guardhouse Mod/GPL/AdjustGuardhouse.gplproj" out.bcd
```

It mirrors `Gplbcc.exe`, which the SDK's `MakeGPL.bat` invokes as `gplbcc -in
<project>.gplproj -out <name>.bcd`. All three sections are written from source;
five of the SDK's seven projects come out byte-identical to the shipped images.
See [docs/gpl/compiler.md](../docs/gpl/compiler.md).

`regent_viewer` — an art browser: units' sets, poses and attachment points,
and the tileset. See [docs/openregent/viewer.md](../docs/openregent/viewer.md).

```sh
cargo run --bin regent_viewer -- "$MAJESTY_HD/Data"
```

It takes archives and directories the way `regent_cli units` does.

`openregent` — a new game, run by the library's world, played and drawn
with `regent_draw`. A new game is made from the custom-game picks and
HD's data alone; `--quest` plays a quest, `--load` one of its own saves.
The HD install: `MAJESTY_HD` in the process
environment, else `[paths] majesty_hd` in `config.toml`, else
`~/.local/share/openregent/data` (`$XDG_DATA_HOME`), else the current
folder or the program's. `.env` is used by tests only. See
[docs/openregent/game.md](../docs/openregent/game.md#the-game).

```sh
cargo run --release -p openregent
cargo run --release -p openregent -- --map-seed 7 --scenario Shad --land East
cargo run --release -p openregent -- --headless --frames 72000
cargo run --release -p openregent -- --quest QU06
```

Multiplayer offers LAN (UDP) and Internet (iroh). Internet hosts copy an invite
in the lobby; guests paste it in Multiplayer Setup. The lobby shows ping to the
host and whether iroh uses a relay.

## Module map

| Module | Contents |
|---|---|
| `data::cam` | The CYLB archive container, end to end |
| `data::payload` | `WAVE`, `PALT`/`SPLT`, `STRT`, and the shared `D*` header |
| `data::tile` | `TILE` — row-RLE and uncompressed, indexed or RGB565 |
| `data::imag` | `IMAG` — which tiles make up which animation, all four versions |
| `data::desc` | The `D*` descriptor tree, and the unit and action stat fields |
| `data::stat` | The attributes whose meaning is known, and the primary-stat codes |
| `data::rgs` | `.rgs`/`.q`, including three coexisting map versions |
| `data::block`, `data::bcd` | Block framing; the `.bcd` three-section file layout |
| `data::settings` | `GOPT`, the terrain lighting constants, and the camera pitch |
| `data::deps` | `BDEP` and `QDEP`, the postfix boolean dependency tables |
| `data::attrib` | The engine's own name for every attribute a save stores by number |
| `view::panel::icons` | Which interface picture a thing gets, when only the executable says |
| `data::smnu` | `SMNU` panel layouts — the keyed word stream, and the rectangles in it |
| `data::qdd` | `.qdd` quest metadata — plain text |
| `data::cp1250` | The codepage `STRT` strings are written in |
| `data::fourcc`, `data::error` | Shared four-character tags and error type |
| `gpl::token`, `gpl::lexer` | The token set and the error-collecting driver |
| `gpl::ast`, `gpl::parser` | Recursive descent, one function per grammar rule |
| `gpl::resolve` | Does every `$Name` resolve? Yields the builtin inventory |
| `gpl::opcode`, `data::gpltype` | The VM's 39 opcodes and GPL's 11 value types |
| `gpl::dat`, `gpl::proj` | The `.dat` prototype and `.gplproj` project formats |
| `gpl::bcd` | Prototypes and code entries inside a `.bcd` — the GPL half |
| `gpl::emit` | Back end: GPL sources to a `.bcd`, all three sections |
| `gpl::diag` | Rendering errors against their source, via `ariadne` |
| `content::dataset`, `data::catalogue` | A dataset definition, and the archives it names, indexed as one |
| `view::sprite`, `content::imagery`, `content::unit` | A unit resolved down to its frames and pixels |
| `content::terrain`, `content::terrain::schema`, `content::terrain::board` | The tileset, the 17 blend cases, and a map assembled from them |
| `content::terrain::grid` | A map's ground as arrays: vertices and cells |
| `content::terrain::ground`, `sim::paint` | Terrain classes per vertex, and painting one onto a map |
| `view::raster::shade` | Palette indices 247–254 as a screen operation, not a colour |
| `view::raster::blend` | Draw modes 5 and 8, translucent, and a unit's draw effects |
| `sim::footprint`, `sim::placement` | The rect a thing stands on, and the ground a building takes |
| `content::terrain::compass` | Which way is north, in GPL's terms and on the screen |
| `data::text`, `view::raster::atlas` | The `STRT` tables that name things, and sprite packing |
| `view::sound` | Which wave a sound cue plays, and the engine's mixing law |
| `view::map::fog` | Fog of war: unexplored ground black, unseen units hidden, part-seen ones darkened |
| `gpl::vm` | The VM, its host interface, and the `gpl.log` trace it writes |
| `sim::world`, `sim::setup` | The game world the builtins act on, and its assembly for a new game |
| `sim::world::kind`, `sim::world::piece_state` | A piece's kind, state bits and owner slot |
| `sim::movement`, `sim::space` | Walking, paths, and the grid's distances |
| `sim::clearing`, `sim::damage`, `sim::light` | What a placed building deletes, the set it shows, the relit ground |
| `base::fixed`, `sim::units` | The engine's 16.16 fixed point, and the simulation's grid units |
| `save` | openregent's own save model |
| `sim::game`, `sim::host` | The game driver, its input decoder, and the live host that answers GPL's builtins |

Four dependencies: `logos` for the lexer, `ariadne` for diagnostics, `serde`
for the save model, `toml` for the project's own files.
The tests add `roxmltree` (the SDK's descriptor XML),
`png` (captures), `toml` (scene files), `bink` (the videos), `ron` and `zstd`
(saved snapshots), `regent_corpus`, `regent_testgame`, `regex` and
`tempfile`.

The drawing code the library does not depend on:

```text
 regent_core
    └── regent_draw: data (Corpus), map, stage, material, texture, terrain,
          │       shadow, soft, shaders, art (the art service), log
          ├── openregent   (sim, menus, panels, audio, saves, settings)
          └── regent_viewer (browser, units, sprites, tiles, grid, main)
 bink: the videos' decoder, vendored; openregent takes it
```

`regent_draw` holds all of both programs' Bevy drawing: materials, shaders
and the code that puts them up. It draws what it is given (a map as a
`map::MapSource`, from a live world); game logic stays in
openregent, engine rules in `regent_core`. The viewer's unit and tileset
browsers are its own.

## The tests are the specification

Every structural claim in the notes is asserted here against the shipped files,
exact record counts included, so a wrong assumption fails loudly rather than
rotting quietly. That is how the three quest-map versions were found.

| File | Corpus | Asserts |
|---|---|---|
| `tests/game_files.rs` | The HD install | The format claims, in detail |
| `tests/editions.rs` | HD, `SDK/` included | The readers over the whole install |
| `tests/save.rs` | The HD install | Hero classes: stat keys, primary stats, gold costs |
| `tests/settings.rs` | `GOPT`, both dependency tables | What the engine is configured with |
| `tests/terrain.rs` | `terrtype.cam` and `tilesetdata.cam` | The terrain classes agree with the art that draws them |
| `tests/smnu.rs` | 616 panels, and HD's thirteen UI layouts | A panel's framing, and that only geometry follows the screen size |
| `tests/quest.rs`, `chart.rs` | The shipped `.q` and `.rgs` | A quest is consumed exactly, and every chart reference lands |
| `tests/view.rs` | The HD install, both datasets | The joins between formats, from a unit down to its pixels |
| `tests/placement.rs` | The HD install; **Measured** on saves | The plateau a building levels, and the bare ground it paints |
| `tests/icons.rs` | The art | The interface pictures a spell draws |
| `tests/attrib.rs`, `opcode.rs` | HD's data, the lexer, the shipped bytecode | The tables copied from the executables agree with them |
| `tests/corpus.rs` | 306 SDK `.gpl` files | Every claim in [docs/gpl/language.md](../docs/gpl/language.md) |
| `tests/mods.rs` | 361 Workshop `.gpl` files, 14 `.cam` archives | The same, on files written by strangers |
| `tests/backend.rs` | Both, plus 21 projects | `regent_cli compile` reproduces `Gplbcc.exe` byte for byte |
| `tests/doc_paths.rs` | The notes | Every Rust path and `.rs` file they name exists |
| `tests/the_corpus/`, `fresh_generated.rs` | The freestyle quest and chart; **Measured** on saves | The generator's inputs; a fresh game against its golden list |

`tests/common/` holds the tests' shared sim and generator helpers,
not a test target. The corpus lookup is the `regent_corpus` crate, re-exported there; it
reads `.env`.

Counts against the mods are deliberately lower bounds, because Steam updates
workshop items without asking.

The conventions these tests enforce — counts from a parser rather than a grep, a
second corpus before calling something impossible — are in
[../README.md](../README.md).
