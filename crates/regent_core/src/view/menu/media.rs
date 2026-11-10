//! A quest's pictures, video and voices on the front end: the
//! briefing's, the summary's, the loading page's and the end's.

use crate::data::fourcc::FourCc;
use crate::sim::world::defeat::End;
use crate::view::menu::quest_map;

/// The briefing's backdrop, `INse` set 1023 (`addinterface_se.cam`, 770
/// by 550): sepia portraits, repeated from the screen's top left.
///
/// **Binary**: `BackgroundFillController` gives the
/// screen-sized fill `INse` (`0x65734e49`) set `0x3ff` and style
/// `0x40000`. **Game**: `menu_barren_waste_start.png` outside the dialog
/// is this picture at `(x % 770, y % 550)`.
pub const BACKDROP: (FourCc, i32) = (FourCc(*b"INse"), 1023);

/// The first set of a slot-indexed picture: slot 0's.
const FIRST_SET: u16 = 1000;

/// The set an unplayable world's pictures take (`0x3f4`): a freestyle one.
const UNPLAYABLE_SET: u16 = 1012;

/// The briefing's video: [`quest_map::video`] of the quest's slot, a
/// `PICT` played by video open in the porthole's rectangle.
#[must_use]
pub fn video(code: FourCc) -> Option<FourCc> {
    quest_map::video(quest_map::slot(code)?)
}

/// The briefing's spoken text, a `WAVE` in `voices.cam` or `mx_voices.cam`:
/// `EQ` and the slot's two digits, `EQ00` "Barren Waste" for `QU01`.
///
/// **Binary**: quest voice code, given to the sound
/// manager. **Corpus**: the `XQDn` slots'
/// `EQ31`-`EQ33` do not ship.
#[must_use]
pub fn voice(code: FourCc) -> Option<FourCc> {
    video(code).map(|v| FourCc([b'E', b'Q', v.0[2], v.0[3]]))
}

/// The end panel's voice, a `DSND` said in its first phase: `ED` and the
/// quest's slot's two digits, `ED00` `End_Barren_Waste` for `QU01`. Only
/// a won quest's panel, `AP60`, says one; `AP61` none.
///
/// **Binary**: `EndGameController`, won, a playable
/// quest (quest playable: not freestyle), gives
/// quest end voice code to the sound manager's
/// method (phase 0, the first; keep 1) in `EndGameController`. A quest outside
/// [`quest_map::SLOTS`], HD's `MD01` and `VAMP`, gets code 0, which
/// `GE_SoundManager` plays nothing for.
#[must_use]
pub fn end_voice(code: FourCc, end: End) -> Option<FourCc> {
    video(code)
        .filter(|_| end == End::Won)
        .map(|v| FourCc([b'E', b'D', v.0[2], v.0[3]]))
}

/// Which dataset a quest loads (`<Dataset base>`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Release {
    /// The base game's (`Majesty`).
    Base,
    /// The expansion's.
    Expansion,
}

/// Whether a quest's world is a quest's or freestyle's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum World {
    Quest,
    /// `FRE0`, `FRE1`: not playable as a listed quest.
    Freestyle,
}

/// `AP44`'s round still ([`briefing::ZOOM`](super::briefing::ZOOM)): record and set.
///
/// **Binary**: record quest zoom record, set quest pog set:
///
/// | Quest | Record | Set |
/// |---|---|---|
/// | `XQD1`-`XQD3` | `IXR1`-`IXR3` | 1000 |
/// | outside [`quest_map::SLOTS`]: HD's `MD01`, `VAMP` | `DXR0` | 1000 |
/// | a base slot | `INTP` | 1000 + slot |
/// | an expansion slot | `MX29` | 1000 + slot - 19 |
/// | freestyle | `INTP` or `MX29` by release | 1012 |
///
/// **Corpus**: no archive holds `DXR0`; **Inferred**: those draw nothing.
#[must_use]
pub fn zoom(code: FourCc, release: Release, world: World) -> Option<(FourCc, u16)> {
    picture(
        code,
        release,
        world,
        [*b"IXR", *b"DXR"],
        [*b"INTP", *b"MX29"],
    )
}

/// The title bar's round icon (the pog, [`super::ids::POG`]), as [`zoom`]: `IXD1`-3,
/// `DX00` (`addinterface.cam`), `INTD`, `IX11`. **Binary**:
/// Quest pog record. The quest map's pog ([`quest_map::pog`]) is
/// the same picture.
#[must_use]
pub fn icon(code: FourCc, release: Release, world: World) -> Option<(FourCc, u16)> {
    picture(
        code,
        release,
        world,
        [*b"IXD", *b"DX0"],
        [*b"INTD", *b"IX11"],
    )
}

