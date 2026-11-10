//! Deep digests folded on a worker thread, off the stepping one: the
//! snapshot is taken at step n, its fold sent with a later bundle.
//!
//! ```text
//!  stepping thread   step n: snapshot ──jobs──> worker: deep_fold
//!                    a later pump:  <──done── (n, digest, snapshot)
//! ```
//!
//! The snapshot comes back with its digest: a desync found at frame n
//! writes it (`Peer::dump`).

use std::sync::Mutex;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread::JoinHandle;

use bevy::log::error;
use regent_core::save::Snapshot;
use regent_core::sim::game::deep_fold;

/// One snapshot folded: its frame, deep digest, and the snapshot.
pub struct Folded {
    pub frame: u32,
    pub deep: u64,
    pub snapshot: Snapshot,
}

/// The worker, started with the first fold; ends with it.
pub struct Folder {
    jobs: Option<Sender<(u32, Snapshot)>>,
    /// Locked so the peer may be a resource: a `Receiver` is not `Sync`.
    done: Mutex<Receiver<Folded>>,
    worker: Option<JoinHandle<()>>,
}

impl Folder {
    /// The worker, started.
    pub fn new() -> Folder {
        let (jobs, todo) = channel::<(u32, Snapshot)>();
        let (tell, done) = channel();
        let worker = std::thread::Builder::new()
            .name("deep-digest".to_owned())
            .spawn(move || {
                for (frame, snapshot) in todo {
                    let deep =
                        crate::perf::timed(crate::perf::Timer::Fold, || deep_fold(&snapshot));

                    // The peer gone: nothing left to tell.
                    if tell
                        .send(Folded {
                            frame,
                            deep,
                            snapshot,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .map_err(|e| error!("net: no deep digest worker: {e}"))
            .ok();

        Folder {
            jobs: Some(jobs),
            done: Mutex::new(done),
            worker,
        }
    }

    /// `snapshot`, of frame `frame`, to fold.
    pub fn fold(&self, frame: u32, snapshot: Snapshot) {
        if let Some(jobs) = &self.jobs
            && jobs.send((frame, snapshot)).is_err()
        {
            error!("net: the deep digest worker is gone");
        }
    }

    /// The folds done since the last call, oldest first.
    pub fn take(&self) -> Vec<Folded> {
        self.done
            .lock()
            .map(|done| done.try_iter().collect())
            .unwrap_or_default()
    }
}

impl Drop for Folder {
    /// The jobs closed, the worker joined: no thread outlives its game.
    fn drop(&mut self) {
        self.jobs = None;

        if let Some(worker) = self.worker.take()
            && worker.join().is_err()
        {
            error!("net: the deep digest worker panicked");
        }
    }
}
