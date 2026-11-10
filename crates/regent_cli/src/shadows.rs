//! Where a unit's art paints its shadow: one PNG per drawn tile, the
//! tile over plain ground beside a map of what each pixel is.
//!
//! ```text
//!  <out>/<ID>_<set>_l<layer>_t<tile>.png
//!  left   as drawn over grass, shadows by the tile's draw mode
//!  right  white hole, grey colour, blue dark colour (brightest channel
//!         under DARK), red a darkening index (the deeper, the redder),
//!         yellow a brightening index, green 247-254 drawn as colours,
//!         magenta a darkening index aloft (`Sprite::aloft`): a part's,
//!         kept where a model casts the ground shadow
//! ```

use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

use regent_core::content::imagery::{Animation, FrameSource};
use regent_core::content::unit::Unit;
use regent_core::data::imag::SetKind;
use regent_core::data::payload::Rgb;
use regent_core::sim::footprint::{self, ArtRect};
use regent_core::view::raster::shade::{CONTROL_FIRST, TRANSPARENT};
use regent_core::view::{Colour, Ink, Shade, Sprite};

use super::util;

/// A colour pixel whose brightest channel is under this shows blue: a
/// painted dark, not a shadow index.
const DARK: u8 = 48;

/// The ground the left panel draws over.
const GRASS: Rgb = Rgb {
    r: 96,
    g: 128,
    b: 64,
};

/// Output pixels per art pixel.
const SCALE: usize = 2;

/// What one pixel is, by the library's reading of its tile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Class {
    Hole,
    Colour,
    Dark,
    Darken,
    Brighten,
    /// 247 to 254 in a tile whose mode draws them as colours.
    Painted,
    /// Darkening, but a part's: [`Sprite::aloft`].
    Aloft,
}

/// Where a layer stands, for [`Sprite::aloft`]: footprint, and its pose.
type Reach = (Option<ArtRect>, (i32, i32));

/// Write `record`'s `set` tiles to `out`, and a count per tile.
pub fn cmd_shadows(record: &str, set: &str, out: &Path, paths: &[String]) -> Result<bool, String> {
    let library = util::library(paths)?;
    let catalogue = library.catalogue().map_err(|e| e.to_string())?;
    let id = util::fourcc(record)?;
    let unit = Unit::find(&catalogue, id)
        .ok_or_else(|| format!("no DUNT record {record}"))?
        .map_err(|e| format!("{record}: {e}"))?;
    let kind = unit
        .animations
        .iter()
        .map(|a| a.kind)
        .find(|k| format!("{k:?}").eq_ignore_ascii_case(set))
        .ok_or_else(|| format!("{record} has no {set} set"))?;

    std::fs::create_dir_all(out).map_err(|e| e.to_string())?;
    println!(
        "  layer tile  mode      size     hole colour dark darken bright painted aloft  by index 247..254"
    );

    let stands = footprint::of_unit(&catalogue, &unit).map_err(|e| e.to_string())?;

    for layer in unit.layers(kind) {
        let pose = layer
            .poses
            .first()
            .map_or((0, 0), |p| (i32::from(p.x), i32::from(p.y)));
        let reach = (stands, pose);

        for (archive, index) in tiles(layer) {
            let Some(Ok(sprite)) = Sprite::at(&catalogue, archive, index) else {
                println!("  tile {index}: does not decode");
                continue;
            };
            let name = format!(
                "{record}_{}_l{}_t{index}.png",
                kind_name(kind),
                layer.variant
            );

            report(layer.variant, index, &sprite, reach);
            write(&out.join(name), &sprite, reach)?;
        }
    }

    Ok(true)
}

fn kind_name(kind: SetKind) -> String {
    format!("{kind:?}").to_lowercase()
}

/// Every stream's first frame of the layer's first pose.
fn tiles(layer: &Animation) -> Vec<(usize, usize)> {
    layer
        .poses
        .first()
        .map(|pose| {
            (0..pose.streams)
                .filter_map(|s| pose.stream(s).first().and_then(FrameSource::tile))
                .collect()
        })
        .unwrap_or_default()
}

/// The raw index at `i`, if indexed.
fn index_at(sprite: &Sprite, i: usize) -> Option<u8> {
    match &sprite.colour {
        Colour::Indexed { indices, .. } => Some(indices[i]),
        Colour::Direct { .. } => None,
    }
}

