//! A game run by the engine's rules: the setup, the frame loop, the
//! player's input. The world answers every builtin and holds every
//! attribute.
//!
//! The setup dispatches [`crate::gpl::startup`] models, then scheduler
//! frames by [`crate::gpl::runtime`]'s queues and due rule. Its host is
//! [`Live`] ([`Game::free`]). What a front end's press or click sends is
//! [`orders`]'s.
//!
//! ```text
//!  World::new_game
//!  setup, now 0:  InitializeGpl ─ unit pass ─ quest script
//!                 ─ NewUnitInit per agent ─ Palace_Pre_Birth per owner
//!  frame n, now 50 n:  Input ──> Game::apply, the one decoder
//!                      World::step: the orders, paused at each dispatch
//!                      Queues ──> unit ──> function attributes, name order
//!                                            └─> threads::run
//!
//!  input before frame n, at the clock of frame n - 1:
//!    Unpreview ── the build menu's preview, agent -6, gone: UnitDelete
//!    Build     ── CREATE_BUILDING_MESSAGE: World::build, its name,
//!                 NewUnitInit
//! ```

pub mod check;
mod digest;
pub mod orders;
mod text;

pub use digest::deep_fold;

use std::fmt;
use std::ops::ControlFlow;
use std::sync::Arc;

use super::attr_objs::AttrObjs;
use super::host::{Faults, Held, Live, Reports};
use crate::base::switch::Switch;
use crate::data::fourcc::FourCc;
use crate::data::gpltype::Kind;
use crate::gpl::load::Entry as MapEntry;
use crate::gpl::ops::{NULL_AGENT, Value};
use crate::gpl::runtime::{self, Queues};
use crate::gpl::vm::{Crash, ObjId, Vm, threads};
use crate::sim::space::pos32;
use crate::sim::world::cheat::{self, Cheat, CheatCode};
use crate::sim::world::{
    CROWN, GENERATE, NEW_UNIT, UNIT_DELETE, World,
    action::{self, Action},
    alter::{Alter, Set},
    build::{BuildOrder, PREVIEW_ID},
    dismiss::GRAVESTONE,
    flag::Flag,
    guild::{
        bounty::{BOUNTY_COST, BOUNTY_PERIOD, Bounty},
        camp::{Camp, KICKOUT, MOVE_CAMP, Resource},
        embassy::EMBASSY_INIT,
        extortion::{EXTORTION_CUT, EXTORTION_TOTAL, Extortion},
        mausoleum::{RESURRECT_BEGIN, Resurrect},
        upkeep::Toggle,
    },
    hire::{DISMISS, Hire},
    kingdom::{Henchmen, SUPPORTED_HENCHMEN},
    missile::{Script, script_args},
    naming::Rename,
    offer::{Menu, Placement, Rules},
    recruit::Recruit,
    research::Research,
    reveal::Sighting,
    spell::cast::{Bless, Blessed, Cast},
    threads::{Gpl, Logging},
};

/// Records one run may execute before it is stopped.
pub const RUN_LIMIT: u64 = 1_000_000;

/// What build preview test dispatches last, the string at
/// `"CanIBuildThisBuilding"`: 0 lets the building be placed.
pub const CAN_BUILD: &str = "CanIBuildThisBuilding";

/// What a new game dispatches on each owner's palace, last
/// ([`crate::gpl::startup::dispatches`]).
pub const PALACE_PRE_BIRTH: &str = "Palace_Pre_Birth";

/// What a new game dispatches first ([`crate::gpl::startup::dispatches`]).
pub const INITIALIZE: &str = "InitializeGpl";

/// The agent hero skill cap asks by name (`"GplAIRoot"`), and its
/// attribute (`"Deathmatch_Rules"`).
pub const AI_ROOT: &str = "GplAIRoot";
pub const DEATHMATCH_RULES: &str = "Deathmatch_Rules";

/// Set, each kept thread list is checked against one built anew
/// (`Game::thread_objs`): a debugging aid for a change that alters
/// agents or attributes and does not drop the lists.
/// `MAJESTY_CHECK_THREADS=1 cargo nextest run`.
pub static CHECK_THREADS: Switch = Switch::new("MAJESTY_CHECK_THREADS");

