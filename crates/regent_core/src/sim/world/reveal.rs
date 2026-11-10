//! Explored ground: one flag byte per map vertex, bit `p` set where player
//! `p` has seen the ground (`Vertex::flags`). **Binary** unless marked.
//!
//! ```text
//!  every move of a unit    ─> GenUnit       ─> the reveal hook
//!     the hook: unheld building or character
//!       disc about its pad rect's cell, SightRange / 32 cells, owner's bit
//!  RevealArea(unit, point, r) ─> disc about the point's cell, r / 32
//!  the reveal disc   vertices within r of the cell's centre, 16.16:
//!
//!        r = 2, cell c:   . x x .     x: (vx - cx - ½)² + (vy - cy - ½)² ≤ r²
//!                         x c x x        over vx in cx - r + 1 ..= cx + r,
//!                         x x x x        c the cell's top-left vertex
//! X x .
//! ```
//!
//! A search sees a piece where its cell's corners are explored,
//! or a building where one of five sample cells'
//! of its `PRIM` rect are.
//!
//! ```text
//!  after each frame's scheduler: GE_Level,      the sight pass
//!    a unit marked (moved, or any vertex gained a bit): its words anew
//!    a building's words changed: the second reveal, a disc about its
//!      rect for each player who sees it
//!    so too a RevealTerrainIfExposed unit: a scorch pond or geyser
//! ```

use crate::content::terrain::grid::Terrain;
use crate::sim::world::kind;
use crate::sim::world::piece_state;
use std::collections::BTreeSet;

use super::{Answer, Found, World, agent_ref, int_ref};
use crate::base::digest::Map;
use crate::base::fixed::{Fixed, ONE, from_whole, mul, whole};
use crate::base::switch::Switch;
use crate::data::attrib::Key;
use crate::data::fourcc::FourCc;
use crate::gpl::ops::{Accessor, OpError, Value};
use crate::gpl::vm::{Fault, Handle, Vm};
use crate::sim::chunks::Chunks;
use crate::sim::space;
use crate::sim::units::PER_CELL;

/// Environment variable that turns on `World::check_sight`: each sight
/// checks that the pieces it skips are as they last revealed, e.g.
/// `MAJESTY_CHECK_SIGHT=1 cargo nextest run`.
pub static CHECK_SIGHT: Switch = Switch::new("MAJESTY_CHECK_SIGHT");

/// `SightRange`, `APU:3`: the radius a unit reveals, coordinates
/// (`0x3555041` in unit reveal disc).
pub const SIGHT_RANGE: Key = crate::data::attrib::SIGHT_RANGE;

/// Signed division toward zero by a power of two, as a sign-corrected shift.
const fn toward_zero(v: i32, by: i32) -> i32 {
    v / by
}

/// Every vertex's explored bits, in shared chunks: a copy shares them,
/// and a reveal copies only the chunks it changes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Explored {
    /// Vertices a row: map, the cells plus one.
    across: i32,
    flags: Chunks<u8>,
    /// A vertex gained a bit since the last sight pass: map bit 0.
    fresh: bool,
}

impl Explored {
    /// A save's explored ground.
    #[must_use]
    pub fn of(terrain: &Terrain<'_>) -> Explored {
        Explored::of_flags(terrain.side(), terrain.vertices().map(|v| v.flags))
    }

    /// Ground `side` cells a side explored as `flags` say, row by row;
    /// a vertex past their end unexplored.
    #[must_use]
    pub fn of_flags(side: u32, flags: impl IntoIterator<Item = u8>) -> Explored {
        let across = side + 1;
        let flags: Vec<u8> = flags
            .into_iter()
            .chain(std::iter::repeat(0))
            .take((across * across) as usize)
            .collect();

        Explored {
            across: across.cast_signed(),
            flags: Chunks::of(&flags),
            fresh: false,
        }
    }

    /// Ground no player has explored, `side` cells a side.
    #[must_use]
    pub fn blank(side: u32) -> Explored {
        let across = side + 1;

        Explored {
            across: across.cast_signed(),
            flags: Chunks::filled(0, (across * across) as usize),
            fresh: false,
        }
    }

