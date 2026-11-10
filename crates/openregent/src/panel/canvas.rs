//! Drawing in a panel's own pixels: pictures, text in egui's font at the
//! original fonts' sizes, and press areas, all through one fit to the
//! window.

use bevy_egui::egui::emath::GuiRounding;
use bevy_egui::egui::{
    self, Color32, Mesh, Painter, Pos2, Rect, Response, Sense, Shape, Vec2, pos2, vec2,
};
use regent_core::data::catalogue::Catalogue;
use regent_core::data::fourcc::FourCc;
use regent_core::sim::world::interface::Stride;

use super::ink::{Ink, Pic, Tone, Wrap};
use super::stripes::Stripes;
use crate::menu::fit::Fit;
use crate::menu::font;
use regent_draw::art::{Answer, Art, ArtSource, Picture, Place, Shown, Snap, Want};

/// Text with no colour of its own: the panels' white.
pub const WHITE: Color32 = Color32::WHITE;

/// A whole texture's coordinates.
const WHOLE: Rect = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));

/// An edit box's text height, of the box's.
const EDIT_TEXT: f32 = 0.55;

/// A text box's inner margin, pixels. **Inferred**: the help box's lines
/// start 4 px inside (`ui_full.png`).
const PAD: f32 = 4.0;

/// How a line sits in its box.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Left,
    Centre,
}

/// One panel's drawing: the pictures, the fonts and the fit.
pub struct Canvas<'a> {
    pub ui: &'a mut egui::Ui,
    pub ink: &'a mut Ink,
    pub art: &'a mut Art,
    /// The catalogue's source: its archives (`art::Source::of`).
    pub source: ArtSource,
    pub catalogue: &'a Catalogue<'static>,
    pub fit: Fit,
    /// What the press areas' ids are told apart by.
    pub salt: &'static str,
}

