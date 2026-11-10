//! A reward flag the player places: `CREATE_REWARD_FLAG_MESSAGE`.
//! **Binary** unless marked.
//!
//! ```text
//!  frame f  GSMessageManager case 0x3ee, the reward flag decode:
//!    the flag is made (PlaceRewardFlag's too): an attack flag on a
//!    unit that has one is refused; GS_Level makes a unit of kind 4;
//!    APR:2 TargetID the target's AE:0, APR:1 RewardCost through
//!    GSUnit's write, APR:0 FlagSeekers 0; place search from the
//!    point, flags 0, then the unit is moved there; relation 1 to a target;
//!    GenUnit: NewUnitInit
//!  a raise   Reward2Controller: AlterAttributeDeltaMessage, APR:1
//!    plus #RewardDelta, sent while the treasury holds that much, at any
//!    reward, 0 too (world::Alter); no lower button
//!  removal   Reward2Controller control 5000, or a double click:
//!    the own flag twice ([`Removal`]), then DESTROY_REWARD_FLAG_MESSAGE
//!    by handle, decoded: GSUnit deletes it at once, UnitDelete; no gold
//!    back ([`World::take_down`])
//!
//!  GSUnit's write on APR:1: the change d = new - old
//!    treasury at most d: d = treasury / #RewardDelta x #RewardDelta
//!    d and old + d not 0: GE_Player takes d from APP:0, store
//! ```
//!
//! So the treasury pays a reward as it is set and raised; lowering it
//! gives back. The reward the message carries is the reward panel's
//! setting (reward panel attack attack, reward panel explore explore, `Reward1Controller`,
//! saved in `GSGL`), changed with no message: an input, as the point is.
//!
//! The point is the click; **Game**: play3's two explore flags stand where
//! `explore_flag_poll`'s `LocationOf` prints them.

use super::alter::Alter;
use super::effectors::NEARER;
use super::interface::Stride;
use super::offer::is_reward_flag;
use super::stack::stack_offset;
use super::{AGENT_ID, Answer, Found, NEW_UNIT, Piece, Place, World, agent_ref, int_ref};
use crate::base::fixed::Fixed;
use crate::data::attrib::Key;
use crate::data::fourcc::FourCc;
use crate::gpl::load::Program;
use crate::gpl::ops::{self, Value};
use crate::gpl::vm::{Fault, Handle, Vm};
use crate::sim::space;
use crate::sim::world::kind;
use crate::sim::world::piece_state;

/// `CREATE_REWARD_FLAG_MESSAGE`'s content: the flag's descriptor, its
/// point, 32nds of a cell, and owner; the reward (message `0x18`) and the
/// target's agent id, 0 for none (message `0x08`, a handle).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Flag {
    pub code: FourCc,
    pub at: (i32, i32),
    pub owner: u32,
    pub reward: i32,
    pub target: u32,
}

/// `APR:1`, `RewardCost`: written through `GSUnit`, it moves gold.
pub const REWARD_COST: Key = Key::new(*b"APR", 1);

/// `APR:2`, `TargetID`.
pub const TARGET_ID: Key = Key::new(*b"APR", 2);

/// `APR:0`, `FlagSeekers`: 0 on a new flag.
pub const SEEKERS: Key = Key::new(*b"APR", 0);

/// The message flag's descriptor, `message_flag`: message flag make
/// pushes `ARE1` with unit kind `MSGF`.
pub const MESSAGE_FLAG: FourCc = FourCc::new(b"ARE1");

/// The reward panel's two settings: reward panel attack,
/// reward panel explore.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rewards {
    pub attack: i32,
    pub explore: i32,
}

impl Rewards {
    /// Both settings as reward panel clamp keeps them against `treasury`,
    /// `initial` being `#RewardInitial`: run as the reward panel opens,
    /// on each press and as the
    /// treasury moves while it is open (method, on `APP:0`).
    ///
    /// ```text
    ///  setting below 0                       #RewardDelta
    ///  treasury under initial or setting     treasury rounded down to
    ///                                        #RewardDelta
    /// ```
    ///
    /// So a short treasury sets 0; under `initial`, every setting
    /// follows the treasury, a lower one too.
    #[must_use]
    pub const fn clamped(self, treasury: i32, initial: i32) -> Rewards {
        Rewards {
            attack: clamp_setting(self.attack, treasury, initial),
            explore: clamp_setting(self.explore, treasury, initial),
        }
    }

