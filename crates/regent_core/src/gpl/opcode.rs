//! The VM's opcode table, read out of the engine.
//!
//! [`Op`] names all 43 opcodes the VM runs; [`Operator`] the 24 a value's
//! class runs. Both number as the bytecode does.
//!
//! **Binary.** `.data` holds 39 fixed-stride records, 4 KiB past the attribute
//! table (the type names sit in `.rodata`):
//!
//! ```text
//! 0x00  char[25]  name    "opAssignEqual"
//! 0x19  char[25]  token   "="
//! ```
//!
//! Fields are NUL-padded; zeros bound the array. `MajestyHD.exe` holds 39 entries.
//!
//! ## The index is the bytecode number
//!
//! [`crate::gpl::bcd`] reads a call as `[u8 n][u16 slot × n][0x26]`, with
//! `0x26` taken from the compiled files alone. `0x26` is 38: `opLParen`, token
//! `call`. Two independent sources agree. See [`CALL`].
//!
//! Section-1 records carry a source line. On one function with source, the
//! opcodes read `opEqual` on the line of its only `==` and `opIf` on the line
//! of its only `if`; lines ascend with the source and arities match.
//!
//! [`crate::gpl::bcd::walk`] reads the full record framing, from
//! `Gplbcc.exe`'s writer.
//! Opcode 39, off the end of this table, marks where a body begins; see
//! [`crate::gpl::bcd::Record::opcode`]. The VM runs 43 opcodes: this table's 39,
//! [`BODY`], and three [`INLINE_ATTRIBUTE`] ones.
//!
//! ## It checks against the lexer
//!
//! `crate::gpl::lexer` was written from the SDK reference and corpus, without
//! this table. Every token here lexes without error: 32 distinct spellings over
//! the 34 entries with one (`-` and `return` twice each). See
//! `every_opcode_token_is_one_the_lexer_knows`.
//!
//! `opApostrophe_s` is `'s`, GPL's possessive, too unusual for chance.
//! `opInserter` is `<<` (parser: `Append`), named for the C++ operator.
//!
//! ## The five with no token
//!
//! `opNop`, `opLocalize`, `opAdvance`, `opRewind`, `opExit`: compiler-only, no
//! `.gpl` file writes them. **Inferred**: `opAdvance` and `opRewind`, beside
//! `opForeach`, are a loop's cursor.
//!
//! `opNegate` shares `-` with `opMinus`; the parser decides which is unary.

/// One entry of the table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Opcode {
    /// The engine's name, as `opAssignEqual`.
    pub name: &'static str,
    /// How a `.gpl` file writes it; empty where none does.
    pub token: &'static str,
}

/// The name printer's bound in `MajestyHD.exe`: below `0x2b`, so
/// opcodes 0 to 42 index the table and larger ones print `illegal op`. The
/// VM's 43. 39 prints an empty name, 40 to 42 whatever follows the table.
pub const HD_NAME_BOUND: u8 = 43;

/// The first opcode the VM's switch takes, `opNop`. Below it, 0 to 23, the
/// operand's value type applies the operator: a
/// 24-case switch in `GplInteger` and `GplFloat`. The switch takes 24 to 42;
/// past 42: "Unknown operation attempted in interpreter", thrown.
pub const FIRST_CONTROL: u8 = Op::Nop.code();

/// Opcode 39, the body boundary. Its case is `opNop`'s: nothing at run time.
/// The function's exit walks the `opDeclare` records before it, releasing each
/// local. **Inferred**: the release, from a call through the value's type.
pub const BODY: u8 = Op::Body.code();

/// Opcodes 40 to 42, which the VM runs and the name table lacks. Named by the
/// profiler strings their cases log. No shipped `.bcd` uses them, and
/// `Gplbcc.exe` compiled all of those. **Inferred**: an inlined attribute
/// access the shipped compiler never emits.
pub const INLINE_ATTRIBUTE: [(u8, &str); 3] = [
    (40, "GetAttribute(inl)"),
    (41, "SetAttribute(inl)"),
    (42, "AdjustAttribute(inl)"),
];

