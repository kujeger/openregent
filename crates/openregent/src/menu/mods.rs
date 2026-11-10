//! The mods page, over the main menu from its Mods button: a placeholder
//! egui window for HD's mods dialog, `APdc`. Every
//! mod the folders hold, in load order, each enabled or not, moved up or
//! down; mod loading on or off. Done writes it to the settings file
//! ([`crate::files::mod_list`]); a new game loads those enabled, the menus' data
//! is not reloaded. Load and Save read and write collections, lists
//! saved apart; neither applies one.
//!
//! ```text
//!  [Mods]  over the main menu, top left
//!
//!  ┌ Mods ──────────────────────────────────────────────────────────┐
//!  │ [x] Load mods                                                  │
//!  │ A new game loads the ticked mods, top to bottom; …             │
//!  │  1 [x] New Game+  Rebalances the game, scales…     [Up] [Down] │  scrolls
//!  │    [ ] Decor Buildings   New decorations…          [Up] [Down] │
//!  │ ────────────────────────────────────────────────────────────── │
//!  │ New Game+                          the picked row's details:   │
//!  │ its long description, the games it works with, its quests,     │
//!  │ files, id and item folder                                      │
//!  │ 2 folders searched · 1 missing          (the lists on hover)   │
//!  │ [Load…] [Save…] [Rescan]                      [Done] [Cancel]  │
//!  └────────────────────────────────────────────────────────────────┘
//!
//!  Load…  a collection's list into the page   [Delete]: asked first
//!  Save…  the page's list as a collection     a name taken: asked first
//!  Rescan the folders scanned again, changed GPL source compiled; the
//!         list's edits kept (`menu::rescan`)
//!  Done   written, if changed, and closed     Cancel: closed, dropped
//! ```
//!
//! Numbers are the enabled mods' load order. The page is the window
//! less a margin, no wider than [`MAX_SIZE`]; the head and foot stay,
//! the list between them scrolls.
//!
//! Loading off greys the list and Load: neither is edited, the list kept.
//!
//! HD keeps its choice one session (**Inferred**, mods free); this
//! game keeps it in the settings file. HD's main menu control 9001,
//! ACTIVATE MODS, has the Load mods toggle as stand-in.

use std::path::{Path, PathBuf};

use bevy::log::warn;
use bevy_egui::egui::{self, Color32, RichText, TextStyle, vec2};
use regent_core::content::dataset::Datasets;
use regent_core::content::mods::{Base, Library, Mod};
use regent_core::data::guid::Guid;
use regent_core::view::menu::control::Able;

use super::page::{Button, Press};
use super::paint::tapped;
use crate::files::compiled::{Builds, Built};
use crate::files::mod_list::{self, Entry, Loading};

/// The window's title, its button's and its own buttons' labels:
/// placeholders.
const TITLE: &str = "Mods";
const UP: &str = "Up";
const DOWN: &str = "Down";
const LOAD_MODS: &str = "Load mods";
const LOAD: &str = "Load…";
const SAVE: &str = "Save…";
const DONE: &str = "Done";
const RESCAN: &str = "Rescan";
const STORE: &str = "Save";
const DELETE: &str = "Delete";

/// Rescan's hover text.
const RESCAN_SAYS: &str = "Scan the mod folders again; changed GPL source is compiled.";

/// A built-in mod's mark ([`crate::files::builtin`]).
const BUILT_IN: &str = "Built in";

/// A row's mark for a mod whose GPL source will not compile.
const REFUSED: &str = "Refused";

/// The dialogs' titles: placeholders.
const LOAD_TITLE: &str = "Load mod list";
const SAVE_TITLE: &str = "Save mod list";
const OVERWRITE_TITLE: &str = "Overwrite mod list";
const DELETE_TITLE: &str = "Delete mod list";

/// The page's most size, HD pixels: scaled as the text
/// ([`crate::ui::layout::scale`]), less where the window is small.
const MAX_SIZE: [f32; 2] = [900.0, 720.0];

