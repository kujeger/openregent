//! The tooltip line in the bottom bar (`regent_core::view::panel::tip`): a control
//! drawn this pass offers its text while hovered; after every panel, the
//! line shows the last offered, as the original's manager decides.
//!
//! ```text
//!  begin ──> no offer
//!  the map's unit under the pointer (cursor::Tipped) ──> offer_as
//!  Canvas::tip, hovered ──> offer (its press area, its text)
//!  show ──> Line::step ──> the text over APMK 2004, fnt7, raised
//! ```

use bevy_egui::egui::{self, Color32, Id, Response, pos2, vec2};
use regent_core::view::menu::Layout;
use regent_core::view::panel::tip::{self, Buttons, Line, Pointer, Rows};

use super::bars;
use super::canvas::{Align, Canvas};
use crate::ui::layout::Frame;

/// Where this pass's offer is kept, in egui's frame data.
const OFFER: &str = "tip line offer";

/// The text's colour ([`tip::COLOUR`]).
const COLOUR: Color32 = Color32::from_rgb(tip::COLOUR.r, tip::COLOUR.g, tip::COLOUR.b);

/// The control under the pointer with a text: its press area and text.
#[derive(Clone)]
struct Offer {
    id: Id,
    text: String,
}

/// A pass begins with no offer.
pub fn begin(ctx: &egui::Context) {
    ctx.data_mut(|d| d.remove::<Offer>(Id::new(OFFER)));
}

/// `text` for the control `response` presses while the pointer is on
/// it; a control drawn later, above, wins.
pub fn offer(response: &Response, text: Option<&str>) {
    if response.hovered() {
        offer_as(&response.ctx, response.id, text);
    }
}

/// `text` for what `id` names under the pointer: the map's unit, offered
/// before any panel's control, which then wins.
pub fn offer_as(ctx: &egui::Context, id: Id, text: Option<&str>) {
    if let Some(text) = text.filter(|t| !t.is_empty()) {
        let offer = Offer {
            id,
            text: text.to_owned(),
        };

        ctx.data_mut(|d| d.insert_temp(Id::new(OFFER), offer));
    }
}

/// The line, where the manager shows it: over `main`'s 2004 in the
/// bottom bar, its bottom at the box's, wrapped at its width.
pub fn show(canvas: &mut Canvas<'_>, (main, frame): (&Layout, &Frame), line: &mut Line<Id>) {
    let ctx = canvas.ui.ctx().clone();
    let offer = ctx.data(|d| d.get_temp::<Offer>(Id::new(OFFER)));
    let (pointer, buttons) = ctx.input(|i| {
        let pointer = if i.pointer.delta() == egui::Vec2::ZERO {
            Pointer::Rested
        } else {
            Pointer::Moved
        };
        let buttons = if i.pointer.any_down() {
            Buttons::Down
        } else {
            Buttons::Up
        };

        (pointer, buttons)
    });
    let shows = line.step(offer.as_ref().map(|o| o.id), pointer, buttons);

    let (Some(offer), true, Some(c)) = (offer, shows, main.control(tip::PLACE)) else {
        return;
    };
    let Some((fit, place)) = bars::fitted(frame, c) else {
        return;
    };
    let stretched = bars::stretched(frame, c).unwrap_or(place.width());

    canvas.fit = fit;

    // The text box is the window less its margins; Canvas pads by as much.
    let margin = pixels(tip::MARGIN);
    let width = stretched + 2.0 * margin;
    let rows = canvas.lines(tip::FONT, width, &offer.text);
    #[allow(clippy::cast_precision_loss, reason = "a few rows")]
    let height = rows as f32 * canvas.pitch(tip::FONT);
    let raised = match rows {
        1 => Rows::One,
        _ => Rows::More,
    };
    #[allow(clippy::cast_possible_truncation, reason = "panel pixels")]
    let top = tip::top(place.height() as i32, height.round() as i32, raised);
    let rect = egui::Rect::from_min_size(pos2(0.0, pixels(top)), vec2(width, height));

    canvas.paragraphs(tip::FONT, rect, &offer.text, (COLOUR, Align::Left, 0.0));
}

/// Panel pixels as a float.
#[allow(clippy::cast_precision_loss, reason = "panel pixels")]
const fn pixels(n: i32) -> f32 {
    n as f32
}
