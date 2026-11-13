//! How the world's sprites would pack into atlases, against how they are
//! held today: one R8 index texture a tile (`regent_draw::texture`), a
//! `feet` texture beside it where the tile darkens (`regent_draw::lie`).
//!
//! ```text
//!  regent_cli atlas [--gutter N] [--held FILE]...
//!
//!  inventory  MAJESTY_HD's expansion dataset: every unit's world sets,
//!             slots (keys), layers and frames, by kind
//!  trim       texels as stored, and cut to what each tile draws
//!  packing    an atlas per key, per set, per unit (view::pack); an
//!             array per key, a layer a tile
//!  held       a drawn run's pictures (openregent's MAJ_PERF_HELD): what
//!             today makes of them, and what each atlas would
//!  timing     decode and pack, per key, per unit, per run
//! ```
//!
//! A key is what `map::pieces` draws: (unit, set, slot), every layer of
//! the set, each layer's slot effects. A tile is counted once a group
//! however many frames name it, as the art service makes it once.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::time::{Duration, Instant};

use regent_core::content::dataset::{Bytecode, Datasets, Install};
use regent_core::content::imagery::{self, Animation, Pose};
use regent_core::content::unit::Unit;
use regent_core::data::catalogue::{Catalogue, Library};
use regent_core::data::desc::UnitKind;
use regent_core::data::fourcc::FourCc;
use regent_core::data::imag::{Blit, Pass, SetKind, set_number};
use regent_core::data::tile::Mode;
use regent_core::view::pack::{self, Rect};
use regent_core::view::raster::blend;
use regent_core::view::raster::shade::{CONTROL_FIRST, TRANSPARENT};
use regent_core::view::{Colour, Ink, PaletteSource, Sprite};

use super::env;

/// Texture size limits an atlas is checked against: wgpu's default and
/// a common desktop one. **Inferred**: the target's UHD 600 takes 8192.
const LIMITS: [u32; 2] = [4096, 8192];

/// A page side within 25% of [`LIMITS`]' first.
const SIDE_NEAR: u32 = 3072;

/// wgpu's default `max_texture_array_layers`.
const ARRAY_LAYERS: usize = 256;

/// Bytes in a megabyte, as the perf log counts them.
const MB: f64 = 1024.0 * 1024.0;

/// A frame table entry: rect (four u16), anchor (two i16), palette and
/// flags (two u16).
const ENTRY_BYTES: u64 = 16;

/// Tile rows a `feet` texture keeps in R8: a drop is under the tile's
/// height (**Inferred**: an upper bound; `lie::feet` picks R16 only
/// where a drop passes 255).
const FEET_R8_ROWS: u32 = 256;

/// A direct-colour tile is flattened to RGBA8.
const RGBA_BYTES: u64 = 4;

/// Gutters the gutter table compares.
const GUTTERS: [u32; 3] = [0, 1, 2];

/// Percentiles the distributions give.
const MEDIAN: f64 = 0.5;
const P99: f64 = 0.99;

/// A unit's subtype, as the draw splits them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Kind {
    Units,
    Buildings,
    Effects,
    Missiles,
    Scatter,
    Particles,
    Other,
}

impl Kind {
    const ALL: [Kind; 7] = [
        Kind::Units,
        Kind::Buildings,
        Kind::Effects,
        Kind::Missiles,
        Kind::Scatter,
        Kind::Particles,
        Kind::Other,
    ];

    const fn of(kind: Option<UnitKind>) -> Kind {
        match kind {
            Some(UnitKind::Character) => Kind::Units,
            Some(UnitKind::Building) => Kind::Buildings,
            Some(UnitKind::Overlay) => Kind::Effects,
            Some(UnitKind::Projectile) => Kind::Missiles,
            Some(UnitKind::General) => Kind::Scatter,
            Some(UnitKind::ParticleSystem) => Kind::Particles,
            None => Kind::Other,
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Kind::Units => "units",
            Kind::Buildings => "buildings",
            Kind::Effects => "effects",
            Kind::Missiles => "missiles",
            Kind::Scatter => "scatter",
            Kind::Particles => "particles",
            Kind::Other => "other",
        }
    }
}

/// What one texture holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Grain {
    /// One atlas a key: what a spawn needs today.
    Key,
    /// One a set of an `IMAG` record, every slot; a slot effect's tiles
    /// in its own record's.
    Set,
    /// One an `IMAG` record, every set (units sharing one share it).
    Unit,
    /// One array texture a key, a layer a tile, each layer the largest.
    Array,
}

impl Grain {
    const ALL: [Grain; 4] = [Grain::Key, Grain::Set, Grain::Unit, Grain::Array];

    /// Whether a group holds a slot effect's tiles under the record they
    /// are drawn from, not each key carrying them.
    const fn lends(self) -> bool {
        matches!(self, Grain::Set | Grain::Unit)
    }

    /// The texture of `IMAG` record `image`'s set `set`, slot `slot`.
    const fn group(self, (image, set, slot): Key) -> Key {
        match self {
            Grain::Key | Grain::Array => (image, set, slot),
            Grain::Set => (image, set, 0),
            Grain::Unit => (image, 0, 0),
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Grain::Key => "atlas/key",
            Grain::Set => "atlas/set",
            Grain::Unit => "atlas/imag",
            Grain::Array => "array/key",
        }
    }
}

/// Whether a tile is packed as stored or cut to what it draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cut {
    Whole,
    Trimmed,
}

impl Cut {
    const ALL: [Cut; 2] = [Cut::Whole, Cut::Trimmed];

    const fn name(self) -> &'static str {
        match self {
            Cut::Whole => "whole",
            Cut::Trimmed => "trimmed",
        }
    }
}

/// What material draws a tile today (`map::cel::picture`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Look {
    /// Indexed, opaque: a solid body, maybe a band.
    Solid,
    /// Indexed, modes 5 and 8: a glass body and its lift.
    Glass,
    /// Direct colour, flattened.
    Colour,
}

/// Which palette a tile reads: an archive's `SPLT`, its own, or none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Colours {
    Splt(usize, usize),
    Own(Tile),
    Direct,
}

/// Archive and index of a `TILE`.
type Tile = (usize, usize);

/// A unit, set (low 16 bits) and slot.
type Key = (FourCc, u16, usize);

/// One tile as today's art service makes it.
#[derive(Debug, Clone, Copy)]
struct Info {
    w: u32,
    h: u32,
    /// The rect of texels not [`TRANSPARENT`]; `None` if all are.
    trim: Option<Rect>,
    /// Texels not [`TRANSPARENT`].
    drawn: u64,
    look: Look,
    /// Whether it draws a shadow band: a darkening pixel.
    band: bool,
    /// The rect of its band's pixels (247 to 254 as an operation).
    shade: Option<Rect>,
    /// The rect of the pixels its body draws: indices under 247.
    paint: Option<Rect>,
    mode: Mode,
    colours: Colours,
    /// Time to decode it into its index texture and bounds.
    decode: Duration,
}

