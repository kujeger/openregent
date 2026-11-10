# `.cam` payload types

`WAVE`, the `D*` descriptor tree, palettes, string tables, `FONT`, `CUT`,
`PICT`, `BTDT`. Pixel formats: [art.md](art.md).

> Part of [README.md](README.md).

## `WAVE`

Unmodified `RIFF`/`WAVE` files. `soundfx.cam` and `voices.cam` are plain `.wav`
containers. `payload::wave`; samples by `payload::Pcm`.

**Corpus**: every HD `WAVE` is 16-bit PCM, mono or stereo, at 11,025, 22,050 or
44,100 Hz (`every_wave_is_sixteen_bit_pcm`). Music is not here: HD streams MP3s
from `Music/`, named by `Data/MusicTracks.txt`; see [sound.md](../original/sound.md).

## `D*` descriptors — a chunk tree

All 1,438 records in `DUNT`, `DMOV`, `DACT`, `DTRN`, `DTPT`, `DSND`, `DSDP`,
`DSDG`, `DFCT`, no exceptions. Each node has a 16-byte header; the body holds
child nodes or leaf data.

```
0x00  char[4]  tag
0x04  u32      body length
0x08  u32      version
0x0c  u32      child count
0x10  ...      body

DUNT
└── DATA
    ├── HEAD   char[4] id, u32 subtype, cstr name, cstr description
    ├── PRIM   the <Engine> section
    └── SECD   the <Game> section
```

| Shape | Records |
|---|---|
| `DUNT [HEAD PRIM SECD]` | 443 |
| `DUNT [HEAD PRIM]` | 140 |
| `DSND [HEAD PRIM]` | 448 |
| `DACT [HEAD PRIM SECD]` | 140 |
| `DSDP [HEAD]` | 130 |
| `DMOV` / `DSDG` / `DTPT` / `DTRN` / `DFCT` | 62 / 50 / 21 / 2 / 2 |

6,203 parts (`every_descriptor_decomposes` holds the table and both totals). The child count makes the walk deterministic. `DATA` and `BTDT`
payloads do not use this header.

