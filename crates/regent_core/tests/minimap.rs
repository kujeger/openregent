//! The minimap against the game: `heighty.GMP` and the two 1920 x 1200
//! captures taken of it (`local/shots/heighty_*_native.png`), one moment,
//! the main view scrolled apart.
//!
//! Needs `MAJESTY_HD` and the captures.

mod common;

use regent_core::data::cam;
use regent_core::data::smnu::{self, Panel};
use regent_core::view::ScreenRect;
use regent_core::view::panel::minimap::{self, Scale};

const LOWER_LEFT: &str = "heighty_lowerleft_native.png";
const UPPER_RIGHT: &str = "heighty_upperright_native.png";

const SIDE: i32 = minimap::RECT[2] - minimap::RECT[0];
const CELLS: i32 = 128;

/// The frames as measured, minimap pixels: the main view's in each
/// capture, then mini view 1's. Positions are the captures', not
/// predicted: the scroll is not in the save.
const MAIN_FRAMES: [(&str, ScreenRect); 2] = [
    (LOWER_LEFT, [0, 117, 73, 175]),
    (UPPER_RIGHT, [102, 4, 175, 62]),
];

/// The main view's frame is as wide as `OverheadGameView` makes
/// its 1,710 pixels, 73; **Inferred**, open: its 1,136 rows predict 59,
/// the captures show 58.
#[test]
fn the_main_views_frame_is_as_wide_as_the_view() {
    let scale = Scale::fitted(SIDE, CELLS);
    let down = 32.0 * 55f32.to_radians().sin();
    let rect = minimap::view_rect((0, 0), (1710, 1136), 1.0, down);
    let frame = minimap::viewport(rect, scale);

    assert_eq!(
        frame[2] - frame[0],
        MAIN_FRAMES[0].1[2] - MAIN_FRAMES[0].1[0]
    );
    assert_eq!(frame[3] - frame[1], 59);
}

/// Control 2001 in `APMK` of every HD `UIData` file and both text
/// archives: one rect, `minimap::RECT`.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn every_main_screen_puts_the_minimap_at_one_rect() {
    let hd = common::hd();
    let mut files: Vec<_> = std::fs::read_dir(hd.join("Data"))
        .expect("HD Data/")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.to_string_lossy().contains("UIData_"))
        .collect();

    files.push(hd.join("Data/textdata.cam"));
    files.push(hd.join("DataMX/mx_textdata.cam"));

    let rects: Vec<[i32; 4]> = files
        .iter()
        .filter_map(|path| {
            let bytes = std::fs::read(path).expect("the archive reads");
            let archive = cam::Archive::parse(&bytes).expect("an archive");
            let (_, main) = archive
                .records()
                .find(|(chunk, r)| chunk.tag.as_bytes() == b"SMNU" && r.id.as_bytes() == b"APMK")?;
            let panel = Panel::parse(main.data).expect("it parses");

            control_rect(&panel, minimap::CONTROL)
        })
        .collect();

    assert_eq!(rects.len(), 14);
    assert!(rects.iter().all(|r| *r == minimap::RECT), "{rects:?}");
}

/// Control `id`'s rect, right and bottom exclusive: a control's items
/// run from its rect to the next.
fn control_rect(panel: &Panel, id: u32) -> Option<[i32; 4]> {
    let mut rect = None;

    for item in panel.items() {
        if item.key == smnu::RECT {
            rect = item
                .value
                .get(..4)
                .map(|v| [v[0], v[1], v[0] + v[2], v[1] + v[3]]);
        }

        let named =
            item.key == smnu::CONTROL && item.value.first().copied() == i32::try_from(id).ok();

        if named {
            return rect;
        }
    }

    None
}
