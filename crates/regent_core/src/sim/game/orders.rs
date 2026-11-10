//! The interface controllers' orders: what a press or a click sends, in
//! the order the engine decodes it, and the controller state a save
//! keeps. A front end asks these rather than build [`Input`]s itself.
//!
//! ```text
//!  build       Unpreview, then Build
//!  spell       Cast at the point or Bless on the target;
//!              the spell stays aimed but for the Abode's six; the spot
//!              taken, a short treasury says GMTX 34 and sends nothing
//!  flag        explore at the clamped point; attack on a flaggable unit
//!  raise       a reward flag's Alter by #RewardDelta (Reward2Controller)
//!  call/rage   short: GMTX 37 or 36, nothing sent; else its price, then
//!              the action (Game::apply decodes the price last)
//!  camp move   CampBegin pays as it begins, the price held per player;
//!              Camp keeps it; CampLeave gives it back (camp move place)
//!  crypt       a row's price by Mausoleum_Resurrect_Cost, ready if afforded
//!  bounty      each term's price by HallOfChampions_Bounty_Cost, ready if
//!              afforded (bounty buttons state)
//! ```

use super::{Game, Input};
use crate::data::fourcc::FourCc;
use crate::gpl::exec::Literal;
use crate::gpl::load::Program;
use crate::gpl::ops::Value;
use crate::sim::world::{
    World,
    alter::Alter,
    build::BuildOrder,
    chat,
    flag::{Flag, REWARD_COST, REWARD_DELTA},
    guild::{
        assembly::NO_GOLD_ASSEMBLY,
        bounty::{BOUNTY_COST, Term},
        camp::{Flow, Resource},
        mausoleum::RESURRECT_COST,
        rage::NO_GOLD_RAGE,
    },
    offer::{ATTACK_FLAG, Aim, Button, EXPLORE_FLAG, REWARD_INITIAL, SpellOffer},
    spell::cast::{ABODE_SPELLS, Bless, Cast},
};

/// A placed building: the preview's
/// `UnitDelete`, then `CREATE_BUILDING_MESSAGE` at its half cell.
#[must_use]
pub fn build(order: BuildOrder) -> Vec<Input> {
    vec![Input::Unpreview(order.code), Input::Build(order)]
}

/// Whether `spell` stays aimed after a click sent it: all but the
/// Abode's six ([`ABODE_SPELLS`]), which go back to select (**Binary**,
/// spell click).
#[must_use]
pub fn keeps_mode(spell: FourCc) -> bool {
    !ABODE_SPELLS.iter().any(|(code, _)| FourCc(*code) == spell)
}

/// A left click at `at`, 32nds, with `owner`'s `spell` aimed:
/// a point spell's unit where the cursor takes it
/// ([`World::spell_point`]); a unit spell on the piece `under` picks,
/// if it takes it ([`World::spell_target`]). Then the treasury: short of
/// the price, nothing sent ([`World::spell_short`]).
///
/// # Errors
///
/// What a refused click says, if anything: a refused spot's
/// ([`World::spell_refusal`]) before a short treasury's. A spell cast
/// from its button ([`cast_now`]) says nothing.
pub fn cast(
    (world, program): (&World, &Program),
    owner: u32,
    spell: &SpellOffer,
    at: (i32, i32),
    under: impl FnOnce() -> Option<u32>,
) -> Result<Input, Option<chat::Line>> {
    let code = spell.row.spell;
    let input = match spell.aim {
        Aim::Point(unit) => world
            .spell_point(program, owner, code, at)
            .then_some(Input::Cast(Cast {
                code: unit,
                at,
                owner,
            })),
        Aim::Unit => under()
            .filter(|t| world.spell_target(program, owner, code, *t))
            .map(|target| {
                Input::Bless(Bless {
                    spell: code,
                    target,
                    owner,
                    at,
                })
            }),
        // SuperCharge, no reach spell: refused, it says nothing.
        Aim::Now => None,
    };

    match (input, world.spell_short(owner, spell.price)) {
        (Some(input), None) => Ok(input),
        (Some(_), short) => Err(short),
        (None, _) => Err(world.spell_refusal(program, owner, code, at)),
    }
}