/// The loading page's still ([`loading::STILL`](super::loading::STILL)), as [`zoom`]: `IXS1`-3,
/// `DXS0`, `INTM`, `IX73` (750 by 375, sepia).
///
/// **Binary**: a quest's record quest segue record, set
/// quest pog set. A freestyle start sets only the set, 1012,
/// keeping the panel's own record: `INTM` in the base [`LOADING`](super::LOADING), `IX73`
/// in the expansion's.
#[must_use]
pub fn segue(code: FourCc, release: Release, world: World) -> Option<(FourCc, u16)> {
    picture(
        code,
        release,
        world,
        [*b"IXS", *b"DXS"],
        [*b"INTM", *b"IX73"],
    )
}

/// [`zoom`] and [`icon`]: `special` the `XQDn` prefix and the extra
/// quests' prefix, each finished by a digit; `records` base and expansion.
fn picture(
    code: FourCc,
    release: Release,
    world: World,
    special: [[u8; 3]; 2],
    records: [[u8; 4]; 2],
) -> Option<(FourCc, u16)> {
    let [xqd, extra] = special;
    let record = match release {
        Release::Base => FourCc(records[0]),
        Release::Expansion => FourCc(records[1]),
    };

    match (world, quest_map::slot(code), code.0) {
        (_, _, [b'X', b'Q', b'D', n]) => Some((FourCc([xqd[0], xqd[1], xqd[2], n]), FIRST_SET)),
        (World::Freestyle, _, _) => Some((record, UNPLAYABLE_SET)),
        (World::Quest, None, _) => Some((FourCc([extra[0], extra[1], extra[2], b'0']), FIRST_SET)),
        (World::Quest, Some(at), _) => {
            let n = match release {
                Release::Base => at,
                Release::Expansion => at.checked_sub(quest_map::BASE_SLOTS)?,
            };

            Some((record, FIRST_SET + u16::try_from(n).ok()?))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_slot_names_its_video_and_voice() {
        assert_eq!(video(FourCc(*b"QU01")), Some(FourCc(*b"QM00")));
        assert_eq!(voice(FourCc(*b"QU01")), Some(FourCc(*b"EQ00")));
        assert_eq!(voice(FourCc(*b"XQ12")), Some(FourCc(*b"EQ30")));
        assert_eq!(video(FourCc(*b"FRE0")), None);
    }

    #[test]
    fn a_won_quest_says_its_slots_end_voice_a_lost_one_none() {
        let won = |code: &[u8; 4]| end_voice(FourCc(*code), End::Won);

        assert_eq!(won(b"QU01"), Some(FourCc(*b"ED00")));
        assert_eq!(won(b"XQ12"), Some(FourCc(*b"ED30")));
        assert_eq!(won(b"MD01"), None);
        assert_eq!(won(b"VAMP"), None);
        assert_eq!(won(b"FRE1"), None);
        assert_eq!(end_voice(FourCc(*b"QU01"), End::Lost), None);
    }

    #[test]
    fn a_quests_pictures_follow_its_release() {
        let barren = FourCc(*b"QU01");
        let vigil = FourCc(*b"XQ12");

        let quest = World::Quest;

        assert_eq!(
            zoom(barren, Release::Base, quest),
            Some((FourCc(*b"INTP"), 1000))
        );
        assert_eq!(
            icon(vigil, Release::Expansion, quest),
            Some((FourCc(*b"IX11"), 1011))
        );
        assert_eq!(
            zoom(FourCc(*b"XQD2"), Release::Expansion, quest),
            Some((FourCc(*b"IXR2"), 1000))
        );
        assert_eq!(
            icon(FourCc(*b"MD01"), Release::Base, quest),
            Some((FourCc(*b"DX00"), 1000))
        );
        assert_eq!(
            icon(FourCc(*b"FRE1"), Release::Expansion, World::Freestyle),
            Some((FourCc(*b"IX11"), 1012))
        );
    }

    #[test]
    fn a_games_loading_still_follows_its_quest_and_release() {
        let still = |code: &[u8; 4], release, world| segue(FourCc(*code), release, world);
        let quest = World::Quest;

        assert_eq!(
            still(b"QU05", Release::Base, quest),
            Some((FourCc(*b"INTM"), 1004))
        );
        assert_eq!(
            still(b"XQ01", Release::Expansion, quest),
            Some((FourCc(*b"IX73"), 1000))
        );
        assert_eq!(
            still(b"XQD3", Release::Expansion, quest),
            Some((FourCc(*b"IXS3"), 1000))
        );
        assert_eq!(
            still(b"VAMP", Release::Expansion, quest),
            Some((FourCc(*b"DXS0"), 1000))
        );
        assert_eq!(
            still(b"FRE1", Release::Expansion, World::Freestyle),
            Some((FourCc(*b"IX73"), 1012))
        );
    }
}
