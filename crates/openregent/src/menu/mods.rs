//! The mods page, over the main menu from its CHANGE MODS button: HD's
//! mods dialog, `APdc`, as one list in its art. Every
//! mod the folders hold, in load order, each enabled or not, moved up or
//! down; mod loading on or off. Done writes it to the settings file
//! ([`crate::files::mod_list`]); a new game loads those enabled, the menus' data
//! is not reloaded. Load and Save read and write collections, lists
//! saved apart; neither applies one.
//!
//! ```text
//!  ┌(pog)──────────────────── Mods ─────────────────────────────┐  INx2 frame
//!  │ [x] LOAD MODS  A new game loads the ticked mods, top to … │  IX90 1003
//!  │ ┌──────────── MODS, IN LOAD ORDER ─────────────────────┐  │  INx1 list box
//!  │ │ 1 [x] New Game+  Rebalances the game, …     [^][v]   │^│
//!  │ │   [ ] Bug fixes  Fixes for faults …  Built in [^][v] │#│  INDj arrows
//!  │ └──────────────────────────────────────────────────────┘v│  INDs bar
//!  │ ┌──────────────────── DESCRIPTION ──────────────────────┐ │
//!  │ │ New Game+   the picked row's details: its long        │^│  scrolls
//!  │ │ description, the games it works with, its quests,     │ │
//!  │ │ files, id and item folder                             │v│
//!  │ └───────────────────────────────────────────────────────┘ │
//!  │ 2 folders searched · 1 missing  (LOAD LIST…)(SAVE LIST…)(RESCAN)
//!  └───────(  DONE  )──────────────────────(  CANCEL  )─────────┘
//!
//!  LOAD LIST…  Load Mod List, in the page's frame art: the saved lists,
//!              the picked one's mods; LOAD, DELETE (asked first), CANCEL
//!  SAVE LIST…  Save Mod List: a name typed, or a list's taken; a name
//!              taken asked first. The questions short AP11 boxes
//!  RESCAN      the folders scanned again, changed GPL source compiled; the
//!              list's edits kept (`menu::rescan`)
//!  DONE        written, if changed, and closed     CANCEL: closed, dropped
//! ```
//!
//! Numbers are the enabled mods' load order. The page is 800 × 712 at
//! the menus' scale: `APdc`'s width, taller for the one list; its frame
//! pieces placed by their anchors, the bars repeated. Its layout is
//! openregent's own (`APdc` shows two lists side by side).
//!
//! Loading off greys the list and LOAD LIST…: neither is edited, the list
//! kept.
//!
//! HD keeps its choice one session (**Inferred**, mods free); this
//! game keeps it in the settings file. HD's main menu control 9001,
//! ACTIVATE MODS, has the Load mods toggle as stand-in.
//!
//! Over a LAN lobby, the host's (`menu::lan`), the page edits the
//! lobby's list instead: Done hands it to the lobby
//! ([`Editor::take_chosen`]) and writes no settings file; Load, Save
//! and Rescan work as here. A mod of the lobby's that a player lacks,
//! or holds in another version, shows its name red, and the players
//! beside it ([`Editor::set_problems`]):
//!
//! ```text
//!  2 [x] Decor Buildings  New decorations…  Bob: missing [^][v]
//!        └ red ┘                            └ red ─────┘
//! ```

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use bevy::log::warn;
use bevy_egui::egui::{self, Color32, Pos2, Rect, Sense, Vec2, pos2, vec2};
use regent_core::content::dataset::Datasets;
use regent_core::content::mods::{Base, Library, Mod};
use regent_core::data::fourcc::FourCc;
use regent_core::data::guid::Guid;
use regent_core::data::smnu::Kind;
use regent_core::view::menu::message::{self, Ask};
use regent_core::view::menu::{
    self as panels, LARGE_FONT, Layout, Role, SMALL_FONT, ShownControl, State, Tick,
};
use regent_draw::art as service;

use super::art::{self, Art, Pack, Pic};
use super::fit::Fit;
use super::font::{self, Font};
use super::page::{Button, Press};
use super::paint::{self, Able, Canvas, Inset, PALE, tapped};
use super::save_page::LIT;
use crate::files::compiled::{Builds, Built};
use crate::files::mod_list::{self, Entry, Loading};

/// The page's size, and its dialogs', layout pixels: `APdc`'s width,
/// taller for the one list. openregent's own.
const PAGE: Vec2 = vec2(800.0, 712.0);
const DIALOG: Vec2 = vec2(600.0, 460.0);

/// A layout rectangle `w` × `h` from (`x`, `y`).
const fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect::from_min_max(pos2(x, y), pos2(x + w, y + h))
}

/// A list's row: `IX90`'s 20 px box and a pixel each side.
const ROW: f32 = 22.0;

/// `APdc`'s pieces (`addinterface_se`): its ground `INx1` 1008, tiled;
/// its lists' and text boxes' wells and edges, `INx1` 1021-1039; its
/// frame, `INx2` 1000-1009, and the scroll ends a big button sits on,
/// 1011.
const GROUND: Pic = Pic::of(FourCc(*b"INx1"), 1008);
const BOXES: FourCc = FourCc(*b"INx1");
const WELL: i32 = 1030;
const FRAME: FourCc = FourCc(*b"INx2");
const BACKING: Pic = Pic::of(FourCc(*b"INx2"), 1011);
const BACKING_WIDTH: f32 = 281.0;
const BACKING_IN: Vec2 = vec2(24.0, 3.0);

/// The pog, `APdc` 1000, `DX00` 1000; the title, 8885, after it.
const POG: Pic = Pic::of(FourCc(*b"DX00"), 1000);
const POG_AT: Pos2 = pos2(14.0, 9.0);
const TITLE_AT: Pos2 = pos2(94.0, 23.0);
const TITLE_END: f32 = 32.0;
const TITLE_HEIGHT: f32 = 45.0;

/// A tick box: `MX18`'s, `IX90` 1003, slot 1 ticked.
const CHECK: Pic = Pic::of(FourCc(*b"IX90"), 1003);
const CHECK_SIZE: Vec2 = vec2(20.0, 20.0);

/// A row's moves: `APdc`'s 9102 and 9103, `INDj` 1000 and 1001; slot 2
/// greyed.
const UP: Pic = Pic::of(FourCc(*b"INDj"), 1000);
const DOWN: Pic = Pic::of(FourCc(*b"INDj"), 1001);

/// DONE's and CANCEL's button, `APdc` 2000 and 2001, `DEM2` 1001; the
/// small one, `AP29`'s row's, `DEM2` 1006.
const BIG: (FourCc, i32) = (FourCc(*b"DEM2"), 1001);
const SMALL: (FourCc, i32) = (FourCc(*b"DEM2"), 1006);
const SMALL_SIZE: Vec2 = vec2(140.0, 26.0);

/// A scroll bar's arrows and thumb, `APdc` 9100, `INDs` 1000
/// (`scroll::THUMB`).
const BAR: (FourCc, i32) = (FourCc(*b"INDs"), 1000);
const BAR_WIDTH: f32 = 25.0;

/// The page's own control ids, in no panel: its bars'.
const LIST_BAR_ID: i32 = 9300;
const DETAILS_BAR_ID: i32 = 9301;

/// The toggle's row under the title bar: its box, LOAD MODS, its line.
const TOGGLE: Rect = rect(40.0, 99.0, 116.0, 20.0);
const TOGGLE_LABEL: Rect = rect(66.0, 99.0, 90.0, 20.0);
const TOGGLE_SAYS: Rect = rect(160.0, 99.0, 600.0, 20.0);

/// The list's well.
const LIST_WELL: Rect = rect(37.0, 154.0, 711.0, 262.0);

/// A mod row's pieces from its band's corner: the number's box, the
/// tick box, the text; the arrows from its right.
const RANK_AT: Vec2 = vec2(2.0, 0.0);
const RANK_WIDTH: f32 = 16.0;
const CHECK_AT: Vec2 = vec2(24.0, 1.0);
const TEXT_AT: Vec2 = vec2(52.0, 0.0);
const TEXT_END: Vec2 = vec2(64.0, 0.0);
const UP_AT: Vec2 = vec2(-58.0, 3.0);
const DOWN_AT: Vec2 = vec2(-29.0, 3.0);

/// A row's note: room past its width, and its right end in from the
/// text's. Between the name and the note, the least room for a short
/// description, and the short description's gap from the note.
const NOTE_ROOM: f32 = 10.0;
const NOTE_INSET: f32 = 6.0;
const NAME_GAP: f32 = 4.0;
const SHORT_LEAST: f32 = 40.0;
const SHORT_ROOM: f32 = 16.0;

/// Between a name and its short description; between a row's notes.
const SHORT_GAP: &str = "   ";
const NOTE_GAP: &str = " · ";

/// The DESCRIPTION box; its text from the box's corner, less the box's
/// size; its bar from the box's top right, and the room it takes. Its
/// lines kept this far inside the box's top and bottom.
const DETAILS: Rect = rect(33.0, 454.0, 732.0, 154.0);
const DETAILS_TEXT_AT: Vec2 = vec2(13.0, 16.0);
const DETAILS_TEXT_LESS: Vec2 = vec2(53.0, 30.0);
const DETAILS_BAR_AT: Vec2 = vec2(-30.0, 12.0);
const DETAILS_BAR: Vec2 = vec2(BAR_WIDTH, 132.0);
const DETAILS_BAR_ROOM: f32 = 30.0;
const EDGE: f32 = 4.0;

/// The foot: its row's top, the folders' line, the small buttons' left
/// and step; DONE's and CANCEL's scroll ends in from the sides and up
/// from the bottom.
const TAIL_TOP: f32 = 624.0;
const SOURCES_AT: Pos2 = pos2(42.0, 626.0);
const SMALL_LEFT: f32 = 318.0;
const SMALL_STEP: f32 = 150.0;
const CLOSE_LEFT: f32 = 83.0;
const CLOSE_UP: f32 = 45.0;

/// The dialogs: the buttons' gap and their row up from the bottom;
/// Load's list well and its line's box, the line's margin; Save's name
/// field, its label and the typed name in it, its list's well and its
/// line; a saved list's name in from its row's left, its count from the
/// right.
const DIALOG_GAP: f32 = 16.0;
const DIALOG_BUTTONS_UP: f32 = 44.0;
const LOAD_WELL: Rect = rect(37.0, 128.0, 511.0, 190.0);
const LOAD_LINE: Rect = rect(33.0, 352.0, 534.0, 38.0);
const LINE_MARGIN: f32 = 28.0;
const SAVE_FIELD: Rect = rect(33.0, 112.0, 534.0, 36.0);
const NAME_LABEL_AT: Vec2 = vec2(14.0, 0.0);
const NAME_LABEL_WIDTH: f32 = 60.0;
const NAME_AT: Vec2 = vec2(70.0, 0.0);
const NAME_END: f32 = 14.0;
const SAVE_WELL: Rect = rect(37.0, 190.0, 511.0, 150.0);
const SAVE_LINE_AT: Pos2 = pos2(42.0, 357.0);
const LIST_NAME_IN: f32 = 13.0;
const COUNT_IN: f32 = 8.0;

/// The page's words: placeholders, HD has none of them.
const TITLE: &str = "Mods";
const LOAD_MODS: &str = "LOAD MODS";
const LIST_HEADING: &str = "MODS, IN LOAD ORDER";
const DESCRIPTION: &str = "DESCRIPTION";
const LOAD_LIST: &str = "LOAD LIST…";
const SAVE_LIST: &str = "SAVE LIST…";
const RESCAN: &str = "RESCAN";
const DONE: &str = "DONE";
const PICK_HINT: &str = "Pick a mod for its details.";
const NONE_FOUND: &str = "None found.";
const MISSING_SAYS: &str = "Listed, but no folder holds it:";

