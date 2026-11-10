//! HD's quest map, `APdb`, against the shipped files: `view::menu::quest_map`.

mod common;

use std::collections::BTreeSet;
use std::path::PathBuf;

use regent_core::data::cam::Archive;
use regent_core::data::catalogue::Library;
use regent_core::data::deps::{self, Difficulty};
use regent_core::data::fourcc::FourCc;
use regent_core::data::text::Text;
use regent_core::sim::world::sound;
use regent_core::view::menu::quest_map::{
    self, Art, BACKGROUND, DIFFICULTY_TEXT, LOCKED_TEXT, Lock, MET_TEXT, Progress, Requires,
    SECRET_TEXT, SLOTS, UNMET_TEXT, WON_TEXT,
};
use regent_core::view::menu::{self, State, Tick};

/// Every `UIData_<w>_<h>.dat` holding `APdb`, with its screen size.
fn layouts() -> Vec<((i32, i32), regent_core::data::smnu::Panel)> {
    let data = common::hd().join("Data");

    common::walk(&data, &["dat"])
        .into_iter()
        .filter_map(|path| {
            let name = path.file_stem()?.to_str()?.to_owned();
            let mut size = name.strip_prefix("UIData_")?.split('_');
            let w = size.next()?.parse().ok()?;
            let h = size.next()?.parse().ok()?;

            quest_map::load(&path).ok().map(|panel| ((w, h), panel))
        })
        .collect()
}

/// The 1920×1200 layout, the captures' size.
fn panel() -> regent_core::data::smnu::Panel {
    quest_map::load(&common::hd().join("Data/UIData_1920_1200.dat")).expect("APdb")
}

#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn the_map_and_its_sites_are_the_same_at_every_screen_size() {
    let all = layouts();
    let first = quest_map::sites(&all[0].1);

    assert_eq!(all.len(), 11);

    for (_, panel) in &all {
        assert_eq!(quest_map::sites(panel), first);
        assert_eq!(quest_map::map(panel), quest_map::map(&all[0].1));
    }
}

#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn every_slot_has_a_hundred_pixel_hit_region_a_lit_picture_and_a_star() {
    let sites = quest_map::sites(&panel());
    let lit: Vec<FourCc> = sites
        .iter()
        .filter_map(|s| s.lit.and_then(|l| l.art).map(|a| a.record))
        .collect();

    assert_eq!(sites.len(), SLOTS.len());
    assert!(
        sites
            .iter()
            .all(|s| (s.hit.rect.width, s.hit.rect.height) == (100, 100))
    );
    assert!(sites.iter().all(|s| s.star.is_some()));
    assert_eq!(lit[..31], [FourCc(*b"IX33"); 31]);
    assert_eq!(
        lit[31..],
        [FourCc(*b"DX31"), FourCc(*b"DX32"), FourCc(*b"DX33")]
    );
}

/// The hit region lies over its picture: its centre inside the lit rect.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_hit_regions_centre_is_on_its_lit_picture() {
    for site in quest_map::sites(&panel()) {
        let hit = site.hit.rect;
        let lit = site.lit.expect("a lit picture").rect;
        let (x, y) = (hit.x + hit.width / 2, hit.y + hit.height / 2);

        assert!(
            (lit.x..lit.x + lit.width).contains(&x) && (lit.y..lit.y + lit.height).contains(&y),
            "{:?}",
            site.quest
        );
    }
}

/// Seven slots can hide behind a "?": the quests `QDEP` locks, and the
/// Holy Chalice, which `QDEP` does not.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn seven_slots_have_a_cloud() {
    let clouded: Vec<usize> = quest_map::sites(&panel())
        .iter()
        .filter(|s| s.cloud.is_some())
        .map(|s| s.slot)
        .collect();

    assert_eq!(clouded, [2, 5, 8, 9, 10, 26, 30]);
}

#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn the_map_is_one_picture_the_panels_size() {
    let library = common::library();
    let catalogue = library.catalogue().unwrap();
    let map = quest_map::map(&panel()).unwrap();
    let art = map.art.unwrap();
    let sprite = quest_map::picture(&catalogue, art, 0, 0).unwrap().unwrap();

    assert_eq!(
        art,
        Art {
            record: FourCc(*b"IX10"),
            set: 1001
        }
    );
    assert_eq!(
        (i32::from(sprite.width), i32::from(sprite.height)),
        (map.rect.width, map.rect.height)
    );
    assert_eq!((map.rect.width, map.rect.height), (1398, 1611));
}

