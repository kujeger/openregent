//! Owners and teams as builtins read them, shared sight, a statistic,
//! two unit writes with no walk (an intent, a combat flash), and an item
//! test. **Binary** unless marked.
//!
//! ```text
//!  SetShareReveal(p, b)  APP:1 = b on player p, then team share reveal:
//!                        with APP:1 set, every vertex another of players
//!                        0-3 on p's team explored becomes p's too
//!  GS_Player             a player's reveal mask: its own bit, and each
//!                        other player under the session's count on its
//!                        team with APP:1 set (Players::reveal_mask)
//! ```

use std::collections::BTreeMap;

use super::{
    Answer, Found, NEW_TEAMS, TEAMS, World, agent_ref, bool_ref,
    defeat::End,
    graveyard::Grave,
    guild::bounty::Hall,
    int_ref,
    ledger::Ledger,
    numbered::{Tallies, Tally},
    put_bool, put_int,
    score::Champion,
    string_ref,
};
use crate::data::attrib::Key;
use crate::data::fourcc::FourCc;
use crate::gpl::ops::Value;
use crate::gpl::vm::{Fault, Handle, Vm};

/// `APV:30`, `AIIntentionString`: what `SpecifyIntent` writes
/// (`0x1e565041` in gpl specify intent); `STRT AITX` names each value.
pub const INTENT: i32 = Key::new(*b"APV", 30).packed();

/// `NeutralTeamNumber`'s answer: gpl neutral team number writes 0.
pub const NEUTRAL_TEAM: i32 = 0;

/// An inventory slot's bank: `INV:n` holds item `n` (`0x564e49` combined in, in
/// gpl agent has inventory item).
pub const INVENTORY: [u8; 3] = *b"INV";

/// The players team share reveal and `SetPlayerTeamNumber` walk:
/// 0 to 3 (a compare with 4, team share reveal, gpl set player team number).
pub const SHARING_PLAYERS: u32 = 4;

/// The session's player count `GS_Player` walks, session:
/// 1 as the session is made. **Inferred**: a single-player
/// game keeps it; the menus that write 2 and 4 were not followed. A game's
/// own count is [`World::humans`]: this unless its setup gave another.
pub const SESSION_PLAYERS: u32 = 1;

/// The one counter `IncrementStatCounter` knows (`_stricmp`): vice, the
/// player's stats, player.
pub const GUTTER: &str = "gutter";

/// Whether a player shares its team's ground: `APP:1`, `SetShareReveal`'s.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sharing {
    On,
    Off,
}

/// One player of the level, what the world keeps of `GE_Player` and
/// `GS_Player`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Player {
    /// `APP:0`, where known ([`super::treasury`]).
    gold: Option<i32>,
    /// `APP:1`.
    sharing: bool,
    /// `APP:2` to `APP:4`, where a bounty wrote them ([`super::guild::bounty`]).
    hall: Option<Hall>,
    /// Player palace handle: its palace's handle, where kept.
    palace: Option<u32>,
    /// Stats: `IncrementStatCounter`'s.
    vice: i32,
    /// Its other statistics ([`super::ledger`]).
    ledger: Ledger,
    /// Its statistics' best hero ([`super::score`]).
    champion: Champion,
    /// Player slain list: slain monsters by code word ([`super::slain`]).
    slain: BTreeMap<u32, i32>,
    /// What it bought and gained, by key, where counted
    /// ([`super::numbered`]).
    counts: Option<[BTreeMap<FourCc, i32>; 2]>,
    /// How its game ended, and where ([`super::defeat`]).
    end: Option<(End, (i32, i32))>,
    /// Player dead-hero list: dead heroes not yet in a graveyard
    /// ([`super::graveyard`]).
    dead: Vec<Grave>,
}

