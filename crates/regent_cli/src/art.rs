//! Sprites and animation metadata.
//!
//! `IMAG` maps animations, poses and facings to `TILE`s; `TILE` holds pixels.

use std::collections::BTreeMap;

use regent_core::data::{cam, imag, tile};

use crate::util::{inputs, name, read_bytes};

/// Running totals over one archive's animation records.
#[derive(Default)]
struct ImagTally {
    records: usize,
    sets: usize,
    slots: usize,
    frames: usize,
    resolved: usize,
    odd: usize,
}

/// A frame reference: index, index with flags, bracketed sentinel, or raw word.
fn frame_ref(target: imag::FrameRef) -> String {
    match target {
        imag::FrameRef::Tile { index, flags: 0 } => format!("{index}"),
        imag::FrameRef::Tile { index, flags } => format!("{index}/{flags:#06x}"),
        imag::FrameRef::Sentinel(v) => format!("[{v}]"),
        imag::FrameRef::Unknown(w) => format!("<{w:#010x}>"),
    }
}

/// Print one record's sets, and add it to the archive's totals.
fn imag_record(
    id: &str,
    label: &str,
    image: &imag::Image<'_>,
    tiles: usize,
    detail: bool,
    tally: &mut ImagTally,
) {
    tally.records += 1;
    tally.sets += image.sets.len();
    tally.slots += image.slot_count();
    tally.frames += image.frame_count();

    if detail {
        println!(
            "  {id} {label:<18} v{} {} sets",
            image.version.number(),
            image.sets.len()
        );
    }

    for set in &image.sets {
        let shape: Vec<String> = set
            .slots
            .iter()
            .map(|s| format!("{}x{}", s.frames, s.streams))
            .collect();

        if detail {
            // The set's playback word 2 is an InitAction's frame time.
            let playback = set.playback().map_or_else(
                || "none".to_owned(),
                |p| format!("kind {} flags {:#x} {} ms", p.kind, p.flags, p.value),
            );

            println!(
                "    {:<16} variant {}  id {:#x}  playback {playback}  {} slots  {}",
                set.kind().to_string(),
                set.variant(),
                set.id,
                set.slots.len(),
                shape.join(" ")
            );
        } else {
            println!(
                "  {id} {label:<18} {:<15} {:>3} slots  {}",
                set.kind().to_string(),
                set.slots.len(),
                shape.first().map_or("", String::as_str)
            );
        }

        for (k, slot) in set.slots.iter().enumerate() {
            for frame in slot.frames() {
                match frame.target() {
                    imag::FrameRef::Tile { index, .. } if usize::from(index) < tiles => {
                        tally.resolved += 1;
                    }
                    _ => tally.odd += 1,
                }
            }

            if detail {
                // A frame's own nudge follows its reference as `@x,y`.
                let refs: Vec<String> = slot
                    .frames()
                    .map(|f| match f.nudge() {
                        (0, 0) => frame_ref(f.target()),
                        (x, y) => format!("{}@{x},{y}", frame_ref(f.target())),
                    })
                    .collect();

                // A slot's own playback, which anim playback prefers.
                let own = slot.playback().map_or_else(String::new, |p| {
                    format!(
                        "  playback kind {} flags {:#x} {} ms",
                        p.kind, p.flags, p.value
                    )
                });

                println!(
                    "      slot {k:<3} at {:>4},{:<4} mask {:#06x}{own}  {}",
                    slot.x,
                    slot.y,
                    slot.attachments,
                    refs.join(" ")
                );

                // Mask blocks, two points each. The Ballista Tower varies them
                // across eight facings with identical art.
                for (bit, block) in slot.attachment_blocks() {
                    println!(
                        "        bit {bit:<2} {:>6},{:<6} {:>6},{:<6}",
                        block[0], block[1], block[2], block[3]
                    );
                }
            }
        }
    }
}

