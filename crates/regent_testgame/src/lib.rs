//! A small made-up game, all authored here, so tests with no game data run
//! the real simulation. No retail bytes or text.
//!
//! ```text
//!  fixtures/units.xml ──> desc::xml (a mod's path) ──┐
//!  art.rs: IMAG, DMOV ──> cam::write ────────────────┴─> Catalogue
//!  fixtures/rules.gpl ──> emit::compile_source ─┐
//!  fixtures/agents.dat ─> emit::compile ────────┴─> Program
//!  Scene::flat ──> flat_world ──> Game ──> frames
//! ```
//!
//! | Unit | Code | What it does (`fixtures/rules.gpl`) |
//! |---|---|---|
//! | Keep | [`KEEP`] | The palace. A full treasury wins; falling loses |
//! | Hut | [`HUT`] | Bought; rises to full health, then pays |
//! | Lodge | [`LODGE`] | Bought; recruits walkers |
//! | Walker | [`WALKER`] | Walks east; hits a biter beside it, a roll |
//! | Biter | [`BITER`] | Hunts walkers, huts, the keep; bites, a roll |
//!
//! What the engine asks for by name, so this game must spell as it does:
//! the scripts `InitializeGpl`, `NewUnitInit`, `Palace_Pre_Birth`,
//! `UnitDelete`, `Unit_Call_Deathscript` and `Generate_Character_Attributes`;
//! the agent member `type`; the player field `gold`; `#ATTRIB_HP` and
//! `#ATTRIB_MaxHP`; and a palace's code prefix `ABJ` ([`KEEP`]). The rest of
//! the names, numbers and codes are invented.
//!
//! [`TOUGH_WALKERS`] is the game's one mod: a `mod.toml` raising the
//! walker's `MaxHP` from 30 to [`TOUGH_WALKER_HP`]; [`TestGame::modded`]
//! is the game's archives with its patch after them.

use std::sync::OnceLock;

use regent_core::content::{flags, mods};
use regent_core::data::cam::{self, ChunkFlag, Written};
use regent_core::data::catalogue::Catalogue;
use regent_core::data::desc::xml;
use regent_core::data::fourcc::FourCc;
use regent_core::data::imag::SetKind;
use regent_core::gpl::load::Program;
use regent_core::gpl::startup;
use regent_core::gpl::vm::{Config, Vm};
use regent_core::gpl::{dat, emit};
use regent_core::sim::game::check::Dropped;
use regent_core::sim::game::{Game, Order};
use regent_core::sim::scene::Scene;
use regent_core::sim::world::World;
use regent_core::sim::world::loss_log::Lost;

mod art;

/// The palace. Its prefix is the engine's ([`regent_core::gpl::startup::PALACES`]);
/// the last letter is invented.
pub const KEEP: FourCc = FourCc::new(b"ABJZ");
pub const HUT: FourCc = FourCc::new(b"ZTH1");
pub const LODGE: FourCc = FourCc::new(b"ZTL1");
pub const WALKER: FourCc = FourCc::new(b"ZTW1");
pub const BITER: FourCc = FourCc::new(b"ZTB1");

/// Every code in the game's archives, with its chunk: none may be a retail
/// one (`the_games_codes_are_no_retail_records`).
pub const CODES: [(&[u8; 4], FourCc); 8] = [
    (b"DUNT", KEEP),
    (b"DUNT", HUT),
    (b"DUNT", LODGE),
    (b"DUNT", WALKER),
    (b"DUNT", BITER),
    (b"DMOV", TROT),
    (b"IMAG", BUILDING_ART),
    (b"IMAG", BODY_ART),
];

const TROT: FourCc = FourCc::new(b"ZMV1");
const BUILDING_ART: FourCc = FourCc::new(b"ZIB1");
const BODY_ART: FourCc = FourCc::new(b"ZIC1");

/// A building's sets: construction, damage, lit, unlit and ruin.
const BUILDING_SETS: [SetKind; 6] = [
    SetKind::Build,
    SetKind::Die,
    SetKind::Active,
    SetKind::Inactive,
    SetKind::Dead,
    SetKind::Crumble,
];

/// A body's sets.
const BODY_SETS: [SetKind; 5] = [
    SetKind::Walk,
    SetKind::Stand,
    SetKind::Attack,
    SetKind::Die,
    SetKind::Dead,
];

/// A building's pad, screen pixels: left, top, right, bottom.
const BUILDING_PAD: art::Rect = [-48, -36, 48, 36];

