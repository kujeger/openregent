//! The app's lifecycle, headless: menus, command-line games, restart,
//! saves, a won game's record, and what a game leaves for the next.

use std::collections::HashMap;
use std::time::Duration;

use bevy::ecs::change_detection::Tick;
use bevy::time::TimeUpdateStrategy;
use regent_core::data::fourcc::FourCc;
use regent_core::gpl::runtime::MS_PER_FRAME;
use regent_core::sim::mapgen::preset::Preset;
use regent_core::sim::world::CROWN;
use regent_core::sim::world::defeat::End;
use regent_core::view::menu::end::{Button as EndButton, Page as EndPage, Panel};
use regent_core::view::menu::music as front_end;
use regent_core::view::menu::quest_map::Progress;

use super::*;
use crate::game::clock::tick;
use crate::game::seen::{Frame, News};

/// Reward settings a save test raises, none the start's.
const REWARD_ATTACK: i32 = 450;
const REWARD_EXPLORE: i32 = 350;

/// Longest a game may take to make, frames drawn meanwhile.
const MAKING: Duration = Duration::from_mins(2);

/// Drawn frames the app test steps.
const STEPPED: u32 = 20;

/// Drawn frames, a game frame each, past the end's fade: a step each
/// two (`FADE_STEP_MS`, strictly past), and some.
const FADED: u32 = 2 * regent_core::sim::world::defeat::FADE_STEPS + STEPPED;

/// Drawn frames, at [`MS_PER_FRAME`], that outlast an end's advisor line:
/// muted, no sound ends by itself, so its length ends it.
const SPOKEN: u32 = 20_000 / MS_PER_FRAME;

/// `press` on the end's box or panel, carried out over a few frames.
fn press(app: &mut App, press: crate::ui::end::Press) {
    app.world_mut()
        .resource_mut::<crate::ui::end::EndScreen>()
        .press(press);

    for _ in 0..STEPPED {
        step(app);
    }
}

/// One drawn frame, a game being made finished first and its Start
/// Quest pressed: stepped as when the making ran on the frame
/// (`menu::begin`).
fn step(app: &mut App) {
    crate::menu::settle(app.world());
    app.world_mut().resource_mut::<crate::menu::Menu>().go();
    app.update();
}

/// `look` run on the game in `world`, on its thread (`game::thread`).
fn run<R: Send + 'static>(
    world: &World,
    look: impl FnOnce(&mut crate::game::sim::Sim) -> R + Send + 'static,
) -> R {
    world
        .resource::<SimThread>()
        .run(look)
        .expect("the game's thread")
}

/// The game in `world`'s frames run.
fn frames(world: &World) -> u32 {
    run(world, |sim| sim.frames())
}

/// The game in `world` as a save takes it, and its frame.
fn kept(world: &World) -> (crate::save::FromSim, u32) {
    run(world, |sim| (crate::save::FromSim::of(sim), sim.frames()))
}

/// A game's way back to the menus, as its panels ask it.
fn returning(app: &mut App, back: crate::menu::Return) {
    app.world_mut().resource_mut::<crate::menu::Returning>().0 = Some(back);
}

/// No profile: nothing recorded in one.
fn nobody() -> crate::files::profile::Player {
    crate::files::profile::Player::default()
}

/// A new game's app, the command line's, headless, stepped on Bevy's
/// multi-threaded executor: every system and run condition runs, so
/// one that cannot run off the main thread panics here.
/// Nothing is recorded, logged to a file, or saved: no input, no
/// `log::open`.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn the_app_runs_headless_for_some_frames() {
    let (quests, maker) = commanding_maker(None, &[]);
    let mut settings = Settings::default();
    let mut menu = crate::menu::Opened::new(quests, crate::menu::Art::default());

    settings.audio.muted = true;
    menu.with(|m| m.menu.command(crate::menu::LoadKind::Freestyle));

    let mut app = menu_app(menu, (maker, nobody()), 1.0, settings, Display::Headless);

    // The one test with no shadow models (the others have the guardhouse's):
    // a player with an empty model folder.
    app.insert_resource(regent_draw::sun::model::Proxies::default());

    // A game frame each drawn frame, whatever the machine's pace.
    let frame = Duration::from_millis(MS_PER_FRAME.into());

    app.insert_resource(TimeUpdateStrategy::ManualDuration(frame));

    for _ in 0..STEPPED {
        step(&mut app);
    }

    let world = app.world();

    assert_eq!(*world.resource::<State<Screen>>().get(), Screen::Playing);
    assert!(frames(world) > 0, "game frames ran");
    assert!(
        world.resource::<stage::View>().rect.is_some(),
        "the egui pass ran"
    );
}

/// The offered quests, and the maker of their games, recording
/// nothing.
fn quest_maker() -> (Vec<crate::game::load::Offered>, Maker) {
    recording_maker(None)
}

/// The same, a game recorded to `dir` if given.
fn recording_maker(dir: Option<std::path::PathBuf>) -> (Vec<crate::game::load::Offered>, Maker) {
    commanding_maker(dir, &[])
}

/// The same, the command line's game `args`, as `main` asks for it.
fn commanding_maker(
    dir: Option<std::path::PathBuf>,
    args: &'static [&'static str],
) -> (Vec<crate::game::load::Offered>, Maker) {
    let install = crate::files::env::Install::from_env().expect("MAJESTY_HD");
    let quests = crate::quests(&install).expect("quests");
    let setup = crate::cli::tests::parsed(std::iter::once(&"openregent").chain(args));
    let offered = crate::cli::quest(&setup, &install).expect("its quest");
    let command = crate::game::begin::Command {
        setup,
        local: regent_core::sim::world::CROWN,
        warm: None,
        events: Vec::new(),
        offered,
        peer: None,
        saved: None,
    };
    let recording = dir.map_or(
        crate::game::begin::Recording::Off,
        crate::game::begin::Recording::In,
    );

    (quests, Maker::new(install, Some(command), recording))
}

/// The game in `app`, what it is.
fn session(app: &App) -> &Session {
    app.world().resource::<Session>()
}

