//! A page's layout on the window: at the interface's scale
//! (`layout::scale`, the game's), centred, the leftover the backdrop's.
//! The original draws its pages at their own size; this draws every page
//! at one scale with the game (the user's choice), less only where the
//! page would not fit the window.
//!
//! ```text
//!  window ┌───────────────────────────┐
//!         │   ┌───────────────────┐   │  scale  = min(H / 848, W / w, H / h)
//!         │   │ layout w × h,     │   │  origin = (window − scaled) / 2
//!         │   │ scaled            │   │
//!         │   └───────────────────┘   │
//!         └───────────────────────────┘
//! ```

use bevy_egui::egui::{self, Pos2, Rect, Vec2, pos2, vec2};
use regent_core::data::smnu;
use regent_core::view::menu::quest_map;

use crate::map::zoom::STEPS;
use crate::ui::layout;
use crate::ui::typeface;

/// The menus' scale on `window` for a `page`-sized layout, points a layout
/// pixel: the interface's ([`layout::scale`]), less where the page would
/// not fit (a portrait window).
///
/// ```text
///  800 × 600 on 1920 × 1200: 1.415, the game's   on 1200 × 1600: 1.5
/// ```
#[must_use]
pub fn scale(window: Vec2, page: Vec2) -> f32 {
    let fits = (window.x / page.x).min(window.y / page.y);
    let scale = layout::scale(window).min(fits);

    if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    }
}

/// Layout space to the window's points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fit {
    /// Points a layout pixel.
    pub scale: f32,
    /// Where layout (0, 0) lands.
    pub origin: Pos2,
}

impl Fit {
    /// A `layout`-sized page centred in `window` at the menus' [`scale`].
    #[must_use]
    pub fn page(window: Rect, layout: Vec2) -> Fit {
        Fit::centred(window, scale(window.size(), layout), layout)
    }

    /// A `layout`-sized page centred in `window`, as large as fits: the
    /// screen pages (main menu, a quest's background and summary) and the
    /// tests' panels.
    #[must_use]
    pub fn of(window: Rect, layout: Vec2) -> Fit {
        let scale = (window.width() / layout.x).min(window.height() / layout.y);
        let scale = if scale.is_finite() && scale > 0.0 {
            scale
        } else {
            1.0
        };

        Fit {
            scale,
            origin: window.center() - layout * scale / 2.0,
        }
    }

    /// A `panel`-sized dialog centred in `window` at the scale of the page
    /// behind it, `scale`: the original draws both at their own size.
    #[must_use]
    pub fn centred(window: Rect, scale: f32, panel: Vec2) -> Fit {
        Fit {
            scale,
            origin: window.center() - panel * scale / 2.0,
        }
    }

    /// A layout point on the window.
    #[must_use]
    pub fn point(self, at: Pos2) -> Pos2 {
        self.origin + at.to_vec2() * self.scale
    }

    /// A window point in layout space.
    #[must_use]
    pub fn back(self, at: Pos2) -> Pos2 {
        pos2(0.0, 0.0) + (at - self.origin) / self.scale
    }

    /// A layout rectangle on the window.
    #[must_use]
    pub fn rect(self, r: Rect) -> Rect {
        Rect::from_min_max(self.point(r.min), self.point(r.max))
    }
}

/// egui's text and spacing at the interface's scale ([`layout::scale`]),
/// its defaults at 1 (a window 848 high): the egui pages' text and
/// buttons grow with the menus'. Set again only when the scale or the
/// typeface changes.
pub fn styled(ctx: &egui::Context) {
    let scale = layout::scale(ctx.viewport_rect().size());
    let id = egui::Id::new("interface scale");
    let key = (scale, typeface::metrics().row);

    if ctx.data(|d| d.get_temp::<(f32, f32)>(id)) != Some(key) {
        ctx.all_styles_mut(|style| scaled(style, scale));
        ctx.data_mut(|d| d.insert_temp(id, key));
    }
}

