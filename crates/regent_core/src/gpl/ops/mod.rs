//! What a GPL operator does, per value type: an executable model.
//!
//! **Binary**, `MajestyHD.exe`. Opcodes 0 to 23 never reach the VM switch.
//! Gpl execute hands each to its left operand:
//!
//! ```text
//! record: op, result slot, lhs slot, rhs slot
//!   result slot empty?   gpl_result_type(op, code(lhs), code(rhs))
//!                        makes a value of that type, or leaves it empty
//!   the lhs value's type  (record, op): one switch per value class
//!     assignment ops     write the left operand in place
//!     value ops          write the result slot through an accessor:
//!                        int&, float&, bool&, string&; another class throws
//! ```
//!
//! So a result's type is the result slot's, decided before the operator runs
//! and kept for every later run of that record. The operator only picks which
//! accessor to call. [`apply`] does both steps.
//!
//! Everything a class does not handle throws "Unsupported operation attempted
//! on X type", a `GplException`; so does every failed accessor, "bad
//! conversion to Y". Gpl thread run catches it and abandons the call.
//! An integer divide by zero is a CPU fault instead: nothing catches it as a
//! `GplException` ([`OpError::Fault`]).
//!
//! Floats are fixed point, [`Gfix`]: 10 fractional bits. An integer converts to
//! one by `<< 10` in 32 bits, which wraps from 2^21 = 2,097,152 up; integer
//! `+`, `-`, `*`, `/`, `!=` and the orderings all pass through that
//! conversion. `==` does not.

use std::cmp::Ordering;

mod list;

pub use list::{FREED_KEPT, List, Node};
pub(crate) use list::{Way, node, numbered, unnode};

pub use super::opcode::Operator;
use crate::base::gfix::Gfix;
use crate::data::gpltype::Kind;
use crate::data::gpltype::TypeCode;

/// Gpl result type's table, in file order.
///
/// Codes are [`TypeCode`]s; rows end at [`crate::gpl::opcode::FIRST_CONTROL`].
/// A missing key and a result of [`TypeCode::NoType`] both leave the slot
/// empty.
/// The table is not the classes: it types `int == float` (always false),
/// `list + list` (unsupported by `GplList`) and `++` on a float as integer
/// (the operator writes only its operand).
pub const RESULT_TYPES: [[u8; 4]; 67] = [
    [7, 6, 6, 6],
    [8, 6, 6, 6],
    [9, 1, 1, 6],
    [9, 2, 2, 6],
    [9, 1, 2, 6],
    [9, 2, 1, 6],
    [9, 3, 3, 6],
    [9, 4, 4, 6],
    [9, 5, 5, 6],
    [9, 6, 6, 6],
    [9, 7, 7, 6],
    [9, 8, 8, 6],
    [10, 1, 1, 6],
    [10, 1, 2, 6],
    [10, 2, 1, 6],
    [10, 2, 2, 6],
    [10, 3, 3, 6],
    [10, 4, 4, 6],
    [10, 5, 5, 6],
    [10, 6, 6, 6],
    [10, 7, 7, 6],
    [10, 8, 8, 6],
    [11, 1, 1, 6],
    [11, 1, 2, 6],
    [11, 2, 1, 6],
    [11, 2, 2, 6],
    [12, 1, 1, 6],
    [12, 1, 2, 6],
    [12, 2, 1, 6],
    [12, 2, 2, 6],
    [13, 1, 1, 6],
    [13, 1, 2, 6],
    [13, 2, 1, 6],
    [13, 2, 2, 6],
    [14, 1, 1, 6],
    [14, 1, 2, 6],
    [14, 2, 1, 6],
    [14, 2, 2, 6],
    [15, 1, 1, 1],
    [15, 1, 2, 2],
    [15, 2, 1, 2],
    [15, 2, 2, 2],
    [15, 3, 3, 3],
    [15, 4, 4, 4],
    [16, 1, 1, 1],
    [16, 1, 2, 2],
    [16, 2, 1, 2],
    [16, 2, 2, 2],
    [17, 1, 1, 1],
    [17, 1, 2, 2],
    [17, 2, 1, 2],
    [17, 2, 2, 2],
    [18, 1, 1, 1],
    [18, 1, 2, 2],
    [18, 2, 1, 2],
    [18, 2, 2, 2],
    [19, 1, 1, 1],
    [20, 5, 3, 12],
    [20, 3, 3, 12],
    [21, 1, 12, 1],
    [21, 2, 12, 1],
    [22, 1, 12, 1],
    [22, 2, 12, 1],
    [23, 1, 12, 1],
    [23, 2, 12, 2],
    [32, 4, 12, 12],
    [38, 8, 12, 12],
];

/// A value class's operator switch, its `GplType` operator call: the opcodes it
/// handles. Every other opcode goes to the throw.
#[derive(Clone, Copy, Debug)]
pub struct Class {
    pub kind: Kind,
    pub cases: &'static [Operator],
}

impl Class {
    /// Whether the class handles `op` rather than throwing.
    #[must_use]
    pub fn handles(&self, op: Operator) -> bool {
        self.kind == Kind::Null || self.cases.contains(&op)
    }
}

