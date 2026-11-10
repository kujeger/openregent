//! What a building's and a kingdom's end call: a palace's treasury, its
//! occupants, its agent's deletion, the loss and the victory. **Binary**
//! unless marked.
//!
//! ```text
//!  GetPlayerData(agent, field)     no unit: throw; the owner's field
//!                                  "gold": the player's APP:0
//!                                  "palace": the owner's first unit coded
//!                                    ABJ in unit list 0, its AE:0, else
//!                                    agent -1; the slot made an agent
//!                                  else: throw
//!  GetBuildingContents(agent)      no unit: nothing; else relation 2,
//!                                  INSIDE, in entry order: each AE:0
//!  DeleteAgent(agent)              UnitDelete(AE:0)
//!                                  at once, nested; then GSUnit writes
//!                                    AE:0 = 0. The unit stays, agentless
//!  DeclareLoss(player | agent)     an agent's owner (no unit: throw);
//!                                  game_end(player, 0, 0, 0)
//!  DeclareVictory(player | agent,  an agent's owner; the point: a coord,
//!    [coord | agent])              an agent's unit_pos32, none given the
//!                                  player's palace's (none: throw);
//!                                  game_end(player, 1, x, y)
//! ```
//!
//! `game_end` ends the game for the local player:
//! the end record takes the flag and
//! point, each call over the last, and the app goes to state 8 before the
//! next frame, pausing every sound (`GE_SoundManager`).
//! Any other player's end is a multiplayer message only (`GMTX` 296, 297).
//! **Game**: the defeat trace runs one more thread run after it, then ends.
//!
//! State 8 runs the game end step each frame, a step on the record's:
//!
//! ```text
//!  0  music stopped (GE_MusicManager), every sound (GE_SoundManager);
//!     won: at a point not (0, 0), an MVa3 victory_star there (minimap
//!       only), set 0x40; the advisor RA26 Advisor_Won_Game, phase GVS5;
//!       to 2
//!     lost: the advisor RA27 Advisor_Lost_Game; to 1
//!  2  won: AP11, "Game Over" (GMTX 310), "You are victorious." (309)
//!  3  won: the view to the point; #EndGameFadeDelay (1500) real ms on
//!  1  every 75 real ms a fade step; past 15 and the advisor done: to 4
//!  4  the end panel: AP60 won, AP61 lost (EndGameController)
//! ```
//!
//! The panel names the quest (freestyle: `GMTX` 80 "Freestyle Game") and
//! says `QEND` or `LOSX` by quest, `FRE0` for freestyle; its STATISTICS
//! button opens `AP84` or `AP83` (`EndSummaryController`), the player
//! stats ([`Summary`]); a won game's CONTINUE PLAYING
//! goes back to state 3.

use super::{Answer, Found, NO_AGENT_ID, Piece, World, agent_ref, list_of, string_ref};
use crate::data::attrib::{EXPERIENCE_LEVEL, FIRST_STAGE_BUILT, STARTED_WITH_THIS_UNIT};
use crate::data::fourcc::FourCc;
use crate::data::gpltype::Kind;
use crate::gpl::ops::{self, Value};
use crate::gpl::vm::{Crash, Fault, Handle, Null, Vm};
use crate::sim::world::kind;
use crate::sim::world::piece_state;

use super::treasury::GOLD;

/// `GetPlayerData`'s second field: `"palace"`.
pub const PALACE: &str = "palace";

/// The palace `GetPlayerData` finds: code `ABJ`, the low three bytes of
/// unit code (its low 24 bits compared with `0x4a4241` in palace filter). Not
/// `ListPalaces`' `ABm` too.
pub const PALACE_CODE: &[u8] = b"ABJ";

/// `GetPlayerData "palace"` with none: the agent id -1.
pub const NO_PALACE: u32 = u32::MAX;

/// How a player's game ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum End {
    /// `DeclareLoss`, or the cheat `I'M A LOSER BABY`: game end
    /// flag 0.
    Lost,
    /// Flag 1: `DeclareVictory`, or the cheat `VICTORY IS MINE`.
    Won,
}

