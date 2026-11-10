//! The builtins that give a unit its orders and read them: `Move`,
//! `StopMoving`, `Hide`, `UnHide`, `PerformAction` and kin. **Binary**
//! unless marked.

use super::*;
use crate::sim::world::piece_state;

impl World {
    /// Push a track 1 order run on push (header bit 2), as `Move` and
    /// `Hide` do: a unit with no order before joins the back of ORDR.
    pub(super) fn push(&mut self, id: u32, intent: Intent) {
        let now = self.now;

        if let Some(t) = self.tracks_of(id) {
            t.intents.insert(
                0,
                TrackOrder {
                    intent,
                    when: now,
                    delta: 0,
                },
            );
        }

        self.dispatches.list(Listed::Unit(id));

        self.run_intents(id);
    }

    /// The count of piece `id`'s `Flash` while one runs, the order's:
    /// the minimap skips the piece on an odd count
    /// (`view::panel::minimap::blinked`).
    #[must_use]
    pub fn flash_count(&self, id: u32) -> Option<u32> {
        self.units.tracks(id)?.flash.map(|f| f.count)
    }

    /// Combat flash on piece `id`: its [`Flash`] set back, else one
    /// pushed, the unit joining the back of ORDR. **Inferred**: a building,
    /// with no movement class, whose orders the world does not run, gets
    /// none here.
    pub(in crate::sim::world) fn flash(&mut self, id: u32) {
        let now = self.now;

        if let Some(t) = self.tracks_of(id) {
            if let Some(f) = t.flash.as_mut() {
                f.hit();
            } else {
                t.flash = Some(Flash::pushed(now));

                self.dispatches.list(Listed::Unit(id));
            }
        }
    }

    /// `GE_OrderProcessor` on piece `id`: an
    /// `OrderFade` to `target` at `rate` pushed on track 1, not run on
    /// push (header info 0), its first run in the unit's next turn.
    /// **Inferred**: a building, with no movement class, whose orders the
    /// world does not run, gets none here. Not modelled: `FadeIn`'s order
    /// cleared off clears the effects too.
    pub(in crate::sim::world) fn fade_push(&mut self, id: u32, target: u8, rate: u32) {
        if let Some(t) = self.tracks_of(id) {
            t.intents.insert(
                0,
                TrackOrder {
                    intent: Intent::Fade { target, rate },
                    when: 0,
                    delta: 0,
                },
            );

            self.dispatches.list(Listed::Unit(id));
        }
    }

    /// Whether the world follows piece `id`'s orders from here, else makes
    /// its place unknown: the answer for a builtin that gives orders.
    pub(super) fn order(&mut self, id: u32) -> bool {
        let why = self.unwalked(id);

        if let Some(why) = why {
            self.lose(id, why);
        }

        why.is_none()
    }

    /// `Move(agent, unit | coord, [mode], [distance])`, gpl move:
    /// the mode strings set info bits ([`MODES`]); the
    /// distance, over 16 toward zero, is both the least and the most,
    /// or a `FollowPath`'s range;
    /// over 16 again, an avoid mode's radius ([`Avoid`]). To a unit, `GenUnit`; to a
    /// coordinate, method; each first takes a held walker out by its
    /// holder's exit (`GenUnit`, as `UnHide`). Each
    /// then stops, except a waypoint added to a running
    /// `FollowPath`. Any other second argument throws.
    pub(in crate::sim::world) fn walk_to(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let id = agent_ref(vm, args, 1)?;
        let to = vm.object(vm.arg(args, 2)?).clone();
        let info = if args.len() > 3 {
            let mode = string_ref(vm, args, 3)?;

            MODES
                .iter()
                .find(|(n, _)| n.eq_ignore_ascii_case(&mode))
                .map_or(0, |(_, bit)| *bit)
        } else {
            0
        };
        let distance = if args.len() > 4 {
            int_ref(vm, args, 4)? / crate::sim::units::PER_HALF
        } else {
            0
        };
        let unit = match self.found(id) {
            Found::Piece(_) => true,
            // The engine calls through a null unit: no message to give.
            Found::Unsure | Found::Gone => false,
        };
        let intent = match to {
            Value::Agent(target) => Intent::FollowUnit {
                target,
                max: distance,
                info,
            },
            Value::Coord(x, y) => Intent::FollowPath {
                waypoints: vec![(x, y)],
                next: 0,
                info,
                range: distance,
                callback: Callback::None,
            },
            other => Err(Fault::throw(format!(
                "Invalid second parameter to $Move(); type {:?}",
                other.kind()
            )))?,
        };

        // method and method first call method, `UnHide`'s core: a held
        // walker leaves by its holder's exit.
        let left = if unit {
            self.leave(id, None)
        } else {
            Answer::Given
        };

        let refused = if !unit {
            Some(Why::Gone)
        } else if left != Answer::Given {
            Some(Why::Held)
        } else {
            None
        };

        // `order` gives up a piece it will not walk itself.
        match refused {
            Some(why) => self.lose(id, why),
            None if self.order(id) => self.give(id, intent, info),
            None => {}
        }

        Ok(Answer::Given)
    }

