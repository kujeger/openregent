//! openregent's own save files: one game in one `.tar.zst`, a tar as one
//! zstd stream, so `tar --zstd -xf` unpacks it. Not the original's
//! `.gmp`.
//!
//! ```text
//!  $XDG_DATA_HOME/openregent/profiles/<dir>/saves/<name>.tar.zst     one player
//!  $XDG_DATA_HOME/openregent/profiles/<dir>/mp-saves/<name>.tar.zst  LAN
//!    meta.ron       version, name, the game, its time, the date, the
//!                   profile, and what it needs: archives, GPL images
//!    minimap.png    the minimap as the player saw it
//!    snapshot.ron   the game (regent_core::save), the interface's part and
//!                   the reward panel's settings
//!    play.input     the session from its start (`record`): a replay
//! ```
//!
//! In that order, so a list reads only the first two members: a few KB
//! of the stream. A file of another version is refused, not migrated.

pub(crate) mod auto;
pub(crate) mod dialog;
pub(crate) mod lan;
pub(crate) mod thread;

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use bevy::log::warn;
use regent_core::data::text::Text;
use regent_core::sim::world::flag;
use regent_core::view::menu::saves::{NAME_CHARS, Stamp};
use regent_core::view::panel::minimap::Picture;
use regent_core::view::panel::scroll;
use serde::{Deserialize, Serialize};

use crate::game::sim::{Session, Sim};
use crate::ui::options::Play;

/// A save's file name ends so.
pub const EXTENSION: &str = ".tar.zst";

/// The container's format: `meta.ron` and the interface's part.
/// The game's own snapshot has its own (`regent_core::save::VERSION`).
pub const VERSION: u32 = 1;

/// A profile's single-player saves, in its directory.
const SAVES: &str = "saves";

/// A profile's LAN saves, in its directory.
const LAN_SAVES: &str = "mp-saves";

/// The members, in order.
const META: &str = "meta.ron";
const MINIMAP: &str = "minimap.png";
const SNAPSHOT: &str = "snapshot.ron";
const PLAY: &str = "play.input";

/// zstd's level: its default.
const LEVEL: i32 = 3;

/// A member's mode in the tar.
const MODE: u32 = 0o644;

/// What a save's game is.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SaveKind {
    /// A quest, by key, and its title.
    Quest { key: String, title: String },
    /// A custom game: its title scroll's name and the picks' codes.
    Custom {
        title: String,
        land: String,
        settlement: String,
        scenario: String,
        monsters: String,
        map: u32,
    },
    /// Anything else, as its title scroll names it.
    Other { title: String },
}

/// What a save needs of the install: a mismatch is refused.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Needs {
    /// The archives, under the install.
    pub archives: Vec<String>,
    /// The GPL images' bytes, hashed ([`hash`]).
    pub images: String,
    /// The mods the game loaded, in order: as HD's save lists each
    /// applied mod's GUID and name, so a missing
    /// one is named.
    pub mods: Vec<Needed>,
}

/// A mod a save needs: its GUID and the name it showed; for one
/// compiled from source, its images' hash (`compiled`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Needed {
    pub guid: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flags: Option<String>,
}

/// `meta.ron`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Meta {
    pub version: u32,
    pub name: String,
    pub game: SaveKind,
    /// Frames run, and the game clock, ms.
    pub frame: u32,
    pub clock: u32,
    /// Seconds since 1970, UTC.
    pub saved: u64,
    pub profile: String,
    pub needs: Needs,
    /// A LAN game's: its seats and the game's hash. `None`, a game of
    /// one.
    pub lan: Option<Lan>,
}

/// What a LAN game's save says of it: who sat where, its delay, and a
/// hash of its game ([`game_hash`]), the same on every peer's save of
/// it, by which a lobby finds a peer's copy.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lan {
    /// Each seat's nick, by seat: its player's profile name, else
    /// `p<seat>`.
    pub seats: Vec<String>,
    /// Frames an order waited.
    pub delay: u32,
    pub game: String,
}

/// The interface's part of a game: what was selected, and where the view
/// looked.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Interface {
    pub selected: Option<u32>,
    pub reading: Option<u32>,
    /// The view's centre, world units, and zoom.
    pub pan: (f32, f32),
    pub zoom: f32,
    /// The tracking window, as the original's `GSGL` keeps it.
    pub tracking: regent_core::view::panel::tracking::Tracker,
}

/// The reward panel's two settings: this machine's, no play's
/// (`regent_core::sim::world::flag::Rewards`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rewards {
    pub attack: i32,
    pub explore: i32,
}

impl From<Rewards> for flag::Rewards {
    fn from(r: Rewards) -> flag::Rewards {
        flag::Rewards {
            attack: r.attack,
            explore: r.explore,
        }
    }
}

impl From<flag::Rewards> for Rewards {
    fn from(r: flag::Rewards) -> Rewards {
        Rewards {
            attack: r.attack,
            explore: r.explore,
        }
    }
}

/// `snapshot.ron`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub game: regent_core::save::Snapshot,
    pub interface: Interface,
    pub rewards: Rewards,
}

/// A whole save.
#[derive(Clone, Debug, PartialEq)]
pub struct Contents {
    pub meta: Meta,
    /// `minimap.png`'s bytes.
    pub minimap: Vec<u8>,
    pub snapshot: Snapshot,
    /// `play.input`'s text.
    pub play: String,
}

/// A save as listed: its file and what it says of itself.
#[derive(Clone, Debug)]
pub struct Listed {
    pub path: PathBuf,
    pub glance: Result<Glance, String>,
}

/// What a list reads: the meta and the minimap.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Glance {
    pub meta: Meta,
    pub minimap: Vec<u8>,
}

/// What a save takes of the [`Sim`], taken where it lives (the sim
/// thread's `Command::Save`).
pub struct FromSim {
    /// The game, or why it makes none ([`Sim::snapshot`]).
    game: Result<regent_core::save::Snapshot, String>,
    /// The session from the start ([`Sim::session`]).
    play: Option<crate::game::record::Session>,
    frame: u32,
    clock: u32,
    rewards: flag::Rewards,
    /// A LAN game's seats, by nick, and its delay.
    seats: Option<(Vec<String>, u32)>,
}

