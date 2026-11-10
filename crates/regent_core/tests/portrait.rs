//! The selection panel and ring ([`regent_core::view::panel::portrait`],
//! [`regent_core::view::map::ring`]), against HD's files.

mod common;

use common::read;
use std::collections::{BTreeMap, HashMap};

use regent_core::content::imagery::FrameSource;
use regent_core::content::unit::Unit;
use regent_core::data::cam;
use regent_core::data::catalogue::Catalogue;
use regent_core::data::catalogue::Library;
use regent_core::data::desc::UnitKind;
use regent_core::data::fourcc::FourCc;
use regent_core::data::imag::Blit;
use regent_core::data::imag::Image;
use regent_core::data::imag::{INTERFACE_ICON, INTERFACE_PANEL, INTERFACE_PORTRAIT, SetKind};
use regent_core::data::smnu::{self, Panel};
use regent_core::data::text::Text;
use regent_core::data::tile::{Mode, Tile};
use regent_core::sim::clearing;
use regent_core::sim::world;
use regent_core::sim::world::kind::Kind;
use regent_core::view::map::hit::{self, Layer};
use regent_core::view::map::ring::band::{self, Band};
use regent_core::view::map::ring::{self, Look};
use regent_core::view::map::{bar, label};
use regent_core::view::panel::portrait::details::{self, Board, Coffer, Posted, Says};
use regent_core::view::panel::portrait::{self, Crown, Health};
use regent_core::view::panel::portrait::{enchant, gear};
use regent_core::view::panel::upper;
use regent_core::view::{Ink, Shade};

fn catalogue_of(library: &regent_core::data::catalogue::Library) -> Catalogue<'_> {
    library.catalogue().expect("a catalogue")
}

fn unit<'a>(catalogue: &Catalogue<'a>, code: [u8; 4]) -> Unit<'a> {
    Unit::find(catalogue, FourCc(code))
        .expect("the unit")
        .expect("it resolves")
}

/// The names read [`Unit::descriptor`], not the whole unit: it must be
/// the same descriptor, for every `DUNT`.
#[test]
fn a_descriptor_alone_is_the_units_own() {
    let library = common::library();
    let catalogue = catalogue_of(library);
    let mut seen = 0;

    for (_, record) in catalogue.tagged(b"DUNT") {
        let unit = unit(&catalogue, record.id.0);
        let desc = Unit::descriptor(&catalogue, record.id)
            .expect("the record")
            .expect("it parses");

        assert_eq!(
            format!("{desc:?}"),
            format!("{:?}", unit.desc),
            "{}",
            unit.id
        );
        seen += 1;
    }

    assert!(seen > 300, "{seen}");
}

/// Every set 1000 is a 100 by 100 picture. Every character that walks
/// has one, so every hero, henchman and monster; the units without are
/// spell effects, markers, signs, and four with no art.
#[test]
fn every_portrait_is_a_hundred_pixels_square() {
    let library = common::library();
    let catalogue = catalogue_of(library);
    let mut kinds: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut walkers_without = Vec::new();

    for (at, _) in catalogue.tagged(b"DUNT") {
        let unit = Unit::at(&catalogue, at).expect("it resolves");
        let Some(kind) = unit.kind else { continue };
        let shown = kinds.entry(format!("{kind:?}")).or_default();
        let picture = portrait::portrait(&catalogue, &unit).map(|p| p.expect("it decodes"));

        shown.1 += 1;

        if let Some(picture) = &picture {
            assert_eq!((picture.width, picture.height), (100, 100), "{}", unit.id);
            shown.0 += 1;
        }

        let walks = unit.animation(SetKind::Walk).is_some();

        if picture.is_none() && walks && unit.kind == Some(UnitKind::Character) {
            walkers_without.push(unit.id);
        }
    }

    assert!(walkers_without.is_empty(), "{walkers_without:?}");
    assert_eq!(kinds["Building"], (108, 120));
    assert_eq!(kinds["Character"], (87, 111));
}

/// The hero panel's backdrop is 200 wide and the panel's height or a row
/// short, and every unit with one has a portrait too.
#[test]
fn a_heros_backdrop_is_the_panels_height() {
    let library = common::library();
    let catalogue = catalogue_of(library);
    let mut backed = 0;

    for (at, _) in catalogue.tagged(b"DUNT") {
        let unit = Unit::at(&catalogue, at).expect("it resolves");
        let Some(picture) = portrait::backdrop(&catalogue, &unit) else {
            continue;
        };
        let picture = picture.expect("it decodes");

        assert_eq!(picture.width, 200, "{}", unit.id);
        assert!((244..=245).contains(&picture.height), "{}", unit.id);
        assert!(unit.interface(INTERFACE_PORTRAIT).is_some(), "{}", unit.id);
        backed += 1;
    }

    assert!(
        unit(&catalogue, *b"AVF1")
            .interface(INTERFACE_PANEL)
            .is_some()
    );
    assert_eq!(backed, 31);
}

/// Groups of a panel's items, one per rectangle: a control and its keys.
fn controls(panel: &Panel) -> Vec<Vec<(i32, Vec<i32>)>> {
    let mut out: Vec<Vec<(i32, Vec<i32>)>> = Vec::new();

    for item in panel.items() {
        if item.key == smnu::RECT || out.is_empty() {
            out.push(Vec::new());
        }

        if let Some(group) = out.last_mut() {
            group.push((item.key, item.value.to_vec()));
        }
    }

    out
}