/// The dialogs' words: placeholders.
const LOAD_TITLE: &str = "Load Mod List";
const SAVE_TITLE: &str = "Save Mod List";
const OVERWRITE_TITLE: &str = "OVERWRITE";
const DELETE_TITLE: &str = "DELETE";
const LISTS_HEADING: &str = "SAVED MOD LISTS";
const LOAD: &str = "LOAD";
const DELETE: &str = "DELETE";
const STORE: &str = "SAVE";
const NAME_LABEL: &str = "NAME:";
const THIS_LIST: &str = "This list: ";
const NONE_SAVED: &str = "None saved.";

/// Rescan's hover text.
const RESCAN_SAYS: &str = "Scan the mod folders again; changed GPL source is compiled.";

/// A built-in mod's mark ([`crate::files::builtin`]).
const BUILT_IN: &str = "Built in";

/// A row's mark for a mod whose GPL source will not compile.
const REFUSED: &str = "Refused";

/// A row's mark for a mod no game loads ([`never_loads`]).
const NEVER_LOADS: &str = "Never loads";

/// Text colours: headings `MMS1`'s text at rest (`APdc`'s set none),
/// text `APdc`'s white, a row's side text dim (`extras`' mod names), the
/// picked name and the toggle's accent (the settings page's values'), a
/// problem `MMS1`'s disabled red, grey while loading is off, a button's
/// navy where its look sets none.
const HEAD: Color32 = Color32::from_rgb(0xf8, 0xb1, 0x52);
const WHITE: Color32 = Color32::WHITE;
const DIM: Color32 = Color32::from_gray(0xa0);
const ACCENT: Color32 = Color32::from_rgb(0xf6, 0xad, 0x52);
const PROBLEM: Color32 = Color32::from_rgb(0xdb, 0x21, 0x24);
const GREY: Color32 = Color32::from_gray(0x70);
const NAVY: Color32 = Color32::from_rgb(0x00, 0x00, 0x3f);

/// Over the list while loading is off. A guess: `IX90` has no greyed
/// ticked slot.
const VEIL: Color32 = Color32::from_black_alpha(120);

/// The toggle's line, on and off.
const ON_SAYS: &str =
    "A new game loads the ticked mods, top to bottom; the lower one wins a clash.";
const OFF_SAYS: &str = "Off: a new game loads no mods. The list is kept.";

/// The pictures the page draws that no panel it reads names, each in
/// every slot a control may show (`art::SLOTS`): the menus' art reads
/// only the pictures named (`art::Store`).
pub(super) fn pictures() -> Vec<(Pack, Pic)> {
    let slotted = |pic: Pic| (0..art::SLOTS).map(move |s| pic.slot(s));
    let frame = (1000..=1011).map(|set| Pic::of(FRAME, set));
    let boxes = (1021..=1039).map(|set| Pic::of(BOXES, set));
    let bar = (0..=crate::ui::scroll::THUMB).map(|n| Pic::of(BAR.0, BAR.1 + n));
    let expansion = frame
        .chain(boxes)
        .chain([GROUND, POG])
        .chain(slotted(CHECK))
        .map(|pic| (Pack::Expansion, pic));
    let base = [UP, DOWN, Pic::of(BIG.0, BIG.1), Pic::of(SMALL.0, SMALL.1)]
        .into_iter()
        .chain(bar)
        .flat_map(slotted)
        .map(|pic| (Pack::Base, pic));

    expansion.chain(base).collect()
}

/// The page's state: the mods found, their folders, where the lists are
/// written, and the list being edited.
#[derive(Default, bevy::prelude::Resource)]
pub struct Editor {
    library: Library,
    /// The mods compiled from source, or refused.
    builds: Builds,
    folders: Vec<PathBuf>,
    /// The list file Done writes; `None` with no config directory.
    list_file: Option<PathBuf>,
    /// The collections' folder; `None` with no data directory.
    lists: Option<PathBuf>,
    /// The list being edited; `None` while closed.
    draft: Option<List>,
    /// The list as the settings file holds it: Done writes a change.
    saved: List,
    /// The dialog open over the page, if any.
    asked: Option<Asked>,
    /// The collections' names, read as a dialog opens, and each one's
    /// entries.
    listed: Vec<String>,
    contents: Vec<Vec<Entry>>,
    /// The collection lit in Load's dialog, and that list's scroll, rows.
    list_lit: Option<usize>,
    list_scroll: f32,
    /// The list's scroll, rows; the details', layout pixels.
    scroll: f32,
    about: f32,
    /// The row whose details show.
    picked: Option<Guid>,
    /// Rescan pressed: the folders to scan again (`menu::rescan`).
    rescan: bool,
    /// Whose list it edits.
    target: Target,
    /// The lobby's list as Done left it, not yet taken.
    chosen: Option<(Vec<Entry>, Loading)>,
    /// Over a lobby, each mod a player lacks or holds in another
    /// version: the players' lines, by GUID.
    problems: BTreeMap<Guid, Vec<String>>,
}

/// Whose list the page edits.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Target {
    /// The settings file's: Done writes it.
    #[default]
    Settings,
    /// A LAN lobby's: Done hands it over.
    Lobby,
}

impl Editor {
    /// The page on the mods `install`'s folders hold.
    pub fn on(&mut self, install: &crate::files::env::Install) {
        let found = (install.mods().clone(), install.builds());

        self.found(found, install.mod_folders.clone());
    }
}

/// A list as the page edits it.
#[derive(Clone, Debug, Default)]
struct List {
    /// Every found mod, in load order.
    rows: Vec<Entry>,
    /// The entries no folder holds: not shown, written back.
    kept: Vec<Entry>,
    loading: Loading,
}

/// Rows the same by GUID and flag alone: their text is rewritten from
/// the manifests on save.
impl PartialEq for List {
    fn eq(&self, other: &List) -> bool {
        let shown = |l: &List| {
            l.rows
                .iter()
                .map(|e| (e.guid, e.enabled))
                .collect::<Vec<_>>()
        };

        shown(self) == shown(other) && self.kept == other.kept && self.loading == other.loading
    }
}

impl List {
    /// Its entries over `library`: a mod gone kept unshown, one back
    /// listed last, one new added disabled; flags as they were.
    fn over(&self, library: &Library) -> List {
        let entries: Vec<Entry> = self.rows.iter().chain(&self.kept).cloned().collect();
        let (rows, kept) = mod_list::rows(&entries, library);

        List {
            rows,
            kept,
            loading: self.loading,
        }
    }
}

/// One edit of the list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Edit {
    Up(usize),
    Down(usize),
    Toggle(usize),
    /// Mod loading turned on or off.
    Loading,
}

/// The page's own buttons.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Act {
    Load,
    Save,
    Rescan,
    Done,
    Cancel,
}

/// A dialog over the page.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Asked {
    /// A collection to load, or delete.
    Load,
    /// The name typed so far.
    Save(String),
    /// Whether to overwrite the collection of that name.
    Overwrite(String),
    /// Whether to delete the collection of that name.
    Delete(String),
}

/// A press in a dialog.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Reply {
    Pick(String),
    Delete(String),
    Store(String),
    Yes,
    No,
}

/// Whether saving over a collection asks first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Over {
    Ask,
    Replace,
}

impl Editor {
    /// The mods `library` holds, found in `folders`, `builds` those
    /// compiled from source; the lists in their XDG places.
    pub fn found(&mut self, (library, builds): (Library, Builds), folders: Vec<PathBuf>) {
        self.library = library;
        self.builds = builds;
        self.folders = folders;
        self.list_file = mod_list::path();
        self.lists = mod_list::lists_place();

        // A rescan with the page open: its lists over the mods found
        // now, the edits kept.
        if let Some(draft) = &self.draft {
            self.draft = Some(draft.over(&self.library));
            self.saved = self.saved.over(&self.library);
        }
    }

    /// Whether Rescan was pressed since the last ask.
    pub const fn take_rescan(&mut self) -> bool {
        std::mem::replace(&mut self.rescan, false)
    }

    /// Opened on `entries`, the file's list, and `loading`, its toggle.
    pub fn open(&mut self, entries: &[Entry], loading: Loading) {
        self.opened(entries, loading, Target::Settings);
    }

    /// Opened over a LAN lobby on its list, `entries` and `loading`.
    pub fn open_lobby(&mut self, entries: &[Entry], loading: Loading) {
        self.opened(entries, loading, Target::Lobby);
    }

    /// The page as `--menu mods:…,SHOWN` holds it for a screenshot:
    /// `load` or `save` its dialog, `overwrite` or `delete` its question
    /// on the first saved list.
    pub fn held(&mut self, shown: &str) {
        self.ask(Asked::Load);

        let first = self.listed.first().cloned().unwrap_or_default();
        let asked = match shown {
            "save" => Asked::Save(first),
            "overwrite" => Asked::Overwrite(first),
            "delete" => Asked::Delete(first),
            _ => Asked::Load,
        };

        self.ask(asked);
    }

    /// Whether it is open over a LAN lobby.
    pub const fn over_lobby(&self) -> bool {
        self.draft.is_some() && matches!(self.target, Target::Lobby)
    }

    /// The lobby's list, once Done gave it: every entry, and the toggle.
    pub const fn take_chosen(&mut self) -> Option<(Vec<Entry>, Loading)> {
        self.chosen.take()
    }

    /// The lobby's mods a player lacks or holds in another version, the
    /// players' lines by GUID: their rows marked.
    pub fn set_problems(&mut self, problems: BTreeMap<Guid, Vec<String>>) {
        self.problems = problems;
    }

    /// Opened on `entries` and `loading`, for `target`.
    fn opened(&mut self, entries: &[Entry], loading: Loading, target: Target) {
        let (rows, kept) = mod_list::rows(entries, &self.library);

        self.target = target;
        self.chosen = None;
        self.problems.clear();

        self.saved = List {
            rows,
            kept,
            loading,
        };
        self.picked = self.saved.rows.first().and_then(|e| e.guid);
        self.draft = Some(self.saved.clone());
        self.asked = None;
        self.scroll = 0.0;
        self.about = 0.0;
    }

    /// The list being edited, `edit` made to it.
    fn edit(&mut self, edit: Edit) {
        if let Some(draft) = &mut self.draft {
            match edit {
                Edit::Loading => {
                    draft.loading = match draft.loading {
                        Loading::On => Loading::Off,
                        Loading::Off => Loading::On,
                    };
                }
                _ => apply(&mut draft.rows, edit),
            }
        }
    }

    /// Whether the list differs from the settings file's.
    fn changed(&self) -> bool {
        self.draft.as_ref().is_some_and(|d| *d != self.saved)
    }

    /// `act` done: a dialog opened, or the page closed, written first
    /// on Done if changed.
    fn act(&mut self, act: Act) {
        match act {
            Act::Load => self.ask(Asked::Load),
            Act::Save => self.ask(Asked::Save(String::new())),
            Act::Rescan => self.rescan = true,
            Act::Done if self.target == Target::Lobby => {
                self.chosen = self.draft.take().map(|draft| {
                    let entries = draft.rows.into_iter().chain(draft.kept).collect();

                    (entries, draft.loading)
                });
            }
            Act::Done => {
                if let (true, Some(path), Some(draft)) =
                    (self.changed(), &self.list_file, &self.draft)
                    && let Err(e) = mod_list::write_at(
                        path,
                        (&draft.rows, &draft.kept),
                        &self.library,
                        Some(draft.loading),
                    )
                {
                    warn!("mods not saved: {e}");
                }

                self.draft = None;
            }
            Act::Cancel => self.draft = None,
        }
    }

    /// `asked` open, the collections read for it.
    fn ask(&mut self, asked: Asked) {
        self.listed = self.lists.as_deref().map_or_default(mod_list::lists);
        self.contents = self.lists.as_deref().map_or_default(|dir| {
            self.listed
                .iter()
                .map(|name| mod_list::read_at(&mod_list::list_path(dir, name)))
                .collect()
        });
        self.list_lit = self
            .list_lit
            .filter(|&i| i < self.listed.len())
            .or_else(|| (!self.listed.is_empty()).then_some(0));
        self.asked = Some(asked);
    }

