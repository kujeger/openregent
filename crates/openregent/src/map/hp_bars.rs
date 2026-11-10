//! The hit point bars over the map's units ([`regent_core::view::map::bar`]).
//!
//! ```text
//!  each drawn piece ──> its descriptor's rect, length and HasHPBar, made once
//!    view::map::bar::shown, and kept by [ui] bars ──> a bar at its body: frame, two shades, fill
//!    else, held or unseen ──> hidden
//! ```
//!
//! Alt held shows every bar as `always` does, under any `[ui] bars`
//! ([`Reveal`]): unhurt units show none under `damaged`, and the bar's
//! colour tells sides apart.
//!
//! The engine draws a unit's bar with the unit, at its
//! body's screen place, right after its picture in the draw list
//! (**Binary**: `IsoGameView` passes part 0 as -1;
//! no later pass draws units): a nearer unit covers it. Here in front of
//! every piece instead, ordered among themselves as the draw list orders
//! units: board y, then agent ([`Tie`]).
//!
//! Each part is both a sprite and a 3D quad of one colour: the 3D world
//! camera (nearest, and the tracking window) draws the quad before the
//! shadows' resolve, as it drew the sprite; the 2D shown camera (the
//! scaled modes) draws the sprite over the scaled world.

use std::collections::HashMap;

use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;

use regent_core::content::unit::Unit;
use regent_core::data::attrib;
use regent_core::data::attrib::{HP, MAX_HP};
use regent_core::data::fourcc::FourCc;
use regent_core::sim::world::World;
use regent_core::view::map::bar::{self, Asked, Body, Stats};
use regent_core::view::map::ring;
use regent_core::view::panel::minimap::Looks;
use regent_core::view::panel::portrait::{self, Health};
use regent_core::view::raster::colour::quantise;
use regent_draw::data::Corpus;
use regent_draw::map::depth::NEAREST;
use regent_draw::plain::Plain;
use regent_draw::present::OVERLAY;
use regent_draw::soft::{self, Blending};

use crate::files::config::{HpBars, Settings};
use crate::game::seen::Watch;
use crate::map::mirror::Pieces;
use crate::map::pieces::Piece;
use crate::map::words::Tie;

/// Where the bars draw from: in front of every piece ([`NEAREST`]).
const BAR_Z: f32 = NEAREST + 1.0;

/// How far in front of the frame each later part draws.
const PART_STEP: f32 = 0.001;

/// How far in front of the bar before it each bar draws: past its parts.
const BAR_STEP: f32 = 4.0 * PART_STEP;

/// A descriptor's bar: its rect and length, whether it has one, and its
/// most hit points when the piece keeps none.
#[derive(Clone, Copy)]
struct Sized {
    rect: [i32; 4],
    body: Body,
    has_bar: bool,
    most: Option<i32>,
}

impl Sized {
    fn of(corpus: &Corpus, code: FourCc) -> Option<Sized> {
        let unit = Unit::find(&corpus.catalogue, code)?.ok()?;
        let (rect, body) = bar::sized(&corpus.catalogue, &unit).ok()??;

        Some(Sized {
            rect,
            body,
            has_bar: bar::has_bar(&unit),
            most: portrait::most_hp(&unit.desc),
        })
    }
}

/// The bars drawn, by agent; each descriptor's size, made once.
#[derive(Resource, Default)]
pub struct Bars {
    drawn: HashMap<u32, Entity>,
    /// The unit quad every part's 3D twin is drawn on.
    quad: Option<Handle<Mesh>>,
    sized: HashMap<FourCc, Option<Sized>>,
    looks: Option<Looks>,
}

impl crate::draw::shelf::Shelved for Bars {
    fn kept(self) -> Self {
        Bars {
            drawn: HashMap::new(),
            ..self
        }
    }
}

/// The bar key, Alt: held, every bar shows as under `always`.
const REVEAL_KEYS: [KeyCode; 2] = [KeyCode::AltLeft, KeyCode::AltRight];

/// Whether the bar key is held.
#[derive(Resource, Default, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Reveal {
    #[default]
    Off,
    Held,
}

/// Keep [`Reveal`] as the bar key: held while the game's keys are live.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
pub fn reveal_key(
    keys: Res<ButtonInput<KeyCode>>,
    mut live: crate::input::map_keys::Live,
    mut reveal: ResMut<Reveal>,
) {
    let now = if keys.any_pressed(REVEAL_KEYS) && live.keys() {
        Reveal::Held
    } else {
        Reveal::Off
    };

    reveal.set_if_neq(now);
}