/// Each lit picture fills its rectangle, `DX31` too, but for the Dark
/// Forest's, two rows short, and Elven Treachery's, three columns short.
/// `DX32` and `DX33` ship nowhere.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_lit_picture_is_its_rectangles_size() {
    let library = common::library();
    let catalogue = library.catalogue().unwrap();

    let short: Vec<(FourCc, (i32, i32))> = quest_map::sites(&panel())
        .iter()
        .filter_map(|site| {
            let lit = site.lit.unwrap();
            let sprite = quest_map::picture(&catalogue, lit.art.unwrap(), 1, 0)?.unwrap();
            let short = (
                lit.rect.width - i32::from(sprite.width),
                lit.rect.height - i32::from(sprite.height),
            );

            (short != (0, 0)).then_some((site.quest, short))
        })
        .collect();

    assert_eq!(
        short,
        [(FourCc(*b"QU05"), (0, 2)), (FourCc(*b"QU10"), (3, 0))]
    );
}

/// Every installed slot's pog is a 66-pixel picture; XQD2 and XQD3 ship
/// none. **Game**: the captures show Urban Renewal's hammer, `IX11` 1009,
/// and the Tomb's dragon bones, `INTD` 1008.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn every_installed_quests_pog_is_sixty_six_pixels() {
    let library = common::library();
    let catalogue = library.catalogue().unwrap();
    let found: Vec<bool> = (0..SLOTS.len())
        .map(|slot| {
            let art = quest_map::pog(slot).unwrap();

            quest_map::picture(&catalogue, art, 0, 0)
                .and_then(Result::ok)
                .is_some_and(|s| (s.width, s.height) == (66, 66))
        })
        .collect();

    assert!(found[..32].iter().all(|f| *f));
    assert_eq!(found[32..], [false, false]);
}

#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn every_painted_slot_has_its_video() {
    let hd = common::hd();
    let held: BTreeSet<FourCc> = [
        hd.join("Data/cinedata1.cam"),
        hd.join("DataMX/mx_cinedata1.cam"),
    ]
    .iter()
    .flat_map(|path: &PathBuf| {
        let bytes = std::fs::read(path).unwrap();
        let archive = Archive::parse(&bytes).unwrap();

        archive
            .records()
            .filter(|(chunk, _)| chunk.tag.as_bytes() == b"PICT")
            .map(|(_, record)| record.id)
            .collect::<Vec<_>>()
    })
    .collect();
    let videos: Vec<bool> = (0..SLOTS.len())
        .map(|slot| held.contains(&quest_map::video(slot).unwrap()))
        .collect();

    assert!(videos[..31].iter().all(|v| *v));
    assert_eq!(videos[31..], [false, false, false]);
}

#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn the_title_bars_texts_are_the_expansions_gmtx() {
    let library = Library::from_files(&[common::hd().join("DataMX/mx_textdata.cam")]).unwrap();
    let catalogue = library.catalogue().unwrap();
    let text = Text::build(&catalogue);
    let gmtx = |i: usize| text.gmtx(i).unwrap().into_owned();
    let names: Vec<String> = [
        Difficulty::Easy,
        Difficulty::Medium,
        Difficulty::Hard,
        Difficulty::Master,
    ]
    .into_iter()
    .map(|d| gmtx(quest_map::difficulty_text(d)))
    .collect();

    assert_eq!(names, ["Beginner", "Advanced", "Expert", "Master"]);
    assert_eq!(
        gmtx(DIFFICULTY_TEXT),
        "\u{1}FFDDAADifficulty: \u{1}E6E1FD%s"
    );
    assert!(gmtx(WON_TEXT).contains("Victories: "));
    assert!(gmtx(LOCKED_TEXT).ends_with("Requires: %s"));
    assert_eq!(gmtx(SECRET_TEXT), "Secret Quest");
    assert_eq!(gmtx(MET_TEXT), "\u{1}656565%s  ");
    assert_eq!(gmtx(UNMET_TEXT), "\u{1}E6E1FD%s  ");
    assert_eq!(gmtx(quest_map::ERASE_TITLE_TEXT), "Erase Victories");
    assert_eq!(
        gmtx(quest_map::ERASE_TEXT),
        "Are you sure you want to erase all of your victories?"
    );
    assert_eq!(gmtx(regent_core::view::menu::NAME_TEXT), "Your Majesty");
}

