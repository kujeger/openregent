//! A `DUNT` record with everything it points at, resolved.
//!
//! [`crate::data::desc`] stops at a unit's references (`IMAG` art, `DSND`
//! sounds, `DMOV` movement). This module follows them across archives: none of
//! the base dataset's 394 units has its art in its own file.
//!
//! Resolves locations, not pixels; see [`super::imagery`].
//!
//! # Two archives, two numbering schemes
//!
//! Confusing them fails silently:
//!
//! ```text
//! unittype.cam          maindata.cam
//!   DUNT "AVA1" ──id──>   IMAG "AVA1" ──position──> TILE 4711
//!         ^                     ^                        ^
//!    the unit's archive    found by id, across      counted within the
//!                          the whole dataset        IMAG's archive, not
//!                                                   the unit's
//! ```
//!
//! No global tile id exists.
//!
//! # Holes are content
//!
//! Every reference is a [`Link`], keeping its id even if unresolved. Nine base
//! dataset units name missing art; show them, do not skip them.

use crate::data::desc::{self, UnitKind};
use crate::data::error::{Error, Result};
use crate::data::fourcc::FourCc;
use crate::data::imag::{self, SetKind};

use super::imagery::{self, Animation, FrameSource};
use crate::data::catalogue::{Catalogue, Located};

/// A reference by id, and where it resolved. `at` is `None` if absent; not an
/// error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Link {
    pub id: FourCc,
    pub at: Option<Located>,
}

impl Link {
    pub(crate) fn find(catalogue: &Catalogue<'_>, tag: [u8; 4], id: FourCc) -> Self {
        Link {
            id,
            at: catalogue.find(&tag, id),
        }
    }

    pub const fn resolved(&self) -> bool {
        self.at.is_some()
    }
}

/// A unit with its references resolved.
#[derive(Debug, Clone)]
pub struct Unit<'a> {
    pub id: FourCc,
    /// Archive of the unit's own record.
    pub archive: usize,
    /// `None` for a subtype outside the six known; none shipped.
    pub kind: Option<UnitKind>,
    /// The descriptor, for stats and flags, read via its tested accessors.
    pub desc: desc::Unit<'a>,
    /// The unit's `IMAG`.
    pub art: Option<Link>,
    /// [`Unit::art`] parsed once, for what reads more of it than
    /// [`Unit::animations`]. `None` when it is absent or did not resolve.
    pub image: Option<imag::Image<'a>>,
    /// Empty when [`Unit::art`] is absent or did not resolve.
    pub animations: Vec<Animation>,
    /// `DMOV` movement class.
    pub movement: Option<Link>,
    /// Default `DSND`.
    pub sound: Option<Link>,
    /// Per-action sound overrides: `(DACT, DSND)`.
    pub overrides: Vec<(Link, Link)>,
}

impl<'a> Unit<'a> {
    /// Resolve the `DUNT` record at `at`.
    ///
    /// # Errors
    ///
    /// If there is no `DUNT` there, or it or its tail will not parse.
    /// Unresolved references are not errors ([`Link`]).
    pub fn at(catalogue: &Catalogue<'a>, at: Located) -> Result<Self> {
        let record = catalogue.record(b"DUNT", at).ok_or(Error::BadValue {
            what: "no DUNT record at this location",
            value: at.record as u64,
        })?;

        let part = desc::Part::parse(record.data)?;
        let desc = desc::Unit::parse(&part)?;

        let art = desc
            .image_id()
            .map(|id| Link::find(catalogue, *b"IMAG", id));
        let found = art.and_then(|link| link.at);
        let image = found
            .map(|found| imagery::image(catalogue, found))
            .transpose()?;
        let animations = found
            .zip(image.as_ref())
            .map(|(found, image)| imagery::animations(catalogue, found, image))
            .unwrap_or_default();

        // Tail: callback, attachments, sounds, checked by `Unit::parse`.
        let tail = desc.tail();

        let movement = tail
            .attachments
            .as_ref()
            .and_then(desc::Attachments::movement_class)
            .map(|(_, id)| Link::find(catalogue, *b"DMOV", id));

        let sound = tail
            .default_sound
            .map(|id| Link::find(catalogue, *b"DSND", id));

        let overrides = tail
            .overrides()
            .map(|o| {
                (
                    Link::find(catalogue, *b"DACT", o.action),
                    Link::find(catalogue, *b"DSND", o.sound),
                )
            })
            .collect();

        Ok(Unit {
            id: record.id,
            archive: at.archive,
            kind: Some(desc.kind()),
            desc,
            art,
            image,
            animations,
            movement,
            sound,
            overrides,
        })
    }

