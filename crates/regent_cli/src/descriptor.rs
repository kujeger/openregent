//! The `D*` descriptor payloads: units, sounds and actions.
//!
//! `desc_unit`, `desc_tail` and `head_names` are `pub(crate)` for
//! [`super::view`], which prints the same stat block from a catalogue.

use std::collections::BTreeMap;

use std::path::PathBuf;

use regent_core::content::dataset::{Bytecode, Datasets, IMAGE_SETS, Install};
use regent_core::data::catalogue::Library;
use regent_core::data::desc::xml;
use regent_core::data::{cam, desc, payload};

use crate::util::{inputs, name, read_bytes};

/// Print the fields a unit descriptor carries.
pub(crate) fn desc_unit(unit: &desc::Unit<'_>) {
    let show = |name: &str, v: Option<u32>| {
        if let Some(v) = v {
            println!("      {name:<18} {v}");
        }
    };

    println!("      {:<18} {:?}", "kind", unit.kind());
    show("menu", unit.menu());

    if let Some(id) = unit.image_id() {
        println!("      {:<18} {id}", "image");
    }

    let info: Vec<&str> = unit.engine().info_flags().map(desc::Info::name).collect();

    if let Some(raw) = unit.engine().info() {
        println!("      {:<18} {raw:#010x}  {}", "info", info.join(" "));
    }

    if let Some(c) = unit.character() {
        let flags: Vec<&str> = c.flag_names().map(desc::CharacterFlag::name).collect();

        if let Some(raw) = c.flags() {
            println!("      {:<18} {raw:#010x}  {}", "flags", flags.join(" "));
        }

        if let Some(d) = c.dialog_id() {
            println!("      {:<18} {d}", "dialog");
        }

        show("max hp", c.max_hp());
        show("attack", c.attack());

        if let (Some(lo), Some(hi)) = (c.attack_range_min(), c.attack_range_max()) {
            println!("      {:<18} {lo}..{hi}", "attack range");
        }

        show("parry", c.parry());
        show("dodge", c.dodge());
        show("speed", c.speed());
        show("strength", c.strength());
        show("intelligence", c.intelligence());
        show("sight range", c.sight_range());
        show("experience", c.experience());
        show("gold", c.gold());
        show("recruit delay", c.recruit_delay());
    } else if let Some(b) = unit.building() {
        let flags: Vec<&str> = b.flag_names().map(desc::BuildingFlag::name).collect();

        if let Some(raw) = b.flags() {
            println!("      {:<18} {raw:#010x}  {}", "flags", flags.join(" "));
        }

        if let Some(d) = b.dialog_id() {
            println!("      {:<18} {d}", "dialog");
        }

        show("max hp", b.max_hp());
        show("cost", b.cost());

        if let Some(to) = b.upgrade_to() {
            println!("      {:<18} {to}", "upgrade to");
        }

        show("sight range", b.sight_range());
        show("guild members", b.max_guild_members());
        show("income type", b.income_type());

        if let Some(v) = b.income_amount() {
            println!("      {:<18} {v}", "income");
        }

        show("gold", b.gold());
    } else if let Some(ps) = unit.particle_system() {
        desc_particles(&ps);
    } else {
        show("stack priority", unit.stack_priority());

        if unit.transparent_to_mouse() {
            println!("      {:<18} TransparentToMouse", "flags");
        }
    }
}

