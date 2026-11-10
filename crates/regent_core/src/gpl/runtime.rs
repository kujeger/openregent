//! How `MajestyHD.exe` runs GPL: where a function's values live, what a call
//! does, how threads are scheduled, and what an error unwinds.
//!
//! **Binary** unless marked. Prose: `docs/formats/bytecode.md`, "How the engine runs
//! a function".
//!
//! ```text
//!  frame ──> scheduler ──> unit ──> agent threads run
//!            budget, two queues       └─> a thread runs      ──┐
//!  engine event ──> dispatch, by name, at once ───────────────┤
//!                                                             v
//!                        invoke the function ──> execute      ──┐
//!                                ^                              │ opLParen
//!                                └───────── call runs <─────────┘
//! ```
//!
//! # One activation per function
//!
//! A script function's values live in its instruction records, one set per
//! function, built once at load by gpl register script from a single
//! array. The record set is the function record's context a, so every call of
//! a function uses the same slots. A call frame — callee and argument vector —
//! is per call, on the C stack.
//!
//! On entry a parameter is copied from its argument, a
//! local reset to its type's default, and `__Return_value__` bound to
//! the caller's result; temporaries and call results keep what the last run
//! left. A normal exit resets the locals. An error skips that in every frame
//! it unwinds, so a `foreach` cursor stays allocated and its next run resumes
//! it (**Inferred** from the executor's error case).
//!
//! Re-entering a function that is running overwrites its parameters, resets
//! its locals, restarts its `foreach` cursors and rebinds its return value;
//! when the inner call returns, the outer one continues on those values.
//! Release play does not stop it: the recursive-call check ([`DebugCheck`]) is
//! debug-only and **Inferred** never to fire.

/// `opReturn` and `opReturnValue` set the record index to this, the
/// prologue's `opExit`; the loop's increment then lands on record 3, the jump
/// to the end. So `opExit` never runs.
pub const RETURN_INDEX: u32 = 2;

/// Where the exit walk starts looking for `opDeclare`: record 3, `0x6c` in
/// 36-byte records.
pub const EXIT_WALK_FROM: u32 = 3;

// ---------------------------------------------------------------------------
// Time and scheduling
// ---------------------------------------------------------------------------

/// Game ms from `when` to `now` on the game clock: an unsigned
/// 32-bit word, so the difference wraps, as [`is_due`]'s does. A `when`
/// after `now` reads as a long time.
///
/// ```
/// use regent_core::gpl::runtime::elapsed;
///
/// assert_eq!(elapsed(150, 100), 50);
/// assert_eq!(elapsed(40, 100), u32::MAX - 59);
/// ```
#[must_use]
pub const fn elapsed(now: u32, when: u32) -> u32 {
    now.wrapping_sub(when)
}

/// Game time per frame. `GE_GlobalData` writes it of the
/// global data, `GE_GlobalData`; no other store found near the global's reads
/// (**Inferred** constant). Not `TimePerFrame`, which only `GSMessageManager`
/// reads.
pub const MS_PER_FRAME: u32 = 50;

/// Real ms the frame loop holds a game frame for at the least, by game
/// speed, fastest first: the table at game paces. The options' slider
/// position `p`, 1 to 9, picks `PACES[9 - p]` into
/// game pace and `GSMessageManager`; `GSMessageManager`
/// waits out the rest of the frame on it. A slow frame is
/// not caught up: the game falls behind real time.
pub const PACES: [u32; 9] = [10, 20, 30, 40, 50, 85, 120, 175, 250];

/// The pace a game starts at: `TimePerFrame`, 50, into
/// `GSMessageManager`, and game pace's file value:
/// slider position 5. So a game second is a real second at the default
/// speed.
pub const DEFAULT_PACE: u32 = 50;

/// The options' speed slider, slowest first: position `p` holds
/// `PACES[9 - p]`.
pub const SLIDER: std::ops::RangeInclusive<u32> = 1..=9;

/// The pace at slider position `p`; `None` off the slider.
/// `Option2MenuController` looks a pace up the other way.
///
/// ```
/// use regent_core::gpl::runtime::slider_pace;
///
/// assert_eq!(slider_pace(1), Some(250));
/// assert_eq!(slider_pace(9), Some(10));
/// assert_eq!(slider_pace(10), None);
/// ```
#[must_use]
pub fn slider_pace(p: u32) -> Option<u32> {
    if SLIDER.contains(&p) {
        PACES.get((SLIDER.end() - p) as usize).copied()
    } else {
        None
    }
}

