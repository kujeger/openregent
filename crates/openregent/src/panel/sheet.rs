//! A decoded panel drawn from its own controls: its background, its
//! frame and the pictures and labels that do not change, then the
//! controls a caller fills, each by id.
//!
//! ```text
//!  panel art (INTI 10xx)   tiled under the whole panel
//!  frame, icons, labels    in control order, unless the caller skips the id
//!  buttons                 their picture's slot by state, label centred
//! ```

use bevy_egui::egui::{Color32, Pos2, Rect, Response, pos2};
use regent_core::data::fourcc::FourCc;
use regent_core::data::smnu::Kind;
use regent_core::sim::world::guild::taxman::Step;
use regent_core::view::menu::{Hover, Layout, Press, Role, ShownControl, State, Tick, drawn_slot};
use regent_core::view::panel::tip;

use super::canvas::{Align, Canvas, WHITE};
use super::ink::{Pic, Tone};
use crate::menu::fit;
use crate::menu::font;

/// A control picture that stands for no picture: `INTI` 1016, 10 × 6,
/// on text boxes, lists and the title scroll.
pub(crate) const NO_ART: (FourCc, i32) = (FourCc(*b"INTI"), 1016);

/// The background family: a control's `INTI` picture is a texture the
/// panel art already gives, so a control draws none of its own.
const TEXTURES: FourCc = FourCc(*b"INTI");

/// The most slots a control's set is looked through for.
const MAX_SLOTS: usize = 16;

/// The SMNU flags of a control drawn without one, as the band's.
pub const UNFLAGGED: i32 = 0;

/// A disabled control's text keeps this share of its colour.
pub const DIMMED: f32 = 0.5;

pub use regent_core::view::menu::Able;

/// Whether a two-state control is lit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lit {
    On,
    Off,
}

impl Lit {
    /// The tick control `c` shows (`Tick::shown_by`): a button shows none.
    #[must_use]
    pub const fn tick(self, c: &ShownControl) -> Tick {
        let tick = match self {
            Lit::On => Tick::Checked,
            Lit::Off => Tick::Clear,
        };

        tick.shown_by(c.kind)
    }
}

/// How many slots `pic`'s set holds: the first missing ends them.
/// Counted once ([`super::ink::Ink::slots`]).
pub fn slots(canvas: &mut Canvas<'_>, pic: Pic) -> usize {
    canvas.ink.slots(canvas.source, pic).unwrap_or_else(|| {
        let count = (0..MAX_SLOTS)
            .take_while(|&s| canvas.size(pic.slot(s)).is_some())
            .count();

        canvas.ink.counted(canvas.source, pic, count);
        count
    })
}

/// A control's look under the pointer: `tick`, hovered, and held while
/// a press that began on it stays on it (`menu::State::pressed`).
#[must_use]
pub fn look(response: &Response, tick: Tick) -> (Tick, Hover, Press) {
    let held = response.hovered() && response.is_pointer_button_down_on();

    (tick, Hover::of(response.hovered()), Press::of(held))
}

/// The slot of `pic` a control draws (`menu::drawn_slot`, lit while
/// held), and its tone: a disabled control whose set holds no disabled
/// slot dimmed instead.
pub fn slot(
    canvas: &mut Canvas<'_>,
    pic: Pic,
    (tick, hover, press): (Tick, Hover, Press),
    state: State,
) -> (usize, Tone) {
    let count = slots(canvas, pic);
    let tone = match state {
        State::Disabled if count <= State::Disabled.index() => Tone::Dim,
        _ => Tone::Plain,
    };

    (drawn_slot((tick, hover), state.pressed(press), count), tone)
}

/// The panel's own rectangle, from its top left.
#[must_use]
pub fn bounds(layout: &Layout) -> Rect {
    layout.panel.rect.map_or(Rect::ZERO, |r| {
        Rect::from_min_size(Pos2::ZERO, fit::rect(r).size())
    })
}

/// A control's rectangle.
#[must_use]
pub fn rect(c: &ShownControl) -> Option<Rect> {
    c.rect.map(fit::rect)
}

/// Its text colour at rest ([`ink`]).
#[must_use]
pub fn colour(c: &ShownControl) -> Color32 {
    ink(c, State::Normal)
}

