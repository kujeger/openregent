use super::*;

#[test]
fn a_straight_step_is_six_thirty_seconds_exactly() {
    let at = [from_whole(13), from_whole(116), Fixed::ZERO];
    let to = [from_whole(13), from_whole(115), Fixed::ZERO];
    let (next, leg) = advance(at, to, from_pos32(6), Finish::Overshoot);

    assert_eq!(pos32(next), (416, 3706));
    assert_eq!(leg, Leg::More);
}

#[test]
fn an_overshooting_step_goes_a_full_step_past_its_node() {
    let at = [from_whole(13), from_whole(116) + from_pos32(2), Fixed::ZERO];
    let to = [from_whole(13), from_whole(116), Fixed::ZERO];
    let step = from_pos32(6);

    assert_eq!(
        advance(at, to, step, Finish::Overshoot),
        (
            [from_whole(13), from_whole(116) - from_pos32(4), Fixed::ZERO],
            Leg::Done
        )
    );
    assert_eq!(advance(at, to, step, Finish::Exact), (to, Leg::Done));
}

/// A homing missile whose target moved far off, as a dragon's
/// fireball at a ranger camp moved mid-flight: 120² + 160² passes
/// 16.16's 32767, and the engine's sum wraps into an unsigned root.
#[test]
fn a_step_toward_a_goal_past_181_tiles_wraps_as_the_engine() {
    let at = [Fixed::ZERO; 3];
    let to = [from_whole(120), from_whole(160), Fixed::ZERO];
    let (next, leg) = advance(at, to, from_whole(5), Finish::Exact);

    assert_eq!(next, [from_whole(3), from_whole(4), Fixed::ZERO]);
    assert_eq!(leg, Leg::More);
}

#[test]
fn every_direction_of_the_compass_is_its_own() {
    let compass = [
        ((0, -1), 0),
        ((1, -1), 4),
        ((1, 0), 8),
        ((1, 1), 12),
        ((0, 1), 16),
        ((-1, 1), 20),
        ((-1, 0), 24),
        ((-1, -1), 28),
    ];

    for ((dx, dy), d) in compass {
        assert_eq!(direction(dx, dy), d, "{dx},{dy}");
    }
}

/// Level ground, 64 vertices a side.
struct Flat;

impl Heights for Flat {
    fn across(&self) -> u32 {
        64
    }

    fn height(&self, _: u32, _: u32) -> Option<Fixed> {
        Some(Fixed::ZERO)
    }
}

const CLASS_1: Class = Class {
    turn_rate: 50,
    turn_step: 4,
    move_rate: 90,
    move_dist: 6,
    size: 0,
    lift: (0, 0),
    flags: 0,
};

/// `DMOV ML04`, Large Flyer: 15 ms and 8 32nds a step, so a frame's
/// step is 26.67 32nds, over the cap.
const LARGE_FLYER: Class = Class {
    turn_rate: 50,
    turn_step: 4,
    move_rate: 15,
    move_dist: 8,
    size: 8,
    lift: (180, 60),
    flags: 0x31,
};

/// A path east with a one-node jog south and back: a class with flag
/// `0x20` keeps its facing through it, the Large Flyer, and never
/// turns; Class 1 turns at the jog.
#[test]
fn flag_0x20_keeps_the_facing_through_a_one_node_jog() {
    let facings = |class: &Class| {
        let mut walker = Walker::new(&Flat, (0, 160), 8, class, 0);
        let mut plans = 0;
        let mut planner = |at: (i32, i32), _: usize| {
            plans += 1;

            if plans == 1 {
                vec![
                    at,
                    (at.0 + 1, at.1),
                    (at.0 + 2, at.1 + 1),
                    (at.0 + 3, at.1 + 1),
                ]
            } else {
                vec![at]
            }
        };
        let mut seen = vec![];

        for frame in 1..40 {
            walker.frame(&Flat, frame * MS_PER_FRAME, &mut planner);
            seen.push(walker.facing);
        }

        seen.dedup();
        seen
    };

    assert_eq!(facings(&LARGE_FLYER), [8]);
    assert!(facings(&CLASS_1).len() > 1, "{:?}", facings(&CLASS_1));
}

/// Ground rising one cell a cell eastward.
struct Slope;

impl Heights for Slope {
    fn across(&self) -> u32 {
        64
    }

    fn height(&self, x: u32, _: u32) -> Option<Fixed> {
        Some(from_whole(i32::try_from(x).unwrap_or_default()))
    }
}

/// A set that keeps the frame: an `InitAction` whose
/// set shares a frame-keeping bit with the set the parts play starts on
/// the frame they show at its push: pushed at 100, from frame 5 of 9 at
/// 50 ms, it reaches its last frame two runs after the push and ends a
/// run later; one that shares none starts on frame 0. The parts'
/// looping set steps a frame a rate from its push (**Inferred**: no
/// step as it is pushed).
#[test]
fn a_frame_keeping_set_starts_on_the_frame_the_parts_show() {
    const KEEP: u32 = 1 << 17;
    let pose = |keeps| action::Pose {
        frames: vec![9; 8],
        playback: 50,
        keeps,
        cues: vec![None; 8],
    };
    let peaks = |keeps| {
        let mut walker = Walker::standing(&Flat, (0, 0), 8, &CLASS_1);

        walker.phase = action::Phase::running(3, 0, 50, 9, KEEP);
        assert_eq!(walker.phase.frame_at(100), 5);

        let mut at = vec![];

        if walker.act(None, pose(keeps), action::Settle::Keep, 100) == action::Peak::Reached {
            at.push(100);
        }

        for t in (150..1000).step_by(50) {
            if walker.run_stage(t) == action::Peak::Reached {
                at.push(t);
            }

            if !walker.acting() {
                return (at, t);
            }
        }

        (at, 0)
    };

    assert_eq!(peaks(KEEP), (vec![200], 250));
    assert_eq!(peaks(0), (vec![450], 500));
}

