# The target, and the HD modding SDK

Windows HD (2012), `steamapps/common/Majesty HD/`: higher resolutions, mod
support, the SDK.

HD is the target: the only edition still sold and patched, it ships the SDK,
and it runs on current machines (Windows natively, x86_64 Linux under wine).

## Consequences of the target

- `GPL Reference.pdf` ships with HD and documents it. HD adds eight builtins
  to the expansion (`AddAttribute`, `CanDropInventoryItem`,
  `GetInventoryItemAgentType`, `GetInventoryItemDropType`,
  `GetInventoryItems`, `IsInventoryItem`, `RemoveAttribute`, `SpecifyName`)
  and one attribute, `ATTRIB_CanDropItem` (its 208th; **Binary**). The SDK's builtin list
  (`tests/common/sdk.rs`) matches HD's executable (**Binary**), bar
  `ReRandom`, which is in no binary (`every_documented_builtin_but_one_is_registered`).
- HD's `bytecode.bcd` is the reference image, 825 functions
  (`the_base_game_code_section_matches_its_sources`).
- **Binary**, the Linux base and expansion executables' registrations: the
  expansion adds thirteen builtins to the base game: `GetEntranceLoc`,
  `GetSpecialEvent1Script`, `GetSpecialEvent2Script`, `PlaceRewardFlag`,
  `RemoveAllBuildingLimits`, `RemoveBuildingLimit`, `RevealWholeMap`,
  `SetBuildingLimit`, `SetHallFlag`, `SetPalace`, `SetShareReveal`,
  `StartEarthquakeSpell`, `StopEarthquakeSpell`.
- The game and `Gplbcc.exe` need x86 (Windows or Wine).

## The HD SDK

`Majesty HD/SDK/`, 22 MB.

| Item | What it is |
|---|---|
| `Documentation/GPL Reference.pdf` | 55 pages. Pp. 1–5 describe the language, the rest each builtin's parameters and return type |
| `Documentation/How To Make A Quest.pdf` | 23 pages |
| `Documentation/GPL Debugger.pdf` | 4 pages. Names the debugger's five panels; [formats/panels.md](../formats/panels.md) |
| `Gplbcc.exe` | GPL compiler, source to `.bcd`. 32-bit x86 PE |
| `RGSeditor.exe` | Map/quest editor: an `.rgs`/`.q` writer |
| `Example/`, `SpecialItemsExample/`, `Adjust Guardhouse Mod/` | Worked mods, source and compiled output |
| `OriginalQuests/` | The base game's sources |
| `Extras/GPL User Define Language template for Notepad++` | Syntax file with the keyword list. A byte-identical `GPL User Defined Language ...` sits beside it |

### The original GPL sources

`SDK/OriginalQuests/` holds 306 `.gpl` files (`every_file_lexes_without_error`)
and most of the SDK's `.dat` files:

| Directory | Contents |
|---|---|
| `GPL/` | Base game logic — `globals.gpl`, `defines.gpl`, `prototype.gpl`, `LowLevel.gpl`, `Hero_Births.gpl`, `Monster_Deaths.gpl`, `QItems.gpl`, per-quest scripts |
| `GPLMx/` | The expansion's equivalents, `mx_`-prefixed |
| `Data/`, `DataMX/` | Compiled `.bcd`, plus eight XML descriptor files each — `Characters`, `Buildings`, `Overlays`, `Projectiles`, `ParticleSystems`, `TerrainFeatures`, `Actions`, `Sounds` |

So the game's AI and rules need no decompiling.
`SDK/OriginalQuests/Data/bytecode.bcd` has the shipped copy's size but not
its bytes: a rebuild, not the shipped file.

### The XML descriptors are the oracle