/// Print one sound's phases, resolving ids through `DSDP` and `DSDG`.
pub fn cmd_sound(wanted: Option<&str>, paths: &[String]) -> Result<bool, String> {
    for p in &inputs(paths, "cam")? {
        let data = read_bytes(p)?;
        let archive = cam::Archive::parse(&data).map_err(|e| format!("{p}: {e}"))?;

        let phase_names = head_names(&archive, *b"DSDP");
        let group_names = head_names(&archive, *b"DSDG");
        let (mut records, mut phases, mut unresolved) = (0, 0, 0);

        println!("== {}", name(p));

        for (chunk, r) in archive.records() {
            if chunk.tag != b"DSND" {
                continue;
            }

            let part = desc::Part::parse(r.data).map_err(|e| format!("{p} {}: {e}", r.id))?;
            let sound = desc::Sound::parse(&part).map_err(|e| format!("{p} {}: {e}", r.id))?;

            records += 1;
            phases += sound.phase_count();

            if wanted.as_ref().is_none_or(|w| *w != r.id.to_string()) {
                continue;
            }

            println!(
                "  DSND {} {:<20} category {}, {} phases",
                r.id,
                r.name(),
                sound.category().unwrap_or(0),
                sound.phase_count()
            );

            for ph in sound.phases() {
                let look = |id: Option<regent_core::data::fourcc::FourCc>,
                            t: &BTreeMap<String, String>| {
                    id.map_or_else(
                        || "-".to_owned(),
                        |f| {
                            t.get(&f.to_string())
                                .cloned()
                                .unwrap_or_else(|| f.to_string())
                        },
                    )
                };

                if ph
                    .id()
                    .is_some_and(|f| !phase_names.contains_key(&f.to_string()))
                {
                    unresolved += 1;
                }

                let flags: Vec<&str> = ph.flag_names().map(desc::PhaseFlag::name).collect();

                println!(
                    "      {:<18} wave {}  group {:<20} {}",
                    look(ph.id(), &phase_names),
                    ph.wave().map_or_else(|| "-".to_owned(), |f| f.to_string()),
                    look(ph.group(), &group_names),
                    flags.join(" ")
                );

                if let Some((up, down)) = ph.volume() {
                    println!("          volume    +{up} -{down}");
                }

                if let Some((up, down)) = ph.frequency() {
                    println!("          frequency +{up} -{down}");
                }

                if let Some(d) = ph.distance() {
                    println!("          distance  {d}");
                }
            }
        }

        println!(
            "   {records} sounds, {phases} phases, {} phase ids and {} groups to resolve against",
            phase_names.len(),
            group_names.len()
        );

        if unresolved > 0 {
            println!("   {unresolved} phase references do not resolve");
        }
    }

    Ok(true)
}

/// Every record of a chunk, as id to `HEAD` name.
pub(crate) fn head_names(archive: &cam::Archive<'_>, tag: [u8; 4]) -> BTreeMap<String, String> {
    archive
        .records()
        .filter(|(chunk, _)| chunk.tag == &tag)
        .filter_map(|(_, r)| {
            let part = desc::Part::parse(r.data).ok()?;
            let head = desc::Head::parse(part.find(b"HEAD")?.body).ok()?;

            Some((
                r.id.to_string(),
                String::from_utf8_lossy(head.name).into_owned(),
            ))
        })
        .collect()
}

/// Print a particle system's emitter and particle settings.
fn desc_particles(ps: &desc::ParticleSystem<'_>) {
    let e = ps.emitter();
    let p = ps.particle();

    let shapes: Vec<&str> = e.shapes().map(desc::EmitterShape::name).collect();
    let flags: Vec<&str> = e.flags().map(desc::EmitterFlag::name).collect();

    println!("      {:<18} {}", "emitter", shapes.join("+"));

    if !flags.is_empty() {
        println!("      {:<18} {}", "emitter flags", flags.join(" "));
    }

    if let (Some(i), Some(m)) = (ps.initial_particles(), ps.max_particles()) {
        println!("      {:<18} {i} initial, {m} max", "particles");
    }

    if let (Some(r), Some(rate)) = (e.radius(), e.rate()) {
        println!("      {:<18} radius {r}, rate {rate}/s", "emission");
    }

    // Both lifespans in ms; emitter `u32::MAX` never expires.
    if let (Some(el), Some(pl)) = (e.lifespan(), p.lifespan()) {
        let el = if el == u32::MAX {
            "forever".to_owned()
        } else {
            format!("{el}ms")
        };

        println!("      {:<18} emitter {el}, particle {pl}ms", "lifespan");
    }

    if let (Some(s), Some(m), Some(d)) = (p.birth_size(), p.max_size(), p.death_size()) {
        println!("      {:<18} {s} -> {m} -> {d}", "size");
    }

    if let Some(g) = p.gravity() {
        println!("      {:<18} {} {} {}", "gravity", g.0, g.1, g.2);
    }

    // RGBA at birth, midlife, death; the alpha ramp draws as mode 8.
    let colours = [p.birth_colour(), p.midlife_colour(), p.death_colour()];

    for (name, c) in ["birth colour", "midlife colour", "death colour"]
        .iter()
        .zip(colours)
    {
        if let Some(c) = c {
            println!("      {name:<18} {c:?}");
        }
    }

    if let Some((fade_in, fade_out)) = p.fade_time() {
        println!("      {:<18} in {fade_in}, out {fade_out}", "fade time");
    }
}