/// Every value class's switch in `MajestyHD.exe`. `GplNull` is a bare
/// Any operator on a null left operand does nothing.
pub const HD_CLASSES: [Class; 9] = [
    Class {
        kind: Kind::Null,
        cases: &[],
    },
    Class {
        kind: Kind::Integer,
        cases: &[
            Operator::AssignEqual,
            Operator::TimesEqual,
            Operator::DivideEqual,
            Operator::ModEqual,
            Operator::PlusEqual,
            Operator::MinusEqual,
            Operator::Equal,
            Operator::NotEqual,
            Operator::LessThan,
            Operator::GreaterThan,
            Operator::LessThanOrEqualTo,
            Operator::GreaterThanOrEqualTo,
            Operator::Plus,
            Operator::Minus,
            Operator::Star,
            Operator::Divide,
            Operator::Mod,
            Operator::Inc,
            Operator::Dec,
            Operator::Negate,
        ],
    },
    Class {
        kind: Kind::Float,
        cases: &[
            Operator::AssignEqual,
            Operator::TimesEqual,
            Operator::DivideEqual,
            Operator::PlusEqual,
            Operator::MinusEqual,
            Operator::Equal,
            Operator::NotEqual,
            Operator::LessThan,
            Operator::GreaterThan,
            Operator::LessThanOrEqualTo,
            Operator::GreaterThanOrEqualTo,
            Operator::Plus,
            Operator::Minus,
            Operator::Star,
            Operator::Divide,
            Operator::Inc,
            Operator::Dec,
            Operator::Negate,
        ],
    },
    Class {
        kind: Kind::String,
        cases: &[
            Operator::AssignEqual,
            Operator::PlusEqual,
            Operator::Equal,
            Operator::NotEqual,
            Operator::LessThan,
            Operator::GreaterThan,
            Operator::LessThanOrEqualTo,
            Operator::GreaterThanOrEqualTo,
            Operator::Plus,
            Operator::Apostrophe,
        ],
    },
    Class {
        kind: Kind::List,
        cases: &[
            Operator::AssignEqual,
            Operator::PlusEqual,
            Operator::MinusEqual,
            Operator::Inserter,
            Operator::Equal,
            Operator::NotEqual,
        ],
    },
    Class {
        kind: Kind::Agent,
        cases: &[
            Operator::AssignEqual,
            Operator::Equal,
            Operator::NotEqual,
            Operator::Apostrophe,
        ],
    },
    Class {
        kind: Kind::Boolean,
        cases: &[
            Operator::AssignEqual,
            Operator::Or,
            Operator::And,
            Operator::Equal,
            Operator::NotEqual,
        ],
    },
    Class {
        kind: Kind::Coord,
        cases: &[Operator::AssignEqual, Operator::Equal, Operator::NotEqual],
    },
    Class {
        kind: Kind::Function,
        cases: &[Operator::AssignEqual, Operator::Equal, Operator::NotEqual],
    },
];

/// The class of `kind`.
#[must_use]
pub fn class(kind: Kind) -> &'static Class {
    &HD_CLASSES[usize::from(kind.code())]
}

/// The agent id of no agent: `$NullAgent()`, a reset agent value
/// and a fresh one all hold -1. The VM
/// keeps ids unsigned, so here `u32::MAX`; [`NULL_AGENT_ID`] is the signed
/// view a save writes.
pub const NULL_AGENT: u32 = u32::MAX;

/// [`NULL_AGENT`] as a signed word: -1.
pub const NULL_AGENT_ID: i32 = NULL_AGENT.cast_signed();

/// A GPL value, minimal: what the operators read and write.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    Null,
    Integer(i32),
    Float(Gfix),
    String(String),
    List(List),
    /// The agent id at `GplAgentRef`.
    Agent(u32),
    Boolean(bool),
    /// The two words at `GplCoord`.
    Coord(i32, i32),
    /// An opaque handle for the function record at `GplFunction`.
    Function(u32),
}

impl Value {
    #[must_use]
    pub const fn kind(&self) -> Kind {
        match self {
            Value::Null => Kind::Null,
            Value::Integer(_) => Kind::Integer,
            Value::Float(_) => Kind::Float,
            Value::String(_) => Kind::String,
            Value::List(_) => Kind::List,
            Value::Agent(_) => Kind::Agent,
            Value::Boolean(_) => Kind::Boolean,
            Value::Coord(..) => Kind::Coord,
            Value::Function(_) => Kind::Function,
        }
    }

    /// A fresh value of a class, as its pool constructor leaves it.
    ///
    /// **Binary** for integer (0) and agent (gpl agentref null:
    /// id -1, [`NULL_AGENT`]), the constructors gpl value pool grow
    /// passes; **Inferred** for the rest.
    #[must_use]
    pub const fn fresh(kind: Kind) -> Value {
        match kind {
            Kind::Null => Value::Null,
            Kind::Integer => Value::Integer(0),
            Kind::Float => Value::Float(Gfix(0)),
            Kind::String => Value::String(String::new()),
            Kind::List => Value::List(List::new()),
            Kind::Agent => Value::Agent(NULL_AGENT),
            Kind::Boolean => Value::Boolean(false),
            Kind::Coord => Value::Coord(0, 0),
            Kind::Function => Value::Function(NULL_FUNCTION),
        }
    }
}

