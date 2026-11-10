//! What a drawing reads of the world: each piece's descriptor, place,
//! facing and what it is doing. Read-only.
//!
//! The world does not keep the animation set the engine draws a walker or
//! a stander in: [`Doing`] says which, and the drawing picks the set
//! (**Inferred**: `Walk` while moving, `Stand` else). It keeps the `Walk`
//! frame ([`Shown::walk_frame`]), a mobile's action and a building's set
//! ([`Shown::look`]).

use super::places::known_of;
use super::walking::{Tracks, held_of};
use super::{Place, World, walking::FIRST_FACING};
use crate::base::fixed::Fixed;
use crate::data::fourcc::FourCc;
use crate::sim::movement::action::Look;
use crate::sim::world::piece_state;

/// One piece, as drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shown {
    /// The agent driving it.
    pub agent: u32,
    /// Its descriptor, a `DUNT` id.
    pub code: FourCc,
    /// The save's object handle; 0 for a piece made since.
    pub handle: u32,
    pub owner: u32,
    /// `LocationOf`: 32nds of a cell. A held piece's is its holder's.
    pub at: (i32, i32),
    /// A flier's own height, 16.16 cells, unit height: the engine draws
    /// its body there, its shadow on the ground
    /// under it. `None` on the ground.
    pub aloft: Option<Fixed>,
    /// The direction word its art slot is picked by: its walker's facing,
    /// else the new game's word, else [`FIRST_FACING`].
    pub facing: i32,
    pub doing: Doing,
    pub housed: Housed,
    /// Game ms a `Walk` frame shows while it walks, a frame a step
    /// ([`Walker::stride_ms`](crate::sim::movement::Walker::stride_ms));
    /// `None` for a piece that does not walk, or whose steps leave the
    /// frame.
    pub stride: Option<u32>,
    /// The `Walk` frame its parts show
    /// ([`Walker::walk_frame`](crate::sim::movement::Walker::walk_frame)):
    /// a step moves it on, a turn or a wait holds it. `None` for a piece
    /// that does not walk, or whose art has no `Walk`.
    pub walk_frame: Option<u32>,
    /// The set it shows, where the world keeps it: a mobile's last
    /// action until it steps, its `InitAction`'s frame, then the after-set
    /// or the last frame, `Dead` once it lies dead; a building's, as
    /// building damage set picks it ([`crate::sim::damage`]):
    /// construction, damage, lit or not, `Crumble` as it falls, `Dead` in
    /// ruins.
    pub look: Option<Look>,
    /// Unit state bit `0x10`: a ruin, or a mobile lying dead.
    pub ruined: bool,
    /// How often its place was set outright, not
    /// stepped to: a teleport, leaving or entering a building, a camp
    /// move. The engine draws a piece where it is; a change says "do not
    /// slide it here". Changes no game state.
    pub jumps: u32,
}

/// What a piece is doing, as far as the world follows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Doing {
    Standing,
    /// `IsMoving`: an unfinished order on track 0.
    Walking,
    /// `IsMoving`, its walk's gait `Stand`: waiting on a walker in its
    /// way, or a spent path's set `0xb1` playing on track 0,
    /// which [`Shown::look`] then names.
    Waiting,
    /// An Action holds it: `World::holds`.
    Acting,
}

/// Whether a piece stands alone, inside another or hangs on one: unit
///  names a holder; state bit [`crate::sim::world::piece_state::HELD`] hides it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Housed {
    Out,
    /// Inside its holder: not drawn.
    Held,
    /// Hung on agent `on`, not held, as a flag on its target: drawn at
    /// `on`'s place plus `offset`, 16.16 cells x, y and height.
    Hung {
        on: u32,
        offset: [Fixed; 3],
    },
}

impl World {
    /// Every piece whose place the world knows, by agent. A piece it
    /// cannot follow is left out.
    #[must_use]
    pub fn shown(&self) -> Vec<Shown> {
        let mut out: Vec<Shown> = self
            .pieces
            .keys()
            .filter_map(|&id| self.shown_one(id))
            .collect();

        out.sort_by_key(|s| s.agent);
        out
    }

    /// How many of [`World::shown`] show an action's frame
    /// ([`Look::Frame`]), without making each whole: a per-frame tally.
    #[must_use]
    pub fn framed(&self) -> usize {
        self.pieces
            .keys()
            .filter(|&&id| self.framed_one(id))
            .count()
    }

