//! Inventory items: an attribute on the unit that carries one, and the
//! level's register of who carries what. **Binary** unless marked.
//!
//! ```text
//!  an item argument, every builtin here (its id, after the INV test)
//!    integer under 256, unsigned: key INV:n (n << 24 | "INV")
//!    other integer: itself
//!    string: empty throws; else its id (string id, string_id); an id
//!            0, -1 or a registered attribute key throws
//!    else: throws
//!
//!  an item key: INV:n, or a descriptor code whose character Flags hold
//!    IsInventoryItem (is item key): the SDK's "MagicBall"
//!
//!  the register, the level's: a multimap, key compared unsigned, to unit
//!    handles; saved as GSGL's pairs. A lookup by key first drops
//!    carriers no unit has (inventory prune)
//!
//!  CreateNewInventoryItem(item, agent, [cloned])
//!    no unit: throw
//!    a carrier already (inventory nth carrier), and no `cloned`: throw
//!          "Tried to create duplicate quest item"
//!    the unit lacks the key, or cloned: the register takes
//!          (key, unit handle), then GSUnit writes the key, 0
//!  TransferInventoryItem(item, from, to)
//!    either unit missing: throw
//!    the delete from `from`; `to` lacking the key takes it
//!          as the create does
//! ```

use crate::data::attrib::Key;
use std::collections::BTreeMap;

use super::{Answer, Found, World, agent_ref, bodies::Class, int_ref, put_bool, put_int};
use crate::data::FourCc;
use crate::data::attrib;
pub use crate::data::fourcc::string_id;
use crate::gpl::ops::{self, Accessor, OpError, Value};
use crate::gpl::vm::{Fault, Handle, Vm};

/// Whether a second carrier of a key is allowed.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Copies {
    Once,
    Cloned,
}

use super::players::INVENTORY;

/// Item agent type's and `GetInventoryItemAgentType`'s answer for an
/// `INV:n` key or a key that is no item: the text at `"Resource_Items"`. The
/// unit a dropped item spawns as (`Hero_Deaths.gpl`'s `SpawnUnit`).
pub const DROPPED_AS: &str = "Resource_Items";

/// `QItems.gpl`'s `#QItem_Spellbook`: the Wizard's Curse book.
pub const SPELLBOOK: u8 = 15;

/// Item drop type's drop type for an `INV:n` key, item drop type.
const SLOT_DROP_TYPE: i32 = 1;

/// Argument `i` as an item key, item id after the `INV` test.
pub(super) fn item_key(vm: &Vm<'_>, args: &[Handle], i: usize) -> Result<i32, Fault> {
    match vm.object(vm.arg(args, i)?) {
        Value::Integer(n) => Ok(u8::try_from(*n).map_or(*n, |n| Key::new(INVENTORY, n).packed())),
        Value::String(s) if s.is_empty() => Err(Fault::throw("Empty string passed in as ID.")),
        Value::String(s) => {
            let id = string_id(s);

            if id == 0 || id == -1 || super::registered(id) {
                Err(Fault::throw(format!(
                    "ID {s} string hashes to a reserved value, change the ID name."
                )))
            } else {
                Ok(id)
            }
        }
        _ => Err(Fault::throw("Invalid parameter type passed as ID")),
    }
}

/// The result `answer`, where known.
fn put_known(
    vm: &mut Vm<'_>,
    args: &[Handle],
    answer: Option<bool>,
) -> std::result::Result<Answer, Fault> {
    match answer {
        Some(b) => {
            let ret = vm.arg(args, 0)?;

            put_bool(vm, ret, b).map(|()| Answer::Given)
        }
        None => Ok(Answer::Unknown),
    }
}

/// Whether `item` is an `INV:n` key.
fn is_slot(item: i32) -> bool {
    item.to_le_bytes()[..3] == INVENTORY
}

/// The level's inventory register, level: each item key's
/// carriers' unit handles, in the order they took it
/// (`CreateNewInventoryItem`). A save's `GSGL` pairs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Register(BTreeMap<i32, Vec<u32>>);

