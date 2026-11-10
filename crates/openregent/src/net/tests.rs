//! LAN play's tests: two games in one process, over a made-up network
//! with latency, loss and reordering, and over localhost UDP.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use regent_core::sim::game::Input;
use regent_core::sim::world::cheat::{Cheat, CheatCode};
use regent_core::sim::world::defeat::End;
use regent_core::sim::world::offer::Menu;

use super::link::{Fake, Weather};
use super::node::{Node, Role};
use super::wire::{Build, Profile};
use super::*;
use crate::game::begin::Recording;
use crate::game::bot::Bot;
use crate::game::record::Event;
use crate::game::sim::Sim;
use crate::setup::Setup;
use crate::testdata::{Scratch, scratch};

/// The host's seat in a new game.
const HOST: u32 = 0;

/// The test's game: two players, the defaults else.
const ARGS: [&str; 3] = ["openregent", "--players", "2"];

/// The virtual clock's step.
const TICK: Duration = Duration::from_millis(5);

/// A frame of game time.
const FRAME: Duration = Duration::from_millis(50);

/// The game of `setup` as seat `seat` plays it, networked with `peer`,
/// recorded in `dir`; its bot.
fn game(setup: &Setup, (seat, peer): (u32, Peer), dir: &std::path::Path) -> (Sim, Bot) {
    let (sim, bot, _) = made(setup, (seat, peer), dir);

    (sim, bot)
}

/// [`game`]'s, and what the game is and its texts, as a save needs.
fn made(setup: &Setup, (seat, peer): (u32, Peer), dir: &std::path::Path) -> (Sim, Bot, Made) {
    let (begun, catalogue) = crate::game::begin::test_setup(setup);
    let mut sim = begun.sim.networked(peer);

    sim.set_local(seat).expect("a human seat");
    Recording::In(dir.to_path_buf()).record(&mut sim, setup, Vec::new());

    let bot = Bot::new(Menu::of(&catalogue), 11 + seat);

    (sim, bot, (begun.session, catalogue))
}

/// What a test's save needs beside the game: the session and the
/// catalogue its texts are read from.
type Made = (
    crate::game::sim::Session,
    regent_core::data::catalogue::Catalogue<'static>,
);

/// One peer of an in-process game: its game (and peer), bot, network
/// end, and game time owed.
struct Seat {
    sim: Sim,
    bot: Bot,
    end: Fake,
    owed: Duration,
    /// Whether it still runs: a cut-off peer stops.
    on: bool,
    made: Made,
    /// Its recording and saves' folder.
    dir: Scratch,
}

/// Two seats of the test's game over a network in `weather`.
fn pair(weather: Weather, name: &str, now: Instant) -> Vec<Seat> {
    table(2, weather, name, now)
}

/// The nicks of [`table`]'s seats.
const NICKS: [&str; 3] = ["Host", "Ann", "Bob"];

/// `players` seats of the test's game over a network in `weather`, a
/// star round the host; named from [`NICKS`] where three or fewer.
fn table(players: u16, weather: Weather, name: &str, now: Instant) -> Vec<Seat> {
    let count = players.to_string();
    let setup = crate::cli::tests::parsed(["openregent", "--players", &count]).recorded();

    table_of(players, weather, name, now, &|seat, peer, dir| {
        made(&setup, (seat, peer), dir)
    })
}

/// A seat's game made: its seat, peer and directory in.
type Maker<'a> = &'a dyn Fn(u32, Peer, &std::path::Path) -> (Sim, Bot, Made);

/// [`table`]'s, each seat's game as `make` makes it.
fn table_of(
    players: u16,
    weather: Weather,
    name: &str,
    now: Instant,
    make: Maker<'_>,
) -> Vec<Seat> {
    table_at(players, (weather, DELAY), name, now, make)
}

/// [`table_of`]'s, orders `delay` frames ahead.
fn table_at(
    players: u16,
    (weather, delay): (Weather, u32),
    name: &str,
    now: Instant,
    make: Maker<'_>,
) -> Vec<Seat> {
    table_by(
        players,
        (weather, delay, Delay::Fixed(delay)),
        name,
        now,
        make,
    )
}

/// [`table_at`]'s, the host steering the delay as `mode` says.
fn table_by(
    players: u16,
    (weather, delay, mode): (Weather, u32, Delay),
    name: &str,
    now: Instant,
    make: Maker<'_>,
) -> Vec<Seat> {
    let ends = Fake::network(weather, players, now);
    let nicks: Vec<String> = NICKS.iter().map(|n| (*n).to_owned()).collect();

    (0..u32::from(players))
        .map(|seat| {
            let end = ends[seat as usize].clone();
            // The host talks to every guest, a guest to the host.
            let (role, remotes) = if seat == HOST {
                let guests = (1..u32::from(players)).map(|g| (ends[g as usize].addr, g, None));

                (Role::Host, guests.collect())
            } else {
                (Role::Guest, vec![(ends[0].addr, HOST, None)])
            };
            let node = Node::new((role, seat), (u32::from(players), delay), remotes, now);
            let node = if seat == HOST {
                node.steered(mode)
            } else {
                node
            };
            let dir = scratch(&format!("{name}-{seat}"));
            let peer = Peer::new(node, Box::new(end.clone()), Some(dir.to_path_buf()))
                .named(nicks.clone());
            let (sim, bot, parts) = make(seat, peer, &dir);

            Seat {
                sim,
                bot,
                end,
                owed: Duration::ZERO,
                on: true,
                made: parts,
                dir,
            }
        })
        .collect()
}

/// Every seat on, at `now`: packets in and out, and the frames time owes
/// stepped where every bundle is in, the bot giving orders.
fn tick(seats: &mut [Seat], now: Instant, last: u32) {
    tick_by(seats, now, last, &mut |_, _| {});
}

/// [`tick`], `after` called with each seat's index and game after each
/// of its steps.
fn tick_by(seats: &mut [Seat], now: Instant, last: u32, after: &mut dyn FnMut(usize, &Sim)) {
    for (n, seat) in seats.iter_mut().enumerate().filter(|(_, s)| s.on) {
        seat.end.set_now(now);
        seat.sim.pump(now);
        seat.owed = (seat.owed + TICK).min(FRAME * 5);

        while seat.owed >= FRAME
            && seat.sim.frames() < last
            && seat.sim.stopped.is_none()
            && !seat.sim.peer().paused()
            && seat.sim.ready()
        {
            let moves = seat.bot.moves(&seat.sim);

            for input in moves {
                seat.sim.input(input);
            }

            seat.sim.step();
            after(n, &seat.sim);
            seat.owed -= FRAME;
        }
    }
}

/// `seats` run on the virtual clock until every one on is at `last`
/// and settled, or `most` ticks pass; the clock after.
fn run(seats: &mut [Seat], mut now: Instant, last: u32, most: u32) -> Instant {
    for _ in 0..most {
        now += TICK;
        tick(seats, now, last);

        let done = seats
            .iter()
            .filter(|s| s.on)
            .all(|s| s.sim.frames() >= last && s.sim.peer().settled());

        if done {
            break;
        }
    }

    now
}

/// The players whose orders `sim` recorded.
fn players(sim: &Sim) -> std::collections::BTreeSet<u32> {
    sim.session()
        .expect("recorded")
        .events
        .iter()
        .filter_map(|e| match e {
            Event::Input { order, .. } => Some(order.player),
            Event::PlayOn { .. } | Event::Surrender { .. } | Event::Digest { .. } => None,
        })
        .collect()
}

/// The digest lines `sim` recorded.
fn digests(sim: &Sim) -> u32 {
    let events = sim.session().expect("recorded").events;

    events
        .iter()
        .filter(|e| matches!(e, Event::Digest { .. }))
        .count()
        .try_into()
        .expect("a count")
}

/// Two games over a network that loses one packet in ten, delays each
/// 20 to 60 ms and so reorders them, each seat's bot giving orders: both
/// take every order at the same frame and play alike, in real time but
/// for the start.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn two_peers_over_a_lossy_network_play_alike() {
    const FRAMES: u32 = 1_000;

    let weather = Weather {
        latency: Duration::from_millis(20),
        jitter: Duration::from_millis(40),
        loss: 10,
    };
    let start = Instant::now();
    let mut seats = pair(weather, "net-lossy", start);

    let end = run(&mut seats, start, FRAMES, FRAMES * 40);

    let [host, guest] = &seats[..] else {
        panic!("two seats")
    };

    assert_eq!(host.sim.stopped, None);
    assert_eq!(guest.sim.stopped, None);
    assert_eq!((host.sim.frames(), guest.sim.frames()), (FRAMES, FRAMES));
    assert_eq!(host.sim.digest(), guest.sim.digest());
    assert_eq!(host.sim.deep_digest(), guest.sim.deep_digest());
    assert_eq!(
        host.sim.session().map(|s| s.events),
        guest.sim.session().map(|s| s.events),
        "the same orders at the same frames"
    );
    assert_eq!(players(&host.sim), [0, 1].into(), "both seats' orders");
    assert_eq!(
        digests(&host.sim),
        FRAMES / crate::game::record::DIGEST_EVERY,
        "the same digest lines in each recording"
    );
    assert!(
        end.duration_since(start) < FRAME * (FRAMES + FRAMES / 50),
        "real time kept, about: {:?}",
        end.duration_since(start)
    );
}

/// A guest cut off mid-game: the host holds for it, a second short of
/// the window waiting on it still, then drops it and plays on alone,
/// the guest's kingdom uncontrolled.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_silent_peer_is_dropped_and_the_other_plays_on() {
    const CUT: u32 = 200;
    const FRAMES: u32 = 400;
    const SHORT: Duration = Duration::from_secs(1);

    let weather = Weather {
        latency: Duration::from_millis(5),
        jitter: Duration::ZERO,
        loss: 0,
    };
    let start = Instant::now();
    let mut seats = pair(weather, "net-drop", start);
    let mut now = run(&mut seats, start, CUT, CUT * 40);

    seats[1].end.cut();
    seats[1].on = false;
    now = run(
        &mut seats,
        now,
        FRAMES,
        ticks(node::DROP_AFTER.saturating_sub(SHORT)),
    );

    let host = &seats[0];
    let waits = host.sim.peer().waiting();

    assert!(!host.sim.peer().dropped(), "within the window");
    assert!(
        host.sim.frames() < CUT + HELD,
        "held: {}",
        host.sim.frames()
    );
    assert_eq!(waits.iter().map(|w| w.seat).collect::<Vec<_>>(), [1]);
    assert!(waits[0].left <= SHORT, "{:?}", waits[0].left);

    run(&mut seats, now, FRAMES, FRAMES * 40);

    let host = &seats[0];

    assert!(host.sim.peer().dropped());
    assert_eq!(host.sim.stopped, None);
    assert_eq!(host.sim.frames(), FRAMES);
    assert_eq!(host.sim.peer().waiting(), []);
}

