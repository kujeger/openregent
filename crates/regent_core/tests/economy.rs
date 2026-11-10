//! What the player's input costs and what research gives, against the
//! SDK's constants: the research times and costs, the reward step, the
//! spells' and the enchantment's prices, the Last Palace multipliers.
//!
//! Needs `MAJESTY_HD`.

mod common;

use regent_core::gpl::exec::Literal;
use regent_core::gpl::load::Program;
use regent_core::sim::world::{
    flag::REWARD_DELTA,
    guild::ENCHANT_COST,
    price::LPS,
    research::{self, COST_PREFIX, TIME_PREFIX},
    spell::cast::SPELL_UNITS,
};

fn constant(program: &Program, name: &str) -> i32 {
    match program.constant(name) {
        Some(Literal::Int(v)) => v,
        other => panic!("{name}: {other:?}"),
    }
}

/// Farseeing is 200, the enchantment 400, a reward step 100.
#[test]
fn the_spells_and_the_enchantment_cost_the_constants() {
    let program = common::sdk_program();

    assert_eq!(constant(&program, SPELL_UNITS[0].2), 200);
    assert_eq!(constant(&program, ENCHANT_COST), 400);
    assert_eq!(constant(&program, "#RewardDelta"), REWARD_DELTA);
}

/// Every research's time and cost is a constant the SDK defines.
#[test]
fn every_research_has_a_time_and_a_cost() {
    let program = common::sdk_program();

    for r in research::ITEMS {
        let time = constant(&program, &format!("{TIME_PREFIX}{}", r.name));
        let cost = constant(&program, &format!("{COST_PREFIX}{}", r.name));

        assert!(time >= 10_000 && cost >= 200, "{}: {time} {cost}", r.name);
    }

    assert_eq!(constant(&program, "#ResearchCostArrows"), 250);
}

/// `globals.gpl` sets each of [`LPS`]'s constants.
#[test]
fn every_lps_constant_is_in_the_program() {
    let program = common::sdk_program();

    for (_, name) in LPS {
        assert!(constant(&program, name) > 0, "{name}");
    }

    assert_eq!(constant(&program, "#LPS_InnMultiplier"), 12);
}
