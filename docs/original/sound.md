# Sound

When the engine plays a sound, how loud, and the music. Records:
[payloads.md](../formats/payloads.md) (`WAVE`, `DSND`, `DSDG`, `DACT`). Code:
`regent_core::view::sound`, `regent_core::sim::world::sound`,
`openregent/src/audio.rs`; tests `tests/sound.rs`.
**Binary** (`MajestyHD.exe`) unless marked.

## What plays, and when

A sound is a `DSND` record and one of its phases (a `DSDP` id). A unit's own
record is its `PRIM` `DefaultSound`.

| Trigger | Record, phase | Placed |
|---|---|---|
| `PlaySound(agent, phase[, float])` | the unit's own | at the unit; no unit, nothing |
| `PlaySound(agent, sound, phase[, float])` | named | at the unit; no unit, unplaced |
| `PlaySound(sound, phase[, player])` | named | unplaced, distance 10,000; another player's skipped |
| an `InitAction` pushed | the action's `Sound`, else the actor's own; `SoundPhase begin` | at the actor |
| an `InitAction`'s peak | same; `peak` | at the actor |
| an effector made | its own; `EEB0 Effector Begin` | at it |
| a player's spell unit made | its own; `EEB0` | at it |
| a player's building placed | `PB01 Place_Building`, `EBE0 Begin` | at it, as its owner's |
| a rangers' camp moved, after `MoveCamp` | the same | at the guild, as its owner's |
| a reward flag made | `PR01 Place_Reward` for an attack flag (`ARA2`), else `PE01 Place_Explore`; `Begin` | unplaced, as its owner's |
| research done | the building's advisor (`RA12`…`RA34`, `world::sound::ADVISORS`; **Binary**), `GVS5` | unplaced, as its owner's |
| the advisor's threads: low treasury, no hero, elves or dwarves available | `RA09`, `RA11`, `RA02`, `RA03`, `GVS5` ([bytecode.md](../gpl/vm.md#the-advisors-lines)) | unplaced, the local player's |
| a placing click the preview refuses, a building's or a camp's | `ER01 Error_Alert`, `Begin` | unplaced, the local player's |
| the local player selects a unit | a site (`APB:1` 0): `BP43 Construction` `Begin`, unless `APB:54`; else its own `ESL0 Select`, unless `APU:14` | at it, as its owner's |
| the `"` key | the selection's own `EG01`, no attribute asked | at it, as its owner's |
| a unit shows a set | its own, looped: `Special-n` `AMI`, `Build-n` `AMB`, `Die-n` `AMD`, `Active-n` `AMA`, `Inactive-n` `AMN`, with the stage digit; other sets stop it | at it |
| a unit dies and is deleted | its own; `EDH0 Death` | unplaced |

Tests: `an_actions_sound_phases_are_the_sdks`,
`a_refused_click_sounds_the_error_alert`,
`the_voice_key_plays_a_warriors_eg01`,
**Measured**.

Nothing plays `SoundPhase end`. A script's float is the `DistanceModifier`.
The decodes, the camp, the flag and the advisor go to the sound manager
directly, with no sight test.

The order panels: a building that fits sounds at its decode, not at the
click; picking one in the menu, recruiting, researching, casting and
upgrading sound nothing at the click. Besides the two `ER01`, the only
other panel sound is the rogues' guild control 8009's `EN01 Extortion` (not
offered in openregent). **Inferred**: no button reaches a unit's sound
another way. A message flag plays `ME02 New_Message`, unplaced, the local
player's (`world::message`); `ME01` and the options' sound tests are not
followed.

A unit's sound:

- Unseen by the local player, it plays only at distance 10,000; an unseen
  unit's ambient loop stops. Seen: the unit's sight word for the local
  player above 0 — one player's word, a held flag's its holder's
  (`World::sight_word`). The selection's sound plays unasked.
- A voice phase, id opening `GV` (`VFX_*`, `Taunt`), is skipped while the
  unit says one. Voices play on DirectSound category 6, the rest on 2.
  Test: `a_speaker_says_one_line_and_cues_stop_at_sixteen`.

## Groups

A phase's `DSDG` group gates its start on real time: at least `interval` ms
since the group's last start, and fewer than `limit` playing. A held-back
phase waits in a queue if `CanQueue` or its group's flag is set
(`Advisor_Group`), retried as groups free; else it is dropped.
`Palace_Sieged_Group`: one, 45 s apart. Tests:
`a_sound_group_is_a_limit_and_an_interval`,
`a_group_holds_back_by_its_limit_and_interval`.

## Loudness

`DistanceModifier` `d`: the phase's, unless the cue gives one.

- 10,000: full volume, centred, anywhere.
- 10,001: the owner's at 10,000; anyone else's mixed at 10,001.
- 10,002: the owner's at 10,000; no one else hears it.

The listener is the view's centre in cells, moved as the view moves.
Hearing reaches `R = SoundViewRadius × width / 32` cells, the view's width
in pixels; `SoundViewRadius` 1.5, plus 1.0 zoomed out
(`the_sound_view_radius_is_the_binarys`). `view::sound::mix`:

```text
s = |at - listener|² / d          (d 0 or 1: undivided; 10,000: s = 0)
s > R²                            silent
r = √s / R
volume = 10,000 - ftol(3300 r)    DirectSound: volume - 10,000, in 1/100 dB
pan    = ftol(±667 r × fold(θ)/90) θ the angle off +x, fold past 90°;
                                   negative (right quieter) for +y, south
```

