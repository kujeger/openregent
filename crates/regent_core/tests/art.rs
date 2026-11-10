//! `TILE` and `IMAG` fields, checked against the corpus.

mod common;

use std::collections::{BTreeMap, BTreeSet};

use regent_core::content::imagery;
use regent_core::content::imagery::FrameSource;
use regent_core::data::imag::{self, FrameRef, Image};
use regent_core::data::tile::{self, Mode, PixelFormat, Tile};
use regent_core::data::{FourCc, cam};
use regent_core::sim::movement::action::frame_ms;
use regent_core::view::raster::shade;
use regent_core::view::sprite::Colour;
use regent_core::view::{Ink, Shade, Sprite};

/// Every `(archive name, TILE payload)` under `root`.
fn tiles(root: &std::path::Path) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();

    for path in common::archives(root) {
        let bytes = std::fs::read(&path).unwrap();
        let Ok(archive) = cam::Archive::parse(&bytes) else {
            continue;
        };
        let name = path.file_name().unwrap().to_string_lossy().into_owned();

        for (chunk, record) in archive.records() {
            if chunk.tag == b"TILE" && !record.data.is_empty() {
                out.push((name.clone(), record.data.to_vec()));
            }
        }
    }

    out
}

/// Version 1 is drawn in mode 0 only, and ships in no other; version 2 holds
/// its holes as 255.
#[test]
fn a_tiles_mode_is_one_its_version_can_draw() {
    let mut modes: BTreeMap<u16, BTreeSet<u16>> = BTreeMap::new();
    let (mut keyed, mut holed) = (0, 0);

    for (_, payload) in tiles(&common::hd()) {
        let Ok(t) = Tile::parse(&payload) else {
            continue;
        };

        modes
            .entry(t.version())
            .or_default()
            .insert(t.mode().word());

        if t.keyed() {
            keyed += 1;

            let mut pixels = t.indices().into_iter().flatten().flatten();
            holed += usize::from(pixels.any(|p| p == shade::TRANSPARENT));
        }
    }

    assert_eq!(
        modes,
        BTreeMap::from([
            (1, BTreeSet::from([0])),
            (2, BTreeSet::from([0, 1, 7])),
            (3, BTreeSet::from([0, 1, 2, 5, 7, 8])),
        ])
    );
    assert_eq!((holed, keyed), (28, 50), "version 2 tiles holding 255");
}

/// Flags bit 5 marks an `SPLT` palette, exactly when none is embedded.
#[test]
fn an_indexed_tile_names_its_palette_one_way() {
    let mut seen: BTreeMap<(bool, bool, bool), usize> = BTreeMap::new();

    for (_, payload) in tiles(&common::hd().join("Data")) {
        let Ok(t) = Tile::parse(&payload) else {
            continue;
        };
        let splt = matches!(t.palette(), tile::PaletteRef::Splt(_));

        *seen
            .entry((
                t.format() == PixelFormat::Indexed,
                splt,
                t.flags() & tile::SPLT_PALETTE != 0,
            ))
            .or_default() += 1;
    }

    assert!(
        seen.keys()
            .all(|&(indexed, splt, flag)| flag == (indexed && splt)),
        "{seen:?}"
    );
}

