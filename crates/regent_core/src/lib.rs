//! Reverse-engineering support for Majesty: The Fantasy Kingdom Sim (Cyberlore,
//! 2000).
//!
//! Target: Windows Gold HD.
//!
//! Seven layers, each using only those below it; `tests/layers.rs` holds
//! the order.
//!
//! ```text
//!  view     what the screen shows of data and sim: sprites, terrain,
//!    │      panels, minimap, fog, sound
//!  sim      the game engine: world, movement, placement, the rules it
//!    │      runs in play, its number units; the game driver, its input
//!    │      and the live host that answers GPL's builtins
//!  gpl      the script language: lexer, parser, compiler, loader, VM;
//!    │      knows nothing of the world
//!  save     openregent's save model, plain data
//!    │
//!  content  an install found and joined: where the files are, datasets,
//!    │      quests, mods, a unit and its art; the terrain model (board,
//!    │      classes, vertex ground); the project's own TOML
//!  data     shipped files read strictly: formats, the value types they
//!    │      store, the record index, text, GPL's value encodings
//!  base     engine-wide primitives: the RNG, fixed-order maps and the
//!           state digest, the fixed-point types
//! ```
//!
//! - [`base`]: no format and no game knowledge; uses nothing else here.
//! - [`data`]: an unexpected value is an error, because a hard failure is a
//!   finding.
//! - [`content`]: finds an install's files and joins their records;
//!   [`content::terrain`] holds the terrain model the sim paints.
//! - [`gpl`]: input is hand-written, so it collects and recovers from
//!   errors.
//! - [`save`]: below the simulation, so each type's conversion sits beside
//!   its private fields.
//!
//! Split by domain, not binary versus text: `.qdd` (text) is [`data`], `.dat`
//! (text) is [`gpl`]. `.bcd` block framing is a generic container, so [`data`]
//! (saves' GPL chunk has the same block shape); the prototypes and bytecode
//! inside are [`gpl`].
//!
//! Compiling needs no game assets. Integration tests do, and fail rather than
//! skip without the install: a suite that passes without a game file proves
//! nothing.
//!
//! # When to split this crate
//!
//! The layers are acyclic, so each could be a crate. [`gpl`] uses `logos`
//! and `ariadne`; [`base`], [`data`], [`content`], [`save`], [`sim`] and
//! [`view`] use `serde`, and [`content`] `toml`. `regent_draw` uses only
//! [`base`], [`data`], [`content`], [`sim`] and [`view`], yet builds all
//! seven. Split if that
//! cost starts to matter.

pub mod base;
pub mod content;
pub mod data;
pub mod gpl;
pub mod save;
pub mod sim;
pub mod view;

#[cfg(test)]
mod tests {
    /// Overflow panics in release too (`Cargo.toml`): a wrap in the
    /// sim would corrupt play unseen. `cargo test --release` checks it.
    #[test]
    fn overflow_panics() {
        let sum = std::panic::catch_unwind(|| std::hint::black_box(u8::MAX) + 1);

        assert!(sum.is_err());
    }
}
