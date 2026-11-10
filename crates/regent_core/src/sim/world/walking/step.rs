//! The frame's step over list ORDR: each unit's tracks run, its orders,
//! fades, actions and their peaks. **Binary** unless marked.

use super::*;
use crate::sim::world::piece_state;

impl World {
    /// The frame's orders: `GE_Level` over `snapshot`, ORDR as the
    /// frame began, units and missiles where each joined, then method. A
    /// missile launched this frame waits for the next. An engine
    /// dispatch, an impact's or a peak's function, stops it there, the
    /// rest kept: whether it ran to the end.
    pub(in crate::sim::world) fn walk(&mut self, snapshot: Vec<Listed>) -> bool {
        let now = self.now;
        let mut left = snapshot.into_iter();

        while let Some(listed) = left.next() {
            let before = self.dispatches.count();

            match listed {
                Listed::Unit(id) => {
                    self.run_path(id);
                    self.run_intents(id);
                    self.run_flash(id);
                    self.run_quakes(id);
                    self.run_timers(id);

                    let peak = self.units.tracks_mut(id).map(|t| {
                        let held = t.walker.holds();
                        let peak = t.walker.run_stage(now);

                        t.released |= held && !t.walker.acting();
                        peak
                    });

                    if peak == Some(Peak::Reached) {
                        self.peaked(id);
                    }

                    self.track_one(id);
                }
                Listed::Missile(handle) => self.fly(handle),
                Listed::Effector(handle) => self.effector_wait(handle),
                Listed::Clone(handle) => self.follow(handle),
            }

            if self.dispatches.count() > before {
                self.dispatches.pause(left.collect());
                return false;
            }
        }

        // Each entry's verdict, in ORDR's order, then ORDR kept to them:
        // `retain` visits each once, in order.
        let (flights, effectors) = (&self.effects.flights, &self.effectors);
        let clones = &self.effects.clones;
        let units = &self.units;
        let stays: Vec<bool> = self
            .dispatches
            .ordr()
            .iter()
            .map(|listed| match listed {
                Listed::Unit(id) => {
                    units.tracks(*id).is_some_and(Tracks::busy)
                        || units.get(*id).is_some_and(|u| u.looping)
                        || self.timing(*id)
                        || self.timing_out(*id)
                        || self.quaking(*id)
                }
                Listed::Missile(handle) => flights.flying(*handle),
                Listed::Effector(handle) => effectors.iter().any(|e| {
                    let pending =
                        e.waiting(now) || e.ending == super::super::effectors::Ending::After;

                    e.handle == *handle && !e.ended && pending
                }),
                Listed::Clone(handle) => clones.has(*handle),
            })
            .collect();
        let mut stays = stays.into_iter();

        self.dispatches.keep(|_| stays.next().unwrap_or_default());

        true
    }

    /// What the walker's orders did to track 1's `Animate`s
    /// ([`loops`]).
    fn track_one(&mut self, id: u32) {
        let played = self
            .units
            .tracks_mut(id)
            .map(|t| t.walker.take_played())
            .unwrap_or_default();
        // Nothing played, nothing changes: the rest is looked up only for
        // a play.
        let fixed =
            !played.is_empty() && self.units.tracks(id).is_some_and(|t| t.walker.is_fixed());

        // The last play stands.
        let animate = played.last().map(|p| {
            let facing = self.facing_word(id);
            let poses = self.body_of(id).and_then(|b| b.poses.as_ref());

            poses.is_some_and(|poses| loops(*p, poses, facing))
        });

        // A building's InitAction ends in `GSUnit`: its set, forced,
        // on the parts in place of the action's last frame, so an emptied
        // chest shows `Dead`, not `chest_open`'s full chest.
        if fixed {
            self.restyle(id, super::super::plume::Force::Forced);

            if self.damage_pick(id).is_some()
                && let Some(t) = self.units.tracks_mut(id)
            {
                t.walker.forget();
            }
        } else if let (false, Some(animate), Some(t)) = (fixed, animate, self.units.tracks_mut(id))
        {
            t.animate = animate;
        }
    }

    /// Track 1's [`Flash`], in the unit's turn.
    fn run_flash(&mut self, id: u32) {
        let now = self.now;

        if let Some(t) = self.units.tracks_mut(id)
            && let Some(f) = t.flash.as_mut()
            && !f.run(now)
        {
            t.flash = None;
        }
    }

