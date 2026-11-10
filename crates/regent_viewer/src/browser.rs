//! The egui panels: a unit list on the left, what it resolves to on the right.
//!
//! Same content as `regent_cli unit`.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::{EguiContexts, egui};
use egui::{LayerId, Ui, UiBuilder};

use regent_core::data::desc;

use regent_draw::data::{Corpus, ZOOM_MAX, ZOOM_MIN};
use regent_draw::stage::{self, View, WorldCamera};
use regent_draw::terrain::TileArt;

use crate::sprites::{Legends, STEPS, StageShown};
use crate::tiles::{self, Showing, Subject, Tiles, letter};
use crate::units::{
    Entry, Filter, KINDS, Pick, Playback, Rows, SPREAD_MAX, SPREAD_MIN, Selection, UnitList,
    purpose,
};
use regent_core::content::imagery::Animation;
use regent_core::data::fourcc::FourCc;

/// Draw both panels.
///
/// egui 0.36 panels attach to a background `Ui`, not a `Context`.
///
/// # Errors
///
/// If the primary egui context is not ready yet.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value; that is the contract"
)]
#[allow(
    clippy::too_many_arguments,
    reason = "a system's arguments are its dependencies, declared rather than fetched"
)]
pub fn panels(
    mut contexts: EguiContexts,
    (corpus, rows): (Res<Corpus>, Res<Rows>),
    shown: Res<StageShown>,
    (mut selection, mut playback): (ResMut<Selection>, ResMut<Playback>),
    (ground, mut tiles): (Res<TileArt>, ResMut<Tiles>),
    terrain_shown: Res<regent_draw::terrain::MapShown>,
    legends: Res<Legends>,
    subject: Res<State<Subject>>,
    mut next: ResMut<NextState<Subject>>,
    camera: Single<&mut Camera, With<WorldCamera>>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut view: ResMut<View>,
) -> Result {
    let at = *subject.get();
    let ctx = contexts.ctx_mut()?.clone();
    let mut background = Ui::new(
        ctx.clone(),
        "viewport".into(),
        UiBuilder::new()
            .layer_id(LayerId::background())
            .max_rect(ctx.viewport_rect()),
    );

    let top = egui::Panel::top("header")
        .show(&mut background, |ui| {
            header(ui, at, (&corpus, &rows), &ground, &mut next);
        })
        .response
        .rect
        .height();

    let left = egui::Panel::left("list")
        .resizable(true)
        .default_size(320.0)
        .show(&mut background, |ui| match at {
            Subject::Units => unit_list(ui, (&corpus, &rows), &mut selection),
            Subject::Tileset => set_list(ui, (&ground, &mut tiles), &mut view),
        })
        .response
        .rect
        .width();

    let right = egui::Panel::right(detail_id(at))
        .resizable(true)
        .default_size(detail_width(&ctx))
        .show(&mut background, |ui| match at {
            Subject::Units => detail(ui, (&corpus, &rows), &mut selection),
            Subject::Tileset => tileset_detail(ui, &ground),
        })
        .response
        .rect
        .width();

    let bottom = egui::Panel::bottom("controls")
        .resizable(true)
        .show(&mut background, |ui| {
            match at {
                Subject::Units => frame_controls(ui, &mut selection, &mut playback),
                Subject::Tileset => {}
            }

            ui.horizontal(|ui| {
                ui.label("zoom");
                ui.add(egui::Slider::new(&mut view.zoom, ZOOM_MIN..=ZOOM_MAX).logarithmic(true));

                for (label, zoom) in STEPS {
                    if ui.button(label).clicked() {
                        view.zoom = zoom;
                        view.pan = Vec2::ZERO;
                    }
                }

                ui.separator();
                ui.color_edit_button_srgb(&mut view.backdrop).on_hover_text(
                    "What is behind the picture, and the clear colour. In the unit browser \
                     it is also what a shadow falls on, since a sprite on its own has no \
                     ground — the default is dirt measured out of the game.",
                );

                if ui.button("dirt").clicked() {
                    view.backdrop = stage::DIRT;
                }

                ui.separator();
                ui.weak("middle-drag or shift-drag to pan, wheel to zoom at the pointer");
            });

            let status = match at {
                Subject::Units => &shown.status,
                Subject::Tileset => &terrain_shown.status,
            };

            ui.label(egui::RichText::new(status).monospace());
        })
        .response
        .rect
        .height();

    stage::viewport(
        &mut camera.into_inner(),
        &window,
        [left, right, top, bottom],
    );

    // In logical points, to match `Window::cursor_position`.
    view.rect = Some(Rect {
        min: Vec2::new(left, top),
        max: Vec2::new(window.width() - right, window.height() - bottom),
    });

    // After writing `rect`, so legends do not lag a resize.
    paint_legends(&ctx, &view, &legends);

    Ok(())
}

