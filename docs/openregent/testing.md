# Testing

What guards openregent's behaviour. Test tiers and how to run them:
[crates/README.md](../../crates/README.md).

| Guard | Holds | Tier |
|---|---|---|
| Unit tests, crafted fixtures | Formats, operators, the VM, the world's rules one at a time | `code` |
| The test game (`regent_testgame`) | The real simulation on a small made-up game: facts after a seeded run; its one mod, Tough Walkers (`TOUGH_WALKERS`, a `mod.toml`), for tests of mods | `code` |
| Quest recordings | Whole quests played as recorded, to the win | `install` |
| Corpus tests | Every shipped and Workshop file reads; the compiler's output matches `Gplbcc.exe`'s | `install` |

## Quest recordings

One `.input` per quest in `assets/input_logs/`, played in openregent and
recorded: the setup, each order with its frame, and `Game::digest` every
100 frames (`DIGEST_EVERY`). No retail bytes. A recording is a session log
([game.md](game.md)); `--replay` plays one in the game.

`game::record`'s `replays` tests replay each to its last digest, then
check the quest is won within one digest interval (`WON_WITHIN`):

```text
 setup ──> Sim ──> step ──> digest at 100, 200, ... ──> compared with the log
                                 first mismatch stops: names the frame
 last digest ──> step on ──> quest won within 100 frames
```

Recorded: The Forsaken Land, Urban Renewal, The Siege, Day of Reckoning,
Balance of Twilight.

- Recorded without mods, unless a recording is meant to cover one.
- Enhancements do not reach the sim: it is built from the setup alone, so a
  recording plays alike whatever the player's `[enhancements]`.
- An intended rule change re-records the digests; the win check stays.
- A mismatch names the first frame whose digest differs.
- Not covered unless recorded: save and load mid-game, freestyle, LAN.

## Evidence before openregent

The original's behaviour was read from the executable (**Binary**) and
checked against saves and engine traces of an older HD data version
(**Measured**). The trace tooling is retired; those claims are no longer
held by a test. openregent, not the patched executable the traces came
from, is now the reference: a recording guards what a trace once checked.
