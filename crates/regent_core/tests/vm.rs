//! Step 2: the VM runs every function that calls no world builtin, matches
//! hand-worked cases from the `ops` and `runtime` tables, and runs the whole
//! corpus against a host that only logs.
//!
//! Needs `MAJESTY_HD` and `MAJESTY_WORKSHOP`.

mod common;

use common::read;
use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;

use regent_core::base::gfix::Gfix;
use regent_core::content::dataset::{Bytecode, Datasets, Install};
use regent_core::data::gpltype::{Kind, TypeCode};
use regent_core::gpl::emit;
use regent_core::gpl::exec::{Callee, Dest, Literal, Operand, Target};
use regent_core::gpl::load::{Entry, Program};
use regent_core::gpl::ops::{NULL_AGENT, NULL_FUNCTION, Value};
use regent_core::gpl::runtime::{Catcher, ThreadMode};
use regent_core::gpl::vm::{
    Call, Checks, Config, Crash, DebugOut, Fault, Host, Null, ObjId, Run, Shared, Thread, Vm, host,
    native,
};

use common::log_host::{LogHost, Logged};

// ---------------------------------------------------------------------------
// Loading

/// HD's own GPL images in load order, base and expansion composed.
fn hd_paths() -> Vec<PathBuf> {
    Install::new(&common::hd(), Bytecode::Install)
        .files(Datasets::Expansion)
        .expect("HD's files")
        .programs
}

/// HD's base game and expansion, loaded as the engine loads them.
fn hd_program() -> Program {
    Program::load(&hd_images()).expect("HD loads")
}

/// A program compiled from `src`.
fn compiled(src: &str) -> Program {
    let bytes = emit::compile_source(src, "test.gpl").expect("compiles");

    Program::load(&[bytes]).expect("loads")
}

fn definition(program: &Program, name: &str) -> u32 {
    match program.entry(name) {
        Some(Entry::Script(d)) => d,
        other => panic!("{name}: {other:?}"),
    }
}

#[test]
fn active_argument_is_visible_inside_a_builtin() {
    struct Capture {
        inner: LogHost,
        attacker: Option<Value>,
        shield_owner: Option<Value>,
    }

    impl Host for Capture {
        fn builtin(&mut self, vm: &mut Vm<'_>, _: &Call<'_>) -> Result<(), Fault> {
            if vm
                .caller()
                .is_some_and(|name| name.eq_ignore_ascii_case("damage"))
            {
                self.attacker = vm.active_arg("damage", "attacker").cloned();
                vm.dispatch(self, "Damage_Shield_Hit", &[Value::Agent(5)])
                    .map_err(Fault::Crash)?;
            } else {
                self.shield_owner = vm.active_arg("damage", "defender").cloned();
            }
            Ok(())
        }

        fn attribute(&mut self, vm: &mut Vm<'_>, agent: u32, name: &str) -> Result<ObjId, Fault> {
            self.inner.attribute(vm, agent, name)
        }

        fn named(&mut self, name: &str) -> Value {
            self.inner.named(name)
        }

        fn log(&mut self, text: &str) {
            self.inner.log(text);
        }
    }

    let program = compiled(
        "function damage(agent attacker, agent defender)\nbegin\n\t$Halt();\nend\n\
         function Damage_Shield_Hit(agent ThisAgent)\nbegin\n\t$Halt();\nend\n",
    );
    let mut vm = Vm::new(&program, RELEASE);
    let mut host = Capture {
        inner: LogHost::default(),
        attacker: None,
        shield_owner: None,
    };

    call(
        &mut vm,
        &mut host,
        "damage",
        &[Value::Agent(5), Value::Agent(6)],
    );

    assert_eq!(host.attacker, Some(Value::Agent(5)));
    assert_eq!(host.shield_owner, Some(Value::Agent(6)));
}

const RELEASE: Config = Config {
    checks: Checks::Release,
    debug_out: DebugOut::Marked,
    ..Config::RELEASE
};

const DEBUG: Config = Config {
    checks: Checks::Debug,
    debug_out: DebugOut::Marked,
    ..Config::RELEASE
};

/// Run `name` on `vm` as a thread would.
fn call(vm: &mut Vm<'_>, host: &mut dyn Host, name: &str, args: &[Value]) -> Run {
    let def = definition(vm.program(), name);

    vm.run(host, def, args, Catcher::Thread)
        .unwrap_or_else(|c| panic!("{name}: {c:?}"))
}

const fn int(i: i32) -> Value {
    Value::Integer(i)
}

/// A GPL float from its fixed-point word.
const fn fixed(raw: i32) -> Value {
    Value::Float(Gfix(raw))
}

fn list(items: &[Value]) -> Value {
    Value::List(items.to_vec().into())
}

fn message(run: &Run) -> Option<&str> {
    run.error.as_ref().map(|e| e.message.as_str())
}

// ---------------------------------------------------------------------------
// Hand-worked cases

/// Integer arithmetic is fixed point into a typed slot: `3 + 0.5` is a float
/// by the result-type table, floored into the integer; `-7 / 2` floors to -4;
/// `2 == 2.0` is false; `3000000 + 0` wraps.
#[test]
fn mixed_arithmetic_lands_in_the_slots_type() {
    let program = compiled(
        "function Mix() is float\n\
         declare\n\tinteger i;\n\tfloat f;\n\
         begin\n\ti = 3 + 0.5;\n\tf = 0.5;\n\treturn i + f;\nend\n\
         function Floor() is integer\nbegin\n\treturn -7 / 2;\nend\n\
         function Same() is boolean\nbegin\n\treturn 2 == 2.0;\nend\n\
         function Wrap() is integer\nbegin\n\treturn 3000000 + 0;\nend\n",
    );
    let mut vm = Vm::new(&program, RELEASE);
    let mut host = LogHost::default();

    assert_eq!(call(&mut vm, &mut host, "Mix", &[]).result, fixed(3584));
    assert_eq!(call(&mut vm, &mut host, "Floor", &[]).result, int(-4));
    assert_eq!(
        call(&mut vm, &mut host, "Same", &[]).result,
        Value::Boolean(false)
    );
    assert_eq!(
        call(&mut vm, &mut host, "Wrap", &[]).result,
        int(-1_194_304)
    );
}

/// Lists equal their prefixes, `[]` equals everything; strings compare
/// case-insensitively.
#[test]
fn lists_equal_prefixes_and_strings_ignore_case() {
    let program = compiled(
        "function Prefix(list a, list b) is boolean\nbegin\n\treturn a == b;\nend\n\
         function Gold() is boolean\nbegin\n\treturn \"Gold\" == \"GOLD\";\nend\n",
    );
    let mut vm = Vm::new(&program, RELEASE);
    let mut host = LogHost::default();
    let yes = Value::Boolean(true);

    let short = list(&[int(1)]);
    let long = list(&[int(1), int(2)]);

    assert_eq!(
        call(&mut vm, &mut host, "Prefix", &[long.clone(), short]).result,
        yes
    );
    assert_eq!(
        call(&mut vm, &mut host, "Prefix", &[list(&[]), long]).result,
        yes
    );
    assert_eq!(call(&mut vm, &mut host, "Gold", &[]).result, yes);
}

/// One activation per function: the inner call overwrites the outer call's
/// parameter, so the outer computes from the innermost value. Its return
/// still reaches its own caller, argument 0 of its own call: through the
/// rebound `__Return_value__` it would land in the inner call's result, and
/// the run would return 0.
#[test]
fn reentry_clobbers_the_running_calls_parameter() {
    let program = compiled(
        "function Clobber(integer n) is integer\nbegin\n\
         \tif (n > 0)\n\t\t$Clobber(n - 1);\n\treturn n + 10;\nend\n",
    );

    for config in [RELEASE, DEBUG] {
        let mut vm = Vm::new(&program, config);
        let mut host = LogHost::default();

        assert_eq!(
            call(&mut vm, &mut host, "Clobber", &[int(3)]).result,
            int(10)
        );
    }
}

