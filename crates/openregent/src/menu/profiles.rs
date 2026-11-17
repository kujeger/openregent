//! The profile page, over the main menu from its Profile button: pick a
//! profile, name a new one, or delete one. The original has no such
//! page; it is drawn in its art: HD's multiplayer session frame `INx3`
//! 1000, its black field cut out and its list plate grown with the list.
//! A new name is asked in the original's name prompt (`AP35`,
//! `regent_core::view::menu::NAME_PROMPT`, `paint::prompt`), which it
//! asks at Play Game, keying no progress by it. Games play as the last
//! used profile, asked nothing (`Profiles::current`).
//!
//! ```text
//!  ┌──────────── Majesty ────────────┐   INx3 1000, rows 124..166 cut
//!  │ [      SELECT YOUR PROFILE     ] │
//!  │  ┌───────────────────────────┐   │   list plate: a row of 22 more
//!  │  │ [ ] Your Majesty          │^│ │   per row past 4, up to 7; then
//!  │  │ [X] Kåre            [DEL] │#│ │   a scroll bar (INDs 1000..1002)
//!  │  │  +  New profile…          │v│ │   pinned below the list
//!  │  └───────────────────────────┘   │
//!  │ ( CONTINUE )      ( CANCEL )     │   DEM2 1005, AP35's
//!  └──────────────────────────────────┘
//!        │ DEL                │ an error
//!        v                    v
//!  AP11 cut short: Delete Kåre and its saves?  YES NO  /  why, OK
//! ```
//!
//! New profile opens the prompt; with no profile it opens on it, and its
//! Cancel goes back to the list, or with none, to the main menu. The
//! first start, no profile existing, opens this page once the main menu
//! is reached ([`Picker::start`]); cancelled, the first game makes the
//! default.

use bevy::log::{info, warn};
use bevy_egui::egui::{self, Color32, Pos2, Rect, pos2, vec2};
use regent_core::data::fourcc::FourCc;
use regent_core::data::smnu;
use regent_core::view::menu::message::{self, Ask};
use regent_core::view::menu::{self as panels, Layout, SMALL_FONT, ShownControl};

use super::Art;
use super::art::{Pack, Pic};
use super::cut::{Flip, Strip};
use super::fit::Fit;
use super::font::{self, Font};
use super::page::{Button, Press};
use super::paint::{self, Able, Canvas, Caret, PALE, SCREEN, tapped};
use super::save_page::LIT;
use crate::files::profile::{DEFAULT_NAME, Listed, Player, Profile, Profiles};

/// The page's frame: HD's multiplayer session panel's (`addinterface_se`,
/// 474 × 326), read with the expansion's archives. Its twin in the base
/// game, `INMe` 1000, is lower.
const FRAME: Pic = Pic::of(FourCc(*b"INx3"), 1000);
const FRAME_PACK: Pack = Pack::Expansion;

/// The frame's rows the page drops: its black field and the plate round
/// it. The stone above and below meet with the sides' straight rules.
const FIELD: (f32, f32) = (124.0, 166.0);

/// The frame's rows repeated, a row of the list each: inside the list
/// plate, straight rules and stone.
const BAND: (f32, f32) = (200.0, 222.0);

/// A list row's height, layout pixels: [`BAND`]'s.
const ROW: f32 = BAND.1 - BAND.0;

/// The rows the frame's own plate holds.
const BASE_ROWS: usize = 4;

/// The most rows shown: six profiles and New profile; more scroll.
const MOST_ROWS: usize = 7;

/// The list plate's top, its first row's, layout pixels, the field cut.
const PLATE_TOP: f32 = 132.0;
const FIRST_ROW: f32 = 135.0;

/// The plate's height at [`BASE_ROWS`].
const PLATE: f32 = 93.0;

/// The heading's box: `AP55`'s text box (24, 93), 425 × 25.
const HEADING_BOX: Rect = Rect::from_min_max(pos2(24.0, 93.0), pos2(449.0, 118.0));

/// The heading: a placeholder, the original has none.
const HEADING: &str = "SELECT YOUR PROFILE";

