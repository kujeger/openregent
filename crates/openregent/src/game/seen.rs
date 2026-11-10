//! The game as the interface sees it, and what the interface hands it:
//! the read model between the drawing's systems and [`Sim`], which lives
//! on its own thread (`game::thread`).
//!
//! ```text
//!  Frame::Input  Orders, Outbox ──> input::send ──> Asks queued (inputs, camp, lines, chat,
//!                                                   play on, music, rewards)
//!  Frame::Step   game::clock::tick: the asks, Interest, pause, speed, time ──> sim thread:
//!                  Sim::pump, Sim::step per 50 ms owed, then publishes; tick takes
//!                  the newest state (sync mode: waits for its own)
//!                  Frame    the facts, the motes, the answers to Interest, and
//!                           Taken: the world (a copy), the steps, fog, effects
//!                  News     sounds, shakes, lines and chat, orders' frames: with
//!                           the state, appended until taken
//!                  Snapshot what is shown (draw), mirrored as entities (mirror)
//!  any system    reads Frame, and the world through Watch; pushes to Outbox;
//!                sets Interest (the panels' and preview's aside runs)
//! ```
//!
//! Only the sim thread touches the [`Sim`]; the interface sends it
//! commands (`game::thread::SimThread`). [`Watch`] lends the copy of the
//! world the last published frame holds, read-only.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use regent_core::data::fourcc::FourCc;
use regent_core::gpl::load::Program;
use regent_core::sim::game::Input;
use regent_core::sim::world::{
    World,
    chat::{Line, Said},
    defeat::End,
    flag::Rewards,
    guild::{
        bounty::Term,
        camp::{Camp, CampMove},
        extortion::Extortion,
    },
    offer::{Button, Menu, Placement, Rules},
    quake::Shake,
    sound::Cue,
};
use regent_core::view::map::particles::Mote;

use crate::game::clock::Steps;
use crate::game::sim::{Seat, Sim};
use crate::net::Audience;

/// Follows a spectator's nick in the chat. A placeholder: HD has no
/// such text.
const SPECTATOR: &str = "(spectator)";

/// The game as the last drawn frame's steps left it, and what the
/// interface's changes since made of it.
#[derive(Resource)]
pub struct Frame {
    /// Frames run.
    pub frames: u32,
    /// The world's clock, ms.
    pub now: u32,
    /// This machine's player.
    pub local: u32,
    /// The track this machine plays ([`Sim::music`]).
    pub music: Option<i32>,
    /// The game's peers: 1 alone (`net`).
    pub players: u32,
    /// The LAN players' names, by seat; empty in a solo game.
    pub nicks: Vec<String>,
    /// A LAN guest's host is back in its lobby: the game is left for it
    /// (`net::Peer::regrouped`).
    pub regrouped: bool,
    /// The seats a LAN game holds for (`net::Peer::waiting`).
    pub waiting: Vec<crate::net::Wait>,
    /// The names of the other LAN players holding the game paused.
    pub pausing: Vec<String>,
    /// The LAN players in play, by seat; empty in a solo game.
    pub listed: Vec<Listed>,
    /// The lockstep delay in force on this peer, frames; the player
    /// list shows it (`net::Peer::delay`).
    pub delay: u32,
    /// Whether this peer hosts the game: it may drop a seat waited on.
    pub hosts: bool,
    /// The local player's game ended ([`Sim::ended`]).
    pub ended: bool,
    /// How it ended, as shown ([`Sim::end`]): a spectator's loss waits
    /// for the game's end.
    pub end: Option<End>,
    /// Whether this machine's player plays or surrendered and watches
    /// ([`Sim::seat`]).
    pub seat: Seat,
    /// The LAN resyncs gone on from ([`Sim::resyncs`]): the drawing
    /// starts again at each (`resynced`).
    pub resyncs: u32,
    pub rules: Rules,
    pub program: &'static Program,
    /// The camp move this machine began and has not sent or left.
    pub camp: Option<CampMove>,
    /// The reward panel's settings, as last handed over.
    pub rewards: Rewards,
    /// Each live particle.
    pub motes: Vec<Mote>,
    /// What [`Interest`] asked, answered.
    pub answers: Answers,
    /// The world and what is drawn of it.
    pub taken: Taken,
    /// The last drawn frame's hand-over the game took before this
    /// (`game::thread`).
    pub handed: u64,
}

