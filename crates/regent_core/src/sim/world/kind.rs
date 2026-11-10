//! A piece's kind.

/// A unit's kind: which of the engine's unit lists takes it
/// (`BuildingList`, `VehicleList`, `RewardFlagList` test it).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Kind(pub u32);

impl Kind {
    /// `BLDG`, `BuildingList`, unit list 0.
    pub const BUILDING: Kind = Kind(0);
    /// `VHCL`, `VehicleList`, unit list 1.
    pub const VEHICLE: Kind = Kind(1);
    /// `GRND`, `TerrainFeatureList`, unit list 2.
    pub const SCATTER: Kind = Kind(2);
    /// `RWRD`, `RewardFlagList`, unit list 4.
    pub const REWARD: Kind = Kind(4);
}
