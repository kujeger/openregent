# The GPL language

Game Play Language; per its documentation, it implements most of Majesty's AI.

Sources: the SDK's `GPL Reference.pdf`, the Notepad++ syntax template, the
306 shipped `.gpl` files in `SDK/OriginalQuests/`
(`every_file_lexes_without_error`), and `Gplbcc.exe` run on made-up
projects ([below](#beyond-the-shipped-corpus)).
Provenance: [original/editions.md](../original/editions.md).

Counts are the shipped corpus.

Counts come from the lexer's tokens or the parser's tree, never grep (grep
counts comment prose). Every count here is pinned by the test named beside it,
mostly in `crates/regent_core/tests/corpus.rs`.

## The reference

55 pages: pp. 1–5 the language, the rest builtins. Three lexical rules —
case-insensitive, `;` terminates, `//` comments — no grammar, no operators.

Its 17-word keyword list omits `is`, `in` and `do` (384 / 458 / 569 uses,
`keyword_counts_match_the_token_stream`),
includes `and`, which is not a keyword ([below](#words-that-look-like-keywords)),
and misprints `prototype` as `prototypep`. Its type list includes `agentref`,
which is not a type ([Types](#types)). The Notepad++ template copies both lists,
fixing the typo and dropping `agentref`, so it is not independent evidence. The
corpus is the authority.

## Lexical facts

**Corpus** (`every_file_lexes_without_error`):

| Element | Rule |
|---|---|
| Case | Insensitive: `TRUE` / `True` / `true`, `ThisAGent` / `THisAgent` |
| Comment | `//` to end of line. No block comment in the corpus |
| String | `"..."`, no escapes, never spans lines |
| Identifier | `[A-Za-z_][A-Za-z0-9_]*`; none begins with `_` |
| Integer | Signed 32-bit. No hex, exponent, suffix or negative literal; `-` is always an operator |

- Many case-folded names (identifiers, `$call` and `#constant` targets) have
  more than one spelling.
- No backslash anywhere in the shipped corpus. Some strings are empty; some
  have significant leading or trailing spaces.
- Longest identifier: `returning_guardian_eval_enemies_simple`; longest name:
  `#Guardian_Atck_Hostiles_Pursuit_Range_Mod`.
- Every integer fits `i32`.

Floats: 62 (`token_counts_match_the_survey`), in three forms. The last two
break a naive `\d+\.\d+`:

| Form | Example |
|---|---|
| Ordinary | `100.0` |
| Trailing dot | `100.` |
| Leading dot | `.75` |

Every dot belongs to a float: no member access, no range syntax. The compiler
has a `DOT` token no rule consumes.

`float` is fixed point. The reference gives a 2²³ integer range and 1/255
precision, and says why: GPL runs in lockstep on every multiplayer client, so
floating-point rounding would desync them. The `.bcd` literal encoding is ×1024
([formats/bytecode.md](../formats/bytecode.md)), which does not match the
reference's 1/255. At run time too, a float holds 10 fractional bits
(`base::gfix::Gfix`, **Measured**). Integer
arithmetic passes through it, so integers wrap past 2²¹, not the reference's
2²³. A reimplementation must match it exactly.

### Encoding

| Corpus | Bytes | Line endings |
|---|---|---|
| Shipped | Pure ASCII, no BOM | CRLF |

Most shipped files lack a final newline, so a `//` comment can run to EOF.
Some use an uppercase `.GPL` extension: glob case-insensitively. Only the code
is ASCII, so files are decoded lossily. **Corpus**: every file lexes and
parses so read (`every_file_parses_without_error`).

## Sigils

| Sigil | Meaning | Shipped sites (`token_counts_match_the_survey`) |
|---|---|---|
| `$Name` | Call or reference a function, builtin or script-defined | 19,819 |
| `#Name` | Named integer constant, resolved at load time | 8,914 |
| `agent's "attr"` | Attribute accessor | 7,001 |

`#Name` resolves when the bytecode loads, so external data can define it. A
name beginning `ATTRIB_` loses the prefix and becomes that engine attribute's
id, so `.bcd` files carry these symbolically.

Every shipped and mod accessor is a lowercase `'s` glued to the
preceding token; no other apostrophe appears in code. `'S` is unknown: no
example. The lexer takes lowercase only.

`OriginalQuests` always writes `identifier 's "string literal"` (house style).
`Gplbcc.exe` takes the general form (`attribute_names_are_any_expression`,
`gplbcc_diff`):

- The name is any expression, evaluated at run time: a variable
  (`mod's key`) or a parenthesised expression (`mod's ("ZqTier" + tier)`).
- Accessors chain: `who's "ZqHome"'s "ZqGold"`. So `'s` is its own token,
  not an identifier suffix.

## Keywords

Shipped token counts. `EXPECTED_KEYWORDS` pins all 26, summing to 37,891
(`keyword_counts_match_the_token_stream`).

```
begin 6215   end 6215   if 5341   agent 3145   function 2298   declare 2011
false 1954   else 1727   expression 1582   integer 1398   return 1181
list 1006   true 813   do 569   foreach 458   in 458   boolean 403
string 394   is 384   coordinate 131   while 111   float 53   prototype 44

break 0   continue 0   define 0
```

`Gplbcc.exe` compiles `break` and `continue` inside a `foreach`
(`break_and_continue_parse_as_statements`, `gplbcc_diff`). `define` is used
nowhere.

Cross-checks:

- `begin` = `end` = 6,215.
- `foreach` = `in` = 458: `in` has no other use.
- `do` = 569 = `foreach` + `while`: both loops need `do`; no `do ... while`
  exists.
- `expression` = 1,582 = the constant definitions.

### Types

Eight, all used: `agent`, `boolean`, `coordinate`, `float`, `function`,
`integer`, `list`, `string`. `function` is keyword and type: of 2,298 uses,
1,967 open a definition (`top_level_item_counts_match`) and the rest declare a
variable or parameter. **Binary**: the compiler
has seven `*TYPE` tokens and reuses `FUNCTION` for the eighth.

`agentref` is not a type: the compiler has no such token and no source uses it.
The runtime class `GplAgentRef` exists (an agent reference is a value), but the
type cannot be written.

### Words that look like keywords

**Binary**: `and`, `or`, `mod` and `not` are the compiler's token names for `&&`, `||`, `%`
and `!` (compare `NOTEQUAL` for `!=`, `INSERTER` for `<<`). The lexer takes all
four as identifiers.

`Gplbcc.exe` compiles `agent mod;` and its use as a variable
(`mod_is_a_variable_name`, `gplbcc_diff`). **Inferred**:
`and`, `or` and `not` are also legal names; no source uses them.

`and`, `or`, `not` and `then` never occur in code; all apparent uses are
comments.

No logical-not: `!` lexes as `NOT`, no rule matches it, and every `!` in either
corpus is part of `!=`. No `elseif`: an `else if` nests an `if`.

## Operators

Complete. Counts pinned by `token_counts_match_the_survey`:

| Op | n | Op | n |
|---|---|---|---|
| `(` `)` | 26,982 | `<<` | 317 |
| `;` | 22,308 | `&&` | 220 |
| `,` | 21,746 | `\|\|` | 131 |
| `=` | 7,469 | `%` | 4 |
| `==` | 2,994 | `/=` | 2 |

Also used: `-`, `>`, `+`, `<`, `!=`, `*`, `+=`, `-=`, `/`, `>=`, `<=`, `++`,
`--`, `*=`.

`<<` appends to a list. No `>>`, `<>`, bitwise or ternary. `++` and `--` are
postfix. Absent: `[ ] { } \ ~ ^` and backtick. `: ? @` occur only in comments
and strings. What each does, per type:
[formats/bytecode.md](../formats/bytecode.md#what-an-operator-does).

## Statement shapes

```
function Name(type param, type param) is returntype
declare
    type localvar;
begin
    ...
end

if (condition) statement else statement
while (condition) do statement
foreach item in list do statement
```

- `is returntype` is optional: 384 of 1,967 definitions have it
  (`keyword_counts_match_the_token_stream`, `top_level_item_counts_match`).
- `if` and `while` always take parentheses (all 5,341 and 111,
  `statement_kind_counts_match`); both loops take `do`.
- A body is one statement. Many `if`s take a bare statement: a nested `if`, a
  call or a `return`.
- `expression #NAME value` has no `=` and no `;`. All 1,582
  (`top_level_item_counts_match`) are bare literals, nearly all integers.
- Under half of non-blank code lines end in `;`, so error recovery cannot
  ignore newlines entirely.

## The grammar

**Binary**: `Gplbcc.exe` is a PCCTS 1.33 / SORCERER C++ program. Its `_match()` and `LA(1)`
sequences give 35 rules and a 140-entry token enum. Where they disagree, the
compiler shows capability, the corpus usage.

```ebnf
script            : ( function_definition | prototype_definition
                    | expression_definition )* EOF ;

function_definition   : 'function' IDENT parameters [ 'is' type_specifier ]
                        [ declaration_block ] function_body ;
prototype_definition  : 'prototype' IDENT parameters
                        [ declaration_block ] function_body ;
expression_definition : 'expression' #Name expression ;        (* no ';' *)

parameters        : '(' [ declaration ( ',' declaration )* ] ')' ;
declaration       : type_specifier IDENT ;
compound_declaration : type_specifier IDENT ( ',' IDENT )* ;
declaration_block : 'declare' ( compound_declaration ';' )* ;
function_body     : compound_statement ;

statement         : expression ';' | compound_statement
                  | selection_statement | iteration_statement
                  | jump_statement | return_statement ;
compound_statement: 'begin' statement* 'end' ;
selection_statement : 'if' '(' expression ')' statement [ 'else' statement ] ;
iteration_statement : 'while' '(' expression ')' 'do' statement
                    | 'do' statement 'while' '(' expression ')' ';'
                    | 'foreach' expression 'in' expression 'do' statement ;
jump_statement    : 'continue' ';' | 'break' ';' ;
return_statement  : 'return' [ expression ] ';' ;

expression        : assignment_expression ;
assignment_expression : logical_or
       [ ('='|'+='|'-='|'*='|'/='|'%='|'<<') assignment_expression ] ;
logical_or        : logical_and    ( '||' logical_and )* ;
logical_and       : equality       ( '&&' equality )* ;
equality          : relational     ( ('=='|'!=')  relational )* ;
relational        : additive       ( '<'|'>'|'<='|'>=' additive )* ;
additive          : multiplicative ( ('+'|'-')    multiplicative )* ;
multiplicative    : prefix         ( ('*'|'/'|'%') prefix )* ;
prefix            : [ '-' | '+' ] postfix ;
postfix           : accessor [ '(' [ argument_list ] ')' ] [ '++' | '--' ] ;
accessor          : primary ( "'s" primary )* ;
primary           : STRING | INT | FLOAT | 'true' | 'false'
                  | '(' expression ')' | IDENT | $Call | #Expr ;
argument_list     : expression ( ',' expression )* ;
```

Easy to get wrong:

- `<<` is in the assignment tier, right-associative: a list append, never
  nested in an expression.
- Assignment is an expression and `statement : expression ';'`, so `=` in an
  `if` condition and a valueless statement are legal; both occur.
- `accessor` takes a full `primary` on the right, so the attribute name is an
  expression.
- `postfix` carries the call, giving all three callee forms: `$name(...)`,
  `(expr)(...)`, `id's "attr"(...)`.
- The corpus overrides the compiler's grammar twice: 236 bare `return;`
  statements (`statement_kind_counts_match`), and definitions with an empty
  `declare` before `begin`.
- `declaration_block` is optional in the grammar, but `Gplbcc.exe` refuses a
  definition without one: `syntax error at "begin" missing DECLARE`.

The EBNF shows 29 of 35 rules (28 written plus the implicit `type_specifier`).
The other six add no syntax: `iterator` and `range_specifier` alias
`expression`, `typed_expression` sits between `expression` and
`assignment_expression`, and `declaration_identifier`, `boolean_t` and
`constant` name single terminals.

**Binary**: `ELLIPSIS`, `DOT`, `QUESTIONMARK` and the curly and square brackets are lexed,
matched by no rule, and absent from every source. **Inferred**: a numeric-range
`foreach` was planned and dropped.

### Precedence

Undocumented. **Binary**: from the rule names in `Gplbcc.exe`, loosest to tightest:

```
assignment → logical_or → logical_and → equality → relational
           → additive → multiplicative → prefix → postfix → accessor → primary
```

So `'s` binds tighter than a call. C-like, without bitwise or ternary tiers.
The corpus parses under it (`every_file_parses_without_error`): consistency,
not proof.

## Agents and units are separate

An `agent` is the GPL-side structure; a `unit` the engine object it binds to
(image, animation, movement). Engine attributes are integers only; GPL
attributes hold any GPL type. Matches the `GenUnit` / `GSUnit` /
`UnitDesc_Engine` split in the class map, and the `AgentID` a save keeps apart
from an object's handle ([formats/saves.md](../formats/saves.md)).

## Beyond the shipped corpus

`Gplbcc.exe`, run on made-up projects (`tests/fixtures/gpl/`,
`gplbcc_diff`), compiles: `break` and `continue` in a `foreach`, `mod` as a
variable name, the general accessor form, `return` inside a `foreach` (bare,
valued, nested), and a negative constant. It refuses: a definition without
`declare`, an undeclared name (a bare `foo(1)` too), `break` in a `while`,
and `do ... while`.

First seen in Workshop mods: `break` and `continue` in CaravanGuards
(`3554989139`) and Majestic Majesty (`678145904`); `return` inside a
`foreach` in Majestic Majesty and Custom Guild: Phantoms Haunt
(`3769947406`); a negative constant in Monster Kingdom (`3743606613`).

Not shown: new operators, `define`, `'S`, escape sequences, brackets or
braces in code, characters outside the base set.

## Status

Lexer, parser and `.dat` reader: done, in `crates/regent_core/src/gpl`
([compiler.md](compiler.md)).

Loading: done. `gpl::exec` decodes records into instructions, `gpl::load`
builds the program as HD does — load order, first wins, one function map —
and every call in the base game and expansion images resolves
([formats/bytecode.md](../formats/bytecode.md#the-decoded-form-and-the-load)).

Interpreter: `gpl::vm` runs instructions as the engine does — one activation
per function, the engine's faults and messages, both check modes, the trace —
and runs the 18 builtins touching no world itself; the rest go to a host
([vm.md](vm.md#the-vm)).

Every world builtin answers from the world (`every_builtin_has_an_answer`,
[settled.md](../settled.md#gpl)). The shipped game calls 141 builtins, 16 of
them native (`the_builtin_inventory_is_complete`).
The SDK documents 166 (162 in the reference index, 4 only in the Notepad++
template). HD registers 165 of them, each with its native address, return
type and signature — required, optional and rest arguments with their types,
all 166 read from the code: `gpl::builtins`,
[formats/bytecode.md](../formats/bytecode.md#builtins-one-map-of-names). Where
the reference's parameter lists differ, the code wins. openregent adds its own
for mods after them, `gpl::builtins::OWN`
([formats/mods.md](../formats/mods.md#openregents-builtins)).
