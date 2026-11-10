//! The chat area: a chat line, and the box game messages and sent lines
//! type into (`view::panel::chat`: four rows, 10 ms a character, a row up each
//! 3 s idle). The look is a placeholder; the timing is the engine's.
//!
//! ```text
//!  Enter ──> the line opens; the game sees no key while it is open
//!  Enter ──> a phrase sends its CHEAT_CODE_MESSAGE, its reply goes in the
//!            box; any other line goes in as typed, in the chat colour;
//!            an empty line in a base quest is PLANET FARGO, reply empty
//!  Escape ─> closed, nothing sent
//!  the world's lines to the crown ──> the box, green
//! ```
//!
//! Cheats are allowed, as the session allows them in a new game.
//!
//! The box scales with the interface and stands above the open spell
//! bar. With `[enhancements] message_links` (this game's) each line about
//! a unit shows its portrait, and a click selects it and centres the view:
//!
//! ```text
//!  [face] Recruited: Ranger          the hero, where it stands now
//!  [face] Library #1 Research ...    the building
//!  [face] Olaf has died              where it was when said
//! ```
//!
//! A dead unit's portrait is a grave's, as the panel draws one.

use std::collections::VecDeque;
use std::sync::Arc;

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::{EguiContexts, egui};

use regent_core::content::unit::Unit;
use regent_core::data::catalogue::Catalogue;
use regent_core::data::payload::{Palette, Rgb};
use regent_core::sim::game::Input;
use regent_core::sim::world::World;
use regent_core::sim::world::chat::LineSubject;
use regent_core::sim::world::cheat::{self, CheatCode, Cheats};
use regent_core::view::panel::chat::{self as rules, InfoBox};
use regent_draw::art::Art;
use regent_draw::data::Corpus;
use regent_draw::map::Maps;
use regent_draw::stage::View;

use crate::files::config::Settings;
use crate::game::seen::{Ask, News, Outbox, Watch};
use crate::map::select::{Reading, Selected};
use crate::menu::fit::Fit;
use crate::panel::Panels;
use crate::panel::canvas::Canvas;
use crate::panel::ink::{Pic, Tone};
use crate::ui::typeface;
use crate::ui::{Covered, Paused};

/// Where the line sits: up from the window's bottom centre.
const LINE_AT: egui::Vec2 = egui::vec2(0.0, -48.0);

/// The line's width, points.
const LINE_WIDTH: f32 = 320.0;

/// The box's font size, HD pixels: `fnt4`'s 15 px row, in egui's font
/// (`typeface::like_egui` in another).
const FONT_SIZE: f32 = 15.0;

/// The box's row pitch, HD pixels: the font with room between rows.
const ROW_PITCH: f32 = 19.0;

/// A row's pitch with portraits, HD pixels: room for a face.
const FACE_PITCH: f32 = 28.0;

/// Between a portrait and its text, HD pixels.
const FACE_GAP: f32 = 4.0;

/// The text's shadow, HD pixels down and right: legible on any ground.
const SHADOW: f32 = 1.0;

/// A hovered linked line's backdrop.
const HOVERED: egui::Color32 = egui::Color32::from_black_alpha(96);

/// Posts whose source is kept: more than the box's rows can show.
const SOURCES: usize = 32;

/// The characters measured for the box's advances: printable ASCII.
const MEASURED: std::ops::RangeInclusive<char> = ' '..='~';

/// `--message-clock`: what the box types and scrolls on. Display only:
/// the simulation never reads it.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq, clap::ValueEnum)]
pub enum Clock {
    /// The wall's, as the engine's (`timeGetTime`, infobox init): readable
    /// at any speed. Stopped while paused, a departure: the engine's runs.
    #[default]
    Real,
    /// The game's: a slower game keeps lines longer; paused, they stay
    /// and typing waits. Deterministic for replays and tests.
    Game,
}

