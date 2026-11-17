//! The Book of Lore's page (`ui::lore` keeps its state): High Scores'
//! `AP64` (`scores`) made the book, as large as the window holds, over
//! the faces. openregent's own: the original has no such page.
//!
//! ```text
//!  ┌[pog]──────────── Book of Lore ─────────────────────┐  8005, INTD 1001
//!  │ ┌──────────────── MONSTERS ─────────────────────┐  │  12: the tab shown
//!  │ │ [face] Avatar of Krolm ▲│[face] [figure] Dragon▲│  │
//!  │ │ [face] Barbarian       ││               Slain: 7││  │  fixed header
//!  │ │ [ ? ]  Unknown         ││ - Breathes fire ...  ││  │  the text scrolls
//!  │ │ ...                    ▼│ KNOWN AT ENCOUNTER   ▼│  │
//!  │ └───────────────────────────────────────────────┘  │  list grown by
//!  │  [MONSTERS][LAIRS][HEROES][HENCHMEN][BUILDINGS]    │  the game row's height
//!  │ [ 31 OF 50 DISCOVERED ]          [ MAIN MENU ]     │  9001's place, 9002
//!  └────────────────────────────────────────────────────┘
//! ```
//!
//! The frame is `INDh` 1000 cut by rows: the game row (460..503) out, the
//! list's rows 250..293 in again. Between list and details a copy of
//! the frame's own scroll track; a details pane that does not scroll
//! covers its track with the frame's left border, mirrored, so it shows
//! no bar.

use bevy_egui::egui::{self, Color32, Painter, Pos2, Rect, Sense, Stroke, pos2, vec2};
use regent_core::data::fourcc::FourCc;
use regent_core::view::menu::scores::id;
use regent_core::view::menu::{SMALL_FONT, ShownControl, State, Tick, hotkey};
use regent_draw::art as service;

use super::art::{Art, Pack, Pic};
use super::cut::{Across, Flip, Strip};
use super::fit::{self, Fit};
use super::font::{self, Font};
use super::paint::{self, Able, Canvas, PALE};
use crate::files::lore::{
    self, Catalogue, Category, Entry, SPAWNS_AFTER, details_needed, spawns_revealed, subtitle,
    unlock_text,
};
use crate::files::profile::{LoreMark, Profile};
use crate::ui::lore::{Book, known};

/// The book's pog: `INTD` 1001, `QU02`'s, a book. Drawn in `AP64`'s pog
/// control in place of its trophy.
const POG: Pic = Pic::of(FourCc(*b"INTD"), 1001);

/// What the title says.
const TITLE: &str = "Book of Lore";

/// `AP64`'s category radios, 6000..6004: the five tabs, by their places
/// left to right.
const TABS: std::ops::RangeInclusive<i32> = 6000..=6004;

/// The frame `INDh` 1000 (600 rows) cut by rows: the game row's
/// 460..503 out, the list's 250..293 in again (`cut::Strip`).
const FRAME_STRIPS: [Strip; 4] = [
    plain(0.0, 293.0, 0.0),
    plain(250.0, 293.0, 293.0),
    plain(293.0, 460.0, 336.0),
    plain(503.0, 600.0, 503.0),
];

/// How far the frame's rows below its list move down: the list grown.
const GROWN: f32 = 43.0;

/// Rows `from..to` of a picture as they are, from row `at`.
const fn plain(from: f32, to: f32, at: f32) -> Strip {
    Strip {
        from,
        to,
        at,
        flip: Flip::Plain,
    }
}

/// The list's pane and the details', layout pixels.
const LIST: Rect = Rect::from_min_max(pos2(45.0, 133.0), pos2(294.0, 431.0));
const DETAILS: Rect = Rect::from_min_max(pos2(342.0, 226.0), pos2(748.0, 431.0));

/// The scroll bars: `AP64`'s 13 moved beside each pane, 294 high.
const LIST_BAR: Pos2 = pos2(300.0, 135.0);
const DETAILS_BAR: Pos2 = pos2(755.0, 135.0);
const BAR_HEIGHT: f32 = 294.0;

/// The frame's scroll track, columns 749..781, its navy margin the
/// first 6: copied between the panes from column 294, rows 130..434,
/// the margin mirrored after it.
const TRACK_X: f32 = 749.0;
const TRACK_WIDTH: f32 = 32.0;
const MARGIN_WIDTH: f32 = 6.0;
const DIVIDER_X: f32 = 294.0;
const DIVIDER_ROWS: (f32, f32) = (130.0, 434.0);

/// The details' track covered where they do not scroll: the frame's
/// left border (columns 19..41 of rows 200..300) mirrored, the navy of
/// its columns 34..39 before it, in 100 row tiles down from row 128.
const EDGE_AT: Pos2 = pos2(749.0, 128.0);
const EDGE_HEIGHT: f32 = 306.0;
const BORDER: Rect = Rect::from_min_max(pos2(19.0, 200.0), pos2(41.0, 300.0));
const NAVY: Rect = Rect::from_min_max(pos2(34.0, 200.0), pos2(39.0, 300.0));
/// Where the mirrored border starts in the edge: its silver line then
/// meets the bottom swirl's stem.
const BORDER_AT: f32 = 10.0;

