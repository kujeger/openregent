//! The buildings that cast from a model (`[enhancements] cast =
//! "models"`, `regent_draw::sun`): each finished one with a model, shown,
//! handed to the bake; its painted shadow dropped (each band frame's a
//! copy without its ground shadow: the layer's [`Own`]), and its bodies
//! kept from its own model's shadow.
//!
//! ```text
//!  O   a debug key ([enhancements] debug_keys; a cheat), while models cast: their
//!      buildings' sprites hidden, their models drawn plain in their place
//!      (`regent_draw::sun::bare`), or the sprites drawn again
//! ```

use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;

use regent_draw::lie::Footing;
use regent_draw::map::Maps;
use regent_draw::map::reel::{Own, Role, Stepped};
use regent_draw::material::{Band, Painted};
use regent_draw::sun::bare::Sprites;
use regent_draw::sun::model::Proxies;
use regent_draw::sun::{Casters, Casting, Casts, Sun};

use regent_core::sim::world::cheat::Cheats;

use crate::files::config::Settings;
use crate::game::seen::Watch;
use crate::map::pieces::{Keyed, Piece};

/// On a body [`casting`] hid: on no render layer, so no camera draws it,
/// its frames left stepping.
#[derive(Component)]
pub(crate) struct Unshown;

/// O hides the sprites of the buildings casting from a model, or draws
/// them again, with `[enhancements] debug_keys`, while models cast: a
/// cheat, as every debug key (it shows what stands behind them).
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
pub(crate) fn key(
    keys: Res<ButtonInput<KeyCode>>,
    mut live: crate::input::map_keys::Live,
    (settings, sun): (Res<Settings>, Res<Sun>),
    seen: Watch,
    mut sprites: ResMut<Sprites>,
) {
    if keys.just_pressed(KeyCode::KeyO)
        && settings.enhancements.debug_keys
        && seen.world().cheats() == Cheats::Allowed
        && sun.models()
        && live.keys()
    {
        *sprites = sprites.toggled();
    }
}

/// What [`casting`] reads of a piece.
type Drawn = (
    Entity,
    &'static Keyed,
    &'static Footing,
    &'static Transform,
    &'static Visibility,
    Option<&'static Children>,
    Has<Casts>,
);

/// The casters, once a frame, after the pieces are placed: each one's
/// bands made over or not, its bodies hidden or not.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
pub(crate) fn casting(
    mut commands: Commands,
    (sun, proxies, maps): (Res<Sun>, Res<Proxies>, Res<Maps>),
    pieces: Query<Drawn, With<Piece>>,
    mut parts: Query<(Stepped, Has<Unshown>), Without<Piece>>,
    sprites: Res<Sprites>,
    mut materials: ResMut<Assets<Band>>,
    mut casters: ResMut<Casters>,
) {
    let mut now = Vec::new();

    for (entity, keyed, footing, put, seen, children, had) in &pieces {
        let (code, set, _) = keyed.0;
        let casts = sun.models() && *seen != Visibility::Hidden && proxies.of(code, set).is_some();

        if casts {
            let flat = maps.flat_y(footing.row);

            now.push(Casting {
                piece: entity,
                code,
                set,
                at: Vec2::new(put.translation.x, flat),
                lift: footing.ground - flat,
                depth: put.translation.z,
            });
        }

        match (casts, had) {
            (true, false) => {
                commands.entity(entity).insert(Casts);
            }
            (false, true) => {
                commands.entity(entity).remove::<Casts>();
            }
            _ => {}
        }

        // Its bands' darkening drawn, or dropped for the model's; its
        // sprite hidden by the debug key, or drawn.
        let hide = casts && *sprites == Sprites::Hidden;

        for child in children.into_iter().flatten() {
            let Ok((mut layer, unshown)) = parts.get_mut(*child) else {
                continue;
            };

            match (layer.layer.role, casts, layer.own.1.is_some()) {
                (Role::Band, true, false) => {
                    let own = Own(layer
                        .layer
                        .cels()
                        .iter()
                        .map(|cel| dropped(cel.as_ref()?, &mut materials))
                        .collect());

                    layer.repaint((None, Some(&own)), &mut commands);
                    commands.entity(*child).insert(own);
                }
                (Role::Band, false, true) => {
                    layer.repaint((None, None), &mut commands);
                    commands.entity(*child).remove::<Own<Band>>();
                }
                _ => {}
            }

            match (layer.layer.role, hide, unshown) {
                (Role::Body, true, false) => {
                    commands
                        .entity(*child)
                        .insert((Unshown, RenderLayers::none()));
                }
                (Role::Body, false, true) => {
                    commands.entity(*child).remove::<(Unshown, RenderLayers)>();
                }
                _ => {}
            }
        }
    }

    casters.set_if_neq(Casters(now));
}

