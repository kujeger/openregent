//! The settings keys the engine accepts; far more than any file sets.
//!
//! The base `GOPT` sets 25. Three tables hold the keys:
//!
//! | Table | Read by | Keys |
//! |---|---|---|
//! | [`GAME`] | `GSINIFile` | 71 |
//! | [`DATASET`] | the dataset XML reader | 8 |
//! | [`ENGINE`] | `GEINIFile` | 39 |
//!
//! 118 keys in the base game.
//!
//! ## Order and case number
//!
//! The parser pairs each key with a case number and switches on it. Position
//! is not the case: `IsoCameraAngle` is [`ENGINE`]`[18]` and case 34 in HD.
//! Key, case, global and reader per key:
//! [`cases`](super::cases).
//!
//! [`GAME`] is two tables in one run: the first [`GAME_CASES`] keys are the
//! parser's, the rest persistent settings with defaults and no case
//! ([`cases::PREFS`](super::cases::PREFS) in HD).
//!
//! ## What each edition holds
//!
//! | Build | `GSINIFile` | Total |
//! |---|---|---|
//! | Base game | [`GAME`], 71 | 118 |
//! | Expansion | [`EXPANSION_GAME`], 77 | 124 |
//! | HD | [`EXPANSION_GAME`] + [`HD_EXTRA`], 90 | 134 distinct |
//!
//! HD's 137 entries hold 134 names: three of its thirteen additions are
//! [`ENGINE`] keys. HD also inserts [`HD_INSERTED`] into the parser keys.
//!
//! Only the game-side table grows. The expansion inserts four of its six, so
//! an index above 36 differs between games; hence [`EXPANSION_GAME`] is whole,
//! as in [`attrib`](crate::data::attrib).
//!
//! ## The Windows layout is not a run
//!
//! MSVC emits these strings reversed and pools duplicates (`CDVolume` once for
//! two tables). So the tables' order was read from a g++ build's forward,
//! unpooled runs; that evidence is in `local/archive/linux-port.md`.

/// `GSINIFile`'s keys, in the order the base game's binaries hold them.
///
/// Quest and save paths, volumes, list sort orders, difficulty bands, `CYLib`
/// caches, and the freestyle slots.
pub const GAME: [&str; 71] = [
    "ConstructData1",
    "ConstructData2",
    "QuestDirectory",
    "SaveGamePath",
    "GameValue1",
    "GameValue2",
    "FramesPerAutosave",
    "FramesPerChecksum",
    "ExecutionWindow",
    "TimePerFrame",
    "BundleFactor",
    "ReportIfFrameLongerThan",
    "TossPlayerAfter",
    "GPLLinesPerFrame",
    "DefaultCDVolume",
    "DefaultSFXVolume",
    "DefaultVoiceVolume",
    "AudibleVolumeLimit",
    "MaxConcurrentSounds",
    "DefaultTaxRoute",
    "SoundViewRadius",
    "FramesPerExecutionAdjustment",
    "DifficultyLimitBeginnerLow",
    "DifficultyLimitBeginnerHigh",
    "DifficultyLimitAdvancedLow",
    "DifficultyLimitAdvancedHigh",
    "DifficultyLimitExpertLow",
    "DifficultyLimitExpertHigh",
    "CYLibCacheTotal",
    "CYLibCacheTILELow",
    "CYLibCacheTILEHigh",
    "CYLibCacheWAVELow",
    "CYLibCacheWAVEHigh",
    "CYLibCachePercentTotal",
    "Language",
    "CYLibGarbageCollectionInterval",
    "CYLibGarbageCollectionLRULimit",
    "LastSaveGameUsed",
    "LastNameUsed",
    "DisplayFlagNames",
    "DisplayFlagStats",
    "DisplayFlagTerrain",
    "BuildingListVerbose",
    "BuildingListShowUnavailable",
    "SFXVolume",
    "VoiceVolume",
    "CDVolume",
    "HeroListVerbose",
    "HeroListSortType",
    "SpellFastBook",
    "LastSessionNameUsed",
    "HenchmenListSortType",
    "BuildingsListSortType",
    "ScrollSpeed",
    "RGS1",
    "RGS2",
    "RGS3",
    "RGS4",
    "RGS5",
    "RGS6",
    "RGS7",
    "RGS8",
    "RGS9",
    "AutosaveConfig",
    "ZoomMessageFlags",
    "ProtocolLastUsed",
    "WhichCinematic",
    "SoundPlaybackFrequency",
    "SoundPlaybackBitsPerSample",
    "SoundPlaybackChannels",
    "PerformanceValue1",
];