/// A LAN player in play, as the player list shows it (`ui::players`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listed {
    pub nick: String,
    /// The player it plays: its colour's.
    pub player: u32,
    pub ping: Ping,
}

/// A listed player's round trip from this machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ping {
    /// This machine's own player: none.
    Own,
    /// Not yet answered.
    Unknown,
    Ms(u32),
}

impl Listed {
    /// `sim`'s LAN players in play; none alone. A seat plays its own
    /// player, this machine's the local one, as its orders do.
    fn of(sim: &Sim) -> Vec<Listed> {
        let peer = sim.peer();
        let pings = peer.pings();

        (0..peer.players())
            .filter(|_| peer.players() > 1)
            .filter(|s| !peer.gone(*s))
            .map(|s| Listed {
                nick: peer.nick(s),
                player: if s == peer.seat() { sim.local() } else { s },
                ping: if s == peer.seat() {
                    Ping::Own
                } else {
                    pings
                        .iter()
                        .find(|(seat, _)| *seat == s)
                        .map_or(Ping::Unknown, |(_, p)| {
                            Ping::Ms(u32::try_from(p.as_millis()).unwrap_or(u32::MAX))
                        })
                },
            })
            .collect()
    }
}

impl Frame {
    /// `sim` now, nothing asked yet.
    pub fn of(sim: &Sim) -> Frame {
        Frame::with(sim, sim.motes(), Answers::default(), Taken::of(sim))
    }

    /// `sim` now, with `motes`, `answers` and `taken`.
    pub fn with(sim: &Sim, motes: Vec<Mote>, answers: Answers, taken: Taken) -> Frame {
        Frame {
            frames: sim.frames(),
            now: sim.world().now(),
            local: sim.local(),
            music: sim.music(),
            players: sim.peer().players(),
            nicks: if sim.peer().players() > 1 {
                (0..sim.peer().players())
                    .map(|seat| sim.peer().nick(seat))
                    .collect()
            } else {
                Vec::new()
            },
            regrouped: sim.peer().regrouped(),
            waiting: sim.peer().waiting(),
            pausing: sim
                .peer()
                .pausers()
                .into_iter()
                .filter(|s| *s != sim.peer().seat())
                .map(|s| sim.peer().nick(s))
                .collect(),
            listed: Listed::of(sim),
            delay: sim.peer().delay(),
            hosts: sim.peer().hosts(),
            ended: sim.ended(),
            end: sim.end(),
            seat: sim.seat(),
            resyncs: sim.resyncs(),
            rules: sim.rules(),
            program: sim.program(),
            camp: sim.camp_move(),
            rewards: sim.rewards(),
            motes,
            answers,
            taken,
            handed: 0,
        }
    }
}

/// What the sim thread takes of the world as it changes, for the
/// drawing: a copy of the world, the last two steps' pieces and
/// missiles, the fog and the effects. Each part is shared until it
/// changes; a drawn frame done with them hands them back to the sim
/// thread to free.
#[derive(Clone)]
pub struct Taken {
    pub world: std::sync::Arc<World>,
    pub steps: std::sync::Arc<Steps>,
    pub fog: std::sync::Arc<crate::map::fog::Seen>,
    pub effects: std::sync::Arc<crate::map::effects::Seen>,
}

impl Taken {
    /// `sim`'s world before any step: no pieces taken yet.
    pub fn of(sim: &Sim) -> Taken {
        Taken::of_world(
            std::sync::Arc::new(sim.world().clone()),
            std::sync::Arc::default(),
            sim.local(),
        )
    }

