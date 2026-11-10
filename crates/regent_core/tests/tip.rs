//! The tooltip line (`regent_core::view::panel::tip`): its place, font and palette
//! in HD's data, the `GMTX` lines it composes with, and the captures'
//! texts composed from the game's own; the map's unit line from a
//! traced save's world.
//!
//! Corpora: `MAJESTY_HD`.

mod common;

use regent_core::content::unit::Unit;
use regent_core::data::catalogue::{Catalogue, Library};
use regent_core::data::fourcc::FourCc;
use regent_core::data::smnu::{self, Panel};
use regent_core::data::text::Text;
use regent_core::sim::world::offer::SPELLS;
use regent_core::view::menu;
use regent_core::view::panel::spell_bar::BUTTONS;
use regent_core::view::panel::tip::{self, Able, Count, Rules, SKILLS, Spell, Upgrade, line};

/// HD's main screen at the captures' size.
const UIDATA: &str = "Data/UIData_1920_1200.dat";

/// The spell bar.
const BAR: FourCc = FourCc::new(b"AP92");

/// `ui_tooltip_spell_*.png`'s lines, as the original drew them.
const FARSEEING: &str = "Cast Farseeing.- (200) gold";
const HEALING: &str = "Cast Healing.- (Unavailable: requires Temple to Fervus)";
const VINES: &str = "Cast Vines.- (Unavailable: requires Temple to Fervus Level 3)";

/// Spell `code`'s button's own tooltip in `AP92`.
fn own(catalogue: &Catalogue<'_>, text: &Text<'_>, code: [u8; 4]) -> String {
    let control = BUTTONS
        .iter()
        .find(|b| b.spell == FourCc(code))
        .map(|b| b.control)
        .expect("a button");
    let layout = menu::layout(catalogue, text, BAR)
        .expect("the bar ships")
        .expect("it parses");

    layout
        .control(control)
        .and_then(|c| c.tooltip.clone())
        .unwrap_or_else(|| panic!("{control} has no tooltip"))
}

/// Descriptor `code`'s description.
fn named(catalogue: &Catalogue<'_>, code: FourCc) -> String {
    Unit::find(catalogue, code)
        .and_then(Result::ok)
        .map_or_else(|| panic!("no {code}"), |u| u.desc.head.title().into_owned())
}

/// The captures' three lines, each the button's own text and what
/// spell tip adds: Farseeing's price (`#SpellCost_Farseeing`,
/// 200), Healing's and Vines' temple at their stages.
#[test]
fn the_captures_spell_lines_are_composed_from_the_games_text() {
    let library = common::library();
    let catalogue = library.catalogue().expect("catalogue");
    let text = Text::build(&catalogue);
    let row = |code: &[u8; 4]| {
        SPELLS
            .iter()
            .find(|r| r.spell == FourCc::new(code))
            .expect("a row")
    };
    let needs = |code: &[u8; 4]| named(&catalogue, row(code).needs());

    let farseeing = tip::spell(&text, &own(&catalogue, &text, *b"Sp10"), Spell::Ready(200));
    let healing = tip::spell(
        &text,
        &own(&catalogue, &text, *b"Sp22"),
        Spell::Needs(&needs(b"Sp22")),
    );
    let vines = tip::spell(
        &text,
        &own(&catalogue, &text, *b"Sp23"),
        Spell::Needs(&needs(b"Sp23")),
    );

    assert_eq!(farseeing, FARSEEING);
    assert_eq!(healing, HEALING);
    assert_eq!(vines, VINES);
    assert_eq!(
        tip::spell(&text, "Cast SuperCharge.", Spell::InEffect),
        "Cast SuperCharge.- Unavailable: Already in effect"
    );
}

