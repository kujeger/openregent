//! The builtins the VM runs itself: those touching no world state.
//!
//! **Binary**, each from its native function in `MajestyHD.exe`:
//! the arguments it fetches, in
//! order, the accessor each goes through — so the same "bad conversion"
//! throws — and what it writes. Every other builtin goes to the
//! [`Host`](super::Host).

use super::{Fault, Handle, ObjId, Vm};
use crate::base::gfix::Gfix;
use crate::gpl::builtins::BuiltinId;
use crate::gpl::builtins::Native;
use crate::gpl::ops::{
    self, Accessor, NULL_AGENT, NULL_FUNCTION, OpError, Value, function_of, int_of, list_of,
};

/// `DebugOut` prints a line whose first argument is this integer even
/// without engine flags value 2, a compare with `0x38f` in gpl debug out.
pub const DEBUG_OUT_MARK: i32 = 911;

/// Whether the VM runs builtin `b` itself.
#[must_use]
pub const fn runs(b: BuiltinId) -> bool {
    b.get().native.is_some()
}

/// Fractional bits gpl sqrt passes fixed sqrt: a GPL float's.
pub const SQRT_FRACTION: u32 = Gfix::FRACTION_BITS;

/// Fixed sqrt: a bit-by-bit square root of an unsigned
/// fixed-point value with `fraction` bits, `fraction / 2 + 16` rounds of two
/// bits each. Floors.
///
/// ```
/// use regent_core::gpl::vm::native::fixed_sqrt;
///
/// assert_eq!(fixed_sqrt(4 << 10, 10), 2 << 10);
/// assert_eq!(fixed_sqrt(2 << 10, 10), 1448); // 1.4140625
/// ```
#[must_use]
pub const fn fixed_sqrt(value: u32, fraction: u32) -> u32 {
    let mut rest = value;
    let mut remainder: u32 = 0;
    let mut root: u32 = 0;
    let mut rounds = (fraction >> 1) + 15;

    loop {
        remainder = (remainder << 2) | (rest >> 30);
        rest <<= 2;
        root <<= 1;

        let trial = root.wrapping_mul(2).wrapping_add(1);

        if remainder >= trial {
            remainder -= trial;
            root += 1;
        }

        if rounds == 0 {
            break root;
        }

        rounds -= 1;
    }
}

/// Float to int, the CRT's `_ftol`: truncates; NaN and overflow give
/// the x87 integer indefinite.
#[must_use]
#[allow(clippy::cast_possible_truncation, reason = "the range is checked")]
pub fn ftol(x: f64) -> i32 {
    if x.is_nan() || x >= 2_147_483_648.0 || x < -2_147_483_648.0 {
        i32::MIN
    } else {
        x.trunc() as i32
    }
}

/// Run builtin `b`.
///
/// # Errors
///
/// What the native throws or faults on.
pub fn run(
    vm: &mut Vm<'_>,
    host: &mut dyn super::Host,
    b: BuiltinId,
    args: &[Handle],
) -> Result<(), Fault> {
    match b.get().native {
        None | Some(Native::Nothing) => Ok(()),

        // Bound through `int&`, then the result's `int&`, then the draw.
        Some(Native::RandomNumber) => {
            let bound = int_ref(vm, args, 1)?;
            let ret = vm.arg(args, 0)?;

            int_of(vm.object(ret))?;
            let drawn = vm.draw(bound);
            put_int(vm, ret, drawn)
        }

        // `float` by value, so an integer widens; the result's `float&`.
        Some(Native::Sqrt) => {
            let x = float_val(vm, args, 1)?;
            let ret = vm.arg(args, 0)?;
            let root = fixed_sqrt(x.cast_unsigned(), SQRT_FRACTION).cast_signed();

            match vm.object_mut(ret) {
                Value::Float(f) => {
                    f.0 = root;
                    Ok(())
                }
                _ => Err(OpError::BadConversion(Accessor::FloatRef).into()),
            }
        }

        // Argument 2 is fetched and read before argument 1; the coordinate
        // goes in through the result's slot 6.
        Some(Native::MakeCoord) => {
            let ret = vm.arg(args, 0)?;
            let y = int_ref(vm, args, 2)?;
            let x = int_ref(vm, args, 1)?;

            ops::assign(vm.object_mut(ret), &Value::Coord(x, y)).map_err(Fault::from)
        }

        Some(Native::GetX) => {
            let (x, _) = coord(vm, args, 1)?;
            let ret = vm.arg(args, 0)?;
            put_int(vm, ret, x)
        }

        Some(Native::GetY) => {
            let (_, y) = coord(vm, args, 1)?;
            let ret = vm.arg(args, 0)?;
            put_int(vm, ret, y)
        }

        // `(int) sqrt(dy * dy + dx * dx)`, the sum in 32 bits, then in floats
        // square root, `_ftol`.
        Some(Native::DistanceBetweenCoords) => {
            let (x1, y1) = coord(vm, args, 1)?;
            let (x2, y2) = coord(vm, args, 2)?;
            let dy = y2.wrapping_sub(y1);
            let dx = x2.wrapping_sub(x1);
            let sum = dy.wrapping_mul(dy).wrapping_add(dx.wrapping_mul(dx));
            let ret = vm.arg(args, 0)?;

            put_int(vm, ret, ftol(f64::from(sum).sqrt()))
        }

        Some(
            native @ (Native::ListSize
            | Native::ListMember
            | Native::RemoveListMember
            | Native::ClearList
            | Native::AddLists
            | Native::Concatenate),
        ) => lists(vm, host, native, args),

        Some(Native::NullAgent) => {
            let ret = vm.arg(args, 0)?;
            ops::assign(vm.object_mut(ret), &Value::Agent(NULL_AGENT)).map_err(Fault::from)
        }

        // Valid unless the null record.
        Some(Native::ValidFunction) => {
            let f = vm.arg(args, 1)?;
            let handle = function_of(vm.object(f))?;
            let ret = vm.arg(args, 0)?;

            match vm.object_mut(ret) {
                Value::Boolean(v) => {
                    *v = handle != NULL_FUNCTION;
                    Ok(())
                }
                _ => Err(OpError::BadConversion(Accessor::BoolRef).into()),
            }
        }

        // `string&`, then the result's `gplfunction&` set by name
        // The null record for an unknown name.
        Some(Native::LookupFunction) => {
            let s = vm.arg(args, 1)?;
            let name = match vm.object(s) {
                Value::String(name) => name.clone(),
                _ => Err(OpError::BadConversion(Accessor::StringRef))?,
            };
            let ret = vm.arg(args, 0)?;
            let handle = vm.function(&name);

            match vm.object_mut(ret) {
                Value::Function(f) => {
                    *f = handle;
                    Ok(())
                }
                _ => Err(OpError::BadConversion(Accessor::FunctionRef).into()),
            }
        }

        Some(Native::DebugOut) => debug_out(vm, host, args),
    }
}

