//! Cheat codes: a phrase typed as a chat line. **Binary** unless marked.
//!
//! ```text
//!  chat line, Enter   ChatController, control 5000
//!    session word set (cheats allowed) ─> chat cheats
//!      the line in capitals (string upper), compared whole with each
//!      GMTX phrase (PHRASES), case kept; a phrase past the table is ""
//!      (empty string), so in a base quest an empty line is PLANET
//!      FARGO
//!      a match: cheat_send(code, arg), a CHEAT_CODE_MESSAGE; the
//!        line becomes the phrase's reply, sent as chat
//!  GSMessageManager, case 0x3f0, before the clock step
//!    cheat apply: switch on the code (Cheat)
//! ```
//!
//! Allowed by the session's word: 1 as the session is made,
//! so in every new game; a loaded game takes its save's
//! (`Save::cheats`). Here [`Cheats`], [`World::cheats`]. A save keeps
//! what a cheat changed — gold, ground, attributes, units — but not the
//! two flags `GIVE ME POWER` and `BUILD ANYTHING` set: the game's start
//! clears both.
//!
//! The engine's two flags are global: one player's cheat is every
//! player's. Here each is per player, so in a game of several humans a
//! cheat is its sender's alone; a player no human seat
//! ([`World::humans`]) keeps the engine's one flag, on while any
//! human's is, so a single-player game plays as the original.

use std::borrow::Cow;

use super::sound::{From, Phase, Voice};
use super::{Dispatch, World, bodies::Class};
use crate::base::digest;
use crate::data::attrib::{CHEAT_WIZ_GUILD_RANGE, EXPERIENCE_LEVEL, HP, Key, MAX_HP};
use crate::data::fourcc::FourCc;
use crate::data::text::{GMTX, Text};
use crate::gpl::ops::Value;
use std::ops::Range;

/// The table the phrases and replies are in.
pub const TABLE: &[u8; 4] = GMTX;

/// The gold `FILL THIS BAG` adds (`0x2710`, cheat apply), by
/// `GS_Player`, as a `PlayerResourceMessage` adds.
pub const GOLD: i32 = 10_000;

/// The levels `GROW UP` gives (5 added, cheat apply).
pub const LEVELS: i32 = 5;

/// `GROW UP`'s script, with the hero and `APV:11` plus [`LEVELS`].
pub const ADVANCE: &str = "advance_to_level";

/// What `RESTORATION` runs on each damaged building with an agent,
/// instead of mending it.
pub const REACHED_MAX: &str = "BuildingReachedMaxHP";

/// `PF66 Planet_fargo`, the sound `PLANET FARGO` plays: phase 0, no place,
/// as the local player's. Only
/// `mx_sounddesc.cam` holds it, so a base quest plays nothing
/// (`GE_SoundManager`: no descriptor). **Game**: none heard there.
pub const FARGO_SOUND: FourCc = FourCc(*b"PF66");

/// A `CHEAT_CODE_MESSAGE`'s code, word: a case of
/// cheat apply. Case 7 does nothing; 1 and 7 no phrase sends.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum Cheat {
    /// [`GOLD`] to the player.
    Gold = 0,
    /// engine flags bit 4, set in the file already: threads run
    /// again after `Halt` ([`super::threads::Gpl`]).
    Flags = 1,
    /// Every vertex explored for the player.
    Reveal = 2,
    /// The player wins (game end won 1).
    Win = 3,
    /// The player loses (game end won 0).
    Lose = 4,
    /// The player's palace `APB:10`: spells reach everywhere.
    Reach = 5,
    /// The player's units mended, buildings [`REACHED_MAX`].
    Restore = 6,
    /// The unit's hit points 0: it dies.
    Kill = 8,
    /// A hero [`LEVELS`] levels up.
    Levels = 9,
    /// Toggles every level flag: every building level counts 3,
    /// and spells reach everywhere.
    Spells = 10,
    /// Sets build cheat: the build menu offers every building.
    Build = 11,
    /// Toggles a frame rate in the day counter's place.
    Frames = 12,
    /// `Cheat_wave_undead`.
    Undead = 13,
    /// `Cheat_wave_raiders`.
    Raiders = 14,
    /// `Cheat_wave_general`.
    General = 15,
    /// `Cheat_wave_goblins`.
    Goblins = 16,
    /// `Cheat_wave_raiders_easy`.
    EasyRaiders = 17,
    /// The player's `APP:1` 1, then team share reveal.
    Share = 18,
    /// The player's `APP:1` 0.
    Alone = 19,
    /// `PF66 Planet_fargo`, by `GE_SoundManager`.
    Fargo = 20,
}

