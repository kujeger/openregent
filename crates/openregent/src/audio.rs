//! Sound and music, through Bevy's audio: the world's cues, units' ambient
//! loops, the selection's sound, and the scripts' music.
//!
//! ```text
//!  News::cues ─────> Sounds::play ──> heard? group free? ──> a Voiced entity
//!  each frame: view centre (cells), view width ──> sound::mix ──> its gains
//!  mirror::Pieces: a piece's set ──> sound::ambient ──> its loop, while in range
//!  Selected changed ──> sound::selection ──> a cue
//!  Interface (input's refused click: sound::refused) ──> a cue
//!  Sim::music n ──> Data/MusicTracks.txt line n ──> Music/<file>
//!    a track's end ──> Ask::MusicStopped (the scripts pick the next)
//!  Menu::music (a page's screen) ──> the same player; none asked: plays on
//!    a silent track ──> started again at the next poll
//!  World::ended ──> music and every sound stopped, the end's advisor;
//!    the track again at the next poll; the advisor done, a won quest's
//!    end voice (Narration)
//!  Narration (the menus) ──> one unplaced voice, stopped as asked
//!  ClickSounds (the menus' presses) ──> a WAVE each, straight
//!  [enhancements] wind: a gust peak nearing the view ──> AM03, quiet,
//!    riding the peak east
//! ```
//!
//! The rules are the library's ([`regent_core::view::sound`]); here only the
//! playing. A wave plays as decoded PCM at its rate, its gains set by the
//! mixer each frame, stereo always.

use std::collections::HashMap;
use std::num::NonZero;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use bevy::audio::{AddAudioSource, AudioPlayer, AudioSource, Decodable, PlaybackSettings, Source};
use bevy::prelude::*;
use bevy::transform::TransformSystems;

use regent_core::content::unit::Unit;
use regent_core::data::cam;
use regent_core::data::desc::spread;
use regent_core::data::fourcc::FourCc;
use regent_core::data::payload::Pcm;
use regent_core::sim::movement::action::Look;
use regent_core::sim::units;
use regent_core::sim::world::{
    self, World,
    draw_view::{Housed, Shown},
    sound::{Cue, Voice},
};
use regent_core::view::menu::{self, music as front_end};
use regent_core::view::sound::{
    self, Admit, Facing, Gate, Mix, Origin, Owner, Placing, Play, Seen, Selected as Selection,
    Sounds, Zoom,
};
use regent_draw::data::Corpus;
use regent_draw::map::Maps;
use regent_draw::map::reel::Reel;
use regent_draw::stage::View;
use regent_draw::wind::{self, Trees, Wind};

use crate::files::config::Settings;
use crate::files::env::Install;
use crate::game::scope::Scoped;
use crate::game::seen::{Ask, News, Outbox, Watch};
use crate::game::sim::Session;
use crate::map::mirror::Pieces;
use crate::map::select::Selected;
use crate::menu::{Screen, in_game};
use crate::ui::end;

/// The listener faces south, so a sound pans by its side of the screen;
/// the engine's faces east (`docs/openregent/enhancements.md`, no switch).
const FACING: Facing = Facing::SOUTH;

/// The music's track list, under the install.
pub(crate) const TRACKS: &str = "Data/MusicTracks.txt";

/// The music's directory, under the install.
pub(crate) const MUSIC: &str = "Music";

/// Sounds, music and the mixer: the mixer and the game sounds' state a
/// game's (`scope`); the music player, the menus' voice and clicks
/// outlast it.
pub struct Plugin;

/// Whether the app opens the audio device.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sound {
    /// Bevy's audio on the default device.
    Played,
    /// No device and no Bevy audio: screenshots and headless runs. Sounds
    /// are kept as asked, but never played nor ended.
    Muted,
}

/// Sounds of `T` played by Bevy's audio; muted (no [`bevy::audio::AudioPlugin`]),
/// only their assets kept.
pub(crate) trait AddSound {
    fn add_sound<T: Asset + Decodable>(&mut self) -> &mut Self;
}

impl AddSound for App {
    fn add_sound<T: Asset + Decodable>(&mut self) -> &mut Self {
        if self.is_plugin_added::<bevy::audio::AudioPlugin>() {
            self.add_audio_source::<T>()
        } else {
            self.init_asset::<T>()
        }
    }
}

/// The systems that start and stop sounds, run before Bevy's audio with
/// their commands in. A sound stopped the frame after it started goes
/// before Bevy's audio sees it: else Bevy's sink, inserted after the
/// despawn, panics. Bevy's audio set is private; it runs after
/// [`TransformSystems::Propagate`], so before that is before it.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Starts;

impl bevy::app::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        // Muted, the music's assets too, else registered by Bevy's audio.
        if !app.is_plugin_added::<bevy::audio::AudioPlugin>() {
            app.init_asset::<AudioSource>();
        }

        app.add_sound::<Wave>()
            .per_game::<Mixer>()
            .init_resource::<Music>()
            .per_game::<Interface>()
            .per_game::<Voices>()
            .per_game::<Finale>()
            .per_game::<Farewell>()
            .per_game::<Spoken>()
            .per_game::<Speaker>()
            .per_game::<Gusts>()
            .init_resource::<Narration>()
            .init_resource::<ClickSounds>()
            .configure_sets(PostUpdate, Starts.before(TransformSystems::Propagate))
            .add_systems(PostUpdate, (narrate, click).in_set(Starts))
            .add_systems(
                PostUpdate,
                (front.run_if(in_menus), loudness).chain().in_set(Starts),
            )
            .add_systems(OnEnter(Screen::Playing), index)
            .add_systems(OnExit(Screen::Playing), farewell_hush)
            .add_systems(
                PostUpdate,
                (
                    end::mark,
                    listen,
                    (cues, select, ambient, gust).run_if(end::playing),
                    finale,
                    start,
                    mix,
                    farewell,
                    music.run_if(end::playing),
                    encore.run_if(not(end::playing)),
                )
                    .chain()
                    .run_if(in_game)
                    .before(loudness)
                    .in_set(Starts),
            );
    }
}

// --------------------------------------------------------------------------
// A playing wave
// --------------------------------------------------------------------------

/// One play of a decoded `WAVE`: its samples, rate, loop, and the gains
/// the mixer writes.
#[derive(Asset, TypePath, Clone)]
pub struct Wave {
    pcm: Arc<Decoded>,
    rate: u32,
    looping: Looping,
    gains: Arc<Gains>,
}

/// A `WAVE`'s samples as stored: 16-bit, channels interleaved. Half the
/// size of `f32`; scaled to -1 to 1 as played ([`Playback`]).
struct Decoded {
    samples: Vec<i16>,
    channels: u16,
    rate: u32,
}

/// A 16-bit sample's full scale: `i16::MAX` plays as 1.0.
const FULL_SCALE: f32 = 32_767.0;

impl Decoded {
    fn of(pcm: &Pcm) -> Self {
        Decoded {
            samples: pcm.samples().collect(),
            channels: pcm.channels,
            rate: pcm.rate,
        }
    }