    /// The open dialog given `reply`.
    fn reply(&mut self, reply: Reply) {
        match (self.asked.take(), reply) {
            (Some(Asked::Load), Reply::Pick(name)) => self.load(&name),
            (Some(Asked::Load), Reply::Delete(name)) => self.ask(Asked::Delete(name)),
            (Some(Asked::Save(_)), Reply::Store(name)) => self.store(&name, Over::Ask),
            (Some(Asked::Overwrite(name)), Reply::Yes) => self.store(&name, Over::Replace),
            (Some(Asked::Overwrite(name)), Reply::No) => self.ask(Asked::Save(name)),
            (Some(Asked::Delete(name)), Reply::Yes) => {
                self.delete(&name);
                self.ask(Asked::Load);
            }
            (Some(Asked::Delete(_)), Reply::No) => self.ask(Asked::Load),
            (asked, Reply::No) => drop(asked),
            (asked, _) => self.asked = asked,
        }
    }

    /// The collection `name` loaded into the page, not applied: its
    /// order, flags and the entries no folder holds; the toggle as was.
    fn load(&mut self, name: &str) {
        if let (Some(dir), Some(draft)) = (&self.lists, &mut self.draft) {
            let entries = mod_list::read_at(&mod_list::list_path(dir, name));
            let (rows, kept) = mod_list::rows(&entries, &self.library);

            draft.rows = rows;
            draft.kept = kept;
        }
    }

    /// The page's list saved as the collection `name`, made safe; a
    /// name taken asks first where `over` says so.
    fn store(&mut self, name: &str, over: Over) {
        let named = mod_list::list_name(name);

        match (named, &self.lists, &self.draft) {
            (Some(named), Some(dir), Some(draft)) => {
                let path = mod_list::list_path(dir, &named);

                if over == Over::Ask && path.exists() {
                    self.ask(Asked::Overwrite(named));
                } else if let Err(e) =
                    mod_list::write_at(&path, (&draft.rows, &draft.kept), &self.library, None)
                {
                    warn!("mod list not saved: {e}");
                }
            }
            // Nothing to name it by: the dialog stays.
            _ => self.ask(Asked::Save(name.to_owned())),
        }
    }

    /// The collection `name` removed.
    fn delete(&self, name: &str) {
        if let Some(dir) = &self.lists {
            let path = mod_list::list_path(dir, name);

            if let Err(e) = std::fs::remove_file(&path) {
                warn!("{}: {e}", path.display());
            }
        }
    }
}

/// `list` with `edit` made: moves stop at the ends.
const fn apply(list: &mut [Entry], edit: Edit) {
    match edit {
        Edit::Up(i) if i > 0 && i < list.len() => list.swap(i - 1, i),
        Edit::Down(i) if i + 1 < list.len() => list.swap(i, i + 1),
        Edit::Toggle(i) if i < list.len() => list[i].enabled = !list[i].enabled,
        _ => {}
    }
}

/// The page, and a dialog over it: Done writes the list, if changed, and
/// closes; Cancel closes, the edits dropped. Either is Cancel's press.
/// Drawn on a layer of its own over the main menu or the lobby, at the
/// menus' scale; a dialog open takes the presses.
pub fn page(
    ctx: &egui::Context,
    (art, shared): (&mut Art, &mut service::Art),
    editor: &mut Editor,
) -> Option<Press> {
    let draft = editor.draft.clone()?;
    let Art { book, ink, .. } = art;
    let (prompt, message) = (book.prompt.as_ref()?, book.message.as_ref()?);
    let fit = Fit::page(ctx.viewport_rect(), PAGE);
    let mut canvas = Canvas::at(above(ctx, "mods"), (ink, shared), fit);
    let fonts = Fonts {
        body: canvas.ink.font(SMALL_FONT)?,
        title: canvas.ink.font(LARGE_FONT)?,
    };

    // The buttons take the name prompt's look: its ACCEPT's font and
    // colours; CANCEL its own words.
    let cancel = prompt
        .control(panels::prompt::CANCEL)
        .and_then(|c| c.text.clone())
        .unwrap_or_else(|| Button::Cancel.label().to_uppercase());
    let look = Look {
        button: prompt.control(panels::prompt::ACCEPT)?,
        cancel: &cancel,
        fonts,
    };
    let free = editor.asked.is_none();
    let window = canvas.ui.max_rect();

    canvas.block(window);

    let pressed = sheet(&mut canvas, editor, &draft, look);
    let reply = dialog(&mut canvas, editor, (look, message));
    let pressed = if free { pressed } else { Pressed::NONE };

    if let Some(edit) = pressed.edit {
        editor.edit(edit);
    }

    // A new row picked: its details from their top.
    if pressed.picked.is_some() && pressed.picked != editor.picked {
        editor.picked = pressed.picked;
        editor.about = 0.0;
    }

    if let Some(reply) = reply {
        editor.reply(reply);
    }

    if let Some(act) = pressed.act {
        editor.act(act);
    }

    matches!(pressed.act, Some(Act::Done | Act::Cancel)).then_some(Press::Button(Button::Cancel))
}

/// A page's ui, told apart by `key`: a layer over the menus' pages, as
/// the save dialog's.
pub(super) fn above(ctx: &egui::Context, key: &str) -> egui::Ui {
    egui::Ui::new(
        ctx.clone(),
        egui::Id::new(key),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::new(
                egui::Order::Foreground,
                egui::Id::new(key),
            ))
            .max_rect(ctx.viewport_rect()),
    )
}

/// The fonts the page sets its text in: `fnt4` for all but the title,
/// `fn11`.
#[derive(Clone, Copy)]
struct Fonts {
    body: Font,
    title: Font,
}

/// What the buttons look like: AP35's ACCEPT, CANCEL's words, the fonts.
#[derive(Clone, Copy)]
struct Look<'a> {
    button: &'a ShownControl,
    cancel: &'a str,
    fonts: Fonts,
}

/// What the page's controls asked this pass.
#[derive(Clone, Copy)]
struct Pressed {
    edit: Option<Edit>,
    picked: Option<Guid>,
    act: Option<Act>,
}

impl Pressed {
    const NONE: Pressed = Pressed {
        edit: None,
        picked: None,
        act: None,
    };
}

/// The page in `APdc`'s art, back to front:
///
/// ```text
///  INx1 1008 tiled   the ground
///  list box          well 1030, edges 1021-1029, heading plate 1023, bar
///  INx2 1000-1009    the frame; DX00 1000 the pog, the title in fn11
///  head              IX90 1003 LOAD MODS, its line
///  rows              number, IX90 box, name and short, note, INDj arrows
///  DESCRIPTION       well 1030, edges 1031-1039, plate 1033, bar
///  tail              folders, DEM2 1006 ×3, INx2 1011 + DEM2 1001 ×2
/// ```
fn sheet(canvas: &mut Canvas<'_>, editor: &mut Editor, draft: &List, look: Look<'_>) -> Pressed {
    let body = look.fonts.body;

    canvas.tiled(
        Pack::Expansion,
        GROUND,
        Rect::from_min_size(Pos2::ZERO, PAGE),
    );

    let (rows, bar) = list_box(canvas, LIST_WELL, LIST_HEADING, body);

    frame(canvas, PAGE);
    title(canvas, (TITLE, PAGE.x), look.fonts.title);

    let toggled = head(canvas, draft.loading, body);
    let (edit, picked) = list(canvas, editor, draft, (rows, bar), body);

    details(canvas, editor, body);

    let act = tail(canvas, editor, draft, look);

    Pressed {
        edit: toggled.then_some(Edit::Loading).or(edit),
        picked,
        act,
    }
}

/// The toggle, LOAD MODS, and a line on what it does: whether it was
/// pressed.
fn head(canvas: &mut Canvas<'_>, loading: Loading, font: Font) -> bool {
    let on = loading == Loading::On;
    let says = match loading {
        Loading::On => ON_SAYS,
        Loading::Off => OFF_SAYS,
    };
    let response = canvas.ui.interact(
        canvas.fit.rect(TOGGLE),
        canvas.ui.id().with("load mods"),
        Sense::click(),
    );

    canvas.picture(Pack::Expansion, CHECK.slot(usize::from(on)), TOGGLE.min);
    put(
        canvas,
        font,
        TOGGLE_LABEL,
        &[(HEAD, LOAD_MODS.to_owned())],
        Side::Left,
    );
    put(
        canvas,
        font,
        TOGGLE_SAYS,
        &[(WHITE, says.to_owned())],
        Side::Left,
    );

    tapped(&response)
}

/// Every found mod in load order, a row each in the list's room `rows`:
/// its number while enabled, box, name and short description, note,
/// moves; the bar at `bar`. Loading off, the list greyed and kept from
/// edits. The edit pressed, and the row picked.
fn list(
    canvas: &mut Canvas<'_>,
    editor: &mut Editor,
    draft: &List,
    (rows, bar): (Rect, Rect),
    font: Font,
) -> (Option<Edit>, Option<Guid>) {
    let on = draft.loading == Loading::On;
    let shown = shown_rows(rows.height());
    let count = draft.rows.len();

    scroll_list(
        canvas,
        (bar, rows),
        (&mut editor.scroll, count, shown),
        "mod list",
    );

    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a list's rows"
    )]
    let first = editor.scroll.round() as usize;
    let ranks = ranks(&draft.rows);
    let listed: Vec<(usize, &Entry, Option<usize>, &Mod)> = draft
        .rows
        .iter()
        .zip(ranks)
        .enumerate()
        .skip(first)
        .take(shown)
        .filter_map(|(i, (e, rank))| {
            Some((i, e, rank, e.guid.and_then(|g| editor.library.find(g))?))
        })
        .collect();
    let able = while_loading(draft.loading);
    let mut pressed = (None, None);

    // The rows' pictures and press areas: a row's box and arrows over its
    // band, which picks it.
    for (n, &(i, e, _, m)) in listed.iter().enumerate() {
        let band = row_band(rows, n);
        let response = canvas.ui.interact(
            canvas.fit.rect(band),
            canvas.ui.id().with(("mod row", i)),
            Sense::click(),
        );

        if on && tapped(&response) {
            pressed.1 = Some(m.guid);
        }

        if on && editor.picked == Some(m.guid) {
            canvas
                .painter()
                .rect_filled(canvas.fit.rect(band), 0.0, LIT);
        }

        let ticked = if e.enabled {
            Tick::Checked
        } else {
            Tick::Clear
        };
        let toggled = tick(canvas, (band.min + CHECK_AT, ticked), ("mod box", i), able);
        let up = push(
            canvas,
            (Pack::Base, UP, band.right_top() + UP_AT),
            ("mod up", i),
            able,
        );
        let down = push(
            canvas,
            (Pack::Base, DOWN, band.right_top() + DOWN_AT),
            ("mod down", i),
            able,
        );
        let edit = toggled
            .then_some(Edit::Toggle(i))
            .or(tapped(&up).then_some(Edit::Up(i)))
            .or(tapped(&down).then_some(Edit::Down(i)));

        pressed.0 = pressed.0.or(edit.filter(|_| on));
    }

    // Loading off: a veil over the pictures, the text grey on it.
    if !on {
        canvas
            .painter()
            .rect_filled(canvas.fit.rect(rows), 0.0, VEIL);
    }

    for (n, &(_, _, rank, m)) in listed.iter().enumerate() {
        let band = row_band(rows, n);
        let number = Rect::from_min_size(band.min + RANK_AT, vec2(RANK_WIDTH, ROW));
        let text = Rect::from_min_max(band.min + TEXT_AT, band.max - TEXT_END);
        let shade = if on { Shade::Lit } else { Shade::Grey };

        if let Some(rank) = rank {
            put(
                canvas,
                font,
                number,
                &[(shade.or(DIM), rank.to_string())],
                Side::Right,
            );
        }

        row_text(canvas, editor, (m, text), (font, shade));
    }

    if editor.library.mods.is_empty() {
        put(
            canvas,
            font,
            row_band(rows, 0),
            &[(DIM, NONE_FOUND.to_owned())],
            Side::Centre,
        );
    }

    pressed
}

/// Whether text is drawn in its colours or greyed: loading off.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Shade {
    Lit,
    Grey,
}