    /// Track 0: the walker's top order, among the units about it.
    pub(super) fn run_path(&mut self, id: u32) {
        // A building has no track 0.
        let fixed = self.units.tracks(id).is_some_and(|t| t.walker.is_fixed());

        if !fixed && let Some(mut t) = self.units.take_tracks(id) {
            let now = self.now;
            let modifier = self.pieces.attribute(id, MOVE_RATE).unwrap_or(0);

            t.walker.modify_rate(modifier);

            // The planner reads the grid up to date.
            self.regrid();

            let nudged = match (
                self.board.ground(),
                self.among(id, (t.goal, t.avoid), t.walker.waiter()),
            ) {
                (Some(ground), Some(mut around)) => {
                    t.walker.among(ground, now, &mut around);
                    around.nudged()
                }
                _ => vec![],
            };

            // Path after wait: a FollowPath to the walker's goal
            // (`GE_OrderProcessor`, MovePath): a
            // point, or the followed unit's place when `OrderFollowUnit`
            // pushed the MovePath.
            for n in nudged {
                match t.goal {
                    Goal::Point(p) | Goal::Beside(_, Some(p)) => {
                        if self.order(n) {
                            let intent = Intent::FollowPath {
                                waypoints: vec![p],
                                next: 0,
                                info: 0,
                                range: 0,
                                callback: Callback::None,
                            };

                            self.give(n, intent, 0);
                        }
                    }
                    Goal::Beside(..) => self.lose(n, Why::Nudged),
                }
            }

            self.moved(id, &t.walker);
            self.units.entry(id).tracks = Some(t);
        }
    }

    /// Track 1: every order, from a snapshot.
    pub(super) fn run_intents(&mut self, id: u32) {
        let snapshot = self
            .units
            .tracks(id)
            .map(|t| t.intents.clone())
            .unwrap_or_default();

        for order in snapshot {
            let present = self
                .units
                .tracks(id)
                .is_some_and(|t| t.intents.contains(&order));

            if present
                && self.now.wrapping_sub(order.when) >= order.delta
                && let Run::End(callback) = self.run_order(id, &order.intent)
            {
                if let Some(t) = self.units.tracks_mut(id) {
                    t.intents.retain(|o| *o != order);
                }

                self.call(id, callback);
            }
        }
    }

    /// One run of a track 1 order.
    fn run_order(&mut self, id: u32, intent: &Intent) -> Run {
        match intent {
            Intent::FollowUnit { target, max, info } => self.follow_unit(id, *target, *max, *info),
            Intent::FollowPath { .. } => self.follow_path(id),
            Intent::Action {
                aim,
                ms,
                pose,
                armed,
                missile,
            } => self.act(id, *aim, *ms, pose.clone(), *armed, *missile),
            Intent::Fade { target, rate } => self.fade_run(id, *target, *rate),
        }
    }

    /// `OrderFade`, a run: the unit's translucency,
    /// its draw effects' level where their mode is 5, else 0, a step
    /// toward `target` in mode 5, its colour kept, the next run `rate`
    /// later; at `target` the order ends, and an end at 0 clears the
    /// draw effects (mode 1, `OrderStack`).
    pub(super) fn fade_run(&mut self, id: u32, target: u8, rate: u32) -> Run {
        let now = self.now;
        let tint = self.units.get(id).and_then(|u| u.tint).unwrap_or_default();
        let level = tint.transparency.unwrap_or(0);
        let next = match level.cmp(&target) {
            std::cmp::Ordering::Less => Some(level + 1),
            std::cmp::Ordering::Greater => Some(level - 1),
            std::cmp::Ordering::Equal => None,
        };

        if let Some(n) = next {
            self.units.entry(id).tint = Some(super::super::draw::Tint {
                transparency: Some(n),
                ..tint
            });

            if let Some(o) = self.units.tracks_mut(id).and_then(|t| {
                t.intents
                    .iter_mut()
                    .find(|o| o.intent == Intent::Fade { target, rate })
            }) {
                o.when = now;
                o.delta = rate;
            }

            Run::On
        } else {
            if let (0, Some(u)) = (target, self.units.get_mut(id)) {
                u.tint = None;
            }

            Run::End(Callback::None)
        }
    }