/// A list row: its height, the first one's gap from the pane's top, its
/// portrait's side and inset, the gap after it.
const ROW: f32 = 28.0;
const ROW_TOP: f32 = 2.0;
const ROW_FACE: f32 = 25.0;
const ROW_INSET: f32 = 3.0;
const ROW_GAP: f32 = 6.0;

/// A titled row's title baseline above its middle, its subtitle's
/// below: the two lines a row holds.
const TITLE_RISE: f32 = 1.0;
const SUBTITLE_DROP: f32 = 7.5;

/// The details' fixed header: the portrait (80 square), the figure's
/// box (90 wide, the portrait's height) and the title's column.
const PORTRAIT: Rect = Rect::from_min_max(pos2(340.0, 138.0), pos2(420.0, 218.0));
const FIGURE: Rect = Rect::from_min_max(pos2(428.0, 138.0), pos2(518.0, 218.0));
const HEAD_X: f32 = 528.0;

/// The header's baselines: title, subtitle, and the counter under the
/// title, or under the subtitle where one shows.
const HEAD_TITLE: f32 = 174.5;
const HEAD_SUBTITLE: f32 = 187.4;
const HEAD_COUNTER: f32 = 194.8;
const HEAD_COUNTER_LOW: f32 = 208.8;

/// An unseen entry's header: "Unknown" and what reveals it, from the
/// details' left.
const UNKNOWN_TITLE: f32 = 172.5;
const UNKNOWN_LINE: f32 = 190.5;

/// The details' text: its first line's top below the pane's, its wrap
/// width, the stats' columns, a spawn row's portrait and pitch.
const TEXT_TOP: f32 = 2.0;
const TEXT_WIDTH: f32 = 400.0;
const STAT_INSET: f32 = 12.0;
const STAT_COLUMN: f32 = 170.0;
const SPAWN_FACE: f32 = 20.0;
const SPAWN_PITCH: f32 = 23.0;
const SPAWN_GAP: f32 = 3.0;

/// Room under the details' last line, layout pixels.
const TEXT_FOOT: f32 = 4.0;

/// The text sizes, ems in layout pixels (openregent's own): the header's
/// title, subtitle and counter; a row's title and subtitle; the body,
/// a heading; the unknown box's "?".
const HEAD_TITLE_EM: f32 = 19.0;
const HEAD_SUBTITLE_EM: f32 = 9.0;
const COUNTER_EM: f32 = 11.0;
const ROW_EM: f32 = 10.0;
const ROW_SUBTITLE_EM: f32 = 7.5;
const BODY_EM: f32 = 8.5;
const HEADING_EM: f32 = 9.5;
const MARK_EM: f32 = 15.5;

/// The body's line pitch, a heading's, of their ems; a blank help line,
/// a paragraph's end and a table's end, of a body line.
const BODY_PITCH: f32 = 1.55;
const HEADING_PITCH: f32 = 1.75;
const BLANK: f32 = 0.45;
const AFTER_HELP: f32 = 0.6;
const AFTER_TABLE: f32 = 0.4;

/// Inknut Antiqua's capital height and ascent, of its em: the text
/// sizes are its ems, its line tops its ascent above the baseline.
const CAP_EM: f32 = 0.80;
const ASCENT_EM: f32 = 1.71;

/// Colours: section headings (`MX17`'s labels), weak text, the picked
/// row (`save_page::LIT`), the portrait's line, the unknown box. Weak
/// text, the line and the box are **guesses**.
const HEADING: Color32 = Color32::from_rgb(0xf8, 0xb1, 0x52);
/// A list row's title: the list box's white (`AP64` 8001).
const ROW_INK: Color32 = Color32::WHITE;
const WEAK: Color32 = Color32::from_rgb(0x9a, 0x96, 0x8c);
const LINE: Color32 = Color32::from_rgb(0xc8, 0x9a, 0x3c);
const BOX_FILL: Color32 = Color32::from_rgb(24, 22, 30);
const BOX_EDGE: Color32 = Color32::from_rgb(70, 64, 56);

/// An unseen entry's words.
const UNKNOWN: &str = "Unknown";
const REVEAL: &str = "Meet, defeat, or own this entry to reveal it.";
const UNPICKED: &str = "Select an entry.";
const NO_PROFILE: &str = "Choose a profile to view the book.";

/// `BV02 Enemy_Barbarian`, its note.
const ENEMY_BARBARIAN: FourCc = FourCc(*b"BV02");
const BARBARIAN_NOTE: &str = "Minimum birth stats. Health and combat stats scale with hero level.";

/// `BBT1 SpireOfDeath` and `BBQ1 FortressofIxmil`, and their notes
/// under the spawns.
const SPIRE_OF_DEATH: FourCc = FourCc(*b"BBT1");
const FORTRESS_OF_IXMIL: FourCc = FourCc(*b"BBQ1");
const SPIRE_NOTE: &str = "Spawn set changes during the quest.";
const FORTRESS_NOTE: &str = "Quest events spawn monsters here.";

