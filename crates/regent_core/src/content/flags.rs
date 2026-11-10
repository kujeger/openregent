//! A mod's inline patches: unit flags and numbers, applied after archive
//! and XML descriptions load, and `.dat` member values, applied after
//! the GPL images load.
//!
//! ```text
//!  mod.toml [[unit]]      ──> compile ──> one archive, loaded last  (DUNT)
//!  mod.toml [[prototype]] ──> initials ──> gpl::load::Program::with_members
//! ```

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::Deserialize;

use super::Error;
use crate::data::cam::{self, ChunkFlag, Written};
use crate::data::catalogue::Catalogue;
use crate::data::desc::{
    BUILDING_NUMBERS, BuildingFlag, CHARACTER_NUMBERS, CharacterFlag, Info, Number, NumberField,
    Part, Unit, UnitKind,
};
use crate::data::fourcc::{FourCc, string_id};
use crate::data::gpltype::TypeCode;

const VERSION: u32 = 1;

/// The largest float a `.dat` text gives: GPL floats are fixed point, 10
/// fraction bits in a signed word (`sim::world::str_to_fixed`).
const MAX_FLOAT: f64 = 2_097_152.0;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    version: u32,
    #[serde(default)]
    unit: Vec<Change>,
    #[serde(default)]
    prototype: Vec<Initial>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Change {
    id: String,
    field: Option<Field>,
    #[serde(default)]
    add: Vec<String>,
    #[serde(default)]
    clear: Vec<String>,
    /// Number fields by XML name: `set = { MaxGuildMembers = 8 }`.
    #[serde(default)]
    set: BTreeMap<String, toml::Value>,
}

/// A `.dat` instance's member values, as `[[prototype]]` writes them:
/// `instance = "GuardHouse2"`, `set = { Max_Guards = 2 }`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Initial {
    /// The instance, any case.
    pub instance: String,
    /// Members by name, any case, and their values.
    pub set: BTreeMap<String, toml::Value>,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Field {
    Info,
    Building,
    Character,
}

impl Field {
    const fn part(self) -> ([u8; 4], usize) {
        match self {
            Self::Info => (*b"PRIM", 0),
            Self::Building => (*b"SECD", 0x28),
            Self::Character => (*b"SECD", 0x60),
        }
    }

    fn mask(self, name: &str) -> Option<u32> {
        match self {
            Self::Info => Info::ALL
                .iter()
                .find(|f| f.name() == name)
                .map(|f| 1 << (*f as u32)),
            Self::Building => BuildingFlag::ALL
                .iter()
                .find(|f| f.name() == name)
                .map(|f| 1 << (*f as u32)),
            Self::Character => CharacterFlag::ALL
                .iter()
                .find(|f| f.name() == name)
                .map(|f| 1 << (*f as u32)),
        }
    }
}

impl Change {
    /// A field for its flags, known flags, numbers by known names and in
    /// range.
    fn check(&self) -> Result<(), Error> {
        let flags = self.add.iter().chain(&self.clear);

        match self.field {
            None if !self.add.is_empty() || !self.clear.is_empty() => {
                Err(Error::NoField(self.id.clone()))?;
            }
            Some(field) => {
                for name in flags {
                    field
                        .mask(name)
                        .ok_or_else(|| Error::UnknownFlag(name.clone()))?;
                }
            }
            None => {}
        }

        for (name, value) in &self.set {
            let number = BUILDING_NUMBERS
                .iter()
                .chain(&CHARACTER_NUMBERS)
                .find(|(n, _, _)| n == name)
                .ok_or_else(|| Error::UnknownNumber(name.clone()))?;

            word(number, value)?;
        }

        Ok(())
    }
}

impl Initial {
    /// What needs no program: each value one some member type takes.
    fn check(&self) -> Result<(), Error> {
        let taken = [
            TypeCode::Integer,
            TypeCode::Float,
            TypeCode::Boolean,
            TypeCode::String,
        ];

        for (member, value) in &self.set {
            taken
                .iter()
                .find_map(|ty| text(*ty, value))
                .ok_or_else(|| self.bad(member, value))?;
        }

        Ok(())
    }

    /// `value` refused for `member`.
    #[must_use]
    pub fn bad(&self, member: &str, value: &toml::Value) -> Error {
        Error::BadNumber {
            name: format!("{}.{member}", self.instance),
            value: value.to_string(),
        }
    }
}