impl End {
    /// The advisor state 8's first step says, phase [`super::sound::ADVISOR`],
    /// unplaced, the sound kept until it ends (`GE_SoundManager`,
    /// game end step).
    #[must_use]
    pub const fn advisor(self) -> FourCc {
        match self {
            End::Won => FourCc(*b"RA26"),
            End::Lost => FourCc(*b"RA27"),
        }
    }

    /// The end panel's heading: `AP60`'s string 5, "YOU HAVE WON!", or
    /// `AP61`'s 6, "YOU LOST!" (state 8's last step, game end step).
    #[must_use]
    pub const fn heading(self) -> (&'static [u8; 4], usize) {
        match self {
            End::Won => (b"AP60", 5),
            End::Lost => (b"AP61", 6),
        }
    }

    /// The verdict's table, `QEND` or `LOSX`, and a freestyle game's key
    /// in it: "Congratulations!  You have won the game." or "You have
    /// lost this game!" (`EndGameController`, quest win verdict;
    /// the steps in [`crate::view::menu::end::verdict`]). The
    /// freestyle quest is `FRE1` (**Corpus**: every freestyle save's quest
    /// id is `FRE12`); the expansion's own tables key it so and lack
    /// the fallback `FRE0`, which the base's, merged under them, hold.
    /// **Inferred**: `FRE1` is in the quest list the lookup asks first.
    #[must_use]
    pub const fn verdict(self) -> (&'static [u8; 4], FourCc) {
        let key = FourCc(*b"FRE1");

        match self {
            End::Won => (b"QEND", key),
            End::Lost => (b"LOSX", key),
        }
    }
}

/// A freestyle game's name on the end panel: `GMTX` 80, "Freestyle Game".
pub const FREESTYLE_NAME: usize = 80;

/// The end's fade: a step each this many real ms (`0x4b` added in
/// game end step).
pub const FADE_STEP_MS: u32 = 75;

/// Fade steps before the end panel, the advisor done too (a compare with `0x0f` at
/// game end step).
pub const FADE_STEPS: u32 = 15;

/// Where no point is given: the cheats' and `DeclareLoss`'s. At it the
/// end shows no star and moves no view.
pub const NO_POINT: (i32, i32) = (0, 0);

/// `DeclareVictory`'s argument vector, the return slot counted, up to
/// which the point is the palace's (a compare with 2 in gpl declare victory).
const PLAYER_ONLY: usize = 2;

impl World {
    /// How player `player`'s game ended, if it has.
    #[must_use]
    pub fn ended(&self, player: u32) -> Option<End> {
        self.players.end(player).map(|(how, _)| how)
    }

    /// Whether every human's game has ended: each of the session's
    /// [`World::humans`]. **Inferred**: the session's players are
    /// slots 0 up.
    #[must_use]
    pub fn humans_ended(&self) -> bool {
        self.humans().all(|p| self.ended(p).is_some())
    }

    /// The point player `player`'s end record holds, 32nds of a cell:
    /// where the victory star stands and the view goes. [`NO_POINT`] for
    /// none.
    #[must_use]
    pub fn end_point(&self, player: u32) -> Option<(i32, i32)> {
        self.players.end(player).map(|(_, at)| at)
    }

    /// Game end: player `player`'s game ends `how`, at `at`. Each
    /// call writes the record over the last.
    pub(super) fn end(&mut self, player: u32, how: End, at: (i32, i32)) {
        self.players.set_end(player, how, at);
    }

    /// CONTINUE PLAYING (`EndGameController` 0x1392): app state 3,
    /// so player `player`'s game runs again. **Inferred**: the engine
    /// keeps its end record, but only state 8 reads it and the next
    /// game end writes it over, so dropping it plays the same.
    pub fn play_on(&mut self, player: u32) {
        self.players.clear_end(player);
    }