impl Canvas<'_> {
    fn painter(&self) -> &Painter {
        self.ui.painter()
    }

    /// `pic` from the catalogue, plain.
    const fn want(&self, pic: Pic) -> Want {
        Want::of(self.source, Picture::Pic(pic))
    }

    /// `pic` at its own size, its top left at panel point `at`; where it
    /// went, panel pixels.
    pub fn picture(&mut self, pic: Pic, tone: Tone, at: Pos2) -> Option<Rect> {
        let want = self.want(pic).tone(tone);
        let place = Rect::from_min_size(at, self.art_size(want)?);
        let shown = self.shown(want, place)?;

        self.painter()
            .image(shown.id, shown.rect, WHOLE, Color32::WHITE);
        Some(place)
    }

    /// `pic`'s own size, panel pixels; `None` where it is not.
    pub fn size(&mut self, pic: Pic) -> Option<Vec2> {
        self.art_size(self.want(pic))
    }

    /// `want`'s own size, panel pixels.
    fn art_size(&mut self, want: Want) -> Option<Vec2> {
        let ctx = self.ui.ctx().clone();

        self.art.size(&ctx, &(self.source, self.catalogue), want)
    }

    /// `want` drawn at panel rectangle `place`: what and where, as the art
    /// service answers.
    fn shown(&mut self, want: Want, place: Rect) -> Option<Shown> {
        let ctx = self.ui.ctx().clone();
        let place = Place {
            scale: self.fit.scale,
            origin: self.fit.origin,
            rect: place,
            snap: Snap::Corner,
        };

        self.art
            .get(&ctx, &(self.source, self.catalogue), want, place)
            .map(Answer::now)
    }

    /// `pic` at its own size from panel point `at`, cut to panel
    /// rectangle `clip`; whether the picture exists.
    pub fn clipped(&mut self, pic: Pic, tone: Tone, at: Pos2, clip: Rect) -> bool {
        let want = self.want(pic).tone(tone);
        let shown = self
            .art_size(want)
            .and_then(|size| self.shown(want, Rect::from_min_size(at, size)));

        if let Some(shown) = shown {
            self.painter().with_clip_rect(self.fit.rect(clip)).image(
                shown.id,
                shown.rect,
                WHOLE,
                Color32::WHITE,
            );
        }

        shown.is_some()
    }

    /// `pic` stretched over panel rectangle `rect`: as shapes where it
    /// is only bands of colour.
    pub fn stretched(&mut self, pic: Pic, tone: Tone, rect: Rect) -> Option<Rect> {
        if let Some(stripes) = self.ink.exact(self.catalogue, pic, tone) {
            self.stripes(&stripes, rect, rect);
        } else {
            let shown = self.shown(self.want(pic).tone(tone), rect)?;

            self.painter()
                .image(shown.id, shown.rect, WHOLE, Color32::WHITE);
        }

        Some(rect)
    }

    /// `stripes` laid over panel rectangle `onto`, cut to `clip`: each
    /// band a flat rectangle, its edges on the window's pixels, so
    /// neighbours meet with no seam.
    pub fn stripes(&self, stripes: &Stripes, onto: Rect, clip: Rect) {
        let ppp = self.ui.ctx().pixels_per_point();
        let mut mesh = Mesh::default();

        for (band, colour) in stripes.bands(onto) {
            let shown = self.fit.rect(band.intersect(clip)).round_to_pixels(ppp);

            if shown.is_positive() {
                mesh.add_colored_rect(shown, colour);
            }
        }

        self.painter().add(Shape::mesh(mesh));
    }

    /// `pic` repeated over panel rectangle `rect` from its top left.
    pub fn tiled(&mut self, pic: Pic, rect: Rect) {
        let want = self.want(pic).wrap(Wrap::Tiled);

        if let Some(size) = self.art_size(want) {
            let uv = Rect::from_min_size(pos2(0.0, 0.0), rect.size() / size);
            let tile = Rect::from_min_size(rect.min, size);

            if let Some(shown) = self.shown(want, tile) {
                self.painter()
                    .image(shown.id, self.fit.rect(rect), uv, Color32::WHITE);
            }
        }
    }

    /// A flat colour over panel rectangle `rect`.
    pub fn fill(&self, rect: Rect, colour: Color32) {
        self.painter().rect_filled(self.fit.rect(rect), 0.0, colour);
    }

    /// `text` on one line in panel rectangle `rect`, centred up and down,
    /// in font `record`; markup sets colours, `colour` the rest.
    pub fn text(
        &mut self,
        record: FourCc,
        rect: Rect,
        text: &str,
        (colour, align): (Color32, Align),
    ) {
        let fit = self.fit;
        let Some(font) = self.ink.font(self.catalogue, record) else {
            return;
        };
        let parts = font::spans(text, colour);
        let line: String = parts.iter().map(|(_, s)| s.as_str()).collect();
        let cap = font.cap(fit.scale);
        let x = match align {
            Align::Left => rect.left() + PAD,
            Align::Centre => rect.center().x - font.width(self.ui.ctx(), fit.scale, &line) / 2.0,
        };
        let pen = pos2(x, rect.center().y + cap / 2.0);
        // Room below for descenders: a line's rectangle is about its capitals.
        let room = rect.expand2(vec2(1.0, cap / 2.0));

        font.draw(
            &self.painter().with_clip_rect(fit.rect(room)),
            fit,
            pen,
            &parts,
        );
    }

    /// `text` wrapped in panel rectangle `rect` from `scroll` pixels down,
    /// each paragraph in the colour its markup last set; how tall the
    /// lines are, pixels.
    pub fn paragraphs(
        &mut self,
        record: FourCc,
        rect: Rect,
        text: &str,
        (colour, align, scroll): (Color32, Align, f32),
    ) -> f32 {
        let ctx = self.ui.ctx().clone();
        let fit = self.fit;
        let Some(font) = self.ink.font(self.catalogue, record) else {
            return 0.0;
        };
        let lines = self.wrapped(font, record, rect.width(), (text, colour));
        let (pitch, cap) = (font.pitch, font.cap(fit.scale));
        let painter = self.painter().with_clip_rect(fit.rect(rect));

        for (n, (line, parts)) in lines.iter().enumerate() {
            #[allow(clippy::cast_precision_loss, reason = "a box's lines")]
            let top = n as f32 * pitch;
            let x = match align {
                Align::Left => rect.left() + PAD,
                Align::Centre => rect.center().x - font.width(&ctx, fit.scale, line) / 2.0,
            };
            let y = rect.top() + top + f32::midpoint(pitch, cap) - scroll;

            font.draw(&painter, fit, pos2(x, y), parts);
        }

        #[allow(clippy::cast_precision_loss, reason = "a box's lines")]
        let tall = lines.len() as f32 * pitch;

        tall
    }

    /// As many of [`Canvas::paragraphs`]' lines of `text` as fit in panel
    /// rectangle `rect`, centred up and down, drawn only inside `clip`; a
    /// text cut short ends in "…". A list row's text keeps to its row
    /// however tall the font.
    pub fn fitted(
        &mut self,
        record: FourCc,
        (rect, clip): (Rect, Rect),
        text: &str,
        colour: Color32,
    ) {
        let ctx = self.ui.ctx().clone();
        let fit = self.fit;
        let Some(font) = self.ink.font(self.catalogue, record) else {
            return;
        };
        let lines = self.wrapped(font, record, rect.width(), (text, colour));
        let (pitch, cap) = (font.pitch, font.cap(fit.scale));

        // The whole lines the rectangle holds, at least one.
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a row's few lines"
        )]
        let room = ((rect.height() / pitch).floor() as usize).max(1);
        let kept = lines.len().min(room);
        let width = rect.width() - 2.0 * PAD;
        let measure = |s: &str| font.width(&ctx, fit.scale, s);

        #[allow(clippy::cast_precision_loss, reason = "a row's few lines")]
        let top = rect.center().y - kept as f32 * pitch / 2.0;
        let painter = self.painter().with_clip_rect(fit.rect(clip));

        for (n, (_, parts)) in lines.iter().take(kept).enumerate() {
            let cut = n + 1 == kept && lines.len() > kept;
            let parts = if cut {
                font::ellipsised(parts, width, &measure)
            } else {
                parts.clone()
            };

            #[allow(clippy::cast_precision_loss, reason = "a row's few lines")]
            let y = top + n as f32 * pitch + f32::midpoint(pitch, cap);

            font.draw(&painter, fit, pos2(rect.left() + PAD, y), &parts);
        }
    }

    /// `text` wrapped to a box `width` wide in `font`, each line with its
    /// characters' colours, kept while drawn (`Ink::wrapped`).
    fn wrapped(
        &mut self,
        font: font::Font,
        record: FourCc,
        width: f32,
        (text, colour): (&str, Color32),
    ) -> super::ink::Lines {
        let ctx = self.ui.ctx().clone();
        let scale = self.fit.scale;
        let width = width - 2.0 * PAD;
        let key = (
            (record, scale.to_bits(), width.to_bits(), text.to_owned()),
            colour,
        );

        self.ink.wrapped.get(ctx.cumulative_pass_nr(), key, || {
            font.wrapped(&ctx, scale, (text, colour), width).into()
        })
    }

    /// How far a press goes this frame: far with either Shift held.
    /// egui's modifier follows the logical key, so both count.
    #[must_use]
    pub fn stride(&self) -> Stride {
        if self.ui.input(|i| i.modifiers.shift) {
            Stride::Far
        } else {
            Stride::One
        }
    }

    /// A press area over panel rectangle `rect`, told apart by `key`.
    pub fn press(&mut self, rect: Rect, key: impl std::hash::Hash + std::fmt::Debug) -> Response {
        let id = self.ui.id().with((self.salt, key));

        self.ui.interact(self.fit.rect(rect), id, Sense::click())
    }

    /// A tooltip area that does not take clicks.
    pub fn hover(&mut self, rect: Rect, key: impl std::hash::Hash + std::fmt::Debug) -> Response {
        let id = self.ui.id().with((self.salt, key));

        self.ui.interact(self.fit.rect(rect), id, Sense::hover())
    }

    /// A one-line text box over panel rectangle `rect`, told apart by
    /// `key`, editing `text` up to `limit` characters; it holds the
    /// keyboard while shown.
    pub fn edit(
        &mut self,
        rect: Rect,
        (text, limit): (&mut String, usize),
        key: impl std::hash::Hash + std::fmt::Debug,
    ) -> Response {
        let screen = self.fit.rect(rect);
        let id = self.ui.id().with((self.salt, key));
        let font = egui::FontId::proportional(screen.height() * EDIT_TEXT);
        let edit = egui::TextEdit::singleline(text)
            .id(id)
            .font(font.clone())
            .horizontal_align(egui::Align::Center)
            .char_limit(limit);

        // A row taller than the box set about its centre, cut to the box.
        let room = crate::ui::typeface::edit_room(self.ui.ctx(), &font, screen);
        let clip = self.ui.clip_rect();

        self.ui.set_clip_rect(clip.intersect(screen));

        let response = self.ui.put(room, edit);

        self.ui.set_clip_rect(clip);

        response.request_focus();
        response
    }

    /// A drag area over panel rectangle `rect`, told apart by `key`.
    pub fn drag(&mut self, rect: Rect, key: impl std::hash::Hash + std::fmt::Debug) -> Response {
        let id = self.ui.id().with((self.salt, key));

        self.ui.interact(self.fit.rect(rect), id, Sense::drag())
    }

    /// How far the wheel scrolled over panel rectangle `rect`, pixels.
    pub fn wheel(&self, rect: Rect) -> f32 {
        let key = (
            self.salt,
            "wheel",
            [rect.min.x, rect.min.y].map(f32::to_bits),
        );
        let over = crate::menu::paint::pointed(self.ui, self.fit.rect(rect), key);
        let delta = self.ui.ctx().input(|i| i.smooth_scroll_delta.y);

        if over { -delta / self.fit.scale } else { 0.0 }
    }

    /// How many lines [`Canvas::paragraphs`] wraps `text` to in a box
    /// `width` wide.
    pub fn lines(&mut self, record: FourCc, width: f32, text: &str) -> usize {
        let ctx = self.ui.ctx().clone();
        let scale = self.fit.scale;
        let key = (record, scale.to_bits(), width.to_bits(), text.to_owned());

        self.ink.font(self.catalogue, record).map_or(0, |f| {
            self.ink.counted.get(ctx.cumulative_pass_nr(), key, || {
                coloured(text, WHITE)
                    .iter()
                    .map(|(_, p)| f.wrap(&ctx, scale, p, width - 2.0 * PAD).len())
                    .sum()
            })
        })
    }

    /// `text`'s width in font `record`, markup left out, pixels; 0
    /// without the font.
    pub fn width(&mut self, record: FourCc, text: &str) -> f32 {
        let ctx = self.ui.ctx().clone();
        let scale = self.fit.scale;

        self.ink.font(self.catalogue, record).map_or(0.0, |f| {
            let line: String = font::spans(text, WHITE)
                .into_iter()
                .map(|(_, s)| s)
                .collect();

            f.width(&ctx, scale, &line)
        })
    }

    /// A line `pitch` of font `record`, pixels; 0 without the font.
    pub fn pitch(&mut self, record: FourCc) -> f32 {
        self.ink
            .font(self.catalogue, record)
            .map_or(0.0, |f| f.pitch)
    }

    /// `text` for the tooltip line while `response`'s area is hovered
    /// ([`super::tip`]).
    pub fn tip(response: &Response, text: Option<&str>) {
        super::tip::offer(response, text);
    }
}

