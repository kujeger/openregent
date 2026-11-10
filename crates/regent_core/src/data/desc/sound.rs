//! `DSND` — which wave plays in which situation, and how.

use super::{Head, Part, bit, field, fixed, float, id_field, require, word};
use crate::data::error::{Error, Result};
use crate::data::fourcc::FourCc;

// --------------------------------------------------------------------------
// Sound
// --------------------------------------------------------------------------
//
// The SDK's `M_Sounds.xml` and `MX_Sounds.xml` describe `DSND` field for
// field. `PRIM` declares length 8 and undercounts: the body is a 12-byte head
// with a phase count, then that many fixed-size entries.
//
// A phase names its situation (`Attack`, `Select`, `Ambient_Die1`) by a `DSDP`
// id, and optionally a mixing group by a `DSDG` id. Both are `HEAD`-only, so
// `Head` reads them.

/// Offsets inside a `DSND` `PRIM` body.
const SOUND_CATEGORY: usize = 0x00;
const SOUND_PHASE_COUNT: usize = 0x08;

/// The fixed part, before the phase array.
const SOUND_HEAD_LEN: usize = 0x0c;

/// A phase entry's stride.
const PHASE_LEN: usize = 0x3c;

/// A `DSDG` `PRIM`: three words.
const GROUP_LEN: usize = 12;

/// A `DSND` record: one sound, and the phases it plays in.
#[derive(Debug, Clone, Copy)]
pub struct Sound<'a> {
    pub prim: &'a [u8],
}

impl<'a> Sound<'a> {
    /// # Errors
    ///
    /// If the tree lacks a `HEAD` or `PRIM`, if `HEAD` will not parse, or if
    /// the body is not a 12-byte head followed by whole phase entries.
    pub fn parse(part: &Part<'a>) -> Result<Self> {
        Head::parse(require(part, *b"HEAD")?.body)?;
        let prim = fixed(require(part, *b"PRIM")?.body, "sound head", SOUND_HEAD_LEN)?;

        let count = word(prim, SOUND_PHASE_COUNT) as usize;
        let want = SOUND_HEAD_LEN + count * PHASE_LEN;

        // Exact on every record; enforced so `phases` is infallible.
        (want == prim.len())
            .then_some(Sound { prim })
            .ok_or(Error::BrokenInvariant {
                what: "sound phase array length",
                expected: want as u64,
                found: prim.len() as u64,
            })
    }

    /// 0 on all but four records.
    pub fn category(&self) -> Option<u32> {
        field(self.prim, SOUND_CATEGORY)
    }

    /// How many phases the record declares. Matches the XML on every record.
    pub fn phase_count(&self) -> usize {
        word(self.prim, SOUND_PHASE_COUNT) as usize
    }

    pub fn phases(&self) -> impl Iterator<Item = Phase<'a>> + '_ {
        self.prim[SOUND_HEAD_LEN..]
            .as_chunks::<PHASE_LEN>()
            .0
            .iter()
            .map(|phase| Phase(phase))
    }
}

/// A bit of a phase's `Flags` word. Sample counts in brackets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhaseFlag {
    Looped = 4,   // 702
    CanQueue = 6, // 11
}

impl PhaseFlag {
    pub const ALL: [PhaseFlag; 2] = [PhaseFlag::Looped, PhaseFlag::CanQueue];

    pub const fn name(self) -> &'static str {
        match self {
            PhaseFlag::Looped => "Looped",
            PhaseFlag::CanQueue => "CanQueue",
        }
    }
}

/// One phase of a sound: which wave plays, and how it is mixed.
#[derive(Debug, Clone, Copy)]
pub struct Phase<'a>(&'a [u8]);

