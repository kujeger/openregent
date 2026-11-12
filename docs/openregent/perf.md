# Perf runs

`tools/perf/` is the fixed set every perf change measures against:

| Tool | Does |
|---|---|
| `build.sh [trace\|dist]` | release with frame pointers and symbols into `target/perf` (`perf record --call-graph fp`; dwarf graphs truncate in deep GPL recursion); `dist`: the shipped `dist` profile, below |
| `run.sh <scratch-dir> [case...]` | runs `tools/perf/cases` with isolated XDG directories under `perf stat` and `/usr/bin/time` |
| `summary.py` | prints one run, or two side by side |
| `quiet.sh [case\|group...]` | the figures to keep (below) |
| `trace_top.py` | sums a trace (below) |
| `cores.sh [all\|one\|eff N]` | this machine's performance cores, one of them, or N efficiency cores |

The cases: headless 128- and 512-cell bot games; drawn frames
on the hidden display with `--running`, in sync mode and (`p*`, with
`MAJ_PERF_PIPELINED=1`) pipelined as played; startup; two LAN peers. The
`camera` cases move the view (`--camera`, `shot::Camera`, after
`--look`): `pan` drags 12 points a frame round a square of 150-frame
sides, `levels` toggles 1:1 and 2:1 each 30 frames, `wheel` steps smooth
zoom a notch each 30 frames.

The `replay` cases (`r-*`) replay `assets/input_logs/` headless, the sim
alone, on one performance core: The Forsaken Land, the shortest, to
iterate on; Urban Renewal and The Siege, the heaviest, to confirm. A
headless replay exits non-zero unless every recorded digest
up to its last frame matched (`headless::checked`), and the summary marks
such a case FAILED: a change that alters the game is not a speed-up.
`just perf-replay` runs them 5 times on a quiet machine and compares with
the last run.

The game's own timers are opt-in: `MAJ_PERF=1`
(`openregent/src/perf/mod.rs`) logs at target `maj_perf`:

- main schedule, frame interval, the wait for the sim thread, step,
  publish, panel query;
- world clone (each copy the sim thread makes), deep digest, the LAN deep
  digest's snapshot and worker fold;
- the ground's compose (`ground`: `map::redraw::redraw` and
  `redraw_inset`) and the fog take (`fog`; `fog_moved`, the share of
  takes whose explored ground changed);
- the entities, the meshes and the meshes a camera drew (`entities`,
  `meshes`, `visible`).

Each series logs `n`, `mean`, `p50` (median), `p99`, `max` and its samples
past 25 ms (`over`, `perf::SPIKE`): a stutter is the tail, not the mean; one
hitch moves the mean, not the median. `summary.py` merges a drawn run's
reports by sample weight (p50 too: an approximation) and reads logs without
`p50`.

## Pinning and clocks

Efficiency cores make timers bimodal, so `run.sh` pins every case to the
performance cores (`PERF_CPUS`, default `cores.sh`: Intel hybrid's P
cores, the highest `cpu_capacity` on ARM, none where every core is alike)
unless the case sets `CPUS=` (`CPUS=all`: none; `CPUS=one`: the last
performance core). Every case runs with `setarch -R`: no address
randomising. Release links Bevy dynamically: the runner sets
`LD_LIBRARY_PATH`.

`build.sh dist` builds the shipped profile (thin LTO, Bevy linked
statically: no `dev` feature), still with frame pointers and symbols, into
`target/perf-dist/dist/openregent`. A case selects it with `BUILD=dist`
(`PERF_DIST_BIN` overrides the path); a static build needs no
`LD_LIBRARY_PATH`.

Without hardware counters (`perf_event_paranoid` above 2, most VMs) a
case runs without `perf stat`: wall and CPU time only, no instructions or
cycles.

For steadier cycles and wall time, fix the clock while measuring:
`sudo cpupower frequency-set -g performance`, and back to the usual
governor after.

Other load skews timings, so the figures to keep come from
`tools/perf/quiet.sh`, run by hand on a quiet machine: no `cargo`,
`rustc`, `clippy-driver` or `openregent` running and a 1-minute load of
at most 2 (it refuses otherwise, unless `--force`). It builds once, runs
the cases 3 times (`-n`) in rounds, and writes
`local/perf-runs/<date>-<commit>[-dirty]/` with `summary.txt` (median and
min–max per figure) and `compare.txt` against the previous run there
(`--against <run>`). Groups are the `#= ` lines of `cases`; the default
is headless, replay, drawn, camera, startup, lan and gpu; `--list` shows each group's
time.

