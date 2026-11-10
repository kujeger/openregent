//! Second acceptance corpus: Steam Workshop mods for Majesty HD.
//!
//! Compiled by the same `Gplbcc.exe` and loaded by the shipped game, so this is
//! legal GPL whatever the documentation says. Unlike the base corpus, mods use
//! `break`, `continue`, non-literal attribute names, and `mod` as a variable.
//!
//! Counts are lower bounds with their snapshot date; they guard against a
//! reader that stops finding things.
//!
//! Two mods ship `.cam` archives (sprites, palettes, sounds, descriptors), a
//! second corpus for the data formats.

mod common;

use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};

use regent_core::data::catalogue::Catalogue;
use regent_core::data::{cam, imag, payload, tile};
use regent_core::gpl::ast::{Expr, ExprKind, StmtKind};
use regent_core::gpl::resolve::Report;
use regent_core::gpl::{Token, lex};
use regent_core::view::{Colour, PaletteSource, Sprite};

fn sdk_quests() -> PathBuf {
    common::sdk().join("OriginalQuests")
}

/// Every `(chunk tag, record name)` pair in a set of archives.
///
/// Names are unique only per chunk; a sprite's `IMAG` and `TILE` share one.
fn contents(paths: &[PathBuf]) -> HashSet<(regent_core::data::FourCc, String)> {
    let mut out = HashSet::new();

    for path in paths {
        let bytes = std::fs::read(path).expect("readable archive");
        let archive =
            cam::Archive::parse(&bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()));

        for (chunk, record) in archive.records() {
            out.insert((chunk.tag, record.name().into_owned()));
        }
    }

    out
}

fn sources(dir: &Path, ext: &str) -> Vec<(PathBuf, String)> {
    common::walk(dir, &[ext])
        .into_iter()
        .map(|p| {
            let bytes = std::fs::read(&p).expect("readable source file");
            // Some files have stray high bytes in comments; decode lossily.
            (p, String::from_utf8_lossy(&bytes).into_owned())
        })
        .collect()
}

/// Snapshot 2026-09-02: 11 mods, 361 `.gpl` files, 6.4 MB, 841,314 tokens,
/// 3,252 functions, 91 prototypes, 2,557 constants.
#[test]
fn every_mod_file_lexes_and_parses() {
    let dir = common::workshop();
    let files = sources(&dir, "gpl");

    assert!(
        files.len() >= 300,
        "expected the workshop mods, found {}",
        files.len()
    );

    for (path, src) in &files {
        let lexed = lex(src);
        assert!(
            lexed.is_ok(),
            "{}: {:?}",
            path.display(),
            &lexed.errors[..lexed.errors.len().min(3)]
        );

        let parsed = regent_core::gpl::parse(src);
        assert!(
            parsed.is_ok(),
            "{}: {:?}",
            path.display(),
            &parsed.errors[..parsed.errors.len().min(3)]
        );
    }
}

/// Snapshot 2026-09-02: 47 files, 956 prototypes, 16,284 values.
#[test]
fn every_mod_dat_file_parses() {
    let dir = common::workshop();
    let files = sources(&dir, "dat");
    let (mut prototypes, mut values) = (0, 0);

    assert!(
        files.len() >= 40,
        "expected mod .dat files, found {}",
        files.len()
    );

    for (path, src) in &files {
        let parsed = regent_core::gpl::dat::parse(src);
        assert!(parsed.is_ok(), "{}: {:?}", path.display(), parsed.errors);

        prototypes += parsed.prototypes.len();
        values += parsed
            .prototypes
            .iter()
            .map(|p| p.values.len())
            .sum::<usize>();
    }

    assert!(prototypes >= 956, "prototype count fell to {prototypes}");
    assert!(values >= 16_284, "value count fell to {values}");
}

/// Snapshot 2026-09-02: 48 uses in 3 files, including `agent mod;` in
/// functions present in shipped `.bcd` files, so `Gplbcc.exe` accepts it. No
/// source uses `mod` as an operator.
#[test]
fn mod_is_a_variable_name() {
    let dir = common::workshop();
    let mut uses = 0;

    for (_, src) in sources(&dir, "gpl") {
        uses += lex(&src)
            .tokens
            .iter()
            .filter(|t| matches!(t.node, Token::Ident(n) if n.eq_ignore_ascii_case("mod")))
            .count();
    }

    assert!(uses >= 40, "expected `mod` used as a name, found {uses}");
}

