//! Picking a piece on the map, and the ring drawn under it.
//!
//! ```text
//!  the mouse in the map or the tracking window ──> Pointing: a world
//!    point and its view's zoom (Aimed)
//!  Aimed ──> Picker: the engine's pick (view::map::hit), front to back: the
//!    first piece met on its pixels, else the last in its box
//!  left click ──> that piece, met on its pixels or a character ──> Selected
//!  right click in the map or the window ──> nothing selected
//!  a message flag met on its pixels ──> Reading: its text in the panel
//!  Selected ──> ring: the game's sprite (view::map::ring), drawn as a piece's frame
//!    a piece whose art has no rect (the Creature Den) ──> fitted to its
//!    tiles, bar and name as first ringed, kept (ring::Drawn)
//!  Hover (cursor), not selected ──> ring::HOVER, AVn8, the same way
//!  a reward flag, selected ──> its brackets, AVn2 (ring::Look::of)
//!  [enhancements] rings = "native" or "modern" ──> each ring but the
//!    brackets drawn as its band instead (rings)
//!  [enhancements] side_ring ──> the hover ring, a player's piece's, in
//!    its side colour (rings::sided); a standing building the engine's
//!    hover ring skips (all but the shrines) takes it too (SideHover)
//! ```

use std::collections::HashMap;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use regent_core::content::unit::Unit;
use regent_core::data::catalogue::Catalogue;
use regent_core::data::desc::UnitKind;
use regent_core::data::fourcc::FourCc;
use regent_core::data::payload::Rgb;
use regent_core::sim::world::{
    draw_view::{Housed, Shown},
    offer::is_reward_flag,
};
use regent_core::view::ScreenRect;
use regent_core::view::map::cursor::{Flagged, Hit, Kind, Over, over};
use regent_core::view::map::hit::{self, Part};
use regent_core::view::map::ring::{self, Fit, Look, Stand, Why};
use regent_core::view::panel::minimap::Looks;
use regent_core::view::panel::tracking;
use regent_draw::data::Corpus;
use regent_draw::map::Maps;
use regent_draw::map::cel::{Cel, Picture};
use regent_draw::ring::{Ellipse, Rings as Drawing};
use regent_draw::stage::View;

use crate::files::config::{HpBars, Settings};
use crate::game::seen::{News, Watch};
use crate::input::{Clicks, Press};
use crate::map::cursor::{Hover, SideHover};
use crate::map::effects::{EffectDrawn, Effects, Hung};
use crate::map::mirror::Pieces;
use crate::map::pieces::{Piece, Shows, Under};
use crate::map::rings::{self, Native};
use crate::ui::tracking::Tracking;
use crate::ui::{Covered, on_map};
use crate::{map::hp_bars, map::labels};
use regent_draw::art::{Assets2, Sprites};

/// The selected piece, by agent.
#[derive(Resource, Default)]
pub struct Selected(pub Option<u32>);

/// The selected message flag, by handle: it has no agent
/// (`world::Message`). Its panel is `EventMessageController`'s.
#[derive(Resource, Default)]
pub struct Reading(pub Option<u32>);

/// The view the pointer is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Through {
    /// The main view's map.
    Map,
    /// The tracking window's picture.
    Window,
}

/// Where the pointer is on the world: a point as the main view draws it,
/// and the view it is seen through. Every mode's click, cursor and pick
/// read it, so the tracking window takes them as the map does: the
/// engine's cursor acts in the view under it (cursor view, set by
/// cursor view take).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aimed {
    /// World units.
    pub world: Vec2,
    pub through: Through,
}

impl Aimed {
    /// The view's zoom, which scales the pick's boxes: the tracking
    /// window's [`tracking::ZOOM`].
    const fn zoom(self, view: &View) -> f32 {
        match self.through {
            Through::Map => view.zoom,
            Through::Window => tracking::ZOOM,
        }
    }

    /// The piece the pick takes here, `ringed` as [`Picker::under_at`].
    pub fn under(self, picker: &mut Picker, ringed: (Option<u32>, Option<u32>)) -> Option<Picked> {
        let zoom = self.zoom(&picker.view);

        picker.under_at(self.world, ringed, zoom)
    }

    /// The ground's whole vertex here, 32nds ([`Maps::pick_at`]).
    pub fn spot(self, maps: &Maps) -> Option<(i32, i32)> {
        maps.pick_at(self.world)
    }
}

/// The pointer on the map or the tracking window ([`Aimed`]).
#[derive(SystemParam)]
pub struct Pointing<'w, 's> {
    window: Single<'w, 's, &'static Window, With<PrimaryWindow>>,
    view: Res<'w, View>,
    covered: Res<'w, Covered>,
    tracking: Res<'w, Tracking>,
}

impl Pointing<'_, '_> {
    /// Where the pointer is on the world; `None` off both views.
    pub fn aimed(&self) -> Option<Aimed> {
        let at = self.window.cursor_position()?;

        aimed(&self.view, &self.covered, &self.tracking, at)
    }
}

