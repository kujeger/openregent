//! `BDEP` and `QDEP` — what a building or a quest needs before it is offered.
//!
//! Two plain-text `DATA` records of `miscdata.cam`; the header says
//! *"Buildings.txt --> USES POSTFIX NOTATION"*. One rule per line: a
//! four-character id, a colon, then a postfix expression over `&&`, `||` and
//! `NOT`:
//!
//! ```text
//! ABQ1 : ABJ2 ABJ3 ||          the Temple to Fervus needs a Palace at 2 or 3
//! ABC1                         a Blacksmith needs nothing, so no colon at all
//! ```
//!
//! ## Why a parser
//!
//! A stack machine checks balance, which a token split cannot: every operator
//! finds its operands and one value remains. Holds for every shipped rule.
//!
//! ## `QDEP` carries one more field
//!
//! ```text
//! QU06 : HARD QU01 QU02 && QU03 && …    a difficulty, then the expression
//! QU01 : EASY NONE                      and `NONE` where there is no rule
//! ```
//!
//! A quest rule is `id : difficulty expression`: one of four [`Difficulty`]
//! codes, then an expression or `NONE` for empty. A `BDEP` reader would take
//! `EASY` and `NONE` for ids.
//!
//! ## A rule gates the build menu, and nothing else
//!
//! **Game** knowledge, not the file. Three rules name themselves (infix via
//! `regent_cli deps`):
//!
//! ```text
//! ABf1 : ABJ1 ABb1 NOT && ABc1 NOT && ABf1 NOT &&    a Gnome Hovel needs no Gnome Hovel
//! ABo1 : ABJ3 ABm1 || ABo1 NOT &&                    likewise, in the expansion
//! ABq1 : ABq1 NOT ABq2 NOT ABq3 NOT && && …
//! ```
//!
//! A save with four Gnome Hovels is no contradiction: the player may build one
//! (two more appear once it stands, and it rebuilds after destruction), but
//! starting scenarios, quests and freestyle options place buildings freely,
//! opposing guilds included.
//!
//! So `NOT` is a plain complement over *is this present* ([`Expr::evaluate`]),
//! and a save cannot refute a rule. Eight kinds stand where their rule
//! forbids them (**Measured**).
//!
//! Buildings are uncapped by default (each costs more than the last), so a
//! rule is a condition, not a quantity. A self-naming rule is one of the few
//! caps.
//!
//! ## `NOT NOT` is not a no-op
//!
//! The file says *"the ABJ3 NOT NOT means exists, ignoring if it is
//! completed"*: a bare id asks finished, `NOT NOT` merely placed.
//! [`Expr::evaluate`] has one predicate and folds `!!x` to `x`. Right for every
//! shipped rule (none turns on a half-built prerequisite); wrong for an
//! engine. [`Expr::holds`] is the engine's: an id under `NOT` asks placed,
//! any other finished (**Binary**, bdep id value).
//!
//! ## The bug the file documents
//!
//! *"The LAST LINE MUST be left blank, otherwise statue prereqs won't be
//! parsed properly !"* — about the game's parser. This reader does not need it
//! (test `the_dependency_tables_do_not_need_their_blank_last_line`).

use super::error::{Error, Result};
use super::fourcc::FourCc;

/// A dependency expression, as the postfix source builds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    /// A building or quest id, true when that thing is there.
    Id(FourCc),
    Not(Box<Expr>),
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
}

/// The word a quest rule uses where it has no requirement.
///
/// Four characters, but not an id, so handled by name rather than by
/// [`Expr::parse`].
const NO_REQUIREMENT: &str = "NONE";

