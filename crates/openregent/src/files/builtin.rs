//! Built-in mods: mod folders inside the binary, unpacked at start to
//! a cache folder that is searched before every other mod folder.
//!
//! ```text
//!  crates/openregent/mods/<name>/     manifest and any mod files
//!     │ include_bytes!
//!     v
//!  BUILT_IN ──unpack──> $XDG_CACHE_HOME/openregent/builtin-mods/<name>/
//!                         GPL projects, if any, compiled to their <Target>
//!                         └─> env::mod_folders' first folder
//! ```
//!
//! Unpacked, not read from memory: the scan, the composition and the art
//! sources read mods from disk. A GPL project, when present, is compiled
//! at unpack by `gpl::emit`.
//!
//! A GUID a mod folder also holds: the built-in's, as the first folder's
//! wins (`content::mods::scan`). A recording naming it replays the files
//! this binary carries.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use bevy::log::warn;
use regent_core::content::mods::{self, MANIFEST, SOURCE_MANIFEST};
use regent_core::data::guid::Guid;
use regent_core::gpl::emit;

/// The unpack folder under openregent's cache directory.
const UNPACKED: &str = "builtin-mods";

/// One built-in mod: a folder of files.
pub struct Builtin {
    /// Its folder, under `mods/` here and under the unpack folder.
    pub name: &'static str,
    /// Each file, its path in the folder.
    files: &'static [(&'static str, &'static [u8])],
    /// Each GPL project, and the `.bcd` it compiles to: the manifest's
    /// `<Target>`.
    programs: &'static [(&'static str, &'static str)],
}

/// A file of `mods/<dir>/`, its path and bytes.
macro_rules! embed {
    ($dir:literal, $path:literal) => {
        (
            $path,
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/mods/",
                $dir,
                "/",
                $path
            )),
        )
    };
}

/// The built-in mods.
pub const BUILT_IN: [Builtin; 2] = [
    Builtin {
        name: "house_mend",
        files: &[
            embed!("house_mend", "mod.toml"),
            embed!("house_mend", "README.md"),
        ],
        programs: &[],
    },
    // Its `[[script]]` compiles at the scan, so `programs` stays empty.
    Builtin {
        name: "bugfixes",
        files: &[
            embed!("bugfixes", "mod.toml"),
            embed!("bugfixes", "README.md"),
            embed!("bugfixes", "GPL/gold.gpl"),
        ],
        programs: &[],
    },
];

impl Builtin {
    /// Its files written to `dir`, then its programs compiled there. A
    /// file already as written is left.
    ///
    /// # Errors
    ///
    /// A file that will not write, or a program that will not compile.
    fn unpack(&self, dir: &Path) -> Result<(), String> {
        for (path, bytes) in self.files {
            write(&dir.join(path), bytes)?;
        }

        // A removed manifest must not shadow its replacement in the cache.
        let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        for entry in entries {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            let manifest = path.extension().is_some_and(|e| e == MANIFEST)
                || path
                    .file_name()
                    .is_some_and(|n| n.eq_ignore_ascii_case(SOURCE_MANIFEST));
            let kept = self.files.iter().any(|(name, _)| dir.join(name) == path);
            if path.is_file() && manifest && !kept {
                std::fs::remove_file(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            }
        }

        for (project, target) in self.programs {
            let compiled = emit::compile_project(&dir.join(project))
                .map_err(|e| format!("{}: {e}", self.name))?;

            write(&dir.join(target), &compiled)?;
        }

        Ok(())
    }

    /// The GUIDs its manifests name.
    fn guids(&self) -> impl Iterator<Item = Guid> {
        self.files
            .iter()
            .filter(|(path, _)| {
                Path::new(path).extension().is_some_and(|e| e == MANIFEST)
                    || *path == SOURCE_MANIFEST
            })
            .flat_map(|(path, bytes)| {
                let text = String::from_utf8_lossy(bytes);
                if *path == SOURCE_MANIFEST {
                    mods::parse_toml(&text, Path::new(path), Path::new(""))
                        .into_iter()
                        .collect()
                } else {
                    mods::parse(&text, Path::new(path), Path::new(""))
                }
            })
            .map(|m| m.guid)
    }
}

/// Whether `guid` is a built-in mod's: the manifests read once a run, as
/// the mods page asks each row each frame.
pub fn holds(guid: Guid) -> bool {
    static GUIDS: OnceLock<Vec<Guid>> = OnceLock::new();

    GUIDS
        .get_or_init(|| BUILT_IN.iter().flat_map(Builtin::guids).collect())
        .contains(&guid)
}

/// Every built-in mod unpacked under `root`, a folder each; any other
/// folder there removed, a mod an older build carried.
///
/// # Errors
///
/// As [`Builtin::unpack`]; a mod that fails is removed, so no folder
/// holds it.
pub fn unpack(root: &Path) -> Result<(), String> {
    let stale = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| !BUILT_IN.iter().any(|b| e.file_name() == b.name));

