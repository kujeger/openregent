//! The `.bcd` sections.
//! Uses both [`regent_core::data`] (block framing) and [`regent_core::gpl`] (contents).

use regent_core::data::bcd;
use regent_core::gpl::bcd::{DataSection, ScopeSection, symbol_count};

use crate::util::{name, read_bytes};

/// Dump the prototypes a `.bcd` carries in its data section.
///
/// Re-encodes each file and compares, to prove the decode complete.
pub fn cmd_datasec(paths: &[String]) -> Result<bool, String> {
    for p in paths {
        let data = read_bytes(p)?;
        let program = bcd::Container::parse(&data).map_err(|e| format!("{p}: {e}"))?;
        let sections = program.sections().map_err(|e| format!("{p}: {e}"))?;
        let section = DataSection::decode(sections.data.body).map_err(|e| format!("{p}: {e}"))?;

        let values: usize = section.prototypes.iter().map(|x| x.values.len()).sum();
        let exact = section.encode() == sections.data.body;

        println!(
            "{:<28} {} prototypes, {values} values, re-encodes {}",
            name(p),
            section.prototypes.len(),
            if exact { "identically" } else { "DIFFERENTLY" }
        );

        for proto in &section.prototypes {
            println!(
                "  [{}] {{{}}}  {} values",
                proto.name,
                proto.shape,
                proto.values.len()
            );

            for (key, value) in &proto.values {
                println!("      {key:<28} {value}");
            }
        }
    }

    Ok(true)
}

/// List what a `.bcd` defines, from its code section's three name tables.
///
/// Bodies are opaque, so only lengths are printed.
pub fn cmd_codesec(paths: &[String]) -> Result<bool, String> {
    for p in paths {
        let data = read_bytes(p)?;
        let program = bcd::Container::parse(&data).map_err(|e| format!("{p}: {e}"))?;
        let sections = program.sections().map_err(|e| format!("{p}: {e}"))?;

        let symbols = symbol_count(sections.symbols.body).map_err(|e| format!("{p}: {e}"))?;
        let code = ScopeSection::decode(sections.code.body).map_err(|e| format!("{p}: {e}"))?;

        println!(
            "{:<28} {} prototypes, {} functions, {} constants; {symbols} symbols, re-encodes {}",
            name(p),
            code.prototypes.len(),
            code.functions.len(),
            code.constants.len(),
            if code.encode() == sections.code.body {
                "identically"
            } else {
                "DIFFERENTLY"
            }
        );

        for (label, table) in [
            ("prototype", &code.prototypes),
            ("function", &code.functions),
            ("constant", &code.constants),
        ] {
            for definition in table {
                println!(
                    "  {label:<10} {:<44} {:>7} bytes",
                    definition.name,
                    definition.body.len()
                );
            }
        }
    }

    Ok(true)
}
