//! Trees bending in a west wind (`[enhancements] wind`, openregent's):
//! drawing alone, the sim untouched. Experimental.
//!
//! Each tree's quads (body, shadow band, lift) are sheared about the
//! tree's foot in the vertex stage (`sway.wesl`), all by one angle:
//!
//! ```text
//!   top   ───── +d       d = A · f · h/H · (½ + ½ sin 2π(t/T − x/λ)) · g
//!        ╱  ╱            g = ¾ + ¼ sin 2π(t/G − x/Λ), the gust
//!   foot ─────   0       d ≥ 0: east and back, never west
//! ```
//!
//! `t` is game time ([`Reel`]: stops on pause, runs with the speed),
//! `x` the tree's world x, so a gust travels west to east. A is
//! [`Wind::reach`], art pixels at the top of a tree H tall
//! ([`Trees::reference`]); h the tree's height, its sprite's top to its
//! foot, so a tall pine leans further than a small tree; f its
//! [`Tree::factor`], less for bare trees.
//!
//! Which trees sway, f, H and λ are a data file: [`DEFAULTS`] shipped,
//! a user's copy laid over it ([`Trees::read`]).
//!
//! The same wind drives openregent's extras: smoke drifts east by the
//! strength integrated over a puff's age ([`drift`]), and a gust's peak
//! sounds as it passes the view ([`gusts`]).

use std::collections::BTreeMap;

use bevy::camera::primitives::Aabb;
use bevy::math::Vec3A;
use bevy::prelude::*;
use bevy::render::render_resource::ShaderType;
use regent_core::data::fourcc::FourCc;

use crate::map::cel::{Cel, Drawing, Picture};
use crate::map::reel::Reel;
use crate::material::{Band, Body};

/// The shipped tree table, `data/wind.toml`, held in the binary.
pub const DEFAULTS: &str = include_str!("../data/wind.toml");

/// Art pixels the reference tree's top leans at most: [`Wind::reach`]'s
/// top, openregent's `wind_amplitude`'s.
pub const MOST: f32 = 4.0;

/// The least share of the quad above the foot the shear divides by: a
/// foot at the quad's top bends nothing wild.
const LEAST_RISE: f32 = 0.01;

/// A gust's wavelength in sway wavelengths: Λ = 4λ.
const GUST_WAVES: f32 = 4.0;

/// Game ms a gust takes: the clock wraps here ([`seconds`]).
pub const GUST_MS: u32 = 28_000;

/// Seconds a gust takes, G.
#[allow(clippy::cast_precision_loss, reason = "28,000 is exact")]
pub const GUST: f32 = GUST_MS as f32 / 1000.0;

/// Seconds a sway takes, T: a seventh of a gust, so the wrap jumps
/// neither. **Inferred**: taste.
pub const PERIOD: f32 = GUST / 7.0;

/// Which descriptors sway and how: [`DEFAULTS`] with a user's file over
/// it. Not the sim's: per machine, in no digest.
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct Trees {
    /// Art pixels: a tree this tall leans [`Wind::reach`] at its top.
    pub reference: f32,
    /// World units (art pixels) between two trees in step, λ.
    pub wave: f32,
    /// By descriptor code; a code not here does not sway.
    pub trees: BTreeMap<FourCc, Tree>,
}

/// One descriptor's sway.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tree {
    /// The lean's scale: 1 leafy, less bare, 0 still.
    pub factor: f32,
    /// Art pixels in place of the sprite's height above its foot.
    pub height: Option<f32>,
}

impl Default for Trees {
    fn default() -> Self {
        let (trees, problems) = Trees::BARE.laid(DEFAULTS);

        for problem in problems {
            tracing::warn!("shipped wind.toml: {problem}");
        }

        trees
    }
}

impl Trees {
    /// What [`DEFAULTS`] is laid over: no trees.
    const BARE: Trees = Trees {
        reference: 1.0,
        wave: 1.0,
        trees: BTreeMap::new(),
    };

