//! A building's tax and repair settings (`world::upkeep`) and a tax
//! collector's amounts (`world::taxman`), as their panels set them.
//!
//! Corpora: `MAJESTY_HD`.

mod common;

use regent_core::gpl::exec::Literal;
use regent_core::sim::world::guild::taxman::{MAX_PICKUP, MAX_RETURN, TAXMAN_INCREMENT};

/// **Binary.** `taxman_constants` reads a step of 25 and maxima of 1000
/// from the SDK's globals.
#[test]
fn the_tax_collectors_step_and_maxima_are_the_globals() {
    let program = common::sdk_leaked();
    let int = |name| match program.constant(name) {
        Some(Literal::Int(v)) => v,
        other => panic!("{name}: {other:?}"),
    };

    assert_eq!(int(TAXMAN_INCREMENT), 25);
    assert_eq!(int(MAX_RETURN), 1000);
    assert_eq!(int(MAX_PICKUP), 1000);
}
