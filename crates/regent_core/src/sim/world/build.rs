//! A building the player places: `CREATE_BUILDING_MESSAGE`. **Binary**
//! unless marked.
//!
//! ```text
//!  the build menu ── order construction: a preview, agent -6, made
//!     │               under engine flag 0x1000
//!  placing ─── SendMessage in frame f: the preview is deleted, UnitDelete
//!     │
//!  frame f + 1, before the clock step: GSMessageManager decodes
//!     └─ the building: price against the treasury (APP:0), else nothing
//!          GS_Level makes the unit and its agent: BuildingRec
//!          HitPoints (APU:0) 1: under construction
//!          BuildingRec (ConstructData1, 3): place search,
//!            flatten, clear (sim::clearing), Dirt
//!          the price off the treasury; GenUnit: NewUnitInit
//! ```

use crate::sim::world::kind;
use std::collections::BTreeMap;

use super::research::NOT_COMPLETED;
use super::{AGENT_ID, Agent, Piece, Place, World, placing::Blockers};
use crate::data::attrib::{
    CURRENT_STAGE_BUILT, GOLD, HP, IS_REPAIRED, IS_TAXED, MAX_GUILD_MEMBERS, MAX_HP, NOT_FLAGGABLE,
    NOT_SPELL_TARGET, SIGHT_RANGE, TAX_RATE,
};
use crate::data::desc::Building;
use crate::data::desc::BuildingFlag;
use crate::data::fourcc::FourCc;
use crate::gpl::load::Program;
use crate::gpl::vm::Fault;
use crate::sim::clearing;
use crate::sim::space;

/// `HitPoints` as agent create placed b leaves a new building:
/// `1`, `0x555041` in agent create placed b.
pub const FIRST_HP: i32 = 1;

/// `BuildingDesc` income type that sets a tax rate (compared with 2 in
/// `BuildingRec`).
const TAXED: u32 = 2;

/// `IsTaxed`'s default, default tax route: the byte at
/// default tax route is 1 in the file.
const DEFAULT_TAXED: i32 = 1;

/// `IsRepaired` for a building other than `ABe1`: `1` in `BuildingRec`.
const REPAIRED: i32 = 1;

/// `General_Housing`, whose `IsRepaired` and `IsTaxed` `BuildingRec`
/// reads from two bytes of its own (a compare with `0x31654241`).
const HOUSING: FourCc = FourCc(*b"ABe1");

/// Its `IsRepaired`: the byte at housing repaired, past `.data`'s file bytes,
/// so 0 at load; its only reference by address is this read (**Inferred**:
/// never written). **Game**: the probed play's House 18 saved 0.
const HOUSING_REPAIRED: i32 = 0;

/// Its `IsTaxed`: the byte at housing tax route, 1 file.
const HOUSING_TAXED: i32 = 1;

/// What building upgrade dispatches after the change: the string at
/// `"building_upgraded"`.
pub const UPGRADED: &str = "building_upgraded";

/// Where build preview make moves the preview: 32nds.
pub const PREVIEW_AT: (i32, i32) = (-1000, -1000);

/// The agent id gpl agent ctor gives under engine flag `0x1000`.
pub const PREVIEW_ID: i32 = -6;

/// A building's attributes as `BuildingRec` makes them,
/// in its order:
///
/// | Attribute | From |
/// |---|---|
/// | `APU:1`, `APU:3`, `APB:0` | max hp, sight range, guild members |
/// | `APU:9`, `APU:10` | `Flags` bit 2 and bit 4, as the bit |
/// | `APB:3` | income, when the income type is 2 |
/// | `APU:2` | gold |
/// | `APB:20`, `APB:16` | 1, and default tax route; for `ABe1` two bytes of its own |
/// | `APB:23` | 1, for a researcher ([`super::research::researcher`]: research done, in `BuildingRec`); a game's Last Palace Guardhouse takes none (`World::vf3_research`) |
///
/// `AP:1`, the name, is drawn after from the descriptor's generator
/// (name next of, `super::naming`); `sim::game` draws it once
/// the building is placed.
#[must_use]
pub fn initial(code: FourCc, b: &Building<'_>) -> Vec<(i32, i32)> {
    let v = |f: Option<u32>| f.unwrap_or_default().cast_signed();
    let flags = b.flags().unwrap_or_default();
    let bit = |f: BuildingFlag| (flags & (1 << f as u32)).cast_signed();
    let taxed = (b.income_type() == Some(TAXED))
        .then(|| (TAX_RATE.packed(), b.income_amount().unwrap_or(0)));
    let (repaired, taxed_route) = if code == HOUSING {
        (HOUSING_REPAIRED, HOUSING_TAXED)
    } else {
        (REPAIRED, DEFAULT_TAXED)
    };
    let research = super::research::researcher(code.0).then_some((NOT_COMPLETED.packed(), 1));

    [
        (MAX_HP.packed(), v(b.max_hp())),
        (SIGHT_RANGE.packed(), v(b.sight_range())),
        (MAX_GUILD_MEMBERS.packed(), v(b.max_guild_members())),
        (NOT_FLAGGABLE.packed(), bit(BuildingFlag::NoFlaggable)),
        (NOT_SPELL_TARGET.packed(), bit(BuildingFlag::NotSpellTarget)),
    ]
    .into_iter()
    .chain(taxed)
    .chain([
        (GOLD.packed(), v(b.gold())),
        (IS_REPAIRED.packed(), repaired),
        (IS_TAXED.packed(), taxed_route),
    ])
    .chain(research)
    .collect()
}