    /// Player `player` surrenders: its game ends lost, as `DeclareLoss`
    /// ends it. openregent's own: the original has no surrender.
    pub fn surrender(&mut self, player: u32) {
        self.end(player, End::Lost, NO_POINT);
    }

    /// Unit owner of agent `id`, `missing` thrown with none; `None`
    /// where the world does not know whether it stands.
    fn owner_or(&self, id: u32, missing: &str) -> Result<Option<u32>, Fault> {
        match self.found(id) {
            Found::Piece(p) => Ok(Some(p.owner)),
            Found::Unsure => Ok(None),
            Found::Gone => Err(Fault::throw(missing)),
        }
    }

    /// `GetPlayerData(agent, field)`, gpl get player data: the unit, then the
    /// field by `_stricmp`. Unknown where a treasury is not followed.
    pub(super) fn player_data(&self, vm: &mut Vm<'_>, args: &[Handle]) -> Result<Answer, Fault> {
        let id = agent_ref(vm, args, 1)?;
        let field = string_ref(vm, args, 2)?;
        let Some(owner) = self.owner_or(id, "GetPlayerData() unit is null")? else {
            return Ok(Answer::Unknown);
        };

        if field.eq_ignore_ascii_case(GOLD) {
            match self.treasury(owner) {
                Some(gold) => {
                    let ret = vm.arg(args, 0)?;

                    super::put_int(vm, ret, gold).map(|()| Answer::Given)
                }
                None => Ok(Answer::Unknown),
            }
        } else if field.eq_ignore_ascii_case(PALACE) {
            let palace = self.palace_of(owner);

            retype_agent(vm, args, palace).map(|()| Answer::Given)
        } else {
            Err(Fault::throw(format!(
                "$GetPlayerData() could not get field \"{field}\""
            )))
        }
    }

    /// `level_unit_list(0, owner)` filtered by palace filter: the first
    /// [`PALACE_CODE`] unit `owner` holds, its `AE:0`; else [`NO_PALACE`].
    /// **Inferred**: unit list 0 in handle order, as `ListPalaces`.
    pub(super) fn palace_of(&self, owner: u32) -> u32 {
        let mut buildings: Vec<(u32, &Piece)> = self
            .pieces
            .iter()
            .map(|(id, p)| (if unbound(p) { NO_AGENT_ID } else { *id }, p))
            .filter(|(_, p)| p.owner == owner && p.code.0.starts_with(PALACE_CODE))
            .collect();

        buildings.sort_by_key(|(_, p)| p.handle);
        buildings.first().map_or(NO_PALACE, |(id, _)| *id)
    }

    /// `GetBuildingContents(agent)`, gpl get building contents: nothing with no unit;
    /// else the building's relation 2 list, `INSIDE`, each member's agent,
    /// appended. A unit enters at the list's end.
    ///
    /// **Inferred**: a unit held since the new game's save comes first, in
    /// handle order. Unknown while a piece the world cannot place might
    /// be inside.
    pub(super) fn contents(&self, vm: &mut Vm<'_>, args: &[Handle]) -> Result<Answer, Fault> {
        let ret = vm.arg(args, 0)?;
        list_of(vm.object(ret))?;
        let id = agent_ref(vm, args, 1)?;
        let handle = match self.found(id) {
            Found::Piece(p) => Some(p.handle),
            Found::Gone => None,
            Found::Unsure => return Ok(Answer::Unknown),
        };

        let Some(handle) = handle else {
            return Ok(Answer::Given);
        };

        match self.occupants(handle) {
            Some(inside) => {
                if let Value::List(out) = vm.object_mut(ret) {
                    out.extend(inside.into_iter().map(Value::Agent));
                }

                Ok(Answer::Given)
            }
            None => Ok(Answer::Unknown),
        }
    }

