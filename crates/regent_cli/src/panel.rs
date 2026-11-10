//! `SMNU` panel layouts, printed as items and rectangles.
//!
//! `SMNU` framing is fitted to the bytes, not read from a header.

use regent_core::data::cam::Archive;
use regent_core::data::smnu::Panel;

use super::util::{gather_all, name, read_bytes};

/// The `SMNU` payloads in one archive, in record order.
fn panels(bytes: &[u8]) -> Result<Vec<(String, Vec<u8>)>, String> {
    let archive = Archive::parse(bytes).map_err(|e| e.to_string())?;

    Ok(archive
        .records()
        .filter(|(chunk, _)| chunk.tag.as_bytes() == b"SMNU")
        .map(|(_, record)| (record.id.to_string(), record.data.to_vec()))
        .collect())
}

/// Print every panel in the archives given.
///
/// # Errors
///
/// If a path will not read, an archive will not parse, or a panel does not.
pub fn cmd_panels(record: Option<&str>, paths: &[String]) -> Result<bool, String> {
    let mut found = 0;

    // HD's UI layouts are `.dat`; the rest `.cam`. Neither is required.
    let mut files = gather_all(paths, "cam").unwrap_or_default();
    files.extend(gather_all(paths, "dat").unwrap_or_default());
    files.sort();

    for path in files {
        let bytes = read_bytes(&path.to_string_lossy())?;
        let Ok(held) = panels(&bytes) else {
            continue;
        };

        for (id, payload) in held {
            if record.is_some_and(|want| want != id) {
                continue;
            }

            let panel =
                Panel::parse(&payload).map_err(|e| format!("{} {id}: {e}", path.display()))?;

            found += 1;
            show(&format!("{} {id}", name(&path.to_string_lossy())), &panel);
        }
    }

    if found == 0 {
        Err("no SMNU record in those paths".to_owned())
    } else {
        Ok(true)
    }
}

/// One panel: its frame, its item count, and every rectangle in it.
fn show(label: &str, panel: &Panel) {
    let rects = panel.rects().count();
    let frame = panel.frame();

    println!(
        "\n== {label}  {} words, {} items, {rects} rectangles",
        panel.words(),
        panel.items().count()
    );

    if let Some(frame) = frame {
        println!(
            "   frame {}x{} at ({}, {})",
            frame.width, frame.height, frame.x, frame.y
        );
    }

    for item in panel.items() {
        let at = item.value_offset();

        if let Some(rect) = item.rect() {
            println!(
                "  {at:#07x}  rect     {:>6} {:>6} {:>6} {:>6}",
                rect.x, rect.y, rect.width, rect.height
            );
        } else if let Some(id) = item.id() {
            println!("  {at:#07x}  id       {id}");
        } else {
            let value = item
                .value
                .iter()
                .map(|w| format!("{w:>12}"))
                .collect::<Vec<_>>()
                .join(" ");

            println!("  {at:#07x}  key {:<4} {value}", item.key);
        }
    }
}
