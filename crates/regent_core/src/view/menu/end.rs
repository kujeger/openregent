//! The game's end on screen: state 8's steps,
//! then the end panels (`EndGameController`, `EndSummaryController`).
//! **Binary** unless marked.
//!
//! ```text
//!  won   advisor ──> AP11 "Game Over" ──OK──> view to the point,
//!                    #EndGameFadeDelay ──> fade ──> AP60
//!  lost  advisor ──────────────────────────────> fade ──> AP61
//!  fade: a step each 75 real ms; at 15 and the advisor done, the panel
//!
//!  AP60 ──STATISTICS──> AP84 ──CONTINUE PLAYING──> state 3, the game on
//!   └──CONTINUE PLAYING──> state 3         └──MAIN MENU──> the menus
//!   └──MAIN MENU──> the menus
//!  AP61 ──STATISTICS──> AP83 ──RESTART──> game restart
//!   └──RESTART──> game restart             └──MAIN MENU──> the menus
//!   └──MAIN MENU──> the menus
//! ```
//!
//! With every expansion quest won and its ending not yet seen, MAIN
//! MENU, RESTART and CONTINUE PLAYING on `AP60` or `AP61` first ask
//! `MV07` ([`Panel::ending`]). openregent's statistics pages ask too
//! ([`Panel::films`]).
//!
//! No statistics page leads back. A network game hides CONTINUE PLAYING
//! and RESTART and names MAIN MENU `GMTX` 336 "Return
//! to Lobby", which goes back to the session's lobby
//! (menus return: next app state 6, not the menus) ([`Play`]).

use std::borrow::Cow;

use crate::content::quest::Messages;
use crate::data::fourcc::FourCc;
use crate::data::text::Text;
use crate::sim::freestyle::Setup;
use crate::sim::world::defeat::{End, FADE_STEP_MS, FADE_STEPS};
use crate::view::menu::film::Film;
use crate::view::menu::quest_map::{Progress, SLOTS};
use crate::view::panel::scroll;

/// The panels: verdict and statistics, won and lost
/// (game end step step 4, `EndGameController` 0x138b).
pub const WON: FourCc = FourCc(*b"AP60");
pub const LOST: FourCc = FourCc(*b"AP61");
pub const WON_STATISTICS: FourCc = FourCc(*b"AP84");
pub const LOST_STATISTICS: FourCc = FourCc(*b"AP83");

/// The panels' controls.
pub mod id {
    /// menus return: hot key M.
    pub const MAIN_MENU: i32 = 5000;
    /// game restart: R.
    pub const RESTART: i32 = 5002;
    /// The statistics page: S.
    pub const STATISTICS: i32 = 5003;
    /// Back to state 3: C.
    pub const CONTINUE: i32 = 5010;

    /// The quest's pog
    /// and name.
    pub use crate::view::menu::ids::POG;
    pub use crate::view::menu::ids::TITLE;
    /// The verdict: `QEND` on `AP60` (control 3), `LOSX` on `AP61`
    /// (4055), the scroll bar ([`crate::view::menu::ids::SCROLL_BAR`])
    /// set on it.
    pub const WON_TEXT: i32 = 3;
    pub const LOST_TEXT: i32 = 4055;
    /// "MAP NUMBER" and its number.
    pub const MAP_LABEL: i32 = 6678;
    pub const MAP_NUMBER: i32 = 6766;
    /// "round end quest picture", `INTM` 1005, 750 × 375 in an 81 × 88
    /// rectangle. **Game**: no capture shows it; **Inferred** hidden.
    pub const UNSHOWN: i32 = 7460;
}

/// The "Game Over" box: `AP11`, title `GMTX` 310, text 309 "You are
/// victorious.", OK alone (step 2).
pub const GAME_OVER: FourCc = FourCc(*b"AP11");
pub const GAME_OVER_TITLE: usize = 310;
pub const VICTORIOUS: usize = 309;

/// Real ms a victory waits after the view moves before the fade, a GPL
/// constant; [`FADE_DELAY_MS`] in `globals.gpl`.
pub const FADE_DELAY: &str = "#EndGameFadeDelay";
pub const FADE_DELAY_MS: u32 = 1500;

/// MAIN MENU's words in a network game (`EndGameController`,
/// `EndSummaryController`).
pub const RETURN_TO_LOBBY: usize = 336;

/// The expansion's ending, the `PICT` video an end panel's press asks
/// first ([`Panel::ending`]); CINEMATIC's second (`menu::CINEMATIC`).
pub const ENDING: FourCc = FourCc(*b"MV07");

