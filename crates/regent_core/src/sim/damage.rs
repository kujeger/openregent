//! The set a building shows: built or not, damaged, lit, in ruins.
//! **Binary** unless marked.
//!
//! Building damage set picks it; `GSUnit`
//! calls it on every attribute write, unforced, and `GSUnit`, as an
//! action's set ends, forced.
//!
//! ```text
//!  unforced and track 2 holds an InitAction (GenUnit      (2, 11)): keep
//!  HP <= 0                              Dead, 0xe0
//!  HP < MaxHP, CurrentStageBuilt 0      a Build stage, 0x50 up, by HP built
//!  HP < MaxHP, CurrentEvent 0           a Die stage, 0x60 up, by HP lost
//!  else, n by CurrentEvent (1 2 4 3 -> 1 2 3 4, else 0):
//!    lit: someone inside (relation 2), or ForceBuildingState 2,
//!         not ForceBuildingState 1                    Active + n, 0xc0
//!    unlit, the art has Inactive, no Sorcerer's spell Inactive + n, 0xd0
//!    unlit otherwise                                  Active + n
//! ```
//!
//! A stage: the art's sets from the block's first, up to
//! ten, while each is there, split 0 to 100 evenly; the percent picks
//! one. None fits, or the art has none: the set stays as it was. The
//! picked set plays from frame 0 unless track 1 animates it already.
//!
//! **Game**, the defeat save: the palace at 95 of 550 hit points has lost
//! 82%, its art eight `Die` sets, so `Die-7`; Wizards Tower #1 at 240 of
//! 250 lost 4%, `Die-1`.

/// `Die` stages drawing the finished building's picture: the first two of
/// eight, so up to a quarter of its hit points lost; its fires show, the
/// wreck does not. **Measured**.
pub const UNMARKED: u16 = 2;

/// `Build`, the first construction stage.
pub const BUILD: u16 = 0x50;

/// `Die`, the first damage stage of a building (a character's death).
pub const DIE: u16 = 0x60;

/// `Active`: lit.
pub const ACTIVE: u16 = 0xc0;

/// `Inactive`: unlit.
pub const INACTIVE: u16 = 0xd0;

/// `Dead`: in ruins.
pub const DEAD: u16 = 0xe0;

/// `Crumble`: what `Become_Rubble` plays as the building falls.
pub const CRUMBLE: u16 = 0xf0;

/// Sets a stage block holds at most: a compare with 10 in building stage.
const STAGES: u16 = 10;

/// What a stage's percent is out of.
const WHOLE: i32 = 100;

/// `ForceBuildingState` values: 1 unlit, 2 lit.
const FORCED_UNLIT: i32 = 1;
const FORCED_LIT: i32 = 2;

/// What [`set`] reads of a building.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Condition {
    /// `APU:0`.
    pub hp: i32,
    /// `APU:1`.
    pub max: i32,
    /// `APB:2` `CurrentStageBuilt` not 0.
    pub built: bool,
    /// The `MaxHP` of its `UpgradeFrom` descriptor: an upgrade's stages
    /// count from there. 0
    /// for none.
    pub base: i32,
    /// `APB:5` `CurrentEvent`.
    pub event: i32,
    /// `ASS:0` `SorcerersAbodeSpellStartTime` not 0.
    pub spell: bool,
    /// `APB:11` `ForceBuildingState`.
    pub force: i32,
    /// A unit inside it: its relation 2 list is not empty.
    pub occupied: bool,
}