/// The level's players: each one's record, and the team masks.
///
/// ```text
///  Players ── slots   the level's, one PLYR each
///     ├─ session      the human seats, 0 up
///     ├─ records      a Player by slot: treasury, sharing, hall,
///     │               palace, vice, ledger, champion, slain, counts,
///     │               end
///     └─ teams        eight masks of eight player bits
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Players {
    slots: Vec<u32>,
    /// The session's player count: slots 0 up are human seats.
    session: u32,
    records: BTreeMap<u32, Player>,
    /// As team of player reads them.
    teams: [u32; TEAMS],
}

impl Default for Players {
    fn default() -> Players {
        Players {
            slots: Vec::new(),
            session: SESSION_PLAYERS,
            records: BTreeMap::new(),
            teams: [0; TEAMS],
        }
    }
}

impl Players {
    /// A new level's players: `slots`, each on [`NEW_TEAMS`]' team, one
    /// human seat ([`SESSION_PLAYERS`]).
    pub(super) fn new(slots: Vec<u32>) -> Players {
        Players {
            slots,
            teams: NEW_TEAMS,
            ..Players::default()
        }
    }

    /// The same players with `session` human seats.
    pub(super) const fn seated(mut self, session: u32) -> Players {
        self.session = session;
        self
    }

    /// The human seats: slots `0..session`.
    pub(super) const fn humans(&self) -> std::ops::Range<u32> {
        0..self.session
    }

    fn record(&self, player: u32) -> Option<&Player> {
        self.records.get(&player)
    }

    fn record_mut(&mut self, player: u32) -> &mut Player {
        self.records.entry(player).or_default()
    }

    /// The level's players, in `PLYR` order.
    pub(super) fn slots(&self) -> &[u32] {
        &self.slots
    }

    /// Whether the level has player `player`.
    pub(super) fn contains(&self, player: u32) -> bool {
        self.slots.contains(&player)
    }

    /// Team of player: the first team whose mask holds `player`,
    /// else 0.
    pub(super) fn team_of(&self, player: u32) -> usize {
        team_of(&self.teams, player)
    }

    /// Team set: take `player` off its team and onto `team`.
    pub(super) fn join(&mut self, player: u32, team: i32) {
        let bit = 1u32.wrapping_shl(player);
        let from = team_of(&self.teams, player);

        self.teams[from] &= !bit;

        if let Some(mask) = usize::try_from(team)
            .ok()
            .and_then(|t| self.teams.get_mut(t))
        {
            *mask |= bit;
        }
    }

    /// The other players of 0 to 3 in the level on `player`'s team, a
    /// bit each: whose explored ground [`World::share_team_ground`]
    /// gives it.
    fn mates(&self, player: u32) -> u8 {
        let team = self.team_of(player);

        (0..SHARING_PLAYERS)
            .filter(|p| *p != player && self.contains(*p))
            .filter(|p| self.team_of(*p) == team)
            .fold(0u8, |mask, p| mask | 1u8.wrapping_shl(p))
    }

    /// `GS_Player`: `owner`'s reveal mask, its own
    /// bit; slot 7 alone; else also each other player under
    /// the session's count ([`Players::humans`]) on its team whose `APP:1`
    /// is set.
    pub(super) fn reveal_mask(&self, owner: u32) -> u8 {
        let own = 1u8.wrapping_shl(owner);
        let team = self.team_of(owner);

        if owner == super::new_map::NEUTRAL {
            own
        } else {
            self.humans()
                .filter(|p| *p != owner && self.shares(*p))
                .filter(|p| self.team_of(*p) == team)
                .fold(own, |mask, p| mask | 1u8.wrapping_shl(p))
        }
    }

    /// The team masks.
    pub(super) const fn teams(&self) -> [u32; TEAMS] {
        self.teams
    }

    /// `player`'s treasury, where known.
    pub(super) fn gold(&self, player: u32) -> Option<i32> {
        self.record(player).and_then(|p| p.gold)
    }

    /// `player`'s treasury to change, where known.
    pub(super) fn gold_mut(&mut self, player: u32) -> Option<&mut i32> {
        self.records.get_mut(&player).and_then(|p| p.gold.as_mut())
    }

    /// `player`'s treasury, known from now on.
    pub(super) fn set_gold(&mut self, player: u32, gold: i32) {
        self.record_mut(player).gold = Some(gold);
    }