/// A game speed key's way: `<` slower, `>` faster.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpeedKey {
    Slower,
    Faster,
}

/// The pace after a speed key at `pace` (map keys cases `0x3c`,
/// `0x3e`, outside a network game): the next of [`PACES`] that way, the
/// end kept; a pace not in it gives [`DEFAULT_PACE`].
#[must_use]
pub fn stepped(pace: u32, speed: SpeedKey) -> u32 {
    let at = PACES.iter().position(|p| *p == pace);
    let next = match speed {
        SpeedKey::Slower => at.map(|i| (i + 1).min(PACES.len() - 1)),
        SpeedKey::Faster => at.map(|i| i.saturating_sub(1)),
    };

    next.map_or(DEFAULT_PACE, |i| PACES[i])
}

/// Game time while a new game runs its setup dispatches — `InitializeGpl`,
/// the scenario, each building's `NewUnitInit` and birth script — so every
/// thread they start has `last` 0.
///
/// **Game**, `local/traces/freestyle_seed1_S_hd`: `Palace_Birth` starts five
/// threads and `lair_birth` two; each holds last 0 in the save at 150.
pub const SETUP_TIME: u32 = 0;

/// Game time of the first frame's threads. Game frame step steps
/// the clock in game frame step before it runs the scheduler.
///
/// **Game**: `GplKickoff` and every `birthScript2`, started at
/// [`SETUP_TIME`] with interval 1, hold last 50.
pub const FIRST_FRAME: u32 = MS_PER_FRAME;

/// Units a frame may visit beyond the count both queues held when it began:
/// the scheduler stops once its visits pass that count.
/// So a frame visits every unit and the next one again, and the start of
/// the pass moves on by one unit each frame. The original's instruction
/// budget can stop a pass sooner; openregent has none
/// (`docs/gpl/runtime.md`).
///
/// **Game**: [`Queues`] with this rule reproduces the queues of both traced
/// saves, the day-1 one after 1242 frames.
pub const EXTRA_UNITS: usize = 1;

/// The scheduler's two unit queues, fields of gpl ai, as a model:
/// units in the order the scheduler takes them. A frame pops the front of
/// active, moves it to the back of next, runs its threads; an empty active
/// swaps with next.
///
/// ```text
///   active: 4 5 6 7        next: 1 2 3
///           ^ taken first        ^ oldest   new units and visited ones
///                                           join at the back
/// ```
///
/// ```
/// use std::ops::ControlFlow;
///
/// use regent_core::gpl::runtime::Queues;
///
/// // Seven agents made at setup, three frames.
/// let mut queues = Queues::new(1..=7);
///
/// for _ in 0..3 {
///     queues.frame(|_, _| ControlFlow::Continue(()));
/// }
///
/// assert_eq!(queues.saved(), (vec![7, 6, 5, 4], vec![3, 2, 1]));
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Queues {
    active: std::collections::VecDeque<u32>,
    next: std::collections::VecDeque<u32>,
}

impl Queues {
    /// Agents made before the first frame, in order of making: each joined
    /// the next queue.
    pub fn new(setup: impl IntoIterator<Item = u32>) -> Queues {
        Queues {
            active: std::collections::VecDeque::new(),
            next: setup.into_iter().collect(),
        }
    }

    /// The lists as a save writes them: running, waiting, each newest first.
    #[must_use]
    pub fn saved(&self) -> (Vec<u32>, Vec<u32>) {
        (
            self.active.iter().rev().copied().collect(),
            self.next.iter().rev().copied().collect(),
        )
    }

    /// A new agent, gpl agent new: the back of the next queue, once.
    /// A log names a hero twice, in `Generate_Character_Attributes` and
    /// `NewUnitInit`, but its agent is made once, at the first.
    pub fn join(&mut self, unit: u32) {
        if !self.active.contains(&unit) && !self.next.contains(&unit) {
            self.next.push_back(unit);
        }
    }

    /// A deleted agent, gpl agent delete: gpl agent unlink
    /// erases it from whichever queue holds it.
    pub fn leave(&mut self, unit: u32) {
        self.active.retain(|u| *u != unit);
        self.next.retain(|u| *u != unit);
    }

    /// Every unit in either queue.
    #[must_use]
    pub fn len(&self) -> usize {
        self.active.len() + self.next.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Every unit in either queue, active first.
    pub fn units(&self) -> impl Iterator<Item = u32> + '_ {
        self.active.iter().chain(&self.next).copied()
    }

