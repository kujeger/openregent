//! Sprite pages asked for before they are drawn, along what the player
//! is about to see: a building placed, its construction begun, will
//! show its later stages and the units it makes.
//!
//! ```text
//!  a building first shown, a start's too: asked early, loaded behind play
//!    its own sets, but its damage and fall           (LATE left out)
//!    its upgrade's alike                              (UpgradeTo)
//!    each unit it makes: every set                    (Produces)
//!    each unit its lair data names: every set         (World::lair_units)
//!    each unit its script spawns: every set           (gpl::spawns)
//!    each set asked with its slot effects' sets       (Sprites::ask_unit)
//! ```
//!
//! A lair spawns its lair data's entries where it has any, else what its
//! script names; its special codes when it falls. Both lists are asked:
//! where entries stand in for the script's, its units are asked unused.
//! A unit's missile and its spells' effects are named by scripts, not by
//! its data: not asked.

use std::collections::{HashMap, HashSet};

use bevy::prelude::*;
use regent_core::data::fourcc::FourCc;
use regent_core::data::imag::SetKind;
use regent_core::gpl::load::Program;
use regent_core::sim::world::World;
use regent_draw::art::Sprites;
use regent_draw::data::Corpus;

use crate::game::clock::Snapshot;
use crate::game::seen::{Frame, Watch};

/// A building's set kinds not asked for early: its damage stages and its
/// fall, which a game may never show.
const LATE: [SetKind; 4] = [
    SetKind::Damage,
    SetKind::Die,
    SetKind::Crumble,
    SetKind::Dead,
];

/// The pieces whose pages were asked for, by agent, and the game frame
/// last looked at.
#[derive(Resource, Default)]
pub(crate) struct Asked {
    agents: HashSet<u32>,
    frame: Option<u32>,
    /// What each spawning building's script spawns ([`spawned`]), read
    /// at the first look.
    spawns: Option<HashMap<FourCc, Vec<FourCc>>>,
}

/// What each spawning building's script can spawn, by its code: the
/// game's own program read (`regent_core::gpl::spawns`), each name the
/// descriptor `SpawnUnit` would find.
fn spawned(program: &Program, world: &World) -> HashMap<FourCc, Vec<FourCc>> {
    let code = |name: &String| world.descriptors().body_named(name).map(|(code, _)| code);

    regent_core::gpl::spawns::spawners(program)
        .iter()
        .filter_map(|spawner| {
            let units = spawner.names.iter().chain(&spawner.staged);

            Some((code(&spawner.instance)?, units.filter_map(code).collect()))
        })
        .collect()
}

/// Each building first shown, its pages and its units' asked for: once a
/// game frame.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
pub(crate) fn ask(
    (snapshot, frame, seen): (Res<Snapshot>, Res<Frame>, Watch),
    corpus: Res<Corpus>,
    mut sprites: ResMut<Sprites>,
    mut asked: ResMut<Asked>,
) {
    if asked.frame != Some(frame.frames) {
        asked.frame = Some(frame.frames);

        let Asked { agents, spawns, .. } = &mut *asked;
        let spawns = spawns.get_or_insert_with(|| spawned(seen.program, seen.world()));

        for shown in &snapshot.now {
            let fresh = agents.insert(shown.agent);
            let building = fresh
                .then(|| corpus.unit(shown.code))
                .flatten()
                .filter(|unit| unit.desc.building().is_some());

            if let Some(unit) = building {
                let desc = unit.desc.building();
                let upgrade = desc
                    .as_ref()
                    .and_then(regent_core::data::desc::Building::upgrade_to);
                let made = desc.map(|b| b.produces()).unwrap_or_default();
                let lair = seen.world().lair_units(shown.handle);
                let spawns = spawns.get(&shown.code).map_or(&[][..], Vec::as_slice);

                sprites.ask_unit(&corpus, shown.code, &LATE);

                if let Some(upgrade) = upgrade {
                    sprites.ask_unit(&corpus, upgrade, &LATE);
                }

                for code in made.into_iter().chain(lair).chain(spawns.iter().copied()) {
                    sprites.ask_unit(&corpus, code, &[]);
                }
            }
        }
    }
}