/// Ann surrenders: on both peers her game ends lost, she watches, her
/// orders refused, her loss not yet shown, and her chat marked for
/// spectators. One game, one recording, which replays. The host
/// surrenders too: the game is over, each shown the loss.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_surrender_spectates_until_every_player_is_out() {
    const GIVEN: u32 = 100;
    const FRAMES: u32 = 300;
    const ANN: u32 = 1;

    let start = Instant::now();
    let mut seats = pair(CALM, "net-surrender", start);
    let mut now = run(&mut seats, start, GIVEN, GIVEN * 40);

    seats[1].sim.surrender();
    seats[1].sim.chat("gg");
    now = run(&mut seats, now, FRAMES, FRAMES * 40);

    one_game(&seats, FRAMES, 0);
    assert!(
        seats
            .iter()
            .all(|s| s.sim.world().ended(ANN) == Some(End::Lost))
    );
    assert!(seats[1].sim.spectating());
    assert!(!seats[0].sim.spectating());
    assert!(seats.iter().all(|s| !s.sim.over() && !s.sim.ended()));
    assert_eq!(seats[1].sim.end(), None, "held while the host plays");
    let later = seats[0]
        .sim
        .session()
        .expect("recorded")
        .events
        .into_iter()
        .filter(|e| matches!(e, Event::Input { before, order } if order.player == ANN && *before > GIVEN + HELD))
        .count();

    assert_eq!(later, 0, "no order of Ann's after her surrender");

    assert_eq!(
        seats[0].sim.peer_mut().take_chat(),
        [(ANN, "gg".to_owned(), Audience::Spectators)]
    );

    seats[0].sim.surrender();
    run(&mut seats, now, FRAMES * 2, FRAMES);

    for seat in &seats {
        assert!(seat.sim.over());
        assert_eq!(seat.sim.end(), Some(End::Lost));
        assert!(seat.sim.ended());
        assert_eq!(seat.sim.stopped, None);
    }
}

/// A dropped player counts as surrendered: the host, left alone,
/// surrenders and the game is over.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_dropped_player_counts_as_surrendered() {
    const CUT: u32 = 100;
    const FRAMES: u32 = 300;

    let start = Instant::now();
    let mut seats = pair(CALM, "net-surrender-drop", start);
    let mut now = run(&mut seats, start, CUT, CUT * 40);

    seats[1].end.cut();
    seats[1].on = false;
    now = run(
        &mut seats,
        now,
        FRAMES,
        FRAMES * 40 + ticks(node::DROP_AFTER),
    );

    let host = &mut seats[0].sim;

    assert!(host.peer().dropped());
    assert!(!host.over(), "the host plays");

    host.surrender();
    run(&mut seats[..1], now, FRAMES * 2, FRAMES);

    let host = &seats[0].sim;

    assert!(host.over());
    assert_eq!(host.end(), Some(End::Lost));
}

/// Frames a peer may run past the last its peers' bundles reached
/// before it holds: a tick's frames owed and the delay, with room.
const HELD: u32 = 20;

/// Ticks of [`TICK`] in `span`.
fn ticks(span: Duration) -> u32 {
    u32::try_from(span.as_millis() / TICK.as_millis()).expect("ticks")
}

/// Each seat's waits: seat and nick.
fn waits(seats: &[Seat]) -> Vec<Vec<(u32, String)>> {
    seats
        .iter()
        .map(|s| {
            let waits = s.sim.peer().waiting().into_iter();

            waits.map(|w| (w.seat, w.nick)).collect()
        })
        .collect()
}

/// A guest's link lost 3 s, its game running: the host holds and shows
/// whom it waits on, as the guest shows its host. The link back, both
/// play on in lockstep: one game, none dropped, nothing told.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_guest_silent_3_s_comes_back_and_play_resumes_in_lockstep() {
    const CUT: u32 = 100;
    const FRAMES: u32 = 400;
    const SILENT: Duration = Duration::from_secs(3);

    let start = Instant::now();
    let mut seats = pair(CALM, "net-back", start);
    let mut now = run(&mut seats, start, CUT, CUT * 40);

    seats[1].end.cut();
    now = run(&mut seats, now, FRAMES, ticks(SILENT));

    assert!(node::WAIT_AFTER < SILENT && SILENT < node::DROP_AFTER);
    assert_eq!(
        waits(&seats),
        [vec![(1, "Ann".to_owned())], vec![(HOST, "Host".to_owned())]]
    );
    assert!(seats.iter().all(|s| s.sim.frames() < CUT + HELD), "held");

    seats[1].end.mend();
    run(&mut seats, now, FRAMES, FRAMES * 40);

    one_game(&seats, FRAMES, 0);
    assert_eq!(waits(&seats), [vec![], vec![]]);
    assert_eq!(gone(&mut seats), [vec![], vec![]]);
}

/// A guest silent past the window is dropped; a machine named as it asks
/// the host from another address and takes its seat back. Every peer
/// goes on from the host's game, the host's chat says so, and both play
/// on as one game, one recording, which played back alone is that game.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_dropped_guest_rejoins_and_both_play_one_game() {
    a_dropped_guest_rejoins(Address::Elsewhere, "net-rejoin");
}

/// The same from its old address, as Return to Lobby asks.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_dropped_guest_rejoins_from_its_old_address() {
    a_dropped_guest_rejoins(Address::Same, "net-rejoin-same");
}

/// Where a rejoining guest asks from.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Address {
    Same,
    Elsewhere,
}

fn a_dropped_guest_rejoins(from: Address, name: &str) {
    const CUT: u32 = 100;
    const BACK: u32 = 300;
    const FRAMES: u32 = 600;
    const PORT: u16 = 1_100;

    let start = Instant::now();
    let mut seats = pair(CALM, name, start);

    seats[1].sim.chat("before");

    let mut now = run(&mut seats, start, CUT, CUT * 40);

    seats[1].end.cut();
    seats[1].on = false;
    now = run(&mut seats, now, BACK, BACK * 40 + ticks(node::DROP_AFTER));

    assert!(seats[0].sim.peer().dropped());
    assert_eq!(
        gone(&mut seats[..1]),
        [left("Ann", Leave::Silent, Who::Player)]
    );
    assert_eq!(
        seats[0].sim.peer_mut().take_chat(),
        [(1, "before".to_owned(), Audience::All)]
    );

    // Ann's machine anew: its game made from the setup, as a welcome
    // makes it, and its hello sent. The old game gone first: its
    // goodbye comes before the hello.
    let host = seats[0].end.addr;
    let end = match from {
        Address::Same => seats[1].end.clone(),
        Address::Elsewhere => seats[0].end.end(PORT),
    };

    seats.pop();
    end.mend();

    let node = Node::new((Role::Guest, 1), (2, DELAY), vec![(host, HOST, None)], now);
    let dir = scratch(&format!("{name}-back"));
    let nicks = NICKS.iter().map(|n| (*n).to_owned()).collect();
    let peer = Peer::new(node, Box::new(end.clone()), Some(dir.to_path_buf())).named(nicks);
    let setup = crate::cli::tests::parsed(ARGS).recorded();
    let (sim, bot, parts) = made(&setup, (1, peer), &dir);
    let hello = Packet::Hello(wire::Hello {
        build: lobby_build(),
        nick: "Ann".to_owned(),
        ready: Ready::Always,
        heard: 0,
        said: Lines::default(),
        holding: wire::Holding::None,
    });

    end.set_now(now);
    end.send(host, &hello.encode());
    seats.push(Seat {
        sim,
        bot,
        end,
        owed: Duration::ZERO,
        on: true,
        made: parts,
        dir,
    });

    // Its lines numbered from 1 again: each comes.
    seats[1].sim.chat("back");
    run(&mut seats, now, FRAMES, FRAMES * 40);

    one_game(&seats, FRAMES, 1);
    assert_eq!(
        seats[0].sim.peer_mut().take_chat(),
        [(1, "back".to_owned(), Audience::All)]
    );
    assert_eq!(
        notices(&mut seats),
        [vec![Notice::Rejoined("Ann".to_owned())], vec![]]
    );
    assert_eq!(players(&seats[0].sim), [0, 1].into(), "both seats' orders");
}

/// Three players; Bob's link lost. Once he is waited on, the host and
/// Ann both show him, Ann as the host reports; the host drops him at
/// once, long before the window, and both play on, each chat saying so
/// once.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn the_hosts_drop_takes_a_waited_on_guest_out_at_once() {
    const CUT: u32 = 100;
    const FRAMES: u32 = 300;
    const WAITED: Duration = Duration::from_secs(3);

    let start = Instant::now();
    let mut seats = table(3, CALM, "net-toss", start);
    let mut now = run(&mut seats, start, CUT, CUT * 40);

    seats[2].end.cut();
    seats[2].on = false;
    now = run(&mut seats, now, FRAMES, ticks(WAITED));

    let bob = vec![(2, "Bob".to_owned())];

    assert_eq!(waits(&seats[..2]), [bob.clone(), bob]);

    seats[0].sim.peer_mut().drop_player(2);

    let on = &mut seats[..2];
    let told = run(on, now, FRAMES, FRAMES * 40);

    assert!(
        told.duration_since(start) < node::DROP_AFTER,
        "before the window"
    );
    assert!(on.iter().all(|s| s.sim.stopped.is_none()));
    assert!(on.iter().all(|s| s.sim.frames() == FRAMES));
    assert_eq!(
        gone(on),
        [
            left("Bob", Leave::Silent, Who::Player),
            left("Bob", Leave::Silent, Who::Player)
        ]
    );
    assert_eq!(waits(on), [vec![], vec![]]);
}

/// A LAN at 5 ms, nothing lost.
const CALM: Weather = Weather {
    latency: Duration::from_millis(5),
    jitter: Duration::ZERO,
    loss: 0,
};

/// Each seat's players gone, taken: what its chat would say.
fn gone(seats: &mut [Seat]) -> Vec<Vec<Left>> {
    seats
        .iter_mut()
        .map(|s| s.sim.peer_mut().take_left())
        .collect()
}

/// `nick` gone `how`, a guest's seat or the host's.
fn left(nick: &str, how: Leave, who: Who) -> Vec<Left> {
    vec![Left {
        nick: nick.to_owned(),
        how,
        who,
    }]
}

/// Three players; a guest quits (its game dropped, as QUIT and the
/// menus drop it). Its goodbye tells the other two at once, long before
/// the silence would, each once; both play on.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_guest_that_quits_is_told_to_the_others_at_once() {
    const QUIT: u32 = 100;
    const TOLD: u32 = QUIT + 20;
    const FRAMES: u32 = 300;

    let start = Instant::now();
    let mut seats = table(3, CALM, "net-quit", start);
    let now = run(&mut seats, start, QUIT, QUIT * 40);

    drop(seats.pop());

    let told = run(&mut seats, now, TOLD, TOLD * 40);

    assert!(
        told.duration_since(now) < node::DROP_AFTER,
        "before the silence"
    );
    assert_eq!(
        gone(&mut seats),
        [
            left("Bob", Leave::Quit, Who::Player),
            left("Bob", Leave::Quit, Who::Player)
        ]
    );

    run(&mut seats, told, FRAMES, FRAMES * 40);

    assert_eq!(gone(&mut seats), [vec![], vec![]], "told once");
    assert!(seats.iter().all(|s| s.sim.stopped.is_none()));
    assert!(seats.iter().all(|s| s.sim.frames() == FRAMES));
}

/// Three players; a guest falls silent. Once dropped, the other two are
/// told it was, each once.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_silent_guest_is_told_to_the_others_once() {
    const CUT: u32 = 100;
    const FRAMES: u32 = 300;

    let start = Instant::now();
    let mut seats = table(3, CALM, "net-silent", start);
    let now = run(&mut seats, start, CUT, CUT * 40);

    seats[2].end.cut();
    seats[2].on = false;
    run(
        &mut seats,
        now,
        FRAMES,
        FRAMES * 40 + ticks(node::DROP_AFTER),
    );

    let on = &mut seats[..2];

    assert!(on.iter().all(|s| s.sim.frames() == FRAMES));
    assert_eq!(
        gone(on),
        [
            left("Bob", Leave::Silent, Who::Player),
            left("Bob", Leave::Silent, Who::Player)
        ]
    );
    assert_eq!(gone(on), [vec![], vec![]]);
}

