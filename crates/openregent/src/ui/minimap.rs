//! The minimap, the bar's top row: the library's picture
//! ([`regent_core::view::panel::minimap`]) as an egui texture, one texel a logical
//! point, and its clicks.
//!
//! ```text
//!  ground  Cache: drawn whole when a placement relays the map, a cell
//!          again as the local player explores it
//!  frame   minimap::draw: reach circles while a reach spell is aimed,
//!          units, marks; uploaded when changed, in place with a GPU
//!          ([`Texture`]), nearest-neighbour
//!  view    the main view's frame over it, egui rectangles a texel wide
//!  input   left press or drag: the main view centred on the ground there
//!          left double click: an explore flag at the panel's reward
//!          right press or drag: the tracking window there, following
//!          nothing
//! ```

use bevy::asset::RenderAssetUsages;
use bevy::ecs::system::SystemParam;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy_egui::egui;
use bevy_egui::egui::emath::GuiRounding;
use bevy_egui::{EguiTextureHandle, EguiUserTextures};

use regent_core::base::fixed::Fixed;
use regent_core::sim::game::orders::explore_flag;
use regent_core::sim::movement;
use regent_core::sim::units::CAMERA_COS;
use regent_core::sim::units::PER_CELL;
use regent_core::sim::world::{World, offer::EXPLORE_FLAG};
use regent_core::view::ScreenRect;
use regent_core::view::panel::minimap::{
    self, Art, Cache, Eyes, Filter, Looks, MAIN_FRAME, MINI_FRAME, Order, Picture, Press, Scale,
    Scene,
};
use regent_draw::data::Corpus;
use regent_draw::map::Maps;
use regent_draw::stage::View;
use regent_draw::terrain;

use crate::game::seen::Frame;
use crate::input::orders::{Mode, Orders};
use crate::input::{self, Awaited, Clicks};
use crate::map::reach::{self, Placing};

/// The local player `player`, as the minimap's eyes and its lit units.
fn local(player: u32) -> u8 {
    u8::try_from(player).unwrap_or_default()
}

/// The minimap's state between frames.
#[derive(Resource, Default)]
pub struct Minimap {
    ground: Option<Cache>,
    /// `Board::relaid` when the ground was drawn whole.
    relaid: Option<u32>,
    /// Whose explored ground it was drawn with.
    eyes: Option<Eyes>,
    looks: Option<Looks>,
    /// The picture uploaded, and its texture.
    drawn: Option<Picture>,
    texture: Option<Texture>,
    /// An image and its egui id lent ahead ([`lend`]), for the texture.
    lent: Option<(Handle<Image>, egui::TextureId)>,
    /// What the drawn picture shows ([`Shows`]).
    shows: Option<Shows>,
    /// Its own double clicks: a mode does not stop one here, as the
    /// engine's `Mini` row takes no mode.
    clicks: Clicks,
    /// Where it was drawn, logical points.
    rect: Option<Rect>,
}

impl crate::draw::shelf::Shelved for Minimap {
    fn kept(self) -> Self {
        Minimap {
            looks: self.looks,
            ..Minimap::default()
        }
    }
}

impl Parts<'_> {
    /// The picture last drawn: what the player saw.
    pub fn picture(&self) -> Option<&Picture> {
        self.state.drawn.as_ref()
    }
}

impl Minimap {
    /// The ground and picture forgotten, to be drawn whole again from
    /// another world (a LAN resync's).
    pub fn forget(&mut self) {
        self.ground = None;
        self.relaid = None;
        self.shows = None;
    }

    /// Whether `at`, logical points, is on the minimap.
    pub fn covers(&self, at: Vec2) -> bool {
        self.rect.is_some_and(|r| r.contains(at))
    }
}

/// The minimap's texture. egui's own `set` makes a new texture each time
/// (`bevy_egui` adds a new image), and the picture changes each game
/// frame; with a GPU an image of ours is written in place instead.
enum Texture {
    /// Ours, lent to egui; the size it was made at.
    Image {
        handle: Handle<Image>,
        id: egui::TextureId,
        size: [usize; 2],
    },
    /// egui's: without a GPU (tests, headless).
    Egui(egui::TextureHandle),
}

