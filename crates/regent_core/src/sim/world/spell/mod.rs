//! A character's spells: what it knows, when each is ready, and the four
//! builtins that read or spend them. **Binary** unless marked.
//!
//! ```text
//!  VehicleRec           every AllowedSpells id: an entry on the unit's
//!                       list: code, cast at the clock,
//!                       timeout 0: ready at once
//!  CastSpell            the entry's cast = clock, timeout = the action's
//!                       TimeoutDuration; then PerformAction, same args
//!  IsSpellAvailable     entry ready (clock - cast >= timeout, or arg 3
//!                       not 0) and CharacterLevel <= APV:11
//!  GetBestSpell         of the entries whose action's SpellType is the
//!                       asked list, ready, CharacterLevel <= APV:11:
//!                       the highest SpellRank, first on a tie; one with a
//!                       ValidationScript only if that script says so
//!  GetSpellAttribute    one of four descriptor words, by name
//! ```

pub mod cast;
pub mod illusion;
pub mod learn;
pub mod summon;
pub mod target;

use std::collections::{BTreeMap, BTreeSet};

use super::{Answer, Found, World, agent_ref, int_ref, put_bool, put_int, string_ref};
use crate::data::attrib::EXPERIENCE_LEVEL;
use crate::data::desc::Action;
use crate::data::fourcc::FourCc;
use crate::gpl::ops::{Accessor, OpError, Value};
use crate::gpl::vm::{Fault, Handle, Vm};
use crate::sim::movement::action::Pose;

use super::effectors::{INTRO_SET, LOOP_SET};

/// The set spell unit timeout done plays first as a spell unit's
/// timeout ends: `0x60` into unit play set.
pub const FADE_SET: u32 = crate::data::imag::SetKind::Die.id();

/// A set id's kind: its low 16 bits; the top byte counts layers
/// (`a_sets_variant_is_the_top_byte_of_its_id`).
const SET_KIND: u32 = 0xffff;

/// What `GetBestSpell` writes with no spell: `NOTHING`.
pub const NO_SPELL: &str = "NOTHING";

/// `GetSpellAttribute`'s names, `_stricmp`ed in this order.
pub const SPELL_FIELDS: [&str; 4] = [
    "effector_duration",
    "timeout_duration",
    "character_level",
    "spell_rank",
];

/// A spell's action as the four builtins read it: its `DACT` game part.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Spell {
    pub code: FourCc,
    /// `HEAD` name: `GetBestSpell`'s answer.
    pub name: String,
    pub effector: u32,
    pub timeout: u32,
    /// `SpellType`: the list `GetBestSpell` asks.
    pub list: u32,
    pub level: u32,
    pub rank: u32,
    /// `ValidationScript`.
    pub check: Option<String>,
}

impl Spell {
    /// `action`'s spell words; absent ones 0.
    #[must_use]
    pub fn of(action: &Action<'_>) -> Spell {
        Spell {
            code: action.head.id,
            name: String::from_utf8_lossy(action.head.name).into_owned(),
            effector: action.effector_duration().unwrap_or_default(),
            timeout: action.timeout_duration().unwrap_or_default(),
            list: action.spell_type().unwrap_or_default(),
            level: action.character_level().unwrap_or_default(),
            rank: action.spell_rank().unwrap_or_default(),
            check: action
                .validation_script()
                .map(|s| String::from_utf8_lossy(s).into_owned()),
        }
    }

    /// `GetSpellAttribute`'s word for `field`, folded.
    fn field(&self, field: &str) -> Option<u32> {
        let at = SPELL_FIELDS
            .iter()
            .position(|f| f.eq_ignore_ascii_case(field))?;

        [self.effector, self.timeout, self.level, self.rank]
            .get(at)
            .copied()
    }
}

/// One entry of a unit's spell list: word 2 the code, 3 when it was last
/// cast, 4 its timeout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Charge {
    pub code: FourCc,
    pub since: u32,
    pub timeout: u32,
}

impl Charge {
    /// A new entry as `VehicleRec` makes it: ready.
    #[must_use]
    pub const fn new(code: FourCc, now: u32) -> Charge {
        Charge {
            code,
            since: now,
            timeout: 0,
        }
    }