/// The call opcode, closing an operand list in section 1.
///
/// Read off the compiled files by [`crate::gpl::bcd`], and independently this
/// table's index for `opLParen`, token `call`.
pub const CALL: u8 = Op::Call.code();

/// Every opcode the VM runs, by bytecode number.
///
/// **Binary.** 0 to 38 are [`TABLE`]; 39 is [`BODY`]; 40 to
/// 42 are [`INLINE_ATTRIBUTE`], which no shipped `.bcd` holds and no
/// load makes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Op {
    AssignEqual,
    TimesEqual,
    DivideEqual,
    ModEqual,
    PlusEqual,
    MinusEqual,
    Inserter,
    Or,
    And,
    Equal,
    NotEqual,
    LessThan,
    GreaterThan,
    LessThanOrEqualTo,
    GreaterThanOrEqualTo,
    Plus,
    Minus,
    Star,
    Divide,
    Mod,
    Apostrophe,
    Inc,
    Dec,
    Negate,
    Nop,
    Localize,
    Reference,
    Declare,
    Advance,
    Rewind,
    Exit,
    If,
    Foreach,
    While,
    Continue,
    Break,
    Return,
    ReturnValue,
    Call,
    Body,
    GetAttribute,
    SetAttribute,
    AdjustAttribute,
}

impl Op {
    /// All 43, index = bytecode number.
    pub const ALL: [Op; 43] = [
        Op::AssignEqual,
        Op::TimesEqual,
        Op::DivideEqual,
        Op::ModEqual,
        Op::PlusEqual,
        Op::MinusEqual,
        Op::Inserter,
        Op::Or,
        Op::And,
        Op::Equal,
        Op::NotEqual,
        Op::LessThan,
        Op::GreaterThan,
        Op::LessThanOrEqualTo,
        Op::GreaterThanOrEqualTo,
        Op::Plus,
        Op::Minus,
        Op::Star,
        Op::Divide,
        Op::Mod,
        Op::Apostrophe,
        Op::Inc,
        Op::Dec,
        Op::Negate,
        Op::Nop,
        Op::Localize,
        Op::Reference,
        Op::Declare,
        Op::Advance,
        Op::Rewind,
        Op::Exit,
        Op::If,
        Op::Foreach,
        Op::While,
        Op::Continue,
        Op::Break,
        Op::Return,
        Op::ReturnValue,
        Op::Call,
        Op::Body,
        Op::GetAttribute,
        Op::SetAttribute,
        Op::AdjustAttribute,
    ];

    /// The opcode with bytecode number `code`; `None` past 42, where the VM
    /// throws.
    #[must_use]
    pub fn from_code(code: u8) -> Option<Op> {
        Self::ALL.get(usize::from(code)).copied()
    }

    #[must_use]
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// The engine's name: the table's, the profiler's for 40 to 42, empty for
    /// [`Op::Body`] as opcode name prints it.
    #[must_use]
    pub fn name(self) -> &'static str {
        let code = self.code();

        TABLE.get(usize::from(code)).map_or_else(
            || {
                INLINE_ATTRIBUTE
                    .iter()
                    .find(|(at, _)| *at == code)
                    .map_or("", |(_, name)| name)
            },
            |entry| entry.name,
        )
    }

    /// Operators, 0 to 23: the left operand's type runs them.
    #[must_use]
    pub const fn is_operator(self) -> bool {
        self.code() < FIRST_CONTROL
    }

    /// The token the source writes; empty where none does, and past the
    /// table.
    #[must_use]
    pub const fn token(self) -> &'static str {
        if (self as usize) < TABLE.len() {
            TABLE[self as usize].token
        } else {
            ""
        }
    }
}