impl Texture {
    fn id(&self) -> egui::TextureId {
        match self {
            Texture::Image { id, .. } => *id,
            Texture::Egui(texture) => texture.id(),
        }
    }

    #[allow(clippy::cast_precision_loss, reason = "a minimap's pixels")]
    fn size(&self) -> egui::Vec2 {
        match self {
            Texture::Image { size, .. } => egui::vec2(size[0] as f32, size[1] as f32),
            Texture::Egui(texture) => texture.size_vec2(),
        }
    }
}

/// What a minimap picture shows: the game frame, the selection, the
/// reach circles, the main view's frame and the scale. It is drawn
/// again only once one changes, not each drawn frame.
type Shows = (
    u32,
    Option<u32>,
    Vec<((i32, i32), i32)>,
    Option<ScreenRect>,
    Scale,
);

/// What the minimap reads and writes besides the bar's.
#[derive(SystemParam)]
pub struct Parts<'w> {
    state: ResMut<'w, Minimap>,
    maps: Res<'w, Maps>,
    ground: Res<'w, terrain::TileArt>,
    awaited: ResMut<'w, Awaited>,
    time: Res<'w, Time<Real>>,
    images: ResMut<'w, Assets<Image>>,
}

/// What the game around the minimap gives it.
pub struct Game<'a> {
    pub corpus: &'a Corpus,
    pub world: &'a World,
    pub frame: &'a Frame,
    /// The highest agent shown: a flag made since is the one a double
    /// click sent.
    pub highest: Option<u32>,
    pub selected: Option<u32>,
    /// Whether the reach shows while a wizards' guild or tower is placed.
    pub placing: Placing,
}

/// The minimap from `at`, the original's place in its panel
/// ([`minimap::RECT`]) on the window, `texel` points a texel; the
/// tracking window's frame over the main view's, `mini` its middle and
/// texels. Its clicks centre `view` or send a flag through `orders`; a
/// right press is where the tracking window goes, 32nds.
pub fn show(
    ui: &mut egui::Ui,
    (at, texel): (egui::Pos2, f32),
    parts: &mut Parts,
    (game, mini): (&Game, Option<(Vec2, Vec2)>),
    (orders, view): (&mut Orders, &mut View),
) -> Option<(i32, i32)> {
    let mut stop = None;

    if let Some((picture, scale, frame)) = compose(parts, game, orders, view) {
        let (id, size) = upload(ui.ctx(), parts, picture);
        let state = &mut *parts.state;
        let rect = egui::Rect::from_min_size(at, size * texel);
        let response = ui.interact(rect, ui.id().with("minimap"), egui::Sense::click_and_drag());
        let whole = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));

        ui.painter().image(id, rect, whole, egui::Color32::WHITE);

        let mini = mini
            .map(|(middle, texels)| mini_frame((&parts.maps, game.world, scale), middle, texels));

        // The main view's, then mini view 1's, as view restore
        // registers them.
        for (frame, colour) in [(frame, MAIN_FRAME), (mini, MINI_FRAME)] {
            if let Some(frame) = frame {
                outline(
                    &ui.painter().with_clip_rect(rect),
                    frame,
                    (at, texel),
                    colour,
                );
            }
        }

        state.rect = Some(Rect::new(rect.min.x, rect.min.y, rect.max.x, rect.max.y));

        // A press or drag centres the main view; a double click flags.
        let seconds = parts.time.elapsed_secs_f64();
        let world = game.world;

        match pressed(ui, (&response, texel), &mut state.clicks, seconds, scale) {
            Some(Order::CentreMain(at)) => look_at(&parts.maps, view, at),
            Some(Order::Explore(at)) => {
                let flag = explore_flag(world, game.frame.local, at, orders.rewards.explore);

                parts.awaited.expect(EXPLORE_FLAG, game.highest);
                orders.outbox.push(flag);
            }
            Some(Order::CentreMini(at)) => stop = Some(at),
            None => {}
        }
    }

    stop
}

