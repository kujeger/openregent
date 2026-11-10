//! The typeface egui draws the game's text in: Inknut Antiqua (SIL Open
//! Font License 1.1, `assets/fonts/inknut-antiqua/`), embedded, or
//! egui's own (Ubuntu Light). `[ui] font` picks it; a change shows from
//! the next frame.
//!
//! ```text
//!  [ui] font ──follow──> ctx.set_fonts ──next pass──> fonts loaded
//!                                                      │
//!  metrics() <── which one is loaded ──────────────────┘
//!  proportional family:  Inknut latin ─> Inknut latin-ext ─> egui's fonts
//! ```
//!
//! Text sized to the original's fonts asks [`metrics`] for the loaded
//! face's proportions, so either fills the same room.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use bevy::prelude::*;
use bevy_egui::egui::{self, FontData, FontDefinitions, FontFamily};
use bevy_egui::{EguiContexts, EguiPrimaryContextPass};

use crate::files::config::{Settings, Typeface};

/// Inknut Antiqua, weight 400: Basic Latin, Latin-1 and a few more
/// (æ ø å é ß œ €).
const INKNUT_LATIN: &[u8] =
    include_bytes!("../../assets/fonts/inknut-antiqua/inknut-antiqua-latin-400-normal.ttf");

/// Inknut Antiqua, weight 400: the rest of Latin Extended (ł ő č).
const INKNUT_LATIN_EXT: &[u8] =
    include_bytes!("../../assets/fonts/inknut-antiqua/inknut-antiqua-latin-ext-400-normal.ttf");

/// What `--licences` prints: the font, where it came from, and its
/// licence with its copyright, as the OFL asks of each copy.
pub const LICENCES: &str = concat!(
    "Inknut Antiqua (inknut-antiqua-latin-400, -latin-ext-400), built in.\n",
    "Copyright (c) 2014 Claus Eggers Sørensen. SIL Open Font License 1.1:\n\n",
    include_str!("../../assets/fonts/inknut-antiqua/LICENSE"),
);

/// The fonts' names in egui's definitions.
const LATIN: &str = "inknut-latin";
const LATIN_EXT: &str = "inknut-latin-ext";

/// A face's proportions, per point of size. Checked against egui by
/// `the_metrics_hold`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Metrics {
    /// `menu::font::SAMPLE`'s width.
    pub sample: f32,
    /// The height a line needs: egui's row for egui's font. Inknut's
    /// row (2.58) is mostly empty, so its glyphs' span, `d`'s top to
    /// `p`'s bottom, given egui's share of room round its own span.
    pub row: f32,
    /// `H`'s height.
    pub cap: f32,
    /// A galley's top to its baseline.
    pub ascent: f32,
    /// `8`'s height: Inknut's figures are old style, so not `0`'s.
    pub digit: f32,
}

/// egui's Ubuntu Light.
pub const EGUI: Metrics = Metrics {
    sample: 19.76,
    row: 1.149,
    cap: 0.693,
    ascent: 0.932,
    digit: 0.73,
};

/// Inknut Antiqua: about 1.2 times as wide as egui's for its size, its
/// glyphs 1.18 times as tall.
pub const INKNUT: Metrics = Metrics {
    sample: 24.6,
    row: 1.35,
    cap: 0.80,
    ascent: 1.703,
    digit: 0.90,
};

/// Whether Inknut is loaded: what [`metrics`] answers by.
static INKNUT_LOADED: AtomicBool = AtomicBool::new(false);

#[cfg(test)]
thread_local! {
    /// A test's face ([`in_face`]): tests share [`INKNUT_LOADED`].
    static TEST_FACE: std::cell::Cell<Option<Typeface>> = const { std::cell::Cell::new(None) };
}

/// The loaded face's proportions.
#[must_use]
pub fn metrics() -> Metrics {
    #[cfg(test)]
    let inknut = TEST_FACE
        .get()
        .map_or(INKNUT_LOADED.load(Ordering::Relaxed), |f| {
            f == Typeface::Inknut
        });
    #[cfg(not(test))]
    let inknut = INKNUT_LOADED.load(Ordering::Relaxed);

    if inknut { INKNUT } else { EGUI }
}

/// `run` with [`metrics`] `face`'s on this thread.
#[cfg(test)]
pub fn in_face<T>(face: Typeface, run: impl FnOnce() -> T) -> T {
    TEST_FACE.set(Some(face));

    let out = run();

    TEST_FACE.set(None);
    out
}

