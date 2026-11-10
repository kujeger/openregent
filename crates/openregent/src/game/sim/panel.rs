//! What the panels ask of the game and hand it: reward settings, the
//! camp move, placements, guild lists.

use super::*;

impl Sim {
    /// The reward panel's settings.
    pub const fn rewards(&self) -> Rewards {
        self.rewards
    }

    /// The reward panel's settings moved (`Reward1Controller`, no
    /// message).
    pub const fn set_rewards(&mut self, rewards: Rewards) {
        self.rewards = rewards;
    }

    /// The camp move this machine began and has not sent or left.
    pub const fn camp_move(&self) -> Option<CampMove> {
        self.camp
    }

    /// The local player begins moving `guild`'s camp at `price`, the
    /// panel's: `CampBegin` taken.
    pub fn camp_begin(&mut self, guild: u32, price: i32) {
        if self.takes_orders() {
            self.camp = Some(CampMove { guild, price });
            self.input(Input::CampBegin(guild));
        }
    }

    /// The camp move's inputs for a fitting spot: the move sent, its
    /// price kept.
    pub fn camp_place(&mut self, camp: Camp) -> Vec<Input> {
        self.camp = None;
        vec![Input::Camp(camp)]
    }

    /// The camp move left: `CampLeave`, its price back. Nothing without
    /// one.
    pub fn camp_leave(&mut self) {
        if self.takes_orders() && self.camp.take().is_some() {
            self.input(Input::CampLeave);
        }
    }

    /// Whether the local player's `code` may go at `at`, 32nds: the
    /// world's verdict, a decode pending, then `CanIBuildThisBuilding`
    /// (`Game::placement`). A build given and not yet sent is pending as
    /// one taken is.
    pub fn placement(&mut self, menu: &Menu, code: FourCc, at: (i32, i32)) -> Placement {
        let local = self.local();
        let given = self.outbox.iter().any(|e| {
            matches!(e, Event::Input { order, .. }
                if order.player == local && matches!(order.input, Input::Build(_)))
        });

        if given {
            match self
                .world()
                .placement(self.program(), menu, local, code, at)
            {
                verdict @ (Placement::Disabled | Placement::Unmet) => verdict,
                _ => Placement::Pending,
            }
        } else {
            self.game.placement(menu, local, code, at)
        }
    }

    /// What the local player's Mausoleum `id` offers to raise
    /// (`Game::crypt`). Empty for any other piece.
    pub fn crypt(&mut self, id: u32) -> Vec<Button<u32>> {
        let local = self.local();

        self.game.crypt(local, id)
    }

    /// What the local player's palace `id` supports, as its panel asks
    /// `GetSupportedHenchmen` (`Game::supported_henchmen`).
    pub fn henchmen(&mut self, id: u32) -> [Option<i32>; 3] {
        self.game.supported_henchmen(id)
    }

    /// What the local player's rogues' guild `id` would extort, as its panel
    /// asks (`Game::extortion`).
    pub fn extortion(&mut self, id: u32) -> Option<Extortion> {
        self.game.extortion(id)
    }

    /// The bounties, short then long, as the Hall of Champions' page
    /// offers them (`Game::bounties`).
    pub fn bounties(&mut self) -> [Option<Button<Term>>; 2] {
        let local = self.local();

        self.game.bounties(local)
    }

    /// Whether deathmatch rules hold, as a hero's skill caps ask
    /// (`Game::rules`).
    pub fn rules(&self) -> regent_core::sim::world::offer::Rules {
        self.game.rules()
    }
}
