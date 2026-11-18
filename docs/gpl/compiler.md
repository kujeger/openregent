# The GPL front end and `.dat` back end

`crates/regent_core/src/gpl/`: lexer, parser, `.dat` reader, a resolver that
cross-checks the corpus, and a back end that compiles a project. Language and
grammar: [language.md](language.md).

## Status

Complete against the 306 shipped `.gpl` and 15 `.dat` files
(`every_file_lexes_without_error`, `every_dat_file_parses`).

| Check | Shipped |
|---|---|
| Files lexed | all, zero errors, 253,176 tokens (`token_counts_match_the_survey`) |
| Files parsed | all, zero errors |
| `.dat` files parsed | 417 prototypes, 5,176 values (`every_dat_file_parses`) |
| `$name` references resolved | 19,819 of 19,819 (`every_function_reference_resolves`) |

Forms the shipped files do not use are held on made-up projects
(`tests/mod_gpl.rs`, `tests/fixtures/gpl/`).

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
├── mod_gpl.rs  forms only mods use, on made-up projects (fixtures/gpl/)
├── backend.rs  the back end, against the compiler's own output
├── gplbcc_diff.rs  Gplbcc.exe and emit on the made-up projects
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
```

Tests read `MAJESTY_HD` (the SDK is its `SDK/`; environment or `.env`);
unset, it fails the tests that need it.

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
  `Gplbcc.exe` refuses `do ... while` ("in rule statement, no viable
  alternative for tree"), as ours does (`both_compilers_refuse_the_same_sources`).

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
<name>.bcd`). A construct no corpus or `Gplbcc.exe` run demonstrates (a
`return` inside a `while` in a `foreach`, two in one loop) is refused rather
than guessed. As in `Gplbcc.exe`,
a name no parameter or local declares, in any case, is an error ("identifier
foo not declared"): a bare `foo(1)` too, since a script's function is `$foo`
(`an_undeclared_identifier_is_refused`).

Made-up projects in `crates/regent_core/tests/fixtures/gpl/` hold the forms
only mods use, with no game data (`tests/mod_gpl.rs`). `tests/gplbcc_diff.rs`
runs `Gplbcc.exe` on the same projects (under `wine` off Windows): the same
bytes, or both refuse. Both refuse a definition with no `declare`
("missing DECLARE"), though the grammar marks the block optional
(`a_definition_needs_its_declare`).

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
| Data sections decoded and re-encoded identically | at least 18 files, 1,017 prototypes, 12,607 values | `every_data_section_round_trips` |
| SDK projects whose `.dat` files reproduce a shipped data section | all 4 with one (GOG; Steam adds a 5th) | `every_project_reproduces_a_shipped_data_section` |
| Section-2 tables that walk and tally | 18 files, 108 prototypes, 4,830 functions, 3,815 constants | `section_one_counts_what_section_two_defines` |
| Definitions whose scope-record count is predicted from source | 3,544 of 3,544 | `section_two_records_count_the_names_in_scope` |
| Section-2 bodies decoded and re-encoded identically | 8,753 of 8,753 | `a_definitions_scope_decodes_and_names_its_own_table` |
| Section-1 entries emitted from source, record for record | 3,608 of 3,609 | `a_whole_code_section_is_built_from_the_sources` |
| Section-1 constant entries checked field by field against the parser | 1,511 | `section_one_records_a_constant_s_file_line_and_value` |
| Section-1 walked to the last byte | 18 files; 1,494 definitions in the base game | `the_record_walk_consumes_section_one_exactly`, `the_walk_counts_the_calls_the_parser_counts` |
| Calls read, against the parser's own count | 7,060, as the parser counts | `the_walk_counts_the_calls_the_parser_counts` |
| Made-up projects compiled by `Gplbcc.exe` and by `emit` | the same bytes, or both refuse | `gplbcc_diff.rs` |

The re-encode row proves only that the decoder loses nothing. The
reproduce-from-source row tests the normalisation (two sort orders, the
duplicate rule).

No shipped project uses `break`, `continue`, `return` inside `foreach` or a
negative constant. Made-up projects hold them: `tests/mod_gpl.rs` checks their
records, and `Gplbcc.exe` writes the same bytes for them (`gplbcc_diff.rs`).
`return` inside `foreach`: [formats/bytecode.md](../formats/bytecode.md). And,
first seen in Monster Kingdom (Workshop `3743606613`):

- `expression #X -2` compiles to an `opNegate` of the literal, not a folded
  `-2`. The engine evaluates the records, so `exec` negates it too.
- A file's last prototype takes the line past its end: one more than
  `lines()` when the file ends in a newline.

`dat::Value::Int` holds a number, not its spelling, so `007` would emit as `7`.
No shipped value is affected.
`integer_spelling_is_normalised` records the limitation.

## Next

The code back end is complete except for the `Snake_Pit_Spawn` mark byte and
the two undemonstrated constructs. A playable game does not need it (HD ships
the sources). It shows that the front end reads GPL as `Gplbcc.exe` does:
reproducing a whole image of another compiler's output cannot happen by
accident.
