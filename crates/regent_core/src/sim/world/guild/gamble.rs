//! The Gambling Hall: its wager as its panel shows it, the spin and its
//! result. **Binary** unless marked.
//!
//! ```text
//!  panel APb8, GamblingHallController
//!              APB:49 GambleTime set: the spin; APB:48 GambleClose set:
//!      the result; else the bet (gamble bet state):
//!        shown  1200-1202 colours, 9012 - and 9013 +, 9011 the wager,
//!               3710 SPIN
//!        hidden 8201 bar, 8826, 8501-8504 results, 8827
//!        the wager APB:47, at least #gambling_min_bet, at most
//!          #gambling_max_bet and the treasury; SPIN hidden at 0;
//!          - while the step fits in it, + while it and the step fit in
//!          the treasury; 8834 MAKE NEW BET while the treasury is under
//!          the least bet; the wager sent (AlterAttribute APB:47)
//!        then sends the colour (APB:46); a colour button sends its own
//!         3710 SPIN sends the wager, then GUILD_ACTION 11; 8834 MAKE
//!      NEW BET GUILD_ACTION 12
//!  decode 11  GS_OrderProcessor:       APB:47 above 0: APB:49 the
//!    clock; the owner pays APB:47 (GE_Player), refused: APB:49 0;
//!    paid: counted spent on buildings, then, with no 0x4007 order on
//!    it, a timed order of #gambling_interval, kind 0x4007, callback
//!    0x200b
//!  decode 12  APB:48 0
//!  end   gamble end: ended, with an agent: Gamble(hall, APB:46,
//!    APB:47) run at once; its integer above 0 to the owner's APP:0
//!    (GS_Player), then APB:53 GambleWinnings it, APB:49 0,
//!    APB:48 1
//! ```
//!
//! `Gamble` (`mx_LowLevel.gpl`) draws the winner, sets `APB:50`
//! `GambleWinner` and returns the winnings. **Game**: the gamble probe's
//! eight spins, each decoded ten frames before its `Gamble`.

use crate::data::attrib::Key;
use crate::gpl::exec::Literal;
use crate::gpl::load::Program;
use crate::gpl::ops::Value;
use crate::sim::world::World;
use crate::sim::world::alter::Set;
use crate::sim::world::ledger::Spend;
use crate::sim::world::timed::Timer;

/// What gamble end runs, the string `"Gamble"`.
pub const GAMBLE: &str = "Gamble";

/// The spin's length, read once: 500 ms.
pub const GAMBLE_INTERVAL: &str = "#gambling_interval";

/// The least and most bet and the step, read once:
/// 25, 1000, 25.
pub const MIN_BET: &str = "#gambling_min_bet";
pub const MAX_BET: &str = "#gambling_max_bet";
pub const BET_STEP: &str = "#gambling_bet";

/// `APB:47` `GambleAmount`, `APB:48` `GambleClose`, `APB:49`
/// `GambleTime`, `APB:46` `GambleColor`, `APB:50` `GambleWinner`,
/// `APB:53` `GambleWinnings`.
const AMOUNT: u8 = 47;
const CLOSE: u8 = 48;
const TIME: u8 = 49;
const COLOUR: u8 = 46;
const WINNER: u8 = 50;
const WINNINGS: u8 = 53;

/// The keys the panel sends (`Input::Set`): the wager
/// and the colour.
pub const AMOUNT_KEY: Key = Key::new(BUILDING, AMOUNT);
pub const COLOUR_KEY: Key = Key::new(BUILDING, COLOUR);

/// The building bank.
const BUILDING: [u8; 3] = *b"APB";

/// The Gambling Hall's code prefix, whose panel (`APb8`) sends the bet.
pub const GAMBLING_HALL: [u8; 3] = *b"ABd";

/// The colours a bet may name, `APB:46` 0 to 2: red, blue, gold.
pub const COLOURS: i32 = 3;

/// What the bet shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Wager {
    pub amount: i32,
    /// `APB:46`: 0 red, 1 blue, 2 gold.
    pub colour: i32,
    /// The treasury.
    pub gold: i32,
    pub limits: BetLimits,
}

/// Which of its three states a Gambling Hall's panel shows:
/// the spin while `APB:49` is set, else
/// the result while `APB:48` is, else the bet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Table {
    /// The bet: [`World::wager`].
    Bet,
    /// The spin's bar: ms since `APB:49` of
    /// [`GAMBLE_INTERVAL`].
    Spin { elapsed: i32, interval: i32 },
    /// The result: `APB:50`, 0 red, 1 blue,
    /// 2 gold, 3 the skulls; `APB:53`.
    Result { winner: i32, winnings: i32 },
}

/// The bet's limits: least, most, step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BetLimits {
    pub least: i32,
    pub most: i32,
    pub step: i32,
}

impl BetLimits {
    /// As `program` sets them.
    #[must_use]
    pub fn of(program: &Program) -> BetLimits {
        let read = |name| match program.constant(name) {
            Some(Literal::Int(v)) => v,
            _ => 0,
        };

        BetLimits {
            least: read(MIN_BET),
            most: read(MAX_BET),
            step: read(BET_STEP),
        }
    }
}

