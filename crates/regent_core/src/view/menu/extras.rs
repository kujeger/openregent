//! The extra quests' list, `APd8`, from the quest map's jewel (control
//! 4034). **Binary**: quest extras screen.
//!
//! ```text
//!  fill    each installed extra quest (a user quest): no map
//!          slot, not XQD1-3, not freestyle; sorted by title
//!          row 0 lit (list message 0x1b)
//!  a pick  GMTX 136 or 137 (quest_map::open_line), then the quest's
//!          <Short>, into DESCRIPTION
//!  LOAD    the lit row's quest, then its briefing; CANCEL the map
//! ```

use std::path::Path;

use crate::content::mods::Library;
use crate::data::fourcc::FourCc;
use crate::data::text::printf;

/// The panel.
pub const PANEL: FourCc = FourCc(*b"APd8");

/// LOAD QUEST, hot key Enter; CANCEL, Escape.
pub const LOAD: i32 = 9000;
pub const CANCEL: i32 = 9001;

/// The lit quest's description (`0x232c`).
pub const DESCRIPTION: i32 = 9004;

/// The quests' list (`0x238c`) and its scroll bar
/// ([`super::ids::SCROLL_BAR`]).
pub const LIST: i32 = 9100;

/// The title, "Downloaded Quests", and the pog, `DX00` 1000, both the
/// panel's own.
pub use super::ids::{POG, TITLE};

/// A pick's description: its line, then its `<Short>`;
/// the line alone where the `<Short>` is empty.
pub const DESCRIPTION_FORMAT: &str = "%s\r\n%s";

/// The order the list shows `titles` in: by title, byte by byte, case
/// kept (quest extras insert: an upper bound by
/// string compare, no case folding), so equal titles keep their
/// records' order. **Game**: `ui_downloaded_quests.png` lists The Wild,
/// a Workshop quest, before the shipped The Wrath of Krolm.
#[must_use]
pub fn order(titles: &[&str]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..titles.len()).collect();

    order.sort_by_key(|&i| titles[i].as_bytes());
    order
}

/// A pick's description, `line` its difficulty or victories
/// ([`open_line`](crate::view::menu::quest_map::open_line)).
#[must_use]
pub fn description(line: &str, short: &str) -> String {
    if short.is_empty() {
        line.to_owned()
    } else {
        printf(DESCRIPTION_FORMAT, &[line.into(), short.into()])
    }
}

/// What a mod folder's quest's row names it by: the title of the first
/// mod in its item folder `package`, else the folder's name, as a
/// Workshop item with a quest alone. Not the original's: its rows hold
/// the title alone.
#[must_use]
pub fn source(library: &Library, package: &Path) -> String {
    library
        .mods
        .iter()
        .find(|m| m.package == package)
        .map_or_else(
            || {
                package
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            },
            crate::content::mods::Mod::title,
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Byte order: an upper case letter before any lower case one, a
    /// tie in its records' order.
    #[test]
    fn the_list_is_sorted_by_title_with_case() {
        let titles = [
            "Vampiric Revenge",
            "The Wrath of Krolm",
            "the lower",
            "The Wild",
            "The Wild",
        ];

        assert_eq!(order(&titles), [3, 4, 1, 0, 2]);
    }

    #[test]
    fn a_description_without_a_short_is_its_line() {
        assert_eq!(description("Difficulty: Expert", ""), "Difficulty: Expert");
        assert_eq!(
            description("Difficulty: Expert", "Destroy all Altars."),
            "Difficulty: Expert\r\nDestroy all Altars."
        );
    }
}
