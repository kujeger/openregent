//! GPL contents of a `.bcd` image's blocks.
//!
//! [`crate::data::bcd`] owns the container's three-section layout, and
//! [`crate::data::block`] the block framing it shares with saves' GPL chunk;
//! network messages are unchecked.
//!
//! # Section 3, data
//!
//! A project's compiled `.dat` files:
//!
//! ```text
//! [u32 1] [u32 nprotos]
//!   ( cstr instance_name
//!     [u32 len] [u32 1] cstr shape_name [u32 npairs]
//!       ( cstr key  cstr value )*
//!     [u32 0] [i32 -1])
//!   *
//! ```
//!
//! **Corpus**: 75 files, 3,101 prototypes, 43,977 values, all re-encoding
//! byte-identically.
//!
//! # Section 2, scopes
//!
//! Three name tables end to end, tiling the section:
//!
//! ```text
//! [u32 1]
//! [u32 nprototypes]  ( cstr name   [u32 len] body [u32 0][i32 -1] )*
//! [u32 nfunctions]   ( cstr name   [u32 len] body [u32 0][i32 -1] )*
//! [u32 nconstants]   ( cstr #name  [u32 len] body [u32 0][i32 -1] )*
//! ```
//!
//! **Corpus**: 253 prototypes, 11,464 functions, 7,741 constants, all
//! re-encoding byte-identically. Each table sorts by byte order
//! (`Advanced_Attack` before `adept_birth`); section 1 sorts case-insensitively.
//!
//! A body is a [`Scope`], not code: the compiler's two symbol arenas when it
//! finished the definition. `Cultist_tree`: 26 statements, two entries;
//! `end_game_script_expert`: five and 449.
//!
//! # Section 1, compiled definitions
//!
//! One entry per name, case-insensitive order, tiling the section. This is the
//! code: entry size tracks expression count at r = +0.988 over the base game's
//! 845 functions and prototypes; section 2's record count tracks nothing.
//!
//! Also the debug information, inline as C strings:
//!
//! - source file, Windows spelling, `TaskModules\SubTasks\Spells.gpl`;
//! - declaration line;
//! - for a function: `__Return_value__`, parameters, locals, called builtins.
//!
//! Each entry is `[cstr name][u32 nrecords]` then [`Record`]s. A record ends
//! `[u16 line][u8 n][u16 slot × n][u8 opcode]`, opcode indexing
//! [`super::opcode::TABLE`]; a call ends in [`super::opcode::CALL`]. On one
//! function with source, the opcodes read `opEqual` on the line of its only
//! `==` and `opIf` on the line of its only `if`.
//!
//! [`walk`] reads the section on all 75 files; [`encode`] writes it back
//! byte-identically. The record shape comes from `Gplbcc.exe`'s writer, not a
//! scan: a scan resynchronises on padding, since zeroes form a valid record
//! (`opAssignEqual` is opcode 0). The walk searches for nothing and must end on
//! the section's last byte.

use std::borrow::Cow;
use std::collections::BTreeSet;

use crate::data::block::{self, Block};
use crate::data::cursor::Cursor;
use crate::data::error::{Error, Result, u32_at};
use crate::data::gpltype::TypeCode;

/// Opens the data section and each prototype in it. Meaning unknown; `1` in
/// every file, so the reader requires it.
const DATA_FLAG: u32 = 1;

/// Opens section 2. Also `1`, unexplained. Kept apart from [`DATA_FLAG`]:
/// different fields that happen to agree.
const SCOPE_FLAG: u32 = 1;

/// Constant name sigil; the only thing telling section 2's third table from
/// its second.
const CONSTANT_SIGIL: char = '#';

/// One `[Name] { Shape (key value)... }` instance, as stored in section 3.
///
/// [`Cow`] strings: borrowed when read, owned when rendered for writing (a
/// `.dat` boolean lowercased, an integer as a number).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proto<'a> {
    /// The instance name.
    pub name: Cow<'a, str>,
    /// The `.gpl` `prototype` this instantiates.
    pub shape: Cow<'a, str>,
    /// Sorted by key, case-insensitively. Text, whatever the declared type.
    pub values: Vec<(Cow<'a, str>, Cow<'a, str>)>,
}