/// **Game**: the bottom capture lists the Tomb's requirements in rule order.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_locked_quests_requirements_are_its_rules_ids_in_order() {
    let bytes = std::fs::read(common::hd().join("DataMX/mx_miscdata.cam")).unwrap();
    let archive = Archive::parse(&bytes).unwrap();
    let source = archive
        .records()
        .find(|(_, r)| r.id == FourCc(*b"QDEP"))
        .map(|(_, r)| String::from_utf8_lossy(r.data).into_owned())
        .unwrap();
    let rules = deps::quests(&source).unwrap();
    let tomb = rules.iter().find(|q| q.id == SLOTS[8]).unwrap();

    assert_eq!(
        tomb.requires.as_ref().unwrap().ids(),
        [FourCc(*b"QU01"), FourCc(*b"QU13"), FourCc(*b"QU17")]
    );
    assert_eq!(tomb.difficulty, Difficulty::Hard);
}

#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn the_frame_is_the_corner_pieces_far_corner_and_stays_put() {
    for (screen, panel) in layouts() {
        let (w, h) = quest_map::frame_size(&panel).unwrap();

        assert!(w <= screen.0 && h <= screen.1, "{screen:?}");
        assert!(w == 1280 || w == 1398, "{screen:?}");
    }

    let panel = panel();

    assert_eq!(quest_map::frame(&panel).len(), quest_map::FIXED.len() - 1);
    assert_eq!(
        quest_map::origin(&panel, (1920, 1200)),
        Some(((261, 0), (261, -205)))
    );
}

/// Every shipped frame is the 1920×1200 one moved by its anchors: exact,
/// or within 1 px on an edge held at the middle; the title and line are
/// left out (`quest_map::anchor`).
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn every_shipped_frame_is_the_largest_moved_by_its_anchors() {
    let base = panel();
    let from = quest_map::frame_size(&base).unwrap();
    let pieces = quest_map::frame(&base);

    for (screen, panel) in layouts() {
        let to = quest_map::frame_size(&panel).unwrap();
        let shipped = quest_map::frame(&panel);

        for ((id, piece), (_, theirs)) in pieces.iter().zip(&shipped) {
            let ours = quest_map::moved(*id, piece.rect, from, to);
            let r = theirs.rect;
            let off = [
                ours.x - r.x,
                ours.y - r.y,
                ours.x + ours.width - r.x - r.width,
                ours.y + ours.height - r.y - r.height,
            ];

            if ![quest_map::TITLE, quest_map::LINE].contains(id) {
                assert!(off.iter().all(|d| d.abs() <= 1), "{screen:?} {id} {off:?}");
            }
        }
    }
}

/// A piece's rectangle by id, moved to a `to`-sized frame.
fn moved_frame(to: (i32, i32)) -> impl Fn(i32) -> regent_core::data::smnu::Rect {
    let base = panel();
    let from = quest_map::frame_size(&base).unwrap();
    let pieces = quest_map::frame(&base);

    move |id| {
        let (_, p) = pieces.iter().find(|(i, _)| *i == id).unwrap();

        quest_map::moved(id, p.rect, from, to)
    }
}

/// Widened past any shipped size, the frame's pieces still meet: corners
/// at the window's corners, the bars between them, the bottom's three
/// parts end to end.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_frame_of_any_width_has_no_gaps() {
    // The last four: openregent's frames at the interface's scale, 848 high
    // (1920 × 1200, 3440 × 1440), and the narrowest, taller.
    for to in [
        (1398, 1200),
        (1920, 1200),
        (2867, 1200),
        (4000, 900),
        (1357, 848),
        (2026, 848),
        (quest_map::LEAST_WIDTH, 848),
        (quest_map::LEAST_WIDTH, 1200),
    ] {
        let at = moved_frame(to);
        let right = |id| at(id).x + at(id).width;
        let bottom = |id| at(id).y + at(id).height;

        assert_eq!((right(8504), bottom(8504)), to);
        assert_eq!((at(8500).x, at(8500).y), (0, 0));
        assert_eq!((at(8501).x, right(8501)), (right(8500), at(8502).x));
        assert_eq!((at(8503).y, bottom(8503)), (bottom(8502), at(8504).y));
        assert_eq!((at(8509).y, bottom(8509)), (bottom(8500), at(8508).y));
        assert_eq!(
            [right(8508), right(8505), right(8506), right(8507)],
            [at(8505).x, at(8506).x, at(8507).x, at(8504).x]
        );
        assert_eq!(right(8511), to.0);
        assert_eq!(bottom(8506), to.1);
    }

    // At the least width the bottom's middle pieces clear its corners;
    // one less and they overlap.
    let clear = |w: i32| {
        let at = moved_frame((w, 848));

        at(8512).x >= at(8508).x + at(8508).width && at(8513).x + at(8513).width <= at(8504).x
    };

    assert!(clear(quest_map::LEAST_WIDTH));
    assert!(!clear(quest_map::LEAST_WIDTH - 1));

    // The compass and its text on the centre piece, centred on the frame.
    let at = moved_frame((2867, 1200));

    assert_eq!(at(8506).x, 2867 / 2 - 109);
    assert_eq!(at(quest_map::FREESTYLE).x - at(8506).x, 62);
}