/// The 22 frames asking for a mirror name the 22 tiles stored mirrored.
#[test]
fn a_mirrored_frame_names_a_tile_stored_mirrored() {
    let bytes = std::fs::read(common::hd().join("Data/maindata.cam")).unwrap();
    let archive = cam::Archive::parse(&bytes).unwrap();

    let stored: BTreeSet<usize> = archive
        .chunk_by_tag(b"TILE")
        .unwrap()
        .records
        .iter()
        .enumerate()
        .filter(|(_, r)| Tile::parse(r.data).is_ok_and(|t| t.mirrored()))
        .map(|(i, _)| i)
        .collect();

    let mut asked = BTreeSet::new();
    let mut frames = 0;

    for (chunk, record) in archive.records() {
        let Ok(image) = (chunk.tag == b"IMAG")
            .then(|| Image::parse(record.data))
            .transpose()
        else {
            continue;
        };

        for frame in image
            .iter()
            .flat_map(|i| &i.sets)
            .flat_map(|s| &s.slots)
            .flat_map(imag::Slot::frames)
            .filter(|f| f.reference & imag::MIRROR != 0)
        {
            frames += 1;

            if let FrameRef::Tile { index, .. } = frame.target() {
                asked.insert(usize::from(index));
            }
        }
    }

    assert_eq!((frames, stored.len()), (22, 22));
    assert_eq!(asked, stored);
}

/// HD's second-pass frames are fliers' shadows: every bit 24 frame sits
/// in a layer with flags bit 9, tiles of mode 1 or 7, and those layers
/// are 26 sets of 15 records, missiles and three fliers.
#[test]
fn every_second_pass_frame_is_a_fliers_shadow() {
    let mut layers = 0;
    let mut codes = BTreeSet::new();
    let mut frames = [0, 0];
    let mut strays = 0;
    let mut modes: BTreeMap<Mode, usize> = BTreeMap::new();

    for root in [common::hd().join("Data"), common::hd().join("DataMX")] {
        for path in common::archives(&root) {
            let bytes = std::fs::read(&path).unwrap();
            let Ok(archive) = cam::Archive::parse(&bytes) else {
                continue;
            };
            let tiles: Vec<&[u8]> = archive
                .chunk_by_tag(b"TILE")
                .map(|c| c.records.iter().map(|r| r.data).collect())
                .unwrap_or_default();

            for (chunk, record) in archive.records() {
                if chunk.tag != b"IMAG" {
                    continue;
                }
                let Ok(image) = Image::parse(record.data) else {
                    continue;
                };

                for set in &image.sets {
                    let second = set.passes().any(|p| p == imag::Pass::Second);

                    if second {
                        layers += 1;
                        codes.insert(record.id);
                    }

                    for slot in &set.slots {
                        let shadows = slot
                            .frames()
                            .enumerate()
                            .filter(|(_, f)| imag::Pass::of(f.reference) == imag::Pass::Second);

                        for (i, frame) in shadows {
                            let stream = i / usize::from(slot.frames.max(1));

                            frames[usize::from(stream > 0)] += 1;
                            strays += usize::from(!second);

                            if let FrameRef::Tile { index, .. } = frame.target() {
                                let tile = Tile::parse(tiles[usize::from(index)]).unwrap();

                                *modes.entry(tile.mode()).or_default() += 1;
                            }
                        }
                    }
                }
            }
        }
    }

    assert_eq!((layers, codes.len()), (26, 15));
    assert_eq!(frames, [2, 1591], "in stream 0, in later streams");
    assert_eq!(strays, 0, "a frame of a pass its layer has no part for");
    assert_eq!(
        modes.keys().copied().collect::<Vec<_>>(),
        [Mode::Shadow, Mode::Band]
    );
}

/// Flag bits come one at a time, and none names a 3D model.
#[test]
fn a_reference_carries_one_flag_at_most() {
    let mut flags: BTreeMap<u32, usize> = BTreeMap::new();

    for root in [common::hd().join("Data"), common::hd().join("DataMX")] {
        for path in common::archives(&root) {
            let bytes = std::fs::read(&path).unwrap();
            let Ok(archive) = cam::Archive::parse(&bytes) else {
                continue;
            };

            for (chunk, record) in archive.records() {
                if chunk.tag != b"IMAG" {
                    continue;
                }
                let Ok(image) = Image::parse(record.data) else {
                    continue;
                };

                for frame in image
                    .sets
                    .iter()
                    .flat_map(|s| &s.slots)
                    .flat_map(imag::Slot::frames)
                {
                    *flags.entry(frame.reference >> 24).or_default() += 1;
                }
            }
        }
    }

    assert!(
        flags.keys().all(|f| f.count_ones() <= 1),
        "two flags combine: {flags:?}"
    );
    assert!(
        !flags.contains_key(&(imag::MODEL >> 24)),
        "a frame names a CY3D model"
    );
}

