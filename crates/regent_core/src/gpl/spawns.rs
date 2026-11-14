//! What each spawning building's script can spawn, read from a loaded
//! program. **Corpus**: the shipped scripts spawn a lair's units by
//! name, from its `.dat` instance's `Spawn_Function`.
//!
//! ```text
//!  .dat instance: type Lair, or a Spawn_Function ──> a spawner
//!  Spawn_Function ──> script function, its calls followed
//!    SpawnUnit(agent, name)
//!      name a string literal        ──> that name
//!      name ThisAgent's "Spawn_Type" ──> the instance's member
//!  a script the instance names writes agent's "Spawn_Function" = f
//!    ──> f's spawns are staged: set later (the Spire of Death's)
//! ```
//!
//! Not followed: the instance's other scripts' own spawns (a falling
//! lair's `Special_Spawn_Type`, a quest's events). A name is a descriptor's
//! HEAD name, folded; whether one exists is the caller's to say.

use std::collections::BTreeMap;

use super::exec::{Callee, DataSet, Definition, Literal, Operand, Target};
use super::load::Program;
use super::opcode::Op;

/// The builtins that put a unit on the map by descriptor name.
const SPAWNERS: [&str; 2] = ["SpawnUnit", "SpawnGraveyard"];

/// The name's place among a spawner builtin's arguments.
const NAME_ARG: usize = 1;

/// The `.dat` member holding a spawner's spawn script, folded.
const SPAWN_FUNCTION: &str = "spawn_function";

/// The `.dat` members a spawn script reads a unit's name from, folded.
const TYPE_MEMBERS: [&str; 2] = ["spawn_type", "special_spawn_type"];

/// The `.dat` member holding an instance's kind, folded, and a lair's.
const TYPE: &str = "type";
const LAIR: &str = "lair";

/// How many slots a value is followed back through.
const DEPTH: u32 = 8;

/// A `.dat` instance whose script spawns units.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Spawner {
    /// The instance's name, as written: its descriptor's HEAD name.
    pub instance: String,
    /// Its `type` member, as written.
    pub kind: String,
    /// Its `Spawn_Function` member, as written; empty without one.
    pub function: String,
    /// The names its spawn function can spawn, folded, in the order found.
    pub names: Vec<String>,
    /// The names later stages add, folded, in the order found.
    pub staged: Vec<String>,
    /// Spawn calls whose name came to no literal or member.
    pub unresolved: usize,
}

/// What a spawn call's name came to.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Named {
    /// A string literal, folded.
    Text(String),
    /// One of [`TYPE_MEMBERS`], read off the agent.
    Member(String),
}

/// What one function does, as far as spawning goes; each list in
/// instruction order, folded, without repeats.
#[derive(Default)]
struct Found {
    names: Vec<Named>,
    unresolved: usize,
    /// Script functions called.
    calls: Vec<String>,
    /// Script functions named as values.
    values: Vec<String>,
    /// Script functions written to an agent's [`SPAWN_FUNCTION`] member.
    stages: Vec<String>,
}

/// Which references a walk follows.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Follow {
    Calls,
    CallsAndValues,
}

fn fold(text: &str) -> String {
    text.to_ascii_lowercase()
}

/// `item` added, unless there.
fn add<T: PartialEq>(list: &mut Vec<T>, item: T) {
    if !list.contains(&item) {
        list.push(item);
    }
}

/// An operand's string or function literal; none for the empty one, a
/// declared local's default.
fn literal(program: &Program, operand: Operand) -> Option<&str> {
    let text = match operand {
        Operand::Value(Literal::Str(sym) | Literal::Function(sym)) => {
            Some(program.names().get(sym))
        }
        _ => None,
    };

    text.filter(|t| !t.is_empty())
}

/// The string and function literals that feed `operand`, through slots
/// and assignments to them.
fn feeds(program: &Program, def: &Definition, operand: Operand, depth: u32, out: &mut Vec<String>) {
    out.extend(literal(program, operand).map(str::to_owned));

    if let (Operand::Slot(slot), Some(depth)) = (operand, depth.checked_sub(1)) {
        // A call's result is not followed: its arguments say nothing of it.
        let read = def.code.get(slot as usize).filter(|i| i.op != Op::Call);

        if let Some(instr) = read {
            feeds(program, def, instr.lhs, depth, out);
            feeds(program, def, instr.rhs, depth, out);
        }

        let written = def
            .code
            .iter()
            .filter(|i| i.op == Op::AssignEqual && i.lhs == Operand::Slot(slot));

        for assign in written {
            feeds(program, def, assign.rhs, depth, out);
        }
    }
}

