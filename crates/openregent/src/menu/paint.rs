//! The menus drawn: the original's pictures where the decoded panels put
//! them, text in egui's font at its fonts' sizes (`font`), buttons
//! hit-tested at their rectangles. Each page is drawn at the interface's
//! scale, centred (`fit`); the sepia faces tile the whole window at the
//! interface's scale (`faces`). Each page takes what it
//! shows and returns what was pressed; the moves are `page`'s.
//!
//! ```text
//!  Main        AP29: room 1, its moving objects, frame 2000, cover 2;
//!              eight buttons, version; HD's mods row under it (`row`)
//!  Map         APdb: map, lit quests, stars, clouds, jewel 4034, frame
//!              8500-8513, title bar
//!  Extras      APd8 over the faces (`extras`)
//!  Erase       the map behind AP11 at its scale: title 8005, text 5000,
//!              Yes 5501, No 5502
//!  Hint        the map behind AP11 the same way, OK 5503 alone
//!  Quit        the main menu behind AP04: title 8005, text 5000, Yes
//!              5501, No 5001
//!  Leaving     the main menu, its exit picture 3000 played out
//!  Profiles    the main menu behind the profile page (`profiles`)
//!  Mods        the main menu behind the mods page (`mods`)
//!  Settings    drawn by the editor (`editor`, `settings`)
//!  Background  APc9: frame, icon 8730, title 8885, text 5002, bar 9101
//!  Summary     AP44: frame, still 8720 under ring 8886, texts 5002, 5003,
//!              bars 9101, 12
//!  Freestyle   APd2 centred on the screen: heading 8005, five buttons
//!  Loading     AP50, while a game is made: frame, still 5000, icon 8730,
//!              title 8885, covers 1-3; bar 5020 empty; a quest made,
//!              AP44's START QUEST centred on the bar's row
//!  Setup       MX17 as large as fits, centred (`setup`)
//! ```

use std::collections::HashMap;
use std::ops::RangeInclusive;

use bevy::log::debug;
use bevy_egui::egui::{self, Color32, Painter, Pos2, Rect, Sense, Vec2, pos2, vec2};
use regent_core::data::fourcc::FourCc;
use regent_core::data::imag::Playback;
use regent_core::data::smnu::{self, Kind};
use regent_core::view::menu::faces;
use regent_core::view::menu::hotkey;
use regent_core::view::menu::message::{self, Ask};
use regent_core::view::menu::quest_map::{self, Lock, Mark, Progress};
use regent_core::view::menu::room::{self, Rule};
use regent_core::view::menu::{
    self as panels, Animator, Hover, Layout, Role, ShownControl, State, Tick, briefing, ids,
    loading, music,
};

use super::art::{self, Art, COVER, Pack, Pic, Quest, ROOM, SPLASH, Store, Wrap};
use super::fit::{self, Fit, Lens};
use super::font::{self, spans};
use super::page::{Button, Page, Press};
use crate::game::load::Offered;
use crate::ui::scroll;
use regent_core::sim::mapgen::preset::Preset;
use regent_draw::art::{self as service, Answer, Place, Scene, Snap, Tone, Want, When, levels};
use regent_draw::stage::PIXELS_PER_NOTCH;

/// `AP44`'s GOALS heading.
const GOALS_HEADING: i32 = 9300;

/// The map's FREESTYLE QUESTS text under the compass.
const FREESTYLE_LABEL: i32 = 5900;

/// `MX17`'s ACCEPT.
const SETUP_ACCEPT: i32 = panels::setup::ACCEPT;

/// The painted quests' lit pictures. Slot 0 of each set holds a wooden
/// compass the map never shows (**Game**: `menu_map_top.png`); the
/// buildings at rest are painted into the map.
const PAINTED: FourCc = FourCc(*b"IX33");

/// The quest map's frame pieces (`quest_map::FIXED`, `INx2`).
const FRAME: RangeInclusive<i32> = 8500..=8513;

/// A text box's inner margin, layout pixels. **Game**: the summary's
/// lines start 5 px inside their box (`menu_barren_waste_start_summary.png`).
const PAD: f32 = 5.0;

/// How far inside a text box's sides its lines wrap, layout pixels.
/// **Inferred**, fitted to both captures: the briefing's longest line,
/// 440 by advances, fits its 447-wide box; the summary's 445-wide box
/// sends a 445-wide line on.
const WRAP_MARGIN: f32 = 2.0;

/// A heading box's end caps, layout pixels: its text stays between.
/// **Inferred** from `ui_save_game.png`: the caps of CURRENT GAME
/// DESCRIPTION's box about 15 px wide.
const HEADING_MARGIN: f32 = 16.0;

/// The briefing's text, its font's size and pitch times this. The user's
/// taste: `fn11` at its own 30.8 reads too large.
const BRIEFING_TEXT: f32 = 0.88;

/// The map's edge scroll, layout pixels a second. **Inferred**:
/// Scroll speed and its ramp are not decoded.
const SCROLL_SPEED: f32 = 800.0;

/// A disabled button's text, its colour this opaque. **Inferred**: the
/// original's disabled look is not read; its captures show none.
pub(super) const DISABLED: f32 = 0.45;

/// Text with no colour word: the pages' pale yellow, `0xffddaa`.
pub(super) const PALE: Color32 = Color32::from_rgb(0xff, 0xdd, 0xaa);

/// What the main menu's version line says.
const VERSION: &str = concat!("OpenRegent ", env!("CARGO_PKG_VERSION"));

/// A page's size when its panel gives none: the original's screen.
pub(super) const SCREEN: Vec2 = vec2(800.0, 600.0);

/// The scroll state of the page shown; reset on a new page.
#[derive(Default)]
pub struct View {
    /// The map's top left, layout space; `None` until it opens.
    scroll: Option<Pos2>,
    /// The map's zoom on the wheel.
    rung: fit::Rung,
    /// Each text box's offset, layout pixels.
    text: [f32; 2],
    /// The quest lit while no pointer is on the map.
    pub(super) hover: Option<usize>,
    /// The main menu's moving pictures, by control id.
    room: HashMap<i32, Animator>,
    /// The room picture lit last pass.
    lit: Option<i32>,
    /// The main menu or map control hovered while no pointer is on the
    /// page.
    pin: Option<i32>,
    /// The quest map's clouds, all on one frame.
    cloud: Animator,
    /// The map opened at this zoom, window pixels a map pixel, centred on
    /// the hovered quest: `--zoom`'s, for screenshots.
    shot_zoom: Option<f32>,
    /// The setup page's open list and typed Map Number.
    pub(super) setup: super::setup::Setup,
    /// The Build Tree Editor's tree, name and scroll.
    pub(super) tree: super::tree::Editor,
    /// The LOAD or SAVE page's list, pick and name.
    pub(super) presets: super::preset_page::Io,
    /// The page's video frame, set each pass: a briefing's, drawn in its
    /// porthole.
    pub(super) video: Option<crate::video::Frame>,
}

impl View {
    /// The main menu, control `pin` hovered with no pointer: `--menu`'s,
    /// for screenshots.
    pub fn pointed(pin: Option<i32>) -> View {
        View {
            pin,
            ..View::default()
        }
    }

    /// The map fitted, its top at `top` (kept inside the frame when
    /// drawn: 0 its top, −∞ its bottom), quest `hover` lit, or control
    /// `pin` hovered, with no pointer: `--menu`'s, for screenshots.
    pub fn pinned(top: Option<f32>, hover: Option<usize>, pin: Option<i32>) -> View {
        View {
            scroll: top.map(|y| pos2(0.0, y)),
            hover,
            pin,
            ..View::default()
        }
    }

    /// Opened at `zoom`, window pixels a map pixel (`shot_zoom`).
    pub fn zoomed(self, zoom: f32) -> View {
        View {
            shot_zoom: Some(zoom),
            ..self
        }
    }
}

/// Whether a button can be pressed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Able {
    Enabled,
    Disabled,
}

/// What a box's lines do past its height: spill over both edges, or
/// start at its top and be cut at its bottom.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Overflow {
    Spill,
    Cut,
}

/// How a text box sets its lines.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Align {
    Left,
    Centre,
}

/// Whether the map takes the pointer: not behind a dialog.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Live {
    Yes,
    No,
}

/// Draw `page`; what was pressed, if anything. `main` picks the main
/// menu's art; `failed` is a game that would not start; `progress` the
/// profile's victories.
pub fn page(
    ctx: &egui::Context,
    (art, shared, view): (&mut Art, &mut service::Art, &mut View),
    page: Page,
    quests: &[Offered],
    (main, failed, progress): (Pack, Option<&str>, &Progress),
) -> Option<Press> {
    let on_map = matches!(page, Page::Map | Page::Extras | Page::Erase | Page::Hint(_));

    // The map's larger levels are let go off its pages, and its 2× made
    // ahead, so the map opens sharp.
    if !on_map {
        shared.rest();
        ahead(ctx, art, shared);
    }

    match page {
        Page::Main => {
            // A slow pass logged: pictures made at once (`Art::get`).
            let start = std::time::Instant::now();
            let press = main_menu(ctx, (art, shared, view), (main, Exit::Hidden));
            let ms = start.elapsed().as_millis();

            if ms > SLOW_MS {
                debug!("menu: main menu pass {ms} ms, held {:?}", shared.stats());
            }

            press
        }
        Page::Map => map(ctx, (art, shared, view), (quests, progress), Live::Yes),
        Page::Erase => {
            let words = &art.book.words;
            let title = words
                .gmtx(quest_map::ERASE_TITLE_TEXT)
                .unwrap_or_else(|| Button::EraseVictories.label().to_owned());
            let text = words.gmtx(quest_map::ERASE_TEXT);

            map(ctx, (art, shared, view), (quests, progress), Live::No);
            message(ctx, (art, shared), (Some(title), text), Ask::Question)
        }
        Page::Hint(text) => {
            let words = &art.book.words;
            let said = (words.gmtx(quest_map::HINT_TITLE_TEXT), words.gmtx(text));

            map(ctx, (art, shared, view), (quests, progress), Live::No);
            message(ctx, (art, shared), said, Ask::Tell)
        }
        Page::Settings | Page::Profiles | Page::Mods | Page::Loads => {
            main_menu(ctx, (art, shared, view), (main, Exit::Hidden));
            None
        }
        Page::Quit => {
            main_menu(ctx, (art, shared, view), (main, Exit::Hidden));
            quit(ctx, (art, shared))
        }
        Page::Leaving => main_menu(ctx, (art, shared, view), (main, Exit::Playing)),
        Page::Background(i) | Page::Summary(i) => {
            quest(ctx, (art, shared, view), page, (i, quests), failed)
        }
        Page::Freestyle => freestyle(ctx, (art, shared), failed),
        // Drawn with the config it edits (`setup`), the table it shows
        // (`scores`), the lobby it shows (`lan`), the list it shows
        // (`extras`).
        Page::Extras
        | Page::Setup
        | Page::LanSetup
        | Page::Trees(_)
        | Page::Presets(..)
        | Page::Scores
        | Page::Credits(_)
        | Page::Intro(_)
        | Page::Cinematics
        | Page::Film(_)
        | Page::Protocol
        | Page::Multiplayer
        | Page::Create
        | Page::Lobby => None,
    }
}

/// The main menu's moving pictures: their frames held while it is drawn
/// (`art::Art::strip`).
const MAIN_MENU: Scene = Scene("main menu");

/// The quest map's clouds and compass, held as the main menu's are.
const QUEST_MAP: Scene = Scene("quest map");

/// A main-menu pass slower than this is logged, milliseconds: a frame
/// missed at 50 frames a second.
const SLOW_MS: u128 = 20;

