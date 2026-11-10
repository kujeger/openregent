//! Game text from `STRT` tables.
//!
//! `crate::data::stat::Stat::Activity` indexes `STRT AITX` in `gpltext.cam` (177
//! sentences). The four codes labelled from saves match — see
//! `the_labelled_activities_are_the_ones_the_game_ships`.
//!
//! ```text
//! AITX[4]   Going home
//! AITX[34]  Resting at home
//! AITX[36]  Berserking
//! AITX[45]  Purchasing armor at the Blacksmith
//! ```
//!
//! ## The tables
//!
//! | Table | Keyed by | Holds |
//! |---|---|---|
//! | `AITX` | index | What a character is doing |
//! | `QUES` | quest id | Quest names, the join `QDEP` needed |
//! | `DPTX` | building id | A building's prerequisites, in English |
//! | `FNTX` | scenario id | Freestyle scenario names |
//! | `FDTX` | scenario id | The same scenarios, described |
//! | `LNTX` | land id | The names of the lands |
//! | `QITM` | index | Items, name and effect |
//! | `HPTX` | `HelpID` | Help topics |
//! | `UNTN`/`ACTN` | descriptor id | Display names — **HD only**; [`crate::data::desc::Head::title`] agrees |
//!
//! Two key conventions: a printable four-character id (`QUES`, via
//! [`Text::keyed`]) or a `u32` index in the same bytes (`AITX`, via
//! [`Text::at`]).
//!
//! ## Where the tables live
//!
//! Across archives and editions. `gpltext.cam` has `AITX`, `QUES` and `DPTX`;
//! `mx_rgstext.cam` has the expansion's `FNTX`, `FDTX` and `LNTX` (81 base
//! entries grow to 137). Hence a whole [`Catalogue`]; a later table shadows an
//! earlier one.
//!
//! Except [`MERGED`]'s three, which the game merges across archives: the
//! first archive's table, then each later one's keys it lacks.
//! So the expansion's menus name `MD01`, held by
//! `gpltext.cam`'s `QUES` alone, and `XQ01`, by `mx_gpltext.cam`'s.
//!
//! ## Strings
//!
//! HD adds a dictionary, the `<Strings>` XML files a dataset, quest or mod
//! lists ([`Strings`]): `Data/InterfaceStrings.xml`, a quest's win and
//! loss messages, a mod's names and help. Each lookup that misses its
//! table asks it. **Binary**:
//!
//! ```text
//!  GMTX index past the table   gmtx string          by the index
//!  AITX code past the table    activity text        by the code
//!  HPTX HelpID not keyed       HelpController       by the HelpID's word
//!  a unit's title              panel name           by the title's string_id
//!  an item's title             item name            the same
//!  the end verdict, step 2     quest win text       IDTXT_QUEST_WIN_MESSAGE
//! ```
//!
//! Where: [`crate::view::panel::portrait::title`],
//! [`crate::view::panel::portrait::details::item_name`], [`crate::view::menu::end::verdict`].
//!
//! ## Markup
//!
//! HD's coloured text object (text rich set sets, text rich insert
//! appends) passes a string through text markup tag,
//! which drops the markup and keeps each colour change's
//! place. Every dialog item's text, text box, tooltip, unit list row and
//! name label goes through it. **Binary**:
//!
//! ```text
//!  0x01 RRGGBB         the text's colour from here (up to six hex digits)
//!  0x02 RRGGBB         the background's (not read here: no string has one)
//!  [color:V] [fcolor:V]  push V, the text's colour from here
//!  [/color] [/fcolor]    pop; empty, the caller's colour, else the top
//!  [bcolor:V] [/bcolor]  the background's own stack (not drawn here)
//!  [other...]            no tag: the '[' is a character
//!
//!  name   case folded, trimmed; ':' or '=' before the value
//!  V      trimmed; one of [`NAMED_COLOURS`] (case folded), else
//!         strtoul("0x" V): leading hex digits, none 0
//!  no ']' after the value: the rest of the text is the value
//! ```
//!
//! [`spans`] reads it; [`plain`] drops it; [`untagged`] drops the tags
//! alone. The info box, the chat lines, reads `0x01`, `0x02`, CR, LF and
//! `0x11` and no tag.

use std::borrow::Cow;
use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet};

use crate::data::FourCc;
use crate::data::fourcc::string_id;
use crate::data::payload::StringTable;
use crate::data::xml::Element;

use super::catalogue::Catalogue;

