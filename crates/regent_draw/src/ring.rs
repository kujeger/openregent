//! A selection ring drawn as a shape, not as its art: openregent's
//! `[enhancements] rings = "native"` or `"modern"`. The band is the library's
//! (`regent_core::view::map::ring::band`), fitted to the ring's tile; this draws it
//! at whatever pixels the view has, its edges smoothed.
//!
//! ```text
//!  quad     the band's box, one unit an art pixel (the quad's scale)
//!  d        distance from the middle line, art pixels (Band::distance)
//!  weight   peak × profile(|d| / half), its outer edge smoothed over a
//!           screen pixel
//!  Glow     dst × (1 + weight)            as the art's glow (soft::GLOW)
//!  Lift     dst + LIFT weight (1 − dst)   then, on the glow: a screen, so
//!                                         it shows on dark ground too
//!  Colour   colour over dst at weight     as the art's mode 8 pixels
//!  Line     colour over dst: a thin line  Rings::Modern, not the art's
//!           on the middle line, a glow
//!           inside it
//! ```
//!
//! All in `Transparent3d`, depth-tested, writing no depth: behind its
//! piece, the body covers it, as the art ring. Under
//! [`crate::shadow::Shadows::Soft`] a glow keeps the shadow's coverage;
//! a colour lifts it as far as it covers ([`Ellipse::specialize`]).

use bevy::asset::{AssetPath, embedded_asset, embedded_path};
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::{Material, MaterialPipeline, MaterialPipelineKey, MaterialPlugin};
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, BlendComponent, BlendFactor, BlendOperation, BlendState, ShaderType,
};
use bevy::render::render_resource::{RenderPipelineDescriptor, SpecializedMeshPipelineError};
use bevy::shader::ShaderRef;
use regent_core::view::map::ring::band::{Band, Paint};

use crate::soft;

/// How selection rings draw: openregent's `[enhancements] rings`.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Deserialize, serde::Serialize,
)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[serde(rename_all = "lowercase")]
pub enum Rings {
    /// The game's pictures, as the original.
    Art,
    /// Drawn as shapes fitted to them ([`Ellipse`]). The default.
    #[default]
    Native,
    /// A thin line on the fitted band's middle line, a soft glow inside
    /// it ([`modern`]): a look not the art's.
    Modern,
}

/// How far past the band's outer edge the quad reaches, art pixels: room
/// for the smoothed edge at any zoom out.
pub const MARGIN: f32 = 4.0;

/// How far a white ring's [`Ink::Lift`] screens what is under it, at
/// full weight: a departure from the art, whose glow alone hardly shows
/// on dark ground. The user's pick, against the art, of 0 to 0.45.
pub const LIFT: f32 = 0.04;

/// [`Rings::Modern`]'s line width, screen pixels.
pub const LINE_WIDTH: f32 = 2.0;

/// How far [`Rings::Modern`]'s glow reaches inside the line, band half
/// widths; it falls off by a smoothstep.
pub const GLOW_REACH: f32 = 2.0;

/// [`Rings::Modern`]'s glow alpha at the line.
pub const GLOW_STRENGTH: f32 = 0.45;

/// How much brighter than the art's colour a [`Rings::Modern`] green or
/// red draws, linear light; the white glow draws white.
pub const BRIGHTEN: f32 = 1.25;

/// [`Rings::Modern`]'s room past [`MARGIN`], art pixels: its line is
/// screen pixels wide, so zoomed out it reaches past a thin band.
pub const LINE_MARGIN: f32 = 4.0;

/// The side ring's line width, screen pixels ([`side`]): thinner than
/// [`LINE_WIDTH`], as the user picked it from the prototypes.
pub const SIDE_WIDTH: f32 = 1.5;

/// The side ring's glow alpha at the line: fainter than
/// [`GLOW_STRENGTH`], so the colour is the line's.
pub const SIDE_GLOW: f32 = 0.25;

/// How far [`Ink::Lift`]'s quad sits in front of its glow's, so it
/// draws after it.
pub const LIFT_STEP: f32 = 1e-4;

/// How a ring meets what is under it: what its pipeline is specialised on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Ink {
    /// Multiplies it by one plus the weight.
    #[default]
    Glow,
    /// Screens it by the weight: `dst + w (1 − dst)`.
    Lift,
    /// Covers it with the colour at the weight.
    Colour,
    /// Covers it with the colour: [`Rings::Modern`]'s line and glow.
    Line,
}

/// The band as the shader reads it: art pixels, colour in linear light.
#[derive(ShaderType, Clone, Copy, Debug, Default, PartialEq)]
pub struct Shape {
    /// The quad's size: what its texture coordinates span.
    pub size: Vec2,
    pub radii: Vec2,
    pub half: f32,
    pub peak: f32,
    pub core: f32,
    pub knee: f32,
    pub edge: f32,
    /// Screen pixels the outer edge is smoothed over.
    pub soft: f32,
    pub colour: Vec4,
    /// [`Ink::Line`]'s line width, screen pixels.
    pub line: f32,
    /// [`Ink::Line`]'s glow reach, half widths.
    pub reach: f32,
}

