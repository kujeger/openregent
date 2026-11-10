//! The example mods in `examples/`, each played in a short headless
//! scene with and without it: the effect its README names.

use std::path::{Path, PathBuf};

use regent_core::data::attrib;
use regent_core::data::fourcc::FourCc;
use regent_core::sim::game::Input;
use regent_core::sim::world::{World, recruit::Recruit};

use crate::files::env::Install;
use crate::game::{begin::Plan, drive, sim::Sim};

/// Big ranger camp.
const BIG_RANGER_CAMP: &str = "{56A39291-8971-4975-BB9A-C4F140535107}";

/// Inn peasants.
const INN_PEASANTS: &str = "{4F78A1F4-E83F-4A84-A374-2F0DD097FED1}";

/// Guardhouse pair.
const GUARDHOUSE_PAIR: &str = "{9C7D9A63-E6F7-4AC7-A7A5-A9CE7B05B3DC}";

/// Veteran recruits.
const VETERAN_RECRUITS: &str = "{3D01DE84-08A6-4C30-8459-EA2BF0530103}";

/// `ABW1`, `Rangers_Guild`.
const RANGERS_GUILD: FourCc = FourCc(*b"ABW1");

/// `ABV1`, `Warriors_Guild`.
const WARRIORS_GUILD: FourCc = FourCc(*b"ABV1");

/// `AVL1`, `Warrior`.
const WARRIOR: FourCc = FourCc(*b"AVL1");

/// Whether a game plays an example mod.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Modded {
    With(&'static str),
    Without,
}

/// The examples folder: a mod folder as any.
fn examples() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("examples")
}

/// A game of `scene` (`assets/scenes`), the examples a mod folder, with
/// the mod or not.
fn game(scene: &str, modded: Modded) -> Sim {
    let scene = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/scenes")
        .join(scene)
        .display()
        .to_string();
    let mut args = vec!["openregent", "--scene", scene.as_str()];

    if let Modded::With(guid) = modded {
        args.extend(["--mods", guid]);
    }

    let install = Install::from_env()
        .expect("MAJESTY_HD")
        .with_mod_folders(vec![examples()]);
    let setup = crate::cli::tests::parsed(args);
    let plan = Plan::of(&install, &setup, None).expect("planned");
    let catalogue = regent_draw::data::catalogue_of(plan.sources()).expect("catalogue");

    plan.begin(&catalogue).expect("begun").sim
}

/// The ids of the pieces standing.
fn standing(world: &World) -> impl Iterator<Item = u32> + '_ {
    (0..world.next_id()).filter(|id| world.piece(*id).is_some())
}

/// The scene's guild's `MaxGuildMembers` once its finishing script ran,
/// and the game's script errors.
fn guild_spots(modded: Modded) -> (i32, usize) {
    let mut sim = game("rangers_guild.toml", modded);

    drive::headless(&mut sim, 20, &mut None, |_| {});

    let world = sim.world();
    let spots = standing(world)
        .find(|id| world.piece(*id).is_some_and(|p| p.code == RANGERS_GUILD))
        .and_then(|id| world.members(id))
        .expect("the guild")
        .spots;

    (spots, sim.reports().count)
}

/// The mod's guild holds 8; the game's 4.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn big_ranger_camp_holds_8_rangers() {
    assert_eq!(guild_spots(Modded::Without), (4, 0));
    assert_eq!(guild_spots(Modded::With(BIG_RANGER_CAMP)), (8, 0));
}

/// Peasants standing after `frames` in the scene of a palace and three
/// inns, and the game's script errors.
fn peasants_after(modded: Modded, frames: u32) -> (usize, usize) {
    let mut sim = game("inns.toml", modded);

    drive::headless(&mut sim, frames, &mut None, |_| {});

    let world = sim.world();
    let peasants = standing(world)
        .filter(|id| {
            world
                .agent_name(*id)
                .is_some_and(|n| n.starts_with("Peasant"))
        })
        .count();

    (peasants, sim.reports().count)
}

/// A level 1 palace keeps 2 peasants; three inns add one.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn inn_peasants_adds_one_for_two_inns() {
    // 60 s of game time: a peasant 5 s in, then one every 10 s.
    const FRAMES: u32 = 1200;

    assert_eq!(peasants_after(Modded::Without, FRAMES), (2, 0));
    assert_eq!(peasants_after(Modded::With(INN_PEASANTS), FRAMES), (3, 0));
}

