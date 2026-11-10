//! What the minimap draws over its ground, from the world: a frame's
//! `GSOverheadGameView` and `OverheadGameView`. **Binary**
//! unless marked; the reading in `docs/original/minimap.md`.
//!
//! ```text
//!  ground     the cached picture (Cache)
//!  reach      white circles about the wizards' guilds and towers, while
//!             a reach spell is aimed (minimap reach)
//!  units      list 0 buildings, 1 characters and marks, 4 reward flags,
//!             MSGF message flags: each shown(), not blinking, its
//!             set 300 frame in its owner's ramp, else a square
//!  frames     the views' rects, 1 px, in order
//! ```

use std::collections::HashMap;

use crate::content::unit::Unit;
use crate::data::attrib::{HAS_EFFECT_INVISIBILITY, Key};
use crate::data::catalogue::Catalogue;
use crate::data::desc::{BuildingFlag, CharacterFlag, UnitKind};
use crate::data::fourcc::FourCc;
use crate::data::imag::SetKind;
use crate::data::payload::{Palette, Rgb};
use crate::sim::movement::action::Look as Playing;
use crate::sim::world::kind::Kind;
use crate::sim::world::{
    World,
    defeat::{End, NO_POINT},
    draw::Overhead,
    draw_view::{Housed, Shown},
    flag::MESSAGE_FLAG,
    minimap_mark::Wave,
    trail::CLONE_PARENT,
};
use crate::view::raster::colour::narrow;
use crate::view::sprite::Sprite;

use super::{
    BLACK, Class, Cloak, Eyes, FALLEN, Filter, Held, LISTS, List, Listed, Lit, PALETTE, Picture,
    REACH, Scale, ScreenRect, Standing, blinked, colour, colours, ramp, shown, square,
};

/// `HasEffectCamouflage`, `APV:45`: hidden from the minimap.
pub const CAMOUFLAGED: Key = Key::new(*b"APV", 45);

/// The caravan, lit while a market is selected.
pub const CARAVAN: FourCc = FourCc(*b"AVa1");

/// The markets that light caravans: Marketplace and Trading Post.
pub const MARKETS: [[u8; 3]; 2] = [*b"ABH", *b"ABL"];

/// The won game's mark, made at the end's point in app state 8, which
/// runs no orders: its frame 0 stays (`World::end_point`).
pub const VICTORY_STAR: FourCc = FourCc(*b"MVa3");

/// What the minimap needs of a descriptor.
#[derive(Clone, Debug)]
pub struct Look {
    /// Its subtype: a square's size.
    pub kind: Option<UnitKind>,
    /// `NotInMiniMap`.
    pub listed: Listed,
    /// Set 300's first slot, frame by frame: the tile and the frame's
    /// nudge; empty without the set.
    frames: Vec<Option<(Sprite, (i16, i16))>>,
    /// Set 300's playback word 2, 0 for none.
    playback: u32,
}

impl Look {
    /// Descriptor `code`'s, from `catalogue`; a code naming no unit has
    /// no set and no kind.
    #[must_use]
    pub fn of(catalogue: &Catalogue<'_>, code: FourCc) -> Look {
        let unit = Unit::find(catalogue, code).and_then(Result::ok);
        let unlisted = unit.as_ref().is_some_and(|u| {
            let building = u
                .desc
                .building()
                .is_some_and(|b| b.has(BuildingFlag::NotInMiniMap));
            let character = u
                .desc
                .character()
                .is_some_and(|c| c.has(CharacterFlag::NotInMiniMap));

            building || character
        });
        let set = unit.as_ref().and_then(|u| {
            u.animations
                .iter()
                .find(|a| a.kind == SetKind::Minimap && a.variant == 0)
        });
        let frames = set
            .and_then(|s| s.poses.first())
            .map(|pose| {
                pose.stream(0)
                    .iter()
                    .zip(pose.nudges.iter().chain(std::iter::repeat(&(0, 0))))
                    .map(|(frame, nudge)| {
                        let (archive, index) = frame.tile()?;
                        let sprite = Sprite::at(catalogue, archive, index)?.ok()?;

                        Some((sprite, *nudge))
                    })
                    .collect()
            })
            .unwrap_or_default();

        Look {
            kind: unit.as_ref().and_then(|u| u.kind),
            listed: if unlisted {
                Listed::Unlisted
            } else {
                Listed::Listed
            },
            frames,
            playback: set.map_or(0, |s| s.playback),
        }
    }

