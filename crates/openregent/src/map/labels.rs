//! The labels on the map: each reward flag's reward over it, and units'
//! names under them, by `[ui] names` ([`regent_core::view::map::label`]).
//!
//! ```text
//!  each drawn piece
//!    a reward flag ──> World::reward; its foot on its pad rect's top
//!    else its name, where `names` takes it and a building stands
//!      ──> label::name: its own name or title, a level from 1; its foot
//!          12 under the rect its bar hangs on, centred on it
//!    feet made once a descriptor, less fnt7's rows under the letters
//!    ──> Words at the piece's place plus its foot, painted by egui
//!  held or unseen ──> hidden; a view's zoom, at its paint (words::shown_at)
//!  the reward, level or descriptor changed ──> remade
//! ```
//!
//! The engine draws both with the unit, after its hit
//! point bar, right after its picture in the draw list (**Binary**:
//! `IsoGameView` passes part 0 as -1; no later pass
//! draws units): a nearer unit covers them. Here over every piece
//! instead, as egui paints it, in board y order (`words::Tie`).

use std::collections::HashMap;

use bevy::prelude::*;

use regent_core::content::unit::Unit;
use regent_core::data::attrib;
use regent_core::data::fourcc::FourCc;
use regent_core::sim::world::World;
use regent_core::sim::world::chat::Who;
use regent_core::sim::world::draw_view::Shown;
use regent_core::view::map::bar::Body;
use regent_core::view::map::cursor::Flagged;
use regent_core::view::map::label;
use regent_core::view::panel::portrait;
use regent_draw::data::Corpus;

use crate::files::config::{Names, Settings};
use crate::game::seen::Watch;
use crate::map::mirror::Pieces;
use crate::map::pieces::Piece;
use crate::map::select::flagged;
use crate::map::words::{self, FLAG_FOOT, Tie, Words};

/// The labels drawn, by agent, with what each was made from; each
/// descriptor's art, made once.
#[derive(Resource, Default)]
pub struct Labels {
    drawn: HashMap<u32, (Entity, LabelText)>,
    arts: HashMap<FourCc, Option<Art>>,
}

/// The names check's last shown setting, for [`Names::toggled`].
#[derive(Resource, Default)]
pub struct LastNames(pub Names);

/// A label.
#[derive(Component)]
pub struct Label;

/// What a label's text is made from: remade when it changes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum LabelText {
    /// A reward flag's reward.
    Reward(i32),
    /// A unit's name: what names it and its level (`GSUnit`
    /// remakes it as `APV:11` is written; a rename redraws its view).
    Name(Who, i32, Option<i32>),
}

/// What a descriptor gives its labels: its body, and each foot, pixels
/// right and down from the screen place.
#[derive(Clone, Copy, Debug)]
struct Art {
    body: Body,
    over: (i32, i32),
    under: (i32, i32),
}

/// Keep a label on every reward flag drawn, and a name on every unit
/// `[ui] names` takes; hidden with its piece. Whether a view shows them
/// at its zoom is `words::shown_at`'s, as it paints.
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
    mirrors: Pieces,
    seen: Watch,
    mut labels: ResMut<Labels>,
    pieces: Query<(&Transform, &Visibility), (With<Piece>, Without<Label>)>,
    mut drawn: Query<(&mut Transform, &mut Visibility, &mut Words), With<Label>>,
) {
    let world = seen.world();
    let names = settings.ui.names;
    let Labels {
        drawn: labels_drawn,
        arts,
    } = &mut *labels;

    // Each label: where, whether seen, what it shows, its paint order.
    let wanted: HashMap<u32, (Vec3, Visibility, LabelText, Tie)> = mirrors
        .iter()
        .filter_map(|shown| {
            let art = (*arts
                .entry(shown.code)
                .or_insert_with(|| art(&corpus, shown.code)))?;
            let (source, (x, y)) = match flagged(shown) {
                Flagged::Yes => (LabelText::Reward(world.reward(shown.agent)?), art.over),
                Flagged::No => {
                    let hero = || world.is_hero(shown.agent);
                    let shows = takes(names, hero) && label::standing(art.body, shown.ruined);

                    shows.then_some(())?;
                    (name_of(world, shown.code, shown.agent), art.under)
                }
            };
            let (place, seen) = pieces.get(mirrors.drawn(shown.agent)?).ok()?;
            let seen = *seen;

            #[allow(clippy::cast_precision_loss, reason = "a few dozen pixels")]
            let at = place.translation + Vec3::new(x as f32, -y as f32, 0.0);

            let tie = Tie {
                y: shown.at.1,
                agent: shown.agent,
            };

            Some((shown.agent, (at, seen, source, tie)))
        })
        .collect();

    // Gone, or made from something else: gone from the screen.
    labels_drawn.retain(|agent, (entity, source)| {
        let keep = wanted.get(agent).is_some_and(|w| w.2 == *source);

        if !keep {
            commands.entity(*entity).despawn();
        }

        keep
    });

    for (agent, (at, seen, source, tie)) in wanted {
        let kept = labels_drawn
            .get(&agent)
            .and_then(|(e, _)| drawn.get_mut(*e).ok());

        if let Some((mut transform, mut visibility, mut words)) = kept {
            transform.set_if_neq(Transform {
                translation: at,
                ..*transform
            });
            visibility.set_if_neq(seen);
            words.tie = tie;
        } else {
            let words = Words {
                text: text(&corpus, &source),
                look: words::FLAG,
                tie,
            };
            let entity = commands
                .spawn((Label, words, Transform::from_translation(at), seen))
                .id();

            labels_drawn.insert(agent, (entity, source));
        }
    }
}