    /// Whether a setting's lower button is enabled: it holds at least
    /// `#RewardDelta`, so none goes below 0.
    #[must_use]
    pub const fn lowers(setting: i32) -> bool {
        REWARD_DELTA <= setting
    }

    /// Whether a setting's raise button is enabled: the treasury holds the
    /// raised setting.
    #[must_use]
    pub const fn raises(setting: i32, treasury: i32) -> bool {
        setting + REWARD_DELTA <= treasury
    }

    /// What a Shift press moves at most: the tooltips' 500.
    #[must_use]
    pub fn far() -> i32 {
        Stride::Far.amount(REWARD_DELTA, REWARD_STRIDE)
    }

    /// `setting` after its lower button's press: each step while
    /// [`Rewards::lowers`] holds of the running value. E.g. 300, far: 0.
    #[must_use]
    pub fn lowered(setting: i32, stride: Stride) -> i32 {
        let steps = reward_steps(stride, |gone| Rewards::lowers(setting - gone));

        setting - steps.map(|_| REWARD_DELTA).sum::<i32>()
    }
}

/// The gold moved before each step a press takes: 0, 100, ... while
/// `fits` holds, each step asked of the running value, not the start's.
pub(super) fn reward_steps(
    stride: Stride,
    fits: impl Fn(i32) -> bool,
) -> impl Iterator<Item = i32> {
    (0..)
        .map(|step| step * REWARD_DELTA)
        .take(stride.steps(REWARD_STRIDE))
        .take_while(move |gone| fits(*gone))
}

/// One setting under [`Rewards::clamped`].
const fn clamp_setting(setting: i32, treasury: i32, initial: i32) -> i32 {
    let setting = if setting < 0 { REWARD_DELTA } else { setting };

    if treasury < initial || treasury < setting {
        treasury - treasury % REWARD_DELTA
    } else {
        setting
    }
}

/// What a left double-click in the select mode sends (select click,
/// select click, on Windows' `WM_LBUTTONDBLCLK`).
///
/// ```text
///  no piece picked       Flag_Explore at the ground's point, the panel's
///                        explore reward; off the ground nothing
///  own ruin or dead      DestroyVehicleMessage
///  a message flag        DestroyEventFlagMessage ([`World::reads`])
///  own reward flag       its RewardCost 0: DestroyRewardFlagMessage,
///                        else the AP62 yes-or-no panel
///  else flaggable        Flag_Attack on it, the panel's attack reward
/// ```
///
/// A flag's message word `0x1c` is 1: its decode selects it for its
/// owner, then the select mode.
/// The panel's own clicks send 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Twice {
    Flag(Flag),
    /// `DESTROY_VEHICLE_MESSAGE` on it: [`crate::sim::game::Input::Clear`] by its handle.
    Clear(u32),
    /// `DESTROY_EVENT_FLAG_MESSAGE` on the message flag of this handle
    /// ([`World::reads`], [`World::read_message`]).
    Read(u32),
    /// The owner's reward flag: taken down, or asked first
    /// ([`World::removal`]).
    Unflag(u32),
}

/// How the flag panel's remove button (control 5000, "Remove this
/// bounty", `Reward2Controller`) or a double click takes down an own
/// flag: own flag twice.
///
/// ```text
///  RewardCost not 0   panel AP62 (KillConfirmController), GMTX 20 + 23:
///                     "Are you sure you want to remove this flag?"
///                     YES 5501: DestroyRewardFlagMessage; NO 5001: none
///  RewardCost 0       DestroyRewardFlagMessage at once
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Removal {
    /// The yes-or-no question first.
    Ask,
    /// The message at once.
    Now,
}

/// `#RewardDelta` in `globals.gpl`: reward constants reads it into
/// reward delta at a new game. A raise adds it; a reward the treasury
/// cannot pay rounds down to it.
pub const REWARD_DELTA: i32 = 100;

/// The `#RewardDelta` steps a Shift press of a reward's button takes: 500
/// gold. A departure from the original, whose press takes one.
pub const REWARD_STRIDE: usize = 5;

