//! A quest's new game (`openregent --quest`): the quest's own `.q` and
//! dataset, its map seeded with the clock word its traced save was made
//! from (**Measured**, by a retired search).
//!
//! Corpora: `MAJESTY_HD`.

mod common;

use common::read;
use regent_core::content::dataset::{Bytecode, Files, Install};
use regent_core::content::quest::{self, Quest};
use regent_core::data::deps;
use regent_core::data::fourcc::FourCc;
use regent_core::data::rgs;
use regent_core::gpl::load::Program;
use regent_core::gpl::ops::Value;
use regent_core::gpl::runtime::{MS_PER_FRAME, ThreadMode};
use regent_core::gpl::vm::{Config, Vm};
use regent_core::sim::game::{AI_ROOT, Game, Input};
use regent_core::sim::mapgen::custom::Art;
use regent_core::sim::setup;
use regent_core::sim::world::cheat::{Cheat, CheatCode};
use regent_core::sim::world::{CROWN, World};

/// The quests HD offers: `QDEP`'s 33, then the one it lacks.
const OFFERED: usize = 34;

/// GPL images the base and the expansion datasets load.
const BASE_IMAGES: usize = 2;
const EXPANSION_IMAGES: usize = 6;

/// The quest of `key`, as the install defines it.
fn quest(key: [u8; 4]) -> Quest {
    quest::offered(&common::hd(), &[])
        .into_iter()
        .find(|q| q.key == FourCc(key))
        .expect("the quest")
}

fn files(quest: &Quest) -> Files {
    Install::new(&common::hd(), Bytecode::Sdk(&common::sdk()))
        .quest(quest)
        .expect("its files")
}

fn program(files: &Files) -> Program {
    let images: Vec<Vec<u8>> = files.programs.iter().map(|p| read(p)).collect();

    Program::load(&images).expect("the quest's images")
}

/// The quest's `.q` and chart bytes.
fn recipe(quest: &Quest) -> (Vec<u8>, Vec<u8>) {
    (
        read(&quest.template_path().expect("template")),
        read(&files(quest).chart),
    )
}

/// The quest's new game on `map_word`, made with its dataset's archives.
fn begun(quest: &Quest, program: &Program, map_word: i32) -> World {
    let files = files(quest);
    let library = common::library_of(&files.archives);
    let catalogue = library.catalogue().expect("catalogue");
    let (map_bytes, chart_bytes) = recipe(quest);
    let map = rgs::File::parse(&map_bytes)
        .and_then(|f| f.map())
        .expect("the quest");
    let patterns = rgs::File::parse(&chart_bytes)
        .and_then(|f| f.chart())
        .expect("the chart");
    let art = Art::of(&catalogue);
    let made =
        regent_core::sim::mapgen::quest::generate(&map, &patterns, map_word, &art).expect("made");

    setup::fresh(&catalogue, program, &made.start())
}

/// HD offers every quest definition but the two freestyle ones, in
/// `QDEP`'s order, each with its template and every file its dataset
/// names; a base quest loads the base game's two images and its own.
#[test]
fn every_quest_the_install_offers_comes_in_its_dependency_order() {
    let library = common::library();
    let catalogue = library.catalogue().expect("catalogue");
    let rules = catalogue
        .find(b"DATA", FourCc::new(b"QDEP"))
        .and_then(|at| catalogue.record(b"DATA", at))
        .map(|r| deps::quests(&String::from_utf8_lossy(r.data)).expect("QDEP"))
        .expect("a QDEP");
    let order: Vec<FourCc> = rules.iter().map(|r| r.id).collect();
    let offered = quest::offered(&common::hd(), &order);
    let keys: Vec<FourCc> = offered.iter().map(|q| q.key).collect();

    assert_eq!(offered.len(), OFFERED);
    assert_eq!(keys[..order.len()], order[..]);
    assert_eq!(keys.last(), Some(&FourCc::new(b"VAMP")));

    for q in &offered {
        let images = if q.expansion() {
            EXPANSION_IMAGES
        } else {
            BASE_IMAGES
        };

        assert!(q.template_path().is_some(), "{}", q.key);
        assert_eq!(
            files(q).programs.len(),
            images + q.programs.len(),
            "{}",
            q.key
        );
    }

    let krolm = quest(*b"MD01");

    assert!(
        files(&krolm)
            .programs
            .iter()
            .any(|p| p.ends_with("MDL1.bcd"))
    );
    assert_eq!(krolm.name, "MD01");
}

