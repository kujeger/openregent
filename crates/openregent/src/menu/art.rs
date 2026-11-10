//! The menus' pictures, layouts, fonts and texts, read once from the
//! install; the pictures drawn through the art service.
//!
//! ```text
//!  base dataset       AP29 APc9 AP44 AP50 APd2 AP11 AP35, INTD/INTP/INTM quest art, fonts
//!  expansion dataset  AP29 APc9 AP44 AP50 (logo cover), MX17, APdb's art, IX11/MX29/IX73, QDEP, GMTX,
//!                     the faces' portraits (`faces`)
//!  UIData_1920_1200   APdb, the quest map, and its STRT; AP29's mods row
//! ```

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::Path;

use bevy::log::warn;
use bevy_egui::egui::{self, ColorImage, Rect, Vec2, vec2};
use regent_core::content::dataset::{Bytecode, Datasets, Install};
use regent_core::content::imagery;
use regent_core::content::imagery::FrameSource;
use regent_core::data::cam;
use regent_core::data::catalogue::Catalogue;
use regent_core::data::catalogue::Library;
use regent_core::data::deps;
use regent_core::data::fourcc::FourCc;
use regent_core::data::imag::Playback;
use regent_core::data::smnu::{Kind, Panel};
use regent_core::data::text::Text;
use regent_core::sim::world::sound::{self, Voice};
use regent_core::view::Sprite;
use regent_core::view::map::cursor;
use regent_core::view::menu::colour::PALETTES;
use regent_core::view::menu::faces;
use regent_core::view::menu::quest_map::{self, Placed, Progress, Site};
use regent_core::view::menu::scores::Category;
use regent_core::view::menu::{
    self, Layout, Release, ShownControl, Style, World, music, room, saves,
};
use regent_core::view::panel::day;
use regent_core::view::picture_file::Key;
use regent_core::view::sound::Sounds;

use super::faces::Faces;
use super::font::Font;
use crate::audio::Said;
use crate::game::load::{Offered, Origin};
use crate::panel::ink;
use crate::ui::scroll;
pub use regent_draw::art::Wrap;
use regent_draw::art::levels::{self, Levels};
use regent_draw::art::read::image;
use regent_draw::art::{Answer, Picture, Place, Reach, Scene, Tile, Want, When};

/// The freestyle quest's code: every freestyle save's (`FRE12`).
const FREESTYLE_QUEST: FourCc = FourCc(*b"FRE1");

/// The fonts the pages name: `fn11`, `fnt4`, `fnt7` (the map's line) and
/// `fnt2` (the name prompt's field).
const FONTS: [FourCc; 4] = [
    menu::LARGE_FONT,
    menu::SMALL_FONT,
    FourCc(*b"fnt7"),
    FourCc(*b"fnt2"),
];

/// Slots read of every control's picture: each `menu::drawn_slot` can
/// give, ticked, hovered and disabled at most (3 + 6 + 2), so a set's
/// count is known; a quest's lit picture is slot 1.
pub const SLOTS: usize = menu::CHECKED_SLOT + menu::HOVER_SLOT + menu::State::Disabled.index() + 1;

/// `AP29`'s room, "Background Image"; on the briefing pages id 1 is the
/// BACKGROUND heading, which has no picture.
pub const ROOM: i32 = 1;

/// `AP29`'s frame and logo bar, `INDm` 1000, "Splash Screen".
pub const SPLASH: i32 = 2000;

/// The expansion panels' logo cover, `IX72` 1004 or 1003.
pub const COVER: i32 = 2;

/// The quest map's Load Downloadable Quest jewel, `DX00` set 1001
/// (`docs/formats/panels.md`, the quest map); the panel names `IX34`.
pub const JEWEL: Pic = Pic::of(quest_map::JEWEL.record, quest_map::JEWEL.set);

/// A set's next layer: its id plus one in the top byte, the layer
/// counter (`docs/formats/art.md`, a repeated set is a stack of layers).
const NEXT_LAYER: i32 = 1 << 24;

/// A locked quest's cloud, `INQc` 1001: the glowing swirl, looping (layer
/// 0, mode 8), and the "?" over it (layer 1, mode 7), at `at` from the
/// swirl's corner.
#[derive(Clone, Copy, Debug)]
pub struct Cloud {
    pub swirl: (FourCc, i32),
    pub mark: Pic,
    pub at: Vec2,
}

/// One picture: an `IMAG` record, set and slot, frame 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Pic {
    pub record: FourCc,
    pub set: i32,
    pub slot: usize,
}

impl Pic {
    /// Slot 0 of `record`'s `set`.
    #[must_use]
    pub const fn of(record: FourCc, set: i32) -> Pic {
        Pic {
            record,
            set,
            slot: 0,
        }
    }

    /// The same picture in `slot`.
    #[must_use]
    pub const fn slot(self, slot: usize) -> Pic {
        Pic { slot, ..self }
    }

    /// As the art service names it: frame 0 of its slot.
    #[must_use]
    pub const fn pic(self) -> ink::Pic {
        ink::Pic::of(self.record, self.set).slot(self.slot)
    }
}

/// Which dataset's archives a picture or layout is read from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Pack {
    Base,
    Expansion,
}

impl Pack {
    /// A quest's: its `<Dataset base>`.
    const fn of(release: Release) -> Pack {
        match release {
            Release::Base => Pack::Base,
            Release::Expansion => Pack::Expansion,
        }
    }

    const fn at(self) -> usize {
        match self {
            Pack::Base => 0,
            Pack::Expansion => 1,
        }
    }
}

/// A room picture's slot: its frames and how they play
/// (`menu::strip`).
pub struct Run {
    pub tiles: Vec<Tile>,
    pub playback: Option<Playback>,
}

