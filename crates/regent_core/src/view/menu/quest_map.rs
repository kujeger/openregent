//! HD's quest map: the painted map of Ardania the player picks a quest on.
//!
//! One `SMNU` panel, `APdb`, held only by HD's `Data/UIData_<w>_<h>.dat`
//! (`docs/formats/panels.md`, the quest map). The panel's own picture is the map,
//! `IX10` set 1001, 1398×1611, one `TILE` in `mx_interfacedata.cam`: base
//! and expansion quests share it, their buildings painted in.
//!
//! ```text
//!  screen ┌────────────── frame: 23 controls, fixed on screen ──────────────┐
//!         │ pog 8730   title 8885                                          │
//!         │            line 5001: Difficulty …  Requires …                 │
//!         │  ┌── panel APdb, scrolls under the frame ──────────────────┐   │
//!         │  │ map IX10 1001                                           │   │
//!         │  │   hit 4000+s  100×100, the hover and click region       │   │
//!         │  │   lit 4100+s  IX33, shown while 4000+s is hovered       │   │
//!         │  │   star 4400+s INQd 1001, shown once won                 │   │
//!         │  │   cloud 7400+s INQc 1001, shown while locked            │   │
//!         │  └─────────────────────────────────────────────────────────┘   │
//!         │ erase 5002        compass 5000 (freestyle)       main menu 8003 │
//!         └────────────────────────────────────────────────────────────────┘
//!  behind: INse 1023, the sepia faces, tiled over the whole screen
//! ```
//!
//! `s` is a slot of [`SLOTS`]. **Binary** unless marked.

use std::collections::BTreeMap;
use std::path::Path;

use crate::content::quest;
use crate::data::cam::Archive;
use crate::data::catalogue::Catalogue;
use crate::data::deps::{self, Difficulty};
use crate::data::error::Result as DataResult;
use crate::data::fourcc::FourCc;
use crate::data::guid::Guid;
use crate::data::smnu::{self, Control, Panel, Rect};
use crate::data::text::printf;
use crate::view::menu::film::Film;
use crate::view::menu::{self, Release, World};
use crate::view::panel::day::Day;
use crate::view::sprite::Sprite;

/// The panel.
pub const PANEL: FourCc = FourCc(*b"APdb");

/// The quest each slot holds: game globals, written by its
/// constructor. A slot's controls are [`HIT`] + slot and the
/// offsets below; quest slot of maps a control back, 4000–4033.
///
/// Wrath of Krolm (`MD01`) and Vampiric Revenge (`VAMP`) hold no slot: the
/// map does not show them. **Inferred**: the download button ([`DOWNLOAD`])
/// reaches them.
pub const SLOTS: [FourCc; 34] = [
    FourCc(*b"QU01"),
    FourCc(*b"QU02"),
    FourCc(*b"QU03"),
    FourCc(*b"QU04"),
    FourCc(*b"QU05"),
    FourCc(*b"QU06"),
    FourCc(*b"QU07"),
    FourCc(*b"QU08"),
    FourCc(*b"QU09"),
    FourCc(*b"QU10"),
    FourCc(*b"QU11"),
    FourCc(*b"QU12"),
    FourCc(*b"QU13"),
    FourCc(*b"QU14"),
    FourCc(*b"QU15"),
    FourCc(*b"QU16"),
    FourCc(*b"QU17"),
    FourCc(*b"QU18"),
    FourCc(*b"QU19"),
    FourCc(*b"XQ01"),
    FourCc(*b"XQ02"),
    FourCc(*b"XQ03"),
    FourCc(*b"XQ04"),
    FourCc(*b"XQ05"),
    FourCc(*b"XQ06"),
    FourCc(*b"XQ07"),
    FourCc(*b"XQ08"),
    FourCc(*b"XQ09"),
    FourCc(*b"XQ10"),
    FourCc(*b"XQ11"),
    FourCc(*b"XQ12"),
    FourCc(*b"XQD1"),
    FourCc(*b"XQD2"),
    FourCc(*b"XQD3"),
];

/// The base game's slots: the first 19.
pub(crate) const BASE_SLOTS: usize = 19;

/// The expansion's last quest slot, `XQ12`: the ending's test runs over
/// slots 19 to it.
pub(crate) const LAST_EXPANSION_SLOT: usize = 30;

/// The first downloadable slot, `XQD1`.
#[cfg(test)]
const DOWNLOAD_SLOTS: usize = 31;

/// A slot's hit region: 100×100, no picture. The hover handler
/// and the click both
/// take this control. **Inferred**: a hit is the rectangle, there being no
/// picture to test pixels of.
pub const HIT: i32 = 4000;

/// A slot's lit picture, [`HIT`] + 100: shown while the hit region is
/// hovered, slot 1 of its set (slot 0 holds a wooden compass no capture
/// shows; the unlit look is the map).
pub const LIT: i32 = HIT + 100;

/// A slot's won star, [`HIT`] + 400: shown when the quest has a victory.
pub const STAR: i32 = HIT + 400;

/// A slot's "?" cloud, [`HIT`] + 3400: shown while its `QDEP` rule is not
/// met. Seven slots have one.
pub const CLOUD: i32 = HIT + 3400;

/// The round quest icon at the title bar's left.
pub use super::ids::POG;

/// The title, `0x22b5`: the quest's name, or [`SECRET_TEXT`] while locked.
pub const TITLE: i32 = 8885;

/// The line under the title, `0x1389`: difficulty, requirements.
pub const LINE: i32 = 5001;

/// The compass: the freestyle dialog, `AP63`. 5900 does the same.
pub const FREESTYLE: i32 = 5000;

/// Erase Victories: asks (`AP11`, `GMTX` 325, 326), then clears.
pub const ERASE: i32 = 5002;

/// Erase Victories' question's title, `Erase Victories`: `GMTX` 325, on
/// `AP11` control `0x1f45`.
pub const ERASE_TITLE_TEXT: usize = 325;