impl Clock {
    /// Now on this clock, ms: `real` the wall's elapsed time, `game` the
    /// simulation's clock.
    fn now(self, real: std::time::Duration, game: u32) -> u32 {
        match self {
            Clock::Real => u32::try_from(real.as_millis()).unwrap_or(u32::MAX),
            Clock::Game => game,
        }
    }
}

/// The line's state.
#[derive(Default, PartialEq, Eq)]
enum State {
    #[default]
    Closed,
    /// Opened this frame: its Enter is not a send.
    Opening,
    Open,
}

/// The box's colours: `IBOX`'s nearest to the engine's.
#[derive(Clone, Copy)]
struct Colours {
    game: Rgb,
    chat: Rgb,
}

impl Colours {
    fn of(corpus: &Corpus) -> Colours {
        let catalogue = &corpus.catalogue;
        let palette = catalogue
            .find(b"PALT", rules::PALETTE)
            .and_then(|at| catalogue.record(b"PALT", at))
            .and_then(|r| Palette::parse(r.data).ok());
        let pick = |rgb: u32| {
            let [_, r, g, b] = rgb.to_be_bytes();

            palette
                .as_ref()
                .map_or(Rgb { r, g, b }, |p| rules::colour(p, rgb))
        };

        Colours {
            game: pick(rules::GAME_RGB),
            chat: pick(rules::CHAT_RGB),
        }
    }
}

/// Whether the box shows portraits and takes clicks
/// (`[enhancements] message_links`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Links {
    Off,
    On,
}

/// The chat line, and the box.
#[derive(Resource, Default)]
pub struct Chat {
    state: State,
    line: String,
    /// Made as the view is first known.
    board: Option<InfoBox>,
    colours: Option<Colours>,
    /// Advances of [`MEASURED`], points, and the widest, at font size
    /// `measured`.
    advances: Vec<i32>,
    widest: i32,
    measured: f32,
    /// The box's posts about a unit, newest last.
    sources: VecDeque<(u64, LineSubject)>,
    /// The wall's time the box has run, for [`Clock::Real`].
    wall: std::time::Duration,
}

impl Chat {
    /// The wall's time run, `delta` more unless `paused`: a pause holds
    /// the box's lines and typing where they are.
    fn wall(&mut self, delta: std::time::Duration, paused: &Paused) -> std::time::Duration {
        if !paused.0 {
            self.wall += delta;
        }

        self.wall
    }
}

/// Enter opens the line where the game's keys are live (`map_keys::Live`),
/// until the game ends. While it is open, or a name box is edited, the
/// game's keys see nothing: egui takes the typing from the window's
/// events.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
pub fn keys(
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut chat: ResMut<Chat>,
    seen: Watch,
    (panels, mut live): (Res<crate::panel::Panels>, crate::input::map_keys::Live),
) {
    let naming = panels.state.naming.is_some();

    if seen.ended {
        chat.state = State::Closed;
    } else if chat.state == State::Closed
        && !naming
        && keys.just_pressed(KeyCode::Enter)
        && !crate::draw::fullscreen::pressed(&keys)
        && live.keys()
    {
        chat.state = State::Opening;
    }

    if chat.state != State::Closed || naming {
        keys.reset_all();
    }
}

/// What the box draws portraits with.
#[derive(bevy::ecs::system::SystemParam)]
pub struct Faces<'w> {
    settings: Res<'w, Settings>,
    panels: ResMut<'w, Panels>,
    art: ResMut<'w, Art>,
    maps: Res<'w, Maps>,
    /// Linked lines are added to it: a click there is not the map's.
    covered: ResMut<'w, Covered>,
}