    /// How long it plays at `rate` samples a second, e.g. 22 050 frames at
    /// 22 050 Hz: one second.
    fn length(&self, rate: u32) -> Duration {
        let frames = self.samples.len() / usize::from(self.channels.max(1));
        let frames = u64::try_from(frames).unwrap_or(u64::MAX);

        Duration::from_secs(frames) / rate.max(1)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Looping {
    Once,
    Forever,
}

/// Left and right amplitudes, as `f32` bits: written by the mixer, read
/// by the audio thread.
#[derive(Default)]
struct Gains {
    left: AtomicU32,
    right: AtomicU32,
}

impl Gains {
    fn set(&self, (left, right): (f32, f32)) {
        self.left.store(left.to_bits(), Ordering::Relaxed);
        self.right.store(right.to_bits(), Ordering::Relaxed);
    }

    fn get(&self) -> (f32, f32) {
        (
            f32::from_bits(self.left.load(Ordering::Relaxed)),
            f32::from_bits(self.right.load(Ordering::Relaxed)),
        )
    }
}

/// A [`Wave`] being played: stereo out, a frame at a time.
pub struct Playback {
    wave: Wave,
    /// Output samples given: two a frame.
    at: usize,
    gains: (f32, f32),
}

impl Iterator for Playback {
    type Item = bevy::audio::Sample;

    fn next(&mut self) -> Option<Self::Item> {
        let pcm = &self.wave.pcm;
        let channels = usize::from(pcm.channels.max(1));
        let frames = pcm.samples.len() / channels;
        let (frame, side) = (self.at / 2, self.at % 2);
        let frame = match (frame < frames, self.wave.looping) {
            (true, _) => Some(frame),
            (false, Looping::Forever) if frames > 0 => {
                self.at = side;
                Some(0)
            }
            _ => None,
        }?;

        // Gains change between frames, never inside one.
        if side == 0 {
            self.gains = self.wave.gains.get();
        }

        let channel = if channels == 1 { 0 } else { side };
        let gain = if side == 0 {
            self.gains.0
        } else {
            self.gains.1
        };

        // Scaled, then gained: the f32 cache's order, so the same bits.
        let sample = f32::from(pcm.samples[frame * channels + channel]) / FULL_SCALE;

        self.at += 1;
        Some(sample * gain)
    }
}

impl Source for Playback {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> bevy::audio::ChannelCount {
        NonZero::new(2).expect("two")
    }

    fn sample_rate(&self) -> bevy::audio::SampleRate {
        NonZero::new(self.wave.rate.max(1)).expect("not zero")
    }

    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

impl Decodable for Wave {
    type Decoder = Playback;

    fn decoder(&self) -> Playback {
        Playback {
            wave: self.clone(),
            at: 0,
            gains: (0.0, 0.0),
        }
    }
}

// --------------------------------------------------------------------------
// The mixer
// --------------------------------------------------------------------------

/// What a sound is placed by.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Spot {
    /// A unit, by agent; where it was last seen.
    Agent(u32, [f32; 3]),
    /// A place in cells, moved by its owner: a gust's.
    Free([f32; 3]),
    Nowhere,
}

/// A sound playing.
#[derive(Component)]
struct Voiced {
    spot: Spot,
    /// The distance it is mixed at (`sound::heard`).
    distance: f32,
    /// Its volume spread, hundredths of a dB.
    offset: i32,
    /// A set pan, for an unplaced sound.
    pan: Option<i32>,
    group: Option<FourCc>,
    /// A voice phase's speaker.
    speaker: Option<u32>,
    /// A unit's ambient loop: the agent and phase.
    ambient: Option<(u32, FourCc)>,
    gains: Arc<Gains>,
}

/// When a once-played sound has played out, on [`Time<Real>`]'s clock,
/// whether or not the device took it: the end's advisor waits on it
/// ([`farewell`]), so a dead or missing device does not hold the end
/// panel back forever.
#[derive(Component)]
struct Due(Duration);

/// How long past its [`Due`] a sound may still be heard: the device's own
/// delay, so a live sound ends by itself first. A chosen margin, not the
/// original's.
const LATE: Duration = Duration::from_millis(500);

/// A sound to start: resolved, heard, placed.
#[derive(Clone, Debug)]
struct Pending {
    play: Play,
    spot: Spot,
    distance: f32,
    speaker: Option<u32>,
    ambient: Option<u32>,
    /// The unit it makes the last speaker, once started (`sound::speaks`).
    speaks: Option<u32>,
}

/// The local player's unit whose sound last played at a global distance:
/// where Backspace centres.
#[derive(Resource, Default)]
pub struct Speaker(pub Option<u32>);

/// The listener, the records, decoded waves and what waits to start.
#[derive(Resource, Default)]
pub(crate) struct Mixer {
    sounds: Sounds,
    /// The view's centre, cells.
    listener: [f32; 3],
    radius: f32,
    waves: HashMap<FourCc, Option<Arc<Decoded>>>,
    /// Each group's last start, real time.
    started: HashMap<FourCc, Instant>,
    /// Cues to start this frame, and those their group holds back.
    pending: Vec<Pending>,
    queued: Vec<Pending>,
    /// Ambient phases by descriptor and set.
    ambients: HashMap<(FourCc, u32), Option<Play>>,
    selected: Option<u32>,
    rng: u32,
}

impl crate::draw::shelf::Shelved for Mixer {
    fn kept(self) -> Self {
        Mixer {
            waves: self.waves,
            ..Mixer::default()
        }
    }
}

/// The records, indexed once. The start's selection is silent:
/// New game setup asks unit select maybe for no sound.
#[allow(clippy::needless_pass_by_value, reason = "a Bevy system")]
fn index(corpus: Res<Corpus>, selected: Res<Selected>, mut mixer: ResMut<Mixer>) {
    mixer.sounds = Sounds::of(&corpus.catalogue).unwrap_or_else(|e| {
        warn!("no sounds: {e}");
        Sounds::default()
    });
    mixer.selected = selected.0;
    mixer.rng = Instant::now().elapsed().subsec_nanos() | 1;
}

/// The listener: the view's centre in cells, and
/// the radius its width gives.
#[allow(clippy::needless_pass_by_value, reason = "a Bevy system")]
fn listen(view: Res<View>, maps: Res<Maps>, mut mixer: ResMut<Mixer>) {
    let origin = maps.aloft_at(Vec2::ZERO, 0.0);
    let across = maps.aloft_at(Vec2::X, 0.0) - origin;
    let down = maps.aloft_at(Vec2::Y, 0.0) - origin;
    let width = view.rect.map_or(0.0, |r| r.width());
    let zoom = if view.zoom < 1.0 {
        Zoom::Out
    } else {
        Zoom::Regular
    };

    if across.x != 0.0 && down.y != 0.0 {
        let cell = (view.pan - origin) / Vec2::new(across.x, down.y);

        mixer.listener = [cell.x, cell.y, 0.0];
    }

    mixer.radius = sound::radius(width, zoom);
}

/// A shown piece's place, cells; the height is not read (**Inferred**: 0,
/// as the listener's).
fn cells(shown: &Shown) -> [f32; 3] {
    let [x, y] = units::cells_f32(shown.at);

    [x, y, 0.0]
}

/// Interface sounds the input code asks for: unplaced, the local
/// player's, e.g. [`sound::refused`].
#[derive(Resource, Default)]
pub struct Interface(pub Vec<(Voice, world::sound::Phase)>);

/// Pieces whose voice the `"` key asked (`sound::voice`): each at itself,
/// as the local player's.
#[derive(Resource, Default)]
pub struct Voices(pub Vec<u32>);

/// The world's cues and the interface's, resolved and heard, into
/// `pending`.
#[allow(clippy::needless_pass_by_value, reason = "a Bevy system")]
fn cues(
    (watch, mut news): (Watch, ResMut<News>),
    corpus: Res<Corpus>,
    pieces: Pieces,
    (mut interface, mut voices): (ResMut<Interface>, ResMut<Voices>),
    mut mixer: ResMut<Mixer>,
) {
    let shown: HashMap<u32, &Shown> = pieces.iter().map(|s| (s.agent, s)).collect();
    let cues = std::mem::take(&mut news.cues);
    let (world, local) = (watch.world(), watch.local);

    for cue in cues {
        if let Some(p) = resolve(
            &mixer.sounds,
            &corpus,
            &shown,
            &cue,
            Via::Unit(world, local),
        ) {
            mixer.pending.push(p);
        }
    }

    // The interface's unplaced, then the `"` key's at their pieces.
    let unplaced = interface
        .0
        .drain(..)
        .map(|said| (world::sound::From::Nowhere, said));
    let placed = voices
        .0
        .drain(..)
        .map(|id| (world::sound::From::Agent(id), sound::voice()));

    for (from, (voice, phase)) in unplaced.chain(placed) {
        let cue = Cue {
            at: world.now(),
            from,
            voice,
            phase,
            distance: 0.0,
            hearer: None,
            owned_by: None,
        };

        if let Some(p) = resolve(&mixer.sounds, &corpus, &shown, &cue, Via::Selection(local)) {
            mixer.pending.push(p);
        }
    }
}

/// How a sound reaches the sound manager.
#[derive(Clone, Copy)]
enum Via<'w> {
    /// A world cue. `GenUnit`: as its unit's owner's; an unseen
    /// unit's only at [`sound::GLOBAL`]. With `owned_by`, straight:
    /// as that player's, sight not asked. The
    /// world, and the local player who hears.
    Unit(&'w World, u32),
    /// `GE_SoundManager`, the selection's and the interface's: as the
    /// local player's, this one, sight not asked.
    Selection(u32),
}

/// Whether player `local` sees unit `id`: the game's
/// sight, whatever the drawn fog.
fn seen(world: &World, id: u32, local: u32) -> Seen {
    Seen::of(world.sight_word(id, local))
}

/// What `cue` starts, if anyone hears it.
fn resolve(
    sounds: &Sounds,
    corpus: &Corpus,
    shown: &HashMap<u32, &Shown>,
    cue: &Cue,
    via: Via<'_>,
) -> Option<Pending> {
    let unit = match cue.from {
        world::sound::From::Agent(id) => Some(*shown.get(&id)?),
        world::sound::From::Nowhere => None,
    };
    let own = unit.map(|s| s.code);
    let play = sounds.play(&corpus.catalogue, &cue.voice, &cue.phase, own, cue.distance)?;
    let local = match via {
        Via::Unit(_, local) | Via::Selection(local) => local,
    };
    let whose = cue
        .owned_by
        .unwrap_or_else(|| unit.map_or(local, |s| s.owner));
    let owner = match via {
        Via::Selection(_) => Owner::Local,
        Via::Unit(..) if whose == local => Owner::Local,
        Via::Unit(..) => Owner::Other,
    };

    // An unplaced cue has no unit to ask; the sound manager asks none.
    let sight = match (via, &cue.from, cue.owned_by) {
        (Via::Unit(world, local), world::sound::From::Agent(id), None) => seen(world, *id, local),
        _ => Seen::Yes,
    };
    let distance = sound::heard(play.distance, owner, sight)?;
    let spot = match (&cue.from, unit) {
        (world::sound::From::Agent(id), Some(s)) => Spot::Agent(*id, cells(s)),
        _ => Spot::Nowhere,
    };
    let speaker = match spot {
        Spot::Agent(id, _) if sound::is_voice(play.phase) => Some(id),
        _ => None,
    };
    let heard_by_me = cue.hearer.is_none_or(|p| p.cast_unsigned() == local);
    // `GenUnit` and `PlaySound`, not the straight path.
    let speaks = match (via, &cue.from, cue.owned_by) {
        (Via::Unit(..), world::sound::From::Agent(id), None)
            if sound::speaks(play.distance, owner) =>
        {
            Some(*id)
        }
        _ => None,
    };

    heard_by_me.then_some(Pending {
        play,
        spot,
        distance,
        speaker,
        ambient: None,
        speaks,
    })
}

/// A new selection's sound: a site's, else its own.
#[allow(clippy::needless_pass_by_value, reason = "a Bevy system")]
fn select(
    selected: Res<Selected>,
    watch: Watch,
    corpus: Res<Corpus>,
    pieces: Pieces,
    mut mixer: ResMut<Mixer>,
) {
    let new = selected.0.filter(|id| mixer.selected != Some(*id));
    let shown: HashMap<u32, &Shown> = pieces.iter().map(|s| (s.agent, s)).collect();

    mixer.selected = selected.0;

    if let Some(id) = new
        && let Some((voice, phase)) = sound::selection(selection_of(id, &watch, &corpus, &shown))
    {
        let cue = Cue {
            at: watch.world().now(),
            from: world::sound::From::Agent(id),
            voice,
            phase,
            distance: 0.0,
            hearer: None,
            owned_by: None,
        };

        // Played as its owner's (`GE_SoundManager` passes 1).
        let via = Via::Selection(watch.local);

        if let Some(p) = resolve(&mixer.sounds, &corpus, &shown, &cue, via) {
            mixer.pending.push(p);
        }
    }
}

/// What a unit's class says of its selection sound
/// ([`Selection::of`]).
fn selection_of(
    id: u32,
    watch: &Watch,
    corpus: &Corpus,
    shown: &HashMap<u32, &Shown>,
) -> Selection {
    let attribute = |key| watch.world().word_of(id, key);
    let kind = shown
        .get(&id)
        .and_then(|s| Unit::find(&corpus.catalogue, s.code)?.ok())
        .and_then(|u| u.kind);

    Selection::of(kind, attribute)
}

/// Each piece's ambient loop, by its set: started when
/// it comes in range, stopped as its set or range changes, or as the
/// local player stops seeing it. A held piece is not drawn and loops
/// nothing (**Inferred**: unseen).
#[allow(clippy::needless_pass_by_value, reason = "a Bevy system")]
fn ambient(
    mut commands: Commands,
    watch: Watch,
    corpus: Res<Corpus>,
    pieces: Pieces,
    loops: Query<(Entity, &Voiced)>,
    mut mixer: ResMut<Mixer>,
) {
    let (listener, radius) = (mixer.listener, mixer.radius);
    let mut wanted: HashMap<(u32, FourCc), Pending> = HashMap::new();

    let watched = pieces.iter().filter(|s| {
        s.housed == Housed::Out && seen(watch.world(), s.agent, watch.local) == Seen::Yes
    });

    for s in watched {
        let Some(Look::Plays { set, .. } | Look::Frame { set, .. }) = s.look else {
            continue;
        };
        let key = (s.code, set);

        if !mixer.ambients.contains_key(&key) {
            let play = sound::ambient(set).and_then(|phase| {
                mixer.sounds.play(
                    &corpus.catalogue,
                    &Voice::Own,
                    &world::sound::Phase::Id(phase),
                    Some(s.code),
                    0.0,
                )
            });

            mixer.ambients.insert(key, play);
        }

        let owner = if s.owner == watch.local {
            Owner::Local
        } else {
            Owner::Other
        };
        let play = mixer.ambients.get(&key).cloned().flatten();
        let heard = play.and_then(|p| Some((sound::heard(p.distance, owner, Seen::Yes)?, p)));

        if let Some((distance, play)) = heard {
            let at = cells(s);

            if sound::mix(listener, at, radius, distance, FACING).volume > 0 {
                wanted.insert(
                    (s.agent, play.phase),
                    Pending {
                        play,
                        spot: Spot::Agent(s.agent, at),
                        distance,
                        speaker: None,
                        ambient: Some(s.agent),
                        speaks: None,
                    },
                );
            }
        }
    }

    // Stop what is no longer wanted; keep what plays.
    for (entity, v) in &loops {
        if let Some(key) = v.ambient
            && wanted.remove(&key).is_none()
        {
            commands.entity(entity).despawn();
        }
    }

    mixer.pending.extend(wanted.into_values());
}

/// Start what is pending, as its group allows ([`sound::Gate`]): a
/// held-back sound that queues waits, else it is dropped.
#[allow(clippy::needless_pass_by_value, reason = "a Bevy system")]
fn start(
    mut commands: Commands,
    corpus: Res<Corpus>,
    // A gust takes no game cue's voice.
    playing: Query<&Voiced, Without<Gusting>>,
    mut waves: ResMut<Assets<Wave>>,
    (mut mixer, mut speaker): (ResMut<Mixer>, ResMut<Speaker>),
    time: Res<Time<Real>>,
) {
    let now = Instant::now();
    let mut gate = Gate::default();

    for v in &playing {
        gate.playing(v.group, v.speaker, source(v.ambient));
    }

    let mut all = std::mem::take(&mut mixer.queued);

    all.append(&mut mixer.pending);

    for p in all {
        let last = p.play.group.and_then(|g| mixer.started.get(&g.id));
        let since = last.map(|t| u64::try_from((now - *t).as_millis()).unwrap_or(u64::MAX));

        match gate.admit(&p.play, since, p.speaker, source(p.ambient)) {
            Admit::Start => {
                if let Some(g) = p.play.group {
                    mixer.started.insert(g.id, now);
                }

                if p.speaks.is_some() {
                    speaker.0 = p.speaks;
                }

                let at = time.elapsed();

                spawn(&mut commands, &corpus, &mut waves, &mut mixer, &p, at);
            }
            Admit::Wait => mixer.queued.push(p),
            Admit::Drop => {}
        }
    }
}

/// A sound with an ambient loop's agent, or none, as the gate counts it.
const fn source<T: Copy>(ambient: Option<T>) -> Origin {
    match ambient {
        Some(_) => Origin::Ambient,
        None => Origin::Cue,
    }
}

/// One sound's entity, started at `now` on [`Time<Real>`]'s clock.
fn spawn(
    commands: &mut Commands,
    corpus: &Corpus,
    waves: &mut Assets<Wave>,
    mixer: &mut Mixer,
    p: &Pending,
    now: Duration,
) {
    let wave = p.play.wave;
    let pcm = mixer
        .waves
        .entry(wave)
        .or_insert_with(|| decode(corpus, wave))
        .clone();

    if let Some(pcm) = pcm {
        let mut draw = |n: u32| next(&mut mixer.rng) % n.max(1);
        // A spread no word holds plays unspread.
        let mut spread_of = |range| spread(range, &mut draw).unwrap_or(0);
        let rate = p.play.rate.unwrap_or(pcm.rate).cast_signed() + spread_of(p.play.rate_spread);
        let offset = shift(&p.play, p.spot, spread_of(p.play.volume_spread));
        let looping = if p.play.looped {
            Looping::Forever
        } else {
            Looping::Once
        };
        let rate = rate.max(1).cast_unsigned();
        let due = Due(now + pcm.length(rate));
        let gains = Arc::new(Gains::default());
        let handle = waves.add(Wave {
            pcm,
            rate,
            looping,
            gains: gains.clone(),
        });
        let settings = match looping {
            Looping::Once => PlaybackSettings::DESPAWN,
            Looping::Forever => PlaybackSettings::ONCE,
        };

        let mut sound = commands.spawn((
            AudioPlayer(handle),
            settings,
            Voiced {
                spot: p.spot,
                distance: p.distance,
                offset,
                pan: p.play.pan,
                group: p.play.group.map(|g| g.id),
                speaker: p.speaker,
                ambient: p.ambient.map(|a| (a, p.play.phase)),
                gains,
            },
        ));

        if looping == Looping::Once {
            sound.insert(due);
        }
    }
}

/// The volume shift `play` keeps at `spot`, `spread` its drawn volume
/// spread: a placed sound's mix overwrites both ([`Play::kept`]).
fn shift(play: &Play, spot: Spot, spread: i32) -> i32 {
    let placing = match spot {
        Spot::Agent(..) | Spot::Free(_) => Placing::Placed,
        Spot::Nowhere => Placing::Unplaced,
    };

    play.kept(placing, spread)
}

/// A `WAVE` record's samples.
fn decode(corpus: &Corpus, wave: FourCc) -> Option<Arc<Decoded>> {
    let at = corpus.catalogue.find(b"WAVE", wave)?;
    let record = corpus.catalogue.record(b"WAVE", at)?;
    let bytes = record.bytes().map_err(|e| warn!("WAVE {wave}: {e}")).ok()?;
    let pcm = Pcm::parse(&bytes).ok()?;

    Some(Arc::new(Decoded::of(&pcm)))
}

/// A xorshift step: the spreads' draws. The engine draws them on its
/// clock-seeded paint RNG, so no game state is touched either way.
const fn next(state: &mut u32) -> u32 {
    let mut x = *state;

    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *state = x;
    x
}

/// Each sound's gains from where its unit is now (**Inferred**: the engine
/// places a unit's sound as it starts; following the unit is the same at
/// the start) and where the view is (**Binary**: `CYSndManager`
/// remixes all as the view moves).
#[allow(clippy::needless_pass_by_value, reason = "a Bevy system")]
fn mix(
    settings: Res<Settings>,
    pieces: Pieces,
    mixer: Res<Mixer>,
    mut playing: Query<&mut Voiced>,
) {
    let places: HashMap<u32, [f32; 3]> = pieces.iter().map(|s| (s.agent, cells(s))).collect();
    let volume = settings.audio.effects_now();

    for mut v in &mut playing {
        if let Spot::Agent(id, at) = v.spot {
            v.spot = Spot::Agent(id, places.get(&id).copied().unwrap_or(at));
        }

        let level = match v.spot {
            Spot::Agent(_, at) | Spot::Free(at) => {
                sound::mix(mixer.listener, at, mixer.radius, v.distance, FACING)
            }
            Spot::Nowhere => Mix {
                pan: v.pan.unwrap_or(0),
                ..Mix::CENTRED
            },
        };
        let shifted = Mix {
            volume: (level.volume + v.offset).clamp(0, sound::FULL),
            ..level
        };
        let silent = level.volume <= 0;
        let (l, r) = shifted.gains();
        let gains = if silent {
            (0.0, 0.0)
        } else {
            (l * volume, r * volume)
        };

        v.gains.set(gains);
    }
}

// --------------------------------------------------------------------------
// Gusts
// --------------------------------------------------------------------------

/// `AM03 Whipping Wind`, a `WAVE` no `sounddesc` record of the base game
/// names: a gust's sound.
const WHIPPING_WIND: FourCc = FourCc(*b"AM03");

/// A gust's volume shift, hundredths of a dB: quiet under the game's
/// cues. **Inferred**: taste.
const GUST_QUIET: i32 = -1200;

/// The game ms gusts were last counted at; `None` with no wind.
#[derive(Resource, Default)]
struct Gusts(Option<u32>);

impl Gusts {
    /// Whether a gust peak reached world x `x` since the last call, game
    /// ms `ms`, `wave` λ. Both counts at `x`: time crosses a peak, a view
    /// moved does not.
    fn crossed(&mut self, ms: u32, x: f32, wave: f32) -> bool {
        let count = |ms| wind::gusts(ms, x, wave).floor();
        let passed = self
            .0
            .is_some_and(|at| ms > at && (count(ms) - count(at) - 1.0).abs() < 0.5);

        self.0 = Some(ms);
        passed
    }
}

/// A gust sound: the peak's world x at game ms `ms`, which it rides east.
#[derive(Component)]
struct Gusting {
    x: f32,
    ms: u32,
}

/// With `[enhancements] wind`, a gust's sound as each gust peak
/// (`regent_draw::wind::gusts`) passes the view, as the trees lean most:
/// started half its length before the peak reaches the view's centre,
/// placed on the peak and moved with it, so it pans west to east. Game
/// time: none while paused. No wind, none, and any playing stops.
#[allow(clippy::needless_pass_by_value, reason = "a Bevy system")]
fn gust(
    mut commands: Commands,
    (wind, trees, reel): (Res<Wind>, Res<Trees>, Option<Res<Reel>>),
    (view, maps, corpus): (Res<View>, Res<Maps>, Res<Corpus>),
    (mut mixer, mut gusts, mut waves): (ResMut<Mixer>, ResMut<Gusts>, ResMut<Assets<Wave>>),
    mut playing: Query<(Entity, &mut Voiced, &Gusting)>,
) {
    let ms = wind.held.or_else(|| reel.map(|r| r.ms)).unwrap_or(0);
    let speed = wind::gust_speed(trees.wave);
    let origin = maps.aloft_at(Vec2::ZERO, 0.0).x;
    let across = maps.aloft_at(Vec2::X, 0.0).x - origin;
    let listener = mixer.listener[1];
    let spot = |x: f32| Spot::Free([(x - origin) / across, listener, 0.0]);

    // Each one playing rides its peak; with no wind, stops.
    for (entity, mut v, g) in &mut playing {
        #[allow(clippy::cast_precision_loss, reason = "a sound's few seconds")]
        let x = g.x + speed * ms.wrapping_sub(g.ms) as f32 / 1000.0;

        if wind.reach > 0.0 && across != 0.0 {
            v.spot = spot(x);
        } else {
            commands.entity(entity).despawn();
        }
    }

    let pcm = mixer
        .waves
        .entry(WHIPPING_WIND)
        .or_insert_with(|| decode(&corpus, WHIPPING_WIND))
        .clone();
    let blowing = (wind.reach > 0.0 && across != 0.0).then_some(pcm).flatten();

    if let Some(pcm) = blowing {
        #[allow(clippy::cast_precision_loss, reason = "a sound's few seconds")]
        let length =
            pcm.samples.len() as f32 / f32::from(pcm.channels.max(1)) / pcm.rate.max(1) as f32;
        let x = view.pan.x - speed * length / 2.0;
        if gusts.crossed(ms, x, trees.wave) {
            let gains = Arc::new(Gains::default());
            let handle = waves.add(Wave {
                rate: pcm.rate,
                pcm,
                looping: Looping::Once,
                gains: gains.clone(),
            });

            commands.spawn((
                AudioPlayer(handle),
                PlaybackSettings::DESPAWN,
                Voiced {
                    spot: spot(x),
                    distance: 0.0,
                    offset: GUST_QUIET,
                    pan: None,
                    group: None,
                    speaker: None,
                    ambient: None,
                    gains,
                },
                Gusting { x, ms },
            ));
        }
    } else {
        gusts.0 = None;
    }
}

// --------------------------------------------------------------------------
// Music
// --------------------------------------------------------------------------

/// Every sound and music track playing.
type Playing<'w, 's> = Query<'w, 's, Entity, Or<(With<Voiced>, With<Track>)>>;

/// Whether the end's sounds are done.
#[derive(Resource, Default)]
struct Finale(bool);

/// The game's end, once (state 8's first step, game end step): the
/// music stopped, every sound,
/// then the advisor, unplaced, the local
/// player's. Nothing starts after.
#[allow(clippy::needless_pass_by_value, reason = "a Bevy system")]
fn finale(
    mut commands: Commands,
    watch: Watch,
    corpus: Res<Corpus>,
    playing: Playing,
    mut finale: ResMut<Finale>,
    mut mixer: ResMut<Mixer>,
) {
    let (world, local) = (watch.world(), watch.local);

    // The game going on again (CONTINUE PLAYING), a later end is heard;
    // a spectator's, once the game is over.
    if watch.end.is_none() {
        finale.0 = false;
    }

    if let Some(how) = watch.end.filter(|_| !finale.0) {
        finale.0 = true;

        for entity in &playing {
            commands.entity(entity).despawn();
        }

        mixer.pending.clear();
        mixer.queued.clear();

        let cue = Cue {
            at: world.now(),
            from: world::sound::From::Nowhere,
            voice: Voice::Sound(how.advisor()),
            phase: world::sound::Phase::Id(world::sound::ADVISOR),
            distance: 0.0,
            hearer: None,
            owned_by: None,
        };
        let advisor = resolve(
            &mixer.sounds,
            &corpus,
            &HashMap::new(),
            &cue,
            Via::Selection(local),
        );

        mixer.pending.extend(advisor);
    }
}

/// Whether the end panel's voice has been asked.
#[derive(Resource, Default)]
struct Farewell(bool);

/// Whether the end's advisor is done (sound playing false): the
/// end panel waits for it (game end step step 1). A sound past its
/// [`Due`] by [`LATE`] counts as ended: Bevy ends it only once the device
/// has played it, never with the device dead or muted.
#[derive(Resource, Default)]
pub struct Spoken(pub bool);

/// A won quest's end voice ([`menu::end_voice`]), once its panel is open
/// ([`end::EndScreen::opened`]): the original's `AP60` opens after the
/// advisor and the fade (game end step step 1) and says it
/// unplaced, in its first phase, kept until a press on the panel stops
/// it. It plays as the menus' voice,
/// hushed as the game is left ([`farewell_hush`]). The original's pause
/// as the panel opens (state 7) is of what plays then, so it is heard.
/// The game going on again, all of it may come again.
#[allow(clippy::needless_pass_by_value, reason = "a Bevy system")]
#[allow(
    clippy::too_many_arguments,
    reason = "a system's arguments are its dependencies"
)]
fn farewell(
    (watch, session): (Watch, Res<Session>),
    corpus: Res<Corpus>,
    mixer: Res<Mixer>,
    finale: Res<Finale>,
    playing: Query<Option<&Due>, With<Voiced>>,
    (shown, mut spoken): (Res<end::EndScreen>, ResMut<Spoken>),
    mut farewell: ResMut<Farewell>,
    mut narration: ResMut<Narration>,
    time: Res<Time<Real>>,
) {
    let now = time.elapsed();
    let ended = |due: Option<&Due>| due.is_some_and(|d| now >= d.0 + LATE);
    let done = finale.0 && mixer.pending.is_empty() && playing.iter().all(ended);

    if !finale.0 {
        farewell.0 = false;
    }

    spoken.0 = finale.0 && (spoken.0 || done);

    if shown.opened() && !farewell.0 {
        farewell.0 = true;

        let end = watch.end;
        let voice = session
            .quest_key()
            .zip(end)
            .and_then(|(code, end)| menu::end_voice(code, end));
        let catalogue = &corpus.catalogue;
        let said = voice.and_then(|sound| {
            let play = mixer.sounds.play(
                catalogue,
                &Voice::Sound(sound),
                &world::sound::Phase::First,
                None,
                0.0,
            )?;
            let at = catalogue.find(b"WAVE", play.wave)?;
            let wave = *catalogue.record(b"WAVE", at)?;

            Some(Said { play, wave })
        });

        if said.is_some() {
            narration.say(said);
        }
    }
}