    /// The agents inside the building of `handle`, in relation 2's order;
    /// `None` while a piece the world cannot place might be inside.
    pub fn occupants(&self, handle: u32) -> Option<Vec<u32>> {
        let mut inside: Vec<(Option<usize>, u32, u32)> = self
            .pieces
            .iter()
            .filter(|(i, _)| self.known(**i).is_some_and(|pl| inside_of(pl, handle)))
            .map(|(i, p)| (self.units.entry_order(*i), p.handle, *i))
            .collect();

        inside.sort_unstable();

        (!self.unplaced()).then(|| inside.into_iter().map(|(_, _, i)| i).collect())
    }

    /// `DeleteAgent(agent)`, gpl delete agent: the unit's `AE:0` deleted, its
    /// `UnitDelete` dispatched at once, then `AE:0` set to 0; the piece
    /// stands on with no agent ([`World::orphan`]). Unknown with no
    /// unit, where the engine reads through a null pointer.
    pub(super) fn delete_agent(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> Result<Answer, Fault> {
        let id = agent_ref(vm, args, 1)?;

        match self.found(id) {
            Found::Piece(_) => {
                self.orphan(id);
                self.dispatches.queue((
                    super::UNIT_DELETE.to_owned(),
                    vec![Value::Integer(id.cast_signed())],
                ));
                Ok(Answer::Given)
            }
            Found::Gone | Found::Unsure => Ok(Answer::Unknown),
        }
    }

    /// `DeclareLoss(player | agent)`, gpl declare loss: an integer is the
    /// player; an agent its unit's owner, or a throw with
    /// no unit. Any other type leaves the player unset: unknown.
    pub(super) fn declare_loss(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> Result<Answer, Fault> {
        let player = match vm.object(vm.arg(args, 1)?) {
            Value::Integer(n) => Some(n.cast_unsigned()),
            Value::Agent(id) => self.owner_or(*id, "$DeclareLoss(): no unit found.")?,
            _ => None,
        };

        match player {
            Some(p) => {
                self.end(p, End::Lost, NO_POINT);
                Ok(Answer::Given)
            }
            None => Ok(Answer::Unknown),
        }
    }

    /// `DeclareVictory(player | agent, [coord | agent])`, gpl declare victory:
    /// an integer is the player, an agent its unit's owner; then the
    /// point: a coord, an agent's unit's unit pos32, or with no
    /// second argument the player's palace's ([`World::palace_unit`]).
    /// Unknown where the engine reads a null unit or an unset point: an
    /// agent with no unit.
    pub(super) fn declare_victory(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> Result<Answer, Fault> {
        let player = match vm.object(vm.arg(args, 1)?) {
            Value::Integer(n) => Some(n.cast_unsigned()),
            Value::Agent(id) => self.unit(*id).map(|p| p.owner),
            _ => Err(Fault::throw(
                "$DeclareVictory(): 1st parameter must be an agent or a player number.",
            ))?,
        };
        let at = match player {
            Some(p) if args.len() <= PLAYER_ONLY => match self.palace_unit(p) {
                Some(palace) => self.position(palace),
                None => Err(Fault::throw(
                    "$DeclareVictory(): no palace found for default victory location.",
                ))?,
            },
            Some(_) => match vm.object(vm.arg(args, 2)?) {
                Value::Coord(x, y) => Some((*x, *y)),
                Value::Agent(id) => self.unit(*id).and_then(|_| self.position(*id)),
                _ => Err(Fault::throw(
                    "$DeclareVictory(): 2nd parameter must be an agent or a coordinate.",
                ))?,
            },
            None => None,
        };

        match (player, at) {
            (Some(p), Some(at)) => {
                self.end(p, End::Won, at);
                Ok(Answer::Given)
            }
            _ => Ok(Answer::Unknown),
        }
    }

    /// `DeleteGamePiece(agent)`, gpl delete game piece: the deletion is deferred,
    /// so the piece stands until the engine's `UnitDelete` ends it
    /// ([`World::ends`]). **Game**: play5's probe prints a deleted flag in
    /// the frame of its `DeleteGamePiece`.
    pub(super) fn delete_piece(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let id = agent_ref(vm, args, 1)?;

        match self.found(id) {
            Found::Piece(_) => {
                self.detach(id, None);

                self.dispatches.delete(id);

                Ok(Answer::Given)
            }
            Found::Gone => Err(Fault::throw(format!(
                "DeleteGamePiece() could not delete agent#{}: no associated unit",
                id.cast_signed()
            ))),
            Found::Unsure => Ok(Answer::Unknown),
        }
    }
}

/// What the end's statistics (`EndSummaryController`,
/// end stat row) read off the world as they show. The rest of its rows
/// are counters at player statistics, kept as the game runs, which the world
/// does not keep but vice: gold made and spent by kind, buildings made and
/// lost, heroes hired and killed, danger.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Summary {
    /// "Gold Balance", `GMTX` 89: the treasury, `APP:0`.
    pub gold: Option<i32>,
    /// "Standing", `GMTX` 100.
    pub standing: usize,
    /// "Alive", `GMTX` 104.
    pub heroes: usize,
    /// "Average Level", `GMTX` 105: their levels' mean, rounded up.
    pub level: i32,
    /// "Vice", `GMTX` 106: stats ([`World::vice`]).
    pub vice: i32,
}

impl Summary {
    /// Each row as `GMTX` text and its number: 89 "Gold Balance:", 100
    /// "Standing:", 104 "Alive:", 105 "Average Level:", 106 "Vice:", each
    /// `%d`.
    #[must_use]
    pub fn rows(&self) -> [(usize, i32); 5] {
        let count = |n: usize| i32::try_from(n).unwrap_or(i32::MAX);

        [
            (89, self.gold.unwrap_or(0)),
            (100, count(self.standing)),
            (104, count(self.heroes)),
            (105, self.level),
            (106, self.vice),
        ]
    }
}

impl World {
    /// Player `player`'s [`Summary`]: its buildings of unit list 0 standing
    /// (`GenUnit` under 3) with their first stage built; its heroes
    /// of unit list 1 standing and not among the
    /// start's, and their levels.
    #[must_use]
    pub fn summary(&self, player: u32) -> Summary {
        let word = |p: &Piece, k| p.word(k);
        let up = |p: &Piece, kind: kind::Kind| {
            p.owner == player
                && p.place
                    .is_some_and(|at| at.kind == kind && at.state & piece_state::RUINED == 0)
        };
        let standing = self
            .pieces
            .values()
            .filter(|p| up(p, kind::Kind::BUILDING) && word(p, FIRST_STAGE_BUILT) != 0)
            .count();
        let levels: Vec<i32> = self
            .pieces
            .iter()
            .filter(|(id, p)| up(p, kind::Kind::VEHICLE) && self.is_hero(**id))
            .filter(|(_, p)| word(p, STARTED_WITH_THIS_UNIT) == 0)
            .map(|(_, p)| word(p, EXPERIENCE_LEVEL))
            .collect();
        let heroes = levels.len();
        let level = i32::try_from(heroes)
            .ok()
            .filter(|n| *n > 0)
            .map_or(0, |n| mean_up(levels.iter().sum(), n));

        Summary {
            gold: self.treasury(player),
            standing,
            heroes,
            level,
            vice: self.vice(player),
        }
    }
}

/// `sum / n`, one up for a remainder: 7 over 2 is 4.
const fn mean_up(sum: i32, n: i32) -> i32 {
    sum / n + if sum % n == 0 { 0 } else { 1 }
}

/// Whether `DeleteAgent` wrote the piece's `AE:0` 0 ([`World::orphan`]).
pub(super) fn unbound(piece: &Piece) -> bool {
    piece.get(super::AGENT_ID) == Some(0)
}

/// Whether `place` is inside the building of `handle`: held there, not a
/// flag hung on it (relation 1).
fn inside_of(place: &super::Place, handle: u32) -> bool {
    place.holder == handle && place.state & piece_state::HELD != 0 && !place.is_reward()
}

/// The return slot replaced by a fresh agent, `gpl_value_of_type(5)`
/// ([`crate::gpl::builtins::Builtin::retype`]), holding `id`.
fn retype_agent(vm: &mut Vm<'_>, args: &[Handle], id: u32) -> Result<(), Fault> {
    let slot = *args.first().ok_or(Crash::ArgPastEnd)?;

    if let Some(old) = vm.get(slot) {
        vm.release(old);
    }

    let fresh = vm.of_type(Kind::Agent.type_code());

    vm.set(slot, fresh);

    let ret = fresh.ok_or(Crash::NullValue(Null::Result))?;

    ops::assign(vm.object_mut(ret), &Value::Agent(id)).map_err(Fault::from)
}

#[cfg(test)]
mod tests {
    use super::super::{Place, places::Motion};
    use super::*;
    use crate::base::fixed::{Fixed, from_whole};

    /// A piece of `code` and `owner`, at handle `handle`, held by `holder`.
    fn piece(code: [u8; 4], owner: u32, handle: u32, kind: kind::Kind, holder: u32) -> Piece {
        let held = if holder == 0 { 0 } else { piece_state::HELD };

        Piece {
            owner,
            handle,
            code: FourCc(code),
            place: Some(Place {
                kind,
                fixed: (Fixed::ZERO, Fixed::ZERO),
                standing: (0, 0),
                holder,
                state: held,
            }),
            ..Piece::default()
        }
    }

    /// A unit held since the save comes first; then each in the order it
    /// entered; a flag hung on the building is not inside.
    #[test]
    fn a_buildings_contents_are_in_entry_order() {
        let mut world = World::default();
        let mobile = |handle, holder| piece(*b"BVH1", 0, handle, kind::Kind::VEHICLE, holder);

        world
            .pieces
            .insert(1, piece(*b"ABJ1", 0, 100, kind::Kind::BUILDING, 0));
        world.pieces.insert(5, mobile(50, 100));
        world.pieces.insert(3, mobile(30, 0));
        world.pieces.insert(8, mobile(80, 0));
        world
            .pieces
            .insert(9, piece(*b"ARA2", 0, 90, kind::Kind::REWARD, 100));

        for id in [5, 3, 8, 9] {
            world.units.set_motion(id, Motion::Still);
        }

        world.enter(8, 1);
        world.enter(3, 1);

        assert_eq!(world.occupants(100), Some(vec![5, 8, 3]));
    }

    /// `GetPlayerData "palace"`: the owner's first `ABJ` by handle, not an
    /// outpost's `ABm`; -1 with none.
    #[test]
    fn a_players_palace_is_its_first_abj() {
        let mut world = World::default();

        world
            .pieces
            .insert(4, piece(*b"ABmA", 0, 10, kind::Kind::BUILDING, 0));
        world
            .pieces
            .insert(6, piece(*b"ABJ2", 0, 30, kind::Kind::BUILDING, 0));
        world
            .pieces
            .insert(2, piece(*b"ABJ1", 1, 20, kind::Kind::BUILDING, 0));

        assert_eq!(world.palace_of(0), 6);
        assert_eq!(world.palace_of(1), 2);
        assert_eq!(world.palace_of(3), NO_PALACE);
    }

    /// The world as a free game's host: what it answers, else nothing.
    struct Free<'w>(&'w mut World);

    impl crate::gpl::vm::Host for Free<'_> {
        fn builtin(
            &mut self,
            vm: &mut Vm<'_>,
            call: &crate::gpl::vm::Call<'_>,
        ) -> Result<(), Fault> {
            self.0
                .run(
                    vm,
                    call,
                    &|_, _, _| None,
                    crate::sim::world::step::Draws::Kept,
                )
                .map(|_| ())
        }

        fn attribute(
            &mut self,
            _: &mut Vm<'_>,
            _: u32,
            name: &str,
        ) -> Result<crate::gpl::vm::ObjId, Fault> {
            Err(Fault::throw(name))
        }

        fn named(&mut self, _: &str) -> Value {
            Value::Integer(0)
        }

        fn log(&mut self, _: &str) {}
    }