/// The line while open; the box, its lines typed on [`Clock`].
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
#[allow(
    clippy::too_many_arguments,
    reason = "a system's arguments are its dependencies"
)]
#[allow(
    clippy::too_many_lines,
    reason = "the chat input, posts, and links share a draw pass"
)]
pub fn line(
    mut contexts: EguiContexts,
    mut chat: ResMut<Chat>,
    corpus: Res<Corpus>,
    mut selected: ResMut<Selected>,
    mut reading: ResMut<Reading>,
    mut view: ResMut<View>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut faces: Faces,
    (time, clock, paused): (Res<Time<Real>>, Res<Clock>, Res<Paused>),
    (seen, mut outbox, mut news): (Watch, ResMut<Outbox>, ResMut<News>),
) -> Result {
    let ctx = contexts.ctx_mut()?.clone();
    let wall = chat.wall(time.delta(), &paused);
    let now = clock.now(wall, seen.world().now());
    let colours = *chat.colours.get_or_insert_with(|| Colours::of(&corpus));
    let mut posts: Vec<(String, Rgb, Option<LineSubject>)> = Vec::new();

    if chat.state != State::Closed {
        let opening = chat.state == State::Opening;
        let edit = egui::Area::new(egui::Id::new("chat"))
            .anchor(egui::Align2::CENTER_BOTTOM, LINE_AT)
            .show(&ctx, |ui| {
                ui.add(egui::TextEdit::singleline(&mut chat.line).desired_width(LINE_WIDTH))
            })
            .inner;
        let (enter, escape) = ctx.input(|i| {
            (
                i.key_pressed(egui::Key::Enter),
                i.key_pressed(egui::Key::Escape),
            )
        });

        edit.request_focus();
        chat.state = State::Open;

        if escape {
            chat.state = State::Closed;
            chat.line.clear();
        } else if enter && !opening {
            let line = std::mem::take(&mut chat.line);

            chat.state = State::Closed;
            let sent = send(
                &corpus,
                &selected,
                (seen.world(), seen.local, &mut outbox),
                &line,
            );

            // A line said as typed goes to the peers too; a cheat says
            // its reply instead.
            if let Some(said) = &sent
                && *said == line
            {
                outbox.push(Ask::Chat(said.clone()));
            }

            posts.extend(sent.map(|s| (s, colours.chat, None)));
        }
    }

    // The other peers' lines.
    let heard = std::mem::take(&mut news.chat).into_iter();

    posts.extend(heard.map(|(who, line)| (format!("{who}: {line}"), colours.chat, None)));

    // The players who left, in the game's colour: a message, no order.
    let left = std::mem::take(&mut news.left).into_iter();

    posts.extend(left.map(|l| (l.line(&corpus.text), colours.game, None)));

    // A resync after a desync, alike.
    let notices = std::mem::take(&mut news.notices).into_iter();

    posts.extend(notices.map(|n| (n.line(&corpus.text), colours.game, None)));

    // The world's lines the local player sees.
    let local = seen.local;
    let said = std::mem::take(&mut news.said);
    let shown = said
        .into_iter()
        .filter(|s| s.seen_by(local))
        .filter_map(|s| {
            let text = rules::text(&corpus.catalogue, &corpus.text, &s)?;

            Some((text, colours.game, s.from))
        });

    posts.extend(shown);

    if let Some(area) = view.rect {
        let links = if faces.settings.enhancements.message_links {
            Links::On
        } else {
            Links::Off
        };
        let scale = crate::ui::layout::scale(egui::vec2(window.width(), window.height()));
        let placed = placed(area, &faces.covered.0, scale, links);
        let font = egui::FontId::proportional(placed.font);

        // The advances again as the window's scale changes the font.
        if chat.advances.is_empty() || (chat.measured - placed.font).abs() > f32::EPSILON {
            measure(&ctx, &font, &mut chat);
        }

        let Chat {
            board,
            advances,
            widest,
            sources,
            ..
        } = &mut *chat;
        #[allow(clippy::cast_possible_truncation, reason = "a view's width, points")]
        let width = placed.width as i32;
        let board =
            board.get_or_insert_with(|| InfoBox::new(width, *widest, rules::SCROLL_MS, now));
        let advance = |c: char| {
            let at = (c as usize).checked_sub(*MEASURED.start() as usize);

            at.and_then(|i| advances.get(i).copied()).unwrap_or(*widest)
        };

        board.resize(width, *widest);

        for (text, colour, from) in posts {
            let post = board.post(&text, colour, now);

            sources.extend(from.map(|f| (post, f)));
        }

        while sources.len() > SOURCES {
            sources.pop_front();
        }

        board.step(now, &advance);

        let rows = Lines {
            board,
            sources,
            links,
            world: seen.world(),
        };
        if let Some(at) = draw(&ctx, (&placed, &font), &rows, (&corpus, &mut faces))
            .and_then(|s| follow(seen.world(), &s, &mut selected, &mut reading))
        {
            crate::ui::minimap::look_at(&faces.maps, &mut view, at);
        }
    }

    Ok(())
}

