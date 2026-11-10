//! A mod's inline patch on the made-up game ([`regent_testgame`]), with no
//! game data: number fields set on a descriptor, member values on a `.dat`
//! instance.

use std::path::{Path, PathBuf};

use regent_core::content::flags;
use regent_core::content::mods;
use regent_core::data::catalogue::Catalogue;
use regent_core::data::desc::{BUILDING_NUMBERS, CHARACTER_NUMBERS, Character, Number, Part, Unit};
use regent_core::data::fourcc::FourCc;
use regent_core::gpl::load::Program;
use regent_core::gpl::ops::Value;
use regent_core::gpl::startup;
use regent_core::gpl::vm::{Config, Vm};
use regent_core::sim::game::Game;
use regent_core::sim::scene::{Placed, Scene};
use regent_testgame::{HUT, KEEP, LODGE, TestGame, WALKER, only};

/// `text` as one patch file.
fn file(text: &str) -> Vec<(PathBuf, String)> {
    vec![(PathBuf::from("patch.toml"), text.to_owned())]
}

/// The game's archives, then `text` compiled over them.
fn archives(text: &str) -> Vec<(String, Vec<u8>)> {
    let game = TestGame::get();
    let patch = flags::compile(&game.catalogue(), &file(text)).expect("the patch compiles");

    game.archives()
        .iter()
        .cloned()
        .chain([("patch".to_owned(), patch)])
        .collect()
}

/// The record `code` in `catalogue`, parsed.
fn unit<'a>(catalogue: &'a Catalogue<'_>, code: FourCc) -> Unit<'a> {
    let at = catalogue.find(b"DUNT", code).expect("the unit");
    let record = catalogue.record(b"DUNT", at).expect("its record");

    Unit::parse(&Part::parse(record.data).expect("a part")).expect("a unit")
}

/// A manifest's number and member entries make its inline patch, the
/// text a save hashes; a bad one refuses the mod at the scan, saying why.
#[test]
fn a_manifest_carries_numbers_and_members() {
    let head = "version = 1\nid = '{56A39291-8971-4975-BB9A-C4F140535107}'\n\
                name = 'T'\nbase = 'Any'\n";
    let unit = "[[unit]]\nid = 'ZTL1'\nset = { MaxGuildMembers = 8 }\n";
    let good = format!("{head}{unit}{PROTOTYPE}");
    let manifest = Path::new("m/mod.toml");
    let modded = mods::parse_toml(&good, manifest, Path::new("m")).expect("the mod");
    let text = modded.inline_patch.expect("a patch");

    assert!(text.contains("MaxGuildMembers"), "{text}");
    assert!(text.contains("[[prototype]]"), "{text}");

    // A patch of units alone says nothing of members.
    let units = mods::parse_toml(&format!("{head}{unit}"), manifest, Path::new("m"))
        .expect("the mod")
        .inline_patch
        .expect("a patch");

    assert!(!units.contains("prototype"), "{units}");

    let bad = [
        (
            "[[unit]]\nid = 'ZTL1'\nset = { Nonsense = 1 }\n",
            "unknown number field Nonsense",
        ),
        (
            "[[unit]]\nid = 'ZTL1'\nset = { MaxHP = -1 }\n",
            "MaxHP cannot hold -1",
        ),
        (
            "[[unit]]\nid = 'ZTL1'\nset = { MaxHP = 4294967296 }\n",
            "MaxHP cannot hold",
        ),
        (
            "[[unit]]\nid = 'ZTL1'\nadd = ['Heals']\n",
            "add and clear need a field",
        ),
        (
            "[[prototype]]\ninstance = 'Hut'\nset = { left = [1] }\n",
            "Hut.left cannot hold [1]",
        ),
        (
            "[[prototype]]\nset = { left = 1 }\n",
            "missing field `instance`",
        ),
    ];

    for (body, why) in bad {
        let refused = mods::parse_toml(&format!("{head}{body}"), manifest, Path::new("m"))
            .expect_err(why)
            .to_string();

        assert!(refused.contains(why), "{refused}");
    }
}

/// A character's number readers, in [`CHARACTER_NUMBERS`]' order.
type Reader<'a> = fn(&Character<'a>) -> Option<u32>;