    /// `world` and `steps`, the fog and effects taken of them, the fog
    /// as player `local` sees it.
    pub fn of_world(
        world: std::sync::Arc<World>,
        steps: std::sync::Arc<Steps>,
        local: u32,
    ) -> Taken {
        let fog = crate::map::fog::Seen::default().after(&world, &steps.now, local);

        Taken {
            effects: std::sync::Arc::new(crate::map::effects::Seen::of(&world)),
            fog: std::sync::Arc::new(fog),
            world,
            steps,
        }
    }

    /// These, `world` and `steps` now: the fog and effects taken again,
    /// the fog as player `local` sees it.
    #[must_use]
    pub fn after(
        &self,
        world: std::sync::Arc<World>,
        steps: std::sync::Arc<Steps>,
        local: u32,
    ) -> Taken {
        let fog = crate::perf::timed(crate::perf::Timer::Fog, || {
            self.fog.after(&world, &steps.now, local)
        });

        Taken {
            effects: std::sync::Arc::new(crate::map::effects::Seen::of(&world)),
            fog: std::sync::Arc::new(fog),
            world,
            steps,
        }
    }
}

/// What the game said since the interface last took it: each list
/// grows at the step and is emptied by its one reader.
#[derive(Resource, Default)]
pub struct News {
    /// Credited deaths awaiting profile progress.
    pub lore_kills: Vec<regent_core::sim::world::combat::LoreKill>,
    /// Buildings the engine deselected, by agent (`select::forget`).
    pub deselects: Vec<u32>,
    /// Sounds the world started (`audio`).
    pub cues: Vec<Cue>,
    /// The earthquakes' view shakes (`quake`).
    pub shakes: Vec<Shake>,
    /// How often `FRAME IT` ran (`frame_rate`).
    pub frame_cheats: u32,
    /// The world's and the interface's lines (`chat`).
    pub said: Vec<Said>,
    /// The other peers' chat: who said it, by nick, and the line
    /// (`chat`).
    pub chat: Vec<(String, String)>,
    /// The players who left a LAN game (`chat`).
    pub left: Vec<crate::net::Left>,
    /// A LAN game's resyncs after a desync (`chat`).
    pub notices: Vec<crate::net::Notice>,
    /// Each hand-over with orders in it, and the frame they come
    /// before (`input::choose`).
    pub ordered: Vec<(u64, u32)>,
    /// LAN saves the steps took, to write (`ui::keep`).
    pub saves: Vec<crate::save::Stepped>,
    /// Autosaves the steps took, to write (`ui::keep`).
    pub autosaves: Vec<crate::save::auto::Taken>,
}

impl News {
    /// `sim`'s since the last call, added.
    pub fn gather(&mut self, sim: &mut Sim) {
        let mut output = sim.take_output();

        self.lore_kills.append(&mut output.lore_kills);
        self.deselects.append(&mut output.deselects);
        self.cues.append(&mut output.cues);
        self.shakes.append(&mut output.shakes);
        self.frame_cheats = self.frame_cheats.wrapping_add(output.frame_cheats);
        self.said.append(&mut output.said);
        self.saves.append(&mut sim.take_saves());
        self.autosaves.append(&mut sim.take_autosaves());
        let own = sim.seat();
        let peer = sim.peer_mut();
        let heard = peer.take_chat();

        self.chat.extend(
            heard
                .into_iter()
                .filter_map(|(seat, line, to)| shown((peer.nick(seat), line, to), own)),
        );
        self.left.append(&mut peer.take_left());
        self.notices.append(&mut peer.take_notices());
    }

    /// `more`, added after these.
    pub fn append(&mut self, mut more: News) {
        self.lore_kills.append(&mut more.lore_kills);
        self.deselects.append(&mut more.deselects);
        self.cues.append(&mut more.cues);
        self.shakes.append(&mut more.shakes);
        self.said.append(&mut more.said);
        self.chat.append(&mut more.chat);
        self.left.append(&mut more.left);
        self.notices.append(&mut more.notices);
        self.ordered.append(&mut more.ordered);
        self.saves.append(&mut more.saves);
        self.autosaves.append(&mut more.autosaves);
    }
}