/// Erase Victories' question, `Are you sure you want to erase all of your
/// victories?`: `GMTX` 326, `AP11` control 5000. Yes (`0x157d`) erases.
pub const ERASE_TEXT: usize = 326;

/// Main Menu: back to `AP29`.
pub const MAIN_MENU: i32 = 8003;

/// Load Downloadable Quest: opens `APd8`; drawn as [`JEWEL`].
pub const DOWNLOAD: i32 = 4034;

/// The FREESTYLE QUESTS text under the compass, no picture; it shares
/// the compass's hover callback.
pub const FREESTYLE_TEXT: i32 = 5900;

/// [`DOWNLOAD`]'s picture, set by quest map setup over the panel's
/// `IX34`: seven slots of one frame, slot 6 the lit jewel.
pub const JEWEL: Art = Art {
    record: FourCc(*b"DX00"),
    set: 1001,
};

/// What a hovered control puts in the title bar: a pog and a `GMTX` title.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Banner {
    pub pog: Art,
    pub title: usize,
}

/// The jewel's: `DX00` 1000, `GMTX` 387 "Downloadable Quests".
pub const DOWNLOAD_BANNER: Banner = Banner {
    pog: Art {
        record: FourCc(*b"DX00"),
        set: 1000,
    },
    title: 387,
};

/// The compass's and its text's: `INTD` 1036, `GMTX` 80 "Freestyle Game".
pub const FREESTYLE_BANNER: Banner = Banner {
    pog: Art {
        record: FourCc(*b"INTD"),
        set: 1036,
    },
    title: 80,
};

/// The title bar `hovered` fills, if it is the jewel, compass or text.
#[must_use]
pub const fn banner(hovered: i32) -> Option<Banner> {
    match hovered {
        DOWNLOAD => Some(DOWNLOAD_BANNER),
        FREESTYLE | FREESTYLE_TEXT => Some(FREESTYLE_BANNER),
        _ => None,
    }
}

/// Whether `control` draws as hovered while `hovered` is under the
/// pointer: itself, or the compass while its text
/// is (freestyle hover's message `0x59`). Hovering one starts its
/// drawn slot's playback: the compass,
/// `INPq` 1039, plays slot 6's nine frames, looping at 150 ms.
#[must_use]
pub fn hover(control: i32, hovered: Option<i32>) -> menu::Hover {
    let over =
        hovered == Some(control) || (control == FREESTYLE && hovered == Some(FREESTYLE_TEXT));

    if over {
        menu::Hover::Over
    } else {
        menu::Hover::Away
    }
}

/// The controls that stay put while the panel scrolls, quest map fixed.
/// The quest map screen moves each back by the scroll. 110 is in no
/// shipped `APdb`.
pub const FIXED: [i32; 23] = [
    110, 666, 8885, 5001, 8730, 5002, 5000, 5900, 8003, 8500, 8501, 8502, 8503, 8504, 8507, 8508,
    8505, 8509, 8510, 8511, 8506, 8512, 8513,
];

/// The frame's bottom right corner piece. With control 110 absent its far
/// corner is the frame's size: 1398×1200 at 1920×1200, the screen at
/// 1280×768.
const FRAME_CORNER: i32 = 8504;

/// The background behind every front-end panel: `INse` set 1023, the sepia
/// faces, 770×550 in `addinterface_se.cam`, tiled over the whole screen
/// (`BackgroundFillController`, flag `0x40000`).
pub const BACKGROUND: Art = Art {
    record: FourCc(*b"INse"),
    set: 1023,
};

/// `SMNU` key 10's tiling bit ([`smnu::style::TILED`]): the background's,
/// and three frame bars narrower than their rectangles.
pub const TILED: i32 = smnu::style::TILED;

/// `SMNU` key 10.
const FLAGS: i32 = 10;

/// A cursor this close to the frame's edge scrolls the map, by
/// scroll speed with scroll ramp, as in play.
pub const EDGE: i32 = 8;

/// `Difficulty: %s`, coloured: `GMTX` 136, an unlocked quest not yet won.
pub const DIFFICULTY_TEXT: usize = 136;

/// `Difficulty: %s  Victories: %d  Best Time: %s`: `GMTX` 137, a won
/// quest; the time is day text's.
pub const WON_TEXT: usize = 137;

/// `Difficulty: %s  Requires: %s`: `GMTX` 138, a locked quest.
pub const LOCKED_TEXT: usize = 138;

/// `Secret Quest`: `GMTX` 268, a locked quest's title.
pub const SECRET_TEXT: usize = 268;

/// `All Southern Quests`: `GMTX` 142, the Day of Reckoning's requirements
/// while locked, in place of the list (quest map hover, slot 5 at
/// quest map hover).
pub const SOUTHERN_TEXT: usize = 142;

/// The slot whose requirements read [`SOUTHERN_TEXT`]: `QU06`.
pub const SOUTHERN_SLOT: usize = 5;

/// The "All Southern Quests" of [`SOUTHERN_TEXT`]: what `QU06`'s `QDEP`
/// rule names, every base slot but its own.
pub const SOUTHERN: [FourCc; 18] = [
    FourCc(*b"QU01"),
    FourCc(*b"QU02"),
    FourCc(*b"QU03"),
    FourCc(*b"QU04"),
    FourCc(*b"QU05"),
    FourCc(*b"QU07"),
    FourCc(*b"QU08"),
    FourCc(*b"QU09"),
    FourCc(*b"QU10"),
    FourCc(*b"QU11"),
    FourCc(*b"QU12"),
    FourCc(*b"QU13"),
    FourCc(*b"QU14"),
    FourCc(*b"QU15"),
    FourCc(*b"QU16"),
    FourCc(*b"QU17"),
    FourCc(*b"QU18"),
    FourCc(*b"QU19"),
];

/// A won requirement, grey `656565`: `GMTX` 323.
pub const MET_TEXT: usize = 323;

/// A requirement not yet won, `E6E1FD`: `GMTX` 324.
pub const UNMET_TEXT: usize = 324;

/// A picture: a record and one of its `IMAG` sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Art {
    pub record: FourCc,
    pub set: i32,
}