fn word(group: &[(i32, Vec<i32>)], key: i32) -> Option<i32> {
    group
        .iter()
        .find(|(k, _)| *k == key)
        .and_then(|(_, v)| v.first().copied())
}

/// The crown `APMK` puts in the selection panel's control, if it has an
/// `APMK`.
fn crown_of(archive: &cam::Archive<'_>) -> Option<FourCc> {
    let (_, main) = archive
        .records()
        .find(|(chunk, r)| chunk.tag.as_bytes() == b"SMNU" && r.id == FourCc::new(b"APMK"))?;
    let panel = Panel::parse(main.data).expect("it parses");
    let slot = controls(&panel)
        .into_iter()
        .find(|g| word(g, smnu::CONTROL) == Some(portrait::PANEL_CONTROL))?;
    let named = |crown: Crown| {
        let (record, set) = crown.art();

        word(&slot, smnu::ID) == Some(record.word().cast_signed())
            && word(&slot, smnu::SET) == Some(i32::from(set))
    };

    [Crown::Gold, Crown::Expansion]
        .into_iter()
        .find(|c| named(*c))
        .map(|c| c.art().0)
}

/// With nothing selected HD shows a crown: every screen size's main screen
/// but 800 by 600's names the gold one; the base game's text names it too,
/// the expansion's the purple one. Both are 202 by 245.
#[test]
fn the_empty_panel_is_a_crown() {
    let hd = common::hd();
    let mut sizes = BTreeMap::new();

    for entry in std::fs::read_dir(hd.join("Data"))
        .expect("HD Data/")
        .flatten()
    {
        let name = entry.file_name().to_string_lossy().into_owned();

        if name.starts_with("UIData_") {
            let bytes = read(&entry.path());
            let archive = cam::Archive::parse(&bytes).expect("a CYLB archive");

            sizes.insert(name, crown_of(&archive));
        }
    }

    let gold = Some(Crown::Gold.art().0);

    assert_eq!(sizes.len(), 13);
    assert_eq!(sizes.remove("UIData_800_600.dat"), Some(None));
    assert!(sizes.values().all(|c| *c == gold), "{sizes:?}");

    let base = read(&hd.join("Data/textdata.cam"));
    let expansion = read(&hd.join("DataMX/mx_textdata.cam"));

    assert_eq!(
        crown_of(&cam::Archive::parse(&base).expect("an archive")),
        gold
    );
    assert_eq!(
        crown_of(&cam::Archive::parse(&expansion).expect("an archive")),
        Some(Crown::Expansion.art().0)
    );

    let library = common::library();
    let catalogue = catalogue_of(library);

    for crown in [Crown::Gold, Crown::Expansion] {
        let picture = portrait::empty(&catalogue, crown)
            .expect("the crown")
            .expect("it decodes");

        assert_eq!((picture.width, picture.height), (202, 245), "{crown:?}");
    }
}

/// The hero panel puts the portrait control where its 100 by 100 picture
/// fits: `AP20`, control 8011 at (50, 28).
#[test]
fn the_hero_panel_lays_the_portrait_out_at_its_size() {
    let library = common::library();
    let catalogue = catalogue_of(library);
    let (_, record) = catalogue
        .tagged(b"SMNU")
        .find(|(_, r)| r.id == FourCc::new(b"AP20"))
        .expect("the hero panel");
    let panel = Panel::parse(record.data).expect("it parses");
    let control = controls(&panel)
        .into_iter()
        .find(|g| word(g, smnu::CONTROL) == Some(upper::id::PORTRAIT))
        .expect("the portrait's control");
    let rect = control
        .iter()
        .find(|(k, _)| *k == smnu::RECT)
        .expect("a rect");

    assert_eq!(rect.1, vec![50, 28, 100, 100]);
}

/// The warriors' guild's panel, `AP52`, holds every control its roster
/// rule fills (`view::panel::upper::war_roster`), and the rogues' guild's
/// the extortion button.
#[test]
fn the_guild_panels_hold_the_controls_their_rules_fill() {
    let library = common::library();
    let catalogue = catalogue_of(library);
    let text = Text::build(&catalogue);
    let ids = |panel: FourCc| {
        let layout = regent_core::view::menu::layout(&catalogue, &text, panel)
            .expect("the panel ships")
            .expect("it parses");

        layout
            .controls
            .iter()
            .filter_map(|c| c.id)
            .collect::<Vec<_>>()
    };
    let war = ids(FourCc::new(b"AP52"));
    let roster = [(Some(1), None), (None, Some(1)), (Some(1), Some(1))]
        .into_iter()
        .map(|(paladins, discord)| {
            upper::war_roster(world::offer::WarRoster {
                warriors: 1,
                paladins,
                discord,
            })
        });

    assert!(war.contains(&upper::id::WARRIORS));

    for r in roster {
        let controls = r
            .frame
            .into_iter()
            .chain(r.temples.iter().flat_map(|t| [t.icon, t.count]));

        for c in controls {
            assert!(war.contains(&c), "{c}");
        }
    }

    assert!(ids(upper::ROGUES_PANEL).contains(&upper::id::SECOND));

    // The outpost's hero count and HEROES button (`Outpost_Controller`).
    let outpost = ids(upper::OUTPOST_PANEL);
    let members = world::hire::Members {
        living: 1,
        dead: 0,
        spots: 6,
    };
    let housed = upper::housed(Some(upper::OUTPOST_PANEL), Some(members)).expect("a count");
    let controls = housed
        .frame
        .into_iter()
        .chain(housed.temples.iter().flat_map(|t| [t.icon, t.count]))
        .chain([upper::id::OUTPOST_HEROES]);

    for c in controls {
        assert!(outpost.contains(&c), "{c}");
    }
}