impl Expr {
    /// Read one postfix expression.
    ///
    /// # Errors
    ///
    /// If an operator finds too few operands, other than one value remains, or
    /// a word is neither an operator nor four characters.
    pub fn parse(source: &str) -> Result<Option<Self>> {
        let mut stack: Vec<Expr> = Vec::new();

        for word in source.split_whitespace() {
            let node = match word {
                "NOT" => Expr::Not(Box::new(pop(&mut stack)?)),

                // The right operand is on top, so it pops first.
                "&&" => {
                    let right = Box::new(pop(&mut stack)?);

                    Expr::And(Box::new(pop(&mut stack)?), right)
                }
                "||" => {
                    let right = Box::new(pop(&mut stack)?);

                    Expr::Or(Box::new(pop(&mut stack)?), right)
                }

                id => Expr::Id(FourCc::try_from(id.as_bytes())?),
            };

            stack.push(node);
        }

        match stack.len() {
            0 => Ok(None),
            1 => Ok(stack.pop()),
            left => Err(Error::BrokenInvariant {
                what: "postfix expression leaves more than one value on the stack",
                expected: 1,
                found: left as u64,
            }),
        }
    }

    /// Every id the expression names, in reading order, repeats included.
    #[must_use]
    pub fn ids(&self) -> Vec<FourCc> {
        let mut out = Vec::new();
        self.collect(&mut out);
        out
    }

    /// Whether the requirement is met, given what is standing.
    ///
    /// `present` answers *is this building standing*. The engine also tells
    /// finished from placed (`NOT NOT`); one predicate cannot.
    ///
    /// `true` means the rule is met. Rules gate the build menu only; starting
    /// scenarios bypass them.
    pub fn evaluate(&self, present: &impl Fn(FourCc) -> bool) -> bool {
        match self {
            Expr::Id(id) => present(*id),
            Expr::Not(inner) => !inner.evaluate(present),
            Expr::And(left, right) => left.evaluate(present) && right.evaluate(present),
            Expr::Or(left, right) => left.evaluate(present) || right.evaluate(present),
        }
    }

    /// Whether the requirement is met as the engine evaluates it
    /// (postfix eval, its id callback bdep id value): an id directly
    /// under `NOT` asks [`Stand::Placed`] or better, any other id
    /// [`Stand::Finished`]. So `x NOT` is "none placed" and `x NOT NOT`
    /// "one placed". **Binary**.
    pub fn holds(&self, stand: &impl Fn(FourCc) -> Stand) -> bool {
        match self {
            Expr::Id(id) => stand(*id) == Stand::Finished,
            Expr::Not(inner) => match inner.as_ref() {
                Expr::Id(id) => stand(*id) == Stand::Absent,
                other => !other.holds(stand),
            },
            Expr::And(left, right) => left.holds(stand) && right.holds(stand),
            Expr::Or(left, right) => left.holds(stand) || right.holds(stand),
        }
    }

    fn collect(&self, out: &mut Vec<FourCc>) {
        match self {
            Expr::Id(id) => out.push(*id),
            Expr::Not(inner) => inner.collect(out),
            Expr::And(left, right) | Expr::Or(left, right) => {
                left.collect(out);
                right.collect(out);
            }
        }
    }
}

/// How far one building code stands for a player, as bdep met maps
/// it: placed while any is under construction (`CurrentStageBuilt` 0),
/// finished once one has it set. Ruins do not count. **Binary**.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Stand {
    Absent,
    Placed,
    Finished,
}

/// Take one operand, or report that an operator has none.
fn pop(stack: &mut Vec<Expr>) -> Result<Expr> {
    stack.pop().ok_or(Error::BrokenInvariant {
        what: "postfix operator with no operand left",
        expected: 1,
        found: 0,
    })
}

/// How hard a quest is, as `QDEP` spells it.
///
/// Declaration order is tier order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Difficulty {
    Easy,
    Medium,
    Hard,
    /// The expansion's own tier, on the two quests that need every other one.
    Master,
}