/// `size` in egui's font as the loaded face draws as tall: 15 points
/// stays 15 in egui's, 12.8 in Inknut.
#[must_use]
pub fn like_egui(size: f32) -> f32 {
    size * EGUI.row / metrics().row
}

/// How far up a galley at [`like_egui`]`(size)` is drawn for its
/// baseline to stand where egui's font at `size` puts it, points: 0 in
/// egui's font.
#[must_use]
pub fn lift(size: f32) -> f32 {
    metrics().ascent * like_egui(size) - EGUI.ascent * size
}

/// A text box's margin above and below its row, points: egui's
/// `TextEdit` default.
const EDIT_MARGIN: f32 = 2.0;

/// Where to put a one-line text box in `font` to show it in `rect`:
/// `rect` grown up and down about its centre to the box's row where that
/// is taller, for the box to centre its row there; clipped to `rect` by
/// the caller. Inknut's row, 2.58 em, outgrows a box egui's fits, and a
/// box too short sets its text low.
#[must_use]
pub fn edit_room(ctx: &egui::Context, font: &egui::FontId, rect: egui::Rect) -> egui::Rect {
    let row = ctx.fonts_mut(|f| f.row_height(font)) + 2.0 * EDIT_MARGIN;

    rect.expand2(egui::vec2(0.0, ((row - rect.height()) / 2.0).max(0.0)))
}

/// egui's definitions for `face`: Inknut first in the proportional
/// family, egui's own after it for what Inknut lacks.
#[must_use]
pub fn definitions(face: Typeface) -> FontDefinitions {
    let mut fonts = FontDefinitions::default();

    if face == Typeface::Inknut {
        for (name, bytes) in [(LATIN, INKNUT_LATIN), (LATIN_EXT, INKNUT_LATIN_EXT)] {
            fonts
                .font_data
                .insert(name.to_owned(), Arc::new(FontData::from_static(bytes)));
        }

        fonts
            .families
            .entry(FontFamily::Proportional)
            .or_default()
            .splice(0..0, [LATIN.to_owned(), LATIN_EXT.to_owned()]);
    }

    fonts
}

/// The face `fonts` load.
fn loaded(fonts: &FontDefinitions) -> Typeface {
    if fonts.font_data.contains_key(LATIN) {
        Typeface::Inknut
    } else {
        Typeface::Egui
    }
}

/// The face the settings ask for loaded, and [`metrics`] set by the one
/// loaded now.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
fn follow(mut contexts: EguiContexts, settings: Res<Settings>) -> Result {
    let ctx = contexts.ctx_mut()?;
    let now = ctx.fonts(|f| loaded(f.definitions()));
    let wanted = settings.ui.font;

    // Tests share the process: one test's app must not move the sizes
    // another measures, so there they stay egui's.
    if cfg!(not(test)) {
        INKNUT_LOADED.store(now == Typeface::Inknut, Ordering::Relaxed);
    }

    if now != wanted {
        ctx.set_fonts(definitions(wanted));
    }

    Ok(())
}

/// [`follow`] each pass: a pass draws in the fonts loaded as it began,
/// so where it runs in the pass does not matter.
pub struct TypefacePlugin;

