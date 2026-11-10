//! `SMNU` — the panel and menu layouts, as a stream of keyed words.
//!
//! A panel is numbers with no lengths, names or redundancy, so one file alone
//! cannot check a reading.
//!
//! HD's thirteen `Data/UIData_<w>_<h>.dat` archives hold the same panels, same
//! lengths, for thirteen screen sizes. A word that differs was computed from
//! the screen size.
//!
//! ## The stream
//!
//! Blocks, each a type word, `[key][value]` properties and [`END`]; a type
//! of [`END`] closes the panel. The first block is the panel's own, type
//! [`MAGIC`]; the rest are its controls ([`Panel::controls`]).
//!
//! ```text
//! 1000                      the panel's block
//! 2  0 182 202 245          the panel's own rectangle
//! 10 2
//! 10 262144
//! 12 1230261833             a four-character id, as a word
//! 13 1029
//! -1                        end of block
//! 0                         a control's type: a button
//! 2  52 26 100 100          a control's rectangle
//! ..
//! -1 -1                     end of block, end of panel
//! ```
//!
//! **Binary**: `CYDialog` reads the panel's block,
//! `CYDialog` makes a control per type ([`Kind`]),
//! `CYDialogItem` reads a control's keys, and
//! Smnu skip property skips one: [`width`] is its
//! table. Read here as items, the end word and the next type are one item
//! of key [`END`].
//!
//! A parse must end exactly at the payload's end (test
//! `every_shipped_panel_consumes_exactly`, all 616 records of three editions).
//!
//! ## What says key 2 is a rectangle
//!
//! The Phantoms Haunt mod disables a button by zeroing the last two of the four
//! words `(103, 162, 93, 26)` at `AP10 + 0x0d34` in HD. This reader puts a
//! [`Rect`] there.
//!
//! Across HD's thirteen resolutions, 96 of the 102 moving words are in key-2
//! items, one in a [`RECT_TOO`]. Only two panels move; in `APdb` (32 words of 3353, eleven files)
//! all are rectangle fields.
//!
//! `APMK` has five files at five widths but two heights:
//!
//! | Moves between two files of | Slot 0 | Slot 1 | Slot 2 | Slot 3 |
//! |---|---|---|---|---|
//! | equal height, so with the width | 11 | 0 | 5 | 0 |
//! | equal width, so with the height | 0 | 40 | 0 | 8 |
//!
//! Nothing crosses: `(x, y, width, height)`.
//!
//! ## The five that are not geometry
//!
//! All in `APMK`, which swaps a picture at a taller screen: two key-12 words go
//! `INse` to `INDz`, three key-13 words move with them.
//!
//! [`ID`] is a record id: all 12,000 values in three editions are four
//! printable characters.
//!
//! ## What is not read
//!
//! Keys without a constant here: what `CYDialogItem` does with them
//! is not read.

use super::error::{Error, Result};
use super::fourcc::FourCc;

/// The word every payload opens with: the panel's own block's type.
pub const MAGIC: i32 = 1000;

/// The word ending a block's properties; as a block's type, the panel's end.
pub const END: i32 = -1;

/// The geometry key, whose four words are a [`Rect`].
pub const RECT: i32 = 2;

/// The key whose word is a four-character record id — see [`Item::id`].
///
/// How a panel reaches its art and captions.
pub const ID: i32 = 12;

/// The key whose word is the `IMAG` set of [`ID`]'s record a control draws.
///
/// **Corpus**: it moves with key 12 where `APMK` swaps a picture
/// (`docs/formats/panels.md`); `APMK`'s empty panel names a
/// `view::panel::portrait::Crown` by it. Panel portrait sets
/// the same pair at run time, as control properties `0x40` and `0x42`.
pub const SET: i32 = 13;

/// The key whose word is the control's id, the number a controller asks
/// for: 8011 the portrait, 2005 the main screen's selection panel.
///
/// **Binary**: the ids `StatsWindowController` pushes (`0x1f4b`, `0x1f45`,
/// `0x1f46`, `0x1f4a`) are this key's values on the panels it drives
/// (`AP20`, `APb5`, `APc5`).
pub const CONTROL: i32 = 6;

