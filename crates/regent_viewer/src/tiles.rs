//! The tileset browser: every set as a labelled sheet. Body sets as their
//! `frames x 4` grid (null markers show as gaps, group columns full); blend
//! sets as seventeen rows, one per case.
//!
//! One texture per sheet (`atlas::tile`, [`grid::compose`]): separate
//! sprites at fractional zoom round independently, leaving gaps.

use bevy::prelude::*;

use regent_core::content::terrain::Tileset;
use regent_core::view::{Sheet, Sprite as Frame};

use regent_draw::plain::{self, Pictures};
use regent_draw::stage::View;
use regent_draw::terrain::{CELL, Drawn, TileArt};
use regent_draw::texture;

use crate::grid;
use crate::sprites::{Legend, Legends};

/// How far apart the browser's sheets sit, in tile pixels.
const SHEET_GAP: f32 = 24.0;

/// Gap between cells in a browser sheet, so slot boundaries are visible.
const BROWSE_GAP: u16 = 2;

/// Which view is up.
#[derive(States, Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Subject {
    /// The unit browser.
    #[default]
    Units,
    /// Every set of the tileset, laid out and labelled.
    Tileset,
}

impl Subject {
    pub const ALL: [(Subject, &'static str); 2] =
        [(Subject::Units, "units"), (Subject::Tileset, "tileset")];
}

/// What the browser is showing: everything, or one set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Showing {
    All,
    One(usize),
}

/// The browser's state over [`TileArt`]'s tiles.
#[derive(Resource)]
pub struct Tiles {
    /// Which set the browser is showing.
    pub set: Showing,
    /// What it last put up, so it does not rebuild without cause.
    browsed: Option<Showing>,
    /// Browser headings, to refill the per-frame legend list.
    captions: Vec<Legend>,
}

impl Default for Tiles {
    fn default() -> Self {
        Tiles {
            set: Showing::All,
            browsed: None,
            captions: Vec::new(),
        }
    }
}

impl Tiles {
    /// Force a rebuild. Called on leaving a subject.
    pub fn forget(&mut self) {
        self.browsed = None;
        self.captions.clear();
    }
}

/// Every set the browser can show, as headings for its list.
pub fn sets(ground: &TileArt) -> Vec<String> {
    ground.tileset.as_ref().map_or_else(Vec::new, |t| {
        let bodies = t
            .bodies
            .iter()
            .map(|b| format!("{0}to{0}  {1}", letter(b.letter), ground.name(b.letter)));
        let blends = t.blends.iter().map(|b| {
            format!(
                "{}to{}  {} over {}",
                letter(b.from),
                letter(b.to),
                ground.name(b.to),
                ground.name(b.from)
            )
        });

        bodies.chain(blends).collect()
    })
}

/// Heading size in screen points. An egui [`Legend`], so it does not scale
/// with zoom.
const CAPTION_POINTS: f32 = 13.0;

/// Every set of the tileset, laid out as labelled sheets.
///
/// Body sets as the full `frames * streams` table, so null markers show as
/// gaps. Blend sets as one row per case.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value; that is the contract"
)]
#[allow(
    clippy::too_many_arguments,
    reason = "a system's arguments are its dependencies, declared rather than fetched"
)]
pub fn browse(
    mut commands: Commands,
    ground: Res<TileArt>,
    mut tiles: ResMut<Tiles>,
    mut view: ResMut<View>,
    mut shown: ResMut<regent_draw::terrain::MapShown>,
    mut legends: ResMut<Legends>,
    (mut images, mut meshes, mut plains): Pictures,
    existing: Query<Entity, With<Drawn>>,
) {
    // The legend list is cleared every frame, so always refill captions.
    if tiles.browsed == Some(tiles.set) {
        legends.0.extend(tiles.captions.iter().cloned());
        return;
    }

    for entity in &existing {
        commands.entity(entity).despawn();
    }

    let Some(tileset) = &ground.tileset else {
        tiles.browsed = Some(tiles.set);
        tiles.captions.clear();
        "no tileset in this corpus".clone_into(&mut shown.status);
        return;
    };

    let mut sheets = compose_sets(&ground, tileset);

    if let Showing::One(only) = tiles.set {
        sheets = sheets.into_iter().skip(only).take(1).collect();
    }

    let total = sheets.len();

    // Measure first, to centre the column on the origin.
    #[allow(
        clippy::cast_precision_loss,
        reason = "a sheet is a few thousand pixels; f32 is exact well past that"
    )]
    let extent = sheets.iter().fold(Vec2::ZERO, |acc, (_, s)| {
        Vec2::new(
            acc.x.max(s.width as f32),
            acc.y + s.height as f32 + SHEET_GAP,
        )
    });
    let content = Vec2::new(extent.x, (extent.y - SHEET_GAP).max(0.0));
    let corner = -content / 2.0;

    let mut y = 0.0;
    let mut captions = Vec::new();

    for (caption, sheet) in sheets {
        let Some(image) = texture::sheet(&sheet) else {
            continue;
        };

        #[allow(
            clippy::cast_precision_loss,
            reason = "a sheet is a few thousand pixels; f32 is exact well past that"
        )]
        let (w, h) = (sheet.width as f32, sheet.height as f32);

        // Shared left edge, so slot columns line up.
        let art = Vec2::new(corner.x + w / 2.0, -corner.y - y - h / 2.0);
        let label = Vec2::new(corner.x + w / 2.0, -corner.y - y + SHEET_GAP / 2.0);

        commands.spawn((
            Drawn,
            plain::picture(
                images.add(image),
                (Vec2::new(w, h), Vec2::ZERO),
                (&mut meshes, &mut plains),
            ),
            Transform::from_translation(art.extend(0.0)),
        ));

        captions.push(Legend {
            at: label,
            text: caption,
            size: CAPTION_POINTS,
        });

        y += h + SHEET_GAP;
    }

    view.content = content;
    tiles.browsed = Some(tiles.set);
    shown.status = format!("{total} sets, {} x {} pixels", extent.x, content.y);

    legends.0.extend(captions.iter().cloned());
    tiles.captions = captions;
}