    /// Ready at `now`: `timeout <= now - since`, unsigned.
    #[must_use]
    pub const fn ready(&self, now: u32) -> bool {
        self.timeout <= now.wrapping_sub(self.since)
    }
}

/// Where best spell pick's walk stands.
enum Pick {
    /// The walk ended: the best spell, if any.
    Done(Option<Spell>),
    /// The candidate at entry `at` asks its `ValidationScript`, `best` the
    /// best before it.
    Asks {
        at: usize,
        best: Option<Spell>,
        candidate: Spell,
    },
}

/// A `GetBestSpell` waiting on a `ValidationScript` the host runs:
/// the walk's place ([`World::take_ask`], [`World::resume_ask`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Asking {
    agent: u32,
    list: u32,
    level: i32,
    at: usize,
    best: Option<Spell>,
    candidate: Spell,
    /// Taken by the host, which runs it.
    script: Option<String>,
}

/// Spell units' ends, `SetSpellUnitTimeout`, and which have passed.
#[derive(Clone, Debug, Default)]
pub(super) struct Timeouts {
    /// Game ms, by agent ([`World::expiry`]).
    ends: BTreeMap<u32, u32>,
    /// Spell units whose timeout has deleted them ([`World::expire`]).
    expired: BTreeSet<u32>,
}

impl Timeouts {
    /// Agent `id`'s end at game ms `end`.
    #[cfg(test)]
    pub(super) fn set(&mut self, id: u32, end: u32) {
        self.ends.insert(id, end);
    }

    /// Agent `id` gone: its timeout is spent, not forgotten, so
    /// [`World::expiry`] still tells when it was due.
    pub(super) fn spend(&mut self, id: u32) {
        if self.ends.contains_key(&id) {
            self.expired.insert(id);
        }
    }
}

impl World {
    /// best spell pick over a unit's entries for `list`, from entry
    /// `from` with `best` so far: each ready entry of the list within the
    /// level whose `SpellRank` beats the best so far is kept, but one with
    /// a `ValidationScript` only once the script, dispatched on the unit,
    /// returns nonzero: there the walk stops to ask.
    fn pick(
        &self,
        charges: &[Charge],
        (level, list): (i32, u32),
        from: usize,
        mut best: Option<Spell>,
    ) -> Pick {
        for (at, c) in charges.iter().enumerate().skip(from) {
            let Some(s) = self.descriptors.spell(c.code).filter(|_| c.ready(self.now)) else {
                continue;
            };
            let rank = best.as_ref().map_or(0, |b| b.rank);

            if s.list == list && s.level.cast_signed() <= level && s.rank > rank {
                if s.check.is_some() {
                    return Pick::Asks {
                        at,
                        best,
                        candidate: s.clone(),
                    };
                }

                best = Some(s.clone());
            }
        }

        Pick::Done(best)
    }

    /// The `ValidationScript` a `GetBestSpell` waits on, and its argument,
    /// the unit's agent: the host runs it and passes its result to
    /// [`World::resume_ask`].
    pub fn take_ask(&mut self) -> Option<(String, Vec<Value>)> {
        let asking = self.pending.asking.as_mut()?;
        let script = asking.script.take()?;

        Some((script, vec![Value::Agent(asking.agent)]))
    }

    /// Drop a `GetBestSpell` waiting on its script: no script to run, so
    /// its answer is unknown.
    pub fn cancel_ask(&mut self) {
        self.pending.asking = None;
    }

    /// Go on with the `GetBestSpell` waiting on its script, which returned
    /// `result`: the candidate is kept when it is not 0.
    ///
    /// # Errors
    ///
    /// An accessor's bad conversion writing the answer.
    pub fn resume_ask(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
        result: &Value,
    ) -> std::result::Result<Answer, Fault> {
        let Some(asking) = self.pending.asking.take() else {
            return Ok(Answer::Unknown);
        };
        let kept = match result {
            Value::Integer(n) => *n != 0,
            Value::Boolean(b) => *b,
            _ => false,
        };
        let best = if kept {
            Some(asking.candidate)
        } else {
            asking.best
        };
        let charges = self
            .pieces
            .get(asking.agent)
            .and_then(|p| p.spells.clone())
            .unwrap_or_default();
        let step = self.pick(&charges, (asking.level, asking.list), asking.at + 1, best);

        self.answer_pick(vm, args, step, (asking.agent, asking.level, asking.list))
    }