/// The page's gap from the window's edges, points; and the room its
/// title bar takes, HD pixels.
const MARGIN: f32 = 16.0;
const TITLE_ROOM: f32 = 48.0;

/// The details' height, in lines of text: the rest scrolls.
const DETAIL_LINES: f32 = 7.0;

/// The toggle's and the picked mod's name's colour: the settings page's
/// values'.
const ACCENT: Color32 = Color32::from_rgb(0xf6, 0xad, 0x52);

/// A problem's colour, as the settings page's refusals'.
const PROBLEM: Color32 = Color32::RED;

/// The toggle's line, on and off.
const ON_SAYS: &str = "A new game loads the ticked mods, top to bottom; a lower one wins where two change the same thing.";
const OFF_SAYS: &str = "Off: a new game loads no mods. The list is kept.";

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
    /// The collections' names, read as a dialog opens.
    listed: Vec<String>,
    /// The row whose details show.
    picked: Option<Guid>,
    /// Rescan pressed: the folders to scan again (`menu::rescan`).
    rescan: bool,
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
        let (rows, kept) = mod_list::rows(entries, &self.library);

        self.saved = List {
            rows,
            kept,
            loading,
        };
        self.picked = self.saved.rows.first().and_then(|e| e.guid);
        self.draft = Some(self.saved.clone());
        self.asked = None;
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
        self.listed = self
            .lists
            .as_deref()
            .map(mod_list::lists)
            .unwrap_or_default();
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
///
/// The head (the toggle) and the foot (details, folders, buttons) stay;
/// the list between them scrolls.
pub fn page(ctx: &egui::Context, editor: &mut Editor) -> Option<Press> {
    let draft = editor.draft.clone()?;
    let on = draft.loading == Loading::On;
    let free = editor.asked.is_none();
    let shown = egui::Window::new(TITLE)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .collapsible(false)
        .resizable(false)
        .fixed_size(page_size(ctx.viewport_rect().size()))
        .show(ctx, |ui| {
            // A dialog open takes the presses.
            ui.add_enabled_ui(free, |ui| {
                let toggled = egui::Panel::top("mods head")
                    .frame(egui::Frame::NONE)
                    .show(ui, |ui| head(ui, draft.loading))
                    .inner;
                let act = egui::Panel::bottom("mods foot")
                    .frame(egui::Frame::NONE)
                    .show(ui, |ui| foot(ui, editor, &draft))
                    .inner;
                let (edit, picked) = egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(ui, |ui| {
                        ui.add_enabled_ui(on, |ui| rows(ui, editor, &draft.rows))
                            .inner
                    })
                    .inner;

                (toggled.then_some(Edit::Loading).or(edit), picked, act)
            })
            .inner
        });
    let (edit, picked, act) = shown.and_then(|s| s.inner).unwrap_or((None, None, None));
    let reply = dialog(ctx, editor.asked.as_mut(), &editor.listed);

    if let Some(edit) = edit {
        editor.edit(edit);
    }

    if picked.is_some() {
        editor.picked = picked;
    }

    if let Some(reply) = reply {
        editor.reply(reply);
    }

    if let Some(act) = act {
        editor.act(act);
    }

    matches!(act, Some(Act::Done | Act::Cancel)).then_some(Press::Button(Button::Cancel))
}

/// The page's size on a `window`-sized window: the window less a
/// margin, no more than [`MAX_SIZE`] at the interface's scale.
fn page_size(window: egui::Vec2) -> egui::Vec2 {
    let scale = crate::ui::layout::scale(window);
    let [width, height] = MAX_SIZE;

    vec2(
        (window.x - 2.0 * MARGIN).min(width * scale),
        (window.y - 2.0 * MARGIN - TITLE_ROOM * scale).min(height * scale),
    )
}

