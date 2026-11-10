//! The Book of Lore: a window over the main menu listing the lore
//! catalogue (`files::lore`), and the system marking what a profile
//! sees, owns and kills in play.

use bevy::log::warn;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use regent_core::data::desc::UnitKind;
use regent_core::data::fourcc::FourCc;
use regent_draw::art::{self, Answer, ArtSource, Pic, Picture, Place, Scene, Snap, Stack, Want};

use crate::draw::frame;
use crate::files::lore::{
    self, Catalogue, Category, Entry, SPAWNS_AFTER, default_spawns, details_needed,
    spawns_revealed, subtitle, unlock_text,
};
use crate::files::profile::{LoreMark, Player, Profile};
use crate::game::seen::{News, Watch};
use crate::menu::Menu;
use crate::ui::figure::{self, Figure};

const PORTRAIT_SIZE: f32 = 200.0;
/// The figure's room beside the portrait.
const FIGURE_SIZE: f32 = 200.0;
/// The figure's running strip, while the book shows it.
const FIGURE_SCENE: Scene = Scene("book of lore");
const ROW_PORTRAIT: f32 = 50.0;
const ROW_HEIGHT: f32 = 56.0;
const LIST_MAX_WIDTH: f32 = 300.0;
const ACCENT: egui::Color32 = egui::Color32::from_rgb(0xf6, 0xad, 0x52);

#[derive(Resource, Default)]
pub struct Book {
    open: bool,
    tab: Category,
    selected: Option<FourCc>,
    dirty: bool,
    art: Option<regent_core::data::catalogue::Catalogue<'static>>,
    /// The last entry's figure, kept: a unit's art takes a parse.
    figure: Option<(FourCc, Option<Figure>)>,
}

impl Book {
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

    /// Ask the shared art service for a portrait at its displayed size.
    fn portrait(
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
    /// is at: a whole multiple of its size where it fits, else shrunk.
    fn figure(
        &mut self,
        ctx: &egui::Context,
        catalogue: &Catalogue,
        shared: &mut art::Art,
        rect: egui::Rect,
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
        let fit = (rect.size() / size).min_elem();
        let scale = if fit >= 1.0 { fit.floor() } else { fit };
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

/// Temporary egui browser; its content comes from the parsed catalogue.
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
    mut art: ResMut<art::Art>,
) -> Result {
    // The main menu's BOOK OF LORE opens or closes it (`menu::row`).
    if pressed.0 {
        pressed.0 = false;
        book.open = !book.open;
    }

    if !menu.is_main() {
        book.open = false;
        return Ok(());
    }

    let Some(catalogue) = catalogue else {
        return Ok(());
    };

    // `--menu lore:CODE`: open on that entry, its tab chosen.
    if let Some(entry) = pressed.1.take().and_then(|code| catalogue.0.get(&code)) {
        book.tab = entry.category;
        book.selected = Some(entry.code);
    }

    let ctx = contexts.ctx_mut()?;
    let mut open = book.open;
    egui::Window::new("Book of Lore")
        .open(&mut open)
        .default_size(egui::vec2(900.0, 620.0))
        .show(ctx, |ui| {
            if let Some(profile) = &player.profile {
                ui.horizontal(|ui| {
                    for category in Category::ALL {
                        if ui
                            .selectable_value(&mut book.tab, category, category.name())
                            .clicked()
                        {
                            book.selected = None;
                        }
                    }
                });
                ui.separator();

                ui.horizontal_top(|ui| {
                    let left_width = LIST_MAX_WIDTH.min(ui.available_width() * 0.45);
                    let height = ui.available_height();
                    ui.allocate_ui_with_layout(
                        egui::vec2(left_width, height),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| list(ui, catalogue.as_ref(), profile, &mut book, &mut art),
                    );
                    ui.separator();

                    ui.allocate_ui_with_layout(
                        ui.available_size(),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            if let Some(entry) =
                                book.selected.and_then(|code| catalogue.0.get(&code))
                            {
                                egui::ScrollArea::vertical().id_salt("lore_details").show(
                                    ui,
                                    |ui| {
                                        details(
                                            ui,
                                            entry,
                                            catalogue.as_ref(),
                                            known(profile, entry.code),
                                            &mut book,
                                            &mut art,
                                        );
                                    },
                                );
                            } else {
                                ui.label("Select an entry.");
                            }
                        },
                    );
                });
            } else {
                ui.label("Choose a profile to view the book.");
            }
        });
    book.open = open;

    Ok(())
}

/// The profile's mark on `code`, with what its won quests teach.
fn known(profile: &Profile, code: FourCc) -> LoreMark {
    lore::known(code, profile.lore.mark(code), &profile.progress)
}

/// Rows keep unknown entries visible without naming them.
fn list(
    ui: &mut egui::Ui,
    catalogue: &Catalogue,
    profile: &Profile,
    book: &mut Book,
    art: &mut art::Art,
) {
    let mut entries: Vec<_> = catalogue
        .0
        .values()
        .filter(|entry| entry.category == book.tab)
        .collect();
    entries.sort_by(|a, b| a.title.cmp(&b.title).then(a.code.cmp(&b.code)));

    let discovered = entries
        .iter()
        .filter(|entry| known(profile, entry.code).seen)
        .count();
    ui.label(format!("Discovered {discovered}/{}", entries.len()));

    egui::ScrollArea::vertical()
        .id_salt(("lore_list", book.tab.name()))
        .show(ui, |ui| {
            for entry in entries {
                row(ui, catalogue, entry, known(profile, entry.code), book, art);
            }
        });
}

/// A selectable portrait and name, or an unknown placeholder.
fn row(
    ui: &mut egui::Ui,
    catalogue: &Catalogue,
    entry: &Entry,
    mark: LoreMark,
    book: &mut Book,
    art: &mut art::Art,
) {
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), ROW_HEIGHT),
        egui::Sense::click(),
    );
    if !ui.is_rect_visible(rect) {
        return;
    }

    let visuals = ui.visuals();
    let fill = if book.selected == Some(entry.code) {
        visuals.selection.bg_fill
    } else if response.hovered() {
        visuals.widgets.hovered.weak_bg_fill
    } else {
        egui::Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, 3.0, fill);

    let image = egui::Rect::from_min_size(
        rect.min + egui::vec2(2.0, 3.0),
        egui::vec2(ROW_PORTRAIT, ROW_PORTRAIT),
    );
    if mark.seen {
        if let Some(shown) = book.portrait(ui.ctx(), catalogue, art, image, entry.code) {
            ui.painter()
                .image(shown.id, shown.rect, shown.uv, egui::Color32::WHITE);
        }
    } else {
        ui.painter()
            .rect_filled(image, 3.0, visuals.widgets.inactive.weak_bg_fill);
        ui.painter().text(
            image.center(),
            egui::Align2::CENTER_CENTER,
            "?",
            egui::FontId::proportional(28.0),
            visuals.weak_text_color(),
        );
    }

    let name = if mark.seen { &entry.title } else { "Unknown" };
    let ink = if mark.seen {
        visuals.strong_text_color()
    } else {
        visuals.weak_text_color()
    };
    let subtitle = if mark.seen {
        subtitle(entry.code)
    } else {
        None
    };
    let title_y = if subtitle.is_some() {
        rect.center().y - 9.0
    } else {
        rect.center().y
    };
    ui.painter().with_clip_rect(rect).text(
        egui::pos2(image.right() + 10.0, title_y),
        egui::Align2::LEFT_CENTER,
        name,
        egui::TextStyle::Body.resolve(ui.style()),
        ink,
    );
    if let Some(subtitle) = subtitle {
        ui.painter().with_clip_rect(rect).text(
            egui::pos2(image.right() + 10.0, rect.center().y + 11.0),
            egui::Align2::LEFT_CENTER,
            subtitle,
            egui::TextStyle::Small.resolve(ui.style()),
            visuals.weak_text_color(),
        );
    }

    if response.clicked() {
        book.selected = Some(entry.code);
    }
}

