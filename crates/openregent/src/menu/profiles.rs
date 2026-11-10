//! The profile page, over the main menu from its Profile button: pick a
//! profile, name a new one, or delete one. The list is a placeholder egui
//! window, the original having none; a new name is asked in the
//! original's name prompt (`AP35`, `regent_core::view::menu::NAME_PROMPT`,
//! `paint::prompt`), which it asks at Play Game, keying no progress by
//! it. Games play as the last used profile, asked nothing
//! (`Profiles::current`).
//!
//! ```text
//!  [Profile: Kåre]  over the main menu, top right
//!
//!  ┌ Profile ─────────────────────────┐      AP35 ┌────────────────────┐
//!  │ Kåre                         │           │ Please enter your  │
//!  │ (•) Your Majesty                 │ New       │ [  Your Majesty  ] │
//!  │ [Continue] [New] [Delete] [Cancel] ──>       │ [ACCEPT] [CANCEL]  │
//!  └──────────────────────────────────┘           └────────────────────┘
//!            │ Delete                     why a name will not do, under it
//!            v
//!  Delete Kåre and its saves?  [Yes] [No]
//! ```
//!
//! With no profile it opens on the prompt; its Cancel goes back to the
//! list, or with none, to the main menu. The first start, no profile
//! existing, opens this page once the main menu is reached
//! ([`Picker::start`]); cancelled, the first game makes the default.

use bevy::log::{info, warn};
use bevy_egui::egui;

use super::Art;
use super::page::{Button, Press};
use super::paint::{self, Caret, tapped};
use crate::files::profile::{DEFAULT_NAME, Listed, Player, Profile, Profiles};

/// The window's title and its own buttons' labels: placeholders.
const CHOOSE_TITLE: &str = "Profile";
const NEW: &str = "New";
const DELETE: &str = "Delete";

/// Which part of the page shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Choose,
    Create,
    /// Delete the listed profile at this index? Asked first.
    Delete(usize),
}

/// The page's state: the profiles listed, the one chosen, a new name.
#[derive(bevy::prelude::Resource)]
pub struct Picker {
    store: Option<Profiles>,
    listed: Vec<Listed>,
    chosen: Option<usize>,
    mode: Mode,
    name: String,
    /// How the name prompt's text starts.
    caret: Caret,
    /// Why the last press did nothing.
    said: Option<String>,
    /// No profile at start-up: the main menu is yet to ask for a name.
    unasked: bool,
}

impl Picker {
    /// Over `store`; `None` with no data directory, where nothing opens.
    #[must_use]
    pub const fn new(store: Option<Profiles>) -> Picker {
        Picker {
            store,
            listed: Vec::new(),
            chosen: None,
            mode: Mode::Create,
            name: String::new(),
            caret: Caret::SelectAll,
            said: None,
            unasked: false,
        }
    }

    /// The profile a menu start plays as, made by nobody: the last used,
    /// remembered, else the first that reads. With none existing, the
    /// main menu will ask for a name ([`Picker::ask`]).
    pub fn start(&mut self) -> Option<Profile> {
        let found = self.store.as_ref().and_then(Profiles::found);
        let remembered = found.as_ref().zip(self.store.as_ref());

        if let Some(Err(why)) = remembered.map(|(profile, store)| store.remember(profile)) {
            warn!("{why}");
        }

        self.unasked = self.store.as_ref().is_some_and(|s| s.list().is_empty());
        found
    }

    /// Whether to ask for a first profile's name now: true once.
    pub const fn ask(&mut self) -> bool {
        let asking = self.unasked;

        self.unasked = false;
        asking
    }

    /// As the page opens: the profiles read again, the last used chosen;
    /// none, New profile, `name` offered.
    pub fn open(&mut self, name: Option<String>) {
        self.name = name.unwrap_or_else(|| DEFAULT_NAME.to_owned());
        self.relist();
    }

