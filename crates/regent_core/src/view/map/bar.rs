//! The hit point bar on the map, over every unit that has one.
//!
//! **Binary** — `GSUnit` draws it when
//! Hp bar shown passes; `GSUnit`
//! places it about the unit's screen place:
//! the body, at its own height. No
//! movement layer test: a flier's bar hangs off its body, as its ring
//! does not.
//!
//! ```text
//!  screen place ─┐   R: PRIM +0x04..+0x10, ground pixels (placement::pixel_rect)
//!                ▼   x0 = (R.x0 + R.x1) / 2 - w / 2, y1 = R.y1
//!        ┌─────────────────────┐
//!        │█████████████        │▒   black frame, the fill in the owner's
//!        └─────────────────────┘▒   colour, w · hp / max long, 3 tall;
//!         ▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒   a shade two pixels deep, right and below
//! ```
//!
//! **Capture** (`local/shots/dragon_hover.png`): an Ice Dragon's
//! body anchor at (558, 320), its fill at x 543..573, y 381..384, its
//! frame black around it, the shade below and right. R's bottom is the
//! art's 53 screen pixels read as ground pixels, 64, so the bar sits
//! 64 pixels under the body: near the ground for a dragon flying 34
//! pixels up, inside its ring.

use crate::data::desc::{BuildingFlag, CharacterFlag, UnitKind};
use crate::data::error::Result;

use crate::content::unit::Unit;
use crate::data::catalogue::Catalogue;
use crate::sim::placement;
use crate::view::ScreenRect;
use crate::view::panel::portrait::Health;
use crate::view::raster::shade::{RAMPS, Shade};

/// The bar's length: 60 for a building, 30 for anything else
/// (`GSUnit`, `GenUnit` method `(0)`, unit kind 0).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Body {
    Building,
    Other,
}

impl Body {
    /// `unit`'s.
    #[must_use]
    pub const fn of(unit: &Unit<'_>) -> Body {
        match unit.desc.kind() {
            UnitKind::Building => Body::Building,
            _ => Body::Other,
        }
    }

    /// The bar's length, pixels.
    #[must_use]
    pub const fn width(self) -> i32 {
        match self {
            Body::Building => BUILDING_WIDTH,
            Body::Other => OTHER_WIDTH,
        }
    }
}

/// A building's bar length: masked with `0x1e`, plus `0x1e`, in `GSUnit`.
pub const BUILDING_WIDTH: i32 = 60;

/// Anyone else's.
pub const OTHER_WIDTH: i32 = 30;

/// The fill's height: `R.y1 - 3` to `R.y1`.
pub const HEIGHT: i32 = 3;

/// The shade two sides of the frame take: rect shade mode 1 with
/// table 3 of ramps shadow8, the first ramp family, `50 + 3 × 6`
/// percent of the value kept.
pub const BEVEL: Shade = Shade::Keep(RAMPS[0].0 + BEVEL_TABLE * RAMPS[0].1);

/// The ramp table the bevel's shade reads.
const BEVEL_TABLE: u32 = 3;

/// How deep the shade is, right and below.
const BEVEL_DEPTH: i32 = 2;

/// Whether the bars show at all: engine flag display flags bit 2, the
/// `DisplayFlagStats` setting (registry default 1), toggled by the main
/// frame's control `0x12`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stats {
    Shown,
    Hidden,
}

/// What hp bar shown asks of a unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Asked {
    /// Unit owner; negative for none.
    pub owner: i32,
    /// `APU:0`.
    pub hp: i32,
    /// `HasHPBar`: `BuildingDesc` bit 3, or `VehicleDesc` bit 2.
    pub has_bar: bool,
}

/// `HasHPBar`, on a building's flags or a character's.
#[must_use]
pub fn has_bar(unit: &Unit<'_>) -> bool {
    let building = unit.desc.building();
    let character = unit.desc.character();

    building.is_some_and(|b| b.has(BuildingFlag::HasHpBar))
        || character.is_some_and(|c| c.has(CharacterFlag::HasHpBar))
}

/// What a unit's art and descriptor give its bar: [`place`]'s rect and
/// length. `None` without art.
///
/// # Errors
///
/// If the art will not parse.
pub fn sized(catalogue: &Catalogue<'_>, unit: &Unit<'_>) -> Result<Option<([i32; 4], Body)>> {
    Ok(placement::pixel_rect(catalogue, unit)?.map(|r| (r, Body::of(unit))))
}