/// Every `STRT` table in a catalogue, parsed, and the XML strings.
#[derive(Debug, Clone, Default)]
pub struct Text<'a> {
    tables: BTreeMap<FourCc, StringTable<'a>>,
    /// A [`MERGED`] table's later archives' copies, in archive order;
    /// `tables` holds its first.
    merged: BTreeMap<FourCc, Vec<StringTable<'a>>>,
    strings: Strings,
}

/// The `<Strings>` files' dictionary: text by key, the id read by
/// [`string_id`], so `IDTXT_QUEST_WIN_MESSAGE` a CRC-32
/// and `350` the number.
///
/// **Binary**: under `<Majesty>`, a `<Language id=>`
/// gives its children its language; a `<Text id=>` takes its own `lang`,
/// else the one around it, else the file's, `en_US`. A later file or
/// entry replaces an earlier key, so a mod's string
/// replaces the install's; its text stored [`unescaped`]. Kept: `en_US`, the language the install
/// shows (**Inferred**: no other is chosen).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Strings(BTreeMap<u32, String>);

/// The language kept, HD's first.
const LANGUAGE: &str = "en_US";

impl Strings {
    /// Add one file's entries, replacing keys already held.
    pub fn add(&mut self, root: &Element<'_>) {
        if root.name == "Majesty" {
            self.walk(root, true);
        }
    }

    /// `element`'s children, `kept` whether its language is `en_US`.
    fn walk(&mut self, element: &Element<'_>, kept: bool) {
        for child in &element.children {
            match child.name {
                "Language" => {
                    if let Some(id) = child.attribute("id") {
                        self.walk(child, id == LANGUAGE);
                    }
                }
                "Text" => {
                    let lang = child.attribute("lang").map_or(kept, |l| l == LANGUAGE);

                    if let Some(id) = child.attribute("id").filter(|_| lang) {
                        self.0
                            .insert(string_id(id).cast_unsigned(), unescaped(&child.text));
                    }
                }
                _ => {}
            }
        }
    }

    /// The text under `key`.
    #[must_use]
    pub fn get(&self, key: u32) -> Option<&str> {
        self.0.get(&key).map(String::as_str)
    }

    /// The text under `id`'s key, text override get.
    #[must_use]
    pub fn of(&self, id: &str) -> Option<&str> {
        self.get(string_id(id).cast_unsigned())
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// The interface's sentences and labels, by index.
pub const GMTX: &[u8; 4] = b"GMTX";

/// What a character is doing, by the code [`crate::data::stat::Stat::Activity`]
/// holds.
pub const ACTIVITY: &[u8; 4] = b"AITX";

/// Quest names, keyed by the id `QDEP` writes its rules over.
const QUEST: &[u8; 4] = b"QUES";

/// The tables the game merges across archives rather than taking the
/// last: `QUES`, `LOSX` and `QEND`, which the game registers at start in
/// the set its `STRT` loader reads.
pub const MERGED: [&[u8; 4]; 3] = [QUEST, b"LOSX", b"QEND"];

/// A building's prerequisites in English, keyed by its `DUNT` id.
const PREREQUISITE: &[u8; 4] = b"DPTX";

/// Freestyle scenario names, and [`SCENARIO_DETAIL`] the descriptions.
const SCENARIO: &[u8; 4] = b"FNTX";
const SCENARIO_DETAIL: &[u8; 4] = b"FDTX";

/// The names of the lands a freestyle game is played in.
const LAND: &[u8; 4] = b"LNTX";

/// Items a hero can carry, by the index an `INV:n` key holds.
const ITEM: &[u8; 4] = b"QITM";

/// Help topics, by the `HelpID` a unit or building descriptor names.
const HELP: &[u8; 4] = b"HPTX";

/// Special events, and [`EVENT_DETAIL`] what each one does.
const EVENT: &[u8; 4] = b"ENTX";
const EVENT_DETAIL: &[u8; 4] = b"EDTX";

/// One string of a position-keyed table: `GMTX` 7 is `Ref::gmtx(7)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ref {
    pub table: FourCc,
    pub index: usize,
}

impl Ref {
    #[must_use]
    pub const fn new(table: &[u8; 4], index: usize) -> Ref {
        Ref {
            table: FourCc(*table),
            index,
        }
    }

