//! The spell bar's buttons and full-size icons, against both datasets'
//! `AP92` and art (`regent_core::view::panel::spell_bar`).

mod common;

use regent_core::content::dataset::Datasets;
use regent_core::data::catalogue::{Catalogue, Library};
use regent_core::data::fourcc::FourCc;
use regent_core::data::text::Text;
use regent_core::sim::world::{offer::SPELLS, spell::cast::ABODE};
use regent_core::view::Sprite;
use regent_core::view::menu::{self, Release};
use regent_core::view::panel::spell_bar::{
    self, ABODE_ICONS, BASE_BUTTONS, BUTTONS, ICONS, LIT_FRAME, RAGE_ICON, SQUEEZED, on_frame,
};

/// The bar, `AP92`.
const BAR: FourCc = FourCc::new(b"AP92");

/// The first `IX23` set without an `INTN` twin: the Abode's.
const FIRST_ABODE_SET: i32 = 1023;

/// The slots the bar draws: at rest, not castable, hovered, both.
const REST: usize = 0;
const UNCASTABLE: usize = 2;
const HOVERED: usize = 6;
const HOVERED_UNCASTABLE: usize = 8;

fn library(sets: Datasets) -> &'static Library {
    common::dataset(sets)
}

fn slot(catalogue: &Catalogue<'_>, (record, set): (FourCc, i32), slot: usize) -> Sprite {
    menu::state(catalogue, record, set, slot)
        .unwrap_or_else(|| panic!("{record} {set} slot {slot} is missing"))
        .unwrap_or_else(|e| panic!("{record} {set}: {e}"))
}

/// Each spell button's control and art in a dataset's bar, left to right.
fn bar(catalogue: &Catalogue<'_>) -> Vec<(i32, (FourCc, i32))> {
    let text = Text::build(catalogue);
    let layout = menu::layout(catalogue, &text, BAR)
        .expect("the bar ships")
        .expect("it parses");
    let mut buttons: Vec<_> = layout
        .controls
        .iter()
        .filter(|c| BUTTONS.iter().any(|b| Some(b.control) == c.id))
        .filter_map(|c| Some((c.rect?.x, c.id?, c.art?)))
        .collect();

    buttons.sort_by_key(|(x, ..)| *x);
    buttons.into_iter().map(|(_, id, art)| (id, art)).collect()
}

/// How far a not-castable icon's frame strays from its picture's at
/// rest, per channel: the slots are drawn apart.
const FRAME_DRIFT: u8 = 8;

/// The largest channel difference of two colours.
fn near(a: [u8; 3], b: [u8; 3]) -> u8 {
    a.iter()
        .zip(b)
        .map(|(p, q)| p.abs_diff(q))
        .max()
        .unwrap_or(0)
}

fn rgb(sprite: &Sprite, x: usize, y: usize) -> [u8; 3] {
    let at = (y * sprite.width as usize + x) * 4;
    let rgba = sprite.rgba();

    [rgba[at], rgba[at + 1], rgba[at + 2]]
}

#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn the_buttons_run_in_each_datasets_bar_order() {
    let base = library(Datasets::Base);
    let expansion = library(Datasets::Expansion);
    let controls = |list: &[spell_bar::Button]| list.iter().map(|b| b.control).collect::<Vec<_>>();
    let ids =
        |bar: Vec<(i32, (FourCc, i32))>| bar.into_iter().map(|(id, _)| id).collect::<Vec<_>>();

    assert_eq!(
        ids(bar(&base.catalogue().expect("an index"))),
        controls(spell_bar::shown(Release::Base))
    );
    assert_eq!(
        ids(bar(&expansion.catalogue().expect("an index"))),
        controls(&BUTTONS)
    );
}

/// The base spells' full-size icons are their base bar's art; the
/// expansion's squeezed set n is `INTN` n, or the Abode's `IX02`.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn each_button_takes_its_full_size_icon() {
    let base = library(Datasets::Base);
    let base = base.catalogue().expect("an index");
    let expansion = library(Datasets::Expansion);
    let expansion = expansion.catalogue().expect("an index");

    for ((control, art), b) in bar(&base).into_iter().zip(&BUTTONS) {
        assert_eq!((control, art), (b.control, (ICONS, b.set)));
    }

    for (control, (record, set)) in bar(&expansion) {
        let b = BUTTONS
            .iter()
            .find(|b| b.control == control)
            .expect("listed");

        assert_eq!(record, SQUEEZED);
        assert_eq!(b.record == ABODE_ICONS, set >= FIRST_ABODE_SET, "{control}");

        if b.record == ICONS {
            assert_eq!(b.set, set, "{control}");
        }
    }

    for b in &BUTTONS {
        let icon = slot(&expansion, (b.record, b.set), REST);
        let side = if b.record == ICONS { 24 } else { 25 };

        assert_eq!((icon.width, icon.height), (side, side), "{}", b.spell);
    }

    for b in spell_bar::shown(Release::Base) {
        assert_eq!(slot(&base, (b.record, b.set), UNCASTABLE).width, 24);
    }
}