/// A room picture's slot named: source, record, set, slot.
type Slot = (Pack, FourCc, i32, usize);

/// Whether `c` is one of the main menu's moving pictures: one with art
/// but the room, frame, cover, buttons and the hidden
/// (`menu::room`): the room objects, and the expansion's window (111).
#[must_use]
pub fn moving(c: &ShownControl) -> bool {
    let still = |id: i32| {
        [ROOM, SPLASH, COVER].contains(&id)
            || room::HIDDEN.contains(&id)
            || menu::Button::SHOWN.iter().any(|b| b.id() == id)
    };

    c.art.is_some() && c.id.is_some_and(|id| !still(id))
}

/// The quest map's panel and its quests' places.
pub struct Map {
    pub layout: Layout,
    pub sites: Vec<Site>,
    /// The frame's pieces and what stays put with them, by id.
    pub pieces: Vec<(i32, Placed)>,
    /// The frame's size: the page's layout.
    pub frame: Vec2,
    /// The map picture's size.
    pub size: Vec2,
    /// FREESTYLE QUESTS' text rectangle, from its corner
    /// (`quest_map::text_rect`).
    pub label: Option<Rect>,
}

impl Map {
    /// On a `frame`-sized frame: the frame's controls moved by their
    /// anchors (`quest_map::moved`), the rest as they are.
    #[must_use]
    #[allow(
        clippy::cast_possible_truncation,
        reason = "a frame's whole layout pixels"
    )]
    pub fn at(&self, frame: Vec2) -> Map {
        let size = |v: Vec2| (v.x as i32, v.y as i32);
        let (from, to) = (size(self.frame), size(frame));
        let mut layout = self.layout.clone();

        for c in &mut layout.controls {
            if let (Some(id), Some(r)) = (c.id, c.rect)
                && quest_map::FIXED.contains(&id)
            {
                c.rect = Some(quest_map::moved(id, r, from, to));
            }
        }

        let pieces = self
            .pieces
            .iter()
            .map(|&(id, p)| {
                let rect = quest_map::moved(id, p.rect, from, to);

                (id, Placed { rect, ..p })
            })
            .collect();

        Map {
            layout,
            sites: self.sites.clone(),
            pieces,
            frame,
            size: self.size,
            label: self.label,
        }
    }
}

/// One offered quest's pictures, by its index in the offered list.
#[derive(Clone, Debug)]
pub struct Quest {
    pub source: Pack,
    /// Its slot on the map; `None` for the extra quests (`MD01`, `VAMP`).
    pub slot: Option<usize>,
    /// The round title-bar icon (`menu::icon`).
    pub icon: Option<Pic>,
    /// The summary's round still (`menu::zoom`).
    pub zoom: Option<Pic>,
    /// The loading page's still (`menu::segue`).
    pub segue: Option<Pic>,
}

/// What the quest map's title bar and the menus' egui pages say: the
/// expansion's `QDEP`, the `GMTX` strings they read, every quest's title.
#[derive(Default)]
pub struct Words {
    pub rules: Vec<deps::Quest>,
    gmtx: HashMap<usize, String>,
    titles: HashMap<FourCc, String>,
    /// The months' names, January first (`saves::MONTHS`).
    months: Vec<String>,
}

impl Words {
    /// `GMTX` string `index`, if read.
    #[must_use]
    pub fn gmtx(&self, index: usize) -> Option<String> {
        self.gmtx.get(&index).cloned()
    }

    /// Month `index`'s name, January 0.
    #[must_use]
    pub fn month(&self, index: usize) -> Option<String> {
        self.months.get(index).cloned()
    }

    /// A quest's title: `STRT QUES`, else the
    /// offered list's.
    #[must_use]
    pub fn title(&self, quest: FourCc) -> Option<String> {
        self.titles.get(&quest).cloned()
    }

    /// The title bar for `quest` under `progress`: title and line
    /// (`quest_map::title_bar`).
    #[must_use]
    pub fn title_bar(&self, quest: FourCc, progress: &Progress) -> Option<(String, String)> {
        quest_map::title_bar(&|i| self.gmtx(i), quest, (&self.rules, progress), &|q| {
            self.title(q)
        })
    }
}

/// Everything the menus draw: what is where, and the pictures. The
/// game's save dialogs and settings editor draw in it too.
#[derive(Default, bevy::prelude::Resource)]
pub struct Art {
    pub book: Book,
    pub ink: Store,
    /// The menus' cursors by `CUR1` set and slot.
    pub cursors: Vec<((u32, usize), regent_draw::art::cursor::Picture)>,
    /// What the menus say, by `DSND`: the map's advisors, each quest's
    /// briefing.
    pub voices: HashMap<FourCc, Said>,
    /// A press's clicks, `WAVE` bytes by record (`menu::music::CLICKS`).
    pub clicks: HashMap<FourCc, &'static [u8]>,
    /// What the freestyle setup page lists.
    pub lists: super::freestyle::Lists,
    /// The `PICT` videos the menus play, the record by id ([`videos`]).
    pub videos: HashMap<FourCc, cam::Record<'static>>,
    /// The cinematics' archives (`menu::CINEMATICS`), read as a video
    /// from them plays.
    pub cinematics: Vec<std::path::PathBuf>,
}