    /// Every known treasury, by player.
    pub(super) fn treasuries(&self) -> BTreeMap<u32, i32> {
        self.records
            .iter()
            .filter_map(|(slot, p)| p.gold.map(|g| (*slot, g)))
            .collect()
    }

    /// Whether `player`'s `APP:1` is set.
    pub(super) fn shares(&self, player: u32) -> bool {
        self.record(player).is_some_and(|p| p.sharing)
    }

    /// `player`'s `APP:1`.
    pub(super) fn set_sharing(&mut self, player: u32, sharing: Sharing) {
        self.record_mut(player).sharing = sharing == Sharing::On;
    }

    /// `player`'s hall words, where a bounty wrote them.
    pub(super) fn hall(&self, player: u32) -> Option<Hall> {
        self.record(player).and_then(|p| p.hall)
    }

    /// `player`'s hall words to change, where a bounty wrote them.
    pub(super) fn hall_mut(&mut self, player: u32) -> Option<&mut Hall> {
        self.records.get_mut(&player).and_then(|p| p.hall.as_mut())
    }

    /// `player`'s hall words written.
    pub(super) fn set_hall(&mut self, player: u32, hall: Hall) {
        self.record_mut(player).hall = Some(hall);
    }

    /// The handle `player` keeps as its palace.
    pub(super) fn palace(&self, player: u32) -> Option<u32> {
        self.record(player).and_then(|p| p.palace)
    }

    /// `player` keeps `handle` as its palace.
    pub(super) fn set_palace(&mut self, player: u32, handle: u32) {
        self.record_mut(player).palace = Some(handle);
    }

    /// `player`'s vice.
    pub(super) fn vice(&self, player: u32) -> i32 {
        self.record(player).map_or(0, |p| p.vice)
    }

    /// One more gutter visit for `player`.
    pub(super) fn add_vice(&mut self, player: u32) {
        self.record_mut(player).vice += 1;
    }

    /// `player`'s statistics.
    pub(super) fn ledger(&self, player: u32) -> Ledger {
        self.record(player).map(|p| p.ledger).unwrap_or_default()
    }

    /// `player`'s statistics, to add to.
    pub(super) fn ledger_mut(&mut self, player: u32) -> &mut Ledger {
        &mut self.record_mut(player).ledger
    }

    /// `player`'s kept champion.
    pub(super) fn champion(&self, player: u32) -> Option<&Champion> {
        self.record(player).map(|p| &p.champion)
    }

    /// `player`'s dead heroes not yet in a graveyard.
    pub(super) fn dead(&self, player: u32) -> &[Grave] {
        self.record(player).map_or(&[], |p| p.dead.as_slice())
    }

    /// A hero of `player`'s died: on its dead list.
    pub(super) fn died(&mut self, player: u32, grave: Grave) {
        self.record_mut(player).dead.push(grave);
    }

    /// `player`'s dead list taken for a graveyard, emptied.
    pub(super) fn bury(&mut self, player: u32) -> Vec<Grave> {
        std::mem::take(&mut self.record_mut(player).dead)
    }

    /// `player`'s slain monsters by code word.
    pub(super) fn slain(&self, player: u32) -> Option<&BTreeMap<u32, i32>> {
        self.record(player).map(|p| &p.slain)
    }

    /// One more monster of code word `code` slain for `player`.
    pub(super) fn slay(&mut self, player: u32, code: u32) {
        *self.record_mut(player).slain.entry(code).or_default() += 1;
    }

    /// One more of `key` for `player`.
    pub(super) fn tally(&mut self, player: u32, key: FourCc, how: Tally) {
        let counts = self.record_mut(player).counts.get_or_insert_default();

        *counts[how as usize].entry(key).or_default() += 1;
    }

    /// `player`'s bought and gained of `key`: `BuildingRec`'s number.
    pub(super) fn total(&self, player: u32, key: FourCc) -> i32 {
        self.record(player)
            .and_then(|p| p.counts.as_ref())
            .map_or(0, |c| c.iter().filter_map(|m| m.get(&key)).sum())
    }

