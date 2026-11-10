//! The GPL virtual machine: runs a loaded [`Program`]'s instructions as
//! Gpl execute does.
//!
//! **Binary** unless marked; the rules are `gpl::runtime`'s and
//! `docs/gpl/runtime.md`'s "How the engine runs a function" and "The VM".
//!
//! ```text
//!  host ──> Vm::run(function, args, Catcher)        a thread or a dispatch
//!             └─> invoke ──> execute(definition)    no frame pushed
//!                   opLParen ──> frame pushed ──> invoke
//!                                 ├─ script     execute, nested
//!                                 ├─ native     builtins that touch no world
//!                                 └─ Host       every other builtin; may call
//!                                               Vm::dispatch, nested again
//! ```
//!
//! # Values
//!
//! The engine's values are pooled objects behind handles; a record's value
//! slot is a handle, an operand a pointer to one, fixed at load. Objects can
//! be shared: `__Return_value__` holds the caller's result object, and
//! `agent's "name"` the attribute's. So here too: [`ObjId`] is an object,
//! [`Handle`] a slot holding one or nothing. Each function has one set of
//! handles, three per record — its own value and its two literal operands —
//! so re-entry clobbers the running call's values, as in the engine.
//!
//! # Faults
//!
//! [`Fault::Exception`] is a `GplException`, caught by the thread or
//! dispatch that started the run. [`Fault::Crash`] is where the engine would
//! fault instead — a divide trap, a null dereference, a read past the
//! argument vector — and nothing catches it.

pub mod host;
pub mod native;
pub mod text;
pub mod threads;

use crate::base::digest::Map;
use crate::base::switch::Switch;
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use super::builtins::{self, BuiltinId};
use super::exec::{self, Definition, Dest, Head, Literal, Operand};
use super::load::{Entry, Program};
use super::opcode::Op;
use super::ops::{self, OpError, Operator, Step, Value};
use super::runtime::ThreadMode;
use super::runtime::{Catcher, DebugCheck, EXIT_WALK_FROM, RETURN_INDEX, ReleaseCheck};
use crate::base::rng::draw;
use crate::data::gpltype::{Kind, TypeCode};
pub use host::{Call, Host};

/// Which checks run: release play, or the GPL debugger's
/// (engine flags `0x2000`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Checks {
    Release,
    /// Adds [`DebugCheck`]'s parameter type, argument count and recursion
    /// checks, tracks the source line, and names the function in messages.
    Debug,
}

/// When `DebugOut` prints: only a line whose first argument is the integer
/// [`native::DEBUG_OUT_MARK`], or every line (engine flags value 2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DebugOut {
    Marked,
    All,
}

/// How a VM runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    pub checks: Checks,
    pub debug_out: DebugOut,
    /// The runtime whose `_gcvt` prints floats.
    pub float_text: text::FloatText,
}

impl Config {
    /// Release play with the shipped flags, file value 4, on HD's own
    /// runtime.
    pub const RELEASE: Config = Config {
        checks: Checks::Release,
        debug_out: DebugOut::Marked,
        float_text: text::FloatText::Msvcr90,
    };
}

impl Default for Config {
    /// [`Config::RELEASE`].
    fn default() -> Self {
        Config::RELEASE
    }
}

/// A `GplException`: what every script error throws, with the engine's
/// message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GplException {
    pub message: String,
}

impl fmt::Display for GplException {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

/// Where the engine faults and nothing catches it. **Inferred** in each case
/// that the game then crashes or reads garbage.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Crash {
    /// A 32-bit signed divide traps: a zero divisor no check caught, or a quotient past 32
    /// bits ([`OpError::Fault`]).
    DivideFault,
    /// A handle the engine dereferences is empty.
    NullValue(Null),
    /// gpl arg past the argument vector. Its checked iterator calls
    /// the invalid-parameter handler, which HD sets to a bare return,
    /// so the read goes on
    /// into whatever follows.
    ArgPastEnd,
    /// A function value naming a constant, called where an earlier callee
    /// typed the record's result: gpl function invoke calls the
    /// constant's native function, 0. On a fresh record the call fails first,
    /// `NullValue(NoClass)`, as [`CONSTANT_RETURNS`] has no class. Also the
    /// VM's own cases: [`Vm::dispatch`] of a builtin or unknown name, and a
    /// host-made handle past the definitions.
    NotCallable,
    /// `break` on a loop whose cursor is freed: the write goes through null.
    NoCursor,
    /// `opForeach` on a cursor whose node its list no longer keeps
    /// ([`ops::FREED_KEPT`]): the engine reads the freed node as its
    /// bytes lie. **Inferred**: undefined once reused.
    FreedNode,
    /// Opcodes 40 to 42, inline attribute access; no shipped `.bcd` holds
    /// them.
    InlineAttribute,
    /// Nesting past [`MAX_DEPTH`]: the engine recurses on the C stack.
    StackOverflow,
    /// `Breakpoint("INT3")`'s `int3`. **Inferred**: with no
    /// debugger attached, an unhandled breakpoint exception.
    Breakpoint,
    /// Not the engine's: [`Vm::set_limit`]'s records ran in one run. The engine
    /// runs a function to its end, so it would hang.
    Unending,
}

/// Which empty handle a [`Crash::NullValue`] dereferences.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Null {
    /// An operand: an operator's left, a missing right, a returned value.
    Operand,
    /// A result slot an operator writes that no type was given
    /// ([`OpError::NoResult`]).
    Result,
    /// A `while` condition: unlike `if`, unchecked.
    Condition,
    /// A `foreach` or `break` list.
    List,
    /// A call argument an accessor reads.
    Argument,
    /// A call's function value.
    Callee,
    /// A type code with no class: gpl value of type has no pool.
    NoClass,
}

/// A run's failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fault {
    Exception(GplException),
    Crash(Crash),
}

impl Fault {
    /// A `GplException` with `message`.
    #[must_use]
    pub fn throw(message: impl Into<String>) -> Fault {
        Fault::Exception(GplException {
            message: message.into(),
        })
    }
}

impl From<OpError> for Fault {
    fn from(e: OpError) -> Fault {
        match (e, e.message()) {
            (_, Some(message)) => Fault::throw(message),
            (OpError::Fault, None) => Fault::Crash(Crash::DivideFault),
            (OpError::NoResult, None) => Fault::Crash(Crash::NullValue(Null::Result)),
            (_, None) => Fault::Crash(Crash::NullValue(Null::Operand)),
        }
    }
}

impl From<Crash> for Fault {
    fn from(c: Crash) -> Fault {
        Fault::Crash(c)
    }
}

/// The return type gpl register script registers for a constant: its
/// scope entry's `unknowntype`, 11, through gpl pool index — 10,
/// undefined, the type gpl function return type gives an unknown name.
/// No class: gpl value of type has no pool for it.
pub const CONSTANT_RETURNS: TypeCode = TypeCode::Undefined;

/// Nested calls past this depth are [`Crash::StackOverflow`]. **Inferred**
/// bound: the engine's is its stack size.
pub const MAX_DEPTH: usize = 256;

/// A pooled value object.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ObjId(u32);

/// The shared null, `gpl_value_of_type(0)`: one object, never released.
pub const SHARED_NULL: ObjId = ObjId(0);

/// A value slot: a record's, or one the VM made for a run's arguments.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Handle {
    /// Handle `at` of definition `def`: three per record, its own value then
    /// its two literal operands.
    Slot { def: u32, at: u32 },
    /// A run's return slot or argument copy.
    Top(u32),
}

/// Which of a trace line's three values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Lhs,
    Rhs,
    Result,
}

/// A record's three handles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
enum Part {
    /// Its own value.
    Own = 0,
    /// Its first operand, when a literal.
    Lhs = 1,
    /// Its second operand, when a literal.
    Rhs = 2,
}

const PARTS: u32 = 3;

