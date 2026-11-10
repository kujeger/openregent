# Cheat codes

A cheat is a chat line. Code: `regent_core::sim::world::cheat`; tests:
`tests/cheat.rs`. **Binary** (`MajestyHD.exe`) unless marked.

## The path

```text
 chat line, Enter
   cheats allowed ─> line put in capitals, compared whole with each
                     GMTX phrase in turn
     match ─> CHEAT_CODE_MESSAGE (code, arg), sent like any order
          ─> the line becomes the phrase's reply (the next GMTX
             string) and goes out as chat
 decode   the message manager, before the clock step ─> the cheat applied
```

The send sites: **Binary**.

- The phrases are data: `GMTX`, fetched again as the dataset changes.
  Freestyle and the expansion's quests take `mx_textdata.cam`'s, 442
  strings; a base quest opens `Data/` alone, so `textdata.cam`'s, 388
  (both counts: `an_empty_line_is_planet_fargo_in_a_base_quest`).
  "fill this bag" works because the line is put in capitals; a trailing
  space does not. Test: `the_phrases_are_hds_text`.
- Past the table a `GMTX` string is empty. So in a base quest
  `PLANET FARGO` (440) is "": an empty line sends code 20, and its reply
  (441) is "" too; typed, the phrase is plain chat. The chat sends an empty
  line like any other. Test: `an_empty_line_is_planet_fargo_in_a_base_quest`.
  **Game**: the chat test runs decode a cheat for an empty line in The
  Barren Waste, none in Freestyle
  (**Measured**).
- Allowed: set as the session is made, so in every new game (**Binary**). A loaded game
  takes its save's word; every corpus save allows
  (**Measured**). Here `World::cheats`.
  **Inferred**: a network game takes the host's, in the session block its
  start broadcast copies.
- The message is the cheat, not the phrase. It goes through the message
  manager, so every peer applies it, logged as `Decoding
  CHEAT_CODE_MESSAGE` with `-debugout`.

## The phrases

Test: `the_decode_has_a_case_per_cheat` (codes and effects:
**Binary**).

| Phrase | GMTX | Code | Arg | Effect |
|---|---|---|---|---|
| `VICTORY IS MINE` | 43 | 3 | player | the game ends, won |
| `I'M A LOSER BABY` | 45 | 4 | player | the game ends, lost |
| `CHEEZY TOWERS` | 47 | 5 | player | palace `APB:10` `CheatWizGuildRange` 1: spells reach everywhere |
| `RESTORATION` | 49 | 6 | player | over buildings, then mobiles: a damaged building with an agent gets `BuildingReachedMaxHP` (not mended); a damaged mobile is mended to `APU:1`. Owner, not team |
| `REVELATION` | 51 | 2 | player | the whole map explored for the player |
| `FILL THIS BAG` | 53 | 0 | player | 10,000 added to `APP:0` |
| `NOW YOU DIE` | 55 | 8 | selection | `APU:0` 0: it dies, any owner's |
| `NIGHT OF THE LIVING DEAD` | 353 | 13 | player | `Cheat_wave_undead()` |
| `PREPARE TO DIE` | 355 | 14 | player | `Cheat_wave_raiders()` |
| `PUMP UP THE VOLUME` | 357 | 15 | player | `Cheat_wave_general()` |
| `GOBLIN RUSH` | 359 | 16 | player | `Cheat_wave_goblins()` |
| `GIVE ME ACTION` | 361 | 17 | player | `Cheat_wave_raiders_easy()` |
| `ALL YOU NEED IS LOVE` | 363 | 18 | player | `APP:1` (`SetShareReveal`) 1, the team shares exploring |
| `I WANT TO BE ALONE` | 365 | 19 | player | `APP:1` 0 |
| `PLANET FARGO` | 440 | 20 | player | sound `PF66` (`Planet_fargo`'s first phase), no place; only `mx_sounddesc.cam` holds it, so a base quest plays nothing (**Measured**; **Game**: none heard) |
| `GROW UP` | 78 | 9 | selected hero | `advance_to_level(hero, APV:11 + 5)` |
| `BUILD ANYTHING` | 59 | 11 | player | sets the build flag |
| `FRAME IT` | 61 | 12 | player | toggles a frame-rate display: each game second the day counter shows frames over real ms, two decimals; the day is back at the next 5000 ms refresh; a click on the counter toggles it again (**Binary**). No player read: every peer's view toggles (`view::panel::day::Frames`) |
| `GIVE ME POWER` | 57 | 10 | player | toggles the every-level flag |

- Code 1 sets an engine flag already set in the file; code 7 is the
  switch's default (**Binary**). No phrase sends either.
- Not cheats: `LEAVE ME ALONE` (339) leaves a network game; `PING` times
  one.
- The waves are GPL (`mx_LowLevel.gpl`): each spawns monsters about the
  first palace while the map holds no more than `#Monster_Spawn_Cap`.
- A selection target with none selected, or `GROW UP` on no hero, sends
  nothing; the phrase goes out as plain chat. **Inferred**: the selection
  is a mobile first, else a building.

## What the two flags do

- Every-level flag: a player's building level answers 3 for any prefix, so
  every spell row is offered and research is priced as at Library level 3;
  the unit it hands back is still the real best. Spells reach everywhere,
  as with `APB:10`. The spell cursor skips a check for `Sp36`, `Sp37` (not
  read further).
- Build flag: the build menu lists and offers every `HumanPlayer` building,
  `NotBuildable` too; the preview skips the upgrade refusal and `BDEP` but
  keeps the cap, a pending placement, the ground and
  `CanIBuildThisBuilding`; an upgrade asks only that the building is
  finished.
- Set only by these cheats; a new game and a load clear both (**Binary**), and no save holds them.
- Both are global in the engine. Here each is per player
  (`World::every_level`, `World::build_cheat`): a player no human seat
  sees any human's, so a single-player game plays as the original.

**Measured**.

## What a save keeps

Everything a cheat changes in units, players and ground: gold, explored
vertices, attributes, the dead, spawned waves. Not the two flags, nor
`FRAME IT`. `SVGD` keeps whether cheats were allowed
([saves.md](../formats/saves.md#svgd--what-the-save-is)).

## In the world

`Input::Cheat` (`cheat CODE ARG` as text) is the message; the game
applies it among the frame's decodes. Modelled: every case but `FRAME
IT` (interface); `PLANET FARGO` is a cue (`Phase::First`, nowhere);
`APP:1` is `SetShareReveal`'s
([bytecode.md](../gpl/vm.md#world-builtins)). The phrases are read from
the loaded text (`cheat::recognise`), so a base quest's empty line is
`PLANET FARGO`. `openregent`: Enter opens a chat line
([openregent/game.md](../openregent/game.md#the-game)); an empty one sends the same.

## The Barren Waste probe's fourth cheat

The Barren Waste probe run decodes a fourth `CHEAT_CODE_MESSAGE` at frame
3242 (162.1 s, day 2). It adds no gold, and only meaningless characters
were typed then. **Inferred**: an empty line, so `PLANET FARGO`
(**Measured**).

## Sends err.log decodes nowhere

**Inferred**: a save sends a message two frames before its clock: each
chat test run's two saves follow an undecoded send by two frames
(**Measured**). The
last send before `END_PAUSE_AND_EXIT` is the exit's.
