//! The tileset and terrain classes, over [`regent_core::content::terrain`].
//!
//! Classes are in `terrtype.cam`, art in `tilesetdata.cam`, so this takes
//! several archives. Prints every count in `docs/formats/terrain.md`.

use std::collections::BTreeSet;

use regent_core::content::imagery::FrameSource;
use regent_core::content::terrain::{Terrain, Tileset};
use regent_core::data::catalogue::Catalogue;

/// The `MapTexture` letter as the cross reference spells it.
fn letter(n: u8) -> char {
    char::from(b'A' + n)
}

/// A frame's tile number, for the references that have one.
const fn tile_index(frame: &FrameSource) -> Option<usize> {
    match frame {
        FrameSource::Tile { index, .. } => Some(*index),
        _ => None,
    }
}

/// The terrain descriptors in the given archives, and their art.
///
/// # Errors
///
/// If the paths hold no readable archive, or a descriptor will not parse.
pub fn cmd_terrain(record: Option<&str>, paths: &[String]) -> Result<bool, String> {
    let library = super::util::library(paths)?;
    let catalogue = library.catalogue().map_err(|e| e.to_string())?;

    let found: Vec<_> = catalogue.tagged(b"DTRN").map(|(at, _)| at).collect();

    if found.is_empty() {
        return Err("no DTRN record in those archives".into());
    }

    for at in found {
        let terrain = Terrain::at(&catalogue, at).map_err(|e| e.to_string())?;

        println!(
            "== {} {}  {} classes of {} slots  art {}",
            terrain.id,
            String::from_utf8_lossy(terrain.desc.head.name),
            terrain.classes.len(),
            terrain.desc.capacity(),
            if terrain.art.resolved() {
                terrain.art.id.to_string()
            } else {
                format!("{} MISSING", terrain.art.id)
            }
        );

        classes(&terrain);

        if let Some(tileset) = &terrain.tileset {
            blends(tileset, record);
            totals(&catalogue, tileset);
        }
    }

    Ok(true)
}

/// One line per terrain class: its letter, its id, its name, and its art.
fn classes(terrain: &Terrain<'_>) {
    for (letter_index, link, class) in &terrain.classes {
        let name = class.map_or_else(
            || "?".to_owned(),
            |c| String::from_utf8_lossy(c.head.name).into_owned(),
        );

        let art = terrain
            .tileset
            .as_ref()
            .and_then(|t| t.body(*letter_index))
            .map_or_else(
                || "no body set".to_owned(),
                |b| {
                    format!(
                        "{} variants, {} groups at {:?}",
                        b.variants.len(),
                        b.groups.len(),
                        b.groups.iter().map(|(i, _)| *i).collect::<Vec<_>>()
                    )
                },
            );

        println!(
            "   {}  {}  {name:<16}  {art}",
            letter(*letter_index),
            link.id
        );
    }
}

/// One line per blend, and the full slot table when a record is named.
fn blends(tileset: &Tileset<'_>, record: Option<&str>) {
    for blend in &tileset.blends {
        let name = format!("{}to{}", letter(blend.from), letter(blend.to));
        let variants: Vec<usize> = blend.cases.iter().map(|c| c.variants.len()).collect();

        println!(
            "   MapTexture{name:<6} {:2} cases, variants {variants:?}",
            blend.cases.len()
        );

        // `record` names a blend (the tileset is one IMAG). Short or
        // `MapTexture` form.
        let wanted = record.is_some_and(|want| {
            want.eq_ignore_ascii_case(&name)
                || want.eq_ignore_ascii_case(&format!("MapTexture{name}"))
        });

        if wanted {
            for case in &blend.cases {
                let tiles: Vec<String> = case
                    .variants
                    .iter()
                    .map(|v| tile_index(v).map_or_else(|| format!("{v:?}"), |i| i.to_string()))
                    .collect();

                println!("      case {:2}  {}", case.slot, tiles.join(" "));
            }
        }
    }
}

/// What the tileset accounts for, against what the archive holds.
///
/// Blend art is four wide, so a three-variant case leaves an unreferenced tile.
fn totals(catalogue: &Catalogue<'_>, tileset: &Tileset<'_>) {
    let held = catalogue.chunk_len(tileset.archive, b"TILE");
    let used: BTreeSet<usize> = tileset.frames().filter_map(|f| tile_index(&f)).collect();
    let unused: Vec<usize> = (0..held).filter(|i| !used.contains(i)).collect();

    println!(
        "   {} references, {} distinct tiles of {held}, {} unreferenced",
        tileset.frames().count(),
        used.len(),
        unused.len()
    );

    if !unused.is_empty() {
        println!("   unreferenced: {unused:?}");
    }
}