/// The layouts and the quests' art: what is where.
#[derive(Default)]
pub struct Book {
    main: [Option<Layout>; 2],
    briefing: [Option<Layout>; 2],
    summary: [Option<Layout>; 2],
    loading: [Option<Layout>; 2],
    pub map: Option<Map>,
    /// HD's mods row under the main menu (`menu::row`): a `UIData`
    /// `AP29` with only the row's controls.
    pub row: Option<Layout>,
    /// The freestyle pick page, `APd2`, the base game's.
    pub freestyle: Option<Layout>,
    /// The message dialog, `AP11`, the base game's.
    pub message: Option<Layout>,
    /// The quit question, `AP04`, the base game's.
    pub quit: Option<Layout>,
    /// The name prompt, `AP35`, the base game's.
    pub prompt: Option<Layout>,
    /// The save and load dialogs, `AP49` and `AP27`, the base game's.
    pub save_game: Option<Layout>,
    pub load_game: Option<Layout>,
    /// The extra quests' list, `APd8`, the base game's.
    pub extras: Option<Layout>,
    /// The Cinematics page: `APd8` in its words (`films`).
    pub films: Option<Layout>,
    /// The high scores, `AP64`, the base game's.
    pub scores: Option<Layout>,
    /// The freestyle setup dialog, `MX17`, the expansion's.
    pub setup: Option<Layout>,
    /// The Build Tree Editor, `MX18`, the expansion's.
    pub tree: Option<Layout>,
    /// Load and Save Freestyle Options, `MX20` and `MX21`, the
    /// expansion's.
    pub load_options: Option<Layout>,
    pub save_options: Option<Layout>,
    /// The multiplayer pages: Multiplayer Setup `AP33`, the session
    /// dialog `AP55`, the lobby `MX19`, as the expansion's datasets hold
    /// them.
    pub protocol: Option<(Layout, Pack)>,
    pub games: Option<Layout>,
    pub session: Option<Layout>,
    pub lobby: Option<Layout>,
    pub quests: Vec<Quest>,
    /// A freestyle game's pictures.
    pub freestyle_game: Option<Quest>,
    pub words: Words,
    pub cloud: Option<Cloud>,
}

/// The pictures read at start: which ones, by the art service's source,
/// and their override names. No pixels: the art service keeps each as a
/// texture, so one is decoded again from its catalogue only as the
/// service asks. Held as RGBA they were 311 MB.
#[derive(Default)]
struct Held {
    /// Each picture's dataset and `menu::state` picture.
    pictures: HashMap<(regent_draw::art::ArtSource, ink::Pic), (Pack, Pic)>,
    /// Each tile's dataset.
    tiles: HashMap<(regent_draw::art::ArtSource, Tile), Pack>,
    keys: HashMap<(regent_draw::art::ArtSource, Picture), Vec<Key>>,
    /// The base game's and the expansion's catalogues, read from.
    catalogues: [Option<Catalogue<'static>>; 2],
}

impl Held {
    /// `pic`, read from `catalogue` as `drawn`, held under `source` with
    /// its override name.
    fn picture(
        &mut self,
        catalogue: &Catalogue<'_>,
        (source, pic): (regent_draw::art::ArtSource, ink::Pic),
        drawn: (Pack, Pic),
    ) {
        let names = regent_draw::art::read::pic_key(catalogue, pic);

        self.pictures.insert((source, pic), drawn);
        self.keys
            .insert((source, Picture::Pic(pic)), names.into_iter().collect());
    }

    /// `pack`'s catalogue, once kept.
    const fn catalogue(&self, pack: Pack) -> Option<&Catalogue<'static>> {
        self.catalogues[pack.at()].as_ref()
    }
}

impl regent_draw::art::Read for Held {
    fn read(&self, source: regent_draw::art::ArtSource, picture: Picture) -> Option<ColorImage> {
        let sprite = match picture {
            Picture::Pic(pic) => self.pictures.get(&(source, pic)).and_then(|&(pack, p)| {
                menu::state(self.catalogue(pack)?, p.record, p.set, p.slot)?.ok()
            }),
            Picture::Tile(tile) => self
                .tiles
                .get(&(source, tile))
                .and_then(|&pack| Sprite::at(self.catalogue(pack)?, tile.0, tile.1)?.ok()),
            Picture::Icon(_) | Picture::Stack(_) => None,
        };

        sprite.map(|s| image(&s))
    }

    fn reach(&self) -> Reach {
        Reach::Part
    }

    fn keys(&self, source: regent_draw::art::ArtSource, picture: Picture) -> Vec<Key> {
        self.keys
            .get(&(source, picture))
            .cloned()
            .unwrap_or_default()
    }
}

/// The pictures read, the room's runs and the fonts, and each dataset's
/// source in the art service the pictures are drawn through.
pub struct Store {
    held: Held,
    /// The portraits the faces behind the menus draw on.
    pub faces: Faces,
    fonts: HashMap<FourCc, Font>,
    runs: HashMap<Slot, Run>,
    sources: [regent_draw::art::ArtSource; 2],
}

/// None read, from no archives.
impl Default for Store {
    fn default() -> Store {
        Store {
            held: Held::default(),
            faces: Faces::default(),
            fonts: HashMap::new(),
            runs: HashMap::new(),
            sources: [regent_draw::art::ArtSource::of(&[]); 2],
        }
    }
}