/// A host that dispatches `name` with `args` from its first `left` builtin
/// calls, recording each dispatch's result.
struct Reentering {
    inner: LogHost,
    name: &'static str,
    args: Vec<Value>,
    left: u32,
    results: Vec<Value>,
}

impl Host for Reentering {
    fn builtin(&mut self, vm: &mut Vm<'_>, _: &Call<'_>) -> Result<(), Fault> {
        if self.left > 0 {
            self.left -= 1;

            let (name, args) = (self.name, self.args.clone());
            let run = vm.dispatch(self, name, &args).map_err(Fault::Crash)?;

            self.results.push(run.result);
        }

        Ok(())
    }

    fn attribute(&mut self, vm: &mut Vm<'_>, agent: u32, name: &str) -> Result<ObjId, Fault> {
        self.inner.attribute(vm, agent, name)
    }

    fn named(&mut self, name: &str) -> Value {
        self.inner.named(name)
    }

    fn log(&mut self, text: &str) {
        self.inner.log(text);
    }
}

/// Re-entry from a builtin, as an engine callback does. The inner call
/// resets the locals on entry and again on exit, and shares the outer call's
/// `foreach` cursor. Its slot 6 on the parameter frees the list's nodes:
/// on the last element the cursor is the sentinel, so the inner loop runs
/// nothing and frees the cursor, and the outer loop restarts. On an earlier
/// element the inner loop reads on through freed nodes, each value released
/// to 0.
#[test]
fn reentry_resets_locals_and_shares_the_cursor() {
    let program = compiled(
        "function Walk(list l, integer at) is integer\n\
         declare\n\tinteger n;\n\tinteger total;\n\
         begin\n\tforeach n in l do\n\t\tbegin\n\
         \t\t\ttotal = total + n;\n\t\t\tif (n == at)\n\t\t\t\t$Halt();\n\
         \t\tend\n\treturn total;\nend\n",
    );
    let def = definition(&program, "Walk");
    let walked = list(&[int(1), int(2), int(4)]);

    // Re-entered at 4. Inner: no iteration, 0. Outer: total reset by the
    // inner exit, then 1 + 2 + 4 again.
    let mut vm = Vm::new(&program, RELEASE);
    let mut host = Reentering {
        inner: LogHost::default(),
        name: "Walk",
        args: vec![walked.clone(), int(4)],
        left: 1,
        results: Vec::new(),
    };
    let run = call(&mut vm, &mut host, "Walk", &[walked.clone(), int(4)]);

    assert_eq!(host.results, [int(0)]);
    assert_eq!(run.result, int(7));

    // Re-entered at 1, the cursor on the node of 2. Inner: the freed 2 and
    // 4, read as 0. Outer: restarts on the new nodes, 1 + 2 + 4.
    let mut vm = Vm::new(&program, RELEASE);
    let mut host = Reentering {
        inner: LogHost::default(),
        name: "Walk",
        args: vec![walked.clone(), int(1)],
        left: 1,
        results: Vec::new(),
    };
    let run = vm.run(&mut host, def, &[walked, int(1)], Catcher::Thread);

    assert_eq!(host.results, [int(0)]);
    assert_eq!(run.map(|r| r.result), Ok(int(7)));
}

/// An error skips the exit walk: the local keeps its value. The next call
/// resets it on entry; a normal exit resets it again.
#[test]
fn an_error_leaves_a_local_set_and_entry_resets_it() {
    let program = compiled(
        "function Keep(boolean fail) is integer\n\
         declare\n\tinteger x;\n\tinteger peek;\n\
         begin\n\tpeek = x;\n\tx = 5;\n\tif (fail)\n\t\tx = x / 0;\n\treturn peek;\nend\n",
    );
    let def = definition(&program, "Keep");
    let mut vm = Vm::new(&program, RELEASE);
    let mut host = LogHost::default();

    let failed = call(&mut vm, &mut host, "Keep", &[Value::Boolean(true)]);

    assert_eq!(message(&failed), Some("Divide by zero error."));
    assert_eq!(vm.local(def, "x"), Some(&int(5)), "no exit walk");

    let clean = call(&mut vm, &mut host, "Keep", &[Value::Boolean(false)]);

    assert_eq!(clean.result, int(0), "reset on entry");
    assert_eq!(vm.local(def, "x"), Some(&int(0)), "reset on exit");
}

/// A host whose agents have one attribute, `script`: a function object with
/// thread fields, the same object on every read.
struct Threaded {
    inner: LogHost,
    thread: Thread,
    script: Option<ObjId>,
}

impl Host for Threaded {
    fn builtin(&mut self, vm: &mut Vm<'_>, call: &Call<'_>) -> Result<(), Fault> {
        self.inner.builtin(vm, call)
    }

    fn attribute(&mut self, vm: &mut Vm<'_>, _: u32, _: &str) -> Result<ObjId, Fault> {
        let o = self.script.unwrap_or_else(|| {
            let o = vm.alloc(Value::Function(vm.function("Walk")));

            vm.set_thread(o, Some(self.thread.clone()));
            o
        });

        self.script = Some(o);
        Ok(o)
    }

    fn named(&mut self, name: &str) -> Value {
        self.inner.named(name)
    }

    fn log(&mut self, text: &str) {
        self.inner.log(text);
    }
}

/// A function assigned to a scheduled or running thread goes to its pending
/// slot; an idle one compares by it.
/// **Game**: the day-1 trace's `has_arrived` prints `$henchmen_travel_to((),
/// Tax_Collector#10), run @500ms  (r=$bring_home_goods)` after such an
/// assignment.
#[test]
fn an_assignment_to_a_busy_thread_is_pending() {
    let program = compiled(
        "function Walk() is integer\nbegin\n\treturn 1;\nend\n\
         function Rest() is integer\nbegin\n\treturn 2;\nend\n\
         function Swap(agent a) is boolean\nbegin\n\
         \ta's \"script\" = $LookupFunction(\"Rest\");\n\
         \treturn a's \"script\" == $LookupFunction(\"Rest\");\nend\n",
    );
    let mut vm = Vm::new(&program, RELEASE);
    let mut host = Threaded {
        inner: LogHost::default(),
        thread: Thread {
            frame: "(), Peasant#8".into(),
            interval: 500,
            mode: ThreadMode::Repeat,
            ..Thread::default()
        },
        script: None,
    };

    let run = call(&mut vm, &mut host, "Swap", &[Value::Agent(8)]);
    let script = host.script.expect("read");

    assert_eq!(run.result, Value::Boolean(true));
    assert_eq!(*vm.object(script), Value::Function(vm.function("Walk")));
    assert_eq!(
        vm.thread(script).and_then(|t| t.pending),
        Some(vm.function("Rest"))
    );
    assert_eq!(
        vm.object_text(&mut host, script),
        "$Walk((), Peasant#8), run @500ms  (r=$Rest)"
    );
}

/// An assignment that throws leaves a busy thread's pending function as it
/// was: the right value is read through `GplFunction&` before the write.
/// **Inferred** from that order; `GplFunction` not read.
#[test]
fn a_throwing_assignment_keeps_the_pending_function() {
    let program = compiled(
        "function Walk() is integer\nbegin\n\treturn 1;\nend\n\
         function Rest() is integer\nbegin\n\treturn 2;\nend\n\
         function Swap(agent a) is boolean\nbegin\n\
         \ta's \"script\" = $LookupFunction(\"Rest\");\n\
         \ta's \"script\" = 5;\n\
         \treturn true;\nend\n",
    );
    let mut vm = Vm::new(&program, RELEASE);
    let mut host = Threaded {
        inner: LogHost::default(),
        thread: Thread {
            interval: 500,
            mode: ThreadMode::Repeat,
            ..Thread::default()
        },
        script: None,
    };

    let run = call(&mut vm, &mut host, "Swap", &[Value::Agent(8)]);
    let script = host.script.expect("read");

    assert!(run.error.is_some());
    assert_eq!(
        vm.thread(script).and_then(|t| t.pending),
        Some(vm.function("Rest"))
    );
}

