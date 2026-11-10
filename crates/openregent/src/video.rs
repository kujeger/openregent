//! A `PICT` Bink video played: decoded on a worker thread, shown as an
//! egui texture at the video's own rate, its sound through Bevy's audio.
//!
//! ```text
//!  worker: the archive read (a .dat) ──> bink::Video ──decode──> RGBA ColorImage ──channel (AHEAD)──┐
//!  frame:  Clock ticked by the frame's time ──due index──> take frames ─────────────────────────────┘──> texture
//!  the first frame shown ──> Sound: bink::Audio, a packet at a time on the audio thread
//! ```
//!
//! The frame never waits on the worker: a frame not yet decoded leaves
//! the last one shown, and the clock starts at the first frame shown.
//!
//! With `[enhancements] video_upscale = "fsr"` each frame shown is also
//! upscaled on the GPU to the pixels it shows at (`regent_draw::art::fsr`):
//!
//! ```text
//!  draw site (egui pass)  Frame::at: the pixels noted; the upscaled texture
//!    │                    if made at them, else the plain one
//!  upscale (PostUpdate)   a target at the noted pixels; a new frame or
//!                         target asked into it, drawn before egui's pass
//! ```

use std::borrow::Cow;
use std::num::NonZero;
use std::ops::Range;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use bevy::audio::{AudioPlayer, Decodable, PlaybackSettings};
use bevy::log::warn;
use bevy::prelude::*;
use bevy_egui::{EguiPostUpdateSet, EguiUserTextures, egui};
use regent_core::data::fourcc::FourCc;
use regent_core::data::{cam, payload};
use regent_draw::art::fsr::{self, VideoUpscale};

use crate::audio::AddSound;

/// Frames the worker decodes ahead of the one shown.
const AHEAD: usize = 2;

/// How a video is paced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pace {
    /// At its frame rate.
    Run,
    /// Held on its first frame shown: a screenshot's still.
    Held,
}

/// Which frame is due: the video's clock, in its own frames. Pure, so
/// the timing is tested without a thread or a window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Clock {
    /// Frames a second, as a fraction.
    rate: (u32, u32),
    /// The frames shown: all but the last (`menu::shown_frames`).
    frames: usize,
    /// Where the video starts, a frame index.
    start: usize,
    /// Time since the start's frame was first shown; `None` before.
    elapsed: Option<Duration>,
    pace: Pace,
}

impl Clock {
    /// A clock for `header`'s video, started at frame `start`.
    pub fn new(header: &bink::Header, start: usize, pace: Pace) -> Clock {
        let frames = regent_core::view::menu::shown_frames(header.frames);

        Clock {
            rate: (header.rate_num, header.rate_den),
            frames,
            start: start.min(frames.saturating_sub(1)),
            elapsed: None,
            pace,
        }
    }

    /// The first frame is up: the clock runs from now.
    pub const fn started(&mut self) {
        if self.elapsed.is_none() {
            self.elapsed = Some(Duration::ZERO);
        }
    }

    /// `dt` passed; a held clock or one not started stays.
    pub fn tick(&mut self, dt: Duration) {
        if let (Some(elapsed), Pace::Run) = (self.elapsed.as_mut(), self.pace) {
            *elapsed += dt;
        }
    }

    /// The frame due now, from the start: frame `n` shows from
    /// `n / rate` seconds on. Past the last shown, the count shown.
    pub fn due(&self) -> usize {
        let (num, den) = self.rate;
        let shown = self.elapsed.map_or(0, |e| {
            let frames = e.as_micros() * u128::from(num) / (u128::from(den) * 1_000_000);

            usize::try_from(frames).unwrap_or(usize::MAX)
        });

        self.start.saturating_add(shown).min(self.frames)
    }

    /// Whether the last frame shown has had its time: the video is over.
    pub fn ended(&self) -> bool {
        self.due() >= self.frames
    }

    pub const fn start(&self) -> usize {
        self.start
    }

    /// How many frames it shows.
    pub const fn frames(&self) -> usize {
        self.frames
    }
}