impl Difficulty {
    /// Every code, and the word `QDEP` writes for it.
    pub const ALL: [(Difficulty, &'static str); 4] = [
        (Difficulty::Easy, "EASY"),
        (Difficulty::Medium, "MEDM"),
        (Difficulty::Hard, "HARD"),
        (Difficulty::Master, "MSTR"),
    ];

    fn from_word(word: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .find(|(_, spelling)| *spelling == word)
            .map(|(which, _)| *which)
    }
}

/// One line of `BDEP`: a building, and what it wants standing first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Building {
    pub id: FourCc,
    /// `None` without a colon, as on most lines.
    pub requires: Option<Expr>,
}

/// One line of `QDEP`: a quest, its difficulty, and what it wants finished.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Quest {
    pub id: FourCc,
    pub difficulty: Difficulty,
    /// `None` for `NONE`, as on most lines.
    pub requires: Option<Expr>,
}

/// Read the building dependency table.
///
/// # Errors
///
/// If a line's id is not four characters, or its expression does not balance.
pub fn buildings(text: &str) -> Result<Vec<Building>> {
    rules(text)
        .map(|(id, rest)| {
            Ok(Building {
                id: FourCc::try_from(id.as_bytes())?,
                requires: Expr::parse(rest)?,
            })
        })
        .collect()
}

/// Read the quest dependency table.
///
/// # Errors
///
/// As [`buildings`], or if a rule names no difficulty.
pub fn quests(text: &str) -> Result<Vec<Quest>> {
    rules(text)
        .map(|(id, rest)| {
            let (word, expression) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));

            let difficulty = Difficulty::from_word(word).ok_or(Error::BadMagic {
                what: "quest difficulty",
                found: word.as_bytes().to_vec(),
            })?;

            Ok(Quest {
                id: FourCc::try_from(id.as_bytes())?,
                difficulty,
                requires: match expression.trim() {
                    NO_REQUIREMENT => None,
                    other => Expr::parse(other)?,
                },
            })
        })
        .collect()
}

