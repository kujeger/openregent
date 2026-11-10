//! Reader for `.gplproj` project files.
//!
//! A project lists the inputs of one `.bcd`. `MakeGPL.bat` runs
//! `gplbcc -in <project>.gplproj -out <name>.bcd` from the project's directory:
//!
//! ```text
//! data="Guardhouse.dat"
//! source="WrathOfKrolm.gpl"
//! ```
//!
//! Paths are Windows-style, relative to the project file. Both keys repeat;
//! listed order is read order. Shipped projects use CRLF; nothing here relies
//! on it.

use std::path::{Path, PathBuf};

/// The files one `.bcd` is compiled from, in listed order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Project {
    /// `.dat` prototype-data files.
    pub data: Vec<PathBuf>,
    /// `.gpl` source files, resolved against the project's directory.
    pub sources: Vec<PathBuf>,
    /// The paths as spelled, Windows separators included. Every section-1
    /// entry records this.
    pub spelled: Vec<String>,
    /// Unrecognised lines, kept so surprises show.
    pub unknown: Vec<String>,
}

/// Parse a project file. `dir` holds it; listed paths are relative to it.
pub fn parse(src: &str, dir: &Path) -> Project {
    let mut out = Project::default();

    for line in src.lines().map(str::trim).filter(|l| !l.is_empty()) {
        match entry(line) {
            Some(("data", path)) => out.data.push(resolve(dir, path)),
            Some(("source", path)) => {
                out.sources.push(resolve(dir, path));
                out.spelled.push(path.to_owned());
            }
            _ => out.unknown.push(line.to_string()),
        }
    }

    out
}

/// Split `key="value"`, case-insensitively on the key.
fn entry(line: &str) -> Option<(&'static str, &str)> {
    let (key, rest) = line.split_once('=')?;
    let path = rest.trim().strip_prefix('"')?.strip_suffix('"')?;

    // Only these two keys appear in any shipped project.
    ["data", "source"]
        .into_iter()
        .find(|k| key.trim().eq_ignore_ascii_case(k))
        .map(|k| (k, path))
}

/// Turn a Windows-style relative path into one for this platform.
fn resolve(dir: &Path, path: &str) -> PathBuf {
    path.split('\\')
        .filter(|c| !c.is_empty())
        .fold(dir.to_path_buf(), |acc, component| acc.join(component))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proj(src: &str) -> Project {
        parse(src, Path::new("/gpl"))
    }

    #[test]
    fn data_only_project() {
        // The Adjust Guardhouse mod, in full.
        let p = proj("data=\"Guardhouse.dat\"\r\n");

        assert_eq!(p.data, [PathBuf::from("/gpl/Guardhouse.dat")]);
        assert_eq!(p.unknown.len(), 0);
    }

    #[test]
    fn backslashes_become_path_components() {
        // SpecialItemsExample reaches up out of its Data/ directory.
        let p = proj("data=\"..\\SpecialItemsExample_prototypes.dat\"");

        assert_eq!(
            p.data,
            [PathBuf::from("/gpl/../SpecialItemsExample_prototypes.dat")]
        );
    }

    #[test]
    fn order_is_preserved() {
        let p = proj("data=\"b.dat\"\ndata=\"a.dat\"\nsource=\"x.gpl\"");

        assert_eq!(p.data.len(), 2);
        assert!(p.data[0].ends_with("b.dat"));
    }

    #[test]
    fn unrecognised_lines_are_kept() {
        let p = proj("weird=\"x\"\nnonsense");

        assert_eq!(p.unknown.len(), 2);
    }
}