/// The command line's game, `args`, opens the app: it plays at once,
/// with no briefing over it; saved, played on and loaded
/// from its Options, it goes on from the save; a won quest is
/// recorded in the profile; the end panel's Main Menu takes it out
/// and shows the main menu; another quest then starts.
#[allow(clippy::too_many_lines, reason = "one game through every way out")]
fn command_line_round(name: &str, args: &'static [&'static str]) {
    let scratch = crate::testdata::scratch(name);
    let base = scratch.path();
    let (quests, maker) = commanding_maker(Some(base.join("recorded")), args);
    let profile = crate::files::profile::Profiles::at(base)
        .create("P")
        .expect("a scratch profile");
    let saves_at = (
        crate::save::dir(&profile.dir, crate::ui::options::Play::Alone),
        profile.name.clone(),
    );
    let player = crate::files::profile::Player {
        profile: Some(profile),
        recorded: false,
    };
    let mut settings = Settings::default();
    let mut menu = crate::menu::Opened::new(quests, crate::menu::Art::default());

    std::fs::create_dir_all(base.join("recorded")).expect("scratch");
    settings.audio.muted = true;
    menu.with(|m| m.menu.command(crate::menu::LoadKind::Freestyle));

    let mut app = menu_app(menu, (maker, player), 1.0, settings, Display::Headless);
    let frame = Duration::from_millis(MS_PER_FRAME.into());
    let screen = |app: &App| *app.world().resource::<State<Screen>>().get();

    app.insert_resource(TimeUpdateStrategy::ManualDuration(frame));

    for _ in 0..STEPPED {
        step(&mut app);
    }

    let quest = session(&app).quest.is_some();

    assert_eq!(screen(&app), Screen::Playing);
    assert!(!app.world().resource::<ui::Paused>().0, "not paused");
    assert!(frames(app.world()) > 0, "game frames ran");

    // Saved.

    let path = saves_at.0.join(format!("c{}", crate::save::EXTENSION));
    let saved = {
        let world = app.world();
        let (game, frames) = kept(world);
        let text = &world.resource::<Corpus>().text;
        let interface = crate::save::Interface {
            selected: None,
            reading: None,
            pan: (0.0, 0.0),
            zoom: 1.0,
            tracking: regent_core::view::panel::tracking::Tracker::default(),
        };
        let contents =
            crate::save::of((game, session(&app)), text, None, interface).expect("contents");

        crate::save::write(&path, &contents).expect("written");
        frames
    };

    assert!(saved > 0);

    for _ in 0..STEPPED {
        step(&mut app);
    }

    // Options, LOAD GAME.
    returning(
        &mut app,
        crate::menu::Return::Load {
            path,
            name: "c".to_owned(),
            place: Some(saves_at),
        },
    );
    app.world_mut()
        .resource_mut::<NextState<Screen>>()
        .set(Screen::Menu);

    for _ in 0..STEPPED {
        step(&mut app);
    }

    let now = frames(app.world());

    assert_eq!(screen(&app), Screen::Playing);
    assert!(
        (saved..saved + STEPPED).contains(&now),
        "{now} goes on from {saved}"
    );

    // A won quest is recorded in the profile.
    run(app.world(), |sim| {
        sim.input(Input::Cheat(CheatCode {
            cheat: Cheat::Win,
            arg: CROWN,
        }));
    });

    for _ in 0..STEPPED {
        step(&mut app);
    }

    let key = session(&app).quest_key();
    let player = app.world().resource::<crate::files::profile::Player>();
    let won = key.and_then(|k| player.profile.as_ref().map(|p| p.progress.victories(k)));

    assert_eq!(player.recorded, quest);
    assert_eq!(won, quest.then_some(1));

    // The end panel's Main Menu, as Options' RETURN TO MAIN MENU.
    returning(&mut app, crate::menu::Return::Main);
    app.world_mut()
        .resource_mut::<NextState<Screen>>()
        .set(Screen::Menu);

    for _ in 0..STEPPED {
        step(&mut app);
    }

    assert_eq!(screen(&app), Screen::Menu);
    assert!(
        app.world().get_resource::<SimThread>().is_none(),
        "the game is out"
    );

    app.world_mut().resource_mut::<crate::menu::Menu>().ask(1);

    for _ in 0..STEPPED {
        step(&mut app);
    }

    let world = app.world();

    assert_eq!(screen(&app), Screen::Playing);
    assert!(session(&app).quest.is_some(), "the menus' quest");
    assert!(frames(world) > 0, "the next game runs");
}

#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_command_line_quest_loads_a_save_records_its_win_and_goes_to_the_menus() {
    command_line_round("command_quest", &["--quest", "QU01"]);
}

#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_command_line_freestyle_game_loads_a_save_and_goes_to_the_menus_for_another() {
    command_line_round("command_freestyle", &["--map-seed", "1"]);
}

/// A quest the menus started, saved, played on, then loaded from the
/// game's Options: the game is taken down and the save's made again
/// by the menus, its selection back, going on from its frame.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_game_loaded_from_its_options_goes_on_from_the_save() {
    let scratch = crate::testdata::scratch("load_again");
    let base = scratch.path();
    let (quests, maker) = recording_maker(Some(base.join("recorded")));
    let profile = crate::files::profile::Profiles::at(base)
        .create("P")
        .expect("a scratch profile");
    let saves_at = (
        crate::save::dir(&profile.dir, crate::ui::options::Play::Alone),
        profile.name.clone(),
    );
    let mut settings = Settings::default();

    std::fs::create_dir_all(base.join("recorded")).expect("scratch");
    settings.audio.muted = true;

    let menu = crate::menu::Opened::starting(quests, 0);
    let mut app = menu_app(menu, (maker, nobody()), 1.0, settings, Display::Headless);
    let frame = Duration::from_millis(MS_PER_FRAME.into());

    app.insert_resource(TimeUpdateStrategy::ManualDuration(frame));
    app.insert_resource(crate::files::profile::Player {
        profile: Some(profile),
        recorded: false,
    });

    for _ in 0..STEPPED {
        step(&mut app);
    }

    // Saved with its palace selected and the rewards raised.
    let palace = run(app.world(), |sim| sim.world().player_palace(CROWN));
    let path = saves_at.0.join(format!("q{}", crate::save::EXTENSION));
    let rewards = regent_core::sim::world::flag::Rewards {
        attack: REWARD_ATTACK,
        explore: REWARD_EXPLORE,
    };
    let interface = crate::save::Interface {
        selected: palace,
        reading: None,
        pan: (0.0, 0.0),
        zoom: 1.0,
        tracking: regent_core::view::panel::tracking::Tracker::default(),
    };

    run(app.world(), move |sim| sim.set_rewards(rewards));

    let saved = {
        let world = app.world();
        let (game, frames) = kept(world);
        let text = &world.resource::<Corpus>().text;
        let contents =
            crate::save::of((game, session(&app)), text, None, interface).expect("contents");

        crate::save::write(&path, &contents).expect("written");
        frames
    };

    assert!(palace.is_some() && saved > 0);

    for _ in 0..STEPPED {
        step(&mut app);
    }

    returning(
        &mut app,
        crate::menu::Return::Load {
            path,
            name: "q".to_owned(),
            place: Some(saves_at),
        },
    );
    app.world_mut()
        .resource_mut::<NextState<Screen>>()
        .set(Screen::Menu);

    for _ in 0..STEPPED {
        step(&mut app);
    }

    let world = app.world();
    let frames = frames(world);

    assert_eq!(*world.resource::<State<Screen>>().get(), Screen::Playing);
    assert!(
        (saved..saved + STEPPED).contains(&frames),
        "{frames} goes on from {saved}"
    );
    assert_eq!(world.resource::<select::Selected>().0, palace);

    let orders = world.resource::<orders::Orders>();

    assert_eq!(orders.rewards, rewards);
}

/// Bytes allocated less bytes freed, on this thread: the allocator
/// counts per thread, so tests running beside do not show.
mod held {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;

    thread_local! {
        static NET: Cell<isize> = const { Cell::new(0) };
    }

    fn add(bytes: usize, sign: isize) {
        let _ = NET.try_with(|n| n.set(n.get() + sign * bytes.cast_signed()));
    }

    pub struct Counting;

    // SAFETY: `System`'s, each call counted.
    unsafe impl GlobalAlloc for Counting {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            add(layout.size(), 1);
            // SAFETY: the caller's layout, passed on.
            unsafe { System.alloc(layout) }
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            add(layout.size(), -1);
            // SAFETY: the caller's pointer and layout, passed on.
            unsafe { System.dealloc(ptr, layout) }
        }

        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
            add(size, 1);
            add(layout.size(), -1);
            // SAFETY: the caller's arguments, passed on.
            unsafe { System.realloc(ptr, layout, size) }
        }
    }

    #[global_allocator]
    static COUNTING: Counting = Counting;

    /// This thread's bytes held.
    pub fn now() -> isize {
        NET.with(Cell::get)
    }
}

/// Games the leak test makes and drops.
const GAMES: usize = 3;

/// Bytes a game may leave behind: none of its world, VM or save. A
/// game's world alone holds megabytes.
const LEFT: isize = 1 << 20;

/// A quest's game, made as the menus make it and ended, its corpus
/// shelved as `uninstall` shelves it, leaves nothing else behind: each
/// game after the first holds no more than the one before. The first
/// fills the caches: archives, program, corpus.
///
/// The count misses what std's own code allocates (a 91 MB `fs::read`
/// counted 0): the archives' bytes, the save's.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn an_ended_game_frees_its_world_and_vm() {
    let (quests, mut maker) = quest_maker();
    let mut shelf = Shelf::default();
    let mut held = || {
        let before = held::now();
        let Made {
            corpus,
            mut sim,
            session,
            ..
        } = {
            let source = crate::game::begin::GameSource::Quest {
                offered: &quests[0],
                word: 1,
            };

            maker.make(source, &mut shelf).expect("made")
        };

        sim.step();
        let paths = corpus.paths().to_vec();

        shelf.put(&paths, corpus);
        drop((sim, session));
        held::now() - before
    };
    let left: Vec<isize> = (0..GAMES).map(|_| held()).collect();

    assert!(
        left[1..].iter().all(|&l| l < LEFT),
        "bytes left per game: {left:?}"
    );
}