    /// Every player's counts, where counted.
    pub(super) fn tallies(&self) -> Tallies {
        Tallies(
            self.records
                .iter()
                .filter_map(|(slot, p)| p.counts.clone().map(|c| (*slot, c)))
                .collect(),
        )
    }

    /// How `player`'s game ended, and where.
    pub(super) fn end(&self, player: u32) -> Option<(End, (i32, i32))> {
        self.record(player).and_then(|p| p.end)
    }

    /// `player`'s game ends `how`, at `at`: written over the last.
    pub(super) fn set_end(&mut self, player: u32, how: End, at: (i32, i32)) {
        self.record_mut(player).end = Some((how, at));
    }

    /// `player`'s game runs again.
    pub(super) fn clear_end(&mut self, player: u32) {
        if let Some(p) = self.records.get_mut(&player) {
            p.end = None;
        }
    }
}

/// Team of player: the first team whose mask holds the player, else 0.
fn team_of(teams: &[u32; TEAMS], player: u32) -> usize {
    let bit = 1u32.wrapping_shl(player);

    teams.iter().position(|mask| mask & bit != 0).unwrap_or(0)
}

impl World {
    /// The unit of agent `id`, as a builtin needing one finds it; `None`
    /// where the world does not know whether it stands.
    fn needed(&self, id: u32, missing: &str) -> std::result::Result<Option<u32>, Fault> {
        match self.found(id) {
            Found::Piece(p) => Ok(Some(p.owner)),
            Found::Unsure => Ok(None),
            Found::Gone => Err(Fault::throw(missing)),
        }
    }

    /// `GetPlayerTeamNumber(agent)`, gpl get player team number: throws with no unit;
    /// else its owner's team.
    pub(super) fn team_number(
        &self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let id = agent_ref(vm, args, 1)?;

        match self.needed(id, "GetPlayerTeamNumber() unit is null")? {
            Some(owner) => {
                let ret = vm.arg(args, 0)?;
                let team = self.players.team_of(owner);

                put_int(vm, ret, i32::try_from(team).unwrap_or(0)).map(|()| Answer::Given)
            }
            None => Ok(Answer::Unknown),
        }
    }

    /// `SetPlayerTeamNumber(agent, team)`, gpl set player team number: throws with no
    /// unit; else its owner onto the team, then each of
    /// players 0 to 3 shares its team's ground
    /// ([`World::share_team_ground`]).
    pub(super) fn set_team(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let id = agent_ref(vm, args, 1)?;
        let owner = self.needed(id, "SetPlayerTeamNumber() unit is null")?;
        let team = int_ref(vm, args, 2)?;

        match owner {
            Some(o) => {
                self.players.join(o, team);

                for p in 0..SHARING_PLAYERS {
                    self.share_team_ground(p);
                }

                Ok(Answer::Given)
            }
            None => Ok(Answer::Unknown),
        }
    }

    /// `UnitsAreSamePlayer(a, b)`, gpl units are same player: throws unless both have
    /// units; else whether their owners are one.
    pub(super) fn same_player(
        &self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let a = agent_ref(vm, args, 1)?;
        let b = agent_ref(vm, args, 2)?;
        let gone = |id: u32| {
            format!(
                "Function $UnitsAreSamePlayer( agent#{} ) failed: unit no longer exists.",
                id.cast_signed()
            )
        };
        let first = self.needed(a, &gone(a))?;
        let second = self.needed(b, &gone(b))?;

        match first.zip(second) {
            Some((x, y)) => {
                let ret = vm.arg(args, 0)?;

                put_bool(vm, ret, x == y).map(|()| Answer::Given)
            }
            None => Ok(Answer::Unknown),
        }
    }

    /// `NeutralTeamNumber()`, gpl neutral team number: [`NEUTRAL_TEAM`].
    pub(super) fn neutral_team(
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let ret = vm.arg(args, 0)?;

        put_int(vm, ret, NEUTRAL_TEAM).map(|()| Answer::Given)
    }