/// `value` as the `.dat` text a member of type `ty` reads back as it:
/// a whole number 0 up (the engine reads a leading `-` as a name), a
/// fixed-point float, `true` or `false`, a string. `None` for a value
/// the type cannot hold, and for the types a manifest does not set.
#[must_use]
pub fn text(ty: TypeCode, value: &toml::Value) -> Option<String> {
    let float = |v: f64| (v.is_finite() && v.abs() < MAX_FLOAT).then(|| v.to_string());

    match (ty, value) {
        (TypeCode::Integer, toml::Value::Integer(v)) => i32::try_from(*v)
            .ok()
            .filter(|v| *v >= 0)
            .map(|v| v.to_string()),
        #[allow(clippy::cast_precision_loss, reason = "a whole number in range")]
        (TypeCode::Float, toml::Value::Integer(v)) => float(*v as f64),
        (TypeCode::Float, toml::Value::Float(v)) => float(*v),
        (TypeCode::Boolean, toml::Value::Boolean(v)) => Some(v.to_string()),
        (TypeCode::String | TypeCode::Function, toml::Value::String(v)) => Some(v.clone()),
        _ => None,
    }
}

/// `value` as number field `field`'s word, where it fits.
fn word(field: &NumberField, value: &toml::Value) -> Result<u32, Error> {
    let (name, _, kind) = field;
    let float = |v: f64| {
        #[allow(clippy::cast_possible_truncation, reason = "the field is an f32")]
        let f = v as f32;

        (f.is_finite() && f >= 0.0).then(|| f.to_bits())
    };
    let fits = match (kind, value) {
        (Number::Count, toml::Value::Integer(v)) => i32::try_from(*v)
            .ok()
            .filter(|v| *v >= 0)
            .map(i32::cast_unsigned),
        (Number::Signed, toml::Value::Integer(v)) => i32::try_from(*v).ok().map(i32::cast_unsigned),
        #[allow(clippy::cast_precision_loss, reason = "a small whole number")]
        (Number::Float, toml::Value::Integer(v)) => float(*v as f64),
        (Number::Float, toml::Value::Float(v)) => float(*v),
        _ => None,
    };

    fits.ok_or_else(|| Error::BadNumber {
        name: (*name).to_owned(),
        value: value.to_string(),
    })
}

/// `files` read and checked as far as no catalogue or program is needed:
/// syntax, version, flag and number names, values.
///
/// # Errors
///
/// As [`compile`] and `Program::with_members`, but for what a unit's
/// record or the program decides.
pub fn check(files: &[(PathBuf, String)]) -> Result<(), Error> {
    read(files).map(|_| ())
}

/// Every file read and checked, in order.
fn read(files: &[(PathBuf, String)]) -> Result<Vec<(PathBuf, File)>, Error> {
    files
        .iter()
        .map(|(path, text)| {
            let at = |e: Error| e.at(path.clone());
            let file: File = toml::from_str(text).map_err(|e| at(Error::from(e)))?;

            if file.version != VERSION {
                Err(at(Error::Version {
                    what: "flag patch",
                    version: file.version,
                }))?;
            }

            for change in &file.unit {
                change.check().map_err(at)?;
            }

            for initial in &file.prototype {
                initial.check().map_err(at)?;
            }

            Ok((path.clone(), file))
        })
        .collect()
}

/// Patch winning unit records. Files and entries apply in order.
///
/// # Errors
///
/// If a file or entry is invalid, or a unit is missing.
pub fn compile(base: &Catalogue<'_>, files: &[(PathBuf, String)]) -> Result<Vec<u8>, Error> {
    let mut records = BTreeMap::new();

    for (path, file) in read(files)? {
        for change in &file.unit {
            apply(base, &mut records, change).map_err(|e| e.at(path.clone()))?;
        }
    }

    let records = records
        .iter()
        .map(|(id, (name, body)): (&FourCc, &(String, Vec<u8>))| {
            (*id, name.as_str(), body.as_slice())
        })
        .collect();
    Ok(cam::write(&[Written {
        tag: FourCc(*b"DUNT"),
        flag: ChunkFlag::Clear,
        records,
    }]))
}

