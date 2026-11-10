# Inn peasants

The palace keeps one more peasant for every 2 inns its player owns.

Shows a GPL function replaced by name: the last loaded image owns a name,
and mods load after the game's scripts.

`GPL/peasants.gpl` replaces `peasant_spawner`, the thread a palace or
outpost runs as `peasant_spawn`. The game's spawns a peasant every
`#peasant_delay` (10 s) up to the building's `max_peasants`, then stops
until a peasant dies. This one adds the player's inns / 2 to a palace's
limit, and keeps running when full, so a new inn brings a peasant without
a death.

- Outposts get no extra peasants.
- An inn still being built counts (**Inferred**: `ListObjects` lists
  unfinished buildings).
- The palace panel's "x of y" still shows `max_peasants`: it asks
  `GetSupportedHenchmen`, which the mod leaves alone.

Test: openregent's `inn_peasants_adds_one_for_two_inns`.
