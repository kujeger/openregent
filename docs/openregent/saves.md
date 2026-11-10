# Saves

openregent saves its own games, not `.gmp`: `regent_core::save` is a
versioned model of the whole game apart from the simulation's types
(`Game::snapshot`, `World::restore`, `Game::resumed`; each type converts
by `save::Saved` beside its fields). A save is one file in the
profile's `saves` directory, a LAN game's in `mp-saves` (`save::dir`;
neither dialog reads the other's), a tar as one zstd stream, so `tar
--zstd -xf` unpacks it:

```text
<name>.tar.zst
  meta.ron      version, name, the game (quest key and title, or the
                custom game's title and picks), frame, clock, the time
                (seconds since 1970), the profile, and what it needs: the archives, a
                hash of the GPL images (FNV-1a over their bytes) and the
                mods (GUID and name, in order); a LAN game's seats
                (nicks), delay and game hash
  minimap.png   the minimap as drawn, unexplored black
  snapshot.ron  the game (regent_core::save::Snapshot) and the interface's
                part: selection, message read, the view's centre and
                zoom, the reward panel's two settings
  play.input    the session from the game's start (`record`): a replay
```

A list reads only `meta.ron` and `minimap.png`, the stream's first few
KB. The container has its own version (`save::VERSION`), the snapshot
its own (`regent_core::save::VERSION`). A file of another version, or whose archives or images differ from
the install's, is refused and named; nothing migrates. A save needing a
mod no folder holds is refused, the mod named, as HD's "Mods Missing"
box; one made with other mods than its setup now loads is refused too
([mods.md](../formats/mods.md#the-mod-set-recorded)). The name offered
is the original's, "(The Siege) 0 Days", at most 100 characters; the
file's name is it made safe as a profile's.

The dialogs are the original's `AP49` and `AP27` in their art
(`save::dialog`, `menu::save_page`; [panels.md](../formats/panels.md#the-save-and-load-dialogs)),
over the main menu, the lobby or the game: the rows newest first, each
with its save's time ("2026-10-08 14:30", local) and days flush right,
the lit one's description, only saves of a game like the one played (alone,
a game of one's; in a LAN game, a LAN game's of as many players), SAVE
GAME over a save of the same name and DELETE GAME asked in `AP11`, why a choice did nothing told in `AP11`.
`AP27`'s left side box shows the lit save's minimap. A save made with
mods ends its description with "Mods: A, B" (placeholder, no game
text names mods), in load order, one no folder holds now in the build
menu's unavailable red (`saves::mods_line`); cut to one line, "+N"
for the rest, red if one of those is missing. `--dialog save`
and `--dialog load` shoot them over a game, `--dialog overwrite` SAVE
GAME's question on a name taken.

Autosave (`[game] autosave`: every `5` game days, the default, `1`,
`10`, or `off`; `autosave`) saves when the original's would
(`regent_core::view::menu::autosave`): as frame 1, and one frame past each
interval, has run, saying `GMTX` 335 "Autosaving game." in the chat;
never once the local player's game has ended. The original offers 5, 10
and 20 days and writes one file, "autosave", over the last; here the
newest 10 are kept, each `autosave.<n>.tar.zst` in the saves folder of
the game's kind, named "autosave Day 005" so the rows sort together and
by day. A `.` never appears in a named save's file (`profile::safe`), so
rotation touches no other save. They list, load and delete as any save.
In a LAN
game each peer autosaves alone at its own setting; peers of one setting
save one game at one frame (the same game hash): no ask crosses the
wire. The original's guests take the host's interval.

Every save, SAVE GAME's, a LAN save's and an autosave's, is written one
way (`save::thread`): `ui::keep` takes the game on the sim thread
(`save::FromSim`, at the dialog's choice or the step that took it) and
queues it with this machine's view, minimap and profile; one thread per
game makes and writes the saves in order, one at a time (`.part`,
synced, renamed). A LAN copy's free name and the next autosave number
are found as it writes, so two queued saves never pick one file, and
rotation never removes one being written. `ui::keep` reads what it
finished each frame: SAVE GAME's "Game Saved." (`GMTX` 311) in the
chat, as the original posts it (**Inferred**: in the game's colour); a
write that failed opens the save dialog again on its name, saying why
(under the settings editor, a chat line instead); a LAN save says so
in the chat. A game that makes no save (stopped, not recorded) keeps
the dialog open at once. The game's end drops the thread's
end (`SaveThread`, per game): the saves queued are written and the
thread joined before the menus come back or the program exits, so a
save loaded next is whole; that wait is theirs, nothing else bounds it.

The save thread does the slow part. Its cost on a late 512-cell game:
`save::tests::save_cost_of_a_late_512_cell_game` (ignored; prints it).

A load reads `play.input`'s setup, makes the world that setup
makes (the generator's or a quest's; `begin::Plan::resumed`) and takes the
snapshot over it: no setup runs. Descriptors, the program and drawing
come from the dataset; the drawing is the restored world's. The recording goes on from the
save's session, so the new `play.input` replays from the very start.
The selection and view come back (the original's load selects nothing:
a departure the save model allows either way); the reward settings are
this machine's (`Sim::rewards`), saved beside the snapshot; a camp
move's paid price is the world's, per player (`World::camp_held`).

A LAN game's save is every peer's (`save::Stepped`, [LAN play](multiplayer.md#lan-play)):
its meta names the seats by nick, the delay, and the game's hash
(`save::game_hash`: FNV-1a over `snapshot.game` as compact RON, then
`play.input`), the same on each peer's copy; the interface's part, the
minimap and the date are each machine's. Orders sealed and not yet run
at the save's step (a delay's worth) are not in it: a loaded game goes
on without them.

Not saved: descriptors and tables, settings of the run (checks, the
clock patch), what the front end takes each frame
(sound cues, chat lines, view shakes), diagnostics (unknown and wrong
answer counts, losses), a log replay's state (a snapshot is of a game
with no log), the chat box, the music's place in its track (the track
starts again), the game speed and pause, the particles (no rule reads
them: a loaded game's systems start afresh, `Sim` seeding their RNG
with the session seed).

Saved at frame N and resumed on a fresh world, a game plays M frames on
as the game never saved, snapshot for snapshot, every RNG with it, and a
snapshot reads back from RON unchanged (`regent_core/tests/resume.rs`): the
start trace's custom game at frames 1, 1,242 and 3,600 (M 1,200); the
largest map, 512 cells, at 1 and 1,200 (M 300); The Barren Waste at 600,
1,214 and 2,458 (M 1,200). A bot-played custom game saved to a file
at 2,400: its `play.input` replayed from the start reaches the
snapshot; loaded as the menus load it, it plays 1,200 frames on as the
game never saved, and the loaded game's recording replays to its end
(`save::tests`). A quest the menus started, saved, played on and loaded
from its Options goes on from the save's frame, its selection and
reward settings back
(`a_game_loaded_from_its_options_goes_on_from_the_save`). A 128-cell game's snapshot is about 0.8 to 1 MB of RON,
130 to 150 KB compressed; a 512-cell one's 7 MB, 1 MB compressed.

