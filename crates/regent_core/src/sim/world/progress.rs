//! A building panel's progress bar while it recruits or researches.
//! **Binary** unless marked.
//!
//! ```text
//!  a guild       Base_GuildController
//!    APB:1 and APB:13 set:
//!    8022  bar   message 0x29 {7, now - APB:56, 0, APB:13}   Base_GuildController
//!    8023  text  APB:12 spelled with APB:14's NameGenType     Base_GuildController
//!  a researcher  research panel state
//!    APB:23, APB:2, APB:13 and APB:44 set:
//!    4500  bar   message 0x29 {7, now - APB:56, 0, APB:13}   research panel state
//!    4501  text  GMTX 215 "Researching %s", the row's GMTX   research panel state
//! ```
//!
//! `now` is game clock; `APB:56` holds the clock at the decode,
//! `APB:13` the order's time, so the bar fills as the order's GS timer
//! runs and is full as it ends. Message `0x29`'s words are a mask, the
//! position, the minimum and the maximum: the day bar sends mask 1 and its
//! position alone. While the order's message waits
//! for its decode (unit queued messages bit 2 for a recruit, bit 1 for a
//! research) the bar shows empty and the text is hidden; the world decodes
//! within the frame, so this is not modelled.
//!
//! The researchers are [`super::research::BY_BUILDING`]'s codes: their
//! controllers derive `Base_ResearchedBuildingController`, the only caller
//! of research panel state; no guild researches (**Inferred**: the
//! controller from the class names, Blacksmith to Trading Post).

use super::research::{self, CURRENT, NOT_COMPLETED};
use super::{BUTTON_DELAY_TIME, World, naming::spelled};
use crate::data::attrib::{
    BUTTON_START_TIME, CURRENT_STAGE_BUILT, FIRST_STAGE_BUILT, PENDING_UNIT_NAME, RECRUIT_TYPE,
};
use crate::data::fourcc::FourCc;
use crate::data::text::{GMTX, Text};

/// The research text, `Researching %s`.
pub const RESEARCHING_TEXT: (&[u8; 4], usize) = (GMTX, 215);

/// What runs, and what names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A hero to come: its name, `APB:12`, and the generator spelling it,
    /// its descriptor's `NameGenType`.
    Recruit {
        generator: Option<FourCc>,
        name: u32,
    },
    /// A research: its id and its row's `GMTX` text.
    Research { id: u32, text: u32 },
}

/// A progress bar: `elapsed` of `time` game ms.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Progress {
    pub kind: Kind,
    pub elapsed: i32,
    pub time: i32,
}

impl Progress {
    /// How full, 0 to 1. **Inferred**: the bar clamps to its range; the
    /// timer ends within a frame of full.
    #[must_use]
    pub fn filled(&self) -> f32 {
        #[allow(clippy::cast_precision_loss, reason = "game ms are small")]
        let part = self.elapsed as f32 / self.time as f32;

        if self.time > 0 {
            part.clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

    /// The text over the bar: the hero's name, or `Researching` the
    /// research's name. `None` where `text` lacks it.
    #[must_use]
    pub fn text(&self, text: &Text) -> Option<String> {
        match self.kind {
            Kind::Recruit { generator, name } => generator
                .map(|g| spelled(text, g, name))
                .filter(|_| name != 0),
            Kind::Research { text: id, .. } => {
                let (table, index) = RESEARCHING_TEXT;
                let name = text.gmtx(usize::try_from(id).ok()?)?;

                Some(text.at(table, index)?.replacen("%s", &name, 1))
            }
        }
    }
}

impl World {
    /// The progress bar agent `id`'s panel shows: a guild's recruit, a
    /// researcher's research; `None` while neither runs.
    #[must_use]
    pub fn progress(&self, id: u32) -> Option<Progress> {
        let piece = self.pieces.get(id)?;
        let word = |k| piece.word(k);
        let researcher = research::researcher(piece.code.0);
        let time = word(BUTTON_DELAY_TIME);
        let elapsed = self.now.cast_signed().wrapping_sub(word(BUTTON_START_TIME));

        let kind = if researcher {
            let running = [NOT_COMPLETED, CURRENT_STAGE_BUILT, CURRENT]
                .iter()
                .all(|k| word(*k) != 0);

            running
                .then(|| research::of(word(CURRENT).cast_unsigned()))
                .flatten()
                .map(|item| Kind::Research {
                    id: item.id,
                    text: item.text,
                })
        } else {
            let code = FourCc::from_word(word(RECRUIT_TYPE).cast_unsigned());

            // A guild shows no recruit before its first stage.
            (word(FIRST_STAGE_BUILT) != 0).then(|| Kind::Recruit {
                generator: self.descriptors.body(code).and_then(|b| b.names),
                name: word(PENDING_UNIT_NAME).cast_unsigned(),
            })
        };

        kind.filter(|_| time != 0).map(|kind| Progress {
            kind,
            elapsed,
            time,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bar_is_as_full_as_its_timer() {
        let at = |elapsed, time| {
            Progress {
                kind: Kind::Research { id: 5000, text: 0 },
                elapsed,
                time,
            }
            .filled()
        };

        assert!((at(7_000, 14_000) - 0.5).abs() < f32::EPSILON);
        assert!((at(15_000, 14_000) - 1.0).abs() < f32::EPSILON);
        assert!(at(5, 0).abs() < f32::EPSILON);
    }
}