impl Wager {
    /// `asked` (`APB:47`) within `limits` and `gold`, the treasury.
    #[must_use]
    pub fn of(asked: i32, colour: i32, gold: i32, limits: BetLimits) -> Wager {
        Wager {
            amount: asked.max(limits.least).min(limits.most).min(gold),
            colour,
            gold,
            limits,
        }
    }

    /// The - button: a step fits in the wager.
    #[must_use]
    pub const fn less(&self) -> bool {
        self.limits.step <= self.amount
    }

    /// The + button: the wager and a step fit in the treasury.
    #[must_use]
    pub const fn more(&self) -> bool {
        self.limits.step + self.amount <= self.gold
    }

    /// SPIN: a wager above 0.
    #[must_use]
    pub const fn spin(&self) -> bool {
        self.amount != 0
    }

    /// MAKE NEW BET: the treasury under the least bet.
    #[must_use]
    pub const fn broke(&self) -> bool {
        self.gold < self.limits.least
    }
}

impl World {
    /// The bet Gambling Hall `id` shows `player`, against its treasury;
    /// `None` while a spin runs or its result shows.
    #[must_use]
    pub fn wager(&self, program: &Program, id: u32, player: u32) -> Option<Wager> {
        let piece = self.pieces.get(id)?;
        let at = |index| {
            piece
                .attributes
                .get(&Key::new(*b"APB", index).packed())
                .copied()
                .unwrap_or(0)
        };
        let gold = self.treasury(player).unwrap_or(0);

        (at(TIME) == 0 && at(CLOSE) == 0)
            .then(|| Wager::of(at(AMOUNT), at(COLOUR), gold, BetLimits::of(program)))
    }

    /// What Gambling Hall `id`'s panel shows; `None` with no such piece.
    #[must_use]
    pub fn table(&self, program: &Program, id: u32) -> Option<Table> {
        let piece = self.pieces.get(id)?;
        let at = |index| piece.word(Key::new(BUILDING, index));
        let begun = at(TIME);

        Some(if begun != 0 {
            Table::Spin {
                elapsed: self.now.cast_signed().wrapping_sub(begun),
                interval: interval(program).cast_signed(),
            }
        } else if at(CLOSE) != 0 {
            Table::Result {
                winner: at(WINNER),
                winnings: at(WINNINGS),
            }
        } else {
            Table::Bet
        })
    }

    /// Whether `s` is a setting the hall's panel sends:
    /// on a
    /// Gambling Hall, the wager within 0 and `#gambling_max_bet` (its
    /// sends keep it so, [`Wager::of`], the treasury at the press its
    /// own), or a colour of [`COLOURS`].
    #[must_use]
    pub fn bet_sent(&self, program: &Program, s: &Set) -> bool {
        let hall = self
            .pieces
            .get(s.agent)
            .is_some_and(|p| p.code.0.starts_with(&GAMBLING_HALL));
        let fits = match s.key {
            AMOUNT_KEY => (0..=BetLimits::of(program).most).contains(&s.value),
            COLOUR_KEY => (0..COLOURS).contains(&s.value),
            _ => false,
        };

        hall && fits
    }

    /// `GUILD_ACTION` 11 on hall `id`, the spin:
    /// with a wager, `APB:49` the clock and the wager paid; refused,
    /// `APB:49` 0; paid, its timer pushed where none runs.
    pub fn spin(&mut self, program: &Program, id: u32) {
        let bet = self
            .pieces
            .get(id)
            .map(|p| (p.owner, p.word(AMOUNT_KEY)))
            .filter(|(_, amount)| *amount > 0);

        if let Some((owner, amount)) = bet {
            let now = self.now.cast_signed();

            self.write(id, Key::new(BUILDING, TIME).packed(), now);

            if self.spend(owner, amount) {
                self.count_paid(owner, Spend::Buildings, amount);

                if !self.timed(id, Timer::Gamble) {
                    self.time(id, Timer::Gamble, interval(program));
                }
            } else {
                self.write(id, Key::new(BUILDING, TIME).packed(), 0);
            }
        }
    }

    /// `GUILD_ACTION` 12 on hall `id`, MAKE NEW BET: `APB:48` 0. No agent
    /// asked.
    pub fn new_bet(&mut self, id: u32) {
        if self.pieces.contains(id) {
            self.write(id, Key::new(BUILDING, CLOSE).packed(), 0);
        }
    }

    /// Callback `0x200b` on hall `id`, ended, in its
    /// ORDR turn: with an agent, [`GAMBLE`] of its colour and wager; the
    /// host hands its result to [`World::gamble_paid`].
    pub(in crate::sim::world) fn gamble_over(&mut self, id: u32) {
        let bet = self
            .pieces
            .get(id)
            .map(|p| (p.word(COLOUR_KEY), p.word(AMOUNT_KEY)));

        if let (Some((colour, amount)), Some(_)) = (bet, self.agent(id)) {
            let args = vec![
                Value::Agent(id),
                Value::Integer(colour),
                Value::Integer(amount),
            ];

            self.dispatch(GAMBLE, args);
        }
    }

