# Source-only mods

`mod.toml` is a openregent mod manifest. This mod gives peasant houses `Heals`: 1 HP every 5 game seconds.

```toml
version = 1
id = "{A2A48A02-2BCE-42B5-8D2F-22B59DB3B720}"
name = "HouseMend"
display_name = "Peasant houses mend themselves"
base = "Any"

[[unit]]
id = "ABe1"
field = "building"
add = ["Heals"]
# clear = ["NotBuildable"]
# set = { MaxHP = 120 }
```

`base` is `Any`, `Majesty`, or `MajestyExpansion`. `description` and `details` are optional.

## Unit patches

Each `[[unit]]` changes the current descriptor of unit `id`. Patches run after archives and XML descriptions; later patches win.

- `field` is `info`, `building`, or `character`; `add` and `clear` name its flags, case sensitive, as listed in [`unit.rs`](../../../regent_core/src/data/desc/unit.rs). Within an entry, `clear` follows `add`. Only the named bits change.
- `set` writes number fields by their XML names: for a building `Cost`, `Multiplier`, `IncomeType`, `IncomeAmount`, `MaxHP`, `MaxGuildMembers`, `SightRange`, `Gold`; for a character `Cost`, `Experience`, `MaxHP`, `SightRange`, `Speed`, `AttackRangeMin`, `AttackRangeMax`, `Vitality`, `Charisma`, `Artifice`, `WillPower`, `Intelligence`, `Strength`, `MagicResistance`, `Attack`, `RangedAttack`, `Parry`, `Dodge`, `WeaponBasicDamage`, `ArmorBasicDamage`, `Gold`, `RecruitDelay`, `PrimaryStat`. Values are whole numbers 0 to 2147483647; `IncomeAmount` may be negative; `Multiplier` is a number not below 0. No `field` is needed for `set` alone. The [Big ranger camp example](../../examples/big_ranger_camp/mod.toml) sets `MaxGuildMembers`.

## Member values

Each `[[prototype]]` sets member values of a GPL `.dat` instance, as if its `.dat` said them, without replacing the file:

```toml
[[prototype]]
instance = "GuardHouse2"    # the instance, as [GuardHouse2] in the .dat
set = { Max_Guards = 2 }    # members, as its prototype declares them
```

- Names are case insensitive. A member must be one the instance's prototype declares; other members keep their `.dat` values.
- A value fits the member's type: an integer 0 to 2147483647, a float, `true` or `false`, a string, or the name of a function. List, coordinate and agent members cannot be set.
- An agent made from the instance, or upgraded to it, takes the value. The [Guardhouse pair example](../../examples/guardhouse_pair/mod.toml) sets `Max_Guards`.

## Builtins

A mod's scripts may also call openregent's own builtins, such as `$GetBuildingLevel(agent)`, listed in [mods.md](../../../../docs/formats/mods.md#openregents-builtins).

## Sources

For source files, add `[load] descriptions = ["Data/units.xml"]` or `strings = ["Data/text.xml"]`. GPL sources use `[[script]]` with `name` and `sources = ["GPL/example.gpl", "GPL/example.dat"]`. openregent compiles and caches them. The [Inn peasants example](../../examples/inn_peasants/mod.toml) uses a `.gpl` source. The new format accepts no `.bcd` or `.cam` files.

Saves record a hash of the unit patches and member values and refuse changed ones on load. Invalid units, flags, fields, values, unit types, instances or members stop loading.
