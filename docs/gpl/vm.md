# The VM

`gpl::vm` runs a loaded `Program` as the engine's interpreter does. **Binary**
unless marked; held by `tests/vm.rs`.

```text
 host ──> Vm::run(function, args, Catcher)      a thread or a dispatch: no frame
            └─> execute ── opLParen ──> frame pushed ──> script: execute
                                                    ├──> native builtin
                                                    └──> Host::builtin
                                                          └─> Vm::dispatch, nested
```

Values are pooled objects behind handles, as in the engine: a record holds
three handles — its own value and its two literal operands — and an operand
names another record's result handle, fixed at load. Objects are shared where
the engine shares them: `__Return_value__` holds the caller's result object,
`agent's "x"` the attribute's. One set of handles per function, so re-entry
clobbers.

| Piece | API |
|---|---|
| Run | `Vm::run(host, def, args, Catcher::Thread \| Dispatch)` → `Run { result, error, report }`, or `Err(Crash)` |
| Nested | `Vm::dispatch(host, name, args)`: catches its own error, restores `Vm::count` |
| Host | `Host::builtin(vm, Call { builtin, args })`, `attribute(vm, agent, name)`, `named(name)`, `log(text)`, `agent_name(id)` |
| Stub | `LogHost` (`crates/regent_core/tests/common/log_host.rs`, tests only): records each call; attributes typed from the prototypes |
| Modes | `Checks::Release \| Debug`, `DebugOut::Marked \| All` |
| Trace | `Vm::set_trace(sink)`: one line per executed record |

A `Fault` is a `GplException`, with the engine's message, or a `Crash`: where
the engine faults and nothing catches — a divide trap, an empty handle
dereferenced, a read past the argument vector. `Crash::Unending` is not the
engine's: the test bound on records per run.

Debug mode (`Checks::Debug`) is the engine's debugger flag. The value
operations named below are the GPL value classes' own: *text* (what the
trace prints), *assign*, *reset* and *parse* (from `.dat` text).

## What the loop does, read again

- The `foreach` cursor is a `std::list` iterator, the node to visit next.
  Each pass compares it with the end sentinel, assigns the node's value,
  then steps to the next node. The node is never rechecked.
- So a change elsewhere does not move it. `-=` and `RemoveListMember` erase
  nodes; `+=` and `<<` insert one before the sentinel; assign and
  `ClearList` free them all.
  - Erasing the element just visited, or any earlier: nothing is skipped.
  - Appending: visited, except on the last pass, when the cursor is already
    the sentinel.
  - Erasing the cursor's node — a duplicate by `-=`, `ClearList`, assign,
    a parameter assigned by a new call or re-entry — leaves it on freed
    memory, and the engine reads it. It checks only the sentinel and a
    null value pointer, which throws "foreach list contains a NULL value".
    Every erase resets the value first (an agent becomes -1), and `-=`
    erases first to last, back to the head after each. So the next pass
    gives the type's default and steps on by the freed node's old link.
    **Inferred**: the heap leaves the node's bytes until reused, and no
    allocation came between; a later read sees whatever reused the value
    (the value pool is LIFO).
  - `gpl::vm` models that: a list keeps its last freed nodes
    (`ops::FREED_KEPT`), and a cursor holds its list, as the checked
    iterator does, so one left by a throw walks on in it, ending at its
    sentinel (**Inferred**: the engine then reads the walked list's
    sentinel as a node). A node not kept is `Crash::FreedNode`.
  - **Game** (`--bot`): `Release_Guards` meets it. A guard seeing an
    enemy its home does not chases, gives up out of the home's range, and
    `Guard_Find_Target` lists it in `Waiting_Guards` again, each round,
    until the home sees an intruder. With two guards listed alternately,
    the second `-=` frees the cursor's node; the next pass reads agent -1
    and `IsHidden` throws, twice, then the loop recovers. **Game** (a
    recorded session, debug checks on): a guard walking home sees a ratman
    within its own 150 but past its home's 450 (`Guard_Eval_Enemies`, then
    `Guard_attack_object`), gives up and lists itself again, every 2 s;
    `Release_Guards` meets two copies at frame 17,212, four at 22,077: one
    throw per freed copy, a frame each. Every answer on the way is a
    modelled builtin's, so the error is the scripts' and the engine's loop,
    not the world's. **Inferred**: HD's scripts do the same, though its
    reused value may be a live agent.
