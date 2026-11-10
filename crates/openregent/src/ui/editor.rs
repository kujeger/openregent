//! The settings editor: every key of the settings file, a control by
//! its value. From the main menu's Adjust Settings and the game's
//! options; drawn in the original's art by `menu::settings`.
//!
//! ```text
//!  Settings ──as the file writes it──> Draft (toml table)
//!     ^                                  │ an edit that reads as settings
//!     └──────── applied at once ─────────┘
//!  Save: written (config::save)   Cancel, Escape: the opened settings back
//! ```
//!
//! The keys are those the file writes, so a new one shows up unasked;
//! `[[mod]]` is the mods page's (`menu::mods`). The page shows them on
//! two tabs (`menu::settings::Tab`), by topic: `[enhancements]` on its own,
//! under a preset list ([`Draft::pick`]); the rest on the other.
//! Its control comes from its value: a bool a check box; a number its
//! plus and minus buttons and a value dragged, in its [`RANGES`] range;
//! a string a list of the names the settings take, when they refuse any
//! other (an enum), else a text field. The docs are read from
//! `config.rs`'s source.

use std::collections::HashMap;
use std::ops::{DerefMut, RangeInclusive};

use bevy::prelude::Resource;
use bevy_egui::egui;
use toml::{Table, Value};

use crate::files::config::{self, Preset, RANGES, Settings};
use crate::menu::Art;
use crate::menu::settings::{self as page, Look, Tab};
use regent_draw::art as service;

/// Where the docs are read from.
const SOURCE: &str = include_str!("../files/config.rs");

/// The struct that holds the sections.
const ROOT: &str = "Settings";

/// A name no enum has: what a key's names are asked with.
const PROBE: &str = "\u{1}";

/// How a key is edited.
#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Check,
    /// In its range, where the code holds it to one.
    Number(Option<RangeInclusive<f64>>),
    /// One of an enum's names.
    Choice(Vec<String>),
    Text,
    /// Shown, not edited: an array or table.
    Fixed,
}

/// A key of the file.
#[derive(Clone, Debug, PartialEq)]
pub struct Field {
    pub section: String,
    pub key: String,
    pub doc: String,
    pub kind: Kind,
}

/// The settings being edited.
#[derive(Clone, Debug)]
pub struct Draft {
    /// As opened: Cancel's.
    opened: Settings,
    /// Section to key to value, as the file writes them.
    table: Table,
    fields: Vec<Field>,
    /// Section docs, by section.
    headings: HashMap<String, String>,
    /// The defaults, as the file writes them.
    defaults: Table,
    /// Why the table does not read as settings, or will not save.
    error: Option<String>,
}

impl Draft {
    /// `settings` to edit; a number out of its range comes in clamped.
    #[must_use]
    pub fn new(settings: &Settings) -> Draft {
        let docs = Docs::read(SOURCE);
        let defaults = table(&Settings::default());
        let table = table(settings);
        let fields = fields(&table, &docs);
        let headings = table
            .keys()
            .map(|s| (s.clone(), docs.heading(s).to_owned()))
            .collect();
        let mut draft = Draft {
            opened: settings.clone(),
            table,
            fields,
            headings,
            defaults,
            error: None,
        };

        draft.clamp();
        draft
    }

    /// The keys, sorted by section, then key.
    #[must_use]
    pub fn fields(&self) -> &[Field] {
        &self.fields
    }

    /// The settings it holds, or why it does not read as settings.
    ///
    /// # Errors
    ///
    /// A value the settings do not take.
    pub fn settings(&self) -> Result<Settings, String> {
        read(&self.table)
    }

    /// Every key back to its default.
    pub fn defaults(&mut self) {
        self.table = self.defaults.clone();
    }

    /// The preset the enhancements are, while the draft reads.
    #[must_use]
    pub fn preset(&self) -> Option<Preset> {
        self.settings().ok().map(|s| s.enhancements.preset())
    }

    /// `preset` picked: its values in the table.
    pub fn pick(&mut self, preset: Preset) {
        if let Ok(mut settings) = self.settings() {
            settings.enhancements = settings.enhancements.picked(preset);
            self.table = table(&settings);
        }
    }

    /// `key`'s value now.
    #[must_use]
    pub fn value(&self, section: &str, key: &str) -> Option<&Value> {
        self.table.get(section)?.get(key)
    }

    /// `key`'s value by default.
    #[must_use]
    pub fn default_of(&self, section: &str, key: &str) -> Option<&Value> {
        self.defaults.get(section)?.get(key)
    }

    /// `section`'s doc.
    #[must_use]
    pub fn heading(&self, section: &str) -> &str {
        self.headings.get(section).map_or("", String::as_str)
    }