impl FromSim {
    pub fn of(sim: &Sim) -> FromSim {
        let peer = sim.peer();
        let seats = (peer.players() > 1).then(|| {
            (
                (0..peer.players()).map(|s| peer.nick(s)).collect(),
                peer.delay(),
            )
        });

        FromSim {
            game: sim.snapshot(),
            play: sim.session(),
            frame: sim.frames(),
            clock: sim.world().now(),
            rewards: sim.rewards(),
            seats,
        }
    }

    /// Why [`made`] will make no save of it, found without making one:
    /// a game that stopped, makes no snapshot, or was not recorded.
    ///
    /// # Errors
    ///
    /// That reason, as [`made`] gives it.
    pub fn ready(&self) -> Result<(), String> {
        match (&self.game, &self.play) {
            (Err(why), _) => Err(why.clone()),
            (Ok(_), None) => Err(NOT_RECORDED.to_owned()),
            (Ok(_), Some(_)) => Ok(()),
        }
    }
}

/// Why a game not recorded makes no save.
const NOT_RECORDED: &str = "a game not recorded does not save";

/// A LAN game's save, taken on every peer as one step left the game
/// (`net`): which seat asked it, its name, and the game.
pub struct Stepped {
    pub seat: u32,
    /// The asker's nick.
    pub nick: String,
    pub name: String,
    pub game: FromSim,
}

/// What `sim` saves as, unnamed and of no profile: its game, time,
/// needs, the minimap `picture`, its snapshot with `interface`, and its
/// session from the start.
///
/// # Errors
///
/// A game that stopped, was not recorded, or makes no snapshot.
#[cfg(test)]
pub fn of(
    (sim, session): (FromSim, &Session),
    text: &Text<'_>,
    picture: Option<&Picture>,
    interface: Interface,
) -> Result<Contents, String> {
    let (kind, needs) = described(session, text);

    made(sim, (kind, needs), png(picture), interface)
}

/// What a save says of `session`'s game: what game it is, and what it
/// needs.
pub fn described(session: &Session, text: &Text<'_>) -> (SaveKind, Needs) {
    let title = |g| scroll::title(g, text).unwrap_or_default();
    let kind = match (&session.quest, &session.setup) {
        (Some((key, name)), _) => SaveKind::Quest {
            key: key.to_string(),
            title: name.clone(),
        },
        (None, Some(setup)) => SaveKind::Custom {
            title: title(scroll::ScrollSubject::Custom(setup)),
            land: setup.land().to_string(),
            settlement: setup.settlement().to_string(),
            scenario: setup.scenario().to_string(),
            monsters: setup.monsters().to_string(),
            map: setup.seed(),
        },
        (None, None) => SaveKind::Other {
            title: OTHER.to_owned(),
        },
    };

    (kind, session.needs.clone())
}

/// The save of `sim`, the game `kind` needing `needs`, with the
/// minimap as PNG: the costly part, which needs neither the session
/// nor the texts, so it runs on the save thread (`save::thread`).
///
/// # Errors
///
/// A game that stopped, was not recorded, or makes no snapshot.
pub fn made(
    sim: FromSim,
    (kind, needs): (SaveKind, Needs),
    minimap: Vec<u8>,
    interface: Interface,
) -> Result<Contents, String> {
    let game = sim.game?;
    let play = sim.play.ok_or(NOT_RECORDED)?.text();
    let lan = sim
        .seats
        .map(|(seats, delay)| {
            Ok::<_, String>(Lan {
                seats,
                delay,
                game: game_hash(&game, &play)?,
            })
        })
        .transpose()?;

    Ok(Contents {
        meta: Meta {
            version: VERSION,
            name: String::new(),
            game: kind,
            frame: sim.frame,
            clock: sim.clock,
            saved: now(),
            profile: String::new(),
            needs,
            lan,
        },
        minimap,
        snapshot: Snapshot {
            game,
            interface,
            rewards: sim.rewards.into(),
        },
        play,
    })
}

/// What a game neither a quest nor a custom game is called.
const OTHER: &str = "A game";

/// A profile's saves directory for games of `play`: single-player and
/// LAN saves apart.
pub fn dir(profile: &Path, play: Play) -> PathBuf {
    profile.join(match play {
        Play::Alone => SAVES,
        Play::Lan => LAN_SAVES,
    })
}

/// The file a save named `name` goes to in `dir`: the name made safe as
/// a profile's is (`profile::safe`), at most the original's
/// [`NAME_CHARS`] long, its file name at most [`FILE_NAME_BYTES`].
///
/// # Errors
///
/// A name that will not do.
pub fn path(dir: &Path, name: &str) -> Result<PathBuf, String> {
    if name.trim().chars().count() > NAME_CHARS {
        Err(format!("a name of at most {NAME_CHARS} characters"))
    } else {
        let stem = crate::files::profile::safe(name).map_err(|e| e.to_string())?;
        let file = format!("{stem}{EXTENSION}");

        // Wide characters can pass the count and not the file system.
        if file.len() > FILE_NAME_BYTES {
            Err("a name too long for a file name".to_owned())
        } else {
            Ok(dir.join(file))
        }
    }
}

/// A file name's bytes at most: Linux's `NAME_MAX`.
const FILE_NAME_BYTES: usize = 255;

/// The file and name a save named `name` goes to in `dir` without
/// overwriting one: `name`, else `name 2`, `name 3`, ... up to
/// [`MOST_COPIES`].
///
/// # Errors
///
/// A name that will not do, or none free.
pub fn free(dir: &Path, name: &str) -> Result<(PathBuf, String), String> {
    // A copy's number fits: "name 2" cut to [`NAME_CHARS`] at the name.
    let named = |n: u32| match n {
        1 => name.to_owned(),
        n => {
            let number = format!(" {n}");
            let room = NAME_CHARS.saturating_sub(number.len());
            let cut: String = name.trim().chars().take(room).collect();

            format!("{cut}{number}")
        }
    };
    let taken = |found: &Result<(PathBuf, String), String>| match found {
        Ok((path, _)) => path.exists(),
        Err(_) => false,
    };

    (1..=MOST_COPIES)
        .map(|n| path(dir, &named(n)).map(|p| (p, named(n))))
        .find(|found| !taken(found))
        .unwrap_or_else(|| Err(format!("{name}: no free name")))
}