/// `style`'s text and spacing egui's defaults times `scale`, the text as
/// tall in the loaded typeface as in egui's own.
fn scaled(style: &mut egui::Style, scale: f32) {
    let base = egui::Style::default();
    let (from, to) = (base.spacing, &mut style.spacing);

    style.text_styles = base
        .text_styles
        .into_iter()
        .map(|(name, font)| {
            let size = match font.family {
                egui::FontFamily::Proportional => typeface::like_egui(font.size * scale),
                _ => font.size * scale,
            };

            (name, egui::FontId::new(size, font.family))
        })
        .collect();

    to.item_spacing = from.item_spacing * scale;
    to.window_margin = from.window_margin * scale;
    to.button_padding = from.button_padding * scale;
    to.menu_margin = from.menu_margin * scale;
    to.indent = from.indent * scale;
    to.interact_size = from.interact_size * scale;
    to.slider_width = from.slider_width * scale;
    to.slider_rail_height = from.slider_rail_height * scale;
    to.combo_width = from.combo_width * scale;
    to.text_edit_width = from.text_edit_width * scale;
    to.icon_width = from.icon_width * scale;
    to.icon_width_inner = from.icon_width_inner * scale;
    to.icon_spacing = from.icon_spacing * scale;
    to.tooltip_width = from.tooltip_width * scale;
    to.menu_width = from.menu_width * scale;
    to.combo_height = from.combo_height * scale;
}

/// A control's rectangle in layout space.
#[must_use]
#[allow(clippy::cast_precision_loss, reason = "a panel's pixels")]
pub fn rect(r: smnu::Rect) -> Rect {
    Rect::from_min_size(
        pos2(r.x as f32, r.y as f32),
        vec2(r.width as f32, r.height as f32),
    )
}

/// The first of `rects` holding `at`, by its key.
pub fn hit<K: Copy>(rects: impl IntoIterator<Item = (K, Rect)>, at: Pos2) -> Option<K> {
    rects
        .into_iter()
        .find(|(_, r)| r.contains(at))
        .map(|(key, _)| key)
}

/// The quest map under its frame, a departure from the original (which
/// shows it at its own size): zoomed, and moved within the frame. The
/// picture's edges are painted to lie under the frame's bars (23 px a
/// side, 93 top, 59 bottom at its own size), so its painted part is kept
/// over the inside of the bars ([`inside`]).
///
/// ```text
///  map pixel m ──zoom──▶ layout  at + m · zoom ──Fit──▶ window
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lens {
    /// Layout pixels a map pixel.
    pub zoom: f32,
    /// Where the map's top left lands, layout space.
    pub at: Pos2,
}

impl Lens {
    /// A layout point on the map.
    #[must_use]
    pub fn back(self, p: Pos2) -> Pos2 {
        pos2(0.0, 0.0) + (p - self.at) / self.zoom
    }

    /// Map space to the window's points, the page at `page`.
    #[must_use]
    pub fn on(self, page: Fit) -> Fit {
        Fit {
            scale: page.scale * self.zoom,
            origin: page.point(self.at),
        }
    }

    /// Zoomed to `zoom`, layout point `p` holding still.
    #[must_use]
    pub fn about(self, zoom: f32, p: Pos2) -> Lens {
        Lens {
            zoom,
            at: p - (p - self.at) * (zoom / self.zoom),
        }
    }

    /// Kept over `frame`, `map` (a part of the map, map pixels) covering
    /// it: no gap inside it on either axis; centred on an axis the part
    /// is too small to cover.
    #[must_use]
    pub fn bounded(self, frame: Rect, map: Rect) -> Lens {
        let size = map.size() * self.zoom;
        let off = map.min.to_vec2() * self.zoom;
        let axis = |at: f32, (low, high): (f32, f32), size: f32| {
            if size < high - low {
                f32::midpoint(low, high) - size / 2.0
            } else {
                at.clamp(high - size, low)
            }
        };

        Lens {
            zoom: self.zoom,
            at: pos2(
                axis(self.at.x + off.x, (frame.left(), frame.right()), size.x),
                axis(self.at.y + off.y, (frame.top(), frame.bottom()), size.y),
            ) - off,
        }
    }