/// Every case of the switch in cheat apply, but 7.
const CHEATS: [Cheat; 20] = [
    Cheat::Gold,
    Cheat::Flags,
    Cheat::Reveal,
    Cheat::Win,
    Cheat::Lose,
    Cheat::Reach,
    Cheat::Restore,
    Cheat::Kill,
    Cheat::Levels,
    Cheat::Spells,
    Cheat::Build,
    Cheat::Frames,
    Cheat::Undead,
    Cheat::Raiders,
    Cheat::General,
    Cheat::Goblins,
    Cheat::EasyRaiders,
    Cheat::Share,
    Cheat::Alone,
    Cheat::Fargo,
];

impl Cheat {
    /// The message's code.
    #[must_use]
    pub const fn code(self) -> u32 {
        self as u32
    }

    /// The cheat of message code `code`.
    #[must_use]
    pub fn of(code: u32) -> Option<Cheat> {
        CHEATS.iter().copied().find(|c| c.code() == code)
    }

    /// The wave script a monster cheat dispatches, with no argument
    /// (strings `"Cheat_wave_raiders_easy"` to `"Cheat_wave_undead"`).
    #[must_use]
    pub const fn wave(self) -> Option<&'static str> {
        match self {
            Cheat::Undead => Some("Cheat_wave_undead"),
            Cheat::Raiders => Some("Cheat_wave_raiders"),
            Cheat::General => Some("Cheat_wave_general"),
            Cheat::Goblins => Some("Cheat_wave_goblins"),
            Cheat::EasyRaiders => Some("Cheat_wave_raiders_easy"),
            _ => None,
        }
    }
}

/// A `CHEAT_CODE_MESSAGE`: the code, and word, the player or a
/// unit ([`Target`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CheatCode {
    pub cheat: Cheat,
    /// The player; for a unit, its agent (the message holds unit,
    /// agent by handle's handle).
    pub arg: u32,
}

/// Whether the session lets the chat send cheats: its word.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Cheats {
    /// 1: every new game.
    #[default]
    Allowed,
    /// 0: a phrase is said as a chat line, and no cheat is sent.
    Barred,
}

/// The session's cheat setting, and the two engine flags cheats set,
/// by the players they are on for; the flags none in a `.gmp`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Flags {
    allowed: Cheats,
    /// every level flag ([`Cheat::Spells`]).
    every_level: digest::Set<u32>,
    /// build cheat ([`Cheat::Build`]).
    build: digest::Set<u32>,
}

impl Flags {
    /// Whether a flag of the players `on` is on for `owner`: its own,
    /// or for a player no human seat of `humans`, any human's.
    fn on(on: &digest::Set<u32>, owner: u32, humans: &Range<u32>) -> bool {
        if humans.contains(&owner) {
            on.contains(&owner)
        } else {
            on.iter().any(|p| humans.contains(p))
        }
    }
}

/// What the chat puts in a message's word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// The local player, app.
    Player,
    /// The selected unit (**Inferred**: a mobile first, else a building;
    /// unit lists 1 then 0 of the game root's); none sends
    /// nothing.
    Selected,
    /// The selected mobile, if a hero.
    Hero,
}

/// A phrase: its `GMTX` index, the cheat it sends, and the reply the
/// chat line becomes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Phrase {
    /// The shipped text, for reference; the game compares
    /// [`TABLE`]`[text]`.
    pub words: &'static str,
    pub text: usize,
    pub reply: usize,
    pub cheat: Cheat,
    pub target: Target,
}

const fn phrase(words: &'static str, text: usize, cheat: Cheat, target: Target) -> Phrase {
    Phrase {
        words,
        text,
        reply: text + 1,
        cheat,
        target,
    }
}

