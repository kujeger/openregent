# Not done

What the original does and openregent does not. None is known to change a
quest's outcome. Each item points to where the docs record it. Unanswered questions are in
[open-questions.md](open-questions.md). Evidence markers:
[README.md](../README.md#evidence).

## Simulation

| Gap | Where |
|---|---|
| A spell unit's set `Animate` order in ORDR. Its look is modelled (`World::spell_look`); every save's ORDR places come out without it | [movement.md](../original/movement.md#track-2) |
| Scatter's `Animate` orders; scatter has no agent and runs nothing in its turn | [movement.md](../original/movement.md#track-2) |
| A step with set 8 showing and set `0xb0` playable; animation only | [movement.md](../original/movement.md#a-step) |
| A ninth occupant of a grid cell is dropped as it is added. Reads take eight at most (`movement::plan::OCCUPANTS`) | [movement.md](../original/movement.md#in-the-world) |
| A deferred piece's delete blocks its cell until the next frame's drain; here it leaves at its `UnitDelete`. Scatter cleared in play is the exception (`Board::clear_later`) | [movement.md](../original/movement.md#in-the-world) |
| A message flag's place in list ORDR; no script reads it | [vm.md](../gpl/vm.md#message-flags) |
| A refused building's unit: made before the search, it takes a handle and an agent, then is deleted | [vm.md](../gpl/vm.md#placing-a-building) |
| Playing a set updates a marked unit's sight mid-frame, changing its sight words and its place in lists 8–15; here only the frame's sight pass does (`World::sight_pass`) | [vm.md](../gpl/vm.md#world-builtins) |
| `SpecifyIntent` with a string intent: `World::specify_intent` answers unknown | [vm.md](../gpl/vm.md#world-builtins) |
| A `#Name` found by descriptor name, the third source the loader tries. Script constants and the attribute registry are read; `tests/load.rs` finds no shipped or Workshop image needing the third | [mods.md](../formats/mods.md#not-done) |

## Particles

[vm.md](../gpl/vm.md#particles). Not in code:

- the clock's wrap past 49 days
- `Flags` (`IsPausable`, `IsPerformanceHog`): parsed (`desc::particle::EmitterFlag`), not used
- two global render switches
- the draw order among other entries (**Inferred**: by row)

In code as **Inferred** stand-ins (`view::map::particles`): the attachment
offset as 0; a birth's two size factors as 1 and 0.

## Interface and network

| Gap | Where |
|---|---|
| The tracking window has no map tooltip: the tooltip's pick takes the main view alone (`map::cursor`, `Through::Map`) | [panels.md](../formats/panels.md#the-tracking-window) |
| A guest that loses the host plays on alone (`net::node`, `lost`); HD asks "Quit Game?" (`GMTX` 290/291) | [multiplayer.md](../openregent/multiplayer.md) |
| The unit browser's CPU path (`Sheet::draw_with`) reads every world tile as mode 7; only the map draws mode 1's ramps | [shadows.md](../openregent/shadows.md#a-shadow-needs-a-destination) |

## Built but unverified against the original

Read from the binary and the scripts, never seen in an engine trace.

- Builtins: the "none" rows of the table in
  [vm.md](../gpl/vm.md#world-builtins). Most have a test
  of their own. These have neither: `LearnSpell`, `FillSpecialLairList`,
  the nine inventory builtins (`DeleteInventoryItem` to
  `GetInventoryItemAgentType`), `AddAttribute`, `RemoveAttribute`,
  `SetPalace`, `EnchantWizTower`, `SetEffectorDirection`, `LogOn`, `LogOff`,
  `UnitIsLocalPlayer`, `SetTreasuryValues`.
- The player's spells the traces never cast (open question 65).
- Settings keys read by name only: `DeferredSpriteLoading`,
  `ExecutionWindow`, `BundleFactor` ([settings.md](../formats/settings.md#what-each-key-does-in-hd)).