/// Section 3: the compiled `.dat` files.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DataSection<'a> {
    /// Sorted by name, case-insensitively, across all the project's `.dat`
    /// files.
    pub prototypes: Vec<Proto<'a>>,
}

impl<'a> DataSection<'a> {
    /// Decode the body of a [`crate::data::bcd::Sections::data`] block.
    ///
    /// # Errors
    ///
    /// Section flag not 1; unterminated string; malformed block; a declared
    /// prototype or pair count not matching the body.
    pub fn decode(body: &'a [u8]) -> Result<Self> {
        let mut cur = Cursor::new(body);
        expect_flag(
            cur.u32("data section flag")?,
            DATA_FLAG,
            "data section flag",
        )?;

        let count = cur.u32("data prototype count")? as usize;
        let mut prototypes = Vec::new();

        for _ in 0..count {
            // The instance name sits outside the block holding the rest.
            let name = cur.str("prototype name")?;
            let block = Block::at(cur.data(), "prototype block", cur.at())?;
            cur.skip("prototype block", block.len)?;

            prototypes.push(decode_proto(name, block.body)?);
        }

        if cur.at() == body.len() {
            Ok(DataSection { prototypes })
        } else {
            Err(Error::BrokenInvariant {
                what: "data section fills its block",
                expected: body.len() as u64,
                found: cur.at() as u64,
            })
        }
    }

    /// Encode the body for [`crate::data::bcd::write_program`].
    ///
    /// # Panics
    ///
    /// A prototype or pair count over `u32::MAX`.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&DATA_FLAG.to_le_bytes());
        let count = u32::try_from(self.prototypes.len()).expect("prototype count fits a u32");
        out.extend_from_slice(&count.to_le_bytes());

        for proto in &self.prototypes {
            push_cstr(&mut out, &proto.name);

            block::write(&mut out, |out| {
                out.extend_from_slice(&DATA_FLAG.to_le_bytes());
                push_cstr(out, &proto.shape);
                let pairs = u32::try_from(proto.values.len()).expect("pair count fits a u32");
                out.extend_from_slice(&pairs.to_le_bytes());

                for (key, value) in &proto.values {
                    push_cstr(out, key);
                    push_cstr(out, value);
                }
            });
        }

        out
    }
}

// --------------------------------------------------------------------------
// Section 2: name tables
// --------------------------------------------------------------------------

/// A section 2 definition: name and scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Definition<'a> {
    /// A constant keeps its `#`; a prototype and a function do not.
    pub name: &'a str,
    /// Block contents between length and terminator; see [`Definition::scope`].
    /// Owned when built by [`crate::gpl::emit`], borrowed when read.
    pub body: Cow<'a, [u8]>,
}

impl Definition<'_> {
    /// Decode the body.
    ///
    /// # Errors
    ///
    /// Body off the [`Scope`] layout. All 19,496 shipped bodies decode exactly.
    pub fn scope(&self) -> Result<Scope> {
        Scope::decode(&self.body)
    }
}

/// A definition's scope.
///
/// Three-word header, then two counted lists. Header plus counts is 20 bytes,
/// so the body length is a multiple of 20:
///
/// ```text
/// [u32 1] [u32 returns] [u32 kind]
/// [u32 n]  ( block )*      the declares
/// [u32 m]  ( block )*      the parameters
/// ```
///
/// A block is the usual `[u32 len][body][u32 0][i32 -1]` framing over
/// `[u32 1][u32 type]`: 20 bytes.
///
/// The lists accumulate across a source file: every name of their kind
/// declared so far, this definition's included. A late function in a long file
/// carries hundreds. See `section_two_records_count_the_names_in_scope`.
///
/// Entries hold only a type, no name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Scope {
    /// A function's `is <type>`, 0 without one. [`TypeCode::Unknown`] on a
    /// prototype or constant: also what an unused section 1 value slot
    /// carries.
    pub returns: u8,
    /// Which of section 2's three tables holds it.
    pub kind: Kind,
    /// Type of every `declare`d name so far in the file: function locals and
    /// prototype members alike.
    pub declares: Vec<u8>,
    /// Type of every parameter so far in the file.
    pub parameters: Vec<u8>,
}

