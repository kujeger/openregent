//! Path walking, file reading, and what several commands parse alike.

use std::io;
use std::path::{Path, PathBuf};

use regent_core::data::catalogue::Library;
use regent_core::data::fourcc::FourCc;
use regent_core::gpl::diag;

/// Whether a file named on the command line is taken whatever its
/// extension, or only one found in a directory is filtered.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Named {
    /// Given by name: taken.
    Given,
    /// Found under a directory: taken only with the extension.
    Found,
}

/// Collect files with the given extension under `path`, recursing.
///
/// Case-insensitive: three shipped sources use `.GPL`.
fn gather(path: &Path, ext: &str, named: Named, out: &mut Vec<PathBuf>) -> io::Result<()> {
    if path.is_dir() {
        let mut entries: Vec<PathBuf> = std::fs::read_dir(path)?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .collect();
        entries.sort();

        for entry in entries {
            gather(&entry, ext, Named::Found, out)?;
        }
    } else if named == Named::Given
        || path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case(ext))
    {
        out.push(path.to_path_buf());
    }

    Ok(())
}

/// Every file `paths` names, and every `.ext` file under a directory they
/// name.
pub fn gather_all(paths: &[String], ext: &str) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();

    for p in paths {
        gather(Path::new(p), ext, Named::Given, &mut files).map_err(|e| format!("{p}: {e}"))?;
    }

    if files.is_empty() {
        Err(format!("no .{ext} files found"))
    } else {
        Ok(files)
    }
}

/// [`gather_all`], as the text each file is called by.
pub fn inputs(paths: &[String], ext: &str) -> Result<Vec<String>, String> {
    Ok(gather_all(paths, ext)?
        .iter()
        .map(|path| path.display().to_string())
        .collect())
}

/// Every non-empty `.cam` under `paths`, loaded.
///
/// # Errors
///
/// If no file reads, or every archive found is empty.
pub fn library(paths: &[String]) -> Result<Library, String> {
    let files = gather_all(paths, "cam")?;
    let library = Library::from_files(&files).map_err(|e| e.to_string())?;

    if library.is_empty() {
        Err("no non-empty .cam archives in those paths".into())
    } else {
        Ok(library)
    }
}

/// A four-character code, such as a record id or a setup pick.
///
/// # Errors
///
/// Text that is not exactly four bytes: `ABJ12` is refused, not cut.
pub fn fourcc(text: &str) -> Result<FourCc, String> {
    <[u8; 4]>::try_from(text.as_bytes())
        .map(|code| FourCc::new(&code))
        .map_err(|_| format!("{text} is not a four-character code"))
}

pub fn read(path: &Path) -> Result<String, String> {
    // Corpus is ASCII; lossy so a stray byte does not fail.
    std::fs::read(path)
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .map_err(|e| format!("{}: {e}", path.display()))
}

/// Read a whole file as bytes.
pub fn read_bytes(path: &str) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|e| format!("{path}: {e}"))
}

/// The last component of a path.
pub fn name(path: &str) -> &str {
    Path::new(path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(path)
}

/// Read every source up front so parsed trees can borrow from them.
pub fn load(paths: &[String], ext: &str) -> Result<Vec<(PathBuf, String)>, String> {
    gather_all(paths, ext)?
        .into_iter()
        .map(|p| read(&p).map(|s| (p, s)))
        .collect()
}

pub fn report_errors(
    path: &Path,
    src: &str,
    errors: &[regent_core::gpl::ParseError],
    suppressed: usize,
) {
    let name = path.display().to_string();

    for e in errors {
        let (line, col) = diag::location(src, e.span.start);
        eprintln!("{name}:{line}:{col}: {}", e.message);
    }

    if suppressed > 0 {
        eprintln!("{name}: {suppressed} more errors suppressed — is this GPL source?");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Four bytes are a code; five are an error, not the first four.
    #[test]
    fn a_code_is_exactly_four_characters() {
        assert_eq!(fourcc("ABJ1"), Ok(FourCc::new(b"ABJ1")));
        assert!(fourcc("ABJ12").is_err());
        assert!(fourcc("ABJ").is_err());
    }
}