    /// The profiles read again, the last used chosen; none, New profile.
    fn relist(&mut self) {
        let last = self.store.as_ref().and_then(Profiles::last);

        self.listed = self.store.as_ref().map(Profiles::list).unwrap_or_default();
        self.chosen = last
            .and_then(|dir| self.listed.iter().position(|l| l.dir == dir))
            .or_else(|| (!self.listed.is_empty()).then_some(0));
        self.mode = if self.listed.is_empty() {
            Mode::Create
        } else {
            Mode::Choose
        };
        self.caret = Caret::SelectAll;
        self.said = None;
    }

    /// Whether the name prompt shows: Escape is its Cancel's.
    #[must_use]
    pub fn naming(&self) -> bool {
        self.mode == Mode::Create
    }

    /// The profile a game plays as, asked nothing: the last used, else
    /// one made, named as the original's prompt offers in `art`'s words
    /// (`Profiles::current`).
    #[must_use]
    pub fn profile(&self, art: &Art) -> Option<Profile> {
        self.current(art.book.words.gmtx(regent_core::view::menu::NAME_TEXT))
    }

    /// The profile a game plays as (`Profiles::current`), one named
    /// `name` made where there is none; `None` with no data directory,
    /// or one that will not be made.
    pub fn current(&self, name: Option<String>) -> Option<Profile> {
        let name = name.unwrap_or_else(|| DEFAULT_NAME.to_owned());
        let made = self.store.as_ref().map(|s| s.current(&name)).transpose();

        made.unwrap_or_else(|why| {
            warn!("{why}");
            None
        })
    }

    /// The profile `opened` made, else why not: on success remembered as
    /// the last and made the player's.
    fn take(&mut self, opened: Result<Profile, String>, player: &mut Player) -> Option<Press> {
        let remembered = opened.and_then(|profile| {
            self.store
                .as_ref()
                .map_or(Ok(()), |s| s.remember(&profile))
                .map(|()| profile)
        });

        match remembered {
            Ok(profile) => {
                info!("profile: {}, {}", profile.name, profile.dir.display());
                player.profile = Some(profile);
                self.said = None;
                Some(Press::Button(Button::Continue))
            }
            Err(why) => {
                self.said = Some(why);
                None
            }
        }
    }

    /// The listed profile at `i` deleted, the player's dropped if it was
    /// that one: the next game makes or picks another
    /// ([`Picker::current`]).
    fn delete(&mut self, i: usize, player: &mut Player) {
        let listed = self.listed.get(i).cloned();
        let deleted = listed
            .zip(self.store.clone())
            .map(|(l, store)| store.delete(&l.dir).map(|()| l));

        match deleted {
            Some(Ok(listed)) => {
                info!("profile deleted: {}", listed.name);

                if player
                    .profile
                    .as_ref()
                    .is_some_and(|p| p.dir.ends_with(&listed.dir))
                {
                    player.profile = None;
                }

                self.relist();
            }
            Some(Err(why)) => {
                self.mode = Mode::Choose;
                self.said = Some(why);
            }
            None => self.mode = Mode::Choose,
        }
    }
}

/// The page: the list's window, or the name prompt; Continue once a
/// profile is the player's, Cancel.
pub fn page(
    ctx: &egui::Context,
    picker: &mut Picker,
    player: &mut Player,
    art: (&mut Art, &mut regent_draw::art::Art),
) -> Option<Press> {
    match picker.mode {
        Mode::Create => create(ctx, picker, player, art),
        Mode::Choose | Mode::Delete(_) => {
            let shown = egui::Window::new(CHOOSE_TITLE)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| match picker.mode {
                    Mode::Delete(i) => {
                        delete(ui, picker, i, player);
                        None
                    }
                    _ => choose(ui, picker, player),
                });

            shown.and_then(|s| s.inner).flatten()
        }
    }
}

