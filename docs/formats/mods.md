# Mods

HD's mods: `.mmxml` manifests, each `<Mod>` a set of files composed over a
game's dataset. **Binary** unless marked. Code: `regent_core::content::mods` (manifest, scan),
`dataset::Install::files_with` and `quest_with` (composition),
`data::desc::xml` and `data::text::Strings` (XML); regent_draw
`data::Sources`; openregent `env::mod_folders`, `begin::chosen`,
`menu::mods`, `mod_list`, `builtin`. Tests: `regent_core/tests/mod_library.rs` on the
Workshop items, `regent_core/tests/desc_xml.rs`, openregent's
`adjust_guardhouse_raises_the_most_guards`,
`adjust_guardhouse_adds_a_guard_on_upgrade`, `a_save_needs_its_mods`,
`xml_mods_play_without_script_errors`, `loading_off_loads_nothing_and_keeps_the_list`,
`a_mod_list_saves_and_loads_back`.

## The manifest

```text
<Majesty>
  <Mod id="{D666FCBE-9906-4582-A6A0-DC96F812441E}">   several a file (Majesty Expanded: many)
    <Name/> <DisplayName lang="en_US"/>
    <Description lang="en_US"><Short/><Long/></Description>
    <DataConfiguration>
      <Dataset base="Any">                            Any, Majesty or MajestyExpansion
        <Load>                                        and <Unload>
          <CAM/> <Constants/> <Template/>
          <GPL>Data/x.bcd</GPL>   or   <GPL><Target>Data\x.bcd</Target><Source/></GPL>
          <Descriptions/> <Strings/> <GPLSource/>
```

- List kinds: 0 CAM, 1 GPL, 2 Constants,
  3 Template, 4 Descriptions, 5 Strings, 6 GPLSource.
- A mod is keyed by its GUID. Elements may come in any
  order: Majesty Expanded writes `DisplayName` after `DataConfiguration`.
- Paths are relative to the manifest's folder, `\` or `/` between parts.
- HD never compiles GPL: only `<Target>` loads; `<Source>` and
  `<GPLSource>` feed the GPL debugger's source view.
- `Dataset base` is read as a word. Any other text
  fits no dataset (**Inferred**); HD's dialog says "unknown".

**Corpus**: 39 mods, 30 quests (`the_workshop_holds_39_mods_and_30_quests`).
One mod lists no XML: Adjust Guardhouse. 7 are for the expansion, 32 for `Any`
(`each_workshop_mod_names_a_base`).

## Where mods are found

HD, at start-up: quest folders for `*.mqxml`, Workshop quests,
`Documents/My Games/MajestyHD/Mods`, then each subscribed Workshop item.
Each folder is walked recursively for `*.mmxml`. A GUID seen twice from one source is skipped.

openregent (`env::mod_folders`), each walked recursively, entries sorted:

```text
 0. the built-in mods, unpacked    always (below)
 1. $XDG_DATA_HOME/openregent/mods   always
 2. ./Mods                         the current folder's, when present
 3. [paths] mods                   in config.toml, a list, each in order
 4. MAJESTY_WORKSHOP, else Steam's default library's workshop/content/73230
```