    /// One frame of gpl scheduler: take a unit, move it to the back of the
    /// other queue, then hand it to `run`, which may [`Queues::join`] new
    /// agents, and breaks to end the pass (a stopped game). At most
    /// [`EXTRA_UNITS`] more units than the queues held at the start.
    pub fn frame(&mut self, mut run: impl FnMut(&mut Queues, u32) -> std::ops::ControlFlow<()>) {
        let count = self.len();
        let mut visited = 0;

        while visited < count + EXTRA_UNITS {
            if self.active.is_empty() {
                std::mem::swap(&mut self.active, &mut self.next);
            }

            let Some(unit) = self.active.pop_front() else {
                break;
            };

            self.next.push_back(unit);
            visited += 1;

            if run(self, unit).is_break() {
                break;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Threads
// ---------------------------------------------------------------------------

/// A thread's mode, less [`SUSPENDED`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[repr(u32)]
pub enum ThreadMode {
    /// Not scheduled: `KillThread`, or a once-thread that ran.
    #[default]
    Off = 0,
    /// Runs once, cleared to [`ThreadMode::Off`] as the run starts.
    Once = 1,
    /// Runs every interval.
    Repeat = 2,
}

/// Mode bit 7: held, not run.
pub const SUSPENDED: u32 = 0x80;

/// Whether gpl thread run runs a thread now: a mode, not suspended,
/// and at least the interval since the last run, unsigned.
///
/// ```
/// use regent_core::gpl::runtime::{SUSPENDED, ThreadMode, is_due};
///
/// assert!(is_due(ThreadMode::Repeat as u32, 1000, 5000, 6000));
/// assert!(!is_due(ThreadMode::Repeat as u32, 1000, 5000, 5950));
/// assert!(!is_due(ThreadMode::Repeat as u32 | SUSPENDED, 1000, 5000, 9000));
/// assert!(!is_due(ThreadMode::Off as u32, 0, 0, 9000));
/// ```
pub const fn is_due(mode: u32, interval: u32, last: u32, now: u32) -> bool {
    mode & !SUSPENDED != 0 && mode & SUSPENDED == 0 && now.wrapping_sub(last) >= interval
}

// ---------------------------------------------------------------------------
// Calls from the engine
// ---------------------------------------------------------------------------

/// Scripts the engine dispatches by a literal name, sorted. Gpl dispatch
/// runs one at once, inside the engine code that fired the event, and costs
/// the frame's budget nothing. Other sites take
/// the name from data: a descriptor, a quest, a spell.
pub const DISPATCHED: [&str; 61] = [
    "Agrela_Heal_Effect",
    "Animate_Bones_Effect",
    "Anti_Magic_Shield_Begin",
    "Blessing_Begin",
    "BuildingReachedMaxHP",
    "CalculateDifficulty",
    "CalculateDifficultyString",
    "CanIBuildThisBuilding",
    "ChainLightning_Begin",
    "ChangeOfHeart_Begin",
    "Cheat_wave_general",
    "Cheat_wave_goblins",
    "Cheat_wave_raiders",
    "Cheat_wave_raiders_easy",
    "Cheat_wave_undead",
    "Dauros_Stone_Skin_Begin",
    "Dismiss_Begin",
    "DoAssembly",
    "DoExtortion",
    "DoMarketDay",
    "DoRageOfKrolm",
    "DoWizTowerEnchant",
    "Embassy_Init",
    "Embassy_Spawn",
    "EndMarketDay",
    "EndWizTowerEnchant",
    "Fervus_Heal_Effect",
    "Fire_Strike_Hit",
    "FrostField_Begin",
    "Gamble",
    "Generate_Character_Attributes",
    "GetSupportedHenchmen",
    "HallOfChampions_Bounty_Cost",
    "HallOfChampions_Bounty_Period",
    "InitializeGpl",
    "Invisibility_Begin",
    "IsValidMiniViewTarget",
    "KickoutRangers",
    "Lightning_Bolt_Hit",
    "Mausoleum_Resurrect_Begin",
    "Mausoleum_Resurrect_Cost",
    "MoveCamp",
    "NewUnitInit",
    "Palace_Pre_Birth",
    "Petrify_Begin",
    "Reanimate_Begin",
    "Resurrection_Begin",
    "ShowExtortionCut",
    "ShowExtortionTotal",
    "SuperCharge_Begin",
    "UnitDelete",
    "Unit_Call_Deathscript",
    "Unit_Dismiss_Gravestone",
    "Unit_Dismiss_Gravestone_fast",
    "Unit_Dismissed",
    "Vigilance_Begin",
    "Vines_Begin",
    "Winged_Feet_Begin",
    "Wither_Begin",
    "advance_to_level",
    "building_upgraded",
];

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// The two places a script error stops. Each logs "Script error" and the call
/// sequence (**Inferred**: only under `-debugout`), pops the whole GPL call
/// stack, and carries on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Catcher {
    /// gpl thread run. Resumes at the function's end: last-run not
    /// updated, so a repeating thread is due again next frame; a once-thread
    /// already cleared its mode; the running byte stays set; the
    /// pending function is not moved in.
    Thread,
    /// gpl dispatch. Resumes where the budget counter is restored;
    /// the engine code that dispatched continues as if the script returned.
    Dispatch,
}

// ---------------------------------------------------------------------------
// Debug and release checks
// ---------------------------------------------------------------------------

/// A check the executor makes only with GPL debugging. Beside these
/// the flag gates only the debugger's breakpoints and stepping, profiling and
/// tracing; gpl call run's hook adds a debugger call-stack entry.
/// Engine flags bit `0x800`, with GPL debugging (`0x2000`, set with
/// `0x100` and `0x80` by `-gpldebug`), times each call and instruction.
///
/// Two more checks no play reaches:
///
/// - `Illegal recursive call to function $%s()` when record 1's `head` is 1.
///   Its one setter, gpl record set head, is called only with 0, by `opExit` —
///   which never runs, see [`RETURN_INDEX`] — and by the constant evaluator.
///   **Inferred**: it never fires.
/// - `Inline $GetAttribute( agent#%d ) failed: unit no longer exists`, opcode
///   40, inline `GetAttribute`. No shipped `.bcd` uses opcodes 40 to 42.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DebugCheck {
    /// `opLocalize`: the argument's type code differs from the parameter's.
    /// Without it an argument converts through the parameter's assign, or
    /// throws a bad conversion.
    ParameterType,
    /// After the last `opLocalize`, fewer parameters than arguments. Without
    /// it extra arguments are ignored.
    TooManyArguments,
    /// More parameters than arguments. Unreachable: gpl arg reads past
    /// the vector first, in both modes. Its checked iterator calls
    /// invalid parameter handler, a bare return, and the read goes on.
    TooFewArguments,
}

impl DebugCheck {
    pub const fn message(self) -> &'static str {
        match self {
            DebugCheck::ParameterType => "Parameter type mismatch: \"%s\"",
            DebugCheck::TooManyArguments => "Function $%s() was passed too many arguments",
            DebugCheck::TooFewArguments => "Function $%s() was passed too few arguments",
        }
    }
}

/// Errors the executor throws in release play too. Also `foreach list
/// contains a NULL value in function $%s()` for an empty `foreach` element,
/// and `Unknown operation attempted in interpreter` for an opcode past 42.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseCheck {
    /// `opIf` on an empty handle.
    NullIf,
    /// `opForeach` on an empty variable handle.
    NullIterator,
}