/// A body's pad.
const BODY_PAD: art::Rect = [-8, -6, 8, 6];

/// Frames of a body's sets: its walk's steps are timed from them.
const BODY_FRAMES: u16 = 4;

/// [`TROT`]: turn ms, directions a turn, step ms, 32nds of a cell a step.
const TROT_GAIT: [u32; 4] = [50, 4, 50, 4];

/// The agent of the `n`th unit a scene places, 0 first: the root takes the
/// first id ([`regent_core::gpl::startup`]), units follow in order.
#[must_use]
pub const fn placed(n: u32) -> u32 {
    const FIRST_UNIT: u32 = 2;

    FIRST_UNIT + n
}

/// The game, once a process.
pub struct TestGame {
    archives: Vec<(String, Vec<u8>)>,
    program: Program,
}

impl TestGame {
    /// The game, built from its source the first time asked.
    ///
    /// # Panics
    ///
    /// A fixture the real readers refuse.
    #[must_use]
    pub fn get() -> &'static TestGame {
        static GAME: OnceLock<TestGame> = OnceLock::new();

        GAME.get_or_init(TestGame::build)
    }

    fn build() -> TestGame {
        let base = vec![("art.cam".to_owned(), base_archive())];
        let units = unit_archive(&Catalogue::build(&base).expect("the base archive"));
        let archives = base.into_iter().chain([("units.cam".to_owned(), units)]);
        let data = dat::parse(AGENTS);

        assert!(data.is_ok(), "agents.dat: {:?}", data.errors);

        let code = emit::compile_source(RULES, "rules.gpl").expect("rules.gpl compiles");
        let images = [code, emit::compile(&[data])];

        TestGame {
            archives: archives.collect(),
            program: Program::load(&images).expect("the images load"),
        }
    }

    /// The game's archives, named, in load order: a test's own archive,
    /// a mod's patch, goes after them.
    #[must_use]
    pub fn archives(&self) -> &[(String, Vec<u8>)] {
        &self.archives
    }

    /// The game's program with `source`'s scripts loaded after its own,
    /// as a mod's are: a test's own functions.
    ///
    /// # Panics
    ///
    /// A source that does not compile.
    #[must_use]
    pub fn program_with(&self, source: &str) -> Program {
        let code = emit::compile_source(RULES, "rules.gpl").expect("rules.gpl compiles");
        let data = emit::compile(&[dat::parse(AGENTS)]);
        let extra = emit::compile_source(source, "extra.gpl").expect("the source compiles");

        Program::load(&[code, data, extra]).expect("the images load")
    }

    /// The game's archives indexed, as an install's are.
    ///
    /// # Panics
    ///
    /// Never for the fixtures, which [`TestGame::get`] has read.
    #[must_use]
    pub fn catalogue(&self) -> Catalogue<'_> {
        Catalogue::build(&self.archives).expect("the archives")
    }

    /// The game's archives, then [`TOUGH_WALKERS`]' patch, as a game
    /// with the mod loads them.
    ///
    /// # Panics
    ///
    /// Never for the fixtures: the mod and its patch read.
    #[must_use]
    pub fn modded(&self) -> Vec<(String, Vec<u8>)> {
        let manifest = std::path::Path::new(TOUGH_WALKERS_FILE);
        let parsed = mods::parse_toml(TOUGH_WALKERS, manifest, std::path::Path::new(""))
            .expect("the mod reads");
        let text = parsed.inline_patch.expect("a patch");
        let patch = flags::compile(&self.catalogue(), &[(manifest.to_path_buf(), text)])
            .expect("the patch compiles");

        self.archives
            .iter()
            .cloned()
            .chain([("tough_walkers".to_owned(), patch)])
            .collect()
    }

    #[must_use]
    pub const fn program(&self) -> &Program {
        &self.program
    }

    /// `run` played: its setup, then its frames, `each` told of every one.
    ///
    /// # Panics
    ///
    /// A scene the game cannot make; a setup or frame that stops.
    pub fn play(&self, run: &Run, mut each: impl FnMut(u32, &World)) -> Played {
        let catalogue = self.catalogue();
        let mut world = run
            .scene
            .flat_world(&catalogue, &self.program)
            .expect("a world");
        let staging = run.scene.staging(&world).expect("staged");
        let vm = Vm::new(&self.program, Config::RELEASE);
        let mut game = Game::free(vm, run.seed, &mut world);
        let mut played = Played::default();

        staging.run(&mut game, startup::FREESTYLE).expect("set up");

        for (frame, order) in &run.inputs {
            game.input(*frame, *order).expect("a frame to come");
        }

        for frame in 0..run.frames {
            game.frame().expect("a frame");
            played.digests.push(game.digest());
            each(frame, game.world());
            played.lost.extend(game.take_lost());
            played.dropped.extend(game.take_dropped());
        }

        played.errors = game.reports().first.clone();
        drop(game);
        played.world = world;
        played
    }
}