    /// The map at `zoom`, centred under `frame` as the original centres
    /// it, the half rounded toward 0 (`quest_map::origin`).
    #[must_use]
    pub fn centred(zoom: f32, frame: Rect, map: Rect) -> Lens {
        let half = ((map.size() * zoom - frame.size()) / 2.0).floor();
        let at = frame.min - half - map.min.to_vec2() * zoom;

        Lens { zoom, at }.bounded(frame, map)
    }
}

/// The least zoom: `map` (a part of the map) covering `frame`, its width
/// as a rule (the map is the taller).
#[must_use]
pub fn fitted(frame: Rect, map: Rect) -> f32 {
    let zoom = (frame.width() / map.width()).max(frame.height() / map.height());

    if zoom.is_finite() && zoom > 0.0 {
        zoom
    } else {
        1.0
    }
}

/// The frame's bars, layout pixels: a side (8509, 8503), the top (8501),
/// the bottom (8505). The map picture's edges are painted as wide, map
/// pixels, to lie under them (**Inferred**, by eye: dark margins).
const SIDE: f32 = 23.0;
const TOP: f32 = 93.0;
const BOTTOM: f32 = 59.0;

/// Inside the bars of `frame`, or the painted part of a map picture of
/// size `frame`'s: what the lens fits and keeps covered, so the dark
/// margins stay under the bars at any zoom. At zoom 1 on the original's
/// frame this is the original's bound.
#[must_use]
pub fn inside(frame: Rect) -> Rect {
    Rect::from_min_max(frame.min + vec2(SIDE, TOP), frame.max - vec2(SIDE, BOTTOM))
}

/// The quest map's narrowest frame, layout pixels: `quest_map::LEAST_WIDTH`
/// wide, the column's height.
#[allow(clippy::cast_precision_loss, reason = "a frame's pixels")]
pub const MAP_LEAST: Vec2 = vec2(quest_map::LEAST_WIDTH as f32, layout::COLUMN.y);

/// The quest map's frame on `window`, a departure from the original
/// (which keeps its layout's 1398 × 1200, the faces at its sides): at the
/// menus' [`scale`] for [`MAP_LEAST`], as wide and high as the window, in
/// whole layout pixels, rounded up so it covers; its fit, centred.
///
/// ```text
///  1920 × 1200: 1.415, so 1357 × 848     3440 × 1440: 1.698, 2026 × 848
///  1200 × 1600: 891 wide fits at 1.347, so 891 × 1188
/// ```
#[must_use]
pub fn spread(window: Rect) -> (Fit, Vec2) {
    let scale = scale(window.size(), MAP_LEAST);
    // A hair under, so a whole size divided out stays whole.
    let frame = (window.size() / scale - Vec2::splat(1e-3)).ceil();

    (Fit::centred(window, scale, frame), frame)
}

/// A step within 3 % above the fit is left out, so a notch always shows
/// a change. **Inferred**: chosen by feel.
const NEAR: f32 = 1.03;

/// The map's zooms, least first, layout pixels a map pixel: the fit, then
/// the view's (`zoom::STEPS`, window pixels a map pixel, so 1 is the
/// picture's own pixels) above it, the page at `page` window points a
/// layout pixel.
#[must_use]
pub fn ladder(fit: f32, page: f32) -> Vec<f32> {
    let above = STEPS.iter().map(|s| s / page).filter(|z| *z > fit * NEAR);

    std::iter::once(fit).chain(above).collect()
}

/// The wheel's place on a [`ladder`]: the zoom, and the part notch a
/// touchpad has sent towards the next.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rung {
    /// `None`: the fit.
    pub zoom: Option<f32>,
    pub carry: f32,
}

impl Rung {
    /// Turned `notches` (+ in) on `ladder`: whole ones step from the entry
    /// at or below the zoom, clamped at the ends; the rest waits. The zoom
    /// after.
    pub fn turn(&mut self, ladder: &[f32], notches: f32) -> f32 {
        let total = self.carry + notches;
        let whole = total.trunc();
        let zoom = self.zoom.unwrap_or(ladder[0]);
        // A hair over, so an entry divided out of a window scale still counts.
        let at = ladder
            .iter()
            .rposition(|z| *z <= zoom * 1.0001)
            .unwrap_or(0);
        let to = ladder[shifted(at, whole, ladder.len())];

        self.carry = total - whole;
        self.zoom = Some(to);
        to
    }
}