/// The outpost houses heroes: a guild of six (`DUNT ABm1`), on `MX08`.
/// The palace houses none.
#[test]
fn the_outpost_is_a_guild_of_six_and_the_palace_no_guild() {
    let library = common::library();
    let catalogue = catalogue_of(library);
    let outpost = unit(&catalogue, *b"ABm1");
    let b = outpost.desc.building().expect("a building");

    assert_eq!(b.dialog_id(), Some(upper::OUTPOST_PANEL));
    assert_eq!(b.max_guild_members(), Some(6));
    assert!(b.has(regent_core::data::desc::BuildingFlag::IsGuild));

    for code in [*b"ABJ1", *b"ABJ2", *b"ABJ3"] {
        let palace = unit(&catalogue, code);

        assert_eq!(
            palace.desc.building().and_then(|b| b.max_guild_members()),
            Some(0)
        );
    }
}

#[test]
fn hit_points_read_as_the_panel_writes_them() {
    let library = common::library();
    let catalogue = catalogue_of(library);
    let text = Text::build(&catalogue);

    assert_eq!(
        Health::of(51, 74).text(&text).as_deref(),
        Some("51 of 74 hp")
    );
    assert_eq!(
        Health::of(-2, 74).text(&text).as_deref(),
        Some("0 of 74 hp")
    );
}

/// Every Blacksmith names `gear::FORGE_PANEL`, which holds the weapon's
/// `INBw` and the armour's `INBa` picture and a "+n" box beside each.
#[test]
fn the_forge_panel_holds_its_gear() {
    let library = common::library();
    let catalogue = catalogue_of(library);

    for code in [*b"ABC1", *b"ABC2", *b"ABC3"] {
        let dialog = unit(&catalogue, code)
            .desc
            .building()
            .and_then(|b| b.dialog_id());

        assert_eq!(dialog, Some(gear::FORGE_PANEL));
    }

    let (_, record) = catalogue
        .tagged(b"SMNU")
        .find(|(_, r)| r.id == gear::FORGE_PANEL)
        .expect("the forge panel");
    let panel = Panel::parse(record.data).expect("it parses");
    let groups = controls(&panel);
    let control = |id| {
        groups
            .iter()
            .find(|g| word(g, smnu::CONTROL) == Some(id))
            .map(|g| (word(g, smnu::ID), word(g, smnu::SET)))
    };
    let art = |record: FourCc, set| Some((Some(record.word().cast_signed()), Some(set)));

    assert_eq!(
        control(gear::FORGE_WEAPON.0),
        art(gear::WEAPON_RECORD, 1012)
    );
    assert_eq!(
        control(gear::FORGE_ARMOUR.0),
        art(gear::ARMOUR_RECORD, 1002)
    );
    assert!(control(gear::FORGE_WEAPON.1).is_some());
    assert!(control(gear::FORGE_ARMOUR.1).is_some());
}

/// Every wizards' guild names `gear::STAFF_PANEL`, which holds its staff
/// and the staff's "+n" box.
#[test]
fn the_wizards_guild_panel_holds_its_staff() {
    let library = common::library();
    let catalogue = catalogue_of(library);

    for code in [*b"ABY1", *b"ABY2", *b"ABY3"] {
        let dialog = unit(&catalogue, code)
            .desc
            .building()
            .and_then(|b| b.dialog_id());

        assert_eq!(dialog, Some(gear::STAFF_PANEL));
    }

    let (_, record) = catalogue
        .tagged(b"SMNU")
        .find(|(_, r)| r.id == gear::STAFF_PANEL)
        .expect("the wizards' guild panel");
    let panel = Panel::parse(record.data).expect("it parses");
    let ids: Vec<_> = controls(&panel)
        .iter()
        .filter_map(|g| word(g, smnu::CONTROL))
        .collect();

    assert!(ids.contains(&gear::FORGE_WEAPON.0));
    assert!(ids.contains(&gear::STAFF_BONUS));
}

/// A reward flag's help page reads `REWARD_HELP`, whatever its kind of
/// flag; a building, chest, sign and hero read their own `HelpID`.
#[test]
fn a_reward_flags_help_is_the_reward_topic() {
    let library = common::library();
    let catalogue = catalogue_of(library);
    let text = Text::build(&catalogue);
    let topic = |kind, code| {
        let id = portrait::help(kind, &unit(&catalogue, code).desc)?;

        text.help(id)
            .map(|t| t.trim().lines().next().unwrap_or_default().to_owned())
    };
    let reward =
        Some("- Heroes are often motivated by rewards offered by the Sovereign".to_owned());

    assert_eq!(topic(Kind::REWARD, *b"ARA2"), reward);
    assert_eq!(topic(Kind::REWARD, *b"ARA4"), reward);
    assert_eq!(
        topic(Kind::BUILDING, *b"ABC1").as_deref(),
        Some("- New building costs are reduced by five percent")
    );
    assert!(
        topic(Kind::VEHICLE, *b"AVL1").is_some(),
        "a hero reads its HelpID"
    );

    // A chest or sign is a building, unit kind 0 (**Measured**).
    assert_eq!(
        topic(Kind::BUILDING, *b"BBt1").as_deref(),
        Some("- Heroes often discover treasure in the course of their quests in the wilderness")
    );
    assert_eq!(
        topic(Kind::BUILDING, *b"BBs2").as_deref(),
        Some("- Indicates a Goblin presence in the area")
    );
    assert_eq!(topic(Kind::BUILDING, *b"BBs1"), None, "the banner has none");
}