impl Register {
    /// From pairs: key, then carrier's handle.
    #[cfg(test)]
    pub(super) fn of(pairs: &[(u32, i32)]) -> Register {
        Register(
            pairs
                .iter()
                .fold(BTreeMap::new(), |mut items, (item, handle)| {
                    items
                        .entry(item.cast_signed())
                        .or_insert_with(Vec::new)
                        .push(handle.cast_unsigned());
                    items
                }),
        )
    }

    /// `item`'s carriers, unit handles in the order they took it.
    pub(super) fn carriers(&self, item: i32) -> &[u32] {
        self.0.get(&item).map_or(&[], Vec::as_slice)
    }

    /// Inventory insert: unit `handle` takes `item`, last.
    fn take(&mut self, item: i32, handle: u32) {
        self.0.entry(item).or_default().push(handle);
    }

    /// The first entry of `item` naming unit `handle` gone.
    fn give_up(&mut self, item: i32, handle: u32) {
        if let Some(carriers) = self.0.get_mut(&item)
            && let Some(at) = carriers.iter().position(|h| *h == handle)
        {
            carriers.remove(at);
        }

        self.0.retain(|_, carriers| !carriers.is_empty());
    }

    /// Inventory prune: `item`'s carriers not `live` go, then where it
    /// stopped, the first carrier of the next key, if not `live`.
    fn prune(&mut self, item: i32, live: impl Fn(u32) -> bool) {
        if let Some(carriers) = self.0.get_mut(&item) {
            carriers.retain(|h| live(*h));
        }

        // The quirk: the range's end is the next key's first entry.
        let next = self
            .0
            .iter()
            .filter(|(k, c)| k.cast_unsigned() > item.cast_unsigned() && !c.is_empty())
            .min_by_key(|(k, _)| k.cast_unsigned())
            .map(|(k, c)| (*k, c[0]));

        if let Some((k, first)) = next
            && !live(first)
            && let Some(carriers) = self.0.get_mut(&k)
        {
            carriers.remove(0);
        }

        self.0.retain(|_, carriers| !carriers.is_empty());
    }

    /// Inventory count's count, after a prune: 0 with no key at or past
    /// `item`; else `item`'s carriers plus one. The engine's count runs
    /// from the key's lower bound to its upper, then adds one, so a key
    /// none carries counts 1 where a later key stands.
    fn count(&self, item: i32) -> i32 {
        let at_or_past = self
            .0
            .keys()
            .any(|k| k.cast_unsigned() >= item.cast_unsigned());
        let carriers = self.carriers(item).len();

        if at_or_past {
            i32::try_from(carriers + 1).unwrap_or(i32::MAX)
        } else {
            0
        }
    }

    /// Inventory first of's walk: in key order, the key compared unsigned (a
    /// `std::multimap`), the first key unit `handle` carries.
    fn first_of(&self, handle: u32) -> Option<i32> {
        self.0
            .iter()
            .filter(|(_, carriers)| carriers.contains(&handle))
            .map(|(k, _)| *k)
            .min_by_key(|k| k.cast_unsigned())
    }

    /// Every key's carriers, as a save holds them.
    pub(super) const fn entries(&self) -> &BTreeMap<i32, Vec<u32>> {
        &self.0
    }

    /// The saved entries.
    pub(super) const fn restored(entries: BTreeMap<i32, Vec<u32>>) -> Register {
        Register(entries)
    }
}

impl World {
    /// Register item attributes supplied when a unit is made.
    pub(super) fn note_items(&mut self, id: u32) {
        if let Some(piece) = self.pieces.get(id) {
            let handle = piece.handle;
            let items: Vec<i32> = piece
                .attributes
                .keys()
                .copied()
                .filter(|item| self.is_item_key(*item) == Some(true))
                .collect();

            for item in items {
                self.level.items.take(item, handle);
            }
        }
    }

    /// The register's carriers of `item`, unit handles in the order they
    /// took it.
    pub fn carriers(&self, item: i32) -> &[u32] {
        self.level.items.carriers(item)
    }

