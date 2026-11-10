//! The menus over the paused game, in the original's panels: OPTIONS'
//! `AP36`, the title scroll's goals `AP88`, and the quit questions `APc8`
//! and `AP04` (`OptionMenuController`, `Option3MenuController`,
//! `QuitMenuController`; [panels.md](docs/formats/panels.md#the-main-screen)).
//!
//! ```text
//!  OPTIONS ──> AP36 ──RESUME PLAY, Escape──> the game, paused as before
//!  scroll  ──> AP88 ──RESUME──> the game
//!                   └─OPTIONS──> AP36
//!  AP36 ──GAME INFO──> AP88, as the scroll opens it
//!       ──ADJUST LEVELS──> the settings editor, for AP37 ──closed──> AP36
//!       ──SAVE GAME──> the save dialog, for AP49 (`save::dialog`) ──closed──> AP36
//!       ──LOAD GAME──> the load dialog, for AP27 ──a save──> the menus,
//!                      which make it
//!       ──RETURN TO MAIN MENU──> APc8 ──YES──> the menus, as the end panel
//!       ──QUIT MAJESTY──────> AP04 ──YES──> exit
//!       ──RESTART───────────> APd3 ──YES──> the menus: a quest's summary,
//!                                  │        else the freestyle pick page
//!                                  └─NO──> AP36
//! ```
//!
//! A LAN game loads in its lobby and is not restarted: LOAD GAME and
//! RESTART are shown disabled ([`Play`]). Its SAVE GAME saves on every
//! peer (`net`). Its RETURN TO MAIN MENU row is split, as SAVE GAME and
//! LOAD GAME share theirs, into MAIN MENU and SURRENDER, openregent's own
//! ([`split`]): SURRENDER asks in `APc8`, its text replaced; YES and the
//! player watches (`Sim::surrender`). Once surrendered, it is disabled
//! ([`Seat`]).
//!
//! ```text
//!  ┌──────── ADJUST LEVELS ────────┐
//!  ┌──────── RESTART ──────────────┐
//!  ┌── MAIN MENU ──┐┌── SURRENDER ─┐   was RETURN TO MAIN MENU
//!  ┌ QUIT MAJESTY ┐┌ RESUME PLAY ──┐
//! ```
//!
//! The original pauses (app state 7) as either opens and resumes on
//! RESUME; here the player's own pause comes back ([`Hold`]).

use bevy_egui::egui::Rect;
use regent_core::data::fourcc::FourCc;
use regent_core::data::smnu;
use regent_core::view::menu::{Layout, ShownControl};

pub use crate::game::sim::Seat;
use crate::menu::fit::Fit;
use crate::panel::canvas::{Align, Canvas};
use crate::panel::sheet::{self, Able};
use crate::ui::layout::Frame;

/// The panels.
const OPTIONS: FourCc = FourCc(*b"AP36");
const GOALS: FourCc = FourCc(*b"AP88");
const TO_MENUS: FourCc = FourCc(*b"APc8");
const TO_SYSTEM: FourCc = FourCc(*b"AP04");
const TO_RESTART: FourCc = FourCc(*b"APd3");

/// `AP36`'s buttons.
mod id {
    pub const GAME_INFO: i32 = 5000;
    pub const SAVE: i32 = 5001;
    pub const LOAD: i32 = 5002;
    pub const LEVELS: i32 = 5003;
    pub const TO_MENUS: i32 = 5004;
    pub const QUIT: i32 = 5005;
    pub const RESUME: i32 = 5006;
    pub const RESTART: i32 = 5007;

    /// SURRENDER, in a LAN game: openregent's own, no id of `AP36`.
    pub const SURRENDER: i32 = 5900;

    /// The question's text box, which SURRENDER's question replaces.
    pub const QUESTION_TEXT: i32 = 5000;

    /// `AP88`'s (`Option3MenuController`): its goals box, scroll bar,
    /// RESUME and OPTIONS.
    pub const GOALS_TEXT: i32 = 5001;
    pub const GOALS_SCROLL: i32 = 5053;
    pub const GOALS_RESUME: i32 = 9004;
    pub const GOALS_OPTIONS: i32 = 8088;

    /// The questions' (`QuitMenuController`).
    pub const YES: i32 = 5501;
    pub const NO: i32 = 5001;
}

