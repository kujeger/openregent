//! `ChangeUnitType` and `RevertUnitType`: a unit takes another
//! descriptor and back. **Binary**.
//!
//! ```text
//!  ChangeUnitType(agent, name)
//!    no unit              throw "ChangeUnitType() unit is null"
//!    no descriptor        throw "... type doesn't exist."
//!    not building/mobile  throw "... not a building or vehicle."
//!    APU:6 the old code, APU:4 its NameGenType, APU:5 its DialogID
//!    change the type (code, 0)
//!  RevertUnitType(agent)
//!    no unit              throw "RevertUnitType() unit is null"
//!    APU:6 0              throw "... no original type set."
//!    APU:6, 4, 5 removed; change the type (APU:6, 0)
//! ```
//!
//! Changing the type with flag 0 swaps the descriptor, art and pad
//! and re-places the unit where it stands; it keeps the attributes, so
//! hit points, stats and the name `AP:1` carry over, and the agent is
//! kept. The scripts adjust the stats themselves (`Change_Shape_Begin`,
//! `Shapeshift_Potion_Effect`). A second change overwrites `APU:6`, so a
//! revert goes back one step.

use super::{Answer, Found, World, agent_ref, bodies::Class, string_ref};
use crate::data::attrib::{Key, NAME_GEN_TABLE};
use crate::data::fourcc::FourCc;
use crate::gpl::vm::{Fault, Handle, Vm};

/// `APU:6`, `OriginalType`: the code before the change (`0x6555041`,
/// gpl change unit type).
pub const ORIGINAL_TYPE: Key = Key::new(*b"APU", 6);

/// `APU:5`, `DialogID`: the old descriptor's `DialogID`, game part.
pub const DIALOG_KEPT: Key = Key::new(*b"APU", 5);

/// A code as an attribute holds it: its four bytes, little-endian.
const fn word_of(code: FourCc) -> i32 {
    code.word().cast_signed()
}

impl World {
    /// `ChangeUnitType(agent, name)`: the old code, name generator and
    /// dialog kept in `APU:6`, `APU:4`, `APU:5`, then the new descriptor.
    pub(super) fn change_type(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let id = agent_ref(vm, args, 1)?;
        let name = string_ref(vm, args, 2)?;
        let old = match self.found(id) {
            Found::Piece(p) => Some(p.code),
            Found::Unsure => None,
            Found::Gone => Err(Fault::throw("ChangeUnitType() unit is null"))?,
        };
        let code = self.descriptors.code_named(&name).ok_or_else(|| {
            Fault::throw(format!(
                "ChangeUnitType( {name} ) failed - type doesn't exist."
            ))
        })?;
        let body = old.and_then(|o| Some((o, self.descriptors.body(o)?.clone())));

        match body {
            Some((_, b)) if matches!(b.class, Class::General | Class::Projectile) => Err(
                Fault::throw("ChangeUnitType() failed - not a building or vehicle."),
            ),
            Some((old, b)) => {
                let kept = [
                    (ORIGINAL_TYPE, word_of(old)),
                    (NAME_GEN_TABLE, b.names.map_or(0, word_of)),
                    (DIALOG_KEPT, b.dialog.map_or(0, word_of)),
                ];

                if let Some(piece) = self.pieces.get_mut(id) {
                    for (Key { bank, index }, v) in kept {
                        piece.attributes.insert(Key::new(bank, index).packed(), v);
                    }
                }

                self.retype(id, code);
                Ok(Answer::Given)
            }
            None => Ok(Answer::Unknown),
        }
    }

    /// `RevertUnitType(agent)`: back to `APU:6`'s code, the three keys
    /// gone.
    pub(super) fn revert_type(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let id = agent_ref(vm, args, 1)?;
        let original = match self.found(id) {
            Found::Piece(p) => Some(p.get(ORIGINAL_TYPE).unwrap_or(0)),
            Found::Unsure => None,
            Found::Gone => Err(Fault::throw("RevertUnitType() unit is null"))?,
        };

        match original {
            Some(0) => Err(Fault::throw(
                "RevertUnitType() failed - no original type set.",
            )),
            Some(code) => {
                if let Some(piece) = self.pieces.get_mut(id) {
                    for kept in [ORIGINAL_TYPE, NAME_GEN_TABLE, DIALOG_KEPT] {
                        piece.attributes.remove(&kept.packed());
                    }
                }

                self.retype(id, FourCc::from_word(code.cast_unsigned()));
                Ok(Answer::Given)
            }
            None => Ok(Answer::Unknown),
        }
    }

    /// `unit_change_type(code, 0)`: the descriptor and its name
    /// swapped, attributes kept; a building shows its set anew.
    /// Pad, art and walking read the code.
    fn retype(&mut self, id: u32, code: FourCc) {
        let name = self.descriptors.body(code).map(|b| b.name.clone());

        if let Some(piece) = self.pieces.get_mut(id) {
            piece.code = code;

            if let Some(name) = name {
                piece.name = name;
            }
        }

        self.restyle(id, super::plume::Force::Unforced);
    }
}