/// Its text colour in `state`: its own or its panel palette's
/// (`font::ink`; **Game**: orange at rest, white lit, red disabled).
/// A control made in code: its first colour word, else white, dimmed
/// disabled.
#[must_use]
pub fn ink(c: &ShownControl, state: State) -> Color32 {
    font::ink(c, Role::Text, state).unwrap_or_else(|| {
        let own = c
            .colours
            .first()
            .copied()
            .and_then(font::colour)
            .unwrap_or(WHITE);

        dim(own, state)
    })
}

/// Its hot key letter's colour in `state`, as [`ink`]; a control made
/// in code: its first word, else `font::HOT_KEY`.
#[must_use]
pub fn hot_ink(c: &ShownControl, state: State) -> Color32 {
    font::ink(c, Role::HotKey, state).unwrap_or_else(|| {
        let own = c
            .hotkey_colours
            .first()
            .copied()
            .and_then(font::colour)
            .unwrap_or(font::HOT_KEY);

        dim(own, state)
    })
}

pub use regent_core::view::map::cursor::Purse;

/// A price's colour in control `id` of `layout`: its own at rest, its
/// disabled one (red) while short. **Inferred**: a short price red, the
/// earlier stand-in's rule; the engine's is not read.
#[must_use]
pub fn price_ink(layout: &Layout, id: i32, purse: Purse) -> Option<Color32> {
    layout.control(id).map(|c| price_in(c, purse))
}

/// [`price_ink`] of control `c`.
#[must_use]
pub fn price_in(c: &ShownControl, purse: Purse) -> Color32 {
    let state = match purse {
        Purse::Holds => State::Normal,
        Purse::Short => State::Disabled,
    };

    ink(c, state)
}

/// The text colour in `state` of a control of `layout` that sets none,
/// its palette's (orange at rest, red disabled): for a text drawn where
/// no control is known. White without a palette.
#[must_use]
pub fn plain(layout: &Layout, state: State) -> Color32 {
    layout.style.map_or_else(
        || dim(WHITE, state),
        |s| {
            let [_, r, g, b] = s.word(Role::Text, state).to_be_bytes();

            Color32::from_rgb(r, g, b)
        },
    )
}

/// `colour` dimmed when `state` is disabled.
fn dim(colour: Color32, state: State) -> Color32 {
    match state {
        State::Disabled => colour.gamma_multiply(DIMMED),
        State::Normal | State::Lit => colour,
    }
}

/// The panel's background and every control `skip` leaves: pictures at
/// rest, labels in their fonts. `backdrop` replaces the panel art (a
/// hero's stonework).
pub fn statics(
    canvas: &mut Canvas<'_>,
    layout: &Layout,
    backdrop: Option<Pic>,
    skip: &dyn Fn(i32) -> bool,
) {
    ground(canvas, layout, backdrop);
    controls(canvas, layout, &layout.controls, skip);
}

/// The panel's background: its art tiled, or `backdrop`.
pub fn ground(canvas: &mut Canvas<'_>, layout: &Layout, backdrop: Option<Pic>) {
    let whole = bounds(layout);

    match (backdrop, layout.panel.art) {
        (Some(pic), _) => {
            canvas.picture(pic, Tone::Plain, Pos2::ZERO);
        }
        (None, Some((record, set))) => canvas.tiled(Pic::of(record, set), whole),
        (None, None) => {}
    }
}

/// The controls of `list`, a run of `layout`'s, that `skip` leaves, in
/// list order.
pub fn controls(
    canvas: &mut Canvas<'_>,
    layout: &Layout,
    list: &[ShownControl],
    skip: &dyn Fn(i32) -> bool,
) {
    for c in list {
        if c.id.is_some_and(skip) {
            continue;
        }

        // A control in the panel's own art is that art: `AP21`'s STR to
        // Resist buttons are `INTI` 1003, as the panel. Drawn whole, the
        // 128 × 128 texture spills past the panel.
        if c.art != layout.panel.art {
            picture(canvas, c);
        }

        // A text box's number is the layout's placeholder: "2000",
        // "15 of 72", "+2". What it would say is not read.
        let placeholder =
            |t: &str| c.kind == Some(Kind::Textbox) && t.chars().any(|ch| ch.is_ascii_digit());

        if let (Some(text), Some(r)) = (c.text.as_deref().filter(|t| !placeholder(t)), rect(c)) {
            label(canvas, c, r, text);
        }

        if let (Some(tip), Some(r)) = (c.tooltip.as_deref(), rect(c)) {
            let response = canvas.hover(
                r,
                ("static tip", c.id, r.min.x.to_bits(), r.min.y.to_bits()),
            );

            Canvas::tip(&response, Some(tip));
        }
    }
}

