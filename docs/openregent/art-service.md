# The art service: one lookup for every interface picture

`regent_draw::art` loads, scales, caches and replaces every picture
openregent and `regent_viewer` draw: panels, menus, the quest map and the
game view's sprites. No other code fetches a picture, so one rule (blank
frames, pixel snapping, stand-ins) holds everywhere. Two tests hold that:
`only_the_art_service_loads_interface_art` and
`only_the_art_service_makes_world_sprites` (`regent_draw/tests/art_service.rs`).
Save thumbnails and the minimap are their own textures.

## The design

```text
 draw code ──Want { picture, size, look }──> Art ──> Ready(texture)
                                              │       Pending(stand-in)
                    ┌─────────────────────────┼──────────────────────┐
                    ▼                         ▼                      ▼
             original archives        derived variants          overrides
             (catalogue, frames;      (xBRZ 2/3/4×, shrink,     (user folder)
              blank frames none)       tones, lit look, tiles)
                    └──────────── memory cache ── disk cache ────────┘
```

- **Key:** a typed `Want`: the picture (record, set, slot, frame, or a
  tile, or a derived source such as a map level or portrait), the target
  size in physical pixels, and the look (tone, glow, clip). `Want::of`
  names the source: an `art::ArtSource`, a hash of the archive paths, as
  ids repeat across archives (base and expansion `AP29` differ).
- **Answer:** `Ready(texture)` or `Pending(stand-in)`. The stand-in is
  the held size nearest by area, else the plain picture. A picture cached
  on disk with no stand-in is made at once (`disk::at_once`); otherwise in
  the task pool.
- **Policy in one place:** factor choice (`ui_upscale`), shrink, pixel
  snapping of a placed rect (`Snap::Edges` for menus, `Snap::Corner` for
  panels), the blank-frame rule (`FrameSource`), budgets, invalidation
  (pixel hash plus method version), dropping unused sizes.
- **Sources:** one `art::Art`, a Bevy resource. `Art::take` and `put`
  move one source's `art::Entries` to and from a shelf, so dropping a
  dataset's game art keeps the menus'; the menus' sources are pinned. A
  source's sized textures age only while it is drawn. Pictures are
  decoded again from their catalogue as asked; nothing else holds pixels.
- **Two front ends** over one shared layer: `art::ui` gives egui texture
  ids and window rectangles (`Art::get` takes a `Place`: a page's scale
  and origin); `art::world::Sprites` makes and keeps, per source, each
  tile's materials, each piece's layers, palette swaps (tints, a
  particle's glow, the fog's darkening) and the cursor's images. The
  map's scatter draws through it too.
- **Quest map levels** (`art/levels.rs`): levels ½×–4×, their tiles and
  lit patches are derived sources. A level is one disk file of its tiles
  and patches; the table and the map's own level are made from the
  menus' start, then 2× ahead; up to 2× kept for the session, 4× dropped
  off the map.
- **Disk cache:** LZ4 blocks of egui's pixels in parts of 1024 px behind
  a header (`disk::Header`), read in parallel. openregent gives the
  directory (`Art::cached`). Uploads are capped at 32 MB a frame. A warm
  2× quest map level loads in 12–35 ms.
- **Overrides:** `$XDG_DATA_HOME/openregent/art/`, files named as
  `regent_cli ui-dump` writes them (`regent_core::view::picture_file`),
  replace pictures (`regent_draw::art::over`). Quest map levels take no
  override: downloadable quests paste their buildings at the original
  map's pixels. See [game.md](game.md).
- **Stats:** `art::Stats`, from `Art::stats`. Log lines go through
  `tracing`; openregent sets the subscriber.

The service takes plain numbers, not openregent's types. World sprites
are not upscaled ([enhancements.md](enhancements.md)).