/// Decode the animation sets an archive carries.
///
/// One line per set; with `--record`, one line per slot with frame references.
/// The summary counts references resolving in this archive's `TILE` chunk.
pub fn cmd_imag(wanted: Option<&str>, paths: &[String]) -> Result<bool, String> {
    let mut failed = 0;

    for p in &inputs(paths, "cam")? {
        let data = read_bytes(p)?;
        let archive = cam::Archive::parse(&data).map_err(|e| format!("{p}: {e}"))?;
        let tiles = archive.chunk_by_tag(b"TILE").map_or(0, |c| c.records.len());
        let mut tally = ImagTally::default();

        println!("== {}", name(p));

        for (chunk, r) in archive.records() {
            if chunk.tag != b"IMAG" || r.data.is_empty() {
                continue;
            }

            let id = r.id.to_string();

            if wanted.as_ref().is_some_and(|w| *w != id) {
                continue;
            }

            match imag::Image::parse(r.data) {
                Ok(image) => {
                    let detail = wanted.is_some();

                    imag_record(&id, &r.name(), &image, tiles, detail, &mut tally);
                }
                Err(e) => {
                    eprintln!("  {id} {}: {e}", r.name());
                    failed += 1;
                }
            }
        }

        println!(
            "   {} records, {} sets, {} slots, {} frames; {} references resolve, {} do not",
            tally.records, tally.sets, tally.slots, tally.frames, tally.resolved, tally.odd
        );
    }

    if failed > 0 {
        eprintln!("{failed} records failed to decode");
    }

    Ok(failed == 0)
}

/// Decode every `TILE` in an archive and summarise what came out.
pub fn cmd_tiles(paths: &[String]) -> Result<bool, String> {
    let mut failed = 0;

    for p in &inputs(paths, "cam")? {
        let data = read_bytes(p)?;
        let archive = cam::Archive::parse(&data).map_err(|e| format!("{p}: {e}"))?;
        let mut versions: BTreeMap<u16, usize> = BTreeMap::new();
        let (mut indexed, mut wide, mut embedded, mut empty) = (0, 0, 0, 0);

        println!("== {}", name(p));

        for (chunk, r) in archive.records() {
            if chunk.tag != b"TILE" {
                continue;
            }

            // Mod archives pad unused slots: references are positional.
            if r.data.is_empty() {
                empty += 1;
                continue;
            }

            let version = tile::Tile::version_word(r.data).map_err(|e| format!("{p}: {e}"))?;
            *versions.entry(version).or_default() += 1;

            match tile::Tile::parse(r.data) {
                Ok(t) => {
                    // Uncompressed: no runs.
                    let shape = match t.runs() {
                        Some(rows) => {
                            format!("{:>5} runs", rows.iter().map(Vec::len).sum::<usize>())
                        }
                        None => "      raw".to_string(),
                    };

                    match t.format() {
                        tile::PixelFormat::Indexed => indexed += 1,
                        tile::PixelFormat::Direct(_) => wide += 1,
                    }

                    let palette = match t.palette() {
                        tile::PaletteRef::Splt(i) => {
                            format!("SPLT {i}")
                        }
                        tile::PaletteRef::Embedded(at) => {
                            embedded += 1;
                            format!("embedded @{at:#x}")
                        }
                    };

                    println!(
                        "  {:<20} v{version} {:>4}x{:<4} {:<7} {shape}  {palette}  mode {} level {}",
                        r.name(),
                        t.width(),
                        t.height(),
                        match t.format() {
                            tile::PixelFormat::Indexed => "8-bit",
                            tile::PixelFormat::Direct(tile::Packing::Rgb555) => "rgb555",
                            tile::PixelFormat::Direct(tile::Packing::Rgb565) => "rgb565",
                        },
                        t.mode().word(),
                        t.level(),
                    );
                }
                Err(e) => {
                    eprintln!("  {}: {e}", r.name());
                    failed += 1;
                }
            }
        }

        let seen: Vec<String> = versions.iter().map(|(v, n)| format!("v{v}:{n}")).collect();

        println!(
            "   {} | {indexed} indexed, {wide} direct, {embedded} embedded palettes, {empty} empty slots",
            seen.join(" ")
        );
    }

    if failed > 0 {
        eprintln!("{failed} tiles failed to decode");
    }

    Ok(failed == 0)
}