/// A slot's trailer is its playback and its effect lists, and nothing else.
#[test]
fn a_slots_trailer_is_its_playback_and_effects() {
    let mut playbacks: BTreeMap<u32, usize> = BTreeMap::new();
    let mut images: BTreeSet<FourCc> = BTreeSet::new();
    let (mut slots, mut slack) = (0, 0);

    for root in [common::hd().join("Data"), common::hd().join("DataMX")] {
        for path in common::archives(&root) {
            let bytes = std::fs::read(&path).unwrap();
            let Ok(archive) = cam::Archive::parse(&bytes) else {
                continue;
            };

            for (chunk, record) in archive.records() {
                if chunk.tag != b"IMAG" {
                    continue;
                }
                let Ok(image) = Image::parse(record.data) else {
                    continue;
                };

                for slot in image.sets.iter().flat_map(|s| &s.slots) {
                    // The loader seeks to each slot, so a version 2 slot's
                    // leftover bytes are slack nothing reads.
                    if image.version == imag::Version::Two {
                        slack += usize::from(!slot.trailer().is_empty());
                        continue;
                    }

                    let effects = slot.effects();
                    let lists = slot.effect_mask.count_ones() as usize;
                    let playback = slot.playback();
                    let want = playback.map_or(0, |_| 20) + 4 * lists + 32 * effects.len();

                    slots += 1;

                    assert_eq!(slot.trailer().len(), want, "{} slot trailer", record.id);

                    if let Some(p) = playback {
                        *playbacks.entry(p.kind).or_default() += 1;

                        assert_eq!(p.rest, [0; 2]);
                        assert!(p.value <= 500, "{p:?}");
                    }

                    images.extend(effects.iter().filter_map(imag::Effect::image));
                    assert!(effects.iter().all(|e| e.set() == 0x40));
                }
            }
        }
    }

    println!("  {slots} slots, {slack} version 2 slots with slack");

    assert_eq!(slack, 7, "version 2 slots with slack");
    assert_eq!(
        playbacks.keys().copied().collect::<Vec<_>>(),
        [0, 1, 2, 4],
        "{playbacks:?}"
    );
    assert_eq!(
        images,
        BTreeSet::from([b"FXa1", b"XR46", b"XR47"].map(FourCc::new))
    );
}

/// A set's header `+0x08` names the particle system a part playing it
/// carries (imag set v4, `v5`: loaded set; `UnitIMG`
/// makes one per part). HD: 947 sets. The palace's `Die-7` smokes on its
/// four fires, not its walls.
#[test]
fn a_set_names_the_particle_system_its_parts_carry() {
    let mut named: BTreeMap<FourCc, usize> = BTreeMap::new();
    let mut palace = Vec::new();

    for root in [common::hd().join("Data"), common::hd().join("DataMX")] {
        for path in common::archives(&root) {
            let bytes = std::fs::read(&path).unwrap();
            let Ok(archive) = cam::Archive::parse(&bytes) else {
                continue;
            };

            for (chunk, record) in archive.records() {
                let Ok(image) = Image::parse(record.data) else {
                    continue;
                };

                if chunk.tag != b"IMAG" {
                    continue;
                }

                for set in &image.sets {
                    let Some(code) = image.particle_system(set) else {
                        continue;
                    };

                    *named.entry(code).or_default() += 1;

                    if record.id == FourCc::new(b"ABJ1") && set.id & 0xff_ffff == 0x66 {
                        palace.push((set.variant(), code));
                    }
                }
            }
        }
    }

    let smoke = FourCc::new(b"XLSL");

    assert_eq!(named.values().sum::<usize>(), 947);
    assert_eq!(named.get(&smoke), Some(&763));
    assert_eq!(named.get(&FourCc::new(b"XLSH")), Some(&166));
    assert_eq!(named.get(&FourCc::new(b"XL23")), Some(&2));
    assert_eq!(palace, [1, 2, 3, 4].map(|v| (v, smoke)));
}

