//! Step 3: every image loads as the engine loads it, and every call resolves.
//!
//! - HD's base game and expansion, from its dataset definitions;
//! - the `datasets.xml` groups;
//! - every Workshop image, over HD's expansion;
//! - images compiled from source decode to the shipped program.
//!
//! Needs `MAJESTY_HD` and `MAJESTY_WORKSHOP`.

mod common;

use common::read;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use regent_core::content::dataset::{self, Bytecode, Datasets, Install};
use regent_core::data::attrib;
use regent_core::data::bcd::Container;
use regent_core::data::cam::Archive;
use regent_core::data::gpltype::TypeCode;
use regent_core::gpl::bcd::{ScopeSection, walk};
use regent_core::gpl::exec::{self, Callee, Definition, Dest, Image, Kind, Literal, Names, Target};
use regent_core::gpl::load::{Missing, Program};
use regent_core::gpl::opcode::Op;
use regent_core::gpl::{builtins, emit};

/// The one `#Name` no script defines and no attribute is:
/// Attribute register gives it bank `GPL`, index 1, at start-up.
const GPL_NAMED_VALUE: &str = "#TeleportInsideDestination";

/// A `#Name` the attribute registry answers: `ATTRIB_` then an attribute.
const ATTRIB: &str = "#ATTRIB_";

/// Every `.bcd` on disk: SDK, Workshop, HD.
fn every_bcd() -> Vec<PathBuf> {
    common::bcds(&[common::sdk(), common::workshop(), common::hd()])
}

fn text(path: &Path) -> String {
    String::from_utf8_lossy(&read(path)).into_owned()
}

/// HD's listed GPL images: the base game alone, or with the expansion.
fn hd_listed(sets: Datasets) -> Vec<PathBuf> {
    let files = Install::new(&common::hd(), Bytecode::Install)
        .files(sets)
        .expect("HD's files");

    // Load order is the listed order reversed.
    dataset::load_order(files.programs)
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_owned()
}

fn load_paths(order: &[PathBuf]) -> Program {
    let images: Vec<Vec<u8>> = order.iter().map(|p| read(p)).collect();

    Program::load(&images).unwrap_or_else(|e| panic!("{order:?}: {e}"))
}

/// A `#Name` the engine answers from its attribute registry.
fn is_named_value(name: &str) -> bool {
    let attribute = name
        .get(..ATTRIB.len())
        .filter(|head| head.eq_ignore_ascii_case(ATTRIB))
        .map(|_| &name[ATTRIB.len()..]);

    match attribute {
        Some(rest) => attrib::EXPANSION
            .iter()
            .map(|(_, _, known)| *known)
            .chain(std::iter::once(attrib::HD_EXTRA))
            .any(|known| known.eq_ignore_ascii_case(rest)),
        None => name.eq_ignore_ascii_case(GPL_NAMED_VALUE),
    }
}

