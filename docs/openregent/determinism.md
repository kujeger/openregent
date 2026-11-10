# Determinism

Play is a function of setup, session word and input, on every machine
(lockstep and cross-machine replays need it):

- **Floats.** Play uses IEEE `+ - * /`, `sqrt` and casts only: correctly
  rounded, so alike on any IEEE target. No libm call: `sin(IsoCameraAngle)`
  is the literal `data::settings::ISO_CAMERA_SIN`, tested against
  `sin`. The engine's own trig is a table (`sim::trig`). Float text
  (`gpl::vm::text`) and parsing are `core`'s, not the C library's.
- **Maps.** `sim`, `gpl` and `save` keep `base::digest::Map`/`Set`: a fixed
  hasher, so iteration order never varies by run. `MAJESTY_SHUFFLE_MAPS=1`
  (or `digest::shuffle()` on a test's thread, as the run-twice tests do)
  keys each map at random again, to show any order dependence.
- **Digest.** `Game::digest` (frames, GPL RNG, `World::digest`: clock, world
  RNG, pieces summed, gold) is cheap; `Game::deep_digest` folds the whole
  snapshot but the local parts (reward panel, pending camp move, inputs to
  come, error count) and costs several frames, so LAN play takes the
  snapshot on the stepping thread and folds it on a worker
  (`game::deep_fold`).
- **Local player.** The world does not hold one: the front end does
  (openregent's `Sim::local`) and passes it to the queries that need a
  viewer. What the engine
  did for its machine's human (start selection, conversion tally, advisor
  threads, Abode bar, message chime, `UnitIsLocalPlayer`) is done for
  every human seat (`World::humans`, saved; a new game's setup gives the
  count, a `.gmp` one); a seat's cues carry
  `hearer`, and the front end plays its own. openregent's `--players N` (recorded) seats a new
  game; `--local N` (not recorded) is the machine's seat. [LAN play](multiplayer.md)
  compares `Game::digest` every frame and
  `Game::deep_digest` every 100 between peers. `play.input` carries
  `Game::digest` every 100 frames; `--replay` stops at the first that
  differs ([game.md](game.md)).
- **Time.** Wall time decides when steps run, never what they do.
  openregent's one loop (`drive`) steps a frame once every seat's orders
  for it are in. An order is taken at a hand-over, before any further
  step, and is recorded with the frame it comes before. A recording
  replays alike in sync mode, pipelined or headless
  (`a_pipelined_game_plays_as_a_sync_one`).
- **Kept lists.** Each agent's thread list (`host::ThreadLists`) is kept
  until that agent changes (`World::agent_stamp`) or its attributes do.
  `MAJESTY_CHECK_THREADS=1` checks each against one built anew.
- **Kept sight.** `World::sight` looks only at pieces marked since its
  last call (`pieces::Stale`, beside the grid's marks) and those unknown
  then; the scatter it samples (ponds, geysers) is listed once per
  `Board::relaid`. `MAJESTY_CHECK_SIGHT=1` checks that each skipped
  piece is as it last revealed, and the list against one made anew.
- **Kept place search.** `World::occupied` reads the collision grid,
  corrected for pieces marked since `World::regrid`, not every piece
  and scatter cell anew. `MAJESTY_CHECK_OCCUPIED=1` checks it against
  the set built whole.
- **Kept range.** `World::rereach` makes anew only the pad rects of
  pieces marked since its last call (`pieces::Stale`); `ListObjects`
  passes over a piece kept out of range. `MAJESTY_CHECK_REACH=1` checks
  the kept rects against ones made anew.
- **Music.** The callback (`Game::music_stopped`) fires on a track's real
  end, so it runs aside and returns the machine's own track, which the
  front end keeps (openregent's `Sim::music`) until the next `PlayMusic`
  (`Output::music_played`); the world keeps the scripts' track
  (`World::music`). Nothing records it.

Overflow panics in release as in dev (`Cargo.toml`, test
`overflow_panics`); peers must still share a profile (`net::wire::Profile`).