/// Control `c`'s own picture at rest, where it has one that is not a
/// texture or the empty picture.
pub fn picture(canvas: &mut Canvas<'_>, c: &ShownControl) {
    let shown = c
        .art
        .filter(|&art| art != NO_ART && !(art.0 == TEXTURES && c.kind != Some(Kind::Button)));

    if let (Some((record, set)), Some(r)) = (shown, rect(c)) {
        canvas.picture(Pic::of(record, set), Tone::Plain, r.min);
    }
}

/// `text` in control `c`'s font and colour in its flags' state (a lit
/// tab's, a disabled control's), its hot key letter marked ([`marked`];
/// **Game**: `ui_warrior_stats.png`'s SPELLS, ITEMS, STATS), centred;
/// wrapped where one line is wider than the control ("Minimum Return"
/// in two).
pub fn label(canvas: &mut Canvas<'_>, c: &ShownControl, r: Rect, text: &str) {
    if let Some(record) = c.font {
        let state = State::of(c.flags);
        let text = marked(c, text.trim(), state);

        wrapped(canvas, record, r, &text, ink(c, state));
    }
}

/// `text` in `record` centred in `r`: one line, or the lines it wraps to
/// centred up and down.
pub fn wrapped(canvas: &mut Canvas<'_>, record: FourCc, r: Rect, text: &str, colour: Color32) {
    if one_line(text, canvas.width(record, text), r.width()) {
        canvas.text(record, r, text, (colour, Align::Centre));
    } else {
        let lines = canvas.lines(record, r.width(), text);
        #[allow(clippy::cast_precision_loss, reason = "a few lines")]
        let tall = canvas.pitch(record) * lines as f32;
        let block = Rect::from_center_size(r.center(), bevy_egui::egui::vec2(r.width(), tall));

        canvas.paragraphs(record, block, text, (colour, Align::Centre, 0.0));
    }
}

/// Whether `text`, `wide` pixels, is set on one line in `room`: it fits
/// and holds no break.
fn one_line(text: &str, wide: f32, room: f32) -> bool {
    wide <= room - 2.0 && !text.contains('\n')
}

/// `text` in control `id` of `layout`, one line, set by `align`, in its
/// font and colour, else `colour`.
pub fn fill(
    canvas: &mut Canvas<'_>,
    layout: &Layout,
    id: i32,
    text: &str,
    (colour, align): (Option<Color32>, Align),
) {
    if let Some(c) = layout.control(id)
        && let (Some(r), Some(record)) = (rect(c), c.font)
    {
        let colour = colour.unwrap_or_else(|| self::colour(c));

        canvas.text(record, r, text, (colour, align));

        if c.tooltip.is_some() {
            let response = canvas.hover(r, ("filled tip", id));

            Canvas::tip(&response, c.tooltip.as_deref());
        }
    }
}

/// Control `c` with what Shift does to it added to its tooltip.
#[must_use]
pub fn hinted(c: &ShownControl, step: Step, amount: i32) -> ShownControl {
    ShownControl {
        tooltip: Some(tip::shift_hint(c.tooltip.as_deref(), step, amount)),
        ..c.clone()
    }
}

/// Control `c` as a button: its picture in its state, `text` (else its
/// own label) centred with the hot key letter in its colour; whether it
/// was pressed this frame, by click or hot key.
pub fn button(canvas: &mut Canvas<'_>, c: &ShownControl, text: Option<&str>, able: Able) -> bool {
    stated(canvas, c, text, able).0
}

