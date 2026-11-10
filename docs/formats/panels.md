# `SMNU` — panel and menu layout

> Part of [README.md](README.md).

Read by [`data::smnu`](../../crates/regent_core/src/data/smnu.rs), held by
`crates/regent_core/tests/smnu.rs`. Also solved in BrandonWill's `SMNUResearch/`,
which includes a compiler.

A payload is blocks: a type word, `[key][value]` items, `-1`. The first
block, type `1000`, is the panel's own; each later one is a control, its type
the class (`smnu::Kind`); a type of `-1` ends the panel. **Binary**, the
engine's property skipper (`smnu::width`): keys 2 and 42 take four words, the
colour lists (34–37, 49–52) a count and that many, 46 and 47 two (three in
the panel's block), 23 a nested list, the rest one. Every shipped record parses exactly (`every_shipped_panel_consumes_exactly`).
Consuming every record does not prove a width: "2 and 36 four words, the
rest one" also consumes every record, but misreads keys 34–37, 42 and 49
(`Panel::controls`).

A panel's `STRT` table has the panel's record id: `AP10` `SMNU` and `AP10`
`STRT` are one dialog.

## Reading it needs a second file

A panel is numbers with no lengths, names or redundancy, so a reading cannot
fail on the file it was fitted to.

HD's thirteen `Data/UIData_<w>_<h>.dat` (CYLB archives) hold the same panels
for thirteen screen sizes, each panel the same length in every file. A word
that differs between files was computed from the screen size.

Two panels differ. 102 words move: 96 inside a key-2 item, one in a key 42
(a control's text rectangle, `smnu::RECT_TOO`). The test diffs payloads and
asks the parser where moving words land, without assuming key 2
(`what_moves_between_resolutions_is_geometry_and_one_picture`).

## Key 2's four words are x, y, width, height

`APMK` exists in five files at five widths and two heights, so a word that
differs between two files of equal height follows the width:

| Follows | Slot 0 | Slot 1 | Slot 2 | Slot 3 |
|---|---|---|---|---|
| screen width | 11 | 0 | 5 | 0 |
| screen height | 0 | 40 | 0 | 8 |

Outside witness: the Phantoms Haunt mod disables `AP10`'s temple-spell button
by zeroing the last two of `(103, 162, 93, 26)` at `AP10 + 0x0d34`, leaving
later controls at their offsets; a zero-size control cannot be drawn or
clicked. The reader puts an item boundary at exactly that offset in HD's
`textdata.cam` (`the_mods_anchor_lands_on_a_rectangle`).

## Key 12 is a record id

The five moving non-geometry words are all in `APMK`: the panel swaps a
picture on a taller screen. Two key-12 words change `INse` → `INDz`; three
key-13 words move with them
(`the_panel_that_is_not_geometry_is_swapping_a_picture`).

**Corpus**, no test: every key-12 value is four printable
characters, an `IMAG` record: `INTI` (interface textures), `DEM2` (buttons).
Key 13 is its set.

## Where the handler lives

**Inferred**, from the Phantoms Haunt mod's report: `AP07` is an
executable-supported recruitment dialog, so handlers are compiled in and only
layout is data.

## The GPL debugger's panels

`Data/GPLDebuggerUI.cam` ships only with HD: six `SMNU` panels and six `STRT`
tables, `GDB1`–`GDB6`, nothing else.

`SDK/Documentation/GPL Debugger.pdf` describes five panels: Line Debugger,
Breakpoints, Tracking, Profiler, Data Sets. `GDB4` and `GDB6` are
byte-identical in both record types: Breakpoints ships twice. Each panel
carries the whole tab strip as labels, so all five names appear in all six
tables.

| | Panel | A label only it carries |
|---|---|---|
| `GDB1` | Line Debugger | `Call Stack` |
| `GDB2` | Profiler | `Total Profile Time :` |
| `GDB3` | Data Sets | `Xml Data` |
| `GDB4`, `GDB6` | Breakpoints | `Jump To` |
| `GDB5` | Tracking | `Game Object and Script Tracking` |

`the_gpl_debuggers_panels_are_five_shipped_as_six`. What the labels say about
the VM: [bytecode.md](bytecode.md).

## The selection panel

Key 6 is a control's id, the number a controller asks for; key 13 is the
`IMAG` set of key 12's record (`smnu::CONTROL`, `smnu::SET`). **Binary**: the
ids the stats window's controller pushes (8011, 8005, 8006, 8010) are key-6
values on the panels it drives.

`AP20` is the hero panel, 202×245 at (0, 182):

| Control | At | Holds |
|---|---|---|
| 8011 | (50, 28), 100×100 | the unit's portrait, its `IMAG` set 1000; pressed while a spell is aimed, a cast on the unit (`World::spell_on`) |
| 8005 | (11, 2) | the name |
| 8006 | (21, 128) | hit point bar, `INBc` 1000; 1010 when poisoned |
| 8010 | (54, 130) | `GMTX` 7, `%d of %d hp` |
| 7002 | (24, 143) | the class |

`APb5` and `APc5` carry the same controls, the portrait at (52, 26).

**Binary**, **Corpus** (`textdata.cam` panels, `GMTX`): another owner's
human-usable building opens `APb4`: the owner's colour frame (9901,
`INDe` 1010 + owner), name, portrait, hit points, level, coffer and 8030
"Belongs to another monarch." (`GMTX` 84 + 251). It has no build, tax,
repair, demolish or recruit controls. Another owner's henchman opens `APc5`:
the same frame, name, portrait, hit points and 8030 "Follows another
monarch." (`GMTX` 85 + 251); no tax controls. `details::Foreign`,
**Measured**.

The panel draws in list order. Every selection panel but two lists 8011
before its 202×245 frame (`INBg` 1000 on a building, `INDe` 1002 on `AP20`,
`INDe` 1010 on `APb5`), so the frame's rim and ornament lie over the portrait
(**Game**: captures of a hero's and a guild's panel, the rounded corners and
the jewel at the bottom). `APa6` (sign, no portrait) and `MX22` list the
frame first; `MX22`'s portrait is then drawn over it (**Inferred**, no
capture). `panel::upper::base`, `the_frame_lies_over_the_portrait`.