/// A chest's (`APa6`), a caravan's and a hero's (`AP20`) "?" is 8028:
/// their help is `AP94`; a building's (`AP01`) is 8029, `AP95`.
#[test]
fn a_panel_with_8028_opens_the_short_help() {
    let library = common::library();
    let catalogue = catalogue_of(library);
    let text = Text::build(&catalogue);
    let page = |id: &[u8; 4]| {
        let layout = regent_core::view::menu::layout(&catalogue, &text, FourCc(*id));

        portrait::help_page(&layout.expect("the panel").expect("a layout"))
    };

    assert_eq!(page(b"APa6"), portrait::SHORT_HELP);
    assert_eq!(page(b"AP20"), portrait::SHORT_HELP);
    assert_eq!(page(b"AP01"), portrait::HELP);
    assert_eq!(portrait::SHORT_HELP, FourCc(*b"AP94"));
}

/// A building with no name shows its first level's title in capitals.
#[test]
fn a_nameless_building_shows_its_first_levels_title() {
    let library = common::library();
    let catalogue = catalogue_of(library);
    let palace = unit(&catalogue, *b"ABJ3");

    assert_eq!(
        palace.desc.building().and_then(|b| b.upgrade_from()),
        Some(FourCc::new(b"ABJ2"))
    );
    assert_eq!(
        portrait::title(&catalogue, &Text::default(), &palace.desc, None).expect("a title"),
        "PALACE"
    );

    let warrior = unit(&catalogue, *b"AVL1");

    assert_eq!(
        portrait::title(&catalogue, &Text::default(), &warrior.desc, None).expect("a title"),
        "Warrior"
    );
    assert_eq!(
        portrait::title(
            &catalogue,
            &Text::default(),
            &warrior.desc,
            Some("Meyarl the Righteous")
        )
        .expect("a title"),
        "Meyarl the Righteous"
    );
}

/// A warrior's ring is the smallest slot, centred on his standing rect.
#[test]
fn a_warriors_ring_is_the_smallest_slot() {
    let library = common::library();
    let catalogue = catalogue_of(library);
    let warrior = unit(&catalogue, *b"AVL1");

    assert_eq!(
        ring::rect(&catalogue, &warrior).expect("art"),
        Some([-15, -9, 10, 12])
    );

    let found = ring::ring(&catalogue, &warrior, Look::Selected)
        .expect("art")
        .expect("a ring");

    assert_eq!((found.slot, found.at), (0, (-3, 1)));
}

/// Every selection ring picture is glow alone: indices 251 to 254 in draw
/// mode 7, which the world blit lightens.
#[test]
fn the_selection_ring_is_all_glow() {
    let library = common::library();
    let catalogue = catalogue_of(library);
    let at = catalogue
        .find(b"IMAG", Look::Selected.image())
        .expect("AVn1");
    let sets = regent_core::content::imagery::sets(&catalogue, at).expect("it parses");
    let stand = sets
        .iter()
        .find(|a| a.id == ring::SET)
        .expect("the ring set");

    assert_eq!(stand.poses.len(), 13);

    for pose in &stand.poses {
        let FrameSource::Tile { archive, index, .. } = pose.frames[0] else {
            panic!("a tile");
        };
        let record = catalogue
            .positional(archive, b"TILE", index)
            .expect("the tile");
        let sprite = regent_core::view::Sprite::at(&catalogue, archive, index)
            .expect("present")
            .expect("it decodes");

        assert_eq!(
            Tile::parse(record.data).expect("it parses").mode(),
            Mode::Band
        );

        for i in (0..sprite.covered.len()).filter(|i| sprite.covered[*i]) {
            assert!(
                matches!(sprite.ink(i, Blit::World), Ink::Shade(Shade::Raise(_))),
                "tile {index} pixel {i}"
            );
        }
    }
}

/// The native ring (`ring::band`) fitted to each slot of the three ring
/// images: its band and the art's drawn pixels overlap by 86 % or more
/// (intersection over union), and its weights differ from the art's by
/// under a fifth of the peak on average, over either's pixels.
#[test]
fn a_native_ring_covers_its_art() {
    let library = common::library();
    let catalogue = catalogue_of(library);

    for look in [Look::Selected, Look::Fits, Look::Blocked] {
        let at = catalogue.find(b"IMAG", look.image()).expect("the image");
        let sets = regent_core::content::imagery::sets(&catalogue, at).expect("it parses");
        let stand = sets.iter().find(|a| a.id == ring::SET).expect("the set");

        for (slot, pose) in stand.poses.iter().enumerate() {
            let FrameSource::Tile { archive, index, .. } = pose.frames[0] else {
                panic!("a tile");
            };
            let sprite = regent_core::view::Sprite::at(&catalogue, archive, index)
                .expect("present")
                .expect("it decodes");
            let band = Band::fit(&sprite, look).expect("a band");
            let art: HashMap<(i32, i32), f32> = band::weights(&sprite)
                .into_iter()
                .map(|(x, y, w, _)| (pixel(x, y), w))
                .collect();

            // Every pixel of the tile, with the band's weight and the art's.
            let (w, h) = (i32::from(sprite.width), i32::from(sprite.height));
            let (ax, ay) = (i32::from(sprite.anchor.0), i32::from(sprite.anchor.1));
            let pairs: Vec<(f32, f32)> = (0..h)
                .flat_map(|y| (0..w).map(move |x| (x - ax, y - ay)))
                .map(|(x, y)| {
                    #[allow(clippy::cast_precision_loss, reason = "art pixels")]
                    let middle = (x as f32 + 0.5, y as f32 + 0.5);

                    (
                        band.weight(middle),
                        art.get(&(x, y)).copied().unwrap_or(0.0),
                    )
                })
                .filter(|(n, a)| *n > 0.0 || *a > 0.0)
                .collect();
            let both = pairs.iter().filter(|(n, a)| *n > 0.0 && *a > 0.0).count();

            #[allow(clippy::cast_precision_loss, reason = "pixel counts")]
            let overlap = both as f32 / pairs.len() as f32;
            #[allow(clippy::cast_precision_loss, reason = "pixel counts")]
            let off = pairs.iter().map(|(n, a)| (n - a).abs()).sum::<f32>()
                / pairs.len() as f32
                / band.peak;

            assert!(overlap >= 0.86, "{look:?} slot {slot}: overlap {overlap}");
            assert!(off < 0.2, "{look:?} slot {slot}: off by {off}");
        }
    }
}

