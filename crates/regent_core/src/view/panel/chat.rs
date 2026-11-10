//! The chat area: game messages ([`crate::sim::world::chat::Said`]) and chat
//! lines, typed into a box of four rows. **Binary** unless marked.
//!
//! ```text
//!  MessageDisplayController       panel AP75's item 1, a
//!    CYDialogInfoBoxItem: 10 px in from the view's sides, 60 px tall,
//!    its top 70 px above the view's bottom; font fnt4, palette IBOX
//!    no fade (bit 2 off); idle scroll on (bit 1),
//!           #MessageBoxScrollSpeed
//!  CYDialogInfoBoxItem lays out  rows = height / (ascent + descent): 60 / 15 = 4;
//!                    columns = width / 4; the cursor on the last row
//!  the infobox queue       a line's characters into a 4096-entry queue
//!  CYDialogInfoBoxItem, each update  real time: (now - last) / 10 ms characters typed
//!                    (typing, below); every 3000 ms with the queue empty
//!                    a line feed, at most `rows` in a row
//!  typing                 code 1: colour; 0x0d: line feed; a printable
//!                    character placed, or a space at a row's start
//!                    dropped; a word that will not fit starts a row
//!                    in the next; a full row ends
//!  a line feed       the cursor down a row; off the last, all rows up
//! ```
//!
//! So a message stays 6 to 9 s once nothing follows it, each line moving
//! up a row every 3 s; three past lines show above the one being typed.

use std::collections::VecDeque;

use super::portrait;
use crate::content::unit::Unit;
use crate::data::catalogue::Catalogue;
use crate::data::desc;
use crate::data::fourcc::FourCc;
use crate::data::payload::{Palette, Rgb};
use crate::data::text::{GMTX, Text, printf, untagged};
use crate::sim::world::{
    self,
    chat::{self, RECRUITED, RESEARCH_COMPLETE, RESEARCHED_FORMAT, Said, Who},
};

/// The table every engine line's text is in.
pub const TABLE: &[u8; 4] = GMTX;

/// The box's height, px.
pub const HEIGHT: i32 = 60;

/// How far in from the view's sides the box stands, px (`MessageDisplayController`,
/// `MessageDisplayController`).
pub const INSET: i32 = 10;

/// How far above the view's bottom edge the box's top is, px.
/// **Inferred**: the bottom edge; the view rect's field
/// order is not read.
pub const RAISE: i32 = 70;

/// The font, `AP75` item 1's key 18: an `IMAG` of glyphs.
pub const FONT: FourCc = FourCc(*b"fnt4");

/// The palette, its key 11: colours are its entries.
pub const PALETTE: FourCc = FourCc(*b"IBOX");

/// Rows: [`HEIGHT`] over `fnt4`'s line, ascent 10 plus descent 5
/// (font image metrics reads them from slot 0's block 1).
pub const ROWS: usize = 4;

/// Pixels a column: method divides the width by 4.
pub const COLUMN_PX: i32 = 4;

/// Real ms a character takes to type:, 10.
pub const TYPE_MS: u32 = 10;

/// The GPL constant holding the idle scroll's period, real ms.
pub const SCROLL_CONSTANT: &str = "#MessageBoxScrollSpeed";

/// Its value in the shipped `globals.gpl`, kept for a front end with no
/// program at hand.
pub const SCROLL_MS: u32 = 3000;

/// A game message's colour: `0x8000ff00`, the nearest [`PALETTE`] entry
/// to it.
pub const GAME_RGB: u32 = 0x0000_ff00;

/// A chat line's colour, the local player's own: chat colour own,
/// `0x80ffae2c`.
pub const CHAT_RGB: u32 = 0x00ff_ae2c;

/// The palette entries a colour may take: `palette_mask(0, 0xf6, 0)`.
const OPEN: usize = 247;

/// Weights of palette nearest's distance: `3 dr² + 4 dg² + 2 db²`.
const WEIGHTS: [i32; 3] = [3, 4, 2];

