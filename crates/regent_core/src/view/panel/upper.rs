//! The selection panel's control ids and choice rules: which panel a
//! piece shows, which controls it shows, what its research button
//! opens, the warriors' guild's roster counts, the rogues' extortion.
//! openregent's `panel::upper` draws them.

use crate::data::fourcc::FourCc;
use crate::data::text::{Text, printf};
use crate::sim::game::Input;
use crate::sim::world::{
    Piece, chat,
    guild::extortion::{EXTORT_TEXT, Extortion},
    hire::Members,
    offer::{ATTACK_FLAG, EXPLORE_FLAG, WarRoster},
    research,
    sound::{Phase, Voice},
};
use crate::view::menu::Able;
use crate::view::panel::tip;
use crate::view::sound;

/// Every panel's common controls; the name, zoom and track are
/// [`crate::view::menu::ids`]' `TITLE`, `ZOOM` and `TRACK`.
pub mod id {
    /// The portrait, `SMNU` key 6 (`data::smnu::CONTROL`): `0x1f4b` in
    /// panel portrait.
    pub const PORTRAIT: i32 = 8011;
    pub const HP_BAR: i32 = 8006;
    pub const HP_TEXT: i32 = 8010;
    pub const LINE: i32 = 8014;
    pub const DOING: i32 = 8030;
    pub const DESTROY: i32 = 8002;
    pub const TIPS: [i32; 2] = [8028, 8029];
    pub const CLASS: i32 = 7002;
    pub const HERO_LEVEL: i32 = 7003;
    pub const XP_BAR: i32 = 4016;
    pub const WEAPON: i32 = 5001;
    pub const WEAPON_BONUS: i32 = 5002;
    pub const ARMOUR_BONUS: i32 = 5003;
    pub const ARMOUR: i32 = 5004;
    pub const TAB_SPELLS: i32 = 4004;
    pub const TAB_ITEMS: i32 = 4002;
    pub const TAB_STATS: i32 = 4006;
    pub const LEVEL: i32 = 7720;
    pub const UPGRADE: i32 = 8007;
    pub const UPGRADE_PRICE: i32 = 8015;
    pub const TAX_ONCE: i32 = 8024;
    pub const TAX_ROUTE: i32 = 8025;
    pub const REPAIR_ONCE: i32 = 8026;
    pub const REPAIR_ROUTE: i32 = 8027;
    pub const COUNTS: [i32; 2] = [8019, WARRIORS];
    /// A warriors' guild's counts (`AP52`): its warriors; a temple
    /// hero's frame, then icon and count, one or both.
    pub const WARRIORS: i32 = 7949;
    pub const ONE_FRAME: i32 = 7950;
    pub const ONE_DISCORD: (i32, i32) = (7954, 7955);
    pub const ONE_PALADINS: (i32, i32) = (7953, 7956);
    pub const BOTH_FRAME: i32 = 7960;
    pub const BOTH_DISCORD: (i32, i32) = (7962, 7964);
    pub const BOTH_PALADINS: (i32, i32) = (7961, 7963);
    /// The outpost's hero icon and count, in [`ONE_FRAME`].
    /// The icon opens its member list.
    pub const HOUSED: (i32, i32) = (7954, 7956);
    /// The outpost's HEROES button: its member list.
    pub const OUTPOST_HEROES: i32 = 5004;
    pub const HEROES: i32 = 8004;
    pub const VISITORS: i32 = 8021;
    pub const RESEARCH: i32 = 8851;
    pub const SECOND: i32 = 8009;
    pub const CARRIED: i32 = 5000;
    pub const PICKUP: i32 = 5001;
    pub const RETURN: i32 = 5002;
    pub const RETURN_DOWN: i32 = 12;
    pub const RETURN_UP: i32 = 11;
    pub const PICKUP_DOWN: i32 = 13;
    pub const PICKUP_UP: i32 = 14;
    pub const REWARD: i32 = 5004;
    pub const RAISE: i32 = 5003;
    pub const REMOVE: i32 = 5000;
    pub const TARGET: i32 = 8888;
    pub const SEEKERS: i32 = 8019;
    pub const VIEW_HEROES: i32 = 8995;
    pub const PALACE_STATS: i32 = 7;
    pub const PALACE_REWARDS: i32 = 5001;
    pub const PALACE_BUILD: i32 = 5002;
    pub const PALACE_ROSTER: i32 = 5003;
    /// The palace's guard, tax collector and peasant icons: each opens
    /// the henchmen roster (`PalaceController`, 9 to 11).
    pub const PALACE_ICONS: [i32; 3] = [9, 10, 11];
}