/// The key whose word is a `STRT` index: the control's text, from the table
/// of the panel's own record id (`AP29` `SMNU`, `AP29` `STRT`).
///
/// **Binary**: `CYDialogItem` case 7, strt string, then
/// item set text (control message `0x5c`).
pub const LABEL: i32 = 7;

/// The key whose word is a `STRT` index, sent as control message `0x5e`:
/// the tooltip, the text
/// `DialogController` hands the tooltip line (`view::panel::tip`). Some
/// read as a designer's note ("Background Image"). **Inferred**: control
/// vf `0x1e0`, which `DialogView` reads, returns it.
pub const TOOLTIP: i32 = 33;

/// The key whose word is the control's hot key, the character it types,
/// upper case: `'P'` (80) Play Game, 13 Enter, 27 Escape; keys no
/// character types from `0xe000` (`view::menu::hotkey`). Not a virtual key
/// code. **Binary**: `CYDialogItem` case 5,
/// `item_hotkey_set(code, 0)`; keys `0x100 + n` set slot `n`
/// (`0x100` to `0x10c`); key match reads slot language.
pub const HOTKEY: i32 = 5;

/// The key whose words are OR'd into the control's flags, each
/// item adding bits. **Binary**: case 3, `CYDialogItem`.
pub const FLAGS: i32 = 3;

/// [`FLAGS`] bit: the pointer passes through it. Dialog hit
/// skips it: the main menu's room, frame and version line.
pub const PASSIVE: i32 = 0x80;

/// [`FLAGS`] bit: disabled. **Binary**: `CYDialogItem` reads it;
/// `CYDialog` gives such a control no key, and method state 2.
/// **Inferred**: the name; 57 shipped controls set it.
pub(crate) const DISABLED: i32 = 4;

/// [`FLAGS`] bit: lit, state 1. **Binary**: `CYDialogItem` gives
/// state 1 for it unless disabled; one of its methods reads it.
/// `DEM2` 1006 and `INTG` 1001 draw slot 1 white or blue for it.
/// **Inferred**: the name; 15 shipped controls set it (`AP85`, `APc1`,
/// `APc2`), and no engine code but method was found to.
pub const LIT: i32 = 0x10;

/// [`FLAGS`] bit: a click on it ends the dialog with its id (`CYDialog`,
/// the mouse-up test).
pub const CLICKS: i32 = 2;

/// The key whose word is a list box's row height, item. **Binary**:
/// `CYDialogItem` case `0x1c`; `CYDialogListboxItem`
/// takes it, else its font's height plus 4.
pub const ROW_HEIGHT: i32 = 28;

/// The key whose words are OR'd into the control's style, how
/// it draws. **Binary**: case 10, `CYDialogItem`,
/// which also hands the picture bits to its widget.
pub const STYLE: i32 = 10;

/// [`STYLE`]'s bits. The low bits, `0x1f`: 2 draws the picture, 0 a filled
/// box with a one pixel edge in colour roles 1 and 2.
pub mod style {

    /// The picture repeated over the rectangle from its corner, not
    /// drawn once (widget draw's tile loop, widget draw).
    pub const TILED: i32 = 0x4_0000;

    /// A raised bevel round it (`CYDialogManager`;
    /// `view::menu::bevel`).
    pub const RAISED: i32 = 0x1_0000;

    /// A sunken bevel round it, unless [`RAISED`].
    pub const SUNKEN: i32 = 0x8_0000;

    /// A text box draws no ground under its text: neither picture nor
    /// fill.
    pub const BARE: i32 = 0x10_0000;
}

/// The key whose word names the font, a `FONT` or an `IMAG` of glyphs:
/// `fnt4`, `fn11`. **Binary**: case `0x12`, control message `0x11`.
pub const FONT: i32 = 18;

