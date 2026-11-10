//! In-world text: a reward flag's reward and a unit's name (`labels`),
//! an effect's amount (`effects`), drawn by egui in its vector font at the window's
//! pixels, not the original's bitmap fonts (`fnt7`, `SM09`) scaled with
//! the view.
//!
//! ```text
//!  entity with Words, at the text's foot: its baseline's middle
//!    Transforms up its ChildOf chain ──> world point (y up)
//!    a Visibility::Hidden on the chain ──> not drawn
//!  world point ──> View::screen_at ──> the interface's egui layer,
//!    before the bars draw (so under them), clipped to the map's rect
//!  size    the original's digits as tall, times the zoom
//!  shadow  a shade copy, SHADOW × zoom right and down, drawn first
//!  order   by depth, then Tie (board y, agent): the nearer over
//! ```
//!
//! Over every piece, as the flag labels drew already; an amount drew at
//! its effect's depth before. **Inferred**: no capture shows an amount
//! behind a piece.

use bevy::prelude::*;
use bevy_egui::egui::emath::GuiRounding;
use bevy_egui::egui::{self, Color32, FontId, LayerId, Painter, Pos2, pos2, vec2};
use bevy_egui::{EguiContexts, EguiPrimaryContextPass};

use regent_core::view::map::label::{INK, SHADE, SHADOW};
use regent_draw::soft::{self, Blending};
use regent_draw::stage::View;

use crate::files::config::Settings;
use crate::menu::in_game;

/// A size is drawn in steps of this many points: few sizes for egui's
/// glyph cache while the zoom glides.
const STEPS_PER_POINT: f32 = 2.0;

/// `fnt7`'s digits: rows tall, and rows from their foot to the label
/// box's bottom, the shadow's included (`fnt7_and_sm09_digits_measure_so`).
pub const FLAG_DIGITS: i32 = 7;
pub const FLAG_FOOT: i32 = 3;

/// `SM09`'s digits: rows tall, and their foot below the anchor plus the
/// frame's nudge, the box's top `-(rows / 2)` above it (`view::map::amount`).
pub const GOLD_DIGITS: i32 = 7;
pub const GOLD_FOOT: i32 = 3;

/// A reward flag's reward, and a unit's name: `fnt7`'s size, [`INK`],
/// shadowed.
pub const FLAG: Look = Look {
    digits: FLAG_DIGITS,
    grow: 1.0,
    colour: Color32::from_rgb(INK.r, INK.g, INK.b),
    shadow: Shadow::Shade,
};

/// An effect's amount: `SM09`'s size, white (`amount::COLOUR`), no
/// shadow.
pub const GOLD: Look = Look {
    digits: GOLD_DIGITS,
    grow: 1.0,
    colour: Color32::WHITE,
    shadow: Shadow::None,
};

/// Whether a text draws a shade copy under it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shadow {
    Shade,
    None,
}

/// How a kind of text draws.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    /// The original's digits, pixels tall at zoom 1.
    pub digits: i32,
    /// Times that: the size's one knob.
    pub grow: f32,
    pub colour: Color32,
    pub shadow: Shadow,
}

/// A text over the map, its entity at its foot.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Words {
    pub text: String,
    pub look: Look,
    pub tie: Tie,
}

/// What orders texts at one depth: lower paints first, so under.
///
/// A unit's label draws with the unit, in the draw list's board y order,
/// so a nearer unit's over a
/// further one's. **Inferred**: on equal y the agent decides; the engine
/// puts a tie after the units listed before it, an order not modelled.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Tie {
    /// Board y, 32nds of a cell.
    pub y: i32,
    pub agent: u32,
}

/// Paints the words, before the bars.
pub struct Plugin;

impl bevy::app::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            EguiPrimaryContextPass,
            paint.before(crate::ui::bar).run_if(in_game),
        );
    }
}

/// An entity's place up its chain, and whether every link is shown.
pub type Chain<'w, 's> = Query<
    'w,
    's,
    (
        &'static Transform,
        &'static Visibility,
        Option<&'static ChildOf>,
    ),
>;

