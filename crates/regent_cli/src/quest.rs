//! The RGS layer: generator charts (`.rgs`) and quest maps (`.q`).
//!
//! A `.q` names chart patterns, never terrain classes or objects directly;
//! patterns name those, sometimes through other patterns.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use regent_core::data::catalogue::Catalogue;
use regent_core::data::fourcc::FourCc;
use regent_core::data::rgs::{self, Chart, chart::Layer};

use super::util::{self, gather_all};

/// Print each chart's patterns, layer by layer.
///
/// # Errors
///
/// If a path will not read, or a chart will not parse.
pub fn cmd_chart(
    resolve: Option<&str>,
    code: Option<&str>,
    paths: &[String],
) -> Result<bool, String> {
    let files: Vec<PathBuf> = gather_all(paths, "rgs")?;

    // Optional: archives only add names to references.
    let library = resolve
        .map(|at| util::library(&[at.to_owned()]))
        .transpose()?;
    let catalogue = library
        .as_ref()
        .map(|l| l.catalogue().map_err(|e| e.to_string()))
        .transpose()?;

    let mut failed = 0;

    for path in files {
        let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let name = path
            .file_name()
            .unwrap_or(path.as_os_str())
            .to_string_lossy();

        let file = match rgs::File::parse(&bytes) {
            Ok(file) => file,
            Err(e) => {
                eprintln!("{name}: {e}");
                failed += 1;
                continue;
            }
        };

        match file.chart() {
            Ok(chart) => show(&name, file.tag, &chart, code, catalogue.as_ref()),
            Err(e) => {
                eprintln!("{name}: {e}");
                failed += 1;
            }
        }
    }

    Ok(failed == 0)
}

/// One chart: its sections, and what each pattern is made of.
fn show(
    name: &str,
    tag: FourCc,
    chart: &Chart<'_>,
    only: Option<&str>,
    catalogue: Option<&Catalogue<'_>>,
) {
    let codes: BTreeSet<FourCc> = chart.patterns().map(|(_, p)| p.code).collect();

    println!(
        "== {name}  {tag}  {} patterns, {} height profiles",
        chart.patterns().count(),
        chart.profiles.len()
    );

    for layer in Layer::ALL {
        let patterns = chart.layer(layer);

        println!("   -- {} ({} records)", layer.name(), patterns.len());

        for pattern in patterns {
            if only.is_some_and(|want| !want.eq_ignore_ascii_case(&pattern.code.to_string())) {
                continue;
            }

            let entries: Vec<String> = pattern
                .entries
                .iter()
                .map(|e| format!("{}:{}{}", e.id, e.weight, mark(e.id, &codes, catalogue)))
                .collect();

            println!(
                "   {}  {:<34} {}",
                pattern.code,
                String::from_utf8_lossy(pattern.name),
                entries.join("  ")
            );
        }
    }

    println!("   -- height profiles");

    for profile in &chart.profiles {
        println!(
            "   {}  {:<34} {:?}",
            profile.code,
            String::from_utf8_lossy(profile.name),
            profile.words
        );
    }
}

/// A reference's name from the archives. Empty for local codes or without
/// archives; `(!)` when it resolves to nothing.
fn mark(id: FourCc, codes: &BTreeSet<FourCc>, catalogue: Option<&Catalogue<'_>>) -> String {
    // Empty slot; the terrain layer pads each record to five.
    if id == FourCc::new(b"NONE") {
        return String::new();
    }

    if codes.contains(&id) {
        return String::new();
    }

    let Some(catalogue) = catalogue else {
        return String::new();
    };

    for tag in [b"DTPT", b"DUNT"] {
        if let Some(at) = catalogue.find(tag, id)
            && let Some(record) = catalogue.record(tag, at)
            && let Ok(part) = regent_core::data::desc::Part::parse(record.data)
            && let Ok(head) =
                regent_core::data::desc::Head::parse(part.find(b"HEAD").map_or(&[][..], |p| p.body))
        {
            return format!("({})", String::from_utf8_lossy(head.name));
        }
    }

    "(!)".to_owned()
}