    /// `CreateNewInventoryItem(item, agent, [cloned])`, gpl create new inventory item.
    /// Unknown for a unit the world cannot place.
    pub(super) fn create_item(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let item = item_key(vm, args, 1)?;
        let id = agent_ref(vm, args, 2)?;
        let cloned = if args.len() > 3 && int_ref(vm, args, 3)? != 0 {
            Copies::Cloned
        } else {
            Copies::Once
        };

        match self.found(id) {
            Found::Gone => Err(Fault::throw(
                "Function $CreateNewInventoryItem failed: unit no longer exists.",
            )),
            Found::Piece(_) => self.give_item(id, item, cloned).map(|()| Answer::Given),
            Found::Unsure => Ok(Answer::Unknown),
        }
    }

    /// Item `item` to agent `id`'s unit: the register's duplicate test,
    /// then inventory create.
    fn give_item(&mut self, id: u32, item: i32, cloned: Copies) -> Result<(), Fault> {
        if self.first_carrier(item).is_some() && cloned == Copies::Once {
            let [_, _, _, n] = item.to_le_bytes();
            let name = attrib::name(INVENTORY, n)
                .filter(|_| is_slot(item))
                .map_or_else(|| item.to_string(), str::to_owned);

            return Err(Fault::throw(format!(
                "Tried to create duplicate quest item \"{name}\""
            )));
        }

        let lacks = self
            .pieces
            .get(id)
            .is_none_or(|p| !p.attributes.contains_key(&item));

        if lacks || cloned == Copies::Cloned {
            self.carry(id, item);
        }

        Ok(())
    }

    /// Item `item` to agent `id`'s unit as `World::carry` gives it,
    /// no duplicate test: a scene's (`sim::scene::Item`).
    pub fn give_item_to(&mut self, id: u32, item: i32) {
        self.carry(id, item);
    }

    /// The register takes `item` with agent `id`'s unit's handle, and the
    /// unit the key at 0.
    fn carry(&mut self, id: u32, item: i32) {
        let handle = self.pieces.handle(id).unwrap_or(0);

        self.level.items.take(item, handle);
        self.write(id, item, 0);
    }

    /// `DeleteInventoryItem(item, agent)`, gpl delete inventory item: throws with no
    /// unit. Unknown for a unit the world cannot place.
    pub(super) fn delete_item(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let item = item_key(vm, args, 1)?;
        let id = agent_ref(vm, args, 2)?;

        match self.found(id) {
            Found::Gone => Err(Fault::throw("DeleteInventoryItem() unit is null")),
            Found::Piece(_) => {
                self.take_item(id, item);
                Ok(Answer::Given)
            }
            Found::Unsure => Ok(Answer::Unknown),
        }
    }

    /// Inventory delete on the register for `item` and agent `id`'s unit:
    /// nothing unless the unit carries the key; else the first register
    /// entry of the key naming the unit's handle goes, then
    /// the key from the unit's attributes. It then
    /// broadcasts `XSC:10`, `SC_ITEM_LOST` (inventory delete, `0x0a435358`),
    /// to the engine's listeners; no script names it; not modelled.
    fn take_item(&mut self, id: u32, item: i32) {
        let carried = self
            .pieces
            .get_mut(id)
            .and_then(|p| p.attributes.remove(&item).map(|_| p.handle));

        if let Some(handle) = carried {
            self.level.items.give_up(item, handle);
        }
    }

    /// `TransferInventoryItem(item, from, to)`, gpl transfer inventory item: throws with
    /// either unit missing; else inventory transfer: the delete from `from`,
    /// then `to`, lacking the key, takes it at 0. The key's value is not
    /// carried over. Unknown for a unit the world cannot place.
    pub(super) fn transfer_item(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let item = item_key(vm, args, 1)?;
        let from = agent_ref(vm, args, 2)?;
        let source = match self.found(from) {
            Found::Gone => Err(Fault::throw("TransferInventoryItem() source is null"))?,
            found => matches!(found, Found::Piece(_)),
        };
        let to = agent_ref(vm, args, 3)?;
        let recipient = match self.found(to) {
            Found::Gone => Err(Fault::throw("TransferInventoryItem() recipient is null"))?,
            found => matches!(found, Found::Piece(_)),
        };

        if source && recipient {
            self.take_item(from, item);

            if self
                .pieces
                .get(to)
                .is_some_and(|p| !p.attributes.contains_key(&item))
            {
                self.carry(to, item);
            }

            Ok(Answer::Given)
        } else {
            Ok(Answer::Unknown)
        }
    }