impl Phase<'_> {
    pub fn flags(&self) -> Option<u32> {
        field(self.0, 0x00)
    }

    pub fn has(&self, flag: PhaseFlag) -> bool {
        bit(self.0, 0x00, flag as u32)
    }

    pub fn flag_names(&self) -> impl Iterator<Item = PhaseFlag> + '_ {
        PhaseFlag::ALL.into_iter().filter(|f| self.has(*f))
    }

    /// A `DSDP` record — the situation this phase covers.
    pub fn id(&self) -> Option<FourCc> {
        id_field(self.0, 0x04)
    }

    /// A `WAVE` record.
    pub fn wave(&self) -> Option<FourCc> {
        id_field(self.0, 0x08)
    }

    /// A `DSDG` record, when the phase belongs to a mixing group.
    pub fn group(&self) -> Option<FourCc> {
        id_field(self.0, 0x0c)
    }

    /// Random volume spread, up and down.
    pub fn volume(&self) -> Option<(u32, u32)> {
        pair(self.0, 0x14)
    }

    /// Random pitch spread, up and down.
    pub fn frequency(&self) -> Option<(u32, u32)> {
        pair(self.0, 0x20)
    }

    /// How far the sound carries.
    pub fn distance(&self) -> Option<f32> {
        float(self.0, 0x38).filter(|v| *v != 0.0)
    }

    /// A set volume, 0 to 10,000: word `0x10` under `Flags` bit 1
    /// (**Binary**, `GE_Sound`: buffer method).
    pub fn set_volume(&self) -> Option<u32> {
        self.set(1, 0x10)
    }

    /// A set playback rate, Hz: word `0x1c` under `Flags` bit 2
    /// (`GE_Sound`: buffer method).
    pub fn set_frequency(&self) -> Option<u32> {
        self.set(2, 0x1c)
    }

    /// A set pan: word `0x28` under `Flags` bit 3 (`GE_Sound`: buffer
    /// method).
    pub fn set_pan(&self) -> Option<i32> {
        self.set(3, 0x28).map(u32::cast_signed)
    }

    fn set(&self, flag: u32, at: usize) -> Option<u32> {
        field(self.0, at).filter(|v| bit(self.0, 0x00, flag) && *v != 0)
    }
}

/// A phase's random spread drawn: one draw of `up + down`, less `down`;
/// none below 2 (**Binary**, sound rate spread, sound volume spread). The engine
/// draws on its paint RNG, so no game state moves. `draw(n)` is `0..n`.
///
/// ```
/// use regent_core::data::desc::spread;
///
/// assert_eq!(spread((2000, 500), |n| n - 1)?, 1999);
/// assert_eq!(spread((2000, 500), |_| 0)?, -500);
/// assert_eq!(spread((1, 0), |_| 7)?, 0);
/// # Ok::<(), regent_core::data::Error>(())
/// ```
///
/// # Errors
///
/// If `up + down` overflows a `u32`, or the drawn value an `i32`.
pub fn spread(range: (u32, u32), draw: impl FnOnce(u32) -> u32) -> Result<i32> {
    let (up, down) = range;
    let refused = |value: u64| Error::BadValue {
        what: "sound spread",
        value,
    };
    let span = up
        .checked_add(down)
        .ok_or_else(|| refused(u64::from(up) + u64::from(down)))?;

    if span > 1 {
        let drawn = i64::from(draw(span)) - i64::from(down);

        i32::try_from(drawn).map_err(|_| refused(u64::from(span)))
    } else {
        Ok(0)
    }
}

/// Two adjacent words, or `None` when both are zero, meaning "not set".
fn pair(body: &[u8], at: usize) -> Option<(u32, u32)> {
    let (up, down) = (field(body, at)?, field(body, at + 4)?);

    (up != 0 || down != 0).then_some((up, down))
}

// --------------------------------------------------------------------------
// Sound group
// --------------------------------------------------------------------------

/// A `DSDG` record: how many of a group's phases may play, and how often.
///
/// **Binary**: `PRIM` is three words, version 2, which
/// `SoundGroupDesc_Engine` copies after its `HEAD`.
/// `GE_SoundGroupDescList` lets a phase start when
/// `timeGetTime` is at least `interval` past the group's last start and
/// fewer than `limit` play; method counts a start, method an end. So
/// `Palace_Sieged_Group`, 1 and 45,000 ms, says "under siege" at most once
/// in 45 s. Real time, not game time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SoundGroup<'a> {
    pub head: Head<'a>,
    /// At most this many of the group's phases at once.
    pub limit: u32,
    /// Real ms after the group's last start before the next.
    pub interval: u32,
    /// Bit 0: a phase the group holds back waits, as a `CanQueue` phase
    /// does, else it is dropped (`SoundDesc` reads it through
    /// `GE_SoundGroupDescList`; `GE_SoundManager` queues,
    /// method retries). Set on `Advisor_Group` only.
    pub flags: u32,
}

impl<'a> SoundGroup<'a> {
    /// # Errors
    ///
    /// No `HEAD` or `PRIM`, a `HEAD` that will not parse, or a `PRIM` short
    /// of three words.
    pub fn parse(part: &Part<'a>) -> Result<Self> {
        let head = Head::parse(require(part, *b"HEAD")?.body)?;
        let prim = fixed(require(part, *b"PRIM")?.body, "sound group", GROUP_LEN)?;

        Ok(SoundGroup {
            head,
            limit: word(prim, 0x00),
            interval: word(prim, 0x04),
            flags: word(prim, 0x08),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_spread_past_a_word_is_refused() {
        // `up + down` past `u32::MAX`; a file could hold it.
        assert!(spread((u32::MAX, 1), |_| 0).is_err());
        assert!(
            spread((0, u32::MAX), |_| 0).is_err(),
            "drawn below i32::MIN"
        );
    }
}