/// Runs mixing the shadow band with colours are mostly in mode 0 tiles,
/// where the band is colours.
#[test]
fn a_mixed_run_is_mostly_a_plain_tile() {
    /// The world archives, where the band should not mix.
    const WORLD: [&str; 2] = ["maindata.cam", "mx_maindata.cam"];
    /// The darken half of the band.
    const SHADOW_LAST: u8 = 250;

    let shadow = shade::CONTROL_FIRST..=SHADOW_LAST;
    let mut mixed: BTreeMap<Mode, usize> = BTreeMap::new();

    for (archive, payload) in tiles(&common::hd()) {
        let Ok(t) = Tile::parse(&payload) else {
            continue;
        };
        let Some(rows) = t.runs() else {
            continue;
        };
        if !WORLD.contains(&archive.as_str()) || t.format() != PixelFormat::Indexed {
            continue;
        }

        for run in rows.iter().flatten() {
            let band = run.pixels.iter().any(|p| shadow.contains(p));
            let colour = run
                .pixels
                .iter()
                .any(|&p| p != 0 && p < shade::CONTROL_FIRST);

            if band && colour {
                *mixed.entry(t.mode()).or_default() += 1;
            }
        }
    }

    assert_eq!(
        mixed,
        BTreeMap::from([(Mode::Plain, 283), (Mode::Shadow, 9)])
    );
}

/// A set's frame time is its playback word 2 as an order plays it: most
/// sets have none and show 100 ms frames; a negative word runs a frame a
/// game frame.
#[test]
fn a_sets_frame_time_is_its_playback_as_an_order_plays_it() {
    let mut times: BTreeMap<u32, usize> = BTreeMap::new();
    let mut named: BTreeMap<(FourCc, u32), u32> = BTreeMap::new();
    let wanted = [
        (FourCc(*b"AVj1"), 0x01),
        (FourCc(*b"AVj1"), 0x08),
        (FourCc(*b"AVj1"), 0x40),
        (FourCc(*b"PRc1"), 0x50),
    ];

    for root in [common::hd().join("Data"), common::hd().join("DataMX")] {
        for path in common::archives(&root) {
            let bytes = std::fs::read(&path).unwrap();
            let Ok(archive) = cam::Archive::parse(&bytes) else {
                continue;
            };

            for (chunk, record) in archive.records() {
                if chunk.tag != b"IMAG" {
                    continue;
                }
                let Ok(image) = Image::parse(record.data) else {
                    continue;
                };

                for set in &image.sets {
                    let ms = frame_ms(set.playback().map_or(0, |p| p.value));

                    *times.entry(ms).or_default() += 1;

                    if wanted.contains(&(record.id, set.id)) {
                        named.insert((record.id, set.id), ms);
                    }
                }
            }
        }
    }

    // The peasant's Walk 100, Stand none, Special 150; Build -50.
    assert_eq!(
        named.values().copied().collect::<Vec<_>>(),
        [100, 100, 150, 50]
    );
    assert_eq!(
        times,
        BTreeMap::from([
            (50, 80),
            (100, 5116),
            (150, 93),
            (200, 24),
            (250, 4),
            (300, 45)
        ])
    );
}