/// Start Quest on the menus makes the quest's game at runtime and
/// plays it. Nothing is recorded.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn the_menus_start_a_quest_at_runtime() {
    let (quests, maker) = quest_maker();
    let mut settings = Settings::default();

    settings.audio.muted = true;
    let menu = crate::menu::Opened::starting(quests, 0);
    let mut app = menu_app(menu, (maker, nobody()), 1.0, settings, Display::Headless);
    let frame = Duration::from_millis(MS_PER_FRAME.into());

    app.insert_resource(TimeUpdateStrategy::ManualDuration(frame));

    // Frames drawn, not waiting, while the game is made off the frame.
    let began = std::time::Instant::now();
    let playing = |app: &App| *app.world().resource::<State<Screen>>().get() == Screen::Playing;

    while !playing(&app) && began.elapsed() < MAKING {
        step(&mut app);
    }

    assert!(playing(&app), "the quest's game made in time");

    for _ in 0..STEPPED {
        step(&mut app);
    }

    let world = app.world();

    assert_eq!(*world.resource::<State<Screen>>().get(), Screen::Playing);
    assert!(session(&app).quest.is_some(), "the quest's game");
    assert!(frames(world) > 0, "game frames ran");
    assert!(!world.resource::<ui::Paused>().0);
}

/// The freestyle test's Map Number.
const FREESTYLE_MAP: i32 = 7;

/// A preset pressed on the pick page: its game made as the custom
/// game's options make it, titled by its Random pick; won, it records
/// nothing, and the menus come back on the main menu.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn the_menus_start_a_freestyle_preset_which_records_no_victory() {
    let (quests, maker) = quest_maker();
    let scratch = crate::testdata::scratch("menus_freestyle");
    let base = scratch.path();
    let profile = crate::files::profile::Profiles::at(base)
        .create("P")
        .expect("a scratch profile");
    let mut settings = Settings::default();

    settings.audio.muted = true;
    let menu = crate::menu::Opened::starting_freestyle(quests, Preset::Beginner, FREESTYLE_MAP);
    let mut app = menu_app(menu, (maker, nobody()), 1.0, settings, Display::Headless);
    let frame = Duration::from_millis(MS_PER_FRAME.into());

    app.insert_resource(TimeUpdateStrategy::ManualDuration(frame));
    app.insert_resource(crate::files::profile::Player {
        profile: Some(profile),
        recorded: false,
    });

    for _ in 0..STEPPED {
        step(&mut app);
    }

    let setup = session(&app).setup.expect("a custom game's setup");
    let text = &app.world().resource::<Corpus>().text;
    let title = regent_core::view::panel::scroll::title(
        regent_core::view::panel::scroll::ScrollSubject::Custom(&setup),
        text,
    );

    assert_eq!(
        *app.world().resource::<State<Screen>>().get(),
        Screen::Playing
    );
    assert!(session(&app).quest.is_none(), "no quest");
    assert_eq!(setup.scenario(), FourCc::new(b"BEGI"));
    assert_eq!(setup.seed(), FREESTYLE_MAP.cast_unsigned());
    assert_eq!(setup.gold(), Preset::Beginner.gold());
    assert_eq!(title.as_deref(), Some("Random (Beginner)"));

    run(app.world(), |sim| {
        sim.input(Input::Cheat(CheatCode {
            cheat: Cheat::Win,
            arg: CROWN,
        }));
    });

    for _ in 0..STEPPED {
        step(&mut app);
    }

    let player = app.world().resource::<crate::files::profile::Player>();
    let progress = player.profile.as_ref().map(|p| p.progress.clone());

    assert_eq!(
        run(app.world(), |sim| sim.world().ended(CROWN)),
        Some(regent_core::sim::world::defeat::End::Won)
    );
    assert!(!player.recorded);
    assert_eq!(progress, Some(Progress::default()), "nothing recorded");

    returning(&mut app, crate::menu::Return::Main);
    app.world_mut()
        .resource_mut::<NextState<Screen>>()
        .set(Screen::Menu);

    for _ in 0..STEPPED {
        step(&mut app);
    }

    assert_eq!(*app.world().resource::<State<Screen>>().get(), Screen::Menu);
}

/// Options' RESTART answered YES, as `ui::quit` takes it: the menus
/// on the game's start page, the game taken down.
fn restart(app: &mut App) {
    let quest = session(app).quest_key();

    returning(app, crate::menu::Return::Restart(quest));
    app.world_mut()
        .resource_mut::<NextState<Screen>>()
        .set(Screen::Menu);

    for _ in 0..STEPPED {
        step(app);
    }
}

/// The map word `play.input` in `dir` records.
fn recorded_word(dir: &std::path::Path) -> Option<String> {
    crate::game::record::read(&dir.join("play.input"))
        .expect("recorded")
        .setup
        .map_seed
        .map(|word| word.to_string())
}

/// Whether a new game's loading page covers it.
fn draped(app: &App) -> bool {
    app.world().resource::<crate::menu::Drape>().is_up()
}

/// A drawn frame, a game being made finished first, nothing pressed.
fn waited(app: &mut App) {
    crate::menu::settle(app.world());
    app.update();
}

/// A quest made is put in under its loading page, Start Quest offered,
/// and waits there, its clock held, until that is pressed.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_made_quest_waits_for_start_quest() {
    let (quests, maker) = recording_maker(None);
    let mut settings = Settings::default();

    settings.audio.muted = true;

    let menu = crate::menu::Opened::starting(quests, 0);
    let mut app = menu_app(menu, (maker, nobody()), 1.0, settings, Display::Headless);
    let screen = |app: &App| *app.world().resource::<State<Screen>>().get();
    let frame = Duration::from_millis(MS_PER_FRAME.into());

    // A game frame owed each drawn frame, were the clock running.
    app.insert_resource(TimeUpdateStrategy::ManualDuration(frame));

    for _ in 0..STEPPED {
        waited(&mut app);
    }

    assert_eq!(screen(&app), Screen::Playing);
    assert!(draped(&app), "under its loading page");
    assert!(app.world().resource::<crate::menu::Menu>().offered());
    assert_eq!(frames(app.world()), 0, "the clock held");

    for _ in 0..STEPPED {
        step(&mut app);
    }

    assert!(!draped(&app));
    assert!(frames(app.world()) > 0, "game frames ran");
}

/// The page over a new game goes by itself once its drawing settles,
/// not before; Start Quest on a held one is offered only then. Under
/// it, the clock does not run. An app that draws: settled only after
/// quiet frames (`regent_draw::settle`).
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_new_games_loading_page_waits_for_its_drawing_to_settle() {
    const MOST: u32 = 600;

    let starts = [
        (
            Wait::Free,
            crate::menu::Opened::new as fn(_, _) -> crate::menu::Opened,
        ),
        (Wait::Held, |quests, _| {
            crate::menu::Opened::starting(quests, 0)
        }),
    ];

    for (wait, opened) in starts {
        let (quests, maker) = commanding_maker(None, &[]);
        let mut settings = Settings::default();
        let mut menu = opened(quests, crate::menu::Art::default());

        settings.audio.muted = true;

        if wait == Wait::Free {
            menu.with(|m| m.menu.command(crate::menu::LoadKind::Freestyle));
        }

        let mut app = menu_app(menu, (maker, nobody()), 1.0, settings, Display::Headless);
        let frame = Duration::from_millis(MS_PER_FRAME.into());
        let playing = |app: &App| *app.world().resource::<State<Screen>>().get() == Screen::Playing;

        app.insert_resource(TimeUpdateStrategy::ManualDuration(frame))
            .insert_resource(regent_draw::settle::Settle::default());

        let began = std::time::Instant::now();

        while !playing(&app) && began.elapsed() < MAKING {
            waited(&mut app);
        }

        let mut under = 0;

        while draped(&app) && under < MOST {
            let settled = || {
                app.world()
                    .resource::<regent_draw::settle::Settle>()
                    .settled()
            };
            let offered = app.world().resource::<crate::menu::Menu>().offered();

            assert_eq!(frames(app.world()), 0, "the clock held");
            assert!(!offered || settled(), "Start Quest offered unsettled");

            if offered {
                app.world_mut().resource_mut::<crate::menu::Menu>().go();
            }

            waited(&mut app);
            under += 1;

            let settled = app
                .world()
                .resource::<regent_draw::settle::Settle>()
                .settled();

            assert!(draped(&app) || settled, "gone unsettled");
        }

        assert!(!draped(&app), "{wait:?}: still up after {MOST} frames");
        assert!(under >= regent_draw::settle::QUIET, "{wait:?}: up {under}");

        for _ in 0..STEPPED {
            waited(&mut app);
        }

        assert!(frames(app.world()) > 0, "{wait:?}: game frames ran");
    }
}

