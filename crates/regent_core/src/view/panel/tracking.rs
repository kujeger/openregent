//! The tracking window: mini view 1, the picture under the page
//! (`APMK` 2002, "Tracking Window"), and its button row. What it
//! follows and why.
//! **Binary** unless marked.
//!
//! ```text
//!  an iso view at zoom 0.5, the local player's
//!    sight, the main view's cursor mode
//!  follows (GSGameView)
//!    a new game      the palace, "given"
//!    a load          the save's (GSGL)
//!    asked           selection panel 8001, a unit list row, Alt+F2..F8,
//!                    Ctrl+1040 the selection
//!    a new message   a message flag, the option on, unless one already
//!    a new hero      the local player's, while following nothing or
//!                    the palace it was given (GSGameView)
//!    minimap right   none: the view stays there
//!  each frame (GSIsoGameView)
//!    held            its holder
//!    unseen          no move; a selected one is deselected
//!    a building      centred on its exit, anything else where it is
//!  gone (GSGameView, 0x7435358), the first that holds:
//!    the option on: the message flag nearest
//!    a spot kept     none, back there
//!    a piece kept    it, if it lives
//!    the local player's hero nearest, then its palace (given)
//!  kept: following on from something, the spot left or the piece
//!    taken (a message flag keeps nothing): the way back from a message
//!  row: 1015 flags, 1013 heroes, 1021 henchmen, 1014 buildings are
//!    filters, all off at first; 1012 back and 1009 forward scan the
//!    next category on, heroes > henchmen > buildings > flags, and in it
//!    the piece after the last one shown, in list order, wrapping;
//!    1016 Autoscan forward every CYCLE_MS game ms; 1040 the main view
//!    to the window's contents, with Ctrl the selection followed.
//!    No filter on: 1012, 1009, 1016 disabled, Autoscan off
//!  caption 4467: the followed piece's panel name (GSIsoGameView)
//!  the option's tips: GMTX 337, 338, "Messages will (not) appear in
//!    the Tracking Window."
//! ```
//!
//! **Inferred**: list order is agent order; the follow step runs each
//! drawn frame; the distances are from the view's middle (the engine's
//! point comes from its top left through the view, unread).

use serde::{Deserialize, Serialize};

use crate::sim::world::{World, bodies::Class, piece_state};

/// What a scan lists ([`scanned`]): the minimap's scans of the local
/// player's standing pieces with an agent. **Binary**.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scanned {
    /// Unit list 1, the hero test.
    Heroes,
    /// Unit list 1, the henchman test.
    Henchmen,
    /// Unit list 0.
    Buildings,
    /// Unit list 4, not held.
    Flags,
}

/// Whether piece `id` is one `owner`'s scan for `scan` lists: owned,
/// standing (not a ruin nor lying dead; **Inferred**), and of the scan's
/// kind. A reward flag held inside another is not listed; the others
/// are, held or not.
#[must_use]
pub fn scanned(world: &World, id: u32, owner: u32, scan: Scanned) -> bool {
    let piece = world.piece(id);
    let place = piece.and_then(|p| p.place);
    let standing = place.is_none_or(|p| p.state & piece_state::RUINED == 0);
    let theirs = piece.is_some_and(|p| p.owner == owner);
    let class = piece.and_then(|p| world.descriptors().class(p.code));

    let kind = match scan {
        Scanned::Heroes => class == Some(Class::Character) && world.is_hero(id),
        Scanned::Henchmen => class == Some(Class::Character) && world.henchman(id),
        Scanned::Buildings => class == Some(Class::Building),
        Scanned::Flags => place.is_some_and(|p| p.is_reward() && p.state & piece_state::HELD == 0),
    };

    theirs && standing && kind
}

/// Its zoom, the main view's zoomed out.
pub const ZOOM: f32 = 0.5;

/// `#miniview_cycle_rate`, game ms between Autoscan's steps
/// (miniview cycle rate, both `globals.gpl`).
pub const CYCLE_MS: u32 = 1500;

/// The button row's columns ([`crate::view::panel::tip::MINI_ROWS`]).
pub mod column {
    pub const FLAGS: usize = 2;
    pub const HEROES: usize = 3;
    pub const HENCHMEN: usize = 4;
    pub const BUILDINGS: usize = 5;
    pub const BACK: usize = 6;
    pub const AUTOSCAN: usize = 7;
    pub const FORWARD: usize = 8;
    pub const ZOOM: usize = 9;
}