    /// Stop, then push `intent`; or add its waypoint to a running
    /// `FollowPath` (`GE_OrderProcessor` with `add_waypoint`, at most
    /// 10 waypoints).
    pub(super) fn give(&mut self, id: u32, intent: Intent, info: u32) {
        let appended = match (&intent, self.units.tracks_mut(id)) {
            (Intent::FollowPath { waypoints: add, .. }, Some(t)) if info & ADD_WAYPOINT != 0 => t
                .intents
                .iter_mut()
                .find_map(|o| match &mut o.intent {
                    Intent::FollowPath { waypoints, .. } => {
                        if waypoints.len() < 10 {
                            waypoints.extend(add);
                        }

                        Some(())
                    }
                    _ => None,
                })
                .is_some(),
            _ => false,
        };

        if !appended {
            let now = self.now;

            if let Some(t) = self.tracks_of(id) {
                t.stop(now);
            }

            self.push(id, intent);
        }
    }

    /// `StopMoving(agent)`, gpl stop moving: `GenUnit`. Throws with no
    /// unit.
    pub(in crate::sim::world) fn stop_moving(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let id = agent_ref(vm, args, 1)?;

        match self.found(id) {
            Found::Gone => Err(Fault::throw(
                "StopMoving() tried to stop a non-existent game-object",
            ))?,
            Found::Unsure => Ok(Answer::Unknown),
            Found::Piece(_) => {
                let now = self.now;

                if let Some(u) = self.units.get_mut(id) {
                    u.looping = false;
                }

                if let Some(t) = self.units.tracks_mut(id) {
                    t.stop(now);
                }

                Ok(Answer::Given)
            }
        }
    }

    /// `IsAdjacent(a, b)`, gpl is adjacent: units adjacent. Throws
    /// unless both have units.
    pub(in crate::sim::world) fn is_adjacent(
        &self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let a = agent_ref(vm, args, 1)?;
        let b = agent_ref(vm, args, 2)?;
        let answer = match (self.found(a), self.found(b)) {
            (Found::Gone, _) | (_, Found::Gone) => Err(Fault::throw(
                "IsAdjacent() tried to query a non-existent game object",
            ))?,
            (Found::Piece(_), Found::Piece(_)) => self.adjacent(a, b),
            _ => None,
        };

        boolean(vm, args, answer)
    }

    /// The orders of a piece whose place is known: `Some` of its tracks'
    /// test, false with none.
    fn read_tracks(&self, id: u32, test: impl Fn(&Tracks) -> bool) -> Option<bool> {
        match self.units.motion(id) {
            Some(super::super::places::Motion::Walking) => None,
            _ => self
                .known(id)
                .map(|_| self.units.tracks(id).is_some_and(test)),
        }
    }