/// Every rule line as `(id, everything after the colon)`.
///
/// A line with no colon (most of `BDEP`) has no dependencies: empty tail, not
/// skipped.
fn rules(text: &str) -> impl Iterator<Item = (&str, &str)> {
    text.lines()
        .map(|line| line.split('#').next().unwrap_or("").trim())
        .filter(|line| !line.is_empty())
        .map(|line| match line.split_once(':') {
            Some((id, rest)) => (id.trim(), rest.trim()),
            None => (line, ""),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(tag: [u8; 4]) -> Expr {
        Expr::Id(FourCc(tag))
    }

    #[test]
    fn a_building_with_no_colon_requires_nothing() {
        let read = buildings("# Blacksmith 1\r\nABC1\r\n").unwrap();

        assert_eq!(
            read,
            vec![Building {
                id: FourCc::new(b"ABC1"),
                requires: None
            }]
        );
    }

    #[test]
    fn the_temple_to_fervus_wants_either_palace() {
        let read = buildings("ABQ1 : ABJ2 ABJ3 ||").unwrap();

        assert_eq!(
            read[0].requires,
            Some(Expr::Or(Box::new(id(*b"ABJ2")), Box::new(id(*b"ABJ3"))))
        );
    }

    /// The operands come off the stack reversed, so `a b &&` is `a && b`.
    ///
    /// Invisible on `&&` and `||`, which commute.
    #[test]
    fn operands_keep_the_order_the_source_wrote_them_in() {
        let Some(Expr::And(left, right)) = buildings("AB01 : AB02 AB03 &&").unwrap()[0]
            .requires
            .clone()
        else {
            panic!("not an And")
        };

        assert_eq!(*left, id(*b"AB02"));
        assert_eq!(*right, id(*b"AB03"));
    }

    /// The check a word split cannot make.
    #[test]
    fn an_unbalanced_expression_is_an_error_and_not_a_reading() {
        for broken in ["ABQ1 : ABJ2 ABJ3", "ABQ1 : &&", "ABQ1 : ABJ2 && ABJ3"] {
            assert!(
                matches!(
                    buildings(broken),
                    Err(Error::BrokenInvariant { .. } | Error::BadValue { .. })
                ),
                "{broken} parsed"
            );
        }
    }

    #[test]
    fn a_three_character_word_is_not_an_id() {
        assert!(matches!(
            buildings("ABQ1 : ABJ"),
            Err(Error::BadValue { .. })
        ));
    }

    #[test]
    fn a_quest_carries_a_difficulty_before_its_expression() {
        let read = quests("QU01 : EASY NONE\r\nQU09 : HARD QU01 QU13 && QU17 &&\r\n").unwrap();

        assert_eq!(read[0].difficulty, Difficulty::Easy);
        assert_eq!(read[0].requires, None);
        assert_eq!(read[1].difficulty, Difficulty::Hard);
        assert_eq!(read[1].requires.as_ref().unwrap().ids().len(), 3);
    }

    /// `EASY` is four characters. Read as a building rule it is an id.
    #[test]
    fn a_quest_rule_read_as_a_building_rule_swallows_the_difficulty() {
        let as_building = buildings("QU01 : EASY NONE");

        // Fails only because two values remain unjoined; hence `quests` is
        // separate.
        assert!(matches!(as_building, Err(Error::BrokenInvariant { .. })));
    }

    #[test]
    fn an_unknown_difficulty_is_refused() {
        assert!(matches!(
            quests("QU01 : TRIV NONE"),
            Err(Error::BadMagic { .. })
        ));
    }

    #[test]
    fn evaluating_a_rule_answers_what_the_game_would_ask() {
        let rule = buildings("ABb1 : ABJ2 ABJ3 || ABC3 && ABc1 NOT &&").unwrap();
        let requires = rule[0].requires.as_ref().unwrap();

        let standing = |want: &[&[u8; 4]]| {
            let owned: Vec<FourCc> = want.iter().map(|t| FourCc::new(t)).collect();

            requires.evaluate(&move |id| owned.contains(&id))
        };

        // Palace 2 and a level 3 blacksmith, no elves: the settlement is on.
        assert!(standing(&[b"ABJ2", b"ABC3"]));

        // The same, with an Elven Bungalow already up.
        assert!(!standing(&[b"ABJ2", b"ABC3", b"ABc1"]));

        // And with no palace at all.
        assert!(!standing(&[b"ABC3"]));
    }

    #[test]
    fn a_comment_can_follow_a_rule_as_well_as_precede_it() {
        let read = buildings("ABY1 : ABJ2 ABJ3 NOT NOT ||  # lvl 2 palace and up").unwrap();

        assert_eq!(read.len(), 1);
        assert_eq!(
            read[0].requires.as_ref().unwrap().ids(),
            [FourCc::new(b"ABJ2"), FourCc::new(b"ABJ3")]
        );
    }

    /// The wizards' guild wants a finished level 2 palace, or a level 3
    /// one merely placed: a palace upgrading to 3 still counts.
    #[test]
    fn an_id_under_not_asks_placed_and_a_bare_id_finished() {
        let rule = buildings("ABY1 : ABJ2 ABJ3 NOT NOT ||").unwrap();
        let requires = rule[0].requires.as_ref().unwrap();
        let with = |code: &[u8; 4], stand: Stand| {
            let code = FourCc::new(code);

            requires.holds(&move |id| if id == code { stand } else { Stand::Absent })
        };

        assert!(with(b"ABJ2", Stand::Finished));
        assert!(!with(b"ABJ2", Stand::Placed));
        assert!(with(b"ABJ3", Stand::Placed));
        assert!(!with(b"ABJ1", Stand::Finished));

        // `x NOT`: none placed, finished or not.
        let none = buildings("ABf1 : ABf1 NOT").unwrap();
        let none = none[0].requires.as_ref().unwrap();

        assert!(!none.holds(&|_| Stand::Placed));
        assert!(none.holds(&|_| Stand::Absent));
    }
}
