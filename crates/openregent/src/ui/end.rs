//! The game's end, as the original's state 8 shows it
//! (`regent_core::view::menu::end`): once the crown's game has ended
//! (`World::ended`), a won game's "Game Over" box, the fade over the
//! interface, then the end panels in their own art, centred at the
//! interface's scale as the options' panels (**Game**: 800 × 600 in the
//! middle of `ui_victory_briefing.png`'s 1680 × 1050).
//!
//! ```text
//!  won   AP11 Game Over ──OK──> view to the point, #EndGameFadeDelay ┐
//!  lost  ────────────────────────────────────────────────────────────┴─> fade,
//!        15 steps of 75 real ms and the advisor done ──> AP60 / AP61
//!
//!  AP60 / AP61   pog, title, YOU HAVE WON! / YOU LOST!, QEND / LOSX
//!  AP84 / AP83   World::statistics in the stat row's controls
//!  buttons       STATISTICS ──> AP84 / AP83, no way back
//!                CONTINUE PLAYING ──> the game on (World::play_on)
//!                RESTART ──> Return::Restart   MAIN MENU ──> Return::Main
//!  a LAN game    no CONTINUE PLAYING nor RESTART; MAIN MENU is "Return
//!                to Lobby" ──> Return::Lobby, the lobby on the game's
//!                socket (net::Peer::regroup); the host's going back
//!                takes each guest still in the game with it
//! ```
//!
//! A won quest is recorded in the profile, any won game in the high
//! scores ([`record`]); a film owed plays full screen before the press
//! that asks it goes on ([`owed`]), as video play blocks in
//! `EndGameController`; the advisor and a won quest's voice are
//! `audio`'s, the voice stopped by any press.
//! The victory star is not drawn. A LAN spectator's loss is shown once
//! every seat is out (`Sim::end`).

use bevy::prelude::*;
use bevy_egui::EguiContexts;
use bevy_egui::egui::{self, Color32, LayerId, Rect, Ui, UiBuilder, vec2};

use regent_core::data::fourcc::FourCc;
use regent_core::data::text::plain;
use regent_core::data::text::{GMTX, Text, printf};
use regent_core::gpl::exec::Literal;
use regent_core::gpl::load::Program;
use regent_core::sim::world::{
    World,
    defeat::{End, FADE_STEPS, FREESTYLE_NAME, NO_POINT},
};
use regent_core::view::menu::end::{
    self, Advisor, Button, Curtain, Mark, Page, Panel, Stage, Step, id,
};
use regent_core::view::menu::film::Film;
use regent_core::view::menu::{Layout, ShownControl as Control, ids, message};
use regent_draw::data::Corpus;
use regent_draw::stage::View;

use crate::files::profile::Player;
use crate::game::seen::{Ask, Outbox, Watch};
use crate::game::sim::Session;
use crate::game::thread::SimThread;
use crate::menu::{Page as MenuPage, Quests, Return, Returning, Screen};
use crate::panel::Panels;
use crate::panel::canvas::{Align, Canvas};
use crate::panel::ink::{Pic, Tone};
use crate::panel::sheet::{self, Able};
use crate::ui::layout::Frame;

/// The light the fade takes off the interface by its last step.
/// **Game**: under the panels the column and bars are half as bright as
/// over the "Game Over" box, the map as bright
/// (`ui_victory.png`, `ui_victory_briefing.png`). **Inferred**: the map
/// is drawn again each frame, the interface not; each step
/// takes an equal share.
const SHADE: f32 = 0.5;

/// The controls a panel fills itself, or does not show.
const OWN: [i32; 10] = [
    id::MAIN_MENU,
    id::RESTART,
    id::STATISTICS,
    id::CONTINUE,
    id::POG,
    id::LOST_TEXT,
    ids::SCROLL_BAR,
    id::MAP_LABEL,
    id::MAP_NUMBER,
    id::UNSHOWN,
];

/// The "Game Over" box's controls it fills, and the two it hides.
const GAME_OVER_OWN: [i32; 5] = [
    message::TITLE,
    message::TEXT,
    message::OK,
    message::YES,
    message::NO,
];

/// Whether the crown's game goes on, copied from the
/// [`crate::game::seen::Frame`] each frame ([`mark`]) for the run conditions.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PlayState {
    #[default]
    Playing,
    Over,
}

