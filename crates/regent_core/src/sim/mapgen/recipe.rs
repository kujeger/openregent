//! What a land hands the map generator.
//!
//! A save's setup names a land: a region of `Freestyle.q`, whose team items
//! name zones and their weights. A zone names three chart patterns: what to
//! scatter, what ground to lay, what shape to give it. Only the chart holds
//! terrain classes and fractal settings.
//!
//! ```text
//! save setup slot 3  "KxHa"
//!   Freestyle.q region KxHa  K'Hal-Kratis
//!     item $Kha level 67 ─┐
//!     item $Khb level 19 ─┤
//!                         └─> zone $Kha  $Khal_Snow
//!                               landscape xfrf   %free_deepsnow
//!                               surface   xsno   %snow
//!                               height    xsli   %slightly_hilly
//!                                           └─> constants.rgs profile
//!                                               hurst, falloff, amplitude
//! ```
//!
//! [`Land::of`] walks the left side from the `.q`; [`ZoneSpec::profile`] and
//! [`Chart::layer`] the right from the `.rgs`. Kept apart, as a caller may hold
//! different editions of each.
//!
//! # An item's level is a share
//!
//! `SpawnItem::level` is `spawn_level` on a lair. In a land's region the items
//! are zones, and the engine reads the field as a pick weight:
//! Distribution build takes the code at `+4` and this at
//! `+8`.

use std::collections::BTreeSet;

use crate::data::fourcc::FourCc;
use crate::data::rgs::chart::{Chart, Layer, Pattern, Profile};
use crate::data::rgs::class_of;
use crate::data::rgs::map::{Map, Zone};

/// A land, and the zones it is made of, in the order the region lists them.
#[derive(Debug, Clone)]
pub struct Land<'a> {
    pub code: FourCc,
    pub name: &'a [u8],
    pub zones: Vec<ZoneSpec<'a>>,
}

/// One zone of a land: its share, and the three patterns that dress it.
#[derive(Debug, Clone)]
pub struct ZoneSpec<'a> {
    pub code: FourCc,
    /// The region item's level: this zone's pick weight.
    pub weight: u32,
    /// How many seeds the region chart plants for this zone.
    ///
    /// **Binary** — region seed place plants the table entry's count
    /// plus one, read by distribution seeds. The entry is this `SpawnItem`, so
    /// the count is `words[0]`.
    ///
    /// Most zones ask for one; eleven freestyle zones ask for more
    /// (Ravenswood's `$Rac` 21, Shovrah Dan's `$Shb` 16), hence patchy lands.
    /// All four shipped freestyle quests agree.
    pub seeds: u32,
    pub name: &'a [u8],
    /// What to scatter: an object-layer pattern.
    pub landscape: FourCc,
    /// What ground to lay: a surface-layer pattern.
    pub surface: FourCc,
    /// What shape to give it: a height profile.
    pub height: FourCc,
}

impl<'a> Land<'a> {
    /// The land a code names, zones resolved against the map's zone records.
    ///
    /// `None` if no region carries the code, the region has no team, an
    /// item names an undefined zone, or a seed count is `u32::MAX`.
    #[must_use]
    pub fn of(map: &Map<'a>, code: FourCc) -> Option<Self> {
        let region = map.used_regions().find(|r| r.code == code)?;
        let team = region.team.as_ref()?;

        let zones = team
            .items
            .iter()
            .map(|item| {
                let zone = map.zones.iter().find(|z| z.code == item.id)?;

                ZoneSpec::of(zone, item.level, item.words[0])
            })
            .collect::<Option<Vec<_>>>()?;

        Some(Land {
            code: region.code,
            name: team.name,
            zones,
        })
    }

    /// The weights normalised, per mille.
    ///
    /// Not the final map share. The weight is how often the grow loop picks a
    /// zone. A zone stops when its
    /// last record is spent and cannot be rehomed; region chart build
    /// then drops it. So many-seeded zones outgrow their weight.
    ///
    /// **Corpus**: excess rises with seed count within a land; on all four
    /// lands with differing seeds the most-seeded zone has the largest excess.
    /// Ravenswood's `$Rac` asks 253, takes 288 on 21 seeds; `$Raa` asks 253,
    /// takes 232 on one. Only the ordering holds: `$Thb` is 43 per mille under
    /// on six seeds.
    ///
    /// Carves are checked via the heightfield, which reproduces every
    /// corpus save and depends on zone ownership per cell.
    #[must_use]
    pub fn shares(&self) -> Vec<u32> {
        // Widened: u32 weights from the file could overflow a u32 sum.
        let total: u64 = self.zones.iter().map(|z| u64::from(z.weight)).sum();
        let share = |weight: u32| {
            let mille = (u64::from(weight) * 1_000).checked_div(total);

            // At most 1,000: a weight is part of the total.
            mille.map_or(0, |m| u32::try_from(m).unwrap_or(u32::MAX))
        };

        self.zones.iter().map(|z| share(z.weight)).collect()
    }
}

impl<'a> ZoneSpec<'a> {
    /// `None` if the seed count overflows.
    fn of(zone: &Zone<'a>, weight: u32, seeds: u32) -> Option<Self> {
        Some(ZoneSpec {
            code: zone.code,
            weight,
            seeds: seeds.checked_add(1)?,
            name: zone.name,
            landscape: zone.landscape,
            surface: zone.surface,
            height: zone.height,
        })
    }

    /// The fractal settings this zone's ground is generated with.
    #[must_use]
    pub fn profile<'c>(&self, chart: &'c Chart<'a>) -> Option<&'c Profile<'a>> {
        chart.profiles.iter().find(|p| p.code == self.height)
    }

    /// The pattern naming the ground this zone lays.
    #[must_use]
    pub fn ground<'c>(&self, chart: &'c Chart<'a>) -> Option<&'c Pattern<'a>> {
        chart.find_in(Layer::Surface, self.surface)
    }

    /// The pattern naming what this zone scatters.
    #[must_use]
    pub fn scatter<'c>(&self, chart: &'c Chart<'a>) -> Option<&'c Pattern<'a>> {
        chart.find_in(Layer::Landscape, self.landscape)
    }

    /// Every terrain class this zone's ground can put on a vertex.
    ///
    /// Two hops: surface pattern to terrain patterns to `DTPT` ids. The class
    /// number is the id's digits (`GR08` is 8), as in a save's cell.
    #[must_use]
    pub fn classes(&self, chart: &Chart<'a>) -> BTreeSet<u8> {
        let Some(surface) = self.ground(chart) else {
            return BTreeSet::new();
        };

        surface
            .entries
            .iter()
            .filter_map(|e| chart.find_in(Layer::Terrain, e.id))
            .flat_map(|terrain| terrain.entries.iter().filter_map(|e| class_of(e.id)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zone(weight: u32) -> ZoneSpec<'static> {
        let none = FourCc::new(b"NONE");

        ZoneSpec {
            code: none,
            weight,
            seeds: 1,
            name: b"",
            landscape: none,
            surface: none,
            height: none,
        }
    }

    #[test]
    fn shares_of_the_widest_weights_do_not_overflow() {
        let land = Land {
            code: FourCc::new(b"KxHa"),
            name: b"",
            zones: vec![zone(u32::MAX), zone(u32::MAX)],
        };

        assert_eq!(land.shares(), [500, 500]);
    }
}