/// The `GplType` accessors the operators call, slots 10 to 27. The default
/// of each throws "bad conversion to" its name; one class overrides a
/// reference accessor, integer and float both override `int` and `float`.
///
/// Slots: `IntRef` to `CoordRef` 10 to 15, `GplBooleanRef` 17, `GplStringRef`
/// 18, `GplListRef` 19, `FunctionRef` 21, `GplCoordRef` 22, `AgentRef` 23,
/// `Int` to `Float` 24 to 27.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Accessor {
    IntRef,
    BoolRef,
    StringRef,
    ListRef,
    FloatRef,
    CoordRef,
    FunctionRef,
    /// `gplcoord&`: `GetX`, `GetY`, `DistanceBetweenCoords`.
    GplCoordRef,
    AgentRef,
    /// `gpllist&`: `ClearList`.
    GplListRef,
    /// `gplstring&`, `gplboolean&`: `PathCost`.
    GplStringRef,
    GplBooleanRef,
    Int,
    Bool,
    Str,
    Float,
}

impl Accessor {
    /// The name its default's message ends with.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Accessor::IntRef => "int&",
            Accessor::BoolRef => "bool&",
            Accessor::StringRef => "string&",
            Accessor::ListRef => "list&",
            Accessor::FloatRef => "float&",
            Accessor::CoordRef => "coord&",
            Accessor::FunctionRef => "gplfunction&",
            Accessor::GplCoordRef => "gplcoord&",
            Accessor::AgentRef => "gplagent&",
            Accessor::GplListRef => "gpllist&",
            Accessor::GplStringRef => "gplstring&",
            Accessor::GplBooleanRef => "gplboolean&",
            Accessor::Int => "int",
            Accessor::Bool => "bool",
            Accessor::Str => "string",
            Accessor::Float => "float",
        }
    }
}

/// Why an operator failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpError {
    /// The left operand's class has no case for the opcode. Thrown.
    Unsupported(Kind),

    /// An accessor on the wrong class. Thrown.
    BadConversion(Accessor),

    /// "Divide by zero error.", checked on a fixed-point divisor. Thrown.
    DivideByZero,

    /// A 32-bit signed divide faulting: a zero divisor the check missed, or a quotient past
    /// 32 bits. Not a `GplException`. **Inferred**: the game crashes.
    Fault,

    /// The operator writes a result slot that is still empty: the record gave
    /// no type and the table has none. **Inferred**: a null dereference.
    NoResult,

    /// A binary operator's right operand is missing. **Inferred** as above.
    NoOperand,

    /// "Assignment to NULL attempted.": [`assign`] into a null from a value
    /// that is not null. Thrown.
    NullAssign,
}

impl OpError {
    /// The engine's message, for the errors it throws.
    #[must_use]
    pub fn message(self) -> Option<String> {
        match self {
            OpError::Unsupported(kind) => kind
                .shout()
                .map(|s| format!("Unsupported operation attempted on {s} type")),
            OpError::BadConversion(a) => Some(format!("bad conversion to {}", a.name())),
            OpError::DivideByZero => Some("Divide by zero error.".to_owned()),
            OpError::NullAssign => Some("Assignment to NULL attempted.".to_owned()),
            OpError::Fault | OpError::NoResult | OpError::NoOperand => None,
        }
    }
}

/// What an operator left for the VM.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    Done,

    /// `agent's "name"`: the VM points the result slot at the agent's
    /// attribute itself, so a later write lands in the attribute. The engine
    /// throws when the agent is gone or deleted, or lacks the attribute;
    /// it never creates one.
    Attribute {
        agent: u32,
        name: String,
    },
}

type Res<T> = Result<T, OpError>;

/// `a * b` in fixed point, the core in fixed10 mul that
/// gpl fixed mul wraps: a 64-bit product,
/// bits 10 to 41. Floors, and wraps past 32 bits.
///
/// ```
/// use regent_core::gpl::ops::fixed_mul;
///
/// assert_eq!(fixed_mul(1536, 1536), 2304); // 1.5 * 1.5 = 2.25
/// assert_eq!(fixed_mul(-1, 1), -1); // -1/1024 * 1/1024 floors
/// ```
#[must_use]
#[allow(clippy::cast_possible_truncation, reason = "the core keeps 32 bits")]
pub const fn fixed_mul(a: i32, b: i32) -> i32 {
    ((a as i64 * b as i64) >> Gfix::FRACTION_BITS) as i32
}

/// `a / b` in fixed point, the core in fixed10 div that
/// gpl fixed div wraps: `a << 10` in 64
/// bits. Truncates toward zero; a zero divisor or a quotient past 32
/// bits faults.
///
/// ```
/// use regent_core::gpl::ops::fixed_div;
///
/// assert_eq!(fixed_div(1024, 3072), Ok(341)); // 1 / 3
/// assert_eq!(fixed_div(-1024, 3072), Ok(-341));
/// ```
///
/// # Errors
///
/// [`OpError::Fault`] where a 32-bit signed divide faults.
pub fn fixed_div(a: i32, b: i32) -> Res<i32> {
    match (i64::from(a) << Gfix::FRACTION_BITS).checked_div(i64::from(b)) {
        Some(q) => i32::try_from(q).map_err(|_| OpError::Fault),
        None => Err(OpError::Fault),
    }
}

/// Gpl result type: the result slot's class, or `None` to leave it
/// empty. Keys are unique, so file order does not matter.
#[must_use]
pub fn result_type(op: Operator, lhs: TypeCode, rhs: TypeCode) -> Option<Kind> {
    let key = [op.code(), lhs.code(), rhs.code()];

    RESULT_TYPES
        .iter()
        .find(|row| row[..3] == key)
        .and_then(|row| Kind::from_code(row[3]))
}