impl Info {
    const fn size(&self, cut: Cut) -> (u32, u32) {
        match (cut, self.trim) {
            (Cut::Whole, _) => (self.w, self.h),
            (Cut::Trimmed, Some(r)) => (r.w, r.h),
            (Cut::Trimmed, None) => (0, 0),
        }
    }

    const fn area(&self, cut: Cut) -> u64 {
        let (w, h) = self.size(cut);

        w as u64 * h as u64
    }

    /// Bytes a texel of its main texture: R8 indices, or RGBA8.
    const fn texel_bytes(&self) -> u64 {
        match self.look {
            Look::Colour => RGBA_BYTES,
            Look::Solid | Look::Glass => 1,
        }
    }

    /// Bytes a texel of its `feet`, none without a band.
    const fn feet_bytes(&self) -> u64 {
        match (self.band, self.h <= FEET_R8_ROWS) {
            (false, _) => 0,
            (true, true) => 1,
            (true, false) => 2,
        }
    }
}

/// What a key blits: its frame references, layer by layer.
#[derive(Debug, Clone, Default)]
struct Shape {
    /// Layers (a part's stream) with a tile.
    layers: usize,
    /// Every frame reference, `None` where no tile is blitted.
    frames: Vec<Option<Tile>>,
    /// The slot effects' tiles, with the `IMAG` record and set they are
    /// drawn from: another record's art (`FXa1`'s fires).
    borrowed: BTreeSet<(FourCc, u16, Tile)>,
}

impl Shape {
    fn tiles(&self) -> BTreeSet<Tile> {
        self.frames.iter().flatten().copied().collect()
    }

    /// Its tiles less those its slot effects borrow.
    fn own(&self) -> BTreeSet<Tile> {
        let borrowed: BTreeSet<Tile> = self.borrowed.iter().map(|b| b.2).collect();

        self.tiles().difference(&borrowed).copied().collect()
    }
}

/// The dataset's world art, decoded.
struct Art<'a> {
    catalogue: &'a Catalogue<'a>,
    units: BTreeMap<FourCc, Unit<'a>>,
    kinds: BTreeMap<FourCc, Kind>,
    /// Each unit's `IMAG` record: units may share one.
    images: BTreeMap<FourCc, FourCc>,
    /// Time to parse each unit.
    parse: BTreeMap<FourCc, Duration>,
    keys: BTreeMap<Key, Shape>,
    tiles: HashMap<Tile, Option<Info>>,
}

/// Whether a set is drawn on the map: not a panel's, the minimap's or
/// the ground's.
const fn world(kind: SetKind) -> bool {
    !matches!(
        kind,
        SetKind::Interface | SetKind::Minimap | SetKind::MapTexture { .. } | SetKind::UnitTexture
    )
}

/// A key's own parts (pass, pose) and its slot effects (`IMAG` record,
/// set, slot).
type Parts<'u> = (Vec<(Pass, &'u Pose)>, Vec<(FourCc, Animation, usize)>);

/// The parts a key draws, as `map::cel::Drawing::layers` makes them:
/// each layer's passes at `slot` (else its first), then each first-pass
/// part's slot effects, last entry first.
fn parts<'u>(catalogue: &Catalogue<'_>, unit: &'u Unit<'_>, set: u16, slot: usize) -> Parts<'u> {
    let own: Vec<(Pass, &Pose)> = unit
        .stack(set)
        .flat_map(|a| a.passes.iter().map(move |p| (a, *p)))
        .filter_map(|(a, pass)| Some((pass, a.poses.get(slot).or_else(|| a.poses.first())?)))
        .collect();

    // **Inferred**: a turning effect takes the key's slot; the draw maps
    // a mobile's facing onto the effect's own slot count.
    let effects = own
        .iter()
        .filter(|(pass, _)| *pass == Pass::First)
        .flat_map(|(_, pose)| pose.effects.iter().rev())
        .filter_map(|entry| {
            let set = imagery::effect(catalogue, entry)?;
            let at = if entry.fixed_slot() {
                entry.slot() as usize
            } else {
                slot
            };

            Some((entry.image()?, set, at))
        })
        .collect();

    (own, effects)
}

/// What key `(set, slot)` of `unit` blits; `still` one frame a stream.
fn shape(
    catalogue: &Catalogue<'_>,
    unit: &Unit<'_>,
    (set, slot): (u16, usize),
    still: Option<u16>,
) -> Shape {
    let (own, effects) = parts(catalogue, unit, set, slot);
    let effect_poses = effects.iter().filter_map(|(image, set, at)| {
        let owner = (*image, set_number(set.id));

        Some((Pass::First, set.poses.get(*at)?, Some(owner)))
    });
    let mut shape = Shape::default();
    let own = own.into_iter().map(|(pass, pose)| (pass, pose, None));

    for (pass, pose, owner) in own.chain(effect_poses) {
        for stream in 0..pose.streams.max(1) {
            let blits = pose.blits(stream, pass);
            let wanted = match still {
                Some(at) => blits
                    .get(usize::from(at))
                    .or_else(|| blits.first())
                    .copied()
                    .into_iter()
                    .collect(),
                None => blits,
            };
            let frames: Vec<Option<Tile>> = wanted
                .into_iter()
                .map(|b| b.map(|b| (b.archive, b.index)))
                .collect();

            if frames.iter().any(Option::is_some) {
                shape.layers += 1;
            }

            if let Some((image, set)) = owner {
                shape
                    .borrowed
                    .extend(frames.iter().flatten().map(|t| (image, set, *t)));
            }

            shape.frames.extend(frames);
        }
    }

    shape
}

/// How much of a tile [`decode`] works out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Measure {
    /// What a load needs: indices, band, bounds. `shade` and `drawn`
    /// are left empty, so a timing is a load's.
    Load,
    /// Everything [`Info`] holds.
    All,
}

