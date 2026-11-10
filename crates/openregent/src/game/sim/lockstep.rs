//! The lockstep side of a game: the peer, digests, and a resync.

use super::*;

impl Sim {
    /// The same game on the LAN with `peer`, giving its orders the
    /// peer's delay ahead ([`Game::with_delay`]).
    #[must_use]
    pub fn networked(self, peer: Peer) -> Sim {
        Sim {
            game: self.game.with_delay(peer.delay()),
            peer,
            ..self
        }
    }

    pub const fn peer(&self) -> &Peer {
        &self.peer
    }

    pub const fn peer_mut(&mut self) -> &mut Peer {
        &mut self.peer
    }

    /// The peer's packets in and out at `now`; a desync found stops the
    /// game. A game stopped leaves the others (`Peer::stopped`).
    pub fn pump(&mut self, now: std::time::Instant) {
        self.peer.pump(now);
        self.check();

        if self.stopped.is_some() {
            self.peer.stopped();
        }
    }

    /// Whether the next frame may run: every seat's orders for it are
    /// in, and no desync. Always, alone.
    pub fn ready(&self) -> bool {
        self.peer.ready()
    }

    /// A desync, once found: the snapshot written, a deep digest's the
    /// snapshot it folded, a quick one's now's. Then the resync's turn:
    /// the host sends its game and goes on from it, a guest goes on from
    /// the host's once come; or the game stops.
    pub(super) fn check(&mut self) {
        if let Some(parted) = self.peer.desync() {
            match parted.held {
                Some((frame, snapshot)) => self.peer.dump(Ok(snapshot), frame),
                None => self.peer.dump(self.snapshot(), self.frames()),
            }
        }

        match self.peer.turn() {
            Some(Turn::Offer(frame)) => {
                let sent = self.payload(frame).and_then(|bytes| {
                    self.peer.offer(frame, bytes.clone());
                    self.resync(self.peer.epoch(), &bytes)
                });

                // The guests wait on the host's file: told, they stop too.
                if let Err(why) = sent {
                    let why = format!("resync: {why}");

                    self.peer.give_up(&why);
                    self.stopped = Some(why);
                }
            }
            Some(Turn::Resume(epoch, bytes)) => {
                if let Err(why) = self.resync(epoch, &bytes) {
                    self.stopped = Some(format!("resync: {why}"));
                }
            }
            Some(Turn::Stop(why)) => self.stopped = Some(why),
            None => {}
        }
    }

    /// The host's resync file, the desync at `frame`: the game as this
    /// step leaves it, and the recording so far.
    pub(super) fn payload(&self, frame: u32) -> Result<Vec<u8>, String> {
        let payload = Payload {
            steps: self.peer.steps(),
            frame,
            game: self.snapshot()?,
            play: self.session().map(|s| s.text()).unwrap_or_default(),
        };

        payload.encode()
    }

    /// The game going on from a resync's file of `epoch`: the host's
    /// game resumed over this world, whose every saved part it replaces
    /// (`World::restore`), so as over one made afresh; the recording
    /// begun again as the host's, so every peer's is again one file; the
    /// lockstep begun again at the host's step. The file is kept beside
    /// the desync's snapshots.
    pub(super) fn resync(&mut self, epoch: u32, bytes: &[u8]) -> Result<(), String> {
        let (payload, text) = Payload::decode(bytes)?;
        let session = crate::game::record::parse(&payload.play)?;

        if let Some(dir) = self.peer.dumps() {
            let name = format!(
                "resync-player{}-frame{}.ron",
                self.peer.seat(),
                payload.game.game.frames
            );
            let path = dir.join(name);

            match std::fs::write(&path, text) {
                Ok(()) => info!("net: the resync's file in {}", path.display()),
                Err(e) => warn!("net: the resync's file not kept: {e}"),
            }
        }

        let vm = Vm::new(self.program(), self.game.vm().config());
        let world = self.game.world().clone();
        let seed = payload.game.game.host.seed;
        let game = Game::resumed(vm, world, payload.game).map_err(|e| e.to_string())?;

        self.game = game.with_delay(self.peer.delay());
        self.sparks = Sparks::new(seed, self.game.world().now());
        // What the frames left goes with the world they left it in.
        self.output = Output::default();

        let world = self.game.world();

        self.camp = world.camp_held(self.local);
        self.said = report::Said::of(&self.game);
        self.replay.restart(self.game.frames);
        // A track the music's callback chose went with the old world.
        self.heard = None;

        if let Some(r) = &mut self.replay.recorder {
            r.restart(&session);
        }

        self.peer.resumed(payload.steps, epoch);
        self.game.set_delay(self.peer.delay());
        self.resyncs += 1;
        Ok(())
    }

