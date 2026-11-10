//! HD's quest map, `APdb`: where each quest sits and what the title bar
//! shows for it. `regent_core::view::menu::quest_map`.

use std::path::{Path, PathBuf};

use regent_core::data::cam::Archive;
use regent_core::data::catalogue::Library;
use regent_core::data::deps::{self, Quest};
use regent_core::data::fourcc::FourCc;
use regent_core::data::smnu::Rect;
use regent_core::data::text::Text;
use regent_core::view::menu::quest_map::{self, Art, Placed};

use super::env;

/// The archives the texts come from: `GMTX`, `QUES`, `QDEP`.
const TEXTS: [&str; 3] = [
    "DataMX/mx_textdata.cam",
    "DataMX/mx_gpltext.cam",
    "DataMX/mx_miscdata.cam",
];

/// The layout the HD captures were taken at.
const LAYOUT: &str = "Data/UIData_1920_1200.dat";

/// Print the map, the frame and every slot.
///
/// # Errors
///
/// If `MAJESTY_HD` is unset or its files will not read.
pub fn cmd_quest_map(layout: Option<&str>, screen: (i32, i32)) -> Result<bool, String> {
    let hd = env::path(env::HD).map_err(|e| e.to_string())?;
    let file = layout.map_or_else(|| hd.join(LAYOUT), PathBuf::from);
    let panel = quest_map::load(&file)?;
    let library = Library::from_files(&TEXTS.map(|a| hd.join(a))).map_err(|e| e.to_string())?;
    let catalogue = library.catalogue().map_err(|e| e.to_string())?;
    let text = Text::build(&catalogue);
    let rules = rules(&hd.join(TEXTS[2]))?;
    let name = |q: FourCc| {
        text.quest(q)
            .map_or_else(|| "-".to_owned(), std::borrow::Cow::into_owned)
    };

    println!("{}", file.display());

    if let Some(map) = quest_map::map(&panel) {
        println!("map        {}", placed(&map));
    }
    if let Some((w, h)) = quest_map::frame_size(&panel) {
        println!("frame      {w}x{h}");
    }
    if let Some((frame, map)) = quest_map::origin(&panel, screen) {
        println!("at {}x{}  frame {frame:?}, map {map:?}", screen.0, screen.1);
    }

    println!("background {} tiled", art(quest_map::BACKGROUND));
    println!();

    for (id, piece) in quest_map::frame(&panel) {
        println!("fixed {id:5}  {}", placed(&piece));
    }

    println!();

    for site in quest_map::sites(&panel) {
        let rule = rules.iter().find(|r| r.id == site.quest);
        let difficulty = rule.map_or_else(
            || "-".to_owned(),
            |r| {
                text.gmtx(quest_map::difficulty_text(r.difficulty))
                    .map_or_else(String::new, std::borrow::Cow::into_owned)
            },
        );
        let requires: Vec<String> = rule
            .and_then(|r| r.requires.as_ref())
            .map(|e| e.ids().into_iter().map(name).collect())
            .unwrap_or_default();

        println!(
            "slot {:2} {} {}",
            site.slot,
            site.quest.trimmed(),
            name(site.quest)
        );
        println!("   difficulty {difficulty}");

        if !requires.is_empty() {
            println!("   requires   {}", requires.join(", "));
        }

        println!("   hit        {}", placed(&site.hit));
        println!("   lit        {}", optional(site.lit.as_ref()));
        println!("   star       {}", optional(site.star.as_ref()));
        println!("   cloud      {}", optional(site.cloud.as_ref()));
        println!(
            "   pog        {}",
            quest_map::pog(site.slot).map_or_else(|| "-".to_owned(), art)
        );
        println!(
            "   video      {}",
            quest_map::video(site.slot).map_or_else(|| "-".to_owned(), |v| v.trimmed().to_owned())
        );
    }

    Ok(true)
}

/// The quest rules' record in the expansion's `mx_miscdata.cam`.
const QUEST_RULES: FourCc = FourCc(*b"QDEP");

/// `QDEP` from the expansion's `mx_miscdata.cam`.
fn rules(path: &Path) -> Result<Vec<Quest>, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let archive = Archive::parse(&bytes).map_err(|e| e.to_string())?;
    let source = archive
        .records()
        .find(|(_, r)| r.id == QUEST_RULES)
        .map(|(_, r)| String::from_utf8_lossy(r.data).into_owned())
        .ok_or("no QDEP")?;

    deps::quests(&source).map_err(|e| e.to_string())
}

fn art(art: Art) -> String {
    format!("{} {}", art.record.trimmed(), art.set)
}

fn rect(r: Rect) -> String {
    format!("({}, {}) {}x{}", r.x, r.y, r.width, r.height)
}

fn placed(p: &Placed) -> String {
    let picture = p.art.map_or_else(|| "no picture".to_owned(), art);
    let tiled = if p.tiled() { ", tiled" } else { "" };

    format!("{} {picture}{tiled}", rect(p.rect))
}

fn optional(p: Option<&Placed>) -> String {
    p.map_or_else(|| "-".to_owned(), placed)
}