    /// The `index`th interface string, in [`GMTX`].
    #[must_use]
    pub const fn gmtx(index: usize) -> Ref {
        Ref::new(GMTX, index)
    }
}

impl<'a> Text<'a> {
    /// Index every `STRT` in the catalogue.
    ///
    /// Unparseable tables are skipped. On duplicates the last wins, so the
    /// expansion's `FNTX` shadows the base game's; a [`MERGED`] table's
    /// first stays, the later ones kept behind it.
    #[must_use]
    pub fn build(catalogue: &Catalogue<'a>) -> Self {
        let mut tables = BTreeMap::new();
        let mut merged: BTreeMap<FourCc, Vec<StringTable<'a>>> = BTreeMap::new();

        for (_, record) in catalogue.tagged(b"STRT") {
            let Ok(table) = StringTable::parse(record.data) else {
                continue;
            };

            match tables.entry(record.id) {
                Entry::Occupied(_) if MERGED.contains(&&record.id.0) => {
                    merged.entry(record.id).or_default().push(table);
                }
                Entry::Occupied(mut held) => {
                    held.insert(table);
                }
                Entry::Vacant(free) => {
                    free.insert(table);
                }
            }
        }

        Text {
            tables,
            merged,
            strings: Strings::default(),
        }
    }

    /// The same, with `strings` for what the tables miss.
    #[must_use]
    pub fn with_strings(self, strings: Strings) -> Self {
        Text { strings, ..self }
    }

    /// The XML strings.
    #[must_use]
    pub const fn strings(&self) -> &Strings {
        &self.strings
    }

    /// A string-table miss's answer from the dictionary.
    fn fallback(&self, key: u32) -> Option<Cow<'a, str>> {
        self.strings.get(key).map(|s| Cow::Owned(s.to_owned()))
    }

    /// One whole table.
    #[must_use]
    pub fn table(&self, id: &[u8; 4]) -> Option<&StringTable<'a>> {
        self.tables.get(&FourCc::new(id))
    }

    /// The `index`th string of a position-keyed table; in a [`MERGED`]
    /// one, past the first archive's copy, each later one's keys it
    /// lacks, in archive order (**Inferred**: the game adds them at the
    /// end, strt load).
    #[must_use]
    pub fn at(&self, id: &[u8; 4], index: usize) -> Option<Cow<'a, str>> {
        self.row(FourCc::new(id), index)
    }

    /// The string `at` refers to.
    #[must_use]
    pub fn get(&self, at: Ref) -> Option<Cow<'a, str>> {
        self.row(at.table, at.index)
    }

    /// Table `id`'s `index`th string, as [`Text::at`] counts.
    fn row(&self, id: FourCc, index: usize) -> Option<Cow<'a, str>> {
        let first = self.tables.get(&id)?;

        match (index.checked_sub(first.len()), self.merged.get(&id)) {
            (Some(past), Some(later)) => {
                let mut held: BTreeSet<FourCc> = first.keys().collect();

                // The later copies' rows whose keys are not yet held.
                later
                    .iter()
                    .flat_map(|t| (0..t.len()).map(move |i| (t, i)))
                    .filter(|(t, i)| t.key(*i).is_some_and(|k| held.insert(k)))
                    .nth(past)
                    .and_then(|(t, i)| t.get(i))
            }
            _ => first.get(index),
        }
    }

    /// The `index`th interface string, from [`GMTX`]; past the table, the
    /// [`Strings`] entry of that number.
    #[must_use]
    pub fn gmtx(&self, index: usize) -> Option<Cow<'a, str>> {
        self.at(GMTX, index)
            .or_else(|| self.fallback(u32::try_from(index).ok()?))
    }

    /// The string under `key` in an id-keyed table; in a [`MERGED`] one,
    /// the first archive's copy holding it.
    #[must_use]
    pub fn keyed(&self, id: &[u8; 4], key: FourCc) -> Option<Cow<'a, str>> {
        let later = self.merged.get(&FourCc::new(id)).into_iter().flatten();

        self.table(id)
            .into_iter()
            .chain(later)
            .find_map(|t| t.get_by_key(key))
    }

    /// The sentence shown for an activity code; past the table, the
    /// [`Strings`] entry of that number (activity text,
    /// text override find code).
    #[must_use]
    pub fn activity(&self, code: i32) -> Option<Cow<'a, str>> {
        usize::try_from(code)
            .ok()
            .and_then(|i| self.at(ACTIVITY, i))
            .or_else(|| self.fallback(code.cast_unsigned()))
    }

    /// A quest's name, by the id `QDEP` and a save's `SVGD` both use.
    #[must_use]
    pub fn quest(&self, id: FourCc) -> Option<Cow<'a, str>> {
        self.keyed(QUEST, id)
    }

    /// A building's prerequisites as shown to the player.
    ///
    /// Prose form of the rule [`crate::data::deps`] parses — see
    /// `the_prose_agrees_with_the_rules_and_names_what_is_never_built`.
    #[must_use]
    pub fn prerequisite(&self, id: FourCc) -> Option<Cow<'a, str>> {
        self.keyed(PREREQUISITE, id)
    }

    /// A freestyle scenario's name, and [`Text::scenario_detail`] its blurb.
    #[must_use]
    pub fn scenario(&self, id: FourCc) -> Option<Cow<'a, str>> {
        self.keyed(SCENARIO, id)
    }

    #[must_use]
    pub fn scenario_detail(&self, id: FourCc) -> Option<Cow<'a, str>> {
        self.keyed(SCENARIO_DETAIL, id)
    }

    /// The name of a land.
    #[must_use]
    pub fn land(&self, id: FourCc) -> Option<Cow<'a, str>> {
        self.keyed(LAND, id)
    }

    /// An item, by the index an `INV:n` key holds.
    ///
    /// E.g. `INV:11` is `QITM[11]`, "Ring of Protection" — see
    /// **Measured**.
    ///
    /// Name, newline, coloured effect; [`spans`] splits, [`plain`] strips.
    #[must_use]
    pub fn item(&self, index: usize) -> Option<Cow<'a, str>> {
        self.at(ITEM, index)
    }

    /// A help topic, by the `HelpID` a descriptor names.
    ///
    /// All 142 characters and buildings resolve
    /// (`a_help_id_resolves_to_its_topic`). Not keyed, the [`Strings`]
    /// entry of the id's word: a
    /// mod's `HelpID value="MK_hPalace"`.
    #[must_use]
    pub fn help(&self, id: FourCc) -> Option<Cow<'a, str>> {
        self.keyed(HELP, id).or_else(|| self.fallback(id.word()))
    }

    /// A special event, and [`Text::event_detail`] what it does to a game.
    ///
    /// Keyed by the `RGS_SpecialEvent` slot code, `NONE` included.
    #[must_use]
    pub fn event(&self, id: FourCc) -> Option<Cow<'a, str>> {
        self.keyed(EVENT, id)
    }

    #[must_use]
    pub fn event_detail(&self, id: FourCc) -> Option<Cow<'a, str>> {
        self.keyed(EVENT_DETAIL, id)
    }

    /// Table count.
    #[must_use]
    pub fn len(&self) -> usize {
        self.tables.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tables.is_empty()
    }
}