    /// The defaults with a user's file `user` laid over them: a key it
    /// holds replaces that key, a tree it lists that tree's entry. A file
    /// not TOML, or a bad value, warns and keeps the default.
    #[must_use]
    pub fn read(user: Option<&str>) -> Trees {
        let shipped = Trees::default();
        let (trees, problems) = match user {
            Some(text) => shipped.laid(text),
            None => (shipped, Vec::new()),
        };

        for problem in problems {
            tracing::warn!("wind.toml: {problem}");
        }

        trees
    }

    /// `self` with file `text` laid over, and what was wrong in it.
    fn laid(mut self, text: &str) -> (Trees, Vec<String>) {
        let table = match text.parse::<toml::Table>() {
            Ok(table) => table,
            Err(e) => return (self, vec![format!("not TOML, defaults kept: {e}")]),
        };
        let mut problems = Vec::new();

        // The global keys, each on its own.
        for (key, value) in &table {
            let field = match key.as_str() {
                "reference" => Some(&mut self.reference),
                "wave" => Some(&mut self.wave),
                _ => None,
            };

            match (field, positive(value)) {
                (Some(field), Some(number)) => *field = number,
                (Some(_), None) => problems.push(format!("{key}: not a number above 0")),
                (None, _) if key == TREES => {}
                (None, _) => problems.push(format!("{key}: unknown key, ignored")),
            }
        }

        // The trees, each entry whole.
        let listed = table.get(TREES).and_then(toml::Value::as_table);

        for (code, entry) in listed.into_iter().flatten() {
            match (FourCc::try_from(code.as_bytes()), tree(entry)) {
                (Ok(fourcc), Some(tree)) => {
                    self.trees.insert(fourcc, tree);
                }
                (Err(_), _) => problems.push(format!("trees.{code}: not a four-character code")),
                (_, None) => problems.push(format!("trees.{code}: needs factor ≥ 0, height > 0")),
            }
        }

        (self, problems)
    }

    /// Descriptor `code`'s sway, if it sways.
    #[must_use]
    pub fn get(&self, code: FourCc) -> Option<Tree> {
        self.trees.get(&code).copied().filter(|t| t.factor > 0.0)
    }

    /// The lean's share per unit of quad height a cel of `tree` takes,
    /// its foot at `foot`, `size_y` art pixels tall: its top moves
    /// `factor · height / reference` of the lean.
    #[must_use]
    pub fn bend(&self, tree: Tree, foot: f32, size_y: f32) -> f32 {
        let rise = (0.5 - foot).max(LEAST_RISE);
        let height = tree.height.unwrap_or(rise * size_y);

        tree.factor * height / (self.reference * rise)
    }
}

/// The table of trees in the file.
const TREES: &str = "trees";

/// `value` as a finite number above 0.
fn positive(value: &toml::Value) -> Option<f32> {
    number(value).filter(|n| *n > 0.0)
}

/// `value` as a finite number, integer or float.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "art pixels and factors"
)]
fn number(value: &toml::Value) -> Option<f32> {
    let n = match value {
        toml::Value::Float(f) => Some(*f as f32),
        toml::Value::Integer(i) => Some(*i as f32),
        _ => None,
    };

    n.filter(|n| n.is_finite())
}

/// A tree entry: `factor` needed, `height` not.
fn tree(entry: &toml::Value) -> Option<Tree> {
    let entry = entry.as_table()?;
    let factor = entry.get("factor").and_then(number).filter(|f| *f >= 0.0)?;
    let height = match entry.get("height") {
        Some(h) => Some(positive(h)?),
        None => None,
    };

    Some(Tree { factor, height })
}

/// A swaying cel's bounds: the unit quad, wider for its lean at
/// [`MOST`], top or bottom, as [`Gust::shove`] gives it.
#[must_use]
pub fn bounds(cel: &Cel, bend: f32) -> Aabb {
    let foot = foot(cel);
    let reach = (0.5 - foot).abs().max((0.5 + foot).abs()) * bend * MOST;

    Aabb {
        center: Vec3A::ZERO,
        half_extents: Vec3A::new(0.5 + reach / cel.size.x.max(1.0), 0.5, 0.0),
    }
}