/// Where a press's click waits for `show`, in egui's frame memory.
const CLICK: &str = "menu click";

/// A press going down on an enabled control of class `kind`: its click
/// (`menu::music::click`), heard once a pass ([`click`]).
pub(super) fn arm(response: &egui::Response, kind: Option<Kind>) {
    let ctx = &response.ctx;
    let down = response.hovered() && ctx.input(|i| i.pointer.primary_pressed());

    if let Some(click) = music::click(kind).filter(|_| down) {
        ctx.data_mut(|d| d.insert_temp(egui::Id::new(CLICK), Some(click)));
    }
}

/// An egui button standing in for a dialog's: its click armed; whether
/// it was clicked.
pub fn tapped(response: &egui::Response) -> bool {
    arm(response, Some(Kind::Button));
    response.clicked()
}

/// The click a press made this pass, if any, taken.
pub fn click(ctx: &egui::Context) -> Option<FourCc> {
    ctx.data_mut(|d| d.remove_temp::<Option<FourCc>>(egui::Id::new(CLICK)))
        .flatten()
}

/// Whose loading page: an offered quest's, by index, or a freestyle
/// game's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shows {
    Quest(usize),
    Freestyle,
}

/// A freestyle game's title when `GMTX` 80 is not read.
const FREESTYLE_GAME: &str = "Freestyle Game";

/// Whether the loading page offers Start Quest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Offer {
    Nothing,
    StartQuest,
}

/// The loading page, `AP50`, filling the window while a game is made: its
/// dataset's frame, the game's still and icon in place of the panel's,
/// its title, the covers. The bar stays empty: no progress is drawn.
/// With `offer`, Start Quest; whether it was pressed, by a click or Enter.
/// Over the game (`layer`), no press reaches what is under it.
pub fn loading(
    ctx: &egui::Context,
    (art, shared): (&mut Art, &mut service::Art),
    (shows, offer): (Shows, Offer),
    quests: &[Offered],
    layer: super::cinema::Layer,
) -> bool {
    let Art { book, ink, .. } = art;
    let (quest, title) = match shows {
        Shows::Quest(i) => (book.quests.get(i), quests.get(i).map(|o| o.title.clone())),
        Shows::Freestyle => (
            book.freestyle_game.as_ref(),
            Some(
                book.words
                    .gmtx(quest_map::FREESTYLE_BANNER.title)
                    .unwrap_or_else(|| FREESTYLE_GAME.to_owned()),
            ),
        ),
    };
    let layout = quest.and_then(|q| book.loading(q.source));

    if let (Some(quest), Some(layout)) = (quest, layout) {
        let source = quest.source;
        let mut canvas = match layer {
            super::cinema::Layer::Menus => Canvas::filling(ctx, (ink, shared), size(layout)),
            super::cinema::Layer::OverGame => {
                let canvas = Canvas::over(ctx, (ink, shared), size(layout));

                sink(&canvas.ui);
                canvas
            }
        };

        canvas.backdrop();

        canvas.panel_art(source, layout);

        // Panel order: still, icon, then the covers over the still's edge
        // and the logo.
        for c in &layout.controls {
            let art = match c.id {
                Some(loading::STILL) => quest.segue,
                Some(ids::POG) => quest.icon,
                Some(loading::TITLE | loading::BAR) => None,
                _ => c.art.map(|(record, set)| Pic::of(record, set)),
            };

            if let (Some(pic), Some(r)) = (art, c.rect) {
                canvas.picture(source, pic, fit::rect(r).min);
            }
        }

        if let (Some(c), Some(title)) = (layout.control(loading::TITLE), title) {
            canvas.label(c, &title, pick(c, State::Normal));
        }

        // The summary's START QUEST, centred on the empty bar's row: a
        // departure, as a modern load is over before the still is seen.
        let summary = book.summary(source).filter(|_| offer == Offer::StartQuest);
        let start = summary.and_then(|s| s.control(briefing::NEXT));

        start.is_some_and(|c| {
            let clicked = canvas.button(source, &mid(c, layout), Able::Enabled);
            let keyed = summary
                .is_some_and(|s| hot(s, (&[Button::StartQuest], control), &keys(ctx)).is_some());

            clicked || keyed
        })
    } else {
        false
    }
}

/// The whole of `ui` taking the pointer: no press, hover or wheel reaches
/// the layers under it.
fn sink(ui: &egui::Ui) {
    ui.interact(ui.max_rect(), ui.id().with("sink"), Sense::click_and_drag());
}

/// The window black over the game, taking the pointer: a game with no
/// loading page, as it settles (`menu::drape`).
pub fn cover(ctx: &egui::Context) {
    let ui = over_game(ctx);

    ui.painter().rect_filled(ui.max_rect(), 0.0, Color32::BLACK);
    sink(&ui);
}

/// The whole window, over the game's interface.
fn over_game(ctx: &egui::Context) -> egui::Ui {
    egui::Ui::new(
        ctx.clone(),
        "film".into(),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::new(egui::Order::Foreground, "film".into()))
            .max_rect(ctx.viewport_rect()),
    )
}

/// `c` moved to the middle of `layout`'s width, its row kept.
fn mid(c: &ShownControl, layout: &Layout) -> ShownControl {
    let rect = c.rect.zip(layout.panel.rect).map(|(r, panel)| smnu::Rect {
        x: (panel.width - r.width) / 2,
        ..r
    });

    ShownControl { rect, ..c.clone() }
}

/// Whether the pointer is over window rectangle `rect` of `ui`, told
/// apart by `key`, no other layer's widget over it. By egui's hit test,
/// not `Ui::rect_contains_pointer`: that asks for an egui area, and the
/// pages over the game are drawn on bare layers.
pub(crate) fn pointed(
    ui: &egui::Ui,
    rect: Rect,
    key: impl std::hash::Hash + std::fmt::Debug,
) -> bool {
    ui.interact(rect, ui.id().with(("pointed", key)), Sense::hover())
        .contains_pointer()
}

/// The whole window, behind any dialog.
pub(super) fn root(ctx: &egui::Context) -> egui::Ui {
    egui::Ui::new(
        ctx.clone(),
        "menu".into(),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::background())
            .max_rect(ctx.viewport_rect()),
    )
}

/// The control's id that is `button` on its page.
const fn control(button: Button) -> i32 {
    match button {
        Button::PlayGame => panels::Button::Play.id(),
        Button::Multiplayer => panels::Button::Multiplayer.id(),
        Button::LoadGame => panels::Button::Load.id(),
        Button::AdjustSettings => panels::Button::Settings.id(),
        Button::Credits => panels::Button::Credits.id(),
        Button::HighScores => panels::Button::HighScores.id(),
        Button::Cinematic => panels::Button::Cinematic.id(),
        Button::ExitGame => panels::Button::Exit.id(),
        Button::EraseVictories => quest_map::ERASE,
        Button::FreestyleQuests => quest_map::FREESTYLE,
        Button::MainMenu => quest_map::MAIN_MENU,
        Button::Downloadable => quest_map::DOWNLOAD,
        Button::Continue | Button::StartQuest => briefing::NEXT,
        Button::Cancel => briefing::CANCEL,
        Button::Yes => message::YES,
        Button::No => message::NO,
        Button::Ok => message::OK,
        Button::BeginnerRandom => Preset::Beginner.control(),
        Button::AdvancedRandom => Preset::Advanced.control(),
        Button::ExpertRandom => Preset::Expert.control(),
        Button::ManualSettings => panels::MANUAL_SETTINGS,
        Button::Accept => SETUP_ACCEPT,
        Button::EditMenu => panels::setup::EDIT_MENU,
        Button::LoadOptions => panels::setup::LOAD,
        Button::SaveOptions => panels::setup::SAVE,
        Button::EraseScores => regent_core::view::menu::scores::id::ERASE,
        // The multiplayer pages map their own (`lan`).
        Button::CreateGame | Button::JoinGame | Button::StartGame | Button::Ready => NO_CONTROL,
    }
}

/// A button on no panel this module draws.
const NO_CONTROL: i32 = -1;

/// The control's id that is `button` on the pick page, `APd2`: its
/// Cancel is 5005.
const fn pick_control(button: Button) -> i32 {
    match button {
        Button::Cancel => panels::FREESTYLE_CANCEL,
        _ => control(button),
    }
}

/// A panel's size: its own rectangle's.
pub(super) fn size(layout: &Layout) -> Vec2 {
    layout.panel.rect.map_or(SCREEN, |r| fit::rect(r).size())
}

/// The colour of control `c`'s text in `state`: as resolved
/// (`font::ink`), else its word, else state 0's, else [`PALE`].
pub(super) fn pick(c: &ShownControl, state: State) -> Color32 {
    font::ink(c, Role::Text, state).unwrap_or_else(|| pick_or(&c.colours, state.index(), PALE))
}

/// The colour word for `state`, else state 0's, else `fallback`.
fn pick_or(words: &[i32], state: usize, fallback: Color32) -> Color32 {
    let at = |i: usize| words.get(i).copied().and_then(font::colour);

    at(state).or_else(|| at(0)).unwrap_or(fallback)
}

/// A picture `width` wide cut to span `from` to `to`: each piece's left and
/// right on the page, and where the whole picture is placed to show it.
///
/// ```text
///  picture  [cap|   middle   |cap]
///  span     [cap|   middle   |   middle   |cap]    the middle repeated, the
///                                                  last repeat cut short
/// ```
///
/// A span no wider than the picture is its left part, and its right cap.
fn slices((from, to): (f32, f32), width: f32, cap: f32) -> Vec<(f32, f32, f32)> {
    let strip = (width - 2.0 * cap).max(1.0);
    let (start, end) = (from + cap, to - cap);
    let count = ((end - start) / strip).ceil().max(0.0);
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a few repeats of a button's picture"
    )]
    let middle = (0..count as u32).map(|n| {
        #[allow(clippy::cast_precision_loss, reason = "a few repeats")]
        let left = start + n as f32 * strip;

        (left, (left + strip).min(end), left - cap)
    });

    std::iter::once((from, start, from))
        .chain(middle)
        .chain(std::iter::once((end, to, to - width)))
        .collect()
}

/// One page's drawing: the window's root, the pictures, the fit.
pub(super) struct Canvas<'a> {
    pub(super) ui: egui::Ui,
    pub(super) ink: &'a mut Store,
    art: &'a mut service::Art,
    pub(super) fit: Fit,
    /// Drawing kept inside this layout rectangle.
    pub(super) clip: Option<Rect>,
    /// Buttons drawn now take their rectangle's width: their pictures cut
    /// with end caps this wide ([`slices`]).
    pub(super) caps: Option<f32>,
    /// The picture, by its layout rectangle, that pictures drawn now lie
    /// on, pixel for pixel: the room under its objects ([`Snap::Over`]).
    under: Option<Rect>,
    /// The button control drawn hovered: lit by its partner
    /// (`menu::room::LIT`), or held so by `--menu main:ID`.
    glow: Option<i32>,
    /// The button control whose picture is drawn elsewhere: the map's
    /// compass, which moves.
    skip: Option<i32>,
    /// A control's text rectangle (`SMNU` key 42), from its corner: its
    /// text is set there, not in its rectangle.
    text: Option<(i32, Rect)>,
}

impl<'a> Canvas<'a> {
    /// A `layout`-sized page centred at the menus' scale (`fit::scale`).
    pub(super) fn new(
        ctx: &egui::Context,
        ink: (&'a mut Store, &'a mut service::Art),
        layout: Vec2,
    ) -> Canvas<'a> {
        let ui = root(ctx);
        let fit = Fit::page(ui.max_rect(), layout);