impl World {
    /// What `owner`'s double-click sends ([`Twice`]): `target` the piece
    /// the select mode picks, `at` the ground's point, 32nds, `None` off
    /// it. `None` where nothing is sent: no sound, no chat line.
    #[must_use]
    pub fn twice(
        &self,
        owner: u32,
        target: Option<u32>,
        at: Option<(i32, i32)>,
        rewards: Rewards,
    ) -> Option<Twice> {
        match target {
            // No treasury test: the decode pays what it can.
            None => at.map(|at| {
                Twice::Flag(Flag {
                    code: super::offer::EXPLORE_FLAG,
                    at: self.explore_point(at),
                    owner,
                    reward: rewards.explore,
                    target: 0,
                })
            }),
            Some(target) => self.twice_on(owner, target, rewards.attack),
        }
    }

    /// [`World::twice`] on piece `target`, `reward` an attack flag's.
    fn twice_on(&self, owner: u32, target: u32, reward: i32) -> Option<Twice> {
        let piece = self.pieces.get(target)?;
        let code = piece.code;
        let own = piece.owner == owner;
        // `GenUnit` 3: a ruin, or lying dead.
        let fallen = piece
            .place
            .is_some_and(|p| p.state & piece_state::RUINED != 0);
        let reward_flag = is_reward_flag(code);

        if own && fallen {
            Some(Twice::Clear(target))
        } else if own && reward_flag {
            Some(Twice::Unflag(target))
        } else if self.flaggable(owner, target, reward) {
            self.position(target).map(|at| {
                Twice::Flag(Flag {
                    code: super::offer::ATTACK_FLAG,
                    at,
                    owner,
                    reward,
                    target,
                })
            })
        } else {
            None
        }
    }

    /// What `owner`'s Space sends with `selected` the selection
    /// (map keys case `0x20`): [`Twice::Flag`] or
    /// [`Twice::Unflag`], `reward` the panel's attack setting.
    ///
    /// ```text
    ///  another owner's piece selected   it
    ///  else a reward flag selected      the piece it hangs on
    ///  its first attack flag owner's    taken down (the own flag twice)
    ///  else standing, the treasury      Flag_Attack on it
    ///  holds reward, flaggable
    /// ```
    ///
    /// Own pieces but flags send nothing. **Inferred**: the first flag
    /// attached is the lowest agent, as attachment follows making.
    #[must_use]
    pub fn space(&self, owner: u32, selected: Option<u32>, reward: i32) -> Option<Twice> {
        let piece = selected.and_then(|id| Some((id, self.pieces.get(id)?)));
        let target = match piece {
            Some((id, p)) if p.owner != owner => Some(id),
            Some((_, p)) => p
                .place
                .filter(|pl| pl.is_reward() && pl.holder != 0)
                .and_then(|pl| self.holder(pl.holder)),
            None => None,
        }?;
        let handle = self.pieces.get(target)?.handle;
        let first = self
            .pieces
            .iter()
            .filter(|(_, p)| {
                p.code == super::offer::ATTACK_FLAG && p.place.is_some_and(|pl| pl.holder == handle)
            })
            .min_by_key(|(id, _)| **id);

        match first {
            Some((flag, p)) if p.owner == owner => Some(Twice::Unflag(*flag)),
            _ if self.flaggable(owner, target, reward) => self.position(target).map(|at| {
                Twice::Flag(Flag {
                    code: super::offer::ATTACK_FLAG,
                    at,
                    owner,
                    reward,
                    target,
                })
            }),
            _ => None,
        }
    }

    /// Reward flag `id`'s `RewardCost`, 0 where unset: a flag placed at 0
    /// stores none. `None` for any other piece.
    #[must_use]
    pub fn reward(&self, id: u32) -> Option<i32> {
        let piece = self
            .pieces
            .get(id)
            .filter(|p| p.place.is_some_and(|pl| pl.is_reward()));

        piece.map(|p| p.get(REWARD_COST).unwrap_or_default())
    }

    /// How reward flag `id` is taken down ([`Removal`]); `None` for any
    /// other piece.
    #[must_use]
    pub fn removal(&self, id: u32) -> Option<Removal> {
        self.reward(id)
            .map(|r| if r == 0 { Removal::Now } else { Removal::Ask })
    }