/// A player's input, as the engine takes it between frames: what a front
/// end sends when the player clicks. [`Game::input`] names the frame it
/// comes before; a message decoded in err.log's frame `f` comes before
/// `f + 1`. Who gave it is its [`Order`]'s.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Input {
    /// The build menu's preview of a building is deleted as the player
    /// places it (build preview make's agent -6): `UnitDelete`
    /// (**Game**: between the frame of `SendMessage` and the next). The
    /// preview is its order's player's.
    Unpreview(FourCc),
    /// `CREATE_BUILDING_MESSAGE` decoded, `GSMessageManager` from
    /// game frame step before the clock step:
    /// [`World::build`], then `NewUnitInit`.
    Build(BuildOrder),
    /// `CREATE_VEHICLE_MESSAGE` on a guild: the decode pays and names
    /// ([`crate::sim::world::hire::Hire`]); the hero comes its delay later:
    /// `Generate_Character_Attributes`, then
    /// `NewUnitInit`.
    Recruit(Recruit),
    /// `CREATE_REWARD_FLAG_MESSAGE`: [`World::flag`], then `NewUnitInit`.
    Flag(Flag),
    /// `PLAYER_SPELL_MESSAGE` making a unit: [`World::cast`], `NewUnitInit`,
    /// then the unit's `PRIM` callback (**Game**: `Farseeing_unit_created`
    /// follows its unit's `NewUnitInit`).
    Cast(Cast),
    /// `PLAYER_SPELL_MESSAGE` cast on a unit, making none: its function
    /// dispatched with the target (player spell apply,
    /// [`World::bless`]). **Game**: the probed play's invisibility at 5229
    /// runs `Invisibility_Begin(Rogue#13)` between the passes.
    Bless(Bless),
    /// `UPGRADE_BUILDING_MESSAGE` with message `0x0c` zero, on a building's
    /// agent: [`World::upgrade`], then `building_upgraded`.
    Upgrade(u32),
    /// `UPGRADE_BUILDING_MESSAGE` with message `0x0c` set: research
    /// ([`World::research_later`]); its frame is the input's.
    Research(Research),
    /// An `AlterAttributeDeltaMessage`, sent with no decode line,
    /// applied in its place among the frame's decodes ([`World::alter`]);
    /// its frame is the input's.
    Alter(Alter),
    /// `GUILD_ACTION_MESSAGE`'s enchantment on a wizards' tower, by agent:
    /// paid, then
    /// `DoWizTowerEnchant` dispatched at once.
    Enchant(u32),
    /// A `PlayerResourceMessage` on the treasury: a press's price
    /// ([`World::resource`]).
    Resource(Resource),
    /// The rangers' guild's camp move pressed, by agent: its
    /// `PlayerResourceMessage` spending `#MoveCampCost`, the price held
    /// for its player ([`World::camp_begin`]). Decodes as that message.
    CampBegin(u32),
    /// The camp move left without its move: its `PlayerResourceMessage`
    /// giving the held price back ([`World::camp_leave`]). Decodes as
    /// that message.
    CampLeave,
    /// `GUILD_ACTION_MESSAGE` action 0, a camp move: the held price kept
    /// ([`World::camp_sent`]); where the place search finds room,
    /// `KickoutRangers`, the move, `MoveCamp`.
    Camp(Camp),
    /// `GUILD_ACTION_MESSAGE` action `0x16` on an embassy, by agent: an
    /// open one shuts; a shut one dispatches `Embassy_Init`, arms its
    /// timer and opens.
    Embassy(u32),
    /// `CHEAT_CODE_MESSAGE`, a chat phrase's ([`crate::sim::world::cheat`]).
    Cheat(CheatCode),
    /// `GUILD_ACTION_MESSAGE` action `0x15`, the Mausoleum panel's
    /// resurrect: [`World::resurrect`], then `Mausoleum_Resurrect_Begin`.
    Resurrect(Resurrect),
    /// `DESTROY_EVENT_FLAG_MESSAGE` on a message flag, by handle: read,
    /// so deleted ([`World::read_message`]). No dispatch: it has no agent.
    /// A flag is no player's (owner -1), and the decode
    /// checks no sender: any player's
    /// read deletes it for all.
    Read(u32),
    /// `DESTROY_REWARD_FLAG_MESSAGE` on a reward flag, by handle: taken
    /// down ([`World::take_down`]), its `UnitDelete` at once. No gold back.
    Unflag(u32),
    /// `DESTROY_BUILDING_MESSAGE` on a building, by agent: an eighth of
    /// its price back, its death script at once ([`World::demolish`]).
    Demolish(u32),
    /// `GUILD_ACTION_MESSAGE` actions 6 to 9 on a building, by agent: a
    /// tax or repair flag flipped, its pair cleared ([`World::toggle`]).
    /// No dispatch.
    Toggle(Toggle),
    /// `GUILD_ACTION_MESSAGE` action 5 on a hero, by agent, the hero
    /// panel's dismissal: `APV:28` set, then `Unit_Dismissed`
    /// ([`crate::sim::world::dismiss::DISMISSED`]).
    Dismiss(u32),
    /// `DESTROY_VEHICLE_MESSAGE`, by handle: a dead hero's dismissal or a
    /// double click on an own ruin or body. [`World::gravestone`], then
    /// `Unit_Dismiss_Gravestone` at once
    /// ([`crate::sim::world::dismiss::GRAVESTONE`]).
    Clear(u32),
    /// `GUILD_ACTION_MESSAGE` action 3 on a guild, by agent, the warriors'
    /// guild's Call to Arms: `DoAssembly` at once
    /// ([`crate::sim::world::guild::assembly::ASSEMBLY`]). Its price is a
    /// [`Input::Resource`] of its own.
    Assemble(u32),
    /// `GUILD_ACTION_MESSAGE` action 10 on a marketplace, by agent, its
    /// Market Day: `APB:22` the clock, `DoMarketDay` at once, then its
    /// timed order; one running runs a duration more instead
    /// ([`crate::sim::world::guild::market::MARKET_DAY`]).
    MarketDay(u32),
    /// `GUILD_ACTION_MESSAGE` action 2 on a rogues' guild, by agent, its
    /// extortion: `DoExtortion` at once
    /// ([`crate::sim::world::guild::extortion::EXTORTION`]).
    Extort(u32),
    /// `SET_UNIT_NAME_MESSAGE`, the name box's Enter: the unit's own name
    /// ([`World::rename`]). No dispatch.
    Rename(Rename),
    /// `GUILD_ACTION_MESSAGE` action 1 on a Temple to Krolm, by agent, its
    /// Rage: with none running, the palace's `APB:52` the clock, then
    /// `DoRageOfKrolm` ([`World::rage_now`]). Its price is an
    /// [`Input::Resource`] of its own.
    Rage(u32),
    /// `GUILD_ACTION_MESSAGE` action `0x14`, a Hall of Champions bounty:
    /// its length and price by interface runs, then [`World::place_bounty`].
    Bounty(Bounty),
    /// `GUILD_ACTION_MESSAGE` action 11 on a Gambling Hall, by agent, its
    /// SPIN: [`World::spin`]; the spin's timer runs `Gamble` as it ends
    /// ([`crate::sim::world::guild::gamble`]).
    Gamble(u32),
    /// `GUILD_ACTION_MESSAGE` action 12 on a Gambling Hall, by agent,
    /// MAKE NEW BET: [`World::new_bet`]. No dispatch.
    NewBet(u32),
    /// An `AlterAttributeMessage` of one key, a panel's: the Gambling
    /// Hall's wager or colour ([`World::assign`]). Its frame is the decode's,
    /// as any message's.
    Set(Set),
}

impl Input {
    /// Whether it decodes as a `PlayerResourceMessage`: after the
    /// frame's other messages (`Game::inputs`).
    #[must_use]
    pub const fn is_resource(&self) -> bool {
        matches!(
            self,
            Input::Resource(_) | Input::CampBegin(_) | Input::CampLeave
        )
    }
}

/// An [`Input`] and the player who gave it: what lockstep play sends.
///
/// An input that names an owner (a building's, flag's, spell's or
/// treasury move's) keeps its own, the message's word. One that names
/// only an agent or a handle, or nothing, has no other. The engine's
/// decode checks no sender (**Binary** for [`Input::Read`]; **Inferred**
/// for the rest); here [`Game::check`] holds each order to its player
/// first: an owner word its own, the piece acted on its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Order {
    /// The slot that gave it.
    pub player: u32,
    pub input: Input,
}

/// A single-player game's input: the crown's.
impl From<Input> for Order {
    fn from(input: Input) -> Order {
        Order {
            player: CROWN,
            input,
        }
    }
}

/// Frames from an order's giving to the frame it comes before, in single
/// player: the next ([`Game::order_frame`]). Lockstep play sets more, to
/// give the orders time to reach every machine ([`Game::with_delay`]).
pub const SOLO_DELAY: u32 = 1;

/// An order for a frame already run: never decoded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Late {
    /// The frame it named.
    pub frame: u32,
    /// Frames run when it came.
    pub ran: u32,
}

impl fmt::Display for Late {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "an order for frame {}, after frame {} ran",
            self.frame, self.ran
        )
    }
}

impl std::error::Error for Late {}

/// How a game begins: new, its name generators seeded with the session
/// word, or loaded, keeping the save's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Start {
    New,
    Loaded,
}

/// Why a game the world runs alone stopped: a run that crashed, or a
/// world refusal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stop(pub String);

/// A crashed run as the game's stop.
fn stopped(crash: Crash) -> Stop {
    Stop(format!("{crash:?}"))
}

/// A world refusal as the game's stop.
fn faulted(fault: &crate::gpl::vm::Fault) -> Stop {
    Stop(format!("{fault:?}"))
}

/// A game in progress, on a world borrowed for `'w` or owned, its
/// scripts run by its own VM.
pub struct Game<'w> {
    vm: Vm<'w>,
    host: Live<'w>,
    queues: Queues,
    /// Frames run.
    pub frames: u32,
    /// Orders by the frame they come before, in the order taken.
    inputs: Vec<(u32, Order)>,
    /// Frames from an order's giving to its frame ([`SOLO_DELAY`]).
    delay: u32,
    /// The agent whose threads run, while the scheduler runs them; kept
    /// by a frame that stopped there.
    pub running: Option<u32>,
    /// The seed the GPL RNG took.
    seed: u32,
    /// Orders refused since the last [`Game::take_dropped`].
    dropped: Vec<check::Dropped>,
    /// The agents stamp and queue length [`Game::leave`] last looked at:
    /// no agent gone and no unit joined since, none is to leave.
    swept: Option<(u64, usize)>,
}

impl<'w> Game<'w> {
    /// A new game on `world`, run by `vm`, its GPL RNG and name generators
    /// seeded with `seed`, the session word.
    pub fn free(mut vm: Vm<'w>, seed: u32, world: impl Into<Held<'w>>) -> Self {
        vm.set_trace(None);
        Game::begin(vm, Live::new(world), seed, Start::New)
    }

