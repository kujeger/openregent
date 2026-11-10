//! What a unit finds: `ListObjects`' search over the unit lists, and
//! `LocationOf` and `DistanceBetweenAgents`. **Binary** unless marked.

use super::{
    Answer, Attributes, Found, NO_AGENT_ID, Piece, Place, Sight, TITLE, World, agent_ref, int_ref,
    list_of, put_int, registered, string_ref,
};
use crate::gpl::ops;
use crate::gpl::ops::Value;
use crate::gpl::vm::{Fault, Handle, Vm};
use crate::sim::space;

use super::reach::Reach;

/// `ListObjects`' integer modifiers: `LowLevel.gpl`'s `#NoHiddenMap` to
/// `#FindFirstMatchOnly`, the switch in gpl list objects. Any other integer is
/// an attribute key, followed by the value it must hold.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Modifier {
    NoHiddenMap,
    MyPlayer,
    NotMyPlayer,
    MyTeam,
    NotMyTeam,
    InsideOtherUnits,
    RewardFlags,
    CheckTitles,
    CheckSubtypes,
    FindFirstMatchOnly,
}

/// `ListObjects`' integer modifiers by value, as the SDK's `LowLevel.gpl`
/// names them.
pub const MODIFIERS: [&str; 10] = [
    "NoHiddenMap",
    "MyPlayer",
    "NotMyPlayer",
    "MyTeam",
    "NotMyTeam",
    "InsideOtherUnits",
    "RewardFlags",
    "CheckTitles",
    "CheckSubtypes",
    "FindFirstMatchOnly",
];

/// `ListObjects`' first modifier argument.
const FIRST_MODIFIER: usize = 5;

/// The GPL attribute a search's type argument must equal, str type.
pub const TYPE: &str = "type";

/// The GPL attribute `#CheckSubtypes` compares.
pub const SUBTYPE: &str = "subtype";

/// The type `#RewardFlags` searches for, str reward flag.
pub const REWARD_FLAG: &str = "RewardFlag";

/// Where a search looks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Lists {
    /// Unit list 8 + owner, `VisibleUnitList`: what the searcher's owner
    /// sees.
    Seen,
    /// `#NoHiddenMap`: unit lists 0, 1 and 4, every building, mobile and
    /// flag.
    Everything,
}

/// Which side of a comparison a search keeps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Side {
    Same,
    Other,
}

/// Whether a search takes held pieces.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Inside {
    Skip,
    Take,
}

/// Whether a search stops at its first match, in list order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Matches {
    All,
    First,
}

/// Whether a walked piece is bound: its agent may be gone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Bound {
    Yes,
    No,
}

/// Who searches: the searcher, its owner, the owner's team as a player
/// bit each, and the squared range (below 0: any).
#[derive(Clone, Copy, Debug)]
struct Searcher {
    me: u32,
    owner: u32,
    mates: u32,
    range: i32,
}

/// A `ListObjects` search, its modifiers read.
#[derive(Clone, Debug)]
struct Search {
    /// Argument 2, or [`REWARD_FLAG`].
    kind: String,
    lists: Lists,
    player: Option<Side>,
    team: Option<Side>,
    inside: Inside,
    /// `#CheckTitles` or `#CheckSubtypes`: the attribute the last string
    /// modifier must equal.
    check: Option<&'static str>,
    against: Option<String>,
    /// Attribute keys and the values they must hold.
    attributes: Vec<(i32, i32)>,
    /// A coordinate modifier's half cell: distances are measured from it.
    center: Option<(i32, i32)>,
    matches: Matches,
}

/// Whether `test`'s side holds: `same` the comparison, asked only when
/// there is a side to keep.
fn side(test: Option<Side>, same: impl FnOnce() -> bool) -> Option<bool> {
    test.map_or(Some(true), |side| Some(same() == (side == Side::Same)))
}

/// Three-valued and: `Some(false)` if any is, `Some(true)` if all are.
/// Stops at the first false: later tests do not run.
fn all_of(tests: impl Iterator<Item = Option<bool>>) -> Option<bool> {
    let mut sure = true;
    let failed = tests
        .inspect(|t| sure &= t.is_some())
        .any(|t| t == Some(false));

    if failed {
        Some(false)
    } else {
        sure.then_some(true)
    }
}

