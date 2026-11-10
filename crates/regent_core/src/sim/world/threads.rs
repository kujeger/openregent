//! GPL threads and the debug builtins: what starts, stops, pauses and
//! tests a thread, and what halts the scripts. **Binary**, each from its
//! native; a thread argument through `GplFunction&`.
//!
//! ```text
//!  NewThread, RunThread, RunThreadOnce  threads::start at the world's now
//!  SetThreadInterval  the interval only
//!  SuspendThread      mode |= 0x80; left = interval - (now - last)
//!  ResumeThread       mode &= !0x80; last = now - interval + left; left = 0
//!  KillThread         mode = 0, the suspension with it
//!  IsRunning          mode less 0x80 not 0, or running
//!  Breakpoint(x)      first call ever: countdown = x as int&
//!                     an integer: countdown - 1; was it <= 1,
//!                       a break request (debug state bit 1)
//!                     "INT3", any case: an int3 trap
//!                     true: a break request; else nothing
//!  Halt()             engine flags &= ~4: no scheduler
//!                     pass, no UnitDelete
//!  NoDebugMessages    engine flags &= ~0x100: the log off,
//!                     trace, DebugOut and error reports
//!  LogOn, LogOff      a bare ret
//! ```
//!
//! No shipped script calls a debug builtin. A break request is read only
//! by the GPL debugger, gpl execute under engine flags
//! `0x2000`: it clears the bit and stops before the next
//! record. Release play never reads it.

use super::{World, int_ref, put_bool};
use crate::gpl::load::Program;
use crate::gpl::ops::{Accessor, OpError, Value};
use crate::gpl::runtime::ThreadMode;
use crate::gpl::vm::host::Call;
use crate::gpl::vm::text::{FloatText, Names};
use crate::gpl::vm::threads::{self, Busy};
use crate::gpl::vm::{Callable, Crash, Fault, Handle, ObjId, Thread, Vm, text};

/// Argument 1 as a thread: the function object it names.
fn thread_arg(vm: &Vm<'_>, args: &[Handle]) -> Result<ObjId, Fault> {
    let o = vm.arg(args, 1)?;

    match vm.object(o) {
        Value::Function(_) => Ok(o),
        _ => Err(OpError::BadConversion(Accessor::FunctionRef).into()),
    }
}

/// The thread fields of `o`: an idle value's where it has none.
fn fields(vm: &Vm<'_>, o: ObjId) -> Thread {
    vm.thread(o).cloned().unwrap_or_else(|| Thread {
        frame: text::IDLE_FRAME.to_owned(),
        ..Thread::default()
    })
}

/// Names for a thread frame's text: functions from the program, agents from
/// the world.
struct Naming<'a> {
    program: &'a Program,
    world: &'a World,
    float_text: FloatText,
}

impl Names for Naming<'_> {
    fn function(&self, handle: u32) -> Option<&str> {
        match Vm::callable(handle) {
            Callable::Null => None,
            Callable::Builtin(b) => Some(b.get().name),
            Callable::Definition(d) => self
                .program
                .definitions()
                .get(d as usize)
                .map(|def| self.program.names().get(def.name)),
        }
    }

    fn agent(&mut self, id: u32) -> Option<String> {
        self.world.agent_name(id)
    }

    fn float_text(&self) -> FloatText {
        self.float_text
    }
}

impl World {
    /// `SuspendThread(thread)`, gpl suspend thread: its suspended bit set, and
    /// the time left kept, `interval - now + last`.
    pub(super) fn suspend_thread(&self, vm: &mut Vm<'_>, args: &[Handle]) -> Result<(), Fault> {
        let o = thread_arg(vm, args)?;
        let mut t = fields(vm, o);

        t.suspended = true;
        t.left = t
            .interval
            .wrapping_sub(self.now.cast_signed())
            .wrapping_add(t.last.cast_signed());
        vm.set_thread(o, Some(t));
        Ok(())
    }

    /// `ResumeThread(thread)`, gpl resume thread: bit 7 cleared, `last` made
    /// from the time left so the next run comes that much later, `left`
    /// zeroed.
    pub(super) fn resume_thread(&self, vm: &mut Vm<'_>, args: &[Handle]) -> Result<(), Fault> {
        let o = thread_arg(vm, args)?;
        let mut t = fields(vm, o);

        t.suspended = false;
        t.last = self
            .now
            .cast_signed()
            .wrapping_sub(t.interval)
            .wrapping_add(t.left)
            .cast_unsigned();
        t.left = 0;
        vm.set_thread(o, Some(t));
        Ok(())
    }

