//! The renderer's fragments taken apart (`MAJ_PERF_HIDE`), with
//! `MAJ_PERF`: classes of world meshes hidden, so a run's `frag_` counts
//! differ by what they drew; and each [`WINDOW`] frames, the screen area
//! the main view's bodies and bands cover, against the pixels their art
//! draws.
//!
//! ```text
//!  MAJ_PERF_HIDE=floor,bands   hide the floor and every band
//!  classes  floor     the whole map, low, under the patches (map::floor)
//!           ground    the patches (map::patch)
//!           inset     the tracking window's own ground
//!           veil      the fog
//!           bands     shadows
//!           things    opaque bodies of the map's scatter (map::things::Thing)
//!           pieces    other opaque bodies: units, buildings, effects
//!           glass     translucent bodies; lift: their twins
//!           plain     other Plain meshes: bars, reach, direct-colour frames
//!           rings     selection rings
//!           tracking  the tracking window's camera off
//!  series   quad_<kind>   texels of the world image the main view's quads cover
//!           art_<kind>    of those, the share their art draws, times the area
//! ```
//!
//! **Inferred**: a quad partly off screen draws the share of its art
//! the whole quad does.

use std::collections::HashMap;

use bevy::camera::visibility::{RenderLayers, VisibilitySystems};
use bevy::prelude::*;
use regent_core::view::raster::shade::{CONTROL_FIRST, TRANSPARENT};
use regent_draw::map::redraw::{GroundSheet, INSET_LAYER};
use regent_draw::map::things::Thing;
use regent_draw::material::{Band, Body, Pass};
use regent_draw::plain::{Lay, Plain};
use regent_draw::present::texel;
use regent_draw::ring::Ellipse;
use regent_draw::stage::View;
use regent_draw::veil::Veil;

use super::{WINDOW, named};

/// The environment variable naming the classes to hide, e.g.
/// `MAJ_PERF_HIDE=floor,bands`.
pub const ENV: &str = "MAJ_PERF_HIDE";

/// A class of world meshes [`ENV`] can hide.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Class {
    Floor,
    Ground,
    Inset,
    Veil,
    Bands,
    Things,
    Pieces,
    Glass,
    Lift,
    Plain,
    Rings,
    Tracking,
}

impl Class {
    fn named(name: &str) -> Option<Class> {
        match name.trim() {
            "floor" => Some(Class::Floor),
            "ground" => Some(Class::Ground),
            "inset" => Some(Class::Inset),
            "veil" => Some(Class::Veil),
            "bands" => Some(Class::Bands),
            "things" => Some(Class::Things),
            "pieces" => Some(Class::Pieces),
            "glass" => Some(Class::Glass),
            "lift" => Some(Class::Lift),
            "plain" => Some(Class::Plain),
            "rings" => Some(Class::Rings),
            "tracking" => Some(Class::Tracking),
            _ => None,
        }
    }
}

/// The classes [`ENV`] names.
#[derive(Resource, Default)]
struct Hidden(Vec<Class>);

impl Hidden {
    fn has(&self, class: Class) -> bool {
        self.0.contains(&class)
    }
}

/// With `MAJ_PERF` on: the hiding, when [`ENV`] names a class, and the
/// coverage series always.
pub fn install(app: &mut App) {
    let classes: Vec<Class> = std::env::var(ENV)
        .unwrap_or_default()
        .split(',')
        .filter(|name| !name.trim().is_empty())
        .filter_map(|name| {
            let class = Class::named(name);

            if class.is_none() {
                warn!(target: super::TARGET, "{ENV}: no class {name}");
            }

            class
        })
        .collect();

    app.insert_resource(Hidden(classes))
        .add_systems(
            PostUpdate,
            (hide, tracking).before(VisibilitySystems::VisibilityPropagate),
        )
        .add_systems(Last, covered.before(super::ended));
}

/// The hidden classes' meshes hidden, each frame: the game's own systems
/// show them again (animation cycles, the fog).
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
#[allow(clippy::type_complexity, reason = "a query names what it reads")]
fn hide(
    hidden: Res<Hidden>,
    (bodies, plains): (Res<Assets<Body>>, Res<Assets<Plain>>),
    mut meshes: Query<
        (
            &mut Visibility,
            (
                Option<&MeshMaterial3d<Body>>,
                Option<&MeshMaterial3d<Plain>>,
            ),
            (
                Has<MeshMaterial3d<Band>>,
                Has<MeshMaterial3d<Veil>>,
                Has<MeshMaterial3d<Ellipse>>,
            ),
            (Has<GroundSheet>, Has<Thing>, Option<&RenderLayers>),
        ),
        With<Mesh3d>,
    >,
) {
    if hidden.0.is_empty() {
        return;
    }

    for (mut visibility, (body, plain), (band, veil, ring), (sheet, thing, layers)) in &mut meshes {
        let pass = body.and_then(|m| bodies.get(&m.0)).map(|b| b.pass);
        let ground = plain
            .and_then(|m| plains.get(&m.0))
            .is_some_and(|p| p.lay == Lay::Ground);
        let inset = layers.is_some_and(|l| l.intersects(&RenderLayers::layer(INSET_LAYER)));

        // Sharp-sprite's patches are a `smooth::Sheet`, not a Plain.
        let class = if sheet {
            Some(Class::Ground)
        } else if ground {
            Some(if inset { Class::Inset } else { Class::Floor })
        } else if let Some(pass) = pass {
            Some(match pass {
                Pass::Glass => Class::Glass,
                Pass::Lift => Class::Lift,
                Pass::Solid if thing => Class::Things,
                Pass::Solid => Class::Pieces,
            })
        } else if band {
            Some(Class::Bands)
        } else if veil {
            Some(Class::Veil)
        } else if ring {
            Some(Class::Rings)
        } else {
            plain.map(|_| Class::Plain)
        };

        put(&mut visibility, class.is_some_and(|c| hidden.has(c)));
    }
}