/// An `InitAction` that keeps the frame and starts on its last fires
/// at its first run, which ends it, as one a frame earlier does: the
/// engine steps past the last frame out of loops.
/// **Game**, the united winter game: dragon 28's fifth breath, started
/// on frame 8, hits nine frames after `PerformAction`, as one on 7.
#[test]
fn an_init_action_started_on_its_last_frame_still_fires() {
    const KEEP: u32 = 1 << 17;
    let pose = action::Pose {
        frames: vec![9; 8],
        playback: 50,
        keeps: KEEP,
        cues: vec![None; 8],
    };
    let fires = |start| {
        let mut walker = Walker::standing(&Flat, (0, 0), 8, &CLASS_1);

        walker.phase = action::Phase::still(start, KEEP);

        let peak = walker.act(None, pose.clone(), action::Settle::Keep, 100);

        (peak, walker.acting())
    };

    assert_eq!(fires(7), (action::Peak::Reached, true));
    assert_eq!(fires(8), (action::Peak::Reached, false));
}

/// An `InitAction`'s after-set plays from the frame the parts show
/// (`GenUnit`: unit play set with frame -1), its last,
/// where the action's set and `Stand` share a frame-keeping bit;
/// else from frame 0. **Game**, the united winter
/// game: dragon 28's walk after a breath, and its stop before the next.
#[test]
fn a_frame_keeping_after_set_plays_on_from_the_last_frame() {
    const KEEP: u32 = 1 << 17;
    let pose = |keeps| action::Pose {
        frames: vec![9; 8],
        playback: 50,
        keeps,
        cues: vec![None; 8],
    };
    let after = |keeps| {
        let mut walker = Walker::standing(&Flat, (0, 0), 8, &CLASS_1);

        walker.loops(Some(pose(KEEP)), Some(pose(KEEP)));
        walker.plays(action::Sets {
            set: 0x10,
            after: STAND,
        });
        walker.act(None, pose(keeps), action::Settle::Keep, 0);

        let end = (50..1000)
            .step_by(50)
            .find(|&t| {
                walker.run_stage(t);
                !walker.acting()
            })
            .unwrap_or_default();

        walker.phase.frame_at(end)
    };

    assert_eq!(after(KEEP), 8);
    assert_eq!(after(0), 0);
}

/// An `InitAction` is pushed as `PerformAction` runs and runs on its
/// push ([`action::Stage::run`]), so a set that keeps the frame shows
/// the one after the frame the parts show then. **Game**, the united
/// winter game: both ice dragons' breaths, 23 and 17 in a row at one
/// spot, start on the frame this gives, `Stand` playing on from the
/// last breath's last frame.
#[test]
fn an_init_action_keeps_the_frame_shown_at_its_push() {
    const KEEP: u32 = 1 << 17;
    let pose = action::Pose {
        frames: vec![9; 8],
        playback: 50,
        keeps: KEEP,
        cues: vec![None; 8],
    };
    let mut walker = Walker::standing(&Flat, (0, 0), 8, &CLASS_1);

    // Looping from frame 0 since 0: frame 3 at 150.
    walker.phase = action::Phase::running(0, 0, 50, 9, KEEP);
    walker.act(None, pose, action::Settle::Keep, 150);

    assert_eq!(
        walker.stage.as_ref().and_then(action::Stage::frame),
        Some(4)
    );
}

/// An action's turn of more than one run makes a frameless class's
/// parts loop their own set from frame 0 through it,
/// so a frame-keeping `InitAction`
/// after it starts on the frame that loop shows; a one-run turn, or a
/// class with walk frames, keeps the held frame. Held on frame 5, a
/// three-run turn from 300 pushes the `InitAction` at 400 on frame 2,
/// which shows 3. **Game**, the united winter game: ice dragon 28's
/// breath at peasant 41 after a turn of 12 directions hits after
/// minotaur 18's blow at 140,600 ms, as the log has it, not at
/// 140,550; with it the game plays its whole log.
#[test]
fn a_frameless_turn_of_several_runs_restarts_the_parts_set() {
    const KEEP: u32 = 1 << 17;
    let pose = action::Pose {
        frames: vec![9; 8],
        playback: 50,
        keeps: KEEP,
        cues: vec![None; 8],
    };
    let shown = |flags, want| {
        let class = Class { flags, ..CLASS_1 };
        let mut walker = Walker::standing(&Flat, (0, 0), 8, &class);

        walker.loops(Some(pose.clone()), Some(pose.clone()));
        walker.play(&pose, 0, 0, Parts::Walk);
        walker.hold_frame(275);
        walker.act(Some(want), pose.clone(), action::Settle::Keep, 300);

        for t in [350, 400] {
            walker.run_stage(t);
        }

        walker.stage.as_ref().and_then(action::Stage::frame)
    };

    assert_eq!(shown(FRAMELESS, 20), Some(3));
    assert_eq!(shown(FRAMELESS, 12), Some(8));
    assert_eq!(shown(0, 20), Some(6));
}