/// Frame 0 of `record`'s interface `set` in HD's archives.
fn interface_sprite(record: [u8; 4], set: u32) -> Sprite {
    let library = common::library();
    let catalogue = library.catalogue().expect("a catalogue");
    let at = catalogue.find(b"IMAG", FourCc(record)).expect("the record");
    let sets = imagery::sets(&catalogue, at).expect("its sets");
    let found = sets.iter().find(|a| a.id == set).expect("the set");

    match found.poses[0].frames[0] {
        FrameSource::Tile { archive, index, .. } => Sprite::at(&catalogue, archive, index)
            .expect("the tile")
            .expect("it resolves"),
        _ => panic!("not a tile"),
    }
}

/// The first pixel of `sprite` holding `index`.
fn first(sprite: &Sprite, index: u8) -> usize {
    match &sprite.colour {
        Colour::Indexed { indices, .. } => (0..indices.len())
            .find(|&i| sprite.covered[i] && indices[i] == index)
            .expect("a pixel of the index"),
        Colour::Direct { .. } => panic!("not indexed"),
    }
}

/// An interface tile's draw mode says what 247–254 do, not its palette:
/// the building panel's frame `INBg` 1025 is mode 1, so its 251–254
/// darken, the name scroll's shadow among them; `INBg` 1018 is mode 0, so
/// its 247 is a colour.
#[test]
fn an_interface_tiles_mode_says_what_its_band_does() {
    let frame = interface_sprite(*b"INBg", 1025);
    let plain = interface_sprite(*b"INBg", 1018);

    assert_eq!(frame.mode, Mode::Shadow);
    assert_eq!(
        frame.as_drawn(first(&frame, 247)),
        Ink::Shade(Shade::Keep(50))
    );
    assert_eq!(
        frame.as_drawn(first(&frame, 254)),
        Ink::Shade(Shade::Keep(92))
    );

    assert_eq!(plain.mode, Mode::Plain);
    assert!(matches!(plain.as_drawn(first(&plain, 247)), Ink::Colour(_)));
}

/// Shadow pixels (shading 247 and up) and those of them 251 and up in
/// the first frame of each stream of every layer of `unit`'s `Active` set,
/// else its `Inactive`; and the tiles' modes.
fn standing_shadow(
    catalogue: &regent_core::data::catalogue::Catalogue<'_>,
    unit: &regent_core::content::unit::Unit<'_>,
) -> (usize, usize, BTreeSet<Mode>) {
    let kind = [imag::SetKind::Active, imag::SetKind::Inactive]
        .into_iter()
        .find(|k| unit.animation(*k).is_some());
    let mut counts = (0, 0, BTreeSet::new());

    for layer in kind.into_iter().flat_map(|k| unit.layers(k)) {
        let Some(pose) = layer.poses.first() else {
            continue;
        };

        for stream in 0..pose.streams {
            let Some(Some(Ok(sprite))) = pose
                .stream(stream)
                .first()
                .and_then(FrameSource::tile)
                .map(|(archive, index)| Sprite::at(catalogue, archive, index))
            else {
                continue;
            };
            let Colour::Indexed { indices, .. } = &sprite.colour else {
                continue;
            };

            counts.2.insert(sprite.mode);

            for (i, index) in indices.iter().enumerate() {
                if matches!(sprite.as_drawn(i), Ink::Shade(Shade::Keep(_))) {
                    counts.0 += 1;
                    counts.1 += usize::from(*index > 250);
                }
            }
        }
    }

    counts
}

