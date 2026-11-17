//! The Book of Lore: the lore catalogue (`files::lore`) as a page over
//! the main menu (`menu::lore` draws it), and the system marking what a
//! profile sees, owns and kills in play.

use bevy::log::warn;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use regent_core::data::fourcc::FourCc;
use regent_draw::art::{self, Answer, ArtSource, Pic, Picture, Place, Scene, Snap, Stack, Want};

use crate::draw::frame;
use crate::files::lore::{self, Catalogue, Category, Entry};
use crate::files::profile::{LoreMark, Player, Profile};
use crate::game::seen::{Frame, News, Watch};
use crate::menu::Menu;
use crate::ui::figure::{self, Figure};

/// The figure's running strip, while the book shows it.
const FIGURE_SCENE: Scene = Scene("book of lore");

/// The book's state: open or not, what it shows, and the art it reads.
#[derive(Resource, Default)]
pub struct Book {
    open: bool,
    /// The category shown.
    pub(crate) tab: Category,
    pub(crate) selected: Option<FourCc>,
    /// How far the list and the details are scrolled, layout pixels.
    pub(crate) list_scroll: f32,
    pub(crate) detail_scroll: f32,
    /// The list scrolls to the picked entry once: picked elsewhere than
    /// in the list.
    pub(crate) follow: bool,
    dirty: bool,
    art: Option<regent_core::data::catalogue::Catalogue<'static>>,
    /// The last entry's figure, kept: a unit's art takes a parse.
    figure: Option<(FourCc, Option<Figure>)>,
}

impl Book {
    /// Whether the book shows.
    #[must_use]
    pub const fn is_open(&self) -> bool {
        self.open
    }

    /// `entry` picked from outside the list: its tab chosen, the list
    /// scrolled to it, its details from their top.
    pub(crate) const fn pick(&mut self, entry: &Entry) {
        self.tab = entry.category;
        self.selected = Some(entry.code);
        self.detail_scroll = 0.0;
        self.follow = true;
    }

    /// The archives' art, read on first use.
    fn art(
        &mut self,
        catalogue: &Catalogue,
    ) -> Option<&regent_core::data::catalogue::Catalogue<'static>> {
        if self.art.is_none() {
            match regent_draw::data::catalogue(&catalogue.1) {
                Ok(art) => self.art = Some(art),
                Err(why) => warn!("lore art: {why}"),
            }
        }

        self.art.as_ref()
    }

    /// Ask the shared art service for a portrait at its displayed size,
    /// window rectangle `rect`.
    pub(crate) fn portrait(
        &mut self,
        ctx: &egui::Context,
        catalogue: &Catalogue,
        shared: &mut art::Art,
        rect: egui::Rect,
        code: FourCc,
    ) -> Option<art::Shown> {
        let source = ArtSource::of(&catalogue.1);
        let read = (source, self.art(catalogue)?);
        let want = Want::of(source, Picture::Pic(Pic::Portrait(code)));
        let place = Place {
            scale: 1.0,
            origin: egui::Pos2::ZERO,
            rect,
            snap: Snap::Corner,
        };

        shared.get(ctx, &read, want, place).map(Answer::now)
    }

    /// Entry `code`'s figure centred in `rect`, at the step the clock
    /// is at: at the page's scale `full` where it fits, else shrunk;
    /// `rect` a window rectangle.
    pub(crate) fn figure(
        &mut self,
        ctx: &egui::Context,
        catalogue: &Catalogue,
        shared: &mut art::Art,
        (rect, full): (egui::Rect, f32),
        code: FourCc,
    ) -> Option<art::Shown> {
        let source = ArtSource::of(&catalogue.1);
        self.art(catalogue)?;
        let found = match self.figure {
            Some((kept, found)) if kept == code => found,
            _ => figure::of(self.art.as_ref()?, code),
        };
        self.figure = Some((code, found));

        let found = found?;
        let read = (source, self.art.as_ref()?);
        let frames: Vec<Want> = (0..found.timeline.steps)
            .map(|step| {
                let stack = Stack {
                    code,
                    set: found.set,
                    facing: found.facing as u16,
                    step,
                };

                Want::of(source, Picture::Stack(stack))
            })
            .collect();
        let tick = u64::from(found.timeline.tick);
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "ms since start: positive, far below u64's reach"
        )]
        let ms = ctx.input(|i| i.time * 1000.0) as u64;
        let shown = usize::try_from(ms / tick).unwrap_or(0) % frames.len();
        let size = shared.size(ctx, &read, *frames.first()?)?;
        let scale = figure_scale(rect.size(), size, full);
        let place = Place {
            scale,
            origin: rect.center() - size * scale / 2.0,
            rect: egui::Rect::from_min_size(egui::Pos2::ZERO, size),
            snap: Snap::Corner,
        };

        if frames.len() > 1 {
            ctx.request_repaint_after(std::time::Duration::from_millis(tick));
        }

        shared
            .strip(ctx, &read, &frames, shown, (place, FIGURE_SCENE))
            .map(Answer::now)
    }
}

