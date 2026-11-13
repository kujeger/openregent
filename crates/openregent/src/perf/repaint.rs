//! How much of egui's paint output changes from one frame to the next:
//! whether a frame's could be kept, and how many of the compose pass's
//! tiles (`regent_draw::compose::TILE`) a partial redraw would draw.
//!
//! ```text
//!  egui_same    1 where the clipped primitives (texture, clip rect,
//!               vertices, indices) equal the frame before's, else 0
//!  egui_tiles   tiles whose triangles changed: a tile's are those
//!               whose clipped bounds meet it, in draw order
//!  egui_marked  tiles any triangle meets
//! ```
//!
//! Every [`WINDOW`] frames a line names the regions that changed most:
//! connected tiles changed in at least [`OFTEN`] of the frames, their
//! bounds in physical pixels and the share.
//!
//! Only the primitives: a user texture (the minimap's, the tracking
//! window's picture) changes without them.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};

use bevy::log::info;
use bevy_egui::egui::{self, epaint::ClippedPrimitive, epaint::Primitive};
use regent_draw::compose::TILE;

use super::{TARGET, WINDOW, named};

/// The share of frames past which a tile's region is named.
const OFTEN: f64 = 0.05;

/// A tile: column and row.
type Tile = (u32, u32);

/// The last frame's tiles and how often each changed.
#[derive(Default)]
pub struct Repaint {
    last: HashMap<Tile, u64>,
    changed: HashMap<Tile, u32>,
    frames: u32,
}

impl Repaint {
    /// This frame's `jobs`, at `scale` pixels a point, against the last.
    pub fn frame(&mut self, jobs: &[ClippedPrimitive], scale: f32) {
        let now = tiles(jobs, scale);
        let differ = |t: &Tile| now.get(t) != self.last.get(t);
        let changed: Vec<Tile> = now
            .keys()
            .chain(self.last.keys())
            .filter(|t| differ(t))
            .copied()
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        for tile in &changed {
            *self.changed.entry(*tile).or_default() += 1;
        }

        let same = if changed.is_empty() { 1.0 } else { 0.0 };

        named("egui_same", same);
        #[allow(clippy::cast_precision_loss, reason = "a count of tiles")]
        {
            named("egui_tiles", changed.len() as f64);
            named("egui_marked", now.len() as f64);
        }

        self.last = now;
        self.frames += 1;

        if self.frames.is_multiple_of(WINDOW) {
            self.name();
        }
    }

    /// The regions changed most often, one line.
    fn name(&self) {
        let often = |t: &Tile| {
            self.changed
                .get(t)
                .is_some_and(|n| f64::from(*n) >= OFTEN * f64::from(self.frames))
        };
        let mut left: Vec<Tile> = self.changed.keys().filter(|t| often(t)).copied().collect();
        let mut regions = Vec::new();

        left.sort_unstable();

        // Connected tiles, four ways, as one region.
        while let Some(seed) = left.pop() {
            let mut todo = vec![seed];
            let (mut min, mut max, mut sum, mut count) = (seed, seed, 0_u64, 0_u32);

            while let Some((x, y)) = todo.pop() {
                min = (min.0.min(x), min.1.min(y));
                max = (max.0.max(x), max.1.max(y));
                sum += u64::from(self.changed[&(x, y)]);
                count += 1;

                let near = [
                    (x.wrapping_sub(1), y),
                    (x + 1, y),
                    (x, y.wrapping_sub(1)),
                    (x, y + 1),
                ];

                for n in near {
                    if let Some(at) = left.iter().position(|t| *t == n) {
                        todo.push(left.swap_remove(at));
                    }
                }
            }

            #[allow(clippy::cast_precision_loss, reason = "counts of frames")]
            let share = sum as f64 / f64::from(count) / f64::from(self.frames);

            regions.push(format!(
                "[{} {} {} {}] {:.0}%",
                min.0 * TILE,
                min.1 * TILE,
                (max.0 + 1) * TILE,
                (max.1 + 1) * TILE,
                share * 100.0
            ));
        }

        info!(target: TARGET, "egui changed {}", regions.join(", "));
    }
}

/// Each tile's hash of the triangles that meet it, in draw order.
fn tiles(jobs: &[ClippedPrimitive], scale: f32) -> HashMap<Tile, u64> {
    let mut out: HashMap<Tile, u64> = HashMap::new();

    for job in jobs {
        let Primitive::Mesh(mesh) = &job.primitive else {
            continue;
        };

        for triangle in mesh.indices.as_chunks::<3>().0 {
            let corners = [0, 1, 2].map(|i| mesh.vertices[triangle[i] as usize]);
            let bounds = egui::Rect::from_points(&corners.map(|v| v.pos)).intersect(job.clip_rect);

            if !bounds.is_positive() {
                continue;
            }

            let mut hasher = DefaultHasher::new();

            mesh.texture_id.hash(&mut hasher);
            [job.clip_rect.min, job.clip_rect.max]
                .map(|p| [p.x.to_bits(), p.y.to_bits()])
                .hash(&mut hasher);

            for v in corners {
                [v.pos.x, v.pos.y, v.uv.x, v.uv.y]
                    .map(f32::to_bits)
                    .hash(&mut hasher);
                v.color.to_array().hash(&mut hasher);
            }

            let hash = hasher.finish();

            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "pixels on a screen, clamped at 0"
            )]
            let (from, to) = (
                (
                    (bounds.min.x * scale).max(0.0) as u32 / TILE,
                    (bounds.min.y * scale).max(0.0) as u32 / TILE,
                ),
                (
                    (bounds.max.x * scale).max(0.0) as u32 / TILE,
                    (bounds.max.y * scale).max(0.0) as u32 / TILE,
                ),
            );

            for y in from.1..=to.1 {
                for x in from.0..=to.0 {
                    let tile = out.entry((x, y)).or_default();

                    *tile = tile.rotate_left(5) ^ hash;
                }
            }
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A rect of `colour` at `at`, points, one job.
    fn rect(at: egui::Rect, colour: egui::Color32) -> ClippedPrimitive {
        let mut mesh = egui::Mesh::default();

        mesh.add_colored_rect(at, colour);

        ClippedPrimitive {
            clip_rect: egui::Rect::EVERYTHING,
            primitive: Primitive::Mesh(mesh),
        }
    }

    /// A rect recoloured changes the tiles it covers, and no other.
    #[test]
    fn a_change_marks_the_tiles_it_covers() {
        let still = rect(
            egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(64.0, 16.0)),
            egui::Color32::RED,
        );
        let at = egui::Rect::from_min_size(egui::pos2(100.0, 0.0), egui::vec2(10.0, 10.0));
        let before = tiles(&[still.clone(), rect(at, egui::Color32::RED)], 1.0);
        let after = tiles(&[still, rect(at, egui::Color32::BLUE)], 1.0);
        let changed: Vec<&Tile> = before
            .keys()
            .filter(|t| before.get(t) != after.get(t))
            .collect();

        assert_eq!(changed, vec![&(100 / TILE, 0)]);
    }
}