A placed sound keeps neither its phase's set volume nor its volume spread:
placing and playing the buffer write the mix's volume and pan over them
(`view::sound::Play::kept`). Only an unplaced sound keeps them.
`Place_Building`'s set volume 5,000 is −50 dB.

The listener faces `+x` and nothing turns it. So a sound north or south of
the view's centre pans, one east or west does not
(`facing_east_pans_north_south`). openregent turns the facing to `+y` so
sounds pan by screen side (`facing_south_pans_by_screen_side`,
[openregent/enhancements.md](../openregent/enhancements.md)). Unplaced
sounds: full volume, centred. The spreads, `VolumeVariation` and
`FrequencyVariation`, draw once at the start on the paint RNG: `up + down`,
less `down`, in 1/100 dB and Hz; the volume's lasts only unplaced.

A category's volume (options) scales it. `MaxConcurrentSounds` and
`AudibleVolumeLimit` have no reader ([settings.md](../formats/settings.md));
the sound manager gives each category 16, its use not read.

## Music

HD's music is MP3s in `Music/`, named by `Data/MusicTracks.txt`, one per
line. `PlayMusic(n)` takes line `n`, from 1; a line opening with a digit is a
CD track. The same track playing is not restarted. `$Freestyle` runs
`Setup_Freestyle_Music`: callback `FreeStyle_Music_Stopped`, track 3,
`EarlyGame.mp3`.

When a track ends, in real time, the frame step's music update dispatches
the callback with the track that played (`Game::music_stopped`).
Freestyle's plays `LastMusicTrack() + 1` up to 5, then 3: early, mid, end
game. The quest callback alternates 3 and 4. Its source says it must not
change game state, as a machine with the music off runs none: here it runs
aside, its track the machine's own until the next `PlayMusic` in play, kept
by the front end, not the world. It draws no RNG. A digital track silent
for a while starts again. Lines 1 and 2 are `GeneralTheme.mp3`;
`EpicQuest.mp3` is line 6. **Corpus**: `EarlyGame.mp3` and `MidGame.mp3`
are one size but differ.

At the game's end, state 8's first step stops the music and every sound,
then says the advisor's `RA26` Advisor_Won_Game or `RA27`
Advisor_Lost_Game, phase `GVS5`, unplaced; the end panel waits for it
(`End::advisor`). The music starts again: the stop leaves the track set,
and the update, run in state 8 and each dialog pass, restarts the silent
track at its next poll. States 7 and 8 pause every buffer playing then but
the music's, category 10. **Inferred**: the stop runs no end callback, so
the same track restarts, not the scripts' next.

`AP60` opens after the advisor and the fade (state 7) and says the quest's
`ED`nn voice: `ED` and its slot's digits, `ED00` `End_Barren_Waste` saying
`EN00`, first phase `VFX_ADVISOR`, unplaced, kept. Started after the pause,
it is heard. Not freestyle; `MD01`, `VAMP` code 0, silent; `XQD1`'s `ED31`
does not ship. `AP61` says nothing. Any press on the panel stops it first.
`menu::end_voice`; tests `every_won_quest_says_its_end_voice`,
`a_won_quest_says_its_slots_end_voice_a_lost_one_none`.

### The front end

**Binary**. `view::menu::music`; tests
`the_front_ends_tracks_are_the_theme_and_the_epic`,
`the_main_menu_and_map_play_the_theme_a_briefing_the_epic`. The menus and
the game share one player, and the dialog loop runs its update, so a menu
track loops as a game's does.

| When | Track |
|---|---|
| start-up, after the intro videos | 2, `GeneralTheme.mp3` |
| main menu `AP29`, each entry | 2 |
| quest map `APdb`/`AP30` opening | 2 |
| briefing `APc9`, summary `AP44` | 6, `EpicQuest.mp3` |
| name prompt, options, the map's dialogs | none asked: what plays plays on |
| Cinematic | stopped for the video, 2 after |
| Credits | no call: plays on under the videos |
| a quest's setup, `PlayMusic(3)` | 3 at the next update: 6 cut, no fade |
| the end panel's Main Menu | 2, `AP29` |

A track the player holds is not restarted: main menu to map keeps the
theme, a briefing's Cancel starts it over. A track is played once; the
update looks for silence every 500 ms and starts the track again at the
first silent look, so a gap under half a second.

Sounds: a press going down on an enabled control plays a `WAVE` directly,
by class: `CK05`, check and radio `CK04`, list box `CK07`; scroll bars,
combo and text boxes none. Category 1, set by the effects volume. Letting
go, hovering and a page change play nothing; Erase Victories' Yes clicks by
its class. Tests: `a_button_clicks_five_a_check_four_a_list_seven_a_scroll_bar_none`,
`every_click_is_a_shipped_wave`, `every_front_end_press_clicks_five`.
`soundfx.cam`'s `OS01`–`OS10` ("OS Map", "OS Horn", "OS Candles"…), the
room objects' sounds by name, are named by no executable or data file.

## In openregent

`World` keeps the cues; `openregent` plays them, the selection's, the ambient
loops, the refused click's and the music. Sight is the game's: the `L`
debug view, lifting the fog, changes nothing heard. **Inferred** there: the
local player is the crown; heights are 0; a sound follows its unit as it
moves; 16 effects at most. At the end it stops the music and every sound
and plays the advisor; the music does not start again. The advisor's
threads speak as the world polls them (`World::with_advisors`). The menus
play their screens' tracks on the same player (`audio::front`), a game that
has asked none keeps the menus', and a silent track starts again a whole
poll after it ends (**Inferred** stand-in for 0 to 500 ms). A press going
down clicks (`menu::paint::arm`, `audio::ClickSounds`).