/// [`PlayState`] from the world's end.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
pub fn mark(frame: Res<crate::game::seen::Frame>, mut game: ResMut<PlayState>) {
    let now = if frame.ended {
        PlayState::Over
    } else {
        PlayState::Playing
    };

    game.set_if_neq(now);
}

/// Whether the crown's game goes on: a run condition.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
pub fn playing(game: Res<PlayState>) -> bool {
    *game == PlayState::Playing
}

/// Record a won quest in the player's profile, once:
/// one more victory, the best time the game
/// clock where shorter, kept by its code, a mod's quest's a CRC of its id
/// (`Progress::win_quest`). A freestyle game, a lost one, or a game with no
/// profile (none could be read or made) records nothing there. Any won
/// game goes in the high scores, once, where they are kept ([`scored`]).
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
pub fn record(
    (seen, session): (Watch, Res<Session>),
    mut player: ResMut<Player>,
    (kept, corpus): (
        Option<ResMut<crate::files::scores::Scoring>>,
        Option<Res<Corpus>>,
    ),
) {
    let won = seen.end == Some(End::Won);
    let quest = session.quest_id.filter(|_| won);

    if let (true, Some(mut kept), Some(corpus)) = (won, kept, corpus)
        && !kept.scored
    {
        let monarch = player
            .profile
            .as_ref()
            .map_or(crate::files::profile::DEFAULT_NAME, |p| p.name.as_str());

        if let Some(path) = &kept.place {
            scored((&seen, &session), &corpus, monarch, path);
        }

        kept.scored = true;
    }

    if let Some(quest) = quest
        && !player.recorded
        && let Some(profile) = &mut player.profile
    {
        profile.progress.win_quest(quest, seen.world().now());

        match profile.save() {
            Ok(()) => info!("{quest} won at {} ms, recorded", seen.world().now()),
            Err(why) => warn!("{why}"),
        }

        player.recorded = true;
    }
}

/// The won game's high scores recorded in the table at `path`:
/// a quest's as EPIC QUESTS, a custom game's as
/// FREESTYLE, played by `monarch`; a hero named as its panel names it.
fn scored(
    (seen, session): (&Watch, &Session),
    corpus: &Corpus,
    monarch: &str,
    path: &std::path::Path,
) {
    use regent_core::content::unit::Unit;
    use regent_core::view::menu::scores::{self, GameKind};
    use regent_core::view::panel::portrait;

    let world = seen.world();
    let text = &corpus.text;
    let (game, title) = if let Some((_, title)) = &session.quest {
        (GameKind::Quests, title.clone())
    } else {
        let title = text
            .at(GMTX, FREESTYLE_NAME)
            .map(|t| plain(&t).trim().to_owned())
            .unwrap_or_default();

        (GameKind::Freestyle, title)
    };
    let named = |id: u32| {
        let code = world.piece(id).map_or(FourCc([0; 4]), |p| p.code);
        let unit = Unit::find(&corpus.catalogue, code).and_then(Result::ok);
        let class = unit
            .as_ref()
            .map_or_else(|| code.to_string(), |u| u.desc.head.title().into_owned());
        let who = portrait::who(world, id, code);

        (
            portrait::panel_name(&corpus.catalogue, text, &who, &class),
            class,
        )
    };
    let scores = world.scores(seen.local, &named);
    let entries = scores::entries(&scores, monarch, &title);

    match crate::files::scores::record(path, game, entries) {
        Ok(()) => info!("high scores: {game:?} won, recorded"),
        Err(why) => warn!("{why}"),
    }
}

/// A press on the end's box or panels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Press {
    /// The "Game Over" box's OK.
    Ok,
    Button(Button),
}

/// State 8 on screen: where it stands, the verdict's scroll, and a press
/// still to carry out.
#[derive(Resource, Default)]
pub struct EndScreen {
    curtain: Option<Curtain>,
    scroll: f32,
    pressed: Option<Press>,
    /// Gone back to the LAN lobby: once.
    left: bool,
    /// The films a press owes, the one playing first ([`owed`]).
    films: std::collections::VecDeque<Film>,
    /// The press they hold up.
    held: Option<Press>,
}

impl EndScreen {
    /// Whether an end panel is open: its voice may start.
    #[must_use]
    pub fn opened(&self) -> bool {
        self.curtain.is_some_and(|c| c.panel().is_some())
    }

