# How the engine runs a function

**Binary**, `MajestyHD.exe`, unless marked. Constants: `gpl::runtime`; the call graph is
held by `tests/runtime.rs`.

```text
 frame ──> scheduler ──> unit ──> its threads
           budget, two queues      └─> thread run ──┐
 engine event ──> dispatch, by name, at once ───────┤
                                                    v
                     function invoke ──> execute ──┐
                           ^                       │ opLParen
                           └────── call run <──────┘
```

## One activation per function

Every function's execution records are built once, at load. A value lives in
its record (offset `0x04`); an operand points to another record's
value. So parameters, locals, temporaries and a call's cached result exist
once per function, not per call. The call frame (callee and argument vector)
is per call, on the C stack.

| Slot | On entry | On exit |
|---|---|---|
| parameter, `opLocalize` | assigned from the argument: a copy, a list element by element | kept |
| local, `opDeclare` | reset to the type's default | reset again |
| temporary | whatever the last run left; made on first run | kept |
| call result, `opLParen` | reset before each call; a callee returning nothing leaves the default | kept |
| `__Return_value__` | bound to the caller's result slot, reset first; a dispatch's is an integer 0, a thread's empty | — |

- Defaults: 0, 0, false, (0, 0), emptied list, agent id -1, the null
  function. A string keeps its text: its reset only reserves 0 bytes. So a
  pooled string, reset on release, carries its last user's text into its
  next use. **Game**, a trace of two days of play: `TryTravelSpell` declares `spellname`
  as the last call's `NOTHING`.
- A call's arguments are the caller's handles, not copies: a builtin can write
  one back. A thread's are copies, made at creation.
- `opReturn` sets the record index to 2; the loop steps to record 3, the jump to
  the end. The prologue's `opExit` at record 2 never runs.
- The exit walk runs only on a normal return.
- A `foreach` cursor lives in its record's `head` word; hence the compiler's
  `opBreak` before a `return` inside a loop.

Re-entry is allowed in release play. It overwrites the running call's
parameters, resets its locals, shares its `foreach` cursors and rebinds its
return slot; the outer call then continues on those values. The debug-only
guard "Illegal recursive call" tests a word whose only write is 0, from the
unreachable `opExit`. **Inferred**: the guard never fires; the VM does not
make the check.

## Calls from the engine are synchronous

A dispatch runs a script by name inside the engine code that fired the event,
and returns to it: no queue. 50 call sites, 73 sites building its handle;
61 distinct literal names
(`runtime::DISPATCHED`), others from data. A dispatch saves and restores the
record count, so a dispatched script costs the frame's budget nothing.

So a builtin can run scripts inside a running script. `SpawnUnit` places a
building, which dispatches `Generate_Character_Attributes`. Making a unit
dispatches `NewUnitInit` (at a new game, the unit pass below), which calls
the unit's `BirthScript` at once; the source's comment says this saves running
birth scripts a frame later. Deleting a GPL agent dispatches `UnitDelete`,
which calls its `DeathScript`. A birth script spawning a unit re-enters
`NewUnitInit`, which reads nothing after its call.

## Threads and the scheduler

Game time is milliseconds: 50 per game frame, the `GE_GlobalData` step, not
`TimePerFrame`. It is every thread's "now"; the scheduler's log calls it
"Frame". `TimePerFrame` is real time: the frame loop waits until a frame has
taken the game speed's pace, 50 ms by default, 10 to 250 by the options
slider, and never catches up (`runtime::PACES`).

A thread is the `GplFunction` value handed to the thread builtins, by reference.
A unit's threads are its function-valued attributes, run in name order
(**Measured**). A thread holds:

- its function, and a replacement moved in before and after a run;
- its argument vector;
- its interval, ms;
- the game time of its last run's frame, or of its creation (`last`);
- its mode: 0 off, 1 once, 2 repeat; and a suspended bit;
- the time left while suspended;
- a running flag.