        Canvas::at(ui, ink, fit)
    }

    /// A `layout`-sized page as large as the window holds: the screen
    /// pages, main menu and a quest's background and summary, exceptions
    /// to the menus' scale.
    pub(super) fn filling(
        ctx: &egui::Context,
        ink: (&'a mut Store, &'a mut service::Art),
        layout: Vec2,
    ) -> Canvas<'a> {
        let ui = root(ctx);
        let fit = Fit::of(ui.max_rect(), layout);

        Canvas::at(ui, ink, fit)
    }

    /// [`Canvas::filling`], over the game's interface.
    pub(super) fn over(
        ctx: &egui::Context,
        ink: (&'a mut Store, &'a mut service::Art),
        layout: Vec2,
    ) -> Canvas<'a> {
        let ui = over_game(ctx);
        let fit = Fit::of(ui.max_rect(), layout);

        Canvas::at(ui, ink, fit)
    }

    pub(super) const fn at(
        ui: egui::Ui,
        (ink, art): (&'a mut Store, &'a mut service::Art),
        fit: Fit,
    ) -> Canvas<'a> {
        Canvas {
            ui,
            ink,
            art,
            fit,
            clip: None,
            caps: None,
            under: None,
            glow: None,
            skip: None,
            text: None,
        }
    }

    pub(super) fn painter(&self) -> Painter {
        let painter = self.ui.painter().clone();

        match self.clip {
            Some(clip) => painter.with_clip_rect(self.fit.rect(clip)),
            None => painter,
        }
    }

    /// The pointer in layout space.
    fn pointer(&self) -> Option<Pos2> {
        self.ui.ctx().pointer_hover_pos().map(|p| self.fit.back(p))
    }

    /// `pic` at its own size, its top left at layout point `at`; where it
    /// went.
    pub(super) fn picture(&mut self, source: Pack, pic: Pic, at: Pos2) -> Option<Rect> {
        self.drawn(self.ink.want(source, pic), at)
    }

    /// `want` at its own size, its top left at layout point `at`; where it
    /// went.
    fn drawn(&mut self, want: Want, at: Pos2) -> Option<Rect> {
        let ctx = self.ui.ctx().clone();
        let place = Rect::from_min_size(at, self.ink.size(self.art, &ctx, want)?);
        let shown = self.shown(want, place)?;

        self.painter()
            .image(shown.id, shown.rect, shown.uv, Color32::WHITE);
        Some(place)
    }

    /// `want` drawn at layout rectangle `place`: what and where, as the
    /// art service answers.
    fn shown(&mut self, want: Want, place: Rect) -> Option<service::Shown> {
        let ctx = self.ui.ctx().clone();
        let place = self.place(place);

        self.ink.get(self.art, &ctx, want, place).map(Answer::now)
    }

    /// Layout rectangle `rect` on the window, as the art service takes it.
    fn place(&self, rect: Rect) -> Place {
        Place {
            scale: self.fit.scale,
            origin: self.fit.origin,
            rect,
            snap: self.under.map_or(Snap::Edges, Snap::Over),
        }
    }

    /// `pic`'s size, layout pixels.
    pub(super) fn size(&mut self, source: Pack, pic: Pic) -> Option<Vec2> {
        let ctx = self.ui.ctx().clone();

        let want = self.ink.want(source, pic);

        self.ink.size(self.art, &ctx, want)
    }

    /// `pic`, `size` big, drawn across `rect`'s width: see [`slices`].
    fn sliced(&mut self, source: Pack, pic: Pic, rect: Rect, (size, cap): (Vec2, f32)) {
        let kept = self.clip;

        for (from, to, x) in slices((rect.min.x, rect.max.x), size.x, cap) {
            self.clip = Some(Rect::from_x_y_ranges(from..=to, rect.y_range()));
            self.picture(source, pic, pos2(x, rect.min.y));
        }

        self.clip = kept;
    }

    /// `pic` repeated over layout rectangle `rect` from its top left.
    pub(super) fn tiled(&mut self, source: Pack, pic: Pic, rect: Rect) {
        let ctx = self.ui.ctx().clone();
        let want = self.ink.want(source, pic).wrap(Wrap::Tiled);

        if let Some(size) = self.ink.size(self.art, &ctx, want) {
            let uv = Rect::from_min_size(pos2(0.0, 0.0), rect.size() / size);

            if let Some(shown) = self.shown(want, Rect::from_min_size(rect.min, size)) {
                self.painter()
                    .image(shown.id, self.fit.rect(rect), uv, Color32::WHITE);
            }
        }
    }

    /// The sepia faces over the whole window, from its top left, at the
    /// interface's scale (`faces`, `layout::scale`): a portrait the size
    /// the game's selection panel draws it. Black, and a portrait in each
    /// cell.
    pub(super) fn backdrop(&mut self) {
        let ctx = self.ui.ctx().clone();
        let window = self.ui.max_rect();
        let painter = self.ui.painter().clone();
        let source = self.ink.source(Pack::Expansion);
        let scale = crate::ui::layout::scale(window.size());

        painter.rect_filled(window, 0.0, Color32::BLACK);

        #[allow(clippy::cast_precision_loss, reason = "a cell's 110 pixels")]
        let (cell, inset) = (faces::CELL as f32, faces::INSET as f32);
        let side = cell - 2.0 * inset;

        // A window with no height has no rows: NaN casts to 0.
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a window's cells"
        )]
        let [columns, rows] =
            [window.width(), window.height()].map(|v| (v / (cell * scale)).ceil() as usize);

        let cells = (0..rows).flat_map(|r| (0..columns).map(move |c| (c, r)));
        let faces = cells.filter_map(|(c, r)| Some((c, r, self.ink.faces.at(c, r)?)));

        for (column, row, pic) in faces {
            #[allow(clippy::cast_precision_loss, reason = "a window's cells")]
            let corner = (vec2(column as f32, row as f32) * cell + Vec2::splat(inset)).to_pos2();
            let want = Want::of(source, service::Picture::Pic(pic.pic())).tone(Tone::Sepia);
            let place = Place {
                scale,
                origin: window.min,
                rect: Rect::from_min_size(corner, Vec2::splat(side)),
                snap: Snap::Edges,
            };

            if let Some(shown) = self.ink.get(self.art, &ctx, want, place).map(Answer::now) {
                painter.image(shown.id, shown.rect, shown.uv, Color32::WHITE);
            }
        }
    }

    /// The panel's own picture, then the controls' that show at rest:
    /// the room, frame and cover, the porthole's ring, and `quest`'s icon
    /// and still in place of the panel's.
    fn pictures(&mut self, source: Pack, layout: &Layout, quest: Option<&Quest>) {
        let own = |c: &ShownControl| c.art.map(|(record, set)| (source, Pic::of(record, set)));

        if let Some((source, pic)) = own(&layout.panel) {
            self.picture(source, pic, pos2(0.0, 0.0));
        }

        for c in &layout.controls {
            let art = match c.id {
                Some(briefing::ZOOM) => quest.and_then(|q| q.zoom.map(|p| (q.source, p))),
                Some(ids::POG) => quest.and_then(|q| q.icon.map(|p| (q.source, p))),
                Some(ROOM | SPLASH | COVER | briefing::PORTHOLE) => own(c),
                _ => None,
            };

            if let (Some((source, pic)), Some(r)) = (art, c.rect) {
                self.picture(source, pic, fit::rect(r).min);
            }
        }
    }

    /// Frame `frame` of room picture slot `at`, its corner at `corner`,
    /// on `scene`.
    fn frame(
        &mut self,
        at: (Pack, FourCc, i32, usize),
        frame: usize,
        (corner, scene): (Pos2, Scene),
    ) {
        let ctx = self.ui.ctx().clone();
        let place = self.place(Rect::from_min_size(corner, Vec2::ZERO));

        if let Some(shown) = self.ink.frame(self.art, &ctx, (at, frame), (place, scene)) {
            let shown = shown.now();

            self.painter()
                .image(shown.id, shown.rect, shown.uv, Color32::WHITE);
        }
    }

    /// Moving picture `c` ticked at `now`, milliseconds, and drawn on
    /// `scene`: its lit slot's frames when `lit`, else its rest slot's.
    fn moving(
        &mut self,
        (source, scene): (Pack, Scene),
        c: &ShownControl,
        animator: &mut Animator,
        (lit, now): (bool, u64),
    ) {
        if let (Some((record, set)), Some(r)) = (c.art, c.rect) {
            let slot = if lit { room::LIT } else { room::REST };
            let at = (source, record, set, slot);
            let count = self.ink.strip(at).map_or(0, |run| run.tiles.len());

            animator.tick(now, count);

            self.frame(at, animator.shown(count), (fit::rect(r).min, scene));
        }
    }

    /// The headings: BACKGROUND, GOALS.
    fn headings(&mut self, layout: &Layout) {
        let headings = layout
            .controls
            .iter()
            .filter(|c| c.art.is_none() && matches!(c.id, Some(ROOM | GOALS_HEADING)));

        for c in headings {
            self.label(
                c,
                c.text.as_deref().unwrap_or_default(),
                pick(c, State::Normal),
            );
        }
    }

    /// `text` on one line centred in control `c`, in its font.
    pub(super) fn label(&mut self, c: &ShownControl, text: &str, colour: Color32) {
        if let Some(r) = c.rect {
            self.centred(c.font, fit::rect(r), &spans(text, colour));
        }
    }

    /// [`Canvas::label`], its font shrunk where the text is wider than
    /// the control less [`HEADING_MARGIN`] a side: the original's narrow
    /// capitals fit (`ui_save_game.png`'s CURRENT GAME DESCRIPTION).
    pub(super) fn fitted(&mut self, c: &ShownControl, text: &str, colour: Color32) {
        let font = c.font.and_then(|r| self.ink.font(r));

        if let (Some(r), Some(font)) = (c.rect, font) {
            let rect = fit::rect(r);
            let room = rect.width() - 2.0 * HEADING_MARGIN;
            let wide = font.width(self.ui.ctx(), self.fit.scale, text);
            let font = if wide > room && wide > 0.0 {
                font.times(room / wide)
            } else {
                font
            };

            self.centred_in(font, rect, &spans(text, colour));
        }
    }

    /// `parts` on one line centred in layout rectangle `rect`.
    pub(super) fn centred(
        &mut self,
        record: Option<FourCc>,
        rect: Rect,
        parts: &[(Color32, String)],
    ) {
        if let Some(font) = record.and_then(|r| self.ink.font(r)) {
            self.centred_in(font, rect, parts);
        }
    }

    /// `parts` on one line centred in layout rectangle `rect`, in `font`.
    fn centred_in(&mut self, font: font::Font, rect: Rect, parts: &[(Color32, String)]) {
        let line: String = parts.iter().map(|(_, s)| s.as_str()).collect();
        let wide = font.width(self.ui.ctx(), self.fit.scale, &line);
        let cap = font.cap(self.fit.scale);
        let pen = pos2(rect.center().x - wide / 2.0, rect.center().y + cap / 2.0);

        font.draw(&self.painter(), self.fit, pen, parts);
    }

    /// Control `c` as a button: its picture in its state, its text
    /// centred, the hot key letter in its colour; whether it was pressed.
    ///
    /// The picture's slot is `menu::drawn_slot`'s, hovered under the
    /// pointer or with its partner hovered ([`Canvas::glow`]), lit while
    /// held (main menu hover pair answers its own method); the
    /// colours index by the state, so hovering keeps them.
    pub(super) fn button(&mut self, source: Pack, c: &ShownControl, able: Able) -> bool {
        self.ticked(source, c, (able, Tick::Clear))
    }

    fn control_art_at(kind: Option<Kind>, rect: Rect, size: Vec2) -> Pos2 {
        if matches!(kind, Some(Kind::Check | Kind::Radio)) {
            pos2(rect.min.x, rect.center().y - size.y / 2.0)
        } else {
            rect.min
        }
    }

    /// Control `c` as [`Canvas::button`] draws it, its checked slots
    /// where `tick` is set and it is a check or radio box; whether it was
    /// pressed.
    pub(super) fn ticked(
        &mut self,
        source: Pack,
        c: &ShownControl,
        (able, tick): (Able, Tick),
    ) -> bool {
        let (Some(r), Some(id)) = (c.rect, c.id) else {
            return false;
        };
        let rect = fit::rect(r);
        let response = self.ui.interact(
            self.fit.rect(rect),
            self.ui.id().with(("menu button", id)),
            Sense::click(),
        );

        if able == Able::Enabled {
            arm(&response, c.kind);
        }

        let (_, _, press) = crate::panel::sheet::look(&response, Tick::Clear);
        let state = match able {
            Able::Enabled => State::of(c.flags).pressed(press),
            Able::Disabled => State::Disabled,
        };
        let hover = Hover::of(response.hovered() || self.glow == Some(id));
        let fade = |colour: Color32| match able {
            Able::Enabled => colour,
            Able::Disabled => colour.gamma_multiply(DISABLED),
        };

        if let Some((record, set)) = c.art.filter(|_| self.skip != Some(id)) {
            let pic = Pic::of(record, set);
            let count = (0..art::SLOTS)
                .take_while(|&s| self.size(source, pic.slot(s)).is_some())
                .count();
            let slot = panels::drawn_slot((tick.shown_by(c.kind), hover), state, count);
            let shown = pic.slot(slot);
            let size = self.size(source, shown).or_else(|| self.size(source, pic));
            let at = size.map_or(rect.min, |size| Self::control_art_at(c.kind, rect, size));

            match (self.caps, size) {
                (Some(cap), Some(size)) => self.sliced(source, shown, rect, (size, cap)),
                _ => {
                    self.picture(source, shown, at)
                        .or_else(|| self.picture(source, pic, at));
                }
            }
        }

        if let Some(text) = &c.text {
            let text = text.trim_end();
            // As resolved, disabled red; dimmed only where unresolved.
            let colour = font::ink(c, Role::Text, state)
                .unwrap_or_else(|| fade(pick_or(&c.colours, state.index(), PALE)));
            let hot = font::ink(c, Role::HotKey, state)
                .unwrap_or_else(|| fade(pick_or(&c.hotkey_colours, state.index(), font::HOT_KEY)));
            let parts = font::hot(text, c.hotkey, (colour, hot));
            let set = match self.text {
                Some((of, r)) if of == id => r.translate(rect.min.to_vec2()),
                _ => rect,
            };

            // A label too wide for one line, with no hot key letter, wraps
            // (**Game**: `ui_freestyle_setup.png`'s BUILDING EDIT MENU).
            if parts.len() == 1 && !self.fits(c.font, set, text) {
                self.lines((c.font, set), text, (colour, Overflow::Spill));
            } else {
                self.centred(c.font, set, &parts);
            }
        }

        able == Able::Enabled && response.clicked()
    }

    /// `text` in control `c`, its font times `size`, wrapped and set by
    /// `align`, scrolled by `scroll` with the wheel and scroll bar `bar`.
    pub(super) fn text_box(
        &mut self,
        (source, c, bar): (Pack, &ShownControl, Option<&ShownControl>),
        (text, align, size): (&str, Align, f32),
        scroll: &mut f32,
    ) {
        let (Some(r), Some(record)) = (c.rect, c.font) else {
            return;
        };
        let rect = fit::rect(r);
        let colour = pick(c, State::Normal);
        let ctx = self.ui.ctx().clone();
        let Some(font) = self.ink.font(record).map(|f| f.times(size)) else {
            return;
        };
        let scale = self.fit.scale;
        let lines = font.wrapped(
            &ctx,
            scale,
            (text, colour),
            rect.width() - 2.0 * WRAP_MARGIN,
        );
        let (pitch, cap) = (font.pitch, font.cap(scale));
        #[allow(clippy::cast_precision_loss, reason = "a box's lines")]
        let most = scroll::most(lines.len() as f32 * pitch, rect.height());
        let screen = self.fit.rect(rect);

        self.wheeled(screen, ("text box", c.id), (scroll, most));

        let painter = self.painter().with_clip_rect(screen);

        for (n, (line, parts)) in lines.iter().enumerate() {
            #[allow(clippy::cast_precision_loss, reason = "a box's lines")]
            let top = n as f32 * pitch;
            let x = match align {
                Align::Left => rect.left() + PAD,
                Align::Centre => rect.center().x - font.width(&ctx, scale, line) / 2.0,
            };
            let y = rect.top() + top + f32::midpoint(pitch, cap) - *scroll;

            font.draw(&painter, self.fit, pos2(x, y), parts);
        }

        self.barred(source, bar, (scroll, most, pitch));
    }

    /// Scroll bar `bar` over a text scrolled `by` of `most`: its arrows,
    /// pink while pressed, each stepping `step`, and its thumb, dragged
    /// (`scroll::Bar`); how far they move the text.
    pub(super) fn scroll_bar(
        &mut self,
        source: Pack,
        bar: &ShownControl,
        (by, most, step): (f32, f32, f32),
    ) -> f32 {
        let (Some(r), Some((record, set)), Some(id)) = (bar.rect, bar.art, bar.id) else {
            return 0.0;
        };
        let pic = |past: i32| Pic::of(record, set + past);
        let (Some(arrow), Some(thumb)) = (
            self.size(source, pic(0)),
            self.size(source, pic(scroll::THUMB)),
        ) else {
            return 0.0;
        };
        let bar = scroll::Bar {
            rect: fit::rect(r),
            arrow,
            thumb,
        };
        let scale = self.fit.scale;

        scroll::pass(
            self,
            (&bar, scale),
            (by, most, step),
            |c, rect, past| {
                let id = c.ui.id().with(("scroll arrow", id, past));

                c.ui.interact(c.fit.rect(rect), id, Sense::click())
            },
            |c, at| {
                let id = c.ui.id().with(("scroll thumb", id));

                c.ui.interact(c.fit.rect(at), id, Sense::drag())
            },
            |c, past, slot, at| {
                c.picture(source, pic(past).slot(slot), at);
            },
        )
    }

    /// `scroll` moved by the wheel over window rectangle `screen`, told
    /// apart by `key`, and kept within `most`.
    pub(super) fn wheeled(
        &self,
        screen: Rect,
        key: impl std::hash::Hash + std::fmt::Debug,
        (scroll, most): (&mut f32, f32),
    ) {
        if pointed(&self.ui, screen, key) {
            *scroll -= self.ui.ctx().input(|i| i.smooth_scroll_delta.y) / self.fit.scale;
        }

        *scroll = scroll.clamp(0.0, most);
    }

    /// `scroll` moved by scroll bar `bar`'s arrows, a `step` each, and its
    /// thumb, kept within `most`; no bar while the text fits.
    pub(super) fn barred(
        &mut self,
        source: Pack,
        bar: Option<&ShownControl>,
        (scroll, most, step): (&mut f32, f32, f32),
    ) {
        if let Some(bar) = bar.filter(|_| most > 0.0) {
            let moved = self.scroll_bar(source, bar, (*scroll, most, step));

            *scroll = (*scroll + moved).clamp(0.0, most);
        }
    }

    /// List bar `bar` over a list of `rows` rows, `shown` at once, in
    /// layout rectangle `list`: an arrow steps a row; wheel movement
    /// accumulates in rows over the list, told apart by `key`.
    pub(super) fn list_bar(
        &mut self,
        source: Pack,
        (bar, list): (&ShownControl, Option<smnu::Rect>),
        (scroll, rows, shown): (&mut f32, usize, usize),
        key: &str,
    ) {
        #[allow(clippy::cast_precision_loss, reason = "a list's rows")]
        let most = rows.saturating_sub(shown) as f32;
        let moved = self.scroll_bar(source, bar, (*scroll, most, 1.0));
        let notches = notches(self.ui.ctx());
        let wheeled = list_wheel(
            &self.ui,
            list.map(|r| self.fit.rect(fit::rect(r))),
            key,
            notches,
        );

        *scroll = (*scroll + moved + wheeled).clamp(0.0, most);
    }

    /// `text` typed in a frameless one-line box over layout rectangle
    /// `rect`, in font `record` and `colour`, at most `most` characters,
    /// set by `align`, `inset` from the box's sides, told apart by
    /// `salt`; its response, `None` without the font.
    pub(super) fn typed(
        &mut self,
        (rect, record, colour): (Rect, Option<FourCc>, Color32),
        (text, most, align): (&mut String, usize, egui::Align),
        (salt, inset): (impl egui::AsIdSalt, Inset),
    ) -> Option<egui::Response> {
        let points = self.ink.font(record?)?.points(self.fit.scale);
        let font = egui::FontId::proportional(points);
        let at = self.fit.rect(rect);
        let edit = egui::TextEdit::singleline(text)
            .id(self.ui.id().with(salt))
            .font(font.clone())
            .text_color(colour)
            .frame(egui::Frame::NONE)
            .horizontal_align(align)
            .vertical_align(egui::Align::Center)
            .char_limit(most)
            .desired_width(at.width());
        let place = match inset {
            Inset::Edges => at,
            Inset::Sides => at.shrink2(vec2(TYPED_INSET, 0.0)),
        };

        self.ui.visuals_mut().selection.bg_fill = SELECTION;

        // A row taller than the box set about its centre, cut to the box.
        let room = crate::ui::typeface::edit_room(self.ui.ctx(), &font, place);
        let clip = self.ui.clip_rect();

        self.ui.set_clip_rect(clip.intersect(place));

        let response = self.ui.put(room, edit);

        self.ui.set_clip_rect(clip);
        Some(response)
    }

    /// The panel's own picture, at its corner.
    pub(super) fn panel_art(&mut self, source: Pack, layout: &Layout) {
        if let Some((record, set)) = layout.panel.art {
            self.picture(source, Pic::of(record, set), pos2(0.0, 0.0));
        }
    }

    /// `text` in control `c`, wrapped, each line and the whole centred in
    /// it, in its font and colour. **Game**: the hint's lines and their
    /// block centred (`ui-quest-map-message.png`).
    pub(super) fn note(&mut self, c: &ShownControl, text: &str) {
        if let Some(r) = c.rect {
            let colour = pick(c, State::Normal);

            self.lines((c.font, fit::rect(r)), text, (colour, Overflow::Spill));
        }
    }

    /// As [`Canvas::note`], but lines past the box held from its top and
    /// cut at its bottom. **Inferred** from `ui_downloaded_quests.png`,
    /// whose five lines start at the box's top, the last cut, where
    /// `ui_load_game.png`'s four are centred.
    pub(super) fn held_note(&mut self, c: &ShownControl, text: &str) {
        if let Some(r) = c.rect {
            let colour = pick(c, State::Normal);

            self.lines((c.font, fit::rect(r)), text, (colour, Overflow::Cut));
        }
    }

    /// Whether `text` sets on one line in layout rectangle `rect`.
    pub(super) fn fits(&self, record: Option<FourCc>, rect: Rect, text: &str) -> bool {
        let font = record.and_then(|record| self.ink.font(record));

        font.is_none_or(|f| {
            f.width(self.ui.ctx(), self.fit.scale, text) <= rect.width() - 2.0 * WRAP_MARGIN
        })
    }

    /// `text` wrapped in layout rectangle `rect`, each line and the whole
    /// centred, in font `record` and `colour`, past it as `overflow` says.
    fn lines(
        &mut self,
        (record, rect): (Option<FourCc>, Rect),
        text: &str,
        (colour, overflow): (Color32, Overflow),
    ) {
        let font = record.and_then(|record| self.ink.font(record));

        if let Some(font) = font {
            let ctx = self.ui.ctx().clone();
            let scale = self.fit.scale;
            let width = rect.width() - 2.0 * WRAP_MARGIN;
            let lines = font.wrapped(&ctx, scale, (text, colour), width);
            let (pitch, cap) = (font.pitch, font.cap(scale));
            #[allow(clippy::cast_precision_loss, reason = "a box's lines")]
            let centred = rect.center().y - lines.len() as f32 * pitch / 2.0;
            let (top, painter) = match overflow {
                Overflow::Spill => (centred, self.painter()),
                Overflow::Cut => (
                    centred.max(rect.min.y),
                    self.painter().with_clip_rect(self.fit.rect(rect)),
                ),
            };

            for (n, (line, parts)) in lines.iter().enumerate() {
                #[allow(clippy::cast_precision_loss, reason = "a box's lines")]
                let y = top + n as f32 * pitch + f32::midpoint(pitch, cap);
                let x = rect.center().x - font.width(&ctx, scale, line) / 2.0;

                font.draw(&painter, self.fit, pos2(x, y), parts);
            }
        }
    }

    /// `text` edited in text box `c`, centred, in its font and colour, the
    /// keys kept on it; `caret` selects it all once it has them, then
    /// keeps.
    fn field(&mut self, c: &ShownControl, text: &mut String, caret: &mut Caret) {
        let font = c.font.and_then(|record| self.ink.font(record)).is_some();

        if let (Some(r), true) = (c.rect, font) {
            let salt = ("text field", c.id);
            let id = self.ui.id().with(salt);
            let length = text.chars().count();

            // egui keeps a selection only on the focused field.
            let focused = self.ui.ctx().memory(|m| m.has_focus(id));

            if *caret == Caret::SelectAll && focused {
                let mut state =
                    egui::text_edit::TextEditState::load(self.ui.ctx(), id).unwrap_or_default();
                let all = egui::text::CCursorRange::two(
                    egui::text::CCursor::new(0),
                    egui::text::CCursor::new(length),
                );

                state.cursor.set_char_range(Some(all));
                state.store(self.ui.ctx(), id);
                *caret = Caret::Keep;
            }

            let response = self.typed(
                (fit::rect(r), c.font, pick(c, State::Normal)),
                (text, panels::prompt::LENGTH, egui::Align::Center),
                (salt, Inset::Edges),
            );

            if let Some(response) = response.filter(|r| !r.has_focus()) {
                response.request_focus();
            }
        }
    }

    /// The pointer taken from the page behind over window rectangle
    /// `window`: a dialog's, made before its own controls.
    pub(super) fn block(&self, window: Rect) {
        self.ui
            .interact(window, self.ui.id().with("dialog block"), Sense::click());
    }

    /// `buttons` from `layout`; the one pressed.
    fn buttons(&mut self, source: Pack, layout: &Layout, buttons: &[Button]) -> Option<Press> {
        self.buttons_by(source, layout, (buttons, control))
    }

    /// `buttons` from `layout`, each at the control `id` names; the one
    /// pressed.
    pub(super) fn buttons_by(
        &mut self,
        source: Pack,
        layout: &Layout,
        (buttons, id): (&[Button], fn(Button) -> i32),
    ) -> Option<Press> {
        buttons.iter().fold(None, |pressed, &button| {
            let now = layout
                .control(id(button))
                .is_some_and(|c| self.button(source, c, Able::Enabled));

            pressed.or(now.then_some(Press::Button(button)))
        })
    }
}

