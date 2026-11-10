//! What each GPL operator does, in worked cases: one per class and operator
//! kind through [`ops::apply`], citing the case.

use regent_core::base::gfix::Gfix;
use regent_core::data::gpltype::{Kind, TypeCode};
use regent_core::gpl::ops::{self, Accessor, Class, OpError, Operator, Step, Value};

/// A GPL float's fractional bits.
const SCALE: u32 = Gfix::FRACTION_BITS;

/// The model throws [`OpError::Unsupported`] on exactly the opcodes a class
/// does not list; a null left operand takes every opcode and does nothing.
#[test]
fn the_model_refuses_exactly_what_each_class_does_not_handle() {
    let samples = [
        Value::Null,
        Value::Integer(3),
        Value::Float(Gfix(1536)),
        Value::String("a".into()),
        Value::List(vec![Value::Integer(1)].into()),
        Value::Agent(7),
        Value::Boolean(true),
        Value::Coord(1, 2),
        Value::Function(9),
    ];

    for lhs in &samples {
        let class: &Class = ops::class(lhs.kind());

        for op in Operator::ALL {
            for rhs in &samples {
                let (mut left, mut dest) = (lhs.clone(), None);
                let got = ops::apply(op, &mut left, Some(rhs), &mut dest);
                let refused = got == Err(OpError::Unsupported(lhs.kind()));

                assert_eq!(
                    refused,
                    !class.handles(op),
                    "{lhs:?} {op:?} {rhs:?}: {got:?}"
                );

                if lhs.kind() == Kind::Null {
                    assert_eq!((got, left), (Ok(Step::Done), Value::Null));
                }
            }
        }
    }
}

// Worked cases. `run` applies one operator with a typed or empty result slot.

#[allow(clippy::needless_pass_by_value, reason = "cases read better by value")]
fn run(
    op: Operator,
    lhs: Value,
    rhs: Value,
    dest: Option<Value>,
) -> (Value, Result<Option<Value>, OpError>) {
    let (mut left, mut slot) = (lhs, dest);
    let got = ops::apply(op, &mut left, Some(&rhs), &mut slot).map(|_| slot);

    (left, got)
}

/// A unary operator: the left operand after, and the result.
fn unary(op: Operator, lhs: Value) -> (Value, Result<Option<Value>, OpError>) {
    let (mut left, mut slot) = (lhs, None);
    let got = ops::apply(op, &mut left, None, &mut slot).map(|_| slot);

    (left, got)
}

/// The result, into an empty slot.
fn value(op: Operator, lhs: Value, rhs: Value) -> Result<Option<Value>, OpError> {
    run(op, lhs, rhs, None).1
}

/// The result, into a boolean slot the compiler typed.
fn judged(op: Operator, lhs: Value, rhs: Value) -> Result<Option<Value>, OpError> {
    run(op, lhs, rhs, Some(Value::Boolean(false))).1
}

/// The left operand after an assignment operator.
fn assigned(op: Operator, lhs: Value, rhs: Value) -> Result<Value, OpError> {
    let (left, got) = run(op, lhs, rhs, None);

    got.map(|_| left)
}

const fn int(i: i32) -> Value {
    Value::Integer(i)
}

/// A float from its fixed-point word.
const fn fixed(f: i32) -> Value {
    Value::Float(Gfix(f))
}

fn text(s: &str) -> Value {
    Value::String(s.to_owned())
}

/// A boolean result.
const fn yes(b: bool) -> Value {
    Value::Boolean(b)
}

const ONE: i32 = 1 << SCALE;
const HALF: i32 = ONE / 2;

/// `GplInteger`: truncated in fixed point, then floored, so negative quotients
/// round down.
#[test]
fn integer_division_floors() {
    assert_eq!(value(Operator::Divide, int(7), int(2)), Ok(Some(int(3))));
    assert_eq!(value(Operator::Divide, int(-7), int(2)), Ok(Some(int(-4))));
    assert_eq!(value(Operator::Divide, int(-1), int(3)), Ok(Some(int(-1))));
    assert_eq!(
        value(Operator::Divide, int(1), int(0)),
        Err(OpError::DivideByZero)
    );
}

/// `GplInteger`: integers pass through `<< 10` in 32
/// bits, so past 2^21 they wrap.
#[test]
fn integer_arithmetic_wraps_past_two_to_the_twenty_first() {
    assert_eq!(
        value(Operator::Plus, int(3_000_000), int(0)),
        Ok(Some(int(-1_194_304)))
    );
    assert_eq!(
        value(Operator::Star, int(2000), int(2000)),
        Ok(Some(int(-194_304)))
    );
    assert_eq!(
        value(Operator::GreaterThan, int(3_000_000), int(0)),
        Ok(Some(yes(false)))
    );

    // `==` reads `int&`, no shift.
    assert_eq!(
        value(Operator::Equal, int(3_000_000), int(3_000_000)),
        Ok(Some(yes(true)))
    );
    assert_eq!(
        value(Operator::NotEqual, int(0), int(1 << 22)),
        Ok(Some(yes(false)))
    );
}

