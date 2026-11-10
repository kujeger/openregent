# The art's camera and sun

How the sprites were rendered, as far as their outlines and painted shadows
tell. Used by shadow proxies, the simple 3D models `regent_draw::sun` casts
shadows from ([openregent/enhancements.md](../openregent/enhancements.md)).
Evidence markers: [README.md](../README.md#evidence).

## Terms

```text
X, Y   a building's ground axes; Z up. Origin: the sprite's anchor
φ      the sun's direction on screen: degrees from screen-right toward the
       viewer. Shadows fall the other way
el     the sun's elevation above the ground
az     the same direction in the building frame:
       az = atan2(-cos φ - sin φ, cos φ - sin φ)      φ 45 → az 270, φ 29 → az 286
```

## The art camera

**Inferred**, from edge slopes and fitted models: orthographic, yawed 45°,
elevation 35°.

```text
screen x      = (X - Y) / √2
screen y down = -((X + Y) / √2 · sin 35° + Z · cos 35°)
```

- Ground edges slope ±0.57 on every building checked (`ABV1`, `ABE2`,
  `ABM1`): ground ×0.574 = cos 55°.
- Vertical edges are vertical, so the camera is orthographic.
- Height ×0.819 (cos 35°) assumes a rotated orthographic camera; one view
  cannot confirm it.
- A box model drawn with this projection lines up with ABV1's tower to the
  pixel.

This differs from the engine's ground: `settings::ISO_CAMERA_ANGLE` gives
sin 55° = 0.819 for ground depth and cos 55° for height. The two need not
agree: a building's footprint is its base polygon with y divided by
sin 55° (`sim::placement::unproject`), so the board takes the art's ground
as drawn. A model turned by the art's camera (`regent_draw::frame::ART`)
casts on flat ground where the art's camera would (`regent_draw::sun`). The
same turn puts each of its sprite's pixels on the model, where another
model's shadow falls on it (`regent_draw::sun::relief`).

## The sun

**Inferred**: sun fits matching each sprite's darkening pixels (247–250, see
[formats/art.md](../formats/art.md#where-a-shadow-falls)). A sprite paints only
the shadow on open ground, never the part behind the body. Where a model
casts, only that ground shadow is dropped: a part's darkening, such as the
Guardhouse's flag, stays (`Sprite::aloft`,
[formats/art.md](../formats/art.md#where-a-shadow-falls), `Cel::aloft`), and a
damage stage's flames and smoke too.

Three fitting models, with what they are worth:

| Model | What it is | Error |
|---|---|---|
| Box | Hand-built boxes, fitted to the outline | A few degrees |
| Billboard | Each body pixel stands over the anchor row (characters) or the base polygon's mid-line (buildings) | Matches the box fit on the Wizard Tower; up to 30° off on deep buildings |
| Slab | Per column, the largest volume inside the base polygon that keeps the outline | Right on the Guardhouse, a third of the tower's height on the Warriors' Guild |

### Characters

**Measured**, no test: billboard fits, `Stand` set, every facing of each character fitted.
Facings of one character agree within a few degrees.

| | φ | el |
|---|---|---|
| Median of the per-character medians | 46.8 | 54.3 |
| Median of the samples with IoU ≥ 0.5 | 46.1 | 54.2 |
| Spread (sd) of per-character medians | 5.0 | 3.6 |
| Tightest: Healer, Monk, Priestess, Witch King | 44–52 | 55–57 |

Bulky bodies fit worst (Rock Golem, Minotaur, Yeti, Stones; IoU < 0.5).

### Buildings

**Measured**, no test:

| Building | φ | el | Fit |
|---|---|---|---|
| Warriors' Guild `ABV1` | 28.5 | 51 | box, IoU 0.79 |
| Guardhouse `ABE2` | 25 | 55 | box |
| Wizards Tower `ABM1` | 26 | 53 | box, and its shadow's two cone tips |
| Goblin Watchtower, Ballista Tower | 38 | 49–59 | slab |
| Wooden Banner (4 variants) | 35–46 | 52–54 | slab |
| Mausoleum | 47 | 42 | slab |
| Goblin Fortress, Liche Queen Lair, Dwarven Settlement | 29–41 | 49–52 | slab, poor (IoU < 0.5) |

Median of the reliable ones: φ 38, el 53.

### What the numbers say

**Inferred**, from the fits above:

- **Elevation is shared**: 51–55° for characters and buildings alike.
- **Direction is not**: characters φ ≈ 46°, buildings φ ≈ 25–38°. The
  detailed `ABV1` model scores 0.42 at φ 45° against 0.79 at its own fit,
  so the difference is not the model.
- **The terrain's light is another sun**: `settings::LIGHT` is φ 45°,
  el 38°. Its direction matches the characters; its elevation matches
  nothing here (`ABV1` scores 0.22 under it).
- **No close light**: the Goblin Fortress's shadow lobes run within 11° of
  parallel across 400 pixels. The Dwarven Settlement (44°) and Mausoleum
  (28°) spread more, but their sails and roofs could make those shapes.
- **φ 45° is a building's wall direction.** Every building is yawed 45°, so
  under a sun at φ 45° walls cast no visible side and a shadow continues the
  walls in a straight line. Off-axis (φ ≈ 29°), each wall casts a wedge, as
  the art does.

### A default

**Game** (by eye on the `ABV1` renders): φ 29°, el ≈ 50° looks right for
buildings. One sun for every shadow puts characters about 15° off their
painted direction; tune it in game before settling a default.

## Open

- Why characters and buildings were lit from different directions.