/// [`Pointing::aimed`] with the pointer at `at`, window points.
pub fn aimed(view: &View, covered: &Covered, tracking: &Tracking, at: Vec2) -> Option<Aimed> {
    let on = |through| move |world| Aimed { world, through };

    if on_map(view, covered, at) {
        view.world_at(at).map(on(Through::Map))
    } else {
        tracking.pointed(at).map(on(Through::Window))
    }
}

/// Select the piece under a left click in the map or the tracking
/// window, met on its pixels or a character it may pick
/// (select click, the select mode's click); empty ground keeps the
/// selection, a right click clears it.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
#[allow(
    clippy::too_many_arguments,
    reason = "a system's arguments are its dependencies"
)]
pub fn pick(
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    pointing: Pointing,
    hover: Res<Hover>,
    clicks: Res<Clicks>,
    mut picker: Picker,
    mut selected: ResMut<Selected>,
    mut reading: ResMut<Reading>,
) {
    // Shift with the left button pans (`stage::pan`). A double click's
    // second press is `WM_LBUTTONDBLCLK`, which selects nothing.
    let panning = keys.pressed(KeyCode::ShiftLeft);
    let double = clicks.now == Some(Press::Double);
    let clicked = buttons.just_pressed(MouseButton::Left) && !panning && !double;
    let cleared = buttons.just_pressed(MouseButton::Right);

    // Clicks on the bar and the day counter are egui's.
    let aimed = pointing.aimed();

    if let (true, Some(aimed)) = (clicked, aimed) {
        let message = picker.message_under(aimed.world);
        let taken = aimed
            .under(&mut picker, (selected.0, hover.0))
            .filter(|p| over(p.kind, p.hit) == Over::Piece);

        match (message, taken) {
            (Some(handle), _) => {
                reading.0 = Some(handle);
                selected.0 = None;
            }
            (None, Some(taken)) => {
                reading.0 = None;
                selected.0 = Some(taken.agent);
            }
            (None, None) => {}
        }
    }

    if let (true, Some(_)) = (cleared, aimed) {
        selected.0 = None;
        reading.0 = None;
    }
}

/// The piece the pick takes, as its callers read it.
#[derive(Clone, Copy, Debug)]
pub struct Picked {
    pub agent: u32,
    pub hit: Hit,
    pub kind: Kind,
    /// [`ring::highlights`] takes it: the hover ring's.
    pub lit: bool,
}

impl Picked {
    /// A standing building [`ring::highlights`] skips: every one but the
    /// shrines. `[enhancements] side_ring` rings it alone when a click
    /// selects it, in select mode ([`SideHover`]).
    pub fn side_only(self) -> bool {
        self.kind == Kind::Building(Stand::Standing)
            && !self.lit
            && over(self.kind, self.hit) == Over::Piece
    }

    /// Met on its pixels or a character: what a click selects and an
    /// attack flag takes.
    pub fn met(self) -> bool {
        self.hit == Hit::Picture || self.kind == Kind::Character
    }
}

/// A descriptor as the pick sorts it.
#[derive(Clone, Copy)]
struct Sort {
    kind: Option<UnitKind>,
    /// [`ring::highlights`] standing, and fallen.
    lit: (bool, bool),
    /// [`hit::solid`].
    solid: bool,
}

impl Sort {
    /// Whether the pick stops at a piece of this sort: a hung one always,
    /// as it picks its holder ([`Picker::holder`]).
    const fn stops(self, housed: Housed) -> bool {
        self.solid || matches!(housed, Housed::Hung { .. })
    }

    fn of(catalogue: &Catalogue<'_>, code: FourCc) -> Sort {
        let unit = Unit::find(catalogue, code).and_then(Result::ok);
        let lit = |stand| {
            unit.as_ref()
                .is_some_and(|u| ring::highlights(&u.desc, stand))
        };

        Sort {
            kind: unit.as_ref().and_then(|u| u.kind),
            lit: (lit(Stand::Standing), lit(Stand::Fallen)),
            solid: unit.as_ref().is_none_or(|u| hit::solid(&u.desc)),
        }
    }
}