/// The game's mod, a `mod.toml`: the walker's `MaxHP` raised.
pub const TOUGH_WALKERS: &str = include_str!("../fixtures/tough_walkers.toml");

/// [`TOUGH_WALKERS`]' file name, as a mod folder holds it.
pub const TOUGH_WALKERS_FILE: &str = mods::SOURCE_MANIFEST;

/// The walker's `MaxHP` under [`TOUGH_WALKERS`].
pub const TOUGH_WALKER_HP: u32 = 90;

const UNITS: &str = include_str!("../fixtures/units.xml");
const RULES: &str = include_str!("../fixtures/rules.gpl");
const AGENTS: &str = include_str!("../fixtures/agents.dat");

/// What XML cannot make: the art and the movement class.
fn base_archive() -> Vec<u8> {
    let trot = art::movement(TROT.0, "Trot", TROT_GAIT);
    let building = art::image(&BUILDING_SETS, BUILDING_PAD, 1);
    let body = art::image(&BODY_SETS, BODY_PAD, BODY_FRAMES);

    cam::write(&[
        Written {
            tag: FourCc::new(b"DMOV"),
            flag: ChunkFlag::Clear,
            records: vec![(TROT, "Trot", trot.as_slice())],
        },
        Written {
            tag: FourCc::new(b"IMAG"),
            flag: ChunkFlag::Clear,
            records: vec![
                (BUILDING_ART, "BuildingArt", building.as_slice()),
                (BODY_ART, "BodyArt", body.as_slice()),
            ],
        },
    ])
}

/// [`UNITS`], over `base`'s names, as a mod's descriptions are.
fn unit_archive(base: &Catalogue<'_>) -> Vec<u8> {
    let tree = regent_core::data::xml::parse(UNITS).expect("units.xml");
    let made = xml::compile(&[&tree], base, &xml::ImageSets::default());

    assert!(made.refused.is_empty(), "units.xml: {:?}", made.refused);

    made.archive()
}

/// A game to play: a scene, its seed, its orders and its length.
#[derive(Clone, Debug)]
pub struct Run {
    pub scene: Scene,
    pub seed: u32,
    /// The frame each comes before, 1 up.
    pub inputs: Vec<(u32, Order)>,
    pub frames: u32,
}

impl Run {
    /// `scene`, seed 1, `frames` frames, no orders.
    #[must_use]
    pub const fn new(scene: Scene, frames: u32) -> Run {
        Run {
            scene,
            seed: 1,
            inputs: Vec::new(),
            frames,
        }
    }

    #[must_use]
    pub const fn seed(mut self, seed: u32) -> Run {
        self.seed = seed;
        self
    }

    /// `order`, before frame `frame`.
    #[must_use]
    pub fn input(mut self, frame: u32, order: impl Into<Order>) -> Run {
        self.inputs.push((frame, order.into()));
        self
    }
}

/// What a played game left.
#[derive(Default)]
pub struct Played {
    /// [`Game::digest`] after each frame.
    pub digests: Vec<u64>,
    pub world: World,
    /// The first script errors: none in a game whose rules hold.
    pub errors: Vec<String>,
    /// What the world stopped following: nothing in a game it can run whole.
    pub lost: Vec<Lost>,
    /// Orders [`Game::check`] refused.
    pub dropped: Vec<Dropped>,
}

impl Played {
    /// The agents of the pieces with `code`, ascending.
    #[must_use]
    pub fn all(&self, code: FourCc) -> Vec<u32> {
        all(&self.world, code)
    }
}

/// `world`'s agents of `code`, ascending.
#[must_use]
pub fn all(world: &World, code: FourCc) -> Vec<u32> {
    (0..world.next_id())
        .filter(|id| world.piece(*id).is_some_and(|p| p.code == code))
        .collect()
}

/// [`Played::all`], its one.
///
/// # Panics
///
/// None, or more than one.
#[must_use]
pub fn only(world: &World, code: FourCc) -> u32 {
    let ids = all(world, code);

    assert_eq!(ids.len(), 1, "{code}: {ids:?}");
    ids[0]
}
