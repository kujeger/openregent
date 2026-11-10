//! The embassy's open and close: `GUILD_ACTION_MESSAGE` action `0x16`,
//! and the recruit timer it runs. **Binary** unless marked; no trace
//! holds either.
//!
//! ```text
//!  panel   Embassy_Controller, controls 8875 and 8876 (open, close):
//!          GUILD_ACTION 0x16, the embassy's handle
//!  decode  guild action apply, case 0x16, the embassy with an agent:
//!          open (AEM:4 set): GenUnit (1, 0x4009), the timer gone
//!          shut: GS_OrderProcessor; then AEM:4 = not what it was
//!          Embassy_Init(agent) dispatched; AEM:3, the duration, over 0:
//!          AEM:2 0, an order of kind 0x4009 on track 1, a tick each
//!          50 ms, counting duration / 50 ticks
//!  a tick  GS_EmbassyRecruitOrder: AEM:5 whether the owner has a
//!          finished guild with room (embassy open guild); with it
//!          and the treasury holding AEM:1, a tick off and AEM:2 the time
//!          run; agent gone: the order ends; ticks left: again in 50 ms;
//!          none: Embassy_Spawn(agent), again, the order ends
//! ```
//!
//! The script picks the hero and pays (`Embassy_Spawn`); `Embassy_Init`
//! draws the duration and cost when they are 0.

use crate::data::attrib::{BUTTON_DELAY_TIME, FIRST_STAGE_BUILT, Key, MAX_GUILD_MEMBERS};
use crate::gpl::ops::Value;
use crate::sim::world::timed::Timer;
use crate::sim::world::{World, offer::Button};

/// What `GS_OrderProcessor` dispatches, `"Embassy_Init"`.
pub const EMBASSY_INIT: &str = "Embassy_Init";

/// What a timer's last tick dispatches, `"Embassy_Spawn"`.
pub const EMBASSY_SPAWN: &str = "Embassy_Spawn";

/// The embassy's code prefix.
pub const EMBASSY: [u8; 3] = *b"ABr";

/// A tick's interval, ms: the order's, `0x32`.
pub const EMBASSY_TICK: u32 = 50;

/// `AEM:1` `EmbassyRecruitCost`, `AEM:2` `EmbassyTimerProgress`, `AEM:3`
/// `EmbassyTimerDuration`, `AEM:4` `EmbassyActiveFlag`, `AEM:5`
/// `EmbassyHasSlot`.
const COST: Key = Key::new(*b"AEM", 1);
const PROGRESS: Key = Key::new(*b"AEM", 2);
const DURATION: Key = Key::new(*b"AEM", 3);
const ACTIVE: Key = Key::new(*b"AEM", 4);
const HAS_SLOT: Key = Key::new(*b"AEM", 5);

impl World {
    /// Whether embassy `id` is open, `AEM:4`.
    #[must_use]
    pub fn embassy_open(&self, id: u32) -> bool {
        self.word_of(id, ACTIVE) != 0
    }

    /// Action `0x16` on an open embassy: its timer gone
    /// (`GenUnit` method `(1, 0x4009)`), then the flag cleared.
    pub fn embassy_close(&mut self, id: u32) {
        self.untime_embassy(id);
        self.write(id, ACTIVE.packed(), 0);
    }

    /// Action `0x16`'s flag on a shut embassy, after `Embassy_Init`.
    pub fn embassy_opened(&mut self, id: u32) {
        self.write(id, ACTIVE.packed(), 1);
    }

    /// `GS_OrderProcessor` after its `Embassy_Init`: a duration
    /// over 0 arms the timer, `AEM:2` 0. A new order's first tick is in
    /// the embassy's next ORDR turn, which for the open's is this frame's;
    /// re-armed by a last tick, the new order runs in that same turn
    /// (**Game**: the probe's second spawn, 971 ticks, comes 970 frames
    /// after the first).
    pub(in crate::sim::world) fn embassy_arm(&mut self, id: u32) {
        let duration = self.word_of(id, DURATION);
        let rearmed = self.pending.rearming.remove(&id);

        if duration > 0 {
            let ticks = (duration.cast_unsigned() / EMBASSY_TICK).cast_signed();

            self.write(id, PROGRESS.packed(), 0);

            if rearmed {
                self.embassy_tick(id, ticks);
            } else {
                self.time(id, Timer::Embassy(ticks), 0);
            }
        }
    }