/// Unresolved references, as text, bar `#Name`s the attribute registry
/// answers.
fn missing(program: &Program) -> Vec<String> {
    program
        .unresolved()
        .iter()
        .filter_map(|u| {
            let owner = program
                .names()
                .get(program.definitions()[u.definition as usize].name);

            match u.missing {
                Missing::Call(name) => Some(format!("{owner}: call {}", program.names().get(name))),
                Missing::Function(name) => {
                    Some(format!("{owner}: value {}", program.names().get(name)))
                }
                Missing::Constant(name) if !is_named_value(program.names().get(name)) => {
                    Some(format!("{owner}: {}", program.names().get(name)))
                }
                Missing::Constant(_) => None,
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// The decoded form

/// All 84 images decode, one instruction per record, nothing refused.
#[test]
fn every_image_decodes_one_instruction_per_record() {
    let files = every_bcd();
    let mut definitions = 0;
    let mut instructions = 0;

    for path in &files {
        let bytes = read(path);
        let mut names = Names::default();
        let image =
            exec::image(&bytes, &mut names).unwrap_or_else(|e| panic!("{}: {e}", path.display()));

        let container = Container::parse(&bytes).expect("a readable image");
        let walked = walk(container.sections().expect("sections").symbols.body).expect("walks");

        for (decoded, compiled) in image.definitions.iter().zip(&walked) {
            assert_eq!(names.get(decoded.name), compiled.name);
            assert_eq!(decoded.code.len(), compiled.records.len());

            for (instr, record) in decoded.code.iter().zip(&compiled.records) {
                assert_eq!(instr.op.code(), record.opcode);
            }
        }

        assert_eq!(image.definitions.len(), walked.len(), "{}", path.display());

        definitions += image.definitions.len();
        instructions += image
            .definitions
            .iter()
            .map(|d| d.code.len())
            .sum::<usize>();
    }

    assert_eq!(files.len(), 75, "images on disk");
    assert_eq!(definitions, 19_466, "definitions decoded");
    assert_eq!(instructions, 1_262_666, "instructions decoded");
}

/// `Acid_Bolt_Hit`, the `docs/formats/bytecode.md` specimen: a signature from the
/// prologue, calls linked to builtins and a script function.
#[test]
fn acid_bolt_hit_decodes_into_its_signature_and_calls() {
    let program = load_paths(&dataset::load_order(hd_listed(Datasets::Base)));
    let f = program.function("acid_bolt_hit").expect("defined");
    let signature = f.signature().expect("a function");

    let params: Vec<&str> = signature
        .params
        .iter()
        .map(|p| program.names().get(p.name))
        .collect();

    assert_eq!(params, ["thisagent", "target"]);
    assert_eq!(signature.returns, TypeCode::Null, "no is clause");

    let calls: Vec<(u32, Vec<u32>, String)> = f
        .code
        .iter()
        .zip(0..)
        .filter_map(|(i, at)| i.call.as_ref().map(|c| (at, c)))
        .map(|(at, c)| {
            let callee = match c.callee {
                Callee::Fn(Target::Builtin(b)) => b.get().name.to_owned(),
                Callee::Fn(Target::Script(d)) => program
                    .names()
                    .get(program.definitions()[d as usize].name)
                    .to_owned(),
                other => format!("{other:?}"),
            };

            (at, c.args.to_vec(), callee.to_ascii_lowercase())
        })
        .collect();

    assert_eq!(
        calls,
        [
            (12, vec![10, 11], "getspellattribute".to_owned()),
            (13, vec![8, 9, 12], "createeffector".to_owned()),
            (17, vec![14, 15, 16], "spell_attack".to_owned()),
        ]
    );
}

/// The compiled type of a destination survives: every declaration keeps its
/// type, and `Snake_Pit_Spawn`'s `opPlus` writes a fresh integer.
#[test]
fn a_destination_keeps_its_compiled_type() {
    let program = load_paths(&dataset::load_order(hd_listed(Datasets::Expansion)));
    let mut typed = 0;

    for definition in program.definitions() {
        if let Some(signature) = definition.signature() {
            for local in signature.params.iter().chain(&signature.locals) {
                assert_eq!(definition.slot_type(local.at), Some(local.ty));
                typed += 1;
            }
        }
    }

    let snake = program.function("Snake_Pit_Spawn").expect("defined");
    let fresh: Vec<&exec::Instr> = snake
        .code
        .iter()
        .filter(|i| matches!(i.dest, Dest::Value(_)))
        .collect();

    assert_eq!(fresh.len(), 1);
    assert_eq!(fresh[0].op, Op::Plus);
    assert_eq!(fresh[0].dest, Dest::Value(Literal::Int(0)));
    assert!(typed > 0);
}

// ---------------------------------------------------------------------------
// HD

/// HD lists `Bytecode.bcd`, `MX_Compatibility.bcd`, then the expansion's
/// four, and loads the list tail first.
#[test]
fn hd_lists_its_images_as_the_definitions_say() {
    let names = |paths: Vec<PathBuf>| -> Vec<String> {
        paths
            .iter()
            .map(|p| file_name(p).to_ascii_lowercase())
            .collect()
    };

    assert_eq!(
        names(hd_listed(Datasets::Base)),
        ["bytecode.bcd", "mx_compatibility.bcd"]
    );
    assert_eq!(
        names(dataset::load_order(hd_listed(Datasets::Expansion))),
        [
            "mx_task.bcd",
            "mx_decision.bcd",
            "mx_data.bcd",
            "mx_build.bcd",
            "mx_compatibility.bcd",
            "bytecode.bcd"
        ]
    );
}

/// Every call and function value resolves; every `#Name` is a script
/// constant or one the attribute registry holds.
#[test]
fn hd_resolves_every_call() {
    for sets in [Datasets::Base, Datasets::Expansion] {
        let program = load_paths(&dataset::load_order(hd_listed(sets)));

        assert_eq!(missing(&program), Vec::<String>::new(), "{sets:?}");
    }
}

/// The map holds the builtins, then every function and constant name no
/// builtin has. Counted from section 2, which the loader does not read.
fn expected_map(order: &[PathBuf]) -> usize {
    let builtin: BTreeSet<String> = builtins::HD
        .iter()
        .map(|b| b.name.to_ascii_lowercase())
        .collect();

    let mut script = BTreeSet::new();

    for path in order {
        let bytes = read(path);
        let container = Container::parse(&bytes).expect("a readable image");
        let scopes = ScopeSection::decode(container.sections().expect("sections").code.body)
            .expect("section 2");

        script.extend(
            scopes
                .functions
                .iter()
                .chain(&scopes.constants)
                .map(|d| d.name.to_ascii_lowercase()),
        );
    }

    builtin.len() + script.difference(&builtin).count()
}

/// The function map has the size the engine registers: 1,703 names for the
/// base game, 2,190 with the expansion; openregent's own builtins besides.
#[test]
fn the_function_map_has_the_size_the_engine_registers() {
    for (sets, size) in [(Datasets::Base, 1_703), (Datasets::Expansion, 2_190)] {
        let order = dataset::load_order(hd_listed(sets));
        let program = load_paths(&order);
        let engine = program.map_len() - builtins::OWN.len();

        assert_eq!(engine, expected_map(&order));
        assert_eq!(engine, size, "{sets:?}");
    }
}

/// The last listed image owns a shared name. With the expansion, 1,476 of
/// `Bytecode.bcd`'s names and 63 of `MX_Compatibility.bcd`'s give way to the
/// four expansion images, which share none among themselves.
#[test]
fn the_last_listed_image_owns_a_shared_name() {
    let order = dataset::load_order(hd_listed(Datasets::Expansion));
    let program = load_paths(&order);

    let dropped = |image: &str| {
        program
            .shadowed()
            .iter()
            .filter(|s| file_name(&order[s.dropped]).eq_ignore_ascii_case(image))
            .count()
    };

    assert!(program.shadowed().iter().all(|s| s.kept < s.dropped));
    assert_eq!(dropped("bytecode.bcd"), 1_476);
    assert_eq!(dropped("MX_Compatibility.bcd"), 63);
    assert_eq!(program.shadowed().len(), 1_476 + 63);

    // The expansion's version runs.
    let f = program.function("Acid_Bolt_Hit").expect("defined");
    let at = program
        .definitions()
        .iter()
        .position(|d| std::ptr::eq(d, f))
        .expect("a definition");

    assert!(file_name(&order[program.origin()[at]]).starts_with("MX_"));
}

// ---------------------------------------------------------------------------
// `datasets.xml`

/// A `DATA` `XML1` record: `datasets.xml`.
fn datasets_xml(cam: &Path) -> String {
    let bytes = read(cam);
    let archive = Archive::parse(&bytes).expect("a readable archive");

    archive
        .records()
        .find(|(chunk, record)| chunk.tag.as_bytes() == b"DATA" && record.id.as_bytes() == b"XML1")
        .map_or_else(
            || panic!("{}: no XML1", cam.display()),
            |(_, record)| String::from_utf8_lossy(record.data).into_owned(),
        )
}

/// The groups load and resolve. Which group each game walks is **Inferred**
/// from the names: not read from a binary.
#[test]
fn the_ports_datasets_groups_resolve_every_call() {
    let linux = common::hd();
    let cases = [
        ("Data", "MajestyCommonData", vec!["Bytecode.bcd"]),
        ("Data", "Rel2", vec!["MDL1.bcd", "Bytecode.bcd"]),
        (
            "DataMX",
            "MajXCommonData",
            vec![
                "MX_Build.bcd",
                "MX_Data.bcd",
                "MX_Decision.bcd",
                "MX_Task.bcd",
            ],
        ),
        (
            "DataMX",
            "Rel0",
            vec!["MX_Compatibility.bcd", "Bytecode.bcd"],
        ),
    ];

    for (dir, group, want) in cases {
        let cam = if dir == "Data" {
            "Data/miscdata.cam"
        } else {
            "DataMX/mx_miscdata.cam"
        };
        let xml = datasets_xml(&linux.join(cam));
        let listed = dataset::datasets_gpl(&xml, group);

        assert_eq!(listed, want, "{group}");

        let order: Vec<PathBuf> = listed
            .iter()
            .map(|name| {
                regent_core::content::on_disk(&linux.join(dir).join(name)).expect("on disk")
            })
            .collect();

        assert_eq!(
            missing(&load_paths(&order)),
            Vec::<String>::new(),
            "{group}"
        );
    }
}

// ---------------------------------------------------------------------------
// Workshop

/// A Workshop item's main image: the first `<GPL>` of its first `.mmxml`.
/// Its other images, AI and quest variants, call into it.
fn main_image(item: &Path) -> Option<PathBuf> {
    let manifest = common::walk(item, &["mmxml"]).into_iter().next()?;
    let text = text(&manifest);
    let after = text.split("<GPL>").nth(1)?;
    let inner = after.trim_start();
    let inner = inner.strip_prefix("<Target>").unwrap_or(inner);
    let listed = inner.split('<').next()?.trim();

    let path = listed
        .split(['\\', '/'])
        .fold(manifest.parent()?.to_path_buf(), |at, part| at.join(part));

    regent_core::content::on_disk(&path)
}

/// The images a Workshop image loads with, listed order: HD's expansion, its
/// item's main image, itself.
fn workshop_listed(image: &Path) -> Vec<PathBuf> {
    let root = common::workshop();
    let item = image
        .strip_prefix(&root)
        .ok()
        .and_then(|rest| rest.components().next())
        .map(|first| root.join(first))
        .expect("inside the Workshop");

    let main: Vec<PathBuf> = main_image(&item).into_iter().collect();

    dataset::compose(&[
        &hd_listed(Datasets::Expansion),
        &main,
        &[image.to_path_buf()],
    ])
}

/// Every Workshop image loads over HD's expansion and its item's main image,
/// as a mod's `<Dataset base="Any">` appends them. Every call and function
/// value resolves.
///
/// One `#Name` no script defines: `NewGamePlus`'s `#LegendaryHeroesExtend`, a
/// text id its `Data/text.xml` declares. **Inferred**: the engine's third
/// source, descriptor by name, answers it.
#[test]
fn every_workshop_image_loads_over_the_expansion() {
    const TEXT_ID: [&str; 2] = [
        "Legendary_Heroes_Time_Extend: #LegendaryHeroesExtend",
        "Legendary_Heroes_Time_Limit: #LegendaryHeroesExtend",
    ];

    let images = common::walk(&common::workshop(), &["bcd"]);
    let mut unresolved = Vec::new();

    for image in &images {
        let program = load_paths(&dataset::load_order(workshop_listed(image)));
        let mut text = missing(&program);

        text.sort();
        text.dedup();

        if !text.is_empty() {
            unresolved.push((file_name(image), text));
        }
    }

    let want: Vec<(String, Vec<String>)> = ["Abilities.bcd", "GPLinfo.bcd", "quests.bcd"]
        .iter()
        .map(|image| ((*image).to_owned(), TEXT_ID.map(str::to_owned).to_vec()))
        .collect();

    assert_eq!(images.len(), 57, "Workshop images");
    assert_eq!(unresolved, want);
}

// ---------------------------------------------------------------------------
// Arguments

/// Script calls whose argument count differs from the callee's parameters,
/// in HD's own program. The prologue names what a callee reads; what the
/// engine does with the difference is the interpreter's question.
const HD_OFF_PROLOGUE: [&str; 3] = [
    "Enemy_Actions -> Check_Heal: 1 for 0",
    "Enemy_Actions -> Check_Lightning_Defense: 1 for 0",
    "Enemy_Actions -> Check_Resurrect: 1 for 0",
];

/// The same, added by Workshop images.
const WORKSHOP_OFF_PROLOGUE: [&str; 14] = [
    "DARK_FOREST -> Setup_Rescue_Buildings: 1 for 0",
    "DAY_OF_RECKONING -> Setup_Rescue_Buildings: 1 for 0",
    "HOLY_CHALICE -> Setup_Rescue_Buildings: 1 for 0",
    "LICHE_QUEEN -> Setup_Rescue_Buildings: 1 for 0",
    "MK_Enemy_Actions -> Check_Heal: 1 for 0",
    "MK_Enemy_Actions -> Check_Lightning_Defense: 1 for 0",
    "MK_Enemy_Actions -> Check_Resurrect: 1 for 0",
    "QUEST_FOR_CROWN -> Setup_Rescue_Buildings: 1 for 0",
    "SLAVE_PITS -> Setup_Rescue_Buildings: 1 for 0",
    "SLAY_DRAGON -> Setup_Rescue_Buildings: 1 for 0",
    "TOMB_DRAGON -> Setup_Rescue_Buildings: 1 for 0",
    "VIGIL -> Setup_Rescue_Buildings: 1 for 0",
    "make_attack_assassination -> attack_end: 2 for 3",
    "spell_attack -> attack_end: 2 for 3",
];

/// Every call passes what its callee reads: a builtin its required
/// arguments; a script function its parameters, bar the calls listed.
#[test]
fn every_call_fits_its_callee() {
    let mut programs = vec![
        (
            "hd",
            load_paths(&dataset::load_order(hd_listed(Datasets::Base))),
        ),
        (
            "hd",
            load_paths(&dataset::load_order(hd_listed(Datasets::Expansion))),
        ),
    ];

    for image in common::walk(&common::workshop(), &["bcd"]) {
        let program = load_paths(&dataset::load_order(workshop_listed(&image)));

        programs.push(("workshop", program));
    }

    let mut short = Vec::new();
    let mut surplus = BTreeSet::new();
    let mut script = BTreeSet::new();
    let mut calls = 0;

    for (label, program) in &programs {
        for definition in program.definitions() {
            for call in definition.code.iter().filter_map(|i| i.call.as_deref()) {
                let n = call.args.len();
                calls += 1;

                match call.callee {
                    Callee::Fn(Target::Builtin(b)) => {
                        let builtin = b.get();

                        if !builtin.args.accepts(n) {
                            short.push(builtin.name);
                        }

                        if builtin.args.most().is_some_and(|most| n > most) {
                            surplus.insert(builtin.name);
                        }
                    }
                    Callee::Fn(Target::Script(d)) => {
                        let callee = &program.definitions()[d as usize];
                        let params = callee.signature().map_or(0, |s| s.params.len());

                        if n != params {
                            script.insert((
                                *label,
                                format!(
                                    "{} -> {}: {n} for {params}",
                                    program.names().get(definition.name),
                                    program.names().get(callee.name)
                                ),
                            ));
                        }
                    }
                    Callee::Slot(_) | Callee::Name(_) => {}
                }
            }
        }
    }

    assert!(calls > 0);
    assert_eq!(short, Vec::<&str>::new(), "builtins passed too few");
    assert_eq!(
        surplus.into_iter().collect::<Vec<_>>(),
        [
            "CastSpell",
            "LocationOf",
            "SetThreadInterval",
            "TeleportToPoint"
        ],
        "builtins passed one they never read"
    );

    let off = |want: &str| -> Vec<&str> {
        script
            .iter()
            .filter(|(label, _)| *label == want)
            .map(|(_, call)| call.as_str())
            .collect()
    };

    assert_eq!(off("hd"), HD_OFF_PROLOGUE);

    // A mod program holds HD's three and adds its own.
    let workshop = off("workshop");
    let added: Vec<&str> = workshop
        .iter()
        .filter(|call| !HD_OFF_PROLOGUE.contains(call))
        .copied()
        .collect();

    assert!(HD_OFF_PROLOGUE.iter().all(|call| workshop.contains(call)));
    assert_eq!(added, WORKSHOP_OFF_PROLOGUE);
}

// ---------------------------------------------------------------------------
// Source against shipped

/// A definition with its debug information dropped: source file and lines.
fn stripped(definition: &Definition) -> Definition {
    let mut out = definition.clone();

    for instr in &mut out.code {
        instr.line = None;

        if instr.name == Some(definition.file) {
            instr.name = None;
        }
    }

    out.file = definition.name;
    out
}

fn decode(bytes: &[u8], names: &mut Names, what: &str) -> Image {
    exec::image(bytes, names).unwrap_or_else(|e| panic!("{what}: {e}"))
}

/// Names of definitions that differ between two images.
fn differing(ours: &Image, theirs: &Image, names: &Names, debug: bool) -> Vec<String> {
    let pick = |d: &Definition| if debug { d.clone() } else { stripped(d) };

    assert_eq!(ours.definitions.len(), theirs.definitions.len());
    assert_eq!(ours.data, theirs.data);

    ours.definitions
        .iter()
        .zip(&theirs.definitions)
        .filter(|(a, b)| pick(a) != pick(b))
        .map(|(a, _)| names.get(a.name).to_owned())
        .collect()
}

/// The SDK's projects compile to the program their images hold, and to the
/// one HD ships, bar debug information. The one difference is
/// `Snake_Pit_Spawn`'s fresh integer, which the compiler does not write; the
/// SDK example's image records another build path.
#[test]
fn decoded_from_source_equals_decoded_from_shipped() {
    const SNAKE_PIT: &str = "Snake_Pit_Spawn";
    const EXAMPLE: &str = "Example/GPL/WrathOfKrolm_Path.gplproj";

    let sdk = common::sdk();
    let hd = common::hd();
    let projects = [
        ("OriginalQuests/GPL/path.gplproj", "Data/bytecode.bcd"),
        (EXAMPLE, "Example/Data/WrathOfKrolm.bcd"),
        (
            "OriginalQuests/GPLMx/Path_Task.gplproj",
            "DataMX/MX_Task.bcd",
        ),
        (
            "OriginalQuests/GPLMx/Path_Data.gplproj",
            "DataMX/MX_Data.bcd",
        ),
        (
            "OriginalQuests/GPLMx/Path_Build.gplproj",
            "DataMX/MX_Build.bcd",
        ),
        (
            "OriginalQuests/GPLMx/Path_Decision.gplproj",
            "DataMX/MX_Decision.bcd",
        ),
    ];

    for (project, image) in projects {
        let mut names = Names::default();
        let compiled =
            emit::compile_project(&sdk.join(project)).unwrap_or_else(|e| panic!("{project}: {e}"));
        let ours = decode(&compiled, &mut names, project);

        let sdk_image = if project == EXAMPLE {
            sdk.join(image)
        } else {
            sdk.join("OriginalQuests").join(image)
        };
        let theirs = decode(&read(&sdk_image), &mut names, image);
        let expected: Vec<String> = if image.ends_with("MX_Data.bcd") {
            vec![SNAKE_PIT.to_owned()]
        } else {
            Vec::new()
        };

        // The SDK image: identical, debug information included, bar the
        // example's build path.
        let debug = project != EXAMPLE;
        assert_eq!(
            differing(&ours, &theirs, &names, debug),
            expected,
            "{image}"
        );

        // HD's shipped image: absolute paths, shifted lines.
        if project != EXAMPLE {
            let shipped = decode(&read(&hd.join(image)), &mut names, image);

            assert_eq!(
                differing(&ours, &shipped, &names, false),
                expected,
                "HD {image}"
            );
        }
    }
}

/// Prototypes and `.dat` instances stand as loaded; a constant's value is
/// its literal. The program's named state, GPL having no globals.
#[test]
fn prototypes_constants_and_data_sets_are_by_name() {
    let program = load_paths(&dataset::load_order(hd_listed(Datasets::Expansion)));

    let prototypes = program
        .definitions()
        .iter()
        .filter(|d| matches!(d.kind, Kind::Prototype(_)))
        .count();

    assert!(prototypes > 0);

    for definition in program.definitions() {
        let name = program.names().get(definition.name);

        match &definition.kind {
            Kind::Prototype(members) => {
                assert_eq!(program.prototype(name).map(<[_]>::len), Some(members.len()));
                assert!(
                    members.iter().all(|m| (1..=8).contains(&m.ty.code())),
                    "{name}"
                );
            }
            Kind::Constant(value) => assert_eq!(program.constant(name), Some(*value)),
            Kind::Function(_) => {}
        }
    }

    for set in program.data() {
        let shape = program.names().get(set.shape);

        assert!(program.prototype(shape).is_some(), "{shape}");
    }
}

/// A negative constant's value: its `opNegate` record evaluated, as
/// gpl eval constant does. Monster Kingdom's
/// `#WYRM_HEAL_MOD_TIER1 -2`.
#[test]
fn a_negative_constant_is_negative() {
    let path = common::workshop().join("3743606613/Data/MonsterKingdom.bcd");
    let mut names = Names::default();
    let image = decode(&read(&path), &mut names, "MonsterKingdom.bcd");
    let value = image
        .definitions
        .iter()
        .find(|d| names.get(d.name) == "#WYRM_HEAL_MOD_TIER1")
        .map(|d| d.kind.clone());

    assert_eq!(value, Some(Kind::Constant(Literal::Int(-2))));
}
