# Architecture, from the class map

Source: MSVC RTTI in `MajestyHD.exe`: every class with a vtable, its
method slots and its ancestors.

- Readings are **Inferred** from names unless stated. Structural claims (what
  derives from what, method counts) are **Binary**, from the RTTI.
- A method count is weak evidence: `TerrainFeatureRec` has 135 and `IsoGenUnit`
  121, which suits a deep
  hierarchy, not a fat class.

## Layering

```
  GS*      Majesty-specific game rules      (most spelled GS_)
  GE*      Reusable game engine             (most spelled GE_)
  Cy*      Cyberlore platform framework     (most spelled CY)
```

**Inferred:** `CY` is Cyberlore's cross-platform framework (app lifecycle,
rendering, sound, files, dialogs, fonts); `GE_` a generic strategy engine on
top (units, levels, orders, terrain); `GS_` is Majesty proper. `CY` has no game
logic and is replaceable; `GE_` plus `GS_` must be reproduced faithfully.

**Binary:** the prefixes are an inheritance convention. Fourteen `GS` classes
derive from the same-named `GE` class — `GS_Player`/`GE_Player`,
`GSINIFile`/`GEINIFile`, `GS_OrderProcessor`/`GE_OrderProcessor` — and
`GS_CYDialogManager` → `GE_CYDialogManager` → `CYDialogManager` is three layers
in one chain.

`data/` also has a `mdl1_*` set (`mdl1_unittype.cam`, `mdl1_maindata.cam`,
`mdl1.bcd`): a second dataset on the same engine.

## The scripting VM

```
GplTapeMachine            GplTapeMachineContainer   GplTapeStore
GplTapeStoreContainer     GplInterpreterUtility     GplSymbols
GplFunction               GplFunctionSet            GplExpressionHandle
GplDispatcherHandle       GplAgentRef               GplDataSet
GplDataInitializationTable GplType                  GplNull
GplBoolean  GplInteger  GplFloat  GplString  GplList  GplCoord
GTS_Frame   GTS_OperandNode   GTMArgumentVector
ExpressionTypeDecoder     SymbolEntry               GplException
```

"Gpl" is *Game Play Language* (SDK). Much game logic runs in this interpreter.

- **Inferred** from the class names only: `GplTapeStore` holds the program
  ("tape"); `GplTapeMachine` runs it.
- **Inferred** from the class names only: `GTS_Frame` and `GTMArgumentVector`
  are call frames and argument passing — a procedural language, not a trigger
  table.
- Nine boxed value types under `GplType`: `GplBoolean`, `GplInteger`,
  `GplFloat`, `GplString`, `GplList`, `GplCoord`, `GplNull`, `GplAgentRef`,
  `GplFunction`.
  Agent references and functions are values. These classes are not the
  declarable types: `agentref` is not declarable. See
  [gpl/language.md](../gpl/language.md).
- `GTS_OperandNode` and `ExpressionTypeDecoder` suggest a tree walk, but the
  bytecode is three-address code: expressions numbered one slot per node in
  post-order; a call names its result and operands by slot. See
  [formats/bytecode.md](../formats/bytecode.md).
- **Inferred:** `GplAgentRef` and `GplDispatcherHandle` bridge to game objects;
  hero behaviour and quest logic are scripted through them.
- **Inferred:** `GplDataInitializationTable` binds script-visible constants at load.

`GplTapeMachine`, `GplTapeStore`, `GTS_Frame`, `GTS_OperandNode` and
`ExpressionTypeDecoder` have one-entry vtables (a destructor). The interpreter's
work is in non-virtual methods, out of RTTI's reach.

## Simulation: message-driven

```
Message  (base)
  MoveMessage  HaltMessage  ActionMessage
  CreateObjectMessage  ManipulateObjectMessage
  CreateBuildingMessage  DestroyBuildingMessage  UpgradeBuildingMessage
  EnterBuildingMessage  ExitBuildingMessage
  CreateVehicleMessage  DestroyVehicleMessage
  AllocUnitMessage  DeallocUnitMessage  SetUnitNameMessage
  AlterAttributeMessage  AlterAttributeDeltaMessage
  AlterPlayerDictionaryMessage  PlayerResourceMessage  PlayerSpellMessage
  CreateRewardFlagMessage  DestroyRewardFlagMessage  DestroyEventFlagMessage
  GuildActionMessage  PauseGameMessage  CheatCodeMessage
  ExecutionAdjustmentMessage  CompoundMessage
MessageBundle  MessageManager  GSMessageManager
```

- **Binary:** the simulation's input vocabulary is the 29 classes deriving from
  `Message`, taken from the hierarchy, not by spelling. `MessageBundle` is
  itself a `Message`, so the enum needs a recursive case. No `*Message` sits
  outside the hierarchy.
