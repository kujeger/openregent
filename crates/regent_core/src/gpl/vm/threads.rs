//! Threads: the builtins that start one, and the run the scheduler makes.
//!
//! **Binary**, `MajestyHD.exe`; the rules are `gpl::runtime`'s.
//!
//! ```text
//!  NewThread / RunThread ──> interval, mode, last = now; arguments if idle
//!  scheduler ──> agent's function attributes, name order ──> run(o, now)
//!                  pending moved in; if due: running, once cleared,
//!                  invoked with the frame, last = now; pending moved in
//! ```

use super::host::{Call, Host};
use super::text::{self, Names};
use super::{Crash, Fault, ObjId, ThreadMode, Vm};
use crate::gpl::ops::{self, Value};
use crate::gpl::runtime::{self, Catcher};

/// Arguments before the thread's own: the return slot, the function, then
/// the interval.
const OWN_ARGS: usize = 3;

/// What a creator writes to a thread already scheduled or running.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Busy {
    /// `NewThread`, `RunThread`: the mode too.
    SetsMode,
    /// `RunThreadOnce`:
    /// interval and last only, so a suspended thread stays suspended.
    KeepsMode,
}

/// `NewThread`, `RunThread`
/// and `RunThreadOnce`: argument 1
/// through `GplFunction&`, 2 through `int&`. An idle thread takes copies of
/// the arguments after those two (gpl frame copy args, then
/// gpl frame erase arg twice); a busy one keeps its own. Interval
/// and `last = now` are written either way, `mode` as `busy` says.
///
/// # Errors
///
/// A bad conversion of either argument; a read past the vector.
pub fn start(
    vm: &mut Vm<'_>,
    call: &Call<'_>,
    mode: ThreadMode,
    busy: Busy,
    now: u32,
    names: &mut dyn Names,
) -> Result<ObjId, Fault> {
    let o = vm.arg(call.args, 1)?;

    ops::function_of(vm.object(o))?;

    let interval = ops::int_of(vm.object(vm.arg(call.args, 2)?))?;
    let mut thread = vm.thread(o).cloned().unwrap_or_default();
    let idle = !thread.busy();

    if idle {
        let ret = call
            .args
            .first()
            .and_then(|h| vm.value(*h))
            .cloned()
            .unwrap_or(Value::Null);
        let args: Vec<Value> = call
            .args
            .iter()
            .skip(OWN_ARGS)
            .map(|h| vm.value(*h).cloned().unwrap_or(Value::Null))
            .collect();
        let frame: Vec<String> = std::iter::once(&ret)
            .chain(&args)
            .map(|v| text::value(v, names))
            .collect();

        thread.frame = frame.join(text::SEPARATOR);
        thread.args = args;
    }

    thread.interval = interval;
    thread.last = now;

    if idle || busy == Busy::SetsMode {
        thread.mode = mode;
    }

    vm.set_thread(o, Some(thread));
    Ok(o)
}

/// What [`run`] did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ran {
    /// Not active: no mode, or the null record.
    Idle,
    /// Active, not due.
    Waiting,
    /// Invoked.
    Ran,
}

/// Gpl thread run on function object `o` at game time
/// `now`. An active thread moves a pending function in; if due, it is
/// running, a once-thread's mode clears, the function runs on the frame,
/// then `last = now` and running clears; a pending function moves in again.
/// A caught error resumes past `last = now`:
/// running stays set, nothing more moves.
///
/// # Errors
///
/// A [`Crash`] inside the run.
pub fn run(vm: &mut Vm<'_>, host: &mut dyn Host, o: ObjId, now: u32) -> Result<Ran, Crash> {
    let function = match vm.object(o) {
        Value::Function(f) => *f,
        _ => ops::NULL_FUNCTION,
    };
    let active = vm
        .thread(o)
        .is_some_and(|t| t.mode != ThreadMode::Off && function != ops::NULL_FUNCTION);

    if active {
        pending_in(vm, o);

        // Nothing cloned unless due: most visits find the thread waiting.
        let due = vm.thread(o).filter(|t| {
            let flags = t.mode as u32 | if t.suspended { runtime::SUSPENDED } else { 0 };

            runtime::is_due(flags, t.interval.cast_unsigned(), t.last, now)
        });

        match due.map(|t| t.args.clone()) {
            Some(args) => invoke(vm, host, o, &args, now),
            None => Ok(Ran::Waiting),
        }
    } else {
        Ok(Ran::Idle)
    }
}

