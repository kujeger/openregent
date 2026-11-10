//! `SMNU` panel layouts, verified by differential.
//!
//! A panel has no lengths, names or redundancy, so a reading cannot fail on
//! the file it was fitted to. HD's thirteen `Data/UIData_<w>_<h>.dat` hold the
//! same panels per screen size; words that differ were computed from the
//! screen size.
//!
//! [`what_moves_between_resolutions_is_geometry_and_one_picture`] asks where
//! the parse puts the differing words: 97 of 102 in rectangles, five in one
//! panel swapping a picture.
//!
//! Do not bucket movers by slot: a one-word key's only slot is 0, so its words
//! would read as rectangle `x`.

mod common;

use std::collections::BTreeMap;
use std::path::Path;

use regent_core::data::cam;
use regent_core::data::smnu::{self, Panel, Rect};

/// Every `SMNU` payload under `root`, as `(archive, id, bytes)`.
///
/// Includes `.dat`: HD's `UIData_*.dat` are CYLB archives.
fn panels(root: &Path) -> Vec<(String, String, Vec<u8>)> {
    let mut files = common::archives(root);

    files.extend(common::walk(root, &["dat"]));
    files.sort();

    let mut out = Vec::new();

    for path in files {
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let Ok(archive) = cam::Archive::parse(&bytes) else {
            continue;
        };

        let name = path
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());

        for (chunk, record) in archive.records() {
            if chunk.tag.as_bytes() == b"SMNU" {
                out.push((name.clone(), record.id.to_string(), record.data.to_vec()));
            }
        }
    }

    out
}

/// Exact counts per edition.
///
/// Weak: one-word items consume anything, so this checks only the two wide keys
/// and the magic.
#[test]
fn every_shipped_panel_consumes_exactly() {
    let corpora = [("Windows HD", common::hd(), 303)];

    for (edition, root, expected) in corpora {
        let found = panels(&root);

        for (archive, id, bytes) in &found {
            Panel::parse(bytes).unwrap_or_else(|e| panic!("{edition} {archive} {id}: {e}"));
        }

        assert_eq!(found.len(), expected, "{edition} panel count changed");
    }
}

/// The format's only outside witness: Phantom's Haunt zeroes the last two of
/// four words at `AP10 + 0x0d34` to disable the
/// temple-spell button, so those four are one control's rectangle.
///
/// The reader must put a rectangle item there, holding the mod author's
/// numbers.
#[test]
fn the_mods_anchor_lands_on_a_rectangle() {
    let anchors = [("Windows HD", common::hd().join("Data/textdata.cam"), 0x0d34)];

    for (edition, path, offset) in anchors {
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let archive = cam::Archive::parse(&bytes).expect("textdata");

        let payload = archive
            .records()
            .find(|(chunk, r)| chunk.tag.as_bytes() == b"SMNU" && r.id.to_string() == "AP10")
            .map(|(_, r)| r.data)
            .expect("AP10");

        let panel = Panel::parse(payload).expect("it parses");
        let item = panel
            .at_offset(offset)
            .unwrap_or_else(|| panic!("{edition}: no item boundary at {offset:#06x}"));

        assert_eq!(
            item.rect(),
            Some(Rect {
                x: 103,
                y: 162,
                width: 93,
                height: 26
            }),
            "{edition}: the mod's rectangle changed"
        );
    }
}

/// HD's thirteen UI layouts, as `(width, height, panels)`.
fn layouts() -> Vec<(i32, i32, BTreeMap<String, Vec<u8>>)> {
    let dir = common::hd().join("Data");
    let mut out = Vec::new();

    for entry in std::fs::read_dir(&dir).expect("HD Data/").flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();

        let Some(size) = name
            .strip_prefix("UIData_")
            .and_then(|rest| rest.strip_suffix(".dat"))
        else {
            continue;
        };

        let Some((width, height)) = size.split_once('_') else {
            continue;
        };

        let bytes = std::fs::read(entry.path()).expect("a UI layout");
        let archive = cam::Archive::parse(&bytes).expect("a CYLB archive");

        let held = archive
            .records()
            .filter(|(chunk, _)| chunk.tag.as_bytes() == b"SMNU")
            .map(|(_, r)| (r.id.to_string(), r.data.to_vec()))
            .collect();

        out.push((
            width.parse().expect("a width"),
            height.parse().expect("a height"),
            held,
        ));
    }

    out.sort_by_key(|(w, h, _)| (*w, *h));
    out
}