The XML in `OriginalQuests/Data/` and `DataMX/` holds the same records as the
shipped `.cam` files, every field named: each unit, action and sound block
makes its record (sounds against HD's `sounddesc.cam`), 767 base and 378
expansion. Test: `the_sdk_xml_makes_the_archive_records`.

```xml
<Description type="Unit" subType="Character" ID="BV01" Name="Krolm" ...>
  <Engine version="1">
    <Info value="BlockGround"/>
    <ImageIDBase value="BV01"/>
    <Attachment kind="Movement" type="Walk" ID="Class 3"/>
  </Engine>
  <Game version="1">
    <MaxHP value="1500"/>  <Strength value="50"/>  <Parry value="55"/>
    <Flags value="Heals"/> ...
  </Game>
</Description>
```

- `ID` is the four-character `.cam` record ID.
- `<Engine>` / `<Game>` matches the class naming.
- A section's `version="1"` is not the chunk version word: `DSND`'s `PRIM` is
  version 3 and `DSDG`'s 2, both XML sections say 1.

HD still loads the binary `unittype.cam` (listed in
`MajestyDatasetDefinitions.xml`), so the XML is the mod format, converted to the
same in-memory descriptors. It gives field names and order; every solved offset
in [formats/payloads.md](../formats/payloads.md) is checked against it.

Load all sixteen files: a subset leaves tags looking unsolvable that the rest
describe.

### `.dat` — prototype definitions

Prototype instances in bracketed text, compiled into `.bcd` with the code. The
SDK's 15 files (most in `OriginalQuests`, the rest in the examples) hold 417
prototypes (`every_dat_file_parses`):

```
[InstanceName]
	{ShapeName
		(key    value)
		(key    value)
	}
[end]
```

- The name after `{` names a `prototype` declaration in `.gpl` (e.g.
  `prototype Hero()` plus a `declare` block of typed members). `.gpl` declares
  the shape, `.dat` named instances with initial values.
- No inheritance: every SDK `prototype` has empty parens, and each entry has one
  `{}` block.
- Three nesting levels. Whitespace separates key and value (no `=` or `:`).
  Values are unquoted, space-free, never empty. No comments. On a duplicate key
  the first wins: Hooligan's `castingrange` ships its first value
  (`first_duplicate_key_wins`).
- **Binary**, never exercised: `Gplbcc.exe`'s token table also accepts quoted
  strings, commas, `<`/`>`, coordinate literals and initializer lists, which no
  shipped file uses.

The compiler's sort orders and normalisation: [gpl/compiler.md](../gpl/compiler.md).

### Routes into the VM

In order of cost:

1. **The reference** — the whole builtin interface.
2. **Differential compilation** — compile controlled inputs with `Gplbcc.exe`
   and diff. Needs x86 (wine).
3. **Disassembly**, for what the first two leave.

Source/bytecode pairs on disk:

| Source | Compiled |
|---|---|
| `Adjust Guardhouse Mod/GPL/Guardhouse.dat` (478 B) | `Adjust Guardhouse Mod/Data/AdjustGuardhouse.bcd` (465 B) |
| `Example/GPL/WrathOfKrolm.gpl` (28 KB) + `_Data.dat` (3.2 KB) | `Example/Data/WrathOfKrolm.bcd` (98 KB) |
| `SpecialItemsExample/SpecialItemsExample.gpl` (21 KB) + `_prototypes.dat` (692 B) | `SpecialItemsExample/Data/SpecialItemsExample.bcd` (62 KB) |

Guardhouse is the only shipped data-only project, and openregent's compiler
reproduces it byte for byte (`guardhouse_mod_compiles_byte_for_byte`).
WrathOfKrolm is a real program with its output.

## The Workshop mods

`steamapps/workshop/content/73230/` — Workshop items: more `.gpl` than the
shipped source, and `.dat` files.

A second, independent corpus: strangers' code, not one team's house style.
They were compiled by `Gplbcc.exe` and load in the retail game, so what they do
is legal GPL (`every_mod_dat_file_parses`). Some items ship `Gplbcc.exe`:
the SDK's build of 2016-04-11, bar `678145904`'s of 2016-03-06. Two language rules rest on them; see
[gpl/language.md](../gpl/language.md).

Two mods ship `.cam` archives, one with over 600 sprite records the install
lacks (`mods_ship_content_the_install_does_not` checks the floor).
Nothing in the SDK writes a `.cam`; their builder is unknown, except for the
one below.

### One mod documents its own build

[Custom Guild: Phantoms Haunt][haunt] (item 3769947406) publishes its toolchain:
a 6,900-line Python CAM builder, a 6,400-line validator, and art-pipeline notes.
The only known third-party tool that writes every payload type.

[haunt]: https://github.com/Phantomstar721/majesty-gold-hd-custom-guild-phantoms-haunt

Its claims about engine rules (unverified unless marked):

- A mod archive is sparse; a zero-length record means "fall through". Does
  not hold: an empty record is padding, never named
  ([formats/README.md](../formats/README.md#no-archive-is-sparse)).
- The executable addresses some slots directly, e.g. a building's build-menu
  icon tile that no animation record names. So walking `IMAG` references does
  not list every drawable tile.
- A `DSND` `HEAD` length is `8 + len(name) + 2`, unpadded; renaming a sound
  moves `PRIM`. Matches the `HEAD` layout in
  [formats/payloads.md](../formats/payloads.md).
- Freestyle crashes when any mod supplies a custom `.cam`. Unexplained.

XML descriptors are a runtime input: the mod defines a unit, three buildings,
six spells, projectiles, overlays and particles in XML only — its archives
hold no `DUNT` or `DACT`, just art, text, sound and one `DATA` record — and it
runs.

Its XML uses `CanUse`, `AllowedWeapon`, `AllowedArmor` and an `<AllowedSpells>`
list, which no known offset explains. `CanUse` is nowhere in `PRIM`: it is
filled at load (`desc::Unit::can_use`).

## HD-specific files

**Inferred**: HD changed presentation, not simulation. Its `Data/` adds:

| File | What it is |
|---|---|
| `UIData_<width>_<height>.dat` | Thirteen, one per resolution (`the_empty_panel_is_a_crown`). CYLB archives holding `SMNU`: one panel's layouts, all the same length; [formats/panels.md](../formats/panels.md) |
| `addinterface_se.cam` | — |
| `ImageSetIDXRef.xml` | Names all 367 animation-set ids; `data::imag`'s kind table is checked against it (`image_set_kinds_match_the_cross_reference`) |
| `setnames.txt` | 380 `[set name] id` lines, read by no executable. Agrees with `ImageSetIDXRef.xml` on every name both hold; adds `Idle`/`Collect` for `Special`/`Carry` and 23 ids 3000–3900 no art uses (`setnames_agrees_with_the_cross_reference`) |
| `InterfaceStrings.xml` | English `IDTXT_*` strings for HD's quest-missing and mod dialogs |

HD quests also ship as `.mqxml` beside the original `.q` files.

## Two HD builds: 1.5.2.24 and 1.5.2.28

**Corpus**, compared file by file, then record by record. No test holds this.

| Install | `MajestyHD.exe` | Scripts (`.bcd`) | Other data |
|---|---|---|---|
| Corpus before 1.5.2.28, Steam `public`, build 1611771 | 1.5.2.24, 3,933,696 bytes | old | old interface |
| Steam `beta2`, build 2536689 | 1.5.2.28, 4,056,064 bytes | GOG's | `UIData_*.dat` `AP42`/`APda` and `InterfaceStrings.xml` differ from GOG's |
| GOG | 1.5.2.28, 4,058,112 bytes | new | new interface; `mx_textdata.cam` |

The version is the `FileVersion` and `ProductVersion` in each exe's own
version resource, the same in both fields. GOG's folder holds no
`goggame-*.info` or installer file, so nothing else states it there;
`appmanifest_73230.acf` names Steam's branch and build, not a version.
Steam `public` is byte-identical to the corpus apart from the files the
corpus adds (trace-patched exes, probes).

Differences, corpus against GOG:

| What | Differs |
|---|---|
| `MajestyHD - Old.exe` | 1.5.1.2 in all |
| GPL, `Data/`, `DataMX/` and the SDK's copies alike | `Release_Guards` (base and expansion), `reconstruct_lists`, `Get_Valid_Hench_Buildings`, `Outpost_death`, `Palace_Death`; `Magical_build_rate` added to `SorcerersAbode2` and `Wizards_Guild2`. Other definitions: line numbers and scope tables only |
| GPL source | The same six files: the fixes skip dead agents, and `Release_Guards` empties its list before the loop, not inside it |
| `UIData_*.dat` | GOG adds `SMNU`/`STRT` `AP42` and `AP55`, changes `APda`; `beta2` has `AP42` (shorter than GOG's), `AP55`, and `APda` unlike GOG's |
| `addinterface_se.cam` | GOG and `beta2` add `multi_session` (`IMAG` `INx3`, `TILE`) |
| `InterfaceStrings.xml` | GOG adds multiplayer and cursor-lock strings; `beta2` has the corpus's |
| Only in GOG | `Data/mx_textdata.cam`, a copy of `DataMX/`'s; the Linux `maj-game.*` |
| Only in Steam | `steam_api.dll`, `binkw32.dll`, `Readme.txt`, three `.url`, the SDK's `TempProject.gplproj`; the Notepad++ template without `.xml` |

`beta2` against the corpus differs in all of the GPL rows above and in
`addinterface_se.cam`; against GOG in the exe bytes, `InterfaceStrings.xml`,
`UIData_*.dat` (`AP42`, `APda`) and the files only in GOG or Steam. No
script differs between `beta2` and GOG. Mods are found outside the
install, so all read the same ones.

### One recording, two builds

**Measured**, no test: a quest XQ05 log recorded on GOG, with two mods (house_mend, built in;
Improved Guards Plus, Workshop 1265189890), replays to its last frame
(24,450) on GOG and on Steam `beta2`, and parts from the corpus's data at
frame 4,000's digest. With GOG's `MX_Build.bcd` on Steam data it replays all
24,450 frames; GOG's `MX_Task.bcd`, `MX_Data.bcd` or `bytecode.bcd` alone do
not help. The only code that file changes is `Release_Guards`, now longer
in records.

How it parts, by snapshots: world and thread queues agree to frame 3,881.
At 3,882 only the queues differ: GOG's frame ends its 1,500-record budget
some threads sooner. At 3,884 a peasant starts repairing Guardhouse1 on GOG and
not on Steam. The cheap digest first differs at 3,938. With no budget
(`budget:unbounded`) both builds agree at frames 4,000 and 7,000. So the fix
changes timing, not outcome, in this game: a longer function spends more of
the budget. **Inferred**: the extra records are `Release_Guards`' run in
frame 3,882; no trace says which thread ran then.

So, under the budget, any change to GPL code could desync two players,
even one that does nothing different. openregent runs no budget
([enhancements.md](../openregent/enhancements.md)), so a longer function
alone no longer parts them (**Inferred**: no other rule of play reads a record count). openregent's LAN hello hashes the SDK `.bcd` bytes
(`lobby::data_hash`) and refuses a peer whose hash differs: these two
builds cannot start a game together. Archives are hashed by name and
length only, and mods are not hashed, as a network game plays none.

### What 1.5.2.28 changes in play

GOG's release post ([forum, 2018-10-09][gog1528]) lists: Direct3D 9 by
default, the SDK, basic multiplayer over GOG Galaxy, mouse edge locking in a
window, middle-button map drag, and bug fixes. DirectPlay LAN is off by
default, back by editing the preferences.

[gog1528]: https://gog.com/forum/majesty_series/majesty_updated_to_15228/page1

The scripts (SDK sources old against new; **Inferred** effects):

| Function | Change | In play |
|---|---|---|
| `Release_Guards` (both) | copies `Waiting_Guards`, empties it, then walks the copy; skips dead guards | the `-=` inside the loop freed the cursor's node when a guard was listed twice ([bytecode.md](../formats/bytecode.md), `foreach`): `IsHidden` threw on agent -1. Gone |
| `reconstruct_lists` (base) | skips and drops dead buildings | the expansion's copy already did; peasants no longer ask a dead building's level |
| `Get_Valid_Hench_Buildings` | skips dead buildings | an outpost's peasants no longer measure to a dead one |
| `Outpost_death` | Palace death if this outpost is the true palace (`GetTruePalace`), else building death | was: "does a building titled Palace remain?", by `ListObjects` over what the owner sees |
| `Palace_Death` | runs `Clean_Palace_Construction_Lists` after handing over to an outpost | the dead palace leaves the lists the outpost inherits |
| `SorcerersAbode2`, `Wizards_Guild2` | `Magical_build_rate` 2 and 5, as level 1 | `magical_upgrade` adds it to hit points each `#magical_build_delay`. An upgrade keeps level 1's value (`UpgradeAgentAttributes` sets only members with text), so only a level 2 made directly read 0, and its upgrade to 3 never finished, bar deathmatch rules |

openregent runs the install's `.bcd`, so it takes these fixes as is.

The interface, all in `UIData_*.dat` (openregent reads `AP42` and `AP55`
from the datasets, so it shows neither):

- `AP42` adds "Play Over the Internet on Steam" (1000); GOG's also "on
  GoG Galaxy" (1001).
- `AP55` adds GAME VISIBILITY, Public, Friends Only, Private
  (5100–5102), on taller art `INx3` 1000 (`multi_session`, 474×326, in
  `addinterface_se.cam`): a lobby's visibility.
- GOG's `APda` adds combo box 9004 (`INcb`) at (26, 365); its three
  strings are `InterfaceStrings.xml`'s `IDTXT_SETUP_LOCK_CURSOR_*`
  (**Inferred**: the cursor lock, `ClipCursorMode`).
- GOG's `InterfaceStrings.xml` adds joining, error and "FRAMES PER
  CHECKSUM" strings. `beta2`'s exe names none of them.

### The executables

**Binary**: disassembly, the RTTI walk and `strings`.

| | 1.5.2.24 | `beta2` 1.5.2.28 | GOG 1.5.2.28 |
|---|---|---|---|
| Built | 2017-02-06 | 2018-02-19 | 2018-10-08, `Release_GOG` |
| `.text` | 0x334000 | 0x34c400 | 0x34c000 |
| Classes with a vtable | 525 (**Binary**) | more | more |
| DLLs | `dplayx`, `ddraw`, `steam_api` | `ws2_32` in, `dplayx` out | `galaxy`, no `steam_api` or `ddraw` |

New in both 1.5.2.28 builds:

- Online play replaces DirectPlay's import: `P2PService`, `P2PSession`,
  `P2PGame`, `P2PPlayer`, `NetworkTransport` under `GSSteam` (Steam
  lobbies, matchmaking, P2P networking) or `GSGalaxy`, each a `CNService`
  family member as `CNDirectPlay*` are. Front end:
  `MultiplayerLobbyBrowserController`, `MultiplayerStagingRoomController`
  (on a new `SMenuController`), `SaveGameBrowserController`.
- A checksum per player, compared with the local one; settings
  `MultiplayerChecksums`, `MultiplayerFramesPerChecksum` (GOG).
- Remotery, a profiler with a WebSocket server: the Winsock, TLS and
  file-mapping imports.
- `HenchmanController`, a `Base_VehicleController` whose 8028 opens
  `AP94`; what else it adds is unread.
- Builtins type-check agents: `DeleteAgent`, `DeleteGamePiece`,
  `DistanceBetweenAgents`, `IsValidGamePiece` report an argument that is
  not an `agentRef`. No builtin name is new.
- Settings keys: `EnableDirectPlay`, `EnableSound`,
  `MessageBoxHeightPercent`, `MessageBoxScrollDelay` (for
  `#MessageBoxScrollSpeed`); GOG adds `ClipCursorMode` and the two
  checksum keys.

Changed vtables (GOG): in `DialogView` (and `SimpleMenu`,
`LoadQuestMenu`, `BackgroundFillDialogView`), `CYDialogManager`,
`GSMessageManager` and the `MapIOManager`s, matched methods sit one or
two slots later. The `CN*` network bases grew too; their methods all
changed.

How much moved (functions' bytes compared with branch targets and image
addresses masked): in each pair most old functions are found unchanged, and
each adds new functions with no twin. Most named functions, and fewer named
sites and data, are found; more of each for `beta2` than for GOG.

Nearly every address moved: code was inserted near the start of `.text`. A
named address is found by its function's bytes, by an operand of a matched
instruction, or by a unique 48–192 byte window; **Inferred** a few window
matches are wrong. The named functions not found (GOG) are in changed
functions: the frame step, pause decode and game start; menus, lobby and
session pages; the selection panel's hero rows; edge scroll; the four
type-checked builtins; list helpers; the `ORDR`, `CONT` and `PRIM` save
readers (**Inferred** changed, not read).

### openregent's tests on 1.5.2.28

1.5.2.28 is the only version openregent supports, either build:
`MAJESTY_HD` names GOG's or `beta2`'s, and the `install` tier passes on
both. Its counts are 1.5.2.28's. The builds differ for two tests:

- GOG's `Data/mx_textdata.cam` is `DataMX/`'s byte for byte;
  `regent_corpus::archives` drops it, so archive, panel and string
  counts are the same.
- GOG's SDK has no project for `SpecialItemsExample`: 19 projects
  reproduce a shipped data section, Steam's `TempProject.gplproj` a 20th
  (`PROJECTS_MATCHED`).