    /// Track 2's `InitAction` reached its cue or last frame:
    /// its shot, if any, its function,
    /// then its `cProc`.
    pub(super) fn peaked(&mut self, id: u32) {
        if let Some(shot) = self.units.tracks_mut(id).and_then(|t| t.shot.take()) {
            self.launch(id, shot);
        }

        // The action's overlay on the actor, for 0 ms.
        if let Some(code) = self.units.tracks_mut(id).and_then(|t| t.overlay.take()) {
            self.hang_overlay(id, code);
        }

        // Its peak phase.
        let peak = self.units.tracks_mut(id).and_then(|t| t.sounding.take());

        if let Some((sound, phase)) = peak.and_then(|a| Some((a.sound, a.peak?))) {
            self.act_cue(id, sound, phase);
        }

        // The action's function, between passes: a melee attack's
        // `make_attack` here.
        if let Some((script, aim)) = self.units.tracks_mut(id).and_then(|t| t.script.take()) {
            self.strike_at(id, &script, aim);
        }

        let proc = self
            .units
            .tracks_mut(id)
            .map_or(0, |t| std::mem::take(&mut t.proc));

        if proc == DYING_DONE {
            self.dying_done(id);
        }
    }

    /// Action sound: the action's sound, else the actor's own, in
    /// `phase`, at the actor.
    pub(super) fn act_cue(&mut self, id: u32, sound: Option<FourCc>, phase: FourCc) {
        let voice = sound.map_or(
            super::super::sound::Voice::Own,
            super::super::sound::Voice::Sound,
        );

        self.cue(
            super::super::sound::From::Agent(id),
            voice,
            super::super::sound::Phase::Id(phase),
            0.0,
        );
    }

    /// Script dispatch of action function `script` at actor `id`'s
    /// peak: the target its order aims at (the actor itself for
    /// [`Aim::None`], none for a point), and the point,:
    /// the aimed point, else the target's place (**Inferred**: no shipped
    /// attack reads it).
    fn strike_at(&mut self, id: u32, script: &super::super::missile::Script, aim: Aim) {
        let target = match aim {
            Aim::Unit(t) => self.unit(t).map(|_| t),
            Aim::None => Some(id),
            Aim::Point(_) => None,
        };
        let point = match aim {
            Aim::Point(p) => p,
            _ => target.and_then(|t| self.position(t)).unwrap_or_default(),
        };
        let actor = self.unit(id).map(|_| id);
        let args = super::super::missile::script_args(script.kind, actor, target, point);

        if let Some(args) = args {
            self.dispatch_script(&script.function, args);
        }
    }

    /// Unit lie down, a dying unit's peak:
    /// bit `0x10`, so it blocks no walker; its
    /// relation 1 units deleted as `DeleteAllEffectors` does;
    /// then a place search from its position, every other unit in the way,
    /// and unit move to there; last, the parts show
    /// [`LAID_SET`] where its art has it.
    ///
    /// **Game**, play5: ranger 45, stopped between half cells, dies at
    /// 262,100 ms and stands at the half cell it was leaving 1,400 ms on.
    /// The probed play's save: four dead monsters in `Dead`, frame 0.
    pub(in crate::sim::world) fn lie_down(&mut self, id: u32) {
        if let Some(place) = self.pieces.place_mut(id) {
            place.state |= piece_state::RUINED;
        }

        self.detach(id, None);

        if !self.lie_in_place(id) {
            self.lose(id, Why::NoRoom);
        }

        let laid = self
            .pose(id, LAID_SET)
            .is_some_and(|p| !p.frames.is_empty());

        if let Some(t) = self.units.tracks_mut(id).filter(|_| laid) {
            t.walker.lay(LAID_SET);
        }
    }

    /// Unit lie down's place search: from piece `id`'s position,
    /// margin and flags 0, every occupant but `id` in the way, whatever
    /// its layer; then unit move to
    /// there. Whether a place was found.
    pub(in crate::sim::world) fn lie_in_place(&mut self, id: u32) -> bool {
        let body = self.body_of(id).cloned();
        let spot = body
            .zip(self.position(id))
            .and_then(|(b, at)| self.place_among(&b, at, Blockers::NotSelf(id)));

        if let Some(to) = spot {
            self.put(id, to);
        }

        spot.is_some()
    }

