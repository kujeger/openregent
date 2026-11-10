//! The save and load dialogs, the original's `AP49` and `AP27` in their
//! own art (`menu::save_page`), on a profile's saves (`save`), listed from
//! their meta and minimap alone. Their rules are the library's
//! (`regent_core::view::menu::saves`).
//!
//! ```text
//!  ┌(pog)────────────── Save Menu ───────────────────┐
//!  │ [SAVE GAME AS:][(Freestyle Game) 4 Days       ]  │
//!  │ ┌─────────────────────────────────────────┐ ^   │
//!  │ │ (The Siege)    2026-10-08 14:30  4 Days │ #   │  newest first,
//!  │ │ lands 2        2026-10-07 09:12  0 Days │ v   │  time, days right
//!  │ [CURRENT GAME DESCRIPTION][SELECTED GAME DESCR.] │
//!  │ (SAVE GAME)      (DELETE GAME)       (CANCEL)    │
//!  └──────────────────────────────────────────────────┘
//! ```
//!
//! `AP27` has LIST OF SAVED GAMES over the list, no name box, GAME
//! DESCRIPTION between two empty side boxes, and LOAD GAME. The left box
//! shows the lit save's minimap: this game's saves carry one, the
//! original's none.
//!
//! A row pressed is lit, and on `AP49` its name goes in the name box;
//! double-clicked, it presses SAVE GAME or LOAD GAME. SAVE GAME on a
//! name taken asks with `AP11` before writing over it, where the original
//! asks nothing (`docs/openregent/enhancements.md`). DELETE GAME asks too. Why a choice did nothing shows in `AP11` with
//! OK. The list holds the profile's saves of a game like the one played,
//! as the original's: alone, a game of one's from `saves`; in a LAN game,
//! a LAN game's of as many players from `mp-saves` (`save::dir`). The
//! filter stays: it holds the seat count, and a file of the other kind
//! in a folder.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use bevy::prelude::Resource;
use bevy_egui::egui;
use regent_core::content::mods::Library;
use regent_core::data::guid::Guid;
use regent_core::view::menu::listbox::{self, Place};
use regent_core::view::menu::saves::{self, Described, Found, Mode, Players};
use regent_core::view::panel::day::Day;

use crate::save::{self, Listed};

/// Why the dialog does nothing with no profile: a placeholder.
const NO_PROFILE: &str = "No profile: a save goes in one. Play Game makes one.";

/// SAVE GAME's question on a name taken: placeholders, the original
/// having none, worded as DELETE GAME's (`GMTX` 346, 347).
pub(crate) const OVERWRITE_TITLE: &str = "Overwrite Saved Game";
pub(crate) const OVERWRITE_TEXT: &str = "Are you sure you want to overwrite this saved game?";

/// Which dialog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dialog {
    Save,
    Load,
}

impl Dialog {
    /// The library's name for it.
    #[must_use]
    pub const fn mode(self) -> Mode {
        match self {
            Dialog::Save => Mode::Save,
            Dialog::Load => Mode::Load,
        }
    }
}

/// What the player chose.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Chose {
    /// Save as `name` to the file.
    Save {
        path: PathBuf,
        name: String,
    },
    Load(PathBuf),
    Cancel,
}

/// What `AP11` asks over the dialog.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Asked {
    /// DELETE GAME's: remove this save.
    Delete(PathBuf),
    /// SAVE GAME's on a name taken: this save, over it.
    Overwrite(Chose),
}

/// Whether the list takes keys first (`listbox`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Focus {
    /// As the dialog opens (**Inferred**, as `listbox` says).
    #[default]
    None,
    /// Since a row was pressed.
    List,
}

/// Whether the list took a key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Taken {
    Yes,
    No,
}

/// The game the save dialog saves, for CURRENT GAME DESCRIPTION: its
/// title, "Freestyle Game" for a custom game, and its clock, ms; whose
/// game it is, for the list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Here {
    pub title: String,
    pub clock: u32,
    pub players: Players,
}

