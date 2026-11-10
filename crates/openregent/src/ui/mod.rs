//! The interface around the map: the left column (minimap, selection,
//! page, mini view), the top and bottom bars and the spell bar, in the
//! original's art ([`crate::panel`]), scaled as [`crate::ui::layout`] says.
//! The map has the rest of the window.
//!
//! ```text
//!  ┌──────────┬──────────────────────────────────────┐
//!  │ minimap  │ 20015                        0 Days  │
//!  ├──────────┤                                      │
//!  │selection │                                      │
//!  ├──────────┤               map                    │
//!  │  page    │                                      │
//!  ├──────────┤ [spell bar]                          │
//!  │mini view │ SPELLS  messages             OPTIONS │
//!  └──────────┴──────────────────────────────────────┘
//! ```

mod bar;
pub(crate) mod chat;
pub(crate) mod editor;
pub(crate) mod end;
pub(crate) mod figure;
pub(crate) mod frame_rate;
pub(crate) mod held;
pub(crate) mod layout;
pub(crate) mod lore;
pub(crate) mod minimap;
pub(crate) mod options;
pub(crate) mod players;
pub(crate) mod scroll;
pub(crate) mod stopped;
pub(crate) mod tracking;
pub(crate) mod typeface;

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::egui;
use egui::{LayerId, Ui, UiBuilder};

use regent_core::data::fourcc::FourCc;
use regent_core::data::qdd;
use regent_core::data::text::{Ref, Text, printf};
use regent_core::sim::world::offer::SpellOffer;
use regent_core::sim::world::{self, World};
use regent_core::view;
use regent_core::view::map::zoom::REGULAR;
use regent_core::view::menu::Tick;
use regent_core::view::menu::quest_map;
use regent_core::view::menu::saves::{Players, SAVED_TEXT};
use regent_core::view::panel::day::Day;
use regent_core::view::panel::spell_bar;
use regent_draw::data::Corpus;
use regent_draw::stage::{self, View};

use crate::files::config::{self, HpBars, Names, Settings};
use crate::files::profile::Player;
use crate::game::seen::{Ask, News, Outbox, Query, Watch};
use crate::game::sim::Session;
use crate::game::thread::SimThread;
use crate::input::orders;
use crate::map::cursor::Tipped;
use crate::map::select::{Reading, Selected};
use crate::map::zoom;
use crate::menu::fit::Fit;
use crate::menu::{Quests, Return, Returning, Screen};
use crate::panel::canvas::{Align, Canvas};
use crate::panel::message::Message;
use crate::panel::upper::{self, Chosen, Game};
use crate::panel::{Page, Panels, bars, lower};
use crate::save;
use crate::save::dialog::{Chose, Saves};
use crate::save::thread::{Done, Job, Kind, Place, SaveThread};
use crate::ui::editor::{Editor, Pass};
use crate::ui::layout::{Frame, Grid};
use crate::ui::options::{Asked, Dialog, Leave, Play, Seat};

pub use bar::bar;

/// The treasury's text: `GMTX` 2, `%d gold`, the headless log's. HD's top
/// bar shows the bare number, control 2006, which
/// `MainFrameController` sets from the local player.
const GOLD_TEXT: Ref = Ref::gmtx(2);

/// The game's own text while paused: `GMTX` 292, "Game Paused".
const PAUSED_TEXT: Ref = Ref::gmtx(292);

/// The pause line's start when other LAN players hold the game: no
/// game text says so.
const PAUSED_BY: &str = "Game paused by";

/// The column's slots, HD pixels: selection and page.
const SELECTION_AT: egui::Pos2 = egui::pos2(0.0, 182.0);
const PAGE_AT: egui::Pos2 = egui::pos2(0.0, 428.0);

/// The minimap's place in the column (`APMK` 2001).
const MINIMAP_AT: egui::Pos2 = egui::pos2(4.0, 3.0);

/// The tracking window's caption (`APMK` 4467).
const VIEW_LABEL: egui::Rect = egui::Rect {
    min: egui::pos2(6.0, 804.0),
    max: egui::pos2(196.0, 818.0),
};

/// What the bar draws with.
#[derive(bevy::ecs::system::SystemParam)]
pub struct Interface<'w> {
    pub panels: ResMut<'w, Panels>,
    /// The art service, the menus' too.
    pub art: ResMut<'w, regent_draw::art::Art>,
    pub covered: ResMut<'w, Covered>,
    pub paused: ResMut<'w, Paused>,
    pub settings: ResMut<'w, Settings>,
    /// The names check's last shown setting.
    pub kept: ResMut<'w, crate::map::labels::LastNames>,
    pub editor: ResMut<'w, Editor>,
    pub selected: ResMut<'w, Selected>,
    pub reading: ResMut<'w, Reading>,
    /// What the map's tooltip tells of.
    pub tipped: Res<'w, Tipped>,
    /// Interface sounds: the message scroll's CLOSE.
    pub sounds: ResMut<'w, crate::audio::Interface>,
    /// The settings editor's and save dialogs' clicks, as the menus'.
    pub clicks: ResMut<'w, crate::audio::ClickSounds>,
    /// Where a quit from the Options menu goes, and a load is made.
    pub returning: ResMut<'w, Returning>,
    /// The menus' art, which the save dialogs and the settings editor
    /// draw in, and the quests' pogs.
    pub menu_art: ResMut<'w, crate::menu::Art>,
    pub quests: Res<'w, Quests>,
    pub saves: ResMut<'w, Saves>,
    /// The profile the game saves to.
    pub player: Res<'w, Player>,
    pub screen: ResMut<'w, NextState<Screen>>,
    pub exit: MessageWriter<'w, AppExit>,
    /// A save dialog's choice, for [`keep`].
    pub saving: ResMut<'w, Saving>,
    /// The tracking window, and the image its camera draws.
    pub tracking: ResMut<'w, crate::ui::tracking::Tracking>,
    pub picture: Res<'w, crate::ui::tracking::Picture>,
    /// The portrait under the pointer, for the cursor.
    pub portrait: ResMut<'w, crate::map::cursor::Portrait>,
}

