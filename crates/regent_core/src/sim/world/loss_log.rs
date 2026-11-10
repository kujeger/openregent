//! What the world loses track of, as it happens: side data for a log,
//! never read back by play.
//!
//! ```text
//!  order not followed ─> Motion::Walking ─┬─> What::Place       first per order and why
//!                       (already unknown) └─> What::Declined    the same
//!
//!  World::take_lost drains them; World::losses counts every one.
//! ```
//!
//! One unknown place stops every later walk (`World::unplaced`), so the
//! first [`What::Place`] is the one to read.

use crate::base::digest::Set;
use std::fmt;
use std::mem::{Discriminant, discriminant};

use super::{World, places::Motion};
use crate::gpl::ops::Value;
use crate::gpl::vm::Vm;
use crate::gpl::vm::host::Call;

/// Why the world stopped following a piece.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Why {
    /// Frames are not stepped.
    NotStepped,
    /// No ground to walk on.
    NoGround,
    /// No movement class.
    NoGait,
    /// Its own place is unknown already.
    Unknown,
    /// Another piece's place is unknown, and it might stand in the way.
    Unplaced(u32),
    /// An order to no unit.
    Gone,
    /// A held walker that did not leave its holder.
    Held,
    /// `Hide` into a building with no entrance.
    NoEntrance,
    /// Nudged by a walker going to no point.
    Nudged,
    /// Dying with no free place to lie.
    NoRoom,
    /// An action with no aim, art or tracks.
    Unacted,
    /// An action with no record, while following a unit.
    Endless,
    /// Made with no place.
    Made,
}

impl fmt::Display for Why {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Why::NotStepped => write!(f, "frames not stepped"),
            Why::NoGround => write!(f, "no ground"),
            Why::NoGait => write!(f, "no movement class"),
            Why::Unknown => write!(f, "its place is unknown"),
            Why::Unplaced(id) => write!(f, "agent {id}'s place is unknown"),
            Why::Gone => write!(f, "no unit"),
            Why::Held => write!(f, "did not leave its holder"),
            Why::NoEntrance => write!(f, "the building has no entrance"),
            Why::Nudged => write!(f, "nudged by a walker going to no point"),
            Why::NoRoom => write!(f, "no free place to lie"),
            Why::Unacted => write!(f, "action with no aim, art or tracks"),
            Why::Endless => write!(f, "action with no record while following"),
            Why::Made => write!(f, "made with no place"),
        }
    }
}

/// The builtin whose answer gave up a piece, and its arguments.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ordered {
    pub builtin: &'static str,
    /// Each argument as text, filled once the builtin returns.
    pub args: Vec<String>,
}

/// What was lost.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum What {
    /// A piece's place became unknown: the world stops following it.
    /// `None` for the world's own step.
    Place { order: Option<Ordered>, why: Why },
    /// An order to a piece the world already does not follow.
    Declined { order: Option<Ordered>, why: Why },
}

/// One thing lost, at the world's clock.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lost {
    /// Game ms.
    pub now: u32,
    /// The piece.
    pub agent: Option<u32>,
    /// The piece's descriptor name.
    pub name: Option<String>,
    pub what: What,
}

impl fmt::Display for Lost {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let agent = match (self.agent, &self.name) {
            (Some(id), Some(name)) => format!("agent {id} ({name})"),
            (Some(id), None) => format!("agent {id}"),
            _ => "no agent".to_owned(),
        };
        let order = |o: &Option<Ordered>| {
            o.as_ref().map_or_else(
                || "the world's own step".to_owned(),
                |o| format!("{}({})", o.builtin, o.args.join(", ")),
            )
        };

        match &self.what {
            What::Place { order: o, why } => write!(
                f,
                "lost {agent} at {} ms: {} not followed ({why}); its place is unknown, \
                 so no later walk is followed",
                self.now,
                order(o)
            ),
            What::Declined { order: o, why } => write!(
                f,
                "declined {agent} at {} ms: {} not followed ({why})",
                self.now,
                order(o)
            ),
        }
    }
}

