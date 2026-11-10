//! The front end's panels, against HD's files and the captures
//! (`regent_core::view::menu`).

mod common;

use regent_core::content::dataset::Datasets;
use regent_core::content::quest;
use regent_core::data::catalogue::{Catalogue, Library};
use regent_core::data::fourcc::FourCc;
use regent_core::data::imag::Image;
use regent_core::data::payload::Pcm;
use regent_core::data::smnu::{self, Kind, Rect};
use regent_core::data::text::Text;
use regent_core::sim::world::sound;
use regent_core::view::Sprite;
use regent_core::view::menu::faces;
use regent_core::view::menu::font::Font;
use regent_core::view::menu::quest_map;
use regent_core::view::menu::{
    self, Button, Layout, Mode, Release, ShownControl, World, briefing, music, room, row,
};

/// One dataset's archives.
fn library(sets: Datasets) -> &'static Library {
    common::dataset(sets)
}

fn layout(catalogue: &Catalogue<'_>, id: FourCc) -> Layout {
    let text = Text::build(catalogue);

    menu::layout(catalogue, &text, id)
        .expect("the panel ships")
        .expect("it parses")
}

fn picture(catalogue: &Catalogue<'_>, (record, set): (FourCc, i32)) -> Sprite {
    menu::still(catalogue, record, set)
        .unwrap_or_else(|| panic!("{record} {set} is missing"))
        .unwrap_or_else(|e| panic!("{record} {set}: {e}"))
}

#[allow(
    clippy::unnecessary_wraps,
    reason = "compared with a control's optional rectangle"
)]
const fn rect(x: i32, y: i32, width: i32, height: i32) -> Option<Rect> {
    Some(Rect {
        x,
        y,
        width,
        height,
    })
}