/// A tile's index texture as `texture::indices` makes it (a hole
/// [`TRANSPARENT`]; direct colour: drawn 0, hole 255), and what it is.
fn decode(
    catalogue: &Catalogue<'_>,
    (archive, index): Tile,
    measure: Measure,
) -> Option<(Info, Vec<u8>)> {
    let began = Instant::now();
    let sprite = Sprite::at(catalogue, archive, index)?.ok()?;
    let (w, h) = (u32::from(sprite.width), u32::from(sprite.height));

    if w == 0 || h == 0 {
        return None;
    }

    let shades =
        |i: usize| sprite.covered[i] && matches!(sprite.ink(i, Blit::World), Ink::Shade(_));
    let (data, look, band, colours) = match &sprite.colour {
        Colour::Indexed { indices, palette } => {
            let data: Vec<u8> = indices
                .iter()
                .zip(&sprite.covered)
                .map(|(i, c)| if *c { *i } else { TRANSPARENT })
                .collect();
            let glass = blend::alphas(sprite.mode, sprite.level, palette).is_some();
            let band = !glass && (0..data.len()).any(shades);
            let look = if glass { Look::Glass } else { Look::Solid };
            let colours = match sprite.source {
                PaletteSource::Splt { archive, index } => Colours::Splt(archive, index),
                _ => Colours::Own((archive, index)),
            };

            (data, look, band, colours)
        }
        Colour::Direct { .. } => {
            let data = sprite
                .covered
                .iter()
                .map(|c| if *c { 0 } else { TRANSPARENT })
                .collect();

            (data, Look::Colour, false, Colours::Direct)
        }
    };
    let trim = pack::bounds(w, h, |i| data[i] != TRANSPARENT);

    // The load's work ends here; the rest is measurement.
    let decode = began.elapsed();
    let (shade, paint, drawn) = match measure {
        Measure::Load => (None, None, 0),
        Measure::All => (
            band.then(|| pack::bounds(w, h, shades)).flatten(),
            pack::bounds(w, h, |i| data[i] < CONTROL_FIRST),
            data.iter().filter(|i| **i != TRANSPARENT).count() as u64,
        ),
    };
    let info = Info {
        w,
        h,
        trim,
        drawn,
        look,
        band,
        shade,
        paint,
        mode: sprite.mode,
        colours,
        decode,
    };

    Some((info, data))
}

impl<'a> Art<'a> {
    /// Every unit with world art in `catalogue`, every key of it, every
    /// tile they name decoded.
    fn read(catalogue: &'a Catalogue<'a>) -> Art<'a> {
        let codes: BTreeSet<FourCc> = catalogue.tagged(b"DUNT").map(|(_, r)| r.id).collect();
        let mut art = Art {
            catalogue,
            units: BTreeMap::new(),
            kinds: BTreeMap::new(),
            images: BTreeMap::new(),
            parse: BTreeMap::new(),
            keys: BTreeMap::new(),
            tiles: HashMap::new(),
        };

        for code in codes {
            let began = Instant::now();
            let Some(Ok(unit)) = Unit::find(catalogue, code) else {
                continue;
            };

            art.parse.insert(code, began.elapsed());
            art.kinds.insert(code, Kind::of(unit.kind));
            art.images.insert(code, unit.art.map_or(code, |l| l.id));
            art.units.insert(code, unit);
        }

        let keys: Vec<Key> = art
            .units
            .iter()
            .flat_map(|(code, unit)| world_keys(*code, unit))
            .collect();

        for key in keys {
            let shape = art.shape(key, None);

            art.keys.insert(key, shape);
        }

        let tiles: BTreeSet<Tile> = art.keys.values().flat_map(Shape::tiles).collect();

        for tile in tiles {
            art.info(tile);
        }

        art
    }

    /// Key `key`'s shape, `still` one frame a stream.
    fn shape(&self, (code, set, slot): Key, still: Option<u16>) -> Shape {
        self.units
            .get(&code)
            .map(|unit| shape(self.catalogue, unit, (set, slot), still))
            .unwrap_or_default()
    }

    /// Tile `tile`, decoded once.
    fn info(&mut self, tile: Tile) -> Option<Info> {
        let catalogue = self.catalogue;

        *self
            .tiles
            .entry(tile)
            .or_insert_with(|| decode(catalogue, tile, Measure::All).map(|(info, _)| info))
    }

    /// Every tile `held` names, or its keys do, decoded.
    fn cover(&mut self, held: &Held) {
        let tiles: BTreeSet<Tile> = held
            .keys
            .iter()
            .flat_map(|(key, _)| self.shape(*key, None).tiles())
            .chain(held.tiles.iter().copied())
            .collect();

        for tile in tiles {
            self.info(tile);
        }
    }

    fn known(&self, tile: &Tile) -> Option<&Info> {
        self.tiles.get(tile).and_then(Option::as_ref)
    }

    fn kind(&self, code: FourCc) -> Kind {
        self.kinds.get(&code).copied().unwrap_or(Kind::Other)
    }

    /// Each group's tiles for `keys` at `grain`: the tiles of every key
    /// it holds.
    fn groups<'k>(
        &self,
        keys: impl Iterator<Item = (&'k Key, &'k Shape)>,
        grain: Grain,
    ) -> BTreeMap<Key, BTreeSet<Tile>> {
        let mut groups: BTreeMap<Key, BTreeSet<Tile>> = BTreeMap::new();

        for (&(code, set, slot), shape) in keys {
            let (image, tiles) = if grain.lends() {
                (self.images.get(&code).copied().unwrap_or(code), shape.own())
            } else {
                (code, shape.tiles())
            };

            groups
                .entry(grain.group((image, set, slot)))
                .or_default()
                .extend(tiles.into_iter().filter(|t| self.known(t).is_some()));

            for (image, set, tile) in shape.borrowed.iter().filter(|_| grain.lends()) {
                if self.known(tile).is_some() {
                    groups
                        .entry(grain.group((*image, *set, 0)))
                        .or_default()
                        .insert(*tile);
                }
            }
        }

        groups
    }
}

/// Every world key of `unit`: each world set, each slot of its first layer.
fn world_keys(code: FourCc, unit: &Unit<'_>) -> Vec<Key> {
    let sets: BTreeSet<u16> = unit
        .animations
        .iter()
        .filter(|a| world(a.kind))
        .map(|a| set_number(a.id))
        .collect();

    sets.into_iter()
        .flat_map(|set| {
            let slots = unit.stack(set).next().map_or(0, |a| a.poses.len()).max(1);

            (0..slots).map(move |slot| (code, set, slot))
        })
        .collect()
}

/// One texture of a packing, and what it holds.
#[derive(Debug, Clone, Copy, Default)]
struct Texture {
    width: u32,
    height: u32,
    /// Array layers; 1 for an atlas.
    layers: usize,
    /// Texels the tiles cover.
    used: u64,
}

impl Texture {
    const fn area(&self) -> u64 {
        self.width as u64 * self.height as u64 * self.layers as u64
    }
}

/// `tiles` as one texture of `grain` at `cut`, `gutter` apart.
fn texture(
    art: &Art<'_>,
    tiles: &BTreeSet<Tile>,
    (grain, cut, gutter): (Grain, Cut, u32),
) -> Texture {
    let sizes: Vec<(u32, u32)> = tiles
        .iter()
        .filter_map(|t| art.known(t))
        .map(|i| i.size(cut))
        .filter(|(w, h)| *w > 0 && *h > 0)
        .collect();
    let used = sizes
        .iter()
        .map(|(w, h)| u64::from(*w) * u64::from(*h))
        .sum();

    match grain {
        Grain::Array => Texture {
            width: sizes.iter().map(|s| s.0).max().unwrap_or(0),
            height: sizes.iter().map(|s| s.1).max().unwrap_or(0),
            layers: sizes.len(),
            used,
        },
        Grain::Key | Grain::Set | Grain::Unit => {
            let packed = pack::pack(&sizes, gutter, LIMITS[0]);

            Texture {
                width: packed.width,
                height: packed.height,
                layers: 1,
                used,
            }
        }
    }
}

