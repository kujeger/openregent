//! Values as text, the way the engine prints them: `GplType`'s text call, the
//! per-record trace line and the script error report.
//!
//! **Binary**, `MajestyHD.exe`. Slot 3 feeds `DebugOut`, the trace
//! and the call sequence both catchers
//! log, so one formatter serves all three.
//!
//! | Class | Text |
//! |---|---|
//! | `GplNull` | `()` |
//! | `GplInteger` | `_ltoa`, base 10 |
//! | `GplFloat` | `_gcvt` of the value as a 32-bit float, 10 digits |
//! | `GplString` | the string |
//! | `GplList` | `{ a, b, c }`; empty `{ }` |
//! | `GplAgentRef` | `nullagent` for id -1, else `name#id`, `agent(free)#id` without a name |
//! | `GplBoolean` | `true`, `false` |
//! | `GplCoord` | `x, y` |
//! | `GplFunction` | `$Name( args)`; an idle value `$Name( 0)`, the null record `$NullFunction( 0)`; a thread [`thread`] |

use std::borrow::Cow;

use super::{Thread, ThreadMode};
use crate::base::gfix::Gfix;
use crate::gpl::opcode::{self, Op};
use crate::gpl::ops::Value;

/// `GplNull`'s text.
pub const NULL_TEXT: &str = "()";

/// A reset agent's text.
pub const NULL_AGENT_TEXT: &str = "nullagent";

/// An agent the engine cannot name: its unit is gone or unnamed.
pub const FREE_AGENT_TEXT: &str = "agent(free)";

/// The null function record's own name: empty, from
/// gpl function record default. A call frame of it prints this; a
/// value prints [`NULL_RECORD_TEXT`]. The builtin registered under the
/// empty name prints `$( 0 )`, in the Freestyle trace.
pub const NULL_FUNCTION_TEXT: &str = "";

/// The null function record's name in `GplFunction`, which puts it in
/// place of the record's own.
pub const NULL_RECORD_TEXT: &str = "NullFunction";

/// An idle function value's frame: one integer return slot holding 0
/// (gpl frame reset, passed 0 by `GplFunction`). Every function
/// value in the Freestyle trace prints it: `$RetrieveAgent( 0 )`.
pub const IDLE_FRAME: &str = "0";

/// An empty handle in a call frame's argument list, `"(nullptr)"`.
pub const NULL_HANDLE_TEXT: &str = "(nullptr)";

/// Between list elements, frame arguments and coordinate halves: `", "`.
pub const SEPARATOR: &str = ", ";

/// Significant digits `GplFloat` asks `_gcvt` for.
pub const FLOAT_DIGITS: usize = 10;

/// A float's value: fixed point times `1 / 1024`, stored as a 32-bit float.
/// `GplFloat` multiplies by the constant 0.0009765625 and stores the
/// product in a `float` before widening it for `_gcvt`.
#[must_use]
#[allow(clippy::cast_possible_truncation, reason = "the engine stores a float")]
pub fn float_value(f: Gfix) -> f64 {
    f64::from(f.to_f64() as f32)
}

/// `_gcvt(value, 10)`, as MSVCR90 formats it.
///
/// **Inferred**: `_gcvt` is imported from MSVCR90, not in the executable;
/// this follows the Microsoft CRT's `gcvt.c`. Ten significant digits, rounded
/// half up on the exact decimal expansion; exponent form when the decimal
/// exponent is below -1 or above 9, three exponent digits; trailing zeros
/// dropped, the decimal point kept.
///
/// ```
/// use regent_core::base::gfix::Gfix;
/// use regent_core::gpl::vm::text::{gcvt, float_value};
///
/// assert_eq!(gcvt(float_value(Gfix(1536))), "1.5");
/// assert_eq!(gcvt(float_value(Gfix(2048))), "2.");
/// assert_eq!(gcvt(float_value(Gfix(-512))), "-0.5");
/// assert_eq!(gcvt(float_value(Gfix(1))), "9.765625e-004");
/// assert_eq!(gcvt(0.0), "0.");
/// ```
#[must_use]
pub fn gcvt(value: f64) -> String {
    let sign = if value.is_sign_negative() && value != 0.0 {
        "-"
    } else {
        ""
    };

    let (digits, decpt) = significant(value.abs(), FLOAT_DIGITS);
    let magnitude = decpt - 1;
    let top = i32::try_from(FLOAT_DIGITS).unwrap_or(i32::MAX) - 1;

    let body = if magnitude < -1 || magnitude > top {
        let (lead, rest) = digits.split_at(1);
        let exponent_sign = if magnitude < 0 { '-' } else { '+' };

        format!(
            "{lead}.{}e{exponent_sign}{:03}",
            trim_zeros(rest),
            magnitude.abs()
        )
    } else {
        // decpt is 0 to 10 here: the integer part is the first decpt digits.
        let split = usize::try_from(decpt).unwrap_or_default();
        let (int, frac) = digits.split_at(split);
        let int = if int.is_empty() { "0" } else { int };

        format!("{int}.{}", trim_zeros(frac))
    };

    format!("{sign}{body}")
}