    /// Write a finished walk's answer, or wait on a candidate's script.
    fn answer_pick(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
        step: Pick,
        (agent, level, list): (u32, i32, u32),
    ) -> std::result::Result<Answer, Fault> {
        match step {
            Pick::Done(best) => {
                let name = best.map_or_else(|| NO_SPELL.to_owned(), |s| s.name);
                let ret = vm.arg(args, 0)?;

                match vm.object_mut(ret) {
                    Value::String(s) => name.clone_into(s),
                    _ => Err(OpError::BadConversion(Accessor::StringRef))?,
                }

                Ok(Answer::Given)
            }
            Pick::Asks {
                at,
                best,
                candidate,
            } => {
                self.pending.asking = Some(Asking {
                    agent,
                    list,
                    level,
                    at,
                    best,
                    script: candidate.check.clone(),
                    candidate,
                });

                Ok(Answer::Asks)
            }
        }
    }

    /// `GetBestSpell(agent, list)`, gpl get best spell: throws with no unit;
    /// else [`World::pick`]: written, or [`Answer::Asks`] while a candidate
    /// waits on its `ValidationScript`. Unknown where the unit's spells are
    /// not known.
    pub(super) fn best_spell(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let id = agent_ref(vm, args, 1)?;
        let list = int_ref(vm, args, 2)?.cast_unsigned();
        let piece = match self.found(id) {
            Found::Piece(p) => Some(p),
            Found::Unsure => None,
            Found::Gone => Err(Fault::throw(format!(
                "Function $GetBestSpell( agent#{} ) failed: unit no longer exists.",
                id.cast_signed()
            )))?,
        };
        let level = piece
            .and_then(|p| p.attributes.get(&EXPERIENCE_LEVEL.packed()).copied())
            .unwrap_or_default();
        let step = piece
            .and_then(|p| p.spells.as_deref())
            .map(|c| self.pick(c, (level, list), 0, None));

        match step {
            Some(step) => self.answer_pick(vm, args, step, (id, level, list)),
            None => Ok(Answer::Unknown),
        }
    }

    /// `IsSpellAvailable(agent, spell, [ignore timeout])`, gpl is spell available:
    /// throws with no unit or for a name no descriptor has; false without
    /// the entry; else ready (or argument 3 not 0) and `CharacterLevel` at
    /// most [`EXPERIENCE_LEVEL`] (`0xb565041` in best spell pick).
    pub(super) fn spell_available(
        &self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let id = agent_ref(vm, args, 1)?;
        let name = string_ref(vm, args, 2)?;
        let piece = match self.found(id) {
            Found::Piece(p) => Some(p),
            Found::Unsure => None,
            Found::Gone => Err(Fault::throw(format!(
                "Function $IsSpellAvailable( agent#{} ) failed: unit no longer exists.",
                id.cast_signed()
            )))?,
        };

        if !self.descriptors.described(&name.to_ascii_lowercase()) {
            Err(Fault::throw(format!(
                "IsSpellAvailable() couldn't find spell {name}"
            )))?;
        }

        let forced = args.len() > 3 && int_ref(vm, args, 3)? != 0;
        let spell = self.descriptors.spell_named(&name);
        let answer = piece.and_then(|p| {
            let charge = p
                .spells
                .as_deref()?
                .iter()
                .find(|c| spell.is_some_and(|s| s.code == c.code))
                .copied();
            let level = p
                .attributes
                .get(&EXPERIENCE_LEVEL.packed())
                .copied()
                .unwrap_or_default();

            Some(charge.zip(spell).is_some_and(|(c, s)| {
                (c.ready(self.now) || forced) && s.level.cast_signed() <= level
            }))
        });

        match answer {
            Some(b) => {
                let ret = vm.arg(args, 0)?;

                put_bool(vm, ret, b).map(|()| Answer::Given)
            }
            None => Ok(Answer::Unknown),
        }
    }

