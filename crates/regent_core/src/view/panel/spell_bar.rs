//! The spell bar, `AP92` (`FastBar`): its buttons in order, each spell's
//! full-size icon, and how a one-slot icon is drawn in the bar's states.
//!
//! The base game's bar holds 22 buttons, 25 × 25, its icons `INTN` 1001 to
//! 1022 (24 × 24). The expansion's squeezes 28 into the same 590 pixels:
//! `IX23` 1001 to 1028, 20 wide. `INTN` ships in both datasets; the
//! Sorcerer's Abode's six spells have no `INTN` set, but `IX02` 1007 to
//! 1012 ("research icons", 25 × 25, one slot) are their `IX23` pictures
//! unsqueezed (`regent_core/tests/spell_bar.rs`).
//!
//! ```text
//!  spell          base AP92      expansion AP92    full size here
//!  Sp10 … Sp31    INTN 1001+     IX23 1001+        INTN, the same set
//!  Sp32 … Sp37    none           IX23 1023 … 1028  IX02 1007 … 1012
//! ```

use crate::data::fourcc::FourCc;

use crate::data::catalogue::Catalogue;
use crate::data::payload::Rgb;
use crate::view::menu::Release;

/// The base game's spell icons: 12 slots each, 24 × 24.
pub const ICONS: FourCc = FourCc::new(b"INTN");

/// The expansion's research icons, among them the Abode spells' at full
/// size: one slot each, 25 × 25.
pub const ABODE_ICONS: FourCc = FourCc::new(b"IX02");

/// The expansion bar's squeezed icons, 20 wide.
pub const SQUEEZED: FourCc = FourCc::new(b"IX23");

/// The Rage of Krolm's icon, for openregent's bar button (an enhancement):
/// its heroes' effect `QRa1`'s (`portrait::enchant`), one slot, 24 × 24.
pub const RAGE_ICON: (FourCc, i32) = (FourCc::new(b"INTn"), 1024);

/// One spell's button: its spell, its `AP92` control (whose tooltip names
/// it) and its full-size icon's set in `record`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Button {
    pub spell: FourCc,
    pub control: i32,
    pub record: FourCc,
    pub set: i32,
}

const fn button(spell: [u8; 4], control: i32, record: FourCc, set: i32) -> Button {
    Button {
        spell: FourCc(spell),
        control,
        record,
        set,
    }
}

/// Every button, in both datasets' `AP92` order, left to right; the
/// Abode's six last, the expansion's alone. **Inferred**: the two
/// Healings, 4401 and 4416, by their places among Agrela's and Fervus's
/// spells.
pub const BUTTONS: [Button; 28] = [
    button(*b"Sp10", 4417, ICONS, 1001),
    button(*b"Sp11", 4418, ICONS, 1002),
    button(*b"Sp12", 4419, ICONS, 1003),
    button(*b"Sp13", 4420, ICONS, 1004),
    button(*b"Sp14", 4421, ICONS, 1005),
    button(*b"Sp15", 4422, ICONS, 1006),
    button(*b"Sp16", 4401, ICONS, 1007),
    button(*b"Sp17", 4411, ICONS, 1008),
    button(*b"Sp18", 4412, ICONS, 1009),
    button(*b"Sp19", 4413, ICONS, 1010),
    button(*b"Sp21", 4415, ICONS, 1012),
    button(*b"Sp20", 4414, ICONS, 1011),
    button(*b"Sp26", 4405, ICONS, 1017),
    button(*b"Sp25", 4404, ICONS, 1016),
    button(*b"Sp27", 4406, ICONS, 1018),
    button(*b"Sp22", 4416, ICONS, 1013),
    button(*b"Sp24", 4403, ICONS, 1015),
    button(*b"Sp23", 4402, ICONS, 1014),
    button(*b"Sp30", 4409, ICONS, 1021),
    button(*b"Sp31", 4410, ICONS, 1022),
    button(*b"Sp28", 4407, ICONS, 1019),
    button(*b"Sp29", 4408, ICONS, 1020),
    button(*b"Sp32", 4423, ABODE_ICONS, 1008),
    button(*b"Sp33", 4424, ABODE_ICONS, 1011),
    button(*b"Sp34", 4425, ABODE_ICONS, 1010),
    button(*b"Sp35", 4426, ABODE_ICONS, 1007),
    button(*b"Sp36", 4427, ABODE_ICONS, 1012),
    button(*b"Sp37", 4428, ABODE_ICONS, 1009),
];

/// The Abode's rest in the expansion's `AP92`: bar 9500, `INBd` 1000 as
/// the Abode panel's 4500, one bar over its six buttons, (464, 4) 124 ×
/// 26. **Binary**: `FastBar` shows it while the Abode rests, filled as the
/// panel's bar, `{7, clock - ASS:0, 0, ASP:0}`, and hides it with no Abode
/// or `ASS:0` 0 (`World::abode_rest`).
pub const RESTING_BAR: i32 = 9500;

/// How many of [`BUTTONS`] the base game's bar holds: all but the Abode's.
pub const BASE_BUTTONS: usize = 22;