/// Which C runtime formats a float: the engine's text depends on it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FloatText {
    /// Windows' MSVCR90: [`gcvt`].
    #[default]
    Msvcr90,
    /// Wine's MSVCR90, which the traced runs used: [`gcvt_wine`].
    Wine,
}

/// A float's text, under `crt`.
#[must_use]
pub fn float(raw: Gfix, crt: FloatText) -> String {
    match crt {
        FloatText::Msvcr90 => gcvt(float_value(raw)),
        FloatText::Wine => gcvt_wine(float_value(raw)),
    }
}

/// `_gcvt(value, 10)` as Wine formats it: `sprintf("%.10g")`. Exponent
/// form when the decimal exponent is below -4 or above 9; trailing zeros
/// dropped, and the decimal point with them.
///
/// **Game**: `20` and `0.7998046875` in the play-3 trace. **Inferred**: the
/// `%g` rule and three exponent digits, as Wine's `msvcrt` prints; no
/// traced float had an exponent. Rounding as [`gcvt`].
///
/// ```
/// use regent_core::base::gfix::Gfix;
/// use regent_core::gpl::vm::text::{gcvt_wine, float_value};
///
/// assert_eq!(gcvt_wine(float_value(Gfix(2048))), "2");
/// assert_eq!(gcvt_wine(float_value(Gfix(1))), "0.0009765625");
/// ```
#[must_use]
pub fn gcvt_wine(value: f64) -> String {
    let sign = if value.is_sign_negative() && value != 0.0 {
        "-"
    } else {
        ""
    };

    let (digits, decpt) = significant(value.abs(), FLOAT_DIGITS);
    let exponent = decpt - 1;
    let top = i32::try_from(FLOAT_DIGITS).unwrap_or(i32::MAX) - 1;
    let point = |int: &str, frac: &str| match trim_zeros(frac) {
        "" => int.to_owned(),
        f => format!("{int}.{f}"),
    };

    let body = if value == 0.0 {
        "0".to_owned()
    } else if exponent < WINE_LOWEST || exponent > top {
        let (lead, rest) = digits.split_at(1);
        let exponent_sign = if exponent < 0 { '-' } else { '+' };

        format!("{}e{exponent_sign}{:03}", point(lead, rest), exponent.abs())
    } else if decpt <= 0 {
        // -4 to -1: leading zeros after the point.
        let zeros = "0".repeat(usize::try_from(-decpt).unwrap_or_default());

        point("0", &format!("{zeros}{digits}"))
    } else {
        let (int, frac) = digits.split_at(usize::try_from(decpt).unwrap_or_default());

        point(int, frac)
    };

    format!("{sign}{body}")
}

/// `%g`'s lowest exponent before the exponent form.
const WINE_LOWEST: i32 = -4;

/// Drop trailing zeros; `_gcvt` keeps the decimal point before them.
fn trim_zeros(s: &str) -> &str {
    s.trim_end_matches('0')
}