/// The pictures the page draws that no panel names (`art::Store`).
pub(super) fn pictures() -> Vec<(Pack, Pic)> {
    vec![(Pack::Base, POG)]
}

/// MAIN MENU pressed: the book closes.
pub(crate) struct Shut;

/// The book's page over the main menu, `book` showing `catalogue` as
/// `profile` knows it; whether MAIN MENU (or Escape) closed it.
pub(crate) fn page(
    ctx: &egui::Context,
    (art, shared): (&mut Art, &mut service::Art),
    book: &mut Book,
    (catalogue, profile): (&Catalogue, Option<&Profile>),
) -> Option<Shut> {
    let Art {
        book: panels, ink, ..
    } = art;
    let layout = panels.scores.as_ref()?;
    let fit = Fit::of(ctx.viewport_rect(), paint::size(layout));
    let mut canvas = Canvas::at(super::mods::above(ctx, "lore"), (ink, shared), fit);
    let base = canvas.ink.font(SMALL_FONT)?;
    let window = canvas.ui.max_rect();

    canvas.backdrop();
    canvas.block(window);
    frame(&mut canvas, layout);

    let entries: Vec<(&Entry, LoreMark)> = listed(catalogue, book.tab)
        .into_iter()
        .map(|e| (e, known(profile, e.code)))
        .collect();
    let seen = entries.iter().filter(|(_, m)| m.seen).count();

    if let Some(c) = layout.control(id::TITLE) {
        canvas.label(c, TITLE, paint::pick(c, State::Normal));
    }

    if let Some(c) = layout.control(id::DEFINITION) {
        canvas.label(c, &book.tab.name().to_uppercase(), PALE);
    }

    // The discovered count in ERASE SCORES' place, its button not drawn.
    if let Some(c) = layout.control(id::ERASE) {
        let said = format!("{seen} OF {} DISCOVERED", entries.len());

        canvas.label(c, &said, PALE);
    }

    tabs(&mut canvas, layout, book);
    list(&mut canvas, layout, book, (&entries, catalogue, base));

    let picked = book
        .selected
        .and_then(|code| entries.iter().find(|(e, _)| e.code == code))
        .copied();
    let profiled = if profile.is_some() {
        Profiled::Yes
    } else {
        Profiled::No
    };
    let chosen = details(
        &mut canvas,
        layout,
        book,
        (picked, profiled),
        (catalogue, base),
    );

    if let Some(entry) = chosen.and_then(|code| catalogue.0.get(&code)) {
        book.pick(entry);
    }

    let closed = layout.control(id::MAIN_MENU).is_some_and(|c| {
        let clicked = canvas.button(Pack::Base, c, Able::Enabled);
        let offered = [(c, hotkey::Able::Enabled)];
        let keyed = paint::keys(ctx)
            .into_iter()
            .any(|k| hotkey::pick(offered, k).is_some());

        clicked || keyed
    });

    closed.then_some(Shut)
}

/// `category`'s entries by title, then code.
fn listed(catalogue: &Catalogue, category: Category) -> Vec<&Entry> {
    let mut entries: Vec<&Entry> = catalogue
        .0
        .values()
        .filter(|e| e.category == category)
        .collect();

    entries.sort_by(|a, b| a.title.cmp(&b.title).then(a.code.cmp(&b.code)));
    entries
}

/// The frame cut by rows, the pog in AP64's place, and the copy of the
/// scroll track between the panes.
fn frame(canvas: &mut Canvas<'_>, layout: &regent_core::view::menu::Layout) {
    if let Some((record, set)) = layout.panel.art {
        let pic = Pic::of(record, set);

        canvas.strips(Pack::Base, pic, Pos2::ZERO, &FRAME_STRIPS);

        // The track's rows as the cut frame holds them: each strip's
        // part between the divider's rows.
        for s in FRAME_STRIPS {
            let top = s.at.max(DIVIDER_ROWS.0);
            let bottom = (s.at + s.rows()).min(DIVIDER_ROWS.1);
            let from = s.from + (top - s.at);

            if bottom > top {
                let rows = bottom - top;
                let track = Rect::from_min_size(pos2(TRACK_X, from), vec2(TRACK_WIDTH, rows));
                let margin = Rect::from_min_size(pos2(TRACK_X, from), vec2(MARGIN_WIDTH, rows));
                let after = DIVIDER_X + TRACK_WIDTH;

                canvas.piece(
                    Pack::Base,
                    pic,
                    (track, pos2(DIVIDER_X, top)),
                    Across::Plain,
                );
                canvas.piece(
                    Pack::Base,
                    pic,
                    (margin, pos2(after, top)),
                    Across::Mirrored,
                );
            }
        }
    }

    // The book in the pog's place: at its corner, as High Scores draws
    // its own.
    let pog = layout.control(id::POG).and_then(|c| c.rect).map(fit::rect);

    if let Some(r) = pog {
        canvas.picture(Pack::Base, POG, r.min);
    }
}