/// Each line the controllers name reads as upgrade tooltip and
/// the methods use it.
#[test]
fn the_gmtx_lines_are_the_ones_the_controllers_name() {
    let library = common::library();
    let catalogue = library.catalogue().expect("catalogue");
    let text = Text::build(&catalogue);
    let at = |n: usize| text.gmtx(n).map(std::borrow::Cow::into_owned);

    for (n, want) in [
        (line::RECRUIT_PRICE, "- (%d) gold"),
        (line::RECRUIT_COUNT, "(%d of %d)"),
        (line::UPGRADE_TO, "Upgrade to "),
        (line::PRICE, "- (%d) gold"),
        (line::UNAVAILABLE, "- (Unavailable: requires %s)"),
        (line::RESEARCH_DONE, "Research Complete"),
        (line::NO_UPGRADE, "This building has no upgrade."),
        (line::FULLY_UPGRADED, "This building is fully upgraded."),
        (line::REPAIR_FIRST, "Building needs to be repaired first."),
        (line::NO_CALL, "No members to Call to Arms."),
        (line::NO_VISITORS, "No visitors present"),
        (line::GUILD_FULL, "The guild is full"),
        (line::NO_MEMBERS, "No members present"),
        (line::NO_BUILDINGS, "No buildings available"),
        (line::NO_SEEKERS, "No heroes interested"),
        (line::DETAILED, "Detailed Information"),
        (
            line::UPGRADE_BARRED,
            "No upgrades of this type are permitted",
        ),
    ] {
        assert_eq!(at(n).as_deref(), Some(want), "GMTX {n}");
    }

    let toggles: Vec<String> = tip::TOGGLES
        .iter()
        .flat_map(|&(_, clear, ticked)| [clear, ticked])
        .filter_map(at)
        .collect();

    assert_eq!(
        toggles,
        [
            "Tax this building once.",
            "Don't tax this building.",
            "Add this building to the tax route.",
            "Remove this building from the tax route.",
            "Repair this building once.",
            "Don't repair this building.",
            "Add this building to the repair route.",
            "Remove this building from the repair route.",
        ]
    );
    assert_eq!(
        tip::upgrade(&text, Upgrade::Heroes { have: 2, need: 4 }).as_deref(),
        Some("You have only 2 Heroes of 4 needed.")
    );
    assert_eq!(
        tip::upgrade(
            &text,
            Upgrade::Offer {
                to: "Palace Level 2",
                price: 2000
            }
        )
        .as_deref(),
        Some("Upgrade to Palace Level 2- (2000) gold")
    );
    assert_eq!(
        tip::recruit(&text, "Recruit a Ranger ", (1, 4), 350),
        "Recruit a Ranger (2 of 4)- (350) gold"
    );
}

/// `APMK`'s control 2004 at 1920 × 1200 places the line; its font is the
/// manager's, `fnt7`, and `TTIP` and `fnt7` ship.
#[test]
fn the_line_stands_in_the_bottom_bars_message_box() {
    let hd = common::hd();
    let uidata = Library::from_files(&[hd.join(UIDATA)]).expect("UIData");
    let catalogue = uidata.catalogue().expect("a catalogue");
    let (_, record) = catalogue
        .tagged(b"SMNU")
        .find(|(_, r)| r.id == FourCc::new(b"APMK"))
        .expect("APMK");
    let panel = Panel::parse(record.data).expect("it parses");
    let place = panel.control(tip::PLACE).expect("2004");
    let rect = place.rect().expect("a rectangle");

    assert_eq!(
        (rect.x, rect.y, rect.width, rect.height),
        (281, 1173, 1556, 26)
    );
    assert_eq!(place.font(), Some(tip::FONT));

    let library = common::library();
    let shipped = library.catalogue().expect("catalogue");

    assert!(shipped.find(b"PALT", tip::PALETTE).is_some());
    assert!(shipped.find(b"IMAG", tip::FONT).is_some());
}

/// No shipped control sets the window's flag bits, so every line stands
/// above its point.
#[test]
fn no_control_sets_the_lines_window_flags() {
    let hd = common::hd();
    let mut files: Vec<_> = ["Data", "DataMX"]
        .iter()
        .flat_map(|d| std::fs::read_dir(hd.join(d)).expect("a data directory"))
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "dat" || x == "cam"))
        .collect();

    files.sort();

    let mut panels = 0;

    for path in files {
        let Ok(library) = Library::from_files(std::slice::from_ref(&path)) else {
            continue;
        };
        let Ok(catalogue) = library.catalogue() else {
            continue;
        };

        for (_, record) in catalogue.tagged(b"SMNU") {
            let Ok(panel) = Panel::parse(record.data) else {
                continue;
            };

            panels += 1;

            for c in panel.controls() {
                assert_eq!(
                    c.flags() & tip::WINDOW_FLAGS,
                    0,
                    "{} {} {:?}",
                    path.display(),
                    record.id,
                    c.word(smnu::CONTROL)
                );
            }
        }
    }

    assert!(panels > 0);
}

