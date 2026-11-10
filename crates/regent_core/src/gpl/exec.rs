//! The decoded execution form: section 1 records, read once.
//!
//! **Binary.** The engine does the same: gpl decode record turns each
//! file record into one 0x24 byte instruction when the scripts are linked, and
//! the interpreter runs those. This module is that step, without the layout.
//!
//! ```text
//! bcd::walk          records, as the file holds them
//!   │ decode         once per definition
//!   ▼
//! Definition         name, file, kind: function signature, constant, prototype
//!   └ Instr × n      one per record, same index: a slot number is an index
//!       op           Op, all 43 the VM runs
//!       dest         what the result slot holds before it runs
//!       lhs, rhs     Operand: a record's result, a literal, a #Name
//!       head         jump target, parameter position or loop, by opcode
//!       call         callee and argument slots, on opLParen
//!       line, name   for errors and the debugger
//! ```
//!
//! What the engine decoder does with a value slot, by type,
//! and the form here:
//!
//! | Slot type | Engine | [`Operand`] |
//! |---|---|---|
//! | 10 `undefined` | a pointer to a record's slot: word the record, flag which slot | [`Operand::Slot`] |
//! | 9 `reference` | queued; gpl fixups resolves the `#Name` after registration | [`Operand::Constant`] |
//! | 11, 12 | nothing | [`Operand::None`] |
//! | 0 to 8 | a fresh value of the type, from word, extra or text | [`Operand::Value`] |
//!
//! A declaration's slot is the last row: a fresh value of the declared type,
//! all payload zero. So a declaration and a literal share [`Literal`].
//!
//! # Dropped, and why
//!
//! - A literal's `flag`: the engine sets a value from its type alone.
//! - The opening slot's tag 11 against 12: the engine allocates nothing for
//!   either. Both are [`Dest::None`]; the opcode says which computes a value.
//! - The operand list's first entry: always the record's own index
//!   (`a_records_index_is_its_expression_slot`); checked, then dropped.
//! - Section 2, the scope table: no record. The engine reads it;
//!   what for is unread.
//! - Byte offsets and the record framing.
//!
//! Any shape the corpus lacks is refused, not guessed: a reference with flag
//! other than 2, an operand list on a record that is no call, a payload field
//! set that its type does not read.

use crate::base::digest::Map;

use super::bcd::{
    CONSTANT_RECORD, Compiled, DataSection, Mark, NO_LINE, RESULT_FLAG, RETURN_RECORD, Record,
    Slot, walk,
};
use super::builtins::BuiltinId;
use super::opcode::Op;
use crate::base::gfix::Gfix;
use crate::data::bcd::Container;
use crate::data::error::{Error, Result};
use crate::data::gpltype::TypeCode;

/// An interned string: a name, a path or a literal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Sym(u32);

/// The string table every [`Sym`] indexes. Case is kept: a literal is text.
#[derive(Clone, Debug, Default)]
pub struct Names {
    text: Vec<Box<str>>,
    index: Map<Box<str>, Sym>,
}

impl Names {
    /// The symbol for `s`, added if new.
    ///
    /// # Panics
    ///
    /// Past `u32::MAX` distinct strings.
    pub fn intern(&mut self, s: &str) -> Sym {
        if let Some(sym) = self.index.get(s) {
            *sym
        } else {
            let sym = Sym(u32::try_from(self.text.len()).expect("under 2^32 strings"));

            self.text.push(s.into());
            self.index.insert(s.into(), sym);
            sym
        }
    }

    /// The text of `sym`.
    ///
    /// # Panics
    ///
    /// On a symbol from another table.
    #[must_use]
    pub fn get(&self, sym: Sym) -> &str {
        &self.text[sym.0 as usize]
    }
}

/// A fresh value, as the engine makes one at load.
///
/// `List`, `Agent` and `Coord` carry nothing: only declarations have them,
/// and the engine takes the type's default.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Literal {
    /// Type 0, the shared null: `__Return_value__` with no `is`.
    Null,
    Int(i32),
    Float(Gfix),
    Str(Sym),
    List,
    Agent,
    Bool(bool),
    Coord,
    /// A function value, by name. Empty on a declared `function` variable:
    /// the empty name is a builtin, `NullFunction`'s.
    Function(Sym),
}

