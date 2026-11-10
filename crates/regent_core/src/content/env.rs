//! The names an install is found by. Reading them is the caller's: the
//! process environment for openregent, `.env` too for `regent_cli` and
//! the tests (`regent_corpus`).

/// The HD install.
pub const HD: &str = "MAJESTY_HD";

/// HD's modding SDK, a folder in the install.
pub const SDK_IN_HD: &str = "SDK";