    /// Whether its art has set 300 (`SetKind::Minimap`): the minimap draws that,
    /// not a square.
    #[must_use]
    pub const fn pictured(&self) -> bool {
        !self.frames.is_empty()
    }

    /// Frame `frame` of set 300, and its nudge.
    #[must_use]
    pub fn icon(&self, frame: u32) -> Option<(&Sprite, (i16, i16))> {
        let at = usize::try_from(frame).ok()?;

        self.frames.get(at)?.as_ref().map(|(s, n)| (s, *n))
    }

    /// How its set 300 plays on a counter.
    #[must_use]
    pub fn wave(&self) -> Option<Wave> {
        Wave::of(u32::try_from(self.frames.len()).ok()?, self.playback)
    }
}

/// Descriptors' [`Look`]s, each read once, and the player colours.
#[derive(Clone, Debug)]
pub struct Looks {
    colours: [Rgb; 10],
    looks: HashMap<FourCc, Look>,
}

impl Looks {
    /// The colours `GEPlayerColors` takes from `catalogue`'s `PALT PLYR`;
    /// black without it.
    #[must_use]
    pub fn new(catalogue: &Catalogue<'_>) -> Looks {
        let plyr = catalogue
            .find(b"PALT", PALETTE)
            .and_then(|at| catalogue.record(b"PALT", at))
            .and_then(|r| Palette::parse(r.data).ok());

        Looks {
            colours: plyr.map_or([BLACK; 10], |p| colours(&p)),
            looks: HashMap::new(),
        }
    }

    /// Descriptor `code`'s look.
    pub fn of(&mut self, catalogue: &Catalogue<'_>, code: FourCc) -> &Look {
        self.looks
            .entry(code)
            .or_insert_with(|| Look::of(catalogue, code))
    }

    /// Colour index `index` of [`colours`]: [`super::WHITE`],
    /// [`FALLEN`], or a player's.
    #[must_use]
    pub fn colour(&self, index: u8) -> Rgb {
        self.colours
            .get(usize::from(index))
            .copied()
            .unwrap_or(self.colours[usize::from(FALLEN)])
    }
}

/// One unit drawn at minimap pixel `at`:
/// its set 300 frame `frame` at its anchor plus its nudge, indices 0 to
/// 15 in `colour`'s [`ramp`]; without the set, its [`square`] in
/// `colour`. `colour` is `None` for no owner: the picture keeps its own
/// palette, and no square is drawn (**Inferred**: owner −1 has no colour
/// table entry).
pub fn put(
    picture: &mut Picture,
    look: &Look,
    code: FourCc,
    frame: u32,
    at: (i32, i32),
    colour: Option<Rgb>,
) {
    let ramped = colour.map(ramp);

    if look.pictured() {
        if let Some((sprite, nudge)) = look.icon(frame) {
            let at = (at.0 + i32::from(nudge.0), at.1 + i32::from(nudge.1));

            picture.stamp(sprite, at, ramped.as_ref());
        }
    } else if let (Some(kind), Some(c)) = (look.kind, colour)
        && let Some(rect) = square(at, code, kind)
    {
        picture.fill(rect, narrow(c));
    }
}

/// What a frame's minimap shows besides the world.
#[derive(Clone, Copy, Debug)]
pub struct Scene<'a> {
    /// The local player: whose lit units.
    pub local: u8,
    /// Whose sight the units show by: the local player's, or
    /// [`Eyes::ALL`], every unit seen (openregent's spectator).
    pub eyes: Eyes,
    /// The local player's selection, by agent.
    pub selected: Option<u32>,
    pub filter: Filter,
    /// Reach circles, each a centre and radius in 32nds
    /// ([`World::reach_rings`]); the original's empty while no reach
    /// spell is aimed.
    pub reach: &'a [((i32, i32), i32)],
    /// The views' frames in minimap pixels ([`super::viewport`]) and
    /// colours, drawn in order.
    pub frames: &'a [(ScreenRect, Rgb)],
    /// The world's pieces as [`World::shown`] lists them: the drawing
    /// has them already, so not listed again here.
    pub shown: &'a [Shown],
}