/// A code past 42.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnknownOp(pub u8);

impl TryFrom<u8> for Op {
    type Error = UnknownOp;

    fn try_from(code: u8) -> Result<Op, UnknownOp> {
        Op::from_code(code).ok_or(UnknownOp(code))
    }
}

/// The 24 operators, 0 to [`FIRST_CONTROL`]: the opcodes the left operand's
/// value class runs, never the VM switch. Same names and numbers as [`Op`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum Operator {
    AssignEqual = Op::AssignEqual as u8,
    TimesEqual = Op::TimesEqual as u8,
    DivideEqual = Op::DivideEqual as u8,
    ModEqual = Op::ModEqual as u8,
    PlusEqual = Op::PlusEqual as u8,
    MinusEqual = Op::MinusEqual as u8,
    Inserter = Op::Inserter as u8,
    Or = Op::Or as u8,
    And = Op::And as u8,
    Equal = Op::Equal as u8,
    NotEqual = Op::NotEqual as u8,
    LessThan = Op::LessThan as u8,
    GreaterThan = Op::GreaterThan as u8,
    LessThanOrEqualTo = Op::LessThanOrEqualTo as u8,
    GreaterThanOrEqualTo = Op::GreaterThanOrEqualTo as u8,
    Plus = Op::Plus as u8,
    Minus = Op::Minus as u8,
    Star = Op::Star as u8,
    Divide = Op::Divide as u8,
    Mod = Op::Mod as u8,
    Apostrophe = Op::Apostrophe as u8,
    Inc = Op::Inc as u8,
    Dec = Op::Dec as u8,
    Negate = Op::Negate as u8,
}

impl Operator {
    /// Every operator, in opcode order.
    pub const ALL: [Operator; 24] = [
        Operator::AssignEqual,
        Operator::TimesEqual,
        Operator::DivideEqual,
        Operator::ModEqual,
        Operator::PlusEqual,
        Operator::MinusEqual,
        Operator::Inserter,
        Operator::Or,
        Operator::And,
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
        Operator::Apostrophe,
        Operator::Inc,
        Operator::Dec,
        Operator::Negate,
    ];

    /// The operator an opcode names; `None` from [`FIRST_CONTROL`] up.
    #[must_use]
    pub fn from_code(code: u8) -> Option<Operator> {
        Operator::ALL.get(usize::from(code)).copied()
    }

    /// The opcode.
    #[must_use]
    pub const fn code(self) -> u8 {
        self as u8
    }

    #[must_use]
    pub const fn op(self) -> Op {
        Op::ALL[self as usize]
    }

    /// Whether it writes its left operand: the assignments, `++`, `--`.
    #[must_use]
    pub const fn writes_lhs(self) -> bool {
        matches!(
            self,
            Operator::AssignEqual
                | Operator::TimesEqual
                | Operator::DivideEqual
                | Operator::ModEqual
                | Operator::PlusEqual
                | Operator::MinusEqual
                | Operator::Inserter
                | Operator::Inc
                | Operator::Dec
        )
    }
}

/// An opcode that is no operator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NotOperator(pub Op);

impl TryFrom<Op> for Operator {
    type Error = NotOperator;

    fn try_from(op: Op) -> Result<Operator, NotOperator> {
        Operator::from_code(op.code()).ok_or(NotOperator(op))
    }
}

impl From<Operator> for Op {
    fn from(operator: Operator) -> Op {
        operator.op()
    }
}