/// Draw every shown [`Words`] at its place on the map.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
pub fn paint(
    mut contexts: EguiContexts,
    settings: Res<Settings>,
    view: Res<View>,
    words: Query<(Entity, &Words)>,
    chain: Chain,
) -> Result {
    let ctx = contexts.ctx_mut()?;

    if let Some(rect) = view.rect.filter(|_| shown_at(&settings, view.zoom)) {
        let clip =
            egui::Rect::from_min_max(pos2(rect.min.x, rect.min.y), pos2(rect.max.x, rect.max.y));
        let painter = ctx
            .layer_painter(LayerId::background())
            .with_clip_rect(clip);

        paint_on(&painter, &view, &words, &chain);
    }

    Ok(())
}

/// Every shown [`Words`] where `view` projects it: the main view, or
/// another view of the same world.
pub fn paint_on(painter: &Painter, view: &View, words: &Query<(Entity, &Words)>, chain: &Chain) {
    let placed = words
        .iter()
        .filter_map(|(entity, words)| Some((placed(entity, chain)?, words)));

    for (at, words) in ranked(placed) {
        draw(painter, view, at.truncate(), words);
    }
}

/// Whether in-world text shows in a view at zoom level `zoom`: at 1.0
/// alone, as `GSUnit` draws names and `IsoUnitIMG` amounts;
/// with smooth zoom at any (`docs/openregent/enhancements.md`). One rule for every
/// view: the main one and the tracking window (zoom 0.5).
pub fn shown_at(settings: &Settings, zoom: f32) -> bool {
    #[allow(clippy::float_cmp, reason = "the engine compares with 1.0 exactly")]
    let regular = zoom == regent_core::view::map::zoom::REGULAR;

    match crate::map::zoom::Mode::of(settings) {
        crate::map::zoom::Mode::Smooth => true,
        crate::map::zoom::Mode::Levels => regular,
    }
}

/// `entity`'s world place and depth, its translations summed up its
/// chain; `None` where a link is hidden.
fn placed(entity: Entity, chain: &Chain) -> Option<Vec3> {
    let links: Vec<_> = std::iter::successors(chain.get(entity).ok(), |(_, _, up)| {
        up.and_then(|p| chain.get(p.parent()).ok())
    })
    .collect();
    let at: Vec3 = links.iter().map(|(t, ..)| t.translation).sum();

    seen(links.iter().map(|(_, v, _)| **v)).then_some(at)
}

/// Texts in paint order: by depth, then [`Tie`], the furthest first; the
/// same order every run, whatever order the query gives.
fn ranked<'w>(texts: impl Iterator<Item = (Vec3, &'w Words)>) -> Vec<(Vec3, &'w Words)> {
    let mut texts: Vec<_> = texts.collect();

    texts.sort_by(|(a, x), (b, y)| a.z.total_cmp(&b.z).then(x.tie.cmp(&y.tie)));
    texts
}

/// Whether a chain of visibilities shows: none hidden.
fn seen(chain: impl IntoIterator<Item = Visibility>) -> bool {
    chain.into_iter().all(|v| v != Visibility::Hidden)
}

/// `words` at world point `at`, its shade first.
fn draw(painter: &Painter, view: &View, at: Vec2, words: &Words) {
    let look = words.look;
    let font = FontId::proportional(points(look, view.zoom));
    let galley = painter.layout_no_wrap(words.text.clone(), font, look.colour);
    let ascent = galley
        .rows
        .first()
        .and_then(|r| r.row.glyphs.first().map(|g| r.pos.y + g.pos.y))
        .unwrap_or_default();
    let corner = corner(view, at, (galley.size().x, ascent));

    if let Some(corner) = corner.map(|c| c.round_to_pixels(painter.pixels_per_point())) {
        if look.shadow == Shadow::Shade {
            painter.galley_with_override_text_color(
                corner + step(view.zoom),
                galley.clone(),
                shade(),
            );
        }

        painter.galley(corner, galley, look.colour);
    }
}

/// The size drawn at view `zoom`, points: the digits as tall as the
/// original's, in steps of 1 / [`STEPS_PER_POINT`].
///
/// ```text
///  fnt7 at zoom 1, egui's font: 7 / 0.73 = 9.6, drawn 9.5 points; at 2: 19
/// ```
pub fn points(look: Look, zoom: f32) -> f32 {
    #[allow(clippy::cast_precision_loss, reason = "a few pixels")]
    let digits = look.digits as f32;
    let size = digits * look.grow * zoom / crate::ui::typeface::metrics().digit;

    (size * STEPS_PER_POINT).round().max(1.0) / STEPS_PER_POINT
}