    /// Inventory prune: the register drops `item`'s carriers no unit has,
    /// then where it stopped, the first carrier of the next key, if no
    /// unit has it.
    fn prune(&mut self, item: i32) {
        let live: Vec<u32> = self
            .level
            .items
            .entries()
            .values()
            .flatten()
            .copied()
            .filter(|h| self.holder(*h).is_some())
            .collect();

        self.level.items.prune(item, |h| live.contains(&h));
    }

    /// `inventory_nth_carrier(item, 0)`: the agent of the register's first carrier
    /// of `item`, after [`World::prune`].
    fn first_carrier(&mut self, item: i32) -> Option<u32> {
        self.prune(item);

        self.level
            .items
            .carriers(item)
            .first()
            .and_then(|h| self.holder(*h))
    }

    /// Inventory count: after [`World::prune`], the register's count
    /// ([`Register::count`]).
    fn count_of(&mut self, item: i32) -> i32 {
        self.prune(item);
        self.level.items.count(item)
    }

    /// Is item key: an `INV:n` key, or a descriptor code whose
    /// game part's flags hold `0x400`: a character's
    /// `IsInventoryItem`. `None` for a building's code: its flags
    /// are not read. **Inferred**: a code no descriptor has is none.
    pub(super) fn is_item_key(&self, item: i32) -> Option<bool> {
        let body = self
            .descriptors
            .body(FourCc::from_word(item.cast_unsigned()));

        match body.map(|b| (b.class, b.item)) {
            _ if is_slot(item) => Some(true),
            Some((Class::Building, _)) => None,
            Some((Class::Character, item)) => Some(item),
            _ => Some(false),
        }
    }

    /// Item drop type: 1 for `INV:n`; a descriptor item's `CanDropItem`
    /// attribute (attrib can drop item, its game part's list), not read
    /// here: `None`; else 0.
    fn drop_type(&self, item: i32) -> Option<i32> {
        match self.is_item_key(item) {
            _ if is_slot(item) => Some(SLOT_DROP_TYPE),
            Some(false) => Some(0),
            _ => None,
        }
    }

    /// Item agent type: a descriptor item's `HEAD` name (descriptor),
    /// else [`DROPPED_AS`].
    fn agent_type(&self, item: i32) -> Option<String> {
        let body = self
            .descriptors
            .body(FourCc::from_word(item.cast_unsigned()));

        match self.is_item_key(item) {
            _ if is_slot(item) => Some(DROPPED_AS.to_owned()),
            Some(true) => body.map(|b| b.name.clone()),
            Some(false) => Some(DROPPED_AS.to_owned()),
            None => None,
        }
    }

    /// `FindInventoryItem(item)`, gpl find inventory item: the agent (`AE:0`) of the
    /// register's first carrier; none leaves the result as it was.
    pub(super) fn find_item(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let item = item_key(vm, args, 1)?;

        if let Some(id) = self.first_carrier(item) {
            let ret = vm.arg(args, 0)?;

            ops::assign(vm.object_mut(ret), &Value::Agent(id)).map_err(Fault::from)?;
        }

        Ok(Answer::Given)
    }

    /// `MakeInventoryAttribute(item)`, gpl make inventory attribute: the item's key.
    pub(super) fn make_attribute(
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let item = item_key(vm, args, 1)?;
        let ret = vm.arg(args, 0)?;

        put_int(vm, ret, item).map(|()| Answer::Given)
    }

