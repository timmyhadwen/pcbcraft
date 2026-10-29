//! Chip and badge: the small labelled shapes of the design.
//!
//! Context: design system 5.15 (chip) and 5.5 (count badge, status badge).
//! Local rules: a chip holds Mono text for a net, pin or file kind; a badge holds a
//! short count or status word with a tint that matches its meaning.
//!
//! # Dependencies
//! - crates/ui-egui/src/theme.rs
//! - crates/ui-egui/src/fonts.rs
//! - crates/ui-egui/src/ui_kit/focus.rs
//!
//! # Do not
//! - Show state by colour alone; badge text always says the state in words or a count.
use super::focus;
use crate::fonts::Role;
use crate::theme::{self, Tokens};
use egui::{
    Color32, CornerRadius, Response, Sense, Stroke, StrokeKind, Ui, Vec2, WidgetInfo, WidgetType,
};

/// Chip height in points.
pub const CHIP_HEIGHT: f32 = 20.0;
/// Badge height in points.
pub const BADGE_HEIGHT: f32 = 18.0;

/// Meaning of a badge, which picks its tint and text colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BadgeKind {
    /// Counts and plain status on a raised fill.
    Neutral,
    /// Warnings: gold on its tint.
    Warning,
    /// Errors: red on its tint.
    Error,
    /// Success: mask green on its tint.
    Success,
    /// Active or current item: copper on its tint.
    Accent,
}

/// The fill and text colour of a badge kind.
///
/// Why: the gallery and the contrast gate need the same pairs the widget paints.
pub fn badge_colors(kind: BadgeKind, t: &Tokens) -> (Color32, Color32) {
    match kind {
        BadgeKind::Neutral => (t.raised, t.muted),
        BadgeKind::Warning => (t.gold_tint, t.gold),
        BadgeKind::Error => (t.red_tint, t.red),
        BadgeKind::Success => (t.mask_tint, t.mask),
        BadgeKind::Accent => (t.copper_tint, t.copper),
    }
}

/// A chip: Mono 12 text on a raised fill with a 1 px line border, 20 high.
///
/// Why: nets, pins and file kinds read as objects, and a chip can be pressed to
/// jump to the thing it names.
///
/// # Rules
/// - `tip` says what pressing it does, or what it names.
///
/// # Do not
/// - Put a sentence in a chip.
pub fn chip(ui: &mut Ui, text: &str, tip: &str) -> Response {
    let t = theme::tokens(ui.ctx());
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_owned(), Role::Mono.font(), t.text);
    let size = Vec2::new(galley.size().x + 12.0, CHIP_HEIGHT);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, text));
    let fill = if response.hovered() {
        t.hover
    } else {
        t.raised
    };
    let radius = CornerRadius::same(theme::RADIUS_CONTROL);
    ui.painter().rect_filled(rect, radius, fill);
    ui.painter().rect_stroke(
        rect,
        radius,
        Stroke::new(theme::BORDER, t.line),
        StrokeKind::Inside,
    );
    let pos = egui::pos2(rect.min.x + 6.0, rect.center().y - galley.size().y / 2.0);
    ui.painter().galley(pos, galley, t.text);
    focus::focus_ring(ui, &response, theme::RADIUS_CONTROL);
    response.on_hover_text(tip)
}

/// A badge: an 18 high pill with a count or a status word.
///
/// Why: finding counts on tabs and Current or Stale on outputs.
///
/// # Rules
/// - Not interactive; the accessible name is its text.
pub fn badge(ui: &mut Ui, text: &str, kind: BadgeKind, tip: &str) -> Response {
    let t = theme::tokens(ui.ctx());
    let (fill, ink) = badge_colors(kind, &t);
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_owned(), Role::Caption.font(), ink);
    let width = (galley.size().x + 12.0).max(BADGE_HEIGHT);
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, BADGE_HEIGHT), Sense::hover());
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Label, true, text));
    ui.painter()
        .rect_filled(rect, CornerRadius::same((BADGE_HEIGHT / 2.0) as u8), fill);
    let pos = egui::pos2(
        rect.center().x - galley.size().x / 2.0,
        rect.center().y - galley.size().y / 2.0,
    );
    ui.painter().galley(pos, galley, ink);
    response.on_hover_text(tip)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Chips are 20 high and badges 18 high.
    #[test]
    fn chip_and_badge_heights() {
        let ctx = egui::Context::default();
        theme::apply(&ctx, &Tokens::DARK);
        let mut heights = Vec::new();
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            heights.push(chip(ui, "VCC", "Net VCC").rect.height());
            heights.push(badge(ui, "3", BadgeKind::Warning, "Warnings").rect.height());
        });
        out.textures_delta.clear();
        assert_eq!(heights, vec![20.0, 18.0]);
    }
}
