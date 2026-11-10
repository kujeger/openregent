# The GPL front end and `.dat` back end

`crates/regent_core/src/gpl/`: lexer, parser, `.dat` reader, a resolver that
cross-checks the corpus, and a back end that compiles a project. Language and
grammar: [language.md](language.md).

## Status

Complete against the 306 shipped `.gpl` and 15 `.dat` files
(`every_file_lexes_without_error`, `every_dat_file_parses`) and every Workshop
mod's `.gpl` and `.dat` (`tests/mods.rs`, lower bounds).

| Check | Shipped | Workshop mods |
|---|---|---|
| Files lexed | all, zero errors, 253,176 tokens (`token_counts_match_the_survey`) | all, zero errors |
| Files parsed | all, zero errors | all, zero errors |
| `.dat` files parsed | 417 prototypes, 5,176 values (`every_dat_file_parses`) | at least 956 prototypes, 16,284 values (`every_mod_dat_file_parses`) |
| `$name` references resolved | 19,819 of 19,819 (`every_function_reference_resolves`) | all, with the base game alongside |

The mods test forms the shipped files do not use.

Parser node counts against the lexer's token counts (`statement_kind_counts_match`,
`top_level_item_counts_match`, `expression_counts_match_the_token_stream`):

| Node | Count | Reconciles against |
|---|---|---|
| `if` / `foreach` / `while` | 5,341 / 458 / 111 | those keyword counts |
| Constant definitions | 1,582 | `expression` |
| `Attr` | 7,001 | `'s` |
| `Func` | 19,819 | `$name` |
| `Assign`, op `=` | 7,469 | `=` |
| `Assign`, op `<<` | 317 | `<<` |
| `Postfix` | 113 | `++` + `--` |

`Block` nodes: 4,204 against 6,215 `begin`. The 2,011 difference is the 1,967
functions plus 44 prototypes, whose outer block the AST stores unwrapped.

Pinned without a token to check against: 5,820 declared names, 16,953
expression statements, 945 / 236 `return` with / without a value.

## Layout

```
crates/regent_core/src/gpl/
├── mod.rs      entry points: lex, parse
├── token.rs    Token enum, logos derive, keyword table
├── lexer.rs    source -> tokens + errors
├── ast.rs      syntax tree, one node per grammar rule
├── parser.rs   recursive descent, one function per grammar rule
├── dat.rs      .dat prototype files (own scanner)
├── proj.rs     .gplproj project files
├── bcd.rs      .bcd contents: section 1 walk, section 2 scope tables, section 3
├── emit.rs     source -> .bcd: the compiler's normalisation rules
├── exec.rs     .bcd -> instructions, the decoded execution form
├── load.rs     images -> one linked program, in the engine's load order
├── opcode.rs   opcodes: 39 named, 43 run; Op, Operator
├── ops/        what each operator does, per value class; list.rs the
│               list and its node numbering
├── runtime.rs  how the engine runs a function: values, calls, threads, errors
├── builtins.rs the 166 builtin registrations, signatures and facts
├── startup.rs  how a new game starts: first dispatches, agents, RNG seeds
├── vm/         the interpreter: mod.rs, host.rs (world trait), native.rs
│               (builtins run here), threads.rs (thread builtins and the
│               scheduler's run), text.rs (trace and error text), snap.rs
│               (to and from a save)
├── resolve.rs  corpus-wide call cross-check + builtin inventory
└── diag.rs     ariadne rendering

crates/regent_core/src/data/
└── gpltype.rs  GPL's 11 value types; TypeCode, Kind

crates/regent_core/src/base/
├── gfix.rs     GPL's float, 22.10 fixed point (Gfix)
└── rng.rs      the engine's random number generator

crates/regent_core/tests/
├── corpus.rs   the 306 shipped files, counts pinned exactly
├── mods.rs     the Workshop mods, counts as lower bounds (Steam updates them)
├── backend.rs  the back end, against the compiler's own output
├── load.rs     decode and load every image; every call resolves
├── opcode.rs   opcode table, against the lexer and the shipped bytecode
├── ops.rs      operators in worked cases
├── runtime.rs  the call graph of the shipped programs
├── builtins.rs builtin registrations, against the SDK docs and scripts
└── vm.rs       the VM on every function calling no world builtin
```

## Use

