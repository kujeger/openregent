//! A new Majesty game, run by the library's world, played and drawn.
//!
//! ```text
//! openregent [--speed <x>]                    the menus (`menu`)
//! openregent [--map-seed <n>] [--land <code>] ... [--seed <n>]
//! openregent --quest <name> [--map-seed <n>]
//! openregent --scene <scene.toml>
//! openregent --headless --frames <n>
//! openregent --replay <play.input> [--headless --frames <n>]
//! openregent [new-game options] --screenshot <out.png> [--size 1920x1200] [--scale <s>] [--frames <n>]
//! openregent --menu <page> [--screenshot <out.png>]   the menus on a page
//! openregent --server [PORT] --players <n> [new-game options] [--headless --frames <n>]
//! openregent --client <host[:port]> [--headless --frames <n>]
//! ```
//!
//! `--screenshot` draws the opening view in a window of `--size`, fog
//! lifted and paused, saves one frame after `--frames` (default 60) and
//! exits; it writes no log, recording or settings (`shot`).
//!
//! The simulation is `regent_core::sim::game::Game::free`; this
//! crate only steps it and draws. See `docs/openregent/game.md`, "The game".

mod audio;
mod cli;
mod draw;
#[cfg(test)]
mod examples;
mod files;
mod game;
mod headless;
mod input;
mod map;
mod menu;
mod net;
mod panel;
mod perf;
mod save;
mod setup;
#[cfg(test)]
mod testdata;
mod ui;
mod video;

use std::process::ExitCode;

use bevy::log::error;
use clap::{CommandFactory, FromArgMatches};
use regent_core::data::text::Text;
use regent_core::sim::world::offer::Menu;

use cli::{Cli, Lists, Opening, codes, hosted, lists, opening, quest, replayed};
use draw::shot::Capture;
use files::env::Install;
use headless::{headless, lan_headless};
use setup::Setup;

fn main() -> ExitCode {
    perf::init();

    let matches = Cli::command().get_matches();
    let cli = Cli::from_arg_matches(&matches).unwrap_or_else(|e| e.exit());
    let cli = hosted(cli, &matches);

    if cli.licences {
        print!("{}", ui::typeface::LICENCES);
        return ExitCode::SUCCESS;
    }

    // A screenshot leaves the user's log alone.
    files::log::start(match cli.mode.screenshot {
        None => files::log::Keep::File,
        Some(_) => files::log::Keep::Terminal,
    });

    let opens = match cli.mode.menu {
        Some(_) => Opening::Menus,
        None => opening(&matches),
    };

    let code = match run(&cli, opens) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            error!("{e}");
            ExitCode::FAILURE
        }
    };

    files::log::finish();
    code
}

fn run(cli: &Cli, opening: Opening) -> Result<(), String> {
    let install = Install::find()?;

    if cli.mode.headless {
        regent_draw::data::headless();
    }

    match opening {
        Opening::Menus => menus(cli, &install, (None, cli.mode.local)),
        Opening::Options => asked_for(cli, &install),
    }
}

/// With `--server`, the guests waited for and welcomed to `setup`'s
/// game; with `--client`, the host's game joined: its setup in `setup`'s
/// place, its seat this machine's. With this machine's player. Either
/// uses the last profile's nick, or its name where none was chosen. A
/// desync's snapshot goes beside the log.
fn lan(
    cli: &Cli,
    setup: Setup,
    install: &Install,
) -> Result<(Setup, u32, Option<net::Peer>), String> {
    let dumps = files::log::dir();
    let last = files::profile::Profiles::default_place().and_then(|p| p.opened_last());
    let nick = last.map(|p| p.nick().to_owned()).unwrap_or_default();

    match (cli.mode.server, &cli.mode.client) {
        (Some(_), _) if setup.players < 2 => Err("--server: --players 2 to 4".to_owned()),
        (Some(port), _) => {
            let build = net::build(install)?;
            let builds = if cli.mode.net_any_build {
                net::Builds::Any
            } else {
                net::Builds::Same
            };
            let delay = cli
                .mode
                .net_delay
                .map_or(net::Delay::Auto, net::Delay::Fixed);
            let players = (setup.players, delay);
            let recorded = setup.recorded();
            let named = (&recorded, nick.as_str());
            let holdings = files::mod_hash::holdings(&install.mods());
            let peer = net::Peer::host(port, (&build, builds), players, named, &holdings, dumps)?;

            Ok((setup, cli.mode.local, Some(peer)))
        }
        (None, Some(host)) => {
            let build = net::build(install)?;
            let holdings = files::mod_hash::holdings(&install.mods());
            let (peer, welcome) = net::Peer::join(host, (&build, &nick), &holdings, dumps)?;

            Ok((welcome.setup, welcome.seat, Some(peer)))
        }
        (None, None) => Ok((setup, cli.mode.local, None)),
    }
}

