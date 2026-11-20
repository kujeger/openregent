//! The settings editor: every setting the page lists, a control by its
//! kind. From the main menu's Adjust Settings and the game's options;
//! drawn in the original's art by `menu::settings`.
//!
//! ```text
//!  Settings ──cloned──> Draft (typed Settings)
//!     ^                   │ an edit: Draft::set, through the key's visitor
//!     └── applied at once ┘
//!  Save: written (config::save)   Cancel, Escape: the opened settings back
//! ```
//!
//! The keys are the settings' own (`files::setting`), so a new one shows
//! up unasked. The page shows them on two tabs (`menu::settings::Tab`), by
//! topic: `[enhancements]` on its own, under a preset list
//! ([`Draft::pick`]); the rest on the other. Its control comes from its
//! kind: a bool a check box; a number its plus and minus buttons and a
//! value dragged, in its range; an enum a list of its choices.

use std::ops::DerefMut;

use bevy::prelude::Resource;
use bevy_egui::egui;

use crate::files::config::{self, Preset, Settings};
use crate::files::setting::{Key, Meta, Now, Part, Setting, Visit, VisitMut};
use crate::menu::Art;
use crate::menu::settings::{self as page, Look, Tab};
use regent_draw::art as service;

/// The settings being edited.
#[derive(Clone, Debug)]
pub struct Draft {
    /// As opened: Cancel's.
    opened: Settings,
    now: Settings,
    /// Every key, sorted by its shown name.
    keys: Vec<Key>,
    /// Why the settings did not save.
    error: Option<String>,
}

impl Draft {
    /// `settings` to edit; a number out of its range comes in clamped.
    #[must_use]
    pub fn new(settings: &Settings) -> Draft {
        let mut now = settings.clone();
        let mut keys = Keys::default();

        now.visit_mut(&mut Clamp);
        now.visit(&mut keys);
        keys.0.sort_by_key(|k| k.meta.name);

        Draft {
            opened: settings.clone(),
            now,
            keys: keys.0,
            error: None,
        }
    }

    /// The keys, sorted by their shown names.
    #[must_use]
    pub fn keys(&self) -> &[Key] {
        &self.keys
    }

    /// The settings it holds.
    #[must_use]
    pub const fn settings(&self) -> &Settings {
        &self.now
    }

    /// Every key back to its default.
    pub fn defaults(&mut self) {
        self.now = Settings::default();
    }

    /// The preset the enhancements are.
    #[must_use]
    pub fn preset(&self) -> Preset {
        self.now.enhancements.preset()
    }

    /// `preset` picked: its values set.
    pub fn pick(&mut self, preset: Preset) {
        self.now.enhancements = self.now.enhancements.picked(preset);
    }

    /// `key`'s value now.
    #[must_use]
    pub fn value(&self, key: Key) -> Option<Now> {
        of(&self.now, key)
    }

    /// `key`'s value by default.
    #[must_use]
    pub fn default_of(key: Key) -> Option<Now> {
        of(&Settings::default(), key)
    }

    /// Why the settings did not save.
    #[must_use]
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// `key` set to `now`, a number held to its range.
    pub fn set(&mut self, key: Key, now: Now) {
        self.now.visit_mut(&mut Put(key, now));
    }
}

/// `key`'s value in `settings`.
fn of(settings: &Settings, key: Key) -> Option<Now> {
    let mut get = Get(key, None);

    settings.visit(&mut get);
    get.1
}

/// Every key, as the settings hand them over.
#[derive(Default)]
struct Keys(Vec<Key>);

impl Visit for Keys {
    fn field<T: Setting>(&mut self, part: &'static Part, meta: &'static Meta, _: &T) {
        self.0.push(Key { part, meta });
    }
}

/// Key `.0`'s value, once found.
struct Get(Key, Option<Now>);

impl Visit for Get {
    fn field<T: Setting>(&mut self, part: &'static Part, meta: &'static Meta, value: &T) {
        if self.0 == (Key { part, meta }) {
            self.1 = Some(value.now());
        }
    }
}