/// Three players; the host quits. Each guest is told once, the host's
/// seat alone, and plays on alone.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_host_that_quits_is_told_to_each_guest_once() {
    const QUIT: u32 = 100;
    const FRAMES: u32 = 200;

    let start = Instant::now();
    let mut seats = table(3, CALM, "net-host-quit", start);
    let now = run(&mut seats, start, QUIT, QUIT * 40);

    drop(seats.remove(0));
    run(&mut seats, now, FRAMES, FRAMES * 40);

    assert!(seats.iter().all(|s| s.sim.frames() == FRAMES));
    assert_eq!(
        gone(&mut seats),
        [
            left("Host", Leave::Quit, Who::Host),
            left("Host", Leave::Quit, Who::Host)
        ]
    );
    assert_eq!(gone(&mut seats), [vec![], vec![]]);
}

/// Three players; a guest's game stops (a resync's file that will not
/// load, its VM's error), its peer still heard. It leaves as one that
/// quits: the others drop it at once and play on.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_guest_whose_game_stops_leaves_and_the_others_play_on() {
    const STOP: u32 = 100;
    const FRAMES: u32 = 300;

    let start = Instant::now();
    let mut seats = table(3, CALM, "net-stopped", start);
    let now = run(&mut seats, start, STOP, STOP * 40);

    seats[2].sim.stopped = Some("a test's stop".to_owned());

    let told = run(&mut seats, now, FRAMES, FRAMES * 20);
    let on = &mut seats[..2];

    assert!(
        told.duration_since(now) < node::DROP_AFTER,
        "before the silence"
    );
    assert!(on.iter().all(|s| s.sim.frames() == FRAMES));
    assert_eq!(
        gone(on),
        [
            left("Bob", Leave::Quit, Who::Player),
            left("Bob", Leave::Quit, Who::Player)
        ]
    );
}

/// A guest that never sends a deep digest (its worker did not start):
/// the snapshot held to compare with it is let go once too old to be.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_snapshot_none_compares_is_let_go_in_time() {
    let now = Instant::now();
    let ends = Fake::network(CALM, 2, now);
    let node = Node::new(
        (Role::Host, 0),
        (2, DELAY),
        vec![(ends[1].addr, 1, None)],
        now,
    );
    let mut peer = Peer::new(node, Box::new(ends[0].clone()), None);
    let snapshot = crate::game::begin::test_game(&[]).0.sim.snapshot();

    peer.held.insert(0, snapshot.expect("a snapshot"));
    peer.steps = node::KEPT_DIGESTS + DEEP_EVERY;
    peer.release();

    assert!(peer.held.is_empty());
}

/// Nobody leaves: nothing is told, and alone never.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn no_one_is_told_gone_while_all_play() {
    const FRAMES: u32 = 200;

    let start = Instant::now();
    let mut seats = table(3, CALM, "net-stay", start);

    run(&mut seats, start, FRAMES, FRAMES * 40);

    assert_eq!(gone(&mut seats), [vec![], vec![], vec![]]);
    assert_eq!(Peer::solo().take_left(), []);
}

/// A test's desync: seat `seat`'s game alone given gold before its next
/// frame, as no other peer's is. A guest's is gone with the resync; the
/// host's is the truth, recorded, so every recording replays it.
fn corrupt(seats: &mut [Seat], seat: usize) {
    let sim = &mut seats[seat].sim;
    let player = u32::try_from(seat).expect("a seat");
    let cheat = Input::Cheat(CheatCode {
        cheat: Cheat::Gold,
        arg: player,
    });

    sim.deliver(Event::Input {
        before: sim.frames() + 1,
        order: regent_core::sim::game::Order {
            player,
            input: cheat,
        },
    });
}

/// Each seat's resync notices, taken.
fn notices(seats: &mut [Seat]) -> Vec<Vec<Notice>> {
    seats
        .iter_mut()
        .map(|s| s.sim.peer_mut().take_notices())
        .collect()
}

/// `sim`'s recording played back alone, as `--replay` plays it, to
/// frame `frames`.
fn replayed(sim: &Sim, frames: u32) -> Sim {
    let session = sim.session().expect("recorded");
    let mut alone = crate::game::begin::test_setup(&session.setup).0.sim;

    alone.replay(session.events);

    while alone.frames() < frames && alone.stopped.is_none() {
        alone.step();
    }

    alone
}

/// Every seat on, in play: one game, one recording, no seat dropped,
/// `epoch` resyncs gone through; and the recording, played back alone,
/// reaches that game.
fn one_game(seats: &[Seat], frames: u32, epoch: u32) {
    let host = &seats[0].sim;
    let events = host.session().map(|s| s.events);

    for seat in seats {
        let sim = &seat.sim;

        assert_eq!(sim.stopped, None);
        assert_eq!(sim.frames(), frames);
        assert_eq!(sim.peer().epoch(), epoch, "no desync since");
        assert!(!sim.peer().dropped(), "none dropped");
        assert_eq!(sim.digest(), host.digest());
        assert_eq!(sim.deep_digest(), host.deep_digest());
        assert_eq!(sim.session().map(|s| s.events), events, "one recording");
    }

    let alone = replayed(host, frames);

    assert_eq!(alone.stopped, None, "every digest line matches");
    assert_eq!(alone.digest(), host.digest());
    assert_eq!(alone.deep_digest(), host.deep_digest());
}

/// Seat `parted`'s game alone given gold at frame 100: the digests
/// part, the host sends its game, and every peer goes on from it, one
/// game for 400 frames on, each chat saying so; the host, which finds
/// every desync, wrote its snapshot, every seat the host's file.
fn a_desync_is_resynced(parted: usize, name: &str) {
    const PARTED: u32 = 100;
    const FRAMES: u32 = 600;

    let start = Instant::now();
    let mut seats = pair(CALM, name, start);
    let now = run(&mut seats, start, PARTED, PARTED * 40);

    corrupt(&mut seats, parted);
    run(&mut seats, now, FRAMES, FRAMES * 40);
    one_game(&seats, FRAMES, 1);

    let said = notices(&mut seats);
    let Some(Notice::Resyncing(frame)) = said[0].first().cloned() else {
        panic!("{said:?}")
    };

    assert!((PARTED..PARTED + 20).contains(&frame), "{frame}");
    assert_eq!(
        said,
        [0, 1].map(|_| vec![Notice::Resyncing(frame), Notice::Resynced])
    );

    for (seat, at) in seats.iter().enumerate() {
        let files: Vec<String> = std::fs::read_dir(&*at.dir)
            .expect("the seat's directory")
            .filter_map(|e| e.ok()?.file_name().into_string().ok())
            .collect();
        let has = |lead: &str| files.iter().any(|f| f.starts_with(lead));

        assert!(has(&format!("resync-player{seat}-frame")), "{files:?}");
        assert!(seat != 0 || has("desync-player0-frame"), "{files:?}");
    }
}

#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_guest_that_parts_is_resynced_from_the_host() {
    a_desync_is_resynced(1, "net-resync-guest");
}

/// The host's game is the truth even where it is the one that parted.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_host_that_parts_resyncs_the_guest_to_its_own_game() {
    a_desync_is_resynced(0, "net-resync-host");
}

/// Three peers over a network that loses one packet in ten and delays
/// each 20 to 60 ms; the second guest parts. Every guest is sent the
/// host's game, the one that agreed too; the first guest stalls 8 s
/// while it comes, longer than a silence holds the game in play. None
/// is dropped, and all go on as one game.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn three_peers_resync_over_a_lossy_network_and_none_is_dropped() {
    const PARTED: u32 = 100;
    const FRAMES: u32 = 500;
    const STALL: Duration = Duration::from_secs(8);

    let weather = Weather {
        latency: Duration::from_millis(20),
        jitter: Duration::from_millis(40),
        loss: 10,
    };
    let start = Instant::now();
    let mut seats = table(3, weather, "net-resync-three", start);
    let mut now = run(&mut seats, start, PARTED, PARTED * 40);

    corrupt(&mut seats, 2);

    while !seats[1].sim.peer().taking() {
        now += TICK;
        tick(&mut seats, now, FRAMES);
        assert!(now.duration_since(start) < FRAME * FRAMES * 2, "no resync");
    }

    assert!(STALL > node::WAIT_AFTER);
    seats[1].on = false;
    let ticks = STALL.as_millis() / TICK.as_millis();

    now = run(&mut seats, now, FRAMES, ticks.try_into().expect("ticks"));
    seats[1].on = true;
    run(&mut seats, now, FRAMES, FRAMES * 40);

    one_game(&seats, FRAMES, 1);
}

/// A desync again within [`RESYNC_GAP`] frames of a resync would come
/// back forever: every peer stops, saying why.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_desync_soon_after_a_resync_stops_every_peer() {
    const FIRST: u32 = 100;
    const AGAIN: u32 = 300;
    const FRAMES: u32 = 600;

    let start = Instant::now();
    let mut seats = pair(CALM, "net-resync-again", start);
    let now = run(&mut seats, start, FIRST, FIRST * 40);

    corrupt(&mut seats, 1);

    let now = run(&mut seats, now, AGAIN, AGAIN * 40);

    assert!(seats.iter().all(|s| s.sim.peer().epoch() == 1), "resynced");
    corrupt(&mut seats, 1);
    run(&mut seats, now, FRAMES, FRAMES * 40);

    let said = notices(&mut seats);

    for (seat, said) in seats.iter().zip(said) {
        let why = seat.sim.stopped.clone().unwrap_or_default();

        assert!(why.contains("desync again at frame"), "{why}");
        assert!(seat.sim.frames() < FRAMES);
        assert_eq!(said.last(), Some(&Notice::Stopped(why)));
    }
}

/// One game given a flag the quick digest does not cover parts at the
/// next deep digest, folded on each worker: the host writes the snapshot
/// of that frame, not of the frame it found it at, and resyncs.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_deep_digest_folded_aside_that_parts_is_resynced() {
    const GIVEN: u32 = 150;
    const PARTED: u32 = 200;
    const FRAMES: u32 = 500;
    // Longest the folds may take, wall time.
    const FOLDING: Duration = Duration::from_secs(30);

    let start = Instant::now();
    let mut seats = pair(CALM, "net-deep", start);
    let now = run(&mut seats, start, GIVEN, GIVEN * 40);
    let guest = &mut seats[1].sim;
    // The build menu's cheat, on the host's player: no order or digest
    // of the guest's own shows it.
    let cheat = Input::Cheat(CheatCode {
        cheat: Cheat::Build,
        arg: 0,
    });

    guest.deliver(Event::Input {
        before: guest.frames() + 1,
        order: regent_core::sim::game::Order {
            player: 0,
            input: cheat,
        },
    });

    let mut now = run(&mut seats, now, PARTED * 2, PARTED * 40);
    let mut last = PARTED * 2;
    let began = Instant::now();

    // The folds run on workers, in real time, and a busy machine may
    // hold them past the frames above: on a frame at a time, a moment
    // apart, until both resynced.
    while seats.iter().any(|s| s.sim.peer().epoch() == 0) && began.elapsed() < FOLDING {
        std::thread::sleep(Duration::from_millis(1));
        last += 1;
        now = run(&mut seats, now, last, 40);
    }

    run(&mut seats, now, FRAMES, FRAMES * 40);
    one_game(&seats, FRAMES, 1);

    let dump = seats[0]
        .dir
        .join(format!("desync-player0-frame{PARTED}.ron"));
    let text = std::fs::read_to_string(&dump).expect("the snapshot written");
    let snapshot: regent_core::save::Snapshot = ron::from_str(&text).expect("a snapshot");

    assert_eq!(snapshot.game.frames, PARTED);
}

/// Where [`written`] puts a seat's save in its folder `dir`.
fn saved_at(dir: &std::path::Path) -> PathBuf {
    dir.join(format!("lan{}", crate::save::EXTENSION))
}