    /// The flag bytes, row by row.
    pub fn flags(&self) -> impl Iterator<Item = u8> + '_ {
        self.flags.iter().copied()
    }

    /// Flag byte `i`, row by row; `None` past the end.
    #[must_use]
    pub fn flag(&self, i: usize) -> Option<u8> {
        (i < self.flags.len()).then(|| *self.flags.get(i))
    }

    /// Each run of flags that differs from `was`'s, as the same map's:
    /// its first index, then `was`'s flags and these. A run both still
    /// share is passed over unread, so a copy taken before a reveal
    /// finds only the runs it touched.
    pub fn differing<'a>(
        &'a self,
        was: &'a Explored,
    ) -> impl Iterator<Item = (usize, &'a [u8], &'a [u8])> {
        self.flags.differing(&was.flags)
    }

    fn index(&self, x: i32, y: i32) -> Option<usize> {
        let inside = (0..self.across).contains(&x) && (0..self.across).contains(&y);

        inside.then(|| (y * self.across + x).cast_unsigned() as usize)
    }

    /// Reveal disc: `mask` onto every vertex within `radius` cells
    /// of the centre of cell `centre`. Nothing for a radius of 0 or less.
    pub fn disc(&mut self, centre: (i32, i32), mask: u8, radius: i32) {
        if radius > 0 {
            let r2 = mul(from_whole(radius), from_whole(radius));
            let (cx, cy) = centre;
            let (fx, fy) = (cx * ONE + ONE / 2, cy * ONE + ONE / 2);
            let xs = (cx - radius + 1).max(0)..(cx + 1 + radius).min(self.across);
            let ys = (cy - radius + 1).max(0)..(cy + 1 + radius).min(self.across);

            for y in ys {
                let dy = y * ONE - fy;

                for x in xs.clone() {
                    let dx = x * ONE - fx;
                    let within = mul(dx, dx).wrapping_add(mul(dy, dy)) <= r2;

                    // Written only if it gains a bit: a shared chunk is copied.
                    if let (true, Some(i)) = (within, self.index(x, y))
                        && self.flags.get(i) & mask != mask
                    {
                        self.fresh = true;
                        *self.flags.get_mut(i) |= mask;
                    }
                }
            }
        }
    }

    /// `mask` onto every vertex (reveal everything, **Inferred** by name).
    pub fn everything(&mut self, mask: u8) {
        for i in 0..self.flags.len() {
            if self.flags.get(i) & mask != mask {
                self.fresh = true;
                *self.flags.get_mut(i) |= mask;
            }
        }
    }

    /// `bits` onto every vertex holding one of `from`'s
    /// (vertex flag rect over the whole map): a match marks the
    /// map, changed or not.
    pub fn share(&mut self, from: u8, bits: u8) {
        for i in 0..self.flags.len() {
            let f = *self.flags.get(i);

            if f & from != 0 {
                self.fresh = true;

                if f & bits != bits {
                    *self.flags.get_mut(i) |= bits;
                }
            }
        }
    }

    /// The bits on the four corners of cell `cell`, as
    /// sight at cell reads them; none off the cells.
    fn corners(&self, cell: (i32, i32)) -> u8 {
        let cells = self.across - 1;
        let (x, y) = cell;
        let on = (0..cells).contains(&x) && (0..cells).contains(&y);
        let at = |x, y| self.index(x, y).map_or(0, |i| *self.flags.get(i));

        if on {
            at(x, y) | at(x + 1, y) | at(x, y + 1) | at(x + 1, y + 1)
        } else {
            0
        }
    }

    /// Cells a side: the vertices less one.
    #[must_use]
    pub const fn cells(&self) -> i32 {
        self.across - 1
    }

    /// `cell_flag_any(cell, 1 << p)`: a corner of `cell` explored by
    /// player `p`.
    pub(super) fn cell_seen(&self, cell: (i32, i32), p: u32) -> bool {
        self.corners(cell) & 1u8.wrapping_shl(p) != 0
    }

    /// Whether player `p` sees a point unit at 16.16 `fixed`: a corner of
    /// its cell explored.
    #[must_use]
    pub fn sees_point(&self, fixed: (Fixed, Fixed), p: u32) -> bool {
        self.corners((whole(fixed.0), whole(fixed.1))) & 1u8.wrapping_shl(p) != 0
    }

    /// Whether player `p` sees a building over cells `rect`: a corner of
    /// one of sight in rect's five samples explored, the corners
    /// and the middle, x1 exclusive, y1 inclusive.
    #[must_use]
    pub fn sees_rect(&self, rect: [i32; 4], p: u32) -> bool {
        let [x0, y0, x1, y1] = rect;
        let middle = ((x1 - x0) / 2 + x0, (y1 - y0) / 2 + y0);
        let samples = [(x0, y0), (x1 - 1, y0), (x1 - 1, y1), (x0, y1), middle];

        samples
            .iter()
            .any(|c| self.corners(*c) & 1u8.wrapping_shl(p) != 0)
    }

    /// Sight at cell's words for cell `cell`: per player,
    /// [`PER_CORNER`] for each corner whose vertex has its bit; none off
    /// the cells.
    fn cell_words(&self, cell: (i32, i32)) -> Words {
        let cells = self.across - 1;
        let (x, y) = cell;
        let on = (0..cells).contains(&x) && (0..cells).contains(&y);
        let at = |x, y| self.index(x, y).map_or(0, |i| *self.flags.get(i));
        let corners = [at(x, y), at(x + 1, y), at(x, y + 1), at(x + 1, y + 1)];

        std::array::from_fn(|p| {
            let bit = 1u8.wrapping_shl(u32::try_from(p).unwrap_or(0));
            let count = corners.iter().filter(|c| *c & bit != 0).count();

            if on {
                PER_CORNER * i32::try_from(count).unwrap_or(0)
            } else {
                0
            }
        })
    }

    /// The seen level of a point unit in cell `cell` for the players
    /// `mask` names: [`level_of`] its cell's words, 25 a corner explored.
    #[must_use]
    pub fn seen_at(&self, cell: (i32, i32), mask: u8) -> i32 {
        level_of(&self.cell_words(cell), mask)
    }

    /// The second reveal about cells `rect`: for each player
    /// whose word in `words` is above 0, the word to [`SEEN`] and a disc
    /// for that player,
    ///
    /// ```text
    ///  h = (x1 − x0) / 2     centre (x0 + h, y0 + h)     radius h + 1
    /// ```
    ///
    /// the width standing for the height too.
    fn second_reveal(&mut self, rect: [i32; 4], words: &mut Words) {
        let [x0, y0, x1, _] = rect;
        let h = (x1 - x0) / 2;

        for (p, word) in (0u32..).zip(words.iter_mut()) {
            if *word > 0 {
                *word = SEEN;
                self.disc((x0 + h, y0 + h), 1u8.wrapping_shl(p), h + 1);
            }
        }
    }

    /// Sight in rect's words for cells `rect`: per player, the most
    /// [`Explored::cell_words`] gives over the five samples of
    /// [`Explored::sees_rect`].
    fn rect_words(&self, rect: [i32; 4]) -> Words {
        let [x0, y0, x1, y1] = rect;
        let middle = ((x1 - x0) / 2 + x0, (y1 - y0) / 2 + y0);
        let samples = [(x0, y0), (x1 - 1, y0), (x1 - 1, y1), (x0, y1), middle];

        samples.iter().fold([0; PLAYERS], |most, c| {
            let words = self.cell_words(*c);

            std::array::from_fn(|p| most[p].max(words[p]))
        })
    }
}

/// Players a unit keeps a sight word for.
pub const PLAYERS: usize = 8;

/// One sight word per player, 0 to 100.
pub type Words = [i32; PLAYERS];

/// What one explored corner adds to a sight word (`0x19`,
/// sight at cell).
pub const PER_CORNER: i32 = 25;

/// Holders a chain may have before [`World::seen_level`], a spot or a
/// position stops climbing: a guard against a save whose piece holds
/// itself, as in play a holder never does.
pub(super) const HOLDERS_MOST: usize = 16;

/// A message flag's kind, unit kind `MSGF`.
const MESSAGE_FLAG: kind::Kind = kind::Kind(FourCc(*b"MSGF").word());

/// The highest of `words` over the players `mask` names, 0 for none:
/// Unit seen level's fold.
#[must_use]
pub fn level_of(words: &Words, mask: u8) -> i32 {
    (0u32..)
        .zip(words)
        .filter(|(p, _)| mask & 1u8.wrapping_shl(*p) != 0)
        .map(|(_, w)| *w)
        .fold(0, i32::max)
}

/// A sight word in full: the owner's, and a player's the second reveal
/// ran for (`0x64`, unit sight update).
pub const SEEN: i32 = 100;

/// Where `GetNearestHiddenCoord`'s answer stays off the board's edge,
/// coordinates: a compare with `0x30` in gpl get nearest hidden coord.
pub const HIDDEN_MARGIN: i32 = 0x30;

/// `GetNearestHiddenCoord`'s draw: `gpl_rng_next(32)`; the draw / 8
/// picks the corner its search starts each ring at.
pub const HIDDEN_DRAW: i32 = 32;

/// Draws per corner of [`HIDDEN_DRAW`].
const DRAWS_PER_CORNER: i32 = 8;

impl Explored {
    /// nearest unexplored: the vertex rings about cell `from`,
    /// outward, each walked clockwise from the corner `draw / 8` names —
    /// top left, top right, bottom right, bottom left — each side from its
    /// first corner to before its last, clipped to the map; the first
    /// vertex player `p` has not explored, as the cell on `from`'s side of
    /// it. `None` once a ring lies wholly off the map.
    ///
    /// ```text
    ///  ring k: x0 = fx − k .. x1 = fx + 1 + k, y likewise, clipped
    ///  start 0: (x0,y0) → (x1,y0) → (x1,y1) → (x0,y1) → back
    ///  cell:   vx − 1 where vx > fx, vy − 1 where vy > fy
    /// ```
    #[must_use]
    pub fn nearest_hidden(&self, from: (i32, i32), p: u32, draw: i32) -> Option<(i32, i32)> {
        let cells = self.across - 1;
        let mask = 1u8.wrapping_shl(p);
        let corner = (draw / DRAWS_PER_CORNER).rem_euclid(4) as usize;
        let (fx, fy) = from;

        (0..)
            .map(|k| [fx - k, fy - k, fx + 1 + k, fy + 1 + k])
            .take_while(|[rx0, ry0, rx1, ry1]| {
                !(*rx0 < 0 && *ry0 < 0 && *rx1 > cells && *ry1 > cells)
            })
            .find_map(|[rx0, ry0, rx1, ry1]| {
                let (x0, y0) = (rx0.max(0), ry0.max(0));
                let (x1, y1) = (rx1.min(cells), ry1.min(cells));
                let top = (x0..x1).map(|x| (x, y0));
                let right = (y0..y1).map(|y| (x1, y));
                let bottom = (x0 + 1..=x1).rev().map(|x| (x, y1));
                let left = (y0 + 1..=y1).rev().map(|y| (x0, y));
                let sides: [Vec<(i32, i32)>; 4] = [
                    top.collect(),
                    right.collect(),
                    bottom.collect(),
                    left.collect(),
                ];

                sides
                    .iter()
                    .cycle()
                    .skip(corner)
                    .take(4)
                    .flatten()
                    .find(|(x, y)| {
                        self.index(*x, *y)
                            .is_some_and(|i| self.flags.get(i) & mask == 0)
                    })
                    .map(|&(x, y)| (x - i32::from(x > fx), y - i32::from(y > fy)))
            })
    }
}