/// A frameless class's path plays `Walk` on track 1 as it is pushed
/// (`GE_OrderProcessor`, through
/// unit play set): a looping `Animate`, which a stop takes off.
/// A class with walk frames plays none.
#[test]
fn a_frameless_paths_walk_is_a_track_one_animate() {
    let pose = action::Pose {
        frames: vec![9; 8],
        playback: 50,
        keeps: 1 << 17,
        cues: vec![None; 8],
    };
    let played = |flags| {
        let class = Class { flags, ..CLASS_1 };
        let mut walker = Walker::new(&Flat, (0, 0), 8, &class, 0);

        walker.loops(Some(pose.clone()), Some(pose.clone()));
        walker.push_path(100);
        walker.take_played()
    };

    assert_eq!(played(FRAMELESS), [Track1::Plays(WALK)]);
    assert_eq!(played(0), []);
}

/// A step moves part 0's frame on by one unless its `Move`'s info has
/// `0x80`. A path's `Move`
/// carries the path's info, so a frameless class's leaves the frame;
/// the `Move` after a `Turn` has info 0,
/// so even a frameless class's steps it, wrapping
/// at `Walk`'s frames. **Game**, the united winter game: dragon 28's
/// turn after its stop puts the breath on the engine's frame.
#[test]
fn a_move_after_a_turn_steps_the_frame_even_when_frameless() {
    const KEEP: u32 = 1 << 17;
    let pose = action::Pose {
        frames: vec![9; 8],
        playback: 50,
        keeps: KEEP,
        cues: vec![None; 8],
    };
    let frame = |finish, start| {
        let class = Class {
            flags: FRAMELESS,
            ..CLASS_1
        };
        let mut walker = Walker::standing(&Flat, (0, 0), 8, &class);

        walker.loops(Some(pose.clone()), Some(pose.clone()));
        walker.phase = action::Phase::still(start, KEEP);
        walker.run_top(
            Top::Move {
                to: [Fixed::ZERO, from_whole(1), Fixed::ZERO],
                finish,
            },
            100,
        );
        walker.phase.frame_at(1000)
    };

    assert_eq!(frame(Finish::Exact, 2), 3);
    assert_eq!(frame(Finish::Exact, 8), 0);
    assert_eq!(frame(Finish::Overshoot, 2), 2);
}

/// A flier flies at its least height, or its lift above the ground
/// where that is higher: over
/// low ground its steps are level, a full 15/32 each; a walker's climb
/// a slope and come up short.
#[test]
fn a_flier_steps_level_at_its_least_height() {
    let steps = |class: &Class| {
        let mut walker = Walker::new(&Slope, (0, 160), 8, class, 0);
        let mut planner =
            |at: (i32, i32), _: usize| (0..8).map(|n| (at.0 + n, at.1)).collect::<Vec<_>>();
        let mut xs = vec![];

        for frame in 1..12 {
            walker.frame(&Slope, frame * MS_PER_FRAME, &mut planner);
            xs.push(pos32(walker.at).0);
        }

        xs.windows(2)
            .map(|w| w[1] - w[0])
            .filter(|d| *d != 0)
            .collect::<Vec<_>>()
    };

    assert!(
        steps(&LARGE_FLYER)[..7].iter().all(|d| *d == 15),
        "{:?}",
        steps(&LARGE_FLYER)
    );
    assert!(
        steps(&CLASS_1).iter().all(|d| *d < 12),
        "{:?}",
        steps(&CLASS_1)
    );
}

/// A flier stands at its flight height, so its unit pos is
/// aloft; a walker on the ground has none of its own.
#[test]
fn a_flier_is_placed_at_its_flight_height() {
    let flier = Walker::new(&Slope, (0, 160), 8, &LARGE_FLYER, 0);
    let walker = Walker::new(&Slope, (0, 160), 8, &CLASS_1, 0);
    let (least, _) = LARGE_FLYER.flight().expect("a flier");

    assert_eq!(flier.aloft(), Some(flier.at[2]));
    assert!(flier.at[2] >= least);
    assert_eq!(walker.aloft(), None);
}

/// Over ground higher than its least height less its lift, a flier
/// flies its lift above the ground, climbing with it: its steps on the
/// slope come up short of 15/32.
#[test]
fn a_flier_flies_its_lift_above_high_ground() {
    let mut walker = Walker::new(&Slope, (160, 160), 8, &LARGE_FLYER, 0);
    let mut planner =
        |at: (i32, i32), _: usize| (0..8).map(|n| (at.0 + n, at.1)).collect::<Vec<_>>();
    let mut xs = vec![];

    for frame in 1..10 {
        walker.frame(&Slope, frame * MS_PER_FRAME, &mut planner);
        xs.push(pos32(walker.at).0);
    }

    let (least, lift) = LARGE_FLYER.flight().expect("a flier");

    assert_eq!(
        walker.at[2],
        ground(&Slope, walker.at[0], walker.at[1]).max(Fixed::ZERO) + lift
    );
    assert!(walker.at[2] > least);
    assert!(xs.windows(2).all(|w| w[1] - w[0] < 15), "{xs:?}");
}