/// Show only the fields the profile has earned.
fn details(
    ui: &mut egui::Ui,
    entry: &Entry,
    catalogue: &Catalogue,
    mark: LoreMark,
    book: &mut Book,
    art: &mut art::Art,
) {
    ui.spacing_mut().item_spacing.y = 1.0;

    if mark.seen {
        ui.label(
            egui::RichText::new(&entry.title)
                .size(20.0)
                .strong()
                .color(ACCENT),
        );
        if let Some(subtitle) = subtitle(entry.code) {
            ui.label(egui::RichText::new(subtitle).small().weak());
        }
        if entry.code == ENEMY_BARBARIAN {
            ui.label("Minimum birth stats. Health and combat stats scale with hero level.");
        }
        let pictured = ui
            .horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(
                    egui::vec2(PORTRAIT_SIZE, PORTRAIT_SIZE),
                    egui::Sense::hover(),
                );
                let portrait = book.portrait(ui.ctx(), catalogue, art, rect, entry.code);
                if let Some(portrait) = portrait {
                    ui.painter().image(
                        portrait.id,
                        portrait.rect,
                        portrait.uv,
                        egui::Color32::WHITE,
                    );
                }

                // Beside it, the entry as the map draws it; none without a set.
                let (rect, _) = ui.allocate_exact_size(
                    egui::vec2(FIGURE_SIZE, FIGURE_SIZE),
                    egui::Sense::hover(),
                );
                if let Some(shown) = book.figure(ui.ctx(), catalogue, art, rect, entry.code) {
                    ui.painter()
                        .image(shown.id, shown.rect, shown.uv, egui::Color32::WHITE);
                }

                portrait.is_some()
            })
            .inner;
        if !pictured {
            ui.label("No portrait in shipped data");
        }

        if let Some(help) = &entry.help {
            ui.add_space(6.0);
            info(ui, help);
        }

        let verb = if entry.kind == UnitKind::Building {
            "Destroyed"
        } else {
            "Slain"
        };
        ui.add_space(8.0);
        ui.separator();
        ui.label(
            egui::RichText::new(format!("{verb}: {}", mark.kills))
                .size(14.0)
                .color(ACCENT),
        );
        section(ui, "Known at encounter");
        stats(ui, entry, Reveal::Basic);

        let needed = details_needed(entry.code);
        if mark.owned || mark.kills >= needed {
            section(ui, "Details");
            stats(ui, entry, Reveal::Learned);

            if !entry.links.is_empty() {
                section(ui, "Connections");
                egui::Grid::new("lore_links").num_columns(2).show(ui, |ui| {
                    for (label, code) in &entry.links {
                        let name = catalogue
                            .0
                            .get(code)
                            .map_or_else(|| code.to_string(), |target| target.title.clone());
                        ui.label(egui::RichText::new(*label).size(14.0).weak());
                        ui.label(egui::RichText::new(name).size(14.0).color(ACCENT));
                        ui.end_row();
                    }
                });
            }
        } else {
            section(ui, "Details");
            ui.label(
                egui::RichText::new(unlock_text(entry.code, mark.kills, needed))
                    .size(14.0)
                    .weak(),
            );
        }

        if entry.category == Category::Lairs {
            spawn_list(ui, entry, catalogue, mark);
        }
    } else {
        ui.label(egui::RichText::new("Unknown").size(20.0).color(ACCENT));
        ui.label("Meet, defeat, or own this entry to reveal it.");
    }
}

