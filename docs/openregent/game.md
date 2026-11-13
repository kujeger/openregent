# The game

`openregent` runs a game, takes the player's input and draws it. The
simulation is the library's. A new game's map comes from
`sim::mapgen::fresh` (the custom game's picks and HD's data); the library
places the start and founds the world on it (`sim::start`,
`setup::fresh`). `sim::game::Game::free` runs it.
The crate steps the game and draws it.

```sh
cargo run --release -p openregent                        # a new game, the start trace's picks
cargo run --release -p openregent -- --map-seed 7 --scenario Shad --land East
cargo run --release -p openregent -- --headless --frames 72000
```

### A new game

With no `--quest` or `--scene` the game is new. These options are the
custom game dialog's picks:

| Option | Pick |
|---|---|
| `--land`, `--settlement`, `--scenario`, `--monsters` | the forces and landscape |
| `--map-seed` | the Map Number |
| `--side`, `--gold` | the player's side (default: the scenario's) and gold |
| `--victory` | eliminate, gold, survive, last-palace (alone, eliminate) |
| `--goal` | gold or days; unset, the dialog's (`Victory::goal`) |
| `--event1`, `--event2` | `EVSC` keys |
| `--limits CODE=N,...` | building caps, as Building Limitations (`World::limited`) |

Unpicked options take the start trace's values (`custom::DEFAULTS`: Wizb,
Ruib, NONE, KxHa, map 1, no event; `no_option_is_the_default_setup`). A
Random pick (`BEGI`, `ADVA`, `EXPE` for a force, `RAND` for the land) is
drawn from the Map Number (`sim::freestyle::random`). The setup keeps the
tag, so the title scroll says "Random (Beginner)". A code option given
alone (`openregent --land`) lists the dialog's offers with their names,
the default marked (`freestyle::offers`); an unknown code is an error
listing them.

### On the map

The opening view centres on the player's palace, else its outpost
(`World::player_palace`; `a_pipelined_game_opens_on_its_palace_not_the_whole_map`).

A flier's ring, and the ring's part of its pick box, sit on the ground
under it, at its shadow (`map::pieces::Under`).