/// A scope block: `[u32 20][u32 1][u32 type][u32 0][i32 -1]`.
const SCOPE_BLOCK: u32 = 20;

/// First word of a scope block body.
const SCOPE_BLOCK_FLAG: u32 = 1;

impl Scope {
    /// Read one definition body.
    ///
    /// # Errors
    ///
    /// Malformed header; a block off the fixed shape; the lists not consuming
    /// the body exactly. The last matters most, as for [`walk`]: a wrong split
    /// would still decode.
    pub fn decode(body: &[u8]) -> Result<Self> {
        let mut cur = Cursor::new(body);

        expect_flag(cur.u32("scope flag")?, SCOPE_BLOCK_FLAG, "scope flag")?;

        let returns = truncate(cur.u32("scope return type")?);
        let kind = Kind::from_code(cur.u32("scope kind")?)?;
        let declares = decode_names(&mut cur, "declares")?;
        let parameters = decode_names(&mut cur, "parameters")?;

        if cur.at() == body.len() {
            Ok(Scope {
                returns,
                kind,
                declares,
                parameters,
            })
        } else {
            Err(Error::BrokenInvariant {
                what: "a scope fills its body",
                expected: body.len() as u64,
                found: cur.at() as u64,
            })
        }
    }

    /// Inverse of [`Scope::decode`].
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&SCOPE_BLOCK_FLAG.to_le_bytes());
        out.extend_from_slice(&u32::from(self.returns).to_le_bytes());
        out.extend_from_slice(&self.kind.code().to_le_bytes());

        for list in [&self.declares, &self.parameters] {
            let count = u32::try_from(list.len()).unwrap_or_default();
            out.extend_from_slice(&count.to_le_bytes());

            for ty in list {
                block::write(&mut out, |out| {
                    out.extend_from_slice(&SCOPE_BLOCK_FLAG.to_le_bytes());
                    out.extend_from_slice(&u32::from(*ty).to_le_bytes());
                });
            }
        }

        out
    }

    /// Names in both lists: body length in 20-byte units, less one for the
    /// header and counts.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.declares.len() + self.parameters.len()
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// `[u32 count]` then that many fixed blocks, each holding a type.
fn decode_names(cur: &mut Cursor<'_>, what: &'static str) -> Result<Vec<u8>> {
    let count = cur.u32(what)? as usize;
    let mut out = Vec::new();

    for _ in 0..count {
        expect_flag(cur.u32(what)?, SCOPE_BLOCK, "scope block length")?;
        expect_flag(cur.u32(what)?, SCOPE_BLOCK_FLAG, "scope block flag")?;

        out.push(truncate(cur.u32(what)?));

        expect_flag(cur.u32(what)?, 0, "scope block terminator")?;
        expect_flag(cur.u32(what)?, u32::MAX, "scope block terminator")?;
    }

    Ok(out)
}

/// A type index; always fits a byte.
fn truncate(word: u32) -> u8 {
    u8::try_from(word).unwrap_or(u8::MAX)
}

/// Section 2: every name the project defines, with its scope.
///
/// Counts match the sources: `data/bytecode.bcd` holds 20, 825, 649, as the
/// base game's `.gpl` files do.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScopeSection<'a> {
    pub prototypes: Vec<Definition<'a>>,
    pub functions: Vec<Definition<'a>>,
    pub constants: Vec<Definition<'a>>,
}