/// A scatter object: its code and half cell.
pub(super) type Feature = (FourCc, (i32, i32));

/// The sight a scatter object `code` is sampled with, if a sight pass
/// samples it: `RevealTerrainIfExposed`, over its rect.
fn exposed(descriptors: &super::descriptors::Descriptors, code: FourCc) -> Option<super::Sight> {
    descriptors
        .shape(code)
        .filter(|s| s.exposure == super::Exposure::Rect)
        .map(|s| s.sight)
}

/// What a piece last revealed from: its half cell and held bit, and its
/// descriptor.
pub(super) type Sighted = (((i32, i32), u8), [u8; 4]);

/// What a piece was at the last sight pass's words: its 16.16 position
/// and held bit, and its descriptor.
pub(super) type Passed = (((Fixed, Fixed), u8), [u8; 4]);

/// Whether pieces reveal as the world follows them. An interface run, as
/// the build preview's `CanIBuildThisBuilding`, must change nothing, so
/// its searches see the ground as the last reveal left it (**Inferred**:
/// the engine reveals as units move, so its ground is never stale).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Sighting {
    #[default]
    Live,
    Frozen,
}

/// A cell from a coordinate: cell pos of pos32, then floored.
const fn cell_of(v: i32) -> i32 {
    v >> 5
}

/// The disc a unit whose pad is `rect` (`sim::placement::pixel_rect`,
/// 32nds about it), at 16.16 `fixed` with sight `sight`, reveals: cell and
/// radius ([`Explored::disc`]).
#[must_use]
pub fn pad_disc(rect: [i32; 4], fixed: (Fixed, Fixed), sight: i32) -> ((i32, i32), i32) {
    let (px, py) = (space::pos32(fixed.0), space::pos32(fixed.1));

    // The pad bounds' corners, each a cell in 16.16 (`fixed_div` by
    // 32.0: times 2048), their mean floored.
    let corner = |v: i32| ONE / PER_CELL * v;
    let (ax, ay) = (corner(px + rect[0]), corner(py + rect[1]));
    let (bx, by) = (corner(px + rect[2]), corner(py + rect[3]));
    let centre = (whole(ax.midpoint(bx)), whole(ay.midpoint(by)));
    let pad = toward_zero(whole((bx - ax) + (by - ay)), 4);

    (centre, pad + toward_zero(sight, PER_CELL))
}

/// Explored ground and what reveals it: the world's fog.
///
/// ```text
///  Fog ── explored       a flag byte per vertex, a bit per player
///     ├─ seen             players who had explored all of a new game's map
///     ├─ blind            players a piece revealed for unfollowed
///     ├─ sighting         whether pieces reveal (not in an interface run)
///     ├─ scatter_words    scatter's sight words, by code and half cell
///     └─ passes           sight passes run
/// ```
///
/// Each unit's own sight state is its record's ([`super::unit::Unit`]).
#[derive(Clone, Debug, Default)]
pub(super) struct Fog {
    explored: Explored,
    /// Bit `p`: player `p` had explored every vertex of the new game's map.
    seen: u8,
    /// Bit `p`: a piece of player `p` revealed where the world could not
    /// follow, so ground `p` has not explored here may be explored.
    blind: u8,
    sighting: Sighting,
    /// Each scatter object's sight words, by code and half cell, as the
    /// last sight pass left them: only those sampled over a rect
    /// ([`super::Exposure::Rect`]).
    scatter_words: Map<Feature, Words>,
    passes: u32,
}

impl Fog {
    /// A world's on `explored`, `seen` the players who had explored all of
    /// it.
    pub(super) fn new(explored: Explored, seen: u8) -> Fog {
        Fog {
            explored,
            seen,
            ..Fog::default()
        }
    }

    /// The explored ground.
    pub(super) const fn explored(&self) -> &Explored {
        &self.explored
    }

    /// The explored ground, to reveal.
    pub(super) const fn explored_mut(&mut self) -> &mut Explored {
        &mut self.explored
    }

    /// Bit `p`: player `p` had explored every vertex of the new game's map.
    pub(super) const fn seen(&self) -> u8 {
        self.seen
    }

    /// The players who explored all of the ground now: a new game's start.
    pub(super) fn mark_seen(&mut self) {
        self.seen = self.explored.flags().fold(u8::MAX, |all, f| all & f);
    }

    /// No piece revealed unfollowed: the start's reveals are followed.
    pub(super) const fn clear_blind(&mut self) {
        self.blind = 0;
    }

    /// Whether pieces reveal.
    pub(super) const fn sighting(&self) -> Sighting {
        self.sighting
    }
}

mod persist_fog {
    use super::*;
    use crate::save::{self as m, Refused, Saved};
    use std::result::Result;

    impl Saved for Fog {
        type Model = m::Fog;

        fn save(&self) -> m::Fog {
            m::Fog {
                explored: self.explored.save(),
                seen: self.seen,
                blind: self.blind,
                sighting: self.sighting.save(),
                scatter_words: self.scatter_words.save(),
                passes: self.passes,
            }
        }

        fn load(model: m::Fog) -> Result<Fog, Refused> {
            Ok(Fog {
                explored: Explored::load(model.explored)?,
                seen: model.seen,
                blind: model.blind,
                sighting: Sighting::load(model.sighting)?,
                scatter_words: Map::load(model.scatter_words)?,
                passes: model.passes,
            })
        }
    }
}

impl World {
    /// Set whether pieces reveal: [`Sighting::Frozen`] for an interface
    /// run.
    pub const fn set_sighting(&mut self, sighting: Sighting) {
        self.fog.sighting = sighting;
    }

    /// The disc unit reveal hook reveals for piece `id` where it
    /// stands: cell, radius, mask. `None` for a held piece, one not a
    /// building or a mobile, or one whose place or shape is unknown.
    fn unit_disc(&self, id: u32) -> Option<((i32, i32), i32, u8)> {
        let piece = self.pieces.get(id)?;
        let place = self.known(id)?;
        let kind = place.is_building() || place.is_vehicle();
        let held = place.state & piece_state::HELD != 0;
        let sight = piece.get(SIGHT_RANGE).unwrap_or(0);
        let disc = self.pad_disc(piece.code, place.fixed, sight)?;

        (kind && !held).then_some((disc.0, disc.1, self.players.reveal_mask(piece.owner)))
    }

    /// The disc a unit of `code` at `fixed`, 16.16, with sight `sight`
    /// reveals: cell and radius. `None` without its shape.
    pub(super) fn pad_disc(
        &self,
        code: FourCc,
        fixed: (Fixed, Fixed),
        sight: i32,
    ) -> Option<((i32, i32), i32)> {
        let rect = self.descriptors.shape(code)?.bounds;

        Some(pad_disc(rect, fixed, sight))
    }