impl ReleaseCheck {
    pub const fn message(self) -> &'static str {
        match self {
            ReleaseCheck::NullIf => "If statement evaluates to a NULL value in function $%s()",
            ReleaseCheck::NullIterator => {
                "foreach statement iterator evaluates to a NULL value in function $%s()"
            }
        }
    }
}

/// To and from the save model.
mod snap {
    use super::*;
    use crate::save::{self as m, Refused, Saved};
    use std::result::Result;

    impl Saved for Queues {
        type Model = m::Queues;

        fn save(&self) -> m::Queues {
            m::Queues {
                active: self.active.save(),
                next: self.next.save(),
            }
        }

        fn load(model: m::Queues) -> Result<Queues, Refused> {
            Ok(Queues {
                active: model.active.into(),
                next: model.next.into(),
            })
        }
    }

    impl Saved for ThreadMode {
        type Model = m::Mode;

        fn save(&self) -> m::Mode {
            match self {
                ThreadMode::Off => m::Mode::Off,
                ThreadMode::Once => m::Mode::Once,
                ThreadMode::Repeat => m::Mode::Repeat,
            }
        }

        fn load(model: m::Mode) -> Result<ThreadMode, Refused> {
            Ok(match model {
                m::Mode::Off => ThreadMode::Off,
                m::Mode::Once => ThreadMode::Once,
                m::Mode::Repeat => ThreadMode::Repeat,
            })
        }
    }
}