    /// Victory calls, each a function of the script below.
    const SOURCE: &str = "function Won(agent a)\nbegin\n\t$DeclareVictory(a);\nend\n\
        function WonAt(integer p, coordinate c)\nbegin\n\t$DeclareVictory(p, c);\nend\n\
        function WonBy(integer p, agent a)\nbegin\n\t$DeclareVictory(p, a);\nend\n\
        function Lost(integer p)\nbegin\n\t$DeclareLoss(p);\nend\n\
        function Moved(agent a, integer p)\nbegin\n\t$SetPalace(a, p);\nend\n";

    /// A palace at cell (10, 20), an outpost at (30, 40) and a hero at
    /// (5, 6), the crown's; agents 6, 4 and 9.
    fn kingdom() -> World {
        let mut world = World::default();
        let at = |id, (x, y): (i32, i32), mut p: Piece, world: &mut World| {
            if let Some(place) = &mut p.place {
                place.fixed = (from_whole(x), from_whole(y));
            }

            world.pieces.insert(id, p);
            world.units.set_motion(id, Motion::Still);
            world.agents.insert(id, super::super::Agent::default());
        };

        at(
            6,
            (10, 20),
            piece(*b"ABJ1", 0, 30, kind::Kind::BUILDING, 0),
            &mut world,
        );
        at(
            4,
            (30, 40),
            piece(*b"ABmA", 0, 50, kind::Kind::BUILDING, 0),
            &mut world,
        );
        at(
            9,
            (5, 6),
            piece(*b"BVH1", 0, 60, kind::Kind::VEHICLE, 0),
            &mut world,
        );
        world.players = super::super::players::Players::new(vec![0]);
        world
    }