    /// `CastSpell(agent, spell, target)`, gpl cast spell: throws with no unit
    /// or for a name no descriptor has; with the entry, its cast becomes
    /// the clock and its timeout the action's, then `PerformAction` with
    /// the same arguments; without it, nothing.
    pub(super) fn cast_spell(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let id = agent_ref(vm, args, 1)?;
        let name = string_ref(vm, args, 2)?;

        if self.unit(id).is_none() {
            Err(Fault::throw(format!(
                "Function $CastSpell( agent#{} ) failed: unit no longer exists.",
                id.cast_signed()
            )))?;
        }

        if !self.descriptors.described(&name.to_ascii_lowercase()) {
            Err(Fault::throw(format!("Couldn't cast spell {name}")))?;
        }

        let spell = self.descriptors.spell_named(&name).cloned();
        let now = self.now;
        let charge = self.pieces.spells_mut(id).map(|list| {
            list.iter_mut()
                .find(|c| spell.as_ref().is_some_and(|s| s.code == c.code))
        });

        match (charge, spell) {
            (Some(Some(c)), Some(s)) => {
                c.since = now;
                c.timeout = s.timeout;
                self.perform(vm, args)
            }
            (Some(None), _) => Ok(Answer::Given),
            _ => Ok(Answer::Unknown),
        }
    }

    /// `SetSpellUnitTimeout(agent, ms)`, gpl set spell unit timeout: no unit, nothing;
    /// else a `Wait` of `ms` on the unit, callback `0x2004`,
    /// which ends it
    /// ([`World::expiry`]); the unit joins the back of ORDR and stays to
    /// its deletion. Returns nothing.
    pub(super) fn spell_timeout(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let id = agent_ref(vm, args, 1)?;
        let ms = int_ref(vm, args, 2)?;

        // A particle system's `Wait` stops it ([`World::time_plume`]).
        let plume = self.time_plume(id, ms.cast_unsigned());

        match self.found(id) {
            Found::Piece(_) if plume => Ok(Answer::Given),
            Found::Piece(_) => {
                if let Some(end) = self.spell_end(id, ms) {
                    self.pending.timeouts.ends.insert(id, end);

                    self.dispatches.list(super::Listed::Unit(id));
                }

                Ok(Answer::Given)
            }
            Found::Gone => Ok(Answer::Given),
            Found::Unsure => Ok(Answer::Unknown),
        }
    }

    /// When spell unit `id`, timed out after `ms` from now, is deleted.
    ///
    /// ```text
    ///  now + ms   the Wait ends: callback 0x2004 (the spell unit's timeout), mode 2
    ///             no set 0x60 in the art: the part's own set plays on,
    ///             a frame a rate, from the frame it shows
    ///  last frame callback 0x1005 with mode 3; the run after ends it:
    ///             mode 2, GenUnit,       a deferred delete: UnitDelete
    /// ```
    ///
    /// With [`FADE_SET`], that set from frame 0 instead. **Inferred**: a
    /// lone set has looped from the call, a frame a rate, so it shows
    /// frame `ms / rate` mod its frames when the Wait ends. Unknown for
    /// several sets and no [`FADE_SET`] ([`end_at`]).
    fn spell_end(&self, id: u32, ms: i32) -> Option<u32> {
        let poses = self.body_of(id)?.poses.as_ref()?;

        end_at(poses, self.now, u32::try_from(ms).ok()?, First::Animate)
    }

    /// Each spell unit whose [`World::expiry`] the clock has reached: its
    /// `UnitDelete` for this frame's `GE_Level`
    /// ([`World::take_deletes`]). **Game**: the probed play's first
    /// `farseeing`, timed in its decode's dispatch at game clock
    /// 121,600 (50 (f + 1), as a thread's `last` and research's
    /// `ButtonStartTime` show the decode's clock), expires at 125,000 and
    /// goes in that frame's deletes, before its pass. The trace checks'
    /// `Stage::Message` clock is a frame earlier, 50 f, so play5's four
    /// fit `<` there (**Inferred**: the frame of the last shown art frame
    /// is not read).
    pub(super) fn expire(&mut self) {
        let now = self.now;
        let due: Vec<u32> = self
            .pending
            .timeouts
            .ends
            .iter()
            .filter(|(id, end)| **end <= now && !self.pending.timeouts.expired.contains(id))
            .map(|(id, _)| *id)
            .collect();

        for id in due {
            self.pending.timeouts.expired.insert(id);

            self.dispatches.delete(id);
        }
    }

