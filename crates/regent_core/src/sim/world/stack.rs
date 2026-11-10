//! Overlays on a unit: where reward flags, effectors and message flags
//! hang and how they stack by `StackPriority`.
//! **Binary** unless marked.

use super::{World, effectors::NEARER};
use crate::base::fixed::{self, Fixed};

/// The attachment point an overlay with `AttachmentPointID` 0 hangs at:
/// `GenUnit` turns 0 into 1.
pub const DEFAULT_ATTACHMENT: u32 = 1;

/// 16.16 cells a stacked overlay moves per lower priority held: up one.
const LIFT: Fixed = fixed::ONE;

/// 16.16 cells per place among overlays of one priority: a quarter
/// (`0x40000`, a divide by 4.0, in `GSUnit`).
const PLACE: Fixed = Fixed::from_bits(1 << 14);

/// 16.16 cells an even count of one priority moves right: an eighth
/// (`fixed_div(1.0, 8.0)` in `GSUnit`).
const EVEN_SHIFT: Fixed = Fixed::from_bits(1 << 13);

/// `GSUnit`'s move of an overlay of `StackPriority`
/// `own` hung last on a holder already carrying overlays of priorities
/// `before`, in relation order: 16.16 cells onto its own offset. None
/// without a priority.
///
/// ```text
///  each priority 1 .. own − 1 held:    y − 1 cell
///  k of its own priority before it:    x − (k + 1) / 4 for k odd,
///                                      x + k / 4 for k even
///  k + 1, itself included, even:       x + 1/8
/// ```
///
/// A priority's count is overlay stack count's over relation 1;
/// its own comes last, so every one of its priority is before it.
#[must_use]
pub fn stack_offset(own: u32, before: &[u32]) -> (Fixed, Fixed) {
    stack_offset_among(own, before, 0)
}

/// [`stack_offset`] for an overlay with `later` of its own priority after
/// it in relation order: overlay stack count stops the before count
/// at the asking overlay, but every one adds to the total the even test
/// reads. So as an overlay leaves and the rest restack,
/// the first of two moves right an eighth, not left three. **Game**, the
/// probed Barren Waste: Hands of Steel and Stone Skin icons.
#[must_use]
pub fn stack_offset_among(own: u32, before: &[u32], later: usize) -> (Fixed, Fixed) {
    let held = |p: u32| before.iter().filter(|b| **b == p).count();
    let k = i32::try_from(held(own)).unwrap_or(i32::MAX);
    let total = k + i32::try_from(later).unwrap_or(0) + 1;
    let lifts = (1..own).filter(|p| held(*p) > 0).count();
    let dy = -LIFT * i32::try_from(lifts).unwrap_or(0);
    let side = if k % 2 == 1 { -(k + 1) } else { k };
    let even = if total % 2 == 0 {
        EVEN_SHIFT
    } else {
        Fixed::ZERO
    };

    if own == 0 {
        (Fixed::ZERO, Fixed::ZERO)
    } else {
        (side * PLACE + even, dy)
    }
}

impl World {
    /// `GenUnit`'s point on agent `target` for `AttachmentPointID`
    /// `attach`, 16.16 cells about it: x, y and height. Its `Hotspot`
    /// point, else set 8's (**Inferred**, as for effectors:
    /// [`super::missile::Hit::offset3_at`]).
    pub(super) fn hang_point(&self, target: u32, attach: u32) -> Option<[Fixed; 3]> {
        let facing = self.facing_word(target);

        self.body_of(target)
            .map(|b| b.hit.offset3_at(attach.max(DEFAULT_ATTACHMENT), facing))
    }