/// What a scan steps through: the engine's numbers, 0 to 3.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Category {
    #[default]
    Heroes,
    Buildings,
    Flags,
    Henchmen,
}

impl Category {
    /// In the engine's order.
    pub const ALL: [Category; 4] = [
        Category::Heroes,
        Category::Buildings,
        Category::Flags,
        Category::Henchmen,
    ];

    /// The order forward steps through.
    const CYCLE: [Category; 4] = [
        Category::Heroes,
        Category::Henchmen,
        Category::Buildings,
        Category::Flags,
    ];

    /// The engine's number, an index of [`Tracker::last`].
    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Category::Heroes => 0,
            Category::Buildings => 1,
            Category::Flags => 2,
            Category::Henchmen => 3,
        }
    }

    /// Its filter bit.
    const fn bit(self) -> u32 {
        1 << self.index()
    }

    /// The filter a row column sets.
    #[must_use]
    pub const fn of_column(column: usize) -> Option<Category> {
        match column {
            column::FLAGS => Some(Category::Flags),
            column::HEROES => Some(Category::Heroes),
            column::HENCHMEN => Some(Category::Henchmen),
            column::BUILDINGS => Some(Category::Buildings),
            _ => None,
        }
    }

    /// What its scan lists.
    #[must_use]
    pub const fn scanned(self) -> Scanned {
        match self {
            Category::Heroes => Scanned::Heroes,
            Category::Buildings => Scanned::Buildings,
            Category::Flags => Scanned::Flags,
            Category::Henchmen => Scanned::Henchmen,
        }
    }

    /// The next category `filters` has on, `way` along `Category::CYCLE`,
    /// this one last; none on: heroes (mini category forward,
    /// mini category back).
    #[must_use]
    pub fn stepped(self, filters: Filters, way: Way) -> Category {
        let at = Self::CYCLE.iter().position(|c| *c == self).unwrap_or(0);
        let step = match way {
            Way::Forward => 1,
            Way::Back => Self::CYCLE.len() - 1,
        };

        (1..=Self::CYCLE.len())
            .map(|n| Self::CYCLE[(at + n * step) % Self::CYCLE.len()])
            .find(|c| filters.on(*c))
            .unwrap_or_default()
    }
}

/// Which way a scan goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Way {
    Forward,
    Back,
}

/// The row's four filters, a bit each ([`Category`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Filters(pub u32);

impl Filters {
    #[must_use]
    pub const fn on(self, category: Category) -> bool {
        self.0 & category.bit() != 0
    }

    pub const fn set(&mut self, category: Category, on: bool) {
        if on {
            self.0 |= category.bit();
        } else {
            self.0 &= !category.bit();
        }
    }

    #[must_use]
    pub const fn none(self) -> bool {
        self.0 == 0
    }
}

/// What the window can follow.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Target {
    /// A piece, by agent.
    Piece(u32),
    /// A message flag, by handle: it has no agent.
    Message(u32),
}

/// What the window follows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Follows {
    /// Never set, or cleared as its piece went (0).
    #[default]
    Nothing,
    /// Told to follow none (`'none'`): it stays where it is.
    Stopped,
    On(Target),
}

/// The way back once what it follows goes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kept {
    #[default]
    Nothing,
    /// Where it stood as it stopped following, 32nds.
    Spot((i32, i32)),
    /// The piece it last took, by agent.
    Piece(u32),
}

/// Where the window centres.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    /// Where agent's body is drawn.
    Piece(u32),
    /// A building's exit, 32nds.
    Exit(u32, (i32, i32)),
    /// A message flag's place, 32nds.
    Point((i32, i32)),
}

/// Where a press of 1040 puts the main view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Jump {
    /// On agent's place, or its holder's.
    Piece(u32),
    /// On a point, 32nds.
    Point((i32, i32)),
}

/// What a frame's follow step asks of the front end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// Centred there.
    Centre(Focus),
    /// The piece is unseen: the view stays, and the piece, selected, is
    /// deselected.
    Unseen(u32),
    /// Following nothing: the view stays.
    Still,
}

