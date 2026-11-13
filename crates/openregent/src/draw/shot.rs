//! `--screenshot`: one frame of the window to a PNG, then exit, for
//! comparing the drawing against the original's captures.
//!
//! ```text
//!  window at --size pixels, scale --scale (1), fog lifted, paused, muted
//!  opening view framed, the loading page gone ──> --frames drawn frames ──> Screenshot
//!  captured ──> PNG at --screenshot ──> exit: 0, or 1 if the size is off
//! ```
//!
//! The clock steps only on the counted frames, from the window at its
//! size; the capture waits for the art, the pipelines and the materials,
//! then for two captures alike; still waiting after `BUSY` frames, or a
//! `--select` matching nothing, it saves nothing and exits 1. Waits take
//! wall-clock time, so frozen they leave every run the same frame,
//! `--running` too.
//!
//! With `--menu` the page asked for is captured, `--frames` frames in,
//! counted and waited for alike.
//!
//! Nothing is written under the data or config directories: no log, no
//! recording, the settings the defaults. The clock steps 1/60 s a drawn
//! frame, not the wall's: the chat box's lines scroll alike every run.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use bevy::pbr::{PreparedMaterial, RenderMaterialInstances, RenderMeshInstances};
use bevy::prelude::*;
use bevy::render::erased_render_asset::ErasedRenderAssets;
use bevy::render::mesh::RenderMesh;
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_resource::PipelineCache;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use bevy::render::{Render, RenderApp, RenderSystems};
use bevy::time::TimeUpdateStrategy;
use bevy::window::{CursorMoved, PrimaryWindow, WindowEvent, WindowResolution};
use regent_draw::stage::View;

use crate::draw::Framed;
use crate::files::config::Settings;
use crate::game::seen::Watch;
use crate::game::sim::Sim;
use crate::map::fog::{Fog, Sight};
use crate::map::mirror::Pieces;
use crate::map::select::{Reading, Selected};
use crate::menu::{Screen, in_game};
use crate::panel::{Page, Panels};
use crate::ui::Paused;
use regent_core::data::fourcc::FourCc;
use regent_core::sim::game::Input;
use regent_core::sim::units::PER_CELL;
use regent_core::sim::world::cheat::{Cheat, CheatCode};
use regent_core::sim::world::{defeat::End, flag::MESSAGE_FLAG};
use regent_core::view::menu::end::{Curtain, Page as EndPage, Stage};

/// The captures' size: HD at 1920 × 1200 (`local/shots/`).
pub const SIZE: &str = "1920x1200";

/// Drawn frames after the opening view before the capture: time for the
/// shaders and art to arrive.
pub const FRAMES: u32 = 60;

/// The clock's step each drawn frame, a 60 Hz display's, in place of
/// the wall clock's: what the chat box scrolls by comes out alike in
/// every run.
const FRAME: Duration = Duration::from_micros(16_667);

/// Drawn frames the room must have kept its size before the capture:
/// the framing reads the room the last frame measured.
const SETTLED: u32 = 2;

/// Drawn frames to wait for the window's size before capturing anyway,
/// to fail on the size.
const WAIT: u32 = 300;

/// Counted frames `--look-from` holds the view before it moves to `--look`.
const FROM: u32 = 10;

/// Drawn frames to wait for upscaled pictures (`art::busy`), the
/// pipelines and the materials ([`drawable`]) before failing.
const BUSY: u32 = 3000;

/// Drawn frames in a row nothing must be pending before the capture: a
/// pipeline is asked for in the render world a frame or two after its
/// entity shows.
const READY: u32 = 3;

/// `--scale`'s default: one pixel a logical point, so the PNG is
/// `--size` on any display.
pub const SCALE: f32 = 1.0;

/// A window size in pixels, `<width>x<height>`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

/// How near the window's size must be to the asked one, logical points:
/// a scale's division is not exact.
const HALF_POINT: f32 = 0.5;

/// `--size`'s parser: `1920x1200`.
pub fn size(text: &str) -> Result<Size, String> {
    let bad = || format!("{text}: want <width>x<height>, e.g. {SIZE}");
    let (width, height) = text.split_once('x').ok_or_else(bad)?;

    match (width.parse(), height.parse()) {
        (Ok(width), Ok(height)) if width > 0 && height > 0 => Ok(Size { width, height }),
        _ => Err(bad()),
    }
}

/// Whether a run is played or captures a screenshot.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Capture {
    Played,
    Shot,
}

impl Capture {
    pub const fn of(shot: Option<&Shot>) -> Self {
        if shot.is_some() {
            Self::Shot
        } else {
            Self::Played
        }
    }
}