    /// Whether a film plays: the music is hushed for it.
    #[must_use]
    pub fn filming(&self) -> bool {
        !self.films.is_empty()
    }

    /// Held at `curtain`: a screenshot's stage (`--end`).
    pub const fn hold(&mut self, curtain: Curtain) {
        self.curtain = Some(curtain);
    }

    /// The panel open, if any.
    #[cfg(test)]
    pub fn panel(&self) -> Option<Panel> {
        self.curtain.and_then(Curtain::panel)
    }

    /// `press` made, as the player makes it.
    #[cfg(test)]
    pub const fn press(&mut self, press: Press) {
        self.pressed = Some(press);
    }
}

/// What carrying out a press touches.
#[derive(bevy::ecs::system::SystemParam)]
pub struct Around<'w> {
    returning: ResMut<'w, Returning>,
    screen: ResMut<'w, NextState<Screen>>,
    player: ResMut<'w, Player>,
    narration: ResMut<'w, crate::audio::Narration>,
    view: ResMut<'w, View>,
    spoken: Res<'w, crate::audio::Spoken>,
    time: Res<'w, Time<Real>>,
    session: Res<'w, Session>,
    thread: Res<'w, SimThread>,
}

/// State 8's steps from the world's end and the real clock, and the
/// press drawn last frame carried out.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
pub fn curtain(
    (seen, mut outbox): (Watch, ResMut<Outbox>),
    mut shown: ResMut<EndScreen>,
    minimap: crate::ui::minimap::Parts,
    mut around: Around,
) {
    let now = u64::try_from(around.time.elapsed().as_millis()).unwrap_or(u64::MAX);
    let advisor = if around.spoken.0 {
        Advisor::Done
    } else {
        Advisor::Speaking
    };

    shown.curtain = match (seen.end, shown.curtain) {
        (None, _) => None,
        (Some(end), None) => Some(Curtain::new(end, now)),
        (Some(_), Some(c)) => Some(c.tick(now, advisor)),
    };

    let mut pressed = shown.pressed.take();

    // A press that owes films waits for them.
    if let (Some(Press::Button(button)), Some(panel)) =
        (pressed, shown.curtain.and_then(Curtain::panel))
    {
        let films = owed(panel, button, play_of(&seen), &mut around.player);

        if !films.is_empty() {
            around.narration.hush();
            shown.films = films.into();
            shown.held = pressed.take();
        }
    }

    if !shown.filming() {
        pressed = pressed.or(shown.held.take());
    }

    if let (Some(press), Some(c)) = (pressed, shown.curtain) {
        let mut left = shown.left;

        shown.curtain = carried(
            (c, press, now),
            (&seen, &mut outbox),
            (&minimap, &mut around, &mut left),
        );
        shown.left = left;
    }

    // The host back in its lobby takes this guest with it, ended or not.
    if seen.regrouped {
        shown.left |= regroup(&mut around, shown.left);
    }
}

/// Whether the game is this machine's alone or a LAN game's, as the end
/// panels tell them apart.
fn play_of(seen: &Watch) -> end::Play {
    match crate::ui::options::Play::of(seen.players) {
        crate::ui::options::Play::Alone => end::Play::Alone,
        crate::ui::options::Play::Lan => end::Play::Network,
    }
}

/// Back to the LAN lobby on the game's socket (`net::Peer::regroup`),
/// the menus on its page, unless `left` already; no socket to keep, the
/// main menu. Whether it has left.
fn regroup(around: &mut Around, left: bool) -> bool {
    if !left {
        match around.thread.regroup() {
            Some(regroup) => around.returning.0 = Some(Return::Lobby(regroup)),
            None => around.returning.0 = Some(Return::Main),
        }

        around.screen.set(Screen::Menu);
    }

    true
}