    /// Each unit the world places reveals about it as `GenUnit`'s
    /// hook does: on unit set pos, and as a walker claims its next
    /// half cell, before the step; a step itself writes
    /// the position alone. Here for every piece
    /// whose half cell, holder or descriptor changed since it last
    /// revealed, where it stands.
    pub(super) fn sight(&mut self) {
        // An interface run reveals nothing; what changed waits for a live
        // one.
        match self.fog.sighting {
            Sighting::Live => self.sight_live(),
            Sighting::Frozen => {}
        }
    }

    /// [`World::sight`], live. Only a piece changed since the last, or
    /// one whose place was unknown then, can reveal: every other's half
    /// cell, holder and descriptor are as it last revealed. So only those
    /// are looked at, in agent order as a scan of all.
    fn sight_live(&mut self) {
        let mut ids = self.pieces.take_sight();

        ids.append(&mut self.units.take_sight());
        self.check_sight(&ids);

        let mut unknown = Vec::new();

        for id in ids.into_iter().filter(|id| self.pieces.contains(*id)) {
            let now = self.sighted_now(id);
            let moved = now.is_some()
                && self.units.get(id).and_then(|u| u.sighted.as_ref()) != now.as_ref();
            let owner = self.pieces.owner(id).unwrap_or(0);

            match (moved, now) {
                (true, Some(now)) => {
                    self.units.entry(id).sighted = Some(now);

                    match self.unit_disc(id) {
                        Some((centre, radius, mask)) => {
                            self.fog.explored.disc(centre, mask, radius);
                        }
                        None if self.reveals(id) => {
                            self.fog.blind |= self.players.reveal_mask(owner);
                        }
                        None => {}
                    }
                }
                (_, None) => {
                    self.fog.blind |= self.players.reveal_mask(owner);
                    unknown.push(id);
                }
                _ => {}
            }
        }

        // Unknown, each marks its owner's players blind again next time.
        self.pieces.sight_again(unknown);
    }

    /// Piece `id`'s half cell, held bit and descriptor, where its place
    /// is known.
    fn sighted_now(&self, id: u32) -> Option<Sighted> {
        self.known(id)
            .map(|p| (p.standing, p.state & piece_state::HELD))
            .zip(self.pieces.get(id).map(|p| p.code.0))
    }

    /// With [`CHECK_SIGHT`] set, panics where the scatter a sight pass
    /// samples, as kept, differs from it measured anew: a change to the
    /// scatter or its stands that did not move [`Board::relaid`].
    ///
    /// [`Board::relaid`]: super::board::Board::relaid
    fn check_exposed(&self, kept: &[super::board::Sampled]) {
        if CHECK_SIGHT.on() {
            let now = self.board.measured(|c| exposed(&self.descriptors, c));

            assert!(
                now == kept,
                "scatter sight kept {kept:?}, measured anew {now:?}"
            );
        }
    }

    /// With [`CHECK_SIGHT`] set, panics where a piece left out of `ids`
    /// would reveal or mark blind: a change that did not mark it.
    fn check_sight(&self, ids: &BTreeSet<u32>) {
        if CHECK_SIGHT.on() {
            let missed = self.pieces.keys().copied().find(|id| {
                let now = self.sighted_now(*id);
                let kept = self.units.get(*id).and_then(|u| u.sighted);

                !ids.contains(id) && (now.is_none() || now != kept)
            });

            assert!(
                missed.is_none(),
                "sight missed #{missed:?}: {}",
                missed.map_or_else(String::new, |id| self.describe(id))
            );
        }
    }

    /// Whether piece `id` reveals where it stands, place aside: a building
    /// or a mobile, not held. `true` where its place is not known.
    fn reveals(&self, id: u32) -> bool {
        self.pieces.place(id).is_none_or(|p| {
            let kind = p.is_building() || p.is_vehicle();

            kind && p.state & piece_state::HELD == 0
        })
    }

    /// The explored ground now.
    #[must_use]
    pub const fn explored(&self) -> &Explored {
        &self.fog.explored
    }

    /// The sight words a unit sampled at its cell holds at point `at`,
    /// 32nds, by the explored ground ([`super::Exposure::Point`]).
    pub(super) fn point_words(&self, at: (i32, i32)) -> Words {
        self.fog.explored.cell_words((cell_of(at.0), cell_of(at.1)))
    }

    /// Unit seen level: the highest of piece `id`'s
    /// sight words over the players `mask` names, its holder chain's
    /// root's, as the last sight pass left them. 0 where
    /// unknown. The iso view draws a unit only above 0
    /// ([`crate::view::map::fog`]).
    #[must_use]
    pub fn seen_level(&self, id: u32, mask: u8) -> i32 {
        let root = std::iter::successors(Some(id), |at| {
            let holder = self.any_piece(*at)?.place?.holder;

            (holder != 0).then(|| self.holder(holder)).flatten()
        })
        .take(HOLDERS_MOST)
        .last()
        .unwrap_or(id);

        self.units.words(root).map_or(0, |w| level_of(w, mask))
    }

    /// Effect `effect`'s seen level over the players `mask` names: its
    /// holder's while hung; alone, its cell's, but a message flag's own
    /// words are 100 for all.
    #[must_use]
    pub fn effect_seen(&self, effect: &super::effectors::Effect, mask: u8) -> i32 {
        match (effect.holder, effect.code == super::flag::MESSAGE_FLAG) {
            (0, true) => SEEN,
            (0, false) => self.cell_seen(effect.at, mask),
            (holder, _) => self.seen_level(holder, mask),
        }
    }

    /// Missile `missile`'s seen level over the players `mask` names: its
    /// cell's.
    #[must_use]
    pub fn missile_seen(&self, missile: &super::missile::Flown, mask: u8) -> i32 {
        self.cell_seen(missile.at, mask)
    }

    /// The explored ground's level at the cell holding point `at`, 16.16.
    fn cell_seen(&self, at: [Fixed; 3], mask: u8) -> i32 {
        self.explored().seen_at((whole(at[0]), whole(at[1])), mask)
    }

    /// `GSUnit`: piece `id`'s sight word for player
    /// `player`; a held reward or message flag answers with its holder's.
    /// 0 where unknown. A unit's sound tests it for the local player,
    /// not [`World::seen_level`].
    #[must_use]
    pub fn sight_word(&self, id: u32, player: u32) -> i32 {
        let place = self.any_piece(id).and_then(|p| p.place);
        let flag = place.filter(|p| (p.is_reward() || p.kind == MESSAGE_FLAG) && p.holder != 0);
        let holder = flag.and_then(|p| self.holder(p.holder));
        let own = || {
            let word = usize::try_from(player).ok();

            self.units
                .words(id)
                .and_then(|w| w.get(word?))
                .copied()
                .unwrap_or(0)
        };

        match holder {
            Some(h) if h != id => self.sight_word(h, player),
            _ => own(),
        }
    }

    /// Whether player `player` sees piece `id` where it stands, by the
    /// explored ground; `None` where its place or shape is unknown, or
    /// the ground unexplored where a piece of the player's revealed
    /// unseen ([`Fog::blind`]).
    pub(super) fn sees(&self, id: u32, player: u32) -> Option<bool> {
        let blind = self.fog.blind & 1u8.wrapping_shl(player) != 0;

        self.sees_now(id, player)
            .and_then(|s| (s || !blind).then_some(s))
    }

