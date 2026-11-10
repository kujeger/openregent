# Bug fixes

Fixes for faults in the shipped scripts. One so far.

## Hidden heroes are paid

Aura of Peace sets a healer's `"type"` to `"hidden"`. The expansion's
explore flag counts a hidden hero as arrived, then deletes the flag. Its
payout, `DropGoldInRadius_SamePlayer`, lists only `"hero"`, `"invisible"`
and `"Camouflaged"`: the hidden hero gets nothing, and alone, the gold is
lost.

`GPL/gold.gpl` replaces the function, adding a `"Hidden"` search. The last
loaded image owns a name, and mods load after the quest's.

Only explore flags call it. Attack flags and deaths pay through
`DropGoldInRadius`, unchanged.

A Ranger journeying off the map is `"Hidden"` too
(`mx_Journey_Offmap.gpl`), so one within reach of an explore flag is paid
as well (**Inferred**: whether it can stand that near is unchecked).

## Adding a fix

Put the fixed script in `GPL/`, list it under `sources` in `mod.toml`, and
add a line here.