/// The buttons a game on `release` shows.
#[must_use]
pub fn shown(release: Release) -> &'static [Button] {
    match release {
        Release::Base => &BUTTONS[..BASE_BUTTONS],
        Release::Expansion => &BUTTONS,
    }
}

/// The Sorcerer's Abode's spell page, `MX07`: a `SpellsController`,
/// as `AP66` to `AP72`, opened
/// by the Abode's 8851.
pub const ABODE_PAGE: FourCc = FourCc::new(b"MX07");

/// Each spell-giving guild's or temple's page, by code prefix, opened by
/// its SPELLS (8009). **Binary**: `Base_GuildController` opens the
/// building's own page, not the spell bar.
pub const TEMPLE_PAGES: [([u8; 3], FourCc); 7] = [
    (*b"ABY", FourCc::new(b"AP66")),
    (*b"ABO", FourCc::new(b"AP67")),
    (*b"ABP", FourCc::new(b"AP68")),
    (*b"ABQ", FourCc::new(b"AP69")),
    (*b"ABT", FourCc::new(b"AP70")),
    (*b"ABU", FourCc::new(b"AP71")),
    (*b"ABR", FourCc::new(b"AP72")),
];

/// The spell page of a building of `code`: its temple's, the Abode's
/// [`ABODE_PAGE`], else none.
#[must_use]
pub fn spell_page(code: FourCc) -> Option<FourCc> {
    let [a, b, c, _] = code.0;

    if [a, b, c] == *b"ABq" {
        Some(ABODE_PAGE)
    } else {
        TEMPLE_PAGES
            .iter()
            .find(|(prefix, _)| *prefix == [a, b, c])
            .map(|(_, page)| *page)
    }
}

/// A spell page's buttons: `SpellsController` sends
/// these by spell button, the Abode's at rest too; 8013 goes back.
pub const PAGE_BUTTONS: std::ops::RangeInclusive<i32> = 4401..=4428;

/// A page button's price box and icon, below and above it by this:
/// Spell button state disables all three together and writes the
/// price into the box.
pub const PAGE_OFFSET: i32 = 1000;

/// The price box of page button `control`.
#[must_use]
pub const fn page_price(control: i32) -> i32 {
    control - PAGE_OFFSET
}

/// The icon of page button `control`.
#[must_use]
pub const fn page_icon(control: i32) -> i32 {
    control + PAGE_OFFSET
}

/// The release whose archives `catalogue` holds: the expansion's where it
/// has [`ABODE_ICONS`]. A base quest loads the base dataset alone.
#[must_use]
pub fn release(catalogue: &Catalogue<'_>) -> Release {
    match catalogue.find(b"IMAG", ABODE_ICONS) {
        Some(_) => Release::Expansion,
        None => Release::Base,
    }
}

/// A hovered icon's frame, its one-pixel edge: the colour every edge
/// pixel of the Abode's `IX23` slot 6 holds, slot 0 otherwise.
pub const LIT_FRAME: Rgb = Rgb {
    r: 250,
    g: 226,
    b: 149,
};

/// An icon the crown may not cast is grey inside its frame, the frame
/// kept: about `GREY_FLOOR` plus `GREY_SHARE` of its brightness
/// (**Inferred**: a fit of the Abode's `IX23` slot 2 against slot 0; the
/// engine draws a slot, not a tone). Hovered it stays so: slot 8 is
/// slot 2.
pub const GREY_FLOOR: f32 = 102.0;
pub const GREY_SHARE: f32 = 0.5;

/// The grey of colour `rgb` in a not-castable icon.
#[must_use]
pub fn grey(
    Rgb {
        r: red,
        g: green,
        b: blue,
    }: Rgb,
) -> u8 {
    let light = 0.299 * f32::from(red) + 0.587 * f32::from(green) + 0.114 * f32::from(blue);

    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a channel"
    )]
    let level = (GREY_FLOOR + GREY_SHARE * light).round().clamp(0.0, 255.0) as u8;

    level
}

/// Whether pixel (`x`, `y`) of a `width` × `height` icon is its frame.
#[must_use]
pub const fn on_frame((x, y): (usize, usize), (width, height): (usize, usize)) -> bool {
    x == 0 || y == 0 || x + 1 == width || y + 1 == height
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_base_bar_lacks_only_the_abodes_six() {
        let base = shown(Release::Base);

        assert_eq!(base.len(), 22);
        assert!(base.iter().all(|b| b.record == ICONS));
        assert!(
            BUTTONS[BASE_BUTTONS..]
                .iter()
                .all(|b| b.record == ABODE_ICONS)
        );
    }

    #[test]
    fn black_greys_to_the_floor_and_the_frame_is_the_edge() {
        assert_eq!(grey([0, 0, 0].into()), 102);
        assert_eq!(grey([255, 255, 255].into()), 230);
        assert!(on_frame((0, 5), (25, 25)));
        assert!(on_frame((24, 5), (25, 25)));
        assert!(!on_frame((1, 1), (25, 25)));
    }
}