/// FREESTYLE QUESTS is set in its key-42 rectangle, the slot at the
/// centre piece's foot, not in its 66-high control. **Game**:
/// `menu_map_top.png`, the text inside the slot.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn the_freestyle_text_sits_in_the_slot_under_the_compass() {
    for (screen, panel) in layouts() {
        let text = quest_map::text_rect(&panel, quest_map::FREESTYLE_TEXT);

        assert_eq!(
            text,
            Some(regent_core::data::smnu::Rect {
                x: 4,
                y: 42,
                width: 168,
                height: 20
            }),
            "{screen:?}"
        );
    }

    assert_eq!(quest_map::text_rect(&panel(), quest_map::MAIN_MENU), None);
}

#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn the_background_is_the_faces_seven_hundred_seventy_wide() {
    let library = common::library();
    let catalogue = library.catalogue().unwrap();
    let sprite = quest_map::picture(&catalogue, BACKGROUND, 0, 0)
        .unwrap()
        .unwrap();

    assert_eq!((sprite.width, sprite.height), (770, 550));
}

/// The expansion's `QDEP`, which HD reads: every quest's rule.
fn rules() -> Vec<deps::Quest> {
    let bytes = std::fs::read(common::hd().join("DataMX/mx_miscdata.cam")).unwrap();
    let archive = Archive::parse(&bytes).unwrap();
    let source = archive
        .records()
        .find(|(_, r)| r.id == FourCc(*b"QDEP"))
        .map(|(_, r)| String::from_utf8_lossy(r.data).into_owned())
        .unwrap();

    deps::quests(&source).unwrap()
}

/// The slots locked under `progress`, as codes.
fn locked(rules: &[deps::Quest], progress: &Progress) -> Vec<String> {
    SLOTS
        .iter()
        .filter(|&&q| quest_map::lock(q, rules, progress) == Lock::Locked)
        .map(ToString::to_string)
        .collect()
}

/// "All Southern Quests" is what the Day of Reckoning's rule names.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn the_southern_quests_are_what_the_day_of_reckoning_requires() {
    let rules = rules();
    let reckoning = quest_map::SLOTS[quest_map::SOUTHERN_SLOT];
    let named = rules
        .iter()
        .find(|r| r.id == reckoning)
        .and_then(|r| r.requires.as_ref())
        .expect("its rule")
        .ids();

    assert_eq!(named, quest_map::SOUTHERN);
}

/// A fresh install locks six quests. **Game**: the Siege (`XQ07`) is open
/// from the start. The Holy Chalice's cloud (slot 2) never shows.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn with_no_victories_six_quests_are_locked() {
    let rules = rules();
    let none = Progress::default();

    assert_eq!(
        locked(&rules, &none),
        ["QU06", "QU09", "QU10", "QU11", "XQ08", "XQ12"]
    );

    // The extras hold no slot, so no lock.
    for extra in [FourCc(*b"MD01"), FourCc(*b"VAMP")] {
        assert_eq!(quest_map::lock(extra, &rules, &none), Lock::Open);
    }
}

/// The Tomb needs the Barren Waste, the Goblin Hordes and Free the Slaves;
/// one victory of each meets it.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn winning_a_quests_requirements_opens_it() {
    let rules = rules();
    let mut progress = Progress::default();
    let tomb = SLOTS[8];

    for quest in [b"QU01", b"QU13"] {
        progress.win(FourCc(*quest), 600_000);
    }

    assert_eq!(quest_map::lock(tomb, &rules, &progress), Lock::Locked);

    progress.win(FourCc(*b"QU17"), 600_000);

    assert_eq!(quest_map::lock(tomb, &rules, &progress), Lock::Open);
    assert_eq!(
        quest_map::requires(tomb, &rules),
        Requires::Listed(vec![FourCc(*b"QU01"), FourCc(*b"QU13"), FourCc(*b"QU17")])
    );
    assert_eq!(quest_map::requires(SLOTS[5], &rules), Requires::Southern);
}

