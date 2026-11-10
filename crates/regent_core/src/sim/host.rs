//! The live host: GPL's builtins answered by the world alone, as in play.
//!
//! ```text
//!  Vm ─ builtin ─> Live ─ Checked::of_id ─> World::run ─> Answer
//!                                             Given:   its dispatches run nested
//!                                             Asks:    its script runs, result handed back
//!                                             Unknown: a Fallback
//!  Vm ─ attribute ─> Core: one VM object per agent attribute, as made
//! ```
//!
//! A [`Fallback`] is where the model does not know the engine's answer:
//! the result keeps its type's default (0, false, empty, the null agent),
//! counted in [`Faults`] and kept the first time per builtin.
//! **Inferred**: a stand-in, not the engine's answer.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::base::digest::{Map, Set};
use std::fmt;

use super::attr_objs::AttrObjs;
use super::world::step::Draws;
use super::world::threads::Logging;
use super::world::{Answer, Attributes, Dispatch, World};
use crate::gpl::builtins::BuiltinId;
use crate::gpl::ops::Value;

use crate::gpl::vm::host::{self, Call, Host};
use crate::gpl::vm::text::{DISPATCH_ERROR, THREAD_ERROR};
use crate::gpl::vm::{Fault, Handle, ObjId, SHARED_NULL, Vm};

/// A value a host or game runs on: borrowed, or owned so a game holds
/// all it plays on and frees it when dropped.
pub enum Held<'w, T = World> {
    Borrowed(&'w mut T),
    Owned(Box<T>),
}

impl<'w, T> From<&'w mut T> for Held<'w, T> {
    fn from(value: &'w mut T) -> Self {
        Held::Borrowed(value)
    }
}

impl From<World> for Held<'_> {
    fn from(world: World) -> Self {
        Held::Owned(Box::new(world))
    }
}

impl<T> std::ops::Deref for Held<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        match self {
            Held::Borrowed(v) => v,
            Held::Owned(v) => v,
        }
    }
}

impl<T> std::ops::DerefMut for Held<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        match self {
            Held::Borrowed(v) => v,
            Held::Owned(v) => v,
        }
    }
}

/// What any host keeps of the world for the VM: the world, each agent
/// attribute's object, and the agents made since the game last queued
/// them.
pub struct Core<'w> {
    world: Held<'w>,
    /// Each attribute's object, by agent and name. A change here drops
    /// its agent's thread list.
    attributes: AttrObjs,
    /// Agents made, in order, with their attribute objects.
    joined: Vec<u32>,
    /// Each agent's thread objects, as last listed.
    thread_lists: ThreadLists,
}

/// Each agent's thread objects in run order, kept while the agent
/// stands unchanged ([`World::agent_stamp`]) and its attributes do.
#[derive(Clone, Default)]
pub struct ThreadLists {
    /// Each list, with its agent's stamp when made.
    lists: Map<u32, (Option<u64>, Arc<[ObjId]>)>,
}

impl ThreadLists {
    /// Agent `id`'s list, if kept at its agent stamp `stamp`.
    fn get(&self, id: u32, stamp: Option<u64>) -> Option<Arc<[ObjId]>> {
        self.lists
            .get(&id)
            .filter(|(made, _)| *made == stamp)
            .map(|(_, list)| list.clone())
    }

    /// Keep agent `id`'s `list`, made at its agent stamp `stamp`.
    fn put(&mut self, id: u32, stamp: Option<u64>, list: Arc<[ObjId]>) {
        self.lists.insert(id, (stamp, list));
    }

    /// Agent `id`'s attributes changed: its list dropped.
    fn forget(&mut self, id: u32) {
        self.lists.remove(&id);
    }
}

impl<'w> Core<'w> {
    #[must_use]
    pub fn new(world: impl Into<Held<'w>>) -> Self {
        Core {
            world: world.into(),
            attributes: AttrObjs::default(),
            joined: Vec::new(),
            thread_lists: ThreadLists::default(),
        }
    }

    /// The world.
    #[must_use]
    pub fn world(&self) -> &World {
        &self.world
    }

    /// The world, to change.
    pub fn world_mut(&mut self) -> &mut World {
        &mut self.world
    }