/// A save dialog's choice this frame, carried out by [`keep`].
#[derive(Resource, Default)]
pub struct Saving(Option<Chose>);

/// A value kept while its key holds: panel data read once a game frame,
/// not each drawn frame.
pub struct Memo<K, T>(Option<(K, T)>);

impl<K, T> Default for Memo<K, T> {
    fn default() -> Self {
        Memo(None)
    }
}

impl<K: PartialEq, T> Memo<K, T> {
    /// The value kept for `key`, made again once the key changed.
    pub fn get(&mut self, key: K, make: impl FnOnce() -> T) -> &T {
        if self.0.as_ref().is_some_and(|(k, _)| *k != key) {
            self.0 = None;
        }

        &self.0.get_or_insert_with(|| (key, make())).1
    }
}

/// The bar's panel data, a game's, kept by game frame ([`Memo`]): it
/// changes no faster, and at 60 Hz the bar draws three times a frame.
#[derive(Resource, Default)]
pub struct BarMemo {
    /// The selection's panel, by frame and agent.
    chosen: Memo<(u32, u32), Chosen>,
    /// The local player's spells, by frame.
    spells: Memo<u32, Vec<SpellOffer>>,
}

/// Whether the selection's panel shows above the page: not under a
/// message, nor beside a palace's whole-column page.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Upper {
    Shown,
    Covered,
}

impl Upper {
    fn of(page: Page, chosen: &Chosen, message: Option<&Message>) -> Upper {
        if message.is_some() || (page.whole() && chosen.palace()) {
            Upper::Covered
        } else {
            Upper::Shown
        }
    }
}

/// The interface run `chosen`'s panel asks, only while its answer can
/// show: an owned palace's henchmen and a rogues' guild's extortion on
/// the panel, a Hall's bounties and a Mausoleum's dead on their pages.
/// Each run copies the game.
fn query(chosen: &Chosen, page: Page, upper: Upper) -> Option<Query> {
    let agent = chosen.agent;
    let panel = upper == Upper::Shown;

    if chosen.palace() {
        (panel && chosen.owned).then_some(Query::Henchmen(agent))
    } else if chosen.rogues() {
        panel.then_some(Query::Extortion(agent))
    } else if chosen.hall() {
        (page == Page::Bounty).then_some(Query::Bounties(agent))
    } else {
        (page == Page::Crypt).then_some(Query::Crypt(agent))
    }
}

/// A save dialog's choice carried out (`saved`): the one place the
/// interface asks the sim thread for the whole game
/// ([`SimThread::save`]). A LAN game's SAVE is asked of every peer
/// instead, and each LAN save a step took is queued here (`lan_saved`).
/// Every save is made and written on the save thread; what it finished
/// is told here, next frames (`told`).
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
pub fn keep(
    (thread, session, mut writer): (Res<SimThread>, Res<Session>, ResMut<SaveThread>),
    corpus: Res<Corpus>,
    (view, orders): (Res<View>, Res<orders::Orders>),
    minimap: minimap::Parts,
    (seen, mut outbox, mut news): (Watch, ResMut<Outbox>, ResMut<News>),
    mut face: Interface,
) {
    let context = (&*session, &*corpus, &minimap, &*view);

    match (face.saving.0.take(), Play::of(seen.players)) {
        (Some(Chose::Save { name, .. }), Play::Lan) => {
            outbox.push(Ask::Save(name.clone()));
            face.saves.named = Some(name);
            face.saves.close();
            face.panels.state.options.close(&mut face.paused.0);
        }
        (Some(chose), _) => saved(chose, &mut face, (&thread, &orders, &mut writer), context),
        (None, _) => {}
    }

    for stepped in std::mem::take(&mut news.saves) {
        let local = seen.local;

        if let Err(line) = lan_saved(stepped, local, (&face, &mut writer), context) {
            news.chat.push(line);
        }
    }

    // The setting to the game, each frame as the reward panel's; the
    // autosaves it took written aside.
    outbox.push(Ask::Autosave(face.settings.game.autosave.days()));

    let place = crate::menu::saves_place(&face.player, Play::of(seen.players));

    for taken in std::mem::take(&mut news.autosaves) {
        if let Some((dir, _)) = &place {
            let name = save::auto::name(taken.day);
            let to = (Place::Autosave(dir.clone()), Kind::Autosave);

            writer.send(job(taken.game, (name, to), &face, context));
        } else {
            warn!("autosave: {NO_PROFILE}");
        }
    }

    for done in writer.done() {
        told(done, &mut face, (&mut news, seen.world().now()));
    }
}

/// A save of `game` named `name`, to write to `place` with this
/// machine's view, selection, minimap and profile.
fn job(
    game: save::FromSim,
    (name, (place, kind)): (String, (Place, Kind)),
    face: &Interface,
    (session, corpus, minimap, view): (&Session, &Corpus, &minimap::Parts, &View),
) -> Job {
    Job {
        game,
        described: save::described(session, &corpus.text),
        minimap: minimap.picture().cloned(),
        interface: interface_of(face, view),
        name,
        profile: profile_of(face),
        place,
        kind,
    }
}

/// A LAN game's save a step took, queued for this machine's profile: as
/// named where this machine asked it, over a save of that name, the
/// dialog having asked; else under a name not taken as it is written.
/// The chat line, as the asker's, where it goes nowhere.
fn lan_saved(
    stepped: save::Stepped,
    local: u32,
    (face, writer): (&Interface, &mut SaveThread),
    context: (&Session, &Corpus, &minimap::Parts, &View),
) -> Result<(), (String, String)> {
    let save::Stepped {
        seat,
        nick,
        name,
        game,
    } = stepped;

    match lan_place(&face.player, (seat, local), &name) {
        Ok(place) => {
            let kind = Kind::Lan { nick };

            writer.send(job(game, (name, (place, kind)), face, context));
            Ok(())
        }
        Err(why) => {
            warn!("LAN save {name}: {why}");
            Err((nick, format!("{NOT_SAVED} {name}: {why}")))
        }
    }
}