/// The details' track covered by the frame's left border, mirrored: no
/// bar where they do not scroll.
fn edge(canvas: &mut Canvas<'_>, layout: &regent_core::view::menu::Layout) {
    if let Some((record, set)) = layout.panel.art {
        let pic = Pic::of(record, set);
        let tile = BORDER.height();
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a few tiles"
        )]
        let tiles = (EDGE_HEIGHT / tile).ceil() as usize;

        for n in 0..tiles {
            #[allow(clippy::cast_precision_loss, reason = "a few tiles")]
            let y = EDGE_AT.y + n as f32 * tile;
            let rows = tile.min(EDGE_AT.y + EDGE_HEIGHT - y);
            let cut = |r: Rect| Rect::from_min_size(r.min, vec2(r.width(), rows));
            let navies = [Across::Plain, Across::Mirrored];

            for (i, across) in navies.into_iter().enumerate() {
                #[allow(clippy::cast_precision_loss, reason = "two pieces")]
                let x = EDGE_AT.x + i as f32 * NAVY.width();

                canvas.piece(Pack::Base, pic, (cut(NAVY), pos2(x, y)), across);
            }

            let at = pos2(EDGE_AT.x + BORDER_AT, y);

            canvas.piece(Pack::Base, pic, (cut(BORDER), at), Across::Mirrored);
        }
    }
}

/// The details' scroll bar's id, to tell its arrows from the list's:
/// openregent's own, `AP64` has one bar.
const DETAILS_BAR_ID: i32 = 9013;

/// `AP64`'s scroll bar moved to `at`, [`BAR_HEIGHT`] high, told apart
/// by `key`.
fn bar(layout: &regent_core::view::menu::Layout, (at, key): (Pos2, i32)) -> Option<ShownControl> {
    let c = layout.control(id::BAR)?;
    #[allow(clippy::cast_possible_truncation, reason = "a panel's pixels")]
    let rect = c.rect.map(|r| regent_core::data::smnu::Rect {
        x: at.x as i32,
        y: at.y as i32,
        height: BAR_HEIGHT as i32,
        ..r
    });

    Some(ShownControl {
        rect,
        id: Some(key),
        ..c.clone()
    })
}

/// The category radios as the five tabs, moved down with the list, the
/// one shown ticked; a press shows its category from the top.
fn tabs(canvas: &mut Canvas<'_>, layout: &regent_core::view::menu::Layout, book: &mut Book) {
    let mut radios: Vec<&ShownControl> = TABS.filter_map(|i| layout.control(i)).collect();

    radios.sort_by_key(|c| c.rect.map_or(0, |r| r.x));

    #[allow(clippy::cast_precision_loss, reason = "a panel's pixels")]
    for (c, category) in radios.into_iter().zip(Category::ALL) {
        let tab = ShownControl {
            text: Some(category.name().to_uppercase()),
            hotkey: None,
            ..paint::lowered(c, GROWN)
        };
        let tick = if book.tab == category {
            Tick::Checked
        } else {
            Tick::Clear
        };

        if canvas.ticked(Pack::Base, &tab, (Able::Enabled, tick)) && book.tab != category {
            book.tab = category;
            book.selected = None;
            book.list_scroll = 0.0;
            book.detail_scroll = 0.0;
        }
    }
}

/// `base` sized `em`: its capitals as tall as an `em` of Inknut's.
fn sized(base: Font, em: f32) -> Font {
    let cap = crate::ui::typeface::metrics().cap * base.size();

    base.times(CAP_EM * em / cap)
}

