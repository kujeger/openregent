//! What a game says of itself: orders dropped, what the world lost,
//! script errors, and a stop.

use super::*;

impl Sim {
    /// The orders `Game::check` refused, to the log: every machine drops
    /// the same.
    pub(super) fn say_dropped(&mut self) {
        for dropped in self.game.take_dropped() {
            warn!("{dropped}");
        }
    }

    /// What the world lost track of, and where the host took a default
    /// for an answer, to the log: each first time as it comes, and every
    /// [`SUMMARY_FRAMES`] the counts since the last.
    ///
    /// ```text
    /// frame 812: world lost agent 31 (Peasant) at 40600 ms: Move(...) not followed (...)
    /// frame 812: host unknown LocationOf at 40600 ms in Go_Home, for agent 31 ...
    /// frame 1200: world since frame 0: 1 places lost, 40 orders declined, ...
    /// ```
    pub(super) fn say_lost(&mut self) {
        let frame = self.game.frames;

        for lost in self.game.take_lost() {
            info!("frame {frame}: world {lost}");
        }

        for fault in self.game.take_faults() {
            info!("frame {frame}: host {fault}");
        }

        let now = frame
            .is_multiple_of(SUMMARY_FRAMES)
            .then(|| Losses::of(self.world(), self.game.faults()))
            .filter(|now| *now != self.said.summed);

        if let Some(now) = now {
            let was = self.said.summed;

            info!(
                "frame {frame}: world since frame {}: {} places lost, {} orders declined, \
                 {} unknown answers",
                frame.saturating_sub(SUMMARY_FRAMES),
                now.places - was.places,
                now.declined - was.declined,
                now.unknown - was.unknown,
            );
            self.said.summed = now;
        }
    }

    /// Script errors since the last frame, to the log: those kept whole,
    /// the rest counted.
    pub(super) fn say_reports(&mut self) {
        let reports = self.game.reports();
        let frame = self.game.frames;

        for report in reports.first.iter().skip(self.said.reported) {
            warn!("script error in frame {frame}: {report}");
        }

        let kept = reports.first.len().saturating_sub(self.said.reported);
        let more = reports.count - self.said.reported - kept;

        if more > 0 {
            warn!("{more} more script errors in frame {frame}");
        }

        self.said.reported = reports.count;
    }

    /// Where the game stopped: frame, clock, the agent whose threads ran,
    /// and the GPL stack the crash unwound, innermost first.
    ///
    /// ```text
    /// frame 21097 at 1054850 ms: Run { at: 0, why: "Unending" }
    ///   agent 312 (Warrior)
    ///   in Spin line 5
    ///   in Outer line 13
    /// ```
    pub(super) fn stop_report(&self, why: &str) -> String {
        let world = self.game.world();
        let agent = self.game.running.map(|id| {
            let name = world.piece(id).map_or("no piece", |p| p.name.as_str());

            format!("\n  agent {id} ({name})")
        });
        let stack = self
            .game
            .vm()
            .unwound()
            .iter()
            .fold(String::new(), |mut out, site| {
                let _ = write!(out, "\n  in {site}");
                out
            });

        format!(
            "frame {} at {} ms: {why}{}{stack}",
            self.game.frames,
            world.now(),
            agent.unwrap_or_default()
        )
    }
}

/// Frames between summaries of what the world lost: a game minute.
const SUMMARY_FRAMES: u32 = 1_200;

/// Totals of what the world lost track of.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Losses {
    pub(super) places: usize,
    pub(super) declined: usize,
    pub(super) unknown: usize,
}

impl Losses {
    pub(super) fn of(world: &World, faults: &Faults) -> Losses {
        let counts = world.losses();

        Losses {
            places: counts.places,
            declined: counts.declined,
            unknown: faults.unknown.values().sum(),
        }
    }
}

/// What the log was last told, so each thing is said once.
#[derive(Default)]
pub(super) struct Said {
    /// Script errors said so far.
    reported: usize,
    /// What the world had lost at the last summary.
    pub(super) summed: Losses,
}

impl Said {
    /// All of `game`'s errors and losses as said: a resynced game's,
    /// which sums from itself.
    pub(super) fn of(game: &Game<'_>) -> Said {
        Said {
            reported: game.reports().count,
            summed: Losses::of(game.world(), game.faults()),
        }
    }
}