/// Where a video's Bink file is.
#[derive(Clone, Debug)]
pub enum VideoSource {
    /// A record of the datasets' `.cam` archives, its bytes read on the
    /// worker ([`cam::Record::bytes`]).
    Record(cam::Record<'static>),
    /// A record of an archive on disk, read on the worker: the
    /// cinematics' `.dat`s (`menu::CINEMATICS`), opened as needed as the
    /// original opens them.
    Archive(PathBuf),
}

/// A Bink file's bytes: held, or read from an archive and shared with
/// the sound.
#[derive(Clone, Debug)]
pub enum Bytes {
    Held(&'static [u8]),
    Read(Arc<[u8]>, Range<usize>),
}

impl AsRef<[u8]> for Bytes {
    fn as_ref(&self) -> &[u8] {
        match self {
            Bytes::Held(bytes) => bytes,
            Bytes::Read(all, at) => &all[at.clone()],
        }
    }
}

/// `id`'s Bink file from `source`.
fn open(id: FourCc, source: VideoSource) -> Result<Bytes, String> {
    match source {
        VideoSource::Record(record) => bink(id, &record),
        VideoSource::Archive(path) => {
            // The index alone; the clip reads its own range.
            let image = cam::Image::read(&path).map_err(|e| e.to_string())?;
            let archive = image
                .archive()
                .map_err(|e| format!("{}: {e}", path.display()))?;
            let record = archive
                .chunk_by_tag(b"PICT")
                .and_then(|c| c.records.iter().find(|r| r.id == id))
                .ok_or_else(|| format!("{}: no PICT {id}", path.display()))?;
            let read = record.bytes().map_err(|e| e.to_string())?.into_owned();

            within(id, read.into())
        }
    }
}

/// `record`'s Bink file: held, or read now.
fn bink(id: FourCc, record: &cam::Record<'static>) -> Result<Bytes, String> {
    match record.bytes().map_err(|e| e.to_string())? {
        Cow::Borrowed(held) => match payload::Pict::parse(held) {
            Ok(payload::Pict::Video { file, .. }) => Ok(Bytes::Held(file)),
            _ => Err(format!("PICT {id} is no video")),
        },
        Cow::Owned(read) => within(id, read.into()),
    }
}

/// The Bink file inside `all`, a `PICT` payload.
fn within(id: FourCc, all: Arc<[u8]>) -> Result<Bytes, String> {
    let at = match payload::Pict::parse(&all) {
        Ok(payload::Pict::Video { file, .. }) => {
            let from = file.as_ptr() as usize - all.as_ptr() as usize;

            Ok(from..from + file.len())
        }
        _ => Err(format!("PICT {id} is no video")),
    };

    at.map(|at| Bytes::Read(all, at))
}

/// What the worker says.
enum Said {
    /// The file opened: its header, its sound if any, the frames shown.
    Opened(bink::Header, Option<Sound>),
    Frame(Decoded),
    /// It would not open or decode: the video is over.
    Failed,
}

/// One frame from the worker.
struct Decoded {
    index: usize,
    image: egui::ColorImage,
}

/// Each player's number, so its sound is told from the next's.
static SERIAL: AtomicU64 = AtomicU64::new(0);

/// A video playing.
pub struct Playing {
    /// The `PICT` id, for the texture's name and the log.
    id: FourCc,
    serial: u64,
    start: usize,
    pace: Pace,
    /// From the worker's `Opened` on.
    clock: Option<Clock>,
    /// The worker said the video will not play.
    failed: bool,
    /// In a mutex only so the menus' resource is `Sync`.
    said: Mutex<Receiver<Said>>,
    worker: Option<JoinHandle<()>>,
    /// A frame taken from the worker, not yet due.
    next: Option<Decoded>,
    texture: Option<egui::TextureHandle>,
    /// Its sound, until the first frame shows and it is taken to play.
    sound: Option<Sound>,
    /// Whether a frame has shown.
    shown: bool,
    /// Held while no frame is shown yet, or, upscaled, its first
    /// upscale: a screenshot waits for it.
    pending: Option<regent_draw::art::Count>,
    /// The frame shown, kept to upscale, and whether it is new since.
    picture: Option<Arc<egui::ColorImage>>,
    fresh: bool,
    upscaled: Option<Upscaled>,
    /// The pixels a draw site last showed it at ([`Frame::at`]).
    wanted: Arc<AtomicU64>,
}

/// A frame upscaled on the GPU: its target, lent to egui.
struct Upscaled {
    target: fsr::Target,
    /// Whether a frame was asked into it: shown from the next pass.
    asked: bool,
}

/// The frame shown, as a draw site takes it.
#[derive(Clone)]
pub struct Frame {
    plain: egui::TextureId,
    upscaled: Option<(egui::TextureId, UVec2)>,
    wanted: Arc<AtomicU64>,
}

impl Frame {
    /// The texture to draw at `rect`, `scale` pixels a point, and where:
    /// the upscaled one, on whole pixels, where made at `rect`'s pixels;
    /// else the plain one at `rect`. The pixels noted for the next
    /// upscale.
    pub fn at(&self, scale: f32, rect: egui::Rect) -> (egui::TextureId, egui::Rect) {
        let pixels = (rect.size() * scale).round();
        let size = pixel_size(pixels);

        self.wanted.store(packed(size), Ordering::Relaxed);

        match self.upscaled {
            Some((id, made)) if made == size => {
                let corner = (rect.min.to_vec2() * scale).round() / scale;

                (
                    id,
                    egui::Rect::from_min_size(corner.to_pos2(), pixels / scale),
                )
            }
            _ => (self.plain, rect),
        }
    }
}

/// `pixels`, whole and not negative, as a size.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a rect's pixels, rounded"
)]
const fn pixel_size(pixels: egui::Vec2) -> UVec2 {
    UVec2::new(pixels.x.max(0.0) as u32, pixels.y.max(0.0) as u32)
}