/// A unit's native ring sits where its art ring does: the band's centre
/// within 3 pixels of the ring rect's (`Ring::rect`), its outer edge
/// inside the rect grown by 3: the palace (slot 9) and a peasant (slot 0).
#[test]
fn a_native_ring_takes_its_art_rings_place() {
    let library = common::library();
    let catalogue = catalogue_of(library);

    for code in [*b"ABJ1", *b"AVj1"] {
        let found = ring::ring(&catalogue, &unit(&catalogue, code), Look::Selected)
            .expect("art")
            .expect("a ring");
        let band = found.band(&catalogue).expect("a band").expect("it decodes");

        #[allow(clippy::cast_precision_loss, reason = "screen pixels")]
        let [x0, y0, x1, y1] = found.rect().map(|v| v as f32);
        let (a, b) = band.radii;
        let (cx, cy) = band.centre;

        assert!((cx - f32::midpoint(x0, x1)).abs() <= 3.0, "{code:?} x {cx}");
        assert!((cy - f32::midpoint(y0, y1)).abs() <= 3.0, "{code:?} y {cy}");
        assert!(a + band.half <= (x1 - x0) / 2.0 + 3.0, "{code:?} a {a}");
        assert!(b + band.half <= (y1 - y0) / 2.0 + 3.0, "{code:?} b {b}");
    }
}

/// The pixel whose middle is `(x, y)`.
#[allow(clippy::cast_possible_truncation, reason = "art pixels")]
const fn pixel(x: f32, y: f32) -> (i32, i32) {
    (x.floor() as i32, y.floor() as i32)
}

/// Every building and character with art gets a ring. Eight have no rect
/// in their art (markers, effects, the Creature Den): theirs is filled
/// from the box they are drawn in ([`ring::Drawn`]).
#[test]
fn every_building_and_character_with_art_has_a_ring() {
    let library = common::library();
    let catalogue = catalogue_of(library);
    let mut without = Vec::new();
    let mut rectless = Vec::new();

    for (at, _) in catalogue.tagged(b"DUNT") {
        let unit = Unit::at(&catalogue, at).expect("it resolves");

        if !matches!(unit.kind, Some(UnitKind::Building | UnitKind::Character))
            || unit.animations.is_empty()
        {
            continue;
        }

        if ring::ring(&catalogue, &unit, Look::Selected)
            .expect("art")
            .is_none()
        {
            without.push(unit.id.to_string());
        }

        if ring::rect(&catalogue, &unit).expect("art").is_none() {
            rectless.push(unit.id.to_string());
        }
    }

    rectless.sort();

    assert!(without.is_empty(), "{without:?}");
    assert_eq!(rectless, RECTLESS.map(|c| FourCc(c).to_string()));
}

/// The units whose art holds no rect for unit rect build.
const RECTLESS: [[u8; 4]; 8] = [
    *b"ABA0", *b"BBA1", *b"BBf1", *b"MVa1", *b"MVa2", *b"MVa3", *b"MVa4", *b"WVe2",
];

/// Each unit of [`RECTLESS`] drawn in slot 0 of its first set, its bar
/// shown where it has one, its name label shown (a building's title; the
/// characters are nameless): its ring's slot and place. The Creature
/// Den's is the capture's
/// (`the_creature_dens_ring_fits_its_drawn_box`). A nameless label has
/// no width, so without a bar it adds nothing.
#[test]
fn a_rectless_units_ring_fits_its_drawn_box() {
    let library = common::library();
    let catalogue = catalogue_of(library);
    let ring_of = |code| {
        let unit = unit(&catalogue, code);
        let set = unit
            .animations
            .iter()
            .find(|a| a.kind != SetKind::Interface)
            .expect("a set");
        let parts = hit::parts(
            &catalogue,
            &[Layer {
                set,
                slot: 0,
                frame: 0,
            }],
        )
        .expect("art");
        let shown = if bar::has_bar(&unit) {
            ring::HpBar::Shown
        } else {
            ring::HpBar::Hidden
        };
        let title = unit.desc.head.title();
        let name = label::name(bar::Body::of(&unit), None, &title, 0);
        let drawn = ring::Drawn::of(
            &catalogue,
            &unit,
            hit::bounds(&parts),
            shown,
            Some(ring::Words::Name(&name)),
        )
        .expect("art");
        let found = ring::fitted(&catalogue, &unit, Look::Selected, ring::Fit::Drawn(drawn))
            .expect("art")
            .expect("a ring");

        (drawn.rect(), found.slot, found.at)
    };

    // Rect, slot, the ring's anchor; in RECTLESS's order.
    let expected = [
        (Some([-29, -51, 31, 24]), 1, (1, -14)),
        (Some([-63, -3, 56, 121]), 3, (-4, 59)),
        (Some([-124, -94, 121, 115]), 5, (-2, 10)),
        (Some([-1, -1, 2, 2]), 0, (0, 0)),
        (Some([-7, -7, 8, 8]), 0, (0, 0)),
        (Some([0, 0, 1, 1]), 0, (0, 0)),
        (Some([-2, -2, 4, 3]), 0, (1, 0)),
        (Some([-28, -7, 24, 49]), 1, (-2, 21)),
    ];

    assert_eq!(RECTLESS.map(ring_of), expected);
}