/// `text`'s paragraphs, each in the colour its markup last set, markup
/// dropped: `\x01FFDD00Upgrading…` a yellow paragraph.
#[must_use]
pub fn coloured(text: &str, start: Color32) -> Vec<(Color32, String)> {
    let mut colour = start;

    font::painted(text, start)
        .into_iter()
        .map(|paragraph| {
            let first = paragraph.first().map_or(colour, |(_, c)| *c);

            colour = paragraph.last().map_or(colour, |(_, c)| *c);
            (first, paragraph.into_iter().map(|(ch, _)| ch).collect())
        })
        .collect()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// One pass of a menu over the game on its own layer, as `ui::covered`
    /// and `end::panel` draw theirs, the wheel turned `lines` (+ up) with
    /// the pointer at `rect`'s middle; how far it scrolled `rect`.
    fn wheeled_over_game(ctx: &egui::Context, rect: Rect, lines: f32) -> f32 {
        let catalogue: &'static Catalogue<'static> =
            Box::leak(Box::new(Catalogue::build(&[]).expect("an empty catalogue")));
        let window = Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0));
        let wheel = egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Line,
            delta: vec2(0.0, lines),
            modifiers: egui::Modifiers::NONE,
            phase: egui::TouchPhase::Move,
        };
        let input = egui::RawInput {
            screen_rect: Some(window),
            events: vec![egui::Event::PointerMoved(rect.center()), wheel],
            ..egui::RawInput::default()
        };
        let (mut ink, mut art) = (Ink::default(), Art::default());
        let mut moved = 0.0;
        let mut output = ctx.run_ui(input, |ui| {
            let mut ui = egui::Ui::new(
                ui.ctx().clone(),
                "options".into(),
                egui::UiBuilder::new()
                    .layer_id(egui::LayerId::new(
                        egui::Order::Foreground,
                        "options".into(),
                    ))
                    .max_rect(window),
            );
            let canvas = Canvas {
                ui: &mut ui,
                ink: &mut ink,
                art: &mut art,
                source: ArtSource::of(&[]),
                catalogue,
                fit: Fit::of(window, window.size()),
                salt: "test",
            };

            moved = canvas.wheel(rect);
        });

        // No painter takes the font atlas here.
        output.textures_delta.clear();
        moved
    }

    /// The wheel scrolls a box of a menu over the game: `AP88`'s goals,
    /// the end's verdict.
    #[test]
    fn the_wheel_scrolls_a_box_over_the_game() {
        let ctx = egui::Context::default();
        let rect = Rect::from_min_size(pos2(100.0, 100.0), vec2(200.0, 100.0));

        // A pass to lay the menu out, as the game's last frame did.
        wheeled_over_game(&ctx, rect, 0.0);

        assert!(wheeled_over_game(&ctx, rect, -1.0) > 0.0);
    }

    #[test]
    fn markup_colours_the_characters_after_it_into_the_next_paragraph() {
        let yellow = Color32::from_rgb(0xff, 0xff, 0x00);
        let paragraphs = font::painted("a\u{1}FFFF00b\nc", WHITE);

        assert_eq!(paragraphs[0], [('a', WHITE), ('b', yellow)]);
        assert_eq!(paragraphs[1], [('c', yellow)]);
    }

    /// A tag opened in one paragraph ends in another; past its end, the
    /// box's colour again.
    #[test]
    fn a_tag_runs_across_paragraphs_to_its_end() {
        let red = Color32::from_rgb(0xff, 0, 0);
        let paragraphs = font::painted("[color:red]a\r\nb[/color]\nc", WHITE);

        assert_eq!(
            paragraphs,
            [vec![('a', red)], vec![('b', red)], vec![('c', WHITE)]]
        );
    }

    #[test]
    fn a_paragraph_takes_the_colour_its_markup_set() {
        let yellow = Color32::from_rgb(0xff, 0xff, 0x00);
        let lines = coloured("plain\n\u{1}FFFF00gold\nstill gold", WHITE);

        assert_eq!(lines[0], (WHITE, "plain".to_owned()));
        assert_eq!(lines[1], (yellow, "gold".to_owned()));
        assert_eq!(lines[2], (yellow, "still gold".to_owned()));
    }

    /// The names of the textures one frame drawing `draw` paints.
    pub(crate) fn painted_textures(draw: impl Fn(&mut Canvas<'_>)) -> Vec<String> {
        // The art keeps its textures while they are read.
        let (ctx, output, _art) = painted(draw);
        let manager = ctx.tex_manager();
        let manager = manager.read();

        output
            .shapes
            .iter()
            .filter_map(|s| match &s.shape {
                Shape::Mesh(mesh) => manager.meta(mesh.texture_id).map(|m| m.name.clone()),
                _ => None,
            })
            .collect()
    }

    /// Where one frame drawing `draw` in typeface `face` shows anything:
    /// each shape's bounds cut to its clip, empty ones left out; window
    /// pixels at the fit's scale 1.
    pub(crate) fn painted_bounds_in(
        face: crate::files::config::Typeface,
        draw: impl Fn(&mut Canvas<'_>),
    ) -> Vec<Rect> {
        let (_, output, _) = crate::ui::typeface::in_face(face, || painted_in(face, draw));

        output
            .shapes
            .iter()
            .map(|s| s.shape.visual_bounding_rect().intersect(s.clip_rect))
            .filter(Rect::is_positive)
            .collect()
    }

    /// One frame drawing `draw` on a 400 × 300 window, and the art that
    /// holds its textures.
    fn painted(
        draw: impl Fn(&mut Canvas<'_>),
    ) -> (egui::Context, egui::FullOutput, regent_draw::art::Art) {
        painted_in(crate::files::config::Typeface::Egui, draw)
    }

    /// [`painted`] with typeface `face`'s fonts loaded.
    fn painted_in(
        face: crate::files::config::Typeface,
        draw: impl Fn(&mut Canvas<'_>),
    ) -> (egui::Context, egui::FullOutput, regent_draw::art::Art) {
        let catalogue = crate::testdata::hd();
        let window = Rect::from_min_size(Pos2::ZERO, vec2(400.0, 300.0));
        let ctx = egui::Context::default();

        ctx.set_fonts(crate::ui::typeface::definitions(face));

        let mut ink = Ink::default();
        let mut art = regent_draw::art::Art::default();
        let input = egui::RawInput {
            screen_rect: Some(window),
            ..egui::RawInput::default()
        };
        let mut output = ctx.run_ui(input, |ui| {
            let mut canvas = Canvas {
                ui,
                ink: &mut ink,
                art: &mut art,
                source: regent_draw::art::ArtSource::of(&[]),
                catalogue,
                fit: Fit::of(window, window.size()),
                salt: "test",
            };

            draw(&mut canvas);
        });

        // No painter takes the textures here.
        output.textures_delta.clear();

        (ctx, output, art)
    }

    /// A list row's icon, drawn cut to its list, is the portrait shrunk,
    /// as an icon drawn whole is.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_list_icon_cut_to_its_list_is_the_portrait_shrunk() {
        // The warrior, a character.
        let icon = Pic::of(FourCc(*b"AVL1"), 1002);
        let clip = Rect::from_min_size(pos2(0.0, 5.0), vec2(100.0, 100.0));
        let names = painted_textures(|c| {
            c.clipped(icon, Tone::Plain, pos2(10.0, 10.0), clip);
        });

        assert!(names.iter().any(|n| n.starts_with("shrunk")), "{names:?}");
    }
}
