//! Cells to world units and back: where things are drawn, which cells
//! the view needs composed, what a click picks, and the marks.

use bevy::prelude::*;
use regent_core::content::terrain::grid::Terrain;

use regent_core::base::fixed::{self, Fixed};
use regent_core::data::settings::{RELIEF, SQUASH};
use regent_core::sim::movement::ground;
use regent_core::view::map::pick;

use super::{CHUNKS, Item, LIFT, Maps, Placed};
use crate::stage::View;
use crate::terrain::CELL;

/// How a point in cells becomes a point in the world.
///
/// The only such conversion, so ground, sprites and marks agree. The whole map
/// is centred on the origin, regardless of the composed region.
#[derive(Debug, Clone, Copy)]
pub(super) struct Projection {
    /// Cells across the map.
    pub(super) side: f32,
    /// World units per cell across: a tile's width.
    pub(super) pitch: f32,
}

impl Projection {
    /// World units per cell down: shorter, as the camera is pitched
    /// ([`SQUASH`]). Overviews too.
    pub(super) fn drop(self) -> f32 {
        self.pitch * SQUASH
    }

    /// Where a point given in cells sits in world units.
    ///
    /// Y negated (rows count down) and scaled by [`SQUASH`].
    pub(super) fn at(self, cell: Vec2) -> Vec2 {
        let half = self.side / 2.0;

        Vec2::new((cell.x - half) * self.pitch, (half - cell.y) * self.drop())
    }

    /// The whole map's size in world units.
    pub(super) fn extent(self) -> Vec2 {
        Vec2::new(self.side * self.pitch, self.side * self.drop())
    }

    /// The inverse: which cell a world point falls in.
    pub(super) fn cell_at(self, world: Vec2) -> Vec2 {
        let half = self.side / 2.0;

        Vec2::new(world.x / self.pitch + half, half - world.y / self.drop())
    }
}

/// The part of the map the art layer has composed, and how finely.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Composed {
    /// Cells covered: `from` inclusive, `to` exclusive.
    pub(super) from: UVec2,
    pub(super) to: UVec2,
    /// Sheet pixels per cell.
    pub(super) resolution: u32,
    /// The ground's rise it was composed for.
    pub(super) rise: Rise,
}

/// How far the ground's heights move its cells on screen, in whole rows.
///
/// ```text
///  screen bottom ─────────────  a hill past it is drawn
///       ▲ up                    `up` rows higher, so in view
///  hill's flat row ── ▲ ──
/// ```
///
/// `up` for the highest ground, `down` for the lowest (below 0, or a
/// negative relief).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct Rise {
    pub(super) up: u32,
    pub(super) down: u32,
}

impl Composed {
    /// Whether what is on screen is already inside what has been drawn.
    ///
    /// `need` must be unpadded, else every drag frame recomposes.
    pub(super) fn holds(self, need: (UVec2, UVec2)) -> bool {
        self.from.cmple(need.0).all() && self.to.cmpge(need.1).all()
    }

    pub(super) const fn span(self) -> UVec2 {
        UVec2::new(self.to.x - self.from.x, self.to.y - self.from.y)
    }
}

/// The widest composed sheet, in pixels a side.
///
/// Keeps memory bounded; a full-resolution 512-cell map would be 16,384 px.
pub(super) const SHEET_MAX: u32 = 4096;

/// Cells composed beyond what is on screen, as a share of the visible span.
///
/// A quarter each way: 2.25x the pixels, half a screen of drag.
pub(super) const MARGIN: f32 = 0.25;

/// Which cells the ground should be composed from, and how finely.
///
/// Full tile per cell if it fits [`SHEET_MAX`], else halved until it does.
/// Powers of two avoid rebuilds on small zoom changes.
pub(super) fn wanted(view: &View, projection: Projection, side: u32, rise: Rise) -> Composed {
    wanted_in(view.seen(), projection, side, rise)
}

/// [`wanted`] for a flat world rect `seen`: the inset's ([`Inset`]).
pub(super) fn wanted_in(
    seen: Option<Rect>,
    projection: Projection,
    side: u32,
    rise: Rise,
) -> Composed {
    let (from, to) = visible_in(seen, projection, side, MARGIN, rise);

    let widest = (to.x - from.x).max(to.y - from.y).max(1);
    let mut resolution = u32::from(CELL.0);

    while resolution > 1 && widest * resolution > SHEET_MAX {
        resolution /= 2;
    }

    Composed {
        from,
        to,
        resolution,
        rise,
    }
}