/// The Rage's icon is its heroes' effect's, one 24 × 24 slot in both
/// datasets, as a base spell's.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn the_rage_icon_is_its_effects_at_spell_size() {
    use regent_core::view::panel::portrait::enchant;

    let effect = enchant::of(FourCc::new(b"QRa1"))
        .and_then(|e| e.icon)
        .expect("the Rage's row");

    assert_eq!((effect.0, i32::from(effect.1)), RAGE_ICON);

    for sets in [Datasets::Base, Datasets::Expansion] {
        let library = library(sets);
        let catalogue = library.catalogue().expect("an index");
        let icon = slot(&catalogue, RAGE_ICON, REST);

        assert_eq!((icon.width, icon.height), (24, 24));
        assert!(menu::state(&catalogue, RAGE_ICON.0, RAGE_ICON.1, 1).is_none());
    }
}

/// `IX02`'s picture, its columns picked nearest to the squeezed width,
/// is nearer its own Abode spell's `IX23` picture than any other's.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn the_abode_icons_are_their_squeezed_pictures_at_full_size() {
    let library = library(Datasets::Expansion);
    let catalogue = library.catalogue().expect("an index");
    let text = Text::build(&catalogue);
    let layout = menu::layout(&catalogue, &text, BAR)
        .expect("the bar ships")
        .expect("it parses");
    let abode = &BUTTONS[BASE_BUTTONS..];
    let squeezed: Vec<Sprite> = abode
        .iter()
        .map(|b| {
            let art = layout.control(b.control).and_then(|c| c.art).expect("art");

            slot(&catalogue, art, REST)
        })
        .collect();
    let distance = |full: &Sprite, small: &Sprite| -> u32 {
        let (w, h) = (small.width as usize, small.height as usize);

        (0..h)
            .flat_map(|y| (0..w).map(move |x| (x, y)))
            .map(|(x, y)| {
                let from = x * full.width as usize / w;
                let (a, b) = (rgb(full, from, y), rgb(small, x, y));

                a.iter()
                    .zip(b)
                    .map(|(p, q)| u32::from(p.abs_diff(q)))
                    .sum::<u32>()
            })
            .sum()
    };

    for (i, b) in abode.iter().enumerate() {
        let full = slot(&catalogue, (b.record, b.set), REST);
        let nearest = (0..abode.len())
            .min_by_key(|&j| distance(&full, &squeezed[j]))
            .expect("six");

        assert_eq!(nearest, i, "{}", b.spell);
    }
}

/// Hovered, an Abode icon's frame lights, the rest kept; not castable,
/// its inside greys about as [`spell_bar::grey`] says, its frame about
/// kept, hovered or not.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn the_abode_icons_states_are_a_lit_frame_and_a_grey_inside() {
    let library = library(Datasets::Expansion);
    let catalogue = library.catalogue().expect("an index");

    for set in FIRST_ABODE_SET..FIRST_ABODE_SET + 6 {
        let look = |s| slot(&catalogue, (SQUEEZED, set), s);
        let (rest, unlit, lit) = (look(REST), look(UNCASTABLE), look(HOVERED));
        let size = (rest.width as usize, rest.height as usize);
        let mut error = 0.0;
        let mut inside = 0.0;
        let mut frame = 0;

        assert_eq!(look(HOVERED_UNCASTABLE).rgba(), unlit.rgba(), "{set}");

        for y in 0..size.1 {
            for x in 0..size.0 {
                let at = rgb(&rest, x, y);

                if on_frame((x, y), size) {
                    assert_eq!(rgb(&lit, x, y), LIT_FRAME.channels(), "{set} ({x}, {y})");
                    frame = frame.max(near(rgb(&unlit, x, y), at));
                } else {
                    let grey = rgb(&unlit, x, y);

                    assert_eq!(rgb(&lit, x, y), at, "{set} ({x}, {y})");
                    assert!(grey[0] == grey[1] && grey[1] == grey[2], "{set} ({x}, {y})");
                    error += f32::from(grey[0].abs_diff(spell_bar::grey(at.into())));
                    inside += 1.0;
                }
            }
        }

        assert!(error / inside < 8.0, "{set}: {}", error / inside);
        assert!(frame <= FRAME_DRIFT, "{set}: {frame}");
    }
}

/// Only the Abode's spells are the expansion's: the base bar's buttons
/// are every spell row of another building.
#[test]
fn the_base_bar_holds_every_spell_but_the_abodes() {
    let mut base: Vec<_> = spell_bar::shown(Release::Base)
        .iter()
        .map(|b| b.spell)
        .collect();
    let mut rows: Vec<_> = SPELLS
        .iter()
        .filter(|r| r.building != ABODE)
        .map(|r| r.spell)
        .collect();

    base.sort_by_key(|s| s.0);
    rows.sort_by_key(|s| s.0);

    assert_eq!(base, rows);
}