/// Who changes what it follows: a scan keeps Autoscan on, anything else
/// stops it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum By {
    Scan,
    Other,
}

/// What the window reads of the game.
pub trait Scene {
    /// The local player.
    fn local(&self) -> u32;
    /// The pieces a scan lists, in list order.
    fn listed(&self, scanned: Scanned) -> Vec<u32>;
    /// Whether a target still stands in the world.
    fn alive(&self, target: Target) -> bool;
    /// The local player's standing characters `IsValidMiniViewTarget`
    /// takes, with their places, in list order.
    fn heroes(&self) -> Vec<(u32, (i32, i32))>;
    /// Whether `IsValidMiniViewTarget` takes agent `agent`.
    fn valid(&self, agent: u32) -> bool;
    /// Every message flag and its place.
    fn messages(&self) -> Vec<(u32, (i32, i32))>;
    /// The local player's palace.
    fn palace(&self) -> Option<u32>;
    /// Where the window centres on a target, its holder's if held.
    fn focus(&self, target: Target) -> Option<Focus>;
    /// A focus's point, 32nds.
    fn point(&self, focus: Focus) -> Option<(i32, i32)>;
    /// Whether the local player sees agent `agent`.
    fn seen(&self, agent: u32) -> bool;
    /// Agent `agent`'s place, or its holder's.
    fn place(&self, agent: u32) -> Option<(i32, i32)>;
}

/// The window's state, saved with a game as the original's `GSGL` does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tracker {
    pub follows: Follows,
    pub kept: Kept,
    /// Following the palace a start gave it: a new hero takes over.
    pub given: bool,
    pub filters: Filters,
    pub autoscan: bool,
    /// The category last scanned.
    pub category: Category,
    /// Each category's last scanned piece, by [`Category::index`]
    /// ( to a field).
    pub last: [Option<u32>; 4],
    /// The view's middle, 32nds: where it last centred.
    pub middle: Option<(i32, i32)>,
    /// The game clock last stepped, for Autoscan.
    pub clock: Option<u32>,
    /// The message scroll's option: a new message flag is followed.
    pub messages: bool,
}

impl Default for Tracker {
    /// As `GSGameView`'s constructor leaves it: nothing
    /// followed, nothing kept, given set, every filter off.
    fn default() -> Tracker {
        Tracker {
            follows: Follows::Nothing,
            kept: Kept::Nothing,
            given: true,
            filters: Filters::default(),
            autoscan: false,
            category: Category::Heroes,
            last: [None; 4],
            middle: None,
            clock: None,
            messages: crate::view::panel::message::FOLLOW_DEFAULT,
        }
    }
}

/// The piece after (or before) `shown` in `list`, wrapping; `shown` gone,
/// the first (or the last); none kept, the first.
#[must_use]
pub fn next(list: &[u32], shown: Option<u32>, way: Way) -> Option<u32> {
    let at = shown.and_then(|l| list.iter().position(|a| *a == l));

    match (shown, at, way) {
        (None, ..) | (Some(_), None, Way::Forward) => list.first(),
        (Some(_), None, Way::Back) => list.last(),
        (_, Some(i), Way::Forward) => list.get(i + 1).or_else(|| list.first()),
        (_, Some(i), Way::Back) => i
            .checked_sub(1)
            .and_then(|j| list.get(j))
            .or_else(|| list.last()),
    }
    .copied()
}

/// The entry of `found` nearest `from`, the first of equals.
fn nearest(found: Vec<(u32, (i32, i32))>, from: Option<(i32, i32)>) -> Option<u32> {
    let gap = |(x, y): (i32, i32)| {
        from.map_or(0, |(fx, fy)| {
            i64::from(x - fx).pow(2) + i64::from(y - fy).pow(2)
        })
    };

    found
        .into_iter()
        .fold(None, |best: Option<(u32, i64)>, (agent, at)| {
            let d = gap(at);

            match best {
                Some((_, b)) if b <= d => best,
                _ => Some((agent, d)),
            }
        })
        .map(|(agent, _)| agent)
}

impl Tracker {
    /// A new game's: the local player's palace followed, given.
    #[must_use]
    pub fn started(scene: &impl Scene) -> Tracker {
        let mut mini = Tracker::default();

        if let Some(palace) = scene.palace() {
            mini.set(Follows::On(Target::Piece(palace)), By::Other, scene);
            mini.given = true;
        }

        mini
    }

