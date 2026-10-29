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
use crate::fonts;
use crate::theme::{CanvasColors, Tokens};
use egui::{Color32, Sense, Stroke, Vec2};

/// Which gallery page to show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    /// Colour roles and canvas colours.
    Swatches,
    /// The five-size type scale in both families.
    Type,
}

impl Page {
    /// The page named on the command line, or `None`.
    pub fn from_name(name: &str) -> Option<Page> {
        match name {
            "swatches" => Some(Page::Swatches),
            "type" => Some(Page::Type),
            _ => None,
        }
    }
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
        ui.label(fonts::caption(name).color(t.muted));
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
    ui.label(fonts::title(format!("Colour roles, {:?}", t.theme)));
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
    ui.label(fonts::title("Canvas colours"));
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

/// Draw the type scale page: every role, with its size and weight named.
///
/// Why: proof that the bundled fonts load and that only the five sizes exist.
pub fn type_scale(ui: &mut egui::Ui, state: &GalleryState) {
    let t = &state.tokens;
    ui.label(fonts::title(format!("Type scale, {:?}", t.theme)));
    ui.add_space(8.0);
    let rows: [(&str, fonts::Role, &str); 7] = [
        (
            "Display 22 / 28, 600",
            fonts::Role::Display,
            "Layout and Manufacture",
        ),
        (
            "Title 16 / 24, 600",
            fonts::Role::Title,
            "Board outline and stackup",
        ),
        (
            "Strong 13 / 20, 600",
            fonts::Role::Strong,
            "VCC is not fully routed",
        ),
        (
            "Button 13 / 20, 500",
            fonts::Role::Button,
            "Run checks   Update layout",
        ),
        (
            "Body 13 / 20, 400",
            fonts::Role::Body,
            "Copper does not join J1 pin 1 and R1 pin 1.",
        ),
        (
            "Mono 12 / 16, 400",
            fonts::Role::Mono,
            "44.00 mm   R1 330 \u{3a9}   GND   gerber-top.gbr",
        ),
        (
            "Caption 11 / 16, 400",
            fonts::Role::Caption,
            "Include in parts list, placement file",
        ),
    ];
    for (name, role, sample) in rows {
        ui.horizontal(|ui| {
            ui.allocate_ui_with_layout(
                Vec2::new(200.0, 28.0),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| ui.label(fonts::caption(name).color(t.muted)),
            );
            ui.label(fonts::text(role, sample));
        });
        ui.add_space(4.0);
    }
    ui.add_space(8.0);
    ui.label(fonts::section_caption("General").color(t.muted));
    ui.label(fonts::caption("Section caption: 11, 600, uppercase").color(t.muted));
}

/// Draw the page named in `state`.
pub fn show(ui: &mut egui::Ui, state: &mut GalleryState) {
    match state.page {
        Page::Swatches => swatches(ui, state),
        Page::Type => type_scale(ui, state),
    }
}