    /// [`World::sees`] by the explored ground alone.
    fn sees_now(&self, id: u32, player: u32) -> Option<bool> {
        let place = self.known(id)?;

        if place.is_building() {
            Some(self.fog.explored.sees_rect(self.cells(id)?, player))
        } else {
            Some(self.fog.explored.sees_point(place.fixed, player))
        }
    }

    /// The cells piece `id`'s `PRIM +0x04..+0x10` rect covers where it
    /// stands: 32nds about unit pos32, over 32 toward zero.
    /// Not its pad rect.
    fn cells(&self, id: u32) -> Option<[i32; 4]> {
        self.rect_cells(self.any_piece(id)?.code, self.position(id)?)
    }

    /// The cells a unit of `code` at `at`, 32nds, covers: its `PRIM
    /// +0x04..+0x10` rect about it, over 32 toward zero.
    fn rect_cells(&self, code: FourCc, at: (i32, i32)) -> Option<[i32; 4]> {
        let rect = self.descriptors.shape(code)?.bounds;
        let (px, py) = at;

        Some([
            toward_zero(px + rect[0], PER_CELL),
            toward_zero(py + rect[1], PER_CELL),
            toward_zero(px + rect[2], PER_CELL),
            toward_zero(py + rect[3], PER_CELL),
        ])
    }

    /// `GE_Level`, after the scheduler each frame:
    /// every unit's `GenUnit`, which marks it where a
    /// vertex gained a bit since the last pass, then
    /// unit sight update; then the map's mark is cleared. A unit
    /// that moved is marked too (unit set pos,
    /// unit pos write). Here by agent id (**Inferred**: the engine
    /// walks list 3; only the mark's timing depends on the order).
    pub(super) fn sight_pass(&mut self) {
        self.fog.passes += 1;
        self.sight();

        let mut ids: Vec<u32> = self.pieces.all().map(|(id, _)| *id).collect();

        ids.sort_unstable();
        ids.dedup();

        for id in ids {
            // A step's every write marks the unit,
            // not a new half cell alone.
            let piece = self.any_piece(id);
            let now = piece
                .and_then(|p| super::places::known_of(p, self.units.motion(id)))
                .map(|p| (p.fixed, p.state & piece_state::HELD))
                .zip(piece.map(|p| p.code.0));
            let moved = self.units.get(id).and_then(|u| u.passed.as_ref()) != now.as_ref();

            // Unmoved, the record holds `now` already.
            if let (true, Some(now)) = (moved, now) {
                self.units.entry(id).passed = Some(now);
            }

            if moved || self.fog.explored.fresh {
                self.update_words(id);
            }
        }

        self.scatter_sight();
        self.fog.explored.fresh = false;
    }

    /// The sight pass over [`Board::scatter`](super::board::Board::scatter): a terrain feature is a unit
    /// of list 3 too (terrain feature list new builds list 2 from
    /// it), so `GenUnit` samples one with `RevealTerrainIfExposed`,
    /// a pond or a geyser, over its rect, and the second reveal runs about
    /// it once a player sees it. A scatter object never moves: marked
    /// when first met and where a vertex gained a bit. After the pieces
    /// (**Inferred**: list 3's order, only the mark's timing depends on
    /// it). No owner word is set (unit owner below 0, unit sight update; a
    /// save's scatter has no owner).
    ///
    /// **Game**, the probed Barren Waste: the saves' explored ground the
    /// world lacked lay about the ponds at (62, 0) and (91, 85), and Ranger
    /// 40's answer at 3855 comes right with the geyser's at (62, 35).
    fn scatter_sight(&mut self) {
        let descriptors = &self.descriptors;
        let exposed = self.board.exposed(|c| exposed(descriptors, c)).to_vec();

        self.check_exposed(&exposed);

        for (key, fixed, sight) in exposed {
            let marked = self.fog.explored.fresh || !self.fog.scatter_words.contains_key(&key);
            let at = (space::pos32(fixed.0), space::pos32(fixed.1));

            if let (true, Some(cells)) = (marked, self.rect_cells(key.0, at)) {
                let old = self
                    .fog
                    .scatter_words
                    .get(&key)
                    .copied()
                    .unwrap_or_default();
                let mut words = self.fog.explored.rect_words(cells);

                if words != old {
                    self.fog.explored.second_reveal(cells, &mut words);
                }

                if sight == super::Sight::Owner {
                    words = [0; PLAYERS];
                }

                self.fog.scatter_words.insert(key, words);
            }
        }
    }

    /// Unit sight update on a marked unit: its
    /// words from the ground, by its rect or its cell ([`super::Exposure`]).
    /// Where a rect's words changed, the second reveal
    /// ([`Explored::second_reveal`]). Then
    /// `NotVisibleToOpponent` zeroes every word, and the owner's is
    /// [`SEEN`]. Words unknown where the place is.
    fn update_words(&mut self, id: u32) {
        let old = self.units.words(id).copied().unwrap_or_default();
        let piece = self.any_piece(id);
        let owner = piece.map_or(0, |p| p.owner);
        let shape = piece.and_then(|p| self.descriptors.shape(p.code)).copied();
        let computed = shape.and_then(|shape| match shape.exposure {
            super::Exposure::Rect => self
                .cells(id)
                .map(|cells| (self.fog.explored.rect_words(cells), Some(cells))),
            super::Exposure::Point => self
                .position(id)
                .map(|(x, y)| (self.fog.explored.cell_words((cell_of(x), cell_of(y))), None)),
        });

        match (computed, shape) {
            (Some((mut words, rect)), Some(shape)) => {
                let changed = words != old;

                if let (true, Some(rect)) = (changed, rect) {
                    self.fog.explored.second_reveal(rect, &mut words);
                }

                if shape.sight == super::Sight::Owner {
                    words = [0; PLAYERS];
                }

                if let Some(w) = usize::try_from(owner).ok().and_then(|o| words.get_mut(o)) {
                    *w = SEEN;
                }

                self.units.entry(id).words = Some(words);

                // Words changed: lists 8 to 15 drop the unit, then take it
                // again at their tail.
                if changed {
                    self.units.entry(id).relisted = Some(self.fog.passes);
                    self.relist_flag(id);
                }
            }
            _ => self.unword(id),
        }
    }

    /// Piece `id`'s words and its place in the lists, no longer known.
    fn unword(&mut self, id: u32) {
        if let Some(u) = self.units.get_mut(id) {
            u.words = None;
            u.relisted = None;
        }
    }

    /// A held reward flag's place in the eight `VisibleUnitList`s, taken
    /// again as its words change: `VisibleUnitList` reads each
    /// player's word through `GSUnit`, the holder's
    /// for a held flag. Kept until its words next change, whatever its
    /// holder's do meanwhile.
    fn relist_flag(&mut self, id: u32) {
        let holder = self
            .any_piece(id)
            .and_then(|p| p.place)
            .filter(|p| p.is_reward() && p.holder != 0)
            .and_then(|p| self.holder(p.holder));

        match holder.and_then(|h| self.units.words(h).copied()) {
            Some(words) => {
                let mask = (0u32..)
                    .zip(words.iter())
                    .filter(|(_, w)| **w > 0)
                    .fold(0u8, |m, (p, _)| m | 1u8.wrapping_shl(p));

                self.units.entry(id).flag_list = Some(mask);
            }
            None => {
                if let Some(u) = self.units.get_mut(id) {
                    u.flag_list = None;
                }
            }
        }
    }