/// The cells the viewport can see, padded by `margin` of its own span,
/// and by `rise` for the rows the heights move into view.
///
/// Zero: on screen only. [`MARGIN`]: what to compose.
pub(super) fn visible(
    view: &View,
    projection: Projection,
    side: u32,
    margin: f32,
    rise: Rise,
) -> (UVec2, UVec2) {
    visible_in(view.seen(), projection, side, margin, rise)
}

/// [`visible`] for a flat world rect `seen`; `None` the whole map.
pub(super) fn visible_in(
    seen: Option<Rect>,
    projection: Projection,
    side: u32,
    margin: f32,
    rise: Rise,
) -> (UVec2, UVec2) {
    #[allow(
        clippy::cast_precision_loss,
        reason = "a map is at most 512 cells; f32 is exact well past that"
    )]
    let whole = side as f32;

    // Componentwise min/max: screen and cell y run opposite ways.
    let (mut low, mut high) = match seen {
        Some(seen) => {
            let (a, b) = (projection.cell_at(seen.min), projection.cell_at(seen.max));

            (a.min(b), a.max(b))
        }
        None => (Vec2::ZERO, Vec2::splat(whole)),
    };

    let pad = (high - low) * margin;
    low -= pad;
    high += pad;

    // High ground draws rows past the bottom edge in view, low ground
    // rows past the top: the flat map alone leaves the backdrop there.
    #[allow(clippy::cast_precision_loss, reason = "a rise is a few dozen rows")]
    {
        high.y += rise.up as f32;
        low.y -= rise.down as f32;
    }

    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "clamped to the map, which is at most 512 cells"
    )]
    let (from, to) = (
        UVec2::new(
            low.x.clamp(0.0, whole).floor() as u32,
            low.y.clamp(0.0, whole).floor() as u32,
        ),
        UVec2::new(
            high.x.clamp(0.0, whole).ceil() as u32,
            high.y.clamp(0.0, whole).ceil() as u32,
        ),
    );

    // Compose at least one cell, even with the camera off the map.
    let to = to.max(from + UVec2::ONE).min(UVec2::splat(side));

    (from.min(to - UVec2::ONE), to)
}

/// How far the ground lifts a thing standing at `cell`, in world units.
///
/// The engine's ground ([`ground`]: two triangles a cell, 0 on the last
/// row and column), times `rise` (relief, lift and pitch combined).
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "16.16 of a 512 cell map fits an i32; a height fits an f32"
)]
pub(super) fn lift_at(terrain: Option<&Terrain<'_>>, rise: f32, cell: Vec2) -> f32 {
    let fixed = |v: f32| Fixed::from_bits((v * Terrain::FIXED) as i32);

    terrain.filter(|_| rise != 0.0).map_or(0.0, |terrain| {
        rise * fixed::to_f32(ground(terrain, fixed(cell.x), fixed(cell.y)))
    })
}

/// Where a thing at `cell`, `height` up in the ground's units, is drawn,
/// in world units; `rise` as [`lift_at`] takes it.
pub(super) fn aloft(projection: Projection, rise: f32, cell: Vec2, height: f32) -> Vec2 {
    projection.at(cell) + Vec2::Y * rise * height
}

/// An [`Item`]'s height in the ground's units: the engine draws a unit at
/// its own height, not the ground's (**Binary**). 16.16, as the ground's.
pub(super) fn item_height(item: &Item) -> f32 {
    fixed::to_f32(item.height)
}

/// Every enabled object's chunk and position, art or none, read once
/// per rebuild.
pub fn positions(maps: &Maps) -> Vec<Placed> {
    let mut found = Vec::new();

    for i in 0..CHUNKS.len() {
        if !maps.objects[i] {
            continue;
        }

        let Some(objects) = maps.current().map(|l| &l.source.objects[i]) else {
            continue;
        };

        found.extend(
            objects
                .iter()
                .enumerate()
                .filter(|(_, item)| !item.held)
                .map(|(index, item)| Placed {
                    chunk: i,
                    index,
                    cell: item.cell(),
                    height: item_height(item),
                }),
        );
    }

    found
}