/// The palace has no `Stand`, so its `Inactive` block 1 sizes the ring:
/// slot 9, 508 by 296, round its base. **Viewer**: drawn over the palace's
/// picture at that offset, it rings the walls.
#[test]
fn the_palaces_ring_fits_its_inactive_rect() {
    let library = common::library();
    let catalogue = catalogue_of(library);
    let palace = unit(&catalogue, *b"ABJ1");
    let rect = ring::rect(&catalogue, &palace)
        .expect("art")
        .expect("a rect");
    let found = ring::ring(&catalogue, &palace, Look::Selected)
        .expect("art")
        .expect("a ring");

    assert_eq!(rect, [-232, -28, 236, 259]);
    assert_eq!((found.slot, found.at), (9, (2, 115)));
}

/// The Creature Den has no `Stand`, no `Inactive` block 1 and no `0x40`
/// set: `IsoGenUnit` fills its rect with the box it is drawn in,
/// its tiles (`Inactive` slot 0), bar and name. **Capture**
/// (`ui_creature_den_selected.png`): the den's tile at (162, 118), so its
/// anchor at (225, 121); the ring slot 3, its anchor at (221, 180).
#[test]
fn the_creature_dens_ring_fits_its_drawn_box() {
    /// The den's `Inactive` set.
    const INACTIVE: u32 = 0xd0;

    let library = common::library();
    let catalogue = catalogue_of(library);
    let den = unit(&catalogue, *b"BBA1");

    assert_eq!(ring::rect(&catalogue, &den).expect("art"), None);

    let set = den
        .animations
        .iter()
        .find(|a| a.id == INACTIVE)
        .expect("Inactive");
    let parts = hit::parts(
        &catalogue,
        &[Layer {
            set,
            slot: 0,
            frame: 0,
        }],
    )
    .expect("art");
    let drawn = ring::Drawn::of(
        &catalogue,
        &den,
        hit::bounds(&parts),
        ring::HpBar::Shown,
        Some(ring::Words::Name("Creature Den")),
    )
    .expect("art");

    assert_eq!(drawn.rect(), Some([-63, -3, 56, 121]));

    let found = ring::fitted(&catalogue, &den, Look::Selected, ring::Fit::Drawn(drawn))
        .expect("art")
        .expect("a ring");

    assert_eq!((found.slot, found.at), (3, (-4, 59)));
}

/// The `NumberedName` units of `kind`, by `HEAD` name.
fn numbered_names(kind: UnitKind) -> Vec<String> {
    let library = common::library();
    let catalogue = catalogue_of(library);
    let mut names: Vec<String> = catalogue
        .tagged(b"DUNT")
        .filter_map(|(_, r)| Unit::find(&catalogue, r.id)?.ok())
        .filter(|u| u.desc.kind() == kind)
        .filter(|u| world::numbered::numbered_key(&catalogue, u).is_some())
        .map(|u| String::from_utf8_lossy(u.desc.head.name).into_owned())
        .collect();

    names.sort();
    names.dedup();
    names
}

/// The `NumberedName` buildings, by `HEAD` name.
#[test]
fn these_buildings_are_numbered() {
    assert_eq!(numbered_names(UnitKind::Building), NUMBERED);
}

/// The `NumberedName` characters, by `HEAD` name: `VehicleRec`'s.
#[test]
fn these_characters_are_numbered() {
    assert_eq!(numbered_names(UnitKind::Character), NUMBERED_CHARACTERS);
}

/// [`these_characters_are_numbered`].
const NUMBERED_CHARACTERS: [&str; 7] = [
    "Caravan",
    "City_Guard",
    "Palace_Guard",
    "Peasant",
    "Peasant_female",
    "Tax_Collector",
    "Veteran_City_Guard",
];

/// [`these_buildings_are_numbered`].
const NUMBERED: [&str; 12] = [
    "Ballista_Tower",
    "General_Housing",
    "Guardhouse1",
    "Guardhouse2",
    "Library1",
    "Library2",
    "Marketplace1",
    "Marketplace2",
    "Marketplace3",
    "Sewers",
    "Trading_Post",
    "Wizards_Tower",
];