```sh
cargo test
cargo run --bin regent_cli -- check-gpl "$SDK/OriginalQuests" # lex + parse everything
cargo run --bin regent_cli -- parse   "$SDK/OriginalQuests"   # AST summary
cargo run --bin regent_cli -- resolve "$SDK/OriginalQuests"   # call resolution report
cargo run --bin regent_cli -- dat     "$SDK"                  # .dat prototypes
cargo run --bin regent_cli -- codesec "$SDK/OriginalQuests/Data/bytecode.bcd"
cargo run --bin regent_cli -- compile project.gplproj out.bcd # compile a project

# Mods call base-game functions, so resolve both trees together:
cargo run --bin regent_cli -- resolve "$SDK/OriginalQuests" "$WORKSHOP"
```

Tests read `MAJESTY_HD` (the SDK is its `SDK/`) and `MAJESTY_WORKSHOP` (environment or `.env`);
an unset variable fails the tests that need it.

## Design

- `logos` 0.16 lexer; hand-written recursive descent parser.
- One parser function per grammar rule, same name, so each tier can be checked
  against the compiler's grammar by eye.
- Zero-copy: tokens and nodes borrow `&str` from the source.
- No node for parentheses.
- Float text is kept, not converted: GPL floats are fixed point, `f64` would
  round. Integers are parsed (lossless).
- Errors are collected, capped at 20 per file, and recovered at statement
  boundaries.

## The resolver

Checks every `$Name` refers to something. This validates the parser and yields
the builtin inventory.

Result: 6,152 calls to corpus functions, 13,667 to builtins, 0 unresolved
(`every_function_reference_resolves`).

The SDK reference index has three errors. Two appear as unresolved names; the
Notepad++ template agrees with the corpus on both:

| Reference index | Actual |
|---|---|
| `AgentHasInventoryInItem` | `AgentHasInventoryItem` |
| `SetStoppedMusicCallback` | `SetMusicStoppedCallback` |