/// Turn a click on the map into a selection.
///
/// Only drawn objects can be picked, so hiding scatter makes buildings easy
/// to hit.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value; that is the contract"
)]
pub fn pick(
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    view: Res<View>,
    window: Single<&Window, With<bevy::window::PrimaryWindow>>,
    mut maps: ResMut<Maps>,
) {
    // Shift with left button pans (`stage::pan`).
    let panning = keys.pressed(KeyCode::ShiftLeft);

    if panning || !buttons.just_pressed(MouseButton::Left) {
        return;
    }

    // Clicks over a panel belong to egui.
    let (Some(rect), Some(cursor)) = (view.rect, window.cursor_position()) else {
        return;
    };

    if !rect.contains(cursor) {
        return;
    }

    let Some(world) = view.world_at(cursor) else {
        return;
    };

    let found = maps.nearest(world);

    // A click on empty ground clears the selection.
    if maps.selected != found {
        maps.selected = found;
        maps.revision += 1;
    }
}

impl Maps {
    /// Where a placed thing is drawn, in world units, at its own height.
    ///
    /// Use this, not `projection.at(thing.cell)` (flat ground), for ring,
    /// crosses and click test.
    pub fn drawn_at(&self, thing: Placed) -> Vec2 {
        self.aloft_at(thing.cell, thing.height)
    }

    /// Where a thing standing at `cell` is drawn, in world units, lift
    /// included.
    pub fn standing_at(&self, cell: Vec2) -> Vec2 {
        let projection = self.projection();
        let rise = RELIEF * LIFT * projection.pitch;
        let up = lift_at(self.terrain().as_ref(), rise, cell);

        projection.at(cell) + Vec2::Y * up
    }

    /// The world height a flat ground point on `row` (cells) is drawn at,
    /// no lift.
    pub fn flat_y(&self, row: f32) -> f32 {
        self.projection().at(Vec2::new(0.0, row)).y
    }

    /// The lowest and highest ground's lifts, world units: what
    /// [`Maps::standing_at`] lifts them by. Reads every vertex.
    pub fn lifts(&self) -> (f32, f32) {
        let rise = RELIEF * LIFT * self.projection().pitch;

        self.terrain().map_or((0.0, 0.0), |terrain| {
            terrain
                .vertices()
                .map(|v| v.height * rise)
                .fold((0.0_f32, 0.0_f32), |(low, high), l| {
                    (low.min(l), high.max(l))
                })
        })
    }

    /// The point the engine's pick gives under world point `world`, in
    /// 32nds: a whole vertex ([`pick::pick`]). `None` off the drawn ground.
    pub fn pick_at(&self, world: Vec2) -> Option<(i32, i32)> {
        let (camera, at) = self.screen(world);

        self.terrain()
            .and_then(|terrain| pick::pick(&terrain, camera, at))
    }

    /// The pick's camera, and world point `world` in its screen pixels:
    /// cells from the map's corner, y down, lifted by the standing rise.
    fn screen(&self, world: Vec2) -> (pick::Camera, [f32; 2]) {
        let projection = self.projection();
        let half = projection.side / 2.0;
        let camera = pick::Camera {
            across: projection.pitch,
            down: projection.drop(),
            up: RELIEF * LIFT * projection.pitch,
        };

        (
            camera,
            [world.x + half * camera.across, half * camera.down - world.y],
        )
    }

    /// Where a thing at `cell`, `height` up in the ground's units, is
    /// drawn: lifted as [`Maps::standing_at`] lifts the ground under it.
    pub fn aloft_at(&self, cell: Vec2, height: f32) -> Vec2 {
        let projection = self.projection();
        let rise = RELIEF * LIFT * projection.pitch;

        aloft(projection, rise, cell, height)
    }

    /// The drawn thing nearest a world point, within [`REACH`].
    ///
    /// World units, not cells, so lifted things answer where they are drawn.
    fn nearest(&self, world: Vec2) -> Option<Placed> {
        let reach = REACH * self.projection().pitch;

        self.placed
            .iter()
            .map(|thing| (thing, self.drawn_at(*thing).distance(world)))
            .filter(|(_, away)| *away <= reach)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(thing, _)| *thing)
    }
}

impl Maps {
    /// How map coordinates reach the world.
    pub(super) fn projection(&self) -> Projection {
        #[allow(
            clippy::cast_precision_loss,
            reason = "a map is at most 512 cells; f32 is exact well past that"
        )]
        Projection {
            side: self.terrain().map_or(1, |t| t.side()) as f32,
            pitch: f32::from(CELL.0),
        }
    }

    /// How far the ground's highest and lowest vertices move on screen,
    /// as [`super::redraw::relief`] lifts them. Reads every vertex: once a compose.
    pub(super) fn rise(&self) -> Rise {
        let projection = self.projection();
        let rows = RELIEF * projection.pitch / projection.drop();

        let (low, high) = self.terrain().map_or((0.0, 0.0), |terrain| {
            terrain
                .vertices()
                .map(|v| v.height * rows)
                .fold((0.0_f32, 0.0_f32), |(low, high), r| {
                    (low.min(r), high.max(r))
                })
        });

        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "not negative, and a few dozen rows"
        )]
        Rise {
            up: high.ceil() as u32,
            down: (-low).ceil() as u32,
        }
    }
}