/// Totals of every loss, the ones not kept as events too.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    pub places: usize,
    pub declined: usize,
}

/// A loss's rate-limit key: its kind, builtin and why.
type Key = (
    Discriminant<What>,
    Option<&'static str>,
    Option<Discriminant<Why>>,
);

/// The events not yet taken, what has been seen, and the totals.
#[derive(Clone, Debug, Default)]
pub struct Losses {
    events: Vec<Lost>,
    seen: Set<Key>,
    counts: Counts,
    /// The checked builtin running: an order's name.
    asking: Option<&'static str>,
}

impl Losses {
    /// Keep `lost` if its key is new.
    fn keep(&mut self, lost: Lost, builtin: Option<&'static str>, why: Option<Why>) {
        let key = (
            discriminant(&lost.what),
            builtin,
            why.as_ref().map(discriminant),
        );

        if self.seen.insert(key) {
            self.events.push(lost);
        }
    }
}

/// A value as a log shows it: a list by its length.
fn text(v: Option<&Value>) -> String {
    match v {
        Some(Value::Agent(id)) => format!("agent {id}"),
        Some(Value::Integer(n)) => n.to_string(),
        Some(Value::String(s)) => format!("{s:?}"),
        Some(Value::Coord(x, y)) => format!("({x}, {y})"),
        Some(Value::List(l)) => format!("a list of {}", l.len()),
        Some(other) => format!("{other:?}"),
        None => "?".to_owned(),
    }
}

impl World {
    /// Stop following piece `id`, for `why`: its place is unknown from
    /// here. Kept as an event the first time per order and why.
    pub(super) fn lose(&mut self, id: u32, why: Why) {
        let followed = self.units.set_motion(id, Motion::Walking) != Some(Motion::Walking);
        let order = self.losses.asking.map(|builtin| Ordered {
            builtin,
            args: Vec::new(),
        });
        let what = if followed {
            self.losses.counts.places += 1;
            What::Place { order, why }
        } else {
            self.losses.counts.declined += 1;
            What::Declined { order, why }
        };
        let lost = Lost {
            now: self.now,
            agent: Some(id),
            name: self.any_piece(id).map(|p| p.name.clone()),
            what,
        };

        self.losses.keep(lost, self.losses.asking, Some(why));
    }

    /// The lowest agent `World::unplaced` finds: a loss names the same
    /// piece whatever the map's order.
    pub(super) fn first_unplaced(&self) -> Option<u32> {
        // `World::known` of a bound piece is of the piece itself: no
        // second lookup. The field test first.
        let bound = self.pieces.iter().map(|(id, p)| (id, p, Some(p)));
        let unbound = (self.pieces.unbound().iter()).map(|(id, p)| (id, p, self.any_piece(*id)));

        bound
            .chain(unbound)
            .filter(|(id, _, _)| **id != super::NO_AGENT_ID)
            .filter(|(id, p, any)| {
                p.place.is_none_or(|pl| pl.holder == 0)
                    && any
                        .and_then(|a| super::places::known_of(a, self.units.motion(**id)))
                        .is_none()
            })
            .map(|(id, _, _)| *id)
            .min()
    }

    /// A checked builtin starts: its name is the order a loss names.
    /// The events count from the returned length.
    pub(super) const fn asking(&mut self, call: &Call<'_>) -> usize {
        self.losses.asking = Some(call.builtin().name);
        self.losses.events.len()
    }

    /// A checked builtin returned: the events since `from` take its
    /// arguments.
    pub(super) fn asked(&mut self, vm: &Vm<'_>, call: &Call<'_>, from: usize) {
        // Most builtins lose nothing: no arguments to write out.
        if self.losses.events.len() > from {
            let args: Vec<String> = call
                .args
                .iter()
                .skip(1)
                .map(|h| text(vm.value(*h)))
                .collect();

            for lost in self.losses.events.iter_mut().skip(from) {
                if let What::Place { order: Some(o), .. } | What::Declined { order: Some(o), .. } =
                    &mut lost.what
                {
                    o.args.clone_from(&args);
                }
            }
        }

        self.losses.asking = None;
    }