/// A size in one word, for [`Playing::wanted`]; 0 is none.
fn packed(size: UVec2) -> u64 {
    (u64::from(size.x) << 32) | u64::from(size.y)
}

/// [`packed`]'s size; none for 0.
#[allow(clippy::cast_possible_truncation, reason = "the halves of a word")]
fn unpacked(word: u64) -> Option<UVec2> {
    (word != 0).then(|| UVec2::new((word >> 32) as u32, word as u32))
}

impl Playing {
    /// `id` from `source`, decoded from frame `start` on.
    ///
    /// # Errors
    ///
    /// No thread to decode on.
    pub fn new(
        id: FourCc,
        source: VideoSource,
        start: usize,
        pace: Pace,
    ) -> Result<Playing, String> {
        let (send, said) = sync_channel(AHEAD);
        let worker = std::thread::Builder::new()
            .name(format!("video {id}"))
            .spawn(move || work(id, source, start, &send))
            .map_err(|e| e.to_string())?;

        Ok(Playing {
            id,
            serial: SERIAL.fetch_add(1, Ordering::Relaxed),
            start,
            pace,
            clock: None,
            failed: false,
            said: Mutex::new(said),
            worker: Some(worker),
            next: None,
            texture: None,
            sound: None,
            shown: false,
            pending: Some(regent_draw::art::Count::new()),
            picture: None,
            fresh: false,
            upscaled: None,
            wanted: Arc::new(AtomicU64::new(0)),
        })
    }

    /// `dt` passed: the due frame into the texture, if decoded; kept to
    /// upscale as `mode` says.
    pub fn tick(&mut self, ctx: &egui::Context, dt: Duration, mode: VideoUpscale) {
        if let Some(clock) = &mut self.clock {
            clock.tick(dt);
        }

        let mut latest = None;
        let said = self
            .said
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        // Every frame up to the due one taken; the last kept.
        loop {
            let next = self
                .next
                .take()
                .map(Said::Frame)
                .or_else(|| said.try_recv().ok());
            let due = self.clock.map_or(0, |c| c.due());

            match next {
                Some(Said::Opened(header, sound)) => {
                    self.clock = Some(Clock::new(&header, self.start, self.pace));
                    self.sound = sound.filter(|_| self.pace == Pace::Run);
                }
                Some(Said::Frame(frame)) if frame.index <= due => latest = Some(frame),
                Some(Said::Frame(frame)) => {
                    self.next = Some(frame);
                    break;
                }
                Some(Said::Failed) => {
                    self.failed = true;
                    drop(self.pending.take());
                    break;
                }
                None => break,
            }
        }

        if let Some(frame) = latest {
            self.show(ctx, frame, mode);
        }
    }