/// `AP36`'s buttons, in no order, and SURRENDER.
const BUTTONS: [i32; 9] = [
    id::GAME_INFO,
    id::SAVE,
    id::LOAD,
    id::LEVELS,
    id::TO_MENUS,
    id::QUIT,
    id::RESUME,
    id::RESTART,
    id::SURRENDER,
];

/// The halves of the split row: the art, font and colours of SAVE GAME
/// and LOAD GAME, whose row is split so.
const HALVES: [i32; 2] = [id::SAVE, id::LOAD];

/// The split row's labels: placeholders, HD has no such text. MAIN MENU
/// keeps RETURN TO MAIN MENU's hot key, M.
const MAIN_MENU: &str = "MAIN MENU";
const SURRENDER: &str = "SURRENDER";

/// SURRENDER's question, in `APc8`'s text box. A placeholder.
const SURRENDER_QUESTION: &str = "Surrender the game? Your kingdom plays on without you, and you watch the whole map. The game ends once every player is out.";

/// `AP36`'s buttons a LAN game disables: a LAN game loads in its
/// lobby, and does not restart. SAVE GAME saves on every peer (`net`).
const ALONE_ONLY: [i32; 2] = [id::LOAD, id::RESTART];

/// Whether the game is this machine's alone or a LAN game: a load and a
/// restart are one machine's new game.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Play {
    Alone,
    Lan,
}

impl Play {
    /// The game of `players` peers.
    #[must_use]
    pub const fn of(players: u32) -> Play {
        if players > 1 { Play::Lan } else { Play::Alone }
    }

    /// How a load or restart button shows.
    #[must_use]
    pub const fn able(self) -> Able {
        match self {
            Play::Alone => Able::Enabled,
            Play::Lan => Able::Disabled,
        }
    }
}

/// Which menu is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dialog {
    /// `AP36`.
    Options,
    /// `AP88`, the goals.
    Goals,
    /// The settings editor, standing in for `AP37`.
    Settings,
    /// The save or load dialog (`save::dialog`), for `AP49` and `AP27`.
    Saves,
    /// A question: `APc8`, `AP04` or `APd3`.
    Confirm(Leave),
}

/// Where a quit goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Leave {
    /// To the menus: `PauseGameMessage` command 5, pause return decode.
    Menus,
    /// Out of the game: command 4, pause exit decode.
    System,
    /// To the menus on the game's start page: game restart.
    Restart,
    /// Out of play, into watching: a LAN game's surrender.
    Surrender,
}

/// What a frame's press asks of the game around the menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Asked {
    /// Open the settings editor.
    Settings,
    /// Open the save dialog.
    Save,
    /// Open the load dialog.
    Load,
    Leave(Leave),
}

/// The player's pause, held while a menu is open: the game pauses as one
/// opens and goes back to how it was as the last closes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Hold(Option<bool>);

impl Hold {
    /// A menu opens over pause `paused`: kept the first time, then held.
    pub const fn open(&mut self, paused: &mut bool) {
        if self.0.is_none() {
            self.0 = Some(*paused);
        }

        *paused = true;
    }

    /// The menus close: the pause as it was before the first opened.
    pub const fn close(&mut self, paused: &mut bool) {
        if let Some(before) = self.0.take() {
            *paused = before;
        }
    }
}

/// The menus' state between frames.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Menus {
    pub open: Option<Dialog>,
    hold: Hold,
    /// The goals box's scroll, pixels.
    scroll: f32,
}

impl Menus {
    /// `dialog` open over the game, paused.
    pub const fn show(&mut self, dialog: Dialog, paused: &mut bool) {
        self.hold.open(paused);
        self.open = Some(dialog);
        self.scroll = 0.0;
    }

    /// Every menu closed, the pause as before.
    pub const fn close(&mut self, paused: &mut bool) {
        self.hold.close(paused);
        self.open = None;
    }
}

/// A panel's fit: centred on the window at the interface's scale, as a
/// panel at (-1, -1) is centred.
pub fn centred(frame: &Frame, layout: &Layout, window: Rect) -> Fit {
    let size = sheet::bounds(layout).size() * frame.scale;

    Fit {
        scale: frame.scale,
        origin: window.center() - size / 2.0,
    }
}

/// The panel `dialog` shows; the editor is egui's.
#[must_use]
pub const fn panel(dialog: Dialog) -> Option<FourCc> {
    match dialog {
        Dialog::Options => Some(OPTIONS),
        Dialog::Goals => Some(GOALS),
        Dialog::Confirm(Leave::Menus | Leave::Surrender) => Some(TO_MENUS),
        Dialog::Confirm(Leave::System) => Some(TO_SYSTEM),
        Dialog::Confirm(Leave::Restart) => Some(TO_RESTART),
        Dialog::Settings | Dialog::Saves => None,
    }
}

