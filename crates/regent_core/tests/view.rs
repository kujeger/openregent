//! `regent_core::view` joins against the real installs: dataset definitions parse
//! and their records resolve.
//!
//! Needs `MAJESTY_HD` (environment or `.env`).

mod common;

use std::collections::BTreeMap;

use regent_core::content::dataset;
use regent_core::content::imagery::FrameSource;
use regent_core::content::unit::{Link, Unit};
use regent_core::data::catalogue::{Library, Located};
use regent_core::data::desc::{self, UnitKind};
use regent_core::data::fourcc::FourCc;
use regent_core::data::imag::Blit;
use regent_core::data::imag::{Pass, SetKind};
use regent_core::view::{Colour, Ink, PaletteSource, Shade, Sprite};

/// HD's two dataset definitions.
#[test]
fn both_dataset_definitions_parse() {
    let hd = common::hd();

    let base_src = std::fs::read_to_string(hd.join("Data/MajestyDatasetDefinitions.xml"))
        .expect("the base game's definition");
    let mx_src = std::fs::read_to_string(hd.join("DataMX/MajestyExpansionDatasetDefinitions.xml"))
        .expect("the expansion's definition");

    let base = dataset::parse(&base_src);
    let mx = dataset::parse(&mx_src);

    assert_eq!(base.name, "Majesty");
    assert_eq!(mx.name, "MajestyExpansion");

    // The expansion loads after the base.
    assert_eq!(base.base, None);
    assert_eq!(mx.base, Some("Majesty"), "the expansion extends the base");

    assert_eq!(base.load.archives.len(), 17, "base archives changed");
    assert_eq!(mx.load.archives.len(), 17, "expansion archives changed");

    assert!(base.unknown.is_empty(), "{:?}", base.unknown);
    assert!(mx.unknown.is_empty(), "{:?}", mx.unknown);

    // Composition also unloads.
    assert!(base.unload.is_empty());
    assert_eq!(mx.unload.constants.len(), 1);
    assert_eq!(mx.unload.constants[0].path, "constants.rgs");
}

/// Archives outside both datasets: the two downloadable content sets.
#[test]
fn the_datasets_account_for_the_shipped_archives() {
    let hd = common::hd();
    let game = common::hd();

    let mut named: Vec<String> = Vec::new();

    for definition in [
        "Data/MajestyDatasetDefinitions.xml",
        "DataMX/MajestyExpansionDatasetDefinitions.xml",
    ] {
        let src = std::fs::read_to_string(hd.join(definition)).expect("a definition");

        for a in &dataset::parse(&src).load.archives {
            named.push(a.path.to_lowercase());
        }
    }

    let mut missing: Vec<String> = Vec::new();

    for dir in ["Data", "DataMX"] {
        for path in common::archives(&game.join(dir)) {
            let name = path.file_name().unwrap().to_string_lossy().to_lowercase();

            if !named.contains(&name) {
                missing.push(name);
            }
        }
    }

    missing.sort();
    missing.dedup();

    // `mdl1_*` is the Krolm content, `xqd1_*` the extra quests; both load via
    // `datasets.xml`.
    assert_eq!(
        missing,
        [
            "mdl1_action.cam",
            "mdl1_maindata.cam",
            "mdl1_sounddesc.cam",
            "mdl1_soundfx.cam",
            "mdl1_unittype.cam",
            "mdl1_voices.cam",
            "xqd1_maindata.cam",
            "xqd1_soundfx.cam",
            "xqd1_unittype.cam",
            "xqd1_voices.cam",
        ],
        "archives outside both datasets changed"
    );
}

/// A dataset loaded by HD's definition, once a process ([`common::defined`]).
fn load(definition: &str, dir: &str) -> &'static Library {
    common::defined(definition, dir)
}

#[test]
fn a_dataset_loads_and_indexes_every_record() {
    let library = load("Data/MajestyDatasetDefinitions.xml", "Data");

    assert_eq!(library.missing, Vec::<&str>::new(), "archives missing");
    assert_eq!(library.empty, Vec::<&str>::new(), "empty archives changed");
    assert_eq!(library.len(), 17, "archives loaded changed");

    let cat = library.catalogue().expect("every archive parses");

    assert_eq!(cat.records(), 24_040, "records in the base dataset changed");
}

/// A unit's `ImageIDBase` names an existing `IMAG` for 385 of 394 base units.
/// Complements `descriptor_fields_match_the_sdk_xml`.
#[test]
fn a_units_image_id_finds_its_animation() {
    let library = load("Data/MajestyDatasetDefinitions.xml", "Data");
    let cat = library.catalogue().expect("every archive parses");

    let mut units = 0;
    let mut resolved = 0;
    let mut same_archive = 0;
    let mut nameless = Vec::new();

    for (archive, (_, parsed)) in cat.archives.iter().enumerate() {
        for (chunk, record) in parsed.records() {
            if chunk.tag != b"DUNT" {
                continue;
            }

            units += 1;

            let part = desc::Part::parse(record.data).expect("a descriptor");
            let unit = desc::Unit::parse(&part).expect("a unit");

            let Some(image) = unit.image_id() else {
                nameless.push(record.id.to_string());
                continue;
            };

            match cat.find(b"IMAG", image) {
                Some(at) => {
                    resolved += 1;
                    same_archive += usize::from(at.archive == archive);
                }
                None => nameless.push(record.id.to_string()),
            }
        }
    }

    assert_eq!(units, 394, "units in the base dataset changed");
    assert_eq!(resolved, 385, "units whose art resolves changed");

    // Art is never in the unit's archive: the index must span the dataset.
    assert_eq!(same_archive, 0, "a unit found its art in its own archive");

    // Nine units name art absent from the dataset.
    nameless.sort();
    assert_eq!(
        nameless,
        [
            "ARA1", "ARA3", "ASZ1", "AVm1", "BVB1", "BVC1", "BVW1", "BVf1", "NRd2"
        ],
        "units with no art changed"
    );
}

/// `Catalogue::find` takes the last duplicate. Guess: load order is a sequence;
/// not observed in the engine.
#[test]
fn repeated_ids_are_recorded_not_hidden() {
    let base = load("Data/MajestyDatasetDefinitions.xml", "Data");
    let base = base.catalogue().expect("every archive parses");

    assert_eq!(
        base.ambiguous().count(),
        12,
        "repeated ids in the base game"
    );

    let mx = load("DataMX/MajestyExpansionDatasetDefinitions.xml", "DataMX");
    let mx = mx.catalogue().expect("every archive parses");

    assert_eq!(
        mx.ambiguous().count(),
        0,
        "the expansion alone is unambiguous"
    );

    // Collisions are only between datasets (`base="Majesty"`), e.g. the
    // expansion reships the Adept.
    assert!(base.find(b"DUNT", FourCc::new(b"AVA1")).is_some());
    assert!(mx.find(b"DUNT", FourCc::new(b"AVA1")).is_some());
}