/// A unit to draw: its descriptor, set 300 frame, pixel and colour.
struct Blip {
    code: FourCc,
    frame: u32,
    at: (i32, i32),
    colour: Option<Rgb>,
}

/// The whole minimap over `ground` ([`super::Cache::picture`]): reach
/// circles, then each list's units in order, then the views' frames.
///
/// **Inferred**: a list's own order is by agent, marks after the
/// characters in the order made, the victory star last.
#[must_use]
pub fn draw(
    ground: &Picture,
    world: &World,
    catalogue: &Catalogue<'_>,
    looks: &mut Looks,
    scene: &Scene<'_>,
    scale: Scale,
) -> Picture {
    let mut picture = ground.clone();
    let pixel = |(x, y): (i32, i32)| (scale.pixel(x), scale.pixel(y));

    for (centre, radius) in scene.reach {
        picture.ring(pixel(*centre), scale.pixel(*radius), narrow(REACH));
    }

    for blip in blips(world, catalogue, looks, scene) {
        let look = looks.of(catalogue, blip.code);

        put(
            &mut picture,
            look,
            blip.code,
            blip.frame,
            pixel(blip.at),
            blip.colour,
        );
    }

    for (rect, colour) in scene.frames {
        picture.frame(*rect, narrow(*colour));
    }

    picture
}

/// The units the minimap shows, in draw order, at their points in 32nds.
fn blips(
    world: &World,
    catalogue: &Catalogue<'_>,
    looks: &mut Looks,
    scene: &Scene<'_>,
) -> Vec<Blip> {
    let mut out = Vec::new();

    for list in LISTS {
        let members = scene
            .shown
            .iter()
            .filter(|s| list_of(world, s.agent) == Some(list));

        for s in members {
            if let Some(blip) = piece(world, catalogue, looks, scene, (list, s)) {
                out.push(blip);
            }
        }

        match list {
            List::Vehicles => out.extend(marks(world, looks, scene)),
            List::Messages => out.extend(messages(world, catalogue, looks, scene)),
            List::Buildings | List::RewardFlags => {}
        }
    }

    out
}

/// The unit list piece `agent` is on, by its unit kind.
fn list_of(world: &World, agent: u32) -> Option<List> {
    match world.piece(agent)?.place?.kind {
        Kind::BUILDING => Some(List::Buildings),
        Kind::VEHICLE => Some(List::Vehicles),
        Kind::REWARD => Some(List::RewardFlags),
        _ => None,
    }
}

/// Piece `s` of `list`, if shown and not blinking.
fn piece(
    world: &World,
    catalogue: &Catalogue<'_>,
    looks: &mut Looks,
    scene: &Scene<'_>,
    (list, s): (List, &Shown),
) -> Option<Blip> {
    let look = looks.of(catalogue, s.code);
    let attribute = |key| world.word_of(s.agent, key);
    let standing = if s.ruined {
        Standing::Down
    } else {
        Standing::Standing
    };
    let cloaked = if [HAS_EFFECT_INVISIBILITY, CAMOUFLAGED, CLONE_PARENT]
        .iter()
        .any(|a| attribute(*a) != 0)
    {
        Cloak::Cloaked
    } else {
        Cloak::Open
    };
    let class = match list {
        List::Buildings => Class::Building {
            listed: look.listed,
            standing,
        },
        List::Vehicles if look.kind == Some(UnitKind::Character) => Class::Character {
            listed: look.listed,
            cloaked,
        },
        List::Vehicles => Class::Missile,
        List::RewardFlags | List::Messages => Class::Other,
    };
    let held = if s.housed == Housed::Held {
        Held::Held
    } else {
        Held::Free
    };
    let seen = seen_by(scene.eyes, |mask| world.seen_level(s.agent, mask));
    let visible = shown(class, seen, world.overhead(s.agent), held, scene.filter);
    let flashing = world.flash_count(s.agent).is_some_and(blinked);

    let owner = u8::try_from(s.owner).ok();
    let index = colour(owner, scene.local, s.code, standing, lit(world, scene, s));

    (visible && !flashing).then(|| Blip {
        code: s.code,
        frame: 0,
        at: s.at,
        colour: index.map(|i| looks.colour(i)),
    })
}