/// The dialogs' state.
#[derive(Resource, Default)]
pub struct Saves {
    pub open: Option<Dialog>,
    /// The profile's saves directory and its name; `None` with no
    /// profile.
    pub place: Option<(PathBuf, String)>,
    /// The saves, in the list's order.
    pub(crate) listed: Vec<Listed>,
    pub(crate) thumbs: HashMap<PathBuf, egui::TextureHandle>,
    /// The name box's text.
    pub(crate) name: String,
    /// The row lit, and the first row shown.
    pub(crate) lit: Option<usize>,
    pub(crate) scroll: f32,
    pub(crate) focus: Focus,
    /// The question asked.
    pub(crate) asking: Option<Asked>,
    /// Why the last choice did nothing.
    pub said: Option<String>,
    pub(crate) here: Option<Here>,
    /// The game's pog, in the menus' art: over a game, its quest's;
    /// else the panel's own.
    pub pog: Option<(crate::menu::art::Pack, crate::menu::art::Pic)>,
    /// The game's save name: the save dialog's offered
    /// name, then the name saved; its row lit as the list fills.
    pub named: Option<String>,
    /// Whose game the dialog is over: only saves of such a game list.
    pub(crate) over: Players,
    /// The mods the folders hold, by GUID: a save's others are marked
    /// in its description, its load refused.
    pub found: Vec<Guid>,
}

impl Saves {
    /// `dialog` open on the saves in `place`, a profile's directory and
    /// name, over a game of `over`'s players: the save dialog offering
    /// `name` for the game `here`.
    pub fn show(
        &mut self,
        dialog: Dialog,
        place: Option<(PathBuf, String)>,
        name: String,
        (here, over): (Option<Here>, Players),
    ) {
        self.open = Some(dialog);
        self.over = over;
        self.said = place.is_none().then(|| NO_PROFILE.to_owned());
        self.place = place;
        self.thumbs.clear();
        self.asking = None;
        self.here = here;
        self.focus = Focus::None;

        // The offered name is the game's save name from now on.
        if dialog == Dialog::Save {
            self.named = Some(name.clone());
        }

        self.name = name;
        self.fill();
    }

    /// The mods `library` holds, found.
    pub fn found_in(&mut self, library: &Library) {
        self.found = library.mods.iter().map(|m| m.guid).collect();
    }

    /// Closed and empty as a game opens or leaves, but the mods found.
    pub fn fresh(&mut self) {
        *self = Saves {
            found: std::mem::take(&mut self.found),
            ..Saves::default()
        };
    }

    pub fn close(&mut self) {
        self.open = None;
        self.asking = None;
    }

    /// The list read again, the saves of a game like the one played
    /// alone, in order, the game's row lit, shown. A save that
    /// will not read stays, to say why.
    pub(crate) fn fill(&mut self) {
        let mut listed: Vec<Listed> = self
            .place
            .as_ref()
            .map(|(dir, _)| save::list(dir))
            .unwrap_or_default()
            .into_iter()
            .filter(|l| {
                l.glance
                    .as_ref()
                    .map_or(true, |g| saves::listed(players(&g.meta), self.over))
            })
            .collect();

        listed.sort_by(|a, b| saves::newest((saved(a), &row_name(a)), (saved(b), &row_name(b))));
        self.listed = listed;

        let names = self.names();
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        let mode = self.open.map_or(Mode::Load, Dialog::mode);

        self.scroll = 0.0;
        self.lit = None;

        if let Some(n) = saves::lit(&names, self.named.as_deref(), mode) {
            self.light(n);
            #[allow(clippy::cast_precision_loss, reason = "a list's rows")]
            let row = n as f32;

            self.scroll = row;
        }
    }

    /// Row `n` lit: on the save dialog, its name in the name box (the
    /// list's handler, message `0x1b`).
    pub(crate) fn light(&mut self, n: usize) {
        self.lit = Some(n);

        if self.open == Some(Dialog::Save)
            && let Some(l) = self.listed.get(n)
        {
            self.name = row_name(l);
        }
    }