/// [`button`], and the state its text takes: lit while held.
pub fn stated(
    canvas: &mut Canvas<'_>,
    c: &ShownControl,
    text: Option<&str>,
    able: Able,
) -> (bool, State) {
    let (Some(r), Some(id)) = (rect(c), c.id) else {
        return (false, able.state(c.flags));
    };
    let response = canvas.press(r, ("button", id, r.min.x.to_bits()));
    let look = look(&response, Tick::Clear);
    let state = able.state(c.flags).pressed(look.2);

    if let Some((record, set)) = c.art.filter(|&a| a != NO_ART) {
        let pic = Pic::of(record, set);
        let (slot, tone) = self::slot(canvas, pic, look, able.state(c.flags));

        canvas
            .picture(pic.slot(slot), tone, r.min)
            .or_else(|| canvas.picture(pic, tone, r.min));
    }

    let text = text.or(c.text.as_deref()).map(str::trim);

    // The text takes the state's colours, lit while held (SPIN white).
    if let (Some(text), Some(record)) = (text, c.font) {
        canvas.text(
            record,
            r,
            &marked(c, text, state),
            (ink(c, state), Align::Centre),
        );
    }

    Canvas::tip(&response, c.tooltip.as_deref());

    let key = hot(canvas, c, able);

    (able == Able::Enabled && response.clicked() || key, state)
}

/// Whether `c`'s hot key pressed it. A control under a disabled ui (a
/// menu over it, the game ended) neither offers nor takes its key: the
/// menu's own controls do.
fn hot(canvas: &Canvas<'_>, c: &ShownControl, able: Able) -> bool {
    crate::input::hotkey::hit_in(canvas.ui, c.hotkey, able)
}

/// `text` as markup for control `c` in `state`: its colour, its hot
/// key's letter in the hot key colour (`font::hot`; [`ink`],
/// [`hot_ink`]).
#[must_use]
pub fn marked(c: &ShownControl, text: &str, state: State) -> String {
    hot_markup(text, c.hotkey, (ink(c, state), hot_ink(c, state)))
}

/// `text` as markup, its hot key's letter in `hot`, the rest `colour`.
#[must_use]
pub fn hot_markup(text: &str, key: Option<i32>, (colour, hot): (Color32, Color32)) -> String {
    font::hot(text, key, (colour, hot))
        .into_iter()
        .flat_map(|(c, s)| [markup(c), s])
        .collect()
}

/// A two-state control: a check or radio box draws slot 3 on while lit
/// (`menu::drawn_slot`; **Game**: the tax and repair checks, `INBb` 1000
/// to 1003, blue); a button, the palace's tabs, its panel's lit state.
/// Whether it was pressed, by click or hot key.
pub fn check(canvas: &mut Canvas<'_>, c: &ShownControl, lit: Lit, able: Able) -> bool {
    let (Some(r), Some(id)) = (rect(c), c.id) else {
        return false;
    };
    let response = canvas.press(r, ("check", id));

    if let Some((record, set)) = c.art {
        let pic = Pic::of(record, set);
        let look = look(&response, lit.tick(c));
        let (slot, tone) = self::slot(canvas, pic, look, able.state(c.flags));

        canvas.picture(pic.slot(slot), tone, r.min);
    }

    Canvas::tip(&response, c.tooltip.as_deref());

    let key = hot(canvas, c, able);

    able == Able::Enabled && response.clicked() || key
}

/// Which of a bar's fills: the hit point bar's poisoned one is green.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fill {
    Plain,
    Poisoned,
}

/// A bar's picture between a control's set and its fill's: the fill
/// is set + 4, a poisoned one + 14, the empty track (where the record
/// has one) + 3. **Inferred**: every bar record (`INBc`, `INBd`, `INDx`,
/// `INDy`, `INS2`, `IMZa`) holds only those sets, 1003, 1004 and 1014,
/// while the panels name 1000.
const BAR_FILL: i32 = 4;
const BAR_POISONED: i32 = 14;
const BAR_TRACK: i32 = 3;

