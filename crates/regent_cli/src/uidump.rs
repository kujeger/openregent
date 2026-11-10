//! Every interface picture of HD as a PNG, for trying upscalers on.
//!
//! ```text
//!  <out>/<archive>/<IMAG>_<set>_s<slot>_f<frame>.png   every IMAG frame
//!  <out>/tiles/<archive>_<n>.png                       TILEs no IMAG names
//!  <out>/portraits/<archive>/…                         maindata's interface sets
//!  <out>/index.tsv                                     one row per file
//! ```
//!
//! Names by `regent_core::view::picture_file`, which openregent's override
//! folder reads: an edited file dropped there replaces its picture.
//!
//! Colours as the screen shows them: each channel cut to 565 and shifted
//! back, low bits zero (`raster::colour::quantise`, `lit::present`). Shadow and glow
//! indices are translucent black and white, and draw modes 5 and 8 take
//! their per-index alpha, as openregent's menus draw them.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};

use regent_core::content::imagery;
use regent_core::data::catalogue::{Catalogue, Library};
use regent_core::data::fourcc::FourCc;
use regent_core::data::imag::Blit;
use regent_core::data::imag::{INTERFACE_ICON, INTERFACE_PANEL, INTERFACE_PORTRAIT, SetKind};
use regent_core::data::smnu::Panel;
use regent_core::view::menu::quest_map;
use regent_core::view::menu::{self, Release, World};
use regent_core::view::panel::portrait;
use regent_core::view::picture_file::{Key, PORTRAITS, stem};
use regent_core::view::raster::blend;
use regent_core::view::raster::colour::quantise;
use regent_core::view::raster::shade::LEVELS;
use regent_core::view::{Colour, Ink, Shade, Sprite};

use super::env;

/// What an archive gives the dump.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Role {
    /// Every set, and the tiles none names.
    Interface,
    /// Only its interface sets, under `portraits/`.
    Portraits,
    /// Panel layouts: where a picture is used.
    Panels,
}

/// HD's files, from the install root.
const SOURCES: [(&str, Role); 11] = [
    ("Data/interfacedata.cam", Role::Interface),
    ("Data/addinterface.cam", Role::Interface),
    ("Data/addinterface_se.cam", Role::Interface),
    ("DataMX/mx_interfacedata.cam", Role::Interface),
    ("DataMX/XQD1_intro.cam", Role::Interface),
    ("Data/maindata.cam", Role::Portraits),
    ("DataMX/mx_maindata.cam", Role::Portraits),
    ("Data/textdata.cam", Role::Panels),
    ("DataMX/mx_textdata.cam", Role::Panels),
    ("Data/UIData_1920_1200.dat", Role::Panels),
    ("Data/GPLDebuggerUI.cam", Role::Panels),
];

const INDEX: &str = "index.tsv";

/// Uses listed per picture before the rest are counted.
const USES_SHOWN: usize = 8;

/// One written file.
struct Row {
    path: String,
    archive: String,
    record: String,
    name: String,
    set: String,
    slot: String,
    frame: String,
    tile: usize,
    width: u16,
    height: u16,
    mode: u16,
    used: String,
}

/// Files and bytes per output folder.
#[derive(Default)]
struct Tally {
    files: usize,
    bytes: u64,
}