/// The phrases in the order chat cheats tries them. Each reply is
/// the next string. Two more lines are no cheat: `LEAVE ME ALONE` (339)
/// leaves a network game, `PING` times one.
pub const PHRASES: [Phrase; 19] = [
    phrase("VICTORY IS MINE", 43, Cheat::Win, Target::Player),
    phrase("I'M A LOSER BABY", 45, Cheat::Lose, Target::Player),
    phrase("CHEEZY TOWERS", 47, Cheat::Reach, Target::Player),
    phrase("RESTORATION", 49, Cheat::Restore, Target::Player),
    phrase("REVELATION", 51, Cheat::Reveal, Target::Player),
    phrase("FILL THIS BAG", 53, Cheat::Gold, Target::Player),
    phrase("NOW YOU DIE", 55, Cheat::Kill, Target::Selected),
    phrase(
        "NIGHT OF THE LIVING DEAD",
        353,
        Cheat::Undead,
        Target::Player,
    ),
    phrase("PREPARE TO DIE", 355, Cheat::Raiders, Target::Player),
    phrase("PUMP UP THE VOLUME", 357, Cheat::General, Target::Player),
    phrase("GOBLIN RUSH", 359, Cheat::Goblins, Target::Player),
    phrase("GIVE ME ACTION", 361, Cheat::EasyRaiders, Target::Player),
    phrase("ALL YOU NEED IS LOVE", 363, Cheat::Share, Target::Player),
    phrase("I WANT TO BE ALONE", 365, Cheat::Alone, Target::Player),
    phrase("PLANET FARGO", 440, Cheat::Fargo, Target::Player),
    phrase("GROW UP", 78, Cheat::Levels, Target::Hero),
    phrase("BUILD ANYTHING", 59, Cheat::Build, Target::Player),
    phrase("FRAME IT", 61, Cheat::Frames, Target::Player),
    phrase("GIVE ME POWER", 57, Cheat::Spells, Target::Player),
];

/// Gmtx string: string `index` of [`TABLE`]; past the table the
/// engine's empty string. The base
/// game's table has 388 strings, the expansion's 442.
fn gmtx<'a>(text: &Text<'a>, index: usize) -> Cow<'a, str> {
    text.at(TABLE, index).unwrap_or_default()
}

/// The phrase `line` is, as the chat compares: `line` in capitals equal
/// to the phrase's text. `"fill this bag"` is `FILL THIS BAG`. A phrase
/// past the loaded table is "", so with the base game's an empty line is
/// `PLANET FARGO` (440), and the phrase typed is nothing.
#[must_use]
pub fn recognise(text: &Text<'_>, line: &str) -> Option<Phrase> {
    let upper = line.to_ascii_uppercase();

    PHRASES
        .iter()
        .find(|p| gmtx(text, p.text) == upper)
        .copied()
}

/// The line `phrase` becomes, sent as chat: its reply, "" past the table
/// (`PLANET FARGO`'s 441 in the base game).
#[must_use]
pub fn reply(text: &Text<'_>, phrase: &Phrase) -> String {
    gmtx(text, phrase.reply).into_owned()
}

impl World {
    /// The message word the chat sends for `target`: `player`,
    /// or the selection `selected`; `None` sends nothing.
    #[must_use]
    pub fn cheat_arg(&self, target: Target, player: u32, selected: Option<u32>) -> Option<u32> {
        match target {
            Target::Player => Some(player),
            Target::Selected => selected.filter(|id| self.pieces.contains(*id)),
            Target::Hero => selected.filter(|id| self.is_hero(*id)),
        }
    }