/// The rows' left and right: the lit band's ends, without and with the
/// scroll bar.
const ROW_LEFT: f32 = 62.0;
const ROW_RIGHT: f32 = 407.0;

/// The scroll bar's left; its arrows' width, `INDs` 1000's.
const BAR_LEFT: f32 = 387.0;
const BAR_WIDTH: f32 = 25.0;

/// Between the bar and the lit band.
const BAR_GAP: f32 = 4.0;

/// The bar's ends in from the list plate's top and bottom; an arrow's
/// height (`INDs` 1000's); how far the track runs under an arrow, and in
/// from the bar's left. All but the arrow's height guesses, set by eye.
const BAR_INSET: f32 = 3.0;
const ARROW: f32 = 27.0;
const TRACK_UNDER: f32 = 7.0;
const TRACK_IN: f32 = 2.0;

/// The bar's track: `AP27`'s, baked in its frame `INDo` 1000 at
/// (761, 200), 21 × 30, tiled between the arrows. A guess: no bar
/// picture of its own holds one.
const TRACK_PIC: Pic = Pic::of(FourCc(*b"INDo"), 1000);
const TRACK: Rect = Rect::from_min_max(pos2(761.0, 200.0), pos2(782.0, 230.0));

/// The bar's arrows' and thumb's set, `INDs` 1000 (`scroll::THUMB`).
const BAR_ART: (FourCc, i32) = (FourCc(*b"INDs"), 1000);

/// A row's tick box, `AP42`'s radio, `INDk` 1003: slot 1 the picked.
const RADIO: Pic = Pic::of(FourCc(*b"INDk"), 1003);
const RADIO_LEFT: f32 = 68.0;

/// A row's name, and New profile's plus, from the left.
const NAME_LEFT: f32 = 100.0;
const PLUS_MIDDLE: f32 = 78.0;

/// The picked row's delete button: `INDk` 1005, 30 × 20, maroon. A
/// guess: no picture in the data shows a bin.
const DELETE_ART: (FourCc, i32) = (FourCc(*b"INDk"), 1005);
const DELETE_WIDTH: f32 = 30.0;

/// Its label, and its font's share of `fnt4`: a guess, the plate is
/// 20 high.
const DELETE_LABEL: &str = "DEL";
const DELETE_TEXT: f32 = 0.72;

/// Between the delete button and the band's right end.
const DELETE_GAP: f32 = 2.0;

/// The buttons' row: `DEM2` 1005 over the frame's baked pills, at x 43
/// and 243, the field cut.
const BUTTONS_TOP: f32 = 239.0;
const CONTINUE_LEFT: f32 = 43.0;
const CANCEL_LEFT: f32 = 243.0;

/// The page's own control ids: in no panel.
const CONTINUE_ID: i32 = 9200;
const CANCEL_ID: i32 = 9201;
const DELETE_ID: i32 = 9202;
const BAR_ID: i32 = 9203;

/// Labels and titles: placeholders, the original has none.
const CONTINUE: &str = "CONTINUE";
const NEW: &str = "New profile…";
const NEW_MARK: &str = "+";
const DELETE_TITLE: &str = "Delete Profile";
const ERROR_TITLE: &str = "Profile";