#[test]
fn krolm_appears_when_an_altar_falls() {
    let q = quest(*b"MD01");
    let program = program(&files(&q));
    let mut world = begun(&q, &program, RUN_SEED.cast_signed());
    let vm = Vm::new(&program, Config::default());
    let mut game = Game::free(vm, RUN_SEED, &mut world);

    game.setup(&q.name).expect("setup");

    let altars: Vec<u32> = game
        .world()
        .lore_pieces()
        .into_iter()
        .filter_map(|(id, code, _)| (code == FourCc(*b"BB01")).then_some(id))
        .collect();
    assert!(altars.len() > 1, "{} altars", altars.len());

    game.call(
        "Unit_Call_Deathscript",
        &[regent_core::gpl::ops::Value::Agent(altars[0])],
    )
    .expect("altar death");

    assert!(
        game.world()
            .lore_pieces()
            .iter()
            .any(|(_, code, _)| *code == FourCc(*b"BV01"))
    );
    assert_eq!(game.reports().count, 0, "{:?}", game.reports().first);
}

/// Slay the Mighty Dragon's key, from its `.mqxml` id.
const SLAY_DRAGON: [u8; 4] = *b"QU08";

/// Vendral's art code.
const VENDRAL: FourCc = FourCc(*b"BVg1");

/// `SLAY_DRAGON`'s delay for `Vendral_Spawner`: `RandomNumber(200000) + 280000`.
const SPAWNER_DELAY: std::ops::Range<i32> = 280_000..480_000;

/// How long past the spawner's run the test watches, ms: the longest
/// `SetThreadInterval` it sets, `RandomNumber(240000) + 120000`.
const SPAWNER_NEXT: u32 = 360_000;

/// How often the test mends the palace, ms.
const MEND_EVERY: u32 = 1000;

/// `RESTORATION`, for the crown.
const MEND: CheatCode = CheatCode {
    cheat: Cheat::Restore,
    arg: CROWN,
};

/// `Vendral_Spawner` runs once in the shipped image. `RunThread` makes a
/// once-thread, cleared as its run starts, and
/// `SetThreadInterval` writes the interval only. So its "another vendral"
/// stages never run: after the one on the map at the start, he comes once
/// more, and again only through the sword (`Slay_Dragon_Victory`).
#[test]
fn slay_dragons_spawner_runs_only_once() {
    let q = quest(SLAY_DRAGON);
    let files = Install::new(&common::hd(), Bytecode::Install)
        .quest(&q)
        .expect("its files");
    let program = program(&files);
    let mut world = begun(&q, &program, RUN_SEED.cast_signed());
    let vm = Vm::new(&program, Config::default());
    let mut game = Game::free(vm, RUN_SEED, &mut world);

    game.setup(&q.name).expect("setup");

    // His first appearance: the quest's map places him.
    assert!(
        game.world()
            .lore_pieces()
            .iter()
            .any(|(_, code, _)| *code == VENDRAL)
    );

    // The script object behind the root's `SpecialSpawnScript`.
    let root = game.world().named_any(AI_ROOT).expect("the root");
    let spawner = game
        .threads(root)
        .into_iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("SpecialSpawnScript"))
        .map(|(_, o)| o)
        .expect("the spawner");
    let thread = game.vm().thread(spawner).expect("started").clone();

    assert_eq!(thread.mode, ThreadMode::Once);
    assert!(SPAWNER_DELAY.contains(&thread.interval));

    // Past the run, and past any second run a repeating thread would make.
    let delay = thread.interval.cast_unsigned();

    while game.world().now() <= delay {
        game.frame().expect("frame");
    }

    let after = game.vm().thread(spawner).expect("started").clone();

    assert_eq!(after.mode, ThreadMode::Off);
    assert_ne!(after.interval, thread.interval, "it set its next interval");

    let ran = after.last;

    // Mended each second: an undefended palace falls to him, and a game
    // that ended stops its clock.
    while game.world().now() <= ran + SPAWNER_NEXT {
        let now = game.world().now();

        if now.is_multiple_of(MEND_EVERY) {
            game.input(now / MS_PER_FRAME + 1, Input::Cheat(MEND))
                .expect("in time");
        }

        game.frame().expect("frame");
        assert!(!game.world().humans_ended(), "the palace fell");
    }

    assert_eq!(game.vm().thread(spawner).expect("started").last, ran);
    assert_eq!(game.reports().count, 0, "{:?}", game.reports().first);
}

