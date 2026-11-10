//! Illusionary Hero's clone, which player spell apply makes itself.
//! **Binary** unless marked.
//!
//! ```text
//!  target APU:0 above 0, else refused
//!  the level makes a CVC1 unit (agent illusionary_hero), the target's
//!    owner, at the target (its holder's place while held)
//!  the type changes to the target's descriptor, agent kept
//!  two world draws of 0x60: the place search from the target + (dx, dy),
//!    each draw less 0x20; the unit moves there
//!  APU:1 and APU:0 the target's MaxHP; set 8; APV:46 1
//!  relation_add(clone, target, 100): its HOME the target
//!  GenUnit: NewUnitInit, then Illusionary_Hero_Birth by the agent
//! ```
//!
//! No trace casts it. **Inferred**: the world draws the new unit's slot
//! once, before the two. Its own name (`GSUnit`,
//! Player spell apply) is `GMTX` 71, `Illusionary %s`, over its descriptor's
//! `HEAD` title: [`Given::Illusion`].

use crate::data::attrib::HAS_EFFECT_ILLUSIONARY_HERO;
use crate::data::fourcc::FourCc;
use crate::gpl::load::Program;
use crate::gpl::vm::Fault;
use crate::sim::world::naming::Given;
use crate::sim::world::{World, placing::Blockers};

/// The unit Illusionary Hero makes before it takes the target's type
/// (`'CVC1'`, player spell apply).
pub const ILLUSION_UNIT: [u8; 4] = *b"CVC1";

/// The bound of each world draw that spreads the clone (`0x60`,
/// player spell apply).
pub const ILLUSION_SPREAD: i32 = 0x60;

/// Taken off each draw, so the spread is about the target (`0x20`,
/// player spell apply).
pub const ILLUSION_CENTRE: i32 = 0x20;

impl World {
    /// Illusionary Hero on `target`: its clone, standing near it; `None`
    /// where the world lacks the target or either descriptor. The caller
    /// dispatches `NewUnitInit`.
    pub(in crate::sim::world) fn illusion(
        &mut self,
        program: &Program,
        target: u32,
    ) -> Result<Option<u32>, Fault> {
        let made = self.pieces.get(target).and_then(|t| {
            let looks = self.descriptors.body(t.code)?;
            let agent = self.descriptors.body(FourCc(ILLUSION_UNIT))?;
            let mut body = looks.clone();

            body.name.clone_from(&agent.name);
            Some((t.code, body, t.owner, self.position(target)?))
        });

        match made {
            Some((code, body, owner, from)) => {
                let id = self.stand(program, (code, &body), owner, &[], Some(from))?;

                self.spread(id, target, &body, from);

                // Its own name, over the title it took: no number.
                let u = self.units.entry(id);

                u.given = Some(Given::Illusion(code));
                u.number = None;
                Ok(Some(id))
            }
            None => Ok(None),
        }
    }

    /// The clone `id` of `target`: moved to the place search's spot about
    /// two world draws, at the target's hit points, illusory, its `HOME`
    /// the target.
    fn spread(
        &mut self,
        id: u32,
        target: u32,
        body: &crate::sim::world::bodies::Body,
        from: (i32, i32),
    ) {
        let dx = self.rngs.draw(ILLUSION_SPREAD) - ILLUSION_CENTRE;
        let dy = self.rngs.draw(ILLUSION_SPREAD) - ILLUSION_CENTRE;
        let spot = self.place_among(body, (from.0 + dx, from.1 + dy), Blockers::Others(id));

        if let Some(to) = spot {
            self.put(id, to);
        }

        let target_piece = self.pieces.get(target);
        let handle = target_piece.map_or(0, |t| t.handle);
        let max = target_piece.and_then(|t| {
            t.attributes
                .get(&crate::data::attrib::MAX_HP.packed())
                .copied()
        });

        if let Some(clone) = self.pieces.get_mut(id) {
            let own = clone
                .attributes
                .get(&crate::data::attrib::MAX_HP.packed())
                .copied();
            let hp = max.or(own).unwrap_or_default();

            clone
                .attributes
                .insert(crate::data::attrib::MAX_HP.packed(), hp);
            clone
                .attributes
                .insert(crate::data::attrib::HP.packed(), hp);
            clone
                .attributes
                .insert(HAS_EFFECT_ILLUSIONARY_HERO.packed(), 1);

            let own_handle = clone.handle;

            self.relations.set_home(own_handle, vec![handle]);
        }
    }
}
