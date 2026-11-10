//! `GplList`: its nodes, and their numbering across a save.

use crate::base::digest::Map;
use std::cell::RefCell;
use std::fmt;
use std::ops::Deref;
use std::sync::atomic::{AtomicU64, Ordering as Atomic};

use super::{Value, reset};
use crate::save::Refused;

/// A node of a [`List`]: which element, not where it is.
pub type Node = u64;

/// Node ids, never reused, so a freed node never matches a later one.
static NODES: AtomicU64 = AtomicU64::new(0);

pub(crate) fn fresh_node() -> Node {
    NODES.fetch_add(1, Atomic::Relaxed)
}

/// A `GplList`: a `std::list`, one node per element.
///
/// **Binary**: `+=` and `<<` insert a new node before the head;
/// `-=` and `RemoveListMember` erase
/// nodes; slot 6 and `ClearList` free them
/// all. A node keeps its id while in the list, so a `foreach` cursor holds
/// one. Equality and `Debug` see the elements only.
///
/// A freed node is still read by a cursor on it (list iter deref,
/// list iter next test only for the sentinel): its value as
/// released, its link as when freed. **Inferred**: the heap leaves a freed
/// node's bytes as they were until reused; the last [`FREED_KEPT`] are
/// kept.
#[derive(Clone, Default)]
pub struct List {
    items: Vec<Value>,
    nodes: Vec<Node>,
    freed: Vec<Freed>,
}

/// Freed nodes a [`List`] keeps for a cursor to read, at least the last
/// erase's. **Inferred**: a model bound; the engine's heap reuses a freed
/// node at its next allocation of that size.
pub const FREED_KEPT: usize = 32;

/// A node freed from a [`List`]: what a cursor on it reads.
#[derive(Clone, Debug)]
struct Freed {
    node: Node,
    /// The element after gpl value release: its class's default.
    value: Value,
    /// The node after it when freed; `None` the sentinel.
    next: Option<Node>,
}

impl List {
    #[must_use]
    pub const fn new() -> List {
        List {
            items: Vec::new(),
            nodes: Vec::new(),
            freed: Vec::new(),
        }
    }

    /// Append `v` as a new node.
    pub fn push(&mut self, v: Value) {
        self.items.push(v);
        self.nodes.push(fresh_node());
    }

    /// Erase every node.
    pub fn clear(&mut self) {
        self.retain(|_| false);
    }

    /// Erase the node at `index`.
    ///
    /// # Panics
    ///
    /// When `index` is out of bounds.
    pub fn remove(&mut self, index: usize) -> Value {
        let value = self.items[index].clone();
        let mut at = 0;

        self.retain(|_| {
            at += 1;
            at - 1 != index
        });

        value
    }

    /// Erase the nodes whose element `keep` refuses, first to last, as
    /// `-=` and list erase range do: each freed node links to the
    /// one after it then.
    pub fn retain(&mut self, mut keep: impl FnMut(&Value) -> bool) {
        let kept: Vec<bool> = self.items.iter().map(&mut keep).collect();

        let freed: Vec<Freed> = kept
            .iter()
            .enumerate()
            .filter(|&(_, &k)| !k)
            .map(|(i, _)| {
                let mut value = self.items[i].clone();
                reset(&mut value);

                Freed {
                    node: self.nodes[i],
                    value,
                    next: self.nodes.get(i + 1).copied(),
                }
            })
            .collect();

        if !freed.is_empty() {
            let mut flags = kept.iter();
            self.items.retain(|_| *flags.next().unwrap_or(&true));
            let mut flags = kept.iter();
            self.nodes.retain(|_| *flags.next().unwrap_or(&true));

            // Oldest out first, never this erase's own.
            let keep = FREED_KEPT.max(freed.len());
            self.freed.extend(freed);
            let excess = self.freed.len().saturating_sub(keep);
            self.freed.drain(..excess);
        }
    }

    /// The node at `index`.
    #[must_use]
    pub fn node(&self, index: usize) -> Option<Node> {
        self.nodes.get(index).copied()
    }

    /// Where `node` is, if still in the list.
    #[must_use]
    pub fn position(&self, node: Node) -> Option<usize> {
        self.nodes.iter().position(|&n| n == node)
    }

    /// What a cursor on `node` reads: its value and the node after, `None`
    /// the sentinel. A freed node gives what it was left holding; `None`
    /// for a node this list never held or no longer keeps.
    #[must_use]
    pub fn visit(&self, node: Node) -> Option<(&Value, Option<Node>)> {
        self.position(node)
            .map(|i| (&self.items[i], self.node(i + 1)))
            .or_else(|| {
                self.freed
                    .iter()
                    .rev()
                    .find(|f| f.node == node)
                    .map(|f| (&f.value, f.next))
            })
    }
}

impl Deref for List {
    type Target = [Value];

    fn deref(&self) -> &[Value] {
        &self.items
    }
}

impl Extend<Value> for List {
    fn extend<I: IntoIterator<Item = Value>>(&mut self, iter: I) {
        iter.into_iter().for_each(|v| self.push(v));
    }
}

impl FromIterator<Value> for List {
    fn from_iter<I: IntoIterator<Item = Value>>(iter: I) -> List {
        let mut list = List::new();
        list.extend(iter);
        list
    }
}

impl From<Vec<Value>> for List {
    fn from(items: Vec<Value>) -> List {
        items.into_iter().collect()
    }
}