/// Each seat's LAN saves taken so far, written to its scratch directory
/// as a save of its profile is: the file read back, by seat.
fn written(seats: &mut [Seat]) -> Vec<crate::save::Contents> {
    seats
        .iter_mut()
        .map(|seat| {
            let stepped = seat.sim.take_saves();
            let [stepped] = <[_; 1]>::try_from(stepped).ok().expect("one save each");
            let (session, catalogue) = &seat.made;
            let text = regent_core::data::text::Text::build(catalogue);
            let interface = crate::save::Interface {
                selected: None,
                reading: None,
                pan: (0.0, 0.0),
                zoom: 1.0,
                tracking: regent_core::view::panel::tracking::Tracker::default(),
            };
            let contents =
                crate::save::of((stepped.game, session), &text, None, interface).expect("contents");
            let path = saved_at(&seat.dir);

            assert_eq!((stepped.seat, stepped.name.as_str()), (1, "Day"));
            crate::save::write(&path, &contents).expect("written");
            crate::save::read(&path).expect("read back")
        })
        .collect()
}

/// A guest's SAVE GAME, asked at frame 100 and ordered through the
/// lockstep: every peer saves the game as one step leaves it, a delay
/// on, and the saves' games are one game, deep digest for deep digest,
/// `play.input` for `play.input`; each names the seats. Each seat's
/// file.
fn every_peer_saves(players: u16, name: &str) -> (Vec<Scratch>, Vec<PathBuf>) {
    const ASKED: u32 = 100;
    const FRAMES: u32 = 200;

    let weather = Weather {
        latency: Duration::from_millis(10),
        jitter: Duration::from_millis(10),
        loss: 5,
    };
    let start = Instant::now();
    let mut seats = table(players, weather, name, start);
    let now = run(&mut seats, start, ASKED, ASKED * 40);

    seats[1].sim.ask_save("Day");
    run(&mut seats, now, FRAMES, FRAMES * 40);

    let saves = written(&mut seats);
    let first = &saves[0];
    let lan = first.meta.lan.clone().expect("a LAN save");
    let deep = regent_core::sim::game::deep_fold(&first.snapshot.game);

    assert!(seats.iter().all(|s| s.sim.stopped.is_none()));
    assert!(
        (ASKED + 1..=ASKED + DELAY).contains(&first.meta.frame),
        "the step a delay on: {}",
        first.meta.frame
    );
    assert_eq!(lan.seats.len(), players as usize);
    assert_eq!(lan.delay, DELAY);

    for save in &saves[1..] {
        assert_eq!(save.meta.frame, first.meta.frame);
        assert_eq!(save.meta.lan.as_ref(), Some(&lan));
        assert_eq!(save.play, first.play);
        assert_eq!(regent_core::sim::game::deep_fold(&save.snapshot.game), deep);
    }

    let paths = seats.iter().map(|s| saved_at(&s.dir)).collect();

    (seats.into_iter().map(|s| s.dir).collect(), paths)
}

#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn two_peers_save_one_game_at_one_frame() {
    every_peer_saves(2, "net-save-2");
}

#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn three_peers_save_one_game_at_one_frame() {
    every_peer_saves(3, "net-save-3");
}

/// Autosaving every day, each of three peers takes its autosaves alone,
/// unasked, as frame 1 and frame 1 past day 1 have run (the original's
/// frames, `view::menu::autosave::due`): one game on every peer, by its LAN
/// hash. A peer autosaving every second day takes frame 1's alone.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn every_peer_autosaves_one_game_at_the_day() {
    const DAY: u32 =
        regent_core::view::panel::day::DAY_MS / regent_core::gpl::runtime::MS_PER_FRAME;
    const FRAMES: u32 = DAY + 10;

    let weather = Weather {
        latency: Duration::from_millis(5),
        jitter: Duration::from_millis(5),
        loss: 2,
    };
    let start = Instant::now();
    let mut seats = table(3, weather, "net-autosave", start);

    seats[0].sim.set_autosave(Some(1));
    seats[1].sim.set_autosave(Some(1));
    seats[2].sim.set_autosave(Some(2));
    run(&mut seats, start, FRAMES, FRAMES * 40);

    let taken: Vec<Vec<crate::save::auto::Taken>> =
        seats.iter_mut().map(|s| s.sim.take_autosaves()).collect();
    let days: Vec<Vec<u32>> = taken
        .iter()
        .map(|t| t.iter().map(|t| t.day).collect())
        .collect();

    assert!(seats.iter().all(|s| s.sim.stopped.is_none()));
    assert_eq!(days, [vec![0, 1], vec![0, 1], vec![0]]);

    // Each of the first two peers' day 1 autosave, made.
    let saved: Vec<crate::save::Contents> = taken
        .into_iter()
        .zip(&seats)
        .take(2)
        .map(|(t, seat)| {
            let (session, catalogue) = &seat.made;
            let text = regent_core::data::text::Text::build(catalogue);
            let described = crate::save::described(session, &text);
            let interface = crate::save::Interface {
                selected: None,
                reading: None,
                pan: (0.0, 0.0),
                zoom: 1.0,
                tracking: regent_core::view::panel::tracking::Tracker::default(),
            };
            let day = t.into_iter().last().expect("day 1's");

            crate::save::made(day.game, described, Vec::new(), interface).expect("contents")
        })
        .collect();

    assert_eq!(saved[0].meta.frame, DAY + 1);
    assert_eq!(saved[0].meta.frame, saved[1].meta.frame);
    assert_eq!(saved[0].meta.lan, saved[1].meta.lan);
    assert_eq!(saved[0].play, saved[1].play);
}

/// The save at `path` loaded as the menus load it, alone; its session.
fn resumed(path: &std::path::Path) -> (Sim, Made, crate::game::record::Session) {
    let install = crate::files::env::Install::from_env().expect("MAJESTY_HD");
    let (plan, session, _) = crate::game::begin::Plan::resumed(&install, path, &[]).expect("loads");
    let catalogue = regent_draw::data::catalogue_of(plan.sources()).expect("catalogue");
    let begun = plan.begin(&catalogue).expect("begun");

    (begun.sim, (begun.session, catalogue), session)
}

/// Each peer's own copy of a LAN save loaded, every seat in its saved
/// seat: they play on in lockstep, bots giving orders, to one game and
/// one recording, which goes on from the save's; the save loaded alone
/// and its recording since played back reaches that game too.
fn a_loaded_save_plays_on(players: u16, name: &str) {
    const MORE: u32 = 300;

    let from = format!("{name}-from");
    let (_saved, paths) = every_peer_saves(players, &from);
    let weather = Weather {
        latency: Duration::from_millis(10),
        jitter: Duration::from_millis(10),
        loss: 5,
    };
    let start = Instant::now();
    let mut seats = table_of(players, weather, name, start, &|seat, peer, dir| {
        let (sim, made, session) = resumed(&paths[seat as usize]);
        let mut sim = sim.networked(peer);
        let bot = Bot::new(Menu::of(&made.1), 21 + seat);

        sim.set_local(seat).expect("a human seat");
        sim.record(crate::game::record::Recorder::in_dir(dir, &session));
        (sim, bot, made)
    });
    let at = seats[0].sim.frames();

    run(&mut seats, start, at + MORE, MORE * 40);

    let host = &seats[0].sim;
    let saved = crate::game::record::parse(&crate::save::read(&paths[0]).expect("read").play)
        .expect("its session");
    let events = host.session().expect("recorded").events;

    for seat in &seats {
        assert_eq!(seat.sim.stopped, None);
        assert_eq!(seat.sim.frames(), at + MORE);
        assert_eq!(seat.sim.digest(), host.digest());
        assert_eq!(seat.sim.deep_digest(), host.deep_digest());
        assert_eq!(seat.sim.session().map(|s| s.events), Some(events.clone()));
    }

    assert_eq!(
        events[..saved.events.len()],
        saved.events[..],
        "on from the save's"
    );
    assert!(
        !players_of(&events[saved.events.len()..]).is_empty(),
        "orders since"
    );

    // Alone: the save loaded, the recording since played back.
    let (mut alone, _, _) = resumed(&paths[0]);

    alone.replay(events[saved.events.len()..].to_vec());

    while alone.frames() < at + MORE && alone.stopped.is_none() {
        alone.step();
    }

    assert_eq!(alone.stopped, None);
    assert_eq!(alone.digest(), host.digest());
    assert_eq!(alone.deep_digest(), host.deep_digest());
}

/// The players whose orders are among `events`.
fn players_of(events: &[Event]) -> std::collections::BTreeSet<u32> {
    events
        .iter()
        .filter_map(|e| match e {
            Event::Input { order, .. } => Some(order.player),
            Event::PlayOn { .. } | Event::Surrender { .. } | Event::Digest { .. } => None,
        })
        .collect()
}

#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_loaded_lan_save_of_two_plays_on_alike_and_as_alone() {
    a_loaded_save_plays_on(2, "net-load-2");
}

#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_loaded_lan_save_of_three_plays_on_alike_and_as_alone() {
    a_loaded_save_plays_on(3, "net-load-3");
}

/// The host loads a LAN save of three in a polled lobby over localhost:
/// a guest with its own copy holds it, one without is sent the host's,
/// window by window, into its saves; the game is startable only once
/// both have it; each sits in the saved seat its nick had, the rest
/// filling, and the welcomes name the game and seats.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_lobby_sends_the_save_to_a_guest_that_lacks_it_and_seats_by_nick() {
    let (_saved, paths) = every_peer_saves(3, "net-lobby-load-from");
    let build = lobby_build();
    let mut hosting = Lobby::bind(0)
        .expect("bound")
        .host(
            (&build, Builds::Same),
            (3, Delay::Fixed(DELAY)),
            (&Setup::default(), "Host"),
        )
        .expect("hosting");
    let host = std::net::SocketAddr::from(([127, 0, 0, 1], hosting.port()));
    let now = Instant::now();
    let (bob_dir, cat_dir) = (scratch("net-lobby-load-bob"), scratch("net-lobby-load-cat"));
    let own = bob_dir.join(format!("Mine{}", crate::save::EXTENSION));

    std::fs::copy(&paths[2], &own).expect("Bob's own copy");

    let join = |nick, dir: &Scratch| {
        let mut joining = Joining::new(host, (&build, nick), now).expect("asking");

        joining.set_saves(Some(Arc::new(crate::save::lan::Shelf::new(
            dir.to_path_buf(),
        ))));
        joining.set_ready(Ready::Always);
        joining
    };
    // Bob's nick is the saved seat 2's; Cat's none.
    let (mut bob, mut cat) = (join("Bob", &bob_dir), join("Cat", &cat_dir));
    let mut step = |hosting: &mut Hosting| {
        for guest in [&mut bob, &mut cat] {
            assert_eq!(guest.poll(Instant::now()), Ok(None));
        }

        hosting.poll(Instant::now());
    };

    until("seated", || {
        step(&mut hosting);
        hosting.unready().is_none()
    });

    choose(&mut hosting, &paths[0]).expect("loads");

    assert_eq!(hosting.unready(), Some(Unready::Save));
    until("sent", || {
        step(&mut hosting);
        hosting.unready().is_none()
    });

    let game = hosting.view().save.expect("offered").game;
    let came = cat.save(&game).expect("Cat's copy").to_path_buf();

    assert_eq!(bob.save(&game), Some(own.as_path()));
    assert_eq!(came.parent(), Some(cat_dir.path()));
    assert_eq!(
        crate::save::read(&came).expect("reads").play,
        crate::save::read(&paths[0]).expect("reads").play
    );
    assert_eq!(hosting.view().save.map(|o| o.sits), Some(vec![0, 2, 1]));

    let (peer, save) = Peer::seated(hosting, None).expect("peer");
    let welcomed = |guest: &mut Joining| {
        let mut welcome = None;

        until("welcomed", || {
            welcome = guest.poll(Instant::now()).expect("asking");
            welcome.is_some()
        });
        welcome.expect("welcome")
    };
    let (bob_w, cat_w) = (welcomed(&mut bob), welcomed(&mut cat));

    assert_eq!(save.as_deref(), Some(paths[0].as_path()));
    assert_eq!((peer.seat(), bob_w.seat, cat_w.seat), (0, 2, 1));
    assert_eq!(
        (bob_w.host, bob_w.save.as_deref()),
        (0, Some(game.as_str()))
    );
    assert_eq!(cat_w.nicks, ["Host", "Cat", "Bob"]);
}