/// A save finished on the save thread, told: SAVE GAME's "Game Saved."
/// in the chat, or the save dialog again saying why, as before it wrote
/// in the frame; a LAN save's chat line, as the asker's. At game time
/// `now`.
fn told(done: Done, face: &mut Interface, (news, now): (&mut News, u32)) {
    let Done {
        kind,
        name,
        written,
    } = done;

    match (kind, written) {
        (Kind::Asked, Ok(_)) => news.said.push(world::chat::Said {
            at: now,
            to: None,
            line: world::chat::Line::Text(SAVED_TEXT.index),
            from: None,
        }),
        (Kind::Asked, Err(why)) => unsaved(face, name, why, &mut news.chat),
        (Kind::Lan { nick }, Ok((_, name))) => news.chat.push((nick, format!("{SAVED} {name}"))),
        (Kind::Lan { nick }, Err(why)) => {
            news.chat.push((nick, format!("{NOT_SAVED} {name}: {why}")));
        }
        (Kind::Autosave, _) => {}
    }
}

/// SAVE GAME's save `name` not written, for `why`: the save dialog open
/// again on it, saying so. Under the settings editor, which the dialog
/// would replace, the chat says it instead.
fn unsaved(face: &mut Interface, name: String, why: String, chat: &mut Vec<(String, String)>) {
    if face.panels.state.options.open == Some(Dialog::Settings) {
        chat.push((NOT_SAVED_HERE.to_owned(), format!("{name}: {why}")));
    } else {
        if face.saves.open.is_none() {
            face.saves.open = Some(save::dialog::Dialog::Save);
            face.saves.name = name;
            face.saves.fill();
        }

        face.saves.said = Some(why);
        face.panels
            .state
            .options
            .show(Dialog::Saves, &mut face.paused.0);
    }
}

/// What the chat says of a LAN save: placeholders.
const SAVED: &str = "saved the game:";
const NOT_SAVED: &str = "saved the game, not here:";
/// What the chat says of SAVE GAME's save not written: a placeholder.
const NOT_SAVED_HERE: &str = "Game not saved";
const NO_PROFILE: &str = "no profile";

/// The menus over the game, the settings editor and the save dialogs
/// while open, and all that lies over the map: `over`, and the whole
/// window under a menu. What a save dialog's player chose. `game` is
/// what a save says of the game ([`saved_game`]); `quest` is the game's,
/// which a restart goes back to; `play` whether it is a LAN game's,
/// which neither loads nor restarts, and `seat` whether its player may
/// still surrender. Whether the player surrendered this pass.
fn covered(
    ctx: &egui::Context,
    (frame, corpus): (&Frame, &Corpus),
    face: &mut Interface,
    mut over: Vec<egui::Rect>,
    (goals, game): (&str, (save::dialog::Here, Option<String>)),
    (quest, play, seat): (Option<FourCc>, Play, Seat),
) -> (Option<Chose>, Surrendered) {
    let mut surrendered = Surrendered::No;
    let shown = face
        .panels
        .state
        .options
        .open
        .and_then(options::panel)
        .and_then(|id| face.panels.layout(&corpus.catalogue, &corpus.text, id));

    if let Some(layout) = shown {
        let mut ui = Ui::new(
            ctx.clone(),
            "options".into(),
            UiBuilder::new()
                .layer_id(LayerId::new(egui::Order::Foreground, "options".into()))
                .max_rect(ctx.viewport_rect()),
        );
        let Panels { ink, state, .. } = &mut *face.panels;
        let mut canvas = Canvas {
            ui: &mut ui,
            ink,
            art: &mut face.art,
            source: corpus.source(),
            catalogue: &corpus.catalogue,
            fit: options::centred(frame, &layout, ctx.viewport_rect()),
            salt: "options",
        };
        let held = (&mut state.options, &mut face.paused.0);

        match options::draw(&mut canvas, &layout, held, (goals, play, seat)) {
            Some(Asked::Settings) => face.editor.open(&face.settings),
            Some(Asked::Save) => {
                show_saves(
                    &mut face.saves,
                    &face.player,
                    save::dialog::Dialog::Save,
                    (game, &corpus.text),
                );
                face.saves.pog = face.quests.pog_pic(&face.menu_art, quest);
            }
            Some(Asked::Load) => {
                show_saves(
                    &mut face.saves,
                    &face.player,
                    save::dialog::Dialog::Load,
                    (game, &corpus.text),
                );
                face.saves.pog = face.quests.pog_pic(&face.menu_art, quest);
            }
            Some(Asked::Leave(Leave::Surrender)) => {
                face.panels.state.options.close(&mut face.paused.0);
                surrendered = Surrendered::Yes;
            }
            Some(Asked::Leave(leave)) => quit(face, leave, quest),
            None => {}
        }
    }

    let (chose, wave) = save_dialog(ctx, (&mut face.menu_art, &mut face.art), &mut face.saves);

    face.clicks.0.extend(wave);

    if chose == Some(Chose::Cancel) {
        face.saves.close();
        face.panels.state.options.open = Some(Dialog::Options);
    }

    // The editor closed goes back to the Options menu, as AP37's OPTIONS.
    let art = (&mut *face.menu_art, &mut *face.art);

    if let Some(closed) = face.editor.show(ctx, art, &mut face.settings) {
        over.push(ctx.viewport_rect());

        if let Some(wave) = crate::menu::settings::click(ctx, &face.menu_art) {
            face.clicks.0.push(wave);
        }

        if closed != Pass::Open && face.panels.state.options.open == Some(Dialog::Settings) {
            face.panels.state.options.open = Some(Dialog::Options);
        }
    }

    if face.panels.state.options.open.is_some() {
        over.push(ctx.viewport_rect());
    }

    face.covered.0 = over
        .into_iter()
        .map(|r| Rect::new(r.min.x, r.min.y, r.max.x, r.max.y))
        .collect();

    (chose.filter(|c| *c != Chose::Cancel), surrendered)
}

