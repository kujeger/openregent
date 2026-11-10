//! A recruit's decode, before its hero comes: recruit decode.
//! **Binary** unless marked.
//!
//! ```text
//!  frame f  CREATE_VEHICLE_MESSAGE, flags bit 1, on the guild:
//!    nothing while APB:13 is set; the hero's Cost against APP:0, taken
//!    (GE_Player); APB:14 the code, APB:15 the message's flags (3),
//!    APB:12 a name drawn from the hero's NameGenType,
//!    APB:56 the clock, APB:13 the recruit delay; a timed order,
//!    callback 0x2006
//!  the delay later  recruit done: the hero (world::Recruit), AP:1
//!    the guild's APB:12 (agent create placed), no draw of its own;
//!    APB:14 and APB:15 removed, APB:12 kept
//! ```
//!
//! A full guild (`APB:0` `MaxGuildMembers`) counts its roster (relation
//! 100), henchmen aside (unit class of, subtype `henchman`): no recruit
//! while the living fill it; paid, with the living and the
//! dead (`GenUnit`, bit `0x10`) filling it, the dead one of
//! lowest `ExperienceLevel`, then `Experience`, first in
//! the roster on a tie, has `Unit_Dismiss_Gravestone_Fast` dispatched.
//! **Game**: play5's last two recruits, at 4908 and 5280,
//! dismiss rangers 41's and 45's graves.
//!
//! So the price is paid at the decode, the delay before the hero: a hero
//! still to come at a save is paid for. **Game**: play3's last ranger,
//! recruited in frame 3492, is not out at the save; the treasury is the
//! save's with its 350 taken. `APB:13` reads 0 on a guild whose hero came
//! (**Inferred**: cleared as the order ends; the world clears it with the
//! hero). **Game**: play3's generators are its save's `NMGM`, its last
//! ranger's name drawn and in the guild's `APB:12`, the hero not out.

use super::chat::{Line, NO_GOLD_UNIT};
use super::timed::Timer;
use super::{BUTTON_DELAY_TIME, World, recruit::Recruit};
use crate::data::attrib::{
    BUTTON_START_TIME, EXPERIENCE_LEVEL, Key, MAX_GUILD_MEMBERS, PENDING_UNIT_NAME, RECRUIT_TYPE,
};
use crate::data::fourcc::FourCc;
use crate::sim::world::piece_state;

/// A recruit's decode: the guild by agent id, the hero's descriptor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hire {
    pub guild: u32,
    pub code: FourCc,
}

/// `APB:15`: the message's flags; a recruit's have bit 1.
const RECRUIT_FLAGS: Key = Key::new(*b"APB", 15);

/// A recruit message's flags word, which recruit decode
/// writes to `APB:15` as passed: bit 1 makes it a recruit;
/// the front end sends bit 0 too
/// (**Game**: play3's save, its last ranger pending, holds 3; bit 0's
/// meaning is not read).
const RECRUIT_SENT: i32 = 3;

/// `APV:16`, `Experience`: with [`EXPERIENCE_LEVEL`], which grave goes
/// first, least of each.
const EXPERIENCE: Key = crate::data::attrib::EXPERIENCE;

/// What a full guild's recruit dispatches on the grave it dismisses: the
/// string `"Unit_Dismiss_Gravestone_fast"`.
pub const DISMISS: &str = "Unit_Dismiss_Gravestone_Fast";

/// A guild's heroes against its spots, as the engine counts them.
///
/// Only the living fill a spot: the recruit button, its tooltip (`GMTX`
/// 9, "(%d of %d)", living + 1 of `APB:0`, guild recruit tip) and a unit
/// list's row (`GMTX` 217, "%d of %d Members", unit list row text) count them
/// alone. A grave stays on the roster until dismissed or raised; a
/// recruit at a guild full of living and dead dismisses one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Members {
    /// Standing roster members, henchmen aside.
    pub living: usize,
    /// Members lying dead.
    pub dead: usize,
    /// `APB:0` `MaxGuildMembers`: the descriptor's, rewritten by an
    /// upgrade or a script (the Legendary Heroes quest sets 1).
    pub spots: i32,
}

/// The agent member unit class of reads, and the value it leaves out of a
/// guild's count.
const SUBTYPE: &str = "subtype";
const HENCHMAN: &str = "henchman";

/// Whether agent `a`'s `subtype` member is `which`, as unit class of
/// classes it.
pub(super) fn is_subtype(a: &super::Agent, which: &str) -> bool {
    a.values.iter().any(|m| {
        m.name.eq_ignore_ascii_case(SUBTYPE)
            && matches!(&m.value, crate::gpl::ops::Value::String(s) if s.eq_ignore_ascii_case(which))
    })
}

/// The heroes due this frame.
#[derive(Clone, Debug, Default)]
pub(super) struct Hires {
    /// Heroes whose recruit's timer ended this frame, until
    /// [`World::arrived`] hands them out.
    arrived: Vec<Recruit>,
}

impl Hires {
    /// Recruit `r`'s hero is due.
    pub(super) fn arrive(&mut self, r: Recruit) {
        self.arrived.push(r);
    }
}

impl World {
    /// The recruits whose hero is due: recruit done, its guild's
    /// timer ended in the guild's turn (`super::timed`).
    pub fn arrived(&mut self) -> Vec<Recruit> {
        std::mem::take(&mut self.pending.hires.arrived)
    }