/// Bar control `c` filled `filled` of 1 with its own pictures, the fill
/// as shapes ([`rows`](super::stripes::Stripes::rows)); else an egui bar
/// in `colour`.
pub fn art_bar(
    canvas: &mut Canvas<'_>,
    c: &ShownControl,
    (filled, fill): (f32, Fill),
    colour: Color32,
) {
    let Some(r) = rect(c) else { return };
    let shown = Rect::from_min_size(
        r.min,
        bevy_egui::egui::vec2(r.width() * filled.clamp(0.0, 1.0), r.height()),
    );
    let drawn = c.art.is_some_and(|(record, set)| {
        let step = match fill {
            Fill::Plain => BAR_FILL,
            Fill::Poisoned => BAR_POISONED,
        };

        let (track, bar) = (
            Pic::of(record, set + BAR_TRACK),
            Pic::of(record, set + step),
        );

        canvas.clipped(track, Tone::Plain, r.min, r);
        canvas
            .ink
            .bar(canvas.catalogue, bar, track)
            .map(|stripes| {
                let onto = Rect::from_min_size(r.min, stripes.size);

                canvas.stripes(&stripes, onto, shown.intersect(r));
            })
            .is_some()
    });

    if !drawn {
        bar(canvas, r, filled, colour);
    }

    if c.tooltip.is_some() {
        let response = canvas.hover(r, ("bar tip", c.id));

        Canvas::tip(&response, c.tooltip.as_deref());
    }
}

/// A bar's fill over panel rectangle `rect`, `filled` of 1, in `colour`
/// with a lighter top half: where a bar has no picture of its own.
pub fn bar(canvas: &Canvas<'_>, rect: Rect, filled: f32, colour: Color32) {
    let inner = rect.shrink2(bevy_egui::egui::vec2(2.0, 2.0));
    let width = inner.width() * filled.clamp(0.0, 1.0);
    let full = Rect::from_min_size(inner.min, bevy_egui::egui::vec2(width, inner.height()));
    let top = Rect::from_min_max(full.min, pos2(full.max.x, full.center().y));

    canvas.fill(inner, Color32::from_black_alpha(160));
    canvas.fill(full, colour);
    canvas.fill(top, Color32::from_white_alpha(40));
}