/// `value`'s first `n` significant digits, rounded half up, and the decimal
/// point's position: value = 0.d1d2... × 10^decpt. Zero gives `n` zeros and
/// decpt 0.
fn significant(value: f64, n: usize) -> (String, i32) {
    if value == 0.0 {
        ("0".repeat(n), 0)
    } else {
        // Exact: a GPL float has at most ten fractional bits.
        let exact = format!("{value:.60e}");
        let (mantissa, exponent) = exact.split_once('e').unwrap_or((&exact, "0"));
        let exponent: i32 = exponent.parse().unwrap_or_default();
        let all: Vec<u8> = mantissa
            .bytes()
            .filter(u8::is_ascii_digit)
            .map(|b| b - b'0')
            .collect();

        let mut kept: Vec<u8> = all.iter().take(n).copied().collect();
        let round_up = all.get(n).is_some_and(|d| *d >= 5);
        let mut decpt = exponent + 1;

        if round_up {
            let carry = kept.iter_mut().rev().try_for_each(|d| {
                if *d == 9 {
                    *d = 0;
                    Ok(())
                } else {
                    *d += 1;
                    Err(())
                }
            });

            if carry.is_ok() {
                kept.insert(0, 1);
                kept.truncate(n);
                decpt += 1;
            }
        }

        (kept.iter().map(|d| char::from(b'0' + d)).collect(), decpt)
    }
}

/// An opcode's token as opcode name reads it with the token flag:
/// the table's for 0 to 38; for 39 to 42 the bytes past the table's end,
/// inside RTTI type names (opcode tokens past table on, stride 0x32).
///
/// **Binary**, read from the image.
pub const TOKENS_PAST_TABLE: [&str; 4] = ["ptedDataPacket@@", "", "acket@@", "?AVCNSystemPacket@@"];

/// The token the trace prints for `op`.
#[must_use]
pub fn token(op: Op) -> &'static str {
    let past = usize::from(op.code()).checked_sub(opcode::TABLE.len());

    match past {
        None => op.token(),
        Some(at) => TOKENS_PAST_TABLE.get(at).copied().unwrap_or_default(),
    }
}

/// What text a value needs from outside the VM: a function handle's name
/// and an agent's name.
pub trait Names {
    /// The function record's name; `None` for the null record.
    fn function(&self, handle: u32) -> Option<&str>;

    /// The agent's name, or `None` for [`FREE_AGENT_TEXT`].
    fn agent(&mut self, id: u32) -> Option<String>;

    /// Which runtime formats floats.
    fn float_text(&self) -> FloatText {
        FloatText::Msvcr90
    }
}

/// `GplType`'s text call.
///
/// ```
/// use regent_core::gpl::ops::Value;
/// use regent_core::gpl::vm::text::{self, Names};
///
/// struct Anon;
/// impl Names for Anon {
///     fn function(&self, _: u32) -> Option<&str> { None }
///     fn agent(&mut self, _: u32) -> Option<String> { None }
/// }
///
/// let list = Value::List(vec![Value::Integer(1), Value::Coord(2, -3), Value::Boolean(true)].into());
/// assert_eq!(text::value(&list, &mut Anon), "{ 1, 2, -3, true }");
/// assert_eq!(text::value(&Value::List(vec![].into()), &mut Anon), "{ }");
/// assert_eq!(text::value(&Value::Agent(7), &mut Anon), "agent(free)#7");
/// ```
pub fn value(v: &Value, names: &mut dyn Names) -> String {
    match v {
        Value::Null => NULL_TEXT.to_owned(),
        Value::Integer(i) => i.to_string(),
        Value::Float(f) => float(*f, names.float_text()),
        Value::String(s) => s.clone(),

        // "{ ", then each element followed by ", ", the last by " ", then "}".
        Value::List(items) => {
            let inner: Vec<String> = items.iter().map(|e| value(e, names)).collect();

            if inner.is_empty() {
                "{ }".to_owned()
            } else {
                format!("{{ {} }}", inner.join(SEPARATOR))
            }
        }

        Value::Agent(id) if *id == crate::gpl::ops::NULL_AGENT => NULL_AGENT_TEXT.to_owned(),
        Value::Agent(id) => {
            let name = names
                .agent(*id)
                .unwrap_or_else(|| FREE_AGENT_TEXT.to_owned());

            format!("{name}#{}", id.cast_signed())
        }

        Value::Boolean(b) => b.to_string(),
        Value::Coord(x, y) => format!("{x}{SEPARATOR}{y}"),
        Value::Function(f) => frame(
            Some(names.function(*f).unwrap_or(NULL_RECORD_TEXT)),
            &[IDLE_FRAME.to_owned()],
        ),
    }
}