/// What to capture, and where.
#[derive(Resource, Clone, Debug)]
pub struct Shot {
    pub path: PathBuf,
    /// The window and PNG, in pixels.
    pub size: Size,
    /// Pixels a logical point: a display's scale, 1.8 at 180 %.
    pub scale: f32,
    /// Drawn frames left before the capture; `None` once asked for.
    pub frames: Option<u32>,
    /// Drawn frames since the view was first framed.
    pub waited: u32,
    /// Drawn frames in a row the window has been the asked size, the
    /// map's room framed in it.
    pub settled: u32,
    /// Drawn frames in a row with no art and no pipeline pending.
    pub ready: u32,
    /// Drawn frames counted: the clock stepped and the game ran.
    pub counted: u32,
    /// The piece to select: the Nth of a descriptor.
    pub select: Option<(FourCc, usize)>,
    /// The spell to arm, once offered.
    pub aim: Option<FourCc>,
    /// The building to place.
    pub build: Option<FourCc>,
    /// The page to open under it.
    pub page: Option<Page>,
    /// The view zoom to draw at, smooth zoom on.
    pub zoom: Option<f32>,
    /// The end's stage to draw, the game ended by [`end`].
    pub end: Option<Ending>,
    /// The cell to centre the view on.
    pub look: Option<(f32, f32)>,
    /// The cell centred first, for [`FROM`] counted frames, before [`Shot::look`]:
    /// what a pan leaves behind.
    pub from: Option<(f32, f32)>,
    /// Where the pointer rests, window points.
    pub pointer: Option<(f32, f32)>,
    /// The cell a minimap drag holds the view on, set where the minimap
    /// sets it: in the panels' pass.
    pub mini: Option<(f32, f32)>,
    /// Whether the game is held paused.
    pub hold: Hold,
    /// The path the view moves along, each drawn frame.
    pub camera: Option<Camera>,
    /// The menu open over the game.
    pub dialog: Option<Dialog>,
    /// The settings editor's tab, as it opens.
    pub settings_tab: Option<crate::menu::settings::Tab>,
    /// The game ms the trees' wind is held at; none, still.
    pub wind: Option<u32>,
    /// Whether the trees' shadows sway too.
    pub wind_shadow: regent_draw::wind::Shadow,
    /// The fog: lifted, as the captures' reveal cheat, or kept.
    pub sight: Sight,
}

/// `--dialog`: a menu over the game, as its button opens it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Dialog {
    /// OPTIONS' `AP36`.
    Options,
    /// The title scroll's `AP88`.
    Goals,
    /// `AP36`'s ADJUST LEVELS: the settings editor.
    Settings,
    /// `AP36`'s SAVE GAME: `AP49`, on the profile's saves.
    Save,
    /// `AP36`'s LOAD GAME: `AP27`.
    Load,
    /// `AP49`'s SAVE GAME on a name taken: its question.
    Overwrite,
}

impl Dialog {
    /// The menu open, as `options` holds it.
    const fn open(self) -> crate::ui::options::Dialog {
        match self {
            Dialog::Options => crate::ui::options::Dialog::Options,
            Dialog::Goals => crate::ui::options::Dialog::Goals,
            Dialog::Settings => crate::ui::options::Dialog::Settings,
            Dialog::Save | Dialog::Load | Dialog::Overwrite => crate::ui::options::Dialog::Saves,
        }
    }
}

/// `--camera`: the view moved along a fixed path, a step each drawn
/// frame once framed, for the perf runs' pans and zooms (`tools/perf/`).
///
/// ```text
///  pan     dragged STRIDE points a frame, a quarter turn each LEG frames:
///          right, down, left, up, a square back to the start
///  levels  the original's wheel: 2:1 out and 1:1 in turn, each HOLD frames
///  wheel   smooth zoom: a notch in each HOLD frames to the top, then out
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Camera {
    Pan,
    Levels,
    Wheel,
}

/// `--camera pan`'s drag a frame, logical points: 720 a second at 60 Hz.
const STRIDE: f32 = 12.0;

/// `--camera pan`'s frames a side of its square.
const LEG: u32 = 150;

/// `--camera levels` and `wheel`: frames between zoom steps.
const HOLD: u32 = 30;

/// `--running`: the game held paused for the capture, or left running.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hold {
    Paused,
    Running,
}

/// `--end`: the end's stage a capture shows (`local/shots/ui_victory*.png`,
/// `ui_defeat*.png`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Ending {
    /// The "Game Over" box.
    GameOver,
    Won,
    WonStatistics,
    Lost,
    LostStatistics,
}

impl Ending {
    const fn curtain(self) -> Curtain {
        let (end, stage) = match self {
            Ending::GameOver => (End::Won, Stage::Told),
            Ending::Won => (End::Won, Stage::Shown(EndPage::Verdict)),
            Ending::WonStatistics => (End::Won, Stage::Shown(EndPage::Statistics)),
            Ending::Lost => (End::Lost, Stage::Shown(EndPage::Verdict)),
            Ending::LostStatistics => (End::Lost, Stage::Shown(EndPage::Statistics)),
        };

        Curtain { end, stage }
    }
}

/// Frames to wait for the cheat's end.
const ENDING: u32 = 10;

/// `sim` won or lost by the cheat, as `ending` asks.
pub fn end(sim: &mut Sim, ending: Ending) {
    let cheat = match ending.curtain().end {
        End::Won => Cheat::Win,
        End::Lost => Cheat::Lose,
    };

    let arg = sim.local();

    sim.input(Input::Cheat(CheatCode { cheat, arg }));

    for _ in 0..ENDING {
        sim.step();
    }
}

/// The asked stage of the end, held once the game has ended.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
fn ending(shot: Res<Shot>, seen: Watch, mut shown: ResMut<crate::ui::end::EndScreen>) {
    if let Some(ending) = shot.end.filter(|_| seen.ended) {
        shown.hold(ending.curtain());
    }
}