    /// `DESTROY_REWARD_FLAG_MESSAGE` decoded: the unit of
    /// `handle`, unless queued for deletion ( bit 2,
    /// `DeleteGamePiece`'s), deleted at once by `GSUnit`. Its agent,
    /// whose `UnitDelete` the caller dispatches at once, and the agent it
    /// hangs on, whose flags then stack again ([`World::restack`]). No gold
    /// comes back: neither the decode nor `GSUnit` reads `APR:1`.
    /// **Game**: the wizard trace's explore flag, taken down at 1138 as a
    /// ranger heads for it: its `UnitDelete` between that frame's pass
    /// and the next, the ranger reset at once by `explore_flag_death`,
    /// the treasury at the save the save's.
    /// **Inferred**: a unit with no agent is not taken down here.
    #[must_use]
    pub fn take_down(&self, handle: u32) -> Option<(u32, Option<u32>)> {
        let id = self
            .holder(handle)
            .filter(|id| self.agents.alive(*id) && !self.dispatches.deleting(*id))?;
        let on = self
            .pieces
            .place(id)
            .and_then(|pl| self.holder(pl.holder).filter(|_| pl.holder != 0));

        Some((id, on))
    }

    /// Whether `a` is the reward flag panel's raise
    /// (`Reward2Controller`, `orders::raise_reward`): on a reward flag,
    /// its `RewardCost` up by `#RewardDelta`, no range.
    #[must_use]
    pub fn raise_sent(&self, a: &Alter) -> bool {
        let flag = self
            .pieces
            .get(a.agent)
            .is_some_and(|p| is_reward_flag(p.code));

        flag && a.key == REWARD_COST && a.change == REWARD_DELTA && a.range == (0, 0)
    }

    /// An explore flag's point: `at`, 32nds, kept on the map
    /// (explore point clamp, Map `pixel_width`, `pixel_height` less 1).
    #[must_use]
    pub fn explore_point(&self, at: (i32, i32)) -> (i32, i32) {
        let (w, h) = self.board.extent();

        (at.0.clamp(0, w - 1), at.1.clamp(0, h - 1))
    }

    /// Reward flag make for `flag`: the next agent id and handle, at
    /// the place search's cell from its point. `None` where no cell is
    /// free. The caller dispatches `NewUnitInit`.
    ///
    /// # Errors
    ///
    /// The agent's `.dat` instance is missing.
    pub fn flag(&mut self, program: &Program, flag: &Flag) -> Result<Option<u32>, Fault> {
        let refused =
            flag.code == super::offer::ATTACK_FLAG && self.flagged_by(flag.owner, flag.target);
        let body = self.descriptors.body(flag.code).cloned();
        let spot = body.as_ref().and_then(|b| self.place(b, flag.at));

        match (refused, body, spot) {
            (false, Some(body), Some(at)) => self.plant(program, flag, &body, at).map(Some),
            _ => Ok(None),
        }
    }

    /// Whether an attack flag of `owner`'s hangs on agent `target`
    /// (attack flag held: the target's relation 1 members coded `ARA2`).
    fn flagged_by(&self, owner: u32, target: u32) -> bool {
        let holder = self.pieces.handle(target);

        holder.is_some_and(|h| {
            self.pieces.values().any(|p| {
                p.code == super::offer::ATTACK_FLAG
                    && p.owner == owner
                    && p.place.is_some_and(|pl| pl.holder == h)
            })
        })
    }

    /// `PlaceRewardFlag(unit, player, reward)`, gpl place reward flag:
    /// Reward flag make of an attack flag of `player`'s on the
    /// unit, from its place, `reward` paid as set;
    /// `NewUnitInit` at once. Its agent, `AE:0`; refused or nowhere to
    /// stand, the return slot as it was.
    pub(super) fn place_reward_flag(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let id = agent_ref(vm, args, 1)?;
        let at = match self.found(id) {
            Found::Piece(_) => self.position(id),
            Found::Unsure => None,
            Found::Gone => Err(Fault::throw("PlaceRewardFlag() unit is null"))?,
        };
        let owner = int_ref(vm, args, 2)?;
        let reward = int_ref(vm, args, 3)?;

        match at {
            Some(at) => {
                let flag = Flag {
                    code: super::offer::ATTACK_FLAG,
                    at,
                    owner: owner.cast_unsigned(),
                    reward,
                    target: id,
                };

                if let Some(made) = self.flag(vm.program(), &flag)? {
                    let ret = vm.arg(args, 0)?;

                    self.dispatch(NEW_UNIT, vec![Value::Agent(made)]);
                    ops::assign(vm.object_mut(ret), &Value::Agent(made))?;
                }

                Ok(Answer::Given)
            }
            None => Ok(Answer::Unknown),
        }
    }

