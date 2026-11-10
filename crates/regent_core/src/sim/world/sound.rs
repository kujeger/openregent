//! Sounds the game starts, as cues: what, from where, when. Kept for a
//! front end; nothing here changes game state. **Binary** unless marked.
//!
//! ```text
//!  PlaySound(agent, phase[, float])          the unit's own
//!  PlaySound(agent, sound, phase[, float])   DefaultSound, or
//!  PlaySound(sound, phase[, player])         a DSND by name
//!  InitAction pushed, and at its peak
//!    the action's Sound, else the actor's; SoundPhase begin, then peak
//!  an effector made                          GenUnit
//!    its own DefaultSound, phase EEB0 Effector Begin
//!  a player's spell unit made
//!    the same
//!  a player's building placed
//!    PB01 Place_Building, phase EBE0 Begin, at it
//!  a rangers' camp moved
//!    the same, at the guild
//!  a reward flag made
//!    PR01 Place_Reward for an attack flag, else PE01 Place_Explore; Begin
//!  research done
//!    the building's advisor (ADVISORS), phase GVS5
//! ```
//!
//! The last four the sound manager plays straight, as the owner's, asking
//! no sight; the flag's and the advisor's
//! unplaced.
//!
//! A building has no tracks: its action's begin sounds as it is asked
//! (**Inferred**), its peak not at all.
//!
//! The player's selection, a unit's ambient loop and the music are the
//! front end's: [`crate::view::sound`].

use crate::base::gfix::Gfix;
use crate::data::fourcc::FourCc;
use crate::gpl::ops::Value;
use crate::gpl::vm::{Fault, Handle, Vm};

use super::{Answer, Found, World, put_int};

/// Phase `EEB0`, `Effector Begin`: what an effector and a player's spell
/// unit play as they are made.
pub const EFFECTOR_BEGIN: FourCc = FourCc(*b"EEB0");

/// Phase `EBE0`, `Begin`.
pub const BEGIN: FourCc = FourCc(*b"EBE0");

/// `PB01 Place_Building`: create building decode plays its `Begin`
/// at the new building.
pub const PLACE_BUILDING: FourCc = FourCc(*b"PB01");

/// `PE01 Place_Explore`: a reward flag's `Begin`.
pub const PLACE_EXPLORE: FourCc = FourCc(*b"PE01");

/// `PR01 Place_Reward`: an attack flag's instead.
pub const PLACE_REWARD: FourCc = FourCc(*b"PR01");

/// Phase `GVS5`, the advisor's voice.
pub const ADVISOR: FourCc = FourCc(*b"GVS5");

/// The advisor who says a building's research is done, by the code's
/// first three letters.
/// Any other plays record 0: nothing (**Inferred**).
pub const ADVISORS: [(&[u8; 3], FourCc); 7] = [
    (b"ABC", FourCc(*b"RA12")),
    (b"ABD", FourCc(*b"RA13")),
    (b"ABE", FourCc(*b"RA18")),
    (b"ABG", FourCc(*b"RA15")),
    (b"ABH", FourCc(*b"RA16")),
    (b"ABL", FourCc(*b"RA17")),
    (b"ABl", FourCc(*b"RA34")),
];

/// Where a cue sounds from.
#[derive(Clone, Debug, PartialEq)]
pub enum From {
    /// A unit, by agent: placed at it, heard by distance.
    Agent(u32),
    /// No place: full volume, centred (a null position, `CYSndBuffer`).
    Nowhere,
}

/// Which `DSND` record a cue plays.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Voice {
    /// The source unit's `DefaultSound`, `PRIM`'s sound tail
    /// (descriptor).
    Own,
    /// The `DefaultSound` of the `DUNT` record of this folded `HEAD` name.
    Unit(String),
    /// A `DSND` by id.
    Sound(FourCc),
    /// A `DSND` by `HEAD` name, as a script names it (desc find name,
    /// folding case).
    Named(String),
}