/// A packing's totals.
#[derive(Debug, Clone, Copy, Default)]
struct Totals {
    textures: usize,
    area: u64,
    used: u64,
    largest: (u32, u32),
    /// Textures past each of [`LIMITS`].
    over: [usize; 2],
    /// Arrays past [`ARRAY_LAYERS`].
    deep: usize,
}

impl Totals {
    fn add(&mut self, t: Texture) {
        if t.used == 0 {
            return;
        }

        self.textures += 1;
        self.area += t.area();
        self.used += t.used;

        if u64::from(t.width) * u64::from(t.height)
            > u64::from(self.largest.0) * u64::from(self.largest.1)
        {
            self.largest = (t.width, t.height);
        }

        for (over, limit) in self.over.iter_mut().zip(LIMITS) {
            *over += usize::from(t.width.max(t.height) > limit);
        }

        self.deep += usize::from(t.layers > ARRAY_LAYERS);
    }

    #[allow(clippy::cast_precision_loss, reason = "a share, shown to 0.1%")]
    fn waste(&self) -> f64 {
        if self.area == 0 {
            0.0
        } else {
            100.0 * (1.0 - self.used as f64 / self.area as f64)
        }
    }
}

/// `groups` packed at `grain`, `cut`, `gutter`.
fn totals(art: &Art<'_>, groups: &BTreeMap<Key, BTreeSet<Tile>>, how: (Grain, Cut, u32)) -> Totals {
    let mut totals = Totals::default();

    for tiles in groups.values() {
        totals.add(texture(art, tiles, how));
    }

    totals
}

#[allow(clippy::cast_precision_loss, reason = "megabytes, shown to 0.1")]
fn mb(bytes: u64) -> f64 {
    bytes as f64 / MB
}

#[allow(clippy::cast_precision_loss, reason = "a share, shown to 0.1%")]
fn share(part: u64, whole: u64) -> f64 {
    if whole == 0 {
        0.0
    } else {
        100.0 * part as f64 / whole as f64
    }
}

/// The `p` quantile of sorted `values`.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "an index into a short list"
)]
fn quantile<T: Copy + Default>(sorted: &[T], p: f64) -> T {
    if sorted.is_empty() {
        T::default()
    } else {
        sorted[((sorted.len() - 1) as f64 * p).round() as usize]
    }
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1e3
}

/// The inventory table: counts and texels by kind.
fn inventory(art: &Art<'_>) {
    println!("== inventory: world sets of MAJESTY_HD's expansion dataset");
    println!(
        "  {:<10} {:>5} {:>5} {:>6} {:>7} {:>8} {:>6} {:>8} {:>8} {:>8}",
        "kind",
        "units",
        "sets",
        "keys",
        "layers",
        "frames",
        "tiles",
        "refs Mt",
        "tiles Mt",
        "trim Mt"
    );

    let mut all = (0, 0, 0, 0, 0, BTreeSet::new(), 0u64);

    for kind in Kind::ALL {
        let keys: Vec<(&Key, &Shape)> = art
            .keys
            .iter()
            .filter(|(k, _)| art.kind(k.0) == kind)
            .collect();

        if keys.is_empty() {
            continue;
        }

        let units: BTreeSet<FourCc> = keys.iter().map(|(k, _)| k.0).collect();
        let sets: BTreeSet<(FourCc, u16)> = keys.iter().map(|(k, _)| (k.0, k.1)).collect();
        let layers: usize = keys.iter().map(|(_, s)| s.layers).sum();
        let frames: usize = keys.iter().map(|(_, s)| s.frames.len()).sum();
        let tiles: BTreeSet<Tile> = keys.iter().flat_map(|(_, s)| s.tiles()).collect();
        let refs: u64 = keys
            .iter()
            .flat_map(|(_, s)| s.frames.iter().flatten())
            .filter_map(|t| art.known(t))
            .map(|i| i.area(Cut::Whole))
            .sum();
        let (whole, trimmed) = areas(art, &tiles);

        println!(
            "  {:<10} {:>5} {:>5} {:>6} {:>7} {:>8} {:>6} {:>8.1} {:>8.1} {:>8.1}",
            kind.name(),
            units.len(),
            sets.len(),
            keys.len(),
            layers,
            frames,
            tiles.len(),
            mega(refs),
            mega(whole),
            mega(trimmed)
        );

        all.0 += units.len();
        all.1 += sets.len();
        all.2 += keys.len();
        all.3 += layers;
        all.4 += frames;
        all.5.extend(tiles);
        all.6 += refs;
    }

    let (whole, trimmed) = areas(art, &all.5);

    println!(
        "  {:<10} {:>5} {:>5} {:>6} {:>7} {:>8} {:>6} {:>8.1} {:>8.1} {:>8.1}",
        "all",
        all.0,
        all.1,
        all.2,
        all.3,
        all.4,
        all.5.len(),
        mega(all.6),
        mega(whole),
        mega(trimmed)
    );
    println!(
        "  Mt: millions of texels (R8: MB). refs: each frame reference its tile's; tiles: each distinct tile once"
    );
}

#[allow(clippy::cast_precision_loss, reason = "kilobytes, shown to 0.1")]
fn kb(bytes: u64) -> f64 {
    bytes as f64 / 1024.0
}

#[allow(clippy::cast_precision_loss, reason = "millions, shown to 0.1")]
fn mega(n: u64) -> f64 {
    n as f64 / 1e6
}

/// Whole and trimmed texels of `tiles`.
fn areas(art: &Art<'_>, tiles: &BTreeSet<Tile>) -> (u64, u64) {
    tiles
        .iter()
        .filter_map(|t| art.known(t))
        .fold((0, 0), |(w, t), i| {
            (w + i.area(Cut::Whole), t + i.area(Cut::Trimmed))
        })
}

/// Single tile sizes: median, p99, max width and height.
fn sizes(art: &Art<'_>) {
    let infos: Vec<&Info> = art.tiles.values().flatten().collect();
    let mut widths: Vec<u32> = infos.iter().map(|i| i.w).collect();
    let mut heights: Vec<u32> = infos.iter().map(|i| i.h).collect();
    let mut tw: Vec<u32> = infos.iter().filter_map(|i| i.trim).map(|r| r.w).collect();
    let mut th: Vec<u32> = infos.iter().filter_map(|i| i.trim).map(|r| r.h).collect();

    for v in [&mut widths, &mut heights, &mut tw, &mut th] {
        v.sort_unstable();
    }

    println!("== tile sizes, {} distinct tiles", infos.len());
    println!("  {:<14} {:>6} {:>6} {:>6}", "", "median", "p99", "max");

    for (name, v) in [
        ("width", &widths),
        ("height", &heights),
        ("trimmed w", &tw),
        ("trimmed h", &th),
    ] {
        println!(
            "  {name:<14} {:>6} {:>6} {:>6}",
            quantile(v, MEDIAN),
            quantile(v, P99),
            v.last().copied().unwrap_or(0)
        );
    }

    let looks = |look: Look| infos.iter().filter(|i| i.look == look).count();

    println!(
        "  solid {}, glass {}, direct colour {}; empty (all holes) {}",
        looks(Look::Solid),
        looks(Look::Glass),
        looks(Look::Colour),
        infos.iter().filter(|i| i.trim.is_none()).count()
    );
}