/// One file per HD resolution.
#[test]
fn hd_ships_one_ui_layout_per_resolution() {
    let found = layouts();
    let sizes: Vec<(i32, i32)> = found.iter().map(|(w, h, _)| (*w, *h)).collect();

    assert_eq!(
        sizes,
        [
            (800, 600),
            (1024, 768),
            (1280, 768),
            (1280, 800),
            (1280, 960),
            (1280, 1024),
            (1360, 768),
            (1440, 900),
            (1600, 900),
            (1600, 1200),
            (1680, 1050),
            (1920, 1080),
            (1920, 1200),
        ],
        "the resolutions HD ships changed"
    );

    for (width, height, held) in &found {
        assert!(!held.is_empty(), "{width}x{height} holds no panel");
    }
}

/// One panel as several screens lay it out: `(width, height, payload)`.
type Versions = Vec<(i32, i32, Vec<u8>)>;

/// One panel at every resolution that holds it, at one payload length.
///
/// A different length means an added or lost control; such versions are
/// dropped.
fn comparable() -> Vec<(String, Versions)> {
    let files = layouts();
    let mut by_panel: BTreeMap<String, Versions> = BTreeMap::new();

    for (width, height, held) in files {
        for (id, bytes) in held {
            by_panel.entry(id).or_default().push((width, height, bytes));
        }
    }

    by_panel
        .into_iter()
        .filter_map(|(id, mut versions)| {
            // Keep the largest set sharing one length.
            let mut counts: BTreeMap<usize, usize> = BTreeMap::new();

            for (_, _, bytes) in &versions {
                *counts.entry(bytes.len()).or_default() += 1;
            }

            let keep = counts
                .iter()
                .max_by_key(|(_, n)| **n)
                .map(|(len, _)| *len)?;
            versions.retain(|(_, _, bytes)| bytes.len() == keep);

            (versions.len() > 1).then_some((id, versions))
        })
        .collect()
}

/// A payload as its words.
fn words_of(bytes: &[u8]) -> Vec<i32> {
    bytes
        .as_chunks::<4>()
        .0
        .iter()
        .copied()
        .map(i32::from_le_bytes)
        .collect()
}

/// The indices of the words that are not the same in every version given.
fn moving(versions: &[(i32, i32, Vec<u8>)]) -> Vec<usize> {
    let all: Vec<Vec<i32>> = versions.iter().map(|(_, _, b)| words_of(b)).collect();

    (0..all[0].len())
        .filter(|i| all.iter().any(|w| w[*i] != all[0][*i]))
        .collect()
}

/// Which item owns each word of a payload, as `word -> (key, slot)`.
fn owners(panel: &Panel) -> BTreeMap<usize, (i32, usize)> {
    let mut out = BTreeMap::new();

    for item in panel.items() {
        for slot in 0..item.value.len() {
            out.insert(item.at + 1 + slot, (item.key, slot));
        }
    }

    out
}