    /// After [`GAMBLE`] on hall `id` returned `won`: above 0 it goes to the
    /// owner's treasury; `APB:53` it, `APB:49` 0, `APB:48` 1.
    pub(in crate::sim::world) fn gamble_paid(&mut self, id: u32, won: i32) {
        if let Some(owner) = self.pieces.owner(id) {
            if won > 0 {
                self.earn(owner, won);
            }

            self.write(id, Key::new(BUILDING, WINNINGS).packed(), won);
            self.write(id, Key::new(BUILDING, TIME).packed(), 0);
            self.write(id, Key::new(BUILDING, CLOSE).packed(), 1);
        }
    }
}

/// [`GAMBLE_INTERVAL`], ms.
fn interval(program: &Program) -> u32 {
    match program.constant(GAMBLE_INTERVAL) {
        Some(Literal::Int(v)) => v.cast_unsigned(),
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::world::CROWN;

    const LIMITS: BetLimits = BetLimits {
        least: 25,
        most: 1000,
        step: 25,
    };

    #[test]
    fn a_new_wager_is_the_least_bet() {
        let w = Wager::of(0, 0, 5000, LIMITS);

        assert_eq!(
            (w.amount, w.less(), w.more(), w.spin(), w.broke()),
            (25, true, true, true, false)
        );
    }

    #[test]
    fn the_wager_keeps_within_the_treasury_and_the_most() {
        assert_eq!(Wager::of(2000, 0, 5000, LIMITS).amount, 1000);

        let short = Wager::of(100, 0, 60, LIMITS);

        assert_eq!((short.amount, short.more()), (60, false));

        let broke = Wager::of(0, 0, 0, LIMITS);

        assert_eq!(
            (broke.amount, broke.less(), broke.spin(), broke.broke()),
            (0, false, false, true)
        );
    }

    const HALL: u32 = 10;

    /// A hall of the crown's wagering `amount`, the treasury `gold`.
    fn hall(amount: i32, gold: i32) -> World {
        let mut world = World::default();

        world
            .pieces
            .insert(HALL, crate::sim::world::Piece::default());
        world.write(HALL, Key::new(BUILDING, AMOUNT).packed(), amount);
        world.players.set_gold(CROWN, gold);
        world.now = 33_250;
        world
    }

    fn word(world: &World, index: u8) -> i32 {
        world.pieces[&HALL].word(Key::new(BUILDING, index))
    }

    /// A spin stamps the clock and pays; its result paid, the winnings
    /// go to the treasury, the time is cleared and the result shows; a
    /// new bet clears it.
    #[test]
    fn a_spin_pays_and_its_result_shows_till_a_new_bet() {
        let mut world = hall(225, 29_570);

        world.spin(&Program::default(), HALL);

        assert_eq!(word(&world, TIME), 33_250);
        assert_eq!(world.treasury(CROWN), Some(29_345));
        assert!(world.timed(HALL, Timer::Gamble));

        world.gamble_paid(HALL, 1_125);

        assert_eq!(world.treasury(CROWN), Some(30_470));
        assert_eq!(
            (
                word(&world, WINNINGS),
                word(&world, TIME),
                word(&world, CLOSE)
            ),
            (1_125, 0, 1)
        );

        world.new_bet(HALL);

        assert_eq!(word(&world, CLOSE), 0);
    }

    /// The panel shows the bet, then the spin while it runs, then the
    /// result till a new bet.
    #[test]
    fn the_table_shows_bet_spin_result_then_bet() {
        let program = Program::default();
        let mut world = hall(225, 29_570);

        assert_eq!(world.table(&program, HALL), Some(Table::Bet));

        world.spin(&program, HALL);
        world.now += 200;

        assert_eq!(
            world.table(&program, HALL),
            Some(Table::Spin {
                elapsed: 200,
                interval: 0
            })
        );

        world.write(HALL, Key::new(BUILDING, WINNER).packed(), 1);
        world.gamble_paid(HALL, 1_125);

        assert_eq!(
            world.table(&program, HALL),
            Some(Table::Result {
                winner: 1,
                winnings: 1_125
            })
        );

        world.new_bet(HALL);

        assert_eq!(world.table(&program, HALL), Some(Table::Bet));
        assert_eq!(world.table(&program, HALL + 1), None);
    }

    /// A wager the treasury lacks: the time set, then cleared; no timer.
    #[test]
    fn a_spin_the_treasury_refuses_stops() {
        let mut world = hall(225, 100);

        world.spin(&Program::default(), HALL);

        assert_eq!(word(&world, TIME), 0);
        assert_eq!(world.treasury(CROWN), Some(100));
        assert!(!world.timed(HALL, Timer::Gamble));
    }
}