/// The box's advances in `font`, whole points.
#[allow(clippy::cast_possible_truncation, reason = "a glyph's width, points")]
fn measure(ctx: &egui::Context, font: &egui::FontId, chat: &mut Chat) {
    chat.advances = ctx.fonts_mut(|f| {
        MEASURED
            .map(|c| f.glyph_width(font, c).ceil() as i32)
            .collect()
    });
    chat.widest = chat.advances.iter().copied().max().unwrap_or_default();
    chat.measured = font.size;
}

/// Select a line's live unit and locate it, or use its last place.
fn follow(
    world: &World,
    source: &LineSubject,
    selected: &mut Selected,
    reading: &mut Reading,
) -> Option<(i32, i32)> {
    let live = world
        .shown_one(source.id)
        .filter(|shown| shown.code == source.code);

    if let Some(shown) = live {
        selected.0 = Some(shown.agent);
        reading.0 = None;
    }

    live.and_then(|shown| world.position(shown.agent))
        .or(source.at)
}

/// What the box shows: its rows, the units its posts are about, and
/// whether those show; the world they are in.
struct Lines<'a> {
    board: &'a InfoBox,
    sources: &'a VecDeque<(u64, LineSubject)>,
    links: Links,
    world: &'a World,
}

impl Lines<'_> {
    /// Row `n`'s unit, while links are on.
    fn source(&self, n: usize) -> Option<&LineSubject> {
        let post = self.board.owners().get(n).copied().flatten()?;
        let found = self.sources.iter().find(|(p, _)| *p == post);

        found.map(|(_, s)| s).filter(|_| self.links == Links::On)
    }

    /// Whether row `n` is its post's first shown.
    fn first(&self, n: usize) -> bool {
        let owners = self.board.owners();

        n == 0 || owners.get(n - 1) != owners.get(n)
    }
}

/// The box's rows, each run of one colour its own text over a shadow;
/// with links, a row about a unit a press area, and its portrait on the
/// post's first row. The unit of a pressed row.
#[allow(clippy::cast_precision_loss, reason = "small offsets and row counts")]
fn draw(
    ctx: &egui::Context,
    (placed, font): (&Placed, &egui::FontId),
    lines: &Lines<'_>,
    (corpus, faces): (&Corpus, &mut Faces),
) -> Option<LineSubject> {
    let at = placed.at;
    let mut chose = None;

    egui::Area::new(egui::Id::new("said"))
        .fixed_pos(at)
        .interactable(lines.links == Links::On)
        .show(ctx, |ui| {
            for (n, row) in lines.board.rows().iter().enumerate() {
                let top = at + egui::vec2(0.0, n as f32 * placed.pitch);
                let galleys = laid(ui, font, row);

                // The row's portrait and text only: the shade and the
                // press area end where the text does.
                let rect = egui::Rect::from_min_size(
                    top,
                    egui::vec2(placed.face + galleys.1.size().x, placed.pitch),
                );

                // A linked row: a press area, its portrait.
                if let Some(source) = lines.source(n) {
                    let id = egui::Id::new(("said", n));
                    let pressed = ui.interact(rect, id, egui::Sense::click());

                    if pressed.hovered() {
                        ui.painter().rect_filled(rect, 0.0, HOVERED);
                        ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
                    }

                    if pressed.clicked() {
                        chose = Some(source.clone());
                    }

                    if lines.first(n) {
                        let tone = tone(lines.world, &corpus.catalogue, source);

                        face(ui, (corpus, faces), (source, tone), top, placed);
                    }

                    faces
                        .covered
                        .0
                        .push(Rect::new(rect.min.x, rect.min.y, rect.max.x, rect.max.y));
                }

                let text =
                    top + egui::vec2(placed.face, (placed.pitch - placed.row) / 2.0 - placed.lift);

                text_row(ui, galleys, (text, placed.shadow));
            }
        });

    chose
}

