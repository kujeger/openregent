//! A unit's order tracks: its walker on track 0, track 1's orders,
//! track 2's action.

use serde::{Deserialize, Serialize};

use super::Code;
use super::world::Missile;

/// An animation's frames and timing, from the art.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pose {
    pub frames: Vec<u16>,
    pub playback: u32,
    pub keeps: u32,
    pub cues: Vec<Option<u16>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sets {
    pub set: u32,
    pub after: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Look {
    Frame { set: u32, frame: u32 },
    Plays { set: u32, since: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Settle {
    Keep,
    Slot(u64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Phase {
    pub frame: u32,
    pub when: u32,
    pub rate: u32,
    pub frames: u32,
    pub keeps: u32,
}

/// Track 2's order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Act {
    Turn {
        want: i32,
        pose: Pose,
        phase: Phase,
    },
    Init {
        frame: u32,
        frames: u32,
        rate: u32,
        fires: u32,
    },
}

/// Track 2 at work.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stage {
    pub order: Act,
    pub when: u32,
    pub delta: u32,
    pub held: bool,
    pub settle: Settle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Finish {
    Exact,
    Overshoot,
}

/// Track 0's top order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Step {
    Turn { want: i32, to: [i32; 3] },
    Move { to: [i32; 3], finish: Finish },
    Wait,
    End,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Gait {
    Walk,
    Stand,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Parts {
    Walk,
    Stand,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Track1 {
    Plays(u32),
    PathEnd,
    Clear,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Movement {
    Moves,
    Fixed,
}

/// A walker: place, facing, path and its orders on track 0.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Walker {
    pub at: [i32; 3],
    pub facing: i32,
    pub nodes: Vec<(i32, i32)>,
    pub index: u64,
    pub keep: u64,
    pub plans: u32,
    pub cell: (i32, i32),
    pub first: bool,
    /// The path order's time and rate.
    pub path: Option<(u32, u32)>,
    pub top: Option<(Step, u32, u32)>,
    pub wants: Option<(i32, i32)>,
    pub waits: u32,
    pub walkers: bool,
    pub after_wait: bool,
    pub step: i32,
    pub move_rate: u32,
    pub class_rate: u32,
    pub rate_modifier: i32,
    pub move_dist: u32,
    pub turn_rate: u32,
    pub turn_step: i32,
    pub flags: u32,
    pub flight: Option<(i32, i32)>,
    pub stage: Option<Stage>,
    pub gait: Gait,
    pub sets: Sets,
    pub left: Option<Look>,
    pub laid: Option<u32>,
    pub played: Vec<Track1>,
    pub movement: Movement,
    pub phase: Phase,
    pub parts: Parts,
    pub walk: Option<Pose>,
    pub stand: Option<Pose>,
    pub end: Option<Pose>,
    pub shown: Option<Pose>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Goal {
    Beside(u32, Option<(i32, i32)>),
    Point((i32, i32)),
}

/// A `MovePath`'s avoid modes and radius.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Avoid {
    pub modes: u32,
    pub radius: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Callback {
    None,
    Enter(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Aim {
    Unit(u32),
    Point((i32, i32)),
    None,
}

/// A track 1 order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Intent {
    FollowUnit {
        target: u32,
        max: i32,
        info: u32,
    },
    FollowPath {
        waypoints: Vec<(i32, i32)>,
        next: u64,
        info: u32,
        range: i32,
        callback: Callback,
    },
    Action {
        aim: Aim,
        ms: Option<u32>,
        pose: Option<(Pose, Settle, Sets)>,
        armed: bool,
        missile: Option<Missile>,
    },
    Fade {
        target: u8,
        rate: u32,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackOrder {
    pub intent: Intent,
    pub when: u32,
    pub delta: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Shot {
    pub missile: Missile,
    pub target: Target,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Target {
    Unit(u32),
    Point((i32, i32)),
}

/// A function an action's peak dispatches.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Script {
    pub function: String,
    pub kind: u32,
}

/// An action's descriptor fields the tracks keep.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Acted {
    pub rate: u32,
    pub set: u32,
    pub projectile: Option<Missile>,
    pub after: i32,
    pub overlay: Option<Code>,
    pub proc: u32,
    pub sound: Option<Code>,
    pub begin: Option<Code>,
    pub peak: Option<Code>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Flash {
    pub count: u32,
    pub when: u32,
}

/// A unit's order tracks.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tracks {
    pub walker: Walker,
    pub goal: Goal,
    pub avoid: Avoid,
    pub intents: Vec<TrackOrder>,
    pub shot: Option<Shot>,
    pub proc: u32,
    pub script: Option<(Script, Aim)>,
    pub pending: Option<(u32, Option<(Script, Aim)>)>,
    pub overlay: Option<Code>,
    pub pending_overlay: Option<Code>,
    pub sounding: Option<Acted>,
    pub pending_sounding: Option<Acted>,
    pub released: bool,
    pub animate: bool,
    pub flash: Option<Flash>,
}