    /// `frame` into the texture; the clock starts at the first. Upscaled,
    /// it is kept, and the screenshot waits on for its upscale.
    fn show(&mut self, ctx: &egui::Context, frame: Decoded, mode: VideoUpscale) {
        if mode == VideoUpscale::Fsr {
            self.picture = Some(Arc::new(frame.image.clone()));
            self.fresh = true;
        } else {
            self.picture = None;
            drop(self.pending.take());
        }

        if let Some(texture) = &mut self.texture {
            texture.set(frame.image, egui::TextureOptions::LINEAR);
        } else {
            let name = format!("video {}", self.id);

            self.texture = Some(ctx.load_texture(name, frame.image, egui::TextureOptions::LINEAR));
        }

        self.shown = true;

        if let Some(clock) = &mut self.clock {
            clock.started();
        }
    }

    /// The frame shown, once there is one.
    pub fn frame(&self) -> Option<Frame> {
        self.texture.as_ref().map(|texture| Frame {
            plain: texture.id(),
            upscaled: self
                .upscaled
                .as_ref()
                .filter(|u| u.asked)
                .map(|u| (u.target.id, u.target.size)),
            wanted: Arc::clone(&self.wanted),
        })
    }

    /// The frame upscaled as `mode` says, within the GPU's largest
    /// texture side `limit`: a target at the pixels last shown at, the
    /// frame asked into it when new or the target is; none where it
    /// would not be larger, or with no egui textures to lend.
    fn upscale(
        &mut self,
        (mode, limit): (VideoUpscale, Option<u32>),
        lent: Option<(&mut Assets<Image>, &mut EguiUserTextures, &mut fsr::Asks)>,
    ) {
        let size = match (mode, limit, &self.picture, &lent) {
            (VideoUpscale::Fsr, Some(limit), Some(picture), Some(_)) => {
                let [w, h] = picture.size.map(|n| u32::try_from(n).unwrap_or(u32::MAX));

                unpacked(self.wanted.load(Ordering::Relaxed))
                    .and_then(|wanted| fsr::fitted(UVec2::new(w, h), wanted, limit))
            }
            _ => None,
        };

        if let (Some(size), Some(picture), Some((images, textures, asks))) =
            (size, &self.picture, lent)
        {
            // A new size: a new target, the frame asked into it.
            if self.upscaled.as_ref().is_none_or(|u| u.target.size != size) {
                self.upscaled = Some(Upscaled {
                    target: fsr::Target::new(images, textures, size),
                    asked: false,
                });
                self.fresh = true;
            }

            if let Some(upscaled) = &mut self.upscaled {
                // Asked last frame: shown this pass.
                if upscaled.asked {
                    drop(self.pending.take());
                }

                if self.fresh {
                    asks.0.push(fsr::Ask {
                        picture: Arc::clone(picture),
                        target: upscaled.target.image(),
                    });
                    upscaled.asked = true;
                    self.fresh = false;
                }
            }
        } else {
            self.upscaled = None;

            if self.shown {
                drop(self.pending.take());
            }
        }
    }

    /// Whether the last frame has had its time, or it would not play.
    pub fn ended(&self) -> bool {
        self.failed || self.clock.is_some_and(|c| c.ended())
    }

    /// Its sound, once, as its first frame shows: to start with it.
    pub const fn take_sound(&mut self) -> Option<Sound> {
        if self.shown { self.sound.take() } else { None }
    }

    pub const fn serial(&self) -> u64 {
        self.serial
    }
}