    /// How many resyncs this game went on from: each a world unlike the
    /// one before, which a drawing takes as a new game's.
    pub const fn resyncs(&self) -> u32 {
        self.resyncs
    }

    /// A resync to `host`'s game, its ground generation made this one's
    /// (`Board::relaid`): another board a drawing keyed on the
    /// generation alone would not see. A test's.
    #[cfg(test)]
    pub fn resync_to(&mut self, host: &Sim) -> Result<(), String> {
        let mut game = host.snapshot()?;

        game.world.board.relaid = self.world().board().relaid();

        let payload = Payload {
            steps: self.peer.steps(),
            frame: self.frames(),
            game,
            play: self.session().map(|s| s.text()).unwrap_or_default(),
        };

        self.resync(self.peer.epoch() + 1, &payload.encode()?)
    }

    /// The game's cheap digest ([`Game::digest`]).
    pub fn digest(&self) -> u64 {
        self.game.digest()
    }

    /// The game's whole digest ([`Game::deep_digest`]).
    ///
    /// # Errors
    ///
    /// A game mid-run.
    pub fn deep_digest(&self) -> Result<u64, String> {
        self.game.deep_digest().map_err(|e| e.to_string())
    }

    /// This machine's orders sealed and sent, every seat's for the frame
    /// taken. With a deep digest due, the snapshot goes with them.
    pub(super) fn exchange(&mut self) -> Vec<(u32, Given)> {
        // The deep digest's snapshot, as a save holds it: folded aside.
        let snapshot = self
            .peer
            .deep_due()
            .then(|| crate::perf::timed(crate::perf::Timer::Snapshot, || self.snapshot().ok()))
            .flatten();
        // After a lowering of the delay a step may seal nothing: the
        // orders wait in the outbox for the one that does.
        let held = if self.peer.seals() {
            std::mem::take(&mut self.outbox)
        } else {
            Vec::new()
        };
        // A digest is never given: each peer records its own.
        let sent = held
            .into_iter()
            .filter_map(|event| match event {
                Event::Input { order, .. } => Some(Given::Order(order)),
                Event::PlayOn { .. } => Some(Given::PlayOn),
                Event::Surrender { .. } => Some(Given::Surrender),
                Event::Digest { .. } => None,
            })
            .collect();
        let game = &self.game;
        let given = self.peer.exchange(sent, || game.digest(), snapshot);

        // A delay the exchange took shows in the frame orders name.
        self.game.set_delay(self.peer.delay());
        given
    }

    /// What the seats gave, taken as events after frame `ran`.
    ///
    /// The frame an own order named is set again as it comes back:
    /// every peer stands still alike at a game's end. This machine's
    /// CONTINUE PLAYING comes back as its seat's: the local player's,
    /// though alone the seat is 0 whoever plays.
    pub(super) fn take_given(&mut self, ran: u32, given: Vec<(u32, Given)>) {
        let (own, local) = (self.peer.seat(), self.local());

        for (seat, given) in given {
            let event = match given {
                Given::Order(order) => Event::Input {
                    before: ran + 1,
                    order,
                },
                Given::PlayOn => Event::PlayOn {
                    after: ran,
                    player: if seat == own { local } else { seat },
                },
                Given::Surrender => Event::Surrender {
                    after: ran,
                    player: if seat == own { local } else { seat },
                },
            };

            self.take(event);
        }
    }
}