const fn part(record: u32, p: Part) -> u32 {
    record * PARTS + p as u32
}

/// `n` as a 32-bit index: a record, an object or a run's handle. A
/// definition's record count is a word in its `.bcd`, and decoding refuses
/// more; objects and handles exhaust memory first.
fn index(n: usize) -> u32 {
    u32::try_from(n).expect("a 32-bit index")
}

/// Where a record's three pointers lead, as handle indices in its function.
#[derive(Clone, Copy, Debug)]
struct Link {
    result: u32,
    lhs: Option<u32>,
    rhs: Option<u32>,
}

/// A `foreach` cursor, the record's `head` word: a `std::list` iterator.
///
/// It holds the next node to visit, so erasing or appending elsewhere does
/// not move it. The end is the head sentinel, never freed: the last pass and
/// `break` leave it there, and an element appended then is not visited.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Cursor {
    /// Not allocated.
    Free,
    /// At a node of a list: the checked iterator's container and node, so
    /// a cursor left by a throw walks on in its own list.
    At { list: ObjId, node: ops::Node },
    /// At the end sentinel.
    End,
}

/// One function's values: its records' handles and cursors, made on first
/// call and kept.
#[derive(Clone, Debug)]
struct Activation {
    handles: Vec<Option<ObjId>>,
    links: Vec<Link>,
    cursors: Vec<Cursor>,
}

/// A call on the GPL depth stack: pushed by
/// gpl call hook for each `opLParen` and for a thread's or dispatch's
/// own function.
#[derive(Clone, Debug)]
struct Frame {
    callee: u32,
    /// Shared with the running call: one allocation a call.
    args: Arc<[Handle]>,
}

/// What a function handle names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Callable {
    /// gpl null record: calls `NullFunction`'s function.
    Null,
    Builtin(BuiltinId),
    /// Index into [`Program::definitions`]: a function, or a constant.
    Definition(u32),
}

/// First function handle of a definition: after the null record and the
/// builtins.
#[allow(clippy::cast_possible_truncation, reason = "166 builtins")]
const FIRST_DEFINITION: u32 = 1 + builtins::HD.len() as u32;

/// The first [`BuiltinId`] of openregent's own builtins
/// ([`builtins::OWN`]).
#[allow(clippy::cast_possible_truncation, reason = "166 builtins")]
const OWN_FROM: u32 = builtins::HD.len() as u32;

/// First function handle of an own builtin: past any definition's, so
/// the engine's handles stay as they were.
const FIRST_OWN: u32 = 0x8000_0000;

/// A `GplFunction`'s thread fields: what the thread builtins and the
/// scheduler write (`docs/gpl/runtime.md`, "Threads and the scheduler").
/// A value without them is idle with the one-integer frame.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Thread {
    /// The call frame's text, gpl frame text: return slot, then the
    /// arguments.
    pub frame: String,
    /// Ms.
    pub interval: i32,
    pub mode: ThreadMode,
    /// Bit 7 of the mode word.
    pub suspended: bool,
    /// Ms: what `SuspendThread` left of the interval.
    pub left: i32,
    /// Byte.
    pub running: bool,
    /// The function to move in after the run.
    pub pending: Option<u32>,
    /// Game time of the last run, or of the start.
    pub last: u32,
    /// The frame's arguments, copies made at the start: what a run passes.
    pub args: Vec<Value>,
}

impl Thread {
    /// Scheduled or running: `GplFunction` and method then assign to
    /// the pending slot.
    #[must_use]
    pub fn busy(&self) -> bool {
        self.mode != ThreadMode::Off || self.running
    }
}

/// A run's outcome, when the engine survives it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Run {
    /// The return slot: the declared type's value; untyped, a dispatch's
    /// integer 0 or a thread's shared null.
    pub result: Value,
    /// The exception the catcher caught.
    pub error: Option<GplException>,
    /// The catcher's log text, with the call sequence.
    pub report: Option<String>,
}

/// A trace sink: one line per executed record. `Send` and `Sync`, so a
/// game with one is too: a Bevy resource, or on a thread of its own.
pub type Trace<'p> = Box<dyn FnMut(&str) + Send + Sync + 'p>;

/// What a [`Trace`] sink shares with the code that reads it back: a
/// `Send` and `Sync` stand-in for `Rc<RefCell<T>>`.
pub struct Shared<T>(Arc<Mutex<T>>);

impl<T> Shared<T> {
    pub fn new(value: T) -> Shared<T> {
        Shared(Arc::new(Mutex::new(value)))
    }

    /// The value, held until the guard drops. A sink that panicked
    /// leaves it as it was.
    pub fn borrow_mut(&self) -> MutexGuard<'_, T> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The same as [`Shared::borrow_mut`]: a reader's name for it.
    pub fn borrow(&self) -> MutexGuard<'_, T> {
        self.borrow_mut()
    }
}

impl<T> Clone for Shared<T> {
    fn clone(&self) -> Shared<T> {
        Shared(Arc::clone(&self.0))
    }
}

/// The VM.
pub struct Vm<'p> {
    program: &'p Program,
    config: Config,
    objects: Vec<Value>,
    /// Thread fields of the function objects that have them.
    threads: Map<ObjId, Thread>,
    free: Vec<u32>,
    activations: Vec<Option<Activation>>,
    top: Vec<Option<ObjId>>,
    frames: Vec<Frame>,
    depth: usize,
    /// gpl error line: set per record under [`Checks::Debug`], -1
    /// after a function; never written in release play, so 0 there.
    error_line: i32,
    /// gpl instr count: every executed record. A dispatch restores it.
    count: u64,
    /// The GPL RNG's state, word `0x24` of gpl ai.
    pub rng: u32,
    /// Records one outermost run may execute before [`Crash::Unending`];
    /// `None` for no bound, as the engine has.
    limit: Option<u64>,
    /// Records the outermost run has executed.
    steps: u64,
    /// Where the last crash unwound, innermost first ([`Vm::unwound`]).
    unwound: Vec<Site<'p>>,
    /// The most records an outermost run executed, and its function: how
    /// near the bound ([`Vm::set_limit`]) play comes.
    longest: Option<(u64, &'p str)>,
    trace: Option<Trace<'p>>,
    /// Each object's value before a change, while a run goes aside
    /// ([`Vm::keep`]).
    journal: Option<Journal>,
}

/// The objects as a run aside found them, for [`Vm::put_back`]: their
/// count, and each value before a change, in order. A copy of every
/// object cost the panels' runs (`Game::aside`) far more than the run.
#[derive(Debug, Default)]
struct Journal {
    len: usize,
    before: Vec<(usize, Value)>,
}

impl Journal {
    /// Object `i`'s value, about to change. One made since needs none:
    /// the count drops it.
    fn note(&mut self, i: usize, objects: &[Value]) {
        if i < self.len {
            self.before.push((i, objects[i].clone()));
        }
    }
}

/// Set, a run aside also copies every object and checks the journal's
/// put-back against the copy ([`Vm::put_back`]): a debugging aid for a
/// change that writes objects some other way.
/// `MAJESTY_CHECK_ASIDE=1 cargo nextest run`.
pub static CHECK_ASIDE: Switch = Switch::new("MAJESTY_CHECK_ASIDE");

/// A VM's state as [`Vm::keep`] took it.
pub struct Kept<'p> {
    /// Every object, with [`CHECK_ASIDE`] set: what the journal must
    /// put back.
    objects: Option<Vec<Value>>,
    threads: Map<ObjId, Thread>,
    free: Vec<u32>,
    activations: Vec<Option<Activation>>,
    top: Vec<Option<ObjId>>,
    frames: Vec<Frame>,
    depth: usize,
    error_line: i32,
    count: u64,
    rng: u32,
    steps: u64,
    unwound: Vec<Site<'p>>,
    longest: Option<(u64, &'p str)>,
}