    /// Each attribute's object, by agent and name.
    #[must_use]
    pub const fn attributes(&self) -> &AttrObjs {
        &self.attributes
    }

    /// Every attribute's object, as a save kept them.
    pub fn set_attributes(&mut self, attributes: AttrObjs) {
        self.attributes = attributes;
        self.forget_threads();
    }

    /// The agents made since the last call, in order, to join the queues.
    pub fn drain_joined(&mut self) -> std::vec::Drain<'_, u32> {
        self.joined.drain(..)
    }

    /// [`Core::adopt`], agent `id` left out of the queues: a preview's
    /// agent -6 is never queued.
    pub fn adopt_unqueued(&mut self, vm: &mut Vm<'_>, id: u32) {
        self.adopt(vm);
        self.joined.retain(|j| *j != id);
    }

    /// Agent `id`'s thread objects, if kept at its agent stamp `stamp`
    /// ([`World::agent_stamp`]).
    #[must_use]
    pub fn kept_threads(&self, id: u32, stamp: Option<u64>) -> Option<Arc<[ObjId]>> {
        self.thread_lists.get(id, stamp)
    }

    /// Keep agent `id`'s thread objects, made at its agent stamp `stamp`.
    pub fn keep_threads(&mut self, id: u32, stamp: Option<u64>, list: Arc<[ObjId]>) {
        self.thread_lists.put(id, stamp, list);
    }

    /// Give each agent the world made since the last call its attribute
    /// objects, as made.
    pub fn adopt(&mut self, vm: &mut Vm<'_>) {
        for id in self.world.take_made() {
            self.thread_lists.forget(id);

            let values = self
                .world
                .agent(id)
                .map(|a| a.values.clone())
                .unwrap_or_default();

            for m in values {
                let o = vm.alloc(m.value);

                self.attributes.insert(id, &m.name, o);
            }

            self.joined.push(id);
        }
    }

    /// Write what the world's `UpgradeAgentAttributes` re-read: a missing
    /// attribute added fresh, a member with text set through slot 7; a
    /// function takes the new function only, its thread kept.
    /// `AddAttribute`'s and `RemoveAttribute`'s too.
    pub fn reread(&mut self, vm: &mut Vm<'_>) {
        for (id, members) in self.world.take_reread() {
            self.thread_lists.forget(id);

            for m in members {
                let o = if let Some(o) = self.attributes.get(id, &m.fresh.name) {
                    o
                } else {
                    let o = vm.alloc(m.fresh.value);

                    self.attributes.insert(id, &m.fresh.name, o);
                    o
                };

                if let Some(v) = m.text {
                    *vm.object_mut(o) = v;
                }
            }
        }

        // `RemoveAttribute`'s.
        for key in self.world.take_removed() {
            self.thread_lists.forget(key.0);
            self.attributes.remove(key.0, &key.1);
        }
    }

    /// Gpl free deleted: each agent deleted since the last call,
    /// the last first, as it pops its list's back; the agent's destructor
    /// releases each attribute object.
    pub fn free_deleted(&mut self, vm: &mut Vm<'_>) {
        for id in self.world.take_deleted().into_iter().rev() {
            self.release_attributes(id, vm);
        }
    }

    /// Agent `id`'s attribute objects back to the pool, in name order,
    /// not the map's: the free list is a stack.
    pub fn release_attributes(&mut self, id: u32, vm: &mut Vm<'_>) {
        self.thread_lists.forget(id);

        let mut mine: Vec<(String, ObjId)> = self
            .attributes
            .of(id)
            .map(|(name, o)| (name.to_owned(), o))
            .collect();

        mine.sort_by(|a, b| a.0.cmp(&b.0));

        for (name, o) in mine {
            self.attributes.remove(id, &name);
            vm.release(o);
        }
    }

    /// The attribute map replaced whole: every kept thread list dropped.
    pub fn forget_threads(&mut self) {
        self.thread_lists.lists.clear();
    }

    /// `agent's "name"` from the world: gpl agentref attribute
    /// throws for a missing agent, then
    /// gpl agent attribute find for a missing
    /// attribute. **Inferred**: the message's last `%s` is the agent's
    /// name.
    ///
    /// # Errors
    ///
    /// The engine's throw for a missing agent or attribute.
    pub fn own_attribute(&self, agent: u32, name: &str) -> Result<ObjId, Fault> {
        match (self.attributes.get(agent, name), self.world.agent(agent)) {
            (Some(o), _) => Ok(o),
            (None, None) => Err(Fault::throw(host::NO_AGENT.replacen("%s", name, 1))),
            (None, Some(a)) => Err(Fault::throw(format!(
                "Tried to access non-existent attribute {name} in agent#{agent}, ({})",
                a.name
            ))),
        }
    }

    /// The world's answer to `call` ([`World::run`]), reading GPL
    /// attributes from their objects.
    ///
    /// # Errors
    ///
    /// What the engine's native throws.
    pub fn answer(
        &mut self,
        vm: &mut Vm<'_>,
        call: &Call<'_>,
        draws: Draws,
    ) -> Result<Answer, Fault> {
        let attributes = &self.attributes;
        let current: &Attributes<'_> = &|vm, id, n| attributes.get(id, n).map(|o| vm.object(o));

        self.world.run(vm, call, current, draws)
    }
}

