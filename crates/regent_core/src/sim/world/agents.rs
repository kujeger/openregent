//! The GPL agents the world holds: by id and by name, the next id, those
//! made since the host last took them, and the attribute changes it is
//! still to write.
//!
//! ```text
//!  CreateAgent, a piece made ──> issue an id, insert, note made
//!  the host ──> take_made: agents to bind; take_rereads, take_removals:
//!               attributes to write or drop on its side
//!  UnitDelete ──> remove; delete also notes it, for the host to free
//!               its attribute objects (take_deleted)
//! ```

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::base::digest::Map;

use super::Agent;
use super::attributes::Reread;

/// Every GPL agent, by id and by name.
#[derive(Clone, Debug, Default)]
pub(super) struct Agents {
    /// Agents by id, each shared with a copy of the world until changed.
    by_id: Map<u32, Arc<Agent>>,
    /// Folded name to agent id: `CreateAgent`'s second argument.
    named: Map<String, u32>,
    /// The next agent id, gpl ai `0x34`.
    next_id: u32,
    /// Agents made since [`Agents::take_made`], in order.
    made: Vec<u32>,
    /// `UpgradeAgentAttributes` re-reads for the host to write.
    rereads: Vec<(u32, Vec<Reread>)>,
    /// GPL attributes `RemoveAttribute` took, for the host to drop.
    removals: Vec<(u32, String)>,
    /// Agents `UnitDelete` ended since [`Agents::take_deleted`], in
    /// order: gpl agent delete's list at root `0x80`.
    deleted: Vec<u32>,
    /// Changes at every insert and removal, unique in the process: what
    /// the queues' sweep of gone agents checks (`Game::leave`).
    stamp: u64,
    /// Each live agent's own stamp, new at each insert and change: what
    /// a cache of its thread list checks (`host::ThreadLists`).
    stamps: Map<u32, u64>,
}

/// The next [`Agents::stamp`]: unique, so a clone or a loaded world
/// never shares one with a changed world.
fn stamp() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);

    NEXT.fetch_add(1, Ordering::Relaxed)
}

impl Agents {
    /// None yet, the next id `first`.
    pub(super) fn new(first: u32) -> Agents {
        Agents {
            next_id: first,
            stamp: stamp(),
            ..Agents::default()
        }
    }

    /// Changes whenever an agent is inserted or removed.
    pub(super) const fn stamp(&self) -> u64 {
        self.stamp
    }

    /// Agent `id`, if alive.
    pub(super) fn get(&self, id: u32) -> Option<&Agent> {
        self.by_id.get(&id).map(|a| &**a)
    }

    /// Agent `id` to change, if alive: its own stamp new.
    pub(super) fn get_mut(&mut self, id: u32) -> Option<&mut Agent> {
        let agent = self.by_id.get_mut(&id).map(Arc::make_mut);

        if agent.is_some() {
            self.stamps.insert(id, stamp());
        }

        agent
    }

    /// Agent `id`'s own stamp, if alive: changes at each insert and
    /// change of it, and differs between worlds that changed apart.
    pub(super) fn stamp_of(&self, id: u32) -> Option<u64> {
        self.stamps.get(&id).copied()
    }

    /// Whether agent `id` is alive.
    pub(super) fn alive(&self, id: u32) -> bool {
        self.by_id.contains_key(&id)
    }

    /// Agent `id` is `agent` from now on.
    pub(super) fn insert(&mut self, id: u32, agent: Agent) {
        self.stamp = stamp();
        self.stamps.insert(id, stamp());
        self.by_id.insert(id, Arc::new(agent));
    }

    /// Agent `id` gone; its name, if any, stays mapped.
    pub(super) fn remove(&mut self, id: u32) {
        self.stamp = stamp();
        self.stamps.remove(&id);
        self.by_id.remove(&id);
    }

