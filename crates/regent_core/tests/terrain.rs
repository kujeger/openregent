//! Terrain descriptors (`terrtype.cam`) against the tileset
//! (`tilesetdata.cam`), both editions.
//!
//! Every named letter has art, every blend joins neighbouring classes.

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use regent_core::content::imagery::FrameSource;
use regent_core::content::terrain::Tileset;
use regent_core::data::catalogue::{Catalogue, Library};
use regent_core::data::desc::{self, Part};
use regent_core::data::fourcc::FourCc;
use regent_core::data::imag::SetKind;
use regent_core::view::{Colour, PaletteSource, Sprite};

/// The two datasets, as the archives that describe their terrain.
///
/// Read separately: a catalogue holding both resolves `GRIT` to whichever
/// loaded last.
fn datasets() -> Vec<(&'static str, Vec<PathBuf>)> {
    let root = common::hd();
    let at = |dir: &str, files: &[&str]| -> Vec<PathBuf> {
        files.iter().map(|f| root.join(dir).join(f)).collect()
    };

    vec![
        ("base", at("Data", &["terrtype.cam", "tilesetdata.cam"])),
        (
            "expansion",
            at("DataMX", &["mx_terrtype.cam", "mx_tilesetdata.cam"]),
        ),
    ]
}

fn library(paths: &[PathBuf]) -> Library {
    for p in paths {
        assert!(Path::new(p).is_file(), "{} is not there", p.display());
    }

    Library::from_files(paths).expect("the terrain archives")
}

/// The `DTRN`, and the `DTPT` records it names, from one dataset.
fn terrain<'a>(catalogue: &Catalogue<'a>) -> desc::Terrain<'a> {
    let (at, record) = catalogue
        .tagged(b"DTRN")
        .next()
        .expect("a terrain descriptor");
    let _ = at;

    let part = Part::parse(record.data).expect("the DTRN tree");

    desc::Terrain::parse(&part).expect("the terrain descriptor")
}

/// The dataset's tileset, through the descriptor that names it.
fn tileset<'a>(catalogue: &Catalogue<'a>) -> Tileset<'a> {
    regent_core::content::terrain::Terrain::find(catalogue)
        .expect("a terrain descriptor")
        .expect("the terrain")
        .tileset
        .expect("the tileset it names")
}

/// The `MapTexture` letter as the cross reference spells it.
fn letter(n: u8) -> char {
    char::from(b'A' + n)
}

/// A frame's tile number, for the references that have one.
const fn tile_index(frame: &FrameSource) -> Option<usize> {
    match frame {
        FrameSource::Tile { index, .. } => Some(*index),
        _ => None,
    }
}

/// Each letter's class name, through the table and the `DTPT` records.
fn class_names(catalogue: &Catalogue<'_>) -> BTreeMap<u8, String> {
    terrain(catalogue)
        .classes()
        .filter_map(|(letter, id)| {
            let at = catalogue.find(b"DTPT", id)?;
            let record = catalogue.record(b"DTPT", at)?;
            let part = Part::parse(record.data).ok()?;
            let class = desc::TerrainPart::parse(&part).ok()?;

            Some((
                letter,
                String::from_utf8_lossy(class.head.name).into_owned(),
            ))
        })
        .collect()
}

