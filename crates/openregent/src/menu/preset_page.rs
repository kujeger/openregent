//! The Load and Save Freestyle Options pages, `MX20` and `MX21`, from
//! the setup page's LOAD and SAVE, in their own art as the setup page:
//! their rules are the library's (`regent_core::view::menu::presets`), the
//! files `presets`'.
//!
//! ```text
//!  ┌(pog)─────────── Save Freestyle Options ─────────────────┐
//!  │ [SAVE FREESTYLE AS][name typed           ]  ┌────────┐ │
//!  │ ┌──────────────────────────────────────┐^│  │  IX22  │ │
//!  │ │ funbar                                │#│  │  1000  │ │
//!  │ │ ...  20 rows                          │v│  │        │ │
//!  │ (SAVE SETTINGS)  (DELETE SETTINGS)  (CANCEL)             │
//!  └──────────────────────────────────────────────────────────┘
//! ```
//!
//! `Load` has SAVED FREESTYLE GAMES over the list, no name box and LOAD
//! SETTINGS. A page closes with Cancel; the multiplayer settings page
//! can open it as the setup page does, on its own player count
//! ([`Io::of`]).

use bevy_egui::egui::{self, Color32, Sense};
use regent_core::data::smnu;
use regent_core::view::menu::hotkey;
use regent_core::view::menu::presets::{self, Mode};
use regent_core::view::menu::{Layout, ShownControl, State, ids};

use super::art::{Art, Pack};
use super::fit;
use super::freestyle::Custom;
use super::page::{Button, Press};
use super::paint::{self, Able, Canvas, Inset};
use super::setup::{boxed, ground, line};
use crate::files::presets::Preset;
use regent_draw::art as service;

/// How far in from the list's left a name starts, as the Build Tree
/// Editor's. **Inferred** from the captures: 19 px.
const NAME_INDENT: i32 = 16;

/// The lit row's band. **Game**: `ui_freestyle_options_load.png`, about
/// `#605240`, the list's whole width.
const LIT: Color32 = Color32::from_rgb(0x60, 0x52, 0x40);

/// The longest name typed. **Inferred**: the name box's key 32, 100.
const NAME_CHARS: usize = 100;

/// The rows the list shows at once: its height over a row's.
const ROWS: usize = 20;

/// The page's buttons, each on one page or both.
const BUTTONS: [i32; 4] = [
    presets::LOAD,
    presets::SAVE,
    presets::DELETE,
    presets::CANCEL,
];

/// The page's state between passes.
#[derive(Debug)]
pub struct Io {
    mode: Mode,
    /// The session's player count: the list shows its presets alone.
    players: u32,
    /// The presets listed.
    listed: Vec<Preset>,
    /// The row lit.
    lit: Option<usize>,
    /// The name as typed, SAVE's.
    name: String,
    /// The first row shown.
    scroll: f32,
}

impl Default for Io {
    fn default() -> Io {
        Io {
            mode: Mode::Load,
            players: crate::files::presets::SOLO,
            listed: Vec::new(),
            lit: None,
            name: String::new(),
            scroll: 0.0,
        }
    }
}

impl Io {
    /// Opening as `mode` on `custom`, in a session of `players`:
    /// the presets listed, SAVE's name
    /// box holding the config's name.
    #[must_use]
    pub fn of(mode: Mode, custom: &Custom, players: u32, default_name: &str) -> Io {
        let name = custom.name.as_deref().unwrap_or(default_name);
        let mut io = Io {
            mode,
            players,
            name: name.to_owned(),
            ..Io::default()
        };

        io.fill(name);
        io
    }

    /// The list read again, the row named `name` lit.
    fn fill(&mut self, name: &str) {
        self.listed = crate::files::presets::place()
            .map(|dir| crate::files::presets::list(&dir, self.players))
            .unwrap_or_default();

        let names: Vec<&str> = self.listed.iter().map(|p| p.name.as_str()).collect();

        self.lit = presets::lit(&names, name, self.mode);
    }
}

/// What a button of the page asks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Asked {
    Load,
    Save,
    Delete,
    Close,
}

/// The page: `custom` loaded over or saved; Cancel once closed.
pub fn page(
    ctx: &egui::Context,
    (art, shared, io): (&mut Art, &mut service::Art, &mut Io),
    custom: &mut Custom,
) -> Option<Press> {
    let asked = {
        let Art { book, ink, .. } = &mut *art;
        let layout = match io.mode {
            Mode::Load => book.load_options.as_ref()?,
            Mode::Save => book.save_options.as_ref()?,
        };
        let size = paint::size(layout);
        let mut canvas = Canvas::filling(ctx, (ink, shared), size);
        let mut asked = None;

        canvas.backdrop();

        canvas.panel_art(Pack::Expansion, layout);

        for c in &layout.controls {
            let id = c.id.unwrap_or_default();

            match id {
                _ if presets::CHAT.contains(&id) || id == io.mode.chat_bar() => {}
                presets::LOAD | presets::SAVE | presets::DELETE | presets::CANCEL => {
                    if canvas.button(Pack::Expansion, c, Able::Enabled) {
                        asked = asked_by(id);
                    }
                }
                presets::NAME => {
                    ground(&mut canvas, c);
                    name(&mut canvas, c, &mut io.name);
                }
                presets::LIST => {
                    ground(&mut canvas, c);
                    rows(&mut canvas, c, io);
                }
                ids::SCROLL_BAR => {
                    let list = layout.control(presets::LIST).and_then(|l| l.rect);

                    canvas.list_bar(
                        Pack::Expansion,
                        (c, list),
                        (&mut io.scroll, io.listed.len(), ROWS),
                        "preset list",
                    );
                }
                // Each picture inside its own rectangle, as the editor's.
                _ => {
                    canvas.clip = c.rect.map(fit::rect);
                    boxed(&mut canvas, c, c.text.as_deref());
                    canvas.clip = None;
                }
            }
        }

        asked.or_else(|| keyed(ctx, layout, io.mode))
    };

    asked.and_then(|asked| done(asked, io, (custom, art)))
}

