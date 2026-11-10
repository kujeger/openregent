//! A game saved at frame N and resumed plays on as the game never saved
//! did: at N + M the two snapshots are one, record for record, every RNG
//! state with them. Over a new freestyle game, the largest map and a quest;
//! each at several frames. A snapshot reads back
//! from its RON text unchanged.
//!
//! ```text
//!  world ── free game ──> N ──> N + M        snapshot b
//!                         └─ snapshot ─ RON ─ resumed on a fresh world
//!                                             ──> N + M  snapshot a == b
//! ```
//!
//! Corpora: `MAJESTY_HD`.

mod common;

use common::read;
use std::collections::BTreeMap;

use regent_core::content::dataset::{Bytecode, Install};
use regent_core::content::quest;
use regent_core::data::catalogue::Library;
use regent_core::data::fourcc::FourCc;
use regent_core::data::rgs;
use regent_core::gpl::load::Program;
use regent_core::gpl::startup;
use regent_core::gpl::vm::text::FloatText;
use regent_core::gpl::vm::{Config, Vm};
use regent_core::save::Snapshot;
use regent_core::sim::freestyle;
use regent_core::sim::freestyle::Picks;
use regent_core::sim::game::{Game, Input};
use regent_core::sim::mapgen::custom::{self, Art};
use regent_core::sim::mapgen::fresh;
use regent_core::sim::setup;
use regent_core::sim::world::{CROWN, World};

/// The quiet day's GPL seed, openregent's default.
const SEED: u32 = 0x0b60_9e2c;

/// The freestyle quest and the chart, under the install.
const QUEST: &str = "QuestsMX/mx_Freestyle.q";
const CHART: &str = "DataMX/mx_constants.rgs";

/// Lines of context a difference shows.
const CONTEXT: usize = 6;

/// How a case's game runs: its settings and what it is given.
struct Run<'w> {
    config: Config,
    float_text: FloatText,
    /// A fresh world, made as the game's was.
    world: &'w dyn Fn() -> World,
    /// The setup's script.
    script: &'w str,
    /// Input taken before the setup.
    inputs: Vec<(u32, Input)>,
    /// Input that names what the world holds at its frame.
    late: &'w dyn Fn(&World, u32) -> Vec<Input>,
}

impl Run<'_> {
    fn vm<'p>(&self, program: &'p Program) -> Vm<'p> {
        let mut vm = Vm::new(program, self.config);

        vm.set_float_text(self.float_text);
        vm
    }

    /// One frame, its late input first.
    fn step(&self, game: &mut Game<'_>) {
        let frame = game.frames + 1;

        for input in (self.late)(game.world(), frame) {
            game.input(frame, input).expect("on time");
        }

        game.frame().expect("a frame runs");
    }
}

/// A snapshot as pretty RON, read back unchanged.
fn ron(snapshot: &Snapshot) -> String {
    let config = ron::ser::PrettyConfig::default().compact_arrays(true);
    let text = ron::ser::to_string_pretty(snapshot, config).expect("serialises");
    let back: Snapshot = ron::from_str(&text).expect("reads back");

    assert!(
        back == *snapshot,
        "a snapshot reads back from RON unchanged"
    );
    text
}