impl PartialEq for List {
    fn eq(&self, other: &List) -> bool {
        self.items == other.items
    }
}

impl Eq for List {}

impl fmt::Debug for List {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(&self.items).finish()
    }
}

/// To and from the save model.
mod snap {
    use super::*;
    use crate::save::{self as m, Saved};
    use std::result::Result;

    impl Saved for List {
        type Model = m::List;

        fn save(&self) -> m::List {
            let List {
                items,
                nodes,
                freed,
            } = self;

            m::List {
                items: items.save(),
                nodes: nodes.iter().map(|&n| node(n)).collect(),
                freed: freed
                    .iter()
                    .map(|f| m::Freed {
                        node: node(f.node),
                        value: f.value.save(),
                        next: f.next.map(node),
                    })
                    .collect(),
            }
        }

        fn load(model: m::List) -> Result<List, Refused> {
            let freed = model
                .freed
                .into_iter()
                .map(|f| {
                    Ok(Freed {
                        node: unnode(f.node)?,
                        value: Value::load(f.value)?,
                        next: f.next.map(unnode).transpose()?,
                    })
                })
                .collect::<Result<_, Refused>>()?;

            Ok(List {
                items: Vec::load(model.items)?,
                nodes: model
                    .nodes
                    .into_iter()
                    .map(unnode)
                    .collect::<Result<_, _>>()?,
                freed,
            })
        }
    }
}

/// How list nodes are numbered while a snapshot is taken or restored.
enum Nodes {
    Off,
    /// A node's id to its number, in the order met.
    Saving(Map<Node, u32>),
    /// A number to the fresh node standing for it.
    Loading(Map<u32, Node>),
}

thread_local! {
    static NUMBERING: RefCell<Nodes> = const { RefCell::new(Nodes::Off) };
}

/// Which way [`numbered`] numbers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Way {
    Saving,
    Loading,
}

/// A numbering in force on this thread; ends it when dropped, a panic
/// included.
struct Numbering;

impl Numbering {
    /// Start numbering `way`. Panics inside another numbering: the inner
    /// one would renumber the outer's nodes.
    fn start(way: Way) -> Numbering {
        let fresh = match way {
            Way::Saving => Nodes::Saving(Map::default()),
            Way::Loading => Nodes::Loading(Map::default()),
        };

        NUMBERING.with(|n| {
            let mut nodes = n.borrow_mut();

            assert!(
                matches!(*nodes, Nodes::Off),
                "list node numbering inside another"
            );
            *nodes = fresh;
        });

        Numbering
    }
}

impl Drop for Numbering {
    fn drop(&mut self) {
        NUMBERING.with(|n| *n.borrow_mut() = Nodes::Off);
    }
}

/// `f` with list nodes numbered one way: a node keeps its identity across
/// a save, as a `foreach` cursor left on one needs, and its number is its
/// order met, so one state saves as one snapshot.
///
/// Node ids come from [`fresh_node`]'s process-wide counter, so a
/// restored node is a new id standing for its number. The numbering is
/// per thread.
pub(crate) fn numbered<R>(way: Way, f: impl FnOnce() -> R) -> R {
    let _numbering = Numbering::start(way);

    f()
}

/// Node `n`'s number.
///
/// # Panics
///
/// Outside a [`Way::Saving`] numbering, or past `u32::MAX` nodes.
pub(crate) fn node(n: Node) -> u32 {
    NUMBERING.with(|nodes| match &mut *nodes.borrow_mut() {
        Nodes::Saving(map) => {
            let next = u32::try_from(map.len()).expect("list node numbers fit a u32");

            *map.entry(n).or_insert(next)
        }
        Nodes::Off | Nodes::Loading(_) => panic!("list node numbering is not saving"),
    })
}

/// The node standing for number `n`.
///
/// # Errors
///
/// Outside a [`Way::Loading`] numbering.
pub(crate) fn unnode(n: u32) -> Result<Node, Refused> {
    NUMBERING.with(|nodes| match &mut *nodes.borrow_mut() {
        Nodes::Loading(map) => Ok(*map.entry(n).or_insert_with(fresh_node)),
        Nodes::Off | Nodes::Saving(_) => Err(Refused::Numbering),
    })
}

#[cfg(test)]
mod numbering {
    use super::*;

    #[test]
    fn a_nodes_number_is_its_order_met() {
        let (a, b) = (fresh_node(), fresh_node());
        let numbers = numbered(Way::Saving, || [node(b), node(a), node(b)]);
        let back = numbered(Way::Loading, || {
            [unnode(0), unnode(1), unnode(0)].map(Result::unwrap)
        });

        assert_eq!(numbers, [0, 1, 0]);
        assert_eq!(back[0], back[2]);
        assert_ne!(back[0], back[1]);
    }

    #[test]
    #[should_panic(expected = "numbering")]
    fn numbering_a_node_outside_a_snapshot_is_refused() {
        node(fresh_node());
    }

    #[test]
    #[should_panic(expected = "numbering")]
    fn a_numbering_inside_another_is_refused() {
        numbered(Way::Saving, || numbered(Way::Loading, || ()));
    }

    #[test]
    fn a_panic_inside_a_numbering_ends_it() {
        let quiet = || std::panic::catch_unwind(|| numbered(Way::Saving, || panic!("inside")));

        assert!(quiet().is_err());
        assert!(std::panic::catch_unwind(|| node(fresh_node())).is_err());
    }
}