/// A picture at a place on the panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placed {
    /// `None` where the panel names no picture, or a set its record lacks.
    pub art: Option<Art>,
    pub rect: Rect,
    /// `SMNU` key 10's words, or-ed.
    pub flags: i32,
}

impl Placed {
    fn of(control: &Control<'_>) -> Option<Placed> {
        let art = control
            .record()
            .zip(control.set())
            .map(|(record, set)| Art { record, set });

        control.rect().map(|rect| Placed {
            art,
            rect,
            flags: control.words(FLAGS).iter().fold(0, |a, w| a | w),
        })
    }

    /// Whether `TILED` is set.
    #[must_use]
    pub const fn tiled(&self) -> bool {
        self.flags & TILED != 0
    }
}

/// One quest's place on the map, in panel pixels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Site {
    pub slot: usize,
    pub quest: FourCc,
    pub hit: Placed,
    /// The 31 painted quests' `IX33`; the downloadable ones' `DX31`–`DX33`,
    /// 160×160, a picture of their own (slot 0 drawn, slot 1 lit).
    pub lit: Option<Placed>,
    pub star: Option<Placed>,
    pub cloud: Option<Placed>,
}

/// `APdb` from one HD `UIData_<w>_<h>.dat`.
///
/// # Errors
///
/// If the file will not read or parse, or holds no `APdb`.
pub fn load(path: &Path) -> Result<Panel, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let archive = Archive::parse(&bytes).map_err(|e| e.to_string())?;
    let found = archive
        .records()
        .find(|(chunk, record)| chunk.tag.as_bytes() == b"SMNU" && record.id == PANEL)
        .map(|(_, record)| record.data)
        .ok_or_else(|| format!("{}: no SMNU APdb", path.display()))?;

    Panel::parse(found).map_err(|e| e.to_string())
}

/// The map: the panel's own rectangle and picture.
#[must_use]
pub fn map(panel: &Panel) -> Option<Placed> {
    panel.controls().first().and_then(Placed::of)
}

/// The control with id `id`.
fn control(panel: &Panel, id: i32) -> Option<Placed> {
    panel
        .controls()
        .iter()
        .find(|c| c.id() == Some(id))
        .and_then(Placed::of)
}

/// Every slot's site, in slot order; a slot with no hit region is left out.
#[must_use]
pub fn sites(panel: &Panel) -> Vec<Site> {
    let controls = panel.controls();
    let find = |id: i32| {
        controls
            .iter()
            .find(|c| c.id() == Some(id))
            .and_then(Placed::of)
    };

    SLOTS
        .iter()
        .enumerate()
        .filter_map(|(slot, &quest)| {
            let offset = i32::try_from(slot).ok()?;

            Some(Site {
                slot,
                quest,
                hit: find(HIT + offset)?,
                lit: find(LIT + offset),
                star: find(STAR + offset),
                cloud: find(CLOUD + offset),
            })
        })
        .collect()
}

/// The frame's pieces, [`FIXED`] order, each the panel holds.
#[must_use]
pub fn frame(panel: &Panel) -> Vec<(i32, Placed)> {
    FIXED
        .iter()
        .filter_map(|&id| control(panel, id).map(|p| (id, p)))
        .collect()
}

/// Control `id`'s text rectangle, from its corner (`smnu::RECT_TOO`):
/// [`FREESTYLE_TEXT`]'s, (4, 42) 168×20, the dark slot under the compass.
#[must_use]
pub fn text_rect(panel: &Panel, id: i32) -> Option<Rect> {
    panel
        .controls()
        .iter()
        .find(|c| c.id() == Some(id))
        .and_then(Control::text_rect)
}

/// The frame's width and height: `FRAME_CORNER`'s far corner.
#[must_use]
pub fn frame_size(panel: &Panel) -> Option<(i32, i32)> {
    let [.., x1, y1] = control(panel, FRAME_CORNER)?.rect.corners();

    Some((x1, y1))
}

/// Where a frame piece's edge holds when the frame changes size: as far
/// from the frame's start, its middle, or its end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    Start,
    Middle,
    End,
}

/// A [`FIXED`] piece's edges' holds: left and right, top and bottom.
/// A piece held at two places stretches; the bars repeat over it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Anchor {
    pub x: (Edge, Edge),
    pub y: (Edge, Edge),
}

/// [`FIXED`] control `id`'s [`Anchor`]; an id not on the frame holds at
/// the top left.
///
/// ```text
///  8500 ┌─ 8501 top bar, stretched ──────────────────────┐ 8502
///  8509 │ 8730 pog, 666   8885 title, 5001 line ──────▶  │ 8503
///  side │                                                │ side
///  8510 ┤ middle                                  middle ├ 8511
///       │      8512 5002   8506 5000 5900  8513 8003     │
///  8508 └─ 8505 to the middle ─┘ centre └─ 8507 to end ──┘ 8504
/// ```
///
/// **Corpus**: every shipped `UIData` frame, 1280 and 1398 wide, 768 to
/// 1200 high, is the 1920×1200 one so moved ([`moved`]), within 1 px on a
/// middle edge. **Inferred**: the title and line hold their right edge at
/// the end; the files' right gaps differ (31 and 23 px, 32 and 22).
#[must_use]
pub const fn anchor(id: i32) -> Anchor {
    use Edge::{End, Middle, Start};

    let (x, y) = match id {
        TITLE | LINE => ((Start, End), (Start, Start)),
        ERASE | FREESTYLE | FREESTYLE_TEXT | MAIN_MENU | 8506 | 8512 | 8513 => {
            ((Middle, Middle), (End, End))
        }
        8501 => ((Start, End), (Start, Start)),
        8502 => ((End, End), (Start, Start)),
        8503 => ((End, End), (Start, End)),
        FRAME_CORNER => ((End, End), (End, End)),
        8505 => ((Start, Middle), (End, End)),
        8507 => ((Middle, End), (End, End)),
        8508 => ((Start, Start), (End, End)),
        8509 => ((Start, Start), (Start, End)),
        8510 => ((Start, Start), (Middle, Middle)),
        8511 => ((End, End), (Middle, Middle)),
        _ => ((Start, Start), (Start, Start)),
    };

    Anchor { x, y }
}