/// Snapshot 2026-09-02: 2 `break`s, 1 `continue`, 610 identifier attribute
/// names, 36 parenthesised ones, 157 chained accessors.
#[test]
fn mods_exercise_forms_the_base_corpus_never_uses() {
    let dir = common::workshop();
    let (mut breaks, mut continues) = (0, 0);
    let (mut computed_attrs, mut chained) = (0, 0);

    for (_, src) in sources(&dir, "gpl") {
        let script = regent_core::gpl::parse(&src).script;

        script.visit_stmts(&mut |s| match s.kind {
            StmtKind::Break => breaks += 1,
            StmtKind::Continue => continues += 1,
            _ => {}
        });

        script.visit_exprs(&mut |e: &Expr<'_>| {
            if let ExprKind::Attr { obj, attr } = &e.kind {
                // Usually a string literal, but any expression works:
                // a variable, or `("prefix" + n)`.
                if !matches!(attr.kind, ExprKind::Str(_)) {
                    computed_attrs += 1;
                }

                if matches!(obj.kind, ExprKind::Attr { .. }) {
                    chained += 1;
                }
            }
        });
    }

    assert!(breaks >= 2, "`break` statements: {breaks}");
    assert!(continues >= 1, "`continue` statements: {continues}");
    assert!(
        computed_attrs >= 500,
        "computed attribute names: {computed_attrs}"
    );
    assert!(chained >= 100, "chained `'s` accessors: {chained}");
}

/// Mods call base-game functions, so resolve against both trees. No mod calls
/// a builtin outside the 166 the SDK documents.
///
/// Snapshot 2026-09-02: 667 files, 21,515 script and 58,243 builtin calls.
#[test]
fn mod_calls_resolve_against_the_base_game() {
    let dir = common::workshop();

    let quests = sdk_quests();

    let mut files = sources(&quests, "gpl");
    files.extend(sources(&dir, "gpl"));

    let mut report = Report::default();

    for (_, src) in &files {
        report.add(&regent_core::gpl::parse(src).script);
    }

    let unresolved = report.unresolved();
    assert!(unresolved.is_empty(), "unresolved `$name`: {unresolved:?}");

    let (script, builtin, none) = report.counts();
    assert_eq!(none, 0);
    assert!(script > 0 && builtin > 0);
}

/// Payload readers hold on bytes Cyberlore never wrote.
///
/// Snapshot 2026-09-02: 14 archives in 2 of the 11 mods, 22,194 records —
/// 21,220 `TILE`, 867 `SPLT`, 35 `IMAG`, 34 `WAVE`, and the rest descriptors,
/// string tables and menus.
#[test]
fn every_mod_archive_parses() {
    let dir = common::workshop();
    let files = common::archives(&dir);

    assert!(
        files.len() >= 14,
        "expected mod archives, found {}",
        files.len()
    );

    let mut records = 0;

    for path in &files {
        let bytes = std::fs::read(path).expect("readable archive");
        let archive =
            cam::Archive::parse(&bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()));

        assert_eq!(archive.version, (1, 1), "{}", path.display());

        for (chunk, record) in archive.records() {
            let tag = chunk.tag.as_bytes();
            let at = || format!("{} {} {}", path.display(), chunk.tag, record.id);

            if payload::WAVE_TAGS.contains(&tag) {
                payload::wave(record.data).unwrap_or_else(|e| panic!("{}: {e}", at()));
            } else if payload::PALETTE_TAGS.contains(&tag) {
                let palette = payload::Palette::parse(record.data)
                    .unwrap_or_else(|e| panic!("{}: {e}", at()));

                assert_eq!(palette.colours.len(), payload::PALETTE_COLOURS, "{}", at());
            } else if payload::DESCRIPTOR_TAGS.contains(&tag) {
                let descriptor = payload::Descriptor::parse(record.data)
                    .unwrap_or_else(|e| panic!("{}: {e}", at()));

                // Payload repeats its chunk's tag, as in the install.
                assert_eq!(descriptor.tag, chunk.tag, "{}", at());
            } else if tag == b"STRT" {
                payload::StringTable::parse(record.data)
                    .unwrap_or_else(|e| panic!("{}: {e}", at()));
            }

            records += 1;
        }
    }

    assert!(records >= 22_000, "record count fell to {records}");
}

/// Phantom adds a unit, a three-level guild and a spell set, each with
/// sprites. The other archives mostly redistribute HD expansion content.
///
/// Snapshot 2026-09-02: 667 `(tag, name)` pairs the install does not have —
/// 638 `TILE`, 26 `IMAG`, 2 `DSND` and 1 `WAVE`.
#[test]
fn mods_ship_content_the_install_does_not() {
    let dir = common::workshop();

    let install = common::hd();

    let shipped = contents(&common::archives(&install));
    let modded = contents(&common::archives(&dir));

    let graphics = modded
        .difference(&shipped)
        .filter(|(tag, _)| matches!(tag.as_bytes(), b"IMAG" | b"TILE"))
        .count();

    assert!(graphics >= 600, "new sprite records fell to {graphics}");
}

