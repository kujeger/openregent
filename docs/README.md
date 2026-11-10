# Documents

The reverse engineering of Majesty Gold HD is done; openregent plays every
quest. State: [status/state.md](status/state.md).

| Where | Holds |
|---|---|
| [settled.md](settled.md) | One line per settled question about the original. Read before calling something unknown |
| [original/](original/README.md) | The original game: editions, class map, engine behaviour |
| [formats/](formats/README.md) | The original's file formats, one document per format |
| [gpl/](gpl/README.md) | GPL, the scripting language: the language, its runtime, our compiler |
| [openregent/](openregent/README.md) | The reimplementation: crates, front end, testing, departures from the original |
| [status/](status/state.md) | The project's state, differences from the original, open questions |

## Evidence

Labels, by evidence:

| Label | Evidence |
|---|---|
| **Binary** | Read from the executable. No test holds it: the executable is not in the repo |
| **Corpus** | Holds over the game's files or saves |
| **Measured** | Measured on saves or traces of an older HD data version. No test holds it now |
| **Game** | Seen in the original game, in play or a screenshot |
| **Viewer** | Looks right drawn in `regent_viewer` |
| **Inferred** | A guess |

## Ground rules

- Publish format specifications, not disassembly.
- Ship no game assets, exported images included.
- Read the install's `EULA` before publishing.
- Mark every finding with its evidence, above.
- Never modify the game install.
