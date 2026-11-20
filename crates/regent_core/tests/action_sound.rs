//! An action's sound, on the made-up game ([`regent_testgame`]): the
//! actor's `SoundOverride` for the action, else the action's `Sound`, else
//! the actor's `DefaultSound`. HD's peasants build so: `Basic_build` names
//! no sound, the male peasant overrides it with `Hammering`, the female
//! with `Sawing`.

use regent_core::data::catalogue::Catalogue;
use regent_core::data::desc::xml;
use regent_core::data::fourcc::FourCc;
use regent_core::gpl::ops::Value;
use regent_core::gpl::startup;
use regent_core::gpl::vm::{Config, Vm};
use regent_core::sim::game::Game;
use regent_core::sim::scene::{Placed, Scene};
use regent_core::sim::world::sound::{From, Voice};
use regent_testgame::{BITER, KEEP, TestGame, WALKER, only};

/// `Labour`, with a sound of its own; the walker overrides it, the biter
/// does not.
const LABOUR: &str = r#"<Majesty>
  <Description type="Sound" subType="Standard" ID="ZSB1" Name="Bang">
    <Engine><Phase ID="Begin"/><Phase ID="Peak"/></Engine>
  </Description>
  <Description type="Sound" subType="Standard" ID="ZSC1" Name="Clatter">
    <Engine><Phase ID="Begin"/><Phase ID="Peak"/></Engine>
  </Description>
  <Description type="Action" subType="Standard" ID="ZAL1" Name="Labour">
    <Engine>
      <ImageSet value="Attack"/>
      <Sound value="Bang"/>
      <SoundPhase begin="Begin" peak="Peak"/>
      <Rate min="0" max="300"/>
    </Engine>
  </Description>
  <Description type="Unit" subType="Character" ID="ZTW1" Name="Walker">
    <Engine>
      <Info value="BlockGround"/>
      <CanUse value="HumanPlayer"/>
      <ImageIDBase value="ZIC1"/>
      <Attachment kind="Movement" type="Walk" ID="Trot"/>
      <SoundOverride type="Labour" ID="Clatter"/>
    </Engine>
    <Game>
      <Cost value="40"/>
      <MaxHP value="30"/>
      <SightRange value="8"/>
      <RecruitDelay value="1000"/>
    </Game>
  </Description>
</Majesty>"#;

const BANG: FourCc = FourCc::new(b"ZSB1");
const CLATTER: FourCc = FourCc::new(b"ZSC1");

const WORK: &str = r#"function Work(agent a)
declare
begin
    $PerformAction(a, "Labour", a);
end
"#;

/// Frames to play: past the action's begin and peak.
const FRAMES: u32 = 10;

/// The game's archives, then [`LABOUR`] made over them.
fn archives() -> Vec<(String, Vec<u8>)> {
    let game = TestGame::get();
    let tree = regent_core::data::xml::parse(LABOUR).expect("the XML");
    let made = xml::compile(&[&tree], &game.catalogue(), &xml::ImageSets::default());

    assert!(made.refused.is_empty(), "{:?}", made.refused);

    game.archives()
        .iter()
        .cloned()
        .chain([("labour".to_owned(), made.archive())])
        .collect()
}

/// The records the cues of unit `code` name, once it is told to labour.
fn sounded(code: FourCc) -> Vec<Voice> {
    let archives = archives();
    let catalogue = Catalogue::build(&archives).expect("the catalogue");
    let program = TestGame::get().program_with(WORK);
    let scene = Scene::flat(128)
        .unit(Placed::new(KEEP, [64.0, 64.0]))
        .unit(Placed::new(code, [40.5, 40.0]));
    let mut world = scene.flat_world(&catalogue, &program).expect("a world");
    let staging = scene.staging(&world).expect("staged");
    let mut game = Game::free(Vm::new(&program, Config::RELEASE), 1, &mut world);

    staging.run(&mut game, startup::FREESTYLE).expect("set up");

    let id = only(game.world(), code);
    let mut voices = Vec::new();

    game.call("Work", &[Value::Agent(id)]).expect("called");

    for _ in 0..FRAMES {
        game.frame().expect("a frame");
        voices.extend(
            game.world()
                .cues()
                .iter()
                .filter(|c| c.from == From::Agent(id))
                .map(|c| c.voice.clone()),
        );
    }

    assert_eq!(game.reports().count, 0, "{:?}", game.reports().first);
    voices
}

/// The walker's override plays in place of the action's own sound.
#[test]
fn an_actors_sound_override_wins_over_the_actions_sound() {
    let voices = sounded(WALKER);

    assert!(voices.contains(&Voice::Sound(CLATTER)), "{voices:?}");
    assert!(!voices.contains(&Voice::Sound(BANG)), "{voices:?}");
}

/// The biter, with no override, plays the action's own sound.
#[test]
fn an_actor_without_an_override_plays_the_actions_sound() {
    let voices = sounded(BITER);

    assert!(voices.contains(&Voice::Sound(BANG)), "{voices:?}");
}