/// Index `at` moved `by` whole steps, kept below `len`.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "small whole numbers, clamped to the ladder"
)]
fn shifted(at: usize, by: f32, len: usize) -> usize {
    (at as f32 + by).clamp(0.0, (len - 1) as f32) as usize
}

/// Which way the frame's edges push the map, a cursor at `p` within
/// `margin` of them: +1 from the top or left (the map moves down or
/// right), −1 from the bottom or right, 0 inside or outside the frame.
#[must_use]
pub fn push(p: Pos2, frame: Rect, margin: f32) -> Vec2 {
    let axis = |at: f32, (low, high): (f32, f32)| {
        if (low..low + margin).contains(&at) {
            1.0
        } else if (high - margin..high).contains(&at) {
            -1.0
        } else {
            0.0
        }
    };

    if frame.contains(p) {
        vec2(
            axis(p.x, (frame.left(), frame.right())),
            axis(p.y, (frame.top(), frame.bottom())),
        )
    } else {
        Vec2::ZERO
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp, reason = "whole numbers, exact in f32")]
mod tests {
    use super::*;

    const WINDOW: Rect = Rect {
        min: pos2(0.0, 0.0),
        max: pos2(1920.0, 1200.0),
    };

    #[test]
    fn a_dialog_is_centred_at_the_scale_of_the_page_behind() {
        let map = Fit::of(WINDOW, vec2(1398.0, 1200.0));
        let message = Fit::centred(WINDOW, map.scale, vec2(251.0, 323.0));

        assert_eq!(message.scale, 1.0);
        assert_eq!(
            message
                .rect(Rect::from_min_size(Pos2::ZERO, vec2(251.0, 323.0)))
                .center(),
            WINDOW.center()
        );
        assert_eq!(message.point(pos2(21.0, 250.0)), pos2(855.5, 688.5));
    }

    #[test]
    fn a_page_fills_the_height_and_is_letterboxed_left_and_right() {
        let fit = Fit::of(WINDOW, vec2(800.0, 600.0));

        assert_eq!(fit.scale, 2.0);
        assert_eq!(fit.origin, pos2(160.0, 0.0));
        assert_eq!(fit.point(pos2(800.0, 600.0)), pos2(1760.0, 1200.0));
    }

    #[test]
    fn a_wide_page_fills_the_width() {
        let fit = Fit::of(WINDOW, vec2(1920.0, 600.0));

        assert_eq!(fit.scale, 1.0);
        assert_eq!(fit.origin, pos2(0.0, 300.0));
    }

    #[test]
    fn a_window_point_maps_back_to_the_layout() {
        let fit = Fit::of(WINDOW, vec2(1398.0, 1200.0));
        let at = pos2(857.0, 1238.0 - 205.0);

        assert_eq!(fit.scale, 1.0);
        assert_eq!(fit.back(fit.point(at)), at);
        assert_eq!(
            fit.rect(rect(smnu::Rect {
                x: 0,
                y: 0,
                width: 10,
                height: 20
            }))
            .width(),
            10.0
        );
    }

    #[test]
    fn the_first_rectangle_holding_a_point_is_hit() {
        let a = Rect::from_min_size(pos2(0.0, 0.0), vec2(100.0, 100.0));
        let b = Rect::from_min_size(pos2(50.0, 50.0), vec2(100.0, 100.0));

        assert_eq!(hit([(1, a), (2, b)], pos2(60.0, 60.0)), Some(1));
        assert_eq!(hit([(1, a), (2, b)], pos2(120.0, 120.0)), Some(2));
        assert_eq!(hit([(1, a), (2, b)], pos2(200.0, 0.0)), None);
    }

