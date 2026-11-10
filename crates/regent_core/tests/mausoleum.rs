//! A dead hero is interred in its owner's Mausoleum, and raised from it.
//!
//! **Binary** and SDK: `be_dead`, past the grave's time, runs
//! `Check_Mausoleum`; the panel's resurrect is `GUILD_ACTION` `0x15`
//! (`world::mausoleum`). **Inferred** all in play: no trace holds a
//! Mausoleum. The game is the quiet day's, a hero
//! recruited at its start, felled by lightning bolts.
//!
//! Corpora: `MAJESTY_HD`.

mod common;

use regent_core::gpl::exec::Literal;
use regent_core::sim::world::guild::mausoleum::MAUSOLEUM_LIMIT;

/// A Mausoleum holds ten: the scripts' `#Mausoleum_Limit`.
#[test]
fn a_mausoleum_holds_ten() {
    assert_eq!(
        common::sdk_program().constant(MAUSOLEUM_LIMIT),
        Some(Literal::Int(10))
    );
}