/// `GplInteger`, `GplFloat`: an integer never equals a float, but
/// is unequal only in fixed point; a float left of an integer compares.
#[test]
fn integer_equality_with_a_float_is_false() {
    assert_eq!(
        value(Operator::Equal, int(2), fixed(2 * ONE)),
        Ok(Some(yes(false)))
    );
    assert_eq!(
        value(Operator::NotEqual, int(2), fixed(2 * ONE)),
        Ok(Some(yes(false)))
    );
    assert_eq!(
        value(Operator::NotEqual, int(2), fixed(2 * ONE + HALF)),
        Ok(Some(yes(true)))
    );
    assert_eq!(
        value(Operator::Equal, fixed(2 * ONE), int(2)),
        Ok(Some(yes(true)))
    );

    // `int&` on a boolean.
    assert_eq!(
        value(Operator::Equal, int(1), Value::Boolean(true)),
        Err(OpError::BadConversion(Accessor::IntRef))
    );
}

/// `GplInteger`: a float on the right writes `float&`. The table types the slot
/// float; an integer slot throws.
#[test]
fn integer_plus_float_writes_a_float() {
    assert_eq!(
        value(Operator::Plus, int(1), fixed(HALF)),
        Ok(Some(fixed(ONE + HALF)))
    );
    assert_eq!(
        run(Operator::Plus, int(1), fixed(HALF), Some(int(0))).1,
        Err(OpError::BadConversion(Accessor::FloatRef))
    );
    assert_eq!(
        value(Operator::Minus, int(1), fixed(HALF)),
        Ok(Some(fixed(HALF)))
    );
}

/// `GplInteger`: the right side floors
/// before it acts.
#[test]
fn integer_assignments_floor_the_right_side() {
    assert_eq!(
        assigned(Operator::AssignEqual, int(0), fixed(2 * ONE + 700)),
        Ok(int(2))
    );
    assert_eq!(
        assigned(Operator::AssignEqual, int(0), fixed(-HALF)),
        Ok(int(-1))
    );
    assert_eq!(
        assigned(Operator::TimesEqual, int(10), fixed(2 * ONE + HALF)),
        Ok(int(20))
    );
    assert_eq!(
        assigned(Operator::TimesEqual, int(10), fixed(-HALF)),
        Ok(int(-10))
    );
    assert_eq!(
        assigned(Operator::PlusEqual, int(1), fixed(2 * ONE + 700)),
        Ok(int(3))
    );

    assert_eq!(
        assigned(Operator::DivideEqual, int(10), fixed(2 * ONE + 900)),
        Ok(int(5))
    );
    assert_eq!(
        assigned(Operator::DivideEqual, int(10), fixed(-HALF)),
        Ok(int(-10))
    );
    assert_eq!(
        assigned(Operator::DivideEqual, int(10), fixed(HALF)),
        Err(OpError::Fault)
    );
    assert_eq!(
        assigned(Operator::DivideEqual, int(10), int(0)),
        Err(OpError::DivideByZero)
    );
}

/// `GplInteger`: `%` reads `int&` and checks nothing.
#[test]
fn integer_remainder_has_no_zero_check() {
    assert_eq!(value(Operator::Mod, int(-7), int(2)), Ok(Some(int(-1))));
    assert_eq!(value(Operator::Mod, int(7), int(0)), Err(OpError::Fault));
    assert_eq!(
        assigned(Operator::ModEqual, int(7), int(0)),
        Err(OpError::Fault)
    );
    assert_eq!(
        assigned(Operator::ModEqual, int(7), fixed(2 * ONE)),
        Err(OpError::BadConversion(Accessor::IntRef))
    );
}

/// `GplInteger`, and the throw.
#[test]
fn integer_unary_and_unsupported() {
    assert_eq!(unary(Operator::Inc, int(1)).0, int(2));
    assert_eq!(unary(Operator::Negate, int(5)).1, Ok(Some(int(-5))));
    assert_eq!(
        unary(Operator::Negate, fixed(HALF)).1,
        Ok(Some(fixed(-HALF)))
    );

    let refused = value(Operator::Inserter, int(1), int(1)).unwrap_err();
    assert_eq!(
        refused.message().as_deref(),
        Some("Unsupported operation attempted on INTEGER type")
    );
}

