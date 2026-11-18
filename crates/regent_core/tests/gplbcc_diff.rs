//! `gpl::emit` against `Gplbcc.exe`, the SDK's compiler, on the made-up
//! projects under `tests/fixtures/gpl/`: the same bytes, or both refuse.
//!
//! ```text
//!  fixture project ──> copy under target/ ──> wine Gplbcc.exe ──> ref.bcd ─┐
//!                  └─> gpl::emit ───────────────────────────────> ours ────┴─> equal
//! ```
//!
//! Off Windows `Gplbcc.exe` runs under `wine`, in a prefix under `target/`;
//! without `wine` the tests fail, as without the install. The compiler exits
//! 0 on an error, so the output file is the verdict, and its log goes in
//! every failure.
//!
//! Needs `MAJESTY_HD`.

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use regent_core::gpl::emit;

/// The SDK's compiler, in [`common::sdk`].
const GPLBCC: &str = "Gplbcc.exe";

/// What the compiler writes, in the project's directory.
const OUTPUT: &str = "ref.bcd";

/// Longest one run may take. The first creates the wine prefix; a crash
/// may leave the debugger waiting.
const RUN_LIMIT: Duration = Duration::from_secs(120);

/// How often a run is checked for its end.
const POLL: Duration = Duration::from_millis(100);

/// Projects both compilers compile, as `dir/name.gplproj`.
const COMPILED: [&str; 4] = [
    "forms/forms.gplproj",
    "base/base.gplproj",
    "addon/addon.gplproj",
    "returns/returns.gplproj",
];

/// Projects both compilers refuse: a bare call to `ZqA` (an undeclared
/// name), a definition with no `declare`, `break` in a `while`, a
/// `do ... while` loop.
const REFUSED: [&str; 4] = [
    "refused/undeclared.gplproj",
    "refused/no_declare.gplproj",
    "refused/while_break.gplproj",
    "refused/do_while.gplproj",
];

/// One `Gplbcc.exe` at a time: they share a wine prefix.
static GPLBCC_RUN: Mutex<()> = Mutex::new(());

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/gpl")
}

/// Scratch space under `target/`.
fn scratch() -> PathBuf {
    Path::new(env!("CARGO_TARGET_TMPDIR")).join("gplbcc")
}

/// `dir` copied, flat, to a fresh `to`.
fn copy_dir(dir: &Path, to: &Path) {
    let _ = fs::remove_dir_all(to);
    fs::create_dir_all(to).expect("a scratch directory");

    for entry in fs::read_dir(dir).expect("a readable directory") {
        let path = entry.expect("an entry").path();

        if path.is_file() {
            fs::copy(&path, to.join(path.file_name().expect("a name"))).expect("copied");
        }
    }
}

/// `Gplbcc.exe` on the project `name` in `dir`: its output, if it wrote
/// one, and its log.
fn gplbcc(dir: &Path, name: &str) -> (Option<Vec<u8>>, String) {
    let _one = GPLBCC_RUN
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let exe = common::sdk().join(GPLBCC);
    let log_path = dir.join("gplbcc.log");
    let log = fs::File::create(&log_path).expect("a log file");
    let _ = fs::remove_file(dir.join(OUTPUT));

    let mut command = if cfg!(windows) {
        Command::new(&exe)
    } else {
        let prefix = scratch().join("prefix");
        let mut wine = Command::new("wine");

        fs::create_dir_all(&prefix).expect("a wine prefix");
        wine.arg(&exe)
            .env("WINEPREFIX", prefix)
            .env("WINEDEBUG", "-all");
        wine
    };

    let mut child = command
        .args(["-in", name, "-out", OUTPUT, "-stdout"])
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(log.try_clone().expect("a log handle"))
        .stderr(log)
        .spawn()
        .unwrap_or_else(|e| panic!("{}: {e}; off Windows it needs wine", exe.display()));

    let deadline = Instant::now() + RUN_LIMIT;
    while child.try_wait().expect("a child").is_none() && Instant::now() < deadline {
        std::thread::sleep(POLL);
    }

    // Past the limit: stop it, and judge what it wrote.
    let _ = child.kill();
    let _ = child.wait();

    let text = fs::read_to_string(&log_path).unwrap_or_default();
    (fs::read(dir.join(OUTPUT)).ok(), text)
}

/// The fixture `project` copied to scratch: its directory there, and the
/// project's file name.
fn staged(project: &str) -> (PathBuf, String) {
    let listed = Path::new(project);
    let parent = listed.parent().expect("dir/name.gplproj");
    let at = scratch().join("projects").join(parent);

    copy_dir(&fixtures().join(parent), &at);

    let name = listed.file_name().expect("a name").to_string_lossy();
    (at, name.into_owned())
}

/// The first offset two images differ at, for a failure.
fn first_difference(ours: &[u8], theirs: &[u8]) -> usize {
    ours.iter()
        .zip(theirs)
        .position(|(a, b)| a != b)
        .unwrap_or(ours.len().min(theirs.len()))
}

/// The harness runs the compiler as `MakeGPL.bat` does: the SDK's Adjust
/// Guardhouse project comes out as its shipped image.
#[test]
fn gplbcc_reproduces_a_shipped_image() {
    let mod_dir = common::sdk().join("Adjust Guardhouse Mod");
    let at = scratch().join("projects/guardhouse");

    copy_dir(&mod_dir.join("GPL"), &at);

    let (theirs, log) = gplbcc(&at, "AdjustGuardhouse.gplproj");
    let shipped = fs::read(mod_dir.join("Data/AdjustGuardhouse.bcd")).expect("shipped");

    assert!(theirs == Some(shipped), "not the shipped image:\n{log}");
}

/// Every fixture project compiles to the same bytes with both compilers.
#[test]
fn every_fixture_compiles_as_gplbcc_does() {
    for project in COMPILED {
        let (dir, name) = staged(project);
        let (theirs, log) = gplbcc(&dir, &name);
        let theirs =
            theirs.unwrap_or_else(|| panic!("{project}: Gplbcc.exe wrote nothing:\n{log}"));
        let ours = emit::compile_project(&dir.join(&name))
            .unwrap_or_else(|e| panic!("{project}: we refuse it: {e}"));

        assert!(
            ours == theirs,
            "{project}: {} bytes against Gplbcc.exe's {}, first differing at {}",
            ours.len(),
            theirs.len(),
            first_difference(&ours, &theirs)
        );
    }
}

/// What `Gplbcc.exe` refuses, ours refuses too.
#[test]
fn both_compilers_refuse_the_same_sources() {
    for project in REFUSED {
        let (dir, name) = staged(project);
        let (theirs, log) = gplbcc(&dir, &name);
        let ours = emit::compile_project(&dir.join(&name));

        assert!(
            theirs.is_none(),
            "{project}: Gplbcc.exe compiled it:\n{log}"
        );
        assert!(
            ours.is_err(),
            "{project}: we compile it; Gplbcc.exe:\n{log}"
        );
    }
}