impl Drop for Playing {
    /// The worker stopped: its next send fails once the channel is gone.
    fn drop(&mut self) {
        let (_, closed) = sync_channel(0);

        drop(std::mem::replace(&mut self.said, Mutex::new(closed)));

        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

/// The worker: `id` opened from `source`, every frame decoded in turn,
/// those from `start` on sent as RGBA; it ends past the frames shown, at
/// an error, or when the player is gone.
fn work(id: FourCc, source: VideoSource, start: usize, send: &SyncSender<Said>) {
    let opened = open(id, source).and_then(|bytes| {
        let video = bink::Video::open(bytes.clone()).map_err(|e| format!("{id}: {e}"))?;
        let sound = bink::Audio::open(bytes.clone())
            .ok()
            .map(|_| Sound { bytes });

        Ok((video, sound))
    });
    let (mut video, sound) = match opened {
        Ok(opened) => opened,
        Err(why) => {
            warn!("{why}");
            let _ = send.send(Said::Failed);
            return;
        }
    };
    let header = *video.header();
    let clock = Clock::new(&header, start, Pace::Run);
    let size = [header.width, header.height];
    let mut rgba = Vec::new();

    if send.send(Said::Opened(header, sound)).is_err() {
        return;
    }

    for index in 0..clock.frames() {
        let frame = match video.next_frame() {
            Some(Ok(frame)) => frame,
            Some(Err(e)) => {
                warn!("video {id} frame {index}: {e}");
                let _ = send.send(Said::Failed);
                return;
            }
            None => return,
        };

        if index >= clock.start() {
            frame.rgba(&mut rgba);

            let image = egui::ColorImage::from_rgba_unmultiplied(size, &rgba);

            if send.send(Said::Frame(Decoded { index, image })).is_err() {
                return;
            }
        }
    }
}

// --------------------------------------------------------------------------
// The sound
// --------------------------------------------------------------------------

/// A video's sound track, played through Bevy's audio, decoded a frame's
/// packet at a time as the audio thread asks.
#[derive(Asset, TypePath, Clone, Debug)]
pub struct Sound {
    bytes: Bytes,
}

/// A [`Sound`] being played.
pub struct Heard {
    audio: Option<bink::Audio<Bytes>>,
    channels: NonZero<u16>,
    rate: NonZero<u32>,
    /// The last packet's samples, and the next to give.
    samples: Vec<f32>,
    at: usize,
}

impl Iterator for Heard {
    type Item = bevy::audio::Sample;

    fn next(&mut self) -> Option<Self::Item> {
        // A frame with no audio gives none: on to the next.
        while self.at == self.samples.len() {
            let audio = self.audio.as_mut()?;

            match audio.next_packet() {
                Some(Ok(samples)) => {
                    self.samples.clear();
                    self.samples.extend_from_slice(samples);
                    self.at = 0;
                }
                Some(Err(e)) => {
                    warn!("video sound: {e}");
                    self.audio = None;
                }
                None => self.audio = None,
            }
        }

        let sample = self.samples[self.at];

        self.at += 1;
        Some(sample)
    }
}

impl bevy::audio::Source for Heard {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> bevy::audio::ChannelCount {
        self.channels
    }

    fn sample_rate(&self) -> bevy::audio::SampleRate {
        self.rate
    }

    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

impl Decodable for Sound {
    type Decoder = Heard;

    fn decoder(&self) -> Heard {
        let audio = bink::Audio::open(self.bytes.clone()).ok();
        let track = audio.as_ref().map(|a| *a.track());
        let one = NonZero::<u16>::MIN;

        Heard {
            channels: track
                .and_then(|t| NonZero::new(u16::try_from(t.channels).ok()?))
                .unwrap_or(one),
            rate: track
                .and_then(|t| NonZero::new(t.rate))
                .unwrap_or(NonZero::<u32>::MIN),
            audio,
            samples: Vec::new(),
            at: 0,
        }
    }
}

/// The sound of the menus' video playing, as its first frame shows,
/// at the music's loudness (**Inferred**: the original's Bink plays
/// through its own `DirectSound` buffer, at no setting of the game's);
/// stopped as the video goes.
#[allow(clippy::needless_pass_by_value, reason = "a Bevy system")]
fn sound(
    mut commands: Commands,
    mut video: ResMut<crate::menu::Video>,
    mut sounds: ResMut<Assets<Sound>>,
    settings: Res<crate::files::config::Settings>,
    mut heard: Local<Option<(u64, Entity)>>,
) {
    let playing = video.playing_mut();
    let serial = playing.as_ref().map(|p| p.serial());

    if let Some((was, entity)) = *heard
        && Some(was) != serial
    {
        commands.entity(entity).try_despawn();
        *heard = None;
    }

    if let Some(playing) = playing
        && let Some(sound) = playing.take_sound()
    {
        let volume = bevy::audio::Volume::Linear(settings.audio.music_now());
        let entity = commands
            .spawn((
                AudioPlayer(sounds.add(sound)),
                PlaybackSettings::DESPAWN.with_volume(volume),
            ))
            .id();

        *heard = Some((playing.serial(), entity));
    }
}

/// The menus' video upscaled as `[enhancements] video_upscale` says,
/// after the pass that drew it (`Playing::upscale`); none without a GPU.
#[allow(clippy::needless_pass_by_value, reason = "a Bevy system")]
fn upscale(
    mut video: ResMut<crate::menu::Video>,
    settings: Res<crate::files::config::Settings>,
    side: Option<Res<regent_draw::art::side::Side>>,
    images: ResMut<Assets<Image>>,
    textures: Option<ResMut<EguiUserTextures>>,
    asks: Option<ResMut<fsr::Asks>>,
) {
    let mode = settings.enhancements.video_upscale;
    let side = side.map(|s| u32::try_from(s.0).unwrap_or(u32::MAX));

    if let Some(playing) = video.playing_mut() {
        let lent = textures
            .zip(asks)
            .map(|(t, a)| (images.into_inner(), t.into_inner(), a.into_inner()));

        playing.upscale((mode, side), lent);
    }
}

/// The videos' sound and upscale: the sound's source, and the systems
/// that start and stop it and upscale the frames.
pub struct Plugin;

impl bevy::app::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        // Not the menus' alone: an end panel's film plays over the game.
        let held = resource_exists::<crate::menu::Video>;

        app.add_sound::<Sound>()
            .add_systems(Update, sound.run_if(held))
            .add_systems(
                PostUpdate,
                upscale.run_if(held).after(EguiPostUpdateSet::EndPass),
            );
    }
}

/// A `PICT` record holding `payload`, in an archive made in memory.
#[cfg(test)]
pub fn held(payload: &'static [u8]) -> cam::Record<'static> {
    let chunk = cam::Written {
        tag: FourCc(*b"PICT"),
        flag: cam::ChunkFlag::Clear,
        records: vec![(FourCc(*b"TEST"), "test", payload)],
    };
    let bytes: &'static [u8] = cam::write(&[chunk]).leak();

    cam::Archive::parse(bytes).expect("an archive").chunks[0].records[0]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 15 fps video of `frames` frames.
    fn header(frames: usize) -> bink::Header {
        bink::Header {
            revision: b'h',
            width: 260,
            height: 260,
            frames,
            rate_num: 15,
            rate_den: 1,
            flags: 0,
            audio_tracks: 0,
        }
    }

    /// One frame's time at 15 fps, and a little.
    const FRAME: Duration = Duration::from_millis(67);

    #[test]
    fn the_clock_waits_for_the_first_frame_then_runs_at_the_videos_rate() {
        let mut clock = Clock::new(&header(10), 0, Pace::Run);

        clock.tick(FRAME * 5);
        assert_eq!(clock.due(), 0, "not started: no time counts");

        clock.started();
        clock.tick(FRAME);
        assert_eq!(clock.due(), 1);

        clock.tick(FRAME * 2);
        assert_eq!(clock.due(), 3);
    }

    #[test]
    fn a_video_ends_once_its_last_shown_frame_has_had_its_time() {
        // 10 frames, 9 shown: the last is never drawn.
        let mut clock = Clock::new(&header(10), 0, Pace::Run);

        clock.started();
        clock.tick(FRAME * 8);
        assert_eq!(clock.due(), 8);
        assert!(!clock.ended());

        clock.tick(FRAME);
        assert!(clock.ended());
        assert_eq!(clock.due(), 9, "held past the end");
    }

    #[test]
    fn a_held_clock_stays_on_its_start_frame() {
        let mut clock = Clock::new(&header(10), 4, Pace::Held);

        clock.started();
        clock.tick(FRAME * 100);
        assert_eq!(clock.due(), 4);
        assert!(!clock.ended());
    }

    /// A frame upscaled at `size`, its wanted pixels none yet.
    fn frame(size: UVec2) -> Frame {
        Frame {
            plain: egui::TextureId::Managed(1),
            upscaled: Some((egui::TextureId::User(2), size)),
            wanted: Arc::new(AtomicU64::new(0)),
        }
    }

    #[test]
    fn the_upscaled_frame_shows_where_made_at_the_rects_pixels_on_whole_ones() {
        // 640 x 480 in 2.5 points a pixel, at 2 pixels a point.
        let frame = frame(UVec2::new(3200, 2400));
        let rect = egui::Rect::from_min_size(egui::pos2(10.3, 0.0), egui::vec2(1600.0, 1200.0));
        let (texture, shown) = frame.at(2.0, rect);

        assert_eq!(texture, egui::TextureId::User(2));
        assert_eq!(shown.min, egui::pos2(10.5, 0.0), "on a whole pixel");
        assert_eq!(shown.size(), rect.size());
        assert_eq!(
            unpacked(frame.wanted.load(Ordering::Relaxed)),
            Some(UVec2::new(3200, 2400))
        );
    }

    #[test]
    fn a_frame_not_upscaled_at_the_rects_pixels_shows_plain_and_asks_for_them() {
        // The window grew: the upscale is at the old size.
        let frame = frame(UVec2::new(1600, 1200));
        let rect = egui::Rect::from_min_size(egui::pos2(10.3, 0.0), egui::vec2(1600.0, 1200.0));
        let (texture, shown) = frame.at(1.5, rect);

        assert_eq!(texture, egui::TextureId::Managed(1));
        assert_eq!(shown, rect, "as drawn without an upscale");
        assert_eq!(
            unpacked(frame.wanted.load(Ordering::Relaxed)),
            Some(UVec2::new(2400, 1800))
        );
    }

    #[test]
    fn the_upscale_follows_the_pixels_shown_at_and_asks_only_for_changes() {
        let mut playing = Playing::new(
            FourCc(*b"QM00"),
            VideoSource::Record(held(b"not bink")),
            0,
            Pace::Run,
        )
        .expect("a worker");
        let (mut images, mut textures) = (Assets::<Image>::default(), EguiUserTextures::default());
        let mut asks = fsr::Asks::default();
        let mut step = |playing: &mut Playing, wanted: UVec2| {
            asks.0.clear();
            playing.wanted.store(packed(wanted), Ordering::Relaxed);
            playing.upscale(
                (VideoUpscale::Fsr, Some(8192)),
                Some((&mut images, &mut textures, &mut asks)),
            );

            (
                asks.0.len(),
                playing.upscaled.as_ref().map(|u| u.target.size),
            )
        };

        playing.picture = Some(Arc::new(egui::ColorImage::filled(
            [640, 480],
            egui::Color32::BLACK,
        )));
        playing.fresh = true;

        let full = UVec2::new(1600, 1200);

        assert_eq!(step(&mut playing, full), (1, Some(full)), "the new frame");
        assert_eq!(step(&mut playing, full), (0, Some(full)), "nothing new");

        let larger = UVec2::new(2400, 1800);

        assert_eq!(step(&mut playing, larger), (1, Some(larger)), "re-targeted");
        assert_eq!(step(&mut playing, UVec2::new(640, 480)), (0, None), "1:1");
    }

    #[test]
    fn a_start_past_the_shown_frames_is_the_last_shown() {
        assert_eq!(Clock::new(&header(10), 50, Pace::Run).start(), 8);
    }
}