/// Whether a press asking the [`ENDING`] marks it seen
/// (`Seen::ending`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    Seen,
    /// RESTART: asked again at the next press.
    Unseen,
}

/// Whose game ended: one machine's, or a network game's, whose panels
/// offer no going on and lead back to the lobby.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Play {
    Alone,
    Network,
}

/// A panel's page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    /// `AP60`, `AP61`: the quest and its verdict.
    Verdict,
    /// `AP84`, `AP83`: the statistics.
    Statistics,
}

/// A panel's button.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    MainMenu,
    Restart,
    Statistics,
    Continue,
}

impl Button {
    #[must_use]
    pub const fn id(self) -> i32 {
        match self {
            Button::MainMenu => id::MAIN_MENU,
            Button::Restart => id::RESTART,
            Button::Statistics => id::STATISTICS,
            Button::Continue => id::CONTINUE,
        }
    }
}

/// Where a press goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    To(Panel),
    /// menus return: the main menu.
    MainMenu,
    /// game restart: a quest's summary, else the freestyle pick
    /// page.
    Restart,
    /// App state 3: the game goes on (`World::play_on`).
    PlayOn,
    /// A network game's MAIN MENU: back to the session's lobby.
    Lobby,
}

/// One of the four panels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Panel {
    pub end: End,
    pub page: Page,
}

impl Panel {
    /// The panel step 4 opens.
    #[must_use]
    pub const fn of(end: End) -> Panel {
        Panel {
            end,
            page: Page::Verdict,
        }
    }

    #[must_use]
    pub const fn record(self) -> FourCc {
        match (self.end, self.page) {
            (End::Won, Page::Verdict) => WON,
            (End::Lost, Page::Verdict) => LOST,
            (End::Won, Page::Statistics) => WON_STATISTICS,
            (End::Lost, Page::Statistics) => LOST_STATISTICS,
        }
    }

    /// Its buttons in a game `play`, as its `SMNU` holds them: a won
    /// game goes on, a lost one restarts; a network game does neither.
    #[must_use]
    pub const fn buttons(self, play: Play) -> &'static [Button] {
        match (self.end, self.page, play) {
            (End::Won, Page::Verdict, Play::Alone) => {
                &[Button::Statistics, Button::Continue, Button::MainMenu]
            }
            (End::Lost, Page::Verdict, Play::Alone) => {
                &[Button::Statistics, Button::Restart, Button::MainMenu]
            }
            (End::Won, Page::Statistics, Play::Alone) => &[Button::Continue, Button::MainMenu],
            (End::Lost, Page::Statistics, Play::Alone) => &[Button::Restart, Button::MainMenu],
            (_, Page::Verdict, Play::Network) => &[Button::Statistics, Button::MainMenu],
            (_, Page::Statistics, Play::Network) => &[Button::MainMenu],
        }
    }

    /// MAIN MENU's words where not its own: a network game's
    /// [`RETURN_TO_LOBBY`].
    #[must_use]
    pub const fn main_menu_text(play: Play) -> Option<usize> {
        match play {
            Play::Alone => None,
            Play::Network => Some(RETURN_TO_LOBBY),
        }
    }

    /// The verdict's text box; none on a statistics page.
    #[must_use]
    pub const fn text(self) -> Option<i32> {
        match (self.end, self.page) {
            (End::Won, Page::Verdict) => Some(id::WON_TEXT),
            (End::Lost, Page::Verdict) => Some(id::LOST_TEXT),
            (_, Page::Statistics) => None,
        }
    }

    /// Where `button` goes in a game `play` (`EndGameController`,
    /// `EndSummaryController`); `None` for one it does not show.
    #[must_use]
    pub fn after(self, button: Button, play: Play) -> Option<Step> {
        let step = match (button, play) {
            (Button::MainMenu, Play::Alone) => Step::MainMenu,
            (Button::MainMenu, Play::Network) => Step::Lobby,
            (button, _) => self.alone(button),
        };

        self.buttons(play).contains(&button).then_some(step)
    }

    /// Whether `button` first asks the [`ENDING`], and marks it seen:
    /// only on `AP60` and
    /// `AP61`, the statistics pages never,
    /// and only while [`Progress::ending_due`]. Won or lost, alone or
    /// a network game's "Return to Lobby" alike.
    ///
    /// ```text
    ///  MAIN MENU         plays MV07, marked seen and saved
    ///  RESTART           plays MV07, not marked
    ///  CONTINUE PLAYING  the cine library opens: MV07, marked seen;
    ///                    else AP11 GMTX 351, 352, not marked
    /// ```
    ///
    /// Video play finds `MV07` only in an open archive,
    /// and only `mx_cinedata2.dat` and
    /// `mx_cinedata3.dat` hold it, which CINEMATIC alone opens,
    /// closing it after. CONTINUE opens the
    /// base's. So in HD no press shows it: the
    /// datasets' `.cam`s and the base's cinematics lack it
    /// (`tests/video.rs`). HD installs the base's cinematics, so the CD
    /// box (351 "CD-ROM", 352 "Please insert the Majesty CD.") never
    /// shows. The press only marks.
    #[must_use]
    pub fn ending(self, button: Button, progress: &Progress) -> Option<Mark> {
        Self::asks(self.page, button).filter(|_| progress.ending_due())
    }

    /// The films `button` asks first, each with its mark: the north's,
    /// as [`Panel::ending`] asks it on the verdict. openregent's: a
    /// statistics page asks as its verdict would, where the original
    /// asks nothing (the user's decision). A game
    /// `play` on a network asks none: the others would wait. The original
    /// asks there too, and nothing shows.
    #[must_use]
    pub fn films(self, button: Button, progress: &Progress, play: Play) -> Vec<(Film, Mark)> {
        let verdict = Panel {
            page: Page::Verdict,
            ..self
        };
        let north = verdict
            .ending(button, progress)
            .map(|mark| (Film::North, mark));

        match play {
            Play::Alone => north.into_iter().collect(),
            Play::Network => Vec::new(),
        }
    }

    /// The mark a press on `page` gives a film due.
    const fn asks(page: Page, button: Button) -> Option<Mark> {
        match (page, button) {
            (Page::Verdict, Button::MainMenu | Button::Continue) => Some(Mark::Seen),
            (Page::Verdict, Button::Restart) => Some(Mark::Unseen),
            _ => None,
        }
    }

    /// Where `button` goes in one machine's game.
    const fn alone(self, button: Button) -> Step {
        match button {
            Button::MainMenu => Step::MainMenu,
            Button::Restart => Step::Restart,
            Button::Continue => Step::PlayOn,
            Button::Statistics => Step::To(Panel {
                page: Page::Statistics,
                ..self
            }),
        }
    }
}

