//! A player's statistics: what the palace's STATISTICS
//! page (`AP40`) and the end's (`AP83`, `AP84`) list. **Binary** unless
//! marked.
//!
//! ```text
//!  field      added by
//!  built      a building placed by the player's, SpawnUnit's,
//!             SpawnGraveyard's creation; and AdjustPlayerData's typed spend
//!  destroyed  a ruin's rubble order made (GS_OrderProcessor)
//!  hired      a priced hero made, a recruit's decode, AdjustPlayerData
//!  killed     a hero deleted, APV:28 0 (GSUnit)
//!  danger     a henchman deleted (GSUnit)
//!  vice       IncrementStatCounter's gutter (World::vice)
//!  made       from GS_Player: every add to APP:0 above 0, the setup's
//!             gold too
//!  research   inline in GS_OrderProcessor, the price paid
//!  buildings  built's price, an upgrade, an enchantment, a gamble
//!             (GS_OrderProcessor)
//!  heroes     hired's price
//!  spells     a player's spell cast
//!  flags      a flag made, its reward; every change GSUnit takes for
//!             APR:1
//!  initial    the player setup's gold
//!  peak       GS_Player after each add: the treasury, where above it;
//!             only the high scores read it
//! ```
//!
//! `GSGL` saves the record (`gsgl::Record`), all but the peak; the
//! constructor zeroes that,
//! so a load starts the peak at 0. Not modelled: a
//! gamble, which the world does not play.

use super::World;
use crate::data::attrib::HAS_BEEN_DISMISSED;

/// What the treasury paid for, in the record's order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spend {
    Research,
    Buildings,
    Heroes,
    Spells,
    Flags,
}

/// One player's record.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Ledger {
    pub built: i32,
    pub destroyed: i32,
    pub hired: i32,
    pub killed: i32,
    pub danger: i32,
    /// Gold in, the setup's included.
    pub made: i32,
    /// By [`Spend`].
    pub spent: [i32; 5],
    pub initial: i32,
    /// The most gold the treasury held,.
    pub peak: i32,
}

impl Ledger {
    /// Spent in all.
    #[must_use]
    pub fn total_spent(&self) -> i32 {
        self.spent.iter().sum()
    }
}

impl Ledger {
    /// The save model's.
    #[must_use]
    pub const fn save(&self) -> crate::save::Ledger {
        crate::save::Ledger {
            built: self.built,
            destroyed: self.destroyed,
            hired: self.hired,
            killed: self.killed,
            danger: self.danger,
            made: self.made,
            spent: self.spent,
            initial: self.initial,
            peak: self.peak,
        }
    }

    /// From the save model.
    #[must_use]
    pub const fn load(m: crate::save::Ledger) -> Ledger {
        Ledger {
            built: m.built,
            destroyed: m.destroyed,
            hired: m.hired,
            killed: m.killed,
            danger: m.danger,
            made: m.made,
            spent: m.spent,
            initial: m.initial,
            peak: m.peak,
        }
    }
}

/// One line of the statistics list: its `GMTX` text and its number, none
/// for a heading or a rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Line {
    pub text: usize,
    pub number: Option<i32>,
}

/// The list's length: stats list fill asks end stat row rows 0 to
/// 25.
pub const LINES: usize = 26;

/// The rule between groups, `GMTX` 108: rows 10, 15, 21, 23, 25.
pub const RULE: usize = 108;

impl World {
    /// Player `player`'s record.
    #[must_use]
    pub fn ledger(&self, player: u32) -> Ledger {
        self.players.ledger(player)
    }

