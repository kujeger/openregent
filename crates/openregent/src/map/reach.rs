//! The wizards' guilds' spell reach on the map while a reach spell is
//! aimed (`[enhancements] show_wizards_guild_spell_range`): the minimap's
//! circles (`World::aimed_reach`), laid on the ground. The original shows
//! them on the minimap alone.
//!
//! Under the same setting, on the map and the minimap, while a wizards'
//! guild or tower is placed: the lenders' circles and the preview's
//! (`World::placing_reach`). The original shows none then.
//!
//! ```text
//!  aimed_reach or placing_reach,  ──changed──> one mesh: per circle a
//!  Board::relaid                               faint fill and a brighter rim
//!  no reach spell aimed, no guild or tower placed, or setting off ──> despawned
//! ```

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

use regent_core::gpl::load::Program;
use regent_core::sim::units::PER_CELL;
use regent_core::sim::world::World;
use regent_core::view::panel::minimap::REACH;
use regent_draw::map::Maps;
use regent_draw::map::depth::FURTHEST;
use regent_draw::plain::Plain;

use crate::files::config::Settings;
use crate::game::seen::Watch;
use crate::input::orders::{Mode, Orders};

/// Over the fog's veil (−925), so it shows on hidden ground, which
/// farseeing aims at; under every object.
const DEPTH: f32 = FURTHEST - 10.0;

/// Points about a circle.
const SEGMENTS: u32 = 96;

/// The fill's rings, centre out, so it bends with the ground.
const BANDS: u32 = 12;

/// The rim's width inward, 32nds: a quarter cell.
const RIM: f32 = 8.0;

/// The fill's and rim's opacity. Not the engine's: its minimap circle is
/// one opaque pixel wide.
const FILL_ALPHA: f32 = 0.12;
const RIM_ALPHA: f32 = 0.6;

/// A circle: centre and radius, 32nds.
type Circle = ((i32, i32), i32);

/// What a drawing shows: circles, and the ground's `Board::relaid`.
type Key = (Vec<Circle>, u32);

/// What is drawn, and its entity.
#[derive(Resource, Default)]
pub struct ReachDrawn {
    shown: Option<(Entity, Key)>,
}

impl ReachDrawn {
    /// What is drawn forgotten, to be drawn again over another ground (a
    /// LAN resync's): its entity, to despawn.
    pub fn forget(&mut self) -> Option<Entity> {
        self.shown.take().map(|(entity, _)| entity)
    }
}

/// Whether the reach shows while a wizards' guild or tower is placed:
/// `show_wizards_guild_spell_range`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Placing {
    Shown,
    Hidden,
}

impl Placing {
    pub const fn of(settings: &Settings) -> Self {
        if settings.enhancements.show_wizards_guild_spell_range {
            Placing::Shown
        } else {
            Placing::Hidden
        }
    }
}

/// The circles while the local player places a wizards' guild or tower
/// (`World::placing_reach`), the preview's at its half cell; none in
/// another mode, or with `placing` hidden.
pub fn placing(
    (world, local): (&World, u32),
    program: &Program,
    orders: &Orders,
    placing: Placing,
) -> Vec<Circle> {
    match (orders.mode, placing) {
        (Mode::Build(code), Placing::Shown) => {
            let at = orders.preview.filter(|p| p.code == code).map(|p| p.at);

            world.placing_reach(program, local, code, at)
        }
        _ => Vec::new(),
    }
}

