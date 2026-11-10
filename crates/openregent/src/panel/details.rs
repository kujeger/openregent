//! What the panels say that the game's tables do not: a guild's heroes
//! against its spots. **Inferred**: the wording is a placeholder; the
//! engine's own counts are `GMTX` 9 on the recruit tooltip, 217 on a
//! list row.

use regent_core::sim::world::hire::Members;

/// A guild's line: "Heroes 2 / 4", and its graves, "Heroes 1 / 4, 1 dead".
/// Only the living take a spot (`Members`).
pub fn members(m: Members) -> String {
    let graves = match m.dead {
        0 => String::new(),
        n => format!(", {n} dead"),
    };

    format!("Heroes {} / {}{graves}", m.living, m.spots)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_guild_line_counts_the_living_and_names_its_graves() {
        let line = |living, dead| {
            members(Members {
                living,
                dead,
                spots: 4,
            })
        };

        assert_eq!(line(2, 0), "Heroes 2 / 4");
        assert_eq!(line(1, 1), "Heroes 1 / 4, 1 dead");
    }
}