/// A save holds function values as handles, by the program's definition
/// order: another program refuses it, even one of the same shape.
#[test]
fn a_save_refuses_another_program() {
    let walk = compiled("function Walk() is integer\nbegin\n\treturn 1;\nend\n");
    let rest = compiled("function Rest() is integer\nbegin\n\treturn 1;\nend\n");
    let saved = Vm::new(&walk, RELEASE).saved();

    assert!(Vm::new(&walk, RELEASE).restore(saved.clone()).is_ok());
    assert!(Vm::new(&rest, RELEASE).restore(saved).is_err());
}

/// A save of the right program but the wrong shape is refused, not
/// restored to panic on the next run: an activation of too few handles,
/// a handle or a top slot past the objects.
#[test]
fn a_save_of_another_shape_is_refused() {
    let walk = compiled("function Walk() is integer\nbegin\n\treturn 1;\nend\n");
    let mut vm = Vm::new(&walk, RELEASE);

    call(&mut vm, &mut LogHost::default(), "Walk", &[]);

    let saved = vm.saved();
    let past = u32::try_from(saved.objects.len()).expect("few objects");
    let def = *saved.activations.keys().next().expect("Walk's");
    let mut short = saved.clone();
    let mut dangling = saved.clone();
    let mut top = saved.clone();

    short
        .activations
        .get_mut(&def)
        .expect("Walk's")
        .handles
        .pop();
    dangling.activations.get_mut(&def).expect("Walk's").handles[0] = Some(past);
    top.top.push(Some(past));

    assert!(Vm::new(&walk, RELEASE).restore(saved).is_ok());
    assert!(Vm::new(&walk, RELEASE).restore(short).is_err());
    assert!(Vm::new(&walk, RELEASE).restore(dangling).is_err());
    assert!(Vm::new(&walk, RELEASE).restore(top).is_err());
}

/// A string local keeps its text through the resets: `GplString`
/// only reserves 0 bytes. **Game**: the play-3 trace's
/// `TryTravelSpell` declares `spellname` as `NOTHING`, the last call's.
#[test]
fn a_reset_string_keeps_its_text() {
    let program = compiled(
        "function Name(string s) is string\n\
         declare\n\tstring kept;\n\tstring peek;\n\
         begin\n\tpeek = kept;\n\tkept = s;\n\treturn peek;\nend\n",
    );
    let def = definition(&program, "Name");
    let mut vm = Vm::new(&program, RELEASE);
    let mut host = LogHost::default();

    call(
        &mut vm,
        &mut host,
        "Name",
        &[Value::String("NOTHING".into())],
    );

    assert_eq!(
        vm.local(def, "kept"),
        Some(&Value::String("NOTHING".into()))
    );

    let second = call(&mut vm, &mut host, "Name", &[Value::String("x".into())]);

    assert_eq!(second.result, Value::String("NOTHING".into()));
}

/// A temporary is typed on its first run and keeps the type: the sum of a
/// list member and 1 is typed integer by the first call, so a float member
/// later cannot write it.
#[test]
fn a_temporary_keeps_the_type_of_its_first_run() {
    let program = compiled(
        "function First(list l) is integer\nbegin\n\treturn $ListMember(l, 1) + 1;\nend\n",
    );
    let mut vm = Vm::new(&program, RELEASE);
    let mut host = LogHost::default();

    assert_eq!(
        call(&mut vm, &mut host, "First", &[list(&[int(2)])]).result,
        int(3)
    );

    let later = call(&mut vm, &mut host, "First", &[list(&[fixed(1536)])]);

    assert_eq!(message(&later), Some("bad conversion to float&"));
}

#[test]
fn foreach_walks_the_list() {
    let program = compiled(
        "function Sum(list l) is integer\ndeclare\n\tinteger n;\n\tinteger total;\n\
         begin\n\tforeach n in l do\n\t\ttotal = total + n;\n\treturn total;\nend\n",
    );
    let mut vm = Vm::new(&program, RELEASE);
    let mut host = LogHost::default();
    let l = list(&[int(1), int(2), int(4)]);

    assert_eq!(
        call(&mut vm, &mut host, "Sum", std::slice::from_ref(&l)).result,
        int(7)
    );
    assert_eq!(call(&mut vm, &mut host, "Sum", &[l]).result, int(7));
    assert_eq!(call(&mut vm, &mut host, "Sum", &[list(&[])]).result, int(0));
}

/// `break` leaves its loop with the cursor at the end, not freed, so the next
/// pass over that loop finds it there and runs no iteration; the one after
/// runs again. Gpl execute writes the end; gpl execute frees it.
#[test]
fn break_leaves_the_cursor_at_the_end() {
    let program = compiled(
        "function FirstOver(list l, integer limit) is integer\n\
         declare\n\tinteger n;\n\tinteger found;\n\
         begin\n\tfound = -1;\n\tforeach n in l do\n\t\tbegin\n\
         \t\t\tif (n > limit)\n\t\t\t\tbegin\n\t\t\t\t\tfound = n;\n\t\t\t\t\tbreak;\n\t\t\t\tend\n\
         \t\tend\n\treturn found;\nend\n",
    );
    let mut vm = Vm::new(&program, RELEASE);
    let mut host = LogHost::default();
    let args = [list(&[int(1), int(5), int(9)]), int(3)];

    let found: Vec<Value> = (0..3)
        .map(|_| call(&mut vm, &mut host, "FirstOver", &args).result)
        .collect();

    assert_eq!(found, [int(5), int(-1), int(5)]);
}

/// `continue` steps onto the record after its `opForeach`, the jump out of
/// the loop: it leaves the loop, and the cursor stays on its
/// node, so the next pass resumes after the element. A new call assigns the
/// parameter by slot 6, which frees that node: the first pass reads it,
/// 3 released to 0.
#[test]
fn continue_leaves_the_loop_and_the_cursor_resumes() {
    let program = compiled(
        "function Count(list l) is integer\ndeclare\n\tinteger n;\n\tinteger c;\n\tinteger k;\n\
         begin\n\twhile (k < 3) do\n\t\tbegin\n\t\t\tk++;\n\
         \t\t\tforeach n in l do\n\t\t\t\tbegin\n\t\t\t\t\tif (n == 2)\n\t\t\t\t\t\tcontinue;\n\
         \t\t\t\t\tc = c + n;\n\t\t\t\tend\n\t\tend\n\treturn c;\nend\n",
    );
    let def = definition(&program, "Count");
    let mut vm = Vm::new(&program, RELEASE);
    let mut host = LogHost::default();
    let args = [list(&[int(1), int(2), int(3)])];

    assert_eq!(call(&mut vm, &mut host, "Count", &args).result, int(5));
    assert_eq!(
        vm.run(&mut host, def, &args, Catcher::Thread)
            .map(|r| r.result),
        Ok(int(4))
    );
}

