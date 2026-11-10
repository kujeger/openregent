//! A building's tax and repair settings: `GUILD_ACTION_MESSAGE` actions
//! 6 to 9. **Binary** unless marked.
//!
//! ```text
//!  panel   Base_BuildingController,      the building's handle:
//!            8024 "Tax this building once"          action 7
//!            8025 "Toggle … on or off tax route"    action 6
//!            8026 "Repair this building once"       action 9
//!            8027 "Toggle … on or off the repair route"  action 8
//!              lights the route button while its flag holds, else the
//!          once button while its flag holds; neither lit is never
//!  decode  guild action apply: the pressed flag flipped (0 → 1,
//!          else 0), then its pair 0
//!  scripts collect_tax visits a building with IsTaxed or QuickTax,
//!          tax_building clears QuickTax there; a peasant repairs one
//!          with IsRepaired or QuickRepair, BuildingReachedMaxHP clears
//!          QuickRepair
//! ```
//!
//! The engine keeps no other state: the four flags are building
//! attributes (`APB:16`, `17`, `20`, `21`), and a save holds them.

use crate::data::attrib::{FIRST_STAGE_BUILT, IS_REPAIRED, IS_TAXED, Key, QUICK_REPAIR, QUICK_TAX};
use crate::sim::world::{World, bodies::Class};

/// `IncomeType` of a building with no tax buttons: `BuildingDesc`
/// compared with 3.
pub const UNTAXED_INCOME: u32 = 3;

/// One of the building panel's four buttons.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Upkeep {
    TaxOnce,
    TaxRoute,
    RepairOnce,
    RepairRoute,
}

impl Upkeep {
    pub const ALL: [Upkeep; 4] = [
        Upkeep::TaxOnce,
        Upkeep::TaxRoute,
        Upkeep::RepairOnce,
        Upkeep::RepairRoute,
    ];

    /// The flag it flips, and the one it clears.
    #[must_use]
    pub const fn keys(self) -> (Key, Key) {
        match self {
            Upkeep::TaxRoute => (IS_TAXED, QUICK_TAX),
            Upkeep::TaxOnce => (QUICK_TAX, IS_TAXED),
            Upkeep::RepairRoute => (IS_REPAIRED, QUICK_REPAIR),
            Upkeep::RepairOnce => (QUICK_REPAIR, IS_REPAIRED),
        }
    }

    /// Its word in a recorded input.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Upkeep::TaxOnce => "tax-once",
            Upkeep::TaxRoute => "tax-route",
            Upkeep::RepairOnce => "repair-once",
            Upkeep::RepairRoute => "repair-route",
        }
    }

    /// The button [`Upkeep::word`] names.
    #[must_use]
    pub fn of_word(word: &str) -> Option<Upkeep> {
        Upkeep::ALL.into_iter().find(|u| u.word() == word)
    }
}

/// A press of one of those buttons on a building, by agent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Toggle {
    pub building: u32,
    pub upkeep: Upkeep,
}

/// What the panel shows of one duty.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Setting {
    Never,
    Once,
    Always,
}

impl Setting {
    /// From a route flag and a once flag, as method lights them.
    #[must_use]
    pub const fn of(route: i32, once: i32) -> Setting {
        match (route, once) {
            (0, 0) => Setting::Never,
            (0, _) => Setting::Once,
            _ => Setting::Always,
        }
    }
}

/// A building's two duties, each `None` while its buttons are disabled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Upkeeps {
    pub tax: Option<Setting>,
    pub repair: Option<Setting>,
}

impl World {
    /// guild action apply cases 6 to 9 on `t`'s building, which
    /// needs no agent (agent by handle finds the unit by handle).
    pub fn toggle(&mut self, t: &Toggle) {
        let (flipped, cleared) = t.upkeep.keys();
        let word = |at: Key| at.packed();

        if let Some(p) = self.pieces.get(t.building) {
            let old = p
                .attributes
                .get(&word(flipped))
                .copied()
                .unwrap_or_default();

            self.write(t.building, word(flipped), i32::from(old == 0));
            self.write(t.building, word(cleared), 0);
        }
    }

    /// Building `id`'s settings as its panel shows them:
    /// tax but for an `IncomeType`
    /// [`UNTAXED_INCOME`] building, repair once its first stage is
    /// built. **Inferred**: both tax buttons enable together (the once
    /// button's flag comes off the stack).
    #[must_use]
    pub fn upkeeps(&self, id: u32) -> Option<Upkeeps> {
        let piece = self.pieces.get(id)?;
        let body = self.body_of(id).filter(|b| b.class == Class::Building)?;
        let word = |at: Key| piece.get(at).unwrap_or_default();
        let tax = Setting::of(word(IS_TAXED), word(QUICK_TAX));
        let repair = Setting::of(word(IS_REPAIRED), word(QUICK_REPAIR));

        Some(Upkeeps {
            tax: (body.income != UNTAXED_INCOME).then_some(tax),
            // The repair buttons wait for the first stage.
            repair: (word(FIRST_STAGE_BUILT) != 0).then_some(repair),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_setting_is_the_route_first_then_once() {
        assert_eq!(Setting::of(0, 0), Setting::Never);
        assert_eq!(Setting::of(0, 1), Setting::Once);
        assert_eq!(Setting::of(1, 0), Setting::Always);
        assert_eq!(Setting::of(1, 1), Setting::Always);
    }

    #[test]
    fn each_button_names_its_action_and_word() {
        for u in Upkeep::ALL {
            assert_eq!(Upkeep::of_word(u.word()), Some(u));
        }
    }
}