impl<'a> ScopeSection<'a> {
    /// Decode the body of a [`crate::data::bcd::Sections::code`] block.
    ///
    /// # Errors
    ///
    /// Section flag not 1; unterminated name; malformed definition block; a
    /// constant without its sigil; the tables not filling the section exactly.
    /// The last matters most: it rejects a plausible prefix.
    pub fn decode(body: &'a [u8]) -> Result<Self> {
        let mut cur = Cursor::new(body);
        expect_flag(
            cur.u32("code section flag")?,
            SCOPE_FLAG,
            "code section flag",
        )?;

        let prototypes = decode_table(&mut cur, "prototype")?;
        let functions = decode_table(&mut cur, "function")?;
        let constants = decode_table(&mut cur, "constant")?;

        // A missing sigil means the walk drifted a table sideways.
        if let Some(odd) = constants
            .iter()
            .find(|d| !d.name.starts_with(CONSTANT_SIGIL))
        {
            return Err(Error::BadMagic {
                what: "constant name sigil",
                found: odd.name.as_bytes().to_vec(),
            });
        }

        if cur.at() == body.len() {
            Ok(ScopeSection {
                prototypes,
                functions,
                constants,
            })
        } else {
            Err(Error::BrokenInvariant {
                what: "code section fills its block",
                expected: body.len() as u64,
                found: cur.at() as u64,
            })
        }
    }

    /// Encode the body for [`crate::data::bcd::write_program`]. Bodies come
    /// from [`ScopeSection::decode`] or [`crate::gpl::emit::scope_section`].
    ///
    /// # Panics
    ///
    /// A table over `u32::MAX` definitions.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&SCOPE_FLAG.to_le_bytes());

        for table in [&self.prototypes, &self.functions, &self.constants] {
            let count = u32::try_from(table.len()).expect("definition count fits a u32");
            out.extend_from_slice(&count.to_le_bytes());

            for definition in table {
                push_cstr(&mut out, definition.name);
                block::write(&mut out, |out| out.extend_from_slice(&definition.body));
            }
        }

        out
    }

    /// Every definition, prototypes then functions then constants.
    pub fn definitions(&self) -> impl Iterator<Item = &Definition<'a>> {
        self.prototypes
            .iter()
            .chain(&self.functions)
            .chain(&self.constants)
    }

    pub const fn len(&self) -> usize {
        self.prototypes.len() + self.functions.len() + self.constants.len()
    }

    pub const fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Distinct names, case-insensitive: what section 1 counts (see
    /// [`symbol_count`]). Differs from [`ScopeSection::len`] only where a name
    /// is defined twice, as in one Workshop mod.
    pub fn distinct_names(&self) -> usize {
        self.definitions()
            .map(|d| d.name.to_lowercase())
            .collect::<BTreeSet<_>>()
            .len()
    }
}

/// `[u32 count]` then that many `[cstr name][block]` entries.
fn decode_table<'a>(cur: &mut Cursor<'a>, what: &'static str) -> Result<Vec<Definition<'a>>> {
    let count = cur.u32(what)? as usize;
    let mut out = Vec::new();

    for _ in 0..count {
        // Name sits outside its block, as in the data section.
        let name = cur.str(what)?;
        let block = Block::at(cur.data(), what, cur.at())?;
        cur.skip(what, block.len)?;

        out.push(Definition {
            name,
            body: Cow::Borrowed(block.body),
        });
    }

    Ok(out)
}

// --------------------------------------------------------------------------
// Section 1: compiled definitions
// --------------------------------------------------------------------------
//
// Case-insensitive name order, so `#` constants come first.
//
// A constant is three records, a function a prologue and a body; see
// [`walk`], [`Record`] and `docs/formats/bytecode.md`. A float constant's value
// is its record's float literal, fixed point in the `extra` word
// ([`super::exec::Literal`]).

/// Which of section 2's three tables a definition is in.
///
/// Every body repeats it as [`Scope::kind`]; all 19,496 agree
/// (`a_definitions_scope_decodes_and_names_its_own_table`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Kind {
    #[default]
    Prototype,
    Function,
    Constant,
}

impl Kind {
    /// Number stored in the body: table order.
    #[must_use]
    pub const fn code(self) -> u32 {
        match self {
            Kind::Prototype => 0,
            Kind::Function => 1,
            Kind::Constant => 2,
        }
    }

