//! Game data shared by the tests in one process.
//!
//! HD's archives are read and indexed once, not once per test: many tests
//! run at once, and each reading its own copy held gigabytes. Read through
//! `regent_draw::data`, the cache the game code under test reads too, so
//! tests and games share one copy; `regent_corpus::expansion` would be a
//! second.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use regent_core::content::mods;
use regent_core::data::catalogue::Catalogue;
use regent_core::data::fourcc::FourCc;
use regent_draw::sun::model::{Model, Proxies};

use crate::files::env;

/// A building with a model: the guardhouse.
pub const GUARDHOUSE: FourCc = FourCc::new(b"ABE1");

/// HD's expansion dataset, indexed once a process. Needs `MAJESTY_HD`.
pub fn hd() -> &'static Catalogue<'static> {
    static HD: OnceLock<Catalogue<'static>> = OnceLock::new();

    HD.get_or_init(|| {
        let hd = crate::files::env::path(crate::files::env::HD).expect("MAJESTY_HD");
        let archives = crate::game::load::archives(&hd).expect("the dataset");

        regent_draw::data::catalogue(&archives).expect("the catalogue")
    })
}

/// A test's folder of its own under the system's temporary folder,
/// removed when dropped. Reads as the `Path` it is.
pub struct Scratch(tempfile::TempDir);

/// A fresh [`Scratch`], named after `name` for a reader.
pub fn scratch(name: &str) -> Scratch {
    let dir = tempfile::Builder::new()
        .prefix(&format!("openregent-{name}-"))
        .tempdir()
        .expect("scratch");

    Scratch(dir)
}

impl Scratch {
    /// The folder.
    pub fn path(&self) -> &std::path::Path {
        self.0.path()
    }
}

impl std::ops::Deref for Scratch {
    type Target = std::path::Path;

    fn deref(&self) -> &std::path::Path {
        self.0.path()
    }
}

impl AsRef<std::path::Path> for Scratch {
    fn as_ref(&self) -> &std::path::Path {
        self.0.path()
    }
}

/// The fixture mods in the repository: hand-made, no game's or
/// Workshop item's text.
///
/// ```text
///  hearth/   Hearth (XML: a Hearthguard unit), Hearth Verdicts (strings);
///            quests Second Again, and Hearth Watch (posts a Hearthguard)
///  wilds/    Thornwood Rules (loads nothing); quest Thornwood (its own
///            verdict strings, names its palace and a lair)
/// ```
const FIXTURE_MODS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/mods");

/// The fixture's mods and quests, by GUID.
pub const HEARTH: &str = "{4EA27B00-1D2C-4E3F-8A51-6B7C8D9E0F01}";
pub const VERDICTS: &str = "{4EA27B00-1D2C-4E3F-8A51-6B7C8D9E0F02}";
pub const WATCH: &str = "{4EA27B00-1D2C-4E3F-8A51-6B7C8D9E0F11}";
pub const THORNWOOD: &str = "{4EA27B00-1D2C-4E3F-8A51-6B7C8D9E0F21}";
pub const THORNWOOD_RULES: &str = "{4EA27B00-1D2C-4E3F-8A51-6B7C8D9E0F22}";
/// Second Again: its id's first word spells `QU02`, a shipped quest's key.
pub const SECOND: &str = "{32305551-0A11-4B22-8000-000000000000}";
/// The Thornwood item's folder.
pub const THORNWOOD_ITEM: &str = "wilds";

/// The SDK's quest template every fixture quest plays on, linked beside
/// each quest's manifest as [`TEMPLATE`]: a template is binary, and none
/// is hand-made.
const SDK_TEMPLATE: &str = "SpecialItemsExample/SpecialItemsExample.q";
const TEMPLATE: &str = "example.q";

/// A GPL project, its compiled image, and the image's folder.
const PROJECT: &str = "gplproj";
const IMAGE: &str = "bcd";
const DATA: &str = "Data";

/// The SDK's Adjust Guardhouse mod, linked in as an item folder.
const SDK_GUARDHOUSE: &str = "Adjust Guardhouse Mod";
const GUARDHOUSE_ITEM: &str = "guardhouse";

/// The fixture mods as one mod folder, built once a process under the
/// tests' cache at one path, so their GPL's compiled cache keys hold:
/// [`FIXTURE_MODS`] copied, each quest's template and the SDK's
/// guardhouse linked in. Needs `MAJESTY_HD`.
pub fn fixture_mods() -> &'static Path {
    static BUILT: OnceLock<PathBuf> = OnceLock::new();

    BUILT.get_or_init(|| {
        let hd = env::path(env::HD).expect("MAJESTY_HD");
        let sdk = hd.join(env::SDK_IN_HD);
        let root = env::test_cache().join("fixture-mods");

        copied(Path::new(FIXTURE_MODS), &root, &sdk.join(SDK_TEMPLATE));
        linked(&sdk.join(SDK_GUARDHOUSE), &root.join(GUARDHOUSE_ITEM));
        root
    })
}

/// The install from `MAJESTY_HD`, its mods the built-in ones and
/// [`fixture_mods`]'.
pub fn fixture_install() -> env::Install {
    let folders = crate::files::builtin::folder()
        .into_iter()
        .chain([fixture_mods().to_path_buf()])
        .collect();

    env::Install::from_env()
        .expect("MAJESTY_HD")
        .with_mod_folders(folders)
}

/// `from`'s files copied under `to`, `template` linked into each folder
/// that holds a quest manifest, and each `GPL/x.gplproj` compiled to
/// `Data/x.bcd` beside it, as the SDK's `MakeGPL.bat` does: a `.mqxml`
/// quest loads its `<Target>` only.
fn copied(from: &Path, to: &Path, template: &Path) {
    let entries: Vec<PathBuf> = std::fs::read_dir(from)
        .expect("the fixture mods")
        .map(|e| e.expect("an entry").path())
        .collect();
    let quest = entries
        .iter()
        .any(|p| p.extension().is_some_and(|e| e == mods::QUEST));

    for path in &entries {
        let at = to.join(path.file_name().expect("a name"));

        if path.is_dir() {
            copied(path, &at, template);
        } else {
            let bytes = std::fs::read(path).expect("a fixture file");

            crate::files::builtin::write(&at, &bytes).expect("written");
        }
    }

    let projects = entries
        .iter()
        .filter(|p| p.extension().is_some_and(|e| e == PROJECT));

    for path in projects {
        let project = to.join(path.file_name().expect("a name"));
        let image = regent_core::gpl::emit::compile_project(&project).expect("compiles");
        let target = to
            .with_file_name(DATA)
            .join(path.with_extension(IMAGE).file_name().expect("a name"));

        crate::files::builtin::write(&target, &image).expect("written");
    }

    if quest {
        linked(template, &to.join(TEMPLATE));
    }
}

/// `link` made a link to `target`, unless it is one already.
fn linked(target: &Path, link: &Path) {
    if std::fs::read_link(link).ok().as_deref() != Some(target) {
        drop(std::fs::remove_file(link));
        std::fs::create_dir_all(link.parent().expect("a parent")).expect("made");
        std::os::unix::fs::symlink(target, link).expect("linked");
    }
}

/// The guardhouse's model: `regent_draw`'s test fixture, our own generic
/// one.
pub fn proxies() -> Proxies {
    const FIXTURE: &[u8] = include_bytes!("../../regent_draw/tests/fixtures/guardhouse.glb");

    let model = Model::gltf(FIXTURE, None).expect("reads");

    [(GUARDHOUSE, model)].into_iter().collect()
}
