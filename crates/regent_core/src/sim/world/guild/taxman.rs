//! A tax collector's panel (`AP74`): its return and pickup amounts.
//! **Binary** unless marked.
//!
//! ```text
//!  TaxCollectorController,      buttons 0xb–0xe:
//!    0xb return +   v < max return, v + step <= max return
//!    0xc return −   0 < v, v − step >= 0
//!    0xd pickup −   0 < v, v − step >= 0
//!    0xe pickup +   v < max pickup, v + step <= max pickup
//!  each alter_delta_send(key, ±step, range (0, max return))
//!  step, maxima: taxman_constants, from GPL on first use
//! ```
//!
//! `collect_tax` reads both: home once the collector carries more than
//! `ReturnAmount`, and only a building whose coffer holds `PickupAmount`
//! is visited. The pickup's range is the return's maximum too, the
//! engine's slip; both are 1000 in HD.

use crate::data::attrib::{Key, PICKUP_AMOUNT, RETURN_AMOUNT};
use crate::data::fourcc::FourCc;
use crate::gpl::exec::Literal;
use crate::gpl::load::Program;
use crate::sim::world::{World, alter::Alter, interface::Stride};

/// The `DialogID` that opens `TaxCollectorController`, whose control 5000
/// it holds: a unit naming it is a tax collector. **Inferred**: the
/// dialog picks the controller.
pub const TAX_DIALOG: FourCc = FourCc(*b"AP74");

/// The constants `taxman_constants` reads: the step, and
/// each amount's maximum.
pub const TAXMAN_INCREMENT: &str = "#Taxman_Increment";
pub const MAX_RETURN: &str = "#Max_Return_Amount";
pub const MAX_PICKUP: &str = "#Max_Pickup_Amount";

/// The `#Taxman_Increment` steps a Shift press takes: 100 in HD. A
/// departure from the original, whose press takes one.
pub const TAX_STRIDE: usize = 4;

/// One of the panel's two amounts. Its buttons: `Return` raise `0xb`,
/// lower `0xc`; `Pickup` lower `0xd`, raise `0xe`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Amount {
    Return,
    Pickup,
}

/// Which way a button moves it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Lower,
    Raise,
}

impl Amount {
    pub const fn key(self) -> Key {
        match self {
            Amount::Return => RETURN_AMOUNT,
            Amount::Pickup => PICKUP_AMOUNT,
        }
    }
}

/// An integer GPL constant, 0 where none.
fn constant(program: &Program, name: &str) -> i32 {
    match program.constant(name) {
        Some(Literal::Int(v)) => v,
        _ => 0,
    }
}

/// What a Shift press moves at most: the tooltips' amount.
#[must_use]
pub fn tax_far(program: &Program) -> i32 {
    Stride::Far.amount(constant(program, TAXMAN_INCREMENT), TAX_STRIDE)
}

/// The range every press of the collector's panel sends, either
/// amount's: 0 to `#Max_Return_Amount`.
#[must_use]
pub fn tax_range(program: &Program) -> (i32, i32) {
    (0, constant(program, MAX_RETURN))
}

impl World {
    /// Whether piece `id` opens the tax collector's panel: its
    /// descriptor's `DialogID` is [`TAX_DIALOG`].
    #[must_use]
    pub fn taxman(&self, id: u32) -> bool {
        self.body_of(id)
            .is_some_and(|b| b.dialog == Some(TAX_DIALOG))
    }

    /// Tax collector `id`'s `amount`, as its panel shows it.
    #[must_use]
    pub fn tax_amount(&self, id: u32, amount: Amount) -> Option<i32> {
        self.pieces
            .get(id)
            .map(|p| p.get(amount.key()).unwrap_or_default())
    }

    /// The message `TaxCollectorController` sends as `amount`'s
    /// `step` button is pressed on collector `id`, if it sends one; its
    /// frame the caller's.
    #[must_use]
    pub fn tax_press(
        &self,
        program: &Program,
        id: u32,
        amount: Amount,
        step: Step,
    ) -> Option<Alter> {
        self.tax_amount(id, amount)
            .and_then(|v| Self::tax_press_at(program, (id, amount), step, v))
    }

    /// Every message a press sends, `stride` far: each step asked of the
    /// value the last left, stopping at the first that does not fit. E.g.
    /// at 50 under the maximum, far: two. The engine's message is one
    /// increment ([`World::tax_sent`]), so a far press sends several.
    #[must_use]
    pub fn tax_presses(
        &self,
        program: &Program,
        (id, amount): (u32, Amount),
        step: Step,
        stride: Stride,
    ) -> Vec<Alter> {
        let start = self.tax_amount(id, amount).unwrap_or_default();

        (0..stride.steps(TAX_STRIDE))
            .scan(start, |v, _| {
                let sent = Self::tax_press_at(program, (id, amount), step, *v)?;

                *v += sent.change;
                Some(sent)
            })
            .collect()
    }

    /// [`World::tax_press`] with the amount at `v`.
    fn tax_press_at(
        program: &Program,
        (id, amount): (u32, Amount),
        step: Step,
        v: i32,
    ) -> Option<Alter> {
        let increment = constant(program, TAXMAN_INCREMENT);
        let max_return = constant(program, MAX_RETURN);
        let max = match amount {
            Amount::Return => max_return,
            Amount::Pickup => constant(program, MAX_PICKUP),
        };
        let (sends, change) = match step {
            Step::Raise => (v < max && v + increment <= max, increment),
            Step::Lower => (0 < v && v - increment >= 0, -increment),
        };

        sends.then_some(Alter {
            frame: 0,
            agent: id,
            key: amount.key(),
            change,
            range: (0, max_return),
        })
    }