/// `visibility` hidden when `hide`, written only then.
fn put(visibility: &mut Visibility, hide: bool) {
    if hide && *visibility != Visibility::Hidden {
        *visibility = Visibility::Hidden;
    }
}

/// The tracking window's camera off, when hidden.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
fn tracking(
    hidden: Res<Hidden>,
    mut cameras: Query<&mut Camera, With<crate::ui::tracking::Camera>>,
) {
    if hidden.has(Class::Tracking) {
        for mut camera in &mut cameras {
            camera.is_active = false;
        }
    }
}

/// An index picture's pixels: all, the ones a body draws (a colour) and
/// the ones a band draws (an operation).
#[derive(Clone, Copy, Default)]
struct Pixels {
    all: u32,
    colour: u32,
    operation: u32,
}

impl Pixels {
    fn of(image: &Image) -> Option<Pixels> {
        let data = image.data.as_ref()?;
        let colour = data.iter().filter(|i| **i < CONTROL_FIRST).count();
        let operation = data
            .iter()
            .filter(|i| (CONTROL_FIRST..TRANSPARENT).contains(*i))
            .count();

        Some(Pixels {
            all: u32::try_from(data.len()).ok()?,
            colour: u32::try_from(colour).ok()?,
            operation: u32::try_from(operation).ok()?,
        })
    }
}

/// Each [`WINDOW`]th frame, the main view's quads: texels of the world
/// image covered, by kind, and the share their art draws.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
#[allow(clippy::type_complexity, reason = "a query names what it reads")]
#[allow(
    clippy::too_many_arguments,
    reason = "a system's arguments are its dependencies"
)]
fn covered(
    view: Res<View>,
    window: Option<Single<&Window, With<bevy::window::PrimaryWindow>>>,
    images: Res<Assets<Image>>,
    (bodies, bands): (Res<Assets<Body>>, Res<Assets<Band>>),
    with_body: Query<(
        &ViewVisibility,
        &GlobalTransform,
        &MeshMaterial3d<Body>,
        Has<Thing>,
    )>,
    with_band: Query<(&ViewVisibility, &GlobalTransform, &MeshMaterial3d<Band>)>,
    mut frames: Local<u32>,
    mut known: Local<HashMap<AssetId<Image>, Pixels>>,
) {
    *frames += 1;

    let (Some(seen), Some(window)) = (view.seen(), window) else {
        return;
    };

    if !frames.is_multiple_of(WINDOW) {
        return;
    }

    let side = texel(view.zoom, window.scale_factor());
    let mut pixels = |id: AssetId<Image>| {
        *known
            .entry(id)
            .or_insert_with(|| images.get(id).and_then(Pixels::of).unwrap_or_default())
    };

    // Texels of a unit quad scaled by `at` on screen.
    let area = |at: &GlobalTransform| {
        let (scale, _, middle) = at.to_scale_rotation_translation();
        let quad = Rect::from_center_size(middle.truncate(), scale.truncate().abs());
        let on = quad.intersect(seen);

        if on.is_empty() {
            0.0
        } else {
            on.size().x * on.size().y / (side * side)
        }
    };

    let mut sums: HashMap<&str, (f64, f64)> = HashMap::new();
    let mut add = |kind: &'static str, texels: f32, drawn: f64| {
        let sum = sums.entry(kind).or_default();

        sum.0 += f64::from(texels);
        sum.1 += drawn * f64::from(texels);
    };

    for (seen, at, material, thing) in &with_body {
        if let (true, Some(body)) = (seen.get(), bodies.get(&material.0)) {
            let p = pixels(body.indices.id());
            let share = f64::from(p.colour) / f64::from(p.all.max(1));
            let kind = match (body.pass, thing) {
                (Pass::Glass, _) => "glass",
                (Pass::Lift, _) => "lift",
                (Pass::Solid, true) => "things",
                (Pass::Solid, false) => "pieces",
            };

            add(kind, area(at), share);
        }
    }

    for (seen, at, material) in &with_band {
        if let (true, Some(band)) = (seen.get(), bands.get(&material.0)) {
            let p = pixels(band.indices.id());
            let share = f64::from(p.operation) / f64::from(p.all.max(1));

            add("bands", area(at), share);
        }
    }

    for (kind, (quad, art)) in sums {
        named(&format!("quad_{kind}"), quad);
        named(&format!("art_{kind}"), art);
    }
}
