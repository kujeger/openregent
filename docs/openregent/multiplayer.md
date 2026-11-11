# LAN play

Lockstep over UDP (`openregent/src/net`): every peer runs the whole game;
only orders cross the wire. The original's protocol is a non-goal.
A new freestyle game, 2 to 4 players, from the command line or the
menus' Multiplayer page (`menu::lan`). No mods, as in HD (**Binary**: its
network start composes no mod set): `--mods` conflicts with `--server` and
`--client`, a guest refuses a setup naming mods, and the data hash is the
stock dataset's ([mods.md](../formats/mods.md#the-mod-set-recorded)).

Internet play (`net::iroh`) carries the same packets between loopback
proxies over iroh ([Internet play](#internet-play)); this section covers
the protocol they share.

Every game runs one path. `Sim` owns a `net::Peer`. Single player (menus,
`--quest`, headless, bots, replays, screenshots) uses
`Peer::solo`: one seat, a loopback link that never sends, delay 1 (the
engine's `order_frame`, frames + 1).

```text
Sim::input / play_on ──> outbox
sim thread, on its own clock (pipelined): a hand-over's Peer::set_local(pause,
            speed); Sim::pump at least every 2 ms; while time is owed and
            Sim::ready: Sim::step, then commands and Sim::pump again
headless:   the same loop (drive), as fast as Sim::ready allows
Sim::step:  Peer::exchange(seal outbox for step n + delay, digest after n)
            ─> each seat's items for step n + 1, set before the next frame
            ─> Sim::take (recorded) ─> the frame
```

The peer counts steps, not frames: an ended game stands still
(`Game::frame`) while steps go on, so a play-on given then still comes
back. Alone, the own bundle for step n + 1 is sealed as that step begins,
so it never waits (`a_peer_alone_never_waits`); pause and speed are this
machine's.

What the outbox hides until the step is mirrored from it: a build given
shows its placement pending (`Sim::placement`), CONTINUE PLAYING ends the
end at once (`Sim::ended`), and a save holds the orders given as taken
(`Sim::snapshot`, `Sim::session`). Not mirrored: a play-on given and saved
before the next step (the save is still ended). Orders given after a
run's last step are not recorded; they never ran.

Cost: the cheap digest is small against a step, and both digests run
only with peers; alone the bundle's digest is 0. The deep digest's
snapshot is taken on the stepping thread, its fold on a worker
(`net::fold`). Headless single player pays less than the noise between
runs.

```text
host    openregent --server [PORT] [--players N] [new-game options]
guest   openregent --client HOST[:PORT]
either  [--headless --frames N [--bot SEED]]
host    [--net-delay N]      frames an order waits, 2 to 50; unset: picked from the
                             guests' pings ([Delay](#delay))
host    [--net-any-build]    guests may run another commit
```

PORT defaults to 47101. The host is player 0 and plays `--players 2`
unless told; the map number is rolled from the clock, as the menus roll
it, unless `--map-seed` is given. A guest's own setup options are
ignored: the host's make the game, and the host picks its seat
(`the_host_is_told_by_its_seat_not_seat_0`). `--local` is refused with
either. Peers wait for the lobby before any window opens; its lines are
in `play.log` (`RUST_LOG=openregent=info` shows them on the terminal too).

```text
menus   Multiplayer Setup (AP33): the open games LAN discovery hears (a row pressed
          fills HOST:PORT, double clicked joins), HOST[:PORT] to join, the port to host on
          ─CREATE GAME─> session dialog (AP55): name, 2-4 players ─ACCEPT─> lobby
          ─JOIN GAME (an address that reads)─> lobby, a guest's
        lobby (MX19): the session's name, its settings (scrolling), each
          seat's status, nick and build, ready ticks, chat
          host: ADJUST SETTINGS ─> MX17 with its chat (BUILDING EDIT
                MENU, LOAD, SAVE as alone; presets of the lobby's players)
                ─ACCEPT─> the guests see it, their ready lapses; CANCEL
                puts the config back
                LOAD GAME ─> the profile's mp-saves: a LAN save of as many
                seats offered (the settings box names it), its setup
                the lobby's; a seat's line pressed moves it to the next
                saved seat; ADJUST SETTINGS' ACCEPT lets it go
                START GAME, every seat taken, every guest ready (and with
                a save, holding it) ─> game
          guest: Ready flips; the host's start ─> game
```

The pages use the setup page's art (`menu::lan`,
[panels.md](../formats/panels.md#the-multiplayer-pages); `--menu
multiplayer`, `create`, `lobby[:N]`, `lan-setup[:N[:ID]]`, the last two
hosting the config on a port the system picks). The protocol page `AP42`
is not drawn: LAN over UDP is the one service. The session dialog drops
HD's GAME VISIBILITY. ALLOW CHEATS starts clear in a newly hosted game
(`menu::lan::NEW_CHEATS`; the original's starts ticked); a press
toggles it. Enabled, as `INDk` 1003's
disabled slots show no tick.

From the menus the lobby is polled each frame through its client
(`net::Session`, over `net::Hosting`, `net::Joining`; the command line
blocks on those); hashing the build and resolving the host run on a
thread. Delay picked ([Delay](#delay)), the same commit only. Cancel lets the socket go; a
host's cancel tells its guests (`a_cancelled_lobby_tells_its_guest`). The
game loads as a menu freestyle start; its end's Main Menu goes back to
the menus. Address, port, name and players last for the session. Each
peer's nick is its profile's name (none: `p<seat>`), shown with its build
(`v0.1.0 1a2b3c4`, `a_build_shows_its_version_and_short_commit`).

The host's settings offer the lists of the session's players
([panels.md](../formats/panels.md#the-setup-dialog), `Dialog`): Player,
Enemy Forces and Wandering Monsters by count, Last Palace with more than
one; a pick not offered becomes its list's first as the page opens or
the game is hosted (`Custom::fit`). A settings change puts `GMTX` 314
(the host has changed the options) in every peer's chat
(`net::Talk::Changed`, each peer in its own words;
`a_lobby_shows_its_setup_carries_chat_and_starts_only_when_all_are_ready`).
**Inferred**: the original's host may not see its own.

```text
lobby   guest: Hello(version, profile, commit, data hash, nick, ready for a
               setup revision, chat lines held, own lines not taken),
               every 250 ms
        host:  Refused(why) | Lobby(seat, players, delay, title, revision, the
               recording's setup, seats, own lines taken, chat lines: a
               player's or "settings changed");
               a guest silent 2 s loses its seat; the host's start (the
               command line's: every seat taken): Welcome(seat, players,
               delay, the recording's setup, nicks, host's seat, save's
               game hash), again until the guest's first game packet
        save:  Lobby offers it (name, game hash, file hash and size,
               saved nicks, each lobby seat's saved seat); a guest finds
               its own copy by the game hash in its profile's mp-saves, or
               says Hello(want, first chunk lacking) each poll and is sent
               Chunk(file, n, 4 KB) × 32 from it (`net::transfer`); the
               file, whole, must hash as offered and read as the game
               offered, else the guest leaves, saying why; it is written
               to its mp-saves under the save's name or the next free
play    star: guests talk to the host, which relays each seat's streams
        at step n each peer seals its bundle for n + delay: its orders and
          play-ons, its digest after step n, and the oldest deep digest
          folded and not yet sent, with its frame (every 100 steps a
          snapshot, folded on a worker); a deep digest is compared once
          sent, so the peer that finds a desync has sent what lets the
          others find it
        step n + 1 runs once every seat's bundle for it is in; the items
          go to Sim::take in seat order, so each peer's play.input is
          the same file and any one replays the game (--replay, --local)
        a SAVE GAME is an item too, a name: every peer saves the game as
          that step leaves it, to its own profile
        the host's delay change is an item too, the new delay: every peer
          takes it with that frame and seals from the next step that many
          frames ahead ([Delay](#following-the-network-in-play)); a bundle
          names the step its digest is of
        a seat dropped rides the host's packets with how it left (Bye or
          silence); a Bye is a peer's last word
        each packet: acks (each seat's last step and chat line held
          without a gap), and all the receiver lacks: a loss is resent in
          the next packet, at most 25 ms on
resync  each packet carries its sender's epoch (resyncs gone through);
          streams of another epoch are not read, the packet still heard
        host, a desync found: epoch + 1, every peer holds; its game at
          the step it holds and its play.input as a file (`net::Payload`,
          RON, zstd) offered: Offer(desync's frame, file hash, size, the
          delay in force) to
          each guest not yet of the epoch
        guest: Want(file, first chunk lacking) each packet; Chunk × 32
          from it (`net::transfer`); whole and as offered, it goes on
          from it (`Sim::resync`) in the host's epoch
        every peer: the lockstep begun again at the host's step with the
          offered delay, its frames to the delay empty, as at the start; the host goes on
          at once and waits on the guests' bundles as ever
        too soon again: Stop(why) to each guest, also before a Bye
```

Tests: `two_peers_over_a_lossy_network_play_alike`,
`the_host_relays_a_guests_streams_to_the_others`,
`chat_lines_arrive_once_in_order`, `a_file_arrives_whole_in_any_order`,
`a_file_not_as_offered_is_refused`.

- **Build check.** The host refuses a guest whose version or profile
  (`debug_assertions`: the sim's `debug_assert!`s; both check overflow)
  differ (`a_differing_build_is_refused_with_its_reason`). Differing
  data is warned, not refused: logged by the host, shown in the guest's
  game list. The data hash covers the GPL images, the freestyle quest,
  the chart and each archive's name (`/`-separated, lower case) and
  length, not the archives' bytes. A differing git commit (`MAJ_COMMIT`, from `openregent/build.rs`;
  `-dirty` with local edits under `crates/`) is refused unless
  `--net-any-build`; the system may differ.
- **Desync.** A differing digest holds every peer that finds it; each
  logs both digests and writes `desync-player<seat>-frame<n>.ron` beside
  its log: for a deep digest the snapshot it folded, of frame n; for the
  quick one, the snapshot at the frame it found it. Folded snapshots are
  kept until every peer's deep digest of their frame is compared
  (`a_snapshot_none_compares_is_let_go_in_time`). The host finds every
  desync and resyncs: its game is the truth, sent to every guest,
  including those that agreed. Each peer, the host too, goes on from that
  file, its world's saved parts replaced (`World::restore` replaces every
  one, as over a fresh world). Each chat says "Desync at frame n:
  resyncing." and "Resynced." (placeholders: HD has no such text), the
  host's once every guest has. Each peer keeps the file as
  `resync-player<seat>-frame<n>.ron` beside its log, not among the
  profile's saves. Its `play.input` is rewritten as the host's up to the
  resync and goes on, so every peer's is again one file and replays from
  the start (**Inferred** limit: where the host's own game parted from
  its recording, the replay parts there too). Orders given while held are
  taken after; those sealed for frames past the host's step are dropped.
  While a resync holds a peer, or a remote is of another epoch, a remote
  may be silent 60 s and is not waited on. Tests:
  `a_guest_that_parts_is_resynced_from_the_host`,
  `a_host_that_parts_resyncs_the_guest_to_its_own_game`,
  `three_peers_resync_over_a_lossy_network_and_none_is_dropped`,
  `a_deep_digest_folded_aside_that_parts_is_resynced`.
  A desync within 1,200 frames (`net::RESYNC_GAP`, **Inferred** bound)
  of the last resync stops every peer, the chat giving `GMTX` 329 (the
  game cannot continue) and why (`a_desync_soon_after_a_resync_stops_every_peer`);
  so does a held guest whose host leaves.
- **Pause and speed.** Pause is everyone's: any peer paused, by the key
  or the options menu, holds every peer, and only it resumes. The host's
  speed paces all: its `--speed`, else the engine's. `<` `>` and
  `[game] speed` change nothing on any peer, as the original ignores
  `<` `>` in a network game without a word (**Binary**); the host's speed
  keys enhancement (1, 2, 3) still sets it, a guest's does nothing.
- **Round trips.** In play each peer pings each remote it talks to once
  a second (`Ping`, `Pong`, as the lobby's); the host's game packets
  carry each guest's round trip to it, so a guest's to another guest is
  its own to the host plus that one's (`Node::pings`,
  `a_guest_sees_another_guests_ping_through_the_host`). The player list
  shows them, and the delay in force here (`ui::players`, `[ui] player_list` = `details`, the default; `players` shows names alone, `off` none).
- **Waits.** A peer silent 2 s (`node::WAIT_AFTER`, **Inferred** bound)
  is waited on: its bundles stop, so every peer holds. The title scroll
  reads `GMTX` 345 "(Waiting for Other Players)", as HD's does while a
  network game waits (**Binary**; `scroll::WAITING`, **Inferred**
  reading). A box below the top bar names whom (`held`, placeholder
  look). The host's box asks in HD's words, `GMTX` 289 (whether to drop a
  player not heard from), with the time left and a Drop Player press
  (288, HD's dialog, **Binary**): the seat is dropped at once
  (`Peer::drop_player`, `the_hosts_drop_takes_a_waited_on_guest_out_at_once`).
  A guest's box says "Waiting for Ann: dropped in 55 s." (placeholder),
  for its host's silence or a seat the host reports in its game packets.
  Back within the window, play goes on where it held; nothing is told
  (`a_guest_silent_3_s_comes_back_and_play_resumes_in_lockstep`).
- **Drops.** A peer silent 60 s (`node::DROP_AFTER`: HD's
  `TossPlayerAfter` file value, which HD compares with each player's
  silence, **Binary**; **Inferred** that HD drops past it; 60 s too before
  its first game packet, while its world is made) is dropped: the host
  plays the seat's orders up to the last frame it holds and tells the
  others; its kingdom plays on uncontrolled
  (`a_peer_is_dropped_after_hds_toss_player_after`,
  `a_silent_peer_is_dropped_and_the_other_plays_on`). A guest that loses
  the host waits the same 60 s, then plays on alone (HD's guest is asked
  `GMTX` 290/291 "Quit Game?" instead; not modelled). A peer whose game
  ends (QUIT, the menus, the window closed: `Peer` dropped) sends three
  copies of `Bye` and is dropped at once
  (`a_guest_that_quits_is_told_to_the_others_at_once`). Each other peer's
  chat says it once, in the game colour (`net::Left`): `GMTX` 298 "Ann
  quit the game.", 295 "Ann was dropped."; the host's leaving adds "You
  play on alone." (placeholder; HD's 308 aborts the game instead)
  (`a_leaving_is_said_in_the_games_words`). HD formats 298 with a name in
  its lobby handler (**Binary**); no use of 295 is found. **Inferred**:
  both are said as a player leaves.
- **Rejoin.** A dropped player (silent or quit) joins the host again as
  at first (Multiplayer, `--client`): the host's game answers a hello
  from a new address whose nick is a dropped seat's, of its build, with
  that seat's welcome again; any other is refused "the game has begun",
  and one during a resync is asked to `Wait`. The host then resyncs every
  peer as after a desync, the Offer naming the seat (`back`): each goes
  on from the host's game at its step, the seat in play again, every
  recording again the host's. The rejoining guest makes its game from
  the welcome (a loaded game's from its own copy of the save,
  `Joining::copy`), then takes the file. Chat says "Ann rejoined the
  game." (placeholder) once every guest has it; no "Desync" or
  "Resynced" line, and a desync soon after may resync
  (`net::RESYNC_GAP` counts from desyncs alone). Protocol version 1: the
  game packet carries the host's waits, the Offer the seat.
  `a_dropped_guest_rejoins_and_both_play_one_game`,
  `a_dropped_guest_rejoins_from_its_old_address`.
- **Surrender.** In a LAN game the Options menu's RETURN TO MAIN MENU
  row is split, as SAVE GAME and LOAD GAME share theirs: MAIN MENU and
  SURRENDER (`ui::options::split`). SURRENDER asks in `APc8`, its text
  replaced (placeholder); YES gives a `Surrender` item in the next
  bundle, and at that step every peer ends the seat's game lost
  (`World::surrender`, as `DeclareLoss`; `surrender` in the recording).
  The kingdom plays on uncontrolled, as a dropped one. The player
  watches at once: no order taken, the fog lifted on the map and the
  minimap (`Eyes::ALL`), its loss not shown. Once every human seat is
  out (ended, surrendered, or dropped, which counts as a surrender:
  `Sim::over`), each spectator is shown its loss, the end panel's Return
  to Lobby as ever. All surrendered, every player sees it
  (`a_surrender_spectates_until_every_player_is_out`,
  `a_dropped_player_counts_as_surrendered`).
- **Chat.** A line typed in the chat box, not a cheat, goes to every
  peer, shown as `nick: line`; it plays no part. A spectator's lines are
  for spectators: marked on the wire (`net::Audience`), shown to a
  spectator as `nick (spectator): line` (placeholder), to a player not
  at all. A spectator reads every line.
- **Saving.** Any player's SAVE GAME (the dialog as alone) goes in its
  next bundle; every peer takes it at that step, the delay on, and
  writes the game as the step leaves it to its own profile's `mp-saves`
  (`ui::keep`): on the asking peer as named, over a save of that name; on
  the others under the name or the next free (`save::free`). Each chat
  says `nick: saved the game: name`
  (`three_peers_save_one_game_at_one_frame`). A save asked while paused
  steps every peer on to its frame (at most the delay), then all stand
  paused again (`Peer::paused`,
  `a_save_asked_while_paused_steps_every_peer_on_to_it`). Each peer
  autosaves alone by its own setting; the saves are one game
  (`every_peer_autosaves_one_game_at_the_day`). HD autosaves every peer at
  the host's interval (**Binary**).
- **Loading.** The host's LOAD GAME in the lobby. A save of one machine
  is refused: a quest or a single-player custom game has one human seat.
  So is one of another number of seats, or one whose game is not the
  hash its meta names
  (`a_lobby_refuses_a_save_that_does_not_fit_or_is_not_what_it_says`).
  Each lobby seat sits in the saved seat whose nick it has, the rest in
  the free ones in order; the lockstep seat is the saved seat, so the
  host may sit in any (`Welcome::host`). Every peer goes on from its own
  copy (`Plan::resumed`, networked), recording on from the save's
  session, so every `play.input` is again the same file
  (`a_lobby_sends_the_save_to_a_guest_that_lacks_it_and_seats_by_nick`).
  The transfer is before the welcome, so no drop timer runs; the 60 s
  first grace covers making the world. The command line's guest takes no
  save: a host that loads one waits on it.
- **Return to Lobby.** A LAN game's end panels offer neither CONTINUE
  PLAYING nor RESTART; MAIN MENU reads `GMTX` 336 "Return to Lobby", as
  HD's (`view::menu::end::Play`). It takes this peer back to the lobby
  page on its game's socket (`net::Peer::regroup`, `Flows::regroup`). A
  guest first says goodbye to the game, as a quit does, then asks its
  host again; while the host plays on, its hello is answered `Wait` (no
  timeout; "still plays: waiting for the host"). The host hosts again on
  its port with the session's name and its freestyle settings, and sends
  `Regroup` to each guest still in the game, again to any game packet
  after: those guests leave their game, ended or not, and ask too. Its
  lobby seats first, by seat, the guests in the game and those asking;
  a guest that quit, or does not ask within 2 s, is gone. Every guest's
  ready lapses; the next game starts as the first did. Two packet kinds
  in `wire` serve it. A `--server` host's lobby shows the menus'
  freestyle settings, not its command line's.
  `return_to_lobby_seats_the_same_players_again`,
  `return_to_lobby_leaves_out_a_guest_that_quit`,
  `a_lan_game_goes_back_to_its_lobby_and_plays_again`.

Limits: the game's LOAD GAME and RESTART are disabled in a LAN game (it
loads in the lobby); own orders reach the game a delay later, so a
pending order's cost shows then; a dropped guest rejoins only through
the menus or `--client`, not on its own.

A resync's world is drawn as a new game's (`resynced`, `Sim::resyncs`):
another board at the same `Board::relaid` or frame keeps nothing drawn
of the old one. Ground and scatter are drawn whole, veil, minimap ground
and frame-keyed panel rows again, no piece slid from its old place; a
selected piece or message flag it lacks is forgotten, a followed one
goes as in play (`a_resync_to_another_board_draws_that_board`). A load in
the lobby takes the game down and makes it again, so needs none of this.

A peer whose drawing stalls (a minimised window on Wayland draws
nothing) plays on: its sim thread keeps time and pumps alone
(`a_stalled_drawing_drops_no_peer_and_freezes_none`, a 6 s stall). A game
started from the menus' lobby pages plays alike on two apps over
localhost, pipelined (`a_lan_game_from_the_menus_plays_alike`).

Measured:

- Two headless peers on one machine (debug build, bots), as fast as
  lockstep allows: about 1,800 to 1,950 frames a second each; the same
  digest after 2,000 frames, the two `play.input`s the same file, and
  its replay the same digest; three peers alike
  (`two_threads_play_alike_over_localhost_udp`).
- On the simulated network of `net::tests` (paced as the drawn game),
  1,000 frames keep real time to 0.3 % up to 70 ms one way, and at 20 to
  60 ms with one packet in ten lost; at 100 ms one way 5 % slower, at
  100 ms with 50 ms jitter and 10 % lost 34 % slower: the delay is the
  room the round trip has.
- A LAN save of two and of three peers taken at frame 100 of a lossy
  network is one game on every peer (`deep_fold` equal, `play.input`
  equal); each peer's own copy plays 300 frames on in lockstep to one
  digest and recording, and the save loaded alone with that recording
  reaches the same digest
  (`a_loaded_lan_save_of_three_plays_on_alike_and_as_alone`). A 148 KB
  save reaches a guest that lacks it over localhost in a polled lobby.
- A guest's or the host's game given gold alone at frame 100 resyncs at
  frame 101: both play 500 frames on to one digest, deep digest and
  recording, which played back alone reaches the same game. Three peers
  at 20 to 60 ms, one packet in ten lost, a guest stalled 8 s while the
  file comes: none dropped. A second desync 200 frames on stops both,
  saying why (`net::tests`, `*resync*`).

## Delay

Frames an order waits (`net::Delay`, `net/delay.rs`). `--net-delay N`
fixes it. Otherwise (menus always, command line unless given) the host
picks it from its lobby pings, and welcomes every guest with it.

```text
guest A ──a── host ──b── guest B     a, b: half a mean round trip
leg      = a + 2 * stddev(round trips)
worst    = the two longest legs added   (one guest: its leg)
delay    = ceil((worst + 10 ms) / 50 ms) + 1, within 2 to 50
```

- Samples: the last 8 round trips per guest (1 a second). A guest with
  fewer than 3, or no guest: the default, 3.
- The lobby shows the delay a start now would have ("Network delay 3
  frames, 150 ms"; `Lobby` carries it) on this machine's own seat row,
  in the link cell that other seats' rows use for their ping. It moves
  with the pings; the start logs it in `play.log` ("net: lockstep
  delay"). Return to Lobby picks anew from the new lobby's pings; a
  rejoin takes the running game's.
- The command-line host, started when every seat is taken, waits up to
  3 s more for 3 pings from each guest, then starts with the default.
  Chosen so a LAN host gets its pings (1 a second) without a wait
  past a few seconds.
- **Measured** (`delay_absorbs_the_path_it_is_picked_for`, simulated
  network, no loss or jitter, 400 frames, paced as the drawn game): a
  delay of `d` keeps real time (under 3 % slower) up to a one-way path of
  `(d - 1)` frames less 10 ms, with 2 peers and 3 (where guest to guest
  is twice a link), for `d` 2 to 4; 30 ms past `(d - 1)` frames it does
  not. A sweep in 10 ms steps put the edge at 45, 95 and 145 ms for
  `d` 2, 3, 4 (2 peers). One packet in ten lost moved it by under 5 ms.
  A uniform 40 ms jitter moved it to the jitter's mean, not its end.
- **Inferred**: the 10 ms headroom (twice the 5 ms gap measured), the
  2 standard deviations of jitter, and the fallback of 3.
- Not done: links between guests (star only); a change from observed
  stalls (the step's real time, a slow machine): only round trips count.

### Following the network in play

With `Delay::Auto` the host changes the delay during play; `Fixed`
never changes.

```text
host    each sealing step: Steer::decide(pings) ─> Item::Delay(d') in the
          bundle for frame F = n + d      (Node::announce, one in flight)
every   at step F - 1 the bundle's items are taken: Node::change(n', d')
peer      raise by k: frames have + 1 ..= n' + d' are in no bundle, empty for
                every seat (a run in `skips`, counted past by `advance`)
              frame:  ... F+d-1 | F+d .. F+d+k-1 | F+d' ...
              sealed:   yes     |  no seat, empty |   yes
          lower by k: Node::seals is false until n + d' passes the last
                sealed frame; the orders wait in the outbox (`Sim::step`)
              step:     n'     n'+1  ...  n'+k-1   n'+k
              sealed:   none    none  ...   none     one (the next frame)
```

- **No game state.** The item is not given to `Sim::take`: no digest, save
  or `play.input` holds it (`Game` keeps the frame an order names, for
  the interface: `Game::set_delay`). A replay plays alone to the same game
  (`a_rising_latency_raises_the_delay_on_every_peer_at_one_step`).
- **Same step everywhere.** The item is in the host's bundle for frame F,
  taken by every peer as it takes that frame's items, so all switch
  between the same two steps. A peer may hold other seats' bundles past
  the gap before it switches: they wait in `bundles` until `change` closes
  the gap.
- **Digest frames.** A bundle names the step its quick digest is of
  (`Bundle::step`): with a lowering some steps seal nothing, and a
  digest is compared only where both peers sent one.
- **One bundle each.** Each frame has one bundle per seat, or is in the
  raise's gap; argued on `Node::seals`, tested by
  `a_raise_leaves_a_gap_and_a_lowering_seals_nothing`.
- **When.** Only the host, with every guest heard and pinged, none waited
  on, no resync, rejoin or stop, not paused, no change in flight
  (`Node::announce`). A guest's `Delay` item, or one outside 2 to 50, is
  dropped on receipt.
- **Rules** (`delay::Steer`): the pick is `Delay::pick` over the 8-sample
  windows, or over each guest's newest 3, the higher. Raise to it when the
  newest round trip alone calls for it and `RAISE_GAP` (2 s, **Inferred**:
  two ping rounds) has passed since the last change. Lower when the pick
  stayed below the delay for `LOWER_HOLD` (30 s, **Inferred**) after the
  last change, to the highest pick met meanwhile. **Measured**: without
  the newest-trip condition a path falling from 300 ms to 20 ms raised the
  delay (a window of both has a wide spread: pick 9 at once).
- **Resync, rejoin.** The Offer carries the delay in force at the file's
  step, which each guest takes in `resume`; a change in flight is dropped
  with the host's later bundles. A rejoin's welcome is sent again with the
  delay in force (`a_rejoin_welcome_and_an_offer_name_the_delay_in_force`).
  Tests: `a_resync_with_a_change_in_flight_carries_the_delay_in_force`,
  `a_save_with_a_change_in_flight_is_one_game`,
  `three_peers_follow_a_lossy_network_up_and_down`,
  `a_told_delay_never_changes`.
- **Shown.** `play.log`: "net: lockstep delay a -> b at step n" on every
  peer. The player list (`[ui] player_list` = `details`) ends with the
  delay in force on this peer.
- **Limit** (**Inferred**): a change takes effect d frames after the
  host decides (the item rides the bundle for frame n + d), so a path
  that grows fast stalls the game until it does.

## LAN discovery

Our own (`net::discover`); HD lists DirectPlay's sessions
([panels.md](../formats/panels.md#the-multiplayer-pages)). Every host's
lobby, the command line's and the menus', announces itself; Multiplayer
Setup lists what it hears.

```text
host    each second, an Open packet (wire kind 10, protocol version 1):
          lobby id, port, title, nick, seats taken, players, land and
          map-seed, the save loaded, build (version, profile, commit,
          data hash), any build allowed
        to each IPv4 interface's broadcast address, 255.255.255.255 and
          127.0.0.1, all at UDP 47100 (net::DISCOVERY_PORT), TTL 1
guest   Multiplayer Setup open: UDP 47100 bound, shared (SO_REUSEADDR,
          SO_REUSEPORT on Unix), so every guest on a machine hears a
          broadcast; a game unheard 3 s leaves the list (Inferred bound);
          one heard twice (two networks) is one game, by its lobby id
row     title (nick)  1/2 Players  land, Map N         build | Full | why not
          (red: version, profile or commit differ, `lobby::mismatch`;
          differing data shown as a warning, `lobby::data_warning`)
```

Announced, not asked: a guest's firewall need only let in 47100; a
query's answer would come to a passing port. Nothing leaves the LAN: a
broadcast is not routed, and TTL 1 stops each packet at the first
router. A row pressed fills the address box with its `HOST:PORT`; double
clicked, joins; typing an address still works. Eight rows show, no
scrolling. IPv4 only: no IPv6 multicast. Without a network up, a guest
hears a host on its own machine at 127.0.0.1; with several guests there,
only one of them (a shared port takes a unicast once). Tests announce
nowhere unless told (`lobby::ANNOUNCED`).
`lan_discovery_finds_hosts_warns_of_other_data_and_drops_one_gone`,
`guests_on_one_machine_share_the_discovery_port`,
`a_host_announces_to_the_limited_broadcast_and_this_machine`.

On two machines: both must let in UDP 47100 (discovery) and the game
port (47101 unless told); `firewalld`'s default zone drops both.

## Internet play

Menus only: Multiplayer, then "Play over the Internet (iroh)" on the
protocol page (`menu::lan`). No command-line option. Lobby, lockstep,
resync, saves, rejoin and Return to Lobby are LAN play's: each game
socket talks UDP on 127.0.0.1 to a local proxy (`net::iroh::Bridge`),
which carries every packet over one iroh connection to the host.

```text
guest game ─UDP─ guest proxy ══iroh══ host proxy, one per guest ─UDP─ host lobby/game
127.0.0.1        Bridge::join  direct  127.0.0.1, kept by guest key   127.0.0.1, any port
                               or relay
```

- **Host.** CREATE GAME binds the lobby on loopback, any port
  (`Lobby::bind_local`), not announced to LAN discovery, and starts an
  iroh endpoint (`Bridge::host`). Once bound, the lobby shows
  [COPY INVITE], which copies the invite to the clipboard. The host
  passes it to friends outside the game.
- **Invite.** The host's endpoint id (its public key) as text, nothing
  more: no address, port or session name. JOIN GAME opens once the
  pasted text reads as an id, spaces around it trimmed
  (`Bridge::check_invite`; `invite_is_an_endpoint_id`,
  `a_padded_invite_fits_and_reads`).
- **Join.** The guest binds a proxy on 127.0.0.1 and dials the host's
  id (`Bridge::join`); its lobby asks that proxy for a seat, as it asks
  a LAN host. A lost connection is dialled again each second. A host
  that closes the connection ends it: "the host left"
  (`a_guest_hears_the_host_leave`).
- **Finding peers.** The endpoint uses iroh's `N0` preset.
  **Inferred** (iroh's documentation, not this code): the id is looked
  up through n0's public discovery service; the connection starts over
  n0's public relay servers and moves to a direct path if hole punching
  works. Guests dial only the host: the lockstep is a star. Protocol
  (ALPN) `openregent/<wire version>`: another wire version does not
  connect (`the_protocol_names_the_wire_version`).
- **What crosses.** The LAN packets, unchanged (**Inferred**: encrypted
  by iroh's QUIC). One that fits a datagram goes as one, unreliable as
  UDP; a larger one (at most `wire::MOST_BYTES`, 16 KB) goes on a
  one-way stream of its own, off the relay loop; past 64 waiting, more
  are dropped, as UDP would (`packets_cross_a_direct_bridge`, 16 B and
  4 KB).
- **Host proxies.** The host's game tells guests apart by address, as
  on a LAN: each guest has its own loopback proxy socket, kept by its
  endpoint id. A guest that reconnects with the same key gets the same
  socket back and its old connection's task stops, so the game hears it
  at the address it had (`a_reconnected_guest_keeps_its_address`): a
  rejoin over iroh is a LAN rejoin from the old address.
- **Identity.** One key per data directory, shared by every profile,
  host and guest alike: `$XDG_DATA_HOME/openregent/iroh.toml`,
  `version = 1`, `secret = "<64 hex digits>"`, plain text. Made on
  first use, then read back (`the_identity_is_kept_across_joins`).
  Without a data directory, a new key for each run.
- **Protected nicks.** While the host's lobby runs, its bridge ties each
  seated guest's nick to the key it connected with, renewed at every
  lobby poll (`Bridge::protect`, from `Hosting::identities`): seated
  guests only, empty nicks none. A freed seat frees its nick; a guest that
  reconnects with the same key keeps its address, seat and nick. During a
  game the list stays as at its start. The host's proxy drops a `Hello`
  whose nick another key holds; other packets pass unchecked
  (`another_key_cannot_claim_a_seat`,
  `protected_nicks_are_this_games_and_named`,
  `a_lobby_nick_is_kept_for_its_key`). A player who loses `iroh.toml`
  cannot take back a seat in a running game.
- **Link health.** Each seat's lobby row shows its link, read from iroh
  every 250 ms: "Connected · 40 ms · Direct" or "Relayed", "Unstable",
  "Disconnected". A guest sees only the host's.
- **Leaving.** A host's endpoint forwards 250 ms more after it is let
  go, so its last packets (the guests told it left) get out, then
  closes.

Differs from LAN: no list of open games ("Internet games are joined by
invite code"), no port to choose, no `--server`/`--client`. Same: 2 to
4 players, delay picked as in the LAN lobby, build check, no mods.

Limits: anyone with the invite may join while seats are free; there is
no password. The host checks only a `Hello`'s nick against its key; a
peer may send anything the wire allows. Peers are trusted friends:
hardening against a hostile peer is postponed. **Inferred** from the
measured network above: with delay 3, a round trip much past 150 ms
runs the game slower than real time (the [Delay](#delay) section
measures it).