/// A sprite's pixel, RGBA.
fn pixel(sprite: &Sprite, rgba: &[u8], x: usize, y: usize) -> [u8; 4] {
    let i = (y * usize::from(sprite.width) + x) * 4;

    [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
}

/// The base `AP29` is the menu `menu_main.png` shows: its buttons, labels,
/// hot keys and art.
#[test]
fn the_main_menu_is_eight_buttons_on_a_bar() {
    let library = library(Datasets::Base);
    let catalogue = library.catalogue().expect("a catalogue");
    let main = layout(&catalogue, menu::MAIN);

    assert_eq!(main.panel.rect, rect(-1, -1, 800, 600));
    assert!(main.centred);

    let shown: Vec<(&str, i32, Option<Rect>)> = Button::SHOWN
        .iter()
        .map(|b| {
            let c = main.control(b.id()).expect("the button");

            assert_eq!(c.kind, Some(Kind::Button));
            assert_eq!(c.art, Some((FourCc(*b"DEM2"), 1006)), "{b:?}");
            assert_eq!(c.font, Some(menu::SMALL_FONT), "{b:?}");

            (
                c.text.as_deref().expect("a label").trim_end(),
                c.hotkey.expect("a key"),
                c.rect,
            )
        })
        .collect();

    assert_eq!(
        shown,
        [
            ("PLAY GAME", 80, rect(24, 536, 140, 26)),
            ("MULTIPLAYER", 77, rect(166, 536, 140, 26)),
            ("CREDITS", 67, rect(495, 536, 140, 26)),
            ("HIGH SCORES", 72, rect(637, 536, 140, 26)),
            ("LOAD GAME", 76, rect(23, 565, 140, 26)),
            ("ADJUST SETTINGS", 65, rect(166, 565, 140, 26)),
            ("CINEMATIC", 73, rect(495, 565, 140, 26)),
            ("EXIT GAME", 27, rect(636, 565, 140, 26)),
        ]
    );

    let cinematic = main.control(Button::Cinematic.id()).expect("CINEMATIC");

    assert_eq!(cinematic.hotkey_at(), Some(1));
    assert_eq!(
        cinematic.colours,
        [
            0x8000_003f_u32.cast_signed(),
            0x8000_00ff_u32.cast_signed(),
            0x4000_0000
        ]
    );
    assert_eq!(
        cinematic.hotkey_colours.first(),
        Some(&0x8000_00ff_u32.cast_signed())
    );
    assert_eq!(
        main.control(Button::Exit.id())
            .and_then(menu::ShownControl::hotkey_at),
        None
    );

    let version = main
        .labelled(menu::VERSION_LABEL)
        .expect("the version text");

    assert_eq!(version.rect, rect(162, 498, 183, 21));

    let room = main.control(1).and_then(|c| c.art).expect("the room");
    let frame = main.control(2000).and_then(|c| c.art).expect("the frame");
    let button = picture(&catalogue, (FourCc(*b"DEM2"), 1006));

    assert_eq!(room, (FourCc(*b"INDm"), 1009));
    assert_eq!(frame, (FourCc(*b"INDm"), 1000));
    assert_eq!((button.width, button.height), (140, 26));

    // At rest, light, dark, and slot 6: one size.
    for slot in [0, 1, 2, 6] {
        let look = menu::state(&catalogue, FourCc(*b"DEM2"), 1006, slot)
            .expect("the slot")
            .expect("it decodes");

        assert_eq!((look.width, look.height), (140, 26), "slot {slot}");
    }
}

/// HD's `UIData` `AP29`: the base buttons, and the mods row under them
/// (`ui_main_menu_hd.png`).
#[test]
fn the_uidata_main_menu_adds_a_mods_row() {
    let uidata = Library::from_files(&[common::hd().join("Data/UIData_1920_1200.dat")])
        .expect("UIData reads");
    let main = layout(&uidata.catalogue().expect("a catalogue"), menu::MAIN);
    let library = library(Datasets::Base);
    let catalogue = library.catalogue().expect("a catalogue");
    let base = layout(&catalogue, menu::MAIN);

    assert_eq!(main.panel.rect, rect(-1, -1, 800, 768));
    assert_eq!(row::bottom(&main), Some(683));
    assert_eq!(row::bottom(&base), None);

    // Above the row, the base layout's buttons where the base puts them,
    // EXIT GAME a pixel right, as the expansion's.
    for b in Button::SHOWN {
        let at = |l: &Layout| l.control(b.id()).and_then(|c| c.rect).map(|r| r.x);
        let right = i32::from(b == Button::Exit);

        assert_eq!(at(&main), at(&base).map(|x| x + right), "{b:?}");
    }

    let row: Vec<_> = row::controls(&main)
        .map(|c| (c.id, c.art, c.rect))
        .collect();
    let surround = Some(row::SURROUND);
    let button = Some((FourCc(*b"DEM2"), 1001));

    assert_eq!(row.len(), 13);
    assert_eq!(row[0], (None, Some(row::STRIP), rect(19, 625, 762, 38)));
    assert!(row.contains(&(None, surround, rect(112, 627, 281, 31))));
    assert!(row.contains(&(None, surround, rect(398, 628, 281, 31))));
    assert!(row.contains(&(Some(row::CHANGE_MODS), button, rect(136, 630, 235, 26))));
    assert!(row.contains(&(Some(row::ACTIVATE_MODS), button, rect(422, 631, 235, 26))));

    let text = |id| main.control(id).and_then(|c| c.text.clone());

    assert_eq!(text(row::CHANGE_MODS).as_deref(), Some("CHANGE MODS"));
    assert_eq!(text(row::ACTIVATE_MODS).as_deref(), Some("ACTIVATE MODS"));

    // The art is the base dataset's.
    let sprite = picture(&catalogue, row::SURROUND);

    assert_eq!((sprite.width, sprite.height), (281, 31));

    for c in row::controls(&main) {
        if let Some(art) = c.art {
            picture(&catalogue, art);
        }
    }
}

/// The briefing's first page, as `menu_barren_waste_start.png` shows it.
#[test]
fn the_briefing_is_a_centred_frame_with_a_porthole_and_a_text() {
    let library = library(Datasets::Base);
    let catalogue = library.catalogue().expect("a catalogue");
    let page = layout(&catalogue, menu::BRIEFING);
    let control = |id| page.control(id).unwrap_or_else(|| panic!("control {id}"));

    assert!(page.centred);
    assert_eq!(page.panel.rect, rect(-1, -1, 800, 600));
    assert_eq!(page.panel.art, Some((FourCc(*b"INDb"), 1002)));

    let frame = picture(&catalogue, (FourCc(*b"INDb"), 1002));

    assert_eq!((frame.width, frame.height), (800, 600));

    let next = control(briefing::NEXT);
    let cancel = control(briefing::CANCEL);

    assert_eq!(
        (next.text.as_deref(), next.hotkey),
        (Some("CONTINUE"), Some(13))
    );
    assert_eq!(
        (cancel.text.as_deref(), cancel.hotkey),
        (Some("CANCEL"), Some(27))
    );
    assert_eq!(next.rect, rect(67, 554, 235, 26));

    let text = control(briefing::BACKGROUND);

    assert_eq!(text.kind, Some(Kind::Textbox));
    assert_eq!(text.rect, rect(309, 122, 447, 382));
    assert_eq!(text.font, Some(menu::LARGE_FONT));
    assert_eq!(text.colours.first(), Some(&0x80ff_ddaa_u32.cast_signed()));

    assert_eq!(control(briefing::PORTHOLE).rect, rect(31, 137, 260, 260));
    assert_eq!(control(briefing::PORTHOLE).art, None);
    assert_eq!(control(briefing::TITLE).font, Some(menu::LARGE_FONT));
    assert_eq!(control(briefing::TITLE).rect, rect(94, 11, 684, 60));
    assert_eq!(control(menu::ids::POG).rect, rect(15, 9, 71, 73));
    assert_eq!(control(menu::ids::SCROLL_BAR).kind, Some(Kind::Scrollbar));
    assert_eq!(
        page.controls
            .iter()
            .find(|c| c.text.as_deref() == Some("BACKGROUND"))
            .and_then(|c| c.font),
        Some(menu::SMALL_FONT)
    );
}

/// `AP11`: a frame, a title, a text, OK over Yes and No, each with its
/// key; Erase Victories shows Yes and No, the hint OK.
#[test]
fn the_message_dialog_holds_a_title_a_text_and_ok_or_yes_and_no() {
    use menu::message::{self, Ask};

    let library = library(Datasets::Base);
    let catalogue = library.catalogue().expect("a catalogue");
    let page = layout(&catalogue, menu::MESSAGE);
    let control = |id| page.control(id).unwrap_or_else(|| panic!("control {id}"));
    let key = |id| (control(id).text.as_deref(), control(id).hotkey);

    assert!(page.centred);
    assert_eq!(page.panel.rect, rect(-1, -1, 251, 323));
    assert_eq!(page.panel.art, Some((FourCc(*b"INDk"), 1000)));
    assert_eq!(control(message::TITLE).rect, rect(32, 47, 187, 26));
    assert_eq!(control(message::TEXT).rect, rect(18, 80, 213, 169));
    assert_eq!(control(message::TEXT).font, Some(menu::SMALL_FONT));
    assert_eq!(key(message::OK), (Some("OK"), Some(13)));
    assert_eq!(key(message::YES), (Some("YES"), Some(i32::from(b'Y'))));
    assert_eq!(key(message::NO), (Some("NO"), Some(i32::from(b'N'))));
    assert_eq!(message::shown(Ask::Question), [0x157d, 0x157e]);
    assert_eq!(message::shown(Ask::Tell), [0x157f]);
}

/// `AP35`: the name prompt's heading, field, Accept and Cancel.
#[test]
fn the_name_prompt_holds_a_field_accept_and_cancel() {
    use menu::prompt;

    let library = library(Datasets::Base);
    let catalogue = library.catalogue().expect("a catalogue");
    let page = layout(&catalogue, menu::NAME_PROMPT);
    let control = |id| page.control(id).unwrap_or_else(|| panic!("control {id}"));
    let key = |id| (control(id).text.as_deref(), control(id).hotkey);

    assert!(page.centred);
    assert_eq!(page.panel.rect, rect(-1, -1, 533, 406));
    assert_eq!(page.panel.art, Some((FourCc(*b"INDn"), 1000)));
    assert_eq!(control(prompt::FIELD).kind, Some(Kind::Textbox));
    assert_eq!(control(prompt::FIELD).rect, rect(49, 266, 431, 33));
    assert_eq!(control(prompt::FIELD).font, Some(FourCc(*b"fnt2")));
    assert_eq!(control(prompt::HEADING).font, Some(menu::LARGE_FONT));
    assert_eq!(
        control(prompt::HEADING).text.as_deref(),
        Some("Please enter your Name...")
    );
    assert_eq!(key(prompt::ACCEPT), (Some("ACCEPT"), Some(13)));
    assert_eq!(key(prompt::CANCEL), (Some("CANCEL"), Some(27)));
}

/// The second page adds the goals and a still over the porthole.
#[test]
fn the_summary_holds_background_and_goals() {
    let library = library(Datasets::Base);
    let catalogue = library.catalogue().expect("a catalogue");
    let page = layout(&catalogue, menu::SUMMARY);
    let control = |id| page.control(id).unwrap_or_else(|| panic!("control {id}"));

    assert_eq!(page.panel.art, Some((FourCc(*b"INDb"), 1000)));
    assert_eq!(control(briefing::NEXT).text.as_deref(), Some("START QUEST"));
    assert_eq!(control(briefing::BACKGROUND).rect, rect(311, 121, 445, 170));
    assert_eq!(control(briefing::BACKGROUND).font, Some(menu::SMALL_FONT));
    assert_eq!(control(briefing::GOALS).rect, rect(311, 339, 445, 170));
    assert_eq!(control(briefing::ZOOM).rect, rect(62, 166, 200, 200));
    assert_eq!(control(briefing::GOALS_SCROLL).kind, Some(Kind::Scrollbar));
    assert_eq!(
        page.controls
            .iter()
            .find(|c| c.text.as_deref() == Some("GOALS"))
            .and_then(|c| c.rect),
        rect(439, 310, 174, 27)
    );
}

/// A control's text keeps to its own rectangle: the warriors' guild's
/// half-band recruit buttons, their price boxes beside it.
#[test]
fn a_controls_text_rectangle_is_read() {
    let library = library(Datasets::Base);
    let catalogue = library.catalogue().expect("a catalogue");
    let page = layout(&catalogue, FourCc(*b"AP52"));
    let text = |id| page.control(id).and_then(|c| c.text_rect);

    assert_eq!(text(5000), rect(3, 1, 56, 25));
    assert_eq!(text(8008), rect(3, 1, 56, 25));
    assert_eq!(text(8910), rect(3, 1, 60, 25));
    assert_eq!(text(8011), None);
}

/// The expansion's panels cover the base logo with its own.
#[test]
fn the_expansions_panels_add_a_logo_cover() {
    let library = library(Datasets::Expansion);
    let catalogue = library.catalogue().expect("a catalogue");

    let main = layout(&catalogue, menu::MAIN);
    let page = layout(&catalogue, menu::BRIEFING);

    assert_eq!(
        main.control(1).and_then(|c| c.art),
        Some((FourCc(*b"IX51"), 1000))
    );
    assert_eq!(
        main.control(2).and_then(|c| c.art),
        Some((FourCc(*b"IX72"), 1004))
    );
    assert_eq!(
        main.control(2).and_then(|c| c.rect),
        rect(309, 533, 184, 60)
    );
    assert_eq!(
        page.control(2).and_then(|c| c.art),
        Some((FourCc(*b"IX72"), 1003))
    );
}

/// `MX17`: the custom-game dialog's frame, its boxes drawn by their
/// style, its lists combo boxes of `INcb`, whose own set and text box
/// draw nothing.
#[test]
fn the_setup_dialog_draws_its_boxes_by_style() {
    use menu::setup;
    use regent_core::data::smnu::style;
    use regent_core::view::menu::bevel::Bevel;

    let library = library(Datasets::Expansion);
    let catalogue = library.catalogue().expect("a catalogue");
    let page = layout(&catalogue, menu::SETUP);
    let control = |id| page.control(id).unwrap_or_else(|| panic!("control {id}"));
    let heading = |text: &str| {
        page.controls
            .iter()
            .find(|c| c.text.as_deref() == Some(text))
            .unwrap_or_else(|| panic!("{text}"))
    };
    let combos = [
        setup::SETTLEMENT,
        setup::SCENARIO,
        setup::MONSTERS,
        setup::LAND,
        setup::VICTORY,
        setup::GOLD,
        setup::LIMITS,
        setup::EVENT_1,
        setup::EVENT_2,
        setup::BUILDINGS,
    ];
    let combo_art = (FourCc(*b"INcb"), 1000);

    assert!(page.centred);
    assert_eq!(page.panel.rect, rect(-1, -1, 800, 600));
    assert_eq!(picture(&catalogue, (FourCc(*b"IX22"), 1002)).width, 800);
    assert_eq!(
        control(setup::TITLE).text.as_deref(),
        Some("Single Player Freestyle Options")
    );

    for id in combos {
        let c = control(id);

        assert_eq!(c.kind, Some(Kind::Combobox), "{id}");
        assert_eq!(c.art, Some(combo_art), "{id}");
        assert_eq!(Bevel::of(c.style), Some(Bevel::Sunken), "{id}");
    }

    let part = |past: i32| menu::still(&catalogue, combo_art.0, combo_art.1 + past);

    assert!(part(0).is_none());
    assert!(part(setup::FIELD).is_none());
    assert_eq!(part(setup::ARROW).map(|s| s.map(|s| s.width)), Some(Ok(25)));
    assert!(part(setup::LIST).is_some());

    // The headings raised, the group boxes and blurbs sunken, all tiled.
    for text in ["PLAYER FORCES", "LANDSCAPE", "MAP NUMBER"] {
        let c = heading(text);

        assert_eq!(Bevel::of(c.style), Some(Bevel::Raised), "{text}");
        assert_ne!(c.style & style::TILED, 0, "{text}");
    }

    for id in [8, 11, 13, 14, setup::SETTLEMENT_TEXT, setup::GOAL_TEXT] {
        let c = control(id);

        assert_eq!(Bevel::of(c.style), Some(Bevel::Sunken), "{id}");
        assert_ne!(c.style & style::TILED, 0, "{id}");
    }

    assert_ne!(control(3643).style & style::BARE, 0);
    assert_eq!(control(setup::PORTRAIT).art, Some((FourCc(*b"IX22"), 1000)));
}

/// Every offered quest's briefing pictures, video and voice ship, but the
/// downloadable quest's video and voice.
#[test]
fn every_quest_has_its_briefing_art() {
    let hd = common::hd();
    let quests = quest::offered(&hd, &[]);

    assert!(quests.len() >= 30, "the quests changed");

    for sets in [Datasets::Base, Datasets::Expansion] {
        let library = library(sets);
        let catalogue = library.catalogue().expect("a catalogue");
        let release = match sets {
            Datasets::Base => Release::Base,
            Datasets::Expansion => Release::Expansion,
        };

        for q in quests
            .iter()
            .filter(|q| q.expansion() == (sets == Datasets::Expansion))
        {
            let code = q.key;
            let Some(slot) = quest_map::slot(code) else {
                // HD's own two quests: the extra quests' icon, no video.
                assert!([*b"MD01", *b"VAMP"].contains(&code.0), "{code} has no slot");
                let icon = menu::icon(code, release, World::Quest).expect("an icon");

                assert_eq!(icon, (FourCc(*b"DX00"), 1000));
                assert_eq!(picture(&catalogue, (icon.0, i32::from(icon.1))).width, 66);
                assert_eq!(menu::video(code), None);
                continue;
            };

            assert_eq!(slot >= 19, q.expansion(), "{code}");

            let zoom = menu::zoom(code, release, World::Quest).expect("a zoom");
            let icon = menu::icon(code, release, World::Quest).expect("an icon");
            let zoomed = picture(&catalogue, (zoom.0, i32::from(zoom.1)));
            let iconic = picture(&catalogue, (icon.0, i32::from(icon.1)));

            assert_eq!((zoomed.width, zoomed.height), (200, 200), "{code}");
            assert_eq!((iconic.width, iconic.height), (66, 66), "{code}");

            let video = menu::video(code).expect("a video");
            let voice = menu::voice(code).expect("a voice");
            let shipped = catalogue.find(b"PICT", video).is_some();

            // The downloadable quest's slot names a video and voice no
            // archive holds.
            assert_eq!(shipped, code.0 != *b"XQD1", "{code}: {video}");
            assert_eq!(
                catalogue.find(b"WAVE", voice).is_some(),
                shipped,
                "{code}: {voice}"
            );
        }
    }

    assert_eq!(menu::video(FourCc(*b"QU01")), Some(FourCc(*b"QM00")));
    assert_eq!(menu::voice(FourCc(*b"QU01")), Some(FourCc(*b"EQ00")));
}

/// Each dataset's loading page holds its still where the freestyle start
/// leaves it, and every offered quest's and freestyle's still ships.
#[test]
fn every_game_has_its_loading_still() {
    let quests = quest::offered(&common::hd(), &[]);

    for (sets, release) in [
        (Datasets::Base, Release::Base),
        (Datasets::Expansion, Release::Expansion),
    ] {
        let library = library(sets);
        let catalogue = library.catalogue().expect("a catalogue");
        let page = layout(&catalogue, menu::LOADING);
        let freestyle = menu::segue(FourCc(*b"FRE1"), release, World::Freestyle).expect("a still");
        let held = page.control(menu::loading::STILL).and_then(|c| c.art);

        assert_eq!(held.map(|a| a.0), Some(freestyle.0), "{sets:?}");
        assert_eq!(
            page.control(menu::loading::STILL).and_then(|c| c.rect),
            rect(24, 97, 750, 375)
        );
        assert!(page.control(menu::loading::TITLE).is_some());
        assert!(page.control(menu::loading::BAR).is_some());

        let stills = quests
            .iter()
            .filter(|q| q.expansion() == (sets == Datasets::Expansion))
            .map(|q| (q.key, menu::segue(q.key, release, World::Quest)))
            .chain([(FourCc(*b"FRE1"), Some(freestyle))]);

        for (code, still) in stills {
            let (record, set) = still.expect("a still");
            let shown = picture(&catalogue, (record, i32::from(set)));

            assert_eq!((shown.width, shown.height), (750, 375), "{code}");
        }
    }
}

/// The backdrop is [`faces::ORIGINAL`]'s portraits on the grid, each pixel
/// [`faces::sepia`] of its own: within 4 a channel on average.
#[test]
fn the_backdrop_is_its_portraits_toned_sepia() {
    let library = library(Datasets::Base);
    let catalogue = library.catalogue().expect("a catalogue");
    let tile = picture(&catalogue, menu::BACKDROP);
    let rgba = tile.rgba();
    let mut error = [0u64; 3];
    let mut count = 0u64;

    for (i, &code) in faces::ORIGINAL.iter().enumerate() {
        let face = picture(&catalogue, (code, faces::PORTRAIT_SET));
        let own = face.rgba();
        let (column, row) = (i % faces::COLUMNS, i / faces::COLUMNS);
        let corner = |c: usize| c * faces::CELL + faces::INSET;

        assert_eq!((face.width, face.height), (100, 100), "{code}");

        for y in 0..100 {
            for x in 0..100 {
                let [r, g, b, _] = pixel(&face, &own, x, y);
                let toned = faces::sepia([r, g, b].into()).channels();
                let shown = pixel(&tile, &rgba, corner(column) + x, corner(row) + y);

                for (e, (a, b)) in error.iter_mut().zip(toned.iter().zip(shown)) {
                    *e += u64::from(a.abs_diff(b));
                }

                count += 1;
            }
        }
    }

    assert!(
        error.iter().all(|e| e / count < 4),
        "{error:?} over {count}"
    );
}

/// The title's font measures the capture's title: "The Barren Waste" is
/// 242 pixels of ink in `menu_barren_waste_start.png`, x 875 to 1116.
#[test]
fn the_large_font_measures_the_captured_title() {
    let library = library(Datasets::Base);
    let catalogue = library.catalogue().expect("a catalogue");
    let at = catalogue.find(b"IMAG", menu::LARGE_FONT).expect("fn11");
    let record = catalogue.record(b"IMAG", at).expect("the record");
    let font = Image::parse(record.data).expect("fn11 parses");

    assert_eq!(record.name(), "Majestica 12");

    let width = Font::of_image(&font).width("The Barren Waste");

    // The advances include the last glyph's trailing gap.
    assert_eq!(width, 244);
}

/// The base room's pictures take the pointer; the expansion's pass it to
/// the triggers, each named for its button.
#[test]
fn the_base_pictures_and_the_expansion_triggers_take_the_pointer() {
    let base = library(Datasets::Base);
    let base = layout(&base.catalogue().expect("a catalogue"), menu::MAIN);
    let expansion = library(Datasets::Expansion);
    let expansion = layout(&expansion.catalogue().expect("a catalogue"), menu::MAIN);
    let takes = |c: &ShownControl| c.flags & smnu::PASSIVE == 0 && c.flags & smnu::CLICKS != 0;

    for (button, id) in room::PICTURES {
        let (b, x) = (base.control(id), expansion.control(id));
        let (b, x) = (b.expect("base picture"), x.expect("expansion picture"));

        assert!(takes(b), "{button:?}");
        assert!(!takes(x), "{button:?}");
        assert_eq!(b.art.map(|(r, _)| r), Some(FourCc(*b"INkm")), "{button:?}");
        assert_eq!(x.art.map(|(r, _)| r), Some(FourCc(*b"IX50")), "{button:?}");
    }

    let named = [
        (Button::Play, "play"),
        (Button::Credits, "credits"),
        (Button::Settings, "levels"),
        (Button::Multiplayer, "mplay"),
        (Button::Load, "load"),
        (Button::Cinematic, "cine"),
        (Button::HighScores, "score"),
        (Button::Exit, "exit"),
    ];

    for (id, button) in room::TRIGGERS {
        let trigger = expansion.control(id).expect("the trigger");
        let tip = trigger.tooltip.as_deref().expect("a tooltip");
        let word = named.iter().find(|(b, _)| *b == button).map(|(_, w)| *w);

        assert!(takes(trigger) && trigger.art.is_none(), "{id}");
        assert!(word.is_some_and(|w| tip.starts_with(w)), "{id}: {tip}");
        assert!(base.control(id).is_none());
    }

    // The map on the table is Play Game in both.
    let under = |layout: &Layout| layout.under((300, 450), &room::HIDDEN).and_then(|c| c.id);

    assert_eq!(under(&base), Some(5100));
    assert_eq!(under(&expansion), Some(7104));
}

/// A picture rests on its first frame, its slot 0 playing back to it; lit,
/// slot 6 plays to the last and holds. The candles loop either way.
#[test]
fn a_room_picture_runs_forward_lit_and_back_at_rest() {
    let library = library(Datasets::Base);
    let catalogue = library.catalogue().expect("a catalogue");
    let strip = |set, slot| {
        let s = menu::strip(&catalogue, FourCc(*b"INkm"), set, slot)
            .expect("the slot")
            .expect("it parses");
        let p = s.playback.expect("a playback");

        (s.frames.len(), Mode::of(&p), p.value)
    };

    // 5102, Multiplayer's.
    assert_eq!(strip(1002, room::REST), (30, Mode::Back, 40));
    assert_eq!(strip(1002, room::LIT), (30, Mode::Forward, 50));
    // 5104, Exit's candles.
    assert_eq!(strip(1000, room::REST), (58, Mode::Loop, 100));
    assert_eq!(strip(1000, room::LIT), (58, Mode::Loop, 50));
}

/// The front end's two tracks are `MusicTracks.txt`'s lines 2 and 6, and
/// both files ship (`menu::music`).
#[test]
fn the_front_ends_tracks_are_the_theme_and_the_epic() {
    let hd = common::hd();
    let list = std::fs::read_to_string(hd.join("Data/MusicTracks.txt")).expect("the track list");
    let lines: Vec<&str> = list.lines().map(str::trim).collect();
    let line = |track: i32| lines[usize::try_from(track - 1).expect("from 1")];

    assert_eq!(line(music::THEME), "GeneralTheme.mp3");
    assert_eq!(line(music::EPIC), "EpicQuest.mp3");

    for track in [music::THEME, music::EPIC] {
        assert!(
            hd.join("Music").join(line(track)).is_file(),
            "{}",
            line(track)
        );
    }
}

/// Each click is a 16-bit `WAVE` in the base game's archives.
#[test]
fn every_click_is_a_shipped_wave() {
    let library = library(Datasets::Base);
    let catalogue = library.catalogue().expect("a catalogue");

    for click in music::CLICKS {
        let at = catalogue.find(b"WAVE", click).expect("the click ships");
        let record = catalogue.record(b"WAVE", at).expect("its record");

        assert!(Pcm::parse(record.data).is_ok(), "{click}");
    }
}

/// Every control the front end presses — the main menu's buttons and room
/// pictures, the briefing's, the map's — clicks `CK05`.
#[test]
fn every_front_end_press_clicks_five() {
    let library = library(Datasets::Base);
    let catalogue = library.catalogue().expect("a catalogue");
    let main = layout(&catalogue, menu::MAIN);
    let pressed = Button::SHOWN
        .iter()
        .map(|b| b.id())
        .chain(room::PICTURES.iter().map(|(_, id)| *id));

    for id in pressed {
        let kind = main.control(id).expect("on the main menu").kind;

        assert_eq!(music::click(kind), Some(music::PRESS), "AP29 {id}");
    }

    for page in [menu::BRIEFING, menu::SUMMARY] {
        for id in [briefing::NEXT, briefing::CANCEL] {
            let kind = layout(&catalogue, page).control(id).expect("a button").kind;

            assert_eq!(music::click(kind), Some(music::PRESS), "{page} {id}");
        }
    }

    let map = quest_map::load(&common::hd().join("Data/UIData_1920_1200.dat")).expect("APdb");
    let hits = (0..quest_map::SLOTS.len()).map(|s| quest_map::HIT + i32::try_from(s).expect("34"));
    let buttons = [quest_map::ERASE, quest_map::MAIN_MENU, quest_map::DOWNLOAD];

    for id in hits.chain(buttons) {
        let kind = map.control(id).and_then(|c| c.class());

        assert_eq!(music::click(kind), Some(music::PRESS), "APdb {id}");
    }
}

/// A won quest's end voice is a `DSND` its own dataset holds, said in its
/// first phase from a shipped `WAVE`, but the downloadable quest's.
#[test]
fn every_won_quest_says_its_end_voice() {
    use regent_core::sim::world::{defeat::End, sound::Voice};
    use regent_core::view::sound::Sounds;

    let hd = common::hd();
    let quests = quest::offered(&hd, &[]);

    for sets in [Datasets::Base, Datasets::Expansion] {
        let library = library(sets);
        let catalogue = library.catalogue().expect("a catalogue");
        let sounds = Sounds::of(&catalogue).expect("a sound index");

        for q in quests
            .iter()
            .filter(|q| q.expansion() == (sets == Datasets::Expansion))
        {
            let code = q.key;
            let said = menu::end_voice(code, End::Won).and_then(|sound| {
                let first = sound::Phase::First;
                let play = sounds.play(&catalogue, &Voice::Sound(sound), &first, None, 0.0)?;

                catalogue
                    .find(b"WAVE", play.wave)
                    .map(|_| (sound, play.wave))
            });

            // `EDnn` says `ENnn`; the downloadable quest's `ED31` does
            // not ship, and HD's own two have none.
            let want = menu::end_voice(code, End::Won)
                .filter(|_| code.0 != *b"XQD1")
                .map(|v| (v, FourCc([b'E', b'N', v.0[2], v.0[3]])));

            assert_eq!(said, want, "{code}");
        }
    }

    assert_eq!(
        menu::end_voice(FourCc(*b"QU01"), End::Won),
        Some(FourCc(*b"ED00"))
    );
}

/// A frame of kind `0x08000000` is no picture: widget draw draws
/// nothing where frame box union finds no tile. Base `INTN` 1039
/// (the spell bar's overlays) and `INPq` 1020 (`AP30` 4107), expansion
/// `IX33` 1001 (`AP30`, `APdb` 4126), hold one in their lit slots only.
#[test]
fn a_frame_of_another_kind_is_no_picture() {
    for (sets, record, set, lit) in [
        (Datasets::Base, *b"INTN", 1039, 2),
        (Datasets::Base, *b"INPq", 1020, 1),
        (Datasets::Expansion, *b"IX33", 1001, 1),
    ] {
        let library = library(sets);
        let catalogue = library.catalogue().expect("a catalogue");
        let art = quest_map::Art {
            record: FourCc(record),
            set,
        };

        assert!(
            menu::state(&catalogue, art.record, set, 0).is_none(),
            "{art:?}"
        );
        assert!(
            quest_map::picture(&catalogue, art, 0, 0).is_none(),
            "{art:?}"
        );
        assert!(
            menu::state(&catalogue, art.record, set, lit).is_some(),
            "{art:?}"
        );
    }
}

/// Each researcher's page (its `DialogID`'s, `view::panel::research::PAGES`)
/// holds a row for each of its researches in `research::BY_BUILDING`,
/// each with a price box; each with an icon at id + 500 but the Magic
/// Bazaar's, whose icons are the panel's own.
#[test]
fn a_research_pages_rows_are_its_researchers_researches() {
    use regent_core::sim::world::research::BY_BUILDING;
    use regent_core::view::panel::research::{self, Row};

    let library = library(Datasets::Expansion);
    let catalogue = library.catalogue().expect("the catalogue");
    let descriptors = common::descriptors();

    for (prefix, ids) in BY_BUILDING {
        let code = FourCc([prefix[0], prefix[1], prefix[2], b'1']);
        let dialog = descriptors.body(code).and_then(|b| b.dialog);
        let page = dialog.and_then(research::page).expect("a research page");
        let layout = layout(&catalogue, page);
        let rows = research::rows(&layout);
        let mut shown: Vec<u32> = rows.iter().map(Row::id).collect();
        let researches: Vec<u32> = ids.into_iter().filter(|i| *i != 0).collect();

        shown.sort_unstable();

        assert_eq!(shown, researches, "{code}");
        assert!(rows.iter().all(|r| r.price.is_some()), "{code}");
        assert_eq!(
            rows.iter().all(|r| r.icon.is_some()),
            page != FourCc(*b"MX03"),
            "{code}"
        );
    }
}

/// The shared ids keep one role in every base panel that holds them
/// (`menu::ids`): 8005 a heading, 8730 the pog but `AP78`'s spell list,
/// 9101 a scroll bar, 8000 and 8001 the zoom and track buttons but
/// `AP64`'s list; 8011 the portrait button.
#[test]
fn each_shared_control_id_keeps_its_role_in_every_panel() {
    use menu::ids;
    use regent_core::view::panel::upper;

    let shared_ids = [
        ids::TITLE,
        ids::POG,
        ids::SCROLL_BAR,
        ids::ZOOM,
        ids::TRACK,
        upper::id::PORTRAIT,
    ];
    let library = library(Datasets::Base);
    let catalogue = library.catalogue().expect("a catalogue");
    let text = Text::build(&catalogue);
    let mut roles: Vec<(i32, Option<Kind>)> = Vec::new();
    let mut spell_list = Vec::new();
    let mut scores_list = Vec::new();

    for (_, record) in catalogue.tagged(b"SMNU") {
        let Some(Ok(layout)) = menu::layout(&catalogue, &text, record.id) else {
            continue;
        };
        let shared = layout.controls.iter().filter_map(|c| {
            c.id.filter(|id| shared_ids.contains(id))
                .map(|id| (id, c.kind))
        });

        for role in shared {
            if role == (ids::POG, Some(Kind::Listbox)) {
                spell_list.push(record.id);
            } else if role == (ids::TRACK, Some(Kind::Listbox)) {
                scores_list.push(record.id);
            } else if !roles.contains(&role) {
                roles.push(role);
            }
        }
    }

    roles.sort_unstable_by_key(|&(id, kind)| (id, format!("{kind:?}")));

    assert_eq!(spell_list, [FourCc(*b"AP78")]);
    assert_eq!(scores_list, [menu::scores::PANEL]);
    assert_eq!(
        roles,
        [
            (ids::ZOOM, Some(Kind::Button)),
            (ids::TRACK, Some(Kind::Button)),
            (ids::TITLE, Some(Kind::Button)),
            (ids::TITLE, Some(Kind::Textbox)),
            (upper::id::PORTRAIT, Some(Kind::Button)),
            (ids::POG, Some(Kind::Button)),
            (ids::SCROLL_BAR, Some(Kind::Scrollbar)),
        ]
    );
}

/// Every panel's palette is `MMS1` but `AP75`'s, which has none; `MMS1`
/// makes text orange at rest, white lit, red disabled, and the hot key
/// letter blue, light blue and red.
#[test]
fn every_panel_takes_its_text_colours_from_mms1() {
    use menu::{Role, State, Style};

    let library = library(Datasets::Base);
    let catalogue = library.catalogue().expect("a catalogue");
    let mut palettes: Vec<(FourCc, Option<FourCc>)> = catalogue
        .tagged(b"SMNU")
        .map(|(_, record)| {
            let panel = smnu::Panel::parse(record.data).expect("it parses");

            (record.id, Style::palette(&panel))
        })
        .filter(|(_, palette)| *palette != Some(FourCc(*b"MMS1")))
        .collect();

    palettes.dedup();
    assert_eq!(palettes, [(FourCc(*b"AP75"), None)]);

    let style = Style::find(&catalogue, Some(FourCc(*b"MMS1"))).expect("MMS1");
    let words = |role| [State::Normal, State::Lit, State::Disabled].map(|s| style.word(role, s));

    assert_eq!(words(Role::Text), [0x80f8_b152, 0x80ff_ffff, 0x80db_2124]);
    assert_eq!(words(Role::HotKey), [0x8052_3bff, 0x8000_a3ff, 0x80de_0000]);
}

/// CREDITS' videos ship: `MV06` in the expansion's archives, `MV01` in
/// the base game's, each a Bink (`PICT` kind 7).
#[test]
fn the_credits_videos_ship() {
    let library = library(Datasets::Expansion);
    let catalogue = library.catalogue().expect("a catalogue");

    for video in menu::CREDITS {
        let at = catalogue.find(b"PICT", video).expect("the video ships");
        let data = catalogue.record(b"PICT", at).expect("its record").data;

        assert_eq!(data.get(..4), Some(&7u32.to_le_bytes()[..]), "{video}");
        assert_eq!(data.get(4..7), Some(&b"BIK"[..]), "{video}");
    }
}

/// `MX20` and `MX21`: the freestyle settings' LOAD and SAVE pages, as
/// the captures show them (`ui_freestyle_options_load.png`, `_save.png`).
#[test]
fn the_freestyle_options_pages_hold_a_list_and_their_buttons() {
    use menu::presets::{self, Mode};

    let library = library(Datasets::Expansion);
    let catalogue = library.catalogue().expect("a catalogue");
    let text = |page: &Layout, id| page.control(id).and_then(|c| c.text.clone());
    let key = |page: &Layout, id| {
        let c = page.control(id).unwrap_or_else(|| panic!("control {id}"));

        (c.text.as_deref().map(str::to_owned), c.hotkey)
    };

    for (mode, title, heading) in [
        (
            Mode::Load,
            "Load Freestyle Options",
            "SAVED FREESTYLE GAMES",
        ),
        (Mode::Save, "Save Freestyle Options", "SAVE FREESTYLE AS"),
    ] {
        let page = layout(&catalogue, mode.panel());
        let shows = |t: &str| page.controls.iter().any(|c| c.text.as_deref() == Some(t));
        let list = page.control(presets::LIST).expect("the list");

        assert!(page.centred);
        assert_eq!(page.panel.rect, rect(-1, -1, 800, 600));
        assert_eq!(page.panel.art, Some((FourCc(*b"IX22"), 1002)));
        assert!(shows(title) && shows(heading), "{mode:?}");
        assert_eq!(list.kind, Some(Kind::Listbox));
        assert_eq!(list.rect, rect(40, 122, 477, 400));
        assert_eq!(
            page.control(menu::ids::SCROLL_BAR).map(|c| c.kind),
            Some(Some(Kind::Scrollbar))
        );
        assert_eq!(
            key(&page, presets::CANCEL),
            (Some("CANCEL".to_owned()), Some(27))
        );
        assert_eq!(
            key(&page, presets::DELETE),
            (Some("DELETE SETTINGS".to_owned()), Some(i32::from(b'D')))
        );
        assert!(page.control(mode.confirm()).is_some());
        assert_eq!(
            page.control(mode.chat_bar()).map(|c| c.kind),
            Some(Some(Kind::Scrollbar))
        );
        assert_eq!(
            page.control(menu::setup::PORTRAIT).and_then(|c| c.art),
            Some((FourCc(*b"IX22"), 1000))
        );
    }

    let load = layout(&catalogue, presets::LOAD_PAGE);
    let save = layout(&catalogue, presets::SAVE_PAGE);

    assert_eq!(
        key(&load, presets::LOAD),
        (Some("LOAD SETTINGS".to_owned()), Some(i32::from(b'L')))
    );
    assert_eq!(
        key(&save, presets::SAVE),
        (Some("SAVE SETTINGS".to_owned()), Some(i32::from(b'S')))
    );
    assert!(load.control(presets::SAVE).is_none() && load.control(presets::NAME).is_none());
    assert!(save.control(presets::LOAD).is_none());
    assert_eq!(
        save.control(presets::NAME).map(|c| (c.kind, c.rect)),
        Some((Some(Kind::Textbox), rect(223, 89, 315, 23)))
    );
    assert_eq!(
        text(&save, presets::NAME).as_deref(),
        Some("Sample settings name")
    );
}

/// The Build Tree Editor `MX18`, each control where `view::menu::tree`
/// names it: the title, the rows' first controls, the list and its bar,
/// and the chat over the picture.
#[test]
fn the_build_tree_editor_holds_its_controls() {
    use menu::tree;

    let library = library(Datasets::Expansion);
    let catalogue = library.catalogue().expect("a catalogue");
    let page = layout(&catalogue, tree::EDITOR);
    let control = |id| page.control(id).unwrap_or_else(|| panic!("control {id}"));
    let text = |id| control(id).text.clone();

    assert_eq!(text(tree::TITLE).as_deref(), Some("Build Tree Editor"));
    assert_eq!(control(tree::POG).art, Some((FourCc(*b"INTD"), 1036)));
    assert_eq!(control(tree::RAIL_TOP).rect, rect(519, 125, 25, 242));
    assert_eq!(control(tree::RAIL_BOTTOM).rect, rect(519, 224, 26, 296));
    assert_eq!(control(tree::SET_ALL).rect, rect(338, 89, 100, 28));
    assert_eq!(text(tree::NAME_LABEL).as_deref(), Some("SAVE AS:"));
    assert_eq!(text(tree::CHAT_HEADING).as_deref(), Some("CHAT ROOM"));
    assert_eq!(
        text(tree::CHAT_HINT).as_deref(),
        Some("Press ENTER to send chat.")
    );
    assert_eq!(text(tree::SEND_CHAT).as_deref(), Some("SEND CHAT"));
    assert_eq!(control(tree::CHAT_BOX).kind, Some(Kind::Textbox));
    assert_eq!(control(tree::CHAT_ENTRY).kind, Some(Kind::Textbox));
    assert_eq!(control(tree::CHAT_BAR).kind, Some(Kind::Scrollbar));
    assert_eq!(control(tree::LIST).rect, rect(20, 125, 350, 400));
    assert_eq!(control(tree::LIST_BOX).rect, rect(13, 118, 503, 412));
    assert_eq!(control(tree::LIST_BOX).art, Some((FourCc(*b"INTI"), 1003)));
    assert_eq!(control(menu::ids::SCROLL_BAR).kind, Some(Kind::Scrollbar));
    assert_eq!(control(tree::BOX).rect, rect(370, 125, 20, 20));
    assert_eq!(control(tree::PLUS).rect, rect(390, 125, 16, 18));
    assert_eq!(control(tree::MINUS).rect, rect(410, 125, 16, 18));
    assert_eq!(control(tree::COUNT).rect, rect(430, 125, 80, 20));

    // The chat lies over the picture, as the setup page's over its
    // portrait.
    let (Some(chat), Some(picture)) = (control(tree::CHAT_BOX).rect, control(tree::PICTURE).rect)
    else {
        panic!("the chat and picture have rectangles");
    };

    assert!(chat.x >= picture.x && chat.x + chat.width <= picture.x + picture.width);
    assert!(chat.y >= picture.y && chat.y + chat.height <= picture.y + picture.height);
}

/// The multiplayer pages: Multiplayer Setup `AP33`, the session dialog
/// `AP55` and the lobby `MX19`, each control where
/// `view::menu::multiplayer` names it; the lobby's seat lines in `GMTX`.
#[test]
fn the_multiplayer_pages_hold_their_controls() {
    use menu::multiplayer::{self, games, lobby, protocol, session};

    let library = library(Datasets::Expansion);
    let catalogue = library.catalogue().expect("a catalogue");
    let words = Text::build(&catalogue);
    let said = |page: &Layout, id| {
        let c = page
            .control(id)
            .unwrap_or_else(|| panic!("{} {id}", page.id));

        (c.kind, c.text.clone().unwrap_or_default(), c.hotkey)
    };
    let button = |text: &str, key: u8| (Some(Kind::Button), text.to_owned(), Some(i32::from(key)));

    let service = layout(&catalogue, multiplayer::PROTOCOL);

    for id in [
        protocol::MODEM,
        protocol::SERIAL,
        protocol::LAN,
        protocol::INTERNET,
    ] {
        assert_eq!(said(&service, id).0, Some(Kind::Radio));
    }
    assert_eq!(said(&service, protocol::ACCEPT), button("ACCEPT", 13));
    assert_eq!(said(&service, protocol::CANCEL), button("CANCEL", 27));

    let setup = layout(&catalogue, multiplayer::GAMES);

    assert_eq!(said(&setup, menu::ids::TITLE).1, "OPEN GAMES");
    assert_eq!(said(&setup, games::LIST).0, Some(Kind::Listbox));
    assert_eq!(said(&setup, menu::ids::SCROLL_BAR).0, Some(Kind::Scrollbar));
    assert_eq!(said(&setup, games::CREATE), button("CREATE GAME", b'C'));
    assert_eq!(said(&setup, games::JOIN), button("JOIN GAME", 13));
    assert_eq!(said(&setup, games::CANCEL), button("CANCEL", 27));
    assert!(setup.control(games::POG).is_some());

    let named = layout(&catalogue, multiplayer::SESSION);

    assert_eq!(named.panel.rect, rect(-1, -1, 474, 276));
    assert_eq!(said(&named, session::NAME).0, Some(Kind::Textbox));
    assert_eq!(said(&named, session::ACCEPT), button("ACCEPT", 13));
    assert_eq!(said(&named, session::CANCEL), button("CANCEL", 27));

    for (id, players) in session::PLAYERS {
        assert_eq!(
            said(&named, id),
            (Some(Kind::Radio), format!("{players} Players"), None)
        );
    }

    let room = layout(&catalogue, multiplayer::LOBBY);

    assert_eq!(said(&room, lobby::TITLE).1, "Session Name");
    assert_eq!(said(&room, lobby::SETTINGS).1, "Settings Display");
    assert_eq!(said(&room, lobby::SETTINGS_BAR).0, Some(Kind::Scrollbar));
    assert_eq!(said(&room, lobby::ADJUST), button("ADJUST SETTINGS", b'A'));
    assert_eq!(said(&room, lobby::LOAD), button("LOAD GAME", b'L'));
    assert_eq!(said(&room, lobby::START), button("START GAME", b'S'));
    assert_eq!(said(&room, lobby::READY), button("Ready", b'R'));
    assert_eq!(said(&room, lobby::CANCEL), button("CANCEL", 27));
    assert_eq!(said(&room, lobby::CHAT).1, "Chat Box Display");
    assert_eq!(said(&room, menu::ids::SCROLL_BAR).0, Some(Kind::Scrollbar));
    assert_eq!(said(&room, lobby::SEND_CHAT), button("SEND CHAT", 13));
    assert_eq!(said(&room, lobby::CHAT_HINT).1, "Press ENTER to send chat");
    assert_eq!(said(&room, lobby::CHAT_ENTRY).0, Some(Kind::Textbox));
    assert_eq!(said(&room, lobby::GAME_NAME_HEADING).1, "GAME NAME");

    for seat in 0..4 {
        let n = seat + 1;

        assert_eq!(said(&room, lobby::NAMES[seat]).1, format!("Player{n} Name"));
        assert_eq!(said(&room, lobby::STATUS[seat]).0, Some(Kind::Check));
        assert_eq!(said(&room, lobby::PICKS[seat]).0, Some(Kind::Combobox));
        assert!(room.control(lobby::BORDERS[seat]).is_some());
        assert!(room.control(lobby::PLATES[seat]).is_some());
    }

    let gmtx = |i| words.gmtx(i).map(|s| s.trim().to_owned());

    assert_eq!(
        gmtx(lobby::status_text(0, false)).as_deref(),
        Some("PLAYER ONE - HOST")
    );
    assert_eq!(
        gmtx(lobby::status_text(1, false)).as_deref(),
        Some("PLAYER TWO - NOT READY")
    );
    assert_eq!(
        gmtx(lobby::status_text(3, true)).as_deref(),
        Some("PLAYER FOUR - READY")
    );
    assert_eq!(gmtx(lobby::READY_TEXT).as_deref(), Some("READY"));
    assert_eq!(gmtx(lobby::NOT_READY_TEXT).as_deref(), Some("NOT READY"));
    assert_eq!(
        gmtx(session::IN_USE_TEXT).as_deref(),
        Some("That session name is already in use.")
    );
}

/// `AP49` and `AP27`, the save and load dialogs, as the captures show
/// them (`ui_save_game.png`, `ui_load_game.png`: 13 rows each), and the
/// strings their controllers read.
#[test]
fn the_save_and_load_dialogs_hold_a_list_descriptions_and_their_buttons() {
    use regent_core::view::menu::saves::{self, Mode};

    let library = library(Datasets::Base);
    let catalogue = library.catalogue().expect("a catalogue");
    let text = Text::build(&catalogue);
    let gmtx = |r: regent_core::data::text::Ref| text.get(r).map(std::borrow::Cow::into_owned);
    let key = |page: &Layout, id| {
        let c = page.control(id).unwrap_or_else(|| panic!("control {id}"));

        (c.text.as_deref().map(str::to_owned), c.hotkey)
    };

    for (mode, title, confirm) in [
        (Mode::Load, "Single Player Load Menu", "LOAD GAME"),
        (Mode::Save, "Save Menu", "SAVE GAME"),
    ] {
        let page = layout(&catalogue, mode.panel());
        let list = page.control(saves::LIST).expect("the list");

        assert!(page.centred);
        assert_eq!(page.panel.rect, rect(-1, -1, 800, 600));
        assert_eq!(key(&page, saves::TITLE).0.as_deref(), Some(title));
        assert_eq!(list.kind, Some(Kind::Listbox));
        assert_eq!(list.rect.map(saves::rows), Some(13));
        assert_eq!(
            page.control(menu::ids::SCROLL_BAR).map(|c| c.kind),
            Some(Some(Kind::Scrollbar))
        );
        assert_eq!(
            key(&page, mode.confirm()),
            (Some(confirm.to_owned()), Some(13))
        );
        assert_eq!(
            key(&page, saves::CANCEL),
            (Some("CANCEL".to_owned()), Some(27))
        );
        assert_eq!(key(&page, saves::DELETE).0.as_deref(), Some("DELETE GAME"));
        assert!(page.control(saves::DESCRIPTION).is_some());
        assert!(page.control(saves::POG).is_some());
    }

    let load = layout(&catalogue, saves::LOAD_PAGE);
    let save = layout(&catalogue, saves::SAVE_PAGE);

    assert!(load.control(saves::NAME).is_none() && load.control(saves::CURRENT).is_none());
    assert_eq!(
        save.control(saves::NAME).map(|c| c.rect),
        Some(rect(223, 95, 491, 22))
    );
    assert!(save.control(saves::CURRENT).is_some());

    assert_eq!(
        gmtx(saves::DESCRIPTION_TEXT).as_deref(),
        Some("%s\nDuration: %s\nSave date: %s\n%s")
    );
    assert_eq!(
        gmtx(saves::LAN_DESCRIPTION_TEXT).as_deref(),
        Some("Duration: %s\nSave date: %s\n%s")
    );
    assert_eq!(
        gmtx(saves::PARTICIPANTS_TEXT).as_deref(),
        Some("Participants:\n")
    );
    assert_eq!(
        gmtx(saves::FREESTYLE_TEXT).as_deref(),
        Some("Freestyle Game")
    );
    assert_eq!(gmtx(saves::SAVED_TEXT).as_deref(), Some("Game Saved."));
    assert_eq!(
        gmtx(saves::DELETE_TITLE).as_deref(),
        Some("Delete Selected Game")
    );
    assert_eq!(
        gmtx(saves::DELETE_TEXT).as_deref(),
        Some("Are you sure you want to delete this saved game?")
    );
    assert_eq!(text.at(&saves::MONTHS, 9).as_deref(), Some("October"));
}

/// `APd8`, the extra quests' list, as `ui_downloaded_quests.png` shows
/// it: the list of 13 rows as `AP27`'s, its buttons and keys, the
/// description box quest extras pick fills, and its pog `DX00`
/// 1000, the jewel's title-bar pog.
#[test]
fn the_extra_quests_list_holds_a_list_a_description_and_two_buttons() {
    use regent_core::view::menu::{extras, saves};

    let library = library(Datasets::Base);
    let catalogue = library.catalogue().expect("a catalogue");
    let page = layout(&catalogue, extras::PANEL);
    let key = |id| {
        let c = page.control(id).unwrap_or_else(|| panic!("control {id}"));

        (c.text.as_deref().map(str::to_owned), c.hotkey)
    };
    let list = page.control(extras::LIST).expect("the list");

    assert!(page.centred);
    assert_eq!(page.panel.rect, rect(-1, -1, 800, 600));
    assert_eq!(key(extras::TITLE).0.as_deref(), Some("Downloaded Quests"));
    assert_eq!(key(extras::LOAD), (Some("LOAD QUEST".to_owned()), Some(13)));
    assert_eq!(key(extras::CANCEL), (Some("CANCEL".to_owned()), Some(27)));
    assert_eq!(list.kind, Some(Kind::Listbox));
    assert_eq!(list.rect.map(saves::rows), Some(13));
    assert_eq!(
        page.control(menu::ids::SCROLL_BAR).map(|c| c.kind),
        Some(Some(Kind::Scrollbar))
    );
    assert_eq!(
        page.control(extras::DESCRIPTION).and_then(|c| c.rect),
        rect(187, 443, 426, 72)
    );
    assert_eq!(
        page.control(extras::POG).and_then(|c| c.art),
        Some((
            quest_map::DOWNLOAD_BANNER.pog.record,
            quest_map::DOWNLOAD_BANNER.pog.set
        ))
    );
}

/// The save and load dialogs' pog is the game's quest's title-bar pog
/// (`SaveGameController`: quest pog record,
/// quest pog set of `CURR`). A freestyle game's quest is `FRE1`,
/// the expansion's definition: `IX11` 1012, the compass, as
/// `ui_menu_map_hover_freestyle.png` shows it. `ui_save_game.png`'s
/// spider is `QU05`'s, `INTD` 1004: that capture's game is the quest
/// "The Dark Forest", whose title its CURRENT GAME DESCRIPTION shows.
#[test]
fn the_save_dialogs_pog_is_the_games_quests() {
    let hd = common::hd();
    let library = library(Datasets::Base);
    let catalogue = library.catalogue().expect("a catalogue");
    let text = Text::build(&catalogue);
    let freestyle = FourCc(*b"FRE1");
    let forest = FourCc(*b"QU05");
    let defined = quest::freestyle(&hd);
    let fre1 = defined.iter().find(|q| q.key == freestyle).expect("FRE1");
    let qu05 = quest::offered(&hd, &[])
        .into_iter()
        .find(|q| q.key == forest)
        .expect("QU05");

    assert!(fre1.expansion());
    assert_eq!(
        menu::icon(freestyle, Release::Expansion, World::Freestyle),
        Some((FourCc(*b"IX11"), 1012))
    );

    assert!(!qu05.expansion());
    assert_eq!(qu05.title(&text), "The Dark Forest");
    assert_eq!(
        menu::icon(forest, Release::Base, World::Quest),
        Some((FourCc(*b"INTD"), 1004))
    );
}

/// The menus' dataset, the expansion's, holds two `STRT QUES`: the
/// base's alone names `MD01`, the expansion's alone `XQ01`. The game
/// merges them, so both are named, and `MD01` is
/// "The Wrath of Krolm", not its `DisplayName`. **Game**:
/// `ui_downloaded_quests.png`.
#[test]
fn the_menus_name_quests_from_both_quest_tables() {
    let hd = common::hd();
    let library = library(Datasets::Expansion);
    let catalogue = library.catalogue().expect("a catalogue");
    let text = Text::build(&catalogue);
    let krolm = quest::offered(&hd, &[])
        .into_iter()
        .find(|q| q.key == FourCc(*b"MD01"))
        .expect("MD01");

    assert_eq!(krolm.display.as_deref(), Some("Wrath of Krolm"));
    assert_eq!(krolm.title(&text), "The Wrath of Krolm");
    assert_eq!(
        text.quest(FourCc(*b"XQ01")).as_deref(),
        Some("The Clash of Empires")
    );
}

/// By position too, the merged `QUES` holds both tables' quests: the
/// first archive's rows, then the keys it lacks (`Text::at`).
#[test]
fn a_merged_table_by_position_holds_both_copies() {
    let library = library(Datasets::Expansion);
    let catalogue = library.catalogue().expect("a catalogue");
    let text = Text::build(&catalogue);
    let rows: Vec<String> = (0..)
        .map_while(|i| text.at(b"QUES", i))
        .map(std::borrow::Cow::into_owned)
        .collect();

    assert!(rows.iter().any(|r| r == "The Wrath of Krolm"), "{rows:?}");
    assert!(rows.iter().any(|r| r == "The Clash of Empires"), "{rows:?}");
}

/// `AP04`: "Really exit Majesty?", YES and NO with their keys.
#[test]
fn the_quit_question_is_yes_or_no() {
    use menu::{quit, room};

    let library = library(Datasets::Base);
    let catalogue = library.catalogue().expect("a catalogue");
    let page = layout(&catalogue, quit::PANEL);
    let control = |id| page.control(id).unwrap_or_else(|| panic!("control {id}"));
    let key = |id| (control(id).text.as_deref(), control(id).hotkey);

    assert!(page.centred);
    assert_eq!(key(quit::YES), (Some("YES"), Some(i32::from(b'Y'))));
    assert_eq!(key(quit::NO), (Some("NO"), Some(i32::from(b'N'))));
    assert_eq!(quit::YES, 0x157d);
    assert_eq!(room::LEAVING, menu::Mode::Forward);
    assert!(room::left(29, 30) && !room::left(28, 30) && room::left(0, 0));
}
