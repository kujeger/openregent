OpenRegent is a mainly Claude-implemented engine for the classic game Majesty,
run on Majesty Gold HD's data. The reverse engineering is done: openregent
plays every quest, as far as known, as the original does. See README.md.

What guards behaviour now ([docs/openregent/testing.md](docs/openregent/testing.md)):
the quest recordings (`install` tier) and the test game (`regent_testgame`,
no game data). A rule change that breaks a recording re-records it only when
the change is meant; a new rule gets a test-game test.

A fact about the original's files is held by a parser and a test.

The rust code also has a cli -- `regent_cli` -- that is used for reading and printing parsed data. Use this to read known data structures. Feel free to expand it, if it does not print everything needed.

Take openregent screenshots with `tools/shot.sh`: a hidden display (headless
cage), scratch data dirs, no window on the user's desktop. Don't run
`--screenshot` directly.

Every guess and inferral must be explicitly noted as being such, and should where possible instead be verified with a parser or tool. Simple grepping often produces wrong results.

If a thing could be verified by game knowledge, ask the user for input.

Before going digging in binaries, or data files, and such: check if it has already been resolved in the rust code.

If `local/AGENTS.md` exists, read it: private notes on the binary.

**Tracked files carry no binary internals.** No addresses, function labels
(`read.*`), vtable slot numbers, memory offsets of engine structs, or
instruction narration: those go in `local/`. Say what the engine does, and
cite file-format fields (`PRIM + 0x04`, a save word) where they hold it.
`regent_core/tests/addresses.rs` fails on the patterns it knows.

**Before recording something as unknown, read [docs/settled.md](docs/settled.md).** It
is one line per closed question and is meant to be read whole.
Sometimes the answer to a question is already in the repo -- just not immediately
connected to the question.
You cannot grep for a word you do not know exists; you can read a short list.

Ensure that the index is kept at or below 150 lines, each at or below 250
characters -- it must be short and to the point. A line holds one fact and its
pointers; detail goes in the linked document. Do not make a line longer to fit
a new fact: put the fact in its document, and give the index a line only for a
closed question about the original. openregent's own additions belong in
[docs/openregent/enhancements.md](docs/openregent/enhancements.md), not the index.

Documents live in [docs/](docs/README.md). Facts about the original go in
`original/`, `formats/` or `gpl/`; openregent's own design goes in
`openregent/`. Keep the two apart.

**A change to game behaviour is an enhancement: make it a toggle.** Put it
behind a setting (e.g. `[enhancements]`), keep the original behaviour
available, and add a row to
[docs/openregent/enhancements.md](docs/openregent/enhancements.md).
Infrastructure, such as running without an install, needs no toggle.

**A test that reads game data needs a tier** (`install`; see
[crates/README.md](crates/README.md)), so `cargo test` still passes without the game.

**A settled constant the code uses belongs in code, not only in prose.**
Put the number in the library with a doc comment and a test that checks it
against the file it came from — `regent_core::data::settings` is the pattern. A
constant read from an executable gets no test against it: tests never read
`.exe` files. A finding nothing uses goes in its document, not in dead code.

Do not treat the markdown documents or commit messages as a diary.
They are technical documents: what the original does, and how openregent
does it. Not how a fact was found.