/// Saves of one name [`free`] tries at most.
const MOST_COPIES: u32 = 999;

/// Write `contents` to `path`, through a file beside it, so a failed
/// write leaves the old save.
///
/// # Errors
///
/// A file that will not be written.
pub fn write(path: &Path, contents: &Contents) -> Result<(), String> {
    let shown = |e: &dyn std::fmt::Display| format!("{}: {e}", path.display());
    let config = ron::ser::PrettyConfig::default();
    let meta = ron::ser::to_string_pretty(&contents.meta, config.clone()).map_err(|e| shown(&e))?;
    // A map's heights and flags an array a line, not a value a line.
    let snapshot = ron::ser::to_string_pretty(&contents.snapshot, config.compact_arrays(true))
        .map_err(|e| shown(&e))?;
    let members: [(&str, &[u8]); 4] = [
        (META, meta.as_bytes()),
        (MINIMAP, &contents.minimap),
        (SNAPSHOT, snapshot.as_bytes()),
        (PLAY, contents.play.as_bytes()),
    ];

    pack(path, &members, contents.meta.saved)
}

/// A save's start alone, `meta` and an empty minimap: all [`glance`]
/// reads.
#[cfg(test)]
pub(crate) fn write_glance(path: &Path, meta: &Meta) -> Result<(), String> {
    let text = ron::ser::to_string(meta).map_err(|e| e.to_string())?;

    pack(path, &[(META, text.as_bytes()), (MINIMAP, &[])], meta.saved)
}

/// `members` written to `path` as a save's archive, each stamped
/// `mtime`; a failed write leaves no partial file.
fn pack(path: &Path, members: &[(&str, &[u8])], mtime: u64) -> Result<(), String> {
    let shown = |e: &dyn std::fmt::Display| format!("{}: {e}", path.display());
    let partial = path.with_extension("part");

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| shown(&e))?;
    }

    let packed = archive(&partial, members, mtime).map_err(|e| shown(&e));

    if packed.is_err() {
        let _ = std::fs::remove_file(&partial);
    }

    packed?;
    std::fs::rename(&partial, path).map_err(|e| shown(&e))
}

/// [`pack`]'s archive written whole to `partial`, on the disk.
fn archive(partial: &Path, members: &[(&str, &[u8])], mtime: u64) -> std::io::Result<()> {
    let file = std::fs::File::create(partial)?;
    let zstd = zstd::Encoder::new(file, LEVEL)?;
    let mut tar = tar::Builder::new(zstd);

    for (name, bytes) in members {
        let mut header = tar::Header::new_gnu();

        header.set_size(bytes.len() as u64);
        header.set_mode(MODE);
        header.set_mtime(mtime);
        header.set_cksum();
        tar.append_data(&mut header, name, *bytes)?;
    }

    let mut file = tar.into_inner()?.finish()?;

    file.flush()?;
    file.sync_all()
}

/// The members of `path` in order, up to `last` of them, each read
/// whole.
fn members(path: &Path, last: usize) -> Result<Vec<(String, Vec<u8>)>, String> {
    let shown = |e: &dyn std::fmt::Display| format!("{}: {e}", path.display());
    let file = std::fs::File::open(path).map_err(|e| shown(&e))?;
    let zstd = zstd::Decoder::new(file).map_err(|e| shown(&e))?;
    let mut tar = tar::Archive::new(zstd);
    let mut out = Vec::new();

    for entry in tar.entries().map_err(|e| shown(&e))?.take(last) {
        let mut entry = entry.map_err(|e| shown(&e))?;
        let name = entry
            .path()
            .map_err(|e| shown(&e))?
            .to_string_lossy()
            .into_owned();
        let mut bytes = Vec::new();

        entry.read_to_end(&mut bytes).map_err(|e| shown(&e))?;
        out.push((name, bytes));
    }

    Ok(out)
}

/// Member `name` of `members`, which must stand at `at`.
fn member<'m>(members: &'m [(String, Vec<u8>)], at: usize, name: &str) -> Result<&'m [u8], String> {
    members
        .get(at)
        .filter(|(n, _)| n == name)
        .map(|(_, bytes)| bytes.as_slice())
        .ok_or_else(|| format!("no {name} where a save has it"))
}

/// A RON member, read.
fn ron_of<T: serde::de::DeserializeOwned>(bytes: &[u8], name: &str) -> Result<T, String> {
    let text = std::str::from_utf8(bytes).map_err(|e| format!("{name}: {e}"))?;

    ron::from_str(text).map_err(|e| format!("{name}: {e}"))
}

/// `meta`, if this build reads its version.
fn checked(meta: Meta) -> Result<Meta, String> {
    if meta.version == VERSION {
        Ok(meta)
    } else {
        Err(format!(
            "save format version {}, this build reads {VERSION}",
            meta.version
        ))
    }
}

/// The meta and minimap of `path`: only the stream's start is read.
///
/// # Errors
///
/// A file that will not read, or one of another version.
pub fn glance(path: &Path) -> Result<Glance, String> {
    let read = members(path, 2)?;
    let meta = checked(ron_of(member(&read, 0, META)?, META)?)?;

    Ok(Glance {
        meta,
        minimap: member(&read, 1, MINIMAP)?.to_vec(),
    })
}

/// The whole save at `path`.
///
/// # Errors
///
/// A file that will not read, or one of another version.
pub fn read(path: &Path) -> Result<Contents, String> {
    let read = members(path, 4)?;
    let meta = checked(ron_of(member(&read, 0, META)?, META)?)?;
    let snapshot: Snapshot = ron_of(member(&read, 2, SNAPSHOT)?, SNAPSHOT)?;
    let play = String::from_utf8(member(&read, 3, PLAY)?.to_vec()).map_err(|e| e.to_string())?;

    Ok(Contents {
        meta,
        minimap: member(&read, 1, MINIMAP)?.to_vec(),
        snapshot: Snapshot {
            game: snapshot.game.checked().map_err(|e| e.to_string())?,
            interface: snapshot.interface,
            rewards: snapshot.rewards,
        },
        play,
    })
}