/// The key whose word names the palette, a `PALT`: `MMS1`, `IBOX`.
/// **Binary**: case `0x0b`, palette holder set.
pub const PALETTE: i32 = 11;

/// The key whose list sets the text colours, colour role 3 ([`colour_role`]).
///
/// **Binary**: `CYDialogItem` draws a control's text in
/// role 3 at the control's state.
pub(crate) const TEXT_COLOURS: i32 = 36;

/// The key whose list sets the hot key letter's colours, colour role 4.
///
/// **Binary**: `CYDialogItem` finds the hot key's character in the
/// text and draws it in role 4. **Game**
/// (`local/shots/menu_main.png`): the blue letter is the hot key's,
/// CINEMATIC's second ('I', 73); EXIT GAME's hot key is Escape, and no
/// letter is blue.
pub(crate) const HOTKEY_COLOURS: i32 = 49;

/// The keys whose value is a counted list of colours, and the role each sets.
///
/// **Binary**: `CYDialogItem` cases `0x22`–`0x25`, `0x31`–`0x34` pass
/// list entry `i` to `item_colour_set(role, i, colour)`.
/// A colour is `0x80rrggbb`; `0x40000000` and `0x20000000` defer to the
/// parent's, then the panel palette's (`view::menu::colour`).
const COLOUR_LISTS: [(i32, i32); 8] = [
    (34, 14),
    (35, 15),
    (TEXT_COLOURS, 3),
    (37, 5),
    (HOTKEY_COLOURS, 4),
    (50, 6),
    (51, 8),
    (52, 9),
];

/// The colour role a list key sets, if it is one.
#[must_use]
pub fn colour_role(key: i32) -> Option<i32> {
    COLOUR_LISTS
        .iter()
        .find_map(|(k, role)| (*k == key).then_some(*role))
}

/// A second four-word key, the text's rectangle: `x, y, width, height`
/// from the control's corner. **Binary**: `CYDialogItem` keeps it; with
/// its width and height set, it sets the
/// text in it, not in the control's rectangle.
pub const RECT_TOO: i32 = 42;

/// The key opening a nested list, closed by [`NESTED_END`].
const NESTED: i32 = 23;

/// The key closing a [`NESTED`] list.
const NESTED_END: i32 = 24;

/// Keys inside a [`NESTED`] list taking one word; the rest take none.
const NESTED_ONE_WORD: [i32; 7] = [1, 7, 12, 13, 17, 25, 26];

/// The keys taking two words in a control and three in the panel's block.
const PAIRS: [i32; 2] = [46, 47];

/// Words of value after `key`, inside a block of type `block`, its value
/// starting `rest`; `None` where `rest` runs short.
///
/// **Binary**: the skip table of smnu skip property,
/// which `CYDialogItem` and `CYDialog` agree with: [`RECT`]
/// and [`RECT_TOO`] four words, a colour list its count plus one, the
/// `PAIRS` two (three in the panel's block), a `NESTED` list to its
/// end, any other key one.
#[must_use]
pub fn width(block: i32, key: i32, rest: &[i32]) -> Option<usize> {
    match key {
        RECT | RECT_TOO => Some(4),
        _ if colour_role(key).is_some() => {
            let count = usize::try_from(*rest.first()?).ok()?;

            Some(count + 1)
        }
        _ if PAIRS.contains(&key) => Some(if block == MAGIC { 3 } else { 2 }),
        NESTED => nested(rest),
        _ => Some(1),
    }
}

/// A [`NESTED`] list's length, its closing key counted.
fn nested(rest: &[i32]) -> Option<usize> {
    let mut at = 0;

    while *rest.get(at)? != NESTED_END {
        at += if NESTED_ONE_WORD.contains(&rest[at]) {
            2
        } else {
            1
        };
    }

    Some(at + 1)
}