/// The entry of `palette` nearest `rgb`, as palette nearest picks
/// it: entries 0 to 246, weighted, the highest index on a tie.
#[must_use]
pub fn colour(palette: &Palette, rgb: u32) -> Rgb {
    let [_, r, g, b] = rgb.to_be_bytes();
    let far = |c: &Rgb| {
        [(c.r, r), (c.g, g), (c.b, b)]
            .iter()
            .zip(WEIGHTS)
            .map(|((a, b), w)| w * (i32::from(*a) - i32::from(*b)).pow(2))
            .sum::<i32>()
    };

    palette.colours[..OPEN]
        .iter()
        .enumerate()
        .min_by_key(|(i, c)| (far(c), std::cmp::Reverse(*i)))
        .map_or(Rgb { r, g, b }, |(_, c)| *c)
}

/// A unit's title, panel name with flags bit 0: a building's chain's
/// first, else its own; empty where a descriptor will not parse, as
/// [`portrait::own_name`].
fn title(catalogue: &Catalogue<'_>, code: FourCc) -> String {
    let unit = Unit::descriptor(catalogue, code).and_then(Result::ok);
    let root = |u: &desc::Unit<'_>| match u.building() {
        Some(_) => portrait::root(catalogue, u).ok(),
        None => Some(u.head.title().into_owned()),
    };

    unit.as_ref().and_then(root).unwrap_or_default()
}

/// `LocalChatMessage`'s name: the unit's own name, else
/// its title (flags 1); where that is not the title alone (flags 5), the
/// title after it in brackets: "Olaf (Ranger)". **Inferred**: no text
/// override (text override has, unread) applies.
#[must_use]
pub fn script_name(catalogue: &Catalogue<'_>, text: &Text<'_>, who: &Who) -> String {
    let title = title(catalogue, who.code);
    let own = portrait::own_name(catalogue, text, who);
    let named = if own.is_empty() { title.clone() } else { own };

    if title.is_empty() || named == title {
        named
    } else {
        format!("{named} ({title})")
    }
}

/// A line's text, its arguments read as the engine writes them, colour
/// tags dropped. A departure: HD's box types a tag as it stands
/// (infobox type reads `0x01`, `0x02`, CR, LF and `0x11` only); here no
/// tag text shows.
#[must_use]
pub fn text(catalogue: &Catalogue<'_>, text: &Text<'_>, said: &Said) -> Option<String> {
    let gmtx = |n: usize| text.at(TABLE, n);

    let line = match &said.line {
        chat::Line::Text(n) => gmtx(*n).map(std::borrow::Cow::into_owned),
        chat::Line::Recruited(code) => {
            let title = Unit::find(catalogue, *code)
                .and_then(Result::ok)
                .map(|u| u.desc.head.title().into_owned())
                .unwrap_or_default();

            gmtx(RECRUITED).map(|f| printf(&f, &[(&title).into()]))
        }
        chat::Line::Researched {
            building,
            text: row,
        } => {
            let name = portrait::own_name(catalogue, text, building);
            let done = gmtx(RESEARCH_COMPLETE)?;
            let row = gmtx(*row)?;

            Some(printf(
                RESEARCHED_FORMAT,
                &[(&name).into(), done.as_ref().into(), row.as_ref().into()],
            ))
        }
        chat::Line::Script { text: n, unit } => {
            let name = unit
                .as_ref()
                .map(|w| script_name(catalogue, text, w))
                .unwrap_or_default();

            text.activity(*n).map(|f| printf(&f, &[(&name).into()]))
        }
        chat::Line::Extorted { amount, kept } => gmtx(world::guild::extortion::EXTORTED)
            .map(|f| printf(&f, &[(*amount).into(), (*kept).into()])),
        chat::Line::Counted { text: row, n } => gmtx(*row).map(|f| printf(&f, &[(*n).into()])),
    };

    line.map(|l| untagged(&l))
}