/// Whether the player confirmed a surrender ([`covered`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Surrendered {
    No,
    Yes,
}

/// The open save or load dialog over the game, in the menus' art, and
/// the click a press in it made this pass (`Art::clicks`), taken: no
/// menu plays it in game (`menu::show`), so left it would play in the
/// menus.
fn save_dialog(
    ctx: &egui::Context,
    (art, shared): (&mut crate::menu::Art, &mut regent_draw::art::Art),
    saves: &mut Saves,
) -> (Option<Chose>, Option<&'static [u8]>) {
    let chose = crate::menu::save_page::page(ctx, (art, shared), saves);
    let click = crate::menu::click(ctx).and_then(|wave| art.clicks.get(&wave).copied());

    (chose, click)
}

/// A save dialog's choice carried out: the game taken and queued to
/// write, the menus closed (**Inferred**: SAVE GAME closes `AP49` and
/// opens nothing, `SaveGameController`); or the save loaded, by the
/// menus, this game taken down. A game that makes no save keeps the
/// dialog open, saying why.
fn saved(
    chose: Chose,
    face: &mut Interface,
    (thread, orders, writer): (&SimThread, &orders::Orders, &mut SaveThread),
    context: (&Session, &Corpus, &minimap::Parts, &View),
) {
    match chose {
        Chose::Save { path, name } => {
            // The panel's settings, the game's to save.
            let taken = thread
                .save(orders.rewards)
                .and_then(|game| game.ready().map(|()| game));

            match taken {
                Ok(game) => {
                    let to = (Place::File(path), Kind::Asked);

                    writer.send(job(game, (name.clone(), to), face, context));
                    face.saves.named = Some(name);
                    face.saves.close();
                    face.panels.state.options.close(&mut face.paused.0);
                }
                Err(why) => {
                    warn!("{why}");
                    face.saves.said = Some(why);
                }
            }
        }
        Chose::Load(path) => {
            let name =
                save::glance(&path).map_or_else(|_| path.display().to_string(), |g| g.meta.name);
            let place = face.saves.place.clone();

            face.saves.close();
            face.panels.state.options.close(&mut face.paused.0);
            face.returning.0 = Some(Return::Load { path, name, place });
            face.screen.set(Screen::Menu);
        }
        Chose::Cancel => {}
    }
}

/// Where a LAN save named `name`, asked by seat `seat`, goes in
/// `player`'s LAN saves, on the machine of seat `local`: the asker's
/// over a save of that name, the others' under a name not taken.
fn lan_place(player: &Player, (seat, local): (u32, u32), name: &str) -> Result<Place, String> {
    crate::menu::saves_place(player, Play::Lan)
        .ok_or_else(|| NO_PROFILE.to_owned())
        .and_then(|(dir, _)| {
            if seat == local {
                save::path(&dir, name).map(Place::File)
            } else {
                Ok(Place::Free(dir))
            }
        })
}

/// The profile's name a save names.
fn profile_of(face: &Interface) -> String {
    face.player
        .profile
        .as_ref()
        .map(|p| p.name.clone())
        .unwrap_or_default()
}

/// This machine's selection and view, as a save keeps them.
fn interface_of(face: &Interface, view: &View) -> save::Interface {
    save::Interface {
        selected: face.selected.0,
        reading: face.reading.0,
        pan: (view.pan.x, view.pan.y),
        zoom: view.zoom,
        tracking: face.tracking.mini.unwrap_or_default(),
    }
}

/// A quit the Options menu confirmed: to the menus as the end panel's
/// Main Menu goes (`end::panel`), to the start page of `quest`'s game
/// (`Flows::restart`), or out.
fn quit(face: &mut Interface, leave: Leave, quest: Option<FourCc>) {
    match leave {
        Leave::Menus => {
            face.returning.0 = Some(Return::Main);
            face.screen.set(Screen::Menu);
        }
        Leave::Restart => {
            face.returning.0 = Some(Return::Restart(quest));
            face.screen.set(Screen::Menu);
        }
        Leave::System => {
            face.exit.write(AppExit::Success);
        }
        Leave::Surrender => {}
    }
}

/// A minimap button's press: the hit point
/// bars on or off (display flag bit 2, `[ui] bars` here); the names off,
/// or back to what last showed (bit 0, `[ui] names`); the zoom
/// toggled; the crown's palace selected, with its sound, any mode left,
/// the view not moved.
fn column_press(
    pressed: Option<bars::Column>,
    face: &mut Interface,
    (world, local, panel, view): (&World, u32, &mut orders::Panel, &mut View),
) {
    match pressed {
        Some(bars::Column::Bars) => {
            face.settings.ui.bars = match face.settings.ui.bars {
                HpBars::Never => HpBars::default(),
                _ => HpBars::Never,
            };
        }
        Some(bars::Column::Names) => {
            let now = face.settings.ui.names;

            face.settings.ui.names = now.toggled(face.kept.0);

            if now != Names::Off {
                face.kept.0 = now;
            }
        }
        Some(bars::Column::Zoom) => zoom::toggle(view),
        Some(bars::Column::Palace) => {
            if let Some(palace) = world.player_palace(local) {
                face.selected.0 = Some(palace);
                face.reading.0 = None;
            }

            panel.orders.mode = orders::Mode::Idle;
        }
        None => {}
    }
}

