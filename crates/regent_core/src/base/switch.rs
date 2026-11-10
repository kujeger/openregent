//! An opt-in debugging switch: an environment variable, read once per
//! run. The one place the library reads the environment.
//!
//! ```text
//! MAJESTY_CHECK_GRID=1 cargo nextest run
//! ```
//!
//! Never `cfg!(debug_assertions)`: a debug build must not pay for a
//! costly self-check.

use std::sync::OnceLock;

/// A switch, by its variable.
#[derive(Debug)]
pub struct Switch {
    var: &'static str,
    asked: OnceLock<bool>,
}

impl Switch {
    /// The switch `var` sets.
    #[must_use]
    pub const fn new(var: &'static str) -> Switch {
        Switch {
            var,
            asked: OnceLock::new(),
        }
    }

    /// Its environment variable.
    #[must_use]
    pub const fn var(&self) -> &'static str {
        self.var
    }

    /// Whether the variable is set, read the first time asked.
    #[must_use]
    pub fn on(&self) -> bool {
        *self
            .asked
            .get_or_init(|| std::env::var_os(self.var).is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unset_variable_is_off() {
        static NEVER: Switch = Switch::new("MAJESTY_TEST_SWITCH_NEVER_SET");

        assert!(!NEVER.on());
        assert_eq!(NEVER.var(), "MAJESTY_TEST_SWITCH_NEVER_SET");
    }
}