impl Search {
    /// The modifiers from argument [`FIRST_MODIFIER`] on, as gpl list objects
    /// reads them: an integer by the switch, a string as the check's text,
    /// a coordinate over 16 toward zero; any other type is passed over.
    fn read(vm: &Vm<'_>, args: &[Handle], kind: String) -> std::result::Result<Search, Fault> {
        let mut search = Search {
            kind,
            lists: Lists::Seen,
            player: None,
            team: None,
            inside: Inside::Skip,
            check: None,
            against: None,
            attributes: Vec::new(),
            center: None,
            matches: Matches::All,
        };
        let mut k = FIRST_MODIFIER;

        while k < args.len() {
            match vm.object(vm.arg(args, k)?) {
                Value::Integer(n) => match Modifier::of(*n) {
                    Some(m) => search.apply(m),
                    None if registered(*n) => {
                        k += 1;
                        search.attributes.push((*n, int_ref(vm, args, k)?));
                    }
                    None => Err(Fault::throw(format!(
                        "$ListObjects() was given an unknown search modifier(=={n})"
                    )))?,
                },
                Value::String(s) => search.against = Some(s.clone()),
                Value::Coord(x, y) => {
                    search.center = Some((
                        x / crate::sim::units::PER_HALF,
                        y / crate::sim::units::PER_HALF,
                    ));
                }
                _ => {}
            }

            k += 1;
        }

        Ok(search)
    }

    fn apply(&mut self, m: Modifier) {
        match m {
            Modifier::NoHiddenMap => self.lists = Lists::Everything,
            Modifier::MyPlayer => self.player = Some(Side::Same),
            Modifier::NotMyPlayer => self.player = Some(Side::Other),
            Modifier::MyTeam => self.team = Some(Side::Same),
            Modifier::NotMyTeam => self.team = Some(Side::Other),
            // The switch falls through to `#InsideOtherUnits`.
            Modifier::RewardFlags => {
                REWARD_FLAG.clone_into(&mut self.kind);
                self.inside = Inside::Take;
            }
            Modifier::InsideOtherUnits => self.inside = Inside::Take,
            Modifier::CheckTitles => self.check = Some(TITLE),
            Modifier::CheckSubtypes => self.check = Some(SUBTYPE),
            Modifier::FindFirstMatchOnly => self.matches = Matches::First,
        }
    }
}

impl Modifier {
    /// In the order of their values, from 0: [`MODIFIERS`].
    pub(super) const ALL: [Modifier; 10] = [
        Modifier::NoHiddenMap,
        Modifier::MyPlayer,
        Modifier::NotMyPlayer,
        Modifier::MyTeam,
        Modifier::NotMyTeam,
        Modifier::InsideOtherUnits,
        Modifier::RewardFlags,
        Modifier::CheckTitles,
        Modifier::CheckSubtypes,
        Modifier::FindFirstMatchOnly,
    ];

    fn of(n: i32) -> Option<Modifier> {
        usize::try_from(n)
            .ok()
            .and_then(|i| Modifier::ALL.get(i))
            .copied()
    }
}

impl World {
    /// Whether piece `piece` is in the unit lists a search of `player`'s
    /// walks.
    ///
    /// `VisibleUnitList` takes a building, mobile or
    /// flag, not in ruins, whose sight word for the player, unit
    /// plus four per player, is set. Unit sight update sets the owner's always
    /// and every other player's from the ground under the unit: a vertex
    /// flag bit per player ([`World::sees`]). A player who had explored the
    /// whole map sees every piece: ground once explored stays so.
    /// Read from the words as the last sight pass left them
    /// ([`World::sight_pass`]), else from the ground at the call. **Game**:
    /// in the dwarves game a shadow beast stepping onto explored ground in
    /// frame 3,283's orders is not yet listed by the settlement's search
    /// there: the pass after the scheduler sets its word.
    ///
    /// An off-map lair is in neither: list 7, `OffMapLairList`, holds it,
    /// which no search walks, and no sight word is ever set for it, its
    /// owner's neither (**Game**: in the united winter trace an off-map
    /// lair's search finds the three on-map lairs, not its five siblings;
    /// a palace's `#NoHiddenMap` search, the three alone).
    /// `at` is the piece's [`World::known`] place.
    pub(super) fn listed(
        &self,
        (id, piece, at): (u32, &Piece, Option<&Place>),
        lists: Lists,
        player: u32,
    ) -> Option<bool> {
        let off_map = at
            .and_then(|p| self.unit_pos_at(p, 0))
            .is_some_and(|(x, _)| super::lairs::OFF_MAP.contains(&space::pos32(x)));

        match lists {
            _ if off_map => Some(false),
            // An agent's unit is a building, a mobile or a flag.
            Lists::Everything => Some(true),
            Lists::Seen => {
                let place = piece.place?;

                if place.ruined() {
                    Some(false)
                } else {
                    self.word(id, piece, player)
                }
            }
        }
    }