    /// `key` to the list, `shown` rows tall, where focused: the lit row,
    /// named as a press names it, and the scroll it leaves
    /// (`listbox::key`).
    pub(crate) fn take(&mut self, key: i32, shown: usize) -> Taken {
        let names = self.names();
        let texts: Vec<&str> = names.iter().map(String::as_str).collect();
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a list's rows"
        )]
        let place = Place {
            lit: self.lit,
            top: self.scroll.round() as usize,
        };
        let moved = (self.focus == Focus::List)
            .then(|| listbox::key(place, key, (&texts, shown)))
            .flatten();

        match moved {
            Some(p) => {
                #[allow(clippy::cast_precision_loss, reason = "a list's rows")]
                let top = p.top as f32;

                self.scroll = top;

                match p.lit {
                    Some(n) if self.lit != Some(n) => self.light(n),
                    lit => self.lit = lit,
                }

                Taken::Yes
            }
            None => Taken::No,
        }
    }

    /// The lit save's minimap as a texture, made once.
    pub(crate) fn thumb(&mut self, ctx: &egui::Context) -> Option<egui::TextureId> {
        let l = self.lit.and_then(|n| self.listed.get(n))?;
        let glance = l.glance.as_ref().ok()?;
        let texture = self.thumbs.entry(l.path.clone()).or_insert_with(|| {
            let image = save::unpng(&glance.minimap).map_or_else(
                || egui::ColorImage::new([1, 1], vec![egui::Color32::BLACK]),
                |(size, rgba)| egui::ColorImage::from_rgba_unmultiplied(size, &rgba),
            );

            ctx.load_texture(
                l.path.display().to_string(),
                image,
                egui::TextureOptions::NEAREST,
            )
        });

        Some(texture.id())
    }

    /// Each row's text.
    pub(crate) fn names(&self) -> Vec<String> {
        self.listed.iter().map(row_name).collect()
    }

    /// The lit row's save.
    pub(crate) fn lit_save(&self) -> Option<&Listed> {
        self.lit.and_then(|n| self.listed.get(n))
    }

    /// Button `id` pressed: what the player chose, if anything.
    pub(crate) fn pressed(&mut self, id: i32) -> Option<Chose> {
        let dir = self.place.as_ref().map(|(dir, _)| dir.clone());
        let lit = self.lit_save().map(|l| l.path.clone());

        match (id, dir) {
            (saves::CANCEL, _) => Some(Chose::Cancel),
            (_, None) => {
                self.said = Some(NO_PROFILE.to_owned());
                None
            }
            (saves::SAVE, Some(dir)) => {
                let name = self.name.trim().to_owned();

                // A name taken asks first.
                match save::path(&dir, &name) {
                    Ok(path) if path.exists() => {
                        self.asking = Some(Asked::Overwrite(Chose::Save { path, name }));
                        None
                    }
                    Ok(path) => Some(Chose::Save { path, name }),
                    Err(why) => {
                        self.said = Some(why);
                        None
                    }
                }
            }
            (saves::LOAD, Some(_)) => match self.lit_save().map(|l| &l.glance) {
                Some(Err(why)) => {
                    self.said = Some(why.clone());
                    None
                }
                _ => lit.map(Chose::Load),
            },
            (saves::DELETE, Some(_)) => {
                self.asking = lit.map(Asked::Delete);
                None
            }
            _ => None,
        }
    }

    /// The question answered: DELETE GAME's yes removes the save and
    /// fills the list again; SAVE GAME's yes is the save chosen.
    pub(crate) fn answer(&mut self, yes: Answer) -> Option<Chose> {
        match (yes, self.asking.take()) {
            (Answer::Yes, Some(Asked::Delete(path))) => {
                if let Err(e) = std::fs::remove_file(&path) {
                    self.said = Some(format!("{}: {e}", path.display()));
                }

                self.thumbs.remove(&path);
                self.fill();
                None
            }
            (Answer::Yes, Some(Asked::Overwrite(chose))) => Some(chose),
            _ => None,
        }
    }
}

/// How a question was answered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Answer {
    Yes,
    No,
}

