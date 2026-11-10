//! The top bar's scroll, `APMK` control 2008: the game's title, and the
//! goals a press shows (`AP88`, over the paused game). **Binary**.
//!
//! ```text
//!  title  scroll title      (MainFrameController)
//!    a quest         the quest title: STRT QUES by its code
//!    a custom game   the random title, the Enemy Forces word:
//!                      BEGI, ADVA, EXPE   GMTX 111-113, "Random (Beginner)"
//!                      else               its name, as the dialog's
//!  goals  Option3MenuController,      control 5001
//!    a quest         its <Short>
//!    a custom game   Maj.qdd's FRG<victory>: the Long Description with the
//!                    goal for gold and days, else the Short; then, but
//!                    for a random settlement, GMTX 321 "Map Number: %ld."

//! ```
//!
//! A network game's title is [`WAITING`] while players wait.

use crate::data::fourcc::FourCc;
use crate::data::qdd;
use crate::data::text::{GMTX, Text, printf};
use crate::sim::freestyle::Setup;
use crate::sim::freestyle::Victory;

/// The dialog's random picks and their titles: scroll random title
/// compares the Enemy Forces word with each, then pushes its `GMTX`
/// index; any other code is named by its record.
pub const RANDOM: [(FourCc, usize); 3] = [
    (FourCc(*b"BEGI"), 111),
    (FourCc(*b"ADVA"), 112),
    (FourCc(*b"EXPE"), 113),
];

/// A custom game's goals by victory condition: `Maj.qdd`'s blocks,
/// `FRG0` to `FRG3` by [`Setup::victory`], `NONE` past them.
pub const GOALS: [&str; 4] = ["FRG0", "FRG1", "FRG2", "FRG3"];

/// The block a condition past [`GOALS`] asks for.
pub const NO_GOALS: &str = "NONE";

/// The conditions whose goals are the Long Description, its `%d` the
/// goal ([`Setup::goal`]): gold and days.
pub const WITH_GOAL: [Victory; 2] = [Victory::Gold, Victory::Survive];

/// Past a settlement not random, `GMTX` 321 with the map number is
/// appended: "\nMap Number: %ld.".
pub const MAP_NUMBER: usize = 321;

/// `GMTX` 345 "(Waiting for Other Players)": the title of a network
/// game whose frame (`GSMessageManager`) is
/// at most a field plus a field plus 1, scroll title's first
/// test. **Inferred**: the frame its bundles do not yet
/// cover, so while a player is waited on.
pub const WAITING: usize = 345;

/// The `.qdd` keys of a block's two texts.
const SHORT: &str = "Short Description";
const LONG: &str = "Long Description";

/// What the scroll names.
#[derive(Clone, Copy, Debug)]
pub enum ScrollSubject<'a> {
    /// A quest: its title (`STRT QUES`) and `<Short>`.
    Quest { title: &'a str, short: &'a str },
    /// A custom game: its setup.
    Custom(&'a Setup),
}

/// The scroll's text.
#[must_use]
pub fn title(game: ScrollSubject<'_>, text: &Text<'_>) -> Option<String> {
    match game {
        ScrollSubject::Quest { title, .. } => Some(title.to_owned()),
        ScrollSubject::Custom(setup) => {
            let code = setup.scenario();
            let random = RANDOM.iter().find(|(c, _)| *c == code);

            match random {
                Some((_, index)) => text.at(GMTX, *index),
                None => text.scenario(code),
            }
            .map(|t| t.trim().to_owned())
        }
    }
}

/// The goals `AP88` shows, `quests` the parsed `Maj.qdd`.
#[must_use]
pub fn goals(game: ScrollSubject<'_>, quests: &[qdd::Quest], text: &Text<'_>) -> Option<String> {
    match game {
        ScrollSubject::Quest { short, .. } => Some(short.to_owned()),
        ScrollSubject::Custom(setup) => {
            let victory = usize::try_from(setup.victory()).ok();
            let block = victory.and_then(|v| GOALS.get(v)).unwrap_or(&NO_GOALS);
            let quest = quests.iter().find(|q| q.id == *block)?;
            let with_goal = Victory::ALL
                .iter()
                .find(|v| v.word() == setup.victory())
                .is_some_and(|v| WITH_GOAL.contains(v));
            let said = if with_goal {
                printf(quest.field(LONG)?.trim(), &[setup.goal().into()])
            } else {
                quest.field(SHORT)?.trim().to_owned()
            };
            let random = RANDOM.iter().any(|(c, _)| *c == setup.settlement());
            let number = text
                .at(GMTX, MAP_NUMBER)
                .filter(|_| !random)
                .map(|f| printf(&f, &[setup.seed().into()]))
                .unwrap_or_default();

            Some(said + &number)
        }
    }
}
