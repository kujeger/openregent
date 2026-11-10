//! Message flags: what `MessageFlag` makes, `IsMessageFlagPresent` finds
//! and a double click reads. **Binary** unless marked.
//!
//! ```text
//!  MessageFlag(agent | coord, [message])
//!    an agent: its unit's place; no unit throws "MessageFlag() unit is null"
//!    the message: an integer, or a string as a number
//!  making the flag
//!    GS_Level: unit kind MSGF, descriptor ARE1 message_flag, owner
//!      -1, at the point, the next handle; no place search, no agent
//!      (unit agent new: descriptor kind 4, unit kind not 4)
//!    an agent given: relation 1, GSUnit: hung on it, as a flag
//!    set 0x40, then the flag wave order: set 300, the minimap's
//!    APM:0 the message, unless 0
//!    option "message mini view" on: mini view 1 follows it, unless it follows a
//!      message flag already
//!    ME02 New_Message, phase Begin, unplaced, the local player's
//!  selected   EventMessageController: control 5000 AITX[APM:0]
//!  read       a double click sends DestroyEventFlagMessage (0x3f5); its
//!             decode deletes the unit at once (GSUnit)
//!  its holder deleted: it goes too (relation member drop)
//!  IsMessageFlagPresent(message)   any MSGF unit's APM:0 equal
//! ```
//!
//! The tracking window's follow is the interface's (`view::panel::tracking`). The
//! wave order's `ORDR` counter is [`World::message_counter`]; a load
//! dates the flag by it. Not modelled: the flag's place in list ORDR
//! (first in the Urban Renewal save's `ORLS`), as its counter's run
//! changes nothing a script reads.

use super::effectors::NEARER;
use super::flag::{MESSAGE_FLAG, Twice};
use super::sound::BEGIN;
use super::stack::stack_offset;
use super::{
    Answer, Found, World, agent_ref,
    effectors::{Effect, LOOP_SET},
    put_bool,
    walking::FIRST_FACING,
};
use crate::base::fixed::Fixed;
use crate::data::attrib::Key;
use crate::data::fourcc::FourCc;
use crate::gpl::ops::Value;
use crate::gpl::vm::{Fault, Handle, Vm};
use crate::sim::movement::action::Look;
use crate::sim::space;

/// The sound a new message flag plays: `ME02 New_Message`.
pub const NEW_MESSAGE: FourCc = FourCc(*b"ME02");

/// `APM:0`, `Message_Text`: a message flag's message, an `AITX` index.
pub const MESSAGE: Key = Key::new(*b"APM", 0);

/// A holder's handle, 0 for none, and a point, 32nds.
type Spot = (u32, (i32, i32));

/// A message flag standing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Message {
    /// Its unit handle.
    pub handle: u32,
    /// [`MESSAGE`]: `STRT AITX`'s index; 0 where none was given.
    pub text: i32,
    /// The handle of the unit it hangs on; 0 for none.
    pub holder: u32,
    /// 16.16 cells, x, y and height: about its holder while hung, else
    /// where it stands.
    pub at: [Fixed; 3],
    /// Game ms it was made: its set plays from here.
    pub made: u32,
}

/// A message argument as gpl message number reads it: an integer as given;
/// a string by string id's rules, the same code
/// ([`crate::data::fourcc::string_id`]): empty 0, four bytes a FOURCC,
/// opening with a digit its `atoi`, else its CRC. So a Text id names its
/// string: The Wild's `SpecifyName(unit, "HOOL")` gives "Hooligan"
/// ([`super::naming::Given`]).
pub(super) fn number(value: &Value) -> Option<i32> {
    match value {
        Value::Integer(n) => Some(*n),
        Value::String(s) => Some(crate::data::fourcc::string_id(s)),
        _ => None,
    }
}

/// Message flags made, in order.
#[derive(Clone, Debug, Default)]
pub(super) struct Messages(Vec<Message>);