/// Every tile resolves its palette by position in its own archive.
///
/// `Dataset::library` does not follow `base="Majesty"`, so the datasets do not
/// overlap. Direct-colour counts sum to 2,274, every such tile in the install.
///
/// A dangling palette is an error, so resolving is the assertion.
#[test]
fn every_tile_in_a_dataset_resolves_to_pixels() {
    let cases = [
        (
            "Data/MajestyDatasetDefinitions.xml",
            "Data",
            [20_738, 19_598, 1_140, 2_401],
        ),
        (
            "DataMX/MajestyExpansionDatasetDefinitions.xml",
            "DataMX",
            [10_873, 9_733, 1_140, 702],
        ),
    ];

    for (definition, dir, expected) in cases {
        let library = load(definition, dir);
        let cat = library.catalogue().expect("every archive parses");

        let (mut resolved, mut indexed, mut direct, mut embedded) = (0, 0, 0, 0);

        for (at, (name, archive)) in cat.archives.iter().enumerate() {
            let Some(chunk) = archive.chunk_by_tag(b"TILE") else {
                continue;
            };

            for (index, record) in chunk.records.iter().enumerate() {
                // Empty record: padding that keeps positional references valid.
                if record.data.is_empty() {
                    continue;
                }

                let sprite = Sprite::resolve(&cat, at, record.data)
                    .unwrap_or_else(|e| panic!("{dir} {name} tile {index}: {e}"));

                assert_eq!(
                    sprite.covered.len(),
                    usize::from(sprite.width) * usize::from(sprite.height),
                    "{dir} {name} tile {index}: coverage does not match the canvas"
                );

                assert_eq!(sprite.rgba().len(), sprite.covered.len() * 4);

                match &sprite.colour {
                    Colour::Indexed { indices, .. } => {
                        indexed += 1;
                        assert_eq!(indices.len(), sprite.covered.len());
                        embedded += usize::from(sprite.source == PaletteSource::Embedded);
                    }
                    Colour::Direct { words, .. } => {
                        direct += 1;
                        assert_eq!(words.len(), sprite.covered.len());
                    }
                }

                resolved += 1;
            }
        }

        assert_eq!(
            [resolved, indexed, direct, embedded],
            expected,
            "{dir}: [resolved, indexed, direct, embedded] changed"
        );
    }
}

/// Nothing dangles in the base dataset: every frame names a tile, and every
/// movement class, default sound and action override resolves.
#[test]
fn every_unit_resolves_what_it_names() {
    let library = load("Data/MajestyDatasetDefinitions.xml", "Data");
    let cat = library.catalogue().expect("every archive parses");

    let (mut units, mut with_art, mut animations, mut poses) = (0, 0, 0, 0);
    let (mut tiles, mut missing, mut sentinel, mut unknown) = (0, 0, 0, 0);
    let (mut movement, mut sound, mut overrides) = (0, 0, 0);
    let mut dangling: Vec<String> = Vec::new();

    for (at, (_, archive)) in cat.archives.iter().enumerate() {
        let Some(chunk) = archive.chunk_by_tag(b"DUNT") else {
            continue;
        };

        for record in 0..chunk.records.len() {
            let u = Unit::at(
                &cat,
                Located {
                    archive: at,
                    record,
                },
            )
            .expect("a unit");

            units += 1;
            with_art += usize::from(u.art.is_some_and(|l| l.resolved()));
            animations += u.animations.len();
            poses += u.animations.iter().map(|a| a.poses.len()).sum::<usize>();

            // A tile number counts within the IMAG's archive, never the unit's.
            let art_at = u.art.and_then(|l| l.at).map(|l| l.archive);

            for f in u.frames() {
                match f {
                    FrameSource::Tile { archive, .. } => {
                        tiles += 1;
                        assert_eq!(
                            Some(archive),
                            art_at,
                            "{}: a frame resolved against the wrong archive",
                            u.id
                        );
                        assert_ne!(
                            archive, at,
                            "{}: a frame resolved against the unit's own archive",
                            u.id
                        );
                    }
                    FrameSource::Missing { archive, index } => {
                        missing += 1;
                        dangling.push(format!("{} tile {index} of archive {archive}", u.id));
                    }
                    FrameSource::Sentinel(_) => sentinel += 1,
                    FrameSource::Unknown(v) => {
                        unknown += 1;
                        dangling.push(format!("{} frame reference {v:#x}", u.id));
                    }
                }
            }

            let mut check = |kind: &str, link: &Link| {
                let ok = link.resolved();
                if !ok {
                    dangling.push(format!("{} {kind} {}", u.id, link.id));
                }
                ok
            };

            movement += usize::from(u.movement.as_ref().is_some_and(|l| check("DMOV", l)));
            sound += usize::from(u.sound.as_ref().is_some_and(|l| check("DSND", l)));

            for (action, snd) in &u.overrides {
                overrides += usize::from(check("DACT", action) & check("DSND", snd));
            }
        }
    }

    assert!(
        dangling.is_empty(),
        "{} references do not resolve: {:?}",
        dangling.len(),
        &dangling[..dangling.len().min(8)]
    );

    assert_eq!([units, with_art], [394, 385], "[units, with art] changed");
    assert_eq!(
        [animations, poses],
        [3_428, 6_540],
        "[animation sets, poses] changed"
    );
    assert_eq!(
        [tiles, missing, sentinel, unknown],
        [59_937, 0, 0, 0],
        "[tile, missing, sentinel, unknown] frame references changed"
    );
    assert_eq!(
        [movement, sound, overrides],
        [93, 195, 5],
        "[DMOV, DSND, override] links changed"
    );
}

/// The catalogue's kind lookup reads what the resolved unit reads, on
/// every descriptor, without resolving art.
#[test]
fn a_units_kind_is_read_off_its_descriptor_alone() {
    let library = load("Data/MajestyDatasetDefinitions.xml", "Data");
    let cat = library.catalogue().expect("every archive parses");

    for (_, record) in cat.tagged(b"DUNT") {
        let unit = Unit::find(&cat, record.id)
            .expect("a unit")
            .expect("it parses");

        assert_eq!(cat.unit_kind(record.id), unit.kind, "{}", record.id);
    }

    assert_eq!(
        cat.unit_kind(FourCc::new(b"AVB1")),
        Some(UnitKind::Character)
    );
    assert_eq!(cat.unit_kind(FourCc::new(b"!!!!")), None);
}

/// A record may name a set its unit lacks (the Healer's `Walk-2`): it
/// draws the first set of that family. A set it has draws as named.
#[test]
fn a_missing_set_falls_back_to_the_first_of_its_family() {
    let library = load("Data/MajestyDatasetDefinitions.xml", "Data");
    let cat = library.catalogue().expect("every archive parses");
    let healer = Unit::find(&cat, FourCc::new(b"AVD1"))
        .expect("a unit")
        .expect("it parses");
    let first_walk = healer.layers(SetKind::Walk).next().expect("a Walk set");
    let walk_2 = 2;

    assert!(
        healer.stack(walk_2).next().is_none(),
        "the Healer has Walk-2"
    );
    assert_eq!(
        healer.drawn(walk_2),
        Some(regent_core::data::imag::set_number(first_walk.id))
    );

    let stand = healer.animation(SetKind::Stand).expect("a Stand set");
    let stand = regent_core::data::imag::set_number(stand.id);

    assert_eq!(healer.drawn(stand), Some(stand));
}

