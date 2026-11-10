# Big ranger camp

The rangers' guild holds 8 rangers instead of 4.

Shows a number field set by a unit patch: `set = { MaxGuildMembers = 8 }`
on `ABW1`, `Rangers_Guild`. A new guild copies it to its
`MaxGuildMembers` attribute, the limit the engine recruits against.

For the expansion only. Its `GuildHasOpenSlots` reads that attribute;
the base game's reads the `.dat` instance's `max_members`, which a unit
patch does not change, so there homeless rangers would join up to 4
only.

A guild finished before the mod was enabled keeps 4. The Legendary Heroes
quest still sets 1.

Test: openregent's `big_ranger_camp_holds_8_rangers`.