/// The live host's answer where the world gave none
/// ([`Answer::Unknown`]: a place, sight or attribute the model does not
/// follow): the result at its type's default.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fallback {
    /// Game ms.
    pub now: u32,
    pub builtin: &'static str,
    /// The calling script function.
    pub site: Option<String>,
    /// The call's first agent argument, and its piece's name.
    pub agent: Option<u32>,
    pub name: Option<String>,
}

impl fmt::Display for Fallback {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let site = self
            .site
            .as_ref()
            .map_or_else(String::new, |s| format!(" in {s}"));
        let agent = match (self.agent, &self.name) {
            (Some(id), Some(name)) => format!("agent {id} ({name})"),
            (Some(id), None) => format!("agent {id}"),
            _ => "no agent".to_owned(),
        };

        write!(
            f,
            "unknown {} at {} ms{site}, for {agent}: the default taken",
            self.builtin, self.now
        )
    }
}

/// Every [`Fallback`] the live host took: counted per builtin, each kept
/// the first time per builtin until taken.
#[derive(Clone, Debug, Default)]
pub struct Faults {
    pub unknown: BTreeMap<&'static str, usize>,
    events: Vec<Fallback>,
    seen: Set<&'static str>,
}

impl Faults {
    /// The fallbacks kept since the last call, oldest first.
    pub fn take(&mut self) -> Vec<Fallback> {
        std::mem::take(&mut self.events)
    }

    /// How many fallbacks so far.
    #[must_use]
    pub fn count(&self) -> usize {
        self.unknown.values().sum::<usize>()
    }

    /// Count `call`'s fallback; keep it the first time.
    fn note(&mut self, vm: &Vm<'_>, world: &World, call: &Call<'_>) {
        let name = call.builtin().name;

        *self.unknown.entry(name).or_default() += 1;

        if self.seen.insert(name) {
            let agent = first_agent(vm, call.args);

            self.events.push(Fallback {
                now: world.now(),
                builtin: name,
                site: vm.caller().map(str::to_owned),
                agent,
                name: agent.and_then(|id| world.piece(id)).map(|p| p.name.clone()),
            });
        }
    }
}

/// A call's first agent argument.
fn first_agent(vm: &Vm<'_>, args: &[Handle]) -> Option<u32> {
    args.iter().skip(1).find_map(|h| match vm.value(*h) {
        Some(Value::Agent(id)) => Some(*id),
        _ => None,
    })
}

/// A builtin's result slot at its type's default: 0, false, empty, the
/// null agent.
pub fn defaulted(vm: &mut Vm<'_>, call: &Call<'_>) {
    if let Some(o) = call.args.first().and_then(|h| vm.get(*h))
        && o != SHARED_NULL
    {
        let fresh = Value::fresh(vm.object(o).kind());

        *vm.object_mut(o) = fresh;
    }
}

/// Script errors reported in play: how many, and the first
/// [`KEEP_REPORTS`] in full.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Reports {
    pub count: usize,
    pub first: Vec<String>,
}

/// How many script errors [`Reports`] keeps in full.
pub const KEEP_REPORTS: usize = 8;

/// Answered calls, counted by builtin id: one index a call, not a
/// lookup by name.
#[derive(Clone, Debug, Default)]
pub struct Calls(Vec<usize>);