/// The control end stat row fills with statistics row `row`
/// (`World::statistics`), its return: none for a heading or a rule.
/// `EndSummaryController` asks rows 1 to 25 but 10, 11, 15, 21, 23
/// and 25; row 16, "Heroes", returns 0.
#[must_use]
pub const fn stat_control(row: usize) -> Option<i32> {
    match row {
        1 => Some(4201),
        2 => Some(4230),
        3 => Some(4231),
        4 => Some(4202),
        5 => Some(46),
        6 => Some(4203),
        7 => Some(4204),
        8 => Some(4205),
        9 => Some(4206),
        12 => Some(4207),
        13 => Some(4208),
        14 => Some(4209),
        17 => Some(4210),
        18 => Some(4211),
        19 => Some(4212),
        20 => Some(4213),
        22 => Some(4214),
        24 => Some(4215),
        _ => None,
    }
}

/// The map number the panels show: a custom game's,
/// its settlement not a random pick (freestyle cfg settlement
/// against `BEGI`, `ADVA`, `EXPE`); none for a quest.
#[must_use]
pub fn map_number(custom: Option<&Setup>) -> Option<u32> {
    custom
        .filter(|s| !scroll::RANDOM.iter().any(|(c, _)| *c == s.settlement()))
        .map(Setup::seed)
}

/// The strings a quest's end message falls back to where `QEND` or
/// `LOSX` has none.
pub const WIN_MESSAGE: &str = "IDTXT_QUEST_WIN_MESSAGE";
pub const LOSS_MESSAGE: &str = "IDTXT_QUEST_LOSS_MESSAGE";

/// The base game's freestyle key in `QEND` and `LOSX`, the last ask.
const FREESTYLE_VERDICT: FourCc = FourCc(*b"FRE0");