/// Building panel controls the band or a later version stands for:
/// recruit and research buttons and bars, their prices, the guilds'
/// second roster blocks, market day and call to arms.
pub const BAND_IDS: [i32; 29] = [
    5000, 5001, 5970, 8008, 8910, 8017, 8022, 8023, 4481, 4484, 4485, 8200, 8201, 8826, 4500, 4501,
    8202, 8723, 5772, 7950, 7953, 7954, 7955, 7956, 7960, 7961, 7962, 7963, 7964,
];

/// The rogues' guild's panel.
pub const ROGUES_PANEL: FourCc = FourCc(*b"AP48");

/// The palace's panel: its counts, pages and build list.
pub const PALACE_PANEL: FourCc = FourCc(*b"AP39");

/// The outpost's panel.
pub const OUTPOST_PANEL: FourCc = FourCc(*b"MX08");

/// The palace panels, [`PALACE_PANEL`] and [`OUTPOST_PANEL`]
/// (`Outpost_Controller` derives `PalaceController`).
pub const PALACE_PANELS: [FourCc; 2] = [PALACE_PANEL, OUTPOST_PANEL];

/// The Hall of Champions' panel, and the Sorcerer's Abode's.
pub const HALL_PANEL: FourCc = FourCc(*b"MX00");
pub const ABODE_PANEL: FourCc = FourCc(*b"MX06");

/// The Fairgrounds' panel.
pub const FAIR_PANEL: FourCc = FourCc(*b"AP08");

/// The Gambling Hall's panel.
pub const GAMBLE_PANEL: FourCc = FourCc(*b"APb8");

/// A reward flag's panels, the owner's and another player's.
pub const ATTACK_PANELS: (FourCc, FourCc) = (FourCc(*b"AP46"), FourCc(*b"APb6"));
pub const EXPLORE_PANELS: (FourCc, FourCc) = (FourCc(*b"AP80"), FourCc(*b"APd0"));

/// Whose a piece is, against the local player.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Owner {
    Local,
    Other,
}

/// The panel a reward flag of descriptor `code` shows, by owner; none
/// for another piece, whose `DialogID` names its panel.
#[must_use]
pub const fn flag_panel(code: FourCc, owner: Owner) -> Option<FourCc> {
    let panels = match code {
        ATTACK_FLAG => Some(ATTACK_PANELS),
        EXPLORE_FLAG => Some(EXPLORE_PANELS),
        _ => None,
    };

    match (panels, owner) {
        (Some((own, _)), Owner::Local) => Some(own),
        (Some((_, other)), Owner::Other) => Some(other),
        (None, _) => None,
    }
}

/// What a building's research button, [`id::RESEARCH`], opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Opens {
    Research,
    Bounty,
}

/// What 8851 opens on a building of panel `panel`:
/// the Hall's bounties; nothing on the Abode, whose button opens its
/// spells; else its research.
#[must_use]
pub const fn research_opens(panel: Option<FourCc>) -> Option<Opens> {
    match panel {
        Some(HALL_PANEL) => Some(Opens::Bounty),
        Some(ABODE_PANEL) => None,
        _ => Some(Opens::Research),
    }
}

/// The Fairgrounds' contests' pictures, and its radios by `APB:5`
/// `CurrentEvent` 0 to 4: none, archery, melee, magic, combo.
pub const CONTEST_ICONS: [i32; 5] = [3001, 3002, 3003, 3004, 7];
pub const CONTESTS: [i32; 5] = [1204, 1201, 1200, 1202, 1203];

/// Whether a Fairgrounds holds contests: once the Tournament is
/// researched.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Contests {
    Closed,
    Open,
}

impl Contests {
    /// The contests of `piece`, on panel `panel`: none but on the
    /// Fairgrounds'.
    #[must_use]
    pub fn of(panel: Option<FourCc>, piece: Option<&Piece>) -> Option<Contests> {
        (panel == Some(FAIR_PANEL)).then(|| {
            if piece.is_some_and(|p| research::done(p, research::TOURNAMENT)) {
                Contests::Open
            } else {
                Contests::Closed
            }
        })
    }

