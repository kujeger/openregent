# openregent

The reimplementation: its crates, front end and departures from the original.

| File | Holds |
|---|---|
| [viewer.md](viewer.md) | The resolution layer, `regent_viewer` (the art browser) |
| [game.md](game.md) | The game: start options, menus, the game loop, text, the selection panel |
| [saves.md](saves.md) | openregent's own save format |
| [multiplayer.md](multiplayer.md) | LAN play: lockstep, discovery |
| [determinism.md](determinism.md) | Play as a function of setup, seed and input |
| [screenshots.md](screenshots.md) | `--screenshot`, and what the drawn game shows |
| [perf.md](perf.md) | Perf runs and their figures |
| [shadows.md](shadows.md) | Shadow drawing modes |
| [decisions.md](decisions.md) | Structural decisions |
| [art-service.md](art-service.md) | The art service: how pictures are loaded and kept |
| [artist-notes.md](artist-notes.md) | Camera and sun for modelling building proxies |
| [enhancements.md](enhancements.md) | Departures from the original |
| [testing.md](testing.md) | What guards behaviour: the test game, quest recordings |

## This workspace's crates

The reimplementation's split, not the game's:

```text
 regent_core             readers, GPL, the world the scripts run in,
    │                    the map generator
    ├── regent_cli          inspection tools, the GPL compiler
    └── regent_draw         drawing: corpus, map, stage, materials, shaders,
          │              the art service; no game rules
          ├── openregent   the game: steps the world; menus, panels, audio,
          │              saves, settings
          └── regent_viewer the art browser: units, tileset
 bink                    the videos' decoder, vendored; used by openregent
```

`regent_draw` draws what it is given, as `CY`'s rendering serves `GE_` and
`GS_`.

The library's layers, lowest first; each uses only those below it:

```text
 base    engine-wide primitives: RNG, fixed-order maps, fixed point
 data    file readers
 content an install found, and the joins over its files
 save    openregent's save model
 gpl     the language and its VM
 sim     the world and the rules it runs: the GE_ and GS_ game layers;
         the game driver and the live host
 view    what the screen shows of data and sim
```

Per crate, and the test tiers: [crates/README.md](../../crates/README.md).