/// The list, Continue, New, Delete and Cancel.
fn choose(ui: &mut egui::Ui, picker: &mut Picker, player: &mut Player) -> Option<Press> {
    for (i, listed) in picker.listed.iter().enumerate() {
        if ui.radio(picker.chosen == Some(i), &listed.name).clicked() {
            picker.chosen = Some(i);
        }
    }

    said(ui, picker.said.as_deref());
    ui.separator();

    let (go, new, delete, cancel) = ui
        .horizontal(|ui| {
            (
                tapped(&ui.button(Button::Continue.label())),
                tapped(&ui.button(NEW)),
                tapped(&ui.button(DELETE)),
                tapped(&ui.button(Button::Cancel.label())),
            )
        })
        .inner;

    if new {
        picker.mode = Mode::Create;
        picker.caret = Caret::SelectAll;
        picker.said = None;
    }

    if let Some(i) = picker.chosen.filter(|_| delete) {
        picker.mode = Mode::Delete(i);
        picker.said = None;
    }

    let chosen = picker.chosen.and_then(|i| picker.listed.get(i)).cloned();
    let opened = chosen.zip(picker.store.clone()).filter(|_| go);

    match opened {
        Some((listed, store)) => picker.take(store.open(&listed.dir), player),
        None => cancel.then_some(Press::Button(Button::Cancel)),
    }
}

/// Delete's question: Yes deletes the profile and its saves, No goes
/// back to the list.
fn delete(ui: &mut egui::Ui, picker: &mut Picker, i: usize, player: &mut Player) {
    let name = picker.listed.get(i).map_or("", |l| l.name.as_str());

    ui.label(format!("Delete {name} and its saves?"));
    ui.separator();

    let (yes, no) = ui
        .horizontal(|ui| {
            (
                tapped(&ui.button(Button::Yes.label())),
                tapped(&ui.button(Button::No.label())),
            )
        })
        .inner;

    if yes {
        picker.delete(i, player);
    } else if no {
        picker.mode = Mode::Choose;
    }
}

/// The name prompt: ACCEPT makes the profile; CANCEL goes back to the
/// list, or with none, cancels the page.
fn create(
    ctx: &egui::Context,
    picker: &mut Picker,
    player: &mut Player,
    art: (&mut Art, &mut regent_draw::art::Art),
) -> Option<Press> {
    let said = picker.said.clone();
    let typed = (&mut picker.name, &mut picker.caret);
    let pressed = paint::prompt(ctx, art, typed, said.as_deref());

    match (pressed, picker.store.clone()) {
        (Some(Button::Accept), Some(store)) => {
            let name = picker.name.clone();

            picker.take(store.create(&name), player)
        }
        (Some(Button::Cancel), _) if picker.listed.is_empty() => {
            Some(Press::Button(Button::Cancel))
        }
        (Some(Button::Cancel), _) => {
            picker.mode = Mode::Choose;
            picker.said = None;
            None
        }
        _ => None,
    }
}

/// Why the last press did nothing, in red.
fn said(ui: &mut egui::Ui, why: Option<&str>) {
    if let Some(why) = why {
        ui.colored_label(egui::Color32::RED, why);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A picker over a folder of its own, not yet made; the scratch
    /// folder holding it, kept while the picker is used.
    fn scratch(name: &str) -> (crate::testdata::Scratch, Picker) {
        let dir = crate::testdata::scratch(&format!("profiles-{name}"));
        let picker = Picker::new(Some(Profiles::at(&dir.join("data"))));

        (dir, picker)
    }

    #[test]
    fn a_first_start_asks_once_and_makes_nothing_itself() {
        let (_dir, mut picker) = scratch("first_start");

        assert!(picker.start().is_none());
        assert!(picker.store.as_ref().is_some_and(|s| s.list().is_empty()));
        assert!(picker.ask());
        assert!(!picker.ask(), "asked once");

        picker.open(None);
        assert!(picker.naming());
        assert_eq!(picker.name, DEFAULT_NAME);
    }

    #[test]
    fn a_start_with_a_profile_takes_it_and_asks_nothing() {
        let (_dir, mut picker) = scratch("has_profile");
        let store = picker.store.clone().expect("a store");

        store.create("Kåre").expect("made");

        assert_eq!(picker.start().map(|p| p.name).as_deref(), Some("Kåre"));
        assert_eq!(store.last().as_deref(), Some("Kåre"));
        assert!(!picker.ask());
    }

    #[test]
    fn no_data_directory_asks_nothing() {
        let mut picker = Picker::new(None);

        assert!(picker.start().is_none());
        assert!(!picker.ask());
    }
}