/// Wheel movement over a list, including its clickable rows.
fn list_wheel(ui: &egui::Ui, list: Option<Rect>, key: &str, notches: f32) -> f32 {
    if list.is_some_and(|r| pointed(ui, r, key)) {
        -notches
    } else {
        0.0
    }
}

/// The quest map's levels made ahead (`levels::Level::AHEAD`) while
/// another page shows.
fn ahead(ctx: &egui::Context, art: &Art, shared: &mut service::Art) {
    let Art { book, ink, .. } = art;
    let made = book.map.as_ref().and_then(|map| painted(book, map));

    if let Some(Painted { map, quests, under }) = made
        && let Some(levels) = ink.levels(
            shared,
            (Pack::Expansion, map),
            quests.into_iter(),
            under.into_iter(),
            When::Ahead,
        )
    {
        levels.ahead(ctx);
    }
}

/// The quest map's picture and its painted quests, those offered: each
/// its slot, lit rectangle and lit picture. What its levels are made
/// from. A downloadable quest has a picture of its own, slot 0 its
/// building over the map's art, slot 1 that lit: slot 0 is pasted into
/// the map at its corner, so it lights as a painted one does.
fn painted(book: &art::Book, map: &art::Map) -> Option<Painted> {
    let (record, set) = map.layout.panel.art?;
    let offered = |slot: usize| book.quests.iter().any(|q| q.slot == Some(slot));
    let (quests, under): (Vec<_>, Vec<_>) = map
        .sites
        .iter()
        .filter(|site| offered(site.slot))
        .filter_map(|site| {
            let l = site.lit?;
            let a = l.art?;
            let at = [l.rect.x, l.rect.y].map(|n| usize::try_from(n).ok());
            let size = [l.rect.width, l.rect.height].map(|n| usize::try_from(n).ok());
            let quest = levels::Quest {
                slot: site.slot,
                at: [at[0]?, at[1]?],
                size: [size[0]?, size[1]?],
            };
            let pic = Pic::of(a.record, a.set);
            let under = (a.record != PAINTED).then_some((quest.at, pic));

            Some(((quest, pic.slot(1)), under))
        })
        .unzip();

    Some(Painted {
        map: Pic::of(record, set),
        quests,
        under: under.into_iter().flatten().collect(),
    })
}