    /// `DeclareVictory`: the player's end record `Won`, at the palace's
    /// place in 32nds with no second argument, else at the coord or the
    /// agent's unit; `SetPalace` moves the palace; each end writes over
    /// the last.
    #[test]
    fn a_victory_is_recorded_with_its_point() {
        let bytes = crate::gpl::emit::compile_source(SOURCE, "test.gpl").expect("compiles");
        let program = crate::gpl::load::Program::load(&[bytes]).expect("loads");
        let mut vm = Vm::new(&program, crate::gpl::vm::Config::default());
        let mut world = kingdom();
        let def = |name| match program.entry(name) {
            Some(crate::gpl::load::Entry::Script(d)) => d,
            _ => panic!("{name}"),
        };
        let catcher = crate::gpl::runtime::Catcher::Thread;
        let mut run = |world: &mut World, name, args: &[Value]| {
            let ran = vm.run(&mut Free(world), def(name), args, catcher);

            assert!(ran.is_ok_and(|r| r.error.is_none()), "{name}");
        };
        let crown = Value::Integer(0);

        run(&mut world, "Won", &[Value::Agent(9)]);
        assert_eq!(world.ended(0), Some(End::Won));
        assert_eq!(world.end_point(0), Some((320, 640)));

        run(&mut world, "WonAt", &[crown.clone(), Value::Coord(5, 7)]);
        assert_eq!(world.end_point(0), Some((5, 7)));

        run(&mut world, "WonBy", &[crown.clone(), Value::Agent(9)]);
        assert_eq!(world.end_point(0), Some((160, 192)));

        run(&mut world, "Moved", &[Value::Agent(4), crown.clone()]);
        run(&mut world, "Won", &[Value::Agent(6)]);
        assert_eq!(world.end_point(0), Some((960, 1280)));

        run(&mut world, "Lost", &[crown]);
        assert_eq!(world.ended(0), Some(End::Lost));
        assert_eq!(world.end_point(0), Some(NO_POINT));
    }

    /// A surrender ends the game lost, with no point, as `DeclareLoss`.
    #[test]
    fn a_surrender_is_a_loss_with_no_point() {
        let mut world = kingdom();

        world.surrender(0);
        assert_eq!(world.ended(0), Some(End::Lost));
        assert_eq!(world.end_point(0), Some(NO_POINT));
    }
}
