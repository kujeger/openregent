//! Text markup: every Workshop colour tag read.
//!
//! Corpus: `MAJESTY_WORKSHOP`. Required.

mod common;

use std::path::{Path, PathBuf};

use regent_core::data::text::{plain, unescaped};

/// The tags' names, lower case, as a search finds them.
const TAG_STARTS: [&str; 4] = ["[color", "[/color", "[fcolor", "[bcolor"];

/// Files a mod's or quest's strings and scripts are in.
const TEXT_FILES: [&str; 4] = ["xml", "mqxml", "mmxml", "gpl"];

/// Every file under `dir` whose extension is one of [`TEXT_FILES`].
fn text_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };

    entries
        .flatten()
        .flat_map(|e| {
            let path = e.path();
            let text = path
                .extension()
                .and_then(|x| x.to_str())
                .is_some_and(|x| TEXT_FILES.iter().any(|t| t.eq_ignore_ascii_case(x)));

            if path.is_dir() {
                text_files(&path)
            } else if text {
                vec![path]
            } else {
                Vec::new()
            }
        })
        .collect()
}

/// Every Workshop line holding a colour tag, stored as HD stores it, is
/// left with none by the reader: no tag text is drawn.
#[test]
fn every_workshop_colour_tag_is_read() {
    let mut tagged = 0;
    let mut files = 0;

    for path in text_files(&common::workshop()) {
        let bytes = std::fs::read(&path).expect("readable");
        let text = String::from_utf8_lossy(&bytes);
        let lines: Vec<&str> = text
            .lines()
            .filter(|l| {
                let lower = l.to_ascii_lowercase();

                TAG_STARTS.iter().any(|t| lower.contains(t))
            })
            .collect();

        for line in &lines {
            let shown = plain(&unescaped(line)).to_ascii_lowercase();

            assert!(
                !TAG_STARTS.iter().any(|t| shown.contains(t)),
                "{}: {line}",
                path.display()
            );
        }

        tagged += lines.len();
        files += usize::from(!lines.is_empty());
    }

    assert_eq!((files, tagged), (10, 1319), "files and lines with a tag");
}