/// Key `.0` set to `.1`.
struct Put(Key, Now);

impl VisitMut for Put {
    fn field<T: Setting>(&mut self, part: &'static Part, meta: &'static Meta, value: &mut T) {
        if self.0 == (Key { part, meta }) {
            value.set(&self.1, meta.range.as_ref());
        }
    }
}

/// Every number held to its range.
struct Clamp;

impl VisitMut for Clamp {
    fn field<T: Setting>(&mut self, _: &'static Part, meta: &'static Meta, value: &mut T) {
        value.set(&value.now(), meta.range.as_ref());
    }
}

/// How the page closed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Close {
    Saved,
    Cancelled,
}

/// A pass of the open page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pass {
    Open,
    Closed(Close),
}

/// The editor: a draft and how its page shows, while open; the tab it
/// opens on.
#[derive(Resource, Default)]
pub struct Editor(Option<(Draft, Look)>, Tab);

impl Editor {
    /// Closed, to open on `tab`: a screenshot's.
    #[must_use]
    pub const fn at(tab: Tab) -> Editor {
        Editor(None, tab)
    }

    /// Whether the window is open.
    #[must_use]
    pub const fn is_open(&self) -> bool {
        self.0.is_some()
    }

    /// Open on `settings`, unless open.
    pub fn open(&mut self, settings: &Settings) {
        let tab = self.1;

        self.0
            .get_or_insert_with(|| (Draft::new(settings), Look::on(tab)));
    }

    /// The page while open, over the menus or a game, each edit
    /// applied to `settings`: whether it closed, and how.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        art: (&mut Art, &mut service::Art),
        settings: &mut impl DerefMut<Target = Settings>,
    ) -> Option<Pass> {
        let (draft, look) = self.0.as_mut()?;
        let before = draft.now.clone();
        let pressed = page::page(ctx, art, (draft, look));

        // An edit: applied at once, and a save's refusal said no more.
        if draft.now != before {
            draft.error = None;
            apply(settings, draft.now.clone());
        }

        let closed = match pressed {
            Some(Press::Save) => match config::save(settings) {
                Ok(()) => Some(Close::Saved),
                Err(e) => {
                    draft.error = Some(format!("not saved: {e}"));
                    None
                }
            },
            Some(Press::Cancel) => {
                apply(settings, draft.opened.clone());
                Some(Close::Cancelled)
            }
            Some(Press::Defaults) => {
                draft.error = None;
                draft.defaults();
                apply(settings, Settings::default());
                None
            }
            None => None,
        };

        if closed.is_some() {
            self.0 = None;
        }

        Some(closed.map_or(Pass::Open, Pass::Closed))
    }
}

/// The main menu's settings page: opened on `settings` if not yet, and
/// shown; how it closed, if it did.
pub fn page(
    ctx: &egui::Context,
    editor: &mut Editor,
    art: (&mut Art, &mut service::Art),
    settings: &mut impl DerefMut<Target = Settings>,
) -> Option<Close> {
    editor.open(settings);
    match editor.show(ctx, art, settings) {
        Some(Pass::Closed(close)) => Some(close),
        _ => None,
    }
}

/// `settings` set to `now`; untouched, so unmarked, when the same.
fn apply(settings: &mut impl DerefMut<Target = Settings>, now: Settings) {
    if **settings != now {
        **settings = now;
    }
}

