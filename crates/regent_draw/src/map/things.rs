//! Every object of the map put up: gathered by chunk, sorted by screen
//! depth, each layer an entity stepping its frames (`reel::Layer`).

use std::collections::BTreeMap;

use bevy::camera::primitives::Aabb;
use bevy::camera::visibility::NoAutoAabb;
use bevy::math::Vec3A;
use bevy::prelude::*;

use regent_core::data::desc::UnitKind;
use regent_core::data::fourcc::FourCc;
use regent_core::data::settings::RELIEF;

use super::cel::{Cel, Drawing, Mobility, Owner, Paint, Store, rungs};
use super::depth::{FURTHEST, NEAREST, sorted_at};
use super::project::{Projection, aloft, item_height, lift_at};
use super::reel::{Beat, Layer, Reel, Role, Sways, cycles, dress};
use super::sheet::Paged;
use super::{BLDG, CHUNKS, LIFT, Maps, Placed, VHCL};
use crate::data::Corpus;
use crate::instance::Lean;
use crate::lie::Footing;
use crate::terrain::Drawn;
use crate::wind;

/// A drawn object, so the marks can find it again without re-reading a save.
///
/// Drawn on the unit quad, so its bounds are known and fixed: Bevy's own
/// would check every thing's mesh for a change each frame.
#[derive(Component)]
#[require(NoAutoAabb, Aabb = UNIT_QUAD)]
pub struct Thing;

/// The bounds of the unit quad every object is drawn on.
const UNIT_QUAD: Aabb = Aabb {
    center: Vec3A::ZERO,
    half_extents: Vec3A::new(0.5, 0.5, 0.0),
};

/// Which save record a drawn object's entity is: a front end removes one
/// the game deleted.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Record {
    /// Which of [`CHUNKS`].
    pub chunk: usize,
    /// Which record of that chunk, in file order.
    pub index: usize,
}

/// Picture key: id, named set, slot reading, slot, frame.
///
/// [`Mobility`] comes from the chunk. The frame is `Some` when held, `None`
/// when the whole stream is decoded (shared by all runners).
type Key = (FourCc, u32, Mobility, u16, Option<u16>);

/// What a whole redraw put up.
pub(super) struct Populated {
    /// How far each building's picture reaches ([`Maps::reach`]).
    pub(super) reach: BTreeMap<u32, f32>,
    /// Every object drawn, in chunk and record order ([`Maps::placed`]).
    pub(super) placed: Vec<Placed>,
}

/// Put every object up.
pub(super) fn populate(
    commands: &mut Commands,
    (maps, trees): (&Maps, &wind::Trees),
    corpus: &Corpus,
    (store, paint): (&mut Store, &mut dyn Paint),
    reel: &Reel,
    projection: Projection,
) -> Populated {
    paint.begin();

    let rise = RELIEF * LIFT * projection.pitch;

    things(
        commands,
        (maps, trees),
        corpus,
        (store, paint),
        reel,
        projection,
        rise,
    )
}