/// The main view centred on `at`, 32nds: a panel's zoom button.
pub fn centre(parts: &Parts, view: &mut View, at: (i32, i32)) {
    look_at(&parts.maps, view, at);
}

/// The main view centred on `at`, 32nds, on `maps`' ground, kept on
/// the map: what is drawn after it in the panels' pass reads the view.
pub fn look_at(maps: &Maps, view: &mut View, at: (i32, i32)) {
    view.centre(maps.standing_at(crate::ui::tracking::cells(at)));
}

/// This frame's picture: the ground brought up to date, then reach
/// circles while a reach spell is aimed (or, an enhancement, a wizards'
/// guild or tower placed) and the units; its scale and the
/// main view's frame, minimap pixels. `None` without a map.
fn compose(
    parts: &mut Parts,
    game: &Game,
    orders: &Orders,
    view: &View,
) -> Option<(Picture, Scale, Option<regent_core::view::ScreenRect>)> {
    let world = game.world;
    // The ground changes only with the game: brought up to it once a
    // game frame.
    let scale = match parts.state.shows {
        Some((at, .., scale)) if at == game.frame.frames && parts.state.ground.is_some() => scale,
        _ => refresh(parts, game.corpus, world, eyes(game.frame))?,
    };
    let frame = main_frame(view, &parts.maps, world, scale);
    // The original's circles for an aimed reach spell; while placing,
    // the enhancement's.
    let program = game.frame.program;
    let reach = match orders.mode {
        Mode::Spell(spell) => world.aimed_reach(program, game.frame.local, spell.row.spell),
        _ => reach::placing((world, game.frame.local), program, orders, game.placing),
    };
    let shows = (game.frame.frames, game.selected, reach, frame, scale);

    let state = &mut *parts.state;
    let catalogue = &game.corpus.catalogue;
    let looks = state.looks.get_or_insert_with(|| Looks::new(catalogue));
    let ground = state.ground.as_ref()?;
    let kept = state
        .drawn
        .as_ref()
        .filter(|_| state.shows.as_ref() == Some(&shows))
        .cloned();

    let picture = kept.unwrap_or_else(|| {
        let reach = &shows.2;
        let scene = Scene {
            local: local(game.frame.local),
            eyes: eyes(game.frame),
            selected: game.selected,
            filter: Filter::DEFAULT,
            reach,
            frames: &[],
            shown: &game.frame.taken.steps.now,
        };

        minimap::draw(ground.picture(), world, catalogue, looks, &scene, scale)
    });

    state.shows = Some(shows);
    Some((picture, scale, frame))
}

/// The main view's `frame`, minimap pixels, as outline rect draws
/// it (`minimap::outline`): four sides a texel wide, the minimap's top
/// left at `at`, `texel` points a pixel, on the window's pixels. Drawn
/// last, as the library's frames are.
fn outline(
    painter: &egui::Painter,
    frame: regent_core::view::ScreenRect,
    (at, texel): (egui::Pos2, f32),
    rgb: regent_core::data::payload::Rgb,
) {
    let [r, g, b] = [rgb.r, rgb.g, rgb.b];
    let colour = egui::Color32::from_rgb(r, g, b);

    #[allow(clippy::cast_precision_loss, reason = "minimap pixels")]
    for [x0, y0, x1, y1] in minimap::outline(frame) {
        let side = egui::Rect::from_min_max(
            at + egui::vec2(x0 as f32, y0 as f32) * texel,
            at + egui::vec2(x1 as f32, y1 as f32) * texel,
        );

        painter.rect_filled(
            side.round_to_pixels(painter.pixels_per_point()),
            0.0,
            colour,
        );
    }
}