/// `--select`'s `CODE` or `CODE:N`; a code may be an XML description's
/// `ID`, read as the engine reads it (`string_id`): The Wild's `gribea1`.
///
/// # Errors
///
/// An empty code, or a count not a number.
pub fn chosen(text: &str) -> Result<(FourCc, usize), String> {
    let (code, n) = text.split_once(':').unwrap_or((text, "0"));
    let code = (!code.is_empty())
        .then(|| FourCc::from_word(regent_core::data::fourcc::string_id(code).cast_unsigned()))
        .ok_or_else(|| format!("{text}: want a code, e.g. AVL1:0"))?;
    let n = n.parse().map_err(|_| format!("{text}: want CODE:N"))?;

    Ok((code, n))
}

/// `--aim`'s spell code.
///
/// # Errors
///
/// A code not four characters.
pub fn spell(text: &str) -> Result<FourCc, String> {
    FourCc::from_prefix(text.as_bytes())
        .ok()
        .filter(|_| text.len() == 4)
        .ok_or_else(|| format!("{text}: want a spell's code, e.g. Sp12"))
}

/// `--build`'s building code.
///
/// # Errors
///
/// Not four characters.
pub fn building(text: &str) -> Result<FourCc, String> {
    spell(text).map_err(|_| format!("{text}: want a building's four-character code"))
}

/// `--look`'s `X,Y`, in cells.
///
/// # Errors
///
/// Not two numbers.
pub fn cell(text: &str) -> Result<(f32, f32), String> {
    let bad = || format!("{text}: want X,Y in cells, e.g. 40,72");
    let (x, y) = text.split_once(',').ok_or_else(bad)?;

    match (x.trim().parse(), y.trim().parse()) {
        (Ok(x), Ok(y)) => Ok((x, y)),
        _ => Err(bad()),
    }
}

/// `--pointer`: the pointer at the asked window point each frame, so
/// what is under it draws hovered, the tooltip line too. One place every
/// frame: the pointer rests. The window keeps it too, so the map's pick
/// meets what is under it.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
fn point(
    shot: Res<Shot>,
    mut window: Single<(Entity, &mut Window), With<PrimaryWindow>>,
    mut events: MessageWriter<WindowEvent>,
) {
    if let Some((x, y)) = shot.pointer {
        let (entity, ref mut window) = *window;

        window.set_cursor_position(Some(Vec2::new(x, y)));
        events.write(WindowEvent::CursorMoved(CursorMoved {
            window: entity,
            position: Vec2::new(x, y),
            delta: None,
        }));
    }
}

/// The asked cell in the middle, after the opening view is framed;
/// `--look-from`'s for the first [`FROM`] frames, else `--mini`'s; with
/// `--camera`, only until settled.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
fn look(shot: Res<Shot>, maps: Res<regent_draw::map::Maps>, mut view: ResMut<View>) {
    if let Some((x, y)) = aim(&shot) {
        let pan = maps.standing_at(Vec2::new(x, y));

        if view.pan != pan {
            view.pan = pan;
        }
    }
}

/// The cell [`look`] centres.
fn aim(shot: &Shot) -> Option<(f32, f32)> {
    // `--mini` too: the opening view is framed again each frame. A
    // moving camera starts there once settled, then leaves it.
    shot.from
        .filter(|_| shot.counted < FROM)
        .or(shot.look)
        .or(shot.mini)
        .filter(|_| shot.camera.is_none() || shot.settled < SETTLED)
}

/// `--mini`: the view centred on the asked cell as a minimap drag
/// centres it, each frame after the panels.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
fn mini(shot: Res<Shot>, maps: Res<regent_draw::map::Maps>, mut view: ResMut<View>) {
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        reason = "a map's 32nds"
    )]
    let at = |cell: f32| (cell * PER_CELL as f32) as i32;

    if let Some((x, y)) = shot.mini {
        crate::ui::minimap::look_at(&maps, &mut view, (at(x), at(y)));
    }
}

/// Select the asked piece once the world shows it, and open its page;
/// `ARE1:N` reads the Nth message flag standing.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
fn choose(
    mut shot: ResMut<Shot>,
    pieces: Pieces,
    mut selected: ResMut<Selected>,
    mut panels: ResMut<Panels>,
    (seen, mut reading): (Watch, ResMut<Reading>),
    mut exit: MessageWriter<AppExit>,
) {
    let asked = shot.select.filter(|_| !pieces.is_empty());
    let flag = |n: usize| seen.world().messages().get(n).map(|m| m.handle);

    // The game is held, so the first frame's pieces are the last.
    if let Some((code, n)) = asked {
        shot.select = None;

        if let (true, Some(handle)) = (code == MESSAGE_FLAG, flag(n)) {
            reading.0 = Some(handle);
        } else {
            let shown: Vec<_> = pieces.iter().map(|s| (s.code, s.agent)).collect();

            match picked(&shown, (code, n)) {
                Ok(agent) => {
                    selected.0 = Some(agent);
                    panels.state.page_of = shot.page.map_or(panels.state.page_of, |_| Some(agent));
                    panels.state.page = shot.page.unwrap_or(panels.state.page);
                }
                // Nothing to show: no picture rather than the wrong one.
                Err(e) => {
                    error!("screenshot: {e}");
                    exit.write(AppExit::error());
                }
            }
        }
    }
}

