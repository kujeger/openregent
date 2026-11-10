//! A unit's set as one picture: every first-pass layer of its stack on
//! one canvas, each at its own frame, as the map places them
//! (`map::cel::Drawing::layers`), for the interface.
//!
//! ```text
//!  layer 0 (building)  ┐
//!  layer 1 (banner)    ├─ each frame's corner = pose offset + nudge - anchor
//!  …                   ┘  canvas = every frame's box, so steps line up
//! ```

use bevy_egui::egui::{Color32, ColorImage};
use regent_core::content::imagery::Blitted;
use regent_core::content::unit::Unit;
use regent_core::data::catalogue::Catalogue;
use regent_core::data::fourcc::FourCc;
use regent_core::data::imag::Pass;
use regent_core::data::tile::Tile;
use regent_core::sim::movement::action::frame_ms;
use regent_core::sim::world::bodies::Facing;
use regent_core::view::Sprite;

use super::Stack;
use super::read::image;

/// Steps a [`Timeline`] holds at most; past it the loop restarts early.
const MAX_STEPS: usize = 240;

/// One stream of one layer of a stack, as [`stacked`] draws it.
#[derive(Clone, Debug)]
pub struct Layer {
    /// Its pose offset, pixels right and down; layer 0 takes none
    /// (`Drawing::posed`).
    pub at: (i32, i32),
    /// Game ms a frame shows (`frame_ms`).
    pub ms: u32,
    /// Its frames; `None` blits nothing.
    pub blits: Vec<Option<Blitted>>,
}

/// The first-pass layers of `unit`'s set `set` facing `facing`, in draw
/// order: by the nudge each sorts at, then layer order, as the map's
/// `cel::rungs`. The second pass, a flier's shadow on the ground, is left
/// out: there is no ground under it here.
#[must_use]
pub fn layers(unit: &Unit<'_>, set: u16, facing: u16) -> Vec<Layer> {
    let base = unit.stack(set).next();
    let turns = Facing::of(&unit.desc);

    // Parts numbered over both passes, as `UnitIMG` makes them.
    let mut found: Vec<(i32, Layer)> = unit
        .stack(set)
        .flat_map(|a| a.passes.iter().map(move |pass| (a, *pass)))
        .enumerate()
        .filter(|(_, (_, pass))| *pass == Pass::First)
        .filter_map(|(part, (a, _))| {
            let slot = turns.slot(facing, a.poses.len());
            let pose = a.poses.get(slot).or_else(|| a.poses.first())?;
            let at = if part > 0 {
                (i32::from(pose.x), i32::from(pose.y))
            } else {
                (0, 0)
            };
            let nudge = base.map_or(a.nudge, |b| a.listed(b));

            Some((0..pose.streams.max(1)).map(move |s| {
                let layer = Layer {
                    at,
                    ms: frame_ms(a.playback),
                    blits: pose.blits(s, Pass::First),
                };

                (nudge, layer)
            }))
        })
        .flatten()
        .collect();

    found.sort_by_key(|(nudge, _)| *nudge);
    found.into_iter().map(|(_, layer)| layer).collect()
}

/// How a stack's layers cycle together: a step every `tick` ms, `steps`
/// to loop. Each layer keeps its own frame time, so the loop is the
/// least multiple of their cycles.
///
/// ```text
///  banner 6 frames × 100 ms, flag 4 × 150 ms  →  tick 50, 600 ms, 12 steps
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timeline {
    pub tick: u32,
    pub steps: usize,
}

impl Timeline {
    /// `layers` together; one step where none moves.
    #[must_use]
    pub fn of(layers: &[Layer]) -> Timeline {
        let moving: Vec<(u64, u64)> = layers
            .iter()
            .filter(|l| l.blits.len() > 1)
            .map(|l| {
                let frames = u64::try_from(l.blits.len()).unwrap_or(u64::MAX);

                (u64::from(l.ms.max(1)), frames)
            })
            .collect();
        let tick = moving.iter().fold(0, |g, &(ms, _)| gcd(g, ms));
        let period = moving.iter().fold(1, |p, &(ms, n)| lcm(p, ms * n));
        let steps = match tick {
            0 => 1,
            _ => usize::try_from(period / tick).unwrap_or(MAX_STEPS),
        };

        Timeline {
            tick: u32::try_from(tick.max(1)).unwrap_or(u32::MAX),
            steps: steps.clamp(1, MAX_STEPS),
        }
    }

    /// The frame `layer` shows at `step`.
    #[must_use]
    pub fn frame(self, layer: &Layer, step: usize) -> usize {
        let ms = u64::try_from(step).unwrap_or(0) * u64::from(self.tick);
        let at = ms / u64::from(layer.ms.max(1));

        usize::try_from(at).unwrap_or(0) % layer.blits.len().max(1)
    }
}

const fn gcd(a: u64, b: u64) -> u64 {
    if b == 0 { a } else { gcd(b, a % b) }
}

const fn lcm(a: u64, b: u64) -> u64 {
    a / gcd(a, b) * b
}

/// Unit `code`'s set `set` facing `facing`: its layers and their
/// timeline. `None` without the unit, or without a layer.
#[must_use]
pub fn stack_of(
    catalogue: &Catalogue<'_>,
    code: FourCc,
    set: u16,
    facing: u16,
) -> Option<(Vec<Layer>, Timeline)> {
    let unit = Unit::find(catalogue, code)?.ok()?;
    let layers = layers(&unit, set, facing);
    let timeline = Timeline::of(&layers);

    (!layers.is_empty()).then_some((layers, timeline))
}

