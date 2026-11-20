# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [Unreleased]

## [0.7.0] - 2026-11-20

### Changed

- WASD scroll setting moved from `enhancements` to `settings`.
- Better info text for settings
- Fix text placement on some buttons (e.g. "HEROES")
- Internally, settings system was redesigned

## [0.6.0] - 2026-11-20

### Added

- WASD-style scrolling, as a setting. Enabling it will make WASD button scroll like the arrow keys, but will override conflicting upgrade hotkeys

### Fixed

- Play sound effects when a peasant (or other) builds a building
- Play sound effect when opening a treasure chest

## [0.5.0] - 2026-11-18

### Changed

- QUIT MAJESTY and RESUME PLAY buttons in Options now wrap, avoiding ugly overflow
- Shapechanged heroes retain their proper panels -- no longer will a cultist seem to be a monster just because they happen to be a Hellbear
- Peasants at work / Magic at work art now shows for buildings under construction
- Stricter GPL compiler, matching the original Gplbcc.exe better

## [0.4.0] - 2026-11-17

### Changed

- Placeholder gui interfaces now use proper art

## [0.3.0] - 2026-11-15

### Added

- Several performance improvements, allowing smoother play on older hardware

### Changed

- Book of Lore now reads Lair spawn info dynamically from data files

## [0.2.0] - 2026-11-12

### Added

- Mods in LAN games: the host picks them in the lobby. Every player must have the same version.

## [0.1.1] - 2026-11-10

### Changed

- More permissive data checks in Multiplayer

## [1.0.0] - 2026-11-10

Initial release