fn readers<'a>() -> [(&'static str, Reader<'a>); 23] {
    [
        ("Cost", Character::cost),
        ("Experience", Character::experience),
        ("MaxHP", Character::max_hp),
        ("SightRange", Character::sight_range),
        ("Speed", Character::speed),
        ("AttackRangeMin", Character::attack_range_min),
        ("AttackRangeMax", Character::attack_range_max),
        ("Vitality", Character::vitality),
        ("Charisma", Character::charisma),
        ("Artifice", Character::artifice),
        ("WillPower", Character::will_power),
        ("Intelligence", Character::intelligence),
        ("Strength", Character::strength),
        ("MagicResistance", Character::magic_resistance),
        ("Attack", Character::attack),
        ("RangedAttack", Character::ranged_attack),
        ("Parry", Character::parry),
        ("Dodge", Character::dodge),
        ("WeaponBasicDamage", Character::weapon_damage),
        ("ArmorBasicDamage", Character::armor_damage),
        ("Gold", Character::gold),
        ("RecruitDelay", Character::recruit_delay),
        ("PrimaryStat", Character::primary_stat),
    ]
}

/// A number patch writes each field its reader reads, and nothing else
/// of the record; a field of the other kind, or none, is refused.
#[test]
fn a_number_patch_sets_each_descriptor_field() {
    let value = |n: usize| 100 + u32::try_from(n).expect("small");
    let walker: Vec<String> = (0..)
        .zip(CHARACTER_NUMBERS)
        .map(|(n, (name, _, _))| format!("{name} = {}", value(n)))
        .collect();
    let text = format!(
        "version = 1\n[[unit]]\nid = 'ZTW1'\nset = {{ {} }}\n\
         [[unit]]\nid = 'ZTL1'\nset = {{ Cost = 7, Multiplier = 1.5, IncomeType = 2, \
         IncomeAmount = -3, MaxHP = 90, MaxGuildMembers = 5, SightRange = 9, Gold = 11 }}\n",
        walker.join(", ")
    );
    let patched = archives(&text);
    let catalogue = Catalogue::build(&patched).expect("the catalogue");
    let walker = unit(&catalogue, WALKER);
    let walker = walker.character().expect("a character");

    let character = readers();

    for (n, ((name, read), (field, _, kind))) in character.iter().zip(CHARACTER_NUMBERS).enumerate()
    {
        assert_eq!(*name, field);
        assert_eq!(kind, Number::Count);
        assert_eq!(read(&walker), Some(value(n)), "{name}");
    }

    let lodge = unit(&catalogue, LODGE);
    let lodge = lodge.building().expect("a building");

    assert_eq!(BUILDING_NUMBERS.len(), 8);
    assert_eq!(lodge.cost(), Some(7));
    assert_eq!(lodge.multiplier(), Some(1.5));
    assert_eq!(lodge.income_type(), Some(2));
    assert_eq!(lodge.income_amount(), Some(-3));
    assert_eq!(lodge.max_hp(), Some(90));
    assert_eq!(lodge.max_guild_members(), Some(5));
    assert_eq!(lodge.sight_range(), Some(9));
    assert_eq!(lodge.gold(), Some(11));

    // One word changed: the flags and every other field stay.
    let game = TestGame::get();
    let before = game.catalogue();
    let record = |c: &Catalogue<'_>| {
        let at = c.find(b"DUNT", LODGE).expect("the lodge");

        c.record(b"DUNT", at).expect("a record").data.to_vec()
    };
    let one = archives("version = 1\n[[unit]]\nid = 'ZTL1'\nset = { MaxGuildMembers = 5 }\n");
    let after = Catalogue::build(&one).expect("the catalogue");
    let (a, b) = (record(&before), record(&after));

    assert_eq!(a.len(), b.len());
    assert_eq!(a.iter().zip(&b).filter(|(x, y)| x != y).count(), 1);

    for (text, why) in [
        (
            "id = 'ZTW1'\nset = { MaxGuildMembers = 8 }",
            "wrong unit type",
        ),
        ("id = 'ZZZZ'\nset = { MaxHP = 8 }", "unknown unit"),
    ] {
        let refused = flags::compile(&before, &file(&format!("version = 1\n[[unit]]\n{text}\n")))
            .expect_err(why)
            .to_string();

        assert!(refused.contains(why), "{refused}");
    }
}