/// The `quest` command line.
#[derive(clap::Args)]
pub struct Args {
    /// An .rgs chart or directory, so references show their names
    #[arg(long, value_name = "PATH")]
    resolve: Option<String>,
    /// Also print unit patterns (the freestyle setups)
    #[arg(long)]
    units: bool,
    /// With --units, also print placements and lairs
    #[arg(long)]
    placements: bool,
    /// Print each freestyle land: zones, seeds, terrain classes.
    /// Classes need --resolve
    #[arg(long)]
    lands: bool,
    paths: Vec<String>,
}

/// How much of a map's unit patterns to print.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Units {
    Hidden,
    /// Each pattern, one line.
    Listed,
    /// Each pattern, and its placements and lairs.
    Placed,
}

impl Units {
    /// What `--units` and `--placements` ask for.
    const fn of(args: &Args) -> Self {
        match (args.units, args.placements) {
            (false, _) => Units::Hidden,
            (true, false) => Units::Listed,
            (true, true) => Units::Placed,
        }
    }
}

/// Print each quest map: its size, its zones, and what they name.
///
/// # Errors
///
/// If a path will not read, or a map will not parse.
pub fn cmd_quest(args: &Args) -> Result<bool, String> {
    let files: Vec<PathBuf> = gather_all(&args.paths, "q")?;

    // Charts, to name a zone's three references.
    let library = args
        .resolve
        .as_ref()
        .map(|at| {
            let found = gather_all(std::slice::from_ref(at), "rgs")?;

            found
                .iter()
                .map(|p| std::fs::read(p).map_err(|e| format!("{}: {e}", p.display())))
                .collect::<Result<Vec<Vec<u8>>, String>>()
        })
        .transpose()?
        .unwrap_or_default();

    let charts: Vec<Chart<'_>> = library
        .iter()
        .filter_map(|b| rgs::File::parse(b).ok()?.chart().ok())
        .collect();

    let mut known: BTreeMap<FourCc, String> = BTreeMap::new();

    for bytes in &library {
        if let Ok(chart) = rgs::File::parse(bytes).and_then(|f| f.chart()) {
            for (_, pattern) in chart.patterns() {
                known
                    .entry(pattern.code)
                    .or_insert_with(|| String::from_utf8_lossy(pattern.name).into_owned());
            }

            for profile in &chart.profiles {
                known
                    .entry(profile.code)
                    .or_insert_with(|| String::from_utf8_lossy(profile.name).into_owned());
            }
        }
    }

    let mut failed = 0;

    for path in files {
        let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let name = path
            .file_name()
            .unwrap_or(path.as_os_str())
            .to_string_lossy();

        match rgs::File::parse(&bytes).and_then(|f| f.map()) {
            Ok(map) => {
                show_map(&name, &map, &known, Units::of(args));

                if args.lands {
                    show_lands(&map, &charts);
                }
            }
            Err(e) => {
                eprintln!("{name}: {e}");
                failed += 1;
            }
        }
    }

    Ok(failed == 0)
}

fn show_map(name: &str, map: &rgs::Map<'_>, known: &BTreeMap<FourCc, String>, units: Units) {
    let look = |id: FourCc| -> String {
        known
            .get(&id)
            .map_or_else(|| id.to_string(), |n| format!("{id} {n}"))
    };

    println!(
        "== {name}  {}  {}  {}x{}  {} zones, {} regions, {} spawners",
        map.tag,
        String::from_utf8_lossy(map.quest),
        map.size.0,
        map.size.1,
        map.zones.len(),
        map.used_regions().count(),
        map.spawners.len(),
    );
    println!(
        "   land {}  template {}  words {:?}  trailer {:?}",
        map.code, map.pattern, map.words, map.trailer
    );

    for zone in &map.zones {
        println!(
            "   {} {:<26} {:<30} {:<28} {}",
            zone.code,
            String::from_utf8_lossy(zone.name),
            look(zone.landscape),
            look(zone.surface),
            look(zone.height),
        );
    }

    if units != Units::Hidden {
        println!("   -- slots ({} records)", map.slots.len());

        for slot in &map.slots {
            println!(
                "   {:>5}  {:<38} active {} gold {} word {} items {:?}",
                slot.key,
                String::from_utf8_lossy(slot.name),
                slot.active,
                slot.gold,
                slot.word,
                slot.items,
            );
        }

        println!("   -- regions ({} named)", map.used_regions().count());

        for region in map.used_regions() {
            let team = region.team.as_ref();

            println!(
                "   {}  {}",
                region.code,
                team.map_or_else(String::new, |t| format!(
                    "{} {} active {} expanding {}  zones {}",
                    t.code,
                    String::from_utf8_lossy(t.name),
                    t.active,
                    t.expanding,
                    // `id:level:words`; on a land, level is the zone's map
                    // share (`regent_core::sim::mapgen::recipe`).
                    t.items
                        .iter()
                        .map(|i| format!("{}:{}:{:?}", i.id, i.level, i.words))
                        .collect::<Vec<_>>()
                        .join(" "),
                )),
            );
        }

        show_units(map, &look, units);
    }
}