/// `BV02 Enemy_Barbarian`.
const ENEMY_BARBARIAN: FourCc = FourCc(*b"BV02");

/// `BBT1 SpireOfDeath`.
const SPIRE_OF_DEATH: FourCc = FourCc(*b"BBT1");

/// `BBQ1 FortressofIxmil`.
const FORTRESS_OF_IXMIL: FourCc = FourCc(*b"BBQ1");

fn spawn_list(ui: &mut egui::Ui, entry: &Entry, catalogue: &Catalogue, mark: LoreMark) {
    let spire = entry.code == SPIRE_OF_DEATH;
    section(
        ui,
        if spire {
            "Scripted spawns"
        } else {
            "Default spawns"
        },
    );
    if spawns_revealed(mark) {
        let spawns = default_spawns(entry.code);
        if spawns.is_empty() {
            ui.label("No regular spawns");
        } else {
            for code in spawns {
                let name = catalogue
                    .0
                    .get(code)
                    .map_or_else(|| code.to_string(), |target| target.title.clone());
                ui.label(egui::RichText::new(name).size(14.0).color(ACCENT));
            }
        }
        if spire {
            ui.label(egui::RichText::new("Spawn set changes during the quest.").weak());
        } else if entry.code == FORTRESS_OF_IXMIL {
            ui.label(egui::RichText::new("Quest events spawn monsters here.").weak());
        }
    } else {
        ui.label(
            egui::RichText::new(unlock_text(entry.code, mark.kills, SPAWNS_AFTER))
                .size(14.0)
                .weak(),
        );
    }
}

#[derive(Clone, Copy)]
enum Reveal {
    Basic,
    Learned,
}

/// HPTX colour markup becomes egui text runs.
fn info(ui: &mut egui::Ui, value: &str) {
    let mut job = egui::text::LayoutJob::default();
    let base = ui.visuals().text_color();

    for span in regent_core::data::text::spans(value) {
        let color = span
            .colour
            .map_or(base, |[r, g, b]| egui::Color32::from_rgb(r, g, b));
        let mut format = egui::TextFormat::simple(egui::FontId::proportional(14.0), color);
        format.line_height = Some(16.0);
        job.append(&span.text.replace("\r\n", "\n"), 0.0, format);
    }

    job.wrap.max_width = ui.available_width();
    ui.label(job);
}

fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(7.0);
    ui.label(egui::RichText::new(title).size(15.0).strong().color(ACCENT));
}

fn stats(ui: &mut egui::Ui, entry: &Entry, reveal: Reveal) {
    let grid = match reveal {
        Reveal::Basic => "lore_basic",
        Reveal::Learned => "lore_learned",
    };

    egui::Grid::new(grid).num_columns(2).show(ui, |ui| {
        for (label, value) in &entry.stats {
            let basic = entry.basic(label);
            if matches!(reveal, Reveal::Basic) == basic {
                ui.label(
                    egui::RichText::new(entry.stat_label(label))
                        .size(14.0)
                        .weak(),
                );
                ui.label(
                    egui::RichText::new(entry.stat_text(label, *value))
                        .size(14.0)
                        .color(ACCENT),
                );
                ui.end_row();
            }
        }
    });
}

pub fn plugin(app: &mut App) {
    app.init_resource::<Book>().add_systems(
        Update,
        sync.after(crate::game::clock::tick)
            .in_set(frame::Frame::Step),
    );
}