impl Shade {
    /// `colour`, or grey.
    const fn or(self, colour: Color32) -> Color32 {
        match self {
            Shade::Lit => colour,
            Shade::Grey => GREY,
        }
    }
}

/// A row's name and short description from `rect`'s left, cut to its
/// room, and its note at its right: a lobby player's problem, a refusal,
/// Never loads, Built in. The name accent while picked, red where a
/// player lacks it. A refusal's reason on hover.
fn row_text(
    canvas: &mut Canvas<'_>,
    editor: &Editor,
    (m, rect): (&Mod, Rect),
    (font, shade): (Font, Shade),
) {
    let ctx = canvas.ui.ctx().clone();
    let scale = canvas.fit.scale;
    let measure = |s: &str| font.width(&ctx, scale, s);
    let problems = editor.problems.get(&m.guid);
    let refused = match editor.builds.get(&m.guid) {
        Some(Built::Refused(why)) => Some(why.as_str()),
        _ => None,
    };
    let notes: Vec<(Color32, String)> = [
        problems.map(|lines| (PROBLEM, lines.join(", "))),
        refused.map(|_| (PROBLEM, REFUSED.to_owned())),
        never_loads(&m.base).then(|| (PROBLEM, NEVER_LOADS.to_owned())),
        crate::files::builtin::holds(m.guid).then(|| (DIM, BUILT_IN.to_owned())),
    ]
    .into_iter()
    .flatten()
    .map(|(c, s)| (shade.or(c), s))
    .collect();
    let note: Vec<(Color32, String)> = notes
        .iter()
        .enumerate()
        .flat_map(|(n, part)| {
            let gap = (n > 0).then(|| (shade.or(DIM), NOTE_GAP.to_owned()));

            gap.into_iter().chain([part.clone()])
        })
        .collect();
    let said: String = note.iter().map(|(_, s)| s.as_str()).collect();
    let side = if said.is_empty() {
        0.0
    } else {
        measure(&said) + NOTE_ROOM
    };

    if !said.is_empty() {
        let at = Rect::from_min_max(
            pos2(rect.right() - side, rect.top()),
            rect.max - vec2(NOTE_INSET, 0.0),
        );

        put(canvas, font, at, &note, Side::Right);

        if let Some(why) = refused {
            canvas
                .ui
                .interact(
                    canvas.fit.rect(at),
                    canvas.ui.id().with(("refused", m.guid)),
                    Sense::hover(),
                )
                .on_hover_text(why);
        }
    }

    let room = rect.width() - side - NAME_GAP;
    let name_colour = match (problems, editor.picked == Some(m.guid)) {
        (Some(_), _) => PROBLEM,
        (None, true) => ACCENT,
        (None, false) => WHITE,
    };
    let name = font::ellipsised(&[(shade.or(name_colour), m.title())], room, &measure);
    let name_wide = measure(&joined(&name));
    let short = m.short.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut runs = if measure(&m.title()) <= room {
        vec![(shade.or(name_colour), m.title())]
    } else {
        name
    };

    // The short description where there is room for some of it.
    if !short.is_empty() && name_wide < room - SHORT_LEAST {
        let left = room - name_wide - SHORT_ROOM;
        let gap = (shade.or(DIM), SHORT_GAP.to_owned());

        runs.push(gap);

        if measure(&short) <= left {
            runs.push((shade.or(DIM), short));
        } else {
            runs.extend(font::ellipsised(&[(shade.or(DIM), short)], left, &measure));
        }
    }

    put(canvas, font, rect, &runs, Side::Left);
}

/// The picked mod's details in the DESCRIPTION box, scrolled by the
/// wheel and its bar: name, long description, the games it works with,
/// quests, files, id, folder (its path on hover).
fn details(canvas: &mut Canvas<'_>, editor: &mut Editor, font: Font) {
    let ctx = canvas.ui.ctx().clone();
    let scale = canvas.fit.scale;
    let shown = Rect::from_min_size(
        DETAILS.min + DETAILS_TEXT_AT,
        DETAILS.size() - DETAILS_TEXT_LESS,
    );
    let paragraphs = about(editor);
    let lines: Vec<(usize, Vec<(Color32, String)>)> = paragraphs
        .iter()
        .enumerate()
        .flat_map(|(n, (colour, text))| {
            font.wrapped(&ctx, scale, (text, *colour), shown.width())
                .into_iter()
                .map(move |(_, parts)| (n, parts))
        })
        .collect();
    let (pitch, cap) = (font.pitch, font.cap(scale));

    #[allow(clippy::cast_precision_loss, reason = "a box's lines")]
    let most = crate::ui::scroll::most(lines.len() as f32 * pitch, shown.height());

    text_box(canvas, DETAILS, Some(DESCRIPTION), font);
    canvas.wheeled(
        canvas.fit.rect(DETAILS),
        "mod details",
        (&mut editor.about, most),
    );

    let bar = bar_control(
        DETAILS_BAR_ID,
        Rect::from_min_size(DETAILS.right_top() + DETAILS_BAR_AT, DETAILS_BAR),
    );
    let moved = canvas.scroll_bar(Pack::Base, &bar, (editor.about, most, pitch));

    editor.about = (editor.about + moved).clamp(0.0, most);

    let clip = Rect::from_x_y_ranges(
        DETAILS.left()..=DETAILS.right() - DETAILS_BAR_ROOM,
        DETAILS.top() + EDGE..=DETAILS.bottom() - EDGE,
    );
    let painter = canvas.painter().with_clip_rect(canvas.fit.rect(clip));
    let folder = paragraphs
        .len()
        .checked_sub(1)
        .filter(|_| editor.picked.is_some());

    for (n, (paragraph, parts)) in lines.iter().enumerate() {
        #[allow(clippy::cast_precision_loss, reason = "a box's lines")]
        let top = shown.top() + n as f32 * pitch - editor.about;

        font.draw(
            &painter,
            canvas.fit,
            pos2(shown.left(), top + f32::midpoint(pitch, cap)),
            parts,
        );

        // The folder's line: its path on hover.
        let path = editor
            .picked
            .and_then(|g| editor.library.find(g))
            .map(|m| m.package.display().to_string());

        if let (true, Some(path)) = (folder == Some(*paragraph), path) {
            let line = Rect::from_min_size(pos2(shown.left(), top), vec2(shown.width(), pitch))
                .intersect(clip);

            if line.is_positive() {
                canvas
                    .ui
                    .interact(
                        canvas.fit.rect(line),
                        canvas.ui.id().with(("mod folder", n)),
                        Sense::hover(),
                    )
                    .on_hover_text(path);
            }
        }
    }
}

/// What the details say, a paragraph each in its colour: nothing picked,
/// a hint.
fn about(editor: &Editor) -> Vec<(Color32, String)> {
    let picked = editor.picked.and_then(|g| editor.library.find(g));

    match picked {
        None => vec![(DIM, PICK_HINT.to_owned())],
        Some(m) => {
            let long = if m.long.is_empty() { &m.short } else { &m.long };
            let brings = Brings::of(&editor.library, m);

            // An enhancement: GPL source compiled where HD needs the Bcd
            // (`compiled`).
            let (built, refused) = match editor.builds.get(&m.guid) {
                Some(Built::Compiled { image, .. }) => (
                    format!(" · GPL: compiled from source, images {image}"),
                    None,
                ),
                Some(Built::Refused(why)) => (String::new(), Some(why)),
                None => (String::new(), None),
            };

            [
                Some((ACCENT, m.title())),
                (!long.is_empty()).then(|| (WHITE, long.clone())),
                Some((WHITE, format!("Works with: {}", works_with(&m.base)))),
                brings.said().map(|s| (WHITE, s)),
                Some((DIM, format!("Files: {}{built}", holds(m)))),
                refused.map(|why| {
                    (
                        PROBLEM,
                        format!("{REFUSED}: its GPL source does not compile: {why}"),
                    )
                }),
                Some((DIM, format!("Id: {} · Folder: {}", m.guid, folder_said(m)))),
            ]
            .into_iter()
            .flatten()
            .collect()
        }
    }
}

/// The foot: the folders searched and the mods passed over or missing
/// (the lists on hover), LOAD LIST… (greyed with loading off), SAVE
/// LIST…, RESCAN; DONE and CANCEL on the frame's bottom.
fn tail(canvas: &mut Canvas<'_>, editor: &Editor, draft: &List, look: Look<'_>) -> Option<Act> {
    let font = look.fonts.body;

    sources(canvas, editor, &draft.kept, font);

    let shown = [
        (LOAD_LIST, while_loading(draft.loading), Act::Load),
        (SAVE_LIST, Able::Enabled, Act::Save),
        (RESCAN, Able::Enabled, Act::Rescan),
    ];
    let small = shown
        .into_iter()
        .enumerate()
        .fold(None, |pressed, (n, (label, able, act))| {
            #[allow(clippy::cast_precision_loss, reason = "three buttons")]
            let at = pos2(SMALL_LEFT + n as f32 * SMALL_STEP, TAIL_TOP);
            let response = labelled(canvas, (SMALL, at), (label, look), able);
            let response = match act {
                Act::Rescan => response.on_hover_text(RESCAN_SAYS),
                _ => response,
            };

            pressed.or(tapped(&response).then_some(act))
        });
    let closing = [
        (DONE, CLOSE_LEFT, Act::Done),
        (
            look.cancel,
            PAGE.x - CLOSE_LEFT - BACKING_WIDTH,
            Act::Cancel,
        ),
    ];
    let closed = closing.into_iter().fold(None, |pressed, (label, x, act)| {
        let at = pos2(x, PAGE.y - CLOSE_UP);

        canvas.picture(Pack::Expansion, BACKING, at);

        let response = labelled(canvas, (BIG, at + BACKING_IN), (label, look), Able::Enabled);

        pressed.or(tapped(&response).then_some(act))
    });

    small.or(closed)
}

/// The folders searched, the mods passed over and those missing: a
/// count each, the list on hover. A missing mod enabled is a problem.
fn sources(canvas: &mut Canvas<'_>, editor: &Editor, kept: &[Entry], font: Font) {
    let folders: Vec<String> = editor
        .folders
        .iter()
        .map(|f| f.display().to_string())
        .collect();
    let skipped = &editor.library.skipped;
    let missing: Vec<String> = kept
        .iter()
        .map(|e| {
            let state = if e.enabled { "enabled" } else { "disabled" };

            format!("{} ({state})", e.label())
        })
        .collect();
    let n = folders.len();
    let missing_colour = if kept.iter().any(|e| e.enabled) {
        PROBLEM
    } else {
        DIM
    };
    let parts = [
        Some((
            DIM,
            format!("{n} {} searched", plural(n, "folder")),
            folders.join("\n"),
        )),
        (!skipped.is_empty()).then(|| {
            (
                DIM,
                format!(" · {} skipped", skipped.len()),
                skipped.join("\n"),
            )
        }),
        (!missing.is_empty()).then(|| {
            (
                missing_colour,
                format!(" · {} missing", missing.len()),
                format!("{MISSING_SAYS}\n{}", missing.join("\n")),
            )
        }),
    ];
    let ctx = canvas.ui.ctx().clone();
    let scale = canvas.fit.scale;
    let mut x = SOURCES_AT.x;

    for (colour, text, tip) in parts.into_iter().flatten() {
        let wide = font.width(&ctx, scale, &text);
        let rect = Rect::from_min_size(pos2(x, SOURCES_AT.y), vec2(wide, ROW));

        put(canvas, font, rect, &[(colour, text.clone())], Side::Left);
        canvas
            .ui
            .interact(
                canvas.fit.rect(rect),
                canvas.ui.id().with(("mod sources", text)),
                Sense::hover(),
            )
            .on_hover_text(tip);
        x += wide;
    }
}