    /// Cheat apply but `RESTORATION` ([`World::damaged_buildings`],
    /// [`World::mend_mobiles`]): what changes at once, and the dispatches
    /// to run, in order: a death's, `GROW UP`'s, a wave's. [`Cheat::Fargo`]
    /// is a cue, [`Cheat::Frames`] the view's ([`super::Output::frame_cheats`]).
    pub fn cheat(&mut self, c: &CheatCode) -> Vec<Dispatch> {
        let mut out = Vec::new();

        match c.cheat {
            Cheat::Gold => self.earn(c.arg, GOLD),
            Cheat::Reveal => self.fog.explored_mut().everything(1u8.wrapping_shl(c.arg)),
            Cheat::Win => self.end(c.arg, super::defeat::End::Won, super::defeat::NO_POINT),
            Cheat::Lose => self.end(c.arg, super::defeat::End::Lost, super::defeat::NO_POINT),
            Cheat::Reach => {
                let palace = self.palace_of(c.arg);

                if self.pieces.contains(palace) {
                    self.write(palace, CHEAT_WIZ_GUILD_RANGE.packed(), 1);
                }
            }
            Cheat::Kill => {
                if self.pieces.contains(c.arg) {
                    self.write(c.arg, HP.packed(), 0);
                }
            }
            Cheat::Levels => {
                if self.is_hero(c.arg) {
                    let level = self
                        .pieces
                        .get(c.arg)
                        .map_or(0, |p| at(p, EXPERIENCE_LEVEL))
                        .wrapping_add(LEVELS);

                    out.push((
                        ADVANCE.to_owned(),
                        vec![Value::Agent(c.arg), Value::Integer(level)],
                    ));
                }
            }
            Cheat::Spells => {
                if !self.cheats.every_level.remove(&c.arg) {
                    self.cheats.every_level.insert(c.arg);
                }
            }
            Cheat::Share => {
                self.set_sharing(c.arg, true);
                self.share_team_ground(c.arg);
            }
            Cheat::Alone => self.set_sharing(c.arg, false),
            Cheat::Build => {
                self.cheats.build.insert(c.arg);
            }
            Cheat::Undead
            | Cheat::Raiders
            | Cheat::General
            | Cheat::Goblins
            | Cheat::EasyRaiders => {
                out.extend(c.cheat.wave().map(|w| (w.to_owned(), Vec::new())));
            }
            Cheat::Flags => self.scripts.set_gpl(super::threads::Gpl::Running),
            Cheat::Fargo => self.cue(From::Nowhere, Voice::Sound(FARGO_SOUND), Phase::First, 0.0),
            Cheat::Frames => self.output.frame_cheats = self.output.frame_cheats.wrapping_add(1),
            Cheat::Restore => {}
        }

        let mut dispatches = self.take_queued();

        dispatches.extend(out);
        dispatches
    }

    /// `RESTORATION`'s first pass, cheat restore unit over unit list 0 with
    /// flag 1: `player`'s buildings under their most hit points that
    /// have an agent, each to get [`REACHED_MAX`]; none is mended.
    /// **Inferred**: list order is agent order.
    #[must_use]
    pub fn damaged_buildings(&self, player: u32) -> Vec<u32> {
        self.restorable(player, Class::Building)
            .into_iter()
            .filter(|id| self.agents.alive(*id))
            .collect()
    }

    /// `RESTORATION`'s second pass, over unit list 1 with flag 0: each
    /// of `player`'s mobiles under its most hit points mended to it.
    pub fn mend_mobiles(&mut self, player: u32) {
        for id in self.restorable(player, Class::Character) {
            let most = self.pieces.get(id).map_or(0, |p| at(p, MAX_HP));

            self.write(id, HP.packed(), most);
        }
    }

    /// `player`'s units of `class` whose `APU:0` is under `APU:1`, by
    /// agent: the pieces' hash map has no order of its own.
    fn restorable(&self, player: u32, class: Class) -> Vec<u32> {
        let mut ids: Vec<u32> = self
            .pieces
            .iter()
            .filter(|(id, p)| {
                p.owner == player
                    && self.body_of(**id).is_some_and(|b| b.class == class)
                    && at(p, HP) < at(p, MAX_HP)
            })
            .map(|(id, _)| *id)
            .collect();

        ids.sort_unstable();
        ids
    }

    /// Whether `GIVE ME POWER` is on for `owner`: every building level
    /// is 3.
    #[must_use]
    pub fn every_level(&self, owner: u32) -> bool {
        Flags::on(&self.cheats.every_level, owner, &self.humans())
    }

    /// Whether `BUILD ANYTHING` is on for `owner`.
    #[must_use]
    pub fn build_cheat(&self, owner: u32) -> bool {
        Flags::on(&self.cheats.build, owner, &self.humans())
    }

    /// Whether the session allows cheats.
    #[must_use]
    pub const fn cheats(&self) -> Cheats {
        self.cheats.allowed
    }

    /// The session's cheat setting: a loaded game's is its save's.
    pub const fn set_cheats(&mut self, cheats: Cheats) {
        self.cheats.allowed = cheats;
    }
}

