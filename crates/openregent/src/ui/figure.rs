//! The Book of Lore's figure beside an entry's portrait: a character's
//! `Stand`, a building's unlit `Inactive`, every layer, as the map
//! draws them (`regent_draw::art::stack`).

use regent_core::content::unit::Unit;
use regent_core::data::catalogue::Catalogue;
use regent_core::data::desc::UnitKind;
use regent_core::data::fourcc::FourCc;
use regent_core::data::imag::SetKind;
use regent_draw::art::stack::{self, Timeline};

/// A direction of 32, 0 north, clockwise.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum Facing {
    /// A new unit's, and a building's: it never turns
    /// (`world::walking::FIRST_FACING`).
    North = 0,
    /// Down and left on screen, toward the reader: a character's. South
    /// is 16, west 24 (`content::terrain::compass`); north shows its back.
    SouthWest = 20,
}

/// What an entry's figure draws: its set, facing and timeline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Figure {
    /// Low 16 bits of the set id.
    pub set: u16,
    pub facing: Facing,
    pub timeline: Timeline,
}

/// The sets a figure of `kind` tries, first found drawn. A building
/// unlit shows `Inactive`, else `Active` where its art has none
/// (`sim::damage`, unlit otherwise). A character standing shows `Stand`.
#[must_use]
pub const fn kinds(kind: UnitKind) -> &'static [SetKind] {
    match kind {
        UnitKind::Building => &[SetKind::Inactive, SetKind::Active],
        _ => &[SetKind::Stand],
    }
}

/// The way a figure of `kind` faces.
#[must_use]
pub const fn facing(kind: UnitKind) -> Facing {
    match kind {
        UnitKind::Building => Facing::North,
        _ => Facing::SouthWest,
    }
}

/// Unit `code`'s figure; `None` without the unit, its set, or a layer
/// that draws.
#[must_use]
pub fn of(catalogue: &Catalogue<'_>, code: FourCc) -> Option<Figure> {
    let unit = Unit::find(catalogue, code)?.ok()?;
    let kind = unit.desc.kind();
    let facing = facing(kind);

    // The kind's first set, `Inactive` + 0 rather than a later event's.
    let set = kinds(kind).iter().find_map(|k| {
        let first = u16::try_from(k.id()).ok()?;

        unit.drawn(first)
    })?;
    let (_, timeline) = stack::stack_of(catalogue, code, set, facing as u16)?;

    Some(Figure {
        set,
        facing,
        timeline,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_building_shows_inactive_else_active() {
        assert_eq!(
            kinds(UnitKind::Building),
            [SetKind::Inactive, SetKind::Active]
        );
        assert_eq!(facing(UnitKind::Building), Facing::North);
    }

    #[test]
    fn a_character_shows_stand_facing_south_west() {
        assert_eq!(kinds(UnitKind::Character), [SetKind::Stand]);
        assert_eq!(facing(UnitKind::Character), Facing::SouthWest);
        assert_eq!(Facing::SouthWest as u16, 20);
    }

    /// Every shipped entry, all with a portrait, draws a figure: 76
    /// characters their `Stand`, 102 buildings their `Inactive` (none
    /// falls back to `Active`), each step one size.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn every_lore_entry_with_a_portrait_has_a_figure() {
        use regent_draw::art::{Pic, Stack, read, stack::stacked};

        let hd = crate::files::env::path(crate::files::env::HD).expect("MAJESTY_HD");
        let lore = crate::files::lore::Catalogue::read(&hd).expect("shipped data");
        let art = regent_draw::data::catalogue(&lore.1).expect("art");
        let stand = u16::try_from(SetKind::Stand.id()).expect("a set id");
        let inactive = u16::try_from(SetKind::Inactive.id()).expect("a set id");
        let mut sets = std::collections::BTreeMap::new();

        let missing: Vec<FourCc> = lore
            .0
            .keys()
            .copied()
            .filter(|&code| read::sprite(&art, Pic::Portrait(code)).is_some())
            .filter(|&code| {
                // Each step's size: none, or two, is a miss.
                of(&art, code).is_none_or(|found| {
                    let sizes: Vec<_> = (0..found.timeline.steps)
                        .map(|step| {
                            let stack = Stack {
                                code,
                                set: found.set,
                                facing: found.facing as u16,
                                step,
                            };

                            stacked(&art, stack).map(|image| image.size)
                        })
                        .collect();

                    *sets.entry(found.set).or_insert(0) += 1;
                    sizes.iter().any(|s| s.is_none() || *s != sizes[0])
                })
            })
            .collect();

        assert!(missing.is_empty(), "no figure: {missing:?}");
        assert_eq!(sets, [(stand, 76), (inactive, 102)].into());
    }
}