/// Spawn every object of every enabled chunk.
///
/// Two passes: [`gather`] collects by chunk; this sorts by screen depth.
///
/// # Mesh, not sprite
///
/// A `Sprite` writes no depth; shadows need depth (see [`crate::material`]).
///
/// Frames are decoded once per distinct descriptor.
#[allow(
    clippy::too_many_arguments,
    reason = "a pass over the whole map needs what it needs, declared rather than fetched"
)]
#[allow(
    clippy::too_many_lines,
    reason = "a thing's parts are placed, sorted and stood in one pass"
)]
fn things(
    commands: &mut Commands,
    (maps, trees): (&Maps, &wind::Trees),
    corpus: &Corpus,
    (store, paint): (&mut Store, &mut dyn Paint),
    reel: &Reel,
    projection: Projection,
    rise: f32,
) -> Populated {
    let mut cache: BTreeMap<Key, Vec<Drawing>> = BTreeMap::new();
    let scene = (maps, trees);
    let (mut standing, reach) = gather(
        scene,
        corpus,
        (store, paint),
        (&mut cache, &mut Pages::Landed),
    );

    // Before the sort: in chunk and record order, as `positions` gives.
    let placed = standing.iter().map(Standing::placed).collect();

    // Painter order: higher on screen is further away.
    standing.sort_by(|a, b| a.cell.y.total_cmp(&b.cell.y));

    #[allow(
        clippy::cast_precision_loss,
        reason = "a save holds tens of thousands of objects, not millions"
    )]
    let step = (NEAREST - FURTHEST) / standing.len().max(1) as f32;
    let rows: Vec<f32> = standing.iter().map(|t| t.cell.y).collect();
    let terrain = maps.terrain();

    for (rank, thing) in standing.iter().enumerate() {
        // Distinct depth per rank: the mesh pipeline compares `GreaterEqual`, so one
        // map row would tie.
        #[allow(
            clippy::cast_precision_loss,
            reason = "a rank is at most a few tens of thousands"
        )]
        let depth = FURTHEST + rank as f32 * step;
        let layers = cache[&thing.key].len();
        let rungs = rungs(&cache[&thing.key]);

        // At its own height, as the engine draws it; a second-pass part,
        // a flier's shadow, on the ground under it.
        // The ground as [`Maps::standing_at`] lifts it, the map read once.
        let at = aloft(projection, rise, thing.cell, thing.height);
        let ground =
            projection.at(thing.cell) + Vec2::Y * lift_at(terrain.as_ref(), rise, thing.cell);

        for (nth, drawing) in cache[&thing.key].iter().enumerate() {
            let mut base = drawing.pass.height(at, ground);

            // A stacked layer's offset places it on the base (burning buildings'
            // `Die` fires at their corners; all 1,451 `Die` offsets are these).
            base += drawing.posed();

            // Layers of one set share a rank and split its step (Palace banner is
            // layer 1). List position here, so survivors spread evenly.
            #[allow(clippy::cast_precision_loss, reason = "a set has at most six layers")]
            let above = rungs[nth] as f32 / layers as f32 * step;

            // A nudged part sorts among the rows its nudge moves it to:
            // a ruin, the Mausoleum's dirt diamond.
            let depth = match drawing.nudge {
                0 => depth,
                px => sorted_at(&rows, thing.cell.y, px, step),
            };

            // Where it stands: its soft shadow lies from there, and the
            // models' shadows find it there (`crate::sun`).
            let footing = Footing {
                row: thing.cell.y,
                ground: ground.y,
            };
            let beat = Beat {
                start: thing.start,
                from: 0,
                ms: drawing.ms,
            };
            // A tree's lean takes it past the unit quad; its foot and
            // bend go in its rows (`instance::Lean`), frame by frame.
            let sways = trees
                .get(thing.key.0)
                .map(|tree| sways(trees, tree, &drawing.cels));

            // A body, then a band where a frame casts (each shader
            // discards the other's pixels), then a lift where a frame is
            // translucent, clearing the coverage of shadows sorted before
            // it.
            for role in [Role::Body, Role::Band, Role::Lift] {
                let layer = Layer::new(drawing.cels.clone(), role, beat, base);

                if role != Role::Body && !layer.draws() {
                    continue;
                }

                let mut spawned = commands.spawn((
                    Drawn,
                    Thing,
                    thing.record,
                    Mesh3d(store.quad.clone()),
                    Lean::default(),
                ));

                dress(&mut spawned, layer, reel.ms, depth + above);

                if role != Role::Lift {
                    spawned.insert(footing);
                }

                if let Some(sways) = &sways {
                    spawned.insert(sways.clone());
                    lean(&mut spawned, sways, beat.up(reel.ms, drawing.cels.len()));
                }
            }
        }
    }

    Populated { reach, placed }
}

/// A tree layer's lean and bounds for each of `cels`.
fn sways(trees: &wind::Trees, tree: wind::Tree, cels: &[Option<Cel>]) -> Sways {
    let each = cels.iter().map(|cel| {
        cel.as_ref().map_or((Lean::default(), UNIT_QUAD), |cel| {
            let foot = wind::foot(cel);
            let bend = trees.bend(tree, foot, cel.size.y);

            (Lean { foot, bend }, wind::bounds(cel, bend))
        })
    });

    Sways(each.collect())
}

/// A tree layer `spawned` leaning as frame `up` of `sways` does.
fn lean(spawned: &mut EntityCommands, sways: &Sways, up: u16) {
    if let Some((lean, bounds)) = sways.0.get(usize::from(up)) {
        spawned.insert((*lean, *bounds));
    }
}

/// One object on the map: what to draw, where, and where its cycle starts.
struct Standing {
    key: Key,
    /// Where it stands, in cells.
    cell: Vec2,
    /// How high: [`super::project::item_height`].
    height: f32,
    /// The frame the save caught it on. See [`Beat::start`].
    start: u16,
    record: Record,
}

impl Standing {
    /// Where it is, for the click test and the marks.
    const fn placed(&self) -> Placed {
        Placed {
            chunk: self.record.chunk,
            index: self.record.index,
            cell: self.cell,
            height: self.height,
        }
    }
}

/// Whether every scatter thing's sprite pages are on their sheets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pages {
    Pending,
    Landed,
}

/// Whether every object's pictures are ready to put up: their pages
/// asked of `paint` where not, so a whole redraw can wait for them.
pub(super) fn ready(
    scene: (&Maps, &wind::Trees),
    corpus: &Corpus,
    drawn: (&mut Store, &mut dyn Paint),
) -> bool {
    let mut pages = Pages::Landed;

    gather(scene, corpus, drawn, (&mut BTreeMap::new(), &mut pages));
    pages == Pages::Landed
}

