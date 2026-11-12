//! What openregent reads and keeps on disk: the install, settings,
//! profiles, mods, the log, the high scores, freestyle presets, build
//! trees and the lore catalogue (`store` the shared file handling). A
//! leaf: nothing here uses the rest of the crate.

pub(crate) mod builtin;
pub(crate) mod compiled;
pub(crate) mod config;
pub(crate) mod env;
pub(crate) mod log;
pub(crate) mod lore;
pub(crate) mod mod_hash;
pub(crate) mod mod_list;
pub(crate) mod presets;
pub(crate) mod profile;
pub(crate) mod scores;
pub(crate) mod store;
pub(crate) mod trees;