impl Literal {
    /// Its type code; [`TypeCode::Null`] for [`Literal::Null`].
    #[must_use]
    pub const fn type_code(&self) -> TypeCode {
        match self {
            Literal::Null => TypeCode::Null,
            Literal::Int(_) => TypeCode::Integer,
            Literal::Float(_) => TypeCode::Float,
            Literal::Str(_) => TypeCode::String,
            Literal::List => TypeCode::List,
            Literal::Agent => TypeCode::Agent,
            Literal::Bool(_) => TypeCode::Boolean,
            Literal::Coord => TypeCode::Coord,
            Literal::Function(_) => TypeCode::Function,
        }
    }
}

/// One of a record's two value slots, resolved.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Operand {
    None,
    /// Record `n`'s result: slot `n` of the definition.
    Slot(u32),
    Value(Literal),
    /// `#Name`, sigil kept. [`super::load`] replaces a script constant with
    /// its value; what remains the engine resolves from its own tables.
    Constant(Sym),
}

/// What a record's result slot holds before it runs: its opening slot.
///
/// **Binary**, gpl decode record, against the corpus's four shapes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Dest {
    /// Tag 11 or 12: nothing at load.
    None,
    /// Tag 10, word -1: the result is the first operand's value itself. A
    /// place: assignments, `opNop` with an operand, declarations.
    Alias,
    /// Tag 1 to 8: a fresh value of that type. Only `Snake_Pit_Spawn`'s
    /// `opPlus`, an integer, in the corpus.
    Value(Literal),
}

/// A record's `head`, read by its opcode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Head {
    None,
    /// An instruction index; one past the end leaves the function.
    Jump(u32),
    /// `opLocalize`: the parameter's position, from 1.
    Param(u32),
    /// `opBreak`, `opContinue`: the loop's `opForeach`.
    Loop(u32),
}

/// A call's target.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Callee {
    /// By name, not yet linked, or linked to nothing.
    Name(Sym),
    /// Linked by [`super::load`].
    Fn(Target),
    /// Through a slot holding a function value: `($Name)(args)`.
    Slot(u32),
}

/// An entry of the function map a call can reach.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Target {
    Builtin(BuiltinId),
    /// Index into [`super::load::Program::definitions`].
    Script(u32),
}

/// `opLParen`'s operands.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Call {
    pub callee: Callee,
    /// Argument slots, in order. The result is the call's own slot.
    pub args: Box<[u32]>,
}

/// One instruction; its index is its slot.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Instr {
    pub op: Op,
    pub dest: Dest,
    pub lhs: Operand,
    pub rhs: Operand,
    pub head: Head,
    /// On [`Op::Call`] only.
    pub call: Option<Box<Call>>,
    /// Source line; `None` where the compiler wrote none.
    pub line: Option<u16>,
    /// The record's symbol: source file, own name, a parameter or local.
    pub name: Option<Sym>,
}

/// A parameter or local: its name, type code and declaring instruction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Local {
    pub name: Sym,
    /// 1 to 8.
    pub ty: TypeCode,
    /// Its `opLocalize` or `opDeclare`; the local is that slot.
    pub at: u32,
}

/// A function's prologue, as a signature.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Signature {
    /// `is <type>`; [`TypeCode::Null`] without one.
    pub returns: TypeCode,
    /// `__Return_value__`'s instruction.
    pub return_at: u32,
    pub params: Vec<Local>,
    pub locals: Vec<Local>,
    /// First instruction after [`Op::Body`].
    pub body: u32,
}

/// What a definition is.
///
/// **Binary**, gpl register script: first record `opAdvance` is a
/// function, a name opening `#` a constant, anything else a prototype.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Function(Signature),
    Constant(Literal),
    /// Members: its `opDeclare` records.
    Prototype(Vec<Local>),
}

/// One definition, decoded.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Definition {
    /// As written; a constant keeps `#`.
    pub name: Sym,
    /// Source file as the compiler received it.
    pub file: Sym,
    pub kind: Kind,
    pub code: Vec<Instr>,
}

/// A constant's sigil.
const SIGIL: char = '#';

impl Definition {
    /// The compiled type of the value slot `at` names, where load fixes it.
    ///
    /// - A declaration: its declared type; `__Return_value__` its `is`.
    /// - A fresh value: that value's type.
    /// - A place: its first operand's type, followed through slots.
    /// - Anything else, an operator's temporary included: `None`, decided at
    ///   run time.
    #[must_use]
    pub fn slot_type(&self, at: u32) -> Option<TypeCode> {
        self.type_within(at, self.code.len())
    }