/// A building the player placed: `CREATE_BUILDING_MESSAGE`'s content. Its
/// point is the build preview's position at the click,
/// which no log or save holds;
/// `BuildingRec` truncates it to a half cell and searches from
/// there. So a placed building's half cell, free, stands for the click:
/// the search keeps it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BuildOrder {
    pub code: FourCc,
    /// 32nds of a cell.
    pub at: (i32, i32),
    pub owner: u32,
}

impl World {
    /// agent create placed b for a player's order: the new piece
    /// and agent, placed and levelled, named (`BuildingRec`,
    /// `world::naming`), the price paid. `None` when the
    /// treasury is short or the place search finds nothing. The caller
    /// dispatches `NewUnitInit`.
    ///
    /// A refused building draws nothing: its slot and name come after the
    /// placement. Not modelled: the unit the
    /// engine made before the search, its handle and
    /// agent, deleted at once (`GSUnit`,
    /// agent create placed b).
    ///
    /// # Errors
    ///
    /// The agent's `.dat` instance is missing.
    pub fn build(&mut self, program: &Program, order: &BuildOrder) -> Result<Option<u32>, Fault> {
        let body = self.descriptors.body(order.code).cloned();
        let price = self.building_price(order.owner, order.code);
        let afford = self.affords(order.owner, price);
        let spot = body
            .as_ref()
            .filter(|_| afford)
            .and_then(|b| self.place_among(b, order.at, Blockers::Standing));

        // A short treasury: a chat line, nothing more.
        if body.is_some() && !afford {
            self.say(
                order.owner,
                super::chat::Line::Text(super::chat::NO_GOLD_BUILDING),
            );
        }

        match (body, spot) {
            (Some(body), Some(at)) => {
                let hp = [(HP.packed(), FIRST_HP)];
                let id = self.raise(program, (order.code, &body), order.owner, &hp, at)?;

                self.name(id);

                // `Place_Building`'s `Begin` at it, as the owner's.
                self.cue_owned(
                    super::sound::From::Agent(id),
                    super::sound::PLACE_BUILDING,
                    super::sound::BEGIN,
                    order.owner,
                );

                self.spend(order.owner, price);

                // Stat built's price.
                self.count_paid(order.owner, super::ledger::Spend::Buildings, price);

                Ok(Some(id))
            }
            _ => Ok(None),
        }
    }