    /// Script errors reported.
    #[must_use]
    pub const fn reports(&self) -> &Reports {
        &self.host.reports
    }

    /// Where the world gave no answer and the default stood in.
    #[must_use]
    pub const fn faults(&self) -> &Faults {
        &self.host.faults
    }

    /// The fallbacks kept since the last call ([`Faults::take`]).
    pub fn take_faults(&mut self) -> Vec<super::host::Fallback> {
        self.host.faults.take()
    }

    /// Answered calls, per builtin.
    #[must_use]
    pub fn calls(&self) -> std::collections::BTreeMap<&'static str, usize> {
        self.host.calls.by_name()
    }
}

impl<'w> Game<'w> {
    /// A game on `host`, run by `vm`, its GPL RNG seeded with `seed`; a
    /// new game's name generators too.
    pub fn begin(mut vm: Vm<'w>, mut host: Live<'w>, seed: u32, start: Start) -> Self {
        vm.seed(seed);

        if start == Start::New {
            host.core_mut().world_mut().seed_names(seed);
        }

        // Building price reads its `#LPS_` constants once.
        host.core_mut().world_mut().read_lps(vm.program());

        vm.set_limit(Some(RUN_LIMIT));

        Game {
            vm,
            host,
            queues: Queues::default(),
            frames: 0,
            inputs: Vec::new(),
            delay: SOLO_DELAY,
            running: None,
            seed,
            dropped: Vec::new(),
            swept: None,
        }
    }

    /// The host.
    #[must_use]
    pub const fn host(&self) -> &Live<'w> {
        &self.host
    }

    /// The host, to change.
    pub const fn host_mut(&mut self) -> &mut Live<'w> {
        &mut self.host
    }

    fn world_mut(&mut self) -> &mut World {
        self.host.core_mut().world_mut()
    }

    /// Take `order` before frame `frame`; a bare [`Input`] is the
    /// crown's.
    ///
    /// # Errors
    ///
    /// A frame already run: the order would never be decoded.
    pub fn input(&mut self, frame: u32, order: impl Into<Order>) -> Result<(), Late> {
        if frame > self.frames {
            self.inputs.push((frame, order.into()));
            Ok(())
        } else {
            Err(Late {
                frame,
                ran: self.frames,
            })
        }
    }

    /// The same game giving its orders `delay` frames ahead
    /// ([`Game::order_frame`]); [`SOLO_DELAY`] unless set.
    #[must_use]
    pub const fn with_delay(mut self, delay: u32) -> Self {
        self.delay = delay;
        self
    }

    /// The delay changed in play ([`Game::with_delay`]); no state of the
    /// game's, so no save or digest holds it.
    pub const fn set_delay(&mut self, delay: u32) {
        self.delay = delay;
    }

    /// The frame an order the player gives now comes before: the next in
    /// single player, [`Game::with_delay`]'s later in lockstep.
    /// The engine takes a click after a frame's scripts in its pacing
    /// wait and decodes it at the
    /// next step's start, before that frame's clock step. **Game**: the
    /// tournament probe's building. A frame whose work
    /// fills the pace leaves no wait: the click waits for the next step's
    /// poll, after its clock step, and comes a frame later; that hangs on
    /// real time, so a front end never takes it so (**Inferred**: at the
    /// default pace a frame's work leaves the wait on any machine
    /// today).
    pub const fn order_frame(&self) -> u32 {
        self.frames + self.delay
    }

    /// Guild action apply case `0x14`: the length, then the price,
    /// each an interface run, then the bounty where they give integers.
    fn bounty(&mut self, b: &Bounty) {
        let index = [Value::Integer(b.term.index())];
        let period = self.interface_int(BOUNTY_PERIOD, &index);
        let price = self.interface_int(BOUNTY_COST, &index);

        if let (Some(period), Some(price)) = (period, price) {
            self.world_mut().place_bounty(b, period, price);
        }
    }

    /// `name(args)` as an interface run: no trace, no reveal. `None`
    /// where the run fails or gives no integer.
    fn interface_int(&mut self, name: &str, args: &[Value]) -> Option<i32> {
        let trace = self.vm.take_trace();

        self.world_mut().set_sighting(Sighting::Frozen);

        let ran = self.vm.dispatch(&mut self.host, name, args);

        self.world_mut().set_sighting(Sighting::Live);

        let logging = self.world().logging();

        self.vm.set_trace(trace.filter(|_| logging == Logging::On));

        match ran.map(|r| r.result) {
            Ok(Value::Integer(n)) => Some(n),
            _ => None,
        }
    }
}