/// The game, list or replay the options ask for; a LAN game once its
/// peers are in.
fn asked_for(cli: &Cli, install: &Install) -> Result<(), String> {
    match lists(&cli.setup, install)? {
        Lists::Printed => Ok(()),
        Lists::None => {
            let setup = Setup::try_from(&cli.setup)?;
            let (setup, local, peer) = lan(cli, setup, install)?;
            let (setup, events) = replayed(&cli.mode, setup)?;
            let offered = match &cli.mode.load {
                Some(path) => loaded_quest(path, install)?,
                None => quest(&setup, install)?,
            };

            if offered.is_none() {
                codes(&setup, &install.hd)?;
            }

            let game = game::begin::Command {
                setup,
                local,
                warm: cli.mode.warm(),
                events,
                offered,
                peer,
                saved: cli.mode.load.clone(),
            };

            play(cli, install, game)
        }
    }
}

/// The quest own save `path` plays, among those offered; `None` for a
/// custom game's.
fn loaded_quest(
    path: &std::path::Path,
    install: &Install,
) -> Result<Option<Box<game::load::Offered>>, String> {
    let key = match save::glance(path)?.meta.game {
        save::SaveKind::Quest { key, .. } => key,
        save::SaveKind::Custom { .. } | save::SaveKind::Other { .. } => return Ok(None),
    };

    Ok(quests(install)?
        .into_iter()
        .find(|o| o.quest.key.to_string() == key)
        .map(Box::new))
}

/// Every quest the base install and the mod folders offer, its archives' bytes kept for the
/// menus and games after (`regent_draw::data::archives`).
///
/// # Errors
///
/// An archive that will not read.
fn quests(install: &Install) -> Result<Vec<game::load::Offered>, String> {
    let catalogue = regent_draw::data::catalogue(&game::load::archives(&install.hd)?)?;

    Ok(game::load::quests(
        &install.hd,
        &catalogue,
        &install.mods().quests,
    ))
}

/// Run `game`, headless as `cli` asks or drawn, recording it, its events
/// played back, on the LAN with its peer. Drawn, the app opens on it
/// with the menus behind.
fn play(cli: &Cli, install: &Install, game: game::begin::Command) -> Result<(), String> {
    if cli.mode.headless {
        let game::begin::Command {
            setup,
            local,
            events,
            offered,
            peer,
            ..
        } = game;
        let plan = game::begin::Plan::of(install, &setup, offered.as_deref())?;
        let catalogue = regent_draw::data::catalogue_of(plan.sources())?;
        let begun = plan.begin(&catalogue)?;
        let needs = begun.session.needs;
        let mut sim = begun.sim;
        let lan = peer.is_some();

        if let Some(peer) = peer {
            sim = sim.networked(peer);
        }

        sim.set_local(local)?;
        let bot = cli
            .mode
            .bot
            .map(|seed| game::bot::Bot::new(Menu::of(&catalogue), seed));

        let digests = game::record::digested(&events);

        game::begin::Recording::Log.record(&mut sim, &setup.recorded().stamped(&needs), events);

        if lan {
            lan_headless(sim, cli.mode.headless_frames(), bot);
            Ok(())
        } else {
            let sim = headless(
                sim,
                cli.mode.headless_frames(),
                &Text::build(&catalogue),
                (bot, game::tune::Tunes::read(&install.hd)),
            );

            headless::checked(&sim, &digests)
        }
    } else {
        let local = game.local;

        menus(cli, install, (Some(game), local))
    }
}