/// What [`gather`] found: draws, and building picture reach.
type Gathered = (Vec<Standing>, BTreeMap<u32, f32>);

/// Objects to draw and the pictures they need: pass one of [`things`].
///
/// Also returns how far right each building's picture reaches
/// ([`Maps::reach`]).
fn gather(
    (maps, trees): (&Maps, &wind::Trees),
    corpus: &Corpus,
    (store, paint): (&mut Store, &mut dyn Paint),
    (cache, pending): (&mut BTreeMap<Key, Vec<Drawing>>, &mut Pages),
) -> Gathered {
    let mut standing: Vec<Standing> = Vec::new();
    let mut beside: BTreeMap<u32, f32> = BTreeMap::new();

    // Kind is on the descriptor: read once per descriptor.
    let mut kinds: BTreeMap<FourCc, Option<UnitKind>> = BTreeMap::new();

    for (i, (tag, _)) in CHUNKS.iter().enumerate() {
        if !maps.objects[i] {
            continue;
        }

        let Some(objects) = maps.current().map(|l| &l.source.objects[i]) else {
            continue;
        };

        for (index, object) in objects.iter().enumerate() {
            // Held things are inside a building: their position is stale.
            if object.held {
                continue;
            }

            // Saved set, slot and frame. A mobile's slot is a direction.
            let moves = if *tag == VHCL {
                Mobility::Walks
            } else {
                Mobility::Placed
            };
            let picked = object.drawing;

            // Cycling: decode the stream; still: the saved frame.
            let kind = *kinds
                .entry(object.id)
                .or_insert_with(|| corpus.catalogue.unit_kind(object.id));
            let still = (!cycles(kind)).then_some(picked.1);
            let key = (object.id, object.set, moves, picked.0, still);
            let found = cache.entry(key).or_insert_with(|| {
                let mut cel = |s, record: Option<Owner>| match paint.cel(corpus, store, s, record) {
                    Paged::Ready(cel) => cel,
                    Paged::Pending => {
                        *pending = Pages::Pending;
                        None
                    }
                };
                let drawings = Drawing::of(corpus, &mut cel, object, moves, still);

                // A tree's materials bend in the wind, when it blows.
                if trees.get(object.id).is_some() {
                    wind::mark(&drawings, trees, store.bodies, store.bands);
                }

                drawings
            });

            if found.is_empty() {
                continue;
            }

            let cell = object.cell();

            if *tag == BLDG {
                beside.insert(object.handle, found[0].right);
            }

            standing.push(Standing {
                key,
                cell,
                height: item_height(object),
                start: picked.1,
                record: Record { chunk: i, index },
            });
        }
    }

    (standing, beside)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::load::Loaded;
    use crate::map::project::positions;
    use crate::map::redraw::redraw;
    use crate::material;
    use crate::stage::View;
    use crate::terrain::TileArt;
    use regent_core::content::imagery::FrameSource;

    /// A thing's fixed bounds are those Bevy works out for the quad.
    #[test]
    fn a_things_bounds_are_the_quads() {
        use bevy::camera::primitives::MeshAabb;

        let quad = Mesh::from(Rectangle::new(1.0, 1.0));

        assert_eq!(quad.get_aabb(), Some(UNIT_QUAD));
    }

    /// A painter that makes no materials.
    #[derive(Resource, Default)]
    struct Artless;

    impl Paint for Artless {
        fn cel(
            &mut self,
            _: &Corpus,
            _: &mut Store,
            _: FrameSource,
            _: Option<Owner>,
        ) -> super::super::sheet::Paged {
            super::super::sheet::Paged::Ready(None)
        }
    }

    /// A thing with no art is not drawn, so a redraw does not place it:
    /// a click cannot select what is not on screen.
    #[test]
    fn a_thing_with_no_art_is_not_placed() {
        let maps = Maps::with(vec![Loaded::drawn(
            "bare".to_owned(),
            super::super::fixtures::flat_with(vec![super::super::fixtures::tree(1, (16, 16))]),
        )]);

        assert_eq!(positions(&maps).len(), 1, "the tree is in the save");

        // No archives: the tree has no art.
        let corpus = Corpus::bare();
        let mut app = App::new();

        app.insert_resource(TileArt::open(&corpus))
            .insert_resource(corpus)
            .insert_resource(maps)
            .init_resource::<View>()
            .init_resource::<crate::terrain::MapShown>()
            .init_resource::<Reel>()
            .init_resource::<Artless>()
            .init_resource::<Assets<Image>>()
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<crate::plain::Plain>>()
            .init_resource::<Assets<material::Body>>()
            .init_resource::<Assets<material::Band>>()
            .init_resource::<wind::Trees>()
            .add_systems(Update, redraw::<Artless>);
        app.update();

        assert_eq!(app.world().resource::<Maps>().placed, Vec::new());
    }
}