/// A building's painted shadow is indices 247 to 250 alone, in a mode 1
/// or 7 tile: no standing building darkens with 251 to 254, though mode
/// 1 would. Where the art paints shade in ordinary colours (the
/// Palace's west walls) it is no shadow to the draw: the Palace's 5,264
/// shadow pixels are the two lobes its towers throw on the grass and a
/// dithered rim (`regent_cli shadows --record ABJ1`).
#[test]
fn a_buildings_shadow_is_indices_247_to_250() {
    let library = common::library();
    let catalogue = library.catalogue().expect("a catalogue");
    let mut buildings = 0;

    for (at, _) in catalogue.tagged(b"DUNT") {
        let Ok(unit) = regent_core::content::unit::Unit::at(&catalogue, at) else {
            continue;
        };

        if unit.kind != Some(regent_core::data::desc::UnitKind::Building) {
            continue;
        }

        let (shadow, high, modes) = standing_shadow(&catalogue, &unit);

        buildings += usize::from(shadow > 0);
        assert_eq!(high, 0, "{}: shades with 251 to 254", unit.id);
        assert!(
            shadow == 0 || modes.iter().any(|m| matches!(m, Mode::Shadow | Mode::Band)),
            "{}",
            unit.id
        );
    }

    assert!(buildings > 100, "{buildings} buildings with a shadow");

    let palace = regent_core::content::unit::Unit::find(&catalogue, FourCc(*b"ABJ1"))
        .expect("the Palace")
        .expect("it reads");
    let guardhouse = regent_core::content::unit::Unit::find(&catalogue, FourCc(*b"ABE1"))
        .expect("the Guardhouse")
        .expect("it reads");

    let (shadow, _, modes) = standing_shadow(&catalogue, &palace);
    assert_eq!(shadow, 5_264);
    assert_eq!(modes, BTreeSet::from([Mode::Shadow]));

    let (shadow, _, modes) = standing_shadow(&catalogue, &guardhouse);
    assert_eq!(shadow, 1_771);
    assert_eq!(modes, BTreeSet::from([Mode::Band]));
}

/// The rows about the anchor of the aloft darkening pixels of `id`'s
/// frame `frame` of its `kind`'s first layer, and how many of the rest.
fn aloft_of(
    catalogue: &regent_core::data::catalogue::Catalogue<'_>,
    id: FourCc,
    kind: imag::SetKind,
    frame: usize,
) -> (Vec<i32>, usize) {
    let unit = regent_core::content::unit::Unit::find(catalogue, id)
        .expect("the unit")
        .expect("it reads");
    let stands = regent_core::sim::footprint::of_unit(catalogue, &unit)
        .expect("reads")
        .expect("it stands");
    let layer = unit.layers(kind).next().expect("the set");
    let (archive, index) = layer.poses[0].stream(0)[frame].tile().expect("a tile");
    let sprite = Sprite::at(catalogue, archive, index)
        .expect("in range")
        .expect("decodes");
    let width = usize::from(sprite.width);
    let top = i32::from(sprite.anchor.1);
    let (aloft, ground): (Vec<usize>, Vec<usize>) = (0..sprite.covered.len())
        .filter(|i| matches!(sprite.as_drawn(*i), Ink::Shade(Shade::Keep(_))))
        .partition(|i| sprite.aloft(*i, stands, (0, 0)));
    let rows = aloft
        .iter()
        .map(|i| i32::try_from(i / width).expect("small") - top)
        .collect();

    (rows, ground.len())
}

/// The level 2 Guardhouse's flag is painted in shadow indices, in the
/// tile that holds its ground shadow, one draw mode: only where it lies
/// tells them apart. All six `Active` frames' flags stay, above row -90;
/// the 3,077 or so pixels on the ground do not. The other buildings with
/// a shipped model paint nothing up there.
#[test]
fn a_guardhouses_flag_is_aloft_and_its_shadow_not() {
    let library = common::library();
    let catalogue = library.catalogue().expect("a catalogue");

    for frame in 0..6 {
        let (rows, ground) = aloft_of(&catalogue, FourCc(*b"ABE2"), imag::SetKind::Active, frame);

        assert!((94..=98).contains(&rows.len()), "frame {frame}: {rows:?}");
        assert!(rows.iter().all(|y| *y < -90), "frame {frame}: {rows:?}");
        assert!((3_077..=3_086).contains(&ground), "frame {frame}: {ground}");
    }

    let plain = [
        (b"ABE2", imag::SetKind::Inactive),
        (b"ABE1", imag::SetKind::Inactive),
        (b"ABE1", imag::SetKind::Active),
        (b"ABV1", imag::SetKind::Inactive),
        (b"ABV1", imag::SetKind::Active),
    ];

    for (code, kind) in plain {
        let (rows, _) = aloft_of(&catalogue, FourCc(*code), kind, 0);

        assert_eq!(rows, [] as [i32; 0], "{code:?} {kind:?}");
    }
}

