//! An effector's end set and the particle system it names.
//!
//! Corpora: `MAJESTY_HD`.

mod common;

/// `energy_blast_effector`'s art: its end set `0x60` names system `XL08`,
/// so its end makes one, a handle. **Game**: play5's at 277,800 ms, 0 ms,
/// is why agent 56 (285,500 ms) is the save's 1007.
#[test]
fn an_effectors_end_set_names_a_particle_system() {
    let bodies = common::bodies();
    let blast = bodies
        .iter()
        .find(|(_, b)| b.name.eq_ignore_ascii_case("energy_blast_effector"))
        .map(|(c, b)| {
            (
                c.to_string(),
                b.vents.get(&0x60).map(|v| v[0].code.to_string()),
            )
        });

    assert_eq!(blast, Some(("WRa1".to_owned(), Some("XL08".to_owned()))));
}