/// Print `PRIM`'s tail: the GPL callback, the attachments and the sounds.
///
/// Ids resolve through `DMOV`, `DSND` and `DACT` names where available.
pub(crate) fn desc_tail(unit: &desc::Unit<'_>, names: &BTreeMap<String, String>) {
    let tail = unit.tail();

    let look = |id: regent_core::data::fourcc::FourCc| {
        names
            .get(&id.to_string())
            .cloned()
            .unwrap_or_else(|| id.to_string())
    };

    if !tail.script.is_empty() {
        println!(
            "      {:<18} {}",
            "script",
            String::from_utf8_lossy(tail.script)
        );
    }

    if let Some((how, id)) = tail.attachments.and_then(|a| a.movement_class()) {
        let how = how.map_or_else(|| "?".to_owned(), |m| m.name().to_owned());
        println!("      {:<18} {} ({how})", "movement", look(id));
    }

    for o in tail.overrides() {
        println!(
            "      {:<18} {} on {}",
            "sound override",
            look(o.sound),
            look(o.action)
        );
    }

    if let Some(s) = tail.default_sound {
        println!("      {:<18} {}", "default sound", look(s));
    }
}

/// Print the terrain descriptor's class table.
///
/// Indices are printed: they are the `MapTexture` letters, and the expansion's
/// records are out of table order.
fn desc_terrain(terrain: &desc::Terrain<'_>) {
    println!(
        "      {:<18} {} of {}",
        "texture classes",
        terrain.classes().count(),
        terrain.capacity()
    );

    for (letter, id) in terrain.classes() {
        println!("      {:<18} {id}", char::from(b'A' + letter));
    }
}

/// Print the fields an action descriptor carries.
fn desc_action(action: &desc::Action<'_>) {
    let show = |name: &str, v: Option<u32>| {
        if let Some(v) = v {
            println!("      {name:<18} {v}");
        }
    };

    if let Some(set) = action.image_set() {
        println!("      {:<18} {set:#06x}", "image set");
    }

    if let Some(after) = action.after_set() {
        println!("      {:<18} {after}", "after set");
    }

    show("effector duration", action.effector_duration());
    show("timeout duration", action.timeout_duration());
    show("character level", action.character_level());
    show("spell rank", action.spell_rank());

    let s = action.script();

    if !s.function.is_empty() {
        println!(
            "      {:<18} {}",
            "script",
            String::from_utf8_lossy(s.function)
        );
    }
}

/// Dump the descriptor tree, and the decoded fields for a named record.
pub fn cmd_desc(wanted: Option<&str>, paths: &[String]) -> Result<bool, String> {
    let paths = &inputs(paths, "cam")?;
    let mut failed = 0;

    // Collect names across all files first. `unittype.cam` sound and action
    // ids need `sounddesc.cam` and `action.cam` on the command line.
    let mut names = BTreeMap::new();

    for p in paths {
        let data = read_bytes(p)?;
        let archive = cam::Archive::parse(&data).map_err(|e| format!("{p}: {e}"))?;

        for tag in [b"DMOV", b"DSND", b"DACT"] {
            names.extend(head_names(&archive, *tag));
        }
    }

    for p in paths {
        let data = read_bytes(p)?;
        let archive = cam::Archive::parse(&data).map_err(|e| format!("{p}: {e}"))?;
        let (mut records, mut parts) = (0, 0);

        println!("== {}", name(p));

        for (chunk, r) in archive.records() {
            if !payload::DESCRIPTOR_TAGS.contains(&chunk.tag.as_bytes()) {
                continue;
            }

            let id = r.id.to_string();

            if wanted.as_ref().is_some_and(|w| *w != id) {
                continue;
            }

            let part = match desc::Part::parse(r.data) {
                Ok(part) => part,
                Err(e) => {
                    eprintln!("  {id}: {e}");
                    failed += 1;
                    continue;
                }
            };

            records += 1;
            parts += part.walk().count();

            let shape: Vec<String> = part
                .parts
                .iter()
                .flat_map(|d| {
                    d.parts
                        .iter()
                        .map(|c| format!("{} v{} {}B", c.tag, c.version, c.body.len()))
                })
                .collect();

            println!("  {} {id} {:<18} {}", chunk.tag, r.name(), shape.join("  "));

            if wanted.is_some() {
                if let Ok(head) = part.find(b"HEAD").map_or(
                    Err(regent_core::data::Error::BadValue {
                        what: "missing HEAD",
                        value: 0,
                    }),
                    |h| desc::Head::parse(h.body),
                ) {
                    println!(
                        "      {:<18} {}",
                        "name",
                        String::from_utf8_lossy(head.name)
                    );
                }

                if chunk.tag == b"DUNT"
                    && let Ok(unit) = desc::Unit::parse(&part)
                {
                    desc_unit(&unit);
                    desc_tail(&unit, &names);
                }

                if chunk.tag == b"DACT"
                    && let Ok(action) = desc::Action::parse(&part)
                {
                    desc_action(&action);
                }

                if chunk.tag == b"DTRN"
                    && let Ok(terrain) = desc::Terrain::parse(&part)
                {
                    desc_terrain(&terrain);
                }
            }
        }

        println!("   {records} records, {parts} parts");
    }

    if failed > 0 {
        eprintln!("{failed} descriptors failed to decode");
    }

    Ok(failed == 0)
}