/// One run of text in a single colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span<'a> {
    /// `None` before the first marker or tag, or past a tag's end: the
    /// caller's default colour.
    pub colour: Option<[u8; 3]>,
    pub text: &'a str,
}

/// Colour marker byte, followed by six hex digits.
const MARK: u8 = 0x01;

/// Colour marker length, byte included.
const MARK_LEN: usize = 7;

/// The tag names that set the text's colour (`"color"`, `"fcolor"`),
/// case folded.
const FORE_TAGS: [&str; 2] = ["color", "fcolor"];

/// The tag name that sets the background's (`"bcolor"`), case folded.
const BACK_TAG: &str = "bcolor";

/// What sets a tag's name off its value.
const TAG_SEPARATORS: [char; 2] = [':', '='];

/// `[/color]`'s mark of a closing tag.
const TAG_CLOSE: char = '/';

/// The table a tag's value is looked up in before it is read as hex,
/// tag colour names: 12 pairs of a name and `0xRRGGBB`, the first match
/// winning, case folded. The sixth entry repeats
/// `light_green` (both name pointers `"light_green"`), so its colour is never
/// found; **Inferred**: `dark_green` was meant.
pub const NAMED_COLOURS: [(&str, u32); 12] = [
    ("red", 0x00ff_0000),
    ("light_red", 0x00ff_8c8c),
    ("dark_red", 0x00a0_0000),
    ("green", 0x0000_ff00),
    ("light_green", 0x007c_ff77),
    ("light_green", 0x0002_9e00),
    ("blue", 0x0000_00ff),
    ("light_blue", 0x0075_aaff),
    ("dark_blue", 0x0000_06c1),
    ("yellow", 0x00ff_ff00),
    ("purple", 0x00b6_00ff),
    ("orange", 0x00ff_9d00),
];

/// What a marker or tag does to the colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Change {
    /// A `0x01` marker: the colour, no push.
    Mark([u8; 3]),
    /// `[color:V]`: V pushed.
    Open(u32),
    /// `[/color]`: one popped.
    Close,
    /// A `bcolor` tag: the background, not drawn here.
    Back,
}