/// The agent of the `n`th of `pieces` (code, agent) with `code`; else
/// the error naming it and the codes shown.
fn picked(pieces: &[(FourCc, u32)], (code, n): (FourCc, usize)) -> Result<u32, String> {
    let found = pieces.iter().filter(|(c, _)| *c == code).nth(n);

    found.map(|(_, agent)| *agent).ok_or_else(|| {
        let codes: std::collections::BTreeSet<String> =
            pieces.iter().map(|(c, _)| c.to_string()).collect();

        format!("--select: no {code}:{n}; shown: {codes:?}")
    })
}

/// Arm the asked spell once the world offers it, as its button does.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
fn arm(mut shot: ResMut<Shot>, seen: Watch, mut orders: ResMut<crate::input::orders::Orders>) {
    if let Some(code) = shot.aim.take() {
        let offer = seen
            .world()
            .spells(seen.program, seen.local)
            .into_iter()
            .find(|o| o.row.spell == code);

        if let Some(offer) = offer {
            orders.mode = crate::input::orders::Mode::Spell(offer);
        } else {
            warn!("--aim: {code} not offered");
        }
    }

    if let Some(code) = shot.build.take() {
        orders.mode = crate::input::orders::Mode::Build(code);
    }
}

/// The view moved a step along `--camera`'s path: `shot.counted` drawn
/// frames, so every run moves alike.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
fn steer(
    shot: Res<Shot>,
    (mut view, mut step): (ResMut<View>, ResMut<crate::map::zoom::Step>),
    mut steered: Local<u32>,
) {
    let frame = shot.counted;
    // Once a counted frame: a wait moves nothing.
    let fresh = frame > 0 && frame != *steered;
    let due = fresh && frame.is_multiple_of(HOLD);

    *steered = frame;

    match shot.camera {
        Some(Camera::Pan) if fresh => {
            // Right, down, left, up: screen points, y down.
            let ways = [Vec2::X, Vec2::Y, Vec2::NEG_X, Vec2::NEG_Y];

            view.drag(-ways[(frame / LEG % 4) as usize] * STRIDE);
        }
        Some(Camera::Levels) if due => crate::map::zoom::toggle(&mut view),
        Some(Camera::Wheel) if due => crate::map::zoom::sweep(&mut view, &mut step, frame / HOLD),
        _ => {}
    }
}

/// The settings a screenshot draws with: the defaults, muted, no
/// autosave; smooth zoom with a `zoom` or `--camera wheel` asked for;
/// the art's rings, as the original's captures (`--rings` picks another); videos unfiltered by
/// FSR (`--video-upscale`); Modern's other picks off, so a shot is a plain one.
pub fn settings(zoom: Option<f32>, camera: Option<Camera>) -> Settings {
    let mut settings = Settings::default();

    settings.audio.muted = true;
    settings.enhancements.smooth_zoom = zoom.is_some() || camera == Some(Camera::Wheel);
    settings.enhancements.rings = crate::files::config::Rings::Art;
    settings.enhancements.side_ring = false;
    settings.enhancements.speed_keys = false;
    settings.enhancements.smooth_arrows = false;
    settings.enhancements.message_links = false;
    settings.enhancements.rage_on_spell_bar = false;
    settings.enhancements.smooth_position = false;
    settings.enhancements.ui_upscale = crate::files::config::Upscale::None;

    // A shot writes no save.
    settings.game.autosave = crate::files::config::Autosave::Off;

    // FSR's video frame comes a pass or more late: plain frames keep a
    // shot the same each run. `--video-upscale fsr` asks for it.
    settings.enhancements.video_upscale = crate::files::config::VideoUpscale::Linear;
    settings
}

/// Set `app`, opening on the command line's game, up to capture `shot`
/// and exit: the window's size; once the game is in, the fog lifted as
/// the captures' reveal cheat did, the game paused.
pub fn install(app: &mut App, shot: Shot) {
    sized(app.world_mut(), shot.size, shot.scale);
    tabbed(app, &shot);

    // The wind held: a shot is the same each run.
    let wind = regent_draw::wind::Wind {
        held: Some(shot.wind.unwrap_or(0)),
        shadow: shot.wind_shadow,
        ..default()
    };

    let flag = pipelines(app);

    app.insert_resource(shot)
        .insert_resource(flag)
        .insert_resource(wind)
        .insert_resource(TimeUpdateStrategy::ManualDuration(FRAME))
        .add_systems(OnEnter(Screen::Playing), (hold, open).chain())
        .add_systems(
            Last,
            (choose, arm, count.after(crate::menu::lift), ending).run_if(in_game),
        )
        .add_systems(First, point)
        .add_systems(
            bevy_egui::EguiPrimaryContextPass,
            mini.after(crate::draw::frame::Ui::Bar).run_if(in_game),
        )
        .add_systems(
            Update,
            steer
                .in_set(crate::draw::frame::Frame::Input)
                .before(regent_draw::stage::pan)
                .run_if(in_game),
        )
        .add_systems(
            Update,
            look.after(crate::draw::frame_start)
                .before(regent_draw::stage::bound)
                .run_if(in_game),
        )
        .init_resource::<Agree>()
        .add_observer(agreed);
}

