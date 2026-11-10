//! The unit browser's sprites on screen, kept in step with the selection.
//!
//! Only selected frames are decoded and uploaded, on change; old textures are
//! dropped. One sprite, or one per pose in ring mode (up to 32, `APA2 Ranger
//! Arrow`). Playback re-uploads every frame step: fine for a viewer.

use std::f32::consts::TAU;

use bevy::input::mouse::MouseWheel;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use regent_core::content::imagery::{Animation, FrameSource};
use regent_core::data::imag::Blit;
use regent_core::data::settings::SQUASH;
use regent_core::view::{Backdrop, Sprite as Frame};

use regent_draw::data::Corpus;
use regent_draw::plain::{self, Pictures};
use regent_draw::stage::{BROWSE_ZOOM, View, turned};
use regent_draw::texture;

use crate::units::{Playback, Selection};

/// One drawn frame, and which pose it came from.
///
/// Ring angle and pose offset are reapplied every frame; the texture only on
/// change.
#[derive(Component)]
pub struct Stage {
    pose: usize,
    /// How many poses were placed with it.
    of: usize,
}

/// The magnifications offered as buttons, largest first.
///
/// Whole ratios only: fractional zoom doubles some rows and not others.
pub const STEPS: [(&str, f32); 3] = [("4:1", 4.0), ("2:1", BROWSE_ZOOM), ("1:1", 1.0)];

/// A word anchored in the world and drawn at a fixed size on the screen.
///
/// Painted by egui in the panel pass: a `Text2d` would scale with camera zoom.
#[derive(Clone)]
pub struct Legend {
    /// Where the word belongs, in world units.
    pub at: Vec2,
    pub text: String,
    /// Size in logical points.
    pub size: f32,
}

/// This frame's legends. Cleared in [`First`], refilled in
/// `Update`.
#[derive(Resource, Default)]
pub struct Legends(pub Vec<Legend>);

/// Drop last frame's legends.
pub fn clear_legends(mut legends: ResMut<Legends>) {
    legends.0.clear();
}

/// What the stage shows, to rebuild only on change.
#[derive(Resource, Default)]
pub struct StageShown {
    placed: Vec<(usize, Option<FrameSource>)>,
    /// The widest sprite up; the ring's radius unit.
    extent: f32,
    /// The last resolve's report.
    pub status: String,
    /// The backdrop baked into the textures; a change forces re-upload.
    backdrop: [u8; 3],
}

impl StageShown {
    /// Nothing shown: the next pass puts the selection up again.
    pub fn forget(&mut self) {
        self.placed.clear();
    }
}

/// Step the frame on, while playback is running.
///
/// Moves the slider's value; runs before [`follow_selection`]. Single-frame
/// sets are skipped.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value; that is the contract"
)]
pub fn advance(time: Res<Time>, mut playback: ResMut<Playback>, mut selection: ResMut<Selection>) {
    let frames = selection.frames();

    #[allow(clippy::cast_precision_loss, reason = "at most 500 ms")]
    let step = selection.frame_ms() as f32 / 1000.0;

    if playback.playing && frames > 1 {
        playback.since += time.delta_secs();

        while playback.playing && playback.since >= step {
            playback.since -= step;

            if selection.frame + 1 < frames {
                selection.frame += 1;
            } else if playback.looping {
                selection.frame = 0;
            } else {
                // Done; the play button rewinds.
                playback.playing = false;
            }
        }
    }
}