/// `return` inside a `while` jumps to record 2; the loop steps to record 3,
/// the jump to the end, and the exit walk resets the counter.
#[test]
fn return_inside_a_loop_ends_the_function() {
    let program = compiled(
        "function Find(integer limit) is integer\ndeclare\n\tinteger i;\n\
         begin\n\twhile (i < 10) do\n\t\tbegin\n\t\t\tif (i == limit)\n\t\t\t\treturn i * 10;\n\
         \t\t\ti++;\n\t\tend\n\treturn -1;\nend\n",
    );
    let def = definition(&program, "Find");
    let mut vm = Vm::new(&program, RELEASE);
    let mut host = LogHost::default();

    assert_eq!(call(&mut vm, &mut host, "Find", &[int(3)]).result, int(30));
    assert_eq!(vm.local(def, "i"), Some(&int(0)));
    assert_eq!(call(&mut vm, &mut host, "Find", &[int(3)]).result, int(30));
    assert_eq!(call(&mut vm, &mut host, "Find", &[int(12)]).result, int(-1));
}

/// A function with no `is` returns the shared null; an operator on a null
/// does nothing and leaves its result slot empty, so `if` throws. The name
/// is filled only under the debug flag.
#[test]
fn an_if_on_an_empty_slot_throws() {
    let program = compiled(
        "function Nothing()\nbegin\nend\n\
         function Test() is integer\nbegin\n\tif ($Nothing() == 5)\n\t\treturn 1;\n\treturn 2;\nend\n",
    );
    let mut host = LogHost::default();

    let mut release = Vm::new(&program, RELEASE);
    let run = call(&mut release, &mut host, "Test", &[]);

    assert_eq!(
        message(&run),
        Some("If statement evaluates to a NULL value in function $()")
    );

    let mut debug = Vm::new(&program, DEBUG);
    let run = call(&mut debug, &mut host, "Test", &[]);

    assert_eq!(
        message(&run),
        Some("If statement evaluates to a NULL value in function $Test()")
    );
}

/// A float never becomes text in a script: `GplString` reads its right
/// operand through `string&`, which `GplFloat` leaves throwing. So the
/// runtime's `_gcvt` shows only in debug output.
#[test]
fn a_float_never_becomes_a_string() {
    let program = compiled(
        "function Assign() is string\ndeclare\n\tstring s;\n\tfloat f;\nbegin\n\tf = 2.5;\n\ts = f;\n\treturn s;\nend\n\
         function Join() is string\ndeclare\n\tstring s;\n\tfloat f;\nbegin\n\tf = 2.5;\n\ts = \"x\" + f;\n\treturn s;\nend\n",
    );
    let mut host = LogHost::default();
    let mut vm = Vm::new(&program, RELEASE);

    for name in ["Assign", "Join"] {
        assert_eq!(
            message(&call(&mut vm, &mut host, name, &[])),
            Some("bad conversion to string&"),
            "{name}"
        );
    }
}

/// Release play ignores an extra argument and converts a mistyped one; the
/// debug flag throws on both. A function with no parameters runs no
/// `opLocalize`, so it takes extras even then: `Enemy_Actions` passes
/// `Check_Heal` one.
#[test]
fn debug_checks_argument_counts_and_types() {
    let program = compiled(
        "function Takes(integer a) is integer\nbegin\n\treturn a;\nend\n\
         function None() is integer\nbegin\n\treturn 7;\nend\n\
         function Extra() is integer\nbegin\n\treturn $Takes(1, 2);\nend\n\
         function Mistyped() is integer\nbegin\n\treturn $Takes(2.5);\nend\n\
         function ToNone() is integer\nbegin\n\treturn $None(1);\nend\n",
    );
    let mut host = LogHost::default();

    let mut release = Vm::new(&program, RELEASE);

    assert_eq!(call(&mut release, &mut host, "Extra", &[]).result, int(1));
    assert_eq!(
        call(&mut release, &mut host, "Mistyped", &[]).result,
        int(2)
    );
    assert_eq!(call(&mut release, &mut host, "ToNone", &[]).result, int(7));

    let mut debug = Vm::new(&program, DEBUG);

    assert_eq!(
        message(&call(&mut debug, &mut host, "Extra", &[])),
        Some("Function $Takes() was passed too many arguments")
    );

    // `%s` is the text of the `opLocalize` record's first value, which
    // holds only the parameter's type: always empty.
    assert_eq!(
        message(&call(&mut debug, &mut host, "Mistyped", &[])),
        Some("Parameter type mismatch: \"\"")
    );
    assert_eq!(call(&mut debug, &mut host, "ToNone", &[]).result, int(7));
}

/// Too few arguments read past the vector: undefined in the engine, since the
/// invalid-parameter handler returns. In both modes: the debug type check
/// fetches the argument before the count check can run.
#[test]
fn too_few_arguments_read_past_the_vector() {
    let program = compiled(
        "function Takes(integer a) is integer\nbegin\n\treturn a;\nend\n\
         function Short() is integer\nbegin\n\treturn $Takes();\nend\n",
    );
    let def = definition(&program, "Short");

    for config in [RELEASE, DEBUG] {
        let mut vm = Vm::new(&program, config);

        assert_eq!(
            vm.run(&mut LogHost::default(), def, &[], Catcher::Thread),
            Err(Crash::ArgPastEnd),
            "{config:?}"
        );
    }
}

/// A function handle a host makes past the definitions names nothing: the
/// engine's handles are pointers, so this is the VM's own case. Called, its
/// return type has no class; run, it is not callable.
#[test]
fn a_handle_past_the_definitions_crashes() {
    let program = compiled("function Via(function f) is integer\nbegin\n\treturn (f)();\nend\n");
    let past = u32::try_from(program.definitions().len()).expect("few definitions");
    let mut vm = Vm::new(&program, RELEASE);
    let via = definition(&program, "Via");
    let handle = Value::Function(Vm::definition_handle(past));

    assert_eq!(
        vm.run(&mut LogHost::default(), via, &[handle], Catcher::Thread),
        Err(Crash::NullValue(Null::NoClass))
    );
    assert_eq!(
        vm.run(&mut LogHost::default(), past, &[], Catcher::Thread),
        Err(Crash::NotCallable)
    );
}

/// A constant is registered with return type 10, undefined: its scope's
/// `unknowntype` through gpl pool index. Called through
/// a function value, that type has no class. Once an earlier callee has typed
/// the record's result, the call reaches gpl function invoke, which
/// calls the constant's native function: 0.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn calling_a_constant_through_a_function_value_crashes() {
    let program = compiled(
        "expression #K 5\n\
         function Five() is integer\nbegin\n\treturn 5;\nend\n\
         function Via(function f) is integer\nbegin\n\treturn (f)();\nend\n",
    );
    let via = definition(&program, "Via");
    let mut vm = Vm::new(&program, RELEASE);
    let constant = Value::Function(vm.function("#K"));
    let five = Value::Function(vm.function("Five"));

    assert_eq!(
        vm.run(
            &mut LogHost::default(),
            via,
            std::slice::from_ref(&constant),
            Catcher::Thread
        ),
        Err(Crash::NullValue(Null::NoClass))
    );

    let mut vm = Vm::new(&program, RELEASE);

    assert_eq!(
        vm.run(&mut LogHost::default(), via, &[five], Catcher::Thread)
            .map(|r| r.result),
        Ok(Value::Integer(5))
    );
    assert_eq!(
        vm.run(&mut LogHost::default(), via, &[constant], Catcher::Thread),
        Err(Crash::NotCallable)
    );

    // No HD function value names a constant.
    let hd = hd_program();
    let names_constant = |o: Operand| {
        matches!(o, Operand::Value(Literal::Function(s))
            if matches!(hd.entry(hd.names().get(s)), Some(Entry::Constant(_))))
    };
    let dest_value = |d: Dest| match d {
        Dest::Value(lit) => Operand::Value(lit),
        Dest::None | Dest::Alias => Operand::None,
    };

    assert!(!hd.definitions().iter().flat_map(|d| &d.code).any(|i| {
        [i.lhs, i.rhs, dest_value(i.dest)]
            .into_iter()
            .any(names_constant)
    }));
}

