//! The scripts whose dispatch the world wraps, each with its whole
//! sequence in one entry of [`ACTIONS`].
//!
//! ```text
//!  player's order ─ allowed? ─┐
//!                             v
//!  any dispatch ──────────> before ─> script runs ─> after ─> returned
//! ```
//!
//! `allowed` is the order's check in `Game::decode`; the other steps key on
//! the script's name, ignoring case, at every dispatch of it, whoever makes
//! it: a player's order or the engine.

use super::build::UPGRADED;
use super::dismiss::DISMISSED;
use super::guild::ENCHANT;
use super::guild::assembly::ASSEMBLY;
use super::guild::embassy::EMBASSY_INIT;
use super::guild::extortion::EXTORTION;
use super::guild::gamble::GAMBLE;
use super::guild::market::MARKET_DAY;
use super::guild::rage::RAGE;
use super::{GENERATE, NEW_UNIT, UNIT_DELETE, World};
use crate::gpl::load::Program;
use crate::gpl::ops::Value;

/// A script the world acts on around its dispatch. A missing step does
/// nothing.
pub struct Action {
    /// The script's name, matched ignoring case.
    pub script: &'static str,
    /// The player's order dispatches the script on agent `id` only where
    /// this holds; it may change the world as it checks.
    allowed: Option<fn(&mut World, u32) -> bool>,
    /// Before the dispatch, on its first argument, an agent.
    before: Option<fn(&mut World, &Program, u32)>,
    /// After the dispatch, on its first argument, an agent or an integer id.
    after: Option<fn(&mut World, &Program, u32)>,
    /// On the run's result, an integer, 0 for any other; its first
    /// argument an agent.
    returned: Option<fn(&mut World, u32, i32)>,
}

/// A building the player upgraded: a disabled upgrade or short treasury
/// refused, then the building changed before the dispatch.
pub const UPGRADE: Action = Action {
    script: UPGRADED,
    allowed: Some(|w, id| !w.upgrade_refused(id)),
    before: Some(|w, _, id| w.upgrade(id)),
    after: None,
    returned: None,
};

/// A dismissed hero: marked before the dispatch.
pub const DISMISS: Action = Action {
    script: DISMISSED,
    allowed: Some(|w, id| w.dismissable(id)),
    before: Some(|w, _, id| w.dismiss(id)),
    after: None,
    returned: None,
};

/// A wizards' guild's enchantment: paid before the dispatch.
pub const ENCHANTMENT: Action = Action {
    script: ENCHANT,
    allowed: None,
    before: Some(World::enchant),
    after: None,
    returned: None,
};

/// A Market Day: its time written before the dispatch;
/// `GS_OrderProcessor` pushes its order after it.
pub const MARKET: Action = Action {
    script: MARKET_DAY,
    allowed: Some(World::market_day_now),
    before: Some(|w, _, id| w.market_day_begins(id)),
    after: Some(World::market_day_arm),
    returned: None,
};

/// A dwarven assembly.
pub const ASSEMBLE: Action = Action {
    script: ASSEMBLY,
    allowed: Some(|w, id| w.assembles(id)),
    before: None,
    after: None,
    returned: None,
};

/// A rogues' guild's extortion.
pub const EXTORT: Action = Action {
    script: EXTORTION,
    allowed: Some(|w, id| w.extorts(id)),
    before: None,
    after: None,
    returned: None,
};

/// The Rage of Krolm: the check stamps the palace before the dispatch.
pub const RAGE_OF_KROLM: Action = Action {
    script: RAGE,
    allowed: Some(World::rage_now),
    before: None,
    after: None,
    returned: None,
};

/// An embassy opened (`Game::embassy`): `GS_OrderProcessor` arms
/// its timer after the dispatch.
pub const EMBASSY: Action = Action {
    script: EMBASSY_INIT,
    allowed: None,
    before: None,
    after: Some(|w, _, id| w.embassy_arm(id)),
    returned: None,
};

/// A spin of the gambling hall: gamble end pays the run's integer
/// (**Inferred**: what the engine's read of a failed run's result gives
/// was not read).
pub const SPIN: Action = Action {
    script: GAMBLE,
    allowed: None,
    before: None,
    after: None,
    returned: Some(World::gamble_paid),
};

/// A unit deleted: `World::unit_deleted` after the dispatch.
pub const DELETE: Action = Action {
    script: UNIT_DELETE,
    allowed: None,
    before: None,
    after: Some(|w, _, id| w.unit_deleted(id)),
    returned: None,
};

/// The first of [`super::ANNOUNCE`] for an agent makes it and binds its
/// piece before the dispatch ([`World::announce`]).
pub const GENERATED: Action = Action {
    script: GENERATE,
    allowed: None,
    before: Some(|w, program, id| w.announce(program, id, None, None)),
    after: None,
    returned: None,
};

/// As [`GENERATED`].
pub const NEW: Action = Action {
    script: NEW_UNIT,
    ..GENERATED
};

/// Every action, one per script.
pub const ACTIONS: [&Action; 12] = [
    &UPGRADE,
    &DISMISS,
    &ENCHANTMENT,
    &MARKET,
    &ASSEMBLE,
    &EXTORT,
    &RAGE_OF_KROLM,
    &EMBASSY,
    &SPIN,
    &DELETE,
    &GENERATED,
    &NEW,
];

impl Action {
    /// The action on script `function`, if any.
    #[must_use]
    pub fn of(function: &str) -> Option<&'static Action> {
        ACTIONS
            .into_iter()
            .find(|a| a.script.eq_ignore_ascii_case(function))
    }

    /// Whether the player's order dispatches the script on agent `id`.
    pub fn allows(&self, world: &mut World, id: u32) -> bool {
        self.allowed.is_none_or(|f| f(world, id))
    }
}

/// The id a dispatch's first argument names: an agent, or an integer as
/// `UnitDelete(integer agentID)` takes it.
pub(super) const fn id_arg(args: &[Value]) -> Option<u32> {
    match args.first() {
        Some(Value::Agent(id)) => Some(*id),
        Some(Value::Integer(id)) => Some(id.cast_unsigned()),
        _ => None,
    }
}

impl World {
    /// The engine dispatches `function` with `args`: its action's step
    /// before it runs.
    pub fn starts(&mut self, program: &Program, function: &str, args: &[Value]) {
        let step = Action::of(function).and_then(|a| a.before);

        if let (Some(step), Some(Value::Agent(id))) = (step, args.first()) {
            step(self, program, *id);
        }
    }

    /// The dispatch of `function` ended: its action's step after it.
    pub fn ends(&mut self, program: &Program, function: &str, args: &[Value]) {
        let step = Action::of(function).and_then(|a| a.after);

        if let (Some(step), Some(id)) = (step, id_arg(args)) {
            step(self, program, id);
        }
    }

    /// The dispatch of `function` with `args` returned `result`: its
    /// action's step on it.
    pub fn returned(&mut self, function: &str, args: &[Value], result: &Value) {
        let won = match result {
            Value::Integer(n) => *n,
            _ => 0,
        };
        let step = Action::of(function).and_then(|a| a.returned);

        if let (Some(step), Some(Value::Agent(id))) = (step, args.first()) {
            step(self, *id, won);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One action per script, ignoring case: a dispatch runs at most one
    /// entry's steps, so the order of [`ACTIONS`] does not matter.
    #[test]
    fn each_script_has_one_action() {
        for a in ACTIONS {
            let same = ACTIONS
                .iter()
                .filter(|b| b.script.eq_ignore_ascii_case(a.script))
                .count();

            assert_eq!(same, 1, "{}", a.script);
        }
    }
}