/// The open menu, its panel `layout` ([`panel`]), in `canvas` fitted by
/// [`centred`]: what its press asks, the menu moved on in `menus`.
/// `goals` is `AP88`'s text; `play` disables what a LAN game cannot do,
/// `seat` a surrender given.
pub fn draw(
    canvas: &mut Canvas<'_>,
    layout: &Layout,
    (menus, paused): (&mut Menus, &mut bool),
    (goals, play, seat): (&str, Play, Seat),
) -> Option<Asked> {
    // RESUME PLAY's Escape is its hot key: taken once (`hotkey`).
    match menus.open? {
        Dialog::Options => {
            match options(canvas, layout, (play, seat)) {
                Some(id::RESUME) => menus.close(paused),
                Some(id::TO_MENUS) => menus.open = Some(Dialog::Confirm(Leave::Menus)),
                Some(id::SURRENDER) => menus.open = Some(Dialog::Confirm(Leave::Surrender)),
                Some(id::QUIT) => menus.open = Some(Dialog::Confirm(Leave::System)),
                Some(id::RESTART) => menus.open = Some(Dialog::Confirm(Leave::Restart)),
                // panel open of `AP88`, as the scroll's pause opens it.
                Some(id::GAME_INFO) => {
                    menus.open = Some(Dialog::Goals);
                    menus.scroll = 0.0;
                }
                Some(id::LEVELS) => {
                    menus.open = Some(Dialog::Settings);
                    return Some(Asked::Settings);
                }
                Some(id::SAVE) => {
                    menus.open = Some(Dialog::Saves);
                    return Some(Asked::Save);
                }
                Some(id::LOAD) => {
                    menus.open = Some(Dialog::Saves);
                    return Some(Asked::Load);
                }
                _ => {}
            }

            None
        }
        Dialog::Goals => {
            match goals_panel(canvas, layout, goals, &mut menus.scroll) {
                Some(id::GOALS_RESUME) => menus.close(paused),
                Some(id::GOALS_OPTIONS) => menus.open = Some(Dialog::Options),
                _ => {}
            }

            None
        }
        Dialog::Confirm(leave) => match question(canvas, layout, leave) {
            Some(id::YES) => Some(Asked::Leave(leave)),
            Some(id::NO) => {
                menus.open = Some(Dialog::Options);
                None
            }
            _ => None,
        },
        Dialog::Settings | Dialog::Saves => None,
    }
}

/// `AP36`: its frame and labels, then each button, some disabled in a
/// LAN game, whose row to the menus is [`split`]; which was pressed.
fn options(canvas: &mut Canvas<'_>, layout: &Layout, (play, seat): (Play, Seat)) -> Option<i32> {
    let layout = match play {
        Play::Alone => layout.clone(),
        Play::Lan => split(layout),
    };

    sheet::statics(canvas, &layout, None, &|c| BUTTONS.contains(&c));

    pressed(canvas, &layout, &BUTTONS, &|id| match id {
        _ if ALONE_ONLY.contains(&id) => play.able(),
        id::SURRENDER if seat == Seat::Spectates => Able::Disabled,
        _ => Able::Enabled,
    })
}

/// `layout`, `AP36`, its RETURN TO MAIN MENU row split in two: MAIN
/// MENU on the left, SURRENDER on the right, each in a half of
/// [`HALVES`] at its own place, centred on the row's height. Without
/// those controls, unchanged.
fn split(layout: &Layout) -> Layout {
    let row = layout.control(id::TO_MENUS).and_then(|c| c.rect);
    let halves = HALVES.map(|i| layout.control(i).cloned());
    let mut out = layout.clone();

    if let (Some(row), [Some(left), Some(right)]) = (row, halves) {
        let half = |c: ShownControl, (id, text, hotkey): (i32, &str, Option<i32>)| {
            let rect = c.rect.map(|r| smnu::Rect {
                y: row.y + (row.height - r.height) / 2,
                ..r
            });

            ShownControl {
                id: Some(id),
                rect,
                text: Some(text.to_owned()),
                tooltip: None,
                hotkey,
                ..c
            }
        };
        let to_menus = layout.control(id::TO_MENUS).and_then(|c| c.hotkey);

        out.controls.retain(|c| c.id != Some(id::TO_MENUS));
        out.controls
            .push(half(left, (id::TO_MENUS, MAIN_MENU, to_menus)));
        out.controls
            .push(half(right, (id::SURRENDER, SURRENDER, None)));
    }

    out
}