/// `x /= 0.5` passes the zero check on the fixed value, then divides by it
/// floored: the CPU traps.
#[test]
fn an_integer_divide_by_half_traps() {
    let program = compiled(
        "function Half() is integer\ndeclare\n\tinteger x;\nbegin\n\tx = 4;\n\tx /= 0.5;\n\treturn x;\nend\n",
    );
    let mut vm = Vm::new(&program, RELEASE);
    let def = definition(&program, "Half");

    assert_eq!(
        vm.run(&mut LogHost::default(), def, &[], Catcher::Thread),
        Err(Crash::DivideFault)
    );
}

/// The catcher logs the call sequence from the debugger's stack, which only
/// the debug flag fills: every frame, the run's own first, each with its
/// return slot first. Release play prints none and never writes the error
/// line.
#[test]
fn a_script_error_reports_the_call_sequence() {
    let program = compiled(
        "function Inner(integer n) is integer\nbegin\n\treturn n / 0;\nend\n\
         function Outer() is integer\nbegin\n\treturn $Inner(5);\nend\n",
    );
    let mut vm = Vm::new(&program, RELEASE);
    let mut host = LogHost::default();
    let run = call(&mut vm, &mut host, "Outer", &[]);
    let expected = "Script error:\n Call Sequence was: - line 0 : Divide by zero error..";

    assert_eq!(run.report.as_deref(), Some(expected));
    assert_eq!(host.log, [expected]);

    // Under the debug flag, every frame, and the throwing record's line.
    let mut debug = Vm::new(&program, DEBUG);
    let run = call(&mut debug, &mut host, "Outer", &[]);

    assert_eq!(
        run.report.as_deref(),
        Some(
            "Script error:\n Call Sequence was:\r\n$Outer( 0 )\r\n    $Inner( 0, 5 ) - line 3 : Divide by zero error.."
        )
    );
}

/// A run that crashes names where: each function on the GPL stack,
/// innermost first, with its source line, in release play too.
#[test]
fn an_unending_run_names_its_stack() {
    let program = compiled(
        "function Spin() is integer\ndeclare\n\tinteger i;\nbegin\n\
         \twhile (i < 1) do\n\t\tbegin\n\t\t\ti = 0;\n\t\tend\n\treturn i;\nend\n\
         function Outer() is integer\nbegin\n\treturn $Spin();\nend\n",
    );
    let mut vm = Vm::new(&program, RELEASE);
    let mut host = LogHost::default();
    let def = definition(&program, "Outer");

    vm.set_limit(Some(10_000));

    let crash = vm.run(&mut host, def, &[], Catcher::Thread).err();
    let stack: Vec<(&str, i32)> = vm.unwound().iter().map(|s| (s.function, s.line)).collect();

    assert_eq!(crash, Some(Crash::Unending));
    assert_eq!(stack.len(), 2, "{stack:?}");
    assert_eq!(stack[0].0, "Spin");
    assert!(matches!(stack[0].1, 5..=7), "{stack:?}");
    assert_eq!(stack[1], ("Outer", 13));
}

/// The trace prints each executed record: a tab per call on the depth
/// stack, the run's own included, the last source line seen, the opcode's
/// token and the operands' text.
#[test]
fn the_trace_prints_the_engines_line() {
    let program = compiled("function Add(integer a) is integer\nbegin\n\treturn a + 1;\nend\n");
    let lines = Shared::new(Vec::new());
    let sink = lines.clone();
    let mut vm = Vm::new(&program, RELEASE);

    vm.set_trace(Some(Box::new(move |line: &str| {
        sink.borrow_mut().push(line.to_owned());
    })));

    let run = call(&mut vm, &mut LogHost::default(), "Add", &[int(4)]);

    assert_eq!(run.result, int(5));

    let lines = lines.borrow();

    assert_eq!(
        lines.len() as u64,
        vm.count(),
        "one line per executed record"
    );
    assert!(
        lines.contains(&"\t$Add(), L#3: op: +, lhs: 4, rhs: 1, result: 5".to_owned()),
        "{lines:#?}"
    );
}

/// A dispatch's return slot starts as an integer 0, a thread's empty. The
/// Freestyle trace prints `declare return, lhs: (), rhs: , result: 0` for
/// untyped dispatches (`NewUnitInit`, `Freestyle`) and `result: ()` for
/// untyped threads (`Tower_Birth`, `GplKickoff`).
#[test]
fn a_dispatch_returns_into_an_integer_slot() {
    let program = compiled("function Plain()\nbegin\nend\n");
    let def = definition(&program, "Plain");

    for (catcher, text, result) in [
        (Catcher::Dispatch, "0", int(0)),
        (Catcher::Thread, "()", Value::Null),
    ] {
        let lines = Shared::new(Vec::new());
        let sink = lines.clone();
        let mut vm = Vm::new(&program, RELEASE);

        vm.set_trace(Some(Box::new(move |line: &str| {
            sink.borrow_mut().push(line.to_owned());
        })));

        let run = vm
            .run(&mut LogHost::default(), def, &[], catcher)
            .expect("no crash");

        assert_eq!(run.result, result, "{catcher:?}");
        assert_eq!(
            lines.borrow()[1],
            format!("\t$Plain(), L#1: op: declare return, lhs: (), rhs: , result: {text}"),
            "{catcher:?}"
        );
    }
}

/// A dispatch's or thread's own function is on the depth stack: its records
/// print one tab, a call from it two.
#[test]
fn the_trace_indents_the_runs_own_function_once() {
    let program = compiled(
        "function Inner(integer n) is integer\nbegin\n\treturn n + 1;\nend\n\
         function Outer() is integer\nbegin\n\treturn $Inner(4);\nend\n",
    );
    let lines = Shared::new(Vec::new());
    let sink = lines.clone();
    let mut vm = Vm::new(&program, RELEASE);

    vm.set_trace(Some(Box::new(move |line: &str| {
        sink.borrow_mut().push(line.to_owned());
    })));

    call(&mut vm, &mut LogHost::default(), "Outer", &[]);

    let lines = lines.borrow();

    assert!(
        lines.contains(&"\t\t$Inner(), L#3: op: +, lhs: 4, rhs: 1, result: 5".to_owned()),
        "{lines:#?}"
    );
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("\t$Outer(), L#7: op: call,")),
        "{lines:#?}"
    );
}

