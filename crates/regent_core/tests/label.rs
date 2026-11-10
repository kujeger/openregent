//! A selected attack flag against HD (`local/shots/flag_selected.png`,
//! 203 x 119): its reward over it in `fnt7` (`view::map::label`) and `AVn2`'s
//! brackets round it (`view::map::ring`); buildings' names under their bars
//! (`maj_mayhew.png`).
//!
//! Needs `MAJESTY_HD` and the captures.

mod common;

use regent_core::content::unit::Unit;
use regent_core::data::catalogue::Catalogue;
use regent_core::data::fourcc::FourCc;
use regent_core::view::map::label::{self};
use regent_core::view::map::ring::{self};

/// The attack flag, and the explore flag.
const ATTACK_FLAG: FourCc = FourCc(*b"ARA2");

/// Its reward, as the capture shows it.
const REWARD: i32 = 500;

fn flag<'a>(catalogue: &Catalogue<'a>) -> Unit<'a> {
    Unit::find(catalogue, ATTACK_FLAG)
        .expect("the flag")
        .expect("it resolves")
}

/// A reward flag's box takes its reward's (`GSUnit`, unit kind 4):
/// a 21 by 11 box over
/// the flag, not a name's under it.
#[test]
fn a_flags_box_takes_its_reward_over_it() {
    let library = common::library();
    let catalogue = library.catalogue().expect("a catalogue");
    let text = label::text(REWARD);
    let drawn = ring::Drawn::of(
        &catalogue,
        &flag(&catalogue),
        None,
        ring::HpBar::Hidden,
        Some(ring::Words::Reward(&text)),
    )
    .expect("it parses");

    assert_eq!(drawn.shown(), Some([-10, -34, 11, -23]));
}