/// The pictures the page draws that no panel it reads names, each in
/// every slot a control may show (`art::SLOTS`): the menus' art reads
/// only the pictures named (`art::Store`).
pub(super) fn pictures() -> Vec<(Pack, Pic)> {
    let bar = (0..=crate::ui::scroll::THUMB).map(|n| Pic::of(BAR_ART.0, BAR_ART.1 + n));
    let controls = [RADIO, Pic::of(DELETE_ART.0, DELETE_ART.1)]
        .into_iter()
        .chain(bar)
        .flat_map(|pic| (0..super::art::SLOTS).map(move |s| (Pack::Base, pic.slot(s))));

    [(FRAME_PACK, FRAME), (Pack::Base, TRACK_PIC)]
        .into_iter()
        .chain(controls)
        .collect()
}

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
    /// The list's first shown profile, rows down.
    scroll: f32,
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
            scroll: 0.0,
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

        self.listed = self.store.as_ref().map_or_default(Profiles::list);
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
        self.scroll = shown_first(self.chosen, self.listed.len());
    }

    /// The page as `--menu profiles:SHOWN` holds it for a screenshot:
    /// `delete` Delete's question on the picked profile, `error` a note
    /// of why a delete failed (a sample).
    pub fn held(&mut self, shown: &str) -> Result<(), String> {
        let name = self
            .chosen
            .and_then(|i| self.listed.get(i))
            .map(|l| l.name.clone());

        match (shown, self.chosen, name) {
            ("delete", Some(i), _) => {
                self.mode = Mode::Delete(i);
                Ok(())
            }
            ("error", _, Some(name)) => {
                self.said = Some(format!("Could not delete {name}: permission denied."));
                Ok(())
            }
            _ => Err("want delete or error, with a profile".to_owned()),
        }
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

/// The page: the list, or the name prompt; Continue once a profile is
/// the player's, Cancel.
pub fn page(
    ctx: &egui::Context,
    picker: &mut Picker,
    player: &mut Player,
    art: (&mut Art, &mut regent_draw::art::Art),
) -> Option<Press> {
    match picker.mode {
        Mode::Create => create(ctx, picker, player, art),
        Mode::Choose | Mode::Delete(_) => listed(ctx, picker, player, art),
    }
}

/// What the list page's controls asked this pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Asked {
    Pick(usize),
    Delete(usize),
    New,
    Continue,
    Cancel,
}

/// The list in its frame, CONTINUE and CANCEL, and over them Delete's
/// question or why a press did nothing.
fn listed(
    ctx: &egui::Context,
    picker: &mut Picker,
    player: &mut Player,
    (art, shared): (&mut Art, &mut regent_draw::art::Art),
) -> Option<Press> {
    let Art { book, ink, .. } = art;
    let (prompt, message) = (book.prompt.as_ref()?, book.message.as_ref()?);
    let mut canvas = Canvas::new(ctx, (ink, shared), SCREEN);
    let window = canvas.ui.max_rect();
    let width = canvas.size(FRAME_PACK, FRAME)?.x;
    let rows = frame_rows(picker.listed.len());

    canvas.fit = Fit::centred(window, canvas.fit.scale, vec2(width, frame_height(rows)));
    canvas.block(window);
    canvas.strips(FRAME_PACK, FRAME, Pos2::ZERO, &frame_strips(rows));

    let font = canvas.ink.font(SMALL_FONT)?;

    canvas.centred_in(font, HEADING_BOX, &font::spans(HEADING, PALE));

    // The buttons take AP35's look: its ACCEPT's art, font and colours.
    let look = prompt.control(panels::prompt::ACCEPT)?;
    let listing = list(&mut canvas, picker, (look, font), rows);
    let pressed = buttons(&mut canvas, (look, prompt), rows);
    let asked = listing.or(pressed);

    // A question or a note over the page takes the presses.
    match (picker.mode, picker.said.clone()) {
        (Mode::Delete(i), _) => {
            let name = picker.listed.get(i).map_or("", |l| l.name.as_str());
            let question = format!("Delete {name} and its saves?");

            match paint::short(
                &mut canvas,
                message,
                (DELETE_TITLE, &question),
                Ask::Question,
            ) {
                Some(message::YES) => picker.delete(i, player),
                Some(message::NO) => picker.mode = Mode::Choose,
                _ => {}
            }

            None
        }
        (_, Some(why)) => {
            if paint::short(&mut canvas, message, (ERROR_TITLE, &why), Ask::Tell).is_some() {
                picker.said = None;
            }

            None
        }
        (_, None) => asked.and_then(|asked| picker.act(asked, player)),
    }
}