/// Every gear picture set has [`gear::FRAMES`] frames, so a level 5
/// item has none of its own.
#[test]
fn every_gear_picture_set_has_four_frames() {
    let library = common::library();
    let catalogue = catalogue_of(library);

    for record in [gear::WEAPON_RECORD, gear::ARMOUR_RECORD] {
        let data = catalogue
            .find(b"IMAG", record)
            .and_then(|at| catalogue.record(b"IMAG", at))
            .map_or_else(|| panic!("no {record}"), |r| r.data);
        let image = Image::parse(data).expect("it parses");
        let frames: Vec<u16> = image
            .sets
            .iter()
            .flat_map(|s| &s.slots)
            .map(|s| s.frames)
            .collect();

        assert_ne!(frames, [] as [u16; 0]);
        assert!(
            frames.iter().all(|&n| i32::from(n) == gear::FRAMES),
            "{record}: {frames:?}"
        );
    }
}

/// [`enchant::ENCHANTMENTS`]' icons are sets HD ships; the lasting
/// effects the original list skips are HD's overlays.
#[test]
fn the_enchantments_icons_and_skipped_overlays_ship() {
    let library = common::library();
    let catalogue = catalogue_of(library);

    // Lasting effects the original list skips, from parsed descriptors.
    for (code, name) in [
        (b"WRb2", "fire_shield_icon"),
        (b"WRf2", "resist_magic_icon"),
        (b"CRB1", "vines_effector"),
        (b"NRe3", "paralytic_gaze_icon"),
        (b"XR25", "Ratman_Plague_Icon"),
        (b"NRi1", "magic_mirror_icon"),
        (b"QRc1", "Poison_icon"),
        (b"QRd1", "Cursed_Icon"),
        (b"QRd0", "ring_icon"),
        (b"QRb1", "charm_icon"),
    ] {
        let unit = Unit::descriptor(&catalogue, FourCc(*code))
            .expect("the overlay")
            .expect("it parses");

        assert_eq!(unit.head.name, name.as_bytes());
    }

    // Every icon is a set HD ships.
    for e in enchant::ENCHANTMENTS {
        let (record, set) = e.icon.expect("original icon");
        let at = catalogue.find(b"IMAG", record).expect("the record");
        let sets = regent_core::content::imagery::sets(&catalogue, at).expect("it parses");

        assert!(
            sets.iter()
                .any(|s| regent_core::data::imag::set_number(s.id) == set),
            "{} {record} {set}",
            e.code
        );
    }
}

/// Active overlays omitted by the original switch get rows in openregent.
#[test]
fn omitted_effects_have_rows() {
    for (code, row) in [
        (b"WRf2", b"WRf1"),
        (b"CRB1", b"CRB2"),
        (b"NRe3", b"NRe3"),
        (b"XR25", b"XR25"),
        (b"NRi1", b"NRi1"),
        (b"QRc1", b"QRc1"),
        (b"QRd1", b"QRd1"),
        (b"QRd0", b"QRd0"),
        (b"QRb1", b"QRb1"),
    ] {
        assert_eq!(
            enchant::of(FourCc(*code)).map(|e| e.code),
            Some(FourCc(*row))
        );
    }

    let library = common::library();
    let catalogue = catalogue_of(library);
    let text = Text::build(&catalogue);

    for (code, title) in [
        (b"NRe3", "Paralytic Gaze"),
        (b"XR25", "Ratman Plague"),
        (b"NRi1", "Magic Mirror"),
        (b"QRc1", "Poisoned!"),
        (b"QRd1", "Cursed"),
        (b"QRd0", "This hero has the Magic Ring."),
        (b"QRb1", "Charmed!"),
    ] {
        let row = enchant::of(FourCc(*code)).expect("added row");

        assert_eq!(row.name(&catalogue, &text), title);
        assert_eq!(row.icon, None);
    }
}

/// A coffer reads as `GMTX` 76 or 77; an empty tower's says nothing.
#[test]
fn a_coffer_says_what_it_holds() {
    let library = common::library();
    let catalogue = catalogue_of(library);
    let text = Text::build(&catalogue);
    let coffer = |gold, income| Coffer { gold, income }.text(&text);

    assert_eq!(
        coffer(2230, Some(2)).as_deref(),
        Some("Coffers hold 2230 gold")
    );
    assert_eq!(coffer(0, Some(2)).as_deref(), Some("Coffers hold no gold"));
    assert_eq!(coffer(0, Some(details::SILENT_INCOME)), None);
}

#[test]
fn the_wizards_curse_sign_keeps_its_rows() {
    let files = [common::hd().join("Data/gpltext.cam")];
    let library = Library::from_files(&files).expect("GPL text archive");
    let catalogue = catalogue_of(&library);
    let text = Text::build(&catalogue);

    assert_eq!(
        text.activity(147).as_deref(),
        Some("O | O | X\r--------------\rX | X | O\r--------------\r   | O | X")
    );
}

/// The two `SignPostController` panels: `APa7` the `BBs` signs, banners
/// and markers alone, `APa6` the `BBt` chests alone.
#[test]
fn the_signs_and_chests_name_their_panels() {
    let library = common::library();
    let catalogue = catalogue_of(library);
    let mut panels: Vec<(String, String)> = catalogue
        .tagged(b"DUNT")
        .filter_map(|(_, r)| Unit::find(&catalogue, r.id)?.ok())
        .filter_map(|u| {
            let dialog = u.desc.building()?.dialog_id()?;

            [details::SIGN_PANEL, details::CHEST_PANEL]
                .contains(&dialog)
                .then(|| (dialog.to_string(), u.id.to_string()[..3].to_owned()))
        })
        .collect();

    panels.sort();
    panels.dedup();

    assert_eq!(
        panels,
        [
            ("APa6".to_owned(), "BBt".to_owned()),
            ("APa7".to_owned(), "BBs".to_owned())
        ]
    );
}

