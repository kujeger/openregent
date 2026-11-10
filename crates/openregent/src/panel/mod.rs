//! The left column's panels and the bars, drawn from the original's
//! decoded panels (`SMNU`), pictures and fonts.
//!
//! ```text
//!  column (APMK, HD's left 206 × 848)
//!   ┌──────────────┐  0   minimap 2001, its frame and buttons
//!   │   minimap    │
//!   ├──────────────┤ 182  selection: the piece's DialogID panel (upper),
//!   │  selection   │      or a question
//!   ├──────────────┤ 428  a page: help, build, rewards, members,
//!   │    page      │      visitors, research, a hero's stats, items,
//!   ├──────────────┤ 675  spells; the palace's statistics
//!   │              │      and rosters fill 182 to 672
//!   │  mini view   │      no mini view: the selection's name
//!   └──────────────┘ 848
//!  top bar: gold, day      bottom bar: SPELLS, messages, OPTIONS
//!  spell bar (AP92) over the map's bottom left
//! ```
//!
//! What is shown is the world's: the library's names, hit points and
//! `portrait::Details`; what a button does is the orders'
//! ([`crate::input::orders`]), sent as `Input`s and so recorded and replayed.

pub mod bars;
pub mod canvas;
pub mod details;
pub mod games;
pub mod ink;
pub mod lower;
pub mod message;
pub mod palace;
pub mod sheet;
pub mod stripes;
pub mod tip;
pub mod upper;

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use bevy::prelude::Resource;
use regent_core::data::catalogue::{Catalogue, Library};
use regent_core::data::fourcc::FourCc;
use regent_core::data::smnu::Panel;
use regent_core::view::menu::colour::PALETTES;
use regent_core::view::menu::{self, Layout, Style};

use regent_core::data::text::Text;
use regent_core::view::menu::quest_map;

use ink::Ink;

/// HD's layouts at 1920 × 1200: the main screen `APMK` and the palace's
/// two-slot pages. The captures' size (`regent_cli panels`).
pub(crate) const UIDATA: &str = "Data/UIData_1920_1200.dat";

/// The main screen.
pub const MAIN: FourCc = FourCc(*b"APMK");

/// What the page under the selection shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Page {
    /// The selected kind's tips, `AP95` (`HelpController`).
    #[default]
    Help,
    /// A building type's tips, `AP95` opened by the build list's "?"
    /// (`BuildingListController`, 8029): the type's `HelpID`, the
    /// selection's name.
    Topic(FourCc),
    /// The palace's building list, `AP03` (`BuildingListController`).
    Build,
    /// The palace's reward flags, `AP41` (`Reward1Controller`).
    Rewards,
    /// A guild's members, `APd1` (`GuildMemberController`).
    Members,
    /// A building's visitors, `AP91` (`BuildingVisitorController`), the
    /// Fairgrounds' `AP09`.
    Visitors,
    /// A building's research, as `APa3`.
    Research,
    /// A Mausoleum's dead, as `APd1`.
    Crypt,
    /// A graveyard's heroes, `AP16` (`GraveyardListController`).
    Graves,
    /// The Hall of Champions' bounties, `MX01`
    /// (`HallOfChampionsP2_Controller`).
    Bounty,
    /// A reward flag's seekers, `AP87` attack, `AP86` explore
    /// (`RewardFlagMemberController`).
    Seekers,
    /// A building's spells (`SpellsController`): the Sorcerer's Abode's
    /// `MX07`, a temple's or wizards' guild's `AP66`–`AP72`.
    Casting,
    /// A hero's statistics, items and spells: `AP21`, `AP22`, `AP78`.
    Stats,
    Items,
    Spells,
    /// The palace's statistics, `AP40` (`SummaryChartsController`).
    Statistics,
    /// The palace's rosters: `AP85` (`Top5Controller`), `APc1`
    /// (`KingdomBuildingsController`), `APc2` (`HenchmenListController`).
    Heroes,
    Buildings,
    Henchmen,
}

impl Page {
    /// Whether the page fills both slots, selection and page: HD's
    /// palace pages are 490 high from (0, 182).
    #[must_use]
    pub const fn whole(self) -> bool {
        matches!(
            self,
            Page::Statistics | Page::Heroes | Page::Buildings | Page::Henchmen
        )
    }
}

/// A question waiting for yes or no (`AP62`, `KillConfirmController`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ask {
    /// Demolish this building.
    Demolish(u32),
    /// Dismiss this hero, by agent.
    Dismiss(u32),
}

