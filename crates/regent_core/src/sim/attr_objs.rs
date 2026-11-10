//! Each GPL attribute's object, by agent and folded name: the live host's
//! attribute map. Each agent holds its own few, so a name is asked of
//! those alone, in any case, with no folded copy made and no name hashed:
//! scripts and searches ask by the thousand a frame, many for a name the
//! agent lacks.
//!
//! ```text
//!  agent ──> its list: (name folded, object) ... ──> the one equal,
//!                                                    ignoring case
//! ```

use std::collections::BTreeMap;

use crate::base::digest::Map;
use crate::gpl::vm::ObjId;
use crate::save::{Refused, Saved};

/// One agent's attributes: each folded name and its object, in order of
/// insertion.
type Own = Vec<(String, ObjId)>;

/// Each attribute's object, by agent and name, any case.
#[derive(Clone, Debug, Default)]
pub struct AttrObjs(Map<u32, Own>);

/// Where `name`, any case, sits in `own`.
fn place(own: &Own, name: &str) -> Option<usize> {
    own.iter().position(|(n, _)| n.eq_ignore_ascii_case(name))
}

impl AttrObjs {
    /// Agent `agent`'s attribute `name`'s object, `name` in any case.
    #[must_use]
    pub fn get(&self, agent: u32, name: &str) -> Option<ObjId> {
        let own = self.0.get(&agent)?;

        place(own, name).map(|i| own[i].1)
    }

    /// Agent `agent`'s attribute `name` is object `o`: the object before,
    /// if any.
    pub fn insert(&mut self, agent: u32, name: &str, o: ObjId) -> Option<ObjId> {
        let own = self.0.entry(agent).or_default();

        if let Some(i) = place(own, name) {
            Some(std::mem::replace(&mut own[i].1, o))
        } else {
            own.push((name.to_ascii_lowercase(), o));
            None
        }
    }

    /// Agent `agent`'s attribute `name` gone: its object, if any. An agent
    /// left with none is dropped.
    pub fn remove(&mut self, agent: u32, name: &str) -> Option<ObjId> {
        let own = self.0.get_mut(&agent)?;
        let gone = place(own, name).map(|i| own.remove(i).1);

        if own.is_empty() {
            self.0.remove(&agent);
        }

        gone
    }

    /// Agent `agent`'s attributes: folded name and object, in no order.
    pub fn of(&self, agent: u32) -> impl Iterator<Item = (&str, ObjId)> {
        self.0
            .get(&agent)
            .into_iter()
            .flatten()
            .map(|(n, o)| (n.as_str(), *o))
    }
}

/// As the map by `(agent, folded name)` before it: the same save.
impl Saved for AttrObjs {
    type Model = BTreeMap<(u32, String), u32>;

    fn save(&self) -> Self::Model {
        self.0
            .iter()
            .flat_map(|(agent, own)| {
                own.iter()
                    .map(move |(n, o)| ((*agent, n.clone()), o.save()))
            })
            .collect()
    }

    fn load(model: Self::Model) -> Result<AttrObjs, Refused> {
        let mut objs = AttrObjs::default();

        for ((agent, name), o) in model {
            objs.0
                .entry(agent)
                .or_default()
                .push((name, ObjId::load(o)?));
        }

        Ok(objs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Asked in any case, an attribute is found, and gone once removed.
    #[test]
    fn an_attribute_is_found_in_any_case() {
        let mut map = AttrObjs::default();
        let o = ObjId::load(7).expect("an object");

        map.insert(3, "Type", o);

        assert_eq!(map.get(3, "TYPE"), Some(o));
        assert_eq!(map.get(3, "type"), Some(o));
        assert_eq!(map.get(4, "type"), None);
        assert_eq!(map.of(3).collect::<Vec<_>>(), vec![("type", o)]);
        assert_eq!(map.insert(3, "TYPE", o), Some(o));
        assert_eq!(map.of(3).count(), 1);
        assert_eq!(map.remove(3, "tYpE"), Some(o));
        assert_eq!(map.get(3, "Type"), None);
        assert!(map.0.is_empty());
    }

    /// A name longer than a hashed word, saved and loaded: the same map.
    #[test]
    fn a_long_name_survives_a_save() {
        let mut map = AttrObjs::default();
        let o = ObjId::load(9).expect("an object");

        map.insert(1, "Deathmatch_Rules", o);

        let loaded = AttrObjs::load(map.save()).expect("loaded");

        assert_eq!(
            map.save(),
            BTreeMap::from([((1, "deathmatch_rules".to_owned()), 9)])
        );
        assert_eq!(loaded.get(1, "DEATHMATCH_RULES"), Some(o));
    }
}