/// A chat line `line` by `nick` for `to`, as this machine's chat shows
/// it from `seat`: a spectator's line reaches the spectators alone, its
/// nick marked.
fn shown((nick, line, to): (String, String, Audience), seat: Seat) -> Option<(String, String)> {
    match to {
        Audience::All => Some((nick, line)),
        Audience::Spectators => {
            (seat == Seat::Spectates).then(|| (format!("{nick} {SPECTATOR}"), line))
        }
    }
}

/// A change the interface hands the game, besides [`crate::input::orders::Orders`]'.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ask {
    /// An order for the next frame ([`Sim::input`]).
    Input(Input),
    /// The interface's own line ([`Sim::say`]).
    Say(Line),
    /// A camp move sent to a fitting spot ([`Sim::camp_place`]).
    CampPlace(Camp),
    /// A chat line to the other peers.
    Chat(String),
    /// The end panel's CONTINUE PLAYING ([`Sim::play_on`]).
    PlayOn,
    /// A music track ended ([`Sim::music_stopped`]).
    MusicStopped,
    /// A camp move begun: the guild and the panel's price
    /// ([`Sim::camp_begin`]).
    CampBegin(u32, i32),
    /// The camp move left, if one was begun ([`Sim::camp_leave`]).
    CampLeave,
    /// The reward panel's settings ([`Sim::set_rewards`]).
    Rewards(Rewards),
    /// A LAN game's save, by this name, asked of every peer
    /// ([`Sim::ask_save`]).
    Save(String),
    /// Game days between autosaves, `None` off ([`Sim::set_autosave`]).
    Autosave(Option<u32>),
    /// The host drops a LAN seat (`net::Peer::drop_player`).
    DropPlayer(u32),
    /// The local player surrenders ([`Sim::surrender`]).
    Surrender,
}

impl Ask {
    /// This change, to `sim`.
    pub fn hand(self, sim: &mut Sim) {
        match self {
            Ask::Input(input) => sim.input(input),
            Ask::Say(line) => sim.say(line),
            Ask::CampPlace(camp) => {
                for input in sim.camp_place(camp) {
                    sim.input(input);
                }
            }
            Ask::Chat(line) => sim.chat(&line),
            Ask::PlayOn => sim.play_on(),
            Ask::MusicStopped => sim.music_stopped(),
            Ask::CampBegin(guild, price) => sim.camp_begin(guild, price),
            Ask::CampLeave => sim.camp_leave(),
            Ask::Rewards(rewards) => sim.set_rewards(rewards),
            Ask::Save(name) => sim.ask_save(&name),
            Ask::Autosave(days) => sim.set_autosave(days),
            Ask::DropPlayer(seat) => sim.peer_mut().drop_player(seat),
            Ask::Surrender => sim.surrender(),
        }
    }

    /// Whether this change is an order for the next frame.
    pub const fn orders(&self) -> bool {
        matches!(self, Ask::Input(_))
    }
}

/// Changes for the game, queued in order by `input::send`.
#[derive(Resource, Default)]
pub struct Outbox(pub Vec<Ask>);

impl Outbox {
    pub fn push(&mut self, ask: Ask) {
        self.0.push(ask);
    }
}

/// A panel's interface run, by the selected piece it asks of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Query {
    /// A Mausoleum's dead, priced ([`Sim::crypt`]).
    Crypt(u32),
    /// A palace's supported henchmen ([`Sim::henchmen`]).
    Henchmen(u32),
    /// A rogues' guild's extortion ([`Sim::extortion`]).
    Extortion(u32),
    /// A Hall of Champions' bounties ([`Sim::bounties`]).
    Bounties(u32),
}

/// A build preview: its code and half cell, 32nds.
pub type Spot = (FourCc, (i32, i32));

/// What the interface wants run aside: the selection's panel, and the
/// build preview.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Interest {
    pub panel: Option<Query>,
    pub preview: Option<Spot>,
}

/// A [`Query`]'s answer.
#[derive(Clone, Debug, PartialEq)]
enum Answer {
    Crypt(Vec<Button<u32>>),
    Henchmen([Option<i32>; 3]),
    Extortion(Option<Extortion>),
    Bounties([Option<Button<Term>>; 2]),
}