/// `press` on `curtain` at real ms `now`: what stands after; `left`
/// once gone back to the LAN lobby.
fn carried(
    (curtain, press, now): (Curtain, Press, u64),
    (seen, outbox): (&Watch, &mut Outbox),
    (minimap, around, left): (&crate::ui::minimap::Parts, &mut Around, &mut bool),
) -> Option<Curtain> {
    match (press, curtain.panel()) {
        (Press::Ok, _) => {
            let point = seen
                .world()
                .end_point(seen.local)
                .filter(|p| *p != NO_POINT);

            if let Some(at) = point {
                crate::ui::minimap::centre(minimap, &mut around.view, at);
            }

            Some(curtain.told(now, point.map(|_| fade_delay(seen.program))))
        }
        (Press::Button(button), Some(panel)) => {
            // Any press stops the end voice first.
            around.narration.hush();

            match panel.after(button, play_of(seen)) {
                Some(Step::To(to)) => Some(Curtain {
                    stage: Stage::Shown(to.page),
                    ..curtain
                }),
                Some(Step::MainMenu) => {
                    around.returning.0 = Some(Return::Main);
                    around.screen.set(Screen::Menu);
                    Some(curtain)
                }
                Some(Step::Restart) => {
                    around.returning.0 = Some(Return::Restart(around.session.quest_key()));
                    around.screen.set(Screen::Menu);
                    Some(curtain)
                }
                Some(Step::PlayOn) => {
                    // A later victory opens `AP60` again, which records.
                    outbox.push(Ask::PlayOn);
                    around.player.recorded = false;
                    None
                }
                Some(Step::Lobby) => {
                    *left |= regroup(around, *left);
                    Some(curtain)
                }
                None => Some(curtain),
            }
        }
        (Press::Button(_), None) => Some(curtain),
    }
}

/// The films `button` on `panel` owes (`Panel::films`), those it marks
/// seen in the player's profile, saved. HD asks `MV07` with no archive
/// holding it open, so plays nothing; here it plays.
fn owed(panel: Panel, button: Button, play: end::Play, player: &mut Player) -> Vec<Film> {
    player.profile.as_mut().map_or_else(Vec::new, |profile| {
        let films = panel.films(button, &profile.progress, play);
        let marked: Vec<Film> = films
            .iter()
            .filter(|(_, mark)| *mark == Mark::Seen)
            .map(|(film, _)| *film)
            .collect();

        if !marked.is_empty() {
            for film in &marked {
                profile.progress.seen.mark(*film);
            }

            match profile.save() {
                Ok(()) => info!("a film seen, recorded"),
                Err(why) => warn!("{why}"),
            }
        }

        films.into_iter().map(|(film, _)| film).collect()
    })
}

/// `#EndGameFadeDelay`, the scripts'.
fn fade_delay(program: &Program) -> u64 {
    match program.constant(end::FADE_DELAY) {
        Some(Literal::Int(ms)) => u64::try_from(ms).unwrap_or(0),
        _ => u64::from(end::FADE_DELAY_MS),
    }
}

/// What the panels draw with.
#[derive(bevy::ecs::system::SystemParam)]
pub struct Drawn<'w> {
    panels: ResMut<'w, Panels>,
    art: ResMut<'w, regent_draw::art::Art>,
    shown: ResMut<'w, EndScreen>,
    /// The quests' pogs, in the menus' art.
    pogs: (Res<'w, Quests>, ResMut<'w, crate::menu::Art>),
    video: ResMut<'w, crate::menu::Video>,
    clock: (Res<'w, Time<Real>>, Res<'w, crate::files::config::Settings>),
    covered: ResMut<'w, crate::ui::Covered>,
}

/// Draw where state 8 stands: the fade over the interface, the box or a
/// panel over all, the whole window kept from the map; its press kept
/// for [`curtain`].
///
/// # Errors
///
/// If the primary egui context is not ready yet.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
pub fn panel(
    mut contexts: EguiContexts,
    (seen, session): (Watch, Res<Session>),
    corpus: Res<Corpus>,
    mut drawn: Drawn,
) -> Result {
    let ctx = contexts.ctx_mut()?.clone();

    if drawn.shown.filming() {
        film(&ctx, &mut drawn);
    } else if let Some(curtain) = drawn.shown.curtain {
        let window = ctx.viewport_rect();
        let frame = Frame::of(window.size());
        let mut ui = Ui::new(
            ctx.clone(),
            "end".into(),
            UiBuilder::new()
                .layer_id(LayerId::new(egui::Order::Foreground, "end".into()))
                .max_rect(window),
        );

        shade(&ui, &frame, curtain.shade());

        // Nothing under the end takes the pointer.
        ui.interact(window, ui.id().with("block"), egui::Sense::click());
        drawn.covered.0.push(bevy::math::Rect::new(
            window.min.x,
            window.min.y,
            window.max.x,
            window.max.y,
        ));

        let record = match curtain.stage {
            Stage::Told => Some(end::GAME_OVER),
            Stage::Fading { .. } => None,
            Stage::Shown(_) => curtain.panel().map(Panel::record),
        };
        let Drawn {
            panels,
            art,
            shown,
            pogs,
            ..
        } = &mut drawn;
        let layout = record.and_then(|r| panels.layout(&corpus.catalogue, &corpus.text, r));

        if let Some(layout) = layout {
            let mut canvas = Canvas {
                ui: &mut ui,
                ink: &mut panels.ink,
                art,
                source: corpus.source(),
                catalogue: &corpus.catalogue,
                fit: crate::ui::options::centred(&frame, &layout, window),
                salt: "end",
            };
            let text = &corpus.text;
            let pressed = match curtain.panel() {
                None => game_over(&mut canvas, &layout, text).then_some(Press::Ok),
                Some(panel) => {
                    let play = play_of(&seen);
                    let said = Said::of((panel, play), (&seen, &session, text), pogs);

                    shows(
                        &mut canvas,
                        &layout,
                        (panel, &said, play),
                        &mut shown.scroll,
                    )
                    .map(Press::Button)
                }
            };

            shown.pressed = shown.pressed.or(pressed);
        }
    }

    Ok(())
}

