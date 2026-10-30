//! The layer tabs along the bottom edge of the board canvas: an LS button for
//! single layer mode, then one tab per layer in stack order (copper top to
//! bottom, silkscreen, paste, mask, the designer's own layers and the board
//! edge), each with a chip in the colour the board draws that layer in.
//! Clicking a tab makes that layer the drawing target.
//!
//! Context: `App::canvas` in `crates/ui-egui/src/canvas.rs` keeps a strip of
//! [`LAYER_TABS_HEIGHT`] under the board and calls [`App::layer_tabs`]. The
//! Layers tree (`crates/ui-egui/src/layers_tree.rs`) stays the full view of
//! the stack; these tabs are the quick switch.
//!
//! # Rules
//! - A click goes through `App::set_target` (`crates/ui-egui/src/draw_target.rs`),
//!   the same setter the Layers tree uses, so the two never disagree.
//! - Layer colours come from `BoardArt`; chrome colours from the theme tokens.
//! - Every tab is at least `App::MIN_TARGET` tall and carries a tooltip.
//!
//! # Dependencies
//! - `crates/ui-egui/src/key_commands.rs` single layer mode (Shift+S).
//!
//! # Do not
//! - Edit the document here; choosing a layer is a view and tool setting.
use super::*;
use crate::draw_target::{Side, Target};
use crate::keymap::KeyAction;

/// Height of the tab strip under the board canvas.
pub(crate) const LAYER_TABS_HEIGHT: f32 = 28.0;
/// Side of the colour chip on each tab.
const CHIP: f32 = 9.0;

/// One tab of the strip.
struct LayerTab {
    target: Target,
    name: String,
    colour: Color32,
    /// False when the layer is hidden in the Layers tree; the tab dims.
    shown: bool,
}

impl App {
    /// The colour the board draws `target` in.
    pub(crate) fn target_colour(&self, target: Target) -> Color32 {
        let art = self.art();
        match target {
            Target::Copper(layer) => art.copper(layer, self.project.copper_layer_count()),
            Target::Silk(Side::Top) => art.silk,
            Target::Silk(Side::Bottom) => art.silk_bottom,
            Target::Mask(Side::Top) => art.mask_top,
            Target::Mask(Side::Bottom) => art.mask_bottom,
            Target::Paste(Side::Top) => art.paste_top,
            Target::Paste(Side::Bottom) => art.paste_bottom,
            Target::Edge => art.edge,
            Target::User(id) => self
                .project
                .board
                .user_layers
                .iter()
                .find(|l| l.id == id)
                .map_or(art.muted, |l| self.user_layer_colour(l.kind)),
        }
    }

    /// Every tab in stack order, top of the board first.
    fn layer_tab_list(&self) -> Vec<LayerTab> {
        let count = self.project.copper_layer_count().max(2);
        let mut tabs: Vec<LayerTab> = (0..count)
            .filter_map(|i| Layer::from_index(i).ok())
            .map(|layer| LayerTab {
                target: Target::Copper(layer),
                name: self.project.layer_name(layer),
                colour: self.target_colour(Target::Copper(layer)),
                shown: self.layer_shown(if layer == Layer::Front {
                    "Top copper"
                } else {
                    "Bottom copper"
                }),
            })
            .collect();
        for (target, name) in [
            (Target::Silk(Side::Top), t!("canvas.layer_tabs.top_silk")),
            (
                Target::Silk(Side::Bottom),
                t!("canvas.layer_tabs.bottom_silk"),
            ),
            (Target::Paste(Side::Top), t!("canvas.layer_tabs.top_paste")),
            (
                Target::Paste(Side::Bottom),
                t!("canvas.layer_tabs.bottom_paste"),
            ),
            (Target::Mask(Side::Top), t!("canvas.layer_tabs.top_mask")),
            (
                Target::Mask(Side::Bottom),
                t!("canvas.layer_tabs.bottom_mask"),
            ),
        ] {
            tabs.push(LayerTab {
                target,
                name,
                colour: self.target_colour(target),
                shown: true,
            });
        }
        for layer in &self.project.board.user_layers {
            tabs.push(LayerTab {
                target: Target::User(layer.id),
                name: layer.name.clone(),
                colour: self.user_layer_colour(layer.kind),
                shown: layer.visible,
            });
        }
        tabs.push(LayerTab {
            target: Target::Edge,
            name: t!("tree.layer.edge"),
            colour: self.target_colour(Target::Edge),
            shown: true,
        });
        tabs
    }

    /// Make `target` the drawing layer from a tab; single layer mode, when
    /// on, moves to the new layer.
    pub(crate) fn pick_layer_tab(&mut self, target: Target) {
        self.set_target(target);
        if self.focus_layer.is_some() {
            // Off and on again: the focus follows the new layer.
            self.dispatch_shortcut(KeyAction::SingleLayerMode);
            self.dispatch_shortcut(KeyAction::SingleLayerMode);
        }
        self.message = t!("canvas.layer_tabs.picked", name = self.target_label());
    }