/// The quest map's levels' pictures ([`painted`]).
struct Painted {
    map: Pic,
    /// Each quest and its lit picture.
    quests: Vec<(levels::Quest, Pic)>,
    /// Pasted into the map at their corners.
    under: Vec<([usize; 2], Pic)>,
}

/// The main menu: room, its moving pictures, frame, the eight buttons,
/// the version line and HD's mods row (`row`). A room object under the
/// pointer lights and a click on it is its button's (`menu::room`).
#[allow(clippy::too_many_lines, reason = "the room, then each part over it")]
fn main_menu(
    ctx: &egui::Context,
    (art, shared, view): (&mut Art, &mut service::Art, &mut View),
    (main, exit): (Pack, Exit),
) -> Option<Press> {
    let Art { book, ink, .. } = art;
    let layout = book.main(main)?;
    let row = book.row.as_ref();
    let size = main_size(layout, row);
    let mut canvas = Canvas::filling(ctx, (ink, shared), size);
    let page = Rect::from_min_size(Pos2::ZERO, size);
    // Made before the buttons, which take a click first.
    let response = canvas.ui.interact(
        canvas.fit.rect(page),
        canvas.ui.id().with("main room"),
        Sense::click(),
    );
    // A page over the room (settings, saves) keeps its objects still.
    // Leaving, the room takes no pointer.
    let hovered = match canvas.pointer() {
        _ if exit == Exit::Playing => None,
        Some(p) if response.contains_pointer() => {
            layout.under(cell(p), &room::HIDDEN).and_then(|c| c.id)
        }
        Some(_) => None,
        None => view.pin,
    };
    let lit = hovered.map_or_else(room::Lit::default, |id| room::lit(rule(main), id));
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "milliseconds since start"
    )]
    let now = (ctx.input(|i| i.time) * 1000.0) as u64;

    canvas.backdrop();

    canvas.panel_art(main, layout);

    // The room's rectangle, its objects laid on its pixels.
    let mut room = None;

    for c in &layout.controls {
        match (c.id, c.art, c.rect) {
            (Some(id @ (ROOM | SPLASH | COVER)), Some((record, set)), Some(r)) => {
                let drawn = canvas.picture(main, Pic::of(record, set), fit::rect(r).min);

                if id == ROOM {
                    room = drawn;
                }
            }
            (Some(id), _, _) if art::moving(c) => {
                let animator = view
                    .room
                    .entry(id)
                    .or_insert_with(|| rested(canvas.ink, main, c));

                // Lit or let go this pass: the slot's playback from the frame shown.
                if lit.picture != view.lit && (lit.picture == Some(id) || view.lit == Some(id)) {
                    let slot = if lit.picture == Some(id) {
                        room::LIT
                    } else {
                        room::REST
                    };

                    animator.play(playing(canvas.ink, main, c, slot));
                }

                canvas.under = room;
                canvas.moving(
                    (main, MAIN_MENU),
                    c,
                    animator,
                    (lit.picture == Some(id), now),
                );
                canvas.under = None;
            }
            _ => {}
        }
    }

    view.lit = lit.picture;

    // Exit's picture, shown once the question says yes: played forward
    // to its last frame, then the game ends; no press is taken.
    let left = (exit == Exit::Playing).then(|| {
        let picture = layout.control(room::EXIT);

        picture.is_none_or(|c| {
            let animator = view.room.entry(room::EXIT).or_insert_with(|| {
                let mut animator = rested(canvas.ink, main, c);

                animator.mode = room::LEAVING;
                animator
            });

            canvas.under = room;
            canvas.moving((main, MAIN_MENU), c, animator, (false, now));
            canvas.under = None;

            let count = c
                .art
                .and_then(|(record, set)| canvas.ink.strip((main, record, set, room::REST)))
                .map_or(0, |run| run.tiles.len());

            room::left(animator.frame, count)
        })
    });

    if let Some(c) = layout.labelled(panels::VERSION_LABEL) {
        canvas.label(c, VERSION, pick(c, State::Normal));
    }

    let in_row = row.and_then(|r| super::row::draw(&mut canvas, r));

    canvas.glow = lit.button.map(panels::Button::id).or(hovered);
    ctx.request_repaint();

    let linked = hovered
        .and_then(room::link)
        .and_then(|(button, _)| of_control(button.id()));

    // A room object clicks as its button would.
    if let Some(c) = hovered
        .filter(|_| linked.is_some())
        .and_then(|id| layout.control(id))
    {
        arm(&response, c.kind);
    }

    let room = linked.filter(|_| response.clicked()).map(Press::Button);
    let pressed = canvas
        .buttons(main, layout, Page::Main.buttons())
        .or(room)
        .or(in_row);

    match left {
        Some(left) => left.then_some(Press::Ended),
        None => pressed,
    }
}

/// The main menu's size: `layout`'s, down to HD's mods row under it.
fn main_size(layout: &Layout, row: Option<&Layout>) -> Vec2 {
    let size = size(layout);

    #[allow(clippy::cast_precision_loss, reason = "a panel's pixels")]
    row.and_then(panels::row::bottom)
        .map_or(size, |bottom| vec2(size.x, size.y.max(bottom as f32)))
}

/// Whether the main menu shows Exit's picture playing out.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Exit {
    Hidden,
    Playing,
}

/// The layout pixel holding point `p`.
#[allow(clippy::cast_possible_truncation, reason = "a panel's pixels")]
const fn cell(p: Pos2) -> (i32, i32) {
    (p.x.floor() as i32, p.y.floor() as i32)
}

/// How the room takes the pointer with `source`'s art: the base `AP29`
/// as HD shows it, the expansion's as `MajX` and HD's `UIData` panels do
/// (`menu::room::Rule`).
const fn rule(source: Pack) -> Rule {
    match source {
        Pack::Base => Rule::Pairs,
        Pack::Expansion => Rule::Triggers,
    }
}