/// A spell cast from its button, aimed at nothing (`SuperCharge`): no
/// target, no point.
#[must_use]
pub const fn cast_now(owner: u32, spell: FourCc) -> Input {
    Input::Bless(Bless {
        spell,
        target: 0,
        owner,
        at: (0, 0),
    })
}

/// `owner`'s explore flag for a click at `at`, 32nds, kept on the map
/// ([`World::explore_point`]), at the panel's `reward`.
#[must_use]
pub fn explore_flag(world: &World, owner: u32, at: (i32, i32), reward: i32) -> Input {
    Input::Flag(Flag {
        code: EXPLORE_FLAG,
        at: world.explore_point(at),
        owner,
        reward,
        target: 0,
    })
}

/// `owner`'s attack flag on `target`, standing at `at`, 32nds, at the
/// panel's `reward`, if it is flaggable ([`World::flaggable`]).
#[must_use]
pub fn attack_flag(
    world: &World,
    owner: u32,
    (target, at): (u32, (i32, i32)),
    reward: i32,
) -> Option<Input> {
    world
        .flaggable(owner, target, reward)
        .then_some(Input::Flag(Flag {
            code: ATTACK_FLAG,
            at,
            owner,
            reward,
            target,
        }))
}

/// The reward flag panel's raise on flag `id` (`Reward2Controller`): its
/// `RewardCost` up by `#RewardDelta`.
#[must_use]
pub const fn raise_reward(id: u32) -> Input {
    Input::Alter(Alter {
        frame: 0,
        agent: id,
        key: REWARD_COST,
        change: REWARD_DELTA,
        range: (0, 0),
    })
}

/// The warriors' guild's Call to Arms at `price`:
/// `owner` short, `GMTX` 37 and nothing sent; else its price, then the
/// call.
///
/// # Errors
///
/// The chat line of a press the treasury cannot pay.
pub fn assemble(
    world: &World,
    owner: u32,
    guild: u32,
    price: i32,
) -> Result<Vec<Input>, chat::Line> {
    paid(
        world,
        owner,
        Input::Assemble(guild),
        price,
        NO_GOLD_ASSEMBLY,
    )
}

/// The Temple to Krolm's Rage at `price`:
/// `owner` short, `GMTX` 36 and nothing sent; else its price, then the
/// Rage.
///
/// # Errors
///
/// The chat line of a press the treasury cannot pay.
pub fn rage(world: &World, owner: u32, temple: u32, price: i32) -> Result<Vec<Input>, chat::Line> {
    paid(world, owner, Input::Rage(temple), price, NO_GOLD_RAGE)
}

/// `price` spent, then `action`, as the controller sends them, where
/// `owner` affords it; else the chat line `short`. The price decodes
/// after the action (`Game::inputs`).
fn paid(
    world: &World,
    owner: u32,
    action: Input,
    price: i32,
    short: usize,
) -> Result<Vec<Input>, chat::Line> {
    if world.affords(owner, price) {
        Ok(vec![spend(owner, price), action])
    } else {
        Err(chat::Line::Text(short))
    }
}

/// `owner` spends `amount` (`PlayerResourceMessage`, spend).
const fn spend(owner: u32, amount: i32) -> Input {
    Input::Resource(Resource {
        owner,
        amount,
        flow: Flow::Spend,
    })
}

/// `#RewardInitial`: both reward panel settings at a new game; 0 where
/// the program lacks it.
#[must_use]
pub fn reward_initial(program: &Program) -> i32 {
    match program.constant(REWARD_INITIAL) {
        Some(Literal::Int(v)) => v,
        _ => 0,
    }
}