The reference's own heading for the first is correct; only the index is wrong.
The third, `RemoveAllBuildingLimits`, is missing from the index though fully
documented; nothing calls it. The template also lists `LogOn`, `LogOff` and
`NullFunction`, documented nowhere (`NullFunction` is named as
`LookupFunction`'s miss value). All four are in the inventory.

- 815 redefined names (`every_function_reference_resolves`): base game and
  expansion are separate compilation units, so `GPLMx/` redefines most of `GPL/`.
- Many functions nothing references: engine entry points (birth, death, quest
  hooks). `.dat` files bind many by name.
- 25 documented builtins the shipped game never calls, e.g. `AddAttribute` and
  `Halt`; mods use some (`the_builtin_inventory_is_complete`). An interpreter
  needs 141 for the base game, 166 for all; 16 of the 141 run natively in
  `gpl::vm`.

## Notes

- `logos` 0.16 rejects unbounded greedy patterns; `//[^\r\n]*` needs
  `allow_greedy = true`.
- `TaskModules/Characters/Garrison.gpl` and its expansion twin are wholly
  commented out yet listed in `path.gplproj`, so they produce no tokens.
  `tests/corpus.rs` pins exactly these two.
- The compiler's token names are not source spellings: `and`, `or`, `mod`,
  `not` lex as identifiers, and there is no `agentref` token
  ([language.md](language.md)).
- `EXPECTED_KEYWORDS` pins each of the 26 `Keyword` variants and asserts they
  sum to the aggregate 37,891.
- `do ... while` and `%=` have no corpus coverage. They follow the compiler's
  grammar and are marked untested in comments.

## The back end

`regent_cli compile` compiles a GPL project to a `.bcd`. All three sections are
written from source. Five of the SDK's seven projects come out byte-identical,
including the base game's `bytecode.bcd`; the two exceptions are asserted to
the byte (`a_whole_bcd_is_compiled_from_its_project`). **Game**: HD runs this compiler's
image: both probe traces loaded a probe image it compiled, with the one
`Snake_Pit_Spawn` mark put back.

```sh
cargo run --bin regent_cli -- compile "$SDK/OriginalQuests/GPL/path.gplproj" out.bcd
cargo run --bin regent_cli -- datasec "$SDK/OriginalQuests/Data/bytecode.bcd"
```

It mirrors `Gplbcc.exe` (`MakeGPL.bat`: `gplbcc -in <project>.gplproj -out
<name>.bcd`). A construct no corpus demonstrates (`do while`, a valueless
`return` inside `foreach`) is refused rather than guessed.

Block framing: `data::block`; the file layout: `data::bcd`. Block contents:
`gpl::bcd`. Compiler behaviour: `gpl::emit`. Format:
[formats/bytecode.md](../formats/bytecode.md).

### What the compiler does to a `.dat`

**Corpus**: the twelve base-game and expansion `.dat` files compiled against the
data sections of `bytecode.bcd` and `MX_Data.bcd`, then every other project on
disk (`tests/backend.rs`).

- Keys and values are stored inline as NUL-terminated strings.
- Values are text regardless of type: `9` → `"9"`, `False` → `"false"`. Only
  booleans are lowercased.
- Prototypes are sorted by name, case-insensitively, across the whole project.
- Keys are sorted case-insensitively within each prototype.
- Of two entries with the same case-folded key, the first wins
  (`first_of_two_duplicate_keys_wins`). A few cases in the twelve files, some
  with differing values: `Hooligan` `castingrange` 250 then 200 (keeps 250), `RatmanCatapult`
  `raider_respond` 1 then 0. `Gnome_Hovel`'s `Max_Members` and `max_members`,
  both 3, collapse to one.

### Evidence

| Check | Scale | Test |
|---|---|---|
| Byte-exact whole files | 5 of the SDK's 7 projects, plus the one data-only mod | `a_whole_bcd_is_compiled_from_its_project`, `guardhouse_mod_compiles_byte_for_byte` |
| Data sections decoded and re-encoded identically | at least 75 files, 3,101 prototypes, 43,977 values | `every_data_section_round_trips` |
| Projects whose `.dat` files reproduce a shipped data section | 20 of 21 | `every_project_reproduces_a_shipped_data_section` |
| Section-2 tables that walk and tally | 75 files, 253 prototypes, 11,473 functions, 7,741 constants | `section_one_counts_what_section_two_defines` |
| Definitions whose scope-record count is predicted from source | 3,544 of 3,544 | `section_two_records_count_the_names_in_scope` |
| Section-2 bodies decoded and re-encoded identically | 19,467 of 19,467 | `a_definitions_scope_decodes_and_names_its_own_table` |
| Section-1 entries emitted from source, record for record | 3,608 of 3,609 | `a_whole_code_section_is_built_from_the_sources` |
| Section-1 constant entries checked field by field against the parser | 1,511 | `section_one_records_a_constant_s_file_line_and_value` |
| Section-1 walked to the last byte | 75 files; 1,494 definitions in the base game | `the_record_walk_consumes_section_one_exactly`, `the_walk_counts_the_calls_the_parser_counts` |
| Calls read, against the parser's own count | 7,060, as the parser counts | `the_walk_counts_the_calls_the_parser_counts` |

The re-encode row proves only that the decoder loses nothing. The
reproduce-from-source row tests the normalisation (two sort orders, the
duplicate rule); most of those projects are mods, and nearly all also contain
`.gpl`.

The one miss, Majestic Majesty, is not a compiler difference: its shipped
`bytecode.bcd` was built from newer `.dat` files than it ships. Prototype names
match, all but two prototypes have our values as a strict subset of theirs,
and the keys only they carry are later features (`AttackPeriod`,
`Fire_Attack_Damage_Max`, …).

Workshop mods that ship a `<GPL>`'s sources and its `.bcd`, compiled from the
manifest's list (`openregent` `workshop_sources_compile_to_their_bcd`, at least
14): all but two are byte-exact once source paths are spelled as the image records them (debug
text, often the author's absolute path). Improved Guards matches as is. The
other two differ for reasons outside the compiler: a manifest missing its
`.dat`, and the `Snake_Pit_Spawn` mark. Two mods set rules:
Phantoms' Haunt, `return` inside `foreach`
([formats/bytecode.md](../formats/bytecode.md)); Monster Kingdom, these two:

- `expression #X -2` compiles to an `opNegate` of the literal, not a folded
  `-2`. The engine evaluates the records, so `exec` negates it too.
- A file's last prototype takes the line past its end: one more than
  `lines()` when the file ends in a newline.

`dat::Value::Int` holds a number, not its spelling, so `007` would emit as `7`.
No value in either corpus is affected.
`integer_spelling_is_normalised` records the limitation.

## Next

The code back end is complete except for the `Snake_Pit_Spawn` mark byte and
the two undemonstrated constructs. A playable game does not need it (HD ships
the sources). It shows that the front end reads GPL as `Gplbcc.exe` does:
reproducing a whole image of another compiler's output cannot happen by
accident.