/// The panels' queries: interface runs, asked when a front end draws or
/// the player points, so at frames and counts no input records. In the
/// engine they keep what they leave (activations made, objects freed in
/// another order, a preview's attribute objects); here each runs aside
/// (`Game::aside`) and leaves nothing a later frame sees, so a recorded
/// game replays the same however often its panels asked. They write
/// nothing the original's play reads (their sources: "called by the
/// interface").
impl Game<'_> {
    /// build preview test for `owner`'s `code` at `at`, 32nds, in
    /// its order: the world's rule (5), a placement not yet decoded (7),
    /// the world's ground tests (2, 1), then [`Game::can_build`] (4).
    pub fn placement(
        &mut self,
        menu: &Menu,
        owner: u32,
        code: FourCc,
        at: (i32, i32),
    ) -> Placement {
        let verdict = self
            .world()
            .placement(self.vm.program(), menu, owner, code, at);

        // Each machine knows its own player's orders only.
        let pending = self.inputs.iter().any(|(f, o)| {
            *f > self.frames && o.player == owner && matches!(o.input, Input::Build(_))
        });

        self.aside(|game| match verdict {
            Placement::Disabled | Placement::Unmet => verdict,
            _ if pending => Placement::Pending,
            Placement::Fits if game.ask_preview(menu, owner, code, at) != 0 => Placement::Refused,
            other => other,
        })
    }

    /// `GetSupportedHenchmen(palace, index)` for each index
    /// ([`Henchmen::ALL`]), as the palace's panel asks its counts.
    /// `None` where a run fails or gives no integer.
    pub fn supported_henchmen(&mut self, palace: u32) -> [Option<i32>; 3] {
        self.aside(|game| {
            Henchmen::ALL.map(|h| {
                let args = [Value::Agent(palace), Value::Integer(h.index())];

                game.interface_int(SUPPORTED_HENCHMEN, &args)
            })
        })
    }

    /// `ShowExtortionTotal(guild)` and `ShowExtortionCut(guild)`, as the
    /// rogues' guild's panel asks them (extortion total,
    /// extortion cut). `None` where a run fails or gives no
    /// integer, as in a game the log checks, which holds no such run
    /// there.
    pub fn extortion(&mut self, guild: u32) -> Option<Extortion> {
        let [total, cut] = self.aside(|game| {
            [EXTORTION_TOTAL, EXTORTION_CUT]
                .map(|name| game.interface_int(name, &[Value::Agent(guild)]))
        });

        Some(Extortion {
            total: total?,
            cut: cut?,
        })
    }

    /// A music track ended, in real time: the music player
    /// (music stopped dispatch, from `GE_MusicManager` in the frame step)
    /// dispatches `SetMusicStoppedCallback`'s function with the track
    /// that played, if one is set. Call it between frames.
    ///
    /// Its source: it "cannot change game state because this may not get
    /// called on some players machines". So it runs aside, and the track
    /// it asks is this machine's alone: players need not hear the same.
    /// `heard` is the track this machine plays if a callback chose it,
    /// else `None` for the scripts' ([`World::music`]); the callback's
    /// `LastMusicTrack` reads it. Returns the track to play from now on,
    /// until the next `PlayMusic` ([`crate::sim::world::Output::music_played`]). It
    /// draws no RNG; in the engine it leaves VM objects and activations
    /// (**Measured**).
    pub fn music_stopped(&mut self, heard: Option<i32>) -> Option<i32> {
        let world = self.world();
        let asked = (
            world.music_stopped().map(str::to_owned),
            heard.or(world.music()),
        );

        match asked {
            (Some(name), Some(track)) => self.aside(|game| {
                game.world_mut().set_music(track);
                game.interface_int(&name, &[Value::Integer(track)]);
                game.world().music()
            }),
            _ => heard,
        }
    }

    /// `ask`, the panels' runs, leaving no trace: the VM and the host
    /// (world, attribute objects, reports, faults) put back as they were
    /// after. A departure from the engine, whose runs keep
    /// what they leave; only object ids and activations differ, which no
    /// value a script reads shows (**Measured**).
    fn aside<R>(&mut self, ask: impl FnOnce(&mut Self) -> R) -> R {
        let vm = self.vm.keep();
        let host = self.host.keep();
        let answer = ask(self);

        self.vm.put_back(vm);
        self.host.put_back(host);
        answer
    }

    /// [`Game::can_build`]'s run: no trace, no reveal, the preview gone
    /// after, its attribute objects freed in name order. Its builtins
    /// draw nothing.
    fn ask_preview(&mut self, menu: &Menu, owner: u32, code: FourCc, at: (i32, i32)) -> i32 {
        let ids = menu.needs(code);

        if ids.is_empty() {
            0
        } else {
            let trace = self.vm.take_trace();

            self.world_mut().set_sighting(Sighting::Frozen);

            let answer = self.preview_run(&ids, owner, code, at);

            self.world_mut().set_sighting(Sighting::Live);

            let logging = self.world().logging();

            self.vm.set_trace(trace.filter(|_| logging == Logging::On));
            answer
        }
    }

    /// [`Game::ask_preview`]'s run.
    fn preview_run(&mut self, ids: &[FourCc], owner: u32, code: FourCc, at: (i32, i32)) -> i32 {
        let deps: Vec<Value> = self
            .world_mut()
            .dependents(ids)
            .into_iter()
            .map(Value::Agent)
            .collect();
        let program = self.vm.program();
        let made = self.world_mut().preview_at(program, code, owner, at);

        match made {
            Ok(id) => {
                self.host.core_mut().adopt_unqueued(&mut self.vm, id);

                let args = [Value::Agent(id), Value::List(deps.into())];
                let ran = self.vm.dispatch(&mut self.host, CAN_BUILD, &args);

                self.host.core_mut().release_attributes(id, &mut self.vm);
                self.world_mut().delete(id);

                match ran {
                    Ok(run) => match run.result {
                        Value::Integer(n) => n,
                        _ => 0,
                    },
                    Err(_) => 0,
                }
            }
            Err(_) => 0,
        }
    }
}

impl<'w> Game<'w> {
    /// The orders due before the next frame, by player, each player's in
    /// the order taken, so every machine decodes them alike however they
    /// arrived; but a `PlayerResourceMessage` decodes after the frame's
    /// other messages (**Game**: the Call to Arms probe's err.log). The
    /// world's own messages, sent in the last frame's step, land first
    /// ([`World::alter_step`]).
    fn inputs(&mut self) -> Result<(), Stop> {
        let next = self.frames + 1;

        self.world_mut().set_now(next * runtime::MS_PER_FRAME);
        self.world_mut().alter_step();

        let mut due: Vec<Order> = self
            .inputs
            .iter()
            .filter(|(f, _)| *f == next)
            .map(|(_, o)| *o)
            .collect();

        // A camp move whose press came in this same frame (its hot key and a
        // click inside 50 ms) decodes after the press, so the hold it
        // needs is taken first and the move goes through, as in the
        // original; else the move is refused and the price stays held.
        let pressed: Vec<u32> = due
            .iter()
            .filter(|o| matches!(o.input, Input::CampBegin(_)))
            .map(|o| o.player)
            .collect();
        let rank = |o: &Order| match o.input {
            Input::Camp(_) if pressed.contains(&o.player) => 2,
            ref i if i.is_resource() => 1,
            _ => 0,
        };

        // Stable: a player's orders keep the order given.
        due.sort_by_key(|o| (rank(o), o.player));

        for order in due {
            self.apply(order)?;
        }

        Ok(())
    }

    /// Decode `order` now, between frames, as `GSMessageManager`
    /// does from game frame step before the clock step: the one
    /// decoder of the player's input, once [`Game::check`] lets it; a
    /// refused one is dropped ([`Game::take_dropped`]). [`Game::input`]
    /// queues one for its frame. A bare [`Input`] is the crown's.
    ///
    /// # Errors
    ///
    /// A run the decode dispatches that stops the game, or a world
    /// refusal.
    pub fn apply(&mut self, order: impl Into<Order>) -> Result<(), Stop> {
        let order = order.into();

        match self.check(order.player, &order.input) {
            Ok(()) => self.decode(order),
            Err(why) => {
                let frame = self.frames + 1;

                self.dropped.push(check::Dropped { frame, order, why });
                Ok(())
            }
        }
    }