/// The engine's pick over the drawn pieces ([`regent_core::view::map::hit`]).
#[derive(SystemParam)]
#[allow(clippy::type_complexity, reason = "a query names what it reads")]
pub struct Picker<'w, 's> {
    corpus: Res<'w, Corpus>,
    pub view: Res<'w, View>,
    mirrors: Pieces<'w, 's>,
    pieces: Query<
        'w,
        's,
        (
            &'static Piece,
            &'static Transform,
            &'static Visibility,
            &'static Children,
            &'static Under,
        ),
    >,
    frames: Query<'w, 's, (&'static Shows, &'static Transform, &'static Visibility)>,
    /// Effects drawn, by handle: message flags among them.
    hung: Res<'w, Hung>,
    effects: Res<'w, Effects>,
    drawn: Query<
        'w,
        's,
        (&'static Transform, &'static Visibility, &'static Children),
        With<EffectDrawn>,
    >,
    /// The rings drawn: each piece's fit.
    drawn_rings: Res<'w, Rings>,
    /// Each descriptor's sort and rings, made once a fit.
    sorts: Local<'s, HashMap<FourCc, Sort>>,
    rings: Local<'s, HashMap<(FourCc, Look, Fit), Option<ScreenRect>>>,
    /// The bar and label each piece's box takes, by agent, remade as
    /// what they are made from changes.
    extras: Local<'s, HashMap<u32, (Extra, Option<ScreenRect>)>>,
    settings: Res<'w, Settings>,
    seen: Watch<'w>,
}

/// What a piece's bar and label rects are made from:
/// its descriptor, owner, the bar's setting and a hit point left, its
/// label's source.
#[derive(Clone, PartialEq)]
struct Extra {
    code: FourCc,
    owner: u32,
    bars: HpBars,
    alive: bool,
    label: Option<labels::LabelText>,
}

/// A drawn piece as the pick reads it: agent, anchor in screen pixels at
/// scale 1 (y down), the tiles it shows with their top left about it, and
/// the ground under it ([`Under`]).
type Seen = (u32, Vec2, Vec<((usize, usize), ScreenRect)>, Under);

impl Picker<'_, '_> {
    /// The piece under world point `at` in a view at zoom `scale`, whose
    /// boxes it scales (the tracking window's 0.5, `IsoGameView`);
    /// `ringed` the selected and the hovered piece, whose boxes take
    /// their rings. `IsoGameView` under `hit::MASK`, then a hung
    /// piece's holder.
    pub fn under_at(
        &mut self,
        at: Vec2,
        ringed: (Option<u32>, Option<u32>),
        scale: f32,
    ) -> Option<Picked> {
        let (agent, hit) = self.met(at, ringed, scale)?;

        self.picked(self.holder(agent), hit)
    }

    /// The piece met on its picture at world point `at`, no holder taken:
    /// the map tooltip's pick (map tip callback, mask `0x30`).
    /// Its reach of 0 changes no picture hit.
    pub fn pictured(&mut self, at: Vec2) -> Option<u32> {
        self.met(at, (None, None), self.view.zoom)
            .filter(|(_, hit)| *hit == Hit::Picture)
            .map(|(agent, _)| agent)
    }

    /// `IsoGameView` at world point `at`: the piece and how it was
    /// met, `ringed` the pieces whose boxes take their rings, in a view at
    /// zoom `scale`.
    fn met(
        &mut self,
        at: Vec2,
        ringed: (Option<u32>, Option<u32>),
        scale: f32,
    ) -> Option<(u32, Hit)> {
        let mut seen = self.seen();

        // A spell effect takes no click: the pick goes on to what is under it.
        seen.retain(|(_, s)| self.stops(s.0));

        // Front to back: the draw list's tail first.
        seen.sort_by(|a, b| b.0.total_cmp(&a.0));

        // The point, y down.
        let at = Vec2::new(at.x, -at.y);
        let seen: Vec<Seen> = seen.into_iter().map(|(_, s)| s).collect();
        let boxes: Vec<Option<ScreenRect>> = seen.iter().map(|s| self.boxed(s, ringed)).collect();
        let inside = |i: usize| {
            boxes[i].is_some_and(|b| hit::boxed(hit::screen_box(b, scale), floor(at * scale)))
        };
        let picture = |i: usize| self.picture(&seen[i], at);

        let (i, hit) = hit::pick(0..seen.len(), inside, picture)?;

        Some((seen[i].0, hit))
    }

    /// The message flag met on its pixels at world point `at`, front
    /// first, by handle. Pick kind ok passes an `MSGF` unit, and
    /// pick unit at gives no holder for it. **Inferred**: as an overlay
    /// it draws over what it might hide, so it comes before [`Picker::under_at`].
    pub fn message_under(&self, at: Vec2) -> Option<u32> {
        let at = Vec2::new(at.x, -at.y);

        self.hung
            .drawn()
            .filter(|(handle, _)| self.effects.message(*handle))
            .filter_map(|(handle, entity)| {
                let (place, shown, children) = self.drawn.get(entity).ok()?;
                let anchor = place.translation;
                let tiles = children
                    .iter()
                    .filter_map(|c| self.frames.get(c).ok())
                    .filter(|(.., shown)| **shown != Visibility::Hidden)
                    .map(|(shows, put, _)| (shows.0, rect_of(put)))
                    .collect();
                let seen = (
                    handle,
                    Vec2::new(anchor.x, -anchor.y),
                    tiles,
                    Under::default(),
                );

                (*shown != Visibility::Hidden && self.picture(&seen, at))
                    .then_some((anchor.z, handle))
            })
            .max_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, handle)| handle)
    }