/// The menus, every quest the base install offers; a quest picked is made
/// as `--quest <key>` makes it, recorded so. With `command` the app opens
/// on that game, the menus behind it; `local` this machine's player.
///
/// ```text
///  thread  quests ──> menu::Art::load ──┐
///  main    settings ──> Bevy's plugins ──┴─ join ──> Menu ──> run
/// ```
fn menus(
    cli: &Cli,
    install: &Install,
    (command, local): (Option<game::begin::Command>, u32),
) -> Result<(), String> {
    // The menus' archives and art read while Bevy starts.
    let reading = {
        let install = install.clone();
        let faces = menu::faces::seed(cli.mode.faces_seed, Capture::of(cli.mode.shot().as_ref()));

        std::thread::spawn(move || -> Result<_, String> {
            let quests = quests(&install)?;
            let art = menu::Art::load(&install.hd, &install.sdk, &quests, faces)?;

            Ok((quests, art))
        })
    };
    let shot = cli.mode.shot();
    let mut settings = match &shot {
        Some(shot) => draw::shot::settings(shot.zoom, shot.camera),
        None => files::config::load()?,
    };

    settings.enhancements.ui_upscale = cli
        .mode
        .ui_upscale
        .unwrap_or(settings.enhancements.ui_upscale);
    settings.enhancements.view_upscale = cli
        .mode
        .view_upscale
        .unwrap_or(settings.enhancements.view_upscale);
    settings.enhancements.video_upscale = cli
        .mode
        .video_upscale
        .unwrap_or(settings.enhancements.video_upscale);
    settings.enhancements.shadows = cli.mode.shadows.unwrap_or(settings.enhancements.shadows);
    settings.enhancements.cast = cli.mode.cast.unwrap_or(settings.enhancements.cast);
    settings.enhancements.sun_direction = cli
        .mode
        .sun_direction
        .unwrap_or(settings.enhancements.sun_direction);
    settings.enhancements.sun_elevation = cli
        .mode
        .sun_elevation
        .unwrap_or(settings.enhancements.sun_elevation);
    settings.enhancements.sun_size = cli.mode.sun_size.unwrap_or(settings.enhancements.sun_size);
    settings.enhancements.self_shadows = cli
        .mode
        .self_shadows
        .unwrap_or(settings.enhancements.self_shadows);
    settings.enhancements.rings = cli.mode.rings.unwrap_or(settings.enhancements.rings);
    settings.ui.font = cli.mode.font.unwrap_or(settings.ui.font);
    settings.enhancements.side_ring = cli
        .mode
        .side_ring
        .unwrap_or(settings.enhancements.side_ring);
    settings.enhancements.smooth_position = cli
        .mode
        .smooth_position
        .unwrap_or(settings.enhancements.smooth_position);
    settings.enhancements.wind |= cli.mode.wind_at.is_some();

    // Asking for an experimental key on the command line turns them on.
    settings.enhancements.experimental |=
        cli.mode.cast.is_some() || cli.mode.self_shadows.is_some() || cli.mode.wind_at.is_some();
    settings.enhancements.wind_amplitude = cli
        .mode
        .wind_amplitude
        .unwrap_or(settings.enhancements.wind_amplitude);

    let game = command.as_ref().map(|c| match &c.offered {
        Some(offered) => menu::LoadKind::Quest(offered.quest.key),
        None => menu::LoadKind::Freestyle,
    });
    let intro = settings.ui.intro;
    let opened = || {
        let (quests, art) = reading
            .join()
            .unwrap_or_else(|e| std::panic::resume_unwind(e))?;

        opened(cli, install, (quests, art), (game, intro))
    };
    let maker = game::begin::Maker::new(install.clone(), command, game::begin::Recording::Log)
        .with_local(local);

    draw::menu(
        opened,
        maker,
        (cli.mode.speed, cli.mode.message_clock),
        (settings, shot),
    )
}

/// The menus on `quests` and `art` as `cli` opens them, `game` the command
/// line's, and the player its games play as.
fn opened(
    cli: &Cli,
    install: &Install,
    (quests, art): (Vec<game::load::Offered>, menu::Art),
    (game, intro): (Option<menu::LoadKind>, files::config::Intro),
) -> Result<(menu::Opened, files::profile::Player), String> {
    let mut opened = menu::Opened::new(quests, art);

    opened
        .with(|menus| open_menus(cli, install, (game, intro), menus))
        .map(|player| (opened, player))
}

/// The menus `cli` opens, `game` the command line's, and the player its
/// games play as.
fn open_menus(
    cli: &Cli,
    install: &Install,
    (game, intro): (Option<menu::LoadKind>, files::config::Intro),
    opened: &mut menu::Flows,
) -> Result<files::profile::Player, String> {
    let shot = cli.mode.shot();

    // The intro before the menus, not before a page or game asked for,
    // nor in a screenshot.
    if intro == files::config::Intro::On
        && cli.mode.menu.is_none()
        && game.is_none()
        && shot.is_none()
    {
        opened.menu.intro();
    }

    // A screenshot reads and writes no kept freestyle config.
    let kept = shot.is_none().then(menu::freestyle::kept_place).flatten();

    if let Some(path) = kept {
        opened.freestyle.keep_at(path);
    }

    opened.mods.on(install);
    opened.extras_on(install);
    opened.saves_on(install);
    opened.lan.on(install.clone());

    // The high scores: a screenshot reads them, writes none.
    if let Some(path) = files::scores::place() {
        let writes = match shot {
            Some(_) => menu::Writes::No,
            None => menu::Writes::Yes,
        };

        opened.scores.keep(path, writes);
    }

    if let Some(page) = &cli.mode.menu {
        opened.open(page)?;
    }

    if let Some(zoom) = cli.mode.zoom {
        opened.menu.zoom(zoom);
    }

    if let Some(game) = game {
        opened.menu.command(game);
    }

    // Games play as the last used profile. With none, the main menu asks
    // for a name; a start past that menu makes one. A screenshot writes
    // nothing.
    let profile = match shot {
        Some(_) => files::profile::Profiles::default_place().and_then(|p| p.opened_last()),
        None if opened.menu.opens_main() => opened.picker.start(),
        None => opened.profile(),
    };
    let player = files::profile::Player {
        profile,
        recorded: false,
    };

    opened.played_by(&player);

    Ok(player)
}