`run.sh` itself refuses nothing, but at start it checks the same
condition (`busy.sh`, shared with `quiet.sh`), warns on stderr and writes
`<out>/disturbed`; `summary.py` marks the cases DISTURBED (machine busy).
`quiet.sh` sets `PERF_GUARD=off` for its repeats: it checks itself.

Other load the load average misses still skews a case. `run.sh` records
it: `ctx k` (thousands of involuntary context switches), `faults` (major
page faults: swap, cold files), and on SMT machines `sibling %`, the busy
share of the pinned cores' sibling threads. A busy sibling slows wall
time and cycles, not instructions; past 5% (`summary.py`'s
`SIBLING_BUSY`) the summary marks the case DISTURBED.

Clock speed sets timings more than the work does. **Inferred**: the
governor clocks down for a load that works a few ms a frame and waits for
vsync, and less work clocks it lower still. So changes are judged mostly by
instructions (`perf stat`, `perf record -e instructions:u`).

Figures are only comparable on one machine, so they are not kept here.

## Renderer and GPU

The renderer's cases (`g-*`, and the weak machines' `lvp-*`, `e1-*`,
`e2-*`) take two more switches:

- `MAJ_PERF_UNCAPPED=1` presents without vsync, so the frame interval is
  the frame's cost, not 16.7 ms.