/// The bars setting in force: `always` while the bar key is held.
const fn in_force(bars: HpBars, reveal: Reveal) -> HpBars {
    match reveal {
        Reveal::Held => HpBars::Always,
        Reveal::Off => bars,
    }
}

/// A bar, parent of its parts.
#[derive(Component)]
pub struct Bar;

/// One part of a bar, in draw order.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    Frame,
    Below,
    Right,
    Fill,
}

impl Part {
    const ALL: [Part; 4] = [Part::Frame, Part::Below, Part::Right, Part::Fill];

    /// Its rect in `placed`.
    const fn rect(self, placed: &bar::Bar) -> regent_core::view::ScreenRect {
        match self {
            Part::Frame => placed.frame,
            Part::Below => placed.bevel[0],
            Part::Right => placed.bevel[1],
            Part::Fill => placed.fill,
        }
    }

    #[allow(clippy::cast_precision_loss, reason = "four parts")]
    fn depth(self) -> f32 {
        let at = Part::ALL.iter().position(|p| *p == self).unwrap_or(0);

        at as f32 * PART_STEP
    }
}

/// The colour a shade part blends: black at the share the bevel's soft
/// factor takes off ([`soft::veil`]), on linear colour as the frame
/// blends. The engine moves each pixel's HSV value to 68%
/// ([`bar::BEVEL`]); black over it at this alpha scales its channels
/// alike.
fn bevel_colour() -> Color {
    let alpha = soft::veil(bar::BEVEL, Blending::Linear).unwrap_or(0);

    Color::srgba_u8(0, 0, 0, alpha)
}

/// Keep a bar on every piece `view::map::bar::shown` passes, at its body; a
/// piece held or unseen shows none, as the engine draws it with the unit.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
#[allow(
    clippy::too_many_arguments,
    reason = "a system's arguments are its dependencies"
)]
#[allow(clippy::type_complexity, reason = "a query names what it reads")]
pub fn follow(
    mut commands: Commands,
    corpus: Res<Corpus>,
    settings: Res<Settings>,
    reveal: Res<Reveal>,
    mirrors: Pieces,
    seen: Watch,
    mut bars: ResMut<Bars>,
    pieces: Query<(&Transform, &Visibility), (With<Piece>, Without<Bar>, Without<Part>)>,
    mut drawn: Query<(&mut Transform, &mut Visibility, &Children), (With<Bar>, Without<Part>)>,
    mut parts: Query<(&Part, &mut Sprite, &mut Transform, &MeshMaterial3d<Plain>), Without<Bar>>,
    (mut meshes, mut plains): (ResMut<Assets<Mesh>>, ResMut<Assets<Plain>>),
) {
    let setting = in_force(settings.ui.bars, *reveal);
    let stats = match setting {
        HpBars::Never => Stats::Hidden,
        HpBars::Always | HpBars::Damaged => Stats::Shown,
    };
    let world = seen.world();
    let Bars {
        drawn: bars_drawn,
        quad: unit,
        sized,
        looks,
    } = &mut *bars;
    let unit = unit.get_or_insert_with(|| meshes.add(Rectangle::new(1.0, 1.0)));
    let looks = looks.get_or_insert_with(|| Looks::new(&corpus.catalogue));

    // Each piece's bar, where it shows one.
    let wanted: HashMap<u32, (Vec3, Visibility, bar::Bar, Color, Tie)> = mirrors
        .iter()
        .filter_map(|shown| {
            let (place, seen) = pieces.get(mirrors.drawn(shown.agent)?).ok()?;
            let size = (*sized
                .entry(shown.code)
                .or_insert_with(|| Sized::of(&corpus, shown.code)))?;
            let health = health(world, shown.agent, size.most)?;
            let asked = Asked {
                owner: shown.owner.cast_signed(),
                hp: health.now,
                has_bar: size.has_bar,
            };

            let shown_now = bar::shown(stats, asked) && kept(setting, health);

            shown_now.then(|| {
                let owner = u8::try_from(shown.owner).unwrap_or(u8::MAX);
                let colour = looks.colour(owner);
                let [r, g, b] = quantise(colour).channels();

                (
                    shown.agent,
                    (
                        place.translation,
                        *seen,
                        bar::place(size.rect, size.body, health),
                        Color::srgb_u8(r, g, b),
                        Tie {
                            y: shown.at.1,
                            agent: shown.agent,
                        },
                    ),
                )
            })
        })
        .collect();

    // Gone, or no longer shown: gone from the screen.
    bars_drawn.retain(|agent, entity| {
        let keep = wanted.contains_key(agent);

        if !keep {
            commands.entity(*entity).despawn();
        }

        keep
    });

    // Overlapping bars in a fixed order, not the query's.
    let depths = depths(wanted.values().map(|w| w.4));

    for (agent, (place, seen, placed, colour, _)) in wanted {
        let z = depths.get(&agent).copied().unwrap_or(BAR_Z);
        let at = Vec3::new(place.x, place.y, z);
        let kept = bars_drawn.get(&agent).and_then(|e| drawn.get_mut(*e).ok());

        if let Some((mut transform, mut visibility, children)) = kept {
            transform.set_if_neq(Transform {
                translation: at,
                ..*transform
            });
            visibility.set_if_neq(seen);

            for child in children {
                if let Ok((part, mut sprite, mut put, plain)) = parts.get_mut(*child) {
                    let (size, middle) = quad(part.rect(&placed));

                    put.set_if_neq(Transform {
                        translation: middle.extend(part.depth()),
                        scale: size.extend(1.0),
                        ..*put
                    });

                    if *part == Part::Fill && sprite.color != colour {
                        sprite.color = colour;

                        if let Some(mut twin) = plains.get_mut(&plain.0) {
                            twin.colour = colour.into();
                        }
                    }
                }
            }
        } else {
            // Spawned this frame: its children come with the commands.
            bars_drawn.entry(agent).or_insert_with(|| {
                let made = (unit.clone(), &mut *plains);

                spawn(&mut commands, (at, seen), &placed, colour, made)
            });
        }
    }
}