    /// `KillThread(thread)`, gpl kill thread: the whole mode word zeroed, so
    /// a suspended thread is no longer suspended either. A running call
    /// runs to its end.
    #[allow(
        clippy::unused_self,
        reason = "a checked builtin, called as the others are"
    )]
    pub(super) fn kill_thread(&self, vm: &mut Vm<'_>, args: &[Handle]) -> Result<(), Fault> {
        let o = thread_arg(vm, args)?;

        if let Some(mut t) = vm.thread(o).cloned() {
            t.mode = ThreadMode::Off;
            t.suspended = false;
            vm.set_thread(o, Some(t));
        }

        Ok(())
    }

    /// `IsRunning(thread)`, gpl is running: gpl thread active,
    /// scheduled (suspended or not) or inside its run.
    #[allow(
        clippy::unused_self,
        reason = "a checked builtin, called as the others are"
    )]
    pub(super) fn is_running(&self, vm: &mut Vm<'_>, args: &[Handle]) -> Result<(), Fault> {
        let o = thread_arg(vm, args)?;
        let busy = vm.thread(o).is_some_and(Thread::busy);
        let ret = vm.arg(args, 0)?;

        put_bool(vm, ret, busy)
    }

    /// `SetThreadInterval(thread, ms)`, gpl set thread interval: writes the interval only.
    /// A thread the world follows stays followed.
    #[allow(
        clippy::unused_self,
        reason = "a checked builtin, called as the others are"
    )]
    pub(super) fn interval(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<(), Fault> {
        let o = vm.arg(args, 1)?;

        if !matches!(vm.object(o), Value::Function(_)) {
            Err(OpError::BadConversion(Accessor::FunctionRef))?;
        }

        let ms = int_ref(vm, args, 2)?;
        let mut thread = vm
            .thread(o)
            .cloned()
            .unwrap_or_else(|| crate::gpl::vm::Thread {
                frame: crate::gpl::vm::text::IDLE_FRAME.to_owned(),
                mode: ThreadMode::Off,
                ..crate::gpl::vm::Thread::default()
            });

        thread.interval = ms;
        vm.set_thread(o, Some(thread));
        Ok(())
    }

    /// `NewThread`, `RunThread` and `RunThreadOnce`: [`threads::start`] at
    /// [`World::now`].
    pub(super) fn thread(
        &mut self,
        vm: &mut Vm<'_>,
        call: &Call<'_>,
        mode: ThreadMode,
        busy: Busy,
    ) -> std::result::Result<(), Fault> {
        let mut names = Naming {
            program: vm.program(),
            world: self,
            float_text: vm.config().float_text,
        };
        threads::start(vm, call, mode, busy, self.now, &mut names)?;
        Ok(())
    }
}

/// The text `Breakpoint` traps on, `"INT3"`, compared folding case
/// (`string_compare(…, 1)` in gpl breakpoint).
pub const TRAP: &str = "INT3";

/// Whether threads run and agents are deleted: engine flags bit
/// 4, set in the file. The scheduler and
/// unit gpl delete test it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Gpl {
    #[default]
    Running,
    /// After `Halt`; cheat code 1 runs it again, as
    /// does loading a save. **Inferred**: a unit's
    /// deletion then leaves its agent, as no `UnitDelete` runs.
    Halted,
}

/// The engine's log, engine flags `0x100` (`-debugout`): every
/// line goes through debug log, debug log part or
/// debug log vprintf, which test it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Logging {
    #[default]
    On,
    /// After `NoDebugMessages`; nothing sets it again.
    Off,
}

/// `Breakpoint`'s state: statics breakpoint countdown, the countdown, and
/// breakpoint guard bit 0, whether it was set; the request bit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Breaks {
    /// `None` before the first call.
    pub countdown: Option<i32>,
    /// gpl debug state bit 1: a break the debugger has not taken.
    pub requested: bool,
}

impl World {
    /// `Breakpoint(x)`, gpl breakpoint. The first call ever sets the
    /// countdown from argument 1 through `int&` (a string or boolean
    /// throws its bad conversion there, the countdown then 0); after, an
    /// integer is not read again. Returns nothing.
    pub(super) fn breakpoint(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<(), Fault> {
        if self.scripts.breaks.countdown.is_none() {
            self.scripts.breaks.countdown = Some(0);
            self.scripts.breaks.countdown = Some(int_ref(vm, args, 1)?);
        }

        match vm.object(vm.arg(args, 1)?) {
            Value::Integer(_) => {
                let left = self.scripts.breaks.countdown.unwrap_or_default();

                self.scripts.breaks.countdown = Some(left.wrapping_sub(1));

                if left <= 1 {
                    self.scripts.breaks.requested = true;
                }

                Ok(())
            }
            Value::String(s) if s.eq_ignore_ascii_case(TRAP) => Err(Crash::Breakpoint.into()),
            Value::Boolean(true) => {
                self.scripts.breaks.requested = true;
                Ok(())
            }
            _ => Ok(()),
        }
    }

    /// `NoDebugMessages()`: the log off, the VM's trace with it.
    pub(super) fn no_debug_messages(&mut self, vm: &mut Vm<'_>) {
        self.scripts.logging = Logging::Off;
        vm.set_trace(None);
    }

    /// Whether threads run and `UnitDelete` is dispatched.
    #[must_use]
    pub const fn gpl(&self) -> Gpl {
        self.scripts.gpl
    }

    /// Whether the engine's log is written.
    #[must_use]
    pub const fn logging(&self) -> Logging {
        self.scripts.logging
    }

    /// `Breakpoint`'s countdown and request.
    #[must_use]
    pub const fn breaks(&self) -> Breaks {
        self.scripts.breaks
    }
}

/// The scripts' run state the world keeps: whether threads run, the log,
/// `Breakpoint`'s state.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Scripts {
    gpl: Gpl,
    logging: Logging,
    breaks: Breaks,
}

impl Scripts {
    /// Threads run, or stop: `Halt`, a cheat.
    pub(super) const fn set_gpl(&mut self, gpl: Gpl) {
        self.gpl = gpl;
    }
}

mod persist {
    use super::*;
    use crate::save::{self as m, Refused, Saved};
    use std::result::Result;

    impl Saved for Scripts {
        type Model = m::Scripts;

        fn save(&self) -> m::Scripts {
            m::Scripts {
                gpl: self.gpl.save(),
                logging: self.logging.save(),
                breaks: self.breaks.save(),
            }
        }

        fn load(model: m::Scripts) -> Result<Scripts, Refused> {
            Ok(Scripts {
                gpl: Gpl::load(model.gpl)?,
                logging: Logging::load(model.logging)?,
                breaks: Breaks::load(model.breaks)?,
            })
        }
    }
}