/// Whose game a save is: a LAN game's of its seats, else one player's.
fn players(meta: &save::Meta) -> Players {
    meta.lan
        .as_ref()
        .map_or(Players::Alone, |lan| Players::Lan(lan.seats.len()))
}

/// A save's row: its name, else its file's.
pub(crate) fn row_name(l: &Listed) -> String {
    l.glance
        .as_ref()
        .map_or_else(|_| file_name(&l.path), |g| g.meta.name.clone())
}

/// When a save was made, seconds since 1970: its meta's time, else, one
/// that will not read, its file's.
fn saved(l: &Listed) -> u64 {
    let modified = || {
        std::fs::metadata(&l.path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_secs())
    };

    l.glance
        .as_ref()
        .map_or_else(|_| modified(), |g| g.meta.saved)
}

/// A row's text right of its name (`saves::side`): its save's time by
/// the local clock and its days; none for a save that will not read.
pub(crate) fn side(l: &Listed, gmtx: &Strings<'_>) -> Option<[String; 2]> {
    let meta = &l.glance.as_ref().ok()?.meta;
    let at = save::stamp_of(meta.saved, save::offset(meta.saved));

    Some(saves::side(at, meta.clock, gmtx))
}

/// `GMTX` strings, or months' names, by index.
pub type Strings<'a> = dyn Fn(usize) -> Option<String> + 'a;

/// How a description marks a save's mods: those found, by GUID; the
/// box's colour; whether a line fits the box.
pub(crate) type Marking<'a> = (&'a [Guid], [u8; 3], &'a dyn Fn(&str) -> bool);

/// A save's description: its title or players, its game time and its
/// date, then its mods, those not `found` red,
/// the line cut where it no longer `fits` (`saves::mods_line`); why it
/// will not read, if not.
pub(crate) fn description(
    l: &Listed,
    (gmtx, months): (&Strings<'_>, &Strings<'_>),
    (found, normal, fits): Marking<'_>,
) -> String {
    match &l.glance {
        Ok(glance) => {
            let meta = &glance.meta;
            let title = match &meta.game {
                save::SaveKind::Quest { title, .. } | save::SaveKind::Other { title } => {
                    title.clone()
                }
                save::SaveKind::Custom { .. } => {
                    gmtx(saves::FREESTYLE_TEXT.index).unwrap_or_default()
                }
            };
            let of = match &meta.lan {
                Some(lan) => Described::Lan(&lan.seats),
                None => Described::Alone(&title),
            };

            let text = dated(of, (meta.clock, meta.saved), (gmtx, months));
            let mods: Vec<(&str, Found)> = meta
                .needs
                .mods
                .iter()
                .map(|m| {
                    let here = Guid::parse(&m.guid).is_some_and(|g| found.contains(&g));

                    (
                        m.name.as_str(),
                        if here { Found::Here } else { Found::Missing },
                    )
                })
                .collect();

            match saves::mods_line(&mods, normal, fits) {
                Some(line) => format!("{text}\n{line}"),
                None => text,
            }
        }
        Err(why) => why.clone(),
    }
}

/// The game `here`'s description, saved now.
pub(crate) fn current(here: &Here, (gmtx, months): (&Strings<'_>, &Strings<'_>)) -> String {
    let of = Described::Alone(&here.title);

    dated(of, (here.clock, save::now()), (gmtx, months))
}

/// `of`'s description at game `clock`, ms, saved at `seconds` since 1970.
fn dated(
    of: Described<'_>,
    (clock, seconds): (u32, u64),
    (gmtx, months): (&Strings<'_>, &Strings<'_>),
) -> String {
    let day = Day::of(clock);
    let days = day.text_by(gmtx).unwrap_or_default();
    let date = saves::date(save::day_of(seconds), months);

    saves::described(of, (&days, &date), gmtx)
}

/// A file's name without its extension.
fn file_name(path: &Path) -> String {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();

    name.strip_suffix(save::EXTENSION)
        .unwrap_or(&name)
        .to_owned()
}