/// How a test's game leaves its loading page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Wait {
    /// By itself.
    Free,
    /// On Start Quest.
    Held,
}

/// A restarted quest: its summary open, no game; Start Quest there
/// makes it again on a new map word, a new session recorded.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_restarted_quest_opens_its_summary_and_starts_on_a_new_map() {
    let scratch = crate::testdata::scratch("restart_quest");
    let base = scratch.path();
    let recorded = base.join("recorded");
    let (quests, maker) = recording_maker(Some(recorded.clone()));
    let key = quests[0].quest.key;
    let mut settings = Settings::default();

    std::fs::create_dir_all(&recorded).expect("scratch");
    settings.audio.muted = true;

    let menu = crate::menu::Opened::starting(quests, 0);
    let mut app = menu_app(menu, (maker, nobody()), 1.0, settings, Display::Headless);
    let frame = Duration::from_millis(MS_PER_FRAME.into());
    let screen = |app: &App| *app.world().resource::<State<Screen>>().get();

    app.insert_resource(TimeUpdateStrategy::ManualDuration(frame));

    for _ in 0..STEPPED {
        step(&mut app);
    }

    let first = recorded_word(&recorded);

    assert_eq!(screen(&app), Screen::Playing);
    assert!(first.is_some(), "the map word recorded");

    restart(&mut app);

    let menu = app.world().resource::<crate::menu::Menu>();

    assert_eq!(screen(&app), Screen::Menu);
    assert!(app.world().get_resource::<SimThread>().is_none(), "no game");
    assert_eq!(menu.page(), crate::menu::Page::Summary(0));

    assert!(
        app.world_mut()
            .resource_mut::<crate::menu::Menu>()
            .start_quest()
    );

    for _ in 0..STEPPED {
        step(&mut app);
    }

    let again = recorded_word(&recorded);

    assert_eq!(screen(&app), Screen::Playing);
    assert_eq!(session(&app).quest_key(), Some(key));
    assert!(frames(app.world()) > 0, "the new game runs");
    assert!(
        again.is_some() && again != first,
        "{again:?} after {first:?}"
    );
    assert!(recorded.join("play.input.1").exists(), "a new session");
}

/// A restarted freestyle game: the pick page open, a new Map Number
/// rolled, no game.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_restarted_freestyle_game_opens_the_pick_page_on_a_new_map_number() {
    let (quests, maker) = quest_maker();
    let mut settings = Settings::default();

    settings.audio.muted = true;
    let menu = crate::menu::Opened::starting_freestyle(quests, Preset::Beginner, FREESTYLE_MAP);
    let mut app = menu_app(menu, (maker, nobody()), 1.0, settings, Display::Headless);
    let frame = Duration::from_millis(MS_PER_FRAME.into());

    app.insert_resource(TimeUpdateStrategy::ManualDuration(frame));

    for _ in 0..STEPPED {
        step(&mut app);
    }

    assert!(session(&app).setup.is_some());

    restart(&mut app);

    let menu = app.world().resource::<crate::menu::Menu>();

    assert_eq!(*app.world().resource::<State<Screen>>().get(), Screen::Menu);
    assert!(app.world().get_resource::<SimThread>().is_none(), "no game");
    assert_eq!(menu.page(), crate::menu::Page::Freestyle);
    assert_ne!(
        app.world()
            .resource::<crate::menu::Freestyle>()
            .custom
            .map_number,
        FREESTYLE_MAP
    );
}

/// The command line's game restarts as the menus' do: its quest's
/// summary, else the pick page.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_command_line_game_restarts_to_its_start_page() {
    let cases: [(&'static [&'static str], Option<&[u8; 4]>); 2] = [
        (&["--quest", "QU01"], Some(b"QU01")),
        (&["--map-seed", "1"], None),
    ];

    for (args, quest) in cases {
        let (quests, maker) = commanding_maker(None, args);
        let offered = |key| quests.iter().position(|q| q.quest.key == FourCc::new(key));
        let page = match quest.and_then(offered) {
            Some(index) => crate::menu::Page::Summary(index),
            None => crate::menu::Page::Freestyle,
        };
        let mut settings = Settings::default();
        let mut menu = crate::menu::Opened::new(quests, crate::menu::Art::default());

        settings.audio.muted = true;
        menu.with(|m| {
            m.menu
                .command(quest.map_or(crate::menu::LoadKind::Freestyle, |k| {
                    crate::menu::LoadKind::Quest(FourCc::new(k))
                }));
        });

        let mut app = menu_app(menu, (maker, nobody()), 1.0, settings, Display::Headless);
        let frame = Duration::from_millis(MS_PER_FRAME.into());

        app.insert_resource(TimeUpdateStrategy::ManualDuration(frame));

        for _ in 0..STEPPED {
            step(&mut app);
        }

        restart(&mut app);

        assert_eq!(*app.world().resource::<State<Screen>>().get(), Screen::Menu);
        assert!(app.world().get_resource::<SimThread>().is_none(), "no game");
        assert_eq!(
            app.world().resource::<crate::menu::Menu>().page(),
            page,
            "{args:?}"
        );
    }
}

/// The assets standing: images, meshes, colours, bodies, bands.
/// Drawn frames until no sprite page job is left, the pages asked for
/// put on their sheets and drawn: the clock held meanwhile, so the game is at the
/// frame it was, a frame a drawn frame after.
fn landed(app: &mut App) {
    let began = std::time::Instant::now();
    let jobs = |app: &App| app.world().resource::<regent_draw::art::Sprites>().jobs();
    let frame = Duration::from_millis(MS_PER_FRAME.into());

    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO));

    while jobs(app) > 0 && began.elapsed() < MAKING {
        step(app);
    }

    // The pieces waiting on them drawn.
    step(app);
    app.insert_resource(TimeUpdateStrategy::ManualDuration(frame));
}

fn assets(app: &App) -> [usize; 5] {
    let world = app.world();

    [
        world.resource::<Assets<Image>>().len(),
        world.resource::<Assets<Mesh>>().len(),
        world.resource::<Assets<regent_draw::plain::Plain>>().len(),
        world.resource::<Assets<material::Body>>().len(),
        world.resource::<Assets<material::Band>>().len(),
    ]
}

/// Where the state a game may leave is defined: the crates whose
/// resources the leftovers check looks at.
const OURS: [&str; 2] = ["openregent::", "regent_draw::"];

/// The resources of [`OURS`] that outlast a game, though one changes
/// them: the app's, the menus' and the shelf.
fn outlasting() -> [std::any::TypeId; 15] {
    use std::any::TypeId;

    [
        TypeId::of::<Settings>(),
        TypeId::of::<Speedup>(),
        TypeId::of::<crate::files::env::Install>(),
        TypeId::of::<Maker>(),
        TypeId::of::<crate::menu::Menu>(),
        TypeId::of::<crate::files::profile::Player>(),
        TypeId::of::<crate::files::scores::Scoring>(),
        TypeId::of::<crate::files::lore::Catalogue>(),
        TypeId::of::<crate::ui::editor::Editor>(),
        TypeId::of::<Shelf>(),
        TypeId::of::<regent_draw::art::Art>(),
        TypeId::of::<Sprites>(),
        TypeId::of::<crate::audio::Narration>(),
        TypeId::of::<crate::audio::ClickSounds>(),
        crate::audio::music_type(),
    ]
}

