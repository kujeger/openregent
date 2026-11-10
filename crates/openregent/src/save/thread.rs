//! Every save made and written on one thread of its own: a late 512-cell
//! game's save takes about 110 ms to make and write, and the frame does
//! not wait.
//!
//! ```text
//!  ui::keep     SAVE GAME, a LAN save a step took, an autosave: the game
//!               taken (FromSim, on the sim thread), with this machine's
//!               view, minimap and profile ──> Job
//!  the thread   jobs in order, one at a time: save::made, then the file
//!               (Place); a free name or the next autosave number found
//!               as it writes, so no two jobs pick one file ──> Done
//!  ui::keep     each Done, next frames: "Game Saved." or the dialog
//!               again saying why; a LAN save's chat line
//!  game ends    SaveThread dropped (per game): the jobs queued written,
//!               the thread joined, so none is lost and a save loaded
//!               next is whole
//! ```
//!
//! The thread starts with the first save of a game.

use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::JoinHandle;

use bevy::log::{info, warn};
use bevy::prelude::Resource;
use regent_core::view::panel::minimap::Picture;

use crate::save::{self, Contents, FromSim};

/// One save written at a time, by whichever thread: the folder read for
/// a free name or the next autosave number holds until the file is in.
pub(crate) static WRITING: Mutex<()> = Mutex::new(());

/// Where a save goes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Place {
    /// This file, over one there.
    File(PathBuf),
    /// This folder, under the job's name or the first copy's not taken
    /// (`save::free`): another peer's LAN save.
    Free(PathBuf),
    /// This folder, as the next autosave (`save::auto::write`).
    Autosave(PathBuf),
}

/// Who asked a save, for what [`Done`] tells.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    /// The save dialog's SAVE GAME.
    Asked,
    /// A LAN save a step took, asked by `nick`.
    Lan {
        nick: String,
    },
    Autosave,
}

/// A save to make and write: the game taken, [`save::made`]'s other
/// parts, and its name, profile and place.
pub struct Job {
    pub game: FromSim,
    pub described: (save::SaveKind, save::Needs),
    /// The minimap; `None`, a black one.
    pub minimap: Option<Picture>,
    pub interface: save::Interface,
    pub name: String,
    pub profile: String,
    pub place: Place,
    pub kind: Kind,
}

/// A job done: the name asked, and the file and name written, or why
/// not.
#[derive(Debug)]
pub struct Done {
    pub kind: Kind,
    pub name: String,
    pub written: Result<(PathBuf, String), String>,
}

/// The interface's end of the save thread, a game's.
#[derive(Resource, Default)]
pub struct SaveThread {
    /// `None` until the first job.
    worker: Option<Worker>,
    /// Jobs done on the asking thread: no thread would start.
    inline: Vec<Done>,
}

/// The running thread: jobs in, done out.
struct Worker {
    /// `None` only as it ends.
    jobs: Option<Sender<Job>>,
    /// Held so the resource is `Sync`; read through `&mut`.
    done: Mutex<Receiver<Done>>,
    /// `None` once joined.
    thread: Option<JoinHandle<()>>,
}

impl SaveThread {
    /// `job` queued: made and written on the thread, after those before
    /// it. Where no thread runs, done here instead.
    pub fn send(&mut self, job: Job) {
        let left = match self.worker().and_then(|w| w.jobs.as_ref()) {
            Some(jobs) => jobs.send(job).err(),
            None => Some(mpsc::SendError(job)),
        };

        if let Some(mpsc::SendError(job)) = left {
            self.inline.push(run(job));
        }
    }

    /// The thread, started on the first call.
    fn worker(&mut self) -> Option<&Worker> {
        if self.worker.is_none() {
            let (jobs, queue) = mpsc::channel::<Job>();
            let (finished, done) = mpsc::channel();
            let spawned = std::thread::Builder::new()
                .name("save".to_owned())
                .spawn(move || {
                    for job in queue {
                        let _ = finished.send(run(job));
                    }
                });

            match spawned {
                Ok(thread) => {
                    self.worker = Some(Worker {
                        jobs: Some(jobs),
                        done: Mutex::new(done),
                        thread: Some(thread),
                    });
                }
                Err(e) => warn!("save thread: {e}"),
            }
        }

        self.worker.as_ref()
    }

