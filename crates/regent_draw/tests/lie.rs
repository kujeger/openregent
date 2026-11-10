//! Where a soft shadow lies, against HD's art: [`regent_draw::lie`].
//!
//! Needs `MAJESTY_HD`.

use regent_corpus as common;

use std::collections::BTreeSet;

use regent_core::content::unit::Unit;
use regent_core::data::catalogue::Catalogue;
use regent_core::data::fourcc::FourCc;
use regent_core::view::Sprite;
use regent_core::view::raster::shade::CONTROL_FIRST;
use regent_draw::lie::{self, ROW};
use regent_draw::map::depth::Depth;
use regent_draw::soft::Ramps;
use regent_draw::texture;

/// The Palace's tile: what its artists painted the shadow on.
const PALACE_TILE: usize = 1036;

/// The Palace's row in cells, for the scene: anywhere on the map.
const PALACE_ROW: f32 = 60.0;

/// How far either side of the shadow's ground row the two units stand.
const APART: f32 = 0.3;

/// The Palace's tile, its indices as the band shader reads them, and its
/// ramps.
fn palace() -> (Sprite, Vec<u8>, Ramps) {
    let library = common::library();
    let catalogue = library.catalogue().expect("a catalogue");
    let unit = Unit::find(&catalogue, FourCc(*b"ABJ1"))
        .expect("the Palace")
        .expect("it reads");
    let (archive, index) = unit
        .frames()
        .filter_map(|f| f.tile())
        .find(|(_, index)| *index == PALACE_TILE)
        .expect("the Palace draws tile 1036");
    let frame = Sprite::at(&catalogue, archive, index)
        .expect("a tile")
        .expect("it decodes");
    let (image, _) = texture::indices(&frame).expect("indexed");
    let ramps = Ramps::of(frame.mode);

    (frame, image.data.expect("its bytes"), ramps)
}

/// Most of the Palace's shadow, 87 % of its darkening pixels, lies in
/// front of its anchor, where the engine, sorting it at the Palace's row,
/// darkens nothing standing in it.
#[test]
fn the_palaces_shadow_lies_in_front_of_it() {
    let (frame, indices, ramps) = palace();
    let width = usize::from(frame.width);
    let anchor = usize::try_from(frame.anchor.1).expect("inside");
    let rows: Vec<usize> = indices
        .iter()
        .enumerate()
        .filter(|(_, i)| lie::darkens(ramps, **i))
        .map(|(n, _)| n / width)
        .collect();
    let south = rows.iter().filter(|r| **r > anchor).count();

    assert_eq!((frame.width, frame.height), (501, 339));
    assert_eq!(frame.anchor, (269, 87));
    assert_eq!((south, rows.len()), (4_586, 5_264), "87 % in front");
}

/// A unit standing in the Palace's front-left shadow, nearer than the
/// Palace, takes it; one standing just past the shadow's ground row does
/// not; and the shadow never falls further back than its caster's.
#[test]
fn a_unit_in_the_palaces_shadow_takes_it() {
    let (frame, indices, ramps) = palace();
    let width = usize::from(frame.width);
    let (left, anchor) = (
        usize::try_from(frame.anchor.0).expect("inside"),
        usize::try_from(frame.anchor.1).expect("inside"),
    );

    // The front-left column with the most shadow in front of the anchor,
    // and its topmost shadow pixel.
    let column = |x: usize| {
        (anchor..usize::from(frame.height))
            .filter(|y| lie::darkens(ramps, indices[y * width + x]))
            .count()
    };
    let x = (0..left).max_by_key(|x| column(*x)).expect("a column");
    let y = (0..usize::from(frame.height))
        .find(|y| lie::darkens(ramps, indices[y * width + x]))
        .expect("a shadow pixel");
    let foot = lie::foot(&indices, width, (x, y), ramps);

    // Placed rows every quarter cell, as dense scatter.
    #[allow(clippy::cast_precision_loss, reason = "a few hundred rows")]
    let depth = Depth::new((0..512).map(|n| n as f32 / 4.0).collect());
    let caster = depth.at(PALACE_ROW);

    #[allow(clippy::cast_precision_loss, reason = "a tile's rows")]
    let (top, ground) = (
        PALACE_ROW - anchor as f32 / ROW,
        PALACE_ROW + (foot as f32 - anchor as f32) / ROW,
    );
    let shadow = lie::lying(&depth, caster, top, foot);
    let (inside, past) = (depth.at(ground - APART), depth.at(ground + APART));

    assert!(ground > PALACE_ROW + 1.0, "the shadow reaches {ground}");
    assert!(inside > caster, "the engine leaves it unshaded");
    assert!(inside <= shadow, "standing in it, it is darkened");
    assert!(past > shadow, "standing past it, it is not");
    assert!((lie::lying(&depth, caster, top, 0) - caster).abs() < f32::EPSILON);
}