    /// `SpecifyIntent(agent, intent)`, gpl specify intent: throws with no unit;
    /// an integer intent, or a string's number, to
    /// [`INTENT`] through `GSUnit`. A string: unknown.
    pub(super) fn specify_intent(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let id = agent_ref(vm, args, 1)?;
        let owner = self.needed(id, "SpecifyIntent() unit is null")?;
        let intent = match vm.object(vm.arg(args, 2)?) {
            Value::String(_) => None,
            _ => Some(int_ref(vm, args, 2)?),
        };

        match (owner, intent) {
            (Some(_), Some(i)) => {
                self.write(id, INTENT, i);
                Ok(Answer::Given)
            }
            _ => Ok(Answer::Unknown),
        }
    }

    /// `AgentHasInventoryItem(item, agent)`, gpl agent has inventory item: throws with no
    /// unit; an item number under 256 is key `INV:n`, found in the unit's
    /// attribute map. -1 searches the level's register
    /// for the unit's first item (`World::first_item`): found, it is
    /// written to argument 1 and the answer is true; not found, false.
    /// **Inferred**: not found, argument 1 takes a stale stack word the
    /// scripts never read; here it is left. Any other item is its key
    /// (item id, `inventory::item_key`).
    pub(super) fn has_item(
        &self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let id = agent_ref(vm, args, 2)?;

        self.needed(id, "AgentHasInventoryItem() unit is null")?;

        let item = vm.arg(args, 1)?;
        let search = matches!(vm.object(item), Value::Integer(-1));
        let has = if search {
            let found = self.unit(id).map(|p| self.first_item(p.handle));

            if let Some(Some(n)) = found {
                put_int(vm, item, n)?;
            }

            found.map(|f| f.is_some())
        } else {
            let item = super::inventory::item_key(vm, args, 1)?;

            self.unit(id).map(|p| p.attributes.contains_key(&item))
        };

        match has {
            Some(b) => {
                let ret = vm.arg(args, 0)?;

                put_bool(vm, ret, b).map(|()| Answer::Given)
            }
            None => Ok(Answer::Unknown),
        }
    }

    /// `CombatSignal(agent)`, gpl combat signal: throws with no unit; else
    /// combat flash restarts or pushes a GS order `0x4001` on its
    /// track 1, 20 runs of 200 ms ([`super::walking::Flash`]): the
    /// minimap's flash, and an order, so it keeps the unit in ORDR.
    /// **Game**: the united winter game's peasant 19, hit at 23,400 ms,
    /// stays in ORDR through its next builds, ahead of minotaur 18.
    pub(super) fn combat_signal(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let id = agent_ref(vm, args, 1)?;

        match self.needed(id, "CombatSignal() unit is null")? {
            Some(_) => {
                self.flash(id);
                Ok(Answer::Given)
            }
            None => Ok(Answer::Unknown),
        }
    }

    /// `NewTeamNumber()`, gpl new team number: the first team with no player,
    /// else 0. Nothing is taken.
    pub(super) fn new_team(
        &self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let ret = vm.arg(args, 0)?;
        let team = self
            .players
            .teams()
            .iter()
            .position(|mask| *mask == 0)
            .unwrap_or(0);

        put_int(vm, ret, i32::try_from(team).unwrap_or(0)).map(|()| Answer::Given)
    }

    /// `SetShareReveal(player, share)`, gpl set share reveal: throws for a player
    /// the level lacks; else its `APP:1`, then
    /// [`World::share_team_ground`].
    pub(super) fn share_reveal(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let player = int_ref(vm, args, 1)?.cast_unsigned();
        let share = bool_ref(vm, args, 2)?;

        if self.players.contains(player) {
            self.set_sharing(player, share);
            self.share_team_ground(player);
            Ok(Answer::Given)
        } else {
            Err(Fault::throw("Function $SetShareReveal: player is invalid."))
        }
    }

