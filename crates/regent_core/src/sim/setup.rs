//! A new game's world, built from HD's archives and a start save.
//!
//! ```text
//!  archives ──> Catalogue ──> descriptors: shapes, bodies, names
//!                         └─> lookups: actions, particles, spells,
//!                                      name generators
//!  start save ──> World::new_game ──> named, measured, every builtin on
//! ```
//!
//! A descriptor or art that will not parse is skipped, its field
//! defaulted, at each `.ok()` below: a table is best effort, and none in
//! HD's two datasets is refused (`tests/fresh.rs`,
//! `every_record_the_tables_read_parses`).

use crate::sim::freestyle::Setup;
use crate::sim::world::piece_state;
use std::collections::BTreeSet;

use crate::base::digest::Map;

use crate::content::unit::Unit;
use crate::data::catalogue::Catalogue;
use crate::data::desc::{self, Info, UnitKind};
use crate::data::fourcc::FourCc;
use crate::data::imag::{Image, SetKind};
use crate::data::text::Text;
use crate::gpl::load::Program;
use crate::sim::movement::{self, action::Pose};
use crate::sim::placement::Heights;
use crate::sim::world::particles::{Spec, Sprite};
use crate::sim::world::{
    EVENT_TABLE, Exposure, NO_EVENT, Shape, Shapes, Sight, World,
    animate::animated,
    bodies::{Bodies, Body, Class, Extras, Facing, Named, Placing, Poses, initial},
    build,
    building_ground::Lays,
    descriptors::Descriptors,
    draw::Overhead,
    event_rating, event_script,
    missile::{Hit, Launch, Missile, Script},
    naming::Generators,
    new_map::{Founding, OnGrid, Raised, Scatter, Seat, mend},
    offer::Menu,
    placing::LAYERS,
    plume::{FIRST_SETS, Lifespan, Particles, vents},
    price::fixed10,
    spell::Spell,
    walking::Act,
};
use crate::sim::{clearing, placement};

/// What the world reads of the archives.
pub struct Tables {
    /// Every descriptor, and what orders read of them by name.
    pub descriptors: Descriptors,
    pub generators: Generators,
}

impl Tables {
    /// Every table, from `catalogue`.
    #[must_use]
    pub fn of(catalogue: &Catalogue<'_>) -> Tables {
        let (named, generators) = lookups(catalogue);

        Tables {
            descriptors: descriptors(catalogue).with_named(named),
            generators,
        }
    }

    /// Every table, with the descriptors `made` already, as
    /// [`descriptors`] gave them for `catalogue`.
    #[must_use]
    pub fn with(catalogue: &Catalogue<'_>, made: &Descriptors) -> Tables {
        let (named, generators) = lookups(catalogue);

        Tables {
            descriptors: made.clone().with_named(named),
            generators,
        }
    }
}

/// What a new game is made from with no save: the generated ground and
/// scatter, the start's units where they stand, the custom game's picks.
pub struct Start<'a> {
    /// Heights before any building levels them.
    pub ground: &'a dyn Heights,
    /// Paint before any building paves it.
    pub cells: &'a [crate::sim::paint::Cell],
    /// Every scatter object in the order placed: code and half cell.
    pub scatter: &'a [(FourCc, (i32, i32))],
    /// Each scatter object's art slot.
    pub slots: &'a [u16],
    /// In the order the unit chart made them.
    pub units: &'a [Raised],
    pub setup: Setup,
    /// The players made ([`Seat::freestyle`], a quest's slots).
    pub seats: &'a [Seat],
    /// The human seats, slots 0 up ([`World::humans`]).
    pub session: u32,
    /// The two special events, `EVSC` keys or `NONE`.
    pub events: [FourCc; 2],
}

/// A special event left unpicked.
pub const NO_EVENTS: FourCc = FourCc::new(b"NONE");

/// The scripts `picked`'s `EVSC` entries name ([`event_script`]); an
/// absent key, [`NO_EVENT`] (**Inferred**: the dialog offers only keys
/// the table has).
#[must_use]
pub fn events(catalogue: &Catalogue<'_>, picked: [FourCc; 2]) -> [String; 2] {
    let text = Text::build(catalogue);
    let script = |code: FourCc| {
        text.keyed(EVENT_TABLE, code)
            .map_or_else(|| NO_EVENT.to_owned(), |e| event_script(&e))
    };

    picked.map(script)
}

