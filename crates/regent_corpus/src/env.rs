//! A variable's path: from the process environment, else from a `.env`
//! file. Tests and `regent_cli` read here; openregent's runtime reads
//! process variables only.
//!
//! `dotenvy` finds `.env` in the working directory or the nearest folder
//! above it, and loads it once, the process environment winning. Under
//! `cargo test` the working directory is the crate's, so the repository
//! root's `.env` is found.
//!
//! A value may be `~/…`, absolute, or relative to the `.env` file's folder
//! (the working directory when there is none).

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// A variable set neither in the environment nor in `.env`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unset(pub String);

impl fmt::Display for Unset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} is not set, in the environment or in .env", self.0)
    }
}

impl std::error::Error for Unset {}

/// A variable's path, the environment first, then `.env`; `None` when
/// set in neither.
#[must_use]
pub fn lookup(key: &str) -> Option<PathBuf> {
    let base = dotenv();

    std::env::var(key).ok().map(|value| expand(value, base))
}

/// [`lookup`], with an error naming both places.
///
/// # Errors
///
/// When set in neither.
pub fn path(key: &str) -> Result<PathBuf, Unset> {
    lookup(key).ok_or_else(|| Unset(key.to_owned()))
}

/// `.env` loaded into the process environment, once: the folder it was
/// found in, if any.
fn dotenv() -> Option<&'static Path> {
    static FOUND: OnceLock<Option<PathBuf>> = OnceLock::new();

    FOUND
        .get_or_init(|| dotenvy::dotenv().ok())
        .as_deref()
        .and_then(Path::parent)
}

/// `~/` expanded; a relative path is from `base`, if any.
fn expand(value: String, base: Option<&Path>) -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();

    match (value.strip_prefix("~/"), base) {
        (Some(rest), _) => PathBuf::from(home).join(rest),
        (None, Some(base)) if Path::new(&value).is_relative() => base.join(value),
        (None, _) => PathBuf::from(value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_relative_value_is_from_the_dotenv_folder() {
        let base = Path::new("/repo");

        assert_eq!(expand("a/b".to_owned(), Some(base)), base.join("a/b"));
        assert_eq!(expand("a/b".to_owned(), None), PathBuf::from("a/b"));
        assert_eq!(expand("/a/b".to_owned(), Some(base)), PathBuf::from("/a/b"));
    }

    #[test]
    fn an_unset_variable_names_both_places() {
        assert_eq!(
            Unset("MAJESTY_HD".to_owned()).to_string(),
            "MAJESTY_HD is not set, in the environment or in .env"
        );
    }
}