/// One unit, all the way down to pixels.
#[test]
fn the_documented_animations_survive_being_fetched() {
    let library = load("Data/MajestyDatasetDefinitions.xml", "Data");
    let cat = library.catalogue().expect("every archive parses");

    let unit = |id: &[u8; 4]| {
        Unit::find(&cat, FourCc::new(id))
            .expect("a unit with this id")
            .expect("it parses")
    };

    // The three shapes in docs/formats/README.md.
    let barbarian = unit(b"AVB1");
    let healer = unit(b"AVD1");
    let ranger = unit(b"AVH1");

    let name = |u: &Unit<'_>| String::from_utf8_lossy(u.desc.head.name).into_owned();
    assert_eq!(name(&barbarian), "Barbarian");
    assert_eq!(name(&healer), "Healer");
    assert_eq!(name(&ranger), "Ranger");

    let walk = barbarian.animation(SetKind::Walk).expect("a Walk set");
    assert_eq!(walk.poses.len(), 8);
    assert!(
        walk.poses
            .iter()
            .all(|p| p.per_stream == 8 && p.streams == 1),
        "Barbarian Walk is eight facings of eight frames"
    );

    // Two streams: frame table is twice the count word.
    let spell = healer.animation(SetKind::Cast).expect("a Cast set");
    assert_eq!(spell.poses.len(), 8);
    assert!(
        spell
            .poses
            .iter()
            .all(|p| p.per_stream == 9 && p.streams == 2 && p.frames.len() == 18),
        "Healer Cast is eight of nine frames in two streams"
    );

    // The only asymmetric Die in the archive.
    let die = ranger.animation(SetKind::Die).expect("a Die set");
    let counts: Vec<u16> = die.poses.iter().map(|p| p.per_stream).collect();
    assert_eq!(counts, [16, 16, 16, 16, 15, 16, 16, 16]);

    assert_eq!(barbarian.id.to_string(), "AVB1");
    assert_eq!(barbarian.kind, Some(UnitKind::Character));

    let art = barbarian.art.expect("an image id");
    let at = art.at.expect("art that resolved");
    assert_ne!(
        at.archive, barbarian.archive,
        "a unit never holds its own art"
    );

    let stats = barbarian.desc.character().expect("a character stat block");
    let vitals = [stats.max_hp(), stats.cost(), stats.speed()];
    assert!(
        vitals.iter().all(Option::is_some),
        "the Barbarian has stats: {vitals:?}"
    );

    assert!(barbarian.movement.is_some_and(|l| l.resolved()));
    assert!(barbarian.sound.is_some_and(|l| l.resolved()));

    // Rasterise every frame to find padded slots.
    let (mut drawn, mut pads, mut pixels) = (0, 0, 0);

    for frame in barbarian.frames() {
        let FrameSource::Tile { archive, index, .. } = frame else {
            panic!("the Barbarian has a frame that is not a tile: {frame:?}");
        };

        match Sprite::at(&cat, archive, index) {
            Some(sprite) => {
                let s = sprite.expect("a tile that decodes");
                assert_eq!(
                    s.covered.len(),
                    usize::from(s.width) * usize::from(s.height)
                );
                assert!(s.drawn() > 0, "an empty frame at tile {index}");
                pixels += s.drawn();
                drawn += 1;
            }
            None => pads += 1,
        }
    }

    assert_eq!(
        [barbarian.animations.len(), drawn, pads, pixels],
        [12, 342, 0, 465_289],
        "[sets, frames drawn, padded slots, covered pixels] changed"
    );
}

/// Every frame reference is art, not a zero-length padding record.
///
/// 26 frames over five tiles draw nothing. An all-terminator tile is legal, so
/// they are content.
#[test]
fn every_unit_frame_composes_to_pixels() {
    let library = load("Data/MajestyDatasetDefinitions.xml", "Data");
    let cat = library.catalogue().expect("every archive parses");

    let (mut drawn, mut pads, mut blank) = (0, 0, 0);
    let mut empty = std::collections::BTreeSet::new();

    for (at, _) in cat.tagged(b"DUNT") {
        let unit = Unit::at(&cat, at).expect("a unit");

        for frame in unit.frames() {
            let FrameSource::Tile { archive, index, .. } = frame else {
                continue;
            };

            match Sprite::at(&cat, archive, index) {
                Some(sprite) => {
                    let s = sprite.unwrap_or_else(|e| {
                        panic!("{} tile {index} of archive {archive}: {e}", unit.id)
                    });

                    if s.drawn() == 0 {
                        blank += 1;
                        empty.insert((archive, index, s.width, s.height));
                    }
                    drawn += 1;
                }
                None => pads += 1,
            }
        }
    }

    assert_eq!(
        [drawn, pads, blank],
        [59_937, 0, 26],
        "[frames rasterised, padded slots, frames drawing nothing] changed"
    );

    // All but two frames are buildings'. 757 and 758 are shared by a dozen
    // buildings: an empty overlay.
    let named: Vec<(usize, usize, u16, u16)> = empty.into_iter().collect();
    assert_eq!(
        named,
        [
            (7, 188, 3, 1),
            (7, 757, 160, 140),
            (7, 758, 161, 108),
            (7, 1261, 1, 1),
            (7, 15_873, 132, 124),
        ],
        "the set of tiles that draw nothing changed"
    );
}

/// **Corpus:** most offset poses are `Die`. A character has one `Die` set of
/// eight poses; a building has many, nearly all single-pose.
///
/// **Game:** a building's `Die` sets are the fires on a damaged building; the
/// offset places each fire.
///
/// So ids 0x60..0x67 mean death on a character and damage overlays on a
/// building (the per-subtype `Flags` shape in `docs/formats/README.md`).
///
/// **Inferred:** `Pose::x`/`y` offsets the piece from the unit's origin. Sign
/// confirmed only by the fires landing right.
///
/// The eight ids are a damage ladder: per-id building set counts 149, 218,
/// 221, 289, 268, 335, 160, 192 (not asserted here). See
/// `a_pose_offset_places_a_stacked_layer` and
/// **Measured**.
#[test]
fn a_pose_offset_places_a_piece_and_die_means_two_things() {
    let library = load("Data/MajestyDatasetDefinitions.xml", "Data");
    let cat = library.catalogue().expect("every archive parses");

    let (mut poses, mut shifted) = (0, 0);
    let (mut min_x, mut max_x, mut min_y, mut max_y) = (0i16, 0i16, 0i16, 0i16);
    let mut kinds: BTreeMap<String, usize> = BTreeMap::new();
    let (mut building_die, mut multi_pose_die) = (0, 0);
    let mut character_die: BTreeMap<usize, usize> = BTreeMap::new();

    for (at, _) in cat.tagged(b"DUNT") {
        let unit = Unit::at(&cat, at).expect("a unit");

        for set in &unit.animations {
            for pose in &set.poses {
                poses += 1;

                if (pose.x, pose.y) != (0, 0) {
                    shifted += 1;
                    *kinds.entry(set.kind.to_string()).or_default() += 1;
                    min_x = min_x.min(pose.x);
                    max_x = max_x.max(pose.x);
                    min_y = min_y.min(pose.y);
                    max_y = max_y.max(pose.y);
                }
            }
        }

        let die = unit.animations.iter().filter(|a| a.kind == SetKind::Die);

        match unit.kind {
            Some(UnitKind::Building) => {
                for set in die {
                    building_die += 1;
                    multi_pose_die += usize::from(set.poses.len() != 1);
                }
            }
            Some(UnitKind::Character) => {
                let count: usize = die.map(|a| a.poses.len()).sum();
                if count > 0 {
                    *character_die.entry(count).or_default() += 1;
                }
            }
            _ => {}
        }
    }

    assert_eq!([poses, shifted], [6_540, 1_472], "[poses, shifted] changed");
    assert_eq!(
        [min_x, max_x, min_y, max_y],
        [-132, 161, -119, 184],
        "the range a pose offset covers changed"
    );

    let counts: Vec<(&str, usize)> = kinds.iter().map(|(k, n)| (k.as_str(), *n)).collect();
    assert_eq!(
        counts,
        [
            ("Active", 4),
            ("Dead", 2),
            ("Die", 1_451),
            ("Inactive", 6),
            ("Special", 1),
            ("Walk", 8),
        ],
        "which set kinds carry an offset changed"
    );

    assert_eq!(
        [building_die, multi_pose_die],
        [1_832, 89],
        "[building Die sets, those with more than one pose] changed"
    );

    // Character: eight facings, bar five with one pose.
    let shapes: Vec<(usize, usize)> = character_die.into_iter().collect();
    assert_eq!(shapes, [(1, 5), (8, 54)], "character Die shapes changed");

    // Control: a hero is one centred piece.
    let barbarian = Unit::find(&cat, FourCc::new(b"AVB1"))
        .expect("the Barbarian")
        .expect("it parses");

    assert!(
        barbarian
            .animations
            .iter()
            .flat_map(|a| a.poses.iter())
            .all(|p| (p.x, p.y) == (0, 0)),
        "a Barbarian pose moved off the origin"
    );
}