/// The first `CalculateDifficulty` dispatch in a real trace:
/// `local/traces/freestyle_seed1_S_hd/gpl.log` lines 8 to 46, a new
/// Freestyle game in `MajestyHD.exe` with the SDK's images.
const CALCULATE_DIFFICULTY: [&str; 39] = [
    "\t$CalculateDifficulty(), L#157: op: , lhs: , rhs: , result: ",
    "\t$CalculateDifficulty(), L#157: op: declare return, lhs: 0, rhs: , result: 0",
    "\t$CalculateDifficulty(), L#157: op: , lhs: 30000, rhs: , result: 30000",
    "\t$CalculateDifficulty(), L#157: op: , lhs: 15, rhs: , result: 15",
    "\t$CalculateDifficulty(), L#157: op: , lhs: 20, rhs: , result: 20",
    "\t$CalculateDifficulty(), L#157: op: , lhs: 50, rhs: , result: 50",
    "\t$CalculateDifficulty(), L#157: op: , lhs: 0, rhs: , result: 0",
    "\t$CalculateDifficulty(), L#157: op: , lhs: 0, rhs: , result: 0",
    "\t$CalculateDifficulty(), L#159: op: declare, lhs: 0, rhs: , result: 0",
    "\t$CalculateDifficulty(), L#159: op: declare, lhs: 0, rhs: , result: 0",
    "\t$CalculateDifficulty(), L#159: op: declare, lhs: 0, rhs: , result: 0",
    "\t$CalculateDifficulty(), L#159: op: ptedDataPacket@@, lhs: , rhs: , result: ",
    "\t$CalculateDifficulty(), L#168: op: *, lhs: 20, rhs: 3, result: 60",
    "\t$CalculateDifficulty(), L#168: op: +, lhs: 15, rhs: 60, result: 75",
    "\t$CalculateDifficulty(), L#168: op: *, lhs: 50, rhs: 2, result: 100",
    "\t$CalculateDifficulty(), L#168: op: +, lhs: 75, rhs: 100, result: 175",
    "\t$CalculateDifficulty(), L#168: op: /, lhs: 175, rhs: 6, result: 29",
    "\t$CalculateDifficulty(), L#168: op: =, lhs: 29, rhs: 29, result: 29",
    "\t$CalculateDifficulty(), L#170: op: /, lhs: 30000, rhs: 1000, result: 30",
    "\t$CalculateDifficulty(), L#170: op: -, lhs: 30, rhs: 20, result: 10",
    "\t$CalculateDifficulty(), L#170: op: =, lhs: 10, rhs: 10, result: 10",
    "\t$CalculateDifficulty(), L#179: op: >, lhs: 0, rhs: 29, result: false",
    "\t$CalculateDifficulty(), L#179: op: if, lhs: false, rhs: , result: ",
    "\t$CalculateDifficulty(), L#183: op: >, lhs: 0, rhs: 29, result: false",
    "\t$CalculateDifficulty(), L#183: op: if, lhs: false, rhs: , result: ",
    "\t$CalculateDifficulty(), L#186: op: ==, lhs: 0, rhs: 0, result: true",
    "\t$CalculateDifficulty(), L#186: op: if, lhs: true, rhs: , result: ",
    "\t$CalculateDifficulty(), L#187: op: =, lhs: 29, rhs: 29, result: 29",
    "\t$CalculateDifficulty(), L#189: op: ==, lhs: 0, rhs: 0, result: true",
    "\t$CalculateDifficulty(), L#189: op: if, lhs: true, rhs: , result: ",
    "\t$CalculateDifficulty(), L#190: op: =, lhs: 29, rhs: 29, result: 29",
    "\t$CalculateDifficulty(), L#198: op: +, lhs: 29, rhs: 29, result: 58",
    "\t$CalculateDifficulty(), L#198: op: /, lhs: 58, rhs: 2, result: 29",
    "\t$CalculateDifficulty(), L#198: op: =, lhs: 29, rhs: 29, result: 29",
    "\t$CalculateDifficulty(), L#203: op: -, lhs: 29, rhs: 10, result: 19",
    "\t$CalculateDifficulty(), L#203: op: =, lhs: 19, rhs: 19, result: 19",
    "\t$CalculateDifficulty(), L#207: ",
    "\t$CalculateDifficulty(), L#207: op: return, lhs: 19, rhs: 19, result: ",
    "\t$CalculateDifficulty(), L#207: op: , lhs: , rhs: , result: ",
];

/// The images the trace's engine loaded, in its log's order.
const TRACE_IMAGES: [&str; 6] = [
    "OriginalQuests/DataMX/MX_Task.bcd",
    "OriginalQuests/DataMX/MX_Decision.bcd",
    "OriginalQuests/DataMX/MX_Data.bcd",
    "OriginalQuests/DataMX/MX_Build.bcd",
    "OriginalQuests/DataMX/MX_Compatibility.bcd",
    "OriginalQuests/Data/bytecode.bcd",
];

/// The VM prints the engine's trace of a dispatch, tabs included.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_dispatch_traces_as_the_engine_logged_it() {
    let sdk = common::sdk();
    let images: Vec<Vec<u8>> = TRACE_IMAGES.iter().map(|p| read(&sdk.join(p))).collect();
    let program = Program::load(&images).expect("the SDK's images load");
    let lines = Shared::new(Vec::new());
    let sink = lines.clone();
    let mut vm = Vm::new(&program, DEBUG);

    vm.set_trace(Some(Box::new(move |line: &str| {
        sink.borrow_mut().push(line.to_owned());
    })));

    let args = [int(30000), int(15), int(20), int(50), int(0), int(0)];
    let run = vm
        .dispatch(&mut LogHost::default(), "CalculateDifficulty", &args)
        .expect("runs");

    assert_eq!(run.result, int(19));
    assert_eq!(*lines.borrow(), CALCULATE_DIFFICULTY);
}

/// Pure builtins, from their native code.
#[test]
fn the_native_builtins_compute_what_the_binary_does() {
    let program = compiled(
        "function Root(float x) is float\nbegin\n\treturn $Sqrt(x);\nend\n\
         function Far(coordinate a, coordinate b) is integer\nbegin\n\treturn $DistanceBetweenCoords(a, b);\nend\n\
         function Both(list a, list b) is list\nbegin\n\treturn $AddLists(a, b);\nend\n\
         function Wrap(list a, integer b) is list\nbegin\n\treturn $Concatenate(a, b);\nend\n\
         function Drop(list a, integer n) is list\nbegin\n\t$RemoveListMember(a, n);\n\treturn a;\nend\n\
         function Draw(integer n) is integer\nbegin\n\treturn $RandomNumber(n);\nend\n\
         function Valid() is boolean\nbegin\n\treturn $ValidFunction($LookupFunction(\"Root\"));\nend\n\
         function Nobody() is agent\nbegin\n\treturn $NullAgent();\nend\n\
         function Say(integer mark) is integer\nbegin\n\t$DebugOut(mark, \"at\", 1.5);\n\treturn 0;\nend\n",
    );
    let mut vm = Vm::new(&program, RELEASE);
    let mut host = LogHost::default();

    // sqrt(2) floors in fixed point: 1448 / 1024.
    assert_eq!(
        call(&mut vm, &mut host, "Root", &[fixed(2048)]).result,
        fixed(1448)
    );
    assert_eq!(
        call(
            &mut vm,
            &mut host,
            "Far",
            &[Value::Coord(0, 0), Value::Coord(3, 4)]
        )
        .result,
        int(5)
    );
    assert_eq!(
        call(
            &mut vm,
            &mut host,
            "Both",
            &[list(&[int(1)]), list(&[int(2)])]
        )
        .result,
        list(&[int(1), int(2)])
    );
    assert_eq!(
        call(&mut vm, &mut host, "Wrap", &[list(&[int(1)]), int(2)]).result,
        list(&[list(&[int(1)]), int(2)])
    );
    assert_eq!(
        call(
            &mut vm,
            &mut host,
            "Drop",
            &[list(&[int(1), int(2)]), int(1)]
        )
        .result,
        list(&[int(2)])
    );

    let out = call(&mut vm, &mut host, "Drop", &[list(&[int(1)]), int(2)]);

    assert_eq!(
        message(&out),
        Some("RemoveListMember( { 1 }, 2 ) specified an out-of-bounds index.")
    );

    vm.seed(1);
    assert_eq!(
        call(&mut vm, &mut host, "Draw", &[int(100)]).result,
        int(83)
    );
    assert_eq!(call(&mut vm, &mut host, "Draw", &[int(1)]).result, int(0));

    // The bound loses its sign bit: -100 draws below
    // 0x7fffff9c, so the state itself; i32::MIN draws below 0, giving 0.
    vm.seed(1);
    assert_eq!(
        call(&mut vm, &mut host, "Draw", &[int(-100)]).result,
        int(0x0afa_cbff)
    );
    assert_eq!(
        call(&mut vm, &mut host, "Draw", &[int(i32::MIN)]).result,
        int(0)
    );

    assert_eq!(
        call(&mut vm, &mut host, "Valid", &[]).result,
        Value::Boolean(true)
    );
    assert_eq!(
        call(&mut vm, &mut host, "Nobody", &[]).result,
        Value::Agent(NULL_AGENT)
    );

    host.log.clear();
    call(&mut vm, &mut host, "Say", &[int(1)]);
    call(&mut vm, &mut host, "Say", &[int(native::DEBUG_OUT_MARK)]);

    assert_eq!(host.log, ["911 at 1.5 "]);

    // Engine flag 2 prints every line, marked or not.
    let mut all = Vm::new(
        &program,
        Config {
            debug_out: DebugOut::All,
            ..RELEASE
        },
    );

    host.log.clear();
    call(&mut all, &mut host, "Say", &[int(1)]);
    call(&mut all, &mut host, "Say", &[int(native::DEBUG_OUT_MARK)]);

    assert_eq!(host.log, ["1 at 1.5 ", "911 at 1.5 "]);
}