/// The wind wanted: openregent's settings, a screenshot's flags.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct Wind {
    /// Art pixels the top of a tree [`Trees::reference`] tall leans at
    /// most; 0 still.
    pub reach: f32,
    /// Game ms the wind is held at, in place of [`Reel`]'s: a
    /// screenshot's.
    pub held: Option<u32>,
    /// Whether the shadow sways with its tree.
    pub shadow: Shadow,
}

/// What a tree's shadow does in the wind.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
pub enum Shadow {
    /// Sheared as its body: the specks in the canopy stay on the leaves.
    #[default]
    Sheared,
    /// Left in place: a test.
    Still,
}

/// Whether a material sways.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Sway {
    #[default]
    Still,
    Tree,
}

/// A material's wind, as `sway.wesl` reads it.
#[derive(ShaderType, Clone, Copy, Debug, Default, PartialEq)]
pub struct Gust {
    /// Seconds, wrapped at [`GUST`].
    pub at: f32,
    /// Art pixels the reference tree's top leans at most: [`Wind::reach`].
    pub reach: f32,
    /// The foot's height on the unit quad, −0.5 its bottom.
    pub foot: f32,
    /// The lean's share per unit of quad height above the foot:
    /// [`Trees::bend`].
    pub bend: f32,
    pub period: f32,
    pub wave: f32,
    pub gust: f32,
    pub gust_wave: f32,
}

impl Gust {
    /// No wind, foot at the centre; waves of 1, unread while still.
    pub const STILL: Gust = Gust {
        at: 0.0,
        reach: 0.0,
        foot: 0.0,
        bend: 0.0,
        period: PERIOD,
        wave: 1.0,
        gust: GUST,
        gust_wave: 1.0,
    };

    /// Whether it bends anything.
    #[must_use]
    pub fn blows(&self) -> bool {
        self.reach > 0.0
    }

    /// Art pixels local height `y` moves for a lean of `lean`, as
    /// `sway.wesl` shears: 0 at the foot.
    #[must_use]
    pub fn shove(&self, lean: f32, y: f32) -> f32 {
        lean * (y - self.foot) * self.bend
    }
}

/// Game ms `ms` as seconds wrapped at [`GUST`]: an `f32` keeps its
/// precision.
#[must_use]
pub fn seconds(ms: u32) -> f32 {
    #[allow(clippy::cast_precision_loss, reason = "under 28,000")]
    let wrapped = (ms % GUST_MS) as f32;

    wrapped / 1000.0
}

/// Smoke particle systems ([`drift`]): `Smoke_Large`, `Smoke_Large_Heavy`
/// (a damaged building's), `Ratasmoke`, `FlammingArrowSmoke`.
pub const SMOKES: [FourCc; 4] = [
    FourCc(*b"XLSL"),
    FourCc(*b"XLSH"),
    FourCc(*b"XL23"),
    FourCc(*b"XL25"),
];

/// Art pixels a second smoke drifts in full wind, per art pixel of
/// [`Wind::reach`]. **Inferred**: taste.
pub const DRIFT: f32 = 8.0;

/// Samples a puff's life is integrated over in [`drift`].
const DRIFT_STEPS: u16 = 8;

/// The wind's strength, 0 to 1, at game seconds `t` and world x `x`,
/// `wave` λ: a tree's lean over its reach (`sway.wesl`'s `sway · gust`).
#[must_use]
pub fn strength(t: f32, x: f32, wave: f32) -> f32 {
    let tau = std::f32::consts::TAU;
    let sway = 0.5 + 0.5 * (tau * (t / PERIOD - x / wave)).sin();
    let gust = 0.75 + 0.25 * (tau * (t / GUST - x / (GUST_WAVES * wave))).sin();

    sway * gust
}