/// Raised ground, 64 vertices a side, every one a cell high.
struct Raised;

impl Heights for Raised {
    fn across(&self) -> u32 {
        64
    }

    fn height(&self, _: u32, _: u32) -> Option<Fixed> {
        Some(ONE)
    }
}

/// Ground height compares a point's cell plus one with the map's
/// width and height in cells, not its vertices:
/// the last cell row and column read 0. **Game**: play3's ranger 31
/// steps short of its node in row 127, (1343, 4063) for (1344, 4064).
#[test]
fn the_last_cell_row_and_column_have_no_ground() {
    let half = ONE / 2;
    let at = |cell: i32| from_whole(cell) + half;

    assert_eq!(ground(&Raised, at(10), at(61)), ONE);
    assert_eq!(ground(&Raised, at(61), at(10)), ONE);
    assert_eq!(ground(&Raised, at(10), at(62)), Fixed::ZERO);
    assert_eq!(ground(&Raised, at(62), at(10)), Fixed::ZERO);
}

/// Move step length caps a step only for a `Move` with info `0x4000`: the
/// `Move` a turn ends in (info 0) takes the full step, so a fast
/// walker lands on its node at once (**Binary**). Class 1's step is
/// under the cap either way.
#[test]
fn only_an_overshooting_step_is_capped() {
    let flyer = Walker::new(&Flat, (0, 0), 0, &LARGE_FLYER, 0);
    let walker = Walker::new(&Flat, (0, 0), 0, &CLASS_1, 0);

    assert_eq!(flyer.step_of(Finish::Exact), Fixed::from_bits(0xd555));
    assert_eq!(flyer.step_of(Finish::Overshoot), STEP_CAP);
    assert_eq!(walker.step_of(Finish::Exact), from_pos32(6));
    assert_eq!(walker.step_of(Finish::Overshoot), from_pos32(6));
}

/// A walk frame a step: class 1's 90 ms rounds to 100; a rate under a
/// frame steps once a frame.
#[test]
fn a_walk_frame_lasts_a_step() {
    let fast = Class {
        move_rate: 30,
        ..CLASS_1
    };

    assert_eq!(
        Walker::new(&Flat, (0, 0), 8, &CLASS_1, 0).stride_ms(),
        Some(100)
    );
    assert_eq!(
        Walker::new(&Flat, (0, 0), 8, &fast, 0).stride_ms(),
        Some(50)
    );

    let frameless = Class {
        flags: FRAMELESS,
        ..CLASS_1
    };

    assert_eq!(
        Walker::new(&Flat, (0, 0), 8, &frameless, 0).stride_ms(),
        None
    );
}

/// The kept length doubles when a plan finds more than
/// [`EXPAND_COUNT`] before it: the count goes up first, then to 0 on
/// doubling. So 5 plans keep 10, 6 keep 20.
#[test]
fn the_kept_length_doubles_after_five_plans_then_six() {
    let mut keeps = vec![];
    let mut walker = Walker::new(&Flat, (0, 0), 8, &CLASS_1, 0);
    let mut planner = |at: (i32, i32), keep: usize| {
        keeps.push(keep);
        vec![at, (at.0 + 1, at.1)]
    };

    for frame in 1..200 {
        walker.frame(&Flat, frame * MS_PER_FRAME, &mut planner);
    }

    let runs: Vec<(usize, usize)> = keeps
        .chunk_by(|a, b| a == b)
        .map(|c| (c[0], c.len()))
        .take(3)
        .collect();

    assert_eq!(runs, [(10, 5), (20, 6), (40, 6)]);
}

/// A `MovePath` spent on the map plays `Stand` on track 1, a looping
/// `Animate`; an `InitAction`'s end drops track 1's
/// `Animate`s, then plays its after-set, if any, the same way.
#[test]
fn a_walks_end_and_an_after_set_play_on_track_one() {
    let mut walker = Walker::new(&Flat, (0, 0), 8, &CLASS_1, 0);
    let mut plans = 0;
    let mut planner = |at: (i32, i32), _: usize| {
        plans += 1;

        if plans == 1 {
            vec![at, (at.0 + 1, at.1)]
        } else {
            vec![at]
        }
    };
    let mut played = vec![];

    for frame in 1..40 {
        walker.frame(&Flat, frame * MS_PER_FRAME, &mut planner);
        played.extend(walker.take_played());
    }

    assert_eq!(played, [Track1::PathEnd]);

    let pose = action::Pose {
        frames: vec![3; 8],
        ..action::Pose::default()
    };
    let mut played = vec![];

    for after in [0, STAND] {
        walker.plays(action::Sets { set: 0x10, after });
        walker.act(None, pose.clone(), action::Settle::Keep, 0);

        for frame in 1..20 {
            walker.run_stage(frame * MS_PER_FRAME);
        }

        played.extend(walker.take_played());
    }

    assert_eq!(played, [Track1::Clear, Track1::Plays(STAND)]);
}

