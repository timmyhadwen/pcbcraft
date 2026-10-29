//! The component gallery: every kit component, every state, drawn on one page.
//!
//! Context: the `ui_kit_gallery` example runs this under eframe and captures a
//! PNG per theme; the unit tests run the same page headless.
//! Local rules: the gallery only calls public kit helpers and `theme`; it holds
//! no app state beyond `GalleryState`.
//!
//! # Dependencies
//! - crates/ui-egui/src/theme.rs
//! - crates/ui-egui/examples/ui_kit_gallery.rs
//!
//! # Do not
//! - Add a component here that the kit does not export.
use crate::theme::{CanvasColors, Tokens};
use egui::{Color32, RichText, Sense, Stroke, Vec2};

/// Which gallery page to show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    /// Colour roles and canvas colours.
    Swatches,
}

/// State the gallery keeps between frames.
#[derive(Clone, Debug)]
pub struct GalleryState {
    /// Tokens of the theme on show.
    pub tokens: Tokens,
    /// The page on show.
    pub page: Page,
}

impl GalleryState {
    /// A gallery for one theme.
    pub fn new(dark: bool) -> Self {
        Self {
            tokens: Tokens::for_dark(dark),
            page: Page::Swatches,
        }
    }
}

fn swatch(ui: &mut egui::Ui, t: &Tokens, name: &str, color: Color32) {
    ui.vertical(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(96.0, 40.0), Sense::hover());
        ui.painter().rect_filled(rect, 4.0, color);
        ui.painter().rect_stroke(
            rect,
            4.0,
            Stroke::new(1.0, t.line),
            egui::StrokeKind::Inside,
        );
        ui.label(RichText::new(name).small().color(t.muted));
    });
}

fn swatch_rows(ui: &mut egui::Ui, t: &Tokens, items: &[(&str, Color32)]) {
    for row in items.chunks(9) {
        ui.horizontal(|ui| {
            for (name, color) in row {
                swatch(ui, t, name, *color);
            }
        });
        ui.add_space(8.0);
    }
}

/// Draw the colour swatch page for the tokens in `state`.
///
/// Why: the first visual proof that both themes carry the right values.
pub fn swatches(ui: &mut egui::Ui, state: &GalleryState) {
    let t = &state.tokens;
    ui.heading(format!("Colour roles, {:?}", t.theme));
    ui.add_space(8.0);
    let roles: [(&str, Color32); 18] = [
        ("ground", t.ground),
        ("panel", t.panel),
        ("raised", t.raised),
        ("hover", t.hover),
        ("line", t.line),
        ("edge", t.edge),
        ("text", t.text),
        ("muted", t.muted),
        ("copper", t.copper),
        ("mask", t.mask),
        ("gold", t.gold),
        ("cyan", t.cyan),
        ("red", t.red),
        ("onacc", t.onacc),
        ("copper tint", t.copper_tint),
        ("gold tint", t.gold_tint),
        ("red tint", t.red_tint),
        ("mask tint", t.mask_tint),
    ];
    swatch_rows(ui, t, &roles);
    ui.add_space(16.0);
    ui.heading("Canvas colours");
    ui.add_space(8.0);
    let c = CanvasColors::for_dark(t.is_dark());
    let canvas: [(&str, Color32); 15] = [
        ("background", c.background),
        ("grid minor", c.grid_minor),
        ("grid major", c.grid_major),
        ("board fill", c.board_fill),
        ("board edge", c.board_edge),
        ("top copper", c.top_copper),
        ("bottom copper", c.bottom_copper),
        ("silkscreen", c.silkscreen),
        ("courtyard", c.courtyard),
        ("ratsnest", c.ratsnest),
        ("selection", c.selection),
        ("wire", c.schematic_wire),
        ("symbol body", c.symbol_body),
        ("pin number", c.pin_number),
        ("sheet fill", c.sheet_fill),
    ];
    swatch_rows(ui, t, &canvas);
}

/// Draw the page named in `state`.
pub fn show(ui: &mut egui::Ui, state: &mut GalleryState) {
    match state.page {
        Page::Swatches => swatches(ui, state),
    }
}
