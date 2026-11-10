//! A monster's panel (`AP97`): its three ratings.

use regent_core::data::text::Text;
use regent_core::view::menu::Layout;
use regent_core::view::panel::portrait::details::{self, Details, RATED};

use super::Chosen;
use crate::panel::canvas::{Align, Canvas};
use crate::panel::sheet;

/// A monster's three ratings (`MonsterController`).
pub(super) fn monster(canvas: &mut Canvas<'_>, layout: &Layout, chosen: &Chosen, text: &Text) {
    if let Some(Details::Monster(ratings)) = chosen.details {
        for (&value, (_, control)) in ratings.iter().zip(RATED) {
            if let Some(line) = details::rating(text, value) {
                sheet::fill(canvas, layout, control, &line, (None, Align::Left));
            }
        }
    }
}