    /// `IsEnteringBuilding(agent)`, gpl is entering building: a `FollowPath` with
    /// callback `0x1003` on track 1.
    pub(in crate::sim::world) fn is_entering(
        &self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let id = agent_ref(vm, args, 1)?;
        let answer = match self.found(id) {
            Found::Gone => Err(Fault::throw(
                "IsEnteringBuilding() tried to query a non-existent game object",
            ))?,
            Found::Unsure => None,
            Found::Piece(_) => self.read_tracks(id, |t| {
                t.path()
                    .is_some_and(|(_, c)| matches!(c, Callback::Enter(_)))
            }),
        };

        boolean(vm, args, answer)
    }

    /// `HasWayPoints(agent)`, gpl has way points: a `FollowPath` holding more
    /// than one waypoint.
    pub(in crate::sim::world) fn has_waypoints(
        &self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let id = agent_ref(vm, args, 1)?;
        let answer = match self.found(id) {
            Found::Gone => Err(Fault::throw(
                "HasWayPoints() tried to query a non-existent game object",
            ))?,
            Found::Unsure => None,
            Found::Piece(_) => self.read_tracks(id, |t| t.path().is_some_and(|(w, _)| w.len() > 1)),
        };

        boolean(vm, args, answer)
    }

    /// `GetNextWayPoint(agent)`, gpl get next way point: the waypoint a
    /// `FollowPath` on track 1 walks to ([`Tracks::heading`]); with none,
    /// the unit's place. No unit throws nothing: the
    /// result takes two stack words never written (**Inferred**: garbage,
    /// so unknown here).
    pub(in crate::sim::world) fn next_waypoint(
        &self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let id = agent_ref(vm, args, 1)?;
        let point = match self.found(id) {
            Found::Piece(_) => match self.units.motion(id) {
                Some(super::super::places::Motion::Walking) => None,
                _ => self
                    .units
                    .tracks(id)
                    .and_then(Tracks::heading)
                    .or_else(|| self.known(id).and_then(|_| self.position(id))),
            },
            Found::Gone | Found::Unsure => None,
        };

        match point {
            Some((x, y)) => {
                let ret = vm.arg(args, 0)?;

                crate::gpl::ops::assign(vm.object_mut(ret), &Value::Coord(x, y))?;
                Ok(Answer::Given)
            }
            None => Ok(Answer::Unknown),
        }
    }

    /// `IsMoving(agent)`'s answer for a followed piece: track 0 holds an
    /// order.
    pub(in crate::sim::world) fn moving_now(&self, id: u32) -> Option<bool> {
        self.read_tracks(id, |t| t.walker.moving())
    }

    /// Whether piece `id` is moving, where known: `IsMoving`.
    #[must_use]
    pub fn is_moving(&self, id: u32) -> Option<bool> {
        match self.units.motion(id) {
            Some(_) => self.moving_now(id),
            None => self.any_piece(id).and_then(|p| p.fixed()).map(|_| false),
        }
    }

    /// `Hide`'s walk: stop, then a
    /// `FollowPath` with the building's entrance (`GenUnit`: art set
    /// `0xd0`, block 2's second point, as the exit) its one waypoint and
    /// callback `0x1003`, run on push.
    pub(in crate::sim::world) fn walk_in(&mut self, unit: u32, building: u32) {
        let entrance = self.exit(building);

        match (entrance, self.order(unit)) {
            (None, true) => self.lose(unit, Why::NoEntrance),
            (Some(at), true) => self.give(
                unit,
                Intent::FollowPath {
                    waypoints: vec![at],
                    next: 0,
                    info: 0,
                    range: 0,
                    callback: Callback::Enter(building),
                },
                0,
            ),
            (_, false) => {}
        }
    }

    /// `UnHide(agent, [coord])`, gpl un hide then
    /// `GE_OrderProcessor`: a held unit leaves its
    /// holder for the place search's cell from the holder's exit, or from
    /// the coordinate when it lies on the map; nothing for a unit not held.
    /// Throws with no unit.
    pub(in crate::sim::world) fn unhide(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let id = agent_ref(vm, args, 1)?;
        let at = if args.len() > 2 {
            match vm.object(vm.arg(args, 2)?) {
                Value::Coord(x, y) => Some((*x, *y)),
                _ => None,
            }
        } else {
            None
        };

        match self.found(id) {
            Found::Gone => Err(Fault::throw("ExitBuilding() unit is null"))?,
            Found::Unsure => Ok(Answer::Unknown),
            Found::Piece(_) => Ok(self.leave(id, at)),
        }
    }