The name is the unit's own: `AP:1` spelled, or a numbered building's or
character's "Wizards Tower #2", "Tax Collector #1"
([saves.md](saves.md#secd--the-name-and-the-spell-list),
`portrait::numbered`); with none, the `HEAD` title, a nameless building's
first level's in capitals.

### What the panels say beyond hit points

**Binary**: each controller reads attributes, default 0 unless noted, and
refreshes a control when its key changes. The descriptor's `DialogID` names
the panel: `AP20` every hero, `AP74` the tax collector (**Inferred**: the
dialog picks the controller).

| Panel | Control | Holds |
|---|---|---|
| `AP20` hero | 7003 | `APV:11`, level |
| | 8030 | `AITX[APV:30]`, the crown's hero with `APV:28` 0; another's `GMTX` 85 "Follows %s." |
| | 5001, 5002 | weapon picture at frame `APV:19`; `GMTX` 191 "+%d" of `APV:20` |
| | 5004, 5003 | armour: `APV:22`, `APV:23` |
| `AP21` hero stats | 4016, 4015 | bar and `GMTX` 8 "%d of %d xp": `APV:16` (default 1, under 0 is 0) of `APV:10` (default 2) |
| | 5002–5011 | `APV:0` to 9, but resistance `APV:8` in 5011 and dodge `APV:9` in 5010; 26–30 star `APV:47`'s primary stat |
| `AP22` hero items | 9000, 9001, 9002 | `APU:2` gold, `APV:24` taxed gold, `APV:35` potions |
| | 9003, 9004 | weapon, armour: the `EN` table of the descriptor's first `AllowedWeapon`/`AllowedArmor` value at `APV:19`/`22` (clamped), " (%d)" basic + upgrades, " +%d" magic; an empty list `GMTX` 350/322. Pictures `INBw`/`INBa` by type, frame the upgrades (`portrait::gear`) |
| | 9005 | each `INV:n` in attribute-map order, `QITM[n]` |
| hero spells | 8730 | each spell entry with its shown byte set and `CharacterLevel` at most `APV:11`: the `DACT` title |
| | 8731 | each effector hung on the hero (relation 1) whose code the engine's 32-case list names: a `GMTX` title and an `INTn`, `INTN` or `IX93` icon. The 52-pixel list has its own scrollbar, 4865. Stone Skin's lasting icon is `MRA2`, `GMTX` 161. The original skips Fire Shield's timed `WRb2`, Resist Magic's `WRf2`, Vines' `CRB1` and the other effects in `the_enchantments_icons_and_skipped_overlays_ship` (the switch: **Binary**). openregent adds rows for them, one row for each shield while casting, and text-only rows where no interface icon is known (`portrait::enchant`) |
| `AP02` blacksmith | 7004, 20 | weapon `INBw` 1012 at frame n, `GMTX` 191 "+%d" of n: n 3, 2, 1 for the first of `APB:28`, 26, 24 researched; none hides both (`gear::Forge`) |
| | 7005, 4001 | armour `INBa` 1002: `APB:29`, 27, 25 |
| `AP53` wizards' guild | 7004, 17 | staff `INBw` 1012 (**Inferred**: frame 0); `GMTX` 191 "+%d" of the code's level digit, else 1 (`gear::staff`) |
| `AP52` warriors' guild | 7949 | living warriors (of `AVL1`) |
| | 7950, 7960 | a temple hero's icon and count while its temple stands finished or the guild holds one: one in 7950 (warrior of discord `AVM1` 7954, 7955; paladin `AVF1` 7953, 7956), both in 7960 (7962, 7964; 7961, 7963). `World::war_roster`, `view::panel::upper::war_roster` |
| `AP74` tax collector | 5000 | `APU:2` |
| `AP90` caravan | 5000 | `APU:2` |
| `AP97` monster | 4001–4003 | `APV:8`, `APV:9`, `APV:7` as `GMTX` 245–249, "Terrible (%d)" to "Excellent (%d)", below 20, 40, 60, 80; 8014 hidden, so no coffer |
| any panel with 8014 | 8014 | `APU:2` not 0: `GMTX` 76 "Coffers hold %d gold"; 0 with `IncomeType` 3: blank; else `GMTX` 77 |
| `APa7` signs, `APa6` chests | 8014 | `APM:0` not 0: `AITX[APM:0]`; else a chest's `APU:2`, `GMTX` 226 "%d Gold in Chest", 0 `GMTX` 233 "Empty" |
| | 8005 | the first item key's name up to its newline, else the name |

The palace's `AP39` has no 8014: its gold is the treasury.

A guild's members are the living on its roster (relation 100, henchmen
aside) against `APB:0` `MaxGuildMembers`; no guild panel line shows them.
The outpost's `MX08` does: it writes the living count to 7956, in frame 7950
beside hero icon 7954, "This building's current number of heroes". The icon
and HEROES (5004) open its member list (`AP18`; openregent `APd1`).
`upper::housed`, `the_guild_panels_hold_the_controls_their_rules_fill`. The
recruit button's tooltip adds `GMTX` 9 "(%d of %d)", living + 1 of `APB:0`;
every unit list's detailed row says `GMTX` 217 "…, %d of %d Members, …". A
grave counts in neither: it holds no spot, and a recruit at a guild full of
living and dead dismisses one ([vm.md](../gpl/vm.md#plays-input)).
`World::members`, **Measured**.

Who houses a hero is the scripts' choice (**Corpus**, SDK `GPLMx`). `adopt`
puts it on a building's `Members` and roster (`SetParent`) and sets its
`home`. Guilds, embassies and outposts house heroes (`ABm1`: `IsGuild`, six
spots); the palace (no spots) does not: `hero_birth` clears a palace-born
hero's `home`. A rescued outpost adopts heroes with no home
(`Outpost_Birth`, `check_strays`). A hero sulks only after losing a home: a
fallen guild's or outpost's members (`Outpost_death`, `Find_New_Home`) take a
home with room, else run `homeless`: an Inn's `sulk`, or `flee_map` with no
Inn. `vigils_heroes_sulk_when_their_outpost_falls_and_no_home_has_room`.

### Signs and chests

**Binary**: `APa6` and `APa7` share one controller, `SignPostController`.
Every `BBs` code names `APa7`: wooden banner, goblin and runic markers, note
stump, obelisk, iron and wooden signs, stone tablet; every `BBt` chest
`APa6`. A quest map places them (`barren_waste.q`'s `BBs1`, nine more `.q`
files); its script posts a message by `Post_Message` (`SetAttribute` of
`APM:0`) or makes a sign by `Create_Sign` (`SpawnUnit` with `APM:0`). A
message is an `AITX` index, as a message flag's; a save keeps `APM:0` in the
unit's property list. The Barren Waste's banner holds 143, "Castle Darkmoor"
(`the_barren_wastes_banner_reads_castle_darkmoor`). The Wizard's Curse's sign
holds `AITX` 147: five rows separated by bare CR
(`the_wizards_curse_sign_keeps_its_rows`).

`APa7`'s 8014 is a scrolling box, 167×188 at (10, 29), over the portrait
(8011, (52, 26)). **Game** (a capture): a sign's panel shows its
name at the top and its text centred both ways in the box, on stone, with a
scroll bar; no portrait (`details::Board::Sign`). `APa6`'s one line is at
(25, 128), under it. Neither panel holds 8006 or 8010, so neither shows hit
points (`portrait::shows_health`), nor does a flag's (**Game**, the user:
signs, flags and chests show no hit points). A sign with no message shows
nothing there (**Inferred**: `STRT APa7` 1 "scrolling text" is the layout's
placeholder). Buttons: 8000 centres the view, 8001 sets mini view 1 to
follow, a chest's 8028 opens `AP94`. Only an `APU:2` change refills the
panel. `view::panel::portrait::details::Posted`. Labels such as "Gold" are in
the panels' art; no table holds them. `tests/portrait.rs`.

### The help page

**Binary**: the help page `AP95` picks its `HPTX` topic by unit kind
(`world::kind::Kind`): a building's `HelpID`, a reward flag's `h152` (heroes are
motivated by rewards), any other the character's `HelpID`; a miss keeps the
page's "No knowledge available". A chest or sign is a building, kind 0
(**Corpus**: saves keep them in `BLDG`; **Binary**: the engine tests for
`BBt` among buildings), so it reads its own `HelpID`: a chest `h155`
(heroes discover treasure), the goblin marker `h151`; the wooden banner has
none. Five characters have none (Krolm `BV01`; Cave Wyrm, Doppleganger,
Undead Orc, Lorphus, all without art), so they show the page's text, as the
original. `portrait::help`, `a_reward_flags_help_is_the_reward_topic`,
**Measured**.

`AP94` runs the same controller on other stonework (`INDe` 1009; **Binary**). A selection
panel holds one "?": 8028 opens `AP94` (hero, henchman, caravan, sign,
resource and tax collector panels), 8029 `AP95` (the rest). Panels with 8028:
`AP20` `AP74` `AP90` `AP97` `AP98` `APa6` `APa8` `APb1` `APb5` `APc5`;
**Inferred** for those whose controller is unread. `portrait::help_page`,
`a_panel_with_8028_opens_the_short_help`.

The main screen `APMK` holds the panel's slot, control 2005, at (0, 182).
With nothing selected it shows a crown:

| Source | Crown |
|---|---|
| 12 of HD's 13 `UIData_<w>_<h>.dat`, all but 800×600 | `INDe` 1000, gold |
| base `textdata.cam` | `INDe` 1000 |
| expansion `mx_textdata.cam` | `IX72` 1001, purple |

**Binary**: the main screen forces `IX72` only while a flag byte is clear;
it is 1 in HD and nothing writes it. **Inferred**: a `UIData` panel shadows
the dataset's, so HD at 800×600 in the expansion shows the purple crown.
`the_empty_panel_is_a_crown`;
[`view::panel::portrait`](../../crates/regent_core/src/view/panel/portrait.rs).

Every start drops a save's selection: a load selects nothing; a new game then
selects the local player's palace, else outpost, silently, the view and free
mini views on it; with neither, nothing (`World::selected`, `player_palace`).
openregent selects every human seat's; each machine shows its own
(`World::selection`).

### A building's progress bar

**Binary**: while a guild recruits or a researcher researches, its panel
shows a bar and a text over it:

| Panel | Shown while | Bar | Text |
|---|---|---|---|
| guild | `APB:1`, `APB:13` | 8022 | 8023: `APB:12` spelled by `APB:14`'s `NameGenType` |
| researcher | `APB:23`, `APB:2`, `APB:13`, `APB:44` | 4500 | 4501: `GMTX` 215 `Researching %s`, the row's `GMTX` text |

Both send the bar message `0x29` `{7, game clock − APB:56, 0, APB:13}`: mask,
position, minimum, maximum (the day bar sends mask 1 and its position).
`APB:56` is the decode's clock, `APB:13` the order's time, so the bar is full
as the order's timer ends. While the order's message waits for its decode the
bar is empty and the text hidden. **Inferred**: the bar clamps to its range.
`World::progress`, **Measured**.

## The day counter

**Binary**: the main screen refreshes the top bar's counter. Control 7021
gets `GMTX` 24, `%d Day%s`, with the days passed and `GMTX` 25, `s`, unless
the count is 1; control 2020's value is whole seconds into the day scaled to
0–98:

```text
days    = clock / 60000
percent = (clock % 60000) / 1000 * 100 / 60
```

The clock is a timer zeroed at game start and rebased on the game clock every
frame. **Inferred**: it is the game clock. The save dialog names a save with
the same text: **Corpus**, every save whose file name says "N Days" has N
days on its `GSGL` clock (**Measured**). HD writes "1 Day" at day 1.

**Corpus**: every `APMK` (12 `UIData` and both datasets') puts 7021 inside
2020, a bar of `IMZa` set 1000 with range 0 to 100 (keys 31, 32), in the top
right corner. `regent_core::view::panel::day`, `tests/day.rs`.

## The main screen

**Binary** unless marked. The main screen's controller takes `APMK`'s
presses.

| Control | Is | Does |
|---|---|---|
| 2008 | the title scroll's text, over `INse` 1007; its `INTI` 1016, all shadow, is not drawn (**Game**: the scroll keeps its colours) | pause command 0, flag 1: paused, `AP88` |
| 9005 | OPTIONS | the same, flag 0: paused, `AP36` |
| 2009 | Select the Palace | the player's palace selected with its sound, mode `Sele`; the view does not move |
| 8734 | zoom check | zoom 1.0 to 0.5, else 1.0 |
| 18 | hit point bars check | display flag bit 2 (`DisplayFlagStats`) |
| 19 | unit names check | display flag bit 0 |
| 21 | minimap textures check | minimap filter bit 2: off, explored ground grey |

A check draws slot 3 while checked, 6 more while hovered, plus its state; a
slot past the set's count comes back (`view::menu::drawn_slot`;
[the panels](#the-panels)). **Game**: a capture's hit point check,
checked, is `INDz` 1016 slot 3, gold.

The scroll is placed by hand in each `UIData`: about 20 pixels left of the
middle of the bar right of the column (x 398 at 1024 wide, 723 at 1680, 844
at 1920).

Its text: a quest's `STRT QUES` title; a custom game's Enemy Forces word:
`BEGI`, `ADVA`, `EXPE` are `GMTX` 111–113, "Random (Beginner)" (**Game**:
a capture), any other the force's name.
`regent_core::view::panel::scroll`, `tests/scroll.rs`.

The pause: app state 7, every sound paused, then `AP88` or `AP36`. RESUME
sends command 1, back to state 3.

| Panel | Buttons |
|---|---|
| `AP36` OPTIONS | 5000 GAME INFO `AP88`, 5001 SAVE GAME `AP49`, 5002 LOAD GAME `AP27`, 5003 ADJUST LEVELS `AP37`, 5004 RETURN TO MAIN MENU `APc8`, 5005 QUIT MAJESTY `AP04`, 5006 RESUME PLAY (Escape), 5007 RESTART `APd3` |
| `AP88` GOALS | 9004 RESUME, 8088 OPTIONS `AP36`; 5001 the goals, scroll bar 5053 |
| `APc8`, `AP04`, `APd3` CONFIRM | 5501 YES: command 5 (the menus, app state 1), command 4 (exit, state 9), the restart; 5001 NO back to `AP36` |

GAME INFO opens `AP88` as the scroll's pause does: one panel. `APd3` asks
whether to restart and lose the current game. YES (also the end panels'
restart) tears down the game as a return to the menus does, then sets the
panel the menus resume on, by the game's kind: a quest (Krolm and Vampiric
Revenge too) `AP44`, its summary, skipping the background page; a freestyle
game `AP63`, the pick page `APd2`, which rolls a new Map Number, the running
game's config copied back first. Start Quest there is a new quest start, its
map on a new clock word; Cancel goes to the map `AP30`. openregent
`Flows::restart`.

The goals: a quest's `<Short>`; a custom game's `Maj.qdd` block `FRG0` to
`FRG3` by its victory condition, the Long Description with the goal for gold
and days, else the Short, then, unless the settlement is random, `GMTX` 321
"Map Number: %ld.".

### The tracking window

**Binary** unless marked. `regent_core::view::panel::tracking`,
`view::panel::tracking`, openregent `tracking`.

Mini view 1 is `APMK` 2002 "Tracking Window" (190×126 at 1920×1200), its
caption 4467, its frame picture 4466. It is an iso view at zoom 0.5, with the
local player's sight and the main view's cursor mode (one call sets views
0–4): a left click selects, a right click deselects (**Game**, the user).
Each drawn frame the view under the pointer becomes the cursor's view, and
every click handler acts in that view: its ground pick and unit pick. So
spells, both reward flags, building and the camp move work in the window as
in the main view, double click included. A handler's true result in view 1
sets that view alone back to Sele; **Inferred**, unread: which events return
true. openregent's right press leaves the mode everywhere. Hit point bars
show; names and amounts follow the zoom rule, so none at 0.5 (**Game**, the
user).

```text
 what it follows
   a new game        the local palace, "given"
   a load            the save's GSGL words 2, 3, 5-15
   8001, a unit list row (AP86, AP87, rosters but buildings), Alt+F2..F8,
   Ctrl+1040         that piece
   a message flag    with the option (default on), unless on one already
   a new hero        while following nothing or the given palace
   minimap right     none; the view there
 each frame: held, its holder; unseen, no move and the piece deselected;
   a building at its exit
 gone, the first that holds: the nearest message flag (the option on);
   the spot kept; the piece kept, alive; the nearest local hero; the
   palace (given)
 kept: following on from something, none keeps the spot left, a piece
   keeps the piece; a message flag keeps nothing
```

The row: 1015 reward flags, 1013 heroes, 1021 henchmen, 1014 buildings are
filter checks, all off at first. 1012 back and 1009 forward step to the next
category on (forward: heroes, henchmen, buildings, flags) and in it to the
piece after its last, in unit list order, wrapping: the local player's,
standing, with an agent; a reward flag not held. 1016 Autoscan steps forward
each time the clock passes a multiple of `#miniview_cycle_rate`, 1500 ms.
1040 centres the main view on the followed piece (or its holder), else on the
window's middle; with Ctrl the selection is followed. No filter on: 1012,
1016, 1009 disabled and Autoscan off. Any follow but a scan's stops Autoscan.

**Corpus**: every save's words name a piece it holds; the quest saves at
or near their start follow their opening message flag
(**Measured**). **Inferred**:
unit list order is agent order; the follow step runs each drawn frame; the
nearest is measured from the view's middle.

### The tooltip line

**Binary** unless marked. `regent_core::view::panel::tip`, `tests/tip.rs`.

```text
 the tip manager (font fnt7), each interface pass:
   pointer moved or a button down ──> its clock restarts
   no button down: find the control under the pointer
     none ──────────────> the line hides
     another, shown ────> it moves there at once
     else ──────────────> shows once the pointer rests 1 ms
                          (500 ms where the control gives 0)
   the text: the control's key 33 (a list row's own); a controller
     override appends or replaces
   placed: APMK 2004's x0, its bottom y1, width x1 - x0; one row raised
     3 px; a window of the text in fnt7, palette TTIP, 4 px inside, its
     bottom at the point
```

At 1920×1200, 2004 is (281, 1173) 1556×26: the text starts 4 px in, right of
SPELLS, its row ending 7 px above the box's bottom. A longer text wraps at the
width and grows upward. A control's flag bits 11–14 could keep the window
below or left of the point; no shipped control sets them. **Game**: the
spell tooltip captures draw the text (216, 152, 72) at 565, the
colour the panels' prices use. **Inferred**: `TTIP`'s role 3; the path is not
read.

The overrides, each starting from the control's own text:

| Panel | Control | Line |
|---|---|---|
| spell bar, spell pages | a spell | own + `GMTX` 29 "- (%d) gold" of its price (none if free); disabled: + 190 "- Unavailable: Already in effect" for SuperCharge while it runs, else + 181 "- (Unavailable: requires %s)" of its building's description at the row's stage |
| any building | 8007 upgrade | replaced, the first that holds: damaged 243, building 240, no `UpgradeTo` 239 if `UpgradeFrom` else 238, disabled 238, capped 398, research 241, heroes 242, member level 244; else 26 "Upgrade to " + its description + 27 "- (%d) gold" |
| | 8004, 8008, 8021 disabled | 271, 270, 269 |
| | 8024–8027 | by tick: 254/255, 252/253, 258/259, 256/257 (what a press would do) |
| a guild | a recruit | own + 9 "(%d of %d)", the hero it would be of the spots, + 0 "- (%d) gold"; disabled 270 |
| a researcher | 8851 | damaged 243, disabled 188 |
| warriors' guild | 8009 | own + 28 of the Call to Arms' price; no members 260 |
| rangers' guild, Temple to Krolm | 8009 | own + 28 of the camp move's, the Rage's price |
| rogues' guild | 8009 | replaced: 327 of the take less the cut, and the cut |
| wizards' tower | 5001 | own + 75 "- (%d) gold" of `#WizTowerUpgradeCost`, enabled or not |
| build list | 5000 BUILD disabled | 272 (348 in a network game) |
| unit lists | 5004 detail | 343 detailed, 344 not |
| a flag | 8019, 8995 disabled | 276, 277 |
| hero | 5001, 5004 | the weapon's, the armour's name |
| hero stats | 5007–5011 | replaced: 380 "This hero's current %s skill - There is a cap of %d on this skill." with the skill (382–386) and `#max_tohit` (hand to hand, ranged) or `#max_toavoid` (parry, dodge; `#deathmatch_max_toavoid` under `GplAIRoot`'s `Deathmatch_Rules`); 5011 magic resistance 381, the deathmatch minimum `#deathmatch_hero_magic_resist`, whatever the rules |
| main screen | a mini view row | its column's first-row control's own (four rows of ten, `APMK` holding two); back, cycle, forward (columns 6–8) disabled 275, 273, 274 "All filters are off"; column 9 own + a hint that Ctrl and click puts the selection in the mini view |
| message scroll | 5005 | by tick 337/338 |

The map has its own handler (`regent_core::view::panel::tip`,
`view::panel::tip::map_unit`):

```text
 pick: the view's pick, reach 0: a piece met on its picture; no holder,
   not the build preview
 nothing while a modal panel is open
 a standing building, a mobile or a message flag; not ABA0, XL00
 named: a TransparentToMouse overlay's holder, a clone's unit (APU:8);
   own name, else the HEAD title
 hit points, unless a building without HasHPBar, a character without it,
   or AVk:
   " (%d of %d hp)" APU:0 raised to 0, APU:1; " (%d of %d hp, %d gold)"
   with gold; none of APU:0, nothing
 no hit points: " (%d gold)" with gold
   gold: APU:2 where HasGoldToolTip, above 0, the local player's
 the local player's standing mobile with APV:30: " is " AITX[APV:30]
```

**Game** wanted: no capture shows a map tooltip. A message flag says its
title, "Message flag". Effectors are made as mobiles (unit kind 1) and would
be picked too; openregent draws them apart from the pieces and picks only
pieces and message flags.

### The game's end

**Binary**. Steps in `regent_core::sim::world::defeat`;
`regent_core::view::menu::end`, `tests/victory.rs`.

At state 8 a won game shows `AP11` (title `GMTX` 310 "Game Over", text 309
"You are victorious.", YES and NO hidden, OK); after OK the view goes to the
victory point and the fade waits `#EndGameFadeDelay` (1500 real ms). A lost
game fades at once. Past 15 steps and the advisor done, the panel opens (**Binary**).
**Game** (a capture): the box centred on the window, the advisor's
line under it.

The four panels are 800×600, centred (`AP60` says placed at (0, 0);
**Game**: a capture centres it). Frame `INSw` 1000, round
picture control 1 (`INSw` 1003 won, 1004 lost; the expansion's `IX78`), pog
8730, title 8005 (the quest's title), the heading at (479, 96) ("YOU HAVE
WON!", "YOU LOST!"). `INTM` 1005 (7460, "round end quest picture", 750×375
in 81×88) shows in no capture.

| Panel | Shows | Buttons, hot key |
|---|---|---|
| `AP60` won | text 3: `QEND` by quest (`fn11`), scroll bar 9101 | 5003 STATISTICS S, 5010 CONTINUE PLAYING C, 5000 MAIN MENU M |
| `AP61` lost | text 4055: `LOSX` by quest | 5003 STATISTICS S, 5002 RESTART R, 5000 MAIN MENU M |
| `AP84` won | backdrop `INSs` 1001, "End Game Statistics" | 5010 CONTINUE PLAYING C, 5000 MAIN MENU M |
| `AP83` lost | backdrop `INSs` 1000 | 5002 RESTART R, 5000 MAIN MENU M |

STATISTICS opens `AP84` or `AP83`; nothing leads back. MAIN MENU returns to
the menus, RESTART restarts ([the main screen](#the-main-screen)), CONTINUE
PLAYING sets app state 3, the game going on, the music again (**Inferred**:
its last track). Any press first stops the end voice. The statistics page
sets the pog alone, its title its own; rows 1 to 25 of the palace's `AP40`
list each go into the control the row names: 4201 Total Made, 4230 Gold
Balance, 4231 Gold Initial, 4202 Total Spent, 46 and 4203 to 4206 by kind,
4207 to 4209 buildings, 4210 to 4213 heroes, 4214 Vice, 4215 Danger Level
(`view::menu::end::stat_control`). A custom game whose settlement is not a
random pick shows 6678 "MAP NUMBER" and 6766 its number.

A network game hides CONTINUE PLAYING and RESTART on all four panels
(property 10 too), and in a lobby session names MAIN MENU `GMTX` 336 "Return
to Lobby"; the return then goes to app state 6, not the menus (**Inferred**:
the session's lobby) (`view::menu::end::Play`; openregent:
[openregent/multiplayer.md](../openregent/multiplayer.md), LAN).

The expansion's ending (`view::menu::end::Panel::ending`): with every
expansion slot (19 to 30) won and the ending's seen byte clear, MAIN MENU,
RESTART and CONTINUE PLAYING on `AP60` or `AP61` first ask for `MV07`; the
statistics pages do not. MAIN MENU (and "Return to Lobby") and CONTINUE then
set the byte, saved; RESTART does not, so asks again next time. CONTINUE
first opens the base's cinematics, failing that `AP11` "CD-ROM" / "Please
insert the Majesty CD." (`GMTX` 351, 352) and no byte. In HD nothing plays:
a video is found only in an open archive, and only `mx_cinedata2.dat` and
`mx_cinedata3.dat` hold `MV07`, which CINEMATIC alone opens and closes
(`tests/video.rs`). HD installs the base's cinematics, so the CD box never
shows. The byte's one effect is CINEMATIC's second video. openregent asks it
from the statistics pages too
([openregent/enhancements.md](../openregent/enhancements.md)).

No end panel tests the base slots: the check loops over slots 19 to 30 alone.
No archive of HD or Gold holds a southern ending: `cinedata2.dat` and
`cinedata3.dat` hold `MV02`, `MV04` and `SPLH`, `cinedata1.cam` `MV01` and the
`QM*` (`tests/video.rs`). **Corpus**, seen by frame: `MV02` is the Cyberlore
logo, `MV04` the manual's "opening movie", ending on the title; `MV01` and
`MV06` credits. Gold's code is encrypted by Steam's stub and unread.
So a southern win plays nothing;
openregent's films: [openregent/enhancements.md](../openregent/enhancements.md),
`view::menu::film`.

"All Southern Quests" (`GMTX` 142) is the text for `QU06`'s rule, which names
the other 18 base quests (`quest_map::SOUTHERN`). No text calls a set
northern; the manual's "Northern Expansion" and slots 19 to 30 do.

**Game**: under the panels the column and bars are half as bright as over the
box, the map as bright (**Inferred**: the map drawn again each frame).

## Game messages

**Binary** unless marked. Every game message is a line in the chat area: shown
where its player is −1 or the local player, a line end appended, no sound.
Engine lines are green (`0x8000ff00`).

| Said by | Line | To |
|---|---|---|
| a recruit's end | `GMTX` 10 `Recruited: %s`, the hero's `HEAD` title | the guild's owner |
| a research's end | `%s %s : %s`: the building's own name, `GMTX` 188, the row's `GMTX` | its owner |
| a decode, the treasury short | `GMTX` 31 recruit, 32 building, 33 upgrade, 34 spell, 41 enchantment, 262 research | the payer |
| a spell's effect | `GMTX` 42, a magic mirror | the caster |
| `LocalChatMessage(player, n[, agent])` | `AITX[n]`, `%s` the agent's own name, else title; "Olaf (Ranger)" where they differ | `player` |
| the interface | a refused build click (`GMTX` 63–70, 394, 395, or `CanIBuildThisBuilding`'s `AITX` line), a short treasury at a button, `GMTX` 311 saved, 335 autosaving | everyone |

The scripts' lines: building complete (`AITX` 173) and upgraded (174) in
`Building_Births`, a hero died (176) and knocked unconscious (268) in
`Hero_Deaths`, an embassy's arrival (273). No line for a hero's level, a
building lost, or a spell cast. A typed line shows in `0x80ffae2c`, a team
line `0x80fe7700`; in a network game after the sender's name in their colour.

The area is `AP75` item 1, an info box placed 10 px in from the view's sides,
60 px tall, its top 70 px above the view's bottom (**Inferred**: bottom, the
rectangle's field order unread). Font `fnt4` (key 18), an `IMAG` of glyphs
whose slot 0 gives ascent 10 and descent 5; palette `IBOX` (key 11), green its
entry 32 (0, 216, 0), the chat colour entry 66.

```text
 rows = 60 / 15 = 4; the cursor on the last
 typing   one character every 10 ms of real time; a CR feeds a line; a
          word that will not fit starts a row; a space at a row's start
          is dropped
 idle     every #MessageBoxScrollSpeed (3000) ms, nothing to type, a line
          feed, at most 4 in a row: every row moves up
```

So three past lines show above the one being typed, and a line stays 6 to 9 s
once nothing follows. No fade. The clock is the wall clock, while paused too
([openregent/screenshots.md](../openregent/screenshots.md)).
`world::chat::Said`, `view::panel::chat`, `tests/chat.rs`.

## What the panels offer

**Binary** unless marked. The rules are in `regent_core::sim::world::offer`,
tested in `tests/offer.rs`.

| Panel | Offers | Ready when |
|---|---|---|
| Build menu | kind 2, `CanUse` HumanPlayer, not `NotBuildable`; a building not ready unlisted, or red (`\x01FF0000`) while the player lists the unavailable (control 5405, prefs `BuildingListShowUnavailable`, default 0; 5404 the icons, `BuildingListVerbose`, default 1); no treasury asked; sorted by name byte by byte, case kept | not disabled (`DisableUnitType`, the quests'), under its cap (`SetBuildingLimit`: the owner's standing buildings of the code's prefix), `BDEP` met |
| Guild | `Produces[0]` (the `SECD` list past `HelpID`) | `FirstStageBuilt`, no recruit running, not full, the hero's type not disabled |
| Warriors' guild | warrior; Fervus the warrior of discord, else Dauros the paladin, in one slot (openregent offers both, a departure) | as a guild |
| Warriors' guild (`AP52`) | CALL TO ARMS (8009, hot key C), `#AssemblyCost` (500) in 4481 ([below](#the-call-to-arms)) | a living member; a short treasury refuses at the press |
| Marketplace (`AP31`) | MARKET DAY (8200, hot key M), no price; while one runs, bar 8201 and 8826 "Market Day in progress" ([below](#the-market-day)) | `APB:1`, `APB:37` the research, no Market Day running (`APB:22` 0) |
| Rogues' guild (`AP48`) | EXTORT (%d) (8009, hot key E), the player's share, no price ([below](#the-extortion)) | `ShowExtortionTotal` at least 1 |
| Research | the stage's rows, past 1 once the earlier stage is done; a row is a panel control of id 5000 to 5099, the research's, with price id + 1000 and icon id + 500, shown or hidden together; any other control always shows: `MX03`'s icons 5500 to 5505 are no row's, so the Magic Bazaar shows all six (`view::panel::research`); not ready, a star (`INBb` 1015) for its price; the button keeps its panel label ("LVL 2 Weapons") and hot key, only the price gets text | not done, stage finished, whole, idle |
| Upgrade | `UpgradeTo`; none once finished if disabled or capped | finished, whole, researched, the owner's standing heroes at least `Upgrade_HerosNeeded` — anywhere, not the roster; only the palace sets it (`heroes_to_upgrade`: 4, then 12); `Upgrade_MemberLevelNeeded` reads the building's own level, and nothing sets it |
| Spells | 28 rows: building, stage, `#SpellCost_` | the owner's best level of the building at least the stage; an Abode spell not while its timer runs, `AP92`'s bar 9500 (`INBd` 1000, the Abode panel's 4500 art) over its six buttons meanwhile, filled as 4500 (**Binary**; `spell_bar::RESTING_BAR`, `World::abode_rest`); SuperCharge sends at once, a unit spell on a target the engine accepts ([vm.md](../gpl/vm.md#the-players-spells)). No gold asked by the button, its tip or its key: the map click, the spot taken, says `GMTX` 34 short and sends nothing (`orders::cast`); SuperCharge's decode says it |
| Rewards | attack and explore, each a setting, lowered to 0, raised within the treasury, kept to it (`Rewards::clamped`) | attack: the treasury holds it |
| A flag (`AP46`, `AP80`) | raise (5003) by `#RewardDelta`, at any reward, 0 too; no lower; remove (5000, "Remove this bounty") | raise: the owner's treasury holds `#RewardDelta`; remove: always |
| Rangers' guild | move camp, `#MoveCampCost` (800) | the camp's `APU:0` not below 0, the treasury holds it; recruits off while moving |
| Sorcerer's Abode (`MX06`) | Spells (8851, S) opens `MX07` (as `AP66`–`AP72`): its six spells 4423–4428, each price box 1000 below, icon 1000 above, price always written; a press aims as the bar's, at rest too (no timer asked); 8013 back. `view::panel::spell_bar::ABODE_PAGE`, openregent `Page::Casting`. One a player: its `BDEP` rule names itself, so the build menu offers none while one stands, finished or not (`the_build_menu_offers_one_sorcerers_abode`) | while `ASS:0` is set and `ASP:0` not run out, 8851 hidden, bar 4500 and 4501 "Sorcerer Is Resting" (`World::resting`) |
| Embassy | open or close, no price | **Inferred**: always. **Game**: the bounty probe opens one at 4371; its timer spawns at 5001, 5971, 6775 |
| A spell-giving guild or temple | SPELLS (8009) opens its own page, a `SpellsController` as `MX07`: wizards' guild `AP66`, Agrela `AP67`, Dauros `AP68`, Fervus `AP69`, Krypta `AP70`, Lunord `AP71`, Helia `AP72`; never the spell bar, which only the main frame shows or hides (**Binary**; `spell_bar::TEMPLE_PAGES`, `each_temples_page_holds_its_own_spells`) | always |
| Temple to Krolm (`AP24`) | RAGE OF KROLM (8009, hot key K), `#RageOfKrolmCost` (1500) in 4484 ([below](#the-rage-of-krolm)) | a hero of the owner's; none running; a short treasury refuses at the press |
| Any own unit | its name box 8005 ([below](#renaming)) | a hero at `#HeroNameEditingLevel` (1) or more; any other unit |
| Mausoleum | 8014 `GMTX` 185, the interred counted; the list `MX05` ([below](#the-mausoleums-list)); a hero's resurrect, `Mausoleum_Resurrect_Cost` | the list: the owner's, `APB:1`, not empty; resurrect: a row, the treasury holds its price ([vm.md](../gpl/vm.md#the-mausoleum)) |
| Graveyard (`AP15`, list `AP16`) | the local player's: 8014 `GMTX` 185, its hero list counted; 8004 opens `AP16`, a row per dead hero: `GMTX` 221 "%s,\nLevel %d" and the code's icon (`world::graveyard`, `details::grave_row`) | another's: 8004 hidden, 8014 blank; no coffer line |
| A building | tax once (8024, action 7), tax route (8025, 6), repair once (8026, 9), repair route (8027, 8), each a `GUILD_ACTION` with the building's handle | tax: `IncomeType` not 3 (towers, guardhouses, dwellings, the fountain); repair: `FirstStageBuilt` |
| A tax collector (`AP74`) | return (0xb up, 0xc down) and pickup (0xd down, 0xe up) by `#Taxman_Increment`, 25: an `AlterAttributeDeltaMessage`, range 0 to `#Max_Return_Amount` | up: below its maximum (`#Max_Return_Amount`, `#Max_Pickup_Amount`, 1000) by a step; down: a step above 0 |
| A hero (`AP20`) | dismiss (8002, hot key X): `AP62` asks, `GMTX` 20 and 22; YES a `GUILD_ACTION` 5 with its handle ([below](#a-heros-dismissal)) | shown only for the player's hero, `APV:28` 0; a dead unit or ruin sends `DESTROY_VEHICLE` unasked, as a double click on an own ruin or body does: its decode dispatches `Unit_Dismiss_Gravestone` at once (`Input::Clear`) |
| Palace | statistics, rewards, rosters, the build menu, upgrade, tax and repair ([its pages](#the-palaces-counts-statistics-and-rosters)) | no hire: palace guards are the scripts' (`Guard_Spawn_Function` up to `Max_guards`) |
| A research page | 8851 opens its own page: `AP02` `AP99`, `AP08` `APa0`, `AP17` `APa1`, `AP26` `APa2`, `AP31` `APa3`, `AP51` `APa4`, `MX02` `MX03`; the Abode's (`MX06`) opens its spells `MX07`, the Hall's (`MX00`) its bounties `MX01` | the generic researcher opens none |
| Fairgrounds (`AP08`) | contest radios 1200–1204 set `APB:5` (none, archery, melee, magic, combo), with pictures 3001–3004 and 7; visitors 8021 open `AP09` | contests: `APB:30` set, which hides 8851, so the page under opens on the tips, not the research (**Inferred**); a radio sends `Input::Set` |
| Gambling Hall (`APb8`) | one of three states on shared places (`World::table`): the bet (colours 1200–1202 by `APB:46`, wager 9011 with − and +, SPIN), the spin (`APB:49`: bar 8201, 8826, MAKE NEW BET), the result (`APB:48`: one of 8501–8504 by `APB:50`, MAKE NEW BET; line 8827 `gpltext.cam`'s `STRT GAMX`: `REDW` "Red Won! you win %d gold" of `APB:53` above 0, else `REDL` "Red Won! you lost", `BLKW`/`BLKL`, `GLDW`/`GLDL`, the skulls `SKUL`) | the bet's rule is `World::wager`; the spin (`GUILD_ACTION` 11) and MAKE NEW BET (12) are `World::spin`, `World::new_bet`; the wager and colour come as `AlterAttributeMessage`s, `Input::Set`: the wager at open, at − and +, at SPIN, back in the bet and on each treasury change; the colour at open and at its radio (`panel::games`) |
| Hall of Champions (`MX01`) | the local player's slain monster types (`World::slain`: a monster mobile counts as its unit is deleted, so a corpse or grave once gone, for slots 0 to n − 1, so a kingdom in slot 0); short and long bounty (2050, 2051, `GUILD_ACTION` 0x14) ([below](#the-hall-of-champions-bounty)) | a bounty: the treasury holds its price |

The build list (`AP03`): a press lights a row, its price `GMTX` 2 "%d gold" in
5002, and leaves any mode; BUILD (5000, B) places the lit row, and a left
double click on the list presses BUILD, so one press never places
(`panel::lower::build_press`). Only the palace's panel opens it, and a panel
belongs to its unit: a new selection or none closes it, and closing leaves
any mode, so a building being placed is dropped (`panel::lower::after_page`).
**Inferred**: another palace page or the "?" tips closes it too.

### The Mausoleum's list

**Binary**. LIST OF DEAD (8004) opens `MX05` (the base building's 8004 opens
`APd1`). Its rows are a unit list's, as a guild's members (`GMTX` 220: "… is
Interred in Mausoleum (0 of 232 hp)"); title 7560 "LIST OF DEAD HEROES",
scroll bar 5010, back 8013 (Tab), detail 5004 (D).

| Control | Does |
|---|---|
| 5000 | a press lights a row: its price, `%d`, in 8006, `\x01FF0000` before it when the treasury is short; no row hides 8006 |
| 5003 RESURRECT, R | enabled with a row the treasury pays; sends `GUILD_ACTION` `0x15` (the Mausoleum, the hero, the price), then the base list's SELECT: the hero selected, its panel opened |

The label sits in its key-42 rectangle, the price over the button's right end
(**Game**, a capture). openregent clears the lit row at the press
(**Inferred**: the list rebuilds). `panel::lower::crypt`.

### Placement and the panels' checks

Under `BUILD ANYTHING` the preview skips the disabled test and `BDEP`, not the
cap; the menu and the upgrade skip all. The upgrade's decode refuses a
disabled `UpgradeTo`, not a capped one; no other decode asks either. A
disabled button never sends; a short treasury does not disable one: the
decode refuses (red price).

The build preview asks, in order, each answer a refusal code:

1. disabled (6);
2. `BDEP` (5);
3. a placement not yet decoded (7: the marker unit `ABA0` holds it until the
   decode deletes it);
4. the `BTDT` cap (8);
5. its `PRIM` rectangle's corners on the map and 85% of its cells
   (`#construction_minpercent_notblocked`) with an explored corner (2);
6. its pad free and no standing building within `ConstructData1` (1). The pad
   test passes what the placement clears (scatter, a ruined or agentless
   building, an emptied chest), the margin test that too and any chest
   (**Game**: a building right next to a full chest is accepted);
7. for a code whose `BDEP` rule names ids, `CanIBuildThisBuilding(preview,
   the buildings of those codes)` (4, its answer a chat line: the wizards'
   tower out of a tower's or guild's range, a marketplace or trading post too
   close to another, an outpost too close to a palace or outpost).

`Game::placement` runs it as an interface run: no trace, no reveal, the
preview gone after (**Inferred**: the engine's searches see live ground, the
run's the last reveal's). Every panel query runs aside (`Game::aside`): VM and
world put back after, so how often the player pointed changes no later frame.
The engine keeps what such a run leaves (activations, object ids, the
preview's attribute objects); no script value shows those
(**Measured**).

The camp move pays at the press (`PlayerResourceMessage`, spend), then the
guild's preview asks the ground alone (2, 1); a click where it fits sends
`GUILD_ACTION` 0 with the point; giving up returns the price
(`PlayerResourceMessage`, add). The decode searches from the point with the
clearing ring, dispatches `KickoutRangers`, moves the guild, clears its ring
and dispatches `MoveCamp`; no flatten, no pad. The embassy's `GUILD_ACTION`
`0x16` shuts an open one (its timer gone), or dispatches `Embassy_Init` and
runs a 50 ms timer: each tick with a finished guild with room and `AEM:1` in
the treasury counts down `AEM:3` / 50 ticks into `AEM:2`; the last dispatches
`Embassy_Spawn` and starts again (`world::guild::camp`,
`world::guild::embassy`). No trace holds either.

A left double click in the select mode is the Rewards tab's shortcut
(`World::twice`). The window class asks Windows for double clicks, timed by
the OS; the first press selects, the second selects nothing. On no piece the
ground pick takes an explore flag at the point, kept on the map; on a piece:
the owner's ruin or dead unit is cleared (`DestroyVehicleMessage`), a message
flag read (`DestroyEventFlagMessage`), the owner's reward flag taken down
(asked first, `AP62`, while it holds a reward), else a flaggable one, any
owner's, takes an attack flag. Each flag at the panel's reward; the attack
flag only when the treasury holds it, the explore flag paid by the decode as
it can. A refusal sends nothing, no sound or chat line. The decode selects a
double click's flag for its owner. openregent sends the flags, timing at
Windows' defaults, 500 ms and 2 pixels each way (**Inferred**); it reads a
message flag (`Input::Read`), takes a flag down (`Input::Unflag`) and clears a
ruin or body (`Input::Clear`).

### Scroll bars

**Binary** unless marked. A scroll bar (type 9) names one set, the up arrow,
`INDj` 1000; set + 1 is the down arrow, set + 2 the thumb (`INDj` 1002,
25×10, the same size whatever the page). The thumb sits
`(pos − min) × (track − thumb) / (max − min)` down the track and drags.
**Game**: a list that fits shows neither arrows nor thumb (captures of the
palace and a guild); scrolled to the top, the thumb lies under the up
arrow (a capture). **Inferred**: the track is the bar less both arrows;
arrow slot 1 (pink) while pressed. Whether a track click pages is not read.
The quest pages' bars name `INDs` 1000: arrows 25×27, thumb 23×27, drawn
centred on the arrows (**Inferred** from a capture of a quest's start).
openregent's `scroll::Bar` holds the geometry for both.

### The message scroll

**Binary**: a selected message flag's panel is `AP77` (unit kind `MSGF`), in
the selection slot. `AP13`, also titled MESSAGE, is a yes and no question the
exe never names.

| Control | Is | Does |
|---|---|---|
| 4003 | the scroll, `INTK` 1000 | |
| 8005 | "MESSAGE", black | the controller leaves it |
| 5000 | text box 161×171 at (20, 32), `fnt4`, `0x3f0000` | `AITX[APM:0]`; 0 clears it |
| 5053 | scroll bar `INDj` 1000, tied to 5000 | |
| 5503 | CLOSE, `INBb` 1005 (stars painted in) | `ME01 Message_OK`, then `DestroyEventFlagMessage`, as a double click |
| 6 | `INTG` 1015, right | centres the view on the flag, then as CLOSE |
| 5005 | check `INTK` 1004, left | the tracking window option (default on): a new message flag followed ([the tracking window](#the-tracking-window)) |

No history: the side controls are a zoom and an option. **Game**
(a capture): the text centred, from the box's top; no scroll arrows
while it fits (**Inferred**: the bar shows them only when the text is longer).
A check draws slot 3 while checked (`menu::drawn_slot`), the down chevron.
`regent_core::view::panel::message`, `World::message_point`,
`the_message_scroll_is_ap77`.

A building's tax and repair settings are four flags, `APB:16` `IsTaxed`,
`APB:17` `QuickTax`, `APB:20` `IsRepaired`, `APB:21` `QuickRepair`: a press
flips its own (0 to 1, else 0) and clears its pair (guild actions 6 to 9). The
panel lights the route button while its flag holds, else the once button:
always, once, or neither, never both. No script runs: `collect_tax` visits a
building with either tax flag and `tax_building` clears `QuickTax` there; a
peasant repairs a damaged building with either repair flag,
`BuildingReachedMaxHP` clears `QuickRepair` (`world::guild::upkeep`,
`Input::Toggle`). A save writes a key once anything wrote it, so `APB:17` or
`APB:21` present names a pair pressed. **Game**: the tax and repair probe's 19
presses, its saves' flags key for key (**Measured**).

A flag comes down by its panel's remove or a double click: holding a reward,
`AP62` asks first (`GMTX` 20 and 23, "Are you sure you want to remove this
flag?"), YES 5501 sends; at 0 it sends at once (`World::removal`).
`DESTROY_REWARD_FLAG_MESSAGE` names the flag's handle; its decode deletes the
unit at once (the holder's flags stacked again) unless already queued for
deletion. No gold comes back (`World::take_down`).

### A hero's dismissal

Guild action 5, on a unit with an agent: `APV:28` `HasBeenDismissed` 1, then
`Unit_Dismissed` at once. Nothing else: no gold back, the guild's roster
(`CONT` relation 100) unchanged. The script takes the hero off its guild's
`Members`, homes it on the palace and sets `flee_map`: it walks into the
palace and deletes itself there (`Hide`, then `DeleteGamePiece`); with no
palace, to the nearest map edge. Only its deletion frees its guild's spot.
`world::dismiss`, `Input::Dismiss`.

The question `AP62` opens in the selection's slot, under the minimap. After
YES the panel opens `APb5`, the hero's follower panel in `textdata.cam`. A
later selection gets it too: a hero with `APV:28` set, and another owner's
unit whose type `CanUse` `HumanPlayer` (no henchman, `APb1` or `APa8`). It
holds name, portrait, hit points, class, LVL and level, zoom and tips: no
gear, experience, page buttons or dismiss. Control 9901 frames it in the
owner's colour, `INDe` 1010 + owner (blue, red, green, purple); 8030 says
`GMTX` 85 "Follows %s." of 251 "another monarch" (nothing for the monsters'
hero). 8002, 4002, 4006 and 8030 are hidden but on the player's undismissed
hero. `details::FOLLOWER_PANEL`, `Hero::shows`. **Game**:
captures of a dismissal; a played game, the wizard deleted after
it walks home; its saves the world's, rosters too (**Measured**).

### The Call to Arms

**Binary**; `world::guild::assembly`, `Input::Assemble`.

```text
 AP52 8009 CALL TO ARMS, C   disabled with no living member; 4481 the
                             price, red under it
 tooltip                     "Issue a Call to Arms ..." + GMTX 28 "- (%d)
                             gold"; none living: GMTX 260
 press                       price in the treasury: PlayerResourceMessage
                             spend, GUILD_ACTION 3 (handle, point); else
                             GMTX 37 "Can't afford a Call to Arms."
 decode, action 3            DoAssembly(guild)
```

The price is read once, `#AssemblyCost`; no cooldown, no timer. `DoAssembly`
is the effect: each living, unfrozen member of the guild's `Members` leaves
its building (`exit_building`, `spawneffect_out`) or has its tasks reset,
gets a 1,200 ms `teleport_effector` (`WRd1`), `TeleportToUnit(member, 50000,
guild, 0)`: the place search from the guild's entrance; then `ActiveScript`
`berserk`. Last, the guild plays `Call_to_arms`. The price's message decodes
after the guild action (**Game**, `err.log`). **Game**: the call to arms
probe, its four heroes at the door; its four saves the world's
(**Measured**).

### The Market Day

**Binary**; `world::guild::market`, `Input::MarketDay`, `World::market_day`.

```text
 AP31 8200 MARKET DAY, M    APB:1 0 hides 8200, 8201, 8826;
                            APB:22 set: 8201 a bar of now - APB:22 over
                            #Market_Day_Duration, 8826 shown, 8200 hidden;
                            else 8200 shown while APB:37
 press                      GUILD_ACTION 10 (handle, no point)
 decode                     a 0x4004 order running: its counter zeroed, a
                            duration more from its end; else APB:22 the
                            clock, DoMarketDay(market), then a timed order
                            of #Market_Day_Duration (45,000)
 end                        APB:22 0, EndMarketDay(market)
```

`DoMarketDay` pays `(duration / revenue_time) × revenue_amount`, ¾ fixed and
a draw of the last ¼ (`random_to_percent`), plus
`RandomNumber(#market_day_bonus)` × level, into the coffer by `Give_Gold`;
suspends the revenue thread, titles it `Closed`, releases its occupants.
`EndMarketDay` plays `Select`, resumes, reopens. The order is the save's
`ORDR` kind `0x4004`. **Game**: the market probe: of 125, 94 and a draw of 3,
plus 61 × level 1, 158 gold; its end in a later save's orders
(**Measured**).

### The extortion

**Binary**; `world::guild::extortion`, `Input::Extort`, `Game::extortion`.

```text
 AP48 8009 EXTORT (%d), E   GMTX 193 of total - cut; disabled while the
                            total is below 1. ShowExtortionTotal,
                            ShowExtortionCut: interface runs
 press                      sound EN01; total above 0: chat GMTX 194
                            "Extorted %d gold, the guild kept %d gold." of
                            total - cut and cut, then GUILD_ACTION 2
 decode, action 2           DoExtortion(guild)
```

No price, no timer. `DoExtortion` takes each owned building's and the guild's
`APU:2`, 20% of each hero's purse and a tax collector's whole
(`#Extortion_Hero_Percent`), the rest of a purse to `StoredGold`; keeps
`#extortion_percent_deduction` (35%), truncated, gives the rest by
`AdjustPlayerData`; clears the palace's `Tax_Route` and `Tax_Targets`, resets
the tax collectors, and hangs `got_gold_bldg` on the palace. **Game**: the
market probe: total 158 (the Market Day's coffer), cut 55, 103 to the
treasury.

### The Rage of Krolm

**Binary**; `world::guild::rage`, `Input::Rage`, `World::rage`.

```text
 AP24 8009 RAGE OF KROLM, K  APB:2 0 hides it, 4484, 8201, 8826; the
                             palace's APB:7 set: 8201 a bar of now - its
                             APB:52 over #Rage_Krolm_Duration (25,000;
                             45,000 at victory index 3), 8826 shown; else
                             8009 and 4484 (red when short), enabled with
                             a hero of the owner's
 press                       the price held: PlayerResourceMessage spend,
                             GUILD_ACTION 1 (handle, point), 8009 disabled;
                             else GMTX 36 "Can't afford to Rage."
 decode, action 1            the palace's APB:7 0: its APB:52 the clock,
                             DoRageOfKrolm(temple)
```

`DoRageOfKrolm` gives each living hero a `Rage_Icon` of the duration, +10 to
four stats and hit points, a speed trail, and the palace's `APB:7` the count;
each icon's end runs `Rage_end`, one off. The guild action decodes before its
price (**Game**, `err.log`). A press while one runs keeps its price
(**Inferred**). **Game**: a played game's five Rages, every line and save;
captures of the temple.

### The Hall of Champions' bounty

**Binary**; `world::guild::bounty`, `Input::Bounty`.

```text
 MX00 8851 Place Bounty, B   opens MX01; while a bounty runs (APP:2 set,
                             now - APP:4 within APP:3) 8851 hides, 4500 a
                             bar of now - APP:4 over APP:3, 4501 GMTX 412
                             "Calling Champions"
 MX01 list 5000              the slain types
 2050 S, 2051 L              each its price (HallOfChampions_Bounty_Cost(1
                             or 2), an interface run: 400, 800), disabled
                             when short; a press: GUILD_ACTION 0x14 (code,
                             index, hall)
 decode, action 0x14         Bounty_Period then Bounty_Cost, interface
                             runs; the price held: taken, APP:2 the code,
                             APP:3 period - 350, APP:4 the clock;
                             Champion_Icon for period - 350 on each monster
                             of the code (unit list 1); a Wait on the
                             palace
 end                         APP:2 and APP:3 0
 SetHallFlag(unit)           a slot 7 unit of the first bounty's code
                             (players 0-3): Champion_Icon for what is left
```

Heroes chase marked monsters (`CheckEffector(.., "Champion_Icon")`). Monsters
are marked in agent order (**Inferred**; the probe's two icon handles agree).
**Game**: a played game's long bounty on ratman catapults (`BVq1`): hall
words, both icons, the palace's `Wait`, every save;
captures of the hall.

### Renaming

**Binary**; `World::naming`, `World::rename`, `Input::Rename`.

```text
 8005 pressed     the local player's unit: a hero with APV:11 below
                  #HeroNameEditingLevel (HD: 1, the expansion's) says
                  GMTX 192 of it; else the box edits
 Enter            SET_UNIT_NAME_MESSAGE 0x3f4: the handle and the text
 decode           the unit's own name string; AP:1 untouched; no dispatch
```

A save writes the string as `SECD`'s name; a load keeps it. The world holds
it as a given name (`World::given_name`). **Game**: a played game's
warrior, saved "Sir Swordsword", its `AP:1` its recruit's draw.

### A flag's seekers

**Binary**. A hero `check_rewards` picks a flag for is appended to the flag's
`heroes` list and sent by `SetParent(hero, flag)`: the flag joins the hero's
`HOME` (101), the hero the flag's `ROSTER` (100), last. `ClearFlag`, or
another flag, takes it off. The engine reads the relation, not the scripts'
list or `APR:0` `FlagSeekers`; a dead hero still holding it counts.

| Where | Shows |
|---|---|
| `AP46`, `AP80` 8019 | the count |
| 8995 View Heroes, V | opens `AP87` (attack) or `AP86` (explore) below; dimmed at 0 |
| 8014 | `GMTX` 180 "Offered by %s.", the flag owner's name; a solo rival is `GMTX` 251 "another monarch", a LAN rival its seat's name |
| `AP86`, `AP87` | a unit list as a guild's members: title 1088 `GMTX` 182 "%d Heroes Attempting", rows icon, level, `GMTX` 220; SELECT (S) selects the row's hero, 5001 (Z) centres on it, a row picked has the tracking window follow it |
| the minimap | the seekers white while the flag is selected ([minimap.md](../original/minimap.md#units)) |

Rows go by the hero lists' sort mode (prefs record 11, set in the heroes
roster): name, level or type; the default 0 leaves the order undefined
(**Inferred**: openregent keeps the relation's). The rival panels `APb6`,
`APd0` have the offer line but neither count nor button. `World::seekers`,
**Measured**; played: **Measured**, six seekers at its last save
in the save's order.

### The palace's counts, statistics and rosters

**Binary**. `AP39` (and the outpost's `MX08`) shows three numbers beside the
guard, tax collector and peasant icons (9 to 11): `GetSupportedHenchmen(palace,
i)` into 14, 13, 12 for i 0, 1, 2 — an interface dispatch of the scripts'
maxima: `max_peasants`, `max_tax_collectors` plus the owner's fountains,
`Max_guards`. **Game** (a capture): a level 1 palace 0, 1, 2. An icon
opens the henchmen roster. `world::kingdom::Henchmen`,
`Game::supported_henchmen`, **Measured**.

STATISTICS (`AP40`) and ROSTER (`AP85`) are `UIData`'s two-slot pages,
202×490 from (0, 182); back 8013 (Tab) opens `AP39` again. The frame's
middle, `INse` 1013 (202×16), has a taller rectangle and key 43 2
(**Inferred**: repeated down it).

| Page | Shows |
|---|---|
| `AP40` | list 5000, rows of 15: rows 0 to 25 of the local player's record, `GMTX` 87 to 108 — Riches, Total Made (gold in less initial), Gold Balance (`APP:0`), Gold Initial, Total Spent and its five kinds, Buildings Constructed, Destroyed, Standing, Heroes Recruited, Killed, Alive, Average Level, Vice, Danger Level; rows 10, 15, 21, 23, 25 a rule |
| `AP85` heroes | the palace owner's heroes, none in a Mausoleum: level, `GMTX` 220 |
| `APc1` buildings | the owner's buildings with an agent (no ruin), Sewers and Graveyards aside: `GMTX` 216, a guild's 217; repair and tax marks at the row's right (`INBb` 1037/1038, 1035/1036, neither the route's slot 2; no tax mark where `IncomeType` is 3); a row picked centres the view |
| `APc2` henchmen | the owner's henchmen: `GMTX` 218, a tax collector's 219 |

A roster's tabs 5013, 5012, 5011 switch between them; check 5004 the detail:
off, rows of 16 with the name alone; on, rows of 40 (henchmen 32). Radios
5005 to 5007 sort (one setting for all rosters): name, byte order; level,
highest first; type, guilds first, then the code as a little-endian word.
Mode 0, the start, sorts by nothing (**Inferred**: the list's order); it
lights the name radio. SELECT selects the lit row. `world::kingdom`,
`World::roster_of`.

The record (`GSGL`'s player record) is kept as the game runs: built (the
player's, a script's), hired and its price, killed (a hero deleted, not
dismissed), danger (a henchman deleted), gold in (every add, the setup's
too), spent on research, buildings (price, upgrade, enchant, gamble), heroes,
spells, flags (a flag's reward as it is made, and each change `APR:1` takes,
so a new flag's reward counts twice), initial gold. Every played save's
record is the world's (`world::ledger`; **Measured**).

## The front end

`regent_core::view::menu`, `crates/regent_core/tests/menu.rs`; `regent_cli
menu` prints the panels (`--quest QU01` a quest's art, `--dump` the
pictures).

### What a control's keys say

**Binary**:

| Key | Holds |
|---|---|
| 2 | rectangle, from the panel's corner; the panel's own at (-1, -1) is centred |
| 5, `0x100`+n | hot key, slot 0 and n; the language's slot is read ([hot keys](#hot-keys)) |
| 6 | control id |
| 7 | text: the panel's `STRT` entry |
| 11, 18 | palette, font |
| 12, 13 | picture: `IMAG` record and set |
| 33 | the tooltip: a second `STRT` text, shown on the [tooltip line](#the-tooltip-line) |
| 36 | text colours by state, `0x80rrggbb` (role 3) |
| 49 | the hot key letter's colours (role 4): the label's first letter that is the hot key |

**Game** (a capture of the main menu): the blue letter is the hot key's: CINEMATIC's
'I', none on EXIT GAME (Escape). The capture's blue (0, 0, 224) and navy
(0, 0, 48) are the data's `0x0000ff` and `0x00003f` dimmed.

### A control's text colours

`regent_core::view::menu::colour`, `tests/menu.rs`. **Binary**: a role's
colour at the control's state is its own list entry, unless the word has
`0x40000000` or `0x20000000` set or the control has no list; then the
parent's; then the dialog's style; else its own word (`0x80000000`, black,
with no list). A list shorter than three leaves the other states
`0xc0000000`.

The style is the panel's own palette, key 11 of its block: role r at state s
is entry e + s (**Binary**).

| Role | 0 fill | 1 light | 2 shade | 3 text | 4 hot key | 6 | 8 | 9 | 10 | 11 | 12 |
|---|---|---|---|---|---|---|---|---|---|---|---|
| Entry e | 16 | 19 | 22 | 48 | 51 | 54 | 57 | 60 | 32 | 35 | 38 |

Role 7 copies role 0; role 13 takes entries 64 to 66 as screen words. Every
panel but `AP75` (none) names `MMS1`: text `f8b152` orange at rest, white lit
(held), `db2124` red disabled; the hot key letter `523bff`, `00a3ff`,
`de0000`. So a text that sets no colour, as the Gambling Hall's "Wheel is
Spinning...", is orange, a disabled VISITORS' "Nobody Here" red, a held
RESEARCH white with a light blue R.

**Game**: the captures' letters are about 7/8 of these
(`d89848`, `c01c18` in a gambling hall capture). **Inferred**: the
bitmap font's brightest shade, as the main menu capture's `0x0000ff` at 224.

### Hot keys

`regent_core::view::menu::hotkey`, `tests/hotkey.rs`. **Binary**: a key is the
character `WM_CHAR` gives, upper case ('R' 82, '?' 63, Tab 9, Enter 13,
Escape 27); keys no character types are `0xe000` + n: F1 to F15 `0xe010` up,
the cursor keys `0xe001` to `0xe00a`. Not Windows virtual key codes. Shift,
Ctrl and Alt ride in the high word. A key goes to the focused control first (a
text box types it), then to the first control in panel order that is shown,
not disabled (key 3 bit 4) and has it; that control is pressed as a click
presses it. Case and Shift do not count for a character key ('?' is Shift and
'/'); Ctrl and Alt do.

| Panel | Keys |
|---|---|
| a guild | R recruit (8008; the warriors' guild 5001, its paladin P, warrior W, warrior of discord D), H heroes, S spells (temples) |
| a building | X demolish, ? tips, U upgrade, V visitors, R research; M move camp, E extortion, K rage of Krolm, C call to arms, M market day |
| a hero `AP20` | E spells, I items, T stats, X dismiss, ? tips |
| the palace `AP39` | R rewards, B buildings, S statistics, O roster, U upgrade |
| a flag `AP46` | + raise, X remove, V its heroes |
| a page | Tab back; build B, rewards A and E, members S select, Z zoom, D details, the Mausoleum's R resurrect, research its rows' letters |
| `APMK` | Escape OPTIONS, F1 the palace, F9 names, F10 hit point bars, F11 zoom, F12 the minimap's textures |
| `AP62` | Y, N |

A key no control takes goes to the map, switched on its code alone
(`regent_core::view::map::map_keys`; **Binary**):

| Key | Does |
|---|---|
| Space | an attack flag on the selected piece of another owner, or on a selected flag's target; one of the player's there comes down instead (`World::space`). The flag is sent as a double click's: its decode selects it. openregent pauses instead ([openregent/game.md](../openregent/game.md)) |
| Backspace | the view on the player's unit that last played a sound at distance 10,000 or 10,001 (`view::sound::speaks`) |
| `` ` `` `~` | `ME01`, then the selected message scroll read |
| `<` `>` | game speed one step along `runtime::PACES` (`runtime::stepped`); not in a network game |
| Delete | the mode left |
| arrows | the view jumps `ScrollSpeed` (32) pixels at the next draw, each key event, auto-repeat too; the window's edge scrolls smoothly (`view::map::pan`) |
| `"` | the selection's `EG01`, at it, as its owner's (`view::sound::voice`) |
| 0 to 9 | a spell key: Ctrl binds the aimed spell, alone aims it where its button is ready, no gold asked |
| F2 to F8 | a focus key: Ctrl binds the selection, else the view's centre; alone selects (a ruin not), Shift also centres, Alt also has mini view 1 follow |

The bindings are the game's `HotKeyManager`: made at the first key, deleted
with the game, saved nowhere. Its third list, buildings', holds no key. Enter
opens the chat.

### The panels

| Panel | Is | Frame | Controls |
|---|---|---|---|
| `AP29` | main menu | `INDm` 1000 over the room `INDm` 1009 | buttons 5002 Play, 5003 Multiplayer, 5006 Credits, 5010 High Scores, 5001 Load, 5504 Settings, 5005 Cinematic, 5004 Exit: `DEM2` 1006, 140×26, `fnt4`; text 10 "Version #" at (162, 498) |
| `APc9` | briefing | `INDb` 1002, advisor and logo painted in | 5000 Continue, 5001 Cancel (`DEM2` 1001); 5002 background text, `fn11`; 8886 porthole, 260×260 at (31, 137); 8885 title, `fn11`; 8730 icon; 9101 scroll bar |
| `AP44` | summary | `INDb` 1000 | 5000 Start Quest, 5001 Cancel; 5002 background, 5003 goals, both `fnt4`; 8720 still, 200×200 at (62, 166); scroll bars 9101, 12 |

A control's picture set holds a slot per look. **Binary**: a control's state
is 2 disabled (flag 4), else 1 lit (flag `0x10`), else 0; the text colours
index by it. The slot drawn is 3 for a checked check or radio box, plus 6
when the control is the hovered one or, on the main menu, its partner is
(below), plus the state; a slot past the set's count loses 6, then 3. Every
kind alike (`view::menu::drawn_slot`, `State`).

A press lights the control. The dialog keeps the control a button went down
on, if enabled; each pass it is held while the pointer is on it, else not;
letting go releases it, which also clears a panel's lit flag. Held under the
pointer a control draws 6 + 1 = 7, in a seven-slot set 1 (`State::pressed`).
**Game**: a capture's held RESEARCH (`INBb` 1006, seven
slots) is slot 1, white (**Measured**). Which
mouse buttons a control takes is not read.

| Set | 0 | 1 lit | 2 disabled | 3 checked | 6 hovered | 7, 8 |
|---|---|---|---|---|---|---|
| `DEM2` 1006 button | grey | white | dark | dark | light blue | white, none |
| `INTN` 1001 spell | icon | inverse | grey | icon | bright | inverse, grey |
| `INDk` 1003 radio | clear | X | grey | X | clear, lit | X, grey |

The lit flag is set in the panel data, the palace roster's current tab
(`AP85` 5013, `APc1` 5012, `APc2` 5011), its frame blue (**Game**:
a capture), and by a press (above).

Key 3 words are OR'd into a control's flags (`smnu::FLAGS`): `0x80` the
pointer passes through (the hit test skips it), 2 a click ends the dialog with
its id. The control under the pointer is the last drawn holding it
(`Layout::under`).

All three are 800×600; the briefing pages are centred. Behind them a fill
covers the screen with `INse` 1023 (`addinterface_se.cam`, 770×550), repeated
from the top left: 7×5 cells of 110, each a unit's set 1000 portrait at
(5, 5) on black, toned sepia by its light (**Corpus**, `view::menu::faces`).
Fonts are `IMAG`s of 256 slots, slot = character: `fn11` "Majestica 12",
`fnt4` "tahona 7"; a glyph's advance is its block 1's `x1`. **Game**: "The
Barren Waste" is 242 pixels of ink in `fn11`, its advances 244. `fnt4` lines
are one pixel a glyph wider than their advances, spaces not counted ("Build a
Fairgrounds to win this quest." 231 by advances, 262 of ink); **Inferred**:
`fnt4`'s slot 0 block 2, `x0` −1, is that spacing (openregent `menu::font`).

The captures show the base game's panels: the main menu is `textdata.cam`'s
`AP29` (at least 95% of the room's pixels; the expansion's `IX51` room, snowy,
under half; **Measured**),
the Barren Waste briefing `textdata.cam`'s `APc9`. The expansion's `AP29`,
`APc9` and `AP44` add a logo cover, `IX72` 1004 and 1003. **Binary**: a
quest's briefing applies the quest's dataset, then makes `APc9`. Start-up and
every return to the menus apply the expansion's, which opens
`mx_textdata.cam` after `textdata.cam`, and the library manager asks the last
opened first. **Game**, the user: the captures are Majesty Gold HD, which
always runs with the expansion; the base panels showing is HD's behaviour with
it loaded. Which code makes the base `AP29` win is open. openregent uses
the base `AP29` by default (`[ui] main_menu`), and a quest's own dataset's
briefing pages.

### The mods row

**Corpus**: twelve `UIData` files hold one `AP29`, 800×768: the base buttons
(EXIT GAME a pixel right), and below y 600 a row of `INx1` frame pieces round
a tiled strip, `INx1` 1008 (19, 625, 762×38). On it CHANGE MODS 9000 and
ACTIVATE MODS 9001, `DEM2` 1001 (235×26), each in an `INx2` 1011 surround
(281×31); all base `addinterface_se.cam`. **Game** (a capture):
the strip is drawn over the four corner pieces it overlaps, though listed
first. openregent draws this row under the base `AP29` with three buttons
(openregent `menu::row`). `view::menu::row`,
`the_uidata_main_menu_adds_a_mods_row`,
**Measured**.

### The main menu's room

**Binary**. `view::menu::room`, `tests/menu.rs`.

Each button has a room object, a picture control over the room, and the
expansion adds picture-less hit regions, its triggers. A click on any of them
ends the menu as the button would:

| Button | Picture: base `INkm`, expansion `IX50` set | Triggers |
|---|---|---|
| Play Game 5002 | 5100, the map, 1003 | 7101–7104 |
| Credits 5006 | 5101, the book, 1001 | 7201–7203 |
| Multiplayer 5003 | 5102, the window, 1002 | 7401 |
| Load Game 5001 | 5103, the chest, 1006 | 7501 |
| Exit Game 5004 | 5104, the candles (base), 1000 | 7801 |
| High Scores 5010 | 5105, the bowl, 1005 | 7701 |
| Cinematic 5005 | 5107, the painting, 1007 | 7601 |
| Adjust Settings 5504 | 5108, the horn, 1004 | 7301 |

(Objects named from the art; the expansion's 5104 covers the whole room and
loops.) Two hover rules:

- **Pairs**, HD with a dataset's `AP29` (HD's flag set, no control 9000): on
  buttons and pictures. Entering one starts its partner's slot-6 playback;
  leaving, slot 0's. Each draws at slot 6 while its partner is hovered: an
  object lights its button light blue.
- **Triggers**, otherwise (`MajX`, HD's `UIData` panels): on buttons and
  triggers; entering starts the picture's slot-6 playback. The expansion's
  pictures are passive.

A picture rests on frame 0 of slot 0, the room painted under it
(**Measured**). Slot 0's playback runs back to
frame 0 and stops, slot 6's forward to the last and holds; the frame carries
over, so letting go mid-run turns back. 40–50 ms a frame, 21–30 frames. The
base candles loop both (100 ms, 50 ms lit), as do the expansion's 5104 and
111 (`IX50` 1009, in no table). A frame steps once a pass when its period has
run (`view::menu::Animator`). **Inferred**: under Pairs a hovered object plays
its own run; the pair rule leaves its playback alone and draws slot 6 plus its
state. The user reports the objects moving under the pointer.

Exit's 3000 (`INkm`/`IX50` 1008) is hidden; after the quit dialog (`AP04`,
"Really exit Majesty?") says yes it is shown and played forward to its last
frame at its set's period (`INkm` 1008, 30 frames at 50 ms), the menu passing
until then, and the game ends (app state 9; **Binary**). `menu::quit`, `room::LEAVING`,
`room::left`, `the_quit_question_is_yes_or_no`; openregent's `Quit` and
`Leaving` pages. The front end's cursor is `CUR1`'s `Dial` row, whose state
is always 0: hovering changes nothing.

### A quest's pictures, video and voice

**Binary**: the game globals hold 34 quest codes, `QU01`–`QU19`,
`XQ01`–`XQ12`, `XQD1`–`XQD3` (`quest_map::SLOTS`). A quest's slot n names:

| What | Record | Where |
|---|---|---|
| video, in the porthole | `QM`n (`QM00` for `QU01`) | `PICT`, `cinedata1.cam`, `mx_cinedata1.cam` |
| voice | `EQ`n | `WAVE`, `voices.cam`, `mx_voices.cam` |
| still (8720) | `INTP` base, `MX29` expansion; `IXR`1–3; `DXR0` a quest outside the table | set 1000 + n, less 19 for the expansion; freestyle 1012 |
| icon (8730) | `INTD` base, `IX11` expansion; `IXD`1–3; `DX00` outside the table | as the still |

`XQD1`'s video and voice do not ship. The voice is a `DSND` of the code,
played in its first phase as `APc9` opens, unplaced, music 6 with it;
Continue or Cancel stops it and every sound. `AP44` has none. HD's `MD01` and
`VAMP` are outside the table: icon `DX00` 1000, still `DXR0`, which no archive
holds, no video. The quest map's pog is the same icon (`quest_map::pog` calls
`menu::icon`). Until the video plays the porthole shows the frame's painted
advisor (**Game**, a capture).

**Binary**: the video starts as `APc9` opens, after the music and voice, at
control 8886's corner, 1:1 (260×260, its own ring painted in), no black fill.
Played once, paced by `BinkWait` in the dialog loop; the last frame drawn
stays until Continue or Cancel closes it. **Inferred**: the window inactive,
the message pump blocks and all stops. openregent: `menu::cinema`,
`paint::View::video`.

### The loading page

**Binary**. `menu::LOADING`, `menu::segue`,
`every_game_has_its_loading_still`.

`AP50`, 800×600, centred, is made after the game's dataset is applied and
stays until its world is made: by Start Quest, a pick page preset, `MX17`'s
Accept, or a network start. A save's load makes none.

| Id | Is | Art |
|---|---|---|
| panel | frame, the bar's trough painted in | `INDg` 1000 |
| 5000 | still, 750×375 at (24, 97) | the game's, below |
| 8730 | icon | `menu::icon` |
| 8885 | title, `fn11`, `0xffddaa` | the quest's title; freestyle `GMTX` 80 |
| 1 | the still's border | `INDg` 1001 |
| 5020 | progress bar, 647×20 at (81, 556) | `INDl` 1000: no such set (it holds 1004, 1006) |
| 2 | **Inferred** a bar cap, 18×36 at (723, 549) | `INDg` 1002 |
| 3 | snow logo cover, expansion only | `IX72` 1002 |

The still:

| Game | Record | Set |
|---|---|---|
| a base quest | `INTM` "pre speia segue" | 1000 + slot |
| an expansion quest | `IX73` "sepia segues" | 1000 + slot − 19 |
| `XQD1`–`XQD3` | `IXS1`–`IXS3` | 1000 |
| `MD01`, `VAMP` | `DXS0` | 1000 |
| freestyle | the panel's own: `INTM` base, `IX73` expansion | 1012 |

A freestyle start sends 5000 only the set. `INTM` 1012 is also `QU13`'s.
`INTM` 1040 and `IX73` 1013 are in no rule. No text of its own: no
"Loading", no tip. No sound: the page keeps the music playing. The bar moves
only while the world's charts are generated (message `0x29` to 5020).
openregent draws it empty.

`PICT SPLH`, the 800×600 TGA, is the start-up still after the intro videos,
not a loading picture. It and the intro come from the cinematics' archive
([credits](#credits)), not the datasets.

### Music and clicks

**Binary** ([sound.md](../original/sound.md#the-front-end),
`view::menu::music`): `AP29` and the map play track 2, `APc9` and `AP44`
track 6; other screens keep what plays. A press going down on an enabled
control clicks by its class, from no `SMNU` key: `CK05` for all, `CK04` check
and radio, `CK07` list box. No hover, room object or page change sounds.

### Credits

**Binary**: CREDITS plays two `PICT` Bink videos, `MV06`
(`mx_cinedata1.cam`) then `MV01` (`cinedata1.cam`), full screen (640×480
unless `-keepintroresolution` or Direct3D), each until it ends or a key or
mouse press stops it, then the main menu again (`view::menu::CREDITS`). No
panel, no text, no music call. **Corpus**: no `STRT` holds credits.

How a full-screen video plays: the cursor hidden; the screen filled black and
the video centred on it; unless windowed, `-keepintroresolution` or Direct3D,
the display set to 640×480 for it, so it fills the screen. A busy loop paced
by `BinkWait`; a queued key (characters, F1–F15, the cursor keys; not a
modifier alone) or a left or right press going down stops it. It stops when
Bink's frame number reaches the count less one (`view::menu::shown_frames`);
**Inferred**: Bink counts from 1, so the last frame is never drawn. Bink
streams it from the archive by a file handle, converted straight to the
screen's 16-bit format. A missing id plays nothing and says nothing.

The other two full-screen screens, from the cinematics' archives
(`view::menu::CINEMATICS`; `WhichCinematic` 1, the default,
`cinedata3.dat` and `mx_cinedata3.dat`; 0 the half-size `2` pair):

| Screen | Plays | Music |
|---|---|---|
| start-up | `MV02`, then `MV04` (HD), then the `SPLH` still; no archive, none of it. Skipped by `-nointro`, `IntroVideo` 0 | no call |
| CINEMATIC 5005 | `MV04`, then `MV07` once the expansion's ending was seen; no archive, `AP11` (texts 351, 352) | stopped, then track 2 |

Each video is stopped by a key or press as above (`view::menu::INTRO`,
`view::menu::CINEMATIC`). `MV07` is the expansion's ending, which the end
panels ask but never find ([the game's end](#the-games-end)).

### The high scores

`regent_core::view::menu::scores`, `regent_core::sim::world::score`,
`tests/scores.rs`. **Binary**.

A won game only, never a loss, is scored, with its game kind: 2 a quest
(Krolm and Vampiric Revenge too), 1 a custom game, 0 a network one. Per
category, the best of players 0 to 6 in a network game, else player 0, the
first of a tie:

| Category | Control | Value | Recorded |
|---|---|---|---|
| MOST GOLD | 6000 | the treasury at its highest after an add; zeroed at start, no save holds it | always |
| LARGEST CITY | 6001 | the player's buildings in unit list 0, no other test | always |
| HIGHEST LEVEL | 6002 | the champion: level, owner, name and class kept in the player's stats (saved in `GSGL`), replaced by a standing, not-start hero of higher level whenever it is computed (the statistics pages too) | level not 0 |
| BEST TIME | 6003 | the level timer, game ms | always |
| MOST HEROES | 6004 | heroes hired | above 0 |

An entry: the player's name, the game's title ("Freestyle Game" for a custom
game), the value, a hero's name and class. A list keeps 10: past it the worst
goes, the first stored of a tie. The store sorts each list by its value,
greatest first; BEST TIME has no sort key, so it sorts by the name read as a
number, which leaves it in the order won. The file is `hiscores.sav` under
the user directory, read before each record and as the screen opens, written
after and as it closes; its layout is not read. openregent keeps its own,
`$XDG_DATA_HOME/openregent/scores.toml`.

`AP64`, 800×600, from HIGH SCORES (5010, the bowl 5105, 7701): frame `INDh`
1000; title 8005 "High Scores" (`fn11`); 12 the category's `GMTX` 175 to 179;
list 8001 (`fnt7`, rows 20 high: key 28), scroll bar 13, a row each entry in
`GMTX` 170 to 174:

| `GMTX` | Arguments |
|---|---|
| 170 gold, 171 city, 174 heroes | name, value, title |
| 172 level | name, class, hero, level, title |
| 173 time | name, title, the day text of the value ([the day counter](#the-day-counter)) |

Radio boxes 7002 EPIC QUESTS, 7000 MULTIPLAYER, 7001 FREESTYLE (keys E, M, F)
pick the game, 6004, 6000 to 6003 the category; it opens on EPIC QUESTS,
MOST HEROES. ERASE SCORES 9001 empties the shown game's five lists, asking
nothing; MAIN MENU 9002 (Escape) writes the file and goes back. The pog 8730
keeps the panel's `INTD` 1039. No music call.

## The quest map

HD's quest picker is `SMNU` `APdb`, only in the `UIData_<w>_<h>.dat` files (11
of 13 hold it); without it the engine falls back to `AP30`, the 800×600 map.
**Binary** unless marked. `regent_core::view::menu::quest_map`,
`tests/quest_map.rs`, `regent_cli quest-map`.

The panel's own picture is the map: `IX10` set 1001, one 1398×1611 `TILE` in
`mx_interfacedata.cam`. Base and expansion quests share it; their buildings
are painted in, so an unhovered quest draws nothing of its own. Map positions
are the same at every screen size; only the frame moves.

A quest has a slot `s`, 0–33, in the game globals' table: `QU01`–`QU19`,
`XQ01`–`XQ12`, `XQD1`–`XQD3`. Wrath of Krolm and Vampiric Revenge hold none,
so the map does not show them. Per slot:

| Control | Holds | Shown |
|---|---|---|
| 4000+s | 100×100 hit region, no picture | always; the downloadable three only when installed; disabled while locked |
| 4100+s | lit picture: `IX33` (31), `DX31`–`DX33` (160×160) | `IX33` slot 1 while 4000+s is hovered; slot 0, a wooden compass, never shows (**Game**) |
| 4400+s | star, `INQd` 1001, 28×26 | the quest has a victory |
| 7400+s | "?" cloud, `INQc` 1001, 180×180, seven slots | its `QDEP` rule is unmet |

A press on 4000+s starts the quest. **Inferred**: the hit test is the
rectangle, there being no picture. Two lit pictures are smaller than their
rectangles (Dark Forest 2 rows, Elven Treachery 3 columns).

A painted lit picture is the map's art at its corner, unshifted, under a warm
glow fading to nothing at its edge, in a palette of its own: within 2 px of
its edge it differs from the map by at most 13 a channel (the palettes,
**Inferred**), inside red rises 28 or more. No smooth brightening or tint of
the map reproduces it (fits leave 8–16 a channel, the glow keeping dark parts
dark and saturating the ground), so it is the map plus a per-pixel difference
(`a_painted_lit_picture_is_the_maps_art_under_a_glow`). One global colour
table fitted to all 31 (17³ RGB, trilinear), faded in from an ellipse filling
0.9 of the rectangle as Φ((d − 16) / 8), leaves 14.0, 11.6, 8.5 a channel;
openregent draws that instead, a departure (`regent_draw::art::glow`,
[openregent/game.md](../openregent/game.md)). A downloadable quest's picture,
`DX31` (only `XQD1` installed here), is its building over the map's art (the
map has none there) in slot 0, and slot 0 under the same glow in slot 1,
within 3 px of its edge under 2 a channel
(`a_downloadable_lit_picture_is_its_slot_0_under_a_glow`).

Key 10 bit `0x100000` marks the downloadable hit regions and the Siege's. It
does not hide: **Game**, the Siege is hoverable and clickable from the start.
Its meaning is unknown. Key 33 indexes `STRT APdb`, a designer's label.

The panel draws in list order, and lists each 4100+s before its 4400+s and
7400+s; hovering plays the lit picture and touches no cloud. So a hovered,
locked quest shows lit under its cloud (`quest_map::marks`; **Game**: Vigil and
the Tomb in captures).

Hovering 4000+s fills the title bar:

| Control | Unlocked | Locked |
|---|---|---|
| 8730 pog | `INTD` 1000+s (base), `IX11` 981+s (expansion), `IXDn` 1000 | same |
| 8885 title | `STRT QUES` title | `GMTX` 268 "Secret Quest" |
| 5001 line | `GMTX` 136 "Difficulty: %s"; won: 137, victories and best time | `GMTX` 138 "… Requires: %s" |

Difficulty is `QDEP`'s, 0–3, shown as `GMTX` 139, 140, 141, 419: Beginner,
Advanced, Expert, Master. Requirements are the rule's ids in order, each
`GMTX` 323 (grey, won) or 324 (pale, not won). **Game**: both captures agree.
Leaving shows `INTD` 1037, which does not exist: no pog.

The video is `QM` and `s` in two digits (`QM00`–`QM30`, `PICT`).

The frame is 23 controls that stay on screen while the panel scrolls: `INx2`
1000–1011 from `addinterface_se.cam` (three bars with key 10 `0x40000`,
tiled), the pog, title and line, Erase Victories 5002, the compass 5000
(freestyle, `AP63`), Main Menu 8003. Its size is control 8504's far corner:
1398×1200 at 1920×1200. **Corpus**: every shipped frame (1280 or 1398 wide,
768 to 1200 high) is the 1920×1200 one with each edge held at the frame's
start, middle or end (`quest_map::anchor`, `moved`), within 1 px; the bottom
bars run from the corners to the centre piece 8506. The title and line boxes
alone fit no rule (right gaps 31 and 23, 32 and 22). FREESTYLE QUESTS' text
is set in its key-42 rectangle, (4, 42) 168×20 from its corner, the slot at
8506's foot. On opening the frame is centred and the map centred under it
(map y −205 at 1920×1200); a cursor within 8 px of the frame's edge scrolls,
as in play. Behind everything, `INse` 1023, 770×550 sepia faces, tiled over
the screen.

Control 4034, Load Downloadable Quest (`DX00` 1001, the jewel at the upper
left), opens `APd8`. **Game**: Krolm and Vampiric Revenge are reached there.

`APd8` (`view::menu::extras`) is 800×600, centred: pog 8730 (`DX00` 1000),
title 8005 "Downloaded Quests", list 9100 (13 rows of 20, as `AP27`) and bar
9101, description 9004 (426×72), LOAD QUEST 9000 (Enter) and CANCEL 9001
(Escape). The list holds each quest outside the table, sorted by title, case
kept, ties in record order; row 0 lit. A pick sets 9004 to two lines: `GMTX`
136, or 137 once won, then the record's `<Short>`; the first line alone with
no `<Short>`. **Game**, a capture: The Wild, The Wrath of
Krolm, Vampiric Revenge, in that order; "Difficulty: Expert" over Krolm's
goals, from the box's top, the fifth line cut. Krolm's title is
`gpltext.cam`'s `QUES`: the expansion's lacks `MD01`, and `QUES` is merged
across archives (`data::text::MERGED`).

The list box takes keys before the hot keys once a press has focused it: Up
and Down move the lit row, PgUp and PgDn a page, Home and End scroll, a
letter lights the next row it starts (`view::menu::listbox`).

Hovering a picture control draws slot 6 plus its state and starts that slot's
playback; leaving plays slot 0's. The jewel's slot 6 is its lit tile, one
frame. The compass, `INPq` 1039, plays slot 6's nine frames looping at
150 ms; the compass and its text 5900 light it from either. Both fill the
title bar (`quest_map::banner`): the jewel pog `DX00` 1000, title `GMTX` 387
"Downloadable Quests"; the compass `INTD` 1036, `GMTX` 80 "Freestyle Game".
**Game**: captures of both hovered.

### The freestyle pages

**Binary**. `sim::mapgen::preset`, `freestyle::random`,
`freestyle::randoms`.

The compass 5000 and its text 5900 end the map screen with `AP63`, which
opens not the base game's `AP63` panel but `SMNU` `APd2` (251×323, centred;
**Game**: a capture). Opening, it rolls the Map Number:
`rng_next(999999999)` on an RNG seeded from `timeGetTime`.

| Control | Does |
|---|---|
| 5000, 5003, 5007 Beginner, Advanced, Expert Random | a preset, then the game; after it, the main menu |
| 5004 Manual Settings | `MX17`: Accept 5000 the game, Cancel 8003 the map |
| 5005 Cancel | the map, `AP30` |

A preset writes the freestyle config: the band's tag (`BEGI`, `ADVA`, `EXPE`)
into all three forces, `RAND` into the land, victory 0, gold
`#starting_gold_easy`, `_normal`, `_hard` (20,000, 15,000, 10,000). On a
second clock RNG it draws one unused number, then event 1 from `EVSC` rated
0–50 (`NONE` kept), 0–80 or 0–100, and event 2: Beginner on
`rng_next(100) < 50`, Advanced `< 25`, from the same list, another code;
Expert always, from that list with 45–100 appended (the list is not
cleared), so its two may repeat.

The config keeps a tag; the game's setup draws the code, the world RNG seeded
with the Map Number, one `rng_next` per Random list in order land,
settlement, scenario, monsters, each list in its map's key order: a land
whose team is active; a settlement, scenario or monsters entry of its kind,
rated in the band (`DIFFICULTY_BANDS`), not flagged `RGC8`, the scenario and
monsters for one player. The charts reseed, so the draws move nothing else.
**Inferred**: the player count tested is the session's, one.

The config is one object for the session: the presets and the dialog write
it, so Manual Settings opens on the last game's settings, a preset's tags
included. The Map Number alone is rolled anew as the pick page opens. The
config outlives the session in the registry: `RGS1`–`RGS9` and both
`RGS_SpecialEvent` are read at start-up and written once, at shut-down. The
Map Number is never written. The dialog's Accept reads it with `sscanf %ld`,
0 made 1, no range test; the dialog has no Random button. **Inferred**:
Cancel keeps edits, as the dialog's controls write the config directly.
openregent keeps the same words but `RGS8`, `RGS9` in `freestyle.toml`,
written on leaving the setup page or starting a game
(`menu::freestyle::Custom::keep`).

### The setup dialog

**Binary**. `view::menu::setup`, `view::menu::bevel`, `smnu::style`,
`sim::freestyle::difficulty`; `tests/menu.rs`, `tests/quest.rs`. **Game**:
a capture.

`MX17`, `mx_textdata.cam`, 800×600 centred: frame `IX22` 1002, the chat panel
painted in. The capture shows `IX22` 1000 (control 3887) over the chat and no
ALLOW CHEATS.

Key 10 is the style, OR'd into the control's flags:

| Bits | Does |
|---|---|
| low 5: 2, 0 | the picture; 0 a box filled in colour role 0, edged in roles 1 and 2 |
| `0x40000` | the picture tiled from the corner |
| `0x10000`, `0x80000` | a raised or sunken bevel |
| `0x100000` | a text box draws no ground |

A bevel is two pixels a side, each strip a shade of what is under it: lit
sides add 8 value steps (mode 2, the `(10, 6)` family's table 3), dark sides
keep 50 % outside and 68 % (raised) or 62 % (sunken) inside. Raised lights
top and left, sunken bottom and right (**Binary**). The headings are raised; group boxes,
blurbs and combo fields sunken. **Inferred**: the capture's dark strips are
nearly black and its lit ones about 12 steps up, as if the 32-bit screen
shaded otherwise; openregent draws the binary's amounts (black over, grey
added).

A combo box (type 11) makes four parts from its own set plus 1 to 4: the
arrow button at its right (its picture's width, else 20 px), the text box,
the drop list (tiled), its scroll bar. `INcb` ships 1001 (the arrow, 25×23)
and 1003 (the list's texture); its own set and the field's draw nothing. The
sunken bit reaches the field and list, not the button. The scroll bar sits
down the list's right, in the arrow's column; its set, `INcb` 1004, does not
ship. **Inferred**: the drop list's height, its row colours and the bar's art
(openregent: the dialog's chat bar 9101, `INDs` 1000).

The lists, as the dialog fills them (`freestyle::offers`, `GOLDS`), for its
players: 1, or the session's in a network game:

| List | Entries |
|---|---|
| Player, Enemy Forces, Wandering Monsters | set 1 patterns by kind (Player Forces: the count's; the others: the count's trailer flag), by code word, then stably by rating; Wandering Monsters first `GMTX` 109 "None", its blurb the same. No Random entries |
| Landscape | regions whose team is active (one player) or expanding (more), region map order; no `RAND`. The flags agree on every shipped region |
| Special Events | `STRT ENTX` in table order, blurbs `EDTX` |
| Victory Conditions | `GMTX` 120–122, 123 "Last Palace standing" with more players; alone, a held 123 turns back to 120 |
| Victory Limits | three goals (`GMTX` 124, 126), then 125 or 127, the player's own (`AP89`); gold goals under start + 30,000 in red, the goal lifted; the goals and the lead: **Binary** |
| Starting Gold | `#starting_gold_very_easy`, `_easy`, `_normal`, `_hard`: 30,000, 20,000, 15,000, 10,000 |
| Building Limitations | `BTDN`'s five in blue, then `.BTD` files on disk ([below](#building-limitations-and-the-build-tree-editor)) |

A config code a force list lacks selects its first entry, and the dialog
writes the pick back; so does the land list. openregent fits the config alike
as the setup page opens and a LAN game is hosted (`Custom::fit`). The events
and the gold do not depend on the players.

The difficulty (5050) is `GMTX` 115 + GPL `CalculateDifficultyString` of
`CalculateDifficulty(gold, player, enemy, wandering, event 1, event 2)`: each
force its pattern's trailer word by the victory (eliminate, gold or days),
50 for a code with no pattern (`NONE`, a Random tag: the default pattern),
each event its `EVSC` number. Both land in setup slots 4 and 5: every corpus
freestyle save agrees.

### Building Limitations and the Build Tree Editor

**Binary**. `data::build_tree`, `view::menu::tree`, `World::limited`;
`tests/build_tree.rs`. **Game**:
captures, list open and closed.

The list's pick is a build tree: a name and building caps, the one object the
cap and upgrade checks read. Filling the list (as the dialog opens) and each
pick load the tree named into it and the name into the config; nothing else
writes it before play, and a game's end empties it. The registry keeps no
name. A network host sends the whole tree.

A tree loads by name: a shipped one (`BTDT`, named by `BTDN`), else the first
`.BTD` file in `BuildTreePath` whose tree has that name; neither, the caps
stay. A `.BTD` file is a `BTDT` record
([payloads.md](payloads.md#btdt--build-tree-variants)), written as
`user<name>.BTD`.

The editor `MX18` edits a copy of the tree. A row is a listed building (the
build menu's, and the `NotBuildable` upgrades), sorted by title, 20 shown:

| Control | Does |
|---|---|
| 2001 + n, the box | ticked: the code has a cap. Pressed: capped, the cap erased; else capped at 0. Greyed while a lower level, by `UpgradeFrom`, is capped at 0 |
| 2101 + n, 2201 + n | +, −: the cap one up, one down, not under 0; greyed uncapped. Hidden, with the count, for an upgrade, the Sorcerer's Abode (`ABq`) and the Gnome Hovel (`ABf1`) |
| 2301 + n | the cap; empty uncapped |
| 8006, 8007 | SET ALL: every row capped at 0. REMOVE ALL: every cap erased |
| 9006 | the name, SAVE AS |
| 9000 SAVE | written under the name typed, then the setup's tree and pick; a shipped name refused |
| 9003 DELETE | the `.BTD` file of that name removed; none (a shipped name has none) refused |
| 9001 CANCEL | nothing kept |
| 3887 | `IX22` 1003, whatever the row |

A refusal shows `AP11`: `GMTX` 409 "Illegal File Name", 410 "The filename is
reserved, or the file does not exist". Name compares are exact, case kept.
The list's keys: space toggles, `+` or `=` raises, `-` lowers.

openregent keeps the player's trees as TOML in
`$XDG_DATA_HOME/openregent/buildtree/` (`version`, `name`, `[caps]`), and
lists the original's `.BTD` files from there and from the install's
`buildtree`, writing none; of two of one name, its TOML. **Inferred**: a
quest started straight after picking a tree would keep its caps in the
original; openregent starts quests with none.

### Load and Save Freestyle Options

**Binary**. `view::menu::presets`; `tests/menu.rs`. **Game**:
captures of both.

The setup dialog's LOAD 6050 and SAVE 6051 open `MX20` and `MX21`
(`mx_textdata.cam`, 800×600 centred, frame `IX22` 1002); each returns to
`MX17`, which fills its lists and Building Limitations again. Enter presses
LOAD or SAVE.

| Control | Does |
|---|---|
| 9100 | the list (`INTI` 1003 or 1005, rows 20 px): every settings file whose player count is the session's, the one named as the config lit, else the first on LOAD, none on SAVE |
| 9006 | SAVE's name box, opening on the config's name (`GMTX` 411 "Default Settings"); refuses `" . / : < > ? \ \|` |
| 9000 LOAD | the lit row's file read over the config, the tree of its name loaded; closed. None lit, nothing |
| 9002 SAVE | the config named as typed, written over the file of that name or a new one, no question; closed |
| 9003 DELETE | the lit row's file removed, the list filled again; stays open |
| 9001 CANCEL | closed, nothing kept |
| 3887 | `IX22` 1000 over the chat (2, 4, 5004–5006 and its bar, 13 or 15) |

A row pressed is lit and nothing else. Load rolls a fresh Map Number on the
clock over the file's. A file is `free<name>.STY` in `FreeStyleIOPath`: a
version word and the config's fields; no `.STY` file is at hand, so none is
read. **Inferred**: the player count is the word the list compares with the
session's.

openregent keeps presets as TOML in `$XDG_DATA_HOME/openregent/freestyle/`
(`version`, `name`, `players`, the config, Map Number and tree name;
`files::presets`), beside the build trees and, like them, not per profile. Its
Load keeps the file's Map Number (RANDOM rolls another;
[openregent/enhancements.md](../openregent/enhancements.md)); an empty name
saves nothing.

### The save and load dialogs

**Binary**. `view::menu::saves`; `tests/menu.rs`. **Game**:
captures of both.

`AP36`'s SAVE GAME opens `AP49` Save Menu, LOAD GAME `AP27` Single Player
Load Menu (`textdata.cam`, 800×600 centred, frames `INDo` 1002 and 1000); the
main menu's Load Game and the lobby's LOAD GAME open `AP27` too. Enter
presses SAVE GAME or LOAD GAME, Escape CANCEL.

| Control | Does |
|---|---|
| 9100 | the list (rows 20 px, 13 shown): each `.GMP` in the save folder: a game of one's outside a network game, a network game's in one of as many players (`saves::listed`); the row named as the game's save name lit, else the first on `AP27`, none on `AP49`. A row lit puts its name in 9006 |
| 9006 | `AP49`'s name box, at most 100 characters, opening on "(%s) %s": the quest's title, the day ("(Freestyle Game) 4 Days"), which becomes the game's save name |
| 9004 | the lit row's description: `GMTX` 82, title, "Duration: 4 Days", "Save date: 8 October 2026" (`MNTH`); a network game's `GMTX` 349, no title, "Participants:" last, the names under it |
| 9005 | `AP49`'s CURRENT GAME DESCRIPTION: the game played, by its title ("The Dark Forest", a freestyle game's "Freestyle Game"), today |
| 9002 SAVE GAME | the game saved under the name typed, over a file of that name, no question; "Game Saved." (`GMTX` 311) in the chat; closed, `AP36` not opened |
| 9000 LOAD GAME | the lit row's save loaded; none lit, nothing |
| 9003 DELETE GAME | `AP11` asks "Delete Selected Game" (`GMTX` 346, 347), YES and NO; YES removes the file and fills the list again |
| 9001 CANCEL | back to `AP36` |
| 8730 | the pog, the game's quest's: a freestyle game's `FRE1`, the expansion's, `IX11` 1012, the compass |

**Inferred**: the list's order is the directory search's, NTFS's: by file
name, upper case, the extension's full stop after the name, so "lands 2"
before "lands" (`saves::order`). A double click presses SAVE GAME or LOAD
GAME.

openregent draws both in their art (`menu::save_page`) over the main menu,
the lobby or the game, at the menus' scale. Its list holds the profile's
saves as the original's filter keeps them, and those that will not read,
saying why. Its rows are newest first (`saves::newest`: the save's time, a
file that will not read by its own; ties by name, as the original's), each
with its time, "2026-10-08 14:30", and days, "4 Days", flush right in two
columns (`saves::side`), a departure
([openregent/enhancements.md](../openregent/enhancements.md)). Its dates are
by the local clock (`save::offset`); "Game Saved." posts once the file is
written, a LAN game's save its own line instead (`save::thread`). `AP27`'s
left side box, empty in the original, shows the lit save's minimap. 9004 adds
a line naming a save's mods, those missing red (`saves::mods_line`). Over a
game, the pog is `Quests::pog`'s. SAVE GAME on a name taken asks first in
`AP11` ("Overwrite Saved Game", placeholder text), a departure
([openregent/enhancements.md](../openregent/enhancements.md)). **Game**:
the save capture's game is the quest `QU05`, The Dark Forest: its title in
CURRENT GAME DESCRIPTION, its pog the spider, `INTD` 1004
(`the_save_dialogs_pog_is_the_games_quests`).

### The multiplayer pages

**Binary**. `view::menu::multiplayer`; `tests/menu.rs`. **Game**:
captures of each screen.

The main menu's Multiplayer asks a name (`AP35`), then:

| Screen | Is | Controls |
|---|---|---|
| `AP42` | Multiplayer Protocol | radios 7 modem, 8 serial, 9 LAN (each enabled by a DirectPlay service found), 10 GameSpy (opens `majestyx-zone.url`); the pick kept for the session; ACCEPT 5002, CANCEL 8003 |
| `AP33` | Multiplayer Setup | OPEN GAMES 8005 over list 5000 (`fn10`) and bar 9101: a row a DirectPlay session, `"-%d %s"` with `GMTX` 318 "Players" (**Inferred**: after the session name, the count of unknown kind), filled again when the session list changes; JOIN GAME 5002 told "no rows" (**Inferred**: disabled while empty); CREATE GAME 5001, JOIN GAME 5002 (Enter), CANCEL 8003 |
| `AP55` | session dialog, 474×276 | name 5001 (`fnt2`, 21 characters, kept for the session); 2, 3, 4 Players 5002–5004; ACCEPT 5000, CANCEL 8003; a name in use `GMTX` 306 |
| `AP73` | the lobby | START GAME 5000 or Ready 5010, ADJUST SETTINGS 5002, chat 5004–5006, LOAD GAME 5008, CANCEL 8003 |

The lobby's screen is `AP73`; HD's capture is `MX19`'s layout (**Inferred**:
the expansion's panel wins, as its others do): a settings box 2000 with bar
2010 where `AP73` has a box a setting; four seats, each a frame 2201–2204,
plate, check box 6701–6704 with its line (`GMTX` 284 "PLAYER ONE - HOST",
281–283 "... - READY", 285–287 "... - NOT READY") and name box 6401–6404
("player1 (v1.5.2.28)"), a combo box 7401–7404 under each, unread and not
shown; title 1021, GAME NAME 1009 over 878. ADJUST SETTINGS opens `MX17` with
its chat and ALLOW CHEATS, its portrait 3887 gone (**Game**). HD's session
dialog adds GAME VISIBILITY (Public, Friends Only, Private), in no panel or
string read: **Inferred** drawn by HD's code.

openregent draws `AP33`, `AP55`, `MX19` and the networked `MX17`, not `AP42`;
[openregent/multiplayer.md](../openregent/multiplayer.md#lan-play) has what
they do. `AP33`'s rows are the games LAN discovery hears, each "title (nick)
1/2 Players  land, Map N" with its build check flush right
(`menu::lan::found_rows`): HD's count and 318, more beside.

### Locks and victories

**Binary**. `quest_map::Progress`, `quest_map::lock`, `quest_map::requires`,
`quest_map::title_bar`; `tests/quest_map.rs`.

A lock is the slot quest's `QDEP` rule evaluated as postfix with this id
rule: a difficulty word only sets the difficulty, `NONE` is 1, any other id
is appended to the requirement list and is 1 once that quest has a victory.
The rule reads no `NOT` context, so the plain complement holds
(`deps::Expr::evaluate`). A quest outside the 34 slots is never locked, nor
any while an unlock-all byte (**Inferred**: STRT `APdb` 83, Cheat All Quests)
is set. With no victories six slots lock: QU06, QU09, QU10, QU11, XQ08, XQ12.
The Holy Chalice has a cloud and no rule.

The title bar of a locked quest: `GMTX` 268, then 138 with the difficulty and
each requirement in token order, repeats kept, `GMTX` 323 (grey) if won, else
324, each ending in two spaces; slot 5, the Day of Reckoning, says `GMTX` 142
"All Southern Quests" instead. A won quest's: 137, the victories and the day
text of the best time.

A victory record is a count and a best time in game ms, keyed by quest code.
It is written when `AP60` opens, for any quest world not freestyle: Krolm
(`MD01`) and Vampiric Revenge (`VAMP`) record too, though the map shows no
place for them. The count goes up by one; the best time is the level's timer
(**Inferred**: the game clock, which state 8 stops) on the first win or when
strictly shorter. A lost game, or a freestyle one, records nothing.

The whole map lives in one file, `questdata_majx.sav` in `Documents/My
Games/MajestyHD` (version 7): the records, the last won quest, and three
seen-once bytes: the expert hint (`GMTX` 146), the map advisor's first voice
(`RA28`/`RA30`), and the expansion's ending (`MV07`, asked by an end panel
once every expansion slot has a victory). Read once at start-up. Not per
player: Play Game's name prompt `AP35` (default `GMTX` 186 "Your Majesty")
stores the name in the registry as `LastNameUsed` and keys nothing by it.
openregent keeps the same facts per profile in its own TOML
([openregent/game.md](../openregent/game.md#the-game)).

`AP35` (base, no `UIData` copy), 533×406, centred, frame `INDn` 1000, its
black, blue-edged well the field's: heading 1 "Please enter your Name..."
(`fn11`), field 5002 431×33 at (49, 266) (`fnt2`, white), ACCEPT 5000
(Enter) and CANCEL 8003 (Escape), `DEM2` 1005. **Binary**: the field takes 21
characters while downloadable content is shown, else 12. **Game**
(a capture): the name centred, all selected on `#004470` as it
opens. `regent_core::view::menu::prompt`.

Erase Victories (5002) asks with `AP11`: `GMTX` 325, 326, Yes 5501.

`AP11` (base `textdata.cam`, no `UIData` copy), 251×323, centred, frame
`INDk` 1000: title 8005, text 5000 213×169 at (18, 80), white `fnt4`; OK 5503
(`INDa` 1004, Enter) over YES 5501 and NO 5502 (`INDp` 1001, 1002, keys Y, N).
Erase hides OK, the hint Yes and No. Box 5000's picture, `INTI` 1016, is a
10×6 magenta fill; the dark well is the frame's. **Game**
(a capture): the text's lines and block centred.
`regent_core::view::menu::message`. Yes empties the map and the seen bytes,
saves, and plays each erased star's set `0x60`.

`Progress::open_map`. As the map opens (each entry, a briefing's Cancel too),
in order:

| Step | When | What |
|---|---|---|
| advisor | the first-voice byte clear | `RA28` "Advisor_Introduction", phase `GVS5`, unplaced; the byte set and saved |
| | else 3 victories, the expert-hint byte clear | `RA30` "Advisor_Expansion", the same way |
| hint | once a session (unsaved) | `AP11` titled `GMTX` 147 "Select A Quest", Yes and No hidden, so OK (5503, Enter) closes it; by the sum of all victories: 0 `GMTX` 143, 1 144, 2 145, 3 146 once ever (the expert-hint byte set, saved); more, none |
| cloud | never in HD | below |

The advisor speaks on under the hint, Erase Victories' question and the other
dialogs the screen opens; leaving the screen stops it. Its test reads the
expert-hint byte before the hint sets it, so `RA30` comes with 146, unless the
session's hint was shown before the third victory: then nothing sets the
byte, and `RA30` plays at every opening until a restart. Erase clears the
session's hint byte too: the hint comes again on the next opening.

The cloud, `INQc` 1001, is two layers: a 17-frame swirl looping at 150 ms in
draw mode 8, a glow, and the "?" (layer 1, mode 7). The screen would play a
cloud's set `0x60` and forget the last won quest for the last of seven words
whose quest names the last won quest in its `QDEP`, is open, while the last
won quest has one victory. The words are control ids (4005, 4008–4010,
4029–4031) where the test wants quest codes, so it never passes: in HD no
cloud lifts (`quest_map::lifted`, `no_cloud_ever_lifts_in_hd`). **Inferred**:
nothing rewrites the words (one reader, no writer). Read as slots, they are
not the clouded slots either (29, 31 in, 2, 26 out). **Game**: the user has
never seen a cloud lift in HD. openregent draws none.

After a quest the end panel's Main Menu goes back to the menus, which resume
on `AP29`, the main menu, not the map.