/// Control `id`'s `rect` on a `from`-sized frame, on a `to`-sized one,
/// each edge held by [`anchor`].
#[must_use]
pub const fn moved(id: i32, rect: Rect, from: (i32, i32), to: (i32, i32)) -> Rect {
    let Anchor { x, y } = anchor(id);
    let left = held(rect.x, x.0, from.0, to.0);
    let right = held(rect.x + rect.width, x.1, from.0, to.0);
    let top = held(rect.y, y.0, from.1, to.1);
    let bottom = held(rect.y + rect.height, y.1, from.1, to.1);

    Rect {
        x: left,
        y: top,
        width: right - left,
        height: bottom - top,
    }
}

/// The narrowest frame [`moved`] lays out whole: the bottom bar's middle
/// pieces (8512 to 8513, held at the middle) still clear its corners
/// (8508, 8504). One less and 8513 overlaps 8504. Narrower than any shipped
/// frame (1280); openregent's, not the original's.
pub const LEAST_WIDTH: i32 = 891;

/// Position `at` on a `from`-long axis, on a `to`-long one, held by `edge`.
const fn held(at: i32, edge: Edge, from: i32, to: i32) -> i32 {
    match edge {
        Edge::Start => at,
        Edge::Middle => at - from / 2 + to / 2,
        Edge::End => at - from + to,
    }
}

/// Where the frame and the map stand when the screen opens, on a
/// `screen`-sized display: the frame centred, the map centred under it,
/// never past its own edge. Returns the frame's
/// top left and the map's, in screen pixels.
///
/// Scrolling moves the map by `(dx, dy)` within `frame − map ≤ d ≤ 0` per
/// axis, measured from the frame's top left.
#[must_use]
pub fn origin(panel: &Panel, screen: (i32, i32)) -> Option<((i32, i32), (i32, i32))> {
    let (fw, fh) = frame_size(panel)?;
    let full = map(panel)?.rect;
    let frame_at = ((screen.0 - fw) / 2, (screen.1 - fh) / 2);
    let inset = |map: i32, frame: i32| (-((map - frame) / 2)).min(0);

    Some((
        frame_at,
        (
            frame_at.0 + inset(full.width, fw),
            frame_at.1 + inset(full.height, fh),
        ),
    ))
}

/// Frame `frame` of slot `slot` of `art`: slot 0 a picture as drawn, 1
/// lit, for buttons. `None` where the record, set, slot or frame is not
/// there, or the frame is no tile.
#[must_use]
pub fn picture(
    catalogue: &Catalogue<'_>,
    art: Art,
    slot: usize,
    frame: usize,
) -> Option<DataResult<Sprite>> {
    Sprite::in_set(
        catalogue,
        art.record,
        u32::try_from(art.set).ok()?,
        (slot, frame),
    )
}

/// The slot `quest` holds.
#[must_use]
pub fn slot(quest: FourCc) -> Option<usize> {
    SLOTS.iter().position(|&q| q == quest)
}

/// A slot's round title-bar icon (quest pog record,
/// quest pog set): a base-dataset quest `INTD` set 1000 + slot, an
/// expansion one `IX11` set 1000 + slot − 19, a downloadable one
/// `IXD1`–`IXD3` set 1000. **Inferred**: the binary tells base from
/// expansion by the quest's dataset; within [`SLOTS`] that is the slot.
///
/// With nothing hovered the pog shows `INTD` set 1037, which `INTD` lacks:
/// nothing.
#[must_use]
pub fn pog(slot: usize) -> Option<Art> {
    let release = if slot < BASE_SLOTS {
        Release::Base
    } else {
        Release::Expansion
    };
    let (record, set) = menu::icon(*SLOTS.get(slot)?, release, World::Quest)?;

    Some(Art {
        record,
        set: i32::from(set),
    })
}

/// A slot's video, `QM` and the slot in two digits:
/// a `PICT` in `cinedata1.cam` (`QM00`–`QM18`) or `mx_cinedata1.cam`
/// (`QM19`–`QM30`). No archive holds `QM31`–`QM33`.
#[must_use]
pub fn video(slot: usize) -> Option<FourCc> {
    (slot < SLOTS.len()).then(|| {
        let digit = |n: usize| b'0' + u8::try_from(n % 10).unwrap_or(0);

        FourCc([b'Q', b'M', digit(slot / 10), digit(slot)])
    })
}

/// The `GMTX` text of a difficulty: Beginner,
/// Advanced, Expert, Master. 419 is the expansion's; the base table has
/// none. **Game**: HD shows Urban Renewal (`MEDM`) "Advanced" and Tomb of
/// the Dragon King (`HARD`) "Expert".
#[must_use]
pub const fn difficulty_text(difficulty: Difficulty) -> usize {
    match difficulty {
        Difficulty::Easy => 139,
        Difficulty::Medium => 140,
        Difficulty::Hard => 141,
        Difficulty::Master => 419,
    }
}

/// One quest's victories, as quest victory record
/// keeps them in the map at game globals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Record {
    /// Games won: entry.
    pub victories: u32,
    /// The shortest won game, game ms at its end: entry,
    /// shown by day text. **Inferred**:
    /// the level's timer (`game_root`) is the game clock.
    pub best_ms: u32,
}

/// The seen-once bytes `questdata_majx.sav` keeps,
/// each set once and cleared only by Erase
/// Victories.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools, reason = "one flag a seen-once byte")]
pub struct Seen {
    /// The map advisor's first voice was played: game globals.
    pub advisor: bool,
    /// [`EXPERT_HINT_TEXT`] was shown:.
    pub expert_hint: bool,
    /// The expansion's ending was asked (expansion ending seen):
    /// set by an end panel's press ([`Progress::ending_due`]),
    /// read by CINEMATIC, which then plays `MV07`
    /// (`menu::CINEMATIC`).
    pub ending: bool,
}

