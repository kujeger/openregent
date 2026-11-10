//! `MAJ_COMMIT`: the git commit this build is of, for the LAN build
//! check (`net::lobby::build`). `-dirty` when tracked files under `crates/`
//! differ from it; `unknown` without git.
//!
//! `MAJ_COMPILER`: a hash of the GPL compiler's source, the `regent_core`
//! library and `Cargo.lock`, keying the compiled-mods cache (`compiled`).
//! Not the commit: an edit not yet committed changes it, a commit
//! elsewhere does not, and it needs no git.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The workspace, where git is asked from.
const WORKSPACE: &str = "../..";

/// The folder of the crates.
const CRATES: &str = "..";

/// `git args` in the workspace, its output trimmed; `None` if it fails.
fn git(args: &[&str]) -> Option<String> {
    Command::new("git")
        .args(args)
        .current_dir(WORKSPACE)
        .output()
        .ok()
        .filter(|out| out.status.success())
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .map(|text| text.trim().to_owned())
}

/// A file in git's own directory, by `git rev-parse --git-path`.
fn git_path(name: &str) -> Option<PathBuf> {
    git(&["rev-parse", "--path-format=absolute", "--git-path", name]).map(PathBuf::from)
}

/// The workspace's crates and manifests: a change to any may make the
/// tree dirty.
fn sources() -> Vec<PathBuf> {
    let root = Path::new(WORKSPACE);
    let crates = std::fs::read_dir(CRATES)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.join("Cargo.toml").is_file());

    crates
        .chain(["Cargo.toml", "Cargo.lock"].map(|name| root.join(name)))
        .collect()
}

/// The library whose source [`compiler`] hashes, and the lock file.
const LIBRARY: &str = "../regent_core/src";
const LOCK: &str = "../../Cargo.lock";

/// FNV-1a, 64 bits: its offset basis and prime.
const FNV_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// Every file under `dir`, sorted.
fn files(dir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .flat_map(|path| {
            if path.is_dir() {
                files(&path)
            } else {
                vec![path]
            }
        })
        .collect();

    out.sort();
    out
}

/// [`LIBRARY`]'s files and [`LOCK`], each path and its bytes, hashed.
fn compiler() -> String {
    let fold = |hash: u64, bytes: &[u8]| {
        bytes
            .iter()
            .fold(hash, |h, b| (h ^ u64::from(*b)).wrapping_mul(FNV_PRIME))
    };
    let hash = files(Path::new(LIBRARY))
        .into_iter()
        .chain([PathBuf::from(LOCK)])
        .fold(FNV_BASIS, |hash, path| {
            let named = fold(hash, path.to_string_lossy().as_bytes());

            fold(named, &std::fs::read(&path).unwrap_or_default())
        });

    format!("{hash:016x}")
}

fn main() {
    let commit = git(&["rev-parse", "HEAD"]);
    let dirty = git(&["status", "--porcelain", "--untracked-files=no", "--", "."])
        .is_some_and(|changes| !changes.is_empty());
    let shown = match (commit, dirty) {
        (Some(commit), true) => format!("{commit}-dirty"),
        (Some(commit), false) => commit,
        (None, _) => "unknown".to_owned(),
    };

    // Again when HEAD moves (a checkout, a commit) or a source changes.
    let branch = git(&["symbolic-ref", "-q", "HEAD"]);
    let watched = ["HEAD", "packed-refs"]
        .into_iter()
        .chain(branch.as_deref())
        .filter_map(git_path)
        .chain(sources())
        .filter(|path| path.exists());

    for path in watched {
        println!("cargo::rerun-if-changed={}", path.display());
    }

    println!("cargo::rustc-env=MAJ_COMMIT={shown}");
    println!("cargo::rustc-env=MAJ_COMPILER={}", compiler());
}