/// The list builtins.
fn lists(
    vm: &mut Vm<'_>,
    host: &mut dyn super::Host,
    native: Native,
    args: &[Handle],
) -> Result<(), Fault> {
    match native {
        Native::ListSize => {
            let list = vm.arg(args, 1)?;
            let len = list_of(vm.object(list))?.len();
            let ret = vm.arg(args, 0)?;

            put_int(vm, ret, i32::try_from(len).unwrap_or(i32::MAX))
        }

        Native::ListMember => list_member(vm, args),
        Native::RemoveListMember => remove_list_member(vm, host, args),

        Native::ClearList => {
            let list = vm.arg(args, 1)?;

            match vm.object_mut(list) {
                Value::List(items) => {
                    items.clear();
                    Ok(())
                }
                _ => Err(OpError::BadConversion(Accessor::GplListRef).into()),
            }
        }

        // Each argument's elements, copied, onto the result.
        Native::AddLists => {
            let ret = vm.arg(args, 0)?;
            list_of(vm.object(ret))?;

            for i in 1..args.len() {
                let from = vm.arg(args, i)?;
                let items = list_of(vm.object(from))?.to_vec();

                push(vm, ret, items);
            }

            Ok(())
        }

        // Each argument, copied whole, onto the result.
        _ => {
            let ret = vm.arg(args, 0)?;

            for i in 1..args.len() {
                let from = vm.arg(args, i)?;
                let item = vm.object(from).clone();

                list_of(vm.object(ret))?;
                push(vm, ret, vec![item]);
            }

            Ok(())
        }
    }
}

/// `ListMember(list, n)`, `n` from 1. The return slot takes a fresh value of
/// the member's type unless it has it already, then the member through slot
/// 6.
fn list_member(vm: &mut Vm<'_>, args: &[Handle]) -> Result<(), Fault> {
    let list = vm.arg(args, 1)?;
    list_of(vm.object(list))?;
    let n = int_ref(vm, args, 2)?;
    let items = list_of(vm.object(list))?;
    let len = items.len();

    let member = usize::try_from(n)
        .ok()
        .filter(|n| (1..=len).contains(n))
        .map(|n| items[n - 1].clone())
        .ok_or_else(|| Fault::throw(LIST_MEMBER_RANGE))?;

    let slot = *args.first().ok_or(super::Crash::ArgPastEnd)?;

    if let Some(ret) = vm.get(slot) {
        if vm.object(ret).kind() == member.kind() {
            ops::reset(vm.object_mut(ret));
        } else {
            vm.release(ret);

            let fresh = vm.of_type(member.kind().type_code());
            vm.set(slot, fresh);
        }
    }

    let ret = vm
        .get(slot)
        .ok_or(super::Crash::NullValue(super::Null::Result))?;
    ops::assign(vm.object_mut(ret), &member).map_err(Fault::from)
}

/// `ListMember`'s throw, `"ListMember() tried to access an element # beyond its size"`.
pub const LIST_MEMBER_RANGE: &str = "ListMember() tried to access an element # beyond its size";

