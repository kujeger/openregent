# OpenRegent

OpenRegent is an open-source RTS engine made to run the data from Cyberlore's excellent game "Majesty: The Fantasy Kingdom Sim", in Rust using the Bevy engine.

OpenRegent ships with no assets, and requires a Majesty Gold HD install in order to do anything.
See also [NOTICE.md](NOTICE.md).

## Current state
OpenRegent, when loading the Majesty Gold HD data, plays almost exactly like it. There are some minor differences (for example, the original game has an "instruction budget" per tick, which OpenRegent doesn't -- typically makes for a more responsive lategame), but nothing that should be overly noticeable by a player.

I've played through every quest without really noticing anything different.

## Supported datafiles
The only "supported" datafiles are from the 1.5.2.28 version (latest GOG, or the Steam beta branch). Older versions probably work, but YMMV.

The older Gold releases (Windows and Linux) have datasets that can be read, but they won't work right. It's out of scope for OpenRegent to work with those, at least for now.

## Configuration
Most settings are changeable in the game, but a couple are only available by editing the `config.toml` config file, such as paths to data or mods.
Config file can be found at:
- Linux: `$XDG_CONFIG_HOME/openregent/config.toml` (default `~/.config/openregent/config.toml`)
- Windows: `%APPDATA%\openregent\config.toml`
- macOS: `~/Library/Application Support/openregent/config.toml`

## Mods
Mods are supported! If they work with HD, they should work with OpenRegent. I haven't tested _all_ mods, but have tested a few.

OpenRegent reads mods from, in order:
1. Built-in mods.
2. Its own mod folder:
   - Linux: `$XDG_DATA_HOME/openregent/mods` (default `~/.local/share/openregent/mods`)
   - Windows: `%APPDATA%\openregent\mods`
   - macOS: `~/Library/Application Support/openregent/mods`
3. `Mods` in the folder OpenRegent is started from.
4. Folders listed under `[paths] mods` in the configuration file.
5. The Steam Workshop folder: `MAJESTY_WORKSHOP` if set, else
   - Linux: `~/.local/share/Steam/steamapps/workshop/content/73230`
   - macOS: `~/Library/Application Support/Steam/steamapps/workshop/content/73230`
   - Windows: `C:\Program Files (x86)\Steam\steamapps\workshop\content\73230`

   Only Steam's default library is checked. If Majesty is in another library, add the folder to `[paths] mods`.

If two folders hold the same mod, the first one wins.

OpenRegent also adds a few new features for mods:
- a different toml-based way of specifying them
- no need to compile mods in advance if they don't ship art -- OpenRegent will happily load and run plain text mods.
- ability to patch some attributes instead of copying entire objects and overwriting them

See [examples](crates/openregent/examples) for some, well, examples.

There are also two [built-in mods](crates/openregent/mods):
- `bugfixes` - Bugfixes for shipped data go here
- `house_mend` - Simple mod that lets peasant houses slowly heal themselves, so the entire village isn't on fire all the time

## Multiplayer
Multiplayer works! Both local LAN, and over the internet using `iroh` (`iroh` will also work over lan and should usually work out a direct connection if possible).

Multiplayer is not as tested as singleplayer.

## Development

### Building
clone repo, and run
- `just build` - builds dev
- `just release` - builds release

### Tests
`just test` runs all tests _that don't require data files_.

`just test-install` also runs the tests that _require_ original Majesty data files, specified by the path in `.env` (see `.env.example`).

### Pre-commit hook
`git config --local core.hooksPath .githooks` makes each commit run `just addresses`, which fails on binary internals in tracked files.

## Enhancements
OpenRegent does have some "enhancements" that are enabled by default, such as:
- position interpolation, that makes units "jump" less when moving around.
- xBRZ upscaled interface art -- much nicer than plain scaling
- several filter options when ingame (e.g. FSR)
There are more -- check ingame Settings to see them, or see (the generated) [enhancements.md](docs/openregent/enhancements.md).

## High-res portraits
Cyberlore has some "collectible cards" available at their website at https://www.cyberlore.com/majesty/downloads.htm . These happen to contain much higher resolution portraits of most heroes plus a few monsters, and OpenRegent can use those!

`tools/cards.py` downloads the cards and extracts the portraits. Run it with [uv](https://docs.astral.sh/uv/): `uv run tools/cards.py`.

## AI Use
OpenRegent is almost entirely vibe-coded -- sorry -- so both code and documentation quality can be a bit off. Buyer beware.
It still represents quite a few hours of human playtesting and making sure it plays like the original game.

## The original game data and GPL (Game Play Language)
Almost all of Majesty's quest and game logic is written in a DSL called GPL, which was a great boon for this project: by creating an interpreter for GPL, plus the engine internals to run it, the road to a playable game was surprisingly short.

Since the Majesty Gold HD edition also ships with an SDK and documentation, this was made even more feasible!

## Documentation
Engine and reverse engineering information exists under [docs/](docs/README.md). This is all generated, so is not particularly fit for human consumption -- but it seems to work well enough for LLMs.

## Third-party code

`crates/bink/` is Kostya Shishkov's Bink 1 video and audio decoder from [NihAV](https://nihav.org) (`git://git.nihav.org/nihav.git`, commit `13f31633cf32`), AGPL-3, vendored and trimmed.
Its [README](crates/bink/README.md) lists the files taken and what changed.

## Third-party assets

`crates/openregent/assets/fonts/inknut-antiqua/` is Claus Eggers Sørensen's Inknut Antiqua, SIL Open Font License 1.1, built into openregent as its text font. Its [README](crates/openregent/assets/fonts/inknut-antiqua/README.md) says which files. `openregent --licences` prints the licence.