/// Whether [`guards_after`] upgrades the guardhouse.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Upgrade {
    Yes,
    No,
}

/// City guards standing after `frames` in `scene`, and the game's script
/// errors. With [`Upgrade::Yes`] the guardhouse is upgraded once its
/// first guard stands.
fn guards_after(scene: &str, modded: Modded, upgrade: Upgrade, frames: u32) -> (usize, usize) {
    let mut sim = game(scene, modded);
    let guards = |world: &World| {
        standing(world)
            .filter(|id| {
                world
                    .agent_name(*id)
                    .is_some_and(|n| n.contains("City_Guard"))
            })
            .count()
    };
    let mut asked = upgrade == Upgrade::No;

    drive::headless(&mut sim, frames, &mut None, |sim| {
        let world = sim.world();
        let house = standing(world).find(|id| {
            world
                .piece(*id)
                .is_some_and(|p| p.code.0.starts_with(b"ABE"))
        });

        if let Some(id) = house.filter(|_| !asked && guards(world) == 1) {
            asked = true;
            sim.input(regent_core::sim::game::Input::Upgrade(id));
        }
    });

    (guards(sim.world()), sim.reports().count)
}

/// A guardhouse upgraded to level 2 posts 2 guards with the mod, 1
/// without. A level 1 guardhouse keeps 1. One placed at level 2 posts 1
/// either way: the game's scripts start no second spawn there, as in HD
/// with the SDK's Adjust Guardhouse.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn guardhouse_pair_posts_two_at_level_2() {
    // 60 s of game time: the first guard at once, the second 17 s later
    // (`#Guard_Spawn_Time`); an upgrade's once it is built.
    const FRAMES: u32 = 1200;

    let pair = Modded::With(GUARDHOUSE_PAIR);
    let level_2 = |modded| guards_after("guardhouse.toml", modded, Upgrade::No, FRAMES);
    let level_1 =
        |modded, upgrade| guards_after("guardhouse_upgrade.toml", modded, upgrade, FRAMES);

    assert_eq!(level_2(Modded::Without), (1, 0));
    assert_eq!(level_2(pair), (1, 0));
    assert_eq!(level_1(pair, Upgrade::No), (1, 0));
    assert_eq!(level_1(Modded::Without, Upgrade::Yes), (1, 0));
    assert_eq!(level_1(pair, Upgrade::Yes), (2, 0));
}

/// The level of a warrior hired at the start of `scene`, and the game's
/// script errors.
fn hired_level(scene: &str, modded: Modded) -> (i32, usize) {
    // 30 s of game time: the warrior's recruit delay, then its birth.
    const FRAMES: u32 = 600;

    let mut sim = game(scene, modded);
    let mut asked = false;

    drive::headless(&mut sim, FRAMES, &mut None, |sim| {
        let world = sim.world();
        let guild = standing(world)
            .find(|id| world.piece(*id).is_some_and(|p| p.code == WARRIORS_GUILD))
            .filter(|_| !asked);

        if let Some(guild) = guild {
            asked = true;
            sim.input(Input::Recruit(Recruit {
                guild,
                code: WARRIOR,
            }));
        }
    });

    let world = sim.world();
    let level = standing(world)
        .find(|id| world.piece(*id).is_some_and(|p| p.code == WARRIOR))
        .map(|id| world.word_of(id, attrib::EXPERIENCE_LEVEL))
        .expect("the warrior");

    (level, sim.reports().count)
}

/// A hero hired under a level 1, 2 or 3 palace starts at that level with
/// the mod, at 1 without; with no palace at 1.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn veteran_recruits_start_at_the_palace_level() {
    let veteran = Modded::With(VETERAN_RECRUITS);
    let palace = |level| format!("warriors_guild_palace{level}.toml");

    assert_eq!(hired_level(&palace(3), Modded::Without), (1, 0));

    for level in 1..=3 {
        assert_eq!(hired_level(&palace(level), veteran), (level, 0));
    }

    assert_eq!(hired_level("warriors_guild_alone.toml", veteran), (1, 0));
}