/// The menu OPTIONS or the title scroll opens over the paused game; the
/// goals' text while they show, a custom game's from `goals`
/// (`Maj.qdd`'s).
fn opened(
    face: &mut Interface,
    pressed: bars::Pressed,
    named: Option<view::panel::scroll::ScrollSubject<'_>>,
    (text, goals): (&Text, &[qdd::Quest]),
) -> String {
    let opened = match (pressed.options, pressed.scroll) {
        (true, _) => Some(Dialog::Options),
        (false, true) => Some(Dialog::Goals),
        (false, false) => None,
    };

    if let Some(dialog) = opened {
        face.panels.state.options.show(dialog, &mut face.paused.0);
    }

    named
        .filter(|_| face.panels.state.options.open == Some(Dialog::Goals))
        .and_then(|g| view::panel::scroll::goals(g, goals, text))
        .unwrap_or_default()
}

/// The hit point bars', names' and zoom's checks: ticked while the bars
/// show, while names show, and while the view is zoomed out.
fn ticks(settings: &Settings, view: &View) -> (Tick, Tick, Tick) {
    let tick = |on: bool| if on { Tick::Checked } else { Tick::Clear };

    (
        tick(settings.ui.bars != HpBars::Never),
        tick(settings.ui.names != Names::Off),
        tick(view.zoom < REGULAR),
    )
}

/// What the title scroll names: the quest, else the custom game's setup.
fn scroll_game(session: &Session) -> Option<view::panel::scroll::ScrollSubject<'_>> {
    match (&session.quest, &session.setup) {
        (Some((_, title)), _) => Some(view::panel::scroll::ScrollSubject::Quest {
            title,
            short: session.goals.as_deref().unwrap_or_default(),
        }),
        (None, Some(setup)) => Some(view::panel::scroll::ScrollSubject::Custom(setup)),
        (None, None) => None,
    }
}

/// The game a save is named for: the quest's title, else a custom
/// game's `GMTX` 80 "Freestyle Game", as the original names its saves.
fn played(session: &Session, text: &Text<'_>) -> Option<String> {
    match (&session.quest, &session.setup) {
        (Some((_, title)), _) => Some(title.clone()),
        (None, Some(_)) => text
            .gmtx(quest_map::FREESTYLE_BANNER.title)
            .map(|t| t.trim().to_owned()),
        (None, None) => None,
    }
}

/// What a save says of the game of `players` peers at `clock`, ms: the
/// game's title ([`played`]) and clock, for CURRENT GAME DESCRIPTION, as
/// the original's quest title; and the game its name says.
pub(crate) fn saved_game(
    session: &Session,
    text: &Text<'_>,
    (clock, players): (u32, u32),
) -> (save::dialog::Here, Option<String>) {
    let game = played(session, text);
    let here = save::dialog::Here {
        title: game.clone().unwrap_or_default(),
        clock,
        players: match Play::of(players) {
            Play::Alone => Players::Alone,
            Play::Lan => Players::Lan(usize::try_from(players).unwrap_or_default()),
        },
    };

    (here, game)
}

/// `dialog` opened over the game on `player`'s saves, as `AP36`'s SAVE
/// GAME and LOAD GAME open it: the save dialog offering a name for
/// `game` ([`saved_game`]).
pub(crate) fn show_saves(
    saves: &mut Saves,
    player: &Player,
    dialog: save::dialog::Dialog,
    ((here, game), text): ((save::dialog::Here, Option<String>), &Text<'_>),
) {
    let over = here.players;
    let play = match over {
        Players::Alone => Play::Alone,
        Players::Lan(_) => Play::Lan,
    };
    let place = crate::menu::saves_place(player, play);

    match dialog {
        save::dialog::Dialog::Save => {
            let day = Day::of(here.clock).text(text).unwrap_or_default();
            let name = save_name(game.as_deref(), &day);

            saves.show(dialog, place, name, (Some(here), over));
        }
        save::dialog::Dialog::Load => saves.show(dialog, place, String::new(), (None, over)),
    }
}

/// A save's offered name: "(The Siege) 0 Days" (`saves::offered`), or
/// the day alone.
fn save_name(game: Option<&str>, day: &str) -> String {
    match game {
        Some(game) => regent_core::view::menu::saves::offered(game, day),
        None => day.to_owned(),
    }
}

/// The map's room: the world camera's viewport and the view's rect.
fn give_map(frame: &Frame, (camera, window): (&mut Camera, &Window), view: &mut View) {
    let map = frame.map;

    stage::viewport(
        camera,
        window,
        [map.min.x, 0.0, map.min.y, frame.bottom.max.y - map.max.y],
    );

    // In logical points, as `Window::cursor_position`.
    view.rect = Some(Rect::new(map.min.x, map.min.y, map.max.x, map.max.y));
}

/// The pause line: the other LAN players holding the game (`others`),
/// else this machine's pause (`local`), `GMTX` 292; none, unpaused.
fn pause_line((local, others): (bool, &[String]), text: &Text) -> Option<String> {
    if others.is_empty() {
        local
            .then(|| text.get(PAUSED_TEXT).map(std::borrow::Cow::into_owned))
            .flatten()
    } else {
        Some(format!("{PAUSED_BY} {}", others.join(", ")))
    }
}

/// Whether a LAN game holds for a silent player (`held`): the title
/// scroll says so.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Held {
    Playing,
    Waiting,
}

impl Held {
    fn of(seen: &Watch) -> Held {
        if seen.waiting.is_empty() {
            Held::Playing
        } else {
            Held::Waiting
        }
    }
}