/// Game frames a panel's answer stands before its query runs again:
/// each run is aside ([`Sim::henchmen`]), 0.3 to 0.6 ms on a 512-cell
/// map late in a game. Cosmetic: a count or a price shows up to 1 s
/// late. Not the engine's, which asks as it draws (**Inferred**).
const REASK: u32 = 20;

/// The answers to [`Interest`], each with what it was asked at: a
/// query runs again only once that changes, a panel's [`REASK`] frames
/// on. A panel shows its answer only for its own piece: nothing until
/// it comes.
#[derive(Clone, Debug, Default)]
pub struct Answers {
    /// The query, the frame, and its answer.
    panel: Option<(Query, u32, Answer)>,
    /// The preview, the frame and the orders held, and its placement.
    preview: Option<(Spot, (u32, usize), Placement)>,
}

impl Answers {
    /// These, with what `interest` asks of `sim` now that they do not
    /// hold. `menu` is the build menu's data.
    pub fn asked(self, sim: &mut Sim, interest: &Interest, menu: &Menu) -> Answers {
        let frame = sim.frames();
        let panel = interest.panel.map(|query| match self.panel {
            Some((asked, at, answer)) if asked == query && (at..at + REASK).contains(&frame) => {
                (asked, at, answer)
            }
            _ => (query, frame, run(sim, query)),
        });
        let when = (frame, sim.held());
        let preview = interest.preview.map(|want| match self.preview {
            Some((asked, at, placement)) if (asked, at) == (want, when) => (asked, at, placement),
            _ => (want, when, sim.placement(menu, want.0, want.1)),
        });

        Answers { panel, preview }
    }

    fn panel(&self, query: Query) -> Option<&Answer> {
        self.panel
            .as_ref()
            .filter(|(asked, ..)| *asked == query)
            .map(|(.., answer)| answer)
    }

    /// The Mausoleum `agent`'s dead, priced; empty until answered.
    pub fn crypt(&self, agent: u32) -> &[Button<u32>] {
        match self.panel(Query::Crypt(agent)) {
            Some(Answer::Crypt(rows)) => rows,
            _ => &[],
        }
    }

    /// The palace `agent`'s supported henchmen; none until answered.
    pub fn henchmen(&self, agent: u32) -> [Option<i32>; 3] {
        match self.panel(Query::Henchmen(agent)) {
            Some(Answer::Henchmen(counts)) => *counts,
            _ => [None; 3],
        }
    }

    /// The rogues' guild `agent`'s extortion; none until answered.
    pub fn extortion(&self, agent: u32) -> Option<Extortion> {
        match self.panel(Query::Extortion(agent)) {
            Some(Answer::Extortion(extortion)) => *extortion,
            _ => None,
        }
    }

    /// The Hall of Champions `agent`'s bounties; none until answered.
    pub fn bounties(&self, agent: u32) -> [Option<Button<Term>>; 2] {
        match self.panel(Query::Bounties(agent)) {
            Some(Answer::Bounties(terms)) => *terms,
            _ => [None; 2],
        }
    }

    /// The local player's `code` at `at`, 32nds; `None` until answered.
    pub fn placement(&self, code: FourCc, at: (i32, i32)) -> Option<Placement> {
        self.preview
            .filter(|(asked, ..)| *asked == (code, at))
            .map(|(.., placement)| placement)
    }
}

/// `query`'s run.
fn run(sim: &mut Sim, query: Query) -> Answer {
    let asked = crate::perf::start();
    let answer = match query {
        Query::Crypt(agent) => Answer::Crypt(sim.crypt(agent)),
        Query::Henchmen(agent) => Answer::Henchmen(sim.henchmen(agent)),
        Query::Extortion(agent) => Answer::Extortion(sim.extortion(agent)),
        Query::Bounties(_) => Answer::Bounties(sim.bounties()),
    };

    crate::perf::stop(crate::perf::Timer::Asked, asked);
    answer
}

/// The world as the last drawn frame's steps left it, read-only, with
/// the [`Frame`]: what the interface's systems read of the game.
#[derive(SystemParam)]
pub struct Watch<'w> {
    frame: Res<'w, Frame>,
}