/// `AP88`: the goals in their box, scrolled by the wheel, RESUME and
/// OPTIONS.
fn goals_panel(
    canvas: &mut Canvas<'_>,
    layout: &Layout,
    goals: &str,
    scroll: &mut f32,
) -> Option<i32> {
    let buttons = [id::GOALS_RESUME, id::GOALS_OPTIONS];
    let own = [id::GOALS_TEXT, id::GOALS_SCROLL];

    sheet::statics(canvas, layout, None, &|c| {
        buttons.contains(&c) || own.contains(&c)
    });

    if let Some(c) = layout.control(id::GOALS_TEXT)
        && let (Some(r), Some(font)) = (sheet::rect(c), c.font)
    {
        let tall = canvas.paragraphs(font, r, goals, (sheet::colour(c), Align::Left, *scroll));

        *scroll = (*scroll + canvas.wheel(r)).clamp(0.0, (tall - r.height()).max(0.0));
    }

    pressed(canvas, layout, &buttons, &|_| Able::Enabled)
}

/// A question: its text, YES and NO. A surrender's asks its own.
fn question(canvas: &mut Canvas<'_>, layout: &Layout, leave: Leave) -> Option<i32> {
    let buttons = [id::YES, id::NO];
    let mut layout = layout.clone();

    if leave == Leave::Surrender {
        for c in &mut layout.controls {
            if c.id == Some(id::QUESTION_TEXT) {
                c.text = Some(SURRENDER_QUESTION.to_owned());
            }
        }
    }

    sheet::statics(canvas, &layout, None, &|c| buttons.contains(&c));

    pressed(canvas, &layout, &buttons, &|_| Able::Enabled)
}

/// Each of `ids` drawn as a button, `able` saying which press; the one
/// pressed.
fn pressed(
    canvas: &mut Canvas<'_>,
    layout: &Layout,
    ids: &[i32],
    able: &dyn Fn(i32) -> Able,
) -> Option<i32> {
    ids.iter()
        .filter_map(|&i| layout.control(i).map(|c| (i, c)))
        .fold(None, |hit, (i, c)| {
            let now = one_line(canvas, c, able(i));

            hit.or(now.then_some(i))
        })
}

/// Button `c`, its label on one line, centred; whether pressed. `AP36`
/// spaces QUIT MAJESTY and RESUME PLAY "QUIT      MAJESTY", perhaps to
/// break in two (**Inferred**); drawn with one space, as the user asked.
fn one_line(canvas: &mut Canvas<'_>, c: &ShownControl, able: Able) -> bool {
    let label = c.text.as_deref().map(spaced);

    sheet::button(canvas, c, label.as_deref(), able)
}