    /// Inverse of [`Kind::code`].
    ///
    /// # Errors
    ///
    /// Anything but 0, 1 or 2. No shipped body has one.
    pub fn from_code(code: u32) -> Result<Self> {
        match code {
            0 => Ok(Kind::Prototype),
            1 => Ok(Kind::Function),
            2 => Ok(Kind::Constant),
            _ => Err(Error::BadMagic {
                what: "scope kind",
                found: code.to_le_bytes().to_vec(),
            }),
        }
    }
}

/// Names section 1 declares. Equals [`ScopeSection::distinct_names`] on every
/// file.
///
/// # Errors
///
/// Body under eight bytes, or leading flag word not zero.
pub fn symbol_count(body: &[u8]) -> Result<u32> {
    // Zero, where the other sections carry 1. Unexplained; checked likewise.
    expect_flag(
        u32_at(body, "symbol section flag", 0)?,
        0,
        "symbol section flag",
    )?;

    u32_at(body, "symbol count", 4)
}

fn decode_proto<'a>(name: &'a str, body: &'a [u8]) -> Result<Proto<'a>> {
    let mut cur = Cursor::new(body);
    expect_flag(cur.u32("prototype flag")?, DATA_FLAG, "prototype flag")?;

    let shape = cur.str("prototype shape")?;
    let count = cur.u32("prototype value count")? as usize;
    let mut values = Vec::new();

    for _ in 0..count {
        let key = cur.str("value key")?;
        values.push((Cow::Borrowed(key), Cow::Borrowed(cur.str("value")?)));
    }

    if cur.at() == body.len() {
        Ok(Proto {
            name: Cow::Borrowed(name),
            shape: Cow::Borrowed(shape),
            values,
        })
    } else {
        Err(Error::BrokenInvariant {
            what: "prototype fills its block",
            expected: body.len() as u64,
            found: cur.at() as u64,
        })
    }
}

fn expect_flag(found: u32, expected: u32, what: &'static str) -> Result<()> {
    if found == expected {
        Ok(())
    } else {
        Err(Error::BadValue {
            what,
            value: u64::from(found),
        })
    }
}

/// Typed value slots per record, after the mark.
const RECORD_VALUES: usize = 2;

/// Section 1's opening word; `Gplbcc.exe` passes it to the block writer as
/// the block tag. `0` in every file; the other sections have `1`.
const CODE_FLAG: u32 = 0;

/// A record's opening slot, written as a [`Slot`].
///
/// Over all 1,270,363 records only `word` and `tag` vary: the string is empty,
/// `extra` and `flag` are zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mark {
    /// 0 or [`OWN_SLOT`].
    pub word: u32,
    /// 1, 10, 11 or 12: a [`TypeCode`], 12 past the eleven names.
    pub tag: u8,
}

impl Mark {
    /// A record yielding neither place nor value.
    pub const NONE: Mark = Mark {
        word: 0,
        tag: TypeCode::Unknown.code(),
    };

    /// A record yielding a place: assignment and kin, an `opNop` with an
    /// operand, a declaration. Its result is its first operand.
    pub const PLACE: Mark = Mark {
        word: OWN_SLOT,
        tag: TypeCode::Undefined.code(),
    };

    /// A record computing a fresh value: an operator.
    pub const VALUE: Mark = Mark {
        word: 0,
        tag: TypeCode::NoType.code(),
    };
}

/// [`Mark::PLACE`]'s word: the record's own slot.
pub const OWN_SLOT: u32 = u32::MAX;

/// A record's line when the compiler wrote none.
pub const NO_LINE: u16 = u16::MAX;

/// `flag` on a reference operand: the referenced record's result slot,
/// the only flag a reference carries.
/// The compiler writes it on integer and boolean literals too, where the
/// engine sets the value from its type alone.
pub const RESULT_FLAG: u8 = 2;