/// Put the selected frames on the stage, if the set of them has moved.
///
/// Two passes: the ring radius needs the widest sprite, known only after
/// decoding all frames.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value; that is the contract"
)]
#[allow(
    clippy::too_many_arguments,
    reason = "a system's arguments are its dependencies, declared rather than fetched"
)]
pub fn follow_selection(
    mut commands: Commands,
    corpus: Res<Corpus>,
    selection: Res<Selection>,
    mut view_content: ResMut<View>,
    mut shown: ResMut<StageShown>,
    mut legends: ResMut<Legends>,
    (mut images, mut meshes, mut plains): Pictures,
    mut stage: Query<(Entity, &Stage, &mut Transform)>,
) {
    let wanted = selection.placement();

    // Placement runs every frame (spread, offset); decode and upload only on
    // change.
    if wanted == shown.placed && view_content.backdrop == shown.backdrop {
        reposition(&selection, shown.extent, &mut stage);
        slots(&selection, wanted.len(), shown.extent, &mut legends);
        return;
    }

    let (resolved, extent, report) = resolve(&corpus, &wanted);

    for (entity, _, _) in stage.iter() {
        commands.entity(entity).despawn();
    }

    let of = wanted.len();

    // One blit per set: high indices are colours in a portrait, shadow on a
    // walking hero. See `regent_core::data::imag::Blit`.
    let blit = selection.animation().map_or(Blit::World, Animation::blit);

    for (pose, frame) in resolved {
        let Some(image) =
            texture::image(&frame, Backdrop::Flat(view_content.backdrop.into()), blit)
        else {
            continue;
        };

        let anchor = texture::anchor(&frame);
        let at = Stage { pose, of };

        let size = Vec2::new(f32::from(frame.width), f32::from(frame.height));

        commands.spawn((
            // Placed by anchor, not canvas centre, which would make walk
            // cycles wander: a share of the size.
            plain::picture(
                images.add(image),
                (size, anchor / size),
                (&mut meshes, &mut plains),
            ),
            Transform::from_translation(offset_of(&selection, &at, extent).extend(0.0)),
            at,
        ));
    }

    slots(&selection, of, extent, &mut legends);

    // Fit span: one sprite, or the ring plus a sprite.
    let span = if of > 1 {
        extent * (2.0 * selection.spread + 1.0)
    } else {
        extent
    };

    view_content.content = Vec2::splat(span);

    shown.placed = wanted;
    shown.extent = extent;
    shown.status = report;
    shown.backdrop = view_content.backdrop;
}

/// Decode every wanted frame; report failures. Also returns the widest sprite.
fn resolve(
    corpus: &Corpus,
    wanted: &[(usize, Option<FrameSource>)],
) -> (Vec<(usize, Frame)>, f32, String) {
    let mut out = Vec::new();
    let mut tiles = std::collections::BTreeSet::new();
    let mut extent: f32 = 1.0;

    // A set with no poses: explain the empty stage.
    let mut report = if wanted.is_empty() {
        vec!["no frame selected".to_owned()]
    } else {
        Vec::new()
    };

    for &(pose, source) in wanted {
        let Some(FrameSource::Tile { archive, index, .. }) = source else {
            // Name sentinels and missing tiles.
            report.push(match source {
                None => "no frame selected".to_owned(),
                Some(other) => format!("pose {pose} is not a tile: {other:?}"),
            });
            continue;
        };

        tiles.insert((archive, index));

        match Frame::at(&corpus.catalogue, archive, index) {
            Some(Ok(frame)) => {
                if wanted.len() == 1 {
                    report.push(format!(
                        "tile {index}  {}x{}  v{}  {} of {} pixels drawn",
                        frame.width,
                        frame.height,
                        frame.version,
                        frame.drawn(),
                        frame.covered.len(),
                    ));
                }

                if frame.width == 0 || frame.height == 0 {
                    report.push(format!("tile {index} has a zero-sized canvas"));
                    continue;
                }

                extent = extent.max(f32::from(frame.width.max(frame.height)));
                out.push((pose, frame));
            }
            Some(Err(e)) => report.push(format!("tile {index} will not decode: {e}")),
            None => report.push(format!("tile {index} is a padding slot, with no bytes")),
        }
    }

    if wanted.len() > 1 {
        // Fewer tiles than poses: facings share tiles.
        report.insert(
            0,
            format!(
                "ring of {} poses, {} distinct tiles",
                wanted.len(),
                tiles.len()
            ),
        );
    }

    (out, extent, report.join("  |  "))
}

/// Where a pose's anchor sits, in world units.
///
/// The pose offset, plus on the ring a push along the slot's angle (slot 0 up,
/// then clockwise). The ring tests that convention. Offset y is negated.
fn offset_of(selection: &Selection, stage: &Stage, extent: f32) -> Vec2 {
    let (x, y) = if selection.offset {
        selection
            .animation()
            .and_then(|a| a.poses.get(stage.pose))
            .map_or((0, 0), |p| (p.x, p.y))
    } else {
        (0, 0)
    };

    let mut at = Vec2::new(f32::from(x), -f32::from(y));

    if stage.of > 1 {
        at += ring(stage.pose, stage.of, extent * selection.spread);
    }

    at
}

/// The point on the ring for slot `pose` of `of`, at `radius`.
///
/// Slot 0 at +Y, then clockwise: sine for X, cosine for Y.
fn ring(pose: usize, of: usize, radius: f32) -> Vec2 {
    #[allow(
        clippy::cast_precision_loss,
        reason = "a slot count is at most 256; f32 is exact well past that"
    )]
    let turn = TAU * pose as f32 / of as f32;

    Vec2::new(turn.sin(), turn.cos()) * radius
}

/// How big a slot number is drawn, in logical points on screen.
const LABEL_POINTS: f32 = 12.0;

/// How far outside its sprite a slot's label sits, as a share of the radius.
const LABEL_OVERHANG: f32 = 1.32;

