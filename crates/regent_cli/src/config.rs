//! Dependency tables (`data::deps`).
//!
//! `deps` prints the shipped postfix rules (`ABJ2 ABJ3 NOT NOT ||`) as infix,
//! to compare with the file's own comments.

use std::collections::BTreeMap;
use std::path::PathBuf;

use regent_core::data::FourCc;
use regent_core::data::cam::Archive;
use regent_core::data::deps::{self, Expr};
use regent_core::data::payload::StringTable;

use super::util::{gather_all, name, read_bytes};

/// Id to name, from the given archives.
type Names = BTreeMap<FourCc, String>;

/// Print `BDEP` and `QDEP` from every archive that carries them.
///
/// # Errors
///
/// If a path will not read, an archive will not parse, or a rule does not.
pub fn cmd_deps(resolve: Option<&str>, paths: &[String]) -> Result<bool, String> {
    let files = gather_all(paths, "cam")?;
    let names = match resolve {
        Some(at) => collect_names(&gather_all(std::slice::from_ref(&at.to_owned()), "cam")?)?,
        None => Names::new(),
    };

    let mut found = 0;

    for path in files {
        let bytes = read_bytes(&path.to_string_lossy())?;
        let archive = Archive::parse(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;

        for (chunk, record) in archive.records() {
            if chunk.tag.as_bytes() != b"DATA" {
                continue;
            }

            let text = String::from_utf8_lossy(record.data);
            let label = format!("{} {}", name(&path.to_string_lossy()), record.id);

            match record.id.as_bytes() {
                b"BDEP" => buildings(&label, &text, &names)?,
                b"QDEP" => quests(&label, &text, &names)?,
                _ => continue,
            }

            found += 1;
        }
    }

    if found == 0 {
        Err("no BDEP or QDEP record in any archive given".to_owned())
    } else {
        Ok(true)
    }
}

/// Names for every id the archives can name.
///
/// Buildings: the `.cam` entry name of their `DUNT` record. Quests: `STRT QUES`,
/// keyed by the `QDEP` code (see
/// `every_quest_id_is_named_by_the_ques_string_table`).
fn collect_names(paths: &[PathBuf]) -> Result<Names, String> {
    let mut out = Names::new();

    for path in paths {
        let bytes = read_bytes(&path.to_string_lossy())?;
        let Ok(archive) = Archive::parse(&bytes) else {
            continue;
        };

        for (chunk, record) in archive.records() {
            match chunk.tag.as_bytes() {
                b"DUNT" => {
                    out.insert(record.id, record.name().into_owned());
                }
                b"STRT" if record.id == FourCc::new(b"QUES") => {
                    let Ok(table) = StringTable::parse(record.data) else {
                        continue;
                    };

                    for i in 0..table.len() {
                        if let (Some(key), Some(text)) = (table.key(i), table.get(i)) {
                            out.insert(key, text.into_owned());
                        }
                    }
                }
                _ => {}
            }
        }
    }

    Ok(out)
}

fn buildings(label: &str, text: &str, names: &Names) -> Result<(), String> {
    let rules = deps::buildings(text).map_err(|e| format!("{label}: {e}"))?;
    let conditional = rules.iter().filter(|r| r.requires.is_some()).count();

    println!("{label}: {} rules, {conditional} conditional", rules.len());

    for rule in rules.iter().filter(|r| r.requires.is_some()) {
        println!(
            "  {} needs {}",
            titled(rule.id, names),
            show(rule.requires.as_ref(), names)
        );
    }

    Ok(())
}

fn quests(label: &str, text: &str, names: &Names) -> Result<(), String> {
    let rules = deps::quests(text).map_err(|e| format!("{label}: {e}"))?;

    println!("{label}: {} quests", rules.len());

    for rule in &rules {
        println!(
            "  {:<40} {:<6} {}",
            titled(rule.id, names),
            format!("{:?}", rule.difficulty),
            show(rule.requires.as_ref(), names)
        );
    }

    Ok(())
}

/// An id, with its name where one is known.
fn titled(id: FourCc, names: &Names) -> String {
    match names.get(&id) {
        Some(title) => format!("{id} {title}"),
        None => id.to_string(),
    }
}

/// A requirement as infix, parenthesised, or `-` where there is none.
fn show(requires: Option<&Expr>, names: &Names) -> String {
    match requires {
        None => "-".to_owned(),
        Some(expr) => infix(expr, names),
    }
}

fn infix(expr: &Expr, names: &Names) -> String {
    match expr {
        Expr::Id(id) => titled(*id, names),
        Expr::Not(inner) => format!("!{}", infix(inner, names)),
        Expr::And(left, right) => {
            format!("({} && {})", infix(left, names), infix(right, names))
        }
        Expr::Or(left, right) => format!("({} || {})", infix(left, names), infix(right, names)),
    }
}