/// Dump every interface picture of `MAJESTY_HD` into `out`.
///
/// # Errors
///
/// When the install will not read or a file will not write.
pub fn cmd_ui_dump(out: &Path) -> Result<bool, String> {
    let hd = env::path(env::HD).map_err(|e| e.to_string())?;
    let paths: Vec<PathBuf> = SOURCES.iter().map(|(p, _)| hd.join(p)).collect();
    let library = Library::from_files(&paths).map_err(|e| e.to_string())?;
    let catalogue = library.catalogue().map_err(|e| e.to_string())?;
    let names: Vec<&str> = catalogue.archives.iter().map(|(name, _)| *name).collect();
    let roles = roles(&names);
    let places = uses(&catalogue);

    let mut rows = Vec::new();
    let mut failed = Vec::new();
    let mut tiled: BTreeSet<(usize, usize)> = BTreeSet::new();
    let mut taken: BTreeSet<String> = BTreeSet::new();
    let mut untiled = 0;

    // Every IMAG frame.
    for (at, record) in catalogue.tagged(b"IMAG") {
        let role = roles[at.archive];
        let archive = stem(catalogue.archives[at.archive].0);
        let id = record.id;

        if role == Role::Panels || record.data.is_empty() {
            continue;
        }

        let sets = match imagery::sets(&catalogue, at) {
            Ok(sets) => sets,
            Err(e) => {
                failed.push(format!("{archive} IMAG {id}: {e}"));
                continue;
            }
        };

        let file = catalogue.archives[at.archive].0;
        let folder = match role {
            Role::Portraits => format!("{PORTRAITS}/"),
            _ => String::new(),
        };

        for set in &sets {
            let wanted = role == Role::Interface || set.kind == SetKind::Interface;

            for (s, pose) in set.poses.iter().enumerate().filter(|_| wanted) {
                for (f, frame) in pose.frames.iter().enumerate() {
                    // Hidden, text or a missing tile: nothing to draw.
                    let Some((a, index)) = frame.tile() else {
                        untiled += 1;
                        continue;
                    };

                    tiled.insert((a, index));

                    let base = format!("{folder}{}", Key::frame(file, id, set.id, s, f).name());
                    let path = unique(&mut taken, base, at.record);
                    let used = match role {
                        Role::Portraits => portrait_use(set.id),
                        _ => places.get(&(id, i64::from(set.id))).map(list),
                    };
                    let row = Row {
                        path,
                        archive: archive.clone(),
                        record: id.trimmed().to_owned(),
                        name: record.name().into_owned(),
                        set: set.id.to_string(),
                        slot: s.to_string(),
                        frame: f.to_string(),
                        tile: index,
                        width: 0,
                        height: 0,
                        mode: 0,
                        used: used.unwrap_or_default(),
                    };

                    match draw(&catalogue, (a, index), set.blit(), out, row) {
                        Ok(row) => rows.push(row),
                        Err(e) => failed.push(e),
                    }
                }
            }
        }
    }

    // Tiles no IMAG names, in the interface archives.
    for (archive, (file, cam)) in catalogue.archives.iter().enumerate() {
        let count = cam.chunk_by_tag(b"TILE").map_or(0, |c| c.records.len());

        if roles[archive] != Role::Interface {
            continue;
        }

        for index in (0..count).filter(|i| !tiled.contains(&(archive, *i))) {
            let row = Row {
                path: Key::tile(file, index).name(),
                archive: stem(file),
                record: String::new(),
                name: String::new(),
                set: String::new(),
                slot: String::new(),
                frame: String::new(),
                tile: index,
                width: 0,
                height: 0,
                mode: 0,
                used: String::new(),
            };

            match draw(&catalogue, (archive, index), Blit::Interface, out, row) {
                Ok(row) => rows.push(row),
                Err(e) if e.ends_with(EMPTY) => {}
                Err(e) => failed.push(e),
            }
        }
    }

    write_index(out, &rows)?;
    report(out, &rows, &failed);
    println!("frames naming no tile: {untiled}");

    Ok(failed.is_empty())
}

/// The error a padding slot gives: skipped, not a failure.
const EMPTY: &str = "empty slot";

/// Write tile `(archive, index)` as `row.path`, filling its size.
fn draw(
    catalogue: &Catalogue<'_>,
    (archive, index): (usize, usize),
    blit: Blit,
    out: &Path,
    mut row: Row,
) -> Result<Row, String> {
    let sprite = match Sprite::at(catalogue, archive, index) {
        Some(Ok(sprite)) => sprite,
        Some(Err(e)) => return Err(format!("{}: tile {index}: {e}", row.path)),
        None => return Err(format!("{}: tile {index}: {EMPTY}", row.path)),
    };

    row.path.push_str(".png");
    row.width = sprite.width;
    row.height = sprite.height;
    row.mode = sprite.mode.word();

    save(&out.join(&row.path), &sprite, blit)?;

    Ok(row)
}