/// `text` with each run of spaces one space.
fn spaced(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use bevy::prelude::KeyCode;
    use bevy_egui::egui::{self, Event, Key, Modifiers, Pos2, RawInput, UiBuilder};
    use regent_core::data::catalogue::Catalogue;
    use regent_core::data::smnu;
    use regent_core::view::menu::hotkey;

    use super::*;
    use crate::menu::fit::Fit;
    use crate::panel::ink::Ink;

    /// The window, pixels.
    const WINDOW: egui::Vec2 = egui::vec2(800.0, 600.0);

    /// A button with Escape for its hot key.
    fn on_escape(id: i32, x: i32) -> ShownControl {
        ShownControl {
            id: Some(id),
            rect: Some(smnu::Rect {
                x,
                y: 0,
                width: 40,
                height: 20,
            }),
            hotkey: Some(hotkey::ESCAPE),
            ..ShownControl::default()
        }
    }

    /// One frame as `ui::bar` draws it: the bottom bar's OPTIONS
    /// (`APMK`, Escape) under a disabled ui while a menu is open, then
    /// `AP36` over it. `escape`: Escape pressed this frame.
    fn frame(ctx: &egui::Context, menus: &mut Menus, paused: &mut bool, escape: bool) {
        let catalogue: &'static Catalogue<'static> =
            Box::leak(Box::new(Catalogue::build(&[]).expect("an empty catalogue")));
        let mut ink = Ink::default();
        let mut art = regent_draw::art::Art::default();
        let window = Rect::from_min_size(Pos2::ZERO, WINDOW);
        let bar = on_escape(id::RESUME + 100, 0);
        let layout = Layout {
            id: OPTIONS,
            panel: ShownControl::default(),
            centred: true,
            style: None,
            controls: vec![on_escape(id::RESUME, 100)],
        };
        let events = if escape {
            vec![Event::Key {
                key: Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }]
        } else {
            vec![]
        };
        let presses = if escape {
            vec![(KeyCode::Escape, hotkey::ESCAPE)]
        } else {
            vec![]
        };

        crate::input::hotkey::claim(ctx, &presses);

        let input = RawInput {
            events,
            screen_rect: Some(window),
            ..RawInput::default()
        };
        let mut output = ctx.run_ui(input, |ui| {
            let mut under = ui.new_child(UiBuilder::new());

            if menus.open.is_some() {
                under.disable();
            }

            let mut canvas = Canvas {
                ui: &mut under,
                ink: &mut ink,
                art: &mut art,
                source: regent_draw::art::ArtSource::of(&[]),
                catalogue,
                fit: Fit::of(window, WINDOW),
                salt: "bar",
            };

            if sheet::button(&mut canvas, &bar, None, Able::Enabled) {
                menus.show(Dialog::Options, paused);
            }

            if menus.open.is_some() {
                let mut canvas = Canvas {
                    ui,
                    ink: &mut ink,
                    art: &mut art,
                    source: regent_draw::art::ArtSource::of(&[]),
                    catalogue,
                    fit: Fit::of(window, WINDOW),
                    salt: "options",
                };

                draw(
                    &mut canvas,
                    &layout,
                    (menus, paused),
                    ("", Play::Alone, Seat::Plays),
                );
            }
        });

        // No painter takes the font atlas here.
        output.textures_delta.clear();
    }

    /// A button `id` at `x`, hot key `key`.
    fn keyed(id: i32, x: i32, key: u8) -> ShownControl {
        ShownControl {
            hotkey: Some(i32::from(key)),
            ..on_escape(id, x)
        }
    }

    /// One frame of the open menu over `controls`, `key` pressed (a hot
    /// key's character); what it asked.
    fn pressing(
        ctx: &egui::Context,
        (menus, paused): (&mut Menus, &mut bool),
        controls: &[ShownControl],
        key: Option<u8>,
    ) -> Option<Asked> {
        let catalogue: &'static Catalogue<'static> =
            Box::leak(Box::new(Catalogue::build(&[]).expect("an empty catalogue")));
        let mut ink = Ink::default();
        let mut art = regent_draw::art::Art::default();
        let window = Rect::from_min_size(Pos2::ZERO, WINDOW);
        let layout = Layout {
            id: OPTIONS,
            panel: ShownControl::default(),
            centred: true,
            style: None,
            controls: controls.to_vec(),
        };
        let presses: Vec<(KeyCode, i32)> = key
            .map(|k| (KeyCode::KeyA, i32::from(k)))
            .into_iter()
            .collect();
        let mut asked = None;

        crate::input::hotkey::claim(ctx, &presses);

        let input = RawInput {
            screen_rect: Some(window),
            ..RawInput::default()
        };
        let mut output = ctx.run_ui(input, |ui| {
            let mut canvas = Canvas {
                ui,
                ink: &mut ink,
                art: &mut art,
                source: regent_draw::art::ArtSource::of(&[]),
                catalogue,
                fit: Fit::of(window, WINDOW),
                salt: "options",
            };

            asked = draw(
                &mut canvas,
                &layout,
                (menus, paused),
                ("", Play::Alone, Seat::Plays),
            );
        });

        output.textures_delta.clear();
        asked
    }

    /// GAME INFO opens the goals, as the scroll does; RESTART asks with
    /// `APd3`, whose YES asks the restart.
    #[test]
    fn game_info_shows_the_goals_and_restart_asks_first() {
        let ctx = egui::Context::default();
        let controls = [
            keyed(id::GAME_INFO, 0, b'G'),
            keyed(id::RESTART, 50, b'R'),
            keyed(id::YES, 100, b'Y'),
        ];
        let mut paused = false;
        let mut menus = Menus::default();
        let mut press = |menus: &mut Menus, key| {
            pressing(&ctx, (menus, &mut paused), &controls, None);
            pressing(&ctx, (menus, &mut paused), &controls, Some(key))
        };

        menus.show(Dialog::Options, &mut false);
        assert_eq!(press(&mut menus, b'G'), None);
        assert_eq!(menus.open, Some(Dialog::Goals));

        menus.open = Some(Dialog::Options);
        assert_eq!(press(&mut menus, b'R'), None);
        assert_eq!(menus.open, Some(Dialog::Confirm(Leave::Restart)));
        assert_eq!(panel(Dialog::Confirm(Leave::Restart)), Some(TO_RESTART));

        assert_eq!(press(&mut menus, b'Y'), Some(Asked::Leave(Leave::Restart)));
    }

    /// One Escape opens the menu, the next closes it (RESUME PLAY).
    #[test]
    fn escape_opens_options_and_the_next_escape_resumes() {
        let ctx = egui::Context::default();
        let mut menus = Menus::default();
        let mut paused = false;

        frame(&ctx, &mut menus, &mut paused, false);
        frame(&ctx, &mut menus, &mut paused, true);
        assert_eq!(menus.open, Some(Dialog::Options));

        frame(&ctx, &mut menus, &mut paused, false);
        assert_eq!(menus.open, Some(Dialog::Options));

        frame(&ctx, &mut menus, &mut paused, true);
        assert_eq!(menus.open, None);
        assert!(!paused);
    }

    #[test]
    fn a_menu_pauses_and_gives_back_the_pause_it_found() {
        for before in [false, true] {
            let mut paused = before;
            let mut menus = Menus::default();

            menus.show(Dialog::Options, &mut paused);
            assert!(paused);

            menus.close(&mut paused);
            assert_eq!(paused, before);
        }
    }

    /// Options to the editor and back, then the goals: one hold, the first
    /// pause kept.
    #[test]
    fn moving_between_menus_keeps_the_first_pause() {
        let mut paused = false;
        let mut menus = Menus::default();

        menus.show(Dialog::Options, &mut paused);
        menus.show(Dialog::Settings, &mut paused);
        menus.show(Dialog::Goals, &mut paused);
        menus.close(&mut paused);

        assert!(!paused);
        assert_eq!(menus.open, None);
    }

    /// A control `id` at `(x, y)`, `width` × `height`, hot key `key`.
    fn placed(
        id: i32,
        (x, y, width, height): (i32, i32, i32, i32),
        key: Option<i32>,
    ) -> ShownControl {
        ShownControl {
            id: Some(id),
            rect: Some(smnu::Rect {
                x,
                y,
                width,
                height,
            }),
            hotkey: key,
            text: Some("label".to_owned()),
            ..ShownControl::default()
        }
    }

    /// `AP36`'s RETURN TO MAIN MENU row, split: MAIN MENU where SAVE
    /// GAME stands across, its hot key kept, and SURRENDER where LOAD
    /// GAME does, each centred on the row. Its rectangles as `AP36`'s.
    #[test]
    fn the_lan_row_to_the_menus_splits_into_main_menu_and_surrender() {
        let layout = Layout {
            id: OPTIONS,
            panel: ShownControl::default(),
            centred: true,
            style: None,
            controls: vec![
                placed(id::SAVE, (22, 114, 100, 28), Some(83)),
                placed(id::LOAD, (127, 114, 100, 28), Some(76)),
                placed(id::TO_MENUS, (21, 208, 207, 31), Some(77)),
            ],
        };
        let out = split(&layout);
        let at = |id| {
            let c = out.control(id).expect("a control");

            (
                c.rect.map(|r| (r.x, r.y, r.width, r.height)),
                c.text.clone(),
                c.hotkey,
            )
        };

        assert_eq!(out.controls.len(), 4);
        assert_eq!(
            at(id::TO_MENUS),
            (
                Some((22, 209, 100, 28)),
                Some(MAIN_MENU.to_owned()),
                Some(77)
            )
        );
        assert_eq!(
            at(id::SURRENDER),
            (Some((127, 209, 100, 28)), Some(SURRENDER.to_owned()), None)
        );
    }

    #[test]
    fn a_spaced_label_is_one_line() {
        assert_eq!(spaced("QUIT      MAJESTY"), "QUIT MAJESTY");
        assert_eq!(spaced("RESUME    PLAY"), "RESUME PLAY");
    }

    #[test]
    fn closing_with_nothing_open_leaves_the_pause() {
        let mut paused = true;
        let mut hold = Hold::default();

        hold.close(&mut paused);
        assert!(paused);
    }
}