/// The code gpl execute passes for an operand slot: its value's, or
/// [`TypeCode::NoType`] for none.
fn code(value: Option<&Value>) -> TypeCode {
    value.map_or(TypeCode::NoType, |v| v.kind().type_code())
}

/// One operator record, as gpl execute runs it.
///
/// `dest` is the result slot. Empty, it is first given a fresh value of
/// [`result_type`]'s class; the value stays for later runs. Assignment
/// operators, `++` and `--` write `lhs`; the rest write `dest`. `rhs` is
/// `None` for the unary operators. The model cannot alias `lhs` and `rhs`;
/// the engine can, and `list = list` then empties the list (**Inferred**
/// from `GplList` clearing before it copies).
///
/// ```
/// use regent_core::gpl::ops::{apply, Operator, Value};
///
/// // 7 / 2 into an empty slot: the table makes it an integer.
/// let mut dest = None;
/// apply(Operator::Divide, &mut Value::Integer(7), Some(&Value::Integer(2)), &mut dest).unwrap();
/// assert_eq!(dest, Some(Value::Integer(3)));
/// ```
///
/// # Errors
///
/// Whatever the engine throws or faults on; see [`OpError`].
pub fn apply(
    op: Operator,
    lhs: &mut Value,
    rhs: Option<&Value>,
    dest: &mut Option<Value>,
) -> Res<Step> {
    if dest.is_none() {
        *dest = result_type(op, lhs.kind().type_code(), code(rhs)).map(Value::fresh);
    }

    let done = |r: Res<()>| r.map(|()| Step::Done);

    match lhs {
        Value::Null => Ok(Step::Done),
        Value::Integer(i) => done(integer(op, i, rhs, dest)),
        Value::Float(f) => done(float(op, &mut f.0, rhs, dest)),
        Value::String(s) => done(string(op, s, rhs, dest)),
        Value::List(l) => done(list(op, l, rhs, dest)),
        Value::Agent(id) => agent(op, id, rhs, dest),
        Value::Boolean(b) => done(boolean(op, b, rhs, dest)),
        Value::Coord(x, y) => done(coord(op, (x, y), rhs, dest)),
        Value::Function(f) => done(function(op, f, rhs, dest)),
    }
}

/// Equality as `GplType`'s compare: the left value's own, reading the right one
/// through the accessor the left class picks.
///
/// ```
/// use regent_core::base::gfix::Gfix;
/// use regent_core::gpl::ops::{equal, Value};
///
/// // Integer reads the float by `int`, which floors: 2 == 2.5.
/// assert_eq!(equal(&Value::Integer(2), &Value::Float(Gfix(2560))), Ok(true));
/// ```
///
/// # Errors
///
/// [`OpError::BadConversion`] when the right value lacks the accessor.
pub fn equal(a: &Value, b: &Value) -> Res<bool> {
    match a {
        Value::Null => Ok(b.kind() == Kind::Null),
        Value::Integer(i) => Ok(*i == int_val(b)?),
        Value::Float(f) => Ok(f.0 == float_val(b)?),
        Value::String(s) => Ok(stricmp(s, str_val(b)?) == Ordering::Equal),
        Value::List(l) => list_eq(l, list_of(b)?),
        Value::Agent(id) => Ok(*id == agent_of(b)?),
        Value::Boolean(x) => Ok(*x == bool_val(b)?),
        Value::Coord(x, y) => Ok((*x, *y) == coord_of(b)?),
        Value::Function(f) => Ok(*f == function_of(b)?),
    }
}

/// `GplType`'s assign: assign `other` to `this` in place. Parameters, return
/// values, `foreach` variables and several builtins write through it.
///
/// **Binary**, each class's method. It is not the `=` operator: a null
/// throws unless `other` is null too, and a
/// string reads `string` by value, not `string&`. A function assigned to a
/// running thread goes to its pending slot, not modelled here.
///
/// ```
/// use regent_core::base::gfix::Gfix;
/// use regent_core::gpl::ops::{assign, OpError, Value};
///
/// // A float into an integer floors, as the integer's `int` accessor does.
/// let mut v = Value::Integer(0);
/// assign(&mut v, &Value::Float(Gfix(-512))).unwrap();
/// assert_eq!(v, Value::Integer(-1));
///
/// assert_eq!(assign(&mut Value::Null, &Value::Integer(1)), Err(OpError::NullAssign));
/// ```
///
/// # Errors
///
/// [`OpError::BadConversion`] when `other` lacks the accessor `this` reads
/// it through; [`OpError::NullAssign`].
pub fn assign(this: &mut Value, other: &Value) -> Res<()> {
    match this {
        Value::Null if other.kind() == Kind::Null => Ok(()),
        Value::Null => Err(OpError::NullAssign),
        Value::Integer(i) => int_val(other).map(|v| *i = v),
        Value::Float(f) => float_val(other).map(|v| f.0 = v),
        Value::String(s) => str_val(other).map(|v| v.clone_into(s)),

        // `GplList` clears first, so `l = 5` leaves `l` empty.
        Value::List(l) => {
            l.clear();
            list_of(other).map(|v| l.extend(v.to_vec()))
        }

        Value::Agent(id) => agent_of(other).map(|v| *id = v),
        Value::Boolean(b) => bool_val(other).map(|v| *b = v),
        Value::Coord(x, y) => coord_of(other).map(|c| (*x, *y) = c),
        Value::Function(f) => function_of(other).map(|v| *f = v),
    }
}