/// The music player, the menus' and the game's alike:
/// HD's track list, and what plays.
#[derive(Resource, Default)]
struct Music {
    /// `MusicTracks.txt`'s lines; `PlayMusic(n)` takes line `n`, from 1.
    tracks: Vec<String>,
    dir: PathBuf,
    playing: Option<i32>,
    entity: Option<Entity>,
    read: bool,
    /// The next look for silence ([`front_end::POLL_MS`]).
    poll: Option<Instant>,
}

/// A music track's entity.
#[derive(Component)]
struct Track;

/// The music player's type, for a test: it outlasts a game.
#[cfg(test)]
pub const fn music_type() -> std::any::TypeId {
    std::any::TypeId::of::<Music>()
}

/// The track the music player last asked, for a test.
#[cfg(test)]
pub fn track(world: &bevy::prelude::World) -> Option<i32> {
    world.get_resource::<Music>().and_then(|m| m.playing)
}

/// The `WAVE` the menus' voice plays, for a test.
#[cfg(test)]
pub fn narrated(world: &bevy::prelude::World) -> Option<FourCc> {
    let narration = world.get_resource::<Narration>()?;

    narration.playing.as_ref().map(|n| n.play.wave)
}

/// Whether the menus are up: a run condition.
#[allow(clippy::needless_pass_by_value, reason = "a run condition")]
fn in_menus(screen: Option<Res<State<Screen>>>) -> bool {
    screen.is_some_and(|s| *s.get() == Screen::Menu)
}