    /// The jobs done since the last call, in order.
    pub fn done(&mut self) -> Vec<Done> {
        let finished = self.worker.as_mut().map_or_else(Vec::new, |w| {
            w.done
                .get_mut()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .try_iter()
                .collect()
        });

        std::mem::take(&mut self.inline)
            .into_iter()
            .chain(finished)
            .collect()
    }

    /// Every job queued done, the thread ended: those not yet taken.
    #[cfg(test)]
    pub fn finish(&mut self) -> Vec<Done> {
        if let Some(w) = &mut self.worker {
            w.end();
        }

        self.done()
    }
}

impl Worker {
    /// The jobs' channel closed, so the thread ends after the last;
    /// joined.
    fn end(&mut self) {
        drop(self.jobs.take());

        if let Some(thread) = self.thread.take()
            && thread.join().is_err()
        {
            warn!("the save thread ended in a panic");
        }
    }
}

impl Drop for Worker {
    /// The jobs queued written before the game goes: the wait is theirs.
    fn drop(&mut self) {
        self.end();
    }
}

/// `job` made and written, and logged.
fn run(job: Job) -> Done {
    let Job {
        game,
        described,
        minimap,
        interface,
        name,
        profile,
        place,
        kind,
    } = job;
    let _writing = WRITING
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let written = save::made(game, described, save::png(minimap.as_ref()), interface)
        .and_then(|c| put(c, (&name, profile), &place));

    match &written {
        Ok((path, named)) => info!("saved {named}: {}", path.display()),
        Err(why) => warn!("save {name}: {why}"),
    }

    Done {
        kind,
        name,
        written,
    }
}

