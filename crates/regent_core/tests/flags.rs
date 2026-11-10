//! Descriptor bitfields: the loader's `CanUse` default.

use regent_core::data::desc::User;

/// The loader fills a zero `CanUse` from the id's first letter: `A` is
/// `HumanPlayer`, `B` is `Monster`.
#[test]
fn the_loader_defaults_can_use_by_the_ids_first_letter() {
    let id = |s: &[u8; 4]| regent_core::data::FourCc::new(s);

    assert_eq!(User::default_for(id(b"AVA1")), Some(User::HumanPlayer));
    assert_eq!(User::default_for(id(b"BBL1")), Some(User::Monster));
    assert_eq!(User::default_for(id(b"XLSL")), None);
}