/// One part of a set as drawn: its place in the draw order, and its
/// darkening and drawn pixels about the anchor, screen rows down.
struct Part {
    order: (i32, usize),
    shadow: Vec<(i32, i32)>,
    body: BTreeSet<(i32, i32)>,
}

/// Every part of set `id` of `unit`, slot 0, frame 0.
fn parts(catalogue: &Catalogue<'_>, unit: &Unit<'_>, id: u16) -> Vec<Part> {
    let base = unit.stack(id).next();

    unit.stack(id)
        .flat_map(|set| set.passes.iter().map(move |pass| (set, *pass)))
        .enumerate()
        .filter_map(|(part, (set, pass))| {
            let pose = set.poses.first()?;
            let blit = (*pose.blits(0, pass).first()?)?;
            let frame = Sprite::at(catalogue, blit.archive, blit.index)?.ok()?;
            let ramps = Ramps::of(frame.mode);
            let data = texture::indices(&frame)?.0.data?;
            let width = usize::from(frame.width);
            let at = |n: usize| {
                let x = i32::try_from(n % width).ok()?;
                let y = i32::try_from(n / width).ok()?;

                Some((
                    x - i32::from(frame.anchor.0) + i32::from(blit.nudge.0) + i32::from(pose.x),
                    y - i32::from(frame.anchor.1) + i32::from(blit.nudge.1) + i32::from(pose.y),
                ))
            };
            let shadow = (0..data.len())
                .filter(|n| lie::darkens(ramps, data[*n]))
                .filter_map(at)
                .collect();
            let body = (0..data.len())
                .filter(|n| data[*n] < CONTROL_FIRST)
                .filter_map(at)
                .collect();

            Some(Part {
                order: (base.map_or(set.nudge, |b| set.listed(b)), part),
                shadow,
                body,
            })
        })
        .collect()
}

/// Where a set's layers sorting together meet, the engine's draw order
/// decides what a shadow darkens of its own set. The caster's depth as a
/// floor keeps that where the shadow lies behind the anchor, and departs
/// from it where the shadow lies in front, on a layer drawn later.
#[test]
fn a_sets_own_layers_take_its_shadow_mostly_as_the_engines() {
    let library = common::library();
    let catalogue = library.catalogue().expect("a catalogue");
    let (mut kept, mut gained) = (0usize, 0usize);

    for (at, _) in catalogue.tagged(b"DUNT") {
        let Ok(unit) = Unit::at(&catalogue, at) else {
            continue;
        };
        let ids: BTreeSet<u16> = unit
            .animations
            .iter()
            .map(|a| regent_core::data::imag::set_number(a.id))
            .collect();

        for id in ids {
            let parts = parts(&catalogue, &unit, id);

            for caster in &parts {
                let together = parts
                    .iter()
                    .filter(|p| p.order != caster.order && p.order.0 == caster.order.0);

                for under in together {
                    let hit = caster.shadow.iter().filter(|p| under.body.contains(p));

                    // Behind the anchor, on an earlier layer: the engine's
                    // darkening, kept. In front, on a later one: darkened
                    // here, not by the engine.
                    for p in hit {
                        match (under.order < caster.order, p.1 > 0) {
                            (true, false) => kept += 1,
                            (false, true) => gained += 1,
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    assert_eq!((kept, gained), (28_618, 5_190));
}