    /// Every drawn piece with its depth: what `IsoGameView` lists.
    fn seen(&self) -> Vec<(f32, Seen)> {
        self.pieces
            .iter()
            .filter(|(_, _, shown, ..)| **shown != Visibility::Hidden)
            .map(|(piece, place, _, children, under)| {
                let at = place.translation;
                let tiles = children
                    .iter()
                    .filter_map(|c| self.frames.get(c).ok())
                    .filter(|(.., shown)| **shown != Visibility::Hidden)
                    .map(|(shows, put, _)| (shows.0, rect_of(put)))
                    .collect();

                (at.z, (piece.0, Vec2::new(at.x, -at.y), tiles, *under))
            })
            .collect()
    }

    /// A piece's box in screen pixels at scale 1: its tiles and anchor,
    /// its ring while selected or hovered, then its bar and label.
    fn boxed(
        &mut self,
        (agent, anchor, tiles, under): &Seen,
        ringed: (Option<u32>, Option<u32>),
    ) -> Option<ScreenRect> {
        let why = match ringed {
            (Some(s), _) if s == *agent => Some(Why::Selected),
            (_, Some(h)) if h == *agent => Some(Why::Hovered),
            _ => None,
        };
        let ring = why.and_then(|why| self.ring(*agent, why));
        let own = hit::covering(tiles.iter().map(|(_, r)| *r));
        let rel = with_ring(own, ring, *under);
        let rel = match (rel, self.extra(*agent)) {
            (Some(r), Some(e)) => Some(hit::union(r, e)),
            (r, e) => r.or(e),
        };
        let (x, y) = floor(*anchor);

        rel.map(|[x0, y0, x1, y1]| [x0 + x, y0 + y, x1 + x, y1 + y])
    }

    /// `agent`'s bar and label rect about its anchor, as the engine's
    /// flags show them; `None` without area.
    fn extra(&mut self, agent: u32) -> Option<ScreenRect> {
        let shown = self.mirrors.get(agent)?;
        let world = self.seen.world();
        let ui = &self.settings.ui;
        let key = Extra {
            code: shown.code,
            owner: shown.owner,
            bars: ui.bars,
            alive: hp_bars::alive(world, agent),
            label: labels::engine_source(ui.names, world, shown),
        };

        let corpus = &self.corpus;
        let made = || {
            let unit = Unit::find(&corpus.catalogue, shown.code)?.ok()?;
            let bar = hp_bars::engine_shows(ui.bars, world, (agent, shown.owner), &unit);
            let text = key.label.as_ref().map(|s| (s, labels::text(corpus, s)));
            let words = text.as_ref().map(|(source, text)| match source {
                labels::LabelText::Reward(_) => ring::Words::Reward(text),
                labels::LabelText::Name(..) => ring::Words::Name(text),
            });

            ring::Drawn::of(&corpus.catalogue, &unit, None, bar, words)
                .ok()?
                .shown()
        };

        match self.extras.get(&agent) {
            Some((kept, rect)) if *kept == key => *rect,
            _ => {
                let rect = made();

                self.extras.insert(agent, (key, rect));
                rect
            }
        }
    }

    /// `agent`'s ring rect, drawn for `why`, about its anchor, made once.
    fn ring(&mut self, agent: u32, why: Why) -> Option<ScreenRect> {
        let shown = self.mirrors.get(agent)?;
        let (code, look) = (shown.code, Look::of(flagged(shown), why));
        let fit = self.drawn_rings.fit(agent);
        let corpus = &self.corpus;

        *self.rings.entry((code, look, fit)).or_insert_with(|| {
            let unit = Unit::find(&corpus.catalogue, code)?.ok()?;

            ring::fitted(&corpus.catalogue, &unit, look, fit)
                .ok()
                .flatten()
                .map(|r| r.rect())
        })
    }

    /// Whether the pick meets a piece's pixels at `at`, screen pixels at
    /// scale 1.
    fn picture(&self, (_, anchor, tiles, _): &Seen, at: Vec2) -> bool {
        let rel = floor(at - *anchor);
        let corner = |r: &ScreenRect| (r[0], r[1]);

        tiles
            .iter()
            .filter_map(|(tile, r)| Part::tile(&self.corpus.catalogue, *tile, corner(r))?.ok())
            .any(|part| part.picks(rel))
    }

    /// A hung piece's holder, but a reward flag's.
    fn holder(&self, agent: u32) -> u32 {
        let shown = self.mirrors.get(agent);

        match shown.map(|s| (s.housed, flagged(s))) {
            Some((Housed::Hung { on, .. }, Flagged::No)) => on,
            _ => agent,
        }
    }

    /// Whether the pick stops at `agent` ([`Sort::stops`]).
    fn stops(&mut self, agent: u32) -> bool {
        let corpus = &self.corpus;

        self.mirrors.get(agent).is_none_or(|shown| {
            self.sorts
                .entry(shown.code)
                .or_insert_with(|| Sort::of(&corpus.catalogue, shown.code))
                .stops(shown.housed)
        })
    }

    /// `agent` met by `hit`, sorted by its descriptor.
    /// Whether the hover ring takes shown `agent` ([`Picked::lit`]).
    pub fn lit(&mut self, agent: u32) -> bool {
        self.picked(agent, Hit::Picture).is_some_and(|p| p.lit)
    }