impl Shape {
    /// `band`'s shape, centred in its quad.
    #[must_use]
    pub fn of(band: &Band) -> Shape {
        let (a, b) = band.radii;
        let colour = match band.paint {
            Paint::Glow => Vec4::ZERO,
            Paint::Colour(c) => Color::srgb_u8(c.r, c.g, c.b).to_linear().to_vec4(),
        };

        Shape {
            size: quad(band),
            radii: Vec2::new(a, b),
            half: band.half,
            peak: band.peak * glow_top(band.paint),
            core: band.profile.core,
            knee: band.profile.knee,
            edge: band.profile.edge,
            soft: 1.0,
            colour,
            line: 0.0,
            reach: 0.0,
        }
    }
}

/// A glow's peak is the strongest raise: the art's gain at its 254, less
/// one ([`soft::GLOW`]). A colour's is its alpha.
#[allow(clippy::cast_precision_loss, reason = "thousandths")]
fn glow_top(paint: Paint) -> f32 {
    match paint {
        Paint::Glow => soft::GLOW[soft::GLOW.len() - 1] as f32 / soft::ONE as f32 - 1.0,
        Paint::Colour(_) => 1.0,
    }
}

/// The quad `band` draws on, art pixels: its outer edge and [`MARGIN`].
#[must_use]
pub fn quad(band: &Band) -> Vec2 {
    let (a, b) = band.radii;

    (Vec2::new(a, b) + band.half + MARGIN) * 2.0
}

/// The quads `band` draws as, in order: a glow and, `lift` above 0, its
/// lift; or a colour.
#[must_use]
pub fn layers(band: &Band, lift: f32) -> Vec<Ellipse> {
    let shape = Shape::of(band);

    match band.paint {
        Paint::Glow if lift > 0.0 => vec![
            Ellipse {
                shape,
                ink: Ink::Glow,
            },
            Ellipse {
                shape: Shape {
                    peak: band.peak * lift,
                    ..shape
                },
                ink: Ink::Lift,
            },
        ],
        Paint::Glow => vec![Ellipse {
            shape,
            ink: Ink::Glow,
        }],
        Paint::Colour(_) => vec![Ellipse {
            shape,
            ink: Ink::Colour,
        }],
    }
}

/// [`Rings::Modern`]'s quad for `band`: its line and glow, white for a
/// glow, the art's colour brightened for green and red.
#[must_use]
pub fn modern(band: &Band) -> Ellipse {
    let shape = Shape::of(band);
    let colour = match band.paint {
        Paint::Glow => Vec4::ONE,
        Paint::Colour(_) => shape.colour * BRIGHTEN,
    };

    Ellipse {
        shape: Shape {
            size: shape.size + LINE_MARGIN * 2.0,
            peak: GLOW_STRENGTH,
            colour,
            line: LINE_WIDTH,
            reach: GLOW_REACH,
            ..shape
        },
        ink: Ink::Line,
    }
}

/// The side ring for `band`: [`modern`]'s line and glow, thinner and
/// fainter, in `colour` (linear light), a piece's side's: openregent's
/// hover ring under `rings = "art"` and `side_ring`, whose tile takes no
/// tint.
#[must_use]
pub fn side(band: &Band, colour: Vec4) -> Ellipse {
    let line = modern(band);

    Ellipse {
        shape: Shape {
            colour,
            line: SIDE_WIDTH,
            peak: SIDE_GLOW,
            ..line.shape
        },
        ..line
    }
}

/// The quads `band` draws as under `rings`, in order; none for
/// [`Rings::Art`], which draws the tile.
#[must_use]
pub fn drawn(band: &Band, rings: Rings) -> Vec<Ellipse> {
    match rings {
        Rings::Art => Vec::new(),
        Rings::Native => layers(band, LIFT),
        Rings::Modern => vec![modern(band)],
    }
}

/// A ring drawn as its band.
#[derive(Asset, AsBindGroup, TypePath, Debug, Clone)]
#[bind_group_data(Ink)]
pub struct Ellipse {
    #[uniform(0)]
    pub shape: Shape,
    pub ink: Ink,
}

impl From<&Ellipse> for Ink {
    fn from(ellipse: &Ellipse) -> Ink {
        ellipse.ink
    }
}

impl Material for Ellipse {
    fn vertex_shader() -> ShaderRef {
        Ellipse::fragment_shader()
    }

    fn fragment_shader() -> ShaderRef {
        ShaderRef::Path(
            AssetPath::from_path_buf(embedded_path!("shaders/ring.wesl")).with_source("embedded"),
        )
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Blend
    }

    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }

    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        Ellipse::shape(descriptor, key.bind_group_data);

        Ok(())
    }
}