/// The save and load dialogs: the game's, and the menus' Load Game.
/// A game's leaving leaves them [`Saves::fresh`], the menus then opening
/// theirs (`menu::Return::Load`).
pub fn plugin(app: &mut bevy::prelude::App) {
    use bevy::prelude::*;

    let fresh = |mut saves: ResMut<Saves>| saves.fresh();

    app.init_resource::<Saves>().add_systems(
        OnExit(crate::menu::Screen::Playing),
        fresh.in_set(crate::game::scope::Leave::Reset),
    );
}

/// A player with a profile in a scratch folder of its own, kept while
/// the player is used; a file
/// in each saves folder: `solo` in the single-player one, `lan` in the
/// LAN one. Neither reads, so the list filter keeps neither out: only
/// the folder does.
#[cfg(test)]
pub(crate) fn two_folders(test: &str) -> (crate::testdata::Scratch, crate::files::profile::Player) {
    use crate::ui::options::Play;

    let base = crate::testdata::scratch(&format!("save_folders-{test}"));
    let profile = crate::files::profile::Profiles::at(&base)
        .create("P")
        .expect("a scratch profile");

    for (play, name) in [(Play::Alone, "solo"), (Play::Lan, "lan")] {
        let dir = save::dir(&profile.dir, play);

        std::fs::create_dir_all(&dir).expect("scratch");
        std::fs::write(dir.join(format!("{name}{}", save::EXTENSION)), b"").expect("a file");
    }

    let player = crate::files::profile::Player {
        profile: Some(profile),
        recorded: false,
    };

    (base, player)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// When [`scratch`]'s first file was changed, seconds since 1970.
    const FIRST_CHANGED: u64 = 1_000;

    /// A saves directory of its own holding a file for each of `names`:
    /// none reads, so each row is its file's name, and its time its
    /// file's, a second newer than the one before.
    fn scratch(test: &str, names: &[&str]) -> crate::testdata::Scratch {
        let dir = crate::testdata::scratch(&format!("saves_dialog-{test}"));

        for (name, at) in names.iter().zip(FIRST_CHANGED..) {
            let path = dir.join(format!("{name}{}", save::EXTENSION));
            let changed = std::time::UNIX_EPOCH + std::time::Duration::from_secs(at);

            std::fs::write(&path, b"").expect("a file");
            std::fs::File::options()
                .write(true)
                .open(&path)
                .and_then(|f| f.set_modified(changed))
                .expect("its time");
        }

        dir
    }

    fn opened(dialog: Dialog, dir: &Path, name: &str) -> Saves {
        let mut saves = Saves::default();

        saves.show(
            dialog,
            Some((dir.to_path_buf(), "p".to_owned())),
            name.to_owned(),
            (None, Players::Alone),
        );
        saves
    }

    /// The rows newest first; on the save dialog the offered
    /// name's row lit, a row pressed putting its name in the box.
    #[test]
    fn the_save_dialog_lights_the_offered_name_and_a_row_names_the_save() {
        let dir = scratch("save", &["lands", "lands 2", "a"]);
        let mut saves = opened(Dialog::Save, &dir, "lands");

        assert_eq!(saves.names(), ["a", "lands 2", "lands"]);
        assert_eq!(saves.lit, Some(2));

        saves.light(0);
        assert_eq!(saves.name, "a");

        saves.name = "new".to_owned();

        let chose = saves.pressed(saves::SAVE);

        assert_eq!(
            chose,
            Some(Chose::Save {
                path: dir.join(format!("new{}", save::EXTENSION)),
                name: "new".to_owned(),
            })
        );
    }

    /// The list takes keys once a row was pressed: Down lights the next
    /// row and names it, a letter the next row it starts.
    #[test]
    fn a_pressed_list_takes_the_cursor_keys_and_letters() {
        use regent_core::view::menu::hotkey::DOWN;

        let dir = scratch("keys", &["lands", "b", "a"]);
        let mut saves = opened(Dialog::Save, &dir, "lands");

        assert_eq!(saves.names(), ["a", "b", "lands"]);
        assert_eq!(saves.take(DOWN, 2), Taken::No);

        saves.light(0);
        saves.focus = Focus::List;

        assert_eq!(saves.take(DOWN, 2), Taken::Yes);
        assert_eq!((saves.lit, saves.name.as_str()), (Some(1), "b"));
        assert_eq!(saves.take(i32::from(b'L'), 2), Taken::Yes);
        assert_eq!((saves.lit, saves.name.as_str()), (Some(2), "lands"));
        assert!((saves.scroll - 1.0).abs() < f32::EPSILON);
    }

    /// SAVE GAME on a name taken asks first: NO saves nothing, YES saves
    /// over it.
    #[test]
    fn save_over_a_name_taken_asks_first() {
        let dir = scratch("overwrite", &["a"]);
        let mut saves = opened(Dialog::Save, &dir, "a");
        let over = Chose::Save {
            path: dir.join(format!("a{}", save::EXTENSION)),
            name: "a".to_owned(),
        };

        assert_eq!(saves.pressed(saves::SAVE), None);
        assert_eq!(saves.asking, Some(Asked::Overwrite(over.clone())));
        assert_eq!(saves.answer(Answer::No), None);
        assert_eq!(saves.asking, None);

        saves.pressed(saves::SAVE);
        assert_eq!(saves.answer(Answer::Yes), Some(over));
    }

    /// The load dialog lights the first row with no save of the game's
    /// name; LOAD of a save that will not read says why.
    #[test]
    fn the_load_dialog_lights_the_first_row_and_will_not_load_a_broken_save() {
        let dir = scratch("load", &["b", "a"]);
        let mut saves = opened(Dialog::Load, &dir, "");

        assert_eq!(saves.lit, Some(0));
        assert_eq!(saves.pressed(saves::LOAD), None);
        assert!(saves.said.is_some());
        assert_eq!(saves.pressed(saves::CANCEL), Some(Chose::Cancel));
    }

    /// DELETE asks; NO keeps the save, YES removes it and fills the list
    /// again.
    #[test]
    fn delete_asks_and_removes_on_yes() {
        let dir = scratch("delete", &["a", "b"]);
        let mut saves = opened(Dialog::Load, &dir, "");

        saves.light(1);
        assert_eq!(saves.pressed(saves::DELETE), None);
        assert!(saves.asking.is_some());

        saves.answer(Answer::No);
        assert_eq!(saves.names(), ["b", "a"]);

        saves.pressed(saves::DELETE);
        saves.answer(Answer::Yes);
        assert_eq!(saves.names(), ["b"]);
        assert_eq!(saves.asking, None);
    }

    /// A save's start named `name`, made at `saved`, seconds since 1970,
    /// a LAN game's of `seats` seats where given.
    fn glanced(dir: &Path, (name, saved): (&str, u64), seats: Option<usize>) {
        let meta = save::Meta {
            version: save::VERSION,
            name: name.to_owned(),
            game: save::SaveKind::Other {
                title: String::new(),
            },
            frame: 0,
            clock: 0,
            saved,
            profile: String::new(),
            needs: save::Needs {
                archives: Vec::new(),
                images: String::new(),
                mods: Vec::new(),
            },
            lan: seats.map(|n| save::Lan {
                seats: (0..n).map(|s| format!("p{s}")).collect(),
                delay: 0,
                game: String::new(),
            }),
        };
        let path = dir.join(format!("{name}{}", save::EXTENSION));

        save::write_glance(&path, &meta).expect("written");
    }

    /// The list holds a game of one's saves alone, a LAN game's of as
    /// many players alone; a save that will not read, always.
    #[test]
    fn the_list_holds_the_saves_of_a_game_like_the_one_played() {
        let dir = scratch("filter", &["broken"]);

        glanced(&dir, ("alone", 0), None);
        glanced(&dir, ("two", 0), Some(2));
        glanced(&dir, ("three", 0), Some(3));

        let shown = |over| {
            let mut saves = Saves::default();

            saves.show(
                Dialog::Load,
                Some((dir.to_path_buf(), "p".to_owned())),
                String::new(),
                (None, over),
            );
            saves.names()
        };

        assert_eq!(shown(Players::Alone), ["broken", "alone"]);
        assert_eq!(shown(Players::Lan(2)), ["broken", "two"]);
        assert_eq!(shown(Players::Lan(3)), ["broken", "three"]);
    }

    /// The list is newest first: a save by its meta's time, one that
    /// will not read by its file's.
    #[test]
    fn the_list_is_newest_first_by_save_time_else_file_time() {
        let dir = scratch("newest", &["broken"]);

        glanced(&dir, ("older", FIRST_CHANGED - 1), None);
        glanced(&dir, ("newer", FIRST_CHANGED + 1), None);

        let saves = opened(Dialog::Load, &dir, "");

        assert_eq!(saves.names(), ["newer", "broken", "older"]);
        assert_eq!(saves.lit, Some(0));
    }

    /// With no profile the dialog says so, and saves nothing.
    #[test]
    fn with_no_profile_the_dialog_says_so() {
        let mut saves = Saves::default();

        saves.show(Dialog::Save, None, "x".to_owned(), (None, Players::Alone));
        assert_eq!(saves.said.as_deref(), Some(NO_PROFILE));

        saves.said = None;
        assert_eq!(saves.pressed(saves::SAVE), None);
        assert!(saves.said.is_some());
    }

    /// A LAN save's description names its players after the original's
    /// "Participants:"; a freestyle save's says "Freestyle Game". A save
    /// with mods names them last, one not found red.
    #[test]
    fn a_description_says_the_title_or_the_players_and_the_mods() {
        let gmtx = |i: usize| match i {
            24 => Some("%d Day%s".to_owned()),
            25 => Some("s".to_owned()),
            80 => Some("Freestyle Game".to_owned()),
            82 => Some("%s\nDuration: %s\nSave date: %s\n%s".to_owned()),
            83 => Some("Participants:\n".to_owned()),
            349 => Some("Duration: %s\nSave date: %s\n%s".to_owned()),
            _ => None,
        };
        let months = |i: usize| (i == 9).then(|| "October".to_owned());
        let at = |game, lan, mods| Listed {
            path: PathBuf::from("x"),
            glance: Ok(save::Glance {
                meta: save::Meta {
                    version: save::VERSION,
                    name: "x".to_owned(),
                    game,
                    frame: 0,
                    clock: 4 * regent_core::view::panel::day::DAY_MS,
                    saved: 1_791_460_800,
                    profile: String::new(),
                    needs: save::Needs {
                        archives: Vec::new(),
                        images: String::new(),
                        mods,
                    },
                    lan,
                },
                minimap: Vec::new(),
            }),
        };
        let custom = save::SaveKind::Custom {
            title: "The Dark Forest".to_owned(),
            land: String::new(),
            settlement: String::new(),
            scenario: String::new(),
            monsters: String::new(),
            map: 0,
        };
        let lan = save::Lan {
            seats: vec!["Ann".to_owned(), "Bob".to_owned()],
            delay: 0,
            game: String::new(),
        };

        let here = Guid(1);
        let mods = vec![
            save::Needed {
                guid: here.to_string(),
                name: "Kept".to_owned(),
                image: None,
                flags: None,
            },
            save::Needed {
                guid: Guid(2).to_string(),
                name: "Gone".to_owned(),
                image: None,
                flags: None,
            },
        ];
        let found = [here];
        let wide = |_: &str| true;
        let extra: Marking<'_> = (&found, [0xff; 3], &wide);

        assert_eq!(
            description(
                &at(custom.clone(), None, Vec::new()),
                (&gmtx, &months),
                extra
            ),
            "Freestyle Game\nDuration: 4 Days\nSave date: 8 October 2026"
        );
        assert_eq!(
            description(
                &at(custom.clone(), Some(lan), Vec::new()),
                (&gmtx, &months),
                extra
            ),
            "Duration: 4 Days\nSave date: 8 October 2026\nParticipants:\nAnn, Bob"
        );
        assert_eq!(
            description(&at(custom, None, mods), (&gmtx, &months), extra),
            "Freestyle Game\nDuration: 4 Days\nSave date: 8 October 2026\n\
             Mods: Kept, \u{1}FF0000Gone\u{1}FFFFFF"
        );
    }
}