    /// Decode `h` now, at this frame's clock, as a message is: the grave
    /// a full guild dismisses, by agent, for [`DISMISS`].
    pub fn hire_now(&mut self, h: Hire) -> Option<u32> {
        self.hire(h)
    }

    /// Whether agent `id` is a henchman: unit class of's class 2, its
    /// `subtype` member `henchman`.
    pub fn henchman(&self, id: u32) -> bool {
        self.agents.get(id).is_some_and(|a| is_subtype(a, HENCHMAN))
    }

    /// Guild `guild`'s roster, henchmen aside, by agent id (**Inferred**:
    /// the roster's order is the agents'), each with whether it is dead.
    pub fn roster(&self, guild: u32) -> Vec<(u32, bool)> {
        let Some(handle) = self.pieces.handle(guild) else {
            return Vec::new();
        };
        let mut out: Vec<(u32, bool)> = self
            .relations
            .homes()
            .filter(|(_, parents)| parents.contains(&handle))
            .filter_map(|(child, _)| self.holder(child))
            .filter(|id| !self.henchman(*id))
            .map(|id| {
                let dead = self
                    .pieces
                    .place(id)
                    .is_some_and(|p| p.state & piece_state::RUINED != 0);

                (id, dead)
            })
            .collect();

        out.sort_unstable();
        out
    }

    /// `MaxGuildMembers` of `guild`; none where it has none.
    fn max_members(&self, guild: u32) -> Option<i32> {
        self.pieces.get(guild)?.get(MAX_GUILD_MEMBERS)
    }

    /// Guild `guild`'s [`Members`]; none for a building not `IsGuild`, as
    /// guild max members gives none.
    #[must_use]
    pub fn members(&self, guild: u32) -> Option<Members> {
        let is_guild = self.piece_body(guild).is_some_and(|b| b.guild);
        let roster = self.roster(guild);
        let dead = roster.iter().filter(|(_, dead)| *dead).count();

        self.max_members(guild)
            .filter(|_| is_guild)
            .map(|spots| Members {
                living: roster.len() - dead,
                dead,
                spots,
            })
    }

    /// The grave a full guild's recruit dismisses: guild grave pick's pick.
    fn grave(&self, guild: u32) -> Option<u32> {
        let roster = self.roster(guild);
        let full = self
            .max_members(guild)
            .is_some_and(|max| usize::try_from(max).is_ok_and(|m| m <= roster.len()));
        let word = |id: u32, k| self.word_of(id, k);

        roster
            .iter()
            .filter(|(_, dead)| full && *dead)
            .map(|(id, _)| (word(*id, EXPERIENCE_LEVEL), word(*id, EXPERIENCE), *id))
            .min_by_key(|(level, experience, _)| (*level, *experience))
            .map(|(.., id)| id)
    }

    /// Whether `guild`'s living fill it: guild full, no recruit.
    pub(super) fn filled(&self, guild: u32) -> bool {
        let living = self.roster(guild).iter().filter(|(_, dead)| !dead).count();

        self.max_members(guild)
            .is_some_and(|max| usize::try_from(max).is_ok_and(|m| m <= living))
    }

    /// Recruit decode for `h`: the grave it dismisses, if any.
    fn hire(&mut self, h: Hire) -> Option<u32> {
        let guild = self
            .pieces
            .get(h.guild)
            .filter(|g| g.word(BUTTON_DELAY_TIME) == 0)
            .filter(|_| !self.filled(h.guild))
            .map(|g| g.owner);
        let mut grave = None;
        let body = self
            .descriptors
            .body(h.code)
            .map(|b| (b.cost, b.delay, b.names));

        let paid = guild
            .zip(body)
            .map(|(owner, (cost, ..))| self.spend(owner, cost));

        // A short treasury: a chat line, nothing more.
        if let (Some(owner), Some(false)) = (guild, paid) {
            self.say(owner, Line::Text(NO_GOLD_UNIT));
        }

        if let (Some(owner), Some((cost, delay, names)), Some(true)) = (guild, body, paid) {
            let now = self.now.cast_signed();

            // Stat hired: counted as bought at the decode.
            self.tally(owner, h.code, super::numbered::Tally::Bought);
            self.count_hired(owner, cost);

            grave = self.grave(h.guild);

            let code = h.code.word().cast_signed();
            let name = names.and_then(|w| self.naming.next(w));

            self.write(h.guild, RECRUIT_TYPE.packed(), code);
            self.write(h.guild, RECRUIT_FLAGS.packed(), RECRUIT_SENT);

            if let Some(n) = name {
                self.write(h.guild, PENDING_UNIT_NAME.packed(), n.cast_signed());
            }

            self.write(h.guild, BUTTON_START_TIME.packed(), now);
            self.write(h.guild, BUTTON_DELAY_TIME.packed(), delay);

            // `GS_OrderProcessor`: the recruit's timer.
            let recruit = Recruit {
                guild: h.guild,
                code: h.code,
            };

            self.time(h.guild, Timer::Recruit(recruit), delay.cast_unsigned());
        }

        grave
    }

    /// Recruit done's words on the guild: the recruit's removed, the
    /// delay 0.
    pub(super) fn hired(&mut self, guild: u32) {
        if let Some(g) = self.pieces.get_mut(guild) {
            g.attributes.remove(&RECRUIT_TYPE.packed());
            g.attributes.remove(&RECRUIT_FLAGS.packed());
            g.attributes.insert(BUTTON_DELAY_TIME.packed(), 0);
        }
    }
}