/// A galley's top left on screen: centred on world point `at`, its
/// baseline on it; the galley `width` wide, its baseline `ascent` down.
/// `None` until the map has a rect.
pub fn corner(view: &View, at: Vec2, (width, ascent): (f32, f32)) -> Option<Pos2> {
    view.screen_at(at)
        .map(|foot| pos2(foot.x - width / 2.0, foot.y - ascent))
}

/// The shadow's step at view `zoom`, points right and down: [`SHADOW`]
/// world pixels.
pub fn step(zoom: f32) -> egui::Vec2 {
    #[allow(clippy::cast_precision_loss, reason = "a pixel")]
    let (x, y) = (SHADOW.0 as f32, SHADOW.1 as f32);

    vec2(x, y) * zoom
}

/// The shadow's colour: black at the share [`SHADE`]'s soft factor
/// takes off ([`soft::veil`]). The engine moves each pixel's HSV value
/// to 68%; egui blends on linear colour, so black over it at this alpha
/// scales its channels alike.
fn shade() -> Color32 {
    Color32::from_black_alpha(soft::veil(SHADE, Blending::Linear).unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    use regent_core::data::font::Font;
    use regent_core::view::map::amount;
    use regent_core::view::map::label::{self, Pixel};

    /// A view of the map from (0, 0) to (1000, 600), panned to `pan`.
    fn view(pan: Vec2, zoom: f32) -> View {
        View {
            rect: Some(Rect::new(0.0, 0.0, 1000.0, 600.0)),
            zoom,
            pan,
            ..default()
        }
    }

    /// A text's foot sits on its world point: the galley centred on it,
    /// its baseline on it, at any zoom.
    #[test]
    fn a_text_sits_centred_on_its_foot_at_any_zoom() {
        let at = Vec2::new(10.0, -20.0);

        assert_eq!(
            corner(&view(Vec2::ZERO, 1.0), at, (30.0, 8.0)),
            Some(pos2(495.0, 312.0))
        );
        assert_eq!(
            corner(&view(Vec2::ZERO, 2.0), at, (60.0, 16.0)),
            Some(pos2(490.0, 324.0))
        );
        assert_eq!(
            corner(&view(at, 2.0), at, (60.0, 16.0)),
            Some(pos2(470.0, 284.0))
        );
        assert_eq!(
            corner(
                &View {
                    rect: None,
                    ..default()
                },
                at,
                (1.0, 1.0)
            ),
            None
        );
    }

    /// The size and the shadow's step grow with the zoom; a size keeps to
    /// half points.
    #[test]
    fn the_size_and_shadow_grow_with_the_zoom() {
        assert!((points(FLAG, 1.0) - 9.5).abs() < f32::EPSILON);
        assert!((points(FLAG, 2.0) - 19.0).abs() < f32::EPSILON);
        assert!((points(GOLD, 1.3) - 12.5).abs() < f32::EPSILON);
        assert!((points(FLAG, 1.07) * STEPS_PER_POINT).fract().abs() < f32::EPSILON);
        assert_eq!(step(1.0), vec2(1.0, 1.0));
        assert_eq!(step(2.5), vec2(2.5, 2.5));
    }

    /// The original draws in-world text at zoom 1.0 alone; smooth zoom
    /// at any. The tracking window, at 0.5, by the same rule.
    #[test]
    fn text_shows_at_one_and_at_any_zoom_when_smooth() {
        let mut smooth = Settings::default();

        smooth.enhancements.smooth_zoom = true;

        // The wheel in three notches and out three in one frame, as
        // `stage::zoom` takes them: not quite 1:1.
        let mut wheeled = view(Vec2::ZERO, 1.0);

        for _ in 0..3 {
            wheeled.magnify(1.15, None);
        }

        wheeled.magnify(1.15_f32.powf(-3.0), None);

        #[allow(clippy::float_cmp, reason = "the miss is the point")]
        let missed = wheeled.zoom != 1.0;

        assert!(missed, "the wheel came back to exactly 1.0");
        let levels = Settings {
            enhancements: crate::files::config::Enhancements::classic(),
            ..Settings::default()
        };

        assert!(shown_at(&levels, 1.0));
        assert!(!shown_at(&levels, regent_core::view::panel::tracking::ZOOM));
        assert!(shown_at(&smooth, regent_core::view::panel::tracking::ZOOM));
        assert!(shown_at(&smooth, wheeled.zoom));
    }

    /// A text shows only where nothing up its chain is hidden.
    #[test]
    fn a_text_hides_with_anything_above_it() {
        assert!(seen([Visibility::Inherited, Visibility::Visible]));
        assert!(!seen([Visibility::Inherited, Visibility::Hidden]));
        assert!(!seen([Visibility::Hidden]));
    }

    /// Its place sums the translations up its chain, and a hidden link
    /// hides it.
    #[test]
    fn a_text_is_placed_up_its_chain() {
        let mut world = World::new();
        let top = world
            .spawn((Transform::from_xyz(100.0, 50.0, 3.0), Visibility::Inherited))
            .id();
        let middle = world
            .spawn((
                Transform::from_xyz(-4.0, 2.0, 0.0),
                Visibility::Inherited,
                ChildOf(top),
            ))
            .id();
        let foot = world
            .spawn((
                Transform::from_xyz(1.0, -3.0, 0.0),
                Visibility::Inherited,
                ChildOf(middle),
            ))
            .id();
        let placed_of = |world: &mut World| {
            world
                .run_system_cached_with(|In(e): In<Entity>, chain: Chain| placed(e, &chain), foot)
                .expect("runs")
        };

        assert_eq!(placed_of(&mut world), Some(Vec3::new(97.0, 49.0, 3.0)));

        world.entity_mut(middle).insert(Visibility::Hidden);

        assert_eq!(placed_of(&mut world), None);
    }

    /// Texts paint furthest first; at one depth by board y, then handle,
    /// whatever order they come in.
    #[test]
    fn texts_paint_by_depth_then_y_then_handle() {
        let words = |y, agent| Words {
            text: String::new(),
            look: FLAG,
            tie: Tie { y, agent },
        };
        let (near, far) = (Vec3::new(0.0, 0.0, 2.0), Vec3::new(0.0, 0.0, 1.0));
        let all = [
            (near, words(0, 1)),
            (far, words(5, 9)),
            (far, words(5, 3)),
            (far, words(-2, 7)),
        ];
        let order = |texts: &[(Vec3, Words)]| -> Vec<u32> {
            ranked(texts.iter().map(|(at, w)| (*at, w)))
                .iter()
                .map(|(_, w)| w.tie.agent)
                .collect()
        };
        let mut reversed = all.clone();

        reversed.reverse();

        assert_eq!(order(&all), [7, 3, 9, 1]);
        assert_eq!(order(&reversed), [7, 3, 9, 1]);
    }

    /// The shade is see-through black.
    #[test]
    fn the_shade_is_see_through_black() {
        let shade = shade();

        assert_eq!((shade.r(), shade.g(), shade.b()), (0, 0, 0));
        assert!(shade.a() > 0 && shade.a() < u8::MAX);
    }

    /// The digits' rows, as `fnt7` and `SM09` draw them: `fnt7`'s 1 to 7
    /// of an 11-row label box, `SM09`'s 0 to 6 of 9 rows.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn fnt7_and_sm09_digits_measure_so() {
        const DIGITS: &str = "0123456789";

        let catalogue = crate::testdata::hd();

        let flag = label::letter(catalogue, DIGITS)
            .expect("parses")
            .expect("fnt7");
        let inked =
            |y: &usize| (0..flag.width).any(|x| flag.pixels[y * flag.width + x] == Pixel::Ink);
        let rows: Vec<usize> = (0..flag.height).filter(inked).collect();
        let (top, bottom) = (rows[0], rows[rows.len() - 1]);
        let height = i32::try_from(flag.height).expect("rows");
        let foot = i32::try_from(bottom).expect("a row") + 1;

        assert_eq!(i32::try_from(bottom - top + 1).ok(), Some(FLAG_DIGITS));
        assert_eq!(height - foot, FLAG_FOOT);

        let at = catalogue.find(b"FONT", amount::FONT).expect("SM09");
        let small =
            Font::parse(catalogue.record(b"FONT", at).expect("a record").data).expect("parses");
        let ys: Vec<i32> = small
            .pixels(DIGITS.as_bytes())
            .iter()
            .map(|p| p.1)
            .collect();
        let (top, bottom) = (ys.iter().min().expect("ink"), ys.iter().max().expect("ink"));
        let rows = i32::try_from(small.rows).expect("rows");

        assert_eq!(bottom - top + 1, GOLD_DIGITS);
        assert_eq!(amount::top(rows) + bottom + 1, GOLD_FOOT);
    }
}