/// `GplFloat`: fixed-point multiply and divide; `++`
/// adds 1.0.
#[test]
fn float_arithmetic_is_fixed_point() {
    assert_eq!(
        value(Operator::Star, fixed(ONE + HALF), fixed(ONE + HALF)),
        Ok(Some(fixed(2304)))
    );
    assert_eq!(
        value(Operator::Divide, fixed(ONE), fixed(3 * ONE)),
        Ok(Some(fixed(341)))
    );
    assert_eq!(
        value(Operator::Divide, fixed(ONE), int(0)),
        Err(OpError::DivideByZero)
    );
    assert_eq!(
        value(Operator::Divide, fixed(2_048_000_000), fixed(1)),
        Err(OpError::Fault)
    );
    assert_eq!(unary(Operator::Inc, fixed(HALF)).0, fixed(ONE + HALF));
}

/// `GplFloat`, and the throw in `GplFloat`: a float result always goes to
/// `float&`; `%` is not a float operator.
#[test]
fn float_results_need_a_float_slot() {
    assert_eq!(
        value(Operator::Plus, fixed(HALF), int(1)),
        Ok(Some(fixed(ONE + HALF)))
    );
    assert_eq!(
        run(Operator::Plus, fixed(HALF), int(1), Some(int(0))).1,
        Err(OpError::BadConversion(Accessor::FloatRef))
    );
    assert_eq!(
        value(Operator::Mod, fixed(ONE), fixed(ONE)),
        Err(OpError::Unsupported(Kind::Float))
    );
    assert_eq!(
        assigned(Operator::AssignEqual, fixed(0), Value::Boolean(true)),
        Err(OpError::BadConversion(Accessor::Float))
    );
}

/// `GplString` to `GplString`: `_stricmp`, which lowers: `_` sorts below `A`.
#[test]
fn string_comparison_ignores_case() {
    assert_eq!(
        value(Operator::Equal, text("Gold"), text("GOLD")),
        Ok(Some(yes(true)))
    );
    assert_eq!(
        judged(Operator::LessThan, text("a"), text("B")),
        Ok(Some(yes(true)))
    );
    assert_eq!(
        judged(Operator::LessThan, text("_"), text("A")),
        Ok(Some(yes(true)))
    );

    // The table types no string ordering: the record must.
    assert_eq!(
        value(Operator::LessThan, text("a"), text("b")),
        Err(OpError::NoResult)
    );
    assert_eq!(
        value(Operator::Equal, text("1"), int(1)),
        Err(OpError::BadConversion(Accessor::StringRef))
    );
}

/// `GplString`, and the throw.
#[test]
fn string_joins() {
    assert_eq!(
        value(Operator::Plus, text("a"), text("b")),
        Ok(Some(text("ab")))
    );
    assert_eq!(
        run(Operator::Apostrophe, text("a"), text("b"), Some(text(""))).1,
        Ok(Some(text("a'b")))
    );
    assert_eq!(
        assigned(Operator::PlusEqual, text("a"), text("b")),
        Ok(text("ab"))
    );
    assert_eq!(
        value(Operator::Minus, text("a"), text("b")),
        Err(OpError::Unsupported(Kind::String))
    );
}

/// `GplList`, `GplList` in `GplList`: lists are equal up to the
/// shorter one's end.
#[test]
fn a_list_equals_its_prefix() {
    let list = |v: &[i32]| Value::List(v.iter().copied().map(int).collect());

    assert_eq!(
        value(Operator::Equal, list(&[1, 2]), list(&[1, 2, 3])),
        Ok(Some(yes(true)))
    );
    assert_eq!(
        value(Operator::Equal, list(&[1, 2]), list(&[1, 3])),
        Ok(Some(yes(false)))
    );
    assert_eq!(
        value(Operator::Equal, list(&[]), list(&[5])),
        Ok(Some(yes(true)))
    );
    assert_eq!(
        value(Operator::NotEqual, list(&[1]), list(&[2])),
        Ok(Some(yes(true)))
    );
}

/// `GplList`, and the throw.
#[test]
fn list_insertion_removal_and_assignment() {
    let list = |v: Vec<Value>| Value::List(v.into());

    assert_eq!(
        assigned(Operator::Inserter, list(vec![]), list(vec![int(2)])),
        Ok(list(vec![list(vec![int(2)])]))
    );
    assert_eq!(
        assigned(
            Operator::MinusEqual,
            list(vec![int(1), int(2), int(1)]),
            int(1)
        ),
        Ok(list(vec![int(2)]))
    );

    // The element's equality reads `int`: a string throws.
    assert_eq!(
        assigned(Operator::MinusEqual, list(vec![int(1)]), text("x")),
        Err(OpError::BadConversion(Accessor::Int))
    );

    // Cleared before the right side is read.
    let (left, got) = run(Operator::AssignEqual, list(vec![int(1)]), int(5), None);
    assert_eq!(
        (left, got),
        (list(vec![]), Err(OpError::BadConversion(Accessor::ListRef)))
    );

    // Typed list by the table, refused by the class.
    assert_eq!(
        ops::result_type(Operator::Plus, TypeCode::List, TypeCode::List),
        Some(Kind::List)
    );
    assert_eq!(
        value(Operator::Plus, list(vec![]), list(vec![])),
        Err(OpError::Unsupported(Kind::List))
    );
}

