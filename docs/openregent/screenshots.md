# Screenshots and the drawn game

`--screenshot <png>` draws the opening view in a window of `--size`
(default 1920x1200, the captures' size), saves one frame `--frames` drawn
frames (default 60) after the view is framed, and exits 0. A PNG not the
asked size exits 1 (`shot`). Take shots through `tools/shot.sh`, which runs
a hidden display.

What a shot holds fixed:

- The fog is lifted, as the captures' reveal cheat did. The game is paused
  and muted, settings are the defaults, rings are the art's (`--rings`
  picks another), no side ring (`--side-ring true`).
- Trees stand still; `--wind-at <ms>` sways them with the wind held at that
  game time (`--wind-amplitude`, `--wind-shadow still`).
- Pieces step a frame at a time; `--smooth-position true` slides them.
- No log, recording or settings file is written. Scale is 1, or `--scale`.
- Under Wayland a fractional display scale rounds the window (1919 at
  1.7); unset `WAYLAND_DISPLAY` to run through X11.

Repeated runs give identical PNGs
(`a_shot_steps_its_clock_by_the_frame`,
`a_running_shot_plays_its_frames_whatever_the_waits`):

- The clock steps 1/60 s per drawn frame, not by the wall clock, and only
  on the `--frames` counted once the window has its size. The chat box
  keeps the real clock, as in a game.
- Before counting, the clock is held until the art is loaded, every
  pipeline compiled, every visible mesh's material prepared and bound, and
  two captures match. Still waiting after `BUSY` frames: nothing is saved,
  exit 1 (`a_shot_still_pending_fails_rather_than_saving`,
  `a_mesh_without_its_material_keeps_a_shot_waiting`).

Aids for panels:

| Flag | Effect |
|---|---|
| `--warm <frames>` | Plays that many frames first (with `--bot`, its presses) |
| `--select CODE[:N]` | Selects the Nth piece of a descriptor; none: an error naming it and the pieces shown, exit 1 (`a_select_missing_names_its_code`) |
| `--page` | Opens a page under the selection |
| `--zoom <z>` | Draws at that zoom, smooth zoom on |
| `--dialog options\|goals\|settings` | Opens `AP36`, `AP88` or the settings editor over the game |
| `--settings-tab settings\|enhancements` | Opens the settings editor on that tab (with `--dialog` or `--menu settings`) |
| `--end` | Holds a stage of the end screens |

## Comparing with the captures

Shots were compared with captures of HD, not in the repository and held
by no test. The captures are HD at 1920 × 1200, one per landscape,
each a seed-1 new game's start with Wizard Hold and Ruins of Esterhausen
(every title bar says so). `--map-seed 1 --land <code>` with the default
picks reproduces each; the code is the land's four letters, as `KxHa`.

Only the map views were compared. HD's map view is capture pixels
`[206, 1916) × [29, 1139)`, inside its frame and above the bottom bar. The
comparison drops the bottom 49 rows, where messages print, and openregent's
overlays. It finds the shift by edges, not colour, and reports per-channel
mean difference, signed (shot − capture) and absolute, plus a diff image,
mid grey where equal. At 1920 × 1200 openregent's column is wider than
HD's (the map starts at x 292), so the shift is not zero.

**Measured**: in both, the view stands flush on the map's left and bottom edges, as HD's
minimap view box shows. Buildings, trees and shadows cancel pixel for
pixel. Ground variants do not (they are invented, below), so the absolute
figure is mostly that; the signed figure is the colour difference. The
selection ring on the start palace lowers every land's absolute difference
by 0.1 to 0.5 per channel.

## Drawing the map

openregent's map draws a `map::MapSource`: the map's vertex and cell
arrays and its object records, made from the live world
(`MapSource::of_world`). It takes levelled heights, shade (`Board::shades`),
explored flags, each cell's paint and picture, and the scatter and terrain
features no ring cleared, each in its set with its slot and frame
(`Board::strewn`). A start piece's saved set also comes from the world
(`Board::saved_looks`, `map::pieces::Saved`).

- A cell generated or painted since its save has no known picture: the
  engine draws it from a clock-seeded stream, so one is invented
  (`dressing::spread`).
- Every corpus save taken into a world draws as the save
  (**Measured**).
