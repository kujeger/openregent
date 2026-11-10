//! The `ParticleSystem` block — 272 bytes at `PRIM + 0x14`.
//!
//! Mostly floats. `M_ParticleSystems.xml` and its expansion counterpart
//! describe all 22 shipped records. The binary keeps XML order:
//!
//! ```text
//! 0x000  u32     AttachmentPointID
//! 0x004  u32     InitialParticles
//! 0x008  u32     MaxParticles
//! 0x00c  3 * f32 Bounds min
//! 0x018  3 * f32 Bounds max
//! 0x024  80      <Emitter>
//! 0x074  148     <Particle>; its last two floats, at 0x100, the spin's
//!                time in and out, unnamed in the XML
//! 0x108  2 * u32 zero on all 22; not the particle readers'
//! ```
//!
//! # What is confirmed
//!
//! Nineteen varying fields are solved by value intersection; `Emitter`'s two
//! bitfields by set equality.
//!
//! The rest are constant on all 22 records (every `Velocity` is `0, 0, 0`).
//!
//! **Binary**: particle prim read, version 5, reads
//! the block field by field in this order: particle bounds read the bounds,
//! Particle emitter read the emitter, particle read the particle, whose last two
//! floats are `0x100` and `0x104`. What each field does is in
//! `sim::world::particles` and `view::map::particles`.

use crate::data::error::{Error, Result};

use super::{field, float};

/// The block's size; a `ParticleSystem`'s `PRIM` is this much longer than
/// other subtypes'.
pub(crate) const BLOCK_LEN: usize = 272;

const EMITTER_AT: usize = 0x24;
const EMITTER_LEN: usize = 80;
const PARTICLE_AT: usize = 0x74;
const PARTICLE_LEN: usize = 148;

/// Three floats: a position, a direction or a spread.
pub type Vec3 = (f32, f32, f32);

/// Four floats, red green blue alpha.
pub type Rgba = (f32, f32, f32, f32);

fn vec3(body: &[u8], at: usize) -> Option<Vec3> {
    Some((float(body, at)?, float(body, at + 4)?, float(body, at + 8)?))
}

fn rgba(body: &[u8], at: usize) -> Option<Rgba> {
    Some((
        float(body, at)?,
        float(body, at + 4)?,
        float(body, at + 8)?,
        float(body, at + 12)?,
    ))
}

/// A `DUNT` record's `<Engine>` block, when the subtype is `ParticleSystem`.
#[derive(Debug, Clone, Copy)]
pub struct ParticleSystem<'a>(pub(super) &'a [u8]);

impl<'a> ParticleSystem<'a> {
    /// # Errors
    ///
    /// If the block is not exactly `BLOCK_LEN` (272) bytes.
    pub fn parse(block: &'a [u8]) -> Result<Self> {
        (block.len() == BLOCK_LEN)
            .then_some(ParticleSystem(block))
            .ok_or(Error::BrokenInvariant {
                what: "particle system block length",
                expected: BLOCK_LEN as u64,
                found: block.len() as u64,
            })
    }

    /// Where the system hangs off its owner. Overlays carry it at another
    /// offset.
    pub fn attach_point(&self) -> Option<u32> {
        field(self.0, 0x00)
    } // 22
    pub fn initial_particles(&self) -> Option<u32> {
        field(self.0, 0x04)
    } // 22
    pub fn max_particles(&self) -> Option<u32> {
        field(self.0, 0x08)
    } // 22

    /// The box the system is culled against.
    pub fn bounds(&self) -> Option<(Vec3, Vec3)> {
        Some((vec3(self.0, 0x0c)?, vec3(self.0, 0x18)?))
    } // 22

    pub fn emitter(&self) -> Emitter<'a> {
        Emitter(&self.0[EMITTER_AT..EMITTER_AT + EMITTER_LEN])
    }

    pub fn particle(&self) -> Particle<'a> {
        Particle(&self.0[PARTICLE_AT..PARTICLE_AT + PARTICLE_LEN])
    }
}

/// Emitter shape. A bitfield: every shipped system sets two bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmitterShape {
    Point = 0,  // 5
    Area = 1,   // 17
    Disk = 2,   // 1
    Column = 3, // 21
}

impl EmitterShape {
    pub const ALL: [EmitterShape; 4] = [
        EmitterShape::Point,
        EmitterShape::Area,
        EmitterShape::Disk,
        EmitterShape::Column,
    ];

    /// The SDK's own spelling, which the XML writes under `Type`.
    pub const fn name(self) -> &'static str {
        match self {
            EmitterShape::Point => "Point",
            EmitterShape::Area => "Area",
            EmitterShape::Disk => "Disk",
            EmitterShape::Column => "Column",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmitterFlag {
    IsPausable = 0,       // 6
    IsPerformanceHog = 1, // 2
}

impl EmitterFlag {
    pub const ALL: [EmitterFlag; 2] = [EmitterFlag::IsPausable, EmitterFlag::IsPerformanceHog];

    pub const fn name(self) -> &'static str {
        match self {
            EmitterFlag::IsPausable => "IsPausable",
            EmitterFlag::IsPerformanceHog => "IsPerformanceHog",
        }
    }
}

/// The `<Emitter>` section: where particles come from and how fast.
#[derive(Debug, Clone, Copy)]
pub struct Emitter<'a>(&'a [u8]);