    /// `GetNearestHiddenCoord(agent, coord&)`, gpl get nearest hidden coord: throws for a
    /// dead agent, or a unit off the cells; one GPL draw; the owner's
    /// nearest unexplored cell from the unit's ([`Explored::nearest_hidden`])
    /// as a coordinate, at least [`HIDDEN_MARGIN`] from the left and top
    /// edge, and pulled in from the right and bottom only past the board;
    /// written to argument 2, true. None: false.
    ///
    /// Unknown where the owner may have explored unseen ([`Fog::blind`]),
    /// or the unit's place is not known.
    pub(super) fn nearest_hidden_coord(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let id = agent_ref(vm, args, 1)?;
        let out = vm.arg(args, 2)?;

        if !matches!(vm.object(out), Value::Coord(..)) {
            Err(OpError::BadConversion(Accessor::CoordRef))?;
        }

        let owner = match self.found(id) {
            Found::Piece(p) => Some(p.owner),
            Found::Gone if !self.agents.alive(id) => Err(Fault::throw(
                "Tried to get GameObjectID for non-existent agent",
            ))?,
            _ => None,
        };

        self.sight();

        let cells = self.fog.explored.across - 1;
        let from = owner
            .filter(|o| self.fog.blind & self.players.reveal_mask(*o) == 0)
            .zip(self.position(id))
            .map(|(o, (x, y))| (o, (cell_of(x), cell_of(y))));
        let off = |c: i32| !(0..cells).contains(&c);

        match from {
            Some((_, (x, y))) if off(x) || off(y) => Err(Fault::throw(
                "GetNearestHiddenCoord() called for off-map unit",
            )),
            Some((owner, at)) => {
                let draw = self.native_draw(vm, HIDDEN_DRAW);
                let found = self.fog.explored.nearest_hidden(at, owner, draw);

                let board = cells * PER_CELL;
                let fit = |v: i32| {
                    let v = v.max(HIDDEN_MARGIN);

                    if v >= board { board - HIDDEN_MARGIN } else { v }
                };

                if let Some((x, y)) = found {
                    *vm.object_mut(out) = Value::Coord(fit(x * PER_CELL), fit(y * PER_CELL));
                }

                let ret = vm.arg(args, 0)?;

                super::put_bool(vm, ret, found.is_some()).map(|()| Answer::Given)
            }
            None => Ok(Answer::Unknown),
        }
    }

    /// `RevealArea(unit, point, radius)`, gpl reveal area: throws with no
    /// unit; the unit's owner's mask about the point's cell, radius / 32
    /// cells; a radius below 0 reveals everything.
    pub(super) fn reveal_area(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let id = agent_ref(vm, args, 1)?;
        let owner = match self.found(id) {
            Found::Piece(p) => Some(p.owner),
            Found::Gone => Err(Fault::throw("RevealArea() unit is null"))?,
            Found::Unsure => None,
        };
        let at = match vm.object(vm.arg(args, 2)?) {
            Value::Coord(x, y) => (cell_of(*x), cell_of(*y)),
            _ => Err(OpError::BadConversion(Accessor::CoordRef))?,
        };
        let radius = int_ref(vm, args, 3)?;

        if owner.is_none() {
            self.fog.blind = u8::MAX;
        }

        match owner {
            Some(owner) => {
                let mask = self.players.reveal_mask(owner);

                self.sight();

                if radius < 0 {
                    self.fog.explored.everything(mask);
                } else {
                    self.fog
                        .explored
                        .disc(at, mask, toward_zero(radius, PER_CELL));
                }

                Ok(Answer::Given)
            }
            None => Ok(Answer::Unknown),
        }
    }

