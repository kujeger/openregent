//! A LAN resync's world drawn as a new game's: a peer's world replaced
//! by the host's (`Sim::resync`) may hold another board at the same
//! `Board::relaid`, and other pieces at the same frame, so what the
//! drawing took of the old world is dropped, not brought up to date.
//!
//! ```text
//!  sim thread  Sim::resyncs moved ──> Taken afresh: no piece walked, fog anew
//!  Frame::Step game::clock::tick: seen::Frame::resyncs moved ──> redraw:
//!                ground, scatter   drawn whole (ground::anew)
//!                Saved             the new world's looks
//!                veil, reach       despawned, drawn again
//!                minimap           its ground drawn whole
//!                tracking          the standing pieces not taken as born
//!                panel rows        kept by frame: forgotten
//!                selection         a piece or message flag gone: forgotten
//! ```
//!
//! The pieces need nothing: `mirror::sync` mirrors any world as it
//! stands, and each piece's drawing follows its mirror.

use bevy::prelude::*;

use regent_draw::map::Maps;

use crate::game::seen::Frame;
use crate::{
    input::orders, map::fog, map::ground, map::reach, map::select, map::trail, ui, ui::minimap,
    ui::tracking,
};

/// The resyncs the drawing was last redrawn after.
#[derive(Resource, Default)]
pub struct Redrawn(u32);

/// Whether the frame's world came from a resync since the last drawn.
/// Asked off a game too, as the frame's sets' conditions are.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
pub fn moved(frame: Option<Res<Frame>>, drawn: Res<Redrawn>) -> bool {
    frame.is_some_and(|f| f.resyncs != drawn.0)
}

/// Drop what the drawing took of the world before the resync.
pub fn redraw(app: &mut World) {
    let frame = app.resource::<Frame>();
    let (resyncs, world) = (frame.resyncs, frame.taken.world.clone());

    app.resource_scope(|app, mut laid: Mut<ground::Laid>| {
        ground::anew(&mut app.resource_mut::<Maps>(), &mut laid, &world);
    });
    app.insert_resource(crate::map::pieces::Saved::of(&world));

    // Entities drawn over the old ground.
    let mut gone = app.resource_mut::<fog::Veils>().forget();

    gone.extend(app.resource_mut::<reach::ReachDrawn>().forget());

    for entity in gone {
        app.despawn(entity);
    }

    app.resource_mut::<minimap::Minimap>().forget();
    app.resource_mut::<tracking::Tracking>().anew(&world);
    app.resource_mut::<trail::Ghosts>().forget();

    // Rows read at a frame the new world may stand at too.
    app.insert_resource(ui::BarMemo::default());
    app.insert_resource(orders::Built::default());
    app.resource_mut::<crate::panel::Panels>().state.forget();

    // A selection the new world lacks.
    let shown = world.shown();
    let mut selected = app.resource_mut::<select::Selected>();

    if selected
        .0
        .is_some_and(|a| shown.iter().all(|s| s.agent != a))
    {
        selected.0 = None;
    }

    let mut reading = app.resource_mut::<select::Reading>();

    if reading
        .0
        .is_some_and(|h| world.messages().iter().all(|m| m.handle != h))
    {
        reading.0 = None;
    }

    app.resource_mut::<Redrawn>().0 = resyncs;
}

/// The resyncs drawn, a game's.
pub fn plugin(app: &mut App) {
    use crate::game::scope::Scoped as _;

    app.per_game::<Redrawn>();
}