/// The panel block's [`RECT`] position meaning "centred on the screen".
///
/// **Binary**: `CYDialog` turns `x` and `y` both -1 into 0, 0 and
/// placement 9 (dialog). **Game**: `APc9`, 800 by 600 at (-1, -1),
/// stands at (560, 300) on a 1920 by 1200 screen
/// (`local/shots/menu_barren_waste_start.png`).
pub const CENTRED: i32 = -1;

/// A control's class, its block's type word.
///
/// **Binary**: `CYDialog`'s switch; the classes are the ones the
/// constructors make. Types 3, 4, 7, 8 and 10 make a plain `CYDialogItem`
/// and ship nowhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// `CYDialogButtonItem`; pictures are buttons too.
    Button,
    /// `CYDialogCheckItem`.
    Check,
    /// `CYDialogRadioItem`.
    Radio,
    /// `CYDialogItem`.
    Item,
    /// `CYDialogTextboxItem`: a text, scrolling or not.
    Textbox,
    /// `CYDialogListboxItem`.
    Listbox,
    /// `CYDialogScrollbarItem`.
    Scrollbar,
    /// `CYDialogComboboxItem`.
    Combobox,
    /// `CYDialogInfoBoxItem`: the typing chat box.
    InfoBox,
}

impl Kind {
    /// The class of a block's type word.
    #[must_use]
    pub const fn of(word: i32) -> Option<Kind> {
        match word {
            0 => Some(Kind::Button),
            1 => Some(Kind::Check),
            2 => Some(Kind::Radio),
            3 | 4 | 7 | 8 | 10 => Some(Kind::Item),
            5 => Some(Kind::Textbox),
            6 => Some(Kind::Listbox),
            9 => Some(Kind::Scrollbar),
            11 => Some(Kind::Combobox),
            12 => Some(Kind::InfoBox),
            _ => None,
        }
    }
}

/// A control's position and size, in pixels from the panel's top left.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl Rect {
    /// `[x0, y0, x1, y1]`, the far edges exclusive.
    #[must_use]
    pub const fn corners(&self) -> [i32; 4] {
        [self.x, self.y, self.x + self.width, self.y + self.height]
    }

    /// Whether `(x, y)` lies inside, the far edges outside.
    #[must_use]
    pub fn holds(&self, (x, y): (i32, i32)) -> bool {
        // Widened, so a far edge past `i32::MAX` cannot overflow.
        let within = |at: i32, from: i32, len: i32| {
            (i64::from(from)..i64::from(from) + i64::from(len)).contains(&i64::from(at))
        };

        within(x, self.x, self.width) && within(y, self.y, self.height)
    }
}

/// One `[key][value]` item of the stream.
#[derive(Debug, Clone, Copy)]
pub struct Item<'a> {
    /// The key's position, in words from the payload start.
    pub at: usize,
    pub key: i32,
    pub value: &'a [i32],
}

impl Item<'_> {
    /// This item's value as a rectangle, if it is one.
    #[must_use]
    pub const fn rect(&self) -> Option<Rect> {
        if let (RECT, [x, y, width, height]) = (self.key, self.value) {
            Some(Rect {
                x: *x,
                y: *y,
                width: *width,
                height: *height,
            })
        } else {
            None
        }
    }

    /// This item's value as the record id it names, if it is one.
    ///
    /// `None` for other keys, and for a word that is not four printable
    /// characters; no shipped panel holds one. [`Item::value`] gives the raw
    /// word.
    #[must_use]
    pub fn id(&self) -> Option<FourCc> {
        match (self.key, self.value) {
            (ID, [word]) => {
                let tag = FourCc::from_word(word.cast_unsigned());

                tag.as_bytes()
                    .iter()
                    .all(u8::is_ascii_graphic)
                    .then_some(tag)
            }
            _ => None,
        }
    }

    /// Where this item's value starts, in bytes from the start of the payload.
    #[must_use]
    pub const fn value_offset(&self) -> usize {
        (self.at + 1) * 4
    }
}

/// A parsed `SMNU` payload.
#[derive(Debug, Clone)]
pub struct Panel {
    items: Vec<(usize, i32, std::ops::Range<usize>)>,
    words: Vec<i32>,
}