    /// Player `player`'s `APP:1`.
    pub(super) fn set_sharing(&mut self, player: u32, share: bool) {
        let sharing = if share { Sharing::On } else { Sharing::Off };

        self.players.set_sharing(player, sharing);
    }

    /// Team share reveal on player `player`: with
    /// its `APP:1` set, every vertex another of players 0 to 3 on its
    /// team explored becomes its too. **Inferred**: the test of player
    ///  first, in the attribute map, always passes.
    pub(super) fn share_team_ground(&mut self, player: u32) {
        let mates = self.players.mates(player);
        let shares = self.players.shares(player) && self.players.contains(player);

        if shares && mates != 0 {
            self.fog
                .explored_mut()
                .share(mates, 1u8.wrapping_shl(player));
        }
    }

    /// `UnitIsLocalPlayer(agent)`, gpl unit is local player: throws with no unit;
    /// else whether its owner is the local player. Here
    /// whether it is a human seat's ([`World::humans`]): the answer
    /// must not differ by machine. No shipped script calls it.
    pub(super) fn local_player(
        &self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let id = agent_ref(vm, args, 1)?;
        let gone = format!(
            "Function $UnitIsLocalPlayer( agent#{} ) failed: unit no longer exists.",
            id.cast_signed()
        );

        match self.needed(id, &gone)? {
            Some(owner) => {
                let ret = vm.arg(args, 0)?;

                put_bool(vm, ret, self.humans().contains(&owner)).map(|()| Answer::Given)
            }
            None => Ok(Answer::Unknown),
        }
    }

    /// `IncrementStatCounter(agent, name)`, gpl increment stat counter: throws with no
    /// unit; [`GUTTER`] adds one to its owner's vice, any other name
    /// nothing. The end screen's "Vice" row.
    pub(super) fn stat_counter(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let id = agent_ref(vm, args, 1)?;
        let owner = self.needed(id, "IncrementStatCounter() unit is null")?;
        let name = string_ref(vm, args, 2)?;

        match owner {
            Some(o) => {
                if name.eq_ignore_ascii_case(GUTTER) {
                    self.players.add_vice(o);
                }

                Ok(Answer::Given)
            }
            None => Ok(Answer::Unknown),
        }
    }

    /// Player `player`'s vice: gutters its heroes visited.
    #[must_use]
    pub fn vice(&self, player: u32) -> i32 {
        self.players.vice(player)
    }

    /// `SetUnitPlayerNumber(unit, player)`, gpl set unit player number: a unit or a
    /// throw; its owner written, a gained one of its key
    /// counted, then method numbers it anew.
    ///
    /// The count is every human seat's ([`World::tally_converted`]). The
    /// hall's `Champion_Icon` work before the write is not modelled, as
    /// [`World::hall_flag`]'s is not, nor the agent's own step.
    pub(super) fn set_owner(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let id = agent_ref(vm, args, 1)?;
        let code = match self.found(id) {
            Found::Piece(p) => Some(p.code),
            Found::Gone => Err(Fault::throw("SetUnitPlayerNumber() unit is null"))?,
            Found::Unsure => None,
        };
        let player = int_ref(vm, args, 2)?.cast_unsigned();

        match (code, self.pieces.get_mut(id)) {
            (Some(code), Some(piece)) => {
                piece.owner = player;
                self.tally_converted(code);
                self.number(id);
                Ok(Answer::Given)
            }
            _ => Ok(Answer::Unknown),
        }
    }
}

mod persist {
    use super::*;
    use crate::save::{self as m, Refused, Saved};
    use std::result::Result;

    impl Saved for Players {
        type Model = m::Players;

