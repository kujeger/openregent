//! GPL's value types, as the engine names them.
//!
//! [`TypeCode`] is every code a record or value carries; [`Kind`] the nine
//! a value can have.
//!
//! **Binary**: eleven packed names in `.rodata`, after the path
//! `gplprocessor/src/gpltype_utils.cpp`.
//!
//! ```text
//! integer float string list agent boolean coord function reference
//! undefined unknowntype
//! ```
//!
//! # The numbering is one-based
//!
//! A section-1 record's first value slot types its value
//! ([`crate::gpl::bcd::Slot::kind`]): 1 for integer, 2 for float; the list
//! opens `integer`, `float`. Also:
//!
//! - Across the 36,816 records of every constant and prototype on disk the
//!   field takes ten distinct values, all in 1..=11. Nothing uses
//!   `reference`.
//!
//! See `every_type_a_record_carries_is_in_the_engines_list`.
//!
//! # A second field of the same alphabet
//!
//! A record's opening slot, [`crate::gpl::bcd::Mark`], tags it with
//! `unknowntype`, `undefined`, or 12, one past the list: [`TypeCode::NoType`].

/// The engine's value-type names, in `.rodata` order.
///
/// Index is the record's number minus one. Use [`name`].
pub const TABLE: [&str; 11] = [
    "integer",
    "float",
    "string",
    "list",
    "agent",
    "boolean",
    "coord",
    "function",
    "reference",
    "undefined",
    "unknowntype",
];

/// What a record's type byte names, or `None` if it names nothing.
///
/// One-based: zero is no type here. It occurs as a literal's type, the
/// shared null ([`crate::gpl::exec::Literal::Null`]).
///
/// ```
/// use regent_core::data::gpltype;
///
/// assert_eq!(gpltype::name(1), Some("integer"));
/// assert_eq!(gpltype::name(2), Some("float"));
/// assert_eq!(gpltype::name(11), Some("unknowntype"));
/// assert_eq!(gpltype::name(0), None);
/// assert_eq!(gpltype::name(12), None);
/// ```
#[must_use]
pub fn name(code: u8) -> Option<&'static str> {
    TABLE.get(usize::from(code).checked_sub(1)?).copied()
}

/// Every type code a record or value carries: 0, the eleven names, and 12.
///
/// ```text
/// 0      Null         the shared GplNull; no name
/// 1..=8  Integer ...  the value classes: see Kind
/// 9      Reference    a #Name operand
/// 10     Undefined    a slot pointing at another record's result
/// 11     Unknown      unknowntype: an unused slot, no value
/// 12     NoType       one past the names
/// ```
///
/// `NoType` is what gpl execute passes for an empty slot or a missing
/// operand, and the result-type table's "no type"; the compiler marks a
/// record computing a fresh value with it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum TypeCode {
    Null,
    Integer,
    Float,
    String,
    List,
    Agent,
    Boolean,
    Coord,
    Function,
    Reference,
    Undefined,
    Unknown,
    NoType,
}

impl TypeCode {
    /// All 13, index = code.
    pub const ALL: [TypeCode; 13] = [
        TypeCode::Null,
        TypeCode::Integer,
        TypeCode::Float,
        TypeCode::String,
        TypeCode::List,
        TypeCode::Agent,
        TypeCode::Boolean,
        TypeCode::Coord,
        TypeCode::Function,
        TypeCode::Reference,
        TypeCode::Undefined,
        TypeCode::Unknown,
        TypeCode::NoType,
    ];

    #[must_use]
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// The code `code`, in a `const` table; fails to compile on one past
    /// [`TypeCode::NoType`].
    ///
    /// # Panics
    ///
    /// On a code past 12.
    #[must_use]
    pub const fn of(code: u8) -> TypeCode {
        assert!((code as usize) < TypeCode::ALL.len(), "a type code");
        TypeCode::ALL[code as usize]
    }