/// Splits a string into coloured runs, its markup dropped: `0x01`
/// markers and colour tags, as text markup tag reads them (the
/// module's [Markup](crate::data::text#markup)).
///
/// The `STRT` tables' only markup is `0x01` and six ASCII hex digits, an
/// RGB colour until the next marker. **Corpus**: 422 of 422 across every
/// `STRT` in both datasets; the only other control bytes are `\n` and
/// `\r`. Tested by `the_only_markup_in_the_games_text_is_a_colour`. A
/// marker without its six digits ends the scan here.
///
/// Tags come from XML strings and scripts: "\[color:7070ff]Living in
/// the...[/color]". A tag's colour is drawn as its low three bytes.
/// **Inferred**: the top byte, which the renderer sets to `0x80`, is not
/// read further.
///
/// ```
/// use regent_core::data::text::spans;
///
/// let runs: Vec<_> = spans("a[color:red]b[/color]c").map(|s| (s.colour, s.text)).collect();
///
/// assert_eq!(runs, [(None, "a"), (Some([0xff, 0, 0]), "b"), (None, "c")]);
/// ```
pub fn spans(text: &str) -> impl Iterator<Item = Span<'_>> {
    let mut rest = text;
    let mut colour = None;
    let mut pushed: Vec<u32> = Vec::new();
    let mut done = false;

    std::iter::from_fn(move || {
        if done {
            return None;
        }

        // Up to the next marker or tag, or the rest.
        let (at, change) = next_change(rest);
        let here = Span {
            colour,
            text: &rest[..at],
        };

        match change {
            Some((used, change)) => {
                match change {
                    Change::Mark(c) => colour = Some(c),
                    Change::Open(v) => {
                        pushed.push(v);
                        colour = Some(rgb(v));
                    }
                    // An empty stack pops nothing and keeps the colour.
                    Change::Close => {
                        if pushed.pop().is_some() {
                            colour = pushed.last().copied().map(rgb);
                        }
                    }
                    Change::Back => {}
                }

                rest = &rest[at + used..];
            }
            None => done = true,
        }

        Some(here)
    })
}

/// Where in `text` the next marker or tag is, and its length and
/// change; `None` at the end, or at a marker without its colour.
fn next_change(text: &str) -> (usize, Option<(usize, Change)>) {
    let mut from = 0;

    loop {
        let Some(at) = text[from..].find([MARK as char, '[']).map(|i| from + i) else {
            return (text.len(), None);
        };
        let tail = &text[at..];

        if tail.as_bytes()[0] == MARK {
            return (at, parse_colour(tail).map(|c| (MARK_LEN, Change::Mark(c))));
        }

        match tag(tail) {
            Some(found) => return (at, Some(found)),
            // No tag: the '[' is a character.
            None => from = at + 1,
        }
    }
}

/// The tag `text` opens with, its length and change, as text tag split
/// splits it and text markup tag reads it; `None` where it is
/// none of the colour tags.
fn tag(text: &str) -> Option<(usize, Change)> {
    let close = text[1..].starts_with(TAG_CLOSE);
    let start = if close { 2 } else { 1 };
    let body = &text[start..];

    // The name ends at a separator or ']'; neither, no tag.
    let stop = body.find([TAG_SEPARATORS[0], TAG_SEPARATORS[1], ']'])?;
    let name = body[..stop].trim_ascii();

    // The value runs to ']', else to the end, the rest swallowed.
    let (value, used) = if body[stop..].starts_with(']') {
        ("", start + stop + 1)
    } else {
        let after = &body[stop + 1..];

        match after.find(']') {
            Some(end) => (&after[..end], start + stop + 1 + end + 1),
            None => ("", text.len()),
        }
    };

    let fore = FORE_TAGS.iter().any(|t| t.eq_ignore_ascii_case(name));
    let change = match (fore, close) {
        (true, true) => Some(Change::Close),
        (true, false) => Some(Change::Open(tag_colour(value))),
        (false, _) => BACK_TAG.eq_ignore_ascii_case(name).then_some(Change::Back),
    };

    change.map(|c| (used, c))
}

/// A tag's value as a colour: one of [`NAMED_COLOURS`],
/// else `strtoul` of `"0x"` and the value: its leading hex digits, none
/// 0, past 32 bits all ones.
///
/// ```
/// use regent_core::data::text::tag_colour;
///
/// assert_eq!(tag_colour(" Light_Blue "), 0x0075_aaff);
/// assert_eq!(tag_colour("7070ff"), 0x0070_70ff);
/// assert_eq!(tag_colour("0x7070ff"), 0, "\"0x0x...\" reads the first 0");
/// ```
#[must_use]
pub fn tag_colour(value: &str) -> u32 {
    let value = value.trim_ascii();
    let named = NAMED_COLOURS
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(value));

    named.map_or_else(
        || {
            value
                .chars()
                .map_while(|c| c.to_digit(16))
                .try_fold(0u32, |n, d| n.checked_mul(16)?.checked_add(d))
                .unwrap_or(u32::MAX)
        },
        |(_, colour)| *colour,
    )
}