    /// What it follows, changed: anything but a
    /// scan stops Autoscan; given is cleared; following on from
    /// something keeps the way back: the spot for none, the piece taken,
    /// nothing new for a message flag.
    fn set(&mut self, to: Follows, by: By, scene: &impl Scene) {
        if by == By::Other {
            self.autoscan = false;
        }

        self.given = false;

        if self.follows != Follows::Nothing {
            match to {
                Follows::Nothing | Follows::Stopped => {
                    self.kept = self.middle.map_or(Kept::Nothing, Kept::Spot);
                }
                Follows::On(Target::Piece(agent)) if scene.alive(Target::Piece(agent)) => {
                    self.kept = Kept::Piece(agent);
                }
                Follows::On(_) => {}
            }
        }

        self.follows = to;
    }

    /// Follow `target`, asked: a panel's track button, a list row, a
    /// key.
    pub fn follow(&mut self, target: Target, scene: &impl Scene) {
        self.set(Follows::On(target), By::Other, scene);
    }

    /// Follow nothing, centred on `at`, 32nds: the minimap's right press.
    pub fn stop_at(&mut self, at: (i32, i32), scene: &impl Scene) {
        self.set(Follows::Stopped, By::Other, scene);
        self.middle = Some(at);
    }

    /// A message flag made: followed with the option on, unless one is
    /// already.
    pub fn message(&mut self, handle: u32, scene: &impl Scene) {
        let reading = matches!(self.follows, Follows::On(Target::Message(_)));

        if self.messages && !reading {
            self.follow(Target::Message(handle), scene);
        }
    }

    /// A piece made: a hero `IsValidMiniViewTarget` takes, followed while
    /// following nothing or the palace it was given (`0x8435358`).
    pub fn born(&mut self, agent: u32, scene: &impl Scene) {
        let palace = scene.palace().map(Target::Piece);
        let idle = match self.follows {
            Follows::Nothing => true,
            Follows::On(t) => self.given && Some(t) == palace,
            Follows::Stopped => false,
        };

        if idle && scene.valid(agent) {
            self.follow(Target::Piece(agent), scene);
        }
    }

    /// What it followed went (`0x7435358`): the first that holds of the
    /// nearest message flag (the option on), the spot kept, the piece
    /// kept, the nearest hero, the palace.
    pub fn gone(&mut self, scene: &impl Scene) {
        self.follows = Follows::Nothing;

        let message = nearest(scene.messages(), self.middle).filter(|_| self.messages);

        if let Some(handle) = message {
            self.set(Follows::On(Target::Message(handle)), By::Other, scene);
            return;
        }

        match self.kept {
            Kept::Spot(at) => {
                self.set(Follows::Stopped, By::Other, scene);
                self.middle = Some(at);
                return;
            }
            Kept::Piece(agent) => {
                self.kept = Kept::Nothing;

                if scene.alive(Target::Piece(agent)) {
                    self.set(Follows::On(Target::Piece(agent)), By::Other, scene);
                    return;
                }
            }
            Kept::Nothing => {}
        }

        if let Some(hero) = nearest(scene.heroes(), self.middle) {
            self.set(Follows::On(Target::Piece(hero)), By::Other, scene);
        } else if let Some(palace) = scene.palace() {
            self.set(Follows::On(Target::Piece(palace)), By::Other, scene);
            self.given = true;
        }
    }

    /// A frame's follow step: what it followed
    /// gone first ([`Tracker::gone`]); then centred on it, unless unseen.
    pub fn step(&mut self, scene: &impl Scene) -> Step {
        if let Follows::On(target) = self.follows
            && !scene.alive(target)
        {
            self.gone(scene);
        }

        let Follows::On(target) = self.follows else {
            return Step::Still;
        };
        let Some(focus) = scene.focus(target) else {
            return Step::Still;
        };
        let unseen = match focus {
            Focus::Piece(agent) | Focus::Exit(agent, _) => (!scene.seen(agent)).then_some(agent),
            Focus::Point(_) => None,
        };

        if let Some(agent) = unseen {
            Step::Unseen(agent)
        } else {
            self.middle = scene.point(focus).or(self.middle);
            Step::Centre(focus)
        }
    }