/// The dialog `editor.asked` over the page, if any, centred on it at its
/// scale: Load's and Save's in the page's frame art, Overwrite's and
/// Delete's questions short `AP11` boxes over them. Its press.
fn dialog(
    canvas: &mut Canvas<'_>,
    editor: &mut Editor,
    (look, message): (Look<'_>, &Layout),
) -> Option<Reply> {
    let asked = editor.asked.clone()?;
    let window = canvas.ui.max_rect();
    let kept = canvas.fit;
    let child = canvas
        .ui
        .new_child(egui::UiBuilder::new().id_salt("mods dialog"));
    let under = std::mem::replace(&mut canvas.ui, child);

    canvas.fit = Fit::centred(window, kept.scale, DIALOG);
    canvas.block(window);

    let reply = match asked {
        Asked::Load => loader(canvas, editor, (look, Lid::Open)),
        Asked::Save(mut name) => {
            let reply = saver(canvas, editor, (&mut name, look, Lid::Open));

            if let Some(Asked::Save(typed)) = &mut editor.asked {
                *typed = name;
            }

            reply
        }
        Asked::Overwrite(name) => {
            saver(canvas, editor, (&mut name.clone(), look, Lid::Covered));
            question(
                canvas,
                message,
                (OVERWRITE_TITLE, &format!("Overwrite \"{name}\"?")),
            )
        }
        Asked::Delete(name) => {
            loader(canvas, editor, (look, Lid::Covered));
            question(
                canvas,
                message,
                (DELETE_TITLE, &format!("Delete \"{name}\"?")),
            )
        }
    };

    canvas.ui = under;
    canvas.fit = kept;
    reply
}

/// Whether a dialog takes presses, or shows under a question.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Lid {
    Open,
    Covered,
}

/// A short `AP11` question over a dialog: YES or NO.
fn question(
    canvas: &mut Canvas<'_>,
    message: &Layout,
    (title, text): (&str, &str),
) -> Option<Reply> {
    match paint::short(canvas, message, (title, text), Ask::Question) {
        Some(message::YES) => Some(Reply::Yes),
        Some(message::NO) => Some(Reply::No),
        _ => None,
    }
}

/// A press on a saved list's row, by its place.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RowPress {
    Once(usize),
    Twice(usize),
}

impl RowPress {
    /// The list Load's dialog loads: one double clicked.
    const fn loads(self) -> Option<usize> {
        match self {
            RowPress::Twice(i) => Some(i),
            RowPress::Once(_) => None,
        }
    }

    /// The list whose name Save's dialog takes: one clicked.
    const fn names(self) -> usize {
        match self {
            RowPress::Once(i) | RowPress::Twice(i) => i,
        }
    }
}

/// Load's dialog: the saved lists, the picked one's mods under them;
/// LOAD and DELETE act on the picked, CANCEL closes. A double click
/// loads.
fn loader(
    canvas: &mut Canvas<'_>,
    editor: &mut Editor,
    (look, lid): (Look<'_>, Lid),
) -> Option<Reply> {
    let font = look.fonts.body;

    dialog_frame(canvas, LOAD_TITLE, look);

    let (rows, bar) = list_box(canvas, LOAD_WELL, LISTS_HEADING, font);
    let chose = collections(canvas, editor, (rows, bar), font);
    let picked = editor.list_lit.and_then(|i| editor.listed.get(i)).cloned();

    text_box(canvas, LOAD_LINE, None, font);

    let line = match editor.list_lit.and_then(|i| editor.contents.get(i)) {
        Some(entries) => contents_said(&editor.library, entries),
        None => vec![(DIM, NONE_SAVED.to_owned())],
    };
    let line = cut_to(canvas, font, &line, LOAD_LINE.width() - LINE_MARGIN);

    put(canvas, font, LOAD_LINE, &line, Side::Centre);

    let able = with(picked.as_ref());
    let shown = [(LOAD, able), (DELETE, able), (look.cancel, Able::Enabled)];
    let pressed = row_buttons(canvas, &shown, look);

    match (lid, pressed, picked) {
        (Lid::Covered, ..) => None,
        (Lid::Open, Some(0), Some(name)) => Some(Reply::Pick(name)),
        (Lid::Open, Some(1), Some(name)) => Some(Reply::Delete(name)),
        (Lid::Open, Some(2), _) => Some(Reply::No),
        (Lid::Open, _, _) => chose
            .and_then(RowPress::loads)
            .and_then(|i| editor.listed.get(i).cloned())
            .map(Reply::Pick),
    }
}

/// Save's dialog: a name typed, or a saved list's taken by a click; the
/// page's list under it; SAVE and CANCEL. Under a question the name is
/// shown, not edited.
fn saver(
    canvas: &mut Canvas<'_>,
    editor: &mut Editor,
    (name, look, lid): (&mut String, Look<'_>, Lid),
) -> Option<Reply> {
    let font = look.fonts.body;

    dialog_frame(canvas, SAVE_TITLE, look);
    text_box(canvas, SAVE_FIELD, None, font);
    put(
        canvas,
        font,
        Rect::from_min_size(
            SAVE_FIELD.min + NAME_LABEL_AT,
            vec2(NAME_LABEL_WIDTH, SAVE_FIELD.height()),
        ),
        &[(HEAD, NAME_LABEL.to_owned())],
        Side::Left,
    );

    let typing = Rect::from_min_max(
        SAVE_FIELD.min + NAME_AT,
        SAVE_FIELD.max - vec2(NAME_END, 0.0),
    );

    match lid {
        Lid::Open => {
            let response = canvas.typed(
                (typing, Some(SMALL_FONT), WHITE),
                (name, usize::MAX, egui::Align::LEFT),
                ("mod list name", Inset::Edges),
            );

            if let Some(response) = response.filter(|r| !r.has_focus()) {
                response.request_focus();
            }
        }
        Lid::Covered => put(canvas, font, typing, &[(WHITE, name.clone())], Side::Left),
    }

    // The list named as typed lit.
    editor.list_lit = editor.listed.iter().position(|l| l == name);

    let (rows, bar) = list_box(canvas, SAVE_WELL, LISTS_HEADING, font);
    let taken = collections(canvas, editor, (rows, bar), font)
        .map(RowPress::names)
        .and_then(|i| editor.listed.get(i));

    if let (Lid::Open, Some(taken)) = (lid, taken) {
        name.clone_from(taken);
    }

    let entries: Vec<Entry> = editor.draft.as_ref().map_or_default(|d| d.rows.clone());
    let line: Vec<(Color32, String)> = std::iter::once((DIM, THIS_LIST.to_owned()))
        .chain(contents_said(&editor.library, &entries))
        .collect();
    let line = cut_to(canvas, font, &line, DIALOG.x - 2.0 * SAVE_LINE_AT.x);

    put(
        canvas,
        font,
        Rect::from_min_size(SAVE_LINE_AT, vec2(DIALOG.x, ROW)),
        &line,
        Side::Left,
    );

    let fits = with(mod_list::list_name(name).as_ref());
    let pressed = row_buttons(canvas, &[(STORE, fits), (look.cancel, Able::Enabled)], look);

    match (lid, pressed) {
        (Lid::Open, Some(0)) => Some(Reply::Store(name.clone())),
        (Lid::Open, Some(1)) => Some(Reply::No),
        _ => None,
    }
}

/// A dialog's ground, frame, pog and title.
fn dialog_frame(canvas: &mut Canvas<'_>, title_text: &str, look: Look<'_>) {
    canvas.tiled(
        Pack::Expansion,
        GROUND,
        Rect::from_min_size(Pos2::ZERO, DIALOG),
    );
    frame(canvas, DIALOG);
    title(canvas, (title_text, DIALOG.x), look.fonts.title);
}

/// The saved lists in the list's room `rows`, a count of each one's
/// enabled mods at its right, the lit one accent; its bar at `bar`. The
/// row pressed, now lit: once, or twice.
fn collections(
    canvas: &mut Canvas<'_>,
    editor: &mut Editor,
    (rows, bar): (Rect, Rect),
    font: Font,
) -> Option<RowPress> {
    let shown = shown_rows(rows.height());

    scroll_list(
        canvas,
        (bar, rows),
        (&mut editor.list_scroll, editor.listed.len(), shown),
        "mod lists",
    );

    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a list's rows"
    )]
    let first = editor.list_scroll.round() as usize;
    let mut chose = None;

    for (n, (i, name)) in editor
        .listed
        .iter()
        .enumerate()
        .skip(first)
        .take(shown)
        .enumerate()
    {
        let band = row_band(rows, n);
        let lit = editor.list_lit == Some(i);
        let response = canvas.ui.interact(
            canvas.fit.rect(band),
            canvas.ui.id().with(("mod list row", i)),
            Sense::click(),
        );

        if response.double_clicked() {
            chose = Some(RowPress::Twice(i));
        } else if tapped(&response) {
            editor.list_lit = Some(i);
            chose = Some(RowPress::Once(i));
        }

        if lit {
            canvas
                .painter()
                .rect_filled(canvas.fit.rect(band), 0.0, LIT);
        }

        let colour = if lit { ACCENT } else { WHITE };
        let enabled = editor
            .contents
            .get(i)
            .map_or(0, |e| e.iter().filter(|e| e.enabled).count());
        let name_at = Rect::from_min_max(band.min + vec2(LIST_NAME_IN, 0.0), band.max);
        let count_at = Rect::from_min_max(band.min, band.max - vec2(COUNT_IN, 0.0));

        put(canvas, font, name_at, &[(colour, name.clone())], Side::Left);
        put(
            canvas,
            font,
            count_at,
            &[(DIM, counted(enabled))],
            Side::Right,
        );
    }

    chose
}

/// `labels` on `DEM2` 1006, centred on a dialog's bottom bar, each as
/// able as it says; the one pressed, by its place.
fn row_buttons(canvas: &mut Canvas<'_>, labels: &[(&str, Able)], look: Look<'_>) -> Option<usize> {
    #[allow(clippy::cast_precision_loss, reason = "a few buttons")]
    let count = labels.len() as f32;
    let left = (DIALOG.x - count * SMALL_SIZE.x - (count - 1.0) * DIALOG_GAP) / 2.0;

    labels
        .iter()
        .enumerate()
        .fold(None, |pressed, (n, &(label, able))| {
            #[allow(clippy::cast_precision_loss, reason = "a few buttons")]
            let at = pos2(
                left + n as f32 * (SMALL_SIZE.x + DIALOG_GAP),
                DIALOG.y - DIALOG_BUTTONS_UP,
            );
            let response = labelled(canvas, (SMALL, at), (label, look), able);

            pressed.or(tapped(&response).then_some(n))
        })
}

/// A place along a box's side: so far from its start, its end, or its
/// middle.
#[derive(Clone, Copy)]
enum Along {
    Start(f32),
    End(f32),
    Middle(f32),
}

impl Along {
    /// Where that is on a side `long`.
    fn at(self, long: f32) -> f32 {
        match self {
            Along::Start(d) => d,
            Along::End(d) => long + d,
            Along::Middle(d) => (long / 2.0).floor() + d,
        }
    }
}

/// How far a repeated piece runs: so long, or its box's side less this.
#[derive(Clone, Copy)]
enum Run {
    Fixed(f32),
    Less(f32),
}

impl Run {
    /// Its length on a side `long`.
    const fn of(self, long: f32) -> f32 {
        match self {
            Run::Fixed(d) => d,
            Run::Less(d) => long - d,
        }
    }
}

/// One picture of a box's edge, placed about the box: where its top left
/// is, and how far it repeats; `None` drawn once.
#[derive(Clone, Copy)]
struct Piece {
    set: i32,
    at: (Along, Along),
    tiled: Option<(Run, Run)>,
}

/// A piece drawn once.
const fn once(set: i32, x: Along, y: Along) -> Piece {
    Piece {
        set,
        at: (x, y),
        tiled: None,
    }
}

/// A piece repeated over `wide` × `tall`.
const fn tiled(set: i32, (x, y): (Along, Along), (wide, tall): (Run, Run)) -> Piece {
    Piece {
        set,
        at: (x, y),
        tiled: Some((wide, tall)),
    }
}