/// A save that will not load into the lobby is refused, and says why: a
/// game of one, of another number of players, or one whose game is not
/// the one its meta names.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_lobby_refuses_a_save_that_does_not_fit_or_is_not_what_it_says() {
    let (_saved, paths) = every_peer_saves(2, "net-lobby-refuse-from");
    let dir = scratch("net-lobby-refuse");
    let build = lobby_build();
    let saved = crate::save::read(&paths[0]).expect("reads");
    let altered = |name: &str, meta: crate::save::Meta| {
        let path = dir.join(format!("{name}{}", crate::save::EXTENSION));
        let contents = crate::save::Contents {
            meta,
            ..saved.clone()
        };

        crate::save::write(&path, &contents).expect("written");
        path
    };
    let alone = altered(
        "alone",
        crate::save::Meta {
            lan: None,
            ..saved.meta.clone()
        },
    );
    let lan = saved.meta.lan.clone().expect("a LAN save");
    let forged = altered(
        "forged",
        crate::save::Meta {
            lan: Some(crate::save::Lan {
                game: "0000000000000000".to_owned(),
                ..lan
            }),
            ..saved.meta.clone()
        },
    );
    let lobby = |players| {
        Lobby::bind(0)
            .expect("bound")
            .host(
                (&build, Builds::Same),
                (players, Delay::Fixed(DELAY)),
                (&Setup::default(), "Host"),
            )
            .expect("hosting")
    };
    let refused =
        |players, path: &std::path::Path| choose(&mut lobby(players), path).expect_err("refused");

    assert!(refused(2, &alone).contains("a game of one"));
    assert!(refused(3, &paths[0]).contains("a game of 2 players"));
    assert!(refused(2, &forged).contains("not the game its meta names"));
    assert_eq!(choose(&mut lobby(2), &paths[0]), Ok(()));
}

/// The LAN save at `path` loaded into `hosting`, as the menus load it.
fn choose(hosting: &mut Hosting, path: &std::path::Path) -> Result<(), String> {
    hosting.choose(crate::save::lan::offered(path)?)
}

/// A save asked while every peer is paused steps them on to its frame,
/// then they stand paused again.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_save_asked_while_paused_steps_every_peer_on_to_it() {
    const ASKED: u32 = 60;

    let weather = Weather {
        latency: Duration::from_millis(5),
        jitter: Duration::ZERO,
        loss: 0,
    };
    let start = Instant::now();
    let mut seats = pair(weather, "net-save-paused", start);
    let mut now = run(&mut seats, start, ASKED, ASKED * 40);

    for seat in &mut seats {
        seat.sim.peer_mut().set_local(true, 1.0);
    }

    now = run(&mut seats, now, ASKED * 2, 40);
    assert!(seats.iter().all(|s| s.sim.peer().paused()), "all paused");

    let stood = seats.iter().map(|s| s.sim.frames()).max().expect("seats");

    seats[1].sim.ask_save("Day");
    run(&mut seats, now, ASKED * 2, ASKED * 40);

    let saves = written(&mut seats);

    assert!(seats.iter().all(|s| s.sim.peer().paused()), "paused again");
    assert!(
        saves[0].meta.frame <= stood + DELAY + 1,
        "{}",
        saves[0].meta.frame
    );
    assert_eq!(saves[0].meta.frame, saves[1].meta.frame);
}

/// Host and guest on threads over localhost UDP: the lobby, the welcome
/// with the setup, and a headless game each, the bots giving orders;
/// both end on one digest.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn two_threads_play_alike_over_localhost_udp() {
    const FRAMES: u32 = 300;

    let build = Build {
        version: "test".to_owned(),
        profile: Profile::this(),
        commit: "test".to_owned(),
        data: 2,
    };
    let (port_tx, port_rx) = std::sync::mpsc::channel();
    let host_build = build.clone();

    let host = std::thread::spawn(move || {
        let setup = crate::cli::tests::parsed(ARGS).recorded();
        let lobby = lobby::Lobby::bind(0).expect("bound");

        port_tx.send(lobby.port()).expect("told");

        let seated = lobby
            .seat(
                (&host_build, Builds::Same),
                (2, Delay::Fixed(DELAY)),
                (&setup, "Host"),
            )
            .expect("seated");
        let peer = Peer::hosted(seated, None).expect("peer");
        let dir = scratch("net-udp-0");
        let (mut sim, bot) = game(&setup, (0, peer), &dir);
        let ran = crate::game::drive::headless(&mut sim, FRAMES, &mut Some(bot), |_| ());

        (ran.frames, sim.digest(), sim.session().map(|s| s.events))
    });

    let guest = std::thread::spawn(move || {
        let port: u16 = port_rx.recv().expect("port");
        let (peer, welcome) =
            Peer::join(&format!("127.0.0.1:{port}"), (&build, "Guest"), None).expect("joined");

        assert_eq!(welcome.nicks, ["Host", "Guest"]);
        assert_eq!(
            (peer.nick(0), peer.nick(1)),
            ("Host".into(), "Guest".into())
        );
        let dir = scratch("net-udp-1");
        let (mut sim, bot) = game(&welcome.setup, (welcome.seat, peer), &dir);
        let ran = crate::game::drive::headless(&mut sim, FRAMES, &mut Some(bot), |_| ());

        (ran.frames, sim.digest(), sim.session().map(|s| s.events))
    });

    let host = host.join().expect("host");
    let guest = guest.join().expect("guest");

    assert_eq!(host.0, FRAMES);
    assert_eq!(host, guest);
}

/// Polls `step` until it says done, a millisecond apart: localhost
/// packets take a moment.
fn until(what: &str, mut step: impl FnMut() -> bool) {
    let done = (0..1_000).any(|_| {
        let done = step();

        if !done {
            std::thread::sleep(Duration::from_millis(1));
        }

        done
    });

    assert!(done, "{what}");
}

/// The build every lobby end of the tests runs.
fn lobby_build() -> Build {
    Build {
        version: "test".to_owned(),
        profile: Profile::this(),
        commit: "test".to_owned(),
        data: 2,
    }
}

/// The lobby polled on one thread, as the menus poll it: each guest
/// seated with its nick, a full lobby refusing, a guest gone silent let
/// go and the seats after it moving up, then every guest welcomed with
/// the setup and every seat's nick.
#[test]
fn a_polled_lobby_seats_guests_by_nick_and_lets_a_silent_one_go() {
    let build = lobby_build();
    let setup = Setup {
        seed: 7,
        ..Setup::default()
    };
    let mut hosting = Lobby::bind(0)
        .expect("bound")
        .host(
            (&build, Builds::Same),
            (3, Delay::Fixed(DELAY)),
            (&setup, "Host"),
        )
        .expect("hosting");
    let host = std::net::SocketAddr::from(([127, 0, 0, 1], hosting.port()));
    let now = Instant::now();
    let later = now + Duration::from_secs(3);
    let join = |nick, at| Joining::new(host, (&build, nick), at).expect("asking");
    let (mut ann, mut bob, mut cat) = (join("Ann", now), join("Bob", now), join("Cat", now));

    assert_eq!(ann.poll(now), Ok(None));
    until("Ann seated", || {
        hosting.poll(now);
        hosting.seats() == [Some("Host"), Some("Ann"), None]
    });
    until("Ann told", || {
        ann.poll(now).expect("asking");
        ann.seated() == Some((1, 3))
    });

    assert_eq!(bob.poll(now), Ok(None));
    until("Bob seated", || hosting.poll(now));
    assert_eq!(cat.poll(now), Ok(None));
    until("Cat refused", || {
        hosting.poll(now);
        cat.poll(now)
            .is_err_and(|why| why.contains("every seat is taken"))
    });

    // Ann falls silent; Bob asks on, and takes seat 1.
    assert_eq!(bob.poll(later), Ok(None));
    until("Ann let go", || {
        !hosting.poll(later) && hosting.seats() == [Some("Host"), Some("Bob"), None]
    });

    let mut cat = join("Cat", later);

    assert_eq!(cat.poll(later), Ok(None));
    until("Cat seated", || hosting.poll(later));
    assert_eq!(hosting.seats(), [Some("Host"), Some("Bob"), Some("Cat")]);

    let peer = Peer::seated(hosting, None).expect("peer").0;
    let welcomed = |guest: &mut Joining| {
        let mut welcome = None;

        until("welcomed", || {
            welcome = guest.poll(later).expect("asking");
            welcome.is_some()
        });
        welcome.expect("welcome")
    };
    let (bob, cat) = (welcomed(&mut bob), welcomed(&mut cat));

    assert_eq!((bob.seat, cat.seat, bob.players), (1, 2, 3));
    assert_eq!((bob.setup, bob.delay), (setup, DELAY));
    assert_eq!(cat.nicks, ["Host", "Bob", "Cat"]);
    assert_eq!((peer.players(), peer.nick(2)), (3, "Cat".to_owned()));
}