/// The button `id` is.
const fn asked_by(id: i32) -> Option<Asked> {
    match id {
        presets::LOAD => Some(Asked::Load),
        presets::SAVE => Some(Asked::Save),
        presets::DELETE => Some(Asked::Delete),
        presets::CANCEL => Some(Asked::Close),
        _ => None,
    }
}

/// A button by its key: Enter LOAD or SAVE ([`Mode::confirm`]), Escape
/// CANCEL, a letter its button's while no box has the keys.
fn keyed(ctx: &egui::Context, layout: &Layout, mode: Mode) -> Option<Asked> {
    let typing = ctx.memory(|m| m.focused().is_some());
    let buttons: Vec<(&ShownControl, hotkey::Able)> = BUTTONS
        .iter()
        .filter_map(|&id| Some((layout.control(id)?, hotkey::Able::Enabled)))
        .collect();

    paint::keys(ctx).into_iter().find_map(|key| match key {
        hotkey::ENTER => asked_by(mode.confirm()),
        hotkey::ESCAPE => Some(Asked::Close),
        _ if typing => None,
        _ => hotkey::pick(buttons.iter().copied(), key).and_then(|c| asked_by(c.id?)),
    })
}

/// `asked` carried out: Cancel where the page closes. LOAD and DELETE
/// with no row lit do nothing.
fn done(asked: Asked, io: &mut Io, (custom, art): (&mut Custom, &mut Art)) -> Option<Press> {
    let close = Some(Press::Button(Button::Cancel));
    let lit = io.lit.and_then(|n| io.listed.get(n)).cloned();
    let place = crate::files::presets::place();

    match asked {
        Asked::Close => close,
        Asked::Load => lit.map(|preset| {
            let cheats = custom.cheats;
            *custom = Custom::loaded(&preset, &art.lists.trees);
            custom.cheats = cheats;
            Press::Button(Button::Cancel)
        }),
        Asked::Save => {
            let saved = place
                .as_deref()
                .ok_or_else(|| "no data directory".to_owned())
                .and_then(|dir| {
                    crate::files::presets::save(dir, &io.name, io.players, &custom.config())
                })
                .map(|name| custom.name = Some(name));

            match saved {
                Ok(()) => close,
                Err(why) => {
                    bevy::log::warn!("{why}");
                    None
                }
            }
        }
        Asked::Delete => {
            if let (Some(preset), Some(dir)) = (lit, place.as_deref())
                && let Err(why) = crate::files::presets::delete(dir, &preset.name)
            {
                bevy::log::warn!("{why}");
            }

            let name = custom.name.clone().unwrap_or_default();

            io.fill(&name);
            None
        }
    }
}

/// The presets' names from the first shown, one a row down list `c`,
/// the lit row's band behind; a row pressed is lit.
fn rows(canvas: &mut Canvas<'_>, c: &ShownControl, io: &mut Io) {
    let Some(r) = c.rect else { return };
    let colour = paint::pick(c, State::Normal);
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a list's rows"
    )]
    let first = io.scroll.round() as usize;
    let shown = io.listed.iter().enumerate().skip(first).take(ROWS);

    for (row, (n, preset)) in shown.enumerate() {
        let row = i32::try_from(row).unwrap_or_default();
        let band = smnu::Rect {
            x: r.x,
            y: r.y + row * presets::ROW_HEIGHT,
            width: r.width,
            height: presets::ROW_HEIGHT,
        };
        let at = canvas.fit.rect(fit::rect(band));
        let pressed = canvas
            .ui
            .interact(at, canvas.ui.id().with(("preset row", n)), Sense::click())
            .clicked();

        if pressed {
            io.lit = Some(n);
        }

        if io.lit == Some(n) {
            canvas.painter().rect_filled(at, 0.0, LIT);
        }

        line(
            canvas,
            &ShownControl {
                rect: Some(smnu::Rect {
                    x: band.x + NAME_INDENT,
                    width: band.width - NAME_INDENT,
                    ..band
                }),
                ..c.clone()
            },
            (&preset.name, colour),
        );
    }
}

/// The name typed in box `c`, the characters a file name cannot hold
/// refused ([`presets::typable`]).
fn name(canvas: &mut Canvas<'_>, c: &ShownControl, typed: &mut String) {
    let changed = c.rect.and_then(|r| {
        canvas.typed(
            (fit::rect(r), c.font, paint::pick(c, State::Normal)),
            (typed, NAME_CHARS, egui::Align::Min),
            ("preset name", Inset::Sides),
        )
    });

    if changed.is_some_and(|r| r.changed()) {
        typed.retain(presets::typable);
    }
}
