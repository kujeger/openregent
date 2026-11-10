//! The game around the world: frames run, the scheduler's queues, input
//! waiting, and what its host keeps of the VM's objects.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// The scheduler's two queues, front first.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Queues {
    pub active: Vec<u32>,
    pub next: Vec<u32>,
}

/// Script errors so far.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reports {
    pub count: u64,
    pub first: Vec<String>,
}

/// What the host keeps.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Host {
    /// Each attribute's VM object, by agent and folded name.
    pub attributes: BTreeMap<(u32, String), u32>,
    /// The seed the GPL RNG took.
    pub seed: u32,
    pub reports: Reports,
}

/// The game at a frame's end.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Game {
    pub frames: u32,
    pub queues: Queues,
    /// Orders taken for frames still to run: each frame and `Order` text.
    pub inputs: Vec<(u32, String)>,
    pub host: Host,
}