/// The due branch of [`run`].
fn invoke(
    vm: &mut Vm<'_>,
    host: &mut dyn Host,
    o: ObjId,
    args: &[Value],
    now: u32,
) -> Result<Ran, Crash> {
    let function = match vm.object(o) {
        Value::Function(f) => *f,
        _ => ops::NULL_FUNCTION,
    };

    begin(vm, o);

    let run = vm.run_handle(host, function, args, Catcher::Thread)?;

    if run.error.is_none() {
        end(vm, o, Some(now));
    }

    Ok(Ran::Ran)
}

/// A due thread's run starts: running set, a once-thread's mode cleared.
pub fn begin(vm: &mut Vm<'_>, o: ObjId) {
    if let Some(t) = vm.threads.get_mut(&o) {
        t.running = true;

        if t.mode == ThreadMode::Once {
            t.mode = ThreadMode::Off;
        }
    }
}

/// A thread's run returned: `last = now` when known, running cleared, a
/// pending function moved in. The fields are re-read: the run may have
/// started its own thread again, assigned it a pending function, or
/// reset or released it, which leaves none to write.
pub fn end(vm: &mut Vm<'_>, o: ObjId, now: Option<u32>) {
    if let Some(t) = vm.threads.get_mut(&o) {
        t.last = now.unwrap_or(t.last);
        t.running = false;
        pending_in(vm, o);
    }
}

/// Move the pending function in, as an active thread's visit does first.
pub fn visit(vm: &mut Vm<'_>, o: ObjId) {
    pending_in(vm, o);
}

/// Move thread `o`'s pending function, if any, into the function slot.
fn pending_in(vm: &mut Vm<'_>, o: ObjId) {
    let pending = vm.threads.get_mut(&o).and_then(|t| t.pending.take());

    if let Some(p) = pending {
        *vm.object_mut(o) = Value::Function(p);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpl::builtins;
    use crate::gpl::load::Program;
    use crate::gpl::vm::{Config, Thread};

    struct Anon;

    impl Names for Anon {
        fn function(&self, _: u32) -> Option<&str> {
            None
        }

        fn agent(&mut self, _: u32) -> Option<String> {
            None
        }
    }

    /// `name(f, interval)` at `now` on thread `f`, as `busy` says.
    fn start_on(vm: &mut Vm<'_>, f: ObjId, name: &str, interval: i32, busy: Busy, now: u32) {
        let builtin = builtins::id(name).expect("registered");
        let ret = vm.alloc(Value::Null);
        let n = vm.alloc(Value::Integer(interval));
        let args = [
            vm.top_handle(Some(ret)),
            vm.top_handle(Some(f)),
            vm.top_handle(Some(n)),
        ];
        let call = Call {
            builtin,
            args: &args,
        };

        start(vm, &call, ThreadMode::Once, busy, now, &mut Anon).expect("starts");
    }

    /// A run that reset its own thread ends with no
    /// thread fields: the end writes no fresh ones.
    #[test]
    fn a_reset_during_the_run_leaves_no_thread() {
        let program = Program::default();
        let mut vm = Vm::new(&program, Config::default());
        let f = vm.alloc(Value::Function(0));
        let busy = Thread {
            mode: ThreadMode::Repeat,
            ..Thread::default()
        };

        vm.set_thread(f, Some(busy));
        begin(&mut vm, f);
        vm.reset(f);
        end(&mut vm, f, Some(150));

        assert_eq!(vm.thread(f), None);
    }

    /// `RunThreadOnce` on a busy thread writes interval
    /// and last only: a suspended repeating thread stays both. Idle, it is
    /// `RunThread`.
    #[test]
    fn run_thread_once_keeps_a_busy_threads_mode() {
        let program = Program::default();
        let mut vm = Vm::new(&program, Config::default());
        let f = vm.alloc(Value::Function(0));
        let g = vm.alloc(Value::Function(0));
        let busy = Thread {
            mode: ThreadMode::Repeat,
            suspended: true,
            ..Thread::default()
        };

        vm.set_thread(f, Some(busy));
        start_on(&mut vm, f, "RunThreadOnce", 500, Busy::KeepsMode, 150);
        start_on(&mut vm, g, "RunThreadOnce", 300, Busy::KeepsMode, 150);

        let kept = vm.thread(f).cloned().unwrap_or_default();
        let made = vm.thread(g).cloned().unwrap_or_default();

        assert_eq!(
            (kept.mode, kept.suspended, kept.interval, kept.last),
            (ThreadMode::Repeat, true, 500, 150)
        );
        assert_eq!(
            (made.mode, made.interval, made.last),
            (ThreadMode::Once, 300, 150)
        );
    }
}