/// The build-tree variants: what each one caps, and to how many.
///
/// Five records in one archive; all that `BuildTreeEditor_Controller` edits.
/// Building ids resolve against any `DUNT` archive given.
///
/// # Errors
///
/// If a path will not read, or an archive will not parse.
pub fn cmd_variants(paths: &[String]) -> Result<bool, String> {
    let mut names = BTreeMap::new();
    let mut loaded = Vec::new();

    for p in inputs(paths, "cam")? {
        let data = read_bytes(&p)?;
        loaded.push((p, data));
    }

    for (p, data) in &loaded {
        let archive = cam::Archive::parse(data).map_err(|e| format!("{p}: {e}"))?;

        for (chunk, record) in archive.records() {
            if chunk.tag.as_bytes() == b"DUNT" {
                names.insert(record.id, record.name().into_owned());
            }
        }
    }

    let (mut found, mut failed) = (0, 0);

    for (p, data) in &loaded {
        let archive = cam::Archive::parse(data).map_err(|e| format!("{p}: {e}"))?;

        for (chunk, record) in archive.records() {
            if chunk.tag.as_bytes() != b"BTDT" {
                continue;
            }

            let variant = match payload::Variant::parse(record.data) {
                Ok(v) => v,
                Err(e) => {
                    println!("   {} {}: {e}", record.id, record.name());
                    failed += 1;
                    continue;
                }
            };

            found += 1;
            println!(
                "== {} v{}  {} building{}",
                variant.name,
                variant.version,
                variant.len(),
                if variant.len() == 1 { "" } else { "s" }
            );

            for (id, cap) in variant.caps() {
                let called = names.get(&id).cloned().unwrap_or_default();
                let how = if cap == 0 {
                    "none allowed".to_owned()
                } else {
                    format!("at most {cap}")
                };

                println!("   {id}  {called:<24} {how}");
            }
        }
    }

    if found == 0 {
        println!("no BTDT records in the archives given");
    }

    Ok(failed == 0)
}

/// `paths`, XML `<Descriptions>` files in load order, made into records
/// over HD's expansion archives (`data::desc::xml`): one line each, what
/// HD would skip, and the archive written to `out` where given.
///
/// # Errors
///
/// No HD install, or archives that will not read.
pub fn cmd_desc_xml(out: Option<&str>, paths: &[String]) -> Result<bool, String> {
    let hd = crate::env::path(crate::env::HD).map_err(|e| e.to_string())?;
    let files = Install::new(&hd, Bytecode::Install)
        .files(Datasets::Expansion)
        .map_err(|e| e.to_string())?;
    let library = Library::from_files(&files.archives).map_err(|e| e.to_string())?;
    let catalogue = library.catalogue().map_err(|e| e.to_string())?;
    let listed: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
    let sources = xml::Sources::read(&listed);
    let (_, unread) = sources.parsed();
    let compiled = sources.compile(&catalogue, &xml::ImageSets::read(&hd.join(IMAGE_SETS)));

    for path in &unread {
        println!("unread  {}", path.display());
    }

    for r in &compiled.records {
        let shipped = if catalogue.find(&r.set.tag(), r.id).is_some() {
            "replaces"
        } else {
            "new"
        };

        println!(
            "{:<6} {:<12} {:<9} {}",
            format!("{:?}", r.set),
            r.id,
            shipped,
            r.name
        );
    }

    for r in &compiled.refused {
        println!("refused {:?} {:?}", r.id, r.why);
    }

    if let Some(out) = out {
        std::fs::write(out, compiled.archive()).map_err(|e| format!("{out}: {e}"))?;
    }

    Ok(unread.is_empty())
}