/// The expansion's `GSINIFile` table, which is [`GAME`] plus six.
///
/// Four inserted after `CYLibGarbageCollectionLRULimit` (`MajestyInstallPath`,
/// `BuildTreePath`, `FreeStyleIOPath`, `ExpansionDataPath`) shift every index
/// above 36. `RGS_SpecialEvent1` and `2` are appended: freestyle slots ten and
/// eleven. Kept whole, like [`attrib::EXPANSION`].
///
/// [`attrib::EXPANSION`]: crate::data::attrib::EXPANSION
pub const EXPANSION_GAME: [&str; 77] = [
    "ConstructData1",
    "ConstructData2",
    "QuestDirectory",
    "SaveGamePath",
    "GameValue1",
    "GameValue2",
    "FramesPerAutosave",
    "FramesPerChecksum",
    "ExecutionWindow",
    "TimePerFrame",
    "BundleFactor",
    "ReportIfFrameLongerThan",
    "TossPlayerAfter",
    "GPLLinesPerFrame",
    "DefaultCDVolume",
    "DefaultSFXVolume",
    "DefaultVoiceVolume",
    "AudibleVolumeLimit",
    "MaxConcurrentSounds",
    "DefaultTaxRoute",
    "SoundViewRadius",
    "FramesPerExecutionAdjustment",
    "DifficultyLimitBeginnerLow",
    "DifficultyLimitBeginnerHigh",
    "DifficultyLimitAdvancedLow",
    "DifficultyLimitAdvancedHigh",
    "DifficultyLimitExpertLow",
    "DifficultyLimitExpertHigh",
    "CYLibCacheTotal",
    "CYLibCacheTILELow",
    "CYLibCacheTILEHigh",
    "CYLibCacheWAVELow",
    "CYLibCacheWAVEHigh",
    "CYLibCachePercentTotal",
    "Language",
    "CYLibGarbageCollectionInterval",
    "CYLibGarbageCollectionLRULimit",
    "MajestyInstallPath",
    "BuildTreePath",
    "FreeStyleIOPath",
    "ExpansionDataPath",
    "LastSaveGameUsed",
    "LastNameUsed",
    "DisplayFlagNames",
    "DisplayFlagStats",
    "DisplayFlagTerrain",
    "BuildingListVerbose",
    "BuildingListShowUnavailable",
    "SFXVolume",
    "VoiceVolume",
    "CDVolume",
    "HeroListVerbose",
    "HeroListSortType",
    "SpellFastBook",
    "LastSessionNameUsed",
    "HenchmenListSortType",
    "BuildingsListSortType",
    "ScrollSpeed",
    "RGS1",
    "RGS2",
    "RGS3",
    "RGS4",
    "RGS5",
    "RGS6",
    "RGS7",
    "RGS8",
    "RGS9",
    "AutosaveConfig",
    "ZoomMessageFlags",
    "ProtocolLastUsed",
    "WhichCinematic",
    "SoundPlaybackFrequency",
    "SoundPlaybackBitsPerSample",
    "SoundPlaybackChannels",
    "PerformanceValue1",
    "RGS_SpecialEvent1",
    "RGS_SpecialEvent2",
];

/// How many of [`GAME`]'s keys the base game's parser has a case for; the
/// rest are persistent settings.
pub const GAME_CASES: usize = 37;

/// How many of [`EXPANSION_GAME`]'s keys the expansion's parser has a case
/// for: [`GAME_CASES`] plus the four inserted paths.
pub const EXPANSION_CASES: usize = 41;

/// The two parser keys both HD builds insert after
/// `CYLibGarbageCollectionLRULimit`, as cases 37 and 38.
///
/// `LocalPath` shadows [`ENGINE`]'s `LOCALPATH`: `GOPT` reaches the game
/// table first, and its case drops the value.
pub const HD_INSERTED: [&str; 2] = ["DirectShowDecodingMethod", "LocalPath"];

