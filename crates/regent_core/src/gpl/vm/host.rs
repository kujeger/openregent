//! What the VM asks of the world: builtins it does not run itself, agent
//! attributes, `#Name` values, the log.
//!
//! A builtin gets the engine's convention: `args[0]` is the return slot,
//! already holding a value of the registered type (reset since the last
//! call); argument `i` is `args[i]`, the caller's own handle, so a builtin
//! may write it back. A builtin may run scripts at once through
//! [`Vm::dispatch`]: engine callbacks nest, free of budget.

use super::{Fault, Handle, ObjId, Vm};
use crate::data::attrib;
use crate::gpl::builtins::{Builtin, BuiltinId};
use crate::gpl::ops::Value;

/// Gpl agentref attribute's throw when the agent does not exist,
/// `"Tried to get attribute "%s"from a non-existent agent "`, spacing as in the image.
pub const NO_AGENT: &str = "Tried to get attribute \"%s\"from a non-existent agent ";

/// Where `#ATTRIB_` names end and the attribute's own name starts.
pub const ATTRIB_PREFIX: &str = "ATTRIB_";

/// `#ATTRIB_Name`, with or without its `#`: the attribute's key as a save
/// stores it, bank then index, read as a little-endian word.
///
/// **Game**: the day-1 trace's `$Concatenate(#ATTRIB_ReturnAmount, …)`
/// prints 810963009, `0x30565041`: `APV` and 48, `ReturnAmount`'s key.
///
/// ```
/// use regent_core::gpl::vm::host::registry;
///
/// assert_eq!(registry("#ATTRIB_ReturnAmount"), Some(0x3056_5041));
/// assert_eq!(registry("attrib_pickupamount"), Some(0x3156_5041));
/// assert_eq!(registry("ReturnAmount"), None);
/// ```
#[must_use]
pub fn registry(name: &str) -> Option<i32> {
    let name = name.strip_prefix('#').unwrap_or(name);
    let attribute = name
        .get(..ATTRIB_PREFIX.len())
        .filter(|head| head.eq_ignore_ascii_case(ATTRIB_PREFIX))
        .and_then(|_| name.get(ATTRIB_PREFIX.len()..))
        .and_then(|rest| {
            attrib::EXPANSION
                .iter()
                .find(|(_, _, n)| n.eq_ignore_ascii_case(rest))
                .map(|(bank, index, _)| (**bank, *index))
        });
    let other = || {
        NAMED
            .iter()
            .find(|(n, _, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, bank, index)| (*bank, *index))
    };

    attribute
        .or_else(other)
        .map(|(bank, index)| i32::from_le_bytes([bank[0], bank[1], bank[2], index]))
}

/// The registry's names that are no attribute: attribute register's
/// ten other callers, start-up code, so a
/// `#Name` no script defines may resolve to one. **Binary**.
///
/// ```
/// use regent_core::gpl::vm::host::registry;
///
/// // Hide's enter-at-once flag: bank `GPL`, index 1.
/// assert_eq!(registry("#TeleportInsideDestination"), Some(0x014c_5047));
/// ```
pub const NAMED: [(&str, [u8; 3], u8); 10] = [
    ("WalkToDestination", *b"GPL", 0),
    ("TeleportInsideDestination", *b"GPL", 1),
    ("INVENTORY_ITEM_SPECIFIER", *b"INV", 0),
    ("WizardID", *b"ABY", 0),
    ("AgrelaID", *b"ABO", 0),
    ("DuarosID", *b"ABP", 0),
    ("FervusID", *b"ABQ", 0),
    ("KryptaID", *b"ABT", 0),
    ("LunordID", *b"ABU", 0),
    ("HeliaID", *b"ABR", 0),
];

/// One builtin call.
#[derive(Clone, Copy, Debug)]
pub struct Call<'a> {
    pub builtin: BuiltinId,
    /// Slot 0 the return value, then the arguments.
    pub args: &'a [Handle],
}

impl Call<'_> {
    #[must_use]
    pub const fn builtin(&self) -> &'static Builtin {
        self.builtin.get()
    }
}

/// The world, as the VM sees it.
pub trait Host {
    /// Run a builtin the VM does not.
    ///
    /// # Errors
    ///
    /// What the builtin throws, or a crash.
    fn builtin(&mut self, vm: &mut Vm<'_>, call: &Call<'_>) -> Result<(), Fault>;

    /// `agent's "name"`: the attribute's own object, which the result slot
    /// then holds, so a write through it lands in the attribute.
    ///
    /// # Errors
    ///
    /// The engine throws for a missing agent or attribute.
    fn attribute(&mut self, vm: &mut Vm<'_>, agent: u32, name: &str) -> Result<ObjId, Fault>;

    /// A `#Name` no script constant defines: the attribute registry, then
    /// descriptors.
    fn named(&mut self, name: &str) -> Value;

    /// A line of the engine's log (`-debugout`): `DebugOut`, script errors.
    fn log(&mut self, text: &str);

    /// An agent's name, for its text; `None` prints
    /// [`super::text::FREE_AGENT_TEXT`].
    fn agent_name(&mut self, id: u32) -> Option<String> {
        let _ = id;
        None
    }
}