    /// Why the draft does not read as settings, or did not save.
    #[must_use]
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Number `key` set to `x`, whole where it was, held to its range.
    pub fn set_number(&mut self, section: &str, key: &str, x: f64) {
        if let Some(old) = self.value(section, key) {
            let value = renumbered(old, x);

            self.set(section, key, value);
        }
    }

    /// `key` set to `value`, a number clamped to its range.
    pub fn set(&mut self, section: &str, key: &str, value: Value) {
        let kind = self
            .fields
            .iter()
            .find(|f| f.section == section && f.key == key)
            .map(|f| &f.kind);
        let value = match kind {
            Some(Kind::Number(Some(range))) => clamped(&value, range),
            _ => value,
        };

        put(&mut self.table, section, key, value);
    }

    /// Every number in its range.
    fn clamp(&mut self) {
        let ranged: Vec<(String, String)> = self
            .fields
            .iter()
            .filter(|f| matches!(f.kind, Kind::Number(Some(_))))
            .map(|f| (f.section.clone(), f.key.clone()))
            .collect();

        for (section, key) in ranged {
            if let Some(value) = self.value(&section, &key).cloned() {
                self.set(&section, &key, value);
            }
        }
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
        let before = draft.table.clone();
        let pressed = page::page(ctx, art, (draft, look));

        draft.error = draft.settings().err();

        if draft.table != before
            && let Ok(now) = draft.settings()
        {
            apply(settings, now);
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

/// `value` as the page shows it: a number to two places, its trailing
/// zeros cut; a bool on or off.
#[must_use]
pub fn shown(value: &Value) -> String {
    match value {
        Value::Boolean(true) => "on".to_owned(),
        Value::Boolean(false) => "off".to_owned(),
        Value::String(s) => s.clone(),
        Value::Float(x) => {
            let text = format!("{x:.2}");

            text.trim_end_matches('0').trim_end_matches('.').to_owned()
        }
        other => other.to_string(),
    }
}

/// `settings` as the file writes them.
fn table(settings: &Settings) -> Table {
    Table::try_from(settings).unwrap_or_default()
}

/// `table` read as settings.
fn read(table: &Table) -> Result<Settings, String> {
    table
        .clone()
        .try_into()
        .map_err(|e: toml::de::Error| e.message().to_owned())
}

/// `table`'s `key` in `section` set to `value`.
fn put(table: &mut Table, section: &str, key: &str, value: Value) {
    if let Some(Value::Table(keys)) = table.get_mut(section) {
        keys.insert(key.to_owned(), value);
    }
}

/// Each key of `table`'s sections, its doc from `docs`.
fn fields(table: &Table, docs: &Docs) -> Vec<Field> {
    table
        .iter()
        .filter_map(|(section, keys)| keys.as_table().map(|keys| (section, keys)))
        .flat_map(|(section, keys)| keys.iter().map(move |(key, value)| (section, key, value)))
        .map(|(section, key, value)| Field {
            section: section.clone(),
            key: key.clone(),
            doc: docs.field(section, key).to_owned(),
            kind: kind(table, section, key, value),
        })
        .collect()
}

/// How `key`, holding `value`, is edited.
fn kind(table: &Table, section: &str, key: &str, value: &Value) -> Kind {
    match value {
        Value::Boolean(_) => Kind::Check,
        Value::Integer(_) | Value::Float(_) => Kind::Number(range(section, key)),
        Value::String(_) => names(table, section, key).map_or(Kind::Text, Kind::Choice),
        _ => Kind::Fixed,
    }
}

/// `key`'s range, from [`RANGES`].
fn range(section: &str, key: &str) -> Option<RangeInclusive<f64>> {
    RANGES
        .iter()
        .find(|(s, k, _)| *s == section && *k == key)
        .map(|(_, _, r)| f64::from(*r.start())..=f64::from(*r.end()))
}

/// An enum key's names, from serde's refusal of one it lacks: "unknown
/// variant `x`, expected one of `a`, `b`" (or "expected `a` or `b`").
/// `None` when the settings take any string.
fn names(table: &Table, section: &str, key: &str) -> Option<Vec<String>> {
    let mut probed = table.clone();

    put(&mut probed, section, key, Value::String(PROBE.to_owned()));

    let refused = read(&probed).err()?;
    let (_, expected) = refused.split_once("expected")?;
    let names: Vec<String> = expected
        .split('`')
        .skip(1)
        .step_by(2)
        .map(str::to_owned)
        .collect();

    (!names.is_empty()).then_some(names)
}

/// A number's value.
#[allow(clippy::cast_precision_loss, reason = "a setting's number")]
pub const fn number(value: &Value) -> Option<f64> {
    match value {
        Value::Float(x) => Some(*x),
        Value::Integer(n) => Some(*n as f64),
        _ => None,
    }
}

/// `x` in `old`'s type: a whole number stays whole.
#[allow(clippy::cast_possible_truncation, reason = "a setting's number")]
const fn renumbered(old: &Value, x: f64) -> Value {
    match old {
        Value::Integer(_) => Value::Integer(x.round() as i64),
        _ => Value::Float(x),
    }
}

/// `value` in `range`; not a number, unchanged.
fn clamped(value: &Value, range: &RangeInclusive<f64>) -> Value {
    number(value).map_or_else(
        || value.clone(),
        |x| renumbered(value, x.clamp(*range.start(), *range.end())),
    )
}

/// The doc comments of a source's structs and their fields.
#[derive(Debug, Default)]
struct Docs {
    /// A struct's own, by name.
    structs: HashMap<String, String>,
    /// A field's type and doc, by struct and field.
    fields: HashMap<(String, String), (String, String)>,
}

impl Docs {
    /// `source`'s, read line by line: `///` lines gather until a struct
    /// or field takes them; a blank line, `}` or a `fn` drops them.
    fn read(source: &str) -> Docs {
        let mut docs = Docs::default();
        let mut pending: Vec<&str> = Vec::new();
        let mut within: Option<&str> = None;

        for line in source.lines().map(str::trim) {
            if let Some(text) = line.strip_prefix("///") {
                pending.push(text.trim());
            } else if let Some(name) = struct_name(line) {
                docs.structs.insert(name.to_owned(), prose(&pending));
                within = Some(name);
                pending.clear();
            } else if let (Some(owner), Some((field, ty))) = (within, field(line)) {
                let at = (owner.to_owned(), field.to_owned());

                docs.fields.insert(at, (ty.to_owned(), prose(&pending)));
                pending.clear();
            } else if line.is_empty() || line.starts_with('}') || line.contains("fn ") {
                within = within.filter(|_| !line.starts_with('}'));
                pending.clear();
            }
        }

        docs
    }

    /// The struct behind `section`: [`ROOT`]'s field's type.
    fn section(&self, section: &str) -> Option<&str> {
        self.fields
            .get(&(ROOT.to_owned(), section.to_owned()))
            .map(|(ty, _)| ty.as_str())
    }

    /// `section`'s doc: its struct's.
    fn heading(&self, section: &str) -> &str {
        self.section(section)
            .and_then(|ty| self.structs.get(ty))
            .map_or("", String::as_str)
    }

    /// `key`'s doc in `section`.
    fn field(&self, section: &str, key: &str) -> &str {
        self.section(section)
            .and_then(|ty| self.fields.get(&(ty.to_owned(), key.to_owned())))
            .map_or("", |(_, doc)| doc.as_str())
    }
}

/// The name a `struct` line opens: `pub struct Audio {` is `Audio`.
fn struct_name(line: &str) -> Option<&str> {
    let rest = line.strip_prefix("pub ").unwrap_or(line);
    let name = rest.strip_prefix("struct ")?.strip_suffix('{')?.trim();

    ident(name).then_some(name)
}

/// A field line's name and type: `pub music: f32,` is `music`, `f32`.
fn field(line: &str) -> Option<(&str, &str)> {
    let rest = line.strip_prefix("pub ").unwrap_or(line);
    let (name, ty) = rest.strip_suffix(',')?.split_once(':')?;

    ident(name).then(|| (name, ty.trim()))
}

/// Whether `s` is a Rust identifier, near enough.
fn ident(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Doc lines as one paragraph, without rustdoc's link brackets.
fn prose(lines: &[&str]) -> String {
    lines.join(" ").replace("[`", "`").replace("`]", "`")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::config::HpBars;

    fn kind_of(draft: &Draft, section: &str, key: &str) -> Option<Kind> {
        draft
            .fields()
            .iter()
            .find(|f| f.section == section && f.key == key)
            .map(|f| f.kind.clone())
    }

    #[test]
    fn the_settings_survive_a_round_trip_through_the_editor() {
        let mut settings = Settings::default();

        settings.ui.bars = HpBars::Never;
        settings.audio.music = 0.25;
        settings.enhancements.smooth_zoom = true;

        assert_eq!(Draft::new(&settings).settings(), Ok(settings));
    }

    #[test]
    fn defaults_puts_every_key_back() {
        let mut settings = Settings::default();

        settings.audio.muted = true;
        settings.enhancements.debug_keys = true;

        let mut draft = Draft::new(&settings);

        draft.defaults();

        assert_eq!(draft.settings(), Ok(Settings::default()));
    }

    /// Picking a preset writes its values, other sections untouched;
    /// the draft reads as the preset from then.
    #[test]
    fn a_picked_preset_shows_in_the_draft() {
        let mut settings = Settings::default();

        settings.audio.muted = true;
        settings.enhancements.side_ring = false;

        let mut draft = Draft::new(&settings);

        assert_eq!(draft.preset(), Some(Preset::Custom));

        draft.pick(Preset::Classic);

        assert_eq!(draft.preset(), Some(Preset::Classic));
        assert_eq!(draft.value("audio", "muted"), Some(&Value::Boolean(true)));
        assert_eq!(
            draft.value("enhancements", "rings"),
            Some(&Value::String("art".to_owned()))
        );

        draft.set("enhancements", "speed_keys", Value::Boolean(true));

        assert_eq!(draft.preset(), Some(Preset::Custom));
    }

    #[test]
    fn a_loudness_is_held_from_0_to_1() {
        let mut settings = Settings::default();

        settings.audio.music = 3.0;

        let mut draft = Draft::new(&settings);

        assert_eq!(draft.value("audio", "music"), Some(&Value::Float(1.0)));

        draft.set("audio", "effects", Value::Float(-2.0));

        assert_eq!(draft.settings().map(|s| s.audio.effects), Ok(0.0));
    }

    #[test]
    fn each_key_is_edited_by_its_type() {
        let draft = Draft::new(&Settings::default());
        let choice = |names: &[&str]| {
            Some(Kind::Choice(
                names.iter().map(|n| (*n).to_owned()).collect(),
            ))
        };

        assert_eq!(
            kind_of(&draft, "ui", "bars"),
            choice(&["always", "damaged", "never"])
        );
        assert_eq!(
            kind_of(&draft, "ui", "names"),
            choice(&["off", "on", "heroes"])
        );
        assert_eq!(
            kind_of(&draft, "ui", "main_menu"),
            choice(&["base", "expansion"])
        );
        assert_eq!(
            kind_of(&draft, "ui", "player_list"),
            choice(&["off", "players", "details"])
        );
        assert_eq!(kind_of(&draft, "ui", "intro"), choice(&["on", "off"]));
        assert_eq!(kind_of(&draft, "ui", "font"), choice(&["inknut", "egui"]));
        assert_eq!(
            kind_of(&draft, "audio", "music"),
            Some(Kind::Number(Some(0.0..=1.0)))
        );
        assert_eq!(kind_of(&draft, "audio", "muted"), Some(Kind::Check));
        assert_eq!(
            kind_of(&draft, "enhancements", "speed_keys"),
            Some(Kind::Check)
        );
        assert_eq!(
            kind_of(&draft, "enhancements", "ui_upscale"),
            choice(&["none", "xbrz"])
        );
        assert_eq!(
            kind_of(&draft, "enhancements", "rings"),
            choice(&["art", "native", "modern"])
        );
    }

    #[test]
    fn every_key_the_file_writes_is_listed() {
        let draft = Draft::new(&Settings::default());
        let text = toml::to_string(&Settings::default()).expect("writes");
        let written = text.lines().filter(|l| l.contains(" = ")).count();

        // The mod list is not a setting: the mods page writes it.
        assert_eq!(draft.fields().len(), written);
        assert!(!text.contains("[mod"), "{text}");
    }

    /// The HD install's path is the file's but never edited here: a bad
    /// value would stop openregent starting.
    #[test]
    fn the_install_path_is_not_listed() {
        let draft = Draft::new(&Settings::default());

        assert!(
            draft
                .fields()
                .iter()
                .all(|f| f.section != config::PATHS && f.key != config::HD_KEY)
        );
    }

    #[test]
    fn the_docs_come_from_the_doc_comments() {
        let draft = Draft::new(&Settings::default());
        let doc = |section: &str, key: &str| {
            draft
                .fields()
                .iter()
                .find(|f| f.section == section && f.key == key)
                .map(|f| f.doc.clone())
        };

        assert_eq!(
            doc("ui", "bars").as_deref(),
            Some("Which units show a hit point bar on the map.")
        );
        assert_eq!(
            doc("enhancements", "debug_keys").as_deref(),
            Some(
                "Debug helpers: L lifts the fog, O shows the models of buildings casting from one in place of their sprites."
            )
        );
        assert_eq!(doc("audio", "music").as_deref(), Some(""));
        assert_eq!(
            draft.headings.get("ui").map(String::as_str),
            Some("The interface.")
        );
    }

    #[test]
    fn a_name_the_enum_lacks_does_not_read() {
        let mut draft = Draft::new(&Settings::default());

        draft.set("ui", "bars", Value::String("sometimes".to_owned()));

        assert!(draft.settings().is_err());
    }
}