impl Emitter<'_> {
    pub fn shape_bits(&self) -> Option<u32> {
        field(self.0, 0x00)
    } // 22

    pub fn is(&self, shape: EmitterShape) -> bool {
        self.shape_bits()
            .is_some_and(|w| w >> shape as u32 & 1 == 1)
    }

    pub fn shapes(&self) -> impl Iterator<Item = EmitterShape> + '_ {
        EmitterShape::ALL.into_iter().filter(|s| self.is(*s))
    }

    pub fn velocity(&self) -> Option<Vec3> {
        vec3(self.0, 0x04)
    }
    pub fn acceleration(&self) -> Option<Vec3> {
        vec3(self.0, 0x10)
    }
    pub fn offset(&self) -> Option<Vec3> {
        vec3(self.0, 0x1c)
    }
    pub fn surface_normal(&self) -> Option<Vec3> {
        vec3(self.0, 0x28)
    }
    pub fn radius(&self) -> Option<f32> {
        float(self.0, 0x34)
    } // 22
    pub fn radius_variance(&self) -> Option<f32> {
        float(self.0, 0x38)
    } // 22
    pub fn direction_variance(&self) -> Option<f32> {
        float(self.0, 0x3c)
    } // 22
    pub fn rate(&self) -> Option<f32> {
        float(self.0, 0x40)
    } // 22
    pub fn lifespan(&self) -> Option<u32> {
        field(self.0, 0x44)
    } // 22
    pub fn recycle(&self) -> Option<u32> {
        field(self.0, 0x48)
    }

    pub fn flag_bits(&self) -> Option<u32> {
        field(self.0, 0x4c)
    } // 22

    pub fn has(&self, flag: EmitterFlag) -> bool {
        self.flag_bits().is_some_and(|w| w >> flag as u32 & 1 == 1)
    }

    pub fn flags(&self) -> impl Iterator<Item = EmitterFlag> + '_ {
        EmitterFlag::ALL.into_iter().filter(|f| self.has(*f))
    }
}

/// The `<Particle>` section: what one particle does over its life.
#[derive(Debug, Clone, Copy)]
pub struct Particle<'a>(&'a [u8]);

impl Particle<'_> {
    pub fn lifespan(&self) -> Option<u32> {
        field(self.0, 0x00)
    } // 22
    pub fn lifespan_variance(&self) -> Option<f32> {
        float(self.0, 0x04)
    } // 22
    pub fn initial_speed(&self) -> Option<f32> {
        float(self.0, 0x08)
    } // 22
    pub fn initial_speed_variance(&self) -> Option<f32> {
        float(self.0, 0x0c)
    } // 22
    pub fn gravity(&self) -> Option<Vec3> {
        vec3(self.0, 0x10)
    } // 22

    /// Colour at birth, midlife and death. The engine interpolates between.
    pub fn birth_colour(&self) -> Option<Rgba> {
        rgba(self.0, 0x1c)
    }
    pub fn midlife_colour(&self) -> Option<Rgba> {
        rgba(self.0, 0x2c)
    }
    pub fn death_colour(&self) -> Option<Rgba> {
        rgba(self.0, 0x3c)
    }

    pub fn birth_size(&self) -> Option<f32> {
        float(self.0, 0x4c)
    } // 22
    pub fn max_size(&self) -> Option<f32> {
        float(self.0, 0x50)
    } // 22
    pub fn death_size(&self) -> Option<f32> {
        float(self.0, 0x54)
    } // 22

    /// How long the size and alpha ramps take, in and out.
    pub fn size_time(&self) -> Option<(f32, f32)> {
        Some((float(self.0, 0x58)?, float(self.0, 0x5c)?))
    } // 22
    pub fn fade_time(&self) -> Option<(f32, f32)> {
        Some((float(self.0, 0x60)?, float(self.0, 0x64)?))
    } // 22

    pub fn jitter_orientation(&self) -> Option<Vec3> {
        vec3(self.0, 0x68)
    }
    pub fn jitter_displacement(&self) -> Option<Vec3> {
        vec3(self.0, 0x74)
    }

    pub fn birth_spin_rate(&self) -> Option<f32> {
        float(self.0, 0x80)
    }
    pub fn midlife_spin_rate(&self) -> Option<f32> {
        float(self.0, 0x84)
    }
    pub fn death_spin_rate(&self) -> Option<f32> {
        float(self.0, 0x88)
    }

    /// The spin's ramp, in and out, shares of life: block `0x100`,
    /// `0x104`, zero on all 22. **Binary**: particle read reads them to
    /// particle,; `CY3D_ParticleSystem` bounds the
    /// spin with them as `SizeTime` bounds the size.
    pub fn spin_time(&self) -> Option<(f32, f32)> {
        Some((float(self.0, 0x8c)?, float(self.0, 0x90)?))
    }
}
