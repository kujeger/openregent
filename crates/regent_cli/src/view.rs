//! Unit commands over a multi-archive catalogue ([`regent_core::view`]).
//!
//! Stats are in `unittype.cam`, art in `maindata.cam`, sounds in
//! `sounddesc.cam`, so names resolve across every archive given.

use std::collections::{BTreeMap, BTreeSet};

use regent_core::content::imagery::FrameSource;
use regent_core::content::unit::Unit;
use regent_core::data::catalogue::Catalogue;
use regent_core::data::desc::UnitKind;
use regent_core::data::fourcc::FourCc;
use regent_core::data::payload;
use regent_core::view::Sprite;

use super::descriptor::{desc_tail, desc_unit, head_names};
use super::util;

/// The tags whose `HEAD` names a unit's links point at.
const NAMED_TAGS: [[u8; 4]; 3] = [*b"DMOV", *b"DSND", *b"DACT"];

/// What the catalogue was built from, and how many ids collide.
/// `Catalogue::find` takes the last loaded on collision.
fn header(catalogue: &Catalogue<'_>, empty: usize) {
    let names: Vec<&str> = catalogue.archives.iter().map(|(n, _)| *n).collect();
    let ambiguous = catalogue.ambiguous().count();

    println!(
        "== {} archives, {empty} empty, {ambiguous} ambiguous ids, {} records",
        names.len(),
        catalogue.records()
    );
    println!("   {}", names.join(" "));
}

/// Every `HEAD` name in the catalogue.
fn names(catalogue: &Catalogue<'_>) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();

    for (_, archive) in &catalogue.archives {
        for tag in NAMED_TAGS {
            out.extend(head_names(archive, tag));
        }
    }

    out
}

fn kind_name(kind: Option<UnitKind>) -> String {
    kind.map_or_else(|| "?".to_owned(), |k| format!("{k:?}"))
}

/// One line per unit: what it is, and whether its art is reachable.
pub fn cmd_units(filter: Option<UnitKind>, paths: &[String]) -> Result<bool, String> {
    let library = util::library(paths)?;
    let catalogue = library.catalogue().map_err(|e| e.to_string())?;

    header(&catalogue, library.empty.len());

    let (mut shown, mut with_art, mut failed) = (0, 0, 0);

    for (at, record) in catalogue.tagged(b"DUNT") {
        // `Unit::at` propagates a bad PRIM tail, so report it and keep going.
        let unit = match Unit::at(&catalogue, at) {
            Ok(u) => u,
            Err(e) => {
                eprintln!("  {}: {e}", record.id);
                failed += 1;
                continue;
            }
        };

        if filter.is_some_and(|want| unit.kind != Some(want)) {
            continue;
        }

        shown += 1;

        let art = match unit.art.and_then(|l| l.at) {
            Some(found) => {
                with_art += 1;
                catalogue.archives[found.archive].0.to_string()
            }
            None => "-- no art --".to_owned(),
        };

        let frames = unit.frames().count();

        println!(
            "  {} {:<14} {:<24} {:>2} sets {:>5} frames  {art}",
            unit.id,
            kind_name(unit.kind),
            String::from_utf8_lossy(unit.desc.head.name),
            unit.animations.len(),
            frames,
        );
    }

    println!("   {shown} units, {with_art} with art, {failed} unreadable");

    Ok(failed == 0)
}