/// A character in the box, and its colour.
type Cell = (char, Rgb);

/// The line feed code, `0x0d`.
const LINE_FEED: char = '\r';

/// The box (`CYDialogInfoBoxItem`): what is shown, row by row, and what
/// is still to type.
#[derive(Clone, Debug)]
pub struct InfoBox {
    /// [`ROWS`] rows, top first.
    rows: Vec<Vec<Cell>>,
    /// Each row's post ([`InfoBox::post`]'s number): a line feed ends
    /// every post, so a row holds one post's characters at most. Not
    /// the engine's: a front end's link from a row to its line.
    owners: Vec<Option<u64>>,
    /// The cursor's row and its pixel offset in it.
    row: usize,
    x: i32,
    /// Characters not yet typed, each with its post.
    queue: VecDeque<(Cell, u64)>,
    /// Posts so far.
    posted: u64,
    /// The box's width, px.
    width: i32,
    /// The widest glyph's advance.
    widest: i32,
    /// Ms of the last typing step and idle scroll step, on the caller's
    /// clock (the engine's: real, `timeGetTime`).
    typed: u32,
    scrolled: u32,
    /// Idle line feeds since a character was placed.
    idle: usize,
    scroll_ms: u32,
}

impl InfoBox {
    /// An empty box `width` px wide, its widest glyph `widest`, its idle
    /// scroll every `scroll_ms`, starting at ms `now`.
    #[must_use]
    pub fn new(width: i32, widest: i32, scroll_ms: u32, now: u32) -> InfoBox {
        InfoBox {
            rows: vec![Vec::new(); ROWS],
            owners: vec![None; ROWS],
            row: ROWS - 1,
            x: 0,
            queue: VecDeque::new(),
            posted: 0,
            width,
            widest,
            typed: now,
            scrolled: now,
            idle: 0,
            scroll_ms,
        }
    }

    /// The box `width` px wide, its widest glyph `widest`: a front end's
    /// font changed. Typed rows stay as they are.
    pub const fn resize(&mut self, width: i32, widest: i32) {
        self.width = width;
        self.widest = widest;
    }

    /// The rows, top first.
    #[must_use]
    pub fn rows(&self) -> &[Vec<Cell>] {
        &self.rows
    }

    /// Each row's post, top first; `None` for a row empty since the box
    /// was made.
    #[must_use]
    pub fn owners(&self) -> &[Option<u64>] {
        &self.owners
    }

    /// `line` queued in `colour`, a line feed after it (chat post
    /// with its flag set); its number, counted from 0. An empty queue
    /// restarts typing's clock.
    pub fn post(&mut self, line: &str, colour: Rgb, now: u32) -> u64 {
        let post = self.posted;

        if self.queue.is_empty() {
            self.typed = now;
        }

        self.queue
            .extend(line.chars().chain([LINE_FEED]).map(|c| ((c, colour), post)));
        self.posted += 1;

        post
    }

    /// method at ms `now`, a glyph's advance by `advance`.
    pub fn step(&mut self, now: u32, advance: &dyn Fn(char) -> i32) {
        let steps = now.saturating_sub(self.typed) / TYPE_MS;

        if !self.queue.is_empty() && steps > 0 {
            for _ in 0..steps {
                self.type_one(advance);
            }

            self.typed = now;
        }

        if now >= self.scrolled + self.scroll_ms {
            if self.idle < ROWS && self.queue.is_empty() {
                self.feed();
                self.idle += 1;
            }

            self.scrolled = now;
        }
    }

