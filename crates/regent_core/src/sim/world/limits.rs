//! Unit types a script disables, and building caps. **Binary**.
//!
//! ```text
//!  DisableUnitType(name)   the code into the level's disabled set;
//!  EnableUnitType(name)    takes it out;
//!                          an unknown name throws; then an interface
//!                          event (0x2435358)
//!  SetBuildingLimit(name, n)    the cap table of the build tree: code -> n
//!  RemoveBuildingLimit(name)    the code's entry erased
//!  RemoveAllBuildingLimits()    every entry erased
//!
//!  upgrade_refused(code)        build menu, preview (6), recruit buttons,
//!                               the upgrade button and its decode
//!  build_cap_ok(owner, code)  build menu, preview (8): with a cap,
//!                               the owner's standing buildings of the
//!                               code's prefix under it
//!  upgrade_uncapped(code)  an upgrade to a capped code is offered
//!                               as none
//! ```
//!
//! A save keeps both: `GSGL`'s code set is the disabled set
//! (gsgl codes read inserts, as
//! Type disable), its tail's pairs the cap table.

use std::collections::{BTreeMap, BTreeSet};

use super::{Answer, World, int_ref, string_ref};
use crate::data::fourcc::FourCc;
use crate::gpl::vm::{Fault, Handle, Vm};

/// The code build cap ok counts every building under: a compare
/// with `0x4c4c41`. **Inferred**: no descriptor has it, so
/// only a cap table entry the game never asks of in play.
pub const EVERY_CODE: [u8; 4] = *b"ALL\0";

/// The type limits the world keeps.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Limits {
    /// Disabled codes, level.
    pub disabled: BTreeSet<FourCc>,
    /// Caps by exact code, build tree.
    pub caps: BTreeMap<FourCc, i32>,
    /// The level's hero lists, each a graveyard's by its
    /// `GravesIndex` (`super::graveyard`).
    pub graves: Vec<Vec<super::graveyard::Grave>>,
}

/// Which way a type's switch goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Switch {
    Disable,
    Enable,
}

impl World {
    /// upgrade refused: whether a script disabled `code`.
    #[must_use]
    pub fn disabled(&self, code: FourCc) -> bool {
        self.limits.disabled.contains(&code)
    }

    /// Building upgrade's refusal: agent `id`'s `UpgradeTo` is
    /// disabled or unaffordable. Its cap is not asked.
    #[must_use]
    pub fn upgrade_refused(&self, id: u32) -> bool {
        self.pieces.get(id).is_some_and(|p| {
            self.descriptors
                .body(p.code)
                .and_then(|body| body.upgrade)
                .is_some_and(|to| {
                    self.disabled(to) || !self.affords(p.owner, self.upgrade_price(p.owner, to))
                })
        })
    }

    /// `build_cap_ok(owner, code)`: no cap on `code` passes; else
    /// `owner`'s standing buildings (`GenUnit` under 3) whose code
    /// shares `code`'s first three letters, or all with [`EVERY_CODE`], under
    /// the cap. So a cap on `Palace1` counts every palace.
    #[must_use]
    pub fn under_cap(&self, owner: u32, code: FourCc) -> bool {
        self.limits.caps.get(&code).is_none_or(|cap| {
            let counted = |c: [u8; 4]| code.0 == EVERY_CODE || c[..3] == code.0[..3];
            let have = self
                .kingdom(owner)
                .filter(|(_, p, _)| counted(p.code.0))
                .count();

            i32::try_from(have).unwrap_or(i32::MAX) < *cap
        })
    }

    /// The world on the build tree `caps`, its cap table replaced: a new
    /// freestyle game on the setup's Building Limitations. **Binary**:
    /// the setup page loads its pick into the table build cap ok
    /// reads, the editor's SAVE its
    /// tree; nothing else writes it before play, and a game's end empties
    /// it. **Inferred**: so a quest after
    /// a freestyle game starts with none; one started straight after
    /// picking a tree would keep its caps, a leak not reproduced here.
    #[must_use]
    pub fn limited(mut self, caps: &BTreeMap<FourCc, i32>) -> World {
        self.limits.caps.clone_from(caps);
        self
    }

    /// The cap table, by exact code.
    #[must_use]
    pub const fn caps(&self) -> &BTreeMap<FourCc, i32> {
        &self.limits.caps
    }

    /// Upgrade uncapped: whether the cap table lacks `code`.
    #[must_use]
    pub fn uncapped(&self, code: FourCc) -> bool {
        !self.limits.caps.contains_key(&code)
    }

    /// `DisableUnitType(name)`, gpl disable unit type, and `EnableUnitType(name)`,
    /// gpl enable unit type: the descriptor by name,
    /// else a throw; its code into the set or out of it. Returns nothing.
    pub(super) fn switch_type(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
        switch: Switch,
    ) -> std::result::Result<Answer, Fault> {
        let name = string_ref(vm, args, 1)?;
        let verb = match switch {
            Switch::Disable => "DisableUnitType",
            Switch::Enable => "EnableUnitType",
        };
        let code = self
            .descriptors
            .code_named(&name)
            .ok_or_else(|| Fault::throw(format!("{verb}( {name} ) failed.")))?;

        match switch {
            Switch::Disable => self.limits.disabled.insert(code),
            Switch::Enable => self.limits.disabled.remove(&code),
        };

        Ok(Answer::Given)
    }

    /// `SetBuildingLimit(name, n)`, gpl set building limit: the descriptor by name,
    /// else a throw; its cap `n`.
    pub(super) fn set_limit(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let name = string_ref(vm, args, 1)?;
        let code = self.descriptors.code_named(&name).ok_or_else(|| {
            Fault::throw(format!(
                "Function $SetBuildingLimit: unit type {name} is invalid."
            ))
        })?;
        let cap = int_ref(vm, args, 2)?;

        self.limits.caps.insert(code, cap);
        Ok(Answer::Given)
    }

    /// `RemoveBuildingLimit(name)`, gpl remove building limit: the descriptor by name,
    /// else a throw; its cap gone.
    pub(super) fn remove_limit(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let name = string_ref(vm, args, 1)?;
        let code = self.descriptors.code_named(&name).ok_or_else(|| {
            Fault::throw(format!(
                "Function $RemoveBuildingLimit: unit type {name} is invalid."
            ))
        })?;

        self.limits.caps.remove(&code);
        Ok(Answer::Given)
    }

    /// `RemoveAllBuildingLimits()`, gpl remove all building limits: the cap table emptied.
    pub(super) fn remove_limits(&mut self) -> Answer {
        self.limits.caps.clear();
        Answer::Given
    }
}