/// The film owed played full screen over all, its end, a key or a press
/// the next; one that will not open goes by at once.
fn film(ctx: &egui::Context, drawn: &mut Drawn) {
    let Drawn {
        art,
        shown,
        pogs: (_, menus),
        video,
        clock: (time, settings),
        ..
    } = drawn;
    let page = shown
        .films
        .front()
        .map_or(MenuPage::Main, |f| MenuPage::Film(*f));

    video.play(page, menus);

    if let Some(playing) = video.playing_mut() {
        playing.tick(ctx, time.delta(), settings.enhancements.video_upscale);
    }

    let over = video.shown().is_none_or(|on| {
        crate::menu::full_screen(
            ctx,
            (&mut **menus, &mut **art),
            on,
            crate::menu::Layer::OverGame,
        )
        .is_some()
    });

    if over {
        shown.films.pop_front();

        // The last stopped, so its sound goes.
        if !shown.filming() {
            video.play(MenuPage::Main, menus);
        }
    }
}

/// The fade's `steps` over the column and bars.
fn shade(ui: &Ui, frame: &Frame, steps: u32) {
    #[allow(clippy::cast_precision_loss, reason = "fifteen steps")]
    let dark = SHADE * steps as f32 / FADE_STEPS as f32;
    let colour = Color32::BLACK.gamma_multiply(dark);

    for r in [frame.column, frame.top, frame.bottom] {
        ui.painter().rect_filled(r, 0.0, colour);
    }
}

/// `AP11` as state 8's step 2 sets it: "Game Over", "You are
/// victorious.", OK alone; whether OK was pressed.
fn game_over(canvas: &mut Canvas<'_>, layout: &Layout, text: &Text) -> bool {
    let gmtx = |i| text.at(GMTX, i).map(|t| plain(&t).trim().to_owned());

    sheet::statics(canvas, layout, None, &|c| GAME_OVER_OWN.contains(&c));

    if let Some(title) = gmtx(end::GAME_OVER_TITLE) {
        sheet::fill(
            canvas,
            layout,
            message::TITLE,
            &title,
            (None, Align::Centre),
        );
    }

    if let (Some(c), Some(said)) = (layout.control(message::TEXT), gmtx(end::VICTORIOUS))
        && let Some(r) = sheet::rect(c)
    {
        sheet::label(canvas, c, r, &said);
    }

    layout
        .control(message::OK)
        .is_some_and(|c| sheet::button(canvas, c, None, Able::Enabled))
}

/// What a panel says.
struct Said {
    /// The quest's name; none on a statistics page, which keeps its own
    /// "End Game Statistics".
    title: Option<String>,
    verdict: String,
    pog: Option<(FourCc, i32)>,
    map_number: Option<u32>,
    /// Each statistics control and its line.
    rows: Vec<(i32, String)>,
    /// MAIN MENU's words where not its own: a LAN game's "Return to
    /// Lobby".
    main_menu: Option<String>,
}