/// A function prologue's record holding `__Return_value__`, its first
/// declaration; record 0's `head` names it. See [`crate::gpl::emit::prologue`].
pub const RETURN_RECORD: u32 = 4;

/// A function prologue's record whose `head` is the record count.
pub const COUNT_RECORD: usize = 3;

/// A constant's record holding its value, after the file and name records.
pub const CONSTANT_RECORD: usize = 2;

/// One of a record's two typed value slots.
///
/// `Gplbcc.exe` writes `[u32][u32][cstr][u8][u8]`: eleven bytes with an
/// empty string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Slot<'a> {
    pub word: u32,
    pub extra: u32,
    /// A string literal, or empty.
    pub text: &'a str,
    /// A [`TypeCode`]. [`TypeCode::Unknown`] on an empty slot.
    pub kind: u8,
    pub flag: u8,
}

impl Slot<'_> {
    /// An unused slot.
    pub const UNUSED: Slot<'static> = Slot {
        word: 0,
        extra: 0,
        text: "",
        kind: TypeCode::Unknown.code(),
        flag: 0,
    };
}

/// One compiled record: an operator, a call, or a declaration.
///
/// **Binary.** Shape from `Gplbcc.exe`'s writer. Fixed for every record, so the
/// stream is walkable:
///
/// ```text
/// [value][value][value]        three typed slots
/// [u32 head] [cstr text]       the symbol this record is about
/// [u16 line]                   the source line
/// [u8 n] [u16 slot × n]        the operands, first being the result
/// [u8 opcode]
/// ```
///
/// 41 bytes plus `text` when the value strings are empty and `n` is zero.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record<'a> {
    /// Opening slot: a header, not a value.
    pub mark: Mark,
    pub values: [Slot<'a>; RECORD_VALUES],
    pub head: u32,
    /// A symbol, a literal, or empty.
    pub text: &'a str,
    pub line: u16,
    /// Result slot, then operands: a call's arguments are `slots[1..]`.
    pub slots: Vec<u16>,
    /// Index into [`super::opcode::TABLE`], except 39 (one past its end),
    /// marking where a function body begins.
    pub opcode: u8,
}

/// A section 1 definition: name and records. Its section 2 counterpart is
/// [`Definition`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Compiled<'a> {
    pub name: &'a str,
    pub records: Vec<Record<'a>>,
}

/// Walk section 1, definition by definition, record by record.
///
/// No searching: counts and record shape give every length, so the walk
/// consumes the section exactly or fails. All 75 files walk.
///
/// ```text
/// [u32 0] [u32 ndefinitions]
///   ( cstr name  [u32 nrecords]  record* )*
/// ```
///
/// # Errors
///
/// Truncated section; a string not UTF-8; the walk not ending at the section
/// end. The last matters most: a wrong record shape drifts rather than fails.
pub fn walk(symbols: &[u8]) -> Result<Vec<Compiled<'_>>> {
    let mut cur = Cursor::new(symbols);

    expect_flag(cur.u32("section 1 tag")?, CODE_FLAG, "section 1 tag")?;

    let count = cur.u32("section 1 definitions")? as usize;
    let mut out = Vec::new();

    for _ in 0..count {
        let name = cur.str("definition name")?;
        let records = cur.u32("definition records")? as usize;

        out.push(Compiled {
            name,
            records: (0..records)
                .map(|_| record(&mut cur))
                .collect::<Result<Vec<_>>>()?,
        });
    }

    // Any other end position means a misread record.
    if cur.at() == symbols.len() {
        Ok(out)
    } else {
        Err(Error::BrokenInvariant {
            what: "section 1 records tile the section",
            expected: symbols.len() as u64,
            found: cur.at() as u64,
        })
    }
}