/// Screen size changes geometry, plus one panel's picture.
///
/// Diffs payloads and asks the parse who owns each differing word. Wrong
/// framing would scatter them. The five non-geometry movers are in `APMK`.
///
/// Totals are pinned, so an empty mover set cannot pass.
#[test]
fn what_moves_between_resolutions_is_geometry_and_one_picture() {
    let mut seen: BTreeMap<i32, usize> = BTreeMap::new();
    let mut movers = Vec::new();

    for (id, versions) in comparable() {
        let moved = moving(&versions);

        if moved.is_empty() {
            continue;
        }

        movers.push((id.clone(), moved.len()));

        let panel = Panel::parse(&versions[0].2).unwrap_or_else(|e| panic!("{id}: {e}"));
        let owner = owners(&panel);

        for at in moved {
            let (key, _) = owner
                .get(&at)
                .copied()
                .unwrap_or_else(|| panic!("{id}: word {at} moves and belongs to no item's value"));

            *seen.entry(key).or_default() += 1;
        }
    }

    assert_eq!(
        movers,
        [("APMK".to_owned(), 70), ("APdb".to_owned(), 32)],
        "the panels that depend on the screen changed"
    );

    // Key 12 is the picture, 13 goes with it; 42 is a second rectangle.
    assert_eq!(
        seen,
        BTreeMap::from([
            (smnu::RECT, 96),
            (smnu::RECT_TOO, 1),
            (smnu::ID, 2),
            (13, 3)
        ]),
        "what moves with the screen changed"
    );
}

/// `APMK` shows `INse` at 768 high, `INDz` at 900.
#[test]
fn the_panel_that_is_not_geometry_is_swapping_a_picture() {
    let (_, versions) = comparable()
        .into_iter()
        .find(|(id, _)| id == "APMK")
        .expect("APMK is in the layouts");

    let panel = Panel::parse(&versions[0].2).expect("APMK parses");
    let owner = owners(&panel);

    let mut swapped = Vec::new();

    for at in moving(&versions) {
        if owner.get(&at).copied().map(|(key, _)| key) != Some(smnu::ID) {
            continue;
        }

        for (_, height, bytes) in &versions {
            let seen = Panel::parse(bytes).expect("it parses");
            let id = seen
                .items()
                .find(|item| item.at + 1 == at)
                .and_then(|item| item.id())
                .expect("a record id");

            swapped.push((*height, id.to_string()));
        }
    }

    swapped.sort();
    swapped.dedup();

    assert_eq!(
        swapped,
        [(768, "INse".to_owned()), (900, "INDz".to_owned())],
        "the picture APMK swaps changed"
    );
}

/// The four words are `(x, y, width, height)`, in that order.
///
/// `APMK`'s comparable set has files sharing a screen height:
///
/// | | 1024×768 | 1280×768 | 1360×768 | 1440×900 | 1600×900 |
/// |---|---|---|---|---|---|
///
/// A word differing within a height group follows the width; otherwise the
/// height. The slots do not mix:
///
/// | Follows | Slot 0 | Slot 1 | Slot 2 | Slot 3 |
/// |---|---|---|---|---|
/// | the width | 11 | 0 | 5 | 0 |
/// | the height | 0 | 40 | 0 | 8 |
#[test]
fn a_rectangles_four_words_are_x_y_width_and_height() {
    let (_, versions) = comparable()
        .into_iter()
        .find(|(id, _)| id == "APMK")
        .expect("APMK is in the layouts");

    let screens: Vec<(i32, i32)> = versions.iter().map(|(w, h, _)| (*w, *h)).collect();

    assert_eq!(
        screens,
        [
            (1024, 768),
            (1280, 768),
            (1360, 768),
            (1440, 900),
            (1600, 900)
        ],
        "the APMK set changed, and the argument depends on its shape"
    );

    let all: Vec<Vec<i32>> = versions.iter().map(|(_, _, b)| words_of(b)).collect();
    let panel = Panel::parse(&versions[0].2).expect("APMK parses");
    let owner = owners(&panel);

    // Files sharing a screen height.
    let groups: [&[usize]; 2] = [&[0, 1, 2], &[3, 4]];

    let (mut by_width, mut by_height) = ([0; 4], [0; 4]);

    for at in moving(&versions) {
        let Some((smnu::RECT, slot)) = owner.get(&at).copied() else {
            continue;
        };

        let varies_within = groups
            .iter()
            .any(|group| group.iter().any(|&i| all[i][at] != all[group[0]][at]));

        if varies_within {
            by_width[slot] += 1;
        } else {
            by_height[slot] += 1;
        }
    }

    assert_eq!(by_width, [11, 0, 5, 0], "what follows the screen width");
    assert_eq!(by_height, [0, 40, 0, 8], "what follows the screen height");
}

