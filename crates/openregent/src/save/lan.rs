//! LAN saves as a lobby needs them: net reads no save, so the host's
//! pick is read here ([`offered`]), and a guest's copies are found and
//! kept here ([`Shelf`]).

use std::path::{Path, PathBuf};

use crate::net;

/// A save sent with no name is kept as.
const UNNAMED: &str = "LAN game";

/// The LAN save at `path`, read for the host's lobby to offer.
///
/// # Errors
///
/// A file that will not read, a game of one, or one whose game is not
/// the one its meta names.
pub fn offered(path: &Path) -> Result<net::Loaded, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let contents = super::read(path)?;
    let lan = contents
        .meta
        .lan
        .ok_or("a game of one does not load into a LAN game")?;
    let game = super::game_hash(&contents.snapshot.game, &contents.play)?;
    let setup = crate::game::record::parse(&contents.play)?.setup;

    if game == lan.game {
        Ok(net::Loaded {
            path: path.to_path_buf(),
            bytes,
            name: contents.meta.name,
            game,
            seats: lan.seats,
            setup,
        })
    } else {
        Err(format!("{}: not the game its meta names", path.display()))
    }
}

/// A guest's LAN saves, in a directory.
pub struct Shelf {
    dir: PathBuf,
}

impl Shelf {
    pub const fn new(dir: PathBuf) -> Shelf {
        Shelf { dir }
    }
}

impl net::Saves for Shelf {
    #[cfg(test)]
    fn dir(&self) -> &Path {
        &self.dir
    }

    fn find(&self, game: &str) -> Option<PathBuf> {
        super::list(&self.dir)
            .into_iter()
            .find(|l| {
                l.glance
                    .as_ref()
                    .ok()
                    .and_then(|g| g.meta.lan.as_ref())
                    .is_some_and(|lan| lan.game == game)
            })
            .map(|l| l.path)
    }

    fn keep(&self, name: &str, game: &str, bytes: &[u8]) -> Result<PathBuf, String> {
        let name = Some(name.trim())
            .filter(|n| !n.is_empty())
            .unwrap_or(UNNAMED);
        let (path, _) = super::free(&self.dir, name)?;
        let partial = path.with_extension("part");
        let shown = |e: std::io::Error| format!("{}: {e}", path.display());

        std::fs::create_dir_all(&self.dir).map_err(shown)?;
        std::fs::write(&partial, bytes).map_err(shown)?;
        std::fs::rename(&partial, &path).map_err(shown)?;

        let contents = super::read(&path)?;
        let came = super::game_hash(&contents.snapshot.game, &contents.play)?;

        if came == game {
            Ok(path)
        } else {
            let _ = std::fs::remove_file(&path);

            Err(format!("its game is {came}, offered as {game}"))
        }
    }
}