The config adds folders and replaces none. A folder named twice is walked
once. A GUID found twice: the first folder's, the rest listed as skipped
(**Inferred** from HD's user-folder-first order). An item folder is a
searched folder's direct child, or the folder itself for a file directly
in it.

## Source-only TOML mods

openregent also scans `mod.toml` files. This source-only manifest names
XML descriptions and strings, GPL source projects, inline unit patches
and `.dat` member values. It accepts no `.bcd` or `.cam` paths. Its [format
and examples](../../crates/openregent/mods/house_mend/README.md) and four
[example mods](../../crates/openregent/examples/README.md) are in the
repo. The examples are not embedded in the binary.

It also scans `quest.toml` in mod folders. A quest names its ID, GPL
function, dataset, difficulty, `.q` template, XML files and `[[script]]`
source groups. The scripts compile on scan; a failed compile removes the
quest from the list. CAM and BCD paths are rejected.

### Inline patches

`[[unit]]` and `[[prototype]]` entries, every applied mod's in load
order, make one patch (`content::flags`). Units are compiled into an
archive loaded after the XML descriptions
(`regent_draw::data::catalogue_of`); members are set on the loaded GPL
program (`Program::with_members`, openregent's `load::patched`):

```text
 [[unit]]       id = "ABW1"
                field = "building", add = [..], clear = [..]   flag bits
                set = { MaxGuildMembers = 8 }                  number fields
   ──> the winning DUNT record, rewritten in place
 [[prototype]]  instance = "GuardHouse2"
                set = { Max_Guards = 2 }                       member values
   ──> the instance's text for each member, as if its .dat said it
```

- A number field is named as the XML names it, by the unit's kind
  (`desc::BUILDING_NUMBERS`, `desc::CHARACTER_NUMBERS`); ids and flag
  words are left out. A value its field cannot hold, a name the kind
  lacks, `add` or `clear` without `field`: the mod is refused at the scan,
  named, or the game at its start, for what the unit's record decides.
- An instance and member are named as a `.dat` names them, any case. The
  value takes the member's type: an integer 0 up (the engine reads a text
  starting `-` as a name), a float below 2097152 either side (fixed
  point), `true` or `false`, a string, or a function's name. List, coord
  and agent members are refused. A value no member type takes is refused
  at the scan; an instance, member or type the program lacks or refuses,
  at the game's start, naming the manifest.
- The instance changes, not the agents: one made, or upgraded to it
  (`UpgradeAgentAttributes`), reads the new text. Its other members and
  the `.dat` file stay as they were.
- A patch with no `[[prototype]]` writes the text it did before, so a
  unit-only mod's saves keep their hash.
- LAN games play no mods ([below](#the-mod-set-recorded)), so no peer
  sees a patch.

Tests: `regent_core/tests/mod_patch.rs` on the made-up game,
`regent_core/tests/flag_patch.rs`, openregent's
`big_ranger_camp_holds_8_rangers`, `guardhouse_pair_posts_two_at_level_2`,
`veteran_recruits_start_at_the_palace_level`.

### openregent's builtins

GPL builtins no engine has, for mods' scripts (`gpl::builtins::OWN`).
openregent's own, not the original's; our compiler takes them as any
call. No shipped script names them, so none behaves otherwise.

| Builtin | Returns |
|---|---|
| `$GetBuildingLevel(agent)` | integer: the building's level, its code's fourth letter, 1 to 3 for the game's; 1 for a code not ending in a digit; 0 for a unit not a building. Throws for a gone unit |

- The completed level, as the spell panel's (`offer::stage`): one less
  while `#ATTRIB_CurrentStageBuilt` is 0. So 0 during the first
  construction, and N while upgrading from N: the code changes when the
  upgrade starts, but not the level. A building lost mid-upgrade never
  reads the new level.
- Handles from `0x80000000`, past every definition's, so the engine's
  builtins and script functions keep theirs (`Vm::builtin_handle`).

Test: `regent_core/tests/building_level.rs`.

## Built-in mods

openregent's own mods, in its binary: `crates/openregent/mods/<name>/`, a mod
folder as any, embedded by `include_bytes!` (`builtin::BUILT_IN`).

```text
 start ──> $XDG_CACHE_HOME/openregent/builtin-mods/<name>/   files written
           any GPL project compiled to its manifest's <Target>
           other folders there removed
       ──> mod folder 0
```

- Unpacked, not served from memory: the scan, composition and art
  sources read files, unchanged. A file holding the bytes already is not
  rewritten; one written goes aside and is renamed in.
- A built-in GPL project, if present, is compiled at unpack by
  `gpl::emit::compile_project`.
- Listed like any other, marked "Built in", off until ticked; recorded,
  saved and refused when missing by GUID, as any mod.
- A GUID a mod folder also holds: the built-in's, folder 0 first. A
  recording naming it replays the files the binary carries; a changed
  copy needs its own GUID.
- A user mod's `<Source>` is compiled where its `.bcd` is missing
  ([below](#gpl-source-compiled)).
- A `mod.toml` `[[script]]` always compiles from source.

### Peasant houses mend themselves

The built-in mod's [README](../../crates/openregent/mods/house_mend/README.md)
documents openregent's inline patch format ([above](#inline-patches)).

`{A2A48A02-2BCE-42B5-8D2F-22B59DB3B720}`, `house_mend`, base `Any`.
`General_Housing` (`ABe1`) gains the building `Heals` flag. The engine
mends it by 1 hit point every 5 game seconds. The mod changes no other
descriptor field or GPL prototype. The same flag applies to a loaded house.
Tests: openregent's `a_house_mends_with_the_builtin_mod` and
`a_loaded_house_mends_with_the_builtin_mod`.

### Bug fixes

The built-in mod's [README](../../crates/openregent/mods/bugfixes/README.md).
`{07C66EB7-1379-4B7A-8328-CDA91B2AD5DB}`, `bugfixes`, base
`MajestyExpansion`; a `[[script]]` over `GPL/gold.gpl`, one fix per file.

- Hidden heroes are paid. Aura of Peace sets a healer's `"type"` to
  `"hidden"`. The expansion's `List_Heroes_near_flag` counts `"Hidden"`,
  so the healer completes an explore flag; the payout
  `DropGoldInRadius_SamePlayer` lists `"hero"`, `"invisible"` and
  `"Camouflaged"` only, so the healer is unpaid, and alone the gold is lost
  (**Corpus**: the SDK's `GPLMx` sources; the shipped `.bcd` matches but
  for line numbers). The mod replaces the function, adding a `"Hidden"`
  search; the last loaded image owns a name. Only explore flags call it
  (`mx_check_rewards.gpl`); attack flags and deaths pay through
  `DropGoldInRadius`, unchanged. Tests: openregent's
  `a_hidden_hero_takes_gold_with_the_bugfixes_mod`,
  `a_hidden_hero_is_paid_with_the_bugfixes_mod`.

## GPL source, compiled

An enhancement ([openregent/enhancements.md](../openregent/enhancements.md)): HD loads
`<Target>` alone. openregent (`compiled`), at the scan:

```text
 <GPL> with <Source>s       Target on disk     the .bcd, as HD
                            none, or missing   compiled: sources hashed, cached at
       $XDG_CACHE_HOME/openregent/compiled-mods/<GUID>/<key>.bcd
       the program list names that file in the Target's place
```

- A `<Source>` is a `.gpl`, a `.dat`, or a `.gplproj` whose files join in
  its place; each found in any case. Compiled by
  `gpl::emit::compile_listed`, `builtin`'s compiler. Section 1 records
  each `.gpl` as the manifest spells it.
- `.bcd` wins: a Workshop mod ships both, and its `.bcd` is what HD
  plays. To test an edit, leave the `.bcd` out. Each compile and reuse is
  said at info.
- The key hashes the compiler's source (`MAJ_COMPILER`, the `regent_core`
  library and `Cargo.lock`, from `build.rs`) and each file's spelling,
  path and bytes: an edit to either recompiles, the older image removed.
- The manifest's spelling is what `MakeGPL.ps1` hands `Gplbcc.exe`:
  Improved Guards compiles to its shipped `.bcd` byte for byte
  ([gpl/compiler.md](../gpl/compiler.md)).
- The Mods page's Rescan scans the folders again and recompiles changed
  sources, and reads the offered quests again: an edit, or a new mod's
  quest, plays without a restart.
- A compile error refuses the mod whole: warned of, `Refused` on the
  Mods page with the compiler's `file:line:column: message`, left out of
  a menu game, an error for `--mods`.
- Two sources under one GUID play apart, so the compiled images' hash
  goes beside the GUID: a save's `Needed.image`, `play.input`'s
  setup `mods: ["{GUID}@hash"]`. A load of other images is refused, the mod
  named. Tests: openregent's `compiled::tests`,
  `a_save_of_other_mod_source_is_refused`.

## Load order

The quest's dataset files, its own lists after, then each active mod in
the player's order, if its base is the quest's dataset or `Any`. Each mod
drops its `Unload` entries and adds its `Load` entries at the tail unless
present.

- CAM: last wins, by chunk and record name (`Catalogue::find`).
- GPL: the list loads tail first, first loaded keeps a name
  (`gpl::load`), so the last listed image owns it: a mod over the base.
- Constants: the last left is the chart.
- A mod for the other dataset is passed over, and not saved as needed (a
  save lists each applicable active mod).
- Freestyle games compose too.

## What loads in openregent

- CAM, GPL `<Target>` (or its compiled `<Source>`), Constants: composed as above.
- `<Descriptions>`, `<Strings>`: composed the same way, the install's
  `InterfaceStrings.xml` first; a listed file not on disk loads nothing
  ([XML](#xml)).
- No archive is sparse: an empty mod record wins like any other, in HD
  too, and a frame's tile is its own archive's
  ([README.md](README.md#no-archive-is-sparse)). Phantoms
  Haunt's art resolves (`phantoms_haunt_names_no_empty_record`).

Adjust Guardhouse (Workshop `617966615`) replaces the `GuardHouse2` data
instance: `Max_Guards` 2, not 1. Its `.bcd` holds that one prototype and
no code. It copies the base game's instance, so over the expansion its
guardhouse also loses `upgradescript`.

Its second guard comes only on an upgrade, in HD as in openregent:

```text
City_Guard_Spawner          a thread run: marked running
  SpawnUnit                 NewUnitInit at once
    City_Guard_Birth        num_guards 1
      RestartGuardSpawnThread
        IsRunning(spawner)  true: still marked running, so no RunThread
BuildingReachedMaxHP        an upgrade finished, no thread of the spawner's
  RestartGuardSpawnThread   1 < 2, not running: RunThread, a second guard
```

**Binary**, and a traced game's `gpl.log` nests `City_Guard_Birth` in
`City_Guard_Spawner`'s run. The SDK comment saying the birth script runs
after the spawner exits fits `NewUnitInit`'s commented-out `RunThread` of
the birth script, not its direct call (**Inferred**: an older comment). A guardhouse placed at level 2 keeps one guard. A guard's death
(`mx_Hero_Deaths.gpl`) restarts the spawner from outside, for one guard:
that guard's birth again finds it running. Test: openregent's
`adjust_guardhouse_adds_a_guard_on_upgrade`.

## The mod set, recorded

- `--mods GUID,…` names the mods, in load order; `play.input` records it,
  so a replay loads them. A GUID no folder holds refuses the game.
- The menus' games take `mods.toml`'s `[[mod]]` entries enabled,
  in order, those found, and record them:

  ```text
  [[mod]]
  id = "17BF53DE-722F-4683-8E63-6B93D1606DFF"   GUID; braces and case free
  name = "Majesty Expanded"                     display name, else <Name>
  description = "…"                             <Short>, else <Long>
  enabled = true
  ```

  Read apart from the settings (`mod_list`); written by the mods page
  alone, on Done, at once: each found mod, `name` and `description`
  rewritten from its manifest and left out where it has none, then each
  entry no folder holds as written. `[mods] enabled` is not read.
- `mods_enabled = false`, top level in `mods.toml`: the menus'
  games take no mods, the list kept. Absent or no boolean: true, so an
  existing list loads. It stands in for HD's ACTIVATE MODS (control
  9001); whether HD's keeps the list is not checked.
- A collection: a list saved apart, the same `[[mod]]` entries, reader
  and writer, no `mods_enabled`. One file a list, named by its file,
  the typed name made safe (`mod_list::list_name`):
  `$XDG_DATA_HOME/openregent/mod-lists/<name>.toml`. The data directory,
  as presets and saves: saved things, not settings. Loaded into the
  page, it replaces the list and its entries no folder holds; it is
  applied by Done alone.
- A save's `meta.ron` lists each applied mod's GUID and name (as HD's SVGD
  version 11 does). A missing one refuses the load, named, as in HD; a
  different set is refused too.
- An HD save's list ([saves.md](saves.md#mods)): HD plays
  a loaded save with exactly those mods, none for version 10; a missing one
  refuses the load, named. openregent loads no HD saves.
- LAN games play no mods: HD's network game start composes none
  (**Inferred**: no mod set is compared). `--mods`
  conflicts with `--server` and `--client`; a guest refuses a setup naming
  mods. The data hash is unchanged.
- Golden and trace tests compose no mods.

## XML

### Descriptions

HD loads each descriptor set's archive records, then every
`<Description>` child of a `<Descriptions>` file's `<Majesty>` whose
`type` is the set's: `Unit`, `Action`, `Sound`. Twice over:

```text
 pass 1   ids and names alone      every set, archives then XML
 pass 2   every field              sounds, units, actions; each set's
                                   archive records again, then its XML
 register                         an id already there is replaced
```

- An XML record replaces an archive record of its id whole: what it
  leaves out is the constructor's, zero.
- A reference by name: the first record of
  its set, by id ascending, whose name matches without case; else
  `default` 0; else the text's id. Pass 1 lets it find a record defined
  later, or in another set: `UpgradeTo value="Guardhouse2"` is `ABE2`.
- An `ID` is read as a string id: four bytes a code, a number its value,
  else CRC-32. Many Workshop descriptions use a name (`MBPortal`), so an
  id field reads any nonzero word (`desc::id_field`).
- HD drops: a `subType` not in the set's table by name, so the SDK
  example's `subType="4"`, and `General` (the unit set spells it
  `Generic`); `Script type` and `cProc` (written 0); `AllowedSpells`' `ID` (an index is the
  count so far).
- A `CanUse` left out stays 0: only the archive loader fills it from the
  id. A made record's root part has
  version 1 (`desc::Origin::Xml`) so `Unit::can_use` tells.

`AllowedWeapon` and `AllowedArmor` are names in the character flag tables
(`Staff` 8, `Leather` 1), each list one entry `(0, value)`;
`AllowedSpells` is `(index, action)`. These are the `SECD` lists
`desc::Character::allowed` reads. A character's `Attributes` and every
unit's `GPLType` are read into fields the archive format lacks: not made.

openregent encodes each description into the archive's own layout
(`desc::xml::compile`), one archive of them loaded after every real one
(`regent_draw::data::Sources`): readers, sim and view are unchanged, and
the last-loaded rule gives the replacement. Checked: every SDK
description (1,145 records, `OriginalQuests`) makes the archive's
record byte for byte, but for words its XML cannot say
(`the_sdk_xml_makes_the_archive_records`: the export's filled
`CanUse`, uninitialised words, unflagged overrides, a name two sounds
share). The Workshop's 60 listed files hold 1,592 descriptions, all
read (`every_workshop_description_reads`); Majestic Majesty replaces 130
shipped records, Phantoms Haunt adds 68 (`a_mods_record_replaces_the_shipped_one`).

**Inferred**: a sound's phases each in order. The loop as read fetches
the first `Phase` again, which would not end.

### Strings

A dataset's, quest's and mods' `<Strings>` files fill one dictionary
(`text::Strings`): a `<Text id>` under a `<Language id="en_US">`, or with
that `lang`, keyed by the string id of its id; a later file's key wins. Its
text is stored unescaped (`text::unescaped`): `\n` and the like their
characters, `[newline]` CR LF, `[tab]` a tab, case ignored. Colour tags are
left for the renderer, which reads them in every dialog item's text, text box, tooltip, list row and
name label, as it reads `STRT`'s `0x01` markers (`text::spans`, the
syntax in `regent_core::data::text`'s Markup):

```text
 [color:7070ff]…[/color]   the text in 0x7070ff, then the colour before
 [fcolor:V] [color=V]      the same; names fold case, values are trimmed
 [color:light_red]         a name from the binary's 12
                           (text::NAMED_COLOURS; **Binary**), else hex digits
 nested                    a stack: a close goes back to the colour under it
 unclosed                  colours to the end; a value with no ']' takes
                           the rest of the text
 [bcolor:V]                the background; dropped here
 [b], [Level 1]            no tag: drawn as text
```

The info box (chat lines) reads no tag; openregent drops
them there (`text::untagged`), so no tag shows. **Corpus**: 1,319 lines in
10 Workshop files hold a tag, every one read
(`every_workshop_colour_tag_is_read`): help, names and action
descriptions (Majestic Majesty, Monster Kingdom, Majesty Extras, Decor
Buildings, The Wild), East Cheswick's briefing, two Majestic Majesty
scripts' service and research names. HD asks it where a table misses:

```text
 GMTX index past the table     by the index
 AITX code past the table      by the code
 HPTX HelpID not keyed         by the HelpID's word
 a unit's title                by the title's id
 an item's title               by the title's id
 a quest's win, loss message   IDTXT_QUEST_WIN_MESSAGE, IDTXT_QUEST_LOSS_MESSAGE
```

So a mod's `HelpID value="MK_hPalace"` shows its string. A script's
string message (`SpecifyName`, `MessageFlag`, `IsMessageFlagPresent`) is
a number by the string id rules, and the activity text finds the string of
that id: The
Wild's `SpecifyName(redinn, "REDINN")` names the inn "Hooligan's Haughty
Hangout", `"HOOL"` its Hooligans, `MessageFlag(guy, "EVENT1")` hangs its
`EVENT1` text. Tests: openregent's `a_workshop_quests_given_name_is_its_string`,
`a_message_is_a_number`. A GPL `#Name`
is not looked up here: HD's fix-up asks script constants, attributes and
descriptor names only, so NewGame+'s
`#LegendaryHeroesExtend`, a text id, is not found in HD either.

The end panel's verdict (`view::menu::end::verdict`), each step asked where the one before is empty:

```text
 1. QEND / LOSX by the quest's code     only a code one of the 34 map slots holds
 2. the quest's <Win lang="en_US">      (none in the corpus)
    else IDTXT_QUEST_WIN_MESSAGE / IDTXT_QUEST_LOSS_MESSAGE
 3. QEND / LOSX by FRE0                 openregent then FRE1 (Inferred)
```

`QEND` and `LOSX`, like `QUES`, are merged across archives
(`data::text::MERGED`): an expansion game's hold the base's `FRE0`.

So every Workshop quest, Wrath of Krolm and Vampiric Revenge say their
strings: Krolm's loss is its XML's "and", not `LOSX MD01`'s "&". A mod's
string replaces a quest's: ME Game Modifiers (`{4233391D-…}`) over The
Wild. A custom game is `FRE1`, no slot, so a mod's message shows there
too (**Binary**; untried in HD). Tests:
`a_verdict_asks_the_tables_then_the_strings`,
`a_mods_string_says_a_quests_verdict`, openregent's
`a_workshop_quests_verdict_is_its_strings`.

An item key is `INV:n`, `QITM` n, or a character's code
whose `Flags` hold `IsInventoryItem`, its `HEAD` title through the strings.
Majestic Majesty's potions are titled `IDTXT_ITEM_HEALINGPOTIONS_1`, so
their names are its strings. Shown by the hero's items page and a chest or sign naming its first item;
no shop panel names an item this way. A dropped item's name comes from
`QITM` (`view::panel::portrait::dropped_name`). Test:
`a_mods_string_names_its_item`. A scene's `items` gives one
(`sim::scene::Item`).

## Quests

A `.mqxml` in a mod folder is a quest, listed whatever mods are enabled,
as HD lists every Workshop quest. It joins
the quest map's downloadable list, after the install's extras; its own
lists compose as an install quest's. A mod's quest is named by its whole
id in `--quest` and the recording, and holds no map slot.

A quest's code comes from its id (`quest::key_of`):

```text
{36305551-0000-0000-0000-000000000000}   only the first word set: that word, QU06
{31305551-0054-0032-0000-000000000000}   anything more: CRC-32 of the 16 bytes,
                                         from all ones, not inverted: 0xea29f48e
```

So Majesty Expanded's Barren Waste is not `QU01`: `STRT QUES` has no
title for it, and it shows its `DisplayName`,
"121 - Original Quest - Barren Waste". NewGame+'s Legendary Heroes
reuses the shipped id `{34305158-0000-…}`, so it is `XQ04`. A second
quest of a code is added only from another source; the code's first record
wins lookups. `<Difficulty>` (Easy, Medium, Hard, Master, any case, else
Easy) is what a quest with no map slot is rated.

### Victories

HD keeps victories by code (`questdata_majx.sav`), so a mod's quest keyed by a CRC has its own, and
NewGame+'s Legendary Heroes shares the shipped `XQ04`'s. They show where
a pick in the extra quests' list `APd8` shows them: `GMTX` 136 with the difficulty, or once won
137 with victories and best time, as the map's title bar.

openregent: `Progress::win_quest` records by code; `progress.toml` names
a CRC code by its id, `[quests."{31305551-…}"]` and `last_won`, as a
CRC is four raw bytes. The extras' list shows each quest's line
(`quest_map::open_line`). Tests: `a_mod_quests_victory_is_kept_by_its_code`,
`a_mod_quests_progress_is_named_by_its_id`.

## Not done

- A `#Name` found by descriptor name (HD's fix-up's third source): HD reads
  the previous dataset's descriptors there.
- HD keeps the chosen set one session (**Inferred**);
  openregent keeps it in the settings file.