/// The Magic Bazaar's last damage stage (`ABl1` `Die-6`, set `0x65`)
/// carries five `FXa1` fires: (fire slot, x, y), x right, y down, word 0
/// used and fixed with its index in the top byte.
#[test]
fn the_bazaars_last_damage_stage_carries_five_fires() {
    let library = common::library();
    let catalogue = library.catalogue().expect("a catalogue");
    let at = catalogue
        .find(b"IMAG", FourCc(*b"ABl1"))
        .expect("the Bazaar");
    let sets = imagery::sets(&catalogue, at).expect("its sets");
    let stage = sets.iter().find(|a| a.id == 0x65).expect("Die-6");
    let effects = &stage.poses[0].effects;
    let fires: Vec<(u32, (i32, i32))> = effects.iter().map(|e| (e.slot(), e.offset())).collect();

    assert_eq!(
        fires,
        [
            (1, (-70, -4)),
            (6, (53, -40)),
            (7, (74, 57)),
            (22, (-36, -8)),
            (19, (12, 5))
        ]
    );

    for (i, e) in (0..).zip(effects) {
        assert_eq!(e.words[0], 5 | i << 24);
        assert_eq!(e.index(), i);
        assert!(e.enabled() && e.fixed_slot());
        assert_eq!(e.image(), Some(FourCc(*b"FXa1")));
    }
}

/// HD's slot effects: 25 records, 415 entries, each resolving to a part:
/// an `FXa1` fire of 23, an `XR46` trail of 32
/// directions, or the one `XR47` dust cloud, which sits on a `Crumble`
/// slot of 21 frames, the 21-frame cloud's length, bar `BBx1`'s second
/// slot of 20.
#[test]
fn every_slot_effect_resolves_to_a_part() {
    let library = common::library();
    let catalogue = library.catalogue().expect("a catalogue");
    let slots_of = BTreeMap::from([(*b"FXa1", 23), (*b"XR46", 32), (*b"XR47", 1)]);
    let mut records: BTreeSet<FourCc> = BTreeSet::new();
    let mut entries = 0;
    let mut short = Vec::new();

    for (_, archive) in &catalogue.archives {
        for (chunk, record) in archive.records() {
            if chunk.tag != b"IMAG" {
                continue;
            }
            let Ok(image) = Image::parse(record.data) else {
                continue;
            };

            for set in &image.sets {
                let carriers = set.slots.iter().enumerate();

                for (nth, slot) in carriers.filter(|(_, s)| !s.effects().is_empty()) {
                    records.insert(record.id);
                    entries += slot.effects().len();

                    for e in slot.effects() {
                        let code = e.image().expect("an image").0;
                        let drawn = imagery::effect(&catalogue, e).expect("it resolves");

                        assert_eq!(drawn.poses.len(), slots_of[&code], "{}", record.id);
                        assert!(e.slot() < u32::try_from(slots_of[&code]).unwrap());

                        if code == *b"XR47" {
                            assert_eq!(set.kind(), imag::SetKind::Crumble, "{}", record.id);
                            assert_eq!(drawn.poses[0].per_stream, 21);

                            if slot.frames != 21 {
                                short.push((record.id.to_string(), nth, slot.frames));
                            }
                        }
                    }
                }
            }
        }
    }

    assert_eq!(records.len(), 25);
    assert_eq!(entries, 415);
    assert_eq!(short, [("BBx1".to_string(), 1, 20)]);
}