impl Seen {
    /// `film` marked seen; the opening has no mark.
    pub const fn mark(&mut self, film: Film) {
        match film {
            Film::Opening => {}
            Film::North => self.ending = true,
        }
    }
}

/// Every quest's [`Record`], by code: what the original keeps in
/// `questdata_majx.sav`, one file for the
/// install, not per player name. Krolm (`MD01`) and Vampiric Revenge
/// (`VAMP`) record too: any quest world not freestyle does.
/// The map shows only [`SLOTS`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Progress {
    records: BTreeMap<FourCc, Record>,
    /// The whole id of each code that is a CRC of one ([`quest::key_of`]):
    /// a mod's quest, which a profile names by its id.
    ids: BTreeMap<FourCc, Guid>,
    /// The quest won last:, saved.
    pub last_won: Option<FourCc>,
    pub seen: Seen,
    /// The hint was offered this session:, not saved.
    pub hinted: bool,
}

impl Progress {
    /// From records, a later one of a code replacing an earlier.
    pub fn of(records: impl IntoIterator<Item = (FourCc, Record)>) -> Progress {
        Progress {
            records: records.into_iter().collect(),
            ..Progress::default()
        }
    }

    /// Every record, by code.
    pub fn records(&self) -> impl Iterator<Item = (FourCc, Record)> + '_ {
        self.records.iter().map(|(&q, &r)| (q, r))
    }

    /// `quest`'s record; `None` never won.
    #[must_use]
    pub fn record(&self, quest: FourCc) -> Option<Record> {
        self.records.get(&quest).copied()
    }

    /// `quest`'s victories, 0 never won.
    #[must_use]
    pub fn victories(&self, quest: FourCc) -> u32 {
        self.record(quest).map_or(0, |r| r.victories)
    }

    /// Whether `quest` has a victory: its star shows, and it meets a rule
    /// naming it.
    #[must_use]
    pub fn won(&self, quest: FourCc) -> bool {
        self.victories(quest) > 0
    }

    /// `quest` won at `ms` of game time: one
    /// more victory; the best time `ms` on the first, or when strictly
    /// shorter; `quest` the last won.
    pub fn win(&mut self, quest: FourCc, ms: u32) {
        let record = self.records.entry(quest).or_insert(Record {
            victories: 0,
            best_ms: ms,
        });

        record.victories = record.victories.wrapping_add(1);
        record.best_ms = record.best_ms.min(ms);
        self.last_won = Some(quest);
    }

    /// The quest of id `id` won at `ms`, as [`Progress::win`] its code
    /// ([`quest::key_of`]): HD keeps victories by code, so a mod's quest
    /// keyed by its id's CRC has its own record, and one reusing a
    /// shipped id (`NewGame+`'s Legendary Heroes, `XQ04`) shares the
    /// shipped quest's.
    pub fn win_quest(&mut self, id: Guid, ms: u32) {
        let code = self.known(id);

        self.win(code, ms);
    }

    /// `id`'s code, its id remembered where the code is a CRC of it.
    pub fn known(&mut self, id: Guid) -> FourCc {
        let code = quest::key_of(id);

        if !quest::by_first_word(id) {
            self.ids.insert(code, id);
        }

        code
    }

    /// The id a code was made from, where it is a CRC of one.
    #[must_use]
    pub fn id(&self, code: FourCc) -> Option<Guid> {
        self.ids.get(&code).copied()
    }

    /// Erase Victories: every record, the last
    /// won and the seen-once bytes gone, the session's byte too.
    pub fn erase(&mut self) {
        *self = Progress::default();
    }

    /// Whether every expansion slot, 19 to 30, has a victory.
    #[must_use]
    pub fn expansion_won(&self) -> bool {
        SLOTS[BASE_SLOTS..=LAST_EXPANSION_SLOT]
            .iter()
            .all(|&quest| self.won(quest))
    }

    /// Whether an end panel's press asks the expansion's ending first:
    /// every expansion slot won
    /// and the ending not yet seen. Per profile here; the original keeps
    /// one for the install. Erase Victories clears it again.
    #[must_use]
    pub fn ending_due(&self) -> bool {
        !self.seen.ending && self.expansion_won()
    }

    /// The films the Cinematics page lists, by victories, not by what
    /// has been watched, so Erase Victories locks them again.
    #[must_use]
    pub fn films(&self) -> Vec<Film> {
        Film::ALL
            .into_iter()
            .filter(|film| match film {
                Film::Opening => true,
                Film::North => self.expansion_won(),
            })
            .collect()
    }

    /// The sum of every quest's victories (quest victories total,
    /// quest victories total).
    #[must_use]
    pub fn total(&self) -> u32 {
        self.records
            .values()
            .fold(0, |n, r| n.wrapping_add(r.victories))
    }

    /// The map opens: its voice,
    /// hint and lifting cloud, in the original's order, each marking what
    /// it showed. The caller saves [`Progress::seen`] and
    /// [`Progress::last_won`] if they changed.
    ///
    /// ```text
    ///  advisor not heard          RA28; heard from now on
    ///  else 3 victories, expert   RA30
    ///       hint unseen
    ///  once a session             a hint by victories
    ///  a cloud lifts              never in HD: lifted
    /// ```
    ///
    /// The voice's test reads the expert hint before the hint marks it, so
    /// RA30 comes with [`EXPERT_HINT_TEXT`], unless the session's hint
    /// was shown before the third victory: then nothing marks it, and RA30
    /// plays at every opening until a restart, as in the original.
    pub fn open_map(&mut self, rules: &[deps::Quest]) -> Opening {
        let voice = if !self.seen.advisor {
            self.seen.advisor = true;
            Some(INTRODUCTION)
        } else if self.total() == EXPERT_AT && !self.seen.expert_hint {
            Some(EXPANSION)
        } else {
            None
        };
        let hint = self.hint();
        let lift = lifted(&LIFTS, self, rules);

        if lift.is_some() {
            self.last_won = None;
        }

        Opening {
            voice,
            hint,
            lift: lift.map(|word| word + (CLOUD - HIT)),
        }
    }

    /// The map's hint: once a
    /// session, by [`Progress::total`]: [`HINTS`] 0 to 2 victories, 3
    /// [`EXPERT_HINT_TEXT`] once ever; more, none.
    fn hint(&mut self) -> Option<usize> {
        let first = !self.hinted;

        self.hinted = true;

        match (first, self.total()) {
            (true, n @ 0..EXPERT_AT) => HINTS.get(usize::try_from(n).ok()?).copied(),
            (true, EXPERT_AT) if !self.seen.expert_hint => {
                self.seen.expert_hint = true;
                Some(EXPERT_HINT_TEXT)
            }
            _ => None,
        }
    }
}

