//! Each order checked against its player before it decodes. Lockstep
//! play takes orders from every machine, and the original's decodes ask
//! no sender ([`super::Order`]), so a forged order would act for another
//! player or make gold. A refused order is dropped, alike on every
//! machine, and kept for the log ([`Game::take_dropped`]).
//!
//! ```text
//!  player       a human seat (World::humans)
//!  owner field  Build, Flag, Cast, Bless, Resource: the player
//!  acted on     the agent or handle: a piece of the player's
//!               (Recruit, Upgrade, Research, Enchant, CampBegin, Camp,
//!               Embassy, Resurrect (Mausoleum and hero), Unflag, Demolish, Toggle, Dismiss,
//!               Clear, Assemble, MarketDay, Extort, Rename, Rage,
//!               Bounty, Gamble, NewBet, Alter, Set)
//!  a kind       a Bless's target: a unit its spell takes
//!               (World::spell_kind), else a script errs on it; an
//!               attack flag's: a unit a flag takes (World::flag_kind)
//!  anyone's     a Bless's or a Flag's target, NOW YOU DIE's and GROW
//!               UP's unit; Unpreview and Read act on nothing owned (a
//!               message flag is no player's)
//!  shapes       Alter: a reward raise or a tax step; Set: a Gambling
//!               Hall's wager or colour, a Fairgrounds' contest; each as
//!               its panel sends it
//!  gold         Resource: a spend, not below 0; Flag's reward and
//!               Resurrect's price not below 0; Camp: the move begun
//!               for its guild; CampLeave gives back only a held price
//!  cheats       the session allows them; a phrase's, and a player's
//!               cheat its sender's
//! ```
//!
//! Not checked: what the decode refuses itself, as a short treasury or a
//! spot that does not fit.

use std::fmt;

use super::{Game, Input, Order};
use crate::data::attrib::CURRENT_EVENT;
use crate::sim::world::cheat::{Cheats, PHRASES, Target};
use crate::sim::world::guild::camp::{Flow, RANGERS_GUILD};
use crate::sim::world::offer::ATTACK_FLAG;
use crate::sim::world::{World, alter::Set};

/// The Fairgrounds' code prefix, whose panel (`AP08`) sends its contest.
pub const FAIRGROUNDS: [u8; 3] = *b"ABD";

/// The contests a Fairgrounds' radios send, `APB:5` `CurrentEvent` 0 to
/// 4: none, archery, melee, magic, combo.
pub const CONTESTS: i32 = 5;

/// Why an order was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// Its player is no human seat.
    Seat,
    /// An owner field names another player.
    Owner,
    /// The agent or handle it acts on is no piece of its player's, or
    /// not of the kind its panel acts on.
    Target,
    /// No panel sends it: an `Alter` or `Set` of another key, change,
    /// range or value.
    Shape,
    /// Gold from nothing: a `Resource` adding, or spending below 0; a
    /// `Flag`'s reward or a `Resurrect`'s price below 0.
    Gold,
    /// A camp placed with no move begun for its guild.
    Camp,
    /// The session bars cheats, no phrase sends it, or it names another
    /// player.
    Cheat,
}

/// An order refused as it came to decode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Dropped {
    /// The frame it came before.
    pub frame: u32,
    pub order: Order,
    pub why: Refusal,
}

impl fmt::Display for Dropped {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "order {} before frame {} dropped: {:?}",
            self.order, self.frame, self.why
        )
    }
}

/// `Ok` where `ok`, else `why`.
const fn unless(ok: bool, why: Refusal) -> Result<(), Refusal> {
    if ok { Ok(()) } else { Err(why) }
}

/// Whether `s` is the Fairgrounds' contest radio's setting.
fn contest_sent(world: &World, s: &Set) -> bool {
    let fair = world
        .piece(s.agent)
        .is_some_and(|p| p.code.0.starts_with(&FAIRGROUNDS));

    fair && s.key == CURRENT_EVENT && (0..CONTESTS).contains(&s.value)
}