/// Hp bar shown: the bars shown, an owner, a hit point left and
/// `HasHPBar`. `GSUnit` also skips a held unit (unit state bit 1).
#[must_use]
pub const fn shown(stats: Stats, asked: Asked) -> bool {
    matches!(stats, Stats::Shown) && asked.owner >= 0 && asked.hp >= 1 && asked.has_bar
}

/// The bar's parts, about the unit's screen place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bar {
    /// Black, under the rest: one pixel round the full length.
    pub frame: ScreenRect,
    /// [`BEVEL`] over what is drawn: below the frame, then right of it.
    pub bevel: [ScreenRect; 2],
    /// The owner's colour (`GEPlayerColors`, the minimap's), as
    /// long as the hit points are of the most.
    pub fill: ScreenRect,
}

impl Bar {
    /// `GSUnit`'s outer rect: the frame and both shades.
    #[must_use]
    pub const fn outer(&self) -> ScreenRect {
        [
            self.frame[0],
            self.frame[1],
            self.bevel[0][2],
            self.bevel[0][3],
        ]
    }
}

/// The bar for a unit whose `PRIM` pixel rect is `r`
/// ([`crate::sim::placement::pixel_rect`]): centred on it, at its bottom.
/// Divisions truncate, as the engine's halving and divide do.
#[must_use]
#[allow(
    clippy::manual_midpoint,
    reason = "the engine's halving truncates toward zero, spelled out"
)]
pub const fn place(r: [i32; 4], body: Body, health: Health) -> Bar {
    let width = body.width();
    let x0 = (r[0] + r[2]) / 2 - width / 2;
    let (y0, y1) = (r[3] - HEIGHT, r[3]);
    let long = if health.max > 0 {
        health.now * width / health.max
    } else {
        0
    };

    // `GSUnit`'s outer rect, one in and three past; `GSUnit`
    // fills it black short of two, and shades the two it left.
    let outer = [x0 - 1, y0 - 1, x0 + width + 3, y1 + 3];

    Bar {
        frame: [
            outer[0],
            outer[1],
            outer[2] - BEVEL_DEPTH,
            outer[3] - BEVEL_DEPTH,
        ],
        bevel: [
            [
                outer[0] + BEVEL_DEPTH,
                outer[3] - BEVEL_DEPTH,
                outer[2],
                outer[3],
            ],
            [
                outer[2] - BEVEL_DEPTH,
                outer[1] + 1,
                outer[2],
                outer[3] - BEVEL_DEPTH,
            ],
        ],
        fill: [x0, y0, x0 + long, y1],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An Ice Dragon's rect, `[-47, -31, 47, 53]` on the screen, read as
    /// ground pixels; full health. Its parts as the capture shows them
    /// about its body at (558, 320) (module docs).
    #[test]
    fn a_dragons_bar_hangs_sixty_four_pixels_under_its_body() {
        let bar = place([-47, -37, 47, 64], Body::Other, Health::of(180, 180));

        assert_eq!(bar.fill, [-15, 61, 15, 64]);
        assert_eq!(bar.frame, [-16, 60, 16, 65]);
        assert_eq!(bar.bevel, [[-14, 65, 18, 67], [16, 61, 18, 65]]);
    }

    #[test]
    fn the_fill_is_as_long_as_the_hit_points_are_of_the_most() {
        let r = [-10, -10, 11, 20];
        let long = |hp, body| {
            let [x0, _, x1, _] = place(r, body, Health::of(hp, 70)).fill;

            x1 - x0
        };

        assert_eq!(long(70, Body::Other), 30);
        assert_eq!(long(35, Body::Building), 30);
        assert_eq!(long(69, Body::Other), 29);
        assert_eq!(long(-5, Body::Other), 0);
    }

    #[test]
    fn only_an_owned_living_unit_with_a_bar_shows_one() {
        let asked = Asked {
            owner: 7,
            hp: 1,
            has_bar: true,
        };

        assert!(shown(Stats::Shown, asked));
        assert!(!shown(Stats::Hidden, asked));
        assert!(!shown(Stats::Shown, Asked { owner: -1, ..asked }));
        assert!(!shown(Stats::Shown, Asked { hp: 0, ..asked }));
        assert!(!shown(
            Stats::Shown,
            Asked {
                has_bar: false,
                ..asked
            }
        ));
    }

    #[test]
    fn the_bevel_keeps_sixty_eight_percent() {
        assert_eq!(BEVEL, Shade::Keep(68));
    }
}