    /// [`Definition::slot_type`], following at most `budget` places.
    fn type_within(&self, at: u32, budget: usize) -> Option<TypeCode> {
        let instr = self.code.get(at as usize)?;

        match (instr.op, instr.dest, instr.lhs) {
            (Op::Reference, _, Operand::Value(Literal::Null)) => None,
            (Op::Reference, _, Operand::Value(lit))
            | (_, Dest::Value(lit), _)
            | (_, Dest::Alias, Operand::Value(lit)) => Some(lit.type_code()),
            (_, Dest::Alias, Operand::Slot(next)) if budget > 0 => {
                self.type_within(next, budget - 1)
            }
            _ => None,
        }
    }

    /// The signature, on a function.
    #[must_use]
    pub const fn signature(&self) -> Option<&Signature> {
        match &self.kind {
            Kind::Function(signature) => Some(signature),
            _ => None,
        }
    }
}

/// Decode one section 1 definition.
///
/// # Errors
///
/// No records; an opcode past 42; a shape the corpus lacks (see the module
/// notes); a prologue off its fixed layout.
pub fn decode(compiled: &Compiled<'_>, names: &mut Names) -> Result<Definition> {
    let count = u32::try_from(compiled.records.len()).map_err(|_| Error::BadValue {
        what: "definition record count",
        value: compiled.records.len() as u64,
    })?;

    let first = compiled.records.first().ok_or(Error::BadValue {
        what: "definition with no records",
        value: 0,
    })?;

    // Sized once: a collect over a `Result` cannot know the count, and
    // grows by copying.
    let mut code = Vec::with_capacity(compiled.records.len());

    for (record, at) in compiled.records.iter().zip(0..) {
        code.push(instr(record, at, count, names)?);
    }

    let kind = if first.opcode == Op::Advance.code() {
        Kind::Function(signature(&code)?)
    } else if compiled.name.starts_with(SIGIL) {
        Kind::Constant(constant(&code)?)
    } else {
        Kind::Prototype(members(&code, Op::Declare))
    };

    Ok(Definition {
        name: names.intern(compiled.name),
        file: names.intern(first.text),
        kind,
        code,
    })
}

/// One record.
fn instr(record: &Record<'_>, at: u32, count: u32, names: &mut Names) -> Result<Instr> {
    let op = Op::from_code(record.opcode).ok_or(Error::BadValue {
        what: "opcode",
        value: u64::from(record.opcode),
    })?;

    let lhs = operand(&record.values[0], count, names)?;
    let rhs = operand(&record.values[1], count, names)?;

    let call = match (op, record.slots.is_empty()) {
        (Op::Call, _) => Some(Box::new(call(record, at, count, lhs)?)),
        (_, true) => None,
        (_, false) => Err(Error::BadValue {
            what: "operand list on a record that is no call",
            value: u64::from(record.opcode),
        })?,
    };

    Ok(Instr {
        op,
        dest: dest(record.mark)?,
        lhs,
        rhs,
        head: head(op, record.head, count)?,
        call,
        line: (record.line != NO_LINE).then_some(record.line),
        name: (!record.text.is_empty()).then(|| names.intern(record.text)),
    })
}

/// A value slot.
fn operand(value: &Slot<'_>, count: u32, names: &mut Names) -> Result<Operand> {
    let refuse = |what| {
        Err(Error::BadValue {
            what,
            value: u64::from(value.kind),
        })
    };

    match TypeCode::try_from(value.kind) {
        Ok(TypeCode::Unknown) if *value == Slot::UNUSED => Ok(Operand::None),
        Ok(TypeCode::Undefined)
            if value.flag == RESULT_FLAG
                && value.word < count
                && value.extra == 0
                && value.text.is_empty() =>
        {
            Ok(Operand::Slot(value.word))
        }
        Ok(TypeCode::Reference) if value.word == 0 && value.extra == 0 => {
            Ok(Operand::Constant(names.intern(value.text)))
        }
        Ok(TypeCode::Unknown) => refuse("payload on an unused slot"),
        Ok(TypeCode::Undefined) => refuse("reference shape"),
        Ok(TypeCode::Reference) => refuse("payload on a #Name"),
        Ok(TypeCode::NoType) | Err(_) => refuse("operand type"),
        Ok(_) => literal(value, names).map(Operand::Value),
    }
}

