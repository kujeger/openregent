//! The world to and from the save model ([`crate::save`]).
//!
//! ```text
//!  World ──saved──> save::World: every field that changes in play
//!  a world made as the saved one was ──restore(save::World)──> the saved
//!    world: descriptors and settings kept, the rest taken
//! ```

use crate::sim::world::kind;
use std::collections::{BTreeMap, BTreeSet};

use crate::base::digest::Map;
use crate::base::fixed::Fixed;
use std::result::Result;

use super::alter::Assign;
use super::attributes::Reread;
use super::board::Board;
use super::build::BuildOrder;
use super::cheat::Flags;
use super::combat::Mend;
use super::defeat::End;
use super::draw::{Overhead, Tint};
use super::effectors::Effectors;
use super::effectors::{Effector, Ending, Hung};
use super::flag::Flag;
use super::frame::Frames;
use super::lairs::Lairs;
use super::limits::Limits;
use super::message::Message;
use super::minimap_mark::{Mark, Wave};
use super::new_map::{LairEntry, OnGrid};
use super::pieces::Pieces;
use super::places::Motion;
use super::players::Players;
use super::recruit::Recruit;
use super::relations::Relations;
use super::reveal::Fog;
use super::spell::cast::Cast;
use super::step::Rngs;
use super::threads::Scripts;
use super::threads::{Breaks, Gpl, Logging};
use super::unit::Units;
use super::*;
use crate::save::{self as m, Refused, Saved};

/// Plain enums: one variant each way.
macro_rules! variants {
    ($ty:ty => $model:ty { $($v:ident),* }) => {
        impl Saved for $ty {
            type Model = $model;

            fn save(&self) -> $model {
                match self {
                    $(<$ty>::$v => <$model>::$v,)*
                }
            }

            fn load(model: $model) -> Result<$ty, Refused> {
                Ok(match model {
                    $(<$model>::$v => <$ty>::$v,)*
                })
            }
        }
    };
}

variants!(Motion => m::Motion { Still, Walking });
variants!(Ending => m::Ending { Wait, After });
variants!(Overhead => m::Overhead { Shown, Hidden });
variants!(End => m::End { Lost, Won });
variants!(Gpl => m::Gpl { Running, Halted });
variants!(Logging => m::Logging { On, Off });
variants!(OnGrid => m::OnGrid { On, Off });
variants!(Sight => m::Sight { Everyone, Owner });
variants!(Frames => m::Frames { Unseen, Stepped });

impl Saved for Member {
    type Model = m::Member;

    fn save(&self) -> m::Member {
        m::Member {
            name: self.name.clone(),
            ty: self.ty,
            value: self.value.save(),
        }
    }

    fn load(model: m::Member) -> Result<Member, Refused> {
        Ok(Member {
            name: model.name,
            ty: model.ty,
            value: Value::load(model.value)?,
        })
    }
}

impl Saved for Agent {
    type Model = m::Agent;

    fn save(&self) -> m::Agent {
        let Agent {
            attributes,
            title,
            name,
            values,
        } = self;

        m::Agent {
            attributes: attributes.clone(),
            title: title.clone(),
            name: name.clone(),
            values: values.save(),
        }
    }

    fn load(model: m::Agent) -> Result<Agent, Refused> {
        Ok(Agent {
            attributes: model.attributes,
            title: model.title,
            name: model.name,
            values: Vec::load(model.values)?,
        })
    }
}

impl Saved for Place {
    type Model = m::Place;

    fn save(&self) -> m::Place {
        let Place {
            kind,
            fixed,
            standing,
            holder,
            state,
        } = *self;

        m::Place {
            kind: kind.0,
            fixed: fixed.save(),
            standing,
            holder,
            state,
        }
    }

    fn load(model: m::Place) -> Result<Place, Refused> {
        Ok(Place {
            kind: kind::Kind(model.kind),
            fixed: <(Fixed, Fixed)>::load(model.fixed)?,
            standing: model.standing,
            holder: model.holder,
            state: model.state,
        })
    }
}

impl Saved for Piece {
    type Model = m::Piece;