/// What the map does as it opens ([`Progress::open_map`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Opening {
    /// The advisor's `DSND`, phase [`ADVISOR_PHASE`], unplaced; stopped
    /// as the map closes.
    pub voice: Option<FourCc>,
    /// The hint's `GMTX` text, in `AP11` titled [`HINT_TITLE_TEXT`].
    pub hint: Option<usize>,
    /// The cloud control whose set `0x60` plays: never in HD ([`lifted`]).
    pub lift: Option<i32>,
}

/// The map advisor's first voice, `RA28` `Advisor_Introduction`
/// (`sounddesc.cam`), while hd flag is set: always in HD. Clear,
/// the expansion alone plays [`EXPANSION`] instead.
pub const INTRODUCTION: FourCc = FourCc(*b"RA28");

/// The advisor with the expert hint, `RA30` `Advisor_Expansion`
/// (`mx_sounddesc.cam`).
pub const EXPANSION: FourCc = FourCc(*b"RA30");

/// The phase both advisors play: `GVS5` (`0x35535647`).
pub const ADVISOR_PHASE: FourCc = FourCc(*b"GVS5");

/// The hint's title, `Select A Quest`: `GMTX` 147, on `AP11` control
/// `0x1f45`; the hint on control 5000; Yes and No hidden, so OK (5503,
/// key Enter, "Dismiss this message.") closes it.
pub const HINT_TITLE_TEXT: usize = 147;

/// The hints by victories: none, one, two, three.
pub const HINTS: [usize; 4] = [143, 144, 145, EXPERT_HINT_TEXT];

/// The expert hint, shown once ever: `GMTX` 146, "Try an expert quest…".
pub const EXPERT_HINT_TEXT: usize = 146;

/// The victories at which the expert hint and [`EXPANSION`] come.
const EXPERT_AT: u32 = 3;

/// The words quest map setup tests for a lifting cloud, at
/// quest lift words: control ids of slots 5, 8–10, 29–31, where
/// quest lift paired wants quest codes.
pub const LIFTS: [u32; 7] = [4005, 4008, 4009, 4010, 4029, 4030, 4031];

/// The cloud that lifts as the map opens (quest map setup,
/// quest map last won): the last of `words` that quest lift paired
/// pairs with the last won quest, that is open, while the last won quest
/// has exactly one victory. The screen then plays its word + 3400's set
/// `0x60` and forgets the last won quest ( to `0x22`).
///
/// Quest lift paired pairs a word that is a slot's quest code with a last won
/// slot quest its `QDEP` names. [`LIFTS`] holds control ids, no quest
/// code, so in HD no cloud ever lifts. **Inferred**: nothing rewrites it (one reader, no writer).
#[must_use]
pub fn lifted(words: &[u32], progress: &Progress, rules: &[deps::Quest]) -> Option<i32> {
    let last = progress.last_won?;
    let paired = |quest: FourCc| {
        slot(quest).is_some()
            && slot(last).is_some()
            && rules
                .iter()
                .find(|r| r.id == quest)
                .and_then(|r| r.requires.as_ref())
                .is_some_and(|e| e.ids().contains(&last))
    };

    words
        .iter()
        .rev()
        .find(|&&word| {
            let quest = FourCc::from_word(word);

            paired(quest)
                && lock(quest, rules, progress) == Lock::Open
                && progress.victories(last) == 1
        })
        .and_then(|&word| i32::try_from(word).ok())
}

/// Whether a quest can be played.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lock {
    Open,
    /// Its rule is unmet: cloud shown, hit region disabled, title bar
    /// [`SECRET_TEXT`] and [`LOCKED_TEXT`].
    Locked,
}

/// `quest`'s lock: a slot quest's
/// `QDEP` rule over `progress`, an id met once won. Qdep quest value
/// reads no `NOT` context, so [`deps::Expr::evaluate`] is the engine's
/// here. A quest outside [`SLOTS`], or with no rule or `NONE`: open.
#[must_use]
pub fn lock(quest: FourCc, rules: &[deps::Quest], progress: &Progress) -> Lock {
    let rule = slot(quest).and_then(|_| rules.iter().find(|r| r.id == quest));
    let met = rule
        .and_then(|r| r.requires.as_ref())
        .is_none_or(|e| e.evaluate(&|id| progress.won(id)));

    if met { Lock::Open } else { Lock::Locked }
}

/// A site's own pictures over the map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    /// [`LIT`] + s, slot 1: the quest hovered.
    Lit,
    /// [`CLOUD`] + s: locked.
    Cloud,
    /// [`STAR`] + s: won.
    Star,
}

/// What a site draws, bottom first.
///
/// **Binary**: quest map hover plays the lit picture and touches no
/// cloud; `APdb` lists every [`LIT`] + s before its [`STAR`] + s and
/// [`CLOUD`] + s, and draws in list order. So a hovered, locked quest
/// shows lit under its cloud. **Game**: `ui_menu_map_hover_quest.png`
/// (Vigil), `menu_map_bottom.png` (the Tomb).
#[must_use]
pub fn marks(lock: Lock, victories: u32, hover: menu::Hover) -> Vec<Mark> {
    let lit = (hover == menu::Hover::Over).then_some(Mark::Lit);
    let star = (victories > 0).then_some(Mark::Star);
    let cloud = (lock == Lock::Locked).then_some(Mark::Cloud);

    [lit, cloud, star].into_iter().flatten().collect()
}