/// A one-frame `Stand` loops nothing (unit play set pushes no
/// `Animate` for a one-frame slot), so the next walk's steps move the
/// `Walk` frame on. **Game**: a city guard's `Stand` is one frame; its
/// walk after a spent path slid on frame 0 (QU12, guard 65 from
/// 97,050 ms).
#[test]
fn a_walk_after_a_one_frame_stand_steps_its_frames() {
    let walk = action::Pose {
        frames: vec![8; 8],
        playback: 100,
        ..action::Pose::default()
    };
    let stand = action::Pose {
        frames: vec![1; 8],
        playback: 100,
        ..action::Pose::default()
    };
    let mut walker = Walker::new(&Flat, (0, 0), 8, &CLASS_1, 0);
    let mut plans = 0;
    let mut planner = |at: (i32, i32), _: usize| {
        plans += 1;

        match plans {
            1 => vec![at, (at.0 + 1, at.1)],
            2 => vec![at],
            _ => (0..6).map(|i| (at.0 + i, at.1)).collect(),
        }
    };

    walker.loops(Some(walk), Some(stand));

    // One step, then the path is spent: `Stand`.
    for frame in 1..40 {
        walker.frame(&Flat, frame * MS_PER_FRAME, &mut planner);
    }

    assert_eq!(walker.take_played(), [Track1::PathEnd]);

    // A new path's steps.
    walker.push_path(40 * MS_PER_FRAME);

    let shown: std::collections::BTreeSet<u32> = (41..80)
        .filter_map(|frame| {
            let now = frame * MS_PER_FRAME;

            walker.frame(&Flat, now, &mut planner);
            walker.walk_frame(now)
        })
        .collect();

    assert!(shown.len() > 1, "walk frames shown: {shown:?}");
}

/// A straight walk east from (0, 0), every frame from `from` to `to`:
/// the `Walk` frame shown as each step lands, `None` without a step.
fn strides(walker: &mut Walker, around: &mut dyn Around, from: u32, to: u32) -> Vec<u32> {
    (from..to)
        .filter_map(|frame| {
            let now = frame * MS_PER_FRAME;
            let at = walker.at;

            walker.among(&Flat, now, around);
            (walker.at != at).then(|| walker.phase.frame_at(now))
        })
        .collect()
}

/// A walk east along the x axis, nothing in its way.
fn east(at: (i32, i32), _: usize) -> Vec<(i32, i32)> {
    (0..8).map(|i| (at.0 + i, at.1)).collect()
}

/// Art of `walk` frames, and a `Stand` of `stand` at `rate` ms.
fn walk_and_stand(walk: u16, stand: u16, rate: u32) -> (action::Pose, action::Pose) {
    let pose = |frames, playback| action::Pose {
        frames: vec![frames; 8],
        playback,
        ..action::Pose::default()
    };

    (pose(walk, 100), pose(stand, rate))
}

/// A step from another set puts `Walk` on the parts first:
/// frame 0, as the sets
/// share no frame-keeping bit, then one on. After an action with no
/// after-set, held on its last frame, 2, the first step shows 1, not 3.
#[test]
fn a_step_after_an_action_starts_walk_on_frame_zero() {
    let mut walker = Walker::standing(&Flat, (0, 0), 8, &CLASS_1);
    let (walk, stand) = walk_and_stand(8, 1, 100);
    let pose = action::Pose {
        frames: vec![3; 8],
        playback: 100,
        ..action::Pose::default()
    };

    walker.loops(Some(walk), Some(stand));
    walker.plays(action::Sets {
        set: 0x10,
        after: 0,
    });
    walker.act(None, pose, action::Settle::Keep, 0);

    for frame in 1..10 {
        walker.run_stage(frame * MS_PER_FRAME);
    }

    assert!(!walker.acting());
    assert_eq!(walker.phase.frame_at(10 * MS_PER_FRAME), 2);

    walker.push_path(10 * MS_PER_FRAME);

    let shown = strides(&mut walker, &mut Alone(&mut east), 11, 20);

    assert_eq!(shown[..3], [1, 2, 3]);
}

/// A walker that waits shows `Stand` (`UnitIMG` method `(8)`, `OrderMovePath`),
/// so its next step starts `Walk` over: frame 1, not the one after
/// the frame it stopped on.
#[test]
fn a_step_after_a_wait_starts_walk_on_frame_zero() {
    /// In the way of the second node until the walker has asked
    /// `left` times.
    struct Jam {
        left: u32,
    }

    impl Around for Jam {
        fn plan(&mut self, from: Cell, keep: usize, _: Blocking) -> Vec<Cell> {
            east(from, keep)
        }

        fn at_goal(&mut self, _: Cell) -> bool {
            true
        }

        fn fits(&mut self, at: Cell, how: Blocking) -> bool {
            let jammed = at == (2, 0) && how == Blocking::Any && self.left > 0;

            self.left = self.left.saturating_sub(u32::from(jammed));
            !jammed
        }

        fn wait(&mut self, _: Cell) -> bool {
            true
        }

        fn nudge(&mut self, _: &[Cell]) {}
    }

    let mut walker = Walker::standing(&Flat, (0, 0), 8, &CLASS_1);
    let (walk, stand) = walk_and_stand(8, 1, 100);

    walker.loops(Some(walk), Some(stand));
    walker.push_path(0);

    let shown = strides(&mut walker, &mut Jam { left: 1 }, 1, 40);

    // Three steps to the first node, the wait, then on from 1.
    assert_eq!(shown, [1, 2, 3, 1, 2, 3, 4, 5, 6]);
}