    fn save(&self) -> m::Piece {
        let Piece {
            owner,
            attributes,
            handle,
            code,
            name,
            place,
            spells,
        } = self;

        m::Piece {
            owner: *owner,
            attributes: attributes.clone(),
            handle: *handle,
            code: m::Code(code.0),
            name: name.clone(),
            place: place.save(),
            spells: spells.save(),
        }
    }

    fn load(model: m::Piece) -> Result<Piece, Refused> {
        Ok(Piece {
            owner: model.owner,
            attributes: model.attributes,
            handle: model.handle,
            code: FourCc(model.code.0),
            name: model.name,
            place: Option::load(model.place)?,
            spells: Option::load(model.spells)?,
        })
    }
}

impl Saved for Effector {
    type Model = m::Effector;

    fn save(&self) -> m::Effector {
        let Effector {
            agent,
            name,
            handle,
            when,
            duration,
            end,
            ending,
            ended,
            value,
        } = self;

        m::Effector {
            agent: *agent,
            name: name.clone(),
            handle: *handle,
            when: *when,
            duration: *duration,
            end: *end,
            ending: ending.save(),
            ended: *ended,
            value: *value,
        }
    }

    fn load(model: m::Effector) -> Result<Effector, Refused> {
        Ok(Effector {
            agent: model.agent,
            name: model.name,
            handle: model.handle,
            when: model.when,
            duration: model.duration,
            end: model.end,
            ending: Ending::load(model.ending)?,
            ended: model.ended,
            value: model.value,
        })
    }
}

impl Saved for Tint {
    type Model = m::Tint;

    fn save(&self) -> m::Tint {
        m::Tint {
            colour: self.colour,
            transparency: self.transparency,
        }
    }

    fn load(model: m::Tint) -> Result<Tint, Refused> {
        Ok(Tint {
            colour: model.colour,
            transparency: model.transparency,
        })
    }
}

impl Saved for Listed {
    type Model = m::Listed;

    fn save(&self) -> m::Listed {
        match *self {
            Listed::Unit(id) => m::Listed::Unit(id),
            Listed::Missile(h) => m::Listed::Missile(h),
            Listed::Effector(h) => m::Listed::Effector(h),
            Listed::Clone(h) => m::Listed::Clone(h),
        }
    }

    fn load(model: m::Listed) -> Result<Listed, Refused> {
        Ok(match model {
            m::Listed::Unit(id) => Listed::Unit(id),
            m::Listed::Missile(h) => Listed::Missile(h),
            m::Listed::Effector(h) => Listed::Effector(h),
            m::Listed::Clone(h) => Listed::Clone(h),
        })
    }
}

impl Saved for BuildOrder {
    type Model = m::BuildOrder;

    fn save(&self) -> m::BuildOrder {
        m::BuildOrder {
            code: self.code.save(),
            at: self.at,
            owner: self.owner,
        }
    }

    fn load(model: m::BuildOrder) -> Result<BuildOrder, Refused> {
        Ok(BuildOrder {
            code: FourCc::load(model.code)?,
            at: model.at,
            owner: model.owner,
        })
    }
}

impl Saved for Recruit {
    type Model = m::Recruit;

    fn save(&self) -> m::Recruit {
        m::Recruit {
            guild: self.guild,
            code: self.code.save(),
        }
    }

    fn load(model: m::Recruit) -> Result<Recruit, Refused> {
        Ok(Recruit {
            guild: model.guild,
            code: FourCc::load(model.code)?,
        })
    }
}

impl Saved for Flag {
    type Model = m::Flag;

    fn save(&self) -> m::Flag {
        let Flag {
            code,
            at,
            owner,
            reward,
            target,
        } = *self;

        m::Flag {
            code: code.save(),
            at,
            owner,
            reward,
            target,
        }
    }

    fn load(model: m::Flag) -> Result<Flag, Refused> {
        Ok(Flag {
            code: FourCc::load(model.code)?,
            at: model.at,
            owner: model.owner,
            reward: model.reward,
            target: model.target,
        })
    }
}