    /// `GetFlag(unit)`, gpl get flag: the first reward flag, unit kind 4,
    /// of the unit's `HOME` list, as its `AE:0` agent;
    /// none, the return slot as it was. Unknown where a member before it
    /// is.
    pub(super) fn get_flag(
        &self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let id = agent_ref(vm, args, 1)?;
        // A piece the log announced, handle 0: its relations are not known.
        let handle = match self.found(id) {
            Found::Piece(p) if p.handle == 0 => None,
            Found::Piece(p) => Some(p.handle),
            Found::Unsure => None,
            Found::Gone => Err(Fault::throw("GetFlag() unit is null"))?,
        };
        let members = handle.map(|h| self.relations.home(h).to_vec());
        let first = members.and_then(|m| {
            m.iter()
                .filter(|h| self.holder(**h).is_some())
                .map(|h| self.is_flag(*h).map(|f| (*h, f)))
                .find(|sort| sort.is_none_or(|(_, f)| f))
                .map_or(Some(None), |sort| sort.map(|(h, _)| Some(h)))
        });
        let agent = |h: u32| {
            self.holder(h)
                .and_then(|a| self.any_piece(a))
                .and_then(|p| p.get(AGENT_ID))
                .unwrap_or_default()
        };

        match first {
            Some(Some(flag)) => {
                let ret = vm.arg(args, 0)?;

                ops::assign(
                    vm.object_mut(ret),
                    &Value::Agent(agent(flag).cast_unsigned()),
                )?;
                Ok(Answer::Given)
            }
            Some(None) => Ok(Answer::Given),
            None => Ok(Answer::Unknown),
        }
    }

    /// The flag's agent and piece at `at`, 32nds.
    fn plant(
        &mut self,
        program: &Program,
        flag: &Flag,
        body: &super::bodies::Body,
        at: (i32, i32),
    ) -> Result<u32, Fault> {
        let id = self.agents.next_id();
        let agent = super::Agent::made(program, &body.name)?;
        let fixed = (
            crate::sim::units::from_pos32(at.0),
            crate::sim::units::from_pos32(at.1),
        );

        self.agents.advance();
        self.handle += 1;
        self.agents.insert(id, agent);
        self.pieces.insert(
            id,
            Piece {
                owner: flag.owner,
                attributes: std::iter::once((super::AGENT_ID.packed(), id.cast_signed())).collect(),
                handle: self.handle,
                code: flag.code,
                name: body.name.clone(),
                place: Some(Place {
                    kind: kind::Kind::REWARD,
                    fixed,
                    standing: (space::half(fixed.0), space::half(fixed.1)),
                    holder: 0,
                    state: body.state,
                }),
                spells: Some(Vec::new()),
            },
        );
        self.agents.note_made(id);

        // Its sound as it is made, unplaced, the owner's.
        let sound = if flag.code == super::offer::ATTACK_FLAG {
            super::sound::PLACE_REWARD
        } else {
            super::sound::PLACE_EXPLORE
        };

        self.cue_owned(
            super::sound::From::Nowhere,
            sound,
            super::sound::BEGIN,
            flag.owner,
        );

        // The reward counted as paid for flags as the flag is made,
        // and again as `APR:1` takes it.
        self.count_paid(flag.owner, super::ledger::Spend::Flags, flag.reward);

        if let Some(target) = self.pieces.get(flag.target).map(|_| flag.target) {
            self.write(id, TARGET_ID.packed(), target.cast_signed());
            self.hang_on(id, target, body.attach);
        }

        self.write(id, REWARD_COST.packed(), flag.reward);
        self.write(id, SEEKERS.packed(), 0);
        Ok(id)
    }