    /// [`Game::apply`]'s decode, unchecked.
    #[allow(clippy::too_many_lines, reason = "an arm an input")]
    fn decode(&mut self, order: Order) -> Result<(), Stop> {
        let Order { player, input } = order;

        // The decode's dispatches see this frame's clock, 50 (f + 1) for
        // err.log's frame f. **Game**: a thread one starts takes it as
        // `last`: the free probe's first explore flag, decoded in frame
        // 1698, polls first in 1709 and 1719, `#Flag_Cycle` 500 apart, and
        // play's wizards' guild upgrade, `magical_upgrade` at 1, first runs
        // two frames after its decode.
        let now = (self.frames + 1) * runtime::MS_PER_FRAME;

        self.world_mut().set_now(now);

        // Decoded in the frame before the next.
        let decoded = self.frames;
        let program = self.vm.program();

        match input {
            Input::Unpreview(code) => self.unpreview(code, player)?,
            Input::Build(order) => self.build(&order)?,
            Input::Recruit(r) => {
                let grave = self.world_mut().hire_now(Hire {
                    guild: r.guild,
                    code: r.code,
                });

                if let Some(id) = grave {
                    self.dispatch(DISMISS, &[Value::Agent(id)])?;
                }
            }
            Input::Flag(flag) => {
                let made = self.world_mut().flag(program, &flag);

                self.announce(made, None, NULL_AGENT)?;
            }
            Input::Cast(cast) => {
                let made = self.world_mut().cast(program, &cast);
                let script = self.world().script_of(cast.code);

                self.announce(made, script, NULL_AGENT)?;
            }
            Input::Bless(bless) => {
                let blessed = self
                    .world_mut()
                    .bless(program, &bless)
                    .map_err(|f| faulted(&f))?;

                match blessed {
                    Some(Blessed::Script(f, agent)) => {
                        self.dispatch(f, &[Value::Agent(agent)])?;
                    }
                    // Its callback only where the engine gives the target.
                    Some(Blessed::Made(id, target)) => {
                        let world = self.world();
                        let script = target.and_then(|_| world.script_of(world.piece(id)?.code));

                        self.announce(Ok(Some(id)), script, target.unwrap_or(NULL_AGENT))?;
                    }
                    None => {}
                }
            }
            Input::Upgrade(id) => self.act(&action::UPGRADE, id)?,
            Input::Research(r) => self.world_mut().research_later(
                program,
                Research {
                    frame: decoded,
                    ..r
                },
            ),
            Input::Enchant(id) => self.act(&action::ENCHANTMENT, id)?,
            Input::Alter(a) => self.world_mut().alter(&a),
            Input::Resource(r) => self.world_mut().resource(&r),
            Input::CampBegin(guild) => self.world_mut().camp_begin(program, player, guild),
            Input::CampLeave => self.world_mut().camp_leave(player),
            Input::Camp(c) => {
                self.world_mut().camp_sent(player);
                self.camp(&c)?;
            }
            Input::Embassy(id) => self.embassy(id)?,
            Input::Cheat(c) => self.cheat(c)?,
            Input::Resurrect(r) => {
                if self.world_mut().resurrect(&r) {
                    self.dispatch(RESURRECT_BEGIN, &[Value::Agent(r.hero)])?;
                }
            }
            Input::Read(handle) => {
                self.world_mut().read_message(handle);
            }
            Input::Unflag(handle) => self.unflag(handle)?,
            Input::Demolish(id) => {
                for (name, args) in self.world_mut().demolish(id) {
                    self.dispatch(&name, &args)?;
                }
            }
            Input::Toggle(t) => self.world_mut().toggle(&t),
            Input::Dismiss(id) => self.act(&action::DISMISS, id)?,
            Input::Clear(handle) => {
                if let Some(id) = self.world().gravestone(handle) {
                    self.dispatch(GRAVESTONE, &[Value::Agent(id)])?;
                }
            }
            Input::Assemble(id) => self.act(&action::ASSEMBLE, id)?,
            Input::MarketDay(id) => self.act(&action::MARKET, id)?,
            Input::Extort(id) => self.act(&action::EXTORT, id)?,
            Input::Rename(r) => self.world_mut().rename(&r),
            Input::Rage(id) => self.act(&action::RAGE_OF_KROLM, id)?,
            Input::Bounty(b) => self.bounty(&b),
            Input::Gamble(id) => self.world_mut().spin(program, id),
            Input::NewBet(id) => self.world_mut().new_bet(id),
            Input::Set(s) => self.world_mut().assign(&s),
        }

        Ok(())
    }

    /// The player's order dispatching `action`'s script on agent `id`,
    /// where its check allows; the world's steps around it are the
    /// action's ([`action`]).
    fn act(&mut self, action: &Action, id: u32) -> Result<(), Stop> {
        if action.allows(self.world_mut(), id) {
            self.dispatch(action.script, &[Value::Agent(id)])
        } else {
            Ok(())
        }
    }

    /// Cheat apply: the world's change, then its dispatches.
    /// `RESTORATION` dispatches to each damaged building, then mends the
    /// mobiles.
    fn cheat(&mut self, c: CheatCode) -> Result<(), Stop> {
        let world = self.world_mut();
        let dispatches = if c.cheat == Cheat::Restore {
            world
                .damaged_buildings(c.arg)
                .into_iter()
                .map(|id| (cheat::REACHED_MAX.to_owned(), vec![Value::Agent(id)]))
                .collect()
        } else {
            world.cheat(&c)
        };

        for (name, args) in dispatches {
            self.dispatch(&name, &args)?;
        }

        if c.cheat == Cheat::Restore {
            self.world_mut().mend_mobiles(c.arg);
        }

        Ok(())
    }

    /// Guild action apply case 0: the camp moves where the search
    /// finds room, between its two dispatches.
    fn camp(&mut self, camp: &Camp) -> Result<(), Stop> {
        match self.world().camp_spot(camp) {
            Some(spot) => {
                let guild = [Value::Agent(camp.guild)];

                self.dispatch(KICKOUT, &guild)?;
                self.world_mut().move_camp(camp.guild, spot);
                self.dispatch(MOVE_CAMP, &guild)?;
                self.world_mut().camp_placed(camp.guild);
                Ok(())
            }
            None => Ok(()),
        }
    }

    /// Guild action apply case `0x16` on embassy `id`, with an
    /// agent: shut if open; else `GS_OrderProcessor`, its
    /// `Embassy_Init` arming the timer as it ends, and open.
    fn embassy(&mut self, id: u32) -> Result<(), Stop> {
        let world = self.world_mut();

        if world.agent(id).is_none() {
            Ok(())
        } else if world.embassy_open(id) {
            world.embassy_close(id);
            Ok(())
        } else {
            self.dispatch(EMBASSY_INIT, &[Value::Agent(id)])?;
            self.world_mut().embassy_opened(id);
            Ok(())
        }
    }

    /// A piece the player's input made: its `NewUnitInit`, then its
    /// `PRIM` callback where it has one, actor the piece and `target` the
    /// target; its agent joins the queues after.
    fn announce(
        &mut self,
        made: Result<Option<u32>, crate::gpl::vm::Fault>,
        script: Option<Script>,
        target: u32,
    ) -> Result<(), Stop> {
        let made = made.map_err(|f| faulted(&f))?;

        if let Some(id) = made {
            self.introduce(id, &[NEW_UNIT])?;

            let args = script.as_ref().and_then(|s| {
                let (x, y) = self.world().piece(id)?.place?.fixed;

                script_args(s.kind, Some(id), Some(target), (pos32(x), pos32(y)))
            });

            if let (Some(s), Some(args)) = (script, args) {
                self.dispatch(&s.function, &args)?;
            }
        }

        self.join_made();
        Ok(())
    }

    /// The heroes whose recruit's timer ended: recruit done, in its
    /// guild's ORDR turn, where the orders stopped ([`World::arrived`]).
    fn arrivals(&mut self) -> Result<(), Stop> {
        let program = self.vm.program();

        for r in self.world_mut().arrived() {
            let made = self
                .world_mut()
                .recruit(program, &r)
                .map_err(|f| faulted(&f))?;

            if let Some(id) = made {
                self.introduce(id, &[GENERATE, NEW_UNIT])?;
            }
        }

        Ok(())
    }

    /// `player`'s preview's agent, made as the menu made it, then
    /// `UnitDelete` on its id, an integer, as the engine dispatches it;
    /// then gone.
    fn unpreview(&mut self, code: FourCc, player: u32) -> Result<(), Stop> {
        let program = self.vm.program();
        let id = self
            .world_mut()
            .preview(program, code, player)
            .map_err(|f| faulted(&f))?;

        self.host.core_mut().adopt_unqueued(&mut self.vm, id);
        self.dispatch(UNIT_DELETE, &[Value::Integer(PREVIEW_ID)])?;
        self.world_mut().delete(id);
        Ok(())
    }

    /// `DESTROY_REWARD_FLAG_MESSAGE` on the flag of `handle`
    /// ([`World::take_down`]): its `UnitDelete` at once, then the flags
    /// left on its holder stack again.
    fn unflag(&mut self, handle: u32) -> Result<(), Stop> {
        if let Some((id, on)) = self.world().take_down(handle) {
            self.dispatch(UNIT_DELETE, &[Value::Integer(id.cast_signed())])?;

            if let Some(target) = on {
                self.world_mut().restack(target);
            }
        }

        Ok(())
    }