/// `GplFunction` for a value with thread fields, `name` its function's
/// and `pending` its pending function's:
///
/// ```text
/// mode 0, not running   $%s( %s )                    name, frame
/// mode 0, running       $%s()                        name
/// else                  $%s(%s), run %s%dms %s %s    name, frame, "once @" or "@",
///                                                    interval, "(s)" if suspended,
///                                                    "(r=$%s)" if pending
/// ```
///
/// ```
/// use regent_core::gpl::runtime::ThreadMode;
/// use regent_core::gpl::vm::{text, Thread};
///
/// let t = Thread { frame: "(), Peasant#8".into(), interval: 500, mode: ThreadMode::Repeat,
///                  running: true, ..Thread::default() };
/// assert_eq!(text::thread("peasant_go_home", &t, Some("peasant_sleep")),
///            "$peasant_go_home((), Peasant#8), run @500ms  (r=$peasant_sleep)");
/// ```
#[must_use]
pub fn thread(name: &str, t: &Thread, pending: Option<&str>) -> String {
    let replaced = pending.map_or_else(String::new, |p| format!("(r=${p})"));
    let suspended = if t.suspended { "(s)" } else { "" };
    let mode = match t.mode {
        ThreadMode::Once => "once @",
        ThreadMode::Off | ThreadMode::Repeat => "@",
    };

    match (t.mode, t.running) {
        (ThreadMode::Off, false) => format!("${name}( {} )", t.frame),
        (ThreadMode::Off, true) => format!("${name}()"),
        _ => format!(
            "${name}({}), run {mode}{}ms {suspended} {replaced}",
            t.frame, t.interval
        ),
    }
}

/// A function value or call frame: `$%s( %s )`, the arguments joined by
/// [`SEPARATOR`], an empty handle as [`NULL_HANDLE_TEXT`].
///
/// A value here carries no frame, so it prints [`IDLE_FRAME`]. A
/// thread's arguments, mode and interval (`$%s(%s), run %s%dms %s %s`) are
/// the host's.
#[must_use]
pub fn frame(name: Option<&str>, args: &[String]) -> String {
    format!(
        "${}( {} )",
        name.unwrap_or(NULL_FUNCTION_TEXT),
        args.join(SEPARATOR)
    )
}

/// Gpl trace line's body: empty for `opNop`, else
/// `op: %s, lhs: %s, rhs: %s, result: %s`, an empty handle printed empty.
#[must_use]
pub fn trace_body(op: Op, lhs: &str, rhs: &str, result: &str) -> String {
    if op == Op::Nop {
        String::new()
    } else {
        format!(
            "op: {}, lhs: {lhs}, rhs: {rhs}, result: {result}",
            token(op)
        )
    }
}

/// One trace line as gpl execute logs it, without the
/// closing `\r\n`: a tab per gpl depth stack entry, then
/// `$%s(), L#%d: %s`. The run's own function is an entry, so its records
/// print one tab.
#[must_use]
pub fn trace_line(depth: usize, function: &str, line: i32, body: &str) -> String {
    let out = format!("{}${function}(), L#{line}: {body}", "\t".repeat(depth));

    // Most lines have no `%`: keep the string instead of copying it.
    if let Cow::Owned(again) = reformatted(&out) {
        again
    } else {
        out
    }
}

/// What a formatted line becomes when it is formatted again:
/// the debug log passes its `vsprintf` output to
/// the log writer as the format. **Game**: `%` prints `op: , lhs: 2`.
///
/// `%%` gives `%`; `%` before a character no conversion starts with gives
/// that character, as MSVC's `vsprintf` does. A real conversion reads an
/// argument that is not there: left as it is (**Inferred**: no trace has
/// one).
#[must_use]
pub fn reformatted(line: &str) -> Cow<'_, str> {
    if line.contains('%') {
        Cow::Owned(escaped(line))
    } else {
        Cow::Borrowed(line)
    }
}

/// [`reformatted`] for a line with a `%`.
fn escaped(line: &str) -> String {
    const STARTS: &str = "-+ #0123456789.*hlLwIcCdiouxXeEfgGaAnpsSZ";
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();

    while let Some(c) = chars.next() {
        match (c, chars.peek()) {
            ('%', Some('%')) => {
                chars.next();
                out.push('%');
            }
            ('%', Some(next)) if !STARTS.contains(*next) => {}
            _ => out.push(c),
        }
    }

    out
}