impl Panel {
    /// Read a payload.
    ///
    /// # Errors
    ///
    /// If the length is not whole words, the first word is not [`MAGIC`], or
    /// the stream overruns the end (meaning [`width`] is wrong).
    pub fn parse(data: &[u8]) -> Result<Self> {
        let (quads, rest) = data.as_chunks::<4>();

        if !rest.is_empty() {
            return Err(Error::BadValue {
                what: "SMNU payload length, which is not whole words",
                value: data.len() as u64,
            });
        }

        let words: Vec<i32> = quads.iter().copied().map(i32::from_le_bytes).collect();

        match words.first() {
            Some(&MAGIC) => {}
            found => {
                return Err(Error::BadMagic {
                    what: "SMNU panel",
                    found: found.map(|w| w.to_le_bytes().to_vec()).unwrap_or_default(),
                });
            }
        }

        let mut items = Vec::new();
        let mut at = 1;
        let mut block = MAGIC;

        while at < words.len() {
            let key = words[at];
            let rest = &words[at + 1..];

            // The end word's value is the next block's type.
            let width = if key == END {
                Some(1)
            } else {
                width(block, key, rest)
            };

            let value = width.map(|n| at + 1..at + 1 + n);

            let Some(value) = value.filter(|v| v.end <= words.len()) else {
                return Err(Error::Truncated {
                    what: "SMNU item value",
                    need: words.len() * 4 + 4,
                    have: words.len() * 4,
                });
            };

            if key == END {
                block = words[value.start];
            }

            // A rectangle's far edges must be numbers too.
            if let (RECT | RECT_TOO, [x, y, width, height]) = (key, &words[value.clone()])
                && (x.checked_add(*width).is_none() || y.checked_add(*height).is_none())
            {
                return Err(Error::BadValue {
                    what: "SMNU rectangle past the widest coordinate",
                    value: at as u64,
                });
            }

            items.push((at, key, value.clone()));
            at = value.end;
        }

        Ok(Panel { items, words })
    }

    /// Every item, in file order.
    pub fn items(&self) -> impl Iterator<Item = Item<'_>> + '_ {
        self.items.iter().map(|(at, key, value)| Item {
            at: *at,
            key: *key,
            value: &self.words[value.clone()],
        })
    }

    /// Every rectangle in the panel, in file order.
    ///
    /// The panel's own first, then its controls.
    pub fn rects(&self) -> impl Iterator<Item = Rect> + '_ {
        self.items().filter_map(|item| item.rect())
    }

    /// The panel's own rectangle, which is always the first item.
    #[must_use]
    pub fn frame(&self) -> Option<Rect> {
        self.items().next().and_then(|item| item.rect())
    }

    /// The item whose value begins at `offset` bytes into the payload.
    ///
    /// For the Phantoms Haunt offset into `AP10`.
    #[must_use]
    pub fn at_offset(&self, offset: usize) -> Option<Item<'_>> {
        self.items().find(|item| item.value_offset() == offset)
    }

    /// How many words the payload held.
    #[must_use]
    pub const fn words(&self) -> usize {
        self.words.len()
    }

    /// The blocks: the panel's own first (type [`MAGIC`]), then each
    /// control, in file order, which is draw order.
    #[must_use]
    pub fn controls(&self) -> Vec<Control<'_>> {
        let mut out = vec![Control {
            kind: MAGIC,
            items: Vec::new(),
        }];

        for item in self.items() {
            match (item.key, item.value) {
                (END, [END]) => break,
                (END, [kind]) => out.push(Control {
                    kind: *kind,
                    items: Vec::new(),
                }),
                _ => {
                    if let Some(last) = out.last_mut() {
                        last.items.push(item);
                    }
                }
            }
        }

        out
    }

    /// The control holding `id` ([`CONTROL`]).
    #[must_use]
    pub fn control(&self, id: i32) -> Option<Control<'_>> {
        self.controls().into_iter().find(|c| c.id() == Some(id))
    }

    /// Whether the panel stands centred on the screen ([`CENTRED`]).
    #[must_use]
    pub fn centred(&self) -> bool {
        self.frame()
            .is_some_and(|r| r.x == CENTRED && r.y == CENTRED)
    }
}

