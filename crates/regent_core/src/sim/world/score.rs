//! What a won game leaves for the high scores: `GS_ScoringSystem`,
//! which the end record's victory calls once a game is
//! won. **Binary** unless marked.
//!
//! ```text
//!  category      value                                   recorded
//!  0 MOST GOLD   the stats' most gold held               always
//!  1 LARGEST     buildings of unit list 0 it owns        always
//!  2 HIGHEST     the champion's level (the stats'        level not 0
//!                best hero), its name and class
//!  3 BEST TIME   the level's timer                       always
//!  4 MOST HEROES the stats' heroes hired                 above 0
//! ```
//!
//! Each takes the best of players 0 to 6 in a network game,
//! else player 0's alone; ties keep the first. The front end's
//! table, what it keeps and shows, is `view::menu::scores`.

use super::{Piece, World};
use crate::data::attrib::{EXPERIENCE_LEVEL, STARTED_WITH_THIS_UNIT};
use crate::sim::world::piece_state;

/// The best hero a player's statistics keep:
/// `GSGL` saves it (`gsgl::Record`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Champion {
    /// Its level; 0, none yet.
    pub level: i32,
    /// The hero's owner: the player's own; 0, none yet. **Corpus**: so in every
    /// save.
    pub owner: u32,
    /// Its name, as panel name gives it.
    pub name: String,
    /// Its descriptor's `HEAD` title, "Warrior".
    pub class: String,
}

/// What [`World::scores`] reads off a won game for one player.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scores {
    /// The most gold the treasury held ([`super::ledger::Ledger::peak`]).
    pub gold: i32,
    /// Its buildings: every one of unit list 0 it owns, ruins and
    /// foundations too. **Inferred**: a ruin stays on the list.
    pub city: i32,
    /// The best hero, where its level is not 0.
    pub champion: Option<Champion>,
    /// The game clock, ms: the level's timer.
    pub time: u32,
    /// Heroes hired.
    pub heroes: i32,
}

impl World {
    /// `player`'s buildings, as `GS_ScoringSystem`'s LARGEST CITY
    /// counts them: unit list 0's whose owner is the player, no
    /// other test.
    #[must_use]
    pub fn city(&self, player: u32) -> i32 {
        let built = |p: &&Piece| p.owner == player && p.is_building();

        i32::try_from(self.pieces.values().filter(built).count()).unwrap_or(i32::MAX)
    }

    /// `player`'s champion as end stat heroes leaves it: the kept
    /// one, unless a standing hero of theirs not among the start's has a
    /// higher level; the first such of the highest, in agent order
    /// (**Inferred**: unit list 1's). `named` gives a hero's name and
    /// class.
    ///
    /// Not kept: the original also keeps the result each time its
    /// statistics show (stats list fill, the end summary), so a
    /// champion seen there and fallen since still counts.
    #[must_use]
    pub fn champion(&self, player: u32, named: &dyn Fn(u32) -> (String, String)) -> Champion {
        let kept = self.players.champion(player).cloned().unwrap_or_default();
        let mut heroes: Vec<(u32, i32)> = self
            .pieces
            .iter()
            .filter(|(id, p)| self.standing_hero(**id, p, player))
            .filter(|(_, p)| p.word(STARTED_WITH_THIS_UNIT) == 0)
            .map(|(id, p)| (*id, p.word(EXPERIENCE_LEVEL)))
            .collect();

        heroes.sort_unstable();

        // A strictly higher level replaces it, so the first of a tie stays.
        heroes.into_iter().fold(kept, |best, (id, level)| {
            if level != 0 && level > best.level {
                let (name, class) = named(id);

                Champion {
                    level,
                    owner: player,
                    name,
                    class,
                }
            } else {
                best
            }
        })
    }

    /// What a won game leaves for the high scores, player `player`'s.
    /// A game asks the local player's: the front end's seat.
    #[must_use]
    pub fn scores(&self, player: u32, named: &dyn Fn(u32) -> (String, String)) -> Scores {
        let champion = Some(self.champion(player, named)).filter(|c| c.level != 0);

        Scores {
            gold: self.ledger(player).peak,
            city: self.city(player),
            champion,
            time: self.now(),
            heroes: self.ledger(player).hired,
        }
    }

    /// Whether piece `id` is a hero of `player`'s standing, as
    /// [`World::summary`] counts its living heroes.
    fn standing_hero(&self, id: u32, piece: &Piece, player: u32) -> bool {
        let up = piece.owner == player
            && piece
                .place
                .is_some_and(|at| at.is_vehicle() && at.state & piece_state::RUINED == 0);

        up && self.is_hero(id)
    }
}
