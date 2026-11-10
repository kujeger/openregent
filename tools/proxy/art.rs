#!/usr/bin/env -S cargo +nightly -q -Zscript run --manifest-path
---cargo
[package]
edition = "2024"

[dependencies]
regent_core = { path = "../../crates/regent_core" }
---

// Per facing of one set: layers' frame 0 composed in anchor coordinates.
// Writes <id>_<set>_p<pose>.pgm (0 hole, 128 body, 255 darkening shadow)
// and prints one JSON line per pose: origin = anchor's pixel in the canvas.
use regent_core::data::catalogue::Library;
use regent_core::data::fourcc::FourCc;
use regent_core::data::imag::Image;
use regent_core::content::imagery::FrameSource;
use regent_core::content::unit::Unit;
use regent_core::view::{Ink, Shade, Sprite};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (dir, out, id, want) = (&a[1], std::path::PathBuf::from(&a[2]), &a[3], &a[4]);
    let files: Vec<_> = dir.split(':').flat_map(|d| std::fs::read_dir(d).unwrap()).filter_map(|e| e.ok()).map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("cam"))).collect();
    let library = Library::from_files(&files).unwrap();
    let cat = library.catalogue().unwrap();
    let unit = Unit::find(&cat, FourCc::new(id.as_bytes().try_into().unwrap())).unwrap().unwrap();

    let kind = unit.animations.iter().map(|x| x.kind)
        .find(|k| format!("{k:?}").eq_ignore_ascii_case(want)).expect("no such set");
    let layers: Vec<_> = unit.layers(kind).collect();
    let poses = layers.first().map_or(0, |l| l.poses.len());

    for p in 0..poses {
        // (x0, y0) = top-left in anchor coordinates, per piece.
        let mut pieces = vec![];
        for (nth, layer) in layers.iter().enumerate() {
            let Some(pose) = layer.poses.get(p) else { continue };
            let off = if nth > 0 { (i32::from(pose.x), i32::from(pose.y)) } else { (0, 0) };
            for s in 0..pose.streams {
                let Some(src) = pose.stream(s).first() else { continue };
                let Some((ar, ix)) = src.tile() else { continue };
                let nudge = pose.nudges.get(usize::from(s) * usize::from(pose.per_stream)).copied().unwrap_or_default();
                let sp = Sprite::at(&cat, ar, ix).unwrap().unwrap();
                let x0 = -i32::from(sp.anchor.0) + i32::from(nudge.0) + off.0;
                let y0 = -i32::from(sp.anchor.1) + i32::from(nudge.1) + off.1;
                eprintln!("{id} pose {p} layer {nth} tile {ix} nudge {nudge:?} off {off:?}");
                pieces.push((x0, y0, sp));
            }
        }
        if pieces.is_empty() { continue }
        let minx = pieces.iter().map(|q| q.0).min().unwrap();
        let miny = pieces.iter().map(|q| q.1).min().unwrap();
        let maxx = pieces.iter().map(|q| q.0 + i32::from(q.2.width)).max().unwrap();
        let maxy = pieces.iter().map(|q| q.1 + i32::from(q.2.height)).max().unwrap();
        let (w, h) = ((maxx - minx) as usize, (maxy - miny) as usize);
        let mut px = vec![0u8; w * h];
        let mut rgb = vec![[96u8, 128, 64]; w * h];
        for (x0, y0, sp) in &pieces {
            for i in 0..sp.covered.len() {
                let v = match sp.as_drawn(i) {
                    Ink::Hole => continue,
                    Ink::Shade(Shade::Keep(_)) => 255,
                    Ink::Shade(_) => continue,
                    Ink::Colour(_) => 128,
                };
                let (x, y) = (i % usize::from(sp.width), i / usize::from(sp.width));
                let row = (*y0 - miny) as usize + y;
                let col = (*x0 - minx) as usize + x;
                // Body over shadow: a later shadow layer does not hide the body.
                let cell = &mut px[row * w + col];
                if *cell != 128 {
                    *cell = v;
                    let g = regent_core::data::payload::Rgb { r: 96, g: 128, b: 64 };
                    let c = match sp.as_drawn(i) {
                        Ink::Colour(c) => c,
                        Ink::Shade(s) => s.over(g),
                        Ink::Hole => g,
                    };
                    rgb[row * w + col] = [c.r, c.g, c.b];
                }
            }
        }
        let mut pgm = format!("P5\n{w} {h}\n255\n").into_bytes();
        pgm.extend(&px);
        std::fs::write(out.join(format!("{id}_{want}_p{p}.pgm")), pgm).unwrap();
        let mut ppm = format!("P6\n{w} {h}\n255\n").into_bytes();
        ppm.extend(rgb.iter().flatten());
        std::fs::write(out.join(format!("{id}_{want}_p{p}.ppm")), ppm).unwrap();
        println!("{{\"id\":\"{id}\",\"set\":\"{want}\",\"pose\":{p},\"origin\":[{},{}]}}", -minx, -miny);
    }

    // Base polygon: list 0 of slot 0 of set 8 or 0xd0.
    if let Some(link) = unit.art.as_ref() {
        let rec = cat.record(b"IMAG", link.at.unwrap()).unwrap();
        let image = Image::parse(rec.data).unwrap();
        let poly = [8u32, 0xd0].iter().find_map(|sid| {
            let s = image.sets.iter().find(|s| s.id == *sid)?;
            s.slots.first()?.point_lists().first().filter(|l| !l.is_empty()).cloned()
        });
        if let Some(poly) = poly {
            println!("{{\"id\":\"{id}\",\"polygon\":{:?}}}", poly.iter().map(|&(x, y)| [x, y]).collect::<Vec<_>>());
        }
    }
}