    /// The losses since the last call, oldest first.
    pub fn take_lost(&mut self) -> Vec<Lost> {
        std::mem::take(&mut self.losses.events)
    }

    /// Every loss so far, kept as an event or not.
    #[must_use]
    pub const fn losses(&self) -> Counts {
        self.losses.counts
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base::fixed::Fixed;
    use crate::gpl::builtins;
    use crate::gpl::load::Program;
    use crate::gpl::vm::Config;
    use crate::sim::world::kind;
    use crate::sim::world::{Agent, Piece, Place};

    const WALKER: u32 = 7;

    /// A world with one placed piece, followed.
    fn world() -> World {
        let mut world = World::default();
        let place = Place {
            kind: kind::Kind::VEHICLE,
            fixed: (Fixed::ZERO, Fixed::ZERO),
            standing: (0, 0),
            holder: 0,
            state: 0,
        };

        world.pieces.insert(
            WALKER,
            Piece {
                name: "Peasant".into(),
                place: Some(place),
                ..Piece::default()
            },
        );
        world.agents.insert(WALKER, Agent::default());
        world.units.set_motion(WALKER, Motion::Still);
        world
    }

    /// Run builtin `name` on `args`.
    fn run(world: &mut World, vm: &mut Vm<'_>, name: &str, args: &[Value]) {
        let builtin = builtins::id(name).expect("a builtin");
        let handles: Vec<crate::gpl::vm::Handle> = std::iter::once(Value::Null)
            .chain(args.iter().cloned())
            .map(|v| {
                let o = vm.alloc(v);
                vm.top_handle(Some(o))
            })
            .collect();
        let call = Call {
            builtin,
            args: &handles,
        };

        let _ = world.run(
            vm,
            &call,
            &|_, _, _| None,
            crate::sim::world::step::Draws::Kept,
        );
    }

    #[test]
    fn an_unfollowed_order_is_one_lost_event_naming_agent_and_order() {
        let program = Program::load::<&[u8]>(&[]).expect("empty program");
        let mut vm = Vm::new(&program, Config::default());
        let mut world = world();
        let args = [Value::Agent(WALKER), Value::Coord(64, 64)];

        // No stepped frames: the world follows no walk.
        run(&mut world, &mut vm, "Move", &args);
        run(&mut world, &mut vm, "Move", &args);

        let lost = world.take_lost();
        let places: Vec<&Lost> = lost
            .iter()
            .filter(|l| matches!(l.what, What::Place { .. }))
            .collect();

        assert_eq!(places.len(), 1, "{lost:?}");
        assert_eq!(places[0].agent, Some(WALKER));
        assert_eq!(
            places[0].what,
            What::Place {
                order: Some(Ordered {
                    builtin: "Move",
                    args: vec!["agent 7".into(), "(64, 64)".into()],
                }),
                why: Why::NotStepped,
            }
        );
        assert!(
            places[0].to_string().starts_with(
                "lost agent 7 (Peasant) at 0 ms: Move(agent 7, (64, 64)) not followed"
            ),
            "{}",
            places[0]
        );
        assert_eq!(
            world.losses(),
            Counts {
                places: 1,
                declined: 1
            }
        );
        assert_eq!(world.motion(WALKER), Some(Motion::Walking));
    }

    /// Of several pieces the world cannot place, the lowest agent comes
    /// first, not the first the map walks to.
    #[test]
    fn the_first_unplaced_is_the_lowest_agent() {
        let mut world = World::default();

        for id in (1..=64).rev() {
            world.pieces.insert(id, Piece::default());
        }

        assert_eq!(world.first_unplaced(), Some(1));
    }
}