/// A line by its count alone, and the doing's joint.
#[test]
fn a_units_numbers_follow_its_name() {
    let library = common::library();
    let catalogue = library.catalogue().expect("catalogue");
    let text = Text::build(&catalogue);
    let told = |count| tip::Told {
        named: 1,
        code: FourCc::new(b"AVj1"),
        count,
        doing: None,
    };

    assert_eq!(
        tip::unit(&text, "Peasant", &told(Count::Gold(7))),
        "Peasant (7 gold)"
    );
    assert_eq!(tip::unit(&text, "Peasant", &told(Count::None)), "Peasant");
    assert_eq!(text.gmtx(line::DOING).as_deref(), Some(" is "));
}

/// The mini view rows say their first row's texts; back, cycle and
/// forward "All filters are off" while disabled; column 9 adds its hint.
/// `APMK` holds the first two rows.
#[test]
fn the_mini_view_rows_say_the_first_rows_texts() {
    let hd = common::hd();
    let uidata = Library::from_files(&[hd.join(UIDATA)]).expect("UIData");
    let catalogue = uidata.catalogue().expect("a catalogue");
    let base = Library::from_files(&[hd.join("Data/textdata.cam")]).expect("textdata");
    let base = base.catalogue().expect("a catalogue");
    let text = Text::build(&base);
    let (_, record) = catalogue
        .tagged(b"SMNU")
        .find(|(_, r)| r.id == FourCc::new(b"APMK"))
        .expect("APMK");
    let panel = Panel::parse(record.data).expect("it parses");
    let layout = menu::resolved(&panel, &text, record.id, None);
    let own = |c: i32| layout.control(c).and_then(|c| c.tooltip.clone());
    let first = tip::MINI_ROWS[0];

    for row in &tip::MINI_ROWS[..2] {
        assert!(row.iter().all(|c| layout.control(*c).is_some()), "{row:?}");
    }

    for (column, (&control, &mirror)) in first.iter().zip(&tip::MINI_ROWS[1]).enumerate() {
        let want = own(control).map(|t| match column {
            tip::HINTED => format!("{t}{}", tip::MINI_HINT),
            _ => t,
        });

        assert_eq!(
            tip::mini(&text, mirror, Able::Enabled, own),
            want,
            "{column}"
        );
    }

    for (column, _) in tip::FILTERS_OFF {
        assert_eq!(
            tip::mini(&text, first[column], Able::Disabled, own).as_deref(),
            Some("All filters are off")
        );
    }

    assert_eq!(tip::mini(&text, tip::PLACE, Able::Enabled, own), None);
}

/// The skill rows as `HeroAttribController` words them, each cap
/// from `globals.gpl` under the rules it reads.
#[test]
fn the_skill_rows_name_their_caps() {
    let library = common::library();
    let catalogue = library.catalogue().expect("catalogue");
    let text = Text::build(&catalogue);
    let program = common::sdk_program();
    let line = |control: i32| {
        let skill = SKILLS.iter().find(|s| s.control == control).expect("a row");

        tip::skill(&text, skill, skill.cap.value(&program, Rules::Standard))
    };
    let caps: Vec<i32> = SKILLS
        .iter()
        .map(|s| s.cap.value(&program, Rules::Deathmatch))
        .collect();

    assert_eq!(
        line(5007).as_deref(),
        Some("This hero's current hand to hand skill - There is a cap of 95 on this skill.")
    );
    assert_eq!(
        line(5010).as_deref(),
        Some("This hero's current dodge skill - There is a cap of 95 on this skill.")
    );
    assert_eq!(
        line(5011).as_deref(),
        Some(
            "This hero's current magic resistance - In deathmatch play all heroes have a \
             minimum of 30 in this skill."
        )
    );
    assert_eq!(caps, [95, 95, 85, 85, 30]);
}

/// The Wizards' Tower's enchant button adds `#WizTowerUpgradeCost`.
#[test]
fn the_towers_enchant_line_adds_its_price() {
    let library = common::library();
    let catalogue = library.catalogue().expect("catalogue");
    let text = Text::build(&catalogue);

    assert_eq!(
        tip::tower(&text, "Enchant Tower ", 500),
        "Enchant Tower - (500) gold"
    );
}