/// What a locked quest's title bar says it needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Requires {
    /// The rule's ids in token order, repeats kept.
    Listed(Vec<FourCc>),
    /// [`SOUTHERN_TEXT`], for [`SOUTHERN_SLOT`].
    Southern,
}

/// `quest`'s requirements as the title bar names them.
#[must_use]
pub fn requires(quest: FourCc, rules: &[deps::Quest]) -> Requires {
    if slot(quest) == Some(SOUTHERN_SLOT) {
        Requires::Southern
    } else {
        let ids = rules
            .iter()
            .find(|r| r.id == quest)
            .and_then(|r| r.requires.as_ref())
            .map(deps::Expr::ids)
            .unwrap_or_default();

        Requires::Listed(ids)
    }
}

/// Every `GMTX` string [`title_bar`] reads: the lines, the difficulties,
/// the requirements' colours, the secret title, and the best time's day.
pub const TITLE_BAR_TEXTS: [usize; 13] = [
    DIFFICULTY_TEXT,
    WON_TEXT,
    LOCKED_TEXT,
    difficulty_text(Difficulty::Easy),
    difficulty_text(Difficulty::Medium),
    difficulty_text(Difficulty::Hard),
    difficulty_text(Difficulty::Master),
    SOUTHERN_TEXT,
    SECRET_TEXT,
    MET_TEXT,
    UNMET_TEXT,
    crate::view::panel::day::DAY_TEXT.index,
    crate::view::panel::day::PLURAL.index,
];

/// The title bar's title and line for hovered `quest`, colour codes kept,
/// as quest map hover sets them:
///
/// ```text
///  open, not won   its title   GMTX 136  Difficulty: %s
///  open, won       its title   GMTX 137  … Victories: %d  Best Time: %s
///  locked          GMTX 268    GMTX 138  … Requires: %s
///                                        each id 323 won, else 324; QU06 142
/// ```
///
/// `gmtx` looks up a `GMTX` string by index, of [`TITLE_BAR_TEXTS`];
/// `title` names a quest. `None` where a string
/// is missing.
#[must_use]
pub fn title_bar(
    gmtx: &dyn Fn(usize) -> Option<String>,
    quest: FourCc,
    (rules, progress): (&[deps::Quest], &Progress),
    title: &dyn Fn(FourCc) -> Option<String>,
) -> Option<(String, String)> {
    let difficulty = rules
        .iter()
        .find(|r| r.id == quest)
        .and_then(|r| gmtx(difficulty_text(r.difficulty)))?;

    match (lock(quest, rules, progress), progress.record(quest)) {
        (Lock::Locked, _) => {
            let needs = match requires(quest, rules) {
                Requires::Southern => gmtx(SOUTHERN_TEXT)?,
                Requires::Listed(ids) => ids
                    .into_iter()
                    .map(|id| {
                        let colour = if progress.won(id) {
                            MET_TEXT
                        } else {
                            UNMET_TEXT
                        };

                        Some(printf(&gmtx(colour)?, &[(&title(id)?).into()]))
                    })
                    .collect::<Option<String>>()?,
            };
            let line = printf(
                &gmtx(LOCKED_TEXT)?,
                &[(&difficulty).into(), (&needs).into()],
            );

            Some((gmtx(SECRET_TEXT)?, line))
        }
        (Lock::Open, record) => Some((title(quest)?, open_line(gmtx, &difficulty, record)?)),
    }
}