**Declared lengths can undercount.** `DACT` and `DSND` leaves count fixed fields
only; trailing strings sit outside. Example: `DACT A010 Henchman_death` declares
a 64-byte `PRIM`, then `henchman_dead\0` fills the 22 bytes before `SECD`. The
displaced sibling is placed by constraint (last child ends exactly at the
parent's end): one position on all 140 three-child `DACT` records, all
displaced. Not by scanning for
tag-shaped words: that finds `summ`, `Tele`, `Fire` inside `summon_`,
`Teleport`, `Fireball`.

**`HEAD`**:

- Subtype (second word): 0 General, 1 Projectile, 2 Building, 3 Character,
  4 Overlay, 5 ParticleSystem. Matches the SDK's `subType` on every base unit
  (`the_sdk_xml_makes_the_archive_records`).
  Nothing else gives the type; `SECD`'s layout depends on it.
- Two names, internal and display (`MRb1`: `hands_of_steel_icon`, "Hands of
  Steel"), on every record; the SDK XML's `Name=` and
  `Description=`. [`desc::Head::title`](../../crates/regent_core/src/data/desc/part.rs).
  Without a player-facing name, the record repeats its internal name with
  underscores loosened, and HD's name table writes `-`.
- HD also ships `UNTN` and `ACTN`, string tables keyed by descriptor id; they
  agree on every shared id (`the_hd_name_tables_agree_with_every_descriptor`).

### Fields, solved against the SDK

The HD SDK ships this data as named XML fields. Intersecting each XML
value against every 4-byte-aligned position, over all records declaring the
field, leaves one offset. `u32` unless noted.

- Oracle: all eight XML families of each dataset —
  `OriginalQuests/Data/M_*.xml` vs `data/`, `DataMX/MX_*.xml` vs `datamx/`.
  Kept separate because the expansion rebalances shared units (`AVA1` Adept
  parry 75 vs 65).

`PRIM`, shared by every unit subtype:

| Offset | Field |
|---|---|
| `0x0c` | `Menu` |
| `0x10` | `ImageIDBase`, a four-character `IMAG` id |

`SECD`, Character:

| Offset | Field | | Offset | Field |
|---|---|---|---|---|
| `0x00` | `DialogID` | | `0x3c` | `Attack` |
| `0x04` | `Cost` | | `0x40` | `RangedAttack` |
| `0x08` | `Experience` | | `0x44` | `Parry` |
| `0x0c` | `MaxHP` | | `0x48` | `Dodge` |
| `0x10` | `SightRange` | | `0x4c` | `WeaponBasicDamage` |
| `0x14` | `Speed` | | `0x50` | `ArmorBasicDamage` |
| `0x18` | `AttackRange` min | | `0x54` | `Gold` |
| `0x1c` | `AttackRange` max | | `0x58` | `RecruitDelay` |
| `0x20` | `Vitality` | | `0x5c` | `NameGenType` |
| `0x28` | `Artifice` | | `0x64` | `HelpID` |
| `0x2c` | `WillPower` | | `0x68` | `PrimaryStat` |
| `0x30` | `Intelligence` | | | |
| `0x34` | `Strength` | | | |
| `0x38` | `MagicResistance` | | | |

`SECD`, Building:

| Offset | Field |
|---|---|
| `0x00` | `DialogID` |
| `0x04` | `Cost` |
| `0x14` | `IncomeType` |
| `0x18` | `IncomeAmount`, **i32** |
| `0x1c` | `MaxHP` |
| `0x20` | `MaxGuildMembers` |
| `0x24` | `SightRange` |
| `0x2c` | `Gold` |
| `0x34` | `HelpID` |

`SECD`, Overlay: `0x04` `StackPriority`. General, Projectile and
ParticleSystem have no `<Game>` values and no `SECD`.

`DACT` `SECD`: `0x04` `EffectorDuration`, `0x08` `TimeoutDuration`,
`0x0c` `SpellType`, the list `GetBestSpell` asks, `0x10` `CharacterLevel`,
`0x14` `SpellRank`.

`DACT` `PRIM`: `0x28`, `0x2c`, `0x30` are `SoundPhase` `begin`, `peak`, `end`,
`DSDP` ids; every SDK action agrees by name (`an_actions_sound_phases_are_the_sdks`).
The engine plays `begin` and `peak`, never `end` ([sound.md](../original/sound.md)).

- A scalar needs ten or more samples. Left out, with fewer: `NameGenType` for
  buildings, `DialogID` for overlays, `SpellType` for actions.
- `0xcdcdcdcd` (MSVC uninitialised heap) in the recruit delay of `AVh1` (base)
  and `BVt1`, `BVu1`, `BVx1` (expansion), where the XML has a value. Accessors
  return `None`.
- Oracle: 3,361 field and 13,313 bit comparisons, all agree
  (`descriptor_fields_match_the_sdk_xml`, `descriptor_bitfields_match_the_sdk_xml`).

### The bitfields

`Info` and `Flags` are one XML element per set bit. A bit is pinned by set
equality: the records with the bit set are exactly the records whose XML names
the flag (a superset means a different, overlapping field). A wrong bit would
have to match exactly, be the only aligned candidate and leave no gap, so
`AllocateLocalID`, on a couple of records, is kept where a scalar with as few
samples is not. Both datasets
solve together: the base game sets `NoFlaggable` and `NotSpellTarget` on the same
buildings; the expansion separates them.

**Binary**: HD's XML importer registers each bitfield's names and masks
(`tests/flags.rs`). The tables name every bit set in shipped data, and bits no
XML separates. Bits marked (unset)
are named there and set on no shipped record.

`Info`, `PRIM + 0x00`, every subtype:

| Bit | Name | | Bit | Name |
|---|---|---|---|---|
| 0 | `Static` | | 10 | `IgnorePlayerColors` (unset) |
| 1 | `TerrainDependent` (unset) | | 11 | `AllocateLocalID` |
| 2 | `Directionless` | | 12 | `NotVisibleInISOView` |
| 3 | `DontBlock` | | 13 | `NotVisibleInOverheadView` |
| 4 | `BlockGround` | | 14 | `NotVisibleIn3DView` (unset) |
| 5 | `BlockFlying` | | 15 | `NotVisibleToOpponent` |
| 6 | `VisibilityIgnoresTeam` (unset) | | 16 | `NoGPLAgent` |
| 7 | `BoundingBoxSelectOnly` (unset) | | 17 | `RevealTerrainIfExposed` |
| 8 | `ModifyTerrainTextureOnPlacement` | | 18 | `NoRandomTargetOnDestination` |
| 9 | `ModifyTerrainHeightOnPlacement` | | | |

`Flags` is in `SECD`, so each subtype has its own enum and bit numbers:

- Building, `SECD + 0x28`: 0 `NotBuildable`,
  1 `IsGuild`, 2 `NoFlaggable`, 3 `HasHPBar`,
  4 `NotSpellTarget`, 5 `NotInMiniMap`, 6 `NumberedName`,
  7 `CanHighlight` (`CB01` Shrine of Darkness, `CB02` Shrine of Light; no XML), 8 `HasGoldToolTip`,
  9 `Heals`.
- Character, `SECD + 0x60`: 0 `Heals`,
  1 `NotFlaggable`, 2 `HasHPBar`, 3 `Poisons`,
  4 `NotSpellTarget`, 5 `NotInMiniMap`, 6 `NumberedName`,
  7 `CanHighlight`, 8 `HasGoldToolTip`, 9 `NotHero`,
  10 `IsInventoryItem` (unset).
- Overlay, `SECD + 0x08`: 0 `TransparentToMouse`.

Readers, **Binary**:

- `NotFlaggable`/`NoFlaggable` and `NotSpellTarget` become agent attributes
  `APU:9` and `APU:10`, from building bits 2 and 4 and character bits 1 and 4.
  The flag-target test reads `APU:9`, the spell-target test `APU:10`. **Game**: an Attack flag
  is refused on a Graveyard, a treasure chest and a Hooligan, and accepted on
  the player's Palace, which lacks the bit —
  `a_flag_is_refused_where_the_flaggable_bit_says`. **Inferred**: the
  spell-target test.
- `CanHighlight`: the hover test reads bit 7 of both words, building first; the unit under the cursor is passed on only with it.
  **Game**: in Balance of Twilight, hovering over either shrine draws a
  circle round it; no other building does.

Notes:

- Building spelling is `NoFlaggable`; `M_Characters.xml` and GPL attribute
  `APU:9 ATTRIB_NotFlaggable` say `NotFlaggable`. `MajestyHD.exe` carries both (**Binary**).
- `Category`, `PRIM + 0x04`: 1 on the four reward flags `ARA1`–`ARA4`, else 0;
  only `None` is registered as a name. `desc::unit::Prim::category`.
- **Game**: two reward flags. Explore goes on any location; Attack on
  friendly or hostile buildings, lairs, monsters and heroes. Only `ARA2
  Flag_Attack` and `ARA4 Flag_Explore` have art; `ARA1 mockup_flag` and
  `ARA3 Flag_Guard` have none. Heroes are `A`, monsters `B`, so every
  character passes `CanUse`; what refuses a flag is `NotFlaggable`.
- `CanUse`, `PRIM + 0x08`: `HumanPlayer` 1, `Monster` 2, `AIPlayer` 4. Zero in
  every shipped `.cam`. The loader fills a zero from the id's first letter,
  `A` → `HumanPlayer`, `B` → `Monster`, on every subtype but particle systems. The SDK XML prints
  the filled value: 142 `A` records, 116 `B` records, no other.
  `desc::Unit::can_use`, `the_loader_fills_can_use_from_the_ids_first_letter`.
  Read by the build menu (`HumanPlayer` buildings only) and under the flag and
  spell tests.
- Class mappings in [original/architecture.md](../original/architecture.md):
  `GE_FactionDescList` ↔ `DFCT`, `GE_SoundPhaseIDDescList` ↔ `DSDP`.

### `PRIM`'s tail

All 583 `DUNT` records (`unit_prim_tails_account_for_every_byte`). Variable
length, fully accounted for:

```
0x00   u32       Info bitfield
0x04   u32       Category
0x08   u32       CanUse, zero in files
0x0c   u32       Menu
0x10   char[4]   ImageIDBase
0x14   ...       subtype block: 12 bytes; 16 for Overlay (0x18 = AttachmentPointID);
                 272 for ParticleSystem
....   cstr      GPL callback name; lone NUL if none
....   ATCH      attachment tree — General, Projectile, Building, Character only
....   u32       1 on every record
....   u32       sound override count, 0 or 1
....   n * 8     [char[4] DACT action][char[4] DSND sound]
....   char[4]   DefaultSound, a DSND id; four zero bytes if absent
....   u32       ShadowType
....   u32       0 on all records
```

The parts sum to the declared length on 583 of 583; the reader walks forward and
checks the total. `DUNT` `PRIM` never undercounts.

**Four unit datasets.** 583 is HD's whole `DUNT` population. Counts on this page
are HD's.

| Dataset | Archive | Records |
|---|---|---|
| base game | `unittype.cam` | 394 |
| expansion | `mx_unittype.cam` | 174 |
| Krolm content | `mdl1_unittype.cam` | 9 |
| bonus quest | `xqd1_unittype.cam` | 6 |

HD: 583. Tail profile per dataset:

| Dataset | `ATCH` | `WEAP` | `MOVE` | overrides | callbacks |
|---|---:|---:|---:|---:|---:|
| base | 249 | 226 | 93 | 5 | 68 |
| expansion | 90 | 73 | 40 | 0 | 45 |
| Krolm | 6 | 6 | 5 | 0 | 0 |
| bonus quest | 4 | 4 | 2 | 0 | 1 |
| HD total | 349 | 309 | 140 | 5 | 114 |

Four tests, one per dataset, in `crates/regent_core/tests/editions.rs`
(`the_base_datasets_unit_tails_read` and siblings); the HD total row is
`unit_prim_tails_account_for_every_byte`'s.

**`ATCH`**: at `PRIM + 0x21` after an empty callback, else further on, since
the unpadded callback string precedes it.

```
ATCH   len 160, 3 children      128 and 2 for a projectile
├── EXPL  32 bytes, 4 slots
├── MOVE  64 bytes, 8 slots
└── WEAP  16 bytes, 2 slots     absent on projectiles (ATCH 349 less WEAP 309)
```

- `ATCH`'s fourth header word is a child count; its children's is a slot count
  (`len == count * 8` always). Hence its own reader, `data::desc::atch`, also
  used by `DTRN`'s `TPRT`.
- Slot: `[u32 kind][char[4] id]`. Every `EXPL` and `WEAP` slot is
  zero (`unit_prim_tails_account_for_every_byte`). `MOVE` uses slot 0 only, on 140 records; kind matches
  the XML `Attachment type`: 1 `Walk`, 4 `Fly`. Via the XML
  `Attachment ID` this indirectly checks `DMOV`; there is no direct oracle.

**`Script`**: in a `DUNT`, the bare callback name; matches XML `GPLFunction` on
583 of 583, 114 non-empty. In a `DACT`, `[u32 type][u32 cProc][cstr]` past the
declared length (the overhang), exact on every SDK-described action.

**Corpus**: 2,282 tail comparisons against the SDK XML
(`unit_tail_joins_match_the_sdk_xml`) — every `Attachment`, `Script`,
`DefaultSound`, `SoundOverride` agrees. Pinned quirks: `XR62`'s default sound
`WH02` has no `DSND` record; `ABM1`'s `BP32` is named `Copy of Wizards_Tower`, so
the exporter printed the id. `SoundOverride` (5 records, below the scalar bar)
is kept because each resolves in two tables (`DACT` and `DSND`), both matching
the XML.

**`ShadowType`** (third word from the end): **Binary**, HD's XML importer registers
`None` 0, `Static` 1, `Dynamic` 2. Exactly one
record holds 2: the expansion's `WPk1`, Horrify bolt, the
SDK's only `<ShadowType value="Dynamic"/>`. All others `None`. Separate from the
shadow baked into sprite pixels ([art.md](art.md)); what a dynamic shadow looks
like is unknown. `desc::Unit::shadow_type`, `the_sdk_names_the_one_dynamic_shadow`.

**Last word**: 0 on all 583 records. **Inferred**: it
is `ShadowOptions` (`Least` 0, `Max` 255), registered next to `ShadowType` and
set by no XML.

### The ParticleSystem block

272 bytes at `PRIM + 0x14`, in XML order. Double-marked: `HEAD` subtype 5, and
`PRIM` version 5 on exactly these 22 records
(`particle_systems_match_the_sdk_xml`), 4 on the rest. The reader
keys on subtype only, so the match is evidence.

```
0x000  u32       AttachmentPointID          (Overlay has it at 0x18)
0x004  u32       InitialParticles
0x008  u32       MaxParticles
0x00c  3 * f32   Bounds min
0x018  3 * f32   Bounds max
0x024  80 bytes  <Emitter>
0x074  148 bytes <Particle>; its last two floats, 0x100 and 0x104, the
                 spin's time in and out, zero on all 22
0x108  2 * u32   zero on all 22, no reader found

Emitter, from 0x024
  +0x00  u32      Type — bitfield: 0 Point, 1 Area,
                  2 Disk, 3 Column. Every record sets two bits
  +0x04  3 * f32  Velocity          +0x28  3 * f32  SurfaceNormal
  +0x10  3 * f32  Acceleration      +0x34  f32      Radius
  +0x1c  3 * f32  Offset            +0x38  f32      RadiusVariance
  +0x3c  f32      DirectionVariance +0x40  f32      Rate
  +0x44  u32      Lifespan, ms; all ones = never stops
  +0x48  u32      Recycle
  +0x4c  u32      Flags — 0 IsPausable, 1 IsPerformanceHog

Particle, from 0x074
  +0x00  u32      Lifespan, ms      +0x4c  f32      BirthSize
  +0x04  f32      LifespanVariance  +0x50  f32      MaxSize
  +0x08  f32      InitialSpeed      +0x54  f32      DeathSize
  +0x0c  f32      InitialSpeedVariance
  +0x10  3 * f32  Gravity           +0x58  2 * f32  SizeTime in, out
  +0x1c  4 * f32  BirthColor        +0x60  2 * f32  FadeTime in, out
  +0x2c  4 * f32  MidlifeColor      +0x68  3 * f32  JitterOrientation
  +0x3c  4 * f32  DeathColor        +0x74  3 * f32  JitterDisplacement
  +0x80  f32      BirthSpinRate     +0x84  f32      MidlifeSpinRate
  +0x88  f32      DeathSpinRate
```

- All 22 are in `unittype.cam` and `mx_unittype.cam`; each has an XML block.
- The varying fields solved by intersection; the two `Emitter` bitfields by
  set equality.
- The rest are constant on all 22 (e.g. `Velocity` `0, 0, 0`, `BirthColor`
  transparent black), so intersection cannot place them. **Inferred** placement:
  every named field in XML order at natural width tiles the 272 bytes exactly;
  nothing refutes it. 1,386 word comparisons agree.
- XML omits `SizeTime` when both halves are zero; the binary always has both
  words. The test compares it only where the XML has it.

### `DSND` — sound

All 448 records, vs `M_Sounds.xml`. Version 3. `PRIM` declares length 8 and
undercounts; the walk stretches it to the next sibling.

```
PRIM body
0x00  u32   Category      0 on all but a few records
0x04  u32   unknown
0x08  u32   phase count
0x0c  phase entries, 0x3c bytes each

phase entry
0x00  u32      Flags       bit 4 Looped, bit 6 CanQueue
0x04  char[4]  phase id    a DSDP record
0x08  char[4]  wave id     a WAVE record
0x0c  char[4]  group id    a DSDG record; four zero bytes if absent
0x10  u32      volume, set under Flags bit 1
0x14  u32      VolumeVariation up
0x18  u32      VolumeVariation down
0x1c  u32      frequency, set under Flags bit 2
0x20  u32      FrequencyVariation up
0x24  u32      FrequencyVariation down
0x28  u32      pan, set under Flags bit 3
0x2c  u32      pan spread, down; 0x30 up
0x38  f32      DistanceModifier
```

**Binary**: the sound player reads `0x10`–`0x30`; bit 1 is the exporter's `<Flags value="2"/>`. `desc::Phase::set_volume` and kin.

- `0x0c + 0x3c * count == len(PRIM)` on 448 of 448; a parse-time invariant.
  Starting the array at `0x10` would decode every field but leave the last entry
  four bytes short and hide `Flags`.
- `DSDP` (65 distinct, `sound_descriptors_match_the_sdk_xml`): `HEAD` only — id and name, targets of the phase ids.
- `DSDG` (25 distinct, same test): `HEAD` and a three-word `PRIM`, version 2: how many of the group
  play at once, real ms between starts, flags (bit 0 on `Advisor_Group` only:
  a held-back phase waits). No XML; **Binary**, `SoundGroupDesc_Engine`.
  `desc::SoundGroup`, `a_sound_group_is_a_limit_and_an_interval`.
- Oracle: 442 records, 1,750 phases, all agree
  (`sound_descriptors_match_the_sdk_xml`). Pinned quirks: four phases name
  `AS01` or `ETA0`, which have no `DSDP` record (the exporter wrote the raw id);
  four carry a flag bit the exporter wrote as `<Flags value="2"/>`, unnamed here
  too.
- The six records without XML are the Krolm content, `AK01`–`AK06`, absent from
  `OriginalQuests`.

## `PALT` and `SPLT` — palettes

Every payload is exactly 1032 bytes:

```
0x00  u8[8]      header, always 00 00 00 01 00 00 00 00 (meaning unknown)
0x08  rgb[256]   4 bytes each: r, g, b, pad
```

- 1,166 palettes (`every_palette_is_256_colours`). Pad is 0 in all but 64
  (`palette_headers_are_constant_and_some_padding_is_uninitialised`), all `SPLT`
  in `maindata.cam`; there every pad byte is `0xcd`, MSVC uninitialised heap. The reader
  drops it.
- `SPLT`: sprite palettes, most in `maindata.cam`. `PALT`: interface palettes.
- **Inferred**, not load-bearing: index 255 is magenta in over half of `SPLT`; entries
  from about 240 up are black more often than the middle. [art.md](art.md)
  explains the latter: 247–254 are the control band (four shadow steps, four
  glow steps), so a half-black band is a sprite using only one of the two.

### Player colour is a palette range, and it is off

**Binary**: settings keys `UsePlayerColorPaletteRange`,
`PlayerColorPaletteRangeStart`, `…End`, beside `GEPlayerColors`,
`CYPaletteArray`, `SetPalette`, `RefreshPalette`, `SharePalette`. The engine can
overwrite a contiguous index range per player (team colour from one palette).

`miscdata.cam` `DATA/GOPT` sets `UsePlayerColorPaletteRange = FALSE` and leaves
both bounds unset: off in the shipped game. A reimplementation may skip it until
a mod enables it. See [settings.md](settings.md). Consistent (**Corpus**, no
test): across every `SPLT` in the install, no index holds the same colour in
every palette.

## `STRT` — string tables

Every shipped table conforms (`every_string_table_parses`):

```
0x00  u16      string count
0x02  u8       encoding    0 = Windows-1250; 8 = Unicode (third-party tooling;
                           no shipped file uses it)
0x03  u8       version     0 = u16 offsets, bare strings
                           2 = u32 offsets, u32 key per string
0x04  offs[]   count entries, relative to payload start
...            records
```

- First offset always `4 + width * count`; offsets increase. Count 0 gives a
  4-byte payload.
- **Inferred** from BrandonWill's `str_tool.py`: the byte split and codepage.
  Our corpora are English; the only bytes above 0x7f are single-character
  glyph-table entries 0x80–0xff, so the codepage is unconfirmed.

### A version 2 record is `[u32 key] cstr`

```
version 0    offset -> cstr
version 2    offset -> [u32 key] cstr
```

- HD's 291 non-empty tables are all keyed (`a_version_two_string_record_carries_a_key`).
- Reading a version 2 table as bare breaks it: `UNTN` yields `XL00-`; a
  small-index key's leading NUL empties every `AITX` line and `AP07` caption.
- **`HKTX` exception** (hotkey names, `textdata.cam`): offsets are right for
  `ALT `, `CTRL`, `SHFT`, `MSEP`, then step by two or three while records run on.
  Walked sequentially, its 34 keyed records fill all 432 bytes. Keys are key
  codes: 9 Tab, 13 Return, 27 ESC, 32 Spc, `0x8000`+ for Center, Page Up,
  arrows, Home, End, F1–F15. The reader follows the offsets;
  `a_version_two_string_record_carries_a_key` pins `HKTX` as the only table
  where offsets and records disagree.

### The key is the join

| Key form | Tables | Examples |
|---|---|---|
| Printable four-character id | `QUES`, `DPTX`, `FNTX`, `FDTX`, `LNTX`, `HPTX`, `ENTX`, `EDTX`, `BTDN`, `UNTN`, `ACTN` | `QU10`, `ABB1`, `Wizb`, `EV01` |
| `u32` index, same four bytes | `AITX`, `QITM` | `4`, `36`, `11` |

Joins are exact: all 394 `UNTN` keys are `DUNT` ids, all 39 `ACTN` keys `DACT`
ids (`hd_fills_in_the_unit_and_action_name_tables`), all 142 non-zero `HelpID`s
in `unittype.cam` name an `HPTX` topic (`a_help_id_resolves_to_its_topic`). `HelpID`
and `DialogID` are keys.

| Table | Archive | Key | Holds |
|---|---|---|---|
| `AITX` | `gpltext` | index | Hero activity sentences: 177 (`the_labelled_activities_are_the_ones_the_game_ships`), more in the expansion's |
| `QITM` | `gpltext` | index | Items: name, newline, effect. `INV:11` is Ring of Protection |
| `QUES` | `gpltext` | quest id | Quest names |
| `DPTX` | `gpltext` | building id | Prerequisites in English, and what is never built |
| `HPTX` | `gpltext` | `HelpID` | Help topic per unit/building — 142 of 142 resolve |
| `FNTX`/`FDTX` | `mx_rgstext` | scenario id | Freestyle scenario names, descriptions |
| `LNTX` | `mx_rgstext` | land id | Lands |
| `ENTX`/`EDTX` | `mx_gpltext` | event id | Special event names, descriptions |
| `BTDN` | `mx_btdata` | variant id | `BTDT`'s five game variants |

- `AITX` index = the value a save stores in `APV:30`; four codes from saves land
  on the right string. `the_labelled_activities_are_the_ones_the_game_ships`.
- [`regent_core::data::text::Text`](../../crates/regent_core/src/data/text.rs) resolves both key
  forms, so a localised install answers in its own language.
- HD fills `UNTN`/`ACTN`, letting mods rename stock units and actions without
  editing `unittype.cam`. `hd_fills_in_the_unit_and_action_name_tables`.
- `gpltext.cam` is the VM's string archive: scripts refer to text by key.
- Tests: `a_version_two_string_record_carries_a_key` (tiling),
  `a_help_id_resolves_to_its_topic`.

### A spell has a name and no description

- Names come from `HEAD` and HD's `UNTN`/`ACTN`. `MRb1` names a `DUNT` overlay
  (icon) and a `DACT` (cast): "Hands of Steel" and "Hand of Steel".
- No description text exists. An overlay's `DialogID` names a whole `STRT` (the
  selection panel); every spell overlay has 0, and actions have no `DialogID`.
  "Hands of Steel" occurs only twice in the HD install, both names.
- **Inferred**: spell book text is built at run time from descriptor numbers
  (rank, character level, durations).
  `a_spell_has_a_name_and_no_description_text`.

### The only markup is a colour

`0x01` then six ASCII hex digits: RGB colour until the next marker.

```text
Ring of Protection \n\x01FFDDAA(+10 dodge,parry, resist, +2 armor)
```

- Every marker (over 400), both datasets. No other control byte in string bodies
  except `\n`, `\r`.
- Exception: `HKTX` strings are key codes (bare `0x1b` = Escape).
  `the_only_markup_in_the_games_text_is_a_colour` pins those three codes too.
- `data::text::spans` splits into coloured runs; `data::text::plain` strips
  markup.

## `FONT`

A bitmap font, a bit a pixel: `data::font`. **Binary**, the
`CYFontPixelmap` loader, for a record not starting with `0xff`, as both
shipped ones (`SM09`, `HB12`, `interfacedata.cam`):

```text
 u8 n, the name        then u16 k: words to the header, itself counted
 13 x u16 header       first, last, spacing, rows, ascent, descent,
                       words a row, where the widths sit; four unread
 bitmap                rows x 2 words a row bytes, MSB first
 column starts         last - first + 3, u16
 widths                as many (u8 advance, u8 bearing)
```

A glyph is the columns from its start to the next's; the pen
moves its advance plus the spacing, a space its advance alone;
a code past last draws the entry after it. `SM09` draws `Got_Gold`'s
amount (`a_font_record_is_a_bitmap_of_glyphs`).

## `CUT` — FLIC provenance

Twenty records, each 886 bytes, in `maindata.cam`. Index flag set: ids 0–19. Only these bytes are ever non-zero:

```text
+0x000  u32    version, 3
+0x014  fixed  211.0 for one source file, 83.0 for the other
+0x034  fixed  frame number in its animation
+0x038  fixed  245.0 or 0.0
+0x03c  fixed  253.0 or 0.0
+0x040  fixed  1.0 or 8.0
+0x04a  cstr   artist's path to the source
+0x24a  u8     frame number again (reader enforces equality)
+0x372  f32    0.004; the record's last four bytes
```

`fixed` is 16.16, as in save-game positions (the engine ships `cyfixedlp.h`).

| Source (Autodesk FLIC) | Frames |
|---|---|
| `J:\Projects\Majesty\Art\Game_Art\Effects\Build_Flag\Build_Flag.flc` | 6 |
| `J:\Projects\Majesty\Art\Game_Art\Effects\Particles\Highlight1.flc` | 14 |

Each covers its animation once (0..5, 0..13), no gaps. 886 bytes cannot hold a
frame, so these record which FLIC the shipped tiles came from.
`every_cut_record_is_a_frame_of_one_of_two_flic_files`.

## `PICT` — video

Records in `cinedata1.cam` and `mx_cinedata1.cam`. Six more in the
cinematics' `.dat` archives, the same container ([panels.md](panels.md#credits)): `cinedata2.dat` and
`cinedata3.dat` hold `MV02`, `MV04` and `SPLH` each, `mx_cinedata2.dat`
and `mx_cinedata3.dat` `MV07`. Payload:
`u32` kind, then an unmodified foreign file.

| Kind | Holds | Records |
|---|---|---|
| 7 | Bink video (RAD Game Tools): `MV01`, `MV06` 640×480; the `QM*` 260×260 | all but one |
| 1 | Truevision TGA, 800×600, 16-bit uncompressed | `SPLH` alone |

- Bink revisions: `BIKh` on the base game's but `QM13` (`BIKf`), `BIKi` on
  every expansion video. All 15 frames a second, video flags 0, no audio
  track; 19,153 frames (`every_bink_record_decodes_to_its_header_frame_count`).
- The six `.dat` videos each have one Bink audio track (RDFT): `MV02`
  stereo 44,100 Hz (`3`) or mono 44,100 Hz (`2`), `MV04` mono 22,050 Hz,
  `MV07` stereo 44,100 Hz; each as long as its video
  (`every_audio_track_decodes_to_the_samples_its_packets_state`). 15
  frames a second. The `3` pair is 640×480; the `2` pair is stored at 320×240
  (`MV07` 640×240), header flags `0x50000000` (`0x20000000`), which
  **Inferred** ask Bink to double it. `MV02` is `BIKf` (`3`) or `BIKh` (`2`), `MV04`
  `BIKh` (`3`) or `BIKi` (`2`), `MV07` `BIKh`.
- The `bink` crate (NihAV's decoder, vendored) decodes all 39 videos, bit-exact
  with ffmpeg (`every_bink_record_decodes_to_its_header_frame_count`,
  `a_few_frames_hash_as_ffmpeg_decodes_them`).
- `SPLH` = 4 (kind) + 18 (TGA header) + 800×600×2 + 26 (TGA v2 footer) =
  960048 bytes, the record length. **Binary**: shown once at start-up,
  after the intro videos; not the loading page
  ([panels.md](panels.md#the-loading-page)).
- Validation: a Bink's stated length must end exactly at the payload's end; a
  TGA must end in `TRUEVISION-XFILE.`. Holds on every record.
  `every_pict_wraps_a_file_that_checks_out`.

## `BTDT` — build-tree variants

Five records in `mx_btdata.cam`:

```text
u32    version, 1
char*  variant name, NUL-terminated
u32    building count
       that many: char[4] building id, u32 cap
```

Each record is consumed exactly. `BTDN` (a `STRT` in the same archive) lists the
same five names. The cap is how many of that building the player may have.

| Variant | Caps | Content |
|---|---|---|
| `default` | none | Unmodified tree |
| `Brute Force` | 10 at 0 | Library, every temple, Wizards' Guild, Wizards' Tower, Sorcerer's Abode banned |
| `No Towers` | 3 at 0 | `Ballista_Tower`, `Guardhouse1`, `Wizards_Tower` |
| `One of a Kind` | 16 at 1 | Every cap is 1 |
| `Wiz War` | 12 at 0, 1 at 1 | Warriors', Rogues', dwarves, elves, gnomes and most temples banned; at most one Rangers' Guild |

`every_build_tree_variant_reads_to_its_last_byte`,
`a_variants_caps_are_what_its_name_says_they_are`; `regent_cli variants`.

A cap table, not a tree. Dependencies are `BDEP`, the postfix boolean language in
[settings.md](settings.md).

**Binary**: one reader for this record, a `.BTD` file and a save's
`GSGL` tail; the version is read and dropped,
the cap is signed. The writer puts 1, the
name cut to 259 bytes, then the caps in the map's order, the code's
little-endian word ascending: each record re-encodes byte for byte
(`every_shipped_tree_encodes_to_its_record`, `data::build_tree`). A
shipped tree takes `BTDN`'s name for its key, so `BTD5` shows as
*Wizard War*. What the editor does:
[panels.md](panels.md#building-limitations-and-the-build-tree-editor).