/// Twenty buildings have multi-pose sets, each using a single pose count.
///
/// The count is not simply the number of visual variants:
/// - Not tied to one kind: `BBJ1 Graveyard` has three variants and no `Die`
///   set.
/// - Not always variants: `ABB1 Ballista_Tower`'s 8 are `Attack` directions.
///
/// As `docs/formats/README.md` says, a slot is not always a direction.
#[test]
fn a_building_uses_one_extra_pose_count_or_none() {
    let library = load("Data/MajestyDatasetDefinitions.xml", "Data");
    let cat = library.catalogue().expect("every archive parses");

    let (mut one, mut mixed) = (0, 0);
    let mut counts: BTreeMap<usize, usize> = BTreeMap::new();
    let mut spread: BTreeMap<String, usize> = BTreeMap::new();

    for (at, _) in cat.tagged(b"DUNT") {
        let unit = Unit::at(&cat, at).expect("a unit");

        if unit.kind != Some(UnitKind::Building) {
            continue;
        }

        let mut distinct: Vec<usize> = unit
            .animations
            .iter()
            .map(|a| a.poses.len())
            .filter(|n| *n > 1)
            .collect();
        distinct.sort_unstable();
        distinct.dedup();

        for set in unit.animations.iter().filter(|a| a.poses.len() > 1) {
            *spread.entry(set.kind.to_string()).or_default() += 1;
        }

        match distinct.len() {
            0 => {}
            1 => {
                one += 1;
                *counts.entry(distinct[0]).or_default() += 1;
            }
            _ => mixed += 1,
        }
    }

    assert_eq!(
        [one, mixed],
        [20, 0],
        "[buildings using one extra-pose count, buildings using two] changed"
    );
    assert_eq!(
        counts.into_iter().collect::<Vec<_>>(),
        [(2, 3), (3, 6), (4, 7), (6, 2), (8, 2)],
        "the extra-pose counts buildings use changed"
    );

    let kinds: Vec<(&str, usize)> = spread.iter().map(|(k, n)| (k.as_str(), *n)).collect();
    assert_eq!(
        kinds,
        [
            ("Active", 7),
            ("Attack", 4),
            ("Build", 4),
            ("Crumble", 4),
            ("Dead", 8),
            ("Die", 89),
            ("Hotspot", 5),
            ("Inactive", 17),
        ],
        "which set kinds carry a building's extra poses changed"
    );
}

/// A pose offset places a stacked layer relative to layer 0, not the object.
/// Applying it to object placement puts scatter slightly wrong.
///
/// No `Die` layer 0 has an offset. Above it, offsets place a building's fires;
/// the Rogues' Guild pipe smoke (`Active` layer 1) confirmed by eye.
///
/// # Layer 0
///
/// 16 layer-0 slots carry an offset, none ground scatter; the viewer ignores
/// them. Eight `Walk` (`BPA5`, unverified), six `Interface`, two `Dead`;
/// unexplained.
#[test]
fn a_pose_offset_places_a_stacked_layer() {
    let files = common::archives(&common::hd().join("Data"));

    let mut slots = 0usize;
    // (kind, is the base layer, carries an offset) -> slots
    let mut spread: BTreeMap<(String, bool, bool), usize> = BTreeMap::new();
    // Records with a layer-0 offset.
    let mut base: BTreeMap<String, usize> = BTreeMap::new();

    for path in files {
        let bytes = std::fs::read(&path).expect("an archive");
        let Ok(archive) = regent_core::data::cam::Archive::parse(&bytes) else {
            continue;
        };

        for (chunk, record) in archive.records() {
            if chunk.tag.as_bytes() != b"IMAG" {
                continue;
            }

            let Ok(image) = regent_core::data::imag::Image::parse(record.data) else {
                continue;
            };

            for set in &image.sets {
                for slot in &set.slots {
                    slots += 1;

                    let moved = slot.x != 0 || slot.y != 0;
                    let key = (set.kind().to_string(), set.variant() == 0, moved);
                    *spread.entry(key).or_default() += 1;

                    if moved && set.variant() == 0 {
                        *base.entry(record.id.trimmed().to_owned()).or_default() += 1;
                    }
                }
            }
        }
    }

    assert_eq!(slots, 10_024, "slots in HD's Data changed");

    let cell = |kind: &str, first: bool, moved: bool| {
        spread
            .get(&(kind.to_owned(), first, moved))
            .copied()
            .unwrap_or(0)
    };
    let moved: usize = spread
        .iter()
        .filter(|((_, _, moved), _)| *moved)
        .map(|(_, n)| n)
        .sum();

    assert_eq!(moved, 1_479, "slots carrying an offset changed");

    // (layer 0 flat, layer 0 moved, layer n flat, layer n moved). Key fact:
    // `Die` layer 0 never moves.
    for (kind, want) in [
        ("Die", [1_081, 0, 12, 1_451]),
        ("Active", [86, 0, 39, 4]),
        ("Inactive", [406, 0, 36, 6]),
        ("Dead", [153, 2, 0, 0]),
        ("Walk", [704, 8, 0, 0]),
        ("Interface", [3_489, 6, 0, 1]),
        ("Special", [616, 0, 3, 1]),
    ] {
        let got = [
            cell(kind, true, false),
            cell(kind, true, true),
            cell(kind, false, false),
            cell(kind, false, true),
        ];

        assert_eq!(got, want, "{kind}: flat and offset slots by layer");
    }

    // These kinds never carry an offset.
    for quiet in ["Attack", "Build", "Carry", "Cast", "Crumble"] {
        assert_eq!(cell(quiet, true, true) + cell(quiet, false, true), 0);
    }

    let scatter: usize = base
        .iter()
        .filter(|(id, _)| id.as_bytes()[1] == b'G')
        .map(|(_, n)| n)
        .sum();

    assert_eq!(base.values().sum::<usize>(), 16, "layer-0 offsets");
    assert_eq!(scatter, 0, "layer-0 offsets on ground scatter");
}