/// Every other quest of a campaign won opens its last ones: the Day of
/// Reckoning, Spires of Death and Vigil for a Fallen Hero.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn the_last_quests_open_on_every_other_quest_of_their_campaign() {
    let rules = rules();
    let mut progress = Progress::default();
    let last = [SLOTS[5], SLOTS[26], SLOTS[30]];

    for &quest in SLOTS[..31].iter().filter(|q| !last.contains(q)) {
        progress.win(quest, 600_000);
    }

    assert_eq!(locked(&rules, &progress).len(), 0);
}

/// The title bar's texts, as quest map hover builds them from the
/// expansion's strings.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn the_title_bar_names_a_secret_quests_needs_and_a_won_ones_best_time() {
    let hd = common::hd();
    let library = Library::from_files(&[
        hd.join("Data/gpltext.cam"),
        hd.join("DataMX/mx_gpltext.cam"),
        hd.join("DataMX/mx_textdata.cam"),
    ])
    .unwrap();
    let catalogue = library.catalogue().unwrap();
    let text = Text::build(&catalogue);
    let title = |q: FourCc| text.quest(q).map(std::borrow::Cow::into_owned);
    let rules = rules();
    let mut progress = Progress::default();

    progress.win(SLOTS[0], 1_260_000);
    progress.win(SLOTS[0], 900_000);

    let gmtx = |i: usize| text.gmtx(i).map(std::borrow::Cow::into_owned);
    let bar = |s: usize| quest_map::title_bar(&gmtx, SLOTS[s], (&rules, &progress), &title);
    let (secret, tomb) = bar(8).unwrap();

    assert_eq!(secret, "Secret Quest");
    assert_eq!(
        tomb,
        "\u{1}FFDDAADifficulty: \u{1}E6E1FDExpert  \u{1}FFDDAARequires: \
         \u{1}656565The Barren Waste  \u{1}E6E1FDHold off the Goblin Hordes  \
         \u{1}E6E1FDFree the Slaves  "
    );
    assert!(bar(5).unwrap().1.ends_with("Requires: All Southern Quests"));

    let (name, won) = bar(0).unwrap();

    assert_eq!(name, "The Barren Waste");
    assert!(won.ends_with("Victories: \u{1}E6E1FD2  \u{1}FFDDAABest Time: \u{1}E6E1FD15 Days"));
    assert_eq!(
        bar(1).unwrap().1,
        "\u{1}FFDDAADifficulty: \u{1}E6E1FDBeginner"
    );
}

/// Quest map setup's lifting clouds: its words are control ids
/// where quest lift paired wants quest codes, so with any quest just won
/// for the first time, every other won once, no cloud lifts. Read as
/// quest codes, the same rule lifts the cloud of a quest naming it.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn no_cloud_ever_lifts_in_hd() {
    let rules = rules();
    let codes: Vec<u32> = SLOTS.iter().map(|q| q.word()).collect();
    let tomb = SLOTS[8].word();

    for &last in &SLOTS {
        let mut progress = Progress::default();

        for &quest in SLOTS.iter().filter(|&&q| q != last) {
            progress.win(quest, 600_000);
        }

        progress.win(last, 600_000);

        assert_eq!(
            quest_map::lifted(&quest_map::LIFTS, &progress, &rules),
            None
        );

        // The Tomb names the Free the Slaves, the last slot to name it.
        if last == FourCc(*b"QU17") {
            let lift = quest_map::lifted(&codes, &progress, &rules);

            assert_eq!(lift.and_then(|w| u32::try_from(w).ok()), Some(tomb));
        }
    }
}

/// Read as slots, the table is not the clouded slots either: 29 and 31
/// have no cloud, 2 and 26 are missing.
#[test]
fn the_lifting_table_is_not_the_clouded_slots() {
    let slots: Vec<u32> = quest_map::LIFTS
        .iter()
        .map(|w| w - quest_map::HIT.cast_unsigned())
        .collect();

    assert_eq!(slots, [5, 8, 9, 10, 29, 30, 31]);
}