/// A call's target, by name; empty through a slot.
fn callee(program: &Program, callee: Callee) -> &str {
    let names = program.names();

    match callee {
        Callee::Name(sym) => names.get(sym),
        Callee::Fn(Target::Builtin(id)) => id.get().name,
        Callee::Fn(Target::Script(at)) => program
            .definitions()
            .get(at as usize)
            .map_or("", |d| names.get(d.name)),
        Callee::Slot(_) => "",
    }
}

/// One function's spawns and references.
fn found(program: &Program, def: &Definition) -> Found {
    let mut found = Found::default();
    let script = |name: &str| program.function(name).is_some();

    for instr in &def.code {
        // Functions named as values.
        for operand in [instr.lhs, instr.rhs] {
            if let Some(name) = literal(program, operand).filter(|n| script(n)) {
                add(&mut found.values, fold(name));
            }
        }

        // `agent's "Spawn_Function" = value`: the functions it may be.
        if instr.op == Op::AssignEqual {
            let (mut member, mut value) = (Vec::new(), Vec::new());

            feeds(program, def, instr.lhs, DEPTH, &mut member);

            if member.iter().any(|m| fold(m) == SPAWN_FUNCTION) {
                feeds(program, def, instr.rhs, DEPTH, &mut value);
            }

            for stage in value.iter().filter(|v| script(v)) {
                add(&mut found.stages, fold(stage));
            }
        }

        if let Some(call) = &instr.call {
            let callee = callee(program, call.callee);

            if script(callee) {
                add(&mut found.calls, fold(callee));
            }

            if SPAWNERS.iter().any(|s| s.eq_ignore_ascii_case(callee)) {
                let mut texts = Vec::new();

                if let Some(slot) = call.args.get(NAME_ARG) {
                    feeds(program, def, Operand::Slot(*slot), DEPTH, &mut texts);
                }

                let texts: Vec<String> = texts.iter().map(|t| fold(t)).collect();
                let member = texts.iter().find(|t| TYPE_MEMBERS.contains(&t.as_str()));

                match member {
                    Some(member) => add(&mut found.names, Named::Member(member.clone())),
                    None if texts.is_empty() => found.unresolved += 1,
                    None => texts
                        .into_iter()
                        .for_each(|t| add(&mut found.names, Named::Text(t))),
                }
            }
        }
    }

    found
}

/// The functions reached from `starts`, themselves included, nearest
/// first.
fn reach(all: &BTreeMap<String, Found>, starts: &[String], follow: Follow) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    let mut at = 0;

    for start in starts {
        if all.contains_key(start) {
            add(&mut seen, start.clone());
        }
    }

    while let Some(found) = seen.get(at).and_then(|name| all.get(name)) {
        let values = match follow {
            Follow::Calls => &[][..],
            Follow::CallsAndValues => &found.values[..],
        };
        let next: Vec<String> = found.calls.iter().chain(values).cloned().collect();

        for name in next {
            add(&mut seen, name);
        }

        at += 1;
    }

    seen
}

/// Each spawner instance of `program`, in the data's order.
#[must_use]
pub fn spawners(program: &Program) -> Vec<Spawner> {
    let names = program.names();
    let all: BTreeMap<String, Found> = program
        .definitions()
        .iter()
        .filter(|def| def.signature().is_some())
        .map(|def| (fold(names.get(def.name)), found(program, def)))
        .collect();

    program
        .data()
        .iter()
        .filter_map(|set| spawner(program, &all, set))
        .collect()
}