    /// `RevealWholeMap(player)`, gpl reveal whole map: every vertex explored for
    /// `player`'s own bit, then the sight pass
    /// at once (`GE_Level`, the tail jump in gpl reveal whole map).
    pub(super) fn reveal_whole_map(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let player = int_ref(vm, args, 1)?.cast_unsigned();

        self.fog.explored.everything(1u8.wrapping_shl(player));
        self.sight_pass();

        Ok(Answer::Given)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(side: i32) -> Explored {
        Explored {
            across: side + 1,
            flags: Chunks::filled(0, ((side + 1) * (side + 1)).cast_unsigned() as usize),
            fresh: false,
        }
    }

    fn row(e: &Explored, y: i32) -> String {
        (0..e.across)
            .map(|x| {
                if e.flags.get(e.index(x, y).unwrap()) & 1 != 0 {
                    'x'
                } else {
                    '.'
                }
            })
            .collect()
    }

    /// A disc of two cells about cell (3, 3): the vertices within two of
    /// the cell's middle, (3.5, 3.5); (2, 2) is 4.5 away squared.
    #[test]
    fn a_disc_is_the_vertices_near_its_cells_middle() {
        let mut e = field(7);

        e.disc((3, 3), 1, 2);

        let rows: Vec<String> = (0..8).map(|y| row(&e, y)).collect();

        assert_eq!(
            rows,
            [
                "........", "........", "...xx...", "..xxxx..", "..xxxx..", "...xx...", "........",
                "........",
            ]
        );
    }

    /// A building four cells wide at cell (10, 10), its `PRIM` rect ±64
    /// 32nds: unit sight update samples cells 8 to 12, x1
    /// exclusive, so an explored vertex at its top-left corner shows it.
    /// Its pad rect, ±4 half cells, would sample only cells 9 and 10.
    fn building_world(explored: (i32, i32)) -> World {
        let mut world = World::default();
        let code = *b"ALAR";
        let at = from_whole(10);

        world.fog.explored = field(16);

        if let Some(i) = world.fog.explored.index(explored.0, explored.1) {
            *world.fog.explored.flags.get_mut(i) = 1;
        }

        world.descriptors_mut().add_shape(
            FourCc(code),
            super::super::Shape {
                rect: [-4, -4, 4, 4],
                bounds: [-64, -64, 64, 64],
                sight: super::super::Sight::Everyone,
                exposure: super::super::Exposure::Rect,
            },
        );
        world.pieces.insert(
            5,
            super::super::Piece {
                owner: 7,
                code: FourCc(code),
                place: Some(super::super::Place {
                    kind: kind::Kind::BUILDING,
                    fixed: (at, at),
                    standing: (20, 20),
                    holder: 0,
                    state: 0,
                }),
                ..Default::default()
            },
        );
        world
    }

    #[test]
    fn a_building_is_seen_by_the_cells_its_prim_rect_covers() {
        assert_eq!(building_world((8, 8)).sees_now(5, 0), Some(true));
        assert_eq!(building_world((12, 13)).sees_now(5, 0), Some(true));
        assert_eq!(building_world((13, 13)).sees_now(5, 0), Some(false));
    }

    /// Player 0 sees one corner, vertex (8, 8), of player 7's building:
    /// the pass reveals for player 0 a disc of radius 3 about cell
    /// (10, 10), the rect's cells 8 to 12 halved, and the word goes to
    /// 100; the owner's is 100.
    #[test]
    fn a_building_seen_in_part_reveals_about_its_rect() {
        let mut world = building_world((8, 8));

        world.fog.explored.fresh = true;
        world.sight_pass();

        let rows: Vec<String> = (6..16)
            .map(|y| row(&world.fog.explored, y)[6..16].to_owned())
            .collect();

        assert_eq!(
            rows,
            [
                "..........",
                "..........",
                "..xxxxx...",
                "..xxxxxx..",
                "..xxxxxx..",
                "..xxxxxx..",
                "..xxxxxx..",
                "...xxxx...",
                "..........",
                "..........",
            ]
        );
        assert_eq!(
            world.units.words(5).copied().map(|w| (w[0], w[7])),
            Some((SEEN, SEEN))
        );
        assert!(!world.fog.explored.fresh);
    }

    /// A scatter object at cell (10, 10), its `PRIM` rect ±64 32nds as
    /// [`building_world`]'s, vertex (8, 8) explored for player 0, sampled
    /// over its rect where `exposure` says.
    fn pond_world(exposure: super::super::Exposure) -> World {
        let mut world = World::default();
        let code = FourCc(*b"CG01");
        let at = from_whole(10);

        world.fog.explored = field(16);

        if let Some(i) = world.fog.explored.index(8, 8) {
            *world.fog.explored.flags.get_mut(i) = 1;
        }

        world.descriptors_mut().add_shape(
            code,
            super::super::Shape {
                rect: [-4, -4, 4, 4],
                bounds: [-64, -64, 64, 64],
                sight: super::super::Sight::Everyone,
                exposure,
            },
        );
        world.board.lay(
            code,
            (20, 20),
            super::super::building_ground::Stand {
                fixed: (at, at),
                height: Fixed::ZERO,
            },
        );
        world
    }

    /// A terrain feature with `RevealTerrainIfExposed`, as the scorch
    /// ponds: seen at one corner, it reveals for player 0 what a building
    /// there would ([`a_building_seen_in_part_reveals_about_its_rect`]),
    /// and no further; one without reveals nothing.
    #[test]
    fn a_pond_seen_in_part_reveals_about_its_rect() {
        let mut pond = pond_world(super::super::Exposure::Rect);
        let mut shrub = pond_world(super::super::Exposure::Point);
        let mut building = building_world((8, 8));

        pond.sight_pass();
        shrub.sight_pass();
        building.fog.explored.fresh = true;
        building.sight_pass();

        let player_0 = |w: &World| -> Vec<u8> { w.fog.explored.flags().map(|f| f & 1).collect() };
        let key = (FourCc(*b"CG01"), (20, 20));

        assert_eq!(player_0(&pond), player_0(&building));
        assert!(!pond.fog.explored.fresh);

        // Marked again: at most the same disc, nothing more.
        pond.fog.explored.fresh = true;
        pond.sight_pass();

        assert_eq!(player_0(&pond), player_0(&building));
        assert_eq!(pond.fog.scatter_words.get(&key).map(|w| w[0]), Some(SEEN));
        assert_eq!(player_0(&shrub).iter().filter(|f| **f != 0).count(), 1);
        assert!(shrub.fog.scatter_words.is_empty());
    }

    /// Player 0's attack flag 9 hung on player 7's building 5: listed for
    /// player 0 by the building's sight word, not its own owner's.
    /// Unseen while no vertex of the
    /// building's rect is explored; seen once one is.
    #[test]
    fn a_held_flag_is_seen_as_its_holder_is() {
        let flagged = |explored| {
            let mut world = building_world(explored);

            if let Some(holder) = world.pieces.get_mut(5) {
                holder.handle = 50;
            }

            world.agents.insert(5, super::super::Agent::default());
            world.pieces.insert(
                9,
                super::super::Piece {
                    owner: 0,
                    handle: 51,
                    code: FourCc(*b"ARA2"),
                    place: Some(super::super::Place {
                        kind: kind::Kind::REWARD,
                        fixed: (Fixed::ZERO, Fixed::ZERO),
                        standing: (0, 0),
                        holder: 50,
                        state: piece_state::HELD,
                    }),
                    ..Default::default()
                },
            );
            world
        };
        let listed = |world: &World| {
            let flag = world.pieces.get(9).cloned().unwrap_or_default();

            world.listed(
                (9, &flag, world.known(9)),
                super::super::search::Lists::Seen,
                0,
            )
        };

        assert_eq!(listed(&flagged((0, 0))), Some(false));
        assert_eq!(listed(&flagged((8, 8))), Some(true));

        // Relisted while player 0 saw the building, it stays listed after
        // the building's word drops, until its own words change.
        let mut kept = flagged((0, 0));

        kept.units.entry(5).words = Some([SEEN, 0, 0, 0, 0, 0, 0, SEEN]);
        kept.relist_flag(9);
        kept.units.entry(5).words = Some([0, 0, 0, 0, 0, 0, 0, SEEN]);

        assert_eq!(listed(&kept), Some(true));

        kept.relist_flag(9);

        assert_eq!(listed(&kept), Some(false));
    }

    /// Unit seen level: the most of the words the mask names, the
    /// holder chain's root's. A flag hung on player 7's building, seen
    /// in part by player 0 through two corners of its rect's samples.
    #[test]
    fn a_units_seen_level_is_its_roots_most_masked_word() {
        let mut world = building_world((8, 8));

        if let Some(holder) = world.pieces.get_mut(5) {
            holder.handle = 50;
        }

        world.pieces.insert(
            9,
            super::super::Piece {
                owner: 0,
                handle: 51,
                code: FourCc(*b"ARA2"),
                place: Some(super::super::Place {
                    kind: kind::Kind::REWARD,
                    fixed: (Fixed::ZERO, Fixed::ZERO),
                    standing: (0, 0),
                    holder: 50,
                    state: 0,
                }),
                ..Default::default()
            },
        );
        world.units.entry(5).words = Some([25, 50, 0, 0, 0, 0, 0, SEEN]);

        assert_eq!(world.seen_level(5, 0b0000_0001), 25);
        assert_eq!(world.seen_level(5, 0b0000_0011), 50);
        assert_eq!(world.seen_level(9, 0b1000_0000), SEEN);
        assert_eq!(world.seen_level(9, 0), 0);
        assert_eq!(world.seen_level(77, 1), 0);
    }

    /// `GSUnit`: one player's word, a held flag its holder's.
    #[test]
    fn a_sight_word_is_one_players_a_held_flags_its_holders() {
        let mut world = building_world((8, 8));

        if let Some(holder) = world.pieces.get_mut(5) {
            holder.handle = 50;
        }

        world.pieces.insert(
            9,
            super::super::Piece {
                owner: 0,
                handle: 51,
                code: FourCc(*b"ARA2"),
                place: Some(super::super::Place {
                    kind: kind::Kind::REWARD,
                    fixed: (Fixed::ZERO, Fixed::ZERO),
                    standing: (0, 0),
                    holder: 50,
                    state: 0,
                }),
                ..Default::default()
            },
        );
        world.units.entry(5).words = Some([25, 50, 0, 0, 0, 0, 0, SEEN]);
        world.units.entry(9).words = Some([SEEN; PLAYERS]);

        assert_eq!(world.sight_word(5, 0), 25);
        assert_eq!(world.sight_word(5, 1), 50);
        assert_eq!(world.sight_word(9, 2), 0);
        assert_eq!(world.sight_word(9, 7), SEEN);
        assert_eq!(world.sight_word(77, 0), 0);
    }

    /// A point unit's level is 25 a corner of its cell explored.
    #[test]
    fn a_point_is_seen_by_its_cells_corners() {
        let mut e = field(4);

        for (x, y) in [(1, 1), (2, 1), (2, 2)] {
            if let Some(i) = e.index(x, y) {
                *e.flags.get_mut(i) = 1;
            }
        }

        assert_eq!(e.seen_at((1, 1), 1), 3 * PER_CORNER);
        assert_eq!(e.seen_at((0, 0), 1), PER_CORNER);
        assert_eq!(e.seen_at((1, 1), 2), 0);
        assert_eq!(e.seen_at((9, 9), 1), 0);
    }

    /// The searcher of [`spell_world`], player 0's abode.
    const ABODE: u32 = 14;

    /// The two spell units of [`spell_world`].
    const GATE: u32 = 350;
    const STORM: u32 = 358;

    /// `LowLevel.gpl`'s `#MyPlayer` and `#FindFirstMatchOnly`.
    const MY_PLAYER: i32 = 1;
    const FIRST_ONLY: i32 = 9;

    /// Player 0's abode on ground player 0 has explored, and no spell.
    fn spell_world() -> World {
        let mut world = World {
            fog: Fog::new(field(16), 0),
            ..Default::default()
        };

        world.fog.explored = Explored::of_flags(16, std::iter::repeat(1));
        world.descriptors_mut().add_shape(
            FourCc(*b"Spel"),
            super::super::Shape {
                rect: [0, 0, 1, 1],
                bounds: [0, 0, 0, 0],
                sight: super::super::Sight::Everyone,
                exposure: super::super::Exposure::Point,
            },
        );
        world.sight_pass();
        cast(&mut world, ABODE, 2);
        world
    }

    /// Player 0's spell unit `id`, made at cell (`x`, 8).
    fn cast(world: &mut World, id: u32, x: i32) {
        world.pieces.insert(
            id,
            super::super::Piece {
                owner: 0,
                code: FourCc(*b"Spel"),
                place: Some(super::super::Place {
                    kind: kind::Kind::VEHICLE,
                    fixed: (from_whole(x), from_whole(8)),
                    standing: (2 * x, 16),
                    holder: 0,
                    state: 0,
                }),
                ..Default::default()
            },
        );
        world.agents.insert(id, super::super::Agent::default());
        world
            .units
            .set_motion(id, super::super::places::Motion::Still);
    }

    /// `Gate_Hit`'s search, `ListObjects(abode, "spell", -1, list,
    /// #FindFirstMatchOnly, #MyPlayer)`: the answer and the list.
    fn first_spell(world: &mut World) -> (Answer, Value) {
        use crate::gpl::builtins;
        use crate::gpl::load::Program;
        use crate::gpl::vm::Config;
        use crate::gpl::vm::host::Call;

        let program = Program::load::<&[u8]>(&[]).expect("empty program");
        let mut vm = Vm::new(&program, Config::default());
        let builtin = builtins::id("ListObjects").expect("a builtin");
        let args = [
            Value::Integer(0),
            Value::Agent(ABODE),
            Value::String("spell".into()),
            Value::Integer(-1),
            Value::List(Vec::new().into()),
            Value::Integer(FIRST_ONLY),
            Value::Integer(MY_PLAYER),
        ];
        let handles: Vec<Handle> = args
            .into_iter()
            .map(|v| {
                let o = vm.alloc(v);
                vm.top_handle(Some(o))
            })
            .collect();
        let call = Call {
            builtin,
            args: &handles,
        };
        let typed =
            |kind: &str| -> &'static Value { Box::leak(Box::new(Value::String(kind.into()))) };
        let (building, spelled) = (typed("building"), typed("spell"));
        let spell: &super::super::Attributes<'_> = &|_, id, name| {
            let kind = if id == ABODE { building } else { spelled };

            (name == super::super::search::TYPE).then_some(kind)
        };
        let answer = world
            .run(&mut vm, &call, &spell, crate::sim::world::step::Draws::Kept)
            .expect("an answer");

        (answer, vm.value(handles[4]).cloned().unwrap_or(Value::Null))
    }