    /// The flags hung on agent `target`'s piece placed again among the
    /// overlays still on it, each against all the others, as an effector
    /// leaves it. **Game**, the probed play: attack flag 61 sits a cell
    /// lower on troll 60 than one lift while `wounded_large` plays, and
    /// rises the cell as it goes at 4891, the `Poison_icon` made with it
    /// still on; a new overlay moves none. **Binary**: a unit's delete,
    /// `GSUnit` as `GE_Level` drains it after the orders, walks
    /// its holder's relation 1 and re-runs `GSUnit`
    /// on each overlay of descriptor kind 4, a flag.
    /// **Inferred**: the leaving unit is no longer counted (its unlink is
    /// unread); the world restacks before the next frame's orders, which
    /// nothing between can see.
    pub fn restack(&mut self, target: u32) {
        let Some(holder) = self.pieces.handle(target) else {
            return;
        };
        let flags: Vec<(u32, u32)> = self
            .pieces
            .iter()
            .filter(|(_, p)| {
                p.place
                    .is_some_and(|pl| pl.holder == holder && pl.is_reward())
            })
            .map(|(id, p)| (*id, p.handle))
            .collect();

        for (id, handle) in flags {
            let Some(body) = self.body_of(id) else {
                continue;
            };
            let (own, attach) = (body.stack, body.attach);
            let point = self.hang_point(target, attach);
            let (others, later) = self.stacked_about(holder, target, handle, own);
            let (dx, dy) = stack_offset_among(own, &others, later);

            if let (Some(p), Some([x, y, _])) = (self.pieces.get_mut(id), point)
                && let Some(place) = p.place.as_mut()
            {
                place.fixed = (x + dx, y + NEARER + dy);
            }
        }

        self.restack_messages(holder);
    }

    /// The `StackPriority` of each overlay on the piece of agent `target`,
    /// handle `holder`, in handle order: flags hung on it, and effectors
    /// still on it (as `CheckEffector` finds them).
    pub(super) fn stacked(&self, holder: u32, target: u32) -> Vec<u32> {
        self.stacked_but(holder, target, 0)
    }

    /// The overlays on agent `target`, handle `holder`, about the one of
    /// handle `me` and priority `own`, for [`stack_offset_among`]: every
    /// other's priority, its own priority's later ones counted apart.
    /// **Inferred**: relation order is handle order.
    pub(super) fn stacked_about(
        &self,
        holder: u32,
        target: u32,
        me: u32,
        own: u32,
    ) -> (Vec<u32>, usize) {
        self.stacked_among(holder, (target, me, own), &[])
    }

    /// [`World::stacked_about`] with `leaving`'s overlays, handle and
    /// priority, counted as still in the holder's relation 1.
    pub(super) fn stacked_among(
        &self,
        holder: u32,
        (target, me, own): (u32, u32, u32),
        leaving: &[(u32, u32)],
    ) -> (Vec<u32>, usize) {
        let mut pairs = self.stacked_pairs(holder, target, me);

        pairs.extend(leaving.iter().filter(|(h, _)| *h != me));
        pairs.sort_unstable();

        let later = pairs.iter().filter(|(h, p)| *h > me && *p == own).count();
        let before = pairs
            .iter()
            .filter(|(h, p)| *h < me || *p != own)
            .map(|(_, p)| *p)
            .collect();

        (before, later)
    }

    /// [`World::stacked`] without the overlay of handle `but`.
    pub(super) fn stacked_but(&self, holder: u32, target: u32, but: u32) -> Vec<u32> {
        self.stacked_pairs(holder, target, but)
            .into_iter()
            .map(|(_, stack)| stack)
            .collect()
    }

    /// [`World::stacked_but`] with each overlay's handle. An effector
    /// counts until it is deleted, its end set played
    /// ([`super::effectors::Effector::hangs`]). **Game**: the tournament
    /// probe's attack flag 62 on ratman 39 keeps its lift at 6360 as a
    /// `wounded_large` leaves, the `damage_shield` past its `Wait` still
    /// playing its end set.
    fn stacked_pairs(&self, holder: u32, target: u32, but: u32) -> Vec<(u32, u32)> {
        let flags = self
            .pieces
            .values()
            .filter(|p| {
                p.place
                    .is_some_and(|pl| pl.holder == holder && pl.is_reward())
            })
            .map(|p| {
                (
                    p.handle,
                    self.descriptors.body(p.code).map_or(0, |b| b.stack),
                )
            });
        let effectors = self
            .effectors
            .iter()
            .filter(|e| e.agent == target && e.hangs(self.now))
            .map(|e| {
                let body = self.descriptors.body_named(&e.name);

                (e.handle, body.map_or(0, |(_, b)| b.stack))
            });
        let mut all: Vec<(u32, u32)> = flags
            .chain(effectors)
            .chain(self.messages_on(holder, but))
            .filter(|(h, _)| *h != but)
            .collect();

        all.sort_unstable();
        all
    }