/// Trimming by kind: texels, and the share of a quad that draws nothing.
fn trimming(art: &Art<'_>) {
    println!("== trimming: distinct tiles by kind");
    println!(
        "  {:<10} {:>9} {:>9} {:>7} {:>9} {:>9}",
        "kind", "whole Mt", "trim Mt", "saved", "empty now", "empty cut"
    );

    for kind in Kind::ALL {
        let tiles: BTreeSet<Tile> = art
            .keys
            .iter()
            .filter(|(k, _)| art.kind(k.0) == kind)
            .flat_map(|(_, s)| s.tiles())
            .collect();

        if tiles.is_empty() {
            continue;
        }

        let (whole, trimmed) = areas(art, &tiles);
        let drawn: u64 = tiles
            .iter()
            .filter_map(|t| art.known(t))
            .map(|i| i.drawn)
            .sum();

        println!(
            "  {:<10} {:>9.1} {:>9.1} {:>6.1}% {:>8.1}% {:>8.1}%",
            kind.name(),
            mega(whole),
            mega(trimmed),
            100.0 - share(trimmed, whole),
            100.0 - share(drawn, whole),
            100.0 - share(drawn, trimmed)
        );
    }

    println!("  empty: texels of a quad that draw nothing (holes), by area");
}

/// The packing table at `gutter`, every grain and cut; then the gutters
/// compared.
fn packing(art: &Art<'_>, gutter: u32) {
    println!("== packing, every key, gutter {gutter}");
    println!(
        "  {:<11} {:<8} {:>7} {:>9} {:>7} {:>11} {:>6} {:>6} {:>6}",
        "grain", "cut", "count", "MB", "waste", "largest", ">4096", ">8192", ">256 l"
    );

    for grain in Grain::ALL {
        let groups = art.groups(art.keys.iter(), grain);

        for cut in Cut::ALL {
            let t = totals(art, &groups, (grain, cut, gutter));

            println!(
                "  {:<11} {:<8} {:>7} {:>9.1} {:>6.1}% {:>11} {:>6} {:>6} {:>6}",
                grain.name(),
                cut.name(),
                t.textures,
                mb(t.area),
                t.waste(),
                format!("{}x{}", t.largest.0, t.largest.1),
                t.over[0],
                t.over[1],
                t.deep
            );
        }
    }

    println!(
        "  MB at a byte a texel; waste: atlas texels no tile covers; >256 l: arrays past wgpu's default layers"
    );
    println!("== gutters, trimmed: total MB");
    println!(
        "  {:<11} {}",
        "grain",
        GUTTERS.map(|g| format!("{g:>8}")).join("")
    );

    for grain in [Grain::Key, Grain::Set, Grain::Unit] {
        let groups = art.groups(art.keys.iter(), grain);
        let row: Vec<String> = GUTTERS
            .iter()
            .map(|g| {
                format!(
                    "{:>8.1}",
                    mb(totals(art, &groups, (grain, Cut::Trimmed, *g)).area)
                )
            })
            .collect();

        println!("  {:<11} {}", grain.name(), row.join(""));
    }
}

/// Shadows: band tiles, their `feet`, by kind.
fn shadows(art: &Art<'_>) {
    println!("== shadows: band tiles (a darkening pixel), every key");
    println!(
        "  {:<10} {:>6} {:>9} {:>9} {:>9}",
        "kind", "bands", "share Mt", "feet MB", "band box"
    );

    for kind in Kind::ALL {
        let tiles: BTreeSet<Tile> = art
            .keys
            .iter()
            .filter(|(k, _)| art.kind(k.0) == kind)
            .flat_map(|(_, s)| s.tiles())
            .collect();
        let bands: Vec<&Info> = tiles
            .iter()
            .filter_map(|t| art.known(t))
            .filter(|i| i.band)
            .collect();

        if bands.is_empty() {
            continue;
        }

        let shared: u64 = bands.iter().map(|i| i.area(Cut::Whole)).sum();
        let feet: u64 = bands
            .iter()
            .map(|i| i.area(Cut::Whole) * i.feet_bytes())
            .sum();
        let cut: u64 = bands
            .iter()
            .map(|i| i.shade.map_or(0, |r| r.area()) * i.feet_bytes())
            .sum();

        println!(
            "  {:<10} {:>6} {:>9.1} {:>9.1} {:>9.1}",
            kind.name(),
            bands.len(),
            mega(shared),
            mb(feet),
            mb(cut)
        );
    }

    println!(
        "  share: the band reads the body's index texture; feet: its own texture, R8 (R16 past {FEET_R8_ROWS} rows); band box: feet cut to the band's pixels"
    );
}

/// The frame table: entries a key and in all.
fn table(art: &Art<'_>) {
    let mut refs: Vec<usize> = art.keys.values().map(|s| s.frames.len()).collect();
    let mut layers: Vec<usize> = art.keys.values().map(|s| s.layers).collect();
    let all: usize = refs.iter().sum();
    let palettes = art
        .keys
        .values()
        .filter(|s| {
            let found: BTreeSet<String> = s
                .tiles()
                .iter()
                .filter_map(|t| art.known(t))
                .map(|i| format!("{:?}", i.colours))
                .collect();

            found.len() > 1
        })
        .count();
    let looks = art
        .keys
        .values()
        .filter(|s| {
            let found: BTreeSet<String> = s
                .tiles()
                .iter()
                .filter_map(|t| art.known(t))
                .map(|i| format!("{:?}", i.look))
                .collect();

            found.len() > 1
        })
        .count();

    refs.sort_unstable();
    layers.sort_unstable();

    println!("== frame table, {ENTRY_BYTES} bytes a frame reference");
    println!(
        "  a key: frames median {} p99 {} max {}; layers median {} max {}",
        quantile(&refs, MEDIAN),
        quantile(&refs, P99),
        refs.last().copied().unwrap_or(0),
        quantile(&layers, MEDIAN),
        layers.last().copied().unwrap_or(0)
    );
    println!(
        "  all keys: {all} references, {:.2} MB; keys with tiles of more than one palette {palettes}, of solid and glass {looks}",
        mb(all as u64 * ENTRY_BYTES)
    );

    for grain in [Grain::Key, Grain::Set, Grain::Unit] {
        let mut counts: Vec<usize> = art
            .groups(art.keys.iter(), grain)
            .values()
            .map(|tiles| {
                let found: BTreeSet<Colours> = tiles
                    .iter()
                    .filter_map(|t| art.known(t))
                    .map(|i| i.colours)
                    .collect();

                found.len()
            })
            .collect();

        counts.sort_unstable();
        println!(
            "  palettes a texture, {:<10}: median {} p99 {} max {}",
            grain.name(),
            quantile(&counts, MEDIAN),
            quantile(&counts, P99),
            counts.last().copied().unwrap_or(0)
        );
    }
}