    /// Whether spell unit `id` is in ORDR: timed out and not yet gone.
    pub(super) fn timing_out(&self, id: u32) -> bool {
        self.pending.timeouts.ends.contains_key(&id) && !self.pending.timeouts.expired.contains(&id)
    }

    /// When the spell unit of agent `id` is deleted, game ms, where
    /// `SetSpellUnitTimeout` timed it and its art is known.
    #[must_use]
    pub fn expiry(&self, id: u32) -> Option<u32> {
        self.pending.timeouts.ends.get(&id).copied()
    }

    /// `GetSpellAttribute(spell, field)`, gpl get spell attribute: throws for a name no
    /// descriptor has or a field not one of [`SPELL_FIELDS`]; else the
    /// word. Unknown for a descriptor that is no action: the engine reads
    /// its words all the same.
    pub(super) fn spell_attribute(
        &self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let name = string_ref(vm, args, 1)?;
        let field = string_ref(vm, args, 2)?;

        if !self.descriptors.described(&name.to_ascii_lowercase()) {
            Err(Fault::throw(format!(
                "GetSpellAttribute() couldn't find spell {name}"
            )))?;
        }

        if !SPELL_FIELDS.iter().any(|f| f.eq_ignore_ascii_case(&field)) {
            Err(Fault::throw(format!(
                "GetSpellAttribute():Descriptor value {field} not found"
            )))?;
        }

        match self
            .descriptors
            .spell_named(&name)
            .and_then(|s| s.field(&field))
        {
            Some(v) => {
                let ret = vm.arg(args, 0)?;

                put_int(vm, ret, v.cast_signed()).map(|()| Answer::Given)
            }
            None => Ok(Answer::Unknown),
        }
    }
}

/// Which of a unit's two track 1 orders runs first in a frame, its
/// looping `Animate` or its timing `Wait`: the one pushed last, as
/// `OrderStack` pushes at the front and method runs front first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum First {
    /// A spell unit's: spell unit animate pushes its `Animate`
    /// after the script's `SetSpellUnitTimeout`.
    Animate,
    /// An effector's: `GenUnit` pushes its `Wait`
    /// after effector animate, so the frame the
    /// `Wait` ends the set has not stepped.
    Wait,
}

/// [`World::spell_end`], and an effector's end, for art `poses`, timed out at `now` after `ms`:
/// [`FADE_SET`] from frame 0 where the art has it, else
/// the set it shows plays on from the frame it shows:
/// its steps before the `Wait` ran, by `first`. Layers of one kind (the
/// id's low 16 bits) are one set, drawn together; layer 0, the lowest
/// id, times it. **Game**: `Got_Gold` (set 0x40, two layers, 16 frames)
/// of 0 ms stands 1,600 ms: play5's and play3's saves hold each made
/// within that, none older; `got_gold_bldg` likewise, 800 ms.
/// `None` for several sets but not [`INTRO_SET`] and [`LOOP_SET`]: which
/// the unit shows is not read.
pub(super) fn end_at(poses: &super::bodies::Poses, now: u32, ms: u32, first: First) -> Option<u32> {
    let stepped = match first {
        First::Animate => ms,
        First::Wait => ms.saturating_sub(1),
    };

    // `GenUnit` skips an effector's animation for a 0 ms `Wait`.
    let animates = first == First::Animate || ms != 0;
    let opening = poses
        .get(&INTRO_SET)
        .zip(poses.get(&LOOP_SET))
        .filter(|_| animates);
    let (own, shown) = shown_at(poses, opening, stepped)?;
    let frames = own.frames_at(0).max(1);

    // 32-bit, as the original: a negative script duration wraps.
    Some(
        now.wrapping_add(ms)
            .wrapping_add((frames - shown).wrapping_mul(own.rate())),
    )
}