/// The function handle a reset function value holds: the null function
/// record, gpl null record.
pub const NULL_FUNCTION: u32 = 0;

/// `GplType`'s reset: reset `this` to its class's default. Locals on entry
/// and exit, and a call's result before each call.
///
/// **Binary**, each class's release: 0, 0, list emptied, agent
/// [`NULL_AGENT`], false, (0, 0), the null function; a null stays null. A
/// string keeps its text: `GplString` only reserves 0
/// bytes, which leaves an allocated string as it was.
///
/// ```
/// use regent_core::gpl::ops::{reset, Value, NULL_AGENT};
///
/// let mut v = Value::Agent(7);
/// reset(&mut v);
/// assert_eq!(v, Value::Agent(NULL_AGENT));
///
/// let mut s = Value::String("kept".into());
/// reset(&mut s);
/// assert_eq!(s, Value::String("kept".into()));
/// ```
pub fn reset(this: &mut Value) {
    match this {
        Value::String(_) => {}
        Value::List(l) => l.clear(),
        Value::Agent(_) => *this = Value::Agent(NULL_AGENT),
        Value::Function(_) => *this = Value::Function(NULL_FUNCTION),
        _ => *this = Value::fresh(this.kind()),
    }
}

/// `GplList`: pairs elements until either list ends, so a
/// prefix is equal. Each pair is the right element's equality with the left;
/// the first unequal pair ends the walk.
fn list_eq(this: &[Value], other: &[Value]) -> Res<bool> {
    this.iter().zip(other).try_fold(
        true,
        |same, (mine, theirs)| {
            if same { equal(theirs, mine) } else { Ok(false) }
        },
    )
}

/// `_stricmp`: bytes compared after ASCII lowering. **Inferred**: the C
/// locale, which MSVC starts in.
fn stricmp(a: &str, b: &str) -> Ordering {
    let lower = |s: &str| {
        s.bytes()
            .map(|c| c.to_ascii_lowercase())
            .collect::<Vec<_>>()
    };

    lower(a).cmp(&lower(b))
}

/// A comparison opcode's verdict on an ordering.
fn order(op: Operator, ord: Ordering) -> bool {
    match op {
        Operator::Equal => ord == Ordering::Equal,
        Operator::NotEqual => ord != Ordering::Equal,
        Operator::LessThan => ord == Ordering::Less,
        Operator::GreaterThan => ord == Ordering::Greater,
        Operator::LessThanOrEqualTo => ord != Ordering::Greater,
        _ => ord != Ordering::Less,
    }
}

/// C's truncating quotient, faulting where x86 does.
fn idiv(a: i32, b: i32) -> Res<i32> {
    a.checked_div(b).ok_or(OpError::Fault)
}

/// a 32-bit signed divide's remainder, faulting where x86 does.
fn irem(a: i32, b: i32) -> Res<i32> {
    a.checked_rem(b).ok_or(OpError::Fault)
}

/// The "Divide by zero error." check, on a fixed-point divisor.
const fn nonzero(f: i32) -> Res<i32> {
    if f == 0 {
        Err(OpError::DivideByZero)
    } else {
        Ok(f)
    }
}

/// An integer as a fixed-point value, `<< 10` in 32 bits.
const fn widen(i: i32) -> i32 {
    Gfix::from_int(i).0
}

/// A fixed-point value floored to an integer, `>> 10`.
const fn narrow(f: i32) -> i32 {
    Gfix(f).floor()
}

fn operand(rhs: Option<&Value>) -> Res<&Value> {
    rhs.ok_or(OpError::NoOperand)
}

fn slot(dest: &mut Option<Value>) -> Res<&mut Value> {
    dest.as_mut().ok_or(OpError::NoResult)
}

// Accessors on the right operand, slots 10 to 27.

pub(crate) const fn int_of(v: &Value) -> Res<i32> {
    match v {
        Value::Integer(i) => Ok(*i),
        _ => Err(OpError::BadConversion(Accessor::IntRef)),
    }
}

pub(crate) const fn bool_of(v: &Value) -> Res<bool> {
    match v {
        Value::Boolean(b) => Ok(*b),
        _ => Err(OpError::BadConversion(Accessor::BoolRef)),
    }
}

pub(crate) fn string_of(v: &Value) -> Res<&str> {
    match v {
        Value::String(s) => Ok(s),
        _ => Err(OpError::BadConversion(Accessor::StringRef)),
    }
}

pub(crate) fn list_of(v: &Value) -> Res<&[Value]> {
    match v {
        Value::List(l) => Ok(l),
        _ => Err(OpError::BadConversion(Accessor::ListRef)),
    }
}

pub(crate) const fn coord_of(v: &Value) -> Res<(i32, i32)> {
    match v {
        Value::Coord(x, y) => Ok((*x, *y)),
        _ => Err(OpError::BadConversion(Accessor::CoordRef)),
    }
}

pub(crate) const fn function_of(v: &Value) -> Res<u32> {
    match v {
        Value::Function(f) => Ok(*f),
        _ => Err(OpError::BadConversion(Accessor::FunctionRef)),
    }
}

pub(crate) const fn agent_of(v: &Value) -> Res<u32> {
    match v {
        Value::Agent(id) => Ok(*id),
        _ => Err(OpError::BadConversion(Accessor::AgentRef)),
    }
}

