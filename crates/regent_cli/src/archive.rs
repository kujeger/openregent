//! Archive level: listing and extracting `.cam` contents. Payloads are only
//! identified, not interpreted.

use std::path::PathBuf;

use regent_core::data::{FileKind, bcd, cam, payload, sniff};

use crate::util::{inputs, name, read_bytes};

pub fn cmd_sniff(paths: &[String]) -> Result<bool, String> {
    for p in paths {
        let data = read_bytes(p)?;
        let kind = match sniff(&data) {
            FileKind::Cam => "cam archive",
            FileKind::Bcd => "bcd gpl program",
            FileKind::RgsChart => "rgs chart",
            FileKind::RgsMap => "rgs map / quest",
            FileKind::Unknown => "unknown",
        };

        println!("{:<28} {:>10}  {kind}", name(p), data.len());
    }

    Ok(true)
}

pub fn cmd_tags(paths: &[String]) -> Result<bool, String> {
    let mut failed = 0;

    for p in &inputs(paths, "cam")? {
        let data = read_bytes(p)?;

        match cam::Archive::parse(&data) {
            Err(e) => {
                println!("{:<28} -- {e}", name(p));
                failed += 1;
            }
            Ok(a) => {
                let summary: Vec<String> = a
                    .chunks
                    .iter()
                    .map(|c| {
                        let bytes: usize = c.records.iter().map(cam::Record::len).sum();
                        let flag = match c.flag {
                            cam::ChunkFlag::Set => "*",
                            cam::ChunkFlag::Clear => "",
                        };

                        format!("{}:{}r/{}B{}", c.tag, c.records.len(), bytes, flag)
                    })
                    .collect();

                println!("{:<28} {}", name(p), summary.join("  "));
            }
        }
    }

    Ok(failed == 0)
}

pub fn cmd_list(paths: &[String]) -> Result<bool, String> {
    for p in &inputs(paths, "cam")? {
        let data = read_bytes(p)?;
        let archive = cam::Archive::parse(&data).map_err(|e| format!("{}: {e}", name(p)))?;

        println!("\n== {}", name(p));

        for (chunk, r) in archive.records() {
            println!(
                "  {:>4}  {:<5} {:<18} @{:#010x} {:>10}",
                chunk.tag,
                r.id,
                r.name(),
                r.offset,
                r.len()
            );
        }
    }

    Ok(true)
}

pub fn cmd_strings(paths: &[String]) -> Result<bool, String> {
    for p in &inputs(paths, "cam")? {
        let data = read_bytes(p)?;
        let archive = cam::Archive::parse(&data).map_err(|e| format!("{}: {e}", name(p)))?;

        let Some(chunk) = archive.chunk_by_tag(b"STRT") else {
            continue;
        };

        for r in &chunk.records {
            let table =
                payload::StringTable::parse(r.data).map_err(|e| format!("{}: {e}", r.id))?;

            println!("\n-- {} {} ({} strings)", r.id, r.name(), table.len());

            // Print the key: it is the join (e.g. `QUES` quest names keyed by
            // the `QUnn` codes `QDEP` uses). Keys are a fourcc or a `u32`
            // index (e.g. `AITX`); print printable ones as text.
            for (i, s) in table.iter().enumerate() {
                match table.key(i) {
                    Some(key) if key.as_bytes().iter().all(u8::is_ascii_graphic) => {
                        println!("  [{i}] {key}  {s}");
                    }
                    Some(key) => println!("  [{i}] #{}  {s}", key.word()),
                    None => println!("  [{i}] {s}"),
                }
            }
        }
    }

    Ok(true)
}

pub fn cmd_extract(path: &str, outdir: &str) -> Result<bool, String> {
    let data = read_bytes(path)?;
    let archive = cam::Archive::parse(&data).map_err(|e| format!("{}: {e}", name(path)))?;
    let mut count = 0;

    for (chunk, r) in archive.records() {
        let dir = PathBuf::from(outdir)
            .join(name(path))
            .join(chunk.tag.trimmed());

        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;

        // Names repeat across records, so the index keeps filenames unique.
        let safe: String = r
            .name()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();
        let ext = if chunk.tag == b"WAVE" { "wav" } else { "bin" };
        let file = dir.join(format!("{count:04}_{}_{safe}.{ext}", r.id.trimmed()));

        std::fs::write(&file, r.data).map_err(|e| format!("{}: {e}", file.display()))?;
        count += 1;
    }

    println!("{}: wrote {count} records to {outdir}", name(path));

    Ok(true)
}

/// Parse every recognised file under the given directories and report failures.
///
/// Prints each failure, then the count checked and failed.
pub fn cmd_check_data(dirs: &[String]) -> Result<bool, String> {
    let mut checked = 0;
    let mut failed = 0;

    for dir in dirs {
        let entries = std::fs::read_dir(dir).map_err(|e| format!("{dir}: {e}"))?;

        for entry in entries.flatten() {
            let path = entry.path();
            let label = path.file_name().unwrap_or_default().to_string_lossy();

            // A directory is not a file to check; one that will not read is.
            if path.is_dir() {
                continue;
            }

            let data = match std::fs::read(&path) {
                Ok(data) => data,
                Err(e) => {
                    println!("FAIL {label}: {e}");
                    failed += 1;
                    continue;
                }
            };

            let outcome = match sniff(&data) {
                FileKind::Cam => check_cam(&data),
                FileKind::Bcd => bcd::Container::parse(&data)
                    .map(|_| ())
                    .map_err(|e| e.to_string()),
                FileKind::RgsChart | FileKind::RgsMap => regent_core::data::rgs::File::parse(&data)
                    .map(|_| ())
                    .map_err(|e| e.to_string()),
                FileKind::Unknown => continue,
            };

            checked += 1;

            if let Err(e) = outcome {
                println!("FAIL {label}: {e}");
                failed += 1;
            }
        }
    }

    println!("checked {checked} files, {failed} failed");

    Ok(failed == 0)
}

/// Parse an archive and every payload whose format is understood.
fn check_cam(data: &[u8]) -> Result<(), String> {
    let archive = cam::Archive::parse(data).map_err(|e| e.to_string())?;

    for (chunk, r) in archive.records() {
        let tag = chunk.tag.as_bytes();

        let outcome = if payload::DESCRIPTOR_TAGS.contains(&tag) {
            payload::Descriptor::parse(r.data).map(|_| ())
        } else if payload::PALETTE_TAGS.contains(&tag) {
            payload::Palette::parse(r.data).map(|_| ())
        } else if payload::WAVE_TAGS.contains(&tag) {
            payload::wave(r.data).map(|_| ())
        } else if tag == b"STRT" {
            payload::StringTable::parse(r.data).map(|_| ())
        } else {
            Ok(())
        };

        outcome.map_err(|e| format!("{} {}: {e}", chunk.tag, r.id))?;
    }

    Ok(())
}