    /// `CountInventoryItem(item)`, gpl count inventory item: [`World::count_of`].
    pub(super) fn count_item(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let item = item_key(vm, args, 1)?;
        let ret = vm.arg(args, 0)?;
        let count = self.count_of(item);

        put_int(vm, ret, count).map(|()| Answer::Given)
    }

    /// `IsInventoryItem(item)`, gpl is inventory item: [`World::is_item_key`].
    pub(super) fn is_item(
        &self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let item = item_key(vm, args, 1)?;

        put_known(vm, args, self.is_item_key(item))
    }

    /// `CanDropInventoryItem(item)`, gpl can drop inventory item: a drop type not 0.
    pub(super) fn can_drop(
        &self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let item = item_key(vm, args, 1)?;

        put_known(vm, args, self.drop_type(item).map(|t| t != 0))
    }

    /// `GetInventoryItemDropType(item)`, gpl get inventory item drop type:
    /// [`World::drop_type`]. Scripts drop an item at once from 2
    /// (`Inventory.gpl`).
    pub(super) fn item_drop_type(
        &self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let item = item_key(vm, args, 1)?;

        match self.drop_type(item) {
            Some(t) => {
                let ret = vm.arg(args, 0)?;

                put_int(vm, ret, t).map(|()| Answer::Given)
            }
            None => Ok(Answer::Unknown),
        }
    }

    /// `GetInventoryItemAgentType(item)`, gpl get inventory item agent type:
    /// [`World::agent_type`].
    pub(super) fn item_agent_type(
        &self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let item = item_key(vm, args, 1)?;

        match self.agent_type(item) {
            Some(name) => {
                let ret = vm.arg(args, 0)?;

                ops::assign(vm.object_mut(ret), &Value::String(name))
                    .map(|()| Answer::Given)
                    .map_err(Fault::from)
            }
            None => Ok(Answer::Unknown),
        }
    }

    /// Inventory first of: the register in key order, the key compared
    /// unsigned (a `std::multimap`), the first key unit `handle` carries;
    /// an `INV:n` key as `n`, any other as itself.
    pub fn first_item(&self, handle: u32) -> Option<i32> {
        self.level.items.first_of(handle).map(|k| {
            if is_slot(k) {
                i32::from(k.to_le_bytes()[3])
            } else {
                k
            }
        })
    }