impl Art {
    /// Read from `hd`, `sdk` its GPL images; `quests` the offered list,
    /// `faces` the faces' seed (`faces::seed`).
    ///
    /// # Errors
    ///
    /// Archives that will not read.
    #[allow(clippy::too_many_lines, reason = "a layout a line")]
    pub fn load(hd: &Path, sdk: &Path, quests: &[Offered], faces: u32) -> Result<Art, String> {
        let [base, expansion] = archives(hd, sdk)?;
        let sources = [&base, &expansion].map(|p| regent_draw::art::ArtSource::of(p));
        // Each archive's bytes read once a process, the quests' and the
        // games' corpora sharing them.
        let base = regent_draw::data::catalogue(&base)?;
        let expansion = regent_draw::data::catalogue(&expansion)?;
        let catalogues = [&base, &expansion];
        let mut book = Book::default();

        for source in [Pack::Base, Pack::Expansion] {
            let catalogue = catalogues[source.at()];
            let text = Text::build(catalogue);
            let layout = |id| menu::layout(catalogue, &text, id).and_then(Result::ok);

            if let Some(protocol) = layout(menu::multiplayer::PROTOCOL) {
                book.protocol = Some((protocol, source));
            }

            book.main[source.at()] = layout(menu::MAIN).map(relabelled);
            book.briefing[source.at()] = layout(menu::BRIEFING);
            book.summary[source.at()] = layout(menu::SUMMARY);
            book.loading[source.at()] = layout(menu::LOADING);

            match source {
                Pack::Base => {
                    book.freestyle = layout(menu::FREESTYLE);
                    book.message = layout(menu::MESSAGE);
                    book.quit = layout(menu::quit::PANEL);
                    book.prompt = layout(menu::NAME_PROMPT);
                    book.save_game = layout(saves::SAVE_PAGE);
                    book.load_game = layout(saves::LOAD_PAGE);
                    book.extras = layout(regent_core::view::menu::extras::PANEL);
                    book.films = book.extras.clone().map(super::films::dressed);
                    book.scores = layout(regent_core::view::menu::scores::PANEL);
                }
                Pack::Expansion => {
                    book.setup = layout(menu::SETUP);
                    book.tree = layout(menu::tree::EDITOR);
                    book.load_options = layout(menu::presets::LOAD_PAGE);
                    book.save_options = layout(menu::presets::SAVE_PAGE);
                    book.games = layout(menu::multiplayer::GAMES);
                    book.session = layout(menu::multiplayer::SESSION);
                    book.lobby = layout(menu::multiplayer::LOBBY);
                }
            }
        }

        book.map = map(&hd.join(crate::panel::UIDATA));
        book.row = row(&hd.join(crate::panel::UIDATA));

        let text = Text::build(&expansion);
        let rules = expansion
            .find(b"DATA", crate::game::load::QUEST_ORDER)
            .and_then(|at| expansion.record(b"DATA", at))
            .and_then(|r| deps::quests(&String::from_utf8_lossy(r.data)).ok())
            .unwrap_or_default();

        book.cloud = book
            .map
            .as_ref()
            .and_then(|m| m.sites.iter().find_map(|s| s.cloud?.art))
            .and_then(|a| cloud(&expansion, a));
        book.quests = quests.iter().map(quest).collect();
        book.freestyle_game = Some(freestyle_game());
        book.words = words(&text, rules, quests);

        let mut ink = Store {
            sources,
            ..Store::default()
        };

        ink.read(&book, &base, &expansion);
        ink.faces = Faces::new(ink.portraits(&expansion), faces);

        for record in FONTS {
            let font = Font::load(&base, record).or_else(|| Font::load(&expansion, record));

            if let Some(font) = font {
                ink.fonts.insert(record, font);
            }
        }

        let cursors = [dialog(), Some(cursor::BUSY)]
            .into_iter()
            .flatten()
            .filter_map(|key| {
                Some((
                    key,
                    regent_draw::art::cursor::picture(&base, key.0, key.1).ok()??,
                ))
            })
            .collect();

        let voices = voices(&expansion, quests);
        let clicks = music::CLICKS
            .into_iter()
            .filter_map(|wave| {
                let at = base.find(b"WAVE", wave)?;

                Some((wave, kept(base.record(b"WAVE", at)?)?))
            })
            .collect();

        let lists = super::freestyle::Lists::load(hd, &expansion)?;
        let videos = videos(&expansion);
        let cinematics = menu::CINEMATICS.iter().map(|p| hd.join(p)).collect();

        ink.held.catalogues = [Some(base), Some(expansion)];

        Ok(Art {
            book,
            ink,
            cursors,
            voices,
            clicks,
            lists,
            videos,
            cinematics,
        })
    }
}

impl Art {
    /// The art service's source the menus' cursors are read from: the
    /// base game's.
    #[must_use]
    pub const fn cursor_source(&self) -> regent_draw::art::ArtSource {
        self.ink.source(Pack::Base)
    }
}

impl Art {
    /// The offered quests now `quests`, as after a mods rescan: their
    /// pictures, titles and briefing voices.
    ///
    /// # Errors
    ///
    /// Archives that will not read.
    pub fn quests_on(&mut self, hd: &Path, sdk: &Path, quests: &[Offered]) -> Result<(), String> {
        let [base, expansion] = archives(hd, sdk)?;
        let base = regent_draw::data::catalogue(&base)?;
        let expansion = regent_draw::data::catalogue(&expansion)?;
        let rules = std::mem::take(&mut self.book.words.rules);

        self.book.quests = quests.iter().map(quest).collect();
        self.book.words = words(&Text::build(&expansion), rules, quests);
        self.ink.read(&self.book, &base, &expansion);
        self.voices = voices(&expansion, quests);
        self.ink.held.catalogues = [Some(base), Some(expansion)];

        Ok(())
    }
}

/// The base game's and the expansion's archives on `hd`, `sdk` their GPL
/// images.
fn archives(hd: &Path, sdk: &Path) -> Result<[Vec<std::path::PathBuf>; 2], String> {
    let install = Install::new(hd, Bytecode::Sdk(sdk));
    let [base, expansion] = [Datasets::Base, Datasets::Expansion].map(|sets| {
        install
            .files(sets)
            .map(|f| f.archives)
            .map_err(|e| e.to_string())
    });

    Ok([base?, expansion?])
}