- Seven HD loops remove the element they visit (`list -= variable`:
  `Cull_Subtypes` and five more; `Dump_Contestants` over an attribute;
  1.5.2.28's `Release_Guards` empties its list before the loop). An index cursor would skip the element after each. No HD
  loop changes its list otherwise in its body, or passes it to a builtin.
  `tests/foreach.rs`.
- A callee gets a copy of a list, so only an attribute list can change under
  a loop from a called script. Two HD loops call one that changes the walked
  attribute, by name on any agent, directly or transitively: in
  `Dump_Contestants`, `Exit_Fair` removes the visited hero from
  `Combatants` (the loop before keeps only heroes targeting this building, so
  it is the walked list); in `Outpost_death`, `Find_New_Home` reaches `adopt`,
  which appends to some guild's `Members`. Neither reaches the cursor's node
  unless `Combatants` holds the visited hero twice in a row (**Inferred**
  possible: `Pursue_Entertainment` appends without a check). The three
  traces run neither function (**Measured**).
  The walk does not see scripts the host dispatches from a builtin, calls
  through function values.
- `continue` sets the index to its `opForeach`; the loop steps onto the
  record after it, the jump out. It leaves the loop, and the cursor stays
  on the next node: the next pass resumes there.
- `break` puts the cursor at the sentinel and takes the same jump. The
  cursor is not freed, so the next pass over that loop frees it and runs no
  iteration; the pass after runs again. Only Workshop code has either: five
  `break`, one `continue`.
- `while` on an empty condition handle dereferences it; only `if` checks.
- An operator on an empty left operand crashes. `GplNull`'s operators do
  nothing, so an operator on a null leaves its result slot empty, and an `if`
  on it throws.
- Assign is not the `=` operator: a null value's assign throws "Assignment
  to NULL attempted." unless the value is null. A function with no `is` type
  that returns a value throws at the return: HD's `Multiple_Attack_Check` and
  `Recall_Check`, always. `ops::assign` and `ops::reset` model assign and
  reset.
- A fresh agent is id -1.
- A building being placed has a GPL agent of id -6, made with the build
  preview. Placing the building deletes it, so `UnitDelete` runs on
  `Inn#-6` in the placement frame (**Game**, the inn trace).
- An argument read past the vector calls the invalid-parameter handler,
  which HD sets to a bare return: the read goes on. Too few arguments is
  undefined in both modes. The debug count check runs at the last
  `opLocalize`, so a function with no parameters takes extras even under
  the debugger.
- The debug type check prints `Parameter type mismatch: ""`: its `%s` is the
  `opLocalize` record's first value text, the type-only slot, empty in every
  image.
- A message's `$%s` is the function name only in debug mode; release play
  prints `$()` (**Inferred**: the name is assigned inside the debug block).
- The error line is written only in debug mode, -1 after a function;
  release reports line 0.

## Trace and report

The trace line, in debug mode with tracing on: a tab per GPL depth-stack
entry, then `$%s(), L#%d: %s`. An entry is pushed for every `opLParen` and
for a thread's or dispatch's own function, so a run's top level prints one
tab, each call one more. **Game**: every trace line of the start trace has
one to four tabs. The line is the last positive record line, from -1;
65535, no line, reads as -1 and is skipped. The body is empty for `opNop`,
else `op: %s, lhs: %s, rhs: %s, result: %s`: the opcode's token, each handle's
text, empty for an empty handle. Opcodes 39 to 42 print the bytes past the
token table. A trace line is printf'd twice, so a `%` in it vanishes.

A value's text: `()`, `_ltoa`, `_gcvt` of the float as a 32-bit value to ten
digits, the string, `{ a, b }`, `nullagent` or `name#id`, `true`, `x, y`, a
function below. `_gcvt` is MSVCR90's, so the text is the runtime's:

| Runtime | 20.0 | 1/1024 | Source |
|---|---|---|---|
| Windows | `20.` | `9.765625e-004` | **Inferred**, the CRT's `gcvt.c` |
| Wine | `20` | `0.0009765625` | **Game**: `20` in the two-day trace; **Inferred** `%.10g` |

`text::FloatText`; the traces are Wine's.

Only debug output shows the difference. `GplFloat` keeps the throwing
`string&` and `string` accessors, and every string path reads one:
`GplString`'s operators (`string&`), its assign (`string`). `s = f` and
`"x" + f` throw "bad conversion to string&". Of the six builtins taking any
type, only `DebugOut` prints. The other users of a value's text are the
trace and the error report.

A function value's text, by its mode and whether it is running:

| State | Text | Example |
|---|---|---|
| mode 0, not running | `$%s( %s )`, name and frame | `$Palace_Guard_Spawner( (), Palace2#6 )` |
| mode 0, running | `$%s()` | `$peasant_spawner()` |
| scheduled | `$%s(%s), run %s%dms %s %s`: `once @` or `@`, interval, `(s)` if suspended, `(r=$%s)` if pending | `$henchmen_travel_to((), Tax_Collector#10), run @500ms  (r=$bring_home_goods)` |

An idle value keeps a frame of one integer 0: `$Name( 0 )`. The null record
prints `NullFunction`; the builtin registered under the empty name prints
`$( 0 )`. A `.dat` function member left empty is the latter, so
`$ValidFunction` is true for it (the Freestyle trace). `gpl::vm` keeps
thread fields beside the value (`vm::Thread`); the world sets them.

`=` on a scheduled or running thread writes its pending function, not the
function; `==` on an idle one compares the pending function when set.
Scripts do this to their own `ActiveScript`: the next task after the run.

A caught error logs `Script error:\n Call Sequence was:` (a dispatch:
`GplDispatcherHandle script error:…`), then per frame of the debug stack
`\r\n`, four spaces per depth and `$Name( return, args )`, then
` - line %d : %s`, the message with `.` appended (`text::MESSAGE_END`). No
line end: the log's next line follows on the report's last. **Game**, both
clock probes: `Script error:\n Call Sequence was:\r\n$Probe_Error( (),
GplAIRoot#1 ) - line 1731 : Divide by zero error..`, the next trace line
on the same line, in `gpl.log`; the VM's report matched it
(**Measured**). The debug stack is
filled only in debug mode, with the same entries as the depth stack: the
run's own function first, then calls by `opLParen`, builtins included.
Release play lists no frame. Either catch pops both stacks until the depth
stack is empty, the outer runs' frames too.

## How it was verified

The VM and the world were checked against the engine's `gpl.log` traces
(`-gpldebug`, trace flag on) of freestyle games, quests and probed games,
diffed line by line, and against the saves those games made. A probe
image added a `GplAIRoot` thread printing every piece's place each frame
(`911 P <frame> <id> <x>, <y> <IsMoving> <InsideBuilding>`); a patched
executable added the game clock. Every run, line, probed place and saved
state matched (**Measured**). No test holds those comparisons now; quest
recordings guard the result ([testing.md](../openregent/testing.md)).

Rules the traces showed:

- A dispatch's return slot starts as an integer 0, whatever the function's
  type: its frame is reset to one integer 0. A thread's starts empty. So
  `declare return` prints `result: 0` for an untyped dispatch, `()` for an
  untyped thread.
- A reset string keeps its text.
- `#ATTRIB_Name` is the attribute's save key, bank then index as a
  little-endian word: `#ATTRIB_ReturnAmount` is 810963009, `APV:48`
  (`host::registry`).
- An attribute is one object per agent and name.
- A builtin's written arguments are `builtins::Builtin::writes`, from the
  SDK reference; `RemoveTitles` also removes what it returns.
  `ListObjects` returns the list's length (**Binary**).
- Seeded, the VM's `RandomNumber` and the world's `RandomCoord` and
  `GetNearestHiddenCoord` draws gave all 589 results of two days of play.

### What the probe showed

**Game**, **Measured**:


- A spawned unit shows in the first probe run after `SpawnUnit`: a peasant
  at the palace + (−176, 256), the door, a tax collector at + (−176, 272).
- Its thread first runs 10 frames later, and 10 after that. `Hide(unit,
  palace)` at the door: moving on the spot that frame, inside at the
  palace's place the next.
- Walking, the place changes every second frame, a third at a turn:
  6 along an axis in 145 of 168 steps; a diagonal leg the same 6, 68 steps
  for (288, −289). No constant 16.16 step reproduces a diagonal leg per
  axis; a straight one is 12288 exactly.
- `IsMoving` is true in the first probe run after `Move` or a walking
  `Hide`, before any step; the first step 3 frames after `Move`, 5 after
  `Hide`. False in the frame of the last step. After `StopMoving` 2 or 3
  more steps. `UnHide` puts a piece inside at the door at once.
- `InsideBuilding` is true the frame after the door, at the building's
  place, not moving; no script call between: the MovePath ends that
  frame and `Hide`'s FollowPath enters ([movement.md](../original/movement.md)).
- Peasants 8 and 10 take one path out of the door to the inn and one back.
  The engine's rules that give them, frame by frame:
  [movement.md](../original/movement.md).

## Builtins run natively

From each native function: argument fetch order, accessor, what is written.
`RandomNumber` (the world RNG's formula on the GPL state, the bound masked
to 31 bits), `Sqrt` (fixed point, floors), `MakeCoord`, `GetX`, `GetY`,
`DistanceBetweenCoords` (`(int) sqrt(dy² + dx²)`, the sum in 32 bits;
wrapped negative it gives 0x80000000, the SSE2 conversion of a NaN root,
**Inferred** from the CRT), `ListSize`, `ListMember`, `RemoveListMember`,
`ClearList`, `AddLists`, `Concatenate`, `NullAgent`, `NullFunction` and the
empty name, `ValidFunction`, `LookupFunction`, `DebugOut` (only with the
integer 911 first or engine flag 2; each argument's text and a space).
Every other builtin goes to the host, with the return slot already typed as
registered.

## World builtins

`sim::world::World` answers world builtins from its own state: all
148 (`Checked::ALL`). Every builtin the VM does not run has an answer:
`every_builtin_has_an_answer`.

HD's Freestyle traces (seed 1, size S) called 17 world builtins at
start-up, 31 in a quiet day and 56 in two days of play, each a superset of
the one before; 98 of the 166 registered names were never called
(**Measured**).

Each world answer was compared with the engine's `call` line, and a
written argument with the line that shows it, per trace: answered, unknown
(a piece whose place, sight or attributes the world did not know), wrong
and misordered calls were counted (**Measured**). A list of the same
elements in another order counted as misordered; so did a `ListMember`
element found elsewhere in the world's list (**Inferred**: the rest
unseen). With frames, the world stepped each before its first run and
walked every unit ([movement.md](../original/movement.md#in-the-world)).

- An agent's attributes are its `.dat` instance's prototype members; the
  instance is the name the engine prints for the agent. Every agent of the
  150 ms save agrees.
- Each member parses the instance's text, `""` when absent
  (`world::from_text`): integer `atoi` after a leading digit, else the
  attribute registry; float the engine's fixed-point parse; boolean
  `false`/`0`/empty or `true`/`1`, else a throw; list a string per token;
  coord two `atoi`s; function the map by name, so an absent member is the
  empty name's builtin, `$( 0 )`; agent the null agent.
- An agent prints as its piece's descriptor name while the piece stands,
  else its own name (`CreateAgent`'s second argument), else `agent(free)`;
  an id no agent has prints `??`.
- A thread's frame prints as it is now: after the palace's upgrade a thread
  started on `Palace1#6` prints `Palace2#6`. `UpgradeAgentAttributes`
  re-parses the new instance, which gives a function member its new name
  and keeps its thread fields.

"No unit" below means the agent has no live game piece. "All three" is
the start, day and two-day traces.

| Builtin | Rule | Checked in |
|---|---|---|
| `RetrieveAgent` | name map, folding case (**Game**), then the agent map; else the null agent | all three |
| `CreateAgent` | an agent under the next id from `.dat` instance arg 1; a non-empty arg 2 maps to it; the return slot stays `nullagent` | all three |
| `HasAttribute` | throws under two arguments; false for a missing agent; else the attribute map, folding case | all three |
| `GetAttribute` | throws with no unit or an unregistered key; else the unit's value, 0 when absent | start, day, five-day |
| `SetAttribute` | the same throws; overwrite or insert; hit points below 1 dispatch the death script ([below](#combat-death-and-spells)) | all three |
| `AdjustAttribute` | the same throws; the value plus the amount, written as `SetAttribute` writes; the result stays 0 | five-day |
| `MagicalAdjustAttribute` | the key and its magical component, key + `0x64000000`, must both be registered, else a throw; each gets the amount, the component first; the result is never written | none; the no-budget defeat game |
| `IsValidGamePiece` | agent alive and its handle finds a unit | all three |
| `GetUnitPlayerNumber` | throws with no unit; else its owner | all three |
| `ListTitles` | elements whose `title` equals arg 2 (case-insensitive), in order, appended; `title` from the VM's attribute object, else as made | start, day |
| `NewThread`, `RunThread` | an idle thread copies the arguments after the function and interval, frame slot 0 null; interval, mode 2 or 1, last = now regardless (`vm::threads::start`) | all three |
| `RunThreadOnce` | as `RunThread`, but a busy thread gets interval and last only: its mode, suspension too, kept (`threads::Busy`) | none; `threads` unit test |
| `BuildingIsRecruiting` | throws with no unit; else `APB:13` not 0 | all three |
| `GetVictoryConditionIndex` | the config's `RGS6`, setup slot 7, the dialog's victory condition (`freestyle::Victory`) | all three |
| `GetVictoryConditionModifier` | `RGS7`, setup slot 8, the condition's goal, gold or days (`world::GOAL_SLOT`) | none; the victory tests |
| `GetSpecialEvent1Script`, `2` | `STRT EVSC` keyed by the config's event code, the text after its number: `NONE` gives `none`; the codes from the save's setup tail. **Game**, united winter: `EV04` gives `Crunch_All_You_Want` | all three, united winter |
| `PlayMusic`, `SetMusicStoppedCallback` | the track, the script name, to the music player; no GPL effect | all three |
| `ListObjects` | [below](#listobjects) | all three, where known |
| `LocationOf` | throws with no unit; else its position in 32nds as a coordinate: a held unit's is its holder's | day, two-day, where known |
| `DistanceBetweenAgents` | throws with no unit; else the squared gap of the two pad rects ([Positions](#positions)), floored root, × 16 | day, two-day, where known |
| `SpawnUnit` | [below](#spawning-and-entering). A lair with entries picks the unit by weight on the world RNG (seeded from the session word, `seed_names`); an off-map lair's unit enters at a point on its edge, one GPL draw (`World::edge_point`), and one that picks nothing throws. The string `override` names the unit despite a lair's pick: Wrath of Krolm's avatar (`krolm_appears_when_an_altar_falls`). **Game**, united winter: every spawn to 84,950 ms | day, two-day, united winter, where known |
| `Hide` | throws unless both have units; nothing for a held unit; `#TeleportInsideDestination` enters at once (the Fortress of Ixmil, which cannot walk); else stop, a FollowPath to the entrance ([movement.md](../original/movement.md#orders)) | day, two-day |
| `InsideBuilding`, `IsHidden` | one test, one message on no unit: the unit has a live holder | day, two-day, where known |
| `Parent` | relation 101, `HOME`: its first member's agent, else agent -1, also with no unit; not the holder | day, two-day, where known |
| `GetParent` | throws with no unit; the first `HOME` member not a reward flag, its `AE:0`; none leaves the null agent | none; **Measured** |
| `IsMoving` | throws with no unit; else an unfinished movement order: false for a building (**Inferred**) | day, two-day, where known |
| `GetBestSpell` | throws with no unit; else the best ready spell of the list, a `ValidationScript` run nested where a candidate has one; `NOTHING` with none ([below](#combat-death-and-spells)) | day, two-day, five-day, dwarves, where known |
| `UnitIsOffmapLair` | false for an agent id below 0; throws with no unit; else whether `OffMapLairList` holds it: a piece at an off-map edge code | dwarves |
| `IsSpellAvailable`, `CastSpell`, `GetSpellAttribute` | [below](#combat-death-and-spells) | five-day, where known |
| `LearnSpell` | throws with no unit, or for a name the action list lacks; else the unit's list gains an entry of the code, ready, unless it holds one; never a unit's code (**Corpus**: a learned `speed_monster` is `A026`, not the effector `XR12`) | none |
| `FillSpecialLairList` | throws with no unit; the list emptied, then each special code of the lair data with a descriptor, as its name; returns the count | none |
| `SpecifyIntent` | throws with no unit; the intent to `APV:30` (a string's number: not modelled) | five-day |
| `CombatSignal` | throws with no unit; an order on track 1, the minimap's flash: 20 runs of 200 ms from push, run on push, a hit setting the count back. No stop takes it off, so the unit stays in `ORDR` some 4 s (`world::walking::Flash`). **Game**: the united winter game's peasant 19, hit, stays ahead of minotaur 18 through its builds, so its death arms in the blow's frame | five-day, united winter |
| `GetPlayerTeamNumber`, `SetPlayerTeamNumber` | throw with no unit; the owner's team; the owner onto a team ([below](#listobjects)) | five-day |
| `UnitsAreSamePlayer`, `NeutralTeamNumber` | both units or throw, owners equal; 0 | five-day |
| `CheckEffector` | throws for a name no descriptor has; no unit, false; else whether an effector of that name hangs on it: its `Wait` pending, or its end set playing (set `0x60` from frame 0, else its one set on; then it is deleted, as a spell unit's timeout ends). Its `Wait` runs before its `Animate` each frame, so the set shows one step fewer than a spell unit's. **Game**: the defeat probe finds champion 12's `Damage_Shield`, 19 frames, 1,850 ms after its `Wait` ends, gone by 2,850. Several sets and no `0x60`: gone at the `Wait`'s end (**Inferred**) | five-day, defeat probe |
| `DeleteEffector`, `DeleteAllEffectors` | the unit's effectors of the name, or all (no unit: nothing; throws); all takes the reward flags hung on it too, in hang order, each deferred, its script run with its own agent, nested | five-day, the probed play |
| `DeleteGamePiece` | throws with no unit; its effectors go; the piece stands until the engine's `UnitDelete` (**Game**) | five-day |
| `AgentHasInventoryItem` | throws with no unit; item `n` under 256 is `INV:n` in the unit's attributes; -1 searches the level's item register in unsigned key order for the unit's first item, writes it to argument 1 (`n` for `INV:n`) and answers whether one was found (**Inferred**: none found, argument 1 takes a stale stack word, left here); any other item its key (below) | five-day, play probe |
| `CreateNewInventoryItem` | the item's key: `n` under 256 is `INV:n`, another integer itself, a string its id (empty 0, a digit `atoi`, four bytes a FOURCC, else CRC-32 from all ones, not inverted (**Binary**); 0, -1 or a registered key throws), so the SDK's `"MagicBall"` is its descriptor's code; no unit, a throw. The register drops carriers no unit has; then a carrier in it (saved as `GSGL`'s pairs, **Corpus**) and no third argument (`#Allow_Cloned_Quest_Item`) throws `Tried to create duplicate quest item`; else, where the unit lacks the key or the item is cloned, the register takes the unit's handle and the unit the key at 0 | play probe |
| `DeleteInventoryItem` | the item's key; throws with no unit; where the unit carries it, the register's first entry of it naming the unit goes, then the key, then an `SC_ITEM_LOST` event no script hears. Unanswered, `Hero_Drop_Quest_Items`' and `Drop_QItems`' `while ($AgentHasInventoryItem(-1, …))` never ends (**Viewer**: two `--bot` soak runs stopped there, [openregent/game.md](../openregent/game.md#the-game)) | none traced |
| `TransferInventoryItem` | the item's key; either unit missing throws; the delete from the first, then the second, lacking the key, takes it at 0 into its attributes and the register | none traced |
| `FindInventoryItem`, `CountInventoryItem` | after the prune, the register's first carrier's agent (none: the result untouched); 0 with no key at or past it, else its carriers plus one, so 1 for a key none carries below a carried one | none traced |
| `MakeInventoryAttribute`, `IsInventoryItem` | the item's key; whether it is `INV:n` or a character's code with `Flags` `IsInventoryItem` (a building's code: unknown, its flag unread) | none traced |
| `CanDropInventoryItem`, `GetInventoryItemDropType`, `GetInventoryItemAgentType` | `INV:n`: true, 1, `Resource_Items` (**Binary**; HD's `AVk4`); a descriptor item: its `CanDropItem` (unread: unknown), its `HEAD` name; no item: false, 0, `Resource_Items` | none traced |
| `AddAttribute`, `RemoveAttribute` | no agent: true, nothing; lacking the name: the type name, upper case, to a type code (`AGENTREF`, not `agent`), 0 or 10 adding nothing, else a fresh value, the value assigned, added: true; holding it: the value assigned, false. Remove: dropped, value released. The world names them, the replay host holds the values | none traced |
| `RemoveTitles` | each element off the front; `title` equal goes to the result, any other back on the end | day, two-day, where known |
| `ListPalaces` | per player 0–7 in the level, its palace: the one it keeps, else the first building it owns coded `ABJ` or `ABm`, kept; those with an agent, appended | day, two-day |
| `SetPalace` | throws with no unit or no such player; the unit becomes the player's palace: `Palace_Death`'s outpost | none |
| `AdjustPlayerData` | throws with no unit or a field but `gold`; the owner's `APP:0` by the amount, under the gold cap | day, two-day |
| `CreateEffector` | nothing with no unit; throws for a name no descriptor has; else re-arms the first of the name still on the unit if its `Wait` pends, else a new unit taking the next handle ([below](#placing-a-building)). `after` runs the end script as the end set ends, not as the `Wait` does (`world::effectors::Ending`); **Game**: the probed play's `Lightning_Bolt_End` seven frames after its hit | day, two-day |
| `SetThreadInterval` | the interval only; the thread stays followed | day, two-day |
| `Move` | stop, then FollowUnit to a unit or FollowPath to a coordinate, run at once ([movement.md](../original/movement.md#orders)); an avoid mode with a distance marks squares round units near the start (`movement::plan::Avoid`) | probe, inn, two-day |
| `StopMoving` | throws with no unit; stops its movement | probe, inn, two-day |
| `IsAdjacent` | throws unless both have units; [Positions](#positions) | inn, two-day, where known |
| `IsEnteringBuilding` | throws with no unit; a FollowPath that ends by entering | inn, two-day, where known |
| `HasWayPoints` | throws with no unit; a FollowPath of more than one waypoint | two-day, where known |
| `GetNextWayPoint` | a FollowPath's waypoint before its next, the first while none is taken; none, the unit's position; no unit, no throw: two stack words never written (**Inferred** garbage; unknown) | none; **Measured** |
| `PathCost` | the planner once ([movement.md](../original/movement.md)), no walker moved: from and to in half cells toward zero, the aim flood counting anyone, A* counting the mode's (`check_buildings` 0x100, buildings but `BBt` chests, also any other text; `check_static_units` 0x200; `check_enemy_units`, `check_all_units` 0x400); the cost, each step's squared length, as a float; `found` whether its last node is `to`. No unit throws (`world::cost`) | none; **Measured**; the mode names: **Binary** |
| `SpecifyName` | throws with no unit; a string becomes the unit's own name: `AITX` at its message number, that empty the string. It stands before `AP:1` and replaces a number. An integer names from a `STRT` table the engine never sets (**Inferred**: nothing) (`world::naming::Given`) | none; **Measured** |
| `UnHide` | throws with no unit (`ExitBuilding() unit is null`); a held unit to the place search's cell from the holder's exit, or a coordinate on the map | inn, two-day, where known |
| `PerformAction` | throws for an action no descriptor names, or no unit; `UnHide`, a stop, an Action on track 1: once tracks 0 and 2 are empty it lasts the action's `Rate` max plus `APV:50`, and track 2 turns to the target (a projectile's toward its aim, [Missiles](#missiles)) and plays the action's set, `(frames − 1) × playback`; the actor's threads wait for both ([movement.md](../original/movement.md#track-2)) | inn, two-day, where known |
| `AgentNumber` | the agent reference for the id, alive or not | probe, inn, two-day |
| `PlaySound`, `LocalChatMessage` | true; nothing: sound and chat only, kept as a cue and a line (`world::chat::Said`, [panels.md](../formats/panels.md#game-messages)) | inn, two-day |
| `SuspendThread`, `ResumeThread` | mode bit 7 set, the remainder = interval − now + last; cleared, last = now − interval + remainder, remainder 0 | two-day, five-day |
| `KillThread`, `IsRunning` | the mode word 0; scheduled or running | five-day |
| `SetParent`, `ClearFlag` | units or throw; the child's `HOME` members of the parent's sort (a reward flag or not) go, the parent appended once; a unit's flag members go. A hero sent to a flag keeps its guild first | two-day, five-day |
| `CancelGuildMembership`, `SetUnitPlayerNumber` | unit or throw; the first non-flag `HOME` drops the unit from its roster (**Inferred**: mutual); the owner written and a count gained; the hall's icon not modelled ([The Mausoleum](#the-mausoleum)) | none; `tests/mausoleum.rs` |
| `UpgradeAgentAttributes` | throws with no unit; the unit's instance re-read: a missing member added, a non-empty text parsed; a function takes the new function, its thread kept. The world names the members, the replay host writes them | two-day, five-day |
| `RandomCoord` | a half cell drawn from the square ± max / 16 about a unit's half cell or a coord / 16, the board less 3 each side; one GPL draw of the area, x = x0 + n mod w, y = y0 + n / w, × 16. A min / 16 square clipped inside it leaves a ring: four bands, top, left, right, bottom, drawn in that order (`world::random`; max below 0 takes the board, no ring) | two-day, five-day, where placed |
| `RevealArea`, `GetNearestHiddenCoord` | [below](#explored-ground) | five-day |
| `GetInventoryItems` | no unit, nothing; else each item key of the unit's attributes appended as an integer, in key order | five-day |
| `MiniMapAnimation` | a coord, or an agent's unit (none: nothing); a descriptor name or a throw; a kind 1 unit made there: the next handle, a minimap mark flashing its set 300 twice ([minimap.md](../original/minimap.md#marks)) | five-day |
| `SetHallFlag` | throws with no unit; a slot 7 unit gets a `Champion_Icon` effector from a player whose `APP:2` names it: none here (**Inferred**: only a building's paid action writes `APP:2`) | two-day, five-day; slot 7: **Binary** |
| `LastMusicTrack` | `PlayMusic`'s track | two-day, five-day |
| `ClearDrawEffects`, `SetDrawEffects` | no unit, nothing; else the unit's tint cleared, or set from a colour (`red`, `gray`, `green`) and a transparency 1-9, neither setting nothing (`world::draw`); read back by no builtin | the probed play |
| `SetSpellUnitTimeout` | no unit, nothing; else a `Wait` of the ms; at its end set `0x60` played from frame 0, else set 0, each base part's own set on from the frame it shows, to its last frame; then a deferred delete, `UnitDelete` (`World::expiry`, **Inferred** phase). Several sets and no `0x60`: set `0x50` once from its making, then `0x40` looped (callback `0x2003`, the body of an effector's `0x100c`); other sets, unknown (`spell::end_at`). **Game**: five-day's four `farseeing` go when the world says | five-day |
| `GetPlayerData` | throws with no unit; the owner's field, case-insensitive: `gold` its treasury, `APP:0`; `palace` its first unit coded `ABJ` (not `ABm`), the return slot made an agent, agent -1 with none (**Inferred**: handle order); any other throws | defeat |
| `GetBuildingContents` | no unit, nothing; else relation 2, `INSIDE`, in entry order: each member's agent, appended. **Inferred**: a unit held since the save first, by handle. Unknown while the world loses a walker that might be inside | defeat, where known |
| `DeleteAgent` | the unit's `UnitDelete` dispatched at once, nested, then `AE:0` 0: the piece stands on with no agent. No unit: the engine reads through a null pointer, unknown. A replay compares the dispatch with the log's | defeat |
| `DeclareLoss` | an integer is the player, an agent its unit's owner (no unit: throws `$DeclareLoss(): no unit found.`); the game ends: for the local player the end record and app state 8, every sound paused; another player's is a multiplayer message. `World::ended`. **Game**: the defeat trace runs one more thread run, then ends. The loss's frame finishes; state 8 steps nothing ([a lost game](#the-games-end)) | defeat |
| `MessageFlag` | [below](#message-flags) | none; `tests/message.rs` |
| `IsMessageFlagPresent` | a message flag whose `APM:0` is the argument (a string as a number) | none; `tests/message.rs` |
| `PlaceRewardFlag` | throws with no unit; an attack flag, owner arg 2, reward arg 3, from the unit's position, hung on it, `NewUnitInit` at once (`World::flag`); refused where the owner's attack flag hangs on it already (the player's flags too); its agent, else the slot as it was | none; `tests/message.rs` |
| `GetFlag` | throws with no unit; the first `HOME` member of unit kind 4, its `AE:0` agent; none, the slot as it was | none; `tests/message.rs` |
| `DisableUnitType`, `EnableUnitType` | the descriptor by name or a throw (`DisableUnitType( %s ) failed.`); its code into the level's set or out, which the panels refuse ([panels.md](../formats/panels.md#what-the-panels-offer)); `GSGL`'s code set. Only the quests call them | none; **Measured** |
| `SetBuildingLimit`, `RemoveBuildingLimit`, `RemoveAllBuildingLimits` | the descriptor by name or a throw (`Function $SetBuildingLimit: unit type %s is invalid.`); the level's cap map, by exact code, written, erased, emptied; `GSGL`'s tail. No shipped caller | none; **Measured** |
| `ChangeUnitType`, `RevertUnitType` | throw with no unit, for no descriptor, for a unit neither building nor mobile; `APU:6` the old code, `APU:4` its `NameGenType`, `APU:5` its `DialogID`, then the type changed: descriptor, art and pad; attributes and agent kept. Revert: `APU:6` 0 throws; the three keys gone, back to `APU:6`'s code | none; **Measured**; the three keys: **Binary** |
| `GetEntranceLoc`, `GetContainerAgent` | the unit's exit or a throw; the holder's agent, else agent -1, no null test (no unit: unknown) | none; **Measured** |
| `SpawnGraveyard` | `be_dead`'s per `#Graveyard_limit` dead heroes: a building of the name for the unit's owner at a coord or its exit, searched with its ring, no price, 1 hit point, `APB:8` `GravesIndex` the level's next hero list, the owner's dead list moved there (a hero deleted with a name goes on it: name, code, level; `world::graveyard`); `NewUnitInit`; the return not written. The local owner's panel `AP15` counts the list, `AP16` lists it. **Corpus**: one save's `BBJ1`, index 0, the day before's 14 dead | none; **Measured**; the dead lists against played saves; 1 hit point, no price: **Binary** |
| `CreateSpellUnit` | throws with no caster unit, for a name the unit registry lacks (so `fire_ball` is the particle system `XL02`, not the action), or a kind not 3, 4 or 5; no target unit, nothing; else a unit at the target's aim point ([Missiles](#missiles); two GPL draws at a building), the caster's owner, the next handle; animated unless a particle system; `NewUnitInit`; its `PRIM` function (unit, target), both at once. No `SightRange`, no price, no return written. A particle system is a plume with an agent; its timeout's end stops it, and its unit goes once its particles have (`world::spell::summon`) | none; **Measured** |
| `ForgetSpell` | throws with no unit or a name the action list lacks; every entry of the code off the list, the rest in order. `LearnSpell` looks up the same list | none; **Measured** |
| `EnchantWizTower` | throws with no unit or one not `ABM1`; the player's enchantment, unpaid. On a tower enchanted it zeroes the timer's count, so the enchantment runs a whole duration more from its end | none |
| `StartEarthquakeSpell`, `StopEarthquakeSpell` | throw with no unit; a quake order on track 1, run on push, every 50 ms, `ms / 50` runs (none given: endless), or taken off. Each run moves the view, and the four other views (told to follow nothing), `min` to `max` pixels each way from where it is, on the clock-seeded paint stream: a shake that wanders. No ground, unit or damage change: `Earthquake_Active` hurts buildings and lairs (`world::quake`, `openregent` `quake`) | none; **Measured** |
| `SetEffectorDirection` | throws for a name no descriptor has; no unit, nothing; the first effector of the name on it turns its layer 0 to `n`, its slot following | none |
| `Breakpoint` | the first call ever sets a countdown from its argument as an integer; an integer counts it down, a break asked at 1 or under; `true` asks one; `"INT3"`, any case, an `int3` (**Inferred**: no debugger, a crash, `Crash::Breakpoint`). Only the debugger takes the break: release play does nothing (`world::threads::Breaks`) | none; **Measured**; the trap text: **Binary** |
| `Halt` | stops the scheduler passes and `UnitDelete` dispatch (**Inferred**: the agent stays); cheat code 1 starts them again (`world::threads::Gpl`) | none; **Measured** |
| `NoDebugMessages` | turns off `-debugout`: no trace, `DebugOut` or error report (`world::threads::Logging`) | none; **Measured** |
| `LogOn`, `LogOff` | nothing | none |
| `NewTeamNumber` | the first team with no player, else 0; takes nothing | none; **Measured** |
| `SetShareReveal` | a player the level lacks throws; its `APP:1`, then the team's ground shared: with it set, what any other of players 0–3 (**Binary**) on its team explored becomes its. `SetPlayerTeamNumber` shares for players 0–3; the cheat ALL YOU NEED IS LOVE and its undo set `APP:1`. A player's reveal mask adds each other player under the session's count (1, **Inferred** for single play) on its team with `APP:1` | none; `players` unit tests |
| `UnitIsLocalPlayer` | throws with no unit; its owner the local player; here a human seat (`World::humans`), alike on every machine | none |
| `IncrementStatCounter` | throws with no unit; `gutter` (case-insensitive) adds one to its owner's vice, the end summary's `GMTX` 106 row; any other name nothing. `GSGL` saves it (`Record::second[1]`) | none; **Measured**; `gutter`: **Binary** |
| `RevealWholeMap` | the player's own bit on every vertex, then the sight pass at once | none; **Measured** |
| `ShowMinimap` | throws with no unit; the unit's `NotVisibleInOverheadView` flag cleared for true, set for false (`World::overhead`) | none; **Measured** |
| `FadeIn`, `FadeOut` | no unit, nothing; a fade order on track 1 to level 0 or 10 at the ms: each run its draw effects' level, 0 unless mode 5, a step toward it in mode 5, colour kept; the run at it ends the order, at 0 clearing the effects. Not run on push; no stop takes it off. Wendigo blink, Ixmil's fortress | none; **Measured**; level 10: **Binary** |
| `ElvesVoice_SetOperative`, `DwarvesVoice_SetOperative` | the local player's advisor thread on while the int is not 0, here every human seat's ([below](#the-advisors-lines)) | none; **Measured** |
| `SetTreasuryValues` | the local player's low treasury thread's interval and trigger, here every human seat's | none |
| `DeclareVictory` | an integer is the player, an agent its unit's owner (any other throws); the point: no second argument the player's palace's position (none throws), a coord, or an agent's unit's (no unit: unset, unknown); the game ends won, as the loss: state 8, the point the victory star's and the view's (`World::end_point`) | none; the victory tests |

**Measured**: over the traces, the world answered every call the engine
made, and agreed with it, but for answers the probe's frame model placed a
frame late. An agent its nested `UnitDelete` ended prints `??` at once:
`Player_Dead_Cleanup` lists `??#2` in the run whose `DeleteAgent` ended it.
In play:

- `CanIBuildThisBuilding` runs on the build menu's preview, which follows
  the pointer: its place is the player's, logged nowhere.
- The rules these answers need are the scheduler's model (a deleted agent
  leaves the queues, a hero joins once, [Play's input](#plays-input)),
  the engine's dispatches between passes
  ([Where an engine dispatch falls](runtime.md#where-an-engine-dispatch-falls)),
  the walk rules of [movement.md](../original/movement.md) and the
  missiles' aim ([Missiles](#missiles)).

### The advisor's lines

Each frame in play, after the hit point tick, the engine polls four
threads of the local player (`world::advisor`, **Binary**): each, while on,
notes the clock, and runs when the clock less its last run is over its
interval (a GPL constant read as the player is made), its last run then the
clock. A line is phase `GVS5`, unplaced, the local player's. Here each
human seat has its own four, polled in seat order; a line is its seat's,
heard by it alone (`Cue::hearer`).

| Thread | Interval | Runs | Line |
|---|---|---|---|
| `Thread_LowTreasury` | `#advisor_timeout_low_treasury`, 180,000 | `APP:0` under `#advisor_trigger_amount_low_treasury`, 1,000 | `RA09` |
| `Thread_NeedHeroes` | `#advisor_timeout_need_heroes`, 60,000 | no hero among the mobiles | `RA11` |
| `Thread_ElvesAvailable` | `#advisor_timeout_elves_available`, 20,000 | `ABc1`'s `BDEP` met, not disabled, under its cap: says it with no `ABc` standing, then off | `RA02` |
| `Thread_DwarvesAvailable` | `#advisor_timeout_dwarves_available`, 20,000 | the same for `ABb1`, `ABb` | `RA03` |

A 20,000 ms thread runs at 20,050, 40,100…; the `GSGL` player block saves
the elves' and dwarves' interval, last poll and on word, not the last run.
**Game**: the two-day trace's, the five-day trace's and the probed play's elves went off at 100,250,
120,300 and 140,350 ms, the dwarves' on; the world's agree
(**Measured**). **Inferred**: the heroes'
list is the world's mobiles. A world runs them with
`World::with_advisors`.

### Message flags

`MessageFlag(agent | coord, [message])` (`world::message`): an `MSGF` unit
of descriptor `ARE1` `message_flag`, owner −1, the next handle, no agent,
no place search: at the coord, or hung on the agent's unit as a reward
flag hangs (relation 1, stacked among its overlays); no unit throws
`MessageFlag() unit is null`. It plays set `0x40`; `APM:0` `Message_Text`
takes the message unless 0; `ME02 New_Message` sounds, unplaced, the local
player's. Every player sees it (its sight words 100). Selected, its panel
shows `AITX[APM:0]`: `#Message_Found_Gold` 249 is the found gold. A double
click sends `DestroyEventFlagMessage` (`Input::Read`); its decode deletes
the unit at once. It goes with its holder. The option (default on) that
sets mini view 1 to follow each new one is the interface's
(`view::panel::tracking`,
[panels.md](../formats/panels.md#the-tracking-window)). Its wave order
loops set 300 for good: the loop count is 2 until the first wrap, then 1
(**Measured**). A load dates the flag by that counter, not by
set `0x40`'s `Animate`, which has wrapped: **Corpus**, all 10 saved flags
(**Measured**). Past a wrap only the
phase is known. Not modelled: its place in list `ORDR`, which no script
reads. A string message reads as a number only when it opens with a digit
(**Inferred** unread otherwise; no shipped call passes one).

### Combat, death and spells

**Binary** unless marked. `world::combat`, `world::spell`;
**Measured** against the traces.

The engine keeps no attack rule: to-hit, damage and armour are the
scripts' (`make_attack`, `hit`, `damage`), each roll a `RandomNumber`. It
dispatches an action's GPL function when its animation peaks
([movement.md](../original/movement.md#track-2)), and does the write:

```text
 SetAttribute, AdjustAttribute
   store; APU:0 below 1 on a unit with AE:0: Unit_Call_Deathscript(agent)
   at once, nested in the builtin; it runs IGDeathScript (gravestone ...)
 each 1000 ms of game time, after the clock step, before the orders
   count = (count + 1) mod 5; every mobile, every building
   PoisonCounter > 0: every tick APU:0 - 1 (dead: counter to 0)
   Flags Heals: count mod (5 + APV:33) = 0: APU:0 one toward MaxHP
 a missile at a building: two GPL RNG draws as its actor turns, two as
   it launches ([Missiles](#missiles))
```

- The tick's words come from `globals.gpl`'s constants and are saved in
  `GSGL`. **Game**: from the new game's, it gives the day, two-day and
  five-day saves' last tick and count.
- The death dispatch: **Game**, every death in the five-day trace runs nested in
  `damage`'s or `spelldamage`'s `AdjustAttribute`; `DeleteGamePiece` later
  queues the unit, and `UnitDelete` (an integer id) ends agent and piece.
- A hero's grave is the hero. `gravestone` (the SDK's `mx_Hero_Deaths.gpl`)
  sets `type` `Dead`, makes `ActiveScript` `be_dead` at 10,000 ms ×
  (`ExperienceLevel` / 3 + 1), at most 90,000, and plays `Basic_death`;
  its peak lies the unit down in set `Dead`, the gravestone
  ([movement.md](../original/movement.md#track-2)); `be_dead` drops its items and
  `DeleteGamePiece`s it. A monster's `monster_gravestone` alike, 10,000
  ms, `be_dead_2`. **Game**, the five-day trace: ranger 45, level 3, dies at 262,100 ms,
  `be_dead` 20,000 ms on; a full guild's recruit runs ranger 41's at once
  ([Play's input](#plays-input)). **Corpus**: the probed play's save, four
  dead monsters in `Dead`; another played save, a Monk's
  grave. A resurrection's `ClearEngineDeathFlags` clears the lying bit
  (`World::raise_dead`). With a Mausoleum, `be_dead` inters instead
  ([The Mausoleum](#the-mausoleum)).
- A missile's draws: `Info` `NoRandomTargetOnDestination` skips them.
  **Game**: the five-day trace's one, a ratman shaman's `Infectious_Cloud` at an inn:
  two unlogged draws before the next logged one, four before the one
  after the launch; one fewer or more does not fit
  (**Measured**).
- A character's `SECD` ends in four lists (`desc::Character::allowed`):
  one empty on every HD character, `AllowedWeapon`, `AllowedArmor`,
  `AllowedSpells`. A new character takes `APV:25` and `APV:26` from the
  first weapon and armour type, `APV:29` from `Poisons`, and a spell entry
  for each spell: its `DACT` id, cast at the clock, timeout 0.
- `CastSpell`: the entry's cast becomes the clock, its timeout the
  action's `TimeoutDuration`, then `PerformAction`; no entry, nothing.
  `IsSpellAvailable`: the entry ready (or argument 3 not 0) and
  `CharacterLevel` at most `APV:11`. `GetBestSpell`: of the ready entries
  of the asked `SpellType` within the level, the highest `SpellRank`, the
  first on a tie. The walk is in entry order: a candidate beating the best
  so far that has a `ValidationScript` runs it at once, nested, on the
  unit, and is kept only if it returns nonzero. The world stops there with
  `Answer::Asks`; the host runs the script and the world goes on
  (`World::resume_ask`). **Game**: the dwarves game's shadow beast,
  `SBeast_Speed_Check`. `GetSpellAttribute`: one of four descriptor words
  by name.
- `SetSpellUnitTimeout` is a `Wait` on a spell unit, not an entry: it
  ends the unit (`world::spell`, `World::expiry`). A farseeing's art is
  one set of 17 frames at 100 ms and no set `0x60`, so after its 2,500 ms
  it plays on from frame 8 to 16 and one run more: `UnitDelete` 3,400 ms
  after its decode, frame f + 68, for all four (**Inferred**: the set has
  looped since the call). Gate's art has `0x60`: its nine frames from 0.
- A hero's level is the scripts' (`hero_birth` sets `APV:11` to 1,
  `give_exp` adds). **Game**: play's 2,577 `GetBestSpell` calls (the `PLAY`
  pin) and the five-day trace's 417 `IsSpellAvailable`
  (**Measured**) all answered
  right.
- An action's function runs after the frame's tick
  ([Where an engine dispatch falls](runtime.md#where-an-engine-dispatch-falls)).
- The five-day trace has no wrong answer once research, flags and the panels' messages
  are placed ([Prices and research](#prices-and-research)); the probe's
  frame lags the clock, by hundreds of frames at the save, but the probe's
  frame model placed runs by the game's.

### The Mausoleum

**Binary** and SDK (`GPLMx/TaskModules/Buildings/Mausoleum.gpl`,
`mx_be_dead.gpl`). `world::guild::mausoleum`, `tests/mausoleum.rs`. No trace
holds one; no corpus save has an `ABp1`.

```text
 death     gravestone: the grave, be_dead its grave time later
 be_dead   members -= hero; Check_Mausoleum(hero):
             Monk, Healer, Paladin ("Title"): never (Dauros, Agrela)
             ListObjects building, #MyPlayer, "Mausoleum", APB:1 = 1;
             the first with "occupants" < #Mausoleum_Limit (10):
             CancelGuildMembership, Hide #TeleportInsideDestination,
             "Type" Dead, intent 278, ActiveScript killed, occupants << hero
           none: the grave goes (DeleteGamePiece) as with no Mausoleum
 panel     Mausoleum_Controller: GMTX 185 "%d Heroes buried here";
           MX05 lists relation 2; a row's price Mausoleum_Resurrect_Cost
           = max(1000 + 250 level, #Mausoleum_min_res_cost 2000)
 resurrect GUILD_ACTION 0x15: Mausoleum, hero, price. The hero's owner's
           APP:0 at least the price: taken; Mausoleum_Resurrect_Begin:
           ClearEngineDeathFlags, occupants -= hero, UnHide, XR11 effector
           1700 ms; its end Mausoleum_Resurrect_Finish: scripts back,
           full hit points, "type" original, Find_New_Home (a guild with
           room, an outpost, an embassy; foreign ones on a loyalty roll)
```

- The grave shows first: interment is `be_dead`'s, at the grave's end
  or a full guild's dismissal.
- The decode reads neither the Mausoleum nor whether the hero is in one.
- `CancelGuildMembership` and `SetUnitPlayerNumber` (`adopt`): see the
  [table](#world-builtins).
- A finished Mausoleum's fall spawns 6 to 11 zombies and skeletons,
  maybe a vampire, then runs `be_dead` on each interred, which may inter
  it in another (`Mausoleum_death`, `release_occupants`).
- **Inferred**: in play; `tests/mausoleum.rs` inters a wizard, refuses a
  paladin and raises the wizard, every builtin answered.

### A building falls

**Binary** unless marked. `world::ruin`, `tests/ruin.rs`.

```text
 APU:0 below 1  deselected, unless APB:7 (a raging palace)
                Unit_Call_Deathscript, nested     building_death:
   type "Dead", occupants out, PerformAction(Become_Rubble), DeleteAgent
 DeleteAgent    UnitDelete dispatched, AE:0 written 0; the unit stays
 Crumble's cue, else last frame: the unit lies down, deselected
   ruined bit, the place search, as a dying hero's
   APB:2 0, an order of #RubbleCleanupPeriod (60,000)
 that order ends: the ruin deleted
```

- A building's `Crumble` is timed apart, as an `InitAction` from the
  `PerformAction`: the guild's 21 frames at 100 ms collapse it 1,900 ms
  on. Its other actions run on tracks with no movement
  ([movement.md](../original/movement.md#track-2), `World::fixed_tracks`): a tower
  shoots.
- **Game**: in the defeat game the guild and the library collapse
  1,900 ms after their `PerformAction`, at the save's rubble orders,
  340,450 and 348,550, once the scheduler's model's lead is taken off:
  1,100 ms here, the same three damage shields' `Wait`s show; the GPL RNG
  has parted by then (entry 781,904). The tower under construction that
  fell first is gone by the save, its minute up.
- A ruin has no agent: `ListObjects` lists none. **Inferred**: as the
  engine, whose search returns agents.
- The collapse lies the unit down as a dying hero's peak does: a place
  search from the building's half cell, margin and flags 0, excluding only
  itself, so a unit on its pad moves the ruin (`World::lie_in_place`,
  `a_collapse_searches_the_ruins_place`). **Game**: the united winter
  game's `Temple_Dauros1` stood at (256, 3168), its ruin at (304, 3168);
  the defeat saves' ruins kept their spots. What stood on the temple's pad
  is unknown: no positions.
- A ruin's delete counts nothing: its hero, henchman and monster counts
  are a mobile's; a building sends only a panel refresh
  (`World::clear_rubble`, `a_ruins_delete_counts_nothing`).
- The selection drops the building at its last hit point, before its
  death script, and again at the collapse (`Output::deselects`,
  `a_building_at_no_hit_points_is_deselected`). A hero at its last hit
  point stays selected. From `DeleteAgent` on, the building is fallen
  (`GenUnit` 3), so a click refuses it (`World::fallen`).
- A recruit running as its guild falls makes no hero: its order's end
  makes one only while the guild stands. The price stays paid
  (`World::recruit`, `a_fallen_guild_recruits_nothing`).
- The collapse is one callback for every unit: a building that acted
  before, so has tracks, collapses all the same, its rubble order started
  (`World::dying_done`, `a_building_with_tracks_falls_to_rubble`).
- A deleted unit takes the units inside it: freeing its relation lists
  clears each type 1 to 99 member's holder and deletes it at once,
  `INSIDE` too. `building_death` lets out only its `occupants` list; a
  tax collector asleep inside (`bring_home_goods` with no gold) is not on
  it, so it stays in the ruin and goes with its rubble or a placement's
  clearing (`World::inside_go`, `a_ruins_delete_takes_the_units_inside`).

### Particle systems

**Binary** unless marked (`world::plume`). No script makes them; each
takes the next handle, draws on neither RNG (particles have their own) and
joins no `ORDR` without a `Wait`.

- A set naming one ([art.md](../formats/art.md#a-set-names-the-particle-system-its-parts-carry))
  makes one per part as it plays and stops the old set's. So a building's
  damage stage (set at every attribute write; `Dead` at hit points below 1,
  before the death script; `Crumble`), a missile's first set (`0x40`, else
  8, else the first) and its impact set `0x60`, played as it strikes; the
  missile stands until that set ends (`world::plume::Spent`). So a
  building's damage stage smokes on its fires, and a ratapult ball trails
  `Ratasmoke`.
- A stop: off its holder; unless a finite emitter is still emitting, its
  window restarts as the particles' lifespan. Past its window the system
  is gone: the particle manager, run after the orders' deletes, frees it;
  a stopped unit is queued and deleted the next frame, a held one stays
  empty.
- `CreateEffector` of a particle system: a unit at the holder's
  `AttachmentPointID` (y plus 1), never re-armed, its `Wait`'s end a stop.
  **Game**: united's ice breaths' 0 ms `Icebreath_hit` on the palace.
- An effector's art makes systems as any unit's: its first set's as
  made, set `0x50`'s, and `0x60`'s as its `Wait` ends
  (`World::effector_vents`). **Game**: the five-day trace's 0 ms
  `energy_blast_effector` makes `XL08`, agent 56's missing handle.
- **Game**, the dwarves game: a damaged building's shot remakes its
  stage's systems, new handles each shot. **Binary**: the shot's set
  is refused and no part's set changes, and replaying a shown set changes
  nothing, so how the shot remakes them is unread.
- A 0 ms effector is not re-armed, even in its frame: the dwarves game's
  two `wounded_large` on agent 56 at 293,500 ms take two handles; its end
  script runs inside `CreateEffector` (the five-day trace). **Binary**: a 0 ms `Wait`
  runs as it is pushed and ends in its first run.
- An effector's or spell unit's end with no set `0x60`: its one set
  plays on, layers of one kind (`id & 0xffff`) being one set
  (`spells::end_at`). **Game**: a 0 ms `Got_Gold` (16 frames at 100 ms)
  stands 1,600 ms; those at the five-day trace's and the two-day trace's saves are each younger, none
  older.
- A spell unit's first set's systems: farseeing trails `XL23`. An
  action's `Overlay` hangs on the actor for 0 ms at its peak
  (`desc::Action::overlay`).

So the defeat, dwarves and united saves' handles are the engine's, and
each record is a world piece, plume, effector, flight or spent missile.

#### Particles

**Binary** unless marked (`view::map::particles`, `world::particles`). The
particle manager updates each system once a frame, last made first, on the
game clock in seconds; the first update steps 1e-5 s, then 0.05.

- First update: `InitialParticles` born. Past the window: all die. While
  emitting (endless, or game ms since the window's start under the
  emitter's lifespan, 0 after a restarting stop): `Rate × step` added to
  a fraction; its whole part born (truncated), one per free slot from a
  cursor, at most `MaxParticles` live.
- A birth: speed `InitialSpeed × (1 − variance × r)`; azimuth
  `r × (2π − 1°)`; tilt off `+z` and start by `Type` bits. `Area+Column`
  (16 of 22, `no_shipped_system_recycles_or_spins`): distance out `r` of `Radius × (1 − RadiusVariance)` to
  `Radius`, tilt `DirectionVariance × that / Radius`, start that far out
  plus up to a cell along the heading. `Point+Column` (5): tilt `r` to
  `DirectionVariance`, at the emitter. `Area+Disk` (`XL28`): tilt 90° ±
  `DirectionVariance`, start that distance along the heading. Place: the
  unit's position plus `Offset`; velocity: the emitter's plus speed along
  the heading. Life `Lifespan × (1 − variance × r)`. `SurfaceNormal` is
  turned into angles no birth reads.
- An update: `Offset` moves by `Velocity` and `Acceleration`; each live
  particle dies past its life or outside `Bounds` about the emitter; else
  velocity `+= Gravity × step`, place `+=` velocity `×` step,
  `+ JitterDisplacement × r` per axis.
- Colour, alpha (`FadeTime`), size (`SizeTime`) and spin are worked out;
  the draw reads only the alpha: below exactly 1, mode 8 at level
  `(1 − alpha) × 9` truncated (`blend::fade`). So smoke glows in and out.
- Drawn: each particle is a part of its system's unit, a frame of the
  system's own art (`ImageIDBase`) in the set its unit plays, blitted at
  its own place as any frame. Slot 0, or a draw over the set's slots;
  frame 0, stepping by the slot's playback word 2, else the set's, round
  at the last. `Smoke_Large` (`XLSL`): one `Special` slot, 38 frames at
  100 ms. It sorts in the draw list by its own place's y, so it draws over
  the shadows of things further back.
- The RNG: `state × 0x19660d + 0x3c6ef35f`, a draw its low 31 bits over
  `2^31`; the slot draw is the world RNG's formula on the same state.
  Seeded from the clock as the program starts, so not reproducible:
  `openregent` seeds it with the GPL seed (**Inferred** stand-in). No rule
  reads a particle, so the world keeps none: it records each frame's pass
  (`World::particle_sweep`), a front end steps them, and a save holds
  none. A birth draws 4 to 6, a slot 1 more, a live particle 5 an update
  (`JitterOrientation`'s two kept nowhere).
- Not modelled: the clock's wrap past 49 days; a particle effector's
  attachment (**Inferred** 0 for a set's systems); a birth's size factor
  (no file field; **Inferred** 1 and 0); `Flags` (`IsPausable`,
  `IsPerformanceHog`) and the render switches; the draw order among other
  entries (**Inferred**: by row).

### What an effect shows

**Binary** unless marked. `world::effectors`, `World::effects`.

```text
 made          the effector's layer 0 takes its holder's part 0 direction
 placed        the holder's attachment point, else set 0xd0 on a building
               or 8, point 3; less the holder's place, y + 1; then stacked
               by StackPriority
 effector      set 0x50, then 0x40; else 0x40; else none, the art's first
               set; not for a 0 ms Wait
 its end       set 0x60 from frame 0, else the set plays on
 spent missile set 0x60 from its impact
 spell unit    set 0x50, then 0x40, else 0x40; 0x60 from its Wait's end
 draw list     drawn in board y order, a tie after: y + 1 puts an overlay
               over its holder; a set with layer flag bit 0 adds its
               screen-pixel nudge
 iso view      one unit pass, that list front to back
```

- A frame each set's `action::frame_ms` from the time it began.
  **Inferred**: an intro ends as its last frame's time is up, as an end
  set's deletion does.
- An intro's end sets 0x40 with a reset: the loop faces slot 0, its
  holder's facing gone (**Game**: the call to arms probe's four
  `teleport_effector`s).
- A record writes its last layer's direction; a layer above the base
  keeps 0: `Got_Gold`'s two layers save 0 on a hero facing 24.
- **Game**: every effect record in the five-day, two-day, defeat and
  dwarves saves is the world's effect of its handle in set, frame,
  facing, place and height (**Measured**).
- `Got_Gold`'s and `got_gold_bldg`'s second layer is no picture: its
  frames are reference kind `0x08000000`, drawn as text, at view zoom 1.0
  only: the value `CreateEffector` was given, `%d` in font `SM09`, colour
  `0x80ffffff`, rising with the frames' nudges, 0 to 48 px. Every shipped
  call gives one; the last integer option is the value
  (`Effector::value`, `FrameSource::is_text`, `view::map::amount`). The
  font is `data::font`.
- Draw order: no later pass draws overlays. Reward flags sort far
  nearer by their set's nudge, 2,000 and 1,500 px (76 and 57 cells), so
  over all about them; thought bubbles and `Got_Gold` have none and sort
  at their holder's y plus their offset (`Set::sort_nudge`).
- A tie draws after the parts listed before it; the order units join
  the list is not read. openregent orders a tie by kind, then id, lower
  behind (**Inferred**); missiles over units (**Game**, the user's
  recollection). See [shadows.md](../openregent/shadows.md#three-things-the-data-forced).
- An effector's systems take the handles after its own: the unit is
  numbered, then vents. `energy_blast_effector`, set `0x60` alone, `XL08`
  on it, shows its 500 ms burst on a target that lives. A kill shows none:
  `Unit_Call_Deathscript`'s `DeleteAllEffectors` defers the delete, and
  the hit runs in the orders, whose deletes free it, `XL08` with it,
  before the frame draws. (**Game**, the user: the burst shows on a kill;
  open.)
- An overlay is held with its holder: **Corpus**, every overlay record's
  state bit 1 is its holder's (196). Hung is not held: a hung flag's
  state is its `DontBlock` alone (the five-day trace's #1007).
- Placement, checked for thought bubbles: the screen place is the unit's
  position projected, a held overlay's its holder's plus its own; the
  blit takes the tile's anchor off; nothing else moves it. `ARG1`'s tile
  anchors at its top left, at the head's point.
- Not followed: the building fallback to set `0xd0` (the world reads set
  8).

### Speed trails

**Binary** unless marked. `world::trail`, `World::clones`, **Measured**.

```text
 TurnOnSpeedTrail(unit, n, [rate])   APU:13 n on the unit; n clones, each
   a unit of its code and owner, the next handle, DontBlock, not live: no
   agent, own spot; held (relation 1) by the clone before, clone 0 by the
   unit; mode 5 at L0 + (10 - L0) i / n, L0 the unit's level, its colour
   kept; hidden; a follow order on track 1, run at once, so in ORDR;
   APU:8 the unit's handle, APU:12 i
 the follow order, every rate ms (0: each frame)
   the clone takes its leader's place, set, slot and frame as last
   recorded, then records the leader's now; shown where it no longer
   stands on its leader; the level again from the unit
 TurnOffSpeedTrail(unit)   the unit's first clone deleted, deferred;
   its holder's drain takes the rest
 HasSpeedTrail(unit)       whether a clone of its code hangs on it
```

- No RNG: a clone makes no slot draw. A clone has no `SightRange`;
  **Inferred**: so it reveals nothing.
- A unit deleted takes its trail at once; `DeleteAllEffectors` and
  `DeleteGamePiece` delete it deferred, as effectors.
- **Corpus**: the clones in played saves are
  the engine's: state `0xe0` (`0x20` `DontBlock`, `0x40` not live, `0x80`
  its own spot), `0xe2` hidden exactly where they stand on their recorded
  leader, the handle before their own their holder. The follow order is
  `OrderFollowClone`, order type `0x4000`.
- **Game**, `openregent` Wizb Cona Mayh map 1: `Speed_Monster` trails its
  target with two clones after the spell's effectors; clone 1 stands
  where clone 0 stood a frame before (**Measured**).
- **Inferred**: the world keeps a walker's look, not its frame, so a
  clone copies what the world shows of its leader; `openregent` runs its
  frames as far behind the unit's as its look is old.

### Missiles

**Binary** unless marked. `world::missile`; `tests/missile.rs`.

```text
 the Action arms   projectile and a target unit: turn toward the target's
                   aim point, in 32nds: position + Hit + scatter
   Hit             Hotspot (set 400) slot 0: x of point 1 (attachment 2),
                   y of point 5 unprojected; else set 8's point 3 x in
                   the drawn slot, y 0
   scatter         a building, projectile without
                   NoRandomTargetOnDestination: x + rng(w / 2) - w / 4,
                   then rng(h / 2) for height; w, h the pad in pixels
 cue, else last    the peak: target unit gone ─> nothing
                   a unit of kind 1, the next handle, no GPL agent, at
                   the shooter's launch point; homing on the unit, whose
                   aim point draws again at a building
 each frame        MoveMissile: goal = target + offset while the target
                   stands, else the last; farther than a step: a step in
                   3D; else on the goal, the impact
 impact            the projectile's function by its Script type:
                   0 (shooter, target), 1 (shooter, point), 2 (target),
                   3 (point); an agent gone, nothing
```

- Launch time: the `InitAction` steps one frame a run from frame 0, so it
  shows frame 1 as pushed; its peak fires once, stepping onto a frame
  whose reference has bit 31 (`imag::CUE`), else reaching the last:
  `cue − 1`, else `frames − 2`, runs after the push, at once for two
  frames or fewer (`an_init_action_peaks_at_its_last_frame`). **Game**:
  the ratman shaman's `Cast` cues frame 4 of 9; at the cue every line of
  the five-day trace matches (`a_cast_launches_at_its_cued_frame`).
- The aim point, a pair of draws at a building, is taken as the action
  turns its actor, as a homing missile is made, as a lobbed one fixes its
  point, and by `CreateSpellUnit` (`world::spell::summon`). No other code
  asks for it.
- The peak's target is a unit, not an agent: a building razed under the
  cast, its agent deleted and its unit standing until its rubble clears,
  is still shot at, two draws at its launch. **Game**, the defeat probe:
  shaman 15 casts `Infectious_Cloud` at wizards' tower 4 at 399,050 ms;
  champion 12's `make_attack` razes the tower in the next frame's orders;
  the cloud leaves at its cue, and the engine's next roll is the one after
  those draws (**Measured**).
- A missile has no agent: agents are made for descriptor kinds 2 and 3,
  some 5, and a unit of kind 4 (a flag); a projectile is kind 1. So
  `ListObjects`, `LocationOf` and kin never see it; it only takes a
  handle.
- Every HD projectile homes but the catapult's ball, whose
  `Parabolic_Missile` is lobbed: bit 2 of the class's `PRIM` word 7
  (`movement::Class::flags`, [movement.md](../original/movement.md#fliers));
  only `Parabolic_Missile` has it (6 there; 2 on the other missiles,
  `every_projectile_homes_but_the_catapults_ball`).
- The lobbed flight (`World::lobbed`, `world::missile::Lob`): the point is
  the target's aim point (the two draws at a building), then a physics
  order: the angle `ParabolaAngle` 75° as table entry 53, gravity
  `-ParabolaGravity / 32` (HD: 4, so −1/8 cell), the launch speed from the
  parabola through the point. Refused (straight down, or rise over run
  past 0.9 × tan), it flies a `MoveMissile` straight to the point,
  following no one, and the impact names no target (`World::straight`;
  **Measured**). Each run adds the velocity to the point, then gravity
  to the height's; it lands passing the point's height, rising or falling,
  and the impact runs with no target. **Game**: the no-budget defeat saves
  hold one ball at 18 and 22 runs, 22.3 and 20.8 cells up: run back, it
  left the catapult's launch point at its cue; 32 runs on it is where the
  probed twin's save has it landed, the log's impact point
  (**Measured**). Its first run is a frame
  after the launch (**Inferred**: no rate is read). `ParabolaGravity` and
  `ParabolaAngle` are read only here.
- A unit's height is a walker's own, on its step's line between nodes, not
  the ground under it: the lob starts from the missile placed at the launch
  point. A missile faces its target's position from the launch point as it
  is made, and keeps it. **Game**: the defeat probe's save holds two spent
  balls; catapult 16, 304/65536 cells above the ground, lobs ball 954
  1/65536 a run slower each way, landing 27 short of the ground's model,
  as saved; the balls face 8 and 11, the palace's and guild's middles, not
  their aimed points (**Measured**).
- **Game**, the five-day trace probe-framed, with `CastSpell` answered: `make_attack`,
  `wandering`, `Advanced_Attack` and `inn_revenue` do not stop on a draw
  (`PLAY5`).
- The offset is 3D. The `Hotspot` height is the point's art y over point
  5's; set 8's the point's own y; a building's scatter adds
  `draw(h / 2) - h / 4` to it, never below 0. Art pixels of height become
  cells as 32 × the cosine table's entry 39 at regular zoom
  (`world::missile::rise`, `CAMERA_COS`; **Inferred**: the player kept
  that zoom).
- Launch point: point 4 of the slot drawn of the shooter's set, `Cast` or
  `Attack` as it plays, else `Attack` (`world::missile::Launch`).
- The `MoveMissile`: rate the projectile's `DMOV` (no attributes, no
  modifier), pushed on track 0, not run on push. The missile joins `ORDR`
  after its shooter has run, so it first moves the next frame. `fast
  missile`, every projectile's movement in the five-day trace, goes 1/32 cell each 1 ms:
  1.5625 cells a frame (`a_fast_missile_flies_a_cell_and_a_half_a_frame`).
- The impact runs in the frame's orders: a log shows it as a dispatch
  after pass f, landing in frame f + 1. A projectile with no function is
  skipped. `make_attack` at a target with `HasEffectMagicMirror` turns
  back homing on the shooter, as actor and target; else the missile plays
  set `0x60` and the function runs by the `Script type`
  (`desc::Unit::script_kind`). An arrow's function is `make_attack`: a
  ranged attack's damage is its impact. Magic Mirror does not return other
  callbacks, such as `Acid_Bolt_Hit`; GPL `spellhit` instead resists them
  (`defenderavoid = 105`). The icon ends the effect after 23,000 ms
  (**Measured**).
- A target gone in flight: the goal stays; at the impact its agent is
  gone, so nothing runs. A shooter gone: nothing.
- **Game**, the five-day trace probe-framed: the world dispatches 37 impacts, the
  log's 37 (28 ranger arrows, six infectious clouds, an energy blast, the
  guardhouse's two arrows at ratman 19, a building's Action on
  `World::fixed_tracks`), each in the log's frame (**Measured**). One, the cloud at the inn, is at a building.
- A missile is a unit, so it joins `ORDR`'s back with its `MoveMissile`
  and runs among the units in that order (`World::fly`).
- A missile at a point (`CreateMissile` at a coordinate, an action aimed
  at one) has no target. Homing, it takes the point on the ground and
  flies it straight; lobbed, it is lobbed there. No draws; it faces the
  point; its impact names no target, so a function that wants one does
  not run (`World::at_point`; **Measured**). `CreateMissile` at
  anything else throws `CreateMissile() has no valid target`, after the
  descriptor's lookup.
- An engine dispatch in the frame's orders, an impact's or a peak's
  action function, runs at once, before the rest (`World::resume`): draws
  fall in the engine's order. **Game**, the defeat probe at 385,450 ms:
  shaman 15's attack peaks, its `make_attack` drawing, before shaman 14,
  behind it in `ORDR`, launches its cloud, drawing twice.

### Spawning and entering

**Binary** unless marked. `world::spawn`, `world::housing`; **Game**, the
probed day: three spawns, **Measured** ([the probe](#what-the-probe-showed)'s door
is this exit's placement).

```text
 SpawnUnit(spawner, name)     descriptor by HEAD name, or throw
   where  coordinate argument, else the spawner's exit: art set 0xd0 in
          the slot it draws, block 2's second point, y unprojected
          (sim::placement::exits); a held spawner's holder's
   made   attributes (world::bodies::initial), relation 100 to the
          spawner (so HOME), the place search, then NewUnitInit
   then   the game view's two IsValidMiniViewTarget runs (interface)
 Hide(unit, building)         a walk to the entrance; the walk is the
                              movement's
 next frame                   inside: holder = building, state HELD;
                              its own position kept
```

- The place search (flags 0) floods half cells from the exit's (32nds
  over 16, toward zero), nearest first by an MSVC 9 heap
  (`space::search`), to the first where no cell the unit takes is taken
  on its layer. A unit takes its polygon's mask, else its pad rect
  half-open (`sim::placement::taken`); every unheld unit, scatter
  included, is registered over the same; two block when their `Info`
  shares bit 4 or 5; a `DontBlock` unit is not on the grid. A resource
  plant, `AVk1`–`AVk3`, is searched with its own test: any occupant
  blocks it, whatever its layer (`World::spawn`, `Blockers::Anyone`;
  **Game**: the Valley of the Serpents' and the Vigil's healers'
  `Seed_Resource`). Off the map blocks. **Game**: the palace's exit is
  (480, 3785); the two peasants land at (448, 3808), the tax collector,
  whose rect reaches up, at (448, 3824).
- The agent is the next id, the piece the next handle from the level's
  counter (**Inferred**: it starts at the save's highest; **Game**:
  531–533 in the day save), owner the integer argument or the spawner's.
  Attributes: the descriptor's, then the list's pairs (**Inferred**
  order); its spells its `AllowedSpells` ([Combat](#combat-death-and-spells)).
  A spawn the world cannot place still takes these. A descriptor with a
  `NameGenType` draws `AP:1` ([below](#placing-a-building)).
- It faces north, 0 (`world::walking::FIRST_FACING`): a facing is the
  animation parts' slot, zeroed as a part is made and kept through set
  changes; the unit draws a slot on the world RNG as it is made but
  applies it only to a `Directionless` descriptor. **Game**: every unit
  spawned and never walked faces 0: the day saves' three, the inn save's
  tax collector (**Measured**).
- A building (place flags 1, margin `ConstructData1`) searches with the
  margin, and its test is the ring its placement clears, not its pad; what
  a building clears lets it through. `maxhp` makes it whole, else 1 hit
  point. **Game**: play's house, spawned at the palace's exit, lands on
  its save's half cell 13 cells east, 3 south (`world::World::place_ringed`).
- A Temple to Fervus's animal is the scripts' alone: `Rest_at_Fervus`
  (`Fervus_Lived_In.gpl`), a cultist resting at home whose
  `Num_Followers` is under `(its level − 1) / 3 + 1`, spawns the temple's
  `Spawn_Type` at the temple, the monsters' (`#Monster_Player`), when
  `RandomNumber(100) + 1 < #Percent_Chance_Fervus_Spawns_Animal` (50,
  resolved at load; the executable holds no such string). `Spawn_Type`
  is the level's `.dat` instance: `Giant_Eagle`, `White_Wolf`,
  `Red_Bear` (**Game**: a roc, a varg, a hell bear). The cultist's
  Charm Monster then makes it its own (`Control_Monster`,
  `SetUnitPlayerNumber`). **Game**: the market probe's level 3 temple's
  `Cultist#11`, `Red_Bear#35` in frame 4220, charmed in 4245
  (**Measured**).
- Every spawner has lair data (a default made on a miss;
  [saves.md](../formats/saves.md#gsgl--the-levels-state)): with pattern
  entries the unit is picked on a world draw, else the script's name
  stands. The world reads the new game's `GSGL`; **Corpus**: the traced
  fresh game has none, so its lairs spawn by name, and the two-day trace's save
  holds its three spawners as defaults. A chart lair's data is its
  spawner's whole: entries, and its special codes, which
  `FillSpecialLairList` names, a falling lair's last spawns. **Corpus**:
  every freestyle start save's lairs are the chart's
  (**Measured**).
- The script's name. **Corpus**: a spawner's `.dat` instance names its
  `Spawn_Function`, which `Lair_Brains` runs on an interval. `Spawn`
  spawns the instance's `Spawn_Type`; a lair with its own function names
  its units as literals (`Graveyard_Spawn`, `Snake_Pit_Spawn`,
  `Rat_Spawn`, `GoblinFortSpawn`). One script writes the member later:
  the Spire of Death's quest sets `Spire_Spawn_Two` to `_Five` as spires
  fall. `gpl::spawns` reads this off a loaded program: 36 spawners in
  the expansion's, no spawn call's name left unresolved, and 3 more in
  two quests'; `regent_cli lair-spawns` prints them.
- A picked mobile is made with `APV:2` (Artifice) the picked entry's
  force, where not 0: the spawn item's `resource`, a `GSGL` entry's
  `rest[3]` (`world::new_map::LairEntry`). **Game**: the Undead
  graveyards' skeletons are born with 1, `#force_raider`, so
  `Skeleton_Birth` makes them raiders.
- `clear` adds a create flag no placement reads: it places as without.
  **Game**: the Forsaken Land's caravans, the Fortress of Ixmil's grove.
- Unknown: a piece the world cannot place that could block.
- Entering: **Game**, all three units: `IsMoving` true the frame of `Hide`,
  inside the next, `IsMoving` false after. The walk gives it: the
  FollowPath pushes a MovePath at once; next frame its plan is one node, it
  ends, and the FollowPath, every frame, ends and enters
  ([movement.md](../original/movement.md#in-the-world)).

### Placing a building

**Binary** unless marked. `world::build`; **Game**, the inn probe trace:
err.log `SendMessage frame 190`, then `Frame 191: Decoding
CREATE_BUILDING_MESSAGE`.

```text
 build menu  preview, agent -6, its unit off the map; never queued,
             no NewUnitInit
 placing     SendMessage: the preview deleted, UnitDelete(-6), an integer;
             AgentNumber(-6) prints Inn#-6 until then
 next frame  the message decoded before the clock step:
               price against the owner's APP:0, else nothing
               next agent id, next handle; the building's attributes
               APU:0 HitPoints 1: under construction
               place search ignoring what a building clears (scatter,
               ruins); flatten; clear; Dirt
               the price paid; NewUnitInit, joins the queues
```

- The message's point is the build preview's position as the player
  clicks, in 32nds; the decode truncates it to a half cell and searches
  from there. No log or save chunk holds it: err.log names the message
  only, `MSGF` is empty, `ORDR` holds unit orders. So the input is the
  placed half cell, the save's `BLDG` record: free, the search keeps it,
  so the model places the building where the engine did from any click
  that led there. A probe printing the preview's
  `LocationOf(AgentNumber(-6))` each frame would log the click.
- A building's attributes (`world::build::initial`): max hp, sight, guild
  members, the two flag bits, the tax rate for income type 2, gold,
  `IsRepaired` 1, `IsTaxed` 1, `ResearchNotCompleted` 1 for a building in
  the research table. The name, `AP:1`, is drawn after (below).
  Construction is the scripts': `peasant_go_build` adds hit points,
  `BuildingReachedMaxHP` sets `FirstStageBuilt` and `CurrentStageBuilt`
  (**Game**).
- The clearing: the message carries `ConstructData1`, 8, and flags 1, so
  the search runs with margin 8 and flags 3. Then two passes
  (`sim::clearing`):

  ```text
  pass 0  own pad mask         a ruined or agentless building, AVk1-3
  pass 1  ring, margin 8       scatter, the same buildings, a BBt chest
                               at 0 hit points
  ring    R = pad rect + 8 half cells each way, x16: ground pixels
          AVn8 set 8: first slot with block 0 as wide as R, else last
          its point list 0, y unprojected, centred on R, rasterised
  ```

  An occupant is found in any half cell it takes on the grid (not a
  `DontBlock` unit); the deletion is deferred, ground and RNG untouched.
  Buildings survive it while the charts run. Every chart unit clears the
  same way (flags 1). The ground is levelled over the own pad, not the
  ring, then paved `GR00` where `Info` bit 8 says; the world deletes at
  once, and a ruin goes with its rubble order (`world::building_ground`).
  An emptied chest keeps its agent, so it goes as `DeleteGamePiece`'s do:
  its `UnitDelete` after the frame's orders. **Game**, The Barren Waste:
  the Rogues' Guild placed by opened chest 18, its `UnitDelete` before
  the next thread run. **Game**: the inn's ring clears the six trees the
  inn save lost, and the two-day trace's seven buildings the 22 its save lost,
  nothing else; margins 8 and 9 fit, 0 to 7 and 10 up do not
  (`tests/clearing.rs`). **Corpus**: **Measured**.
- A name: a building or mobile with `AP:1` unset and a `NameGenType`
  draws from that generator (`world::naming`). 17 generators,
  `NM01`–`NM16` over `STRT` `HN01`–`HN64` four each, `NM17` over
  `BN01`–`BN04`, buildings. A generator lists every pick of one string a
  table, table 0 fastest, packed `c0 << 24 | c1 << 16 | c2 << 8 | c3`,
  and shuffles it (`i` with `rng_next(n)`, its own stream from the session
  word); a draw takes the next, and past the end reseeds with
  `rng_next(0x0fffffff)`. A table's count is its strings (**Inferred**;
  the inn fits it). **Game**: the inn is `BN01` 42 of 43, "The Cocky
  Gnome", and `NMGM` is the save's. `World::build` names a building once
  placed, so one the search refuses draws no name and no slot
  (`a_refused_building_draws_nothing`). Not modelled: that refused unit,
  made before the search, takes a handle and an agent, deleted at once. A
  start unit draws too, on generators seeded before the map is made:
  `World::seed_names` names them in creation order. In freestyle only the
  Broken Alliance's inn, a setup no single-player dialog offers.
- An effector is a unit of kind 1: it takes the next handle from the
  level's counter, as every placed piece does; a building with `Info`
  `AllocateLocalId` counts from `0x80000000` instead. One of the same
  name still on the unit with its `Wait` pending is re-armed instead, not
  for a particle system. **Game**: 534 goes between the inn day's two
  spawns, to the thought bubble. Only the first of the name still
  attached is asked, oldest first: one in its end set, its `Wait` gone,
  has a new one made though a later one waits (`World::rearmed`).
  **Game**: the probed play's hero 16 at 254,100 ms (frame 5082, day 4,
  14.1 s in), its second `thought_bubble_talk` of the frame meeting the
  251,600 ms one in its end set.

### Play's input

**Binary** unless marked. The message manager decodes a message by type,
each logging `Frame f: Decoding NAME`; a type it does not know logs
nothing (play's other `SendMessage` lines). Its dispatch runs between the
scheduler passes of frames f and f + 1, before the clock step; a recruit's
hero comes from a timed order, after the step (`world::input_timing::Stage`).

```text
 pass f ── messages ── clock to 50 (f + 1) ── orders ── pass f + 1
           NewUnitInit, building_upgraded     a hero: Generate_Character_Attributes
```

**Game**, the five-day trace: all 49 decodes land there in the probe's frames, a
hero its recruit delay later (**Measured**).
A hero's dispatch sees the next frame's clock, so its threads start then (the two-day trace's first wizard's `Wizard_tree` first runs
300 ms later).

A click is taken at the frame step's next interface poll and decoded at
the step after. Both log lines print the frames run when the step began,
so a send prints one behind the frames run.

```text
 step n   counter = n - 1 ─ decodes ─ clock to 50 n ─ poll ─ orders, pass n
          ─ pacing wait: polls until the pace
 step n + 1   the click's decode, before frame n + 1's clock step
```

- A click after pass n is taken in the pacing wait while the frame's work
  leaves time: it comes before frame n + 1 (`Game::order_frame`). **Game**:
  the tournament probe's first building, preview deleted and built between
  frame 259's probe and 260's (**Measured**).
- A frame whose work fills the pace has no wait: the click is taken at the
  next step's poll, after its clock step, and comes before n + 2. **Game**:
  44 of the 46 buildings the clock probes place, the probed play's
  first among them. **Inferred**: the trace's logging (tens of KB of
  gpl.log a frame) made the probed frames' work fill the pace; at the
  default pace the wait takes nearly every click, so a front end takes
  its input between frames.
- Every kind of order times so (**Inferred**: each is sent into the one
  queue the decodes drain). Only the build preview's deletion happens at
  the poll itself.

| Message | Does |
|---|---|
| `CREATE_BUILDING` `0x3e9` | [Placing a building](#placing-a-building) |
| `DESTROY_BUILDING` `0x3ea` | the building of the message's handle (panel `AP62`'s yes): an eighth of its price (its building price, times `GameValue2`) added to the owner's `APP:0`, then `APU:0` 0, so its death script at once: `World::demolish`, `Input::Demolish`. **Game**, the probed Barren Waste: a Rogues' Guild at 335 |
| `CREATE_VEHICLE` `0x3eb`, recruit | on the guild, unless its `APB:13` is set: the price; the hero's name drawn into `APB:12`; `APB:14` its code, `APB:15` the message's flags (3), `APB:56` the clock, `APB:13` its recruit delay; a timed order |
| — the delay later | the hero made at the guild's exit as `SpawnUnit` makes one, `HOME` the guild: `Generate_Character_Attributes`, `NewUnitInit` |
| `UPGRADE_BUILDING` `0x3ed`, an upgrade | once built (`APB:2`), the price; the type changed to its `UpgradeTo` (`SECD + 0x08`): descriptor, grid and rect anew, the building's attributes, `APB:2` 0; then `building_upgraded` |
| — research | ([below](#prices-and-research)): `world::research::Research` |
| `CREATE_REWARD_FLAG` `0x3ee` | a unit of kind 4; `TargetID`, `RewardCost` (paid, [below](#prices-and-research)), `FlagSeekers` 0; the place search from the point; an attack flag then hangs on its target ([Positions](#positions)); `NewUnitInit` |
| `DESTROY_REWARD_FLAG` `0x3ef` | the unit of the message's handle, unless queued for deletion, deleted at once: `UnitDelete`, the holder's flags stacked again; no gold back: `World::take_down`, `Input::Unflag` ([panels.md](../formats/panels.md#what-the-panels-offer)) |
| `PLAYER_SPELL` `0x3f1` | by spell code, the price in the message; a unit at the point, no search (Gate searches), then `NewUnitInit`: `world::spell::cast::Cast`; else a function, or the engine's own work ([The player's spells](#the-players-spells)). The unit takes its descriptor's `SightRange` before it is placed, so it reveals. **Game**: five-day's four `farseeing` stand where the probe prints them until `UnitDelete` |
| `GUILD_ACTION` `0x3f2` | by action; the wizards' guild's enchantment pays, `APB:4` + 1, dispatches `DoWizTowerEnchant` at once: `world::guild`; 0 a camp move: search, `KickoutRangers`, the move and its clearing, `MoveCamp` (`world::guild::camp`); `0x16` an embassy's open or close, its timer (`world::guild::embassy`); `0x15` a resurrection ([The Mausoleum](#the-mausoleum)); 6 to 9 a tax or repair flag flipped, its pair 0, no dispatch (`world::guild::upkeep`, [panels.md](../formats/panels.md#what-the-panels-offer)); 5 a hero's dismissal: `APV:28` 1, then `Unit_Dismissed` at once (`world::dismiss`, [panels.md](../formats/panels.md#a-heros-dismissal)); 3 the Call to Arms: `DoAssembly(guild)` at once, its price a `PlayerResourceMessage` decoded after (`world::guild::assembly`, [panels.md](../formats/panels.md#the-call-to-arms)); 10 the Market Day: `APB:22` the clock, `DoMarketDay(market)` at once, then a timed order whose end zeroes `APB:22` and dispatches `EndMarketDay` (`world::guild::market`, [panels.md](../formats/panels.md#the-market-day)); 2 the extortion: `DoExtortion(guild)` at once (`world::guild::extortion`, [panels.md](../formats/panels.md#the-extortion)); 1 the Rage of Krolm: the palace's `APB:7` 0, its `APB:52` the clock, `DoRageOfKrolm(temple)` (`world::guild::rage`, [panels.md](../formats/panels.md#the-rage-of-krolm)); `0x14` a bounty: length and price by interface runs, `APP:2`–`4`, `Champion_Icon`s, a palace `Wait` (`world::guild::bounty`, [panels.md](../formats/panels.md#the-hall-of-champions-bounty)) |
| `SET_UNIT_NAME` `0x3f4` | the unit of the handle takes the name, `AP:1` kept; no dispatch: `World::rename`, `Input::Rename` ([panels.md](../formats/panels.md#renaming)) |
| no decode line | `AlterAttributeDeltaMessage`: a panel's change to one unit's word: `world::alter::Alter` |
| `PlayerResourceMessage` `0xb`, logged `Decoding PlayerResourceMessage: Amount = %d` | a player word: flag 1 spent if held, 0 added; the camp move's price and its return: `world::guild::camp::Resource` |

- **Game**, play: the four wizards come at `50 (f + 1)` plus 14,000 ms, the
  descriptor's delay; the last guild's `ButtonStartTime` is the save's
  (**Measured**). The
  explore flags stand where `explore_flag_poll`'s `LocationOf` prints them;
  a building where its save has it (**Measured**).
- A recruit's decode (`world::hire::Hire`) pays, draws
  the hero's name into the guild's `APB:12` and writes the other words;
  the hero comes its delay later and takes `APB:12` as its `AP:1`, so it
  draws none; `APB:12` stays. **Game**: the two-day trace's and the five-day trace's generators
  are their saves' `NMGM`, every hero's `AP:1` its record's, and the two-day trace's
  rangers' guild holds its last ranger's name, the hero not out
  (`names_hold`).
- At a guild full of living and dead (`MaxGuildMembers`, henchmen aside)
  a recruit dismisses the grave of least `ExperienceLevel`, then
  `Experience`: `Unit_Dismiss_Gravestone_Fast`.
- The enchantment is `Input::Enchant`; its timed order
  (`#Wiz_Tower_Enchantment_Duration`, 90,000 ms) dispatches
  `EndWizTowerEnchant` (`World::enchant_over`) and sets `APB:45` 0.
- A player spell that makes no unit is `Input::Bless`: its function runs
  with the target between the passes, the price taken, refused where the
  target's key for that spell is set (`SPELL_SCRIPTS`; **Inferred**:
  nothing paid when refused).

`sim::game::Input` is the player's input a front end
sends, given to `Game::input` with the frame it comes before (err.log's
decode frame f + 1); `Game::apply` is the one decoder: `Build` with its
preview's `Unpreview` just before it, `Recruit` (the decode pays and
names; the world makes the hero its delay later,
`Generate_Character_Attributes` then
`NewUnitInit`, after the clock step), `Flag`, `Cast` (then the unit's
`PRIM` callback, `Farseeing_unit_created`), `Upgrade` (`building_upgraded`),
`Research`, `Alter`, `Enchant`, `Resource`, `Camp`, `Embassy`, `MarketDay`,
`Extort`, `Rename`, `Rage` and `Bounty`. A decode's dispatches see the
frame's clock, 50 (f + 1). **Game**: the free probe's first explore flag,
decoded in frame 1698, polls first in 1709 and 1719, `#Flag_Cycle` 500
apart; play's wizards' guild upgrade thread first runs two frames after
its decode. `IsValidMiniViewTarget` and `CanIBuildThisBuilding` are
skipped as the interface's runs (**Inferred**: queries). A front end asks
the second through `Game::placement`, which runs it with no trace and no
reveal and leaves the game as it was (**Measured**).

#### Played traces

Traced games' input, read off their logs, played every script line
(**Measured**). Rules they showed:

- A spell unit timed out in a decode's dispatch starts its `Wait` at the
  game clock 50 (f + 1) and goes in the deletes of the frame whose clock
  reaches its expiry (**Game**: the first farseeing, 125,000 ms, frame
  2500).
- A ratman the player's sewer spawns for `team` has no `Parent`; a rats'
  nest's own has the nest (**Inferred**: `HOME` only for a spawner of the
  unit's owner).
- A flag hung on a unit is placed again among its other overlays as an
  effector leaves it, each against all the others (**Inferred**: troll
  60's attack flag rises a cell at 4891 as `wounded_large` goes,
  `Poison_icon` still on; a new overlay moves none).
- An overlay leaving its holder still counts in the restack its delete
  makes, each measured at the holder's facing now; an effector counts in
  its holder's restack until deleted, its end set played.
- An embassy re-armed by its last tick ticks in that turn, so its second
  spawn comes 971 ticks on.
- Scatter a clearing deletes in play stays on the grid until the next
  drain (a rat's nest placed at 4839; ranger 25 plans around its `NGa1`
  at 4840).
- A building's vents' set on load is its condition's where the saved set
  is none of its art's; a load keeps each saved own name; the null record
  saves as `NullFunction` (the embassy's `.dat` `activescript` names no
  function), so a load runs it as the builtin.

### The player's spells

**Binary** unless marked. `world::spell::cast`, `world::spell::illusion`,
`world::spell::target`, `world::teleport`; **Measured**, `tests/economy.rs`. Traces cast
invisibility, anti-magic shield, lightning bolt on a unit, Farseeing,
Lightning Storm, Stone Skin, Petrify, Vigilance, Gate, SuperCharge and
Dismiss, each played whole-log; the rest is **Inferred** where it
meets the game ([status/open-questions.md](../status/open-questions.md) 65).

```text
 apply(spell, _, player, x, y, target, price)
  price above APP:0   chat, nothing
  Sp15 SuperCharge    no target: the palace; APB:9 set refuses; APB:51 the
                      clock; SuperCharge_Begin(palace)
  a spell unit code   a unit (Cast); Sp12, Sp30 only on a building, the
                      unit at the point, its callback (unit, building)
  no target           refused
  APV:44, not ours    refused: a magic mirror
  Sp24 Illusionary    APU:0 above 0: a CVC1 unit (agent illusionary_hero)
                      becomes the target's type, the target's owner; two
                      world draws of 0x60 less 0x20 spread its place
                      search; MaxHP and HP the target's; its own name
                      GMTX 71 "Illusionary %s" over its title; APV:46;
                      HOME the target; NewUnitInit
  the rest            a guard, then function(target): SPELL_SCRIPTS
  refused             nothing paid
  Sp32-Sp37           even refused: the best ABq's ASS:0 the clock, ASP:0
                      #SpellDuration_…; the fast bar sends none of the six
                      while ASS:0 is set
```

| Spell | Dispatches | Guard |
|---|---|---|
| Agrela, Fervus healing | `Agrela_Heal_Effect`, `Fervus_Heal_Effect` | refused at full hit points unless `APV:60` (plague) |
| Resurrection, ReAnimate | `Resurrection_Begin`, `Reanimate_Begin` | none; the cursor takes only one lying dead |
| Animate Bones | `Animate_Bones_Effect` | `APV:62` |
| Fire strike | `Fire_Strike_Hit` | none; a building makes `SVA1` |
| Frost Field, Chain Lightning, Dismiss | `FrostField_Begin`, `ChainLightning_Begin`, `Dismiss_Begin` | none |

- The cursor (`World::spell_target`): never a `NotSpellTarget` or held
  unit; a character spell wants it seen, a character (**Inferred**:
  `CanUse` by its code's letter), not a resource pile, not invisible,
  standing, its key unset; change of heart a hero; resurrection and
  reanimate one lying dead; invisibility, lightning bolt and anti-magic
  shield within a wizards' guild's or tower's reach (`#SpellCastRange`,
  doubled while SuperCharged, plus `#LevelSpellRangeBonus` a stage), the
  last two buildings too.
- The reach: the owner's guilds and towers, standing and finished
  (`APU:0` above 0, `APB:1`); the gap from the pad at the building's place
  to the point, 32nds, at most 800, 1,600 SuperCharged; a guild 250 more a
  stage past 1; a tower's enchantment and `#wiz_tower_range` (the build
  rule's) play no part. The point spells: Farseeing hidden ground in
  reach, Lightning Storm explored in reach (`World::spell_point`). The
  decode tests no reach: the click sends only what the cursor takes;
  refused, Sp10 to Sp14 out of reach say `GMTX` 40 "Out of range.", no
  sound (`World::spell_refusal`); a cast leaves the mode as it was but for
  the Abode's six (**Measured**
  **Measured**).
- Their scripts' builtins: `ClearEngineDeathFlags` clears the lying bit;
  `TeleportToPoint` and `TeleportToUnit` clamp to their reach,
  place-search and drop every order; `CreateMissile` launches as an
  action's projectile; `GetBoardExtents`; `GetBuildingContainer`.
  **Inferred**: `TeleportToUnit`'s second unit is the owner.
- The fast bar clears a spent Abode timer by a message setting both words
  0, sent each frame until it lands (`World::abode_bar`). The clone's own
  name, "Illusionary Wizard", is `naming::Given::Illusion`, spelled by
  `view::panel::portrait::illusion`. **Inferred**: the clone takes one
  world slot draw before the two.

#### The Gate's destination

The Gate sends a hero to the player's first listed spell unit, whatever
it is. `Gate_Hit` (`mx_Spells.gpl` line 698) asks `ListObjects(abode,
"spell", -1, Gates, #FindFirstMatchOnly, #MyPlayer)`; 13 instances of
`mx_Spell_Data.dat` have type `spell`, Farseeing and the storms among
them. The title filter is commented out (lines 695–696);
`Gate_Unit_Created` finds its Abode with `#CheckTitles`. The engine does
what the reference says of `#FindFirstMatch`: the first unit found, not
necessarily the closest (`GPL Reference.pdf`). It stops at the first
match, the walk in list order. So the bug is the script's. **Game**, the
spells trace (**Measured**): at frames 2282
and 2283 (114.1 s, day 1) wizards 15 and 36 land on Farseeing 33 (made at
2224), not on Lightning Storm 34 (2241, nearest) or Gate 35 (2259); from
2564 the gate is the only one. The three were made in agent id order, so
the trace does not separate list order from id.

### Prices and research

**Binary** unless marked. `world::price`, `world::research`,
`world::flag`, `world::alter`; `tests/economy.rs` checks the tables against
the program's constants; **Measured** against the five-day trace.

```text
 building  Cost; a built Blacksmith takes GameValue1 (0.05) off;
           x Multiplier / 1024, truncated, per same-prefix building owned
 upgrade   the same on UpgradeTo's Cost, no multiplier
 recruit   the hero's Cost, at the decode
 research  #ResearchCost… less 51/1024 (0.05) per Library level
 spell     the message's price, the spell table's #SpellCost_…
 enchant   #WizTowerUpgradeCost, once (APB:4)
 reward    APR:1 written: the change is paid, rounded down to
           #RewardDelta when the treasury is short; lowering gives back
 each      taken if the treasury holds it, else refused
```

- A price counts standing buildings only: the walk skips the dead and
  dying, and on a building one ruined or with `AE:0` 0. So the price
  falls at `building_death`'s `DeleteAgent`, not as the rubble goes, and
  clearing rubble changes no price. A fallen Blacksmith
  discounts nothing.
  HD; Gold's code is not read.
- In a Last Palace game (victory index 3), five codes multiply by an
  `#LPS_` constant / 10 instead, `(c << 20) / 10240` truncated: the Inn's
  12 is 1228. `world::price::LPS`, read from the program at `Game::begin`.
- `GameValue1` and `GameValue2` are settings no shipped `GOPT` sets;
  static initialisers make them 0.05 and 0.125. The Library's 0.05 is a
  third copy of the same text.
- Research: at err.log's frame f, the price, then `CurrentResearchItem`
  the id, `ButtonStartTime` `50 (f + 1)`, `ButtonDelayTime` the time; the
  time later, in the orders, the row's attribute is set,
  `ResearchNotCompleted` cleared once every research of the building's
  stage is done, and the two words zeroed. No GPL dispatch. 26 rows, time
  and cost from the loaded GPL's constants; seven researchers. An upgrade
  sets `ResearchNotCompleted` from the new stage.
- In a Last Palace game a Guardhouse's research counts as done whatever
  its attributes: made with no `ResearchNotCompleted`, cleared on an
  upgrade, its stage 2 research offered with stage 1's undone
  (`World::done_by`, `a_last_palace_guardhouse_has_done_its_research`).
  The rows themselves still wait to be researched.
- A flag's reward is the reward panel's setting, changed with no message
  (attack and explore, saved in `GSGL`); raises and the tax collector
  panel's `PickupAmount` and `ReturnAmount` are
  `AlterAttributeDeltaMessage`s, err.log's sends with no decode line. Each
  is applied at once, in its place among the decodes, the frame after its
  send, before the clock step and the hit point tick (type `0xc`; an
  `AlterAttributeMessage` is type 8). A game applies it among the frame's
  inputs; the fast bar's setting, sent in a step, before the next frame's
  inputs (**Inferred**: ahead of the clicks)
  (`an_alteration_lands_in_its_place_among_the_decodes`).
- **Game**, the five-day trace: eleven researches (the eleven `UPGRADE_BUILDING`
  decodes no `building_upgraded` follows; which is which **Inferred** from
  stage and reads), ten flags, 37 of its sends: every `GetAttribute`
  answer is right, 38 words of the save's research buildings, guilds,
  standing flag and tax collectors are the world's, and the treasury ends
  at the save's 2,132, 130 short without the Blacksmith's discount.
  The two-day trace: eight researches, seven recruits, 30 sends; the treasury is the
  save's 15,268, its last ranger paid for and not yet out, and its
  research and guild words the save's (**Measured**).

### Positions

**Binary** unless marked. `sim::space`; the save side in
[saves.md](../formats/saves.md#where-a-unit-stands).

```text
 position            16.16 cells
    >> 11            32nds of a cell          GPL coordinates
    / 16, to zero    half cells               the unit's grid place
 pad rect            half cells about that    PRIM +0x14..+0x20
 distance            squared gap, 32 bits
```

- A held unit — one with a holder, not shown — measures from its holder's
  half cell, and its position reads as its holder's; its own stays
  (**Game**: the day save).
- A held overlay (descriptor kind 4 or 5) adds its own position to its
  holder's. An attack flag is one: it is linked to its target (relation
  1), and its own position set to the target's attachment point
  (`AttachmentPointID`, 0 read as 1) less the target's position, y plus 1
  in 16.16 (`world::flag`). **Game**, the five-day trace: both attack flags print 1 x
  past their ratman champions, whose `Hotspot` point 0 is at x 1, and
  `InsideBuilding` true: a flag's `LocationOf` is wrong only where its
  target's is.
- An overlay (kind 4, not `NotVisibleInIsoView`, `StackPriority` n above
  0) is then stacked among the holder's relation 1 overlays
  (`world::stack::stack_offset`): each priority below n held lifts it a
  cell; with k of priority n before it, x moves −(k + 1)/4 cell for k odd,
  +k/4 for k even, and +1/8 when k + 1 is even. The world counts flags
  hung on the target and its effectors, in handle order (**Inferred**:
  relation 1 appends); a restack's eighth counts later ones too. The five-day trace's
  two attack flags hang alone.
- Setting a position writes the half cell from it; a walking mobile's is
  set by its movement too (**Game**: 214 of 424 unheld corpus mobiles
  differ, **Measured**). A building never moves: its half cell is its position's.
- Pad rect: the art's point list 0 bounds, else its footprint rect, both
  ground pixels; minima floored, maxima ceiled over 16
  (`sim::placement::pad_rect`). Half-open. A unit drawing no footprint set
  measures as a point.
- Gap: per axis `b.x0 - a.x1` when `b` is right of `a`, `b.x1 - a.x0` when
  left, else 0; squared and summed. A point is a one-half-cell rect.
- `IsAdjacent` (not answered: every play call has a mobile): the first
  unit's rect grown one half cell each way overlaps the second's, and
  where the second has a pad mask, a covered half cell of it.

### ListObjects

`ListObjects(agent, type, range, list, modifiers…)`.

```text
 searcher's unit, else throw "was given a null agent to search with"
 lists   VisibleUnitList 8 + owner  |  #NoHiddenMap: lists 0, 1, 4
   └─ each unit, in list order ──> test ──> (gap², agent id, agent)
 sort by gap², then agent id (MSVC std::sort; a total order)
 list = the agents, in order; result = their count
```

A unit passes when all hold:

- not held, unless `#InsideOtherUnits`; not the searcher;
- its gap to the searcher, or to a coordinate modifier (over 16, toward
  zero), is within range: range over 16, toward zero, squared, for a
  positive range; 0 takes only touching units; below 0, any;
- `#MyPlayer` / `#NotMyPlayer`: same or other owner;
  `#MyTeam` / `#NotMyTeam`: same or other team;
- `AE:0` is not 0: it has an agent;
- its GPL `type` equals argument 2 (case-insensitive; `#RewardFlags` makes
  it `RewardFlag` and implies `#InsideOtherUnits`);
- `#CheckTitles` / `#CheckSubtypes` with a string modifier: `title` /
  `subtype` equals the last string given;
- any other integer modifier is an attribute key, registered or it throws
  (`unknown search modifier`), and the next argument the value it must
  hold (0 when absent);
- `#FindFirstMatchOnly` stops the walk at its first match, in list order:
  a list is walked from its head, and units are appended. A
  `VisibleUnitList` drops a unit and takes it again, at its tail, each
  time its words change, so the first match is the one whose words
  changed longest ago (`World::first_listed`). The world leaves it unknown
  where two matches changed in one sight pass, and for `#NoHiddenMap`'s
  lists. **Game**: `Gate_Hit`'s search for the player's `spell` takes a
  Farseeing made first, not the nearer storm nor the gate
  ([the Gate](#the-gates-destination)).

Modifier values are `LowLevel.gpl`'s constants, 0 to 9
(`world::search::MODIFIERS`, `list_objects_modifiers_are_the_sdks_constants`).

The level's lists: 0 `BuildingList` (unit kind 0), 1 `VehicleList` (1), 2
`TerrainFeatureList` (2), 3 `AllUnitsList` (the source of the others), 4
`RewardFlagList` (4), 7 `OffMapLairList`, `MSGF` `MessageFlagList`, 8–15
`VisibleUnitList`, one per player. A `VisibleUnitList` takes a building,
mobile or flag, not a ruined building, whose sight word for the player is
set. The owner's word is always set; every other player's comes from the
ground under the unit: vertex flag bit *p* is ground player *p* explored,
25 per explored corner; a building's over its `PRIM +0x04..+0x10`
rect in cells, not its pad rect ([below](#explored-ground)). A hung reward
flag's word is its holder's, and a unit is taken into the lists or out
only as its own words change: so a flag stays listed as its holder was
seen at that change (`World::relist_flag`). **Game**: the probed play's
`check_rewards` drops a flag hung on an unseen giant rat at 5368 and keeps
one whose champion has gone from sight at 5628.

The world reads the words as the last sight pass left them, the pass
marking every unit whose position was written since (every step, not a
new half cell alone). **Game**: in the dwarves game a shadow beast steps
onto explored ground in frame 3,283's orders; the settlement's search in
that frame's scheduler does not list it, the ground at the call would.
Not modelled: the engine also recomputes a unit's words mid-frame, before
a sound at the unit and after each set it plays (walk, stand, an action's
set and after-set, flags, spell units, effectors, missiles, a building's
damage stage), so its place in lists 8–15 changes mid-frame. Modelling it
means the world updating at each of those, in time, and knowing whether
a `DSND` record exists, which it does not track. No trace reaches a case
it decides: over every test, no `#FindFirstMatchOnly` search meets two
units relisted in one pass (`World::first_listed`, Unknown); the pass's
own order, list 3's join order against the world's agent ids, is a second
cause. A unit whose descriptor has `Info` bit 15, `NotVisibleToOpponent`,
is seen by its owner only.

Teams: a new level puts every player on team 0, then slot 7 alone on team
1; `SetPlayerTeamNumber` moves the unit's owner. So the monsters' lair
searching `#NotMyTeam` finds the crown's buildings.

The world answers when every unit is decided: its place fixed (a new game's
building), its sight known — its owner's, a player who had explored the
whole map (**Game**: slot 7's bit is on every vertex; **Inferred**: ground
stays explored), or the explored ground under it ([below](#explored-ground))
— and its attributes read from the save or given. A unit the other tests
reject needs no place.

### Explored ground

**Binary** unless marked. `world::reveal`; `Explored`, one flag byte per
vertex from the save's `MVTX`, bit *p* player *p*'s.

```text
 a position set, or a walker claiming its next half cell
   └─ unheld building or mobile: a reveal disc
          centre  mean of its PRIM +0x04..+0x10 pixel rect's corners about
                  its position, 32nds, floored to a cell
          radius  (w + h) / 32 / 4 + SightRange (APU:3) / 32, toward zero
        disc(centre, owner's mask, radius)
 a step within a half cell: no reveal
 RevealArea(unit, point, r): the point's cell, the unit owner's mask, r / 32;
   r below 0: every vertex
 disc: vertices x in cx − r + 1 ..= cx + r (and y), clipped,
   with (x − cx − ½)² + (y − cy − ½)² ≤ r², 16.16 fixed point
```

- The mask is the owner's bit, and a teammate's sharing (`APP:1`,
  `SetShareReveal`), which no play sets.
- A search sees a point unit where a corner of its cell is explored, a
  building where a corner of one of five sample cells of its `PRIM` rect
  is (the four corners, x1 exclusive and y1 inclusive, and the middle).
  Where a piece of the player was not placed, the ground it may have
  explored is unknown, and so is an unexplored answer (`World::blind`).
- The sight pass (`World::sight_pass`): after the scheduler each frame,
  every unit is marked where a vertex gained a bit since the last pass,
  then a marked unit's words are recomputed; a step marks it too. The
  sample test answers whether a word changed, not whether one is seen. A
  building, or a unit with `RevealTerrainIfExposed`, whose words changed
  reveals for each player whose word is above 0 a disc of radius h + 1
  about (x0 + h, y0 + h), h = (x1 − x0) / 2 of its cell rect, the width
  standing for the height; that word becomes 100. The owner's word is
  always 100. Not followed: the mid-frame recompute before a sound or art
  play ([ListObjects](#listobjects)).
- Terrain features are units of list 3 too, so the pass reaches scatter:
  `RevealTerrainIfExposed` is set on the scorch ponds `CG01`, `CG02` and
  the scorch geyser `YG03`, which so reveal about them once a corner of
  their rect is seen (`World::scatter_sight`). They have no owner word.
- `GetNearestHiddenCoord(agent, coord&)`: throws for a dead agent or a unit
  off the cells; one GPL draw below 32; from the unit's cell, for its
  owner: vertex rings outward, ring k from fx − k to fx + 1 + k, clipped;
  each walked clockwise from the corner draw / 8 names (top left, top
  right, bottom right, bottom left), each side from its first corner to
  before its last; the first unexplored vertex as the cell on the unit's
  side of it. Found: the cell × 32, at least 0x30, into argument 2, true;
  none, false.
- What the iso view draws (`view::map::fog`): the ground
  per vertex, one
  the view's mask (player 0's bit in single play, **Inferred**) has not
  explored at shade 0.001, black, Gouraud over the cell's cut; a triangle
  of three such corners untextured; mask bit 31 shows all. A unit is
  drawn and clickable only where its seen level is above 0; from 1 to 99
  in its sprite's palette `(100 − n) / 25`, palette 0 with HSL lightness
  times 1 − k / 4 (four palettes); a tint's palette wins. Words come from
  explored flags only, so there is no seen-but-not-now ground. The halved
  ground shade is another view's.

**Game**: from the new game the world reveals what the day and inn saves
explored, vertex for vertex (**Measured**): the inn and the peasants walking to
it. The two-day trace ends none short and 34 beyond (the log running past the save),
the five-day trace none short and 45 beyond. A spell unit's `SightRange` reveals (the
four `farseeing`s). The second reveal changes no vertex of the five-day trace.
`RandomCoord` answers right wherever the world placed the unit;
`GetNearestHiddenCoord` 23 of the five-day trace's 24, the last from a ranger walked
otherwise. Terrain features revealing, the clock probe plays its whole
log, the probed Barren Waste every `GetNearestHiddenCoord` to its victory,
and each save's explored ground is the world's
(**Measured**).

## A game

`sim::game::Game` runs a game on the live host (`sim::host::Live`): the
setup of [Starting a game](runtime.md#starting-a-game) at game time 0,
then scheduler frames. The world is `setup::fresh`
([With no save](runtime.md#with-no-save)).

```text
 setup, now 0   InitializeGpl                   CreateAgent: GplAIRoot, 1
                World::unit_pass                agents 2..7, handle order
                Freestyle                       the quest's script
                NewUnitInit × 6, Palace_Pre_Birth on the palace
 frame n        now = 50 n; runtime::Queues; per unit its function
                attributes by name, vm::threads::run each
```

- The unit pass makes each agent from its piece's descriptor name and
  writes `AE:0`.
- A thread holding a builtin runs it: the towers' `RevenueScript` holds
  the empty name's, runs every frame, and prints nothing.

A builtin the world does not answer, or cannot here (`Answer::Unknown`),
leaves its result at its type's default: 0, false, empty, the null agent;
a `host::Fallback`, counted in `host::Faults` and logged the first time
per builtin. **Inferred**: a stand-in, not the engine's answer. A throw
goes to the script, as in the engine.

`World::shown` is what a drawing reads: per piece its descriptor, place,
facing, whether it walks or acts, and whether it is held. `openregent`
runs it ([openregent/game.md](../openregent/game.md#the-game)).

**Game**: from a new freestyle game the traces' setup, a quiet day, an inn
placed on the probed day, defeat games and quests played line for line,
and the saves' script state and records came out (**Measured**).

### The frame

The engine's own dispatches between passes are the world's. After the
frame's step, `Game::frame` runs `World::take_between`, then
`World::take_deletes`, then the scheduler:

```text
 step     tick, orders, missiles
 between  the tick's deaths; each order's callback in turn: an action's
          function at its peak, a missile's impact, an effector's end
          script
 deletes  UnitDelete of each DeleteGamePiece since, last first, and of
          each spell unit whose timeout deleted it in an earlier frame
 pass     the GPL scheduler
```

An effector's `Wait` is an order in `ORDR`, joined as it is pushed. As it
ends, the holder's descriptor script runs once with the holder's agent,
where the holder has an agent: `Damage_Shield`'s
`Damage_Shield_End(holder)`. Deleting effectors with their holder or by
name runs it too, so an effector's end script runs once, whichever comes
first. **Game**: the defeat trace's first `Damage_Shield_End`, Giant_Rat 13's, at
212,300 ms.

An action's function and `cProc` are its own: a `PerformAction` while
another's `InitAction` plays leaves that one's to its peak, and the new
Action takes its own when it arms. **Game**: shaman 14 casts twice in one
`raider` run at 282,800 ms, a shield on champion 12, then a cloud at the
tower; the shield's `Damage_Shield_Begin` runs at its peak.

An action's function takes its arguments by its `Script type`
(`missile::script_args`): a ratman's attack is
`Advanced_Attack(actor, target)`, which calls `make_attack`. A peak at
the `PerformAction` that pushed it dispatches inside the builtin
(**Inferred**). **Inferred**: the engine runs each callback inside the
order pass, so an order after it sees what it did; here they run after
the step.

An agent `UnitDelete` ends leaves the scheduler's queues at once
(`runtime::Queues::leave`).

### Rules from the defeat games

The defeat trace, a freestyle game run on with no input until its palace
falls, showed:

- The first unit in a walker's way is the one met row by row over its pad
  rect there, not the lowest id: at 287,250 ms ratman 15 meets standing
  ratman 16, not walking champion 11, plans round walkers and goes north,
  as the probe shows ([movement.md](../original/movement.md#waiting-on-a-walker)).
- An action's function needs the actor's and target's agents when it
  runs (`World::stands`): at 302,000 ms champion 11's blow razes the tower,
  and ratman 16's, peaking in the same frame's orders, runs no
  `make_attack`.
- A peak runs its function at once, inside the order pass: at 352,400 ms
  ratman 16's blow at the palace peaks before the catapult, behind it in
  `ORDR`, launches its sixth ball, so `damage` rolls before the launch's
  two draws. `Game` pauses the orders at each dispatch, runs it, then
  the rest; the world steps no more while a dispatch runs, so the
  dispatches it makes run inside it.
- A walker's `MovementRateModifier` (the class's rate plus `APV:34`) is
  read as each `Move` is made, so a change takes with the next `Move`
  (`Walker::modify_rate`): the plague's +100 at 360,650 ms slows its
  target.
- `Parent` reads the `HOME` member's `AE:0`, which `DeleteAgent` set to
  0: at 378,500 ms the razed palace's peasants' `GetPalace` gets agent 0,
  `??#0`.

**Inferred**: freed function objects the engine's pool
hands out again keep their interval and last run, three on one catapult,
which the VM clears. No script reads them.

### The game's end

A lost game stands still, and a won one. **Binary**: the game's end sets
the app's next state 8, taken before the next frame step, so the loss's
frame finishes; in state 8 that step runs no clock step, hit point tick or
particle update. **Game**: every defeat trace ends soon after
`DeclareLoss`. `Game::frame` then does nothing.

State 8 then plays the end out each frame (the steps in
`world::defeat`): music and sounds stopped, the advisor's "won" or "lost",
a won game's victory star at its point on the minimap, its "Game Over" box
and the view moved there, a 15-step fade of 75 real ms, the end panel
`AP60` or `AP61`, its statistics `AP84` or `AP83` (`World::summary` gives
the four rows the world holds).

The one-player dialog offers three victory conditions (`freestyle::Victory`;
"Last Palace standing" with more players), each brought to its
`DeclareVictory` with no answer unknown
(`regent_core/tests/victory_play.rs`): Eliminate all foes, the start's foes
gone, at the first 4,000 ms check (`VictoryCondition_Three`); Gather gold,
60,000 for 30,000 to start, once the treasury passes it
(`VictoryCondition_Two`); Survive, 10 days, at 600,000 ms
(`VictoryCondition_One`). Each `victoryforall` or the richest names the
palace, so the point is its place. A one-player game is lost only to its
palace razed with no outpost (`Palace_Death`; an outpost takes over by
`SetPalace`): the default game at frame 7,618. No freestyle script times a
loss; `multiple_loss` is for other players.

## What runs

- 59 HD functions call, transitively, only native builtins. All 156 runs on
  generated arguments finish or stop on an exception the engine throws too:
  29 null-agent attribute reads, 11 integer list samples where agents are
  wanted, one divide by zero. No crash, in either mode
  (`every_function_calling_no_world_builtin_runs`).
- All 11,472 functions of every image, each image loaded over HD's, run
  26,551 times against `LogHost`: 15 crash, 47 hit the bound, the rest finish
  or throw (`the_whole_corpus_runs_against_a_logging_host`). Every crash and most throws come from the stub: attributes no
  prototype declares are null, builtins return defaults, nothing changes
  between reads. The table is in `tests/vm.rs`.