/// A row's text laid out: a black copy for its shadow, and each run of
/// one colour its own.
fn laid(
    ui: &egui::Ui,
    font: &egui::FontId,
    row: &[(char, Rgb)],
) -> (Arc<egui::Galley>, Arc<egui::Galley>) {
    let job = |shade: Option<egui::Color32>| {
        let mut job = egui::text::LayoutJob::default();

        for (c, colour) in row {
            let colour = shade.unwrap_or(egui::Color32::from_rgb(colour.r, colour.g, colour.b));

            job.append(
                &c.to_string(),
                0.0,
                egui::TextFormat::simple(font.clone(), colour),
            );
        }

        ui.fonts_mut(|f| f.layout_job(job))
    };

    (job(Some(egui::Color32::BLACK)), job(None))
}

/// A row's [`laid`] text at `at`, its shadow `shadow` down and right
/// under it.
fn text_row(
    ui: &egui::Ui,
    (shade, ink): (Arc<egui::Galley>, Arc<egui::Galley>),
    (at, shadow): (egui::Pos2, f32),
) {
    ui.painter()
        .galley(at + egui::Vec2::splat(shadow), shade, egui::Color32::BLACK);
    ui.painter().galley(at, ink, egui::Color32::WHITE);
}

/// A line's portrait tone: a grave's, as the panel draws one
/// (`panel::upper`'s portrait), while its unit lies dead or is gone
/// (**Inferred**: a unit gone has died; a dismissed hero is too). A
/// building keeps its colours, as the panel's.
fn tone(world: &World, catalogue: &Catalogue<'_>, source: &LineSubject) -> Tone {
    let here = world
        .piece(source.id)
        .is_some_and(|p| p.code == source.code);
    let lying = world.shown_one(source.id).is_some_and(|s| s.ruined);
    let dead = !here || lying;
    let building = Unit::find(catalogue, source.code)
        .and_then(Result::ok)
        .is_some_and(|u| u.desc.building().is_some());

    if dead && !building {
        Tone::Grave
    } else {
        Tone::Plain
    }
}

/// `source`'s portrait in `tone`, a square the row's height at `top`.
fn face(
    ui: &mut egui::Ui,
    (corpus, faces): (&Corpus, &mut Faces),
    (source, tone): (&LineSubject, Tone),
    top: egui::Pos2,
    placed: &Placed,
) {
    let side = placed.face - placed.gap;
    let rect = egui::Rect::from_min_size(top, egui::Vec2::splat(side));
    let Faces { panels, art, .. } = faces;
    let mut canvas = Canvas {
        ui,
        ink: &mut panels.ink,
        art,
        source: corpus.source(),
        catalogue: &corpus.catalogue,
        fit: Fit {
            scale: 1.0,
            origin: egui::Pos2::ZERO,
        },
        salt: "said",
    };

    canvas.stretched(Pic::Portrait(source.code), tone, rect);
}

