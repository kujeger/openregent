//! A building's fall in the defeat trace: the log replayed on the
//! scheduler's frames, the world answering, until the palace falls.
//!
//! Corpora: `MAJESTY_HD`.

mod common;

use regent_core::gpl::exec::Literal;
use regent_core::gpl::load::Program;
use regent_core::sim::world::ruin::{RUBBLE_CONSTANT, RUBBLE_MS};

#[test]
fn a_ruin_stands_its_rubble_period() {
    let images: Vec<Vec<u8>> = common::sdk_images()
        .iter()
        .map(|p| std::fs::read(p).unwrap())
        .collect();
    let program = Program::load(&images).expect("SDK loads");

    assert_eq!(
        program.constant(RUBBLE_CONSTANT),
        Some(Literal::Int(RUBBLE_MS.cast_signed()))
    );
}