    /// Take piece `id` out of its holder, as `UnHide` does.
    pub(in crate::sim::world) fn leave(&mut self, id: u32, at: Option<(i32, i32)>) -> Answer {
        let on_map = |(x, y): (i32, i32)| {
            self.board.holds((
                x / crate::sim::units::PER_HALF,
                y / crate::sim::units::PER_HALF,
            ))
        };
        let held = self.known(id).map(|p| p.holder != 0);
        let holder = self
            .known(id)
            .and_then(|p| self.holder(p.holder))
            .filter(|_| held == Some(true));
        let from = match (at.filter(|a| on_map(*a)), holder) {
            (Some(a), _) => Some(a),
            (None, Some(h)) => self.exit(h),
            (None, None) => None,
        };
        let body = self.body_of(id).cloned();
        let spot = from.zip(body).and_then(|(a, b)| self.place(&b, a));

        match (held, spot) {
            (Some(false), _) => Answer::Given,
            (Some(true), Some(to)) if !self.unplaced() => {
                self.put(id, to);

                // `GE_OrderProcessor`: the holder's `GSUnit`.
                if let Some(h) = holder {
                    self.restyle(h, super::super::plume::Force::Forced);
                }

                Answer::Given
            }
            _ => Answer::Unknown,
        }
    }

    /// Unit move to a held piece's leaving spot, 32nds: no holder,
    /// on the grid there.
    pub(in crate::sim::world) fn put(&mut self, id: u32, to: (i32, i32)) {
        let fixed = (
            crate::sim::units::from_pos32(to.0),
            crate::sim::units::from_pos32(to.1),
        );

        if let Some(place) = self.pieces.place_mut(id) {
            place.holder = 0;
            place.state &= !piece_state::HELD;
            place.fixed = fixed;
            place.standing = (space::half(fixed.0), space::half(fixed.1));
        }

        if let (Some(ground), Some(t)) = (self.board.ground(), self.units.tracks_mut(id)) {
            t.walker.put(ground, to);
        }

        self.jumped(id);
        if self.units.motion(id).is_none() {
            self.units
                .set_motion(id, super::super::places::Motion::Still);
        }
    }