/// The hint's texts and title are the expansion's `GMTX` 143–147.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn the_map_hints_are_the_expansions_gmtx() {
    let hd = common::hd();
    let library = Library::from_files(&[
        hd.join("Data/gpltext.cam"),
        hd.join("DataMX/mx_gpltext.cam"),
        hd.join("DataMX/mx_textdata.cam"),
    ])
    .unwrap();
    let catalogue = library.catalogue().unwrap();
    let text = Text::build(&catalogue);
    let gmtx = |i: usize| text.gmtx(i).map(std::borrow::Cow::into_owned);

    assert_eq!(
        gmtx(quest_map::HINT_TITLE_TEXT).as_deref(),
        Some("Select A Quest")
    );
    assert!(
        gmtx(quest_map::HINTS[0])
            .unwrap()
            .contains("'The Bell, the Book, and the Candle'")
    );
    assert!(
        gmtx(quest_map::EXPERT_HINT_TEXT)
            .unwrap()
            .starts_with("Try an expert quest")
    );
}

/// The map's two advisors are `DSND`s with a `GVS5` phase: the
/// introduction in the base game's `sounddesc.cam`, the expansion's in
/// its own.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn the_map_advisors_play_their_waves_in_the_voice_phase() {
    use regent_core::sim::world::sound::Voice;
    use regent_core::view::sound::Sounds;

    let hd = common::hd();
    let library = Library::from_files(&[
        hd.join("Data/sounddesc.cam"),
        hd.join("DataMX/mx_sounddesc.cam"),
    ])
    .unwrap();
    let catalogue = library.catalogue().unwrap();
    let sounds = Sounds::of(&catalogue).expect("a sound index");
    let wave = |sound| {
        let phase = sound::Phase::Id(quest_map::ADVISOR_PHASE);

        sounds
            .play(&catalogue, &Voice::Sound(sound), &phase, None, 0.0)
            .map(|p| p.wave)
    };

    assert_eq!(wave(quest_map::INTRODUCTION), Some(FourCc(*b"RA28")));
    assert_eq!(wave(quest_map::EXPANSION), Some(FourCc(*b"RA30")));

    // The briefing's voice is a `DSND` too, played in its first phase.
    let briefing = regent_core::view::menu::voice(SLOTS[0]).unwrap();
    let first = sounds.play(
        &catalogue,
        &Voice::Sound(briefing),
        &sound::Phase::First,
        None,
        0.0,
    );

    assert_eq!(first.map(|p| p.wave), Some(FourCc(*b"EQ00")));
}

/// `APdb` draws in list order: each lit picture before its star and
/// cloud, so a hovered, locked quest shows lit under its cloud
/// (`quest_map::marks`).
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn every_lit_picture_is_listed_before_its_star_and_cloud() {
    let ids: Vec<i32> = panel()
        .controls()
        .iter()
        .filter_map(regent_core::data::smnu::Control::id)
        .collect();
    let at = |id: i32| ids.iter().position(|&i| i == id);

    for slot in 0..34 {
        let lit = at(quest_map::LIT + slot).expect("a lit picture");
        let marks = [quest_map::STAR + slot, quest_map::CLOUD + slot];

        assert!(marks.iter().filter_map(|&m| at(m)).all(|m| m > lit));
    }
}

/// The hovered jewel draws slot 6, its one frame slot 1's lit tile, not
/// slot 0's (**Game**: `ui_menu_map_hover_downloadable.png`).
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn the_hovered_jewel_draws_its_lit_tile() {
    let library = common::library();
    let catalogue = library.catalogue().unwrap();
    let Art { record, set } = quest_map::JEWEL;
    let hover = quest_map::hover(quest_map::DOWNLOAD, Some(quest_map::DOWNLOAD));
    let slot = menu::drawn_slot((Tick::Clear, hover), State::Normal, 7);
    let frames = |s: usize| menu::strip(&catalogue, record, set, s).unwrap().unwrap();

    assert_eq!(slot, menu::HOVER_SLOT);
    assert_eq!(frames(slot).frames.len(), 1);
    assert_eq!(frames(slot).frames, frames(1).frames);
    assert_ne!(frames(slot).frames, frames(0).frames);
}

