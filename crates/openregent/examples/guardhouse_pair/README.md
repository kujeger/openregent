# Guardhouse pair

A guardhouse upgraded to level 2 posts two guards instead of one.

Shows a `.dat` member value set from the manifest: `[[prototype]]`
names the instance `GuardHouse2` and sets its `Max_Guards` to 2. The
`.dat` file and the instance's other members stay the game's; no GPL is
added.

- `Max_Guards` is the guardhouse prototype's guard count, which the
  game's guard scripts read. A level 1 guardhouse is made from
  `GuardHouse1`, so it keeps one.
- An upgrade re-reads the members from `GuardHouse2`
  (`UpgradeAgentAttributes`), then the game's `RestartGuardSpawnThread`
  spawns the second guard.
- A guardhouse placed at level 2, as in a scene, posts one: the game's
  scripts start no second spawn there. HD does the same with the SDK's
  Adjust Guardhouse, a `.dat` that sets 2
  (`assets/scenes/guardhouse.toml`).

Tested on the expansion. **Inferred**: the base game's `GuardHouse2`
and guard scripts are alike, so it works there too.

Test: openregent's `guardhouse_pair_posts_two_at_level_2`.