/// Whether the simulation stepped since this last said so: a game's
/// first frame counts, so pieces already present (a loaded save, a
/// paused start) are folded in. Pieces, fog and kills change only at steps.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
fn stepped(frame: Option<Res<Frame>>, mut last: Local<Option<u32>>) -> bool {
    frame.is_some_and(|frame| {
        let ran = frame.is_added() || took_step(*last, frame.frames);

        *last = Some(frame.frames);

        ran
    })
}

/// Whether a game at frame `now` ran a step since frame `last`, if it
/// was looked at before.
fn took_step(last: Option<u32>, now: u32) -> bool {
    last != Some(now)
}

/// How large a figure of `size` art pixels shows in `room` window
/// pixels: at the page's scale, `full` window pixels to an art pixel,
/// or shrunk to fit. Never larger.
fn figure_scale(room: egui::Vec2, size: egui::Vec2, full: f32) -> f32 {
    (room / size).min_elem().min(full)
}

/// Fold simulation events and visible pieces into this profile.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
pub fn sync(
    seen: Watch,
    mut news: ResMut<News>,
    catalogue: Res<Catalogue>,
    mut player: ResMut<Player>,
    mut book: ResMut<Book>,
) {
    let kills = std::mem::take(&mut news.lore_kills);
    let Some(profile) = &mut player.profile else {
        return;
    };

    for kill in kills {
        if kill.owner == seen.local && catalogue.0.contains_key(&kill.victim) {
            profile.lore.kill(kill.victim);
            book.dirty = true;
        }
    }

    let mask = regent_core::view::map::fog::mask(seen.local);

    for (id, code, owner) in seen.world().lore_pieces() {
        if catalogue.0.contains_key(&code) {
            if owner == seen.local {
                book.dirty |= profile.lore.own(code);
            } else if seen.world().seen_level(id, mask) > 0 {
                book.dirty |= profile.lore.see(code);
            }
        }
    }

    if book.dirty {
        match profile.save_lore() {
            Ok(()) => book.dirty = false,
            Err(why) => warn!("{why}"),
        }
    }
}

/// The book over the main menu while open: BOOK OF LORE opens it, its
/// MAIN MENU closes it (`menu::lore`).
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
pub fn show(
    mut contexts: EguiContexts,
    catalogue: Option<Res<Catalogue>>,
    player: Res<Player>,
    (menu, mut pressed): (Res<Menu>, ResMut<crate::menu::Lore>),
    mut book: ResMut<Book>,
    (mut art, mut menus): (ResMut<art::Art>, ResMut<crate::menu::Art>),
) -> Result {
    // The main menu's BOOK OF LORE opens or closes it (`menu::row`).
    if pressed.0 {
        pressed.0 = false;
        book.open = !book.open;
    }

    if !menu.is_main() {
        book.open = false;
    } else if let Some(catalogue) = catalogue {
        // `--menu lore:CODE`: open on that entry, its tab chosen.
        if let Some(entry) = pressed.1.take().and_then(|code| catalogue.0.get(&code)) {
            book.pick(entry);
        }

        if book.open {
            let ctx = contexts.ctx_mut()?;
            let shut = crate::menu::lore::page(
                ctx,
                (&mut menus, &mut art),
                &mut book,
                (catalogue.as_ref(), player.profile.as_ref()),
            );

            book.open = shut.is_none();
        }
    }

    Ok(())
}

/// The profile's mark on `code`, with what its won quests teach; none
/// without a profile.
pub(crate) fn known(profile: Option<&Profile>, code: FourCc) -> LoreMark {
    profile.map_or_else(LoreMark::default, |p| {
        lore::known(code, p.lore.mark(code), &p.progress)
    })
}

pub fn plugin(app: &mut App) {
    app.init_resource::<Book>().add_systems(
        Update,
        sync.after(crate::game::clock::tick)
            .run_if(stepped)
            .in_set(frame::Frame::Step),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A frame not looked at before, or at another count, is a step's.
    #[test]
    fn sync_waits_for_a_step() {
        assert!(took_step(None, 0));
        assert!(took_step(Some(0), 1));
        assert!(!took_step(Some(1), 1));
    }

    #[test]
    fn a_figure_shrinks_to_fit_and_never_grows() {
        let room = egui::vec2(400.0, 400.0);

        // A small figure in a large room stays at the page's scale.
        assert_eq!(figure_scale(room, egui::vec2(40.0, 50.0), 2.0), 2.0);

        // A figure too large for its room at that scale shrinks to fit.
        assert_eq!(figure_scale(room, egui::vec2(100.0, 800.0), 2.0), 0.5);
    }
}
