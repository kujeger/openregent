//! The interface's frame: the column, the bars and the panels drawn,
//! what they asked done, and the map given the rest.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::{EguiContexts, egui};
use egui::{LayerId, Ui, UiBuilder};

use regent_core::sim::game::Input;
use regent_core::view::panel::spell_bar;
use regent_core::view::panel::tracking::Target;
use regent_draw::data::Corpus;
use regent_draw::stage::{View, WorldCamera};

use crate::files::env::Install;
use crate::game::seen::{Interest, Watch};
use crate::game::sim::Session;
use crate::input;
use crate::input::orders;
use crate::map::cursor::Tipped;
use crate::map::mirror::Pieces;
use crate::menu::fit;
use crate::panel::canvas::Canvas;
use crate::panel::message::Message;
use crate::panel::upper::{Chosen, Game};
use crate::panel::{Panels, bars, tip};
use crate::ui::layout::Frame;
use crate::ui::options::Play;

use super::{
    BarMemo, Held, Interface, MINIMAP_AT, Upper, around, column_fit, column_press, covered,
    give_map, mini_view, minimap, opened, panels, query, reset, saved_game, scroll_game,
    spell_grid, spells_over, ticks,
};

/// Draw the column and bars, and give the map the rest: its rect and the
/// world camera's viewport.
///
/// # Errors
///
/// If the primary egui context is not ready yet.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
#[allow(
    clippy::too_many_arguments,
    reason = "a system's arguments are its dependencies"
)]
#[allow(
    clippy::too_many_lines,
    reason = "the bar's whole frame, a step a block"
)]
pub fn bar(
    mut contexts: EguiContexts,
    corpus: Res<Corpus>,
    (seen, session, mut interest): (Watch, Res<Session>, ResMut<Interest>),
    pieces: Pieces,
    camera: Single<&mut Camera, With<WorldCamera>>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut view: ResMut<View>,
    mut face: Interface,
    mut panel: orders::Panel,
    mut minimap: minimap::Parts,
    install: Res<Install>,
    mut kept: ResMut<BarMemo>,
) -> Result {
    let frame = Frame::of(egui::vec2(window.width(), window.height()));
    let buttons = spell_bar::shown(spell_bar::release(&corpus.catalogue));
    let rage = bars::rage_temple(&face.settings.enhancements, seen.world(), seen.local);

    // The open spell bar's rows lie over the map.
    let frame = frame.with_spells(if face.panels.state.spells_closed {
        0.0
    } else {
        spell_grid(&frame, buttons, rage).height()
    });
    let shown = face.selected.0.and_then(|agent| pieces.get(agent));
    let BarMemo { chosen, spells } = &mut *kept;
    let chosen = shown.map(|s| {
        chosen.get((seen.frames, s.agent), || {
            Chosen::of(&corpus, (seen.world(), seen.local), s)
        })
    });
    let message = face
        .reading
        .0
        .and_then(|h| Message::of(&corpus, seen.world(), h))
        .map(|m| m.following(face.tracking.mini.unwrap_or_default().messages));

    reset(&mut face.panels, chosen, seen.world(), seen.program);

    // The panel's interface run, answered as the next drawn frame
    // steps; nothing shows until then.
    let page = face.panels.state.page;

    interest.panel = chosen
        .as_ref()
        .and_then(|c| query(c, page, Upper::of(page, c, message.as_ref())));

    let answers = &seen.answers;
    let crypt = shown.map_or(&[][..], |s| answers.crypt(s.agent));
    let agent = chosen.map(|c| c.agent);
    let henchmen = agent.map_or([None; 3], |a| answers.henchmen(a));
    let extortion = agent.and_then(|a| answers.extortion(a));
    let bounty = agent.map_or([None; 2], |a| answers.bounties(a));

    let ctx = contexts.ctx_mut()?.clone();

    fit::styled(&ctx);
    tip::begin(&ctx);

    let mut ui = Ui::new(
        ctx.clone(),
        "interface".into(),
        UiBuilder::new()
            .layer_id(LayerId::background())
            .max_rect(ctx.viewport_rect()),
    );

    // An ended game takes no order (`end`), nor one under a menu.
    if seen.ended || face.panels.state.options.open.is_some() {
        ui.disable();
    }

    let player = face.player.profile.as_ref().map_or_else(
        || crate::files::profile::DEFAULT_NAME.to_owned(),
        |p| p.name.clone(),
    );
    let now: Vec<_> = pieces.iter().copied().collect();
    let game = Game {
        corpus: &corpus,
        frame: seen.frames,
        world: seen.world(),
        local: seen.local,
        program: seen.program,
        now: &now,
        crypt,
        player: &player,
        nicks: &seen.nicks,
        henchmen,
        extortion,
        bounty,
        rules: seen.rules,
        hd: &install.hd,
    };
    let text = &corpus.text;
    let catalogue = &corpus.catalogue;

    // The map's tooltip, offered before every panel's control. None under
    // a menu: **Inferred**, the options are `SMenuManager`'s modal
    // panel, which unit tip waits out.
    if face.panels.state.options.open.is_none() {
        let told = match *face.tipped {
            Tipped::Piece(agent) => regent_core::view::panel::tip::map_unit(
                catalogue,
                text,
                game.world,
                (agent, game.local),
            ),
            Tipped::Message(_) => regent_core::view::panel::tip::message_flag(catalogue),
            Tipped::Nothing => None,
        };

        tip::offer_as(&ctx, egui::Id::new(*face.tipped), told.as_deref());
    }

    let main = face
        .panels
        .sized((catalogue, text), crate::panel::MAIN, &install.hd);
    let source = corpus.source();

    face.art.set(face.settings.enhancements.ui_upscale);

    if let Some(main) = &main {
        let mut canvas = Canvas {
            ui: &mut ui,
            ink: &mut face.panels.ink,
            art: &mut face.art,
            source,
            catalogue,
            fit: column_fit(&frame, egui::Pos2::ZERO),
            salt: "column",
        };

        bars::column(&mut canvas, main);

        // The tracking window's row: a press filters, scans, or puts the
        // main view on what it shows.
        let mini = face.tracking.mini.unwrap_or_default();
        let row = bars::mini_row(&mut canvas, main, text, &mini);
        let ctrl = canvas.ui.input(|i| i.modifiers.ctrl);

        if let Some((column, checked)) = row {
            let live = regent_core::view::panel::tracking::Live {
                world: seen.world(),
                local: seen.local,
            };
            let jump = crate::ui::tracking::press(
                &mut face.tracking,
                &live,
                (column, checked, ctrl),
                face.selected.0,
            );

            if let Some(at) = jump {
                minimap::centre(&minimap, &mut view, at);
            }
        }

        let pressed = bars::column_buttons(&mut canvas, main, ticks(&face.settings, &view));

        column_press(
            pressed,
            &mut face,
            (seen.world(), seen.local, &mut panel, &mut view),
        );
    }

    let minimap_rect = egui::Rect::from_min_size(
        frame.in_column(MINIMAP_AT),
        egui::Vec2::splat(176.0 * frame.scale),
    );
    let mini = minimap::Game {
        corpus: &corpus,
        world: seen.world(),
        frame: &seen,
        highest: pieces.highest(),
        selected: face.selected.0,
        placing: crate::map::reach::Placing::of(&face.settings),
    };

    let stop = minimap::show(
        &mut ui,
        (minimap_rect.min, frame.scale),
        &mut minimap,
        (&mini, face.tracking.frame()),
        (&mut panel.orders, &mut view),
    );

    // A right press there: the tracking window stops there.
    if let Some(at) = stop {
        let live = regent_core::view::panel::tracking::Live {
            world: seen.world(),
            local: seen.local,
        };

        crate::ui::tracking::stop_at(&mut face.tracking, &live, at);
    }

    let (upper_asked, lower_asked) = panels(
        &mut ui,
        (&mut face.panels, (&mut face.art, source)),
        &frame,
        (&game, chosen, message.as_ref()),
        &mut panel,
    );

    face.portrait.0 = upper_asked.hovered;

    let named =
        crate::ui::tracking::caption(&corpus, (seen.world(), seen.frames), &mut face.tracking);

    mini_view(
        &mut ui,
        &frame,
        (&mut face.panels, (&mut face.art, source)),
        &corpus,
        (&mut face.tracking, &face.picture, named),
    );

    let named = scroll_game(&session);
    let pressed = around(
        &mut ui,
        (
            &mut face.panels,
            (&mut face.art, source),
            &frame,
            main.as_deref(),
        ),
        (
            &game,
            super::pause_line((face.paused.0, &seen.pausing), text),
            named,
            Held::of(&seen),
            rage,
        ),
        (&mut panel, spells),
    );

    // The tooltip line, once every control has offered its text.
    if let Some(main) = &main {
        let Panels { ink, state, .. } = &mut *face.panels;
        let mut canvas = Canvas {
            ui: &mut ui,
            ink,
            art: &mut face.art,
            source,
            catalogue,
            fit: bars::WINDOW,
            salt: "tip",
        };

        tip::show(&mut canvas, (main, &frame), &mut state.tip);
    }
    let goals = opened(&mut face, pressed, named, (text, install.goals()));

    // The save dialogs mark a save's mods not found: those found, once.
    if face.saves.found.is_empty() {
        face.saves.found_in(&install.mods());
    }

    let (chose, surrendered) = covered(
        &ctx,
        (&frame, &corpus),
        &mut face,
        spells_over(&frame),
        (&goals, saved_game(&session, text, (seen.now, seen.players))),
        (session.quest_key(), Play::of(seen.players), seen.seat),
    );

    face.saving.0 = chose;
    panel.orders.surrender |= surrendered == crate::ui::Surrendered::Yes;

    // A page's SELECT picks a hero; a zoom centres the view.
    if let Some(agent) = lower_asked.select {
        face.selected.0 = Some(agent);
        face.reading.0 = None;
    }

    if let (Some(on), Some(mini)) = (upper_asked.follows, &mut face.tracking.mini) {
        mini.messages = on;
    }

    if let Some(agent) = upper_asked.track.or(lower_asked.track) {
        let live = regent_core::view::panel::tracking::Live {
            world: seen.world(),
            local: seen.local,
        };

        crate::ui::tracking::follow(&mut face.tracking, &live, Target::Piece(agent));
    }

    if let Some(agent) = upper_asked.portrait {
        input::portrait(
            &mut panel.orders,
            (seen.world(), seen.program, seen.local),
            agent,
        );
    }

    if let Some(at) = upper_asked.zoom.or(lower_asked.zoom) {
        minimap::centre(&minimap, &mut view, at);
    }

    // The message scroll's CLOSE or zoom reads its flag, as a double
    // click does; the panel goes at once (**Inferred**).
    if let Some(handle) = upper_asked.read {
        panel.orders.outbox.push(Input::Read(handle));
        face.sounds
            .0
            .push(regent_core::view::panel::message::closed());
        face.reading.0 = None;
    }

    give_map(&frame, (&mut camera.into_inner(), &window), &mut view);

    Ok(())
}