impl<'p> Vm<'p> {
    #[must_use]
    pub fn new(program: &'p Program, config: Config) -> Vm<'p> {
        Vm {
            program,
            config,
            objects: vec![Value::Null],
            threads: Map::default(),
            free: Vec::new(),
            activations: vec![None; program.definitions().len()],
            top: Vec::new(),
            frames: Vec::new(),
            depth: 0,
            error_line: 0,
            count: 0,
            rng: 0,
            limit: None,
            steps: 0,
            unwound: Vec::new(),
            longest: None,
            trace: None,
            journal: None,
        }
    }

    /// Gpl instr count: every executed record. A dispatch restores
    /// it.
    #[must_use]
    pub const fn count(&self) -> u64 {
        self.count
    }

    /// Bound each outermost run at `limit` records; `None` for no bound, as
    /// the engine has.
    pub const fn set_limit(&mut self, limit: Option<u64>) {
        self.limit = limit;
    }

    /// The most records an outermost run executed, and its function: how
    /// near the bound ([`Vm::set_limit`]) play comes.
    #[must_use]
    pub const fn longest(&self) -> Option<(u64, &'p str)> {
        self.longest
    }

    /// Print floats as `text`'s runtime does ([`Config::float_text`]).
    pub const fn set_float_text(&mut self, text: text::FloatText) {
        self.config.float_text = text;
    }

    #[must_use]
    pub const fn program(&self) -> &'p Program {
        self.program
    }

    #[must_use]
    pub const fn config(&self) -> Config {
        self.config
    }

    /// The functions the last [`Crash`] unwound, innermost first, each at
    /// its last source line: the GPL call stack where it stopped. Not the
    /// engine's; it reports nothing on a crash.
    #[must_use]
    pub fn unwound(&self) -> &[Site<'p>] {
        &self.unwound
    }