/// The top and bottom bars, the title scroll naming `named` (while a
/// LAN game holds, `view::panel::scroll::WAITING`), and, open,
/// the spell bar: what was pressed.
fn around(
    ui: &mut Ui,
    (panels, (art, source), frame, main): (
        &mut Panels,
        (&mut regent_draw::art::Art, regent_draw::art::ArtSource),
        &Frame,
        Option<&regent_core::view::menu::Layout>,
    ),
    (game, paused, named, held, rage): (
        &Game<'_>,
        Option<String>,
        Option<view::panel::scroll::ScrollSubject<'_>>,
        Held,
        Option<u32>,
    ),
    (orders, spells): (&mut orders::Panel, &mut Memo<u32, Vec<SpellOffer>>),
) -> bars::Pressed {
    let catalogue = &game.corpus.catalogue;
    let text = &game.corpus.text;
    let gold = game.world.treasury(game.local);
    let day = Day::of(game.world.now());
    let mut pressed = bars::Pressed::default();

    if let Some(main) = main {
        let Panels { ink, state, .. } = &mut *panels;
        let mut canvas = Canvas {
            ui,
            ink,
            art: &mut *art,
            source,
            catalogue,
            fit: bars::WINDOW,
            salt: "bars",
        };

        bars::bars(&mut canvas, main, frame);
        let over = state.frames.text();
        let clicked = bars::labels(&mut canvas, main, frame, (gold, (day, over)), text);

        if let Some(line) = &paused {
            bars::pause_line(&mut canvas, main, frame, line);
        }

        if clicked {
            state.frames.click();
        }

        let title = match held {
            Held::Waiting => text
                .gmtx(view::panel::scroll::WAITING)
                .map(|t| t.trim().to_owned()),
            Held::Playing => named.and_then(|g| view::panel::scroll::title(g, text)),
        };

        pressed = bars::buttons(&mut canvas, main, frame, &title.unwrap_or_default());
        state.spells_closed ^= pressed.spells;
    }

    let layout = panels.layout(catalogue, text, bars::SPELL_BAR);
    let buttons = spell_bar::shown(spell_bar::release(catalogue));
    let grid = spell_grid(frame, buttons, rage);

    if let Some(layout) = layout.filter(|_| frame.spells.height() > 0.0) {
        let mut canvas = Canvas {
            ui,
            ink: &mut panels.ink,
            art,
            source,
            catalogue,
            fit: Fit {
                scale: frame.scale,
                origin: frame.spells.min,
            },
            salt: "spell bar",
        };
        let width = frame.spells.width() / frame.scale;
        let world = game.world;
        let offers = spells.get(game.frame, || world.spells(game.program, game.local));

        bars::spell_bar(
            &mut canvas,
            (&layout, &grid, width),
            buttons,
            (world, game.local, offers, game.corpus),
            orders,
        );

        if let (Some(temple), Some(cell)) = (rage, grid.cell(buttons.len())) {
            bars::rage(&mut canvas, cell, (game, temple), orders);
        }
    }

    pressed
}

/// What the open spell bar covers of the map, so a click there is not
/// the map's.
fn spells_over(frame: &Frame) -> Vec<egui::Rect> {
    [frame.spells]
        .into_iter()
        .filter(|r| r.height() > 0.0)
        .collect()
}

/// The spell bar's buttons' places across `frame`'s map, `rage`'s Rage
/// last.
fn spell_grid(frame: &Frame, buttons: &[spell_bar::Button], rage: Option<u32>) -> Grid {
    layout::spell_grid(&bars::groups(buttons, rage), frame.map.width(), frame.scale)
}

/// The column's fit: HD pixel `at` of the column on the window.
fn column_fit(frame: &Frame, at: egui::Pos2) -> Fit {
    Fit {
        scale: frame.scale,
        origin: frame.in_column(at),
    }
}

/// A new selection opens its first page.
fn reset(
    panels: &mut Panels,
    chosen: Option<&Chosen>,
    world: &World,
    program: &regent_core::gpl::load::Program,
) {
    let agent = chosen.map(|c| c.agent);
    let state = &mut panels.state;

    if state.page_of != agent {
        state.page_of = agent;
        state.scroll = 0.0;
        state.row = None;
        state.ask = None;
        state.bet = None;
        state.page = chosen.map_or(crate::panel::Page::Help, |c| {
            c.first_page(world, program, state.hero_page)
        });
    }
}