/// Art pixels east a puff `age` seconds old at world x `x` has drifted by
/// game seconds `t`: [`strength`] over its life, [`DRIFT`] · `reach` a
/// second at full strength. Never west; 0 with no wind.
#[must_use]
pub fn drift(t: f32, age: f32, x: f32, wave: f32, reach: f32) -> f32 {
    let step = age.max(0.0) / f32::from(DRIFT_STEPS);

    // Midpoints, back from now.
    let felt: f32 = (0..DRIFT_STEPS)
        .map(|i| strength(t - (f32::from(i) + 0.5) * step, x, wave))
        .sum();

    felt * step * DRIFT * reach
}

/// Gust peaks that have passed world x `x` by game ms `ms`, `wave` λ:
/// rises by 1 as each peak passes, so its whole part counts them. A peak
/// is where [`strength`]'s gust term is 1; it runs east at
/// [`gust_speed`].
///
/// ```
/// use regent_draw::wind::{GUST_MS, gusts};
///
/// // One more peak each gust period, wherever the view.
/// assert_eq!(gusts(GUST_MS, 100.0, 64.0) - gusts(0, 100.0, 64.0), 1.0);
/// ```
#[must_use]
pub fn gusts(ms: u32, x: f32, wave: f32) -> f64 {
    let quarter = 0.25;

    f64::from(ms) / f64::from(GUST_MS) - f64::from(x / (GUST_WAVES * wave)) - quarter
}

/// World units a second a gust peak runs east, `wave` λ.
#[must_use]
pub fn gust_speed(wave: f32) -> f32 {
    GUST_WAVES * wave / GUST
}

/// Mark `drawings`' materials as a tree's: each sways in the wind; each
/// frame's foot and bend are its instances' (`instance::Lean`).
pub fn mark(
    drawings: &[Drawing],
    trees: &Trees,
    bodies: &mut Assets<Body>,
    bands: &mut Assets<Band>,
) {
    let cels = drawings.iter().flat_map(|d| d.cels.iter().flatten());
    let shaped = |gust: &mut Gust| {
        gust.wave = trees.wave;
        gust.gust_wave = GUST_WAVES * trees.wave;
    };

    for cel in cels {
        if let Picture::Indexed(body) = &cel.body
            && let Some(mut body) = bodies.get_mut(body)
        {
            body.sway = Sway::Tree;
            shaped(&mut body.gust);
        }

        let lift = cel.lift.as_ref().and_then(|lift| bodies.get_mut(lift));

        if let Some(mut lift) = lift {
            lift.sway = Sway::Tree;
            shaped(&mut lift.gust);
        }

        if let Some(mut band) = cel.band.as_ref().and_then(|band| bands.get_mut(band)) {
            band.sway = Sway::Tree;
            shaped(&mut band.gust);
        }
    }
}

/// `cel`'s anchor's height on its unit quad: the quad's centre stands
/// `anchor` below it (`map::things`).
#[must_use]
pub fn foot(cel: &Cel) -> f32 {
    cel.anchor.y / cel.size.y.max(1.0)
}