/// The resources of [`OURS`] changed after `began` and not since
/// `ending`, by name; those [`outlasting`] if `lasting` counts them.
fn touched(world: &World, (began, ending): (Tick, Tick), lasting: Lasting) -> Vec<String> {
    let now = world.read_change_tick();
    let outlasting = outlasting();

    world
        .iter_resources()
        .filter(|(id, info, _)| {
            let name = info.name().to_string();
            let ticks = world.get_resource_change_ticks_by_id(*id);
            let changed = ticks.is_some_and(|t| {
                t.changed.is_newer_than(began, now) && !t.changed.is_newer_than(ending, now)
            });
            let outlasts = info.type_id().is_some_and(|t| outlasting.contains(&t));

            OURS.iter().any(|o| name.starts_with(o))
                && changed
                && (lasting == Lasting::Counted || !outlasts)
        })
        .map(|(_, info, _)| info.name().to_string())
        .collect()
}

/// Whether [`touched`] counts the resources that outlast a game.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Lasting {
    Counted,
    Skipped,
}

/// Whether this process holds the file at `path` open.
fn held_open(path: &std::path::Path) -> bool {
    std::fs::read_dir("/proc/self/fd").is_ok_and(|fds| {
        fds.filter_map(Result::ok)
            .any(|fd| std::fs::read_link(fd.path()).is_ok_and(|p| p == path))
    })
}

/// A recorded game left mid-play for the menus: by the drawn frame the
/// game is out, its thread is joined and its recording closed, so no
/// line of it can land in the next game's `play.input`.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_game_left_mid_play_joins_its_thread_and_closes_its_recording() {
    let scratch = crate::testdata::scratch("left_mid_play");
    let base = scratch.path();
    let recorded = base.join("recorded");
    let (quests, maker) = recording_maker(Some(recorded.clone()));
    let mut settings = Settings::default();

    std::fs::create_dir_all(&recorded).expect("scratch");
    settings.audio.muted = true;

    let menu = crate::menu::Opened::starting(quests, 0);
    let mut app = menu_app(menu, (maker, nobody()), 1.0, settings, Display::Headless);
    let frame = Duration::from_millis(MS_PER_FRAME.into());

    app.insert_resource(TimeUpdateStrategy::ManualDuration(frame));

    for _ in 0..STEPPED {
        step(&mut app);
    }

    let file = recorded
        .join("play.input")
        .canonicalize()
        .expect("recorded");
    let ended = app.world().resource::<SimThread>().ended();

    assert!(held_open(&file), "recording");
    assert!(!ended(), "running");

    returning(&mut app, crate::menu::Return::Main);
    app.world_mut()
        .resource_mut::<NextState<Screen>>()
        .set(Screen::Menu);
    step(&mut app);

    assert!(app.world().get_resource::<SimThread>().is_none(), "no game");
    assert!(ended(), "the thread joined");
    assert!(!held_open(&file), "the recording closed");
}

/// A game played, ended and another started: as the first ends every
/// resource it changed is back to its start or gone (`scope`), but
/// those that outlast a game; the second runs.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_game_leaves_nothing_for_the_next() {
    let (quests, maker) = quest_maker();
    let mut settings = Settings::default();

    settings.audio.muted = true;
    let menu = crate::menu::Opened::starting(quests, 0);
    let mut app = menu_app(menu, (maker, nobody()), 1.0, settings, Display::Headless);
    let frame = Duration::from_millis(MS_PER_FRAME.into());

    app.insert_resource(TimeUpdateStrategy::ManualDuration(frame));

    // The loading page: the game not yet made.
    step(&mut app);
    let began = app.world().read_change_tick();

    for _ in 0..STEPPED {
        step(&mut app);
    }

    assert_eq!(
        *app.world().resource::<State<Screen>>().get(),
        Screen::Playing
    );

    returning(&mut app, crate::menu::Return::Main);
    app.world_mut()
        .resource_mut::<NextState<Screen>>()
        .set(Screen::Menu);

    let ending = app.world().read_change_tick();

    step(&mut app);

    let world = app.world();

    assert_eq!(*world.resource::<State<Screen>>().get(), Screen::Menu);
    assert!(
        world.get_resource::<SimThread>().is_none(),
        "the game is out"
    );

    // The check sees names and changes: the start marked the player.
    let touched_all = touched(world, (began, ending), Lasting::Counted);

    assert!(
        touched_all
            .iter()
            .any(|n| n == "openregent::files::profile::Player"),
        "{touched_all:?}"
    );
    assert_eq!(
        touched(world, (began, ending), Lasting::Skipped),
        Vec::<String>::new()
    );

    app.world_mut().resource_mut::<crate::menu::Menu>().ask(1);

    for _ in 0..STEPPED {
        step(&mut app);
    }

    assert_eq!(
        *app.world().resource::<State<Screen>>().get(),
        Screen::Playing
    );
    assert!(frames(app.world()) > 0, "the next game runs");
}

/// The same quest played twice from the menus: the second game makes
/// no art the first made, as its corpus and art come off the shelf;
/// between them, the game's entities are gone and its art stays.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_second_game_on_a_dataset_reuses_the_firsts_art() {
    let (quests, maker) = quest_maker();
    let mut settings = Settings::default();

    settings.audio.muted = true;
    let menu = crate::menu::Opened::starting(quests, 0);
    let mut app = menu_app(menu, (maker, nobody()), 1.0, settings, Display::Headless);
    let frame = Duration::from_millis(MS_PER_FRAME.into());
    let entities = |app: &mut App| {
        app.world_mut()
            .query_filtered::<Entity, Without<IsResource>>()
            .iter(app.world())
            .count()
    };
    let index = |app: &App| app.world().resource::<Corpus>().catalogue.archives.as_ptr();

    app.insert_resource(TimeUpdateStrategy::ManualDuration(frame));

    // The loading page: the game not yet made.
    step(&mut app);
    let standing = (entities(&mut app), assets(&app));

    for _ in 0..STEPPED {
        step(&mut app);
    }

    landed(&mut app);

    let first = index(&app);
    let pages = app.world().resource::<regent_draw::art::Sprites>().pages();

    returning(&mut app, crate::menu::Return::Main);
    app.world_mut()
        .resource_mut::<NextState<Screen>>()
        .set(Screen::Menu);

    for _ in 0..STEPPED {
        step(&mut app);
    }

    let between = assets(&app);

    assert_eq!(entities(&mut app), standing.0, "no entity of the game");
    assert!(app.world().get_resource::<Corpus>().is_none());
    assert!(app.world().resource::<Shelf>().len() > 0, "the art shelved");
    assert!(
        between.iter().zip(standing.1).all(|(b, s)| *b >= s) && between[0] > standing.1[0],
        "the art stays: {between:?} after {:?}",
        standing.1
    );

    app.world_mut().resource_mut::<crate::menu::Menu>().ask(0);
    step(&mut app);

    for _ in 0..STEPPED {
        step(&mut app);
    }

    landed(&mut app);

    assert_eq!(
        *app.world().resource::<State<Screen>>().get(),
        Screen::Playing
    );
    assert_eq!(index(&app), first, "the first game's corpus");
    // No sprite page made twice: the first game's every set kept. The
    // other assets follow timing, not art made again: a key the first
    // game passed while its pages were on their way draws now (its
    // materials), and the ground's patches follow when the view settled.
    assert_eq!(
        app.world().resource::<regent_draw::art::Sprites>().pages(),
        pages,
        "no page made twice"
    );
}

/// The high scores at `path` hold one won quest's time, `monarch`'s.
fn scored_once(path: &std::path::Path, monarch: &str) {
    use regent_core::view::menu::scores::{Category, GameKind};

    let table = crate::files::scores::read(path).expect("the high scores");
    let times = table.list(GameKind::Quests, Category::Time);

    assert_eq!(times.len(), 1, "recorded once");
    assert_eq!(times[0].monarch, monarch);
}

