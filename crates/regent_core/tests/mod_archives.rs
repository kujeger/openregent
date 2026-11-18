//! A mod's archives over a base game's, both made up
//! (`fixtures/archives`): the data formats on bytes no Cyberlore tool
//! wrote, and how a mod's records compose with the base's. Needs no game
//! data.

#[path = "fixtures/archives/mod.rs"]
mod archives;

use std::collections::{BTreeSet, HashSet};

use regent_core::content::imagery::{self, FrameSource};
use regent_core::content::unit::Unit;
use regent_core::data::catalogue::Catalogue;
use regent_core::data::deps;
use regent_core::data::desc::{self, User};
use regent_core::data::fourcc::FourCc;
use regent_core::data::imag::{
    self, INTERFACE_ICON, INTERFACE_PANEL, INTERFACE_PORTRAIT, set_number,
};
use regent_core::data::{cam, payload, tile};
use regent_core::view::{Colour, PaletteSource, Sprite};

use archives::Archives;

/// Every `(chunk tag, record name)` pair in `archives`.
fn contents(archives: &Archives) -> HashSet<(FourCc, String)> {
    archives
        .iter()
        .flat_map(|(name, bytes)| {
            let archive = cam::Archive::parse(bytes).unwrap_or_else(|e| panic!("{name}: {e}"));

            archive
                .records()
                .map(|(chunk, record)| (chunk.tag, record.name().into_owned()))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// The mod's archives, its descriptions made into one after them.
fn mod_archives() -> Archives {
    let composed = archives::composed();
    let first = archives::base().len();

    composed.into_iter().skip(first).collect()
}

/// `DATA` record `id` as text.
fn text_of(bytes: &[u8], id: FourCc) -> String {
    let archive = cam::Archive::parse(bytes).expect("a readable archive");
    let (_, record) = archive
        .records()
        .find(|(chunk, record)| chunk.tag.as_bytes() == b"DATA" && record.id == id)
        .unwrap_or_else(|| panic!("no {id}"));

    String::from_utf8_lossy(record.data).into_owned()
}

/// Each payload reader takes the mod's records: archive version 1.1,
/// RIFF/WAVE sounds, 256-colour palettes, descriptors repeating their
/// chunk's tag, string tables.
#[test]
fn every_mod_archive_parses() {
    let mut tags = BTreeSet::new();

    for (name, bytes) in &mod_archives() {
        let archive = cam::Archive::parse(bytes).unwrap_or_else(|e| panic!("{name}: {e}"));

        assert_eq!(archive.version, cam::VERSION, "{name}");

        for (chunk, record) in archive.records() {
            let tag = chunk.tag.as_bytes();
            let at = || format!("{name} {} {}", chunk.tag, record.id);

            if payload::WAVE_TAGS.contains(&tag) {
                payload::wave(record.data).unwrap_or_else(|e| panic!("{}: {e}", at()));
            } else if payload::PALETTE_TAGS.contains(&tag) {
                let palette = payload::Palette::parse(record.data)
                    .unwrap_or_else(|e| panic!("{}: {e}", at()));

                assert_eq!(palette.colours.len(), payload::PALETTE_COLOURS, "{}", at());
            } else if payload::DESCRIPTOR_TAGS.contains(&tag) {
                let descriptor = payload::Descriptor::parse(record.data)
                    .unwrap_or_else(|e| panic!("{}: {e}", at()));

                assert_eq!(descriptor.tag, chunk.tag, "{}", at());
            } else if tag == b"STRT" {
                payload::StringTable::parse(record.data)
                    .unwrap_or_else(|e| panic!("{}: {e}", at()));
            }

            tags.insert(*tag);
        }
    }

    // What the mod ships: every reader above had something to read.
    let want: BTreeSet<[u8; 4]> = [
        b"TILE", b"SPLT", b"IMAG", b"DATA", b"STRT", b"WAVE", b"DUNT", b"DSND",
    ]
    .into_iter()
    .copied()
    .collect();

    assert_eq!(tags, want);
}

/// Every mod tile decodes, in each of the three versions. `TILE`
/// references are positional, so a mod pads with empty records
/// (`docs/formats/README.md`, no archive is sparse).
#[test]
fn every_mod_tile_decodes() {
    let (mut versions, mut empty) = (BTreeSet::new(), 0);

    for (name, bytes) in &mod_archives() {
        let archive = cam::Archive::parse(bytes).expect("a readable archive");

        for (_, record) in archive.records().filter(|(c, _)| c.tag == b"TILE") {
            if record.data.is_empty() {
                empty += 1;
            } else {
                let t = tile::Tile::parse(record.data)
                    .unwrap_or_else(|e| panic!("{name} {}: {e}", record.name()));

                if let Some(rows) = t.runs() {
                    assert_eq!(rows.len(), usize::from(t.height()));
                }

                assert!(t.max_end() <= t.width(), "{name} {}", record.name());
                versions.insert(t.version());
            }
        }
    }

    assert_eq!(empty, archives::PADDING, "the padding records");
    assert_eq!(
        versions,
        BTreeSet::from([tile::VERSION_OPAQUE, tile::VERSION_KEYED, tile::VERSION_RLE])
    );
}

/// All four `IMAG` layouts decode, and every frame is a tile of the
/// `IMAG`'s own archive: references are positional.
#[test]
fn every_mod_imag_layout_decodes() {
    let (mut versions, mut frames) = (BTreeSet::new(), 0);

    for (name, bytes) in &mod_archives() {
        let archive = cam::Archive::parse(bytes).expect("a readable archive");
        let tiles = archive.chunk_by_tag(b"TILE").map_or(0, |c| c.records.len());

        for (_, record) in archive.records().filter(|(c, _)| c.tag == b"IMAG") {
            let image = imag::Image::parse(record.data)
                .unwrap_or_else(|e| panic!("{name} {}: {e}", record.name()));

            for frame in image
                .sets
                .iter()
                .flat_map(|s| &s.slots)
                .flat_map(imag::Slot::frames)
            {
                let imag::FrameRef::Tile { index, .. } = frame.target() else {
                    panic!("{name} {}: {frame:?}", record.name());
                };

                assert!(usize::from(index) < tiles, "{name} {}", record.name());
                frames += 1;
            }

            versions.insert(image.version.number());
        }
    }

    assert_eq!(versions, BTreeSet::from([2, 3, 4, 5]));
    assert_eq!(frames, archives::MOD_FRAMES);
}

/// Each indexed mod tile finds its palette in its own archive: an `SPLT`
/// it numbers, or one embedded. The mod's `SPLT` chunk is unflagged, so
/// the number is a record id, not a position
/// ([`Catalogue::positional`]).
#[test]
fn every_mod_tile_resolves_its_palette() {
    let (mut indexed, mut embedded, mut direct) = (0, 0, 0);

    for (name, bytes) in mod_archives() {
        let owned = vec![(name.clone(), bytes)];
        let catalogue = Catalogue::build(&owned).expect("a catalogue");
        let Some(chunk) = catalogue.archives[0].1.chunk_by_tag(b"TILE") else {
            continue;
        };

        for (index, record) in chunk.records.iter().enumerate() {
            if record.data.is_empty() {
                continue;
            }

            let sprite = Sprite::resolve(&catalogue, 0, record.data)
                .unwrap_or_else(|e| panic!("{name} tile {index}: {e}"));

            match (&sprite.colour, sprite.source) {
                (Colour::Indexed { .. }, PaletteSource::Embedded) => embedded += 1,
                (Colour::Indexed { .. }, _) => indexed += 1,
                (Colour::Direct { .. }, _) => direct += 1,
            }
        }

        // `shade_stand` names palette 1: the chunk's first record.
        let stand = catalogue.archives[0]
            .1
            .chunk_by_tag(b"TILE")
            .and_then(|c| c.records.iter().find(|r| r.name() == "shade_stand"))
            .expect("the mod's stand tile");
        let sprite = Sprite::resolve(&catalogue, 0, stand.data).expect("resolves");
        let pal1 = catalogue
            .find(b"SPLT", FourCc::from_word(1))
            .expect("palette 1");
        let want = payload::Palette::parse(catalogue.record(b"SPLT", pal1).unwrap().data).unwrap();
        let Colour::Indexed { palette, .. } = &sprite.colour else {
            panic!("indexed");
        };

        assert_eq!(pal1.record, 0, "stored first");
        assert_eq!(palette.colours, want.colours);
    }

    assert_eq!((indexed, embedded, direct), (4 + 5, 1, 1));
}

/// The mod ships sprite records, by tag and name, the base lacks; its
/// copies of base tiles keep the base's names, so are not new.
/// **Inferred**: that a mod's copies keep the stock names.
#[test]
fn mods_ship_content_the_install_does_not() {
    let shipped = contents(&archives::base());
    let modded = contents(&mod_archives());
    let new: HashSet<(FourCc, String)> = modded
        .difference(&shipped)
        .filter(|(tag, _)| matches!(tag.as_bytes(), b"IMAG" | b"TILE"))
        .cloned()
        .collect();
    let tile = |name: &str| (FourCc::new(b"TILE"), name.to_owned());

    for name in archives::own_tile_names() {
        assert!(new.contains(&tile(name)), "{name}");
    }

    for name in archives::copied_tile_names() {
        assert!(!new.contains(&tile(name)), "{name} is a copy");
    }

    let images = new
        .iter()
        .filter(|(tag, _)| tag.as_bytes() == b"IMAG")
        .count();

    assert_eq!(images, archives::MOD_IMAGES.len());
}

/// The mod composed over the base: each frame of its `IMAG`s names a
/// full tile of its own archive, so its empty `TILE` records are padding
/// none reaches. No id lookup lands on an empty record, no `IMAG` of the
/// mod's shares a base id, and its low tiles are the base's at the base's
/// positions (`docs/formats/README.md`, no archive is sparse).
#[test]
fn a_mod_names_no_empty_record() {
    let base = archives::base();
    let composed = archives::composed();
    let catalogue = Catalogue::build(&composed).expect("the archives");
    let first = base.len();
    let (mut images, mut frames) = (0, 0);

    for (at, record) in catalogue
        .tagged(b"IMAG")
        .filter(|(at, _)| at.archive >= first)
    {
        assert_eq!(
            catalogue.all(b"IMAG", record.id).len(),
            1,
            "{} shared",
            record.id
        );

        for frame in imagery::sets(&catalogue, at)
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
                matches!(Sprite::at(&catalogue, archive, index), Some(Ok(_))),
                "{} tile {index}",
                record.id
            );
            frames += 1;
        }

        images += 1;
    }

    assert_eq!(images, archives::MOD_IMAGES.len());
    assert_eq!(frames, archives::MOD_FRAMES);

    // Every record an id finds is full.
    for (_, archive) in &catalogue.archives {
        for chunk in archive
            .chunks
            .iter()
            .filter(|c| c.flag == cam::ChunkFlag::Clear)
        {
            for record in &chunk.records {
                let found = catalogue.find(&chunk.tag.0, record.id).expect("indexed");

                assert_ne!(catalogue.record(&chunk.tag.0, found).unwrap().data.len(), 0);
            }
        }
    }

    // The padding is there, and the low tiles copy the base's in place.
    let tiles = |name: &str| {
        let (_, bytes) = composed.iter().find(|(n, _)| n == name).expect(name);
        let archive = cam::Archive::parse(bytes).expect("readable");

        archive
            .chunk_by_tag(b"TILE")
            .expect("tiles")
            .records
            .iter()
            .map(|r| r.data.to_vec())
            .collect::<Vec<_>>()
    };
    let stock = tiles("base_art.cam");
    let modded = tiles("mod_art.cam");
    let copies = modded
        .iter()
        .zip(&stock)
        .filter(|(m, s)| !m.is_empty() && m == s)
        .count();

    assert_eq!(
        modded.iter().filter(|t| t.is_empty()).count(),
        archives::PADDING
    );
    assert_eq!(copies, archives::COPIED);
}

/// A mod hero clones a base unit's `IMAG` set ids and swaps the art: the
/// engine knows the interface pictures by id alone, so a new hero must
/// reuse the stock ones (`docs/formats/art.md`, the three `Interface`
/// sets).
#[test]
fn a_mod_hero_uses_the_stock_interface_ids() {
    let composed = archives::composed();
    let catalogue = Catalogue::build(&composed).expect("the archives");
    let ids = |art: FourCc| -> Vec<u16> {
        let at = catalogue.find(b"IMAG", art).expect("the art");

        imagery::sets(&catalogue, at)
            .expect("decodes")
            .iter()
            .map(|set| set_number(set.id))
            .collect()
    };
    let held = |art: FourCc, set: u16| {
        Sprite::in_set(&catalogue, art, u32::from(set), (0, 0))
            .expect("the picture")
            .expect("decodes")
    };

    assert_eq!(ids(archives::HERO_ART), ids(archives::BASE_HERO_ART));

    for (set, size) in [
        (INTERFACE_PORTRAIT, archives::PORTRAIT),
        (INTERFACE_PANEL, archives::PANEL),
        (INTERFACE_ICON, archives::ICON),
    ] {
        let mine = held(archives::HERO_ART, set);
        let stock = held(archives::BASE_HERO_ART, set);
        let fill = |s: &Sprite| match &s.colour {
            Colour::Indexed { indices, .. } => indices.iter().copied().max(),
            Colour::Direct { .. } => None,
        };

        assert_eq!((mine.width, mine.height), size, "set {set}");
        assert_eq!((stock.width, stock.height), size, "set {set}");
        assert_eq!(fill(&stock), Some(archives::BASE_FILL), "set {set}");
        assert_ne!(fill(&mine), fill(&stock), "set {set}: the art is swapped");
    }

    // The hero's description names the cloned art.
    let hero = Unit::find(&catalogue, FourCc::new(b"ZMH1"))
        .expect("the hero")
        .expect("reads");

    let art = hero.art.expect("art");

    assert_eq!(art.id, archives::HERO_ART);
    assert_eq!(art.at, catalogue.find(b"IMAG", archives::HERO_ART));
}

/// [`deps::Expr::parse`] balances the stack on a mod's `BDEP` as on the
/// base's. The mod's table replaces the base's whole: last loaded wins,
/// no merging (`docs/formats/README.md`).
#[test]
fn a_mods_dependency_table_parses() {
    let composed = archives::composed();
    let counts: Vec<(usize, usize)> = [archives::BASE_BDEP, archives::MOD_BDEP]
        .iter()
        .map(|text| {
            let rules = deps::buildings(text).expect("balanced");

            (
                rules.len(),
                rules.iter().filter(|r| r.requires.is_some()).count(),
            )
        })
        .collect();

    // (rules, conditional): the mod's adds one conditional rule.
    assert_eq!(counts, vec![(4, 3), (5, 4)]);

    let catalogue = Catalogue::build(&composed).expect("the archives");
    let at = catalogue.find(b"DATA", archives::BDEP).expect("a table");
    let (name, _) = &composed[at.archive];

    assert_eq!(name, "mod_misc.cam");
    assert_eq!(catalogue.all(b"DATA", archives::BDEP).len(), 2);
    assert_eq!(
        text_of(&composed[at.archive].1, archives::BDEP),
        archives::MOD_BDEP
    );
}

/// Every id in the mod's rules is a base `DUNT` but its guild, which only
/// the mod's XML defines: the mod ships no `DUNT` archive, and HD makes
/// the guild from the XML at load.
#[test]
fn a_mod_rule_names_a_building_its_xml_defines() {
    let base = archives::base();
    let base_catalogue = Catalogue::build(&base).expect("the base");
    let rules = deps::buildings(archives::MOD_BDEP).expect("balanced");
    let named: BTreeSet<FourCc> = rules
        .iter()
        .flat_map(|r| std::iter::once(r.id).chain(r.requires.iter().flat_map(deps::Expr::ids)))
        .collect();
    let unresolved: BTreeSet<FourCc> = named
        .iter()
        .copied()
        .filter(|id| Unit::find(&base_catalogue, *id).is_none())
        .collect();

    assert_eq!(unresolved, BTreeSet::from([archives::CRYPT]));

    // The mod's own archives hold no `DUNT`; its descriptions make one.
    let cams = archives::mod_cams();
    let mod_catalogue = Catalogue::build(&cams).expect("the mod");

    assert_eq!(mod_catalogue.tagged(b"DUNT").count(), 0);

    let composed = archives::composed();
    let catalogue = Catalogue::build(&composed).expect("the archives");

    for id in named {
        assert!(Unit::find(&catalogue, id).is_some(), "{id}");
    }
}

/// The made archive, loaded last, wins its ids: the replaced unit's stats
/// are the mod's, a field its XML leaves out is zero, not the base's
/// (`data::desc::xml`), and a building names, by name, an upgrade the
/// mod defines further down.
#[test]
fn a_made_archive_wins_its_ids() {
    let composed = archives::composed();
    let catalogue = Catalogue::build(&composed).expect("the archives");
    let unit = |id: FourCc| {
        let at = catalogue.find(b"DUNT", id).expect("the unit");
        let record = catalogue.record(b"DUNT", at).expect("its record");
        let part = desc::Part::parse(record.data).expect("a tree");

        assert_eq!(at.archive, catalogue.archives.len() - 1, "{id}");
        assert_eq!(catalogue.all(b"DUNT", id).len(), 2, "{id}: the base's too");

        part
    };

    let sentry = unit(archives::SENTRY);
    let sentry = desc::Unit::parse(&sentry).expect("a unit");
    let character = sentry.character().expect("a character");

    assert_eq!(character.max_hp(), Some(24), "16 in the base");
    assert_eq!(character.strength(), Some(11), "9 in the base");
    assert_eq!(character.attack(), Some(40), "30 in the base");
    assert_eq!(character.sight_range(), Some(0), "7 in the base");
    assert!(!sentry.can_use(User::Monster));
    assert!(sentry.can_use(User::HumanPlayer), "its XML says so");

    let post = unit(archives::POST);
    let post = desc::Unit::parse(&post).expect("a unit");

    assert_eq!(
        post.building().expect("a building").upgrade_to(),
        Some(archives::POST_2)
    );
}

/// The mod's sound plays its own wave: the `DSND` its XML makes names
/// the `WAVE` its archive ships, 16-bit PCM.
#[test]
fn a_mods_sound_names_its_wave() {
    let composed = archives::composed();
    let catalogue = Catalogue::build(&composed).expect("the archives");
    let at = catalogue.find(b"DSND", archives::SOUND).expect("the sound");
    let part = desc::Part::parse(catalogue.record(b"DSND", at).unwrap().data).expect("a tree");
    let sound = desc::Sound::parse(&part).expect("a sound");
    let waves: Vec<Option<FourCc>> = sound.phases().map(|p| p.wave()).collect();

    assert_eq!(waves, vec![Some(archives::WAVE)]);

    let wave = catalogue.find(b"WAVE", archives::WAVE).expect("the wave");
    let pcm = payload::Pcm::parse(catalogue.record(b"WAVE", wave).unwrap().data).expect("PCM");
    let samples: Vec<i16> = pcm.samples().collect();

    assert_eq!(composed[wave.archive].0, "mod_sound.cam");
    assert_eq!((pcm.channels, pcm.rate), (1, archives::WAVE_RATE));
    assert_eq!(samples, archives::WAVE_SAMPLES);
}