/// A colour as text markup: `\x01RRGGBB`.
fn markup(c: Color32) -> String {
    format!("\u{1}{:02X}{:02X}{:02X}", c.r(), c.g(), c.b())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A label the panel breaks ("Teleportation\nAmulets", `APa3` 5043)
    /// takes the lines path even where its widest line fits.
    #[test]
    fn a_broken_label_is_not_drawn_as_one_line() {
        assert!(one_line("Market Day", 80.0, 120.0));
        assert!(!one_line("Train Intelligence", 130.0, 120.0));
        assert!(!one_line("Teleportation\nAmulets", 90.0, 120.0));
    }

    /// Every `.rs` file under `dir`, with its text.
    fn sources(dir: &std::path::Path) -> Vec<(std::path::PathBuf, String)> {
        std::fs::read_dir(dir)
            .expect("src")
            .flatten()
            .map(|e| e.path())
            .flat_map(|p| match (p.is_dir(), p.extension()) {
                (true, _) => sources(&p),
                (false, Some(e)) if e == "rs" => {
                    let text = std::fs::read_to_string(&p).expect("readable");

                    vec![(p, text)]
                }
                _ => Vec::new(),
            })
            .collect()
    }

    /// A press reaches a control's slot only as the engine's: [`look`]'s
    /// `menu::Press`, which lights the state (`menu::State::pressed`).
    /// A map press and the scroll arrows (`scroll::PRESSED`, the scroll
    /// bar's own rule, `scroll::pass`) are not controls' slots. A file picking a slot by
    /// hover goes through `drawn_slot`, and takes the press with it.
    #[test]
    fn every_control_slot_is_the_engines() {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");

        for (path, text) in sources(&src) {
            let lines: Vec<&str> = text.lines().collect();

            for (i, line) in lines.iter().enumerate() {
                let press = line.contains("is_pointer_button_down_on")
                    && !line.contains("contains(\"is_pointer");
                let near = |what: &str| {
                    lines[i..(i + 4).min(lines.len())]
                        .iter()
                        .any(|l| l.contains(what))
                };
                let (arrow, shared) = (near("scroll::PRESSED"), near("Press::of("));
                let map = path.ends_with("minimap.rs");
                let bar = path.ends_with("ui/scroll.rs");

                assert!(
                    !press || arrow || shared || map || bar,
                    "{}:{}: {line}",
                    path.display(),
                    i + 1
                );
            }

            let hover_slot = text.contains(".hovered()") && text.contains(".slot(");

            assert!(
                !hover_slot || text.contains("drawn_slot") || text.contains("sheet::slot("),
                "{} picks a slot by hover without drawn_slot",
                path.display()
            );

            let bare = text.contains("Hover::of(response.hovered())")
                && text.contains(".slot(")
                && !path.ends_with("panel/sheet.rs");

            assert!(!bare, "{} takes no press (sheet::look)", path.display());
        }
    }

    /// The look of a control at `rect` through `events`, one frame each:
    /// the last frame's.
    fn looks(rect: Rect, events: &[bevy_egui::egui::Event]) -> (Tick, Hover, Press) {
        use bevy_egui::egui::{self, RawInput, Sense};

        let ctx = egui::Context::default();
        let mut last = (Tick::Clear, Hover::Away, Press::Free);

        for event in events {
            let input = RawInput {
                events: vec![event.clone()],
                ..RawInput::default()
            };

            let mut output = ctx.run_ui(input, |ui| {
                let response = ui.interact(rect, ui.id().with("held"), Sense::click());

                last = look(&response, Tick::Clear);
            });

            output.textures_delta.clear();
        }

        last
    }

    /// A button held down over it is held (`menu::State::pressed`), then
    /// hovered once let go; dragged off, neither.
    #[test]
    fn a_button_held_under_the_pointer_looks_held() {
        use bevy_egui::egui::{Event, Modifiers, PointerButton};

        let rect = Rect::from_min_size(pos2(10.0, 10.0), bevy_egui::egui::vec2(100.0, 20.0));
        let (on, off) = (pos2(50.0, 20.0), pos2(300.0, 300.0));
        let button = |pos, pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        let down = [
            Event::PointerMoved(on),
            button(on, true),
            Event::PointerMoved(on),
        ];
        let up = [&down[..], &[button(on, false), Event::PointerMoved(on)]].concat();
        let dragged = [&down[..], &[Event::PointerMoved(off)]].concat();

        assert_eq!(looks(rect, &down), (Tick::Clear, Hover::Over, Press::Held));
        assert_eq!(looks(rect, &up), (Tick::Clear, Hover::Over, Press::Free));
        assert_eq!(
            looks(rect, &dragged),
            (Tick::Clear, Hover::Away, Press::Free)
        );
    }

    #[test]
    fn markup_reads_back_as_its_colour() {
        let gold = Color32::from_rgb(0xff, 0xdd, 0x00);
        let spans = font::spans(&format!("a{}b", markup(gold)), WHITE);

        assert_eq!(spans[1], (gold, "b".to_owned()));
    }

    /// Every shape `AP21`'s statics draw, at `scale` in the column's page
    /// slot, with the slot itself, in window points.
    fn stats_page(scale: f32) -> (Vec<Rect>, Rect) {
        use bevy_egui::egui::{self, RawInput};
        let catalogue = crate::testdata::hd();
        let text = regent_core::data::text::Text::build(catalogue);
        let layout = regent_core::view::menu::layout(catalogue, &text, FourCc(*b"AP21"))
            .expect("AP21")
            .expect("parses");
        let fit = fit::Fit {
            scale,
            origin: pos2(0.0, 428.0 * scale),
        };
        let ctx = egui::Context::default();
        let mut ink = super::super::ink::Ink::default();
        let mut art = regent_draw::art::Art::default();

        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            let mut canvas = Canvas {
                ui,
                ink: &mut ink,
                art: &mut art,
                source: regent_draw::art::ArtSource::of(&[]),
                catalogue,
                fit,
                salt: "test",
            };

            statics(&mut canvas, &layout, None, &|_| false);
        });

        // No painter takes the font atlas or the pictures here.
        output.textures_delta.clear();

        let shapes = output
            .shapes
            .iter()
            .map(|s| s.shape.visual_bounding_rect())
            .filter(Rect::is_positive)
            .collect();

        (shapes, fit.rect(bounds(&layout)))
    }

    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn the_stats_page_stays_inside_its_slot() {
        for scale in [0.75, 1.0, 1.415, 2.0] {
            let (shapes, slot) = stats_page(scale);

            assert!(!shapes.is_empty(), "nothing drawn at {scale}");

            for r in shapes {
                assert!(
                    slot.expand(1.0).contains_rect(r),
                    "{r:?} outside {slot:?} at {scale}"
                );
            }
        }
    }
}