/// The thirteen persistent settings HD appends to [`EXPANSION_GAME`], in
/// order.
///
/// A diff: HD appends and moves no index.
///
/// Ten are new: a Direct3D path, a window, a frame-rate switch, SDK tools.
/// `UseGPLDebugger` enables the debugger whose five panels `docs/formats/panels.md`
/// reads.
///
/// The `Pix*` trio is copied from [`ENGINE`] (where it stays in every
/// edition). The game-side table serialises itself, so HD's `MajXPrefs`
/// writes a resolution. HD's `GEINIFile`
/// still reads them from `GOPT`; `MajXPrefs` then overrides
/// ([`cases::PREFS`](super::cases::PREFS)).
///
/// `MajestyHD - Old.exe`, an earlier HD build, has the first eight and none of
/// the last five.
pub const HD_EXTRA: [&str; 13] = [
    "PixWidth",
    "PixHeight",
    "PixDepth",
    "Windowed",
    "DebugFlags",
    "VSync",
    "BlitMode",
    "IntroVideo",
    "UseD3D",
    "UseGPLDebugger",
    "UseDebugWindow",
    "SDKPath",
    "MapDrag",
];

/// The dataset XML reader's element names, identical in every edition.
///
/// Not an INI table: tags in `datasets.xml` (`DataGroups` is also `majxprefs`'
/// root). Their own run; kept to tell such names apart from keys.
pub const DATASET: [&str; 8] = [
    "XmlDataRoot",
    "LoadData",
    "DataGroups",
    "LoadDataGroup",
    "LoadCam",
    "DATAMX",
    "LoadGPL",
    "UnloadData",
];

/// `GEINIFile`'s keys, the engine-side table. Identical in every edition.
///
/// Renderer, paths, terrain lighting, projectile arcs. Some are set by no
/// shipped file and run at binary defaults, such as
/// [`ISO_CAMERA_ANGLE`](super::ISO_CAMERA_ANGLE) (index 18). HD has no case
/// for `HeightThreshold`, `OdbcPath` or `ConstructBuffer`: see
/// [`cases::ENGINE`](super::cases::ENGINE).
///
/// The binary follows them with the twelve values the parser accepts (`TRUE`,
/// `OFF`, the four bit depths, `DirectDraw`, `DIB`).
pub const ENGINE: [&str; 39] = [
    "CDPATH",
    "LOCALPATH",
    "LOADCAM",
    "UsePlayerColorPaletteRange",
    "PlayerColorPaletteRangeStart",
    "PlayerColorPaletteRangeEnd",
    "ScrollSpeedLow",
    "ScrollSpeedHigh",
    "PixDepth",
    "PixWidth",
    "PixHeight",
    "PixExclusive",
    "PixAlloc",
    "LightX",
    "LightY",
    "LightZ",
    "Contrast",
    "Ambient",
    "IsoCameraAngle",
    "MaxSlope",
    "ShowIntro",
    "MaxCYLibs",
    "ExternalLogging",
    "GPLPATH",
    "GplsPerFrame",
    "GplPseudorandom",
    "OdbcPath",
    "XmlPath",
    "ConstructBuffer",
    "HeightThreshold",
    "CDAudio",
    "CDVolume",
    "PathHorizon",
    "RepathLength",
    "CollisionDelay",
    "ExpandCount",
    "ParabolaGravity",
    "ParabolaAngle",
    "DeferredSpriteLoading",
];

/// Which of the three tables a key belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Table {
    /// [`GAME`], and the expansion's and HD's additions to it.
    Game,

    /// [`DATASET`].
    Dataset,

    /// [`ENGINE`].
    Engine,
}