    /// The strip of layer tabs in `strip`, under the board canvas.
    ///
    /// # Rules
    /// - Scrolls sideways when the window is too narrow for every tab.
    pub(super) fn layer_tabs(&mut self, ui: &mut egui::Ui, strip: Rect) {
        let tokens = crate::theme::tokens(ui.ctx());
        ui.painter().rect_filled(strip, 0.0, tokens.panel);
        ui.painter().line_segment(
            [strip.left_top(), strip.right_top()],
            Stroke::new(1.0, tokens.line),
        );
        let tabs = self.layer_tab_list();
        let current = self.current_target();
        let single = self.focus_layer.is_some();
        let font = crate::fonts::Role::Caption.font();
        let mut picked = None;
        let mut toggle_single = false;
        let mut unpin = false;
        let inner = Rect::from_min_max(strip.min + Vec2::new(0.0, 2.0), strip.max);
        ui.scope_builder(egui::UiBuilder::new().max_rect(inner), |ui| {
            egui::ScrollArea::horizontal()
                .id_salt("layer-tabs")
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 1.0;
                        let map = self.preferences.active_keymap();
                        let ls_tip = format!(
                            "{}\n{}",
                            keymap::action_tooltip(KeyAction::SingleLayerMode),
                            map.keys_text(KeyAction::SingleLayerMode)
                        );
                        let ls = tab_button(
                            ui,
                            &t!("canvas.layer_tabs.ls"),
                            None,
                            single,
                            true,
                            &font,
                            &tokens,
                        )
                        .on_hover_text(ls_tip);
                        crate::a11y::name_control(
                            &ls,
                            &t!("canvas.layer_tabs.ls.name"),
                            Some(single),
                        );
                        self.record_icon("layer-tabs-ls", ls.rect);
                        if ls.clicked() {
                            toggle_single = true;
                        }
                        ui.add_space(6.0);
                        for tab in &tabs {
                            let on = tab.target == current;
                            let response = tab_button(
                                ui,
                                &tab.name,
                                Some(tab.colour),
                                on,
                                tab.shown,
                                &font,
                                &tokens,
                            )
                            .on_hover_text(t!("canvas.layer_tabs.tip", name = tab.name.clone()));
                            crate::a11y::name_control(&response, &tab.name, Some(on));
                            self.record_icon(&format!("layer-tab:{}", tab.name), response.rect);
                            if response.clicked() {
                                picked = Some(tab.target);
                            }
                        }
                        if let Some(pinned) = self.pinned_view {
                            let saved = std::mem::replace(&mut self.target, pinned);
                            let name = self.target_label();
                            self.target = saved;
                            ui.add_space(12.0);
                            ui.label(
                                crate::fonts::caption(t!("view.viewing", layer = name))
                                    .color(tokens.muted),
                            )
                            .on_hover_text(t!("view.viewing.tip"));
                            if crate::icons::icon_button(ui, "close", &t!("view.unpin"), false)
                                .clicked()
                            {
                                unpin = true;
                            }
                        }
                    });
                });
        });
        if toggle_single {
            self.dispatch_shortcut(KeyAction::SingleLayerMode);
        }
        if let Some(target) = picked {
            self.pick_layer_tab(target);
        }
        if unpin {
            self.pinned_view = None;
        }
    }
}

/// One tab: an optional colour chip and the name. The current tab sits on a
/// raised fill with a line in its layer colour along the top.
fn tab_button(
    ui: &mut egui::Ui,
    name: &str,
    chip: Option<Color32>,
    on: bool,
    shown: bool,
    font: &FontId,
    tokens: &crate::theme::Tokens,
) -> egui::Response {
    let text_colour = match (on, shown) {
        (true, _) => tokens.text,
        (false, true) => tokens.muted,
        (false, false) => tokens.muted.gamma_multiply(0.5),
    };
    let galley = ui
        .painter()
        .layout_no_wrap(name.to_owned(), font.clone(), text_colour);
    let pad = 8.0;
    let chip_room = if chip.is_some() { CHIP + 5.0 } else { 0.0 };
    let width = (galley.size().x + chip_room + pad * 2.0).max(App::MIN_TARGET);
    let height = (LAYER_TABS_HEIGHT - 2.0).max(App::MIN_TARGET);
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
    let painter = ui.painter();
    if on {
        painter.rect_filled(rect, 0.0, tokens.raised);
    } else if response.hovered() {
        painter.rect_filled(rect, 0.0, tokens.hover);
    }
    if chip.is_none() {
        // A tab without a layer is a mode switch: it reads as a button.
        painter.rect_stroke(
            rect.shrink2(Vec2::new(2.0, 3.0)),
            2.0,
            Stroke::new(1.0, tokens.edge),
            egui::StrokeKind::Inside,
        );
    }
    let accent = chip.unwrap_or(tokens.text);
    if on {
        painter.line_segment(
            [rect.left_top(), rect.right_top()],
            Stroke::new(2.0, accent),
        );
    }
    let mut x = rect.left() + pad;
    if let Some(colour) = chip {
        let colour = if shown {
            colour
        } else {
            colour.gamma_multiply(0.35)
        };
        let swatch = Rect::from_center_size(
            Pos2::new(x + CHIP / 2.0, rect.center().y),
            Vec2::splat(CHIP),
        );
        painter.rect_filled(swatch, 1.0, colour);
        x += chip_room;
    }
    // A tab cut by the edge of a narrow window shows its chip only; the
    // strip scrolls to bring its name into view.
    if ui.clip_rect().right() >= rect.right() {
        let y = rect.center().y - galley.size().y / 2.0;
        painter.galley(Pos2::new(x, y), galley, text_colour);
    }
    response
}