/// `parts` on one line from layout point `pen`, its baseline, cut to
/// `width` with "…" where wider.
fn line(
    (canvas, painter): (&Canvas<'_>, &Painter),
    font: Font,
    (pen, width): (Pos2, f32),
    parts: &[(Color32, String)],
) {
    let ctx = canvas.ui.ctx();
    let scale = canvas.fit.scale;
    let measure = |s: &str| font.width(ctx, scale, s);
    let whole: String = parts.iter().map(|(_, s)| s.as_str()).collect();

    if measure(&whole) <= width {
        font.draw(painter, canvas.fit, pen, parts);
    } else {
        font.draw(
            painter,
            canvas.fit,
            pen,
            &font::ellipsised(parts, width, &measure),
        );
    }
}

/// The list: a row an entry, its portrait and title, an unseen one's
/// "?" and "Unknown"; the picked one lit. A press picks; the wheel and
/// its bar scroll it.
fn list(
    canvas: &mut Canvas<'_>,
    layout: &regent_core::view::menu::Layout,
    book: &mut Book,
    (entries, catalogue, base): (&[(&Entry, LoreMark)], &Catalogue, Font),
) {
    #[allow(clippy::cast_precision_loss, reason = "a list's rows")]
    let tall = entries.len() as f32 * ROW + ROW_TOP;
    let most = crate::ui::scroll::most(tall, LIST.height());
    let at = entries
        .iter()
        .position(|(e, _)| Some(e.code) == book.selected);

    if let (true, Some(at)) = (book.follow, at) {
        book.list_scroll = centred(at, most);
    }

    book.follow = false;

    let screen = canvas.fit.rect(LIST);

    canvas.wheeled(screen, "lore list", (&mut book.list_scroll, most));

    let painter = canvas.painter().with_clip_rect(screen);
    let (title, small, mark) = (
        sized(base, ROW_EM),
        sized(base, ROW_SUBTITLE_EM),
        sized(base, MARK_EM),
    );

    for (n, (entry, seen)) in entries.iter().enumerate() {
        #[allow(clippy::cast_precision_loss, reason = "a list's rows")]
        let top = LIST.top() + ROW_TOP + n as f32 * ROW - book.list_scroll;
        let row = Rect::from_min_size(pos2(LIST.left(), top), vec2(LIST.width(), ROW));

        if row.intersects(LIST) {
            let response = canvas.ui.interact(
                canvas.fit.rect(row.intersect(LIST)),
                canvas.ui.id().with(("lore row", entry.code)),
                Sense::click(),
            );

            paint::arm(&response, None);

            if book.selected == Some(entry.code) {
                painter.rect_filled(canvas.fit.rect(row), 0.0, super::save_page::LIT);
            }

            let face = Rect::from_min_size(
                pos2(LIST.left() + ROW_INSET, top + (ROW - ROW_FACE) / 2.0),
                vec2(ROW_FACE, ROW_FACE),
            );
            let text_x = face.right() + ROW_GAP;
            let room = LIST.right() - text_x;
            let mid = row.center().y;

            // A seen entry's portrait, title and subtitle under it; an
            // unseen one's "?" and "Unknown".
            let centre = pos2(text_x, mid + title.cap(canvas.fit.scale) / 2.0);
            let (named, sub) = if seen.seen {
                portrait((canvas, &painter), (book, catalogue), (entry.code, face));
                ((ROW_INK, entry.title.clone()), subtitle(entry.code))
            } else {
                unknown_box((canvas, &painter), mark, face);
                ((WEAK, UNKNOWN.to_owned()), None)
            };
            let pen = sub.map_or(centre, |_| pos2(text_x, mid - TITLE_RISE));

            line((canvas, &painter), title, (pen, room), &[named]);

            if let Some(sub) = sub {
                let below = pos2(text_x, mid + SUBTITLE_DROP);

                line(
                    (canvas, &painter),
                    small,
                    (below, room),
                    &[(HEADING, sub.to_owned())],
                );
            }

            if response.clicked() && book.selected != Some(entry.code) {
                book.selected = Some(entry.code);
                book.detail_scroll = 0.0;
            }
        }
    }

    let bar = bar(layout, (LIST_BAR, id::BAR));

    canvas.barred(Pack::Base, bar.as_ref(), (&mut book.list_scroll, most, ROW));
}

/// The list's scroll, of `most`, with row `at` in its middle.
fn centred(at: usize, most: f32) -> f32 {
    #[allow(clippy::cast_precision_loss, reason = "a list's rows")]
    let top = ROW_TOP + at as f32 * ROW;

    (top - (LIST.height() - ROW) / 2.0).clamp(0.0, most)
}

/// `code`'s portrait over layout rectangle `rect`, cut by `painter`;
/// whether it has one.
fn portrait(
    (canvas, painter): (&mut Canvas<'_>, &Painter),
    (book, catalogue): (&mut Book, &Catalogue),
    (code, rect): (FourCc, Rect),
) -> bool {
    let ctx = canvas.ui.ctx().clone();
    let screen = canvas.fit.rect(rect);
    let shown = book.portrait(&ctx, catalogue, canvas.shared(), screen, code);

    if let Some(shown) = &shown {
        painter.image(shown.id, shown.rect, shown.uv, Color32::WHITE);
    }

    shown.is_some()
}

/// An unseen entry's portrait: a dark box, its edge, a "?" in `mark`.
fn unknown_box((canvas, painter): (&Canvas<'_>, &Painter), mark: Font, rect: Rect) {
    let screen = canvas.fit.rect(rect);
    let scale = canvas.fit.scale;
    let wide = mark.width(canvas.ui.ctx(), scale, "?");
    let pen = pos2(
        rect.center().x - wide / 2.0,
        rect.center().y + mark.cap(scale) / 2.0,
    );

    painter.rect_filled(screen, 0.0, BOX_FILL);
    painter.rect_stroke(
        screen,
        0.0,
        Stroke::new(scale, BOX_EDGE),
        egui::StrokeKind::Inside,
    );
    mark.draw(painter, canvas.fit, pen, &[(WEAK, "?".to_owned())]);
}

/// One mark of the details' text, from the text's top: a line from its
/// baseline's start, or a spawn's row.
enum Mark {
    Line {
        pen: Pos2,
        font: Font,
        parts: Vec<(Color32, String)>,
    },
    Spawn {
        top: f32,
        code: FourCc,
        title: String,
    },
}

/// The details' text laid out down from its top: its marks, and how far
/// down the next goes.
struct Sheet<'a> {
    ctx: &'a egui::Context,
    scale: f32,
    body: Font,
    heading: Font,
    marks: Vec<Mark>,
    /// Each line's top: where a scroll to the end may stop.
    tops: Vec<f32>,
    y: f32,
}

impl Sheet<'_> {
    /// A body line's pitch.
    fn pitch() -> f32 {
        BODY_EM * BODY_PITCH
    }

    /// `parts` on a line from `x`, in `font` of `em`.
    fn put(&mut self, x: f32, (font, em): (Font, f32), parts: Vec<(Color32, String)>) {
        let pen = pos2(x, self.y + ASCENT_EM * em);

        self.tops.push(self.y);
        self.marks.push(Mark::Line { pen, font, parts });
    }

    /// `text`, its markup's colours over `colour`, each paragraph evened
    /// out over its lines (`font::balanced`); a blank line a part of one.
    fn paragraphs(&mut self, text: &str, colour: Color32) {
        let (ctx, scale, body) = (self.ctx, self.scale, self.body);

        for source in font::painted(text, colour) {
            let plain: String = source.iter().map(|(ch, _)| ch).collect();

            if plain.trim().is_empty() {
                self.y += Self::pitch() * BLANK;
            } else {
                let mut count = |w: f32| body.wrap(ctx, scale, &plain, w).len();
                let width = font::balanced(TEXT_WIDTH, &mut count);
                let mut cursor = 0;

                for text in body.wrap(ctx, scale, &plain, width) {
                    let parts = font::recoloured(&text, &source, &mut cursor);

                    self.put(DETAILS.left(), (body, BODY_EM), parts);
                    self.y += Self::pitch();
                }
            }
        }
    }

    /// A section's heading.
    fn heading(&mut self, text: &str) {
        self.put(
            DETAILS.left(),
            (self.heading, HEADING_EM),
            vec![(HEADING, text.to_owned())],
        );
        self.y += HEADING_EM * HEADING_PITCH;
    }

    /// One weak line, inset as the tables are.
    fn note(&mut self, text: &str) {
        let x = DETAILS.left() + STAT_INSET;

        self.put(x, (self.body, BODY_EM), vec![(WEAK, text.to_owned())]);
        self.y += Self::pitch();
    }

    /// A table: each label weak, its value beside it.
    fn table(&mut self, rows: &[(String, String)]) {
        for (label, value) in rows {
            let (left, right) = (DETAILS.left() + STAT_INSET, DETAILS.left() + STAT_COLUMN);

            self.put(left, (self.body, BODY_EM), vec![(WEAK, label.clone())]);
            self.put(right, (self.body, BODY_EM), vec![(PALE, value.clone())]);
            self.y += Self::pitch();
        }

        self.y += Self::pitch() * AFTER_TABLE;
    }
}

/// `entry`'s stats that `reveal` shows, labelled and set as the book
/// words them.
fn stats(entry: &Entry, reveal: Reveal) -> Vec<(String, String)> {
    entry
        .stats
        .iter()
        .filter(|(label, _)| entry.basic(label) == (reveal == Reveal::Basic))
        .map(|(label, value)| {
            (
                entry.stat_label(label).to_owned(),
                entry.stat_text(label, *value),
            )
        })
        .collect()
}

/// Which of an entry's stats: those known at encounter, or learned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Reveal {
    Basic,
    Learned,
}

