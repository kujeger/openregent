# Data formats

The `.cam` container, the small plain-text formats, and an index of the
format documents.

| Extension | Format | Status |
|---|---|---|
| `.cam` | CYLB archive | Solved — container and every payload type |
| `.bcd` | GPL bytecode in a serialised memory block | Solved — all three sections, read and written; nothing in a record unread |
| `.rgs` | RGS map-generator charts | Solved — all four shipped files, five sections each |
| `.q` | Quest / map | Solved — all 36 quests, three versions |
| `.qdd` | Quest metadata | Plain text |
| `.gmp` | Save game | Solved — container and the generated map inside |
| `.xml` | Dataset manifest | Plain XML |

Cutscenes are Bink.
`titlepage.bmp` and the icons are BMP.

Reference implementation: [`crates/regent_core/src/data`](../../crates/regent_core/src/data).
Every structural claim is asserted against shipped files in the tests; where
prose and tests disagree, tests win. `tools/` scripts are for exploration.

## One file per format

| File | Holds |
|---|---|
| below | The `.cam` container, and the small plain-text formats |
| [payloads.md](payloads.md) | `WAVE`, the `D*` descriptor tree and fields, `PALT`/`SPLT`, `STRT`, `FONT`, `BTDT`, `CUT`, `PICT` |
| [art.md](art.md) | `TILE`, all three versions, and `IMAG` animation metadata |
| [settings.md](settings.md) | The `DATA` records — `BDEP`, `QDEP`, `XML1`, `GOPT` |
| [panels.md](panels.md) | `SMNU`, panel and menu layout |
| [terrain.md](terrain.md) | `DTRN`/`DTPT`, the tileset, the seventeen blend cases |
| [bytecode.md](bytecode.md) | `.bcd`, its block framing and three sections |
| [maps.md](maps.md) | `.rgs` charts and `.q` quests |
| [saves.md](saves.md) | `.gmp` saves, the map inside, and the projection |
| [mods.md](mods.md) | `.mmxml` mod manifests, where mods are found, load order |

Engine behaviour once read from these files (movement, sound, minimap,
cheats): [original/](../original/README.md).

## `.cam` — the CYLB archive

**Corpus.** Holds on all 46 shipped archives, none empty
(`every_parses_as_cylb`). Reader: `data::cam`.

```
header (0x14 bytes)
    0x00  char[8]  "CYLBPC  "        two trailing spaces
    0x08  u16      version major     always 1
    0x0a  u16      version minor     always 1
    0x0c  u32      chunk count
    0x10  u32      total bytes of all chunk record tables

directory (immediately after header, 8 bytes per chunk)
    char[4]  tag
    u32      absolute offset of this chunk's record table

chunk (at that offset)
    u32      record count
    u32      flags
    record[] 28 bytes each:
        char[4]   id          unique within the chunk
        char[16]  name        NUL-padded, often truncated
        u32       ofs_data    absolute
        u32       len_data

payload heap
    starts at 0x14 + 8*chunk_count + len_tables
    payloads stored back to back in record order
```

All little-endian. Invariants:

1. The first chunk's offset is `0x14 + 8 * count`, the directory end.
2. The `0x10` field is the summed size of all chunk tables; the heap starts
   at directory end plus that value.

In HD the heap runs to end of file in every archive:
`no_archive_carries_trailing_bytes`.

### The chunk `flags` field

**Corpus.** 1 in every `TILE`, `SPLT` and `CUT` chunk, 0 elsewhere. Not
compression: `TILE` decodes as plain row-RLE and a `SPLT` is a flat 1032 bytes.
In every flagged chunk the record id is the record's index. Two unflagged
`IMAG` records have an id that is neither a tag nor an index, one `0xcd`.
Test: `a_flagged_chunk_numbers_its_records`.