- When a placement levels, paves or clears (`Board::relaid` past the
  source's `generation`), `ground` draws the world again. A `GRND` record
  the world dropped loses its entities; the rest are found by handle and
  renumbered (`map::things::Record`, `a_redrawn_record_is_found_by_its_handle`).
- Scatter keeps its own height, as in the engine (`Board::stands`): only
  what the flatten reseats moves; the rest stays where it was drawn though
  the ground under it moved. A full redraw draws the same
  (`a_shifted_tree_lands_where_a_redraw_draws_it`).

```text
 tick:   owed game time ──> Game::frame per 50 ms ──> World::shown
 ground and scatter: regent_draw's map::redraw::redraw on the Source, BLDG and VHCL off
 a placement: Board::relaid ──> ground::relay: ground and GRND from the world, cleared scatter despawned
 pieces: one entity each, a child per layer, stepped by map::reel::step at game time
```

## Game time and speed

- Fixed 50 ms frames (`runtime::MS_PER_FRAME`) times `--speed`, at most ten
  per drawn frame. The engine does not catch up a slow frame; openregent
  does, up to ten.
- A piece is drawn where the last frame left it, as the engine projects the
  stored position (**Binary**). With `[enhancements] smooth_position`
  (`game::clock::Motion`) a piece, its effects and ring, and a speed-trail
  clone slide between their last two places
  (`a_piece_is_drawn_between_its_last_two_places`). A piece placed outright
  (`Shown::jumps`: a teleport, a door, a camp move) draws at once, as the
  engine does not smooth (**Game**: a wizard's teleport;
  `a_jump_draws_at_its_new_place_at_once`). Missiles (by handle) and
  particles (by system, pool slot and birth, `Mote::id`) slide the same
  way; a new particle starts where it is
  (`a_particle_slides_from_its_own_last_place`).
- Speed 1 is the engine's default: each frame lasts at least
  `TimePerFrame`, 50 real ms. The options' game speed picks 10 to 250 ms
  (`runtime::PACES`, **Binary**). `[game] speed` is the slider's position,
  `1` to `9`, default `5` (`speed`, `the_game_speed_is_the_sliders_position`).
  Each game starts at it, or at `--speed`. A change in the settings editor
  applies at once; `<` `>` step along `PACES` and the setting follows,
  unwritten. The original keeps no speed past the session and starts each
  game at 50 ms: a departure. Not in a LAN game
  ([multiplayer.md](multiplayer.md)).
- Speed keys: 1, 2 and 3 set 1×, 2× and 4× with `[enhancements] speed_keys`
  (on in Modern). A stopped game stays stopped.
- Art steps on game time, so `--speed` scales it. Each frame shows for its
  set's `action::frame_ms`. A new set opens on frame 0; a turn keeps the
  frame (`a_new_set_opens_on_frame_0_and_steps_at_its_rate`).
- Pause: Space or the Pause key. Space is a departure: the original's Space
  hangs an attack flag (`World::space`), which openregent does not bind; the
  original's pause key is not read. Paused, no frame steps, art stops,
  selection goes on, and `GMTX` 292 "Game Paused" shows in the message strip.

## Pieces

- A flier's body draws at its own height (`Shown::aloft`,
  `map::pieces::body_at`), its shadow on the ground under it, and what hangs
  on it with it ([movement.md](../original/movement.md#fliers), **Binary**;
  `a_flier_draws_its_body_at_its_own_height`). With `smooth_position` its
  height slides too (`a_fliers_height_slides_with_its_place`).
- A walker's `Walk` frame is the world's (`Shown::walk_frame`,
  `map::pieces::pin`): a step moves it on; a turn, a wait or the frame
  between nodes holds it (**Binary**; `a_walkers_drawn_frame_is_the_worlds`).
  A walker waiting on another (`Doing::Waiting`) draws `Stand`, as the
  engine plays it.
- Which set a piece shows:
  - The set the world keeps comes first (`Shown::look`, one `action::Look`):
    a held frame, or a set playing from a game time.
  - A mobile's action until it steps (`Walker::look`): its `InitAction`'s
    frame, held as track 2 steps it (**Binary**), then its after-set from
    frame 0, or with none its last frame held (**Inferred**).
  - A death: `Die`, then from its peak `Dead`, frame 0 (**Binary**): a
    hero's gravestone, until its script deletes it.
  - A building (`World::look`): construction, `Die-n` by damage,
    `Active`/`Inactive`, `Crumble` as it falls, `Dead` in ruins, in its
    record's slot; gone when its rubble's timer ends (**Binary**).
  - Otherwise `Walk` while `IsMoving`, `Stand` otherwise (a building's
    `Inactive`), its slot from its facing. **Inferred**: the world keeps no
    walk or stand set, only the `Walk` frame.
  - A set the art lacks shows as if none were kept.
- A held piece is not drawn (state bit 1, **Binary**). A hung one, such as
  an attack flag on its target, draws at its holder's drawn place plus its
  offset and height (`Housed::Hung`).
- A particle-system piece draws only its particles.
- An earthquake's runs (`Output::shakes`) each shake the view `min` to `max`
  pixels each way on a clock-seeded stream, as the engine does (`quake`,
  **Binary**).
- Missiles (`World::flown`): each where its flight is, lifted by its
  height (`Maps::aloft_at`), in its `Walk` or first set from its launch,
  the slot by its heading to its goal (**Inferred**). Not selectable.
- Effects (`World::effects`, `effects`): each effector at its holder's
  drawn place plus its projected offset; a spent missile where it struck;
  each in its set from when it began, slot by facing
  ([vm.md](../gpl/vm.md#what-an-effect-shows), **Binary**). A spell unit
  plays its intro, loop and end set as a piece. Depth by board y; an
  overlay level with its holder draws in front of it. One held with its
  holder is not drawn.
- Speed trails (`World::clones`, `trail`): each shown clone where it
  stands, in its leader's look as last recorded, mode 5 at its level, no
  shadow ([vm.md](../gpl/vm.md#speed-trails), **Binary**); its frames as
  far behind the unit's as that look is old (**Inferred**). Not selectable.
- Particles (`particles`, `Sim::motes`): each live one a frame of its
  system's art at its place, lifted by its height; body and shadow pass
  as a piece's, glass at mode 8 while its alpha is below 1; depth by its
  row (**Binary**, [vm.md](../gpl/vm.md#particles)). `Sim` owns them
  (`view::map::particles::Sparks`), stepped after each frame from the
  world's particle pass. Not saved: a loaded game's start afresh. Their RNG
  takes `--seed`, a save's GPL RNG or a snapshot's session seed
  (**Inferred** stand-in for the clock). The default game's first, a
  damaged building's smoke, comes at 276,250 ms.
- Scatter depth: between the scatter rows either side, as the viewer ranks
  scatter.

## Depth and draw modes

- Depth adds the set's sort nudge, screen pixels over 32 sin 55°
  (`content::imagery::sort_cells`, **Binary**): reward flags 2,000 and
  1,500, a hero −15, a ruin −1,000. Thought bubbles and `Got_Gold` take
  none: they sort at their holder's y plus their offset. A higher layer
  whose set sorts on its own takes its own nudge (`Animation::listed`,
  `map::cel::rungs`), such as the Mausoleum's dirt diamond under it; the
  map view and the preview order layers the same way.
- Draw modes 5 and 8 draw translucent (`material::Pass::Glass`), alpha per
  index from `view::raster::blend`, no shadow
  ([art.md](../formats/art.md#modes-5-and-8-are-translucent)). A tinted
  piece's palette goes to the tint's nearest colours; a see-through one
  draws mode 5 at its level, its shadow dropped.
- Each translucent frame (map thing, piece, trail clone, preview or ring)
  lifts the soft shadow under it (`material::Pass::Lift`, **Inferred**
  approximation), so the shadow of anything sorted after still falls on it.
  A see-through level above 4 is under half drawn and lifts nothing: the
  invisibility (9) and illusionary hero (5) levels that `GetProperUnitArt`
  sets (`Spells.gpl`) never do; a speed trail's nearest clones do. A glass
  particle lifts where at least half drawn (`regent_draw::soft::lifts`,
  **Inferred** approximation: coverage resolves last, the engine shades as
  it draws).
- Every stream of a slot draws, each frame at its nudge. A layer with flags
  bit 9 adds a second pass of its bit 24 frames, a flier's shadow, on the
  ground under the piece after its body; the pick skips it (`Pose::blits`,
  `Drawing::pass`, `map::pieces::stands`,
  [art.md](../formats/art.md#the-second-pass-is-a-fliers-shadow),
  **Binary**). A flying missile's falls under its flight; the map view's
  under a saved thing's own height.
- A text frame (`Got_Gold`'s amount) draws the effector's value
  (`Effect::value`) in font `SM09`'s size, white, centred on the anchor plus
  the frame's nudge (`words`), at view zoom 1.0 only (`view::map::amount`,
  `words::shown_at`, **Binary**); at any zoom with `smooth_zoom`
  (`text_shows_at_one_and_at_any_zoom_when_smooth`). **Inferred**: a colour
  whose top byte is `0x80` draws nothing.

## Orders and modes

The player's input is `game::Input`, sent before the next frame
(`Sim::input`, [vm.md](../gpl/vm.md#plays-input)). What is offered, at what
price, and where a building fits are the library's (`world::offer`,
[panels.md](../formats/panels.md#what-the-panels-offer)); the panels send
them (`panel`, `orders`). A price over the treasury shows red; the decode
refuses it.

- The palace's BUILDINGS page is the build list (`AP03`,
  `World::build_list`, `by_title`; `the_drawn_build_rows_go_by_title_not_by_code`):
  the offered, the rest red under the 5405 toggle, icons under 5404. A
  press lights a row and leaves any mode; a double click or BUILD places it
  (`a_build_row_is_placed_on_a_double_click_only`). "?" (8029) opens `AP95`
  on the lit row's (else the first row's) `HelpID` under the type's name, a
  departure; its back button 8004 reopens the list.
- REWARDS sets the flags (`AP41`, ± `#RewardDelta`, kept within the
  treasury, `Rewards::clamped`).
- The spell bar casts. A spell shows in colour where the crown may cast
  it, whatever the treasury (a short treasury says `GMTX` 34 on click,
  `orders::cast`); else its slot 2, grey in its temple's colour (hovered,
  6 more, `menu::drawn_slot`). While the Abode rests, its bar 9500 lies over
  its six buttons, filled as its panel's 4500, their press ignored
  (**Binary**; `spell_bar::RESTING_BAR`, `World::abode_rest`, `rest_bar`). A departure
  ([enhancements.md](enhancements.md)): every icon full size,
  `view::panel::spell_bar::BUTTONS` in `AP92`'s order: the base spells'
  `INTN` sets, the Abode's six `IX02` 1007–1012, the expansion's `IX23`
  1023–1028 unsqueezed; one slot, toned grey inside the frame when not
  castable, the frame `LIT_FRAME` when hovered
  (`a_one_slot_icon_tones_as_the_slots_it_lacks`). A base quest's catalogue
  has no `IX02`: its 22 spells, no Abode's; else all 28
  (`the_base_bar_holds_every_spell_but_the_abodes`).
- The selection's buttons:

| Button | Behaviour |
|---|---|
| Recruit, research, upgrade (8007), enchant | As the panel offers |
| Rangers' guild camp move | Preview as below |
| Warriors' guild Call to Arms | 8009, key C, price in 4481; disabled with no living member; short: `GMTX` 37; `Input::Assemble`, `Input::Resource` |
| Rogues' guild EXTORT | 8009, key E, `GMTX` 193 of `Game::extortion`'s share (`Sim::extortion`); enabled from 1; press sounds `EN01`, says `GMTX` 194, sends `Input::Extort` (`the_extortion_says_both_shares`) |
| Marketplace MARKET DAY | 8200, key M, once researched, `Input::MarketDay`; while running bar 8201 and text 8826 (`World::market_day`) |
| Temple to Krolm RAGE OF KROLM | 8009, key K, price in 4484 on the `INTI` 1016 box, red when short; enabled with a hero; short: `GMTX` 36; `Input::Rage`, `Input::Resource`; while running 8201 and 8826 (`World::rage`) |
| Hall of Champions bounty | Page `MX01`: the slain types, the first lit; 2050 S and 2051 L say `Game::bounties`; `Input::Bounty`; while one runs `MX00` shows 4500 and `GMTX` 412 (`World::bounty_running`; `place_bounty_opens_the_bounty_page`) |
| Own unit's name box | 8005: a press edits it in an egui box over its rectangle; Enter sends `Input::Rename`, Escape drops it; game keys and chat idle meanwhile. A hero below `#HeroNameEditingLevel` says `GMTX` 192 (`World::naming`; `a_low_heros_name_box_says_the_level`) |
| Embassy open or close | Its timer's share |
| Mausoleum RESURRECT | R, for the hero lit on LIST OF DEAD (`MX05`), priced by `Game::crypt` in 8006, red when short; the press selects the hero (`resurrect_sends_the_lit_hero_and_its_price`) |
| Flag raise and removal | Raise `AlterAttributeDelta`, at any reward; removal `Input::Unflag`, asked first while it holds a reward (`orders::remove`); a double click on it too |
| Tax and repair checks | 8024–8027, each flips its flag (`Input::Toggle`), dimmed where `World::upkeeps` disables them |
| Demolish | 8002, asked first, `Input::Demolish` |
| Dismiss hero | 8002, key X, asked first in the selection's slot, `Input::Dismiss`; only the player's hero not yet dismissed; then `APb5` (`Hero::panel`) |
| Tax collector amounts | Return and pickup, ± `#Taxman_Increment` where `World::tax_press` sends (`Input::Alter`) |

A mode waits for the map (`input`). Right click or Escape cancels.

- Building: the preview follows the cursor on its picked vertex over a
  green or red ring fitted to its clearing reach (`preview`,
  `Fit::Clearing`; `only_a_fitting_preview_shows_the_green_ring`). The
  verdict is `Game::placement`'s, its `CanIBuildThisBuilding` run each
  update as an interface run. A click where it fits sends `Unpreview`, then
  `Build`. A refused click sounds `Error_Alert`
  (`only_a_refused_preview_sounds_the_error`).
- Camp move: the guild's preview, asking the ground alone; a click sends
  `Camp`; leaving any other way sends `CampLeave` and the held price comes
  back (`World::camp_leave`).
- Point spell: `Cast` where the cursor's test takes the point
  (`World::spell_point`). Unit spell: `Bless` on the piece clicked if the
  cursor's test takes it (`World::spell_target`). A reach spell out of reach
  is refused with `GMTX` 40 in the chat (`World::spell_refusal`).
  SuperCharge casts at once. A cast keeps its spell chosen but for the
  Abode's six (**Binary**; `a_spell_stays_chosen_after_a_cast_but_the_abodes`).
  While aiming, a press on the selection portrait 8011 casts on its unit as
  a click on it would (`World::spell_on`); else nothing. Over it the portrait shows the spell's
  cursor, its slot by that unit (`cursor::Portrait`; **Binary**).
- Spell reach: drawn on the ground while aimed with `[enhancements]
  show_wizards_guild_spell_range` (on by default; `reach`: the minimap's
  circles, `World::aimed_reach`, a faint white fill and rim over the veil,
  under every piece). The same shows, on ground and minimap, while a
  Wizards' Guild or Tower is placed, with the preview's own circle
  (`World::placing_reach`); the original shows none then.
- Explore flag at the point; attack flag on a flaggable piece.
- A double click with no mode sends a flag as `World::twice` says, the flag
  then selected ([panels.md](../formats/panels.md#what-the-panels-offer)).
  A double click is a second press within 500 ms and 2 pixels, Windows'
  defaults (**Inferred**; `a_second_press_near_in_time_and_place_is_a_double_click`).
- Message flags: drawn as an effect (`World::effects`, set `0x40`), always
  seen when standing alone. A click on its pixels selects it
  (`select::Reading`, before other pieces: **Inferred**); a double click
  sends `Input::Read`. The selection slot shows the message scroll, `AP77`
  ([panels.md](../formats/panels.md#the-message-scroll), `panel::message`;
  `the_message_scroll_is_ap77`): its text centred, wrapped and scrolled;
  CLOSE plays `ME01` and sends `Input::Read`; the zoom button centres the
  view on the flag first. The panel then goes (**Inferred**: at once, not at
  the decode). No ring. The tracking window check sets whether a new
  message flag is followed.

Enemy forces are the start's units: buildings, mobiles, terrain features
and off-map lairs, as the generator places them.

## The main screen

The interface is HD's main screen, `APMK` from `UIData_1920_1200.dat`
(labels from the dataset's `STRT APMK`, the file's index a table two
longer), in the original's pictures, text as the menus' (`panel`).

- Column: its left 206 × 848: minimap, selection panel at (0, 182), a page
  at (0, 428), the mini view's frame (`apmk_pieces_fall_in_their_places`).
- Top bar (29 high): gold and day. Bottom bar (35): SPELLS, the message
  strip and OPTIONS.
- Spell bar, open: a band above the bottom bar, the bars' width, `AP92`'s
  frame `INDz` 1012 kept at its ends and repeated between
  (`a_frame_picture_keeps_its_ends_and_repeats_its_middle`). Buttons laid
  out by `layout::spell_grid`: 25 apart from (4, 4), 5 more between temples
  (**Inferred** from the base `AP92`); a new row where the next button, or
  a temple that fits a row whole, does not fit. Rows stack upward, 29
  apart; the band is `AP92`'s 34 high plus 29 a row, over the map's bottom
  (`Frame::with_spells`), which keeps its size; a click there is the bar's
  (`a_narrow_bar_wraps_into_rows_stacked_upward`,
  `a_click_on_the_spell_bar_or_options_does_not_reach_the_map`). A
  departure: the original's `AP92` is 590 wide.
- Scale (`layout::Frame`, `layout::scale`, the menus' too): window height /
  848 for everything. The column is `206 × scale` wide, its shape kept. The
  bars are as tall at that scale, pieces kept to their left or right edge
  or centred; a piece 1000 wide or more stretches over the blank middle.
  So a window made only wider changes only the bars' middles and the map
  (`a_wider_window_only_widens_the_bars_and_the_map`,
  `a_taller_window_widens_the_column_and_keeps_its_shape`). The map has the
  rest: its rect and the world camera's viewport.
- Controls: every control's picture, panels' and menus', is the engine's
  slot (`view::menu::drawn_slot`, `panel::sheet::slot`;
  `every_control_slot_is_the_engines`): hovered 6 on, held down lit too (7,
  a seven-slot set 1; `panel::sheet::look`); disabled its slot 2, dimmed
  only where its set has none.

### Top bar

- Gold: the crown's gold, the bare number, control 2006 (**Inferred**: from
  the local player; `gold_reads_as_a_bare_number_without_the_table`).
- Day: `day::Day::of(World::now)`, text `GMTX` 24 `%d Day%s` in 7021 over
  2020, which fills over the day. A day is 60,000 game ms; day 0 at the
  start; the bar counts whole seconds, 0 to 98 of 100, as the original
  (**Binary**, [panels.md](../formats/panels.md#the-day-counter);
  `a_day_starts_at_zero_and_turns_every_minute`,
  `the_bar_counts_whole_seconds`).
- `--headless` prints both.
- The title scroll names the game (`view::panel::scroll`): a quest's title,
  a custom game's Enemy Forces name (`the_scroll_names_the_game_as_the_top_bar_does`).
  A press opens the goals.

### Minimap

The minimap (`minimap`, [minimap.md](../original/minimap.md)) is the
library's picture (`view::panel::minimap::draw`), 176 texels at `(4, 3)` in
the column, a texel `scale` points, nearest-neighbour.

- The ground is cached; a cell is redrawn as the crown explores it, all of
  it after a placement relays the map.
- Drawn over it: units, icons, the selection and its kin in white (a
  selected flag's seekers), marks, the combat blink, reach circles (as
  above), the main view's frame from the world camera and zoom, then the
  tracking window's, light grey (`the_main_views_frame_is_as_wide_as_the_view`).
- Left press or drag centres the view there; a double click sends an
  explore flag, then selected; right press or drag puts the tracking window
  there, following nothing; neither leaves a mode.
- Filters all on. **Inferred**: a list's own order by agent.
- Buttons ([panels.md](../formats/panels.md#the-main-screen)):

| Button | Effect |
|---|---|
| Select the Palace | Selects the crown's palace with its sound and leaves any mode; the view stays, as the original's |
| Zoom check | Toggles 1:1 and 2:1 out about the view's middle (`zoom::toggle`); checked while out |
| Hit point bars check | Sets `[ui] bars` to `never` and back to the default; checked while shown |
| Names check | Sets `[ui] names` to `off` and back to the value before (`on` if none, `Names::toggled`; `the_names_check_toggles_off_and_back`); checked while not `off` |
| Minimap textures check | Stands checked, does nothing |

F9, F10 and F11 press the names, bars and zoom checks (`APMK` key 5).

### Selection

- The pick (`select::Picker`, [art.md](../formats/art.md#the-pick)) is the
  engine's: front to back, the first drawn piece met on a pixel of its
  current frames, else the last whose box, grown 4 screen pixels, holds the
  point; a hung piece gives its holder. The box is the shown tiles, the
  anchor and, selected or hovered, the ring
  (`a_spell_effect_takes_no_click_but_a_hung_piece_does`,
  `the_pick_meets_a_buildings_body_not_its_gaps_or_shadow`).
- The game starts with the local player's palace selected, silently,
  before frame 1; with no palace, nothing (**Binary**, `World::selected`;
  **Measured**). Later picks are the
  interface's.
- A left click in the map selects the piece the pick takes; empty ground
  keeps the selection; a right click clears it (**Game**).
- The selection panel is the piece's `DialogID` panel (a flag's by kind and
  owner: `AP46`, `AP80`, `APb6`, `APd0`), its controls drawn where the panel
  puts them; those a controller fills come from the world (`panel::upper`):
  - name 8005 (`portrait::title`: `AP:1` spelled by
    `world::naming::spelled` with `APU:4`'s generator, else
    `NameGenType`'s; else `World::numbered` by `portrait::numbered`,
    "Wizards Tower #2"; a nameless building its first level's title in
    capitals, `a_nameless_building_shows_its_first_levels_title`; a flag's
    panel its own words);
  - portrait 8011 (set 1000; a piece lying dead, `Shown::ruined`, grey and
    darker, **Inferred** tone);
  - hit points 8006/8010 (`portrait::Health`, `GMTX` 7, none on a grave;
    `hit_points_read_as_the_panel_writes_them`);
  - line 8014 (coffer `GMTX` 76/77, a Mausoleum's 185, a sign's or
    chest's); activity 8030;
  - a hero's (`AP20`, on its set 1001): class, level, experience bar,
    weapon and armour pictures at their upgrade (`portrait::gear`) with
    their `+n`;
  - a Blacksmith's researched gear (`gear::Forge`, none: empty;
    `the_forge_panel_holds_its_gear`);
  - a building's level (its code's digit, **Inferred**), upgrade
    8007/8015, tax and repair checks, member count 8019, demolition, page
    buttons (HEROES, VISITORS ("Nobody Here" when empty, `GMTX` 230),
    RESEARCH, the palace's BUILDINGS, REWARDS, STATISTICS and ROSTER);
  - the palace's counts beside its henchmen icons (`Sim::henchmen`,
    `GetSupportedHenchmen` as an interface run; an icon opens the henchmen
    roster; **Measured**);
  - a temple's SPELLS (8009) opening its own page `AP66`–`AP72`, the
    Abode's Spells (8851) `MX07`, both `Page::Casting`
    (`spell_bar::spell_page`); neither opens the spell bar (**Binary**);
  - the band at (7, 190): a running recruit's or research's bar with its
    text (`World::progress`,
    [panels.md](../formats/panels.md#a-buildings-progress-bar)), else the
    recruits side by side (`GMTX` 17 "RECRUIT %s"; a full guild `GMTX` 19
    "GUILD IS FULL"; a warriors' guild with a temple the original's half
    buttons, `GMTX` 18 "Warrior Discord", the price in its box, three in
    thirds) and an enchantment;
  - a warriors' guild's counts by kind (`World::war_roster`); a tax
    collector's amounts and ±; a monster's ratings (`details::rating`);
  - a flag's reward, raise, removal, target, "Offered by" the profile's
    name, its seekers' count and View Heroes (`World::seekers`).
- The page under it (`panel::lower`):
  - tips (`AP95`, or `AP94` under a panel with 8028,
    `portrait::help_page`; its `HelpID` text, a flag's `h152`,
    `portrait::help`, markup colours, scrolled;
    `a_panel_with_8028_opens_the_short_help`);
  - the build menu; the rewards;
  - a guild's members (`APd1`: icon, level, `GMTX` 220 "%s is %s (%d of %d
    hp)", title "Heroes 2 / 4, 1 dead", `World::members`, `World::roster`);
  - visitors (`AP91`, the Fairgrounds' `AP09`, `World::occupants`;
    `the_fairgrounds_visitors_page_is_ap09`);
  - a flag's seekers (`AP87`, `AP86`, as the members;
    **Measured**);
  - research (the building's own panel, each row its research id's
    controls);
  - the Mausoleum's dead (`MX05`, rows as the members');
  - a graveyard's heroes (`AP16`, the local player's, `GMTX` 221 rows,
    detail always);
  - a hero's statistics (`AP21`, ten stats, the primary starred), items
    (`AP22`, `gear::Gear::name` "Longsword (12) +1", gold, taxed gold,
    potions, items by `details::item_name`: `QITM`, or a mod's item
    character's title through its strings, the effect on hover) and spells
    (`AP78`, `INTn` icon and `DACT` title; enchantments, the effectors hung
    on the hero, with their `GMTX` title and icon:
    `view::panel::portrait::enchant`);
  - the yes or no of `AP62`.
- A new selection opens its page: a hero the last hero page, else items;
  the palace its build menu; a guild its members; a researcher its
  research; the rest their tips. Nothing selected: the crown and no page.
  The build menu and its tips are the palace's alone; leaving them ends a
  placement (`the_build_list_shows_only_under_the_palace`,
  `leaving_the_build_list_drops_a_placement`).
- A one-slot page has no back button, nor its hot key Tab: the selection
  above it is always shown (a departure; the original draws it).
- A page filling both slots keeps it (`lower::Span::Both`), back to the
  palace and its build menu: the palace's STATISTICS (`AP40`,
  `World::statistics`) and rosters (`AP85`, `APc1`, `APc2`:
  `World::roster_of`, tabs, sorts, the detail check, SELECT; a building row
  picked centres the view, its repair and tax marks at its right;
  **Measured**). These are
  `UIData`'s 490-high panels from (0, 182), drawn instead of the selection
  (`panel::palace`,
  [panels.md](../formats/panels.md#the-palaces-counts-statistics-and-rosters)).
- A list row's text wraps in it; lines past the row are cut, the last
  ending in "…" (**Inferred**: the original's cut is not seen;
  `a_cut_line_ends_in_an_ellipsis_within_its_width`). Placeholder text (a
  number in a text box, "2000") is not drawn.
- A bar (hit points, poisoned green, experience, progress, day) draws its
  record's set + 4 cut to its share over set + 3 where there is one. The
  panels name 1000; the records (`INBc`, `INBd`, `INDx`, `INDy`, `INS2`,
  `IMZa`) hold only 1003, 1004 and 1014 (**Inferred**). Pictures are
  flattened by their draw mode (`Sprite::as_drawn`).
- A list or text that scrolls shows its scroll bar's arrows and thumb; the
  thumb drags (`a_drag_through_the_whole_track_scrolls_the_whole_text`).

### Tracking window

Mini view 1 (`tracking`;
[panels.md](../formats/panels.md#the-tracking-window)).

- What it follows is `view::panel::tracking`'s, run on the published world
  each drawn frame (`a_new_game_follows_the_palace_at_its_exit`). Its
  caption is the followed piece's panel name.
- A second camera draws the world into a 380 × 252 image every second drawn
  frame (a cosmetic departure, half the cost), one texel an art pixel,
  shadows resolved into it as in the world image: layers 0, `OVERLAY`
  (bars) and `map::redraw::INSET_LAYER`, its own ground sheet
  (`map::redraw::Inset`), composed under the main ground's patches, which
  draw where both reach. A recomposed sheet keeps the one before under it
  for `map::redraw::RETIRE` frames, since the new texture can come a frame
  late; this keeps black ground out of the window.
- The column shows the image bilinear in `APMK` 2002's rect, 0.5 times the
  column's scale, the same under every `view_upscale`.
- The pointer on it is on the map (`select::Pointing`, `Aimed`): every
  mode's cursor, click and order as in the main view, the pick's boxes at
  zoom 0.5 (**Binary**; `the_pointer_lands_on_what_the_window_draws`). No
  map tooltip over it. Names and amounts paint over it by the main view's
  rule (`words::shown_at`) at zoom 0.5: none unless smooth zoom.
- Driven by its button row, the selection panel's 8001, unit list rows, Alt
  with a focus key, the message scroll's check and the minimap's right
  press.
- Saved with the interface.

## The view

- The view opens at 1:1 (an art pixel a logical point) on the player's
  palace, or with no palace on the map's middle. It never shows past the
  map's rect (`stage::bound`): zoomed out, it stops at the least zoom that
  fits the room on both axes (`stage::least`), at any window shape, then
  pans in (`stage::bounded`; `a_bounded_view_never_shows_past_the_extent`).
  A pan set in the panels' pass (a minimap drag) is bounded at once
  (`View::centre`). The zoom range is openregent's, so this rule is too. The
  map draws as a rectangle, north up. The original holds the view's top left
  to the map less the view as it scrolls (**Binary**), giving the same
  edges; every HD capture agrees
  ([Comparing](#comparing-with-the-captures)).
- Scroll (`pan`, `view::map::pan`): the window's edge, as the original's,
  on by default. The pointer within 8 px of a window edge scrolls toward
  it at `ScrollSpeed` × 20 px a second, ramping from a fifth to full over
  500 ms (the original's numbers: **Binary**), by real time each drawn
  frame
  (`a_cursor_near_an_edge_scrolls_toward_it`,
  `the_ramp_rises_from_a_fifth_over_half_a_second`,
  `full_speed_is_twenty_times_the_scroll_speed_a_second`). Not under a menu,
  once the game ends, or while a panel control is held; hovering a panel
  stops nothing, as the original's edge is the window's (**Inferred**: the
  original takes the frame only for a held or pressed control). Off the
  window winit reports no pointer, so the original's `-edgescroll` (any
  distance past a window) has no case. The arrows step `ScrollSpeed` a key
  event, auto-repeat too, as the original's; held, they scroll as the edge
  does with `[enhancements] smooth_arrows`. `[ui] scroll_speed` is
  `ScrollSpeed`, 32 to 64
  (`the_scroll_speed_is_the_originals_and_held_to_its_slider`).
- Zoom (`zoom`): the original's two levels, 1:1 and 2:1 out (scale 0.5,
  `view::map::zoom`, **Binary**: the only values the original uses;
  `the_toggle_goes_out_from_regular_and_back_from_anything_else`). The
  wheel picks one, in or out, about the pointer; over a menu, the settings
  editor or the spell bar (`ui::Covered`) it zooms nothing.
  - With `smooth_zoom` a notch steps `zoom::STEPS`, quarter octaves 0.5 to 4
    (`2^(k/4)`) and 3, held as an index so 0.5, 1, 2, 3, 4 stay exact;
    touchpad deltas add up to notches. With no palace the view then fits
    the map, at the entry at or below (`zoom::snap`), raised to the least
    zoom; so does the wheel's lowest notch.
  - A middle click or 0 goes back to 1:1: the click as the wheel, not under
    `ui::Covered`; 0 not while a menu, the settings editor or a text box
    has the keys.
  - The viewer keeps its own `1.15^notches`.
  - The sound radius takes the zoomed-out cell below 1:1 (`audio::listen`).

## Rings and cursor

- The selection ring is the game's sprite (`ring::ring`, `Look::Selected`):
  its tile anchor at the piece's plus `at`, drawn as a piece's frame
  (`map::cel::cel`, through `art::Sprites`), so its glow indices go through
  the shadow pass and lighten what is under it
  (`the_rings_anchor_lands_at_its_offset_from_the_pieces`). Depth: just
  behind its piece, so the body covers it; hidden while held. The piece's
  shadow, drawn after the ring, darkens its glow: **Game**, the original
  shows the ring under the body, combined with the shadow. **Inferred**: the
  engine blits the ring, then the piece, body and shadow at once.
- The hover ring (`cursor::Hover`, `ring::HOVER`, `AVn8`): under the piece
  the pick takes, if `ring::highlights` takes it, in the select mode or a
  unit spell's mode when the spell takes it; not under a selected piece.
  Placed as the selection ring
  (`the_hover_ring_takes_every_walker_and_two_buildings`). **Inferred**:
  none while the cursor is off the map, where the engine keeps its last
  target.
- Side ring (`[enhancements] side_ring`, on by default, `rings::sided`):
  the hover ring under a player's piece (owner 0 to 7) takes its side
  colour, the bar's (`rings::side_colour`). A standing building that
  `ring::highlights` skips (all but the shrines) takes it too, in the
  select mode (`cursor::SideHover`, `Picked::side_only`), fitted as its
  selection ring, outside the pick's box
  (`the_side_ring_uses_the_click_hitbox_for_buildings`). One ring, tinted:
  native and modern rings tint their band; the `art` tile takes no tint, so
  a thin line replaces it (`regent_draw::ring::side`). Independent of
  `[ui] bars`: under `never` it is the only side mark. Off, the hover ring
  is the art's.
- Native rings (`[enhancements] rings = "native"`, `rings`): each ring but
  the flag's brackets drawn as a band (`view::map::ring::band`,
  `regent_draw::ring::Ellipse`) in place of its tile: same piece, look,
  slot, anchor and depth. The band is fitted to the slot's tile (least
  squares ellipse, width from its pixel count, a fall-off a look). The
  white ring multiplies colour as the art's glow, then screens it by
  `LIFT` (0.04, so it shows on dark ground), a departure from the art's
  look; green and red blend their colour, lifting the soft shadow they
  cover. The outer edge is smoothed over a screen pixel (`fwidth`), so it
  stays sharp at every zoom and upscale. The build preview's ring too.
  Changing the setting remakes the rings.
- Modern rings (`rings = "modern"`, `regent_draw::ring::modern`, the
  default; `the_rings_are_modern_by_default_and_read`): the same band,
  piece, slot and depth as native, drawn as a 2 screen pixel line on its
  middle (`LINE_WIDTH`) with a glow inside, alpha 0.45 at the line
  (`GLOW_STRENGTH`) falling off by a smoothstep over 2 half widths
  (`GLOW_REACH`); white for the selection, the art's green and red
  brightened by 1.25 (`BRIGHTEN`), blended as native's.
- The cursor (`cursor`, [art.md](../formats/art.md#the-cursor-is-cur1)): the
  game's `CUR1` picture as the window's hardware cursor, its hotspot the
  tile anchor; bevy_egui's cursor updates off. Off the map, the `Dial`
  hand. A mode's amount writes beside it in egui's font. Nothing under a
  hardware cursor can be darkened: a shadow index draws black at the share
  its soft factor takes off (`soft::veil`, blended on encoded bytes,
  **Inferred**). **Inferred**: the engine's screen pixels count from the
  map's corner, openregent's world from its middle, so a box at zoom 0.5
  may sit a pixel apart.

## Fog and shadows

- Fog (`fog`, `view::map::fog`, the veil's drawing `regent_draw::veil`,
  **Binary**): ground with a corner player 0 has not explored lies under a
  black veil, its alpha Gouraud from the corners over the cell's cut, as
  the engine draws such a vertex at shade 0.001. One 16-cell chunk is a
  mesh, redrawn as its vertices are revealed.
- A piece, effect or scatter frame at seen level 0 (`World::seen_level`:
  25 a corner of its cell, a building's by its rect, its owner's 100) is
  hidden and cannot be clicked; below 100 it draws its palette darkened
  (`fog::palette`, `fog::darkened`; `a_units_seen_level_picks_its_palette`),
  a tinted piece its tint's. Ground never fogs again.
- **Inferred**: the veil scales the drawn ground where the engine scales
  the shade; scatter goes by its cell; missiles and particles are hidden
  by theirs, not darkened.
- L lifts the fog, with `debug_keys`.
- A selected piece gone unseen keeps the selection and its panel; its ring
  hides with it (the original draws the ring in the unit's draw). Only the
  tracking window's follow step drops a selection unseen
  (`an_unseen_piece_holds_the_view_and_asks_a_deselect`).
- A speed-trail clone is seen as its unit, the root of its holder chain.
- Shadows (`[enhancements] shadows`): `soft` by default, no stacking;
  `blend` stacks in draw order as the engine's do (**Binary**,
  `view::raster::stack`). See
  [A shadow needs a destination](shadows.md#a-shadow-needs-a-destination).

## Keys, menus and chat

- The map's own keys (`map_keys`, [panels.md](../formats/panels.md#hot-keys)):
  Backspace, `` ` ``, `<` `>`, Delete, the arrows, the spell keys 0 to 9
  and focus keys F2 to F8 (**Binary**).
  Digits 1 to 3 stay the speed keys with `speed_keys`, 0 the zoom reset
  with smooth zoom. Alt held shows every hit point bar under any `[ui]
  bars` (`hp_bars::Reveal`, openregent's own;
  `the_bar_key_shows_every_bar_while_held`).
- Hot keys (`hotkey`, [panels.md](../formats/panels.md#hot-keys)): a key is
  a click on the first drawn, enabled panel control whose `SMNU` key it is,
  so the same `Input`, recorded and replayed
  (`the_first_enabled_control_takes_the_key_once`). R recruits at a guild
  (`r_recruits_at_a_guild`); every key the panels drawn through `sheet`
  carry works (X, ?, U, H, V, S, the special powers, Y and N, Escape for
  OPTIONS). A claimed key is the panel's: M, K, L and Escape act only where
  no drawn control has them; Escape stays the cancel while a mode is on.
  Not while the chat line is open, nor on the front end. Under a menu, or
  once the game has ended, only the menu's controls take keys
  (**Inferred**: the original sends a key to the open dialog): `AP36`'s
  Escape is RESUME PLAY, not `APMK`'s OPTIONS
  (`escape_opens_options_and_the_next_escape_resumes`).
- Mute: M; kept as `[audio] muted`.
- Full screen: `[ui] window`: `windowed` (default), `full_screen`
  (borderless, the screen's size) or `exclusive` (the screen's video mode;
  Bevy falls back to borderless where the system has none, as Wayland),
  applied as it changes (`fullscreen`; `the_window_takes_the_setting`).
  Alt+Enter flips windowed and full screen and writes the file: in the
  menus, also while a text box has the keyboard, and in a game where its
  keys are live. Enter with Alt opens no chat (`alt_enter_is_no_enter_key`).
  HD's own is MajXPrefs `Windowed`, full screen by default; openregent opens
  windowed.
- Menus over the game (`options`,
  [panels.md](../formats/panels.md#the-main-screen)), in the original's
  panels, centred at the interface's scale:
  - OPTIONS opens `AP36`; the title scroll `AP88`; the goals (a quest's
    `<Short>`, a custom game's `Maj.qdd` block;
    `a_custom_games_goals_are_its_conditions_block`).
  - Under one the game pauses and the interface takes no press; closing the
    last gives back the pause the player had
    (`a_menu_pauses_and_gives_back_the_pause_it_found`). The original
    resumes unpaused.
  - RESUME PLAY or Escape, and `AP88`'s RESUME, close. `AP88`'s OPTIONS
    goes to `AP36`. ADJUST LEVELS opens the settings editor, back to `AP36`
    as it closes. GAME INFO opens `AP88`, as the scroll does.
  - RETURN TO MAIN MENU asks `APc8`, QUIT MAJESTY `AP04`; YES goes to the
    main menu (as the end panel's Main Menu) or out; NO back.
  - SAVE GAME and LOAD GAME open the save dialogs
    ([Saves](saves.md#saves)): CANCEL goes back to `AP36`; a save closes the
    menus, as the original opens nothing after it; a load makes the loaded
    save's game.
  - RESTART asks `APd3` (`game_info_shows_the_goals_and_restart_asks_first`);
    YES takes the game down as RETURN TO MAIN MENU does and opens the menus
    on its start page (`Flows::restart`): a quest's summary `AP44`, whose
    Start Quest makes it again on a new map word and Cancel goes to the map;
    a freestyle game's pick page `APd2`, a new Map Number rolled. A command
    line game too. The new game records a new `play.input`; victories are
    untouched. A freestyle restart keeps the session's config, where the
    original takes the running game's.
  - The menus' Start Quest reads its map word off the clock
    (`freestyle::clock`, as the original), so each start is a new map; the
    GPL seed stays `--seed`'s default.
  - `AP36` spaces "QUIT      MAJESTY" and "RESUME    PLAY"; openregent draws
    them on one line with one space (`options::spaced`;
    `a_spaced_label_is_one_line`).
- Chat (`chat`, placeholder): Enter opens a line, Enter sends, Escape
  cancels; while open the game sees no key. A cheat phrase sends
  `Input::Cheat` and shows its reply ([cheats.md](../original/cheats.md));
  in a base quest an empty line is `PLANET FARGO`, as in the engine.
  - Sent lines and the world's game messages to the crown type into a
    four-row box at the view's bottom, as the engine's chat area does
    ([panels.md](../formats/panels.md#game-messages);
    `the_box_holds_four_rows_of_its_font`). egui's font stands in for
    `fnt4`, at the interface's scale, shadowed. The box stands above the
    open spell bar (`the_box_stands_above_an_open_spell_bar`).
  - `[enhancements] message_links`: a line about a unit (`Said::from`) shows
    its portrait, a grave's for a dead unit; a click selects it if present
    and centres the view, else centres on its last place. The hover shade
    covers the row's text.
  - The box types and scrolls on the wall clock, as the engine's
    (**Binary**), so lines read at any speed. A departure: its wall time
    stops while the game is paused (`ui::Paused`, the options too), so
    lines wait out a pause where the engine's scroll on
    (`a_real_clock_line_waits_out_a_pause`). `--message-clock game`
    (`chat::Clock`; a flag, never recorded) puts it on the simulation's
    clock, for replays and tests: a slower game keeps lines longer; paused,
    they stay and typing waits (`a_game_clock_line_expires_on_game_time_alone`).
    Display only.

## The end

`end`, `regent_core::view::menu::end`,
[panels.md](../formats/panels.md#the-games-end).

- Once `World::ended(CROWN)` is set the game stands still, as state 8.
  Music and sounds stop and the advisor speaks.
- A won game shows `AP11` "Game Over", "You are victorious.", OK; then the
  view goes to the victory point and waits `#EndGameFadeDelay`. A lost game
  goes on at once (`a_victory_waits_for_ok_then_fades_after_the_delay`,
  `with_no_point_a_victory_fades_at_once`).
- The fade, 15 steps of 75 real ms, darkens the column and bars to half,
  not the map (**Game**; `the_panel_waits_for_fifteen_steps_and_the_advisor`).
- With the advisor done, `AP60` or `AP61`: pog, the quest's title or
  `GMTX` 80, "YOU HAVE WON!" or "YOU LOST!", the verdict
  (`view::menu::end::verdict`: `QEND` or `LOSX` for a map slot's quest,
  else the quest's or a mod's strings, else `FRE0`) centred in its box,
  scrolled where longer; a custom game's map number.
- STATISTICS opens `AP84` or `AP83`: `World::statistics` in its controls,
  no way back (`the_statistics_fill_eighteen_controls`).
- MAIN MENU is `Flows::back`, RESTART `Flows::restart`, CONTINUE PLAYING
  `World::play_on`, recorded: the game runs again, and a later end comes
  again (`a_won_game_goes_on_and_a_lost_one_restarts`).
- With every expansion quest won, MAIN MENU, RESTART and CONTINUE PLAYING
  on `AP60`/`AP61` of a lone game play the owed film first, over the game,
  music hushed, then go on; MAIN MENU and CONTINUE PLAYING mark it seen
  (`end::owed`, `Panel::films`;
  `the_expansions_ending_comes_once_from_the_verdicts_presses`). HD has no
  `MV07` there.
- Hot keys as the panels' (S, C, R, M); no Escape. Drawn in the panels' own
  art through the art service, centred at the interface's scale, as `AP36`
  (**Game**: 800 × 600 at 1 : 1 in the 1680 × 1050 captures); text in
  egui's font. The bar is greyed and the chat line shut; `Sim::input` drops
  any order (**Inferred**: state 8's screens send none).
- The stopped track starts again at the next poll (`audio::encore`). As the
  panel opens a won quest says its `ED`nn voice (`audio::farewell`,
  `menu::end_voice`); any press stops it.
- Not drawn: the victory star.

## Settings

`$XDG_CONFIG_HOME/openregent/config.toml`, written with the defaults when
absent (`config`). Every key, its default and its range are in the
settings editor, whose text comes from the keys' doc comments
(`the_docs_come_from_the_doc_comments`). Sections: `[ui]`, `[audio]`,
`[game]`, `[enhancements]`, `[paths]`.

- Read key by key (`config::lenient`): an unknown key or section is
  ignored (`an_unknown_key_or_section_is_ignored_and_kept_by_a_save`); a
  value of the wrong type, out of `config::RANGES`, or an enum name no
  longer taken gives that key its default and a warning
  (`an_invalid_value_takes_its_default_alone`,
  `a_value_out_of_range_takes_its_default`). A file that is not TOML is an
  error.
- A save (`toml_edit`) writes a value only where it differs from the file's
  reading, or is missing: comments, order, unknown keys and an invalid value
  stay, the last until its setting changes
  (`a_save_keeps_the_files_comments`,
  `a_save_keeps_an_invalid_value_until_changed`).
- `[paths] majesty_hd` is read apart (`config::hd_at`): a comment in a new
  file, never written as an active value, and not in the settings editor,
  where a bad value would stop the next start
  (`a_save_keeps_the_install_path`, `the_install_path_is_not_listed`).
- The mod list has its own file, `$XDG_CONFIG_HOME/openregent/mods.toml`,
  written by the mods page alone (`mod_list`;
  `the_mod_list_has_its_own_file`).
- The wind's trees: `$XDG_CONFIG_HOME/openregent/wind.toml`, read at start,
  never written, no in-game editor (`config::trees`). It is laid over the
  shipped `regent_draw/data/wind.toml` (`regent_draw::wind::Trees`; its
  header explains each field): a key replaces that key, a `[trees]` entry
  that tree's whole entry. Not TOML, or a bad value: a warning, the default
  kept.

## Sound

`audio`, [sound.md](../original/sound.md).

- Plays the world's cues, the selection's sound, each piece's ambient loop
  while in range, a refused placing click's `Error_Alert`
  (`audio::Interface`), and the scripts' music; a track's end runs their
  callback between frames.
- Mixed as the engine mixes, about the view's centre, stereo by its pan
  (`the_sound_view_radius_is_the_binarys`). Waves play as decoded PCM, kept
  16-bit and scaled to `f32` as played. A placed sound plays at the mix's
  volume alone, an unplaced one at its set volume and spread
  (`a_placed_sound_keeps_neither_set_volume_nor_spread`).
- An unseen unit's sound plays only at 10,000, its loop stopped
  (`World::sight_word`), by the game's sight, not the L view.
- The advisor's timed lines (low treasury, no hero, elves and dwarves
  available) are world cues (`World::with_advisors`).
- **Inferred**: heights are 0, a sound follows its unit, 16 effects at most.
- `--headless` counts the cues.

## Headless counts and the log

- A builtin the world does not answer takes its type's default, counted per
  builtin; one it cannot answer here (`Answer::Unknown`) too. **Inferred**:
  a stand-in, not the engine's answer.
- `--headless` prints both counts, the pieces, action frames, missiles
  flown, buildings fallen and in ruins, effects drawn by code, units
  tinted, speed-trail clones, places lost, orders declined, the script
  errors' count, the longest run in records and its function
  (`Vm::longest`: how near play comes to the 1,000,000-record `Unending`
  guard), and the end's frame, won or lost, where it stops
  (`the_headless_line_says_won_or_lost`).
- Each script error goes to the log as it comes, with its frame; past the
  eight kept whole, a count.
- What the world loses track of goes to the log as it comes, with its frame
  (`world::loss_log`, `World::take_lost`): a piece whose place becomes
  unknown, the order not followed and why (one unknown place stops every
  later walk); an order to a piece already lost; a builtin's first unknown
  call, its agent and calling function. Once per order and reason, or per
  builtin; every 1,200 frames the counts since.

**App**: `--headless` runs no Bevy. `draw::base` builds the drawn app;
`Display::Headless` drops winit, the GPU (`backends: None`) and the sound's
loudness, and keeps every system. Its test steps a new game's app 20 frames
on Bevy's multi-threaded executor, so a system or run condition that cannot
run off the main thread panics there (`the_app_runs_headless_for_some_frames`).

## Headless runs

**Game**: the new game from the start trace's picks is the start save's
world piece for piece, and after 1,242 frames from seed `0x0b609e2c` its
state is the quiet day's save's `GPL ` chunk (`regent_core/tests/fresh_generated.rs`).
Headless, with no answer unknown and no script error, the raiders raze the
kingdom and `DeclareLoss` runs at frame 7,618 (day 6, 20.9 s; 380,900 ms);
the game then stands still, as the engine's does
([vm.md](../gpl/vm.md#a-game)).

Other corpus setups, 6,000 frames each (`--headless --frames 6000`):

| Picks | Start units | Pieces | Unknown |
|---|---|---|---|
| Wizb Ruib Rave, map 5 | 6 | 17 | 0 |
| Cham Gobf East, map 470427919 | 49 | 56 | 0 |
| Wizb Cona Mayh, map 1 | 33 | 40 | 0 |
| Wizb Shad East, map 1 | 17 | 24 | 0 |
| Wizb Skea East, map 1 | 44 | 59 | 0 |
| Fron dliW Thol, map 4 | 14 | 25 | 0 |
| Wizb Ruib KxHa, map 1, `--monsters Drac` | 8 | 14 | 0 |

The Drac game is lost at frame 5,827. The world answers lairs' picks,
off-map edges and `UnitIsOffmapLair`, so no walker is lost. `LearnSpell`
lets Cona's war party cast `Speed_Monster`: on East Cheswick, map 1,
trail clones by frame 6,000, which `Speed_Monster_End` deletes. No script
error anywhere.

Bot runs (`--bot`, those setups, 30,000 to 40,000 frames, some with `--gold
150000`): the longest run stays far under the 1,000,000-record guard, so it
trips only on a true loop. `Release_Guards` (line 115) can free the
cursor's node with `-=` when a guard is listed twice in `Waiting_Guards`,
the scripts' doing; the engine then reads a freed node, and so does the VM
([vm.md](../gpl/vm.md#what-the-loop-does-read-again)), with two `IsHidden`
throws at each such meeting.

**Game**, quests (`regent_core/tests/quest_start.rs`): HD's four corpus
quest starts (two Day of Reckoning, the Siege and Urban Renewal), on their
clock words and each save's GPL seed, run to the save's clock: every
building, scattered object and height is the save's. Every quest runs 600
frames with no script error. Run 6,000 frames, all 34
(`every_quest_the_install_offers_comes_in_its_dependency_order`) lose no place and
leave nothing unknown. Every builtin they call is answered
(`every_builtin_has_an_answer`). Engine rules this relies on:
`#TeleportInsideDestination` resolves (`host::NAMED`), so the Fortress of
Ixmil and Balance of Twilight's sage enter at once; a plant's `SpawnUnit`
searches with its own test; `clear` places as without; a unit drawing no
footprint set measures as a point (`fire_ball`). Not reproduced: Urban
Renewal's start.

Gaps: the walk cases [movement.md](../original/movement.md) marks not
modelled apply. Cell variants are invented; 15,781 of the start save's
16,384 cells' classes come out
(**Measured**).
