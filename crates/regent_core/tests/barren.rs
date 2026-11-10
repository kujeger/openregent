//! The Barren Waste (`QU01`) begun: the quest's own world, generated on
//! the clock word its traced 0-day save was made from ([`MADE_ON`]), on
//! the traced session seed.
//!
//! Corpora: `MAJESTY_HD`.

mod common;

use common::read;
use regent_core::content::dataset::{Bytecode, Files, Install};
use regent_core::content::quest::{self, Quest};
use regent_core::data::catalogue::Library;
use regent_core::data::fourcc::FourCc;
use regent_core::data::rgs;
use regent_core::data::text::Text;
use regent_core::gpl::load::Program;
use regent_core::gpl::vm::{Config, Vm};
use regent_core::sim::game::Game;
use regent_core::sim::mapgen::custom::Art;
use regent_core::sim::setup;
use regent_core::sim::world::{CROWN, World};
use regent_core::view::panel::portrait::Details;

/// Its quest, and the script `<Name>` the setup dispatches.
const KEY: &[u8; 4] = b"QU01";
const SCRIPT: &str = "BARREN_WASTE";

/// The clock word the map was made from. **Measured**, by a retired
/// search: 59,655,909 words fit the 0-day save's force
/// chart, all alike on units and seeds; of their 10,618,430 clock words
/// (`t | (t & 0x3f) << 21`) this one's grown regions hold
/// [`REGIONS`] of the save's cells, the next 11,326, and its whole map
/// gives [`HEIGHTS`] of the save's heights, the next 401: the rest is
/// ground its buildings levelled.
const MADE_ON: i32 = 0x0725_f9f9;

/// The traced session's seed.
const SEED: u32 = 0x0b60_9e2c;

fn quest() -> Quest {
    quest::offered(&common::hd(), &[])
        .into_iter()
        .find(|q| q.key == FourCc(*KEY))
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

/// The quest's map and chart, read.
fn recipe(quest: &Quest) -> (Vec<u8>, Vec<u8>) {
    (
        read(&quest.template_path().expect("template")),
        read(&files(quest).chart),
    )
}

/// The quest's start on [`MADE_ON`], made with its dataset's archives.
fn begun(quest: &Quest, program: &Program) -> World {
    let files = files(quest);
    let library = Library::from_files(&files.archives).expect("archives");
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
        regent_core::sim::mapgen::quest::generate(&map, &patterns, MADE_ON, &art).expect("made");

    setup::fresh(&catalogue, program, &made.start())
}

/// The quest's setup posts `AITX` 143 on its banner (`Post_Message`):
/// selected, it reads "Castle Darkmoor" (**Game**, the user's).
#[test]
fn the_barren_wastes_banner_reads_castle_darkmoor() {
    let q = quest();
    let files = files(&q);
    let program = program(&files);
    let mut world = begun(&q, &program);
    let vm = Vm::new(&program, Config::default());
    let mut game = Game::free(vm, SEED, &mut world);

    game.setup(SCRIPT).expect("setup");
    drop(game);

    let library = Library::from_files(&files.archives).expect("archives");
    let catalogue = library.catalogue().expect("catalogue");
    let text = Text::build(&catalogue);
    let banner = world
        .shown()
        .iter()
        .find(|s| s.code == FourCc(*b"BBs1"))
        .expect("the banner")
        .agent;

    let Some(Details::Posted(posted)) = Details::of(&catalogue, &world, banner, CROWN) else {
        panic!("a sign");
    };

    assert_eq!(posted.text(&text).as_deref(), Some("Castle Darkmoor"));
}