/// The game, just in: fog lifted unless `--fog fogged`, paused unless
/// `--running`.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
fn hold(mut commands: Commands, mut fog: ResMut<Fog>, mut paused: ResMut<Paused>, shot: Res<Shot>) {
    fog.sight = shot.sight;
    paused.0 = shot.hold == Hold::Paused;

    // The clock held until the frames count ([`Step`]).
    commands.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO));
}

/// The `--dialog` menu opened over the game just in; the settings
/// editor on the settings, a save dialog on the profile's saves.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
fn open(
    shot: Res<Shot>,
    (mut panels, mut paused): (ResMut<Panels>, ResMut<Paused>),
    (mut editor, settings): (ResMut<crate::ui::editor::Editor>, Res<Settings>),
    (mut saves, player): (
        ResMut<crate::save::dialog::Saves>,
        Res<crate::files::profile::Player>,
    ),
    (quests, art): (Res<crate::menu::Quests>, Res<crate::menu::Art>),
    (session, corpus, seen): (
        Res<crate::game::sim::Session>,
        Res<regent_draw::data::Corpus>,
        Watch,
    ),
) {
    if let Some(dialog) = shot.dialog {
        panels.state.options.show(dialog.open(), &mut paused.0);

        let game = || crate::ui::saved_game(&session, &corpus.text, (seen.now, seen.players));
        let saving = match dialog {
            Dialog::Save | Dialog::Overwrite => Some(crate::save::dialog::Dialog::Save),
            Dialog::Load => Some(crate::save::dialog::Dialog::Load),
            Dialog::Options | Dialog::Goals | Dialog::Settings => None,
        };

        if dialog == Dialog::Settings {
            editor.open(&settings);
        }

        if let Some(saving) = saving {
            crate::ui::show_saves(&mut saves, &player, saving, (game(), &corpus.text));
            saves.pog = quests.pog_pic(&art, session.quest_key());
        }

        // The question SAVE GAME asks of a name taken, taken or not.
        let taken = saves
            .place
            .as_ref()
            .and_then(|(dir, _)| crate::save::path(dir, &saves.name).ok());

        if let (Dialog::Overwrite, Some(path)) = (dialog, taken) {
            let name = saves.name.clone();

            saves.asking = Some(crate::save::dialog::Asked::Overwrite(
                crate::save::dialog::Chose::Save { path, name },
            ));
        }
    }
}

/// Set `app`, on the menus, up to capture `shot` after its frames and
/// exit.
pub fn menus(app: &mut App, shot: Shot) {
    sized(app.world_mut(), shot.size, shot.scale);
    tabbed(app, &shot);

    let flag = pipelines(app);

    app.insert_resource(shot)
        .insert_resource(flag)
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO))
        .add_systems(Last, count_menu)
        .init_resource::<Agree>()
        .add_observer(agreed);
}

/// [`Prepared`], kept by the render world; without one nothing
/// compiles, so ready from the start.
fn pipelines(app: &mut App) -> Prepared {
    let flag = Prepared(Arc::new(AtomicBool::new(
        app.get_sub_app(RenderApp).is_none(),
    )));

    if let Some(render) = app.get_sub_app_mut(RenderApp) {
        render
            .insert_resource(flag.clone())
            .add_systems(Render, prepared.in_set(RenderSystems::Cleanup));
    }

    flag
}

/// The settings editor to open on `--settings-tab`'s tab.
fn tabbed(app: &mut App, shot: &Shot) {
    if let Some(tab) = shot.settings_tab {
        app.insert_resource(crate::ui::editor::Editor::at(tab));
    }
}

/// The window `size` pixels at `scale`, fixed.
fn sized(world: &mut World, size: Size, scale: f32) {
    let mut windows = world.query_filtered::<&mut Window, With<PrimaryWindow>>();

    for mut window in windows.iter_mut(world) {
        window.resolution =
            WindowResolution::new(size.width, size.height).with_scale_factor_override(scale);
        window.resizable = false;
    }
}

/// Count down the menus' drawn frames as [`count`] does the game's:
/// counted once the window holds its size, the clock stepping on those
/// alone; the capture waits for the art and the pipelines.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
fn count_menu(
    mut commands: Commands,
    mut shot: ResMut<Shot>,
    (windows, prepared): (Query<&Window, With<PrimaryWindow>>, Res<Prepared>),
) {
    let sized = as_asked(&shot, &windows);

    shot.settled = if sized { shot.settled + 1 } else { 0 };
    advance(&mut commands, &mut shot, &prepared);
}

/// Whether a window is `shot`'s size, in logical points: its pixels
/// over the scale.
fn as_asked(shot: &Shot, windows: &Query<&Window, With<PrimaryWindow>>) -> bool {
    #[allow(clippy::cast_precision_loss, reason = "a window's pixels")]
    let asked = Vec2::new(shot.size.width as f32, shot.size.height as f32) / shot.scale;

    windows.iter().any(|w| {
        (Vec2::new(w.width(), w.height()) - asked)
            .abs()
            .max_element()
            < HALF_POINT
    })
}

