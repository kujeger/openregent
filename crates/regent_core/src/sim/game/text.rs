//! [`Order`] as one line of text, to record a session and play it back:
//! `p` and its player, then its [`Input`]'s words.
//!
//! ```text
//!  p0 build Gld1 3200 4500 0       the crown's building
//!
//!  INPUT:
//!  unpreview CODE                  build CODE X Y OWNER
//!  recruit GUILD CODE              flag CODE X Y OWNER REWARD TARGET
//!  cast CODE X Y OWNER             bless SPELL TARGET OWNER
//!  upgrade AGENT                   research BUILDING ITEM FRAME
//!  alter FRAME AGENT KEY CHANGE LO HI      (KEY as APR:1)
//!  enchant AGENT                   cheat CODE ARG   (the message's words)
//!  resurrect MAUSOLEUM HERO PRICE   unflag HANDLE
//!  demolish AGENT                  toggle AGENT BUTTON   (as tax-once)
//!  dismiss AGENT                   assemble AGENT
//!  market AGENT                    extort AGENT
//!  rename AGENT NAME   (NAME escaped as a code)    rage AGENT
//!  bounty HALL CODE TERM   (TERM short or long)    clear HANDLE
//!  campbegin GUILD                 campleave
//! ```
//!
//! A code byte that is no printable ASCII, a space or `%` is `%` and two
//! hex digits: `"Ice "` is `Ice%20`.

use std::fmt;
use std::str::FromStr;

use super::{Input, Order};
use crate::data::attrib::Key;
use crate::data::fourcc::FourCc;
use crate::sim::world::cheat::{Cheat, CheatCode};
use crate::sim::world::{
    alter::{Alter, Set},
    build::BuildOrder,
    flag::Flag,
    guild::{
        bounty::{Bounty, Term},
        camp::{Camp, Flow, Resource},
        mausoleum::Resurrect,
        upkeep::{Toggle, Upkeep},
    },
    naming::Rename,
    recruit::Recruit,
    research::Research,
    spell::cast::{Bless, Cast},
};

/// Escapes a code byte.
const ESCAPE: u8 = b'%';

/// Opens an order's player: `p0`.
const PLAYER: char = 'p';

/// Between an attribute key's bank and index.
const KEY_SEPARATOR: char = ':';

/// `bytes` with each byte not graphic ASCII, or [`ESCAPE`], escaped.
fn escaped(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|&b| {
            if b.is_ascii_graphic() && b != ESCAPE {
                char::from(b).to_string()
            } else {
                format!("%{b:02x}")
            }
        })
        .collect()
}

/// The bytes [`escaped`] wrote.
fn unescaped(text: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut bytes = text.bytes();

    while let Some(b) = bytes.next() {
        if b == ESCAPE {
            let hex: Vec<u8> = bytes.by_ref().take(2).collect();
            let byte = std::str::from_utf8(&hex)
                .ok()
                .and_then(|h| u8::from_str_radix(h, 16).ok())
                .ok_or_else(|| format!("{text}: a bad escape"))?;

            out.push(byte);
        } else {
            out.push(b);
        }
    }

    Ok(out)
}

fn code_text(code: FourCc) -> String {
    escaped(code.as_bytes())
}

/// A bounty's term as a word.
const fn term_text(term: Term) -> &'static str {
    match term {
        Term::Short => "short",
        Term::Long => "long",
    }
}

/// A treasury move's direction as a word.
const fn flow_text(flow: Flow) -> &'static str {
    match flow {
        Flow::Spend => "spend",
        Flow::Add => "add",
    }
}

fn key_text(Key { bank, index }: Key) -> String {
    format!("{}{KEY_SEPARATOR}{index}", escaped(&bank))
}