A thread runs when its mode is set, it is not suspended, and
`now − last >= interval` unsigned. A once-thread's mode clears as its run
starts; after the run, `last = now`. Nothing runs a thread as it is made.

A new game's setup dispatches run at game time 0, so a thread they start has
`last` 0. The frame steps the clock before it runs the scheduler: the first
frame's `now` is 50 (`runtime::SETUP_TIME`, `runtime::FIRST_FRAME`). A
thread started at `last` with interval `i` first runs at game time
`max(50, ceil((last + i) / 50) × 50)`: interval 1 at setup in the first
frame, 300 in the sixth.

**Game**, a fresh freestyle game's trace, saved at 150, then played to frame 11
(**Measured**):

| Thread | Started | Save |
|---|---|---|
| `KickoffFunction`, each `birthScript2` | `RunThread` 1 at setup | ran at 50: mode 0, last 50 |
| a tower's `RevenueScript` | `NewThread` 0 in frame 1 | runs every frame: last 150 |
| `Guard_Function` | `NewThread` 300 at setup | not yet due: mode 2, last 0 |
| `Guard_Spawn_Function` | `RunThread` 500 at setup | not yet due: mode 1, last 0 |

The trace runs past the save: `Building_Guard` at 300 and
`Palace_Guard_Spawner` at 500 are its last runs. Its `err.log`'s
"Total number of frames=11" counts the whole session.

A function value's empty name is two records: a null record, which
`ValidFunction` rejects and no thread runs, and `NullFunction`, registered
under the empty name, valid and runnable. A save writes both as the empty
name; the loader looks it up, so both load as `NullFunction`. The towers'
`RevenueScript` holds `NullFunction` in play: `ValidFunction` is true in the
trace, and the thread runs.

| Builtin | Writes |
|---|---|
| `NewThread` | interval, mode 2, last = now |
| `RunThread` | interval, mode 1, last = now |
| `RunThreadOnce` | as `RunThread`; a thread already scheduled or running keeps its mode |
| `SetThreadInterval` | interval |
| `SuspendThread` | suspended; left = interval − (now − last) |
| `ResumeThread` | not suspended; last = now − interval + left |
| `KillThread` | mode 0 |

The creators copy the arguments only when the thread is neither scheduled nor
running.

A script can hit this. `SLAY_DRAGON` starts `Vendral_Spawner` with `RunThread`,
and the function's stages each call `SetThreadInterval` for "another vendral".
The mode clears before the run, and the interval alone cannot restart it, so
only the first stage runs: Vendral and seven harpies, 280–480 s in. With the
Vendral the map places at the start, that is two appearances. The third comes
from `Slay_Dragon_Victory`, once the sword is picked up and a Dwarven
Settlement stands. **Binary** for the mode, **Corpus** for the rest;
`slay_dragons_spawner_runs_only_once` holds it on the install's image. The
comments in `Vendral_Spawner` show repeats were intended (**Inferred**).

The scheduler runs only under an engine flag that play sets. It keeps two
unit queues, each a FIFO: it takes the oldest unit of the active one, puts it
at the back of the other, then runs its due threads;
an empty active queue swaps. A new GPL agent joins the back of the second
queue, once (a log names a hero twice). A deleted agent leaves whichever queue
holds it at once and is freed between units. A save writes each list newest
first.

The budget is checked before each unit, never inside one. With budget left, a
frame visits every unit and one more (`runtime::EXTRA_UNITS`), so each frame's
pass starts one unit further on. `runtime::Queues` is the model.

```text
 7 agents, 8 visits a frame        active      next
 after frame 1                     2 3 4 5 6 7 1
 after frame 2                     3 4 5 6 7   1 2
 after frame 3 (the save at 150)   4 5 6 7     1 2 3
```