    /// A building's agent and piece at `at`, 32nds, found free: the next
    /// agent id and handle, `BuildingRec`'s attributes then `given`;
    /// its ground levelled, cleared and paved ([`super::building_ground`]).
    ///
    /// # Errors
    ///
    /// The agent's `.dat` instance is missing.
    pub(super) fn raise(
        &mut self,
        program: &Program,
        (code, body): (FourCc, &super::bodies::Body),
        owner: u32,
        given: &[(i32, i32)],
        at: (i32, i32),
    ) -> Result<u32, Fault> {
        let id = self.agents.next_id();
        let agent = Agent::made(program, &body.name).map_err(|_| {
            Fault::throw(format!(
                "NewAgent() could not successfully initialize object {}",
                body.name
            ))
        })?;
        let mut attributes: BTreeMap<i32, i32> = body.attributes.iter().copied().collect();

        self.vf3_research(code.0, &mut attributes);
        attributes.extend(given.iter().copied());

        let fixed = (
            crate::sim::units::from_pos32(at.0),
            crate::sim::units::from_pos32(at.1),
        );
        let standing = (space::half(fixed.0), space::half(fixed.1));

        attributes.insert(AGENT_ID.packed(), id.cast_signed());
        self.agents.advance();
        self.handle += 1;
        self.slot_draw(super::World::made_slots(body));

        self.take_ground(body, standing, clearing::Time::Playing);
        self.agents.insert(id, agent);
        self.pieces.insert(
            id,
            Piece {
                owner,
                attributes,
                handle: self.handle,
                code,
                name: body.name.clone(),
                place: Some(Place {
                    kind: kind::Kind::BUILDING,
                    fixed,
                    standing,
                    holder: 0,
                    state: 0,
                }),
                spells: Some(Vec::new()),
            },
        );
        self.note_items(id);
        self.agents.note_made(id);

        // Counted as placed, before method names it; its
        // price counted by the caller, none for a script's.
        self.tally(owner, code, super::numbered::Tally::Bought);
        self.count_built(owner, 0);

        // Agent create placed b's building damage set.
        self.restyle(id, super::plume::Force::Unforced);
        Ok(id)
    }

    /// Building upgrade for agent `id`'s building,
    /// `UPGRADE_BUILDING_MESSAGE` with no research: it becomes its
    /// `UpgradeTo` (unit change type, as
    /// `ChangeUnitType`): descriptor, grid and rect anew, place kept;
    /// `BuildingRec`'s attributes over its own, gold and the two
    /// defaults only where absent; then `CurrentStageBuilt` 0, the
    /// upgrade under construction. The engine then
    /// dispatches [`UPGRADED`].
    ///
    /// `ResearchNotCompleted` is set by research done on the new
    /// stage. The price is the new code's cost
    /// (`World::upgrade_price`); an unaffordable upgrade stops before
    /// changing the building. Unit change type's flag 1
    /// runs `GSUnit`: a slot draw on the world RNG
    /// (`World::slot_draw`). **Game**: The Barren Waste's upgrades at
    /// 2312 and 2578 are its two play draws.
    pub fn upgrade(&mut self, id: u32) {
        let target = self
            .piece_body(id)
            .and_then(|b| b.upgrade)
            .and_then(|code| Some((code, self.descriptors.body(code)?.clone())));

        // The price, taken before the change.
        if let (Some((code, _)), Some(owner)) = (&target, self.pieces.owner(id)) {
            let price = self.upgrade_price(owner, *code);

            if !self.spend(owner, price) {
                return;
            }

            self.count_paid(owner, super::ledger::Spend::Buildings, price);
        }

        // Unit change type's `GSUnit`, before the attributes.
        if let Some((_, body)) = &target {
            self.slot_draw(super::World::made_slots(body));
        }

        if let (Some((code, body)), Some(piece)) = (target, self.pieces.get_mut(id)) {
            let absent = [GOLD.packed(), IS_REPAIRED.packed(), IS_TAXED.packed()];

            for (k, v) in &body.attributes {
                if !absent.contains(k) || !piece.attributes.contains_key(k) {
                    piece.attributes.insert(*k, *v);
                }
            }

            piece.attributes.insert(CURRENT_STAGE_BUILT.packed(), 0);
            piece.code = code;
            piece.name = body.name;
        }

        // `ResearchNotCompleted` from research done on the new stage.
        let left = i32::from(!self.researched(id));

        if let Some(piece) = self.pieces.get_mut(id) {
            piece.attributes.insert(NOT_COMPLETED.packed(), left);
        }

        // Upgrade apply's building damage set.
        self.restyle(id, super::plume::Force::Unforced);
    }