impl Saved for Message {
    type Model = m::Message;

    fn save(&self) -> m::Message {
        let Message {
            handle,
            text,
            holder,
            at,
            made,
        } = *self;

        m::Message {
            handle,
            text,
            holder,
            at: at.save(),
            made,
        }
    }

    fn load(model: m::Message) -> Result<Message, Refused> {
        Ok(Message {
            handle: model.handle,
            text: model.text,
            holder: model.holder,
            at: <[Fixed; 3]>::load(model.at)?,
            made: model.made,
        })
    }
}

impl Saved for Wave {
    type Model = m::Wave;

    fn save(&self) -> m::Wave {
        m::Wave {
            frames: self.frames,
            interval: self.interval,
        }
    }

    fn load(model: m::Wave) -> Result<Wave, Refused> {
        Ok(Wave {
            frames: model.frames,
            interval: model.interval,
        })
    }
}

impl Saved for Mark {
    type Model = m::Mark;

    fn save(&self) -> m::Mark {
        let Mark {
            handle,
            code,
            owner,
            at,
            made,
            wave,
            sight,
        } = *self;

        m::Mark {
            handle,
            code: code.save(),
            owner,
            at,
            made,
            wave: wave.save(),
            sight: sight.save(),
        }
    }

    fn load(model: m::Mark) -> Result<Mark, Refused> {
        Ok(Mark {
            handle: model.handle,
            code: FourCc::load(model.code)?,
            owner: model.owner,
            at: model.at,
            made: model.made,
            wave: Option::load(model.wave)?,
            sight: Sight::load(model.sight)?,
        })
    }
}

impl Saved for Cast {
    type Model = m::Cast;

    fn save(&self) -> m::Cast {
        m::Cast {
            code: self.code.save(),
            at: self.at,
            owner: self.owner,
        }
    }

    fn load(model: m::Cast) -> Result<Cast, Refused> {
        Ok(Cast {
            code: FourCc::load(model.code)?,
            at: model.at,
            owner: model.owner,
        })
    }
}

impl Saved for Assign {
    type Model = m::Assign;

    fn save(&self) -> m::Assign {
        m::Assign {
            frame: self.frame,
            agent: self.agent,
            words: self.words.clone(),
        }
    }

    fn load(model: m::Assign) -> Result<Assign, Refused> {
        Ok(Assign {
            frame: model.frame,
            agent: model.agent,
            words: model.words,
        })
    }
}

impl Saved for LairEntry {
    type Model = m::LairEntry;

    fn save(&self) -> m::LairEntry {
        m::LairEntry {
            code: self.code.save(),
            weight: self.weight,
            force: self.force,
        }
    }

    fn load(model: m::LairEntry) -> Result<LairEntry, Refused> {
        Ok(LairEntry {
            code: FourCc::load(model.code)?,
            weight: model.weight,
            force: model.force,
        })
    }
}

impl Saved for Reread {
    type Model = m::Reread;

    fn save(&self) -> m::Reread {
        m::Reread {
            fresh: self.fresh.save(),
            text: self.text.save(),
        }
    }

    fn load(model: m::Reread) -> Result<Reread, Refused> {
        Ok(Reread {
            fresh: Member::load(model.fresh)?,
            text: Option::load(model.text)?,
        })
    }
}

impl Saved for Breaks {
    type Model = m::Breaks;

    fn save(&self) -> m::Breaks {
        m::Breaks {
            countdown: self.countdown,
            requested: self.requested,
        }
    }

    fn load(model: m::Breaks) -> Result<Breaks, Refused> {
        Ok(Breaks {
            countdown: model.countdown,
            requested: model.requested,
        })
    }
}

impl Saved for Limits {
    type Model = m::Limits;

    fn save(&self) -> m::Limits {
        m::Limits {
            disabled: self.disabled.save(),
            caps: self.caps.save(),
            graves: self.graves.save(),
        }
    }

    fn load(model: m::Limits) -> Result<Limits, Refused> {
        Ok(Limits {
            disabled: BTreeSet::load(model.disabled)?,
            caps: BTreeMap::load(model.caps)?,
            graves: Vec::load(model.graves)?,
        })
    }
}