    /// `regent_cli quest-map` at 1920 × 1200.
    const FRAME: Rect = Rect {
        min: pos2(0.0, 0.0),
        max: pos2(1398.0, 1200.0),
    };
    const WHOLE: Rect = Rect {
        min: pos2(0.0, 0.0),
        max: pos2(1398.0, 1611.0),
    };

    /// The map is the frame's width: its own size fits, its height more
    /// than covering the frame's.
    #[test]
    fn the_map_fits_the_frames_width() {
        assert_eq!(fitted(FRAME, WHOLE), 1.0);
        assert_eq!(
            fitted(FRAME, Rect::from_min_size(Pos2::ZERO, vec2(699.0, 1611.0))),
            2.0
        );
        assert_eq!(
            fitted(FRAME, Rect::from_min_size(Pos2::ZERO, vec2(1398.0, 100.0))),
            12.0
        );
    }

    /// The frame fills the window at the interface's scale: 848 high
    /// where the window is wide enough, else [`MAP_LEAST`] wide.
    #[test]
    fn the_frame_spreads_to_the_windows_edges() {
        let at = |w, h| spread(Rect::from_min_size(Pos2::ZERO, vec2(w, h)));

        assert_eq!(at(1920.0, 1200.0).1, vec2(1357.0, 848.0));
        assert_eq!(at(3440.0, 1440.0).1, vec2(2026.0, 848.0));
        assert_eq!(at(1696.0, 848.0).1, vec2(1696.0, 848.0));
        assert_eq!(at(1200.0, 1600.0).1, vec2(891.0, 1188.0));

        // Covering the window, centred: no gap at any edge.
        for (w, h) in [
            (1280.0, 720.0),
            (1920.0, 1200.0),
            (3440.0, 1440.0),
            (1200.0, 1600.0),
        ] {
            let (fit, frame) = at(w, h);
            let drawn = fit.rect(Rect::from_min_size(Pos2::ZERO, frame));

            assert!(drawn.min.x <= 0.0 && drawn.min.y <= 0.0);
            assert!(drawn.max.x >= w && drawn.max.y >= h);
            assert!(drawn.width() - w < fit.scale && drawn.height() - h < fit.scale);
        }
    }

    /// One scale everywhere: the game's interface, a menu page, the quest
    /// map and a dialog over it, on any landscape window.
    #[test]
    fn the_menus_and_the_game_share_one_scale() {
        for window in [
            vec2(1280.0, 720.0),
            vec2(1920.0, 1200.0),
            vec2(2560.0, 1440.0),
            vec2(3440.0, 1440.0),
        ] {
            let game = crate::ui::layout::Frame::of(window).scale;
            let rect = Rect::from_min_size(Pos2::ZERO, window);

            assert_eq!(game, window.y / 848.0);
            assert_eq!(Fit::page(rect, vec2(800.0, 600.0)).scale, game);
            assert_eq!(Fit::page(rect, vec2(251.0, 323.0)).scale, game);
            assert_eq!(spread(rect).0.scale, game);
            assert_eq!(scale(window, MAP_LEAST), game);
        }
    }

    /// A portrait window too narrow for a page shrinks it to fit.
    #[test]
    fn a_page_too_wide_for_the_window_fits_its_width() {
        let window = Rect::from_min_size(Pos2::ZERO, vec2(1200.0, 1600.0));
        let fit = Fit::page(window, vec2(800.0, 600.0));

        assert_eq!(fit.scale, 1.5);
        assert_eq!(fit.origin, pos2(0.0, 350.0));
    }

    /// On a spread frame the painted map fits inside the bars: larger,
    /// scrolling further down, its dark margins under the bars.
    #[test]
    fn the_map_fits_a_spread_frames_width() {
        let frame = Rect::from_min_size(Pos2::ZERO, vec2(2750.0, 1200.0));
        let (inner, painted) = (inside(frame), inside(WHOLE));
        let lens = Lens::centred(fitted(inner, painted), inner, painted);

        assert_eq!(lens.zoom, 2.0);
        assert_eq!(lens.at.x, 23.0 - 46.0);
        assert_eq!(
            Lens {
                at: pos2(0.0, -9e9),
                ..lens
            }
            .bounded(inner, painted)
            .at
            .y,
            1141.0 - 1552.0 * 2.0
        );
    }