/// Re-encode section 1; inverse of [`walk`].
///
/// Writes every field read, including the unexplained `head`, `extra` and
/// `flag`, so the round trip checks completeness.
///
/// Counts over `u32` (impossible from a file) write zero rather than panic.
#[must_use]
pub fn encode(definitions: &[Compiled<'_>]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&CODE_FLAG.to_le_bytes());

    let count = u32::try_from(definitions.len()).unwrap_or_default();
    out.extend_from_slice(&count.to_le_bytes());

    for definition in definitions {
        push_cstr(&mut out, definition.name);

        let records = u32::try_from(definition.records.len()).unwrap_or_default();
        out.extend_from_slice(&records.to_le_bytes());

        for record in &definition.records {
            // Mark: a value's eleven bytes, unused fields zero.
            out.extend_from_slice(&record.mark.word.to_le_bytes());
            out.extend_from_slice(&0u32.to_le_bytes());
            out.push(0);
            out.push(record.mark.tag);
            out.push(0);

            for value in &record.values {
                out.extend_from_slice(&value.word.to_le_bytes());
                out.extend_from_slice(&value.extra.to_le_bytes());
                push_cstr(&mut out, value.text);
                out.push(value.kind);
                out.push(value.flag);
            }

            out.extend_from_slice(&record.head.to_le_bytes());
            push_cstr(&mut out, record.text);
            out.extend_from_slice(&record.line.to_le_bytes());
            out.push(u8::try_from(record.slots.len()).unwrap_or_default());

            for slot in &record.slots {
                out.extend_from_slice(&slot.to_le_bytes());
            }

            out.push(record.opcode);
        }
    }

    out
}

fn push_cstr(out: &mut Vec<u8>, s: &str) {
    out.extend_from_slice(s.as_bytes());
    out.push(0);
}

/// Opening slot: a value with only two fields set.
fn mark(cur: &mut Cursor<'_>) -> Result<Mark> {
    let value = value(cur)?;

    Ok(Mark {
        word: value.word,
        tag: value.kind,
    })
}

/// `[u32][u32][cstr][u8][u8]`.
fn value<'a>(cur: &mut Cursor<'a>) -> Result<Slot<'a>> {
    Ok(Slot {
        word: cur.u32("value word")?,
        extra: cur.u32("value extra")?,
        text: cur.str("value text")?,
        kind: cur.u8("value type")?,
        flag: cur.u8("value flag")?,
    })
}