/// Every `EVSC` entry in table order, `NONE` first: its key and rating
/// ([`event_rating`]), an entry that does not scan rated 0.
#[must_use]
pub fn event_list(catalogue: &Catalogue<'_>) -> Vec<(FourCc, i32)> {
    let text = Text::build(catalogue);
    let Some(table) = text.table(EVENT_TABLE) else {
        return Vec::new();
    };

    (0..table.len())
        .filter_map(|i| {
            let rating = table.get(i).and_then(|e| event_rating(&e)).unwrap_or(0);

            Some((table.key(i)?, rating))
        })
        .collect()
}

/// The new game `start` makes, measured by `catalogue`, its tick from
/// `program`'s constants: what
/// [`new_game`] reads from a start save, made instead
/// ([`World::fresh`]).
#[must_use]
pub fn fresh(catalogue: &Catalogue<'_>, program: &Program, start: &Start<'_>) -> World {
    founded(catalogue, program, start, Tables::of(catalogue))
}

/// [`fresh`], the descriptors `made` of `catalogue` already ([`descriptors`]):
/// they are measured once.
#[must_use]
pub fn fresh_by(
    catalogue: &Catalogue<'_>,
    program: &Program,
    start: &Start<'_>,
    made: &Descriptors,
) -> World {
    founded(catalogue, program, start, Tables::with(catalogue, made))
}

fn founded(
    catalogue: &Catalogue<'_>,
    program: &Program,
    start: &Start<'_>,
    tables: Tables,
) -> World {
    let scatter: Vec<Scatter> = start
        .scatter
        .iter()
        .zip(start.slots.iter().copied().chain(std::iter::repeat(0)))
        .map(|(&(code, at), slot)| Scatter {
            code,
            at,
            grid: grid(catalogue, code),
            slot,
        })
        .collect();
    let founding = Founding {
        ground: start.ground,
        cells: start.cells,
        scatter: &scatter,
        units: start.units,
        setup: start.setup,
        seats: start.seats,
        session: start.session,
        mend: mend(program),
    };

    World::fresh(&founding, tables.descriptors)
        .with_events(events(catalogue, start.events))
        .with_names(tables.generators)
        .with_advisors(&Menu::of(catalogue), program)
}

/// Whether `code`'s units stand on the collision grid: off with `Info`
/// `DontBlock`.
fn grid(catalogue: &Catalogue<'_>, code: FourCc) -> OnGrid {
    let off = Unit::find(catalogue, code)
        .and_then(std::result::Result::ok)
        .is_some_and(|u| u.desc.engine().has(Info::DontBlock));

    if off { OnGrid::Off } else { OnGrid::On }
}

/// A unit's art as its actions' `InitAction`s play it: each set by id,
/// the first of an id as set by id scans.
fn poses(unit: &Unit<'_>) -> Option<Poses> {
    let image = unit.image.as_ref()?;

    Some(
        image
            .sets
            .iter()
            .rev()
            .map(|s| (s.id, Pose::of(s)))
            .collect(),
    )
}

/// The set id a new `unit`'s record names ([`Body::recorded`]): the
/// last layer of [`Unit::initial`]'s set.
pub fn recorded(unit: &Unit<'_>) -> Option<u32> {
    let first = unit.initial()?;
    let low = crate::data::imag::set_number(first.id);
    let last = unit
        .animations
        .iter()
        .rfind(|a| crate::data::imag::set_number(a.id) == low)
        .map_or(first.id, |a| a.id);

    Some(last)
}

/// Where a missile at the unit aims, from its art.
fn hit(unit: &Unit<'_>) -> Hit {
    unit.image.as_ref().map_or_else(Default::default, Hit::of)
}

/// Where the unit's missiles leave it, from its art.
fn launch(unit: &Unit<'_>) -> Launch {
    unit.image
        .as_ref()
        .map_or_else(Default::default, Launch::of)
}

/// The unit's pad in ground pixels: width and height.
fn span(catalogue: &Catalogue<'_>, unit: &Unit<'_>) -> (i32, i32) {
    placement::pixel_rect(catalogue, unit)
        .ok()
        .flatten()
        .map_or((0, 0), |[x0, y0, x1, y1]| (x1 - x0, y1 - y0))
}

/// A building's price, 0 for anything else.
fn building_cost(desc: &desc::Unit<'_>) -> i32 {
    desc.building()
        .and_then(|b| b.cost())
        .or_else(|| desc.character()?.cost())
        .unwrap_or_default()
        .cast_signed()
}

