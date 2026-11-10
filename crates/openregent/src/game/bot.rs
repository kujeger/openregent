//! A scripted player for headless soak runs (`--bot`): every few seconds
//! it presses what the panels offer, picked by its own seeded stream.
//! Not the original's AI; a test driver, as the local player. Its inputs
//! are recorded as a player's are, so a run that stops replays.
//!
//! ```text
//!  every PERIOD frames, one of:
//!    build      an offered building where it fits near the palace
//!    recruit    a ready hero at one of its guilds
//!    research / upgrade / enchant   a ready button of its building
//!    cast       an offered spell: at a point, or on its unit
//!    flag       explore at a point, or attack a flaggable piece
//!    raise      its flag's reward
//! ```

use regent_core::gpl::load::Program;
use regent_core::sim::game::{Input, orders};
use regent_core::sim::units::PER_HALF;
use regent_core::sim::world::{
    World,
    build::BuildOrder,
    draw_view::Shown,
    flag::Flag,
    offer::{ATTACK_FLAG, Aim, EXPLORE_FLAG, Menu, Placement},
    recruit::Recruit,
    research::Research,
    spell::cast::{Bless, Cast},
};

use crate::game::sim::Sim;

/// Frames between moves: five game seconds.
const PERIOD: u32 = 100;

/// Half cells a side of the square searched for a building's place.
const REACH: i32 = 24;

/// A flag's reward.
const REWARD: i32 = 200;

/// Kinds of move, picked in turn by the stream.
const MOVES: u32 = 8;

/// The scripted player.
pub struct Bot {
    menu: Menu,
    /// Its stream's state: an LCG, apart from the game's.
    state: u32,
}

impl Bot {
    pub const fn new(menu: Menu, seed: u32) -> Bot {
        Bot { menu, state: seed }
    }

    /// A draw below `n`, 0 for none.
    const fn draw(&mut self, n: usize) -> usize {
        self.state = self.state.wrapping_mul(1_103_515_245).wrapping_add(12_345);

        match n {
            0 => 0,
            n => (self.state >> 8) as usize % n,
        }
    }

    /// The inputs for the frame after `sim`'s: a move every [`PERIOD`],
    /// as its local player's.
    pub fn moves(&mut self, sim: &Sim) -> Vec<Input> {
        if sim.frames().is_multiple_of(PERIOD) {
            let kind = self.draw(MOVES as usize);

            self.pick((sim.world(), sim.local()), sim.program(), kind)
        } else {
            Vec::new()
        }
    }

    fn pick(
        &mut self,
        (world, local): (&World, u32),
        program: &Program,
        kind: usize,
    ) -> Vec<Input> {
        let shown = world.shown();
        let mine: Vec<u32> = shown
            .iter()
            .filter(|s| s.owner == local)
            .map(|s| s.agent)
            .collect();
        let palace = world
            .palaces()
            .first()
            .and_then(|id| shown.iter().find(|s| s.agent == *id))
            .map(|s| s.at);
        let Some(home) = palace else {
            return Vec::new();
        };
        let near = |bot: &mut Bot, spread: i32| {
            let side = usize::try_from(spread * 2).unwrap_or(1);
            let d = |b: &mut Bot| i32::try_from(b.draw(side)).unwrap_or(0) - spread;

            ((home.0 + d(bot)).max(0), (home.1 + d(bot)).max(0))
        };

        match kind {
            0 => self.build((world, local), program, home),
            1 => {
                let guild = mine[self.draw(mine.len())..].iter().find_map(|id| {
                    let ready: Vec<_> = world
                        .recruits(&self.menu, *id)
                        .into_iter()
                        .filter(|b| b.ready)
                        .collect();

                    ready.first().map(|b| (*id, b.what))
                });

                guild
                    .map(|(guild, code)| Input::Recruit(Recruit { guild, code }))
                    .into_iter()
                    .collect()
            }
            2 => mine
                .iter()
                .find_map(|id| {
                    let b = world
                        .researches(program, *id)
                        .into_iter()
                        .find(|b| b.ready)?;

                    Some(Input::Research(Research {
                        building: *id,
                        item: b.what.id,
                        frame: 0,
                    }))
                })
                .into_iter()
                .collect(),
            3 => mine
                .iter()
                .find_map(|id| {
                    let upgrade = world.upgrade_offer(*id).filter(|b| b.ready);
                    let enchant = world.enchant_offer(program, *id).filter(|b| b.ready);

                    upgrade
                        .map(|_| Input::Upgrade(*id))
                        .or(enchant.map(|_| Input::Enchant(*id)))
                })
                .into_iter()
                .collect(),
            4 => {
                let at = near(self, 40 * PER_HALF);

                self.cast((world, local), program, &mine, at)
            }
            5 => vec![Input::Flag(Flag {
                code: EXPLORE_FLAG,
                at: near(self, 80 * PER_HALF),
                owner: local,
                reward: REWARD,
                target: 0,
            })],
            6 => self.attack((world, local), &shown),
            _ => mine
                .iter()
                .find(|id| world.raisable(local, **id))
                .map(|id| orders::raise_reward(*id))
                .into_iter()
                .collect(),
        }
    }