/// The widest monospace row the detail panel draws, in characters.
///
/// The art layer's row. A property row is 50, an animation set 56.
const WIDEST_ROW: f32 = 74.0;

/// Points the panel needs beyond its text: two margins and a scroll bar.
const PANEL_FURNITURE: f32 = 36.0;

/// How wide the detail panel opens, in points.
///
/// Characters times the font's glyph width, so theme and scale cannot break it.
fn detail_width(ctx: &egui::Context) -> f32 {
    let font = ctx.style_of(ctx.theme()).text_styles[&egui::TextStyle::Monospace].clone();
    let column = ctx.fonts_mut(|fonts| fonts.glyph_width(&font, '0'));
    WIDEST_ROW * column + PANEL_FURNITURE
}

/// The detail panel's id, one per subject.
///
/// egui stores width per id; a shared id would share the width.
const fn detail_id(at: Subject) -> &'static str {
    match at {
        Subject::Units => "detail-units",
        Subject::Tileset => "detail-tileset",
    }
}

/// Draw the world-anchored words, at a fixed size, over the picture.
///
/// Drawn dark then light, readable on any art.
fn paint_legends(ctx: &egui::Context, view: &View, legends: &Legends) {
    let Some(rect) = view.rect else { return };

    let mut painter = ctx.layer_painter(LayerId::new(egui::Order::Middle, "legends".into()));

    // Clip to the picture region.
    painter.set_clip_rect(egui::Rect::from_min_max(
        egui::pos2(rect.min.x, rect.min.y),
        egui::pos2(rect.max.x, rect.max.y),
    ));

    for legend in &legends.0 {
        let Some(at) = view.screen_at(legend.at) else {
            continue;
        };

        let font = egui::FontId::proportional(legend.size);

        for (offset, colour) in [
            (1.0, egui::Color32::from_black_alpha(200)),
            (0.0, egui::Color32::WHITE),
        ] {
            painter.text(
                egui::pos2(at.x + offset, at.y + offset),
                egui::Align2::CENTER_CENTER,
                &legend.text,
                font.clone(),
                colour,
            );
        }
    }
}

/// The subject selector, and one line saying what is loaded.
///
/// Subjects without data are disabled.
fn header(
    ui: &mut Ui,
    at: Subject,
    (corpus, rows): (&Corpus, &Rows),
    ground: &TileArt,
    next: &mut NextState<Subject>,
) {
    ui.horizontal(|ui| {
        ui.strong("OpenRegent");
        ui.separator();

        for (subject, name) in Subject::ALL {
            let ready = match subject {
                Subject::Units => true,
                Subject::Tileset => ground.ready(),
            };

            if ui
                .add_enabled(ready, egui::Button::selectable(at == subject, name))
                .clicked()
            {
                next.set(subject);
            }
        }

        ui.separator();
        ui.label(match at {
            Subject::Units => rows.summary(corpus),
            Subject::Tileset => ground.summary(),
        });
    });
}

