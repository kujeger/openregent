//! The top bar's day counter, against HD's text and saves.

mod common;

use common::read;
use regent_core::data::cam;
use regent_core::data::smnu::{self, Panel, Rect};
use regent_core::data::text::Text;
use regent_core::view::panel::day::{self, DAY_MS, Day};

/// `20 Days`, `1 Day`, `0 Days`, from HD's own strings.
#[test]
fn the_counter_reads_as_the_game_writes_it() {
    let library = common::library();
    let catalogue = library.catalogue().expect("a catalogue");
    let text = Text::build(&catalogue);
    let shown = |clock| Day::of(clock).text(&text).expect("the strings");

    assert_eq!(shown(20 * DAY_MS), "20 Days");
    assert_eq!(shown(DAY_MS + 1), "1 Day");
    assert_eq!(shown(0), "0 Days");
}

/// Groups of a panel's items, one per rectangle: a control and its keys.
fn controls(panel: &Panel) -> Vec<Vec<(i32, Vec<i32>)>> {
    let mut out: Vec<Vec<(i32, Vec<i32>)>> = Vec::new();

    for item in panel.items() {
        if item.key == smnu::RECT || out.is_empty() {
            out.push(Vec::new());
        }

        if let Some(group) = out.last_mut() {
            group.push((item.key, item.value.to_vec()));
        }
    }

    out
}

fn word(group: &[(i32, Vec<i32>)], key: i32) -> Option<i32> {
    group
        .iter()
        .find(|(k, _)| *k == key)
        .and_then(|(_, v)| v.first().copied())
}

/// Key 31 and 32 on the bar: its range.
const BAR_LOW: i32 = 31;
const BAR_HIGH: i32 = 32;

/// The bar's picture: `IMZa` set 1000.
const BAR_ART: (&[u8; 4], i32) = (b"IMZa", 1000);

/// What `APMK` says of the counter.
struct Counter {
    text: Rect,
    bar: Rect,
    /// Keys 31 and 32.
    range: (i32, i32),
    /// Keys 12 and 13.
    art: (i32, i32),
}

/// The counter's controls in `APMK`, if it has one.
fn counter(archive: &cam::Archive<'_>) -> Option<Counter> {
    let (_, main) = archive
        .records()
        .find(|(chunk, r)| chunk.tag.as_bytes() == b"SMNU" && r.id.as_bytes() == b"APMK")?;
    let panel = Panel::parse(main.data).expect("it parses");
    let groups = controls(&panel);
    let of = |control| {
        groups
            .iter()
            .find(|g| word(g, smnu::CONTROL) == Some(control))
    };
    let rect = |g: &[(i32, Vec<i32>)]| {
        let (_, v) = g.first().filter(|(k, _)| *k == smnu::RECT)?;

        Some(Rect {
            x: v[0],
            y: v[1],
            width: v[2],
            height: v[3],
        })
    };
    let (text, bar) = (of(day::TEXT_CONTROL)?, of(day::BAR_CONTROL)?);
    let range = (word(bar, BAR_LOW)?, word(bar, BAR_HIGH)?);
    let art = (word(bar, smnu::ID)?, word(bar, smnu::SET)?);

    Some(Counter {
        text: rect(text)?,
        bar: rect(bar)?,
        range,
        art,
    })
}

/// Every main screen puts the day's text inside its bar, in the top
/// right corner, the bar an `IMZa` picture counting 0 to 100.
#[test]
fn the_main_screen_puts_the_days_text_on_its_bar() {
    let hd = common::hd();
    let mut files: Vec<_> = std::fs::read_dir(hd.join("Data"))
        .expect("HD Data/")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.to_string_lossy().contains("UIData_"))
        .collect();

    files.push(hd.join("Data/textdata.cam"));
    files.push(hd.join("DataMX/mx_textdata.cam"));

    let mut found = 0;

    for path in files {
        let bytes = read(&path);
        let archive = cam::Archive::parse(&bytes).expect("an archive");
        let Some(Counter {
            text,
            bar,
            range,
            art,
        }) = counter(&archive)
        else {
            continue;
        };
        let name = path.display();

        assert_eq!(range, (0, day::FULL.cast_signed()), "{name}");
        assert!(text.x >= bar.x && text.y >= bar.y, "{name}");
        assert!(text.x + text.width <= bar.x + bar.width, "{name}");
        assert!(text.y + text.height <= bar.y + bar.height, "{name}");
        assert_eq!(art, (i32::from_le_bytes(*BAR_ART.0), BAR_ART.1), "{name}");
        assert!(bar.x >= 700 && bar.y < 10, "{name}: {bar:?}");
        found += 1;
    }

    assert_eq!(found, 14);
}