impl fmt::Display for Input {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Input::Unpreview(code) => write!(f, "unpreview {}", code_text(code)),
            Input::Build(o) => write!(
                f,
                "build {} {} {} {}",
                code_text(o.code),
                o.at.0,
                o.at.1,
                o.owner
            ),
            Input::Recruit(r) => write!(f, "recruit {} {}", r.guild, code_text(r.code)),
            Input::Flag(g) => write!(
                f,
                "flag {} {} {} {} {} {}",
                code_text(g.code),
                g.at.0,
                g.at.1,
                g.owner,
                g.reward,
                g.target
            ),
            Input::Cast(c) => write!(
                f,
                "cast {} {} {} {}",
                code_text(c.code),
                c.at.0,
                c.at.1,
                c.owner
            ),
            Input::Bless(b) => write!(
                f,
                "bless {} {} {} {} {}",
                code_text(b.spell),
                b.target,
                b.owner,
                b.at.0,
                b.at.1
            ),
            Input::Upgrade(id) => write!(f, "upgrade {id}"),
            Input::Research(r) => write!(f, "research {} {} {}", r.building, r.item, r.frame),
            Input::Alter(a) => write!(
                f,
                "alter {} {} {} {} {} {}",
                a.frame,
                a.agent,
                key_text(a.key),
                a.change,
                a.range.0,
                a.range.1
            ),
            Input::Enchant(id) => write!(f, "enchant {id}"),
            Input::Resource(r) => {
                write!(f, "resource {} {} {}", r.owner, r.amount, flow_text(r.flow))
            }
            Input::CampBegin(guild) => write!(f, "campbegin {guild}"),
            Input::CampLeave => write!(f, "campleave"),
            Input::Camp(c) => write!(f, "camp {} {} {}", c.guild, c.at.0, c.at.1),
            Input::Embassy(id) => write!(f, "embassy {id}"),
            Input::Resurrect(r) => write!(f, "resurrect {} {} {}", r.mausoleum, r.hero, r.price),
            Input::Cheat(c) => write!(f, "cheat {} {}", c.cheat.code(), c.arg),
            Input::Read(handle) => write!(f, "read {handle}"),
            Input::Unflag(handle) => write!(f, "unflag {handle}"),
            Input::Demolish(id) => write!(f, "demolish {id}"),
            Input::Dismiss(id) => write!(f, "dismiss {id}"),
            Input::Clear(handle) => write!(f, "clear {handle}"),
            Input::Assemble(id) => write!(f, "assemble {id}"),
            Input::MarketDay(id) => write!(f, "market {id}"),
            Input::Extort(id) => write!(f, "extort {id}"),
            Input::Toggle(t) => write!(f, "toggle {} {}", t.building, t.upkeep.word()),
            Input::Rename(r) => write!(f, "rename {} {}", r.unit, escaped(r.name().as_bytes())),
            Input::Rage(id) => write!(f, "rage {id}"),
            Input::Bounty(b) => write!(
                f,
                "bounty {} {} {}",
                b.hall,
                code_text(b.code),
                term_text(b.term)
            ),
            Input::Gamble(id) => write!(f, "gamble {id}"),
            Input::NewBet(id) => write!(f, "newbet {id}"),
            Input::Set(s) => write!(f, "set {} {} {}", s.agent, key_text(s.key), s.value),
        }
    }
}

impl fmt::Display for Order {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{PLAYER}{} {}", self.player, self.input)
    }
}

impl FromStr for Order {
    type Err = String;

    /// The line [`Order`]'s `Display` wrote.
    fn from_str(line: &str) -> Result<Order, String> {
        let (word, input) = line.trim_start().split_once(' ').unwrap_or((line, ""));
        let player = word
            .strip_prefix(PLAYER)
            .and_then(|n| n.parse().ok())
            .ok_or_else(|| format!("{line}: {word} is no player"))?;

        Ok(Order {
            player,
            input: input.parse()?,
        })
    }
}

/// A line's words, read in turn.
struct Words<'a> {
    line: &'a str,
    words: std::str::SplitWhitespace<'a>,
}