impl World {
    /// `MessageFlag(agent | coord, [message])`: a message flag at the
    /// coord, or hung on the agent's unit.
    pub(super) fn message_flag(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let coord = if let Value::Coord(x, y) = vm.object(vm.arg(args, 1)?) {
            Some((*x, *y))
        } else {
            None
        };
        let spot = match coord {
            Some(at) => Some((0, at)),
            None => self.unit_spot(agent_ref(vm, args, 1)?)?,
        };
        let text = if args.len() > 2 {
            number(vm.object(vm.arg(args, 2)?))
        } else {
            Some(0)
        };

        match (spot, text) {
            (Some((holder, at)), Some(text)) => {
                self.raise_message(holder, at, text);
                Ok(Answer::Given)
            }
            _ => Ok(Answer::Unknown),
        }
    }

    /// Agent `id`'s unit's handle and place, 32nds; `None` where unknown.
    fn unit_spot(&self, id: u32) -> std::result::Result<Option<Spot>, Fault> {
        match self.found(id) {
            Found::Piece(p) => Ok(self.position(id).map(|at| (p.handle, at))),
            Found::Unsure => Ok(None),
            Found::Gone => Err(Fault::throw("MessageFlag() unit is null")),
        }
    }

    /// Message flag make: the flag at `at`, 32nds, hung on the unit
    /// of handle `holder` unless 0.
    fn raise_message(&mut self, holder: u32, at: (i32, i32), text: i32) {
        let fixed = (
            crate::sim::units::from_pos32(at.0),
            crate::sim::units::from_pos32(at.1),
        );
        let height = self
            .board
            .height_at(fixed.0, fixed.1)
            .unwrap_or(Fixed::ZERO);
        let standing = [fixed.0, fixed.1, height];
        let at = match holder {
            0 => standing,
            h => self.message_spot(h, 0).unwrap_or(standing),
        };

        self.handle += 1;
        self.outbox.messages.0.push(Message {
            handle: self.handle,
            text,
            holder,
            at,
            made: self.now,
        });

        // Each machine chimed for its own human: a cue each seat.
        for seat in self.humans() {
            self.cue_seat(NEW_MESSAGE, BEGIN, seat);
        }
    }

    /// Where a message flag hangs on the unit of handle `holder`, among
    /// the overlays on it but `but`: `GenUnit`'s point less the
    /// holder's place, y + 1, stacked, as a reward flag
    /// hangs. `None` where the holder has no agent here.
    fn message_spot(&self, holder: u32, but: u32) -> Option<[Fixed; 3]> {
        let target = self.holder(holder)?;
        let (attach, own) = self
            .descriptors
            .body(MESSAGE_FLAG)
            .map_or((0, 0), |b| (b.attach, b.stack));
        let [x, y, z] = self.hang_point(target, attach)?;
        let (dx, dy) = stack_offset(own, &self.stacked_but(holder, target, but));

        Some([x + dx, y + NEARER + dy, z])
    }

    /// The message flags hung on the unit of handle `holder` placed again
    /// among its overlays, as an overlay leaves it ([`World::restack`]).
    pub(super) fn restack_messages(&mut self, holder: u32) {
        let hung: Vec<u32> = self
            .outbox
            .messages
            .0
            .iter()
            .filter(|m| m.holder == holder && holder != 0)
            .map(|m| m.handle)
            .collect();

        for handle in hung {
            let spot = self.message_spot(holder, handle);

            if let (Some(spot), Some(m)) = (
                spot,
                self.outbox
                    .messages
                    .0
                    .iter_mut()
                    .find(|m| m.handle == handle),
            ) {
                m.at = spot;
            }
        }
    }

    /// The `StackPriority` of each message flag on the unit of handle
    /// `holder` but `but`, with its handle.
    pub(super) fn messages_on(&self, holder: u32, but: u32) -> Vec<(u32, u32)> {
        let own = self.descriptors.body(MESSAGE_FLAG).map_or(0, |b| b.stack);

        self.outbox
            .messages
            .0
            .iter()
            .filter(|m| m.holder == holder && m.handle != but)
            .map(|m| (m.handle, own))
            .collect()
    }