/// What a seen entry's details say under its header, from their top.
fn sheet(sheet: &mut Sheet<'_>, (entry, mark): (&Entry, LoreMark), catalogue: &Catalogue) {
    let name = |code: &FourCc| {
        catalogue
            .0
            .get(code)
            .map_or_else(|| code.to_string(), |e| e.title.clone())
    };

    if entry.code == ENEMY_BARBARIAN {
        sheet.paragraphs(BARBARIAN_NOTE, WEAK);
        sheet.y += Sheet::pitch() * BLANK;
    }

    if let Some(help) = &entry.help {
        sheet.paragraphs(help, Color32::WHITE);
        sheet.y += Sheet::pitch() * AFTER_HELP;
    }

    sheet.heading("KNOWN AT ENCOUNTER");
    sheet.table(&stats(entry, Reveal::Basic));
    sheet.heading("DETAILS");

    let needed = details_needed(entry.code);

    if mark.owned || mark.kills >= needed {
        sheet.table(&stats(entry, Reveal::Learned));

        if !entry.links.is_empty() {
            let links: Vec<(String, String)> = entry
                .links
                .iter()
                .map(|(label, code)| ((*label).to_owned(), name(code)))
                .collect();

            sheet.heading("CONNECTIONS");
            sheet.table(&links);
        }
    } else {
        sheet.note(&unlock_text(entry.code, mark.kills, needed));
    }

    if entry.category == Category::Lairs {
        spawns(sheet, (entry, mark), catalogue);
    }
}