impl Ellipse {
    /// A glow multiplies colour and keeps alpha, as a band's glow
    /// ([`crate::material`]); a colour blends over, alpha the most of the
    /// target's and its own, so it lifts the shadow it covers.
    ///
    /// [`Material::specialize`]'s work, without Bevy's pipeline: the
    /// compile tests call it.
    pub fn shape(descriptor: &mut RenderPipelineDescriptor, ink: Ink) {
        crate::plain::unculled(descriptor);

        let keep = BlendComponent {
            src_factor: BlendFactor::Zero,
            dst_factor: BlendFactor::One,
            operation: BlendOperation::Add,
        };

        // dst × (1 + src.rgb): a glow's gain.
        let gain = BlendComponent {
            src_factor: BlendFactor::Dst,
            dst_factor: BlendFactor::One,
            operation: BlendOperation::Add,
        };

        // Max takes no factors; WebGPU wants them One.
        let most = BlendComponent {
            src_factor: BlendFactor::One,
            dst_factor: BlendFactor::One,
            operation: BlendOperation::Max,
        };

        let blend = match ink {
            Ink::Glow => BlendState {
                color: gain,
                alpha: keep,
            },
            // dst + src (1 − dst): a screen.
            Ink::Lift => BlendState {
                color: BlendComponent {
                    src_factor: BlendFactor::OneMinusDst,
                    dst_factor: BlendFactor::One,
                    operation: BlendOperation::Add,
                },
                alpha: keep,
            },
            Ink::Colour | Ink::Line => BlendState {
                color: BlendState::ALPHA_BLENDING.color,
                alpha: most,
            },
        };

        if let Some(depth) = descriptor.depth_stencil.as_mut() {
            depth.depth_write_enabled = Some(false);
        }

        if let Ok(fragment) = descriptor.fragment_mut() {
            match ink {
                Ink::Colour => fragment.shader_defs.push("COLOUR".into()),
                Ink::Line => fragment.shader_defs.push("LINE".into()),
                Ink::Glow | Ink::Lift => {}
            }

            for target in fragment.targets.iter_mut().flatten() {
                target.blend = Some(blend);
            }
        }
    }
}

/// Register the material and its shader.
pub struct Plugin;

impl bevy::app::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shaders/ring.wesl");

        app.add_plugins(MaterialPlugin::<Ellipse>::default());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use regent_core::data::payload::Rgb;
    use regent_core::view::map::ring::Look;

    fn band(paint: Paint) -> Band {
        Band {
            centre: (0.0, 0.0),
            radii: (40.0, 22.0),
            half: 3.0,
            paint,
            peak: 0.6,
            profile: Look::Fits.profile().expect("a ring"),
        }
    }

    /// A glow's full weight is the art's strongest gain less one; a
    /// colour's its alpha. The quad holds the outer edge and the margin.
    #[test]
    #[allow(clippy::float_cmp, reason = "exact sums of small values")]
    fn a_shape_keeps_the_arts_strength_and_size() {
        let glow = Shape::of(&band(Paint::Glow));
        let green = Shape::of(&band(Paint::Colour(Rgb { r: 0, g: 193, b: 0 })));

        assert!((glow.peak - 0.6).abs() < 1e-6);
        assert!((green.peak - 0.6).abs() < 1e-6);
        assert_eq!(glow.size, Vec2::new(94.0, 58.0));
        assert_eq!(
            layers(&band(Paint::Glow), 0.5)
                .iter()
                .map(|e| (e.ink, e.shape.peak))
                .collect::<Vec<_>>(),
            [(Ink::Glow, 0.6), (Ink::Lift, 0.3)]
        );
        assert!(green.colour.y > 0.0 && green.colour.x == 0.0);
    }

    /// The modern ring: one quad, its line and glow the constants, white
    /// for a glow, green brightened, with room for the line.
    #[test]
    #[allow(clippy::float_cmp, reason = "exact sums of small values")]
    fn a_modern_ring_is_a_line_and_a_glow() {
        let colour = Paint::Colour(Rgb { r: 0, g: 193, b: 0 });
        let white = drawn(&band(Paint::Glow), Rings::Modern);
        let green = modern(&band(colour)).shape;

        assert_eq!(white.len(), 1);
        assert_eq!(white[0].ink, Ink::Line);
        assert_eq!(white[0].shape.colour, Vec4::ONE);
        assert_eq!(white[0].shape.size, Vec2::new(102.0, 66.0));
        assert_eq!(
            (
                white[0].shape.peak,
                white[0].shape.line,
                white[0].shape.reach
            ),
            (0.45, 2.0, 2.0)
        );
        assert_eq!(green.colour, Shape::of(&band(colour)).colour * 1.25);
        assert!(drawn(&band(Paint::Glow), Rings::Art).is_empty());
        assert_eq!(drawn(&band(Paint::Glow), Rings::Native).len(), 2);
    }

    /// The side ring: the modern ring's quad and reach, its own colour,
    /// line and glow.
    #[test]
    #[allow(clippy::float_cmp, reason = "exact constants")]
    fn a_side_ring_is_a_thin_modern_ring_in_its_colour() {
        let red = Vec4::new(1.0, 0.0, 0.0, 1.0);
        let line = modern(&band(Paint::Glow));
        let side = side(&band(Paint::Glow), red);

        assert_eq!(side.ink, Ink::Line);
        assert_eq!(side.shape.colour, red);
        assert_eq!((side.shape.line, side.shape.peak), (1.5, 0.25));
        assert_eq!(
            (side.shape.size, side.shape.reach),
            (line.shape.size, line.shape.reach)
        );
    }
}