    fn picked(&mut self, agent: u32, hit: Hit) -> Option<Picked> {
        let shown = self.mirrors.get(agent)?;
        let corpus = &self.corpus;
        let sort = *self
            .sorts
            .entry(shown.code)
            .or_insert_with(|| Sort::of(&corpus.catalogue, shown.code));
        // A building agentless since its `DeleteAgent` is fallen too,
        // so the click refuses it.
        let stand = if shown.ruined || self.seen.world().fallen(agent) {
            Stand::Fallen
        } else {
            Stand::Standing
        };
        let lit = match stand {
            Stand::Standing => sort.lit.0,
            Stand::Fallen => sort.lit.1,
        };

        Some(Picked {
            agent,
            hit,
            kind: Kind::of(sort.kind, stand, flagged(shown)),
            lit,
        })
    }
}

/// Whether `shown` is a reward flag.
pub(crate) fn flagged(shown: &Shown) -> Flagged {
    if is_reward_flag(shown.code) {
        Flagged::Yes
    } else {
        Flagged::No
    }
}

/// A frame quad's tile rect about its piece's anchor, screen pixels at
/// scale 1, y down: `put` is its middle and size, world units, y up.
fn rect_of(put: &Transform) -> ScreenRect {
    let size = put.scale.truncate();
    let middle = put.translation.truncate();
    let corner = Vec2::new(middle.x - size.x / 2.0, -(middle.y + size.y / 2.0));
    let (x0, y0) = floor(corner.round());
    let (w, h) = floor(size.round());

    [x0, y0, x0 + w, y0 + h]
}

/// A point's pixel.
#[allow(clippy::cast_possible_truncation, reason = "screen pixels")]
const fn floor(at: Vec2) -> (i32, i32) {
    (at.x.floor() as i32, at.y.floor() as i32)
}

/// Forget a selected piece that has left the world, or a building the
/// engine deselected (`Output::deselects`): one at no hit points, or
/// collapsed.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
pub fn forget(pieces: Pieces, mut news: ResMut<News>, mut selected: ResMut<Selected>) {
    let deselects = std::mem::take(&mut news.deselects);
    let gone = selected.0.is_some_and(|agent| !pieces.contains(agent));

    if gone || dropped(selected.0, &deselects) {
        selected.0 = None;
    }
}

/// Whether `deselects` names the `selected` piece.
fn dropped(selected: Option<u32>, deselects: &[u32]) -> bool {
    selected.is_some_and(|agent| deselects.contains(&agent))
}

/// How far behind its piece the ring draws, so the piece's body covers
/// it: short of the scatter row before on maps of under 10,000 rows.
/// **Game**: the ring shows under the body, combined with its shadow (the
/// user, against the original). **Inferred**: the engine blits the ring,
/// then the piece, body and shadow at once, so the shadow stacks on
/// the ring's glow (`regent_core::view::raster::stack`).
const RING_DROP: f32 = 0.004;

/// The rings drawn, the selection's and the hover's, the agent each is
/// under and its side colour, if taken; their pictures are the art
/// service's (`art::Sprites`).
#[derive(Resource, Default)]
pub struct Rings {
    drawn: [Option<(Entity, Key)>; 2],
    /// How they were drawn: remade when `[enhancements] rings` or
    /// `side_ring` changes.
    with: (Drawing, bool),
    /// The side colours, read once.
    looks: Option<Looks>,
    /// The box each piece without a rect of its own was first ringed in,
    /// kept: `IsoGenUnit` fills an empty unit rect once
    /// ([`ring::Drawn`]).
    filled: HashMap<u32, ring::Drawn>,
}

impl Rings {
    /// How agent `agent`'s ring fits: by the box it was filled with, else
    /// its own rect.
    fn fit(&self, agent: u32) -> Fit {
        self.filled.get(&agent).map_or(Fit::Own, |d| Fit::Drawn(*d))
    }
}

/// A ring's piece, by agent, and its side colour, if taken.
type Key = (u32, Option<Rgb>);

/// A descriptor's selection ring, as the viewer draws a frame: its
/// materials, and its quad about the piece's anchor.
#[derive(Clone)]
struct Glow {
    cel: Cel,
    put: Transform,
    quad: Handle<Mesh>,
}

/// A ring's quads: the art's frame, or its band.
#[derive(Clone)]
enum Quads {
    Art(Glow),
    Native(Native, Handle<Mesh>),
}

/// The selection ring.
#[derive(Component)]
pub struct Ring;