    /// The innermost GPL function running: a builtin's caller.
    #[must_use]
    pub fn caller(&self) -> Option<&'p str> {
        self.frames
            .iter()
            .rev()
            .filter(|f| matches!(Self::callable(f.callee), Callable::Definition(_)))
            .find_map(|f| self.function_name(f.callee))
    }

    /// A named parameter in the innermost active call of `function`.
    #[must_use]
    pub fn active_arg(&self, function: &str, parameter: &str) -> Option<&Value> {
        let frame = self.frames.iter().rev().find(|frame| {
            self.function_name(frame.callee)
                .is_some_and(|name| name.eq_ignore_ascii_case(function))
        })?;
        let Callable::Definition(def) = Self::callable(frame.callee) else {
            return None;
        };
        let signature = self.program.definitions().get(def as usize)?.signature()?;
        let at = signature.params.iter().position(|param| {
            self.program
                .names()
                .get(param.name)
                .eq_ignore_ascii_case(parameter)
        })?;

        self.value(*frame.args.get(at + 1)?)
    }

    /// Print every executed record to `sink`, as the engine does with
    /// engine flags `0x2000` and `0x200`.
    pub fn set_trace(&mut self, sink: Option<Trace<'p>>) {
        self.trace = sink;
    }

    /// The trace sink, taken: an interface run prints nothing.
    pub fn take_trace(&mut self) -> Option<Trace<'p>> {
        self.trace.take()
    }

    /// Every value a run may change, kept for [`Vm::put_back`]: objects
    /// (by a journal from now on), free list, activations, threads, RNG
    /// and counters. Not the program, config, bound or trace sink.
    ///
    /// # Panics
    ///
    /// Inside another run aside: one journal at a time.
    #[must_use]
    pub fn keep(&mut self) -> Kept<'p> {
        let Vm {
            program: _,
            config: _,
            objects,
            threads,
            free,
            activations,
            top,
            frames,
            depth,
            error_line,
            count,
            rng,
            limit: _,
            steps,
            unwound,
            longest,
            trace: _,
            journal,
        } = self;

        assert!(journal.is_none(), "a run aside inside another");

        *journal = Some(Journal {
            len: objects.len(),
            before: Vec::new(),
        });

        Kept {
            objects: CHECK_ASIDE.on().then(|| objects.clone()),
            threads: threads.clone(),
            free: free.clone(),
            activations: activations.clone(),
            top: top.clone(),
            frames: frames.clone(),
            depth: *depth,
            error_line: *error_line,
            count: *count,
            rng: *rng,
            steps: *steps,
            unwound: unwound.clone(),
            longest: *longest,
        }
    }

    /// The VM as [`Vm::keep`] kept it: what ran since leaves no trace.
    /// The objects made since dropped, each changed one's first value
    /// put back.
    ///
    /// # Panics
    ///
    /// With [`CHECK_ASIDE`] set, objects the journal did not put back.
    pub fn put_back(&mut self, kept: Kept<'p>) {
        let Kept {
            objects,
            threads,
            free,
            activations,
            top,
            frames,
            depth,
            error_line,
            count,
            rng,
            steps,
            unwound,
            longest,
        } = kept;

        // Latest first, so an object changed twice ends at its first.
        if let Some(Journal { len, before }) = self.journal.take() {
            self.objects.truncate(len);

            for (i, value) in before.into_iter().rev() {
                self.objects[i] = value;
            }
        }

        if let Some(objects) = objects {
            assert!(
                self.objects == objects,
                "the journal's objects differ from the copy"
            );
        }

        self.threads = threads;
        self.free = free;
        self.activations = activations;
        self.top = top;
        self.frames = frames;
        self.depth = depth;
        self.error_line = error_line;
        self.count = count;
        self.rng = rng;
        self.steps = steps;
        self.unwound = unwound;
        self.longest = longest;
    }

    /// Gpl rng seed: both seed and state. The engine takes the clock
    /// for 0; here 0 is 0.
    pub const fn seed(&mut self, seed: u32) {
        self.rng = seed;
    }

    /// Gpl rng next: rng next's formula on the GPL state; a
    /// draw below `bound & 0x7fffffff`, 0 when that is under 2.
    ///
    /// ```
    /// use regent_core::gpl::{load::Program, vm::{Config, Vm}};
    ///
    /// let program = Program::load::<&[u8]>(&[]).unwrap();
    /// let mut vm = Vm::new(&program, Config::default());
    ///
    /// vm.seed(1);
    /// assert_eq!(vm.draw(100), 83);
    /// assert_eq!(vm.rng, 0x0afa_cbff);
    /// ```
    pub const fn draw(&mut self, bound: i32) -> i32 {
        draw(&mut self.rng, bound)
    }

    // -----------------------------------------------------------------------
    // Objects and handles

    /// A new object holding `v`.
    ///
    /// # Panics
    ///
    /// Past 2^32 objects, which memory does not hold.
    pub fn alloc(&mut self, v: Value) -> ObjId {
        if let Some(at) = self.free.pop() {
            if let Some(journal) = self.journal.as_mut() {
                journal.note(at as usize, &self.objects);
            }

            self.objects[at as usize] = v;
            ObjId(at)
        } else {
            let at = index(self.objects.len());

            self.objects.push(v);
            ObjId(at)
        }
    }

    /// Gpl value of type: a fresh object of a type code; 0 the shared
    /// null. `None` for a code with no class.
    pub fn of_type(&mut self, code: TypeCode) -> Option<ObjId> {
        match Kind::try_from(code) {
            Ok(Kind::Null) => Some(SHARED_NULL),
            Ok(kind) => Some(self.alloc(Value::fresh(kind))),
            Err(_) => None,
        }
    }

    /// Gpl value release: back to the pool. The shared null stays.
    pub fn release(&mut self, o: ObjId) {
        if o != SHARED_NULL {
            self.threads.remove(&o);
            self.free.push(o.0);
        }
    }

    /// `GplType`'s reset on an object: `ops::reset`, and a function's thread
    /// fields cleared.
    pub fn reset(&mut self, o: ObjId) {
        self.threads.remove(&o);
        ops::reset(self.object_mut(o));
    }

    /// Give function object `o` thread fields, or none.
    pub fn set_thread(&mut self, o: ObjId, t: Option<Thread>) {
        match t {
            Some(t) => self.threads.insert(o, t),
            None => self.threads.remove(&o),
        };
    }

    #[must_use]
    pub fn thread(&self, o: ObjId) -> Option<&Thread> {
        self.threads.get(&o)
    }

    #[must_use]
    pub fn object(&self, o: ObjId) -> &Value {
        &self.objects[o.0 as usize]
    }

    /// `ops::assign` of object `from` to object `to`. `from` is read where
    /// it lies, not copied first: a list's copy would take its freed nodes,
    /// which an assignment drops.
    fn assign(&mut self, to: ObjId, from: ObjId) -> Result<(), OpError> {
        let mut dest = std::mem::replace(self.object_mut(to), Value::Null);
        let out = if to == from {
            let v = dest.clone();

            ops::assign(&mut dest, &v)
        } else {
            ops::assign(&mut dest, self.object(from))
        };

        self.objects[to.0 as usize] = dest;
        out
    }

    pub fn object_mut(&mut self, o: ObjId) -> &mut Value {
        let i = o.0 as usize;

        if let Some(journal) = self.journal.as_mut() {
            journal.note(i, &self.objects);
        }

        &mut self.objects[i]
    }

    /// The object `h` holds.
    #[must_use]
    pub fn get(&self, h: Handle) -> Option<ObjId> {
        match h {
            Handle::Slot { def, at } => self.activations[def as usize]
                .as_ref()
                .and_then(|a| a.handles[at as usize]),
            Handle::Top(at) => self.top[at as usize],
        }
    }

    /// Put `o` in `h`.
    pub fn set(&mut self, h: Handle, o: Option<ObjId>) {
        let slot = match h {
            Handle::Slot { def, at } => self.activations[def as usize]
                .as_mut()
                .map(|a| &mut a.handles[at as usize]),
            Handle::Top(at) => Some(&mut self.top[at as usize]),
        };

        if let Some(slot) = slot {
            *slot = o;
        }
    }

    /// The value `h` holds.
    #[must_use]
    pub fn value(&self, h: Handle) -> Option<&Value> {
        self.get(h).map(|o| self.object(o))
    }

    /// The objects record `at` of `def` reads: its two operands, and for
    /// `opLocalize` the argument it copies. Empty before `def` first runs.
    #[must_use]
    pub fn reads(&self, def: u32, at: usize) -> Vec<(Side, ObjId)> {
        let instr = &self.program.definitions()[def as usize].code[at];
        let arg = match (instr.op, instr.head) {
            (Op::Localize, Head::Param(p)) => self
                .frames
                .last()
                .and_then(|f| f.args.get(p as usize))
                .and_then(|h| self.get(*h)),
            _ => None,
        };

        self.activations[def as usize]
            .as_ref()
            .map_or_else(Vec::new, |act| {
                let link = act.links[at];
                let own = |i: Option<u32>| i.and_then(|i| act.handles[i as usize]);

                [
                    (Side::Lhs, own(link.lhs)),
                    (Side::Rhs, own(link.rhs)),
                    (Side::Result, arg),
                ]
                .into_iter()
                .filter_map(|(side, o)| o.map(|o| (side, o)))
                .collect()
            })
    }

    /// The object record `at` of `def` prints as its result.
    #[must_use]
    pub fn result(&self, def: u32, at: usize) -> Option<ObjId> {
        self.activations[def as usize]
            .as_ref()
            .and_then(|act| act.handles[act.links[at].result as usize])
    }

    /// Entries on the GPL depth stack: a trace line's tabs.
    #[must_use]
    pub const fn depth(&self) -> usize {
        self.frames.len()
    }

    /// Argument `i` of a call, as gpl arg fetches it.
    ///
    /// # Errors
    ///
    /// [`Crash::ArgPastEnd`] past the vector; [`Crash::NullValue`] on an
    /// empty handle, which the accessor would dereference.
    pub fn arg(&self, args: &[Handle], i: usize) -> Result<ObjId, Fault> {
        let h = args.get(i).ok_or(Crash::ArgPastEnd)?;

        Ok(self.get(*h).ok_or(Crash::NullValue(Null::Argument))?)
    }

    /// The value of parameter or local `name` of function `def`, as its one
    /// activation holds it now; `None` before its first call.
    #[must_use]
    pub fn local(&self, def: u32, name: &str) -> Option<&Value> {
        let definition = self.program.definitions().get(def as usize)?;
        let signature = definition.signature()?;
        let local = signature
            .params
            .iter()
            .chain(&signature.locals)
            .find(|l| self.program.names().get(l.name).eq_ignore_ascii_case(name))?;
        let link = self.activations[def as usize].as_ref()?.links[local.at as usize];

        self.value(Handle::Slot {
            def,
            at: link.result,
        })
    }

    // -----------------------------------------------------------------------
    // Function handles

    /// The function handle the map gives `name`: the null record when absent.
    #[must_use]
    pub fn function(&self, name: &str) -> u32 {
        Self::named_handle(self.program, name).unwrap_or(ops::NULL_FUNCTION)
    }

    /// The handle `program`'s map gives `name`:
    /// a builtin's, the empty name's too, or a script function's or
    /// constant's. `None` when absent.
    #[must_use]
    pub fn named_handle(program: &Program, name: &str) -> Option<u32> {
        match program.entry(name)? {
            Entry::Builtin(b) => Some(Self::builtin_handle(b)),
            Entry::Script(d) | Entry::Constant(d) => Some(FIRST_DEFINITION + d),
        }
    }

    /// The function handle of builtin `b`: one past its index for the
    /// engine's, from `0x80000000` for openregent's own.
    #[must_use]
    pub fn builtin_handle(b: BuiltinId) -> u32 {
        if b.own() {
            FIRST_OWN + (u32::from(b) - OWN_FROM)
        } else {
            1 + u32::from(b)
        }
    }

    /// The function handle of definition `def`.
    #[must_use]
    pub const fn definition_handle(def: u32) -> u32 {
        FIRST_DEFINITION + def
    }

    /// What a function handle names.
    #[must_use]
    pub fn callable(handle: u32) -> Callable {
        let own = handle
            .checked_sub(FIRST_OWN)
            .filter(|k| (*k as usize) < builtins::OWN.len())
            .and_then(|k| u16::try_from(k + OWN_FROM).ok());

        match (own, handle.checked_sub(1)) {
            (Some(b), _) => Callable::Builtin(BuiltinId(b)),
            (None, None) => Callable::Null,
            (None, Some(b)) if b < FIRST_DEFINITION - 1 => {
                Callable::Builtin(BuiltinId(u16::try_from(b).unwrap_or_default()))
            }
            (None, Some(_)) => Callable::Definition(handle - FIRST_DEFINITION),
        }
    }

    /// A function record's name; `None` for the null record.
    #[must_use]
    pub fn function_name(&self, handle: u32) -> Option<&'p str> {
        match Self::callable(handle) {
            Callable::Null => None,
            Callable::Builtin(b) => Some(b.get().name),
            Callable::Definition(d) => self
                .program
                .definitions()
                .get(d as usize)
                .map(|def| self.program.names().get(def.name)),
        }
    }

    /// Gpl function return type by the record's name: a builtin's
    /// registered type, a script function's `is` type, a constant's
    /// [`CONSTANT_RETURNS`]. Gpl register script registers its scope
    /// entry's type; **Inferred** that a function's equals its
    /// prologue's.
    fn return_type(&self, handle: u32) -> Option<TypeCode> {
        match Self::callable(handle) {
            Callable::Null => Some(builtins::NOTHING),
            Callable::Builtin(b) => Some(b.get().returns),
            Callable::Definition(d) => self.program.definitions().get(d as usize).and_then(|def| {
                def.signature()
                    .map(|s| s.returns)
                    .or(matches!(def.kind, exec::Kind::Constant(_)).then_some(CONSTANT_RETURNS))
            }),
        }
    }

    // -----------------------------------------------------------------------
    // Text

    /// A value's text, `GplType`'s text call.
    pub fn text(&self, host: &mut dyn Host, v: &Value) -> String {
        text::value(
            v,
            &mut Namer {
                vm: self,
                host: Some(host),
            },
        )
    }

    /// An object's text, its thread fields included.
    pub fn object_text(&self, host: &mut dyn Host, o: ObjId) -> String {
        match (self.object(o), self.threads.get(&o)) {
            (Value::Function(f), Some(t)) => {
                let name = |f: u32| self.function_name(f).unwrap_or(text::NULL_RECORD_TEXT);

                // The frame prints as it is now: an agent's name follows its
                // piece. **Game**: a palace thread started on `Palace1#6`
                // prints `Palace2#6` after the upgrade.
                let live = (!t.args.is_empty()).then(|| {
                    let texts: Vec<String> = std::iter::once(&Value::Null)
                        .chain(&t.args)
                        .map(|v| self.text(host, v))
                        .collect();

                    Thread {
                        frame: texts.join(text::SEPARATOR),
                        ..t.clone()
                    }
                });

                text::thread(name(*f), live.as_ref().unwrap_or(t), t.pending.map(name))
            }
            (v, _) => self.text(host, v),
        }
    }

    /// A handle's text, empty when it holds nothing, as the trace prints it.
    fn handle_text(&self, host: &mut dyn Host, h: Option<Handle>) -> String {
        h.and_then(|h| self.get(h))
            .map(|o| self.object_text(host, o))
            .unwrap_or_default()
    }

    // -----------------------------------------------------------------------
    // Runs

    /// Run function `def` with copies of `args`, as a thread or a dispatch
    /// runs it: gpl call hook pushes its frame, as for a call, and
    /// gpl callstack pop pops it. The catcher takes a `GplException`,
    /// logs its report through [`Host::log`] and empties the whole call
    /// stack, outer runs' frames included. A [`Catcher::Dispatch`] restores
    /// [`Vm::count`].
    ///
    /// # Errors
    ///
    /// A [`Crash`], which nothing in the engine catches.
    pub fn run(
        &mut self,
        host: &mut dyn Host,
        def: u32,
        args: &[Value],
        catcher: Catcher,
    ) -> Result<Run, Crash> {
        self.run_handle(host, Self::definition_handle(def), args, catcher)
    }

    /// [`Vm::run`] for any function handle: a thread may hold a builtin, as
    /// the towers' `RevenueScript` holds the empty name's.
    ///
    /// # Errors
    ///
    /// A [`Crash`], which nothing in the engine catches.
    pub fn run_handle(
        &mut self,
        host: &mut dyn Host,
        callee: u32,
        args: &[Value],
        catcher: Catcher,
    ) -> Result<Run, Crash> {
        let saved = self.count;

        if self.depth == 0 {
            self.steps = 0;
            self.unwound.clear();
        }

        // A dispatch's return slot is its frame's integer 0 whatever the
        // function's type; a
        // thread's is empty. The Freestyle trace prints `declare return …
        // result: 0` for an untyped dispatch, `result: ()` for a thread.
        let slot = match catcher {
            Catcher::Dispatch => Some(self.alloc(Value::Integer(0))),
            Catcher::Thread => None,
        };
        let ret = self.top_handle(slot);
        let mut handles = vec![ret];

        for a in args {
            let o = self.alloc(a.clone());
            handles.push(self.top_handle(Some(o)));
        }

        self.frames.push(Frame {
            callee,
            args: handles.as_slice().into(),
        });

        let outcome = self.invoke(host, callee, &handles);

        if outcome.is_ok() {
            self.frames.pop();
        }

        if catcher == Catcher::Dispatch {
            self.count = saved;
        }

        let result = self.value(ret).cloned().unwrap_or(Value::Null);

        // The report reads the frames, this run's handles among them.
        let finished = match outcome {
            Ok(()) => Ok(Run {
                result,
                error: None,
                report: None,
            }),
            Err(Fault::Exception(e)) => {
                let report = self.report(host, catcher, &e);

                host.log(&report);
                self.frames.clear();

                Ok(Run {
                    result,
                    error: Some(e),
                    report: Some(report),
                })
            }
            Err(Fault::Crash(c)) => {
                self.frames.clear();
                Err(c)
            }
        };

        // The return object goes back to the pool, yet the callee's
        // `__Return_value__` handle (`Op::Reference`) still names it, and a
        // later object may take its id: so the engine's frame does
        // (gpl frame args release; **Inferred**: nothing clears the
        // handle). The next call's `opReference` rebinds it before any read;
        // a save keeps the stale id as it stands.
        self.drop_top(&handles);

        if self.depth == 0 && self.longest.is_none_or(|(most, _)| self.steps > most) {
            self.longest = self.function_name(callee).map(|name| (self.steps, name));
        }

        finished
    }

    /// Gpl dispatch: run a script by name at once, free of budget.
    ///
    /// # Errors
    ///
    /// A [`Crash`]; a name that is no script function is
    /// [`Crash::NotCallable`].
    pub fn dispatch(
        &mut self,
        host: &mut dyn Host,
        name: &str,
        args: &[Value],
    ) -> Result<Run, Crash> {
        match self.program.entry(name) {
            Some(Entry::Script(def)) => self.run(host, def, args, Catcher::Dispatch),
            _ => Err(Crash::NotCallable),
        }
    }

    pub(crate) fn top_handle(&mut self, o: Option<ObjId>) -> Handle {
        let at = index(self.top.len());

        self.top.push(o);
        Handle::Top(at)
    }

    /// Release a run's own handles; they are the last made.
    fn drop_top(&mut self, handles: &[Handle]) {
        for h in handles.iter().rev() {
            if let Handle::Top(at) = *h {
                if let Some(o) = self.top[at as usize] {
                    self.release(o);
                }

                self.top.truncate(at as usize);
            }
        }
    }

    /// The catcher's log text. It walks gpl debug stack, which
    /// gpl call hook fills only under the debug flag, so release play
    /// lists no frame.
    fn report(&self, host: &mut dyn Host, catcher: Catcher, e: &GplException) -> String {
        let head = match catcher {
            Catcher::Thread => text::THREAD_ERROR,
            Catcher::Dispatch => text::DISPATCH_ERROR,
        };

        let frames: Vec<String> = match self.config.checks {
            Checks::Debug => self
                .frames
                .iter()
                .map(|f| self.frame_text(host, f))
                .collect(),
            Checks::Release => Vec::new(),
        };

        text::report(head, &frames, self.error_line, &e.message)
    }

    fn frame_text(&self, host: &mut dyn Host, f: &Frame) -> String {
        let args: Vec<String> = f
            .args
            .iter()
            .map(|h| {
                self.value(*h)
                    .map_or_else(|| text::NULL_HANDLE_TEXT.to_owned(), |v| self.text(host, v))
            })
            .collect();

        text::frame(self.function_name(f.callee), &args)
    }

    /// Gpl function invoke: run a function record.
    fn invoke(&mut self, host: &mut dyn Host, callee: u32, args: &[Handle]) -> Result<(), Fault> {
        match Self::callable(callee) {
            Callable::Null => Ok(()),
            Callable::Builtin(b) if native::runs(b) => native::run(self, host, b, args),
            Callable::Builtin(b) => host.builtin(self, &Call { builtin: b, args }),
            Callable::Definition(d) => match self
                .program
                .definitions()
                .get(d as usize)
                .and_then(Definition::signature)
            {
                Some(_) if self.depth >= MAX_DEPTH => Err(Crash::StackOverflow.into()),
                Some(_) => {
                    self.depth += 1;
                    let out = self.execute(host, d, args);
                    self.depth -= 1;

                    // A run the host stopped is one the engine finished:
                    // its loops are left, not held mid-list.
                    if matches!(out, Err(Fault::Crash(Crash::Unending))) {
                        self.act_mut(d).cursors.fill(Cursor::Free);
                    }

                    out
                }
                None => Err(Crash::NotCallable.into()),
            },
        }
    }

    /// Make `def`'s activation if it has none.
    fn activate(&mut self, host: &mut dyn Host, def: u32) {
        if self.activations[def as usize].is_none() {
            let definition = &self.program.definitions()[def as usize];
            let records = index(definition.code.len());
            let mut handles = Vec::with_capacity(definition.code.len() * PARTS as usize);

            for instr in &definition.code {
                let own = match instr.dest {
                    Dest::Value(lit) => Some(self.literal(lit)),
                    Dest::None | Dest::Alias => None,
                };
                let lhs = self.operand(host, instr.lhs);
                let rhs = self.operand(host, instr.rhs);

                handles.extend([own, lhs, rhs]);
            }

            let links = (0..records).map(|at| link(definition, at)).collect();

            self.activations[def as usize] = Some(Activation {
                handles,
                links,
                cursors: vec![Cursor::Free; definition.code.len()],
            });
        }
    }

    /// A literal operand's object, made at load.
    fn operand(&mut self, host: &mut dyn Host, operand: Operand) -> Option<ObjId> {
        match operand {
            Operand::Value(lit) => Some(self.literal(lit)),
            Operand::Constant(name) => {
                let v = host.named(self.program.names().get(name));
                Some(self.alloc(v))
            }
            Operand::None | Operand::Slot(_) => None,
        }
    }

    /// A literal's object: the shared null for type 0, else a fresh one.
    fn literal(&mut self, lit: Literal) -> ObjId {
        let v = match lit {
            Literal::Null => None,
            Literal::Int(i) => Some(Value::Integer(i)),
            Literal::Float(f) => Some(Value::Float(f)),
            Literal::Str(s) => Some(Value::String(self.program.names().get(s).to_owned())),
            Literal::List => Some(Value::List(ops::List::new())),
            Literal::Agent => Some(Value::Agent(ops::NULL_AGENT)),
            Literal::Bool(b) => Some(Value::Boolean(b)),
            Literal::Coord => Some(Value::Coord(0, 0)),
            Literal::Function(name) => Some(Value::Function(
                self.function(self.program.names().get(name)),
            )),
        };

        v.map_or(SHARED_NULL, |v| self.alloc(v))
    }

    fn act(&self, def: u32) -> &Activation {
        self.activations[def as usize]
            .as_ref()
            .expect("activated before it runs")
    }

    fn act_mut(&mut self, def: u32) -> &mut Activation {
        self.activations[def as usize]
            .as_mut()
            .expect("activated before it runs")
    }

    /// The object in handle `at` of `def`.
    fn obj(&self, def: u32, at: Option<u32>) -> Option<ObjId> {
        at.and_then(|at| self.act(def).handles[at as usize])
    }

    /// Gpl execute: one function, from record 0.
    fn execute(&mut self, host: &mut dyn Host, def: u32, args: &[Handle]) -> Result<(), Fault> {
        self.activate(host, def);

        let definition: &'p Definition = &self.program.definitions()[def as usize];
        let debug = self.config.checks == Checks::Debug;
        let name = self.program.names().get(definition.name);

        // The name in messages is filled only under the debug flag.
        let named = if debug { name } else { "" };

        let mut run = Running {
            def,
            args,
            named,
            line: -1,
            params: 0,
        };
        let mut at: usize = 0;

        while at < definition.code.len() {
            self.count += 1;
            self.steps += 1;

            if self.limit.is_some_and(|most| self.steps > most) {
                self.unwind(def, at, run.line);
                return Err(Crash::Unending.into());
            }

            let instr = &definition.code[at];
            let line = instr.line.map_or(-1, |l| i32::from(l.cast_signed()));

            if line > 0 {
                run.line = line;

                if debug {
                    self.error_line = line;
                }
            }

            let next = self.step(host, definition, &mut run, at).inspect_err(|f| {
                if matches!(f, Fault::Crash(_)) {
                    self.unwind(def, at, run.line);
                }
            })?;

            if self.trace.is_some() {
                self.trace_record(host, def, at, name, run.line);
            }

            at = next.wrapping_add(1);
        }

        if debug {
            self.error_line = -1;
        }

        self.exit_walk(definition, def);
        Ok(())
    }

    /// Note `def` at record `at`, line `line`, as a crash leaves it.
    fn unwind(&mut self, def: u32, at: usize, line: i32) {
        let name = self.program.definitions()[def as usize].name;

        self.unwound.push(Site {
            function: self.program.names().get(name),
            record: at,
            line,
        });
    }

    /// The exit walk: from record 3 to the first `opDeclare`, then reset
    /// every `opDeclare`'s value up to opcode 39.
    fn exit_walk(&mut self, definition: &Definition, def: u32) {
        let locals: Vec<u32> = definition
            .code
            .iter()
            .zip(0u32..)
            .skip(EXIT_WALK_FROM as usize)
            .take_while(|(i, _)| i.op != Op::Body)
            .filter(|(i, _)| i.op == Op::Declare)
            .map(|(_, at)| at)
            .collect();

        for at in locals {
            let result = self.act(def).links[at as usize].result;

            if let Some(o) = self.obj(def, Some(result)) {
                self.reset(o);
            }
        }
    }

    fn trace_record(&mut self, host: &mut dyn Host, def: u32, at: usize, name: &str, line: i32) {
        let instr = &self.program.definitions()[def as usize].code[at];
        let wiring = self.act(def).links[at];
        let h = |i: Option<u32>| i.map(|at| Handle::Slot { def, at });

        let lhs = self.handle_text(host, h(wiring.lhs));
        let rhs = self.handle_text(host, h(wiring.rhs));
        let result = self.handle_text(host, h(Some(wiring.result)));
        let body = text::trace_body(instr.op, &lhs, &rhs, &result);
        let out = text::trace_line(self.frames.len(), name, line, &body);

        if let Some(sink) = self.trace.as_mut() {
            sink(&out);
        }
    }

    /// Run record `at`; the index the loop then steps from.
    fn step(
        &mut self,
        host: &mut dyn Host,
        definition: &'p Definition,
        run: &mut Running<'_, '_>,
        at: usize,
    ) -> Result<usize, Fault> {
        let instr = &definition.code[at];
        let def = run.def;
        let link = self.act(def).links[at];
        let jump = |head: Head| match head {
            Head::Jump(to) => (to as usize).wrapping_sub(1),
            _ => at,
        };

        match instr.op {
            op if let Ok(operator) = Operator::try_from(op) => {
                self.operate(host, def, at, operator).map(|()| at)
            }
            Op::Localize => self.localize(definition, run, at).map(|()| at),

            // Bind `__Return_value__` to the caller's result object.
            Op::Reference => {
                let caller = run.args[0];
                let declared = self
                    .obj(def, link.lhs)
                    .map_or(TypeCode::Null, |o| self.object(o).kind().type_code());

                if let Some(o) = self.get(caller) {
                    self.reset(o);
                } else {
                    let o = self
                        .of_type(declared)
                        .ok_or(Crash::NullValue(Null::NoClass))?;
                    self.set(caller, Some(o));
                }

                let shared = self.get(caller);
                let own = part(index(at), Part::Own);
                self.act_mut(def).handles[own as usize] = shared;
                Ok(at)
            }

            Op::Declare => {
                if let Some(o) = self.obj(def, Some(link.result)) {
                    self.reset(o);
                }

                Ok(at)
            }

            Op::Advance | Op::Rewind => Ok(jump(instr.head)),

            Op::If => {
                let o = self.obj(def, link.lhs).ok_or_else(|| {
                    Fault::throw(format_name(ReleaseCheck::NullIf.message(), run.named))
                })?;

                Ok(if ops::bool_of(self.object(o))? {
                    at
                } else {
                    jump(instr.head)
                })
            }

            Op::While => {
                let o = self
                    .obj(def, link.lhs)
                    .ok_or(Crash::NullValue(Null::Condition))?;

                Ok(if ops::bool_of(self.object(o))? {
                    at
                } else {
                    jump(instr.head)
                })
            }

            Op::Foreach => self.foreach(def, at, link, run.named),

            // Both name their `opForeach`; the loop steps past it, onto the
            // jump out of the loop. `break` puts the cursor at the end first.
            Op::Continue => Ok(loop_of(instr.head, at)),
            Op::Break => {
                let each = loop_of(instr.head, at);
                self.list_at(def, self.act(def).links[each].rhs)?;

                let cursor = &mut self.act_mut(def).cursors[each];

                if *cursor == Cursor::Free {
                    Err(Crash::NoCursor.into())
                } else {
                    *cursor = Cursor::End;
                    Ok(each)
                }
            }

            Op::Return => Ok(RETURN_INDEX as usize),
            Op::ReturnValue => {
                let value = self
                    .obj(def, link.rhs)
                    .ok_or(Crash::NullValue(Null::Operand))?;
                let caller = self
                    .get(run.args[0])
                    .ok_or(Crash::NullValue(Null::Result))?;
                self.assign(caller, value)?;
                Ok(RETURN_INDEX as usize)
            }

            Op::Call => self.call(host, definition, def, at).map(|()| at),

            Op::GetAttribute | Op::SetAttribute | Op::AdjustAttribute => {
                Err(Crash::InlineAttribute.into())
            }

            // opNop, opExit and 39 do nothing at run time. opExit clears
            // the debug recursion guard, which nothing sets
            // (`runtime::DebugCheck`), so its check is not made.
            _ => Ok(at),
        }
    }

    /// An operator, 0 to 23: typed result slot, then the left operand's class.
    fn operate(
        &mut self,
        host: &mut dyn Host,
        def: u32,
        at: usize,
        operator: Operator,
    ) -> Result<(), Fault> {
        let link = self.act(def).links[at];
        let lhs = self
            .obj(def, link.lhs)
            .ok_or(Crash::NullValue(Null::Operand))?;
        let rhs = self.obj(def, link.rhs);
        let place = Some(link.result) == link.lhs;
        let writes_lhs = operator.writes_lhs();

        let dest_obj = if place {
            Some(lhs)
        } else {
            self.obj(def, Some(link.result))
        };

        // The operands moved out, not cloned, where no other names the
        // same object; every path below writes them back.
        let right_copy = rhs
            .filter(|r| *r == lhs || Some(*r) == dest_obj)
            .map(|r| self.object(r).clone());
        let mut dest = dest_obj.map(|o| {
            if o == lhs {
                // A list written in place never reads its result slot
                // (`ops::list`), and a place's slot is dropped below: no
                // copy.
                match self.object(o) {
                    Value::List(_) if place && writes_lhs => Value::List(ops::List::new()),
                    v => v.clone(),
                }
            } else {
                std::mem::replace(self.object_mut(o), Value::Null)
            }
        });
        let mut left = std::mem::replace(self.object_mut(lhs), Value::Null);

        // A thread compares by its pending function when idle, and takes an
        // assignment there when busy.
        let thread = self
            .threads
            .get(&lhs)
            .map(|t| (t.pending, t.running, t.busy()));
        let original = thread.map(|_| left.clone());

        if let (Some((Some(p), false, _)), Operator::Equal | Operator::NotEqual) =
            (thread, operator)
        {
            left = Value::Function(p);
        }

        let right = right_copy.as_ref().or_else(|| rhs.map(|r| self.object(r)));
        let step = ops::apply(operator, &mut left, right, &mut dest);

        // A throwing assignment wrote nothing: the right value is read first.
        match (thread, operator, &left, original) {
            (Some((_, _, true)), Operator::AssignEqual, Value::Function(f), Some(original))
                if step.is_ok() =>
            {
                let pending = Some(*f);

                if let Some(t) = self.threads.get_mut(&lhs) {
                    t.pending = pending;
                }

                left = original;
            }
            (Some(_), Operator::Equal | Operator::NotEqual, _, Some(original)) => left = original,
            _ => {}
        }

        // `l = l` clears the list it then copies from.
        let self_copy =
            rhs == Some(lhs) && operator == Operator::AssignEqual && left.kind() == Kind::List;

        // A place is its own result: a value operator there writes it.
        if place && !writes_lhs {
            *self.object_mut(lhs) = dest.unwrap_or(left);
        } else {
            *self.object_mut(lhs) = if self_copy {
                Value::List(ops::List::new())
            } else {
                left
            };

            match (dest, dest_obj) {
                (Some(v), Some(o)) if !place => *self.object_mut(o) = v,
                (Some(v), None) => {
                    let o = self.alloc(v);
                    self.act_mut(def).handles[link.result as usize] = Some(o);
                }
                _ => {}
            }
        }

        match step? {
            Step::Done => Ok(()),

            // The result slot holds the attribute's own object.
            Step::Attribute { agent, name } => {
                let o = host.attribute(self, agent, &name)?;
                self.act_mut(def).handles[link.result as usize] = Some(o);
                Ok(())
            }
        }
    }

    /// `opLocalize`: copy argument `head` into the parameter.
    fn localize(
        &mut self,
        definition: &Definition,
        run: &mut Running<'_, '_>,
        at: usize,
    ) -> Result<(), Fault> {
        let def = run.def;
        let debug = self.config.checks == Checks::Debug;
        let link = self.act(def).links[at];

        let position = match definition.code[at].head {
            Head::Param(p) => p as usize,
            _ => 0,
        };

        let arg = self.arg(run.args, position)?;
        let param = self
            .obj(def, Some(link.result))
            .ok_or(Crash::NullValue(Null::Operand))?;

        // `%s` is the record's first value text: the
        // parameter's type slot, empty in every image.
        if debug && self.object(arg).kind() != self.object(param).kind() {
            let text = match definition.code[at].lhs {
                Operand::Value(Literal::Str(s) | Literal::Function(s)) => {
                    self.program.names().get(s)
                }
                _ => "",
            };

            return Err(Fault::throw(format_name(
                DebugCheck::ParameterType.message(),
                text,
            )));
        }

        self.assign(param, arg)?;

        if debug {
            run.params += 1;

            let given = run.args.len() - 1;
            let last = definition
                .code
                .get(at + 1)
                .is_some_and(|next| next.op != Op::Localize);

            if last && run.params != given {
                return Err(Fault::throw(format_name(
                    DebugCheck::TooManyArguments.message(),
                    run.named,
                )));
            }

            if !last && given < run.params {
                return Err(Fault::throw(format_name(
                    DebugCheck::TooFewArguments.message(),
                    run.named,
                )));
            }
        }

        Ok(())
    }

    /// The list a `foreach` or `break` reads: its record's second operand.
    fn list_obj(&self, def: u32, rhs: Option<u32>) -> Result<ObjId, Fault> {
        let o = self.obj(def, rhs).ok_or(Crash::NullValue(Null::List))?;

        match self.object(o) {
            Value::List(_) => Ok(o),
            _ => Err(OpError::BadConversion(ops::Accessor::ListRef).into()),
        }
    }

    /// [`Vm::list_obj`]'s list.
    fn list_at(&self, def: u32, rhs: Option<u32>) -> Result<&ops::List, Fault> {
        match self.object(self.list_obj(def, rhs)?) {
            Value::List(items) => Ok(items),
            _ => Err(OpError::BadConversion(ops::Accessor::ListRef).into()),
        }
    }

    /// What a cursor on `node` of `list` reads: value and next node. One
    /// the list does not know is freed memory the model has not kept.
    fn visited(&self, list: ObjId, node: ops::Node) -> Result<(&Value, Option<ops::Node>), Fault> {
        match self.object(list) {
            Value::List(items) => items.visit(node),
            _ => None,
        }
        .ok_or_else(|| Crash::FreedNode.into())
    }

    /// `opForeach`: the next element into the variable and past the exit
    /// jump, or at the end free the cursor and take the jump.
    ///
    /// The cursor is read as the engine reads it: only the
    /// sentinel ends the loop, so a freed node gives its value as released
    /// and its old link. A cursor left on another list's node walks that
    /// list; at its sentinel the loop ends (**Inferred**: the engine reads
    /// the walked list's sentinel unequal, then that sentinel's unset value).
    fn foreach(&mut self, def: u32, at: usize, link: Link, named: &str) -> Result<usize, Fault> {
        let var = self.obj(def, link.lhs).ok_or_else(|| {
            Fault::throw(format_name(ReleaseCheck::NullIterator.message(), named))
        })?;
        let walked = self.list_obj(def, link.rhs)?;

        let cursor = match self.act(def).cursors[at] {
            Cursor::Free => self
                .list_at(def, link.rhs)?
                .node(0)
                .map(|node| (walked, node)),
            Cursor::At { list, node } => Some((list, node)),
            Cursor::End => None,
        };

        let element = cursor
            .map(|(list, node)| self.visited(list, node).map(|(v, _)| v.clone()))
            .transpose()?;

        if let (Some((list, node)), Some(element)) = (cursor, element) {
            // Assigned, then stepped: a throw leaves the cursor on the node.
            self.act_mut(def).cursors[at] = Cursor::At { list, node };
            ops::assign(self.object_mut(var), &element)?;

            let next = self.visited(list, node)?.1;

            self.act_mut(def).cursors[at] =
                next.map_or(Cursor::End, |node| Cursor::At { list, node });

            Ok(at + 1)
        } else {
            self.act_mut(def).cursors[at] = Cursor::Free;
            Ok(at)
        }
    }

    /// `opLParen`: type or reset the result, push a frame, invoke.
    fn call(
        &mut self,
        host: &mut dyn Host,
        definition: &Definition,
        def: u32,
        at: usize,
    ) -> Result<(), Fault> {
        let link = self.act(def).links[at];
        let own = part(index(at), Part::Own);
        let callee_of = |vm: &Self| -> Result<u32, Fault> {
            let o = vm
                .obj(def, link.lhs)
                .ok_or(Crash::NullValue(Null::Callee))?;

            match vm.object(o) {
                Value::Function(f) => Ok(*f),
                _ => Err(OpError::BadConversion(ops::Accessor::FunctionRef).into()),
            }
        };

        let callee = match self.obj(def, Some(own)) {
            None => {
                let callee = callee_of(self)?;
                let ty = self
                    .return_type(callee)
                    .ok_or(Crash::NullValue(Null::NoClass))?;
                let o = self.of_type(ty).ok_or(Crash::NullValue(Null::NoClass))?;

                self.act_mut(def).handles[own as usize] = Some(o);
                callee
            }
            Some(o) => {
                self.reset(o);
                callee_of(self)?
            }
        };

        let call = definition.code[at]
            .call
            .as_deref()
            .ok_or(Crash::NullValue(Null::Callee))?;
        let links = &self.act(def).links;
        let args: Arc<[Handle]> = std::iter::once(own)
            .chain(call.args.iter().map(|slot| links[*slot as usize].result))
            .map(|at| Handle::Slot { def, at })
            .collect();

        self.frames.push(Frame {
            callee,
            args: Arc::clone(&args),
        });

        self.invoke(host, callee, &args)?;
        self.frames.pop();
        Ok(())
    }
}