    /// Infobox type: line feeds taken as they come, then one character.
    fn type_one(&mut self, advance: &dyn Fn(char) -> i32) {
        while self
            .queue
            .front()
            .is_some_and(|((c, _), _)| *c == LINE_FEED)
        {
            self.queue.pop_front();
            self.feed();
        }

        if let Some(((c, colour), post)) = self.queue.front().copied() {
            if self.wraps(advance) {
                self.feed();
            }

            let at_start = self.rows[self.row].is_empty();

            if !(at_start && c == ' ') {
                self.rows[self.row].push((c, colour));
                self.owners[self.row] = Some(post);
                self.x += advance(c);
                self.idle = 0;

                // A full row ends.
                let columns = usize::try_from(self.width / COLUMN_PX).unwrap_or(0);

                if self.rows[self.row].len() >= columns || self.x + self.widest > self.width {
                    self.feed();
                }
            }

            self.queue.pop_front();
        }
    }

    /// Infobox wraps: whether the word from the queue's front on will
    /// not fit the row.
    fn wraps(&self, advance: &dyn Fn(char) -> i32) -> bool {
        let columns = usize::try_from(self.width / COLUMN_PX).unwrap_or(0);
        let word = self
            .queue
            .iter()
            .map(|((c, _), _)| *c)
            .take_while(|c| *c != ' ' && *c != LINE_FEED);
        let (wide, count) = word.fold((self.x, self.rows[self.row].len()), |(x, n), c| {
            (x + advance(c), n + 1)
        });

        wide > self.width || count > columns
    }

    /// A line feed: the cursor down a row; off the last, every row up.
    fn feed(&mut self) {
        self.x = 0;

        if self.row + 1 < ROWS {
            self.row += 1;
        } else {
            self.rows.remove(0);
            self.rows.push(Vec::new());
            self.owners.remove(0);
            self.owners.push(None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GREEN: Rgb = Rgb { r: 0, g: 216, b: 0 };

    fn shown(board: &InfoBox) -> Vec<String> {
        board
            .rows()
            .iter()
            .map(|r| r.iter().map(|(c, _)| *c).collect())
            .collect()
    }

    /// A line types a character each 10 ms, then its line feed moves it up
    /// off the cursor's row.
    #[test]
    fn a_line_types_a_character_each_ten_ms() {
        let mut board = InfoBox::new(400, 8, SCROLL_MS, 0);

        board.post("Died: Olaf", GREEN, 0);
        board.step(50, &|_| 6);

        assert_eq!(shown(&board)[ROWS - 1], "Died:");

        board.step(110, &|_| 6);

        assert_eq!(shown(&board), ["", "", "Died: Olaf", ""]);
    }

    /// With nothing to type, every scroll period feeds a line: a line on
    /// the third row is gone after three.
    #[test]
    fn an_idle_box_scrolls_a_row_each_period() {
        let mut board = InfoBox::new(400, 8, SCROLL_MS, 0);

        board.post("Built", GREEN, 0);
        board.step(100, &|_| 6);

        let after = |n: u32| {
            let mut b = board.clone();

            (1..=n).for_each(|k| b.step(k * SCROLL_MS, &|_| 6));
            shown(&b)
        };

        assert_eq!(after(1), ["", "Built", "", ""]);
        assert_eq!(after(2), ["Built", "", "", ""]);
        assert_eq!(after(3), ["", "", "", ""]);
    }

    /// A word that will not fit starts the next row.
    #[test]
    fn a_word_that_will_not_fit_starts_a_row() {
        let mut board = InfoBox::new(40, 4, SCROLL_MS, 0);

        board.post("aaaa bbbb", GREEN, 0);
        board.step(1000, &|_| 6);

        assert_eq!(shown(&board), ["", "aaaa ", "bbbb", ""]);
    }

    /// Each row knows its post, a wrapped line's both rows, as rows
    /// move up.
    #[test]
    fn each_row_knows_its_post() {
        let mut board = InfoBox::new(40, 4, SCROLL_MS, 0);
        let first = board.post("aaaa bbbb", GREEN, 0);
        let second = board.post("cc", GREEN, 0);

        board.step(1000, &|_| 6);

        assert_eq!(shown(&board), ["aaaa ", "bbbb", "cc", ""]);
        assert_eq!(
            board.owners(),
            [Some(first), Some(first), Some(second), None]
        );
    }
}