    /// The placed building, then its `NewUnitInit`; its agent joins the
    /// queues after, as the engine's dispatches do between passes.
    fn build(&mut self, order: &BuildOrder) -> Result<(), Stop> {
        let program = self.vm.program();
        let built = self
            .world_mut()
            .build(program, order)
            .map_err(|f| faulted(&f))?;

        // A ruin the placement cleared: the units inside it go first.
        for (name, args) in self.world_mut().take_queued() {
            self.dispatch(&name, &args)?;
        }

        if let Some(id) = built {
            self.introduce(id, &[NEW_UNIT])?;
        }

        self.join_made();
        Ok(())
    }

    /// Agent `id`, just made: its attribute objects, then each of
    /// `scripts` on it, in order: [`NEW_UNIT`], a recruit's [`GENERATE`]
    /// before it.
    fn introduce(&mut self, id: u32, scripts: &[&str]) -> Result<(), Stop> {
        self.host.core_mut().adopt(&mut self.vm);

        for name in scripts {
            self.dispatch(name, &[Value::Agent(id)])?;
        }

        Ok(())
    }

    /// The agents made since the last call join the queues.
    fn join_made(&mut self) {
        for id in self.host.core_mut().drain_joined() {
            self.queues.join(id);
        }
    }

    /// New game setup at game time 0, `quest` the quest's script.
    ///
    /// # Errors
    ///
    /// The first line, answer or run that is not the engine's.
    pub fn setup(&mut self, quest: &str) -> Result<(), Stop> {
        self.setup_from(quest, runtime::SETUP_TIME)
    }

    /// [`Game::setup`] at game time `ms`, a whole frame's, the frames run
    /// set to match, as a load sets them: a scene's start
    /// ([`crate::sim::scene`]). **Inferred**: the engine sets up at 0 only.
    ///
    /// # Errors
    ///
    /// As [`Game::setup`].
    pub fn setup_from(&mut self, quest: &str, ms: u32) -> Result<(), Stop> {
        let program = self.vm.program();

        // The scatter before the setup's spawns clear any (`World::start_frames`).
        let standing = self.world().board().scatter().to_vec();

        self.world_mut().set_now(ms);
        self.dispatch(INITIALIZE, &[])?;

        let units = self
            .world_mut()
            .unit_pass(program)
            .map_err(|f| Stop(format!("unit pass: {f:?}")))?;

        self.host.core_mut().adopt(&mut self.vm);

        let first = self.world().next_id();

        if let Some(MapEntry::Script(_)) = program.entry(quest) {
            self.dispatch(quest, &[])?;
        }

        // `GS_Level` visits every unit, so a unit the quest's script
        // spawned is announced again, after the start's. **Game**: The
        // Barren Waste's twenty chests, `NewUnitInit` inside each
        // `SpawnUnit` and again in the pass.
        let spawned: Vec<u32> = (first..self.world().next_id())
            .filter(|id| self.world().piece(*id).is_some())
            .collect();

        for id in units.into_iter().chain(spawned) {
            self.dispatch(NEW_UNIT, &[Value::Agent(id)])?;
        }

        for id in self.world().palaces() {
            self.dispatch(PALACE_PRE_BIRTH, &[Value::Agent(id)])?;
        }

        self.world_mut().start_frames(&standing);
        self.world_mut().start_sets();

        self.queues = Queues::new(self.host.core_mut().drain_joined());
        self.swept = None;
        self.frames = ms / runtime::MS_PER_FRAME;
        Ok(())
    }

    /// One game frame: the clock steps by `runtime::MS_PER_FRAME`, then
    /// gpl scheduler takes every unit, with no instruction budget
    /// (`docs/gpl/runtime.md`). None once every human's game has ended
    /// ([`World::humans_ended`]): `DeclareLoss` puts the app in state 8,
    /// where game frame step steps no clock and runs no orders or threads.
    /// **Inferred**: no input either; the defeat screen sends none.
    ///
    /// # Errors
    ///
    /// As [`Game::setup`].
    pub fn frame(&mut self) -> Result<(), Stop> {
        if self.world().humans_ended() {
            return Ok(());
        }

        self.inputs()?;

        let now = runtime::FIRST_FRAME + self.frames * runtime::MS_PER_FRAME;
        let mut queues = std::mem::take(&mut self.queues);
        let mut stop = None;

        self.frames += 1;
        self.world_mut().set_now(now);
        self.host
            .core_mut()
            .world_mut()
            .step(Some(&mut self.vm.rng));

        // The orders stop at each engine dispatch they make, an impact's or
        // a peak's function, which runs there, inside the order pass, as
        // script dispatch does; then the rest of the orders. So a
        // blow's rolls come before a later unit's launch draws.
        loop {
            for id in self.host.core_mut().drain_joined() {
                queues.join(id);
            }

            for ((name, args), scripted) in self.world_mut().take_between() {
                if !scripted || self.world().stands(&args) {
                    self.dispatch(&name, &args)?;
                }
            }

            // A recruit's timer ended in its guild's turn, which stopped there.
            self.arrivals()?;

            for id in self.host.core_mut().drain_joined() {
                queues.join(id);
            }

            if !self.world().paused() {
                break;
            }

            self.host
                .core_mut()
                .world_mut()
                .resume(Some(&mut self.vm.rng));
        }

        // Then `GE_Level`'s deletes.
        for (name, args) in self.world_mut().take_deletes() {
            self.dispatch(&name, &args)?;
        }

        self.leave(&mut queues);

        // `Halt` cleared engine flags bit 4: no scheduler pass.
        if self.world().gpl() == Gpl::Running {
            // gpl free deleted before the first unit and after each.
            self.host.core_mut().free_deleted(&mut self.vm);

            queues.frame(|q, unit| match (&stop, self.unit(unit, now)) {
                (Some(_), _) => ControlFlow::Break(()),
                (None, Ok(())) => {
                    for id in self.host.core_mut().drain_joined() {
                        q.join(id);
                    }

                    self.leave(q);
                    self.host.core_mut().free_deleted(&mut self.vm);

                    ControlFlow::Continue(())
                }
                (None, Err(d)) => {
                    stop = Some(d);
                    ControlFlow::Break(())
                }
            });
        }

        self.queues = queues;

        // A stop keeps the agent it stopped in.
        if stop.is_none() {
            self.running = None;
        }

        stop.map_or(Ok(()), Err)
    }

    /// What the frames left for the front end since the last call: cues,
    /// lines, shakes and the rest ([`World::take_output`]).
    pub fn take_output(&mut self) -> crate::sim::world::Output {
        self.world_mut().take_output()
    }

    /// What the world lost track of since the last call
    /// ([`World::take_lost`]).
    pub fn take_lost(&mut self) -> Vec<crate::sim::world::loss_log::Lost> {
        self.world_mut().take_lost()
    }

    /// An engine dispatch of `function` with `args` between frames, as a
    /// random event's or a callback's: what the tests drive scripts by.
    ///
    /// # Errors
    ///
    /// As [`Game::frame`].
    pub fn call(&mut self, function: &str, args: &[Value]) -> Result<(), Stop> {
        self.dispatch(function, args)
    }

    /// Agent threads run: each of the agent's threads, in name order;
    /// none while an order holds the agent (`World::holds`).
    fn unit(&mut self, id: u32, now: u32) -> Result<(), Stop> {
        self.running = Some(id);

        let threads = if self.world().holds(id) {
            Arc::from([])
        } else {
            self.thread_objs(id)
        };

        for &o in threads.iter() {
            threads::run(&mut self.vm, &mut self.host, o, now).map_err(stopped)?;
        }

        Ok(())
    }