/// One function a crash unwound ([`Vm::unwound`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Site<'p> {
    pub function: &'p str,
    /// The record running.
    pub record: usize,
    /// The last source line seen; -1 before one.
    pub line: i32,
}

impl fmt::Display for Site<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} line {}", self.function, self.line)
    }
}

/// What one `execute` keeps on its stack.
struct Running<'a, 'n> {
    def: u32,
    args: &'a [Handle],
    /// The function's name in messages: empty in release play.
    named: &'n str,
    /// Last positive source line, the trace's; -1 before one.
    line: i32,
    /// Parameters copied so far, for the debug count check.
    params: usize,
}

/// A record's pointers: a place's result is its first operand's, followed
/// through places; any other record's is its own value.
fn link(definition: &Definition, at: u32) -> Link {
    let instr = &definition.code[at as usize];
    let operand = |o: Operand, p: Part| match o {
        Operand::Slot(n) => Some(result_of(definition, n, definition.code.len())),
        Operand::Value(_) | Operand::Constant(_) => Some(part(at, p)),
        Operand::None => None,
    };

    let lhs = operand(instr.lhs, Part::Lhs);

    Link {
        result: match instr.dest {
            Dest::Alias => lhs.unwrap_or(part(at, Part::Own)),
            Dest::None | Dest::Value(_) => part(at, Part::Own),
        },
        lhs,
        rhs: operand(instr.rhs, Part::Rhs),
    }
}