/// `Active` (working) and `Inactive` (idle) differ on 54 of 60 records with
/// both. The six that do not include all three Palaces (tile 1036 plus the
/// same overlay).
///
/// `ABH3 Marketplace3`'s `Active` has an extra layer: awnings and pennants.
#[test]
fn a_buildings_two_pictures_are_two_pictures_bar_six() {
    let library = load("Data/MajestyDatasetDefinitions.xml", "Data");
    let cat = library.catalogue().expect("every archive parses");

    // Every frame of a kind, layer by layer.
    let frames = |unit: &Unit<'_>, kind: SetKind| -> Vec<FrameSource> {
        unit.layers(kind)
            .flat_map(|set| set.poses.iter())
            .flat_map(|pose| pose.frames.iter().copied())
            .collect()
    };

    let (mut both, mut differ) = (0, 0);
    let mut identical: Vec<String> = Vec::new();

    for (at, _) in cat.tagged(b"DUNT") {
        let Ok(unit) = Unit::at(&cat, at) else {
            continue;
        };

        let working = frames(&unit, SetKind::Active);
        let idle = frames(&unit, SetKind::Inactive);

        if working.is_empty() || idle.is_empty() {
            continue;
        }

        both += 1;

        if working == idle {
            identical.push(unit.id.trimmed().to_owned());
        } else {
            differ += 1;
        }
    }

    identical.sort();

    assert_eq!((both, differ), (60, 54), "records carrying both kinds");
    assert_eq!(
        identical,
        ["ABJ1", "ABJ2", "ABJ3", "ABL1", "ABe1", "BBA1"],
        "which records draw the same thing working and idle"
    );
}

/// All three are `SetKind::Interface`; ask by id. Position varies (icon is set
/// 8 on `AVb0`, set 3 on `AVL1`); size does not.
#[test]
fn a_units_interface_sets_are_three_sizes_of_one_picture() {
    use regent_core::data::imag::{INTERFACE_ICON, INTERFACE_PANEL, INTERFACE_PORTRAIT};

    let library = load("Data/MajestyDatasetDefinitions.xml", "Data");
    let catalogue = library.catalogue().unwrap();

    let want = [
        (INTERFACE_PORTRAIT, (100, 100)),
        (INTERFACE_PANEL, (200, 245)),
        (INTERFACE_ICON, (25, 25)),
    ];

    let mut seen = [0usize; 3];

    for (at, _) in catalogue.tagged(b"DUNT") {
        let Ok(unit) = Unit::at(&catalogue, at) else {
            continue;
        };

        for (which, (id, size)) in want.iter().enumerate() {
            let Some(set) = unit.interface(*id) else {
                continue;
            };
            let source = set.poses[0].stream(0)[0];
            let regent_core::content::imagery::FrameSource::Tile { archive, index, .. } = source
            else {
                continue;
            };
            let Some(Ok(sprite)) = regent_core::view::Sprite::at(&catalogue, archive, index) else {
                continue;
            };

            // Only the panel varies: 244 high on `AVb0`, 245 elsewhere.
            let close = if *id == INTERFACE_PANEL { 2 } else { 0 };

            assert!(
                sprite.width.abs_diff(size.0) <= close && sprite.height.abs_diff(size.1) <= close,
                "{} {id} is {} by {}, not {size:?}",
                unit.id,
                sprite.width,
                sprite.height
            );

            seen[which] += 1;
        }
    }

    // Portrait: 81 buildings, 60 characters, 3 scatter, 2 overlays. Panel: 20
    // characters. Icon: 52 buildings, 24 characters.
    assert_eq!(seen, [146, 20, 76], "the interface sets moved");

    // Every recruitable class has an icon, which the map view needs.
    for id in [
        "AVA1", "AVB1", "AVL1", "AVN1", "AVe1", "AVj1", "AVg1", "AVb0",
    ] {
        let four = regent_core::data::fourcc::FourCc::new(id.as_bytes()[..4].try_into().unwrap());
        let unit = Unit::find(&catalogue, four).unwrap().unwrap();

        assert!(unit.interface(INTERFACE_ICON).is_some(), "{id} has no icon");
    }
}

/// (id, name, lowest body pixel below anchor row, shadow centroid relative to
/// anchor if any).
type Stance = (FourCc, String, i32, Option<(i32, i32)>);

/// One [`Stance`] per base character, from its first standing frame.
fn characters_against_their_anchor(
    cat: &regent_core::data::catalogue::Catalogue<'_>,
) -> Vec<Stance> {
    let mut out = Vec::new();

    for (_, parsed) in &cat.archives {
        for (chunk, record) in parsed.records() {
            if chunk.tag != b"DUNT" {
                continue;
            }

            let Some(Ok(unit)) = Unit::find(cat, record.id) else {
                continue;
            };

            if unit.desc.kind() != UnitKind::Character {
                continue;
            }

            // Both put the feet on the ground.
            let Some(kind) = [SetKind::Stand, SetKind::Walk]
                .into_iter()
                .find(|k| unit.animation(*k).is_some())
            else {
                continue;
            };
            let Some(FrameSource::Tile { archive, index, .. }) = unit
                .animation(kind)
                .and_then(|set| set.poses.first())
                .and_then(|pose| pose.stream(0).first())
                .copied()
            else {
                continue;
            };
            let Some(Ok(frame)) = Sprite::at(cat, archive, index) else {
                continue;
            };

            let (mut foot, mut rows, mut cols, mut band) = (None, 0usize, 0usize, 0usize);
            let width = usize::from(frame.width);

            for row in 0..usize::from(frame.height) {
                for col in 0..width {
                    let i = row * width + col;

                    match frame.ink(i, Blit::World) {
                        Ink::Colour(_) => foot = foot.max(Some(row)),
                        // Darkening only; `Raise` is a glow round a figure.
                        Ink::Shade(Shade::Keep(_)) => {
                            band += 1;
                            rows += row;
                            cols += col;
                        }
                        Ink::Shade(Shade::Raise(_)) | Ink::Hole => {}
                    }
                }
            }

            let Some(foot) = foot.and_then(|row| i32::try_from(row).ok()) else {
                continue;
            };

            let anchor = (i32::from(frame.anchor.0), i32::from(frame.anchor.1));

            // A centroid is inside the tile, so both fit an i32.
            let mean = |total: usize| i32::try_from(total / band).expect("a tile coordinate");
            let shadow = (band > 0).then(|| (mean(cols) - anchor.0, mean(rows) - anchor.1));

            out.push((
                record.id,
                String::from_utf8_lossy(unit.desc.head.name)
                    .trim()
                    .to_owned(),
                foot - anchor.1,
                shadow,
            ));
        }
    }

    out.sort_by_key(|found| found.0);
    out
}

/// The anchor at `+0x0a` (`docs/formats/art.md`) is where the unit stands: the
/// lowest body pixel is at or below it, median six rows (feet are an ellipse
/// centred on the anchor).
///
/// The four exceptions fly or hover, confirmed by eye. So flight height is
/// readable off the art.
#[test]
fn a_character_stands_on_its_anchor_unless_it_flies() {
    let library = load("Data/MajestyDatasetDefinitions.xml", "Data");
    let cat = library.catalogue().expect("every archive parses");
    let found = characters_against_their_anchor(&cat);

    assert_eq!(found.len(), 59, "characters with a standing frame changed");

    let airborne: Vec<(String, i32)> = found
        .iter()
        .filter(|(_, _, foot, _)| *foot < 0)
        .map(|(_, name, foot, _)| (name.clone(), *foot))
        .collect();

    assert_eq!(
        airborne,
        [
            ("Evil_Oculus".to_owned(), -5),
            ("Giant_Eagle".to_owned(), -37),
            ("Harpy".to_owned(), -39),
            ("Black_Phantom".to_owned(), -6),
        ],
        "the characters drawn off the ground changed",
    );

    let mut feet: Vec<i32> = found.iter().map(|(_, _, foot, _)| *foot).collect();
    feet.sort_unstable();

    assert_eq!(feet[feet.len() / 2], 6, "the median foot moved");
}