/// Every panel in every edition opens with a rectangle.
///
/// Some have a control past the frame; controls may hang outside their parent.
#[test]
fn a_panel_opens_with_a_frame_the_rest_mostly_fits_inside() {
    let mut framed = 0;
    let mut overflowing = 0;

    for root in [common::hd()] {
        for (archive, id, bytes) in panels(&root) {
            let panel = Panel::parse(&bytes).unwrap_or_else(|e| panic!("{archive} {id}: {e}"));
            let Some(frame) = panel.frame() else {
                panic!("{archive} {id}: the first item is not a rectangle");
            };

            framed += 1;

            if panel
                .rects()
                .skip(1)
                .any(|r| r.x + r.width > frame.width || r.y + r.height > frame.height)
            {
                overflowing += 1;
            }
        }
    }

    assert_eq!(framed, 303, "panels changed");
    assert_eq!(
        overflowing, 46,
        "panels with a control outside the frame changed"
    );
}

/// The shipped GPL debugger's own interface, and what it does not name.
///
/// `GPLDebuggerUI.cam` ships with HD only; `UseGPLDebugger` enables it. It
/// does not name opcode 39 (past the 39-entry table): no string matches an
/// opcode name.
///
/// `GDB1` to `GDB6`, against the PDF's five panels (Line Debugger,
/// Breakpoints, Tracking, Profiler, Data Sets): `GDB4` and `GDB6` are
/// byte-identical.
#[test]
fn the_gpl_debuggers_panels_are_five_shipped_as_six() {
    use regent_core::data::payload::StringTable;

    let path = common::hd().join("Data/GPLDebuggerUI.cam");
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let archive = cam::Archive::parse(&bytes).expect("a CYLB archive");

    let mut menus: BTreeMap<String, &[u8]> = BTreeMap::new();
    let mut tables: BTreeMap<String, &[u8]> = BTreeMap::new();

    for (chunk, record) in archive.records() {
        let into = match chunk.tag.as_bytes() {
            b"SMNU" => &mut menus,
            b"STRT" => &mut tables,
            _ => panic!("{} holds a {}", path.display(), chunk.tag),
        };

        into.insert(record.id.to_string(), record.data);
    }

    let names: Vec<&String> = menus.keys().collect();

    assert_eq!(names, ["GDB1", "GDB2", "GDB3", "GDB4", "GDB5", "GDB6"]);
    assert_eq!(tables.keys().collect::<Vec<_>>(), names, "one table each");

    // Six records, five panels.
    assert_eq!(menus["GDB4"], menus["GDB6"], "the duplicated layout");
    assert_eq!(tables["GDB4"], tables["GDB6"], "the duplicated labels");

    let words = |id: &str| -> Vec<String> {
        StringTable::parse(tables[id])
            .unwrap_or_else(|e| panic!("{id}: {e}"))
            .iter()
            .map(std::borrow::Cow::into_owned)
            .collect()
    };

    // A label unique to each panel.
    for (id, marker) in [
        ("GDB1", "Call Stack"),
        ("GDB2", "Total Profile Time :"),
        ("GDB3", "Xml Data"),
        ("GDB4", "Jump To"),
        ("GDB5", "Game Object and Script Tracking"),
    ] {
        assert!(words(id).iter().any(|s| s == marker), "{id} lost {marker}");
    }

    // Stepping labels. **Inferred**: "Micro Step" is one GPL operation,
    // "Step Over" a source line.
    let debugger = words("GDB1");

    for label in ["Step Into", "Step Over", "Step Out", "Micro Step", "Locals"] {
        assert!(debugger.iter().any(|s| s == label), "GDB1 lost {label}");
    }

    // No opcode name.
    let everything: Vec<String> = names.iter().flat_map(|id| words(id)).collect();

    for entry in regent_core::gpl::opcode::TABLE {
        assert!(
            !everything.iter().any(|s| s == entry.name),
            "the debugger names {}, so it may name opcode 39 too",
            entry.name
        );
    }
}