/// Keep a ring under the selected piece, and the hover's under the
/// hovered one unless selected (a unit draws one ring): a new one for a
/// new piece, hidden while the piece is, gone with it. The engine draws
/// the ring in its unit's draw, so a unit held or
/// unseen under the fog shows none; the selection stays.
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
    selected: Res<Selected>,
    hover: Res<Hover>,
    side_hover: Res<SideHover>,
    mirrors: Pieces,
    mut rings: ResMut<Rings>,
    mut sprites: ResMut<Sprites>,
    mut stores: Assets2,
    mut ellipses: ResMut<Assets<Ellipse>>,
    seen: Watch,
    pieces: Query<(&Transform, &Visibility, &Under, &Children), (With<Piece>, Without<Ring>)>,
    frames: Query<(&Shows, &Transform, &Visibility), Without<Ring>>,
    mut drawn: Query<(&mut Transform, &mut Visibility), With<Ring>>,
) {
    let under = |agent: Option<u32>| {
        agent.and_then(|agent| {
            let shown = mirrors.get(agent)?;
            let (place, seen, under, children) = pieces.get(mirrors.drawn(agent)?).ok()?;
            let tiles = hit::covering(
                children
                    .iter()
                    .filter_map(|c| frames.get(c).ok())
                    .filter(|(.., shown)| **shown != Visibility::Hidden)
                    .map(|(_, put, _)| rect_of(put)),
            );

            Some((agent, (shown, tiles), (place.translation, *seen, *under)))
        })
    };
    let with = settings.enhancements.rings;
    let sides = settings.enhancements.side_ring;
    let looks = rings
        .looks
        .get_or_insert_with(|| Looks::new(&corpus.catalogue))
        .clone();

    // The engine's hover, else, `side_ring` on, a player's building the
    // side ring alone takes.
    let building = side_hover.0.filter(|agent| {
        sides
            && mirrors
                .get(*agent)
                .is_some_and(|s| rings::side_colour(&looks, (Why::Hovered, s.owner)).is_some())
    });
    let hovered = hover.0.or(building).filter(|h| selected.0 != Some(*h));
    let wanted = [
        (under(selected.0), Why::Selected),
        (under(hovered), Why::Hovered),
    ];

    // A filled box goes with its piece.
    rings.filled.retain(|agent, _| mirrors.contains(*agent));

    // Drawn another way: every ring made again.
    if rings.with != (with, sides) {
        for (entity, _) in rings.drawn.iter_mut().filter_map(Option::take) {
            commands.entity(entity).despawn();
        }

        rings.with = (with, sides);
    }

    for (i, (under, why)) in wanted.into_iter().enumerate() {
        // A reward flag's ring is its brackets; the
        // hover ring takes its side's colour, `side_ring` on.
        let under = under.map(|(agent, (shown, tiles), at)| {
            let side = sides
                .then(|| rings::side_colour(&looks, (why, shown.owner)))
                .flatten();

            (
                (agent, side),
                (shown, tiles),
                Look::of(flagged(shown), why),
                at,
            )
        });

        // A new piece or side, or none: a new ring, or none.
        if rings.drawn[i].map(|(_, key)| key) != under.map(|(key, ..)| key) {
            if let Some((entity, _)) = rings.drawn[i].take() {
                commands.entity(entity).despawn();
            }

            if let Some(((agent, side), (shown, tiles), look, at)) = under {
                let fit = fit_of(
                    &corpus,
                    &mut rings.filled,
                    (shown, tiles),
                    (&settings, seen.world()),
                );
                let code = shown.code;
                let banded = banded(
                    &corpus,
                    (&mut sprites, &mut stores),
                    &mut ellipses,
                    (code, look, fit, (with, side)),
                );
                let quads = banded.or_else(|| {
                    glow(&corpus, (&mut sprites, &mut stores), (code, look, fit)).map(Quads::Art)
                });
                let entity = spawn(&mut commands, quads, at);

                rings.drawn[i] = Some((entity, (agent, side)));
            }
        }

        let ring = rings.drawn[i].and_then(|(entity, _)| drawn.get_mut(entity).ok());

        if let (Some((.., at)), Some((mut transform, mut visibility))) = (under, ring) {
            (*transform, *visibility) = put(at);
        }
    }
}

/// `shown`'s fit: its own rect, else the box it was first ringed in,
/// drawn as `tiles` (`IsoGenUnit` fills an empty unit rect once),
/// its bar and name as the engine's flags show them.
fn fit_of(
    corpus: &Corpus,
    filled: &mut HashMap<u32, ring::Drawn>,
    (shown, tiles): (&Shown, Option<ScreenRect>),
    (settings, world): (&Settings, &regent_core::sim::world::World),
) -> Fit {
    let catalogue = &corpus.catalogue;
    let made = || {
        let unit = Unit::find(catalogue, shown.code)?.ok()?;

        // Only a unit with no rect of its own is filled.
        matches!(ring::rect(catalogue, &unit), Ok(None)).then_some(())?;

        let who = (shown.agent, shown.owner);
        let bar = hp_bars::engine_shows(settings.ui.bars, world, who, &unit);
        let name = labels::engine_name(corpus, settings.ui.names, world, (shown.agent, shown.code));

        let name = name.as_deref().map(ring::Words::Name);

        ring::Drawn::of(catalogue, &unit, tiles, bar, name).ok()
    };

    match filled.get(&shown.agent).copied().or_else(made) {
        Some(drawn) => {
            filled.insert(shown.agent, drawn);
            Fit::Drawn(drawn)
        }
        None => Fit::Own,
    }
}