/// A placed lodge of a patched descriptor recruits against the patch's
/// `MaxGuildMembers`.
#[test]
fn a_number_patch_reaches_the_world() {
    let archives = archives("version = 1\n[[unit]]\nid = 'ZTL1'\nset = { MaxGuildMembers = 5 }\n");
    let catalogue = Catalogue::build(&archives).expect("the catalogue");
    let program = TestGame::get().program();
    let scene = Scene::flat(128)
        .unit(Placed::new(KEEP, [64.0, 64.0]))
        .unit(Placed::new(LODGE, [50.0, 40.0]));
    let mut world = scene.flat_world(&catalogue, program).expect("a world");
    let staging = scene.staging(&world).expect("staged");
    let mut game = Game::free(Vm::new(program, Config::RELEASE), 1, &mut world);

    staging.run(&mut game, startup::FREESTYLE).expect("set up");

    let lodge = only(game.world(), LODGE);

    assert_eq!(game.world().members(lodge).expect("a guild").spots, 5);
}

/// The member entry the member tests set: a hut made already risen.
const PROTOTYPE: &str = "[[prototype]]\ninstance = 'Hut'\nset = { left = 1 }\n";

/// `text`'s member values over the game's program; `None` when it sets
/// none.
fn members(text: &str) -> Result<Option<Program>, String> {
    TestGame::get()
        .program()
        .with_members(&file(text))
        .map_err(|e| e.to_string())
}

/// A member entry sets an instance's text, the case its prototype
/// spells; a hut made of it holds the value, one of the game's own the
/// `.dat` value.
#[test]
fn a_member_patch_sets_an_instances_value() {
    let patched = members(&format!("version = 1\n{PROTOTYPE}"))
        .expect("the patch applies")
        .expect("a program");
    let left = |program: &Program| {
        let set = program.data_set("Hut").expect("the instance");

        set.values
            .iter()
            .find(|(k, _)| program.names().get(*k) == "left")
            .map(|(_, v)| program.names().get(*v).to_owned())
    };

    assert_eq!(left(&patched), Some("1".to_owned()));
    assert_eq!(left(TestGame::get().program()), None);
    assert!(members("version = 1\n").expect("empty").is_none());

    let catalogue = TestGame::get().catalogue();
    let made = |program: &Program| {
        let scene = Scene::flat(128)
            .unit(Placed::new(KEEP, [64.0, 64.0]))
            .unit(Placed::new(HUT, [50.0, 40.0]));
        let mut world = scene.flat_world(&catalogue, program).expect("a world");
        let staging = scene.staging(&world).expect("staged");
        let mut game = Game::free(Vm::new(program, Config::RELEASE), 1, &mut world);

        staging.run(&mut game, startup::FREESTYLE).expect("set up");

        let hut = only(game.world(), HUT);
        let agent = game.world().agent(hut).expect("an agent");
        let member = |name: &str| {
            agent
                .values
                .iter()
                .find(|m| m.name == name)
                .map(|m| m.value.clone())
        };

        (member("left"), member("type"))
    };
    let hut = Some(Value::String("hut".to_owned()));

    assert_eq!(made(&patched), (Some(Value::Integer(1)), hut.clone()));
    assert_eq!(
        made(TestGame::get().program()),
        (Some(Value::Integer(0)), hut)
    );
}

/// A member entry the program cannot take is refused, naming why; a
/// function value naming one is taken.
#[test]
fn a_member_patch_refuses_what_does_not_fit() {
    let entry = |instance: &str, set: &str| {
        format!("version = 1\n[[prototype]]\ninstance = '{instance}'\nset = {{ {set} }}\n")
    };

    for (instance, set, why) in [
        ("Nowhere", "left = 1", "unknown .dat instance Nowhere"),
        ("Hut", "right = 1", "Hut has no member right"),
        ("Hut", "left = -1", "Hut.left cannot hold -1"),
        ("Hut", "left = 'x'", "Hut.left cannot hold \"x\""),
        ("Hut", "left = 1.5", "Hut.left cannot hold 1.5"),
        ("Hut", "type = 1", "Hut.type cannot hold 1"),
        ("Hut", "brain = 'nothing_at_all'", "Hut.brain cannot hold"),
        ("Hut", "goal = '1 2'", "Hut.goal cannot hold"),
    ] {
        let refused = members(&entry(instance, set)).expect_err(why);

        assert!(refused.contains(why), "{refused}");
        assert!(refused.starts_with("patch.toml: "), "{refused}");
    }

    let brain = members(&entry("hut", "Brain = 'walker_brain'"))
        .expect("a function's name")
        .expect("a program");
    let set = brain.data_set("Hut").expect("the instance");

    assert!(set.values.iter().any(|(k, v)| {
        brain.names().get(*k) == "brain" && brain.names().get(*v) == "walker_brain"
    }));
}