impl Music {
    /// The track list of the install at `hd`, read once.
    fn read(&mut self, hd: &Path) {
        if !self.read {
            let text = std::fs::read_to_string(hd.join(TRACKS)).unwrap_or_default();

            self.read = true;
            self.tracks = text.lines().map(|l| l.trim().to_owned()).collect();
            self.dir = hd.join(MUSIC);
        }
    }

    /// Whether the track playing has ended since last asked: despawned at
    /// its end, or with the game it was made in. The next poll is a whole
    /// period on, so a game's callback, run between frames, picks first
    /// (the engine runs it in the same update; its gap is 0 to a poll).
    fn ended(&mut self, tracks: &Query<(), With<Track>>) -> bool {
        let ended = self.entity.is_some_and(|e| tracks.get(e).is_err());

        if ended {
            self.entity = None;
            self.poll = Some(Instant::now() + Duration::from_millis(front_end::POLL_MS));
        }

        ended
    }

    /// Whether the poll is due, every [`front_end::POLL_MS`] real ms.
    fn polled(&mut self) -> bool {
        let now = Instant::now();
        let due = self.poll.is_none_or(|at| now >= at);

        if due {
            self.poll = Some(now + Duration::from_millis(front_end::POLL_MS));
        }

        due
    }

    /// Track `want` played: line
    /// `n` of the list, a file under `Music`, once; a line opening with a
    /// digit is a CD track, not played here. The track playing plays on;
    /// another is stopped first, no fade. `again` starts a silent one
    /// over.
    fn ask(&mut self, want: Option<i32>, again: Again, (commands, sources, loud): Out<'_, '_, '_>) {
        let silent = self.entity.is_none() && again == Again::Yes;

        if want != self.playing || silent {
            if let Some(e) = self.entity.take() {
                commands.entity(e).despawn();
            }

            self.playing = want;

            let file = want
                .and_then(|n| self.tracks.get(usize::try_from(n - 1).ok()?))
                .filter(|name| !name.starts_with(|c: char| c.is_ascii_digit()));
            let bytes = file.and_then(|name| std::fs::read(self.dir.join(name)).ok());

            if let Some(bytes) = bytes {
                let handle = sources.add(AudioSource {
                    bytes: bytes.into(),
                });
                let volume = bevy::audio::Volume::Linear(loud);

                self.entity = Some(
                    commands
                        .spawn((
                            AudioPlayer(handle),
                            PlaybackSettings::DESPAWN.with_volume(volume),
                            Track,
                        ))
                        .id(),
                );
            }
        }
    }
}

