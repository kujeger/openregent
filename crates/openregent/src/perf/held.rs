//! The art service's held pictures, written out for `regent_cli atlas`
//! (`MAJ_PERF_HELD`, with `MAJ_PERF`): every [`WINDOW`] drawn frames the
//! file is written anew, so the last holds what the run made.
//!
//! ```text
//!  MAJ_PERF_HELD=<file>
//!  piece  <code> <set> <slot> <frame or ->   a piece's picture (art::world::Piece)
//!  thing  <code> <set> <slot> -              a scatter thing's (map::things)
//!  tile   <archive> <index>                  a tile made into materials
//! ```
//!
//! Tab separated. A scatter thing's picture cycles, so it holds no frame.

use std::collections::BTreeSet;
use std::fmt::Write as _;

use bevy::log::warn;
use bevy::prelude::*;
use regent_core::data::imag::set_number;
use regent_draw::art::world::Sprites;
use regent_draw::data::Corpus;
use regent_draw::map::{Maps, SCATTER};

use super::WINDOW;

/// The environment variable naming the file, e.g.
/// `MAJ_PERF_HELD=held.tsv`.
pub const ENV: &str = "MAJ_PERF_HELD";

/// With [`ENV`] set, the dump put in `app`.
pub fn install(app: &mut App) {
    if std::env::var_os(ENV).is_some() {
        app.add_systems(Last, held.before(super::ended));
    }
}

/// Each [`WINDOW`] frames, the held pictures written to [`ENV`]'s file.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
fn held(
    sprites: Option<Res<Sprites>>,
    corpus: Option<Res<Corpus>>,
    maps: Option<Res<Maps>>,
    mut frames: Local<u32>,
) {
    *frames += 1;

    if frames.is_multiple_of(WINDOW)
        && let (Some(sprites), Some(corpus), Some(path)) = (sprites, corpus, std::env::var_os(ENV))
        && let Err(e) = std::fs::write(&path, listed(&sprites, &corpus, maps.as_deref()))
    {
        warn!("{}: {e}", path.display());
    }
}

/// The dump's text: pieces, scatter things, tiles.
fn listed(sprites: &Sprites, corpus: &Corpus, maps: Option<&Maps>) -> String {
    let (pieces, tiles) = sprites.held(corpus.source());
    let mut out = String::new();

    for p in pieces {
        let still = p.still.map_or_else(|| "-".to_owned(), |f| f.to_string());

        let _ = writeln!(out, "piece\t{}\t{}\t{}\t{still}", p.code, p.set, p.slot);
    }

    // As `map::things` keys them: the unit's drawn set, the saved slot.
    let things = maps
        .and_then(Maps::current)
        .map(|l| l.source.objects[SCATTER].as_slice())
        .unwrap_or_default();

    let drawn: BTreeSet<(String, u16, u16)> = things
        .iter()
        .filter(|i| !i.held)
        .filter_map(|i| {
            let set = corpus.unit(i.id)?.drawn(set_number(i.set))?;

            Some((i.id.to_string(), set, i.drawing.0))
        })
        .collect();

    for (code, set, slot) in drawn {
        let _ = writeln!(out, "thing\t{code}\t{set}\t{slot}\t-");
    }

    for (archive, index) in tiles {
        let name = corpus.catalogue.archives.get(archive).map_or("?", |a| a.0);

        let _ = writeln!(out, "tile\t{name}\t{index}");
    }

    out
}