    /// `OrderAction`, a run. Armed, its `ms` have
    /// passed: it ends. It ends too when its target is gone. As it ends,
    /// track 2's order takes its GPL agent. It waits while
    /// track 0 or 2 holds an order; else it arms: `when`
    /// now, `delta` its `ms`, and track 2 turns to its aim and plays its
    /// set (action turn, [`crate::sim::movement::action`]).
    ///
    /// With a projectile at a unit, action face turns toward the
    /// unit's method, drawing at a building ([`World::aim`]).
    ///
    /// Without the action's record it never ends, and a unit following
    /// something has an unknown place; without the actor's art track 2 is
    /// unknown, and so is its place.
    pub(super) fn act(
        &mut self,
        id: u32,
        aim: Aim,
        ms: Option<u32>,
        pose: Option<(Pose, Settle, Sets)>,
        armed: bool,
        missile: Option<super::super::missile::Missile>,
    ) -> Run {
        let now = self.now;
        let gone = matches!(aim, Aim::Unit(t) if self.any_piece(t).is_none());
        let following = self.units.tracks(id).is_some_and(Tracks::following);
        let busy = self
            .units
            .tracks(id)
            .is_none_or(|t| t.walker.moving() || t.walker.acting());

        if armed || gone {
            if let Some(t) = self.units.tracks_mut(id) {
                t.walker.hand_over();
            }

            Run::End(Callback::None)
        } else if busy {
            Run::On
        } else {
            let to = match (aim, missile) {
                (Aim::Unit(t), Some(m)) => self.aim_at(t, m).map(Some),
                (Aim::Unit(t), None) => self.position(t).map(Some),
                (Aim::Point(p), _) => Some(Some(p)),
                (Aim::None, _) => Some(None),
            };
            let want = to.map(|p| {
                p.zip(self.position(id))
                    .map(|(b, a)| direction(b.0 - a.0, b.1 - a.1))
            });

            let shot = missile.map(|m| super::super::missile::Shot {
                missile: m,
                target: match aim {
                    Aim::Unit(t) => super::super::missile::Target::Unit(t),
                    Aim::Point(p) => super::super::missile::Target::Point(p),
                    Aim::None => super::super::missile::Target::Unit(id),
                },
            });

            match (want, pose.clone(), self.units.tracks_mut(id)) {
                (Some(want), Some((played, settle, sets)), Some(t)) => {
                    t.shot = shot;

                    if let Some((proc, script)) = t.pending.take() {
                        t.proc = proc;
                        t.script = script;
                        t.overlay = t.pending_overlay.take();
                        t.sounding = t.pending_sounding.take();
                    }

                    let begin = t.sounding.and_then(|a| Some((a.sound, a.begin?)));

                    t.walker.plays(sets);

                    let peak = t.walker.act(want, played, settle, now);

                    if let Some(o) = t
                        .intents
                        .iter_mut()
                        .find(|o| matches!(o.intent, Intent::Action { .. }))
                    {
                        o.when = now;
                        o.delta = ms.unwrap_or(u32::MAX);
                        o.intent = Intent::Action {
                            aim,
                            ms,
                            pose,
                            armed: ms.is_some(),
                            missile,
                        };
                    }

                    // The `InitAction` pushed: its begin phase.
                    if let Some((sound, phase)) = begin {
                        self.act_cue(id, sound, phase);
                    }

                    if peak == Peak::Reached {
                        self.peaked(id);
                    }

                    // A building's one-frame set ends as it is pushed, and
                    // `GSUnit` puts its own set back at once: a
                    // script's shot shows no frame of the action's set.
                    if self.units.tracks(id).is_some_and(|t| t.walker.is_fixed()) {
                        self.track_one(id);
                    }
                }
                _ => self.lose(id, Why::Unacted),
            }

            if ms.is_none() && following {
                self.lose(id, Why::Endless);
            }

            Run::On
        }
    }

