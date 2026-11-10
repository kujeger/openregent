# `.bcd` — GPL bytecode

Container and its three sections. Language: [gpl/language.md](../gpl/language.md);
compiler: [gpl/compiler.md](../gpl/compiler.md).

> Part of [README.md](README.md).

`.bcd` is the program for the `Gpl*` VM. HD loads it from the `<GPL>` lists of
`MajestyDatasetDefinitions.xml`
([below](#the-decoded-form-and-the-load)).

Corpus: 75 `.bcd` files — HD 9, HD modding SDK 9, eleven Workshop mods 57.

## Block framing

```
[u32 len]  [body ...]  [u32 0]  [i32 -1]
 ^ len counts itself, the body and the 8-byte terminator
```

A block at `X` with length `L` spans `X .. X+L-1`. A `.bcd` is a tree of blocks.
No alignment or padding.

**Inferred**: the writer is `MemSerialBlock`, which also backs network messages.
Save games use a different tag/size/version/count chunk tree.

## File shape

```
0x00  u32   file size - 4
0x04  u32   file size - 4     outer block length; the block ends at EOF
0x08  i32   -1                fixed pair, identical in every file; meaning unknown
0x0c  u32   0
0x10        section 1         code and debug info, one entry per name
            section 2         scope table: three name tables
            section 3         data: prototype instances from `.dat` files
```

Only `0x00` is a header outside the outer block; the outer block's terminator is
the last eight bytes. The three sections tile its body. **Corpus**:
`data::bcd::Container` checks the header and the tiling on all 75 files
(`every_image_decodes_one_instruction_per_record`). **Binary**: `Gplbcc.exe`
prints "Serializing data files..", "Serializing symbol table..",
"Serializing bytecode..".

`data::bcd::Sections` names section 1 `symbols` and section 2 `code`; both names
are misnomers.

## Section 2 — the scope table

```
[u32 1]
[u32 nprototypes]  ( cstr name   [u32 len] body [u32 0][i32 -1] )*
[u32 nfunctions]   ( cstr name   [u32 len] body [u32 0][i32 -1] )*
[u32 nconstants]   ( cstr #name  [u32 len] body [u32 0][i32 -1] )*
```

- Constants keep their `#` sigil.
- Each table sorts by byte order (`Advanced_Attack` before `adept_birth`);
  section 1 sorts case-insensitively.
- Section 1 opens `[u32 0][u32 count]`; `count` is the number of distinct names
  section 2 defines. Majestic Majesty defines `StringInList` twice: two entries,
  one name.
- Corpus total: 253 prototypes, 11,473 functions, 7,741 constants
  (`section_one_counts_what_section_two_defines`); 19,467 bodies
  (`a_definitions_scope_decodes_and_names_its_own_table`).
- HD `bytecode.bcd`: 20 prototypes, 825 functions, 649 constants; the files
  `path.gplproj` lists hold the same prototypes and constants
  (`the_base_game_code_section_matches_its_sources`).
- `GPL/` has two unlisted files: `Garrison.gpl` (fully commented out) and
  `Mad_Temples.gpl` (functions the image lacks).
- Every name in `data/bytecode.bcd` is defined in the base game's sources, and
  vice versa, bar `Mad_Temples.gpl`'s.

Code: `gpl::bcd::ScopeSection`; re-encodes byte-identically on all 75 files
(`a_definitions_scope_decodes_and_names_its_own_table`).
`regent_cli codesec` prints definitions and body sizes.

### A record is a name in scope

> A definition's record count (body length / 20) is 1 + the number of names
> declared by it and by every earlier definition in the same source file —
> parameters and `declare` lines alike. Resets per file.

Holds on 3,544 of 3,544: base game 1,494, plus 2,050 across four expansion
projects (Build, Data, Decision, Task) and the SDK example `WrathOfKrolm`.
`section_two_records_count_the_names_in_scope`.

So size depends on position, not content:

- `Cultist_tree`: 26 statements, 2 records.
- `end_game_script_expert`: 5 statements, 449 records, late in a long file.
- Single-record constants sit in files declaring nothing before them; the
  large ones sit late in files full of functions.

### What a body holds

```
[u32 1] [u32 returns] [u32 kind]
[u32 n]  ( [u32 20][u32 1][u32 type][u32 0][i32 -1] )*   the declares
[u32 m]  ( same )*                                       the parameters
```

Header (12 bytes) plus the two counts (8) make the "1 +" record. Each list entry
is a framed block with a two-word body (flag, type) — 20 bytes, hence the
multiple of 20.

| Field | Meaning |
|---|---|
| `returns` | Function's `is <type>`, 0 without one; 11 (`unknowntype`) on prototypes and constants |
| `kind` | 0 prototype, 1 function, 2 constant — repeats which table holds the entry |
| lists | Type of every `declare`d name and every parameter |

- List types are only 1–8, the source-level types. 9–11 (`reference`,
  `undefined`, `unknowntype`) appear only in section 1 value slots.
- Lists accumulate across the file, so late functions carry hundreds of unused
  entries. Only types are stored; names are in section 1.
- Code: `gpl::bcd::Scope`. `a_definitions_scope_decodes_and_names_its_own_table`:
  all 19,467 bodies decode exactly, `kind` matches the table.
- `gpl::emit::scope_section` builds it from source; all seven SDK images match
  byte for byte (`a_whole_scope_section_is_built_from_the_sources`).

Locals do not live here: they live in each function's records, one set per
function ([How the engine runs a function](../gpl/runtime.md#how-the-engine-runs-a-function)).
**Inferred**: section 2 is a single-pass compiler's symbol table dumped at each
definition's end.

## Section 1 — code and debug info

```text
[u32 0] [u32 ndefinitions]
  ( cstr name  [u32 nrecords]  record* )*
```

One entry per name, case-insensitive order, so `#` constants come first. Base
game: 649 constants then 845 functions and prototypes
(`the_base_game_code_section_matches_its_sources`).

### The record

**Binary**: shape read from `Gplbcc.exe`'s record writer, not from the files:

```text
[mark]                    a header, same shape as a value slot
[value][value]            [u32 word][u32 extra][cstr text][u8 kind][u8 flag] each
[u32 head]                numeric operand; meaning depends on opcode
[cstr text]               the symbol this record is about, often empty
[u16 line]                source line, 65535 = none
[u8 n] [u16 slot × n]     operands; the first is the result slot
[u8 opcode]
```

Minimum 42 bytes (empty strings, no operands).

| Field | Meaning |
|---|---|
| mark | Header, decided by opcode ([below](#the-mark-is-decided-by-the-opcode)) |
| value slot ×2 | Operands: slot reference, or immediate plus type |
| ↳ `kind` | Type index into [`data::gpltype`](#value-types) |
| ↳ `word` | Integer immediate, or slot number |
| ↳ `extra` | Float literal, fixed point ×1024 |
| ↳ `flag` (trailing byte) | 2 = `word` holds the value (a slot number, or an integer/boolean literal); 0 otherwise |
| head | Record index (jump target) or parameter position |
| text | Symbol name |
| line | Source line |
| operand list | Call's result slot and arguments |
| opcode | Operation; its arity = number of value slots used |

`gpl::bcd::walk` consumes section 1 exactly: 19,466 definitions, 1,262,666
records. It searches nothing — counts and lengths are stated — so a wrong shape
fails instead of drifting. `the_record_walk_consumes_section_one_exactly`.

`gpl::bcd::encode` writes it back byte-identically: 63,761,000 bytes. All three
sections round-trip, so no byte is unaccounted for. `every_code_section_round_trips`.

Cross-checks:

- Base game: 7,060 `opLParen` records, as the parser's 7,060 call nodes
  (`the_walk_counts_the_calls_the_parser_counts`).
- 19,466 definitions vs 19,467 section 2 bodies: the duplicate `StringInList`.
- `Acid_Bolt_Hit` walks to: source file, itself, `__Return_value__`,
  `thisagent`, `target`, then `12 <- (10 11)`, `13 <- (8 9 12)`,
  `17 <- (14 15 16)`.

### Value types

`data::gpltype`, one-based, as the executables list them:

```
1 integer  2 float  3 string  4 list  5 agent  6 boolean  7 coord
8 function  9 reference  10 undefined  11 unknowntype
```

- First value slot `kind`: `unknowntype` empty, `string` on a literal,
  `function` on a called builtin, `undefined` on a parameter.
- Second value slot `kind`: 0–11; 0 = no type yet (`__Return_value__`).
- `reference` appears only on a `#Name` constant operand.
  `every_type_a_record_carries_is_in_the_engines_list` walks 36,819 records of
  constant and prototype entries: ten types, never 0, never `reference`.

### Opcode table

In each game executable's data. 39 records, fixed stride, bounded by zeros (**Binary**):

```text
0x00  char[25]  name    "opAssignEqual"
0x19  char[25]  token   "="
```

`gpl::opcode` finds it by searching for `opAssignEqual`.

| | | | |
|---|---|---|---|
| 0 `opAssignEqual` `=` | 1 `opTimesEqual` `*=` | 2 `opDivideEqual` `/=` | 3 `opModEqual` `%=` |
| 4 `opPlusEqual` `+=` | 5 `opMinusEqual` `-=` | 6 `opInserter` `<<` | 7 `opOr` `\|\|` |
| 8 `opAnd` `&&` | 9 `opEqual` `==` | 10 `opNotEqual` `!=` | 11 `opLessThan` `<` |
| 12 `opGreaterThan` `>` | 13 `opLessThanOrEqualTo` `<=` | 14 `opGreaterThanOrEqualTo` `>=` | 15 `opPlus` `+` |
| 16 `opMinus` `-` | 17 `opStar` `*` | 18 `opDivide` `/` | 19 `opMod` `%` |
| 20 `opApostrophe_s` `'s` | 21 `opInc` `++` | 22 `opDec` `--` | 23 `opNegate` `-` |
| 24 `opNop` | 25 `opLocalize` | 26 `opReference` `declare return` | 27 `opDeclare` `declare` |
| 28 `opAdvance` | 29 `opRewind` | 30 `opExit` | 31 `opIf` `if` |
| 32 `opForeach` `foreach` | 33 `opWhile` `while` | 34 `opContinue` `continue` | 35 `opBreak` `break` |
| 36 `opReturn` `return` | 37 `opReturnValue` `return` | 38 `opLParen` `call` | |

- The 34 non-empty tokens are all spellings the lexer (written without this
  table) produces; 32 distinct, since `-` and `return` repeat.
  `every_opcode_token_is_one_the_lexer_knows`.
- No token: `opNop`, `opLocalize`, `opAdvance`, `opRewind`, `opExit` — the
  compiler's own.
- `opNegate` and `opMinus` share `-`; arity tells them apart.
- `Gplbcc.exe` has no such table. **Inferred**: the names serve the VM's
  debugger.

**The array position is the opcode byte.** The call terminator `0x26` was read
off the files independently; `0x26` = 38 = `opLParen`, token `call`.
`the_call_terminator_is_the_call_opcode`; `gpl::bcd` uses `opcode::CALL`. On
`Remove_Krolms_Rage` (`mdl1.bcd`, source available): `opEqual` sits on the line
of its only `==`, `opIf` on its only `if`, lines ascend, arities match.

### Opcode 39 — body start, unnamed in the table

Not in the table (bytes past entry 38 are unrelated). Appears exactly once per
function, never in constants or prototypes. HD base game: 825, the section 2
function count. Five functions end on it (empty body).
`opcode_thirty_nine_separates_declarations_from_the_body`.

Only six opcodes precede it:

| Opcode | Declares |
| --- | --- |
| 28 `opAdvance` | source file, and the record-count entry — two per function |
| 24 `opNop` | the function's own name |
| 26 `opReference` | `__Return_value__` |
| 30 `opExit` |  |
| 27 `opDeclare` | a `declare`d local |
| 25 `opLocalize` | a parameter |

No name exists for it: not in the executables, the debugger's panels
(`the_gpl_debuggers_panels_are_five_shipped_as_six`) or its manual.
The table's one reader accepts 0 to 42 and prints 39 as an empty name (`opcode::HD_NAME_BOUND`; **Binary**).

### The VM runs 43 opcodes

**Binary**, `MajestyHD.exe`'s interpreter loop.

| Opcode | Run by |
|---|---|
| 0–23, operators | the left operand's value class ([below](#what-an-operator-does)) |
| 24–42 | one switch, a case each |
| 43 and up | a thrown "Unknown operation attempted in interpreter" |

- 39 shares `opNop`'s case: nothing at run time. At exit the loop walks the
  `opDeclare` records up to 39 and resets each local to its type's default;
  it does not release it.
- 40, 41, 42 have their own cases, which log `GetAttribute(inl)`,
  `SetAttribute(inl)`, `AdjustAttribute(inl)` to the profiler. No shipped
  `.bcd` uses them; 39 is the highest opcode in all 75. **Inferred**: inlined
  attribute access the shipped compiler never emits.

The dispatch: **Binary**. The corpus:
`no_shipped_bytecode_uses_the_inline_attribute_opcodes`. Constants in
`gpl::opcode`.

### What an operator does

**Binary**, `MajestyHD.exe`. An instruction holds result, left and right
operands. An operator runs in two steps:

```text
result slot empty?  look up the result type of (op, type(left), type(right));
                    make a value of that type, kept for every later run;
                    no row, or 12: the slot stays empty
left's class runs the op
  =, *=, /=, %=, +=, -=, <<, ++, --   write the left value in place
  the rest                            write the result slot through an
                                      accessor: int&, float&, bool&, string&
```

The operator does not choose the result's type; the slot has one already. An
accessor on the wrong class throws "bad conversion to" its name. An empty
slot is `12`, and so is a missing right operand.

The result-type table is 67 rows of `[op, left, right, result]`, ended by op
24. It is not the classes: it types `int == float`, `list + list` and `++` on
a float, and types no string ordering and no `&&` of mixed types, which the
compiled record must then type.

Nine classes; codes 9–11 have none:

| Class | Handles | Right side read as |
|---|---|---|
| `GplNull` | every op, doing nothing, `=` included | — |
| `GplInteger` | all but `<<` `\|\|` `&&` `'s` | see below |
| `GplFloat` | all but `%=` `<<` `\|\|` `&&` `%` `'s` | `float` |
| `GplString` | `=` `+=` comparisons `+` `'s` | `string&` |
| `GplList` | `=` `+=` `-=` `<<` `==` `!=` | a copy, `list&` |
| `GplAgentRef` | `=` `==` `!=` `'s` | `gplagent&` |
| `GplBoolean` | `=` `\|\|` `&&` `==` `!=` | `bool&`, `bool` |
| `GplCoord` | `=` `==` `!=` | `coord&` |
| `GplFunction` | `=` `==` `!=` | `gplfunction&` |

Anything else throws "Unsupported operation attempted on X type", X the class
in capitals. All throws are `GplException`; the thread run catches it and
abandons the call ([Errors](../gpl/runtime.md#errors)).

Floats are fixed point, 10 fractional bits. Multiply keeps bits 10–41 of the
64-bit product: floors, wraps. Divide divides `a << 10` in 64 bits: truncates;
a zero divisor or a quotient past 32 bits faults. An integer becomes fixed by
`<< 10`, a float an integer by an arithmetic `>> 10`, which floors.

`GplInteger` does its arithmetic in fixed point. Where a right float matters
it tests the right operand's type code, not the result's:

| Op | Right float | Right integer | Other right |
|---|---|---|---|
| `==` | false, unread | `a == b` | `int&` throws |
| `!=` `<` `>` `<=` `>=` | `a << 10` against `f` | `a << 10` against `b << 10` | `float` throws |
| `+` `-` | `float&` = `a << 10 ± f` | `int&` = `(a << 10 ± b << 10) >> 10` | throws |
| `*` | `float&` = `mul(a << 10, f)` | `int&` = `mul(a << 10, b << 10) >> 10` | throws |
| `/` | zero check, `float&` = `div(a << 10, f)` | same, `int&` = `div(..) >> 10` | throws |
| `=` `+=` `-=` `*=` | right side floored first | as is | throws |
| `/=` | zero check on `f`, then integer divide by `f >> 10` | integer divide | throws |
| `%` `%=` | `int&` throws | integer divide, no zero check | throws |

Consequences:

- Integers past 2^21 = 2,097,152 wrap in `+ - * /`, `!=` and the orderings:
  `3000000 + 0` is `-1194304`, `3000000 > 0` false, `2000 * 2000` `-194304`.
  `==` is exact.
- `-7 / 2` is `-4`: truncation in fixed point, then a floor.
- `2 == 2.0` is false, `2 != 2.0` false, `2.0 == 2` true.
- `x /= 0.5` divides by zero past the check: a CPU fault, not a
  `GplException`. **Inferred**: the game crashes. So does `x % 0`.
- `x /= -0.5` negates `x`; `x *= 2.5` doubles it.

Other classes:

- Strings compare case-insensitively: `"Gold" == "GOLD"`; `"a" 's "b"` is
  `"a'b"`.
- Lists: `==` walks both until either ends, so a prefix is equal and `[]`
  equals everything. `-=` removes every element equal to the right value, by
  the element's own equality. `=` clears before it reads `list&`, so
  `l = 5` empties `l` and throws, and `l = l` empties it.
- `||` and `&&` read the right side only when the left does not decide, but
  both operands were computed by earlier records. `true || 5` is true,
  `false || 5` throws.
- `agent's "x"` points the result slot at the attribute value itself; a
  missing attribute or agent throws, nothing is created.
- Assigning a function to a running thread stores it as pending.

Held by worked cases in `tests/ops.rs`; the class switches, result types and
fixed-point cores: **Binary**. The model is
`gpl::ops::apply`.

### Function prologue

`a_definitions_prologue_follows_from_its_signature`:

```text
[0] opAdvance   head 4              source file
[1] opNop                           own name
[2] opExit
[3] opAdvance   head = nrecords     line 65535
[4] opReference                     __Return_value__ (type = `is <type>`)
    opLocalize  head = 1, 2, ...    one per parameter, in order
    opDeclare                       one per declared name, in order
    (39)                            body starts
```

- First record: `opAdvance` head 4 on 11,472 functions; `opNop` head 0 on 7,994
  constants and prototypes. Nothing else.
- `opLocalize` head = 1-based parameter position, 14,743 of 14,743.
- **Corpus**, not test-pinned: the 39 sits on the last `opDeclare`'s line.
- Against `OriginalQuests` sources: all 825 functions, 1,003 parameters, 1,926
  locals match in name, order and type (multi-name lines flattened).
  `the_prologue_pairs_with_the_signature_it_came_from`.

Pair sources with the SDK's own `.bcd`, not the shipped one. With the debugger
on, the engine loads the SDK images because their symbols match the SDK
sources. The shipped image records absolute paths
(`\projects\ap\gpl\TaskModules\SubTasks\Spells.gpl` vs relative
`TaskModules\SubTasks\Spells.gpl`) and per-file shifted lines.

### A record's index is its slot number

A call's result slot is always its own record index: 221,978 of 221,978.
Record *i* computes slot *i*; an operand naming slot *j* names record *j*.
Expressions are numbered in post-order: three-address code
(`GTS_OperandNode`, [original/architecture.md](../original/architecture.md)), not a stack
machine. `a_records_index_is_its_expression_slot`.

The `u16` after an entry's name is therefore the low half of its `u32`
record count, not a slot count: equal on the base game's definitions
(**Corpus**; `bcd::walk` reads the `u32`).

`Fire_Strike_Active`, `if (target != $nullagent()) $Fire_Strike_Damage(target);`:

```text
 [6] opLocalize  target
 [8] opLParen    slots [8]      $nullagent()  -> slot 8
 [9] opNotEqual                 slots 6 and 8 -> slot 9
[10] opIf        head 13        on slot 9
[11] opNop                      slot 6        -> slot 11
[12] opLParen    slots [12, 11] $Fire_Strike_Damage(<11>)
```

### Calls

A function's `opLParen` records are its calls in post-order (arguments first,
inner before outer): 824 of 825 base-game functions, 7,058 calls.
`the_bodys_calls_are_the_trees_calls_in_post_order`.

The exception, `Guardian_Birth`, uses the corpus's only parenthesised callee,
`($Monster_Birth)(ThisAgent);`:

```text
[10] opNop                    $Monster_Birth by name  -> slot 10
[11] opNop                    slot 5 (ThisAgent)      -> slot 11
[12] opLParen slots [12, 11]  callee = slot 10, by reference
```

The first value slot is the callee: a name typed `function`, or a slot
reference (`flag` 2, `word` = slot). 221,724 named, 254 through a slot, never
neither. `every_calls_callee_is_a_name_or_a_slot`.

`opNop` materialises a value into its own slot, from a name and type or from
another slot.

### Builtins: one map of names

**Binary**, `MajestyHD.exe`. Two registration functions put 166 names into one
case-insensitive map: a core block of 70, then a game block of 96. A loaded
`.bcd` adds its functions to the same map, each bound to the interpreter loop.

A registration is a name, a native function, two contexts (0 for a builtin,
the code for a script function) and a return type, a `data::gpltype` code (0
is none). Nothing records arguments. A call:

```text
opLParen  first run   a value of the callee's return type,
                      cached in the instruction as the return slot
          every run   function(args, context a, context b)
```

`args` is a vector of values: 0 the return slot, 1 on the arguments. A
builtin fetches each by index and converts it through one of 18 accessors;
each default throws "bad conversion to" its type, so the accessor names the
argument's type.

- 165 names are the SDK's documented builtins bar `ReRandom`, which nothing
  registers. The 166th is the empty name, bound to `NullFunction`'s function.
- The return type is the slot's type before the call. Two builtins put a
  fresh value there first: `GetPlayerData` an agent for `"palace"`,
  `ListMember` one of the member's type (`builtins::Builtin::retype`). Every
  other return slot is converted as registered (**Binary**).

What they touch — **Binary** by block, **Inferred** by name within it:

| Block | Groups |
|---|---|
| core | movement and paths; coordinates, `RandomNumber`, `Sqrt`; lists; threads and function values; agent create, delete, parent, container; GPL attributes (`Has`, `Add`, `RemoveAttribute`); effectors; fades, minimap, sound, music; debug output |
| game | unit spawn, type, teleport, hide; engine attributes (`Get`, `Set`, `Adjust`, `MagicalAdjustAttribute`); player data, teams, owners; spells and speed trails; inventory; victory and loss; buildings, palaces, limits, rewards; map reveal; titles and lairs; message flags |

#### Arguments, read from the code

**Binary.** A small abstract interpreter (decoding with `iced-x86`) walked
each builtin's machine code, every path and every helper an argument is
handed to. It recorded each argument fetch, each
type-code check, each accessor called, each check of the argument count, and
each loop over the remaining arguments (a rest).

- An accessor takes the value classes overriding it: one each, but the
  by-value `int` and `float` take integer and float.
- An argument's types are the union over paths of its accessors, narrowed by
  the type checks on each path; a null too if code 0 is checked for. A copy
  into a fresh value takes any type.
- An argument past the fewest a call can pass is optional: fewer throws a
  count check, or reaches a fetch past the end.
- All 166 settle from the code; none falls back on the reference. The table
  is `builtins::HD`, one `Signature` each.
- Every shipped and Workshop call passes the required arguments. Four
  builtins get one they never read: `CastSpell` (a fourth `""`),
  `LocationOf`, `SetThreadInterval`, `TeleportToPoint`.
- Reading a value as text says nothing of its type: error paths print every
  argument that way.

Where the binary and the reference disagree:

| Builtin | Binary | Reference |
|---|---|---|
| `SpawnGraveyard`, `CreateSpellUnit` | returns agent | nothing |
| `AdjustAttribute`, `MagicalAdjustAttribute` | returns integer | nothing |
| `ListPalaces` | returns list | nothing |
| `PlaySound` | returns boolean | nothing |
| `AgentNumber` | takes an integer, returns an agent | takes an agent, returns an agentRef |
| `CreateSpellUnit` | reads a third argument, an agent | two |
| `CreateAgent` | name required; a null means none | optional |
| `CreateNewInventoryItem`, `IsSpellAvailable` | third is an integer, read through `int&`: `true` throws | boolean |
| seven inventory builtins | item by integer or string | integer |
| `Breakpoint` | boolean, integer or string | boolean or integer |
| `DebugOut` | any number of any type, printed only under an engine debug flag or with 911 first | one string |
| `MessageFlag` | message optional | required |
| `Move` | modifier optional, then an optional integer | modifier required, no integer |
| `SpecifyName` | string or integer | string |
| `Sqrt`, `SetEffectorDirection` | read by value: integer or float | float; integer |
| `SpawnUnit` | options by type code, any order | coordinate, integer, string, list, in order |
| `ListObjects` | modifiers integer, string or coordinate | integers |
| `PlaySound` | one signature, the union of three overloads | three listed |

Four registered names are not in the reference: `LogOn`, `LogOff`,
`NullFunction`, `SetMusicStoppedCallback`.

Held by `corpus_calls_pass_what_the_builtins_read`; the registrations,
accessors, arguments and return slots: **Binary**.
Table in `gpl::builtins`.

### Arity: the two value slots are the operands

Number of used value slots is fixed per opcode, except `opNop` (0 or 1).
`an_opcodes_arity_is_how_many_value_slots_it_uses`.

| Arity | Opcodes |
|---|---|
| 2 | 20 binary operators, `opForeach` (variable, iterable), `opReturnValue` |
| 1 | `opInc`, `opDec`, `opNegate`, `opIf`, `opWhile` (condition), `opReturn`, `opLParen` (callee), `opLocalize`, `opDeclare`, `opReference` (slot holds the declared *type*) |
| 0 | `opAdvance`, `opRewind`, `opExit`, `opBreak`, `opContinue`, 39 |

- `opMinus` 2, `opNegate` 1: same token, split by arity.
- An operand is a slot reference (`flag` 2, `word` = slot, typed `undefined`)
  or an immediate (literal in `word` or `text`, type in `kind`).
- Some `opPlus` records have two immediates: the compiler does not fold constants.

### `head` is a jump target

Seven opcodes always carry a non-zero `head`; the other 32 never do.
`a_records_head_is_set_on_exactly_the_control_transfer_opcodes`.

- `opIf`, `opWhile`, `opContinue`, `opBreak`, `opAdvance`, `opRewind`: record
  index.
- `opLocalize`: parameter position.
- None on `opForeach` (cursor opcodes jump for it), `opReturn`,
  `opReturnValue`, `opLParen`.

All 188,101 jump heads land in `0..=nrecords`; 3,812 go backwards, only on
`opRewind` and `opWhile` (loops). `Fire_Strike_Active`'s `opIf` heads to 13 in a
13-record definition: one past the end skips the body of an `if` without
`else`. `a_jumps_head_lands_on_a_record`.

**Inferred**: `opAdvance`/`opRewind` are a `foreach` cursor.

### The mark is decided by the opcode

Only `word` (0 or `0xFFFFFFFF`) and the `kind` byte, here called `tag` (1, 10,
11, 12), vary; `extra`, `text` and `flag` are always zero, so it is no value.
`a_records_opening_slot_carries_no_payload`. `word` is `-1` exactly when `tag`
is 10:

| Mark | Opcodes | Yields |
|---|---|---|
| `-1`, 10 | `opAssignEqual`, `opTimesEqual`, `opDivideEqual`, `opPlusEqual`, `opMinusEqual`, `opInserter`, `opInc`, `opDec`, `opLocalize`, `opDeclare` | a place |
| `0`, 12 | 14 value-computing ops: arithmetic, comparison, `opAnd`, `opOr`, `opNegate`, `opApostrophe_s` | a temporary |
| `0`, 11 | everything else: control flow, `opLParen`, `opReference`, `opExit`, 39 | neither |

- `opNop`: tag 10 with an operand, 11 without.
- 12 is past the type list, so `tag` is not a type index.
- Recomputable from the opcode; an emitter need not store it.
- `opModEqual` never occurs in the corpus.
- **Unexplained**: `Snake_Pit_Spawn`'s `opPlus` has tag 1 (`integer`) where all
  others have 12, in all three files shipping it. Its operands are two `integer`
  references, like many others. `a_records_mark_is_decided_by_its_opcode`.

### Float literals

A value slot's `extra` is non-zero on 344 slots, all typed `float`. Fixed
point, scale 1024, truncated: `100.` = 102400, `1.5` = 1536, `0.2` = 204.
Checked against SDK sources literal for literal. The shipped float constants
are all 1.5 = 1536; `word` holds integer immediates.
`a_float_literal_is_fixed_point_with_a_scale_of_1024`.

### Constants and prototypes

Neither is a function with parts missing: no record count, no
`__Return_value__`, no 39; both open with `opNop`.

Constant: three `opNop` records, all on the `expression` keyword's line:

```text
[0]  source file
[1]  #Name, as written
[2]  value, first slot, marked as a place
```

**Corpus**: constants are nearly all integers; the others are floats,
strings or a boolean.

Prototype: two header records, then one `opDeclare` per member (declared type,
own line). No prototype takes a parameter; a prototype with a statement compiles it
to nothing. The header records carry the line where the *next*
definition starts (or the file's last line). **Inferred**: the compiler
writes a prototype only after reading past `end`.

### What the shipped debugger says

HD ships a GPL debugger: `UseGPLDebugger` or `-gpldebug`; UI in
`GPLDebuggerUI.cam` ([panels.md](panels.md)); manual
`SDK/Documentation/GPL Debugger.pdf`. Its manual confirms:

- Micro Step runs one GPL operation of the current line: a line holds
  several operations, the record and its `u16 line`.
- The call stack lists each call with the calling agent and trigger time:
  calls nest at run time, but share each function's one set of slots.
- With the debugger on, the engine loads the SDK's `.bcd`, whose symbols match
  the SDK sources; the manual says those differ from the originals only in
  formatting and comments. So the SDK sources are the game's sources.
- Hotkeys: `_` open, `|` break when the selected object runs GPL, `+` break on
  any object of its type. Objects do not run GPL every frame.

### Emitter

`gpl::emit::function` compiles a definition from its parse tree, given only the
source file's spelling, and matches `Gplbcc.exe` record for record:

| Project | Functions | Exact |
|---|---|---|
| `OriginalQuests` (base game) | 825 | 825 |
| Expansion's five and `WrathOfKrolm` (SDK example, not used while fitting) | 1,158 | 1,158 |

Only difference: the `Snake_Pit_Spawn` mark byte. `the_emitter_reproduces_every_base_game_function`,
`the_emitter_holds_on_projects_it_was_not_fitted_to`.

Rules the emitter needed:

- A `#Name` constant keeps its sigil and is typed `reference`.
- `flag` 2 means `word` holds the value. Integer and boolean literals carry it;
  a slot number is such a value, typed `undefined`. Flag-0 integers and booleans
  occur only on declarations (declared type, `word` 0).
- Brackets are not erased: a parenthesised leaf gets its own slot, so
  `($Monster_Birth)(ThisAgent)` differs from `$Monster_Birth(ThisAgent)`. The
  parser keeps brackets inside the expression span.
- `__Return_value__`'s type slot carries the `is <type>` clause.
- A call argument, returned value or condition that needs a bare slot number
  gets an `opNop` first. An operator's operand may name a variable directly.
- Source lines — a record sits on the line of the last thing that fed it:
  - operator: its right operand's line (the record it produced, else its
    position). `a &&\n b` puts `opAnd` on `b`'s line; `x =\n $f(\n a)` puts the
    assignment on the call's.
  - call: its last argument's line, not the closing bracket's.
  - `else` jump: the greatest line its branch touches (an empty `begin end` has
    a line but no record).
  - loop `opRewind`: the body's last statement's line (a `break` record has no
    line).

**`break` and `continue`**: no shipped source uses them; Workshop mods do
(`mods_exercise_forms_the_base_corpus_never_uses`). `head` is the enclosing `opForeach`'s
record index — the loop, not a destination — and the record has no line. Hence
`break` outside `foreach` is a compile error. `break_and_continue_name_their_loop`:
145 mod functions; `only_workshop_images_hold_break_and_continue`.

**`return value` inside `foreach`**: an `opBreak` naming the loop, then the
value's records, then the loop's `opRewind`; `opReturnValue` follows, on the
`foreach` line, and the loop's jump out lands on it. Three shipped `opBreak`s
are this (Majestic Majesty's `defend_building_packet`, Phantom's
`Phantom_Priestess_Assigned_To` in both copies). The value's records never
run, and a loop that runs out returns too: code after the loop is dead. A
literal is made at load, so Phantom's function always returns `True`; a
computed value's slot is empty and the return crashes (**Inferred** from the
VM's model, for Majestic Majesty). `gpl::emit` `Body::returns`;
`a_return_inside_a_foreach_compiles_as_shipped`,
`a_return_inside_a_foreach_returns_its_unrun_value`.

Declined: `do while` (unused anywhere); `break`/`continue` outside `foreach`
(compiler rejects it too); a `return` inside `foreach` beyond the shipped
shape: valueless, nested, in a `while`, or two in one loop.

### A whole section, and a whole `.bcd`

`gpl::emit::code_section` builds section 1 from a project's sources. Against the
SDK's seven images (`a_whole_code_section_is_built_from_the_sources`):

| | |
|---|---|
| Entries compared | 3,609: 1,983 functions, 1,582 constants, 44 prototypes |
| Entries differing | 1 (`Snake_Pit_Spawn`) |
| Sections byte-identical | 6 of 7 |

Entry paths are spelled as the compiler received them — relative in
`OriginalQuests`, `C:\UGServer\Cyberlore\…` in the SDK example,
`\projects\ap\gpl\…` in the shipped game — so the caller supplies them.

`gpl::emit::compile_project` writes all three sections: a GPL compiler. Five of
seven SDK images come out byte for byte, including the base
`bytecode.bcd`. `a_whole_bcd_is_compiled_from_its_project` asserts the two
differences:

| Image | Why |
|---|---|
| `MX_Data.bcd` | `Snake_Pit_Spawn`'s mark byte |
| `WrathOfKrolm.bcd` | Records `C:\UGServer\Cyberlore\…\WrathOfKrolm.gpl`; the project lists `WrathOfKrolm.gpl`: 66 bytes × 20 definitions |

### Section 1 seen as bytes

Byte-level facts, in record terms:

- A constant is three `opNop` records: the source file, the name with its
  `#`, the value in the first value slot (`bcd::CONSTANT_RECORD`). Each
  carries the source line. `section_one_records_a_constant_s_file_line_and_value`:
  name, file, line and value on 1,511 constants (base 649, expansion 862).
- A scan sees a `cstr` then 41 bytes: the previous record's last four bytes
  and the next record's three value slots and `head`. Its byte 13 is the
  mark's tag, its byte 24 the first value slot's type.
- **Corpus**, no test: over the base game's functions and prototypes, entry
  size tracks expression count closely, statement count less, section 2
  record count not at all.
- The `u16` after the source-file string is the declaration line on every
  entry but the prototypes, which record a later line.
- `GplKickoff` (empty, two locals): `lowlevel.gpl` at line 211,
  `__Return_value__`, `AIRootAgent` at 213, `agentIdx` at 214.
- A call is `[u8 n][u16 slot × n][0x26]`, result first. `Acid_Bolt_Hit`
  (`acid_bolt_hit_walks_into_the_calls_its_source_makes`):

  ```
  12 <- ( 10 11 )        GetSpellAttribute("acid_bolt", "effector_duration")
  13 <- ( 8 9 12 )       createeffector(target, "acid_bolt_effector", <12>)
  17 <- ( 14 15 16 )     spell_attack(thisagent, target, 25)
  ```

- Scanning without the walk's framing misreads zero runs as `opAssignEqual`
  records: many phantom assignments in one file.

## Section 3 — data

`gpl::bcd::DataSection`; framing as elsewhere, contents GPL:

```
[u32 1] [u32 nprotos]
  ( cstr instance_name
    [u32 len] [u32 1] cstr shape_name [u32 npairs]
      ( cstr key  cstr value )*
    [u32 0] [i32 -1]
  )*
```

- Every value is text regardless of type: `9` is `"9"`.
- Both flag words are 1 everywhere.
- Strings are NUL-terminated ASCII, never empty.
- At least 3,101 prototypes, 43,977 values; all re-encode byte-identically
  (`every_data_section_round_trips`).

## The decoded form and the load

**Binary**, `MajestyHD.exe`.

### One instruction per record

At link time each record becomes an instruction holding its three value slots
(opening, first, second). By slot type:

| Type | At load |
|---|---|
| 10 | a pointer into an instruction: word the record (-1 this one), flag the slot — 0 first, 1 second, 2 result |
| 9 | queued; the `#Name` is resolved after registration |
| 11, 12 | nothing |
| 0–8 | a fresh value of the type, from word, extra or text |

- A reference, flag 2, names a record's result. A place's opening slot
  (`-1`, 10, flag 0) makes the result its first operand's value. So `flag`
  on a type-10 slot picks a slot; on a literal it is ignored.
- A declaration is the last row: a fresh value of the declared type.
- A call takes its opcode from a name-to-opcode map, default 38, whose setter
  nothing calls. That is why 40–42 never run.

`gpl::exec` is that decode without the layout: `Op` (43), `Dest`, `Operand`,
`Head`, `Call`, a `Signature` from the prologue, strings interned. Dropped: a
literal's flag, tag 11 against 12, the call's own result slot, section 2.
Any shape the corpus lacks is refused. All 75 images decode: 19,466
definitions, 1,262,666 instructions
(`every_image_decodes_one_instruction_per_record`).

### Load order and duplicate names

```text
read MajestyDatasetDefinitions.xml, the expansion's
compose        base list, then own images not yet listed
apply          builtins first; then the GPL list, tail first
read each .bcd per section: a name already read is dropped
register       functions and constants into the function map; a name
               present is skipped
fix up         function values must exist; #Name slots
```

- First loaded wins, loaded tail first: the last listed image owns a name.
  HD lists `Bytecode.bcd`, `MX_Compatibility.bcd`, then the expansion's four;
  a mod (`<Dataset base="Any">`) appends. With the expansion, 1,476 of
  `Bytecode.bcd`'s names and 63 of `MX_Compatibility.bcd`'s give way; the
  expansion images share none (`the_last_listed_image_owns_a_shared_name`).
- A builtin keeps its name: builtins are registered first.
- Constants share the function map, registered with native function 0. The
  return type registered for any script name is its scope entry's; a
  constant's `unknowntype` becomes 10, undefined, which has no value class
  (`vm::CONSTANT_RETURNS`). So a call through a function value naming a
  constant fails making its return slot, and on a record an earlier callee
  typed, calls address 0
  (`calling_a_constant_through_a_function_value_crashes`). No shipped
  function value names one. The map holds 1,703 names for the base game,
  2,190 with the expansion, re-derived from section 2
  (`the_function_map_has_the_size_the_engine_registers`).
- A `#Name` no script defines falls to the attribute registry by name
  (`ATTRIB_…`, and ten others, `host::NAMED`: `TeleportInsideDestination`
  is bank `GPL` index 1), then descriptors, then a logged miss. A missing
  function value is an error.
- HD has a loader for `datasets.xml` that nothing calls.

`gpl::load` builds the program: every call in HD's base game, expansion and
all 57 Workshop images resolves
(`every_workshop_image_loads_over_the_expansion`). One `#Name`, in one mod, reaches no script
constant or attribute: a text id. Images compiled from source decode to the
shipped program, debug information aside; the one difference is
`Snake_Pit_Spawn`'s fresh integer. Script calls whose argument count differs
from the callee's prologue: three in HD, 14 more in mods
(`every_call_fits_its_callee`).

How the engine runs a function: [gpl/runtime.md](../gpl/runtime.md). The VM: [gpl/vm.md](../gpl/vm.md).

## The files

| File | Size | Holds (**Inferred**) |
|---|---|---|
| `data/bytecode.bcd` | 3.8 MB | Base game program |
| `datamx/bytecode.bcd` | 3.8 MB | Byte-identical to the above |
| `datamx/mx_task.bcd` | 2.6 MB | Task / behaviour scripts |
| `datamx/mx_data.bcd` | 1.9 MB | Script data tables |
| `datamx/mx_build.bcd` | 1.3 MB | Build tree |
| `datamx/mx_decision.bcd` | 407 KB | AI decision logic |
| `data/mdl1.bcd` | 102 KB | `mdl1` dataset |
| `datamx/xqd1_bytecode.bcd` | 78 KB | Extra-quest dataset |
| `datamx/mx_compatibility.bcd` | 18 KB | Version compatibility shims |

**Inferred**: hero AI is tasks chosen by a decision layer (from the expansion's
task/decision/data/build split).