/// A won quest the menus started is recorded in the profile and the
/// high scores; the end panel's Main Menu takes the game out and shows
/// the main menu; and another quest then starts.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_won_quest_is_recorded_and_the_menus_come_back_for_another() {
    let (quests, maker) = quest_maker();
    let first = quests[0].quest.key;
    let scratch = crate::testdata::scratch("menus_again");
    let base = scratch.path();
    let profile = crate::files::profile::Profiles::at(base)
        .create("P")
        .expect("a scratch profile");
    let mut settings = Settings::default();

    settings.audio.muted = true;
    let menu = crate::menu::Opened::starting(quests, 0);
    let mut app = menu_app(menu, (maker, nobody()), 1.0, settings, Display::Headless);
    let frame = Duration::from_millis(MS_PER_FRAME.into());
    let entities = |app: &mut App| {
        app.world_mut()
            .query_filtered::<Entity, Without<IsResource>>()
            .iter(app.world())
            .count()
    };

    let scores = base.join("scores.toml");

    app.insert_resource(TimeUpdateStrategy::ManualDuration(frame));
    app.insert_resource(crate::files::profile::Player {
        profile: Some(profile),
        recorded: false,
    });
    app.insert_resource(crate::files::scores::Scoring {
        place: Some(scores.clone()),
        scored: false,
    });

    // The loading page: the game not yet made.
    step(&mut app);
    let standing = entities(&mut app);

    assert_eq!(crate::audio::track(app.world()), Some(front_end::EPIC));

    for _ in 0..STEPPED {
        step(&mut app);
    }

    let asked = run(app.world(), |sim| sim.music());

    assert!(asked.is_some(), "the quest's script plays its own");
    assert_eq!(
        crate::audio::track(app.world()),
        asked,
        "the game's music took over"
    );

    run(app.world(), |sim| {
        sim.input(Input::Cheat(CheatCode {
            cheat: Cheat::Win,
            arg: CROWN,
        }));
    });

    for _ in 0..STEPPED {
        step(&mut app);
    }

    let player = app.world().resource::<crate::files::profile::Player>();
    let won = player.profile.as_ref().map(|p| p.progress.victories(first));

    assert!(player.recorded);
    assert_eq!(won, Some(1), "recorded once");

    scored_once(&scores, "P");

    // The "Game Over" box waits for OK; then the advisor done and the
    // fade over, AP60 and the quest's end voice.
    assert_eq!(
        app.world().resource::<crate::ui::end::EndScreen>().panel(),
        None
    );
    assert_eq!(crate::audio::narrated(app.world()), None);

    press(&mut app, crate::ui::end::Press::Ok);

    for _ in 0..FADED + SPOKEN {
        step(&mut app);
    }

    let end_voice = regent_core::view::menu::end_voice(first, End::Won);
    let wave = end_voice.map(|v| FourCc([b'E', b'N', v.0[2], v.0[3]]));

    assert_eq!(
        app.world().resource::<crate::ui::end::EndScreen>().panel(),
        Some(Panel::of(End::Won))
    );
    assert!(wave.is_some(), "{first} has an end voice");
    assert_eq!(crate::audio::narrated(app.world()), wave);

    // The end panel's Main Menu.
    press(&mut app, crate::ui::end::Press::Button(EndButton::MainMenu));

    for _ in 0..STEPPED {
        step(&mut app);
    }

    assert_eq!(*app.world().resource::<State<Screen>>().get(), Screen::Menu);
    assert!(
        app.world().get_resource::<SimThread>().is_none(),
        "the game is out"
    );
    assert!(app.world().get_resource::<Maps>().is_none());
    assert_eq!(entities(&mut app), standing, "nothing of the game stands");
    assert_eq!(crate::audio::track(app.world()), Some(front_end::THEME));
    assert_eq!(
        crate::audio::narrated(app.world()),
        None,
        "the voice stopped"
    );

    app.world_mut().resource_mut::<crate::menu::Menu>().ask(1);

    for _ in 0..STEPPED {
        step(&mut app);
    }

    let world = app.world();

    assert_eq!(*world.resource::<State<Screen>>().get(), Screen::Playing);
    assert!(frames(world) > 0, "the next game runs");
    assert!(!world.resource::<crate::files::profile::Player>().recorded);
}

/// A won game's statistics page, then CONTINUE PLAYING: the game runs
/// on, the end gone; a later end comes again.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_won_game_goes_on_after_continue_playing() {
    let (quests, maker) = quest_maker();
    let mut settings = Settings::default();

    settings.audio.muted = true;
    let menu = crate::menu::Opened::starting(quests, 0);
    let mut app = menu_app(menu, (maker, nobody()), 1.0, settings, Display::Headless);
    let frame = Duration::from_millis(MS_PER_FRAME.into());
    let shown = |app: &App| app.world().resource::<crate::ui::end::EndScreen>().panel();
    let win = |app: &mut App| {
        run(app.world(), |sim| {
            sim.input(Input::Cheat(CheatCode {
                cheat: Cheat::Win,
                arg: CROWN,
            }));
        });

        for _ in 0..STEPPED {
            step(app);
        }

        press(app, crate::ui::end::Press::Ok);

        for _ in 0..FADED + SPOKEN {
            step(app);
        }
    };

    app.insert_resource(TimeUpdateStrategy::ManualDuration(frame));

    for _ in 0..=STEPPED {
        step(&mut app);
    }

    win(&mut app);

    let won = Panel::of(End::Won);
    let statistics = Panel {
        page: EndPage::Statistics,
        ..won
    };

    assert_eq!(shown(&app), Some(won));

    // RESTART is not on a won game's panel: nothing.
    press(&mut app, crate::ui::end::Press::Button(EndButton::Restart));
    assert_eq!(shown(&app), Some(won));

    press(
        &mut app,
        crate::ui::end::Press::Button(EndButton::Statistics),
    );
    assert_eq!(shown(&app), Some(statistics));

    let before = frames(app.world());

    press(&mut app, crate::ui::end::Press::Button(EndButton::Continue));

    let (ended, now) = run(app.world(), |sim| (sim.ended(), sim.frames()));

    assert!(!ended, "the end is gone");
    assert!(now > before, "the game runs");
    assert_eq!(shown(&app), None);
    assert_eq!(
        *app.world().resource::<State<Screen>>().get(),
        Screen::Playing
    );

    win(&mut app);
    assert_eq!(shown(&app), Some(won), "a later end comes again");
}

/// A GPU side over egui's default 2048 reaches egui, which caps the art
/// service's upscales at it: a 3200-wide window's 3072-wide room is made
/// at 3072, not at 2048 and stretched.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn egui_takes_the_gpus_texture_side() {
    const SIDE: usize = 8192;

    let (quests, maker) = quest_maker();
    let mut settings = Settings::default();

    settings.audio.muted = true;
    let menu = crate::menu::Opened::new(quests, crate::menu::Art::default());
    let mut app = menu_app(menu, (maker, nobody()), 1.0, settings, Display::Headless);

    app.insert_resource(art::side::Side(SIDE));
    step(&mut app);
    step(&mut app);

    let world = app.world_mut();
    let mut contexts =
        world.query_filtered::<&mut bevy_egui::EguiContext, With<PrimaryEguiContext>>();
    let mut context = contexts.single_mut(world).expect("egui");

    assert_eq!(context.get_mut().input(|i| i.max_texture_side), SIDE);
}

/// The news a game's drawn frames took: cues, lines and chat, counted
/// from the start, at each game frame a state showed.
#[derive(Resource, Default)]
struct Heard {
    /// The lists' lengths before `game::clock::tick`.
    before: [usize; 3],
    total: [usize; 3],
    at: Vec<(u32, [usize; 3])>,
}

/// The news lists' lengths.
fn lengths(news: &News) -> [usize; 3] {
    [news.cues.len(), news.said.len(), news.chat.len()]
}

#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
fn heard_before(news: Option<Res<News>>, mut heard: ResMut<Heard>) {
    if let Some(news) = news {
        heard.before = lengths(&news);
    }
}

/// What `game::clock::tick` added, counted; noted at the frame shown.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
fn heard_after(news: Option<Res<News>>, frame: Option<Res<Frame>>, mut heard: ResMut<Heard>) {
    if let (Some(news), Some(frame)) = (news, frame) {
        let now = lengths(&news);

        for (n, (now, before)) in now.iter().zip(heard.before).enumerate() {
            heard.total[n] += now - before;
        }

        let total = heard.total;

        if heard.at.last().is_none_or(|(f, _)| *f != frame.frames) {
            heard.at.push((frame.frames, total));
        }
    }
}