/// A drawn frame's [`Step`] taken: a frame counted, or the capture
/// asked for; the next frame's clock stepping on a counted one alone.
fn advance(commands: &mut Commands, shot: &mut Shot, prepared: &Prepared) {
    shot.waited += 1;

    let pending = regent_draw::art::busy() || !prepared.0.load(Ordering::Relaxed);

    shot.ready = if pending { 0 } else { shot.ready + 1 };

    let step = step(shot);

    match step {
        Step::Capture => {
            commands.spawn(Screenshot::primary_window());
            shot.frames = None;
        }
        Step::Count => {
            shot.frames = shot.frames.map(|left| left - 1);
            shot.counted += 1;
        }
        Step::Fail => {
            error!(
                "screenshot: art, pipelines or materials pending after {BUSY} frames; not saved"
            );
            commands.write_message(AppExit::error());
            shot.frames = None;
        }
        Step::Wait => {}
    }

    let next = match step {
        Step::Count => FRAME,
        Step::Wait | Step::Capture | Step::Fail => Duration::ZERO,
    };

    commands.insert_resource(TimeUpdateStrategy::ManualDuration(next));
}

/// What a drawn frame does once the opening view is framed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    /// Held: the next frame's clock stands, so the game too. How long a
    /// wait lasts is the wall clock's: the window's size, art, pipelines.
    Wait,
    /// One of `--frames`: the next frame's clock steps.
    Count,
    /// The capture asked for.
    Capture,
    /// Still pending after [`BUSY`] frames: no picture, a failed exit.
    Fail,
}

/// `shot`'s step this frame: frames count once the window holds its
/// size; the capture waits for the art and pipelines too, failing past
/// [`BUSY`] frames.
const fn step(shot: &Shot) -> Step {
    let steady = shot.settled >= SETTLED || shot.waited >= WAIT;

    match shot.frames {
        Some(0) if steady && shot.ready >= READY => Step::Capture,
        Some(0) if steady && shot.waited >= BUSY => Step::Fail,
        Some(left) if left > 0 && steady => Step::Count,
        _ => Step::Wait,
    }
}

/// Count down the drawn frames once the opening view is set, framing it
/// again each frame: the window's first frames can come at the display's
/// scale, 3260 × 2037 for 1920 × 1200 at 1.7, and the bound would keep a
/// framing made in that room. At zero, with the window the asked size
/// for [`SETTLED`] frames, ask for the capture; after [`WAIT`] frames, ask
/// regardless, to fail on the size. The game is held paused. Nothing is
/// counted under the game's loading page (`menu::drape`).
///
/// Only counted frames step the clock ([`Step`]), so a `--running` game
/// plays `--frames` frames whatever the waits: every run the same frame.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
fn count(
    mut commands: Commands,
    (mut framed, drape): (ResMut<Framed>, Res<crate::menu::Drape>),
    mut shot: ResMut<Shot>,
    mut paused: ResMut<Paused>,
    mut view: ResMut<View>,
    (windows, prepared): (Query<&Window, With<PrimaryWindow>>, Res<Prepared>),
) {
    // The window measured, not the room: the room ends above the bottom
    // bar, so its corner is never the window's.
    let sized = view.rect.is_some() && as_asked(&shot, &windows);

    paused.0 = shot.hold == Hold::Paused;

    // The asked zoom, about the view's middle; no lower than the map's
    // bound allows (`View::least`), which `stage::bound` would undo.
    let least = view.least().unwrap_or(0.0);
    #[allow(clippy::float_cmp, reason = "set exactly below")]
    if let Some(zoom) = shot.zoom.map(|z| z.max(least)).filter(|z| *z != view.zoom) {
        let by = zoom / view.zoom;

        view.magnify(by, None);
        view.zoom = zoom;
    }

    shot.settled = if sized { shot.settled + 1 } else { 0 };

    if framed.0 && !drape.is_up() {
        // Framed again each frame, but under a moving camera once
        // settled: its moves would be undone.
        framed.0 = shot.camera.is_some() && shot.settled >= SETTLED;
        advance(&mut commands, &mut shot, &prepared);
    }
}

/// Whether the render world had nothing waiting when it last drew: a
/// pipeline still compiling leaves its pieces out of the frame, a
/// material or mesh not yet prepared draws wrong ([`drawable`]).
#[derive(Resource, Clone)]
struct Prepared(Arc<AtomicBool>);

/// [`Prepared`], as the pipeline cache and the 3D meshes stand after the
/// frame's queue.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
fn prepared(
    cache: Res<PipelineCache>,
    flag: Res<Prepared>,
    (materials, made): (
        Res<RenderMaterialInstances>,
        Res<ErasedRenderAssets<PreparedMaterial>>,
    ),
    (instances, meshes): (Res<RenderMeshInstances>, Res<RenderAssets<RenderMesh>>),
) {
    let compiled = cache.waiting_pipelines().next().is_none();

    flag.0.store(
        compiled && drawable(&materials, &made, &instances, &meshes),
        Ordering::Relaxed,
    );
}

/// Whether every visible mesh draws as asked: its material prepared,
/// its instance bound to that material's slot, its mesh prepared.
///
/// Bevy extracts a mesh whose material is not yet prepared with a
/// placeholder slot, set right once it is (2D's `extract_2d_mesh`; 3D's
/// mesh extraction reads the material's binding as it stands).
/// **Inferred**: until then it draws another material's textures, the
/// coloured blocks a shot once saved in place of the pieces.
fn drawable(
    materials: &RenderMaterialInstances,
    prepared: &ErasedRenderAssets<PreparedMaterial>,
    instances: &RenderMeshInstances,
    meshes: &RenderAssets<RenderMesh>,
) -> bool {
    materials.instances.iter().all(|(entity, material)| {
        let made = prepared.get(material.asset_id).map(|m| m.binding);
        let mesh = instances.render_mesh_queue_data(*entity);

        match (made, mesh) {
            (Some(made), Some(mesh)) => {
                let bound = mesh.material_bindings_index();

                bound.group == made.group
                    && bound.slot == made.slot
                    && meshes.get(mesh.mesh_asset_id()).is_some()
            }
            _ => false,
        }
    })
}