    /// Whether [`World::shown_one`] of piece `id` shows [`Look::Frame`]:
    /// its look first, as most show none.
    fn framed_one(&self, id: u32) -> bool {
        let tracks = self.units.tracks(id);
        let framed = matches!(self.shown_look(id, tracks), Some(Look::Frame { .. }));
        let known = || {
            self.pieces
                .get(id)
                .and_then(|p| known_of(p, self.units.motion(id)))
                .is_some()
        };

        framed && known() && self.position(id).is_some()
    }

    /// Piece `id` as drawn, where the world knows its place.
    #[must_use]
    pub fn shown_one(&self, id: u32) -> Option<Shown> {
        let piece = self.pieces.get(id)?;
        // One lookup of the unit, not one for each of its facts.
        let unit = self.units.get(id);
        let motion = unit.and_then(|u| u.motion);
        let tracks = unit.and_then(|u| u.tracks.as_ref());
        let place: &Place = known_of(piece, motion)?;
        // As `World::is_moving`, of a known place; waiting is a walk whose
        // gait is `Stand` ([`Tracks::waits`]).
        let acting = held_of(motion, tracks) || self.falling(id).is_some();
        let moving = motion.is_some() && tracks.is_some_and(Tracks::moving);
        let doing = match (acting, moving, tracks.is_some_and(Tracks::waits)) {
            (true, ..) => Doing::Acting,
            (false, true, true) => Doing::Waiting,
            (false, true, false) => Doing::Walking,
            (false, false, _) => Doing::Standing,
        };

        Some(Shown {
            agent: id,
            code: piece.code,
            handle: piece.handle,
            owner: piece.owner,
            at: self.position(id)?,
            aloft: self.aloft_of(id, tracks),
            facing: facing_of(tracks, unit.and_then(|u| u.direction)),
            doing,
            housed: self.housed(id, place),
            stride: tracks.and_then(Tracks::stride),
            walk_frame: tracks.and_then(|t| t.walk_frame(self.now)),
            look: self.shown_look(id, tracks),
            ruined: place.state & piece_state::RUINED != 0,
            jumps: unit.map_or(0, |u| u.jumps),
        })
    }

    /// Whether agent `id` is fallen (`GenUnit` 3 or more): ruined,
    /// or lying dead; a building also once `DeleteAgent` left it
    /// agentless. Pick kind ok refuses a
    /// fallen building.
    #[must_use]
    pub fn fallen(&self, id: u32) -> bool {
        self.pieces.get(id).is_some_and(|p| match p.place {
            Some(place) if place.is_building() => !super::price::stands(p),
            Some(place) => place.ruined(),
            None => false,
        })
    }

    /// Piece `id`'s place set outright: [`Shown::jumps`].
    pub(super) fn jumped(&mut self, id: u32) {
        let u = self.units.entry(id);

        u.jumps = u.jumps.wrapping_add(1);
    }

    /// How piece `id` at `place` is housed: held, hung, or out.
    fn housed(&self, id: u32, place: &Place) -> Housed {
        let on = (place.holder != 0)
            .then(|| self.holder(place.holder))
            .flatten();
        // Asked only of a hung piece: its holder is found by a scan.
        let offset = || {
            self.hung_offset(id)
                .unwrap_or([place.fixed.0, place.fixed.1, Fixed::ZERO])
        };

        // A hung piece is held with its holder (as every overlay record).
        match (place.state & piece_state::HELD != 0, on) {
            (true, _) => Housed::Held,
            (false, Some(on)) if self.is_held(on) => Housed::Held,
            (false, Some(on)) => Housed::Hung {
                on,
                offset: offset(),
            },
            (false, None) => Housed::Out,
        }
    }

    /// The set piece `id` shows: its track 2's, else a building's
    /// ([`World::look`]), else a spell unit's ([`World::spell_look`]).
    fn shown_look(&self, id: u32, tracks: Option<&Tracks>) -> Option<Look> {
        tracks
            .and_then(Tracks::look)
            .or_else(|| {
                self.look(id).map(|p| Look::Plays {
                    set: u32::from(p.set),
                    since: p.since,
                })
            })
            .or_else(|| self.spell_look(id))
    }

    /// The direction word piece `id` is drawn by.
    pub(super) fn facing_word(&self, id: u32) -> i32 {
        facing_of(self.units.tracks(id), self.units.direction(id))
    }
}

/// [`World::facing_word`] of a piece whose `tracks` and new game
/// `direction` are read already.
fn facing_of(tracks: Option<&Tracks>, direction: Option<u16>) -> i32 {
    match (tracks, direction) {
        (Some(t), _) => t.facing(),
        (None, Some(d)) => i32::from(d),
        (None, None) => FIRST_FACING,
    }
}