/// The toggle, and a line on what it does: whether it was pressed.
fn head(ui: &mut egui::Ui, loading: Loading) -> bool {
    let mut ticked = loading == Loading::On;
    let label = RichText::new(LOAD_MODS).strong().color(ACCENT);
    let toggled = ui.checkbox(&mut ticked, label).changed();
    let says = match loading {
        Loading::On => ON_SAYS,
        Loading::Off => OFF_SAYS,
    };

    ui.label(says);
    ui.separator();
    toggled
}

/// The foot: the picked mod's details, the folders searched and the
/// mods passed over or missing, the buttons.
fn foot(ui: &mut egui::Ui, editor: &Editor, draft: &List) -> Option<Act> {
    let picked = editor.picked.and_then(|g| editor.library.find(g));

    ui.separator();
    details(ui, editor, picked);
    ui.separator();
    sources(ui, editor, &draft.kept);
    ui.separator();
    acts(ui, draft.loading)
}

/// The picked mod's details, a fixed height that scrolls: name, long
/// description, the games it works with, quests, files, id, folder.
fn details(ui: &mut egui::Ui, editor: &Editor, picked: Option<&Mod>) {
    let library = &editor.library;

    let line = ui.text_style_height(&TextStyle::Body) + ui.spacing().item_spacing.y;

    egui::ScrollArea::vertical()
        .id_salt("mod details")
        .max_height(DETAIL_LINES * line)
        .min_scrolled_height(DETAIL_LINES * line)
        .auto_shrink([false, false])
        .show(ui, |ui| match picked {
            None => {
                ui.weak("Pick a mod for its details.");
            }
            Some(m) => {
                let about = if m.long.is_empty() { &m.short } else { &m.long };
                let brings = Brings::of(library, m);

                ui.label(RichText::new(m.title()).strong().color(ACCENT));

                if !about.is_empty() {
                    ui.label(about);
                }

                ui.label(format!("Works with: {}", works_with(&m.base)));

                if let Some(said) = brings.said() {
                    ui.label(said);
                }

                ui.weak(format!("Files: {}", holds(m)));

                // An enhancement: GPL source compiled where HD needs the
                // Bcd (`compiled`).
                match editor.builds.get(&m.guid) {
                    Some(Built::Compiled { image, .. }) => {
                        ui.weak(format!("GPL: compiled from source, images {image}"));
                    }
                    Some(Built::Refused(why)) => {
                        ui.colored_label(
                            PROBLEM,
                            format!("{REFUSED}: its GPL source does not compile: {why}"),
                        );
                    }
                    None => {}
                }

                ui.weak(format!("Id: {}", m.guid));
                ui.weak(format!("Folder: {}", folder_said(m)))
                    .on_hover_text(m.package.display().to_string());
            }
        });
}

/// The folders searched, the mods passed over and those missing: a
/// count each, the list on hover. A missing mod enabled is a problem.
fn sources(ui: &mut egui::Ui, editor: &Editor, kept: &[Entry]) {
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

    ui.horizontal_wrapped(|ui| {
        let n = folders.len();

        ui.weak(format!("{n} {} searched", plural(n, "folder")))
            .on_hover_text(folders.join("\n"));

        if !skipped.is_empty() {
            ui.weak(format!("· {} skipped", skipped.len()))
                .on_hover_text(skipped.join("\n"));
        }

        if !missing.is_empty() {
            let text = format!(
                "· {} missing: listed, but no folder holds it",
                missing.len()
            );
            let label = if kept.iter().any(|e| e.enabled) {
                ui.colored_label(PROBLEM, text)
            } else {
                ui.weak(text)
            };

            label.on_hover_text(missing.join("\n"));
        }
    });
}