/// `pieces` of `record` about layout rectangle `rect`, in order.
fn pieces(canvas: &mut Canvas<'_>, record: FourCc, rect: Rect, pieces: &[Piece]) {
    let (w, h) = (rect.width(), rect.height());

    for piece in pieces {
        let pic = Pic::of(record, piece.set);
        let at = rect.min + vec2(piece.at.0.at(w), piece.at.1.at(h));

        match piece.tiled {
            Some((wide, tall)) => {
                let span = Rect::from_min_size(at, vec2(wide.of(w), tall.of(h)));

                canvas.tiled(Pack::Expansion, pic, span);
            }
            None => {
                canvas.picture(Pack::Expansion, pic, at);
            }
        }
    }
}

/// A list's edges about its well: `APdc`'s places for them about its
/// left list's well, (37, 124) 331 × 242, the sides run to the corners.
const LIST_EDGES: [Piece; 8] = [
    once(1021, Along::Start(-15.0), Along::Start(-31.0)),
    tiled(
        1022,
        (Along::Start(30.0), Along::Start(-20.0)),
        (Run::Less(47.0), Run::Fixed(23.0)),
    ),
    once(1024, Along::End(-17.0), Along::Start(-31.0)),
    tiled(
        1025,
        (Along::End(-3.0), Along::Start(53.0)),
        (Run::Fixed(26.0), Run::Less(101.0)),
    ),
    once(1026, Along::End(-3.0), Along::End(-48.0)),
    tiled(
        1027,
        (Along::Start(10.0), Along::End(-5.0)),
        (Run::Less(13.0), Run::Fixed(11.0)),
    ),
    once(1028, Along::Start(-12.0), Along::End(-8.0)),
    tiled(
        1029,
        (Along::Start(-6.0), Along::Start(13.0)),
        (Run::Fixed(9.0), Run::Less(21.0)),
    ),
];

/// A list's heading plate, `INx1` 1023: its middle right of the well's,
/// its top above the well's; its words' box in from its left and top,
/// narrower by `LIST_WORDS_LESS`. `APdc`'s, its plate (74, 89) on that
/// well; the words' box openregent's own.
const LIST_PLATE: i32 = 1023;
const LIST_PLATE_RIGHT: f32 = 5.5;
const LIST_PLATE_UP: f32 = 35.0;
const LIST_WORDS_AT: Vec2 = vec2(24.0, 5.0);
const LIST_WORDS_LESS: f32 = 48.0;
const PLATE_WORDS_HEIGHT: f32 = 20.0;

/// A list's rows' room inside its well: in from its left and top, its
/// right and bottom. openregent's own.
const ROWS_IN: Vec2 = vec2(5.0, 4.0);
const ROWS_END: Vec2 = vec2(3.0, 5.0);

/// A list's bar: right of the well and down from its top, shorter than
/// the well by `LIST_BAR_LESS`. `APdc`'s 9100, (371, 135) 25 × 225 on
/// that well.
const LIST_BAR_AT: Vec2 = vec2(3.0, 11.0);
const LIST_BAR_LESS: f32 = 17.0;

/// A list as `APdc` draws its lists: the well `INx1` 1030 over `well`,
/// its edges 1021-1029, the heading's plate 1023 centred on its top. The
/// rows' room, and the scroll bar's.
fn list_box(canvas: &mut Canvas<'_>, well: Rect, heading: &str, font: Font) -> (Rect, Rect) {
    let plate_pic = Pic::of(BOXES, LIST_PLATE);

    canvas.tiled(Pack::Expansion, Pic::of(BOXES, WELL), well);
    pieces(canvas, BOXES, well, &LIST_EDGES);

    if let Some(plate) = canvas.size(Pack::Expansion, plate_pic) {
        let at = pos2(
            well.center().x + LIST_PLATE_RIGHT - plate.x / 2.0,
            well.top() - LIST_PLATE_UP,
        );
        let words = Rect::from_min_size(
            at + LIST_WORDS_AT,
            vec2(plate.x - LIST_WORDS_LESS, PLATE_WORDS_HEIGHT),
        );

        canvas.picture(Pack::Expansion, plate_pic, at);
        put(
            canvas,
            font,
            words,
            &[(HEAD, heading.to_owned())],
            Side::Centre,
        );
    }

    (
        Rect::from_min_max(well.min + ROWS_IN, well.max - ROWS_END),
        Rect::from_min_size(
            well.right_top() + LIST_BAR_AT,
            vec2(BAR_WIDTH, well.height() - LIST_BAR_LESS),
        ),
    )
}

/// A text box's edges about it: `APdc`'s places for them about its
/// DESCRIPTION box, (33, 428) 732 × 98.
const TEXT_EDGES: [Piece; 8] = [
    once(1031, Along::Start(-7.0), Along::Start(-10.0)),
    tiled(
        1032,
        (Along::Start(14.0), Along::Start(-2.0)),
        (Run::Less(26.0), Run::Fixed(4.0)),
    ),
    once(1034, Along::End(-12.0), Along::Start(-10.0)),
    tiled(
        1035,
        (Along::End(-2.0), Along::Start(11.0)),
        (Run::Fixed(5.0), Run::Less(23.0)),
    ),
    once(1036, Along::End(-12.0), Along::End(-12.0)),
    tiled(
        1037,
        (Along::Start(14.0), Along::End(-3.0)),
        (Run::Less(26.0), Run::Fixed(4.0)),
    ),
    once(1038, Along::Start(-7.0), Along::End(-13.0)),
    tiled(
        1039,
        (Along::Start(0.0), Along::Start(12.0)),
        (Run::Fixed(4.0), Run::Less(25.0)),
    ),
];

/// A text box's heading plate, `INx1` 1033: its middle right of the
/// box's, its top above the box's (`APdc`'s, (267, 412) on that box);
/// its words' box in from its left and top, and its width (openregent's
/// own).
const TEXT_PLATE: i32 = 1033;
const TEXT_PLATE_RIGHT: f32 = 1.0;
const TEXT_PLATE_UP: f32 = 16.0;
const TEXT_WORDS_AT: Vec2 = vec2(23.0, 6.0);
const TEXT_WORDS_WIDTH: f32 = 220.0;

/// `APdc`'s DESCRIPTION box over `rect`: the well 1030, its edges and
/// corners 1031-1039, the heading's plate 1033 centred on its top where
/// it has a heading.
fn text_box(canvas: &mut Canvas<'_>, rect: Rect, heading: Option<&str>, font: Font) {
    let plate_pic = Pic::of(BOXES, TEXT_PLATE);

    canvas.tiled(Pack::Expansion, Pic::of(BOXES, WELL), rect);
    pieces(canvas, BOXES, rect, &TEXT_EDGES);

    if let (Some(heading), Some(plate)) = (heading, canvas.size(Pack::Expansion, plate_pic)) {
        let at = pos2(
            rect.center().x + TEXT_PLATE_RIGHT - plate.x / 2.0,
            rect.top() - TEXT_PLATE_UP,
        );
        let words = Rect::from_min_size(
            at + TEXT_WORDS_AT,
            vec2(TEXT_WORDS_WIDTH, PLATE_WORDS_HEIGHT),
        );

        canvas.picture(Pack::Expansion, plate_pic, at);
        put(
            canvas,
            font,
            words,
            &[(HEAD, heading.to_owned())],
            Side::Centre,
        );
    }
}

/// The frame's bars, `APdc`'s 8501 and 8505: the title bar `INx2` 1001
/// from x 115 to 46 short of the right, 93 high; the bottom bar 1005
/// from x 52 to 51 short of the right, 59 up from the bottom.
const TITLE_BAR: i32 = 1001;
const TITLE_BAR_LEFT: f32 = 115.0;
const TITLE_BAR_END: f32 = 46.0;
const TITLE_BAR_HEIGHT: f32 = 93.0;
const BOTTOM_BAR: i32 = 1005;
const BOTTOM_BAR_LEFT: f32 = 52.0;
const BOTTOM_BAR_END: f32 = 51.0;
const BOTTOM_BAR_UP: f32 = 59.0;

/// The frame's sides, then its corners and the sides' middle pieces
/// over them: `APdc`'s places, 8500 to 8511, on its 800 × 600, each
/// kept to its corner or middle; the sides run between the corners.
const FRAME_PIECES: [Piece; 8] = [
    tiled(
        1003,
        (Along::End(-23.0), Along::Start(121.0)),
        (Run::Fixed(23.0), Run::Less(210.0)),
    ),
    tiled(
        1007,
        (Along::Start(0.0), Along::Start(122.0)),
        (Run::Fixed(23.0), Run::Less(212.0)),
    ),
    once(1000, Along::Start(0.0), Along::Start(0.0)),
    once(1002, Along::End(-46.0), Along::Start(0.0)),
    once(1004, Along::End(-51.0), Along::End(-89.0)),
    once(1006, Along::Start(0.0), Along::End(-90.0)),
    once(1008, Along::Start(0.0), Along::Middle(-35.0)),
    once(1009, Along::End(-16.0), Along::Middle(-37.0)),
];

/// `APdc`'s frame, `INx2` 1000-1009, on a page `size` big: its corners
/// held, its sides repeated, the bottom bar's halves held to its ends;
/// the title bar repeated, or narrower than its piece, its two ends held.
///
/// ```text
///  1000 ─ 1001 (title bar) ───── 1002
///  1007                          1003    1008, 1009 on the sides' middles
///  1006 ── 1005 ┊ 1005 ───────── 1004
/// ```
fn frame(canvas: &mut Canvas<'_>, size: Vec2) {
    let pic = |set: i32| Pic::of(FRAME, set);
    let (w, h) = (size.x, size.y);
    let title_room = Rect::from_min_max(
        pos2(TITLE_BAR_LEFT, 0.0),
        pos2(w - TITLE_BAR_END, TITLE_BAR_HEIGHT),
    );

    if let Some(bar) = canvas.size(Pack::Expansion, pic(TITLE_BAR)) {
        if title_room.width() >= bar.x {
            canvas.tiled(Pack::Expansion, pic(TITLE_BAR), title_room);
        } else {
            ends(canvas, pic(TITLE_BAR), title_room, bar.x);
        }
    }

    // The sides under the bottom bar, the corners over it.
    let (sides, corners) = FRAME_PIECES.split_at(2);
    let page = Rect::from_min_size(Pos2::ZERO, size);

    pieces(canvas, FRAME, page, sides);

    if let Some(bottom) = canvas.size(Pack::Expansion, pic(BOTTOM_BAR)) {
        let top = h - BOTTOM_BAR_UP;
        let room = Rect::from_min_max(
            pos2(BOTTOM_BAR_LEFT, top),
            pos2(w - BOTTOM_BAR_END, top + bottom.y),
        );

        ends(canvas, pic(BOTTOM_BAR), room, bottom.x);
    }

    pieces(canvas, FRAME, page, corners);
}

/// `pic`, `wide` pixels, over `room`: its left from the left edge to the
/// middle, its right from the middle to the right edge.
fn ends(canvas: &mut Canvas<'_>, pic: Pic, room: Rect, wide: f32) {
    let kept = canvas.clip;
    let middle = room.center().x.floor();
    let halves = [
        (room.min.x, room.min.x..=middle),
        (room.max.x - wide, middle..=room.max.x),
    ];

    for (left, span) in halves {
        let half = Rect::from_x_y_ranges(span, room.y_range());

        canvas.clip = Some(kept.map_or(half, |k| k.intersect(half)));
        canvas.picture(Pack::Expansion, pic, pos2(left, room.min.y));
    }

    canvas.clip = kept;
}

/// The pog, `DX00` 1000, and `text` in the title bar of a page `width`
/// wide, in `font`.
fn title(canvas: &mut Canvas<'_>, (text, width): (&str, f32), font: Font) {
    let room = Rect::from_min_max(TITLE_AT, pos2(width - TITLE_END, TITLE_AT.y + TITLE_HEIGHT));

    canvas.picture(Pack::Expansion, POG, POG_AT);
    put(canvas, font, room, &[(PALE, text.to_owned())], Side::Centre);
}

/// A press that takes clicks while mods load.
const fn while_loading(loading: Loading) -> Able {
    match loading {
        Loading::On => Able::Enabled,
        Loading::Off => Able::Disabled,
    }
}