/// Whether a silent track starts over.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Again {
    Yes,
    No,
}

/// What starting a track needs: commands, the sources, its loudness.
type Out<'a, 'w, 's> = (&'a mut Commands<'w, 's>, &'a mut Assets<AudioSource>, f32);

/// Plays the track the scripts ask; a game that has asked none yet keeps
/// what plays, the menus' track. As a track ends, the scripts' callback
/// picks the next; one it leaves silent starts again
/// at the next poll.
#[allow(clippy::needless_pass_by_value, reason = "a Bevy system")]
fn music(
    mut commands: Commands,
    (watch, mut outbox): (Watch, ResMut<Outbox>),
    (settings, install): (Res<Settings>, Res<Install>),
    mut music: ResMut<Music>,
    mut sources: ResMut<Assets<AudioSource>>,
    tracks: Query<(), With<Track>>,
) {
    music.read(&install.hd);

    if music.ended(&tracks) {
        outbox.push(Ask::MusicStopped);
    }

    let want = watch.music.or(music.playing);
    let again = if music.polled() {
        Again::Yes
    } else {
        Again::No
    };

    music.ask(
        want,
        again,
        (&mut commands, &mut sources, settings.audio.music_now()),
    );
}

/// The menus' music: each page's screen's track ([`front_end::track`]), on
/// a page that asks none what plays; a track that ends starts again at
/// the next poll. No callback runs here.
#[allow(clippy::needless_pass_by_value, reason = "a Bevy system")]
fn front(
    mut commands: Commands,
    menu: Option<Res<crate::menu::Menu>>,
    (settings, install): (Res<Settings>, Res<Install>),
    mut music: ResMut<Music>,
    mut sources: ResMut<Assets<AudioSource>>,
    tracks: Query<(), With<Track>>,
) {
    music.read(&install.hd);
    music.ended(&tracks);

    // A page that stops the music (a video's) asks none, and none
    // starts again over it.
    let hushed = menu.as_ref().is_some_and(|m| m.hushes_music());
    let want = if hushed {
        None
    } else {
        menu.and_then(|m| m.music()).or(music.playing)
    };
    let again = if music.polled() && !hushed {
        Again::Yes
    } else {
        Again::No
    };

    music.ask(
        want,
        again,
        (&mut commands, &mut sources, settings.audio.music_now()),
    );
}

