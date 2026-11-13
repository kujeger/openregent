//! A drawn frame of the game: its systems in named sets, in order, while
//! [`Screen::Playing`](crate::menu::Screen::Playing).
//!
//! ```text
//!  PreUpdate (after Bevy's input)
//!    Keys::Chat     the chat line takes the keys first        chat
//!    Keys::Hot      the panels' hot keys                      hotkey
//!  Update
//!    Frame::Input   zoom, pan, the keys (pause, mute, speed, gold, fog, the map's), the
//!                   edge's scroll, orders
//!                   and the outbox sent; the preview asked (seen::Interest)
//!    Frame::Step    the sim thread's newest state ──> Snapshot, Reel, seen::Frame;
//!                   a LAN resync's world drawn as a new game's (resynced);
//!                   the pieces mirrored (mirror);
//!                   the ground relaid, the fog taken, a quake; a gone selection forgotten;
//!                   what the tracking window follows (tracking)
//!    Frame::Draw    map, ground and scatter, its fog; sprite pages landed;
//!                   pieces, missiles, particles
//!    Frame::Pick    a click picks or orders, on the map or the tracking window; the
//!                   preview and reach; the cursor
//!    Frame::Over    effects, their fog and tints, trails; rings, bars, names
//!    Frame::View    the opening view framed once; fit, snap, bound, aim; the
//!                   tracking window aimed, its ground
//!    Frame::End     the game's end: recorded, its curtain (unordered with the above)
//!  EguiPrimaryContextPass
//!    Ui::Bar        the column and bars, map's rect; a save   ui
//!    Ui::Chat       the chat line                             chat
//!    Ui::Over       the end's box and panels; the cursor's amount
//!  PostUpdate       the view bounded again, after the panels moved it (a minimap
//!                   drag), before the cameras take it; sound and music (audio)
//! ```
//!
//! Under a new game's loading page (`menu::drape`) the keys, the input's
//! zoom, pan and keys, and the picks do not run: the rest draws the game.
//!
//! Each step reads what the one before made: input reaches the game
//! before it steps, the drawing reads the step's snapshot, a pick reads
//! the drawn pieces. Within a step the order is fixed too, so two runs
//! draw alike.

use bevy::ecs::schedule::ScheduleConfigs;
use bevy::ecs::system::ScheduleSystem;
use bevy::prelude::*;
use bevy_egui::EguiPrimaryContextPass;

use regent_draw::map;
use regent_draw::stage;

use crate::menu::{in_game, undraped};
use crate::{
    draw, draw::resynced, input, input::hotkey, input::map_keys, input::orders, map::cursor,
    map::effects, map::fog, map::ground, map::hp_bars, map::labels, map::mirror, map::pan,
    map::particles, map::preview, map::quake, map::reach, map::select, map::trail, map::zoom, perf,
    ui, ui::chat, ui::end, ui::held, ui::stopped, ui::tracking,
};

/// The game's steps in `Update`, in order.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Frame {
    Input,
    Step,
    Draw,
    Pick,
    Over,
    View,
    /// The game's end: unordered with the steps.
    End,
}

/// The keys in `PreUpdate`, in order.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Keys {
    Chat,
    Hot,
}

/// The interface in the egui pass, in order.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Ui {
    Bar,
    Chat,
    Over,
}