- **Inferred:** deterministic lockstep — peers exchange messages, not state.
  Evidence: `CNMasterSlaveGame`, `MemSerialBlock_serialize_Imp` /
  `_unserialize_Imp`, `SerUnserialize_AttributeMap`, and GPL's fixed-point
  `float`, which keeps clients from diverging through rounding.
- **Inferred:** message serialisation is the network path too;
  `ExecutionAdjustmentMessage` is lockstep clock correction.
- The library: `sim::game::Input` is the player's messages the model
  decodes, `Game::apply` its one decoder, as `GSMessageManager` has one,
  called from the frame step before the clock step.
  `sim::game::orders` is what the interface controllers send for a
  press or click, in decode order; a front end asks it.

## Order system

```
OrderHeader  OrderEntry  OrderStack  OrderTracker
OrderMove  OrderMovePath  OrderFollowPath  OrderFollowUnit  OrderFollowClone
OrderMoveMissile  OrderPhysics  OrderPhysicsMissile
OrderAction  OrderInitAction  OrderAnimate  OrderAnimateExtended
OrderTurn  OrderWait  OrderCounter  OrderFade
```

Per-unit behaviour is a stack of orders. Majesty-specific ones:
`GS_MarketDayOrder`, `GS_GambleOrder`, `GS_ResearchOrder`,
`GS_WizTowerEnchantOrder`, `GS_RubbleRemovalOrder`, `GS_DelayedRecruitOrder`,
`GS_EmbassyRecruitOrder`, `GS_Order_Earthquake` — heroes go to market, gamble
and research on their own.

`GS_OrderProcessor` and `GE_OrderProcessor` drive it; `GSOrderCombatSignal`
interrupts it. The player issues none, so a save's `ORDR` chunk, if it holds
orders, holds the simulation's own; see [formats/saves.md](../formats/saves.md).

The library runs a frame's orders in `World::step`: paused at each engine
dispatch they make, which the caller runs before `World::resume`.

## Data-driven descriptors

A `*Desc_Engine` record type pairs with a `GE_*DescList` container. **Binary,
HD:** two hierarchies — `GameDescSubPart` over 18 record types, `GameDescList`
over 13 containers. A
reimplementation's descriptor layer should have that shape.

| Descriptor classes | Source file |
|---|---|
| `UnitDesc_Engine`, `CommonUnitDesc_Engine`, `CommandUnitDesc`, `GE_UnitDescList`, `GS_UnitDescList` | `unittype.cam` |
| `TerrainDesc_Engine`, `TerrainPartDesc_Engine`, `GE_TerrainDescList`, `GE_TerrainPartDescList` | `terrtype.cam`, `tilesetdata.cam` |
| `ActionDesc`, `ActionDesc_Std_Engine`, `Game_ActionDesc`, `GE_ActionDescList`, `GS_ActionDescList` | `action.cam` |
| `SoundDesc`, `SoundDesc_General_Engine`, `SoundGroupDesc_Engine`, `GE_SoundDescList`, `GE_SoundGroupDescList`, `GE_SoundPhaseIDDescList` | `sounddesc.cam` |
| `WeaponDesc_Std_Engine`, `GE_WeaponDescList` | `unittype.cam` — the `WEAP` slots of `DUNT`'s `PRIM` |
| `ExplosionDesc_Std_Engine`, `GE_ExplosionDescList` | `unittype.cam` — the `EXPL` slots |
| `MovementDesc_Linear_Engine`, `GE_MoveDescList` | `unittype.cam` — the `MOVE` slots point at its `DMOV` records |
| `ProjectileDesc_Engine`, `ParticleSystemDesc_Engine` | `unittype.cam` — the subtype block of `DUNT`'s `PRIM`. The particle one is 272 bytes and feeds `CY3D_ParticleSystem` / `CY3D_PS_Particle` |
| `GE_FactionDescList` | `company.cam` — the `DFCT` records |
| `OverlayDesc_Engine`, `GS_OverlayDesc`, `BuildingDesc`, `VehicleDesc` | — |

See [formats/payloads.md](../formats/payloads.md).

## Map generation

```
RGSChart  RGSCellChart  RGSFractalChart  RGSForceChart  RGSLandscapeChart
RGSMobjectChart  RGSRegionChart  RGSSurfaceChart  RGSTerrainChart  RGSUnitChart
RGSPattern  RGSDistributionPattern  RGSLayoutPattern
RGSDistNode  RGSLayoutNode  RGSNode
RGSFractalSettings  RGSSetOfRegions  RGSSetOfDistPatterns
RGSSetOfLayoutPatterns  RGSSetOfFractalSettings  RGSPlayer
```

A chart-driven procedural map generator, configured by the `.rgs` files
(`constants.rgs`, `constants_down.rgs`, `mx_constants.rgs`,
`mx_constants_down.rgs`). Five of these classes are the five sections of an
`.rgs`, matched by what each section references; see
[formats/maps.md](../formats/maps.md).

Freestyle mode (`quests/freestyle.q`, `FreestyleIO_Controller`) uses it and
takes a seed, so the generator is checkable.

