//! `CreateSpellUnit`: a spell unit a script makes. **Binary**,
//! Gpl create spell unit, unless marked.
//!
//! ```text
//!  CreateSpellUnit(caster, name, target)
//!    no caster unit     throw "unit no longer exists"
//!    no target unit     nothing
//!    the unit registry by name,
//!      none: throw; kind not 3, 4 or 5: throw "not a vehicle"
//!    the target's aim for the descriptor: where it stands: the target's place plus
//!                              its hit point, two GPL draws at a building
//!                              (IsoGenUnit)
//!    GE_Level makes it (1, code, 0, 0x50, x, y, z, caster's owner, 0)
//!    kind not 5         animated: set 0x50, then 0x40
//!    GenUnit            NewUnitInit(unit)
//!    script_call_agents its PRIM function (unit, target)
//! ```
//!
//! Registered as returning an agent, it writes none. Unlike a cast's unit,
//! it gets no `SightRange` (**Inferred**: so
//! reveals nothing) and no price. Shipped calls: `Earthquake`
//! (`Earthquake_Event`, quests), `Lightning_Storm`, `Meteor_Storm`,
//! `Critical_hit`, `vortex`, and `fire_ball`, a particle system, which
//! stands as a [`crate::sim::world::plume::Plume`] with an agent.

use crate::sim::world::kind;
use std::collections::BTreeMap;

use crate::base::fixed::Fixed;
use crate::data::fourcc::FourCc;
use crate::gpl::load::Program;
use crate::gpl::ops::Value;
use crate::gpl::vm::{Fault, Handle, Vm};
use crate::sim::space;
use crate::sim::world::{
    Agent, Answer, Found, Piece, Place, World, agent_ref,
    bodies::{Body, Class},
    missile::{Missile, Spread},
    places::Motion,
    string_ref,
};

impl World {
    /// `CreateSpellUnit(caster, name, target)`. Unknown where the caster
    /// or target is, or the target's place.
    pub(in crate::sim::world) fn create_spell_unit(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let caster = agent_ref(vm, args, 1)?;
        let name = string_ref(vm, args, 2)?;
        let owner = match self.found(caster) {
            Found::Piece(p) => Some(p.owner),
            Found::Gone => Err(Fault::throw(format!(
                "Function $CreateSpellUnit( agent#{} ) failed: unit no longer exists.",
                caster.cast_signed()
            )))?,
            Found::Unsure => None,
        };
        let target = agent_ref(vm, args, 3)?;
        let aimed = match self.found(target) {
            Found::Piece(_) => Some(true),
            Found::Gone => Some(false),
            Found::Unsure => None,
        };

        match (owner, aimed) {
            (Some(owner), Some(true)) => {
                let (code, body) = self
                    .unit_named(&name)
                    .and_then(|c| Some((c, self.descriptors.body(c)?.clone())))
                    .ok_or_else(|| {
                        Fault::throw(format!(
                            "CreateSpellUnit() could not make unit of type {name} (could not find description) "
                        ))
                    })?;

                if !matches!(
                    body.class,
                    Class::Character | Class::Overlay | Class::Particles
                ) {
                    Err(Fault::throw(format!(
                        "CreateSpellUnit() could not make unit of type {name} (not a vehicle) "
                    )))?;
                }

                match self.summon_point(code, body.spread, target) {
                    Some(at) => {
                        let made = Made { code, owner, at };

                        self.summon(vm.program(), &made, &body, target)?;
                        Ok(Answer::Given)
                    }
                    None => Ok(Answer::Unknown),
                }
            }
            (Some(_), Some(false)) => Ok(Answer::Given),
            _ => Ok(Answer::Unknown),
        }
    }

    /// The unit registry's first descriptor named `name`
    /// ([`crate::sim::world::bodies::body_names`]).
    fn unit_named(&self, name: &str) -> Option<FourCc> {
        self.descriptors.body_named(name).map(|(code, _)| code)
    }

    /// `target.method(descriptor)`: the target's place, 3D, plus where a
    /// missile of `code` would hit it, drawing as method does.
    fn summon_point(&mut self, code: FourCc, spread: Spread, target: u32) -> Option<[Fixed; 3]> {
        let missile = Missile {
            spread,
            ..Missile::of(code, 0)
        };
        let at = self.unit_pos3(target)?;
        let off = self.aim(target, missile)?;

        Some([at[0] + off[0], at[1] + off[1], at[2] + off[2]])
    }

    /// `GE_Level` and what follows: the unit, the next agent and
    /// handle, a particle system its own plume; else its first set's
    /// systems and spell unit animate; then `NewUnitInit` and its
    /// `PRIM` function queued, in that order.
    fn summon(
        &mut self,
        program: &Program,
        made: &Made,
        body: &Body,
        target: u32,
    ) -> std::result::Result<(), Fault> {
        let id = self.agents.next_id();
        let agent = Agent::made(program, &body.name)?;
        let particles = body.class == Class::Particles;

        if particles {
            self.stand_plume(made.code, made.owner, made.at);
        } else {
            self.handle += 1;
        }

        self.agents.advance();
        self.agents.insert(id, agent);
        self.pieces.insert(
            id,
            Piece {
                owner: made.owner,
                attributes: BTreeMap::from([(
                    crate::sim::world::AGENT_ID.packed(),
                    id.cast_signed(),
                )]),
                handle: self.handle,
                code: made.code,
                name: body.name.clone(),
                place: Some(Place {
                    kind: kind::Kind::VEHICLE,
                    fixed: (made.at[0], made.at[1]),
                    standing: (space::half(made.at[0]), space::half(made.at[1])),
                    holder: 0,
                    state: body.state,
                }),
                spells: Some(Vec::new()),
            },
        );
        self.units.set_motion(id, Motion::Still);
        self.agents.note_made(id);

        if !particles {
            self.first_vents(self.handle, id, made.code);
            self.units.entry(id).spelled = Some(self.now);
        }

        self.dispatch(crate::sim::world::NEW_UNIT, vec![Value::Agent(id)]);

        let function = body.script.as_ref().map(|s| s.function.as_str());

        if let Some(f) = function.filter(|f| !f.is_empty()) {
            self.dispatch(f, vec![Value::Agent(id), Value::Agent(target)]);
        }

        Ok(())
    }
}

/// A spell unit to make: its descriptor, owner, and point, 16.16 cells.
struct Made {
    code: FourCc,
    owner: u32,
    at: [Fixed; 3],
}
