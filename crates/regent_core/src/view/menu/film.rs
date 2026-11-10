//! The films of openregent's Cinematics page, and of a campaign's end.
//!
//! ```text
//!  Opening  MV04  always           the manual's "opening movie"
//!  North    MV07  slots 19 to 30   end::ENDING, as EndGameController      asks it
//! ```
//!
//! No southern film: no archive holds one, and no binary read plays one
//! (HD, the Linux port's `majesty`). A southern win plays nothing.
//!
//! The list is openregent's: the original's CINEMATIC plays `MV04`, then
//! `MV07` ([`super::CINEMATIC`]). Base and Gold ship no third film: their
//! `cinedata2.dat`, `cinedata3.dat` hold `MV02`, `MV04` and `SPLH` alone.
//! The Linux port's base binary plays no film at an end; its expansion
//! plays `MV07` at the end panels (`local/archive/linux-port.md`).

use crate::data::fourcc::FourCc;
use crate::view::menu::{CINEMATIC, end};

/// A film the page lists. [`Film::ALL`] is its order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Film {
    Opening,
    North,
}

impl Film {
    pub const ALL: [Film; 2] = [Film::Opening, Film::North];

    /// Its `PICT` video.
    #[must_use]
    pub const fn id(self) -> FourCc {
        match self {
            Film::Opening => CINEMATIC[0],
            Film::North => end::ENDING,
        }
    }

    /// Its archive's place in [`super::CINEMATICS`].
    #[must_use]
    pub const fn archive(self) -> usize {
        match self {
            Film::Opening => 0,
            Film::North => 1,
        }
    }

    /// Its row. openregent's words: no `GMTX` names a film.
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Film::Opening => "The Opening",
            Film::North => "Victory in the North",
        }
    }

    /// What the lit row says of it.
    #[must_use]
    pub const fn blurb(self) -> &'static str {
        match self {
            Film::Opening => "The opening movie.",
            Film::North => "Every quest of the Northern Expansion won.",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_films_ids_are_the_originals_videos() {
        assert_eq!(Film::Opening.id(), FourCc(*b"MV04"));
        assert_eq!(Film::North.id(), FourCc(*b"MV07"));
    }
}