/// A sprite as an RGBA PNG.
fn save(path: &Path, sprite: &Sprite, blit: Blit) -> Result<(), String> {
    let fail = |e: &dyn std::fmt::Display| format!("{}: {e}", path.display());

    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| fail(&e))?;
    }

    let file = File::create(path).map_err(|e| fail(&e))?;
    let mut encoder = png::Encoder::new(
        BufWriter::new(file),
        u32::from(sprite.width),
        u32::from(sprite.height),
    );

    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);

    encoder
        .write_header()
        .and_then(|mut w| w.write_image_data(&rgba(sprite, blit)))
        .map_err(|e| fail(&e))
}

/// Straight RGBA as the menus draw it, colours on the 565 screen.
fn rgba(sprite: &Sprite, blit: Blit) -> Vec<u8> {
    let alphas = match &sprite.colour {
        Colour::Indexed { indices, palette } => {
            blend::alphas(sprite.mode, sprite.level, palette).map(|a| (indices, a))
        }
        Colour::Direct { .. } => None,
    };

    (0..sprite.covered.len())
        .flat_map(|i| {
            // The interface goes by the tile's draw mode, as openregent.
            let drawn = match blit {
                Blit::Interface => sprite.as_drawn(i),
                Blit::World => sprite.ink(i, blit),
            };

            match (&alphas, drawn) {
                (None, _) => ink(drawn),
                (Some((indices, a)), Ink::Colour(rgb)) => {
                    let [r, g, b] = quantise(rgb).channels();

                    [r, g, b, a[usize::from(indices[i])]]
                }
                (Some(_), _) => [0; 4],
            }
        })
        .collect()
}

/// One pixel: a shadow black at what it darkens by, a glow white at
/// what it lightens by (openregent's `menu::art::ink`).
fn ink(ink: Ink) -> [u8; 4] {
    let alpha = |share: u32| u8::try_from(share.min(u32::from(u8::MAX))).unwrap_or(u8::MAX);

    match ink {
        Ink::Hole => [0; 4],
        Ink::Colour(rgb) => {
            let [r, g, b] = quantise(rgb).channels();

            [r, g, b, u8::MAX]
        }
        Ink::Shade(Shade::Keep(percent)) => [0, 0, 0, alpha(255 * (100 - percent.min(100)) / 100)],
        Ink::Shade(Shade::Raise(steps)) => [255, 255, 255, alpha(255 * steps / LEVELS)],
    }
}

/// Where each picture is used: every panel control naming it, the quest
/// map's and briefing's pictures, the empty selection panel and the
/// health bar. Keyed by record and set.
fn uses(catalogue: &Catalogue<'_>) -> BTreeMap<(FourCc, i64), BTreeSet<String>> {
    let mut out: BTreeMap<(FourCc, i64), BTreeSet<String>> = BTreeMap::new();
    let mut add = |record: FourCc, set: i64, what: String| {
        out.entry((record, set)).or_default().insert(what);
    };

    // `SMNU` controls: `AP29#8003`, the panel's own block without an id.
    for (_, record) in catalogue.tagged(b"SMNU") {
        let Ok(panel) = Panel::parse(record.data) else {
            continue;
        };

        for control in panel.controls() {
            if let Some((art, set)) = control.art() {
                let id = control.id().map_or_else(String::new, |c| format!("#{c}"));

                add(art, i64::from(set), format!("{}{id}", record.id.trimmed()));
            }
        }
    }

    let bg = quest_map::BACKGROUND;
    add(bg.record, i64::from(bg.set), "front-end background".into());

    for (slot, code) in quest_map::SLOTS.iter().enumerate() {
        if let Some(pog) = quest_map::pog(slot) {
            add(pog.record, i64::from(pog.set), format!("{code} pog"));
        }

        if let Some((record, set)) = menu::zoom(*code, Release::Base, World::Quest) {
            add(record, i64::from(set), format!("{code} briefing still"));
        }
    }

    for release in [Release::Base, Release::Expansion] {
        if let Some((record, set)) = menu::zoom(FourCc(*b"FRE0"), release, World::Freestyle) {
            add(record, i64::from(set), "freestyle briefing still".into());
        }
    }

    for crown in [portrait::Crown::Gold, portrait::Crown::Expansion] {
        let (record, set) = crown.art();
        add(record, i64::from(set), "empty selection panel".into());
    }

    for bar in [portrait::Bar::Plain, portrait::Bar::Poisoned] {
        let set = bar.set();
        add(portrait::BAR, i64::from(set), "selection health bar".into());
    }

    out
}