/// `GplAgentRef`, and the throw.
#[test]
fn agents_compare_by_id_and_hand_out_attributes() {
    assert_eq!(
        value(Operator::Equal, Value::Agent(4), Value::Agent(4)),
        Ok(Some(yes(true)))
    );
    assert_eq!(
        value(Operator::Equal, Value::Agent(4), Value::Null),
        Err(OpError::BadConversion(Accessor::AgentRef))
    );

    let mut dest = None;
    let step = ops::apply(
        Operator::Apostrophe,
        &mut Value::Agent(4),
        Some(&text("gold")),
        &mut dest,
    );
    assert_eq!(
        step,
        Ok(Step::Attribute {
            agent: 4,
            name: "gold".into()
        })
    );
    assert_eq!(dest, None, "the table gives 's no type");

    assert_eq!(
        value(Operator::Plus, Value::Agent(1), Value::Agent(1)),
        Err(OpError::Unsupported(Kind::Agent))
    );
}

/// `GplBoolean`: `||` and `&&` read the right side
/// only when they must; `==` reads it by value.
#[test]
fn boolean_operators_read_the_right_side_lazily() {
    let (t, f) = (Value::Boolean(true), Value::Boolean(false));

    assert_eq!(judged(Operator::Or, t.clone(), int(5)), Ok(Some(yes(true))));
    assert_eq!(
        judged(Operator::Or, f.clone(), int(5)),
        Err(OpError::BadConversion(Accessor::BoolRef))
    );
    assert_eq!(
        judged(Operator::And, f.clone(), int(5)),
        Ok(Some(yes(false)))
    );
    assert_eq!(
        judged(Operator::Equal, t.clone(), int(1)),
        Err(OpError::BadConversion(Accessor::Bool))
    );
    assert_eq!(
        value(Operator::NotEqual, t.clone(), f.clone()),
        Ok(Some(yes(true)))
    );
    assert_eq!(
        value(Operator::Plus, t, f),
        Err(OpError::Unsupported(Kind::Boolean))
    );
}

/// `true && 5`: the table has no type for boolean and integer, so the slot
/// stays empty; `false && 5` then writes nowhere.
#[test]
fn an_untyped_result_slot_is_never_written() {
    assert_eq!(
        ops::result_type(Operator::And, TypeCode::Boolean, TypeCode::Integer),
        None
    );
    assert_eq!(
        value(Operator::And, Value::Boolean(false), int(5)),
        Err(OpError::NoResult)
    );
}

/// `GplCoord`, and the throw.
#[test]
fn coords_assign_and_compare_only() {
    assert_eq!(
        value(Operator::Equal, Value::Coord(1, 2), Value::Coord(1, 2)),
        Ok(Some(yes(true)))
    );
    assert_eq!(
        value(Operator::Equal, Value::Coord(1, 2), Value::Coord(1, 3)),
        Ok(Some(yes(false)))
    );
    assert_eq!(
        assigned(
            Operator::AssignEqual,
            Value::Coord(0, 0),
            Value::Coord(5, 6)
        ),
        Ok(Value::Coord(5, 6))
    );
    assert_eq!(
        value(Operator::Plus, Value::Coord(0, 0), Value::Coord(0, 0)),
        Err(OpError::Unsupported(Kind::Coord))
    );
}

/// `GplFunction`, and the throw.
#[test]
fn functions_assign_and_compare_only() {
    assert_eq!(
        assigned(
            Operator::AssignEqual,
            Value::Function(1),
            Value::Function(2)
        ),
        Ok(Value::Function(2))
    );
    assert_eq!(
        value(Operator::Equal, Value::Function(1), Value::Function(1)),
        Ok(Some(yes(true)))
    );
    assert_eq!(
        judged(Operator::Equal, Value::Function(1), int(1)),
        Err(OpError::BadConversion(Accessor::FunctionRef))
    );
}

/// `GplNull` returns at once: even `=` does nothing.
#[test]
fn a_null_left_operand_ignores_every_operator() {
    assert_eq!(
        assigned(Operator::AssignEqual, Value::Null, int(5)),
        Ok(Value::Null)
    );
}
