//! The row under the main menu: HD's mods row (`menu::row`), holding
//! openregent's three buttons where HD holds two.
//!
//! ```text
//!  HD    @(  [ CHANGE MODS ]  )@@(  [ ACTIVATE MODS ]  )@     two 281 wide
//!  here @( [CHANGE MODS] )@ @( [ PROFILE ] )@ @( [BOOK OF LORE] )@   235
//! ```
//!
//! Each surround is `INx2` 1011's scroll ends, its middle cut out; each
//! button the name prompt's `DEM2` 1005 (189 × 25), HD's 235-wide
//! `DEM2` 1001 leaving no room for three. Spacing, font and colours are
//! HD's.

use bevy_egui::egui::{self, pos2};
use regent_core::data::fourcc::FourCc;
use regent_core::data::smnu::{self, Rect};
use regent_core::view::menu::{Layout, row};

use super::art::{Pack, Pic};
use super::fit;
use super::page::Press;
use super::paint::{Able, Canvas};

/// BOOK OF LORE pressed: `ui::lore::show` opens or closes its window;
/// and the entry `--menu lore:CODE` opens it on.
#[derive(bevy::prelude::Resource, Default)]
pub struct Lore(pub bool, pub Option<FourCc>);

/// A slot's surround width: `INx2` 1011 less its middle.
const SLOT: i32 = 235;

/// The button in each slot.
const BUTTON: (FourCc, i32) = (FourCc(*b"DEM2"), 1005);

/// PROFILE's and BOOK OF LORE's control ids: openregent's own, in no
/// panel.
const PROFILE: i32 = 9100;
const LORE: i32 = 9101;

/// The row's buttons, left to right.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Slot {
    Mods,
    Profile,
    Lore,
}

impl Slot {
    const ALL: [Slot; 3] = [Slot::Mods, Slot::Profile, Slot::Lore];

    /// Its control id: CHANGE MODS' for Mods, the others openregent's own.
    const fn id(self) -> i32 {
        match self {
            Slot::Mods => row::CHANGE_MODS,
            Slot::Profile => PROFILE,
            Slot::Lore => LORE,
        }
    }

    const fn press(self) -> Press {
        match self {
            Slot::Mods => Press::Mods,
            Slot::Profile => Press::Profiles,
            Slot::Lore => Press::Lore,
        }
    }

    /// Its label; Mods' is HD's CHANGE MODS text.
    fn label(self, hd: Option<&str>) -> String {
        match self {
            Slot::Mods => hd.unwrap_or("CHANGE MODS").to_owned(),
            Slot::Profile => "PROFILE".to_owned(),
            Slot::Lore => "BOOK OF LORE".to_owned(),
        }
    }
}

/// `row` drawn under the main menu; the button pressed, if any.
pub(super) fn draw(canvas: &mut Canvas, row: &Layout) -> Option<Press> {
    // HD's surrounds, left to right: the panel lists the right one first.
    let mut surrounds: Vec<Rect> = row
        .controls
        .iter()
        .filter(|c| c.art == Some(row::SURROUND))
        .filter_map(|c| c.rect)
        .collect();

    surrounds.sort_by_key(|r| r.x);

    // The frame pieces, the strip last as HD shows it (`row::STRIP`);
    // not HD's surrounds and buttons.
    let pieces = row
        .controls
        .iter()
        .filter(|c| c.id.is_none() && c.art != Some(row::SURROUND));
    let (strips, corners): (Vec<_>, Vec<_>) = pieces.partition(|c| c.art == Some(row::STRIP));

    for c in corners.iter().chain(&strips) {
        if let (Some((record, set)), Some(r)) = (c.art, c.rect) {
            let (pic, rect) = (Pic::of(record, set), fit::rect(r));

            if c.style & smnu::style::TILED == 0 {
                canvas.picture(Pack::Base, pic, rect.min);
            } else {
                canvas.tiled(Pack::Base, pic, rect);
            }
        }
    }

    let hd = row.control(row::CHANGE_MODS)?;
    let strip = strips.first()?.rect?;
    let (first, second) = (surrounds.first()?, surrounds.get(1)?);
    let button = Pic::of(BUTTON.0, BUTTON.1);
    let size = canvas.size(Pack::Base, button)?;

    // HD's gap between its surrounds, the three centred on the strip.
    let gap = second.x - (first.x + first.width);
    let count = i32::try_from(Slot::ALL.len()).unwrap_or_default();
    let left = strip.x + (strip.width - count * SLOT - (count - 1) * gap) / 2;

    #[allow(clippy::cast_possible_truncation, reason = "a picture's pixels")]
    let (width, height) = (size.x as i32, size.y as i32);

    Slot::ALL
        .into_iter()
        .zip((0..).map(|n| left + n * (SLOT + gap)))
        .fold(None, |pressed, (slot, x)| {
            let around = Rect {
                x,
                width: SLOT,
                ..*first
            };

            surround(canvas, around);

            let mut c = hd.clone();

            c.id = Some(slot.id());
            c.art = Some(BUTTON);
            c.text = Some(slot.label(hd.text.as_deref()));
            c.hotkey = None;
            c.rect = Some(Rect {
                x: x + (SLOT - width) / 2,
                y: around.y + (around.height - height) / 2,
                width,
                height,
            });

            let now = canvas.button(Pack::Base, &c, Able::Enabled);

            pressed.or(now.then_some(slot.press()))
        })
}

/// `INx2` 1011 over `rect`, narrower than it: its left half from the
/// left edge, its right half from the right.
///
/// ```text
///  @(────────────────────────)@   281
///  @(──────────┊─────────)@       rect.width, the middle cut
/// ```
fn surround(canvas: &mut Canvas, rect: Rect) {
    let pic = Pic::of(row::SURROUND.0, row::SURROUND.1);
    let rect = fit::rect(rect);

    if let Some(size) = canvas.size(Pack::Base, pic) {
        let middle = rect.center().x;
        let halves = [
            (rect.min.x, rect.min.x..=middle),
            (rect.max.x - size.x, middle..=rect.max.x),
        ];

        for (at, span) in halves {
            canvas.clip = Some(egui::Rect::from_x_y_ranges(span, rect.y_range()));
            canvas.picture(Pack::Base, pic, pos2(at, rect.min.y));
        }

        canvas.clip = None;
    }
}