impl Picker {
    /// What `asked` does: a row picked, Delete's question asked, the
    /// name prompt opened, the picked profile taken, or the page left.
    fn act(&mut self, asked: Asked, player: &mut Player) -> Option<Press> {
        match asked {
            Asked::Pick(i) => {
                self.chosen = Some(i);
                None
            }
            Asked::Delete(i) => {
                self.mode = Mode::Delete(i);
                None
            }
            Asked::New => {
                self.mode = Mode::Create;
                self.caret = Caret::SelectAll;
                None
            }
            Asked::Continue => {
                let chosen = self.chosen.and_then(|i| self.listed.get(i)).cloned();

                chosen
                    .zip(self.store.clone())
                    .and_then(|(listed, store)| self.take(store.open(&listed.dir), player))
            }
            Asked::Cancel => Some(Press::Button(Button::Cancel)),
        }
    }
}

/// The list's rows in the plate, `rows` of them with New profile, the
/// picked lit with its delete button, and the scroll bar where the
/// profiles do not all show; what a row asked.
fn list(
    canvas: &mut Canvas<'_>,
    picker: &mut Picker,
    (look, font): (&ShownControl, Font),
    rows: usize,
) -> Option<Asked> {
    let count = picker.listed.len();
    let slots = rows - 1;
    let scrolls = count > slots;
    let right = if scrolls {
        BAR_LEFT - BAR_GAP
    } else {
        ROW_RIGHT
    };

    if scrolls {
        bar(canvas, picker, look, rows);
    }

    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a list's rows"
    )]
    let first = (picker.scroll.round() as usize).min(count.saturating_sub(slots));
    let shown = picker.listed.iter().enumerate().skip(first).take(slots);
    let mut asked = None;

    for (row, (i, listed)) in shown.enumerate() {
        let band = row_band(row, right);
        let lit = picker.chosen == Some(i);
        let response = canvas.ui.interact(
            canvas.fit.rect(band),
            canvas.ui.id().with(("profile row", i)),
            egui::Sense::click(),
        );

        if tapped(&response) {
            asked = Some(Asked::Pick(i));
        }

        if lit {
            canvas
                .painter()
                .rect_filled(canvas.fit.rect(band), 0.0, LIT);
        }

        let radio = RADIO.slot(usize::from(lit));

        canvas.picture(Pack::Base, radio, pos2(RADIO_LEFT, band.top() + 1.0));

        // The name cut where the delete button starts.
        let delete_left = right - DELETE_GAP - DELETE_WIDTH;
        let room = Rect::from_x_y_ranges(band.left()..=delete_left, band.y_range());

        left(canvas, (font, room), &listed.name, PALE);

        if lit && delete(canvas, look, (font, band.top(), delete_left)) {
            asked = Some(Asked::Delete(i));
        }
    }

    // New profile, pinned below the list: no tick box, a plus in its
    // column.
    let band = row_band(slots.min(count), right);
    let response = canvas.ui.interact(
        canvas.fit.rect(band),
        canvas.ui.id().with("new profile"),
        egui::Sense::click(),
    );
    let plus = Rect::from_center_size(pos2(PLUS_MIDDLE, band.center().y), vec2(ROW, ROW));

    canvas.centred_in(font, plus, &font::spans(NEW_MARK, PALE));
    left(canvas, (font, band), NEW, Color32::WHITE);

    if tapped(&response) {
        asked = Some(Asked::New);
    }

    asked
}

/// Row `row`'s band in the plate, its lit band's width, to `right`.
#[allow(clippy::cast_precision_loss, reason = "a list's rows")]
fn row_band(row: usize, right: f32) -> Rect {
    let top = FIRST_ROW + row as f32 * ROW;

    Rect::from_min_max(pos2(ROW_LEFT, top), pos2(right, top + ROW))
}

/// `text` on one line from [`NAME_LEFT`] in `room`, cut to it, centred up
/// and down, in `font` and `colour`.
fn left(canvas: &Canvas<'_>, (font, room): (Font, Rect), text: &str, colour: Color32) {
    let pen = pos2(
        NAME_LEFT,
        room.center().y + font.cap(canvas.fit.scale) / 2.0,
    );
    let painter = canvas.painter().with_clip_rect(canvas.fit.rect(room));

    font.draw(&painter, canvas.fit, pen, &font::spans(text, colour));
}