/// The videos the menus name: CREDITS' and every quest slot's, those
/// shipped (video open takes a `PICT` by id), each read as it
/// plays. **Inferred**: the expansion's catalogue, holding both
/// `cinedata1.cam` and `mx_cinedata1.cam`, finds each as the original's
/// archives do.
fn videos(catalogue: &Catalogue<'static>) -> HashMap<FourCc, cam::Record<'static>> {
    let slots = (0..quest_map::SLOTS.len()).filter_map(quest_map::video);

    menu::CREDITS
        .into_iter()
        .chain(slots)
        .filter_map(|id| {
            Some((
                id,
                *catalogue.record(b"PICT", catalogue.find(b"PICT", id)?)?,
            ))
        })
        .collect()
}

/// A small record's bytes, kept for the process: a click's.
fn kept(record: &cam::Record<'static>) -> Option<&'static [u8]> {
    match record.bytes() {
        Ok(Cow::Borrowed(bytes)) => Some(bytes),
        Ok(Cow::Owned(bytes)) => Some(bytes.leak()),
        Err(e) => {
            warn!("{}: {e}", record.id);
            None
        }
    }
}

/// The map's advisors in their phase, and each offered quest's briefing
/// in its first; a voice not shipped is left out.
/// **Inferred**: the expansion's catalogue, which holds the base game's
/// archives, says what a base quest's own dataset would.
fn voices(catalogue: &Catalogue<'static>, quests: &[Offered]) -> HashMap<FourCc, Said> {
    let sounds = Sounds::of(catalogue).unwrap_or_else(|e| {
        warn!("no voices: {e}");
        Sounds::default()
    });
    let advisor = sound::Phase::Id(quest_map::ADVISOR_PHASE);
    let advisors = [quest_map::INTRODUCTION, quest_map::EXPANSION].map(|s| (s, advisor.clone()));
    let briefings = quests
        .iter()
        .filter_map(|q| menu::voice(q.quest.key))
        .map(|s| (s, sound::Phase::First));

    advisors
        .into_iter()
        .chain(briefings)
        .filter_map(|(sound, phase)| {
            let play = sounds.play(catalogue, &Voice::Sound(sound), &phase, None, 0.0)?;
            let at = catalogue.find(b"WAVE", play.wave)?;
            let wave = *catalogue.record(b"WAVE", at)?;

            Some((sound, Said { play, wave }))
        })
        .collect()
}

/// The front end's cursor, `CUR1` set and slot: `Dial`'s, which
/// `SMenuManager` sets on the dialog manager. Its
/// state callback is always 0, so a hovered button or picture keeps it.
#[must_use]
pub fn dialog() -> Option<(u32, usize)> {
    let dial = cursor::Cursor::Dialog;

    dial.set().map(|set| (set, dial.slot()))
}

/// The main menu's CINEMATIC named for its list. openregent's
/// (`menu::films`); the hot key letter stays its 'I'.
fn relabelled(mut main: Layout) -> Layout {
    let id = menu::Button::Cinematic.id();

    if let Some(c) = main.controls.iter_mut().find(|c| c.id == Some(id)) {
        c.text = Some(CINEMATICS_LABEL.to_owned());
    }

    main
}

/// [`relabelled`]'s words.
const CINEMATICS_LABEL: &str = "CINEMATICS";

impl Book {
    /// The main menu's panel: base or expansion.
    #[must_use]
    pub const fn main(&self, source: Pack) -> Option<&Layout> {
        self.main[source.at()].as_ref()
    }

    /// The briefing's panel for a quest of `source`.
    #[must_use]
    pub const fn briefing(&self, source: Pack) -> Option<&Layout> {
        self.briefing[source.at()].as_ref()
    }

    /// The summary's panel for a quest of `source`.
    #[must_use]
    pub const fn summary(&self, source: Pack) -> Option<&Layout> {
        self.summary[source.at()].as_ref()
    }

    /// The loading page for a game of `source`.
    #[must_use]
    pub const fn loading(&self, source: Pack) -> Option<&Layout> {
        self.loading[source.at()].as_ref()
    }
}

/// Every picture `layout` names, each in its first slots and lit.
fn named(layout: &Layout) -> Vec<Pic> {
    std::iter::once(&layout.panel)
        .chain(&layout.controls)
        .filter_map(|c| c.art.map(|art| (art, c.kind)))
        .flat_map(|((record, set), kind)| {
            // A scroll bar names its up arrow; its down arrow and thumb
            // follow it. A combo box's parts follow its own set.
            let last = match kind {
                Some(Kind::Scrollbar) => set + scroll::THUMB,
                Some(Kind::Combobox) => set + menu::setup::LIST_SCROLL,
                _ => set,
            };

            (set..=last).flat_map(move |set| {
                (0..SLOTS)
                    .chain([room::LIT])
                    .map(move |s| Pic::of(record, set).slot(s))
            })
        })
        .collect()
}

impl Store {
    /// The panels' pictures, from their source's catalogue.
    fn layout_pictures(book: &Book) -> Vec<(Pack, Pic)> {
        let mut wanted = Vec::new();

        for source in [Pack::Base, Pack::Expansion] {
            let layouts = [
                &book.main[source.at()],
                &book.briefing[source.at()],
                &book.summary[source.at()],
                &book.loading[source.at()],
                &book.row,
                &book.freestyle,
                &book.message,
                &book.quit,
                &book.prompt,
                &book.save_game,
                &book.load_game,
                &book.extras,
                &book.scores,
                &book.setup,
                &book.tree,
                &book.load_options,
                &book.save_options,
                &book.games,
                &book.session,
                &book.lobby,
            ];

            for layout in layouts.into_iter().flatten() {
                wanted.extend(named(layout).into_iter().map(|p| (source, p)));
            }
        }

        if let Some((layout, source)) = &book.protocol {
            wanted.extend(named(layout).into_iter().map(|pic| (*source, pic)));
        }

        wanted
    }

    /// Every picture `book` names, from its source's catalogue.
    fn read(&mut self, book: &Book, base: &Catalogue<'_>, expansion: &Catalogue<'_>) {
        let mut wanted = Self::layout_pictures(book);

        if let Some(c) = &book.cloud {
            wanted.push((Pack::Expansion, c.mark));
        }

        if let Some(map) = &book.map {
            let lit = map.sites.iter().filter_map(|s| s.lit.and_then(|l| l.art));
            let lit = lit.flat_map(|a| (0..SLOTS).map(move |s| Pic::of(a.record, a.set).slot(s)));

            wanted.extend(named(&map.layout).into_iter().map(|p| (Pack::Expansion, p)));
            wanted.extend(lit.map(|p| (Pack::Expansion, p)));
            wanted.push((Pack::Expansion, JEWEL));
            wanted.push((Pack::Expansion, JEWEL.slot(room::LIT)));
            wanted.extend(
                [quest_map::DOWNLOAD_BANNER, quest_map::FREESTYLE_BANNER]
                    .map(|b| (Pack::Expansion, Pic::of(b.pog.record, b.pog.set))),
            );
        }

        for q in book.quests.iter().chain(&book.freestyle_game) {
            wanted.extend(
                [q.icon, q.zoom, q.segue]
                    .into_iter()
                    .flatten()
                    .map(|p| (q.source, p)),
            );
        }

        for source in [Pack::Base, Pack::Expansion] {
            let catalogue = match source {
                Pack::Base => base,
                Pack::Expansion => expansion,
            };
            let pictures = book.main[source.at()]
                .iter()
                .flat_map(|l| &l.controls)
                .filter(|c| moving(c) || c.id == Some(room::EXIT))
                .filter_map(|c| c.art);

            for (record, set) in pictures {
                for slot in [room::REST, room::LIT] {
                    self.run(catalogue, (source, record, set, slot));
                }
            }
        }

        // The map's compass moves while hovered.
        let compass = book
            .map
            .as_ref()
            .and_then(|m| m.layout.control(quest_map::FREESTYLE)?.art);

        let runs = compass.into_iter().flat_map(|(record, set)| {
            [room::REST, room::LIT].map(|slot| (Pack::Expansion, record, set, slot))
        });

        runs.for_each(|at| self.run(expansion, at));

        if let Some(Cloud {
            swirl: (record, set),
            ..
        }) = book.cloud
        {
            self.run(expansion, (Pack::Expansion, record, set, 0));
        }

        for (source, pic) in wanted {
            let catalogue = match source {
                Pack::Base => base,
                Pack::Expansion => expansion,
            };

            let key = (self.source(source), pic.pic());

            // Decoded to know it reads; drawn later from a decode again.
            if !self.held.pictures.contains_key(&key)
                && let Some(Ok(_)) = menu::state(catalogue, pic.record, pic.set, pic.slot)
            {
                self.held.picture(catalogue, key, (source, pic));
            }
        }
    }

    /// Read every portrait the faces may show (`faces::pool`), 100 by 100;
    /// the ones read.
    fn portraits(&mut self, catalogue: &Catalogue<'_>) -> Vec<Pic> {
        let side = faces::CELL - 2 * faces::INSET;
        let source = self.source(Pack::Expansion);
        let mut read = Vec::new();

        for pic in faces::pool(catalogue)
            .into_iter()
            .map(super::faces::portrait)
        {
            let sprite = menu::still(catalogue, pic.record, pic.set).and_then(Result::ok);

            if let Some(s) = sprite
                && (usize::from(s.width), usize::from(s.height)) == (side, side)
            {
                // `menu::still` is slot 0.
                let still = (Pack::Expansion, pic.slot(0));

                self.held.picture(catalogue, (source, pic.pic()), still);
                read.push(pic);
            }
        }

        read
    }

    /// Read room picture slot `at` and its tiles, each tile once.
    fn run(&mut self, catalogue: &Catalogue<'_>, at: Slot) {
        let (source, record, set, slot) = at;

        if let Some(Ok(strip)) = menu::strip(catalogue, record, set, slot) {
            let tiles = regent_draw::art::read::tiles(&strip.frames);
            let named = strip.frames.iter().enumerate().filter_map(|(frame, f)| {
                let tile = f.tile()?;
                let file = catalogue.archives.get(tile.0)?.0;
                let by_frame = u32::try_from(set)
                    .ok()
                    .and_then(|set| Key::of(catalogue, record, set, slot, frame));

                Some((tile, by_frame.into_iter().chain([Key::tile(file, tile.1)])))
            });

            // A room frame's tile by its frame's name, else its own.
            for (tile, keys) in named {
                self.held
                    .keys
                    .entry((self.source(source), Picture::Tile(tile)))
                    .or_insert_with(|| keys.collect());
            }

            for &tile in &tiles {
                let key = (self.source(source), tile);

                if !self.held.tiles.contains_key(&key)
                    && let Some(Ok(_)) = Sprite::at(catalogue, tile.0, tile.1)
                {
                    self.held.tiles.insert(key, source);
                }
            }

            self.runs.insert(
                at,
                Run {
                    tiles,
                    playback: strip.playback,
                },
            );
        }
    }

    /// Room picture slot `at`, if read.
    pub fn strip(&self, at: Slot) -> Option<&Run> {
        self.runs.get(&at)
    }

    /// `source`'s in the art service: its archives'.
    #[must_use]
    pub const fn source(&self, source: Pack) -> regent_draw::art::ArtSource {
        self.sources[source.at()]
    }

    /// The art service's sources of both datasets: the menus'.
    #[must_use]
    pub const fn sources(&self) -> [regent_draw::art::ArtSource; 2] {
        self.sources
    }

    /// `pic` from `source`'s archives, plain.
    #[must_use]
    pub const fn want(&self, source: Pack, pic: Pic) -> Want {
        Want::of(self.source(source), Picture::Pic(pic.pic()))
    }

    /// Frame `frame` of room picture slot `at`, if read, drawn at
    /// `place` on `scene`; its other frames made ahead
    /// (`art::Art::strip`).
    pub fn frame(
        &self,
        art: &mut regent_draw::art::Art,
        ctx: &egui::Context,
        (at, frame): (Slot, usize),
        (place, scene): (Place, Scene),
    ) -> Option<Answer> {
        let source = self.source(at.0);
        let frames: Vec<Want> = self
            .runs
            .get(&at)?
            .tiles
            .iter()
            .map(|&tile| Want::of(source, Picture::Tile(tile)))
            .collect();

        art.strip(ctx, &self.held, &frames, frame, (place, scene))
    }

    /// `want`'s own size, layout pixels (`art::Art::size`).
    pub fn size(
        &self,
        art: &mut regent_draw::art::Art,
        ctx: &egui::Context,
        want: Want,
    ) -> Option<Vec2> {
        art.size(ctx, &self.held, want)
    }

    /// `want` drawn at `place` (`art::Art::get`).
    pub fn get(
        &self,
        art: &mut regent_draw::art::Art,
        ctx: &egui::Context,
        want: Want,
        place: Place,
    ) -> Option<Answer> {
        art.get(ctx, &self.held, want, place)
    }

    /// The quest map's levels (`art::Art::levels`).
    pub fn levels<'a>(
        &self,
        art: &'a mut regent_draw::art::Art,
        (source, map): (Pack, Pic),
        quests: impl Iterator<Item = (levels::Quest, Pic)>,
        under: impl Iterator<Item = ([usize; 2], Pic)>,
        when: When,
    ) -> Option<&'a mut Levels> {
        let quests = quests.map(|(quest, pic)| (quest, Picture::Pic(pic.pic())));
        let under = under.map(|(at, pic)| (at, Picture::Pic(pic.pic())));
        let map = (self.source(source), Picture::Pic(map.pic()));

        art.levels(&self.held, map, quests, under, when)
    }

    /// Font `record`.
    pub fn font(&self, record: FourCc) -> Option<Font> {
        self.fonts.get(&record).copied()
    }
}