    /// `GetInventoryItems(agent)`, gpl get inventory items: with no unit, nothing;
    /// else each key of the unit's attribute list that is an `INV:n` slot
    /// or a descriptor code whose game part has `Flags` bit `0x400`,
    /// appended to the return slot as an integer.
    ///
    /// **Inferred**: the list is a map, in key order. Unknown where
    /// `World::is_item_key` is.
    pub(super) fn inventory_items(
        &self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let ret = vm.arg(args, 0)?;

        if !matches!(vm.object(ret), Value::List(_)) {
            Err(OpError::BadConversion(Accessor::ListRef))?;
        }

        let id = agent_ref(vm, args, 1)?;
        let keys = match self.found(id) {
            Found::Gone => Some(Vec::new()),
            Found::Piece(p) if self.complete(id) => {
                let tests: Option<Vec<(i32, bool)>> = p
                    .attributes
                    .keys()
                    .map(|k| self.is_item_key(*k).map(|item| (*k, item)))
                    .collect();

                tests.map(|tests| {
                    let mut items: Vec<i32> = tests
                        .into_iter()
                        .filter_map(|(k, item)| item.then_some(k))
                        .collect();

                    items.sort_by_key(|k| k.cast_unsigned());
                    items
                })
            }
            _ => None,
        };

        match keys {
            Some(keys) => {
                if let Value::List(out) = vm.object_mut(ret) {
                    out.extend(keys.into_iter().map(Value::Integer));
                }

                Ok(Answer::Given)
            }
            None => Ok(Answer::Unknown),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::attrib::EXPERIENCE_LEVEL;
    use crate::gpl::vm::ObjId;
    use crate::sim::world::{Checked, Piece};

    fn world() -> World {
        let mut world = World::default();

        for (id, handle) in [(13, 130), (14, 140)] {
            world.pieces.insert(
                id,
                Piece {
                    handle,
                    ..Piece::default()
                },
            );
        }
        world
    }

    fn slot(n: u8) -> i32 {
        Key::new(INVENTORY, n).packed()
    }

    /// The market's ring of protection, item 11: the first buyer takes
    /// `INV:11` at 0 and the register notes him; a second needs `cloned`
    /// (`#Allow_Cloned_Quest_Item`, which `Purchase_Prot_Ring` passes),
    /// else the engine throws.
    #[test]
    fn a_new_item_is_an_attribute_and_a_register_entry() {
        let mut world = world();
        let ring = slot(11);

        assert!(world.give_item(13, ring, Copies::Once).is_ok());
        assert_eq!(world.pieces[&13].attributes.get(&ring), Some(&0));
        assert_eq!(world.carriers(ring), vec![130]);

        assert!(world.give_item(14, ring, Copies::Once).is_err());
        assert!(world.give_item(14, ring, Copies::Cloned).is_ok());
        assert_eq!(world.carriers(ring), vec![130, 140]);
    }

    /// `AgentHasInventoryItem(-1, …)`'s search: the
    /// register in key order, unsigned, the first key a unit's handle
    /// carries; an `INV:n` key reads back as `n`.
    #[test]
    fn a_search_finds_the_first_item_in_key_order() {
        let mut world = world();

        assert_eq!(world.first_item(130), None);

        for n in [200, 11, 5] {
            assert!(world.give_item(13, slot(n), Copies::Cloned).is_ok());
        }

        assert!(world.give_item(14, slot(200), Copies::Cloned).is_ok());

        assert_eq!(world.first_item(130), Some(5));
        assert_eq!(world.first_item(140), Some(200));
    }

    /// String id: a digit `atoi`, four bytes a FOURCC read in file
    /// order, quotes padded out, else CRC-32 from all ones, not inverted:
    /// standard CRC-32 is the inverse.
    #[test]
    fn a_string_id_is_a_number_a_code_or_a_crc() {
        let standard = |text: &str| {
            !text.bytes().fold(u32::MAX, |c, b| {
                crate::data::crc::CRC_TABLE[((c ^ u32::from(b)) & 0xff) as usize] ^ (c >> 8)
            })
        };

        assert_eq!(string_id(""), 0);
        assert_eq!(string_id("12abc"), 12);
        assert_eq!(string_id("20000"), 20000);
        assert_eq!(
            string_id("2000"),
            i32::from_le_bytes(*b"2000"),
            "four bytes are a code before a number"
        );
        assert_eq!(string_id("HGUA"), i32::from_le_bytes(*b"HGUA"));
        assert_eq!(string_id("'ab'"), i32::from_le_bytes(*b"ab  "));
        assert_eq!(standard("123456789"), 0xcbf4_3926);
        assert_eq!(
            string_id("MagicBall"),
            (!standard("MagicBall")).cast_signed()
        );
    }

    /// The SDK's `"MagicBall"`, no integer under 256: its id is its key.
    /// An attribute's name, or nothing, throws.
    #[test]
    fn a_named_item_is_its_string_id() {
        let program = crate::gpl::load::Program::load::<&[u8]>(&[]).expect("empty program");
        let mut vm = Vm::new(&program, crate::gpl::vm::Config::default());
        let key_of = |vm: &mut Vm<'_>, v: Value| {
            let o = vm.alloc(v);
            let h = vm.top_handle(Some(o));

            item_key(vm, &[h, h], 1)
        };

        assert_eq!(
            key_of(&mut vm, Value::String("MagicBall".into())),
            Ok(string_id("MagicBall"))
        );
        assert_eq!(key_of(&mut vm, Value::Integer(11)), Ok(slot(11)));
        assert_eq!(key_of(&mut vm, Value::Integer(256)), Ok(256));
        assert_eq!(key_of(&mut vm, Value::Integer(-1)), Ok(-1));
        assert!(key_of(&mut vm, Value::String(String::new())).is_err());
        assert!(key_of(&mut vm, Value::String("0".into())).is_err());
        assert!(key_of(&mut vm, Value::Float(crate::base::gfix::Gfix(0))).is_err());
        assert!(key_of(&mut vm, Value::String("APV\u{b}".into())).is_err());
    }

    /// Inventory transfer: the item leaves its carrier's attributes and the
    /// register, and its taker gains both, at 0.
    #[test]
    fn a_transfer_moves_the_key_and_the_register_entry() {
        let mut world = world();
        let ball = string_id("MagicBall");

        assert!(world.give_item(13, ball, Copies::Once).is_ok());
        world
            .pieces
            .get_mut(13)
            .map(|p| p.attributes.insert(ball, 7));

        world.take_item(13, ball);
        world.carry(14, ball);

        assert!(!world.pieces[&13].attributes.contains_key(&ball));
        assert_eq!(world.pieces[&14].attributes.get(&ball), Some(&0));
        assert_eq!(world.carriers(ball), vec![140]);
    }

    /// Inventory prune then inventory nth carrier: a carrier no unit has goes
    /// before the first is read, as does the next key's first.
    #[test]
    fn a_lookup_drops_carriers_no_unit_has() {
        let mut world = world();

        world.agents.insert(13, super::super::Agent::default());
        world.level.items = Register::of(&[
            (slot(11).cast_unsigned(), 999),
            (slot(11).cast_unsigned(), 130),
            (slot(12).cast_unsigned(), 998),
            (slot(12).cast_unsigned(), 130),
        ]);

        assert_eq!(world.first_carrier(slot(11)), Some(13));
        assert_eq!(world.carriers(slot(11)), vec![130]);
        assert_eq!(world.carriers(slot(12)), vec![130]);
    }

    /// Inventory count: the carriers plus one, and 1 for a key none
    /// carries below one that is carried.
    #[test]
    fn a_count_is_one_past_the_carriers() {
        let mut world = world();

        world.agents.insert(13, super::super::Agent::default());
        world.agents.insert(14, super::super::Agent::default());

        assert_eq!(world.count_of(slot(11)), 0);

        assert!(world.give_item(13, slot(11), Copies::Once).is_ok());
        assert!(world.give_item(14, slot(11), Copies::Cloned).is_ok());

        assert_eq!(world.count_of(slot(11)), 3);
        assert_eq!(world.count_of(slot(10)), 1);
        assert_eq!(world.count_of(slot(12)), 0);
    }

    /// Is item key, item drop type, item agent type for an `INV:n`
    /// key and a key that is no item.
    #[test]
    fn a_slot_drops_as_resource_items() {
        let world = world();

        assert_eq!(world.is_item_key(slot(11)), Some(true));
        assert_eq!(world.drop_type(slot(11)), Some(1));
        assert_eq!(world.agent_type(slot(11)).as_deref(), Some(DROPPED_AS));

        assert_eq!(world.is_item_key(EXPERIENCE_LEVEL.packed()), Some(false));
        assert_eq!(world.drop_type(EXPERIENCE_LEVEL.packed()), Some(0));
        assert_eq!(
            world.agent_type(EXPERIENCE_LEVEL.packed()).as_deref(),
            Some(DROPPED_AS)
        );
    }

    /// The world as a free game's host: what it answers, else nothing.
    struct Free<'w>(&'w mut World);