/// An open quest's line, `difficulty` its text: `GMTX` 136 with it, or
/// once won 137 with it, the victories and the best time. The map's
/// title bar and the extra quests' list `APd8`
/// on a pick, a mod's quest's record by its code, say
/// it so. `None` where a string is missing.
#[must_use]
pub fn open_line(
    gmtx: &dyn Fn(usize) -> Option<String>,
    difficulty: &str,
    record: Option<Record>,
) -> Option<String> {
    match record {
        Some(record) => {
            let best = Day::of(record.best_ms).text_by(gmtx)?;
            let args = [difficulty.into(), record.victories.into(), (&best).into()];

            Some(printf(&gmtx(WON_TEXT)?, &args))
        }
        None => Some(printf(&gmtx(DIFFICULTY_TEXT)?, &[difficulty.into()])),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A profile's counts add in 32 bits: a victory past the most wraps,
    /// as does the total.
    #[test]
    fn victories_count_in_32_bits() {
        let (one, two) = (SLOTS[0], SLOTS[1]);
        let most = Record {
            victories: u32::MAX,
            best_ms: 1,
        };
        let mut progress = Progress::of([(one, most), (two, most)]);

        assert_eq!(progress.total(), u32::MAX - 1);

        progress.win(one, 1);

        assert_eq!(progress.victories(one), 0);
    }

    /// Majesty Expanded's Barren Waste is keyed by its id's CRC, so its
    /// victory is its own; `NewGame+`'s Legendary Heroes reuses the
    /// shipped id, so its victory is the shipped quest's.
    #[test]
    fn a_mod_quests_victory_is_kept_by_its_code() {
        let copy = Guid::parse("{31305551-0054-0032-0000-000000000000}").expect("a GUID");
        let heroes = Guid::parse("{34305158-0000-0000-0000-000000000000}").expect("a GUID");
        let mut progress = Progress::default();

        progress.win_quest(copy, 1);
        progress.win_quest(heroes, 1);

        let crc = quest::key_of(copy);

        assert!(!progress.won(FourCc(*b"QU01")));
        assert_eq!(progress.victories(crc), 1);
        assert_eq!(progress.id(crc), Some(copy));
        assert_eq!(progress.last_won, Some(FourCc(*b"XQ04")));
        assert!(progress.won(FourCc(*b"XQ04")));
        assert_eq!(progress.id(FourCc(*b"XQ04")), None);
    }

    #[test]
    fn the_expansions_ending_comes_with_every_expansion_quest_won() {
        let mut progress = Progress::default();

        for &quest in &SLOTS[BASE_SLOTS..LAST_EXPANSION_SLOT] {
            progress.win(quest, 1);
        }

        assert_eq!(SLOTS[LAST_EXPANSION_SLOT], FourCc(*b"XQ12"));
        assert!(!progress.expansion_won(), "XQ12 still to win");

        progress.win(SLOTS[LAST_EXPANSION_SLOT], 1);
        assert!(progress.expansion_won());
        assert!(progress.ending_due());

        progress.seen.ending = true;
        assert!(!progress.ending_due(), "once");

        progress.erase();
        assert!(!progress.expansion_won());
        assert!(!progress.seen.ending);
    }

    /// Every base quest won unlocks nothing; the expansion's, the north's
    /// film, until Erase Victories.
    #[test]
    fn only_the_expansion_unlocks_a_film() {
        let base = Progress::of(SLOTS[..BASE_SLOTS].iter().map(|&q| {
            (
                q,
                Record {
                    victories: 1,
                    best_ms: 1,
                },
            )
        }));

        assert_eq!(base.films(), [Film::Opening]);

        let mut progress = Progress::of(SLOTS[BASE_SLOTS..=LAST_EXPANSION_SLOT].iter().map(|&q| {
            (
                q,
                Record {
                    victories: 1,
                    best_ms: 1,
                },
            )
        }));

        assert_eq!(progress.films(), [Film::Opening, Film::North]);

        progress.erase();
        assert_eq!(progress.films(), [Film::Opening]);
    }

    const QU01: FourCc = FourCc(*b"QU01");

    #[test]
    fn a_victory_counts_and_keeps_the_shortest_time() {
        let mut progress = Progress::default();

        progress.win(QU01, 900_000);
        progress.win(QU01, 1_200_000);
        progress.win(QU01, 600_000);

        assert_eq!(
            progress.record(QU01),
            Some(Record {
                victories: 3,
                best_ms: 600_000
            })
        );

        progress.erase();

        assert_eq!(progress.victories(QU01), 0);
        assert!(!progress.won(QU01));
    }

    #[test]
    fn a_hovered_locked_quest_draws_lit_under_its_cloud() {
        use menu::Hover::{Away, Over};

        assert_eq!(marks(Lock::Locked, 0, Over), [Mark::Lit, Mark::Cloud]);
        assert_eq!(marks(Lock::Locked, 0, Away), [Mark::Cloud]);
        assert_eq!(marks(Lock::Open, 2, Over), [Mark::Lit, Mark::Star]);
        assert_eq!(marks(Lock::Open, 0, Away), []);
    }

    #[test]
    fn a_slots_video_is_qm_and_the_slot_in_two_digits() {
        assert_eq!(video(0), Some(FourCc(*b"QM00")));
        assert_eq!(video(28), Some(FourCc(*b"QM28")));
        assert_eq!(video(34), None);
    }

    #[test]
    fn a_pog_is_counted_from_its_datasets_first_slot() {
        assert_eq!(
            pog(8),
            Some(Art {
                record: FourCc(*b"INTD"),
                set: 1008
            })
        );
        assert_eq!(
            pog(28),
            Some(Art {
                record: FourCc(*b"IX11"),
                set: 1009
            })
        );
        assert_eq!(
            pog(32),
            Some(Art {
                record: FourCc(*b"IXD2"),
                set: 1000
            })
        );
    }

    /// Opens the map as a new session would: the session byte cleared.
    fn session(progress: &mut Progress) -> Opening {
        progress.hinted = false;
        progress.open_map(&[])
    }

    #[test]
    fn the_first_map_plays_the_introduction_once_ever_and_hints_once_a_session() {
        let mut progress = Progress::default();
        let first = progress.open_map(&[]);

        assert_eq!(first.voice, Some(INTRODUCTION));
        assert_eq!(first.hint, Some(143));
        assert!(progress.seen.advisor);

        // Back from a briefing, the same session: nothing.
        let again = progress.open_map(&[]);

        assert_eq!((again.voice, again.hint), (None, None));

        progress.win(QU01, 600_000);

        let next = session(&mut progress);

        assert_eq!((next.voice, next.hint), (None, Some(144)));
    }

    #[test]
    fn three_victories_bring_the_expert_hint_and_its_voice_once_ever() {
        let mut progress = Progress::default();

        progress.open_map(&[]);

        for _ in 0..3 {
            progress.win(QU01, 600_000);
        }

        let expert = session(&mut progress);

        assert_eq!(expert.voice, Some(EXPANSION));
        assert_eq!(expert.hint, Some(EXPERT_HINT_TEXT));
        assert!(progress.seen.expert_hint);

        let later = session(&mut progress);

        assert_eq!((later.voice, later.hint), (None, None));

        // Past three, no hint at all.
        progress.win(QU01, 600_000);
        progress.seen.expert_hint = false;

        assert_eq!(session(&mut progress).hint, None);
    }

    #[test]
    fn erase_victories_forgets_the_seen_bytes_and_the_session() {
        let mut progress = Progress::default();

        progress.open_map(&[]);
        progress.win(QU01, 600_000);
        progress.erase();

        assert_eq!(progress, Progress::default());
        assert_eq!(progress.open_map(&[]).voice, Some(INTRODUCTION));
    }

    #[test]
    fn a_win_is_the_last_won() {
        let mut progress = Progress::default();

        progress.win(QU01, 600_000);
        progress.win(FourCc(*b"VAMP"), 600_000);

        assert_eq!(progress.last_won, Some(FourCc(*b"VAMP")));
        assert_eq!(progress.total(), 2);
    }

    #[test]
    fn the_slots_are_the_two_lists_then_three_downloads() {
        assert_eq!(slot(FourCc(*b"QU01")), Some(0));
        assert_eq!(slot(FourCc(*b"XQ01")), Some(BASE_SLOTS));
        assert_eq!(slot(FourCc(*b"XQD1")), Some(DOWNLOAD_SLOTS));
        assert_eq!(slot(FourCc(*b"MD01")), None);
    }
}