    /// Flag `id` hung on `target` after the place search: relation 1
    /// makes the target its holder, not held: no state bit
    /// [`crate::sim::world::piece_state::HELD`], so drawn (**Game**: play5's save, flag
    /// #1007 on troll #569, state `DontBlock` alone). `GSUnit`
    /// sets its own position to the target's attachment point `attach`
    /// less the target's position, y plus 1 (`GenUnit`,
    /// `GenUnit`). Unit pos adds the holder's position to a held
    /// overlay's own, so the flag follows its target. With no `Hotspot`
    /// on the target the point is set 8's ([`World::hang_point`]).
    ///
    /// Then `GSUnit`'s stacking among the overlays already on the
    /// target ([`stack_offset`]): the other flags hung on it and its
    /// effectors, in handle order (**Inferred**: relation 1 appends).
    fn hang_on(&mut self, id: u32, target: u32, attach: u32) {
        let holder = self.pieces.handle(target);
        let point = self.hang_point(target, attach);
        let own = self.body_of(id).map_or(0, |b| b.stack);
        let (dx, dy) = holder.map_or((Fixed::ZERO, Fixed::ZERO), |h| {
            stack_offset(own, &self.stacked(h, target))
        });
        let spot = holder.zip(point);

        if let Some(p) = self.pieces.get_mut(id) {
            p.place = spot.and_then(|(handle, [x, y, _])| {
                p.place.map(|place| Place {
                    fixed: (x + dx, y + NEARER + dy),
                    holder: handle,
                    ..place
                })
            });
        }

        self.jumped(id);
    }

    /// Where flag `id` hangs about its holder, 16.16 cells: its own
    /// position, x and y, then its point's height. `None` unless hung.
    /// **Game**: play5's save, flag #1007 on troll #569, height 4.156.
    #[must_use]
    pub fn hung_offset(&self, id: u32) -> Option<[Fixed; 3]> {
        let place = self.pieces.get(id)?.place?;
        let target = self.holder(place.holder)?;
        let attach = self.body_of(id).map_or(0, |b| b.attach);
        let z = self
            .hang_point(target, attach)
            .map_or(Fixed::ZERO, |p| p[2]);

        place
            .is_reward()
            .then_some([place.fixed.0, place.fixed.1, z])
    }