/// A chest with no message says its gold, `GMTX` 226, or `GMTX` 233.
#[test]
fn a_chest_says_its_gold() {
    let library = common::library();
    let catalogue = catalogue_of(library);
    let text = Text::build(&catalogue);
    let says = |says| {
        Posted {
            board: Board::Chest,
            says,
            item: None,
        }
        .text(&text)
    };

    assert_eq!(says(Says::Gold(120)).as_deref(), Some("120 Gold in Chest"));
    assert_eq!(says(Says::Gold(0)).as_deref(), Some("Empty"));
    assert_eq!(says(Says::Nothing), None);
}

/// The build preview's ring fits its pad grown by `ConstructData1`: far
/// wider than a selected building's. An inn's: slot 8, 465 pixels, not 3.
#[test]
fn the_previews_ring_fits_its_clearing_reach() {
    let library = common::library();
    let catalogue = catalogue_of(library);
    let ring_of = |code, fit| {
        let found = ring::fitted(&catalogue, &unit(&catalogue, code), Look::Fits, fit)
            .expect("art")
            .expect("a ring");

        (found.slot, found.at)
    };

    // Inn, statue, warriors' guild: code, reach, preview ring, own ring.
    let cases = [
        (*b"ABF1", [-208, -128, 224, 272], (8, (8, 72)), (3, (8, 61))),
        (*b"ABk1", [-160, -144, 160, 176], (6, (0, 16)), (1, (0, 19))),
        (
            *b"ABV1",
            [-256, -160, 224, 288],
            (9, (-16, 64)),
            (5, (-19, 49)),
        ),
    ];

    for (code, reach, preview, own) in cases {
        let u = unit(&catalogue, code);

        assert_eq!(
            clearing::reach(&catalogue, &u, clearing::MARGIN).expect("art"),
            Some(reach)
        );
        assert_eq!(ring_of(code, ring::Fit::Clearing), preview);
        assert_eq!(ring_of(code, ring::Fit::Own), own);
    }
}

/// `sprite`'s grey levels, each the mean of a `block`-square of pixels.
fn greys(sprite: &regent_core::view::Sprite, block: usize) -> Vec<f64> {
    let (wide, rgba) = (usize::from(sprite.width), sprite.rgba());
    let grey = |i: usize| {
        let p = &rgba[i * 4..i * 4 + 3];

        0.30 * f64::from(p[0]) + 0.59 * f64::from(p[1]) + 0.11 * f64::from(p[2])
    };
    let side = wide / block;

    (0..side * side)
        .map(|n| {
            let (x, y) = (n % side * block, n / side * block);
            let sum: f64 = (0..block * block)
                .map(|k| grey((y + k / block) * wide + x + k % block))
                .sum();

            #[allow(clippy::cast_precision_loss, reason = "a few pixels")]
            let count = (block * block) as f64;

            sum / count
        })
        .collect()
}

/// Pearson correlation of `a` and `b`.
fn correlation(a: &[f64], b: &[f64]) -> f64 {
    #[allow(clippy::cast_precision_loss, reason = "a few pixels")]
    let n = a.len() as f64;
    let (ma, mb) = (a.iter().sum::<f64>() / n, b.iter().sum::<f64>() / n);
    let cov: f64 = a.iter().zip(b).map(|(x, y)| (x - ma) * (y - mb)).sum();
    let va: f64 = a.iter().map(|x| (x - ma).powi(2)).sum();
    let vb: f64 = b.iter().map(|y| (y - mb).powi(2)).sum();

    cov / (va * vb).sqrt()
}

/// A character's 25x25 list icon is its 100x100 portrait shrunk, whole
/// frame; a building's is another drawing. So `icon_source` gives the
/// portrait for exactly the icons that match it.
#[test]
fn a_characters_list_icon_is_its_portrait_shrunk() {
    // The portrait's pixels to one icon pixel, a side.
    const SHRINK: usize = 4;
    // Lowest character 0.70 (`AVb0`, `AVb2`), highest building 0.20.
    const SAME: f64 = 0.5;

    let library = common::library();
    let catalogue = catalogue_of(library);
    let mut shrunk = Vec::new();
    let mut drawn = Vec::new();

    for (at, _) in catalogue.tagged(b"DUNT") {
        let unit = Unit::at(&catalogue, at).expect("it resolves");
        let icon = unit
            .interface(INTERFACE_ICON)
            .and_then(|set| match set.poses[0].stream(0)[0] {
                FrameSource::Tile { archive, index, .. } => {
                    regent_core::view::Sprite::at(&catalogue, archive, index)
                }
                _ => None,
            });
        let (Some(icon), Some(full)) = (icon, portrait::portrait(&catalogue, &unit)) else {
            continue;
        };
        let (icon, full) = (icon.expect("it decodes"), full.expect("it decodes"));
        let r = correlation(&greys(&full, SHRINK), &greys(&icon, 1));
        let source = portrait::icon_source(&catalogue, &unit).map(|s| s.expect("it decodes"));

        assert_eq!(source.is_some(), r > SAME, "{} {r:.2}", unit.id);

        if source.is_some() {
            shrunk.push(unit.id.to_string());
        } else {
            drawn.push(unit.id.to_string());
        }
    }

    // Records, base and expansion: 38 characters, 64 buildings.
    assert_eq!(
        (shrunk.len(), drawn.len()),
        (38, 64),
        "{shrunk:?} {drawn:?}"
    );
}
