//! What each spawning building's script can spawn, from `MAJESTY_HD`:
//! `regent_core::gpl::spawns` over the expansion's program, joined to
//! its descriptors, then each quest's lines that differ.

use std::collections::BTreeMap;

use regent_core::content::dataset::{Bytecode, Datasets, Files, Install};
use regent_core::content::quest;
use regent_core::content::unit::Unit;
use regent_core::data::catalogue::Library;
use regent_core::data::desc::UnitKind;
use regent_core::data::fourcc::FourCc;
use regent_core::gpl::load::Program;
use regent_core::gpl::spawns::{self, Spawner};

use super::env;

/// One spawning building's line: the characters its spawn function
/// spawns, and those later stages add.
#[derive(PartialEq, Eq)]
struct Row {
    spawner: Spawner,
    codes: Vec<FourCc>,
    staged: Vec<FourCc>,
}

impl Row {
    fn line(&self, code: FourCc) -> String {
        let listed = |codes: &[FourCc]| -> String {
            let codes: Vec<String> = codes.iter().map(ToString::to_string).collect();

            codes.join(" ")
        };
        let staged = if self.staged.is_empty() {
            String::new()
        } else {
            format!("  staged: {}", listed(&self.staged))
        };

        format!(
            "{code} {:<20} {:<12} {:<17} {}{staged}",
            self.spawner.instance,
            self.spawner.kind,
            self.spawner.function,
            listed(&self.codes),
        )
    }
}

/// The spawning buildings of one set of files, by code: a spawner whose
/// name is a building descriptor's, with the character descriptors its
/// names are.
fn scan(files: &Files) -> Result<BTreeMap<FourCc, Row>, String> {
    let library = Library::from_files(&files.archives).map_err(|e| e.to_string())?;
    let catalogue = library.catalogue().map_err(|e| e.to_string())?;
    let images = files
        .programs
        .iter()
        .map(|path| std::fs::read(path).map_err(|e| format!("{}: {e}", path.display())))
        .collect::<Result<Vec<_>, String>>()?;
    let program = Program::load(&images).map_err(|e| e.to_string())?;

    // Descriptors of `kind` by HEAD name, folded.
    let of_kind = |kind: UnitKind| -> BTreeMap<String, FourCc> {
        catalogue
            .tagged(b"DUNT")
            .filter_map(|(at, _)| Unit::at(&catalogue, at).ok())
            .filter(|unit| unit.kind == Some(kind))
            .map(|unit| {
                let name = String::from_utf8_lossy(unit.desc.head.name).to_ascii_lowercase();

                (name, unit.id)
            })
            .collect()
    };
    let (buildings, characters) = (of_kind(UnitKind::Building), of_kind(UnitKind::Character));

    let rows = spawns::spawners(&program)
        .into_iter()
        .filter_map(|spawner| {
            let code = *buildings.get(&spawner.instance.to_ascii_lowercase())?;
            let characters_of = |names: &[String]| -> Vec<FourCc> {
                names
                    .iter()
                    .filter_map(|name| characters.get(name).copied())
                    .collect()
            };
            let codes = characters_of(&spawner.names);
            let staged = characters_of(&spawner.staged);

            Some((
                code,
                Row {
                    spawner,
                    codes,
                    staged,
                },
            ))
        })
        .collect();

    Ok(rows)
}

/// Each spawning building of `MAJESTY_HD`'s expansion dataset, a line:
/// its code, its `.dat` instance and `type`, its spawn function, and the
/// characters that spawns, with those later stages add. Then each quest's
/// lines that differ.
///
/// # Errors
///
/// No install, or files that will not read.
pub fn cmd_lair_spawns() -> Result<bool, String> {
    let hd = env::path(env::HD).map_err(|e| e.to_string())?;
    let install = Install::new(&hd, Bytecode::Install);
    let files = install
        .files(Datasets::Expansion)
        .map_err(|e| e.to_string())?;
    let expansion = scan(&files)?;
    let unresolved: usize = expansion.values().map(|r| r.spawner.unresolved).sum();

    for (code, row) in &expansion {
        println!("{}", row.line(*code));
    }

    println!(
        "{} spawning buildings, {unresolved} spawn calls unresolved",
        expansion.len()
    );

    for quest in quest::offered(&hd, &[]) {
        let files = install.quest(&quest).map_err(|e| e.to_string())?;

        for (code, row) in scan(&files)? {
            if expansion.get(&code) != Some(&row) {
                println!("{}: {}", quest.key, row.line(code));
            }
        }
    }

    Ok(true)
}