/// Every opcode in engine array order; the index is the bytecode number.
pub const TABLE: [Opcode; 39] = [
    Opcode {
        name: "opAssignEqual",
        token: "=",
    },
    Opcode {
        name: "opTimesEqual",
        token: "*=",
    },
    Opcode {
        name: "opDivideEqual",
        token: "/=",
    },
    Opcode {
        name: "opModEqual",
        token: "%=",
    },
    Opcode {
        name: "opPlusEqual",
        token: "+=",
    },
    Opcode {
        name: "opMinusEqual",
        token: "-=",
    },
    Opcode {
        name: "opInserter",
        token: "<<",
    },
    Opcode {
        name: "opOr",
        token: "||",
    },
    Opcode {
        name: "opAnd",
        token: "&&",
    },
    Opcode {
        name: "opEqual",
        token: "==",
    },
    Opcode {
        name: "opNotEqual",
        token: "!=",
    },
    Opcode {
        name: "opLessThan",
        token: "<",
    },
    Opcode {
        name: "opGreaterThan",
        token: ">",
    },
    Opcode {
        name: "opLessThanOrEqualTo",
        token: "<=",
    },
    Opcode {
        name: "opGreaterThanOrEqualTo",
        token: ">=",
    },
    Opcode {
        name: "opPlus",
        token: "+",
    },
    Opcode {
        name: "opMinus",
        token: "-",
    },
    Opcode {
        name: "opStar",
        token: "*",
    },
    Opcode {
        name: "opDivide",
        token: "/",
    },
    Opcode {
        name: "opMod",
        token: "%",
    },
    Opcode {
        name: "opApostrophe_s",
        token: "'s",
    },
    Opcode {
        name: "opInc",
        token: "++",
    },
    Opcode {
        name: "opDec",
        token: "--",
    },
    Opcode {
        name: "opNegate",
        token: "-",
    },
    Opcode {
        name: "opNop",
        token: "",
    },
    Opcode {
        name: "opLocalize",
        token: "",
    },
    Opcode {
        name: "opReference",
        token: "declare return",
    },
    Opcode {
        name: "opDeclare",
        token: "declare",
    },
    Opcode {
        name: "opAdvance",
        token: "",
    },
    Opcode {
        name: "opRewind",
        token: "",
    },
    Opcode {
        name: "opExit",
        token: "",
    },
    Opcode {
        name: "opIf",
        token: "if",
    },
    Opcode {
        name: "opForeach",
        token: "foreach",
    },
    Opcode {
        name: "opWhile",
        token: "while",
    },
    Opcode {
        name: "opContinue",
        token: "continue",
    },
    Opcode {
        name: "opBreak",
        token: "break",
    },
    Opcode {
        name: "opReturn",
        token: "return",
    },
    Opcode {
        name: "opReturnValue",
        token: "return",
    },
    Opcode {
        name: "opLParen",
        token: "call",
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_opcode_is_its_own_bytecode_number() {
        for (op, code) in Op::ALL.iter().zip(0u8..) {
            assert_eq!(op.code(), code, "{op:?}");
            assert_eq!(Op::from_code(code), Some(*op));
        }

        assert_eq!(Op::from_code(HD_NAME_BOUND), None);
        assert_eq!(Op::try_from(HD_NAME_BOUND), Err(UnknownOp(HD_NAME_BOUND)));
    }

    #[test]
    fn the_names_are_the_engines() {
        for (entry, op) in TABLE.iter().zip(Op::ALL) {
            assert_eq!(op.name(), entry.name);
            assert_eq!(op.token(), entry.token);
        }

        assert_eq!(Op::Body.code(), 39);
        assert_eq!(Op::Body.name(), "");
        assert_eq!(Op::Call.code(), 38);

        for (code, name) in INLINE_ATTRIBUTE {
            assert_eq!(Op::from_code(code).map(Op::name), Some(name));
        }
    }

    #[test]
    fn operators_end_where_the_switch_begins() {
        assert!(Op::Negate.is_operator());
        assert!(!Op::Nop.is_operator());
        assert_eq!(FIRST_CONTROL, 24);

        for op in Op::ALL {
            let operator = Operator::try_from(op);

            assert_eq!(operator.is_ok(), op.is_operator(), "{op:?}");
            assert_eq!(operator.map(Op::from), operator.map(|_| op));
        }
    }
}
