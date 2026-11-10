//! [`regent_core::sim::trig::PI`], as the engine parses it.

use regent_core::base::fixed::{Fixed, ONE};
use regent_core::sim::trig;

/// Pi as the engine parses it; sets the frontier ring's step.
///
/// No constant in the image: pi is filled at start-up from
/// `"3.14"`, so this checks `Str2Fixed`'s arithmetic on that string.
#[test]
fn pi_is_the_string_the_executable_parses() {
    let mut fraction = Fixed::ZERO;

    for digit in [4, 1] {
        fraction = regent_core::base::fixed::div(digit * ONE + fraction, 10 * ONE);
    }

    assert_eq!(trig::PI, 3 * ONE + fraction);
}