/// The picked row's delete button, from `left` on the row whose top is
/// `top`, its label in `font` made small; whether it was pressed.
fn delete(
    canvas: &mut Canvas<'_>,
    look: &ShownControl,
    (font, top, left): (Font, f32, f32),
) -> bool {
    let rect = Rect::from_min_size(pos2(left, top + 1.0), vec2(DELETE_WIDTH, ROW - 2.0));
    let button = ShownControl {
        id: Some(DELETE_ID),
        art: Some(DELETE_ART),
        rect: Some(super::setup::whole(rect)),
        text: None,
        hotkey: None,
        ..look.clone()
    };
    let pressed = canvas.button(Pack::Base, &button, Able::Enabled);

    canvas.centred_in(
        font.times(DELETE_TEXT),
        rect,
        &font::spans(DELETE_LABEL, PALE),
    );
    pressed
}

/// The list's scroll bar: `AP27`'s track between `INDs`' arrows, a row a
/// step, the wheel over the list; the New row stays.
fn bar(canvas: &mut Canvas<'_>, picker: &mut Picker, look: &ShownControl, rows: usize) {
    let plate = plate_height(rows);
    let (top, bottom) = (PLATE_TOP + BAR_INSET, PLATE_TOP + plate - BAR_INSET);
    let track_left = BAR_LEFT + TRACK_IN;
    let track_end = bottom - ARROW + TRACK_UNDER;
    let kept = canvas.clip;

    // The track's tiles, from under the up arrow to over the down one.
    let mut y = top + ARROW - TRACK_UNDER;

    while y < track_end {
        let tile = Rect::from_min_size(pos2(track_left, y), TRACK.size())
            .intersect(Rect::everything_above(track_end));

        canvas.clip = Some(tile);
        canvas.picture(Pack::Base, TRACK_PIC, tile.min - TRACK.min.to_vec2());
        y += TRACK.height();
    }

    canvas.clip = kept;

    let rect = Rect::from_min_max(pos2(BAR_LEFT, top), pos2(BAR_LEFT + BAR_WIDTH, bottom));
    let control = ShownControl {
        id: Some(BAR_ID),
        art: Some(BAR_ART),
        rect: Some(super::setup::whole(rect)),
        ..look.clone()
    };
    let list = super::setup::whole(row_band(0, BAR_LEFT).union(row_band(rows - 1, BAR_LEFT)));

    canvas.list_bar(
        Pack::Base,
        (&control, Some(list)),
        (&mut picker.scroll, picker.listed.len(), rows - 1),
        "profile list",
    );
}

/// CONTINUE and CANCEL in `look`'s art below the list; what was
/// pressed. CANCEL's words are `prompt`'s own.
fn buttons(
    canvas: &mut Canvas<'_>,
    (look, prompt): (&ShownControl, &Layout),
    rows: usize,
) -> Option<Asked> {
    let top = BUTTONS_TOP + grown(rows);
    let cancel = prompt
        .control(panels::prompt::CANCEL)
        .and_then(|c| c.text.clone())
        .unwrap_or_else(|| Button::Cancel.label().to_uppercase());
    let shown = [
        (
            CONTINUE_ID,
            CONTINUE_LEFT,
            CONTINUE.to_owned(),
            Asked::Continue,
        ),
        (CANCEL_ID, CANCEL_LEFT, cancel, Asked::Cancel),
    ];

    shown
        .into_iter()
        .fold(None, |pressed, (id, x, text, asked)| {
            #[allow(clippy::cast_possible_truncation, reason = "a page's whole pixels")]
            let rect = look.rect.map(|r| smnu::Rect {
                x: x as i32,
                y: top as i32,
                ..r
            });
            let button = ShownControl {
                id: Some(id),
                rect,
                text: Some(text),
                hotkey: None,
                ..look.clone()
            };
            let now = canvas.button(Pack::Base, &button, Able::Enabled);

            pressed.or(now.then_some(asked))
        })
}