    /// `owner`'s preview build preview make makes for `code`: agent
    /// [`PREVIEW_ID`] from the descriptor's instance, its unit off the map,
    /// no handle (**Game**: the inn trace's inn takes the next handle). Made
    /// where it is deleted: no script sees it before (**Game**).
    ///
    /// # Errors
    ///
    /// The agent's `.dat` instance is missing.
    pub fn preview(&mut self, program: &Program, code: FourCc, owner: u32) -> Result<u32, Fault> {
        let id = PREVIEW_ID.cast_unsigned();
        let name = self
            .descriptors
            .body(code)
            .map(|b| b.name.clone())
            .unwrap_or_default();
        let agent = Agent::made(program, &name)?;
        let fixed = (
            crate::sim::units::from_pos32(PREVIEW_AT.0),
            crate::sim::units::from_pos32(PREVIEW_AT.1),
        );

        self.agents.insert(id, agent);
        self.agents.note_made(id);
        self.pieces.insert(
            id,
            Piece {
                code,
                name,
                owner,
                place: Some(Place {
                    kind: kind::Kind::BUILDING,
                    fixed,
                    standing: (space::half(fixed.0), space::half(fixed.1)),
                    holder: 0,
                    state: 0,
                }),
                ..Piece::default()
            },
        );
        Ok(id)
    }

    /// The preview of `owner`'s `code` where build preview test
    /// moves it: `at`, 32nds, snapped to its half cell.
    ///
    /// # Errors
    ///
    /// As [`World::preview`].
    pub fn preview_at(
        &mut self,
        program: &Program,
        code: FourCc,
        owner: u32,
        at: (i32, i32),
    ) -> Result<u32, Fault> {
        let id = self.preview(program, code, owner)?;
        // Toward zero, as build preview test snaps.
        let snap = |v: i32| {
            crate::sim::units::from_pos32(
                v / crate::sim::units::PER_HALF * crate::sim::units::PER_HALF,
            )
        };
        let fixed = (snap(at.0), snap(at.1));

        if let Some(place) = self.pieces.place_mut(id) {
            place.fixed = fixed;
            place.standing = (space::half(fixed.0), space::half(fixed.1));
        }

        self.jumped(id);
        Ok(id)
    }

    /// End agent `id` and its piece, as gpl agent delete does after
    /// `UnitDelete`.
    pub fn delete(&mut self, id: u32) {
        self.poison_sources.remove(&id);
        self.agents.remove(id);
        self.pieces.remove(id);
        self.units.take_tracks(id);
        self.dispatches.unlist(super::Listed::Unit(id));
    }
}

#[cfg(test)]
mod tests {
    use super::super::bodies::Class;
    use super::super::ruin::tests::body;
    use super::*;

    /// **Binary**: a building the place search refuses draws no name.
    /// Agent create placed b makes the unit, then `BuildingRec`
    /// refuses it and `GSUnit` deletes it; the name is
    /// `BuildingRec`'s, tail called by `GSUnit` after a placement,
    /// and nothing else on the way calls it.
    #[test]
    fn a_refused_building_draws_nothing() {
        let code = FourCc(*b"ABP1");
        let wide: Vec<(i32, i32)> = (-8..=8).map(|x| (x, 0)).collect();
        let mut world = World {
            board: super::super::board::Board::sized((8, 8)),
            ..World::default()
        };

        world
            .descriptors_mut()
            .add_body(code, body(Class::Building, 0x30, wide));
        world.set_treasury(0, 1_000);

        let mut rngs = world.rngs.clone();
        let order = BuildOrder {
            code,
            at: (64, 64),
            owner: 0,
        };

        assert_eq!(world.build(&Program::default(), &order).ok(), Some(None));
        assert_eq!(world.rngs.draw(1_000), rngs.draw(1_000));
    }

    #[test]
    fn an_unaffordable_upgrade_keeps_the_building_and_gold() {
        let old = FourCc(*b"ABM2");
        let next = FourCc(*b"ABM3");
        let id = 7;
        let mut world = World::default();
        let mut old_body = body(Class::Building, 0x30, vec![]);
        old_body.upgrade = Some(next);
        let mut next_body = body(Class::Building, 0x30, vec![]);
        next_body.cost = 1_000;

        world.descriptors_mut().add_body(old, old_body);
        world.descriptors_mut().add_body(next, next_body);
        world.pieces.insert(
            id,
            Piece {
                code: old,
                owner: 0,
                ..Piece::default()
            },
        );
        world.set_treasury(0, 500);

        assert!(world.upgrade_refused(id));
        world.upgrade(id);
        assert_eq!(world.piece(id).map(|p| p.code), Some(old));
        assert_eq!(world.treasury(0), Some(500));

        world.set_treasury(0, 1_000);
        assert!(!world.upgrade_refused(id));
        world.upgrade(id);
        assert_eq!(world.piece(id).map(|p| p.code), Some(next));
        assert_eq!(world.treasury(0), Some(0));
    }
}