/// A blitted frame's box against the stack's anchor, `[x, y, w, h]`: its
/// pose offset and nudge, less its own anchor.
fn corner(catalogue: &Catalogue<'_>, layer: &Layer, blit: Blitted) -> Option<[i32; 4]> {
    let record = catalogue.positional(blit.archive, b"TILE", blit.index)?;
    let tile = Tile::parse(record.data).ok()?;
    let (ax, ay) = tile.anchor();
    let x = layer.at.0 + i32::from(blit.nudge.0) - i32::from(ax);
    let y = layer.at.1 + i32::from(blit.nudge.1) - i32::from(ay);

    Some([x, y, i32::from(tile.width()), i32::from(tile.height())])
}

/// A stack's step as one picture: each layer's frame at that step drawn
/// over the last as [`image`] gives it, its shadow translucent black over
/// what lies under, on a canvas holding every frame of every layer.
/// `None` where nothing draws.
#[must_use]
pub fn stacked(catalogue: &Catalogue<'_>, stack: Stack) -> Option<ColorImage> {
    let (layers, timeline) = stack_of(catalogue, stack.code, stack.set, stack.facing)?;

    // The canvas: every frame's box, so no step moves.
    let boxes: Vec<[i32; 4]> = layers
        .iter()
        .flat_map(|l| l.blits.iter().flatten().map(move |b| (l, *b)))
        .filter_map(|(l, b)| corner(catalogue, l, b))
        .collect();
    let left = boxes.iter().map(|b| b[0]).min()?;
    let top = boxes.iter().map(|b| b[1]).min()?;
    let right = boxes.iter().map(|b| b[0] + b[2]).max()?;
    let bottom = boxes.iter().map(|b| b[1] + b[3]).max()?;
    let size = [
        usize::try_from(right - left).ok()?,
        usize::try_from(bottom - top).ok()?,
    ];
    let mut canvas = ColorImage::filled(size, Color32::TRANSPARENT);

    for layer in &layers {
        let frame = timeline.frame(layer, stack.step);
        let drawn = layer.blits.get(frame).copied().flatten().and_then(|b| {
            let at = corner(catalogue, layer, b)?;
            let sprite = Sprite::at(catalogue, b.archive, b.index)?.ok()?;

            Some((at, image(&sprite)))
        });

        if let Some(([x, y, ..], picture)) = drawn {
            over(&mut canvas, &picture, (x - left, y - top));
        }
    }

    Some(canvas)
}

/// `top` over `under`, its corner at `at`, clipped to `under`.
fn over(under: &mut ColorImage, top: &ColorImage, (x, y): (i32, i32)) {
    let [w, h] = under.size;
    let rows = top.pixels.chunks(top.size[0].max(1));

    for (row, line) in (y..).zip(rows) {
        for (col, &src) in (x..).zip(line) {
            if let (Ok(px), Ok(py)) = (usize::try_from(col), usize::try_from(row))
                && px < w
                && py < h
            {
                let dst = &mut under.pixels[py * w + px];
                *dst = blend(src, *dst);
            }
        }
    }
}

/// One premultiplied pixel over another.
fn blend(src: Color32, dst: Color32) -> Color32 {
    let keep = u16::from(u8::MAX - src.a());
    let mix = |s: u8, d: u8| {
        let kept = (u16::from(d) * keep + u16::from(u8::MAX / 2)) / u16::from(u8::MAX);

        u8::try_from(u16::from(s) + kept).unwrap_or(u8::MAX)
    };

    Color32::from_rgba_premultiplied(
        mix(src.r(), dst.r()),
        mix(src.g(), dst.g()),
        mix(src.b(), dst.b()),
        mix(src.a(), dst.a()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layer(ms: u32, frames: usize) -> Layer {
        Layer {
            at: (0, 0),
            ms,
            blits: vec![None; frames],
        }
    }

    /// Two moving layers loop at the least multiple of their cycles; a
    /// one-frame layer adds nothing.
    #[test]
    fn a_stacks_timeline_loops_at_its_layers_least_multiple() {
        let timeline = Timeline::of(&[layer(100, 6), layer(150, 4), layer(50, 1)]);

        assert_eq!(
            timeline,
            Timeline {
                tick: 50,
                steps: 12
            }
        );
        assert_eq!(timeline.frame(&layer(100, 6), 3), 1);
        assert_eq!(timeline.frame(&layer(150, 4), 11), 3);
    }

    #[test]
    fn a_still_stack_is_one_step() {
        assert_eq!(Timeline::of(&[layer(100, 1)]).steps, 1);
    }

    /// Half black over white is grey: a shadow darkens what is under.
    #[test]
    fn half_black_over_white_is_grey() {
        assert_eq!(
            blend(Color32::from_black_alpha(128), Color32::WHITE),
            Color32::from_rgba_premultiplied(127, 127, 127, 255)
        );
    }

    /// A picture past the canvas's edge is cut, not wrapped.
    #[test]
    fn a_layer_past_the_edge_is_cut() {
        let mut under = ColorImage::filled([2, 2], Color32::TRANSPARENT);
        let top = ColorImage::filled([2, 2], Color32::WHITE);

        over(&mut under, &top, (1, -1));

        assert_eq!(
            under.pixels,
            [
                Color32::TRANSPARENT,
                Color32::WHITE,
                Color32::TRANSPARENT,
                Color32::TRANSPARENT
            ]
        );
    }
}