/// Tile decoding on art unseen when the format was derived.
///
/// `TILE` references are positional, so overriding tile 17,000 needs 17,000
/// slots; most mod tile records are empty padding.
///
/// Snapshot 2026-09-02: 21,220 records across two mods: 19,274 empty, 1,876
/// version 3, 69 version 1, 1 version 2.
#[test]
fn every_mod_tile_decodes() {
    let dir = common::workshop();
    let (mut v3, mut empty, mut raw) = (0, 0, 0);

    for path in common::archives(&dir) {
        let bytes = std::fs::read(&path).expect("readable archive");
        let archive = cam::Archive::parse(&bytes).unwrap();

        for (chunk, record) in archive.records() {
            if chunk.tag != b"TILE" {
                continue;
            }

            // Padding slot.
            if record.data.is_empty() {
                empty += 1;
                continue;
            }

            let version = tile::Tile::version_word(record.data).unwrap();

            assert!(
                matches!(version, 1..=3),
                "{} {}: tile version {version}",
                path.display(),
                record.name()
            );

            let t = tile::Tile::parse(record.data).unwrap_or_else(|e| {
                panic!("{} tile v{version} {}: {e}", path.display(), record.name())
            });

            if let Some(rows) = t.runs() {
                assert_eq!(rows.len(), usize::from(t.height()));
                v3 += 1;
            } else {
                raw += 1;
            }

            assert!(
                t.max_end() <= t.width(),
                "{} {}",
                path.display(),
                record.name()
            );
        }
    }

    assert!(v3 >= 1_800, "mod v3 tile count fell to {v3}");
    assert!(empty >= 19_000, "padding-slot count fell to {empty}");
    assert!(raw >= 60, "mod v1/v2 tile count fell to {raw}");
}

/// All four `IMAG` layouts decode on non-Cyberlore art.
///
/// Snapshot 2026-09-02: 35 records (versions 2/3/4/5: 2, 3, 29, 1), 371
/// slots, 3,570 frames.
#[test]
fn every_mod_imag_record_decodes() {
    let dir = common::workshop();
    let (mut records, mut slots, mut frames) = (0, 0, 0);
    let mut versions: BTreeSet<u32> = BTreeSet::new();
    let (mut tiles, mut other) = (0, 0);

    for path in common::archives(&dir) {
        let bytes = std::fs::read(&path).expect("readable archive");
        let archive = cam::Archive::parse(&bytes).unwrap();
        let count = archive.chunk_by_tag(b"TILE").map_or(0, |c| c.records.len());

        for (chunk, record) in archive.records() {
            if chunk.tag != b"IMAG" || record.data.is_empty() {
                continue;
            }

            versions.insert(imag::Image::version_word(record.data).unwrap());

            let image = imag::Image::parse(record.data)
                .unwrap_or_else(|e| panic!("{} {}: {e}", path.display(), record.name()));

            records += 1;
            slots += image.slot_count();
            frames += image.frame_count();

            for set in &image.sets {
                for slot in &set.slots {
                    for frame in slot.frames() {
                        match frame.target() {
                            imag::FrameRef::Tile { index, .. } => {
                                // Positional, into this archive's tiles.
                                assert!(
                                    usize::from(index) < count,
                                    "{} {}: tile {index} of {count}",
                                    path.display(),
                                    record.name()
                                );
                                tiles += 1;
                            }
                            _ => other += 1,
                        }
                    }
                }
            }
        }
    }

    assert!(records >= 35, "mod imag record count fell to {records}");
    assert!(slots >= 370, "mod slot count fell to {slots}");
    assert!(frames >= 3_500, "mod frame count fell to {frames}");
    assert_eq!(
        versions,
        BTreeSet::from([2, 3, 4, 5]),
        "the mods stopped covering every layout"
    );
    assert!(
        tiles >= 3_500 && other <= 10,
        "{tiles} tiles, {other} other"
    );
}

/// Mod tiles resolve their palettes.
///
/// The palette reference is a position into the tile's own archive. Mod tiles
/// were packed by an unidentified tool, so this is non-Cyberlore evidence.
///
/// Snapshot 2026-09-03: 1,946 tiles, 1,912 indexed, 80 with an embedded
/// palette.
#[test]
fn every_mod_tile_resolves_its_palette() {
    let dir = common::workshop();
    let (mut tiles, mut indexed, mut embedded) = (0, 0, 0);

    for path in common::archives(&dir) {
        let owned = vec![(path.display().to_string(), std::fs::read(&path).unwrap())];
        let cat = Catalogue::build(&owned).unwrap();

        let Some(chunk) = cat.archives[0].1.chunk_by_tag(b"TILE") else {
            continue;
        };

        for (index, record) in chunk.records.iter().enumerate() {
            // Padding slot.
            if record.data.is_empty() {
                continue;
            }

            let sprite = Sprite::resolve(&cat, 0, record.data)
                .unwrap_or_else(|e| panic!("{} tile {index}: {e}", path.display()));

            tiles += 1;

            if matches!(sprite.colour, Colour::Indexed { .. }) {
                indexed += 1;
                embedded += usize::from(sprite.source == PaletteSource::Embedded);
            }
        }
    }

    assert!(tiles >= 1_946, "mod tile count fell to {tiles}");
    assert!(indexed >= 1_912, "indexed mod tile count fell to {indexed}");
    assert!(
        embedded >= 80,
        "embedded mod palette count fell to {embedded}"
    );
}