/// The handle record `at`'s result pointer names, following at most
/// `budget` places.
fn result_of(definition: &Definition, at: u32, budget: usize) -> u32 {
    let instr = &definition.code[at as usize];

    match (instr.dest, instr.lhs) {
        (Dest::Alias, Operand::Slot(n)) if budget > 0 => result_of(definition, n, budget - 1),
        (Dest::Alias, Operand::Value(_) | Operand::Constant(_)) => part(at, Part::Lhs),
        _ => part(at, Part::Own),
    }
}

/// The record a `break` or `continue` names.
const fn loop_of(head: Head, at: usize) -> usize {
    match head {
        Head::Loop(each) => each as usize,
        _ => at,
    }
}

/// A message with its one `%s`.
fn format_name(message: &str, name: &str) -> String {
    message.replacen("%s", name, 1)
}

/// Names for [`text::value`]: functions from the VM, agents from the host.
struct Namer<'v, 'p, 'h> {
    vm: &'v Vm<'p>,
    host: Option<&'h mut dyn Host>,
}

impl text::Names for Namer<'_, '_, '_> {
    fn function(&self, handle: u32) -> Option<&str> {
        self.vm.function_name(handle)
    }

    fn agent(&mut self, id: u32) -> Option<String> {
        self.host.as_mut().and_then(|h| h.agent_name(id))
    }

    fn float_text(&self) -> text::FloatText {
        self.vm.config.float_text
    }
}