/// The end voice stopped as the game is left, the panel's Main Menu
/// (`EndGameController` stops it at any press); a quit ends all.
fn farewell_hush(mut narration: ResMut<Narration>) {
    narration.hush();
}

/// The music after the game's end: the stopped track starts again at the
/// next poll. **Binary**: `GE_MusicManager` leaves the track marked and
/// polls in state 8 and each dialog pass, the end
/// panel's too; the pause of states 7 and 8 spares category 10, the
/// music's. **Inferred**: the
/// stop runs no end callback (`CYDSSndBuffer` calls none), so the
/// same track, not the scripts' next.
#[allow(clippy::needless_pass_by_value, reason = "a Bevy system")]
fn encore(
    mut commands: Commands,
    settings: Res<Settings>,
    mut music: ResMut<Music>,
    mut sources: ResMut<Assets<AudioSource>>,
    tracks: Query<(), With<Track>>,
    shown: Res<end::EndScreen>,
) {
    music.ended(&tracks);

    let want = music.playing;
    let again = if music.polled() {
        Again::Yes
    } else {
        Again::No
    };

    // A film hushes the track; it starts again at the next poll after.
    if shown.filming() {
        if let Some(e) = music.entity.take() {
            commands.entity(e).despawn();
        }
    } else {
        music.ask(
            want,
            again,
            (&mut commands, &mut sources, settings.audio.music_now()),
        );
    }
}