/// The sets, and the frame's systems in them.
pub fn plugin(app: &mut App) {
    app.configure_sets(
        PreUpdate,
        (Keys::Chat, Keys::Hot)
            .chain()
            .after(bevy::input::InputSystems)
            .run_if(in_game)
            .run_if(undraped),
    )
    .configure_sets(
        Update,
        (
            Frame::Input,
            Frame::Step,
            Frame::Draw,
            Frame::Pick,
            Frame::Over,
            Frame::View,
        )
            .chain()
            .run_if(in_game),
    )
    .configure_sets(Update, Frame::End.run_if(in_game))
    .configure_sets(
        EguiPrimaryContextPass,
        (Ui::Bar, Ui::Chat, Ui::Over).chain().run_if(in_game),
    )
    .add_systems(PreUpdate, chat::keys.in_set(Keys::Chat))
    .add_systems(PreUpdate, hotkey::press.in_set(Keys::Hot))
    .add_systems(
        EguiPrimaryContextPass,
        (ui::bar, ui::keep).chain().in_set(Ui::Bar),
    )
    .add_systems(EguiPrimaryContextPass, chat::line.in_set(Ui::Chat))
    .add_systems(
        EguiPrimaryContextPass,
        (
            end::panel,
            held::shown,
            stopped::shown,
            cursor::amount,
            crate::ui::players::shown,
        )
            .in_set(Ui::Over),
    )
    .add_systems(
        PostUpdate,
        stage::bound
            .after(bevy_egui::EguiPostUpdateSet::EndPass)
            .before(regent_draw::present::Present)
            .run_if(in_game),
    );

    update(app);
}

/// The game's keys in `Frame::Input`: pause, mute, speed, full screen,
/// fog, casting, bars, the map's; the edge's scroll.
fn keys() -> ScheduleConfigs<ScheduleSystem> {
    (
        ui::pause_key,
        ui::mute_key,
        crate::game::speed::speed_key,
        crate::draw::fullscreen::game_key,
        fog::key,
        crate::map::cast::key,
        hp_bars::reveal_key,
        // No edge in a screenshot: its headless pointer may rest at one.
        (
            map_keys::keys,
            pan::scroll.run_if(not(resource_exists::<crate::draw::shot::Shot>)),
        )
            .chain(),
    )
        .into_configs()
}

/// The frame's systems in `Update`, step by step.
fn update(app: &mut App) {
    app.add_systems(
        Update,
        (
            (
                zoom::systems().run_if(undraped),
                stage::pan.run_if(ui::over_map).run_if(undraped),
                (keys().run_if(undraped), crate::game::speed::apply),
                input::send,
                input::want,
            )
                .chain()
                .in_set(Frame::Input),
            (
                crate::game::clock::tick,
                resynced::redraw.run_if(resynced::moved),
                mirror::sync,
                ground::relay,
                fog::capture,
                quake::shake,
                crate::ui::frame_rate::step,
                select::forget,
                input::choose,
                tracking::step,
            )
                .chain()
                .in_set(Frame::Step),
            (
                mirror::check.run_if(mirror::checking),
                map::reel::step,
                effects::speak,
                perf::ground_began,
                map::redraw::redraw::<regent_draw::art::Sprites>,
                perf::ground_ended,
                fog::ground,
                fog::scatter,
                regent_draw::art::world::land,
                crate::map::pieces::pieces,
                crate::map::cast::casting,
                crate::map::pieces::missiles,
                particles::motes,
            )
                .chain()
                .in_set(Frame::Draw),
            (
                (
                    input::press,
                    select::pick.run_if(orders::idle),
                    input::cancel,
                    input::click,
                    input::aim,
                )
                    .chain()
                    .run_if(undraped),
                preview::listed,
                preview::follow,
                reach::follow,
                cursor::point.run_if(undraped),
            )
                .chain()
                .in_set(Frame::Pick),
            (
                effects::capture,
                effects::draw,
                fog::units,
                effects::tint,
                trail::draw,
                trail::paint,
                crate::map::tie::untie,
                select::follow,
                hp_bars::follow,
                labels::follow,
            )
                .chain()
                .in_set(Frame::Over),
            (
                draw::frame_start,
                stage::fit,
                zoom::snap.run_if(zoom::smooth),
                stage::bound,
                stage::aim,
                tracking::aim,
                // After the aim: a far jump's window has ground at once.
                perf::ground_began,
                map::redraw::redraw_inset,
                perf::ground_ended,
                perf::ground_noted,
            )
                .chain()
                .in_set(Frame::View),
            (end::record, end::curtain).in_set(Frame::End),
        ),
    );
}