/// A press that takes clicks where there is `what` to act on.
const fn with<T>(what: Option<&T>) -> Able {
    match what {
        Some(_) => Able::Enabled,
        None => Able::Disabled,
    }
}

/// Where a line sits in its rectangle.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    Left,
    Centre,
    Right,
}

/// `runs` on one line in layout rectangle `rect`, centred up and down,
/// set by `side`, in `font`.
fn put(canvas: &Canvas<'_>, font: Font, rect: Rect, runs: &[(Color32, String)], side: Side) {
    let scale = canvas.fit.scale;
    let wide = font.width(canvas.ui.ctx(), scale, &joined(runs));
    let x = match side {
        Side::Left => rect.left(),
        Side::Centre => rect.center().x - wide / 2.0,
        Side::Right => rect.right() - wide,
    };
    let pen = pos2(x, rect.center().y + font.cap(scale) / 2.0);

    font.draw(&canvas.painter(), canvas.fit, pen, runs);
}

/// `runs`' text.
fn joined(runs: &[(Color32, String)]) -> String {
    runs.iter().map(|(_, s)| s.as_str()).collect()
}

/// `runs` cut with an ellipsis to `width` where wider.
fn cut_to(
    canvas: &Canvas<'_>,
    font: Font,
    runs: &[(Color32, String)],
    width: f32,
) -> Vec<(Color32, String)> {
    let ctx = canvas.ui.ctx().clone();
    let scale = canvas.fit.scale;
    let measure = |s: &str| font.width(&ctx, scale, s);

    if measure(&joined(runs)) <= width {
        runs.to_vec()
    } else {
        font::ellipsised(runs, width, &measure)
    }
}

/// A press area over `pic`'s slots at `at`, told apart by `key`: slot 0
/// at rest, lit while held, its disabled slot greyed, no click taken
/// then; its response.
fn push(
    canvas: &mut Canvas<'_>,
    (pack, pic, at): (Pack, Pic, Pos2),
    key: impl std::hash::Hash + std::fmt::Debug,
    able: Able,
) -> egui::Response {
    let size = canvas.size(pack, pic).unwrap_or_default();
    let sense = match able {
        Able::Enabled => Sense::click(),
        Able::Disabled => Sense::hover(),
    };
    let response = canvas.ui.interact(
        canvas.fit.rect(Rect::from_min_size(at, size)),
        canvas.ui.id().with(("mods press", key)),
        sense,
    );
    let count = (0..art::SLOTS)
        .take_while(|&s| canvas.size(pack, pic.slot(s)).is_some())
        .count();
    let (tick, hover, press) = crate::panel::sheet::look(&response, Tick::Clear);
    let state = match able {
        Able::Enabled => State::Normal.pressed(press),
        Able::Disabled => State::Disabled,
    };

    if able == Able::Enabled {
        paint::arm(&response, Some(Kind::Button));
    }

    let slot = panels::drawn_slot((tick, hover), state, count);

    canvas.picture(pack, pic.slot(slot), at);
    response
}

/// A button, `art` at `at`, its `label` centred in `look`'s font and
/// colours by its state; its response.
fn labelled(
    canvas: &mut Canvas<'_>,
    ((record, set), at): ((FourCc, i32), Pos2),
    (label, look): (&str, Look<'_>),
    able: Able,
) -> egui::Response {
    let pic = Pic::of(record, set);
    let response = push(canvas, (Pack::Base, pic, at), ("button", label), able);
    let size = canvas.size(Pack::Base, pic).unwrap_or_default();
    let (_, _, press) = crate::panel::sheet::look(&response, Tick::Clear);
    let state = match able {
        Able::Enabled => State::Normal.pressed(press),
        Able::Disabled => State::Disabled,
    };
    // Disabled grey: the look's own is red.
    let colour = match able {
        Able::Enabled => font::ink(look.button, Role::Text, state).unwrap_or(NAVY),
        Able::Disabled => GREY,
    };
    let rect = Rect::from_min_size(at, size);

    put(
        canvas,
        look.fonts.body,
        rect,
        &[(colour, label.to_owned())],
        Side::Centre,
    );
    response
}

/// A row's box, `IX90` 1003 at `at`, ticked or not, told apart by `key`;
/// whether it was pressed.
fn tick(
    canvas: &mut Canvas<'_>,
    (at, ticked): (Pos2, Tick),
    key: impl std::hash::Hash + std::fmt::Debug,
    able: Able,
) -> bool {
    let rect = Rect::from_min_size(at, CHECK_SIZE);
    let response = canvas.ui.interact(
        canvas.fit.rect(rect),
        canvas.ui.id().with(("mods box", key)),
        Sense::click(),
    );

    canvas.picture(
        Pack::Expansion,
        CHECK.slot(usize::from(ticked == Tick::Checked)),
        at,
    );
    able == Able::Enabled && tapped(&response)
}

/// A scroll bar at `bar` over a list of `rows` rows in `list`, `shown`
/// at once: `INDs` 1000's arrows and thumb, a row a step, the wheel over
/// the list.
fn scroll_list(
    canvas: &mut Canvas<'_>,
    (bar, list): (Rect, Rect),
    (scroll, rows, shown): (&mut f32, usize, usize),
    key: &str,
) {
    let control = bar_control(LIST_BAR_ID, bar);

    canvas.list_bar(
        Pack::Base,
        (&control, Some(super::setup::whole(list))),
        (scroll, rows, shown),
        key,
    );
}

/// A scroll bar control at `rect` in `INDs` 1000, its id `id`.
fn bar_control(id: i32, rect: Rect) -> ShownControl {
    ShownControl {
        kind: Some(Kind::Scrollbar),
        id: Some(id),
        rect: Some(super::setup::whole(rect)),
        art: Some(BAR),
        ..ShownControl::default()
    }
}

/// Row `n`'s band in a list's room `rows`.
#[allow(clippy::cast_precision_loss, reason = "a list's rows")]
fn row_band(rows: Rect, n: usize) -> Rect {
    Rect::from_min_size(
        pos2(rows.left(), rows.top() + n as f32 * ROW),
        vec2(rows.width(), ROW),
    )
}

/// How many whole rows a list's room `tall` high shows.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a list's rows"
)]
fn shown_rows(tall: f32) -> usize {
    (tall / ROW).floor().max(0.0) as usize
}

/// A count of mods in words: "1 mod", "4 mods".
fn counted(n: usize) -> String {
    format!("{n} {}", plural(n, "mod"))
}

/// A list's enabled mods in words, the count dim, the names after it:
/// "4 mods: New Game+, Bug fixes". A mod no folder holds by its written
/// name.
fn contents_said(library: &Library, entries: &[Entry]) -> Vec<(Color32, String)> {
    let names: Vec<String> = entries
        .iter()
        .filter(|e| e.enabled)
        .map(|e| {
            e.guid
                .and_then(|g| library.find(g))
                .map_or_else(|| e.label(), Mod::title)
        })
        .collect();

    vec![
        (DIM, format!("{}: ", counted(names.len()))),
        (WHITE, names.join(", ")),
    ]
}

/// Each row's place in the load order, counting the enabled alone; none
/// for one disabled.
///
/// ```text
///  [x] [ ] [x]  →  1 – 2
/// ```
fn ranks(rows: &[Entry]) -> Vec<Option<usize>> {
    rows.iter()
        .scan(0, |n, e| {
            *n += usize::from(e.enabled);
            Some(e.enabled.then_some(*n))
        })
        .collect()
}

/// The quests in a mod's item folder, and the other mods in it: the
/// quests are the folder's, not one mod's, and the quest map lists them
/// whatever mods are enabled ([`Library`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Brings {
    quests: usize,
    others: usize,
}

impl Brings {
    fn of(library: &Library, m: &Mod) -> Brings {
        let quests = library.quests.iter().filter(|q| q.package == m.package);
        let others = library
            .mods
            .iter()
            .filter(|o| o.package == m.package && o.guid != m.guid);

        Brings {
            quests: quests.count(),
            others: others.count(),
        }
    }

    /// The details' line: whose quests, and where they show.
    fn said(self) -> Option<String> {
        let quests = format!("{} {}", self.quests, plural(self.quests, "quest"));
        let whose = match self.others {
            0 => format!("Brings {quests}"),
            n => format!(
                "Its folder, shared with {n} other {}, brings {quests}",
                plural(n, "mod")
            ),
        };

        (self.quests > 0)
            .then(|| format!("{whose}: on the quest map's downloadable list, enabled or not."))
    }
}

/// Whether no game loads a mod of `base`: neither dataset fits it.
const fn never_loads(base: &Base) -> bool {
    !base.fits(Datasets::Base) && !base.fits(Datasets::Expansion)
}

/// The games a mod's base loads it in, in words ([`Base::fits`]): a
/// freestyle game runs on the expansion's dataset, a quest on its own
/// (`begin`).
fn works_with(base: &Base) -> String {
    match base {
        Base::Any => "any game".to_owned(),
        Base::Expansion => "freestyle and expansion quests".to_owned(),
        Base::Majesty => "original Majesty quests only".to_owned(),
        Base::Other(text) => format!("none: unknown dataset \"{text}\""),
    }
}

/// `word`, with an s past one.
fn plural(n: usize, word: &str) -> String {
    if n == 1 {
        word.to_owned()
    } else {
        format!("{word}s")
    }
}

/// A mod's folder as the details say it: built in, else its last part.
fn folder_said(m: &Mod) -> String {
    if crate::files::builtin::holds(m.guid) {
        format!("{BUILT_IN}, unpacked to {}", folder_name(&m.package))
    } else {
        folder_name(&m.package)
    }
}

/// A folder's last part.
fn folder_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