/// `APdb` and its texts from the `UIData` file at `path`; `None` without.
fn map(path: &Path) -> Option<Map> {
    let panel = quest_map::load(path).ok()?;
    let library = Library::from_files(&[path.to_path_buf()]).ok()?;
    let catalogue = library.catalogue().ok()?;
    let text = Text::build(&catalogue);
    // Its palette is the main data's, beside the `UIData` file.
    let palettes = Library::from_files(&[path.with_file_name(PALETTES)]).ok()?;
    let style = Style::find(&palettes.catalogue().ok()?, Style::palette(&panel));
    let layout = menu::resolved(&panel, &text, quest_map::PANEL, style);
    let (w, h) = quest_map::frame_size(&panel)?;
    let pieces = quest_map::frame(&panel);
    let full = quest_map::map(&panel)?.rect;

    #[allow(clippy::cast_precision_loss, reason = "a panel's pixels")]
    Some(Map {
        layout,
        sites: quest_map::sites(&panel),
        pieces,
        frame: vec2(w as f32, h as f32),
        size: vec2(full.width as f32, full.height as f32),
        label: quest_map::text_rect(&panel, quest_map::FREESTYLE_TEXT).map(super::fit::rect),
    })
}

/// HD's mods row from the `UIData` file at `path`: its `AP29` with only
/// the row's controls (`menu::row`), texts and palette as [`map`]'s;
/// `None` without.
fn row(path: &Path) -> Option<Layout> {
    let library = Library::from_files(&[path.to_path_buf()]).ok()?;
    let catalogue = library.catalogue().ok()?;
    let text = Text::build(&catalogue);
    let record = catalogue.record(b"SMNU", catalogue.find(b"SMNU", menu::MAIN)?)?;
    let panel = Panel::parse(record.data).ok()?;
    let palettes = Library::from_files(&[path.with_file_name(PALETTES)]).ok()?;
    let style = Style::find(&palettes.catalogue().ok()?, Style::palette(&panel));
    let mut layout = menu::resolved(&panel, &text, menu::MAIN, style);

    layout.controls = menu::row::controls(&layout).cloned().collect();

    Some(layout)
}