/// A new bar at `at`, its parts as `placed` lays them out; each part's
/// 3D twin on `quad`, its colour a new [`Plain`].
fn spawn(
    commands: &mut Commands,
    (at, seen): (Vec3, Visibility),
    placed: &bar::Bar,
    colour: Color,
    (quad_mesh, plains): (Handle<Mesh>, &mut Assets<Plain>),
) -> Entity {
    let parent = commands
        .spawn((Bar, Transform::from_translation(at), seen))
        .id();

    for part in Part::ALL {
        let (size, middle) = quad(part.rect(placed));
        let tint = match part {
            Part::Frame => Color::BLACK,
            Part::Below | Part::Right => bevel_colour(),
            Part::Fill => colour,
        };

        commands.spawn((
            part,
            // A unit quad, sized by the scale, as the twin.
            Sprite {
                color: tint,
                custom_size: Some(Vec2::ONE),
                ..default()
            },
            Mesh3d(quad_mesh.clone()),
            MeshMaterial3d(plains.add(Plain::colour(tint))),
            Transform {
                translation: middle.extend(part.depth()),
                scale: size.extend(1.0),
                ..default()
            },
            // Interface, not world art: at the window's pixels.
            RenderLayers::layer(OVERLAY),
            ChildOf(parent),
        ));
    }

    parent
}

/// Each bar's depth, by agent: [`BAR_Z`], then a [`BAR_STEP`] nearer per
/// bar ordered before it by [`Tie`] (board y, then agent), so the nearer
/// unit's over; the same every run, whatever order the bars come in.
fn depths(ties: impl IntoIterator<Item = Tie>) -> HashMap<u32, f32> {
    let mut ties: Vec<Tie> = ties.into_iter().collect();

    ties.sort_unstable();

    #[allow(clippy::cast_precision_loss, reason = "hundreds of bars")]
    let depths = ties
        .iter()
        .enumerate()
        .map(|(nth, t)| (t.agent, BAR_Z + nth as f32 * BAR_STEP))
        .collect();

    depths
}

/// Whether `bars` keeps a bar the engine would draw: under `damaged`,
/// only below the most hit points, 51 of 74 but not 74 of 74. A building
/// under construction is below, so its bar shows its progress.
const fn kept(bars: HpBars, health: Health) -> bool {
    match bars {
        HpBars::Always => true,
        HpBars::Damaged => health.now < health.max,
        HpBars::Never => false,
    }
}