    /// An offered spell: at `at`, or on one of `mine` its cursor takes.
    fn cast(
        &mut self,
        (world, local): (&World, u32),
        program: &Program,
        mine: &[u32],
        at: (i32, i32),
    ) -> Vec<Input> {
        let spells = world.spells(program, local);
        let spell = spells.get(self.draw(spells.len()));
        let takes: Vec<u32> = mine
            .iter()
            .copied()
            .filter(|id| {
                spell.is_some_and(|s| world.spell_target(program, local, s.row.spell, *id))
            })
            .collect();
        let unit = takes.get(self.draw(takes.len())).copied();

        match spell.map(|s| (s.aim, s.row.spell)) {
            Some((Aim::Point(code), _)) => vec![Input::Cast(Cast {
                code,
                at,
                owner: local,
            })],
            Some((Aim::Unit, spell)) => unit
                .map(|target| {
                    Input::Bless(Bless {
                        spell,
                        target,
                        owner: local,
                        at,
                    })
                })
                .into_iter()
                .collect(),
            _ => Vec::new(),
        }
    }

    /// An attack flag on a flaggable piece not its own.
    fn attack(&mut self, (world, local): (&World, u32), shown: &[Shown]) -> Vec<Input> {
        let targets: Vec<_> = shown
            .iter()
            .filter(|s| s.owner != local && world.flaggable(local, s.agent, REWARD))
            .collect();

        targets
            .get(self.draw(targets.len()))
            .map(|t| {
                Input::Flag(Flag {
                    code: ATTACK_FLAG,
                    at: t.at,
                    owner: local,
                    reward: REWARD,
                    target: t.agent,
                })
            })
            .into_iter()
            .collect()
    }

    /// An offered building, its preview deleted, then placed where it
    /// fits nearest a drawn spot about `home`.
    fn build(
        &mut self,
        (world, local): (&World, u32),
        program: &Program,
        home: (i32, i32),
    ) -> Vec<Input> {
        let offers: Vec<_> = world
            .build_menu(&self.menu, local)
            .into_iter()
            .filter(|o| o.offered && world.affords(local, o.price))
            .collect();
        let Some(code) = offers.get(self.draw(offers.len())).map(|o| o.code) else {
            return Vec::new();
        };
        let mut spots: Vec<(i32, i32)> = (-REACH..=REACH)
            .flat_map(|dx| (-REACH..=REACH).map(move |dy| (dx, dy)))
            .collect();

        spots.sort_by_key(|(dx, dy)| dx * dx + dy * dy);

        let skip = self.draw(spots.len() / 4);
        let placer = world.placer(program, &self.menu, local, code);
        let at = spots
            .iter()
            .cycle()
            .skip(skip)
            .take(spots.len())
            .map(|(dx, dy)| (home.0 + dx * PER_HALF * 2, home.1 + dy * PER_HALF * 2))
            .find(|at| placer(*at) == Placement::Fits);

        at.map(|at| {
            orders::build(BuildOrder {
                code,
                at,
                owner: local,
            })
        })
        .unwrap_or_default()
    }
}
