//! `$GetBuildingLevel`, openregent's own builtin, on the made-up game
//! ([`regent_testgame`]): a script reads a hut's level, 0 while first
//! built, 1, still 1 while its upgrade is built, then 2 once it is finished.

use regent_core::data::attrib::{self, Key};
use regent_core::data::catalogue::Catalogue;
use regent_core::data::desc::xml;
use regent_core::data::fourcc::FourCc;
use regent_core::gpl::builtins;
use regent_core::gpl::ops::Value;
use regent_core::gpl::startup;
use regent_core::gpl::vm::{Callable, Config, Vm};
use regent_core::sim::game::{Game, Input};
use regent_core::sim::scene::{Placed, Scene};
use regent_core::sim::world::World;
use regent_testgame::{HUT, KEEP, TestGame, only};

/// The hut a level 2 of it: `ZTH1` again, upgrading to `ZTH2`.
const UPGRADE: &str = r#"<Majesty>
  <Description type="Unit" subType="Building" ID="ZTH1" Name="Hut">
    <Engine>
      <Info value="BlockGround"/>
      <CanUse value="HumanPlayer"/>
      <ImageIDBase value="ZIB1"/>
    </Engine>
    <Game>
      <Cost value="120"/>
      <MaxHP value="80"/>
      <SightRange value="6"/>
      <UpgradeTo value="Hut2"/>
    </Game>
  </Description>
  <Description type="Unit" subType="Building" ID="ZTH2" Name="Hut2">
    <Engine>
      <Info value="BlockGround"/>
      <CanUse value="HumanPlayer"/>
      <ImageIDBase value="ZIB1"/>
    </Engine>
    <Game>
      <Cost value="100"/>
      <MaxHP value="80"/>
      <SightRange value="6"/>
    </Game>
  </Description>
</Majesty>"#;

const HUT_2: FourCc = FourCc::new(b"ZTH2");

/// A script writing the level where the test reads it: `MaxGuildMembers`,
/// an attribute no rule of the game reads. `Finish` ends a stage as the
/// game's `BuildingReachedMaxHP` does; the made-up rules never do. With
/// the script the engine dispatches after an upgrade, which the game's
/// rules lack.
const SEEN: &str = "function LevelSeen(agent b)
begin
    $SetAttribute(b, #ATTRIB_MaxGuildMembers, $GetBuildingLevel(b));
end

function Finish(agent b)
begin
    $SetAttribute(b, #ATTRIB_CurrentStageBuilt, 1);
end

function building_upgraded(agent b)
begin
end
";

/// `MaxGuildMembers`.
const SEEN_KEY: Key = Key::new(*b"APB", 0);

/// The game's archives, then [`UPGRADE`] made over them.
fn archives() -> Vec<(String, Vec<u8>)> {
    let game = TestGame::get();
    let tree = regent_core::data::xml::parse(UPGRADE).expect("the XML");
    let made = xml::compile(&[&tree], &game.catalogue(), &xml::ImageSets::default());

    assert!(made.refused.is_empty(), "{:?}", made.refused);

    game.archives()
        .iter()
        .cloned()
        .chain([("upgrade".to_owned(), made.archive())])
        .collect()
}

/// The script reads 1 for a placed hut and the keep, 1 for the hut while
/// its upgrade is built, then 2 once finished; no script errors.
#[test]
fn a_script_reads_a_buildings_level() {
    let archives = archives();
    let catalogue = Catalogue::build(&archives).expect("the catalogue");
    let program = TestGame::get().program_with(SEEN);
    let scene = Scene::flat(128)
        .gold(1000)
        .unit(Placed::new(KEEP, [64.0, 64.0]))
        .unit(Placed::new(HUT, [50.0, 40.0]));
    let mut world = scene.flat_world(&catalogue, &program).expect("a world");
    let staging = scene.staging(&world).expect("staged");
    let mut game = Game::free(Vm::new(&program, Config::RELEASE), 1, &mut world);

    staging.run(&mut game, startup::FREESTYLE).expect("set up");

    let (hut, keep) = (only(game.world(), HUT), only(game.world(), KEEP));
    let seen = |game: &mut Game<'_>, id: u32| {
        game.call("LevelSeen", &[Value::Agent(id)]).expect("called");

        let world: &World = game.world();

        attrib::get(&world.piece(id).expect("the piece").attributes, SEEN_KEY)
    };

    assert_eq!(seen(&mut game, hut), Some(1));
    assert_eq!(seen(&mut game, keep), Some(1));

    game.input(1, Input::Upgrade(hut)).expect("in time");
    game.frame().expect("a frame");
    game.frame().expect("a frame");

    assert_eq!(game.world().piece(hut).map(|p| p.code), Some(HUT_2));
    assert_eq!(seen(&mut game, hut), Some(1));

    game.call("Finish", &[Value::Agent(hut)]).expect("called");

    assert_eq!(seen(&mut game, hut), Some(2));
    assert_eq!(game.reports().count, 0, "{:?}", game.reports().first);
}

/// A hut still in its first construction reads 0, as a temple then
/// grants no spells; finished, 1.
#[test]
fn a_building_under_construction_reads_0() {
    let archives = archives();
    let catalogue = Catalogue::build(&archives).expect("the catalogue");
    let program = TestGame::get().program_with(SEEN);
    let scene = Scene::flat(128)
        .gold(1000)
        .unit(Placed::new(KEEP, [64.0, 64.0]))
        .unit(
            Placed::new(HUT, [50.0, 40.0])
                .set("FirstStageBuilt", 0)
                .set("CurrentStageBuilt", 0),
        );
    let mut world = scene.flat_world(&catalogue, &program).expect("a world");
    let staging = scene.staging(&world).expect("staged");
    let mut game = Game::free(Vm::new(&program, Config::RELEASE), 1, &mut world);

    staging.run(&mut game, startup::FREESTYLE).expect("set up");

    let hut = only(game.world(), HUT);
    let seen = |game: &mut Game<'_>| {
        game.call("LevelSeen", &[Value::Agent(hut)])
            .expect("called");

        let world: &World = game.world();

        attrib::get(&world.piece(hut).expect("the piece").attributes, SEEN_KEY)
    };

    assert_eq!(seen(&mut game), Some(0));

    game.call("Finish", &[Value::Agent(hut)]).expect("called");

    assert_eq!(seen(&mut game), Some(1));
    assert_eq!(game.reports().count, 0, "{:?}", game.reports().first);
}

/// The own builtin is a builtin by name and by handle, past every engine
/// handle: the engine's builtins and definitions keep theirs.
#[test]
fn the_own_builtin_moves_no_engine_handle() {
    let program = TestGame::get().program_with(SEEN);
    let own = builtins::id("getbuildinglevel").expect("registered");
    let handle = Vm::named_handle(&program, "GetBuildingLevel").expect("a handle");
    let last = builtins::id("ValidFunction").expect("the engine's last");

    assert!(own.own() && !last.own());
    assert_eq!(Vm::callable(handle), Callable::Builtin(own));
    assert_eq!(own.get().name, "GetBuildingLevel");
    assert_eq!(Vm::named_handle(&program, "ValidFunction"), Some(166));
    assert_eq!(Vm::definition_handle(0), 167);
    assert_eq!(Vm::callable(167), Callable::Definition(0));
    assert_eq!(builtins::documented().count(), 166);
}