impl Saved for Mend {
    type Model = m::Mend;

    fn save(&self) -> m::Mend {
        let Mend {
            period,
            poison,
            heal,
            last,
            count,
        } = *self;

        m::Mend {
            period,
            poison,
            heal,
            last,
            count,
        }
    }

    fn load(model: m::Mend) -> Result<Mend, Refused> {
        Ok(Mend {
            period: model.period,
            poison: model.poison,
            heal: model.heal,
            last: model.last,
            count: model.count,
        })
    }
}

impl Saved for Hung {
    type Model = m::Hung;

    fn save(&self) -> m::Hung {
        let Hung {
            code,
            made,
            base,
            spot,
            facing,
        } = *self;

        m::Hung {
            code: code.save(),
            made,
            base: base.save(),
            spot: spot.save(),
            facing,
        }
    }

    fn load(model: m::Hung) -> Result<Hung, Refused> {
        Ok(Hung {
            code: FourCc::load(model.code)?,
            made: model.made,
            base: <[Fixed; 3]>::load(model.base)?,
            spot: <[Fixed; 3]>::load(model.spot)?,
            facing: model.facing,
        })
    }
}

/// Every field: what stands until it ends, play state a load restores.
impl Saved for Outbox {
    type Model = m::Outbox;

    fn save(&self) -> m::Outbox {
        let Outbox {
            marks,
            messages,
            music,
        } = self;

        m::Outbox {
            marks: marks.save(),
            messages: messages.save(),
            music: music.save(),
        }
    }

    fn load(model: m::Outbox) -> Result<Outbox, Refused> {
        Ok(Outbox {
            marks: Saved::load(model.marks)?,
            messages: Saved::load(model.messages)?,
            music: Saved::load(model.music)?,
        })
    }
}

impl Saved for Level {
    type Model = m::Level;

    fn save(&self) -> m::Level {
        m::Level {
            victory: self.victory,
            goal: self.goal,
            events: self.events.clone(),
            mending: self.mending.save(),
            items: self.items.entries().clone(),
        }
    }

    fn load(model: m::Level) -> Result<Level, Refused> {
        Ok(Level {
            victory: model.victory,
            goal: model.goal,
            events: model.events,
            mending: Mend::load(model.mending)?,
            items: super::inventory::Register::restored(model.items),
        })
    }
}

impl Saved for Pending {
    type Model = m::Pending;

    fn save(&self) -> m::Pending {
        m::Pending {
            research: self.research.save(),
            alters: self.alters.save(),
            timers: self.timers.save(),
            timeouts: self.timeouts.save(),
            rearming: self.rearming.clone(),
            asking: self.asking.save(),
            camps: self.camps.save(),
        }
    }

    fn load(model: m::Pending) -> Result<Pending, Refused> {
        Ok(Pending {
            research: Saved::load(model.research)?,
            alters: Saved::load(model.alters)?,
            hires: super::hire::Hires::default(),
            timers: Saved::load(model.timers)?,
            timeouts: Saved::load(model.timeouts)?,
            rearming: model.rearming,
            asking: Option::load(model.asking)?,
            camps: Saved::load(model.camps)?,
        })
    }
}

impl Saved for Effects {
    type Model = m::Effects;

    fn save(&self) -> m::Effects {
        m::Effects {
            flights: self.flights.save(),
            plumes: self.plumes.save(),
            clones: self.clones.save(),
            quakes: self.quakes.save(),
        }
    }

    fn load(model: m::Effects) -> Result<Effects, Refused> {
        Ok(Effects {
            flights: Saved::load(model.flights)?,
            plumes: Saved::load(model.plumes)?,
            clones: Saved::load(model.clones)?,
            quakes: Saved::load(model.quakes)?,
        })
    }
}