/// A quest whose crown starts with an outpost and no palace starts on it
/// as on a palace: new game setup centres the view on
/// player palace of and selects it. **Game**: the
/// outpost is selected and centred. `XQ06` so starts.
#[test]
fn a_quest_with_only_an_outpost_starts_on_it() {
    let q = quest(*b"XQ06");
    let program = program(&files(&q));
    let world = begun(&q, &program, 1);
    let outpost = world.player_palace(CROWN).expect("a palace or outpost");

    assert_eq!(&world.opening().pieces[&outpost].code.0[..3], b"ABm");
    assert_eq!(world.selected(), Some(outpost));
}

/// Frames each quest runs: half a game minute.
const RUN_FRAMES: u32 = 600;

/// The GPL seed the quests run on, and their maps' word: the quiet
/// day's.
const RUN_SEED: u32 = 0x0b60_9e2c;

/// Quests whose runs once lost the world a place or an answer, and the
/// frames each runs to show it: the Fortress of Ixmil's and Balance of
/// Twilight's `Hide #TeleportInsideDestination`, the Valley of the
/// Serpents' and the Vigil's healers' `SpawnUnit` of a plant, the Day of
/// Reckoning's `fire_ball` spell unit's `ListObjects`, and the Forsaken
/// Land's `SpawnUnit` of a caravan with `clear`, at 180,000 ms.
const ONCE_LOST: [(&[u8; 4], u32); 6] = [
    (b"XQ03", RUN_FRAMES),
    (b"XQ11", RUN_FRAMES),
    (b"XQ12", RUN_FRAMES),
    (b"XQD1", RUN_FRAMES),
    (b"QU06", RUN_FRAMES),
    (b"QU12", 3_610),
];

/// Parts the quests are run in, each its own test: nextest runs them
/// side by side.
const PARTS: usize = 6;

/// Every quest runs [`RUN_FRAMES`] with no script error. Each of
/// [`ONCE_LOST`] then runs on to
/// its frames with no script error, no place lost and no builtin unknown.
/// Part `part` runs every [`PARTS`]th quest from it.
fn quests_run(part: usize) {
    let quests = quest::offered(&common::hd(), &[]);

    assert!(quests.len() >= PARTS, "{} quests", quests.len());

    for q in quests.into_iter().skip(part).step_by(PARTS) {
        let program = program(&files(&q));
        let mut world = begun(&q, &program, RUN_SEED.cast_signed());
        let vm = Vm::new(&program, Config::default());
        let mut game = Game::free(vm, RUN_SEED, &mut world);

        game.setup(&q.name).expect("setup");

        for _ in 0..RUN_FRAMES {
            game.frame().expect("frame");
        }

        assert_eq!(
            game.reports().count,
            0,
            "{}: {:?}",
            q.key,
            game.reports().first
        );

        let Some((_, frames)) = ONCE_LOST.iter().find(|(key, _)| FourCc(**key) == q.key) else {
            continue;
        };

        for _ in RUN_FRAMES..*frames {
            game.frame().expect("frame");
        }

        assert_eq!(
            game.reports().count,
            0,
            "{}: {:?}",
            q.key,
            game.reports().first
        );
        assert_eq!(game.world().losses().places, 0, "{}", q.key);
        assert!(
            game.faults().unknown.is_empty(),
            "{}: {:?}",
            q.key,
            game.faults().unknown
        );
    }
}

#[test]
fn every_quest_runs_with_no_script_error_part_0() {
    quests_run(0);
}

#[test]
fn every_quest_runs_with_no_script_error_part_1() {
    quests_run(1);
}

#[test]
fn every_quest_runs_with_no_script_error_part_2() {
    quests_run(2);
}

#[test]
fn every_quest_runs_with_no_script_error_part_3() {
    quests_run(3);
}

#[test]
fn every_quest_runs_with_no_script_error_part_4() {
    quests_run(4);
}

#[test]
fn every_quest_runs_with_no_script_error_part_5() {
    quests_run(5);
}

/// Vigil for a Fallen Hero's key.
const VIGIL: [u8; 4] = *b"XQ12";

/// The outpost's code.
const OUTPOST: FourCc = FourCc(*b"ABm1");

/// An outpost's `MaxGuildMembers` (`DUNT ABm1`).
const OUTPOST_SPOTS: usize = 6;