/// The page's buttons, Load and Save left, Done and Cancel right: Load
/// greyed with loading off.
fn acts(ui: &mut egui::Ui, loading: Loading) -> Option<Act> {
    let on = loading == Loading::On;

    ui.horizontal(|ui| {
        let load = tapped(&ui.add_enabled(on, egui::Button::new(LOAD))).then_some(Act::Load);
        let save = tapped(&ui.button(SAVE)).then_some(Act::Save);
        let rescan = tapped(&ui.button(RESCAN).on_hover_text(RESCAN_SAYS)).then_some(Act::Rescan);
        let right = egui::Layout::right_to_left(egui::Align::Center);
        let closed = ui
            .with_layout(right, |ui| {
                let cancel = tapped(&ui.button(Button::Cancel.label())).then_some(Act::Cancel);
                let done = tapped(&ui.button(DONE)).then_some(Act::Done);

                done.or(cancel)
            })
            .inner;

        load.or(save).or(rescan).or(closed)
    })
    .inner
}

/// The dialog `asked` over the page, if any: its press.
fn dialog(ctx: &egui::Context, asked: Option<&mut Asked>, listed: &[String]) -> Option<Reply> {
    let asked = asked?;
    let title = match asked {
        Asked::Load => LOAD_TITLE,
        Asked::Save(_) => SAVE_TITLE,
        Asked::Overwrite(_) => OVERWRITE_TITLE,
        Asked::Delete(_) => DELETE_TITLE,
    };

    egui::Window::new(title)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .collapsible(false)
        .resizable(false)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| match asked {
            Asked::Load => picked(ui, listed),
            Asked::Save(name) => named(ui, name, listed),
            Asked::Overwrite(name) => question(ui, &format!("Overwrite the mod list \"{name}\"?")),
            Asked::Delete(name) => question(ui, &format!("Delete the mod list \"{name}\"?")),
        })
        .and_then(|s| s.inner)
        .flatten()
}

/// Load's dialog: each collection, picked or deleted.
fn picked(ui: &mut egui::Ui, listed: &[String]) -> Option<Reply> {
    if listed.is_empty() {
        ui.weak("None saved.");
    }

    let picked = egui::Grid::new("mod lists")
        .show(ui, |ui| {
            listed.iter().fold(None, |pressed, name| {
                let pick = tapped(&ui.button(name)).then(|| Reply::Pick(name.clone()));
                let delete = tapped(&ui.button(DELETE)).then(|| Reply::Delete(name.clone()));

                ui.end_row();
                pressed.or(pick).or(delete)
            })
        })
        .inner;

    ui.separator();

    let cancel = tapped(&ui.button(Button::Cancel.label())).then_some(Reply::No);

    picked.or(cancel)
}

/// Save's dialog: a name typed, or a collection's taken.
fn named(ui: &mut egui::Ui, name: &mut String, listed: &[String]) -> Option<Reply> {
    ui.text_edit_singleline(name);

    let taken = listed.iter().fold(None, |pressed, listed| {
        pressed.or(tapped(&ui.button(listed)).then(|| listed.clone()))
    });

    if let Some(taken) = taken {
        name.clone_from(&taken);
    }

    ui.separator();
    ui.horizontal(|ui| {
        let fit = mod_list::list_name(name).is_some();
        let save = tapped(&ui.add_enabled(fit, egui::Button::new(STORE)))
            .then(|| Reply::Store(name.clone()));
        let cancel = tapped(&ui.button(Button::Cancel.label())).then_some(Reply::No);

        save.or(cancel)
    })
    .inner
}

/// A question: Yes or No.
fn question(ui: &mut egui::Ui, text: &str) -> Option<Reply> {
    ui.label(text);
    ui.horizontal(|ui| {
        let yes = tapped(&ui.button(Button::Yes.label())).then_some(Reply::Yes);
        let no = tapped(&ui.button(Button::No.label())).then_some(Reply::No);

        yes.or(no)
    })
    .inner
}