/// The panels' state between frames.
#[derive(Default)]
pub struct State {
    pub page: Page,
    /// The page drawn last, none with nothing selected.
    pub shown: Option<Page>,
    /// The piece the page was opened for: a new selection resets it.
    pub page_of: Option<u32>,
    /// The hero page last opened, the next hero's first.
    pub hero_page: Option<Page>,
    /// The page's scroll, pixels.
    pub scroll: f32,
    /// The hero's enchantments list scroll, pixels.
    pub enchant_scroll: f32,
    /// The message scroll's.
    pub message: message::Scroll,
    /// The build list's toggles.
    pub build: lower::Toggles,
    /// The build list's rows, kept.
    built: lower::Built,
    /// The rosters' sorts and detail.
    pub rosters: palace::Rosters,
    /// The roster shown, its rows kept.
    listed: palace::Listed,
    /// The page list's lit row.
    pub row: Option<usize>,
    /// The build list's presses, told single or double.
    pub build_clicks: crate::input::Clicks,
    /// Whether the spell bar is closed: open at a new game, as `ui_full.png`.
    pub spells_closed: bool,
    /// The menus over the paused game: OPTIONS', the goals.
    pub options: crate::ui::options::Menus,
    pub ask: Option<Ask>,
    /// A unit's name box being edited: its agent and the text so far.
    pub naming: Option<(u32, String)>,
    /// The Gambling Hall panel's own wager and colour.
    pub bet: Option<games::Bet>,
    /// The tooltip line's control.
    pub tip: regent_core::view::panel::tip::Line<bevy_egui::egui::Id>,
    /// `FRAME IT`'s frame rate in the day counter's place (`frame_rate`).
    pub frames: regent_core::view::panel::day::Frames,
}

/// The panels' art, layouts and state.
#[derive(Resource, Default)]
pub struct Panels {
    pub ink: Ink,
    layouts: HashMap<FourCc, Option<Arc<Layout>>>,
    /// `UIData`'s layouts and their texts.
    uidata: HashMap<FourCc, Arc<Layout>>,
    uidata_read: bool,
    pub state: State,
}

impl crate::draw::shelf::Shelved for Panels {
    fn kept(self) -> Self {
        Panels {
            state: State::default(),
            ..self
        }
    }
}

impl State {
    /// The rows kept by game frame forgotten: a LAN resync's world may
    /// stand at the frame they were read at.
    pub fn forget(&mut self) {
        self.built = lower::Built::default();
        self.listed = palace::Listed::default();
    }
}

impl Panels {
    /// Panel `id` from the game's catalogue, read once.
    pub fn layout(
        &mut self,
        catalogue: &Catalogue<'_>,
        text: &Text,
        id: FourCc,
    ) -> Option<Arc<Layout>> {
        self.layouts
            .entry(id)
            .or_insert_with(|| {
                menu::layout(catalogue, text, id)
                    .and_then(Result::ok)
                    .map(Arc::new)
            })
            .clone()
    }

    /// Panel `id` from the `UIData` of HD at `hd` (1920 × 1200), else the
    /// catalogue's.
    pub fn sized(
        &mut self,
        (catalogue, text): (&Catalogue<'_>, &Text),
        id: FourCc,
        hd: &Path,
    ) -> Option<Arc<Layout>> {
        if !self.uidata_read {
            self.uidata_read = true;
            self.uidata = uidata(hd);
        }

        self.uidata
            .get(&id)
            .cloned()
            .or_else(|| self.layout(catalogue, text, id))
    }
}

/// Every panel of HD's 1920 × 1200 `UIData` but the quest map, its
/// texts the base game's: the file holds no `STRT APMK`, and its labels
/// index `textdata.cam`'s, not the expansion's (**Game**: 6002 names 21,
/// "SPELLS" there, "OPTIONS" in `mx_textdata.cam`; `ui_full.png` shows
/// SPELLS). None without the install at `hd`.
fn uidata(hd: &Path) -> HashMap<FourCc, Arc<Layout>> {
    let read = || -> Option<HashMap<FourCc, Arc<Layout>>> {
        let library = Library::from_files(&[hd.join(UIDATA)]).ok()?;
        let catalogue = library.catalogue().ok()?;
        let base = Library::from_files(&[hd.join(BASE_TEXT)]).ok()?;
        let base = base.catalogue().ok()?;
        let text = Text::build(&base);
        let palettes = Library::from_files(&[hd.join("Data").join(PALETTES)]).ok()?;
        let palettes = palettes.catalogue().ok()?;

        Some(
            catalogue
                .tagged(b"SMNU")
                .filter(|(_, record)| record.id != quest_map::PANEL)
                .filter_map(|(_, record)| {
                    let panel = Panel::parse(record.data).ok()?;
                    let style = Style::find(&palettes, Style::palette(&panel));

                    Some((
                        record.id,
                        Arc::new(menu::resolved(&panel, &text, record.id, style)),
                    ))
                })
                .collect(),
        )
    };

    read().unwrap_or_default()
}

/// The base game's texts, which `UIData`'s labels index.
const BASE_TEXT: &str = "Data/textdata.cam";

/// The panels' art and state, a game's.
pub fn plugin(app: &mut bevy::prelude::App) {
    use crate::game::scope::Scoped as _;

    app.per_game::<Panels>();
}