- `MAJ_PERF_GPU=1` (`openregent/src/perf/gpu.rs`) adds `gpu_frame` (two
  timestamps around the frame's one submit); Bevy's
  `RenderDiagnosticsPlugin` per pass (`gpu_`, `cpu_` the encoding,
  `frag_` fragment invocations); the render world's schedule; draw calls
  and pipeline and material bind group changes per phase after batching
  (`draws_*`, `items`, `pipelines`, `binds`; egui's from its paint jobs);
  and every 600 frames the GPU memory: wgpu's allocator report, Bevy's
  images, the art service's upscaled and plain textures.
  egui's draws: `draws_egui`, `binds_egui` (texture changes),
  `clips_egui` (draws a clip rect alone splits) and `textures_egui`
  (distinct textures). `MAJ_PERF_EGUI_DUMP=<frame>` logs that frame's
  draws one a line: texture, picture, clip rect, bounds.

- `MAJ_PERF_HIDE=<class,...>` (`openregent/src/perf/hide.rs`) hides
  classes of world meshes each frame, so two runs' `frag_` counts differ
  by what a class drew: `floor`, `ground` (patches), `inset`, `veil`,
  `bands`, `things` (the map's scatter), `pieces` (other opaque
  bodies), `glass`, `lift`, `plain`, `rings`, and `tracking` (the
  tracking window's camera off). With `MAJ_PERF` on, every 600 frames
  also `quad_<kind>`, the world image's texels the main view's body and
  band quads cover, and `art_<kind>`, the share of those their art
  draws.

Fragment counts are deterministic but for one thing: the masked phase
draws its bins in pipeline id order, and ids follow the order pipelines
are first specialised, which varies run to run. Where a body draws before
one under it, the GPU's early depth test drops the one under, so a
run's opaque count can differ from the last's by that. The ground
(patches, inset) draws in the opaque phase, before every masked mesh
(`plain::Alpha::MaskFirst`): the floor under it fails the early depth
test every run, and the ground under bodies is shaded. Compare classes
in runs with `tracking` hidden too: the window draws every second frame.

Drivers: Asahi (Honeykrisp) gives no usable timestamp inside a pass (0
or nonsense: left out), so on it only `gpu_frame` and Bevy's
encoder-level `upscaling`; lavapipe gives every pass. Lavapipe is Mesa's
CPU Vulkan, picked with
`VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.aarch64.json`. wgpu
lists `EXPERIMENTAL_COOPERATIVE_MATRIX` for it and the driver then
refuses the device, so openregent asks for no experimental feature
(`draw::UNUSED_FEATURES`, wgpu's `all_experimental_mask`).

The `igpu` cases (`ig*`) run the `g128-early`, `g512-late` and a
1280x720 game on the integrated GPU: `WGPU_ADAPTER_NAME` takes a substring
of the adapter's name, `$igpu` in `cases`. `run.sh` sets it from
`PERF_IGPU`, else from the last parenthesis of the first
`INTEGRATED_GPU` `deviceName` in `vulkaninfo --summary` (e.g.
`RADV RAPHAEL_MENDOCINO`). Without one a case fails (FAILED); it never
falls back to the main GPU. The group is not in `quiet.sh`'s default
groups; name it (`quiet.sh igpu`).

Memory:

- wgpu's allocator takes blocks of 8 to 64 MB
  (`MemoryHints::MemoryUsage`, `draw::defaults`); its larger default
  blocks held far more than the live allocations.
- The xBRZ main menu holds its 8 running strips' 231 frames at window
  size; leaving the page keeps one frame a strip (`art::ui`'s `trim`).
- Bevy checks each material type with an entity for specialization every
  frame, a parallel pass costing about as much for one entity as for
  thousands, so the idle upscale quads keep no material
  (`present::equip`).

## Threads

Every schedule of the main and render worlds runs on Bevy's
single-threaded executor (`openregent/src/draw/threads.rs`): the game
thread, the main thread and the render thread do the frame. Bevy's
multithreaded executor wakes pool threads for each system, about 1,000
futex calls a frame in a late game; single-threaded, that frame takes
about half the time on 1, 2 and 16 cores.

The pools stay Bevy's: the ground's patches draw on the compute pool,
upscaling on the async compute pool. Without the render thread a frame
takes longer.

Switches for the `threads` cases (`x-<config>-<cores>`):
`MAJ_PERF_EXECUTOR=multi` (Bevy's executor), `MAJ_PERF_POOL=<n>` (compute
pool threads; IO and async compute 1 each) and `MAJ_PERF_UNPIPELINED=1`
(no render thread). The summary's `vol k` is the voluntary context
switches in thousands: the threads' waits.

## Traces

`tools/perf/build.sh trace` builds openregent's `trace` feature (Bevy's
`trace_chrome`, off by default: spans cost) into `target/perf-trace`; the
`t*` cases write `<case>/trace.json` to open in ui.perfetto.dev, or sum
with `tools/perf/trace_top.py <trace.json> [count] [match]`: spans by total
ms, per frame and calls, over the drawn frames from 10 after the game's first
`game::clock::tick` (a trace with no game: after the first third); it names
the rule used. `--schedule <Name>` (`PostUpdate`, `Update`, `Render`, ...)
instead gives that schedule's wall ms per frame, the spans in it (its thread's
outermost, and task-pool `system:` spans inside its intervals: another
schedule's concurrent systems leak in), and the unattributed remainder, the
executor's own cost. By hand:
`TRACE_CHROME=<file> openregent ...` with that build; without
`TRACE_CHROME` the file is `./trace-<time>.json`.

## What keeps a late game cheap

A late 512-cell game has 22,000 entities, of which a camera draws about
200. Each measure below avoids visiting every entity, or every flag,
each frame:

- `regent_draw::cull` parks the map's things far from the screen and the
  tracking window's ground: buckets of 512 world units, parked past
  half a view's span, back within a quarter. A parked thing has no
  `ViewVisibility`, so Bevy's visibility check, its reset and the 2D
  extraction skip it. About 2,400 meshes of 21,500 are left in a late
  512-cell game. A 512-cell map's ground is composed in patches
  (`map::patch`), so a pan composes only what comes into view; a 128-cell
  map fits one sheet, composed once.
- `fog::scatter` redoes only the cells about vertices whose flags the
  local player's mask sees changed, and walks nothing when none did. A
  record moves only with `Maps`; a new or gone one changes the count or
  is a removal, so no frame scans for a changed `Record`. A reveal looks
  at the frames on its cells (`by_cell`). Flags are compared by blocks,
  as are the veil's (`veil::changed`) and the minimap's
  (`minimap::Cache::changed`).
- `lie::tag` tags no parked band; one coming out is tagged that frame
  (after `cull`).
- A `map::things::Thing` carries the unit quad's `Aabb` and
  `NoAutoAabb`, so Bevy computes no bounds for it. Ground meshes (veil,
  ground patches, area, reach) are culled by their `Aabb`, for each
  camera and the tracking window.
- `Depth::of` sorts the placed rows again only when they changed.
- `World::shown_one` reads a unit's record once, not once for each fact
  of it. The records sit in an `IdTable` (`sim::id_table`): found by
  index through pages of 1,024 ids, kept in a store whose slots are
  reused, so memory follows the units alive, not the ids ever given.
  A negative GPL id (the build preview's, -6) is kept apart: as a page
  index it would make 4 M of them.
- The collision grid sits in shared chunks (`sim::chunks`), and the
  ground, paint, agents and grid entries behind `Arc`s: the sim thread's
  world copy at each publish shares them, and a write copies only what it
  touches. `reveal::Explored` keeps its 263,000 flags in shared chunks of
  1,024, a reveal copying the chunks it writes and only if a flag gains a
  bit; `Explored::differing` gives the chunks two copies no longer share,
  so the fog take, the veil and scatter (`veil::vertices`) and the
  minimap (`minimap::Cache`) read only those.
- A panel's run aside (`Game::aside`, the selected palace's henchmen
  each 20 game frames) journals the objects it changes
  (`gpl::vm::Vm::keep`); `MAJESTY_CHECK_ASIDE=1` checks the journal
  against a full copy.
- `ListObjects` (`World::passes`) runs its tests cheapest first, the
  agent's life after the side tests, and looks a piece's place up once;
  the verdict does not depend on their order. A ranged search passes
  over a bound piece whose pad rect is known out of range
  (`sim::world::reach`): one failed test is a sure no, and the list is
  sorted after the walk, so the walk's order does not count either.
  `World::relook` gathers the holders once a frame, not a scan a
  building.
- `Game::rules` reads the one attribute. `net::node::Node::fed` lets go
  only of digests past the window.
- The panels (`ui::bar`) are egui's, drawn whole each frame; what they
  read is made once and kept. Names read the descriptor alone
  (`Unit::descriptor`; `Unit::find` parses every `IMAG` set). A
  roster's rows (`palace::roster`) and the build list's
  (`lower::build`) are kept by game frame. Slot counts are kept by
  picture (`Ink::slots`), wrapped text by font, width and text for a pass
  and the next (`ink::Recent`); a spell button's tooltip is made only
  under the pointer.
- `World::build_menu`'s rule, price and discount and `World::spells`'
  levels gather the owner's buildings once
  (`World::buildings_by_prefix`, `stages`, `smith`); tests check both
  against each code asked alone on a played save. The minimap's units
  (`minimap::units`) take the published frame's `Steps::now`, made by
  `World::shown` after the step, in its order.
- A walker's wait lists who stands once, and `World::blocker` reads the
  cells only of those within reach of the rect. A plan and a flood find
  a cell's node in a window of cells (`plan::Marks`), not a hash; a plan
  asks no node's cell twice whether it passes.

Shots with each measure match the build without it pixel for pixel (late
game, zoomed out, map edges, FSR, pans, wheel, levels, Wild Expanse,
panels), except the tracking window: it draws every second frame by
`FrameCount`, and a build may begin its game a drawn frame later than
another (**Inferred**: loading time), so a shot of a panning view can
show the window a frame apart, the map the same. Two runs of one binary
can too (`--speed 4` shots, the window alone).

## Where the time goes

In the step, the GPL scripts take most of the time; within them the
world's piece builtins (the piece group of `World::checked`) and
`ListObjects`' tests of each piece in range per call (`World::passes`). In the
drawn game the per-step `Steps::after` (`World::shown` of every piece)
also counts. **Inferred**: a write to a chunk or agent the published world
shares copies it first, which costs `Game::frame` a little.

Left as they are:

- `ListObjects`' walk of the pieces in range, or of all without one:
  about 200 pieces a call in Siege, 5% of them listed.
- Per drawn frame a building's panel scans every piece for "Nobody
  Here" (`World::occupants`); without an index of what each building
  holds, the scan stays.
- Bevy's change scans (transform sync, dirty trees, each material's
  specialisation check, visibility propagation) still visit parked
  things.
- The fog take's seen-level maps: a `HashMap` of every shown piece,
  effect and missile, rebuilt each take.
- Bevy's `Disabled`: hidden from every query, a parked thing would need
  `Allow<Disabled>` in each system that changes it while far
  (`ground::relay` moves and despawns, `fog::scatter` hides and darkens,
  `map::redraw::redraw` despawns, `map::reel::animate` cycles), and each
  change marked again on the way back (transform, mesh, material,
  visibility), or Bevy's passes miss it; the saving is small.
- `material::Body` bindless (`#[bindless]`, indices `R8Unorm` read as
  `round(v * 255)`, a `BINDLESS` path in `body.wesl`). Bevy 0.20 bins 2D
  meshes by slab, so bodies batch with fewer draws and binds, shots
  pixel-equal. But a slab holds every body's textures (~1,700 late), and
  wgpu-core tracks each texture of a bind group at each bind: more
  instructions in late games, and slower lavapipe passes.

## Startup

A game without a screen (`--headless`) reads the archives without their
`TILE` pixels (`cam::Defer::Pixels`, `regent_draw::data::headless`): they
are most of the files, and a page-faulted copy of them was most of the
startup's system time. The simulation reads the few it measures from the
file (`Record::bytes`, `sim::footprint`). A drawn game still holds them.
