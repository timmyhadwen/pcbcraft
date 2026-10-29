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
use super::button::{self, Kind, Look, Size, State};
use super::chip::{self, BadgeKind};
use super::icons;
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
    /// Buttons in every kind, size and state, plus chips and badges.
    Buttons,
}

impl Page {
    /// The page named on the command line, or `None`.
    pub fn from_name(name: &str) -> Option<Page> {
        match name {
            "swatches" => Some(Page::Swatches),
            "type" => Some(Page::Type),
            "buttons" => Some(Page::Buttons),
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

const STATES: [(&str, State); 5] = [
    ("Rest", State::Rest),
    ("Hover", State::Hover),
    ("Active", State::Active),
    ("Disabled", State::Disabled),
    ("Focus", State::Focus),
];

fn state_header(ui: &mut egui::Ui, t: &Tokens) {
    ui.horizontal(|ui| {
        ui.allocate_space(Vec2::new(96.0, 20.0));
        for (name, _) in STATES {
            ui.allocate_ui_with_layout(
                Vec2::new(144.0, 20.0),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| ui.label(fonts::section_caption(name).color(t.muted)),
            );
        }
    });
}

fn state_row(ui: &mut egui::Ui, t: &Tokens, name: &str, look: Look<'_>) {
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(
            Vec2::new(96.0, 36.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| ui.label(fonts::caption(name).color(t.muted)),
        );
        for (_, state) in STATES {
            ui.allocate_ui_with_layout(
                Vec2::new(144.0, 36.0),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.add_space(4.0);
                    button::static_button(ui, look, state);
                },
            );
        }
    });
}

/// Draw the buttons page: every kind in every state, three sizes, a live row,
/// chips and badges.
///
/// Why: the visual proof for ticket #761.
pub fn buttons(ui: &mut egui::Ui, state: &GalleryState) {
    let t = &state.tokens;
    ui.label(fonts::title(format!("Buttons, {:?}", t.theme)));
    ui.add_space(8.0);
    state_header(ui, t);
    let plus = Some(icons::PLUS);
    let rows: [(&str, Look<'_>); 7] = [
        (
            "Primary",
            Look {
                kind: Kind::Primary,
                size: Size::Normal,
                icon: None,
                label: "Save",
            },
        ),
        (
            "Primary, icon",
            Look {
                kind: Kind::Primary,
                size: Size::Normal,
                icon: plus,
                label: "Add part",
            },
        ),
        (
            "Secondary",
            Look {
                kind: Kind::Secondary,
                size: Size::Normal,
                icon: None,
                label: "Cancel",
            },
        ),
        (
            "Secondary, icon",
            Look {
                kind: Kind::Secondary,
                size: Size::Normal,
                icon: Some(icons::FIT),
                label: "Fit",
            },
        ),
        (
            "Ghost",
            Look {
                kind: Kind::Ghost,
                size: Size::Normal,
                icon: None,
                label: "Hide",
            },
        ),
        (
            "Icon",
            Look {
                kind: Kind::Icon,
                size: Size::Normal,
                icon: Some(icons::MORE),
                label: "",
            },
        ),
        (
            "Icon, zoom",
            Look {
                kind: Kind::Icon,
                size: Size::Normal,
                icon: Some(icons::ZOOM_IN),
                label: "",
            },
        ),
    ];
    for (name, look) in rows {
        state_row(ui, t, name, look);
    }
    ui.add_space(16.0);
    ui.label(fonts::title("Sizes"));
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        for (name, size) in [
            ("Small 24", Size::Small),
            ("Normal 28", Size::Normal),
            ("Large 32", Size::Large),
        ] {
            ui.vertical(|ui| {
                ui.label(fonts::caption(name).color(t.muted));
                button::primary_button(
                    ui,
                    None,
                    "Update layout",
                    "Bring the board in line with the schematic",
                    size,
                    true,
                );
                button::secondary_button(
                    ui,
                    None,
                    "Run checks",
                    "Check the board against its rules",
                    size,
                    true,
                );
                button::ghost_button(
                    ui,
                    None,
                    "Mute",
                    "Hide this finding until the next run",
                    size,
                    true,
                );
                button::icon_button(ui, icons::PLUS, "Add", "Add a layer", size, true);
            });
            ui.add_space(24.0);
        }
    });
    ui.add_space(16.0);
    ui.label(fonts::title("Chips and badges"));
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        chip::chip(ui, "VCC", "Net VCC, press to highlight it");
        chip::chip(ui, "R1.2", "Pin 2 of R1, press to select it");
        chip::chip(ui, "gerber-top.gbr", "Top copper file");
        ui.add_space(16.0);
        chip::badge(ui, "3", BadgeKind::Warning, "Three warnings");
        chip::badge(ui, "0", BadgeKind::Neutral, "No muted findings");
        chip::badge(ui, "Current", BadgeKind::Success, "Output is up to date");
        chip::badge(
            ui,
            "Stale",
            BadgeKind::Warning,
            "Output is older than the board",
        );
        chip::badge(ui, "2 errors", BadgeKind::Error, "Two errors");
        chip::badge(ui, "Active", BadgeKind::Accent, "Active layer");
    });
}

/// Draw the page named in `state`.
pub fn show(ui: &mut egui::Ui, state: &mut GalleryState) {
    match state.page {
        Page::Swatches => swatches(ui, state),
        Page::Type => type_scale(ui, state),
        Page::Buttons => buttons(ui, state),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::a11y::audit;
    use crate::theme;

    /// Run every gallery page once, with the accessibility tree on, and return
    /// the interactive nodes as (role, name).
    fn interactive(dark: bool) -> Vec<(egui::accesskit::Role, String)> {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut state = GalleryState::new(dark);
        theme::apply(&ctx, &state.tokens);
        crate::fonts::install(&ctx);
        let mut nodes = Vec::new();
        for page in [Page::Swatches, Page::Type, Page::Buttons] {
            state.page = page;
            for pass in 0..2 {
                let mut out = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            Vec2::new(1280.0, 900.0),
                        )),
                        ..Default::default()
                    },
                    |ui| show(ui, &mut state),
                );
                if pass == 1 {
                    nodes.extend(audit::interactive_nodes(&out));
                }
                out.textures_delta.clear();
            }
        }
        nodes
    }

    /// Every live control in the gallery announces a name, in both themes.
    #[test]
    fn every_kit_control_has_an_accessible_name() {
        for dark in [true, false] {
            let nodes = interactive(dark);
            assert!(nodes.len() >= 15, "only {} interactive nodes", nodes.len());
            for (role, name) in nodes {
                assert!(!name.is_empty(), "{role:?} has no accessible name");
            }
        }
    }
}