impl Said {
    /// `panel`'s: the quest's title, pog and verdict,
    /// a custom game's map number, the statistics (`EndSummaryController`,
    /// which sets the pog alone), MAIN MENU's words in a game `play`.
    fn of(
        (panel, play): (Panel, end::Play),
        (seen, session, text): (&Watch, &Session, &Text),
        (quests, art): &(Res<Quests>, ResMut<crate::menu::Art>),
    ) -> Said {
        let quest = session.quest.as_ref();
        let title = quest.map_or_else(
            || {
                text.at(GMTX, FREESTYLE_NAME)
                    .map(|t| plain(&t).trim().to_owned())
                    .unwrap_or_default()
            },
            |(_, title)| title.clone(),
        );
        let (title, rows) = match panel.page {
            Page::Verdict => (Some(title), Vec::new()),
            Page::Statistics => (None, statistics((seen.world(), seen.local), text)),
        };

        Said {
            title,
            // Markup kept: control 3 draws it.
            verdict: end::verdict(text, panel.end, session.quest_key(), &session.messages)
                .trim()
                .to_owned(),
            pog: quests.pog(art, quest.map(|q| q.0)),
            map_number: end::map_number(session.setup.as_ref().filter(|_| quest.is_none())),
            rows,
            main_menu: Panel::main_menu_text(play)
                .and_then(|at| text.at(GMTX, at))
                .map(|t| plain(&t).trim().to_owned()),
        }
    }
}

/// The crown's statistics lines in their controls:
/// "Total Made: 70".
fn statistics((world, local): (&World, u32), text: &Text) -> Vec<(i32, String)> {
    world
        .statistics(local)
        .iter()
        .enumerate()
        .filter_map(|(row, line)| {
            let control = end::stat_control(row)?;
            let format = text.at(GMTX, line.text)?;
            let number = line.number.unwrap_or_default();

            Some((control, printf(plain(&format).trim(), &[number.into()])))
        })
        .collect()
}

/// `panel` in `layout` saying `said`: its frame and pictures, the pog
/// and title, the verdict scrolled by `scroll` or the statistics, the
/// map number, the buttons a game `play` offers; the one pressed.
fn shows(
    canvas: &mut Canvas<'_>,
    layout: &Layout,
    (panel, said, play): (Panel, &Said, end::Play),
    scroll: &mut f32,
) -> Option<Button> {
    let filled = |c: i32| {
        OWN.contains(&c)
            || panel.text() == Some(c)
            || (c == id::TITLE && said.title.is_some())
            || said.rows.iter().any(|(id, _)| *id == c)
    };

    sheet::statics(canvas, layout, None, &filled);

    if let (Some((record, set)), Some(r)) =
        (said.pog, layout.control(id::POG).and_then(sheet::rect))
    {
        canvas.picture(Pic::of(record, set), Tone::Plain, r.min);
    }

    if let Some(title) = &said.title {
        sheet::fill(canvas, layout, id::TITLE, title, (None, Align::Centre));
    }

    if let Some(n) = said.map_number {
        if let Some(c) = layout.control(id::MAP_LABEL)
            && let (Some(r), Some(label)) = (sheet::rect(c), c.text.as_deref())
        {
            sheet::label(canvas, c, r, label);
        }

        sheet::fill(
            canvas,
            layout,
            id::MAP_NUMBER,
            &n.to_string(),
            (None, Align::Centre),
        );
    }

    if let Some(c) = panel.text().and_then(|t| layout.control(t)) {
        verdict(
            canvas,
            (c, layout.control(ids::SCROLL_BAR)),
            &said.verdict,
            scroll,
        );
    }

    for (control, line) in &said.rows {
        sheet::fill(canvas, layout, *control, line, (None, Align::Centre));
    }

    // A LAN game neither goes on nor restarts: one machine's new game.
    panel.buttons(play).iter().fold(None, |hit, &b| {
        let text = match b {
            Button::MainMenu => said.main_menu.as_deref(),
            _ => None,
        };
        let now = layout
            .control(b.id())
            .is_some_and(|c| sheet::button(canvas, c, text, Able::Enabled));

        hit.or(now.then_some(b))
    })
}