/// The seen level `level` gives with `eyes`' mask; with [`Eyes::ALL`],
/// full sight.
fn seen_by(eyes: Eyes, level: impl FnOnce(u8) -> i32) -> i32 {
    if eyes == Eyes::ALL {
        crate::sim::world::reveal::SEEN
    } else {
        level(eyes.mask())
    }
}

/// Why piece `s` draws white, as minimap colour asks.
fn lit(world: &World, scene: &Scene<'_>, s: &Shown) -> Lit {
    let selection = scene.selected.and_then(|id| Some((id, world.piece(id)?)));
    let market = selection.is_some_and(|(_, p)| {
        p.owner == u32::from(scene.local) && MARKETS.iter().any(|m| p.code.0.starts_with(m))
    });

    match selection {
        Some((id, _)) if id == s.agent => Lit::Selected,
        _ if market && s.code == CARAVAN => Lit::Trade,
        Some((id, _)) if world.kin(s.agent, id) => Lit::Kin,
        _ => Lit::Not,
    }
}

/// The minimap marks standing that the local player sees, then the won
/// game's victory star: list 1 units, behind the mobiles' filter.
fn marks(world: &World, looks: &Looks, scene: &Scene<'_>) -> Vec<Blip> {
    let eyes = scene.eyes;
    let marks = world
        .minimap_marks()
        .into_iter()
        .filter(|m| eyes == Eyes::ALL || world.mark_seen(m, eyes.mask()))
        .map(|m| {
            let owner = u8::try_from(m.owner).ok();
            let index = colour(owner, scene.local, m.code, Standing::Standing, Lit::Not);

            Blip {
                code: m.code,
                frame: m.frame(world.now()).unwrap_or(0),
                at: m.at,
                colour: index.map(|i| looks.colour(i)),
            }
        });

    // **Inferred**: drawn as the marks are, though app state 8 runs no
    // sight pass to see it by.
    let local = u32::from(scene.local);
    let star = world
        .end_point(local)
        .filter(|at| world.ended(local) == Some(End::Won) && *at != NO_POINT)
        .map(|at| Blip {
            code: VICTORY_STAR,
            frame: 0,
            at,
            colour: None,
        });

    if scene.filter.has(Filter::MOBILES) {
        marks.chain(star).collect()
    } else {
        Vec::new()
    }
}

/// The message flags standing, not held: owner −1, so in their own
/// colours. One alone is seen by all (its sight words are 100); a hung
/// one as its holder.
fn messages(
    world: &World,
    catalogue: &Catalogue<'_>,
    looks: &mut Looks,
    scene: &Scene<'_>,
) -> Vec<Blip> {
    let wave = looks.of(catalogue, MESSAGE_FLAG).wave();
    let eyes = scene.eyes;

    world
        .effects()
        .into_iter()
        .filter(|e| e.code == MESSAGE_FLAG && !e.held)
        .filter_map(|e| {
            let at = if e.holder == 0 {
                crate::sim::movement::pos32(e.at)
            } else {
                world.position(e.holder)?
            };
            let seen = seen_by(eyes, |mask| world.effect_seen(&e, mask));
            let since = match e.look {
                Playing::Plays { since, .. } => since,
                Playing::Frame { .. } => world.now(),
            };
            let frame = wave.map_or(0, |w| w.frame(world.now().saturating_sub(since)));
            let visible = shown(
                Class::Other,
                seen,
                Overhead::Shown,
                Held::Free,
                Filter::DEFAULT,
            );

            visible.then_some(Blip {
                code: e.code,
                frame,
                at,
                colour: None,
            })
        })
        .collect()
}
