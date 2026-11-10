//! Game messages: lines the engine and the scripts put in the chat area.
//! The world records them, changing nothing. **Binary** unless marked.
//!
//! ```text
//!  a game event                       the line            to
//!  a recruit done                     GMTX 10, title      the guild's owner
//!  research done                      "%s %s : %s"        the building's owner
//!  a short treasury, at the decode    GMTX 31 to 41, 262  the payer
//!  a magic mirror spell               GMTX 42             the caster
//!  LocalChatMessage                   AITX[n], a name     argument 1
//!        └─ chat_colour(1, 0x8000ff00, 0xff): green
//!           chat_post(text, player, 1): shown where player is -1 or
//!           the local player (app), a line end appended
//! ```
//!
//! Every engine site posts the same green and plays no sound. How the
//! chat area shows a line: [`crate::view::panel::chat`].

use super::World;
use super::naming::NAME;
use crate::data::attrib::NAME_GEN_TABLE;
use crate::data::fourcc::FourCc;
use crate::gpl::ops::Value;
use crate::gpl::vm::{Fault, Handle, Vm};

/// `GMTX` string 10, `Recruited: %s`: recruit done's line.
pub const RECRUITED: usize = 10;

/// `GMTX` string 188, `Research Complete`.
pub const RESEARCH_COMPLETE: usize = 188;

/// Research done callback's format (`"%s %s: %s\r"`): the building's
/// own name, [`RESEARCH_COMPLETE`], the research's row text. Its line end
/// is the format's; the post adds none.
pub const RESEARCHED_FORMAT: &str = "%s %s : %s";

/// `GMTX` 31, `Can't afford unit.`: a recruit's decode.
pub const NO_GOLD_UNIT: usize = 31;

/// `GMTX` 32, `Can't afford building.` (agent create placed b,
/// agent create placed b).
pub const NO_GOLD_BUILDING: usize = 32;

/// `GMTX` 34, `Can't afford spell.` (player spell apply,
/// player spell apply).
pub const NO_GOLD_SPELL: usize = 34;

/// `GMTX` 40, `Out of range.`: a spell click out of a wizards' guild's or tower's reach
/// (`0x28`, spell click; [`World::spell_refusal`]). The interface's line, not
/// a decode's.
pub const OUT_OF_REACH: usize = 40;

/// `GMTX` 41, `Can't afford to enchant tower.`.
pub const NO_GOLD_ENCHANT: usize = 41;

/// `GMTX` 42, `The magic mirror nullifies your spell.`.
pub const MIRRORED: usize = 42;

/// `GMTX` 262, `Can't afford research.` (`GS_OrderProcessor`,
/// `GS_OrderProcessor`).
pub const NO_GOLD_RESEARCH: usize = 262;

/// Chat post's player for everyone.
const EVERYONE: i32 = -1;

/// A unit as a line names it, read as the line is said: its descriptor,
/// `AP:1` and `APU:4`, its number ("Wizards Tower #2").
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Who {
    pub code: FourCc,
    /// `AP:1`, the packed name, where set and not 0.
    pub name: Option<u32>,
    /// `APU:4`, the generator a name is spelled with, where set.
    pub table: Option<i32>,
    pub number: Option<i32>,
    /// What `SpecifyName` gave it, where it did.
    pub given: Option<super::naming::Given>,
}

/// What a line says, its text not yet looked up.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Line {
    /// A `GMTX` string as it stands.
    Text(usize),
    /// [`RECRUITED`] with the hero's descriptor's `HEAD` title.
    Recruited(FourCc),
    /// [`RESEARCHED_FORMAT`]: the building, and its research's `GMTX` row.
    Researched { building: Who, text: usize },
    /// `LocalChatMessage`: `AITX[text]`, its `%s` the unit's name; no unit,
    /// empty.
    Script { text: i32, unit: Option<Who> },
    /// The rogues' guild's press: [`super::guild::extortion::EXTORTED`] of the player's
    /// share and the guild's.
    Extorted { amount: i32, kept: i32 },
    /// A `GMTX` string, its `%d` `n`: the name box's refusal
    /// ([`super::naming::RENAME_REFUSED`] of the level).
    Counted { text: usize, n: i32 },
}

/// The unit a line is about, as it was when said: the recruited hero,
/// the researching building, a script's unit. Not the engine's: a front
/// end's way to the event. No rule reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineSubject {
    pub id: u32,
    pub code: FourCc,
    /// Its place then, 32nds ([`World::position`]): a dead hero's last.
    pub at: Option<(i32, i32)>,
}

/// A line said: when, to whom, what, and about whom.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Said {
    /// Game ms.
    pub at: u32,
    /// The one player who sees it; `None`, everyone.
    pub to: Option<u32>,
    pub line: Line,
    /// The unit it is about, where there is one.
    pub from: Option<LineSubject>,
}

impl Said {
    /// Whether player `local` sees it: chat post's test.
    #[must_use]
    pub fn seen_by(&self, local: u32) -> bool {
        self.to.is_none_or(|p| p == local)
    }
}

impl World {
    /// `line` said now to player `to`.
    pub(super) fn say(&mut self, to: u32, line: Line) {
        let at = self.now;

        self.output.said.push(Said {
            at,
            to: Some(to),
            line,
            from: None,
        });
    }

    /// `line` said now to player `to`, about agent `id`; its place in
    /// [`super::Output::said`].
    pub(super) fn say_of(&mut self, to: u32, line: Line, id: Option<u32>) -> usize {
        let n = self.output.said.len();

        self.say(to, line);

        if let Some(id) = id {
            self.said_of(n, id);
        }

        n
    }

    /// Line `n` of [`super::Output::said`] about agent `id`: for a line said
    /// before its unit stands, as a recruit's.
    pub(super) fn said_of(&mut self, n: usize, id: u32) {
        let from = self.source(id);

        if let Some(said) = self.output.said.get_mut(n) {
            said.from = from;
        }
    }

    /// Agent `id` as a line's [`LineSubject`], where it has a unit.
    fn source(&self, id: u32) -> Option<LineSubject> {
        self.unit(id).map(|piece| LineSubject {
            id,
            code: piece.code,
            at: self.position(id),
        })
    }

    /// Agent `id` as a line names it, where it has a unit.
    pub(super) fn who(&self, id: u32) -> Option<Who> {
        let piece = self.unit(id)?;
        let word = |k| piece.get(k);

        Some(Who {
            code: piece.code,
            name: word(NAME).filter(|n| *n != 0).map(i32::cast_unsigned),
            table: word(NAME_GEN_TABLE),
            number: self.numbered(id),
            given: self.given_name(id).cloned(),
        })
    }

    /// `LocalChatMessage(player, text[, agent])`, gpl local chat message: returns
    /// nothing. A text given as a string names a keyed `AITX` string;
    /// no shipped script gives one, so it is not
    /// modelled.
    pub(super) fn local_chat(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<(), Fault> {
        let value = |i: usize| args.get(i).and_then(|h| vm.value(*h)).cloned();
        let player = super::int_ref(vm, args, 1)?;
        let agent = match value(3) {
            Some(Value::Agent(id)) => Some(id),
            _ => None,
        };
        let unit = agent.and_then(|id| self.who(id));
        let from = agent.and_then(|id| self.source(id));

        if let Some(Value::Integer(text)) = value(2) {
            let at = self.now;
            let to = (player != EVERYONE).then_some(player.cast_unsigned());

            self.output.said.push(Said {
                at,
                to,
                line: Line::Script { text, unit },
                from,
            });
        }

        Ok(())
    }
}
