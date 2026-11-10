//! Inventory items: the dropped item's type and the register a save keeps.
//!
//! Corpus: `MAJESTY_HD`. Required.

mod common;

use regent_core::sim::world::{
    inventory::{DROPPED_AS, SPELLBOOK},
    players::INVENTORY,
};

#[test]
fn the_spellbook_slot_is_the_quests_constant() {
    use regent_core::gpl::ast::{ExprKind, Item};

    let source = std::fs::read_to_string(common::sdk().join("OriginalQuests/GPL/QItems.gpl"))
        .expect("QItems.gpl");
    let value = regent_core::gpl::parse(&source)
        .script
        .items
        .into_iter()
        .find_map(|item| match item {
            Item::Constant(c) if c.name.eq_ignore_ascii_case("QItem_Spellbook") => {
                Some(c.value.kind)
            }
            _ => None,
        });

    assert_eq!(value, Some(ExprKind::Int(i64::from(SPELLBOOK))));
}

/// `Inventory.gpl` spawns a dropped item as `GetInventoryItemAgentType`
/// names it: HD's `AVk4`, `Resource_items`, by a name folding case.
#[test]
fn a_dropped_slot_item_spawns_as_a_descriptor() {
    let library = common::library();
    let catalogue = library.catalogue().expect("catalogue");
    let descriptors = regent_core::sim::setup::descriptors(&catalogue);

    assert!(descriptors.described(&DROPPED_AS.to_ascii_lowercase()));
}

/// A dropped spellbook shows as "Book of Spells", the panels' title for
/// its inventory key.
#[test]
fn a_dropped_spellbook_is_titled_book_of_spells() {
    let library = common::library();
    let catalogue = library.catalogue().expect("catalogue");
    let text = regent_core::data::text::Text::build(&catalogue);
    let key = regent_core::data::attrib::Key::new(INVENTORY, SPELLBOOK).packed();
    let item = regent_core::view::panel::portrait::details::item(&catalogue, &text, key);

    assert_eq!(
        item.map(|(name, _)| name).as_deref(),
        Some("Book of Spells")
    );
}