/// The set a building in condition `c` shows; `has` says whether its art
/// holds a set. `None` keeps the one it shows.
///
/// ```
/// use regent_core::sim::damage::{self, Condition};
///
/// let palace = Condition { hp: 95, max: 550, built: true, ..Condition::default() };
///
/// assert_eq!(damage::set(&palace, |s| (0x60..0x68).contains(&s)), Some(0x66));
/// assert_eq!(damage::set(&Condition { hp: -3, ..palace }, |_| true), Some(damage::DEAD));
/// ```
#[must_use]
pub fn set(c: &Condition, has: impl Fn(u16) -> bool) -> Option<u16> {
    let full = c.hp >= c.max;

    if c.hp <= 0 {
        Some(DEAD)
    } else if !c.built && !full {
        stage(BUILD, built_percent(c), &has)
    } else if !full && c.event == 0 {
        stage(DIE, lost_percent(c), &has)
    } else {
        let n = variant(c.event);
        let lit = match c.force {
            FORCED_UNLIT => false,
            FORCED_LIT => true,
            _ => c.occupied,
        };

        if !lit && has(INACTIVE) && !c.spell {
            Some(INACTIVE + n)
        } else {
            Some(ACTIVE + n)
        }
    }
}

/// The set of block `first` at `percent`: of the `n` sets the art has
/// from `first` on, the `i`th where `i * 100 / n <= percent < (i + 1) *
/// 100 / n`.
///
/// ```
/// use regent_core::sim::damage::{stage, DIE};
///
/// let four = |s: u16| (DIE..DIE + 4).contains(&s);
///
/// assert_eq!(stage(DIE, 24, four), Some(DIE));
/// assert_eq!(stage(DIE, 25, four), Some(DIE + 1));
/// assert_eq!(stage(DIE, 100, four), None);
/// ```
#[must_use]
pub fn stage(first: u16, percent: i32, has: impl Fn(u16) -> bool) -> Option<u16> {
    let n = (0..STAGES).take_while(|i| has(first + i)).count();
    let n = i32::try_from(n).unwrap_or(0);

    (0..n)
        .find(|i| i * WHOLE / n <= percent && percent < (i + 1) * WHOLE / n)
        .and_then(|i| u16::try_from(i).ok())
        .map(|i| first + i)
}

/// Hit points built, percent: `HP * 100 / MaxHP`; an upgrade's from its
/// former stage's `MaxHP`, what it held, up (none held counts as 0).
fn built_percent(c: &Condition) -> i32 {
    let span = c.max - c.base;

    if span == 0 {
        0
    } else {
        (c.hp - c.base).max(0) * WHOLE / span
    }
}

/// Hit points lost, percent: `(MaxHP - HP) * 100 / MaxHP`.
const fn lost_percent(c: &Condition) -> i32 {
    if c.max == 0 {
        0
    } else {
        (c.max - c.hp) * WHOLE / c.max
    }
}

/// The lit or unlit set's offset by `CurrentEvent`: 1, 2, 4, 3 give 1, 2,
/// 3, 4; anything else 0.
const fn variant(event: i32) -> u16 {
    match event {
        1 => 1,
        2 => 2,
        4 => 3,
        3 => 4,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_whole_building_is_lit_only_with_someone_inside() {
        let inn = Condition {
            hp: 100,
            max: 100,
            built: true,
            ..Condition::default()
        };
        let all = |_| true;

        assert_eq!(set(&inn, all), Some(INACTIVE));
        assert_eq!(
            set(
                &Condition {
                    occupied: true,
                    ..inn
                },
                all
            ),
            Some(ACTIVE)
        );
        assert_eq!(set(&Condition { force: 2, ..inn }, all), Some(ACTIVE));
        assert_eq!(set(&Condition { spell: true, ..inn }, all), Some(ACTIVE));
        assert_eq!(set(&Condition { event: 4, ..inn }, all), Some(INACTIVE + 3));
        assert_eq!(set(&inn, |s| s != INACTIVE), Some(ACTIVE));
    }

    #[test]
    fn an_upgrade_builds_from_its_former_stages_hit_points() {
        // A Palace2 at 560 of 700, from a Palace1 of 550: 6% built.
        let up = Condition {
            hp: 560,
            max: 700,
            base: 550,
            ..Condition::default()
        };
        let three = |s: u16| (BUILD..BUILD + 3).contains(&s);

        assert_eq!(set(&up, three), Some(BUILD));
        assert_eq!(set(&Condition { hp: 640, ..up }, three), Some(BUILD + 1));
    }
}
