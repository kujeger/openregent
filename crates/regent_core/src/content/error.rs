use std::fmt;
use std::path::PathBuf;

/// Why a variable, a manifest or a flag patch could not be used.
///
/// Displays as one line, for a log or a skipped list:
/// `mods/x/mod.toml: invalid source path ../a.xml`.
#[derive(Debug)]
pub enum Error {
    /// TOML that will not parse.
    Toml(toml::de::Error),
    /// TOML that will not write.
    TomlOut(toml::ser::Error),
    /// A `version` other than the one known, of a `mod`, `quest` or
    /// `flag patch`.
    Version { what: &'static str, version: u32 },
    /// An `id` that is not a GUID, of a `mod` or `quest`.
    Id { what: &'static str, id: String },
    /// A `base` naming no dataset.
    Dataset(String),
    /// A source path that is not relative, or has the wrong extension.
    SourcePath(String),
    /// A script with no name or no sources.
    Script,
    /// A script name used twice in one mod.
    DuplicateScript(String),
    /// A unit no archive record has.
    UnknownUnit(String),
    /// A unit found in the index, but with no record.
    MissingUnit(String),
    /// A unit record that will not parse.
    Record(crate::data::Error),
    /// A field not of this unit's kind: `building` on a hero.
    WrongKind(String),
    /// A unit record without a part: the unit, then the part's tag.
    Lacks { unit: String, part: String },
    /// A flag word cut short.
    ShortField(String),
    /// A flag name no flag has.
    UnknownFlag(String),
    /// `add` or `clear` with no `field`: the unit.
    NoField(String),
    /// A number field name no descriptor has.
    UnknownNumber(String),
    /// A value its number field or `.dat` member cannot hold.
    BadNumber { name: String, value: String },
    /// A `.dat` instance no image has.
    UnknownInstance(String),
    /// A member the instance's prototype does not declare.
    UnknownMember { instance: String, member: String },
    /// Any of the above, in a file.
    In(PathBuf, Box<Error>),
}

impl Error {
    /// This error, in `path`.
    #[must_use]
    pub fn at(self, path: PathBuf) -> Error {
        Error::In(path, Box::new(self))
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Toml(e) => write!(f, "{e}"),
            Error::TomlOut(e) => write!(f, "{e}"),
            Error::Version { what, version } => write!(f, "unsupported {what} version {version}"),
            Error::Id { what, id } => write!(f, "invalid {what} id {id}"),
            Error::Dataset(base) => write!(f, "unknown dataset {base}"),
            Error::SourcePath(path) => write!(f, "invalid source path {path}"),
            Error::Script => f.write_str("a script needs a name and sources"),
            Error::DuplicateScript(name) => write!(f, "duplicate script name {name}"),
            Error::UnknownUnit(id) => write!(f, "unknown unit {id}"),
            Error::MissingUnit(id) => write!(f, "missing unit {id}"),
            Error::Record(e) => write!(f, "{e}"),
            Error::WrongKind(id) => write!(f, "{id} has the wrong unit type"),
            Error::Lacks { unit, part } => write!(f, "{unit} lacks {part}"),
            Error::ShortField(id) => write!(f, "{id} has a short flag field"),
            Error::UnknownFlag(name) => write!(f, "unknown flag {name}"),
            Error::NoField(id) => write!(f, "{id}: add and clear need a field"),
            Error::UnknownNumber(name) => write!(f, "unknown number field {name}"),
            Error::BadNumber { name, value } => write!(f, "{name} cannot hold {value}"),
            Error::UnknownInstance(name) => write!(f, "unknown .dat instance {name}"),
            Error::UnknownMember { instance, member } => {
                write!(f, "{instance} has no member {member}")
            }
            Error::In(path, e) => write!(f, "{}: {e}", path.display()),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Toml(e) => Some(e),
            Error::TomlOut(e) => Some(e),
            Error::Record(e) => Some(e),
            Error::In(_, e) => Some(e.as_ref()),
            _ => None,
        }
    }
}

impl From<toml::de::Error> for Error {
    fn from(e: toml::de::Error) -> Self {
        Error::Toml(e)
    }
}

impl From<toml::ser::Error> for Error {
    fn from(e: toml::ser::Error) -> Self {
        Error::TomlOut(e)
    }
}

impl From<crate::data::Error> for Error {
    fn from(e: crate::data::Error) -> Self {
        Error::Record(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_error_in_a_file_names_the_file_first() {
        let e = Error::SourcePath("../a.xml".to_owned()).at(PathBuf::from("mods/x/mod.toml"));

        assert_eq!(
            e.to_string(),
            "mods/x/mod.toml: invalid source path ../a.xml"
        );
    }
}