/// The filter row and the scrolling list.
fn unit_list(ui: &mut Ui, (corpus, rows): (&Corpus, &Rows), selection: &mut Selection) {
    ui.horizontal(|ui| {
        ui.label("filter");
        ui.text_edit_singleline(&mut selection.search);
    });

    ui.horizontal_wrapped(|ui| {
        let filters = std::iter::once((Filter::All, "all"))
            .chain(KINDS.map(|(kind, label)| (Filter::Kind(kind), label)))
            .chain(std::iter::once((Filter::Interface, "Interface")));

        for (filter, label) in filters {
            let button = ui.selectable_label(selection.filter == filter, label);
            let button = if filter == Filter::Interface {
                button.on_hover_text(
                    "IMAG records of the interface archives, and the four selection \
                     rings. A unit's portrait and hero backdrop are in its own sets.",
                )
            } else {
                button
            };

            if button.clicked() {
                selection.filter = filter;
            }
        }
    });

    ui.separator();

    // Collected first, so the click can borrow `selection` mutably.
    let visible = shown(rows, selection.filter, &selection.search);

    ui.label(format!(
        "{} of {}",
        visible.len(),
        rows.units.len() + rows.pictures.len()
    ));

    egui::ScrollArea::vertical().show(ui, |ui| {
        for pick in visible {
            let Some(entry) = rows.entry(pick) else {
                continue;
            };

            if ui
                .selectable_label(
                    selection.picked == Some(pick),
                    egui::RichText::new(list_row(entry)).monospace(),
                )
                .clicked()
            {
                selection.select((corpus, rows), pick);
            }
        }
    });
}

/// Every row passing the filters: units first, then pictures.
fn shown(rows: &Rows, filter: Filter, search: &str) -> Vec<Pick> {
    let picks = |entries: &[Entry], source| {
        entries
            .iter()
            .enumerate()
            .filter(|(_, e)| e.matches(filter, search))
            .map(|(index, _)| Pick { source, index })
            .collect::<Vec<_>>()
    };

    [
        picks(&rows.units, UnitList::Unit),
        picks(&rows.pictures, UnitList::Picture),
    ]
    .concat()
}

/// One list line. `!` marks a unit with no art; a picture shows its
/// archive and known uses.
fn list_row(entry: &Entry) -> String {
    let counts = format!(
        "{}  {:<22} {:>2} sets {:>5} frames",
        entry.id, entry.name, entry.sets, entry.frames
    );

    match entry.source {
        UnitList::Unit if entry.art.is_some() => format!(" {counts}"),
        UnitList::Unit => format!("!{counts}"),
        UnitList::Picture => format!(
            " {counts}  {}  {}",
            entry.art.as_deref().unwrap_or_default(),
            entry.uses.join("; ")
        ),
    }
}

/// A picture's record, archive, uses and sets.
fn picture_detail(ui: &mut Ui, rows: &Rows, selection: &mut Selection) {
    let Some(entry) = selection.picked.and_then(|pick| rows.entry(pick)) else {
        ui.label("Select a unit or a picture.");
        return;
    };

    let mut wanted = None;

    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.heading(format!("{} {}", entry.id, entry.name));
        ui.label(format!(
            "IMAG in {}",
            entry.art.as_deref().unwrap_or_default()
        ));

        for said in &entry.uses {
            ui.label(format!("drawn as  {said}"));
        }

        ui.separator();
        wanted = sets(ui, selection.sets(), UnitList::Picture, entry.id);
    });

    if let Some(set) = wanted {
        selection.show(set);
    }
}

/// Everything the selected unit names, or the selected picture.
fn detail(ui: &mut Ui, (corpus, rows): (&Corpus, &Rows), selection: &mut Selection) {
    let Some(unit) = selection.resolved.as_ref() else {
        picture_detail(ui, rows, selection);
        return;
    };

    let mut wanted = None;

    egui::ScrollArea::vertical().show(ui, |ui| {
        // Display name, then internal name.
        let title = unit.desc.head.title();
        let internal = String::from_utf8_lossy(unit.desc.head.name);

        ui.heading(format!("{} {title}", unit.id));

        if internal != title {
            ui.label(egui::RichText::new(internal).weak());
        }

        let kind = unit
            .kind
            .map_or_else(|| "?".to_owned(), |k| format!("{k:?}"));
        ui.label(format!("subtype  {kind}"));

        let flags: Vec<&str> = unit
            .desc
            .engine()
            .info_flags()
            .map(desc::Info::name)
            .collect();
        if !flags.is_empty() {
            ui.label(format!("info     {}", flags.join(" ")));
        }

        stats(ui, unit);
        links(ui, unit);
        wanted = panel_art(ui, unit);
        described(ui, corpus, unit);

        if !unit.animations.is_empty() {
            ui.separator();
            wanted = wanted.or(sets(ui, &unit.animations, UnitList::Unit, unit.id));
        }
    });

    if let Some(set) = wanted {
        selection.show(set);
    }
}