/// `DistanceBetweenCoords` sums the squares in 32 bits. A sum
/// wrapped positive roots the wrapped value; one wrapped negative roots to
/// NaN, and float to int takes `cvttsd2si` on any SSE2 CPU,
/// which gives 0x80000000.
#[test]
fn distance_between_coords_wraps_the_sum() {
    let program = compiled(
        "function Far(coordinate a, coordinate b) is integer\nbegin\n\treturn $DistanceBetweenCoords(a, b);\nend\n",
    );
    let mut vm = Vm::new(&program, RELEASE);
    let mut host = LogHost::default();
    let mut far = |dx: i32| {
        call(
            &mut vm,
            &mut host,
            "Far",
            &[Value::Coord(0, 0), Value::Coord(dx, 0)],
        )
        .result
    };

    // 65536² is 2^32, which wraps to 0.
    assert_eq!(far(1 << 16), int(0));
    // 65539² wraps to 393,225, whose root is 627.07.
    assert_eq!(far((1 << 16) + 3), int(627));
    // 46341² is past i32::MAX: negative.
    assert_eq!(far(46_341), int(i32::MIN));
    assert_eq!(far(46_340), int(46_340));
}

/// A host running a script from inside a builtin, as engine callbacks do:
/// the dispatch runs at once, catches its own error, costs no budget.
struct Dispatching {
    inner: LogHost,
    callback: &'static str,
}

impl Host for Dispatching {
    fn builtin(&mut self, vm: &mut Vm<'_>, call: &Call<'_>) -> Result<(), Fault> {
        let name = self.callback;
        let run = vm.dispatch(self, name, &[int(7)]).map_err(Fault::Crash)?;

        self.inner.calls.push(Logged {
            name: call.builtin().name,
            args: vec![Some(run.result)],
        });
        Ok(())
    }

    fn attribute(&mut self, vm: &mut Vm<'_>, agent: u32, name: &str) -> Result<ObjId, Fault> {
        self.inner.attribute(vm, agent, name)
    }

    fn named(&mut self, name: &str) -> Value {
        self.inner.named(name)
    }

    fn log(&mut self, text: &str) {
        self.inner.log(text);
    }
}

#[test]
fn a_builtin_can_dispatch_a_script_at_once() {
    let program = compiled(
        "function Callback(integer n) is integer\nbegin\n\treturn n / 0;\nend\n\
         function Fine(integer n) is integer\nbegin\n\treturn n * 2;\nend\n\
         function Outer() is integer\nbegin\n\t$Halt();\n\treturn 3;\nend\n",
    );
    let mut vm = Vm::new(&program, RELEASE);

    // Alone, `Outer` executes this many records.
    let mut alone = Vm::new(&program, RELEASE);
    call(&mut alone, &mut LogHost::default(), "Outer", &[]);

    let mut failing = Dispatching {
        inner: LogHost::default(),
        callback: "Callback",
    };
    let run = call(&mut vm, &mut failing, "Outer", &[]);

    assert_eq!(run.result, int(3), "the dispatch caught its error");
    assert_eq!(vm.count(), alone.count(), "the dispatch cost nothing");
    assert!(failing.inner.log[0].starts_with("GplDispatcherHandle script error:"));

    let mut fine = Dispatching {
        inner: LogHost::default(),
        callback: "Fine",
    };
    call(&mut vm, &mut fine, "Outer", &[]);

    assert_eq!(fine.inner.calls[0].args, [Some(int(14))]);
}

// ---------------------------------------------------------------------------
// The corpus

/// Test values for a parameter of type `code`, edges included: 0, ±0.5,
/// negatives, near 2^21, empty lists, the null agent.
fn samples(code: TypeCode) -> Vec<Value> {
    const NEAR_WRAP: i32 = 1 << 21;

    match Kind::try_from(code).ok() {
        Some(Kind::Integer) => vec![int(0), int(1), int(-7), int(NEAR_WRAP - 1), int(NEAR_WRAP)],
        Some(Kind::Float) => vec![
            fixed(0),
            fixed(512),
            fixed(-512),
            fixed(-3 << 10),
            fixed(i32::MAX),
        ],
        Some(Kind::String) => vec![Value::String(String::new()), Value::String("Gold".into())],
        Some(Kind::List) => vec![
            list(&[]),
            list(&[int(1), int(2)]),
            list(&[Value::Agent(NULL_AGENT)]),
        ],
        Some(Kind::Agent) => vec![Value::Agent(NULL_AGENT), Value::Agent(1)],
        Some(Kind::Boolean) => vec![Value::Boolean(false), Value::Boolean(true)],
        Some(Kind::Coord) => vec![Value::Coord(0, 0), Value::Coord(5, -3)],
        Some(Kind::Function) => vec![Value::Function(NULL_FUNCTION)],
        _ => vec![Value::Null],
    }
}

/// Argument vectors for a function: variant `k` gives each parameter its
/// type's `k`-th sample, wrapping.
fn arguments(program: &Program, def: u32) -> Vec<Vec<Value>> {
    let params = program.definitions()[def as usize]
        .signature()
        .map(|s| s.params.clone())
        .unwrap_or_default();
    let widest = params
        .iter()
        .map(|p| samples(p.ty).len())
        .max()
        .unwrap_or(1);

    (0..widest)
        .map(|k| {
            params
                .iter()
                .map(|p| {
                    let options = samples(p.ty);
                    options[k % options.len()].clone()
                })
                .collect()
        })
        .collect()
}

/// Functions whose calls, followed through script calls, reach only builtins
/// the VM runs itself; a call through a value could reach anything.
fn pure_functions(program: &Program) -> Vec<u32> {
    let mut impure: HashSet<u32> = HashSet::new();

    loop {
        let before = impure.len();

        for (def, d) in program.definitions().iter().zip(0u32..) {
            let reaches_world = def.signature().is_some()
                && !impure.contains(&d)
                && def
                    .code
                    .iter()
                    .filter_map(|i| i.call.as_deref())
                    .any(|c| match c.callee {
                        Callee::Fn(Target::Builtin(b)) => !native::runs(b),
                        Callee::Fn(Target::Script(s)) => impure.contains(&s),
                        Callee::Slot(_) | Callee::Name(_) => true,
                    });

            if reaches_world {
                impure.insert(d);
            }
        }

        if impure.len() == before {
            break;
        }
    }

    program
        .definitions()
        .iter()
        .zip(0u32..)
        .filter(|(def, d)| def.signature().is_some() && !impure.contains(d))
        .map(|(_, d)| d)
        .collect()
}