Budget: starts at `GPLLinesPerFrame`, 1500; every executed record counts, from
zero each frame. A unit is overtime when one of its threads ran over 100 ms
late and it executed records. Every frame adds to a frame count. Once the
units visited since the last adjustment reach the queues' count and 40 frames
have passed: if at least 15% of those units ran overtime, the budget grows by
overtime records over frames, at most 500, nothing under 50, capped at 10,000.
It logs `Frame %u, adding …` to `err.log`.

The budget's only readers are the scheduler's two tests and its growth. The
test before the first unit compares a count just zeroed, so only the test
after each unit stops a pass. No GPL builtin returns the clock.
Play stays a function of setup, seed and input while the budget does not
grow: it counts records, but its growth reads wall time.

openregent runs no budget: each pass visits every unit and one more
([enhancements.md](../openregent/enhancements.md)).

**Game**, against the engine's traces (**Measured**):

- A quiet day: the scheduler's model (`Queues`, `is_due`) put the 285 runs
  of the seven script threads started at setup in the log's order and
  reached the save's queues; the budget never stopped a frame.
- Five days of play under load, at 3–5 fps: passes stop often and some due
  runs start late, most by one frame, some by over 100 ms; overtime units
  stay far under 15%, so the budget never grows. No trace's `err.log` has
  an `adding` line.
- The defeat game's shipped scripts alone reach the budget: frame 5172
  (258,600 ms), the third wave spawned, is its first stopped pass.