/// `line` sent as chat cheats sends it: where the session allows
/// cheats (`World::cheats`), a phrase with its target sends its cheat
/// and says the reply; else the line as typed. An empty line is a phrase
/// in a base quest (`PLANET FARGO`, past the base table), its reply empty
/// too. An empty line says nothing.
fn send(
    corpus: &Corpus,
    selected: &Selected,
    (world, local, outbox): (&World, u32, &mut Outbox),
    line: &str,
) -> Option<String> {
    let allowed = world.cheats() == Cheats::Allowed;
    let phrase = cheat::recognise(&corpus.text, line).filter(|_| allowed);
    let sent = phrase.and_then(|p| {
        let arg = world.cheat_arg(p.target, local, selected.0)?;

        outbox.push(Ask::Input(Input::Cheat(CheatCode {
            cheat: p.cheat,
            arg,
        })));

        Some(cheat::reply(&corpus.text, &p))
    });
    let said = sent.map_or_else(|| line.to_owned(), |reply| reply.trim_end().to_owned());

    (!said.is_empty()).then_some(said)
}

/// The chat line, a game's.
pub fn plugin(app: &mut bevy::prelude::App) {
    use crate::game::scope::Scoped as _;

    app.per_game::<Chat>().init_resource::<Clock>();
}

/// The box's place and sizes on the window, points.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Placed {
    /// The first row's top left.
    at: egui::Pos2,
    /// The text's width, past the portraits.
    width: f32,
    font: f32,
    /// How far up the text is drawn: `typeface::lift`.
    lift: f32,
    /// A row's text height, and the pitch rows stand at.
    row: f32,
    pitch: f32,
    /// The portraits' column, the gap after them included; 0 without.
    face: f32,
    gap: f32,
    shadow: f32,
}