/// `int` by value: `GplInteger` as is, `GplFloat` `>> 10`.
///
/// # Errors
///
/// [`OpError::BadConversion`] for any other class.
pub const fn int_val(v: &Value) -> Res<i32> {
    match v {
        Value::Integer(i) => Ok(*i),
        Value::Float(f) => Ok(f.floor()),
        _ => Err(OpError::BadConversion(Accessor::Int)),
    }
}

/// `float` by value: `GplInteger` `<< 10`, `GplFloat` as is.
pub(crate) const fn float_val(v: &Value) -> Res<i32> {
    match v {
        Value::Integer(i) => Ok(widen(*i)),
        Value::Float(f) => Ok(f.0),
        _ => Err(OpError::BadConversion(Accessor::Float)),
    }
}

pub(crate) const fn bool_val(v: &Value) -> Res<bool> {
    match v {
        Value::Boolean(b) => Ok(*b),
        _ => Err(OpError::BadConversion(Accessor::Bool)),
    }
}

pub(crate) fn str_val(v: &Value) -> Res<&str> {
    match v {
        Value::String(s) => Ok(s),
        _ => Err(OpError::BadConversion(Accessor::Str)),
    }
}

// Writes into the result slot, through its reference accessor.

fn set_int(dest: &mut Option<Value>, value: i32) -> Res<()> {
    match slot(dest)? {
        Value::Integer(i) => {
            *i = value;
            Ok(())
        }
        _ => Err(OpError::BadConversion(Accessor::IntRef)),
    }
}

fn set_float(dest: &mut Option<Value>, value: i32) -> Res<()> {
    match slot(dest)? {
        Value::Float(f) => {
            f.0 = value;
            Ok(())
        }
        _ => Err(OpError::BadConversion(Accessor::FloatRef)),
    }
}

fn set_bool(dest: &mut Option<Value>, value: bool) -> Res<()> {
    match slot(dest)? {
        Value::Boolean(b) => {
            *b = value;
            Ok(())
        }
        _ => Err(OpError::BadConversion(Accessor::BoolRef)),
    }
}

fn set_string(dest: &mut Option<Value>, value: String) -> Res<()> {
    match slot(dest)? {
        Value::String(s) => {
            *s = value;
            Ok(())
        }
        _ => Err(OpError::BadConversion(Accessor::StringRef)),
    }
}

/// The result's `bool&`, fetched before the comparison by the cases that do
/// so; its error then comes first.
fn bool_slot(dest: &mut Option<Value>) -> Res<()> {
    match slot(dest)? {
        Value::Boolean(_) => Ok(()),
        _ => Err(OpError::BadConversion(Accessor::BoolRef)),
    }
}

/// `GplInteger`. Where a right float matters the case tests the right
/// operand's type code, not the result's: `int + float` writes `float&`,
/// `int + int` works in fixed point and writes `(a << 10 + b << 10) >> 10`
/// to `int&`.
fn integer(op: Operator, this: &mut i32, rhs: Option<&Value>, dest: &mut Option<Value>) -> Res<()> {
    let right_float = matches!(rhs, Some(Value::Float(_)));

    // A fixed-point result, to `float&` after a right float, else floored
    // to `int&`.
    let put = |dest: &mut Option<Value>, fixed: i32| {
        if right_float {
            set_float(dest, fixed)
        } else {
            set_int(dest, narrow(fixed))
        }
    };

    match op {
        // `GplInteger`: `int` by value, so a float floors.
        Operator::AssignEqual => int_val(operand(rhs)?).map(|v| *this = v),

        // `GplInteger`: the right side floored first, `(f >> 10) * this`.
        Operator::TimesEqual => {
            float_val(operand(rhs)?).map(|f| *this = narrow(f).wrapping_mul(*this))
        }

        // `GplInteger`: checks the fixed value, divides by it floored, so a
        // divisor in (0, 1) faults and -0.5 divides by -1.
        Operator::DivideEqual => {
            let f = nonzero(float_val(operand(rhs)?)?)?;
            idiv(*this, narrow(f)).map(|q| *this = q)
        }

        // `GplInteger`: `int&`, no zero check.
        Operator::ModEqual => irem(*this, int_of(operand(rhs)?)?).map(|r| *this = r),

        // `GplInteger`: the right side floored.
        Operator::PlusEqual => {
            float_val(operand(rhs)?).map(|f| *this = this.wrapping_add(narrow(f)))
        }
        Operator::MinusEqual => {
            float_val(operand(rhs)?).map(|f| *this = this.wrapping_sub(narrow(f)))
        }

        // `GplInteger`: a float on the right is never equal; else `int&`.
        Operator::Equal if right_float => set_bool(dest, false),
        Operator::Equal => {
            let other = int_of(operand(rhs)?)?;
            set_bool(dest, *this == other)
        }

        // `GplInteger` to `GplInteger`: `this << 10` against the fixed value.
        Operator::NotEqual
        | Operator::LessThan
        | Operator::GreaterThan
        | Operator::LessThanOrEqualTo
        | Operator::GreaterThanOrEqualTo => {
            let other = float_val(operand(rhs)?)?;
            set_bool(dest, order(op, widen(*this).cmp(&other)))
        }

        // `GplInteger`.
        Operator::Plus => {
            let other = float_val(operand(rhs)?)?;
            put(dest, widen(*this).wrapping_add(other))
        }
        Operator::Minus => {
            let other = float_val(operand(rhs)?)?;
            put(dest, widen(*this).wrapping_sub(other))
        }

        // `GplInteger`.
        Operator::Star => {
            let other = float_val(operand(rhs)?)?;
            put(dest, fixed_mul(widen(*this), other))
        }

        // `GplInteger`: truncates in fixed point, then floors: -7 / 2 is -4.
        Operator::Divide => {
            let other = nonzero(float_val(operand(rhs)?)?)?;
            put(dest, fixed_div(widen(*this), other)?)
        }

        // `GplInteger`: `int&`, no zero check.
        Operator::Mod => {
            let rest = irem(*this, int_of(operand(rhs)?)?)?;
            set_int(dest, rest)
        }

        Operator::Inc => {
            *this = this.wrapping_add(1);
            Ok(())
        }
        Operator::Dec => {
            *this = this.wrapping_sub(1);
            Ok(())
        }
        Operator::Negate => set_int(dest, this.wrapping_neg()),

        _ => Err(OpError::Unsupported(Kind::Integer)),
    }
}

