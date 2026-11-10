//! `LearnSpell` and `ForgetSpell`: a spell entry added to or taken off a
//! unit's list. **Binary**, gpl learn spell,
//! Gpl forget spell.
//!
//! ```text
//!  LearnSpell(agent, name, [flag])
//!    no unit          throw "unit no longer exists"
//!    descriptor_by_name(name), none: throw "Couldn't find spell"
//!    an entry of its code on the list: nothing
//!    else appended: code, cast at the clock, timeout 0,
//!                                so ready at once; the flag a byte no
//!                                save writes
//!  ForgetSpell(agent, name)
//!    no unit          throw "unit no longer exists"
//!    no descriptor    throw "Couldn't find spell %s to forget"
//!    the list copied, cleared, each entry of another code appended
//!                     back: every entry of the code gone, the rest in
//!                     order, timers kept
//! ```

use crate::gpl::vm::{Fault, Handle, Vm};
use crate::sim::world::spell::Charge;
use crate::sim::world::{Answer, Found, World, agent_ref, string_ref};

impl World {
    /// `LearnSpell(agent, name, [flag])`: throws with no unit or for a
    /// name no descriptor has; else the unit's list gains an entry of the
    /// descriptor's code, ready, unless it holds one. Returns nothing.
    pub(in crate::sim::world) fn learn_spell(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let id = agent_ref(vm, args, 1)?;
        let name = string_ref(vm, args, 2)?;

        if matches!(self.found(id), Found::Gone) {
            Err(Fault::throw(format!(
                "Function $LearnSpell( agent#{} ) failed: unit no longer exists.",
                id.cast_signed()
            )))?;
        }

        let code = self
            .descriptors
            .action_named(&name)
            .ok_or_else(|| Fault::throw(format!("Couldn't find spell {name} to learn")))?;
        let now = self.now;
        let list = self.pieces.spells_mut(id);

        match list {
            Some(list) => {
                if !list.iter().any(|c| c.code == code) {
                    list.push(Charge::new(code, now));
                }

                Ok(Answer::Given)
            }
            None => Ok(Answer::Unknown),
        }
    }

    /// `ForgetSpell(agent, name)`: throws with no unit or for a name no
    /// descriptor has; else every entry of the descriptor's code leaves
    /// the unit's list. Returns nothing.
    pub(in crate::sim::world) fn forget_spell(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let id = agent_ref(vm, args, 1)?;
        let name = string_ref(vm, args, 2)?;

        if matches!(self.found(id), Found::Gone) {
            Err(Fault::throw(format!(
                "Function $ForgetSpell( agent#{} ) failed: unit no longer exists.",
                id.cast_signed()
            )))?;
        }

        let code = self
            .descriptors
            .action_named(&name)
            .ok_or_else(|| Fault::throw(format!("Couldn't find spell {name} to forget")))?;
        let list = self.pieces.spells_mut(id);

        match list {
            Some(list) => {
                list.retain(|c| c.code != code);
                Ok(Answer::Given)
            }
            None => Ok(Answer::Unknown),
        }
    }
}