/// A run's category: the exception's message with its arguments cut, or the
/// crash.
fn category(outcome: &Result<Run, Crash>) -> Option<String> {
    let template = |message: &str| {
        [host::NO_AGENT, native::REMOVE_RANGE]
            .into_iter()
            .find(|t| {
                t.split('%')
                    .next()
                    .is_some_and(|head| message.starts_with(head))
            })
            .map_or_else(|| message.to_owned(), str::to_owned)
    };

    match outcome {
        Ok(run) => run.error.as_ref().map(|e| template(&e.message)),
        Err(crash) => Some(format!("crash: {crash:?}")),
    }
}

/// Records one run may execute before it counts as a hang.
const LIMIT: u64 = 200_000;

/// Run every variant of every function in `defs`; count outcomes by category.
fn tally(program: &Program, defs: &[u32], config: Config) -> (usize, BTreeMap<String, usize>) {
    let mut runs = 0;
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();

    for &def in defs {
        for args in arguments(program, def) {
            // A fresh VM per run: the corpus is not one game.
            let mut vm = Vm::new(program, config);
            let mut host = LogHost::new(program);

            vm.set_limit(Some(LIMIT));
            runs += 1;

            let outcome = vm.run(&mut host, def, &args, Catcher::Thread);

            if let Some(what) = category(&outcome) {
                *seen.entry(what).or_default() += 1;
            }
        }
    }

    (runs, seen)
}

/// Pure functions in HD's base game and expansion.
const PURE_FUNCTIONS: usize = 59;

/// Their runs, one per sample vector.
const PURE_RUNS: usize = 156;

/// What stops a pure run, in both modes. Each the engine throws too, given
/// the same arguments:
///
/// - the null agent sample read for an attribute;
/// - an integer list sample where the function wants agents;
/// - `random_to_percent(0, 0)` dividing by zero.
const PURE_STOPS: [(&str, usize); 3] = [
    ("Divide by zero error.", 1),
    (
        "Tried to get attribute \"%s\"from a non-existent agent ",
        29,
    ),
    ("bad conversion to gplagent&", 11),
];

/// Every function whose calls reach no world builtin runs on its generated
/// arguments with no crash; what stops is a `GplException` from the
/// arguments.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn every_function_calling_no_world_builtin_runs() {
    let program = hd_program();
    let pure = pure_functions(&program);

    assert_eq!(pure.len(), PURE_FUNCTIONS);

    for config in [RELEASE, DEBUG] {
        let (runs, seen) = tally(&program, &pure, config);
        let stops: Vec<(&str, usize)> = seen.iter().map(|(w, n)| (w.as_str(), *n)).collect();

        assert_eq!(runs, PURE_RUNS);
        assert_eq!(stops, PURE_STOPS, "{config:?}");
    }
}

/// HD functions returning a value with no `is` type: the return slot is the
/// shared null, and `GplNull` throws "Assignment to NULL attempted." at
/// every `return` with a value, in the engine too.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_value_returned_without_a_type_throws() {
    let program = hd_program();
    let untyped: Vec<&str> = program
        .definitions()
        .iter()
        .filter(|d| {
            d.signature().is_some_and(|s| s.returns == TypeCode::Null)
                && d.code
                    .iter()
                    .any(|i| i.op == regent_core::gpl::opcode::Op::ReturnValue)
        })
        .map(|d| program.names().get(d.name))
        .collect();

    assert_eq!(untyped, UNTYPED_RETURNS);

    let def = definition(&program, UNTYPED_RETURNS[0]);
    let mut vm = Vm::new(&program, RELEASE);
    let mut host = LogHost::new(&program);
    let run = vm
        .run(&mut host, def, &[Value::Agent(1)], Catcher::Thread)
        .expect("no crash");

    assert_eq!(message(&run), Some("Assignment to NULL attempted."));
}

/// HD's functions that return a value but declare no type.
const UNTYPED_RETURNS: [&str; 2] = ["Multiple_Attack_Check", "Recall_Check"];

/// Script functions over every image, each counted in the image that
/// defines it.
const CORPUS_FUNCTIONS: usize = 11_472;

/// Their runs, one per sample vector.
const CORPUS_RUNS: usize = 26_551;

/// What stops a corpus run. Most are the logging host's: its builtins leave
/// defaults (empty lists, zeros, null functions), its attributes are fresh
/// values, null for a name no prototype declares, and nothing changes
/// between reads.
///
/// - Null agent sample read for an attribute: the engine's throw.
/// - `ListMember` of an empty list a builtin left; divide by a zero default.
/// - An attribute no prototype declares, so null: an `if` on the empty
///   result of an operator on it, or conversions from it (`bool`, `int`, ...);
///   an operator on that empty result is a null-operand crash.
/// - Integer list samples where agents are wanted (`gplagent&`).
/// - `Multiple_Attack_Check` and `Recall_Check` return a value with no type.
/// - A call to a function no loaded image defines calls the null record, so
///   its result is null: `ecs_return_rat`'s `$rn`.
/// - `while` on an empty result of a null attribute: unchecked, a crash.
/// - [`Crash::Unending`]: loops waiting on world state the host never
///   changes, or counting to a 2^21 sample.
const CORPUS_STOPS: [(&str, usize); 19] = [
    ("Assignment to NULL attempted.", 9),
    ("Divide by zero error.", 239),
    (
        "If statement evaluates to a NULL value in function $()",
        146,
    ),
    (
        "ListMember() tried to access an element # beyond its size",
        1411,
    ),
    (
        "Tried to get attribute \"%s\"from a non-existent agent ",
        8074,
    ),
    ("bad conversion to bool", 308),
    ("bad conversion to bool&", 4),
    ("bad conversion to coord&", 2),
    ("bad conversion to float", 5),
    ("bad conversion to gplagent&", 884),
    ("bad conversion to gpllist&", 1),
    ("bad conversion to int", 17),
    ("bad conversion to list&", 21),
    ("bad conversion to string", 12),
    ("bad conversion to string&", 16),
    ("crash: NullValue(Condition)", 4),
    ("crash: NullValue(Operand)", 6),
    ("crash: NullValue(Result)", 5),
    ("crash: Unending", 47),
];

/// Every `.bcd` on disk: SDK, Workshop, HD.
fn every_bcd() -> Vec<PathBuf> {
    let roots = [common::sdk(), common::workshop(), common::hd()];

    let mut out: Vec<PathBuf> = roots
        .iter()
        .flat_map(|root| common::walk(root, &["bcd"]))
        .collect();

    out.sort();
    out.dedup();
    out
}

/// HD's images in load order, the context another image loads over.
fn hd_images() -> Vec<Vec<u8>> {
    hd_paths().iter().map(|p| read(p)).collect()
}

/// Every function of every image, each image loaded first over HD's
/// expansion so its calls resolve, run on every sample vector in release
/// play.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn the_whole_corpus_runs_against_a_logging_host() {
    let context = hd_images();
    let mut functions = 0;
    let mut runs = 0;
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();

    for path in every_bcd() {
        let images: Vec<Vec<u8>> = std::iter::once(read(&path))
            .chain(context.iter().cloned())
            .collect();
        let program = Program::load(&images).expect("loads");
        let own: Vec<u32> = program
            .definitions()
            .iter()
            .zip(program.origin())
            .zip(0u32..)
            .filter(|((d, origin), _)| **origin == 0 && d.signature().is_some())
            .map(|(_, d)| d)
            .collect();

        functions += own.len();

        let (n, found) = tally(&program, &own, RELEASE);

        runs += n;

        for (what, count) in found {
            *seen.entry(what).or_default() += count;
        }
    }

    let stops: Vec<(&str, usize)> = seen.iter().map(|(w, n)| (w.as_str(), *n)).collect();

    assert_eq!((functions, runs), (CORPUS_FUNCTIONS, CORPUS_RUNS));
    assert_eq!(stops, CORPUS_STOPS);
}
