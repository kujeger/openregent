//! The panels' pictures named, and their fonts and stripes, read from the
//! game's catalogue on first use; the pictures are the art service's
//! (`art::Art`, shared with the menus).

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Arc;

use bevy_egui::egui::Color32;

use regent_core::data::catalogue::Catalogue;
use regent_core::data::fourcc::FourCc;

use super::stripes::Stripes;
use crate::menu::font::Font;
use regent_draw::art::ArtSource;
use regent_draw::art::look::toned;
use regent_draw::art::read::{image, sprite};
pub use regent_draw::art::{Pic, Tone, Wrap};

/// Which pictures a [`Stripes`] is cut from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Cut {
    /// A picture in a tone, where its bands are exact.
    Exact(Pic, Tone),
    /// A bar's fill over its track.
    Bar { fill: Pic, track: Pic },
}

/// The panels' fonts and stripes.
#[derive(Default)]
pub struct Ink {
    fonts: HashMap<FourCc, Option<Font>>,
    stripes: HashMap<Cut, Option<Stripes>>,
    /// Each picture's slot count ([`super::sheet::slots`]): every
    /// control asks each frame, a lookup a slot.
    slots: HashMap<(ArtSource, Pic), usize>,
    /// Text wrapped into lines, coloured ([`super::canvas::Canvas::paragraphs`]),
    /// and its line counts ([`super::canvas::Canvas::lines`]): a list's
    /// rows wrap the same words each frame.
    pub(super) wrapped: Recent<(Wrapping, Color32), Lines>,
    pub(super) counted: Recent<Wrapping, usize>,
}

/// What a text wraps by: its font, scale and width (as bits), and the
/// text.
pub(super) type Wrapping = (FourCc, u32, u32, String);

/// Wrapped lines, each with its characters' colours.
pub(super) type Lines = Arc<[(String, Vec<(Color32, String)>)]>;

/// Values kept while asked for: this egui pass's and the last's. A
/// value not asked for in a pass goes.
pub(super) struct Recent<K, V> {
    pass: u64,
    now: HashMap<K, V>,
    last: HashMap<K, V>,
}

impl<K, V> Default for Recent<K, V> {
    fn default() -> Self {
        Recent {
            pass: 0,
            now: HashMap::new(),
            last: HashMap::new(),
        }
    }
}

impl<K: Eq + Hash, V: Clone> Recent<K, V> {
    /// The value for `key` in egui pass `pass`, made where neither this
    /// pass nor the last asked for it.
    pub(super) fn get(&mut self, pass: u64, key: K, make: impl FnOnce() -> V) -> V {
        if pass != self.pass {
            self.pass = pass;
            self.last = std::mem::take(&mut self.now);
        }

        let last = &mut self.last;

        self.now
            .entry(key)
            .or_insert_with_key(|k| last.remove(k).unwrap_or_else(make))
            .clone()
    }
}

impl Ink {
    /// `pic` in `tone` as bands, where each row or each column is one
    /// colour ([`Stripes::exact`]); made on first use.
    pub fn exact(&mut self, catalogue: &Catalogue<'_>, pic: Pic, tone: Tone) -> Option<Stripes> {
        self.stripes
            .entry(Cut::Exact(pic, tone))
            .or_insert_with(|| {
                let image = toned(read(catalogue, pic)?, tone);

                Stripes::exact(&image)
            })
            .clone()
    }

    /// Bar picture `fill` as rows where it differs from `track`
    /// ([`Stripes::rows`]); made on first use.
    pub fn bar(&mut self, catalogue: &Catalogue<'_>, fill: Pic, track: Pic) -> Option<Stripes> {
        self.stripes
            .entry(Cut::Bar { fill, track })
            .or_insert_with(|| {
                let fill = read(catalogue, fill)?;
                let track = read(catalogue, track);

                Stripes::rows(&fill, track.as_ref())
            })
            .clone()
    }

    /// `pic`'s slot count in `source`, once [`Ink::counted`]: a game's
    /// catalogue is read whole, so a count stays.
    pub fn slots(&self, source: ArtSource, pic: Pic) -> Option<usize> {
        self.slots.get(&(source, pic)).copied()
    }

    /// `pic`'s slot count in `source` kept.
    pub fn counted(&mut self, source: ArtSource, pic: Pic, count: usize) {
        self.slots.insert((source, pic), count);
    }

    /// Font `record`, read on first use.
    pub fn font(&mut self, catalogue: &Catalogue<'_>, record: FourCc) -> Option<Font> {
        *self
            .fonts
            .entry(record)
            .or_insert_with(|| Font::load(catalogue, record))
    }
}

/// `pic`'s pixels from `catalogue`.
fn read(catalogue: &Catalogue<'_>, pic: Pic) -> Option<bevy_egui::egui::ColorImage> {
    sprite(catalogue, pic).map(|s| image(&s))
}

#[cfg(test)]
mod tests {
    use super::*;
    use regent_core::data::fourcc::FourCc;

    /// A value asked for in a pass or the one before is kept; one not
    /// asked for in a whole pass is made again.
    #[test]
    fn a_recent_value_lasts_one_unasked_pass() {
        let mut recent: Recent<u8, u32> = Recent::default();
        let mut made = 0;
        let mut ask = |recent: &mut Recent<u8, u32>, (pass, key)| {
            recent.get(pass, key, || {
                made += 1;
                made
            })
        };

        assert_eq!(ask(&mut recent, (1, 1)), 1);
        assert_eq!(ask(&mut recent, (1, 1)), 1);
        assert_eq!(ask(&mut recent, (2, 1)), 1);
        assert_eq!(ask(&mut recent, (3, 2)), 2);
        assert_eq!(ask(&mut recent, (4, 2)), 2);
        assert_eq!(ask(&mut recent, (4, 1)), 3);
    }

    /// The spell bar's overlays, base `INTN` 1039 and expansion `IX23`
    /// 1039, hold a picture in slots 2 and 8 only; their other slots are
    /// kind `0x08000000`, no tile. Read as tile 0 they drew `CUR1`'s
    /// cursor under each base game spell.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_frame_of_another_kind_is_no_picture() {
        let catalogue = crate::testdata::hd();

        for record in [*b"INTN", *b"IX23"] {
            let pic = Pic::of(FourCc(record), 1039);

            assert!(sprite(catalogue, pic).is_none(), "{pic:?}");
            assert!(sprite(catalogue, pic.slot(2)).is_some(), "{pic:?}");
        }
    }
}