    impl crate::gpl::vm::Host for Free<'_> {
        fn builtin(
            &mut self,
            vm: &mut Vm<'_>,
            call: &crate::gpl::vm::Call<'_>,
        ) -> Result<(), Fault> {
            if Checked::of_id(call.builtin).is_some() {
                self.0
                    .run(
                        vm,
                        call,
                        &|_, _, _| None,
                        crate::sim::world::step::Draws::Kept,
                    )
                    .map(|_| ())
            } else {
                Ok(())
            }
        }

        fn attribute(&mut self, _: &mut Vm<'_>, _: u32, name: &str) -> Result<ObjId, Fault> {
            Err(Fault::throw(name))
        }

        fn named(&mut self, _: &str) -> Value {
            Value::Integer(0)
        }

        fn log(&mut self, _: &str) {}
    }

    /// `Hero_Drop_Quest_Items`' loop, as a dying hero runs it: each item
    /// the search finds is deleted, so the search ends. A world that kept
    /// the item would loop for ever.
    #[test]
    fn a_dying_heros_items_are_deleted_one_by_one() {
        const SOURCE: &str = "function Give(agent a)\nbegin\n\
             \t$CreateNewInventoryItem(11, a);\n\t$CreateNewInventoryItem(5, a);\nend\n\
             function Drop(agent a) is integer\ndeclare\n\tinteger w;\n\tinteger n;\nbegin\n\
             \tw = -1;\n\twhile ($AgentHasInventoryItem(w, a)) do\n\t\tbegin\n\
             \t\t\t$DeleteInventoryItem(w, a);\n\t\t\tn = n + 1;\n\t\t\tw = -1;\n\t\tend\n\
             \treturn n;\nend\n";

        let bytes = crate::gpl::emit::compile_source(SOURCE, "test.gpl").expect("compiles");
        let program = crate::gpl::load::Program::load(&[bytes]).expect("loads");
        let mut vm = Vm::new(&program, crate::gpl::vm::Config::default());
        let mut world = world();
        let def = |name| match program.entry(name) {
            Some(crate::gpl::load::Entry::Script(d)) => d,
            _ => panic!("{name}"),
        };
        let hero = Value::Agent(13);

        world.agents.insert(13, super::super::Agent::default());
        vm.set_limit(Some(100_000));

        let mut host = Free(&mut world);
        let catcher = crate::gpl::runtime::Catcher::Thread;
        let given = vm.run(&mut host, def("Give"), std::slice::from_ref(&hero), catcher);
        let dropped = vm.run(&mut host, def("Drop"), &[hero], catcher);

        assert!(given.is_ok_and(|r| r.error.is_none()));
        assert_eq!(dropped.map(|r| r.result), Ok(Value::Integer(2)));
        assert_eq!(world.first_item(130), None);
        assert!(world.level.items.entries().values().all(Vec::is_empty));
        assert!(world.pieces[&13].attributes.is_empty());
    }

    /// The SDK's Dorgo picks the ball up (`DorgsBasicScript`): the ball's
    /// unit gives it up, Dorgo finds himself its holder, and the item's
    /// key names its transfer script's third argument.
    #[test]
    fn dorgo_takes_the_magic_ball() {
        const SOURCE: &str = "function Take(agent ball, agent dorgo) is agent\nbegin\n\
             \t$CreateNewInventoryItem(\"MagicBall\", ball);\n\
             \t$TransferInventoryItem(\"MagicBall\", ball, dorgo);\n\
             \treturn $FindInventoryItem(\"MagicBall\");\nend\n\
             function Key() is integer\nbegin\n\
             \treturn $MakeInventoryAttribute(\"MagicBall\");\nend\n";

        let bytes = crate::gpl::emit::compile_source(SOURCE, "test.gpl").expect("compiles");
        let program = crate::gpl::load::Program::load(&[bytes]).expect("loads");
        let mut vm = Vm::new(&program, crate::gpl::vm::Config::default());
        let mut world = world();
        let def = |name| match program.entry(name) {
            Some(crate::gpl::load::Entry::Script(d)) => d,
            _ => panic!("{name}"),
        };

        world.agents.insert(13, super::super::Agent::default());
        world.agents.insert(14, super::super::Agent::default());

        let mut host = Free(&mut world);
        let catcher = crate::gpl::runtime::Catcher::Thread;
        let args = [Value::Agent(13), Value::Agent(14)];
        let holder = vm.run(&mut host, def("Take"), &args, catcher);
        let item = vm.run(&mut host, def("Key"), &[], catcher);
        let ball = string_id("MagicBall");

        assert_eq!(holder.map(|r| r.result), Ok(Value::Agent(14)));
        assert_eq!(item.map(|r| r.result), Ok(Value::Integer(ball)));
        assert_eq!(world.carriers(ball), vec![140]);
    }
}