/// Buttons for the unit's selection panel pictures: portrait (set 1000),
/// hero backdrop (1001), list icon (1002). Returns the set clicked.
fn panel_art(ui: &mut Ui, unit: &regent_core::content::unit::Unit<'_>) -> Option<usize> {
    let found: Vec<(usize, &str)> = unit
        .animations
        .iter()
        .enumerate()
        .filter_map(|(i, set)| purpose(UnitList::Unit, unit.id, set.id).map(|said| (i, said)))
        .collect();

    let mut clicked = None;

    if !found.is_empty() {
        ui.separator();

        ui.horizontal_wrapped(|ui| {
            ui.label("panel art");

            for (i, said) in found {
                if ui.button(said).clicked() {
                    clicked = Some(i);
                }
            }
        });
    }

    clicked
}

/// What the game itself says about this unit, where it says anything.
///
/// `HPTX`: help topic by `HelpID` (142 units and buildings have one).
/// `DPTX`: a building's requirements, as the build menu words them.
fn described(ui: &mut Ui, corpus: &Corpus, unit: &regent_core::content::unit::Unit<'_>) {
    let help = unit
        .desc
        .character()
        .and_then(|c| c.help_id())
        .or_else(|| unit.desc.building().and_then(|b| b.help_id()))
        .and_then(|id| corpus.text.help(id));

    let needs = corpus.text.prerequisite(unit.id);

    if help.is_none() && needs.is_none() {
        return;
    }

    ui.separator();

    if let Some(needs) = needs {
        ui.label(egui::RichText::new(format!("needs  {}", needs.trim())).monospace());
    }

    // The game's only markup: a colour, for help headings.
    if let Some(help) = help {
        for span in regent_core::data::text::spans(&help) {
            if span.text.trim().is_empty() {
                continue;
            }

            let mut line = egui::RichText::new(span.text.trim_end());

            if let Some([r, g, b]) = span.colour {
                line = line.color(egui::Color32::from_rgb(r, g, b));
            }

            ui.label(line);
        }
    }
}

/// One `name  value` line, skipped when the field is absent.
fn row(ui: &mut Ui, name: &str, v: Option<u32>) {
    if let Some(v) = v {
        ui.label(format!("{name:<9}{v}"));
    }
}

/// The `Character` or `Building` block, whichever the subtype gives.
fn stats(ui: &mut Ui, unit: &regent_core::content::unit::Unit<'_>) {
    if let Some(c) = unit.desc.character() {
        ui.separator();
        row(ui, "hp", c.max_hp());
        row(ui, "cost", c.cost());
        row(ui, "attack", c.attack());
        row(ui, "parry", c.parry());
        row(ui, "dodge", c.dodge());
        row(ui, "speed", c.speed());
        row(ui, "sight", c.sight_range());
        row(ui, "xp", c.experience());
    } else if let Some(b) = unit.desc.building() {
        ui.separator();
        row(ui, "hp", b.max_hp());
        row(ui, "cost", b.cost());
        row(ui, "sight", b.sight_range());
        row(ui, "guild", b.max_guild_members());
    }
}

/// One reference. A dangling link prints its id.
fn link(ui: &mut Ui, name: &str, l: Option<regent_core::content::unit::Link>) {
    if let Some(l) = l {
        let mark = if l.resolved() {
            ""
        } else {
            "  -- not found --"
        };
        ui.label(format!("{name:<9}{}{mark}", l.id));
    }
}

