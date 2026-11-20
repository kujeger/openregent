//! What a setting is, apart from its value: its key, name, text, place
//! on the settings page and range ([`Meta`]), and how a value of its type
//! is edited ([`Setting`]).
//!
//! ```text
//!  #[derive(Sections)] Settings ─┐   (regent_derive)
//!  #[derive(Section)] Ui, …     ─┴─> Settings::visit(&mut v) calls
//!                                    v.field(&PART, &META, &self.ui.wasd_scroll)
//!                                    for each setting, typed
//!  a Visit: the file read (`config`), written, the page's rows and
//!  controls (`ui::editor`)
//! ```
//!
//! Each field is declared once, in `config`:
//!
//! ```text
//!  #[setting(name = "WASD scroll", about = "…", group = Interface)]
//!  pub wasd_scroll: bool,
//! ```

use std::ops::RangeInclusive;

use serde::Serialize;
use serde::de::DeserializeOwned;

/// A section of the file, `[ui]`: a field of `Settings`.
#[derive(Debug)]
pub struct Part {
    /// Its name in the file.
    pub name: &'static str,
    /// The page's tab it shows on.
    pub tab: Tab,
}

/// A setting: a field of a section.
#[derive(Debug)]
pub struct Meta {
    /// Its key in the file.
    pub key: &'static str,
    /// Its row's name: `#[setting(name)]`, else the key in words.
    pub name: &'static str,
    /// What it does; empty for nothing to say.
    pub about: &'static str,
    /// Its heading on the page; `None` for a gated or hidden key.
    pub group: Option<Group>,
    pub gate: Gate,
    pub show: Show,
    /// A number's range; out of it in the file, the default.
    pub range: Option<RangeInclusive<f64>>,
}

/// A tab of the settings page.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, clap::ValueEnum)]
pub enum Tab {
    /// Every section but `[enhancements]`.
    #[default]
    Settings,
    /// `[enhancements]`.
    Enhancements,
}

/// A topic of a tab, its heading on the page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    Display,
    Game,
    Interface,
    Sound,
}

/// Whether a key acts on its own or behind the experimental switch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gate {
    Always,
    /// Acts only while the switch is on; no preset sets it.
    Experimental,
    /// The switch itself: the page's fold.
    Switch,
}

/// Whether the page lists a key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Show {
    Page,
    /// In the file only (`debug_keys`).
    Hidden,
}

/// A number's kind: shown and stepped whole, or to two places.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Whole {
    Integer,
    Fraction,
}

/// A value as the page edits it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Now {
    Check(bool),
    Number(f64, Whole),
    /// The choice at `.0` of the labels `.1`.
    Choice(usize, &'static [&'static str]),
}

impl Now {
    /// As the page shows it: a bool On or Off, a number to two places
    /// its trailing zeros cut, a choice its label.
    #[must_use]
    pub fn shown(&self) -> String {
        match *self {
            Now::Check(true) => "On".to_owned(),
            Now::Check(false) => "Off".to_owned(),
            Now::Number(x, Whole::Integer) => format!("{x:.0}"),
            Now::Number(x, Whole::Fraction) => number(x),
            Now::Choice(at, labels) => labels.get(at).copied().unwrap_or_default().to_owned(),
        }
    }
}

