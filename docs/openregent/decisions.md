# Structural decisions

- **Archive bytes are leaked, once per file.** `view` borrows; a Bevy
  `Resource` must be `'static`. `regent_draw::data::archives` reads each
  file once and leaks it, so a `Catalogue<'static>` is `Send + Sync`, and a
  corpus opened again reads nothing; the corpus's own index is freed with
  it. Videos and sounds stay on disk, read as they play
  ([formats/README.md](../formats/README.md#what-openregent-holds-in-memory)). The GPL `Program` is leaked the same way, once per image list, as
  `Vm` borrows it. A game's own state is owned: `Game` holds its `Vm` and
  a `sim::host::Held` world. No `unsafe`, no self-referential crate, no `Arc` rework.
- **The camera owns magnification.** [`aim`](../../crates/regent_draw/src/stage.rs)
  writes `pan` into the world camera's translation and `1 / zoom` into its
  orthographic scale; everything else spawns at true position, scale one. So
  the map view rewrites no transforms per frame, the painter's cursor goes
  through one transform, and zoom-about-pointer is four lines.
- **Marker component, not `With<Camera2d>`.** Two cameras exist (world,
  egui); Bevy skips a system whose `Single` is ambiguous, with no warning.
  `Without<EguiContext>` is ambiguous before `bevy_egui` inserts it. Tested
  windowless: both cameras, one system; only the marked camera follows the pan.
- **The world camera is 3D, its picture the 2D one's.** A `Camera3d`
  (`stage::eye`): orthographic with 2D's depth range, no MSAA,
  tonemapping, deband dither or HDR; so is the tracking window's. The world
  frame is the art's metric frame turned by the art camera (yaw 45°,
  elevation 35°, **Inferred**), so the camera is unturned
  (`regent_draw::frame`, `frame::ART`): x, y the screen's, z toward the
  viewer. A sprite's quad keeps its 2D translation: facing the camera at
  its anchor, slid along the view to its draw depth. Ground vertices sit
  at their metric points (`frame::ground_z`) and draw at their rung
  (`card.wesl`); culled by their screen footprint (`redraw::grounded`).
  Order and pixels are 2D's: no pixel differed against a 2D build
  over the cases compared. Bevy 3D keeps a transparent item's mesh
  centre from its queue, as 2D kept its z: `sort::rekey` moves it each
  frame. Hit point bars are a sprite and a 3D quad at once: the 3D world
  camera draws the quad, the 2D shown camera the sprite, its key the
  queue's z, moved each frame too (`sort::rekey_2d`), so a frame never
  draws over its fill. The shown, EASU and egui cameras stay 2D: they
  draw quads and panels, not the world.
  A turned camera would mix a 512-cell map's positions (8,000 units)
  into every view depth, which f32 holds to about 0.001.
  Cost: with Bevy 0.20's indirect draws a frame costs far more than 2D's,
  as each sprite, its own material, is its own batch, and GPU culling
  pays buffer writes, bind groups and compute passes per batch. The 3D
  cameras draw without them (`NoIndirectDrawing`), and light clusters
  are worked out on the CPU (`stage::clusters`): about 1.2 to 1.3 times
  2D. Bevy's CPU instance builder would match 2D, but draws no binned
  (opaque, cutout) mesh unless `GpuPreprocessingSupport` is forced to
  `None` each frame (**Inferred**: a Bevy 0.20 bug).
- **The models' shadows read Bevy's map in the world's own materials.**
  Bodies and ground (`body.wesl`, `plain.wesl`, `sheet.wesl`) sample the
  sun's cascade (`shaders/lit.wesl`) and write coverage into alpha as
  bands do. The ground uses its metric point; a body a point upright over
  its footing (its `MeshTag`: x along the card, its height on the screen
  over the footing as art height, `frame::point`). A casting building's
  bodies take the point on its model instead, and none where the model
  shades itself (`sun::relief`): a grid a model in sprite pixels about the
  anchor, worked out on the CPU (orthographic view and sun, rigid model:
  one grid for every such building, both views, again only as the sun
  turns), each texel its depth over the upright point, or "shaded", and
  a lift toward the sun past its own faces within the filter's reach (4
  units, plus a sized sun's search reach), so its own model darkens none of it: another building's
  shadow, no acne, no fringe along its painted shade. Pixels off the
  model stand upright, tested alike. The bodies' tag names the building's
  slot in a shared texture (anchor, flat ground, layer). Under
  `[enhancements] self_shadows` (`sun::Own::Cast`) a texel on the model
  is never shaded and its lift only clears its own face: a
  slope-scaled bias, 0.5 + 4 × tan θ units (θ the sun off the face's
  normal, tan capped at 4), none on faces turned from the sun; moving
  along the light keeps the map texel, so other shadows stay put. Texels
  off the model stay as by default: their upright point lies behind the
  model and would speckle with its shadow. Same shader, a different
  bake. The slope bias keeps the 4 units under a sized sun: sized to the
  search, it lifts walls past their own eave's shade; the face's own
  depths found in the search lie near, so they ask for the least filter.
  Every body binds
  both textures, 3.5 MB and 16 KB, empty while nothing casts. Bevy fits a cascade to
  the camera's frustum, whose depth range is the draw order's, so
  `sun::fitted` replaces each lit view's after Bevy builds it (the world
  camera's and the tracking window's, both on `LIT_LAYER`): the
  light-space bounds of the box drawn within the view, lifted between the
  ground's least lift and its most plus a body's reach, whole units,
  centred on a texel, so a pan keeps the texels on the ground. At zoom 1,
  1920×1200 on hills: 0.76 × 0.61 world units a texel; the tracking
  window's (380×252): 0.26 × 0.19. Bevy 0.20 sizes the map's array by
  the most cascades one view has, not their sum (`bevy_pbr`
  `prepare_lights`): each view clears and draws the same layer again
  before its main pass, so the window costs a second map draw, no
  memory. Both shadow maps are 1² unless a model casts (`stage::IDLE_SHADOW_MAP`): 89 MB less than
  a 4096² map and Bevy's 1024² cube kept always.
