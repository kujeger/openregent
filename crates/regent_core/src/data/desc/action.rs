//! `DACT` — spells, attacks and transitions.

use super::{Head, Part, cstr, field, fixed, id_field, require};
use crate::data::FourCc;
use crate::data::cursor::Cursor;
use crate::data::error::{Error, Result};

/// A `DACT` record: one spell, attack or transition.
#[derive(Debug, Clone, Copy)]
pub struct Action<'a> {
    pub head: Head<'a>,
    pub prim: &'a [u8],
    pub secd: &'a [u8],
    script: Script<'a>,
    validation: &'a [u8],
}

/// A GPL callback an action runs. Unlike a unit's bare name, it has two words
/// in front.
#[derive(Debug, Clone, Copy)]
pub struct Script<'a> {
    /// The XML's `type`. Non-zero on one record of 135.
    pub kind: u32,
    /// The XML's `cProc`. Non-zero on three.
    pub c_proc: u32,
    /// Empty when the action declares no function.
    pub function: &'a [u8],
}

impl<'a> Action<'a> {
    /// Read a record, checking every field the accessors read.
    ///
    /// # Errors
    ///
    /// If the tree lacks a `HEAD`, `PRIM` or `SECD`, `HEAD` will not parse,
    /// `PRIM` or `SECD` is short of its fields, the script block does not
    /// fill `PRIM`'s overhang, or the validation script is unterminated. No
    /// shipped record fails.
    pub fn parse(part: &Part<'a>) -> Result<Self> {
        let head = Head::parse(require(part, *b"HEAD")?.body)?;
        let prim_part = require(part, *b"PRIM")?;
        let prim = fixed(prim_part.body, "action PRIM fields", PRIM_FIXED)?;
        let secd = fixed(
            require(part, *b"SECD")?.body,
            "action SECD fields",
            VALIDATION_AT,
        )?;
        let script = script(prim, prim_part.declared)?;
        let validation = cstr(&secd[VALIDATION_AT..], "validation script")?;

        Ok(Action {
            head,
            prim,
            secd,
            script,
            validation,
        })
    }

    /// The script block past `PRIM`'s declared length:
    /// `[u32 type][u32 cProc][cstr function]`. Fills the overhang exactly on
    /// every action of all four datasets.
    pub const fn script(&self) -> Script<'a> {
        self.script
    }

    /// `ImageSet`, the whole id of the set the actor plays: `PRIM + 0x04`,
    /// `ActionDesc_Std_Engine`, read by
    /// `GenUnit` for an action's `InitAction`.
    /// `Basic_build`'s `Carry-2` is `0x91`.
    pub fn image_set(&self) -> Option<u32> {
        field(self.prim, 0x04)
    }

    /// `Sound`, a `DSND` id: `PRIM + 0x08`, `ActionDesc_Std_Engine`.
    /// `Guardhouse_Arrow`'s is `BP13`.
    pub fn sound(&self) -> Option<FourCc> {
        id_field(self.prim, 0x08)
    }

    /// `Projectile`, a `DUNT` id: `PRIM + 0x0c`, `ActionDesc_Std_Engine`,
    /// as [`Action::rate`] maps it. `Guardhouse_Arrow`'s is
    /// `APA2`, `Green_Ranger_Arrow`.
    pub fn projectile(&self) -> Option<FourCc> {
        id_field(self.prim, 0x0c)
    }

    /// `SoundPhase` `begin`, `peak`, `end`, `DSDP` ids: `PRIM + 0x28`,
    /// `+ 0x2c`, `+ 0x30`. **Binary**: action sound plays
    /// [`Action::sound`] (none: the actor's `DefaultSound`) in `begin` as
    /// the `InitAction` is pushed
    /// and in `peak` at its peak;
    /// nothing reads `end`. `ballista_bolt`'s peak is `EAK0`, `Attack`.
    pub fn sound_phases(&self) -> [Option<FourCc>; 3] {
        [0x28, 0x2c, 0x30].map(|at| id_field(self.prim, at))
    }

    /// `Overlay`, a `DUNT` id: `PRIM + 0x10`, `ActionDesc_Std_Engine`.
    /// Init action peak hangs it on the actor for 0 ms.
    /// `level_up`'s is `ARC1`, `Level_Up`.
    pub fn overlay(&self) -> Option<FourCc> {
        id_field(self.prim, 0x10)
    }

    /// `Distance` min and max: `PRIM + 0x18`, `+ 0x1c`.
    pub fn distance(&self) -> Option<(u32, u32)> {
        field(self.prim, 0x18).zip(field(self.prim, 0x1c))
    }

    /// `Rate` min and max, ms: `PRIM + 0x20`, `+ 0x24`.
    ///
    /// **Binary**: the engine part copies `PRIM` whole;
    /// the XML reader stores `Rate`.
    /// The max is an Action order's length.
    pub fn rate(&self) -> Option<(u32, u32)> {
        field(self.prim, 0x20).zip(field(self.prim, 0x24))
    }

    /// The set the actor plays once its `InitAction` ends: `PRIM + 0x34`,
    /// `ActionDesc_Std_Engine`, which action after set reads for
    /// `GenUnit`; 0 plays nothing, -1 a default.
    /// `basic_attack_with_stand`'s is `Stand`, 8.
    pub fn after_set(&self) -> Option<i32> {
        field(self.prim, 0x34).map(u32::cast_signed)
    }

    pub fn effector_duration(&self) -> Option<u32> {
        field(self.secd, 0x04)
    } // 21
    pub fn timeout_duration(&self) -> Option<u32> {
        field(self.secd, 0x08)
    } // 42
    /// `SpellType`: the list `GetBestSpell` asks for, the SDK's `#List_*`
    /// constants (`#List_Attack` 1, `#List_Travel` 4). **Binary**:
    /// Best spell pick compares the asked list with
    /// word `0x10` of the in-memory block, four above the file's.
    pub fn spell_type(&self) -> Option<u32> {
        field(self.secd, 0x0c)
    }
    pub fn character_level(&self) -> Option<u32> {
        field(self.secd, 0x10)
    } // 26
    pub fn spell_rank(&self) -> Option<u32> {
        field(self.secd, 0x14)
    } // 42

    /// `ValidationScript`, the GPL function best spell pick asks
    /// before it keeps the spell: a string at `SECD + 0x1c`, in memory
    ///  with its length. `None` when empty. **Corpus**:
    /// `teleport` (`WRd1`) holds `teleport_check`.
    pub fn validation_script(&self) -> Option<&'a [u8]> {
        Some(self.validation).filter(|s| !s.is_empty())
    }
}

/// Where `ValidationScript` sits in an action's `SECD`.
const VALIDATION_AT: usize = 0x1c;

/// `PRIM`'s fixed fields, through [`Action::after_set`]; the declared
/// length is 64 on every record.
const PRIM_FIXED: usize = 0x38;

/// The script block over `PRIM`'s `declared` length, which it must fill.
fn script(prim: &[u8], declared: usize) -> Result<Script<'_>> {
    let mut r = Cursor::at_offset(prim, declared);
    let script = Script {
        kind: r.u32("action script type")?,
        c_proc: r.u32("action script cProc")?,
        function: r.cstr("action script function")?,
    };

    if r.done() {
        Ok(script)
    } else {
        Err(Error::BrokenInvariant {
            what: "action script fills PRIM's overhang",
            expected: prim.len() as u64,
            found: r.at() as u64,
        })
    }
}