/// Every table that holds `key`, over every edition's vocabulary.
///
/// A list: six names are in two tables.
///
/// | Name | In |
/// |---|---|
/// | `CDVolume` | [`GAME`] and [`ENGINE`] |
/// | `LoadCam` / `LOADCAM` | [`DATASET`] and [`ENGINE`] |
/// | `LocalPath` / `LOCALPATH` | HD's game-side parser and [`ENGINE`] |
/// | `PixWidth`, `PixHeight`, `PixDepth` | [`ENGINE`] always, and HD's game-side table too |
///
/// Case-insensitive: a shipped `GOPT` writes `CDAUDIO` and `LOADCAM` for the
/// binary's `CDAudio` and `LoadCam`.
///
/// ```
/// use regent_core::data::settings::keys::{self, Table};
///
/// assert_eq!(keys::tables("IsoCameraAngle"), [Table::Engine]);
/// assert_eq!(keys::tables("cdaudio"), [Table::Engine]);
/// assert_eq!(keys::tables("RGS9"), [Table::Game]);
/// assert_eq!(keys::tables("UseGPLDebugger"), [Table::Game]);
/// assert_eq!(keys::tables("PixWidth"), [Table::Game, Table::Engine]);
/// assert_eq!(keys::tables("DirectShowDecodingMethod"), [Table::Game]);
/// assert_eq!(keys::tables("GAMEOPT"), []);
/// ```
#[must_use]
pub fn tables(key: &str) -> Vec<Table> {
    let holds = |run: &[&str]| run.iter().any(|k| k.eq_ignore_ascii_case(key));

    [
        (
            Table::Game,
            holds(&EXPANSION_GAME) || holds(&HD_EXTRA) || holds(&HD_INSERTED),
        ),
        (Table::Dataset, holds(&DATASET)),
        (Table::Engine, holds(&ENGINE)),
    ]
    .into_iter()
    .filter_map(|(table, held)| held.then_some(table))
    .collect()
}

/// Whether any edition's engine knows `key` at all.
#[must_use]
pub fn known(key: &str) -> bool {
    !tables(key).is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_expansion_inserts_four_keys_and_appends_two() {
        // The insertion point, which shifts every index above it.
        assert_eq!(GAME[36], "CYLibGarbageCollectionLRULimit");
        assert_eq!(EXPANSION_GAME[36], "CYLibGarbageCollectionLRULimit");
        assert_eq!(EXPANSION_GAME[37], "MajestyInstallPath");
        assert_eq!(GAME[37], "LastSaveGameUsed");

        // The two on the end: freestyle slots ten and eleven.
        assert_eq!(
            &EXPANSION_GAME[75..],
            ["RGS_SpecialEvent1", "RGS_SpecialEvent2"]
        );

        // Nothing is dropped: the base table is the expansion's, minus six.
        for key in GAME {
            assert!(EXPANSION_GAME.contains(&key), "{key} is gone");
        }

        assert_eq!(EXPANSION_GAME.len(), GAME.len() + 6);
    }

    /// Every key names its own table, and exactly six name two.
    #[test]
    fn six_names_are_in_two_tables_and_the_rest_in_one() {
        for key in EXPANSION_GAME
            .into_iter()
            .chain(HD_EXTRA)
            .chain(HD_INSERTED)
        {
            assert!(tables(key).contains(&Table::Game), "{key}");
        }

        for key in DATASET {
            assert!(tables(key).contains(&Table::Dataset), "{key}");
        }

        for key in ENGINE {
            assert!(tables(key).contains(&Table::Engine), "{key}");
        }

        let shared: Vec<&str> = EXPANSION_GAME
            .into_iter()
            .chain(HD_EXTRA)
            .chain(HD_INSERTED)
            .chain(DATASET)
            .chain(ENGINE)
            .filter(|key| tables(key).len() > 1)
            .collect();

        assert_eq!(
            shared,
            [
                "CDVolume",
                "PixWidth",
                "PixHeight",
                "PixDepth",
                "LocalPath",
                "LoadCam",
                "LOCALPATH",
                "LOADCAM",
                "PixDepth",
                "PixWidth",
                "PixHeight",
                "CDVolume",
            ],
            "a name is in two tables that was not before"
        );
    }

    /// The one key the engine names and nothing has ever written.
    #[test]
    fn the_ini_file_the_engine_looks_for_is_no_key_of_its_own() {
        assert!(!known("GAMEOPT.INI"));
        assert!(!known("GAMEOPT"));
    }
}