- **A sized sun softens the models' shadows with distance**
  (`[enhancements] sun_size`, degrees, `sun::cast::Light::size`).
  Bevy 0.20's PCSS (`experimental_pbr_pcss`) does not fit: its penumbra,
  (z_blocker − z)·size/z on map depth, is a perspective formula, which
  on an orthographic fitted box grows toward its far plane and with
  its depth span; its filter is 8 taps with per-pixel rotated noise,
  meant for TAA. The feature is on only for the light's
  `soft_shadow_size` (carrying the tangent of half the size) and the
  map's linear sampler. `lit.wesl` samples itself: 16 gathers (64
  texels) within `ALONG` (400 units) × that tangent find the blockers;
  none, lit; all, covered; else the filter radius is the world gap along
  the light (map depth times the cascade's depth span) times the
  tangent, at least 1.5 texels, at most the search's; a grid of 2×2
  compares, tent weighted, at most 1.5 texels apart up to 12 a side:
  no noise, no visible banding. Size 0 uses Bevy's Gaussian filter.
  The relief's max filter is linear
  time (`relief::slide`) as the wider reach widens it.
  Cost (`tools/fps.sh`): none for painted shadows; with `--cast models`
  a little more than Bevy's filter.
- **Screen↔world are a pair on `View`**, both from the camera's `pan` and
  `zoom`. Windowless tests: they invert; zoom about a point holds it still at
  three positions and three factors; zoom reaches the camera as the scale's
  reciprocal; a drag holds its grab point at four magnifications either side of
  1 (at 1, adding screen pixels to a world pan is wrong but invisible).
- **Labels are egui's.** Drawing systems give a world position; the panel pass
  projects and paints it at a real point size, dark then light. Countering zoom
  on a `Text2d` blurs or re-rasterises.
- **Panel widths are arithmetic**: characters × glyph width from the font.
  `WIDEST_ROW` = 74 columns (detail tables, art layer cell-count line);
  `MAP_COLUMNS` = 37 for the map panel. One panel id per subject: egui stores
  width per id.
- **A widget truncates only to the width offered.** `selectable_label` extends
  instead of wrapping, and the scroll area and panel follow. `TextWrapMode::Truncate`
  alone does nothing: the row must be two columns, one fixed, one taking the
  rest, with the wrap mode set inside the scroll area so prose above still wraps.
- A wrong shader and a wrong decode look alike; a stock `Image` is the plain-RGBA
  reference for the indexed material.
- Exported images are game assets; keep them out of the repository.
- **Rust readers, not Kaitai.** Rust is the reimplementation language, so
  no spec-to-code gap. Kaitai cannot dispatch a payload type on the owning
  chunk's tag, as `.cam` needs, nor express GPL bytecode; its Rust target
  is immature; zero-copy matters for the 92 MB `maindata.cam`. Lost: a
  language-neutral spec, which the markdown covers.
- **Save compatibility is not a goal.** openregent writes and loads its
  own saves only ([saves.md](saves.md)); the `.gmp` reader stays as a
  format reference.
- **Bevy for anything that draws** ([game.md](game.md)).
- **The map generator reproduces HD only**: it is held to Windows-made maps.
- **The VM runs `.bcd`, with no second IR.** An operator's result depends
  on the type of the slot it writes to, and the compiler picks those
  types, so an IR without them computes other numbers. `.bcd` is also what
  the engine runs, and it runs images shipped without source. Records are
  decoded once into an execution form: operands resolved, jumps as
  indices, source lines kept. An IR, if needed, goes inside the compiler,
  between the AST and `emit`, where byte identity proves it.