    /// A GPL string attribute of agent `id`: the VM's object where a run has
    /// one, else as made. `None` when unknown, or not a string.
    fn text<'s>(
        &'s self,
        vm: &'s Vm<'_>,
        gpl: &Attributes<'_>,
        id: u32,
        name: &str,
    ) -> Option<&'s str> {
        match gpl(vm, id, name) {
            Some(Value::String(s)) => Some(s),
            Some(_) => None,
            None => self
                .agents
                .get(id)?
                .values
                .iter()
                .find(|m| m.name.eq_ignore_ascii_case(name))
                .and_then(|m| match &m.value {
                    Value::String(s) => Some(s.as_str()),
                    _ => None,
                }),
        }
    }

    /// Whether piece `id` passes `search`, and how far it is from `from`,
    /// the searcher or the coordinate: the tests of list objects test, in any
    /// order, as none has an effect. Cheapest first; the first false ends
    /// it, and then the gap may be unknown. `any` is the piece
    /// [`World::any_piece`] gives for `id`: its place is looked up once.
    fn passes(
        &self,
        vm: &Vm<'_>,
        gpl: &Attributes<'_>,
        search: &Search,
        searcher: Searcher,
        from: Option<space::HalfRect>,
        (id, piece, any, bound): (u32, &Piece, &Piece, Bound),
    ) -> (Option<bool>, Option<i32>) {
        let measure = |at: Option<&Place>| {
            let foot = self
                .descriptors
                .shape(any.code)
                .zip(at)
                .and_then(|(shape, place)| {
                    Some(space::placed(shape.rect, self.spot_at(place, 0)?))
                });

            from.zip(foot).map(|(a, b)| space::gap2(b, a))
        };
        let equals = |name: &str, want: &str| {
            self.text(vm, gpl, id, name)
                .map(|t| t.eq_ignore_ascii_case(want))
        };

        // `all_of` spelt out: no test runs past the first false.
        let mut sure = true;
        let mut holds = |t: Option<bool>| {
            sure &= t.is_some();
            t != Some(false)
        };
        let mut at = None;
        let mut gap = None;
        let held =
            // `AE:0`, the agent id, is 0 on scatter.
            holds(Some(id != NO_AGENT_ID))
            && holds(Some(id != searcher.me))
            && holds(side(search.player, || piece.owner == searcher.owner))
            && holds(side(search.team, || {
                searcher.mates & 1u32.wrapping_shl(piece.owner) != 0
            }))
            // A ruin `DeleteAgent` left has no agent to list: false, as
            // if not walked. Asked after the cheap tests.
            && holds(Some(bound == Bound::No || self.agents.alive(id)))
            // `World::complete`: `any`'s handle.
            && holds(all_of(search.attributes.iter().map(|(k, v)| {
                match (piece.attributes.get(k), any.handle != 0) {
                    (Some(x), _) => Some(x == v),
                    (None, true) => Some(*v == 0),
                    (None, false) => None,
                }
            })))
            && {
                at = super::places::known_of(any, self.units.motion(id));

                holds(match (at, search.inside) {
                    (_, Inside::Take) => Some(true),
                    (Some(place), Inside::Skip) => Some(place.holder == 0),
                    (None, Inside::Skip) => None,
                })
            }
            // Out of range the gap is measured only for a match (below).
            && (searcher.range < 0 || {
                gap = measure(at);

                holds(gap.map(|d| d <= searcher.range))
            })
            && holds(self.listed((id, piece, at), search.lists, searcher.owner))
            && holds(equals(TYPE, &search.kind))
            && holds(match (search.check, &search.against) {
                (Some(name), Some(want)) => equals(name, want),
                _ => Some(true),
            });
        let verdict = if held {
            sure.then_some(true)
        } else {
            Some(false)
        };

        if verdict == Some(true) && searcher.range < 0 {
            gap = measure(at);
        }

        (verdict, gap)
    }

    /// `ListObjects(agent, type, range, list, modifiers…)`, gpl list objects.
    ///
    /// Every piece in the searched lists, not the searcher, not held unless
    /// `#InsideOtherUnits`, within range of the searcher or a coordinate
    /// modifier, of the player and team the modifiers ask, whose GPL `type`
    /// equals argument 2 and whose checked attributes hold. The list gets
    /// them nearest first, then by agent id (list objects insertion compares both
    /// words); the result is their count.
    ///
    /// Unknown when a piece could pass but its place, sight or attributes
    /// are not known, or two pass and one's distance is not, or which is
    /// listed first, for `#FindFirstMatchOnly`.
    pub(super) fn objects(
        &self,
        vm: &mut Vm<'_>,
        args: &[Handle],
        gpl: &Attributes<'_>,
    ) -> std::result::Result<Answer, Fault> {
        let ret = vm.arg(args, 0)?;
        let me = agent_ref(vm, args, 1)?;
        let owner = match self.found(me) {
            Found::Piece(p) => Some(p.owner),
            Found::Unsure => None,
            Found::Gone => Err(Fault::throw(
                "$ListObjects() was given a null agent to search with",
            ))?,
        };
        let kind = string_ref(vm, args, 2)?;
        let range = space::range2(int_ref(vm, args, 3)?);
        let list = vm.arg(args, 4)?;

        list_of(vm.object(list))?;

        let search = Search::read(vm, args, kind)?;
        let mut found = Vec::new();
        let mut unsure = owner.is_none();

        let from = match search.center {
            Some(c) => Some(space::point(c)),
            None => self.footprint(me),
        };

        // A player's team goes by its bit, `1 << (player & 31)`: the 32
        // bits on the owner's team, looked up once.
        let searcher = owner.map(|o| Searcher {
            me,
            owner: o,
            mates: (0..u32::BITS)
                .filter(|p| self.players.team_of(*p) == self.players.team_of(o))
                .fold(0, |m, p| m | 1 << p),
            range,
        });

        debug_assert!(self.reaches.built(), "World::rereach before ListObjects");

        // A bound piece known out of range is a sure no, passed over
        // unread ([`super::reach`]).
        let far = |reach: Reach| match (searcher, from, reach) {
            (Some(s), Some(origin), Reach::Rect(pad)) => {
                s.range >= 0 && space::gap2(pad, origin) > s.range
            }
            _ => false,
        };

        // Each with its `World::any_piece`: a bound one is its own.
        let listable = self
            .reaches
            .iter()
            .filter(|(_, reach)| !far(*reach))
            .filter_map(|(id, _)| self.pieces.get(id).map(|p| (id, p, p, Bound::Yes)))
            .chain(
                self.pieces
                    .unbound()
                    .iter()
                    .map(|(id, p)| (*id, p, self.any_piece(*id).unwrap_or(p), Bound::No)),
            );

        for (id, piece, any, bound) in listable {
            let (verdict, gap) = searcher.map_or((None, None), |s| {
                self.passes(vm, gpl, &search, s, from, (id, piece, any, bound))
            });

            match verdict {
                Some(true) => found.push((gap, id)),
                Some(false) => {}
                None => unsure = true,
            }
        }

        // `#FindFirstMatchOnly` keeps the first in the walk's order.
        let kept = match search.matches {
            _ if found.len() < 2 => Some(found),
            Matches::All => found.iter().all(|(gap, _)| gap.is_some()).then_some(found),
            Matches::First => self.first_listed(&found, search.lists).map(|f| vec![f]),
        };

        if let (false, Some(mut found)) = (unsure, kept) {
            found.sort_unstable_by_key(|(gap, id)| (gap.unwrap_or_default(), id.cast_signed()));

            let count = i32::try_from(found.len()).unwrap_or(i32::MAX);
            let items: Vec<Value> = found.iter().map(|(_, id)| Value::Agent(*id)).collect();

            *vm.object_mut(list) = Value::List(items.into());
            put_int(vm, ret, count)?;
            Ok(Answer::Given)
        } else {
            Ok(Answer::Unknown)
        }
    }

    /// Of `found`, the one a walk of `lists` meets first.
    /// `GEUnitList` walks a `VisibleUnitList` from its head, and a
    /// unit goes to its tail each time its words change
    /// ([`super::unit::Unit::relisted`]). Unknown where two changed in one sight
    /// pass: the engine's pass walks list 3 in join order, the world's
    /// agent ids, and a set or sound can update a unit mid-frame
    /// (`GenUnit` after unit play set, method; not
    /// modelled, docs/formats/bytecode.md); or for `#NoHiddenMap`'s lists 0, 1
    /// and 4, whose order is not followed. **Game**: no trace meets the
    /// case.
    fn first_listed(
        &self,
        found: &[(Option<i32>, u32)],
        lists: Lists,
    ) -> Option<(Option<i32>, u32)> {
        let passes: Option<Vec<u32>> = found
            .iter()
            .map(|(_, id)| self.units.get(*id).and_then(|u| u.relisted))
            .collect();
        let passes = passes.filter(|_| lists == Lists::Seen)?;
        let first = passes.iter().min()?;
        let alone = passes.iter().filter(|p| *p == first).count() == 1;

        passes
            .iter()
            .position(|p| p == first)
            .filter(|_| alone)
            .and_then(|i| found.get(i))
            .copied()
    }

    /// `LocationOf(agent)`, gpl location of: the unit's position as a
    /// coordinate, unit pos32: a held unit's is its holder's.
    /// Throws when there is no unit.
    pub(super) fn location(
        &self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let id = agent_ref(vm, args, 1)?;
        let at = match self.found(id) {
            Found::Piece(_) => self.position(id),
            Found::Unsure => None,
            Found::Gone => Err(Fault::throw(
                "LocationOf() tried to query a non-existent game-object",
            ))?,
        };

        match at {
            Some((x, y)) => {
                let ret = vm.arg(args, 0)?;

                ops::assign(vm.object_mut(ret), &Value::Coord(x, y))?;
                Ok(Answer::Given)
            }
            None => Ok(Answer::Unknown),
        }
    }

    /// `DistanceBetweenAgents(a, b)`, gpl distance between agents: agents distance, the floored
    /// root of the squared gap between the two pad rects, in coordinates.
    /// Throws when either has no unit.
    pub(super) fn distance(
        &self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let a = agent_ref(vm, args, 1)?;
        let b = agent_ref(vm, args, 2)?;
        let gone = || Fault::throw("DistanceBetween() tried to query a non-existent game-object");
        let gap = match (self.found(a), self.found(b)) {
            (Found::Gone, _) | (_, Found::Gone) => Err(gone())?,
            (Found::Piece(_), Found::Piece(_)) => self
                .footprint(a)
                .zip(self.footprint(b))
                .map(|(x, y)| space::gap2(x, y)),
            _ => None,
        };

        match gap {
            Some(d) => {
                let ret = vm.arg(args, 0)?;

                put_int(vm, ret, space::distance(d))?;
                Ok(Answer::Given)
            }
            None => Ok(Answer::Unknown),
        }
    }

    /// Whether piece `id`'s sight word for `player` is set, as
    /// `VisibleUnitList` reads it through `GSUnit`:
    /// a held reward flag answers with its holder's word,
    /// so a flag hung on a unit the player does not see is not listed,
    /// whoever owns it; any other piece its own, the owner's always set.
    /// The engine takes a unit into the lists or out only as its own
    /// words change, so a held flag keeps what its holder's word was then
    /// ([`World::relist_flag`]); where no change is seen yet, its holder's
    /// now (**Inferred**).
    pub(super) fn word(&self, id: u32, piece: &Piece, player: u32) -> Option<bool> {
        self.word_in(id, piece, player, 0)
    }

    /// [`World::word`], `depth` holders up: none past
    /// [`super::reveal::HOLDERS_MOST`].
    fn word_in(&self, id: u32, piece: &Piece, player: u32, depth: usize) -> Option<bool> {
        let place = piece
            .place
            .filter(|_| depth < super::reveal::HOLDERS_MOST)?;
        let sight = self.descriptors.shape(piece.code).map(|s| s.sight);
        let seen = self.fog.seen() & 1u8.wrapping_shl(player) != 0;
        let held = place.is_reward() && place.holder != 0;

        if let (true, Some(mask)) = (held, self.units.get(id).and_then(|u| u.flag_list)) {
            Some(mask & 1u8.wrapping_shl(player) != 0)
        } else if held {
            let holder = self.holder(place.holder)?;

            self.word_in(holder, self.any_piece(holder)?, player, depth + 1)
        } else if piece.owner == player {
            Some(true)
        } else {
            match (sight?, seen) {
                (Sight::Owner, _) => Some(false),
                (Sight::Everyone, true) => Some(true),
                (Sight::Everyone, false) => match self.units.words(id) {
                    Some(words) => usize::try_from(player)
                        .ok()
                        .and_then(|p| words.get(p))
                        .map(|w| *w > 0),
                    None => self.sees(id, player),
                },
            }
        }
    }
}