/// A building's `Multiplier` in scale 1024.
fn multiplier(desc: &desc::Unit<'_>) -> i32 {
    desc.building()
        .and_then(|b| b.multiplier())
        .map_or(0, fixed10)
}

/// A character's or building's `NameGenType`.
fn name_gen_type(desc: &desc::Unit<'_>) -> Option<FourCc> {
    desc.character()
        .map_or_else(|| desc.building()?.name_gen_type(), |c| c.name_gen_type())
}

/// Every descriptor in `catalogue`, as the world places, measures and
/// names them: pad rect and sight, body, and every `HEAD` name, folded;
/// nothing by name ([`lookups`]).
#[must_use]
pub fn descriptors(catalogue: &Catalogue<'_>) -> Descriptors {
    let codes: BTreeSet<FourCc> = catalogue
        .tagged(b"DUNT")
        .filter_map(|(_, r)| {
            Some(
                desc::Head::parse(desc::Part::parse(r.data).ok()?.find(b"HEAD")?.body)
                    .ok()?
                    .id,
            )
        })
        .collect();
    let described = [b"DUNT", b"DACT"]
        .iter()
        .flat_map(|tag| catalogue.tagged(tag))
        .filter_map(|(_, r)| {
            let part = desc::Part::parse(r.data).ok()?;
            let head = desc::Head::parse(part.find(b"HEAD")?.body).ok()?;

            Some(String::from_utf8_lossy(head.name).to_ascii_lowercase())
        })
        .collect();
    let gaits: Map<FourCc, movement::Class> = catalogue
        .tagged(b"DMOV")
        .filter_map(|(_, r)| Some((r.id, movement::Class::parse(r.data).ok()?)))
        .collect();
    // An art that will not parse leaves every unit without a ring.
    let ring = clearing::ring_art(catalogue).ok();
    let mut shapes = Shapes::default();
    let mut bodies = Bodies::default();

    for code in codes {
        if let Some(Ok(unit)) = Unit::find(catalogue, code) {
            if let Some(shape) = shape(catalogue, &unit) {
                shapes.insert(code, shape);
            }

            bodies.insert(code, body(catalogue, &unit, code, (&gaits, ring.as_ref())));
        }
    }

    Descriptors::new(shapes, bodies, described)
}

/// Who may see a unit its owner does not.
fn sight(unit: &Unit<'_>) -> Sight {
    if unit.desc.engine().has(Info::NotVisibleToOpponent) {
        Sight::Owner
    } else {
        Sight::Everyone
    }
}

/// A unit's pad rect and sight, where its art gives a pad.
fn shape(catalogue: &Catalogue<'_>, unit: &Unit<'_>) -> Option<Shape> {
    let rect = placement::pad_rect(catalogue, unit).ok()??;
    let bounds = placement::pixel_rect(catalogue, unit).ok()??;

    Some(Shape {
        rect,
        bounds,
        sight: sight(unit),
        exposure: Exposure::of(&unit.desc),
    })
}

