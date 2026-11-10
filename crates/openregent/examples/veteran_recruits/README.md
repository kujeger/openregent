# Veteran recruits

A hero hired from a guild starts at its player's palace level: 1, 2 or
3. A player without a palace hires at level 1.

Shows a function member set from the manifest: each hero's
`birthScript` names a function of the mod. No shipped function is
replaced.

`GPL/veteran.gpl` holds the new birth scripts. Each runs the game's
(`hero_birth`, the adept's `adept_birth`), then:

- the palace: the game's `GetTruePalace`, its level `$GetBuildingLevel`
  ([mods.md](../../../../docs/formats/mods.md#openregents-builtins));
- the level: the game's `advance_to_level`, as its Veteran Heroes event
  and two quests raise a hero at birth. Each level adds hit points and
  maximum hit points, attack and defence skills, and a primary stat
  point on odd levels, as a level earned does. The level up effect and
  voice line are left out; experience stays 0.

Only a hero whose `home` survives `hero_birth` rises: one from a
building with members. A hero a script spawns at the palace stays at 1.

Expansion only: the base game's adept has no `adept_birth`.

**Inferred**, untested:

- an outpost's hero rises too, if its outpost had a free slot;
- a hero a quest script spawns at a guild rises too;
- every class but the warrior; the test hires a warrior.

Test: openregent's `veteran_recruits_start_at_the_palace_level`.