/// The lobby as the menus run it, a hello each step of the clock: the
/// session's name, setup and seats shown to the guests, chat carried
/// every way once, a guest's ready lapsing with a changed setup, the
/// change said in the host's and every guest's chat, and
/// the game startable only with every seat taken and every guest
/// ready; a guest of other data refused.
#[test]
#[allow(clippy::too_many_lines, reason = "one lobby, step by step")]
fn a_lobby_shows_its_setup_carries_chat_and_starts_only_when_all_are_ready() {
    let build = lobby_build();
    let setup = Setup {
        gold: 30_000,
        ..Setup::default()
    };
    let changed = Setup {
        gold: 10_000,
        ..Setup::default()
    };
    let mut hosting = Lobby::bind(0)
        .expect("bound")
        .host(
            (&build, Builds::Same),
            (3, Delay::Fixed(DELAY)),
            (&setup, "Host"),
        )
        .expect("hosting");
    let host = std::net::SocketAddr::from(([127, 0, 0, 1], hosting.port()));
    let clock = std::cell::Cell::new(Instant::now());
    let join = |nick, at| Joining::new(host, (&build, nick), at).expect("asking");
    let mut ann = join("Ann", clock.get());
    let mut bob = join("Bob", clock.get());

    hosting.set_title("foobar");

    // Each step a hello's interval on: every guest says hello, the host
    // answers, the guests hear it.
    let step = |hosting: &mut Hosting, guests: &mut [&mut Joining]| {
        clock.set(clock.get() + Duration::from_millis(250));

        for guest in guests.iter_mut() {
            assert_eq!(guest.poll(clock.get()), Ok(None));
        }

        std::thread::sleep(Duration::from_millis(2));
        hosting.poll(clock.get());
        std::thread::sleep(Duration::from_millis(2));

        for guest in guests.iter_mut() {
            assert_eq!(guest.poll(clock.get()), Ok(None));
        }
    };
    let seats = |room: &Room| {
        room.seats
            .iter()
            .map(|s| s.nick.clone())
            .collect::<Vec<_>>()
    };

    until("Ann seated", || {
        step(&mut hosting, &mut [&mut ann]);
        ann.room().is_some_and(|r| r.seats.len() == 2)
    });
    assert_eq!(hosting.unready(), Some(Unready::Seats));

    until("Bob seated", || {
        step(&mut hosting, &mut [&mut ann, &mut bob]);
        [&ann, &bob]
            .iter()
            .all(|g| g.room().is_some_and(|r| r.seats.len() == 3))
    });

    let room = bob.room().expect("a room").clone();

    assert_eq!(
        (room.title.as_str(), room.seat, room.players),
        ("foobar", 2, 3)
    );
    assert_eq!((room.setup, room.revision), (setup.clone(), 0));
    assert_eq!(seats(bob.room().expect("a room")), ["Host", "Ann", "Bob"]);
    assert_eq!(room.seats[1].build, build.shown());
    assert_eq!(hosting.unready(), Some(Unready::Guests));

    // Both ready: startable. The setup changed: neither is, till each
    // readies again.
    ann.set_ready(Ready::For(0));
    bob.set_ready(Ready::For(0));
    until("both ready", || {
        step(&mut hosting, &mut [&mut ann, &mut bob]);
        hosting.unready().is_none()
    });
    assert!(hosting.view().seats.iter().all(|s| s.ready));

    hosting.set_setup(&changed);
    assert_eq!(hosting.unready(), Some(Unready::Guests));
    until("the change heard", || {
        step(&mut hosting, &mut [&mut ann, &mut bob]);
        bob.room().is_some_and(|r| r.revision == 1)
    });
    assert!(!ann.ready() && !bob.ready());
    assert_eq!(bob.room().map(|r| r.setup.clone()), Some(changed.clone()));

    // Chat each way, held once by all.
    hosting.say("welcome");
    ann.say("hi");
    ann.say("again");
    until("chat heard", || {
        step(&mut hosting, &mut [&mut ann, &mut bob]);
        bob.chat().len() == 4 && ann.chat().len() == 4
    });

    for _ in 0..4 {
        step(&mut hosting, &mut [&mut ann, &mut bob]);
    }

    let chat = hosting.view().chat.lines;
    let line = |who: &str, what: &str| Talk::Said(who.to_owned(), what.to_owned());

    assert_eq!(chat.len(), 4);
    assert_eq!(chat[0], Talk::Changed);
    assert!(chat.contains(&line("Host", "welcome")));
    assert!(chat.contains(&line("Ann", "hi")) && chat.contains(&line("Ann", "again")));
    assert_eq!((ann.chat(), bob.chat()), (&chat[..], &chat[..]));

    // Other data: refused.
    let other = Build {
        data: 9,
        ..lobby_build()
    };
    let mut eve = Joining::new(host, (&other, "Eve"), clock.get()).expect("asking");

    until("Eve refused", || {
        let _ = eve.poll(clock.get());
        hosting.poll(clock.get());
        eve.poll(clock.get()).is_err_and(|why| why.contains("data"))
    });

    // Ready again, then started: each welcomed to the changed setup.
    ann.set_ready(Ready::For(1));
    bob.set_ready(Ready::For(1));
    until("ready again", || {
        step(&mut hosting, &mut [&mut ann, &mut bob]);
        hosting.unready().is_none()
    });

    let _peer = Peer::seated(hosting, None).expect("peer").0;

    for guest in [&mut ann, &mut bob] {
        let mut welcome = None;

        until("welcomed", || {
            welcome = guest.poll(clock.get()).expect("asking");
            welcome.is_some()
        });
        assert_eq!(welcome.map(|w| w.setup), Some(changed.clone()));
    }
}

/// A host that gives up tells its seated guests why; a seat with no
/// nick is named by its number.
#[test]
fn a_host_that_leaves_tells_its_guests_and_no_nick_is_a_seat_number() {
    let build = lobby_build();
    let mut hosting = Lobby::bind(0)
        .expect("bound")
        .host(
            (&build, Builds::Same),
            (3, Delay::Fixed(DELAY)),
            (&Setup::default(), ""),
        )
        .expect("hosting");
    let host = std::net::SocketAddr::from(([127, 0, 0, 1], hosting.port()));
    let now = Instant::now();
    let mut guest = Joining::new(host, (&build, ""), now).expect("asking");

    assert_eq!(guest.poll(now), Ok(None));
    until("seated", || {
        hosting.poll(now);
        hosting.seats() == [Some(""), Some(""), None]
    });

    hosting.leave();

    until("told", || {
        guest
            .poll(now)
            .is_err_and(|why| why.contains("the host left"))
    });

    assert_eq!(Peer::solo().nick(0), "p0");
}

/// LAN discovery over localhost: three hosts found with their sessions,
/// seats, maps and builds, the one of other data marked; a guest joins
/// one at the address found and its seats count up; a host that leaves
/// drops off the list.
#[test]
#[allow(clippy::too_many_lines, reason = "one search, step by step")]
fn lan_discovery_finds_hosts_marks_a_misfit_and_drops_one_gone() {
    let build = lobby_build();
    let other = Build {
        data: 9,
        ..lobby_build()
    };
    let setup = Setup {
        seed: 7,
        land: Some(regent_core::data::fourcc::FourCc::new(b"East")),
        map_seed: Some(12),
        ..Setup::default()
    };
    let mut finder = Finder::on(0).expect("finder");
    let to = vec![std::net::SocketAddr::from(([127, 0, 0, 1], finder.port()))];
    let host = |title: &str, nick, build: &Build| {
        let mut hosting = Lobby::bind(0)
            .expect("bound")
            .host(
                (build, Builds::Same),
                (2, Delay::Fixed(DELAY)),
                (&setup, nick),
            )
            .expect("hosting");

        hosting.announce_to(to.clone()).expect("beacon");
        hosting.set_title(title);
        hosting
    };
    let mut ann = host("Ann's game", "Ann", &build);
    let mut bob = host("Bob's game", "Bob", &build);
    let mut cat = host("Cat's game", "Cat", &other);
    let now = Instant::now();
    let titles = |finder: &Finder| {
        let mut titles: Vec<String> = finder
            .found()
            .iter()
            .map(|f| f.open.title.clone())
            .collect();

        titles.sort();
        titles
    };

    until("three found", || {
        finder.poll(now);
        ann.poll(now);
        bob.poll(now);
        cat.poll(now);
        finder.poll(now);
        finder.found().len() == 3
    });
    assert_eq!(titles(&finder), ["Ann's game", "Bob's game", "Cat's game"]);

    let find = |finder: &Finder, nick: &str| {
        finder
            .found()
            .iter()
            .find(|f| f.open.nick == nick)
            .cloned()
            .expect("found")
    };
    let found_ann = find(&finder, "Ann");

    assert_eq!(found_ann.host.port(), ann.port());
    assert_eq!((found_ann.open.seated, found_ann.open.players), (1, 2));
    assert_eq!(
        found_ann.open.setup,
        Setup {
            seed: Setup::default().seed,
            ..setup.clone()
        }
    );
    assert_eq!(found_ann.open.save, None);
    assert_eq!(found_ann.misfit(&build), None);
    assert!(!found_ann.full());
    assert!(
        find(&finder, "Cat")
            .misfit(&build)
            .is_some_and(|why| why.contains("data"))
    );

    // A guest joins Ann at the address found: her game is full.
    let mut guest = Joining::new(found_ann.host, (&build, "Dan"), now).expect("asking");

    assert_eq!(guest.poll(now), Ok(None));
    until("Dan seated", || ann.poll(now));

    let later = now + Duration::from_secs(1);

    until("Ann full", || {
        finder.poll(later);
        ann.poll(later);
        bob.poll(later);
        cat.poll(later);
        finder.poll(later);
        find(&finder, "Ann").full()
    });

    // Bob leaves; Ann and Cat announce on.
    bob.leave();

    let gone = now + Duration::from_secs(4);

    until("Bob dropped", || {
        finder.poll(gone);
        ann.poll(gone);
        cat.poll(gone);
        finder.poll(gone);
        titles(&finder) == ["Ann's game", "Cat's game"]
    });
}

/// Every guest on a machine listens on the one discovery port: a second
/// finder binds where the first is. Each lobby's id is its own.
#[test]
fn guests_on_one_machine_share_the_discovery_port() {
    let first = Finder::on(0).expect("first");
    let second = Finder::on(first.port()).expect("second");
    let beacon = || discover::Beacon::new(discover::Targets::Given(Vec::new())).expect("beacon");

    assert_eq!(first.port(), second.port());
    assert_ne!(beacon().id(), beacon().id());
}

/// Host and guest over localhost UDP, each game on a pipelined sim
/// thread: the guest's drawing stalls past `node::WAIT_AFTER` after its
/// first frame, as a minimised window's does. Its sim thread keeps time,
/// pumps and steps alone: neither peer is dropped, neither freezes, and
/// both play alike.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_stalled_drawing_drops_no_peer_and_freezes_none() {
    use crate::game::seen::Interest;
    use crate::game::thread::{Mode, Pace, SimThread};

    const STALL: Duration = Duration::from_millis(6_000);
    const DRAWN: Duration = Duration::from_millis(16);
    const RUNNING: Pace = Pace {
        paused: false,
        speed: 1.0,
    };

    let build = lobby_build();
    let (port_tx, port_rx) = std::sync::mpsc::channel();
    let host_build = build.clone();

    // The lobby blocks: each side seated on its own thread.
    let host = std::thread::spawn(move || {
        let setup = crate::cli::tests::parsed(ARGS).recorded();
        let lobby = lobby::Lobby::bind(0).expect("bound");

        port_tx.send(lobby.port()).expect("told");

        let seated = lobby
            .seat(
                (&host_build, Builds::Same),
                (2, Delay::Fixed(DELAY)),
                (&setup, "Host"),
            )
            .expect("seated");
        let peer = Peer::hosted(seated, None).expect("peer");

        let dir = scratch("net-stall-0");

        (game(&setup, (0, peer), &dir).0, dir)
    });
    let guest = std::thread::spawn(move || {
        let port: u16 = port_rx.recv().expect("port");
        let (peer, welcome) =
            Peer::join(&format!("127.0.0.1:{port}"), (&build, "Guest"), None).expect("joined");

        let dir = scratch("net-stall-1");

        (game(&welcome.setup, (welcome.seat, peer), &dir).0, dir)
    });
    let spawn = |sim: Sim| {
        SimThread::spawn(sim, std::sync::Arc::default(), Mode::Pipelined)
            .expect("spawned")
            .0
    };
    let (host, _host_dir) = host.join().expect("host");
    let (guest, _guest_dir) = guest.join().expect("guest");
    let mut host = spawn(host);
    let mut guest = spawn(guest);

    // Each drawing's first frame starts its clock; the guest's then stalls.
    let _ = guest.frame(Interest::default(), RUNNING, DRAWN.as_secs_f32());
    let began = Instant::now();

    while began.elapsed() < STALL {
        let _ = host.frame(Interest::default(), RUNNING, DRAWN.as_secs_f32());
        host.told();

        std::thread::sleep(DRAWN);
    }

    let look = |sim: &mut Sim| (sim.frames(), sim.peer().dropped(), sim.stopped.clone());
    let (host_frames, host_dropped, host_stopped) = host.run(look).expect("host");
    let (guest_frames, guest_dropped, guest_stopped) = guest.run(look).expect("guest");

    // 6 s: 120 frames, but for a busy machine's lag.
    let least = STALL.as_millis() / FRAME.as_millis() * 3 / 4;

    assert_eq!((host_stopped, guest_stopped), (None, None));
    assert!(!host_dropped && !guest_dropped, "no peer dropped");
    assert!(
        u128::from(guest_frames) >= least,
        "guest ran: {guest_frames}"
    );
    assert!(u128::from(host_frames) >= least, "host ran: {host_frames}");
    assert!(
        host_frames.abs_diff(guest_frames) <= 2 * DELAY,
        "in lockstep: {host_frames} {guest_frames}"
    );
}

