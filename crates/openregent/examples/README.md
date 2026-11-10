# Example mods

Source-only `mod.toml` mods, written for openregent. Each folder's
README says what it does and which mechanism it shows.

- `big_ranger_camp`: the rangers' guild holds 8. A number field set by
  a unit patch.
- `inn_peasants`: a peasant more for every 2 inns. A GPL function
  replaced by name.
- `guardhouse_pair`: a guardhouse upgraded to level 2 posts 2 guards. A
  `.dat` member value set from the manifest.
- `veteran_recruits`: a hero hired starts at its palace's level. Birth
  scripts set from the manifest to the mod's functions.

To use one, copy its folder into a mod folder
([formats/mods.md](../../../docs/formats/mods.md#where-mods-are-found)).
GPL sources compile when the folder is scanned. The manifest format:
[house_mend](../mods/house_mend/README.md).

Tests: openregent's `compiled::tests::the_example_mods_compile` and
`examples`.