/// Every found mod in load order, a row each: its number while enabled,
/// box, name and short description, problems, moves. The
/// edit pressed, and the row picked.
fn rows(ui: &mut egui::Ui, editor: &Editor, draft: &[Entry]) -> (Option<Edit>, Option<Guid>) {
    let (library, picked) = (&editor.library, editor.picked);

    if library.mods.is_empty() {
        ui.weak("None found.");
    }

    let ranks = ranks(draft);
    let rank_width = ui.text_style_height(&TextStyle::Body) * 1.5;

    egui::ScrollArea::vertical()
        .id_salt("mods")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            draft
                .iter()
                .zip(ranks)
                .enumerate()
                .fold((None, None), |pressed, (i, (row, rank))| {
                    let Some(m) = row.guid.and_then(|g| library.find(g)) else {
                        return pressed;
                    };
                    let mut on = row.enabled;

                    ui.horizontal(|ui| {
                        let number = rank.map(|r| r.to_string()).unwrap_or_default();

                        ui.add_sized(
                            [rank_width, ui.spacing().interact_size.y],
                            egui::Label::new(number),
                        );

                        let toggled = ui.checkbox(&mut on, "").changed();
                        let right = egui::Layout::right_to_left(egui::Align::Center);
                        let (moved, chose) = ui
                            .with_layout(right, |ui| {
                                let down = tapped(&ui.button(DOWN)).then_some(Edit::Down(i));
                                let up = tapped(&ui.button(UP)).then_some(Edit::Up(i));

                                if never_loads(&m.base) {
                                    ui.colored_label(PROBLEM, "Never loads");
                                }

                                if let Some(Built::Refused(why)) = editor.builds.get(&m.guid) {
                                    ui.colored_label(PROBLEM, REFUSED).on_hover_text(why);
                                }

                                if crate::files::builtin::holds(m.guid) {
                                    ui.weak(BUILT_IN);
                                }

                                let pick = if picked == Some(m.guid) {
                                    Pick::Picked
                                } else {
                                    Pick::Free
                                };
                                let shown = line(ui, m, Able::of(row.enabled), pick);

                                (up.or(down), tapped(&shown).then_some(m.guid))
                            })
                            .inner;

                        (
                            pressed.0.or(toggled.then_some(Edit::Toggle(i))).or(moved),
                            pressed.1.or(chose),
                        )
                    })
                    .inner
                })
        })
        .inner
}

/// Whether a row is the picked one.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Pick {
    Picked,
    Free,
}

