# Movement

How a walker moves in `MajestyHD.exe`: orders, the planner, turns and
steps. **Binary** unless marked. Model: `sim::movement`. `tests/movement.rs`
holds it against the inn probe ([vm.md](../gpl/vm.md#what-the-probe-showed)): both
peasants' walks to the inn and home, every frame, and every half cell the
planner gives them.

## The frame

Each frame the level runs every unit's orders, then the scripts:

```text
 frame
   ORDR, a snapshot in insertion order, per unit:
     track 0   the top order only    MovePath, Move, Turn
     track 1   every order           FollowUnit, FollowPath
     track 2   the top order only    an action's Turn, InitAction
   a unit with no order leaves ORDR
   GPL scheduler tick                scripts: Move, IsMoving, the probe
```

- Orders run before scripts: a `Move` in frame k first walks in k + 1.
- A unit joins the back of `ORDR` with its first order; the list keeps
  insertion order. A unit still in it keeps its place; one found idle
  leaves at once.
- An order runs when `now − when ≥ delta`. A push sets `when = now` and
  `delta` to the order's rate. An order flagged "run on push" gets
  `delta = 0` and runs as it is pushed.
- An order re-arms with `when = now`, `delta = rate` and runs at most once
  a frame: its period is its rate raised to a whole frame
  (`movement::interval`).
- Each order ends with its callback, then its own cleanup.

## Orders

Order types (`ORDR` entries, [saves.md](../formats/saves.md#the-chunks-keyed-by-a-handle)): 1 Turn, 2 Move, 3 MoveMissile,
4 MovePath, 5 and 8 Animate, 6 FollowUnit, 7 FollowPath, 9 Wait, 10 Action,
11 InitAction, 12 Counter, 13 AnimateExtended, 14 Fade, 15 PhysicsMissile,
16 Tracker.

| Builtin | Does |
|---|---|
| `Move(agent, unit)` | A held walker first leaves by its holder's exit, as `UnHide`. Then FollowUnit on track 1: the target's handle, min and max distance over 16, rate 500 |
| `Move(agent, coord)` | `UnHide`'s core first, then FollowPath on track 1: up to 10 waypoints; `add_waypoint` appends to a running one. **Game**: a city guard leaves its guardhouse on `Move` |
| `Move`'s third argument | flags: `add_waypoint` 1, `loop` 3, `avoid_vehicles` 4, `avoid_buildings` 8, `avoid_everything` 12. Both numbers after it come from argument 4 |
| `IsMoving` | track 0 holds an order |
| `HasWayPoints` | a FollowPath with more than one waypoint |
| `StopMoving` | clears FollowUnit, FollowPath, Animate and Action from track 1; track 0 runs on |
| `Hide(agent, building)` | FollowPath on track 1, the building's entrance its one waypoint; its callback enters |
| `UnHide(agent)` | at once to the nearest free cell from the holder's exit |
| `PerformAction(agent, action, target)` | `UnHide`, a stop, then an Action on track 1, run on push |

`Move` and `Hide` stop first; each pushes its order run on push.

**FollowUnit**, on push and every 500 ms: the walker is close when
adjacent (`IsAdjacent`), or, with a max distance, when the pad-rect gap is
within it. Not close, with track 0 empty and no Action on track 1: push a
MovePath toward the target. Close: take the MovePath off track 0 and end,
unless flag bit 2 is set. A step or turn already on top finishes; that is
arrival.

The order holds the target's unit handle and looks it up among all units,
not GPL agents. So a building `DeleteAgent` left standing as a ruin is
followed until its rubble goes. The MovePath keeps the target's place as it
was when pushed. **Game**: **Measured**.

**FollowPath**, on push and every frame after, with track 0 empty and no
Action: a MovePath to the next waypoint, no target. Past the last waypoint
it ends and its callback runs. Its rate, 500, is never used: it runs on
push, so its delta is 0, and it never re-arms.

**Action**: waits until tracks 0 and 2 are empty, then turns to its target
or point on track 2 ([below](#track-2)), re-arms with its rate and ends on
its next run. The rate is the action's `Rate` max plus
`ActionRateModifier` (`APV:50`), rounded as a move rate
(`world::walking::action_ms`): `Basic_build` 300 ms. **Game**: `IsMoving`
is false as the peasants turn to the inn.

**Entering**: `Hide`'s callback enters the building when the squared
pad-rect gap is at most 4 half cells (`movement::ENTER_GAP2`); else the
walker stays outside. The entrance is the exit point `SpawnUnit` uses: the
second point of block 2 of the slot the building draws
(`sim::placement::exits`). So a walker is inside the frame after its last
step: that frame its MovePath plans, is at the goal and ends, and
FollowPath, after it on track 1, ends and enters.

## Fliers

On movement layer 4, a leg aims at the higher of ground plus the class's
lift, and the class's least height. Over low ground the flier flies level
and each capped step is a full 15/32; over high ground it climbs with it
(`movement::aloft`, `Class::flight`). **Inferred**: `ML04`, the Large
Flyer, the only class with a least height, is layer 4's. Class flag `0x20`
keeps the facing through a one-node jog (`KEEPS_FACING`).

`PRIM` words 6, 7 and 8 are the least height, flags and lift
(`Class::lift`, `Class::flags`): the Large Flyer has least height 180/32,
lift 60/32, flags `0x31`. The reader stores word 7 and word 8 swapped
in memory, and the writer swaps them back (**Binary**). Words 9–12 are dropped and
derived at load. A version 1 `PRIM` gives words 0–3 only. **Corpus**: all
62 shipped `DMOV`s are version 2, words 9–12 zero
(`every_shipped_dmov_keeps_its_flags_in_word_seven`). **Game**: the united
winter game's two ice dragons fly frame for frame as the probe shows, with
lift 60. Word 7's bit 2 lobs the catapult's ball
([vm.md](../gpl/vm.md#missiles)).

Placing a unit on layer 4 uses the same height, so a flier is aloft before
it walks (`World::aloft`). Its body draws at that height; only the
second-pass shadow stands on the ground
([art.md](../formats/art.md#the-second-pass-is-a-fliers-shadow)).
`Shown::aloft` carries it to the drawing. **Corpus**: both ice dragons in
the united winter save sit at 368,640, 180/32 cells
(**Measured**). **Game**: an ice dragon
hovering over ground about 6 cells high draws its shadow 34 pixels under
its body: the lift, 60/32 cells at `32 × cos 55°` pixels a cell
(**Measured**). openregent
draws a flier its height less the ground over its shadow, at the same
scale.

Three units fly, all on `ML04` with `ATCH` movement kind 4 (`Fly`): `BVD1`
Dragon, `BVg1` Vendral, `BVm1` Ice Dragon. Harpy, Giant Eagle, Krolm Eagle
and Evil Oculus walk on the ground (`ML05`, `ML06`, `CL1B`); their flight
is drawn into the art
([art.md](../formats/art.md#the-anchor-is-the-ground-and-four-characters-are-off-it)).
**Inferred**: the unit's movement layer is that `ATCH` kind.

## Track 2

An action's turn and animation (`movement::action`).

```text
 the Action arms
   facing differs  a Turn on track 2, run on push: turn_step directions
                   a run, every turn rate; on the direction it pushes
                   the InitAction
   else            facing set, the InitAction at once
   own target      the InitAction at once
 the InitAction (order 11)
   the action's ImageSet from frame 0, a frame a run, at once and every
   rate ms; the run past the last frame ends it
```

**Length.** `(frames − 1) × rate` from its push. `frames` is the frame
count of the slot the facing draws. The slot is rounded, not truncated:
`w = 32 / slots`, slot `((w/2 + d) / w) mod slots`. `rate` is the set's
playback word 2, never the slot's, else 100, rounded as a move rate. Set
0, or one the art lacks, ends at once. Example: `Basic_build` plays
`Carry-2`, set `0x91`: `AVj1` four frames at 100 ms, 300 ms; `AVj2` six,
500 ms. A turn of g directions first adds `ceil(g / 4) − 1` runs of 50 ms.

**Start frame.** 0, unless the old and new sets share one of the
frame-keeping bits (`ImagSet` bits 17–19); then the frame the parts show
(`action::Phase`). In HD only `BVm1`'s (the ice dragon's) `Walk`, `Stand`
and `Attack` carry one, all nine frames at 50 ms.

- A frameless class's path plays `Walk` as it is pushed: a looping
  `Animate` on track 1 (its argument 2 is a tag, not a loop count). A spent
  path plays `Stand`. Each starts from the parts' frame.
- A stop takes every `Animate` off, an after-set's too, so the parts hold
  their frame.
- The `Move` after a `Turn` has flags 0, so its step moves the frame on,
  for a frameless class too.
- An after-set plays on from the action's last frame.
- `PerformAction` pushes the `InitAction` keeping the frame the parts show.
  Started on the last frame, it fires at the run past it, which ends it.
- While a frameless class turns for an action over more than one run, the
  parts loop their own set from frame 0, so the `InitAction` starts on that
  loop's frame. **Game**:
  `a_frameless_turn_of_several_runs_restarts_the_parts_set`.
- **Inferred**: a looping `Animate` first steps a rate after its push; a
  new unit's parts hold frame 0 until an order plays a set.
- **Game**: every dragon breath in the united winter game starts on the
  frame this gives (**Measured**).

**The peak.** Reaching its cue, else its last frame, fires once: the
action's projectile, overlay, function and `cProc` callback. That is frame
`cue − 1`, else `frames − 2`, after the push; at once for two frames or
fewer (`action::Peak`, `Pose::peak_ms`). A cue is the first frame after 0
whose reference has bit 31 (`imag::CUE`): the ratman shaman's `Cast` cues
frame 4 of 9, 300 ms, not 700 (`a_cast_launches_at_its_cued_frame`). The
world launches the projectile there ([vm.md](../gpl/vm.md#missiles)). The
log dispatches the function (`make_attack`, a spell's) in the orders after
the frame's tick
([runtime.md](../gpl/runtime.md#where-an-engine-dispatch-falls)); to-hit
and damage are the scripts'. **Game**: nearly all of the five-day trace's
([vm.md](../gpl/vm.md#how-it-was-verified)) basic attacks
run `make_attack` at the frame this gives
(**Measured**).

With a projectile at a unit, the turn aims at the unit's attachment point
from its art, not its position, scattered by two GPL draws on a building
([vm.md](../gpl/vm.md#missiles)).

**Dying.** A dying unit lies down at the peak. `Basic_death`'s and
`Become_Rubble`'s `cProc` is `0x2000`, which:

1. marks the unit ruined, so it blocks no walker;
2. deletes its relation-1 units;
3. searches for a place from its position, letting only itself through
   on any layer, and moves it there, onto a half cell;
4. puts set `Dead` on its parts (a hero's gravestone), held through the
   rest of the `InitAction`, after-set 0.

**Corpus**: every lying mobile draws `Dead` frame 0, one dying its `Die`
(**Measured**). `Henchman_death`'s `cProc` is 0:
a henchman stays where it fell (`World::lie_down`,
`a_heros_death_peaks_into_the_dying_callback`).

A unit killed mid-step lies down at the peak too. The stop takes the
`MovePath` off, not the step or turn on top, and the Action waits for
track 0, so the step lands first. **Game**:
**Measured**.

**What waits for it.** A `MovePath`, so a unit following something walks
on only after it, and the next Action. `IsMoving` does not see it;
`StopMoving` leaves it.

**What keeps a unit in ORDR.** Any order on any track, track 1's included:

- a looping `Animate`: `Stand` from a spent `MovePath`, or an
  `InitAction`'s after-set (none for a one-frame slot or a set the art
  lacks; `world::walking::loops`);
- `CombatSignal`'s flash order (`world::walking::Flash`). A stop takes the
  `Animate` off, not the flash. An `InitAction`'s end takes track 1's
  `Animate`s off (`movement::Track1`).
- **Game**, the five-day trace: a ranger's arrow flash keeps the champion it hit in
  `ORDR`, ahead of the ranger, so the champion's next `MovePath` plans
  before the ranger claims its next half cell.

**Buildings and timers.** A building's set plays unforced on every write
(at setup, as it is built or upgraded) and forced at its `InitAction`'s end
and as a unit enters or leaves it. Unless an `Animate` already shows the
set, one is pushed per part keyed 0 (`imag::Set::part_key`) with more than
one frame, so a building stays in ORDR while its set loops
(`world::animate`). These timers also hold a building in ORDR and end in
its turn (`world::timed`):

| Timer | Ends with |
|---|---|
| recruit | the hero is made |
| research | the research is done |
| enchantment | the enchantment is done |

A spell unit's `SetSpellUnitTimeout` `Wait` lists it until its deletion;
its set's `Animate` follows the `Wait` and is not modelled. Scatter's
`Animate`s are not modelled: scatter has no agent and runs nothing in its
turn. **Game**: every save's building and spell-unit ORDR places come out.

**The end.** The Action's end plays the action's after-set, XML
`CompletionImageSet` (`desc::Action::after_set`): 8, `Stand`, for
`basic_attack_with_stand` and the ratmen's spells and idles; 0 plays
nothing; -1 the action list's default (**Inferred** 0: no setter found). A
one-frame set pushes no `Animate`. The order leaves track 2 in the run that
ends it.

**Facing after the action.** Before an after-set plays, each part's
direction comes back from its slot: `slot × 1000 / slots / 32`, truncated.
A whole turn is 1000 here, not 1024, so every slot but 0 lands one short
of its centre: of 8 slots, 0, 3, 7, … 27 (`action::settled`,
`action::Settle`; **Binary**). The slots are those of the set the
parts show: the action's, else, if the art lacks it, the one before (**Inferred**:
`Stand`, which a walk's end plays). A walk's end plays `Stand` without
this. **Game**, the five-day trace: every ratman path after a `Stand` after-set turns a
run longer, as a facing of 12 settles to 11 then turns to 16 in two runs
(`an_after_set_turns_the_facing_back_to_its_slots_first_direction`).

**The agent is held.** `PerformAction` gives its Action the actor's agent.
An order holding an agent marks it, and the agent's threads do not run. As
the Action ends, track 2's order takes the agent; the `InitAction`'s end
releases it. So the actor's threads wait for the later of its Action and
its animation. The mark is the agent's, one for all its orders: an
`InitAction` that ends clears it though an Action pushed while it played
took the agent too, so the threads run from then (`World::holds`).
**Game**: a shaman casts twice in one `raider` run and runs `raider` again
once the first cast's animation is done
(**Measured**). `Move` gives its orders no
agent; **Game**: the inn probe's peasants run their threads while walking.

**Game**: in the inn probe the builders' `peasant_go_build` runs every
500 ms (`#Henchmen_Cycle`); a build holds `AVj1` 300 ms after its turn, so
no run moves, and both walks home start on time.

## MovePath

Rate 0: it runs every frame it is on top.

```text
 track 2 busy? wait
 start  nearest cell the walker fits, still things only
 goal   target: nearest free cell beside it, anyone blocks
        point:  nearest free cell to it, still things only
 path spent (index + 1 ≥ count)?  plan again, index 0: aim, then A*
        aim: flood seeded at the goal, anyone blocks, asked from the start
        count ≥ 2: clear "plan round walkers"
        count ≤ 1, "plan round walkers", start ≠ goal: set "nudge", Wait
        count ≤ 1: the MovePath ends; on the map the walker stands,
                   off it a last Move to the point
 path[index] ≠ start?  count 0, plan next frame
 next = path[index + 1]
   free for anyone      ─> step order, claim next, index + 1
   free for still only  ─> a walker in the way: below
   blocked              ─> plan next frame
```

- **The aim** is the free cell nearest the goal, counting anyone; on a tie
  the one nearer the start (`movement::plan::aim`). A walker on the goal
  moves it; the goal itself counted only still things. **Game**, the five-day trace: a
  peasant by its palace door, a giant rat's rect over the door cell: the
  aim is the cell it stands on, the plan one node, and it enters.
- **Following a unit**, the aim ranks first by the pad-rect gap to it, not
  the distance from the goal. This matters only when the goal is taken: a
  free cell beside the unit is the seed, and the seed comes first
  (`movement::plan::Rank`). **Game**: the dwarves game, a skeleton aims at
  the cell beside its hemmed-in peasant nearer itself.
- **Kept length.** A plan keeps its first 10 nodes (`RepathLength`), so the
  walker plans again at the ninth. Plans are counted; one past
  `ExpandCount` (4) doubles the kept length, up to `PathHorizon` 1024, and
  resets the count: 5 plans keep 10, then 6 keep each length.
- **A point off the map.** The start and goal floods clamp their seed to
  the map, so the plan ends at the edge. The spent MovePath then pushes a
  Move straight to the point, flags 0: off the board, landing on the
  point, no `Stand`, no claim, so the held half cell stays the edge's
  (`Around::beyond`). FollowPath ends as it lands. A path back starts from
  there; its first step walks onto the map. A ranger's `Journey_Offmap`
  walks to `FarthestMapEdge`, 250 32nds past the farthest edge;
  `hide_off_map` needs it within 32, else it walks again, so without this
  the ranger loops at the edge (**Measured**). **Game**: the wizard
  trace's rangers walk off and back so
  (**Measured**). No minimap or portrait sign of it is
  drawn (**Game**).

### Waiting on a walker

The next half cell is free of still things, not of walkers:

```text
 record the wanted cell
 wait_for?  count up; fewer than 4 before:
            Wait order, track 0, 1000 ms (CollisionDelay)
 else  count 0
       "nudge" set?  clear it; nudge; plan next frame
       else          plan next frame counting walkers
```

`movement::Walker::among` runs it against `movement::Around`;
`movement::wait_for` is the test.

- **The wanted cell** is written and cleared only by a path's run, as it
  blocks, steps or ends. A new path, a stop or a path's removal leaves it.
  So a stale wanted cell still closes another walker's ring, and that
  walker does not wait. **Game**: the dwarves game, a skeleton refuses a
  wait because a third walker's stale wanted cell closes a ring.
- **No free cell beside a followed unit**: the goal is the target's own
  half cell.
- **`wait_for`**: no wait in a ring, each walker waiting for the next's
  cell. Else the first unit in the way over the pad rect at the cell: none,
  wait; else wait if it is moving and no slower. Speed is `move_dist / 32`
  over the move rate; the wait is skipped only when the walker is strictly
  faster. A waiting walker is moving: its Wait is on track 0. **Game**,
  the five-day trace: a ratman waits 1000 ms for the faster champion in its way.
- **The first unit in the way**: the pad rect's half cells clipped to the
  map, rows then columns, each cell's list in turn; the first on a shared
  layer, not ruined. **Inferred**: a cell's list is in agent order.
  **Game**: `a_ruin_is_no_walkers_blocker`.
- **The Wait** (order 9) ends on its first run, 1000 ms on; the MovePath
  runs the frame after. A step clears the wanted cell; the wait count is
  reset only as above.
- **The nudge**: among the path's nodes from the one held to at most the
  fifth, the first where a unit on the walker's pad rect, row by row, has a
  movement class, is not frozen (`APV:63`), is not hostile and has the
  walker's owner (never `AVh1`). If its handle (0 when still) is below the
  walker's (0 when still), it gets a FollowPath to the walker's goal.
  **Game**, the defeat probe: a waiting champion nudges a standing one,
  which walks to its goal round by the south.
- **Sets.** A wait plays set 8, `Stand`; the wait on walkers short of the
  goal plays none. `movement::Gait` keeps it until the next step
  (**Inferred**: a step shows the walk). A step with `Stand` still showing
  and set `0xb0` playable is animation only, not modelled.

**Step orders.**

- If the facing differs from the node's direction: a Turn, run on push.
  Else a Move with flag "capped", at once for the path's first node, else
  `delta = rate − 50`.
- A Turn turns `turn_step` directions a run, the shorter way, every turn
  rate. On the direction it ends and pushes a Move with flags 0:
  `delta = rate`, landing exactly on the node.
- Direction: 32 directions, 0 north, clockwise; within a quadrant by
  `short * 100 / long / 25`.

## A step

A Move runs every `rate` ms:

```text
 d     = node − position                  x, y, z in 16.16 cells
 dist  = fixed_sqrt(Σ fixed_mul(d, d))    taken before the step
 dist ≤ step, flags 0     position = node, done
 else  position += d * step / dist        fixed_mul, then fixed_div
       "capped": x, y capped at 15/32; done when dist ≤ step
```

- A node is a half cell's corner, `(x, y) << 4` in GPL coordinates, on the
  ground: z is the ground's height there, two triangles a cell, cut by
  `(x ^ y)` parity. z is not re-read, so a slope shortens a step. The last
  cell row and column read height 0. **Game**:
  `the_last_cell_row_and_column_have_no_ground`.
- A capped step at a node's end goes a full step past it: straight legs
  read 6, 6, 6 across nodes. After a turn the Move lands on its node.
- Step length: `move_dist / 32` cells, times `50 / rate` under a frame,
  capped at 15/32 only for a capped Move; the Move after a turn takes the
  full step. Only a class faster than the cap shows it: the Large Flyer,
  8/32 at 15 ms, 26.7/32 a frame, lands on its node at once
  (`only_an_overshooting_step_is_capped`).
- Rates: the class's plus `MovementRateModifier` (`APV:34`) or
  `TurnRateModifier` (`APV:51`); from 50 up rounded to whole frames,
  `(25 + r) / 50 * 50`. Class 1's 90 ms is 100.

**Walk frames.**

- A step is a `Walk` frame: it puts set 1 and moves part 0's frame on by
  one, unless the path has the frameless flag. So a walk frame lasts the
  move rate, at least a frame (`Walker::stride_ms`). A class with flag 1 is
  frameless: its path push plays `Walk` as an `Animate` at the set's rate
  instead (`movement::FRAMELESS`). The `Move` after a `Turn` has flags 0,
  so its step moves the frame even then (`Walker::step_frame`). A frame
  with no step, a turn's or a wait's, holds it (`Walker::walk_frame`,
  `Shown::walk_frame`).
- Every `Move` puts `Walk` on the parts first. From another set the frame
  goes to 0 unless the sets share a frame-keeping bit; on `Walk` nothing
  changes. A wait puts `Stand` on the same way. So the first step after a
  wait, a path's end or an action shows frame 1 (`movement::Parts`,
  `action::Phase::put`). **Game**: every walker in the probed saves
  draws the world's frame (**Measured**).
- Art without `Walk` or `Stand` (HD bodies with a gait: missiles, spell
  units, Strangleweed, illusionary_hero): the set switch fails and the
  parts keep their set, at first the art's first (`Body::first`). A step
  still moves part 0's frame on, wrapping at that set's count; a wait
  changes nothing (`a_step_without_walk_steps_the_set_shown`).
- A `Stand` looping from a path's end or an after-set is not stopped by the
  next `MovePath`, nor by a `FollowUnit`'s or `FollowPath`'s next path. Only
  a new `Move` from GPL stops it (it stops first), or a frameless class's
  path, whose `Walk` replaces it. Left running, it steps part 0 on at
  `Stand`'s rate through the walk, wrapping at `Walk`'s count; the step
  runs first, on track 0. **Corpus**: of HD's bodies with `Walk` and a
  multi-frame `Stand`, only the Dryad steps walk frames, and its path ends
  on set `0xb1`; so only an after-set's `Stand` can loop into a Dryad's
  walk (`only_the_dryad_steps_walk_frames_past_a_looping_stand`). No save
  holds a walker with an `Animate` on track 1.
- Where the art has set `0xb1`, a spent `MovePath` plays it instead of
  `Stand`, as an `Animate` of part 0 on track 0, not track 1. Frame 0; none
  for a one-frame slot. Not run on push: its first run is the next frame's,
  then one a set rate, a frame a run; the run past the last frame ends it
  and the frame holds. Track 0 is busy until then: `IsMoving` is true, and
  the next path, follow or Action waits (`movement::PATH_END`,
  `Walker::play_end`). The next walk starts `Walk` on frame 0. **Corpus**:
  only the Daemonwood (3 frames at 100 ms) and the Dryad (4 at 150 ms)
  have the set (`both_walk_end_sets_play_on_track_zero`). A path's end,
  either way, leaves `Gait::Stand`.

`DMOV` `PRIM`, from byte 4 (`movement::Class`):

| Word | Field | Class 1 |
|---|---|---|
| 0 | turn rate, ms | 50 |
| 1 | directions a turn step | 4 |
| 2 | move rate, ms | 90 |
| 3 | 32nds of a cell a step | 6 |
| 4 | over 16, ancestors a plan checks for turns | 0 |
| 6, 7 | a flier's least height and lift, 32nds | 0, 0 |
| 8 | flags, below | 0 |
| 9–12 | derived at load | 0 |

Word 8 flags:

| Bit | Effect |
|---|---|
| `0x01` | frameless: paths do not step walk frames |
| `0x10` | tramples: the plan lets scatter, ruins and agentless buildings (`AE:0` 0 after `DeleteAgent`) through (`movement::TRAMPLES`). **Game**: `an_agentless_building_lets_a_trampler_through` |
| `0x20` | keeps the facing through a one-node jog |

## The planner

- A* over half cells, eight neighbours, `dx` outer, `dy` inner, each
  tested before it is weighed. g adds 1 a side step, 2 a diagonal; h is
  the squared distance to the goal; the open heap orders by f alone, as
  MSVC 9's `push_heap`/`pop_heap`.
- A node reached cheaper leaves the open heap, which is rebuilt
  (`make_heap`), or the closed list.
- At most 1024 pops. The answer is the node nearest the goal by h, the
  first popped among equals; the path is its parents.
- With avoid modes set, agents of that kind within 20 half cells first
  mark a square round their cell impassable, half side the radius over 16
  ([below](#in-the-world)).
- With a class size, the turns along a node's last `size / 16` ancestors
  must sum to under 5 directions.

**The flood**, for the start and goal: seeded at one cell, it pops the
least rank, then the least squared distance from an asking cell;
neighbours x outer, y inner, each once. Beside a target the rank is the
pad-rect gap to it, and only cells beside it are taken or passed through.

**The grid.** A unit stands on the covered half cells of its pad mask,
else its half-open pad rect (`sim::placement::taken`), about its claimed
half cell; not while held, nor with `DontBlock`. That is the descriptor's
`Info` bit, set as a unit takes its descriptor and never cleared
(`world::piece_state::DONT_BLOCK`, **Measured**).
A cell blocks a walker when it lists 8 units, or one sharing a collision
layer (`Info` bits 4, 5), not ruined, not the walker. Planning and the
start count what stands still (no vehicle); the goal and each step count
anyone.

A walker on no layer is blocked by nothing and blocks nothing. The Liche
Queen (`BVj1`, `Info` `DontBlock` alone) is the one such monster: she
walks through buildings and units. A `PRIM` of version 1 would get both
layers; every shipped unit's is version 4
(`the_liche_queen_walks_on_no_collision_layer`).

A walker fits only with its whole footprint on the map
(`sim::placement::Edge`): a mask blocks at any covered half cell off the
map; a half-open rect blocks where it starts below 0 or ends at the map's
size, so a rect never takes the last row or column. **Game**, the five-day trace: a
champion at the map's top edge walks round a ratman by the south, where
the north would hang its rect off the map
(`a_pad_rect_off_the_map_does_not_fit`).

## What the probe shows

**Game**, a probed trace of a day with an inn placed:

- Peasant 8 (`Move` in frame 440) and 10 (320): every frame to arrival,
  position and `IsMoving`, is the model's, from the save's ground and the
  claimed half cells.
- The planner gives both paths exactly. 10 arrives first at (46, 202);
  with it standing there, 8's goal beside the inn moves and its path ends
  at (48, 205).
- The 3-frame pauses are instant turns: the MovePath runs, the Turn ends
  at once, its Move waits a full rate. The 5-frame one at (400, 3696)
  hides a step too short to print.
- Arrival is FollowUnit's tick adjacent to the inn, frames 710 and 580; the
  scripts' `StopMoving` in the same frames leaves track 0 alone.
- At the door a walker faces within 4 of north-west but not on it
  (**Measured**); peasant
  8, spawned and never walked, faces 0, a new unit's.
- Home by `Hide` in frame 810, both at once, every frame the model's
  (**Measured**). Each starts facing
  the inn it built: `PerformAction` on a unit turns the actor toward the
  target's position, 10 to 4, 8 to 1; so a 4-run turn and the first step
  in 816.
- The 24-frame pause is a wait: in 922 peasant 8's next half cell holds
  10, moving, so 8 waits 1000 ms; the Wait ends in 942, the MovePath turns
  in 943, the step lands in 945. 10 never waits.
- Inside the frame after the door, 1085 and 1118: the MovePath ends there
  and FollowPath enters at once. Both stop on (28, 238) facing 12, the
  last leg's.

## In the world

`sim::world` runs every unit's orders in `World::step`, before the
scheduler, as the engine does (`world::walking`):

```text
 World::step(now) ── ORDR, insertion order, per unit:
    track 0   Walker::among(ground, now, Among)     plan, turn, step, wait
    track 1   every order, from a snapshot:
              FollowUnit  every 500 ms: not adjacent ─> MovePath beside it
              FollowPath  every frame:  track 0 empty ─> next waypoint,
                                        past the last ─> callback
              Action      PerformAction's: arms with tracks 0 and 2
                                        empty, ends its rate later
    track 2   Walker::run_stage          the action's Turn, InitAction
 then units with no order leave ORDR
```

- The ground is the new game's save heights, levelled by each building
  the world places (`sim::placement::flatten`). A body's `DMOV` class
  comes from its descriptor's movement attachment.
- `Among` is the world as `movement::Around`: the collision grid of every
  unheld piece at its claimed half cell, and the scatter. The start flood
  counts `how`; the goal beside a target and the aim count anyone; the goal
  at a point counts what stands still. A waiting walker is weighed by
  `wait_for` against every other unit. A nudged unit gets a FollowPath to
  the walker's goal point: beside a unit, that unit's place when the
  MovePath was pushed.
- **The grid changes** in two engine calls: re-register (off the old half
  cell, onto the new; skipped for a held or `DontBlock` unit) and remove.
  Re-register runs on every placement (made, teleported, unhidden, placed,
  reseated, lying down), on a walker's claim of its next half cell (its
  steps leave the grid alone) and on load. Remove runs on a delete, on
  entering a building or an overlay, and on a retype (then re-register
  with the new pad). A ruin is not re-registered: the blocker test reads
  the ruined bit live. The board keeps the grid (`world::board`); each
  change to a piece or its motion marks it, and `World::regrid`
  re-registers each marked piece before a plan reads the grid. With
  `MAJESTY_CHECK_GRID` set, each read checks it against a grid built whole
  (`World::check_grid`).
- Not modelled: a ninth occupant is dropped from a cell; a deferred
  piece's delete stays on the grid, blocking, until the next frame's
  drain. Here a piece leaves at its `UnitDelete`. Scatter a clearing in
  play deletes does stay until the drain (`Board::clear_later`; **Game**:
  the tournament probe's ranger plans round a scatter piece its frame's
  rat's nest cleared).
- A walker's place is the piece's 16.16 position and claimed half cell.
  `LocationOf`, `DistanceBetweenAgents`, `ListObjects` and `IsAdjacent`
  read those.
- `Move`, `Hide`, `StopMoving`, `UnHide`, `PerformAction`, `IsMoving`,
  `IsEnteringBuilding` and `HasWayPoints` are world builtins
  ([vm.md](../gpl/vm.md#world-builtins)).
- The world runs no thread of an agent an order holds
  (`World::holds`): its Action, or track 2 after it.
- The world follows a unit's orders only while frames are stepped, the
  ground and its class are known and every piece is placed. Else the
  unit's place becomes unknown, and while one is, no walker is followed.
- **Avoid modes** mark squares round buildings or mobiles near the start
  impassable, but only with a radius: `Move`'s distance over 16, twice.
  Each occupant within 20 half cells of the plan's start that blocks under
  the plan's rules marks a square of half side the radius over 16; only the
  A* reads the marks, and its start cell is cleared. With still things only
  counted, no mobile blocks, so `avoid_vehicles` marks only once a plan
  counts anyone. A plan with a radius that keeps under two nodes plans
  again with none. Model: `movement::plan::Avoid`, kept with the walker's
  goal (**Measured**). No shipped script gives a radius; `Go_Heal`,
  `Follow_Heal`, `flee_map`, `animal` and others give a mode alone, and
  plan as plain `Move`s. **Game**: a wizard `Move`d with `avoid_vehicles`
  and no distance must still walk.
- A new unit faces north, 0: its facing is its parts' slot, zeroed at
  construction and not written by `SpawnUnit`
  (`world::walking::FIRST_FACING`). **Game**: every unit spawned and never
  walked is saved facing 0.

**Game**: stepped by the scheduler's model, each thread run once a frame
while due, the world placed every run of the two probe traces and the day
trace, and answered every call. The inn day walks both peasants out, round
the palace, home and in, every probe line the engine's (**Measured**,
[vm.md](../gpl/vm.md#a-game)).

## Open

The five-day and two-day traces on the scheduler's frames give every
answer as the engine did (**Measured**).

- An action's peak effect: the overlay is not modelled. The missile flies
  ([vm.md](../gpl/vm.md#missiles)); one at a point is not followed.
- A building's actions: a building has no movement class, so it gets no
  Turn; its facing is set at once and the action's set plays in that
  facing's slot (`movement::Walker::fixed`). The world gives a building
  tracks at its first `PerformAction`, its `Crumble` aside. A one-frame set
  ends in its push run, inside the script's `PerformAction`; its end puts
  the building's set back at once, so no frame shows the action's
  ([art.md](../formats/art.md#which-set-a-building-shows)).
- A building's launch point: the facing sets the direction of every
  layer-0 part; the set's switch takes each part's slot from it, rounded to
  the set's slots, or for a `Directionless` descriptor the direction
  itself, 0 past the last. The launch reads point 4 of the first layer-0
  part's slot. So the Dwarven Settlement launches from its facing's slot
  of four, the `Directionless` ballista tower from slot `direction`. Below
  its `MaxHP` a building gets no set but `Crumble`: its Action plays none
  and launches at once (**Inferred**: its `InitAction` steps part 0's own
  set for one frame). **Game**: all 168 shots of the dwarves log land in
  its frames (**Measured**).
- No probe frame shows an action's end: track 2 rests on the binary and
  the data, not a trace.