impl<'a> Words<'a> {
    fn word(&mut self) -> Result<&'a str, String> {
        self.words
            .next()
            .ok_or_else(|| format!("{}: too few words", self.line))
    }

    fn number<T: FromStr>(&mut self) -> Result<T, String> {
        let word = self.word()?;

        word.parse()
            .map_err(|_| format!("{}: {word} is not a number", self.line))
    }

    fn code(&mut self) -> Result<FourCc, String> {
        let bytes = unescaped(self.word()?)?;

        FourCc::try_from(bytes.as_slice()).map_err(|e| format!("{}: {e}", self.line))
    }

    fn flow(&mut self) -> Result<Flow, String> {
        match self.word()? {
            "spend" => Ok(Flow::Spend),
            "add" => Ok(Flow::Add),
            other => Err(format!("{}: {other} is no flow", self.line)),
        }
    }

    fn cheat(&mut self) -> Result<Cheat, String> {
        let code = self.number()?;

        Cheat::of(code).ok_or_else(|| format!("{}: {code} is no cheat", self.line))
    }

    fn upkeep(&mut self) -> Result<Upkeep, String> {
        let word = self.word()?;

        Upkeep::of_word(word).ok_or_else(|| format!("{}: {word} is no button", self.line))
    }

    fn term(&mut self) -> Result<Term, String> {
        match self.word()? {
            "short" => Ok(Term::Short),
            "long" => Ok(Term::Long),
            other => Err(format!("{}: {other} is no term", self.line)),
        }
    }

    fn name(&mut self) -> Result<String, String> {
        let bytes = unescaped(self.word()?)?;

        String::from_utf8(bytes).map_err(|e| format!("{}: {e}", self.line))
    }

    fn point(&mut self) -> Result<(i32, i32), String> {
        Ok((self.number()?, self.number()?))
    }

    fn key(&mut self) -> Result<Key, String> {
        let word = self.word()?;
        let line = self.line;
        let bad = || format!("{line}: {word} is not a key");
        let (bank, index) = word.split_once(KEY_SEPARATOR).ok_or_else(bad)?;
        let bank = <[u8; 3]>::try_from(unescaped(bank)?.as_slice()).map_err(|_| bad())?;

        Ok(Key::new(bank, index.parse().map_err(|_| bad())?))
    }
}

impl FromStr for Input {
    type Err = String;