    /// `PerformAction(agent, action, target | coord)`,
    /// gpl perform action, gpl perform action do: throws for
    /// an action no descriptor names or an agent with no unit; then
    /// `GenUnit`: `UnHide`, then
    /// `GE_OrderProcessor` with its second argument 0,
    /// so always a stop; an Action on track 1, its
    /// length [`action_ms`], run as it is pushed ([`World::act`]).
    ///
    /// An action with a projectile at a unit aims at the target's method
    /// ([`World::aim`]).
    pub(in crate::sim::world) fn perform(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let id = agent_ref(vm, args, 1)?;
        let action = string_ref(vm, args, 2)?;
        let aim = match vm.object(vm.arg(args, 3)?) {
            Value::Agent(t) if *t == id => Aim::None,
            Value::Agent(t) => Aim::Unit(*t),
            Value::Coord(x, y) => Aim::Point((*x, *y)),
            _ => Err(Fault::throw(
                "PerformAction() parameter 2 (action type string) was wrong type",
            ))?,
        };
        let folded = action.to_ascii_lowercase();

        if !self.descriptors.described(&folded) {
            Err(Fault::throw(format!("Couldn't perform action {action}")))?;
        }

        let no_unit = || {
            Fault::throw(
                "PerformAction() tried to operate on an agent that had no corresponding unit",
            )
        };
        let target = match aim {
            Aim::Unit(t) => Some(self.found(t)),
            _ => None,
        };

        match (self.found(id), target) {
            (Found::Gone, _) | (_, Some(Found::Gone)) => Err(no_unit())?,
            (Found::Unsure, _) | (_, Some(Found::Unsure)) => Ok(Answer::Unknown),
            (Found::Piece(_), _) => {
                let left = self.leave(id, None);
                let act = self.descriptors.named().actions.get(&folded).copied();
                let modifier = self
                    .any_piece(id)
                    .and_then(|p| p.get(ACTION_RATE))
                    .unwrap_or(0);
                let ms = act.map(|a| action_ms(a.rate, modifier));
                // An after-set of -1, the list's default, is 0 (**Inferred**).
                let sets = |a: Act| Sets {
                    set: a.set,
                    after: u32::try_from(a.after).unwrap_or(0),
                };
                let played = |a: Act| if self.damaged(id, a.set) { 0 } else { a.set };
                let pose = act
                    .and_then(|a| Some((self.pose(id, played(a))?, self.settle(id, a)?, sets(a))));
                let missile = act.and_then(|a| Some(a.projectile?.from(a.set)));
                let proc = act.map_or(0, |a| a.proc);
                let script = self.descriptors.named().scripts.get(&folded).cloned();
                let now = self.now;

                if proc != DYING_DONE {
                    self.fixed_tracks(id);
                }

                if let Some(u) = self.units.get_mut(id) {
                    u.looping = false;
                }

                if let Some(t) = self.tracks_of(id) {
                    t.stop(now);
                    t.pending = Some((proc, script.map(|s| (s, aim))));
                    t.pending_overlay = act.and_then(|a| a.overlay);
                    t.pending_sounding = act;
                    t.released = false;
                    t.intents.insert(
                        0,
                        TrackOrder {
                            intent: Intent::Action {
                                aim,
                                ms,
                                pose,
                                armed: false,
                                missile,
                            },
                            when: now,
                            delta: 0,
                        },
                    );

                    self.dispatches.list(Listed::Unit(id));

                    self.run_action(id);
                } else {
                    // A building's `Crumble` (`DYING_DONE`), timed apart:
                    // no tracks ([`World::fixed_tracks`] makes them for its
                    // other actions). Its begin phase as asked
                    // (**Inferred**: pushed at once, nothing ahead of it).
                    if let Some((a, phase)) = act.and_then(|a| Some((a, a.begin?))) {
                        self.act_cue(id, a.sound, phase);
                    }

                    // Its fall alone is followed.
                    if proc == DYING_DONE {
                        let pose = act.and_then(|a| self.pose(id, a.set));

                        self.fall(id, pose);
                    }
                }

                Ok(left)
            }
        }
    }

    /// `AgentNumber(n)`, gpl agent number: the agent
    /// reference for id `n`, alive or not.
    #[allow(
        clippy::unused_self,
        reason = "a checked builtin, called as the others are"
    )]
    pub(in crate::sim::world) fn agent_number(
        &self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<(), Fault> {
        let n = int_ref(vm, args, 1)?;
        let ret = vm.arg(args, 0)?;

        ops::assign(vm.object_mut(ret), &Value::Agent(n.cast_unsigned())).map_err(Fault::from)
    }

    /// `IsMoving(agent)`, gpl is moving: `GenUnit` method `(0)`, an order in
    /// slot 0 not yet done. Throws with no unit. Known for a building
    /// (**Inferred**: no orders) and a piece the world follows.
    pub(in crate::sim::world) fn moving(
        &self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let id = agent_ref(vm, args, 1)?;
        let moving = match self.found(id) {
            Found::Piece(_) => self.is_moving(id),
            Found::Unsure => None,
            Found::Gone => Err(Fault::throw(
                "IsMoving() tried to query a non-existent game object",
            ))?,
        };

        match moving {
            Some(b) => {
                let ret = vm.arg(args, 0)?;

                put_bool(vm, ret, b)?;
                Ok(Answer::Given)
            }
            None => Ok(Answer::Unknown),
        }
    }
}