mod snap;

#[cfg(test)]
mod tests {
    use super::*;

    /// A run aside that changes objects, one twice, makes one and reuses
    /// a freed one leaves them as a copy taken before: the journal's
    /// put-back passes [`CHECK_ASIDE`]'s check.
    #[test]
    fn a_run_aside_puts_every_object_back() {
        let program = Program::load::<&[u8]>(&[]).expect("empty program");
        let mut vm = Vm::new(&program, Config::default());
        let [a, b, freed] = [1, 2, 3].map(|n| vm.alloc(Value::Integer(n)));

        vm.release(freed);

        let mut kept = vm.keep();

        kept.objects = Some(vm.objects.clone());
        *vm.object_mut(a) = Value::Integer(10);
        *vm.object_mut(a) = Value::Integer(11);
        *vm.object_mut(b) = Value::String("changed".to_owned());

        let reused = vm.alloc(Value::Boolean(true));
        let made = vm.alloc(Value::Integer(4));

        assert_eq!((reused, made.0), (freed, 4));

        vm.put_back(kept);

        assert_eq!(vm.objects.len(), 4);
        assert_eq!(vm.object(a), &Value::Integer(1));
        assert_eq!(vm.object(freed), &Value::Integer(3));
        assert!(vm.journal.is_none());
    }
}