## Platform framework

HD has more.

- **Archives:** `CYLibManager`, `CYLibTranslator` and `LibraryController` are
  the `.cam` layer. `CYLibLink` is not: it is the resource base class, over the
  thirteen classes loaded *from* an archive — `CYSprite`, `CYPaletteArray`,
  `CYOffport` and variants, every sound buffer.
- **Rendering:** `CYRenderDevice`, `CYSoftwareRenderDevice`, `CYSoftwareShader`
  in `_555`, `_565`, `_555_AccurateColor`, `_565_AccurateColor`. Software only,
  16-bit colour, a polygon path rather than a blitter — hence ground of
  displaced geometry. `CYOffport` is an offscreen surface.
- **Sprites, fonts, palettes:** `CYSprite`, `CYPixSprite`, `CYPictDataReader`,
  `CYPictStdReader`, `CYPaletteArray`, `CYFont`, `CYFontImage`,
  `CYFontPixelmap`. Team colour is a palette-range swap, not separate art
  (`CYPaletteArray` has `SetPalette`, `RefreshPalette`, `SharePalette`; the game
  side is `GEPlayerColors`), and the shipped game switches it off; see
  [formats/payloads.md](../formats/payloads.md).
- **Isometric 3D:** `CyIso3DObj`, `Cy3DObj`, `Cy3DObjDesc`, `CY3DMap`, and the
  `CY3D_ParticleSystem` pair.
- **Sound:** `CYSndManager` with `CYNULLSndManager`, `CYSndBuffer`,
  `CYSndStreamInfo`, `CYSndBlockStreamInfo`,
  `CYSndBRStreamInfo`.
- **Strings:** `CYStrTbl`, `CYStringStrTbl`. **Inferred:** for `textdata.cam`
  and `gpltext.cam`.
- **Platform split:** `CYApp`, `CYFileSystem`.

## UI

HD has more. The hierarchy below is HD's.

```
DialogController                 74 classes below it, the UI's real root
  MainFrameController            just another one of them
  StatsWindowController          55 below it
    Base_BuildingController
      Base_GuildController
      Base_ResearchedBuildingController
        BlacksmithController     the deepest game class, five levels down
    Base_UnitListController
    Base_VehicleController
```

Eight `*Controller` classes are outside it: `GameviewController`,
`KeyboardController`, and the GPL debugger's six, rooted in
`GplDebugView_BaseController` (18 classes below it).

The debugger's archive is read — five panels as six records
(`the_gpl_debuggers_panels_are_five_shipped_as_six`), see
[formats/panels.md](../formats/panels.md) — and names no opcode, so it does not
explain opcode 39. Running it may show what section 2 is *for*.

The building controllers list the buildings to support: `BlacksmithController`,
`MarketplaceController`, `WizardsGuildController`, `TempleToKrolm_Controller`,
`GraveyardController`, `LairController`.

## Odds and ends

- `Thread_NeedHeroes`, `Thread_LowTreasury`, `Thread_DwarvesAvailable`,
  `Thread_ElvesAvailable`. **Inferred:** storylines triggered by game
  conditions, not OS threads.
- `Morgue`, `GravesRegistration`, `GraveyardController` — **Inferred:** hero
  death and resurrection bookkeeping.
- Netcode is behind an interface: `CNGrappleTransport` and `CNGrappleSession`
  derive from abstract `CNTransport` and `CNSession`. A reimplementation drops
  Grapple.
- `NameGenerator`, `NameGeneratorIMP`, `NameGeneratorManager` — procedural hero
  names, which a save's `SECD` holds.
- Hot keys: `HotKey`, `HotKeyList`, `HotKeyManager`, `FastHotKeyList`,
  `BuildingHotKeyList`, `SpellHotKeyList`; their names are the `HKTX` string
  table.
- Pool allocators, which a reimplementation ignores: `ArrayPool`,
  `PathGridArrayPool`, `PathNodeArrayPool`, `IsoDrawEntryMemPool`.
- `PathGrid` is the pathfinding structure. Two view modes, `IsoGameView` and
  `OverheadGameView`, with `GSIsoGameView` / `GSOverheadGameView` on top, all
  from `GameView`. The `Info` bitfield's two visibility flags are per view, so
  there may be two projections to support.
- Every placed-thing list derives from `ObjList`: `BuildingList`,
  `VehicleList`, `AllUnitsList`, `TerrainFeatureList` and eight more — the same
  set as the save's object chunks.
- **Inferred:** the `*IMP` suffix marks pimpl implementation classes —
  `CYOffportIMP`, `NameGeneratorIMP`, `DialogViewManagerIMP`,
  `KeyboardControllerIMP`. Composition leaves no trace in the hierarchy, but
  three of the four have no ancestors, which fits. `CYOffportIMP` derives from
  `CYLibLink`, so it is a loadable resource itself.