/// The ids this unit points at, and whether they landed.
fn links(ui: &mut Ui, unit: &regent_core::content::unit::Unit<'_>) {
    ui.separator();
    link(ui, "art", unit.art);
    link(ui, "movement", unit.movement);
    link(ui, "sound", unit.sound);

    for (action, sound) in &unit.overrides {
        ui.label(format!("override {} on {}", sound.id, action.id));
    }
}

/// One row per animation set, with its use where known. Returns the set
/// clicked.
fn sets(ui: &mut Ui, animations: &[Animation], source: UnitList, id: FourCc) -> Option<usize> {
    let mut clicked = None;

    for (i, set) in animations.iter().enumerate() {
        let per = set.poses.first().map_or(0, |p| p.per_stream);
        let streams = set.poses.first().map_or(0, |p| p.streams);
        let total: usize = set.poses.iter().map(|p| p.frames.len()).sum();
        let said = purpose(source, id, set.id).unwrap_or_default();

        // Raw id: sets of one SetKind can print identically otherwise.
        let row = egui::RichText::new(format!(
            "{:<14} L{}  {:#06x}  {:>2} poses  {per:>3}x{streams}  {total:>4} frames  {said}",
            set.kind.to_string(),
            set.variant,
            regent_core::data::imag::set_number(set.id),
            set.poses.len(),
        ))
        .monospace();

        if ui
            .selectable_label(false, row)
            .on_hover_text("Show this set")
            .clicked()
        {
            clicked = Some(i);
        }
    }

    clicked
}

/// Which set, which pose, which stream, which frame.
///
/// Sliders, not a compass rose: poses are not always facings (eight on a
/// `Walk` are, ten on a rock are variants; one record has 32). The ring is
/// opt-in. See `docs/formats/art.md`.
fn frame_controls(ui: &mut Ui, selection: &mut Selection, playback: &mut Playback) {
    if selection.picked.is_none() {
        ui.label("Select a unit.");
        return;
    }

    let animations = selection.sets();
    let sets = animations.len();
    if sets == 0 {
        ui.label("This has no animations.");
        return;
    }

    // Read out before the mutable borrows below.
    let set = selection.set.min(sets - 1);
    let label = animations[set].kind.to_string();
    let poses = animations[set].poses.len();
    let streams = animations[set].poses.first().map_or(1, |p| p.streams);
    let frames = selection.frames();

    selection.set = set;

    ui.horizontal(|ui| {
        ui.label("set");
        if ui
            .add(egui::Slider::new(&mut selection.set, 0..=sets - 1))
            .changed()
        {
            selection.pose = 0;
            selection.stream = 0;
            selection.frame = 0;
        }
        ui.label(label);
    });

    ui.horizontal(|ui| {
        ui.label("pose");
        ui.add_enabled(
            !selection.ring && poses > 1,
            egui::Slider::new(&mut selection.pose, 0..=poses.saturating_sub(1)),
        );
        ui.label(format!("of {poses}"));

        if streams > 1 {
            ui.separator();
            ui.label("stream");
            ui.add(egui::Slider::new(&mut selection.stream, 0..=streams - 1));
        }

        ui.separator();
        ui.add_enabled(poses > 1, egui::Checkbox::new(&mut selection.ring, "ring"))
            .on_hover_text(
                "Every pose at once, slot 0 up and the rest clockwise. \
                 The labels say which slot is which, so if the art disagrees \
                 with that order you can read the real one off it.",
            );
        ui.add_enabled(
            selection.ring,
            egui::Slider::new(&mut selection.spread, SPREAD_MIN..=SPREAD_MAX).text("spread"),
        );
    });

    ui.horizontal(|ui| {
        ui.label("frame");
        ui.add_enabled(
            frames > 1,
            egui::Slider::new(&mut selection.frame, 0..=frames.saturating_sub(1)),
        );
        ui.label(format!("of {frames}"));

        ui.separator();
        play(ui, selection, playback, frames);

        ui.separator();
        ui.checkbox(&mut selection.offset, "pose offset");

        ui.separator();
        ui.checkbox(&mut selection.anchor, "anchor").on_hover_text(
            "Where the tile says the thing stands, at +0x0a. It sits under a \
             hero's feet; the four characters drawn in the air — Harpy, Giant \
             Eagle, Black Phantom, Evil Oculus — leave a gap above it, and \
             that gap is how high they are flying.",
        );
    });

    selection.pose = selection.pose.min(poses.saturating_sub(1));

    attachment_controls(ui, selection);
}