    /// Whether control `id` shows: the contests once open, the research
    /// button until then.
    #[must_use]
    pub fn shows(self, id: i32) -> bool {
        let contest = CONTEST_ICONS.contains(&id) || CONTESTS.contains(&id);

        match self {
            Contests::Closed => !contest,
            Contests::Open => id != id::RESEARCH,
        }
    }
}

/// A temple hero's count on a warriors' guild's panel: its icon, the
/// count's box, the count.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Temple {
    pub icon: i32,
    pub count: i32,
    pub living: usize,
}

/// What a warriors' guild's counts show beside its warriors' 7949: the
/// frame, if any, and the temple heroes in it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Roster {
    pub frame: Option<i32>,
    pub temples: Vec<Temple>,
}

/// A warriors' guild's counts beside its portrait:
/// its warriors in 7949; one
/// temple hero's icon and count in the one-slot frame 7950, both in the
/// two-slot frame 7960, warriors of discord over paladins.
#[must_use]
pub fn war_roster(r: WarRoster) -> Roster {
    let temple = |(icon, count), living| Temple {
        icon,
        count,
        living,
    };

    match (r.discord, r.paladins) {
        (Some(d), Some(p)) => Roster {
            frame: Some(id::BOTH_FRAME),
            temples: vec![temple(id::BOTH_DISCORD, d), temple(id::BOTH_PALADINS, p)],
        },
        (Some(d), None) => Roster {
            frame: Some(id::ONE_FRAME),
            temples: vec![temple(id::ONE_DISCORD, d)],
        },
        (None, Some(p)) => Roster {
            frame: Some(id::ONE_FRAME),
            temples: vec![temple(id::ONE_PALADINS, p)],
        },
        (None, None) => Roster {
            frame: None,
            temples: vec![],
        },
    }
}

/// The heroes an outpost houses, beside its portrait
/// (`Outpost_Controller`, and method on an attribute
/// change): frame 7950, hero icon 7954, and in 7956 its living roster,
/// `guild_living_count(0)`. "This building's current number of
/// heroes" (`STRT MX08` 27). None on another panel.
#[must_use]
pub fn housed(panel: Option<FourCc>, members: Option<Members>) -> Option<Roster> {
    let (frame, (icon, count)) = (id::ONE_FRAME, id::HOUSED);

    members
        .filter(|_| panel == Some(OUTPOST_PANEL))
        .map(|m| Roster {
            frame: Some(frame),
            temples: vec![Temple {
                icon,
                count,
                living: m.living,
            }],
        })
}

/// The rogues' guild's extortion button (`AP48` 8009, hot key E):
/// `EXTORT (%d)` of the player's share, its line both shares, enabled
/// while the total is at least 1.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Extort {
    pub label: Option<String>,
    pub tip: Option<String>,
    pub able: Able,
}

impl Extort {
    /// The button of extortion `e` on panel `panel`: none but on the
    /// rogues' guild's.
    #[must_use]
    pub fn of(text: &Text, panel: FourCc, e: Option<Extortion>) -> Option<Extort> {
        e.filter(|_| panel == ROGUES_PANEL).map(|e| Extort {
            label: text
                .gmtx(EXTORT_TEXT)
                .map(|f| printf(&f, &[e.amount().into()])),
            tip: tip::extortion(text, (e.total, e.cut)),
            able: Able::of(e.ready()),
        })
    }
}

/// What a press of the extortion button does:
/// it sounds `EN01`; with a total it
/// says both shares and sends the extortion of guild `guild`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Extorted {
    pub sound: (Voice, Phase),
    pub said: Option<chat::Line>,
    pub sent: Option<Input>,
}

