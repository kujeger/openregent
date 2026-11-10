//! To and from the save model.

use super::*;
use crate::save::{self as m, Refused, Saved};
use std::result::Result;

impl Saved for HeightMap {
    type Model = m::HeightMap;

    fn save(&self) -> m::HeightMap {
        m::HeightMap {
            across: self.across,
            heights: self.heights.save(),
        }
    }

    fn load(model: m::HeightMap) -> Result<HeightMap, Refused> {
        Ok(HeightMap {
            across: model.across,
            heights: Vec::load(model.heights)?,
        })
    }
}

impl Saved for Goal {
    type Model = m::Goal;

    fn save(&self) -> m::Goal {
        match *self {
            Goal::Beside(id, at) => m::Goal::Beside(id, at),
            Goal::Point(at) => m::Goal::Point(at),
        }
    }

    fn load(model: m::Goal) -> Result<Goal, Refused> {
        Ok(match model {
            m::Goal::Beside(id, at) => Goal::Beside(id, at),
            m::Goal::Point(at) => Goal::Point(at),
        })
    }
}

impl Saved for Avoid {
    type Model = m::Avoid;

    fn save(&self) -> m::Avoid {
        m::Avoid {
            modes: self.modes,
            radius: self.radius,
        }
    }

    fn load(model: m::Avoid) -> Result<Avoid, Refused> {
        Ok(Avoid {
            modes: model.modes,
            radius: model.radius,
        })
    }
}

impl Saved for Callback {
    type Model = m::Callback;

    fn save(&self) -> m::Callback {
        match *self {
            Callback::None => m::Callback::None,
            Callback::Enter(id) => m::Callback::Enter(id),
        }
    }

    fn load(model: m::Callback) -> Result<Callback, Refused> {
        Ok(match model {
            m::Callback::None => Callback::None,
            m::Callback::Enter(id) => Callback::Enter(id),
        })
    }
}

impl Saved for Aim {
    type Model = m::Aim;

    fn save(&self) -> m::Aim {
        match *self {
            Aim::Unit(id) => m::Aim::Unit(id),
            Aim::Point(at) => m::Aim::Point(at),
            Aim::None => m::Aim::None,
        }
    }

    fn load(model: m::Aim) -> Result<Aim, Refused> {
        Ok(match model {
            m::Aim::Unit(id) => Aim::Unit(id),
            m::Aim::Point(at) => Aim::Point(at),
            m::Aim::None => Aim::None,
        })
    }
}

impl Saved for Intent {
    type Model = m::Intent;

    fn save(&self) -> m::Intent {
        match self {
            Intent::FollowUnit { target, max, info } => m::Intent::FollowUnit {
                target: *target,
                max: *max,
                info: *info,
            },
            Intent::FollowPath {
                waypoints,
                next,
                info,
                range,
                callback,
            } => m::Intent::FollowPath {
                waypoints: waypoints.clone(),
                next: next.save(),
                info: *info,
                range: *range,
                callback: callback.save(),
            },
            Intent::Action {
                aim,
                ms,
                pose,
                armed,
                missile,
            } => m::Intent::Action {
                aim: aim.save(),
                ms: *ms,
                pose: pose.save(),
                armed: *armed,
                missile: missile.save(),
            },
            Intent::Fade { target, rate } => m::Intent::Fade {
                target: *target,
                rate: *rate,
            },
        }
    }

    fn load(model: m::Intent) -> Result<Intent, Refused> {
        Ok(match model {
            m::Intent::FollowUnit { target, max, info } => Intent::FollowUnit { target, max, info },
            m::Intent::FollowPath {
                waypoints,
                next,
                info,
                range,
                callback,
            } => Intent::FollowPath {
                waypoints,
                next: usize::load(next)?,
                info,
                range,
                callback: Callback::load(callback)?,
            },
            m::Intent::Action {
                aim,
                ms,
                pose,
                armed,
                missile,
            } => Intent::Action {
                aim: Aim::load(aim)?,
                ms,
                pose: Option::load(pose)?,
                armed,
                missile: Option::load(missile)?,
            },
            m::Intent::Fade { target, rate } => Intent::Fade { target, rate },
        })
    }
}

impl Saved for TrackOrder {
    type Model = m::TrackOrder;

    fn save(&self) -> m::TrackOrder {
        m::TrackOrder {
            intent: self.intent.save(),
            when: self.when,
            delta: self.delta,
        }
    }

    fn load(model: m::TrackOrder) -> Result<TrackOrder, Refused> {
        Ok(TrackOrder {
            intent: Intent::load(model.intent)?,
            when: model.when,
            delta: model.delta,
        })
    }
}

impl Saved for Flash {
    type Model = m::Flash;

    fn save(&self) -> m::Flash {
        m::Flash {
            count: self.count,
            when: self.when,
        }
    }

    fn load(model: m::Flash) -> Result<Flash, Refused> {
        Ok(Flash {
            count: model.count,
            when: model.when,
        })
    }
}

impl Saved for Act {
    type Model = m::Acted;

    fn save(&self) -> m::Acted {
        let Act {
            rate,
            set,
            projectile,
            after,
            overlay,
            proc,
            sound,
            begin,
            peak,
        } = self;

        m::Acted {
            rate: *rate,
            set: *set,
            projectile: projectile.save(),
            after: *after,
            overlay: overlay.save(),
            proc: *proc,
            sound: sound.save(),
            begin: begin.save(),
            peak: peak.save(),
        }
    }

    fn load(model: m::Acted) -> Result<Act, Refused> {
        Ok(Act {
            rate: model.rate,
            set: model.set,
            projectile: Option::load(model.projectile)?,
            after: model.after,
            overlay: Option::load(model.overlay)?,
            proc: model.proc,
            sound: Option::load(model.sound)?,
            begin: Option::load(model.begin)?,
            peak: Option::load(model.peak)?,
        })
    }
}

impl Saved for Tracks {
    type Model = m::Tracks;

    fn save(&self) -> m::Tracks {
        let Tracks {
            walker,
            goal,
            avoid,
            intents,
            shot,
            proc,
            script,
            pending,
            overlay,
            pending_overlay,
            sounding,
            pending_sounding,
            released,
            animate,
            flash,
        } = self;

        m::Tracks {
            walker: walker.save(),
            goal: goal.save(),
            avoid: avoid.save(),
            intents: intents.save(),
            shot: shot.save(),
            proc: *proc,
            script: script.save(),
            pending: pending.save(),
            overlay: overlay.save(),
            pending_overlay: pending_overlay.save(),
            sounding: sounding.save(),
            pending_sounding: pending_sounding.save(),
            released: *released,
            animate: *animate,
            flash: flash.save(),
        }
    }

    fn load(model: m::Tracks) -> Result<Tracks, Refused> {
        Ok(Tracks {
            walker: Walker::load(model.walker)?,
            goal: Goal::load(model.goal)?,
            avoid: Avoid::load(model.avoid)?,
            intents: Vec::load(model.intents)?,
            shot: Option::load(model.shot)?,
            proc: model.proc,
            script: Option::load(model.script)?,
            pending: Option::load(model.pending)?,
            overlay: Option::load(model.overlay)?,
            pending_overlay: Option::load(model.pending_overlay)?,
            sounding: Option::load(model.sounding)?,
            pending_sounding: Option::load(model.pending_sounding)?,
            released: model.released,
            animate: model.animate,
            flash: Option::load(model.flash)?,
        })
    }
}