/// Keep the reach drawn while a reach spell is aimed, or a wizards'
/// guild or tower placed; none otherwise.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
#[allow(
    clippy::too_many_arguments,
    reason = "a system's arguments are its dependencies"
)]
pub fn follow(
    mut commands: Commands,
    settings: Res<Settings>,
    orders: Res<Orders>,
    seen: Watch,
    maps: Res<Maps>,
    mut drawn: ResMut<ReachDrawn>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut colours: ResMut<Assets<Plain>>,
) {
    let world = seen.world();
    let circles = match orders.mode {
        Mode::Spell(spell) if settings.enhancements.show_wizards_guild_spell_range => {
            world.aimed_reach(seen.program, seen.local, spell.row.spell)
        }
        _ => placing(
            (world, seen.local),
            seen.program,
            &orders,
            Placing::of(&settings),
        ),
    };
    let wanted = (!circles.is_empty()).then(|| (circles, world.board().relaid()));

    if wanted.as_ref() != drawn.shown.as_ref().map(|(_, key)| key) {
        if let Some((entity, ..)) = drawn.shown.take() {
            commands.entity(entity).despawn();
        }

        drawn.shown = wanted.map(|key| {
            let material = Plain::colour(Color::srgb_u8(REACH.r, REACH.g, REACH.b));
            let entity = commands
                .spawn((
                    Mesh3d(meshes.add(mesh(&maps, &key.0))),
                    MeshMaterial3d(colours.add(material)),
                    Transform::from_xyz(0.0, 0.0, DEPTH),
                ))
                .id();

            (entity, key)
        });
    }
}

/// Every circle as rings of quads on the ground: [`BANDS`] of fill, then
/// the rim; the opacity in the vertex colour.
fn mesh(maps: &Maps, circles: &[Circle]) -> Mesh {
    #[allow(clippy::cast_precision_loss, reason = "a map is at most 512 cells")]
    let side = maps.terrain().map_or(0.0, |t| t.side() as f32);

    #[allow(clippy::cast_precision_loss, reason = "32nds of a 512 cell map")]
    let strips = |&((x, y), radius): &Circle| {
        let centre = Vec2::new(x as f32, y as f32);
        let r = radius as f32;
        let edge = (r - RIM).max(0.0);
        let fill = (0..BANDS).map(move |k| {
            let step = edge / BANDS as f32;

            (centre, step * k as f32, step * (k + 1) as f32, FILL_ALPHA)
        });

        fill.chain(std::iter::once((centre, edge, r, RIM_ALPHA)))
    };

    // A ground point, 32nds, held to the map and lifted to the ground.
    let ground = |p: Vec2| {
        #[allow(clippy::cast_precision_loss, reason = "32 per cell")]
        let cell = (p / PER_CELL as f32).clamp(Vec2::ZERO, Vec2::splat(side));
        let at = maps.standing_at(cell);

        [at.x, at.y, 0.0]
    };

    let mut positions = Vec::new();
    let mut tints = Vec::new();
    let mut indices = Vec::new();

    // A strip: an inner and an outer ring of SEGMENTS points, joined
    // in quads.
    for (centre, inner, outer, alpha) in circles.iter().flat_map(strips) {
        #[allow(clippy::cast_possible_truncation, reason = "a few thousand points")]
        let base = positions.len() as u32;

        #[allow(clippy::cast_precision_loss, reason = "under a hundred")]
        for s in 0..SEGMENTS {
            let dir = Vec2::from_angle(std::f32::consts::TAU * s as f32 / SEGMENTS as f32);

            positions.push(ground(centre + dir * inner));
            positions.push(ground(centre + dir * outer));

            tints.push([1.0, 1.0, 1.0, alpha]);
            tints.push([1.0, 1.0, 1.0, alpha]);
        }

        indices.extend((0..SEGMENTS).flat_map(|s| {
            let (i0, o0) = (base + 2 * s, base + 2 * s + 1);
            let next = (s + 1) % SEGMENTS;
            let (i1, o1) = (base + 2 * next, base + 2 * next + 1);

            [i0, o0, o1, i0, o1, i1]
        }));
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );

    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, tints);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

/// The reach drawn, a game's.
pub fn plugin(app: &mut bevy::prelude::App) {
    use crate::game::scope::Scoped as _;

    app.per_game::<ReachDrawn>();
}
