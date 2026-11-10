//! A hero's enchantments: the second list of its spells page, `AP78`'s
//! control 8731. **Binary**.
//!
//! ```text
//!  HeroSpellsController        the spell list (8730),
//!                              then the enchantments
//!  HeroSpellsController        both again every 500 ms
//!  the enchantments            each member of the selected
//!    unit's relation list 1 (the units hung on it), in list
//!    order: its descriptor code through a switch to a GMTX
//!    title and an icon set; a code not in the switch is skipped
//! ```
//!
//! The units hung on a unit are its effectors (`CreateEffector`); a
//! spell's lasting one is its `_icon`: `Dauros_Stone_Skin_Begin` hangs
//! `dauros_stone_skin_icon` (`MRA2`) for 30,000 ms.
//! `OpenRegent` adds the active effects that switch skips. It also uses
//! the timed buff to distinguish Fire Shield from Flame Shield while
//! their shared casting visual plays. See `docs/openregent/enhancements.md`.

use crate::content::unit::Unit;
use crate::data::catalogue::Catalogue;
use crate::data::fourcc::FourCc;
use crate::data::text::{self, Ref, Text};
use crate::sim::world::World;

/// One arm of hero enchantments' switch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Enchantment {
    /// The hung unit's descriptor code.
    pub code: FourCc,
    /// Its `GMTX` title; absent uses the overlay descriptor's title.
    pub title: Option<Ref>,
    /// Its `IMAG` icon; absent effects use text alone.
    pub icon: Option<(FourCc, u16)>,
}

impl Enchantment {
    /// The row's title from the game text or its parsed descriptor.
    #[must_use]
    pub fn name(&self, catalogue: &Catalogue<'_>, text: &Text<'_>) -> String {
        self.title
            .and_then(|key| text.get(key))
            .map(|t| text::plain(&t).into_owned())
            .or_else(|| {
                Unit::descriptor(catalogue, self.code)
                    .and_then(Result::ok)
                    .map(|unit| unit.head.title().into_owned())
            })
            .unwrap_or_else(|| self.code.to_string())
    }
}

/// The heroes' own spells' icons, as the spell list's.
const HEROES: FourCc = FourCc::new(b"INTn");

/// The player's spells' icons, as the spell bar's.
const PLAYERS: FourCc = FourCc::new(b"INTN");

/// The expansion's potions' and spells' icons.
const EXPANSION: FourCc = FourCc::new(b"IX93");

/// Shield overlays shared by two spells.
const FIRE_SHIELD_VISUAL: FourCc = FourCc::new(b"WRb1");
const FIRE_SHIELD_BUFF: FourCc = FourCc::new(b"WRb2");
const FLAME_SHIELD_BUFF: FourCc = FourCc::new(b"WRb0");

/// Rows that replace brief visuals or may have several attached copies.
const ONE_ROW: [FourCc; 4] = [
    FIRE_SHIELD_VISUAL,
    FourCc::new(b"WRf1"),
    FLAME_SHIELD_BUFF,
    FourCc::new(b"CRB2"),
];

/// Timed overlays the original switch skips, paired with their named row.
const ALIASES: [(FourCc, FourCc); 3] = [
    (FIRE_SHIELD_BUFF, FIRE_SHIELD_VISUAL),
    (FourCc::new(b"WRf2"), FourCc::new(b"WRf1")),
    (FourCc::new(b"CRB1"), FourCc::new(b"CRB2")),
];

/// Active effects with no row in the original switch.
const EXTRA: [FourCc; 7] = [
    FourCc::new(b"NRe3"),
    FourCc::new(b"XR25"),
    FourCc::new(b"NRi1"),
    FourCc::new(b"QRc1"),
    FourCc::new(b"QRd1"),
    FourCc::new(b"QRd0"),
    FourCc::new(b"QRb1"),
];

const fn arm(code: [u8; 4], title: usize, icon: (FourCc, u16)) -> Enchantment {
    Enchantment {
        code: FourCc(code),
        title: Some(Ref::gmtx(title)),
        icon: Some(icon),
    }
}