/// The saves in `dir`, newest first; each glanced at.
pub fn list(dir: &Path) -> Vec<Listed> {
    let mut listed: Vec<Listed> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.to_string_lossy().ends_with(EXTENSION))
        .map(|path| Listed {
            glance: glance(&path),
            path,
        })
        .collect();

    listed.sort_by_key(|l| {
        let saved = l.glance.as_ref().map_or(0, |g| g.meta.saved);

        (std::cmp::Reverse(saved), l.path.clone())
    });
    listed
}

/// The minimap as PNG; a black one of the minimap's size where there is
/// none (a game not drawn).
pub fn png(picture: Option<&Picture>) -> Vec<u8> {
    let side = usize::try_from(
        regent_core::view::panel::minimap::RECT[2] - regent_core::view::panel::minimap::RECT[0],
    )
    .unwrap_or_default();
    let (width, height, rgba) = picture.map_or_else(
        || (side, side, [0, 0, 0, u8::MAX].repeat(side * side)),
        |p| (p.width, p.height, p.rgba()),
    );
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(
        &mut out,
        u32::try_from(width).unwrap_or_default(),
        u32::try_from(height).unwrap_or_default(),
    );

    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);

    let written = encoder
        .write_header()
        .and_then(|mut w| w.write_image_data(&rgba));

    if let Err(e) = written {
        warn!("minimap: {e}");
    }

    out
}

/// A PNG's size and RGBA pixels.
pub fn unpng(bytes: &[u8]) -> Option<([usize; 2], Vec<u8>)> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));

    // 8 bits a channel, whatever the file's: a reader wants 4 bytes a pixel.
    decoder.set_transformations(
        png::Transformations::EXPAND | png::Transformations::ALPHA | png::Transformations::STRIP_16,
    );

    let mut reader = decoder.read_info().ok()?;
    let mut buffer = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buffer).ok()?;

    buffer.truncate(info.buffer_size());

    let size = [info.width, info.height].map(|n| usize::try_from(n).unwrap_or_default());
    let rgba = info.color_type == png::ColorType::Rgba && buffer.len() == size[0] * size[1] * 4;

    rgba.then_some((size, buffer))
}

pub use crate::files::store::hash;

/// A game's hash: its snapshot as compact RON, then its `play` text
/// ([`hash`]). Every peer's save of a LAN game at one step has the same:
/// the interface's part, the minimap and the meta are left out.
///
/// # Errors
///
/// A snapshot RON will not write.
pub fn game_hash(game: &regent_core::save::Snapshot, play: &str) -> Result<String, String> {
    let ron = ron::to_string(game).map_err(|e| e.to_string())?;

    Ok(hash([ron.as_bytes(), play.as_bytes()]))
}

/// Why `needs` is not what this install gives: `None` when it is.
pub fn mismatch(needs: &Needs, here: &Needs) -> Option<String> {
    let named = |mods: &[Needed]| {
        let names: Vec<&str> = mods.iter().map(|m| m.name.as_str()).collect();

        if names.is_empty() {
            "none".to_owned()
        } else {
            names.join(", ")
        }
    };

    // One GUID, another source compiled: the mod named.
    let recompiled = needs
        .mods
        .iter()
        .zip(&here.mods)
        .find(|(a, b)| a.guid == b.guid && a.image != b.image);
    let repatched = needs
        .mods
        .iter()
        .zip(&here.mods)
        .find(|(a, b)| a.guid == b.guid && a.flags != b.flags);

    if let Some((m, _)) = recompiled {
        Some(format!(
            "the save's mod {} was compiled from other GPL source",
            m.name
        ))
    } else if let Some((m, _)) = repatched {
        Some(format!(
            "the save's mod {} has different unit flags",
            m.name
        ))
    } else if needs.mods != here.mods {
        Some(format!(
            "the save was made with mods {}, this game loads {}",
            named(&needs.mods),
            named(&here.mods)
        ))
    } else if needs.images != here.images {
        Some("the save's GPL images differ from this install's".to_owned())
    } else if needs.archives != here.archives {
        Some("the save's archives differ from this install's".to_owned())
    } else {
        None
    }
}

/// Seconds a day.
const DAY_SECONDS: u64 = 86_400;

/// Now, seconds since 1970.
pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Seconds an hour and a minute.
const HOUR_SECONDS: i64 = 3_600;
const MINUTE_SECONDS: i64 = 60;

/// `seconds` since 1970 as a local year, month from 1, and day.
/// **Inferred**: the original dates its saves by the local clock.
pub fn day_of(seconds: u64) -> (i64, usize, i64) {
    let (y, m, d, _, _) = stamp_of(seconds, offset(seconds));

    (y, m, d)
}

/// `seconds` since 1970, `ahead` seconds ahead of UTC, as a save list's
/// row shows it: "2026-10-08 14:30" (`saves::stamp`).
pub fn stamp_of(seconds: u64, ahead: i64) -> Stamp {
    let local = i64::try_from(seconds).unwrap_or_default() + ahead;
    let day = i64::try_from(DAY_SECONDS).unwrap_or_default();
    let (y, m, d) = civil(local.div_euclid(day));
    let into = local.rem_euclid(day);

    (
        y,
        usize::try_from(m).unwrap_or_default(),
        d,
        into / HOUR_SECONDS,
        into % HOUR_SECONDS / MINUTE_SECONDS,
    )
}

/// How far this machine's clock is ahead of UTC at `seconds` since
/// 1970, its time zone's summer time included.
pub fn offset(seconds: u64) -> i64 {
    let at = jiff::Timestamp::from_second(i64::try_from(seconds).unwrap_or_default())
        .unwrap_or_default();

    i64::from(jiff::tz::TimeZone::system().to_offset(at).seconds())
}