/// What a unit's interface set is for (`view::panel::portrait`).
fn portrait_use(set: u32) -> Option<String> {
    let what = match u16::try_from(set).ok()? {
        INTERFACE_PORTRAIT => "selection portrait",
        INTERFACE_PANEL => "hero panel backdrop",
        INTERFACE_ICON => "list icon",
        _ => return None,
    };

    Some(what.to_owned())
}

/// Uses, the first few by name.
fn list(uses: &BTreeSet<String>) -> String {
    let mut out: Vec<&str> = uses.iter().take(USES_SHOWN).map(String::as_str).collect();
    let more = uses.len().saturating_sub(USES_SHOWN);
    let tail = format!("+{more} more");

    if more > 0 {
        out.push(&tail);
    }

    out.join(", ")
}

/// `base`, or `base_r<record>` when another record took it.
fn unique(taken: &mut BTreeSet<String>, base: String, record: usize) -> String {
    let path = if taken.contains(&base) {
        format!("{base}_r{record}")
    } else {
        base
    };

    taken.insert(path.clone());
    path
}

fn write_index(out: &Path, rows: &[Row]) -> Result<(), String> {
    let mut text = String::from(
        "path\tarchive\trecord\tname\tset\tslot\tframe\ttile\twidth\theight\tmode\tused\n",
    );

    for r in rows {
        let _ = writeln!(
            text,
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            r.path,
            r.archive,
            r.record,
            r.name.replace('\t', " "),
            r.set,
            r.slot,
            r.frame,
            r.tile,
            r.width,
            r.height,
            r.mode,
            r.used
        );
    }

    let path = out.join(INDEX);
    std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))
}

/// Each loaded archive's role, by its file name.
///
/// Not by place: [`Library::from_files`] skips zero-byte files. A name not
/// in [`SOURCES`] gets [`Role::Panels`], which draws nothing.
fn roles(names: &[&str]) -> Vec<Role> {
    let role = |name: &str| {
        SOURCES
            .iter()
            .find(|(path, _)| Path::new(path).file_name().is_some_and(|f| f == name))
            .map_or(Role::Panels, |(_, role)| *role)
    };

    names.iter().map(|name| role(name)).collect()
}

/// Files and bytes per folder, then failures.
fn report(out: &Path, rows: &[Row], failed: &[String]) {
    let mut tally: BTreeMap<&str, Tally> = BTreeMap::new();

    for r in rows {
        let folder = r.path.rsplit_once('/').map_or("", |(d, _)| d);
        let bytes = std::fs::metadata(out.join(&r.path)).map_or(0, |m| m.len());
        let t = tally.entry(folder).or_default();

        t.files += 1;
        t.bytes += bytes;
    }

    for (folder, t) in &tally {
        println!("{folder:<32} {:>6} files {:>12} bytes", t.files, t.bytes);
    }

    let files: usize = tally.values().map(|t| t.files).sum();
    let bytes: u64 = tally.values().map(|t| t.bytes).sum();

    println!("{:<32} {files:>6} files {bytes:>12} bytes", "total");
    println!("index: {}", out.join(INDEX).display());

    for f in failed {
        eprintln!("failed: {f}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A zero-byte source is not loaded, so the rest must not shift roles.
    #[test]
    fn an_archives_role_follows_its_name_not_its_place() {
        let loaded = ["addinterface.cam", "maindata.cam", "textdata.cam"];

        assert_eq!(
            roles(&loaded),
            [Role::Interface, Role::Portraits, Role::Panels]
        );
    }
}