/// Frame `cel`'s band while a model casts: a copy of its own, drawing a
/// part's darkening alone ([`Cel::aloft`](regent_draw::map::cel::Cel::aloft)),
/// else none of the ground's. Bands are shared by a page's frames and
/// the placing preview draws a finished building with no model behind
/// it, so the band itself is left alone.
fn dropped(cel: &regent_draw::map::cel::Cel, materials: &mut Assets<Band>) -> Option<Handle<Band>> {
    let copy = materials.get(cel.band.as_ref()?)?.clone();
    let made = match &cel.aloft {
        Some(aloft) => Band {
            indices: aloft.indices.clone(),
            feet: aloft.feet.clone(),
            on: aloft.on,
            ..copy
        },
        None => Band {
            painted: Painted::Dropped,
            ..copy
        },
    };

    Some(materials.add(made))
}

#[cfg(test)]
mod tests {
    use bevy::ecs::system::RunSystemOnce;

    use regent_core::data::imag::SetKind;
    use regent_core::data::tile::Mode;
    use regent_draw::lie;
    use regent_draw::material::On;
    use regent_draw::shadow::Shadows;
    use regent_draw::smooth::Sampling;
    use regent_draw::soft::Ramps;
    use regent_draw::sun::Cast;
    use regent_draw::wind::{Gust, Sway};

    use std::sync::Arc;

    use regent_draw::instance::Framed;
    use regent_draw::map::cel::{Aloft, Cel, Picture};
    use regent_draw::map::reel::{Beat, Layer, dress};

    use super::*;

    use crate::testdata::{GUARDHOUSE, proxies};

    /// A piece of the guardhouse drawing `set`, with a band layer of one
    /// frame, drawing `band`, its part's darkening `aloft`.
    fn piece(
        world: &mut World,
        (agent, set): (u32, SetKind),
        (band, aloft): (Handle<Band>, Option<Aloft>),
    ) -> Entity {
        let set = set.id().try_into().expect("small");
        let cel = Cel {
            body: Picture::Indexed(Handle::default()),
            band: Some(band),
            aloft,
            lift: None,
            size: Vec2::ONE,
            anchor: Vec2::ZERO,
            tile: (0, 0),
            frame: Framed::default(),
        };
        let layer = Layer::new(
            Arc::from([Some(cel)]),
            Role::Band,
            Beat::runs(0, 1),
            Vec2::ZERO,
        );
        let piece = world
            .spawn((
                Piece(agent),
                Keyed((GUARDHOUSE, set, 0)),
                Footing {
                    row: 0.0,
                    ground: 0.0,
                },
                Transform::default(),
                Visibility::Inherited,
            ))
            .id();
        let mut commands = world.commands();
        let mut part = commands.spawn(ChildOf(piece));

        dress(&mut part, layer, 0, 0.0);
        world.flush();
        piece
    }

    /// A band of the guardhouse, its tile drawn from `indices`.
    fn band(indices: Handle<Image>) -> Band {
        Band {
            indices,
            rows: lie::ROWS,
            shadows: Shadows::default(),
            ramps: Ramps::of(Mode::Band),
            sway: Sway::Still,
            gust: Gust::STILL,
            feet: Handle::default(),
            sampling: Sampling::Nearest,
            painted: Painted::Kept,
            table: regent_draw::instance::TABLE,
            on: On::Own,
        }
    }