/// What the world reads of one descriptor.
#[allow(clippy::too_many_lines, reason = "a field a line")]
fn body(
    catalogue: &Catalogue<'_>,
    unit: &Unit<'_>,
    code: FourCc,
    (gaits, ring_art): (&Map<FourCc, movement::Class>, Option<&Option<Image<'_>>>),
) -> Body {
    let engine = unit.desc.engine();
    let (character, building) = (unit.desc.character(), unit.desc.building());

    Body {
        name: String::from_utf8_lossy(unit.desc.head.name).into_owned(),
        class: match unit.desc.kind() {
            UnitKind::Building => Class::Building,
            UnitKind::Character => Class::Character,
            UnitKind::Overlay => Class::Overlay,
            UnitKind::ParticleSystem => Class::Particles,
            UnitKind::General => Class::General,
            UnitKind::Projectile => Class::Projectile,
        },
        layer: engine.info().unwrap_or_default() & LAYERS,
        cells: placement::taken(catalogue, unit)
            .ok()
            .flatten()
            .unwrap_or_default(),
        edge: placement::edge(catalogue, unit),
        exits: placement::exits(catalogue, unit)
            .ok()
            .flatten()
            .unwrap_or_default(),
        facing: Facing::of(&unit.desc),
        attributes: match (&character, &building) {
            (Some(c), _) => initial(c),
            (None, Some(b)) => build::initial(code, b),
            (None, None) => Vec::new(),
        },
        lists: match &character {
            Some(c) if !c.lists().is_empty() => Extras::Held,
            _ => Extras::Empty,
        },
        spells: character
            .as_ref()
            .map(desc::Character::allowed)
            .map(|a| a.spells.iter().map(|(_, id)| *id).collect())
            .unwrap_or_default(),
        heals: match (&character, &building) {
            (Some(c), _) => c.has(desc::CharacterFlag::Heals),
            (None, Some(b)) => b.has(desc::BuildingFlag::Heals),
            (None, None) => false,
        },
        not_hero: character
            .as_ref()
            .is_some_and(|c| c.has(desc::CharacterFlag::NotHero)),
        item: character
            .as_ref()
            .is_some_and(|c| c.has(desc::CharacterFlag::IsInventoryItem)),
        guild: building
            .as_ref()
            .is_some_and(|b| b.has(desc::BuildingFlag::IsGuild)),
        income: building
            .as_ref()
            .and_then(desc::Building::income_type)
            .unwrap_or_default(),
        gait: unit
            .desc
            .tail()
            .attachments
            .and_then(|a| a.movement_class())
            .and_then(|(_, id)| gaits.get(&id).copied()),
        mask: placement::shape(catalogue, unit).ok().flatten(),
        ring: ring_art.and_then(|art| {
            clearing::ring_in(catalogue, unit, clearing::MARGIN, art.as_ref())
                .ok()
                .flatten()
        }),
        lays: if engine.has(Info::ModifyTerrainTextureOnPlacement) {
            Lays::Dirt
        } else {
            Lays::Nothing
        },
        cost: building_cost(&unit.desc),
        multiplier: multiplier(&unit.desc),
        delay: character
            .as_ref()
            .and_then(desc::Character::recruit_delay)
            .unwrap_or_default()
            .cast_signed(),
        names: name_gen_type(&unit.desc),
        dialog: match (&character, &building) {
            (Some(c), _) => c.dialog_id(),
            (None, Some(b)) => b.dialog_id(),
            (None, None) => None,
        },
        tally: Some(crate::sim::world::numbered::tally_key(catalogue, code)),
        numbered: crate::sim::world::numbered::numbered_key(catalogue, unit),
        poses: poses(unit),
        made_in: unit.initial().map(|a| a.id),
        slots: unit
            .initial()
            .map_or(0, |set| u16::try_from(set.poses.len()).unwrap_or(u16::MAX)),
        placing: if unit.animation(SetKind::Inactive).is_some() {
            Placing::Placed
        } else {
            Placing::Left
        },
        recorded: recorded(unit),
        upgrade: building.as_ref().and_then(desc::Building::upgrade_to),
        upgraded: building.as_ref().and_then(desc::Building::upgrade_from),
        base_hp: building
            .as_ref()
            .and_then(desc::Building::upgrade_from)
            .and_then(|from| Unit::find(catalogue, from)?.ok())
            .and_then(|from| from.desc.building()?.max_hp())
            .map_or(0, u32::cast_signed),
        hit: hit(unit),
        launch: launch(unit),
        script: Some(Script {
            function: String::from_utf8_lossy(unit.desc.tail().script).into_owned(),
            kind: unit.desc.script_kind().unwrap_or_default(),
        }),
        attach: match unit.desc.kind() {
            UnitKind::Overlay => engine.attach_point().unwrap_or_default(),
            _ => 0,
        },
        stack: if engine.has(Info::NotVisibleInIsoView) {
            0
        } else {
            unit.desc.stack_priority().unwrap_or_default()
        },
        span: span(catalogue, unit),
        overhead: if engine.has(Info::NotVisibleInOverheadView) {
            Overhead::Hidden
        } else {
            Overhead::Shown
        },
        vents: unit.image.as_ref().map_or_else(Default::default, vents),
        animated: unit.image.as_ref().map_or_else(Default::default, animated),
        first: unit.image.as_ref().and_then(first_set),
        particles: particles(unit),
        state: made_state(unit),
        spread: Missile::of(code, engine.info().unwrap_or_default()).spread,
        sight: sight(unit),
    }
}

/// `UnitIMG`'s first set: the first of [`FIRST_SETS`] the art has,
/// else its first (slot 0).
/// Unit state's low byte as `GenUnit` sets it from its
/// descriptor: [`crate::sim::world::piece_state::DONT_BLOCK`] for `Info` `DontBlock`.
fn made_state(unit: &Unit<'_>) -> u8 {
    if unit.desc.engine().has(Info::DontBlock) {
        piece_state::DONT_BLOCK
    } else {
        0
    }
}

fn first_set(image: &Image<'_>) -> Option<u32> {
    FIRST_SETS
        .iter()
        .copied()
        .find(|id| image.sets.iter().any(|s| s.id == *id))
        .or_else(|| image.sets.first().map(|s| s.id))
}

/// A particle system's lifespans, unit state bits (`DontBlock`),
/// `AttachmentPointID`, particle rules and sprite: its art's first set
/// (**Inferred**: a system's unit plays no other).
fn particles(unit: &Unit<'_>) -> Option<Particles> {
    let system = unit.desc.particle_system()?;
    let emitter = system.emitter().lifespan()?;
    let lifespan = Lifespan {
        emitter: (emitter != u32::MAX).then_some(emitter),
        particle: system.particle().lifespan()?,
    };
    let state = made_state(unit);

    let sprite = unit
        .image
        .as_ref()
        .and_then(|i| Some(Sprite::of(i, first_set(i)?)))
        .unwrap_or_default();

    Some(Particles {
        lifespan,
        state,
        attach: system.attach_point().unwrap_or_default(),
        spec: Spec::of(&system)?,
        sprite,
    })
}

/// `catalogue`'s actions and particle systems by folded name, and its 17
/// name generators over its `STRT` tables, a later archive's table
/// winning.
#[must_use]
pub fn lookups(catalogue: &Catalogue<'_>) -> (Named, Generators) {
    let parts = |tag: &'static [u8; 4]| {
        catalogue
            .tagged(tag)
            .filter_map(|(_, r)| desc::Part::parse(r.data).ok())
            .collect::<Vec<_>>()
    };
    let acts = parts(b"DACT");
    let actions = acts
        .iter()
        .filter_map(|p| desc::Action::parse(p).ok())
        .map(|a| {
            (
                String::from_utf8_lossy(a.head.name).to_ascii_lowercase(),
                Act {
                    rate: a.rate().map_or(0, |(_, max)| max),
                    set: a.image_set().unwrap_or(0),
                    after: a.after_set().unwrap_or(0),
                    proc: a.script().c_proc,
                    overlay: a.overlay(),
                    sound: a.sound(),
                    begin: a.sound_phases()[0],
                    peak: a.sound_phases()[1],
                    projectile: a.projectile().map(|code| {
                        let info = Unit::find(catalogue, code)
                            .and_then(std::result::Result::ok)
                            .and_then(|u| u.desc.engine().info())
                            .unwrap_or_default();

                        Missile::of(code, info)
                    }),
                },
            )
        })
        .collect();
    let particles = parts(b"DUNT")
        .iter()
        .filter_map(|p| desc::Unit::parse(p).ok())
        .filter(|u| u.kind() == UnitKind::ParticleSystem)
        .map(|u| String::from_utf8_lossy(u.head.name).to_ascii_lowercase())
        .collect();
    let spells = acts
        .iter()
        .filter_map(|p| desc::Action::parse(p).ok())
        .map(|a| {
            (
                String::from_utf8_lossy(a.head.name).to_ascii_lowercase(),
                Spell::of(&a),
            )
        })
        .collect();
    let scripts = acts
        .iter()
        .filter_map(|p| desc::Action::parse(p).ok())
        .filter_map(|a| {
            let script = a.script();
            let function = String::from_utf8_lossy(script.function).into_owned();

            (!function.is_empty()).then(|| {
                (
                    String::from_utf8_lossy(a.head.name).to_ascii_lowercase(),
                    Script {
                        function,
                        kind: script.kind,
                    },
                )
            })
        })
        .collect();
    let codes = catalogue
        .tagged(b"DACT")
        .filter_map(|(_, r)| {
            let part = desc::Part::parse(r.data).ok()?;
            let action = desc::Action::parse(&part).ok()?;

            Some((
                r.id,
                String::from_utf8_lossy(action.head.name).to_ascii_lowercase(),
            ))
        })
        .collect();
    let text = Text::build(catalogue);
    let count = |id: FourCc| {
        text.table(&id.0)
            .map_or(0, |t| u32::try_from(t.len()).unwrap_or(0))
    };

    (
        Named {
            actions,
            particles,
            spells,
            scripts,
            codes,
        },
        Generators::new(count, 0),
    )
}