/// The heads of the two catchers' reports.
pub const THREAD_ERROR: &str = "Script error:\n Call Sequence was:";
pub const DISPATCH_ERROR: &str = "GplDispatcherHandle script error:\n Call Sequence was:";

/// A script error report, as both catchers build it: the head, then per
/// call frame, outermost first, `\r\n`, four spaces per depth and the frame,
/// then ` - line %d : %s`, the message with [`MESSAGE_END`] after it. No
/// line end: debug log writes the text as is, so the log's next line
/// follows on its last.
///
/// **Game**, the clock probe: `Script error:\n Call Sequence was:\r\n$Probe_Error(
/// (), GplAIRoot#1 ) - line 1731 : Divide by zero error..`, then a trace
/// line on the same line.
#[must_use]
pub fn report(head: &str, frames: &[String], line: i32, message: &str) -> String {
    let mut out = head.to_owned();

    for (depth, f) in frames.iter().enumerate() {
        out.push_str("\r\n");
        out.push_str(&"    ".repeat(depth));
        out.push_str(f);
    }

    format!("{out} - line {line} : {message}{MESSAGE_END}")
}

/// What gpl exception text puts after an exception's
/// message, by `sprintf_s` of the string at exception text end: a message ending
/// in a full stop ends in two.
pub const MESSAGE_END: &str = ".";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_float_prints_ten_significant_digits() {
        assert_eq!(gcvt(float_value(Gfix(1024 * 1000))), "1000.");
        assert_eq!(gcvt(float_value(Gfix(1024 * 2_000_000 + 1))), "2000000.");
        assert_eq!(gcvt(float_value(Gfix(103))), "0.1005859375");
        assert_eq!(gcvt(float_value(Gfix(102))), "9.9609375e-002");
    }

    /// **Game**: the play-3 trace, run under Wine, prints the literal
    /// `20.0` of `Autospawn_Lair` as `20`, and `0.7998046875` in full.
    #[test]
    fn wine_prints_an_integral_float_bare() {
        assert_eq!(float(Gfix(20 * 1024), FloatText::Wine), "20");
        assert_eq!(float(Gfix(819), FloatText::Wine), "0.7998046875");
        assert_eq!(float(Gfix(1536), FloatText::Wine), "1.5");
        assert_eq!(float(Gfix(0), FloatText::Wine), "0");
        assert_eq!(float(Gfix(1), FloatText::Wine), "0.0009765625");
        assert_eq!(float(Gfix(-512), FloatText::Wine), "-0.5");
        assert_eq!(float(Gfix(20 * 1024), FloatText::Msvcr90), "20.");
    }

    #[test]
    fn rounding_carries_into_a_new_digit() {
        assert_eq!(
            significant(9.999_999_999_9, 10),
            ("1000000000".to_owned(), 2)
        );
    }

    /// Names one function, handle 1.
    struct One;

    impl Names for One {
        fn function(&self, handle: u32) -> Option<&str> {
            (handle == 1).then_some("RetrieveAgent")
        }

        fn agent(&mut self, _: u32) -> Option<String> {
            None
        }
    }

    /// The Freestyle trace (`local/traces/freestyle_seed1_S_hd`) prints
    /// `$RetrieveAgent( 0)`. `GplFunction` names the null record
    /// `NullFunction`.
    #[test]
    fn a_function_value_prints_its_idle_frame() {
        assert_eq!(value(&Value::Function(1), &mut One), "$RetrieveAgent( 0 )");
        assert_eq!(
            value(&Value::Function(crate::gpl::ops::NULL_FUNCTION), &mut One),
            "$NullFunction( 0 )"
        );
    }

    #[test]
    fn the_trace_prints_tokens_past_the_table() {
        assert_eq!(token(Op::AssignEqual), "=");
        assert_eq!(token(Op::Body), "ptedDataPacket@@");
        assert_eq!(trace_body(Op::Nop, "a", "b", "c"), "");
        assert_eq!(
            trace_body(Op::Plus, "1", "2", "3"),
            "op: +, lhs: 1, rhs: 2, result: 3"
        );
    }

    #[test]
    fn a_trace_line_is_formatted_twice() {
        assert_eq!(
            trace_line(1, "f", 9, &trace_body(Op::Mod, "3", "2", "1")),
            "\t$f(), L#9: op: , lhs: 3, rhs: 2, result: 1"
        );
        assert_eq!(reformatted("100%%"), "100%");
    }
}