/// The set art `poses` plays on as its `Wait` ends, `stepped` ms of
/// steps after it began, and the frame it shows.
///
/// ```text
///  set 0x60           from frame 0
///  one set            looped from the start (Inferred)
///  0x50, then 0x40    0x50 once; callback 0x2003 or 0x100c (the same
///                     body) then 0x40 looped from frame 0
/// ```
///
/// **Inferred**: 0x50 ends after its last frame shows, as
/// [`super::effectors`] has it.
fn shown_at<'p>(
    poses: &'p super::bodies::Poses,
    opening: Option<(&'p Pose, &'p Pose)>,
    stepped: u32,
) -> Option<(&'p Pose, u32)> {
    let kinds: BTreeSet<u32> = poses.keys().map(|id| id & SET_KIND).collect();
    let lone = kinds.len() == 1;
    let looped = |p: &Pose, ms: u32| ms / p.rate() % p.frames_at(0).max(1);

    match (poses.get(&FADE_SET), lone, opening) {
        (Some(fade), _, _) => Some((fade, 0)),
        (None, true, _) => poses.values().next().map(|p| (p, looped(p, stepped))),
        (None, false, Some((intro, lp))) => {
            let length = intro.frames_at(0).max(1) * intro.rate();

            if stepped < length {
                Some((intro, stepped / intro.rate()))
            } else {
                Some((lp, looped(lp, stepped - length)))
            }
        }
        (None, false, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::movement::action::Pose;

    fn pose(frames: u16) -> Pose {
        Pose {
            frames: vec![frames],
            playback: 0,
            keeps: 0,
            cues: vec![None],
        }
    }

    /// `Damage_Shield`'s one set, 19 frames at 100 ms: its 40,000 ms
    /// `Wait` runs before the set's 400th step, so it shows frame 0 and
    /// plays 19 frames. **Game**, the defeat probe: `CheckEffector` finds
    /// champion 12's shield 1,850 ms after its `Wait` ends; shown frame 1,
    /// it would be gone at 1,800.
    #[test]
    fn an_effectors_wait_ends_before_its_set_steps() {
        let poses = super::super::bodies::Poses::from([(0x40, pose(19))]);

        assert_eq!(
            end_at(&poses, 1000, 40_000, First::Wait),
            Some(1000 + 40_000 + 1900)
        );
    }

    /// Farseeing's one set, 17 frames at 100 ms: after 2,500 ms it shows
    /// frame 8, and plays on to its last.
    #[test]
    fn a_spell_unit_of_one_set_plays_it_on_to_its_last_frame() {
        let poses = super::super::bodies::Poses::from([(0x40, pose(17))]);

        assert_eq!(
            end_at(&poses, 1000, 2500, First::Animate),
            Some(1000 + 2500 + 900)
        );
    }

    /// Wind storm's art (`DRB1`): set 0x50, 9 frames, then 0x40, 15,
    /// both at 100 ms, no 0x60. Its 4,700 ms `Wait` ends 3,800 ms into
    /// the loop: frame 8 shows, and the loop plays on to its last.
    #[test]
    fn a_spell_unit_of_intro_and_loop_ends_after_its_loop() {
        let poses = super::super::bodies::Poses::from([(0x50, pose(9)), (0x40, pose(15))]);

        assert_eq!(
            end_at(&poses, 1000, 4700, First::Animate),
            Some(1000 + 4700 + 700)
        );
    }

    /// The same art timed out within its intro: frame 4 of 0x50 shows,
    /// and the intro plays on to its last; no loop.
    #[test]
    fn a_spell_unit_timed_out_in_its_intro_ends_after_it() {
        let poses = super::super::bodies::Poses::from([(0x50, pose(9)), (0x40, pose(15))]);

        assert_eq!(
            end_at(&poses, 1000, 400, First::Animate),
            Some(1000 + 400 + 500)
        );
    }

    /// Gate's art has set 0x60, nine frames: it plays from frame 0 as the
    /// `Wait` ends, whatever else the art holds.
    #[test]
    fn a_spell_unit_with_a_fade_set_plays_it_whole() {
        let poses = super::super::bodies::Poses::from([
            (0x50, pose(7)),
            (0x40, pose(15)),
            (FADE_SET, pose(9)),
        ]);

        assert_eq!(
            end_at(&poses, 1000, 2500, First::Animate),
            Some(1000 + 2500 + 900)
        );
    }

    /// `Got_Gold`'s art is set 0x40 in two layers, 16 frames at 100 ms:
    /// one set, so a 0 ms effector of it plays all 16 frames.
    /// A negative script duration reaches here as a huge `ms`; the
    /// original's 32-bit sum wraps.
    #[test]
    fn an_end_past_the_clock_wraps() {
        let poses = super::super::bodies::Poses::from([(0x40, pose(19))]);

        assert_eq!(
            end_at(&poses, 1000, u32::MAX, First::Wait),
            Some(1000 + 1800 - 1)
        );
    }

    #[test]
    fn layers_of_one_set_play_as_that_set() {
        let poses = super::super::bodies::Poses::from([(0x40, pose(16)), (0x0100_0040, pose(16))]);

        assert_eq!(end_at(&poses, 1000, 0, First::Wait), Some(1000 + 1600));
    }

    /// A spell unit whose expiry the frame's clock reaches dispatches its
    /// `UnitDelete` from that frame's deletes, once.
    #[test]
    fn a_timed_out_spell_unit_is_deleted_in_the_frame_it_expires() {
        let mut world = World::default();
        let deletes = |w: &mut World| w.take_deletes().len();

        world.pending.timeouts.ends.insert(40, 284_100);
        world.now = 284_050;
        world.expire();

        assert_eq!(deletes(&mut world), 0);

        world.now = 284_100;
        world.expire();

        assert_eq!(deletes(&mut world), 1);

        world.now = 284_150;
        world.expire();

        assert_eq!(deletes(&mut world), 0);
        assert_eq!(world.expiry(40), Some(284_100));
    }
}

/// To and from the save model.
mod persist {
    use super::*;
    use crate::save::{self as m, Refused, Saved};
    use std::result::Result;

    impl Saved for Spell {
        type Model = m::Spell;

        fn save(&self) -> m::Spell {
            let Spell {
                code,
                name,
                effector,
                timeout,
                list,
                level,
                rank,
                check,
            } = self;

            m::Spell {
                code: code.save(),
                name: name.clone(),
                effector: *effector,
                timeout: *timeout,
                list: *list,
                level: *level,
                rank: *rank,
                check: check.clone(),
            }
        }

        fn load(model: m::Spell) -> Result<Spell, Refused> {
            Ok(Spell {
                code: FourCc::load(model.code)?,
                name: model.name,
                effector: model.effector,
                timeout: model.timeout,
                list: model.list,
                level: model.level,
                rank: model.rank,
                check: model.check,
            })
        }
    }

    impl Saved for Charge {
        type Model = m::Charge;

        fn save(&self) -> m::Charge {
            m::Charge {
                code: self.code.save(),
                since: self.since,
                timeout: self.timeout,
            }
        }

        fn load(model: m::Charge) -> Result<Charge, Refused> {
            Ok(Charge {
                code: FourCc::load(model.code)?,
                since: model.since,
                timeout: model.timeout,
            })
        }
    }

    impl Saved for Asking {
        type Model = m::Asking;

        fn save(&self) -> m::Asking {
            let Asking {
                agent,
                list,
                level,
                at,
                best,
                candidate,
                script,
            } = self;

            m::Asking {
                agent: *agent,
                list: *list,
                level: *level,
                at: at.save(),
                best: best.save(),
                candidate: candidate.save(),
                script: script.clone(),
            }
        }

        fn load(model: m::Asking) -> Result<Asking, Refused> {
            Ok(Asking {
                agent: model.agent,
                list: model.list,
                level: model.level,
                at: usize::load(model.at)?,
                best: Option::load(model.best)?,
                candidate: Spell::load(model.candidate)?,
                script: model.script,
            })
        }
    }

    impl Saved for Timeouts {
        type Model = m::Timeouts;

        fn save(&self) -> m::Timeouts {
            m::Timeouts {
                ends: self.ends.clone(),
                expired: self.expired.clone(),
            }
        }

        fn load(model: m::Timeouts) -> Result<Timeouts, Refused> {
            Ok(Timeouts {
                ends: model.ends,
                expired: model.expired,
            })
        }
    }
}