/// The cloud of `art`, its "?" placed by its layer's pose and its
/// frame's nudge, each tile's anchor taken off. **Inferred**: the
/// interface draws a set's layers as the map draws a unit's.
fn cloud(catalogue: &Catalogue<'_>, art: quest_map::Art) -> Option<Cloud> {
    let sets = imagery::sets(catalogue, catalogue.find(b"IMAG", art.record)?).ok()?;
    let corner = |id: i32| -> Option<Vec2> {
        let set = sets.iter().find(|a| i64::from(a.id) == i64::from(id))?;
        let pose = set.poses.first()?;
        let nudge = pose.nudges.first().copied().unwrap_or_default();
        let anchor = match pose.frames.first()? {
            FrameSource::Tile { archive, index, .. } => {
                Sprite::at(catalogue, *archive, *index)?.ok()?.anchor
            }
            _ => (0, 0),
        };

        Some(vec2(
            f32::from(pose.x) + f32::from(nudge.0) - f32::from(anchor.0),
            f32::from(pose.y) + f32::from(nudge.1) - f32::from(anchor.1),
        ))
    };
    let mark = art.set + NEXT_LAYER;

    Some(Cloud {
        swirl: (art.record, art.set),
        mark: Pic::of(art.record, mark),
        at: corner(mark)? - corner(art.set)?,
    })
}