/// Light from the lower right (`data::settings::LIGHT`); shadows fall up-left.
/// See `docs/formats/art.md`, "Where a shadow falls".
///
/// Only x is asserted: low, long animals spread their shadow around the anchor.
#[test]
fn a_characters_shadow_falls_to_the_left_of_its_anchor() {
    let library = load("Data/MajestyDatasetDefinitions.xml", "Data");
    let cat = library.catalogue().expect("every archive parses");
    let found = characters_against_their_anchor(&cat);

    let shadowed: Vec<&Stance> = found.iter().filter(|f| f.3.is_some()).collect();

    assert_eq!(shadowed.len(), 57, "characters with a shadow changed");

    let right: Vec<&str> = shadowed
        .iter()
        .filter(|f| f.3.expect("a shadow").0 >= 0)
        .map(|f| f.1.as_str())
        .collect();

    assert!(
        right.is_empty(),
        "a shadow moved to the other side: {right:?}"
    );

    // Furthest: the Oculus, lowest of the four fliers.
    let furthest = shadowed
        .iter()
        .min_by_key(|f| f.3.expect("a shadow").0)
        .expect("a shadow");

    assert_eq!(
        (furthest.1.as_str(), furthest.3),
        ("Evil_Oculus", Some((-63, -18)))
    );
}