/// Where the box goes over the map `area`, `scale` points an HD pixel:
/// the engine's insets, scaled; its bottom as far above the lowest
/// thing `covered` over the map's bottom (the open spell bar) as the
/// engine's is above the view's.
#[allow(clippy::cast_precision_loss, reason = "small offsets")]
fn placed(area: Rect, covered: &[Rect], scale: f32, links: Links) -> Placed {
    let floor = covered
        .iter()
        .filter(|r| r.max.y >= area.max.y && r.min.y > area.min.y)
        .map(|r| r.min.y)
        .fold(area.max.y, f32::min);
    let row = ROW_PITCH * scale;
    let (pitch, face) = match links {
        Links::Off => (row, 0.0),
        Links::On => (FACE_PITCH * scale, (FACE_PITCH + FACE_GAP) * scale),
    };
    let inset = rules::INSET as f32 * scale;
    let below = (rules::RAISE - rules::HEIGHT) as f32 * scale;

    Placed {
        at: egui::pos2(
            area.min.x + inset,
            floor - below - rules::ROWS as f32 * pitch,
        ),
        width: (area.width() - 2.0 * inset - face).max(0.0),
        font: typeface::like_egui(FONT_SIZE * scale),
        lift: typeface::lift(FONT_SIZE * scale),
        row,
        pitch,
        face,
        gap: FACE_GAP * scale,
        shadow: SHADOW * scale,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_link_selects_its_living_source() {
        let (begun, _) = crate::game::begin::test_game(&[]);
        let world = begun.sim.world();
        let shown = world.shown().into_iter().next().expect("a piece");
        let source = LineSubject {
            id: shown.agent,
            code: shown.code,
            at: Some(shown.at),
        };

        let mut selected = Selected(None);
        let mut reading = Reading(Some(42));

        assert_eq!(
            follow(world, &source, &mut selected, &mut reading),
            world.position(shown.agent)
        );
        assert_eq!(selected.0, Some(shown.agent));
        assert_eq!(reading.0, None);

        let wrong = LineSubject {
            code: regent_core::data::fourcc::FourCc(*b"XXXX"),
            ..source.clone()
        };
        selected.0 = None;
        reading.0 = Some(42);
        assert_eq!(
            follow(world, &wrong, &mut selected, &mut reading),
            source.at
        );
        assert_eq!(selected.0, None);
        assert_eq!(reading.0, Some(42));

        let gone = LineSubject {
            id: u32::MAX,
            ..source.clone()
        };
        assert_eq!(follow(world, &gone, &mut selected, &mut reading), source.at);
        assert_eq!(selected.0, None);
        assert_eq!(reading.0, Some(42));
    }

    /// What holds the keyboard as Enter is pressed.
    #[derive(Clone, Copy, Debug)]
    enum Over {
        Bare,
        Typing,
        Menu,
    }

    /// Whether Enter, pressed `over`, opens the line.
    fn enter_opens(over: Over) -> bool {
        let (begun, _) = crate::game::begin::test_game(&[]);
        let mut app = App::new();
        let mut panels = Panels::default();

        if matches!(over, Over::Menu) {
            panels.state.options.open = Some(crate::ui::options::Dialog::Saves);
        }

        app.insert_resource(panels)
            .insert_resource(crate::game::seen::Frame::of(&begun.sim))
            .init_resource::<Chat>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<bevy_egui::EguiUserTextures>()
            .add_systems(Update, keys);

        // SAVE GAME's name box focused: egui wants the keyboard.
        if matches!(over, Over::Typing) {
            let mut egui = bevy_egui::EguiContext::default();

            egui.get_mut()
                .memory_mut(|m| m.request_focus(egui::Id::new("save name")));
            app.world_mut().spawn((egui, bevy_egui::PrimaryEguiContext));
        }

        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Enter);
        app.update();

        app.world().resource::<Chat>().state == State::Opening
    }

    /// Enter opens the chat on the bare map alone: under a menu, or in a
    /// text box (SAVE GAME's name), it is theirs.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn enter_opens_the_chat_on_the_bare_map_alone() {
        assert!(enter_opens(Over::Bare));
        assert!(!enter_opens(Over::Typing));
        assert!(!enter_opens(Over::Menu));
    }

    const MAP: Rect = Rect {
        min: Vec2::new(200.0, 30.0),
        max: Vec2::new(1000.0, 600.0),
    };

    /// Ms to type a short line: its characters, 10 ms each.
    const TYPED: u32 = 100;

    /// "Built" posted at 0 ms, typed, then the box stepped once each
    /// scroll period on `clock`, the real and game ms `elapsed` gives the
    /// period's number: whether a row still shows it.
    fn still_shown(clock: Clock, elapsed: impl Fn(u32) -> (u32, u32)) -> bool {
        let mut board = InfoBox::new(400, 8, rules::SCROLL_MS, 0);
        let step = |b: &mut InfoBox, (real, game): (u32, u32)| {
            let real = std::time::Duration::from_millis(real.into());

            b.step(clock.now(real, game), &|_| 6);
        };
        #[allow(clippy::cast_possible_truncation, reason = "four rows")]
        let periods = rules::ROWS as u32;

        board.post("Built", Rgb { r: 0, g: 216, b: 0 }, 0);
        step(&mut board, (TYPED, TYPED));
        (1..=periods).for_each(|k| step(&mut board, elapsed(k)));

        board.rows().iter().any(|r| !r.is_empty())
    }

    /// On the game clock a line leaves after the scroll periods of game
    /// time, whatever the wall's clock did: paused, it stays.
    #[test]
    fn a_game_clock_line_expires_on_game_time_alone() {
        let period = |k: u32| TYPED + k * rules::SCROLL_MS;

        assert!(still_shown(Clock::Game, |k| (period(k) * 10, TYPED)));
        assert!(!still_shown(Clock::Game, |k| (TYPED, period(k))));
    }

    /// On the real clock, the default, the wall's time carries it off and
    /// the game's does nothing.
    #[test]
    fn a_real_clock_line_expires_on_real_time_alone() {
        let period = |k: u32| TYPED + k * rules::SCROLL_MS;

        assert!(still_shown(Clock::Real, |k| (TYPED, period(k) * 10)));
        assert!(!still_shown(Clock::Real, |k| (period(k), TYPED)));
        assert_eq!(Clock::default(), Clock::Real);
    }

    /// "Built" posted at 0 ms, the box run in 50 ms frames on the real
    /// clock, held where `paused` says: the unpaused ms it shows for, and
    /// whether it showed through every held frame.
    fn real_life(paused: impl Fn(u32) -> Paused) -> (u32, bool) {
        const FRAME_MS: u32 = 50;
        let frame = std::time::Duration::from_millis(FRAME_MS.into());
        let mut chat = Chat::default();
        let mut board = InfoBox::new(400, 8, rules::SCROLL_MS, 0);
        let (mut shown_ms, mut held) = (0, true);

        board.post("Built", Rgb { r: 0, g: 216, b: 0 }, 0);

        for n in 1..1000 {
            let pause = paused(n);
            let real = chat.wall(frame, &pause);

            board.step(Clock::Real.now(real, 0), &|_| 6);

            let shown = board.rows().iter().any(|r| !r.is_empty());

            if pause.0 {
                held &= shown;
            } else if shown {
                shown_ms += FRAME_MS;
            } else {
                break;
            }
        }

        (shown_ms, held)
    }

    /// Paused longer than a scroll period, the real clock's box waits:
    /// the line stays, and leaves after the rest of its time.
    #[test]
    fn a_real_clock_line_waits_out_a_pause() {
        let unpaused = real_life(|_| Paused(false));
        let paused = real_life(|n| Paused((20..=200).contains(&n)));

        assert!(unpaused.0 > rules::SCROLL_MS, "{unpaused:?}");
        assert_eq!(paused, (unpaused.0, true));
    }

    /// The bottom of the box's last row.
    #[allow(clippy::cast_precision_loss, reason = "four rows")]
    fn bottom(p: &Placed) -> f32 {
        p.at.y + rules::ROWS as f32 * p.pitch
    }

    /// An open spell bar over the map's bottom: the box stands above it.
    #[test]
    fn the_box_stands_above_an_open_spell_bar() {
        let spells = Rect::new(200.0, 540.0, 1000.0, 600.0);
        let p = placed(MAP, &[spells], 1.0, Links::Off);

        assert!(bottom(&p) <= spells.min.y, "{p:?}");
    }

    /// A menu over the whole window does not move it.
    #[test]
    fn a_menu_over_the_window_does_not_move_the_box() {
        let menu = Rect::new(0.0, 0.0, 1000.0, 600.0);

        assert_eq!(
            placed(MAP, &[menu], 1.0, Links::Off),
            placed(MAP, &[], 1.0, Links::Off)
        );
    }

    /// Its text grows with the interface.
    #[test]
    fn the_box_text_scales_with_the_window() {
        let (one, two) = (
            placed(MAP, &[], 1.0, Links::Off),
            placed(MAP, &[], 2.0, Links::Off),
        );

        assert!((two.font - 2.0 * one.font).abs() < 0.01, "{one:?} {two:?}");
        assert!(
            (two.pitch - 2.0 * one.pitch).abs() < 0.01,
            "{one:?} {two:?}"
        );
    }

    /// A session that bars cheats says a phrase as typed and sends no
    /// cheat: `FILL THIS BAG` adds gold only where they are allowed.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_session_barring_cheats_sends_none() {
        const PHRASE: &str = "fill this bag";

        let cli = crate::cli::tests::parsed(["openregent"]);
        let hd = crate::files::env::path(crate::files::env::HD).expect("MAJESTY_HD");
        let corpus =
            Corpus::open(&crate::game::load::archives(&hd).expect("the dataset")).expect("corpus");
        let gained = |cheats| {
            let mut sim = crate::game::begin::test_setup(&cli).0.sim;
            let local = sim.local();

            sim.set_cheats(cheats);

            let gold = sim.world().treasury(local);
            let mut outbox = Outbox::default();
            let said = send(
                &corpus,
                &Selected(None),
                (sim.world(), local, &mut outbox),
                PHRASE,
            );

            for ask in outbox.0.drain(..) {
                ask.hand(&mut sim);
            }
            sim.step();
            (said, gold != sim.world().treasury(local))
        };

        let (said, gained_barred) = gained(Cheats::Barred);

        assert_eq!(said.as_deref(), Some(PHRASE));
        assert!(!gained_barred);
        assert!(gained(Cheats::Allowed).1);
    }
}