/// The selection panel and the page under it; what each asks.
fn panels(
    ui: &mut Ui,
    (panels, (art, source)): (
        &mut Panels,
        (&mut regent_draw::art::Art, regent_draw::art::ArtSource),
    ),
    frame: &Frame,
    (game, chosen, message): (&Game<'_>, Option<&Chosen>, Option<&Message>),
    orders: &mut orders::Panel,
) -> (upper::Asked, lower::Asked) {
    let catalogue = &game.corpus.catalogue;
    let mut ink = std::mem::take(&mut panels.ink);
    let mut canvas = Canvas {
        ui,
        ink: &mut ink,
        art,
        source,
        catalogue,
        fit: column_fit(frame, SELECTION_AT),
        salt: "selection",
    };
    // The palace's statistics and rosters take both slots.
    let whole =
        message.is_none() && panels.state.page.whole() && chosen.is_some_and(upper::Chosen::palace);
    let upper = match (message, chosen) {
        (Some(message), _) => message.show(&mut canvas, panels, game),
        (None, Some(_)) if whole => upper::Asked::default(),
        (None, Some(chosen)) if lower::ask(&mut canvas, panels, game, chosen, orders) => {
            upper::Asked::default()
        }
        (None, Some(chosen)) => upper::show(&mut canvas, panels, game, chosen, orders),
        (None, None) => {
            upper::crown(&mut canvas);
            upper::Asked::default()
        }
    };

    canvas.fit = column_fit(frame, if whole { SELECTION_AT } else { PAGE_AT });
    canvas.salt = "page";

    let lower = lower::show(&mut canvas, panels, game, chosen, orders);

    panels.ink = ink;
    (upper, lower)
}

/// The tracking window: its picture, and the followed piece's name
/// under it, `named`.
fn mini_view(
    ui: &mut Ui,
    frame: &Frame,
    (panels, (art, source)): (
        &mut Panels,
        (&mut regent_draw::art::Art, regent_draw::art::ArtSource),
    ),
    corpus: &Corpus,
    (tracking, picture, named): (
        &mut crate::ui::tracking::Tracking,
        &crate::ui::tracking::Picture,
        Option<String>,
    ),
) {
    let picture_rect = egui::Rect::from_min_max(
        frame.in_column(crate::ui::tracking::PICTURE.min),
        frame.in_column(crate::ui::tracking::PICTURE.max),
    );

    crate::ui::tracking::show(ui, picture_rect, (tracking, picture));

    if let Some(named) = named {
        let Panels { ink, .. } = panels;
        let mut canvas = Canvas {
            ui,
            ink,
            art,
            source,
            catalogue: &corpus.catalogue,
            fit: column_fit(frame, egui::Pos2::ZERO),
            salt: "mini view",
        };

        canvas.text(
            regent_core::data::fourcc::FourCc(*b"fnt7"),
            VIEW_LABEL,
            &named,
            (egui::Color32::WHITE, Align::Centre),
        );
    }
}

/// `gold` in [`GOLD_TEXT`]: "30000 gold"; the bare number without the
/// table, `?` while the world does not know it.
pub fn gold_text(gold: Option<i32>, text: &Text) -> String {
    let number = gold.map_or_else(|| "?".to_owned(), |g| g.to_string());

    text.get(GOLD_TEXT)
        .map_or_else(|| number.clone(), |f| printf(&f, &[(&number).into()]))
}

/// Mute or unmute the sound, kept in the settings file.
pub fn toggle_mute(settings: &mut Settings) {
    settings.audio.muted = !settings.audio.muted;

    if let Err(e) = config::save(settings) {
        warn!("settings not saved: {e}");
    }
}

/// M toggles the mute, where the game's keys are live.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
pub fn mute_key(
    keys: Res<ButtonInput<KeyCode>>,
    mut live: crate::input::map_keys::Live,
    mut settings: ResMut<Settings>,
) {
    if keys.just_pressed(KeyCode::KeyM) && live.keys() {
        toggle_mute(&mut settings);
    }
}

/// What egui draws over the map, in logical points: the spell bar and
/// the options.
#[derive(Resource, Default)]
pub struct Covered(pub Vec<Rect>);

/// Whether the game is paused: no frames step, so game time and the art
/// on it stop; drawing and selection go on.
#[derive(Resource, Default)]
pub struct Paused(pub bool);

/// The keys that toggle the pause.
const PAUSE_KEYS: [KeyCode; 2] = [KeyCode::Space, KeyCode::Pause];

/// Space or Pause toggles the pause, but under a menu. Space is a
/// departure: the original's attack flag (`World::space`).
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
pub fn pause_key(keys: Res<ButtonInput<KeyCode>>, panels: Res<Panels>, mut paused: ResMut<Paused>) {
    // A menu holds the pause until it closes.
    if keys.any_just_pressed(PAUSE_KEYS) && panels.state.options.open.is_none() {
        paused.0 = !paused.0;
    }
}

/// Whether `at`, in logical points, is on the map: not the column or a
/// bar, nor what lies over the map.
pub fn on_map(view: &View, covered: &Covered, at: Vec2) -> bool {
    let over = covered.0.iter().any(|r| r.contains(at));

    view.rect.is_some_and(|r| r.contains(at)) && !over
}

/// Whether the cursor is on the map ([`on_map`]): where a drag pans.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
pub fn over_map(
    view: Res<View>,
    covered: Res<Covered>,
    window: Single<&Window, With<PrimaryWindow>>,
) -> bool {
    window
        .cursor_position()
        .is_some_and(|at| on_map(&view, &covered, at))
}

/// What the interface covers, a game's; whether it is paused, which the
/// game is made with.
pub fn plugin(app: &mut bevy::prelude::App) {
    use crate::game::scope::Scoped as _;

    app.per_game::<Covered>()
        .per_game::<Saving>()
        .per_game::<SaveThread>()
        .per_game::<BarMemo>()
        .made_by_game::<Paused>();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Other players' pauses are named, this machine's or not; this
    /// machine's alone is the game's own line.
    #[test]
    fn the_pause_line_names_the_others_that_paused() {
        let text = Text::default();
        let others = ["Ann".to_owned(), "Bo".to_owned()];

        assert_eq!(
            pause_line((true, &others), &text).as_deref(),
            Some("Game paused by Ann, Bo")
        );
        assert_eq!(pause_line((false, &[]), &text), None);
    }

    /// SAVE GAME and LOAD GAME over a game list its kind's folder alone:
    /// a single-player game's `saves`, a LAN game's `mp-saves`.
    #[test]
    fn a_game_save_dialog_lists_only_its_kinds_folder() {
        let (_dir, player) = save::dialog::two_folders("in_game");
        let text = Text::default();
        let shown = |dialog, players| {
            let here = save::dialog::Here {
                title: String::new(),
                clock: 0,
                players,
            };
            let mut dialogs = Saves::default();

            show_saves(&mut dialogs, &player, dialog, ((here, None), &text));
            dialogs.names()
        };

        for dialog in [save::dialog::Dialog::Save, save::dialog::Dialog::Load] {
            assert_eq!(shown(dialog, Players::Alone), ["solo"], "{dialog:?}");
            assert_eq!(shown(dialog, Players::Lan(2)), ["lan"], "{dialog:?}");
        }
    }

    /// A LAN save goes to `mp-saves` on every peer: the asker's under
    /// its name, the others' under a name not taken.
    #[test]
    fn a_lan_save_lands_in_mp_saves_on_every_peer() {
        let (_dir, player) = save::dialog::two_folders("lan_save");
        let lan = save::dir(&player.profile.as_ref().expect("profile").dir, Play::Lan);

        for (seat, local) in [(0, 0), (0, 1), (1, 1)] {
            let place = lan_place(&player, (seat, local), "x").expect("a place");
            let dir = match &place {
                Place::File(path) => path.parent(),
                Place::Free(dir) => Some(dir.as_path()),
                Place::Autosave(_) => None,
            };
            let free = matches!(place, Place::Free(_));

            assert_eq!(dir, Some(lan.as_path()), "{seat} on {local}");
            assert_eq!(free, seat != local, "{seat} on {local}");
        }
    }

    /// A press in a save dialog over the game plays its click at once,
    /// one, and leaves none queued for the menus.
    #[test]
    fn a_save_dialog_press_clicks_at_once_and_queues_none() {
        use std::collections::HashMap;

        use egui::{Event, Modifiers, PointerButton, RawInput};
        use regent_core::data::smnu::{self, Kind};
        use regent_core::view::menu::music::PRESS;
        use regent_core::view::menu::{Layout, ShownControl, saves as rules};

        static WAVE: [u8; 4] = *b"RIFF";

        let ctx = egui::Context::default();
        let window = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        let panel = smnu::Rect {
            x: -1,
            y: -1,
            width: 800,
            height: 600,
        };
        let cancel = smnu::Rect {
            x: 530,
            y: 552,
            width: 235,
            height: 26,
        };
        let mut art = crate::menu::art::Art::default();

        // AP27 with its CANCEL alone.
        art.book.load_game = Some(Layout {
            id: rules::LOAD_PAGE,
            panel: ShownControl {
                rect: Some(panel),
                ..ShownControl::default()
            },
            centred: true,
            style: None,
            controls: vec![ShownControl {
                id: Some(rules::CANCEL),
                kind: Some(Kind::Button),
                rect: Some(cancel),
                ..ShownControl::default()
            }],
        });
        art.clicks = HashMap::from([(PRESS, &WAVE[..])]);

        let mut shared = regent_draw::art::Art::default();
        let mut saves = Saves::default();

        saves.show(
            save::dialog::Dialog::Load,
            None,
            String::new(),
            (None, Players::Alone),
        );
        saves.said = None;

        let mut pass = |events| {
            let input = RawInput {
                events,
                screen_rect: Some(window),
                ..RawInput::default()
            };
            let mut shown = (None, None);
            let mut output = ctx.run_ui(input, |ui| {
                shown = save_dialog(ui.ctx(), (&mut art, &mut shared), &mut saves);
            });

            output.textures_delta.clear();
            shown
        };

        pass(vec![]);

        let fit = Fit::page(window, egui::vec2(800.0, 600.0));
        let at = fit.point(crate::menu::fit::rect(cancel).center());
        let button = |pressed| Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };

        let (_, moved) = pass(vec![Event::PointerMoved(at)]);
        let (_, pressed) = pass(vec![button(true)]);
        let (chose, released) = pass(vec![button(false)]);

        assert_eq!(moved, None);
        assert_eq!(pressed, Some(&WAVE[..]));
        assert_eq!((chose, released), (Some(Chose::Cancel), None));
        assert_eq!(crate::menu::click(&ctx), None);
    }

    #[test]
    fn a_save_is_offered_the_games_name_and_day() {
        assert_eq!(save_name(Some("The Siege"), "0 Days"), "(The Siege) 0 Days");
        assert_eq!(
            save_name(Some("Freestyle Game"), "1 Day"),
            "(Freestyle Game) 1 Day"
        );
        assert_eq!(save_name(None, "2 Days"), "2 Days");
    }

    /// A memo makes its value once a key: again only once the key changes.
    #[test]
    fn a_memo_makes_its_value_once_a_key() {
        let mut memo = Memo::default();
        let mut made = 0;
        let mut get = |memo: &mut Memo<u32, u32>, key| {
            *memo.get(key, || {
                made += 1;
                key * 10
            })
        };

        assert_eq!(get(&mut memo, 1), 10);
        assert_eq!(get(&mut memo, 1), 10);
        assert_eq!(get(&mut memo, 2), 20);
        assert_eq!(made, 2);
    }

    /// A panel's interface run is asked only while its answer shows: a
    /// palace's henchmen on its own panel, not beside a whole-column page
    /// nor for a rival's palace; a Mausoleum's dead on their page.
    #[test]
    fn a_panel_asks_only_what_it_can_show() {
        let chosen = |panel: &[u8; 4], owned| Chosen {
            agent: 7,
            code: FourCc(*b"ABJ1"),
            kind: upper::Kind::Building,
            panel: Some(FourCc(*panel)),
            name: String::new(),
            title: String::new(),
            health: None,
            grave: false,
            poison: 0,
            details: None,
            foreign: None,
            owned,
            at: (0, 0),
            contests: None,
        };
        let asked = |c: &Chosen, page| query(c, page, Upper::of(page, c, None));
        let palace = chosen(b"AP39", true);
        let crypt = chosen(b"AP31", true);

        assert_eq!(asked(&palace, Page::Help), Some(Query::Henchmen(7)));
        assert_eq!(asked(&palace, Page::Henchmen), None);
        assert_eq!(asked(&chosen(b"AP39", false), Page::Help), None);
        assert_eq!(asked(&crypt, Page::Crypt), Some(Query::Crypt(7)));
        assert_eq!(asked(&crypt, Page::Help), None);
    }

    #[test]
    fn a_click_on_the_spell_bar_or_options_does_not_reach_the_map() {
        let view = View {
            rect: Some(Rect::new(200.0, 0.0, 1000.0, 600.0)),
            ..default()
        };
        let covered = Covered(vec![
            Rect::new(208.0, 560.0, 800.0, 592.0),
            Rect::new(930.0, 560.0, 992.0, 592.0),
        ]);

        assert!(on_map(&view, &covered, Vec2::new(500.0, 300.0)));
        assert!(!on_map(&view, &covered, Vec2::new(250.0, 570.0)));
        assert!(!on_map(&view, &covered, Vec2::new(100.0, 20.0)));
        assert!(!on_map(&view, &covered, Vec2::new(950.0, 580.0)));
        assert!(on_map(&view, &Covered::default(), Vec2::new(900.0, 580.0)));
    }

    #[test]
    fn gold_reads_as_a_bare_number_without_the_table() {
        let text = Text::default();

        assert_eq!(gold_text(Some(30_000), &text), "30000");
        assert_eq!(gold_text(None, &text), "?");
    }
}
