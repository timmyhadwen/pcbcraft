//! The classic menu bar: File Edit View Project Place Design Tools Reports
//! Window Help, with each item's shortcut on the right and the letter of its
//! two-key accelerator underlined (Place > Wire shows "P, W" and underlines W).
//!
//! The menus are built from the keymap, so a rebind shows up here at once,
//! and clicking an item runs the same `App::dispatch_shortcut` the keys run.
//!
//! # Rules
//! - Items for another workspace stay listed but greyed, with a tooltip that
//!   says which tab enables them.
//! - Every item carries a tooltip (docs/tooltips.md): what it does and its keys.
//! - No design change here; items call `dispatch_shortcut` or switch views.
//!
//! # Dependencies
//! - `crates/ui-egui/src/keymap.rs` names, tooltips and key text per action.
//! - `crates/ui-egui/src/lib.rs` `App::header` places the bar; File, Edit,
//!   View and Help keep their own builders there.
//!
//! # Do not
//! - Add a menu item for an action the app cannot run; list it as missing.

use super::*;
use crate::keymap::KeyAction;

impl App {
    /// The menus between View and Help.
    pub(super) fn classic_menus(&mut self, ui: &mut egui::Ui) {
        self.project_menu(ui);
        self.place_menu(ui);
        self.design_menu(ui);
        self.tools_menu(ui);
        self.reports_menu(ui);
        self.window_menu(ui);
    }

    /// One menu item that runs `action`, with its keys and accelerator.
    pub(super) fn action_item(&mut self, ui: &mut egui::Ui, action: KeyAction) {
        let map = self.preferences.active_keymap();
        let keys = map.menu_keys(action).unwrap_or_default();
        let accelerator = map.sequences_for(action).first().map(|s| s.second);
        let enabled = keymap::action_available(action, self.tab);
        let text = accelerated_label(ui, keymap::action_name(action), accelerator);
        let tip = format!(
            "{}\n{}",
            keymap::action_tooltip(action),
            map.keys_text(action)
        );
        let response = ui
            .add_enabled(enabled, egui::Button::new(text).shortcut_text(keys))
            .on_hover_text(tip)
            .on_disabled_hover_text(t!("menu.classic.other_tab"));
        if response.clicked() {
            self.dispatch_shortcut(action);
            ui.close();
        }
    }

    fn page_item(&mut self, ui: &mut egui::Ui, tab: Tab, label: String, tip: String) {
        if ui.button(label).on_hover_text(tip).clicked() {
            self.change_tab(tab);
            ui.close();
        }
    }

