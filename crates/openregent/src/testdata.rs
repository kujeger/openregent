//! Game data shared by the tests in one process.
//!
//! HD's archives are read and indexed once, not once per test: many tests
//! run at once, and each reading its own copy held gigabytes. Read through
//! `regent_draw::data`, the cache the game code under test reads too, so
//! tests and games share one copy; `regent_corpus::expansion` would be a
//! second.

use std::sync::OnceLock;

use regent_core::data::catalogue::Catalogue;
use regent_core::data::fourcc::FourCc;
use regent_draw::sun::model::{Model, Proxies};

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

/// The guardhouse's model: `regent_draw`'s test fixture, our own generic
/// one.
pub fn proxies() -> Proxies {
    const FIXTURE: &[u8] = include_bytes!("../../regent_draw/tests/fixtures/guardhouse.glb");

    let model = Model::gltf(FIXTURE, None).expect("reads");

    [(GUARDHOUSE, model)].into_iter().collect()
}
