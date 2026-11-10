# State

Target: Windows Gold HD ([original/editions.md](../original/editions.md)),
reimplemented in Rust on Bevy as `openregent`.

The reverse engineering is done. openregent plays every quest, as far as
known, as the original does; quest recordings guard it
([openregent/testing.md](../openregent/testing.md)).

| Strand | State |
|---|---|
| File formats | Every chunk type read ([formats/README.md](../formats/README.md)) |
| GPL | Every shipped and mod file parses; every shipped function compiles byte for byte; the VM runs as the engine's ([gpl/](../gpl/README.md)) |
| Simulation | Every world builtin answers from the world (`every_builtin_has_an_answer`) |
| Map generator | Reproduces every corpus save at every map size; terrain class 0.14 points short of its bar (**Measured**), accepted |
| Game | New game, load, LAN play, the front end ([openregent/game.md](../openregent/game.md)) |

Left:

- Differences from the original: [not-done.md](not-done.md).
- Unknowns about the original that might change play: [open-questions.md](open-questions.md).
- openregent's own additions: [openregent/enhancements.md](../openregent/enhancements.md).