impl Extorted {
    /// A press on guild `guild`'s button, extortion `e`.
    #[must_use]
    pub fn of(e: Extortion, guild: u32) -> Extorted {
        let collected = e.total > 0;

        Extorted {
            sound: sound::extorted(),
            said: collected.then_some(chat::Line::Extorted {
                amount: e.amount(),
                kept: e.cut,
            }),
            sent: collected.then_some(Input::Extort(guild)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roster(paladins: Option<usize>, discord: Option<usize>) -> Roster {
        war_roster(WarRoster {
            warriors: 3,
            paladins,
            discord,
        })
    }

    #[test]
    fn one_temple_hero_takes_the_one_slot_frame_and_both_the_two_slot_one() {
        let one = roster(Some(2), None);

        assert_eq!(one.frame, Some(7950));
        assert_eq!(
            one.temples,
            [Temple {
                icon: 7953,
                count: 7956,
                living: 2
            }]
        );
        assert_eq!(roster(None, Some(1)).frame, Some(7950));
        assert_eq!(roster(None, Some(1)).temples[0].icon, 7954);

        let both = roster(Some(2), Some(1));

        assert_eq!(both.frame, Some(7960));
        assert_eq!(
            both.temples.iter().map(|t| t.icon).collect::<Vec<_>>(),
            [7962, 7961],
            "discord over paladins"
        );
        assert_eq!(
            roster(None, None),
            Roster {
                frame: None,
                temples: vec![]
            }
        );
    }

    #[test]
    fn the_band_stands_for_every_roster_frame_and_temple_control() {
        let shown = [
            roster(Some(1), None),
            roster(None, Some(1)),
            roster(Some(1), Some(1)),
        ];

        for r in shown {
            let controls = r
                .frame
                .into_iter()
                .chain(r.temples.iter().flat_map(|t| [t.icon, t.count]));

            for c in controls {
                assert!(BAND_IDS.contains(&c), "{c}");
            }
        }
    }

    #[test]
    fn the_outpost_counts_its_living_heroes_beside_its_portrait() {
        let members = Members {
            living: 4,
            dead: 1,
            spots: 6,
        };

        assert_eq!(
            housed(Some(OUTPOST_PANEL), Some(members)),
            Some(Roster {
                frame: Some(7950),
                temples: vec![Temple {
                    icon: 7954,
                    count: 7956,
                    living: 4
                }]
            })
        );
        assert_eq!(housed(Some(PALACE_PANEL), Some(members)), None);
        assert_eq!(housed(Some(OUTPOST_PANEL), None), None);
    }

    #[test]
    fn a_flag_shows_its_owners_panel_or_the_foreign_one() {
        let panel = |code, owner| flag_panel(code, owner).map(|p| p.to_string());

        assert_eq!(panel(ATTACK_FLAG, Owner::Local).as_deref(), Some("AP46"));
        assert_eq!(panel(ATTACK_FLAG, Owner::Other).as_deref(), Some("APb6"));
        assert_eq!(panel(EXPLORE_FLAG, Owner::Local).as_deref(), Some("AP80"));
        assert_eq!(panel(EXPLORE_FLAG, Owner::Other).as_deref(), Some("APd0"));
        assert_eq!(flag_panel(FourCc(*b"ABV1"), Owner::Local), None);
    }

    #[test]
    fn the_research_button_opens_the_halls_bounties_and_nothing_on_the_abode() {
        assert_eq!(research_opens(Some(HALL_PANEL)), Some(Opens::Bounty));
        assert_eq!(research_opens(Some(ABODE_PANEL)), None);
        assert_eq!(research_opens(Some(FAIR_PANEL)), Some(Opens::Research));
        assert_eq!(research_opens(None), Some(Opens::Research));
    }

    #[test]
    fn open_contests_hide_the_research_button_and_closed_ones_themselves() {
        for c in CONTEST_ICONS.iter().chain(&CONTESTS) {
            assert!(
                !Contests::Closed.shows(*c) && Contests::Open.shows(*c),
                "{c}"
            );
        }

        assert!(Contests::Closed.shows(id::RESEARCH) && !Contests::Open.shows(id::RESEARCH));
        assert!(Contests::Closed.shows(id::UPGRADE) && Contests::Open.shows(id::UPGRADE));
    }

    #[test]
    fn only_the_fairgrounds_holds_contests() {
        assert_eq!(Contests::of(Some(FAIR_PANEL), None), Some(Contests::Closed));
        assert_eq!(Contests::of(Some(GAMBLE_PANEL), None), None);
        assert_eq!(Contests::of(None, None), None);
    }

    #[test]
    fn an_empty_extortion_only_sounds() {
        let none = Extorted::of(Extortion { total: 0, cut: 0 }, 9);

        assert_eq!((none.said, none.sent), (None, None));
        assert_eq!(none.sound, sound::extorted());

        let some = Extorted::of(Extortion { total: 50, cut: 10 }, 9);

        assert_eq!(
            some.said,
            Some(chat::Line::Extorted {
                amount: 40,
                kept: 10
            })
        );
        assert_eq!(some.sent, Some(Input::Extort(9)));
    }
}