    /// The statistics list: row n is
    /// `end_stat_row(n)`, `GMTX` 87 to 108 by a switch, the numbers
    /// the record's, the treasury `APP:0`, and [`World::summary`]'s
    /// standing buildings and living heroes.
    #[must_use]
    pub fn statistics(&self, player: u32) -> [Line; LINES] {
        let l = self.ledger(player);
        let s = self.summary(player);
        let count = |n: usize| i32::try_from(n).unwrap_or(i32::MAX);
        let line = |text, number| Line { text, number };
        let rule = line(RULE, None);

        [
            line(87, None),
            line(88, Some(l.made - l.initial)),
            line(89, Some(s.gold.unwrap_or(0))),
            line(90, Some(l.initial)),
            line(91, Some(l.total_spent())),
            line(92, Some(l.spent[Spend::Research as usize])),
            line(93, Some(l.spent[Spend::Buildings as usize])),
            line(94, Some(l.spent[Spend::Heroes as usize])),
            line(95, Some(l.spent[Spend::Spells as usize])),
            line(96, Some(l.spent[Spend::Flags as usize])),
            rule,
            line(97, None),
            line(98, Some(l.built)),
            line(99, Some(l.destroyed)),
            line(100, Some(count(s.standing))),
            rule,
            line(101, None),
            line(102, Some(l.hired)),
            line(103, Some(l.killed)),
            line(104, Some(count(s.heroes))),
            line(105, Some(s.level)),
            rule,
            line(106, Some(s.vice)),
            rule,
            line(107, Some(l.danger)),
            rule,
        ]
    }

    /// `owner`'s record, to add to.
    pub(super) fn ledger_mut(&mut self, owner: u32) -> &mut Ledger {
        self.players.ledger_mut(owner)
    }

    /// `amount` paid by `owner` for `what`.
    pub(super) fn count_paid(&mut self, owner: u32, what: Spend, amount: i32) {
        self.ledger_mut(owner).spent[what as usize] += amount;
    }

    /// Stat built: one more building, its price.
    pub(super) fn count_built(&mut self, owner: u32, price: i32) {
        self.ledger_mut(owner).built += 1;
        self.count_paid(owner, Spend::Buildings, price);
    }

    /// Stat hired: one more hero, its price.
    pub(super) fn count_hired(&mut self, owner: u32, price: i32) {
        self.ledger_mut(owner).hired += 1;
        self.count_paid(owner, Spend::Heroes, price);
    }

    /// Stats destroyed add: one more ruin.
    pub(super) fn count_destroyed(&mut self, owner: u32) {
        self.ledger_mut(owner).destroyed += 1;
    }

    /// Player setup: the setup's `gold` added to an empty treasury
    /// (`GS_Player`, none for 0), the peak, and kept as the initial.
    pub(super) fn count_initial(&mut self, owner: u32, gold: i32) {
        let l = self.ledger_mut(owner);

        l.made += gold.max(0);
        l.peak = l.peak.max(gold);
        l.initial = gold;
    }

    /// `GSUnit` on piece `id` as it is deleted: a hero not dismissed
    /// (`APV:28` 0) counts as killed, a henchman as danger, for its owner.
    pub(super) fn count_deleted(&mut self, id: u32) {
        let dismissed = HAS_BEEN_DISMISSED.packed();
        let piece = self
            .pieces
            .get(id)
            .map(|p| (p.owner, p.attributes.get(&dismissed).copied().unwrap_or(0)));

        match piece {
            Some((owner, 0)) if self.is_hero(id) => self.ledger_mut(owner).killed += 1,
            Some((owner, _)) if self.henchman(id) => self.ledger_mut(owner).danger += 1,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The peak is the treasury at its highest after an add: the setup's
    /// gold, then each add, never a spend.
    #[test]
    fn the_peak_is_the_most_gold_an_add_left() {
        let mut world = World::default();

        world.count_initial(0, 3_000);
        world.players.set_gold(0, 3_000);
        world.earn(0, 500);
        world.earn(0, -2_000);
        world.earn(0, 100);

        assert_eq!(world.ledger(0).peak, 3_500);
    }

    #[test]
    fn the_list_has_its_headings_and_rules_where_the_switch_puts_them() {
        let rows = World::default().statistics(0);
        let rules: Vec<usize> = (0..LINES).filter(|n| rows[*n].text == RULE).collect();
        let headings: Vec<usize> = (0..LINES)
            .filter(|n| rows[*n].number.is_none() && rows[*n].text != RULE)
            .collect();

        assert_eq!(rules, [10, 15, 21, 23, 25]);
        assert_eq!(headings, [0, 11, 16]);
        assert_eq!(rows[2].text, 89);
    }
}