#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_catalogue_tells_its_release() {
    let base = library(Datasets::Base);
    let expansion = library(Datasets::Expansion);

    assert_eq!(
        spell_bar::release(&base.catalogue().expect("an index")),
        Release::Base
    );
    assert_eq!(
        spell_bar::release(&expansion.catalogue().expect("an index")),
        Release::Expansion
    );
}

/// `MX07` holds the Abode's six buttons, each with its price box and its
/// bar icon at the page's offsets.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn the_abodes_page_holds_its_six_spells() {
    let expansion = library(Datasets::Expansion);
    let catalogue = expansion.catalogue().expect("an index");
    let text = Text::build(&catalogue);
    let page = menu::layout(&catalogue, &text, spell_bar::ABODE_PAGE)
        .expect("the page ships")
        .expect("it parses");
    let abode: Vec<_> = BUTTONS.iter().filter(|b| b.record == ABODE_ICONS).collect();
    let held: Vec<i32> = page
        .controls
        .iter()
        .filter_map(|c| c.id)
        .filter(|id| spell_bar::PAGE_BUTTONS.contains(id))
        .collect();

    assert_eq!(held, abode.iter().map(|b| b.control).collect::<Vec<_>>());

    for b in abode {
        let icon = page.control(spell_bar::page_icon(b.control));

        assert!(page.control(spell_bar::page_price(b.control)).is_some());
        assert_eq!(icon.and_then(|c| c.art), Some((b.record, b.set)));
    }
}

/// Each temple's page holds its own spells' buttons and no other, each
/// with its price box and icon; the Abode's page is its own.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn each_temples_page_holds_its_own_spells() {
    let expansion = library(Datasets::Expansion);
    let catalogue = expansion.catalogue().expect("an index");
    let text = Text::build(&catalogue);

    for (prefix, page) in spell_bar::TEMPLE_PAGES {
        let layout = menu::layout(&catalogue, &text, page)
            .expect("the page ships")
            .expect("it parses");
        let held: Vec<i32> = layout
            .controls
            .iter()
            .filter_map(|c| c.id)
            .filter(|id| spell_bar::PAGE_BUTTONS.contains(id))
            .collect();
        let mut own: Vec<i32> = SPELLS
            .iter()
            .filter(|r| r.building == prefix)
            .filter_map(|r| BUTTONS.iter().find(|b| b.spell == r.spell))
            .map(|b| b.control)
            .collect();
        let mut sorted = held.clone();

        own.sort_unstable();
        sorted.sort_unstable();
        assert_eq!(sorted, own, "{page}");

        for control in held {
            assert!(layout.control(spell_bar::page_price(control)).is_some());
            assert!(layout.control(spell_bar::page_icon(control)).is_some());
        }

        let code = FourCc([prefix[0], prefix[1], prefix[2], b'1']);

        assert_eq!(spell_bar::spell_page(code), Some(page));
    }

    let abode = FourCc([ABODE[0], ABODE[1], ABODE[2], b'2']);

    assert_eq!(spell_bar::spell_page(abode), Some(spell_bar::ABODE_PAGE));
}

/// The expansion bar's rest bar lies over the Abode's six buttons and
/// no other, with the Abode panel's bar art (4500).
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn the_rest_bar_covers_the_abodes_six_buttons() {
    let expansion = library(Datasets::Expansion);
    let catalogue = expansion.catalogue().expect("an index");
    let text = Text::build(&catalogue);
    let layout = menu::layout(&catalogue, &text, BAR)
        .expect("the bar ships")
        .expect("it parses");
    let panel = menu::layout(
        &catalogue,
        &text,
        regent_core::view::panel::upper::ABODE_PANEL,
    )
    .expect("the panel ships")
    .expect("it parses");
    let rest = layout
        .control(spell_bar::RESTING_BAR)
        .expect("the rest bar");
    let r = rest.rect.expect("its rectangle");
    let xs: Vec<(i32, bool)> = layout
        .controls
        .iter()
        .filter_map(|c| {
            let b = BUTTONS.iter().find(|b| Some(b.control) == c.id)?;

            Some((c.rect?.x, b.record == ABODE_ICONS))
        })
        .collect();
    let abode = xs.iter().filter(|(_, a)| *a).map(|(x, _)| *x);
    let inside = |x: i32| (r.x..r.x + r.width).contains(&x);

    assert_eq!((r.x, r.y, r.width, r.height), (464, 4, 124, 26));
    assert_eq!(abode.clone().min(), Some(r.x));
    assert!(abode.clone().all(inside));
    assert_eq!(abode.count(), 6);
    assert!(xs.iter().filter(|(_, a)| !*a).all(|(x, _)| !inside(*x)));
    assert_eq!(rest.art, Some((FourCc::new(b"INBd"), 1000)));
    assert_eq!(rest.art, panel.control(4500).and_then(|c| c.art));
}