    for entry in stale {
        drop(std::fs::remove_dir_all(entry.path()));
    }

    BUILT_IN.iter().try_for_each(|b| {
        let dir = root.join(b.name);

        b.unpack(&dir)
            .inspect_err(|_| drop(std::fs::remove_dir_all(&dir)))
    })
}

/// The built-in mods' folder, unpacked once a run: under the cache
/// directory, the tests' under the system's temporary folder. `None` with
/// no cache directory or on a failure, said.
pub fn folder() -> Option<PathBuf> {
    static UNPACKED_AT: OnceLock<Option<PathBuf>> = OnceLock::new();

    UNPACKED_AT
        .get_or_init(|| {
            let root = if cfg!(test) {
                Some(crate::files::env::test_cache().join(UNPACKED))
            } else {
                dirs::cache_dir().map(|d| d.join(crate::files::env::OWN).join(UNPACKED))
            }?;

            unpack(&root)
                .map_err(|e| warn!("built-in mods not unpacked: {e}"))
                .ok()
                .map(|()| root)
        })
        .clone()
}

/// `bytes` at `path`, its folders made; written aside and renamed in, so
/// another process reading it sees the old file or the new. Left alone
/// when it holds them already.
pub fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let said = |e: std::io::Error| format!("{}: {e}", path.display());

    if std::fs::read(path).is_ok_and(|old| old == bytes) {
        Ok(())
    } else {
        let aside = path.with_extension(format!("part{}", std::process::id()));

        std::fs::create_dir_all(path.parent().unwrap_or(Path::new("."))).map_err(said)?;
        std::fs::write(&aside, bytes).map_err(said)?;
        std::fs::rename(&aside, path).map_err(said)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh scratch folder of its own.
    fn scratch(name: &str) -> crate::testdata::Scratch {
        crate::testdata::scratch(&format!("builtin-{name}"))
    }

    /// The house mod loads its inline flag patch without GPL or XML.
    #[test]
    fn house_mod_loads_flag_patch() {
        let root = scratch("compiles");

        unpack(&root).expect("unpacked");

        let library = mods::scan(&[root.to_path_buf()]);

        assert_eq!(library.mods.len(), BUILT_IN.len());

        let house = Guid(0xA2A4_8A02_2BCE_42B5_8D2F_22B5_9DB3_B720);
        let m = library
            .mods
            .iter()
            .find(|m| m.guid == house)
            .expect("house_mend");

        assert!(holds(m.guid));
        assert!(holds(Guid(0x07C6_6EB7_1379_4B7A_8328_CDA9_1B2A_D5DB)));
        assert_eq!(m.load.programs.len(), 0);
        assert_eq!(m.load.descriptions.len(), 0);
        assert!(m.inline_patch.is_some());
    }

    #[test]
    fn unpack_removes_old_manifest() {
        let root = scratch("old_manifest");
        let dir = root.join("house_mend");
        std::fs::create_dir_all(&dir).expect("made");
        let old = dir.join("HouseMend.mmxml");
        std::fs::write(&old, "<Majesty/>").expect("written");

        unpack(&root).expect("unpacked");

        assert!(!old.exists());
        assert!(dir.join("mod.toml").exists());
    }

    /// A folder no built-in mod has is removed; others' files stay.
    #[test]
    fn a_stale_builtin_folder_is_removed() {
        let root = scratch("stale");
        let old = root.join("gone");

        std::fs::create_dir_all(&old).expect("made");
        std::fs::write(old.join("Old.mmxml"), "<Majesty/>").expect("written");
        unpack(&root).expect("unpacked");

        assert!(!old.exists());
        assert!(root.join(BUILT_IN[0].name).is_dir());
    }
}