/// `RemoveListMember`'s throw, `"RemoveListMember( %s, %d ) specified an out-of-bounds index"`: the list's text and the index.
pub const REMOVE_RANGE: &str = "RemoveListMember( %s, %d ) specified an out-of-bounds index.";

fn remove_list_member(
    vm: &mut Vm<'_>,
    host: &mut dyn super::Host,
    args: &[Handle],
) -> Result<(), Fault> {
    let list = vm.arg(args, 1)?;
    list_of(vm.object(list))?;
    let n = int_ref(vm, args, 2)?;
    let len = list_of(vm.object(list))?.len();

    if let Some(n) = usize::try_from(n).ok().filter(|n| (1..=len).contains(n)) {
        if let Value::List(items) = vm.object_mut(list) {
            items.remove(n - 1);
        }

        Ok(())
    } else {
        let shown = vm.text(host, vm.object(list));

        Err(Fault::throw(
            REMOVE_RANGE
                .replacen("%s", &shown, 1)
                .replacen("%d", &n.to_string(), 1),
        ))
    }
}

/// Printed only with [`super::DebugOut::All`] or the integer mark first: each
/// argument's text and a space, one log line.
fn debug_out(vm: &mut Vm<'_>, host: &mut dyn super::Host, args: &[Handle]) -> Result<(), Fault> {
    let prints = match vm.config().debug_out {
        super::DebugOut::All => true,
        super::DebugOut::Marked => {
            let first = vm.arg(args, 1)?;
            matches!(vm.object(first), Value::Integer(DEBUG_OUT_MARK))
        }
    };

    if prints {
        let mut line = String::new();

        for i in 1..args.len() {
            let o = vm.arg(args, i)?;
            line.push_str(&vm.object_text(host, o));
            line.push(' ');
        }

        host.log(&line);
    }

    Ok(())
}

/// Append `items` to the list in `ret`.
fn push(vm: &mut Vm<'_>, ret: ObjId, items: Vec<Value>) {
    if let Value::List(list) = vm.object_mut(ret) {
        list.extend(items);
    }
}

/// Write `v` into object `o` through `int&`.
///
/// # Errors
///
/// [`OpError::BadConversion`] where `o` holds no integer.
pub fn put_int(vm: &mut Vm<'_>, o: ObjId, v: i32) -> Result<(), Fault> {
    match vm.object_mut(o) {
        Value::Integer(i) => {
            *i = v;
            Ok(())
        }
        _ => Err(OpError::BadConversion(Accessor::IntRef).into()),
    }
}

/// Argument `i` through `int&`.
///
/// # Errors
///
/// A missing argument, or [`OpError::BadConversion`] for one that holds
/// no integer.
pub fn int_ref(vm: &Vm<'_>, args: &[Handle], i: usize) -> Result<i32, Fault> {
    let o = vm.arg(args, i)?;

    Ok(int_of(vm.object(o))?)
}

/// Argument `i` through `float` by value: an integer widens.
fn float_val(vm: &Vm<'_>, args: &[Handle], i: usize) -> Result<i32, Fault> {
    let o = vm.arg(args, i)?;

    Ok(ops::float_val(vm.object(o))?)
}

/// Argument `i` through `gplcoord&`.
fn coord(vm: &Vm<'_>, args: &[Handle], i: usize) -> Result<(i32, i32), Fault> {
    let o = vm.arg(args, i)?;

    match vm.object(o) {
        Value::Coord(x, y) => Ok((*x, *y)),
        _ => Err(OpError::BadConversion(Accessor::GplCoordRef).into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The 18 registrations run here, by name.
    #[test]
    fn the_natives_are_the_builtins_touching_no_world() {
        let names: Vec<&str> = BuiltinId::all()
            .map(BuiltinId::get)
            .filter(|b| b.native.is_some())
            .map(|b| b.name)
            .collect();

        assert_eq!(
            names,
            [
                "",
                "AddLists",
                "ClearList",
                "Concatenate",
                "DebugOut",
                "DistanceBetweenCoords",
                "GetX",
                "GetY",
                "ListMember",
                "ListSize",
                "LookupFunction",
                "MakeCoord",
                "NullAgent",
                "NullFunction",
                "RandomNumber",
                "RemoveListMember",
                "Sqrt",
                "ValidFunction",
            ]
        );
    }

    #[test]
    fn the_square_root_floors() {
        assert_eq!(fixed_sqrt(0, 10), 0);
        assert_eq!(fixed_sqrt(1 << 10, 10), 1 << 10);
        assert_eq!(fixed_sqrt(9 << 10, 10), 3 << 10);
        assert_eq!(fixed_sqrt(1, 10), 32); // sqrt(1/1024) = 1/32
    }

    #[test]
    fn ftol_truncates() {
        assert_eq!(ftol(2.9), 2);
        assert_eq!(ftol(-2.9), -2);
        assert_eq!(ftol(f64::NAN), i32::MIN);
    }
}