/// The playing track follows the music setting as it changes: a mute.
#[allow(clippy::needless_pass_by_value, reason = "a Bevy system")]
fn loudness(settings: Res<Settings>, mut sinks: Query<&mut AudioSink, With<Track>>) {
    if settings.is_changed() {
        let volume = bevy::audio::Volume::Linear(settings.audio.music_now());

        for mut sink in &mut sinks {
            sink.set_volume(volume);
        }
    }
}

// --------------------------------------------------------------------------
// The menus' voice
// --------------------------------------------------------------------------

/// A voice the menus play: its phase, and its `WAVE` record, read as it
/// plays.
#[derive(Clone)]
pub struct Said {
    pub play: Play,
    pub wave: cam::Record<'static>,
}

/// The menus' one voice, unplaced, kept to be stopped
/// (`GE_SoundManager` with keep 1): the map's advisor, a briefing's.
#[derive(Resource, Default)]
pub struct Narration {
    asked: Option<Asked>,
    playing: Option<Narrated>,
}

/// What the menus last asked of their voice.
enum Asked {
    Say(Said),
    Hush,
}

impl Narration {
    /// Stop what plays, then play `said`.
    pub fn say(&mut self, said: Option<Said>) {
        self.asked = Some(said.map_or(Asked::Hush, Asked::Say));
    }

    /// Stop what plays.
    pub const fn hush(&mut self) {
        self.asked = Some(Asked::Hush);
    }
}

/// The voice's entity, its gains and its phase.
#[derive(Clone)]
struct Narrated {
    entity: Entity,
    gains: Arc<Gains>,
    play: Play,
}

/// The voice asked starts, the last stopped; it follows the effects'
/// loudness as it changes, a mute too.
#[allow(clippy::needless_pass_by_value, reason = "a Bevy system")]
fn narrate(
    mut commands: Commands,
    settings: Res<Settings>,
    mut narration: ResMut<Narration>,
    mut waves: ResMut<Assets<Wave>>,
    alive: Query<(), With<AudioPlayer<Wave>>>,
) {
    if let Some(asked) = narration.asked.take() {
        let asked = match asked {
            Asked::Say(said) => Some(said),
            Asked::Hush => None,
        };

        if let Some(old) = narration.playing.take()
            && alive.get(old.entity).is_ok()
        {
            commands.entity(old.entity).despawn();
        }

        let pcm = asked.as_ref().and_then(|said| {
            let bytes = said.wave.bytes().map_err(|e| warn!("voice: {e}")).ok()?;

            Pcm::parse(&bytes).ok().map(|pcm| Decoded::of(&pcm))
        });

        if let (Some(said), Some(pcm)) = (asked, pcm) {
            let gains = Arc::new(Gains::default());
            let handle = waves.add(Wave {
                rate: said.play.rate.unwrap_or(pcm.rate),
                pcm: Arc::new(pcm),
                looping: Looping::Once,
                gains: gains.clone(),
            });
            let entity = commands
                .spawn((AudioPlayer(handle), PlaybackSettings::DESPAWN))
                .id();

            narration.playing = Some(Narrated {
                entity,
                gains,
                play: said.play,
            });
        }
    }

    if let Some(n) = &narration.playing {
        let level = Mix {
            volume: (sound::FULL + n.play.kept(Placing::Unplaced, 0)).clamp(0, sound::FULL),
            pan: n.play.pan.unwrap_or(0),
        };
        let (l, r) = level.gains();
        let volume = settings.audio.effects_now();

        n.gains.set((l * volume, r * volume));
    }
}

// --------------------------------------------------------------------------
// The menus' clicks
// --------------------------------------------------------------------------