/// A player's leaving is said in the game's words, `GMTX` 298 and 295;
/// the host's adds that the guest plays on alone.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_leaving_is_said_in_the_games_words() {
    let hd = crate::files::env::path(crate::files::env::HD).expect("MAJESTY_HD");
    let archives = crate::game::load::archives(&hd).expect("the dataset");
    let catalogue = regent_draw::data::catalogue(&archives).expect("catalogue");
    let text = regent_core::data::text::Text::build(&catalogue);
    let line = |how, who| left("Ann", how, who)[0].line(&text);

    assert_eq!(line(Leave::Quit, Who::Player), "Ann quit the game.");
    assert_eq!(line(Leave::Silent, Who::Player), "Ann was dropped.");
    assert_eq!(
        line(Leave::Quit, Who::Host),
        "Ann quit the game. You play on alone."
    );
}

/// A host's welcome the guest's lockstep cannot keep is refused, not
/// indexed by: a seat past the players, no players, more than four, a
/// delay below the least, a host's seat past the players.
#[test]
fn a_welcome_past_its_seats_is_refused() {
    let welcome = |seat, players, delay, host| Welcome {
        seat,
        players,
        delay,
        setup: Setup::default(),
        nicks: Vec::new(),
        host,
        save: None,
    };
    let guest = |w: &Welcome| {
        let socket = std::net::UdpSocket::bind("127.0.0.1:0").expect("a socket");
        let host = socket.local_addr().expect("an address");

        Peer::guest(socket, host, w, None).is_ok()
    };

    assert!(guest(&welcome(1, 2, DELAY, 0)));
    assert!(!guest(&welcome(2, 2, DELAY, 0)));
    assert!(!guest(&welcome(0, 0, DELAY, 0)));
    assert!(!guest(&welcome(1, 9, DELAY, 0)));
    assert!(!guest(&welcome(1, 2, 0, 0)));
    assert!(!guest(&welcome(1, 2, DELAY, 2)));
    assert!(guest(&welcome(1, 2, MOST_DELAY, 0)));
    assert!(!guest(&welcome(1, 2, MOST_DELAY + 1, 0)));
    assert!(!guest(&welcome(1, 2, u32::MAX, 0)));
}

/// A loaded game may seat the host past seat 0: its goodbye is the
/// host's, a seat 0 guest's a player's.
#[test]
fn the_host_is_told_by_its_seat_not_seat_0() {
    let now = Instant::now();
    let (host_at, guest_at) = (
        std::net::SocketAddr::from(([10, 0, 0, 1], 1)),
        std::net::SocketAddr::from(([10, 0, 0, 2], 2)),
    );
    let peer = |role, seat, remote: (std::net::SocketAddr, u32)| {
        let node = Node::new(
            (role, seat),
            (2, DELAY),
            vec![(remote.0, remote.1, None)],
            now,
        );

        Peer::new(node, Box::new(Loopback::default()), None)
            .named(vec!["Ann".to_owned(), "Host".to_owned()])
    };
    let pair = || {
        (
            peer(Role::Host, 1, (guest_at, 0)),
            peer(Role::Guest, 0, (host_at, 1)),
        )
    };
    let told = |from: &mut Peer, to: &mut Peer, at| {
        for (_, bytes) in from.node.bye() {
            let packet = wire::Packet::decode(&bytes).expect("decoded");

            to.node.receive(at, packet, now);
        }

        to.take_left()
    };
    let (mut host, mut guest) = pair();

    assert_eq!(
        told(&mut host, &mut guest, host_at),
        left("Host", Leave::Quit, Who::Host)
    );

    let (mut host, mut guest) = pair();

    assert_eq!(
        told(&mut guest, &mut host, guest_at),
        left("Ann", Leave::Quit, Who::Player)
    );
}

/// A lobby of three, Ann and Bob always ready, played into a game over
/// localhost UDP: the three peers, each named, pumped till all talk.
fn three_in_play(build: &Build) -> (Peer, Peer, Peer) {
    let mut hosting = Lobby::bind(0)
        .expect("bound")
        .host(
            (build, Builds::Same),
            (3, Delay::Fixed(DELAY)),
            (&Setup::default(), "Host"),
        )
        .expect("hosting");
    let host = std::net::SocketAddr::from(([127, 0, 0, 1], hosting.port()));
    let now = Instant::now();
    let join = |nick| {
        let mut joining = Joining::new(host, (build, nick), now).expect("asking");

        joining.set_ready(Ready::Always);
        joining
    };
    let (mut ann, mut bob) = (join("Ann"), join("Bob"));

    until("seated", || {
        let _ = (ann.poll(Instant::now()), bob.poll(Instant::now()));

        hosting.poll(Instant::now());
        hosting.unready().is_none()
    });

    let host = Peer::seated(hosting, None).expect("peer").0;
    let welcomed = |mut guest: Joining| {
        let mut welcome = None;

        until("welcomed", || {
            welcome = guest.poll(Instant::now()).expect("asking");
            welcome.is_some()
        });
        Peer::welcomed(guest, &welcome.expect("welcome"), None).expect("peer")
    };
    let mut peers = [host, welcomed(ann), welcomed(bob)];

    for _ in 0..50 {
        pump(&mut peers);
        std::thread::sleep(Duration::from_millis(2));
    }

    let [host, ann, bob] = peers;

    (host, ann, bob)
}

/// Each peer's packets in and out.
fn pump(peers: &mut [Peer]) {
    for peer in peers {
        peer.pump(Instant::now());
    }
}

/// Return to Lobby: Bob goes back first and waits while the host plays
/// on; the host goes back and calls Ann from her game; both are seated
/// again in their seats, the next game startable once they are ready,
/// each welcomed to it.
#[test]
fn return_to_lobby_seats_the_same_players_again() {
    let build = lobby_build();
    let (mut host, mut ann, mut bob) = three_in_play(&build);
    let back = bob.regroup().expect("a socket");

    assert!(!back.hosts());

    let mut bob_back = back.join(&build, Instant::now()).expect("asking");

    until("Bob waits", || {
        pump(std::slice::from_mut(&mut host));
        bob_back.poll(Instant::now()).expect("asking");
        bob_back.held()
    });
    assert!(bob_back.room().is_none());

    let back = host.regroup().expect("a socket");
    let setup = Setup {
        gold: 30_000,
        ..Setup::default()
    };

    assert!(back.hosts());
    assert_eq!(back.players(), 3);

    let mut hosting = back
        .host((&build, Builds::Same), (&setup, "Again"), Instant::now())
        .expect("hosting");

    until("Ann called", || {
        pump(std::slice::from_mut(&mut ann));
        hosting.poll(Instant::now());
        ann.regrouped()
    });

    let mut ann_back = ann
        .regroup()
        .expect("a socket")
        .join(&build, Instant::now())
        .expect("asking");

    until("both seated", || {
        let now = Instant::now();

        hosting.poll(now);
        ann_back.poll(now).expect("asking");
        bob_back.poll(now).expect("asking");
        ann_back.seated() == Some((1, 3)) && bob_back.seated() == Some((2, 3))
    });
    assert_eq!(hosting.seats(), [Some("Host"), Some("Ann"), Some("Bob")]);
    assert_eq!(hosting.unready(), Some(Unready::Guests));
    assert!(!bob_back.held());
    assert_eq!(
        bob_back.room().map(|r| (r.title.clone(), r.setup.clone())),
        Some(("Again".to_owned(), setup.clone()))
    );

    ann_back.set_ready(Ready::Always);
    bob_back.set_ready(Ready::Always);
    until("ready", || {
        let now = Instant::now();

        let _ = (ann_back.poll(now), bob_back.poll(now));
        hosting.poll(now);
        hosting.unready().is_none()
    });

    // The running game had a delay of DELAY; the next is picked anew.
    for seat in [1, 2] {
        for _ in 0..delay::SAMPLES {
            hosting.sample(seat, Duration::from_millis(200));
        }
    }

    hosting.poll(Instant::now());

    let host = Peer::seated(hosting, None).expect("peer").0;
    let welcomed = |guest: &mut Joining| {
        let mut welcome = None;

        until("welcomed", || {
            welcome = guest.poll(Instant::now()).expect("asking");
            welcome.is_some()
        });
        welcome.expect("welcome")
    };
    let (ann, bob) = (welcomed(&mut ann_back), welcomed(&mut bob_back));

    assert_eq!((ann.seat, bob.seat), (1, 2));
    assert_eq!(bob.nicks, ["Host", "Ann", "Bob"]);
    assert_eq!(host.players(), 3);
    assert!(host.delay() > DELAY, "a new delay: {}", host.delay());
    assert_eq!(bob.delay, host.delay());
}

/// A guest that quit the game, not back to the lobby, is gone from the
/// host's lobby; the one called back is seated.
#[test]
fn return_to_lobby_leaves_out_a_guest_that_quit() {
    let build = lobby_build();
    let (mut host, mut ann, bob) = three_in_play(&build);

    drop(bob);
    until("Bob gone", || {
        pump(std::slice::from_mut(&mut host));
        !host.take_left().is_empty()
    });

    let mut hosting = host
        .regroup()
        .expect("a socket")
        .host(
            (&build, Builds::Same),
            (&Setup::default(), ""),
            Instant::now(),
        )
        .expect("hosting");

    assert_eq!(hosting.seats(), [Some("Host"), Some("Ann"), None]);

    until("Ann called", || {
        pump(std::slice::from_mut(&mut ann));
        hosting.poll(Instant::now());
        ann.regrouped()
    });

    let mut ann_back = ann
        .regroup()
        .expect("a socket")
        .join(&build, Instant::now())
        .expect("asking");

    until("Ann seated", || {
        hosting.poll(Instant::now());
        ann_back.poll(Instant::now()).expect("asking");
        ann_back.seated() == Some((1, 3))
    });
    assert_eq!(hosting.seats(), [Some("Host"), Some("Ann"), None]);
    assert!(Peer::solo().regroup().is_none());
}

/// `players` seats over a network of one-way `latency` and a delay of
/// `delay`, as the drawn game is paced: how much longer than real time
/// `frames` frames take, as a fraction; the seats play alike.
fn slowdown(players: u16, (latency, delay): (Duration, u32), frames: u32) -> f64 {
    let weather = Weather {
        latency,
        jitter: Duration::ZERO,
        loss: 0,
    };
    let count = players.to_string();
    let setup = crate::cli::tests::parsed(["openregent", "--players", &count]).recorded();
    let start = Instant::now();
    let mut seats = table_at(
        players,
        (weather, delay),
        "net-delay",
        start,
        &|seat, peer, dir| made(&setup, (seat, peer), dir),
    );
    let end = run(&mut seats, start, frames, frames * 60);

    assert!(seats.iter().all(|s| s.sim.frames() == frames));
    assert!(
        seats
            .windows(2)
            .all(|w| w[0].sim.digest() == w[1].sim.digest())
    );

    end.duration_since(start).as_secs_f64() / (FRAME.as_secs_f64() * f64::from(frames)) - 1.0
}