/// The compass, hovered or its text hovered, plays slot 6: nine frames
/// looping at 150 ms; at rest slot 0, one frame (**Game**: the needle
/// moves in `ui_menu_map_hover_freestyle.png`).
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn the_hovered_compass_loops_nine_frames() {
    let library = common::library();
    let catalogue = library.catalogue().unwrap();
    let strip = |s: usize| {
        menu::strip(&catalogue, FourCc(*b"INPq"), 1039, s)
            .unwrap()
            .unwrap()
    };
    let compass = quest_map::FREESTYLE;
    let slot = |hovered| {
        menu::drawn_slot(
            (Tick::Clear, quest_map::hover(compass, hovered)),
            State::Normal,
            9,
        )
    };
    let lit = strip(slot(Some(quest_map::FREESTYLE_TEXT)));
    let playback = lit.playback.unwrap();

    assert_eq!(slot(Some(compass)), menu::HOVER_SLOT);
    assert_eq!(slot(Some(quest_map::FREESTYLE_TEXT)), menu::HOVER_SLOT);
    assert_eq!(slot(Some(quest_map::DOWNLOAD)), 0);
    assert_eq!(lit.frames.len(), 9);
    assert_eq!(
        (menu::Mode::of(&playback), playback.value),
        (menu::Mode::Loop, 150)
    );
    assert_eq!(strip(0).frames.len(), 1);
}

/// What the jewel and the compass put in the title bar.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn the_jewel_and_compass_name_themselves_in_the_title_bar() {
    let library = common::library();
    let catalogue = library.catalogue().unwrap();
    let text = Text::build(&catalogue);
    let said = |id: i32| {
        let banner = quest_map::banner(id).unwrap();
        let pog = quest_map::picture(&catalogue, banner.pog, 0, 0)
            .unwrap()
            .unwrap();

        (
            text.gmtx(banner.title).unwrap().into_owned(),
            (pog.width, pog.height),
        )
    };

    assert_eq!(said(quest_map::DOWNLOAD).0, "Downloadable Quests");
    assert_eq!(
        said(quest_map::FREESTYLE),
        ("Freestyle Game".to_owned(), (66, 66))
    );
    assert_eq!(
        quest_map::banner(quest_map::FREESTYLE_TEXT),
        quest_map::banner(quest_map::FREESTYLE)
    );
    assert_eq!(quest_map::banner(quest_map::ERASE), None);
}

/// A painted quest's lit picture is the map's own art at its corner,
/// unshifted, under a warm glow that fades out at its edge, in a palette
/// of its own: lit is no function of the map, so it is kept per pixel
/// (openregent's `menu::levels`). Per picture: the mean difference from the
/// map is least at offset (0, 0) within 2 px; opaque pixels within 2 px
/// of its edge differ little (under 16 a channel, at most 13.1: the two
/// palettes, **Inferred**); further in, red rises by over 25 (28 at least).
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "a picture's pixels"
)]
fn a_painted_lit_picture_is_the_maps_art_under_a_glow() {
    use regent_core::view::sprite::Colour;

    let library = common::library();
    let catalogue = library.catalogue().unwrap();
    let map_art = quest_map::map(&panel()).unwrap().art.unwrap();
    let map = quest_map::picture(&catalogue, map_art, 0, 0)
        .unwrap()
        .unwrap();
    let (mw, rgba) = (usize::from(map.width), map.rgba());
    let at = |x: usize, y: usize, c: usize| f64::from(rgba[(y * mw + x) * 4 + c]);
    let palette = |s: &regent_core::view::Sprite| match &s.colour {
        Colour::Indexed { palette, .. } => Some(palette.colours),
        Colour::Direct { .. } => None,
    };

    for site in quest_map::sites(&panel()) {
        let lit = site.lit.unwrap();
        let art = lit.art.unwrap();

        if art.record != FourCc(*b"IX33") {
            continue;
        }

        let sprite = quest_map::picture(&catalogue, art, 1, 0).unwrap().unwrap();
        let (w, h) = (usize::from(sprite.width), usize::from(sprite.height));
        let own = sprite.rgba();
        let opaque = |x: usize, y: usize| own[(y * w + x) * 4 + 3] == u8::MAX;
        let corner = (lit.rect.x as usize, lit.rect.y as usize);
        let pixels: Vec<(usize, usize)> = (0..h)
            .flat_map(|y| (0..w).map(move |x| (x, y)))
            .filter(|&(x, y)| opaque(x, y))
            .collect();
        let off = |(dx, dy): (isize, isize)| {
            let sum: f64 = pixels
                .iter()
                .map(|&(x, y)| {
                    let mx = (corner.0 + x).wrapping_add_signed(dx);
                    let my = (corner.1 + y).wrapping_add_signed(dy);

                    (0..3)
                        .map(|c| (f64::from(own[(y * w + x) * 4 + c]) - at(mx, my, c)).abs())
                        .sum::<f64>()
                })
                .sum();

            sum / pixels.len() as f64
        };
        let best = (-2..=2)
            .flat_map(|dy| (-2..=2).map(move |dx| (dx, dy)))
            .min_by(|a, b| off(*a).total_cmp(&off(*b)));

        assert_eq!(best, Some((0, 0)), "{}", site.quest);
        assert!(palette(&sprite).is_some(), "{}", site.quest);
        assert_ne!(palette(&sprite), palette(&map), "{}", site.quest);

        // Within 2 px of a see-through pixel or the picture's edge.
        let near_edge = |x: usize, y: usize| {
            (-2..=2_isize).any(|dy| {
                (-2..=2_isize).any(|dx| {
                    let (nx, ny) = (x.wrapping_add_signed(dx), y.wrapping_add_signed(dy));

                    nx >= w || ny >= h || !opaque(nx, ny)
                })
            })
        };
        let (edge, inner): (Vec<_>, Vec<_>) = pixels.iter().partition(|&&(x, y)| near_edge(x, y));
        let mean = |set: &[&(usize, usize)], c: usize, f: fn(f64) -> f64| {
            set.iter()
                .map(|&&(x, y)| {
                    f(f64::from(own[(y * w + x) * 4 + c]) - at(corner.0 + x, corner.1 + y, c))
                })
                .sum::<f64>()
                / set.len() as f64
        };

        for c in 0..3 {
            assert!(
                mean(&edge, c, f64::abs) < 16.0,
                "{} channel {c}",
                site.quest
            );
        }

        assert!(mean(&inner, 0, |d| d) > 25.0, "{}", site.quest);
    }
}