/// Run the frames, or hold one still.
///
/// A frame shows [`Selection::frame_ms`].
fn play(ui: &mut Ui, selection: &mut Selection, playback: &mut Playback, frames: usize) {
    let mark = if playback.playing { "⏸" } else { "▶" };

    if ui
        .add_enabled(frames > 1, egui::Button::new(mark))
        .on_hover_text(format!(
            "{} ms a frame: the set's playback, as the game plays it.",
            selection.frame_ms()
        ))
        .clicked()
    {
        playback.playing = !playback.playing;

        // Rewind if at the end.
        if playback.playing && selection.frame + 1 >= frames {
            selection.frame = 0;
        }

        playback.since = 0.0;
    }

    ui.checkbox(&mut playback.looping, "loop");
}

/// The attachment marks.
fn attachment_controls(ui: &mut Ui, selection: &mut Selection) {
    let blocks: usize = selection
        .animation()
        .map_or(0, |a| a.poses.iter().map(|p| p.attachments.len()).sum());

    ui.horizontal(|ui| {
        ui.add_enabled(
            blocks > 0,
            egui::Checkbox::new(&mut selection.points, "attachment points"),
        )
        .on_hover_text(
            "Two marks per attachment block, its two points: red bit 0, green \
             bit 1, blue bit 2. A slot can move these while drawing the same \
             picture — that is how a Ballista Tower fires in eight directions \
             without turning.",
        );
        ui.label(format!("{blocks} in this set"));
    });
}

/// The tileset's sets, as a list that filters the stage down to one.
///
/// A change requests a reframe.
fn set_list(ui: &mut Ui, (ground, tiles): (&TileArt, &mut Tiles), view: &mut View) {
    let mut want = tiles.set;

    ui.horizontal(|ui| {
        if ui
            .selectable_label(tiles.set == Showing::All, "all sets")
            .clicked()
        {
            want = Showing::All;
        }
    });

    ui.separator();

    egui::ScrollArea::vertical().show(ui, |ui| {
        for (i, name) in tiles::sets(ground).into_iter().enumerate() {
            if ui
                .selectable_label(
                    tiles.set == Showing::One(i),
                    egui::RichText::new(name).monospace(),
                )
                .clicked()
            {
                want = Showing::One(i);
            }
        }
    });

    if want != tiles.set {
        tiles.set = want;
        view.refit = true;
    }
}

/// What the tileset holds, in the same terms `regent_cli terrain` prints.
fn tileset_detail(ui: &mut Ui, ground: &TileArt) {
    let Some(tileset) = &ground.tileset else {
        ui.label("No tileset in this corpus.");
        return;
    };

    ui.heading(tileset.id.to_string());
    ui.separator();

    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.label(
            "A body set is drawn as its whole frames-by-streams table, so the \
             null markers show as gaps. Three quarters of that grid is nothing, \
             and what the full columns are has not been decided.",
        );
        ui.separator();

        for body in &tileset.bodies {
            ui.label(
                egui::RichText::new(format!(
                    "{0}to{0}  {1:<16} {2:>3} x {3}  {4:>3} variants  {5} groups",
                    letter(body.letter),
                    ground.name(body.letter),
                    body.frames,
                    body.streams,
                    body.variants.len(),
                    body.groups.len(),
                ))
                .monospace(),
            );
        }

        ui.separator();

        for blend in &tileset.blends {
            let variants: Vec<usize> = blend.cases.iter().map(|c| c.variants.len()).collect();

            ui.label(
                egui::RichText::new(format!(
                    "{}to{}  {:>2} cases  {:?}",
                    letter(blend.from),
                    letter(blend.to),
                    blend.cases.len(),
                    variants,
                ))
                .monospace(),
            );
        }
    });
}