/// Which phase of the record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Phase {
    /// A `DSDP` id.
    Id(FourCc),
    /// A `DSDP` `HEAD` name, as a script names it.
    Named(String),
    /// Phase 0: the record's first (`sound_phase_at(0)` in sound new
    /// and `GE_SoundManager`).
    First,
}

/// A sound the engine starts.
#[derive(Clone, Debug, PartialEq)]
pub struct Cue {
    /// Game ms.
    pub at: u32,
    pub from: From,
    pub voice: Voice,
    pub phase: Phase,
    /// `DistanceModifier`; 0 takes the phase's own.
    pub distance: f32,
    /// Only this player hears it: `PlaySound`'s third form, and a human
    /// seat's own (`World::cue_seat`).
    pub hearer: Option<i32>,
    /// Played by the sound manager straight, as
    /// this player's: no sight test. `None`: through its unit,
    /// as the unit's owner's.
    pub owned_by: Option<u32>,
}

/// The music player's state.
#[derive(Clone, Debug, Default)]
pub(super) struct Music {
    /// `PlayMusic`'s track.
    track: Option<i32>,
    /// `SetMusicStoppedCallback`'s script.
    stopped: Option<String>,
}

impl World {
    /// `PlayMusic(track)`: the music player's track.
    pub(super) fn play_music(
        &mut self,
        vm: &Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<(), Fault> {
        self.outbox.music.track = Some(super::int_ref(vm, args, 1)?);
        self.output.music_played = true;
        Ok(())
    }

    /// `SetMusicStoppedCallback(function)`: what the music player
    /// dispatches when a track ends.
    pub(super) fn music_callback(
        &mut self,
        vm: &Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<(), Fault> {
        self.outbox.music.stopped = Some(super::string_ref(vm, args, 1)?);
        Ok(())
    }

    /// The scripts' track: `PlayMusic`'s last, if any. A machine whose
    /// track ended plays the stop callback's instead, until the next
    /// `PlayMusic` ([`super::Output::music_played`]): its own, kept by its
    /// front end (`Game::music_stopped`).
    #[must_use]
    pub const fn music(&self) -> Option<i32> {
        self.outbox.music.track
    }

    /// The music player's track set to `track`, as the stop callback's
    /// run aside finds it: this machine's (`Game::music_stopped`).
    pub(crate) const fn set_music(&mut self, track: i32) {
        self.outbox.music.track = Some(track);
    }

    /// `SetMusicStoppedCallback`'s script, if any.
    #[must_use]
    pub fn music_stopped(&self) -> Option<&str> {
        self.outbox.music.stopped.as_deref()
    }

    /// The cues not yet taken, oldest first.
    #[must_use]
    pub fn cues(&self) -> &[Cue] {
        &self.output.cues
    }

    /// Queues a cue at the world's clock.
    pub(super) fn cue(&mut self, from: From, voice: Voice, phase: Phase, distance: f32) {
        let at = self.now;

        self.output.cues.push(Cue {
            at,
            from,
            voice,
            phase,
            distance,
            hearer: None,
            owned_by: None,
        });
    }

    /// Queues a cue the sound manager plays straight, as `owner`'s:
    /// `phase` of record `sound`.
    pub(super) fn cue_owned(&mut self, from: From, sound: FourCc, phase: FourCc, owner: u32) {
        self.cue(from, Voice::Sound(sound), Phase::Id(phase), 0.0);

        if let Some(c) = self.output.cues.last_mut() {
            c.owned_by = Some(owner);
        }
    }

    /// A cue as [`World::cue_owned`]'s, `seat`'s and heard by `seat`
    /// alone: the engine played it on its human's machine only.
    pub(super) fn cue_seat(&mut self, sound: FourCc, phase: FourCc, seat: u32) {
        self.cue_owned(From::Nowhere, sound, phase, seat);

        if let Some(c) = self.output.cues.last_mut() {
            c.hearer = Some(seat.cast_signed());
        }
    }

    /// `PlaySound`, gpl play sound: three forms, told by argument 1's type
    /// and the count; true always.
    ///
    /// - agent, phase, \[float\]: the unit's own record; no unit, nothing.
    /// - agent, sound, phase, \[float\]: no unit, played with no place.
    /// - sound, phase, \[player\]: no place; another player's is skipped
    ///   there, by the local player, so kept here with its hearer.
    ///
    /// The float is `DistanceModifier` (`* 1/1024`); the third form gives
    /// 10,000. A unit's play is `GenUnit`.
    pub(super) fn play_sound(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<(), Fault> {
        let value = |i: usize| args.get(i).and_then(|h| vm.value(*h)).cloned();
        let text = |v: Option<Value>| match v {
            Some(Value::String(s)) => Some(s),
            _ => None,
        };
        let float = |v: Option<Value>| match v {
            Some(Value::Float(f)) => fixed_to_f32(f.0),
            _ => 0.0,
        };

        match value(1) {
            Some(Value::Agent(id)) => {
                let unit = matches!(self.found(id), Found::Piece(_));
                let third = value(3);
                let own = args.len() == 3 || matches!(third, Some(Value::Float(_)));

                if own {
                    let phase = text(value(2));

                    if let (true, Some(phase)) = (unit, phase) {
                        self.cue(
                            From::Agent(id),
                            Voice::Own,
                            Phase::Named(phase),
                            float(third),
                        );
                    }
                } else if let (Some(sound), Some(phase)) = (text(value(2)), text(third)) {
                    let from = if unit { From::Agent(id) } else { From::Nowhere };

                    self.cue(
                        from,
                        Voice::Named(sound),
                        Phase::Named(phase),
                        float(value(4)),
                    );
                }
            }
            first => {
                if let (Some(sound), Some(phase)) = (text(first), text(value(2))) {
                    let hearer = match value(3) {
                        Some(Value::Integer(p)) => Some(p),
                        _ => None,
                    };
                    let distance = if hearer.is_some() { GLOBAL } else { 0.0 };

                    self.cue(
                        From::Nowhere,
                        Voice::Named(sound),
                        Phase::Named(phase),
                        distance,
                    );

                    if let Some(c) = self.output.cues.last_mut() {
                        c.hearer = hearer;
                    }
                }
            }
        }

        let ret = vm.arg(args, 0)?;

        super::put_bool(vm, ret, true)
    }

    /// `LastMusicTrack()`, gpl last music track: the music player's,
    /// which `PlayMusic` writes with its track
    /// and nothing else here: [`World::music`]. Unknown before
    /// any `PlayMusic`.
    pub(super) fn last_track(
        &self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        match self.music() {
            Some(track) => {
                let ret = vm.arg(args, 0)?;

                put_int(vm, ret, track).map(|()| Answer::Given)
            }
            None => Ok(Answer::Unknown),
        }
    }
}

/// `DistanceModifier` 10,000: heard at full volume, anywhere
/// (`0x461c4000`, gpl play sound).
pub const GLOBAL: f32 = 10_000.0;

/// A GPL float, fixed point, as `float`.
#[allow(clippy::cast_precision_loss, reason = "a GPL float is 21 bits")]
fn fixed_to_f32(f: i32) -> f32 {
    f as f32 / Gfix::ONE.0 as f32
}

mod persist {
    use super::*;
    use crate::save::{self as m, Refused, Saved};
    use std::result::Result;

    impl Saved for Music {
        type Model = m::Music;

        fn save(&self) -> m::Music {
            m::Music {
                track: self.track,
                stopped: self.stopped.clone(),
            }
        }

        fn load(model: m::Music) -> Result<Music, Refused> {
            Ok(Music {
                track: model.track,
                stopped: model.stopped,
            })
        }
    }
}