/// A lair's spawns: a row a creature, its portrait and name, once
/// revealed; else what reveals them. The Spire's and the Fortress's
/// notes under them.
fn spawns(sheet: &mut Sheet<'_>, (entry, mark): (&Entry, LoreMark), catalogue: &Catalogue) {
    let spire = entry.code == SPIRE_OF_DEATH;

    sheet.heading(if spire {
        "SCRIPTED SPAWNS"
    } else {
        "DEFAULT SPAWNS"
    });

    if spawns_revealed(mark) {
        let codes = catalogue.spawns(entry.code);

        if codes.is_empty() {
            sheet.note("No regular spawns");
        } else {
            sheet.y += SPAWN_GAP;
        }

        for &code in codes {
            let title = catalogue
                .0
                .get(&code)
                .map_or_else(|| code.to_string(), |e| e.title.clone());

            sheet.tops.push(sheet.y);
            sheet.marks.push(Mark::Spawn {
                top: sheet.y,
                code,
                title,
            });
            sheet.y += SPAWN_PITCH;
        }

        if spire {
            sheet.note(SPIRE_NOTE);
        } else if entry.code == FORTRESS_OF_IXMIL {
            sheet.note(FORTRESS_NOTE);
        }
    } else {
        sheet.note(&unlock_text(entry.code, mark.kills, SPAWNS_AFTER));
    }
}

/// The end of a text scrolled `most` far, moved on to the first line
/// that starts there or after: at its end the pane shows no line cut at
/// its top. `most` where no line starts after it.
fn stop(tops: &[f32], most: f32) -> f32 {
    let after = tops.iter().copied().filter(|&t| t >= most);

    if most > 0.0 {
        after
            .fold(None, |least: Option<f32>, t| {
                Some(least.map_or(t, |l| l.min(t)))
            })
            .unwrap_or(most)
    } else {
        most
    }
}

/// Whether a profile reads the book.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Profiled {
    Yes,
    No,
}

/// The details: a seen entry's fixed header (portrait, figure, title,
/// counter) and its text under it, scrolled by the wheel and its bar;
/// an unseen one's "Unknown". Where the text does not scroll, no bar.
/// The spawn pressed.
fn details(
    canvas: &mut Canvas<'_>,
    layout: &regent_core::view::menu::Layout,
    book: &mut Book,
    (picked, profiled): (Option<(&Entry, LoreMark)>, Profiled),
    (catalogue, base): (&Catalogue, Font),
) -> Option<FourCc> {
    let painter = canvas.painter();
    let body = sized(base, BODY_EM);
    let (said, colour) = match (picked, profiled) {
        (Some(_), _) => (REVEAL, Color32::WHITE),
        (None, Profiled::Yes) => (UNPICKED, WEAK),
        (None, Profiled::No) => (NO_PROFILE, WEAK),
    };

    // A seen entry: its header, then its text, how far that scrolls.
    let (chosen, most) = match picked {
        Some((entry, mark)) if mark.seen => {
            header(canvas, book, (entry, mark), (catalogue, base));

            let ctx = canvas.ui.ctx().clone();
            let mut laid = Sheet {
                ctx: &ctx,
                scale: canvas.fit.scale,
                body,
                heading: sized(base, HEADING_EM),
                marks: Vec::new(),
                tops: Vec::new(),
                y: 0.0,
            };

            sheet(&mut laid, (entry, mark), catalogue);

            let most = crate::ui::scroll::most(laid.y + TEXT_TOP + TEXT_FOOT, DETAILS.height());
            let most = stop(&laid.tops, most);

            (
                text(canvas, book, (&laid.marks, most), (catalogue, body)),
                most,
            )
        }
        _ => {
            if picked.is_some() {
                let head = sized(base, HEAD_TITLE_EM);
                let at = pos2(DETAILS.left(), UNKNOWN_TITLE);

                line(
                    (canvas, &painter),
                    head,
                    (at, DETAILS.width()),
                    &[(PALE, UNKNOWN.to_owned())],
                );
            }

            let at = pos2(DETAILS.left(), UNKNOWN_LINE);

            line(
                (canvas, &painter),
                body,
                (at, DETAILS.width()),
                &[(colour, said.to_owned())],
            );
            (None, 0.0)
        }
    };

    if most > 0.0 {
        let bar = bar(layout, (DETAILS_BAR, DETAILS_BAR_ID));

        canvas.barred(
            Pack::Base,
            bar.as_ref(),
            (&mut book.detail_scroll, most, Sheet::pitch()),
        );
    } else {
        edge(canvas, layout);
    }

    chosen
}