    /// What `piece`'s band draws.
    fn painted(world: &World, piece: Entity) -> Painted {
        let child = world.get::<Children>(piece).expect("a band")[0];
        let band = world.get::<MeshMaterial3d<Band>>(child).expect("a band");

        world
            .resource::<Assets<Band>>()
            .get(&band.0)
            .expect("made")
            .painted
    }

    /// Two buildings share a tile's band, one in a set that does not cast.
    /// It keeps its painted shadow, whichever the query reaches last.
    #[test]
    fn a_building_not_casting_keeps_its_painted_shadow_beside_one_casting() {
        for damaged_first in [true, false] {
            let mut app = App::new();

            app.insert_resource(Sun {
                cast: Cast::Models,
                ..Sun::default()
            })
            .insert_resource(Maps::with(Vec::new()))
            .insert_resource(proxies())
            .init_resource::<Sprites>()
            .init_resource::<Casters>()
            .init_resource::<Assets<Band>>();

            let band = app.world_mut().resource_mut::<Assets<Band>>().add(Band {
                indices: Handle::default(),
                rows: lie::ROWS,
                shadows: Shadows::default(),
                ramps: Ramps::of(Mode::Band),
                sway: Sway::Still,
                gust: Gust::STILL,
                feet: Handle::default(),
                sampling: Sampling::Nearest,
                painted: Painted::Kept,
                table: regent_draw::instance::TABLE,
                on: On::Own,
            });
            let world = app.world_mut();

            let (damaged, finished) = if damaged_first {
                let damaged = piece(world, (1, SetKind::Build), (band.clone(), None));

                (damaged, piece(world, (2, SetKind::Inactive), (band, None)))
            } else {
                let finished = piece(world, (1, SetKind::Inactive), (band.clone(), None));

                (piece(world, (2, SetKind::Build), (band, None)), finished)
            };

            world.run_system_once(casting).expect("ran");
            world.flush();

            assert_eq!(painted(world, damaged), Painted::Kept, "{damaged_first}");
            assert_eq!(
                painted(world, finished),
                Painted::Dropped,
                "{damaged_first}"
            );

            // No longer casting, it goes back to the tile's band.
            *world.get_mut::<Visibility>(finished).expect("placed") = Visibility::Hidden;
            world.run_system_once(casting).expect("ran");
            world.flush();

            assert_eq!(painted(world, finished), Painted::Kept, "{damaged_first}");
        }
    }

    /// A flag lies in the tile with the ground shadow (the level 2
    /// Guardhouse's). While a model casts its band draws the tile without
    /// the shadow, darkening kept, not nothing; then its own again.
    #[test]
    fn a_casting_band_keeps_its_parts_darkening() {
        let mut app = App::new();

        app.insert_resource(Sun {
            cast: Cast::Models,
            ..Sun::default()
        })
        .insert_resource(Maps::with(Vec::new()))
        .insert_resource(proxies())
        .init_resource::<Sprites>()
        .init_resource::<Casters>()
        .init_resource::<Assets<Band>>();

        let mut images = Assets::<Image>::default();
        let tile = images.add(Image::default());
        let flag = images.add(Image::default());
        let shared = app
            .world_mut()
            .resource_mut::<Assets<Band>>()
            .add(band(tile.clone()));
        let world = app.world_mut();
        let aloft = Aloft {
            indices: flag.clone(),
            feet: Handle::default(),
            on: On::Own,
        };
        let building = piece(world, (1, SetKind::Inactive), (shared, Some(aloft)));
        let child = world.get::<Children>(building).expect("a band")[0];

        world.run_system_once(casting).expect("ran");
        world.flush();

        let drawn = |world: &World| {
            let handle = &world.get::<MeshMaterial3d<Band>>(child).expect("a band").0;
            let band = world.resource::<Assets<Band>>().get(handle).expect("made");

            (band.indices.clone(), band.painted)
        };

        assert_eq!(drawn(world), (flag, Painted::Kept));

        *world.get_mut::<Visibility>(building).expect("placed") = Visibility::Hidden;
        world.run_system_once(casting).expect("ran");
        world.flush();

        assert_eq!(drawn(world), (tile, Painted::Kept));
    }
}