    /// The engine's name; `None` for [`TypeCode::Null`] and
    /// [`TypeCode::NoType`].
    ///
    /// ```
    /// use regent_core::data::gpltype::TypeCode;
    ///
    /// assert_eq!(TypeCode::Coord.name(), Some("coord"));
    /// assert_eq!(TypeCode::Null.name(), None);
    /// ```
    #[must_use]
    pub fn name(self) -> Option<&'static str> {
        name(self.code())
    }
}

/// A code past [`TypeCode::NoType`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnknownType(pub u8);

impl TryFrom<u8> for TypeCode {
    type Error = UnknownType;

    fn try_from(code: u8) -> Result<TypeCode, UnknownType> {
        TypeCode::ALL
            .get(usize::from(code))
            .copied()
            .ok_or(UnknownType(code))
    }
}

/// A value class: the codes a value can have, 0 to 8.
///
/// **Binary**: gpl value pool grow builds codes 1 to 8
/// only; 9 to 11 share a pool it never fills, so no value has them. Code 0 is
/// `GplNull`, the shared value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum Kind {
    Null = TypeCode::Null as u8,
    Integer = TypeCode::Integer as u8,
    Float = TypeCode::Float as u8,
    String = TypeCode::String as u8,
    List = TypeCode::List as u8,
    Agent = TypeCode::Agent as u8,
    Boolean = TypeCode::Boolean as u8,
    Coord = TypeCode::Coord as u8,
    Function = TypeCode::Function as u8,
}

impl Kind {
    /// Every class, by code.
    pub const ALL: [Kind; 9] = [
        Kind::Null,
        Kind::Integer,
        Kind::Float,
        Kind::String,
        Kind::List,
        Kind::Agent,
        Kind::Boolean,
        Kind::Coord,
        Kind::Function,
    ];

    /// The class a type code names; `None` for one with no class.
    #[must_use]
    pub fn from_code(code: u8) -> Option<Kind> {
        Kind::ALL.get(usize::from(code)).copied()
    }

    /// The type code, a `GplType`'s word.
    #[must_use]
    pub const fn code(self) -> u8 {
        self as u8
    }

    #[must_use]
    pub const fn type_code(self) -> TypeCode {
        TypeCode::of(self.code())
    }

    /// The engine's name; `None` for [`Kind::Null`].
    #[must_use]
    pub fn name(self) -> Option<&'static str> {
        self.type_code().name()
    }

    /// The name in its "Unsupported operation" message; `GplNull` has none.
    #[must_use]
    pub const fn shout(self) -> Option<&'static str> {
        match self {
            Kind::Null => None,
            Kind::Integer => Some("INTEGER"),
            Kind::Float => Some("FLOAT"),
            Kind::String => Some("STRING"),
            Kind::List => Some("LIST"),
            Kind::Agent => Some("AGENT"),
            Kind::Boolean => Some("BOOLEAN"),
            Kind::Coord => Some("COORD"),
            Kind::Function => Some("FUNCTION"),
        }
    }
}

impl From<Kind> for TypeCode {
    fn from(kind: Kind) -> TypeCode {
        kind.type_code()
    }
}

impl TryFrom<TypeCode> for Kind {
    type Error = UnknownType;

    fn try_from(code: TypeCode) -> Result<Kind, UnknownType> {
        Kind::from_code(code.code()).ok_or(UnknownType(code.code()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_code_is_its_own_number_and_names_the_table() {
        for (ty, code) in TypeCode::ALL.iter().zip(0u8..) {
            assert_eq!(ty.code(), code);
            assert_eq!(TypeCode::try_from(code), Ok(*ty));
            assert_eq!(ty.name(), name(code));
        }

        assert_eq!(TypeCode::try_from(13), Err(UnknownType(13)));
        assert_eq!(TypeCode::Unknown.name(), Some("unknowntype"));
        assert_eq!(TypeCode::NoType.name(), None);
    }

    #[test]
    fn a_class_is_the_code_of_its_type() {
        for kind in Kind::ALL {
            assert_eq!(Kind::try_from(kind.type_code()), Ok(kind));
            assert_eq!(kind.type_code().code(), kind.code());
        }

        assert!(Kind::try_from(TypeCode::Reference).is_err());
    }
}