**Binary**: the flag marks indexed records. Set, a number under 2^24 is the
position and the stored id is unread; clear, the number is matched against the
ids, the last match winning (Phantoms Haunt's `SPLT` chunk). Tests:
`a_flagged_chunk_is_indexed_by_position_not_id`,
`an_unflagged_chunk_is_indexed_by_its_last_matching_id`.

Tile numbering is archive-local: an `IMAG` frame indexes its own archive's
`TILE` chunk. No global tile id.

### No archive is sparse

An empty record does not stand for the stock one. **Binary**:

- A library keeps every record, empty ones too.
- A fetch by id tries the libraries from the last opened; the first holding
  the tag and id answers, even with an empty record. Test:
  `a_later_image_overrides_by_name_and_its_empty_record_wins`.
- An `IMAG` fetches tiles only from the library it came from.
- A load reads at the record's offset and ignores its length, so an empty
  `TILE` would load the next full record.

`phantom_maindata.cam`'s `TILE` records are mostly empty: padding, since a
frame number is a position. None of its 2,608 frames names an empty one. Its low tiles are stock copies, its own art past the stock count. No
other Workshop or SDK archive has an empty record. Test:
`phantoms_haunt_names_no_empty_record`.

Later archives override earlier ones by section and record name, whole records.
No merging: two mods supplying `DATA/BDEP` give one table.

### What openregent holds in memory

All but `PICT` and `WAVE` payloads (`cam::DEFERRED`, `cam::Image`): of
`Data`'s and `DataMX`'s archives, 184 MB of Bink videos and 156 MB of sound
are left out, each in archives holding nothing else. Their index is
read with the rest, so lookup, override and empty records are unchanged;
a record's bytes are read from its own range of the file as it plays
(`Record::bytes`), an error if the file changed since. Tests:
`an_image_reads_every_record_as_the_whole_file_does`,
`a_file_changed_after_its_index_is_an_error`.

```text
 file    [header, directory, tables][DUNT][WAVE ......][DUNT][PICT ....]
 Image   [header, directory, tables][DUNT]             [DUNT]
```

### Contents

| Archive | Contents |
|---|---|
| `maindata.cam` | `TILE`, `IMAG`, `SPLT`, `CUT`; most of its bytes are tiles |
| `interfacedata.cam` | `TILE`, `IMAG`, `PALT`, `FONT` |
| `voices.cam` | `WAVE` |
| `soundfx.cam` | `WAVE` |
| `unittype.cam` | 394 `DUNT` (`the_base_datasets_unit_tails_read`), `DMOV` |
| `mx_unittype.cam` | 174 `DUNT` (`the_expansions_unit_tails_read`) |
| `sounddesc.cam` | `DSND`, `DSDP`, `DSDG` |
| `mx_sounddesc.cam` | `DSND`; phase and group tables not repeated |
| `textdata.cam` | `STRT`, `SMNU` |
| `action.cam` | `DACT` |
| `terrtype.cam` | `DTPT`, one `DTRN` |
| `gpltext.cam` | `STRT`, keyed — `AITX` what a hero is doing, `QUES` quest names, `DPTX` prerequisites in English |
| `mx_rgstext.cam` | `STRT` — freestyle scenarios, their blurbs, the lands |
| `miscdata.cam` | `DATA` — dependency tables, settings file, `datasets.xml` |

## `.qdd` — quest metadata

**Corpus**, read by `data::qdd`. Plain text, CRLF, `//` comments. Repeating
blocks; a `Name` key starts a quest:

```
Name =
[QUEST_ID]

Short Description =
[value, may span lines]

Long Description =
[value, may span lines]
```

`quests/maj.qdd` holds more than ten blocks; HD and the Workshop
mods ship six `.qdd` files (`every_shipped_qdd_parses`).

A developer comment says the quest name must match `RGSMissionTemplate`. Ids
match `.q` filenames (`barren_waste.q` ↔ `BARREN_WASTE`), linking metadata,
quest maps and the RGS layer: `quest_metadata_ids_match_quest_filenames`.

## `datasets.xml`

**Corpus.** Plain XML, root `<XmlDataRoot>`. Maps release
classifications to the GPL program each loads: `MajestyOriginal` as `Rel0`,
`MajestyDownloadable` as `Rel2`, plus a common group. No `Rel1`. Names `.bcd`
as GPL bytecode.

HD ships a fuller manifest, `MajestyDatasetDefinitions.xml`, naming every data
file and its role: `<CAM>` archives in load order, `<GPL>` bytecode,
`<GPLSource>`, `<Constants>` (naming `constants.rgs`), `<Strings>`,
`<Template>`. [`content::dataset`](../../crates/regent_core/src/content/dataset.rs) reads it.

## Duplicates

**Corpus.** Byte-identical pairs: `data/bytecode.bcd` and
`datamx/bytecode.bcd`; `data/xqd1_intro.cam` and `datamx/xqd1_intro.cam`. The
expansion re-ships some base files.
