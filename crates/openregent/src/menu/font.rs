//! Text drawn by egui at the window's own pixel size, in its
//! proportional font (`typeface`: Inknut Antiqua or egui's own). The original's
//! `IMAG` fonts give only a size and a line pitch, so a text takes about
//! the room it took.
//!
//! ```text
//!  IMAG font ── SAMPLE's width, line pitch, M's dark edge ──> Font, layout pixels
//!  drawn ──> size × fit.scale points, in half points
//!  measured ──> egui's width at those points ÷ fit.scale: what is drawn
//! ```
//!
//! A size is as wide as the original over [`SAMPLE`], no taller than its
//! line pitch: the bitmap fonts are wide for their height, so `fnt4` and
//! `fn10` keep their pitch and run narrower.

use bevy_egui::egui::emath::GuiRounding;
use bevy_egui::egui::text::LayoutJob;
use bevy_egui::egui::{self, Color32, FontId, Painter, Pos2, TextFormat, vec2};
use regent_core::content::imagery::{self, FrameSource};
use regent_core::data::catalogue::{Catalogue, Located};
use regent_core::data::fourcc::FourCc;
use regent_core::data::imag::Blit;
use regent_core::data::imag::Image;
use regent_core::view::{Ink, Sprite, menu};
use std::borrow::Cow;

use super::fit::Fit;
use crate::ui::typeface::{self, Metrics};

/// `fn11`'s line pitch in a text box, pixels. **Game**: the Barren Waste
/// briefing's background lines stand 47 apart (`menu_barren_waste_start.png`);
/// its slot 0 gives no line height, as `fnt4`'s does.
const LARGE_PITCH: i32 = 47;

/// The character whose glyph shows a font's edge ([`edge`]).
const CAPITAL: u8 = b'M';

/// The line both fonts are measured over.
pub const SAMPLE: &str = "The quick brown fox jumps over the lazy dog";

/// A size is drawn in steps of this many points: few sizes for egui's
/// glyph cache while a window is resized.
const STEPS_PER_POINT: f32 = 2.0;

/// The colour word's flag for "this colour" (`SMNU` key 36, markup):
/// `0x80rrggbb`. Others, `0x40000000`, set none.
const COLOUR_SET: u32 = 0x8000_0000;

/// A hot key letter's colour where its control sets none (`SMNU` key
/// 49). **Game**: `ui_full.png`'s STATISTICS, BUILDINGS, ROSTER and
/// REWARDS, whose panel sets none, draw theirs `#4830e0`; `menu_main.png`'s
/// `0x0000ff` draws `#0000e0`, the glyph's top at 0xe0 of 0xff, so this
/// at full. **Inferred**: the engine's default.
pub const HOT_KEY: Color32 = rgb(0x0052_37ff);

/// A text's colour from a control's colour word; `None` where it sets
/// none.
#[must_use]
pub fn colour(word: i32) -> Option<Color32> {
    let word = word.cast_unsigned();

    (word & COLOUR_SET != 0).then(|| rgb(word))
}

/// Control `c`'s colour for `role` in `state`, its own or its panel
/// palette's (`menu::colour`); `None` for a control made in code.
#[must_use]
pub fn ink(c: &menu::ShownControl, role: menu::Role, state: menu::State) -> Option<Color32> {
    c.inks
        .colour(role, state)
        .map(|[r, g, b]| Color32::from_rgb(r, g, b))
}

/// `0xrrggbb` as a colour.
#[must_use]
pub const fn rgb(word: u32) -> Color32 {
    let [_, r, g, b] = word.to_be_bytes();

    Color32::from_rgb(r, g, b)
}

/// An original font as egui draws it, in layout pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Font {
    /// [`SAMPLE`]'s width in the original, pixels.
    pub sample: f32,
    /// A text box's line pitch: the original's.
    pub pitch: f32,
    /// The dark edge round each glyph, its share of the text's colour;
    /// `None` without one ([`edge`]).
    pub edge: Option<f32>,
}