    fn project_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button(t!("menu.project"), |ui| {
            self.action_item(ui, KeyAction::UpdateBoardFromSchematic);
            self.action_item(ui, KeyAction::ToggleAutoSync);
            ui.separator();
            self.page_item(
                ui,
                Tab::Checks,
                t!("menu.view.checks"),
                t!("menu.view.checks.tip"),
            );
        })
        .response
        .on_hover_text(t!("menu.project.tip"));
    }

    fn place_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button(t!("menu.place"), |ui| {
            let schematic = [
                KeyAction::SymbolPalette,
                KeyAction::WireTool,
                KeyAction::PlaceNetLabel,
                KeyAction::PlacePowerPort,
                KeyAction::PlaceJunction,
            ];
            let board = [
                KeyAction::RouteTool,
                KeyAction::ViaTool,
                KeyAction::PlaceFootprint,
                KeyAction::TextTool,
            ];
            // The current workspace's items come first.
            let (first, second) = if self.tab == Tab::Pcb {
                (&board[..], &schematic[..])
            } else {
                (&schematic[..], &board[..])
            };
            for action in first {
                self.action_item(ui, *action);
            }
            ui.separator();
            for action in second {
                self.action_item(ui, *action);
            }
        })
        .response
        .on_hover_text(t!("menu.place.tip"));
    }

    fn design_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button(t!("menu.design"), |ui| {
            self.action_item(ui, KeyAction::UpdateBoardFromSchematic);
            ui.separator();
            self.page_item(
                ui,
                Tab::Rules,
                t!("menu.view.rules"),
                t!("menu.view.rules.tip"),
            );
            self.action_item(ui, KeyAction::LayerPanel);
        })
        .response
        .on_hover_text(t!("menu.design.tip"));
    }

    fn tools_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button(t!("menu.tools"), |ui| {
            self.action_item(ui, KeyAction::CommandPalette);
            self.action_item(ui, KeyAction::RepeatLastCommand);
            ui.separator();
            self.action_item(ui, KeyAction::CycleCornerMode);
            self.action_item(ui, KeyAction::CycleRouteMode);
            self.action_item(ui, KeyAction::SelectConnected);
            self.action_item(ui, KeyAction::ToggleNetHighlight);
            ui.separator();
            self.action_item(ui, KeyAction::ToggleAssistant);
            self.page_item(
                ui,
                Tab::Settings,
                t!("menu.edit.preferences"),
                t!("menu.edit.preferences.tip"),
            );
        })
        .response
        .on_hover_text(t!("menu.tools.tip"));
    }

    fn reports_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button(t!("menu.reports"), |ui| {
            self.action_item(ui, KeyAction::Measure);
            ui.separator();
            self.page_item(
                ui,
                Tab::Checks,
                t!("menu.view.checks"),
                t!("menu.view.checks.tip"),
            );
        })
        .response
        .on_hover_text(t!("menu.reports.tip"));
    }

    fn window_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button(t!("menu.window"), |ui| {
            for (tab, label) in [
                (Tab::Schematic, t!("docks.workbench_tabs.schematic")),
                (Tab::Pcb, t!("docks.workbench_tabs.layout")),
                (Tab::Files, t!("docks.workbench_tabs.files")),
                (Tab::PlotFiles, t!("docks.workbench_tabs.manufacture")),
            ] {
                let tip = t!(
                    "docks.workbench_tabs.switch_to_the_label_view2",
                    label = label.clone()
                );
                self.page_item(ui, tab, label, tip);
            }
            ui.separator();
            if ui
                .button(t!("menu.view.reset_layout"))
                .on_hover_text(t!("menu.view.reset_layout.tip"))
                .clicked()
            {
                self.reset_layout();
                ui.close();
            }
        })
        .response
        .on_hover_text(t!("menu.window.tip"));
    }

    /// The View menu's zoom, grid and layer items, above the page links.
    pub(super) fn view_actions(&mut self, ui: &mut egui::Ui) {
        for action in [
            KeyAction::ZoomFitDesign,
            KeyAction::ZoomToSelection,
            KeyAction::ZoomIn,
            KeyAction::ZoomOut,
            KeyAction::Redraw,
        ] {
            self.action_item(ui, action);
        }
        ui.separator();
        for action in [
            KeyAction::CycleGrid,
            KeyAction::ToggleUnits,
            KeyAction::LayerNext,
            KeyAction::LayerPrev,
            KeyAction::CopperLayerNext,
            KeyAction::SingleLayerMode,
        ] {
            self.action_item(ui, action);
        }
        ui.separator();
    }

    /// The Edit menu's selection items, below Undo and Redo.
    pub(super) fn edit_actions(&mut self, ui: &mut egui::Ui) {
        for action in [
            KeyAction::DeleteSelection,
            KeyAction::MoveTool,
            KeyAction::Rotate,
            KeyAction::FlipHorizontal,
            KeyAction::MirrorVertical,
            KeyAction::OpenInspector,
            KeyAction::FindInTree,
        ] {
            self.action_item(ui, action);
        }
    }
}

/// `name` with the first `accelerator` letter underlined, as menus show it.
fn accelerated_label(
    ui: &egui::Ui,
    name: &str,
    accelerator: Option<egui::Key>,
) -> egui::WidgetText {
    let Some(key) = accelerator else {
        return name.into();
    };
    let letter = keymap::key_name(key);
    let Some(at) = name.to_ascii_lowercase().find(&letter.to_ascii_lowercase()) else {
        return name.into();
    };
    let end = at + letter.len();
    let font = egui::TextStyle::Button.resolve(ui.style());
    // PLACEHOLDER takes the button's own colour, so a disabled item still greys.
    let plain = egui::TextFormat::simple(font.clone(), egui::Color32::PLACEHOLDER);
    let underlined = egui::TextFormat {
        underline: egui::Stroke::new(1.0, ui.visuals().text_color()),
        ..egui::TextFormat::simple(font, egui::Color32::PLACEHOLDER)
    };
    let mut job = egui::text::LayoutJob::default();
    job.append(&name[..at], 0.0, plain.clone());
    job.append(&name[at..end], 0.0, underlined);
    job.append(&name[end..], 0.0, plain);
    job.into()
}