- A quest's play reaches it from the start: The Forsaken Land's first
  frame runs 2,909 records, and about a fifth of its frames pass 1,500
  (openregent's run of the recording). Every quest trace ran with the
  budget patched out.
- With the game clock printed each frame, the model put every run in its
  clocked frame, to each game's end. This needs the engine's hold on a
  unit whose `InitAction` is waiting
  ([movement.md](../original/movement.md#track-2)) and a missile shot at a
  razed building's unit ([Missiles](vm.md#missiles)).

## Where an engine dispatch falls

A log shows the engine's own top-level dispatches between two passes. The
frame step says which clock each sees:

```text
 pass f ─ messages ─ clock to 50 (f+1) ─ hit point tick ─ level: orders ─ deletes ── music ── pass f+1
          input                                           callbacks       UnitDelete
```

- Before the step: a message's dispatch (the player's input), the music's
  stop callback after the pass, the interface's queries (**Inferred**),
  and the build preview's `UnitDelete`, agent -6 (**Game**: each just
  before its building's `NewUnitInit`).
- After the step and tick: an order's callback (an action's function at its
  last frame, a missile's hit, a timed order's hero) and `UnitDelete`.
  `DeleteGamePiece` defers the unit; the level drains the queue after the
  orders and dispatches `UnitDelete`.
- Which side of the step a dispatch falls: `world::input_timing::Stage::unheld`,
  `world::input_timing::BEFORE_STEP`
  (`a_dispatch_between_passes_runs_after_the_clock_step`).

## The GPL RNG

The GPL RNG is the world RNG's formula on its own state; `Vm::draw` models it.
A new game seeds it once, from a session word that
the session's constructor fills with a raw `timeGetTime`.
A save holds the state, not the seed.
`GplPseudorandom`, 0 in HD, seeds 1 instead and reseeds every frame
([Starting a game](#starting-a-game)).

Every caller of the GPL draw (no other code holds the formula's multiplier),
and what models it:

| Caller | Draws | Modelled |
|---|---|---|
| `RandomNumber` | one | the VM (`builtins::Builtin::draws`) |
| `RandomCoord` | one | `world::random` |
| `GetNearestHiddenCoord` | one | `world::reveal` |
| a missile at a building, without `NoRandomTargetOnDestination` | two | `world::missile`: aim and launch; `world::spell::summon` |
| `SpawnUnit` | an off-map edge point | no: no traced spawner is off the map |
| the frame's reseed, `GplPseudorandom` only | three | no; off in HD |
| one more caller | — | unreachable |

The particle RNG, the paint RNG and the world RNG are other streams.

**Game**, **Measured**:

| Run | Seed | Draws before the save | Saved state |
|---|---|---|---|
| start | `0x0b13af01` | 0 | `0x0b13af01` |
| day | `0x0b609e2c` | 2 | `0x13e97c72` |
| play | `0x0b6386e7` | 742 of 771 | `0x4938ec44` |

All 577 of the play's `RandomNumber` results of known bound come out; its
other draws are exactly its `RandomCoord` and `GetNearestHiddenCoord` calls.
Each seed is the only one of all 2³² that gives the logged draws. They are
millisecond ticks: day and play lie 190,651 ms apart, their logs 190 s. Start
to day is 75 s shorter than their logs: the value is the clock when a menu
last rebuilt the session, not at launch. Each save's `NMGM` holds the same
seed ([Starting a game](#starting-a-game)).

## Three games of one setup

Seed 1, size S, no input at first (**Measured**).
From `InitializeGpl` on, the logs agree line for line up to the first
`RandomNumber`, `peasant_spawner` at 5000 ms: the day draws 0, a male peasant,
the play 1, a female. Before it only interface dispatches differ. The start ran
the custom-game dialog's `CalculateDifficulty` twice more. The play dispatched
`GetSupportedHenchmen` three times between 3600 and 3900 ms (**Inferred**: the
palace panel); its first input message, frame 28's `CREATE_VEHICLE_MESSAGE` in
`err.log`, left no trace line. Frame times differed, 68 ms against 124 ms on
average (`err.log`); the scheduler runs on game time only.

So, as far as these traces go, GPL play is a function of setup, GPL seed and
input. The seed is the clock's, so two new games part at their first draw.

**Game**: two runs with the seed patched to the day's, `0x0b609e2c`, and no
input play one game until the lair's
monsters raze the palace and `Palace_Death` calls `DeclareLoss`: every line,
less the interface's runs and `FreeStyle_Music_Stopped`, which a track
ending in real time dispatches; it only picks the next track. The day trace,
of the same seed unpatched, is a prefix of both
(**Measured**).

**Game**: two runs with `--no-budget` are one game to `DeclareLoss`. They part from the budgeted runs at
frame 5,172, where the engine's budget cut the budgeted run's pass short, as the model
predicts (**Measured**).

## Starting a game

**Binary** unless marked. Constants: `gpl::startup`.

```text
 custom-game dialog            CalculateDifficulty…; charts from the map number
 game start          world RNG     <── session word
                     GPL RNG       <── session word  (1 with GplPseudorandom)
 new game setup, game time 0
   1 InitializeGpl              CreateAgent: GplAIRoot, agent 1
   2 unit pass 1                an agent per qualifying unit, ids 2…
   3 the quest's script         "Freestyle", or the quest's name
   4 unit pass 2                NewUnitInit per agent, same order
   5 Palace_Pre_Birth           one per owner, on its palace
   then a world draw per scenery unit whose slot has frames
 first frame step, game time 50
```

- Both passes walk the master unit list, the one every other unit list is
  built from. Charts place buildings under an engine flag that pass 2
  refuses, so the map's buildings get agents only in pass 1.
- A unit gets an agent if its descriptor kind is 3 or 2 (**Inferred**:
  Character, Building), kind 5 unless a flag bit is set, or a unit in one
  further engine state. Scatter is `General`: no agent. The new agent takes the next
  id and the unit's handle, and writes its id to `AE:0`.
- The quest script's name is data: a freestyle world uses `Freestyle`; any
  other quest its record's name, its `.mqxml`'s `<Name>` (**Corpus**: `MD01`
  and `XQD1` are functions, their `.q` ids `Krolm` and
  `DOWNLOADABLE_QUEST_1` none). So `Freestyle` is no literal in
  `runtime::DISPATCHED`. A quest's world:
  [maps.md](../formats/maps.md#a-quest-is-generated-from-its-own-q-on-the-clock).
- Palaces: code `ABJ*`, else an outpost `ABm*`, keyed by owner.
- Pass 2 walks the list as it stands after the script, so a unit the
  script spawned is announced twice: `NewUnitInit` inside its `SpawnUnit`,
  again after the start's units (`Game::setup`). **Game**, The Barren
  Waste: `setup_random_Treasure`'s twenty chests, agents 13–32.

**Game**, every setup trace (**Measured**):
`InitializeGpl`, `Freestyle`, six `NewUnitInit` on agents 2 to 7,
`Palace_Pre_Birth` on `Palace1#6`, then the palace panel's
`GetSupportedHenchmen`. The start save's agents 2–7 are its six `BLDG`
records in chunk order, handles 525–530 rising; none of its scatter objects
has one (**Measured**).

### With no save

`world::new_map` (`World::fresh`), `setup::fresh`: the world a start save
gives, made from the setup's picks, the generator's ground and scatter, and
the start's units with their places and slots. Generation is the generator's
(`sim::mapgen::fresh`); the place search and the slot draws on its charts are
the library's (`sim::start`). **Binary** unless marked.

```text
 freestyle setup        seven player entries, gold the setup's slot 6
 world made
   players              a Player per live entry, APP:0 its gold unless 0;
                        then slot 7, flagged unaligned
   scatter              handles 1, 2, ..
   off-map lairs        handles before the scatter's
   per chart unit, the next handle, by its record class:
     create             initial attributes (world::build::initial)
     animate            a world draw for the art slot;
                        a building: APB:1, APB:2 1, APU:0 its max
     building           place search, level, clear the ring, Dirt
     mobile             an agent, at its place
     terrain feature    no agent; scatter to the grid and the clearing
 slot 7 sees every vertex
 new game setup         ... Palace_Pre_Birth; the local player's palace
                        selected; the hit point tick's words from
                        #PeriodicHPDeltaPeriod, #PoisonInterval,
                        #HealingInterval, clock and count 0
```

- A chart building is made at its chart cell's corner and reveals there
  first, about its pad alone, before its search moves it
  (`Raised::made_at`). **Game**: the Liche Queen's temple, moved 3.5 cells
  south, leaves the 0-day save's six explored vertices at the map's edge.
  **Inferred**: no `SightRange` yet, as the disc's radius is the pad's.
- Each building reveals about it as it is placed (`World::sight`'s rule);
  scatter its ring reaches is deleted, as for a player's building.
- A building's or mobile's agent is the next after `GplAIRoot`'s, in
  handle order; its `AE:0` is written as a start save holds it. A terrain
  feature takes a handle and no agent (**Corpus**: `AE:0` 0 in `GRND`),
  and clears the scatter its ring reaches, as a mobile does. **Game**: the
  gamble probe's ruined keeps.
- The chart's owner is the cell's, else 7, `-1` for `AVk*`. Owner, record
  class, art slot and place are inputs (`world::new_map::Raised`), from
  `sim::start::raised`.
- The slot's bound is the slot count of the set a unit is made in,
  `Unit::initial`: its first `Inactive`, else its first set. **Corpus**:
  every start building's and terrain feature's slot, and every scatter
  record's set.
- The setup is `freestyle::setup_of` the picks: gold `freestyle::GOLD` 30,000
  and tileset `GRIT` unless picked, slots 7 and 8 zero, as on every corpus
  setup; slots 4, 5, 9, 10 zero (**Inferred**: unread). The side is the
  scenario's (`custom::chosen`).
- A chart mobile clears its ring as a building does. A building whose art
  lacks `Inactive` is not placed: it stays on its chart cell, unlevelled,
  nothing cleared or paved (`Sited::Left`). **Corpus**: the Day of
  Reckoning's monsters, the Siege's markers.
- **Inferred**: a chart mobile is made as a hired one, known to stand still;
  a building whose search found nothing is neither levelled nor cleared. No
  save holds the latter.
- `EnchantmentLevel` (`APB:4`) is `Tower_Birth`'s write, not the engine's:
  a new game on a start save skipped it (**Measured**).
- A chart lair takes its spawner's lair data and settings
  (`world::new_map::Settings`) at its making, keyed `1000 × template
  placement + unit`: `LUC:9`, `LUC:8` spawn rate, `APU:1`/`APU:0`, `LUC:10`,
  each word not 0. **Corpus**: every freestyle start save's `GSGL` lairs
  (**Measured**). No search finds an off-map
  lair.
- **Inferred**: the local palace is selected. Every human seat's palace is
  selected, so the state is alike on every machine; each shows its own
  (`World::selection`). **Game**: the start save's palace is its one
  selected piece.

**Game**: from the start save's picks alone (land, settlement, scenario,
monsters, map number), the generator's ground, scatter and start, the world
was the start save's, piece for piece, attribute for
attribute: handles, levelled heights, cleared scatter, explored ground,
players, gold, tick. The quiet day from it reaches the quiet day's save's `GPL `
chunk. Its drawing holds the start save's scatter record for record, slots
included, its heights and explored flags, and 15,781 of 16,384 cells' classes
(the world's pads, paved in the engine's order;
**Measured**):
`tests/fresh_generated.rs`, `tests/fresh.rs`.

### Every RNG, and its seed

The session word is the clock when a menu last rebuilt the session; a network
start overwrites it with a broadcast value.

| Stream | New game | Every frame, `GplPseudorandom` 1 | Saved |
|---|---|---|---|
| world | map number for the charts (a quest's: the clock), then the session word | frame + 1, before the clock step | `SVGD` `+0x74`  |
| GPL | session word; 1 with the flag | frame + 1, then three draws of 100 thrown away | `GPL ` chunk, state |
| 17 name generators, own streams (`world::naming`) | session word | — | `NMGM`, seed and count |
| paint | the clock, first use | — | — |
| particles | not read | — | — |

- A single-player load reads neither saved state into play: it seeds the
  world and GPL RNGs from the session word, as a new game does. Only a
  network game uses the saved states. The tests load with the saved states
  (`Seeds::Saved`) to go on as the saved game did.
- No C `rand`: HD imports none.
- In play the world RNG draws for a lair's pick, for every unit made (its
  art slot: spawns, recruits, graveyards, the player's buildings), for an
  upgrade and an illusion (the same slot), and twice more for an illusion's
  spread. The slot applies only to a `Directionless` descriptor
  (`World::slot_draw`). Every other caller is the map generator, menus' own
  RNGs, paint or dead code. **Inferred**, from a byte scan: no other slot
  draw. **Game**, the united winter game: the ice cave's third pick is the
  log's yeti only with every made unit's draw
  (**Measured**).
- Before play, after `Palace_Pre_Birth`, the setup plays set 0 on each
  scatter object and terrain feature and draws once for a start frame on
  each whose slot has frames (`World::start_frames`). The pass does not skip a
  piece a setup spawn's ring cleared: the delete only queues until frame 1.
  **Game**: without the Liche Queen's looping scatter draws, Ruined Keep
  15's first pick, at frame 3365, is a goblin archer, not the log's skeleton
  (**Measured**); The Barren Waste's chests clear two, drawn all the
  same.
- **Game**: at every traced save, the game from the start, and each save
  loaded with its saved states, holds `SVGD` `+0x74`
  (**Measured**): freestyle plays and probes, tax and repair,
  defeats, gate, wizard, dwarves, united winter, the Liche Queen, both Barren
  Waste runs.
- A network game seeds neither from the session: the world from its saved
  state, or, if that is 0, not at all, turning `GplPseudorandom` on. The
  freestyle dialog chooses single player (**Inferred**).
- An unregistered native seeds the GPL RNG from its argument; nothing reaches
  it. **Inferred**: the documented `ReRandom`.
- **Game**: every traced save's `NMGM` holds its game's GPL seed in all 17
  generators; every corpus save's generators share one seed
  (**Measured**). A save names its seed.

So no setting fixes everything. `GplPseudorandom` fixes the GPL stream from
the start and the world stream from frame 1; names, the world's setup draws
and paint stay on the clock. The session constructor's `timeGetTime` call
seeds every stream that bears on play, so one constant there fixes them. Paint and particles stay
clock-seeded: paint picks art and the earthquake spell's order, particles
only what is drawn (**Inferred**). Play is then a function of setup and seed:
**Game**, two patched runs with no input are one game to defeat except where
the music callback falls (**Measured**,
[three games of one setup](#three-games-of-one-setup)). Untested in the game,
as it is off in the shipped one: `GplPseudorandom`.

The session seed does not reach a quest's map. Chart generation seeds with
the map number, 0 on every quest, and seed 0 means a `timeGetTime` of its
own. A custom game's map number is the dialog's. Base and expansion quests
share one reader and one start; the `.bcd` is only script
([maps.md](../formats/maps.md#a-quest-is-generated-from-its-own-q-on-the-clock)).

## Errors

Every script error (VM, builtin, a bad conversion) throws `GplException`. The
interpreter catches nothing, so each script frame it unwinds skips its exit
walk: locals keep their values, a `foreach` cursor stays allocated. The next
call's parameter assign or reset frees a parameter's or local's nodes, so only
a cursor at the sentinel survives it. Two catchers:

- The thread run: logs "Script error" and the call sequence, only under
  `-debugout`; pops the whole call stack; resumes past `last = now`. A
  repeating thread is due again next frame; a once-thread has already cleared
  its mode; the running flag stays set; the pending function is not moved in.
- The dispatch: the same log, then the engine code continues as if the script
  had returned.

**Game**, the error probe in both clock probes: a once-thread on
`GplAIRoot` divides by zero at 5000 ms. The report reaches `gpl.log` as
`gpl::vm` writes it ([Trace and report](vm.md#trace-and-report)); play goes on,
every later line the VM's; the save holds the thread with mode 0, last 0
(**Measured**).

## Debug and release checks

Behind an engine flag that `-gpldebug` sets:

| Check | Release play instead |
|---|---|
| "Illegal recursive call" | nothing; never fires anyway |
| parameter type code | the parameter's assign converts, or throws a bad conversion |
| too many arguments | extras ignored |
| too few arguments | the builtin reads past the vector, in both modes: the invalid-parameter handler does nothing |
| inline attribute unit and name, opcodes 40–42 | unchecked; unused by shipped `.bcd` |

The flag also gates breakpoints and stepping, profiling, tracing and the
debugger's call-stack entries. Always on: a null `if` condition, a null
`foreach` variable or element, an unknown opcode. The `foreach` list is read
unchecked.

## No shipped script recurses by name

- `no_shipped_function_reaches_itself_by_name`: over all nine HD images, no
  cycle among calls naming their callee. 23 functions also call through a
  value.
- `every_value_call_reads_an_attribute`: each is `(x's "attr")(...)`. Resolved
  through assignments, locals, direct-call parameters and `.dat` members, only
  the expansion's `EventScript` stays open: `$LookupFunction` of a quest's
  event name.
- `recursion_needs_a_value_the_sources_do_not_narrow`: three cycles remain,
  each through a value the model cannot narrow. **Inferred** none occurs:
  `Player_Dead_Cleanup` calls lairs' death scripts, and the deaths calling it
  are a palace's and an outpost's; `QuestScript` copies `StartingScript`
  before `Be_Dumb` is stored there; `Freestyle`'s event script is a special
  event.

Engine callbacks run inside builtins, so a script can still re-enter through
the engine; the graph does not model that.
