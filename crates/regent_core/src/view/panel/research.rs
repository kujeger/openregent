//! A researcher's research page: which panel, and which of its controls
//! each research row owns. **Binary** unless marked.
//!
//! ```text
//!  8851 on a building panel ── its controller opens its page (PAGES)
//!  ResearchController ── research buttons:
//!    each control of id 5000 to 5099, a research id, in panel order
//!      └─ the research button: shown or hidden together
//!           id          the button: picture, label, hot key, tip (SMNU's)
//!           id + 1000   the price box
//!           id + 500    the icon
//! ```
//!
//! Every other control is the panel's own and always shows. `MX03`'s
//! icons, 5500 to 5505, are no row's id + 500 (its rows are 5061 to
//! 5066), so the Magic Bazaar shows all six, whatever its rows show.

use std::ops::RangeInclusive;

use crate::data::fourcc::FourCc;
use crate::view::menu::{Layout, ShownControl};

/// Each researcher's research page, by its building panel (`DialogID`):
/// blacksmith, fairgrounds, guardhouse, library, marketplace, trading
/// post, magic bazaar (each controller's method, control 8851).
pub const PAGES: [(FourCc, FourCc); 7] = [
    (FourCc(*b"AP02"), FourCc(*b"AP99")),
    (FourCc(*b"AP08"), FourCc(*b"APa0")),
    (FourCc(*b"AP17"), FourCc(*b"APa1")),
    (FourCc(*b"AP26"), FourCc(*b"APa2")),
    (FourCc(*b"AP31"), FourCc(*b"APa3")),
    (FourCc(*b"AP51"), FourCc(*b"APa4")),
    (FourCc(*b"MX02"), FourCc(*b"MX03")),
];

/// The control ids research buttons hands to a row: `0x1388` to
/// `0x13eb`.
pub const BUTTONS: RangeInclusive<i32> = 5000..=5099;

/// A row's price box and icon, from its button's id.
pub const PRICE_STEP: i32 = 1000;
pub const ICON_STEP: i32 = 500;

/// The research page of building panel `dialog`.
#[must_use]
pub fn page(dialog: FourCc) -> Option<FourCc> {
    PAGES.iter().find(|(d, _)| *d == dialog).map(|(_, p)| *p)
}

/// One research row of a page: the button, whose id is the research's,
/// and the price box and icon it shows and hides with it, where the
/// panel has them.
#[derive(Clone, Copy, Debug)]
pub struct Row<'a> {
    pub button: &'a ShownControl,
    pub price: Option<&'a ShownControl>,
    pub icon: Option<&'a ShownControl>,
}

impl Row<'_> {
    /// The research id: the button's.
    #[must_use]
    pub fn id(&self) -> u32 {
        self.button
            .id
            .and_then(|i| u32::try_from(i).ok())
            .unwrap_or_default()
    }

    /// The control ids it owns.
    pub fn controls(&self) -> impl Iterator<Item = i32> + '_ {
        [Some(self.button), self.price, self.icon]
            .into_iter()
            .flatten()
            .filter_map(|c| c.id)
    }
}

/// The rows of research page `layout`, in panel order.
#[must_use]
pub fn rows(layout: &Layout) -> Vec<Row<'_>> {
    layout
        .controls
        .iter()
        .filter_map(|c| c.id.filter(|i| BUTTONS.contains(i)).map(|i| (c, i)))
        .map(|(button, id)| Row {
            button,
            price: layout.control(id + PRICE_STEP),
            icon: layout.control(id + ICON_STEP),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_panel_names_its_research_page() {
        assert_eq!(page(FourCc(*b"AP08")), Some(FourCc(*b"APa0")));
        assert_eq!(page(FourCc(*b"MX02")), Some(FourCc(*b"MX03")));
        assert_eq!(page(FourCc(*b"AP39")), None);
    }
}