/// A button of the page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Press {
    Save,
    Defaults,
    Cancel,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::config::HpBars;
    use crate::files::setting::Whole;

    fn key(draft: &Draft, section: &str, name: &str) -> Key {
        *draft
            .keys()
            .iter()
            .find(|k| k.part.name == section && k.meta.key == name)
            .expect("the key")
    }

    #[test]
    fn the_settings_survive_a_round_trip_through_the_editor() {
        let mut settings = Settings::default();

        settings.ui.bars = HpBars::Never;
        settings.audio.music = 0.25;
        settings.enhancements.smooth_zoom = true;

        assert_eq!(Draft::new(&settings).settings(), &settings);
    }

    #[test]
    fn defaults_puts_every_key_back() {
        let mut settings = Settings::default();

        settings.audio.muted = true;
        settings.enhancements.debug_keys = true;

        let mut draft = Draft::new(&settings);

        draft.defaults();

        assert_eq!(draft.settings(), &Settings::default());
    }

    /// Picking a preset writes its values, other sections untouched;
    /// the draft reads as the preset from then.
    #[test]
    fn a_picked_preset_shows_in_the_draft() {
        let mut settings = Settings::default();

        settings.audio.muted = true;
        settings.enhancements.side_ring = false;

        let mut draft = Draft::new(&settings);

        assert_eq!(draft.preset(), Preset::Custom);

        draft.pick(Preset::Classic);

        assert_eq!(draft.preset(), Preset::Classic);
        assert!(draft.settings().audio.muted);
        assert_eq!(
            draft
                .value(key(&draft, "enhancements", "rings"))
                .map(|n| n.shown()),
            Some("Original art".to_owned())
        );

        draft.set(key(&draft, "enhancements", "speed_keys"), Now::Check(true));

        assert_eq!(draft.preset(), Preset::Custom);
    }

    #[test]
    #[allow(clippy::float_cmp, reason = "held exactly to its end")]
    fn a_loudness_is_held_from_0_to_1() {
        let mut settings = Settings::default();

        settings.audio.music = 3.0;

        let mut draft = Draft::new(&settings);

        assert_eq!(draft.settings().audio.music, 1.0);

        draft.set(
            key(&draft, "audio", "effects"),
            Now::Number(-2.0, Whole::Fraction),
        );

        assert_eq!(draft.settings().audio.effects, 0.0);
    }

    /// A choice picked by its place in the list.
    #[test]
    fn a_choice_is_set_by_its_place() {
        let mut draft = Draft::new(&Settings::default());
        let bars = key(&draft, "ui", "bars");

        assert_eq!(
            draft.value(bars),
            Some(Now::Choice(1, &["Always", "When hurt", "Never"]))
        );

        draft.set(bars, Now::Choice(2, &[]));

        assert_eq!(draft.settings().ui.bars, HpBars::Never);
    }

    #[test]
    fn each_key_is_edited_by_its_kind() {
        let draft = Draft::new(&Settings::default());
        let now = |section: &str, name: &str| draft.value(key(&draft, section, name));

        assert_eq!(now("audio", "muted"), Some(Now::Check(false)));
        assert_eq!(
            now("audio", "music"),
            Some(Now::Number(1.0, Whole::Fraction))
        );
        assert_eq!(
            now("ui", "scroll_speed"),
            Some(Now::Number(32.0, Whole::Integer))
        );
        assert!(matches!(now("ui", "font"), Some(Now::Choice(0, _))));
    }

    /// A row's name and text are its `#[setting(…)]`'s; without `name`,
    /// the key in words.
    #[test]
    fn the_rows_take_their_settings_text() {
        let draft = Draft::new(&Settings::default());
        let text = |section: &str, name: &str| {
            let k = key(&draft, section, name);

            (k.meta.name, k.meta.about)
        };

        assert_eq!(text("ui", "bars").0, "Hit point bars");
        assert_eq!(text("ui", "wasd_scroll").0, "WASD scroll");
        assert_eq!(text("ui", "font").0, "Font");
        assert_eq!(
            text("audio", "muted"),
            ("Mute", "Silence all sound. The volumes are kept.")
        );
    }

    /// The HD install's path is the file's but never edited here: a bad
    /// value would stop openregent starting.
    #[test]
    fn the_install_path_is_not_listed() {
        let draft = Draft::new(&Settings::default());

        assert!(draft.keys().iter().all(|k| k.part.name != config::PATHS));
    }
}