    /// The game clock at `now` ms: Autoscan scans forward each time it
    /// passes a multiple of [`CYCLE_MS`].
    pub fn tick(&mut self, now: u32, scene: &impl Scene) {
        let passed = self.clock.map_or(now.is_multiple_of(CYCLE_MS), |was| {
            now / CYCLE_MS > was / CYCLE_MS
        });

        self.clock = Some(now);

        if self.autoscan && passed {
            self.scan(Way::Forward, scene);
        }
    }

    /// The next category on, and its next piece followed, if any.
    pub fn scan(&mut self, way: Way, scene: &impl Scene) {
        let category = self.category.stepped(self.filters, way);
        let found = next(
            &scene.listed(category.scanned()),
            self.last[category.index()],
            way,
        );

        self.category = category;
        self.last[category.index()] = found;

        if let Some(agent) = found {
            self.set(Follows::On(Target::Piece(agent)), By::Scan, scene);
        }
    }

    /// Whether the row's scan buttons take presses: a filter on.
    #[must_use]
    pub const fn scans(&self) -> bool {
        !self.filters.none()
    }

    /// A press on the row's `column`, a check now `checked`, with Ctrl
    /// `ctrl` and `selected` the selection:
    /// where 1040 puts the main view. No filter left on stops Autoscan.
    pub fn press(
        &mut self,
        column: usize,
        (checked, ctrl): (bool, bool),
        selected: Option<u32>,
        scene: &impl Scene,
    ) -> Option<Jump> {
        let mut jump = None;

        match column {
            column::BACK => self.scan(Way::Back, scene),
            column::FORWARD => self.scan(Way::Forward, scene),
            column::AUTOSCAN => self.autoscan = checked,
            column::ZOOM if ctrl => {
                if let Some(agent) = selected {
                    self.follow(Target::Piece(agent), scene);
                }
            }
            column::ZOOM => {
                jump = match self.follows {
                    Follows::On(Target::Piece(agent)) if scene.alive(Target::Piece(agent)) => {
                        Some(Jump::Piece(agent))
                    }
                    Follows::On(Target::Message(handle)) => scene
                        .messages()
                        .into_iter()
                        .find(|(h, _)| *h == handle)
                        .map(|(_, at)| Jump::Point(at)),
                    _ => None,
                }
                .or_else(|| self.middle.map(Jump::Point));
            }
            c => {
                if let Some(category) = Category::of_column(c) {
                    self.filters.set(category, checked);
                }
            }
        }

        if self.filters.none() {
            self.autoscan = false;
        }

        jump
    }
}

/// The window's [`Scene`] on a world: the published one, as the front
/// end draws it, for its local player.
pub struct Live<'a> {
    pub world: &'a World,
    pub local: u32,
}

impl Live<'_> {
    /// The local player's pieces `scanned` lists, by agent.
    fn owned(&self, scanned: Scanned) -> Vec<u32> {
        let local = self.local;
        let mut found: Vec<u32> = self
            .world
            .shown()
            .into_iter()
            .map(|s| s.agent)
            .filter(|a| self::scanned(self.world, *a, local, scanned))
            .collect();

        found.sort_unstable();
        found.dedup();
        found
    }
}