/// Where two RON texts first part, with context: `None` if one.
fn parted(a: &str, b: &str) -> Option<String> {
    let (a, b): (Vec<&str>, Vec<&str>) = (a.lines().collect(), b.lines().collect());
    let at = a
        .iter()
        .zip(&b)
        .position(|(x, y)| x != y)
        .or_else(|| (a.len() != b.len()).then(|| a.len().min(b.len())))?;
    let from = at.saturating_sub(CONTEXT);
    let shown = |lines: &[&str]| {
        lines[from.min(lines.len())..(at + CONTEXT).min(lines.len())]
            .iter()
            .map(|l| l.chars().take(SHOWN).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    };

    Some(format!(
        "line {at}:\n--- resumed\n{}\n--- never saved\n{}",
        shown(&a),
        shown(&b)
    ))
}

/// zstd's level, as the save file's.
const LEVEL: i32 = 3;

/// A line shown of a difference, at most.
const SHOWN: usize = 160;

/// The game run whole to the last save plus `more`, snapshots taken at
/// each of `saves` and `more` frames on; each save resumed on a fresh
/// world and run `more` frames: its snapshot the whole run's. Returns the
/// sizes of the snapshots taken at `saves`: RON, and RON compressed.
fn gate(program: &Program, run: &Run<'_>, saves: &[u32], more: u32) -> Vec<(usize, usize)> {
    let last = saves.iter().max().copied().unwrap_or(0) + more;
    let mut taken: BTreeMap<u32, Snapshot> = BTreeMap::new();
    let vm = run.vm(program);
    let mut game = Game::free(vm, SEED, (run.world)());

    for (frame, input) in &run.inputs {
        game.input(*frame, *input).expect("on time");
    }

    game.setup(run.script).expect("setup");

    let wanted = |f: u32| saves.contains(&f) || saves.iter().any(|s| s + more == f);

    while game.frames < last {
        run.step(&mut game);

        if wanted(game.frames) {
            taken.insert(game.frames, game.snapshot().expect("snapshot"));
        }
    }

    let w = game.world();

    // A game still going: its frames did something to resume.
    assert!(w.ended(CROWN).is_none(), "the game ended");
    println!(
        "at frame {last}: {} pieces, treasury {:?}, {} input",
        w.shown().len(),
        w.treasury(CROWN),
        run.inputs.len()
    );

    saves
        .iter()
        .map(|&at| {
            let text = ron(&taken[&at]);
            let snapshot: Snapshot = ron::from_str(&text).expect("reads");
            let vm = run.vm(program);
            let mut resumed = Game::resumed(vm, (run.world)(), snapshot).expect("resumes");

            assert_eq!(resumed.frames, at);

            while resumed.frames < at + more {
                run.step(&mut resumed);
            }

            let a = ron(&resumed.snapshot().expect("snapshot"));
            let b = ron(&taken[&(at + more)]);

            if let Some(apart) = parted(&a, &b) {
                panic!("saved at {at}, at {}: {apart}", at + more);
            }

            let packed = zstd::encode_all(text.as_bytes(), LEVEL).expect("compresses");

            (text.len(), packed.len())
        })
        .collect()
}

/// The start trace's picks: `Wizb`, `Ruib`, `NONE`, `KxHa`, map 1.
fn freestyle(program: &Program) -> World {
    picked(program, &Picks::default())
}

/// A custom game on `picks`.
fn picked(program: &Program, picks: &Picks) -> World {
    let hd = common::hd();
    let find = |name: &str| regent_core::content::on_disk(&hd.join(name)).expect("on disk");
    let (quest, chart) = (read(&find(QUEST)), read(&find(CHART)));
    let map = rgs::File::parse(&quest)
        .and_then(|f| f.map())
        .expect("quest");
    let patterns = rgs::File::parse(&chart)
        .and_then(|f| f.chart())
        .expect("chart");
    let library = common::library();
    let catalogue = library.catalogue().expect("catalogue");
    let art = Art::of(&catalogue);
    let choice = custom::chosen(picks, &map).expect("choice");
    let made = fresh::generate(&map, &patterns, choice, freestyle::GOLD, &art).expect("made");

    setup::fresh(&catalogue, program, &made.start())
}

fn sdk_program() -> Program {
    let images: Vec<Vec<u8>> = common::sdk_images().iter().map(|p| read(p)).collect();

    Program::load(&images).expect("SDK loads")
}

/// A new freestyle game, no input, openregent's settings: saved at its
/// first frame, a day in, three days in.
#[test]
fn a_freestyle_game_resumed_plays_on_as_never_saved() {
    let program = sdk_program();
    let world = || freestyle(&program);
    let run = Run {
        config: Config::default(),
        float_text: FloatText::default(),
        world: &world,
        script: startup::FREESTYLE,
        inputs: Vec::new(),
        late: &|_, _| Vec::new(),
    };
    let sizes = gate(&program, &run, &[1, 1_242, 3_600], 1_200);

    println!("snapshot bytes, RON and compressed: {sizes:?}");
}

/// The largest map, 512 cells a side: saved at its first frame and a
/// minute in.
#[test]
fn a_largest_map_resumed_plays_on_as_never_saved() {
    let program = sdk_program();
    let picks = Picks {
        side: Some(LARGEST),
        players: None,
        ..Picks::default()
    };
    let world = || picked(&program, &picks);
    let run = Run {
        config: Config::default(),
        float_text: FloatText::default(),
        world: &world,
        script: startup::FREESTYLE,
        inputs: Vec::new(),
        late: &|_, _| Vec::new(),
    };
    let sizes = gate(&program, &run, &[1, 1_200], 300);

    println!("snapshot bytes, RON and compressed: {sizes:?}");
}

/// Cells a side of the largest map.
const LARGEST: u32 = 512;

/// The Barren Waste (`QU01`) on its traced map word.
const BARREN: &[u8; 4] = b"QU01";
const BARREN_WORD: i32 = 0x0725_f9f9;

/// The quest's game, no input, in its own dataset: saved at 30 s, a
/// day, two days.
#[test]
fn a_quest_resumed_plays_on_as_never_saved() {
    let quest = quest::offered(&common::hd(), &[])
        .into_iter()
        .find(|q| q.key == FourCc(*BARREN))
        .expect("the quest");
    let files = Install::new(&common::hd(), Bytecode::Sdk(&common::sdk()))
        .quest(&quest)
        .expect("its files");
    let images: Vec<Vec<u8>> = files.programs.iter().map(|p| read(p)).collect();
    let program = Program::load(&images).expect("the quest's images");
    let library = Library::from_files(&files.archives).expect("archives");
    let catalogue = library.catalogue().expect("catalogue");
    let (template, chart) = (
        read(&quest.template_path().expect("template")),
        read(&files.chart),
    );
    let map = rgs::File::parse(&template)
        .and_then(|f| f.map())
        .expect("the quest");
    let patterns = rgs::File::parse(&chart)
        .and_then(|f| f.chart())
        .expect("the chart");
    let art = Art::of(&catalogue);
    let world = || {
        let made = regent_core::sim::mapgen::quest::generate(&map, &patterns, BARREN_WORD, &art)
            .expect("made");

        setup::fresh(&catalogue, &program, &made.start())
    };
    let run = Run {
        config: Config::default(),
        float_text: FloatText::default(),
        world: &world,
        script: &quest.name,
        inputs: Vec::new(),
        late: &|_, _| Vec::new(),
    };

    gate(&program, &run, &[600, 1_214, 2_458], 1_200);
}