/// `set` as a spawner, where it is one.
fn spawner(program: &Program, all: &BTreeMap<String, Found>, set: &DataSet) -> Option<Spawner> {
    let names = program.names();
    let members: Vec<(String, &str)> = set
        .values
        .iter()
        .map(|(key, value)| (fold(names.get(*key)), names.get(*value)))
        .collect();
    let member = |key: &str| members.iter().find(|(k, _)| k == key).map(|(_, v)| *v);
    let kind = member(TYPE).unwrap_or_default();
    let function = member(SPAWN_FUNCTION).unwrap_or_default();

    (fold(kind) == LAIR || !function.is_empty()).then(|| {
        // The spawn function's reach, then each stage's that a script of
        // the instance's sets.
        let called = reach(all, &[fold(function)], Follow::Calls);
        let scripts: Vec<String> = members.iter().map(|(_, value)| fold(value)).collect();
        let written: Vec<String> = reach(all, &scripts, Follow::CallsAndValues)
            .iter()
            .filter_map(|name| all.get(name))
            .flat_map(|found| found.stages.iter().cloned())
            .collect();
        let later = reach(all, &written, Follow::Calls);

        // What the functions `reached` name, and their unresolved calls.
        let spawned = |reached: &[String]| -> (Vec<String>, usize) {
            let founds = reached.iter().filter_map(|name| all.get(name));
            let mut texts = Vec::new();
            let mut unresolved = 0;

            for found in founds {
                unresolved += found.unresolved;

                for named in &found.names {
                    let text = match named {
                        Named::Text(text) => Some(text.clone()),
                        Named::Member(key) => member(key).map(fold),
                    };

                    if let Some(text) = text {
                        add(&mut texts, text);
                    }
                }
            }

            (texts, unresolved)
        };
        let (first, unresolved) = spawned(&called);
        let (staged, more) = spawned(&later);

        Spawner {
            instance: names.get(set.name).to_owned(),
            kind: kind.to_owned(),
            function: function.to_owned(),
            staged: staged.into_iter().filter(|n| !first.contains(n)).collect(),
            names: first,
            unresolved: unresolved + more,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpl::{dat, emit};

    const SCRIPTS: &str = r#"
Function Spawn (agent ThisAgent)
Begin
	$SpawnUnit (ThisAgent, ThisAgent's "Spawn_Type");
End

Function Fall (agent ThisAgent)
Begin
	$SpawnUnit (ThisAgent, ThisAgent's "Special_Spawn_Type");
	$SpawnUnit (ThisAgent, "Ghost");
End

Function Pack (agent ThisAgent)
Begin
	$SpawnUnit (ThisAgent, "Wolf");
End

Function Den_Spawn (agent ThisAgent)
Declare
	string pick;
Begin
	if ($RandomNumber (2) == 0)
		pick = "Bear";
	else
		pick = "Boar";

	$SpawnUnit (ThisAgent, pick);
	$Pack (ThisAgent);
End

Function Tower_One (agent ThisAgent)
Begin
	$SpawnUnit (ThisAgent, "Imp");
End

Function Tower_Two (agent ThisAgent)
Begin
	$SpawnUnit (ThisAgent, "Fiend");
End

Function Tower_Scan (agent ThisAgent)
Declare
	function spawn;
Begin
	spawn = $Tower_Two;
	ThisAgent's "Spawn_Function" = spawn;
End
"#;

    const DATA: &str = "
[Cave]
	{Lair
		(type Lair)
		(Spawn_Type Rat)
		(Special_Spawn_Type Ogre)
		(Spawn_Function Spawn)
		(IGdeathscript Fall)
	}
[end]

[Den]
	{Lair
		(type Lair)
		(Spawn_Function Den_Spawn)
	}
[end]

[Tower]
	{Lair
		(type Lair)
		(Spawn_Function Tower_One)
		(ActiveScript Tower_Scan)
	}
[end]

[Ruin]
	{Lair
		(type Lair)
	}
[end]

[Drain]
	{Building
		(type AutoBuilding)
		(Spawn_Type Rat)
		(Spawn_Function Spawn)
	}
[end]

[House]
	{Building
		(type Building)
	}
[end]
";

    fn scanned() -> Vec<Spawner> {
        let code = emit::compile_source(SCRIPTS, "test.gpl").expect("compiles");
        let parsed = dat::parse(DATA);

        assert!(parsed.is_ok(), "{:?}", parsed.errors);

        let data = emit::compile(&[parsed]);
        let program = Program::load(&[code, data]).expect("loads");

        spawners(&program)
    }

    fn spawner(instance: &str) -> Spawner {
        scanned()
            .into_iter()
            .find(|s| s.instance == instance)
            .expect("a spawner")
    }

    fn names(instance: &str) -> Vec<String> {
        spawner(instance).names
    }

    /// A lair, or an instance with a spawn function, is a spawner; a
    /// plain building is none.
    #[test]
    fn a_lair_or_a_spawn_function_makes_a_spawner() {
        let found: Vec<String> = scanned().into_iter().map(|s| s.instance).collect();

        assert_eq!(found, ["Cave", "Den", "Drain", "Ruin", "Tower"]);
    }

    /// `ThisAgent's "Spawn_Type"` is the instance's member; its death
    /// script's spawns are not its spawn function's.
    #[test]
    fn the_spawn_type_member_is_read_and_a_death_spawn_left_out() {
        assert_eq!(names("Cave"), ["rat"]);
        assert_eq!(names("Drain"), ["rat"]);
    }

    /// Literals reach the call through a local, and through a called
    /// function.
    #[test]
    fn literals_are_followed_through_locals_and_calls() {
        assert_eq!(names("Den"), ["bear", "boar", "wolf"]);
    }

    /// A function one of the instance's scripts writes as its spawn
    /// function spawns for it too: a later stage.
    #[test]
    fn a_stage_written_later_is_followed() {
        let tower = spawner("Tower");

        assert_eq!(tower.names, ["imp"]);
        assert_eq!(tower.staged, ["fiend"]);
    }

    /// A lair with no spawn function spawns nothing, and nothing is left
    /// unresolved.
    #[test]
    fn a_lair_without_a_function_spawns_nothing() {
        let ruin = spawner("Ruin");

        assert!(ruin.names.is_empty() && ruin.staged.is_empty());
        assert_eq!(ruin.unresolved, 0);
    }
}