/// A freestyle game's app in `mode` at `speed`, recorded under `name`,
/// drawn until it shows `frames` game frames: the news it took, by
/// frame, and the digests it recorded.
fn heard_game(name: &str, mode: game::thread::Mode, speed: f32, frames: u32) -> (Heard, Vec<u64>) {
    let scratch = crate::testdata::scratch(name);
    let base = scratch.path();

    std::fs::create_dir_all(base.join("recorded")).expect("scratch");

    let (quests, maker) = recording_maker(Some(base.join("recorded")));
    let mut settings = Settings::default();
    let mut menu = crate::menu::Opened::new(quests, crate::menu::Art::default());

    settings.audio.muted = true;
    menu.with(|m| m.menu.command(crate::menu::LoadKind::Freestyle));

    let mut app = menu_app(menu, (maker, nobody()), speed, settings, Display::Headless);
    let frame = Duration::from_millis(MS_PER_FRAME.into());

    // Sync: a game frame each drawn frame. Pipelined: the wall clock's.
    app.insert_resource(TimeUpdateStrategy::ManualDuration(frame))
        .insert_resource(mode)
        .init_resource::<Heard>()
        .add_systems(
            Update,
            (
                heard_before.before(tick),
                heard_after.after(tick).before(crate::map::mirror::sync),
            ),
        );

    let began = std::time::Instant::now();
    let shown = |app: &App| app.world().get_resource::<Frame>().map_or(0, |f| f.frames);

    while shown(&app) < frames && began.elapsed() < MAKING {
        step(&mut app);

        if mode == game::thread::Mode::Pipelined {
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    let events = run(app.world(), |sim| sim.session())
        .expect("recorded")
        .events;
    let digests = events
        .iter()
        .filter_map(|e| match e {
            crate::game::record::Event::Digest { digest, .. } => Some(*digest),
            _ => None,
        })
        .collect();
    let heard = app.world_mut().remove_resource::<Heard>().expect("heard");

    (heard, digests)
}

/// A game drawn pipelined (its own clock, 8x, the drawing never
/// waiting) plays as one drawn in sync mode (a frame a drawn frame): at
/// each frame a pipelined state showed, the same cues, lines and chat
/// taken so far, and the same digests recorded.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_pipelined_game_plays_as_a_sync_one() {
    // The first cue comes at frame 400 or so.
    const FRAMES: u32 = 1_200;

    let (sync, sync_digests) = heard_game("pipelined-sync", game::thread::Mode::Sync, 1.0, FRAMES);
    let (piped, piped_digests) =
        heard_game("pipelined-own", game::thread::Mode::Pipelined, 8.0, FRAMES);
    let at: HashMap<u32, [usize; 3]> = sync.at.iter().copied().collect();
    let digests = (FRAMES / crate::game::record::DIGEST_EVERY) as usize;

    // The first drawn frame steps the first game frame.
    let missing: Vec<u32> = (1..FRAMES).filter(|f| !at.contains_key(f)).collect();

    assert!(missing.is_empty(), "sync shows every frame: {missing:?}");

    let compared: Vec<_> = piped
        .at
        .iter()
        .filter(|(f, _)| (1..FRAMES).contains(f))
        .collect();

    assert!(compared.len() > 10, "{compared:?}");

    for (frame, heard) in compared {
        assert_eq!(at.get(frame), Some(heard), "frame {frame}");
    }

    assert!(sync.total[0] > 0, "cues heard: {:?}", sync.total);
    assert!(sync_digests.len() >= digests && piped_digests.len() >= digests);
    assert_eq!(sync_digests[..digests], piped_digests[..digests]);
}

/// A new game drawn pipelined opens on its palace at the regular zoom,
/// smooth zoom on, though its first drawn frame comes before the sim
/// thread's first state, nothing mirrored: the view waits for the
/// palace's mirror rather than fitting the map.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
#[allow(clippy::float_cmp, reason = "the zoom is set, not worked out")]
fn a_pipelined_game_opens_on_its_palace_not_the_whole_map() {
    let (quests, maker) = commanding_maker(None, &[]);
    let mut settings = Settings::default();
    let mut menu = crate::menu::Opened::new(quests, crate::menu::Art::default());

    settings.audio.muted = true;
    settings.enhancements.smooth_zoom = true;
    menu.with(|m| m.menu.command(crate::menu::LoadKind::Freestyle));

    let mut app = menu_app(menu, (maker, nobody()), 1.0, settings, Display::Headless);
    let frame = Duration::from_millis(MS_PER_FRAME.into());

    app.insert_resource(TimeUpdateStrategy::ManualDuration(frame))
        .insert_resource(game::thread::Mode::Pipelined);

    let began = std::time::Instant::now();
    let framed = |app: &App| app.world().get_resource::<Framed>().is_some_and(|f| f.0);

    while !framed(&app) && began.elapsed() < MAKING {
        step(&mut app);
        std::thread::sleep(Duration::from_millis(5));
    }

    let world = app.world();
    let view = world.resource::<stage::View>();
    let seen = world.resource::<Frame>();
    let palace = seen
        .taken
        .world
        .player_palace(seen.local)
        .expect("a palace");

    assert!(framed(&app), "framed");
    assert_eq!(
        view.zoom, START_ZOOM,
        "the regular zoom, not the map fitted"
    );
    assert!(
        view.pan != Vec2::ZERO,
        "on the palace {palace}: {}",
        view.pan
    );
}

/// A LAN peer's menus and maker, recording in the scratch directory
/// `name`, LAN games hosted and joined on the install.
fn lan_peer(name: &str) -> (crate::testdata::Scratch, crate::menu::Opened, Maker) {
    let base = crate::testdata::scratch(name);

    let install = crate::files::env::Install::from_env().expect("MAJESTY_HD");
    let quests = crate::quests(&install).expect("quests");
    let mut menu = crate::menu::Opened::new(quests, crate::menu::Art::default());
    let maker = Maker::new(
        install.clone(),
        None,
        crate::game::begin::Recording::In(base.to_path_buf()),
    );

    menu.with(|m| m.lan.on(install));
    (base, menu, maker)
}

/// [`lan_peer`]'s app, muted, pipelined, at 8x.
fn lan_app((menu, maker): (crate::menu::Opened, Maker)) -> App {
    let mut settings = Settings::default();

    settings.audio.muted = true;

    let mut app = menu_app(menu, (maker, nobody()), 8.0, settings, Display::Headless);

    app.insert_resource(game::thread::Mode::Pipelined);
    app
}

/// Host and guest apps of a LAN game from the menus over localhost, the
/// lobby pages polled, ready from the first: scratch directories
/// `name`-host and `name`-guest.
fn lan_pair(name: &str) -> (App, App, [crate::testdata::Scratch; 2]) {
    let (host_dir, mut hosting, host_maker) = lan_peer(&format!("{name}-host"));
    #[allow(
        clippy::redundant_closure_for_method_calls,
        reason = "the method path fixes Flows' lifetime; the closure keeps it general"
    )]
    let port = hosting.with(|m| m.host_lan());
    let (guest_dir, mut joining, guest_maker) = lan_peer(&format!("{name}-guest"));

    joining.with(|m| m.join_lan(port));
    (
        lan_app((hosting, host_maker)),
        lan_app((joining, guest_maker)),
        [host_dir, guest_dir],
    )
}

/// Game frames shown in `app`; none out of a game.
fn shown_frames(app: &App) -> u32 {
    app.world().get_resource::<Frame>().map_or(0, |f| f.frames)
}

/// A LAN game started from the menus, as played: host and guest each
/// an app over localhost, lobby pages polled, games made side by side,
/// pipelined, at 8x. Both play some hundred frames alike, no peer
/// dropped, none stopped.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_lan_game_from_the_menus_plays_alike() {
    const FRAMES: u32 = 400;

    let (mut host, mut guest, _dirs) = lan_pair("lan-menu");
    let shown = shown_frames;
    let began = std::time::Instant::now();

    // No settle: the two makings run at once, as on two machines.
    while (shown(&host) < FRAMES || shown(&guest) < FRAMES) && began.elapsed() < MAKING {
        host.update();
        guest.update();
        std::thread::sleep(Duration::from_millis(5));
    }

    let look = |app: &App| {
        run(app.world(), |sim| {
            let digests: Vec<u64> = sim
                .session()
                .expect("recorded")
                .events
                .iter()
                .filter_map(|e| match e {
                    crate::game::record::Event::Digest { digest, .. } => Some(*digest),
                    _ => None,
                })
                .collect();

            let peers = sim.peer().players();

            (digests, (peers, sim.peer().dropped()), sim.stopped.clone())
        })
    };
    let (host_digests, (host_peers, host_dropped), host_stopped) = look(&host);
    let (guest_digests, (guest_peers, guest_dropped), guest_stopped) = look(&guest);
    let digests = (FRAMES / crate::game::record::DIGEST_EVERY) as usize;

    assert_eq!((host_peers, guest_peers), (2, 2), "a LAN game");
    assert!(
        !draped(&host) && !draped(&guest),
        "no loading page over a LAN game"
    );
    assert!(shown(&host) >= FRAMES && shown(&guest) >= FRAMES);
    assert_eq!((host_stopped, guest_stopped), (None, None));
    assert!(!host_dropped && !guest_dropped, "no peer dropped");
    assert!(host_digests.len() >= digests && guest_digests.len() >= digests);
    assert_eq!(host_digests[..digests], guest_digests[..digests]);
}

