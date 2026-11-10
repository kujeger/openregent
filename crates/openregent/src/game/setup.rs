//! [`Setup`]'s ties to the game: a save's mods, the menus' freestyle
//! config.

use regent_core::sim::world::cheat::Cheats;

use crate::menu::freestyle::Custom;
use crate::save::Needs;
use crate::setup::{Setup, Winning};

impl Setup {
    /// The same, each mod compiled from source marked with its images'
    /// hash as `needs` holds it, so a replay of other source is refused.
    ///
    /// ```text
    ///  {A2A4…},{17BF…}   ──>   {A2A4…}@0123456789abcdef,{17BF…}
    /// ```
    #[must_use]
    pub fn stamped(mut self, needs: &Needs) -> Setup {
        for wanted in &mut self.mods {
            let image = needs
                .mods
                .iter()
                .find(|m| regent_core::data::guid::Guid::parse(&m.guid) == Some(wanted.guid))
                .and_then(|m| m.image.clone());

            if image.is_some() {
                wanted.image = image;
            }
        }

        self
    }

    /// The same, asking for the freestyle game `custom` holds, as its
    /// options would.
    #[must_use]
    pub fn freestyle(self, custom: &Custom) -> Setup {
        Setup {
            land: Some(custom.land),
            settlement: Some(custom.settlement),
            scenario: Some(custom.scenario),
            monsters: Some(custom.monsters),
            map_seed: Some(custom.map_number),
            gold: custom.gold,
            victory: Winning::of(custom.victory),
            goal: Some(custom.goal),
            events: custom.events,
            limits: Some(custom.caps.clone()).filter(|c| !c.is_empty()),
            ..self
        }
    }

    /// The freestyle game `custom` holds, hosted on the LAN for
    /// `players`, its cheat rule the session's.
    pub fn hosted(custom: &Custom, players: u32) -> Setup {
        Setup {
            players,
            cheats: Some(custom.cheats),
            ..Setup::default().freestyle(custom)
        }
    }

    /// The freestyle config it holds, as a guest's lobby shows it: the
    /// dialog's default where not given. The tree's name is not in it,
    /// its caps are.
    pub fn custom(&self) -> Custom {
        let (victory, goal) = self.aim();
        let given = Custom::default();

        Custom {
            settlement: self.settlement.unwrap_or(given.settlement),
            scenario: self.scenario.unwrap_or(given.scenario),
            monsters: self.monsters.unwrap_or(given.monsters),
            land: self.land.unwrap_or(given.land),
            map_number: self.map_seed.unwrap_or(given.map_number),
            gold: self.gold,
            victory,
            goal,
            events: self.events,
            cheats: self.cheats.unwrap_or(Cheats::Allowed),
            caps: self.caps(),
            ..given
        }
    }
}