/// The `look` ring fitted to descriptor `code` by `fit`. `None` without
/// one.
///
/// `AVn1`'s tiles are glow indices (251 to 254): the band material marks
/// them and the shadow pass lightens the ground, as for a piece's shadow.
fn glow(
    corpus: &Corpus,
    (sprites, stores): (&mut Sprites, &mut Assets2),
    (code, look, fit): (FourCc, Look, Fit),
) -> Option<Glow> {
    let unit = Unit::find(&corpus.catalogue, code)?.ok()?;
    let ring = ring::fitted(&corpus.catalogue, &unit, look, fit).ok()??;
    let cel = sprites.frame(corpus, stores, ring.frame)?;
    let put = Transform {
        translation: offset(ring.at, cel.anchor).extend(0.0),
        scale: cel.size.extend(1.0),
        ..default()
    };

    Some(Glow {
        cel,
        put,
        quad: sprites.quad(&mut stores.meshes),
    })
}

/// The `look` ring fitted to descriptor `code` by `fit`, drawn as its
/// band `with` a look, in a `side` colour if given. `None` without one,
/// for a flag's brackets, or with the art's and no side.
fn banded(
    corpus: &Corpus,
    (sprites, stores): (&mut Sprites, &mut Assets2),
    ellipses: &mut Assets<Ellipse>,
    (code, look, fit, (with, side)): (FourCc, Look, Fit, (Drawing, Option<Rgb>)),
) -> Option<Quads> {
    let unit = Unit::find(&corpus.catalogue, code)?.ok()?;
    let ring = ring::fitted(&corpus.catalogue, &unit, look, fit).ok()??;
    let native = match side {
        Some(colour) => rings::sided(corpus, ellipses, &ring, (with, colour)),
        None => rings::native(corpus, ellipses, &ring, with),
    }?;

    Some(Quads::Native(native, sprites.quad(&mut stores.meshes)))
}

/// A ring quad's middle about the piece's anchor, world units, y up: the
/// tile's anchor `at` screen pixels from the piece's (y down), `anchor`
/// the tile's anchor from its middle (as [`Cel::anchor`]).
fn offset(at: (i32, i32), anchor: Vec2) -> Vec2 {
    #[allow(clippy::cast_precision_loss, reason = "screen pixels")]
    let at = Vec2::new(at.0 as f32, -(at.1 as f32));

    at - anchor
}

/// A ring's entity at a piece at `at`, with a child per material of
/// `quads`; none without a ring.
fn spawn(commands: &mut Commands, quads: Option<Quads>, at: (Vec3, Visibility, Under)) -> Entity {
    let (transform, visibility) = put(at);
    let parent = commands.spawn((Ring, transform, visibility)).id();

    if let Some(Quads::Native(Native(layers), quad)) = &quads {
        for (ellipse, put) in layers {
            commands.spawn((
                Mesh3d(quad.clone()),
                MeshMaterial3d(ellipse.clone()),
                *put,
                ChildOf(parent),
            ));
        }
    }

    if let Some(Quads::Art(Glow { cel, put, quad })) = quads {
        let quad = Mesh3d(quad);

        match cel.body {
            Picture::Indexed(body) => {
                commands.spawn((quad.clone(), MeshMaterial3d(body), put, ChildOf(parent)));
            }
            Picture::Colour(colour) => {
                commands.spawn((quad.clone(), MeshMaterial3d(colour), put, ChildOf(parent)));
            }
        }

        // The red ring draws mode 8: its lift clears the shadow under it.
        if let Some(lift) = cel.lift {
            commands.spawn((quad.clone(), MeshMaterial3d(lift), put, ChildOf(parent)));
        }

        if let Some(band) = cel.band {
            commands.spawn((quad, MeshMaterial3d(band), put, ChildOf(parent)));
        }
    }

    parent
}

/// Where a ring goes under a piece at `place`: on the ground `under` it,
/// a flier's shadow; it shows as the piece does.
fn put((place, seen, under): (Vec3, Visibility, Under)) -> (Transform, Visibility) {
    (
        Transform::from_translation(place + under.0.extend(0.0) - Vec3::Z * RING_DROP),
        seen,
    )
}

/// A box of `own` tiles and a `ring` about the anchor, screen pixels at
/// scale 1 (y down): the ring where it draws, on the ground `under` the
/// piece.
fn with_ring(
    own: Option<ScreenRect>,
    ring: Option<ScreenRect>,
    under: Under,
) -> Option<ScreenRect> {
    let (dx, dy) = floor(Vec2::new(under.0.x, -under.0.y));
    let ring = ring.map(|[x0, y0, x1, y1]| [x0 + dx, y0 + dy, x1 + dx, y1 + dy]);

    match (own, ring) {
        (Some(o), Some(r)) => Some(hit::union(o, r)),
        (o, r) => o.or(r),
    }
}