/// A fresh value of a slot's type, from the field that type reads. Any other
/// field set is refused.
fn literal(value: &Slot<'_>, names: &mut Names) -> Result<Literal> {
    let word_only = value.extra == 0 && value.text.is_empty();
    let text_only = value.word == 0 && value.extra == 0;
    let bare = word_only && value.word == 0;

    #[allow(clippy::cast_possible_wrap)]
    let found = match TypeCode::try_from(value.kind) {
        Ok(TypeCode::Null) if bare => Some(Literal::Null),
        Ok(TypeCode::Integer) if word_only => Some(Literal::Int(value.word as i32)),
        Ok(TypeCode::Float) if value.word == 0 && value.text.is_empty() => {
            Some(Literal::Float(Gfix(value.extra as i32)))
        }
        Ok(TypeCode::String) if text_only => Some(Literal::Str(names.intern(value.text))),
        Ok(TypeCode::List) if bare => Some(Literal::List),
        Ok(TypeCode::Agent) if bare => Some(Literal::Agent),
        Ok(TypeCode::Boolean) if word_only => Some(Literal::Bool(value.word != 0)),
        Ok(TypeCode::Coord) if bare => Some(Literal::Coord),
        Ok(TypeCode::Function) if text_only => Some(Literal::Function(names.intern(value.text))),
        _ => None,
    };

    found.ok_or(Error::BadValue {
        what: "literal payload its type does not read",
        value: u64::from(value.kind),
    })
}

/// The opening slot. Its payload is zero
/// (`a_records_opening_slot_carries_no_payload`), so a fresh value takes its
/// type's zero. A string or function tag would need text; none occurs.
fn dest(mark: Mark) -> Result<Dest> {
    let fresh = match TypeCode::try_from(mark.tag) {
        Ok(TypeCode::Null) => Some(Literal::Null),
        Ok(TypeCode::Integer) => Some(Literal::Int(0)),
        Ok(TypeCode::Float) => Some(Literal::Float(Gfix(0))),
        Ok(TypeCode::List) => Some(Literal::List),
        Ok(TypeCode::Agent) => Some(Literal::Agent),
        Ok(TypeCode::Boolean) => Some(Literal::Bool(false)),
        Ok(TypeCode::Coord) => Some(Literal::Coord),
        _ => None,
    };

    match (mark, fresh) {
        (Mark::NONE | Mark::VALUE, _) => Ok(Dest::None),
        (Mark::PLACE, _) => Ok(Dest::Alias),
        (Mark { word: 0, .. }, Some(lit)) => Ok(Dest::Value(lit)),
        _ => Err(Error::BadValue {
            what: "opening slot",
            value: u64::from(mark.tag),
        }),
    }
}

/// `head`, by opcode. **Corpus**: set on exactly these seven
/// (`a_records_head_is_set_on_exactly_the_control_transfer_opcodes`).
fn head(op: Op, head: u32, count: u32) -> Result<Head> {
    let jump = head <= count;

    match op {
        Op::If | Op::While | Op::Advance | Op::Rewind if jump => Ok(Head::Jump(head)),
        Op::Break | Op::Continue if head < count => Ok(Head::Loop(head)),
        Op::Localize => Ok(Head::Param(head)),
        _ if head == 0 => Ok(Head::None),
        _ => Err(Error::BadValue {
            what: "record head",
            value: u64::from(head),
        }),
    }
}

/// `opLParen`: callee from the first value slot, arguments from the operand
/// list after the result, each one of the definition's `count` records.
fn call(record: &Record<'_>, at: u32, count: u32, callee: Operand) -> Result<Call> {
    let (result, args) = record.slots.split_first().ok_or(Error::BadValue {
        what: "call without a result slot",
        value: u64::from(at),
    })?;

    let callee = match (u32::from(*result) == at, callee) {
        (false, _) => Err(Error::BrokenInvariant {
            what: "a call's result is its own slot",
            expected: u64::from(at),
            found: u64::from(*result),
        }),
        (true, Operand::Value(Literal::Function(name))) => Ok(Callee::Name(name)),
        (true, Operand::Slot(slot)) => Ok(Callee::Slot(slot)),
        (true, _) => Err(Error::BadValue {
            what: "callee",
            value: u64::from(at),
        }),
    }?;

    let args: Box<[u32]> = args.iter().map(|slot| u32::from(*slot)).collect();

    match args.iter().find(|slot| **slot >= count) {
        Some(slot) => Err(Error::BadValue {
            what: "call argument past the records",
            value: u64::from(*slot),
        }),
        None => Ok(Call { callee, args }),
    }
}