/// `GplFloat`. The right side is always read as `float` by value, so an
/// integer is widened, and every value op writes `float&`.
fn float(op: Operator, this: &mut i32, rhs: Option<&Value>, dest: &mut Option<Value>) -> Res<()> {
    match op {
        Operator::AssignEqual => float_val(operand(rhs)?).map(|f| *this = f),
        Operator::TimesEqual => float_val(operand(rhs)?).map(|f| *this = fixed_mul(*this, f)),

        Operator::DivideEqual => {
            let other = nonzero(float_val(operand(rhs)?)?)?;
            fixed_div(*this, other).map(|q| *this = q)
        }

        Operator::PlusEqual => float_val(operand(rhs)?).map(|f| *this = this.wrapping_add(f)),
        Operator::MinusEqual => float_val(operand(rhs)?).map(|f| *this = this.wrapping_sub(f)),

        Operator::Equal
        | Operator::NotEqual
        | Operator::LessThan
        | Operator::GreaterThan
        | Operator::LessThanOrEqualTo
        | Operator::GreaterThanOrEqualTo => {
            let other = float_val(operand(rhs)?)?;
            set_bool(dest, order(op, (*this).cmp(&other)))
        }

        Operator::Plus => {
            let other = float_val(operand(rhs)?)?;
            set_float(dest, this.wrapping_add(other))
        }
        Operator::Minus => {
            let other = float_val(operand(rhs)?)?;
            set_float(dest, this.wrapping_sub(other))
        }
        Operator::Star => {
            let other = float_val(operand(rhs)?)?;
            set_float(dest, fixed_mul(*this, other))
        }

        Operator::Divide => {
            let other = nonzero(float_val(operand(rhs)?)?)?;
            set_float(dest, fixed_div(*this, other)?)
        }

        // `++` adds 1.0, not the smallest step.
        Operator::Inc => {
            *this = this.wrapping_add(Gfix::ONE.0);
            Ok(())
        }
        Operator::Dec => {
            *this = this.wrapping_sub(Gfix::ONE.0);
            Ok(())
        }
        Operator::Negate => set_float(dest, this.wrapping_neg()),

        _ => Err(OpError::Unsupported(Kind::Float)),
    }
}

/// `GplString`. Comparisons are `_stricmp`: case-insensitive, so
/// `"Gold" == "GOLD"`. The right side must be a string (`string&`).
fn string(
    op: Operator,
    this: &mut String,
    rhs: Option<&Value>,
    dest: &mut Option<Value>,
) -> Res<()> {
    match op {
        Operator::AssignEqual => string_of(operand(rhs)?).map(|s| s.clone_into(this)),
        Operator::PlusEqual => string_of(operand(rhs)?).map(|s| this.push_str(s)),

        Operator::Equal
        | Operator::NotEqual
        | Operator::LessThan
        | Operator::GreaterThan
        | Operator::LessThanOrEqualTo
        | Operator::GreaterThanOrEqualTo => {
            let ord = stricmp(this, string_of(operand(rhs)?)?);
            set_bool(dest, order(op, ord))
        }

        Operator::Plus => {
            let joined = format!("{this}{}", string_of(operand(rhs)?)?);
            set_string(dest, joined)
        }

        // `GplString`: `"a" 's "b"` is `"a'b"`.
        Operator::Apostrophe => {
            let joined = format!("{this}'{}", string_of(operand(rhs)?)?);
            set_string(dest, joined)
        }

        _ => Err(OpError::Unsupported(Kind::String)),
    }
}

/// `GplList`. Lists hold values, copied in and out.
fn list(op: Operator, this: &mut List, rhs: Option<&Value>, dest: &mut Option<Value>) -> Res<()> {
    match op {
        // `GplList`: clears, then reads `list&`, so `l = 5` empties `l`
        // before it throws. Elements are deep copies.
        Operator::AssignEqual => {
            this.clear();
            list_of(operand(rhs)?).map(|l| this.extend(l.to_vec()))
        }

        // `GplList`: a copy of the right value, of its own type, appended;
        // a list goes in as one element.
        Operator::PlusEqual | Operator::Inserter => operand(rhs).map(|v| this.push(v.clone())),

        // `GplList`: removes every element equal to the right value, by the
        // element's equality. One that throws stops the walk: what went
        // before is removed, the rest kept.
        Operator::MinusEqual => {
            let other = operand(rhs)?;
            let mut failed = None;

            this.retain(|e| {
                if failed.is_some() {
                    true
                } else {
                    match equal(e, other) {
                        Ok(same) => !same,
                        Err(err) => {
                            failed = Some(err);
                            true
                        }
                    }
                }
            });

            failed.map_or(Ok(()), Err)
        }

        Operator::Equal => {
            bool_slot(dest)?;
            let same = list_eq(this, list_of(operand(rhs)?)?)?;
            set_bool(dest, same)
        }

        Operator::NotEqual => {
            let same = list_eq(this, list_of(operand(rhs)?)?)?;
            set_bool(dest, !same)
        }

        _ => Err(OpError::Unsupported(Kind::List)),
    }
}