/// The ring's slot numbers: outside each sprite, on the same ray.
///
/// Rebuilt every frame; at most 32.
fn slots(selection: &Selection, of: usize, extent: f32, legends: &mut Legends) {
    if of <= 1 {
        return;
    }

    let radius = extent * selection.spread * LABEL_OVERHANG;

    legends.0.extend((0..of).map(|pose| Legend {
        at: ring(pose, of, radius),
        text: pose.to_string(),
        size: LABEL_POINTS,
    }));
}

/// Put everything where the selection says.
fn reposition(
    selection: &Selection,
    extent: f32,
    stage: &mut Query<(Entity, &Stage, &mut Transform)>,
) {
    for (_, at, mut transform) in stage {
        transform.translation = offset_of(selection, at, extent).extend(0.0);
    }
}

/// One colour per attachment bit. Only 0, 1 and 2 are ever set.
///
/// Chosen to stand out on grey stonework.
const BIT_COLOURS: [Color; 3] = [
    Color::srgb(1.0, 0.35, 0.35),
    Color::srgb(0.4, 1.0, 0.45),
    Color::srgb(0.4, 0.7, 1.0),
];

/// Radius of an attachment mark, in logical points on screen.
///
/// Divided by zoom to stay constant on screen.
const MARK: f32 = 6.0;

/// Mark the tile's anchor: where the thing meets the ground.
///
/// Checks the reading in `docs/formats/art.md`: the mark sits under a hero's feet,
/// and 39 rows below the Harpy's (flight height).
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value; that is the contract"
)]
pub fn anchor(
    mut gizmos: Gizmos,
    selection: Res<Selection>,
    view: Res<View>,
    shown: Res<StageShown>,
    stage: Query<&Stage>,
) {
    if !selection.anchor {
        return;
    }

    let mark = MARK / view.zoom;

    for at in &stage {
        // Sprites are placed by anchor; nothing to add.
        let world = offset_of(&selection, at, shown.extent);

        // Flat ellipse (ground, squashed like a map cell) and a saltire.
        gizmos.ellipse_2d(world, Vec2::new(mark * 2.0, mark * 2.0 * SQUASH), ANCHOR);
        gizmos.cross_2d(
            Isometry2d::new(world, Rot2::degrees(45.0)),
            mark * 1.5,
            ANCHOR,
        );
    }
}

/// The anchor mark's colour.
const ANCHOR: Color = Color::srgb(0.3, 1.0, 1.0);

/// Mark every drawn pose's attachment points.
///
/// Gizmos, not entities. A point is measured from the tile's anchor
/// (**Viewer**: the Guardhouse's six firing points land one per face).
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value; that is the contract"
)]
pub fn points(
    mut gizmos: Gizmos,
    selection: Res<Selection>,
    view: Res<View>,
    shown: Res<StageShown>,
    stage: Query<&Stage>,
) {
    if !selection.points {
        return;
    }

    let mark = MARK / view.zoom;

    for at in &stage {
        let Some(pose) = selection.animation().and_then(|a| a.poses.get(at.pose)) else {
            continue;
        };

        let centre = offset_of(&selection, at, shown.extent);

        for &(bit, block) in &pose.attachments {
            let colour = BIT_COLOURS
                .get(bit as usize)
                .copied()
                .unwrap_or(Color::WHITE);

            // A block is two points. Rows count down; negate y.
            for [x, y] in [[block[0], block[1]], [block[2], block[3]]] {
                let world = centre + Vec2::new(f32::from(x), -f32::from(y));

                gizmos.circle_2d(world, mark, colour);
                gizmos.cross_2d(world, mark * 2.0, colour);
            }
        }
    }
}

/// How much one wheel notch changes the magnification.
///
/// Multiplicative, so it feels the same at any zoom.
const ZOOM_PER_NOTCH: f32 = 1.15;

/// Zoom on the wheel, but only over the picture.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value; that is the contract"
)]
pub fn zoom(
    mut wheel: MessageReader<MouseWheel>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut view: ResMut<View>,
) {
    if let Some((notches, cursor)) = turned(&mut wheel, &window, &view) {
        view.magnify(ZOOM_PER_NOTCH.powf(notches), Some(cursor));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    /// Leaving Units forgets what the stage showed: back with the same
    /// selection, the sprites go up again.
    #[test]
    fn leaving_units_forgets_the_stage() {
        let mut world = World::new();

        world.insert_resource(StageShown {
            placed: vec![(0, None)],
            ..StageShown::default()
        });
        world.run_system_once(crate::clear_stage).expect("runs");

        assert_eq!(world.resource::<StageShown>().placed.len(), 0);
    }
}