/// The page button whose control is `id`.
fn of_control(id: i32) -> Option<Button> {
    Page::Main
        .buttons()
        .iter()
        .copied()
        .find(|b| control(*b) == id)
}

/// Picture `c`'s slot `slot` playback, as anim playback reads it.
fn playing(ink: &Store, source: Pack, c: &ShownControl, slot: usize) -> Option<Playback> {
    let (record, set) = c.art?;

    ink.strip((source, record, set, slot))?.playback
}

/// Picture `c`'s animator as it starts: its rest slot's playback.
fn rested(ink: &Store, source: Pack, c: &ShownControl) -> Animator {
    let mut animator = Animator::default();

    animator.play(playing(ink, source, c, room::REST));
    animator
}

/// The quest map: the map under its frame, zoomed by the wheel, moved by a
/// drag and the frame's edges; a won quest starred, a locked one under its cloud; the
/// quest under the pointer lit and named in the title bar; a press on an
/// open one, or the jewel, or a button.
#[allow(clippy::too_many_lines, reason = "the map's whole page")]
fn map(
    ctx: &egui::Context,
    (art, shared, view): (&mut Art, &mut service::Art, &mut View),
    (quests, progress): (&[Offered], &Progress),
    live: Live,
) -> Option<Press> {
    let Art { book, ink, .. } = art;
    let original = book.map.as_ref()?;
    // The frame spread to the window's edges at the interface's scale
    // (`fit::spread`).
    let (spread, size) = fit::spread(ctx.viewport_rect());
    let map = &original.at(size);
    let mut canvas = Canvas::new(ctx, (ink, shared), map.frame);

    canvas.fit = spread;

    let frame = Rect::from_min_size(Pos2::ZERO, map.frame);
    let pointer = canvas.pointer().filter(|_| live == Live::Yes);
    let response = canvas.ui.interact(
        canvas.fit.rect(frame),
        canvas.ui.id().with("quest map"),
        Sense::click_and_drag(),
    );
    let page = canvas.fit;

    let focus = view.hover.and_then(|q| book.quests[q].slot);
    let lens = scrolled(ctx, (page, pointer), (map, &response, focus), view);

    // What is under the pointer: a quest or the jewel, not a frame piece.
    let pieces: Vec<&quest_map::Placed> = map
        .pieces
        .iter()
        .filter(|(id, _)| FRAME.contains(id))
        .map(|(_, piece)| piece)
        .collect();
    let covered = |p: Pos2| pieces.iter().any(|piece| fit::rect(piece.rect).contains(p));
    let open = pointer.filter(|p| frame.contains(*p) && !covered(*p));
    // The pointer on the map, map pixels.
    let aim = open.map(|p| lens.back(p));
    let quest_of = |slot: usize| book.quests.iter().position(|q| q.slot == Some(slot));
    let sites: Vec<(usize, &quest_map::Site)> = map
        .sites
        .iter()
        .filter_map(|s| Some((quest_of(s.slot)?, s)))
        .collect();
    let hovered = match aim {
        Some(m) => fit::hit(sites.iter().map(|&(q, s)| (q, fit::rect(s.hit.rect))), m),
        None => view.hover,
    };

    canvas.backdrop();

    // The map and what lies on it, in map pixels through the lens, kept
    // inside the frame.
    canvas.fit = lens.on(page);
    canvas.clip = Some(Rect::from_min_max(
        lens.back(frame.min),
        lens.back(frame.max),
    ));

    // The map, a painted quest lit under the pointer: one picture, from
    // its level for the scale (`levels`).
    let lit = hovered.and_then(|q| book.quests[q].slot);

    if let Some(Painted {
        map: picture,
        quests: painted,
        under,
    }) = painted(book, map)
    {
        let surface = canvas.painter();
        let fit = canvas.fit;

        if let Some(levels) = canvas.ink.levels(
            canvas.art,
            (Pack::Expansion, picture),
            painted.into_iter(),
            under.into_iter(),
            When::Now,
        ) {
            levels.draw((ctx, &surface), (fit.scale, fit.origin), lit);
        }
    }

    marks(
        &mut canvas,
        (book, &mut view.cloud),
        &sites,
        (quests, progress),
        hovered,
    );

    // The jewel, the compass or its text under the pointer.
    let jewel_at = map
        .layout
        .control(quest_map::DOWNLOAD)
        .and_then(|c| c.rect)
        .map(|r| fit::rect(r).min);
    let jewel = jewel_at
        .zip(canvas.size(Pack::Expansion, art::JEWEL))
        .map(|(at, size)| Rect::from_min_size(at, size));
    let on_jewel = aim.zip(jewel).is_some_and(|(m, j)| j.contains(m));
    let control = match pointer {
        Some(p) => compass_under(&map.layout, p).or(on_jewel.then_some(quest_map::DOWNLOAD)),
        None => view.pin,
    };

    // The jewel lit while hovered: one frame, no playback.
    if let Some(at) = jewel_at {
        let slot = lit_slot(quest_map::hover(quest_map::DOWNLOAD, control));

        canvas.picture(Pack::Expansion, art::JEWEL.slot(slot), at);
    }

    canvas.clip = None;
    canvas.fit = page;

    for piece in &pieces {
        if let Some(a) = piece.art {
            let pic = Pic::of(a.record, a.set);
            let rect = fit::rect(piece.rect);

            // A bar wider than its rectangle (`INx2` 1005) is cut to it;
            // narrower, it repeats, as the spread frame's bottom bars do.
            let short = canvas
                .size(Pack::Expansion, pic)
                .is_some_and(|s| rect.width() > s.x || rect.height() > s.y);

            if piece.tiled() || short {
                canvas.tiled(Pack::Expansion, pic, rect);
            } else {
                canvas.clip = Some(rect);
                canvas.picture(Pack::Expansion, pic, rect.min);
                canvas.clip = None;
            }
        }
    }

    match (hovered, control.and_then(quest_map::banner)) {
        (Some(q), _) => {
            let bar = book.words.title_bar(quests[q].quest.key, progress);

            title_bar(&mut canvas, &map.layout, &book.quests[q], bar);
        }
        (None, Some(b)) => banner(&mut canvas, &map.layout, &book.words, b),
        (None, None) => {}
    }

    // The compass, lit and moving while it or its text is hovered.
    if let Some(c) = map.layout.control(quest_map::FREESTYLE) {
        let hover = quest_map::hover(quest_map::FREESTYLE, control);
        let lit = hover == Hover::Over;
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "milliseconds since start"
        )]
        let now = (ctx.input(|i| i.time) * 1000.0) as u64;
        let animator = view
            .room
            .entry(quest_map::FREESTYLE)
            .or_insert_with(|| rested(canvas.ink, Pack::Expansion, c));

        if lit != (view.lit == Some(quest_map::FREESTYLE)) {
            animator.play(playing(canvas.ink, Pack::Expansion, c, lit_slot(hover)));
        }

        canvas.moving((Pack::Expansion, QUEST_MAP), c, animator, (lit, now));
        canvas.skip = Some(quest_map::FREESTYLE);
        view.lit = lit.then_some(quest_map::FREESTYLE);
    }

    // The jewel is drawn above, its press the map's.
    let drawn: Vec<Button> = Page::Map
        .buttons()
        .iter()
        .copied()
        .filter(|&b| b != Button::Downloadable)
        .collect();
    let pressed = canvas.buttons(Pack::Expansion, &map.layout, &drawn);
    // Shift and the left button pan (`dragged`); they press nothing.
    let shift = ctx.input(|i| i.modifiers.shift);
    let clicked = live == Live::Yes && response.clicked() && !shift;

    // The text presses as the compass does.
    canvas.text = map.label.map(|r| (FREESTYLE_LABEL, r));

    let label = map
        .layout
        .control(FREESTYLE_LABEL)
        .is_some_and(|c| canvas.button(Pack::Expansion, c, Able::Enabled));
    let pressed = pressed.or(label.then_some(Press::Button(Button::FreestyleQuests)));

    // A locked quest's hit region is disabled: hovered, never pressed.
    let unlocked = hovered.filter(|q| {
        quest_map::lock(quests[*q].quest.key, &book.words.rules, progress) == Lock::Open
    });
    let target = unlocked
        .and_then(|q| book.quests[q].slot)
        .and_then(|s| Some(quest_map::HIT + i32::try_from(s).ok()?))
        .or(on_jewel.then_some(quest_map::DOWNLOAD));

    if let Some(id) = target.filter(|_| live == Live::Yes) {
        arm(&response, map.layout.control(id).and_then(|c| c.kind));
    }

    pressed
        .or(unlocked.filter(|_| clicked).map(Press::Quest))
        .or((clicked && on_jewel).then_some(Press::Button(Button::Downloadable)))
}

/// The compass or its text under layout point `p`; the text, listed
/// later, first.
fn compass_under(layout: &Layout, p: Pos2) -> Option<i32> {
    [quest_map::FREESTYLE_TEXT, quest_map::FREESTYLE]
        .into_iter()
        .find(|&id| {
            layout
                .control(id)
                .and_then(|c| c.rect)
                .is_some_and(|r| fit::rect(r).contains(p))
        })
}

/// The slot a map picture draws: 6 hovered, else 0. The jewel's and the
/// compass's sets hold more than six slots, so `menu::drawn_slot` keeps
/// it; state 0, as hovering sets none (`menu::State`).
const fn lit_slot(hover: Hover) -> usize {
    match hover {
        Hover::Over => room::LIT,
        Hover::Away => room::REST,
    }
}

/// The title bar of the hovered jewel or compass: its pog and title, no
/// line (`quest_map::banner`).
fn banner(canvas: &mut Canvas<'_>, layout: &Layout, words: &art::Words, b: quest_map::Banner) {
    if let Some(r) = layout.control(quest_map::POG).and_then(|c| c.rect) {
        canvas.picture(
            Pack::Expansion,
            Pic::of(b.pog.record, b.pog.set),
            fit::rect(r).min,
        );
    }

    if let (Some(c), Some(title)) = (layout.control(quest_map::TITLE), words.gmtx(b.title)) {
        canvas.label(c, &title, pick(c, State::Normal));
    }
}

/// Each locked quest's cloud, looping on `animator`, and each won quest's
/// star, in map pixels.
fn marks(
    canvas: &mut Canvas<'_>,
    (book, animator): (&art::Book, &mut Animator),
    sites: &[(usize, &quest_map::Site)],
    (quests, progress): (&[Offered], &Progress),
    hovered: Option<usize>,
) {
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "milliseconds since start"
    )]
    let now = (canvas.ui.ctx().input(|i| i.time) * 1000.0) as u64;
    let swirl = book
        .cloud
        .map(|c| (Pack::Expansion, c.swirl.0, c.swirl.1, 0));
    let count = swirl
        .and_then(|at| canvas.ink.strip(at))
        .map_or(0, |run| run.tiles.len());

    if let Some(run) = swirl.and_then(|at| canvas.ink.strip(at)) {
        if animator.period == 0 {
            animator.play(run.playback);
        }

        animator.tick(now, count);
    }

    for &(q, site) in sites {
        let key = quests[q].quest.key;
        let lock = quest_map::lock(key, &book.words.rules, progress);
        let hover = if hovered == Some(q) {
            Hover::Over
        } else {
            Hover::Away
        };
        // Over the lit pictures, drawn before: a hovered cloud stays.
        let shown = quest_map::marks(lock, progress.victories(key), hover);
        let cloud = site
            .cloud
            .filter(|_| shown.contains(&Mark::Cloud))
            .zip(book.cloud);
        let star = site.star.filter(|_| shown.contains(&Mark::Star));

        if let (Some((placed, c)), Some(at)) = (cloud, swirl) {
            let corner = fit::rect(placed.rect).min;

            canvas.frame(at, animator.shown(count), (corner, QUEST_MAP));
            canvas.picture(Pack::Expansion, c.mark, corner + c.at);
        }

        if let Some((placed, a)) = star.and_then(|s| Some((s, s.art?))) {
            let corner = fit::rect(placed.rect).min;

            canvas.picture(Pack::Expansion, Pic::of(a.record, a.set), corner);
        }
    }

    canvas.ui.ctx().request_repaint();
}

