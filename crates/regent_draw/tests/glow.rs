//! The quest map's hover look against the original's lit pictures.
//!
//! Needs `MAJESTY_HD`.

use regent_corpus as common;

use regent_core::data::fourcc::FourCc;
use regent_core::view::menu::quest_map;
use regent_draw::art::glow::{self, Grid, Picture, Table};

/// The 1920×1200 layout, the captures' size.
fn panel() -> regent_core::data::smnu::Panel {
    quest_map::load(&common::hd().join("Data/UIData_1920_1200.dat")).expect("APdb")
}

/// The hover look ([`glow`]), fitted on the 31 painted lit
/// pictures and drawn through the ellipse, leaves a mean 14.0, 11.6, 8.5
/// a channel against them over their opaque pixels; the map alone 33.7,
/// 25.2, 9.1. The experiment's figures, reproduced within 0.2; the
/// ellipse on the lit rectangle, as drawn.
#[test]
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "a picture's pixels"
)]
fn the_hover_look_is_near_the_lit_pictures() {
    const MEAN: [f64; 3] = [14.0, 11.6, 8.5];
    const PLAIN: [f64; 3] = [33.7, 25.2, 9.1];
    const WITHIN: f64 = 0.2;

    /// A lit picture's corner, its rectangle's size, its own and pixels.
    type Lit = ([usize; 2], [usize; 2], [usize; 2], Vec<u8>);

    let library = common::library();
    let catalogue = library.catalogue().unwrap();
    let map_art = quest_map::map(&panel()).unwrap().art.unwrap();
    let map = quest_map::picture(&catalogue, map_art, 0, 0)
        .unwrap()
        .unwrap();
    let map_rgba = map.rgba();
    let map_picture = Picture {
        size: [usize::from(map.width), usize::from(map.height)],
        rgba: &map_rgba,
    };
    let lits: Vec<Lit> = quest_map::sites(&panel())
        .iter()
        .filter_map(|site| {
            let lit = site.lit?;
            let art = lit.art.filter(|a| a.record == FourCc(*b"IX33"))?;
            let sprite = quest_map::picture(&catalogue, art, 1, 0)?.unwrap();
            let at = [lit.rect.x as usize, lit.rect.y as usize];

            Some((
                at,
                [lit.rect.width as usize, lit.rect.height as usize],
                [usize::from(sprite.width), usize::from(sprite.height)],
                sprite.rgba(),
            ))
        })
        .collect();
    let pictures: Vec<([usize; 2], Picture)> = lits
        .iter()
        .map(|(at, _, size, rgba)| (*at, Picture { size: *size, rgba }))
        .collect();

    assert_eq!(pictures.len(), 31);

    let table = Table::fit(map_picture, &pictures);
    let mean = |table: &Table| {
        let mut sum = [0.0; 3];
        let mut count = 0.0;

        // The ellipse on the rectangle, as drawn.
        for (&(at, lit), (_, size, ..)) in pictures.iter().zip(&lits) {
            let grid = Grid {
                at,
                size: *size,
                scale: 1.0,
            };
            let rect = [at[0], at[1], size[0], size[1]].map(|n| n as f64);
            let weights = glow::weights(&glow::ellipse(rect, grid), grid);
            let [w, h] = lit.size;

            for (x, y) in (0..h).flat_map(|y| (0..w).map(move |x| (x, y))) {
                let i = y * w + x;
                let wt = weights[y * size[0] + x];
                let own = &lit.rgba[i * 4..i * 4 + 4];

                if own[3] > 0 {
                    let m = (at[1] + y) * map_picture.size[0] + at[0] + x;
                    let under = [0, 1, 2].map(|c| f32::from(map_rgba[m * 4 + c]) / 255.0);
                    let out = table.look(under, wt);

                    for c in 0..3 {
                        sum[c] += (f64::from(out[c]) * 255.0 - f64::from(own[c])).abs();
                    }

                    count += 1.0;
                }
            }
        }

        sum.map(|s| s / count)
    };
    let fitted = mean(&table);
    let plain = mean(&Table::identity());

    println!("fitted {fitted:?} plain {plain:?}");

    for c in 0..3 {
        assert!((fitted[c] - MEAN[c]).abs() < WITHIN, "{fitted:?}");
        assert!((plain[c] - PLAIN[c]).abs() < WITHIN, "{plain:?}");
    }
}