/// A tag colour's low three bytes, red first.
const fn rgb(colour: u32) -> [u8; 3] {
    let [_, r, g, b] = colour.to_be_bytes();

    [r, g, b]
}

/// Text with its markers and colour tags removed (as [`spans`] reads
/// them). Borrows when there are none.
#[must_use]
pub fn plain(text: &str) -> Cow<'_, str> {
    let mut runs = spans(text);

    match (runs.next(), runs.next()) {
        (Some(one), None) => Cow::Borrowed(one.text),
        (first, second) => Cow::Owned(
            first
                .into_iter()
                .chain(second)
                .chain(runs)
                .map(|s| s.text)
                .collect(),
        ),
    }
}

/// `text` without its colour tags, `0x01` markers kept: for the chat
/// lines, whose info box reads markers and no tag.
///
/// ```
/// use regent_core::data::text::untagged;
///
/// assert_eq!(untagged("[color:light_red]Potions:[/color] [1]"), "Potions: [1]");
/// ```
#[must_use]
pub fn untagged(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;

    // Each colour tag's place and length in turn; another '[' stays.
    let next = |rest: &str| {
        rest.match_indices('[')
            .find_map(|(at, _)| tag(&rest[at..]).map(|(used, _)| (at, used)))
    };

    while let Some((at, used)) = next(rest) {
        out.push_str(&rest[..at]);
        rest = &rest[at + used..];
    }

    out.push_str(rest);
    out
}

/// `\` escapes in a stored string and their characters.
const ESCAPES: [(char, &str); 5] = [
    ('r', "\r"),
    ('n', "\n"),
    ('t', "\t"),
    ('\\', "\\"),
    ('"', "\""),
];

/// Bracketed words a stored string replaces, case ignored.
const WORDS: [(&str, &str); 2] = [("[newline]", "\r\n"), ("[tab]", "\t")];

/// A `<Strings>` text or a quest's message as HD stores it
/// (string unescape, from strings put and the quest's reader):
/// `\r`, `\n`, `\t`, `\\` and `\"` made their characters, another `\x`
/// kept whole; `[newline]` CR LF and `[tab]` a tab, case ignored; colour
/// tags left for the renderer ([`spans`]).
///
/// ```
/// use regent_core::data::text::unescaped;
///
/// assert_eq!(unescaped("Done.[NewLine][newline]\\\"Off\\\" \\q"), "Done.\r\n\r\n\"Off\" \\q");
/// ```
#[must_use]
pub fn unescaped(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;

    while let Some(at) = rest.find(['\\', '[']) {
        out.push_str(&rest[..at]);

        let tail = &rest[at..];
        let lower = tail.to_ascii_lowercase();
        let escape = tail[1..]
            .chars()
            .next()
            .filter(|_| tail.starts_with('\\'))
            .and_then(|c| ESCAPES.iter().find(|(e, _)| *e == c));
        let word = WORDS.iter().find(|(w, _)| lower.starts_with(w));

        let (put, used) = match (escape, word) {
            (Some((c, put)), _) => (*put, 1 + c.len_utf8()),
            (None, Some((w, put))) => (*put, w.len()),
            (None, None) => (&tail[..1], 1),
        };

        out.push_str(put);
        rest = &tail[used..];
    }

    out.push_str(rest);
    out
}

/// Colour of a well-formed marker at the start of `text`.
fn parse_colour(text: &str) -> Option<[u8; 3]> {
    let digits = text.strip_prefix(MARK as char)?.get(..6)?;

    let pair = |at: usize| u8::from_str_radix(digits.get(at..at + 2)?, 16).ok();

    Some([pair(0)?, pair(2)?, pair(4)?])
}

/// One `sprintf` argument.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arg<'a> {
    Int(i64),
    Str(&'a str),
}

impl From<i32> for Arg<'_> {
    fn from(n: i32) -> Self {
        Arg::Int(n.into())
    }
}

impl From<u32> for Arg<'_> {
    fn from(n: u32) -> Self {
        Arg::Int(n.into())
    }
}

impl<'a> From<&'a str> for Arg<'a> {
    fn from(s: &'a str) -> Self {
        Arg::Str(s)
    }
}

impl<'a> From<&'a String> for Arg<'a> {
    fn from(s: &'a String) -> Self {
        Arg::Str(s)
    }
}

/// The conversions a game string holds: `%d` and `%ld` a number, `%s` a
/// string. **Corpus**: no other, and no width or flag, in any edition's
/// `STRT` (`the_games_text_formats_only_d_ld_and_s`).
const CONVERSIONS: [&str; 3] = ["ld", "d", "s"];