    /// The line [`Input`]'s `Display` wrote.
    #[allow(clippy::too_many_lines, reason = "one arm an input")]
    fn from_str(line: &str) -> Result<Input, String> {
        let mut w = Words {
            line,
            words: line.split_whitespace(),
        };

        let input = match w.word()? {
            "unpreview" => Input::Unpreview(w.code()?),
            "build" => Input::Build(BuildOrder {
                code: w.code()?,
                at: w.point()?,
                owner: w.number()?,
            }),
            "recruit" => Input::Recruit(Recruit {
                guild: w.number()?,
                code: w.code()?,
            }),
            "flag" => Input::Flag(Flag {
                code: w.code()?,
                at: w.point()?,
                owner: w.number()?,
                reward: w.number()?,
                target: w.number()?,
            }),
            "cast" => Input::Cast(Cast {
                code: w.code()?,
                at: w.point()?,
                owner: w.number()?,
            }),
            "bless" => Input::Bless(Bless {
                spell: w.code()?,
                target: w.number()?,
                owner: w.number()?,
                at: w.point()?,
            }),
            "upgrade" => Input::Upgrade(w.number()?),
            "research" => Input::Research(Research {
                building: w.number()?,
                item: w.number()?,
                frame: w.number()?,
            }),
            "alter" => Input::Alter(Alter {
                frame: w.number()?,
                agent: w.number()?,
                key: w.key()?,
                change: w.number()?,
                range: w.point()?,
            }),
            "enchant" => Input::Enchant(w.number()?),
            "resource" => Input::Resource(Resource {
                owner: w.number()?,
                amount: w.number()?,
                flow: w.flow()?,
            }),
            "campbegin" => Input::CampBegin(w.number()?),
            "campleave" => Input::CampLeave,
            "camp" => Input::Camp(Camp {
                guild: w.number()?,
                at: w.point()?,
            }),
            "embassy" => Input::Embassy(w.number()?),
            "resurrect" => Input::Resurrect(Resurrect {
                mausoleum: w.number()?,
                hero: w.number()?,
                price: w.number()?,
            }),
            "cheat" => Input::Cheat(CheatCode {
                cheat: w.cheat()?,
                arg: w.number()?,
            }),
            "read" => Input::Read(w.number()?),
            "unflag" => Input::Unflag(w.number()?),
            "demolish" => Input::Demolish(w.number()?),
            "dismiss" => Input::Dismiss(w.number()?),
            "clear" => Input::Clear(w.number()?),
            "assemble" => Input::Assemble(w.number()?),
            "market" => Input::MarketDay(w.number()?),
            "extort" => Input::Extort(w.number()?),
            "toggle" => Input::Toggle(Toggle {
                building: w.number()?,
                upkeep: w.upkeep()?,
            }),
            "rename" => {
                let unit = w.number()?;

                Input::Rename(Rename::new(unit, &w.name()?))
            }
            "rage" => Input::Rage(w.number()?),
            "bounty" => Input::Bounty(Bounty {
                hall: w.number()?,
                code: w.code()?,
                term: w.term()?,
            }),
            "gamble" => Input::Gamble(w.number()?),
            "newbet" => Input::NewBet(w.number()?),
            "set" => Input::Set(Set {
                agent: w.number()?,
                key: w.key()?,
                value: w.number()?,
            }),
            other => Err(format!("{line}: {other} is no input"))?,
        };

        match w.words.next() {
            None => Ok(input),
            Some(extra) => Err(format!("{line}: {extra} is one word too many")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every variant, a spaced code and a negative number among them.
    fn every() -> Vec<Input> {
        let code = FourCc::new(b"Ice ");

        vec![
            Input::Unpreview(FourCc::new(b"Gld1")),
            Input::Build(BuildOrder {
                code: FourCc::new(b"Gld1"),
                at: (3200, -16),
                owner: 0,
            }),
            Input::Recruit(Recruit {
                guild: 17,
                code: FourCc::new(b"Warr"),
            }),
            Input::Flag(Flag {
                code,
                at: (1, 2),
                owner: 0,
                reward: 150,
                target: 9,
            }),
            Input::Cast(Cast {
                code: FourCc::new(b"%ab%"),
                at: (5, 6),
                owner: 0,
            }),
            Input::Bless(Bless {
                spell: code,
                target: 4,
                owner: 0,
                at: (7, 8),
            }),
            Input::Upgrade(12),
            Input::Research(Research {
                building: 3,
                item: 1004,
                frame: 77,
            }),
            Input::Alter(Alter {
                frame: 5,
                agent: 8,
                key: Key::new(*b"APR", 1),
                change: -100,
                range: (0, 0),
            }),
            Input::Enchant(21),
            Input::Resource(Resource {
                owner: 0,
                amount: 800,
                flow: Flow::Spend,
            }),
            Input::CampBegin(9),
            Input::CampLeave,
            Input::Camp(Camp {
                guild: 9,
                at: (320, 640),
            }),
            Input::Embassy(30),
            Input::Resurrect(Resurrect {
                mausoleum: 40,
                hero: 41,
                price: 2000,
            }),
            Input::Cheat(CheatCode {
                cheat: Cheat::Gold,
                arg: 0,
            }),
            Input::Read(901),
            Input::Unflag(902),
            Input::Toggle(Toggle {
                building: 53,
                upkeep: Upkeep::RepairRoute,
            }),
            Input::Dismiss(10),
            Input::Clear(903),
            Input::Assemble(2),
            Input::MarketDay(7),
            Input::Extort(8),
            Input::Rename(Rename::new(10, "Sir Swordsword")),
            Input::Rage(25),
            Input::Bounty(Bounty {
                hall: 7,
                code: FourCc::new(b"BVs1"),
                term: Term::Long,
            }),
            Input::Gamble(10),
            Input::NewBet(10),
            Input::Set(Set {
                agent: 10,
                key: Key::new(*b"APB", 47),
                value: 225,
            }),
        ]
    }

    #[test]
    fn every_input_reads_back_from_its_text() {
        for input in every() {
            let text = input.to_string();

            assert_eq!(text.parse::<Input>(), Ok(input), "{text}");
        }
    }

    #[test]
    fn an_order_reads_back_from_its_text() {
        let order = Order {
            player: 3,
            input: Input::Upgrade(12),
        };
        let text = order.to_string();

        assert_eq!(text, "p3 upgrade 12");
        assert_eq!(text.parse::<Order>(), Ok(order));
    }

    #[test]
    fn an_order_without_its_player_is_an_error() {
        assert!("upgrade 12".parse::<Order>().is_err());
        assert!("px upgrade 12".parse::<Order>().is_err());
        assert!("p1".parse::<Order>().is_err());
    }

    #[test]
    fn a_spaced_code_is_escaped() {
        let text = Input::Unpreview(FourCc::new(b"Ice ")).to_string();

        assert_eq!(text, "unpreview Ice%20");
    }

    #[test]
    fn a_short_or_long_line_is_an_error() {
        assert!("build Gld1 1 2".parse::<Input>().is_err());
        assert!("upgrade 1 2".parse::<Input>().is_err());
        assert!("dance 1".parse::<Input>().is_err());
        assert!("cheat 7 0".parse::<Input>().is_err());
    }
}
