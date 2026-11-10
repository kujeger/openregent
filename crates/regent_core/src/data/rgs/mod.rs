//! The RGS map-generator layer: `.rgs` charts and `.q` quest maps.
//!
//! Both open with a four-character tag, repeated at offset 8:
//!
//! ```text
//! 0x00  char[4]  tag
//! 0x04  u32      0
//! 0x08  char[4]  tag, repeated
//! 0x0c  u32      0
//! ```
//!
//! `.rgs` holds generator configuration (the `RGSChart` family); `.q` holds
//! quest maps. Both use the same 22 `RGS*` classes.
//!
//! The fourth character is a version; several ship at once: maps `RGM6` (19
//! files), `RGM9` (13), `RGMa` (4); charts `RGCA` (1), `RGCB` (4).
//!
//! Bodies: see [`chart`] and [`map`].

pub mod chart;
pub mod class;
pub mod map;

pub use chart::Chart;
pub use class::{blendable, class_of, code_of};
pub use map::Map;

use super::error::{Error, Result, slice, u32_at};
use super::fourcc::FourCc;

const HEADER_LEN: usize = 16;

/// Which RGS file kind a tag denotes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// `RGC*` — generator configuration, shipped as `.rgs`.
    Chart,
    /// `RGM*` — a quest map, shipped as `.q`.
    Map,
}

/// A recognised RGS-family file.
///
/// A chart's body is decoded by [`Chart::parse`], a map's by [`Map::parse`].
#[derive(Debug, Clone, Copy)]
pub struct File<'a> {
    pub tag: FourCc,
    pub kind: Kind,
    pub body: &'a [u8],
}

impl<'a> File<'a> {
    /// Identify an `.rgs` or `.q` file from its framing.
    ///
    /// # Errors
    ///
    /// If the file is under 16 bytes, the tag is neither `RGM*` nor `RGC*`, or
    /// the tag is not repeated at offset 8.
    pub fn parse(data: &'a [u8]) -> Result<Self> {
        let head = slice(data, "rgs header", 0, HEADER_LEN)?;
        let tag = FourCc::from_prefix(head)?;
        let repeat = FourCc::from_prefix(&head[8..])?;

        if tag != repeat {
            return Err(Error::BadMagic {
                what: "rgs repeated tag",
                found: repeat.as_bytes().to_vec(),
            });
        }

        for at in [4, 12] {
            let pad = u32_at(head, "rgs padding", at)?;

            if pad != 0 {
                return Err(Error::BadValue {
                    what: "rgs padding",
                    value: u64::from(pad),
                });
            }
        }

        // The first three characters give the kind; the fourth is the version.
        let kind = match &tag.as_bytes()[..3] {
            b"RGM" => Kind::Map,
            b"RGC" => Kind::Chart,
            _ => {
                return Err(Error::BadMagic {
                    what: "rgs",
                    found: head[..4].to_vec(),
                });
            }
        };

        Ok(File {
            tag,
            kind,
            body: &data[HEADER_LEN..],
        })
    }

    /// Read this file's body as a generator chart.
    ///
    /// # Errors
    ///
    /// If this is a map rather than a chart, or the body will not parse.
    pub fn chart(&self) -> Result<Chart<'a>> {
        if self.kind == Kind::Chart {
            Chart::parse(self.tag, self.body)
        } else {
            Err(Error::BadMagic {
                what: "rgs chart",
                found: self.tag.as_bytes().to_vec(),
            })
        }
    }

    /// Read this file's body as a quest map.
    ///
    /// # Errors
    ///
    /// If this is a chart rather than a map, or the body will not parse.
    pub fn map(&self) -> Result<Map<'a>> {
        if self.kind == Kind::Map {
            Map::parse(self.tag, self.body)
        } else {
            Err(Error::BadMagic {
                what: "rgs map",
                found: self.tag.as_bytes().to_vec(),
            })
        }
    }
}
