//! A panel's controls with their strings looked up, and the control
//! under a point.

use std::borrow::Cow;

use crate::data::catalogue::Catalogue;
use crate::data::error::Result;
use crate::data::fourcc::FourCc;
use crate::data::smnu::{self, Kind, Panel, Rect};
use crate::data::text::Text;

use super::colour::{Inks, Style};

/// One control, its strings looked up.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShownControl {
    /// Its class; `None` for an unknown type word.
    pub kind: Option<Kind>,
    /// Its id ([`smnu::CONTROL`]).
    pub id: Option<i32>,
    /// Its rectangle, from the panel's top left.
    pub rect: Option<Rect>,
    /// Its text's rectangle, from its own corner ([`smnu::RECT_TOO`]).
    pub text_rect: Option<Rect>,
    /// Its picture: record and set.
    pub art: Option<(FourCc, i32)>,
    pub font: Option<FourCc>,
    pub palette: Option<FourCc>,
    /// Its text's index in the panel's table ([`smnu::LABEL`]).
    pub label: Option<usize>,
    /// That text.
    pub text: Option<String>,
    /// Its tooltip ([`smnu::TOOLTIP`]).
    pub tooltip: Option<String>,
    /// Its hot key ([`smnu::HOTKEY`]).
    pub hotkey: Option<i32>,
    /// Text colours by state (`smnu::TEXT_COLOURS`).
    pub colours: Vec<i32>,
    /// The hot key letter's colours (`smnu::HOTKEY_COLOURS`).
    pub hotkey_colours: Vec<i32>,
    /// Both as resolved through the panel's palette.
    pub inks: Inks,
    /// Its flags ([`smnu::FLAGS`]).
    pub flags: i32,
    /// Its style ([`smnu::STYLE`]).
    pub style: i32,
}

impl ShownControl {
    /// Where in [`ShownControl::text`] the hot key letter stands, drawn in
    /// [`ShownControl::hotkey_colours`]: see [`hotkey_letter`].
    #[must_use]
    pub fn hotkey_at(&self) -> Option<usize> {
        hotkey_letter(self.text.as_deref()?, self.hotkey?)
    }
}

/// A panel with its strings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    pub id: FourCc,
    /// The panel's own block: its frame, picture, font and palette.
    pub panel: ShownControl,
    /// Centred on the screen ([`smnu::CENTRED`]).
    pub centred: bool,
    /// The controls, in draw order.
    pub controls: Vec<ShownControl>,
    /// The colours its palette gives controls that set none.
    pub style: Option<Style>,
}

impl Layout {
    /// The control holding `id`.
    #[must_use]
    pub fn control(&self, id: i32) -> Option<&ShownControl> {
        self.controls.iter().find(|c| c.id == Some(id))
    }

    /// The control showing text `index` of the panel's table.
    #[must_use]
    pub fn labelled(&self, index: usize) -> Option<&ShownControl> {
        self.controls.iter().find(|c| c.label == Some(index))
    }

    /// The control under point `at`, from the panel's top left: the last
    /// in draw order whose rectangle holds it, passing over
    /// [`smnu::PASSIVE`] controls and the `hidden` ids.
    ///
    /// **Binary**: dialog hit walks the controls
    /// from the last; the first hit wins unless an earlier one has a
    /// higher method ( bit 21). **Inferred**: no front-end control
    /// sets that bit; its key is not read.
    #[must_use]
    pub fn under(&self, at: (i32, i32), hidden: &[i32]) -> Option<&ShownControl> {
        self.controls.iter().rev().find(|c| {
            c.flags & smnu::PASSIVE == 0
                && c.id.is_none_or(|id| !hidden.contains(&id))
                && c.rect.is_some_and(|r| r.holds(at))
        })
    }
}

/// The panel `id` from `catalogue`, the last loaded copy; `None` without one.
///
/// # Errors
///
/// A record that does not parse.
pub fn layout(catalogue: &Catalogue<'_>, text: &Text<'_>, id: FourCc) -> Option<Result<Layout>> {
    let record = catalogue.record(b"SMNU", catalogue.find(b"SMNU", id)?)?;

    Some(Panel::parse(record.data).map(|panel| {
        let style = Style::find(catalogue, Style::palette(&panel));

        resolved(&panel, text, id, style)
    }))
}

/// A parsed panel with its strings, `STRT` table `id`, and its palette's
/// `style` ([`Style::find`]).
#[must_use]
pub fn resolved(panel: &Panel, text: &Text<'_>, id: FourCc, style: Option<Style>) -> Layout {
    let string = |index: Option<usize>| index.and_then(|i| text.at(&id.0, i)).map(Cow::into_owned);

    let shown = |c: &smnu::Control<'_>| {
        let colours = c.colours(smnu::TEXT_COLOURS).unwrap_or_default();
        let hotkey_colours = c.colours(smnu::HOTKEY_COLOURS).unwrap_or_default();

        ShownControl {
            kind: c.class(),
            id: c.id(),
            rect: c.rect(),
            text_rect: c.text_rect(),
            art: c.art(),
            font: c.font(),
            palette: c.palette(),
            label: c.label(),
            text: string(c.label()),
            tooltip: string(c.tooltip()),
            hotkey: c.hotkey(),
            colours: colours.to_vec(),
            hotkey_colours: hotkey_colours.to_vec(),
            inks: Inks::of((colours, hotkey_colours), style),
            flags: c.flags(),
            style: c.style(),
        }
    };

    let blocks = panel.controls();
    // `Panel::controls` puts the panel's own block first.
    let mut all = blocks.iter().map(shown);

    Layout {
        id,
        panel: all.next().unwrap_or_default(),
        centred: panel.centred(),
        controls: all.collect(),
        style,
    }
}

/// The first character of `text` that is the hot key's, ASCII case
/// ignored: CINEMATIC's 'I' (73) is at 1. `None` for a key no letter
/// types, as Enter (13) and Escape (27).
///
/// **Binary**: `CYDialogItem` looks the key's character up in the
/// text and draws it in colour role 4. **Inferred**: the
/// first match, case ignored; every shipped label is upper case.
#[must_use]
pub fn hotkey_letter(text: &str, hotkey: i32) -> Option<usize> {
    let key = u8::try_from(hotkey)
        .ok()
        .filter(u8::is_ascii_alphanumeric)?;

    text.bytes().position(|b| b.eq_ignore_ascii_case(&key))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hot_key_letter_is_the_first_of_its_character() {
        assert_eq!(hotkey_letter("CINEMATIC", i32::from(b'I')), Some(1));
        assert_eq!(hotkey_letter("PLAY GAME", i32::from(b'P')), Some(0));
        assert_eq!(hotkey_letter("EXIT GAME", 27), None);
        assert_eq!(hotkey_letter("CONTINUE", 13), None);
    }
}