/// A row's name and short description, one line cut to its room, picked
/// or not: a click picks it. The name is brighter while enabled.
fn line(ui: &mut egui::Ui, m: &Mod, enabled: Able, picked: Pick) -> egui::Response {
    let size = vec2(ui.available_width(), ui.spacing().interact_size.y);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let visuals = ui.visuals();
    let pad = ui.spacing().button_padding.x;

    // The pick's band, else the hover's.
    if picked == Pick::Picked {
        ui.painter()
            .rect_filled(rect, 2.0, visuals.selection.bg_fill);
    } else if response.hovered() {
        ui.painter()
            .rect_filled(rect, 2.0, visuals.widgets.hovered.weak_bg_fill);
    }

    // Picked: both bright on the band.
    let (name, about) = match (picked, enabled) {
        (Pick::Picked, _) => (visuals.strong_text_color(), visuals.text_color()),
        (Pick::Free, Able::Enabled) => (visuals.strong_text_color(), visuals.weak_text_color()),
        (Pick::Free, Able::Disabled) => (visuals.text_color(), visuals.weak_text_color()),
    };
    let font = TextStyle::Body.resolve(ui.style());
    let short = m.short.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut job = egui::text::LayoutJob::default();

    job.append(
        &m.title(),
        0.0,
        egui::TextFormat::simple(font.clone(), name),
    );
    job.append(&short, font.size, egui::TextFormat::simple(font, about));
    job.wrap = egui::text::TextWrapping::truncate_at_width(rect.width() - 2.0 * pad);

    let galley = ui.painter().layout_job(job);
    let at = egui::pos2(rect.left() + pad, rect.center().y - galley.size().y / 2.0);

    ui.painter().galley(at, galley, name);
    response
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

    /// The Workshop's Adjust Guardhouse holds one image; Majesty
    /// Expanded's XML is said.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_mod_says_what_it_holds() {
        let install = crate::files::env::Install::from_env().expect("MAJESTY_HD");
        let library = install.mods();
        let by = |guid: &str| {
            library
                .find(Guid::parse(guid).expect("a GUID"))
                .expect("found")
        };

        assert_eq!(holds(by("{D666FCBE-9906-4582-A6A0-DC96F812441E}")), "1 GPL");
        assert!(holds(by("{17BF53DE-722F-4683-8E63-6B93D1606DFF}")).contains("XML descriptions"));
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

    /// Majesty Expanded's Workshop item holds 28 quests and 23 other
    /// mods: the quests are said as the folder's. New Game+ is alone in
    /// its item with 1 quest; Adjust Guardhouse has none.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_mods_quests_are_its_folders() {
        let install = crate::files::env::Install::from_env().expect("MAJESTY_HD");
        let library = install.mods();
        let of = |guid: &str| {
            Brings::of(
                &library,
                library
                    .find(Guid::parse(guid).expect("a GUID"))
                    .expect("found"),
            )
        };
        let expanded = of(EXPANDED);
        let new_game = of("{DAA41917-B61F-4934-98E5-A6E386FE6DD5}");
        let guardhouse = of("{D666FCBE-9906-4582-A6A0-DC96F812441E}");

        assert_eq!(
            expanded,
            Brings {
                quests: 28,
                others: 23
            }
        );
        assert!(
            expanded
                .said()
                .expect("said")
                .contains("shared with 23 other mods")
        );
        assert!(
            new_game
                .said()
                .expect("said")
                .starts_with("Brings 1 quest:")
        );
        assert_eq!(guardhouse.said(), None);
    }

    const EXPANDED: &str = "{17BF53DE-722F-4683-8E63-6B93D1606DFF}";
    const UNKNOWN: &str = "{00000000-0000-0000-0000-000000000001}";

    /// An editor on the install's mods, its settings file and
    /// collections in a fresh scratch folder.
    fn editor(name: &str) -> (crate::testdata::Scratch, Editor) {
        let install = crate::files::env::Install::from_env().expect("MAJESTY_HD");
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

        editor.open(&[entry(EXPANDED, true)], Loading::On);

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
            "[[mod]]\nid = \"{EXPANDED}\"\nenabled = true\n\n\
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
        assert_eq!(draft.rows[1].guid, Guid::parse(EXPANDED));
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
        let listed = |e: &Editor| e.lists.as_deref().map(mod_list::lists).unwrap_or_default();

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

        editor.open(&[entry(EXPANDED, true)], Loading::On);
        editor.edit(Edit::Down(0));
        editor.edit(Edit::Loading);
        editor.act(Act::Done);

        let entries = mod_list::read_at(&path);

        assert_eq!(editor.draft, None);
        assert_eq!(mod_list::loading_at(&path), Loading::Off);
        assert_eq!(mod_list::chosen(&entries, Loading::Off).len(), 0);
        assert_eq!(entries[1].guid, Guid::parse(EXPANDED));
        assert!(entries[1].enabled, "the list kept");
    }

    /// Cancel drops the edits and closes; nothing written.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn cancel_discards_the_edits() {
        let (_dir, mut editor) = editor("cancel");

        editor.open(&[entry(EXPANDED, true)], Loading::On);
        editor.edit(Edit::Toggle(0));
        editor.edit(Edit::Loading);
        editor.act(Act::Cancel);

        assert_eq!(editor.draft, None);
        assert!(!editor.list_file.as_ref().expect("a path").exists());
    }
}