/// What a drawn run held (openregent `MAJ_PERF_HELD`).
struct Held {
    /// Keys and the frame each holds, `None` the whole stream.
    keys: BTreeSet<(Key, Option<u16>)>,
    /// Tiles made into materials.
    tiles: BTreeSet<Tile>,
    /// Tile lines naming an archive this catalogue lacks.
    unknown: usize,
}

impl Held {
    /// The dump at `path`, its archive names read against `catalogue`.
    fn read(path: &str, catalogue: &Catalogue<'_>) -> Result<Held, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
        let archives: HashMap<&str, usize> = catalogue
            .archives
            .iter()
            .enumerate()
            .map(|(i, (name, _))| (*name, i))
            .collect();
        let mut held = Held {
            keys: BTreeSet::new(),
            tiles: BTreeSet::new(),
            unknown: 0,
        };

        for line in text.lines() {
            let words: Vec<&str> = line.split('\t').collect();

            match words.as_slice() {
                ["piece" | "thing", code, set, slot, still] => {
                    let code = super::util::fourcc(code)?;
                    let set = set.parse().map_err(|_| format!("{path}: set {set}"))?;
                    let slot = slot.parse().map_err(|_| format!("{path}: slot {slot}"))?;
                    let still = still.parse().ok();

                    held.keys.insert(((code, set, slot), still));
                }
                ["tile", archive, index] => {
                    let index = index.parse().map_err(|_| format!("{path}: tile {index}"))?;

                    match archives.get(archive) {
                        Some(a) => {
                            held.tiles.insert((*a, index));
                        }
                        None => held.unknown += 1,
                    }
                }
                _ => return Err(format!("{path}: a line not understood: {line}")),
            }
        }

        Ok(held)
    }
}

/// Today's materials for `tiles`: a body each (a glass one and its lift),
/// a band each where it darkens.
fn materials_now(art: &Art<'_>, tiles: &BTreeSet<Tile>) -> usize {
    tiles
        .iter()
        .filter_map(|t| art.known(t))
        .map(|i| match i.look {
            Look::Glass => 2,
            Look::Solid => 1 + usize::from(i.band),
            Look::Colour => 1,
        })
        .sum()
}

/// An atlas's materials: a body a group, palette and look (a glass one
/// with its lift), a band a group and mode, over the tiles `shown`.
fn materials_atlas(
    art: &Art<'_>,
    groups: &BTreeMap<Key, BTreeSet<Tile>>,
    shown: &BTreeSet<Tile>,
) -> usize {
    let mut bodies = BTreeSet::new();
    let mut bands = BTreeSet::new();

    for (group, tiles) in groups {
        for info in tiles
            .iter()
            .filter(|t| shown.contains(t))
            .filter_map(|t| art.known(t))
        {
            let colours = format!("{:?}", info.colours);
            let look = format!("{:?}", info.look);

            if info.look == Look::Glass {
                bodies.insert((*group, colours.clone(), "lift".to_owned()));
            }

            if info.band {
                bands.insert((*group, format!("{:?}", info.mode)));
            }

            bodies.insert((*group, colours, look));
        }
    }

    bodies.len() + bands.len()
}

/// The textures a run holding keys `full` loads at `grain`: a key's
/// whole stream, else every group its keys touch, whole.
fn held_groups(
    art: &Art<'_>,
    full: &BTreeMap<Key, Shape>,
    grain: Grain,
) -> BTreeMap<Key, BTreeSet<Tile>> {
    let touched = art.groups(full.iter(), grain);

    if grain.lends() {
        let mut all = art.groups(art.keys.iter().chain(full.iter()), grain);

        all.retain(|id, _| touched.contains_key(id));
        all
    } else {
        touched
    }
}

/// What a run made today: its keys and tiles, what they take, and
/// their materials.
fn held_today(art: &Art<'_>, held: &Held, shapes: &BTreeMap<Key, Shape>, made: &BTreeSet<Tile>) {
    let explained: BTreeSet<Tile> = shapes.values().flat_map(Shape::tiles).collect();
    let outside: BTreeSet<Tile> = made.difference(&explained).copied().collect();
    let bytes = |tiles: &BTreeSet<Tile>| -> (u64, u64) {
        tiles
            .iter()
            .filter_map(|t| art.known(t))
            .fold((0, 0), |(b, f), i| {
                (
                    b + i.area(Cut::Whole) * i.texel_bytes(),
                    f + i.area(Cut::Whole) * i.feet_bytes(),
                )
            })
    };
    let (now, feet) = bytes(made);
    let (outside_bytes, _) = bytes(&outside);
    let units: BTreeSet<FourCc> = held.keys.iter().map(|((c, ..), _)| *c).collect();
    let (whole, trimmed) = areas(art, made);
    let drawn: u64 = made
        .iter()
        .filter_map(|t| art.known(t))
        .map(|i| i.drawn)
        .sum();

    println!(
        "  {} keys ({} holding one frame), {} units; {} tiles made ({} not in this catalogue), {} outside any key ({:.1} MB: rings, previews)",
        held.keys.len(),
        held.keys.iter().filter(|(_, s)| s.is_some()).count(),
        units.len(),
        made.len(),
        held.unknown,
        outside.len(),
        mb(outside_bytes)
    );

    println!(
        "  made tiles: trimming keeps {:.1}% of texels; empty share of a quad {:.1}% whole, {:.1}% trimmed",
        share(trimmed, whole),
        100.0 - share(drawn, whole),
        100.0 - share(drawn, trimmed)
    );
    let bands: Vec<&Info> = made
        .iter()
        .filter_map(|t| art.known(t))
        .filter(|i| i.band)
        .collect();
    let band_whole: u64 = bands.iter().map(|i| i.area(Cut::Whole)).sum();
    let band_box: u64 = bands.iter().map(|i| i.shade.map_or(0, |r| r.area())).sum();
    let refs: usize = shapes.values().map(|s| s.frames.len()).sum();
    let paint: u64 = made
        .iter()
        .filter_map(|t| art.known(t))
        .map(|i| i.paint.map_or(0, |r| r.area()))
        .sum();

    println!(
        "  made bodies: their colour box (indices under {CONTROL_FIRST}) is {:.1}% of their quads",
        share(paint, whole)
    );
    println!(
        "  made bands: {}; their band box is {:.1}% of their quads; frame table {refs} references, {:.1} KB",
        bands.len(),
        share(band_box, band_whole),
        kb(refs as u64 * ENTRY_BYTES)
    );
    println!(
        "  today: indices {:.1} MB (and as much again on the CPU: MAIN_WORLD), feet {:.1} MB, {} textures, {} materials",
        mb(now),
        mb(feet),
        made.len()
            + made
                .iter()
                .filter_map(|t| art.known(t))
                .filter(|i| i.band)
                .count(),
        materials_now(art, made)
    );
}