impl Font {
    /// The `IMAG` font `record` from `catalogue`; `None` where it is
    /// missing or will not parse.
    pub fn load(catalogue: &Catalogue<'_>, record: FourCc) -> Option<Font> {
        let at = catalogue.find(b"IMAG", record)?;
        let image = Image::parse(catalogue.record(b"IMAG", at)?.data).ok()?;
        let measures = regent_core::view::menu::font::Font::of_image(&image);
        let sample = measures.width(SAMPLE);
        let pitch = match measures.line() {
            Some(line) if line > 0 => line,
            _ => LARGE_PITCH,
        };

        #[allow(clippy::cast_precision_loss, reason = "a line's pixels")]
        let font = Font::of(sample as f32, pitch as f32);

        Some(Font {
            edge: edge(catalogue, at),
            ..font
        })
    }

    /// The font whose [`SAMPLE`] is `sample` pixels wide and whose lines
    /// stand `pitch` apart.
    #[must_use]
    pub const fn of(sample: f32, pitch: f32) -> Font {
        Font {
            sample,
            pitch,
            edge: None,
        }
    }

    /// The font `by` times as large, its lines as much closer.
    #[must_use]
    pub fn times(self, by: f32) -> Font {
        Font {
            sample: self.sample * by,
            pitch: self.pitch * by,
            ..self
        }
    }

    /// egui's size at scale 1 in the loaded face ([`typeface::metrics`]).
    #[must_use]
    pub fn size(self) -> f32 {
        self.size_in(typeface::metrics())
    }

    /// egui's size at scale 1 in a face of `metrics`: as wide as the
    /// original, unless a row would then be taller than the pitch.
    #[must_use]
    pub fn size_in(self, metrics: Metrics) -> f32 {
        (self.sample / metrics.sample).min(self.pitch / metrics.row)
    }

    /// The size drawn on a page at `scale`, points: in steps of
    /// 1 / [`STEPS_PER_POINT`].
    #[must_use]
    pub fn points(self, scale: f32) -> f32 {
        (self.size() * scale * STEPS_PER_POINT).round().max(1.0) / STEPS_PER_POINT
    }

    fn id(self, scale: f32) -> FontId {
        FontId::proportional(self.points(scale))
    }

    /// The capital height at `scale`, layout pixels.
    #[must_use]
    pub fn cap(self, scale: f32) -> f32 {
        typeface::metrics().cap * self.points(scale) / scale
    }

    /// `text`'s width as drawn at `scale`, layout pixels: its colour
    /// markup, not drawn, left out.
    #[must_use]
    pub fn width(self, ctx: &egui::Context, scale: f32, text: &str) -> f32 {
        let id = self.id(scale);
        let drawn: String = regent_core::data::text::spans(text)
            .map(|s| s.text)
            .collect();
        let wide = ctx.fonts_mut(|f| f.layout_no_wrap(drawn, id, Color32::WHITE).size().x);

        wide / scale
    }

    /// `text` broken into lines at most `width` wide as drawn at `scale`.
    #[must_use]
    pub fn wrap(self, ctx: &egui::Context, scale: f32, text: &str, width: f32) -> Vec<String> {
        wrap(text, width, &|line| self.width(ctx, scale, line))
    }