        fn save(&self) -> m::Players {
            let records = self
                .records
                .iter()
                .map(|(slot, p)| {
                    let model = m::Player {
                        gold: p.gold,
                        sharing: p.sharing,
                        hall: p.hall.map(|h| m::Hall {
                            code: h.code,
                            length: h.length,
                            began: h.began,
                        }),
                        palace: p.palace,
                        vice: p.vice,
                        ledger: p.ledger.save(),
                        champion: m::Champion {
                            level: p.champion.level,
                            owner: p.champion.owner,
                            name: p.champion.name.clone(),
                            class: p.champion.class.clone(),
                        },
                        slain: p.slain.clone(),
                        counts: p.counts.save(),
                        end: p.end.save(),
                        dead: p.dead.save(),
                    };

                    (*slot, model)
                })
                .collect();

            m::Players {
                slots: self.slots.clone(),
                session: self.session,
                records,
                teams: self.teams,
            }
        }

        fn load(model: m::Players) -> Result<Players, Refused> {
            let records = model
                .records
                .into_iter()
                .map(|(slot, p)| {
                    let player = Player {
                        gold: p.gold,
                        sharing: p.sharing,
                        hall: p.hall.map(|h| Hall {
                            code: h.code,
                            length: h.length,
                            began: h.began,
                        }),
                        palace: p.palace,
                        vice: p.vice,
                        ledger: Ledger::load(p.ledger),
                        champion: Champion {
                            level: p.champion.level,
                            owner: p.champion.owner,
                            name: p.champion.name,
                            class: p.champion.class,
                        },
                        slain: p.slain,
                        counts: Option::load(p.counts)?,
                        end: Option::load(p.end)?,
                        dead: Vec::load(p.dead)?,
                    };

                    Ok((slot, player))
                })
                .collect::<Result<_, Refused>>()?;

            Ok(Players {
                slots: model.slots,
                session: model.session,
                records,
                teams: model.teams,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::world::reveal::Explored;

    /// Players 0, 1 and 7 on a new level's teams, a 1-cell map player 1
    /// explored.
    fn level() -> World {
        let mut explored = Explored::blank(1);

        explored.everything(0b10);

        World {
            players: Players::new(vec![0, 1, 7]),
            fog: super::super::reveal::Fog::new(explored, 0),
            ..World::default()
        }
    }

    /// Sharing on, a player takes what its teammates among 0 to 3
    /// explored; sharing off, nothing.
    #[test]
    fn a_sharing_player_takes_its_teammates_ground() {
        let mut world = level();
        let seen = |w: &World, bit: u8| w.fog.explored().flags().filter(|f| f & bit != 0).count();
        let theirs = seen(&world, 0b10);

        assert!(theirs > 0);

        world.share_team_ground(0);

        assert_eq!(seen(&world, 0b01), 0);

        world.set_sharing(0, true);
        world.share_team_ground(0);

        assert_eq!(seen(&world, 0b01), theirs);
    }

    /// The reveal mask: its own bit; with the crown sharing, a teammate's
    /// reveals for the crown too; slot 7 always alone; another team not.
    #[test]
    fn a_teammates_units_reveal_for_a_sharing_player() {
        let mut world = level();

        assert_eq!(world.players.reveal_mask(1), 0b10);

        world.set_sharing(0, true);

        assert_eq!(world.players.reveal_mask(1), 0b11);
        assert_eq!(world.players.reveal_mask(0), 0b01);
        assert_eq!(world.players.reveal_mask(7), 0x80);

        world.players.join(1, 2);

        assert_eq!(world.players.reveal_mask(1), 0b10);
    }

    /// A second human seat's sharing reaches the crown's mask; with one
    /// seat, player 1 is no seat and does not.
    #[test]
    fn a_second_seat_shares_with_the_crown() {
        let mut world = level();

        world.set_sharing(1, true);

        assert_eq!(world.players.reveal_mask(0), 0b01);

        world.players = world.players.clone().seated(2);

        assert_eq!(world.humans(), 0..2);
        assert_eq!(world.players.reveal_mask(0), 0b11);
    }

    /// The human seats go through a save.
    #[test]
    fn the_session_is_saved() {
        use crate::save::Saved;

        let players = Players::new(vec![0, 1, 7]).seated(2);

        assert_eq!(Players::load(players.save()), Ok(players));
        assert_eq!(Players::default().humans(), 0..SESSION_PLAYERS);
    }
}