/// `contents` written to `place` as `name` of `profile`: the file and
/// name.
fn put(
    contents: Contents,
    (name, profile): (&str, String),
    place: &Place,
) -> Result<(PathBuf, String), String> {
    // Moved, not copied: a late game's snapshot is large.
    let named = |name: String| Contents {
        meta: save::Meta {
            name,
            profile,
            ..contents.meta
        },
        ..contents
    };

    match place {
        Place::File(path) => {
            save::write(path, &named(name.to_owned())).map(|()| (path.clone(), name.to_owned()))
        }
        Place::Free(dir) => {
            let (path, free) = save::free(dir, name)?;

            save::write(&path, &named(free.clone())).map(|()| (path, free))
        }
        Place::Autosave(dir) => crate::save::auto::write(dir, &named(name.to_owned()))
            .map(|path| (path, name.to_owned())),
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use regent_core::data::text::Text;

    use super::*;
    use crate::game::record::Recorder;
    use crate::game::sim::{Session, Sim};

    /// A fresh scratch folder of its own.
    fn scratch(test: &str) -> crate::testdata::Scratch {
        crate::testdata::scratch(&format!("save_thread-{test}"))
    }

    /// A custom game recorded to `dir`, some frames in, and its texts.
    fn game(dir: &Path) -> (Sim, Session, Text<'static>) {
        const FRAMES: u32 = 50;

        let cli = crate::cli::tests::parsed(["openregent", "--map-seed", "3"]);
        let (begun, catalogue) = crate::game::begin::test_setup(&cli);
        let (mut sim, session) = (begun.sim, begun.session);
        let catalogue = Box::leak(Box::new(catalogue));

        sim.record(Recorder::open(
            &dir.join("play.input"),
            &dir.join("play.input.1"),
            &cli.recorded(),
        ));

        for _ in 0..FRAMES {
            sim.step();
        }

        (sim, session, Text::build(catalogue))
    }

    fn interface() -> save::Interface {
        save::Interface {
            selected: None,
            reading: None,
            pan: (1.0, 2.0),
            zoom: 1.0,
            tracking: regent_core::view::panel::tracking::Tracker::default(),
        }
    }

    /// A job of `sim`'s game named `name`, to `place`.
    fn job(
        (sim, session, text): (&Sim, &Session, &Text<'_>),
        name: &str,
        (place, kind): (Place, Kind),
    ) -> Job {
        Job {
            game: FromSim::of(sim),
            described: save::described(session, text),
            minimap: None,
            interface: interface(),
            name: name.to_owned(),
            profile: "p".to_owned(),
            place,
            kind,
        }
    }

    /// A save sent returns before it is written, the writing held: the
    /// file is made on the save thread, and comes back done as a save
    /// written in the frame was, but for the date.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_save_is_written_off_the_asking_thread_as_one_written_in_place() {
        let dir = scratch("off_thread");
        let (sim, session, text) = game(&dir);
        let here = dir.join(format!("here{}", save::EXTENSION));
        let there = dir.join(format!("there{}", save::EXTENSION));
        let c = save::of((FromSim::of(&sim), &session), &text, None, interface()).expect("made");
        let meta = save::Meta {
            name: "a".to_owned(),
            profile: "p".to_owned(),
            ..c.meta
        };

        save::write(&here, &save::Contents { meta, ..c }).expect("written");

        let mut writer = SaveThread::default();
        let held = WRITING.lock().expect("the lock");
        let to = (Place::File(there.clone()), Kind::Asked);

        writer.send(job((&sim, &session, &text), "a", to));
        assert!(writer.done().is_empty() && !there.exists(), "held");

        drop(held);

        let done = writer.finish();

        assert_eq!(done.len(), 1);
        assert_eq!(done[0].kind, Kind::Asked);
        assert_eq!(done[0].written, Ok((there.clone(), "a".to_owned())));

        let (here, mut there) = (save::read(&here), save::read(&there));

        if let (Ok(here), Ok(there)) = (&here, &mut there) {
            there.meta.saved = here.meta.saved;
        }

        assert_eq!(here.expect("read"), there.expect("read"));
    }

    /// Saves queued at once are written one after another, in order:
    /// two to one file leave the last; two LAN copies of one name find
    /// two names; autosaves number on. A save that will not write comes
    /// back saying why.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn queued_saves_are_written_in_order_one_at_a_time() {
        let dir = scratch("queued");
        let (sim, session, text) = game(&dir);
        let game = (&sim, &session, &text);
        let file = dir.join(format!("x{}", save::EXTENSION));
        let lan = dir.join("lan");
        let blocked = dir.join("play.input").join(format!("y{}", save::EXTENSION));
        let nick = || Kind::Lan {
            nick: "n".to_owned(),
        };
        let mut writer = SaveThread::default();

        for (name, to) in [
            ("first", (Place::File(file.clone()), Kind::Asked)),
            ("second", (Place::File(file.clone()), Kind::Asked)),
            ("x", (Place::Free(lan.clone()), nick())),
            ("x", (Place::Free(lan.clone()), nick())),
            ("auto", (Place::Autosave(dir.to_path_buf()), Kind::Autosave)),
            ("auto", (Place::Autosave(dir.to_path_buf()), Kind::Autosave)),
            ("y", (Place::File(blocked), Kind::Asked)),
        ] {
            writer.send(job(game, name, to));
        }

        let done = writer.finish();
        let names: Vec<Option<&str>> = done
            .iter()
            .map(|d| d.written.as_ref().ok().map(|(_, n)| n.as_str()))
            .collect();
        let auto = |n| dir.join(format!("autosave.{n}{}", save::EXTENSION));

        assert_eq!(
            names,
            [
                Some("first"),
                Some("second"),
                Some("x"),
                Some("x 2"),
                Some("auto"),
                Some("auto"),
                None
            ]
        );
        assert_eq!(
            save::read(&file).map(|c| c.meta.name),
            Ok("second".to_owned())
        );
        assert_eq!(save::list(&lan).len(), 2);
        assert!(auto(1).exists() && auto(2).exists());
        assert_eq!(done[6].name, "y");
    }

    /// A save still queued as the game ends is written before the save
    /// thread goes.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_save_queued_as_the_game_ends_is_written() {
        let dir = scratch("ends");
        let (sim, session, text) = game(&dir);
        let file = dir.join(format!("x{}", save::EXTENSION));
        let mut writer = SaveThread::default();

        writer.send(job(
            (&sim, &session, &text),
            "x",
            (Place::File(file.clone()), Kind::Asked),
        ));
        drop(writer);

        assert_eq!(save::read(&file).map(|c| c.meta.name), Ok("x".to_owned()));
    }
}
