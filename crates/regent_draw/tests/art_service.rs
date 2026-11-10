//! The art service's callers do not make art themselves: grep checks
//! over the workspace's drawing code (`docs/openregent/art-service.md`).

use std::path::{Path, PathBuf};

/// The directory of the crates, `crates/`.
fn crates() -> &'static Path {
    let here = Path::new(env!("CARGO_MANIFEST_DIR"));

    here.parent().unwrap_or(here)
}

/// Every source file under crate `name`'s `src` but those under `skip`,
/// and its text without its test modules.
fn sources(name: &str, skip: Option<&str>) -> Vec<(PathBuf, String)> {
    let src = crates().join(name).join("src");
    let skipped = skip.map(|s| src.join(s));
    let mut files = vec![src];
    let mut read = Vec::new();

    while let Some(path) = files.pop() {
        if path.is_dir() && Some(&path) != skipped.as_ref() {
            let entries = std::fs::read_dir(&path).expect("a source directory");

            files.extend(entries.map(|e| e.expect("an entry").path()));
        } else if path.extension().is_some_and(|e| e == "rs") {
            let text = std::fs::read_to_string(&path).expect("a source file");

            read.push((path, untested(&text)));
        }
    }

    read
}

/// `text` without its `#[cfg(test)]` modules: a test's fixtures may make
/// art, the game may not. Braces are counted per character: an unmatched
/// brace in a test's string literal moves the cut's end.
fn untested(text: &str) -> String {
    let mut kept = String::new();
    let mut lines = text.lines().peekable();

    while let Some(line) = lines.next() {
        let opens_test = line.trim() == "#[cfg(test)]"
            && lines
                .peek()
                .is_some_and(|next| next.trim_start().starts_with("mod ") && next.ends_with('{'));

        if opens_test {
            // Skip the module: from its `mod … {` to the brace that closes it.
            let mut depth = 0usize;

            for inner in lines.by_ref() {
                depth =
                    (depth + inner.matches('{').count()).saturating_sub(inner.matches('}').count());

                if depth == 0 {
                    break;
                }
            }
        } else {
            kept.push_str(line);
            kept.push('\n');
        }
    }

    kept
}

/// openregent's source files.
fn game() -> Vec<(PathBuf, String)> {
    let read = sources("openregent", None);

    assert!(read.len() > 50, "{} files read", read.len());
    read
}

/// Only the art service makes interface art into textures, upscales
/// or shrinks it, or holds the textures: no other source file of
/// openregent, `regent_viewer` or `regent_draw` names those calls. openregent's save
/// list thumbnails, live minimap and tracking window are textures of
/// their own, drawn from a save or the world, not art; so are a video's
/// frames.
#[test]
fn only_the_art_service_loads_interface_art() {
    const CALLS: [&str; 6] = [
        "load_texture",
        "TextureHandle",
        "shrink::shrink",
        "scale::xbrz",
        "disk::",
        "xbrz::",
    ];
    // Under openregent/src.
    const OWN: [&str; 4] = [
        "save/dialog.rs",
        "ui/minimap.rs",
        "ui/tracking.rs",
        "video.rs",
    ];
    const OWN_CALLS: [&str; 2] = ["load_texture", "TextureHandle"];

    let game_src = crates().join("openregent").join("src");
    let files = game()
        .into_iter()
        .chain(sources("regent_viewer", None))
        .chain(sources("regent_draw", Some("art")));

    for (path, text) in files {
        let own = path
            .strip_prefix(&game_src)
            .is_ok_and(|at| OWN.iter().any(|o| at == Path::new(o)));
        let banned = CALLS
            .iter()
            .filter(|c| !(own && OWN_CALLS.contains(c)))
            .find(|c| text.contains(**c));

        assert_eq!(banned, None, "{}", path.display());
    }
}

/// Only the art service makes openregent's world sprites into Bevy assets
/// or keeps them: index and palette textures, bodies, bands, palette
/// variants, the cursor's images. No openregent source file makes a
/// frame's materials (`map::cel::cel`, `Drawing::layers`) or a texture
/// (`regent_draw::texture`), adds an image, body or band, or paints the
/// map itself (`impl Paint`). The viewer does, with its own `Fresh`.
/// The tracking window adds the image its camera draws into, no art.
#[test]
fn only_the_art_service_makes_world_sprites() {
    const CALLS: [&str; 8] = [
        "Palettes",
        "cel::cel",
        "Drawing::layers",
        "texture::",
        "images.add",
        "bodies.add",
        "bands.add",
        "impl Paint",
    ];

    const TARGET: (&str, &str) = ("tracking.rs", "images.add");

    for (path, text) in game() {
        let target = path.ends_with(TARGET.0);
        let banned = CALLS
            .iter()
            .filter(|c| !(target && **c == TARGET.1))
            .find(|c| text.contains(**c));

        assert_eq!(banned, None, "{}", path.display());
    }
}