/// Art without `Walk` or `Stand`: `UnitIMG` fails, so the parts
/// keep the set they show. A step still moves part 0's frame on,
/// wrapping at that set's frames; a wait
/// changes nothing. 53 HD bodies with a gait, the
/// Strangleweed among them.
#[test]
fn a_step_without_walk_steps_the_set_shown() {
    let mut walker = Walker::standing(&Flat, (0, 0), 8, &CLASS_1);
    let own = action::Pose {
        frames: vec![4; 8],
        playback: 100,
        ..action::Pose::default()
    };

    walker.loops(None, None);
    walker.shown = Some(Box::new(own));
    walker.push_path(0);

    let shown = strides(&mut walker, &mut Alone(&mut east), 1, 30);

    assert_eq!(shown[..6], [1, 2, 3, 0, 1, 2]);
    assert_eq!(walker.parts, Parts::Other);

    walker.stand(30 * MS_PER_FRAME);

    assert_eq!(
        walker.phase.frame_at(30 * MS_PER_FRAME),
        shown[shown.len() - 1]
    );
    assert_eq!(walker.parts, Parts::Other);
}

/// No order clears a looping `Stand` as a new path starts:
/// `GE_OrderProcessor` pushes the `MovePath` only, so a path
/// end's or an after-set's `Animate` runs on into the walk, stepping
/// part 0 on at `Stand`'s rate, wrapping at the count of the set the
/// part shows. The step puts `Walk` on from
/// frame 0 first, and runs before track 1. Stand
/// of 4 at 150 ms from 0; the path's first step at 150 shows 1, then
/// that run, 2. **Inferred**: only the Dryad, its `Walk` of 4, could
/// show it in HD, and only after an after-set's `Stand`.
#[test]
fn a_looping_stand_runs_on_into_the_next_walk_from_frame_zero() {
    let mut walker = Walker::standing(&Flat, (0, 0), 8, &CLASS_1);
    let (walk, stand) = walk_and_stand(8, 4, 150);

    walker.loops(Some(walk), Some(stand.clone()));
    walker.play(&stand, 0, 0, Parts::Stand);
    walker.push_path(100);

    // Its first step lands at 150 (frame 3).
    let shown: Vec<u32> = (3..12)
        .map(|frame| {
            let now = frame * MS_PER_FRAME;

            walker.among(&Flat, now, &mut Alone(&mut east));
            walker.phase.frame_at(now)
        })
        .collect();

    // Steps at 150, 250, 350...; `Stand`'s runs at 300, 450.
    assert_eq!(shown, [2, 2, 3, 4, 5, 5, 7, 7, 0]);
}

/// One step east, then the path is spent; later plans walk on east.
fn one_step_then_east() -> impl FnMut((i32, i32), usize) -> Vec<(i32, i32)> {
    let mut plans = 0;

    move |at, keep| {
        plans += 1;

        match plans {
            1 => vec![at, (at.0 + 1, at.1)],
            2 => vec![at],
            _ => east(at, keep),
        }
    }
}

/// A walk's end shows `Stand`, as a wait does: its own orders last
/// played it.
#[test]
fn a_walks_end_stands() {
    let mut walker = Walker::new(&Flat, (0, 0), 8, &CLASS_1, 0);
    let mut planner = one_step_then_east();
    let (walk, stand) = walk_and_stand(8, 4, 150);

    walker.loops(Some(walk), Some(stand));

    for frame in 1..40 {
        walker.frame(&Flat, frame * MS_PER_FRAME, &mut planner);
    }

    assert_eq!(walker.take_played(), [Track1::PathEnd]);
    assert_eq!(walker.gait(), Gait::Stand);
}

/// Where the art has set `0xb1`, a walk's end puts it on the parts
/// and pushes an `Animate` on track 0 from frame 0,
/// flags `0x150`, no loops left: first run the
/// next frame, then every 150 ms, a frame a run; the run past the last
/// frame ends it, which empties track 0. The last frame holds, and the
/// next walk puts `Walk` on from frame 0. The Dryad's sets.
#[test]
fn a_walks_end_plays_set_0xb1_once_on_track_zero() {
    let mut walker = Walker::new(&Flat, (0, 0), 8, &CLASS_1, 0);
    let mut planner = one_step_then_east();
    let (walk, stand) = walk_and_stand(4, 4, 150);

    walker.loops(Some(walk), Some(stand.clone()));
    walker.ends(stand);

    let end = (1..40)
        .find(|frame| {
            walker.frame(&Flat, frame * MS_PER_FRAME, &mut planner);
            !walker.take_played().is_empty()
        })
        .expect("the walk's end");
    let shown: Vec<(bool, Option<u32>)> = (end..end + 12)
        .map(|frame| {
            if frame > end {
                walker.frame(&Flat, frame * MS_PER_FRAME, &mut planner);
            }

            let look = match walker.look() {
                Some(action::Look::Frame {
                    set: PATH_END,
                    frame,
                }) => Some(frame),
                _ => None,
            };

            (walker.moving(), look)
        })
        .collect();

    assert_eq!(
        shown,
        [
            (true, Some(0)),
            (true, Some(1)),
            (true, Some(1)),
            (true, Some(1)),
            (true, Some(2)),
            (true, Some(2)),
            (true, Some(2)),
            (true, Some(3)),
            (true, Some(3)),
            (true, Some(3)),
            (false, Some(3)),
            (false, Some(3)),
        ]
    );

    walker.push_path((end + 12) * MS_PER_FRAME);

    let strides = strides(&mut walker, &mut Alone(&mut planner), end + 13, end + 30);

    assert_eq!(strides[..3], [1, 2, 3]);
}