/// Whose sight the minimap shows: the local player's, or every vertex
/// and unit for a spectator.
fn eyes(frame: &Frame) -> Eyes {
    if frame.seat == crate::game::sim::Seat::Spectates {
        Eyes::ALL
    } else {
        Eyes::of(local(frame.local))
    }
}

/// The ground brought up to the world: drawn whole at first, after a
/// placement relays the map, and as `eyes` change, else the cells
/// explored since. The map is read only then. The scale, `None` without
/// a map.
fn refresh(parts: &mut Parts, corpus: &Corpus, world: &World, eyes: Eyes) -> Option<Scale> {
    let scale = Scale::fitted(
        minimap::RECT[2] - minimap::RECT[0],
        world.explored().cells(),
    );
    let explored = world.explored();
    let state = &mut *parts.state;
    let whole = state.relaid != Some(world.board().relaid()) || state.eyes != Some(eyes);
    let cells = state
        .ground
        .as_ref()
        .filter(|_| !whole)
        .map(|cache| cache.changed(explored));
    let work = cells.as_ref().is_none_or(|c| !c.is_empty());

    if work {
        let terrain = parts.maps.current()?.terrain()?;
        let art = Art {
            tileset: parts.ground.tileset.as_ref()?,
            catalogue: &corpus.catalogue,
        };

        if let (Some(cache), Some(cells)) = (state.ground.as_mut(), cells) {
            cache.reveal(&terrain, art, explored, &cells);
        } else {
            state.ground = Some(Cache::new(&terrain, art, scale, eyes, explored));
            state.relaid = Some(world.board().relaid());
            state.eyes = Some(eyes);
        }
    }

    Some(scale)
}

/// `picture` in the minimap's texture, uploaded only when changed: its
/// id and size.
fn upload(
    ctx: &egui::Context,
    parts: &mut Parts,
    picture: Picture,
) -> (egui::TextureId, egui::Vec2) {
    let size = [picture.width, picture.height];
    let image = || egui::ColorImage::from_rgba_unmultiplied(size, &picture.rgba());
    let state = &mut *parts.state;
    let changed = state.drawn.as_ref() != Some(&picture);

    let texture = match (state.texture.take(), state.lent.take()) {
        (Some(Texture::Image { handle, id, .. }), _) => {
            if changed {
                let _ = parts.images.insert(&handle, gpu_image(&image()));
            }

            Texture::Image { handle, id, size }
        }
        (Some(Texture::Egui(mut texture)), _) => {
            if changed {
                texture.set(image(), egui::TextureOptions::NEAREST);
            }

            Texture::Egui(texture)
        }
        (None, Some((handle, id))) => {
            let _ = parts.images.insert(&handle, gpu_image(&image()));

            Texture::Image { handle, id, size }
        }
        (None, None) => {
            Texture::Egui(ctx.load_texture("minimap", image(), egui::TextureOptions::NEAREST))
        }
    };
    let shown = (texture.id(), texture.size());

    state.drawn = Some(picture);
    state.texture = Some(texture);
    shown
}

/// An image and its egui id for the minimap's first texture, where egui
/// takes images of ours (a GPU): before the bar's pass, which cannot
/// reach egui's user textures.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
fn lend(
    state: Option<ResMut<Minimap>>,
    images: Res<Assets<Image>>,
    textures: Option<ResMut<EguiUserTextures>>,
) {
    if let (Some(mut state), Some(mut textures)) = (state, textures)
        && state.texture.is_none()
        && state.lent.is_none()
    {
        let handle = images.reserve_handle();
        // Weak: the state holds the image, and egui frees the id when it
        // goes.
        let id = textures.add_image(EguiTextureHandle::Weak(handle.id()));

        state.lent = Some((handle, id));
    }
}

/// `image` as a texture on the GPU alone, as egui's own would be: its
/// bytes (`bevy_egui`'s `color_image_as_bevy_image`), sampled nearest.
fn gpu_image(image: &egui::ColorImage) -> Image {
    let bytes = image
        .pixels
        .iter()
        .flat_map(egui::Color32::to_array)
        .collect();
    let [width, height] = image.size.map(|n| u32::try_from(n).unwrap_or(u32::MAX));
    let mut image = Image::new(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        bytes,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );

    image.sampler = ImageSampler::nearest();
    image
}