    /// At zoom 1 on the original's frame the bars and the margins agree:
    /// the bound and the opening are the whole map's.
    #[test]
    fn inside_the_bars_is_the_originals_bound_at_its_own_size() {
        let (inner, painted) = (inside(FRAME), inside(WHOLE));

        assert_eq!(fitted(inner, painted), 1.0);
        assert_eq!(Lens::centred(1.0, inner, painted).at, pos2(0.0, -205.0));
        assert_eq!(
            Lens {
                zoom: 1.0,
                at: pos2(0.0, -9e9)
            }
            .bounded(inner, painted)
            .at,
            pos2(0.0, -411.0)
        );
    }

    /// At 1920 × 1200 (page scale 1) the fit is the view's 1, then
    /// 1.19 up to 4; at 2560 × 1440 (1.2) the fit is 1.2 window pixels a
    /// map pixel, so 1.19 is under it; a fit 2 % under a step leaves the
    /// step out.
    #[test]
    fn the_ladder_starts_at_the_fit_and_climbs_the_views_steps() {
        let ladder = ladder(1.0, 1.0);

        assert_eq!(ladder[0], 1.0);
        assert_eq!(ladder[1], STEPS[5]);
        assert_eq!(ladder.last(), Some(&4.0));
        assert_eq!(ladder.len(), 10);

        let wide = super::ladder(1.0, 1.2);

        assert_eq!(wide[0], 1.0);
        assert!((wide[1] * 1.2 - std::f32::consts::SQRT_2).abs() < 1e-6);
        assert!(wide.windows(2).all(|w| w[0] < w[1]));

        assert_eq!(super::ladder(0.98, 1.0)[1], STEPS[5]);
    }

    #[test]
    fn the_wheel_climbs_the_ladder_and_stops_at_its_ends() {
        let ladder = [0.9, 1.0, 2.0, 4.0];
        let mut rung = Rung::default();

        assert_eq!(rung.turn(&ladder, -1.0), 0.9);
        assert_eq!(rung.turn(&ladder, 1.0), 1.0);
        assert_eq!(rung.turn(&ladder, 0.5), 1.0);
        assert_eq!(rung.turn(&ladder, 0.5), 2.0);
        assert_eq!(rung.turn(&ladder, 9.0), 4.0);
        assert_eq!(rung.turn(&ladder, -2.0), 1.0);

        // A zoom between entries, after a resize: steps from the one below.
        let mut rung = Rung {
            zoom: Some(1.5),
            carry: 0.0,
        };

        assert_eq!(rung.turn(&ladder, 1.0), 2.0);
    }

    /// A map point and its layout point, both ways, and on the window.
    #[test]
    fn a_map_point_maps_to_layout_and_back() {
        let lens = Lens {
            zoom: 2.0,
            at: pos2(-100.0, -300.0),
        };
        let site = pos2(857.0, 1238.0);
        let page = Fit {
            scale: 0.5,
            origin: pos2(261.0, 0.0),
        };

        let layout = pos2(1614.0, 2176.0);

        assert_eq!(lens.back(layout), site);
        assert_eq!(lens.on(page).point(site), page.point(layout));
        assert_eq!(lens.on(page).back(page.point(layout)), site);
    }

    /// Zooming about the cursor keeps the map point under it.
    #[test]
    fn zooming_holds_the_point_under_the_cursor() {
        let lens = Lens {
            zoom: 1.0,
            at: pos2(23.0, -162.0),
        };
        let cursor = pos2(700.0, 400.0);
        let under = lens.back(cursor);
        let zoomed = lens.about(2.0, cursor);

        assert_eq!(zoomed.zoom, 2.0);
        assert_eq!(zoomed.back(cursor), under);
    }