/// `MovementRateModifier` takes with the next `Move`: each is made with
/// `GSUnit`'s rate then. Class 1 steps every 100 ms; plagued, +100,
/// every 200 ms from the move after.
#[test]
fn a_rate_modifier_takes_at_the_next_move() {
    let mut walker = Walker::new(&Flat, (0, 0), 8, &CLASS_1, 0);
    let mut planner = |at: (i32, i32), _: usize| (0..40).map(|n| (at.0 + 8 * n, at.1)).collect();
    let mut steps = vec![];
    let mut last = walker.at;

    for frame in 1..80 {
        if frame == 40 {
            walker.modify_rate(100);
        }

        walker.frame(&Flat, frame * MS_PER_FRAME, &mut planner);

        if walker.at != last {
            steps.push(frame);
            last = walker.at;
        }
    }

    let gaps: Vec<u32> = steps.windows(2).map(|w| w[1] - w[0]).collect();
    let (before, after) = gaps.split_at(gaps.len() - 5);

    assert!(before[1..10].iter().all(|g| *g == 2), "{gaps:?}");
    assert!(after.iter().all(|g| *g == 4), "{gaps:?}");
}

/// A walker waits for a moving unit no faster a cell than itself, or
/// for nobody; not for one standing, a faster one, or in a ring.
#[test]
fn a_walker_waits_for_a_moving_blocker_outside_a_ring() {
    let unit = |wants: Option<Cell>, pace: i32, moving: bool| Waiter {
        wants,
        pace: Fixed::from_bits(pace),
        moving,
    };
    let (a, b) = ((1, 0), (0, 0));
    // Unit 0 stands on b, unit 1 on a.
    let on = |_: usize, c: Cell| match c {
        (0, 0) => Some(0),
        (1, 0) => Some(1),
        _ => None,
    };

    assert!(wait_for(
        1,
        &[unit(None, 100, true), unit(Some(b), 100, false)],
        &on
    ));
    assert!(!wait_for(
        1,
        &[unit(None, 100, false), unit(Some(b), 100, false)],
        &on
    ));
    assert!(!wait_for(
        1,
        &[unit(None, 50, true), unit(Some(b), 100, false)],
        &on
    ));
    assert!(!wait_for(
        1,
        &[unit(Some(a), 100, true), unit(Some(b), 100, true)],
        &on
    ));
    assert!(wait_for(0, &[unit(Some((5, 5)), 100, true)], &on));
}

/// A new path keeps the wait record: only `OrderMovePath` clears
/// it, as it steps or its path ends; `GE_OrderProcessor`, a stop
/// and a path's removal leave it.
#[test]
fn a_new_path_keeps_the_cell_its_walker_wanted() {
    let mut walker = Walker::standing(&Flat, (0, 0), 0, &CLASS_1);

    walker.wants = Some((3, 3));
    walker.push_path(0);

    assert_eq!(walker.wants(), Some((3, 3)));
}

/// Track 2 frame by frame from an Action arming in frame 0: `busy`
/// in each frame after tracks 0 to 2 ran, with the facing.
fn stage(want: Option<i32>, facing: i32, pose: action::Pose) -> Vec<(bool, i32)> {
    let mut walker = Walker::standing(&Flat, (0, 0), facing, &CLASS_1);

    walker.act(want, pose, action::Settle::Keep, 0);

    let mut frames = vec![(walker.acting(), walker.facing)];

    for frame in 1..12 {
        walker.run_stage(frame * MS_PER_FRAME);
        frames.push((walker.acting(), walker.facing));
    }

    frames
}

/// A peasant's `Basic_build` set, `Carry-2` of `AVj1`: four frames at
/// 100 ms. Already facing, the `InitAction` runs as it is pushed and
/// every 100 ms, and the run past frame 3 ends it: 300 ms.
#[test]
fn an_init_action_holds_track_two_its_frames_less_one_times_its_rate() {
    let carry = action::Pose {
        frames: vec![4; 8],
        playback: 100,
        keeps: 0,
        cues: vec![],
    };
    let busy: Vec<bool> = stage(Some(8), 8, carry).iter().map(|f| f.0).collect();

    assert_eq!(
        busy,
        [
            true, true, true, true, true, true, false, false, false, false, false, false
        ]
    );
}

/// A turn of 16 at class 1's 4 a frame runs in frames 0 to 3; its end
/// pushes the `InitAction`, run at once, so 300 ms from frame 3.
#[test]
fn an_actions_turn_runs_before_its_init_action_on_track_two() {
    let carry = action::Pose {
        frames: vec![4; 8],
        playback: 100,
        keeps: 0,
        cues: vec![],
    };
    let frames = stage(Some(16), 0, carry);
    let facings: Vec<i32> = frames.iter().map(|f| f.1).take(4).collect();
    let busy: Vec<bool> = frames.iter().map(|f| f.0).collect();

    assert_eq!(facings, [4, 8, 12, 16]);
    assert_eq!(
        busy,
        [
            true, true, true, true, true, true, true, true, true, false, false, false
        ]
    );
}