    /// Gpl agent unlink: each agent its `UnitDelete` ended leaves
    /// the queues at once (`runtime::Queues::leave`).
    ///
    /// Skipped where no agent went and no unit joined since the last:
    /// scanning the queues after every unit was quadratic.
    fn leave(&mut self, queues: &mut Queues) {
        let seen = Some((self.world().agents_stamp(), queues.len()));

        if self.swept != seen {
            let gone: Vec<u32> = queues
                .units()
                .filter(|u| self.world().agent(*u).is_none())
                .collect();

            for id in gone {
                queues.leave(id);
            }

            self.swept = Some((self.world().agents_stamp(), queues.len()));
        }
    }

    /// Gpl dispatch of `name`, as the engine does it.
    fn dispatch(&mut self, name: &str, args: &[Value]) -> Result<(), Stop> {
        let program = self.vm.program();
        let halted = self.world().gpl() == Gpl::Halted;

        // Halted, unit gpl delete neither dispatches nor deletes.
        if halted && name.eq_ignore_ascii_case(UNIT_DELETE) {
            self.world_mut().unit_gone(args);
            Ok(())
        } else {
            self.world_mut().starts(program, name, args);

            let ran = self.vm.dispatch(&mut self.host, name, args);

            self.world_mut().ends(program, name, args);

            if let Ok(run) = &ran {
                self.world_mut().returned(name, args, &run.result);
            }

            self.host.core_mut().adopt(&mut self.vm);
            ran.map(|_| ()).map_err(stopped)
        }
    }

    /// Agent `id`'s attributes, by declared name, in the map's order: by
    /// name, ignoring case.
    #[must_use]
    pub fn attributes(&self, id: u32) -> Vec<(String, ObjId)> {
        self.attributes_by(id, |_| true)
    }

    /// The rules a hero's skill caps read: `GplAIRoot`
    /// by name, its `Deathmatch_Rules` where it has one, the value's
    /// first byte not 0.
    #[must_use]
    pub fn rules(&self) -> Rules {
        let core = self.host.core();
        let root = core.world().named_any(AI_ROOT);

        // Its attribute by folded name, where one of its members has the
        // name: what [`Game::attributes`] lists, without the list (it ran
        // each published frame).
        let declared = |id: &u32| {
            core.world().agent(*id).is_some_and(|a| {
                a.values
                    .iter()
                    .any(|m| m.name.eq_ignore_ascii_case(DEATHMATCH_RULES))
            })
        };
        let set = root
            .filter(declared)
            .and_then(|id| core.attributes().get(id, DEATHMATCH_RULES))
            .map(|o| self.vm.object(o));

        match set {
            Some(Value::Boolean(true)) => Rules::Deathmatch,
            Some(Value::Integer(n)) if n & 0xff != 0 => Rules::Deathmatch,
            _ => Rules::Standard,
        }
    }

    /// [`Game::threads`]' objects, kept until the agent or its
    /// attributes change (`host::ThreadLists`): building each unit's list
    /// each frame was a quarter of the step. With [`CHECK_THREADS`] set,
    /// checked against the list built anew.
    fn thread_objs(&mut self, id: u32) -> Arc<[ObjId]> {
        let stamp = self.world().agent_stamp(id);
        let kept = self.host.core().kept_threads(id, stamp);

        // The self-check, opt-in: it builds the list anyway.
        if let Some(kept) = kept.as_ref().filter(|_| CHECK_THREADS.on()) {
            assert_eq!(
                kept[..],
                self.thread_list(id)[..],
                "agent {id}'s kept thread list at frame {}",
                self.frames
            );
        }

        kept.unwrap_or_else(|| {
            let list = self.thread_list(id);

            self.host.core_mut().keep_threads(id, stamp, list.clone());
            list
        })
    }

    /// [`Game::threads`]' objects, built anew.
    fn thread_list(&self, id: u32) -> Arc<[ObjId]> {
        self.threads(id).into_iter().map(|(_, o)| o).collect()
    }

    /// Agent `id`'s threads in run order: its function-typed attributes.
    /// **Measured**.
    #[must_use]
    pub fn threads(&self, id: u32) -> Vec<(String, ObjId)> {
        let functions: Vec<String> = self
            .world()
            .agent(id)
            .map(|a| {
                a.values
                    .iter()
                    .filter(|m| m.ty == Kind::Function.code())
                    .map(|m| m.name.to_ascii_lowercase())
                    .collect()
            })
            .unwrap_or_default();

        self.attributes_by(id, |folded| functions.iter().any(|f| f == folded))
    }

    /// [`Game::attributes`] whose folded name `keep` takes. Each name is
    /// folded once: this runs for every unit every frame.
    fn attributes_by(&self, id: u32, keep: impl Fn(&str) -> bool) -> Vec<(String, ObjId)> {
        let core = self.host.core();
        let values = core.world().agent(id).map_or(&[][..], |a| &a.values[..]);
        let mut out: Vec<(String, &str, ObjId)> = values
            .iter()
            .filter_map(|m| {
                let folded = m.name.to_ascii_lowercase();
                let o = keep(&folded)
                    .then(|| core.attributes().get(id, &folded))
                    .flatten()?;

                Some((folded, m.name.as_str(), o))
            })
            .collect();

        // Stable: names alike but for case keep prototype order.
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out.into_iter()
            .map(|(_, name, o)| (name.to_owned(), o))
            .collect()
    }

    /// The scheduler's queues.
    #[must_use]
    pub const fn queues(&self) -> &Queues {
        &self.queues
    }

    /// The world.
    #[must_use]
    pub fn world(&self) -> &World {
        self.host.core().world()
    }

    /// Player `player`'s ended game goes on: the end panel's CONTINUE
    /// PLAYING (`World::play_on`).
    pub fn play_on(&mut self, player: u32) {
        self.world_mut().play_on(player);
    }

    /// Player `player` surrenders (`World::surrender`).
    pub fn surrender(&mut self, player: u32) {
        self.world_mut().surrender(player);
    }

    /// The VM, as the game left it.
    #[must_use]
    pub const fn vm(&self) -> &Vm<'w> {
        &self.vm
    }
}

/// To and from the save model.
mod snap {
    use super::*;
    use crate::save::{self as m, Refused, Saved};
    use std::result::Result;

    impl<'w> Game<'w> {
        /// The game at a frame's end, as a save holds it: VM, world and
        /// its own.
        ///
        /// # Errors
        ///
        /// A game mid-run: a frame that stopped inside a unit's threads
        /// ([`Game::running`]).
        pub fn snapshot(&self) -> Result<m::Snapshot, Refused> {
            let Game {
                vm,
                host,
                queues,
                frames,
                inputs,
                delay: _,
                running,
                seed,
                dropped: _,
                swept: _,
            } = self;
            let Live {
                core,
                calls: _,
                faults: _,
                reports,
            } = host;

            if let Some(id) = running {
                Err(Refused::MidRun(*id))?;
            }

            // Lists keep their nodes' identity; numbered in this order.
            Ok(crate::gpl::ops::numbered(
                crate::gpl::ops::Way::Saving,
                || m::Snapshot {
                    version: m::VERSION,
                    vm: vm.saved(),
                    world: core.world().saved(),
                    game: m::Game {
                        frames: *frames,
                        queues: queues.save(),
                        inputs: inputs
                            .iter()
                            .filter(|(f, _)| *f > *frames)
                            .map(|(f, o)| (*f, o.to_string()))
                            .collect(),
                        host: m::Host {
                            attributes: core.attributes().save(),
                            seed: *seed,
                            reports: m::Reports {
                                count: reports.count as u64,
                                first: reports.first.clone(),
                            },
                        },
                    },
                },
            ))
        }