/// One block of a panel: the panel's own, or a control.
#[derive(Debug, Clone)]
pub struct Control<'a> {
    /// The block's type word: [`MAGIC`] for the panel's own, else a
    /// [`Kind`].
    pub kind: i32,
    /// Its properties, in file order.
    pub items: Vec<Item<'a>>,
}

impl Control<'_> {
    /// The value of the last item of `key` holding one word.
    ///
    /// The last, as the reader applies each in turn: a later [`ID`] wins.
    #[must_use]
    pub fn word(&self, key: i32) -> Option<i32> {
        self.items
            .iter()
            .rev()
            .find_map(|i| match (i.key, i.value) {
                (k, [w]) if k == key => Some(*w),
                _ => None,
            })
    }

    /// The class, if a control's.
    #[must_use]
    pub const fn class(&self) -> Option<Kind> {
        Kind::of(self.kind)
    }

    /// Its rectangle; a control's from the panel's top left.
    #[must_use]
    pub fn rect(&self) -> Option<Rect> {
        self.items.iter().find_map(Item::rect)
    }

    /// Its text's rectangle ([`RECT_TOO`]), from its own corner.
    #[must_use]
    pub fn text_rect(&self) -> Option<Rect> {
        self.items.iter().find_map(|i| match (i.key, i.value) {
            (RECT_TOO, [x, y, width, height]) => Some(Rect {
                x: *x,
                y: *y,
                width: *width,
                height: *height,
            }),
            _ => None,
        })
    }

    /// Its id ([`CONTROL`]), the number the code asks for.
    #[must_use]
    pub fn id(&self) -> Option<i32> {
        self.word(CONTROL)
    }

    /// Every one-word value of `key`, in order: keys 3 and 10 repeat.
    #[must_use]
    pub fn words(&self, key: i32) -> Vec<i32> {
        self.items
            .iter()
            .filter_map(|i| match (i.key, i.value) {
                (k, [w]) if k == key => Some(*w),
                _ => None,
            })
            .collect()
    }

    /// Its flags: every [`FLAGS`] word OR'd.
    #[must_use]
    pub fn flags(&self) -> i32 {
        self.words(FLAGS).into_iter().fold(0, |all, w| all | w)
    }

    /// Its style: every [`STYLE`] word OR'd.
    #[must_use]
    pub fn style(&self) -> i32 {
        self.words(STYLE).into_iter().fold(0, |all, w| all | w)
    }

    /// The record its picture comes from ([`ID`]).
    #[must_use]
    pub fn record(&self) -> Option<FourCc> {
        self.items.iter().rev().find_map(Item::id)
    }

    /// The `IMAG` set of [`Control::record`] ([`SET`]).
    #[must_use]
    pub fn set(&self) -> Option<i32> {
        self.word(SET)
    }

    /// The picture: [`ID`]'s record and [`SET`]'s set.
    #[must_use]
    pub fn art(&self) -> Option<(FourCc, i32)> {
        let record = self.items.iter().rev().find_map(Item::id)?;

        Some((record, self.word(SET)?))
    }

    /// The font ([`FONT`]).
    #[must_use]
    pub fn font(&self) -> Option<FourCc> {
        self.word(FONT)
            .map(|w| FourCc::from_word(w.cast_unsigned()))
    }

    /// The palette ([`PALETTE`]).
    #[must_use]
    pub fn palette(&self) -> Option<FourCc> {
        self.word(PALETTE)
            .map(|w| FourCc::from_word(w.cast_unsigned()))
    }

    /// The text's `STRT` index ([`LABEL`]).
    #[must_use]
    pub fn label(&self) -> Option<usize> {
        self.word(LABEL).and_then(|w| usize::try_from(w).ok())
    }

    /// The tooltip's `STRT` index ([`TOOLTIP`]).
    #[must_use]
    pub fn tooltip(&self) -> Option<usize> {
        self.word(TOOLTIP).and_then(|w| usize::try_from(w).ok())
    }

    /// The hot key, slot 0 ([`HOTKEY`]).
    #[must_use]
    pub fn hotkey(&self) -> Option<i32> {
        self.word(HOTKEY)
    }

    /// A colour list's colours, its count dropped.
    #[must_use]
    pub fn colours(&self, key: i32) -> Option<&[i32]> {
        self.items
            .iter()
            .rev()
            .find(|i| i.key == key && colour_role(key).is_some())
            .and_then(|i| i.value.get(1..))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The opening of HD's `AP10`, down to the first control.
    fn sample() -> Vec<u8> {
        let words: [i32; 11] = [MAGIC, RECT, 0, 182, 202, 245, 10, 2, ID, 1_230_261_833, 13];
        let mut out: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();

        out.extend_from_slice(&1029i32.to_le_bytes());
        out
    }

    #[test]
    fn a_rectangle_past_the_widest_coordinate_is_refused() {
        let words = [MAGIC, RECT, i32::MAX, 0, 10, 10];
        let bytes: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();

        assert!(matches!(Panel::parse(&bytes), Err(Error::BadValue { .. })));
    }

    #[test]
    fn a_panel_opens_with_its_own_rectangle() {
        let panel = Panel::parse(&sample()).unwrap();

        assert_eq!(
            panel.frame(),
            Some(Rect {
                x: 0,
                y: 182,
                width: 202,
                height: 245
            })
        );
    }

    #[test]
    fn a_narrow_key_takes_one_word_and_the_stream_consumes_exactly() {
        let panel = Panel::parse(&sample()).unwrap();
        let keys: Vec<i32> = panel.items().map(|item| item.key).collect();

        assert_eq!(keys, [RECT, 10, ID, 13]);
        assert_eq!(panel.words(), 12);
    }

    /// `1_230_261_833` is `INTI`, the interface texture archive
    /// (`docs/formats/panels.md`).
    #[test]
    fn an_id_key_reads_back_as_four_characters() {
        let panel = Panel::parse(&sample()).unwrap();
        let ids: Vec<FourCc> = panel.items().filter_map(|item| item.id()).collect();

        assert_eq!(ids, [FourCc::new(b"INTI")]);
    }

    /// A small-number `ID` gives `None`, not mojibake.
    #[test]
    fn an_id_that_is_not_four_characters_is_refused_rather_than_rendered() {
        let words = [MAGIC, ID, 33];
        let bytes: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
        let panel = Panel::parse(&bytes).unwrap();
        let item = panel.items().next().unwrap();

        assert_eq!(item.id(), None);
        assert_eq!(item.value, [33]);
    }

    /// The offset is the one the Phantoms Haunt mod quotes, in miniature.
    #[test]
    fn an_item_is_found_by_the_byte_offset_of_its_value() {
        let panel = Panel::parse(&sample()).unwrap();

        assert_eq!(panel.at_offset(8).and_then(|i| i.rect()), panel.frame());
        assert_eq!(panel.at_offset(9).map(|i| i.key), None);
    }

    #[test]
    fn something_that_does_not_open_with_the_magic_is_refused() {
        let bytes = 7i32.to_le_bytes();

        assert!(matches!(Panel::parse(&bytes), Err(Error::BadMagic { .. })));
    }

    #[test]
    fn a_wide_key_running_off_the_end_is_refused() {
        let words = [MAGIC, RECT, 1, 2];
        let bytes: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();

        assert!(matches!(Panel::parse(&bytes), Err(Error::Truncated { .. })));
    }

    #[test]
    fn a_payload_that_is_not_whole_words_is_refused() {
        assert!(matches!(
            Panel::parse(&[0, 0, 0]),
            Err(Error::BadValue { .. })
        ));
    }
}