/// A seen entry's fixed header: its portrait in a gold line, its figure
/// moving, its title, subtitle and counter (`lore::counter`).
fn header(
    canvas: &mut Canvas<'_>,
    book: &mut Book,
    (entry, mark): (&Entry, LoreMark),
    (catalogue, base): (&Catalogue, Font),
) {
    let painter = canvas.painter();
    let width = DETAILS.right() - HEAD_X;
    let framed = portrait(
        (canvas, &painter),
        (book, catalogue),
        (entry.code, PORTRAIT),
    );

    if framed {
        let around = canvas.fit.rect(PORTRAIT.expand(1.0));
        let stroke = Stroke::new(canvas.fit.scale, LINE);

        painter.rect_stroke(around, 0.0, stroke, egui::StrokeKind::Inside);
    }

    let ctx = canvas.ui.ctx().clone();
    let room = canvas.fit.rect(FIGURE);

    let room = (room, canvas.fit.scale);

    if let Some(shown) = book.figure(&ctx, catalogue, canvas.shared(), room, entry.code) {
        painter.image(shown.id, shown.rect, shown.uv, Color32::WHITE);
    }

    let title = (sized(base, HEAD_TITLE_EM), pos2(HEAD_X, HEAD_TITLE));

    line(
        (canvas, &painter),
        title.0,
        (title.1, width),
        &[(PALE, entry.title.clone())],
    );

    let counter = match subtitle(entry.code) {
        Some(sub) => {
            let at = pos2(HEAD_X, HEAD_SUBTITLE);

            line(
                (canvas, &painter),
                sized(base, HEAD_SUBTITLE_EM),
                (at, width),
                &[(WEAK, sub.to_owned())],
            );
            HEAD_COUNTER_LOW
        }
        None => HEAD_COUNTER,
    };
    let said = lore::counter(entry.kind, mark);
    let at = pos2(HEAD_X, counter);

    line(
        (canvas, &painter),
        sized(base, COUNTER_EM),
        (at, width),
        &[(HEADING, said)],
    );
}

/// The details' `marks` in their pane, scrolled by the wheel within
/// `most`: the lines, each spawn's portrait and name a press; the
/// spawn pressed.
fn text(
    canvas: &mut Canvas<'_>,
    book: &mut Book,
    (marks, most): (&[Mark], f32),
    (catalogue, body): (&Catalogue, Font),
) -> Option<FourCc> {
    let screen = canvas.fit.rect(DETAILS);

    canvas.wheeled(screen, "lore details", (&mut book.detail_scroll, most));

    let painter = canvas.painter().with_clip_rect(screen);
    let top = DETAILS.top() + TEXT_TOP - book.detail_scroll;
    let mut chosen = None;

    for mark in marks {
        match mark {
            Mark::Line { pen, font, parts } => {
                font.draw(&painter, canvas.fit, *pen + vec2(0.0, top), parts);
            }
            Mark::Spawn {
                top: at,
                code,
                title,
            } => {
                let x = DETAILS.left() + STAT_INSET;
                let face = Rect::from_min_size(pos2(x, top + at), vec2(SPAWN_FACE, SPAWN_FACE));
                let name = pos2(
                    face.right() + ROW_GAP,
                    face.center().y + body.cap(canvas.fit.scale) / 2.0,
                );
                let wide = body.width(canvas.ui.ctx(), canvas.fit.scale, title);
                let row = Rect::from_min_max(face.min, pos2(name.x + wide, face.max.y));

                if row.intersects(DETAILS) {
                    let response = canvas.ui.interact(
                        canvas.fit.rect(row.intersect(DETAILS)),
                        canvas.ui.id().with(("lore spawn", *code)),
                        Sense::click(),
                    );

                    paint::arm(&response, None);
                    portrait((canvas, &painter), (book, catalogue), (*code, face));
                    body.draw(&painter, canvas.fit, name, &[(PALE, title.clone())]);

                    if response.clicked() {
                        chosen = Some(*code);
                    }
                }
            }
        }
    }

    chosen
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The cut frame is the page's 600 rows, each drawn once, and the
    /// rows below the list move down by [`GROWN`].
    #[test]
    #[allow(clippy::float_cmp, reason = "whole rows")]
    fn the_cut_frame_fills_the_page_and_grows_the_list() {
        let end = FRAME_STRIPS
            .iter()
            .try_fold(0.0, |at, s| (s.at == at).then_some(at + s.rows()));

        assert_eq!(end, Some(600.0));
        assert_eq!(FRAME_STRIPS[2].at - FRAME_STRIPS[2].from, GROWN);
    }

    /// At its end the text stops on a line's top; a text that fits
    /// does not scroll.
    #[test]
    #[allow(clippy::float_cmp, reason = "set values")]
    fn a_scrolled_text_ends_on_a_lines_top() {
        let tops = [0.0, 13.0, 26.0, 39.0];

        assert_eq!(stop(&tops, 20.0), 26.0);
        assert_eq!(stop(&tops, 26.0), 26.0);
        assert_eq!(stop(&tops, 50.0), 50.0);
        assert_eq!(stop(&tops, 0.0), 0.0);
    }

    /// The picked row centred in the list, held within its scroll.
    #[test]
    #[allow(clippy::float_cmp, reason = "set values")]
    fn the_picked_row_is_centred_in_the_list() {
        assert_eq!(centred(0, 500.0), 0.0);
        assert_eq!(centred(30, 100.0), 100.0);

        let row = centred(10, 500.0) + (LIST.height() - ROW) / 2.0;

        assert!((row - (ROW_TOP + 10.0 * ROW)).abs() < f32::EPSILON);
    }
}
