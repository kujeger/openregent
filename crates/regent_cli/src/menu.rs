//! The front end's panels: the main menu and a quest's briefing, each
//! control with its rectangle, picture, font and text
//! ([`regent_core::view::menu`]).

use std::fmt::Write;
use std::path::Path;

use regent_core::content::dataset::{Bytecode, Datasets, Install};
use regent_core::data::catalogue::{Catalogue, Library};
use regent_core::data::fourcc::FourCc;
use regent_core::data::text::Text;
use regent_core::view::menu::quest_map;
use regent_core::view::menu::{self, Layout, Release, Role, ShownControl, State, World};

use super::{env, util};

/// The panels shown when none is asked for.
const PANELS: [FourCc; 3] = [menu::MAIN, menu::BRIEFING, menu::SUMMARY];

/// Print the panels from `MAJESTY_HD`'s `sets`; with `quest`, its briefing
/// pictures; with `dump`, each picture as a PAM file there.
///
/// # Errors
///
/// When the install, a panel or a picture will not read.
pub fn cmd_menu(
    sets: Datasets,
    record: Option<&str>,
    quest: Option<&str>,
    dump: Option<&str>,
    stills: &[String],
) -> Result<bool, String> {
    let hd = env::path(env::HD).map_err(|e| e.to_string())?;
    let files = Install::new(&hd, Bytecode::Install)
        .files(sets)
        .map_err(|e| e.to_string())?
        .archives;
    let library = Library::from_files(&files).map_err(|e| e.to_string())?;
    let catalogue = library.catalogue().map_err(|e| e.to_string())?;
    let text = Text::build(&catalogue);

    let wanted: Vec<FourCc> = match record {
        Some(id) => vec![util::fourcc(id)?],
        None => PANELS.to_vec(),
    };

    for id in wanted {
        let layout = menu::layout(&catalogue, &text, id)
            .ok_or_else(|| format!("no SMNU {id}"))?
            .map_err(|e| format!("{id}: {e}"))?;

        show(&layout);

        if let Some(dir) = dump {
            pictures(&catalogue, &layout, Path::new(dir))?;
        }
    }

    if let Some(code) = quest {
        briefing(&catalogue, code, sets, dump.map(Path::new))?;
    }

    for asked in stills {
        still(&catalogue, asked, dump.map(Path::new))?;
    }

    Ok(true)
}

/// One panel: its own block, then a line per control.
fn show(layout: &Layout) {
    let place = if layout.centred { "centred" } else { "placed" };

    println!("\n== {}  {place}  {}", layout.id, line(&layout.panel));

    for control in &layout.controls {
        println!("   {}", line(control));
    }
}

/// One control as text.
fn line(c: &ShownControl) -> String {
    let mut out = format!(
        "{:<9} id {:>5}",
        c.kind
            .map_or_else(|| "Panel".to_owned(), |k| format!("{k:?}")),
        c.id.map_or_else(|| "-".to_owned(), |i| i.to_string()),
    );

    if let Some(r) = c.rect {
        let _ = write!(out, "  {}x{} at ({}, {})", r.width, r.height, r.x, r.y);
    }

    if let Some((record, set)) = c.art {
        let _ = write!(out, "  art {record} {set}");
    }

    if let Some(font) = c.font {
        let _ = write!(out, "  font {font}");
    }

    if let Some(palette) = c.palette {
        let _ = write!(out, "  palette {palette}");
    }

    if let Some(key) = c.hotkey {
        let _ = write!(out, "  key {key}");
    }

    if !c.colours.is_empty() {
        let _ = write!(out, "  colours {}", hex(&c.colours));
    }

    if !c.hotkey_colours.is_empty() {
        let _ = write!(out, "  key colours {}", hex(&c.hotkey_colours));
    }

    if c.text.is_some() || !c.colours.is_empty() {
        let _ = write!(out, "  text in {}", resolved(c, Role::Text));
    }

    if c.hotkey_at().is_some() {
        let _ = write!(out, "  key in {}", resolved(c, Role::HotKey));
    }

    if let Some(text) = &c.text {
        let shown: String = text.chars().take(40).collect();

        let _ = write!(out, "  text {shown:?}");

        if let Some(at) = c.hotkey_at() {
            let _ = write!(out, " (key letter {at})");
        }
    }

    if let Some(tip) = &c.tooltip {
        let _ = write!(out, "  tip {tip:?}");
    }

    out
}