        /// The saved game going on, on `world` made as the saved one was
        /// (its dataset and setup), the saved state taken over it.
        ///
        /// # Errors
        ///
        /// A snapshot of another version, or naming what this build or
        /// program lacks.
        pub fn resumed(
            mut vm: Vm<'w>,
            world: impl Into<Held<'w>>,
            snapshot: m::Snapshot,
        ) -> Result<Self, Refused> {
            let m::Snapshot {
                version: _,
                vm: kept,
                world: saved,
                game,
            } = snapshot.checked()?;

            vm.set_trace(None);

            let mut resumed = Game::begin(vm, Live::new(world), game.host.seed, Start::Loaded);

            crate::gpl::ops::numbered(crate::gpl::ops::Way::Loading, || {
                resumed.vm.restore(kept)?;
                resumed.world_mut().restore(saved)?;
                resumed.restore_saved(game)
            })?;

            Ok(resumed)
        }

        /// The game's own state from `game`.
        fn restore_saved(&mut self, game: m::Game) -> Result<(), Refused> {
            let inputs = game
                .inputs
                .into_iter()
                .map(|(f, text)| Ok((f, text.parse::<Order>().map_err(Refused::Input)?)))
                .collect::<Result<_, Refused>>()?;

            self.frames = game.frames;
            self.queues = Queues::load(game.queues)?;
            self.swept = None;
            self.inputs = inputs;
            self.host
                .core
                .set_attributes(AttrObjs::load(game.host.attributes)?);
            self.host.reports.count = usize::load(game.host.reports.count)?;
            self.host.reports.first = game.host.reports.first;
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::attrib::Key;
    use crate::gpl::load::Program;
    use crate::gpl::vm::Config;
    use crate::sim::world::guild::gamble::{COLOUR_KEY, GAMBLING_HALL};
    use crate::sim::world::{Piece, alter::Assign};

    /// A frame that stopped inside a unit's threads keeps the unit: the
    /// game is mid-run, and saves only once a frame ends whole.
    #[test]
    fn a_game_mid_run_refuses_a_snapshot() {
        let program = Program::load::<&[u8]>(&[]).expect("empty program");
        let vm = Vm::new(&program, Config::default());
        let mut game = Game::free(vm, 1, World::default());

        assert!(game.snapshot().is_ok());

        game.running = Some(3);

        assert!(game.snapshot().is_err());
    }

    const UNIT: u32 = 10;
    const WORD: Key = COLOUR_KEY;
    const FRAME: u32 = 3;

    /// A bare game whose world has [`UNIT`], the crown's Gambling Hall.
    fn with_unit<T>(test: impl FnOnce(&mut Game<'_>) -> T) -> T {
        let program = Program::load::<&[u8]>(&[]).expect("empty program");
        let vm = Vm::new(&program, Config::default());
        let mut world = World::default();
        let mut code = [0; 4];

        code[..3].copy_from_slice(&GAMBLING_HALL);
        world.add_test_piece(
            UNIT,
            Piece {
                owner: CROWN,
                code: FourCc(code),
                ..Piece::default()
            },
        );

        test(&mut Game::free(vm, 1, world))
    }

    /// Message queue run hands each message, in
    /// turn, to method or method or the decode: a change then a setting
    /// of one word leaves the setting. No panel sends both on one word,
    /// so the decode alone.
    #[test]
    fn an_alteration_lands_in_its_place_among_the_decodes() {
        let word = with_unit(|game| {
            let alter = Alter {
                frame: 0,
                agent: UNIT,
                key: WORD,
                change: 10,
                range: (0, 0),
            };
            let set = Set {
                agent: UNIT,
                key: WORD,
                value: 2,
            };

            game.decode(Input::Alter(alter).into()).expect("decoded");
            game.decode(Input::Set(set).into()).expect("decoded");
            game.world().piece(UNIT).map(|p| p.word(WORD))
        });

        assert_eq!(word, Some(2));
    }

    /// The fast bar's setting, sent in the last frame's step, lands
    /// before the frame's inputs: a click's setting of the word after it
    /// stays.
    #[test]
    fn a_setting_sent_in_the_step_lands_before_the_inputs() {
        let word = with_unit(|game| {
            let cleared = Assign {
                frame: FRAME - 2,
                agent: UNIT,
                words: vec![(WORD.packed(), 0)],
            };
            let set = Set {
                agent: UNIT,
                key: WORD,
                value: 2,
            };

            game.world_mut().assign_later(cleared);
            game.input(FRAME, Input::Set(set)).expect("on time");

            for _ in 0..FRAME {
                game.frame().expect("frame runs");
            }

            game.world().piece(UNIT).map(|p| p.word(WORD))
        });

        assert_eq!(word, Some(2));
    }

    /// Player `player`'s setting of [`WORD`] to `value`.
    const fn setting(player: u32, value: i32) -> Order {
        Order {
            player,
            input: Input::Set(Set {
                agent: UNIT,
                key: WORD,
                value,
            }),
        }
    }

    /// A frame's orders decode by player, each player's in the order
    /// given, however they arrived: the crown's setting stays, and the
    /// orders of players no human seat are dropped in that order.
    #[test]
    fn a_frames_orders_decode_by_player_then_as_given() {
        let (word, dropped) = with_unit(|game| {
            for order in [setting(2, 7), setting(1, 4), setting(2, 9), setting(0, 1)] {
                game.input(FRAME, order).expect("on time");
            }

            for _ in 0..FRAME {
                game.frame().expect("frame runs");
            }

            (
                game.world().piece(UNIT).map(|p| p.word(WORD)),
                game.take_dropped(),
            )
        });
        let dropped: Vec<(Order, check::Refusal)> =
            dropped.into_iter().map(|d| (d.order, d.why)).collect();

        assert_eq!(word, Some(1));
        assert_eq!(
            dropped,
            [setting(1, 4), setting(2, 7), setting(2, 9)].map(|o| (o, check::Refusal::Seat))
        );
    }

    /// An order for a frame already run is refused, not dropped.
    #[test]
    fn an_order_for_a_frame_run_is_late() {
        with_unit(|game| {
            game.frame().expect("frame runs");

            assert_eq!(game.input(1, setting(0, 1)), Err(Late { frame: 1, ran: 1 }));
            assert_eq!(game.input(2, setting(0, 1)), Ok(()));
        });
    }

    /// Orders come before the next frame in single player, the delay's
    /// later in lockstep.
    #[test]
    fn the_order_frame_is_the_delay_ahead() {
        const DELAY: u32 = 4;

        let program = Program::load::<&[u8]>(&[]).expect("empty program");
        let solo = Game::free(Vm::new(&program, Config::default()), 1, World::default());
        let lockstep =
            Game::free(Vm::new(&program, Config::default()), 1, World::default()).with_delay(DELAY);

        assert_eq!(solo.order_frame(), SOLO_DELAY);
        assert_eq!(SOLO_DELAY, 1);
        assert_eq!(lockstep.order_frame(), DELAY);
    }

    /// A pending order keeps its player through a save.
    #[test]
    fn a_pending_order_keeps_its_player_in_a_snapshot() {
        let program = Program::load::<&[u8]>(&[]).expect("empty program");
        let mut game = Game::free(Vm::new(&program, Config::default()), 1, World::default());

        game.input(FRAME, setting(3, 1)).expect("on time");

        let saved = game.snapshot().expect("a snapshot").game.inputs;

        assert_eq!(saved, [(FRAME, format!("p3 set {UNIT} APB:46 1"))]);
    }
}