/// An offered quest's pictures.
fn quest(offered: &Offered) -> Quest {
    let key = offered.quest.key;
    let release = if offered.quest.expansion() {
        Release::Expansion
    } else {
        Release::Base
    };
    let pic = |art: Option<(FourCc, u16)>| art.map(|(record, set)| Pic::of(record, i32::from(set)));

    Quest {
        source: Pack::of(release),
        // A mod's quest has no place on the map, though its key be a
        // shipped quest's: it is listed with the extras.
        slot: quest_map::slot(key).filter(|_| offered.origin == Origin::Install),
        icon: pic(menu::icon(key, release, World::Quest)),
        zoom: pic(menu::zoom(key, release, World::Quest)),
        segue: pic(menu::segue(key, release, World::Quest)),
    }
}

/// A freestyle game's pictures: the expansion's, as openregent's freestyle
/// games load its dataset. **Inferred**: HD's custom game does too.
fn freestyle_game() -> Quest {
    let (release, world) = (Release::Expansion, World::Freestyle);
    let pic = |art: Option<(FourCc, u16)>| art.map(|(record, set)| Pic::of(record, i32::from(set)));

    Quest {
        source: Pack::of(release),
        slot: None,
        icon: pic(menu::icon(FREESTYLE_QUEST, release, world)),
        zoom: pic(menu::zoom(FREESTYLE_QUEST, release, world)),
        segue: pic(menu::segue(FREESTYLE_QUEST, release, world)),
    }
}

/// The words the menus need; `text` the expansion's tables, which hold
/// the base game's too, `rules` its `QDEP`.
fn words(text: &Text<'_>, rules: Vec<deps::Quest>, offered: &[Offered]) -> Words {
    let wanted = quest_map::TITLE_BAR_TEXTS
        .into_iter()
        .chain([
            quest_map::ERASE_TITLE_TEXT,
            quest_map::ERASE_TEXT,
            menu::NAME_TEXT,
            quest_map::HINT_TITLE_TEXT,
            quest_map::DOWNLOAD_BANNER.title,
            quest_map::FREESTYLE_BANNER.title,
            day::DAY_TEXT.index,
            day::PLURAL.index,
        ])
        .chain(
            [
                saves::DESCRIPTION_TEXT,
                saves::LAN_DESCRIPTION_TEXT,
                saves::PARTICIPANTS_TEXT,
                saves::FREESTYLE_TEXT,
                saves::DELETE_TITLE,
                saves::DELETE_TEXT,
            ]
            .map(|r| r.index),
        )
        .chain(quest_map::HINTS)
        .chain(super::lan::TEXTS)
        .chain([super::lan::CHANGED_TEXT, super::lan::PLAYERS_TEXT])
        .chain(Category::ALL.into_iter().map(Category::row_text))
        .chain(Category::ALL.into_iter().map(Category::definition_text));
    let gmtx = wanted
        .filter_map(|i| Some((i, text.gmtx(i)?.into_owned())))
        .collect();
    let listed = offered.iter().map(|o| (o.quest.key, o.title.clone()));
    let named = rules
        .iter()
        .filter_map(|r| Some((r.id, text.quest(r.id)?.into_owned())));
    let titles = listed.chain(named).collect();
    let months = text
        .table(&saves::MONTHS)
        .map(|t| t.iter().map(std::borrow::Cow::into_owned).collect())
        .unwrap_or_default();

    Words {
        rules,
        gmtx,
        titles,
        months,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_panel_art_is_queued() {
        let art = (FourCc(*b"INMp"), 1000);
        let control_art = (FourCc(*b"IX35"), 1000);
        let layout = Layout {
            id: menu::multiplayer::PROTOCOL,
            panel: ShownControl {
                art: Some(art),
                ..ShownControl::default()
            },
            centred: true,
            controls: vec![ShownControl {
                art: Some(control_art),
                ..ShownControl::default()
            }],
            style: None,
        };
        let book = Book {
            protocol: Some((layout, Pack::Expansion)),
            ..Book::default()
        };

        let wanted = Store::layout_pictures(&book);

        assert!(wanted.contains(&(Pack::Expansion, Pic::of(art.0, art.1))));
        assert!(wanted.contains(&(Pack::Expansion, Pic::of(control_art.0, control_art.1))));
    }

    /// Every picture and tile held reads again from its catalogue: holding
    /// no pixels loses none.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn every_held_picture_reads_from_its_catalogue() {
        use regent_draw::art::Read;

        let install = crate::files::env::Install::from_env().expect("MAJESTY_HD");
        let quests = crate::quests(&install).expect("quests");
        let art = Art::load(&install.hd, &install.sdk, &quests, 0).expect("the menus' art");
        let held = &art.ink.held;
        let reads = |source, picture| held.read(source, picture).is_some();

        assert!(!held.pictures.is_empty() && !held.tiles.is_empty());
        assert!(
            held.pictures
                .keys()
                .all(|&(s, p)| reads(s, Picture::Pic(p)))
        );
        assert!(held.tiles.keys().all(|&(s, t)| reads(s, Picture::Tile(t))));
    }
}