/// `GplAgentRef`. Agents compare by id; `'s` hands the VM an attribute.
fn agent(op: Operator, id: &mut u32, rhs: Option<&Value>, dest: &mut Option<Value>) -> Res<Step> {
    match op {
        Operator::AssignEqual => agent_of(operand(rhs)?).map(|other| {
            *id = other;
            Step::Done
        }),

        Operator::Equal | Operator::NotEqual => {
            let same = *id == agent_of(operand(rhs)?)?;
            set_bool(dest, same == (op == Operator::Equal)).map(|()| Step::Done)
        }

        Operator::Apostrophe => string_of(operand(rhs)?).map(|name| Step::Attribute {
            agent: *id,
            name: name.to_owned(),
        }),

        _ => Err(OpError::Unsupported(Kind::Agent)),
    }
}

/// `GplBoolean`. `||` and `&&` read the right side only when the left
/// does not decide, but both operands were computed before the record ran.
fn boolean(
    op: Operator,
    this: &mut bool,
    rhs: Option<&Value>,
    dest: &mut Option<Value>,
) -> Res<()> {
    match op {
        // `GplBoolean`: `bool` by value.
        Operator::AssignEqual => bool_val(operand(rhs)?).map(|b| *this = b),

        // `bool&`: `true || 5` is true, `false || 5` throws.
        Operator::Or => {
            let value = *this || bool_of(operand(rhs)?)?;
            set_bool(dest, value)
        }
        Operator::And => {
            let value = *this && bool_of(operand(rhs)?)?;
            set_bool(dest, value)
        }

        Operator::Equal => {
            bool_slot(dest)?;
            let same = *this == bool_val(operand(rhs)?)?;
            set_bool(dest, same)
        }
        Operator::NotEqual => {
            let same = *this == bool_val(operand(rhs)?)?;
            set_bool(dest, !same)
        }

        _ => Err(OpError::Unsupported(Kind::Boolean)),
    }
}

/// `GplCoord`: assignment and component-wise equality only.
fn coord(
    op: Operator,
    this: (&mut i32, &mut i32),
    rhs: Option<&Value>,
    dest: &mut Option<Value>,
) -> Res<()> {
    let (x, y) = this;

    match op {
        Operator::AssignEqual => coord_of(operand(rhs)?).map(|c| (*x, *y) = c),

        Operator::Equal | Operator::NotEqual => {
            let same = (*x, *y) == coord_of(operand(rhs)?)?;
            set_bool(dest, same == (op == Operator::Equal))
        }

        _ => Err(OpError::Unsupported(Kind::Coord)),
    }
}

/// `GplFunction`. **Binary**, not modelled: assigning to a running
/// thread's function stores it as pending,.
fn function(
    op: Operator,
    this: &mut u32,
    rhs: Option<&Value>,
    dest: &mut Option<Value>,
) -> Res<()> {
    match op {
        Operator::AssignEqual => function_of(operand(rhs)?).map(|f| *this = f),

        Operator::Equal => {
            bool_slot(dest)?;
            let same = *this == function_of(operand(rhs)?)?;
            set_bool(dest, same)
        }
        Operator::NotEqual => {
            let same = *this == function_of(operand(rhs)?)?;
            set_bool(dest, !same)
        }

        _ => Err(OpError::Unsupported(Kind::Function)),
    }
}

/// To and from the save model.
mod snap {
    use super::*;
    use crate::save::{self as m, Refused, Saved};
    use std::result::Result;

    impl Saved for Value {
        type Model = m::Value;

        fn save(&self) -> m::Value {
            use m::Value as M;

            match self {
                Value::Null => M::Null,
                Value::Integer(n) => M::Integer(*n),
                Value::Float(f) => M::Float(f.0),
                Value::String(s) => M::String(s.clone()),
                Value::List(l) => M::List(l.save()),
                Value::Agent(a) => M::Agent(*a),
                Value::Boolean(b) => M::Boolean(*b),
                Value::Coord(x, y) => M::Coord(*x, *y),
                Value::Function(f) => M::Function(*f),
            }
        }

        fn load(model: m::Value) -> Result<Value, Refused> {
            use m::Value as M;

            Ok(match model {
                M::Null => Value::Null,
                M::Integer(n) => Value::Integer(n),
                M::Float(f) => Value::Float(Gfix(f)),
                M::String(s) => Value::String(s),
                M::List(l) => Value::List(List::load(l)?),
                M::Agent(a) => Value::Agent(a),
                M::Boolean(b) => Value::Boolean(b),
                M::Coord(x, y) => Value::Coord(x, y),
                M::Function(f) => Value::Function(f),
            })
        }
    }
}