/// The verdict in text box `c`, each line centred: a text that fits
/// centred up and down (`ui_defeat_briefing.png`), a longer one from the
/// top, scrolled by the wheel and the arrows of scroll bar `bar`
/// (`ui_victory_briefing.png`).
fn verdict(
    canvas: &mut Canvas<'_>,
    (c, bar): (&Control, Option<&Control>),
    text: &str,
    scroll: &mut f32,
) {
    let (Some(r), Some(font)) = (sheet::rect(c), c.font) else {
        return;
    };
    let colour = sheet::colour(c);
    let pitch = canvas.pitch(font);
    #[allow(clippy::cast_precision_loss, reason = "a box's lines")]
    let tall = pitch * canvas.lines(font, r.width(), text) as f32;
    let most = crate::ui::scroll::most(tall, r.height());

    if most > 0.0 {
        canvas.paragraphs(font, r, text, (colour, Align::Centre, *scroll));

        // An arrow steps a line.
        let arrowed = bar.map_or(0.0, |b| arrows(canvas, b, (*scroll, most, pitch.max(1.0))));
        let moved = canvas.wheel(r) + arrowed;

        *scroll = (*scroll + moved).clamp(0.0, most);
    } else {
        let block = Rect::from_center_size(r.center(), vec2(r.width(), tall));

        canvas.paragraphs(font, block, text, (colour, Align::Centre, 0.0));
        *scroll = 0.0;
    }
}

/// Scroll bar `bar`'s arrows and thumb over a text scrolled `by` of
/// `most`, in its own art (`INDs` 1000: up, down, thumb); how far they
/// move it, each arrow a `step`, the thumb dragged through the track.
fn arrows(canvas: &mut Canvas<'_>, bar: &Control, (by, most, step): (f32, f32, f32)) -> f32 {
    let (Some(rect), Some((record, set))) = (sheet::rect(bar), bar.art) else {
        return 0.0;
    };
    let pic = |past: i32| Pic::of(record, set + past);
    let (Some(arrow), Some(thumb)) = (
        canvas.size(pic(0)),
        canvas.size(pic(crate::ui::scroll::THUMB)),
    ) else {
        return 0.0;
    };
    let bar = crate::ui::scroll::Bar { rect, arrow, thumb };
    let scale = canvas.fit.scale;

    crate::ui::scroll::pass(
        canvas,
        (&bar, scale),
        (by, most, step),
        |c, r, past| c.press(r, ("end arrow", past)),
        |c, at| c.drag(at, "end thumb"),
        |c, past, slot, at| {
            c.picture(pic(past).slot(slot), Tone::Plain, at);
        },
    )
}

/// The end's state, a game's.
pub fn plugin(app: &mut bevy::prelude::App) {
    use crate::game::scope::Scoped as _;

    app.per_game::<PlayState>().per_game::<EndScreen>();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A LAN game's MAIN MENU reads as HD's `GMTX` 336.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_lan_games_main_menu_is_return_to_lobby() {
        let hd = crate::files::env::path(crate::files::env::HD).expect("MAJESTY_HD");
        let archives = crate::game::load::archives(&hd).expect("the dataset");
        let catalogue = regent_draw::data::catalogue(&archives).expect("catalogue");
        let text = Text::build(&catalogue);
        let words = Panel::main_menu_text(end::Play::Network).and_then(|at| text.gmtx(at));

        assert_eq!(
            words.map(|t| t.trim().to_owned()),
            Some("Return to Lobby".to_owned())
        );
    }

    /// With every expansion quest won, RESTART leaves the ending unseen,
    /// MAIN MENU marks it in the profile and on disk.
    #[test]
    fn the_press_that_asks_the_ending_marks_the_profile() {
        use regent_core::view::menu::quest_map::SLOTS;

        let scratch = crate::testdata::scratch("end_ending");
        let dir = scratch.path();

        let profiles = crate::files::profile::Profiles::at(dir);
        let mut made = profiles.create("P").expect("made");

        // Every slot to `XQ12`, the expansion's last.
        for &quest in &SLOTS[..=30] {
            made.progress.win(quest, 1);
        }

        let mut player = Player {
            profile: Some(made),
            recorded: false,
        };
        let seen = |player: &Player| {
            player
                .profile
                .as_ref()
                .is_some_and(|p| p.progress.seen.ending)
        };

        let press = |button, player: &mut Player| {
            owed(Panel::of(End::Lost), button, end::Play::Alone, player)
        };

        assert_eq!(press(Button::Restart, &mut player), [Film::North]);
        assert!(!seen(&player));

        assert_eq!(press(Button::MainMenu, &mut player), [Film::North]);
        assert!(seen(&player));
        assert!(press(Button::MainMenu, &mut player).is_empty(), "once");
        assert!(profiles.open("P").is_ok_and(|p| p.progress.seen.ending));
    }
}