/// Every file's `[[prototype]]` entries, checked as [`check`] does, in
/// order, each with its file: what `gpl::load::Program::with_members`
/// sets.
///
/// # Errors
///
/// As [`check`].
pub fn initials(files: &[(PathBuf, String)]) -> Result<Vec<(PathBuf, Initial)>, Error> {
    Ok(read(files)?
        .into_iter()
        .flat_map(|(path, file)| file.prototype.into_iter().map(move |i| (path.clone(), i)))
        .collect())
}

fn apply(
    base: &Catalogue<'_>,
    records: &mut BTreeMap<FourCc, (String, Vec<u8>)>,
    change: &Change,
) -> Result<(), Error> {
    let id = FourCc::from_word(string_id(&change.id).cast_unsigned());
    let at = base
        .find(b"DUNT", id)
        .ok_or_else(|| Error::UnknownUnit(change.id.clone()))?;
    let original = base
        .record(b"DUNT", at)
        .ok_or_else(|| Error::MissingUnit(change.id.clone()))?;
    let mut payload = records
        .get(&id)
        .map_or_else(|| original.data.to_vec(), |(_, body)| body.clone());

    if let Some(field) = change.field {
        flags(&mut payload, change, field)?;
    }

    numbers(&mut payload, change)?;
    records.insert(id, (original.name().into_owned(), payload));
    Ok(())
}

/// Where in `payload` part `tag`'s word at `field_at` starts.
fn offset(payload: &[u8], change: &Change, tag: [u8; 4], field_at: usize) -> Result<usize, Error> {
    let part = Part::parse(payload)?;
    let body = part.find(&tag).ok_or_else(|| Error::Lacks {
        unit: change.id.clone(),
        part: String::from_utf8_lossy(&tag).into_owned(),
    })?;

    if body.body.get(field_at..field_at + 4).is_none() {
        Err(Error::ShortField(change.id.clone()))?;
    }

    Ok(body.body.as_ptr() as usize - payload.as_ptr() as usize + field_at)
}

/// The word at `at`, written by `f`.
fn rewrite(
    payload: &mut [u8],
    at: usize,
    change: &Change,
    f: impl FnOnce(u32) -> Result<u32, Error>,
) -> Result<(), Error> {
    let bytes = payload
        .get_mut(at..at + 4)
        .ok_or_else(|| Error::ShortField(change.id.clone()))?;
    let word = f(u32::from_le_bytes(bytes.try_into().expect("four bytes")))?;

    bytes.copy_from_slice(&word.to_le_bytes());
    Ok(())
}

/// `change`'s flags in `field`'s word, every other field kept.
fn flags(payload: &mut [u8], change: &Change, field: Field) -> Result<(), Error> {
    let kind = Unit::parse(&Part::parse(payload)?)?.kind();
    let suitable = match field {
        Field::Info => true,
        Field::Building => kind == UnitKind::Building,
        Field::Character => kind == UnitKind::Character,
    };

    if !suitable {
        Err(Error::WrongKind(change.id.clone()))?;
    }

    let (tag, field_at) = field.part();
    let at = offset(payload, change, tag, field_at)?;
    let mask = |name: &String| {
        field
            .mask(name)
            .ok_or_else(|| Error::UnknownFlag(name.clone()))
    };

    rewrite(payload, at, change, |mut word| {
        for name in &change.add {
            word |= mask(name)?;
        }

        for name in &change.clear {
            word &= !mask(name)?;
        }

        Ok(word)
    })
}

/// `change`'s numbers in the unit's `SECD`, by its kind's names.
fn numbers(payload: &mut [u8], change: &Change) -> Result<(), Error> {
    let kind = Unit::parse(&Part::parse(payload)?)?.kind();
    let table: &[NumberField] = match kind {
        UnitKind::Building => &BUILDING_NUMBERS,
        UnitKind::Character => &CHARACTER_NUMBERS,
        _ => &[],
    };

    for (name, value) in &change.set {
        let field = table
            .iter()
            .find(|(n, _, _)| n == name)
            .ok_or_else(|| Error::WrongKind(change.id.clone()))?;
        let new = word(field, value)?;
        let at = offset(payload, change, *b"SECD", field.1)?;

        rewrite(payload, at, change, |_| Ok(new))?;
    }

    Ok(())
}