/// The verdict's text (quest win verdict won, `EndGameController`
/// lost), each step asked only where the one before gave nothing or an
/// empty text:
///
/// ```text
///  1. QEND / LOSX by the quest's key     a quest-map slot's key alone
///  2. the quest's own <Win> / <Loss>     (content::quest::Messages)
///     else the strings' WIN_MESSAGE / LOSS_MESSAGE
///  3. QEND / LOSX by FRE0
/// ```
///
/// So Wrath of Krolm, Vampiric Revenge and every Workshop quest take
/// step 2: a mod's or quest's `<Strings>` say it. `quest` is `None`
/// for a custom game, keyed `FRE1` ([`End::verdict`]), no slot's key.
/// Step 2 needs the quest's record; every game
/// has one, a custom game the install's `FRE1` definition
/// (**Inferred**: quest dir scan takes every `.mqxml`). The
/// expansion's own tables lack `FRE0`, but the base's, merged under
/// them ([`MERGED`](crate::data::text::MERGED)), hold it. Without the
/// base's, `FRE1` (**Inferred**, [`End::verdict`]).
#[must_use]
pub fn verdict(text: &Text<'_>, end: End, quest: Option<FourCc>, own: &Messages) -> String {
    let (table, freestyle) = end.verdict();
    let code = quest.unwrap_or(freestyle);
    let keyed = |key| text.keyed(table, key).filter(|t| !t.is_empty());
    let (own, message) = match end {
        End::Won => (&own.win, WIN_MESSAGE),
        End::Lost => (&own.loss, LOSS_MESSAGE),
    };

    SLOTS
        .contains(&code)
        .then(|| keyed(code))
        .flatten()
        .map(Cow::into_owned)
        .or_else(|| own.clone().filter(|t| !t.is_empty()))
        .or_else(|| {
            text.strings()
                .of(message)
                .filter(|t| !t.is_empty())
                .map(str::to_owned)
        })
        .or_else(|| {
            keyed(FREESTYLE_VERDICT)
                .or_else(|| keyed(freestyle))
                .map(Cow::into_owned)
        })
        .unwrap_or_default()
}

/// Where state 8 stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    /// Won: the "Game Over" box, until OK.
    Told,
    /// Fading: `steps` taken, the last at real ms `last`.
    Fading { last: u64, steps: u32 },
    /// The panel.
    Shown(Page),
}

/// State 8 for one end, in real ms.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Curtain {
    pub end: End,
    pub stage: Stage,
}

impl Curtain {
    /// Step 0, at real ms `now`: won, the box; lost, the fade from now.
    #[must_use]
    pub const fn new(end: End, now: u64) -> Curtain {
        let stage = match end {
            End::Won => Stage::Told,
            End::Lost => Stage::Fading {
                last: now,
                steps: 0,
            },
        };

        Curtain { end, stage }
    }

    /// The box's OK at `now` (step 3): the fade `delay` ms on, the view
    /// moved; with no point the record's own value stands, and the fade
    /// starts at once. **Inferred**: is 0 there, as the record's
    /// constructor leaves it.
    #[must_use]
    pub const fn told(self, now: u64, delay: Option<u64>) -> Curtain {
        let last = match delay {
            Some(ms) => now + ms,
            None => 0,
        };

        match self.stage {
            Stage::Told => Curtain {
                stage: Stage::Fading { last, steps: 0 },
                ..self
            },
            _ => self,
        }
    }

    /// Step 1 at `now`: a step once more than [`FADE_STEP_MS`] past the
    /// last; at [`FADE_STEPS`], with the advisor done, the panel.
    #[must_use]
    pub const fn tick(self, now: u64, advisor: Advisor) -> Curtain {
        match self.stage {
            Stage::Fading { last, steps } => {
                let (last, steps) = if now > last + FADE_STEP_MS as u64 {
                    (now, steps + 1)
                } else {
                    (last, steps)
                };
                let stage = match advisor {
                    Advisor::Done if steps >= FADE_STEPS => Stage::Shown(Page::Verdict),
                    _ => Stage::Fading { last, steps },
                };

                Curtain { stage, ..self }
            }
            _ => self,
        }
    }

    /// The panel shown, if any.
    #[must_use]
    pub const fn panel(self) -> Option<Panel> {
        match self.stage {
            Stage::Shown(page) => Some(Panel {
                end: self.end,
                page,
            }),
            _ => None,
        }
    }

    /// The fade's steps so far, at most [`FADE_STEPS`]: none before it,
    /// all under the panel.
    #[must_use]
    pub const fn shade(self) -> u32 {
        match self.stage {
            Stage::Told => 0,
            Stage::Fading { steps, .. } if steps < FADE_STEPS => steps,
            _ => FADE_STEPS,
        }
    }
}