/// The host goes back to the lobby, as its end panel's Return to Lobby
/// does: the guest, still in the game, follows; both are seated in one
/// lobby, and the next game, ready from the first, starts and runs.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_lan_game_goes_back_to_its_lobby_and_plays_again() {
    const FRAMES: u32 = 50;

    let (mut host, mut guest, _dirs) = lan_pair("lan-back");
    let screen = |app: &App| *app.world().resource::<State<Screen>>().get();
    let both = |host: &mut App, guest: &mut App, done: &dyn Fn(&App, &App) -> bool| {
        let began = std::time::Instant::now();

        while !done(host, guest) && began.elapsed() < MAKING {
            host.update();
            guest.update();
            std::thread::sleep(Duration::from_millis(5));
        }

        done(host, guest)
    };
    let playing =
        |host: &App, guest: &App| shown_frames(host) >= FRAMES && shown_frames(guest) >= FRAMES;

    assert!(both(&mut host, &mut guest, &playing), "the first game runs");

    let back = host
        .world()
        .resource::<SimThread>()
        .regroup()
        .expect("a LAN game's socket");

    returning(&mut host, crate::menu::Return::Lobby(back));
    host.world_mut()
        .resource_mut::<NextState<Screen>>()
        .set(Screen::Menu);

    // Each seen in the lobby; the next game starts as soon as both are
    // seated, maybe within one pass.
    let seated = |app: &App| {
        screen(app) == Screen::Menu
            && app.world().resource::<crate::menu::Menu>().page() == crate::menu::Page::Lobby
    };
    let mut lobby = [false; 2];
    let began = std::time::Instant::now();

    while !(lobby == [true; 2] && playing(&host, &guest)) && began.elapsed() < MAKING {
        host.update();
        guest.update();
        lobby[0] |= seated(&host);
        lobby[1] |= seated(&guest);
        std::thread::sleep(Duration::from_millis(5));
    }

    assert_eq!(lobby, [true; 2], "both in the lobby");
    assert!(playing(&host, &guest), "the next game runs");

    let peers = |app: &App| run(app.world(), |sim| (sim.peer().players(), sim.peer().seat()));

    assert_eq!((peers(&host), peers(&guest)), ((2, 0), (2, 1)));
}

/// The pan as the panels' pass left it, the rest of the pass reads.
#[derive(Resource, Default)]
struct Dragged(Vec2);

/// Centres the view on the map's corner, as a minimap drag there does:
/// in the panels' pass.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
fn drag_mini(maps: Res<Maps>, mut view: ResMut<stage::View>, mut dragged: ResMut<Dragged>) {
    crate::ui::minimap::look_at(&maps, &mut view, (0, 0));
    dragged.0 = view.pan;
}

/// A minimap drag to the map's corner leaves the view inside the map
/// at once: what the rest of the panels' pass draws (names) and the
/// cameras (`present::follow`) read it bounded, not a frame later.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_minimap_drag_to_the_corner_keeps_the_view_on_the_map() {
    let (quests, maker) = commanding_maker(None, &[]);
    let mut settings = Settings::default();
    let mut menu = crate::menu::Opened::new(quests, crate::menu::Art::default());

    settings.audio.muted = true;
    menu.with(|m| m.menu.command(crate::menu::LoadKind::Freestyle));

    let mut app = menu_app(menu, (maker, nobody()), 1.0, settings, Display::Headless);
    let frame = Duration::from_millis(MS_PER_FRAME.into());

    app.insert_resource(TimeUpdateStrategy::ManualDuration(frame))
        .init_resource::<Dragged>()
        .add_systems(
            bevy_egui::EguiPrimaryContextPass,
            drag_mini
                .after(crate::draw::frame::Ui::Bar)
                .run_if(crate::menu::in_game),
        );

    for _ in 0..STEPPED {
        step(&mut app);
    }

    let view = app.world().resource::<stage::View>();
    let (extent, rect) = (view.bounds.expect("bounded"), view.rect.expect("a room"));
    let bounded = stage::bounded(view.pan, view.zoom, rect.size(), extent);

    assert_eq!(view.pan, bounded, "drawn inside the map");
    assert_eq!(app.world().resource::<Dragged>().0, bounded, "at once");
}

/// Each drawn scatter part's record and place, in order.
fn strewn(app: &mut App) -> Vec<(usize, [u32; 3])> {
    let mut parts: Vec<(usize, [u32; 3])> = app
        .world_mut()
        .query::<(&regent_draw::map::things::Record, &Transform)>()
        .iter(app.world())
        .filter(|(record, _)| record.chunk == SCATTER)
        .map(|(record, put)| (record.index, put.translation.to_array().map(f32::to_bits)))
        .collect();

    parts.sort_unstable();
    parts
}

/// A LAN resync to another board, smaller, at the same ground
/// generation (`Board::relaid`), is drawn as that board: its ground and scatter as
/// a new drawing's, the fog's veil and the minimap's ground over them;
/// a selected piece it lacks is forgotten.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_resync_to_another_board_draws_that_board() {
    let (quests, maker) = commanding_maker(None, &["--map-seed", "1", "--side", "256"]);
    let mut settings = Settings::default();
    let mut menu = crate::menu::Opened::new(quests, crate::menu::Art::default());

    settings.audio.muted = true;
    menu.with(|m| m.menu.command(crate::menu::LoadKind::Freestyle));

    let mut app = menu_app(menu, (maker, nobody()), 1.0, settings, Display::Headless);
    let frame = Duration::from_millis(MS_PER_FRAME.into());

    app.insert_resource(TimeUpdateStrategy::ManualDuration(frame));

    for _ in 0..STEPPED {
        step(&mut app);
    }

    let cli = crate::cli::tests::parsed(["openregent", "--map-seed", "2", "--side", "128"]);
    let host = crate::game::begin::test_setup(&cli).0.sim;
    let theirs: std::collections::HashSet<u32> =
        host.world().shown().iter().map(|s| s.agent).collect();
    let ours = app.world().resource::<Frame>().taken.world.shown();
    let lacking = ours.iter().map(|s| s.agent).find(|a| !theirs.contains(a));

    app.world_mut().resource_mut::<select::Selected>().0 = lacking;

    run(app.world(), move |sim| sim.resync_to(&host)).expect("resynced");

    for _ in 0..STEPPED {
        step(&mut app);
    }

    let world = app.world().resource::<Frame>().taken.world.clone();
    let fresh = app.world().resource::<ground::Laid>().fresh(&world);
    let maps = app.world().resource::<Maps>();

    assert!(
        maps.current().map(|l| &l.source) == fresh.as_ref(),
        "the ground"
    );
    assert_eq!(app.world().resource::<select::Selected>().0, None);
    assert_eq!(crate::map::fog::stale_veils(app.world()), [], "the veil");

    let (mini, mini_fresh) = crate::ui::minimap::grounds(app.world(), &world);

    assert!(mini.is_some() && mini == mini_fresh, "the minimap's ground");

    // The scatter as a new drawing of the same map draws it: its pages
    // landed first, then kept.
    landed(&mut app);

    let drawn = strewn(&mut app);

    app.world_mut().resource_mut::<Maps>().revision += 1;
    step(&mut app);

    assert_ne!(drawn.len(), 0);
    assert_eq!(drawn, strewn(&mut app), "the scatter");
}
