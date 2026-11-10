//! Each player's slain monster types, player slain list: what the Hall of
//! Champions' bounty page (`MX01`) lists. **Binary** unless marked.
//!
//! ```text
//!  GSUnit, a unit deleted: a mobile of
//!    unit class 4 (original_type monster, after hero, henchman
//!    and building) adds 1 to its code in the map of each player in
//!    slots 0 to n - 1, n the players the level holds (GS_Level
//!    counts them by slot): with slots 0 and 7, slot 0 only
//!  GSGL: each player block's code and count pairs (gsgl::Player::pairs)
//!  the bounty list: a row per map node, in its order
//! ```
//!
//! **Inferred**: the map is a `std::map` of the code word, so its order
//! is the code read as a little-endian `u32`, ascending.

use super::{
    World,
    bodies::Class,
    search::{SUBTYPE, TYPE},
};
use crate::data::fourcc::FourCc;
use crate::gpl::ops::Value;

/// The attribute unit class of reads last, `"original_type"`, and the
/// value it asks: class monster.
const ORIGINAL_TYPE: &str = "original_type";
const MONSTER: &str = "monster";

/// What unit class of tells before class 4.
const HERO: &str = "hero";
const HENCHMAN: &str = "henchman";
const BUILDING: &str = "building";

impl World {
    /// Player `player`'s slain monster types and counts, in the map's
    /// order.
    #[must_use]
    pub fn slain(&self, player: u32) -> Vec<(FourCc, i32)> {
        self.players
            .slain(player)
            .map(|m| {
                m.iter()
                    .map(|(&code, &n)| (FourCc::from_word(code), n))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// `GSUnit` on piece `id` as it is deleted: a monster counts
    /// for each player in a slot below the number of players.
    pub(super) fn count_slain(&mut self, id: u32) {
        let code = self
            .pieces
            .get(id)
            .filter(|p| self.descriptors.class(p.code) == Some(Class::Character))
            .map(|p| p.code.word())
            .filter(|_| self.monster(id));

        let count = u32::try_from(self.players.slots().len()).unwrap_or(u32::MAX);
        let slots: Vec<u32> = (0..count).filter(|s| self.players.contains(*s)).collect();

        if let Some(code) = code {
            for player in slots {
                self.players.slay(player, code);
            }
        }
    }

    /// Unit class of 4: agent `id` has a `subtype` and a `type`,
    /// neither hero, henchman nor building, and its `original_type` is
    /// monster.
    pub(super) fn monster(&self, id: u32) -> bool {
        let member = |name: &str| {
            self.agents.get(id).and_then(|a| {
                a.values
                    .iter()
                    .find(|m| m.name.eq_ignore_ascii_case(name))
                    .map(|m| match &m.value {
                        Value::String(s) => s.to_ascii_lowercase(),
                        _ => String::new(),
                    })
            })
        };

        match (member(SUBTYPE), member(TYPE), member(ORIGINAL_TYPE)) {
            (Some(subtype), Some(kind), Some(original)) => {
                subtype != HERO && subtype != HENCHMAN && kind != BUILDING && original == MONSTER
            }
            _ => false,
        }
    }
}