/// The asks' record, a game's, and the system before the drawing.
pub(crate) fn plugin(app: &mut App) {
    use crate::game::scope::Scoped as _;

    app.per_game::<Asked>().add_systems(
        Update,
        ask.in_set(crate::draw::frame::Frame::Step)
            .after(crate::game::clock::tick),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::clock::Steps;
    use bevy::ecs::system::RunSystemOnce;
    use regent_core::data::imag::set_number;

    /// A freestyle game whose start shows lairs.
    const GAME: [&str; 4] = ["--map-seed", "7", "--side", "512"];

    /// Whether a set kind is drawn on the map: one a sheet holds.
    const fn world_kind(kind: SetKind) -> bool {
        !matches!(
            kind,
            SetKind::Interface
                | SetKind::Minimap
                | SetKind::MapTexture { .. }
                | SetKind::UnitTexture
        )
    }

    /// A new game's palace shown asks for its peasants', tax collectors'
    /// and guards' every set, and its own but its damage and fall; a
    /// unit it does not make is not asked for.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_shown_palace_asks_for_its_units_pages() {
        const PALACE: FourCc = FourCc::new(b"ABJ1");
        const MADE: [FourCc; 3] = [
            FourCc::new(b"AVf1"),
            FourCc::new(b"AVg1"),
            FourCc::new(b"AVj1"),
        ];
        const NOT_MADE: FourCc = FourCc::new(b"AVL1");

        let hd = crate::files::env::path(crate::files::env::HD).expect("MAJESTY_HD");
        let corpus =
            Corpus::open(&crate::game::load::archives(&hd).expect("the dataset")).expect("corpus");
        let (begun, _catalogue) = crate::game::begin::test_game(&[]);
        let frame = Frame::of(&begun.sim);
        let shown = begun.sim.world().shown();
        let mut app = App::new();

        assert!(shown.iter().any(|s| s.code == PALACE), "a palace");

        app.insert_resource(Snapshot::of(Steps::showing(shown)))
            .insert_resource(frame)
            .insert_resource(corpus)
            .init_resource::<Sprites>()
            .init_resource::<Asked>();
        app.world_mut().run_system_once(ask).expect("asked");

        let world = app.world();
        let (corpus, sprites) = (world.resource::<Corpus>(), world.resource::<Sprites>());
        let sets = |code: FourCc, kinds: &dyn Fn(SetKind) -> bool| -> Vec<(FourCc, u16)> {
            let unit = corpus.unit(code).expect("a unit");
            let record = unit.art.as_ref().expect("its art").id;

            unit.animations
                .iter()
                .filter(|a| kinds(a.kind))
                .map(|a| (record, set_number(a.id)))
                .collect()
        };

        for code in MADE {
            let unasked: Vec<_> = sets(code, &world_kind)
                .into_iter()
                .filter(|o| sprites.unasked(corpus, *o))
                .collect();

            assert_eq!(unasked, [], "{code}");
        }

        let own = |k: SetKind| world_kind(k) && !LATE.contains(&k);
        let late = |k: SetKind| LATE.contains(&k);

        assert!(
            sets(PALACE, &own)
                .iter()
                .all(|o| !sprites.unasked(corpus, *o))
        );
        assert!(
            sets(PALACE, &late)
                .iter()
                .all(|o| sprites.unasked(corpus, *o))
        );
        assert!(
            sets(NOT_MADE, &world_kind)
                .iter()
                .all(|o| sprites.unasked(corpus, *o))
        );
        assert!(sprites.jobs(regent_draw::map::sheet::Need::Early) > 0);
    }

    /// A lair shown asks for every set of each unit its script spawns.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_shown_lair_asks_for_its_spawns_pages() {
        let hd = crate::files::env::path(crate::files::env::HD).expect("MAJESTY_HD");
        let corpus =
            Corpus::open(&crate::game::load::archives(&hd).expect("the dataset")).expect("corpus");
        let (begun, _catalogue) = crate::game::begin::test_game(&GAME);
        let frame = Frame::of(&begun.sim);
        let shown = begun.sim.world().shown();
        let scripted = spawned(frame.program, begun.sim.world());
        let spawns: Vec<FourCc> = shown
            .iter()
            .filter_map(|s| scripted.get(&s.code))
            .flatten()
            .copied()
            .collect();
        let mut app = App::new();

        assert!(!spawns.is_empty(), "a lair");

        app.insert_resource(Snapshot::of(Steps::showing(shown)))
            .insert_resource(frame)
            .insert_resource(corpus)
            .init_resource::<Sprites>()
            .init_resource::<Asked>();
        app.world_mut().run_system_once(ask).expect("asked");

        let world = app.world();
        let (corpus, sprites) = (world.resource::<Corpus>(), world.resource::<Sprites>());

        for code in spawns {
            let unit = corpus.unit(code).expect("a unit");
            let record = unit.art.as_ref().expect("its art").id;
            let unasked: Vec<_> = unit
                .animations
                .iter()
                .filter(|a| world_kind(a.kind))
                .map(|a| (record, set_number(a.id)))
                .filter(|o| sprites.unasked(corpus, *o))
                .collect();

            assert_eq!(unasked, [], "{code}");
        }
    }
}