/// A control-band index is an operation (e.g. shadow), lost when flattened to
/// RGB, so the renderer wants indexed frames.
///
/// Direct-colour frames: `Interface` portraits (icon path) and `ABD1`
/// Fairgrounds' `Crumble`, which the map draws. So the map needs a direct path
/// for that one ruin, without shadow.
#[test]
fn a_units_frames_are_indexed_bar_the_fairgrounds_ruin() {
    let base = load("Data/MajestyDatasetDefinitions.xml", "Data");
    let mx = load("DataMX/MajestyExpansionDatasetDefinitions.xml", "DataMX");

    let mut direct: BTreeMap<String, usize> = BTreeMap::new();
    let mut outside: Vec<String> = Vec::new();
    let mut frames = 0usize;

    for library in [&base, &mx] {
        let cat = library.catalogue().expect("every archive parses");

        for (_, parsed) in &cat.archives {
            for (chunk, record) in parsed.records() {
                if chunk.tag != b"DUNT" {
                    continue;
                }

                let Some(Ok(unit)) = Unit::find(&cat, record.id) else {
                    continue;
                };

                for set in &unit.animations {
                    for pose in &set.poses {
                        for source in &pose.frames {
                            let FrameSource::Tile { archive, index, .. } = *source else {
                                continue;
                            };
                            let Some(Ok(frame)) = Sprite::at(&cat, archive, index) else {
                                continue;
                            };

                            frames += 1;

                            if matches!(frame.colour, Colour::Direct { .. }) {
                                *direct.entry(set.kind.to_string()).or_default() += 1;

                                if set.kind != SetKind::Interface {
                                    outside.push(record.id.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    outside.sort();
    outside.dedup();

    assert_eq!(frames, 73_219, "frames walked changed");
    assert_eq!(
        direct,
        BTreeMap::from([("Crumble".to_owned(), 9), ("Interface".to_owned(), 38)]),
        "the sets holding a direct-colour frame changed",
    );
    assert_eq!(
        outside,
        ["ABD1"],
        "the Fairgrounds is no longer the only one"
    );
}

/// A slot's point run is `[count][pairs]` repeated, ending at count 0. List
/// 0's count is the last header dword, one word before the run as sliced.
///
/// **Binary** — slot point list returns
/// `slot + 8 * which`: two `{count, pointer}` vectors.
///
/// List 0 of a building is its base polygon, painted bare on placement.
/// Hence pads have clipped
/// corners, unlike the footprint rect.
#[test]
fn a_slots_point_run_is_a_chain_of_lists() {
    let files = common::archives(&common::hd().join("Data"));
    let (mut with, mut whole, mut lists) = (0usize, 0usize, 0usize);

    for path in files {
        let bytes = std::fs::read(&path).expect("an archive");
        let Ok(archive) = regent_core::data::cam::Archive::parse(&bytes) else {
            continue;
        };

        for (chunk, record) in archive.records() {
            if chunk.tag.as_bytes() != b"IMAG" {
                continue;
            }

            let Ok(image) = regent_core::data::imag::Image::parse(record.data) else {
                continue;
            };

            for set in &image.sets {
                for slot in &set.slots {
                    if slot.points.is_empty() {
                        continue;
                    }

                    with += 1;
                    whole += usize::from(slot.points_whole());
                    lists += slot.point_lists().len();
                }
            }
        }
    }

    println!("  {with} slots carry points, {whole} account for every byte, {lists} lists in all");

    assert_eq!(with, 168, "how many slots carry a point run changed");
    assert_eq!(
        whole,
        with,
        "{} slots have bytes the chain does not account for",
        with - whole
    );
    assert_eq!(lists, 195, "how many lists those slots carry changed");
}

/// A slot's block 0 is the footprint rect.
///
/// **Binary** — `IsoFootprintRectManager` takes it as
/// `left top right bottom` if `left < right` and `top < bottom`. Scatter
/// refuses a spot whose cells are not all free. Block 0 is at `+0x00` on
/// version 2 and the header end later, present only with mask bit 0.
/// Degenerate rects (point or line)
/// fail the engine's test.
///
/// Version-2 slots keep the pose offset at `+0x18`, where all are zero.
/// Reading it at the wrong place gave 76 false layer-0 offsets.
#[test]
fn a_slots_footprint_is_where_the_thing_stands() {
    use regent_core::data::imag::Version;

    let files = common::archives(&common::hd().join("Data"));

    let (mut slots, mut rect, mut empty, mut degenerate) = (0usize, 0usize, 0usize, 0usize);
    let (mut two, mut two_moved, mut two_offset, mut base_moved) = (0usize, 0usize, 0usize, 0usize);
    let mut version_of_moved: BTreeMap<u32, usize> = BTreeMap::new();

    for path in files {
        let bytes = std::fs::read(&path).expect("an archive");
        let Ok(archive) = regent_core::data::cam::Archive::parse(&bytes) else {
            continue;
        };

        for (chunk, record) in archive.records() {
            if chunk.tag.as_bytes() != b"IMAG" {
                continue;
            }

            let Ok(image) = regent_core::data::imag::Image::parse(record.data) else {
                continue;
            };

            for set in &image.sets {
                for slot in &set.slots {
                    let [left, top, right, bottom] = slot.footprint;

                    slots += 1;

                    if slot.footprint == [0; 4] {
                        empty += 1;
                    } else if left < right && top < bottom {
                        rect += 1;
                    } else {
                        degenerate += 1;
                    }

                    if image.version == Version::Two {
                        two += 1;
                        two_moved += usize::from((slot.footprint[0], slot.footprint[1]) != (0, 0));
                        two_offset += usize::from((slot.x, slot.y) != (0, 0));
                    }

                    if (slot.x, slot.y) != (0, 0) && set.variant() == 0 {
                        base_moved += 1;
                        *version_of_moved.entry(image.version.number()).or_default() += 1;
                    }
                }
            }
        }
    }

    println!(
        "  {slots} slots: {rect} rects, {empty} empty, {degenerate} degenerate; \
         version 2 {two}, {two_moved} with a footprint, {two_offset} with an offset; \
         layer 0 offsets {base_moved} by version {version_of_moved:?}"
    );

    assert_eq!(slots, 10_024, "slots in HD's Data changed");
    assert_eq!(
        (rect, empty, degenerate),
        (3_320, 6_494, 210),
        "what a slot's block 0 reads as changed"
    );
    // `Slot::x` reads version 2 at `+0x18`, as the engine does.
    assert_eq!(two_offset, 0, "a version-2 slot moved its pose");
    assert_eq!(
        version_of_moved.get(&2).copied().unwrap_or(0),
        0,
        "a version-2 slot carries a layer-0 offset"
    );
    assert_eq!(
        two_moved, 76,
        "the version-2 slots with a footprint changed"
    );
}

/// Units with only a `Special` set and a valid, ignored slot rect:
/// (id, slot rect, frame union).
///
/// **Binary** — `IsoFootprintRectManager` asks
/// slot block for a rect on sets 8 and `0xd0` only, then unions frame
/// boxes on 8, `0xd0`, `0x40`. So these are measured by their pictures.
///
/// Three are scatter codes: see
/// `a_special_set_is_measured_by_its_pictures_and_not_by_its_slot` in
/// `the_corpus/whole_generator.rs`.
const SPECIAL_ONLY: [(&[u8; 4], [i16; 4], [i16; 4]); 6] = [
    (b"ARB2", [-1, -74, 4, 8], [-11, -27, 8, -13]),
    (b"ARE1", [-27, -30, 27, 35], [-21, -25, 20, 30]),
    (b"NGe1", [-30, -18, 32, 18], [-37, -30, 38, 24]),
    (b"NGf1", [-19, -11, 21, 17], [-23, -20, 18, 15]),
    (b"NGl1", [-40, -35, 78, 47], [-60, -61, 87, 64]),
    (b"YG02", [-64, -68, 90, 42], [-65, -44, 97, 44]),
];

/// (nudged frames, all frames, min, max) over HD's art.
///
/// **Binary** — frame box union adds the `i16` pair at
/// frame `+4`/`+6` to the tile box. The loader swaps the file's words, so this
/// is `regent_core::data::imag::Frame::metadata`. The picture-sized spread
/// supports the reading.
const NUDGES: (usize, usize, (i16, i16), (i16, i16)) = (2_895, 86_868, (-196, -149), (110, 130));

/// Set `0x40` gives a frame union, never its slot rect.
#[test]
fn a_special_sets_slot_rect_is_never_the_footprint() {
    let library = hd_art();
    let catalogue = library.catalogue().expect("a catalogue of HD's art");
    let mut found = 0;

    for (at, record) in catalogue.tagged(b"DUNT") {
        let Ok(unit) = Unit::at(&catalogue, at) else {
            continue;
        };

        // Engine order: slot 0 of set 8, then 0xd0; proper rects only.
        let stands = |id: u32| {
            unit.animations
                .iter()
                .find(|set| set.id == id)
                .and_then(|set| set.poses.first())
                .map(|pose| pose.footprint)
                .filter(|r| r[0] < r[2] && r[1] < r[3])
        };

        let (None, None, Some(rect)) = (stands(0x08), stands(0xd0), stands(0x40)) else {
            continue;
        };
        let got = regent_core::sim::footprint::of_unit(&catalogue, &unit).expect("its art");
        let want = SPECIAL_ONLY
            .iter()
            .find(|(id, _, _)| record.id == FourCc::new(id));

        println!("  {} slot {rect:?} -> {got:?}", record.id);
        found += 1;

        assert_eq!(
            want.map(|(_, slot, union)| (*slot, *union)),
            Some((rect, got.expect("a union"))),
            "{} is measured differently from what the table says",
            record.id,
        );
    }

    assert_eq!(
        found,
        SPECIAL_ONLY.len(),
        "how many such units there are moved"
    );
}

#[test]
fn a_frames_metadata_word_is_a_nudge() {
    use regent_core::data::imag::Image;

    let library = hd_art();
    let catalogue = library.catalogue().expect("a catalogue of HD's art");
    let (mut frames, mut nudged) = (0usize, 0usize);
    let mut spread = ((0i16, 0i16), (0i16, 0i16));

    for (at, _) in catalogue.tagged(b"IMAG") {
        let Some(record) = catalogue.record(b"IMAG", at) else {
            continue;
        };
        let Ok(image) = Image::parse(record.data) else {
            continue;
        };

        for frame in image
            .sets
            .iter()
            .flat_map(|set| &set.slots)
            .flat_map(regent_core::data::imag::Slot::frames)
        {
            let (x, y) = frame.nudge();

            frames += 1;
            nudged += usize::from((x, y) != (0, 0));
            spread = (
                (spread.0.0.min(x), spread.0.1.min(y)),
                (spread.1.0.max(x), spread.1.1.max(y)),
            );
        }
    }

    println!("  {nudged} of {frames} frames carry a nudge, spread {spread:?}");

    assert_eq!(
        (nudged, frames, spread.0, spread.1),
        NUDGES,
        "what a frame's metadata word holds moved",
    );
}

/// All HD archives, both datasets.
fn hd_art() -> &'static Library {
    common::library()
}

/// A generated name reads one pick a table, table 0 in the top byte:
/// `NM01` spells from `HN01`..`HN04`, of which HD fills two; `NM17`, the
/// buildings', from `BN01`.
#[test]
fn a_packed_name_spells_one_pick_a_table() {
    let library = hd_art();
    let catalogue = library.catalogue().expect("a catalogue");
    let text = regent_core::data::text::Text::build(&catalogue);
    let hero = FourCc(*b"NM01");
    let spelled = |which, packed| regent_core::sim::world::naming::spelled(&text, which, packed);

    assert_eq!(spelled(hero, 0), "Clovis Daysbane");
    assert_eq!(spelled(hero, 3 << 24 | 3 << 16), "Lucius Longstrider");
    assert_eq!(spelled(FourCc(*b"NM17"), 1 << 24), "The Pourhouse");

    // An empty table adds nothing, whatever it picks; an unknown
    // generator spells nothing.
    assert_eq!(spelled(hero, 1 << 8), "Clovis Daysbane");
    assert_eq!(spelled(FourCc(*b"NM99"), 0), "");
}

/// `Got_Gold`'s second layer is the amount the effector carries, drawn
/// as text (reference kind `0x08000000`, `IsoUnitIMG`), not tile 0
/// of its archive: no frame of it blits a tile; the coin's all do. A
/// hidden frame (bit 30) blits none either.
#[test]
fn got_golds_amount_layer_blits_no_tile() {
    let library = hd_art();
    let catalogue = library.catalogue().expect("a catalogue of HD's art");
    let unit = Unit::find(&catalogue, FourCc(*b"ARB2"))
        .expect("Got_Gold")
        .expect("Got_Gold parses");
    let frames = |variant: u8| -> Vec<FrameSource> {
        unit.stack(0x40)
            .filter(|a| a.variant == variant)
            .flat_map(|a| a.poses.iter().flat_map(|p| p.frames.clone()))
            .collect()
    };

    assert_eq!(frames(1).len(), 16);
    assert!(frames(1).iter().all(|f| f.tile().is_none()));
    assert!(frames(0).iter().all(|f| f.tile().is_some()));

    let hidden = FrameSource::Tile {
        archive: 0,
        index: 1,
        flags: 0x4000,
    };

    assert_eq!(hidden.tile(), None);
    assert!(!hidden.is_text());

    // The amount's frames are text, rising with their nudges.
    let amount = unit.stack(0x40).find(|a| a.variant == 1).expect("layer 1");
    let rise: Vec<Option<i16>> = amount.poses[0]
        .texts(0)
        .iter()
        .map(|t| t.map(|(_, y)| y))
        .collect();

    assert_eq!(
        rise,
        [
            0, -4, -8, -12, -16, -22, -24, -28, -32, -36, -39, -42, -44, -46, -47, -48
        ]
        .map(Some)
    );
}

/// `IsoGenUnit` blits every stream of a slot at the part's frame,
/// each frame at its own nudge: `energy_blast_effector`'s burst and its
/// streak.
#[test]
fn every_stream_blits_at_its_frames_nudge() {
    let library = hd_art();
    let catalogue = library.catalogue().expect("a catalogue of HD's art");
    let unit = Unit::find(&catalogue, FourCc(*b"WRa1"))
        .expect("the unit")
        .expect("it parses");
    let blast = &unit.stack(0x60).next().expect("the set").poses[0];
    let drawn = |s| blast.blits(s, Pass::First).iter().flatten().count();

    assert_eq!(blast.streams, 2);
    assert_eq!((drawn(0), drawn(1)), (10, 10));
    assert_eq!(
        blast.blits(1, Pass::First)[0].map(|b| b.nudge),
        Some((-14, 33))
    );
    assert_eq!(
        blast.blits(0, Pass::First)[0].map(|b| b.nudge),
        Some((0, 0))
    );
}

/// A layer with flags bits 8 and 9 makes two parts:
/// the energy blast missile's first draws its body, stream 0, its
/// second exactly the bit 24 frames, stream 1, its shadow.
#[test]
fn a_missiles_second_part_draws_its_bit_24_frames() {
    let library = hd_art();
    let catalogue = library.catalogue().expect("a catalogue of HD's art");
    let unit = Unit::find(&catalogue, FourCc(*b"WPa2"))
        .expect("the unit")
        .expect("it parses");
    let set = unit.stack(0x40).next().expect("the set");
    let missile = &set.poses[0];
    let drawn = |s, pass| missile.blits(s, pass).iter().flatten().count();
    let second: Vec<bool> = missile
        .frames
        .iter()
        .map(|f| matches!(f, FrameSource::Tile { flags, .. } if Pass::of(u32::from(*flags) << 16) == Pass::Second))
        .collect();

    assert_eq!(set.passes, [Pass::First, Pass::Second]);
    assert_eq!(missile.streams, 2);
    assert_eq!((drawn(0, Pass::First), drawn(1, Pass::First)), (14, 0));
    assert_eq!((drawn(0, Pass::Second), drawn(1, Pass::Second)), (0, 14));
    assert_eq!(second.iter().filter(|s| **s).count(), 14);
    assert!(second[14..].iter().all(|s| *s), "the shadow is stream 1");
}

/// `IsoDrawList` sorts a part whose set has layer flags bit 0 at its
/// y plus set `+0x3c` screen pixels: reward flags far nearer than all
/// about them, so over them; thought bubbles and `Got_Gold` take none.
#[test]
fn a_sets_sort_nudge_is_read_where_its_flag_says() {
    let library = hd_art();
    let catalogue = library.catalogue().expect("a catalogue of HD's art");
    let nudge = |code: &[u8; 4], set: u16| {
        Unit::find(&catalogue, FourCc(*code))
            .expect("the unit")
            .expect("it parses")
            .stack(set)
            .next()
            .expect("the set")
            .nudge
    };

    assert_eq!(nudge(b"ARA2", 0x40), 2000);
    assert_eq!(nudge(b"ARA4", 0x40), 1500);
    assert_eq!(nudge(b"ABJ1", 0xe0), -1000);
    assert_eq!(nudge(b"AVH1", 0x08), -15);
    assert_eq!(nudge(b"ARG1", 0x40), 0);
    assert_eq!(nudge(b"ARB2", 0x40), 0);
}

/// `IsoDrawList` lists a higher layer's part apart only where its
/// set has layer flags bit 0, at its own nudge; else it draws with part
/// 0. The Mausoleum's dirt diamond, `Inactive` layer 1, sorts 1,000
/// pixels further, under the building; so do seven expansion buildings'
/// ground layers. Vendral's `Cast` layer 1, at 79 to its body's 80, is HD's
/// one other.
#[test]
fn a_buildings_ground_layer_sorts_under_it() {
    use regent_core::content::imagery;

    let library = hd_art();
    let catalogue = library.catalogue().expect("a catalogue of HD's art");
    let listed = |code: &[u8; 4], set: u16| -> Vec<i32> {
        let unit = Unit::find(&catalogue, FourCc(*code))
            .expect("the unit")
            .expect("it parses");
        let stack: Vec<_> = unit.stack(set).collect();

        stack.iter().map(|a| a.listed(stack[0])).collect()
    };

    assert_eq!(listed(b"ABp1", 0xd0), [0, -1000]);
    assert_eq!(listed(b"ABJ1", 0xc0), [0, 0], "the palace's banner");
    assert_eq!(listed(b"ABJ1", 0xe0)[0], -1000, "a ruin");

    // Records with a layer above 0 sorting under its base.
    let under: Vec<String> = catalogue
        .tagged(b"IMAG")
        .filter(|(at, _)| {
            imagery::sets(&catalogue, *at).is_ok_and(|sets| {
                sets.iter().any(|a| {
                    let base = sets
                        .iter()
                        .find(|b| b.id == u32::from(regent_core::data::imag::set_number(a.id)));

                    a.variant > 0 && base.is_some_and(|b| a.listed(b) < b.listed(b))
                })
            })
        })
        .map(|(_, r)| r.id.to_string())
        .collect();

    assert_eq!(
        under,
        [
            "BVg1", "ABl1", "ABl2", "ABl3", "ABp1", "BBu1", "BBz1", "BBQ1"
        ]
    );
}

/// `FONT` is a bitmap font a bit a pixel (`CYFontPixelmap`'s loader,
/// font pixelmap load): both records parse, and `SM09`, the
/// font `Got_Gold`'s amount is drawn in, draws digits.
#[test]
fn a_font_record_is_a_bitmap_of_glyphs() {
    use regent_core::data::font::Font;

    let library = hd_art();
    let catalogue = library.catalogue().expect("a catalogue of HD's art");
    let fonts: BTreeMap<String, Font<'_>> = catalogue
        .tagged(b"FONT")
        .map(|(_, r)| (r.id.to_string(), Font::parse(r.data).expect("parses")))
        .collect();
    let small = &fonts["SM09"];
    let shape = |font: &Font<'_>, text: &[u8]| -> Vec<String> {
        let lit = font.pixels(text);
        let rows = i32::try_from(font.rows).expect("rows");

        (0..rows)
            .map(|y| {
                (0..font.width(text))
                    .map(|x| if lit.contains(&(x, y)) { '#' } else { '.' })
                    .collect()
            })
            .collect()
    };

    assert_eq!(fonts.len(), 2);
    assert_eq!((small.first, small.last, small.rows), (0, 219, 9));
    assert_eq!(usize::from(small.ascent + small.descent), small.rows);
    assert_eq!(fonts["HB12"].rows, 13);
    assert_eq!(
        shape(small, b"10"),
        [
            ".##..####..",
            "###.##..##.",
            ".##.##..##.",
            ".##.##..##.",
            ".##.##..##.",
            ".##.##..##.",
            ".##..####..",
            "...........",
            "...........",
        ]
    );

    // `Got_Gold`'s 100, as `IsoUnitIMG` boxes it: 18 x 9, its top
    // 4 rows above the anchor.
    let rows = i32::try_from(small.rows).expect("rows");

    assert_eq!(small.width(b"100"), 18);
    assert_eq!(regent_core::view::map::amount::top(rows), -4);
}