/// Phantoms Haunt, Workshop item 3769947406: the one item with empty
/// records.
const PHANTOMS: &str = "{8c48289e-7c70-4426-8913-133f3544a182}";

/// How many of a mod archive's full tiles equal the stock archive's tile
/// at the same position.
fn stock_copies(modded: &cam::Archive<'_>, stock: &cam::Archive<'_>) -> usize {
    let (Some(mine), Some(theirs)) = (modded.chunk_by_tag(b"TILE"), stock.chunk_by_tag(b"TILE"))
    else {
        return 0;
    };

    mine.records
        .iter()
        .zip(&theirs.records)
        .filter(|(m, s)| !m.data.is_empty() && m.data == s.data)
        .count()
}

/// No archive is sparse (`Catalogue::positional`, `Catalogue::find`).
/// Phantoms Haunt composed over the expansion: each of the 2,608 frames of
/// its 28 `IMAG`s names a full tile of its own archive, so its 19,274 empty
/// `TILE` records are padding none reaches. No id lookup lands on an
/// empty record, and no `IMAG` of the mod's shares a stock id. Its low
/// tiles are stock copies at the stock positions: 403 of
/// `maindata.cam`'s, 113 of `interfacedata.cam`'s.
#[test]
fn phantoms_haunt_names_no_empty_record() {
    use regent_core::content::dataset::{Bytecode, Datasets, Install};
    use regent_core::content::imagery::{self, FrameSource};
    use regent_core::content::mods;
    use regent_core::data::catalogue::Library as Archives;
    use regent_core::data::guid::Guid;

    let library = mods::scan(&[common::workshop()]);
    let phantoms = library
        .find(Guid::parse(PHANTOMS).expect("a GUID"))
        .expect("Phantoms Haunt");
    let hd = common::hd();
    let install = Install::new(&hd, Bytecode::Install);
    let stock = install.files(Datasets::Expansion).expect("stock");
    let modded = install
        .files_with(Datasets::Expansion, &[phantoms])
        .expect("modded");

    // Its seven archives load after the expansion's.
    assert_eq!(modded.archives[..stock.archives.len()], stock.archives[..]);
    assert_eq!(modded.archives.len(), stock.archives.len() + 7);

    let archives = Archives::from_files(&modded.archives).expect("readable");
    let cat = archives.catalogue().expect("parses");
    let first = stock.archives.len();
    let (mut images, mut frames) = (0, 0);

    for (at, record) in cat.tagged(b"IMAG").filter(|(at, _)| at.archive >= first) {
        assert_eq!(cat.all(b"IMAG", record.id).len(), 1, "{} shared", record.id);

        for frame in imagery::sets(&cat, at)
            .expect("decodes")
            .iter()
            .flat_map(|a| &a.poses)
            .flat_map(|p| &p.frames)
        {
            let FrameSource::Tile { archive, index, .. } = *frame else {
                panic!("{}: {frame:?}", record.id);
            };

            assert_eq!(archive, at.archive, "{}", record.id);
            assert!(
                matches!(Sprite::at(&cat, archive, index), Some(Ok(_))),
                "{} tile {index}",
                record.id
            );
            frames += 1;
        }

        images += 1;
    }

    assert_eq!(images, 28);
    assert_eq!(frames, 2_608);

    // Every record an id finds is full.
    for (_, archive) in &cat.archives {
        for chunk in archive
            .chunks
            .iter()
            .filter(|c| c.flag == cam::ChunkFlag::Clear)
        {
            for record in &chunk.records {
                let found = cat.find(&chunk.tag.0, record.id).expect("indexed");

                assert_ne!(cat.record(&chunk.tag.0, found).unwrap().data.len(), 0);
            }
        }
    }

    // The low tiles copy the stock ones in place.
    let by_name = |name: &str| {
        cat.archives
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map_or_else(|| panic!("{name}"), |(_, a)| a)
    };

    assert_eq!(
        stock_copies(by_name("phantom_maindata.cam"), by_name("maindata.cam")),
        403
    );
    assert_eq!(
        stock_copies(
            by_name("phantom_interfacedata.cam"),
            by_name("interfacedata.cam")
        ),
        113
    );
}