/// A day count since 1970 as year, month, day: Howard Hinnant's
/// `civil_from_days`.
const fn civil(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };

    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Single-player and LAN saves go to folders of their own.
    #[test]
    fn single_player_and_lan_saves_go_to_their_own_folders() {
        let profile = Path::new("p");

        assert_eq!(dir(profile, Play::Alone), profile.join("saves"));
        assert_eq!(dir(profile, Play::Lan), profile.join("mp-saves"));
    }
    use regent_core::sim::world::offer::Menu;

    use crate::files::env::Install;
    use crate::game::begin::{Begun, Plan};
    use crate::game::record::{self, Recorder};

    /// A fresh scratch folder of its own.
    fn scratch(name: &str) -> crate::testdata::Scratch {
        crate::testdata::scratch(&format!("saves-{name}"))
    }

    /// The images' hash is FNV-1a over their bytes run together, as
    /// saves made before it was cached hashed the files ("a" is FNV-1a's
    /// published test vector).
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn the_images_hash_is_fnv1a_over_the_files_in_order() {
        assert_eq!(hash([b"a".as_slice()]), "af63dc4c8601ec8c");
        assert_eq!(hash([b"ab".as_slice()]), hash([b"a".as_slice(), b"b"]));
        assert_ne!(hash([b"ab".as_slice()]), hash([b"b".as_slice(), b"a"]));

        let install = Install::from_env().expect("MAJESTY_HD");
        let paths = regent_core::content::dataset::Install::new(
            &install.hd,
            regent_core::content::dataset::Bytecode::Sdk(&install.sdk),
        )
        .files(regent_core::content::dataset::Datasets::Expansion)
        .expect("the dataset")
        .programs;
        let files: Vec<Vec<u8>> = paths
            .iter()
            .map(|p| std::fs::read(p).expect("an image"))
            .collect();

        assert_eq!(
            crate::game::load::images_hash(&paths).expect("hashed"),
            hash(files.iter().map(Vec::as_slice))
        );
    }

    /// The interface a headless game saves with.
    fn interface() -> Interface {
        Interface {
            selected: None,
            reading: None,
            pan: (1.0, 2.0),
            zoom: 1.0,
            tracking: regent_core::view::panel::tracking::Tracker::default(),
        }
    }

    /// The game `setup` asks for, recorded to `dir`, and what it is.
    fn begun(
        setup: &crate::setup::Setup,
        catalogue: &regent_core::data::catalogue::Catalogue<'_>,
        dir: &Path,
    ) -> (Sim, Session) {
        let install = Install::from_env().expect("MAJESTY_HD");
        let Begun {
            mut sim, session, ..
        } = Plan::of(&install, setup, None)
            .and_then(|plan| plan.begin(catalogue))
            .expect("begun");

        sim.record(Recorder::open(
            &dir.join("play.input"),
            &dir.join("play.input.1"),
            &setup.recorded(),
        ));
        (sim, session)
    }

    /// `frames` frames of `sim`, `bot` pressing.
    fn played(sim: &mut Sim, bot: &mut Option<crate::game::bot::Bot>, frames: u32) {
        for _ in 0..frames {
            if let Some(bot) = bot {
                for input in bot.moves(sim) {
                    sim.input(input);
                }
            }

            sim.step();
        }
    }

    /// A save's cost in a late 512-cell game, the perf runs' `d512-late`
    /// game: the part on the sim thread ([`FromSim::of`]); the frame's
    /// stall, a save made and written in it as SAVE GAME once did, and a
    /// save queued to the save thread as every save is now; and the save
    /// thread's part. Measured, not checked:
    /// `cargo test --release -- --ignored save_cost`.
    #[test]
    #[ignore = "slow: a measurement"]
    fn save_cost_of_a_late_512_cell_game() {
        const AT: u32 = 14_000;

        let dir = scratch("save_cost");
        let catalogue = crate::testdata::hd();
        let text = Text::build(catalogue);
        let cli = crate::cli::tests::parsed(["openregent", "--map-seed", "7", "--side", "512"]);
        let mut bot = Some(crate::game::bot::Bot::new(Menu::of(catalogue), 5));
        let (mut sim, session) = begun(&cli, catalogue, &dir);

        played(&mut sim, &mut bot, AT);
        assert_eq!((sim.frames(), sim.stopped.as_deref()), (AT, None));

        let start = std::time::Instant::now();
        let game = FromSim::of(&sim);
        let taken = start.elapsed();

        // In the frame: made and written.
        let path = dir.join(format!("in_frame{EXTENSION}"));
        let start = std::time::Instant::now();
        let contents = of((game, &session), &text, None, interface()).expect("contents");

        write(&path, &contents).expect("written");

        let in_frame = start.elapsed();
        let size = std::fs::metadata(&path).map(|m| m.len());

        // Queued: the frame's part, then the save thread's.
        let game = FromSim::of(&sim);
        let mut writer = crate::save::thread::SaveThread::default();
        let start = std::time::Instant::now();

        game.ready().expect("a game to save");
        writer.send(crate::save::thread::Job {
            game,
            described: described(&session, &text),
            minimap: None,
            interface: interface(),
            name: "queued".to_owned(),
            profile: String::new(),
            place: crate::save::thread::Place::File(dir.join(format!("queued{EXTENSION}"))),
            kind: crate::save::thread::Kind::Asked,
        });

        let queued = start.elapsed();
        let done = writer.finish();
        let thread = start.elapsed();

        assert!(done.iter().all(|d| d.written.is_ok()));
        eprintln!(
            "save at frame {AT}: sim thread {taken:?}; frame stall: in the frame \
             {in_frame:?}, queued {queued:?}; save thread {thread:?}; {size:?} bytes"
        );
    }

    /// A custom game the bot plays, saved at frame `AT` to a file: its
    /// list reads the meta and minimap; the save's `play.input` replayed
    /// from the start reaches the snapshot's state; loaded as the menus
    /// load it, it plays on as the game never saved, and its recording,
    /// gone on from the save's session, replays to the loaded game's end.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_saved_game_replays_to_its_snapshot_and_loads_to_play_on() {
        const AT: u32 = 2_400;
        const MORE: u32 = 1_200;
        const BOT: u32 = 7;

        let dir = scratch("played");
        let catalogue = crate::testdata::hd();
        let text = Text::build(catalogue);
        let cli = crate::cli::tests::parsed(["openregent", "--map-seed", "3"]);
        let mut bot = Some(crate::game::bot::Bot::new(Menu::of(catalogue), BOT));
        let (mut sim, session) = begun(&cli, catalogue, &dir);

        played(&mut sim, &mut bot, AT);

        let path = dir.join(format!("bot{EXTENSION}"));
        let contents =
            of((FromSim::of(&sim), &session), &text, None, interface()).expect("contents");

        write(&path, &contents).expect("written");

        let saved = read(&path).expect("reads back");
        let listed = list(&dir);

        assert_eq!(saved, contents);
        assert_eq!(listed.len(), 1);
        assert_eq!(
            listed[0].glance.as_ref().ok().map(|g| &g.meta),
            Some(&contents.meta)
        );
        assert!(contents.play.lines().count() > 10, "the bot pressed");

        // The session replayed from its start.
        let session = record::parse(&saved.play).expect("session");
        let setup = session.setup.clone();
        let mut replayed = begun(&setup, catalogue, &dir).0;

        replayed.replay(session.events.clone());
        played(&mut replayed, &mut None, AT);

        assert!(
            replayed.snapshot().expect("snapshot") == saved.snapshot.game,
            "the save's play.input replays to its snapshot"
        );

        // Loaded as the menus load it, recording on.
        let install = Install::from_env().expect("MAJESTY_HD");
        let quests = crate::quests(&install).expect("quests");
        let (plan, session, _) = Plan::resumed(&install, &path, &quests).expect("loads");
        let mut loaded = plan.begin(catalogue).expect("loads").sim;

        loaded.record(Recorder::resume(
            &dir.join("loaded.input"),
            &dir.join("loaded.input.1"),
            &session,
        ));

        assert_eq!(loaded.frames(), AT);

        played(&mut sim, &mut None, MORE);
        played(&mut loaded, &mut None, MORE);

        let ended = loaded.snapshot().expect("snapshot");

        assert!(
            ended == sim.snapshot().expect("snapshot"),
            "a loaded game plays on as one never saved"
        );

        // The loaded game's recording, from the very start.
        let again = record::read(&dir.join("loaded.input")).expect("recorded");
        let mut from_start = begun(&again.setup, catalogue, &dir).0;

        from_start.replay(again.events.clone());
        played(&mut from_start, &mut None, AT + MORE);

        assert!(
            from_start.snapshot().expect("snapshot") == ended,
            "a loaded game's recording replays to it"
        );
    }

    /// A camp move a LAN guest held when the game was saved is still its
    /// own when the game loads with that guest local, not the crown's.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_loaded_camp_move_is_the_local_players() {
        const GUEST: u32 = 1;
        const RANGERS: regent_core::data::fourcc::FourCc =
            regent_core::data::fourcc::FourCc(*b"ABW1");
        const PRICE: i32 = 0;

        let dir = scratch("camp");
        let scene = dir.join("camp.toml");

        std::fs::write(
            &scene,
            "version = 1\n\
             map = { flat = { side = 128 } }\n\
             players = 2\n\
             [[unit]]\ncode = \"ABJ1\"\nat = [40, 64]\n\
             [[unit]]\ncode = \"ABJ1\"\nat = [88, 64]\nowner = 1\n\
             [[unit]]\ncode = \"ABW1\"\nat = [96, 64]\nowner = 1\n",
        )
        .expect("scene");

        let catalogue = crate::testdata::hd();
        let text = Text::build(catalogue);
        let cli =
            crate::cli::tests::parsed(["openregent", "--scene", scene.to_str().expect("utf-8")]);
        let (mut sim, session) = begun(&cli, catalogue, &dir);
        let guild = (0..10_000)
            .find(|id| {
                sim.world()
                    .piece(*id)
                    .is_some_and(|p| p.code == RANGERS && p.owner == GUEST)
            })
            .expect("the guest's guild");

        sim.set_local(GUEST).expect("a human seat");
        sim.camp_begin(guild, PRICE);
        played(&mut sim, &mut None, 2);

        let held = sim.world().camp_held(GUEST);

        assert!(held.is_some(), "the guest holds a camp move");

        let path = dir.join(format!("camp{EXTENSION}"));
        let contents =
            of((FromSim::of(&sim), &session), &text, None, interface()).expect("contents");

        write(&path, &contents).expect("written");

        let install = Install::from_env().expect("MAJESTY_HD");
        let quests = crate::quests(&install).expect("quests");
        let (plan, _, _) = Plan::resumed(&install, &path, &quests).expect("loads");
        let mut loaded = plan.begin(catalogue).expect("loads").sim;

        loaded.set_local(GUEST).expect("a human seat");

        assert_eq!(loaded.camp_move(), held);
    }

    /// A game with a mod saves the mod as needed: it loads where the mod
    /// is found, and is refused, the mod named, where it is not.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_save_needs_its_mods() {
        const GUARDHOUSE: &str = "{D666FCBE-9906-4582-A6A0-DC96F812441E}";

        let dir = scratch("modded");
        let scene =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/scenes/guardhouse.toml");
        let cli = crate::cli::tests::parsed([
            "openregent".to_owned(),
            "--scene".to_owned(),
            scene.display().to_string(),
            "--mods".to_owned(),
            GUARDHOUSE.to_owned(),
        ]);
        let (begun, catalogue) = crate::game::begin::test_setup(&cli);
        let (mut sim, session) = (begun.sim, begun.session);
        let text = Text::build(&catalogue);

        sim.record(Recorder::open(
            &dir.join("play.input"),
            &dir.join("play.input.1"),
            &cli.recorded(),
        ));
        played(&mut sim, &mut None, 20);

        let path = dir.join(format!("modded{EXTENSION}"));
        let contents =
            of((FromSim::of(&sim), &session), &text, None, interface()).expect("contents");

        write(&path, &contents).expect("written");

        let install = Install::from_env().expect("MAJESTY_HD");
        let quests = crate::quests(&install).expect("quests");
        let loaded = Plan::resumed(&install, &path, &quests).err();
        let bare = install.clone().with_mod_folders(Vec::new());
        let refused = Plan::resumed(&bare, &path, &quests)
            .err()
            .unwrap_or_default();

        assert_eq!(loaded, None, "loads where the mod is");
        assert!(refused.contains("Adjust Guardhouse"), "{refused}");
        assert!(refused.contains(GUARDHOUSE), "{refused}");
    }

    /// `scenes/damaged_house.toml` with `mods`, saved at frame `AT` and
    /// loaded: the house's hit points at the save and `MORE` frames on
    /// from the load, and in the game never saved.
    fn house_hp_saved(mods: &[&str]) -> (i32, i32, i32) {
        const AT: u32 = 100;
        const MORE: u32 = 400;

        let dir = scratch(&format!("house_mend{}", mods.len()));
        let scene =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/scenes/damaged_house.toml");
        let args = ["openregent".to_owned(), "--scene".to_owned()]
            .into_iter()
            .chain([scene.display().to_string()])
            .chain(mods.iter().map(|m| (*m).to_owned()));
        let cli = crate::cli::tests::parsed(args);
        let (begun, catalogue) = crate::game::begin::test_setup(&cli);
        let (mut sim, session) = (begun.sim, begun.session);
        let text = Text::build(&catalogue);

        sim.record(Recorder::open(
            &dir.join("play.input"),
            &dir.join("play.input.1"),
            &cli.recorded(),
        ));
        played(&mut sim, &mut None, AT);

        let path = dir.join(format!("house{EXTENSION}"));
        let contents =
            of((FromSim::of(&sim), &session), &text, None, interface()).expect("contents");

        write(&path, &contents).expect("written");

        let install = Install::from_env().expect("MAJESTY_HD");
        let quests = crate::quests(&install).expect("quests");
        let (plan, _, _) = Plan::resumed(&install, &path, &quests).expect("loads");
        let mut loaded = plan.begin(&catalogue).expect("loads").sim;

        // The scene's one house's hit points.
        let hp = |sim: &Sim| {
            let world = sim.world();

            (0..world.next_id())
                .filter_map(|id| world.piece(id))
                .find(|p| p.code.0 == *b"ABe1")
                .and_then(|p| p.attributes.get(&regent_core::data::attrib::HP.packed()))
                .copied()
                .expect("the house's hit points")
        };
        let at_save = hp(&loaded);

        played(&mut sim, &mut None, MORE);
        played(&mut loaded, &mut None, MORE);

        (at_save, hp(&loaded), hp(&sim))
    }

    /// A loaded house keeps native healing when the mod is selected.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_loaded_house_mends_with_the_builtin_mod() {
        const HOUSE_MEND: &str = "{A2A48A02-2BCE-42B5-8D2F-22B59DB3B720}";

        let (at_save, loaded, never_saved) = house_hp_saved(&["--mods", HOUSE_MEND]);

        assert!(
            loaded > at_save,
            "mends after the load: {at_save} -> {loaded}"
        );
        assert_eq!(loaded, never_saved);
        assert_eq!(house_hp_saved(&[]), (20, 20, 20));
    }

    /// The Magic Bazaar at 1 hit point shows its last damage stage,
    /// `Die-6` (set `0x65`), whose slot effects draw five fires. Saved and
    /// loaded, it shows that set as the game never saved does, and plays
    /// on alike: a save holds a piece's set, not its parts, which come
    /// from the art.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_loaded_damaged_bazaar_shows_its_fires() {
        const AT: u32 = 100;
        const MORE: u32 = 100;
        const BAZAAR: regent_core::data::fourcc::FourCc =
            regent_core::data::fourcc::FourCc(*b"ABl1");
        const LAST_STAGE: u16 = regent_core::sim::damage::DIE + 5;
        const SCENE: &str = r#"
            version = 1
            map = { flat = { side = 128 } }

            [[unit]]
            code = "ABJ1"
            at = [64, 64]

            [[unit]]
            code = "ABl1"
            at = [80, 64]
            attributes = { HP = 1 }
        "#;

        let dir = scratch("bazaar");
        let scene = dir.join("bazaar.toml");

        std::fs::write(&scene, SCENE).expect("the scene");

        let cli = crate::cli::tests::parsed([
            "openregent".to_owned(),
            "--scene".to_owned(),
            scene.display().to_string(),
        ]);
        let (begun, catalogue) = crate::game::begin::test_setup(&cli);
        let (mut sim, session) = (begun.sim, begun.session);
        let text = Text::build(&catalogue);

        sim.record(Recorder::open(
            &dir.join("play.input"),
            &dir.join("play.input.1"),
            &cli.recorded(),
        ));
        played(&mut sim, &mut None, AT);

        let path = dir.join(format!("bazaar{EXTENSION}"));
        let contents =
            of((FromSim::of(&sim), &session), &text, None, interface()).expect("contents");

        write(&path, &contents).expect("written");

        let install = Install::from_env().expect("MAJESTY_HD");
        let quests = crate::quests(&install).expect("quests");
        let (plan, _, _) = Plan::resumed(&install, &path, &quests).expect("loads");
        let mut loaded = plan.begin(&catalogue).expect("loads").sim;

        // The Bazaar's set, as drawn.
        let set = |sim: &Sim| {
            sim.world()
                .shown()
                .into_iter()
                .find(|s| s.code == BAZAAR)
                .and_then(|s| s.look)
                .map(regent_core::sim::movement::action::Look::set)
        };
        let bazaar = regent_core::content::unit::Unit::find(&catalogue, BAZAAR)
            .expect("the Bazaar")
            .expect("it resolves");
        let fires = bazaar
            .stack(LAST_STAGE)
            .next()
            .map(|s| s.poses[0].effects.len());

        let stage = u32::from(LAST_STAGE);

        assert_eq!(fires, Some(5));
        assert_eq!((set(&sim), set(&loaded)), (Some(stage), Some(stage)));

        played(&mut sim, &mut None, MORE);
        played(&mut loaded, &mut None, MORE);

        assert_eq!(set(&loaded), Some(stage));
        assert!(
            loaded.snapshot().expect("snapshot") == sim.snapshot().expect("snapshot"),
            "a loaded game plays on as one never saved"
        );
    }

    /// The mods a save needs differ from a game's: said, by name.
    #[test]
    fn a_save_of_other_mods_is_refused() {
        let needed = |names: &[&str]| Needs {
            archives: Vec::new(),
            images: String::new(),
            mods: names
                .iter()
                .map(|n| Needed {
                    guid: String::new(),
                    name: (*n).to_owned(),
                    image: None,
                    flags: None,
                })
                .collect(),
        };

        assert_eq!(mismatch(&needed(&["A"]), &needed(&["A"])), None);
        assert_eq!(
            mismatch(&needed(&["A", "B"]), &needed(&[])),
            Some("the save was made with mods A, B, this game loads none".to_owned())
        );
    }

    /// A mod compiled from other source than the save's: refused, named.
    #[test]
    fn a_save_of_other_mod_source_is_refused() {
        let needed = |image: &str| Needs {
            archives: Vec::new(),
            images: String::new(),
            mods: vec![Needed {
                guid: "G".to_owned(),
                name: "A".to_owned(),
                image: Some(image.to_owned()),
                flags: None,
            }],
        };

        assert_eq!(mismatch(&needed("1"), &needed("1")), None);
        assert_eq!(
            mismatch(&needed("1"), &needed("2")),
            Some("the save's mod A was compiled from other GPL source".to_owned())
        );
    }

    #[test]
    fn a_save_of_other_flag_patch_is_refused() {
        let needed = |flags: &str| Needs {
            archives: Vec::new(),
            images: String::new(),
            mods: vec![Needed {
                guid: "G".to_owned(),
                name: "A".to_owned(),
                image: None,
                flags: Some(flags.to_owned()),
            }],
        };

        assert_eq!(mismatch(&needed("1"), &needed("1")), None);
        assert_eq!(
            mismatch(&needed("1"), &needed("2")),
            Some("the save's mod A has different unit flags".to_owned())
        );
    }

    /// A save of another version is refused, by its meta.
    #[test]
    fn a_save_of_another_version_is_refused() {
        let dir = scratch("version");
        let path = dir.join(format!("old{EXTENSION}"));
        let meta = Meta {
            version: VERSION + 1,
            name: "old".to_owned(),
            game: SaveKind::Other {
                title: OTHER.to_owned(),
            },
            frame: 0,
            clock: 0,
            saved: 0,
            profile: String::new(),
            needs: Needs {
                archives: Vec::new(),
                images: String::new(),
                mods: Vec::new(),
            },
            lan: None,
        };
        let text = ron::to_string(&meta).expect("ron");
        let file = std::fs::File::create(&path).expect("file");
        let mut tar = tar::Builder::new(zstd::Encoder::new(file, LEVEL).expect("zstd"));
        let mut header = tar::Header::new_gnu();

        header.set_size(text.len() as u64);
        header.set_cksum();
        tar.append_data(&mut header, META, text.as_bytes())
            .expect("member");
        tar.into_inner().expect("tar").finish().expect("zstd");

        let refused = glance(&path).expect_err("refused");

        assert!(refused.contains("save format version"), "{refused}");
    }

    /// A peer's copy of a LAN save takes a name not taken.
    #[test]
    fn a_free_name_counts_past_those_taken() {
        let dir = scratch("free");

        assert_eq!(free(&dir, "Day").map(|(_, n)| n), Ok("Day".to_owned()));

        for taken in ["Day", "Day 2"] {
            std::fs::write(path(&dir, taken).expect("a path"), b"").expect("written");
        }

        let (at, name) = free(&dir, "Day").expect("free");

        assert_eq!(
            (at, name.as_str()),
            (path(&dir, "Day 3").expect("a path"), "Day 3")
        );
    }

    /// A name of the most characters, taken: its copy's number fits by
    /// the name cut short.
    #[test]
    fn a_free_name_of_the_most_characters_cuts_it_for_the_number() {
        let dir = scratch("free-longest");
        let longest = "d".repeat(NAME_CHARS);

        std::fs::write(path(&dir, &longest).expect("a path"), b"").expect("written");

        let (_, name) = free(&dir, &longest).expect("free");

        assert_eq!(name, format!("{} 2", "d".repeat(NAME_CHARS - 2)));
    }

    /// A name whose file name is past the file system's 255 bytes, though
    /// of few enough characters, is refused as too long.
    #[test]
    fn a_name_past_a_file_names_bytes_is_refused() {
        let dir = scratch("name-bytes");
        let wide = "\u{30a2}".repeat(NAME_CHARS);

        assert!(path(&dir, &wide).is_err());
        assert!(path(&dir, &"\u{30a2}".repeat(60)).is_ok());
    }

    /// A date from its seconds and the clock's lead on UTC.
    #[test]
    fn a_stamp_is_the_local_date_and_time_from_its_seconds() {
        assert_eq!(stamp_of(0, 0), (1970, 1, 1, 0, 0));
        assert_eq!(stamp_of(951_782_400, 0), (2000, 2, 29, 0, 0));
        assert_eq!(stamp_of(1_791_634_980, 0), (2026, 10, 10, 12, 23));
        assert_eq!(stamp_of(1_791_634_980, 2 * 3_600), (2026, 10, 10, 14, 23));
        assert_eq!(stamp_of(951_782_400, -60), (2000, 2, 28, 23, 59));
    }

    #[test]
    fn a_png_reads_back() {
        let png = png(None);
        let (size, rgba) = unpng(&png).expect("decodes");

        assert_eq!(size, [176, 176]);
        assert_eq!(&rgba[..4], &[0, 0, 0, 255]);
    }

    /// A 16-bit minimap, as an edited save may hold, reads as 8 bits a
    /// channel: egui's image asserts 4 bytes a pixel.
    #[test]
    fn a_16_bit_png_reads_as_8_bits() {
        let mut out = Vec::new();
        let mut encoder = png::Encoder::new(&mut out, 2, 2);

        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Sixteen);
        encoder
            .write_header()
            .and_then(|mut w| w.write_image_data(&[0xff; 2 * 2 * 8]))
            .expect("written");

        let (size, rgba) = unpng(&out).expect("decodes");

        assert_eq!((size, rgba.len()), ([2, 2], 2 * 2 * 4));
    }
}