#[cfg(test)]
mod tests {
    use super::super::places::Motion;
    use super::super::ruin::tests::standing;
    use super::*;
    use crate::sim::world::kind::Kind;

    /// A flag hung on a unit draws there, unless the unit is held: then it
    /// is held too, as every overlay record in the corpus.
    #[test]
    fn a_hung_piece_is_held_with_its_holder() {
        let mut world = World::default();
        let mut flag = standing(*b"ARA2", 300, Kind::REWARD, (0, 0));

        if let Some(p) = flag.place.as_mut() {
            p.holder = 200;
            p.fixed = (Fixed::from_bits(2048), Fixed::from_bits(-65535));
        }

        world
            .pieces
            .insert(1, standing(*b"ABJ1", 100, Kind::BUILDING, (10, 10)));
        world
            .pieces
            .insert(2, standing(*b"BVr1", 200, Kind::VEHICLE, (20, 20)));
        world.pieces.insert(3, flag);

        for id in 1..=3 {
            world.units.set_motion(id, Motion::Still);
        }

        let housed = |world: &World| {
            world
                .shown()
                .iter()
                .find(|s| s.agent == 3)
                .map(|s| s.housed)
        };

        assert_eq!(
            housed(&world),
            Some(Housed::Hung {
                on: 2,
                offset: [
                    Fixed::from_bits(2048),
                    Fixed::from_bits(-65535),
                    Fixed::ZERO
                ]
            })
        );

        if let Some(p) = world.pieces.place_mut(2) {
            p.holder = 100;
            p.state |= piece_state::HELD;
        }

        assert_eq!(housed(&world), Some(Housed::Held));
    }
}

/// To and from the save model.
mod persist {
    use super::*;
    use crate::save::{self as m, Refused, Saved};
    use std::result::Result;

    impl Saved for Doing {
        type Model = m::Doing;

        fn save(&self) -> m::Doing {
            match self {
                Doing::Standing => m::Doing::Standing,
                Doing::Walking => m::Doing::Walking,
                Doing::Waiting => m::Doing::Waiting,
                Doing::Acting => m::Doing::Acting,
            }
        }

        fn load(model: m::Doing) -> Result<Doing, Refused> {
            Ok(match model {
                m::Doing::Standing => Doing::Standing,
                m::Doing::Walking => Doing::Walking,
                m::Doing::Waiting => Doing::Waiting,
                m::Doing::Acting => Doing::Acting,
            })
        }
    }

    impl Saved for Housed {
        type Model = m::Housed;

        fn save(&self) -> m::Housed {
            match *self {
                Housed::Out => m::Housed::Out,
                Housed::Held => m::Housed::Held,
                Housed::Hung { on, offset } => m::Housed::Hung {
                    on,
                    offset: offset.save(),
                },
            }
        }

        fn load(model: m::Housed) -> Result<Housed, Refused> {
            Ok(match model {
                m::Housed::Out => Housed::Out,
                m::Housed::Held => Housed::Held,
                m::Housed::Hung { on, offset } => Housed::Hung {
                    on,
                    offset: <[Fixed; 3]>::load(offset)?,
                },
            })
        }
    }

    impl Saved for Shown {
        type Model = m::Shown;

        fn save(&self) -> m::Shown {
            let Shown {
                agent,
                code,
                handle,
                owner,
                at,
                aloft,
                facing,
                doing,
                housed,
                stride,
                walk_frame,
                look,
                ruined,
                jumps,
            } = *self;

            m::Shown {
                agent,
                code: code.save(),
                handle,
                owner,
                at,
                aloft: aloft.save(),
                facing,
                doing: doing.save(),
                housed: housed.save(),
                stride,
                walk_frame,
                look: look.save(),
                ruined,
                jumps,
            }
        }

        fn load(model: m::Shown) -> Result<Shown, Refused> {
            Ok(Shown {
                agent: model.agent,
                code: FourCc::load(model.code)?,
                handle: model.handle,
                owner: model.owner,
                at: model.at,
                aloft: Option::load(model.aloft)?,
                facing: model.facing,
                doing: Doing::load(model.doing)?,
                housed: Housed::load(model.housed)?,
                stride: model.stride,
                walk_frame: model.walk_frame,
                look: Option::load(model.look)?,
                ruined: model.ruined,
                jumps: model.jumps,
            })
        }
    }
}