/// `format` as the engine's `sprintf` writes it, each conversion the
/// next of `args` in turn, `%%` a percent sign. **Inferred**: a
/// conversion past the last argument writes nothing (the engine would
/// read stack garbage); one of the other kind writes the argument as it
/// is.
///
/// ```
/// use regent_core::data::text::printf;
///
/// assert_eq!(printf("%d of %d hp", &[51.into(), 74.into()]), "51 of 74 hp");
/// assert_eq!(printf("Day %d%s, 100%%", &[3.into(), "s".into()]), "Day 3s, 100%");
/// ```
#[must_use]
pub fn printf(format: &str, args: &[Arg<'_>]) -> String {
    let mut out = String::new();
    let mut next = args.iter();
    let mut rest = format;

    while let Some(at) = rest.find('%') {
        out.push_str(&rest[..at]);

        let after = &rest[at + 1..];
        let conversion = CONVERSIONS.iter().find(|c| after.starts_with(*c));

        rest = match conversion {
            _ if after.starts_with('%') => {
                out.push('%');
                &after[1..]
            }
            Some(c) => {
                match next.next() {
                    Some(Arg::Int(n)) => out.push_str(&n.to_string()),
                    Some(Arg::Str(s)) => out.push_str(s),
                    None => {}
                }

                &after[c.len()..]
            }
            None => {
                out.push('%');
                after
            }
        };
    }

    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// English kept, by `Language` or `lang`; a later file's key wins;
    /// a number keys as the number, a name as its CRC; the text stored
    /// unescaped.
    #[test]
    fn strings_keep_english_and_the_last_file() {
        let install = crate::data::xml::parse(
            "<Majesty><Language id=\"en_US\">\
             <Text id=\"IDTXT_A\">first</Text><Text id=\"350\">bare</Text>\
             <Text id=\"IDTXT_B\" lang=\"de_DE\">deutsch</Text></Language>\
             <Language id=\"fr_FR\"><Text id=\"IDTXT_C\">français</Text></Language>\
             <Text id=\"IDTXT_D\">top</Text><Text id=\"IDTXT_E\">a[newline]b</Text></Majesty>",
        )
        .expect("well-formed");
        let tamed = crate::data::xml::parse(
            "<Majesty><Language id=\"en_US\"><Text id=\"IDTXT_A\">second</Text>\
             </Language></Majesty>",
        )
        .expect("well-formed");
        let mut strings = Strings::default();

        strings.add(&install);
        strings.add(&tamed);

        assert_eq!(strings.of("IDTXT_A"), Some("second"));
        assert_eq!(strings.get(350), Some("bare"));
        assert_eq!(strings.of("IDTXT_B"), None);
        assert_eq!(strings.of("IDTXT_C"), None);
        assert_eq!(strings.of("IDTXT_D"), Some("top"));
        assert_eq!(strings.of("IDTXT_E"), Some("a\r\nb"), "unescaped");
        assert_eq!(strings.len(), 4);

        let text = Text::default().with_strings(strings);

        assert_eq!(text.gmtx(350).as_deref(), Some("bare"), "past GMTX");
    }

    #[test]
    fn printf_fills_in_order_and_keeps_what_it_does_not_know() {
        let args = [Arg::Str("Bob"), Arg::Int(-2)];

        assert_eq!(printf("%s has %ld", &args), "Bob has -2");
        assert_eq!(printf("%s and %s", &args[..1]), "Bob and ");
        assert_eq!(printf("50% off %x", &[]), "50% off %x");
    }

    /// A `QITM` item string.
    const RING: &str = "Ring of Protection \n\u{1}FFDDAA(+10 dodge,parry, resist, +2 armor)";

    #[test]
    fn a_colour_marker_starts_a_new_span() {
        let got: Vec<Span<'_>> = spans(RING).collect();

        assert_eq!(
            got,
            vec![
                Span {
                    colour: None,
                    text: "Ring of Protection \n"
                },
                Span {
                    colour: Some([0xFF, 0xDD, 0xAA]),
                    text: "(+10 dodge,parry, resist, +2 armor)"
                },
            ]
        );
    }

    #[test]
    fn plain_text_drops_the_marker_and_keeps_the_words() {
        assert_eq!(
            plain(RING),
            "Ring of Protection \n(+10 dodge,parry, resist, +2 armor)"
        );
    }

    /// No markup, no allocation.
    #[test]
    fn text_without_markup_is_borrowed() {
        assert!(matches!(plain("Berserking"), Cow::Borrowed(_)));
    }

    /// A malformed marker ends the scan; the rest is dropped.
    #[test]
    fn a_marker_with_no_colour_after_it_ends_the_scan() {
        let got: Vec<&str> = spans("before\u{1}xy").map(|s| s.text).collect();

        assert_eq!(got, ["before"]);
    }

    #[test]
    fn an_empty_string_is_one_empty_span() {
        assert_eq!(spans("").count(), 1);
        assert_eq!(plain(""), "");
    }

    /// `text`'s runs, empty ones dropped.
    fn runs(text: &str) -> Vec<(Option<[u8; 3]>, &str)> {
        spans(text)
            .filter(|s| !s.text.is_empty())
            .map(|s| (s.colour, s.text))
            .collect()
    }

    const BLUE: Option<[u8; 3]> = Some([0x70, 0x70, 0xff]);
    const RED: Option<[u8; 3]> = Some([0xff, 0, 0]);

    /// The Wild's Grizzly Bear: a hex tag colours its words, then the
    /// caller's colour again.
    #[test]
    fn a_hex_tag_colours_the_text_up_to_its_end() {
        assert_eq!(
            runs("Bears.\r\n[color:7070ff]Living in the...[/color] Done"),
            [
                (None, "Bears.\r\n"),
                (BLUE, "Living in the..."),
                (None, " Done")
            ]
        );
    }

    /// Names and tags fold case; `=` separates as `:` does; the value is
    /// trimmed (Majestic Majesty's `[color: BDB76B]`).
    #[test]
    fn a_tag_folds_case_and_takes_either_separator() {
        assert_eq!(runs("[COLOR=Red]a"), [(RED, "a")]);
        assert_eq!(runs("[fcolor: red ]a"), [(RED, "a")]);
        assert_eq!(runs("[color: BDB76B]a"), [(Some([0xbd, 0xb7, 0x6b]), "a")]);
    }

    /// Tags nest: a close goes back to the colour under it, and past the
    /// last to the caller's; a close with none open changes nothing.
    #[test]
    fn tags_nest_and_an_extra_close_changes_nothing() {
        assert_eq!(
            runs("[color:red]a[color:7070ff]b[/color]c[/color]d[/color]e"),
            [
                (RED, "a"),
                (BLUE, "b"),
                (RED, "c"),
                (None, "d"),
                (None, "e")
            ]
        );
        assert_eq!(runs("\u{1}FF0000a[/color]b"), [(RED, "a"), (RED, "b")]);
    }

    /// An unclosed tag colours to the end; a value with no `]` takes the
    /// rest of the text with it, black.
    #[test]
    fn an_unclosed_tag_runs_to_the_end() {
        assert_eq!(runs("a[color:red]b"), [(None, "a"), (RED, "b")]);
        assert_eq!(runs("a[color:red b"), [(None, "a")]);
        assert_eq!(
            spans("a[color:red b").last().map(|s| s.colour),
            Some(Some([0; 3]))
        );
    }

    /// Other brackets are text: Majestic Majesty's "Embers [Level 1]", a
    /// lone `[`, a name with no end.
    #[test]
    fn a_bracket_that_is_no_colour_tag_is_text() {
        assert_eq!(runs("Embers [Level 1]"), [(None, "Embers [Level 1]")]);
        assert_eq!(runs("[b]x[/b] [ [color"), [(None, "[b]x[/b] [ [color")]);
        assert!(matches!(plain("Embers [Level 1]"), Cow::Borrowed(_)));
    }

    /// A background tag is dropped and changes nothing drawn.
    #[test]
    fn a_background_tag_is_dropped() {
        assert_eq!(
            runs("a[bcolor:red]b[/bcolor]c"),
            [(None, "a"), (None, "b"), (None, "c")]
        );
    }

    /// A named colour is the table's; a name the table lacks is hex,
    /// leading digits only.
    #[test]
    fn a_tag_value_is_a_name_or_hex() {
        assert_eq!(tag_colour("light_red"), 0x00ff_8c8c);
        assert_eq!(tag_colour("light_green"), 0x007c_ff77, "the first of two");
        assert_eq!(tag_colour("dark_green"), 0xda, "hex digits d, a");
        assert_eq!(tag_colour("123456789"), u32::MAX, "past 32 bits");
        assert_eq!(tag_colour(""), 0);
    }

    #[test]
    fn plain_drops_tags_and_markers_and_untagged_the_tags_alone() {
        let text = "[color:light_red]Potions:[/color] 1\n\u{1}FFDDAA(+10)";

        assert_eq!(plain(text), "Potions: 1\n(+10)");
        assert_eq!(untagged(text), "Potions: 1\n\u{1}FFDDAA(+10)");
    }
}