/// `x` to two places, its trailing zeros cut: 0.5, 29, -180.
#[must_use]
pub fn number(x: f64) -> String {
    let text = format!("{x:.2}");

    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// A setting's type: read and written by serde, edited on the page.
pub trait Setting: Serialize + DeserializeOwned + Clone + PartialEq {
    /// Its value as the page edits it.
    fn now(&self) -> Now;

    /// Set to `now`, of its kind; a number held to `range`.
    fn set(&mut self, now: &Now, range: Option<&RangeInclusive<f64>>);
}

impl Setting for bool {
    fn now(&self) -> Now {
        Now::Check(*self)
    }

    fn set(&mut self, now: &Now, _: Option<&RangeInclusive<f64>>) {
        if let Now::Check(on) = now {
            *self = *on;
        }
    }
}

impl Setting for i32 {
    fn now(&self) -> Now {
        Now::Number(f64::from(*self), Whole::Integer)
    }

    #[allow(clippy::cast_possible_truncation, reason = "held to its range")]
    fn set(&mut self, now: &Now, range: Option<&RangeInclusive<f64>>) {
        if let Now::Number(x, _) = now {
            *self = held(*x, range).round() as i32;
        }
    }
}

impl Setting for f32 {
    fn now(&self) -> Now {
        Now::Number(f64::from(*self), Whole::Fraction)
    }

    #[allow(clippy::cast_possible_truncation, reason = "held to its range")]
    fn set(&mut self, now: &Now, range: Option<&RangeInclusive<f64>>) {
        if let Now::Number(x, _) = now {
            *self = held(*x, range) as f32;
        }
    }
}

/// `x` in `range`, if any.
fn held(x: f64, range: Option<&RangeInclusive<f64>>) -> f64 {
    range.map_or(x, |r| x.clamp(*r.start(), *r.end()))
}

/// Whether `value` is in `meta`'s range; true for a key with none.
pub fn in_range<T: Setting>(meta: &Meta, value: &T) -> bool {
    match (&meta.range, value.now()) {
        (Some(range), Now::Number(x, _)) => range.contains(&x),
        _ => true,
    }
}

/// An enum as a [`Setting`]: each variant and its label on the page. A
/// variant left out does not compile.
///
/// ```text
///  choices! { Intro { On => "on", Off => "off" } }
/// ```
macro_rules! choices {
    ($($ty:ident { $($variant:ident => $label:literal),+ $(,)? })+) => {$(
        impl $crate::files::setting::Setting for $ty {
            fn now(&self) -> $crate::files::setting::Now {
                const ALL: &[$ty] = &[$($ty::$variant),+];
                const LABELS: &[&str] = &[$($label),+];
                // Every variant listed, or this match is not exhaustive.
                const _: fn(&$ty) = |v| match v { $($ty::$variant => {}),+ };

                let at = ALL.iter().position(|v| v == self).unwrap_or_default();

                $crate::files::setting::Now::Choice(at, LABELS)
            }

            fn set(
                &mut self,
                now: &$crate::files::setting::Now,
                _: Option<&std::ops::RangeInclusive<f64>>,
            ) {
                const ALL: &[$ty] = &[$($ty::$variant),+];

                if let $crate::files::setting::Now::Choice(at, _) = now
                    && let Some(v) = ALL.get(*at)
                {
                    *self = *v;
                }
            }
        }
    )+};
}

pub(crate) use choices;

/// What is done with each setting of a [`Section`], read only.
pub trait Visit {
    fn field<T: Setting>(&mut self, part: &'static Part, meta: &'static Meta, value: &T);
}

/// What is done with each setting of a [`Section`], changing it.
pub trait VisitMut {
    fn field<T: Setting>(&mut self, part: &'static Part, meta: &'static Meta, value: &mut T);
}

/// A section's settings, each handed to a visitor: `#[derive(Section)]`.
pub trait Section {
    fn visit<V: Visit>(&self, part: &'static Part, v: &mut V);
    fn visit_mut<V: VisitMut>(&mut self, part: &'static Part, v: &mut V);
}

/// A key's place: its section and setting, as the derives give them.
#[derive(Clone, Copy, Debug)]
pub struct Key {
    pub part: &'static Part,
    pub meta: &'static Meta,
}

impl PartialEq for Key {
    fn eq(&self, other: &Key) -> bool {
        std::ptr::eq(self.part, other.part) && std::ptr::eq(self.meta, other.meta)
    }
}

impl Eq for Key {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_value_shows_as_the_page_shows_it() {
        assert_eq!(Now::Check(true).shown(), "On");
        assert_eq!(Now::Number(32.0, Whole::Integer).shown(), "32");
        assert_eq!(Now::Number(0.5, Whole::Fraction).shown(), "0.5");
        assert_eq!(Now::Number(29.0, Whole::Fraction).shown(), "29");
        assert_eq!(Now::Number(-180.0, Whole::Fraction).shown(), "-180");
        assert_eq!(Now::Choice(1, &["a", "b"]).shown(), "b");
    }

    #[test]
    #[allow(clippy::float_cmp, reason = "exact in binary")]
    fn a_number_is_held_to_its_range() {
        let (mut whole, mut part) = (32, 1.0_f32);

        whole.set(&Now::Number(70.4, Whole::Integer), Some(&(32.0..=64.0)));
        part.set(&Now::Number(-2.0, Whole::Fraction), Some(&(0.0..=1.0)));

        assert_eq!(whole, 64);
        assert_eq!(part, 0.0);
    }
}