    /// `GSUnit` on [`REWARD_COST`] of flag `id`: the owner pays the
    /// change, what the treasury holds rounded down to [`REWARD_DELTA`]
    /// when short; no change, or a reward of 0, stores nothing.
    pub(super) fn reward_write(&mut self, id: u32, value: i32) {
        let at = REWARD_COST.packed();
        let piece = self
            .pieces
            .get(id)
            .map(|p| (p.owner, p.attributes.get(&at).copied().unwrap_or_default()));

        if let Some((owner, old)) = piece {
            // 32-bit, as `GSUnit`'s: it wraps.
            let wanted = value.wrapping_sub(old);
            let change = match self.treasury(owner) {
                Some(g) if g <= wanted => g / REWARD_DELTA * REWARD_DELTA,
                _ => wanted,
            };

            if change != 0 && old.wrapping_add(change) != 0 && self.spend(owner, change) {
                // `GSUnit`: the change counted as paid for flags.
                self.count_paid(owner, super::ledger::Spend::Flags, change);

                if let Some(p) = self.pieces.get_mut(id) {
                    p.attributes.insert(at, old.wrapping_add(change));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::CROWN;
    use super::super::offer::EXPLORE_FLAG;
    use super::super::ruin::tests::standing;
    use super::*;
    use crate::sim::game::Input;
    use crate::sim::game::orders::raise_reward;

    /// The flag's agent id in a test's world.
    const FLAG: u32 = 1;

    /// The reward panel's settings against the treasury, an initial 500
    /// (the shipped `#RewardInitial` is 0): a negative setting becomes
    /// `#RewardDelta`; over the treasury, or with the treasury under 500,
    /// the treasury rounded down.
    #[test]
    fn the_reward_settings_follow_a_short_treasury() {
        let set = |attack, explore| Rewards { attack, explore };

        assert_eq!(set(300, -100).clamped(2000, 500), set(300, 100));
        assert_eq!(set(900, 0).clamped(850, 500), set(800, 0));
        assert_eq!(set(100, 300).clamped(450, 500), set(400, 400));
        assert_eq!(set(200, 200).clamped(50, 500), set(0, 0));
        assert!(!Rewards::lowers(0) && Rewards::lowers(REWARD_DELTA));
        assert!(Rewards::raises(0, REWARD_DELTA) && !Rewards::raises(1, REWARD_DELTA));
    }

    #[test]
    fn a_far_press_moves_500() {
        assert_eq!(Rewards::far(), 500);
    }

    /// A far lower takes as many steps as fit; a near one, one.
    #[test]
    fn a_far_lower_stops_at_the_last_whole_step() {
        assert_eq!(Rewards::lowered(800, Stride::Far), 300);
        assert_eq!(Rewards::lowered(500, Stride::Far), 0);
        assert_eq!(Rewards::lowered(300, Stride::Far), 0);
        assert_eq!(Rewards::lowered(350, Stride::Far), 50);
        assert_eq!(Rewards::lowered(50, Stride::Far), 50);
        assert_eq!(Rewards::lowered(300, Stride::One), 200);
    }

    /// Against the treasury, the setting's raise goes as far as it holds.
    #[test]
    fn a_far_raise_stops_at_the_treasury() {
        let mut world = World::default();

        // No treasury: anything.
        assert_eq!(world.raised_reward(CROWN, 100, Stride::Far), 600);

        world.set_treasury(CROWN, 1000);
        assert_eq!(world.raised_reward(CROWN, 100, Stride::Far), 600);
        assert_eq!(world.raised_reward(CROWN, 100, Stride::One), 200);

        world.set_treasury(CROWN, 500);
        assert_eq!(world.raised_reward(CROWN, 200, Stride::Far), 500);
        assert_eq!(world.raised_reward(CROWN, 500, Stride::Far), 500);
    }

    /// A flag raised by `gold`'s worth of orders, all landing in one tick:
    /// each is the panel's single raise, and each pays from what the last
    /// left.
    fn raised_flag(gold: i32, stride: Stride) -> (usize, i32, Option<i32>) {
        let mut world = World::default();
        let mut piece = standing(EXPLORE_FLAG.0, 1, kind::Kind::REWARD, (0, 0));

        piece.owner = CROWN;
        world.add_test_piece(FLAG, piece);
        world.set_treasury(CROWN, gold);

        let raises = world.flag_raises(CROWN, FLAG, stride);

        for _ in 0..raises {
            let Input::Alter(alter) = raise_reward(FLAG) else {
                panic!("a raise is an alter");
            };

            assert!(world.raise_sent(&alter));
            world.alter(&alter);
        }

        (
            raises,
            world.treasury(CROWN).expect("a treasury"),
            world.reward(FLAG),
        )
    }

    #[test]
    fn a_far_flag_raise_in_one_tick_pays_each_step() {
        assert_eq!(raised_flag(1000, Stride::Far), (5, 500, Some(500)));
        assert_eq!(raised_flag(500, Stride::Far), (5, 0, Some(500)));
        assert_eq!(raised_flag(350, Stride::Far), (3, 50, Some(300)));
        assert_eq!(raised_flag(99, Stride::Far), (0, 99, Some(0)));
        assert_eq!(raised_flag(1000, Stride::One), (1, 900, Some(100)));
    }

    /// Not an own flag: no raise, however far.
    #[test]
    fn another_monarchs_flag_takes_no_far_raise() {
        let mut world = World::default();
        let mut piece = standing(EXPLORE_FLAG.0, 1, kind::Kind::REWARD, (0, 0));

        piece.owner = CROWN + 1;
        world.add_test_piece(FLAG, piece);

        assert_eq!(world.flag_raises(CROWN, FLAG, Stride::Far), 0);
    }

    /// `GSUnit`'s change is a 32-bit `new - old`: it wraps.
    #[test]
    fn a_reward_change_wraps() {
        let mut world = World::default();
        let mut piece = super::super::Piece {
            owner: CROWN,
            ..super::super::Piece::default()
        };

        piece.attributes.insert(REWARD_COST.packed(), -1);
        world.add_test_piece(1, piece);
        world.set_treasury(CROWN, 1000);
        world.reward_write(1, i32::MAX);

        assert_eq!(world.treasury(CROWN), Some(1000_i32.wrapping_sub(i32::MIN)));
    }
}