/// Three value slots, then the instruction.
fn record<'a>(cur: &mut Cursor<'a>) -> Result<Record<'a>> {
    let mark = mark(cur)?;
    let values = [value(cur)?, value(cur)?];
    let head = cur.u32("record head")?;
    let text = cur.str("record text")?;
    let line = cur.u16("record line")?;
    let operands = cur.u8("record operands")? as usize;

    Ok(Record {
        mark,
        values,
        head,
        text,
        line,
        slots: (0..operands)
            .map(|_| cur.u16("record slot"))
            .collect::<Result<Vec<_>>>()?,
        opcode: cur.u8("record opcode")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::bcd::{Container, write_program};

    /// Empty sections of a data-only project.
    const NO_SYMBOLS: [u8; 8] = [0; 8];
    const NO_CODE: [u8; 16] = {
        let mut body = [0; 16];
        body[0] = 1;
        body
    };

    /// A count past the bytes is an error, not an allocation that aborts.
    #[test]
    fn a_count_past_the_bytes_is_refused() {
        let symbols = [0, 0, 0, 0, 0xff, 0xff, 0xff, 0xff];

        assert!(walk(&symbols).is_err());
    }

    fn proto<'a>(name: &'a str, shape: &'a str, values: &[(&'a str, &'a str)]) -> Proto<'a> {
        Proto {
            name: Cow::Borrowed(name),
            shape: Cow::Borrowed(shape),
            values: values
                .iter()
                .map(|(k, v)| (Cow::Borrowed(*k), Cow::Borrowed(*v)))
                .collect(),
        }
    }

    #[test]
    fn a_written_program_reads_back() {
        let section = DataSection {
            prototypes: vec![
                proto("Alpha", "Hero", &[("a", "1"), ("b", "false")]),
                proto("Beta", "Lair", &[]),
            ],
        };

        let bytes = write_program(&NO_SYMBOLS, &NO_CODE, &section.encode());
        let program = Container::parse(&bytes).expect("header");
        let sections = program.sections().expect("three sections");

        assert_eq!(sections.symbols.body, NO_SYMBOLS);
        assert_eq!(sections.code.body, NO_CODE);
        assert_eq!(
            DataSection::decode(sections.data.body).expect("data section"),
            section
        );
    }

    #[test]
    fn a_flag_word_other_than_one_is_an_error() {
        let section = DataSection {
            prototypes: vec![proto("A", "S", &[("k", "v")])],
        };
        let mut body = section.encode();
        body[0] = 2;

        assert!(DataSection::decode(&body).is_err());
    }

    #[test]
    fn an_empty_data_section_is_twenty_bytes() {
        // As in 35 shipped files.
        let mut framed = Vec::new();
        block::write(&mut framed, |o| {
            o.extend_from_slice(&DataSection::default().encode());
        });

        assert_eq!(
            framed,
            b"\x14\0\0\0\x01\0\0\0\0\0\0\0\0\0\0\0\xff\xff\xff\xff"
        );
    }

    fn definition<'a>(name: &'a str, body: &'a [u8]) -> Definition<'a> {
        Definition {
            name,
            body: Cow::Borrowed(body),
        }
    }

    #[test]
    fn a_code_section_round_trips() {
        let section = ScopeSection {
            prototypes: vec![definition("AIRoot", &[1, 2, 3])],
            functions: vec![
                definition("Acid_Bolt_Hit", &[4, 5]),
                definition("Animal", &[]),
            ],
            constants: vec![definition("#QNumber_Krolm", &[6])],
        };

        let bytes = section.encode();

        assert_eq!(ScopeSection::decode(&bytes).expect("round trip"), section);
        assert_eq!(section.len(), 4);
        assert_eq!(section.distinct_names(), 4);
    }

    #[test]
    fn an_empty_code_section_is_the_sixteen_bytes_the_one_mod_carries() {
        // As in `Adjust Guardhouse`, and as `emit` writes for a data-only
        // project.
        assert_eq!(
            ScopeSection::default().encode(),
            b"\x01\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0"
        );
        assert!(ScopeSection::default().is_empty());
    }

    #[test]
    fn a_name_defined_twice_counts_once_as_a_symbol() {
        // Majestic Majesty defines `StringInList` twice: the one file where
        // the section 1 and section 2 counts disagree. Case-insensitive.
        let section = ScopeSection {
            functions: vec![
                definition("StringInList", &[1]),
                definition("stringinlist", &[2]),
            ],
            ..ScopeSection::default()
        };

        assert_eq!(section.len(), 2);
        assert_eq!(section.distinct_names(), 1);
    }

    #[test]
    fn a_constant_without_its_sigil_is_refused() {
        // A lost sigil means functions read as constants.
        let mut bytes = ScopeSection {
            constants: vec![definition("#Ok", &[])],
            ..ScopeSection::default()
        }
        .encode();

        let at = bytes.iter().position(|b| *b == b'#').expect("the sigil");
        bytes[at] = b'X';

        assert!(matches!(
            ScopeSection::decode(&bytes).unwrap_err(),
            Error::BadMagic { .. }
        ));
    }

    #[test]
    fn a_code_section_that_does_not_fill_its_block_is_refused() {
        let mut bytes = ScopeSection {
            functions: vec![definition("A", &[9])],
            ..ScopeSection::default()
        }
        .encode();
        bytes.push(0);

        assert!(matches!(
            ScopeSection::decode(&bytes).unwrap_err(),
            Error::BrokenInvariant { .. }
        ));
    }

    #[test]
    fn the_symbol_section_is_a_flag_and_a_count() {
        assert_eq!(symbol_count(&NO_SYMBOLS).expect("empty"), 0);

        let mut body = NO_SYMBOLS;
        body[4] = 7;
        assert_eq!(symbol_count(&body).expect("seven"), 7);

        // Section 1 opens with 0; the others with 1.
        body[0] = 1;
        assert!(symbol_count(&body).is_err());
    }
}