impl Game<'_> {
    /// What `owner`'s Mausoleum `id` offers to raise: each interred hero
    /// priced by `Mausoleum_Resurrect_Cost` ([`Game::resurrect_price`]),
    /// ready where `owner` affords it. A hero
    /// the run gives no price is left out.
    pub fn crypt(&mut self, owner: u32, id: u32) -> Vec<Button<u32>> {
        let heroes = self.world().mausoleum_offer(owner, id);
        // One aside for the rows: each run's keep copies the game. None
        // for no rows, which any piece but a Mausoleum has.
        let priced: Vec<(u32, Option<i32>)> = if heroes.is_empty() {
            Vec::new()
        } else {
            self.aside(|game| {
                heroes
                    .into_iter()
                    .map(|hero| {
                        let price = game.interface_int(RESURRECT_COST, &[Value::Agent(hero)]);

                        (hero, price)
                    })
                    .collect()
            })
        };

        priced
            .into_iter()
            .filter_map(|(hero, price)| {
                let price = price?;

                Some(Button {
                    what: hero,
                    price,
                    ready: self.world().affords(owner, price),
                })
            })
            .collect()
    }

    /// The Hall of Champions' two bounties, short then long, each priced
    /// ([`Game::bounty_price`]) and ready where `owner` affords it.
    /// None where the run gives no price.
    pub fn bounties(&mut self, owner: u32) -> [Option<Button<Term>>; 2] {
        let prices = self.aside(|game| {
            Term::ALL.map(|term| game.interface_int(BOUNTY_COST, &[Value::Integer(term.index())]))
        });

        std::array::from_fn(|i| {
            prices[i].map(|price| Button {
                what: Term::ALL[i],
                price,
                ready: self.world().affords(owner, price),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpl::vm::{Config, Vm};
    use crate::sim::world::{
        CROWN, Piece,
        guild::camp::{Camp, CampMove},
        offer::{SPELLS, aim},
    };

    /// `world` with the crown's rangers' guilds 4 and 5.
    fn with_rangers(mut world: World) -> World {
        for id in [4, 5] {
            let piece = Piece {
                owner: CROWN,
                code: FourCc(*b"ABW1"),
                ..Piece::default()
            };

            world.add_test_piece(id, piece);
        }

        world
    }

    /// The treasury short of `price`.
    fn short_of(price: i32) -> World {
        let mut world = World::default();

        world.set_treasury(CROWN, price - 1);
        world
    }

    #[test]
    fn a_placed_building_deletes_its_preview_first() {
        let order = BuildOrder {
            code: FourCc::new(b"ABC1"),
            at: (320, 480),
            owner: CROWN,
        };

        assert_eq!(
            build(order),
            [Input::Unpreview(order.code), Input::Build(order)]
        );
    }

    /// A cast keeps its spell aimed, but for the Abode's six.
    #[test]
    fn a_spell_stays_aimed_after_a_cast_but_the_abodes() {
        assert!(keeps_mode(FourCc::new(b"Sp10")));
        assert!(!keeps_mode(FourCc::new(b"Sp33")));
        assert!(ABODE_SPELLS.iter().all(|(c, _)| !keeps_mode(FourCc(*c))));
    }

    /// A button's spell sends nothing from a click.
    #[test]
    fn a_click_casts_no_spell_of_its_button() {
        let program = Program::load::<&[u8]>(&[]).expect("empty program");
        let world = World::default();
        let now = SPELLS
            .iter()
            .find(|r| aim(r.spell) == Aim::Now)
            .expect("a spell cast at once");
        let offer = SpellOffer {
            row: *now,
            price: 0,
            aim: Aim::Now,
            ready: true,
        };

        assert_eq!(
            cast((&world, &program), CROWN, &offer, (0, 0), || Some(1)),
            Err(None)
        );
        assert_eq!(
            cast_now(CROWN, now.spell),
            Input::Bless(Bless {
                spell: now.spell,
                target: 0,
                owner: CROWN,
                at: (0, 0),
            })
        );
    }

    /// The Call to Arms and the Rage: the price, then the action; short,
    /// their chat lines and nothing sent.
    #[test]
    fn a_call_to_arms_sends_its_price_then_the_call() {
        let world = World::default();

        assert_eq!(
            assemble(&world, CROWN, 2, 500),
            Ok(vec![spend(CROWN, 500), Input::Assemble(2)])
        );
        assert_eq!(
            rage(&world, CROWN, 28, 1500),
            Ok(vec![spend(CROWN, 1500), Input::Rage(28)])
        );
        assert_eq!(
            assemble(&short_of(500), CROWN, 2, 500),
            Err(chat::Line::Text(NO_GOLD_ASSEMBLY))
        );
        assert_eq!(
            rage(&short_of(1500), CROWN, 28, 1500),
            Err(chat::Line::Text(NO_GOLD_RAGE))
        );
    }

    /// The raise: `RewardCost` up by `#RewardDelta`.
    #[test]
    fn a_reward_raise_alters_its_cost_by_the_delta() {
        assert_eq!(
            raise_reward(9),
            Input::Alter(Alter {
                frame: 0,
                agent: 9,
                key: REWARD_COST,
                change: REWARD_DELTA,
                range: (0, 0),
            })
        );
    }

    /// A camp move pays as it begins; sent, it keeps the price; left,
    /// the price comes back, once; a second begun leaves the first.
    #[test]
    fn a_camp_move_pays_first_and_refunds_when_left() {
        const GOLD: i32 = 1000;

        let program = Program::load::<&[u8]>(&[]).expect("empty program");
        let mut world = World::default();

        world.set_treasury(CROWN, GOLD);

        let vm = Vm::new(&program, Config::default());
        let mut game = Game::free(vm, 1, with_rangers(world));
        let held = |game: &Game<'_>| game.world().camp_held(CROWN);
        let camp = Camp {
            guild: 4,
            at: (64, 96),
        };

        game.apply(Input::CampBegin(4)).expect("decoded");

        assert_eq!(held(&game), Some(CampMove { guild: 4, price: 0 }));

        game.apply(Input::Camp(camp)).expect("decoded");
        game.apply(Input::CampLeave).expect("decoded");

        assert_eq!(held(&game), None);

        game.apply(Input::CampBegin(4)).expect("decoded");
        game.apply(Input::CampBegin(5)).expect("decoded");

        assert_eq!(held(&game).map(|m| m.guild), Some(5));

        game.apply(Input::CampLeave).expect("decoded");
        game.apply(Input::CampLeave).expect("decoded");

        assert_eq!(held(&game), None);
        assert_eq!(game.world().treasury(CROWN), Some(GOLD));
    }

    /// The engine's refund is the price kept, not the price taken: a
    /// spend the treasury was short of took nothing, yet the cancel pays
    /// the price back.
    #[test]
    fn a_camp_move_left_refunds_its_price_though_the_spend_was_short() {
        const PRICE: i32 = 300;

        let mut world = short_of(PRICE);
        let held = CampMove {
            guild: 4,
            price: PRICE,
        };

        world.hold_camp(CROWN, held);

        let program = Program::load::<&[u8]>(&[]).expect("empty program");
        let vm = Vm::new(&program, Config::default());
        let mut game = Game::free(vm, 1, world);

        game.apply(Input::CampLeave).expect("decoded");

        assert_eq!(game.world().treasury(CROWN), Some(2 * PRICE - 1));
    }

    /// A camp move begun survives a snapshot: the world holds it.
    #[test]
    fn a_camp_move_begun_is_saved() {
        let program = Program::load::<&[u8]>(&[]).expect("empty program");
        let vm = Vm::new(&program, Config::default());
        let mut game = Game::free(vm, 1, with_rangers(World::default()));

        game.apply(Input::CampBegin(4)).expect("decoded");

        let snapshot = game.snapshot().expect("a snapshot");
        let vm = Vm::new(&program, Config::default());
        let resumed = Game::resumed(vm, with_rangers(World::default()), snapshot).expect("resumed");

        assert!(resumed.world().camp_held(CROWN).is_some());
        assert_eq!(
            resumed.world().camp_held(CROWN),
            game.world().camp_held(CROWN)
        );
    }
}