/// Callback `0x1000` fires once, as the `InitAction` reaches its cue,
/// else its last frame: `frames - 2` runs after its push, at once for
/// two frames or fewer. A shaman's `Cast`, nine frames at 100 ms cued
/// at frame 4, peaks 300 ms in; uncued, 700.
#[test]
fn an_init_action_peaks_at_its_last_frame() {
    let cued = |frames: u16, cue: Option<u16>| {
        let pose = action::Pose {
            frames: vec![frames; 8],
            playback: 100,
            keeps: 0,
            cues: vec![cue; 8],
        };
        let mut walker = Walker::standing(&Flat, (0, 0), 8, &CLASS_1);
        let mut at = vec![];

        if walker.act(Some(8), pose, action::Settle::Keep, 0) == action::Peak::Reached {
            at.push(0);
        }

        for frame in 1..30 {
            let now = frame * MS_PER_FRAME;

            if walker.run_stage(now) == action::Peak::Reached {
                at.push(now);
            }
        }

        at
    };
    let peaks = |frames: u16| cued(frames, None);

    assert_eq!(cued(9, Some(4)), [300]);
    assert_eq!(peaks(9), [700]);
    assert_eq!(peaks(4), [200]);
    assert_eq!(peaks(2), [0]);
    assert_eq!(peaks(1), [0]);
    assert_eq!(peaks(0), [0]);
}

/// An action on the actor itself turns nothing; a set it lacks plays
/// no frame, so track 2 is empty at once.
#[test]
fn a_missing_set_leaves_track_two_empty() {
    assert_eq!(stage(None, 0, action::Pose::default())[0], (false, 0));
    assert_eq!(stage(Some(8), 0, action::Pose::default())[1], (false, 8));
}

/// An after-set sets the facing back to its slot's first direction as
/// the `InitAction` ends. Play5's ratman
/// champion 35 attacks facing 12 and ends on 11, so its turn to 16
/// takes two runs; without an after-set it stays on 12.
#[test]
fn an_after_set_turns_the_facing_back_to_its_slots_first_direction() {
    let attack = action::Pose {
        frames: vec![8; 8],
        playback: 100,
        keeps: 0,
        cues: vec![],
    };
    let end = |settle| {
        let mut walker = Walker::standing(&Flat, (0, 0), 12, &CLASS_1);

        walker.act(Some(12), attack.clone(), settle, 0);

        for frame in 1..20 {
            walker.run_stage(frame * MS_PER_FRAME);
        }

        (walker.acting(), walker.facing)
    };

    assert_eq!(end(action::Settle::Slot(8)), (false, 11));
    assert_eq!(end(action::Settle::Keep), (false, 12));
}

/// Track 2 shows its `InitAction`'s frame, a frame a run from frame 1
/// at its push, then its after-set; without one, the last frame stays.
/// A step clears it.
#[test]
fn track_two_shows_its_frame_then_its_after_set() {
    let attack = action::Pose {
        frames: vec![4; 8],
        playback: 100,
        keeps: 0,
        cues: vec![],
    };
    let looks = |after| {
        let mut walker = Walker::standing(&Flat, (0, 0), 12, &CLASS_1);

        walker.plays(action::Sets { set: 0x10, after });
        walker.act(Some(12), attack.clone(), action::Settle::Keep, 0);

        (0..8)
            .map(|frame| {
                walker.run_stage(frame * MS_PER_FRAME);
                walker.look()
            })
            .collect::<Vec<_>>()
    };
    let frame = |frame| Some(action::Look::Frame { set: 0x10, frame });

    assert_eq!(
        looks(8),
        [
            frame(1),
            frame(1),
            frame(2),
            frame(2),
            frame(3),
            frame(3),
            Some(action::Look::Plays { set: 8, since: 300 }),
            Some(action::Look::Plays { set: 8, since: 300 }),
        ]
    );
    assert_eq!(looks(0)[7], frame(3));
}

/// A `MovePath` waits while track 2 is busy: no plan
/// until the `InitAction` ends; the Action's agent goes with it.
#[test]
fn a_move_path_waits_for_track_two() {
    let mut plans = vec![];
    let mut walker = Walker::standing(&Flat, (0, 0), 8, &CLASS_1);
    let carry = action::Pose {
        frames: vec![4; 8],
        playback: 100,
        keeps: 0,
        cues: vec![],
    };

    walker.act(Some(8), carry, action::Settle::Keep, 0);
    walker.hand_over();
    walker.push_path(0);

    for frame in 1..10 {
        let now = frame * MS_PER_FRAME;

        walker.frame(&Flat, now, &mut |at: (i32, i32), _| {
            plans.push(now);
            vec![at]
        });
        assert_eq!(walker.holds(), walker.acting());
        walker.run_stage(now);
    }

    assert_eq!(plans.first(), Some(&350));
}

#[test]
fn a_rate_rounds_to_whole_frames() {
    assert_eq!(rate(120, 0), 100);
    assert_eq!(rate(75, 0), 100);
    assert_eq!(rate(74, 0), 50);
    assert_eq!(rate(0, -5), 1);
}