/// A drawn run's art, today and at each grain.
fn held_report(art: &Art<'_>, path: &str, held: &Held, gutter: u32) {
    let shapes: BTreeMap<Key, Shape> = held
        .keys
        .iter()
        .map(|(key, still)| (*key, art.shape(*key, *still)))
        .collect();
    let made: BTreeSet<Tile> = held
        .tiles
        .iter()
        .filter(|t| art.known(t).is_some())
        .copied()
        .collect();

    println!("== held: {path}");
    held_today(art, held, &shapes, &made);

    let full: BTreeMap<Key, Shape> = held
        .keys
        .iter()
        .map(|(key, _)| (*key, art.shape(*key, None)))
        .collect();

    println!(
        "  {:<11} {:<8} {:>7} {:>8} {:>8} {:>9} {:>9}",
        "grain", "cut", "count", "MB", "unshown", "feet MB", "materials"
    );

    for grain in Grain::ALL {
        let groups = held_groups(art, &full, grain);

        for cut in Cut::ALL {
            let mut t = Totals::default();
            let mut feet = 0;

            for tiles in groups.values() {
                let tex = texture(art, tiles, (grain, cut, gutter));
                let deepest = tiles
                    .iter()
                    .filter_map(|t| art.known(t))
                    .map(Info::feet_bytes)
                    .max();

                feet += tex.area() * deepest.unwrap_or(0);
                t.add(tex);
            }

            let unshown: u64 = groups
                .values()
                .flatten()
                .collect::<BTreeSet<_>>()
                .into_iter()
                .filter(|t| !made.contains(t))
                .filter_map(|t| art.known(t))
                .map(|i| i.area(cut))
                .sum();

            println!(
                "  {:<11} {:<8} {:>7} {:>8.1} {:>8.1} {:>9.1} {:>9}",
                grain.name(),
                cut.name(),
                t.textures,
                mb(t.area),
                mb(unshown),
                mb(feet),
                materials_atlas(art, &groups, &made)
            );
        }
    }

    println!(
        "  unshown: tiles an atlas holds that the run never made; feet: a parallel atlas where any frame darkens"
    );

    timing_held(art, &shapes, (&made, &full), gutter);
}

/// The run's load cost: today's (unit parse and tile decode), and each
/// grain's (decode, pack and copy into the atlas), measured anew.
fn timing_held(
    art: &Art<'_>,
    shapes: &BTreeMap<Key, Shape>,
    (made, full): (&BTreeSet<Tile>, &BTreeMap<Key, Shape>),
    gutter: u32,
) {
    let units: BTreeSet<FourCc> = shapes.keys().map(|k| k.0).collect();
    let parse: Duration = units.iter().filter_map(|c| art.parse.get(c)).sum();
    let began = Instant::now();

    for tile in made {
        let _ = decode(art.catalogue, *tile, Measure::Load);
    }

    let today = began.elapsed();

    println!(
        "  load: today {:.1} ms decoding {} tiles, {:.1} ms parsing {} units",
        ms(today),
        made.len(),
        ms(parse),
        units.len()
    );

    for grain in [Grain::Key, Grain::Set, Grain::Unit] {
        let groups = held_groups(art, full, grain);
        let began = Instant::now();

        for tiles in groups.values() {
            let _ = build(art, tiles, gutter);
        }

        println!(
            "        {:<11} {:.1} ms to decode, pack and copy {} atlases",
            grain.name(),
            ms(began.elapsed()),
            groups.len()
        );
    }
}

/// `tiles` decoded, trimmed, packed and copied into one R8 atlas: the
/// work an atlas's load does before upload.
fn build(art: &Art<'_>, tiles: &BTreeSet<Tile>, gutter: u32) -> Vec<u8> {
    let decoded: Vec<(Info, Vec<u8>)> = tiles
        .iter()
        .filter_map(|t| decode(art.catalogue, *t, Measure::Load))
        .collect();
    let sizes: Vec<(u32, u32)> = decoded.iter().map(|(i, _)| i.size(Cut::Trimmed)).collect();
    let packed = pack::pack(&sizes, gutter, LIMITS[0]);
    let width = packed.width as usize;
    let mut atlas = vec![TRANSPARENT; width * packed.height as usize];

    for ((info, data), at) in decoded.iter().zip(&packed.rects) {
        let Some(trim) = info.trim else { continue };
        let (w, x0) = (trim.w as usize, trim.x as usize);

        for row in 0..trim.h as usize {
            let from = (trim.y as usize + row) * info.w as usize + x0;
            let to = (at.y as usize + row) * width + at.x as usize;

            atlas[to..to + w].copy_from_slice(&data[from..from + w]);
        }
    }

    atlas
}

/// Load cost over every key and unit: decode (measured once a tile) and
/// pack.
fn timing(art: &Art<'_>, gutter: u32) {
    let decode = |tiles: &BTreeSet<Tile>| -> Duration {
        tiles
            .iter()
            .filter_map(|t| art.known(t))
            .map(|i| i.decode)
            .sum()
    };
    let pack_time = |tiles: &BTreeSet<Tile>| -> Duration {
        let sizes: Vec<(u32, u32)> = tiles
            .iter()
            .filter_map(|t| art.known(t))
            .map(|i| i.size(Cut::Trimmed))
            .collect();
        let began = Instant::now();
        let _ = pack::pack(&sizes, gutter, LIMITS[0]);

        began.elapsed()
    };
    let spread = |mut v: Vec<Duration>| -> String {
        v.sort_unstable();

        format!(
            "median {:.2} p99 {:.2} max {:.2} ms",
            ms(quantile(&v, MEDIAN)),
            ms(quantile(&v, P99)),
            ms(v.last().copied().unwrap_or_default())
        )
    };

    let keys = art.groups(art.keys.iter(), Grain::Key);
    let units = art.groups(art.keys.iter(), Grain::Unit);
    let key_decode: Vec<Duration> = keys.values().map(decode).collect();
    let key_pack: Vec<Duration> = keys.values().map(pack_time).collect();
    let unit_load: Vec<Duration> = units
        .iter()
        .map(|((code, ..), tiles)| art.parse.get(code).copied().unwrap_or_default() + decode(tiles))
        .collect();
    let unit_pack: Vec<Duration> = units.values().map(pack_time).collect();
    let all: Duration = art.tiles.values().flatten().map(|i| i.decode).sum();

    println!("== timing, this machine, one thread");
    println!("  a key:  decode {}", spread(key_decode));
    println!("          pack   {}", spread(key_pack));
    println!("  a unit: parse and decode {}", spread(unit_load));
    println!("          pack   {}", spread(unit_pack));
    println!("  every tile decoded: {:.0} ms", ms(all));
}