impl Game<'_> {
    /// Whether `player` may give `input` now: the module's rules.
    ///
    /// # Errors
    ///
    /// The first rule it breaks.
    pub fn check(&self, player: u32, input: &Input) -> Result<(), Refusal> {
        let world = self.world();
        let own = |id: u32| world.piece(id).is_some_and(|p| p.owner == player);
        let mine = |id: u32| unless(own(id), Refusal::Target);
        let owner = |o: u32| unless(o == player, Refusal::Owner);

        unless(world.humans().contains(&player), Refusal::Seat)?;

        match *input {
            Input::Unpreview(_) | Input::Read(_) | Input::CampLeave => Ok(()),
            Input::Build(o) => owner(o.owner),
            Input::Flag(g) => {
                owner(g.owner)?;
                unless(g.reward >= 0, Refusal::Gold)?;
                unless(
                    g.code != ATTACK_FLAG || world.flag_kind(g.target),
                    Refusal::Target,
                )
            }
            Input::Cast(c) => owner(c.owner),
            Input::Bless(b) => {
                owner(b.owner)?;
                unless(world.spell_kind(b.spell, b.target), Refusal::Target)
            }
            Input::Resource(r) => {
                owner(r.owner)?;
                unless(r.flow == Flow::Spend && r.amount >= 0, Refusal::Gold)
            }
            Input::Recruit(r) => mine(r.guild),
            Input::Research(r) => mine(r.building),
            // The hero's owner pays: the sender's. Not tested interred, as
            // a second click before the first decodes is the panel's too.
            Input::Resurrect(r) => {
                mine(r.mausoleum)?;
                mine(r.hero)?;
                unless(r.price >= 0, Refusal::Gold)
            }
            Input::Toggle(t) => mine(t.building),
            Input::Rename(r) => mine(r.unit),
            Input::Bounty(b) => mine(b.hall),
            Input::Upgrade(id)
            | Input::Enchant(id)
            | Input::Embassy(id)
            | Input::Demolish(id)
            | Input::Dismiss(id)
            | Input::Assemble(id)
            | Input::MarketDay(id)
            | Input::Extort(id)
            | Input::Rage(id)
            | Input::Gamble(id)
            | Input::NewBet(id) => mine(id),
            Input::Unflag(handle) | Input::Clear(handle) => {
                unless(world.by_handle(handle).is_some_and(own), Refusal::Target)
            }
            Input::CampBegin(guild) => {
                let rangers = world
                    .piece(guild)
                    .is_some_and(|p| p.code.0.starts_with(&RANGERS_GUILD));

                unless(own(guild) && rangers, Refusal::Target)
            }
            Input::Camp(c) => {
                let held = world.camp_held(player).map(|m| m.guild);

                mine(c.guild)?;
                unless(held == Some(c.guild), Refusal::Camp)
            }
            Input::Alter(a) => {
                let sent = world.raise_sent(&a) || world.tax_sent(self.vm.program(), &a);

                mine(a.agent)?;
                unless(sent, Refusal::Shape)
            }
            Input::Set(s) => {
                let sent = world.bet_sent(self.vm.program(), &s) || contest_sent(world, &s);

                mine(s.agent)?;
                unless(sent, Refusal::Shape)
            }
            Input::Cheat(c) => {
                let target = PHRASES
                    .iter()
                    .find(|p| p.cheat == c.cheat)
                    .map(|p| p.target);
                let fits = match target {
                    Some(Target::Player) => c.arg == player,
                    Some(Target::Selected | Target::Hero) => true,
                    None => false,
                };

                unless(world.cheats() == Cheats::Allowed && fits, Refusal::Cheat)
            }
        }
    }

    /// The orders refused since the last call, in decode order.
    pub fn take_dropped(&mut self) -> Vec<Dropped> {
        std::mem::take(&mut self.dropped)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::attrib::TOTAL_GOLD;
    use crate::data::fourcc::FourCc;
    use crate::gpl::load::Program;
    use crate::gpl::vm::{Config, Vm};
    use crate::sim::game::orders::cast_now;
    use crate::sim::world::alter::Alter;
    use crate::sim::world::build::BuildOrder;
    use crate::sim::world::cheat::{Cheat, CheatCode};
    use crate::sim::world::flag::Flag;
    use crate::sim::world::flag::{REWARD_COST, REWARD_DELTA};
    use crate::sim::world::guild::camp::{Camp, CampMove, Resource};
    use crate::sim::world::guild::gamble::{AMOUNT_KEY, COLOUR_KEY, GAMBLING_HALL};
    use crate::sim::world::guild::mausoleum::Resurrect;
    use crate::sim::world::offer::EXPLORE_FLAG;
    use crate::sim::world::spell::cast::Bless;
    use crate::sim::world::{CROWN, Piece};

    /// A player no human seat.
    const RIVAL: u32 = 1;

    /// The pieces' agents; each one's handle is [`HANDLE`] past it.
    const MINE: u32 = 1;
    const THEIRS: u32 = 2;
    const RANGERS: u32 = 3;
    const HALL: u32 = 4;
    const FLAG: u32 = 5;
    const FAIR: u32 = 6;
    const HANDLE: u32 = 100;

    /// A code of `prefix`, level 1.
    const fn code(prefix: [u8; 3]) -> [u8; 4] {
        [prefix[0], prefix[1], prefix[2], b'1']
    }

    /// A game on the empty program with the crown's pieces and a rival's.
    fn game<T>(test: impl FnOnce(&mut Game<'_>) -> T) -> T {
        let program = Program::load::<&[u8]>(&[]).expect("empty program");
        let vm = Vm::new(&program, Config::default());
        let mut world = World::default();
        let pieces = [
            (MINE, *b"ABC1", CROWN),
            (THEIRS, *b"ABC1", RIVAL),
            (RANGERS, code(RANGERS_GUILD), CROWN),
            (HALL, code(GAMBLING_HALL), CROWN),
            (FLAG, EXPLORE_FLAG.0, CROWN),
            (FAIR, code(FAIRGROUNDS), CROWN),
        ];

        for (id, code, owner) in pieces {
            let piece = Piece {
                owner,
                code: FourCc(code),
                handle: HANDLE + id,
                ..Piece::default()
            };

            world.add_test_piece(id, piece);
        }

        test(&mut Game::free(vm, 1, world))
    }

    #[test]
    fn a_player_no_human_seat_gives_no_order() {
        game(|game| {
            assert_eq!(
                game.check(RIVAL, &Input::Demolish(THEIRS)),
                Err(Refusal::Seat)
            );
            assert_eq!(game.check(CROWN, &Input::Read(1)), Ok(()));
        });
    }

    /// Build, Flag, Cast, Bless and Resource carry their owner: it must
    /// be the order's player.
    #[test]
    fn an_owner_word_must_be_the_players() {
        game(|game| {
            let build = |owner| {
                Input::Build(BuildOrder {
                    code: FourCc::new(b"ABC1"),
                    at: (0, 0),
                    owner,
                })
            };
            let spend = |owner, amount| {
                Input::Resource(Resource {
                    owner,
                    amount,
                    flow: Flow::Spend,
                })
            };
            let spell = FourCc::new(b"Sp10");

            assert_eq!(game.check(CROWN, &build(CROWN)), Ok(()));
            assert_eq!(game.check(CROWN, &build(RIVAL)), Err(Refusal::Owner));
            assert_eq!(game.check(CROWN, &cast_now(CROWN, spell)), Ok(()));
            assert_eq!(
                game.check(CROWN, &cast_now(RIVAL, spell)),
                Err(Refusal::Owner)
            );
            assert_eq!(game.check(CROWN, &spend(CROWN, 10)), Ok(()));
            assert_eq!(game.check(CROWN, &spend(RIVAL, 10)), Err(Refusal::Owner));
        });
    }

    /// An order acts only on its player's pieces: not a rival's, nor one
    /// that is not there; a handle's piece likewise.
    #[test]
    fn an_order_acts_on_its_players_pieces_only() {
        game(|game| {
            assert_eq!(game.check(CROWN, &Input::Demolish(MINE)), Ok(()));
            assert_eq!(
                game.check(CROWN, &Input::Demolish(THEIRS)),
                Err(Refusal::Target)
            );
            assert_eq!(game.check(CROWN, &Input::Upgrade(99)), Err(Refusal::Target));
            assert_eq!(game.check(CROWN, &Input::Unflag(HANDLE + FLAG)), Ok(()));
            assert_eq!(
                game.check(CROWN, &Input::Clear(HANDLE + THEIRS)),
                Err(Refusal::Target)
            );
        });
    }

    /// A unit spell's target must be of its kind: invisibility on a
    /// building is refused; a spell aimed at nothing asks no target.
    #[test]
    fn a_spell_goes_only_on_a_unit_of_its_kind() {
        game(|game| {
            let bless = |spell: &[u8; 4], target| {
                Input::Bless(Bless {
                    spell: FourCc::new(spell),
                    target,
                    owner: CROWN,
                    at: (0, 0),
                })
            };

            assert_eq!(
                game.check(CROWN, &bless(b"Sp11", MINE)),
                Err(Refusal::Target)
            );
            assert_eq!(game.check(CROWN, &bless(b"Sp15", 0)), Ok(()));
        });
    }

    /// An attack flag's target must be a unit a flag takes: not a piece
    /// of no flaggable kind; an explore flag names none.
    #[test]
    fn an_attack_flag_goes_only_on_a_flaggable_unit() {
        game(|game| {
            let flag = |code, target| {
                Input::Flag(Flag {
                    code,
                    at: (0, 0),
                    owner: CROWN,
                    reward: 0,
                    target,
                })
            };

            assert_eq!(
                game.check(CROWN, &flag(ATTACK_FLAG, FLAG)),
                Err(Refusal::Target)
            );
            assert_eq!(game.check(CROWN, &flag(EXPLORE_FLAG, 0)), Ok(()));
        });
    }

    /// No gold from nothing: a treasury add, or a spend below 0.
    #[test]
    fn a_resource_makes_no_gold() {
        game(|game| {
            let moved = |amount, flow| {
                Input::Resource(Resource {
                    owner: CROWN,
                    amount,
                    flow,
                })
            };

            assert_eq!(game.check(CROWN, &moved(10, Flow::Add)), Err(Refusal::Gold));
            assert_eq!(
                game.check(CROWN, &moved(-10, Flow::Spend)),
                Err(Refusal::Gold)
            );
        });
    }

    /// A flag's reward is what the panel sends: never below 0, which
    /// would pay its owner.
    #[test]
    fn a_flag_reward_makes_no_gold() {
        game(|game| {
            let flag = |reward| {
                Input::Flag(Flag {
                    code: EXPLORE_FLAG,
                    at: (0, 0),
                    owner: CROWN,
                    reward,
                    target: 0,
                })
            };

            assert_eq!(game.check(CROWN, &flag(REWARD_DELTA)), Ok(()));
            assert_eq!(game.check(CROWN, &flag(-50_000)), Err(Refusal::Gold));
            assert_eq!(game.check(CROWN, &flag(i32::MIN)), Err(Refusal::Gold));
        });
    }

    /// A resurrection raises the sender's hero in the sender's
    /// Mausoleum, for a price not below 0.
    #[test]
    fn a_resurrection_raises_an_own_hero_for_a_price() {
        game(|game| {
            let raise = |mausoleum, hero, price| {
                Input::Resurrect(Resurrect {
                    mausoleum,
                    hero,
                    price,
                })
            };

            assert_eq!(game.check(CROWN, &raise(HALL, MINE, 500)), Ok(()));
            assert_eq!(
                game.check(CROWN, &raise(HALL, MINE, -1)),
                Err(Refusal::Gold)
            );
            assert_eq!(
                game.check(CROWN, &raise(HALL, THEIRS, 500)),
                Err(Refusal::Target)
            );
            assert_eq!(
                game.check(CROWN, &raise(THEIRS, MINE, 500)),
                Err(Refusal::Target)
            );
        });
    }

    /// A camp move begins on an own rangers' guild, and is placed only
    /// for the guild begun.
    #[test]
    fn a_camp_is_placed_only_once_begun() {
        game(|game| {
            let camp = |guild| Input::Camp(Camp { guild, at: (0, 0) });
            let held = CampMove {
                guild: RANGERS,
                price: 0,
            };

            assert_eq!(game.check(CROWN, &Input::CampBegin(RANGERS)), Ok(()));
            assert_eq!(
                game.check(CROWN, &Input::CampBegin(MINE)),
                Err(Refusal::Target)
            );
            assert_eq!(game.check(CROWN, &camp(RANGERS)), Err(Refusal::Camp));

            game.world_mut().hold_camp(CROWN, held);

            assert_eq!(game.check(CROWN, &camp(RANGERS)), Ok(()));
        });
    }

    /// An Alter is the reward raise on an own flag; a Set a Gambling
    /// Hall's wager or colour, or a Fairgrounds' contest, in range.
    #[test]
    fn an_attribute_message_is_as_a_panel_sends_it() {
        game(|game| {
            let alter = |agent, change| {
                Input::Alter(Alter {
                    frame: 0,
                    agent,
                    key: REWARD_COST,
                    change,
                    range: (0, 0),
                })
            };
            let set = |agent, key, value| Input::Set(Set { agent, key, value });
            let shape = Err(Refusal::Shape);

            assert_eq!(game.check(CROWN, &alter(FLAG, REWARD_DELTA)), Ok(()));
            assert_eq!(game.check(CROWN, &alter(FLAG, -REWARD_DELTA)), shape);
            assert_eq!(game.check(CROWN, &alter(MINE, REWARD_DELTA)), shape);
            assert_eq!(game.check(CROWN, &set(HALL, COLOUR_KEY, 2)), Ok(()));
            assert_eq!(game.check(CROWN, &set(HALL, COLOUR_KEY, 3)), shape);
            // The empty program's most is 0.
            assert_eq!(game.check(CROWN, &set(HALL, AMOUNT_KEY, 0)), Ok(()));
            assert_eq!(game.check(CROWN, &set(HALL, AMOUNT_KEY, 1)), shape);
            assert_eq!(game.check(CROWN, &set(FAIR, CURRENT_EVENT, 4)), Ok(()));
            assert_eq!(
                game.check(CROWN, &set(FAIR, CURRENT_EVENT, CONTESTS)),
                shape
            );
            assert_eq!(game.check(CROWN, &set(MINE, TOTAL_GOLD, 1)), shape);
        });
    }

    /// A cheat needs the session's leave, a phrase that sends it, and a
    /// player's its sender's; a unit's may be anyone's.
    #[test]
    fn a_cheat_is_allowed_and_its_senders() {
        game(|game| {
            let cheat = |cheat, arg| Input::Cheat(CheatCode { cheat, arg });
            let refused = Err(Refusal::Cheat);

            assert_eq!(game.check(CROWN, &cheat(Cheat::Gold, CROWN)), Ok(()));
            assert_eq!(game.check(CROWN, &cheat(Cheat::Gold, RIVAL)), refused);
            assert_eq!(game.check(CROWN, &cheat(Cheat::Kill, THEIRS)), Ok(()));
            assert_eq!(game.check(CROWN, &cheat(Cheat::Flags, CROWN)), refused);

            game.world_mut().set_cheats(Cheats::Barred);

            assert_eq!(game.check(CROWN, &cheat(Cheat::Gold, CROWN)), refused);
        });
    }

    /// A refused order is dropped and kept for the log; the frame's
    /// others decode.
    #[test]
    fn a_refused_order_is_dropped_and_logged() {
        game(|game| {
            let gold = Input::Cheat(CheatCode {
                cheat: Cheat::Gold,
                arg: CROWN,
            });

            game.world_mut().set_treasury(CROWN, 0);
            game.input(1, Input::Demolish(THEIRS)).expect("on time");
            game.input(1, gold).expect("on time");
            game.frame().expect("frame");

            let dropped = game.take_dropped();

            assert_eq!(game.world().treasury(CROWN), Some(10_000));
            assert_eq!(
                dropped,
                [Dropped {
                    frame: 1,
                    order: Input::Demolish(THEIRS).into(),
                    why: Refusal::Target,
                }]
            );
            assert_eq!(game.take_dropped().len(), 0);
        });
    }
}
