//! A piece's state bits and its owner slot.

/// The owner slot of things belonging to no kingdom: slot 7.
///
/// **Game**: The Day of Reckoning's eleven buildings here are neutrals to
/// discover (seven temples, `ABb1 Dwarven_Settlement`, `ABc1 Elven_Bungalow`,
/// `ABf1 Gnome_Hovel`, `ABh1 Royal_Gardens`), not monsters.
pub const UNALIGNED: u32 = 7;

/// State bit: in ruins.
///
/// Set on every building killed in play (`BBx1 RatsNest` at −3 hit points)
/// and on `BBM1 Ruined_Shrine1` from the start. Not "hit points gone":
/// opened treasure chests sit at zero without it.
pub const RUINED: u8 = 0x10;

/// The owner of a record that names no player.
pub const NOBODY: u32 = u32::MAX;

/// State bit: selected by the player.
///
/// Combines with others: a selected, held hero reads `0x03`. Not "building
/// working": lit buildings carry no state bit.
pub const SELECTED: u8 = 0x01;

/// State bit: held by another piece, so not drawn itself.
///
/// A hero in a guild, a goblin in a warband. An overlay or flag hung on a
/// unit has a holder but not this bit, unless its holder is held: then it
/// is too.
pub const HELD: u8 = 0x02;

/// State bit: its descriptor's `Info` bit `DontBlock`, set as the unit takes
/// its descriptor. Such a unit is never on the collision grid.
pub const DONT_BLOCK: u8 = 0x20;