/// What the pointer asks of the minimap this frame:
/// a left double click an explore flag; else a left press, or the mouse
/// moving with it held, the main view centred. Only inside its rect.
fn pressed(
    ui: &egui::Ui,
    (response, texel): (&egui::Response, f32),
    clicks: &mut Clicks,
    seconds: f64,
    scale: Scale,
) -> Option<Order> {
    let (at, pressed, held, right) = ui.input(|i| {
        (
            i.pointer.interact_pos(),
            i.pointer.primary_pressed(),
            i.pointer.primary_down(),
            i.pointer.secondary_down(),
        )
    });
    let at = at.filter(|p| response.rect.contains(*p))?;
    let twice = (pressed && response.hovered())
        .then(|| clicks.press(seconds * 1000.0, Vec2::new(at.x, at.y)))
        == Some(input::Press::Double);
    let dragged = response.is_pointer_button_down_on();
    let press = match (twice, right && dragged) {
        (true, _) => Press::LeftTwice,
        (false, true) => Press::Right,
        (false, false) => Press::Left,
    };

    #[allow(clippy::cast_possible_truncation, reason = "a pixel of the minimap")]
    let pixel = |p: egui::Pos2| {
        let from = (p - response.rect.min) / texel;

        (from.x.floor() as i32, from.y.floor() as i32)
    };

    (twice || ((held || right) && dragged)).then(|| minimap::click(press, pixel(at), scale))
}

/// The main view's frame in minimap pixels: its rect on flat ground in
/// 32nds, lowered by the ground's lift,
/// to pixels.
/// The view's top left counts in its own screen pixels from the map's
/// corner, as the engine's scroll does.
fn main_frame(
    view: &View,
    maps: &Maps,
    world: &World,
    scale: Scale,
) -> Option<regent_core::view::ScreenRect> {
    let rect = view.rect?;

    Some(frame_at(
        (maps, world, scale),
        (view.pan, rect.size(), view.zoom),
    ))
}

/// The tracking window's frame in minimap pixels, as [`main_frame`]'s:
/// its picture, `texels` art pixels, about `middle`, at its zoom.
fn mini_frame(
    (maps, world, scale): (&Maps, &World, Scale),
    middle: Vec2,
    texels: Vec2,
) -> regent_core::view::ScreenRect {
    let zoom = regent_core::view::panel::tracking::ZOOM;

    frame_at((maps, world, scale), (middle, texels * zoom, zoom))
}

/// A view's frame in minimap pixels: `middle` its middle, world units,
/// `room` its own pixels at `zoom`.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "screen pixels, a few thousand"
)]
fn frame_at(
    (maps, world, scale): (&Maps, &World, Scale),
    (middle, room, zoom): (Vec2, Vec2, f32),
) -> regent_core::view::ScreenRect {
    let half = room / (2.0 * zoom);
    let origin = maps.aloft_at(Vec2::ZERO, 0.0);
    let across = maps.aloft_at(Vec2::X, 0.0).x - origin.x;
    let down = origin.y - maps.aloft_at(Vec2::Y, 0.0).y;

    // World y runs up, the screen's down.
    let corner = Vec2::new(middle.x - half.x - origin.x, origin.y - (middle.y + half.y)) * zoom;
    let scroll = (corner.x.round() as i32, corner.y.round() as i32);
    let size = (room.x.round() as i32, room.y.round() as i32);
    let flat = minimap::view_rect(scroll, size, zoom * across / PER_CELL as f32, zoom * down);

    let height = world.board().heights().map_or(Fixed::ZERO, |h| {
        movement::on_ground(h, minimap::lift_point(flat))[2]
    });
    let map = world.explored().cells() * PER_CELL;
    let lifted = minimap::lift(flat, height, CAMERA_COS, map);

    minimap::viewport(lifted, scale)
}