    /// `text` broken into lines as [`Font::wrap`] does, each line with its
    /// coloured runs: the markup read over the whole text first
    /// ([`painted`]), so a colour runs on across lines; `colour` where
    /// none is set.
    #[must_use]
    pub fn wrapped(
        self,
        ctx: &egui::Context,
        scale: f32,
        (text, colour): (&str, Color32),
        width: f32,
    ) -> Vec<(String, Vec<(Color32, String)>)> {
        painted(text, colour)
            .into_iter()
            .flat_map(|source| {
                let plain: String = source.iter().map(|(ch, _)| ch).collect();
                let mut cursor = 0;

                self.wrap(ctx, scale, &plain, width)
                    .into_iter()
                    .map(|line| {
                        let parts = recoloured(&line, &source, &mut cursor);

                        (line, parts)
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    /// `parts` on one line, its baseline starting at layout point `pen`;
    /// under it the dark edge, a layout pixel out each way, where the
    /// font has one.
    pub fn draw(self, painter: &Painter, fit: Fit, pen: Pos2, parts: &[(Color32, String)]) {
        let id = self.id(fit.scale);
        let job = |tone: &dyn Fn(Color32) -> Color32| {
            let mut job = LayoutJob::default();

            for (colour, s) in parts {
                job.append(s, 0.0, TextFormat::simple(id.clone(), tone(*colour)));
            }

            painter.layout_job(job)
        };
        let ppp = painter.pixels_per_point();
        let top = fit.point(pen) - vec2(0.0, typeface::metrics().ascent * id.size);
        let at = top.round_to_pixels(ppp);

        if let Some(share) = self.edge {
            let edge = job(&|c| darker(c, share));
            let out = fit.scale.round_to_pixels(ppp).max(1.0 / ppp);

            for step in [
                vec2(-out, -out),
                vec2(out, -out),
                vec2(-out, out),
                vec2(out, out),
            ] {
                painter.galley(at + step, edge.clone(), Color32::BLACK);
            }
        }

        painter.galley(at, job(&|c| c), Color32::WHITE);
    }
}

/// `colour` at `share` of its brightness, its alpha kept.
fn darker(colour: Color32, share: f32) -> Color32 {
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a channel"
    )]
    let dim = |v: u8| (f32::from(v) * share).round() as u8;

    Color32::from_rgba_unmultiplied(
        dim(colour.r()),
        dim(colour.g()),
        dim(colour.b()),
        colour.a(),
    )
}

/// The dark edge of font `at`'s [`CAPITAL`]: the mean brightness of its
/// pixels darker than half its brightest, as a share of the brightest;
/// `None` where it has none.
///
/// **Inferred**: the engine tints a glyph's ramp by the text's colour, so
/// `fn11`, `fn10` and `fnt2`, ringed by dark ramp pixels, draw ringed by
/// a darker shade of it; `fnt4` and `fnt7` have none.
fn edge(catalogue: &Catalogue<'_>, at: Located) -> Option<f32> {
    let sets = imagery::sets(catalogue, at).ok()?;
    let frame = sets
        .first()?
        .poses
        .get(usize::from(CAPITAL))?
        .frames
        .first()?;
    let sprite = match frame {
        FrameSource::Tile { archive, index, .. } => Sprite::at(catalogue, *archive, *index)?.ok(),
        _ => None,
    }?;
    let lights: Vec<f32> = (0..sprite.covered.len())
        .filter_map(|i| match sprite.ink(i, Blit::Interface) {
            Ink::Colour(c) => Some(f32::from(c.r.max(c.g).max(c.b))),
            _ => None,
        })
        .collect();
    let brightest = lights.iter().copied().fold(0.0, f32::max);
    let dark: Vec<f32> = lights
        .into_iter()
        .filter(|&l| l < brightest / 2.0)
        .collect();

    #[allow(clippy::cast_precision_loss, reason = "a glyph's pixels")]
    (!dark.is_empty() && brightest > 0.0)
        .then(|| dark.iter().sum::<f32>() / dark.len() as f32 / brightest)
}

/// `text` broken into lines at most `width` wide by `measure`, a line's
/// width: CR, LF and CR LF breaks kept, words split at spaces, a word
/// wider than `width` alone.
#[must_use]
pub fn wrap(text: &str, width: f32, measure: &dyn Fn(&str) -> f32) -> Vec<String> {
    line_breaks(text)
        .split('\n')
        .flat_map(|paragraph| {
            let mut lines: Vec<String> = Vec::new();
            let indent = paragraph.len() - paragraph.trim_start().len();
            let mut line = paragraph[..indent].to_owned();

            for word in paragraph.split_whitespace() {
                let longer = format!("{line} {word}");

                if line.trim().is_empty() {
                    line.push_str(word);
                } else if measure(&longer) <= width {
                    line = longer;
                } else {
                    lines.push(std::mem::replace(&mut line, word.to_owned()));
                }
            }

            lines.push(line);
            lines
        })
        .collect()
}

/// The game's bare CR and CR LF breaks as LF.
fn line_breaks(text: &str) -> Cow<'_, str> {
    if text.contains('\r') {
        Cow::Owned(text.replace("\r\n", "\n").replace('\r', "\n"))
    } else {
        Cow::Borrowed(text)
    }
}

/// What ends a line cut short.
pub const ELLIPSIS: char = '…';

/// Coloured line `parts` ending in [`ELLIPSIS`], characters dropped from
/// its end until it is at most `width` wide by `measure`: "Purchasing
/// healing" to "Purchasing heal…".
#[must_use]
pub fn ellipsised(
    parts: &[(Color32, String)],
    width: f32,
    measure: &dyn Fn(&str) -> f32,
) -> Vec<(Color32, String)> {
    let mut chars: Vec<(Color32, char)> = parts
        .iter()
        .flat_map(|(c, s)| s.chars().map(move |ch| (*c, ch)))
        .collect();
    let plain = |chars: &[(Color32, char)]| -> String {
        chars.iter().map(|(_, ch)| *ch).chain([ELLIPSIS]).collect()
    };

    while !chars.is_empty() && measure(&plain(&chars)) > width {
        chars.pop();
    }

    // No space before the mark: "heal …" reads as a word left out.
    while chars.last().is_some_and(|(_, ch)| ch.is_whitespace()) {
        chars.pop();
    }

    // The mark in the colour of what it follows; then runs by colour again.
    let last = chars
        .last()
        .map(|(c, _)| *c)
        .or_else(|| parts.first().map(|(c, _)| *c))
        .unwrap_or(Color32::WHITE);

    chars.into_iter().chain([(last, ELLIPSIS)]).fold(
        Vec::new(),
        |mut runs: Vec<(Color32, String)>, (c, ch)| {
            match runs.last_mut() {
                Some((run, s)) if *run == c => s.push(ch),
                _ => runs.push((c, ch.to_string())),
            }
            runs
        },
    )
}

/// `text` with its hot key's letter (`menu::hotkey_letter`: the first
/// letter or digit that types `key`, case ignored) in `hot`, the rest in
/// `colour`. A key no letter types (Escape, Enter, `?`) marks none, as
/// the original's EXIT GAME shows (`menu_main.png`).
#[must_use]
pub fn hot(
    text: &str,
    key: Option<i32>,
    (colour, hot): (Color32, Color32),
) -> Vec<(Color32, String)> {
    let at = key
        .and_then(|k| menu::hotkey_letter(text, k))
        .filter(|&at| text.is_char_boundary(at + 1));

    match at {
        Some(at) => [
            (colour, &text[..at]),
            (hot, &text[at..=at]),
            (colour, &text[at + 1..]),
        ]
        .into_iter()
        .filter(|(_, s)| !s.is_empty())
        .map(|(c, s)| (c, s.to_owned()))
        .collect(),
        None => vec![(colour, text.to_owned())],
    }
}

/// `line`, a line wrapped from `source`'s characters, given back their
/// colours: `cursor` walks `source`, whitespace there matching a space in
/// `line`.
#[must_use]
pub fn recoloured(
    line: &str,
    source: &[(char, Color32)],
    cursor: &mut usize,
) -> Vec<(Color32, String)> {
    let mut parts: Vec<(Color32, String)> = Vec::new();
    let mut last = source.get(*cursor).map_or(Color32::WHITE, |(_, c)| *c);

    for ch in line.chars() {
        while source.get(*cursor).is_some_and(|(s, _)| s.is_whitespace()) {
            *cursor += 1;
        }

        let colour = if ch.is_whitespace() {
            last
        } else {
            let colour = source.get(*cursor).map_or(last, |(_, c)| *c);

            *cursor += 1;
            colour
        };

        match parts.last_mut() {
            Some((c, s)) if *c == colour => s.push(ch),
            _ => parts.push((colour, ch.to_string())),
        }

        last = colour;
    }

    parts
}

/// `text`'s paragraphs, each character in the colour its markup set; the
/// markup, read over the whole text, dropped, so a colour runs on into
/// the next paragraph and a tag's end anywhere after. Breaks may be CR
/// (the sign's `AITX`), LF or CR LF (`text::unescaped`).
#[must_use]
pub fn painted(text: &str, start: Color32) -> Vec<Vec<(char, Color32)>> {
    let normalized = line_breaks(text);
    let cells: Vec<(char, Color32)> = spans(&normalized, start)
        .into_iter()
        .flat_map(|(c, s)| s.chars().map(move |ch| (ch, c)).collect::<Vec<_>>())
        .collect();

    cells
        .split(|(ch, _)| *ch == '\n')
        .map(<[(char, Color32)]>::to_vec)
        .collect()
}

/// `text` cut where its markup sets the colour
/// ([`regent_core::data::text::spans`]: GMTX markers, XML colour tags); the
/// text before any takes `start`. Empty runs dropped.
#[must_use]
pub fn spans(text: &str, start: Color32) -> Vec<(Color32, String)> {
    regent_core::data::text::spans(text)
        .filter(|s| !s.text.is_empty())
        .map(|s| {
            let colour = s
                .colour
                .map_or(start, |[r, g, b]| Color32::from_rgb(r, g, b));

            (colour, s.text.to_owned())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One character one pixel, a space too.
    #[allow(clippy::cast_precision_loss, reason = "short test lines")]
    fn chars(s: &str) -> f32 {
        s.chars().count() as f32
    }

    /// `run` given a context with egui's fonts loaded.
    fn with_fonts<T>(run: impl FnOnce(&egui::Context) -> T) -> T {
        let ctx = egui::Context::default();
        let mut out = None;
        let mut run = Some(run);

        let mut done = ctx.run_ui(egui::RawInput::default(), |ui| {
            if let Some(run) = run.take() {
                out = Some(run(ui.ctx()));
            }
        });

        // No renderer takes the font atlas.
        done.textures_delta.clear();

        out.expect("ran once")
    }

    #[test]
    fn a_line_breaks_at_the_last_space_that_fits() {
        let lines = wrap("aa bb cc\n\ndd", 5.0, &chars);

        assert_eq!(lines, ["aa bb", "cc", "", "dd"]);
    }

    #[test]
    fn a_signs_carriage_returns_break_rows() {
        let board = "O | O | X\r--------------\rX | X | O\r--------------\r   | O | X";
        let rows = [
            "O | O | X",
            "--------------",
            "X | X | O",
            "--------------",
            "   | O | X",
        ];

        assert_eq!(wrap(board, 20.0, &chars), rows);
        let painted: Vec<String> = painted(board, Color32::WHITE)
            .into_iter()
            .map(|row| row.into_iter().map(|(ch, _)| ch).collect())
            .collect();
        assert_eq!(painted, rows);
        assert_eq!(wrap("first\r\nsecond", 20.0, &chars), ["first", "second"]);
    }

    #[test]
    fn a_word_wider_than_the_box_stands_alone() {
        assert_eq!(wrap("a bbbbbb c", 3.0, &chars), ["a", "bbbbbb", "c"]);
    }

    #[test]
    fn markup_sets_each_spans_colour() {
        let pale = Color32::from_rgb(0xff, 0xdd, 0xaa);
        let lilac = Color32::from_rgb(0xe6, 0xe1, 0xfd);
        let text = "\u{1}FFDDAADifficulty: \u{1}E6E1FDExpert";

        assert_eq!(
            spans(text, Color32::WHITE),
            [
                (pale, "Difficulty: ".to_owned()),
                (lilac, "Expert".to_owned())
            ]
        );
        assert_eq!(spans("plain", pale), [(pale, "plain".to_owned())]);
    }

    /// The key's first letter, case ignored: ROSTER's O; none for
    /// Escape (27) or `?` (63).
    #[test]
    fn the_hot_key_letter_is_the_first_that_types_it() {
        let (navy, blue) = (Color32::from_rgb(0, 0, 0x3f), HOT_KEY);
        let key = |c: u8| Some(i32::from(c));

        assert_eq!(
            hot("ROSTER", key(b'O'), (navy, blue)),
            [
                (navy, "R".to_owned()),
                (blue, "O".to_owned()),
                (navy, "STER".to_owned())
            ]
        );
        assert_eq!(
            hot("RECRUIT Wizard", key(b'r'), (navy, blue)),
            [(blue, "R".to_owned()), (navy, "ECRUIT Wizard".to_owned())]
        );
        assert_eq!(
            hot("EXIT GAME", Some(27), (navy, blue)),
            [(navy, "EXIT GAME".to_owned())]
        );
        assert_eq!(
            hot("HELP", key(b'?'), (navy, blue)),
            [(navy, "HELP".to_owned())]
        );
        assert_eq!(hot("YES", None, (navy, blue)), [(navy, "YES".to_owned())]);
    }

    /// QUIT      MAJESTY wrapped to two lines keeps its blue Q.
    #[test]
    fn a_wrapped_line_keeps_its_letters_colours() {
        let (navy, blue) = (Color32::from_rgb(0, 0, 0x3f), HOT_KEY);
        let source: Vec<(char, Color32)> = hot("QUIT      MAJESTY", Some(81), (navy, blue))
            .into_iter()
            .flat_map(|(c, s)| s.chars().map(move |ch| (ch, c)).collect::<Vec<_>>())
            .collect();
        let mut cursor = 0;
        let first = recoloured("QUIT", &source, &mut cursor);
        let second = recoloured("MAJESTY", &source, &mut cursor);

        assert_eq!(first, [(blue, "Q".to_owned()), (navy, "UIT".to_owned())]);
        assert_eq!(second, [(navy, "MAJESTY".to_owned())]);
    }

    #[test]
    fn a_colour_word_sets_a_colour_only_with_its_flag() {
        assert_eq!(
            colour(0x80ff_ddaa_u32.cast_signed()),
            Some(Color32::from_rgb(0xff, 0xdd, 0xaa))
        );
        assert_eq!(colour(0x4000_0000), None);
    }

    /// `fn11` (sample 608, pitch 47) is as wide as the original; `fnt4`
    /// (316, 15) as tall as its pitch allows, so narrower.
    #[test]
    fn a_size_matches_the_width_unless_the_pitch_is_tighter() {
        let large = Font::of(608.0, 47.0);
        let small = Font::of(316.0, 15.0);

        for m in [typeface::EGUI, typeface::INKNUT] {
            let (large, small) = (large.size_in(m), small.size_in(m));

            assert!((large * m.sample - 608.0).abs() < 0.01, "{m:?}");
            assert!(large * m.row < 47.0, "{m:?}");
            assert!((small * m.row - 15.0).abs() < 0.01, "{m:?}");
            assert!(small * m.sample < 316.0, "{m:?}");
        }
    }

    #[test]
    #[allow(clippy::float_cmp, reason = "half points, exact in f32")]
    fn the_size_scales_with_the_page_in_half_points() {
        let font = Font::of(10.0 * typeface::EGUI.sample, 15.0);

        assert_eq!(font.points(1.0), 10.0);
        assert_eq!(font.points(1.415), 14.0);
        assert_eq!(font.points(1.43), 14.5);
        assert_eq!(font.points(2.0), 20.0);
        assert_eq!(font.points(0.0), 0.5);
    }

    /// The briefing's `fn11` at 0.88: 30.8 drawn at 27.1, its lines 41.4
    /// apart, its edge kept.
    #[test]
    fn a_font_times_a_share_shrinks_its_size_and_pitch() {
        let large = Font {
            edge: Some(0.5),
            ..Font::of(608.0, 47.0)
        };
        let small = large.times(0.88);

        assert!((large.size_in(typeface::EGUI) - 30.8).abs() < 0.05);
        assert!((small.size_in(typeface::EGUI) - 27.1).abs() < 0.05);
        assert!((small.pitch - 41.36).abs() < 0.01);
        assert_eq!(small.edge, large.edge);
    }

    /// Colour markup is not drawn, so it takes no width.
    #[test]
    #[allow(clippy::float_cmp, reason = "the same glyphs laid out")]
    fn markup_takes_no_width() {
        let font = Font::of(316.0, 15.0);
        let (plain, marked) = with_fonts(|ctx| {
            (
                font.width(ctx, 1.0, "Mods: A, B"),
                font.width(ctx, 1.0, "Mods: \u{1}FF0000A\u{1}FFFFFF, B"),
            )
        });

        assert_eq!(marked, plain);
    }

    /// A line cut short ends in the mark, within its width, a trailing
    /// space dropped and each run's colour kept.
    #[test]
    fn a_cut_line_ends_in_an_ellipsis_within_its_width() {
        let (white, yellow) = (Color32::WHITE, rgb(0x00ff_ff00));
        // A character a pixel wide.
        #[allow(clippy::cast_precision_loss, reason = "a short line")]
        let measure = |s: &str| s.chars().count() as f32;
        let parts = [
            (white, "Bob is ".to_owned()),
            (yellow, "Resting".to_owned()),
        ];

        assert_eq!(
            ellipsised(&parts, 10.0, &measure),
            [(white, "Bob is ".to_owned()), (yellow, "Re…".to_owned())]
        );
        assert_eq!(
            ellipsised(&parts, 8.0, &measure),
            [(white, "Bob is…".to_owned())]
        );
        assert_eq!(ellipsised(&parts, 0.0, &measure), [(white, "…".to_owned())]);
    }

    /// Each wrapped line, drawn, fits its box, and its next word would
    /// not: the wrap measures what is drawn, at each scale.
    #[test]
    fn wrapped_lines_fit_their_box_as_drawn() {
        let font = Font::of(316.0, 15.0);
        let text = "Survive the onslaught of all the most evil creatures in the realm. \
                    No temples of any kind, or any non-Human dwelling may be built.";
        let width = 180.0;

        for scale in [1.0, 1.415, 2.0] {
            let (lines, drawn, next) = with_fonts(|ctx| {
                let lines = font.wrap(ctx, scale, text, width);
                let id = font.id(scale);
                let drawn: Vec<f32> = lines
                    .iter()
                    .map(|l| {
                        ctx.fonts_mut(|f| f.layout_no_wrap(l.clone(), id.clone(), Color32::WHITE))
                            .size()
                            .x
                            / scale
                    })
                    .collect();
                let next: Vec<f32> = lines
                    .windows(2)
                    .map(|pair| {
                        let word = pair[1].split(' ').next().unwrap_or_default();

                        font.width(ctx, scale, &format!("{} {word}", pair[0]))
                    })
                    .collect();

                (lines, drawn, next)
            });

            assert!(lines.len() > 2, "{lines:?}");
            assert!(drawn.iter().all(|&w| w <= width), "{scale}: {drawn:?}");
            assert!(next.iter().all(|&w| w > width), "{scale}: {next:?}");
        }
    }

    /// The Wild's Grizzly Bear help: its tagged sentence wraps to several
    /// lines, each in the tag's colour; the sentence before is the
    /// box's, and no tag is left in any line.
    #[test]
    fn a_tag_colours_every_line_it_wraps_to() {
        const HELP: &str = "Grizzly Bears are a more ferocious species of Hellbears.\r\n\r\n\
                            [color:7070ff]Living in the Hellfire Mountains, Grizzly Bears \
                            rarely are spotted by humans.[/color]";
        let font = Font::of(316.0, 15.0);
        let blue = Color32::from_rgb(0x70, 0x70, 0xff);
        let lines = with_fonts(|ctx| font.wrapped(ctx, 1.0, (HELP, Color32::WHITE), 180.0));
        let tagged: Vec<_> = lines
            .iter()
            .skip_while(|(l, _)| !l.is_empty())
            .skip(1)
            .collect();

        assert!(tagged.len() > 1, "{lines:?}");
        assert!(lines.iter().all(|(l, _)| !l.contains('[')), "{lines:?}");
        assert!(lines[0].1.iter().all(|(c, _)| *c == Color32::WHITE));
        assert!(
            tagged
                .iter()
                .all(|(_, parts)| parts.iter().all(|(c, _)| *c == blue))
        );
    }

    /// HD's fonts: [`SAMPLE`]'s width by advances and spacing, the line
    /// pitch and the dark edge, as `Font::load` reads them; the sizes
    /// they give.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    #[allow(clippy::float_cmp, reason = "the same arithmetic, and whole pitches")]
    fn hds_fonts_give_these_sizes() {
        let catalogue = crate::testdata::hd();
        let fonts = [
            (*b"fn11", (608.0, 47.0), 30.8, true),
            (*b"fn10", (503.0, 18.0), 15.7, true),
            (*b"fnt2", (647.0, 25.0), 21.8, true),
            (*b"fnt4", (316.0, 15.0), 13.1, false),
            (*b"fnt7", (224.0, 10.0), 8.7, false),
        ];

        for (record, (sample, pitch), size, edged) in fonts {
            let font = Font::load(catalogue, FourCc(record)).expect("the font");
            let name = FourCc(record);

            let drawn = font.size_in(typeface::EGUI);

            assert_eq!(font.sample, sample, "{name}");
            assert_eq!(font.pitch, pitch, "{name}");
            assert!((drawn - size).abs() < 0.05, "{name}: {drawn}");
            assert_eq!(font.edge.is_some(), edged, "{name}: {:?}", font.edge);
        }
    }
}