    /// The effectors on agent `target` stacked again as an overlay
    /// leaves it, each against all the others: `GSUnit` re-runs
    /// `GSUnit` on each overlay its holder carries,
    /// as for its flags ([`World::restack`]); method measures the
    /// attachment point anew, at the holder's facing now. The effectors
    /// `leaving` still count toward each total: `GSUnit` restacks
    /// before the unit leaves its holder. **Game**: the bounty probe's
    /// `Champion_Icon` on ratman catapult 42 stands an even eighth out at
    /// the last save, though the `Damage_Shield` beside it is gone.
    pub(super) fn restack_hung(&mut self, target: u32, leaving: &[u32]) {
        let Some(holder) = self.pieces.handle(target) else {
            return;
        };
        let facing = self.facing_word(target);
        let point = |attach| {
            self.body_of(target).map(|b| {
                let [x, y, z] = b.hit.offset3_at(attach, facing);

                [x, y + NEARER, z]
            })
        };
        let on: Vec<(u32, u32, Option<[Fixed; 3]>)> = self
            .effectors
            .iter()
            .filter(|e| e.agent == target && e.hangs(self.now))
            .filter_map(|e| {
                let code = self.effectors.hung(e.handle)?.code;
                let own = self.descriptors.body(code)?;

                Some((e.handle, own.stack, point(own.attach)))
            })
            .collect();

        let gone: Vec<(u32, u32)> = leaving
            .iter()
            .filter_map(|h| {
                let code = self.effectors.hung(*h)?.code;

                Some((*h, self.descriptors.body(code)?.stack))
            })
            .collect();

        for (handle, own, base) in on {
            let (others, later) = self.stacked_among(holder, (target, handle, own), &gone);
            let (dx, dy) = stack_offset_among(own, &others, later);

            if let Some(h) = self.effectors.hung_mut(handle) {
                h.base = base.unwrap_or(h.base);
                h.spot = [h.base[0] + dx, h.base[1] + dy, h.base[2]];
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An attack flag, priority 2, alone: where it hangs.
    #[test]
    fn a_lone_overlay_stays_at_its_attachment_point() {
        assert_eq!(stack_offset(2, &[]), (Fixed::ZERO, Fixed::ZERO));
        assert_eq!(stack_offset(0, &[1, 2]), (Fixed::ZERO, Fixed::ZERO));
    }

    /// Below a priority 1 overlay it lifts a cell; beside a second flag
    /// it moves half a cell left and an eighth right, two cells in all;
    /// a third goes half a cell right.
    #[test]
    fn overlays_stack_up_by_priority_and_aside_by_place() {
        assert_eq!(stack_offset(2, &[1]), (Fixed::ZERO, -LIFT));
        assert_eq!(stack_offset(2, &[1, 1, 3]), (Fixed::ZERO, -LIFT));
        assert_eq!(
            stack_offset(2, &[2]),
            (-2 * PLACE + EVEN_SHIFT, Fixed::ZERO)
        );
        assert_eq!(stack_offset(2, &[2, 2]), (2 * PLACE, Fixed::ZERO));
        assert_eq!(
            stack_offset(3, &[2, 1, 3, 3, 3]),
            (-4 * PLACE + EVEN_SHIFT, -2 * LIFT)
        );
    }

    /// The first of two icons restacked: none of its priority before it,
    /// but two in all, so an eighth right; the second half a cell left
    /// and an eighth right, as when hung.
    #[test]
    fn a_restacked_overlay_counts_the_later_ones_in_its_total() {
        assert_eq!(stack_offset_among(1, &[], 1), (EVEN_SHIFT, Fixed::ZERO));
        assert_eq!(stack_offset_among(1, &[1], 0), stack_offset(1, &[1]));
        assert_eq!(stack_offset_among(1, &[1], 1), (-2 * PLACE, Fixed::ZERO));
    }
}