impl Calls {
    /// One more call of `builtin`.
    fn note(&mut self, builtin: BuiltinId) {
        let i = builtin.index();

        if i >= self.0.len() {
            self.0.resize(i + 1, 0);
        }

        self.0[i] += 1;
    }

    /// The counts by builtin name, each called builtin once.
    #[must_use]
    pub fn by_name(&self) -> BTreeMap<&'static str, usize> {
        let mut out = BTreeMap::new();
        let called = (0..).map(BuiltinId).zip(&self.0).filter(|(_, n)| **n > 0);

        for (id, n) in called {
            *out.entry(id.get().name).or_default() += n;
        }

        out
    }
}

/// The host of a game the world runs alone.
pub struct Live<'w> {
    pub core: Core<'w>,
    /// Answered calls, per builtin.
    pub calls: Calls,
    pub faults: Faults,
    pub reports: Reports,
}

impl<'w> Live<'w> {
    #[must_use]
    pub fn new(world: impl Into<Held<'w>>) -> Self {
        Live {
            core: Core::new(world),
            calls: Calls::default(),
            faults: Faults::default(),
            reports: Reports::default(),
        }
    }

    /// The fallback for `call`: counted, kept, the result its default.
    fn fall_back(&mut self, vm: &mut Vm<'_>, call: &Call<'_>) {
        self.faults.note(vm, self.core.world(), call);
        defaulted(vm, call);
    }

    /// Everything a run may change here, kept for [`Live::put_back`].
    #[must_use]
    pub fn keep(&self) -> Kept {
        Kept {
            world: (*self.core.world).clone(),
            attributes: self.core.attributes.clone(),
            thread_lists: self.core.thread_lists.clone(),
            joined: self.core.joined.clone(),
            calls: self.calls.clone(),
            faults: self.faults.clone(),
            reports: self.reports.clone(),
        }
    }

    /// The host as [`Live::keep`] kept it: its thread lists too, made
    /// for the attributes and agents put back, so the next frame need not
    /// list every agent's threads again.
    pub fn put_back(&mut self, kept: Kept) {
        let Kept {
            world,
            attributes,
            thread_lists,
            joined,
            calls,
            faults,
            reports,
        } = kept;

        *self.core.world = world;
        self.core.attributes = attributes;
        self.core.thread_lists = thread_lists;
        self.core.joined = joined;
        self.calls = calls;
        self.faults = faults;
        self.reports = reports;
    }
}

/// A live host's state as [`Live::keep`] took it.
pub struct Kept {
    world: World,
    attributes: AttrObjs,
    thread_lists: ThreadLists,
    joined: Vec<u32>,
    calls: Calls,
    faults: Faults,
    reports: Reports,
}

/// Run the scripts a native dispatches and reads before it answers
/// ([`Answer::Asks`]): each at once, nested in the builtin, its result
/// handed back, until the world answers.
///
/// # Errors
///
/// A nested run's crash.
fn ask_scripts(
    host: &mut Live<'_>,
    vm: &mut Vm<'_>,
    call: &Call<'_>,
    mut answer: Result<Answer, Fault>,
) -> Result<Result<Answer, Fault>, Fault> {
    while matches!(answer, Ok(Answer::Asks)) {
        let script = host.core.world_mut().take_ask();

        let Some((name, args)) = script else {
            host.core.world_mut().cancel_ask();
            answer = Ok(Answer::Unknown);
            break;
        };

        let run = vm.dispatch(host, &name, &args).map_err(Fault::Crash)?;

        host.core.adopt(vm);
        answer = host.core.world_mut().resume_ask(vm, call.args, &run.result);
    }

    Ok(answer)
}

/// The engine's dispatches from inside a builtin the world answered.
///
/// # Errors
///
/// As [`ask_scripts`].
fn run_queued(host: &mut Live<'_>, vm: &mut Vm<'_>, queued: Vec<Dispatch>) -> Result<(), Fault> {
    for (name, args) in queued {
        host.core.world_mut().starts(vm.program(), &name, &args);
        vm.dispatch(host, &name, &args).map_err(Fault::Crash)?;
        host.core.world_mut().ends(vm.program(), &name, &args);
        host.core.adopt(vm);
    }

    Ok(())
}