impl World {
    /// Every field that changes in play, as a save holds it.
    ///
    /// Left out, and why (each field left out says so beside it; the
    /// world holds no per-machine state, module doc):
    /// - the dataset's tables: `descriptors`, the advisors' rules;
    /// - what the front end takes each frame: `output` (`Output`);
    /// - diagnostics: `losses`;
    /// - the frame's, empty between frames: the lent GPL RNG, and the dispatches'
    ///   and hires' own (`frame::Dispatches`, `hire::Hires`).
    #[must_use]
    #[allow(clippy::too_many_lines, reason = "a field a line")]
    pub fn saved(&self) -> m::World {
        let World {
            agents,
            pieces,
            units,
            now,
            level,
            outbox,
            scripts,
            // Read-only tables: the dataset's.
            descriptors: _,
            board,
            // Kept from the pieces: made anew after a load.
            reaches: _,
            handle,
            // Per-frame: a log the front end takes (`World::take_lost`).
            losses: _,
            relations,
            dispatches,
            players,
            effectors,
            naming,
            frames,
            pending,
            lairs,
            rngs,
            fog,
            effects,
            cheats,
            ruins,
            advisors,
            limits,
            poison_sources,
            // From the program: `Game::begin` reads it again.
            lps: _,
            // Per-frame: the front end takes it.
            output: _,
        } = self;

        // Mid-frame state, empty between frames, where a save is taken.
        debug_assert!(!rngs.lending());

        m::World {
            agents: agents.save(),
            pieces: pieces.bound().save(),
            map: pieces.unbound().save(),
            units: units.save(),
            now: *now,
            level: level.save(),
            outbox: outbox.save(),
            scripts: scripts.save(),
            board: board.save(),
            handle: *handle,
            relations: relations.save(),
            dispatches: dispatches.save(),
            players: players.save(),
            effectors: effectors.save(),
            naming: naming.save(),
            frames: frames.save(),
            pending: pending.save(),
            lairs: lairs.save(),
            global: rngs.state(),
            fog: fog.save(),
            effects: effects.save(),
            cheats: cheats.save(),
            ruins: ruins.save(),
            advisors: advisors.watches(),
            limits: limits.save(),
            poison_sources: poison_sources.clone(),
        }
    }

    /// The saved world's state over this one, made as the saved one was
    /// (its dataset and setup): what [`World::saved`] leaves out stays.
    ///
    /// # Errors
    ///
    /// A model naming what this build lacks: a builtin, a research item.
    #[allow(clippy::too_many_lines, reason = "a field a line")]
    pub fn restore(&mut self, model: m::World) -> Result<(), Refused> {
        let m::World {
            agents,
            pieces,
            map,
            units,
            now,
            level,
            outbox,
            scripts,
            board,
            handle,
            relations,
            dispatches,
            players,
            effectors,
            naming,
            frames,
            pending,
            lairs,
            global,
            fog,
            effects,
            cheats,
            ruins,
            advisors,
            limits,
            poison_sources,
        } = model;

        self.agents = Saved::load(agents)?;
        self.pieces = Pieces::of(Map::load(pieces)?, Map::load(map)?);
        self.forget_reaches();
        self.units = Units::load(units)?;
        self.now = now;
        self.level = Level::load(level)?;
        self.outbox = Outbox::load(outbox)?;
        self.scripts = Scripts::load(scripts)?;
        self.board = Board::load(board)?;
        self.handle = handle;
        self.relations = Relations::load(relations)?;
        self.dispatches = frame::Dispatches::load(dispatches)?;
        self.players = Players::load(players)?;
        self.effectors = Effectors::load(effectors)?;
        self.naming = Saved::load(naming)?;
        self.frames = Frames::load(frames)?;
        self.pending = Pending::load(pending)?;
        self.lairs = Lairs::load(lairs)?;
        self.rngs = Rngs::seeded(global);
        self.fog = Fog::load(fog)?;
        self.effects = Effects::load(effects)?;
        self.cheats = Flags::load(cheats)?;
        self.ruins = ruin::Ruins::load(ruins)?;
        self.advisors.rewatch(advisors);
        self.limits = Limits::load(limits)?;
        self.poison_sources = poison_sources;
        self.output = super::Output::default();
        Ok(())
    }
}