/// The map's lens this frame: the last, else fitted and centred; zoomed
/// by the wheel about the cursor, moved by a drag and by a cursor at the
/// frame's edges; kept within the frame.
fn scrolled(
    ctx: &egui::Context,
    (fit, pointer): (Fit, Option<Pos2>),
    (map, response, focus): (&art::Map, &egui::Response, Option<usize>),
    view: &mut View,
) -> Lens {
    let frame = Rect::from_min_size(Pos2::ZERO, map.frame);
    // The painted map inside the bars (`fit::inside`).
    let (view_in, painted) = (
        fit::inside(frame),
        fit::inside(Rect::from_min_size(Pos2::ZERO, map.size)),
    );
    let least = fit::fitted(view_in, painted);
    // `--zoom`'s, about the hovered quest's hit region, held each pass:
    // the window takes its size after the first.
    let shot = view.shot_zoom.map(|z| {
        let quest = map.sites.iter().find(|s| Some(s.slot) == focus);

        view.rung.zoom = Some(z / (fit.scale * ctx.pixels_per_point()));
        quest.map_or(painted.center(), |s| fit::rect(s.hit.rect).center())
    });
    let zoom = view.rung.zoom.unwrap_or(least).max(least);
    let opened = match (view.scroll, shot) {
        (_, Some(m)) => Lens {
            zoom,
            at: view_in.center() - m.to_vec2() * zoom,
        },
        (Some(at), None) => Lens { zoom, at },
        (None, None) => Lens::centred(zoom, view_in, painted),
    };

    // The wheel over the frame steps the zoom, the cursor held still.
    let over = pointer.filter(|p| frame.contains(*p));
    let turned = over.map(|p| (p, notches(ctx))).filter(|(_, n)| *n != 0.0);
    let zoomed = match turned {
        Some((p, n)) => opened.about(view.rung.turn(&fit::ladder(least, fit.scale), n), p),
        None => opened,
    };

    #[allow(clippy::cast_precision_loss, reason = "a margin's pixels")]
    let margin = quest_map::EDGE as f32;
    let push = pointer.map_or(Vec2::ZERO, |p| fit::push(p, frame, margin));
    let edge = push * SCROLL_SPEED * ctx.input(|i| i.stable_dt);
    let drag = pointer.map_or(Vec2::ZERO, |_| dragged(ctx, response) / fit.scale);
    let lens = Lens {
        at: zoomed.at + edge + drag,
        ..zoomed
    }
    .bounded(view_in, painted);

    view.scroll = Some(lens.at);

    if push != Vec2::ZERO {
        ctx.request_repaint();
    }

    lens
}

/// The wheel's notches this pass, + in, as in play (`stage::turned`).
pub(super) fn notches(ctx: &egui::Context) -> f32 {
    ctx.input(|i| {
        i.events
            .iter()
            .map(|e| match e {
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta,
                    ..
                } => delta.y / PIXELS_PER_NOTCH,
                egui::Event::MouseWheel { delta, .. } => delta.y,
                _ => 0.0,
            })
            .sum()
    })
}

/// How far the pointer panned the map this pass, window points: a middle
/// drag, or a left one with Shift, as in play (`stage::pan`).
fn dragged(ctx: &egui::Context, response: &egui::Response) -> Vec2 {
    let shift = ctx.input(|i| i.modifiers.shift);
    let panning = response.dragged_by(egui::PointerButton::Middle)
        || (shift && response.dragged_by(egui::PointerButton::Primary));

    if panning {
        response.drag_delta()
    } else {
        Vec2::ZERO
    }
}

/// The title bar for `quest`: its round icon, and `bar`'s title and line
/// (`Words::title_bar`).
fn title_bar(
    canvas: &mut Canvas<'_>,
    layout: &Layout,
    quest: &Quest,
    bar: Option<(String, String)>,
) {
    let pog = layout.control(quest_map::POG).and_then(|c| c.rect);

    if let (Some(r), Some(icon)) = (pog, quest.icon) {
        canvas.picture(quest.source, icon, fit::rect(r).min);
    }

    if let Some((title, line)) = bar {
        if let Some(c) = layout.control(quest_map::TITLE) {
            canvas.label(c, &title, pick(c, State::Normal));
        }

        if let Some(c) = layout.control(quest_map::LINE) {
            canvas.label(c, &line, pick(c, State::Normal));
        }
    }
}

/// The message dialog `AP11` over the map, centred at the map's scale,
/// the interface's (the original draws both at their own size): its frame, `title`, the
/// text in its box, and the buttons `ask` shows; the one pressed, by a
/// click or its hot key (Enter OK, Y Yes, N No).
pub(super) fn message(
    ctx: &egui::Context,
    (art, shared): (&mut Art, &mut service::Art),
    (title, text): (Option<String>, Option<String>),
    ask: Ask,
) -> Option<Press> {
    let Art { book, ink, .. } = art;
    let layout = book.message.as_ref()?;
    let mut canvas = Canvas::new(ctx, (ink, shared), fit::MAP_LEAST);
    let window = canvas.ui.max_rect();
    let buttons: Vec<Button> = message::shown(ask)
        .iter()
        .filter_map(|&id| of_message(id))
        .collect();

    canvas.fit = Fit::centred(window, canvas.fit.scale, size(layout));
    canvas.block(window);
    canvas.pictures(Pack::Base, layout, None);

    if let (Some(c), Some(title)) = (layout.control(message::TITLE), title) {
        canvas.label(c, &title, pick(c, State::Normal));
    }

    if let (Some(c), Some(text)) = (layout.control(message::TEXT), text) {
        canvas.note(c, &text);
    }

    let clicked = canvas.buttons(Pack::Base, layout, &buttons);
    let keyed = hot(layout, (&buttons, control), &keys(ctx)).map(Press::Button);

    clicked.or(keyed)
}

/// Exit's question, `AP04`, centred over the main menu: its own title
/// and text, YES or NO by click or key.
fn quit(ctx: &egui::Context, (art, shared): (&mut Art, &mut service::Art)) -> Option<Press> {
    let Art { book, ink, .. } = art;
    let layout = book.quit.as_ref()?;
    let mut canvas = Canvas::new(ctx, (ink, shared), fit::MAP_LEAST);
    let window = canvas.ui.max_rect();
    let buttons = Page::Quit.buttons();

    // Its own ids: NO is 5001, as the menu's Load Game.
    canvas.ui = canvas.ui.new_child(egui::UiBuilder::new().id_salt("quit"));

    canvas.fit = Fit::centred(window, canvas.fit.scale, size(layout));
    canvas.block(window);
    canvas.pictures(Pack::Base, layout, None);

    if let Some(c) = layout.control(message::TITLE) {
        canvas.label(
            c,
            c.text.as_deref().unwrap_or_default(),
            pick(c, State::Normal),
        );
    }

    if let Some(c) = layout.control(message::TEXT) {
        canvas.note(c, c.text.as_deref().unwrap_or_default());
    }

    let clicked = canvas.buttons_by(Pack::Base, layout, (buttons, quit_control));
    let keyed = hot(layout, (buttons, quit_control), &keys(ctx)).map(Press::Button);

    clicked.or(keyed)
}

/// The control that is `button` on `AP04`.
const fn quit_control(button: Button) -> i32 {
    match button {
        Button::Yes => panels::quit::YES,
        Button::No => panels::quit::NO,
        _ => NO_CONTROL,
    }
}

/// The page button that is `AP11`'s control `id`.
const fn of_message(id: i32) -> Option<Button> {
    match id {
        message::OK => Some(Button::Ok),
        message::YES => Some(Button::Yes),
        message::NO => Some(Button::No),
        _ => None,
    }
}

/// The keys typed this pass, as the engine codes them:
/// Enter, Escape, and each character. Alt+Enter is the window's
/// (`fullscreen`), no Enter.
pub fn keys(ctx: &egui::Context) -> Vec<i32> {
    ctx.input(|i| {
        i.events
            .iter()
            .filter_map(|e| match e {
                egui::Event::Key {
                    key: egui::Key::Enter,
                    pressed: true,
                    repeat: false,
                    modifiers,
                    ..
                } if !modifiers.alt => Some(hotkey::ENTER),
                egui::Event::Key {
                    key: egui::Key::Escape,
                    pressed: true,
                    repeat: false,
                    ..
                } => Some(hotkey::ESCAPE),
                egui::Event::Text(t) => {
                    let mut chars = t.chars();

                    match (chars.next(), chars.next()) {
                        (Some(c), None) => i32::try_from(u32::from(c)).ok(),
                        _ => None,
                    }
                }
                _ => None,
            })
            .collect()
    })
}

/// The cursor keys pressed this pass, repeats too, as the engine codes
/// them (key window: `WM_KEYDOWN`, auto-repeat).
pub fn cursor_keys(ctx: &egui::Context) -> Vec<i32> {
    ctx.input(|i| {
        i.events
            .iter()
            .filter_map(|e| match e {
                egui::Event::Key {
                    key, pressed: true, ..
                } => match key {
                    egui::Key::PageUp => Some(hotkey::PAGE_UP),
                    egui::Key::ArrowRight => Some(hotkey::RIGHT),
                    egui::Key::PageDown => Some(hotkey::PAGE_DOWN),
                    egui::Key::ArrowDown => Some(hotkey::DOWN),
                    egui::Key::End => Some(hotkey::END),
                    egui::Key::ArrowLeft => Some(hotkey::LEFT),
                    egui::Key::Home => Some(hotkey::HOME),
                    egui::Key::ArrowUp => Some(hotkey::UP),
                    _ => None,
                },
                _ => None,
            })
            .collect()
    })
}

/// The first of `buttons` on `layout` whose hot key one of `pressed` is
/// (`hotkey::pick`), each at the control `id` names.
pub(super) fn hot(
    layout: &Layout,
    (buttons, id): (&[Button], fn(Button) -> i32),
    pressed: &[i32],
) -> Option<Button> {
    let offered: Vec<(&ShownControl, hotkey::Able)> = buttons
        .iter()
        .filter_map(|&b| Some((layout.control(id(b))?, hotkey::Able::Enabled)))
        .collect();

    pressed
        .iter()
        .find_map(|&p| hotkey::pick(offered.iter().copied(), p))
        .and_then(|c| buttons.iter().copied().find(|&b| c.id == Some(id(b))))
}

/// The name field's selection. **Game**: `ui-profile-name.png`.
pub(super) const SELECTION: Color32 = Color32::from_rgb(0x00, 0x44, 0x70);

/// Where a typed line sits in its box: against its edges, or
/// [`TYPED_INSET`] pixels in from its sides.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Inset {
    Edges,
    Sides,
}

/// [`Inset::Sides`]' pixels.
const TYPED_INSET: f32 = 3.0;

/// How the name field's text starts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Caret {
    /// All of it selected, as the prompt opens (**Game**:
    /// `ui-profile-name.png`).
    SelectAll,
    Keep,
}

