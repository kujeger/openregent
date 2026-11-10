//! Events in and out: this machine's orders held for the peer, a
//! recording written, a replay fed back.

use super::*;

impl Sim {
    /// `event`, this machine's: held for the next bundle. Play parts
    /// from a replay's recording here: its digests left go unchecked.
    pub(super) fn give(&mut self, event: Event) {
        self.replay.script.clear();
        self.outbox.push(event);
    }

    /// This machine's orders given and not yet taken: a save holds them
    /// as taken, as single player took each as given before the outbox.
    /// None with peers: there each reaches the game a delay on, at a
    /// frame set as it comes back, and a save is every peer's alike.
    pub(super) fn given(&self) -> impl Iterator<Item = Event> + '_ {
        let alone = self.peer.players() == 1;

        self.outbox
            .iter()
            .copied()
            .filter(move |e| alone && matches!(e, Event::Input { .. }))
    }

    /// The session recorded so far, from the game's start, and the
    /// orders given since the last frame.
    pub fn session(&self) -> Option<crate::game::record::Session> {
        self.replay.recorder.as_ref().map(|r| {
            let mut session = r.session().clone();

            session.events.extend(self.given());
            session
        })
    }

    /// Record every input to `recorder` from now on.
    pub fn record(&mut self, recorder: Recorder) {
        self.replay.recorder = Some(recorder);
    }

    /// Play `events` back, each at its frame; live input waits for the
    /// last.
    pub fn replay(&mut self, events: Vec<Event>) {
        self.replay.script = events.into();
    }

    /// Whether a replay's events are still to come; digests are not.
    pub fn replaying(&self) -> bool {
        self.replay
            .script
            .iter()
            .any(|e| !matches!(e, Event::Digest { .. }))
    }

    /// The last frame a replay's recorded digest matched.
    pub const fn matched(&self) -> Option<u32> {
        self.replay.matched
    }

    /// The replay's events due between the last frame and the next.
    pub(super) fn scripted(&mut self) {
        let now = self.game.frames;
        let due = |e: &Event| match *e {
            Event::Input { before, .. } => before <= now + 1,
            Event::PlayOn { after, .. }
            | Event::Surrender { after, .. }
            | Event::Digest { after, .. } => after <= now,
        };

        while let Some(event) = self.replay.script.front().filter(|e| due(e)).copied() {
            self.replay.script.pop_front();

            match event {
                Event::Digest { after, digest } => self.verify(after, digest),
                _ => self.take(event),
            }
        }
    }

    /// `event`, recorded, now: the one way a change from outside reaches
    /// the game.
    ///
    /// The live game steps frames by wall time (`game::clock::tick`) and its
    /// panels, fades and music run on the real clock, so a replay matches
    /// only while every change reaching the simulation comes through here
    /// with its frame. The panels' queries and the music callback run aside
    /// (`Game::aside`); the reward settings and told lines reach no play
    /// (`the_rewards_and_told_lines_reach_no_play`).
    pub(super) fn take(&mut self, event: Event) {
        if let Some(r) = &mut self.replay.recorder {
            r.event(&event);
        }

        match event {
            Event::Input { before, order } => {
                // An order for a frame already run would never be decoded:
                // the game stops, as this machine's play would part from
                // the rest.
                if let Err(late) = self.game.input(before, order) {
                    error!("order {order}: {late}");
                    self.stopped = Some(self.stop_report(&late.to_string()));
                }
            }
            Event::PlayOn { player, .. } => self.game.play_on(player),
            Event::Surrender { player, .. } => {
                // A replay's own surrender spectates as the live one did.
                if player == self.local {
                    self.seat = Seat::Spectates;
                }
                self.game.surrender(player);
            }
            Event::Digest { .. } => {}
        }
    }

    /// Every [`DIGEST_EVERY`] frames run, the digest recorded; a replay's
    /// recorded digests due compared with it.
    pub(super) fn digests(&mut self) {
        let now = self.game.frames;

        if now != self.replay.digested && now.is_multiple_of(DIGEST_EVERY) {
            let event = Event::Digest {
                after: now,
                digest: self.game.digest(),
            };

            if let Some(r) = &mut self.replay.recorder {
                r.event(&event);
            }

            self.replay.digested = now;
        }

        while let Some(&Event::Digest { after, digest }) =
            self.replay.script.front().filter(|e| e.frame() <= now)
        {
            self.replay.script.pop_front();
            self.verify(after, digest);
        }
    }

    /// A replay's recorded `digest` of frame `after` against the game's:
    /// one that differs stops the game, its snapshot beside the
    /// recording.
    ///
    /// ```text
    /// replay: frame 700's digest is 3f2a9c0d1e4b5a67 at frame 700, recorded 0123456789abcdef; last match frame 600
    /// ```
    pub(super) fn verify(&mut self, after: u32, digest: u64) {
        let now = self.game.frames;
        let ours = self.game.digest();

        if after == now && ours == digest {
            self.replay.matched = Some(after);
        } else if self.stopped.is_none() {
            let last = self.replay.matched.map_or_else(
                || "none matched".to_owned(),
                |f| format!("last match frame {f}"),
            );
            let why = format!(
                "replay: frame {after}'s digest is {ours:016x} at frame {now}, recorded \
                 {digest:016x}; {last}"
            );

            error!("{why}");

            if let Some(dir) = self.replay.recorder.as_ref().and_then(Recorder::dir) {
                crate::net::dump(&dir.join(format!("replay-frame{now}.ron")), self.snapshot());
            } else {
                warn!("replay: no snapshot: not recorded");
            }

            self.stopped = Some(why);
        }
    }
}

/// A recording written and a replay fed back.
pub(super) struct Replay {
    /// Where each input goes as it is taken.
    pub(super) recorder: Option<Recorder>,
    /// A replay's events still to come, in order.
    pub(super) script: VecDeque<Event>,
    /// The last frame whose digest was recorded, or the first's.
    digested: u32,
    /// The last frame a replay's recorded digest matched.
    matched: Option<u32>,
}

impl Replay {
    /// None of either, for a game at frame `frames`.
    pub(super) const fn from(frames: u32) -> Replay {
        Replay {
            recorder: None,
            script: VecDeque::new(),
            digested: frames,
            matched: None,
        }
    }

    /// A resynced game at frame `frames`: its replay dropped, no digest
    /// matched; the recorder kept.
    pub(super) fn restart(&mut self, frames: u32) {
        self.digested = frames;
        self.matched = None;
        self.script.clear();
    }
}