impl Watch<'_> {
    /// The copy of the world the frame was published with.
    pub fn world(&self) -> &World {
        &self.frame.taken.world
    }
}

impl std::ops::Deref for Watch<'_> {
    type Target = Frame;

    fn deref(&self) -> &Frame {
        &self.frame
    }
}

/// The news, the outbox and the interest, a game's.
pub fn plugin(app: &mut App) {
    use crate::game::scope::Scoped as _;

    app.per_game::<News>()
        .per_game::<Outbox>()
        .per_game::<Interest>()
        .made_by_game::<Frame>();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A spectator's line shows to a spectator, its nick marked, and to
    /// no player; every other line to all.
    #[test]
    fn a_spectators_line_shows_to_spectators_alone() {
        let line = |to| ("Ann".to_owned(), "gg".to_owned(), to);

        assert_eq!(shown(line(Audience::Spectators), Seat::Plays), None);
        assert_eq!(
            shown(line(Audience::Spectators), Seat::Spectates),
            Some(("Ann (spectator)".to_owned(), "gg".to_owned()))
        );
        assert_eq!(
            shown(line(Audience::All), Seat::Plays),
            Some(("Ann".to_owned(), "gg".to_owned()))
        );
        assert_eq!(
            shown(line(Audience::All), Seat::Spectates),
            Some(("Ann".to_owned(), "gg".to_owned()))
        );
    }

    /// What the step answers is what the game says when asked straight:
    /// the palace's henchmen and a preview's placement, each for what
    /// was asked alone.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn the_answers_are_the_games_and_only_for_what_was_asked() {
        let (begun, catalogue) = crate::game::begin::test_game(&[]);
        let mut sim = begun.sim;
        let menu = Menu::of(&catalogue);

        sim.step();

        let (world, local) = (sim.world(), sim.local());
        let palace = world.player_palace(local).expect("a palace");
        let at = world.position(palace).expect("placed");
        let code = world
            .build_menu(&menu, local)
            .iter()
            .find(|o| o.offered)
            .map(|o| o.code)
            .expect("something to build");
        let interest = Interest {
            panel: Some(Query::Henchmen(palace)),
            preview: Some((code, at)),
        };
        let answers = &Answers::default().asked(&mut sim, &interest, &menu);

        assert_eq!(answers.henchmen(palace), sim.henchmen(palace));
        assert_eq!(answers.henchmen(palace + 1), [None; 3]);
        assert_eq!(answers.crypt(palace).len(), 0);
        assert_eq!(
            answers.placement(code, at),
            Some(sim.placement(&menu, code, at))
        );
        assert_eq!(answers.placement(code, (at.0 + 32, at.1)), None);
    }

    /// A panel's query runs again only `REASK` frames on, or once the
    /// query changes.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_panels_answer_stands_for_reask_frames() {
        let (begun, catalogue) = crate::game::begin::test_game(&[]);
        let mut sim = begun.sim;
        let menu = Menu::of(&catalogue);

        sim.step();

        let (world, local) = (sim.world(), sim.local());
        let palace = world.player_palace(local).expect("a palace");
        let mut interest = Interest {
            panel: Some(Query::Henchmen(palace)),
            preview: None,
        };
        let mut answers = Answers::default();
        let asked_at = |answers: &Answers| answers.panel.as_ref().map(|(_, at, _)| *at);

        answers = answers.asked(&mut sim, &interest, &menu);

        let first = sim.frames();

        for _ in 1..REASK {
            sim.step();
            answers = answers.asked(&mut sim, &interest, &menu);
            assert_eq!(asked_at(&answers), Some(first));
        }

        sim.step();
        answers = answers.asked(&mut sim, &interest, &menu);
        assert_eq!(asked_at(&answers), Some(first + REASK));

        interest.panel = Some(Query::Crypt(palace));
        answers = answers.asked(&mut sim, &interest, &menu);
        assert_eq!(
            answers.panel.as_ref().map(|(q, at, _)| (*q, *at)),
            Some((Query::Crypt(palace), first + REASK))
        );
    }
}