    /// Whether `a` is a message the collector's panel sends
    /// ([`World::tax_press`]): on a collector, either amount's key,
    /// either button's change, and the range every press sends. Not
    /// whether it lands: the decode keeps the range.
    #[must_use]
    pub fn tax_sent(&self, program: &Program, a: &Alter) -> bool {
        let increment = constant(program, TAXMAN_INCREMENT);
        let range = tax_range(program);

        self.taxman(a.agent)
            && [Amount::Return, Amount::Pickup]
                .iter()
                .any(|m| m.key() == a.key)
            && [increment, -increment].contains(&a.change)
            && a.range == range
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::attrib::PRIMARY_STAT;
    use crate::sim::world::bodies::Class;
    use crate::sim::world::kind;
    use crate::sim::world::ruin::tests::{body, standing};

    /// The shipped constants, for a press's steps.
    const CONSTANTS: &str = "expression #Taxman_Increment 25\n\
         expression #Max_Return_Amount 1000\n\
         expression #Max_Pickup_Amount 1000\n";

    /// A collector at `start`, its panel's presses sent and landed in one
    /// tick, each passing `tax_sent`: how many, and the amount after.
    fn pressed(amount: Amount, start: i32, step: Step, stride: Stride) -> (usize, i32) {
        const COLLECTOR: [u8; 4] = *b"ABtc";

        let bytes = crate::gpl::emit::compile_source(CONSTANTS, "test.gpl").expect("compiles");
        let program = Program::load(&[bytes]).expect("loads");
        let mut world = World::default();
        let mut taxing = body(Class::Building, 0, vec![(0, 0)]);
        let mut piece = standing(COLLECTOR, 1, kind::Kind::BUILDING, (0, 0));

        taxing.dialog = Some(TAX_DIALOG);
        piece.attributes.insert(amount.key().packed(), start);
        world.descriptors_mut().add_body(FourCc(COLLECTOR), taxing);
        world.pieces.insert(1, piece);

        let sent = world.tax_presses(&program, (1, amount), step, stride);

        for alter in &sent {
            assert!(world.tax_sent(&program, alter));
            world.alter(alter);
        }

        (sent.len(), world.tax_amount(1, amount).expect("a piece"))
    }

    #[test]
    fn a_far_tax_press_goes_as_far_as_the_limits_allow() {
        use Step::{Lower, Raise};
        use Stride::{Far, One};

        for amount in [Amount::Return, Amount::Pickup] {
            assert_eq!(pressed(amount, 500, Raise, Far), (4, 600));
            assert_eq!(pressed(amount, 500, Lower, Far), (4, 400));
            assert_eq!(pressed(amount, 950, Raise, Far), (2, 1000));
            assert_eq!(pressed(amount, 50, Lower, Far), (2, 0));
            assert_eq!(pressed(amount, 1000, Raise, Far), (0, 1000));
            assert_eq!(pressed(amount, 0, Lower, Far), (0, 0));
            assert_eq!(pressed(amount, 500, Raise, One), (1, 525));
        }
    }

    #[test]
    fn a_far_tax_press_moves_four_increments() {
        let bytes = crate::gpl::emit::compile_source(CONSTANTS, "test.gpl").expect("compiles");
        let program = Program::load(&[bytes]).expect("loads");

        assert_eq!(tax_far(&program), 100);
    }

    /// A collector's steps pass, as either button sends them; another
    /// key, change or piece does not. The empty program's constants are
    /// 0: a step of 0 within (0, 0).
    #[test]
    fn a_tax_step_is_as_the_panel_sends_it() {
        const COLLECTOR: [u8; 4] = *b"ABtc";
        const OTHER: [u8; 4] = *b"ABtd";

        let program = Program::load::<&[u8]>(&[]).expect("empty program");
        let mut world = World::default();
        let mut taxing = body(Class::Building, 0, vec![(0, 0)]);

        taxing.dialog = Some(TAX_DIALOG);
        world.descriptors_mut().add_body(FourCc(COLLECTOR), taxing);
        world
            .descriptors_mut()
            .add_body(FourCc(OTHER), body(Class::Building, 0, vec![(0, 0)]));
        world
            .pieces
            .insert(1, standing(COLLECTOR, 1, kind::Kind::BUILDING, (0, 0)));
        world
            .pieces
            .insert(2, standing(OTHER, 2, kind::Kind::BUILDING, (0, 0)));

        let step = |agent, key, change| Alter {
            frame: 0,
            agent,
            key,
            change,
            range: (0, 0),
        };

        assert!(world.tax_sent(&program, &step(1, RETURN_AMOUNT, 0)));
        assert!(world.tax_sent(&program, &step(1, PICKUP_AMOUNT, 0)));
        assert!(!world.tax_sent(&program, &step(1, PICKUP_AMOUNT, 5)));
        assert!(!world.tax_sent(&program, &step(1, PRIMARY_STAT, 0)));
        assert!(!world.tax_sent(&program, &step(2, RETURN_AMOUNT, 0)));
    }
}