/// By when Vigil's outposts, once seen, are rescued: `Vigil_victory`
/// runs `rescue_buildings` every `#VictoryCondition_callback_frequency`,
/// 4000 ms.
const RESCUED_BY: u32 = 10_000;

/// Agent `id`'s GPL attribute `name`, as the VM holds it.
fn attribute(game: &Game<'_>, id: u32, name: &str) -> Option<Value> {
    game.attributes(id)
        .into_iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, o)| game.vm().object(o).clone())
}

/// Whether agent `id`'s string attribute `name` is `want`.
fn says(game: &Game<'_>, id: u32, name: &str, want: &str) -> bool {
    matches!(attribute(game, id, name), Some(Value::String(s)) if s.eq_ignore_ascii_case(want))
}

/// Agent `id`'s `Members` list.
fn members(game: &Game<'_>, id: u32) -> Vec<u32> {
    match attribute(game, id, "Members") {
        Some(Value::List(l)) => l
            .iter()
            .filter_map(|v| match v {
                Value::Agent(a) => Some(*a),
                _ => None,
            })
            .collect(),
        _ => vec![],
    }
}

/// Vigil for a Fallen Hero's heroes start with no home: the palace houses
/// none (`hero_birth`). A rescued outpost adopts heroes with no home up to
/// its six spots (`Outpost_Birth`, `check_strays`). A fallen outpost's
/// members find a home with room, else go homeless and sulk at an inn
/// (`Outpost_death`, `Find_New_Home`, `homeless`). The scripts' doing,
/// not the engine's.
#[test]
fn vigils_heroes_sulk_when_their_outpost_falls_and_no_home_has_room() {
    let q = quest(VIGIL);
    let program = program(&files(&q));
    let mut world = begun(&q, &program, RUN_SEED.cast_signed());
    let vm = Vm::new(&program, Config::default());
    let mut game = Game::free(vm, RUN_SEED, &mut world);
    let no_home = Some(Value::Agent(u32::MAX));
    let home = |game: &Game<'_>, id| attribute(game, id, "home");

    game.setup(&q.name).expect("setup");

    let heroes: Vec<u32> = game
        .world()
        .lore_pieces()
        .into_iter()
        .map(|(id, ..)| id)
        .filter(|id| says(&game, *id, "subtype", "hero"))
        .collect();

    assert_ne!(heroes.as_slice(), []);
    assert!(heroes.iter().all(|h| home(&game, *h) == no_home));

    // Seen, the outposts are rescued; mended, none falls unasked.
    let reveal = CheatCode {
        cheat: Cheat::Reveal,
        arg: CROWN,
    };

    game.input(1, Input::Cheat(reveal)).expect("in time");

    while game.world().now() < RESCUED_BY {
        let now = game.world().now();

        if now.is_multiple_of(MEND_EVERY) {
            game.input(now / MS_PER_FRAME + 1, Input::Cheat(MEND))
                .expect("in time");
        }

        game.frame().expect("frame");
    }

    let mut outposts: Vec<(u32, Vec<u32>)> = game
        .world()
        .lore_pieces()
        .into_iter()
        .filter(|(_, code, _)| *code == OUTPOST)
        .map(|(id, ..)| (id, members(&game, id)))
        .collect();

    outposts.sort_by_key(|(_, m)| m.len());

    let [(fallen, homed), (kept, full)] = &outposts[..] else {
        panic!("two outposts: {outposts:?}");
    };

    assert_eq!(full.len(), OUTPOST_SPOTS);
    assert_ne!(homed.as_slice(), []);
    assert!(
        homed
            .iter()
            .all(|h| home(&game, *h) == Some(Value::Agent(*fallen)))
    );

    let kill = CheatCode {
        cheat: Cheat::Kill,
        arg: *fallen,
    };
    let now = game.world().now();

    game.input(now / MS_PER_FRAME + 1, Input::Cheat(kill))
        .expect("in time");

    while game.world().now() < now + MEND_EVERY {
        game.frame().expect("frame");
    }

    // The other outpost is full: no member finds a home.
    assert_eq!(members(&game, *kept).len(), OUTPOST_SPOTS);
    assert!(homed.iter().all(|h| home(&game, *h) == no_home));
    assert!(
        homed.iter().any(|h| says(&game, *h, "taskname", "sulking")),
        "{homed:?}"
    );
    assert_eq!(game.reports().count, 0, "{:?}", game.reports().first);
}