/// Control `c`'s colours for `role` at rest, lit and disabled, as
/// resolved: `rrggbb`, or `-` for none.
fn resolved(c: &ShownControl, role: Role) -> String {
    [State::Normal, State::Lit, State::Disabled]
        .map(|state| {
            c.inks.colour(role, state).map_or_else(
                || "-".to_owned(),
                |[r, g, b]| format!("{r:02x}{g:02x}{b:02x}"),
            )
        })
        .join(",")
}

fn hex(words: &[i32]) -> String {
    words
        .iter()
        .map(|w| format!("{:08x}", w.cast_unsigned()))
        .collect::<Vec<_>>()
        .join(",")
}

/// A quest's briefing pictures, video and voice.
fn briefing(
    catalogue: &Catalogue<'_>,
    code: &str,
    sets: Datasets,
    dump: Option<&Path>,
) -> Result<(), String> {
    let code = util::fourcc(code)?;
    let release = match sets {
        Datasets::Base => Release::Base,
        Datasets::Expansion => Release::Expansion,
    };

    println!("\n== quest {code}  slot {:?}", quest_map::slot(code));
    println!("   video {:?}", menu::video(code).map(|v| v.to_string()));
    println!("   voice {:?}", menu::voice(code).map(|v| v.to_string()));

    for (name, art) in [
        ("zoom", menu::zoom(code, release, World::Quest)),
        ("icon", menu::icon(code, release, World::Quest)),
    ] {
        let Some((record, set)) = art else {
            println!("   {name} none");
            continue;
        };

        let found = menu::still(catalogue, record, i32::from(set));
        let size = match &found {
            Some(Ok(s)) => format!("{}x{}", s.width, s.height),
            Some(Err(e)) => e.to_string(),
            None => "missing".to_owned(),
        };

        println!("   {name} {record} {set}  {size}");

        if let (Some(dir), Some(Ok(sprite))) = (dump, found) {
            write_pam(&dir.join(format!("{code}_{name}.pam")), &sprite)?;
        }
    }

    Ok(())
}

/// One picture asked for as `RECORD:SET`: its size and where it lives.
fn still(catalogue: &Catalogue<'_>, asked: &str, dump: Option<&Path>) -> Result<(), String> {
    let mut parts = asked.split(':');
    let (Some(record), Some(set)) = (parts.next(), parts.next()) else {
        return Err(format!("{asked}: not RECORD:SET[:SLOT]"));
    };
    let record = util::fourcc(record)?;
    let set: i32 = set.parse().map_err(|e| format!("{asked}: {e}"))?;
    let slot: usize = parts
        .next()
        .map_or(Ok(0), str::parse)
        .map_err(|e| format!("{asked}: {e}"))?;
    let archive = catalogue
        .find(b"IMAG", record)
        .map_or("?", |at| catalogue.archives[at.archive].0);

    match menu::state(catalogue, record, set, slot) {
        Some(Ok(sprite)) => {
            println!(
                "\n== {record} {set} slot {slot}  {}x{}  in {archive}",
                sprite.width, sprite.height
            );

            if let Some(dir) = dump {
                let name = format!("{}_{set}_{slot}.pam", record.trimmed());

                std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
                write_pam(&dir.join(name), &sprite)?;
            }
        }
        Some(Err(e)) => println!("\n== {record} {set}  {e}"),
        None => println!("\n== {record} {set}  missing"),
    }

    Ok(())
}

/// Every picture a panel names, as `<panel>_<record>_<set>.pam`.
fn pictures(catalogue: &Catalogue<'_>, layout: &Layout, dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;

    let all = std::iter::once(&layout.panel).chain(&layout.controls);

    for (record, set) in all.filter_map(|c| c.art) {
        let Some(Ok(sprite)) = menu::still(catalogue, record, set) else {
            println!("   {record} {set}: no picture");
            continue;
        };

        let name = format!("{}_{}_{set}.pam", layout.id, record.trimmed());

        write_pam(&dir.join(name), &sprite)?;
    }

    Ok(())
}

/// A picture as a PAM (`P7`, `RGB_ALPHA`): no image crate needed.
fn write_pam(path: &Path, sprite: &regent_core::view::Sprite) -> Result<(), String> {
    let header = format!(
        "P7\nWIDTH {}\nHEIGHT {}\nDEPTH 4\nMAXVAL 255\nTUPLTYPE RGB_ALPHA\nENDHDR\n",
        sprite.width, sprite.height
    );
    let mut bytes = header.into_bytes();

    bytes.extend(sprite.rgba());
    std::fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))
}