/// Whether set kind `kind` is damage or rubble: a building's `Die`
/// stages (fires and wrecked tiles), `Dead`, `Crumble` and `Damage`.
/// `sim::damage` picks them only as hit points fall; on a character
/// `Die` and `Dead` are its death and corpse.
const fn spare(kind: SetKind) -> bool {
    matches!(
        kind,
        SetKind::Die | SetKind::Dead | SetKind::Crumble | SetKind::Damage
    )
}

/// One `IMAG` record's tiles: those only its damage and rubble sets
/// draw, and the rest.
#[derive(Default)]
struct Split {
    spare: BTreeSet<Tile>,
    rest: BTreeSet<Tile>,
}

/// Each `IMAG` record's [`Split`], from its set groups.
fn splits(art: &Art<'_>, keys: &BTreeMap<Key, Shape>) -> BTreeMap<FourCc, Split> {
    let mut found: BTreeMap<FourCc, Split> = BTreeMap::new();

    for ((image, set, _), tiles) in art.groups(keys.iter(), Grain::Set) {
        let building = image_kind(art, image) == Kind::Buildings;
        let split = found.entry(image).or_default();

        // A character's death and corpse show whenever one dies.
        if building && spare(SetKind::from_id(set)) {
            split.spare.extend(tiles);
        } else {
            split.rest.extend(tiles);
        }
    }

    for split in found.values_mut() {
        split.spare = split.spare.difference(&split.rest).copied().collect();
    }

    found
}

/// The kind an `IMAG` record draws: its first unit's.
fn image_kind(art: &Art<'_>, image: FourCc) -> Kind {
    art.images
        .iter()
        .find(|(_, i)| **i == image)
        .map_or(Kind::Other, |(code, _)| art.kind(*code))
}

/// Part A: damage and rubble apart, one page a record, upgrades.
fn sheets(art: &Art<'_>, runs: &[(&String, Held)], gutter: u32) {
    let found = splits(art, &art.keys);

    println!(
        "== a building's damage and rubble (Die, Dead, Crumble, Damage) apart; tiles only they draw"
    );
    println!(
        "  {:<10} {:>9} {:>9} {:>7}",
        "kind", "rest Mt", "spare Mt", "share"
    );

    for kind in Kind::ALL {
        let of: Vec<&Split> = found
            .iter()
            .filter(|(i, _)| image_kind(art, **i) == kind)
            .map(|(_, s)| s)
            .collect();
        let rest: u64 = of.iter().map(|s| areas(art, &s.rest).0).sum();
        let spare: u64 = of.iter().map(|s| areas(art, &s.spare).0).sum();

        if rest + spare > 0 {
            println!(
                "  {:<10} {:>9.1} {:>9.1} {:>6.1}%",
                kind.name(),
                mega(rest),
                mega(spare),
                share(spare, rest + spare)
            );
        }
    }

    for (path, held) in runs {
        let full: BTreeMap<Key, Shape> = held
            .keys
            .iter()
            .map(|(key, _)| (*key, art.shape(*key, None)))
            .collect();
        let touched: BTreeSet<FourCc> = art
            .groups(full.iter(), Grain::Unit)
            .keys()
            .map(|k| k.0)
            .collect();
        let spare: BTreeSet<Tile> = touched
            .iter()
            .filter_map(|i| found.get(i))
            .flat_map(|s| s.spare.iter().copied())
            .collect();
        let made: BTreeSet<Tile> = spare.intersection(&held.tiles).copied().collect();
        let back: BTreeSet<Tile> = spare.difference(&made).copied().collect();

        println!(
            "  {path}: {} records; their damage and rubble {:.1} MB, of which made {:.1} MB, held back {:.1} MB",
            touched.len(),
            mega(areas(art, &spare).0),
            mega(areas(art, &made).0),
            mega(areas(art, &back).0)
        );
    }

    println!("== one page a record (gutter {gutter}, whole tiles): sides past 3072 and 4096");

    for ((image, ..), tiles) in art.groups(art.keys.iter(), Grain::Unit) {
        let page = texture(art, &tiles, (Grain::Unit, Cut::Whole, gutter));
        let side = page.width.max(page.height);
        let rest = found.get(&image).map_or(&tiles, |s| &s.rest);
        let without = texture(art, rest, (Grain::Unit, Cut::Whole, gutter));

        if side > SIDE_NEAR {
            println!(
                "  {image} {:<10} {}x{}{}; without damage and rubble {}x{}",
                image_kind(art, image).name(),
                page.width,
                page.height,
                if side > LIMITS[0] { " PAST 4096" } else { "" },
                without.width,
                without.height
            );
        }
    }

    upgrades(art);
}

/// Each building with an `UpgradeFrom`: whether its art is its own
/// `IMAG` record.
fn upgrades(art: &Art<'_>) {
    let mut own = 0;
    let mut shared = Vec::new();

    for (code, unit) in &art.units {
        let Some(from) = unit.desc.building().and_then(|b| b.upgrade_from()) else {
            continue;
        };

        if art.images.get(code) == art.images.get(&from) {
            shared.push(format!("{from}->{code}"));
        } else {
            own += 1;
        }
    }

    println!(
        "== upgrades: {own} buildings with an UpgradeFrom draw their own IMAG record; {} share their base's: {}",
        shared.len(),
        shared.join(" ")
    );
}

/// The atlas measurement: inventory, trimming, packing, shadows, frame
/// table and timing over `MAJESTY_HD`'s expansion dataset; then each
/// `held` run's dump.
///
/// # Errors
///
/// If the install or a dump will not read.
pub fn cmd_atlas(gutter: u32, held: &[String]) -> Result<bool, String> {
    let hd = env::path(env::HD).map_err(|e| e.to_string())?;
    let files = Install::new(&hd, Bytecode::Install)
        .files(Datasets::Expansion)
        .map_err(|e| e.to_string())?
        .archives;
    let library = Library::from_files(&files).map_err(|e| e.to_string())?;
    let catalogue = library.catalogue().map_err(|e| e.to_string())?;
    let mut art = Art::read(&catalogue);
    let runs = held
        .iter()
        .map(|path| Ok((path, Held::read(path, &catalogue)?)))
        .collect::<Result<Vec<_>, String>>()?;
    let inventory_tiles: BTreeSet<Tile> = art.tiles.keys().copied().collect();

    for (_, run) in &runs {
        art.cover(run);
    }

    // The inventory's figures over its own tiles alone.
    let extra = art
        .tiles
        .keys()
        .filter(|t| !inventory_tiles.contains(t))
        .count();

    if extra > 0 {
        println!("({extra} tiles the runs made lie outside every world key)");
    }

    inventory(&art);
    sizes(&art);
    trimming(&art);
    packing(&art, gutter);
    shadows(&art);
    table(&art);
    timing(&art, gutter);

    for (path, run) in &runs {
        held_report(&art, path, run, gutter);
    }

    sheets(&art, &runs, gutter);

    Ok(true)
}