/// A downloadable quest's lit picture, `DX31` slot 1, is its slot 0 under
/// the painted quests' glow: slot 0 is the map's art with the building
/// painted in (the map holds none there), slot 1 that, within 3 px of its
/// edge under 2 a channel on average, its middle 40 × 40 red rising over
/// 25. So openregent pastes slot 0 into the map and lights it as it does a
/// painted quest. Only `XQD1` is installed here.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
#[allow(clippy::cast_precision_loss, reason = "a picture's pixels")]
fn a_downloadable_lit_picture_is_its_slot_0_under_a_glow() {
    let library = common::library();
    let catalogue = library.catalogue().unwrap();
    let slot = quest_map::slot(FourCc(*b"XQD1")).unwrap();
    let site = quest_map::sites(&panel())
        .into_iter()
        .find(|s| s.slot == slot)
        .unwrap();
    let art = site.lit.unwrap().art.unwrap();
    let pixels = |s: usize| {
        let sprite = quest_map::picture(&catalogue, art, s, 0).unwrap().unwrap();

        (usize::from(sprite.width), sprite.rgba())
    };
    let ((w, rest), (_, lit)) = (pixels(0), pixels(1));
    let h = rest.len() / 4 / w;
    let diff = |x: usize, y: usize, c: usize| {
        let i = (y * w + x) * 4 + c;

        f64::from(lit[i]) - f64::from(rest[i])
    };
    let mean = |set: &[(usize, usize)], c: usize, f: fn(f64) -> f64| {
        set.iter().map(|&(x, y)| f(diff(x, y, c))).sum::<f64>() / set.len() as f64
    };
    let all: Vec<(usize, usize)> = (0..h).flat_map(|y| (0..w).map(move |x| (x, y))).collect();
    let edge: Vec<_> = all
        .iter()
        .copied()
        .filter(|&(x, y)| x.min(y) < 3 || x + 3 >= w || y + 3 >= h)
        .collect();
    let middle: Vec<_> = all
        .iter()
        .copied()
        .filter(|&(x, y)| x.abs_diff(w / 2) < 20 && y.abs_diff(h / 2) < 20)
        .collect();

    assert_eq!(art.record, FourCc(*b"DX31"));
    assert!(rest.chunks(4).all(|p| p[3] == u8::MAX), "slot 0 opaque");

    for c in 0..3 {
        assert!(mean(&edge, c, f64::abs) < 2.0, "channel {c}");
    }

    assert!(mean(&middle, 0, |d| d) > 25.0);
}