    /// Resolve the unit with this id.
    ///
    /// # Errors
    ///
    /// As [`Unit::at`].
    pub fn find(catalogue: &Catalogue<'a>, id: FourCc) -> Option<Result<Self>> {
        catalogue
            .find(b"DUNT", id)
            .map(|at| Self::at(catalogue, at))
    }

    /// The descriptor of unit `id` alone, its art not followed: names and
    /// panels read only the descriptor, and [`Unit::at`]'s imagery is
    /// nine tenths of its cost.
    ///
    /// # Errors
    ///
    /// If its `DUNT` will not parse.
    pub fn descriptor(catalogue: &Catalogue<'a>, id: FourCc) -> Option<Result<desc::Unit<'a>>> {
        let record = catalogue.record(b"DUNT", catalogue.find(b"DUNT", id)?)?;

        Some(desc::Part::parse(record.data).and_then(|part| desc::Unit::parse(&part)))
    }

    /// The first animation of a kind.
    pub fn animation(&self, kind: SetKind) -> Option<&Animation> {
        self.animations.iter().find(|a| a.kind == kind)
    }

    /// The set a new unit is made in: its first `Inactive`, else its first
    /// set. Its slot count bounds `GSUnit`'s slot draw.
    ///
    /// **Corpus**: the set of every `GRND` record of the 60 freestyle saves
    /// and the traces, and the slot of every start building and terrain
    /// feature (`sim::mapgen`; **Measured**).
    #[must_use]
    pub fn initial(&self) -> Option<&Animation> {
        self.animation(SetKind::Inactive)
            .or_else(|| self.animations.first())
    }

    /// One of the three interface pictures, by id (position varies, and
    /// [`Unit::animation`] cannot tell them apart). Pass
    /// [`INTERFACE_ICON`](crate::data::imag::INTERFACE_ICON) for the 25x25
    /// panel icon.
    #[must_use]
    pub fn interface(&self, id: u16) -> Option<&Animation> {
        self.animations
            .iter()
            .find(|a| a.kind == SetKind::Interface && crate::data::imag::set_number(a.id) == id)
    }

    /// Every set of a kind, in layer (draw) order.
    ///
    /// A repeated kind is a stack. `ABJ1 Palace1` `Active`: layer 0 the palace
    /// (462x247), layer 1 a six-frame 46x49 banner. Same two layers on
    /// `Inactive` (`a_buildings_two_pictures_are_two_pictures_bar_six`). `ABY1
    /// Wizards_Guild1` `Build`: layer 0 building, layer 1 scaffolding.
    ///
    /// Sorted by [`Animation::variant`], lowest first (record order).
    /// Counterexample: `BB01 Krolm_Altar` has fire on layer 0, altar on layer
    /// 1; the rule or the record is off. See `docs/formats/art.md`.
    pub fn layers(&self, kind: SetKind) -> impl Iterator<Item = &Animation> {
        let mut found: Vec<&Animation> =
            self.animations.iter().filter(|a| a.kind == kind).collect();

        found.sort_by_key(|a| a.variant);
        found.into_iter()
    }

    /// The set a record naming set `named` is drawn in: that set if the
    /// unit has it, else the first of its family ([`SetKind::from_id`]).
    ///
    /// A record may name a set its unit lacks (`AVD1` `Walk-2`, an
    /// expansion `BBQ1` `Attack-3`); the engine falls back within the
    /// family. Low 16 bits, as [`Unit::stack`] takes them.
    #[must_use]
    pub fn drawn(&self, named: u16) -> Option<u16> {
        let low = |set: &Animation| crate::data::imag::set_number(set.id);

        if self.stack(named).next().is_some() {
            Some(named)
        } else {
            self.layers(SetKind::from_id(named)).next().map(low)
        }
    }

    /// Every layer of one exact set, in layer order.
    ///
    /// A [`SetKind`] is a range (`Die` is 96–103: a building's eight damage
    /// stages), so [`Unit::layers`] would draw all stages. This takes the exact
    /// set a save names: the low 16
    /// bits of the packed id, every layer.
    pub fn stack(&self, id: u16) -> impl Iterator<Item = &Animation> {
        let mut found: Vec<&Animation> = self
            .animations
            .iter()
            .filter(|a| crate::data::imag::set_number(a.id) == id)
            .collect();

        found.sort_by_key(|a| a.variant);
        found.into_iter()
    }

    /// Every frame of every animation, in order.
    pub fn frames(&self) -> impl Iterator<Item = FrameSource> + '_ {
        self.animations
            .iter()
            .flat_map(|a| a.poses.iter())
            .flat_map(|p| p.frames.iter().copied())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unresolved_link_keeps_its_id() {
        let link = Link {
            id: FourCc::new(b"AVA1"),
            at: None,
        };

        assert!(!link.resolved());
        assert_eq!(link.id.to_string(), "AVA1");
    }
}