/// Every tree material's wind to the wanted, each frame it changes.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value; that is the contract"
)]
pub fn blow(
    wind: Res<Wind>,
    reel: Option<Res<Reel>>,
    mut bodies: ResMut<Assets<Body>>,
    mut bands: ResMut<Assets<Band>>,
) {
    let ms = wind.held.or_else(|| reel.map(|r| r.ms)).unwrap_or(0);

    // Still, the clock is left alone: no material changes while no wind blows.
    let wanted = |reach: f32| {
        if reach > 0.0 {
            (seconds(ms), reach)
        } else {
            (0.0, 0.0)
        }
    };
    let body = wanted(wind.reach);
    let band = wanted(match wind.shadow {
        Shadow::Sheared => wind.reach,
        Shadow::Still => 0.0,
    });
    let stale = |sway: Sway, gust: &Gust, (at, reach): (f32, f32)| {
        sway == Sway::Tree && *gust != Gust { at, reach, ..*gust }
    };

    // Ids first: `get_mut` marks a material changed, to be prepared again.
    let ids: Vec<_> = bodies
        .iter()
        .filter(|(_, b)| stale(b.sway, &b.gust, body))
        .map(|(id, _)| id)
        .collect();

    for id in ids {
        if let Some(mut made) = bodies.get_mut(id) {
            (made.gust.at, made.gust.reach) = body;
        }
    }

    let ids: Vec<_> = bands
        .iter()
        .filter(|(_, b)| stale(b.sway, &b.gust, band))
        .map(|(id, _)| id)
        .collect();

    for id in ids {
        if let Some(mut made) = bands.get_mut(id) {
            (made.gust.at, made.gust.reach) = band;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Corpus;
    use regent_core::content::imagery::FrameSource;
    use regent_core::content::unit::Unit;
    use regent_core::data::desc::UnitKind;
    use regent_core::view::Sprite;

    /// The trees the wind first bent, before the table: 19 codes.
    const FIRST: [&[u8; 4]; 19] = [
        b"FG01", b"GG01", b"GG02", b"LG01", b"LG02", b"UG01", b"UG02", b"IG02", b"IG08", b"PG07",
        b"PG08", b"CG07", b"CG08", b"RG07", b"RG08", b"WG07", b"WG08", b"NGa1", b"NGr1",
    ];

    /// HD's archives, base and expansion.
    fn corpus() -> Corpus {
        let mut paths = crate::common::archives(&crate::common::hd().join("Data"));

        paths.extend(crate::common::archives(&crate::common::hd().join("DataMX")));

        Corpus::open(&paths).expect("HD's archives")
    }

    /// The shipped defaults, and whatever they got wrong.
    fn shipped() -> (Trees, Vec<String>) {
        Trees::BARE.laid(DEFAULTS)
    }

    /// A cel `size` art pixels, its anchor at `foot` rows above its bottom.
    fn cel(size: Vec2, foot_row: f32) -> Cel {
        Cel {
            body: Picture::Colour(Handle::default()),
            band: None,
            aloft: None,
            lift: None,
            size,
            anchor: Vec2::new(0.0, foot_row - size.y / 2.0),
            tile: (0, 0),
            frame: crate::instance::Framed::default(),
        }
    }

    /// How far `cel`'s top moves for a lean of 1, as `tree` in `trees`.
    fn top(trees: &Trees, tree: Tree, cel: &Cel) -> f32 {
        let gust = Gust {
            foot: foot(cel),
            bend: trees.bend(tree, foot(cel), cel.size.y),
            ..Gust::STILL
        };

        gust.shove(1.0, 0.5)
    }

    /// The shipped file parses whole and holds every first tree.
    #[test]
    fn the_shipped_wind_table_parses_and_holds_the_first_trees() {
        let (trees, problems) = shipped();

        assert_eq!(problems, Vec::<String>::new());
        assert_eq!(trees, Trees::default());

        for code in FIRST {
            assert!(trees.get(FourCc(*code)).is_some(), "{code:?}");
        }
    }

    /// Every listed tree is a descriptor of HD's, named a tree.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn every_tree_is_a_descriptor_named_a_tree() {
        let corpus = corpus();
        let named = ["tree", "pine", "cypress", "birch", "dead"];

        for code in Trees::default().trees.keys() {
            let unit = Unit::find(&corpus.catalogue, *code)
                .expect("listed")
                .expect("parses");
            let name = String::from_utf8_lossy(unit.desc.head.name).to_lowercase();

            assert!(named.iter().any(|n| name.contains(n)), "{code:?}: {name}");
        }
    }

    /// The reference height is the first trees' median: rows from a
    /// tile's top to its anchor, over their tiles. A typical tree leans
    /// as far as before heights counted.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    #[allow(clippy::float_cmp, reason = "whole numbers, exact")]
    fn the_reference_height_is_the_first_trees_median() {
        let corpus = corpus();
        let mut tiles: Vec<(usize, usize)> = FIRST
            .iter()
            .flat_map(|code| {
                let unit = Unit::find(&corpus.catalogue, FourCc(**code))
                    .expect("listed")
                    .expect("parses");

                unit.animations
                    .iter()
                    .flat_map(|a| a.poses.iter())
                    .flat_map(|p| p.frames.iter().filter_map(FrameSource::tile))
                    .collect::<Vec<_>>()
            })
            .collect();

        tiles.sort_unstable();
        tiles.dedup();

        let mut heights: Vec<u16> = tiles
            .iter()
            .map(|&(archive, index)| {
                let frame = Sprite::at(&corpus.catalogue, archive, index)
                    .expect("found")
                    .expect("decodes");

                u16::try_from(frame.anchor.1).expect("in the tile")
            })
            .collect();

        heights.sort_unstable();

        let median = f32::from(heights[heights.len() / 2]);

        assert_eq!(Trees::default().reference, median, "{heights:?}");
    }

    /// Two trees of one factor: their tops move as their heights.
    #[test]
    fn a_trees_top_moves_as_its_height() {
        let trees = Trees::default();
        let leafy = Tree {
            factor: 1.0,
            height: None,
        };
        let tall = cel(Vec2::new(160.0, 196.0), 20.0);
        let short = cel(Vec2::new(110.0, 90.0), 12.0);
        let ratio = top(&trees, leafy, &tall) / top(&trees, leafy, &short);

        assert!((ratio - 176.0 / 78.0).abs() < 1e-5, "{ratio}");
        assert!(
            (top(
                &trees,
                leafy,
                &cel(Vec2::splat(100.0), 100.0 - trees.reference)
            ) - 1.0)
                .abs()
                < 1e-5
        );
    }

    /// A tree's factor scales its lean; a height given stands in for
    /// the sprite's.
    #[test]
    fn a_trees_factor_and_height_scale_its_lean() {
        let trees = Trees::default();
        let tall = cel(Vec2::new(160.0, 196.0), 20.0);
        let leafy = Tree {
            factor: 1.0,
            height: None,
        };
        let bare = Tree {
            factor: 0.3,
            ..leafy
        };
        let given = Tree {
            height: Some(trees.reference),
            ..leafy
        };

        assert!((top(&trees, bare, &tall) / top(&trees, leafy, &tall) - 0.3).abs() < 1e-5);
        assert!((top(&trees, given, &tall) - 1.0).abs() < 1e-5);
    }

    /// A user's file replaces the keys and trees it names, adds trees,
    /// and stills one by factor 0; the rest stay shipped.
    #[test]
    #[allow(clippy::float_cmp, reason = "whole numbers, exact")]
    fn a_users_wind_table_overrides_entry_by_entry() {
        let shipped = Trees::default();
        let user = Trees::read(Some(
            "wave = 400\n\
             [trees]\n\
             CG07 = { factor = 0.5, height = 90 }\n\
             PG06 = { factor = 0.2 }\n\
             FG01 = { factor = 0 }\n",
        ));

        assert_eq!(user.wave, 400.0);
        assert_eq!(user.reference, shipped.reference);
        assert_eq!(
            user.get(FourCc(*b"CG07")),
            Some(Tree {
                factor: 0.5,
                height: Some(90.0)
            })
        );
        assert_eq!(user.get(FourCc(*b"PG06")).map(|t| t.factor), Some(0.2));
        assert_eq!(user.get(FourCc(*b"FG01")), None);
        assert_eq!(user.get(FourCc(*b"IG02")), shipped.get(FourCc(*b"IG02")));
    }

    /// A file not TOML keeps the defaults; a bad value keeps that key's
    /// or tree's default, and each says so.
    #[test]
    fn a_broken_wind_table_falls_back_to_the_defaults() {
        let shipped = Trees::default();
        let (broken, problems) = shipped.clone().laid("[trees\nCG07 = ");

        assert_eq!(broken, shipped);
        assert_eq!(problems.len(), 1);

        let (bad, problems) = shipped.clone().laid(
            "reference = -3\nwave = \"far\"\n[trees]\nCG07 = { factor = -1 }\nlonger = { factor = 1 }\n",
        );

        assert_eq!(bad, shipped);
        assert_eq!(problems.len(), 4, "{problems:?}");
    }

    /// The strength stays within 0 to 1 and nears both ends over a
    /// gust.
    #[test]
    fn the_wind_strength_runs_from_nought_to_one() {
        let wave = Trees::default().wave;
        let samples = (0..2800u16).map(|i| strength(f32::from(i) / 100.0, 0.0, wave));
        let (least, most) = samples.fold((1.0f32, 0.0f32), |(l, m), s| (l.min(s), m.max(s)));

        assert!((0.0..0.01).contains(&least), "{least}");
        assert!((0.95..=1.0).contains(&most), "{most}");
    }

    /// Smoke drifts further the older it is, and twice the reach drifts
    /// twice as far; newborn or windless, not at all.
    #[test]
    #[allow(clippy::float_cmp, reason = "exact zeros")]
    fn smoke_drifts_east_with_age_and_reach() {
        let wave = Trees::default().wave;

        assert_eq!(drift(3.0, 0.0, 100.0, wave, 2.0), 0.0);
        assert_eq!(drift(3.0, 2.0, 100.0, wave, 0.0), 0.0);

        let ages = [0.5, 1.0, 2.0, 4.0].map(|age| drift(10.0, age, 300.0, wave, 2.0));

        assert!(ages.windows(2).all(|w| w[0] < w[1]), "{ages:?}");
        assert!((drift(10.0, 2.0, 300.0, wave, 4.0) - 2.0 * ages[2]).abs() < 1e-3);
    }

    /// A gust peak passes a place once a gust, and a place `wave` · 4
    /// east a whole gust later; at the peak the gust term is 1.
    #[test]
    #[allow(clippy::float_cmp, reason = "exact in binary")]
    fn a_gust_peak_passes_once_a_gust_running_east() {
        let wave = 800.0;
        let gust_wave = GUST_WAVES * wave;

        assert_eq!(gusts(GUST_MS, 0.0, wave) - gusts(0, 0.0, wave), 1.0);
        assert_eq!(gusts(GUST_MS, gust_wave, wave), gusts(0, 0.0, wave));
        assert_eq!(gust_speed(wave), gust_wave / GUST);

        // A peak passes x = 0 at a quarter gust: sway's and gust's sine at 1.
        let peak = GUST_MS / 4;
        let term = (std::f32::consts::TAU * (seconds(peak) / GUST)).sin();

        assert_eq!(gusts(peak, 0.0, wave), 0.0);
        assert!((term - 1.0).abs() < 1e-6);
    }

    /// Every smoke is a particle system named smoke.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn every_smoke_is_a_particle_system_named_smoke() {
        let corpus = corpus();

        for code in SMOKES {
            let unit = Unit::find(&corpus.catalogue, code)
                .expect("listed")
                .expect("parses");
            let name = String::from_utf8_lossy(unit.desc.head.name).to_lowercase();

            assert!(name.contains("smoke"), "{code:?}: {name}");
            assert_eq!(unit.kind, Some(UnitKind::ParticleSystem), "{code:?}");
        }
    }

    /// The clock wraps at a whole gust, so no sine jumps.
    #[test]
    #[allow(clippy::float_cmp, reason = "exact in binary")]
    fn the_wind_clock_wraps_at_a_whole_gust() {
        assert_eq!(seconds(28_000), 0.0);
        assert_eq!(seconds(29_500), 1.5);
        assert_eq!((GUST / PERIOD).fract(), 0.0);
    }
}
