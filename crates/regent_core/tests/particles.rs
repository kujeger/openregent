//! Particles: what the world reads of each shipped particle system.
//! `sim::world::particles`.
//!
//! Needs `MAJESTY_HD`.

mod common;

use regent_core::data::desc::EmitterShape;
use regent_core::data::fourcc::FourCc;
use regent_core::sim::world::particles::Sprite;

/// `Smoke_Large`, a burning building's: an area column of radius 0.5, 3
/// births a second, each 4 s give or take a fifth, no more than 10, in a
/// box 10 cells up; its sprite is its art's one `Special` slot, 38 frames
/// at 100 ms.
#[test]
fn a_burning_buildings_smoke_reads_its_descriptor() {
    let bodies = common::bodies();
    let smoke = bodies[&FourCc(*b"XLSL")]
        .particles
        .clone()
        .expect("a particle system");
    let spec = smoke.spec;
    let column = 1 << EmitterShape::Area as u32 | 1 << EmitterShape::Column as u32;

    assert_eq!((spec.initial, spec.max, spec.shape), (0, 10, column));
    assert_eq!((spec.radius, spec.radius_variance), (0.5, 1.0));
    assert_eq!((spec.direction_variance, spec.rate), (20.0, 3.0));
    assert_eq!((spec.life, spec.life_variance), (4.0, 0.2));
    assert_eq!((spec.speed, spec.speed_variance), (3.0, 0.2));
    assert_eq!(spec.bounds, ([-10.0, -10.0, 0.0], [10.0, 10.0, 10.0]));
    assert_eq!(
        smoke.sprite,
        Sprite {
            set: 0x40,
            slots: vec![(38, 100)],
        }
    );
}

/// Every shipped system: none recycles or spins, so a birth always draws
/// its sprite's slot and nothing turns; three birth shapes; and
/// `placeholder_spell`'s `RadiusVariance` of 3 is clamped to 1.
#[test]
fn no_shipped_system_recycles_or_spins() {
    let bodies = common::bodies();
    let systems: Vec<_> = bodies
        .iter()
        .filter_map(|(code, b)| Some((*code, b.particles.clone()?)))
        .collect();

    assert_eq!(systems.len(), 22);
    assert!(systems.iter().all(|(_, s)| !s.spec.recycle));
    assert!(systems.iter().all(|(_, s)| s.spec.spin_time == (0.0, 0.0)));
    assert!(systems.iter().all(|(_, s)| s.spec.spin == 0.0));

    // Births by shape: 16 an area column, 5 a point column, `XL28` an area disk.
    let bit = |s: EmitterShape| 1 << s as u32;
    let shaped = |a, b| {
        systems
            .iter()
            .filter(|(_, s)| s.spec.shape == bit(a) | bit(b))
            .count()
    };

    assert_eq!(shaped(EmitterShape::Area, EmitterShape::Column), 16);
    assert_eq!(shaped(EmitterShape::Point, EmitterShape::Column), 5);
    assert_eq!(shaped(EmitterShape::Area, EmitterShape::Disk), 1);

    let placeholder = systems.iter().find(|(c, _)| *c == FourCc(*b"XL00"));

    assert_eq!(placeholder.map(|(_, s)| s.spec.radius_variance), Some(1.0));
}