/// The delay absorbs a one-way path of `delay - 1` frames less a few
/// milliseconds, and no more, between two peers and, with the guest to
/// guest path through the host being twice a link, between three. This is
/// where `delay`'s formula comes from.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn delay_absorbs_the_path_it_is_picked_for() {
    const FRAMES: u32 = 400;
    // Slower than real time by more than this is not kept.
    const KEPT: f64 = 0.03;
    const FRAME_MS: u64 = 50;

    for players in [2_u16, 3] {
        // The longest path: a link, or two of them through the host.
        let links = u64::from(players - 1);

        for delay in 2..=4_u32 {
            let path = u64::from(delay - 1) * FRAME_MS;
            let at = |path: u64| (Duration::from_millis(path / links), delay);

            let inside = slowdown(players, at(path - 10), FRAMES);
            let outside = slowdown(players, at(path + 30), FRAMES);

            assert!(inside < KEPT, "{players} peers, delay {delay}: {inside:.3}");
            assert!(
                outside > KEPT,
                "{players} peers, delay {delay}: {outside:.3}"
            );
        }
    }
}

/// A host whose guests ping 160 ms round trips picks more than the
/// default, which on that network (80 ms a link, 160 ms guest to guest)
/// would run the game far slower than real time; three peers play alike
/// with the pick, at close to real time. With too few pings it picks the
/// default.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn the_host_picks_a_delay_a_slow_network_keeps() {
    const FRAMES: u32 = 400;
    const LATENCY: Duration = Duration::from_millis(80);

    let build = lobby_build();
    let mut hosting = Lobby::bind(0)
        .expect("bound")
        .host(
            (&build, Builds::Same),
            (3, Delay::Auto),
            (&Setup::default(), "Host"),
        )
        .expect("hosting");
    let host = std::net::SocketAddr::from(([127, 0, 0, 1], hosting.port()));
    let now = Instant::now();
    let join = |nick| Joining::new(host, (&build, nick), now).expect("asking");
    let (mut ann, mut bob) = (join("Ann"), join("Bob"));

    until("both seated", || {
        hosting.poll(now);
        ann.poll(now).expect("asking");
        bob.poll(now).expect("asking");
        hosting.seats() == [Some("Host"), Some("Ann"), Some("Bob")]
    });
    assert!(!hosting.sampled(), "one ping each is too few");
    assert_eq!(hosting.view().delay, DELAY, "the default without pings");

    for seat in [1, 2] {
        for _ in 0..delay::SAMPLES {
            hosting.sample(seat, LATENCY * 2);
        }
    }

    hosting.poll(now);

    let picked = hosting.view().delay;

    assert!(hosting.sampled());
    assert!(picked > DELAY, "{picked}");
    assert!(
        slowdown(3, (LATENCY, DELAY), FRAMES) > 0.1,
        "the default is too little"
    );
    assert!(
        slowdown(3, (LATENCY, picked), FRAMES) < 0.03,
        "the pick keeps real time"
    );
}

/// A made-up network of one-way `ms`, `jitter_ms` of jitter and `loss`
/// in a hundred lost.
const fn sky(ms: u64, jitter_ms: u64, loss: u32) -> Weather {
    Weather {
        latency: Duration::from_millis(ms),
        jitter: Duration::from_millis(jitter_ms),
        loss,
    }
}

/// Each seat's delay changes seen: the step it took effect after, and
/// the delay.
type Changes = Vec<Vec<(u32, u32)>>;

/// A game whose host steers the delay while the weather turns.
struct Voyage {
    seats: Vec<Seat>,
    now: Instant,
    seen: Changes,
    /// Weather changes to come, by the host's frame.
    weather: Vec<(u32, Weather)>,
    /// The delay each seat last showed.
    last: Vec<u32>,
}

impl Voyage {
    /// `players` seats in weather `start`, the host steering from `delay`.
    fn new(players: u16, (start, delay, mode): (Weather, u32, Delay), name: &str) -> Voyage {
        let count = players.to_string();
        let setup = crate::cli::tests::parsed(["openregent", "--players", &count]).recorded();
        let now = Instant::now();
        let seats = table_by(
            players,
            (start, delay, mode),
            name,
            now,
            &|seat, peer, dir| made(&setup, (seat, peer), dir),
        );

        Voyage {
            seen: vec![Vec::new(); seats.len()],
            last: vec![delay; seats.len()],
            seats,
            now,
            weather: Vec::new(),
        }
    }

    /// The weather turns to `weather` once the host is at `frame`.
    fn then(mut self, frame: u32, weather: Weather) -> Voyage {
        self.weather.push((frame, weather));
        self
    }

    /// On until every seat is at `last`, or `most` ticks; `each` is
    /// called every tick.
    fn sail(&mut self, last: u32, most: u32, each: &mut dyn FnMut(&mut [Seat])) {
        for _ in 0..most {
            self.now += TICK;

            let (seen, held) = (&mut self.seen, &mut self.last);

            tick_by(&mut self.seats, self.now, last, &mut |n, sim| {
                let delay = sim.peer().delay();

                if delay != held[n] {
                    seen[n].push((sim.peer().steps(), delay));
                    held[n] = delay;
                }
            });

            if let Some((at, weather)) = self.weather.first().copied()
                && self.seats[0].sim.frames() >= at
            {
                self.weather.remove(0);
                self.seats[0].end.set_weather(weather);
            }

            each(&mut self.seats);

            let done = self
                .seats
                .iter()
                .filter(|s| s.on)
                .all(|s| s.sim.frames() >= last && s.sim.peer().settled());

            if done {
                break;
            }
        }
    }

    /// Every seat took the same changes at the same steps, none sealed
    /// out of order, and ended on one delay.
    fn alike(&self) {
        for (n, seen) in self.seen.iter().enumerate() {
            assert_eq!(seen, &self.seen[0], "seat {n} changed as the host did");
        }

        for seat in &self.seats {
            assert_eq!(seat.sim.peer().misseals(), 0, "no frame sealed twice");
            assert_eq!(seat.sim.peer().delay(), self.seats[0].sim.peer().delay());
        }
    }

    /// Real time from now over `frames` more frames, as a fraction
    /// longer than game time.
    fn pace(&mut self, frames: u32) -> f64 {
        let (from, began) = (self.seats[0].sim.frames(), self.now);

        self.sail(from + frames, frames * 80, &mut |_| {});

        self.now.duration_since(began).as_secs_f64() / (FRAME.as_secs_f64() * f64::from(frames))
            - 1.0
    }
}

/// Latency rising mid-game: the delay rises to cover it on every peer
/// at the same step, the digests and recordings stay alike, a recording
/// replays alone to the same game, and the game returns to near real
/// time.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_rising_latency_raises_the_delay_on_every_peer_at_one_step() {
    const RISE: u32 = 150;
    const SETTLED: u32 = 500;
    const KEPT: u32 = 400;

    let mut v =
        Voyage::new(2, (sky(10, 0, 0), 2, Delay::Auto), "net-steer-up").then(RISE, sky(150, 0, 0));

    v.sail(SETTLED, SETTLED * 80, &mut |_| {});

    assert!(v.seats[0].sim.peer().delay() >= 5, "{:?}", v.seen);

    // Near real time again: the delay covers the path.
    let slow = v.pace(KEPT);

    assert!(slow < 0.05, "{slow:.3}");
    v.alike();
    one_game(&v.seats, SETTLED + KEPT, 0);
}

/// Latency falling: the delay lowers only after the hold, no frame is
/// sealed twice or missed, and the peers play alike.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_falling_latency_lowers_the_delay_after_the_hold() {
    const FALL: u32 = 100;
    const FRAMES: u32 = 1_100;

    let hold = u32::try_from(delay::LOWER_HOLD.as_millis() / FRAME.as_millis()).expect("frames");
    let mut v = Voyage::new(2, (sky(150, 0, 0), 5, Delay::Auto), "net-steer-down")
        .then(FALL, sky(10, 0, 0));

    v.sail(FALL + hold / 2, FRAMES * 80, &mut |_| {});
    assert!(
        v.seen[0].is_empty(),
        "no lowering before the hold: {:?}",
        v.seen
    );
    v.sail(FRAMES, FRAMES * 80, &mut |_| {});

    assert!(v.seats[0].sim.peer().delay() <= 3, "{:?}", v.seen);
    assert!(
        v.seen[0].iter().all(|(step, _)| *step >= FALL + hold),
        "{:?}",
        v.seen
    );
    v.alike();
    one_game(&v.seats, FRAMES, 0);
    assert_eq!(
        players(&v.seats[0].sim),
        [0, 1].into(),
        "both seats' orders"
    );
}

/// Three peers over a lossy, jittery network whose path rises and falls:
/// none is dropped and all play alike, the delay moving both ways.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn three_peers_follow_a_lossy_network_up_and_down() {
    const FRAMES: u32 = 1_800;

    let mut v = Voyage::new(3, (sky(10, 20, 10), 3, Delay::Auto), "net-steer-three")
        .then(100, sky(90, 30, 10))
        .then(500, sky(10, 20, 10));

    v.sail(FRAMES, FRAMES * 80, &mut |_| {});

    let moves = &v.seen[0];
    let rises = moves
        .iter()
        .zip(moves.iter().skip(1))
        .filter(|(a, b)| b.1 > a.1)
        .count();
    let falls = moves
        .iter()
        .zip(moves.iter().skip(1))
        .filter(|(a, b)| b.1 < a.1)
        .count();

    assert!(
        moves.first().is_some_and(|c| c.1 > 3) || rises > 0,
        "{moves:?}"
    );
    assert!(falls > 0, "{moves:?}");
    v.alike();
    one_game(&v.seats, FRAMES, 0);
}

/// A told delay never changes, whatever the network does.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_told_delay_never_changes() {
    const FRAMES: u32 = 400;

    let mut v = Voyage::new(2, (sky(10, 0, 0), 3, Delay::Fixed(3)), "net-steer-fixed")
        .then(50, sky(150, 0, 0))
        .then(200, sky(10, 0, 0));

    v.sail(FRAMES, FRAMES * 120, &mut |_| {});

    assert!(v.seen.iter().all(Vec::is_empty));
    assert!(v.seats.iter().all(|s| s.sim.peer().delay() == 3));
    v.alike();
    one_game(&v.seats, FRAMES, 0);
}

/// A desync found while a change is in flight: the resync names the
/// delay in force, every peer goes on with it, and the game stays one.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_resync_with_a_change_in_flight_carries_the_delay_in_force() {
    const FRAMES: u32 = 900;

    let mut v = Voyage::new(2, (sky(10, 0, 0), 2, Delay::Auto), "net-steer-resync")
        .then(100, sky(150, 0, 0));
    let mut parted = false;

    v.sail(FRAMES, FRAMES * 80, &mut |seats| {
        if !parted && seats[0].sim.peer().changing() {
            parted = true;
            corrupt(seats, 1);
        }
    });

    assert!(parted, "a change was in flight");
    v.alike();
    one_game(&v.seats, FRAMES, 1);
    assert!(v.seats[0].sim.peer().delay() >= 5);
}

/// A save asked while a change is in flight is one game on every peer,
/// and so is the game after both.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_save_with_a_change_in_flight_is_one_game() {
    const FRAMES: u32 = 700;

    let mut v =
        Voyage::new(2, (sky(10, 0, 0), 2, Delay::Auto), "net-steer-save").then(100, sky(150, 0, 0));
    let mut asked = false;

    v.sail(FRAMES, FRAMES * 80, &mut |seats| {
        if !asked && seats[0].sim.peer().changing() {
            asked = true;
            seats[1].sim.ask_save("Day");
        }
    });

    assert!(asked);

    let saves = written(&mut v.seats);

    assert_eq!(saves[0].meta.frame, saves[1].meta.frame);
    assert_eq!(saves[0].play, saves[1].play);
    assert_eq!(
        regent_core::sim::game::deep_fold(&saves[0].snapshot.game),
        regent_core::sim::game::deep_fold(&saves[1].snapshot.game)
    );
    v.alike();
    one_game(&v.seats, FRAMES, 0);
}