/// Whether the engine draws agent `agent`'s bar, of `unit`, owned by
/// `owner`, under setting `bars`: `damaged` only
/// filters what the engine shows, so counts as shown.
pub fn engine_shows(
    bars: HpBars,
    world: &World,
    (agent, owner): (u32, u32),
    unit: &Unit<'_>,
) -> ring::HpBar {
    let stats = match bars {
        HpBars::Never => Stats::Hidden,
        HpBars::Always | HpBars::Damaged => Stats::Shown,
    };
    let asked = Asked {
        owner: owner.cast_signed(),
        hp: health(world, agent, None).map_or(0, |h| h.now),
        has_bar: bar::has_bar(unit),
    };

    if bar::shown(stats, asked) {
        ring::HpBar::Shown
    } else {
        ring::HpBar::Hidden
    }
}

/// Whether agent `agent` has a hit point left, as hp bar shown
/// asks.
pub fn alive(world: &World, agent: u32) -> bool {
    health(world, agent, None).is_some_and(|h| h.now >= 1)
}

/// A piece's hit points, `APU:0` of `APU:1`, the descriptor's most when
/// it keeps none. `None` without a piece.
fn health(world: &World, agent: u32, most: Option<i32>) -> Option<Health> {
    let piece = world.piece(agent)?;
    let attribute = |key| attrib::get(&piece.attributes, key);
    let now = attribute(HP).unwrap_or(0);
    let max = attribute(MAX_HP).or(most).unwrap_or(0);

    Some(Health::of(now, max))
}

/// A rect in screen pixels about a piece's anchor (y down) as a quad:
/// its size and its middle, world units (y up).
#[allow(clippy::cast_precision_loss, reason = "screen pixels")]
fn quad([x0, y0, x1, y1]: regent_core::view::ScreenRect) -> (Vec2, Vec2) {
    let size = Vec2::new((x1 - x0) as f32, (y1 - y0) as f32);
    let middle = Vec2::new((x0 + x1) as f32 / 2.0, -((y0 + y1) as f32) / 2.0);

    (size, middle)
}

/// The hit point bars drawn, a game's.
pub fn plugin(app: &mut bevy::prelude::App) {
    use crate::game::scope::Scoped as _;

    app.per_game::<Bars>().init_resource::<Reveal>();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A rect 30 wide and 3 tall, 61 to 64 pixels under the anchor: its
    /// middle 62.5 world units down.
    #[test]
    fn a_bars_rect_is_a_quad_about_the_anchor() {
        let (size, middle) = quad([-15, 61, 15, 64]);

        assert_eq!(size, Vec2::new(30.0, 3.0));
        assert_eq!(middle, Vec2::new(0.0, -62.5));
    }

    #[test]
    fn a_damaged_units_bar_shows_and_a_whole_ones_does_not() {
        let damaged = Health::of(51, 74);
        let whole = Health::of(74, 74);

        assert!(kept(HpBars::Damaged, damaged));
        assert!(!kept(HpBars::Damaged, whole));
        assert!(kept(HpBars::Always, whole));
        assert!(!kept(HpBars::Never, damaged));
    }

    /// The bar key held shows an unhurt unit's bar under every setting;
    /// released, the setting rules.
    #[test]
    fn the_bar_key_shows_every_bar_while_held() {
        let whole = Health::of(74, 74);

        for bars in [HpBars::Always, HpBars::Damaged, HpBars::Never] {
            assert!(kept(in_force(bars, Reveal::Held), whole));
            assert_eq!(in_force(bars, Reveal::Off), bars);
        }
    }

    /// Bars order by board y, then agent, whatever order they come in;
    /// each past the parts of the one before.
    #[test]
    fn bars_draw_by_y_then_agent() {
        let tie = |y, agent| Tie { y, agent };
        let all = [tie(5, 9), tie(5, 3), tie(-2, 7), tie(8, 1)];
        let mut reversed = all;

        reversed.reverse();

        for ties in [all, reversed] {
            let z = depths(ties);

            assert!(z[&7] < z[&3] && z[&3] < z[&9] && z[&9] < z[&1]);
            assert!(z[&3] - z[&7] > Part::Fill.depth());
        }
    }

    /// The parts draw frame first, fill last.
    #[test]
    fn the_fill_draws_over_the_frame() {
        assert!(Part::Frame.depth() < Part::Below.depth());
        assert!(Part::Right.depth() < Part::Fill.depth());
    }

    /// Black at the bevel's soft share: the 68% shade's factor keeps
    /// 0.379 of linear light, so about 0.62 is taken.
    #[test]
    fn the_bevel_takes_what_its_soft_factor_does() {
        let alpha = bevel_colour().alpha();

        assert!((alpha - 0.62).abs() < 0.01, "{alpha}");
    }
}