/// Clicks to play: each a `WAVE` record's bytes ([`front_end::click`]).
#[derive(Resource, Default)]
pub struct ClickSounds(pub Vec<&'static [u8]>);

/// Each click asked starts, straight and whole, at the effects' loudness
/// as it starts (`CYSndManager`, category 1).
#[allow(clippy::needless_pass_by_value, reason = "a Bevy system")]
fn click(
    mut commands: Commands,
    settings: Res<Settings>,
    mut clicks: ResMut<ClickSounds>,
    mut waves: ResMut<Assets<Wave>>,
) {
    let volume = settings.audio.effects_now();

    for bytes in clicks.0.drain(..) {
        if let Ok(pcm) = Pcm::parse(bytes) {
            let gains = Arc::new(Gains::default());

            gains.set((volume, volume));

            let handle = waves.add(Wave {
                rate: pcm.rate,
                pcm: Arc::new(Decoded::of(&pcm)),
                looping: Looping::Once,
                gains,
            });

            commands.spawn((AudioPlayer(handle), PlaybackSettings::DESPAWN));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wave(samples: Vec<i16>, channels: u16, looping: Looping) -> Wave {
        let gains = Arc::new(Gains::default());

        gains.set((1.0, 0.5));
        Wave {
            pcm: Arc::new(Decoded {
                samples,
                channels,
                rate: 22_050,
            }),
            rate: 22_050,
            looping,
            gains,
        }
    }

    /// `Place_Building`'s `Begin`: set volume 5,000, spread up 5,000.
    fn placing() -> Play {
        Play {
            sound: FourCc(*b"PB01"),
            phase: world::sound::BEGIN,
            wave: FourCc(*b"PB01"),
            looped: false,
            queues: false,
            distance: sound::OWNER_ONLY,
            group: None,
            volume_spread: (5000, 0),
            rate_spread: (1000, 2000),
            volume: Some(5000),
            rate: None,
            pan: None,
        }
    }

    /// A wave's length is its frames over the rate it plays at: two
    /// channels' 44 100 samples at 22 050 Hz is one second, two at half.
    #[test]
    fn a_waves_length_is_its_frames_over_its_rate() {
        let pcm = wave(vec![0; 44_100], 2, Looping::Once).pcm;

        assert_eq!(pcm.length(22_050), Duration::from_secs(1));
        assert_eq!(pcm.length(11_025), Duration::from_secs(2));
    }

    /// A view jump west, game time still, starts no gust; time
    /// reaching the next peak does.
    #[test]
    fn a_gust_sounds_on_time_not_on_a_view_jump() {
        let wave = 64.0;
        let mut gusts = Gusts::default();
        let ms = wind::GUST_MS * 2;

        // Peaks lie a width apart; a jump of 0.9 widths west passes one.
        let width = wind::gust_speed(wave) * wind::GUST;

        assert!(!gusts.crossed(ms, 0.0, wave));
        assert!(!gusts.crossed(ms + 16, -width * 0.9, wave));
        assert!(!gusts.crossed(ms + 32, 0.0, wave));
        assert!(gusts.crossed(ms + wind::GUST_MS, 0.0, wave));
    }

    #[test]
    fn a_placed_sound_keeps_neither_set_volume_nor_spread() {
        let at = Spot::Agent(1, [0.0; 3]);

        assert_eq!(shift(&placing(), at, 2500), 0);
        assert_eq!(shift(&placing(), Spot::Nowhere, 2500), -2500);
    }

    #[test]
    fn a_mono_wave_plays_on_both_sides_at_their_gains() {
        let out: Vec<f32> = wave(vec![i16::MAX, -i16::MAX], 1, Looping::Once)
            .decoder()
            .collect();

        assert_eq!(out, [1.0, 0.5, -1.0, -0.5]);
    }

    #[test]
    fn a_looped_wave_starts_over() {
        let out: Vec<f32> = wave(vec![i16::MAX, -i16::MAX], 2, Looping::Forever)
            .decoder()
            .take(6)
            .collect();

        assert_eq!(out, [1.0, -0.5, 1.0, -0.5, 1.0, -0.5]);
    }

    /// The expansion's corpus and its `WAVE` ids, each once.
    fn expansion_waves() -> (Corpus, Vec<FourCc>) {
        let hd = crate::files::env::path(crate::files::env::HD).expect("MAJESTY_HD");
        let corpus =
            Corpus::open(&crate::game::load::archives(&hd).expect("the dataset")).expect("corpus");
        let mut ids: Vec<FourCc> = corpus
            .catalogue
            .tagged(b"WAVE")
            .map(|(_, r)| r.id)
            .collect();

        ids.sort_unstable_by_key(|id| id.0);
        ids.dedup();
        (corpus, ids)
    }

    /// What the mixer played when it cached `f32`: each sample scaled to
    /// -1 to 1 once, then times its side's gain.
    fn f32_path(pcm: &Pcm, (left, right): (f32, f32)) -> Vec<f32> {
        let scale = f32::from(i16::MAX);
        let samples: Vec<f32> = pcm.samples().map(|s| f32::from(s) / scale).collect();
        let channels = usize::from(pcm.channels.max(1));
        let second = usize::from(channels > 1);

        samples
            .chunks_exact(channels)
            .flat_map(|frame| [frame[0] * left, frame[second] * right])
            .collect()
    }

    /// Every expansion `WAVE` plays the same bits from the 16-bit cache
    /// as from the old `f32` one: the same two roundings, in one order.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_16_bit_wave_plays_the_bits_the_f32_cache_did() {
        let (corpus, ids) = expansion_waves();
        let gains = (0.812_3, 0.377);

        for id in &ids {
            let at = corpus.catalogue.find(b"WAVE", *id).expect("listed");
            let record = corpus.catalogue.record(b"WAVE", at).expect("found");
            let bytes = record.bytes().expect("read");
            let pcm = Pcm::parse(&bytes).expect("16-bit PCM");
            let mut played = wave(Vec::new(), pcm.channels, Looping::Once);

            played.pcm = Arc::new(Decoded::of(&pcm));
            played.gains.set(gains);

            let new: Vec<u32> = played.decoder().map(f32::to_bits).collect();
            let old: Vec<u32> = f32_path(&pcm, gains)
                .into_iter()
                .map(f32::to_bits)
                .collect();

            assert_eq!(new, old, "{id:?}");
        }
    }

    /// Every expansion `WAVE` cached, as the mixer fills it: the cache is
    /// the archives' PCM bytes, half the `f32` form's.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn every_expansion_wave_caches_its_pcm_bytes() {
        let (corpus, ids) = expansion_waves();
        let cache: HashMap<FourCc, Option<Arc<Decoded>>> =
            ids.iter().map(|id| (*id, decode(&corpus, *id))).collect();
        let samples: usize = cache.values().flatten().map(|d| d.samples.len()).sum();
        let pcm_bytes: usize = ids
            .iter()
            .filter_map(|id| corpus.catalogue.find(b"WAVE", *id))
            .filter_map(|at| corpus.catalogue.record(b"WAVE", at))
            .filter_map(|r| r.bytes().ok())
            .filter_map(|bytes| Pcm::parse(&bytes).ok().map(|pcm| pcm.data.len()))
            .sum();
        let cached = samples * size_of::<i16>();

        println!(
            "{} waves, {samples} samples: {cached} bytes cached, {} as f32",
            cache.len(),
            samples * size_of::<f32>(),
        );
        assert_eq!(cached, pcm_bytes);
    }

    /// The menus' music changed the frame after it started: its first
    /// track goes before Bevy's audio gives it a sink, and Bevy's insert
    /// on a despawned entity panics (a crash seen once in
    /// `a_command_line_quest_loads_a_save_records_its_win_and_goes_to_the_menus`).
    /// One thread runs the systems in their sorted order, so the panic,
    /// where the order allows it, comes each run. Needs an audio device:
    /// with none, Bevy plays nothing.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_track_stopped_before_its_sink_is_in_does_not_panic() {
        let mut app = App::new();

        app.add_plugins((
            MinimalPlugins,
            bevy::state::app::StatesPlugin,
            bevy::asset::AssetPlugin::default(),
            bevy::audio::AudioPlugin {
                global_volume: bevy::audio::GlobalVolume::new(bevy::audio::Volume::SILENT),
                ..default()
            },
            Plugin,
        ))
        .insert_state(Screen::Menu)
        .insert_resource(Settings::default())
        .insert_resource(Install::from_env().expect("MAJESTY_HD"))
        .init_resource::<end::PlayState>()
        .edit_schedule(PostUpdate, |s| {
            s.set_executor(bevy::ecs::schedule::SingleThreadedExecutor::new());
        });

        // The main menu's track, then the summary's.
        crate::menu::Opened::new(Vec::new(), crate::menu::Art::default()).insert(app.world_mut());
        app.update();
        crate::menu::Opened::starting(Vec::new(), 0).insert(app.world_mut());
        app.update();

        app.update();

        let sinks = app
            .world_mut()
            .query::<&bevy::audio::AudioSink>()
            .iter(app.world())
            .count();

        if sinks == 0 {
            warn!("no audio device: nothing played, nothing shown");
        }
    }
}