    /// `GS_EmbassyRecruitOrder`: one tick with `left` ticks to go.
    pub(in crate::sim::world) fn embassy_tick(&mut self, id: u32, left: i32) {
        let owner = self.pieces.owner(id);
        let cost = self.word_of(id, COST);
        let rich = owner
            .and_then(|o| self.treasury(o))
            .is_none_or(|g| g >= cost);
        let room = owner.is_some_and(|o| self.open_guild(o).is_some());
        let counted = rich && room;
        let left = left - i32::from(counted);

        self.write(id, HAS_SLOT.packed(), i32::from(room));

        if counted {
            let tick = EMBASSY_TICK.cast_signed();
            let run = self.word_of(id, DURATION) - left * tick;

            self.write(id, PROGRESS.packed(), run);
        }

        // Agent gone: the order ends. Ticks left: again. None: the spawn,
        // then method again (`Embassy_Init`, armed as it ends).
        if !self.agents.alive(id) {
        } else if left > 0 {
            self.time(id, Timer::Embassy(left), EMBASSY_TICK);
        } else {
            self.pending.rearming.insert(id);
            self.dispatch(EMBASSY_SPAWN, vec![Value::Agent(id)]);
            self.dispatch(EMBASSY_INIT, vec![Value::Agent(id)]);
        }
    }

    /// Embassy open guild: `owner`'s first building
    /// of unit list 0 standing (`GenUnit` under 3), finished
    /// (`APB:1`), a guild (`IsGuild`) with room (guild has room:
    /// fewer living than `MaxGuildMembers` and no recruit running, or
    /// fewer than one short of it). **Inferred**: list 0 in agent order.
    #[must_use]
    pub fn open_guild(&self, owner: u32) -> Option<u32> {
        let mut ids: Vec<u32> = self
            .pieces
            .iter()
            .filter(|(id, p)| {
                p.owner == owner
                    && self
                        .known(**id)
                        .is_some_and(|k| k.is_building() && !k.ruined())
            })
            .map(|(id, _)| *id)
            .collect();

        ids.sort_unstable();
        ids.into_iter().find(|id| {
            let guild = self.piece_body(*id).is_some_and(|b| b.guild);
            let max = self.word_of(*id, MAX_GUILD_MEMBERS);
            let living = self.roster(*id).iter().filter(|(_, dead)| !dead).count();
            let living = i32::try_from(living).unwrap_or(i32::MAX);
            let room =
                (living < max && self.word_of(*id, BUTTON_DELAY_TIME) == 0) || living < max - 1;

            guild && self.word_of(*id, FIRST_STAGE_BUILT) != 0 && room
        })
    }

    /// `owner`'s embassy `id`'s open or close button
    /// (`Embassy_Controller`, 8875 and 8876): what it does, open
    /// (`true`) or close; no price. **Inferred**: always ready; the
    /// panel's state was not read. `None` for any other piece.
    #[must_use]
    pub fn embassy_offer(&self, owner: u32, id: u32) -> Option<Button<bool>> {
        self.pieces.owned(id, owner, &EMBASSY).map(|_| Button {
            what: !self.embassy_open(id),
            price: 0,
            ready: true,
        })
    }

    /// Embassy `id`'s timer: run and duration, ms, and the next hero's
    /// price (`AEM:2`, `AEM:3`, `AEM:1`), as its panel's bar shows it.
    #[must_use]
    pub fn embassy_timer(&self, id: u32) -> (i32, i32, i32) {
        (
            self.word_of(id, PROGRESS),
            self.word_of(id, DURATION),
            self.word_of(id, COST),
        )
    }
}