fn classify(sprite: &Sprite, i: usize, (stands, pose): Reach) -> Class {
    let control = index_at(sprite, i).is_some_and(|x| (CONTROL_FIRST..TRANSPARENT).contains(&x));

    match sprite.as_drawn(i) {
        Ink::Hole => Class::Hole,
        Ink::Shade(Shade::Keep(_)) if stands.is_some_and(|s| sprite.aloft(i, s, pose)) => {
            Class::Aloft
        }
        Ink::Shade(Shade::Keep(_)) => Class::Darken,
        Ink::Shade(Shade::Raise(_)) => Class::Brighten,
        Ink::Colour(_) if control && sprite.covered[i] => Class::Painted,
        Ink::Colour(c) if c.r.max(c.g).max(c.b) < DARK => Class::Dark,
        Ink::Colour(_) => Class::Colour,
    }
}

fn report(layer: u8, tile: usize, sprite: &Sprite, reach: Reach) {
    let n = sprite.covered.len();
    let mut counts = [0usize; 7];
    let mut by_index = [0usize; (TRANSPARENT - CONTROL_FIRST) as usize];

    for i in 0..n {
        counts[classify(sprite, i, reach) as usize] += 1;

        if let Some(x) = index_at(sprite, i).filter(|x| (CONTROL_FIRST..TRANSPARENT).contains(x)) {
            by_index[usize::from(x - CONTROL_FIRST)] += 1;
        }
    }

    let [hole, colour, dark, darken, bright, painted, aloft] = counts;
    let size = format!("{}x{}", sprite.width, sprite.height);

    println!(
        "  {layer:>5} {tile:>5}  {:<9} {size:<8} {hole:>6} {colour:>6} {dark:>4} {darken:>6} {bright:>6} {painted:>7} {aloft:>5}  {by_index:?}",
        format!("{:?}", sprite.mode)
    );
}

/// One output pixel of each panel.
fn pixels(sprite: &Sprite, i: usize, reach: Reach) -> ([u8; 3], [u8; 3]) {
    let grey = |c: Rgb| {
        let y = (u32::from(c.r) * 3 + u32::from(c.g) * 6 + u32::from(c.b)) / 10;
        let y = u8::try_from(y).unwrap_or(u8::MAX);
        [y, y, y]
    };
    let drawn = match sprite.as_drawn(i) {
        Ink::Hole => GRASS,
        Ink::Colour(c) => c,
        Ink::Shade(s) => s.over(GRASS),
    };
    // The more a shadow keeps, the paler its red: 50 % full red.
    let pale = |s: Shade| match s {
        Shade::Keep(p) => u8::try_from(p.saturating_sub(50) * 4).unwrap_or(u8::MAX),
        Shade::Raise(_) => 0,
    };
    let class = match (classify(sprite, i, reach), sprite.as_drawn(i)) {
        (Class::Hole, _) => [255, 255, 255],
        (Class::Darken, Ink::Shade(s)) => [255, pale(s), pale(s)],
        (Class::Brighten, _) => [255, 230, 0],
        (Class::Painted, _) => [0, 200, 0],
        (Class::Aloft, _) => [255, 0, 255],
        (Class::Dark, _) => [0, 60, 255],
        (_, Ink::Colour(c)) => grey(c),
        _ => [255, 255, 255],
    };

    ([drawn.r, drawn.g, drawn.b], class)
}

fn write(path: &Path, sprite: &Sprite, reach: Reach) -> Result<(), String> {
    let (w, h) = (usize::from(sprite.width), usize::from(sprite.height));
    let width = w * 2 * SCALE;
    let mut rgb = vec![0u8; width * h * SCALE * 3];

    for y in 0..h {
        for x in 0..w {
            let (left, right) = pixels(sprite, y * w + x, reach);

            for (dy, dx) in (0..SCALE).flat_map(|a| (0..SCALE).map(move |b| (a, b))) {
                let row = (y * SCALE + dy) * width;
                let l = (row + x * SCALE + dx) * 3;
                let r = l + w * SCALE * 3;

                rgb[l..l + 3].copy_from_slice(&left);
                rgb[r..r + 3].copy_from_slice(&right);
            }
        }
    }

    let fail = |e: &dyn std::fmt::Display| format!("{}: {e}", path.display());
    let file = File::create(path).map_err(|e| fail(&e))?;
    let mut encoder = png::Encoder::new(
        BufWriter::new(file),
        u32::try_from(width).map_err(|e| fail(&e))?,
        u32::try_from(h * SCALE).map_err(|e| fail(&e))?,
    );

    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .and_then(|mut w| w.write_image_data(&rgb))
        .map_err(|e| fail(&e))
}