/// What a mod's load list holds, by kind:
///
/// ```text
///  1 GPL
///  1 GPL, 13 XML descriptions, 3 XML strings
/// ```
pub fn holds(m: &Mod) -> String {
    let lists = &m.load;
    let kinds = [
        (lists.archives.len(), "CAM"),
        (lists.programs.len(), "GPL"),
        (lists.constants.len(), "constants"),
        (lists.descriptions.len(), "XML descriptions"),
        (lists.strings.len(), "XML strings"),
    ];
    let listed: Vec<String> = kinds
        .iter()
        .filter(|(n, _)| *n > 0)
        .map(|(n, kind)| format!("{n} {kind}"))
        .collect();

    if listed.is_empty() {
        "nothing".to_owned()
    } else {
        listed.join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use regent_core::data::guid::Guid;

    fn entry(guid: &str, enabled: bool) -> Entry {
        Entry::of(Guid::parse(guid).expect("a GUID"), enabled)
    }

    const A: &str = "{00000000-0000-0000-0000-00000000000A}";
    const B: &str = "{00000000-0000-0000-0000-00000000000B}";

    #[test]
    fn an_edit_moves_or_toggles() {
        let mut list = vec![entry(A, false), entry(B, true)];

        apply(&mut list, Edit::Up(1));
        assert_eq!(list, [entry(B, true), entry(A, false)]);

        apply(&mut list, Edit::Up(0));
        apply(&mut list, Edit::Down(1));
        assert_eq!(list, [entry(B, true), entry(A, false)], "the ends hold");

        apply(&mut list, Edit::Toggle(1));
        assert_eq!(list, [entry(B, true), entry(A, true)]);
    }

    /// The SDK's Adjust Guardhouse holds one image; the fixture's
    /// Hearth's XML is said.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_mod_says_what_it_holds() {
        let install = crate::testdata::fixture_install();
        let library = install.mods();
        let by = |guid: &str| {
            library
                .find(Guid::parse(guid).expect("a GUID"))
                .expect("found")
        };

        assert_eq!(holds(by("{D666FCBE-9906-4582-A6A0-DC96F812441E}")), "1 GPL");
        assert!(holds(by(crate::testdata::HEARTH)).contains("XML descriptions"));
    }

    /// The page's list shows eleven whole rows; Load's eight, Save's six.
    #[test]
    fn a_list_shows_its_whole_rows() {
        let room = |well: Rect| well.height() - 9.0;

        assert_eq!(shown_rows(room(LIST_WELL)), 11);
        assert_eq!(shown_rows(room(LOAD_WELL)), 8);
        assert_eq!(shown_rows(room(SAVE_WELL)), 6);
        assert_eq!(row_band(LIST_WELL, 2).top(), LIST_WELL.top() + 2.0 * ROW);
    }

    /// A saved list is said by its enabled mods: their count, then
    /// their names; one no folder holds by its written name, its id.
    #[test]
    fn a_saved_list_says_its_enabled_mods() {
        let said = contents_said(&Library::default(), &[entry(A, true), entry(B, false)]);

        assert_eq!(said[0], (DIM, "1 mod: ".to_owned()));
        assert_eq!(said[1].1, Guid::parse(A).expect("a GUID").to_string());
        assert_eq!(counted(4), "4 mods");
    }

    /// One click takes a saved list's name in Save's dialog; Load's
    /// loads on a double click alone.
    #[test]
    fn a_click_names_and_a_double_click_loads() {
        assert_eq!(RowPress::Once(2).names(), 2);
        assert_eq!(RowPress::Twice(2).names(), 2);
        assert_eq!(RowPress::Once(2).loads(), None);
        assert_eq!(RowPress::Twice(2).loads(), Some(2));
    }

    /// `--menu mods:…,overwrite` shows Overwrite's question.
    #[test]
    fn a_held_page_shows_its_question() {
        let mut editor = Editor::default();

        editor.open_lobby(&[], Loading::On);
        editor.held("overwrite");
        assert_eq!(editor.asked, Some(Asked::Overwrite(String::new())));
    }

    /// Numbers for the enabled alone, in order.
    #[test]
    fn only_enabled_mods_are_numbered() {
        let list = [entry(A, true), entry(B, false), entry(A, true)];

        assert_eq!(ranks(&list), [Some(1), None, Some(2)]);
    }

    /// The words agree with `Base::fits`: freestyle runs on the
    /// expansion's dataset; an unknown base fits neither.
    #[test]
    fn a_base_is_said_as_the_games_it_fits() {
        let freestyle = Datasets::Expansion;

        assert!(Base::Expansion.fits(freestyle));
        assert!(works_with(&Base::Expansion).starts_with("freestyle"));
        assert!(!Base::Majesty.fits(freestyle));
        assert!(works_with(&Base::Majesty).ends_with("only"));
        assert!(!never_loads(&Base::Any));
        assert!(never_loads(&Base::Other("Northland".to_owned())));
    }

    /// The fixture's Hearth item holds 2 quests and 2 other mods: the
    /// quests are said as the folder's. Thornwood Rules is alone in its
    /// item with 1 quest; Adjust Guardhouse has none.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_mods_quests_are_its_folders() {
        let install = crate::testdata::fixture_install();
        let library = install.mods();
        let of = |guid: &str| {
            Brings::of(
                &library,
                library
                    .find(Guid::parse(guid).expect("a GUID"))
                    .expect("found"),
            )
        };
        let hearth = of(HEARTH);
        let rules = of(crate::testdata::THORNWOOD_RULES);
        let guardhouse = of("{D666FCBE-9906-4582-A6A0-DC96F812441E}");

        assert_eq!(
            hearth,
            Brings {
                quests: 2,
                others: 2
            }
        );
        assert!(
            hearth
                .said()
                .expect("said")
                .contains("shared with 2 other mods")
        );
        assert!(rules.said().expect("said").starts_with("Brings 1 quest:"));
        assert_eq!(guardhouse.said(), None);
    }

    const HEARTH: &str = crate::testdata::HEARTH;
    const UNKNOWN: &str = "{00000000-0000-0000-0000-000000000001}";

    /// An editor on the fixture mods, its settings file and collections
    /// in a fresh scratch folder.
    fn editor(name: &str) -> (crate::testdata::Scratch, Editor) {
        let install = crate::testdata::fixture_install();
        let dir = crate::testdata::scratch(&format!("mods-page-{name}"));
        let mut editor = Editor::default();

        editor.found((install.mods().clone(), install.builds()), Vec::new());
        editor.list_file = Some(dir.join("mods.toml"));
        editor.lists = Some(dir.join(mod_list::LISTS));
        (dir, editor)
    }

    fn rows(editor: &Editor) -> &[Entry] {
        &editor.draft.as_ref().expect("open").rows
    }

    /// The built-in mod is listed, marked, off until ticked.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn the_page_lists_the_builtin_mod() {
        let (_dir, mut editor) = editor("builtin");
        let guid = Guid::parse(crate::cli::tests::HOUSE_MEND_MOD).expect("a GUID");

        editor.open(&[], Loading::On);

        let row = rows(&editor)
            .iter()
            .find(|e| e.guid == Some(guid))
            .expect("listed");
        let m = editor.library.find(guid).expect("found");

        assert!(!row.enabled);
        assert!(folder_said(m).starts_with(BUILT_IN));
        assert_eq!(m.title(), "Peasant houses mend themselves");
    }

    /// Rescan asks once; the mods found again keep the list's edits, a
    /// mod gone kept unshown and one back listed last, its flag kept.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_rescan_keeps_the_edits() {
        let (_dir, mut editor) = editor("rescan");
        let guid = Guid::parse(crate::cli::tests::HOUSE_MEND_MOD).expect("a GUID");
        let full = editor.library.clone();
        let at = |editor: &Editor| rows(editor).iter().position(|e| e.guid == Some(guid));

        editor.open(&[], Loading::On);
        editor.act(Act::Rescan);

        assert!(editor.take_rescan());
        assert!(!editor.take_rescan());

        let first = at(&editor).expect("listed");

        editor.edit(Edit::Toggle(first));

        let mut gone = full.clone();

        gone.mods.retain(|m| m.guid != guid);
        editor.found((gone, Builds::default()), Vec::new());

        assert_eq!(at(&editor), None);
        assert!(editor.changed());

        editor.found((full, Builds::default()), Vec::new());

        let last = rows(&editor).len() - 1;

        assert_eq!(at(&editor), Some(last));
        assert!(rows(&editor)[last].enabled);
    }

    /// The file's order and flags open it; an edit is a change.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn an_edit_is_a_change() {
        let (_dir, mut editor) = editor("change");

        editor.open(&[entry(HEARTH, true)], Loading::On);

        assert!(!editor.changed());
        assert!(rows(&editor)[0].enabled);
        assert!(rows(&editor)[1..].iter().all(|e| !e.enabled));

        editor.edit(Edit::Toggle(0));
        assert!(editor.changed());
    }

    /// A collection saved and loaded back: order, flags, and an entry no
    /// folder holds; the toggle not with it, nothing applied.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_mod_list_saves_and_loads_back() {
        let (_dir, mut editor) = editor("collection");
        let path = editor.list_file.clone().expect("a path");
        let text = format!(
            "[[mod]]\nid = \"{HEARTH}\"\nenabled = true\n\n\
             [[mod]]\nid = \"{UNKNOWN}\"\nname = \"gone\"\nenabled = true\n"
        );

        std::fs::write(&path, text).expect("written");
        editor.open(&mod_list::read_at(&path), Loading::On);
        std::fs::remove_file(&path).expect("removed");
        editor.edit(Edit::Down(0));
        editor.act(Act::Save);
        editor.reply(Reply::Store(" set/one ".to_owned()));

        let saved = editor.draft.clone().expect("open");

        assert_eq!(editor.asked, None);
        assert_eq!(
            editor.lists.as_deref().map(mod_list::lists),
            Some(vec!["set_one".to_owned()])
        );

        editor.open(&[], Loading::Off);
        editor.act(Act::Load);
        assert_eq!(editor.listed, ["set_one"]);

        editor.reply(Reply::Pick("set_one".to_owned()));

        let draft = editor.draft.clone().expect("open");

        assert_eq!(draft.loading, Loading::Off, "the toggle as was");
        assert_eq!(
            draft,
            List {
                loading: Loading::Off,
                ..saved
            }
        );
        assert_eq!(draft.rows[1].guid, Guid::parse(HEARTH));
        assert!(draft.rows[1].enabled);
        assert_eq!(draft.kept.len(), 1, "kept");
        assert!(
            !editor.list_file.as_ref().expect("a path").exists(),
            "not applied"
        );
    }

    /// A name taken asks first: No goes back to the name, Yes writes.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn saving_over_a_mod_list_asks_first() {
        let (_dir, mut editor) = editor("overwrite");
        let named = || Reply::Store("set".to_owned());

        editor.open(&[], Loading::On);
        editor.act(Act::Save);
        editor.reply(named());
        editor.edit(Edit::Toggle(0));
        editor.act(Act::Save);
        editor.reply(named());
        assert_eq!(editor.asked, Some(Asked::Overwrite("set".to_owned())));

        editor.reply(Reply::No);
        assert_eq!(editor.asked, Some(Asked::Save("set".to_owned())));

        editor.reply(named());
        editor.reply(Reply::Yes);
        assert_eq!(editor.asked, None);

        let path = mod_list::list_path(editor.lists.as_deref().expect("a folder"), "set");

        assert!(mod_list::read_at(&path)[0].enabled, "overwritten");
    }

    /// Delete asks first; No keeps it, Yes removes it.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn deleting_a_mod_list_asks_first() {
        let (_dir, mut editor) = editor("delete");
        let listed = |e: &Editor| e.lists.as_deref().map_or_default(mod_list::lists);

        editor.open(&[], Loading::On);
        editor.act(Act::Save);
        editor.reply(Reply::Store("set".to_owned()));
        editor.act(Act::Load);
        editor.reply(Reply::Delete("set".to_owned()));
        assert_eq!(editor.asked, Some(Asked::Delete("set".to_owned())));

        editor.reply(Reply::No);
        assert_eq!(editor.asked, Some(Asked::Load));
        assert_eq!(listed(&editor), ["set"]);

        editor.reply(Reply::Delete("set".to_owned()));
        editor.reply(Reply::Yes);
        assert_eq!(editor.asked, Some(Asked::Load));
        assert_eq!(listed(&editor).len(), 0);
    }

    /// Done writes the list and the toggle, and closes: loading off, a
    /// game loads none, the list kept.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn done_applies_the_list() {
        let (_dir, mut editor) = editor("done");
        let path = editor.list_file.clone().expect("a path");

        editor.open(&[entry(HEARTH, true)], Loading::On);
        editor.edit(Edit::Down(0));
        editor.edit(Edit::Loading);
        editor.act(Act::Done);

        let entries = mod_list::read_at(&path);

        assert_eq!(editor.draft, None);
        assert_eq!(mod_list::loading_at(&path), Loading::Off);
        assert_eq!(mod_list::chosen(&entries, Loading::Off).len(), 0);
        assert_eq!(entries[1].guid, Guid::parse(HEARTH));
        assert!(entries[1].enabled, "the list kept");
    }

    /// Over a lobby, Done hands the list over, the entries no folder
    /// holds with it, and writes no settings file.
    #[test]
    fn done_over_a_lobby_hands_the_list_over_and_writes_nothing() {
        let dir = crate::testdata::scratch("mods-page-lobby");
        let mut editor = Editor {
            list_file: Some(dir.path().join("mods.toml")),
            ..Editor::default()
        };

        editor.open_lobby(&[entry(UNKNOWN, true)], Loading::On);
        assert!(editor.over_lobby());

        editor.edit(Edit::Loading);
        editor.act(Act::Done);

        let (entries, loading) = editor.take_chosen().expect("handed over");

        assert!(!editor.over_lobby());
        assert_eq!(loading, Loading::Off);
        assert_eq!(entries[0].guid, Guid::parse(UNKNOWN));
        assert!(!editor.list_file.as_ref().expect("a path").exists());
    }

    /// Cancel drops the edits and closes; nothing written.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn cancel_discards_the_edits() {
        let (_dir, mut editor) = editor("cancel");

        editor.open(&[entry(HEARTH, true)], Loading::On);
        editor.edit(Edit::Toggle(0));
        editor.edit(Edit::Loading);
        editor.act(Act::Cancel);

        assert_eq!(editor.draft, None);
        assert!(!editor.list_file.as_ref().expect("a path").exists());
    }
}