    /// Agent `id` ended by `UnitDelete`: gone, and noted for the host to
    /// free, where it lived.
    pub(super) fn delete(&mut self, id: u32) {
        self.stamp = stamp();
        self.stamps.remove(&id);

        if self.by_id.remove(&id).is_some() {
            self.deleted.push(id);
        }
    }

    /// The agents deleted since the last call, in order of deletion.
    pub(super) fn take_deleted(&mut self) -> Vec<u32> {
        std::mem::take(&mut self.deleted)
    }

    /// The next id to issue.
    pub(super) const fn next_id(&self) -> u32 {
        self.next_id
    }

    /// The next id issued: the one after it is next.
    pub(super) const fn advance(&mut self) {
        self.next_id += 1;
    }

    /// Agent `id` announced: the next id at least the one after it.
    pub(super) fn announced(&mut self, id: u32) {
        self.next_id = self.next_id.max(id + 1);
    }

    /// Agent `id` made, for the host's next [`Agents::take_made`].
    pub(super) fn note_made(&mut self, id: u32) {
        self.made.push(id);
    }

    /// The agents made since the last call, in order of making.
    pub(super) fn take_made(&mut self) -> Vec<u32> {
        std::mem::take(&mut self.made)
    }

    /// `name` maps to agent `id`, folded.
    pub(super) fn name(&mut self, name: &str, id: u32) {
        self.named.insert(name.to_ascii_lowercase(), id);
    }

    /// The agent `name` maps to, any case, alive or not.
    pub(super) fn named_any(&self, name: &str) -> Option<u32> {
        self.named.get(&name.to_ascii_lowercase()).copied()
    }

    /// The live agent `name` maps to, any case.
    pub(super) fn named(&self, name: &str) -> Option<u32> {
        self.named
            .get(&name.to_ascii_lowercase())
            .filter(|id| self.alive(**id))
            .copied()
    }

    /// Every name and the agent it maps to, sorted.
    pub(super) fn names(&self) -> Vec<(String, u32)> {
        let mut named: Vec<(String, u32)> =
            self.named.iter().map(|(n, id)| (n.clone(), *id)).collect();

        named.sort();
        named
    }

    /// Re-reads of agent `id`'s members for the host to write.
    pub(super) fn reread(&mut self, id: u32, members: Vec<Reread>) {
        self.rereads.push((id, members));
    }

    /// Agent `id`'s attribute `name`, folded, for the host to drop.
    pub(super) fn removed(&mut self, id: u32, name: String) {
        self.removals.push((id, name));
    }

    /// The re-reads since the last call.
    pub(super) fn take_rereads(&mut self) -> Vec<(u32, Vec<Reread>)> {
        std::mem::take(&mut self.rereads)
    }

    /// The attributes dropped since the last call.
    pub(super) fn take_removals(&mut self) -> Vec<(u32, String)> {
        std::mem::take(&mut self.removals)
    }
}

/// To and from the save model.
mod persist {
    use super::*;
    use crate::save::{self as m, Refused, Saved};

    impl Saved for Agents {
        type Model = m::Agents;

        fn save(&self) -> m::Agents {
            m::Agents {
                by_id: self.by_id.save(),
                named: self.named.save(),
                next_id: self.next_id,
                made: self.made.clone(),
                rereads: self.rereads.save(),
                removals: self.removals.clone(),
                deleted: self.deleted.clone(),
            }
        }

        fn load(model: m::Agents) -> std::result::Result<Agents, Refused> {
            let by_id: Map<u32, Arc<Agent>> = Map::load(model.by_id)?;
            let stamps = by_id.keys().map(|id| (*id, stamp())).collect();

            Ok(Agents {
                by_id,
                named: Map::load(model.named)?,
                next_id: model.next_id,
                made: model.made,
                rereads: Vec::load(model.rereads)?,
                removals: model.removals,
                deleted: model.deleted,
                stamp: stamp(),
                stamps,
            })
        }
    }
}