/// The rows the frame holds for `count` profiles and New profile: no
/// fewer than its plate's own, no more than [`MOST_ROWS`].
fn frame_rows(count: usize) -> usize {
    (count + 1).clamp(BASE_ROWS, MOST_ROWS)
}

/// How much taller the frame is for `rows` rows than for its own.
#[allow(clippy::cast_precision_loss, reason = "a list's rows")]
fn grown(rows: usize) -> f32 {
    rows.saturating_sub(BASE_ROWS) as f32 * ROW
}

/// The list plate's height for `rows` rows.
fn plate_height(rows: usize) -> f32 {
    PLATE + grown(rows)
}

/// The frame's height for `rows` rows: its field cut, its band repeated.
fn frame_height(rows: usize) -> f32 {
    frame_strips(rows).last().map_or(0.0, |s| s.at + s.rows())
}

/// The frame's rows for `rows` list rows: its top to the field, the
/// stone below the field to the band, the band once a row past
/// [`BASE_ROWS`], then the band and the rest.
///
/// ```text
///  0..124  166..200  [200..222] × (rows − 4)  200..326
/// ```
fn frame_strips(rows: usize) -> Vec<Strip> {
    const WHOLE: f32 = 326.0;

    let repeats = std::iter::repeat_n(BAND, rows.saturating_sub(BASE_ROWS));
    let runs = [(0.0, FIELD.0), (FIELD.1, BAND.0)]
        .into_iter()
        .chain(repeats)
        .chain(std::iter::once((BAND.0, WHOLE)));

    runs.scan(0.0, |at, (from, to)| {
        let strip = Strip {
            from,
            to,
            at: *at,
            flip: Flip::Plain,
        };

        *at += to - from;
        Some(strip)
    })
    .collect()
}

/// The list's first shown profile with `chosen` picked of `count`: the
/// top, or as far down as shows the picked on the last profile row.
#[allow(clippy::cast_precision_loss, reason = "a list's rows")]
fn shown_first(chosen: Option<usize>, count: usize) -> f32 {
    let slots = frame_rows(count) - 1;

    chosen.map_or(0, |i| (i + 1).saturating_sub(slots)) as f32
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

    /// The frame holds its own four rows for a few profiles, grows a row
    /// a profile to six and New profile, then holds seven.
    #[test]
    fn the_frame_grows_with_the_list_to_seven_rows() {
        assert_eq!(frame_rows(0), BASE_ROWS);
        assert_eq!(frame_rows(3), BASE_ROWS);
        assert_eq!(frame_rows(4), 5);
        assert_eq!(frame_rows(6), MOST_ROWS);
        assert_eq!(frame_rows(9), MOST_ROWS);
    }

    /// The frame less its field, 326 − 42 high at four rows; a row of
    /// the band more each row past them.
    #[test]
    fn the_frame_is_cut_by_its_field_and_grown_by_its_band() {
        assert_eq!(frame_height(BASE_ROWS), 284.0);
        assert_eq!(frame_height(MOST_ROWS), 284.0 + 3.0 * ROW);

        let strips = frame_strips(5);
        let runs: Vec<(f32, f32, f32)> = strips.iter().map(|s| (s.from, s.to, s.at)).collect();

        assert_eq!(
            runs,
            [
                (0.0, 124.0, 0.0),
                (166.0, 200.0, 124.0),
                (200.0, 222.0, 158.0),
                (200.0, 326.0, 180.0)
            ]
        );
    }

    /// The picked profile shows: the list scrolled to put it on the last
    /// profile row where it is further down.
    #[test]
    fn the_list_opens_scrolled_to_the_picked_profile() {
        assert_eq!(shown_first(Some(2), 3), 0.0);
        assert_eq!(shown_first(Some(4), 9), 0.0);
        assert_eq!(shown_first(Some(8), 9), 3.0);
        assert_eq!(shown_first(None, 9), 0.0);
    }

    #[test]
    fn no_data_directory_asks_nothing() {
        let mut picker = Picker::new(None);

        assert!(picker.start().is_none());
        assert!(!picker.ask());
    }
}