    /// `OrderFollowUnit`. The order holds the target's
    /// unit handle, which agent by handle looks up in unit
    /// list 3: the unit, not its GPL agent, so a unit `DeleteAgent` left
    /// standing, a building's ruin, is still followed.
    fn follow_unit(&mut self, id: u32, target: u32, max: i32, info: u32) -> Run {
        let now = self.now;
        let there = self.position(target);
        let close = match (self.pieces.contains(target), max) {
            (false, _) => None,
            (true, 0) => self.adjacent(id, target),
            (true, m) => self
                .footprint(id)
                .zip(self.footprint(target))
                .map(|(a, b)| space::within(a, b, m)),
        };
        let Some(t) = self.units.tracks_mut(id) else {
            return Run::End(Callback::None);
        };
        let rearm =
            |t: &mut Tracks| {
                if let Some(o) = t.intents.iter_mut().find(
                    |o| matches!(o.intent, Intent::FollowUnit { target: x, .. } if x == target),
                ) {
                    o.when = now;
                    o.delta = FOLLOW_MS;
                }
            };

        match close {
            None => Run::End(Callback::None),
            Some(false) => {
                if !t.walker.moving() && !t.acting() {
                    t.goal = Goal::Beside(target, there);
                    // path start passes the least distance,.
                    t.avoid = Avoid::of(info, max / crate::sim::units::PER_HALF);
                    t.walker.push_path(now);
                }

                rearm(t);
                Run::On
            }
            Some(true) => {
                if t.walker.moving() {
                    t.walker.arrive();
                }

                if info & KEEP == 0 {
                    Run::End(Callback::None)
                } else {
                    rearm(t);
                    Run::On
                }
            }
        }
    }

    /// `OrderFollowPath`. It never re-arms its timer,
    /// so from its push it runs every frame.
    fn follow_path(&mut self, id: u32) -> Run {
        let now = self.now;
        let Some(t) = self.units.tracks_mut(id) else {
            return Run::End(Callback::None);
        };
        let idle = !t.walker.moving() && !t.acting();
        let Some(Intent::FollowPath {
            waypoints,
            next,
            info,
            range,
            callback,
        }) = t
            .intents
            .iter_mut()
            .map(|o| &mut o.intent)
            .find(|i| matches!(i, Intent::FollowPath { .. }))
        else {
            return Run::End(Callback::None);
        };

        if !idle {
            Run::On
        } else if *next >= waypoints.len() {
            Run::End(*callback)
        } else {
            let point = waypoints[*next];

            *next += 1;

            if *info & KEEP != 0 && *next >= waypoints.len() {
                *next = 0;
            }

            t.goal = Goal::Point(point);
            t.avoid = Avoid::of(*info, *range / crate::sim::units::PER_HALF);
            t.walker.push_path(now);
            Run::On
        }
    }

    /// An order's callback. `0x1003` enters the building
    /// when the squared pad-rect gap to it is at most [`ENTER_GAP2`]; else
    /// the unit stays outside.
    pub(super) fn call(&mut self, id: u32, callback: Callback) {
        if let Callback::Enter(building) = callback {
            let close = self
                .footprint(id)
                .zip(self.footprint(building))
                .is_some_and(|(a, b)| space::gap2(a, b) <= ENTER_GAP2);

            if close {
                self.enter(id, building);
            }
        }
    }

    /// The walker's place as the piece's: its position,
    /// the half cell it claimed.
    pub(super) fn moved(&mut self, id: u32, walker: &Walker) {
        if let Some(place) = self.pieces.place_mut(id)
            && place.holder == 0
        {
            place.fixed = (walker.at[0], walker.at[1]);
            place.standing = walker.cell();
        }
    }

    /// Run the Action on track 1 as it is pushed (header bit 2).
    pub(super) fn run_action(&mut self, id: u32) {
        let action = self.units.tracks(id).and_then(|t| {
            t.intents
                .iter()
                .find(|o| matches!(o.intent, Intent::Action { .. }))
                .cloned()
        });

        if let Some(TrackOrder {
            intent:
                Intent::Action {
                    aim,
                    ms,
                    pose,
                    armed,
                    missile,
                },
            ..
        }) = action
            && let Run::End(_) = self.act(id, aim, ms, pose, armed, missile)
            && let Some(t) = self.units.tracks_mut(id)
        {
            t.intents
                .retain(|o| !matches!(o.intent, Intent::Action { .. }));
        }
    }
}