/// Selection ring size in screen points (divided by zoom).
const MARK: f32 = 7.0;

/// Ring round the selected thing.
const SELECTED: Color = Color::srgb(1.0, 1.0, 1.0);

/// Click reach in cells (about a building), zoom-independent.
const REACH: f32 = 2.0;

/// Ring the selected thing.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value; that is the contract"
)]
pub fn marks(mut gizmos: Gizmos, maps: Res<Maps>, view: Res<View>) {
    let mark = MARK / view.zoom;

    if let Some(thing) = maps.selected {
        gizmos.circle_2d(maps.drawn_at(thing), mark * 1.5, SELECTED);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::fixtures::{art_of, at, hillside, holding, looking, sloped};

    /// `nearest` takes world units: a cell loses the lift.
    #[test]
    fn a_click_finds_the_nearest_drawn_thing_within_reach() {
        let things = vec![
            at(0, 0, 10.0, 10.0),
            at(0, 1, 12.0, 10.0),
            at(1, 7, 40.0, 40.0),
        ];
        let maps = holding(things.clone());
        let (first, second, far) = (
            maps.drawn_at(things[0]),
            maps.drawn_at(things[1]),
            maps.drawn_at(things[2]),
        );

        // Nearest wins, not first.
        let middle = (first + second) / 2.0;

        assert_eq!(
            maps.nearest(middle + Vec2::X).map(|p| p.index),
            Some(1),
            "the far side of the midpoint"
        );
        assert_eq!(
            maps.nearest(middle - Vec2::X).map(|p| p.index),
            Some(0),
            "the near side"
        );

        // The chunk comes back too.
        assert_eq!(maps.nearest(far).map(|p| p.chunk), Some(1));
    }

    #[test]
    fn a_click_on_empty_ground_finds_nothing() {
        let thing = at(0, 0, 10.0, 10.0);
        let maps = holding(vec![thing]);
        let reach = REACH * maps.projection().pitch;
        let drawn = maps.drawn_at(thing);

        // Just inside the reach, then just outside it.
        assert!(maps.nearest(drawn + Vec2::Y * (reach - 1.0)).is_some());
        assert!(maps.nearest(drawn + Vec2::Y * (reach + 1.0)).is_none());
        assert!(holding(Vec::new()).nearest(Vec2::ZERO).is_none());
    }

    /// Sprite, ring and click test all use [`Maps::drawn_at`].
    #[test]
    fn a_thing_on_a_hill_is_marked_where_it_is_drawn() {
        let thing = at(0, 0, 1.5, 1.5);
        let maps = holding(vec![thing]);
        let flat = maps.projection().at(thing.cell);

        // No terrain: no lift.
        assert_eq!(maps.drawn_at(thing), flat, "a flat map lifts nothing");
    }

    /// A record above its ground (flat here) is drawn at the record's
    /// height, as `things` places its sprite.
    #[test]
    fn a_thing_is_drawn_at_its_records_height_not_its_grounds() {
        let thing = Placed {
            height: 2.0,
            ..at(2, 0, 1.5, 1.5)
        };
        let maps = holding(vec![thing]);
        let projection = maps.projection();
        let rise = RELIEF * LIFT * projection.pitch;
        let ground = maps.standing_at(thing.cell);

        assert_eq!(maps.drawn_at(thing), ground + Vec2::Y * rise * 2.0);
        assert_eq!(
            maps.drawn_at(thing),
            aloft(projection, rise, thing.cell, thing.height)
        );
        assert_ne!(maps.drawn_at(thing), ground, "the ground under it is 0");
    }

    /// The cursor takes the vertex the engine's pick gives, in 32nds.
    #[test]
    fn the_cursor_takes_the_engines_vertex_in_32nds() {
        let maps = sloped(8, hillside);
        let near = |x: f32, y: f32| maps.pick_at(maps.standing_at(Vec2::new(x, y)));

        assert_eq!(near(3.1, 2.9), Some((96, 96)));
        assert_eq!(near(4.95, 2.1), Some((160, 64)));

        // Off the drawn ground: nothing.
        assert_eq!(maps.pick_at(maps.standing_at(Vec2::new(-3.0, 4.0))), None);
    }

    #[test]
    fn a_cell_is_squashed_down_the_screen_and_not_across() {
        let projection = Projection {
            side: 128.0,
            pitch: 32.0,
        };

        assert!(
            (projection.drop() - 26.213).abs() < 1e-3,
            "{}",
            projection.drop()
        );

        // One cell across and one down, from the same origin.
        let origin = projection.at(Vec2::ZERO);
        let across = projection.at(Vec2::new(1.0, 0.0)) - origin;
        let down = projection.at(Vec2::new(0.0, 1.0)) - origin;

        assert!((across.x - 32.0).abs() < 1e-3, "across changed: {across:?}");
        assert!((down.y + 26.213).abs() < 1e-3, "down changed: {down:?}");
    }

    /// A full-resolution 512-cell map would be 16,384 px square. Swept over the
    /// three map sizes and eight zooms.
    #[test]
    fn no_composition_ever_exceeds_the_sheet_budget() {
        for side in [128, 256, 512] {
            for zoom in [0.02, 0.05, 0.2, 0.5, 1.0, 4.0, 12.0, 32.0] {
                let want = wanted(
                    &looking(zoom, Vec2::ZERO),
                    art_of(side),
                    side,
                    Rise::default(),
                );
                let span = want.span();

                assert!(
                    span.x * want.resolution <= SHEET_MAX && span.y * want.resolution <= SHEET_MAX,
                    "{side} cells at zoom {zoom}: {span} cells at {} each",
                    want.resolution
                );

                assert!(
                    want.to.x <= side && want.to.y <= side,
                    "{want:?} off the map"
                );
                assert!(
                    want.from.x < want.to.x && want.from.y < want.to.y,
                    "{want:?}"
                );
            }
        }
    }

    /// The other half of the budget rule: do not coarsen a close view.
    #[test]
    fn a_close_look_gets_every_pixel_the_artist_drew() {
        for side in [128, 256, 512] {
            let want = wanted(
                &looking(1.0, Vec2::ZERO),
                art_of(side),
                side,
                Rise::default(),
            );

            assert_eq!(
                want.resolution,
                u32::from(CELL.0),
                "{side} cells at 1:1 was coarsened"
            );
        }
    }

    #[test]
    fn zoomed_out_the_region_is_the_entire_map() {
        for side in [128, 256, 512] {
            // Zoom that fits the map in 800 points.
            #[allow(clippy::cast_precision_loss, reason = "at most 512")]
            let zoom = 800.0 / (side as f32 * f32::from(CELL.0));
            let want = wanted(
                &looking(zoom, Vec2::ZERO),
                art_of(side),
                side,
                Rise::default(),
            );

            assert_eq!(want.from, UVec2::ZERO, "{side} cells");
            assert_eq!(want.to, UVec2::splat(side), "{side} cells");
        }
    }

    /// Without a margin the ground recomposes every drag frame, invisibly.
    #[test]
    fn a_small_pan_needs_no_new_picture_and_a_large_one_does() {
        let side = 256;
        let projection = art_of(side);
        let start = looking(1.0, Vec2::ZERO);
        let have = wanted(&start, projection, side, Rise::default());

        // Inside the margin: a quarter of the 800-point viewport.
        let nudged = visible(
            &looking(1.0, Vec2::new(60.0, 0.0)),
            projection,
            side,
            0.0,
            Rise::default(),
        );
        assert!(have.holds(nudged), "{have:?} did not hold {nudged:?}");

        // Two screens away: not covered.
        let far = visible(
            &looking(1.0, Vec2::new(1600.0, 0.0)),
            projection,
            side,
            0.0,
            Rise::default(),
        );
        assert!(!have.holds(far), "{have:?} claimed to hold {far:?}");
    }

    #[test]
    fn a_change_of_resolution_is_never_covered() {
        let side = 512;
        let projection = art_of(side);

        let close = wanted(&looking(1.0, Vec2::ZERO), projection, side, Rise::default());
        let far = wanted(
            &looking(0.05, Vec2::ZERO),
            projection,
            side,
            Rise::default(),
        );

        assert_ne!(
            close.resolution, far.resolution,
            "a whole map and a close look were composed alike"
        );
    }

    /// Flat ground composes as before: no rise, no extra rows.
    #[test]
    fn flat_ground_has_no_rise() {
        assert_eq!(sloped(8, |_, _| 0.0).rise(), Rise::default());
    }
}
