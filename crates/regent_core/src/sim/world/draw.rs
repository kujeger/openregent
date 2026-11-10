//! How a unit is drawn tinted or see-through: `SetDrawEffects`,
//! `ClearDrawEffects`, `FadeIn` and `FadeOut`; and whether the minimap
//! shows it, `ShowMinimap`. **Binary** unless marked.
//!
//! ```text
//!  ClearDrawEffects(agent)            no unit: nothing; else the
//!                                     effects are cleared
//!  SetDrawEffects(agent, colour, n)   no unit: nothing; colour "red",
//!                                     "gray", "green" (_stricmp) is 1, 2,
//!                                     3 in the word's top byte; n of 1-9
//!                                     a transparency, bits 8-23, with
//!                                     flag 5; neither: nothing; else
//!                                     the unit takes the word
//! ```
//!
//! No builtin reads them back: they change only the drawing (**Inferred**:
//! no engine path reads them back). `GetProperUnitArt` sets them for
//! invisibility and the like.
//!
//! ```text
//!  FadeIn(agent, ms)    GE_OrderProcessor (unit, 0, ms):
//!  FadeOut(agent, ms)   (unit, 10, ms): an OrderFade on track
//!                                   1; no unit, nothing
//!  OrderFade       each run the level a step toward its
//!                                   target, mode 5; at it the order ends,
//!                                   and an end at 0 clears the effects
//!  ShowMinimap(agent, b)  the unit's minimap bit: clear for
//!                                   true, set for false; no unit, throws
//! ```
//!
//! The minimap bit is set as a unit takes a descriptor with `Info`
//! `NotVisibleInOverheadView`, the minimap's view.

use super::{Answer, Found, World, agent_ref, bool_ref, int_ref, string_ref};
use crate::gpl::vm::{Fault, Handle, Vm};

/// The colours `SetDrawEffects` takes, in its order: 1, 2, 3.
pub const COLOURS: [&str; 3] = ["red", "gray", "green"];

/// The transparencies it takes, inclusive.
pub const TRANSPARENCY: (i32, i32) = (1, 9);

/// The level `FadeOut` fades to (`0x0a`, gpl fade out); `FadeIn`'s
/// is 0.
pub const FADED: u8 = 10;

/// Which way a fade goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fading {
    In,
    Out,
}

/// Whether the minimap shows a unit: unit state bit `0x400` clear.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Overhead {
    #[default]
    Shown,
    Hidden,
}

/// A unit's draw effects: its colour, 1-3 of [`COLOURS`], and its
/// transparency, 1-9; each `None` where not set.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Tint {
    pub colour: Option<u8>,
    pub transparency: Option<u8>,
}

impl World {
    /// `ClearDrawEffects(agent)`.
    pub(super) fn clear_draw(&mut self, vm: &mut Vm<'_>, args: &[Handle]) -> Result<Answer, Fault> {
        let id = agent_ref(vm, args, 1)?;

        match self.found(id) {
            Found::Piece(_) => {
                if let Some(u) = self.units.get_mut(id) {
                    u.tint = None;
                }

                Ok(Answer::Given)
            }
            Found::Gone => Ok(Answer::Given),
            Found::Unsure => Ok(Answer::Unknown),
        }
    }

    /// `SetDrawEffects(agent, colour, transparency)`.
    pub(super) fn set_draw(&mut self, vm: &mut Vm<'_>, args: &[Handle]) -> Result<Answer, Fault> {
        let id = agent_ref(vm, args, 1)?;
        let colour = string_ref(vm, args, 2)?;
        let level = int_ref(vm, args, 3)?;
        let tint = tint(&colour, level);

        match (self.found(id), tint) {
            (Found::Piece(_), Some(t)) => {
                self.units.entry(id).tint = Some(t);
                Ok(Answer::Given)
            }
            (Found::Unsure, _) => Ok(Answer::Unknown),
            _ => Ok(Answer::Given),
        }
    }

    /// The draw effects of agent `id`'s unit, where any are set.
    #[must_use]
    pub fn tint(&self, id: u32) -> Option<Tint> {
        self.units.get(id).and_then(|u| u.tint)
    }

    /// Every unit's draw effects, by agent.
    pub fn tints(&self) -> impl Iterator<Item = (u32, Tint)> + '_ {
        self.units
            .iter()
            .filter_map(|(id, u)| u.tint.map(|t| (id, t)))
    }

    /// `FadeIn(agent, ms)` or `FadeOut(agent, ms)`: an `OrderFade` to 0
    /// or [`FADED`], a step each `ms` ([`World::fade_push`]). No unit:
    /// nothing, the native asks none.
    pub(super) fn fade(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
        way: Fading,
    ) -> Result<Answer, Fault> {
        let id = agent_ref(vm, args, 1)?;
        let rate = int_ref(vm, args, 2)?.cast_unsigned();
        let target = match way {
            Fading::In => 0,
            Fading::Out => FADED,
        };

        match self.found(id) {
            Found::Piece(_) => {
                self.fade_push(id, target, rate);
                Ok(Answer::Given)
            }
            Found::Gone => Ok(Answer::Given),
            Found::Unsure => Ok(Answer::Unknown),
        }
    }

    /// `ShowMinimap(agent, shown)`: throws with no unit.
    pub(super) fn show_minimap(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> Result<Answer, Fault> {
        let id = agent_ref(vm, args, 1)?;
        let shown = if bool_ref(vm, args, 2)? {
            Overhead::Shown
        } else {
            Overhead::Hidden
        };

        match self.found(id) {
            Found::Piece(_) => {
                self.units.entry(id).overhead = Some(shown);
                Ok(Answer::Given)
            }
            Found::Gone => Err(Fault::throw(
                "ShowMinimap() tried to change visibility of a non-existent game-object",
            )),
            Found::Unsure => Ok(Answer::Unknown),
        }
    }

    /// Whether the minimap shows agent `id`'s unit: as `ShowMinimap` last
    /// set it, else as its descriptor makes it.
    #[must_use]
    pub fn overhead(&self, id: u32) -> Overhead {
        let made = || self.body_of(id).map_or(Overhead::Shown, |b| b.overhead);

        self.units
            .get(id)
            .and_then(|u| u.overhead)
            .unwrap_or_else(made)
    }
}

/// The effects a `SetDrawEffects` call sets: `None` where it names neither
/// a colour nor a transparency, which sets nothing.
fn tint(colour: &str, level: i32) -> Option<Tint> {
    let colour = COLOURS
        .iter()
        .position(|c| c.eq_ignore_ascii_case(colour))
        .and_then(|i| u8::try_from(i + 1).ok());
    let (min, max) = TRANSPARENCY;
    let transparency = (min..=max)
        .contains(&level)
        .then(|| u8::try_from(level).ok())
        .flatten();

    (colour.is_some() || transparency.is_some()).then_some(Tint {
        colour,
        transparency,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A colour or a transparency in range sets the effects; neither sets
    /// nothing: `"gray", 0` sets only the colour.
    #[test]
    fn set_draw_effects_takes_a_colour_and_a_transparency() {
        assert_eq!(
            tint("Gray", 0),
            Some(Tint {
                colour: Some(2),
                transparency: None
            })
        );
        assert_eq!(
            tint("blue", 5),
            Some(Tint {
                colour: None,
                transparency: Some(5)
            })
        );
        assert_eq!(tint("blue", 10), None);
    }
}