    /// `VisibleUnitList` 8 + owner is walked from its head, and a unit
    /// goes to its tail each time its words change (unit sight update:
    /// dropped from lists 8 to 15, then unit list insert's
    /// `push_back`). So `#FindFirstMatchOnly` takes the spell whose
    /// words changed longest ago: the storm, made first, though its agent
    /// id is higher; then the gate, once player 1 explores the storm's
    /// cell and the storm's words change.
    #[test]
    fn a_first_match_search_takes_the_unit_listed_first() {
        let mut world = spell_world();

        cast(&mut world, STORM, 4);
        world.sight_pass();
        cast(&mut world, GATE, 6);
        world.sight_pass();

        assert_eq!(
            first_spell(&mut world),
            (Answer::Given, Value::List(vec![Value::Agent(STORM)].into()))
        );

        if let Some(i) = world.fog.explored.index(4, 8) {
            *world.fog.explored.flags.get_mut(i) |= 2;
        }

        world.fog.explored.fresh = true;
        world.sight_pass();

        assert_eq!(
            first_spell(&mut world),
            (Answer::Given, Value::List(vec![Value::Agent(GATE)].into()))
        );
    }

    /// Two spells listed in one sight pass: the engine takes them in its
    /// own walk's order, and a sound's mid-frame update can move one, so
    /// the first is not known.
    #[test]
    fn a_first_match_between_units_listed_in_one_pass_is_unknown() {
        let mut world = spell_world();

        cast(&mut world, STORM, 4);
        cast(&mut world, GATE, 6);
        world.sight_pass();

        assert_eq!(first_spell(&mut world).0, Answer::Unknown);
    }

    #[test]
    fn a_disc_of_no_radius_reveals_nothing() {
        let mut e = field(4);

        e.disc((2, 2), 1, 0);
        assert!(e.flags().all(|f| f == 0));
    }
}

/// To and from the save model.
mod persist {
    use super::*;
    use crate::save::{self as m, Refused, Saved};
    use std::result::Result;

    impl Saved for Explored {
        type Model = m::Explored;

        fn save(&self) -> m::Explored {
            m::Explored {
                across: self.across,
                flags: self.flags().collect(),
                fresh: self.fresh,
            }
        }

        fn load(model: m::Explored) -> Result<Explored, Refused> {
            // A flag a vertex: `index` checks a vertex against `across` only.
            let across = usize::try_from(model.across).map_err(Refused::Number)?;

            if model.flags.len() == across * across {
                Ok(Explored {
                    across: model.across,
                    flags: Chunks::of(&model.flags),
                    fresh: model.fresh,
                })
            } else {
                Err(Refused::Length)
            }
        }
    }

    impl Saved for Sighting {
        type Model = m::Sighting;

        fn save(&self) -> m::Sighting {
            match self {
                Sighting::Live => m::Sighting::Live,
                Sighting::Frozen => m::Sighting::Frozen,
            }
        }

        fn load(model: m::Sighting) -> Result<Sighting, Refused> {
            Ok(match model {
                m::Sighting::Live => Sighting::Live,
                m::Sighting::Frozen => Sighting::Frozen,
            })
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// Flags short of `across` squared are refused: a vertex inside
        /// `across` would index past them.
        #[test]
        fn short_explored_flags_are_refused() {
            let model = |flags: usize| m::Explored {
                across: 3,
                flags: vec![0; flags],
                fresh: false,
            };

            assert!(Explored::load(model(9)).is_ok());
            assert!(matches!(Explored::load(model(8)), Err(Refused::Length)));
            assert!(matches!(Explored::load(model(10)), Err(Refused::Length)));
        }
    }
}