/// A function's prologue, fixed by `emit::prologue`:
///
/// ```text
/// [0] opAdvance  [1] opNop name  [2] opExit  [3] opAdvance  [4] opReference
/// opLocalize × params, opDeclare × locals, Body
/// ```
fn signature(code: &[Instr]) -> Result<Signature> {
    let ret = code
        .get(RETURN_RECORD as usize)
        .filter(|i| i.op == Op::Reference);
    let body = code.iter().position(|i| i.op == Op::Body);

    match (ret, body) {
        (Some(ret), Some(body)) => {
            let params = members(&code[..body], Op::Localize);
            let ordered = params
                .iter()
                .zip(1..)
                .all(|(p, nth)| code[p.at as usize].head == Head::Param(nth));

            if ordered {
                Ok(Signature {
                    returns: operand_type(ret.lhs).unwrap_or(TypeCode::Null),
                    return_at: RETURN_RECORD,
                    params,
                    locals: members(&code[..body], Op::Declare),
                    body: u32::try_from(body + 1).unwrap_or(u32::MAX),
                })
            } else {
                Err(Error::BadValue {
                    what: "parameter position",
                    value: params.len() as u64,
                })
            }
        }
        _ => Err(Error::BadValue {
            what: "function prologue",
            value: code.len() as u64,
        }),
    }
}

/// The declarations of opcode `op`, in order.
fn members(code: &[Instr], op: Op) -> Vec<Local> {
    code.iter()
        .zip(0..)
        .filter(|(i, _)| i.op == op)
        .filter_map(|(i, at)| {
            Some(Local {
                name: i.name?,
                ty: operand_type(i.lhs)?,
                at,
            })
        })
        .collect()
}

/// A literal operand's type code.
const fn operand_type(operand: Operand) -> Option<TypeCode> {
    match operand {
        Operand::Value(lit) => Some(lit.type_code()),
        _ => None,
    }
}

/// A constant's value: instruction 2's first operand, negated where it is
/// an `opNegate` (`-2` in the source). The engine evaluates the records;
/// **Inferred** that these two are all it
/// meets, as `emit::constant` writes no other.
fn constant(code: &[Instr]) -> Result<Literal> {
    match code.get(CONSTANT_RECORD).map(|i| (i.op, i.lhs)) {
        Some((Op::Negate, Operand::Value(Literal::Int(n)))) => Ok(Literal::Int(n.wrapping_neg())),
        Some((Op::Negate, Operand::Value(Literal::Float(f)))) => {
            Ok(Literal::Float(Gfix(f.0.wrapping_neg())))
        }
        Some((Op::Nop, Operand::Value(lit))) => Ok(lit),
        _ => Err(Error::BadValue {
            what: "constant value",
            value: code.len() as u64,
        }),
    }
}

/// A `.dat` instance from section 3.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DataSet {
    pub name: Sym,
    /// The prototype it instantiates.
    pub shape: Sym,
    /// Key and value, both text, as the compiler stored them.
    pub values: Vec<(Sym, Sym)>,
}

/// One `.bcd`, decoded: sections 1 and 3.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Image {
    /// Section 1 order: by name, case-insensitively.
    pub definitions: Vec<Definition>,
    pub data: Vec<DataSet>,
}

