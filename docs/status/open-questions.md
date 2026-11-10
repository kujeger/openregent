# Open questions

Unknowns about the original that might change play; openregent plays
every quest as far as known. Read
[settled.md](../settled.md) before adding. Evidence markers:
[README.md](../README.md#evidence).

| # | Question | Known | Settles it |
|---|---|---|---|
| 65 | Do the player's spells no trace casts do in the game what the world does? | **Binary**, the engine's spell apply and target check; the scripts the SDK's: the healings, Resurrection, ReAnimate, Illusionary Hero, Animate Bones, Frost Field, Chain Lightning, fire strike on a unit, lightning bolt on a building, the Abode's timer ([vm.md](../gpl/vm.md#the-players-spells)). **Measured** casts each in a no-log game: paid, scripts clean, every builtin answered. **Game**, the wizard trace: Gate, its unit's search, two heroes teleported to it where the probe has them, the Abode's words cleared by the fast bar's message (`World::abode_bar`). **Game**, the spells trace (**Measured**): SuperCharge twice; Dismiss on a tax collector, sent to the map's far edge where the probe has it; the Gate's hits on the first listed spell unit, a Farseeing twice ([vm.md](../gpl/vm.md#the-gates-destination)). **Inferred**: the clone's world draws and name, nothing paid when refused | A trace of the original casting each once, an Abode spell twice inside its timer, a healing on a whole hero, Illusionary Hero and Resurrection on a grave: the decode frames, the clone's place, the teleports' places by `911 P` |
| — | The camp move and embassy against the game | Read from the binary only (`world::guild::camp`, `world::guild::embassy`): no trace holds a `GUILD_ACTION` 0 or `0x16`, or a `PlayerResourceMessage`. **Inferred**: no flatten or pad at the new camp; an embassy timer's first tick in its next ORDR turn; the open and close button always ready | A traced game moving a camp and opening an embassy |