/// Compose every set of the tileset into a labelled sheet.
fn compose_sets(ground: &TileArt, tileset: &Tileset<'static>) -> Vec<(String, Sheet)> {
    let mut sheets: Vec<(String, Sheet)> = Vec::new();

    for body in &tileset.bodies {
        let cells: Vec<Option<&Frame>> = body.grid.iter().map(|f| ground.tile(f)).collect();
        let columns = usize::from(body.frames).max(1);

        sheets.push((
            format!(
                "MapTexture{0}to{0}  {1}  {2} frames x {3} streams, {4} variants, {5} groups",
                letter(body.letter),
                ground.name(body.letter),
                body.frames,
                body.streams,
                body.variants.len(),
                body.groups.len(),
            ),
            grid::compose(
                &cells,
                grid::Grid {
                    columns,
                    cell: CELL,
                    gap: BROWSE_GAP,
                },
            ),
        ));
    }

    for blend in &tileset.blends {
        let widest = blend
            .cases
            .iter()
            .map(|c| c.variants.len())
            .max()
            .unwrap_or(0);
        let mut cells: Vec<Option<&Frame>> = Vec::with_capacity(blend.cases.len() * widest);

        // A row per case, padded so short cases show gaps.
        for case in &blend.cases {
            for i in 0..widest {
                cells.push(case.variants.get(i).and_then(|f| ground.tile(f)));
            }
        }

        sheets.push((
            format!(
                "MapTexture{}to{}  {} over {}  {} cases",
                letter(blend.from),
                letter(blend.to),
                ground.name(blend.to),
                ground.name(blend.from),
                blend.cases.len(),
            ),
            grid::compose(
                &cells,
                grid::Grid {
                    columns: widest.max(1),
                    cell: CELL,
                    gap: BROWSE_GAP,
                },
            ),
        ));
    }

    sheets
}

/// The `MapTexture` letter as the cross reference spells it.
pub fn letter(n: u8) -> char {
    char::from(b'A' + n)
}