/// Decode a whole `.bcd` file.
///
/// # Errors
///
/// A malformed container or section, or a definition [`decode`] refuses.
pub fn image(bytes: &[u8], names: &mut Names) -> Result<Image> {
    let program = Container::parse(bytes)?;
    let sections = program.sections()?;

    let definitions = walk(sections.symbols.body)?
        .iter()
        .map(|compiled| decode(compiled, names))
        .collect::<Result<Vec<_>>>()?;

    let data = DataSection::decode(sections.data.body)?
        .prototypes
        .iter()
        .map(|proto| DataSet {
            name: names.intern(&proto.name),
            shape: names.intern(&proto.shape),
            values: proto
                .values
                .iter()
                .map(|(key, value)| (names.intern(key), names.intern(value)))
                .collect(),
        })
        .collect();

    Ok(Image { definitions, data })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A call's argument slot names a record of its definition.
    #[test]
    fn a_call_argument_past_the_records_is_refused() {
        let record = |slots| Record {
            mark: Mark::NONE,
            values: [Slot::UNUSED, Slot::UNUSED],
            head: 0,
            text: "",
            line: 0,
            slots,
            opcode: Op::Call.code(),
        };

        assert!(call(&record(vec![5, 3]), 5, 20, Operand::Slot(1)).is_ok());
        assert!(call(&record(vec![5, 20]), 5, 20, Operand::Slot(1)).is_err());
    }

    #[test]
    fn interning_is_case_sensitive_and_stable() {
        let mut names = Names::default();
        let a = names.intern("Gold");
        let b = names.intern("gold");

        assert_ne!(a, b, "a literal keeps its case");
        assert_eq!(names.intern("Gold"), a);
        assert_eq!(names.get(b), "gold");
    }

    fn slot(kind: u8, word: u32, extra: u32, text: &str, flag: u8) -> Slot<'_> {
        Slot {
            word,
            extra,
            text,
            kind,
            flag,
        }
    }

    #[test]
    fn a_slot_decodes_by_its_type() {
        let mut names = Names::default();
        let mut op = |v: Slot<'_>| operand(&v, 20, &mut names);

        assert_eq!(op(slot(10, 7, 0, "", 2)), Ok(Operand::Slot(7)));
        assert_eq!(op(Slot::UNUSED), Ok(Operand::None));
        assert_eq!(
            op(slot(1, u32::MAX, 0, "", 2)),
            Ok(Operand::Value(Literal::Int(-1)))
        );

        // `1.5`, fixed point.
        assert_eq!(
            op(slot(2, 0, 1536, "", 0)),
            Ok(Operand::Value(Literal::Float(Gfix(1536))))
        );
        assert_eq!(
            op(slot(6, 1, 0, "", 2)),
            Ok(Operand::Value(Literal::Bool(true)))
        );

        // A declaration: the declared type, nothing else.
        assert_eq!(op(slot(5, 0, 0, "", 0)), Ok(Operand::Value(Literal::Agent)));
        assert!(matches!(
            op(slot(9, 0, 0, "#Limit", 0)),
            Ok(Operand::Constant(_))
        ));
    }

    #[test]
    fn a_shape_the_corpus_lacks_is_refused() {
        let mut names = Names::default();
        let mut op = |v: Slot<'_>| operand(&v, 20, &mut names);

        // A reference picking another slot of its record, or past the end.
        assert!(op(slot(10, 7, 0, "", 0)).is_err());
        assert!(op(slot(10, 20, 0, "", 2)).is_err());

        // An integer with text; an agent with a payload.
        assert!(op(slot(1, 3, 0, "x", 2)).is_err());
        assert!(op(slot(5, 1, 0, "", 0)).is_err());
        assert!(op(slot(12, 0, 0, "", 0)).is_err());
    }

    #[test]
    fn the_opening_slot_decides_the_destination() {
        let mark = |word, tag| dest(Mark { word, tag });

        assert_eq!(mark(u32::MAX, 10), Ok(Dest::Alias));
        assert_eq!(mark(0, 11), Ok(Dest::None));
        assert_eq!(mark(0, 12), Ok(Dest::None));
        assert_eq!(mark(0, 1), Ok(Dest::Value(Literal::Int(0))));
        assert!(mark(0, 10).is_err());
    }

    #[test]
    fn a_head_is_read_by_its_opcode() {
        assert_eq!(head(Op::If, 13, 13), Ok(Head::Jump(13)));
        assert_eq!(head(Op::Localize, 2, 13), Ok(Head::Param(2)));
        assert_eq!(head(Op::Break, 6, 13), Ok(Head::Loop(6)));
        assert_eq!(head(Op::Plus, 0, 13), Ok(Head::None));
        assert!(head(Op::If, 14, 13).is_err());
        assert!(head(Op::Plus, 1, 13).is_err());
    }
}
