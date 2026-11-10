//! What a finished game leaves for the next on its dataset: the corpus
//! and the art made from it, not the game's state.
//!
//! ```text
//!  draw::uninstall ──> shelve: Corpus, TileArt, Lifts, ... (Kept::kept),
//!                      the game's source's art (art::Art::take,
//!                      art::Sprites::take)
//!  menu::begin ──> the maker takes the dataset's Corpus, else opens it
//!  draw::install ──> unshelve: the same, in place of fresh ones
//! ```
//!
//! Keyed by the corpus's archive files in load order: a record's art
//! depends on every archive before it (the last loaded wins), so art is
//! kept per dataset. The archives' bytes are shared further, per file
//! (`regent_draw::data::archives`), and GPL programs per image list
//! (`crate::game::load::images`).

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::path::PathBuf;

use bevy::prelude::*;
use regent_draw::data::Corpus;

/// A resource a game leaves for the next on its dataset.
pub trait Shelved: Resource {
    /// What outlasts the game: its art, without the game's state.
    #[must_use]
    fn kept(self) -> Self
    where
        Self: Sized,
    {
        self
    }
}

impl Shelved for Corpus {}

impl Shelved for regent_draw::terrain::TileArt {}

impl Shelved for regent_draw::art::ui::Entries {}

impl Shelved for regent_draw::art::world::Entries {}

/// Each dataset's kept resources, by type.
#[derive(Resource, Default)]
pub struct Shelf(HashMap<Vec<PathBuf>, HashMap<TypeId, Box<dyn Any + Send + Sync>>>);

impl Shelf {
    /// The `T` the dataset on `paths` left, if any.
    pub fn take<T: Shelved>(&mut self, paths: &[PathBuf]) -> Option<T> {
        self.0
            .get_mut(paths)?
            .remove(&TypeId::of::<T>())?
            .downcast()
            .ok()
            .map(|b| *b)
    }

    /// `kept` for the next game on the dataset on `paths`.
    pub fn put<T: Shelved>(&mut self, paths: &[PathBuf], kept: T) {
        self.0
            .entry(paths.to_vec())
            .or_default()
            .insert(TypeId::of::<T>(), Box::new(kept));
    }

    /// How many resources it holds: a test's measure.
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.0.values().map(HashMap::len).sum()
    }
}

/// Which way [`shift`] moves the kept resources.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Way {
    /// Out of the world, onto the shelf: a game ends.
    Off,
    /// Off the shelf, into the world: a game begins.
    On,
}

/// The art a game made, moved `way` under `paths`. Not the corpus nor the
/// ground: the game is made on those (`menu::begin`, `draw::install`).
pub fn shift(world: &mut World, paths: &[PathBuf], way: Way) {
    moved::<crate::map::pieces::Lifts>(world, paths, way);
    moved::<crate::map::hp_bars::Bars>(world, paths, way);
    moved::<crate::panel::Panels>(world, paths, way);
    moved::<crate::ui::minimap::Minimap>(world, paths, way);
    moved::<crate::audio::Mixer>(world, paths, way);
    interface(world, paths, way);
    sprites(world, paths, way);
}

/// The interface art the game's source holds in the art service, moved
/// `way` under `paths`; a source the menus draw from stays
/// (`art::Kept::Pinned`).
fn interface(world: &mut World, paths: &[PathBuf], way: Way) {
    let source = regent_draw::art::ArtSource::of(paths);

    match way {
        Way::Off => {
            let taken = world
                .get_resource_mut::<regent_draw::art::Art>()
                .map(|mut art| art.take(source));

            if let Some(entries) = taken {
                world.get_resource_or_init::<Shelf>().put(paths, entries);
            }
        }
        Way::On => {
            let kept = world
                .get_resource_mut::<Shelf>()
                .and_then(|mut shelf| shelf.take::<regent_draw::art::Entries>(paths));

            if let (Some(entries), Some(mut art)) =
                (kept, world.get_resource_mut::<regent_draw::art::Art>())
            {
                art.put(entries);
            }
        }
    }
}

/// The world's sprites the game's source holds in the art service, moved
/// `way` under `paths`, as [`interface`] moves its pictures.
fn sprites(world: &mut World, paths: &[PathBuf], way: Way) {
    let source = regent_draw::art::ArtSource::of(paths);

    match way {
        Way::Off => {
            let taken = world
                .get_resource_mut::<regent_draw::art::Sprites>()
                .map(|mut sprites| sprites.take(source));

            if let Some(entries) = taken {
                world.get_resource_or_init::<Shelf>().put(paths, entries);
            }
        }
        Way::On => {
            let kept = world
                .get_resource_mut::<Shelf>()
                .and_then(|mut shelf| shelf.take::<regent_draw::art::world::Entries>(paths));

            if let (Some(entries), Some(mut sprites)) =
                (kept, world.get_resource_mut::<regent_draw::art::Sprites>())
            {
                sprites.put(entries);
            }
        }
    }
}

fn moved<T: Shelved>(world: &mut World, paths: &[PathBuf], way: Way) {
    match way {
        Way::Off => shelve::<T>(world, paths),
        Way::On => unshelve::<T>(world, paths),
    }
}

/// `T` out of `world` onto the shelf under `paths`, what it keeps only.
pub fn shelve<T: Shelved>(world: &mut World, paths: &[PathBuf]) {
    if let Some(resource) = world.remove_resource::<T>() {
        world
            .get_resource_or_init::<Shelf>()
            .put(paths, resource.kept());
    }
}

/// `T` off the shelf under `paths` into `world`, if shelved.
pub fn unshelve<T: Shelved>(world: &mut World, paths: &[PathBuf]) {
    let kept = world
        .get_resource_mut::<Shelf>()
        .and_then(|mut shelf| shelf.take::<T>(paths));

    if let Some(kept) = kept {
        world.insert_resource(kept);
    }
}