/// Everything one unit names, followed.
pub fn cmd_unit(wanted: &str, paths: &[String]) -> Result<bool, String> {
    let library = util::library(paths)?;
    let catalogue = library.catalogue().map_err(|e| e.to_string())?;

    header(&catalogue, library.empty.len());

    let id = util::fourcc(wanted)?;
    let Some(found) = Unit::find(&catalogue, id) else {
        return Err(format!("no DUNT record {wanted} in these archives"));
    };
    let unit = found.map_err(|e| format!("{wanted}: {e}"))?;

    println!(
        "  {} {}",
        unit.id,
        String::from_utf8_lossy(unit.desc.head.name)
    );

    let description = String::from_utf8_lossy(unit.desc.head.description);
    if !description.is_empty() {
        println!("      {:<18} {description}", "description");
    }

    if let Some(topic) = help(&catalogue, &unit) {
        println!("      {:<18} {topic}", "help");
    }

    println!(
        "      {:<18} {}",
        "archive", catalogue.archives[unit.archive].0
    );

    // The stat block, byte for byte as `regent_cli desc --record` prints it.
    desc_unit(&unit.desc);
    desc_tail(&unit.desc, &names(&catalogue));

    art(&catalogue, &unit);

    Ok(true)
}

/// `regent_cli selection`: the panel's pictures, name and the ring, from
/// [`regent_core::view::panel::portrait`] and [`regent_core::view::map::ring`].
pub fn cmd_selection(wanted: Option<&str>, paths: &[String]) -> Result<bool, String> {
    use regent_core::view::map::ring::{self, Look};
    use regent_core::view::panel::portrait::{self, Crown};

    let library = util::library(paths)?;
    let catalogue = library.catalogue().map_err(|e| e.to_string())?;

    header(&catalogue, library.empty.len());

    for crown in [Crown::Gold, Crown::Expansion] {
        let (record, set) = crown.art();
        let shown = picture(portrait::empty(&catalogue, crown));

        println!("  empty {crown:?}: {record} set {set}  {shown}");
    }

    let Some(wanted) = wanted else {
        return Ok(true);
    };

    let id = util::fourcc(wanted)?;
    let unit = Unit::find(&catalogue, id)
        .ok_or_else(|| format!("no DUNT record {wanted} in these archives"))?
        .map_err(|e| format!("{wanted}: {e}"))?;
    let art = unit
        .art
        .map_or_else(|| "-".to_owned(), |l| l.id.to_string());
    let title = portrait::title(
        &catalogue,
        &regent_core::data::text::Text::default(),
        &unit.desc,
        None,
    )
    .map_err(|e| e.to_string())?;

    println!("  {} {title}", unit.id);
    println!(
        "      {:<10} {art} set 1000  {}",
        "portrait",
        picture(portrait::portrait(&catalogue, &unit))
    );
    println!(
        "      {:<10} {art} set 1001  {}",
        "backdrop",
        picture(portrait::backdrop(&catalogue, &unit))
    );

    let rect = ring::rect(&catalogue, &unit).map_err(|e| e.to_string())?;
    let found = ring::ring(&catalogue, &unit, Look::Selected).map_err(|e| e.to_string())?;

    println!("      {:<10} {rect:?}", "rect");

    // An empty rect is filled from the piece as drawn, which a
    // descriptor alone does not give.
    match (rect, found) {
        (None, _) => println!("      {:<10} fits the box it is drawn in", "ring"),
        (Some(_), Some(r)) => println!(
            "      {:<10} {} slot {} at {:?}  {}",
            "ring",
            r.look.image(),
            r.slot,
            r.at,
            picture(r.sprite(&catalogue))
        ),
        (Some(_), None) => println!("      {:<10} none", "ring"),
    }

    Ok(true)
}

/// A picture's size, tile and palette, or why there is none.
fn picture(found: Option<regent_core::data::error::Result<Sprite>>) -> String {
    match found {
        Some(Ok(s)) => format!("{}x{}, palette {:?}", s.width, s.height, s.source),
        Some(Err(e)) => format!("FAILED {e}"),
        None => "none".to_owned(),
    }
}