Hit point bars (`view::map::bar`, `hp_bars`;
[art.md](../formats/art.md#the-hit-point-bar-hangs-off-the-body)):

- `[ui] bars = "damaged"` (default): a unit with `HasHPBar` below its most
  hit points, or a building under construction, shows its bar at its body.
- `always` is the original's rule: the bar shows at full health too
  (**Binary**; `only_an_owned_living_unit_with_a_bar_shows_one`).
- `never` hides them, as the original's `DisplayFlagStats` off.
- Holding Alt shows them all.
- Bars draw over every piece (**Inferred**: the engine's pass order is
  unread); the bevel shade is translucent black
  (`the_bevel_keeps_sixty_eight_percent`).

In-world text ([Text and shapes](#text-and-shapes)), drawn at view zoom
1.0 alone, with a drop shadow:

- A reward flag shows its reward over it, remade when raised (`labels`,
  `view::map::label`;
  [art.md](../formats/art.md#a-reward-flags-reward-hangs-over-it);
  `the_reward_is_its_number_and_the_shadow_keeps_68_percent`).
- `[ui] names`: a unit's name hangs 12 pixels under its bar's rect,
  centred: a building's name or title, anyone else's name and level
  (`label::name`; [art.md](../formats/art.md#a-units-name-hangs-under-its-bar);
  `a_name_sits_centred_twelve_under_its_rect`). `on` (default) is the
  original's `DisplayFlagNames`; `heroes` shows heroes only (an
  openregent addition); `off` none (`each_names_setting_takes_its_units`).
- A text is remade when the reward, level or descriptor changes
  (**Inferred** for an upgrade: the engine remakes only on `APV:11`).

A selected flag rings in `AVn2`'s brackets (`ring::Look::of`). The
library's `fnt7` label and the brackets match a capture of a selected flag pixel
for pixel (`tests/label.rs`).

### Paths and session options

The HD install is the first of (`env::find`; `each_place_wins_in_order`):

1. `MAJESTY_HD` in the environment;
2. `[paths] majesty_hd` in the settings file (`~/` expanded, relative to
   the file's folder);
3. `$XDG_DATA_HOME/openregent/data` (beside the mods folder);
4. the current folder, then the program's.

The last three count only if they hold the two dataset definitions and
`Quests/Maj.qdd`; a data folder holding other files is warned of and
passed over (`an_xdg_folder_without_an_install_passes_on`). A set value
that is no install is an error naming its source, path and missing files
(`a_set_value_that_is_no_install_is_an_error`). None found is an error
naming every place (`no_install_anywhere_names_every_place`). The install
used is logged at info and printed. The SDK is the
install's `SDK`. `.env` is read by tests only.

- `--seed`: the GPL session word; default the quiet day's, `0x0b609e2c`.

### Quests

`--quest <key or name>` starts a quest (`QU06`, `day_of_reckoning`).
Alone it lists HD's 34 offers (`content::quest::offered`): every `.mqxml`
but the two freestyle ones, in `QDEP`'s order (**Inferred**: the menu's
order is unread), each titled by `STRT QUES` (`load::quests`).

- Dataset: the base game's archives and two images, or the expansion's,
  then the definition's own (`content::dataset::Install::quest`).
- Map: its `.q`'s
  ([maps.md](../formats/maps.md#a-quest-is-generated-from-its-own-q-on-the-clock),
  `sim::mapgen::quest`), on `--map-seed`, else `--seed` (**Inferred**:
  both stand in for clock words).
- Players and gold: its slots'. Setup: the GPL function its `<Name>`
  names.
- It starts unpaused, with no briefing over it. The title scroll and
  Options' Game Info show the title and goals (`AP88`). The end panel
  names the quest and keys its verdict by it.
- `play.input`'s setup holds `quest` and `map_seed`
  (`a_quest_records_its_name_and_map_word_and_no_picks`).

### Scenes

`--scene <file.toml>` starts a hand-built world for tests and shots
(`sim::scene`). Example: `assets/scenes/palace_shadow.toml`.

- `map = { flat = { side = 128 } }`: height 0, all `GR01`, no scatter.
- `map = { freestyle = { land = "KxHa", map_number = 3 } }`: the custom
  game's map; the start's units kept unless `chart = "cleared"`.
- Each `[[unit]]`: descriptor `code`, cell `at = [x, y]`, `owner`,
  `facing` (art slot), `attributes` by engine name (`data::attrib`:
  `HP`, `Gold`).
- `players`, `gold`, `victory` (default `survive`), `goal`, `day`,
  `second`.

A placed unit is a start unit (`world::new_map::Raised`), made as the
unit chart makes one, exactly where asked, so the setup's unit pass,
`NewUnitInit` and `Palace_Pre_Birth` run on it. Attributes are written
after the setup. The setup runs at the scene's clock (`Game::setup_from`;
**Inferred**: the engine always starts at 0). `play.input`'s setup holds
`scene` (its path) and `seed`, so a replay reads the file again
(`a_scene_records_its_path_and_no_picks`).

### The menus

With no option but `--speed` or `--message-clock`, the window opens on
the menus (`menu`; `the_menus_open_only_without_setup_or_mode_options`),
drawn from the decoded panels in the original's pictures, buttons
hit-tested at their rectangles. Text is egui's, not the original's (a
departure, for sharpness; [Text and shapes](#text-and-shapes)).
Filtering is bilinear.

**Scale.** One interface scale everywhere: window height / 848
(`layout::scale`, `layout::Frame`;
`the_menus_and_the_game_share_one_scale`). This is a departure: the
original draws its menus at their own size and its game at HD's.

- A page is drawn at that scale, centred, the sepia faces tiled around
  it. A page too wide for the window shrinks to fit (`menu::fit::scale`;
  `a_page_too_wide_for_the_window_fits_its_width`).
- Screen pages fill the window, uniformly scaled (`Fit::of`,
  `paint::Canvas::filling`): the main menu `AP29`, a quest's background
  `APc9` and summary `AP44`, the setup page `MX17`, Multiplayer Setup
  `AP33`, the lobby `MX19`. The user chose this; their buttons grow past
  the interface scale on a large window.
- Dialogs keep the scale: the pick `APd2`, message `AP11`, name prompt
  `AP35`, session `AP55`
  (`a_dialog_is_centred_at_the_scale_of_the_page_behind`). The quest map
  spreads its frame instead.
- egui's own pages (settings, profiles, saves, extras) take egui's
  default text and spacing times the scale (`menu::fit::styled`).

**Main menu.** `AP29`, base or expansion by `[ui] main_menu` (default
base); the version line is openregent's. Its room objects move
(`menu::room`, [panels.md](../formats/panels.md#the-main-menus-room)): a
hovered button, object or (expansion) trigger plays the object's lit run
by its `IMAG` playback; letting go plays it back; a click on an object or
trigger is its button's. Base art follows HD's pair rule (a hovered
object also draws its button light blue), expansion art the trigger rule.
The cursor is `CUR1`'s `Dial`, busy while a game is made.

**Intro and films.** Unless `[ui] intro = "off"`, a page or game is asked
for, or a screenshot runs, the intro plays `MV02` then `MV04` full screen,
each skipped by a key or click, the music after
(`Page::Intro`; `the_intro_plays_mv02_then_mv04_each_skipped_by_a_press`).
The `SPLH` still after them is not shown. Cinematics lists the films the
profile's victories unlock (`Page::Cinematics`, `menu::films`,
`Progress::films`; `APd8`'s panel in other words). A pick stops the music
and plays the film (`Page::Film`), then returns to the list and the main
menu's track (`cinematics_lists_its_films_each_playing_back_to_the_list`,
`the_intro_and_a_film_stop_the_music`). Credits plays its two videos full
screen, shape kept, cursor hidden, each until it ends or a key or click
(`menu::cinema`; `credits_play_both_videos_each_stopped_by_a_press`). A
video that does not open shows a note. The music plays on, as the
original makes no call.

**Video.** Films and credits read the cinematics' `.dat` archives on the
video's worker (`video::VideoSource::Archive`;
`each_video_is_read_from_its_archive`). A worker thread decodes two
frames ahead to RGBA (`video::Playing`, the `bink` crate); a frame never
waits on it. The clock starts at the first frame shown (`video::Clock`;
`the_clock_waits_for_the_first_frame_then_runs_at_the_videos_rate`). The
last frame is never shown (`menu::shown_frames`). Sound goes through
Bevy's audio from the first frame shown, a packet decoded at a time on
the audio thread, at the music's loudness (`video::Sound`; **Inferred**:
the original's Bink sound follows no game setting). Upscaling: see
[`video_upscale`](#upscaling).

**High Scores** opens `AP64` in its art (`menu::scores`,
[panels.md](../formats/panels.md#the-high-scores)). The table is read
from `scores.toml` in the data directory as it opens and written as it
closes (`a_kept_table_comes_back_in_its_order`). A won quest or custom
game is recorded there at its end (`end::record`;
`a_won_game_records_every_category_it_reaches`), never from a screenshot.

### The settings editor

Adjust Settings (or its room object) opens openregent's settings page
(`editor`, drawn by `menu::settings`), not `AP37`
(`adjust_settings_opens_the_editor_which_closes_to_the_main_menu`). It
uses the Build Tree Editor's art (`MX18`), centred at the interface scale
over the main menu, whose room objects do not light under it. It is as
wide as its rows need (`the_page_is_as_wide_as_its_rows_or_buttons_need`):
the frame's picture cut in bands, its right side drawn in, the bottom
bar's slots joined (`settings::BANDS`).

- Two tabs, SETTINGS and ENHANCEMENTS (`settings::Tab`;
  `every_key_shows_on_exactly_one_tab`), drawn in SET ALL's picture cut
  to fit (end caps, middle repeated: `paint::slices`). The shown tab is
  lit, its text the normal colour (SET ALL's lit white does not read
  there).
- SETTINGS groups its keys under GAME, SOUND, DISPLAY (`window`) and
  INTERFACE (`settings::Group::setting`; `the_settings_are_grouped`).
- ENHANCEMENTS opens with the preset list (Modern, Classic, Custom), then
  keys under DISPLAY, GAME and INTERFACE (`settings::GROUPED`), then the
  Experimental row: `MX18`'s + or - folds its keys, its check box
  switches them, and off they show disabled
  (`the_enhancements_are_grouped_under_the_preset`,
  `the_switch_decides_whether_its_keys_are_live`).
- 25 px rows, so the setup page's combo box fits; every other key of a
  section on a translucent dark band
  (`every_other_key_of_a_section_is_shaded`); a scroll bar and the wheel.
- Under the list: the hovered key's name (else the tab's first) with
  RESET, then its doc and default in a box
  (`the_hint_gives_the_default_and_range`). Docs come from `config.rs`'s
  source.
- The keys are the file's as written, so a new key shows unasked. A bool
  is `MX18`'s check box; a number its + and - (a twentieth of its range,
  `config::RANGES`) and value box, dragged across the range; a string
  `MX17`'s combo box when the settings refuse unknown names (an enum:
  serde's error lists the names), else a text field.
- An edit applies at once; every key is read each frame. SAVE writes the
  file (a refusal shows red in the doc box); CANCEL or Escape restores the
  settings as opened; DEFAULTS resets every key. Buttons use SET ALL's
  picture.

In game, the Options menu's ADJUST LEVELS opens the same page, same size,
over the paused game.

### Profiles

Every game, from the menus or the command line, plays as the last used
profile, asking nothing (`Profiles::current`;
`the_current_profile_is_the_last_else_the_first_else_a_new_one`): with
none, the first listed, else a new one named as the original's prompt
offers (`GMTX` 186 "Your Majesty"), made silently. A screenshot reads the
last and makes none. A start that reaches the main menu with no profile
asks once (`Picker::start`; `a_first_start_asks_once_and_makes_nothing_itself`):
the name prompt opens offering that name; if cancelled, the first game
makes it silently.

The main menu's Profile button (placeholder egui, top right, naming the
profile; the original has none and asks a name, `AP35`, at Play Game)
opens the profile page (`menu::profiles`;
`the_profile_page_opens_from_the_main_menu_and_goes_back_to_it`): pick one
(the last used chosen), name a new one, or delete one and its saves
(asked first). The chosen one becomes the last used. Continue or Cancel
goes back. The list is egui (the original has none). A new name is asked
in `AP35`'s art (`paint::prompt`), centred over the main menu at its
scale: heading, an egui text edit in field 5002 at `fnt2`'s size,
centred, at most 21 characters, the name selected as it opens (**Game**:
a capture). ACCEPT or Enter makes it; CANCEL or Escape goes
back to the list (with none, the main menu). Why a name is refused shows
under it.

A profile is openregent's own (`profile`): a directory per name, made
safe (letters, digits, space, `-`, `_` kept, the rest `_`;
`a_name_becomes_a_safe_directory`). An empty or too long name, one of
stand-ins only, or one another profile holds in any case is refused
(`a_name_another_profile_holds_in_any_case_is_refused`). Each file is TOML
with a `version`; another version is refused and left alone
(`a_file_of_another_version_is_refused_and_kept`):

```text
$XDG_DATA_HOME/openregent/profiles.toml           version, last = "<dir>"
$XDG_DATA_HOME/openregent/iroh.toml               version, secret: the iroh key (`net::iroh`)
$XDG_DATA_HOME/openregent/profiles/<dir>/profile.toml    version, name
$XDG_DATA_HOME/openregent/profiles/<dir>/progress.toml   version, last_won,
    [seen] advisor, expert_hint, [quests.QU01] victories, best_ms;
    a mod's quest, keyed by a CRC of its id, as [quests."{GUID}"]
$XDG_DATA_HOME/openregent/profiles/<dir>/saves/<name>.tar.zst     a save
$XDG_DATA_HOME/openregent/profiles/<dir>/mp-saves/<name>.tar.zst  a LAN save
```

Play Game goes to the quest map. Load Game (or the chest) opens the
profile's saves (`AP27` over the main menu; [Saves](saves.md#saves);
`load_game_lists_the_profiles_saves`); a load shows its game's loading
page.

### Mods

The main menu's Mods button (placeholder egui, top left; HD's control
9000) opens the mods page (`menu::mods`, in place of HD's `APdc`;
`the_mods_page_opens_from_the_main_menu_and_goes_back_to_it`).

- A row per mod found, in load order: its number while enabled
  (`only_enabled_mods_are_numbered`), an enabled box, its name and
  `<Short>` cut to one line, "Never loads" for a base no dataset fits,
  "Built in" for a mod the binary carries
  ([mods.md](../formats/mods.md#built-in-mods)), Up and Down.
- A clicked row shows details below: `<Long>` (else the whole `<Short>`),
  the games its base fits in words (`a_base_is_said_as_the_games_it_fits`),
  its item folder's quests, what it lists ("1 GPL, 13 XML descriptions"),
  GUID and item folder ("Built in, unpacked to …").
- A footer counts the folders searched
  ([mods.md](../formats/mods.md#where-mods-are-found)), GUIDs skipped as
  found twice, and entries no folder holds (red if one is enabled); each
  list shows on hover.
- The page is the window less a margin, at most 900 × 720 HD pixels
  scaled; the list scrolls.
- Load mods, a box above the list, turns mod loading off: the list and
  LOAD greyed and kept.
- DONE writes `mods.toml`'s `[[mod]]` entries and `mods_enabled` if
  changed (`mod_list`, `store::replace`;
  `the_mod_list_has_its_own_file`): each found mod's `id`, `name`,
  `description`, `enabled`, in order, then entries no folder holds, as
  written. CANCEL or Escape drops the edit.
- LOAD… picks a collection (`mod-lists/<name>.toml`) into the page, not
  applied; its Delete asks first. SAVE… names one; a taken name asks
  first. `[mods] enabled` is not read and is kept as an unknown key.

A game started after loads the enabled mods (none with loading off); the
menus' data is not reloaded. Mods are found once a run. `--menu
mods:GUID,…` opens the page with those enabled; `off` among them turns
loading off.

A game's corpus takes its quest's and mods' XML
(`regent_draw::data::Sources`): descriptions merged into one archive
loaded last, strings into the text ([mods.md](../formats/mods.md#xml)).

The extras list (the map's jewel, `APd8`) lists the mod folders' quests
with `MD01` and `VAMP`, by title, each naming its item's first mod (an
enhancement, [enhancements.md](enhancements.md)). A pick's line is as
`APd8` gives it (`quest_map::open_line`): its `<Difficulty>`, and once
won its victories and best time.

### Message dialog

`AP11` (`paint::message`) is the base game's art over the map, centred at
the frame's scale; the original draws both at their own size, so it stays
small beside a zoomed map. Title in 8005; text wrapped and centred both
ways in 5000 (**Game**: a capture). It shows the buttons
the dialog asks for (`menu::message::shown`;
`the_message_shows_its_pages_buttons`), pressed by click or hot key
(`hotkey::pick`; `a_messages_keys_press_only_its_shown_buttons`). The map
behind takes no pointer. It is not dimmed, as in the capture.

### The quest map

`APdb` from `UIData_1920_1200.dat`, opening centred under the frame
(`the_map_opens_centred`).

**Frame.** Spread to the window's edges, a departure (`menu::fit::spread`;
`the_frame_spreads_to_the_windows_edges`): the original keeps 1398 × 1200
with the faces beside it. The frame is drawn at the interface scale, so
it is window size / scale layout pixels (1357 × 848 at 1920 × 1200),
rounded up to cover the window; at least `quest_map::LEAST_WIDTH` 891
wide (the bottom's middle pieces clear its corners), the scale less on a
narrower window. Pieces move by the shipped layouts' own anchors
(`quest_map::anchor`; `every_shipped_frame_is_the_largest_moved_by_its_anchors`),
bars repeating (`a_frame_of_any_width_has_no_gaps`).

**Zoom and pan,** a departure too (`menu::fit::Lens`): the original shows
the map at its own size, scrolled by wheel and edges.

- The least zoom fits the map's painted part inside the bars
  (`fit::inside`; `inside_the_bars_is_the_originals_bound_at_its_own_size`);
  its dark margins stay under them. On the original's frame this is the
  original's view; a wide window shows the map larger, scrolling further
  down.
- The wheel steps through the view's zooms (`zoom::STEPS`, window pixels
  a map pixel, up to 4) about the cursor; a step within 3 % above the fit
  is left out (`the_ladder_starts_at_the_fit_and_climbs_the_views_steps`,
  `zooming_holds_the_point_under_the_cursor`).
- A middle drag, or Shift and a left drag, pans, as in play. The frame's
  edges scroll (`eight_pixels_from_the_frames_edges_push_the_map`). The
  map always covers the inside of the bars
  (`the_map_stops_at_the_frames_edges`).
- Hit regions, lit pictures and marks follow. The jewel lies on the map,
  so it zooms with it. `--zoom` with `--menu map:KEY` opens at that zoom
  about the quest.

**Levels.** The map is drawn from a level picked by window pixels a map
pixel (`art::levels`, a derived source of the art service, `Art::levels`;
`the_level_is_the_least_at_or_over_the_scale`):

| Window px / map px | Level |
|---|---|
| ≤ 0.5 | the map, area-halved |
| ≤ 1 | the map |
| ≤ 2 | xBRZ 2× |
| above | xBRZ 4× |

With `ui_upscale = none`, the map itself above 0.5, as no other picture is
upscaled (`Level::within`; `with_upscaling_off_the_map_stands_for_xbrz`).

- Each level is cut in tiles of 1024 px plus a 1 px gutter, under egui's
  2048, so linear sampling meets across tiles
  (`tiles_meet_on_the_map_without_a_gap`).
- Each level with its tiles and lit patches is one file in the art
  service's disk cache (`<key>-<w>x<h>-level.lz4`: 1× 5.5 MB, 2× 22 MB,
  4× 53 MB), keyed by the map, the quests and the lit table
  (`a_cached_level_reads_back_as_made`). Parts are read in parallel and
  made in the task pool only where missing.
- As the menus start, a task makes the table and the map's own level,
  then (upscaling on) the 2× level (`Level::AHEAD`, the map's at its fit
  on a 1920 × 1200 window). Its tiles upload 32 MB a frame
  (`a_level_ahead_uploads_over_frames_and_four_is_let_go`). The map waits
  for the table and its own level.
- A level wanted with only the map's own to stand in is read at once if
  cached and uploaded whole
  (`a_cached_level_with_no_stand_in_is_shown_at_once`); otherwise the
  nearest made level draws meanwhile.
- Textures: 36 MB at 2×, 144 MB at 4× (bevy_egui keeps two CPU copies of
  each). 4× is dropped off the map's pages; the rest is kept for the
  session.

**Hover glow.** A hovered painted quest is lit by a look, not its lit
picture, a departure (`regent_draw::art::glow`). The lit picture is the
map's art under a glow in its own palette
([panels.md](../formats/panels.md#the-quest-map);
`a_painted_lit_picture_is_the_maps_art_under_a_glow`).

```text
lit = map + w · (T(map) − map)
  T  one 17³ RGB table, trilinear
  w  Φ((d − 16) / 8), d = map pixels in from the edge of an ellipse
     filling 0.9 of the quest's lit rectangle
```

(`the_weight_is_half_at_sixteen_pixels_in`,
`the_ellipse_fills_nine_tenths_of_the_rect`.) It is applied to the
level's own pixels in a patch on the level's grid, d measured at the
level's pixels over its factor, so it cannot drift at any zoom and is
sharp at 2× and 4× (`the_weight_follows_the_map_at_every_level`). Against
the 31 lit pictures it leaves 14.0, 11.6, 8.5 a channel (the map alone
33.7, 25.2, 9.1; `tests/quest_map.rs`). A downloadable quest's picture
(`DX31`) is its building over the map's art, slot 1 that under the same
glow (`a_downloadable_lit_picture_is_its_slot_0_under_a_glow`): slot 0 is
pasted into the map before its levels are made, and lit the same way.

T is fitted at run time to the user's lit pictures: least squares on
their full-strength pixels (w over 0.97), second differences smoothing
it, conjugate gradients; under 0.5 s, once
(`a_fit_takes_a_lit_pictures_change`). It is cached as
`lit-v1/<hash>.lut` (59 KB) beside the art service's pictures, keyed by
the map's and pictures' pixels.

**Locks and marks.** The profile's progress decides each quest's look
(`quest_map::lock`, `quest_map::title_bar`,
[panels.md](../formats/panels.md#locks-and-victories);
`with_no_victories_six_quests_are_locked`,
`the_title_bar_names_a_secret_quests_needs_and_a_won_ones_best_time`):

- A hovered quest is lit and named with its icon.
- A locked quest lies under its cloud, the swirl looping and glowing
  (mode 8 by index, `blend::alphas`) with its "?" layer; hoverable, not
  pressable, titled Secret Quest with its requirements. Hovered, it is lit
  under its cloud.
- A won quest is starred; its line gives victories and best time.
- The jewel lights when hovered, the compass turns when it or its text is
  hovered, each naming itself in the title bar
  (`the_jewel_and_compass_name_themselves_in_the_title_bar`).
- Erase Victories asks with `AP11` (`GMTX` 325, 326; YES, NO, keys Y, N);
  Yes erases the profile's records (`erase_victories_asks_first`).

**Opening the map** (from the main menu or a quest's pages) runs
`Progress::open_map` ([panels.md](../formats/panels.md#locks-and-victories)):
the advisor's first voice, else the expert one; the hint once a session
(`AP11`, OK or Enter; Escape too, **Inferred**); seen flags and last won
quest saved in the profile. No cloud lifts, as in HD
(`no_cloud_ever_lifts_in_hd`). Escape on the map goes to the main menu.

### Custom game pages

The compass (or its text) opens the freestyle pick page `APd2` in its
art, centred, a Map Number rolled on the clock
([panels.md](../formats/panels.md#the-freestyle-pages),
`sim::mapgen::preset`; `the_compass_opens_the_pick_page_whose_presets_start_a_game`).
Beginner, Advanced or Expert Random writes its preset over the session's
freestyle config and starts it
(`a_preset_writes_the_config_and_keeps_the_map_number`).

Manual Settings opens the setup page `MX17` in its art (`menu::setup`,
[panels.md](../formats/panels.md#the-setup-dialog);
`manual_settings_opens_the_setup_page_whose_accept_starts_a_game`):

- Forces, events, landscape, victory and goal, gold in combo boxes; the
  Map Number typed, with openregent's RANDOM under it where the capture
  shows no ALLOW CHEATS.
- Difficulty worked out as the dialog does.
- Opens on the last settings kept in `freestyle.toml`, Map Number apart
  (`a_kept_config_comes_back_but_its_map_number`), else the registry's
  defaults: BEGI, BEGI, NONE, RAND, 30,000, eliminate; no event
  (**Inferred**; `the_config_opens_on_the_registrys_defaults`).
- Building Limitations lists the five shipped trees in blue, then the
  player's ([panels.md](../formats/panels.md#building-limitations-and-the-build-tree-editor),
  `files::trees`; `a_saved_tree_lists_after_the_shipped_and_deletes`);
  its pick's caps are recorded as `--limits`.
- BUILDING EDIT MENU opens the Build Tree Editor `MX18` in its art
  (`menu::tree`, `view::menu::tree`; `--menu tree[:NAME]`).
- LOAD and SAVE open Load and Save Freestyle Options `MX20`, `MX21` in
  their art (`menu::preset_page`, `view::menu::presets`; `--menu
  load-options`, `save-options`;
  [panels.md](../formats/panels.md#load-and-save-freestyle-options)).
  Presets are TOML files in `$XDG_DATA_HOME/openregent/freestyle/`
  (`files::presets`; `a_saved_preset_lists_and_reads_back_every_field`);
  the box beside them names the last loaded or saved.
- Not shown: ALLOW CHEATS and the chat.
- An open list takes the wheel; a press off it or Escape closes it.
  Accept (or A) starts the game; Cancel or Escape on either page goes back
  to the map (`cancel_and_escape_leave_the_freestyle_pages_for_the_map`).

A freestyle start makes the game as its options do
(`setup::Setup::freestyle`) and records it so. Won, it records no
victory, as the original
(`the_menus_start_a_freestyle_preset_which_records_no_victory`). Its end
goes back to the main menu.

### Extras

The jewel opens `APd8` over the faces (`menu::extras`), in the base
game's art like the load dialog: the quests with no map slot, by title
(`view::menu::extras::order`), row 0 lit. A click lights a row and
focuses the list for the cursor keys and letters (`view::menu::listbox`;
`a_pressed_list_takes_the_arrow_keys`). A double click, LOAD QUEST or
Enter opens its briefing; CANCEL or Escape the map. A long title is cut
with "…" before its mod's name (`a_long_title_is_cut_before_its_mods_name`).
The lit quest's description is its line, then its `<Short>`, from the
box's top and cut at its bottom (**Inferred** from a
capture). `--menu extras:KEY` lights that quest's row.

### Briefing pages

A quest's background `APc9` shows `<Long>`, with its quest slot's `QM`nn
video in the porthole 8886, 1:1, from the page's opening, once; the last
shown frame is held until the page closes; with no video, the frame's
still (`a_briefing_plays_its_quest_slots_video`). The summary `AP44`
shows `<Long>`, `<Short>` and its round still. Both come from the quest's
own dataset.

- A text longer than its box shows its scroll bar, `INDs` 1000 to 1002:
  arrows pink while pressed, a line a press, the thumb dragged (**Game**:
  captures of a briefing). Geometry is shared with the
  panels' (`scroll::Bar`; `the_thumb_runs_between_the_arrows_as_far_as_the_text_is_scrolled`).
- The briefing's text is `fn11` at 0.88 of its size and pitch (27.1, lines
  41.4 apart; the user's taste, `paint::BRIEFING_TEXT`). The summary's
  `fnt4` keeps its own.
- The background page plays its briefing voice. Each voice plays
  unplaced at the effects' loudness, muted with them
  (`audio::Narration`), and stops as its page goes, as the original's.
- Cancel or Escape goes back to the map.

Start Quest makes the game as `--quest <key>` does and records it the
same way, so it replays (`the_menus_start_a_quest_at_runtime`).

### Sound in the menus

Music follows the page's screen on the game's player (`Page::music`,
[sound.md](../original/sound.md#the-front-end)): main menu and map track
2, a quest's pages 6, the rest keep the current track. A quest's script
cuts in with its own; the end panel's Main Menu brings 2 back.

A press going down on a working control clicks `CK05`
(`menu::paint::arm`) at the effects' loudness, in the settings editor and
save dialogs too, in game at the press (`ui::save_dialog`); none is left
queued for the menus.

### Loading page

A quest's or freestyle game's start (from the menus, a restart or the
command line) first draws the loading page `AP50` filling the window
([panels.md](../formats/panels.md#the-loading-page),
`menu::paint::loading`; `every_start_and_load_shows_a_loading_page`), from
its game's dataset: frame, the still `menu::segue`, the icon, the title,
the covers; the bar empty, no progress drawn. A save shows its game's
page: a quest's, else freestyle's (a departure: the original's load makes
no `AP50`).

The game made, it is put in under the same page (`menu::drape`), the
world drawn beneath it, until its drawing's start-up work settles
(`regent_draw::settle`: no pipeline compiling, ground patch wanted,
sprite page made or picture upscaling, three frames in a row; 30 s at
most): the first frames' cost is paid before play. Under it the game
clock does not start, no input reaches the game and no game sound
starts. Then a held game offers Start Quest, the others' page goes by
itself. A LAN game is never covered: its peers start as they did.

### The game's end

A won quest is recorded in the profile once, by its code, the game clock
its time (`end::record`, `Progress::win_quest`; **Binary**;
`a_won_quest_is_recorded_and_the_menus_come_back_for_another`). A mod's
copy of a quest keeps its own record. The end panel's Main Menu goes back
to the main menu, as the original does (**Binary**).

### `--menu`

`--menu <page>` opens on a page; with `--screenshot` it captures it,
writing nothing. The last profile's progress shows.

| Page | Opens |
|---|---|
| `main[:ID]` | main menu, control `ID` hovered |
| `profiles`, `mods[:GUIDS]`, `loads`, `settings` | those pages |
| `map[-top\|-bottom][:KEY\|:ID]` | the quest map |
| `extras[:KEY]`, `erase`, `hint` | the extras list, Erase Victories, the hint |
| `background:KEY`, `summary:KEY` | a quest's briefing pages |
| `freestyle`, `setup`, `tree[:NAME]`, `load-options`, `save-options` | custom game pages |
| `loading[:KEY]` | the loading page, freestyle's or the quest's |
| `scores`, `credits`, `intro`, `cinematic` (every film), `ending` (`MV07`) | those pages |
| `PAGE@FRAME` | the page's video held at a frame, silent |

### One app

Every drawn run is one app (`draw::menu`): the menus, their game maker
(`begin::Maker`) and the shelf. The menus' quests and art are read on a
thread while Bevy starts (`main::menus`). A game is made on a thread while
its loading page draws (`menu::begin`), then installed on the main one.

Every game (the menus', the command line's, a load's, a headless run's,
the tests') is made one way (`begin::Plan`): its dataset, the world before
its game, then the `Sim` (resumed or set up) and its `Session`,
what is shown and saved of it (quest, setup, needs). A game the options
ask for (`--quest`, the custom game's picks, `--scene`, `--replay`) is made by the same maker, the app opening on its loading
page (a save's: none). From then on it is any game: LOAD GAME, Return to
Main Menu and the end panel work as for the menus' games, and its won
quest is recorded
(`a_command_line_quest_loads_a_save_records_its_win_and_goes_to_the_menus`).
The menus' own games take no command line option. `--headless` makes no
app.

The app runs in the Bevy state `menu::Screen`: `Menu`, then `Playing`,
where the game's systems run in named sets in a fixed order (`frame`:
keys, input, step, draw, pick, overlays, view; the egui pass).
`draw::install` puts a made game into the world. As `Playing` ends,
`draw::uninstall` despawns every entity made since the menus, then
removes or resets each resource a module registered as a game's
(`scope`; a test finds an unregistered one;
`a_game_leaves_nothing_for_the_next`, `an_ended_game_frees_its_world_and_vm`).
A load from the game's Options takes the game down the same way; the
menus make the save's. The `Sim` owns its `World` (`Game` holds it,
`sim::host::Held`), so both go with it.

**The shelf.** The corpus, ground and art made from them go on
`shelf::Shelf`, keyed by the corpus's archive list; the next game on that
dataset takes them back, so nothing is made twice
(`a_second_game_on_a_dataset_reuses_the_firsts_art`). The interface art
and the world's sprites go too: the game source's entries in the art
service (`art::Art::take`, `art::Sprites::take`), unless the menus draw
from that source. Archive bytes are read once a process per file
(`regent_draw::data::archives`), except videos and sounds, read as they
play. GPL programs are read once per image list (`load::images`). Not
shelved: the map's ground patches (`regent_draw::map::redraw::redraw`
draws them anew; the scatter's sprites are the art service's).

`menu::page` holds the pages and moves between them; `menu::paint` draws
them from `menu::art`, read once at start, through the art service
([Text and shapes](#text-and-shapes)).

### Logs, recordings, replays

```sh
cargo run --release -p openregent -- --quest QU06 --map-seed 108500272   # a Day of Reckoning map
cargo run --release -p openregent -- --replay ~/.local/share/openregent/play.input
cargo run --release -p openregent -- --replay play.input --headless --frames 30000
cargo run --release -p openregent -- --headless --frames 40000 --bot 1   # a scripted player
cargo run --release -p openregent -- --map-seed 1 --land Rave --screenshot rave.png
```

- **Log**: Rust `log` (tracing). The terminal (stderr) shows openregent's
  warnings and up and Bevy's info; `$XDG_DATA_HOME/openregent/play.log`
  adds openregent's info. `RUST_LOG` sets both. Each session overwrites
  the file; the previous is `play.log.1`. A headless run's summary and the
  `list` outputs go to stdout.
- **Recording**: `play.input` beside the log (`record`), written line by
  line as played; the previous is `play.input.1`
  (`a_session_reads_back_from_its_lines`).

  ```text
  setup <RON>                   the game's setup on one line (setup::Setup::recorded):
                                paths absolute; a new game's unpicked map codes,
                                Map Number and goal as defaults
  input <frame> p<player> <input>   an order taken before that frame
                                (game::Order's text, sim::game::text)
  play-on <frame> p<player>     CONTINUE PLAYING after the end's frame
  digest <frame> <hex>          every 100 frames (record::DIGEST_EVERY, 5 game s),
                                Game::digest once that frame ran
  ```

  LAN peers' recordings carry the same digest lines. An order for a frame
  already run stops the game (`game::Late`). One its player may not give
  (`Game::check`) is dropped as it is decoded, and logged. Anything else
  reaching the game unrecorded would make a replay differ: the panels'
  queries run aside (`Game::aside`); the reward settings and the
  interface's lines reach no world or VM state.
- **`--replay <file>`**: that setup, replacing the options, and each event
  at its frame; headless or drawn, `--frames` and `--speed` the caller's.
  Live input waits for the last event and drops the digests left; the
  replay records itself again, to the same file. Each recorded digest is
  checked as its frame runs; a mismatch stops the game (`replay: frame
  F's digest is ..., recorded ...; last match frame L`) and writes
  `replay-frame<F>.ron`, the snapshot, beside the new recording
  (`a_replay_that_parts_stops_at_the_digest`). A recording without digest
  lines replays unchecked. A recorded session replays to the same world
  (`a_recorded_session_replays_to_the_same_world`).
- **A stop** names its frame, game clock, the agent whose threads ran and
  the GPL stack the crash unwound, innermost first, each function at its
  last source line (`Vm::unwound`, decoded lines, release play too). The
  window says it once; `--headless` prints it.
- **`--bot <seed>`**, headless: every 100 frames presses one thing the
  panels offer (build, recruit, research, upgrade, enchant, cast, flag,
  raise), from its own random stream (`bot`). A soak driver, not the
  original's AI.
- Headless, a music track ends in game time, its MP3's length on
  (`tune`; `a_track_ends_its_length_on`): the music callback runs as a
  drawn game's does at the default pace (**Inferred**: a stand-in for the
  wall clock). A headless replay also plays on past a recorded CONTINUE
  PLAYING.

## The game loop

The `Sim` lives on its own thread, "sim" (`game::thread`), which also
owns the game clock. One drawn frame while `Playing` (`frame`, `seen`),
pipelined, as played:

```text
 Frame::Input  keys, clicks ──> Orders, seen::Outbox ──> input::send ──> asks queued
               input::want: the build preview ──> seen::Interest
 Frame::Step   game::clock::tick: the newest state, if one came, taken; then
                 Command::Frame: the asks, Interest, pause, speed
                 sim thread, on its own: asks handed over as they come; Sim::pump;
                 Sim::step per 50 ms its clock owes, each step's pieces taken;
                 a state put once the last was taken
                 seen::Frame     frames, clock, local, ended, rules, camp move,
                                 rewards, motes, the answers to Interest, the
                                 hand-over taken, and seen::Taken: a copy of the
                                 world, the last two steps' pieces and missiles,
                                 fog, effects
                 seen::News      cues, shakes, lines, peers' chat, orders' frames:
                                 riding the state, kept until taken
                 game::clock::Snapshot  the Taken steps; the slide worked out for now
               mirror::sync: a mirror entity per shown piece (Seen, Was)
 later sets    read Frame, the pieces through mirror::Pieces, the world
               through seen::Watch
 egui pass     ui::bar: the selection's panel query ──> Interest; ui::keep: a save
```

- **One owner.** Only the sim thread touches the `Sim`. The interface
  sends it commands (`game::thread::Command`: a drawn frame's hand-over, a
  save, a spent `Taken` to free; a test's look, in tests only) and reads
  what it publishes: state in a slot that keeps the latest; news and the
  game's stop on a channel, never dropped. Sends never block
  (`a_frame_is_published_as_the_game_left_it`).
- **Two modes, one path** (`game::thread::Mode`).
  - *Pipelined*, as played: the thread's own clock (`Instant`s, at most 10
    frames behind, then it falls behind) owes the time from the first
    hand-over on. Between steps it handles commands and pumps the network;
    it naps until the next step is due, at most 2 ms with peers (`PUMP`).
    `game::clock::tick` takes the newest state and never waits. A state is
    put only once the drawing took the last, so at 4x the world is not
    copied each step, and no state is dropped: news ride it, never ahead
    of the frame that shows them. This is lossless because only the sim
    thread puts, only into an empty slot, and only the drawing empties it
    (`a_pipelined_game_runs_without_drawn_frames`).
  - *Sync*, for tests and screenshots: the drawn frame's time is owed, and
    `game::clock::tick` waits for the state answering its hand-over, so
    play, `play.input`, digests and shots are byte for byte the same as on
    one thread. `a_pipelined_game_plays_as_a_sync_one` checks the news
    taken by each shown frame against sync mode's. `MAJ_PERF_PIPELINED=1`
    runs a screenshot's game pipelined (perf runs).
- **One loop** (`drive`) steps, pumps and keeps the clock for both modes
  and every headless run (single player, LAN): wait, `Sim::pump`, steps
  while owed and `Sim::ready`, each followed by a pump
  (`the_headless_drive_plays_as_steps_do`). Runs differ in pacing (the
  drawn frame's time, the thread's `Instant`s, flat out for N steps) and
  in the hooks around a step (commands and publishing; a headless run's
  bot and tallies).
- **Minimised.** A minimised window (no frames drawn: Wayland sends none)
  keeps its game and LAN peer running, as the original's game runs on
  minimised (the user's recollection, unchecked in the binary). There is
  no pause on minimise: winit 0.30 on Wayland reports neither minimised
  nor occluded, and pausing on focus loss would pause on every alt-tab.
- **Slide.** A state carries the time owed, when, and the rate (the
  speed, 0 paused); `game::clock::tick` works out the owed time at each
  drawn frame, so pieces slide between states. Sync mode's slide is the
  drawn frame's.
- **World copy.** The world is copied when it changed (a step ran, or a
  track's end ran the scripts' callback) and shared between frames while
  it does not. A drawn frame hands its `Taken` back to be freed on the sim
  thread. The fog's seen levels and explored copy and the effects are
  taken with it, on the sim thread.
- **Timers.** Pipelined, `step` and `steps` are per state, not per drawn
  frame.
- **Changes for the game** (an order, a cheat, the interface's line, a
  chat line, a camp move begun, sent or left, the reward settings,
  CONTINUE PLAYING, a track's end) go to `seen::Outbox` or straight to
  `input::send`, queued, and handed over with the frame, before any
  further step. Sync, an order takes the frame it was given on.
  Pipelined, the game may be a frame or more past the one shown, so the
  thread reports each hand-over's order frame back (`News::ordered`;
  `an_order_is_reported_with_its_frame`); a double click's flag is
  selected once that frame has run (`input::choose`). While a hand-over
  with an order is not yet in a state (`SimThread::in_flight`), the build
  preview shows Pending: a build sent there may take the spot.
- **Save and end.** A save asks the sim thread for what it takes
  (`Command::Save`: `save::FromSim`) and waits for the answer. The game's
  end drops the `SimThread`: the commands' channel closes, the thread ends
  and is joined, so its recorder is closed before the next game opens
  `play.input` (`a_game_left_mid_play_joins_its_thread_and_closes_its_recording`).
  A thread that panics stops the game: the drawing keeps its last frame,
  `SimThread::stopped` says why (`a_panic_on_the_sim_thread_stops_the_game`).
- **Panel queries.** The panels' interface runs (`Game::aside`: the
  crypt's prices, a palace's henchmen, the extortion, the bounties) and
  the preview's `CanIBuildThisBuilding` are asked through
  `seen::Interest` and answered as the sim thread publishes
  (`the_answers_are_the_games_and_only_for_what_was_asked`). A panel's is
  asked again 20 game frames on (1 s, `seen::REASK`;
  `a_panels_answer_stands_for_reask_frames`); each run copies the world
  and journals the VM objects it changes (`gpl::vm::Vm::keep`). The
  preview's is asked again when its frame or the orders held change. A
  panel shows an answer for its own piece only: a new selection's is
  blank for one drawn frame. The preview is asked before the step:
  answered that frame in sync mode, the next pipelined. An aside keeps the
  thread lists (`host::Live::put_back`), so the next step need not list
  every agent's threads again.
- **Mirrors.** Each shown piece is an entity, its mirror: `mirror::Seen`
  (the piece as shown), `Was` (its place a step before, for the slide),
  `Phase` and `Pin` (its frames' timing, `map::pieces::pieces`'s).
  `mirror::sync` alone writes `Seen` and `Was`, and only as they change,
  so `Changed<Seen>` means the world moved it
  (`a_mirror_lives_while_its_piece_is_shown`). Its drawing hangs on it
  (`DrawingOf`, despawned with it); a new set is a new drawing.
  `mirror::Pieces` reads them by agent, in agent order, as `World::shown`
  lists them. `MAJESTY_CHECK_MIRROR=1` checks them against `World::shown`
  each drawn frame (`mirror::check`).
- `Sim` is `Send` and `Sync`: `gpl::vm::Trace` is bound so, and a sink
  shares its lines through `gpl::vm::Shared`.
- Headless runs, `--screenshot`'s end (`shot::end`), the warm-up
  (`begin::warm`) and the tests step the `Sim` directly, with no app.

## Text and shapes

### Fonts

The menus' and panels' text is drawn by egui at the window's pixels, in
a proportional font, not the `IMAG` fonts scaled (`menu::font`): Inknut
Antiqua, built in (`typeface`, `[ui] font = "inknut"`, the default;
`the_font_is_inknut_by_default`), or egui's own Ubuntu Light (`"egui"`).
Inknut is the `latin` file, then `latin-ext`, then egui's fonts for what
neither holds (`inknut_draws_nordic_and_central_european_letters`). A
change shows from the next frame; a screenshot takes the default unless
`--font`. Inknut's figures are old style (a 1 reads as a small capital
I).

Sizing asks the loaded face's proportions (`typeface::metrics`,
`the_metrics_hold`). Inknut's row is 2.58 em, mostly empty, so its "row"
is its glyphs' span (`d` top to `p` bottom, 1.14 em) given egui's share
of room, 1.35 em. Each `IMAG` font gives a size as wide as it over a
sample line but no taller than its line pitch, and keeps its pitch
(`a_size_matches_the_width_unless_the_pitch_is_tighter`); the size is
times the page's or column's scale, in half points.

| Font | egui's | Inknut |
|---|---|---|
| `fn11` | 30.8 | 24.7 |
| `fn10` | 15.7 | |
| `fnt2` | 21.8 | |
| `fnt4` | 13.1 | 11.1 |
| `fnt7` | 8.7 | |

(`hds_fonts_give_these_sizes`.) Text in a fixed egui size (chat, egui
pages' styles) is drawn as tall in Inknut (`typeface::like_egui`, 0.85
times), its baseline kept (`typeface::lift`). The briefing's text is
drawn smaller (`Font::times`; `a_font_times_a_share_shrinks_its_size_and_pitch`).

Wrapping and centring measure the drawn text
(`wrapped_lines_fit_their_box_as_drawn`); wrapped lines keep each
letter's colour (`a_wrapped_line_keeps_its_letters_colours`). Colours and
markup are kept: `STRT`'s `0x01` markers and XML strings' colour tags,
`[color:7070ff]…[/color]` (`text::spans`), read over the whole text before
it wraps (`Font::wrapped`, `font::painted`), so a tag colours every line
and paragraph to its end (`a_tag_colours_every_line_it_wraps_to`).
Panels, briefings, tooltips, the help page, a message or sign's text,
the end verdict and item rows draw them; chat lines drop tags
(`view::panel::chat::text`).

A button's hot key letter is the first that types its key
(`menu::hotkey_letter`; none for Escape, Enter, `?`;
`the_hot_key_letter_is_the_first_that_types_it`). It takes `SMNU` key
49's colour, else `#5237ff` (`font::HOT_KEY`, **Inferred** from
a capture's blue letters), in every label drawn, a caller's too
(RECRUIT). A font whose glyphs are ringed by dark ramp pixels (`fn11`,
`fn10`, `fnt2`) draws a darker ring of the text's colour (**Inferred**).

### In-world text

A reward flag's reward, a unit's name (`fnt7`) and an effect's amount
(`SM09`) are egui text too (`words`): painted in the interface's layer
before the bars, clipped to the map, at the entity's world place through
`View::screen_at`, so crisp at any zoom and display scale
(`a_text_sits_centred_on_its_foot_at_any_zoom`). The size makes digits as
tall as the original's, 7 pixels, times the zoom (`words::FLAG`,
`words::GOLD`; `grow` the one knob), the baseline where the original's
digits stood (`fnt7_and_sm09_digits_measure_so`). The flag's shadow is a
shade copy `SHADOW` × zoom right and down
(`the_size_and_shadow_grow_with_the_zoom`). egui's font is narrower and
lighter than the bitmaps; Inknut is sized by `8`'s height. Both draw over
every piece.

### Shapes

Also drawn at the window's pixels (`panel::stripes`):

- a panel bar's fill, one flat band a row at its row's mean, where it
  differs from its track (**Inferred**: the rows are dithered;
  `a_bars_rows_are_their_mean_colours`, `a_fill_keeps_to_where_its_track_differs`);
- a stretched piece that is only bands (the column's edge, `INDz` 1006;
  `a_picture_of_one_colour_a_column_is_stripes`);
- the minimap's view frame.

Painted art (backgrounds, frames, portraits, icons, tracks) stays bitmap.

### List icons

A character's 25 × 25 list icon (set 1002: the palace's peasants, guards
and tax collectors, a guild's recruit, member, visitor and hero rows) is
drawn as its 100 × 100 portrait, whole frame, area-shrunk to the slot's
window pixels (`portrait::icon_source`, `art::shrink`, `art::Art::get`;
`only_a_list_icon_is_drawn_as_its_portrait`): a departure, for sharpness.
The icon is that portrait shrunk, so nothing is cropped. A building's
icon is its own drawing.

### The art service

Interface pictures come from one service, `art::Art`, a resource shared
by the menus and the panels ([art-service.md](art-service.md)).

- A `Want` names the picture's source (`art::ArtSource`: its archive
  files in load order; ids repeat across them, base and expansion `AP29`
  differ), the picture (`art::Picture`: a set's frame, a list icon's
  portrait, a room frame's tile), its tone and its wrap.
- The original is read through `art::Read` (the game's catalogue, or the
  menus' pictures read at start), toned and kept in memory. A picture from
  one source is made once, whoever asks. A reader holding part of a
  source (the menus') does not mark a picture absent (`art::Reach`).
- Asked with a `Place` (a rectangle and its page's fit), the answer,
  `Ready` or `Pending`, names a texture and where to draw it: the plain
  picture at its rectangle, or one made for the window pixels it covers;
  while pending, the held size nearest by area (`art::stand_in`).
- The panels and menus draw through it (`panel::canvas`, `menu::paint`),
  room frames included. `Ink` and `menu::art::Store` keep fonts, stripes
  and the room's runs.
- A source's sized textures age only in passes that draw it: the menus'
  wait out a game. As a game ends its source's entries go on the shelf
  with its dataset and return with the next game on it (`art::Entries`);
  the menus' two sources are pinned (`art::Shelving`).

The game view's sprites come from the same service's Bevy front end,
`art::Sprites` (`art::world`), keyed by the same `art::ArtSource`, each
made once: each `IMAG` record's sheet (`map::sheet`), each tile's
materials, each piece's layers (`art::world::Piece`), a body's
palette swaps (`art::world::Variant`: draw-effect tints, a fading
particle's glow, the fog's darkening), and the cursor's images. Pieces,
missiles, effects, particles, rings, the build preview and the map's
scatter all draw through it (`regent_draw::map::redraw::redraw` takes a
`map::cel::Paint`; the viewer's own is `Fresh`, made anew each redraw).
Nothing is scaled ([enhancements.md](enhancements.md)).

A sheet is a record's whole art: **Game** (the user), a unit once
shown plays nearly all its sets in a game, and a building kind its
upgrades, each its own record ([formats/art.md](../formats/art.md)).
Damage and rubble (`Die`, `Dead`, `Crumble`, `Damage`) may never show,
but go on the same pages: they are 65% of buildings' texels, yet in the
`g512-late` and `d-battle` perf runs 5.9 and 2.9 MB (`regent_cli
atlas`).

A sheet holds a record's world sets, each packed when a frame of it
first draws (`view::pack`, a gutter of index 255) into pages of palette
indices at most 4096 a side, more pages where one is not enough, with a
`feet` page beside each that casts a shadow. A frame is decoded once.
Its materials are the page's: a `Body` a palette and pass (a
translucent frame's also by mode and level), a `Band` a ramp
(`material::On::Sheet`).

Every body and band draws on the unit quad. Its `MeshTag` is its row in
one table on the GPU (`instance::TABLE`, written in place): the row
holds the frame's rectangle in its page and what the tag held before,
a body's footing or a band's ground row (`instance::Tag`), and a
tree's foot and bend for its lean (`instance::Lean`). The shaders
clamp to the rectangle and read the page there, so a frame draws the
texels its own texture held. The frame's size and anchor stay on the
CPU, in its quad's transform.

A drawing's layer is one entity a role for the drawing's life
(`map::reel::Layer`): its body, a band where any frame casts, a lift
where any frame is translucent. Each holds the layer's frames (shared,
`Drawing::cels`) and their `Beat` (first frame, its time, ms a frame; a
held frame never steps). Where the frame up changes, `reel::step` writes
the quad's transform (each frame's size and anchor), its rectangle (one
row), its visibility (hidden on a frame with nothing in its role) and
its material where it differs. A material made over for one entity (the
fog's darkening, draw effects, a cast model's band) is its `reel::Own`,
frame by frame. A piece, missile or effect whose key changes keeps its
entities, given the new frames (`pieces::lay`); only a layer, band or
lift the new key has more or fewer of is spawned or despawned
(`a_turn_spawns_nothing`). A tinted piece is drawn anew.

On their own texture still, a material each (`material::On::Own`): the
one direct-colour frame (a `Plain`, not indexed), and a model-cast copy
of a band (`Aloft`: its indices less the pixels a model casts, made per
piece). The page's indices stay on the CPU, once a page, for
`perf::hide`'s counts.

Two tests hold the boundary: no other file makes a sprite's materials or
adds an image, body or band (`only_the_art_service_makes_world_sprites`);
no other file loads, upscales or shrinks interface art or holds a
texture, except the save list's thumbnails and the minimap
(`only_the_art_service_loads_interface_art`). `Art::stats` counts what it
holds and the disk cache's reads and writes (`art::Stats`).

### Override folder

PNGs a user drops in `$XDG_DATA_HOME/openregent/art/` (`art::over`)
replace interface pictures; nothing is shipped. Names are `regent_cli
ui-dump`'s (`regent_core::view::picture_file`, one rule both ways), so a
dumped file can be edited and dropped back
(`a_scan_keys_files_by_the_picture_they_name`):

```text
 <archive>/<IMAG>_<set>_s<slot>_f<frame>.png   a set's frame
 portraits/<archive>/…                         the same (the dump's folder)
 tiles/<archive>_<n>.png                       a raw tile
```

- `<archive>` is the archive holding the `IMAG` record as the picture's
  source finds it (`Catalogue::find`, the last loaded), file name without
  extension, lower case: `interfacedata/AP29…` replaces the base game's,
  `mx_interfacedata/…` the expansion's.
- A reader names a picture's files (`art::Read::keys`): a set's frame; a
  portrait, backdrop or list icon its unit's interface set's first frame;
  a room frame its run's frame, then its tile.
- The file may be any size: placement, rectangle and `Art::size` stay the
  original's; tones apply (a spell icon's frame where the original's
  lies, `look::toned_as`). Larger than its window pixels, it is shrunk;
  smaller, stretched, or with `ui_upscale = "xbrz"` upscaled as the
  original is.
- The folder is scanned (names, lengths, times) at start and when the
  settings editor closes. A change drops every held picture; a shelved
  game's entries made under other files are not put back.
- A file is read (decoded once, held as the picture's original) when its
  picture is first drawn; a texture at its own size is made only where it
  is drawn so (`a_shrunk_override_uploads_no_plain_texture`). An
  unreadable file, or a PNG no rule names, is logged and left out.
  `Stats::overrides` counts pictures drawn from one and files found.
- Neither the game view's sprites (indices, not RGBA) nor the quest map's
  levels read it.

### Upscaling

**Interface pictures.** With `[enhancements] ui_upscale = "xbrz"` (on in
Modern; `art`), the menus' and panels' pictures are upscaled with xBRZ
(crate `xbrz-rs`, GPL-3.0-only) at the least of 2×, 3×, 4× covering their
scale on screen (4× above it; none at 1× or less), then area-shrunk to
their window pixels.

- Snapping: corner on a pixel (`art::Snap::Corner`); the menus' each edge
  on a pixel, so the quest map's frame pieces meet (`art::Snap::Edges`).
- Made in Bevy's async task pool; the nearest held size, else the plain
  picture, draws until then. A screenshot waits for them (`art::busy`).
- A room object's frame is laid on the room's pixels (`art::Snap::Over`):
  its texture made from the part of the frame over whole room pixels, the
  same area boxes the room's shrink takes, cut 2 px in (xBRZ's reach;
  plain, half a pixel, linear sampling's), so it meets the room pixel for
  pixel (`a_room_objects_frame_meets_the_room_pixel_for_pixel`). While a
  new window's is made, an old cut stands in, shown where the new cut lies
  (`art::ui::seen`). A plain picture is drawn at its snapped rectangle
  too.
- Known flaw (**Inferred**, from bevy_egui's shader): a see-through pixel
  at a sprite's edge draws darker (bevy_egui blends egui's
  gamma-premultiplied colour as linear), a faint line round the candles
  under xBRZ.
- A picture already on disk with no other size drawn (a page's first
  frame) is read and shrunk at once (`art::disk::at_once`;
  `a_cached_picture_with_no_stand_in_is_ready_at_once`): no plain flash on
  a page change, at the cost of a hitch.
- The shrink runs on the GPU (`art::gpu`): the upscaled picture uploaded,
  a pass drawing its area shrink into an egui user texture the same frame,
  the same pixels as the CPU's within a step of rounding
  (`the_gpu_shrinks_as_the_cpu_does`). The CPU shrinks where no GPU lends
  a texture (tests, headless).
- A picture not shrunk on the GPU is uploaded to a lent texture as it is
  (`art::gpu::Gpu::uploaded`): bevy_egui keeps two RAM copies of each egui
  texture (`EguiManagedTexture::color_image` and a main-world `Image`), a
  lent one none.
- `art::binds` replaces bevy_egui's per-frame bind groups: each texture
  keeps a place in a bindless chunk, and a chunk whose textures stay keeps
  its bind group.
- RAM held: each picture's original (`Entries::originals`, 1×, to upscale
  again on a resize) and the menus' read pictures (`menu::art::Store`,
  1×); the upscaled picture lives one frame.
- A resize keeps the old size drawn while the task runs. A pinned source's
  picture keeps the size it was last drawn at past `STALE` passes; other
  sizes and cuts go. So the main menu, back from the quest map or a game,
  reads none (`a_pinned_pictures_last_size_is_kept_while_away`).

**Animation rule.** As a page loads it may wait for its still pictures;
once shown, its animations run smooth, a frame not made by its turn drawn
plain until the made one catches up.

- Running strips (`Art::strip`) are held by scene (`art::Scene`: the main
  menu's room objects, the quest map's clouds and compass).
- While a strip of the scene is drawn, every frame of each is held, made
  in tasks (disk read and LZ4 off the main thread), nearest the shown
  frame first, at most `BUDGET` (4) originals read or frames uploaded a
  pass (`frames_made_ahead_keep_to_the_budget`). None is dropped, so after
  the first loop none is read again
  (`a_drawn_scene_holds_every_frame_of_its_strips`).
- A frame not made by its turn draws its plain picture, or its old size
  on a resize (`a_frame_not_made_by_its_turn_shows_its_plain_picture`). A
  dragged window starts each frame once
  (`a_dragged_window_starts_each_frame_once`).
- A strip first drawn after its scene's entry (a room object's lit slot,
  first hovered) reads nothing at once; its frames show plain until made.
- Once no strip of the scene is drawn, each strip keeps the frame it
  showed first on its last entry (the menus' animators start over on
  entry, so it is the next first frame) and drops the rest. Back on the
  scene that frame and the stills are held, so the first pass reads
  nothing (`a_scene_left_keeps_each_strips_first_frame`).
- The in-game panels run no strips (their frame pictures are states,
  drawn through `Art::get`).
- A still picture not in the disk cache shows plain while made: the rule
  holds only with a warm cache.

**Disk cache.** Each upscale is cached under
`$XDG_CACHE_HOME/openregent/upscale/xbrz-v1/`,
`<hash>-<w>x<h>-<clamp|repeat>-x<f>.lz4`, keyed by its own pixels (a
changed archive, tone or compositing makes a new file;
`the_cache_file_changes_with_pixels_edge_factor_and_version`): a header
(magic `MJAC`, method version, size, edge, factor), then parts of at most
1024 px a side, each an LZ4 block (`lz4_flex` 0.14) of egui's
premultiplied pixels, read in parallel (`art::disk`). A picture reads back
as made (`a_cached_picture_reads_back_as_written`).

- xBRZ takes straight alpha and weighs colour by alpha: no dark fringe.
  Before it, 4 px of padding: edge pixels repeated (an opaque edge stays
  opaque), or wrapped for tiles (`Wrap::Tiled`: no seam).
- Room objects, clouds and the compass: each frame, through `Art::strip`.
- A list icon's portrait shrinks from the upscaled one where its slot is
  larger.
- Limits: a picture over 4096² once upscaled takes a smaller factor or
  none. A texture is at most egui's `max_texture_side` a side, the GPU's
  largest (`art::side`; `egui_takes_the_gpus_texture_side`); larger is
  drawn stretched. The quest map has its own levels.
- Text, the game view and the stripe shapes are untouched. `--ui-upscale`
  sets it for a screenshot.

**World view.** With `[enhancements] view_upscale = "sharp"` (default) or
`"fsr"` (`"nearest"` draws straight to the window; `regent_draw::present`;
`each_mode_reads_by_its_name`), the world camera draws into an image at
the art grid, one texel an art pixel at any zoom, and a quad scales it
into the map's rect: sharp-bilinear, or FSR 1.

- FSR 1: EASU and RCAS ported from AMD's MIT `ffx_fsr1.h`
  (FidelityFX-FSR v1.0.2): EASU into an image the rect's pixels, RCAS at
  0.25 stops, the sample's default; constants as `FsrEasuCon`,
  `FsrRcasCon` (`present::EasuCon`, `present::rcas_con`;
  `easu_con_reads_where_the_mapping_does`).
- Shadows, fog, effects, particles and selection rings (glow indices: the
  shadow pass) stay in the image. Hit point bars are on
  `present::OVERLAY`, drawn at window pixels over the quad.
- Where an art pixel is smaller than a physical pixel (zoom below 1 /
  scale), the texel is a physical pixel
  (`a_texel_is_an_art_pixel_unless_smaller_than_a_pixel`).
- The camera snaps so texel middles sit a quarter past whole world units
  (`present::SNAP`; `the_camera_snaps_to_the_texels`): art pixel edges lie
  on whole or half units; snapping to whole units would drop or double
  columns between odd and even cels. The image's corner rounds to a whole
  pixel: a pan steps a pixel.
- Sharp at one texel a pixel (zoom times scale at most 1) would copy the
  image unchanged, so no quad draws: the shadow resolve writes the
  window, each pixel reading the texel the quad would
  (`present::Route::Straight`, `shadow::Written::Under`;
  `straight_puts_each_texel_where_the_quad_does`).
- One pass writes the window, last (`regent_draw::compose`): the world
  (resolved, or the shown camera's picture), then egui's camera's picture
  blended over the tiles egui painted (`compose::Marks`), each with the
  pipeline and blend of the pass it replaces, so the picture is the same.
  Under sharp at one texel a pixel the hit point bars' tiles come from
  the shown camera, which draws them over the world resolved under them
  (`each_route_composes_its_own_base`).
- `--view-upscale` and `--scale` (pixels a logical point; `--size` stays
  the PNG's) set it for a screenshot.
- FSR above 2× looks painted: EASU's kernel is fixed in texels.

With `view_upscale = "sharp-sprite"` or `"fsr-sprite"`
(`regent_draw::smooth`) the world is drawn at physical pixels, as
nearest, and each sprite and the ground smooth their own texels
(`only_the_sprite_modes_sample_sharp`).

- Bodies take `SHARP` in `body.wesl`: four taps blended as `sharp.wesl`
  blends texels; a neighbour off the tile or at index 247 and up takes
  the pixel's own colour, so outlines and shadow edges stay nearest
  (shadow coverage meets colour on one pixel edge).
- Bodies and bands take `SNAP`: below 2 pixels a texel
  (`smooth::SNAP_BELOW`) each quad moves by what puts its corner's place
  within its world unit (`smooth::lattice`) on a pixel corner
  (`smooth::moved`; `a_quad_snaps_to_the_pixels_below_two`); otherwise a
  half-unit cel blends half and half at 1×. A piece's frames differ in
  size and anchor, but their corners lie whole units apart, so they move
  alike and keep their places relative to each other at any zoom
  (`a_layers_frames_move_alike_at_any_zoom`). At 1× the two agree. A
  tree's foot snaps; its lean stays smooth.
- Since a frame's rectangle comes from the instance table
  (`instance::TABLE`), a few pixels of these two modes differ from what
  a frame's own texture drew: 1 to 85 pixels a 1920 × 1200 view, most one
  565 step, at most a sprite's edge row a pixel over (up to 144 a
  channel). The quads' inputs (transform, frame rectangle,
  footing) are bit for bit the same; only where the GPU puts the
  snapped quad moves, by a fraction of a pixel. **Inferred**: the
  driver orders the snap's arithmetic differently once the rectangle is
  fetched in the vertex stage.
- Ground patches get `smooth::Sheet` (one linear tap at a warped uv) and a
  one-texel apron of their neighbours (`patch::APRON`), so the blend
  crosses seams. The other modes keep apron-less patches and
  `plain::Plain`, pixel for pixel.
- The world camera is pinned so world point 0 lands on a pixel corner
  (`present::pinned`; `the_sprite_camera_pins_whole_units_to_pixels`).
- Fsr-sprite draws the world into the map rect's pixels and RCAS shows it
  1:1; no EASU.
- Look (shots at zoom 1 to 3): interiors as sharp; at 1× crisp; at 1.5×
  outlines and shadow edges step unevenly (1, 2, 1, 2 pixels), where
  sharp blends them.

**Video.** With `[enhancements] video_upscale = "fsr"` (default;
`"linear"` is egui's sampler; `the_video_upscale_is_fsr_by_default_and_read`)
a video shown larger than it is (full screen, a briefing's porthole) is
upscaled on the GPU by the same shaders (`regent_draw::art::fsr`): each
new frame, or new on-screen size, uploaded, EASU into an image the size
it shows at, RCAS into an egui texture, before egui's pass. The draw site
notes the pixels (`video::Frame::at`); the next frame's upscale targets
them, and until then the plain texture shows, so nothing waits
(`a_frame_not_upscaled_at_the_rects_pixels_shows_plain_and_asks_for_them`).
The upscaled frame is drawn on whole pixels
(`the_upscaled_frame_shows_where_made_at_the_rects_pixels_on_whole_ones`).
A screenshot shows the plain frames, the same each run; `--video-upscale
fsr` asks for FSR. Perf cases: `g-credits(-fsr)`, `g-brief(-fsr)`,
`lvp-credits(-fsr)`.

## The selection panel and ring, for a front end

The library gives what the game's selection panel and ring show
([formats/art.md](../formats/art.md#the-selection-ring-is-a-sprite),
[formats/panels.md](../formats/panels.md#the-selection-panel)):

```text
 portrait::portrait(cat, unit)    set 1000, 100x100      interface blit
 portrait::backdrop(cat, unit)    set 1001, hero panel   interface blit
 portrait::empty(cat, Crown::Gold)  nothing selected     interface blit
 portrait::Health::of(APU:0, APU:1).text(&text)   "51 of 74 hp"
 portrait::Details::of(cat, world, agent)        Hero, Purse, Coffer, Buried, Posted
 details::Experience.text(&text)                 "235 of 1000 xp"
 details::Coffer.text(&text)                     "Coffers hold 2230 gold"
 details::Posted.text(&text)                     "Castle Darkmoor"
 portrait::shows_health(cat, unit)               false for a sign
 portrait::Bar::of(APV:27).set()                  INBc 1000 or 1010
 portrait::title(cat, unit, name)  name = world::naming::spelled(text, generator, AP:1)
 ring::ring(cat, unit, Look::Selected)  slot, frame, anchor offset; world blit
 ring::fitted(cat, unit, look, Fit::Clearing)  the build preview's ring
 day::Day::of(clock).text(&text)  "20 Days"; .filled() the top bar's fill
 world.progress(agent)            recruit or research; .filled(), .text(&text)
 world.members(agent)             a guild's living, dead and spots
```

A picture is a `Sprite`: `regent_draw::texture::image(&sprite,
Backdrop::None, Blit::Interface)` makes it an `Image`. The ring is a world
sprite of glow indices: drawn flat it shows its palette's magenta, so it
needs the shadow pass, as a piece's frames do. `regent_cli selection
--record <ID>` prints a unit's records, sizes and ring.