/// Every `MapTexture` set of the dataset's tileset, as `(from, to)`.
fn map_textures(catalogue: &Catalogue<'_>) -> Vec<(u8, u8)> {
    catalogue
        .tagged(b"IMAG")
        .flat_map(|(at, record)| {
            let image = regent_core::data::imag::Image::parse(record.data).expect("an IMAG record");
            let _ = at;

            image
                .sets
                .iter()
                .filter_map(|s| match s.kind() {
                    SetKind::MapTexture { from, to } => Some((from, to)),
                    _ => None,
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

/// One class per letter. Capacity bounds the `MapTexture` grid width.
#[test]
fn the_terrain_descriptor_names_one_texture_class_per_letter() {
    // (capacity, filled). 16 matches `MAP_TEXTURE_CLASSES` in `data/imag.rs`,
    // found there from the art.
    let want = BTreeMap::from([("base", (10, 10)), ("expansion", (16, 11))]);

    for (name, paths) in datasets() {
        let lib = library(&paths);
        let cat = lib.catalogue().expect("the catalogue");
        let desc = terrain(&cat);

        let classes: Vec<_> = desc.classes().collect();

        assert_eq!(
            (desc.capacity(), classes.len()),
            want[name],
            "{name} texture table shape"
        );

        // Letters are slot indices: 0..n, no holes.
        let letters: Vec<usize> = classes.iter().map(|(l, _)| usize::from(*l)).collect();
        let expected: Vec<usize> = (0..classes.len()).collect();

        assert_eq!(letters, expected, "{name} letters are contiguous");

        // Ids in table order.
        let ids: Vec<String> = classes.iter().map(|(_, id)| id.to_string()).collect();
        let expected: Vec<String> = (0..classes.len()).map(|i| format!("GR{i:02}")).collect();

        assert_eq!(ids, expected, "{name} table order");
    }
}

/// Record index as letter works on the base game only.
#[test]
fn the_expansion_table_order_is_not_the_archive_record_order() {
    let paths = datasets()
        .into_iter()
        .find(|(n, _)| *n == "expansion")
        .map(|(_, p)| p)
        .expect("the expansion");

    let lib = library(&paths);
    let cat = lib.catalogue().expect("the catalogue");

    let by_record: Vec<String> = cat.tagged(b"DTPT").map(|(_, r)| r.id.to_string()).collect();
    let by_slot: Vec<String> = terrain(&cat)
        .classes()
        .map(|(_, id)| id.to_string())
        .collect();

    assert_eq!(
        by_record[1], "GR10",
        "the expansion's second DTPT record is Snow_child"
    );
    assert_eq!(by_slot[1], "GR01", "but slot 1 is Plains_parent");
    assert_ne!(by_record, by_slot, "the two orders differ");
}

/// All 21 `DTPT` records: 29 zero bytes of `PRIM`.
#[test]
fn a_terrain_class_prim_is_twenty_nine_zero_bytes() {
    let mut seen = 0;

    for (name, paths) in datasets() {
        let lib = library(&paths);
        let cat = lib.catalogue().expect("the catalogue");

        for (_, record) in cat.tagged(b"DTPT") {
            let part = Part::parse(record.data).expect("the DTPT tree");
            let class = desc::TerrainPart::parse(&part).expect("the terrain class");

            assert_eq!(class.prim.len(), 29, "{name} {} PRIM length", record.id);
            assert!(class.is_blank(), "{name} {} PRIM is not blank", record.id);

            seen += 1;
        }
    }

    assert_eq!(seen, 21, "terrain class record count");
}

/// The base game declares `GR09 Snow` with no art; the expansion draws it.
/// `constants.rgs` (not checked here) never references `GR09`.
#[test]
fn the_tileset_and_the_terrain_table_agree_on_the_letters() {
    let want_unpainted = BTreeMap::from([("base", vec!["GR09"]), ("expansion", vec![])]);

    for (name, paths) in datasets() {
        let lib = library(&paths);
        let cat = lib.catalogue().expect("the catalogue");
        let desc = terrain(&cat);

        let bodies: BTreeSet<u8> = map_textures(&cat)
            .iter()
            .filter(|(from, to)| from == to)
            .map(|(from, _)| *from)
            .collect();

        let unpainted: Vec<String> = desc
            .classes()
            .filter(|(letter, _)| !bodies.contains(letter))
            .map(|(_, id)| id.to_string())
            .collect();

        assert_eq!(
            unpainted, want_unpainted[name],
            "{name} classes without art"
        );

        // Nothing is drawn that the table does not name.
        let named: BTreeSet<u8> = desc.classes().map(|(l, _)| l).collect();
        let unnamed: Vec<u8> = bodies.difference(&named).copied().collect();

        assert!(
            unnamed.is_empty(),
            "{name} draws unnamed letters {unnamed:?}"
        );
    }
}

/// One `DTRN` and one `IMAG` per dataset, same id.
#[test]
fn the_tileset_descriptor_shares_its_id_with_its_art() {
    for (name, paths) in datasets() {
        let lib = library(&paths);
        let cat = lib.catalogue().expect("the catalogue");

        let descriptors: Vec<FourCc> = cat.tagged(b"DTRN").map(|(_, r)| r.id).collect();
        let art: Vec<FourCc> = cat.tagged(b"IMAG").map(|(_, r)| r.id).collect();

        assert_eq!(descriptors.len(), 1, "{name} DTRN count");
        assert_eq!(art.len(), 1, "{name} IMAG count");
        assert_eq!(descriptors[0], art[0], "{name} descriptor and art id");
    }
}

/// The blends form a tree rooted at the first class, and the names say so.
///
/// Every blend joins `Dirt` to a `*_parent`, or a `*_parent` to its
/// `*_child`, by parsed `HEAD` names.
#[test]
fn every_blend_joins_a_terrain_class_to_its_tree_neighbour() {
    let want = BTreeMap::from([("base", 8), ("expansion", 10)]);

    for (name, paths) in datasets() {
        let lib = library(&paths);
        let cat = lib.catalogue().expect("the catalogue");
        let desc = terrain(&cat);

        let names: BTreeMap<u8, String> = desc
            .classes()
            .filter_map(|(letter, id)| {
                let at = cat.find(b"DTPT", id)?;
                let record = cat.record(b"DTPT", at)?;
                let part = Part::parse(record.data).ok()?;
                let class = desc::TerrainPart::parse(&part).ok()?;

                Some((
                    letter,
                    String::from_utf8_lossy(class.head.name).into_owned(),
                ))
            })
            .collect();

        let blends: Vec<(u8, u8)> = map_textures(&cat)
            .into_iter()
            .filter(|(from, to)| from != to)
            .collect();

        assert_eq!(blends.len(), want[name], "{name} blend count");

        for (from, to) in blends {
            let a = names.get(&from).map_or("?", String::as_str);
            let b = names.get(&to).map_or("?", String::as_str);

            let root_to_parent = a == "Dirt" && b.ends_with("_parent");
            let parent_to_child = a
                .strip_suffix("_parent")
                .is_some_and(|stem| b.strip_suffix("_child") == Some(stem));

            assert!(
                root_to_parent || parent_to_child,
                "{name} blend {a} to {b} is not a tree edge"
            );
        }
    }
}

/// 17 slots on all 18 sets. Variant counts vary by authoring: the base eight
/// are `[1,3,3,4,4,4,4,3,3,4,…]`, the expansion's two `[1,4,4,…]`.
///
/// Slot meanings: `docs/formats/terrain.md`.
#[test]
fn every_blend_set_has_seventeen_cases() {
    let mut sets = 0;

    for (name, paths) in datasets() {
        let lib = library(&paths);
        let cat = lib.catalogue().expect("the catalogue");
        let tileset = tileset(&cat);

        for blend in &tileset.blends {
            assert_eq!(
                blend.cases.len(),
                17,
                "{name} {}to{} case count",
                letter(blend.from),
                letter(blend.to)
            );

            sets += 1;
        }
    }

    assert_eq!(sets, 18, "blend set count across both editions");
}

/// Stream 0 holds the ground variants. Streams 1 to 3 are `tile 0` with flag
/// `0x4000`, except at group columns, where all four hold distinct tiles.
///
/// `A` has a group at column 16, inside its variants, reusing their tiles;
/// other groups trail. **Undecoded**: what a group is.
#[test]
fn a_body_set_is_stream_zero_plus_undecoded_groups() {
    // letter -> (frames, variants, group columns)
    let want = BTreeMap::from([
        ('A', (34, 31, vec![16, 32, 33])),
        ('B', (26, 24, vec![24, 25])),
        ('J', (32, 24, (24..32).collect())),
        ('K', (32, 24, (24..32).collect())),
    ]);

    for (name, paths) in datasets() {
        let lib = library(&paths);
        let cat = lib.catalogue().expect("the catalogue");

        for body in &tileset(&cat).bodies {
            assert_eq!(
                body.streams,
                4,
                "{name} {} stream count",
                letter(body.letter)
            );

            let Some((frames, variants, groups)) = want.get(&letter(body.letter)) else {
                continue;
            };

            let at: Vec<usize> = body.groups.iter().map(|(i, _)| *i).collect();

            assert_eq!(
                (body.frames, body.variants.len(), at),
                (*frames, *variants, groups.clone()),
                "{name} body {}",
                letter(body.letter)
            );

            // A group spans every stream.
            for (column, tiles) in &body.groups {
                assert_eq!(
                    tiles.len(),
                    usize::from(body.streams),
                    "{name} {} group at {column}",
                    letter(body.letter)
                );
            }
        }
    }
}

/// Where a blend's first case comes from, and where it does not.
///
/// On the base eight sets, slot 0 (destination covers the whole tile) is the
/// same tile record as the destination body's last variant.
///
/// Authoring, not format: the expansion's `AtoJ` slot 0 is tile 819, `JtoJ`'s
/// last variant 831.
#[test]
fn a_base_blend_slot_zero_is_its_destinations_last_body_tile() {
    let sharing = |paths: &[PathBuf]| -> Vec<(String, bool)> {
        let lib = library(paths);
        let cat = lib.catalogue().expect("the catalogue");
        let tileset = tileset(&cat);

        tileset
            .blends
            .iter()
            .map(|blend| {
                let first = blend.cases[0].variants.first().and_then(tile_index);
                let last = tileset
                    .body(blend.to)
                    .and_then(|b| b.variants.last())
                    .and_then(tile_index);

                (
                    format!("{}to{}", letter(blend.from), letter(blend.to)),
                    first.is_some() && first == last,
                )
            })
            .collect()
    };

    for (name, paths) in datasets() {
        let shared = sharing(&paths);

        // The base game's eight, in either edition.
        for (set, same) in shared.iter().take(8) {
            assert!(
                same,
                "{name} {set} slot 0 should be its destination's last tile"
            );
        }

        for (set, same) in shared.iter().skip(8) {
            assert!(!same, "{name} {set} was expected not to share a tile");
        }
    }
}

/// Blend art sits in a four-wide atlas; a three-variant case leaves a cell
/// empty. Four per set, eight sets: the base tileset's 32 unreferenced tiles.
///
/// **Unexplained**: the expansion's extra 896 to 899.
#[test]
fn the_unreferenced_tiles_are_the_atlas_gaps() {
    let want = BTreeMap::from([("base", (808, 32)), ("expansion", (1052, 36))]);

    for (name, paths) in datasets() {
        let lib = library(&paths);
        let cat = lib.catalogue().expect("the catalogue");
        let tileset = tileset(&cat);

        let total = cat.chunk_len(tileset.archive, b"TILE");
        let used: BTreeSet<usize> = tileset.frames().filter_map(|f| tile_index(&f)).collect();
        let unused: Vec<usize> = (0..total).filter(|i| !used.contains(i)).collect();

        assert_eq!((total, unused.len()), want[name], "{name} tile accounting");

        // The first set's gaps; the rest repeat the pattern.
        assert_eq!(unused[..4], [59, 63, 83, 87], "{name} AtoB's gaps");

        if name == "expansion" {
            assert_eq!(
                unused[32..],
                [896, 897, 898, 899],
                "the expansion's unexplained run"
            );
        }
    }
}

/// Every terrain tile: 32x32 direct colour, version 1, fully covered. A plain
/// blit for the compositor.
#[test]
fn every_terrain_tile_is_a_uniform_opaque_square() {
    let mut seen = 0;

    for (name, paths) in datasets() {
        let lib = library(&paths);
        let cat = lib.catalogue().expect("the catalogue");
        let tileset = tileset(&cat);

        for index in tileset.frames().filter_map(|f| tile_index(&f)) {
            let sprite = Sprite::at(&cat, tileset.archive, index)
                .unwrap_or_else(|| panic!("{name} tile {index} is missing"))
                .unwrap_or_else(|e| panic!("{name} tile {index}: {e}"));

            assert_eq!(
                (sprite.width, sprite.height),
                (32, 32),
                "{name} tile {index}"
            );
            assert_eq!(sprite.version, 1, "{name} tile {index} version");
            assert!(
                matches!(sprite.colour, Colour::Direct { .. }),
                "{name} tile {index} is not direct colour"
            );
            assert_eq!(sprite.source, PaletteSource::None, "{name} tile {index}");
            assert!(
                sprite.covered.iter().all(|c| *c),
                "{name} tile {index} has a gap"
            );

            seen += 1;
        }
    }

    assert_eq!(seen, 787 + 1029, "terrain tile references");
}

/// Eleven classes: five chains `Dirt -> parent -> child`. The generator puts
/// Dirt between biomes (**Game**), and a child sits only inside its parent, so
/// no biome-to-biome or Dirt-to-child blend is needed. `constants.rgs` agrees
/// (see `chart.rs`).
#[test]
fn the_terrain_classes_are_ladders_of_three_sharing_dirt() {
    // Biomes with both blends drawn.
    let want = BTreeMap::from([
        ("base", vec!["Arid", "Plains", "Scorch", "Swamp"]),
        (
            "expansion",
            vec!["Arid", "Plains", "Scorch", "Snow", "Swamp"],
        ),
    ]);

    for (name, paths) in datasets() {
        let lib = library(&paths);
        let cat = lib.catalogue().expect("the catalogue");
        let names = class_names(&cat);

        let stem = |letter: u8| -> Option<(String, bool)> {
            let full = names.get(&letter)?;

            full.strip_suffix("_parent")
                .map(|s| (s.to_owned(), false))
                .or_else(|| full.strip_suffix("_child").map(|s| (s.to_owned(), true)))
        };

        let root = names
            .iter()
            .find(|(_, n)| *n == "Dirt")
            .map(|(l, _)| *l)
            .expect("a Dirt class");

        let mut rungs: BTreeMap<String, BTreeSet<&'static str>> = BTreeMap::new();

        for (from, to) in map_textures(&cat).into_iter().filter(|(f, t)| f != t) {
            match (from == root, stem(from), stem(to)) {
                // Dirt to parent: lower rung.
                (true, _, Some((biome, false))) => {
                    rungs.entry(biome).or_default().insert("lower");
                }
                // Parent to own child: upper rung.
                (false, Some((a, false)), Some((b, true))) if a == b => {
                    rungs.entry(a).or_default().insert("upper");
                }
                _ => panic!(
                    "{name}: blend {} to {} is neither rung of a ladder",
                    names[&from], names[&to]
                ),
            }
        }

        let complete: Vec<&str> = rungs
            .iter()
            .filter(|(_, r)| r.len() == 2)
            .map(|(b, _)| b.as_str())
            .collect();

        assert_eq!(complete, want[name], "{name} complete ladders");

        // Nothing joins Dirt to a child.
        for (from, to) in map_textures(&cat).into_iter().filter(|(f, t)| f != t) {
            assert!(
                !(from == root && matches!(stem(to), Some((_, true)))),
                "{name}: Dirt should never abut a child, but {} does",
                names[&to]
            );
        }
    }
}