impl Scene for Live<'_> {
    fn local(&self) -> u32 {
        self.local
    }

    fn listed(&self, scanned: Scanned) -> Vec<u32> {
        self.owned(scanned)
    }

    fn alive(&self, target: Target) -> bool {
        match target {
            Target::Piece(agent) => self.world.piece(agent).is_some(),
            Target::Message(handle) => self.world.message_point(handle).is_some(),
        }
    }

    fn heroes(&self) -> Vec<(u32, (i32, i32))> {
        self.owned(Scanned::Heroes)
            .into_iter()
            .filter_map(|a| Some((a, self.world.position(a)?)))
            .collect()
    }

    fn valid(&self, agent: u32) -> bool {
        // The shipped `IsValidMiniViewTarget`: `subtype` `hero`, the
        // local player's (**Inferred**: the new-unit message reaches the
        // window only for the local player's pieces, its watch set).
        scanned(self.world, agent, self.local, Scanned::Heroes)
    }

    fn messages(&self) -> Vec<(u32, (i32, i32))> {
        self.world
            .messages()
            .into_iter()
            .filter_map(|m| Some((m.handle, self.world.message_point(m.handle)?)))
            .collect()
    }

    fn palace(&self) -> Option<u32> {
        self.world.player_palace(self.local)
    }

    fn focus(&self, target: Target) -> Option<Focus> {
        match target {
            Target::Piece(agent) => {
                let at = self.world.held_in(agent).unwrap_or(agent);

                if self.world.is_building(at) {
                    self.world.exit_of(at).map(|p| Focus::Exit(at, p))
                } else {
                    self.world.piece(at).map(|_| Focus::Piece(at))
                }
            }
            Target::Message(handle) => self.world.message_point(handle).map(Focus::Point),
        }
    }

    fn point(&self, focus: Focus) -> Option<(i32, i32)> {
        match focus {
            Focus::Piece(agent) => self.world.position(agent),
            Focus::Exit(_, at) | Focus::Point(at) => Some(at),
        }
    }

    fn seen(&self, agent: u32) -> bool {
        // Not yet sighted counts as seen: a new game's first drawn frame
        // comes before its first sight pass (**Inferred**: the engine
        // sights a piece as it is made, unit sight update).
        let world = self.world;

        !world.sighted(agent) || world.sight_word(agent, self.local) != 0
    }

    fn place(&self, agent: u32) -> Option<(i32, i32)> {
        self.world.position(agent)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    /// A world of a few pieces, each with a kind and a place.
    #[derive(Default)]
    struct Fake {
        pieces: BTreeMap<u32, (Scanned, (i32, i32))>,
        unseen: Vec<u32>,
        messages: Vec<(u32, (i32, i32))>,
        palace: Option<u32>,
    }

    impl Fake {
        fn with(pieces: &[(u32, Scanned, (i32, i32))]) -> Fake {
            Fake {
                pieces: pieces.iter().map(|(a, s, at)| (*a, (*s, *at))).collect(),
                ..Fake::default()
            }
        }
    }

    impl Scene for Fake {
        fn local(&self) -> u32 {
            0
        }

        fn listed(&self, scanned: Scanned) -> Vec<u32> {
            self.pieces
                .iter()
                .filter(|(_, (s, _))| *s == scanned)
                .map(|(a, _)| *a)
                .collect()
        }

        fn alive(&self, target: Target) -> bool {
            match target {
                Target::Piece(a) => self.pieces.contains_key(&a),
                Target::Message(h) => self.messages.iter().any(|(m, _)| *m == h),
            }
        }

        fn heroes(&self) -> Vec<(u32, (i32, i32))> {
            self.listed(Scanned::Heroes)
                .into_iter()
                .map(|a| (a, self.pieces[&a].1))
                .collect()
        }

        fn valid(&self, agent: u32) -> bool {
            self.pieces
                .get(&agent)
                .is_some_and(|(s, _)| *s == Scanned::Heroes)
        }

        fn messages(&self) -> Vec<(u32, (i32, i32))> {
            self.messages.clone()
        }

        fn palace(&self) -> Option<u32> {
            self.palace
        }

        fn focus(&self, target: Target) -> Option<Focus> {
            match target {
                Target::Piece(a) => self.pieces.get(&a).map(|(s, at)| match s {
                    Scanned::Buildings => Focus::Exit(a, *at),
                    _ => Focus::Piece(a),
                }),
                Target::Message(h) => self
                    .messages
                    .iter()
                    .find(|(m, _)| *m == h)
                    .map(|(_, at)| Focus::Point(*at)),
            }
        }

        fn point(&self, focus: Focus) -> Option<(i32, i32)> {
            match focus {
                Focus::Piece(a) => self.place(a),
                Focus::Exit(_, at) | Focus::Point(at) => Some(at),
            }
        }

        fn seen(&self, agent: u32) -> bool {
            !self.unseen.contains(&agent)
        }

        fn place(&self, agent: u32) -> Option<(i32, i32)> {
            self.pieces.get(&agent).map(|(_, at)| *at)
        }
    }

    const PALACE: u32 = 1;

    /// A start: the palace, two heroes, a henchman, a guild.
    fn town() -> Fake {
        Fake {
            palace: Some(PALACE),
            ..Fake::with(&[
                (PALACE, Scanned::Buildings, (0, 0)),
                (2, Scanned::Buildings, (100, 0)),
                (5, Scanned::Heroes, (300, 0)),
                (7, Scanned::Heroes, (50, 0)),
                (9, Scanned::Henchmen, (10, 10)),
            ])
        }
    }

    #[test]
    fn a_new_game_follows_the_palace_at_its_exit() {
        let town = town();
        let mut mini = Tracker::started(&town);

        assert_eq!(mini.follows, Follows::On(Target::Piece(PALACE)));
        assert!(mini.given);
        assert_eq!(mini.step(&town), Step::Centre(Focus::Exit(PALACE, (0, 0))));
        assert_eq!(mini.middle, Some((0, 0)));
    }

    #[test]
    fn a_new_hero_takes_over_from_the_given_palace_only() {
        let town = town();
        let mut mini = Tracker::started(&town);

        mini.born(9, &town);
        assert_eq!(
            mini.follows,
            Follows::On(Target::Piece(PALACE)),
            "a henchman"
        );

        mini.born(5, &town);
        assert_eq!(mini.follows, Follows::On(Target::Piece(5)));
        assert!(!mini.given);

        mini.born(7, &town);
        assert_eq!(mini.follows, Follows::On(Target::Piece(5)), "busy");

        mini.follow(Target::Piece(PALACE), &town);
        mini.born(7, &town);
        assert_eq!(
            mini.follows,
            Follows::On(Target::Piece(PALACE)),
            "asked, not given"
        );
    }

    #[test]
    fn a_dead_hero_gives_way_to_the_nearest_hero_then_the_palace() {
        let mut town = town();
        let mut mini = Tracker::started(&town);

        mini.follow(Target::Piece(5), &town);
        mini.step(&town);
        town.pieces.remove(&5);

        // Nearest the hero's last place, (300, 0): hero 7 at (50, 0).
        assert_eq!(mini.step(&town), Step::Centre(Focus::Piece(7)));

        town.pieces.remove(&7);
        mini.step(&town);
        assert_eq!(mini.follows, Follows::On(Target::Piece(PALACE)));
        assert!(mini.given);
    }

    #[test]
    fn a_read_message_goes_back_to_what_was_followed_before() {
        let mut town = town();
        let mut mini = Tracker::started(&town);

        mini.follow(Target::Piece(5), &town);
        mini.step(&town);
        town.messages.push((40, (900, 900)));
        mini.message(40, &town);
        assert_eq!(mini.step(&town), Step::Centre(Focus::Point((900, 900))));
        assert_eq!(mini.kept, Kept::Piece(5));

        town.messages.clear();
        mini.step(&town);
        assert_eq!(mini.follows, Follows::On(Target::Piece(5)));
    }

    #[test]
    fn with_the_option_on_a_gone_target_takes_the_nearest_message() {
        let mut town = town();
        let mut mini = Tracker::started(&town);

        mini.follow(Target::Piece(7), &town);
        mini.step(&town);
        town.messages = vec![(40, (900, 0)), (41, (60, 0))];
        town.pieces.remove(&7);
        mini.step(&town);
        assert_eq!(mini.follows, Follows::On(Target::Message(41)));

        mini.messages = false;
        mini.follow(Target::Piece(5), &town);
        town.pieces.remove(&5);
        mini.step(&town);
        assert_eq!(
            mini.follows,
            Follows::On(Target::Piece(PALACE)),
            "no hero left"
        );
    }

    #[test]
    fn a_minimap_stop_stays_and_keeps_the_spot_it_left() {
        let mut town = town();
        let mut mini = Tracker::started(&town);

        mini.step(&town);
        mini.stop_at((500, 500), &town);
        assert_eq!(mini.step(&town), Step::Still);
        assert_eq!(
            (mini.kept, mini.middle),
            (Kept::Spot((0, 0)), Some((500, 500)))
        );

        // A new hero does not take a stopped window.
        mini.born(5, &town);
        assert_eq!(mini.follows, Follows::Stopped);

        // A message flag keeps nothing: once read, back to the spot.
        town.messages.push((40, (900, 900)));
        mini.message(40, &town);
        assert_eq!(mini.kept, Kept::Spot((0, 0)));
        town.messages.clear();
        mini.step(&town);
        assert_eq!(
            (mini.follows, mini.middle),
            (Follows::Stopped, Some((0, 0)))
        );
    }

    #[test]
    fn an_unseen_piece_holds_the_view_and_asks_a_deselect() {
        let mut town = town();
        let mut mini = Tracker::started(&town);

        mini.follow(Target::Piece(5), &town);
        mini.step(&town);
        town.unseen.push(5);
        town.pieces.insert(5, (Scanned::Heroes, (999, 999)));

        assert_eq!(mini.step(&town), Step::Unseen(5));
        assert_eq!(mini.middle, Some((300, 0)));
    }

    #[test]
    fn forward_steps_heroes_henchmen_buildings_flags_and_wraps() {
        let on = |cs: &[Category]| {
            let mut f = Filters::default();

            for c in cs {
                f.set(*c, true);
            }
            f
        };
        let all = on(&Category::ALL);
        let order: Vec<Category> = std::iter::successors(Some(Category::Heroes), |c| {
            Some(c.stepped(all, Way::Forward))
        })
        .take(5)
        .collect();

        assert_eq!(
            order,
            [
                Category::Heroes,
                Category::Henchmen,
                Category::Buildings,
                Category::Flags,
                Category::Heroes
            ]
        );
        assert_eq!(Category::Heroes.stepped(all, Way::Back), Category::Flags);
        assert_eq!(
            Category::Buildings.stepped(on(&[Category::Buildings]), Way::Forward),
            Category::Buildings
        );
        assert_eq!(
            Category::Flags.stepped(Filters::default(), Way::Back),
            Category::Heroes
        );
    }

    #[test]
    fn a_scan_takes_the_next_piece_in_list_order_wrapping() {
        assert_eq!(next(&[3, 5, 8], None, Way::Back), Some(3));
        assert_eq!(next(&[3, 5, 8], Some(5), Way::Forward), Some(8));
        assert_eq!(next(&[3, 5, 8], Some(8), Way::Forward), Some(3));
        assert_eq!(next(&[3, 5, 8], Some(3), Way::Back), Some(8));
        assert_eq!(next(&[3, 5, 8], Some(4), Way::Forward), Some(3));
        assert_eq!(next(&[3, 5, 8], Some(4), Way::Back), Some(8));
        assert_eq!(next(&[], Some(4), Way::Back), None);
    }

    #[test]
    fn the_row_scans_its_filters_and_autoscan_keeps_on_until_asked_off() {
        let town = town();
        let mut mini = Tracker::started(&town);

        mini.press(column::BUILDINGS, (true, false), None, &town);
        mini.press(column::AUTOSCAN, (true, false), None, &town);
        assert!(mini.autoscan && mini.scans());

        mini.press(column::FORWARD, (false, false), None, &town);
        assert_eq!(mini.follows, Follows::On(Target::Piece(PALACE)));

        // Every CYCLE_MS of game time, a step forward; a scan keeps it on.
        mini.tick(1450, &town);
        mini.tick(1500, &town);
        assert_eq!(mini.follows, Follows::On(Target::Piece(2)));
        assert!(mini.autoscan);
        mini.tick(1550, &town);
        assert_eq!(mini.follows, Follows::On(Target::Piece(2)));

        // Asked to follow anything else, it stops.
        mini.follow(Target::Piece(5), &town);
        assert!(!mini.autoscan);

        // The last filter off stops it too.
        mini.press(column::AUTOSCAN, (true, false), None, &town);
        mini.press(column::BUILDINGS, (false, false), None, &town);
        assert!(!mini.autoscan && !mini.scans());
    }

    #[test]
    fn the_zoom_button_jumps_the_main_view_or_takes_the_selection_with_ctrl() {
        let town = town();
        let mut mini = Tracker::started(&town);

        assert_eq!(
            mini.press(column::ZOOM, (false, false), Some(9), &town),
            Some(Jump::Piece(PALACE))
        );
        assert_eq!(
            mini.press(column::ZOOM, (false, true), Some(9), &town),
            None
        );
        assert_eq!(mini.follows, Follows::On(Target::Piece(9)));

        mini.stop_at((70, 80), &town);
        assert_eq!(
            mini.press(column::ZOOM, (false, false), None, &town),
            Some(Jump::Point((70, 80)))
        );
    }
}