/// The help topic a unit's `HelpID` names.
///
/// `HPTX` is keyed by the `HelpID`. Only the first line is printed.
/// `None` without a `HelpID` or without `gpltext.cam`.
fn help(catalogue: &Catalogue<'_>, unit: &Unit<'_>) -> Option<String> {
    let id = unit
        .desc
        .character()
        .and_then(|c| c.help_id())
        .or_else(|| unit.desc.building().and_then(|b| b.help_id()))?;

    let at = catalogue.find(b"STRT", FourCc::new(b"HPTX"))?;
    let record = catalogue.record(b"STRT", at)?;
    let topics = payload::StringTable::parse(record.data).ok()?;

    let text = topics.get_by_key(id)?;
    let first = text.lines().next()?.trim();

    Some(format!("{id}  {first}"))
}

/// The animation sets, and whether their frames are really pixels.
fn art(catalogue: &Catalogue<'_>, unit: &Unit<'_>) {
    let Some(link) = unit.art else {
        return;
    };

    let Some(at) = link.at else {
        println!(
            "      {:<18} {} -- not in these archives --",
            "art", link.id
        );
        return;
    };

    println!(
        "      {:<18} {} in {}",
        "art", link.id, catalogue.archives[at.archive].0
    );

    for set in &unit.animations {
        let frames: usize = set.poses.iter().map(|p| p.frames.len()).sum();
        let streams = set.poses.first().map_or(0, |p| p.streams);
        let per = set.poses.first().map_or(0, |p| p.per_stream);

        // Pose offset range: places a stacked layer on its base
        // (`docs/formats/art.md`).
        let xs = set.poses.iter().map(|p| p.x);
        let ys = set.poses.iter().map(|p| p.y);
        let offset = match (xs.clone().min(), xs.max(), ys.clone().min(), ys.max()) {
            (Some(x0), Some(x1), Some(y0), Some(y1)) if (x0, x1, y0, y1) == (0, 0, 0, 0) => {
                String::new()
            }
            (Some(x0), Some(x1), Some(y0), Some(y1)) => {
                format!("  offset x {x0}..{x1} y {y0}..{y1}")
            }
            _ => String::new(),
        };

        // Slot 0's footprint (the one the engine uses), pixels about the
        // anchor.
        let stands = match set.poses.first().map(|p| p.footprint) {
            Some([0, 0, 0, 0]) | None => String::new(),
            Some([left, top, right, bottom]) => {
                format!("  stands {left},{top}..{right},{bottom}")
            }
        };

        // Raw id: sets of one SetKind can differ only here.
        println!(
            "        {:<26} {:>3} poses  {per:>3} frames x {streams} streams  {frames:>5} total{offset}{stands}",
            format!(
                "{} {:#06x}",
                set.kind,
                regent_core::data::imag::set_number(set.id)
            ),
            set.poses.len(),
        );
    }

    // Rasterise every tile, to surface decode failures.
    let (mut tiles, mut ok, mut blank, mut pads) = (BTreeSet::new(), 0, 0, 0);
    let mut failed: Vec<String> = Vec::new();
    let mut modes: BTreeMap<String, usize> = BTreeMap::new();

    for frame in unit.frames() {
        let FrameSource::Tile { archive, index, .. } = frame else {
            continue;
        };

        if !tiles.insert((archive, index)) {
            continue;
        }

        match Sprite::at(catalogue, archive, index) {
            Some(Ok(s)) => {
                *modes
                    .entry(format!("{:?} level {}", s.mode, s.level))
                    .or_default() += 1;

                if s.drawn() == 0 {
                    blank += 1;
                } else {
                    ok += 1;
                }
            }
            Some(Err(e)) => failed.push(format!("tile {index}: {e}")),
            None => pads += 1,
        }
    }

    println!(
        "      {:<18} {} distinct, {ok} decode, {blank} draw nothing, {pads} padding slots",
        "tiles",
        tiles.len()
    );

    // The draw mode picks the blit (docs/formats/art.md).
    for (mode, n) in &modes {
        println!("      {:<18} {n} {mode}", "draw mode");
    }

    for e in &failed {
        println!("      {:<18} {e}", "FAILED");
    }
}