impl Plugin for TypefacePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(EguiPrimaryContextPass, follow);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::menu::font::SAMPLE;
    use bevy_egui::egui::{Color32, FontId};

    /// `run` given a context with `fonts` loaded.
    fn with_fonts<T>(fonts: FontDefinitions, run: impl FnOnce(&egui::Context) -> T) -> T {
        let ctx = egui::Context::default();
        let mut out = None;
        let mut run = Some(run);

        ctx.set_fonts(fonts);

        let mut done = ctx.run_ui(egui::RawInput::default(), |ui| {
            if let Some(run) = run.take() {
                out = Some(run(ui.ctx()));
            }
        });

        // No renderer takes the font atlas.
        done.textures_delta.clear();

        out.expect("ran once")
    }

    /// `text`'s first glyph's drawn box in `f` at `id`.
    fn glyph(f: &mut egui::epaint::text::FontsView<'_>, id: &FontId, text: &str) -> egui::Rect {
        let galley = f.layout_no_wrap(text.to_owned(), id.clone(), Color32::WHITE);
        let glyph = galley.rows[0].row.glyphs[0];

        egui::Rect::from_min_size(glyph.uv_rect.offset.to_pos2(), glyph.uv_rect.size)
    }

    /// Each face's [`Metrics`] as egui lays it out, within 2%: a newer
    /// egui or another font file fails here first.
    #[test]
    fn the_metrics_hold() {
        const SIZE: f32 = 100.0;

        let measured = |face| {
            with_fonts(definitions(face), |ctx| {
                let id = FontId::proportional(SIZE);

                ctx.fonts_mut(|f| {
                    let sample = f.layout_no_wrap(SAMPLE.to_owned(), id.clone(), Color32::WHITE);
                    let ascent = ascent_of(f, &id, "H");
                    let span = glyph(f, &id, "p").max.y - glyph(f, &id, "d").min.y;

                    (
                        sample.size().x,
                        f.row_height(&id),
                        span,
                        glyph(f, &id, "H").height(),
                        ascent,
                        glyph(f, &id, "8").height(),
                    )
                })
            })
        };
        let near = |got: f32, want: f32| (got / SIZE - want).abs() < 0.02 * want;

        let (sample, row, egui_span, cap, ascent, digit) = measured(Typeface::Egui);
        let all = format!("{sample} {row} {egui_span} {cap} {ascent} {digit}");

        assert!(near(sample, EGUI.sample), "{all}");
        assert!(near(row, EGUI.row), "{all}");
        assert!(near(cap, EGUI.cap), "{all}");
        assert!(near(ascent, EGUI.ascent), "{all}");
        assert!(near(digit, EGUI.digit), "{all}");

        let (sample, _, span, cap, ascent, digit) = measured(Typeface::Inknut);
        let all = format!("{sample} {span} {cap} {ascent} {digit}");

        assert!(near(sample, INKNUT.sample), "{all}");
        assert!(near(span * row / egui_span, INKNUT.row), "{all}");
        assert!(near(cap, INKNUT.cap), "{all}");
        assert!(near(ascent, INKNUT.ascent), "{all}");
        assert!(near(digit, INKNUT.digit), "{all}");
    }

    /// The ascent of the font `text`'s first glyph comes from.
    fn ascent_of(f: &mut egui::epaint::text::FontsView<'_>, id: &FontId, text: &str) -> f32 {
        let galley = f.layout_no_wrap(text.to_owned(), id.clone(), Color32::WHITE);

        galley.rows[0].row.glyphs[0].font_ascent
    }

    /// `--licences` carries the copyright and the whole licence.
    #[test]
    fn the_licence_ships_with_the_font() {
        assert!(LICENCES.contains("Claus Eggers"));
        assert!(LICENCES.contains("SIL OPEN FONT LICENSE Version 1.1"));
        assert!(
            LICENCES
                .trim_end()
                .ends_with("OTHER DEALINGS IN THE FONT SOFTWARE.")
        );
    }

    #[test]
    fn the_definitions_load_the_face_asked_for() {
        assert_eq!(loaded(&definitions(Typeface::Inknut)), Typeface::Inknut);
        assert_eq!(loaded(&definitions(Typeface::Egui)), Typeface::Egui);
        assert_eq!(definitions(Typeface::Egui), FontDefinitions::default());
    }

    /// A chat line's letters all come from Inknut, the latin file or the
    /// latin-ext one; a symbol neither has from egui's own fonts. A
    /// glyph's face is told by its face's ascent.
    #[test]
    fn inknut_draws_nordic_and_central_european_letters() {
        const LINE: &str = "æøå é ß ł ő €";
        const SIZE: f32 = 20.0;

        let faces = with_fonts(definitions(Typeface::Inknut), |ctx| {
            ctx.fonts_mut(|f| {
                let id = FontId::proportional(SIZE);
                let galley = f.layout_no_wrap(format!("{LINE}★"), id, Color32::WHITE);

                galley.rows[0]
                    .row
                    .glyphs
                    .iter()
                    .filter(|g| !g.chr.is_whitespace())
                    .map(|g| (g.chr, g.font_face_ascent))
                    .collect::<Vec<_>>()
            })
        });
        let inknut = |a: f32| (a - INKNUT.ascent * SIZE).abs() < 0.5;
        let (star, letters) = faces.split_last().expect("glyphs");

        assert!(letters.iter().all(|(_, a)| inknut(*a)), "{faces:?}");
        assert!(!inknut(star.1), "{faces:?}");
    }
}