/// The selection's state, a game's: the message read and the rings; the
/// selected piece, which the game is made with.
pub fn plugin(app: &mut bevy::prelude::App) {
    use crate::game::scope::Scoped as _;

    app.per_game::<Reading>()
        .per_game::<Rings>()
        .made_by_game::<Selected>();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A building the engine deselects leaves the selection; another's
    /// deselect leaves it be.
    #[test]
    fn an_engine_deselect_drops_only_that_piece() {
        assert!(dropped(Some(7), &[3, 7]));
        assert!(!dropped(Some(8), &[7]));
        assert!(!dropped(None, &[7]));
    }

    /// A storm lets the pick through; a hero stops it; so does what hangs
    /// on a unit, whose click belongs to that unit.
    #[test]
    fn a_spell_effect_takes_no_click_but_a_hung_piece_does() {
        let effect = Sort {
            kind: Some(UnitKind::Character),
            lit: (false, false),
            solid: false,
        };
        let hero = Sort {
            solid: true,
            ..effect
        };
        let hung = Housed::Hung {
            on: 1,
            offset: [regent_core::base::fixed::Fixed::default(); 3],
        };

        assert!(!effect.stops(Housed::Out));
        assert!(effect.stops(hung));
        assert!(hero.stops(Housed::Out));
    }

    #[test]
    fn the_side_ring_uses_the_click_hitbox_for_buildings() {
        let building = Picked {
            agent: 0,
            hit: Hit::Box,
            kind: Kind::Building(Stand::Standing),
            lit: false,
        };

        assert_eq!(over(building.kind, building.hit), Over::Ground);
        assert!(!building.side_only());
        assert!(
            Picked {
                hit: Hit::Picture,
                ..building
            }
            .side_only()
        );
    }

    /// The pick's boxes at the view's zoom: the main view's own, the
    /// tracking window's 0.5 whatever the main view's.
    #[test]
    #[allow(clippy::float_cmp, reason = "zooms are set, not computed")]
    fn a_pick_in_the_tracking_window_takes_its_zoom() {
        let view = View {
            zoom: 2.0,
            ..View::default()
        };
        let at = |through| Aimed {
            world: Vec2::ZERO,
            through,
        };

        assert_eq!(at(Through::Map).zoom(&view), 2.0);
        assert_eq!(at(Through::Window).zoom(&view), tracking::ZOOM);
        assert_eq!(tracking::ZOOM, 0.5);
    }

    #[test]
    fn a_frames_quad_gives_its_tiles_rect_about_the_anchor() {
        // A 10 by 6 tile, top left 3 left of and 5 above the anchor: its
        // middle 2 right and 2 up, world units.
        let put = Transform {
            translation: Vec3::new(2.0, 2.0, 0.0),
            scale: Vec3::new(10.0, 6.0, 1.0),
            ..default()
        };

        assert_eq!(rect_of(&put), [-3, -5, 7, 1]);
    }

    #[test]
    fn the_rings_anchor_lands_at_its_offset_from_the_pieces() {
        // Ring anchor 3 px left of and 1 px below the piece's; the tile's
        // anchor 2 right of and 5 above its middle.
        let middle = offset((-3, 1), Vec2::new(2.0, 5.0));

        assert_eq!(middle, Vec2::new(-5.0, -6.0));
        assert_eq!(middle + Vec2::new(2.0, 5.0), Vec2::new(-3.0, -1.0));
    }

    /// A unit held or unseen draws no ring (ring draw runs in its
    /// draw); the ring sits just behind it.
    #[test]
    fn a_ring_shows_as_its_piece_does() {
        let at = Vec3::new(1.0, 2.0, 3.0);

        for seen in [Visibility::Hidden, Visibility::Inherited] {
            let (transform, shown) = put((at, seen, Under::default()));

            assert_eq!(shown, seen);
            assert!(transform.translation.z < at.z);
        }
    }

    /// A flier's ring sits on the ground under it, not at its body.
    #[test]
    fn a_fliers_ring_sits_on_the_ground_under_it() {
        let body = Vec3::new(1.0, 20.0, 3.0);
        let under = Under(Vec2::new(0.0, -12.0));
        let (transform, _) = put((body, Visibility::Inherited, under));

        assert_eq!(transform.translation.truncate(), Vec2::new(1.0, 8.0));
    }

    /// A flier's box takes its ring where the ring draws, on the ground:
    /// 12 world units down, 12 screen
    /// pixels lower.
    #[test]
    fn a_fliers_box_takes_its_ring_on_the_ground() {
        let own = Some([-5, -10, 5, 0]);
        let ring = Some([-4, -2, 4, 2]);

        assert_eq!(
            with_ring(own, ring, Under(Vec2::new(0.0, -12.0))),
            Some([-5, -10, 5, 14])
        );
        assert_eq!(
            with_ring(own, ring, Under::default()),
            Some([-5, -10, 5, 2])
        );
        assert_eq!(with_ring(None, ring, Under::default()), ring);
    }
}