    /// No gap inside the frame: the map stops at its edges; fitted, it
    /// scrolls 411 at most, as the original's.
    #[test]
    fn the_map_stops_at_the_frames_edges() {
        let at = |x, y, zoom| Lens {
            zoom,
            at: pos2(x, y),
        };

        assert_eq!(at(10.0, 10.0, 1.0).bounded(FRAME, WHOLE).at, pos2(0.0, 0.0));
        assert_eq!(
            at(-50.0, -500.0, 1.0).bounded(FRAME, WHOLE).at,
            pos2(0.0, -411.0)
        );
        assert_eq!(
            at(-9e9, -9e9, 2.0).bounded(FRAME, WHOLE).at,
            pos2(1398.0 - 2796.0, 1200.0 - 3222.0)
        );
        assert_eq!(
            at(-50.0, -50.0, 2.0).bounded(FRAME, WHOLE).at,
            pos2(-50.0, -50.0)
        );

        // Too small to cover: centred.
        let small = at(0.0, 0.0, 0.5).bounded(FRAME, WHOLE);

        assert_eq!(small.at.x, 699.0 - 349.5);
    }

    /// `regent_cli quest-map`: at 1920 × 1200 the map opens at y −205 under
    /// the frame; zoomed, centred both ways.
    #[test]
    fn the_map_opens_centred() {
        assert_eq!(Lens::centred(1.0, FRAME, WHOLE).at, pos2(0.0, -205.0));
        assert_eq!(Lens::centred(2.0, FRAME, WHOLE).at, pos2(-699.0, -1011.0));
    }

    #[test]
    fn eight_pixels_from_the_frames_edges_push_the_map() {
        assert_eq!(push(pos2(7.0, 600.0), FRAME, 8.0), vec2(1.0, 0.0));
        assert_eq!(push(pos2(8.0, 8.0), FRAME, 8.0), Vec2::ZERO);
        assert_eq!(push(pos2(1393.0, 1193.0), FRAME, 8.0), vec2(-1.0, -1.0));
        assert_eq!(push(pos2(700.0, 3.0), FRAME, 8.0), vec2(0.0, 1.0));
        assert_eq!(push(pos2(1500.0, 3.0), FRAME, 8.0), Vec2::ZERO);
    }

    /// The bottom bar's pieces, each snapped to pixels on its own, still
    /// meet: 8505, the compass mount 8506, 8507 (`INx2` 1005, 1010, 1005;
    /// the 1920 × 1200 layout's). Each piece's corner and side rounded
    /// apart left a column empty right of 8506 at 1920 × 1200.
    #[test]
    #[allow(clippy::float_cmp, reason = "edges on whole pixels")]
    fn the_bottom_bars_pieces_meet_on_pixels() {
        use regent_core::data::smnu;
        use regent_core::view::menu::quest_map;
        use regent_draw::art::{Place, Snap};

        let base = (1398, 1200);
        let pieces = [
            (8505, (52, 1141, 538, 59)),
            (8506, (590, 1051, 218, 149)),
            (8507, (808, 1141, 539, 59)),
        ];

        for (w, h) in [
            (1280.0, 720.0),
            (1920.0, 1080.0),
            (1920.0, 1200.0),
            (2560.0, 1440.0),
            (3440.0, 1440.0),
            (1200.0, 1600.0),
        ] {
            for ppp in [1.0, 1.5, 2.0] {
                let (fit, frame) = spread(Rect::from_min_size(Pos2::ZERO, vec2(w, h) / ppp));
                #[allow(clippy::cast_possible_truncation, reason = "whole layout pixels")]
                let to = (frame.x as i32, frame.y as i32);
                let shown: Vec<Rect> = pieces
                    .iter()
                    .map(|&(id, (x, y, width, height))| {
                        let r = smnu::Rect {
                            x,
                            y,
                            width,
                            height,
                        };
                        let place = Place {
                            scale: fit.scale,
                            origin: fit.origin,
                            rect: rect(quest_map::moved(id, r, base, to)),
                            snap: Snap::Edges,
                        };

                        place.pixels(ppp).1
                    })
                    .collect();

                for pair in shown.windows(2) {
                    assert_eq!(pair[0].max.x, pair[1].min.x, "{w}×{h} at {ppp}");
                }
            }
        }
    }
}