/// The minimap's state, a game's.
pub fn plugin(app: &mut bevy::prelude::App) {
    use crate::game::scope::Scoped as _;

    app.per_game::<Minimap>().add_systems(PreUpdate, lend);
}

/// The minimap's ground as drawn, and as drawn afresh from the drawn
/// map and `world`'s explored ground: a test's.
#[cfg(test)]
pub fn grounds(app: &bevy::prelude::World, world: &World) -> (Option<Picture>, Option<Picture>) {
    let state = app.resource::<Minimap>();
    let corpus = app.resource::<Corpus>();
    let player = app.resource::<Frame>().local;
    let fresh = || {
        let terrain = app.resource::<Maps>().current()?.terrain()?;
        let art = Art {
            tileset: app.resource::<terrain::TileArt>().tileset.as_ref()?,
            catalogue: &corpus.catalogue,
        };
        let scale = Scale::fitted(
            minimap::RECT[2] - minimap::RECT[0],
            world.explored().cells(),
        );
        let eyes = Eyes::of(local(player));

        Some(
            Cache::new(&terrain, art, scale, eyes, world.explored())
                .picture()
                .clone(),
        )
    };

    (state.ground.as_ref().map(|c| c.picture().clone()), fresh())
}

#[cfg(test)]
mod tests {
    use super::*;
    use regent_core::view::panel::minimap::WHITE;
    use regent_core::view::raster::colour::narrow;

    /// A new game's minimap after its first frames: the crown's palace
    /// its set 300 cross in the crown's blue at its point, the far corner
    /// unexplored black, the main view's frame white on top.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_new_games_minimap_shows_the_palace_under_the_frame() {
        let (begun, catalogue) = crate::game::begin::test_game(&[]);
        let mut sim = begun.sim;

        for _ in 0..2 {
            sim.step();
        }

        let world = sim.world();
        let dressing = regent_core::sim::mapgen::custom::Art::of(&catalogue);
        let source = regent_draw::map::MapSource::of_world(world, &|c, at| dressing.dress(c, at))
            .expect("a map");
        let terrain = source.terrain().expect("terrain");
        let tileset = regent_core::content::terrain::Terrain::find(&catalogue)
            .expect("a DTRN")
            .expect("it parses")
            .tileset
            .expect("its art");
        let art = Art {
            tileset: &tileset,
            catalogue: &catalogue,
        };
        let scale = Scale::fitted(176, world.explored().cells());
        let ground = Cache::new(
            &terrain,
            art,
            scale,
            Eyes::of(local(sim.local())),
            world.explored(),
        );
        let mut looks = Looks::new(&catalogue);
        let frame = [20, 30, 60, 60];
        let scene = Scene {
            local: local(sim.local()),
            eyes: Eyes::of(local(sim.local())),
            selected: None,
            filter: Filter::DEFAULT,
            reach: &[],
            frames: &[(frame, MAIN_FRAME)],
            shown: &world.shown(),
        };
        let drawn = minimap::draw(
            ground.picture(),
            world,
            &catalogue,
            &mut looks,
            &scene,
            scale,
        );

        let palace = world
            .shown()
            .into_iter()
            .find(|s| s.owner == sim.local() && s.code.0.starts_with(b"ABJ"))
            .expect("the local player's palace");
        let pixel = |picture: &Picture| {
            let (x, y) = (scale.pixel(palace.at.0), scale.pixel(palace.at.1));

            picture.at(x.cast_unsigned() as usize, y.cast_unsigned() as usize)
        };
        let blue = pixel(&drawn).expect("on the minimap");

        assert_ne!(Some(blue), pixel(ground.picture()));
        assert!(blue >> 11 == 0 && blue & 0x1f != 0, "{blue:#06x}");
        assert_eq!(drawn.at(20, 45), Some(narrow(looks.colour(WHITE))));
        assert_eq!(drawn.at(drawn.width - 1, drawn.height - 1), Some(0));
    }
}