/// Whether `names` shows a unit's name; `hero` asked only for
/// [`Names::Heroes`].
fn takes(names: Names, hero: impl FnOnce() -> bool) -> bool {
    match names {
        Names::Off => false,
        Names::On => true,
        Names::Heroes => hero(),
    }
}

/// Agent `agent`'s name, of descriptor `code`, while the engine shows
/// names under setting `names` (`DisplayFlagNames`): `heroes` only
/// filters what it shows, so counts as shown.
pub fn engine_name(
    corpus: &Corpus,
    names: Names,
    world: &World,
    (agent, code): (u32, FourCc),
) -> Option<String> {
    match names {
        Names::Off => None,
        Names::On | Names::Heroes => Some(text(corpus, &name_of(world, code, agent))),
    }
}

/// What `shown`'s label is made from while the engine boxes it:
/// a reward flag's always, a name while names show
/// (`heroes` only filters what it shows).
pub(crate) fn engine_source(names: Names, world: &World, shown: &Shown) -> Option<LabelText> {
    match (flagged(shown), names) {
        (Flagged::Yes, _) => Some(LabelText::Reward(
            world.reward(shown.agent).unwrap_or_default(),
        )),
        (Flagged::No, Names::Off) => None,
        (Flagged::No, Names::On | Names::Heroes) => Some(name_of(world, shown.code, shown.agent)),
    }
}

/// Agent `agent`'s name label's source, of descriptor `code`.
fn name_of(world: &World, code: FourCc, agent: u32) -> LabelText {
    LabelText::Name(
        portrait::who(world, agent, code),
        level(world, agent),
        portrait::dropped_item(world, agent),
    )
}

/// Agent `id`'s level, `APV:11`, as its name shows it (`0xb565041` at
/// `GSUnit`); 0 unset.
fn level(world: &World, id: u32) -> i32 {
    world.word_of(id, attrib::EXPERIENCE_LEVEL)
}

/// A label's text: a reward's; a name as `GSUnit` writes it.
pub(crate) fn text(corpus: &Corpus, source: &LabelText) -> String {
    match source {
        LabelText::Reward(reward) => label::text(*reward),
        LabelText::Name(who, level, item) => item
            .and_then(|key| {
                portrait::details::item(&corpus.catalogue, &corpus.text, key).map(|(name, _)| name)
            })
            .or_else(|| {
                Unit::find(&corpus.catalogue, who.code)
                    .and_then(Result::ok)
                    .map(|unit| {
                        let own = portrait::own_name(&corpus.catalogue, &corpus.text, who);
                        let title = unit.desc.head.title();

                        label::name(Body::of(&unit), Some(&own), &title, *level)
                    })
            })
            .unwrap_or_default(),
    }
}

/// Descriptor `code`'s body and feet: a reward's box bottom on the pad
/// rect's top (`label::placed`), a name's 12 under its bottom, centred
/// (`label::below`); each less the rows under `fnt7`'s letters. `None`
/// without art.
fn art(corpus: &Corpus, code: FourCc) -> Option<Art> {
    let catalogue = &corpus.catalogue;
    let unit = Unit::find(catalogue, code)?.ok()?;
    let [x, _, _, over] = label::placed(catalogue, &unit, (0, 0)).ok()??;
    let [mid, _, _, under] = label::below(catalogue, &unit, (0, 0)).ok()??;

    Some(Art {
        body: Body::of(&unit),
        over: (x, over - FLAG_FOOT),
        under: (mid, under - FLAG_FOOT),
    })
}

/// The names drawn and the names check's last setting, a game's.
pub fn plugin(app: &mut bevy::prelude::App) {
    use crate::game::scope::Scoped as _;

    app.per_game::<Labels>().per_game::<LastNames>();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `on` names every unit, `heroes` heroes alone, `off` none.
    #[test]
    fn each_names_setting_takes_its_units() {
        assert!(takes(Names::On, || false));
        assert!(takes(Names::Heroes, || true));
        assert!(!takes(Names::Heroes, || false));
        assert!(!takes(Names::Off, || true));
    }

    /// A unit renamed has its label made again: the name is in its
    /// source (`World::rename` redraws its view, method).
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_rename_makes_the_name_label_again() {
        let (begun, _) = crate::game::begin::test_game(&[]);
        let mut world = begun.sim.world().clone();
        let palace = world.player_palace(begun.sim.local()).expect("a palace");
        let code = world.piece(palace).expect("placed").code;
        let before = name_of(&world, code, palace);

        world.rename(&regent_core::sim::world::naming::Rename::new(
            palace, "Home",
        ));

        assert_ne!(name_of(&world, code, palace), before);
    }
}