/// The name prompt `AP35` over the main menu, centred at its scale: its
/// frame, heading, `name` edited in its field (at most
/// `menu::prompt::LENGTH` characters), why a name will not do under it,
/// ACCEPT and CANCEL; the one pressed, by a click or its hot key (Enter,
/// Escape).
pub fn prompt(
    ctx: &egui::Context,
    (art, shared): (&mut Art, &mut service::Art),
    (name, caret): (&mut String, &mut Caret),
    said: Option<&str>,
) -> Option<Button> {
    let Art { book, ink, .. } = art;
    let layout = book.prompt.as_ref()?;
    let mut canvas = Canvas::new(ctx, (ink, shared), SCREEN);
    let window = canvas.ui.max_rect();
    let panel = size(layout);
    let buttons = [Button::Accept, Button::Cancel];

    canvas.fit = Fit::centred(window, canvas.fit.scale, panel);
    canvas.block(window);
    canvas.pictures(Pack::Base, layout, None);

    if let Some(c) = layout.control(panels::prompt::HEADING) {
        canvas.label(
            c,
            c.text.as_deref().unwrap_or_default(),
            pick(c, State::Normal),
        );
    }

    if let Some(c) = layout.control(panels::prompt::FIELD) {
        canvas.field(c, name, caret);
    }

    if let Some(why) = said {
        let at = canvas.fit.point(pos2(panel.x / 2.0, panel.y));

        canvas.ui.painter().text(
            at,
            egui::Align2::CENTER_TOP,
            why,
            egui::FontId::proportional(14.0),
            Color32::RED,
        );
    }

    let clicked = canvas.buttons_by(Pack::Base, layout, (&buttons, prompt_control));
    let keyed = hot(layout, (&buttons, prompt_control), &keys(ctx)).map(Press::Button);

    match clicked.or(keyed) {
        Some(Press::Button(button)) => Some(button),
        _ => None,
    }
}

/// The control's id that is `button` on the name prompt, `AP35`.
const fn prompt_control(button: Button) -> i32 {
    match button {
        Button::Accept => panels::prompt::ACCEPT,
        Button::Cancel => panels::prompt::CANCEL,
        _ => control(button),
    }
}

/// A quest's background or summary page, filling the window: frame,
/// icon, title, its texts, the still on the summary, and why its game
/// would not start.
fn quest(
    ctx: &egui::Context,
    (art, shared, view): (&mut Art, &mut service::Art, &mut View),
    page: Page,
    (i, quests): (usize, &[Offered]),
    failed: Option<&str>,
) -> Option<Press> {
    let Art { book, ink, .. } = art;
    let (quest, offered) = (book.quests.get(i)?, quests.get(i)?);
    let source = quest.source;
    let (layout, boxes) = match page {
        Page::Summary(_) => (
            book.summary(source)?,
            vec![
                (
                    briefing::BACKGROUND,
                    ids::SCROLL_BAR,
                    &offered.quest.long,
                    Align::Centre,
                    1.0,
                ),
                (
                    briefing::GOALS,
                    briefing::GOALS_SCROLL,
                    &offered.quest.short,
                    Align::Left,
                    1.0,
                ),
            ],
        ),
        _ => (
            book.briefing(source)?,
            vec![(
                briefing::BACKGROUND,
                ids::SCROLL_BAR,
                &offered.quest.long,
                Align::Centre,
                BRIEFING_TEXT,
            )],
        ),
    };
    let mut canvas = Canvas::filling(ctx, (ink, shared), size(layout));

    canvas.backdrop();
    canvas.pictures(source, layout, Some(quest));

    // The briefing's video in the porthole, 1:1 at its corner, over its
    // picture.
    if let (Page::Background(_), Some(video), Some(r)) = (
        page,
        &view.video,
        layout.control(briefing::PORTHOLE).and_then(|c| c.rect),
    ) {
        let unit = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
        let (texture, rect) = video.at(ctx.pixels_per_point(), canvas.fit.rect(fit::rect(r)));

        canvas.painter().image(texture, rect, unit, Color32::WHITE);
    }

    canvas.headings(layout);

    if let Some(c) = layout.control(briefing::TITLE) {
        canvas.label(c, &offered.title, pick(c, State::Normal));
    }

    for ((text_id, bar_id, text, align, size), scroll) in boxes.into_iter().zip(&mut view.text) {
        if let Some(c) = layout.control(text_id) {
            let bar = layout.control(bar_id);

            canvas.text_box((source, c, bar), (text, align, size), scroll);
        }
    }

    if let (Some(why), Some(r)) = (failed, layout.control(briefing::GOALS).and_then(|c| c.rect)) {
        let at = canvas.fit.point(fit::rect(r).left_bottom());

        canvas.ui.painter().text(
            at,
            egui::Align2::LEFT_BOTTOM,
            why,
            egui::FontId::proportional(14.0),
            Color32::RED,
        );
    }

    canvas.buttons(source, layout, page.buttons())
}

/// The pick page, `APd2`: its frame centred on the screen, the heading
/// 8005, the five buttons; why a game would not start under it.
fn freestyle(
    ctx: &egui::Context,
    (art, shared): (&mut Art, &mut service::Art),
    failed: Option<&str>,
) -> Option<Press> {
    let Art { book, ink, .. } = art;
    let layout = book.freestyle.as_ref()?;
    let mut canvas = Canvas::new(ctx, (ink, shared), SCREEN);
    let panel = size(layout);

    canvas.backdrop();
    canvas.fit.origin += (SCREEN - panel) / 2.0 * canvas.fit.scale;
    canvas.pictures(Pack::Base, layout, None);

    if let Some(c) = layout.control(ids::TITLE) {
        canvas.label(
            c,
            c.text.as_deref().unwrap_or_default(),
            pick(c, State::Normal),
        );
    }

    if let Some(why) = failed {
        let at = canvas.fit.point(pos2(panel.x / 2.0, panel.y));

        canvas.ui.painter().text(
            at,
            egui::Align2::CENTER_TOP,
            why,
            egui::FontId::proportional(14.0),
            Color32::RED,
        );
    }

    canvas.buttons_by(
        Pack::Base,
        layout,
        (Page::Freestyle.buttons(), pick_control),
    )
}

#[cfg(test)]
mod tests {
    /// The pieces cover the span edge to edge, the caps the picture's own
    /// ends, each piece from inside the picture.
    #[test]
    #[allow(clippy::float_cmp, reason = "sums of the same small numbers")]
    fn a_cut_picture_covers_its_span() {
        for (from, to) in [(10.0, 110.0), (10.0, 140.0), (10.0, 350.0), (10.0, 60.0)] {
            let pieces = super::slices((from, to), 100.0, 6.0);

            assert_eq!(pieces[0], (from, from + 6.0, from));
            assert_eq!(pieces[pieces.len() - 1], (to - 6.0, to, to - 100.0));

            for pair in pieces.windows(2) {
                assert_eq!(pair[0].1, pair[1].0);
            }

            for (left, right, x) in pieces {
                assert!(left >= x && right <= x + 100.0, "{left} {right} {x}");
            }
        }
    }

    use super::*;

    #[test]
    fn session_radio_art_centres_in_its_row() {
        for (y, height) in [(189.0, 30.0), (188.0, 32.0), (190.0, 28.0)] {
            let rect = Rect::from_min_size(pos2(76.0, y), vec2(87.0, height));
            let at = Canvas::control_art_at(Some(Kind::Radio), rect, vec2(20.0, 20.0));

            assert!((at.y - 194.0).abs() < f32::EPSILON);
        }
    }

    #[test]
    fn small_wheel_move_over_a_row_stays_small() {
        let ctx = egui::Context::default();
        let window = Rect::from_min_size(Pos2::ZERO, egui::vec2(100.0, 100.0));
        let list = Rect::from_min_size(pos2(10.0, 10.0), egui::vec2(80.0, 80.0));
        let row = Rect::from_min_size(list.min, egui::vec2(80.0, 20.0));
        let mut moved = 0.0;

        for _ in 0..2 {
            let input = egui::RawInput {
                screen_rect: Some(window),
                events: vec![egui::Event::PointerMoved(row.center())],
                ..egui::RawInput::default()
            };

            let mut output = ctx.run_ui(input, |ui| {
                ui.interact(row, ui.id().with("row"), Sense::click());
                moved = list_wheel(ui, Some(list), "list", 0.1);
            });
            output.textures_delta.clear();
        }

        assert!((moved + 0.1).abs() < f32::EPSILON);
    }

    /// `AP11`'s three buttons with their keys, as `menu.rs` tests them.
    fn dialog() -> Layout {
        let button = |id, key| ShownControl {
            id: Some(id),
            hotkey: Some(key),
            ..ShownControl::default()
        };

        Layout {
            id: panels::MESSAGE,
            panel: ShownControl::default(),
            centred: true,
            style: None,
            controls: vec![
                button(message::OK, hotkey::ENTER),
                button(message::YES, i32::from(b'Y')),
                button(message::NO, i32::from(b'N')),
            ],
        }
    }

    fn shown(ask: Ask) -> Vec<Button> {
        message::shown(ask)
            .iter()
            .filter_map(|&id| of_message(id))
            .collect()
    }

    #[test]
    fn the_message_shows_its_pages_buttons() {
        assert_eq!(shown(Ask::Question), Page::Erase.buttons());
        assert_eq!(shown(Ask::Tell), Page::Hint(143).buttons());
    }

    #[test]
    fn a_messages_keys_press_only_its_shown_buttons() {
        let layout = dialog();
        let press = |ask, key: i32| hot(&layout, (&shown(ask), control), &[key]);

        assert_eq!(press(Ask::Tell, hotkey::ENTER), Some(Button::Ok));
        assert_eq!(press(Ask::Tell, i32::from(b'y')), None);
        assert_eq!(press(Ask::Question, i32::from(b'y')), Some(Button::Yes));
        assert_eq!(press(Ask::Question, i32::from(b'N')), Some(Button::No));
        assert_eq!(press(Ask::Question, hotkey::ENTER), None);
        assert_eq!(press(Ask::Question, hotkey::ESCAPE), None);
    }

    #[test]
    fn the_name_prompt_takes_enter_as_accept_and_escape_as_cancel() {
        let button = |id, key| ShownControl {
            id: Some(id),
            hotkey: Some(key),
            ..ShownControl::default()
        };
        let layout = Layout {
            id: panels::NAME_PROMPT,
            panel: ShownControl::default(),
            centred: true,
            style: None,
            controls: vec![
                button(panels::prompt::ACCEPT, hotkey::ENTER),
                button(panels::prompt::CANCEL, hotkey::ESCAPE),
            ],
        };
        let buttons = [Button::Accept, Button::Cancel];
        let press = |key| hot(&layout, (&buttons, prompt_control), &[key]);

        assert_eq!(press(hotkey::ENTER), Some(Button::Accept));
        assert_eq!(press(hotkey::ESCAPE), Some(Button::Cancel));
        assert_eq!(press(i32::from(b'a')), None);
    }

    /// The keys a pass with `events` reads.
    fn read(events: Vec<egui::Event>) -> Vec<i32> {
        let ctx = egui::Context::default();
        let input = egui::RawInput {
            events,
            ..egui::RawInput::default()
        };
        let mut typed = Vec::new();
        let mut output = ctx.run_ui(input, |ui| typed = keys(ui.ctx()));

        output.textures_delta.clear();

        typed
    }

    /// Alt+Enter is the window's (`fullscreen`): no Enter button fires.
    #[test]
    fn alt_enter_is_no_enter_key() {
        let enter = |modifiers| egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        };

        assert_eq!(read(vec![enter(egui::Modifiers::NONE)]), [hotkey::ENTER]);
        assert_eq!(read(vec![enter(egui::Modifiers::ALT)]), Vec::<i32>::new());
    }
}