/// The two unit-pattern sets.
///
/// In `mx_Freestyle.q` the custom-game dialog's lists are unit patterns, keyed
/// by the codes a save's setup block holds: `Wizb`, `Ruib`, `Rave`.
fn show_units(map: &rgs::Map<'_>, look: &dyn Fn(FourCc) -> String, units: Units) {
    for (which, set) in map.units.iter().enumerate() {
        println!("   -- unit patterns, set {which} ({} records)", set.len());

        for unit in set {
            println!(
                "   {} {}  {:<38} alt {}  {:?}  {}  {} placements, {} lairs  sizes {:?} trailer {:?} flag {:?}",
                unit.faction,
                unit.code,
                String::from_utf8_lossy(unit.name),
                unit.alternate,
                unit.words,
                unit.ground,
                unit.placements.len(),
                unit.spawners.len(),
                unit.sizes,
                unit.trailer,
                unit.flag,
            );

            if units != Units::Placed {
                continue;
            }

            for place in &unit.placements {
                println!(
                    "        place {:<28} {:>6}  cells {:?}",
                    look(place.id),
                    place.word,
                    place.cells,
                );
            }

            for (key, spawner) in &unit.spawners {
                println!(
                    "        lair  {key} {:<24} {:?} team {} {}",
                    String::from_utf8_lossy(spawner.team.name),
                    spawner.words,
                    spawner.team.code,
                    spawner
                        .team
                        .items
                        .iter()
                        .map(|i| format!("{}:{}:{:?}", i.id, i.level, i.words))
                        .collect::<Vec<_>>()
                        .join(" "),
                );
            }
        }
    }
}

/// Each freestyle land, as the map generator takes it in.
///
/// Per zone: region-chart seeds, map share, and terrain classes unique to it.
/// A zone with no unique class cannot be checked against a save's ground.
fn show_lands(map: &rgs::Map<'_>, charts: &[Chart<'_>]) {
    println!("   -- lands");

    for region in map.used_regions() {
        let Some(land) = regent_core::sim::mapgen::recipe::Land::of(map, region.code) else {
            continue;
        };

        let painted: Vec<BTreeSet<u8>> = land
            .zones
            .iter()
            .map(|z| {
                charts
                    .iter()
                    .map(|c| z.classes(c))
                    .find(|s| !s.is_empty())
                    .unwrap_or_default()
            })
            .collect();

        let seeds: u32 = land.zones.iter().map(|z| z.seeds).sum();
        let tellable = painted
            .iter()
            .enumerate()
            .filter(|(i, mine)| {
                let others: BTreeSet<u8> = painted
                    .iter()
                    .enumerate()
                    .filter(|(j, _)| j != i)
                    .flat_map(|(_, s)| s.iter().copied())
                    .collect();

                mine.difference(&others).any(|c| *c != 0)
            })
            .count();

        println!(
            "   {}  {:<28} {:>3} seeds  {}/{} zones tell apart",
            land.code,
            String::from_utf8_lossy(land.name),
            seeds,
            tellable,
            land.zones.len()
        );

        for (zone, classes) in land.zones.iter().zip(&painted) {
            println!(
                "      {}  {:<26} {:>2} seeds  share {:>3}  classes {:?}",
                zone.code,
                String::from_utf8_lossy(zone.name),
                zone.seeds,
                zone.weight,
                classes
            );
        }
    }
}