/// Every arm, by title. The switch has 32; `GMTX` 432 "Power Shock" has
/// none.
pub const ENCHANTMENTS: [Enchantment; 32] = [
    arm(*b"WRb1", 148, (HEROES, 1002)),
    arm(*b"WRf1", 149, (HEROES, 1006)),
    arm(*b"HRb1", 150, (HEROES, 1009)),
    arm(*b"HRc1", 151, (HEROES, 1010)),
    arm(*b"MRa1", 152, (HEROES, 1011)),
    arm(*b"MRb1", 153, (HEROES, 1012)),
    arm(*b"MRc1", 154, (HEROES, 1013)),
    arm(*b"CRb1", 155, (HEROES, 1016)),
    arm(*b"CRc1", 156, (HEROES, 1017)),
    arm(*b"LRa1", 157, (HEROES, 1021)),
    arm(*b"WRB2", 158, (PLAYERS, 1002)),
    arm(*b"WRD2", 159, (PLAYERS, 1004)),
    arm(*b"HRB2", 160, (PLAYERS, 1008)),
    arm(*b"MRA2", 161, (PLAYERS, 1010)),
    arm(*b"MRB2", 162, (PLAYERS, 1011)),
    arm(*b"MRC2", 163, (PLAYERS, 1012)),
    arm(*b"CRB2", 164, (PLAYERS, 1014)),
    arm(*b"PRB2", 165, (PLAYERS, 1017)),
    arm(*b"DRA2", 166, (PLAYERS, 1019)),
    arm(*b"QRa1", 167, (HEROES, 1024)),
    arm(*b"XR05", 425, (EXPANSION, 1016)),
    arm(*b"XR07", 426, (EXPANSION, 1015)),
    arm(*b"XR09", 427, (EXPANSION, 1018)),
    arm(*b"XR14", 428, (EXPANSION, 1017)),
    arm(*b"XR21", 429, (EXPANSION, 1007)),
    arm(*b"XR29", 430, (EXPANSION, 1012)),
    arm(*b"WRb0", 431, (EXPANSION, 1005)),
    arm(*b"XR59", 433, (EXPANSION, 1009)),
    arm(*b"XR01", 434, (EXPANSION, 1019)),
    arm(*b"XR03", 435, (EXPANSION, 1014)),
    arm(*b"XR57", 436, (EXPANSION, 1000)),
    arm(*b"XR13", 437, (EXPANSION, 1002)),
];

/// The row of a hung unit's code, including openregent's added effects.
#[must_use]
pub fn of(code: FourCc) -> Option<Enchantment> {
    let named = ALIASES
        .iter()
        .find(|(from, _)| *from == code)
        .map_or(code, |(_, to)| *to);

    ENCHANTMENTS
        .iter()
        .find(|e| e.code == named)
        .copied()
        .or_else(|| {
            EXTRA.contains(&named).then_some(Enchantment {
                code: named,
                title: None,
                icon: None,
            })
        })
}

/// What openregent lists for `agent`. **Inferred**: relation list 1 keeps
/// hang order.
#[must_use]
pub fn on(world: &World, agent: u32) -> Vec<Enchantment> {
    let hung: Vec<_> = world.hung_on(agent).collect();
    let flame = hung.contains(&FLAME_SHIELD_BUFF);
    let fire = hung.contains(&FIRE_SHIELD_BUFF);
    let mut rows = Vec::new();

    for code in hung {
        // Flame Shield shares Fire Shield's casting visual.
        let named = if code == FIRE_SHIELD_VISUAL && flame && !fire {
            FLAME_SHIELD_BUFF
        } else {
            code
        };

        if let Some(row) = of(named)
            && (!(EXTRA.contains(&row.code) || ONE_ROW.contains(&row.code)) || !rows.contains(&row))
        {
            rows.push(row);
        }
    }

    rows
}