/// A word of `piece`'s, 0 where unset.
fn at(piece: &super::Piece, Key { bank, index }: Key) -> i32 {
    piece.get(Key::new(bank, index)).unwrap_or_default()
}

/// To and from the save model.
mod persist {
    use super::*;
    use crate::save::{self as m, Refused, Saved};
    use std::result::Result;

    impl Saved for Flags {
        type Model = m::Cheats;

        fn save(&self) -> m::Cheats {
            m::Cheats {
                allowed: self.allowed == Cheats::Allowed,
                every_level: self.every_level.iter().copied().collect(),
                build: self.build.iter().copied().collect(),
            }
        }

        fn load(model: m::Cheats) -> Result<Flags, Refused> {
            Ok(Flags {
                allowed: if model.allowed {
                    Cheats::Allowed
                } else {
                    Cheats::Barred
                },
                every_level: model.every_level.into_iter().collect(),
                build: model.build.into_iter().collect(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::world::Agent;
    use crate::sim::world::kind::Kind;
    use crate::sim::world::ruin::tests::{body, standing};

    /// `RESTORATION`'s damaged buildings come in agent order, whatever
    /// order the pieces' hash map hands them in: each round a fresh
    /// world, a fresh hash.
    #[test]
    fn damaged_buildings_come_in_agent_order() {
        const ROUNDS: usize = 16;
        const TEMPLE: [u8; 4] = *b"ABt1";
        const DAMAGED: [u32; 4] = [9, 3, 12, 6];

        for _ in 0..ROUNDS {
            let mut world = World::default();

            world
                .descriptors_mut()
                .add_body(FourCc(TEMPLE), body(Class::Building, 0x30, vec![(0, 0)]));

            for (id, x) in DAMAGED.into_iter().zip(0..) {
                let mut piece = standing(TEMPLE, id, Kind::BUILDING, (x, 0));

                piece.attributes.insert(HP.packed(), 1);
                piece.attributes.insert(MAX_HP.packed(), 2);
                world.pieces.insert(id, piece);
                world.agents.insert(id, Agent::default());
            }

            assert_eq!(world.damaged_buildings(0), vec![3, 6, 9, 12]);
        }
    }

    /// `BUILD ANYTHING` and `GIVE ME POWER` are their sender's; a player
    /// no human seat sees a human's, as the engine's one flag would show
    /// it; `GIVE ME POWER` again turns it off.
    #[test]
    fn the_flag_cheats_are_their_senders() {
        const MONSTERS: u32 = 6;

        let mut world = World::default();
        let send = |world: &mut World, cheat| world.cheat(&CheatCode { cheat, arg: 0 });

        assert!(!world.humans().contains(&MONSTERS));

        send(&mut world, Cheat::Build);
        send(&mut world, Cheat::Spells);

        assert!(world.build_cheat(0) && world.build_cheat(MONSTERS));
        assert!(world.every_level(0) && world.every_level(MONSTERS));

        send(&mut world, Cheat::Spells);

        assert!(!world.every_level(0) && !world.every_level(MONSTERS));

        world.cheats.build = std::iter::once(MONSTERS).collect();

        assert!(!world.build_cheat(0), "no human's");
    }

    /// With two human seats, a seat's flag is its own; a player no seat
    /// sees either's.
    #[test]
    fn a_flag_cheat_is_one_seats_of_two() {
        const MONSTERS: u32 = 7;

        let mut world = World::default();

        world.players = world.players.clone().seated(2);
        world.cheats.build = std::iter::once(1).collect();

        assert!(world.build_cheat(1) && world.build_cheat(MONSTERS));
        assert!(!world.build_cheat(0));
    }

    /// The session's setting and the flags' players go through a save.
    #[test]
    fn the_cheat_setting_and_flags_are_saved() {
        use crate::save::Saved;

        let mut world = World::default();

        assert_eq!(world.cheats(), Cheats::Allowed);

        world.set_cheats(Cheats::Barred);
        world.cheat(&CheatCode {
            cheat: Cheat::Build,
            arg: 0,
        });

        assert_eq!(Flags::load(world.cheats.save()), Ok(world.cheats.clone()));
    }
}