impl<'w> Live<'w> {
    /// The world and the state its builtins share.
    #[must_use]
    pub const fn core(&self) -> &Core<'w> {
        &self.core
    }

    pub const fn core_mut(&mut self) -> &mut Core<'w> {
        &mut self.core
    }
}

impl Host for Live<'_> {
    /// A world builtin by its [`Checked`](super::world::Checked): the world's answer, then what
    /// it dispatched inside it; an unknown answer a [`Fallback`]. A throw
    /// goes to the script. Every builtin the VM does not run has an answer
    /// (`every_builtin_has_an_answer`).
    fn builtin(&mut self, vm: &mut Vm<'_>, call: &Call<'_>) -> Result<(), Fault> {
        let answer = self.core.answer(vm, call, Draws::Lent);
        let answer = ask_scripts(self, vm, call, answer)?;

        self.core.reread(vm);
        self.core.adopt(vm);

        let queued = self.core.world_mut().take_queued();

        if matches!(answer, Ok(Answer::Unknown)) {
            // No builtin queues a dispatch and then answers unknown.
            debug_assert_eq!(queued.len(), 0);

            self.fall_back(vm, call);
        } else {
            self.calls.note(call.builtin);
            run_queued(self, vm, queued)?;
        }

        answer.map(|_| ())
    }

    fn attribute(&mut self, _vm: &mut Vm<'_>, agent: u32, name: &str) -> Result<ObjId, Fault> {
        self.core.own_attribute(agent, name)
    }

    fn named(&mut self, name: &str) -> Value {
        Value::Integer(host::registry(name).unwrap_or_default())
    }

    /// A script error, counted and the first kept; nothing after
    /// `NoDebugMessages`.
    fn log(&mut self, line: &str) {
        let report = line.starts_with(THREAD_ERROR) || line.starts_with(DISPATCH_ERROR);

        if report && self.core.world().logging() == Logging::On {
            self.reports.count += 1;

            if self.reports.first.len() < KEEP_REPORTS {
                self.reports.first.push(line.to_owned());
            }
        }
    }

    fn agent_name(&mut self, id: u32) -> Option<String> {
        self.core.world().agent_name(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpl::builtins;
    use crate::gpl::load::Program;
    use crate::gpl::vm::Config;

    /// Calls counted by id read back by name, each called builtin once.
    #[test]
    fn calls_are_counted_by_builtin() {
        let ids = [BuiltinId(0), BuiltinId(1)];
        let mut calls = Calls::default();

        calls.note(ids[0]);
        calls.note(ids[1]);
        calls.note(ids[0]);

        assert_eq!(
            calls.by_name(),
            BTreeMap::from([(ids[0].get().name, 2), (ids[1].get().name, 1)])
        );
    }

    /// An agent the empty world may not have made yet: its place unknown.
    const UNSEEN: u32 = 7;

    /// `LocationOf` of an agent the world cannot place: the result keeps
    /// its type's default, counted each time and kept once.
    #[test]
    fn an_unknown_answer_falls_back_to_the_default_counted_and_kept_once() {
        let program = Program::load::<&[u8]>(&[]).expect("empty program");
        let mut vm = Vm::new(&program, Config::default());
        let mut live = Live::new(World::default());
        let builtin = builtins::id("LocationOf").expect("a builtin");
        let slot = vm.alloc(Value::Coord(5, 5));
        let agent = vm.alloc(Value::Agent(UNSEEN));
        let handles = [vm.top_handle(Some(slot)), vm.top_handle(Some(agent))];
        let call = Call {
            builtin,
            args: &handles,
        };

        for _ in 0..2 {
            live.builtin(&mut vm, &call).expect("no throw");
        }

        let kept = live.faults.take();

        assert_eq!(
            *vm.object(slot),
            Value::fresh(crate::data::gpltype::Kind::Coord)
        );
        assert_eq!(live.faults.unknown.get("LocationOf"), Some(&2));
        assert_eq!(live.faults.count(), 2);
        assert_eq!(kept.len(), 1, "{kept:?}");
        assert_eq!(kept[0].agent, Some(UNSEEN));
        assert!(
            kept[0].to_string().starts_with("unknown LocationOf"),
            "{}",
            kept[0]
        );
    }
}