/// Whether the advisor's line still plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Advisor {
    Speaking,
    Done,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_won_game_goes_on_and_a_lost_one_restarts() {
        let won = Panel::of(End::Won);
        let lost = Panel::of(End::Lost);
        let stats = |p: Panel| Panel {
            page: Page::Statistics,
            ..p
        };

        assert_eq!(won.record(), WON);
        assert_eq!(lost.record(), LOST);
        assert_eq!(stats(won).record(), WON_STATISTICS);
        assert_eq!(stats(lost).record(), LOST_STATISTICS);

        assert_eq!(
            won.after(Button::Statistics, Play::Alone),
            Some(Step::To(stats(won)))
        );
        assert_eq!(won.after(Button::Continue, Play::Alone), Some(Step::PlayOn));
        assert_eq!(won.after(Button::Restart, Play::Alone), None);
        assert_eq!(
            lost.after(Button::Restart, Play::Alone),
            Some(Step::Restart)
        );
        assert_eq!(lost.after(Button::Continue, Play::Alone), None);
        assert_eq!(
            lost.after(Button::MainMenu, Play::Alone),
            Some(Step::MainMenu)
        );

        // No way back from the statistics.
        assert_eq!(stats(won).after(Button::Statistics, Play::Alone), None);
        assert_eq!(
            stats(won).after(Button::Continue, Play::Alone),
            Some(Step::PlayOn)
        );
        assert_eq!(
            stats(lost).after(Button::Restart, Play::Alone),
            Some(Step::Restart)
        );
        assert_eq!(stats(lost).after(Button::Continue, Play::Alone), None);
    }

    /// A network game's panels neither go on nor restart: MAIN MENU,
    /// "Return to Lobby", goes back to the lobby; the statistics stay.
    #[test]
    fn a_network_game_goes_back_to_its_lobby() {
        let net = |end, page| Panel { end, page };

        for end in [End::Won, End::Lost] {
            let verdict = net(end, Page::Verdict);
            let stats = net(end, Page::Statistics);

            assert_eq!(
                verdict.after(Button::MainMenu, Play::Network),
                Some(Step::Lobby)
            );
            assert_eq!(
                verdict.after(Button::Statistics, Play::Network),
                Some(Step::To(stats))
            );
            assert_eq!(stats.buttons(Play::Network), [Button::MainMenu]);

            for gone in [Button::Continue, Button::Restart] {
                assert_eq!(verdict.after(gone, Play::Network), None);
                assert_eq!(stats.after(gone, Play::Network), None);
            }
        }

        assert_eq!(Panel::main_menu_text(Play::Alone), None);
        assert_eq!(Panel::main_menu_text(Play::Network), Some(336));
    }

    /// Everything won, one press asks the north's film alone, none for
    /// the south; a network game asks none.
    #[test]
    fn a_press_asks_the_norths_film_alone() {
        let progress = Progress::of(SLOTS[..=30].iter().map(|&q| {
            (
                q,
                crate::view::menu::quest_map::Record {
                    victories: 1,
                    best_ms: 1,
                },
            )
        }));
        let won = Panel::of(End::Won);

        assert_eq!(
            won.films(Button::MainMenu, &progress, Play::Alone),
            [(Film::North, Mark::Seen)]
        );
        assert_eq!(won.films(Button::MainMenu, &progress, Play::Network), []);
        assert_eq!(won.films(Button::Statistics, &progress, Play::Alone), []);
    }

    /// openregent's departure: a statistics page's exits ask the film as
    /// its verdict's would, once; a LAN game's none.
    #[test]
    fn the_statistics_exits_ask_the_film_as_the_verdicts() {
        let mut progress = Progress::of(SLOTS.iter().map(|&q| {
            (
                q,
                crate::view::menu::quest_map::Record {
                    victories: 1,
                    best_ms: 1,
                },
            )
        }));
        let stats = |end| Panel {
            page: Page::Statistics,
            ..Panel::of(end)
        };
        let north = |mark| vec![(Film::North, mark)];

        assert_eq!(
            stats(End::Won).films(Button::MainMenu, &progress, Play::Alone),
            north(Mark::Seen)
        );
        assert_eq!(
            stats(End::Won).films(Button::Continue, &progress, Play::Alone),
            north(Mark::Seen)
        );
        assert_eq!(
            stats(End::Lost).films(Button::Restart, &progress, Play::Alone),
            north(Mark::Unseen)
        );
        assert_eq!(
            stats(End::Won).films(Button::MainMenu, &progress, Play::Network),
            []
        );

        progress.seen.ending = true;
        assert_eq!(
            stats(End::Lost).films(Button::MainMenu, &progress, Play::Alone),
            [],
            "once"
        );
    }

    /// The ending is asked by MAIN MENU, RESTART and CONTINUE PLAYING on
    /// the verdict, won or lost, while every expansion quest is won and
    /// it is unseen; RESTART alone leaves it unseen.
    #[test]
    fn the_expansions_ending_comes_once_from_the_verdicts_presses() {
        use crate::view::menu::quest_map::{BASE_SLOTS, LAST_EXPANSION_SLOT as LAST, SLOTS};

        let mut progress = Progress::default();
        let won = Panel::of(End::Won);
        let lost = Panel::of(End::Lost);
        let stats = Panel {
            page: Page::Statistics,
            ..won
        };

        // The base's quests and eleven of the expansion's are not enough.
        for &quest in &SLOTS[..LAST] {
            progress.win(quest, 1);
        }

        assert_eq!(won.ending(Button::MainMenu, &progress), None);
        progress.win(SLOTS[LAST], 1);

        assert_eq!(won.ending(Button::MainMenu, &progress), Some(Mark::Seen));
        assert_eq!(won.ending(Button::Continue, &progress), Some(Mark::Seen));
        assert_eq!(lost.ending(Button::MainMenu, &progress), Some(Mark::Seen));
        assert_eq!(lost.ending(Button::Restart, &progress), Some(Mark::Unseen));
        assert_eq!(won.ending(Button::Statistics, &progress), None);
        assert_eq!(stats.ending(Button::MainMenu, &progress), None);
        assert_eq!(stats.ending(Button::Continue, &progress), None);

        progress.seen.ending = true;
        assert_eq!(won.ending(Button::MainMenu, &progress), None, "once");

        // The expansion's alone, no base quest won, is enough.
        let mut expansion = Progress::default();

        for &quest in &SLOTS[BASE_SLOTS..=LAST] {
            expansion.win(quest, 1);
        }

        assert_eq!(won.ending(Button::Continue, &expansion), Some(Mark::Seen));
    }

    #[test]
    fn the_statistics_fill_eighteen_controls() {
        let filled: Vec<usize> = (0..crate::sim::world::ledger::LINES)
            .filter(|&row| stat_control(row).is_some())
            .collect();

        assert_eq!(
            filled,
            [
                1, 2, 3, 4, 5, 6, 7, 8, 9, 12, 13, 14, 17, 18, 19, 20, 22, 24
            ]
        );
    }

    #[test]
    fn a_victory_waits_for_ok_then_fades_after_the_delay() {
        let delay = u64::from(FADE_DELAY_MS);
        let told = Curtain::new(End::Won, 0);

        assert_eq!(told.stage, Stage::Told);
        assert_eq!(told.tick(10_000, Advisor::Done), told, "the box waits");

        let fading = told.told(1_000, Some(delay));

        assert_eq!(fading.shade(), 0);
        assert_eq!(fading.tick(1_000 + delay, Advisor::Done).shade(), 0);

        // A step each 75 ms past the last, strictly.
        let step = u64::from(FADE_STEP_MS);
        let first = fading.tick(1_000 + delay + step + 1, Advisor::Done);

        assert_eq!(first.shade(), 1);
        assert_eq!(
            first
                .tick(1_000 + delay + 2 * step + 1, Advisor::Done)
                .shade(),
            1
        );
    }

    #[test]
    fn the_panel_waits_for_fifteen_steps_and_the_advisor() {
        let step = u64::from(FADE_STEP_MS) + 1;
        let run = |advisor| {
            (1..=u64::from(FADE_STEPS))
                .fold(Curtain::new(End::Lost, 0), |c, n| c.tick(n * step, advisor))
        };

        assert_eq!(run(Advisor::Speaking).panel(), None);
        assert_eq!(run(Advisor::Speaking).shade(), FADE_STEPS);
        assert_eq!(run(Advisor::Done).panel(), Some(Panel::of(End::Lost)));
        assert_eq!(
            run(Advisor::Speaking).tick(10_000, Advisor::Done).panel(),
            Some(Panel::of(End::Lost))
        );
    }

    #[test]
    fn with_no_point_a_victory_fades_at_once() {
        let fading = Curtain::new(End::Won, 0).told(5_000, None);

        assert_eq!(fading.tick(5_000, Advisor::Done).shade(), 1);
    }
}