/// Captures of the held frame to wait for two alike in a row.
const AGREE: u32 = 10;

/// The game's last capture, and how many were taken.
#[derive(Resource, Default)]
struct Agree {
    last: Option<Vec<u8>>,
    taken: u32,
}

/// The game's capture, once two in a row agree: the frame is held
/// ([`Step`]), so a pass that drew late (a fresh target, a pipeline
/// done that frame) shows as a difference. Then as [`saved`].
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy observer takes its parameters by value"
)]
fn agreed(
    captured: On<ScreenshotCaptured>,
    (shot, mut agree): (Res<Shot>, ResMut<Agree>),
    mut commands: Commands,
    exit: MessageWriter<AppExit>,
) {
    let alike = agree.last.is_some() && agree.last == captured.image.data;

    agree.taken += 1;

    if alike || agree.taken >= AGREE {
        if !alike {
            warn!("screenshot: no two captures alike in {AGREE}");
        }

        saved(captured, shot, exit);
    } else {
        agree.last.clone_from(&captured.image.data);
        commands.spawn(Screenshot::primary_window());
    }
}

/// The capture, written as a PNG; exit, failing if it is not the asked
/// size (a display that would not give the window its pixels).
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy observer takes its parameters by value"
)]
fn saved(captured: On<ScreenshotCaptured>, shot: Res<Shot>, mut exit: MessageWriter<AppExit>) {
    let drawn = Size {
        width: captured.image.width(),
        height: captured.image.height(),
    };
    let written = captured
        .image
        .clone()
        .try_into_dynamic()
        .map_err(|e| e.to_string())
        .and_then(|image| {
            image
                .to_rgb8()
                .save(&shot.path)
                .map_err(|e| format!("{}: {e}", shot.path.display()))
        });

    let outcome = match written {
        Err(e) => Err(e),
        Ok(()) if drawn != shot.size => Err(format!(
            "{}: drawn {}x{}, not {}x{}; a fractional display scale \
             rounds a Wayland window: unset WAYLAND_DISPLAY for X11",
            shot.path.display(),
            drawn.width,
            drawn.height,
            shot.size.width,
            shot.size.height
        )),
        Ok(()) => Ok(()),
    };

    match outcome {
        Ok(()) => {
            info!("screenshot: {}", shot.path.display());
            exit.write(AppExit::Success);
        }
        Err(e) => {
            error!("screenshot: {e}");
            exit.write(AppExit::error());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A shot draws the art's rings, whatever the game's default.
    #[test]
    fn a_shot_draws_the_arts_rings() {
        assert_eq!(
            settings(None, None).enhancements.rings,
            crate::files::config::Rings::Art
        );
    }

    #[test]
    fn a_size_reads_as_width_by_height() {
        assert_eq!(
            size(SIZE),
            Ok(Size {
                width: 1920,
                height: 1200
            })
        );
        assert!(size("1920").is_err());
        assert!(size("0x5").is_err());
        assert!(size("ax5").is_err());
    }

    #[test]
    fn a_select_missing_names_its_code() {
        let pieces = [(FourCc::new(b"ABQ3"), 7), (FourCc::new(b"AVL1"), 9)];

        assert_eq!(picked(&pieces, (FourCc::new(b"ABQ3"), 0)), Ok(7));

        let missed = picked(&pieces, (FourCc::new(b"ABQ4"), 0)).expect_err("no ABQ4");

        assert!(missed.contains("ABQ4:0"), "{missed}");
    }

    /// A paused shot of the defaults.
    fn test_shot() -> Shot {
        Shot {
            path: PathBuf::new(),
            size: size(SIZE).expect("a size"),
            scale: SCALE,
            frames: Some(FRAMES),
            waited: 0,
            settled: 0,
            ready: 0,
            counted: 0,
            select: None,
            aim: None,
            build: None,
            page: None,
            zoom: None,
            end: None,
            look: None,
            from: None,
            pointer: None,
            mini: None,
            hold: Hold::Paused,
            camera: None,
            dialog: None,
            settings_tab: None,
            wind: None,
            wind_shadow: regent_draw::wind::Shadow::Sheared,
            sight: Sight::Lifted,
        }
    }

    /// A capture's clock steps a fixed time a drawn frame, so the chat
    /// box's lines scroll off alike in every run: the game's on its way
    /// in (`hold` holds it there), the menus' held until frames count.
    #[test]
    fn a_shot_steps_its_clock_by_the_frame() {
        let shot = test_shot();

        for (put, first) in [
            (install as fn(&mut App, Shot), FRAME),
            (menus, Duration::ZERO),
        ] {
            let mut app = App::new();

            put(&mut app, shot.clone());

            assert!(matches!(
                app.world().get_resource::<TimeUpdateStrategy>(),
                Some(TimeUpdateStrategy::ManualDuration(d)) if *d == first
            ));
        }
    }

    /// Where a shot is taken.
    #[derive(Clone, Copy)]
    enum Over {
        Game,
        /// The game, under its loading page this many frames.
        Draped(u32),
        Menus,
    }

    /// The clock's steps before the capture of a `--running` shot whose
    /// window takes `sizing` frames to reach its size, the art or the
    /// pipelines pending `late` frames past the countdown.
    fn stepped(sizing: u32, late: u32, over: Over) -> u32 {
        let mut app = App::new();
        let shot = Shot {
            hold: Hold::Running,
            ..test_shot()
        };
        let asked = (shot.size.width, shot.size.height);
        let flag = Prepared(Arc::new(AtomicBool::new(false)));
        let view = View {
            rect: Some(Rect::new(0.0, 0.0, 100.0, 100.0)),
            ..default()
        };

        app.insert_resource(shot)
            .insert_resource(flag.clone())
            .insert_resource(TimeUpdateStrategy::ManualDuration(FRAME));

        let window = app
            .world_mut()
            .spawn((Window::default(), PrimaryWindow))
            .id();

        match over {
            Over::Game | Over::Draped(_) => {
                let drape = match over {
                    Over::Draped(_) => crate::menu::Drape::up(),
                    Over::Game | Over::Menus => crate::menu::Drape::default(),
                };

                app.insert_resource(Framed(true))
                    .insert_resource(view)
                    .insert_resource(drape)
                    .init_resource::<Paused>()
                    .init_resource::<Fog>()
                    .add_systems(Update, count);
                app.world_mut().run_system_cached(hold).unwrap();
            }
            Over::Menus => {
                app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO))
                    .add_systems(Update, count_menu);
            }
        }

        let mut steps = 0;

        for frame in 0..1000 {
            let size = if frame < sizing { (640, 480) } else { asked };

            app.world_mut()
                .get_mut::<Window>(window)
                .unwrap()
                .resolution = WindowResolution::new(size.0, size.1);
            flag.0
                .store(frame >= sizing + FRAMES + late, Ordering::Relaxed);
            // The opening view framed again each frame, as `draw` does.
            app.insert_resource(Framed(true));

            let counted = matches!(
                app.world().resource::<TimeUpdateStrategy>(),
                TimeUpdateStrategy::ManualDuration(d) if *d == FRAME
            );

            // The page gone: frames count from the next.
            if let Over::Draped(up) = over {
                assert!(!counted || frame > up, "counted at {frame}, under the page");

                if frame == up {
                    app.insert_resource(crate::menu::Drape::default());
                }
            }

            if counted {
                steps += 1;
            }

            app.update();

            let shots = app
                .world_mut()
                .query::<&Screenshot>()
                .iter(app.world())
                .count();

            if shots > 0 {
                return steps;
            }
        }

        panic!("no capture");
    }

    /// A `--running` shot, and a menu shot, plays `--frames` frames
    /// however long the window, the art, the pipelines and the game's
    /// loading page keep it waiting; none under the page.
    #[test]
    fn a_running_shot_plays_its_frames_whatever_the_waits() {
        let runs = [(0, 0), (70, 0), (0, 9), (75, 30)];

        for over in [Over::Game, Over::Draped(40), Over::Draped(90), Over::Menus] {
            let steps = runs.map(|(sizing, late)| stepped(sizing, late, over));

            assert_eq!(steps, [FRAMES; 4]);
        }
    }

    /// A shot still waiting on art or pipelines at [`BUSY`] fails rather
    /// than saving a picture with stand-ins in it.
    #[test]
    fn a_shot_still_pending_fails_rather_than_saving() {
        let shot = |ready| Shot {
            frames: Some(0),
            settled: SETTLED,
            waited: BUSY,
            ready,
            ..test_shot()
        };

        assert_eq!(step(&shot(0)), Step::Fail);
        assert_eq!(step(&shot(READY)), Step::Capture);
    }

    /// A visible mesh whose material the render world has not prepared
    /// keeps the shot waiting; none at all, ready.
    #[test]
    fn a_mesh_without_its_material_keeps_a_shot_waiting() {
        let mut materials = RenderMaterialInstances::default();
        let prepared = ErasedRenderAssets::<PreparedMaterial>::default();
        let instances = RenderMeshInstances::CpuBuilding(default());
        let meshes = RenderAssets::<RenderMesh>::default();

        assert!(drawable(&materials, &prepared, &instances, &meshes));

        materials.instances.insert(
            Entity::from_raw_u32(1).expect("an entity").into(),
            bevy::pbr::RenderMaterialInstance {
                asset_id: AssetId::<regent_draw::plain::Plain>::default().untyped(),
                last_change_tick: default(),
            },
        );

        assert!(!drawable(&materials, &prepared, &instances, &meshes));
    }

    /// `--look-from` holds its cell for [`FROM`] counted frames, however
    /// long the waits before them.
    #[test]
    fn a_pan_starts_after_counted_frames() {
        let shot = |counted| Shot {
            from: Some((1.0, 1.0)),
            look: Some((2.0, 2.0)),
            waited: FROM + 40,
            counted,
            ..test_shot()
        };

        assert_eq!(aim(&shot(0)), Some((1.0, 1.0)));
        assert_eq!(aim(&shot(FROM)), Some((2.0, 2.0)));
    }
}