    /// Message flags standing, in the order made: a hung one goes with
    /// its holder.
    #[must_use]
    pub fn messages(&self) -> Vec<Message> {
        self.outbox
            .messages
            .0
            .iter()
            .filter(|m| m.holder == 0 || self.holder(m.holder).is_some())
            .copied()
            .collect()
    }

    /// Message flag `handle`'s place, 32nds, as unit pos gives
    /// it: a hung one's holder's place plus its offset. The message
    /// scroll's zoom centres here.
    #[must_use]
    pub fn message_point(&self, handle: u32) -> Option<(i32, i32)> {
        let m = self.messages().into_iter().find(|m| m.handle == handle)?;
        let base = match m.holder {
            0 => Some((Fixed::ZERO, Fixed::ZERO)),
            h => self.unit_pos(self.holder(h)?),
        };

        base.map(|(x, y)| (space::pos32(x + m.at[0]), space::pos32(y + m.at[1])))
    }

    /// The message flags as effects drawn: set `0x40` from their making,
    /// `Directionless`.
    pub(super) fn message_effects(&self) -> Vec<Effect> {
        self.messages()
            .into_iter()
            .map(|m| {
                let holder = self.holder(m.holder).unwrap_or_default();

                Effect {
                    handle: m.handle,
                    code: MESSAGE_FLAG,
                    holder,
                    at: m.at,
                    facing: FIRST_FACING,
                    look: Look::Plays {
                        set: LOOP_SET,
                        since: m.made,
                    },
                    value: None,
                    held: holder != 0 && self.is_held(holder),
                }
            })
            .collect()
    }

    /// What the local player's double click on message flag `handle`
    /// sends: [`Twice::Read`], where it stands.
    #[must_use]
    pub fn reads(&self, handle: u32) -> Option<Twice> {
        self.messages()
            .iter()
            .any(|m| m.handle == handle)
            .then_some(Twice::Read(handle))
    }

    /// `DESTROY_EVENT_FLAG_MESSAGE` decoded: message flag
    /// `handle` deleted at once; its holder's flags stack again.
    /// Whether it stood.
    pub fn read_message(&mut self, handle: u32) -> bool {
        let gone = self
            .messages()
            .iter()
            .find(|m| m.handle == handle)
            .map(|m| m.holder);

        let kept: Vec<Message> = self
            .messages()
            .into_iter()
            .filter(|m| m.handle != handle)
            .collect();

        self.outbox.messages.0 = kept;

        if let Some(target) = gone.and_then(|h| self.holder(h)) {
            self.restack(target);
        }

        gone.is_some()
    }

    /// `IsMessageFlagPresent(message)`: whether a message flag carries
    /// it. Unknown for a string it cannot read ([`number`]).
    pub(super) fn message_present(
        &self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        match number(vm.object(vm.arg(args, 1)?)) {
            Some(text) => {
                let ret = vm.arg(args, 0)?;
                let present = self.messages().iter().any(|m| m.text == text);

                put_bool(vm, ret, present)?;
                Ok(Answer::Given)
            }
            None => Ok(Answer::Unknown),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Gpl message number: an integer, an empty string 0, a string opening
    /// with digits their number, four bytes a FOURCC, a name its CRC.
    #[test]
    fn a_message_is_a_number() {
        assert_eq!(number(&Value::Integer(249)), Some(249));
        assert_eq!(number(&Value::String(String::new())), Some(0));
        assert_eq!(number(&Value::String("118 slaves".into())), Some(118));
        assert_eq!(
            number(&Value::String("ARE1".into())),
            Some(i32::from_le_bytes(*b"ARE1"))
        );
        assert_eq!(
            number(&Value::String("REDINN".into())),
            Some(crate::data::crc::crc(b"REDINN").cast_signed())
        );
    }
}

mod persist {
    use super::*;
    use crate::save::{self as m, Refused, Saved};
    use std::result::Result;

    impl Saved for Messages {
        type Model = Vec<m::Message>;

        fn save(&self) -> Self::Model {
            self.0.save()
        }

        fn load(model: Self::Model) -> Result<Messages, Refused> {
            Vec::load(model).map(Messages)
        }
    }
}
