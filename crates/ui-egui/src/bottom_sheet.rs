//! The bottom sheet: a collapsible panel under the canvas with the tabs
//! Findings, Output and Console.
//!
//! Context: it replaces the old Checks page (epic #758, tickets #769, #779).
//! Collapsed it is a 36 px tab strip; View > Checks and the status bar counts
//! open it on Findings.
//!
//! Local rules: all words come from `crates/text/strings/en.txt`; every
//! control carries a tooltip; findings are read from `App::checks` and
//! changed only through engine commands (`rules.exclude`,
//! `rules.exclude_region`).

use super::*;

/// The tabs of the bottom sheet.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum SheetTab {
    #[default]
    Findings,
    Output,
    Console,
}

impl SheetTab {
    /// The region tab id this variant answers to.
    fn id(self) -> &'static str {
        match self {
            SheetTab::Findings => "findings",
            SheetTab::Output => "output",
            SheetTab::Console => "console",
        }
    }
    fn from_id(id: &str) -> Self {
        match id {
            "output" => SheetTab::Output,
            "console" => SheetTab::Console,
            _ => SheetTab::Findings,
        }
    }
}

/// Which findings the Findings tab lists.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum SeverityFilter {
    #[default]
    All,
    Errors,
    Warnings,
    Notes,
}

/// State of the bottom sheet.
#[derive(Clone, Debug, Default)]
pub(crate) struct SheetState {
    pub(crate) filter: SeverityFilter,
}

/// One finding flattened for display.
struct Row {
    key: &'static str,
    code: String,
    message: String,
    objects: Vec<Id>,
    /// 0 error, 1 warning, 2 note.
    rank: u8,
}

impl App {
    /// Height of the tab strip, and of the sheet when collapsed.
    const SHEET_STRIP: f32 = 36.0;
    /// Height of the sheet when open.
    const SHEET_OPEN: f32 = 232.0;

    /// Opens the sheet on a tab.
    ///
    /// # Rules
    /// Used by View > Checks and the status bar counts.
    pub(crate) fn open_sheet(&mut self, tab: SheetTab) {
        let region = self.region();
        region.bottom_open = true;
        region.bottom_tab = tab.id().to_owned();
    }

    fn sheet_rows(&self) -> Vec<Row> {
        let mut rows = Vec::new();
        for (key, name) in [("erc", "erc"), ("drc", "drc")] {
            let Some(list) = self.checks.get(name).and_then(Value::as_array) else {
                continue;
            };
            for f in list {
                let code = f
                    .get("code")
                    .and_then(Value::as_str)
                    .unwrap_or("finding")
                    .to_owned();
                let rank = match self.project.rule_severity(&code, "warning").as_str() {
                    "error" => 0,
                    "info" => 2,
                    _ => 1,
                };
                rows.push(Row {
                    key,
                    code,
                    message: f
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_owned(),
                    objects: f
                        .get("objects")
                        .and_then(Value::as_array)
                        .map(|v| v.iter().filter_map(Value::as_u64).collect())
                        .unwrap_or_default(),
                    rank,
                });
            }
        }
        rows.sort_by_key(|r| r.rank);
        rows
    }

    /// Draws the sheet at the bottom of the centre area.
    ///
    /// # Rules
    /// Call before the canvas so the canvas gets the remaining height.
    ///
    /// # Do not
    /// Draw it across the asides; it sits under the canvas only.
    pub(crate) fn bottom_sheet(&mut self, ui: &mut egui::Ui) {
        let rows = self.sheet_rows();
        let count = rows.len();
        let open = self.region().bottom_open;
        let tab = SheetTab::from_id(&self.region().bottom_tab);
        let height = if open {
            Self::SHEET_OPEN
        } else {
            Self::SHEET_STRIP
        };
        egui::Panel::bottom("bottom-sheet")
            .exact_size(height)
            .resizable(false)
            .frame(Self::dock_frame(ui.style()).inner_margin(egui::Margin::symmetric(12, 4)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.set_min_height(Self::SHEET_STRIP - 8.0);
                    self.sheet_tab_strip(ui, count, open, tab);
                });
                if open {
                    ui.separator();
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| match tab {
                            SheetTab::Findings => self.sheet_findings(ui, rows),
                            SheetTab::Output => self.sheet_output(ui),
                            SheetTab::Console => self.sheet_console(ui),
                        });
                }
            });
    }

    fn sheet_tab_strip(&mut self, ui: &mut egui::Ui, count: usize, open: bool, current: SheetTab) {
        for region_tab in self.workspace().bottom_tabs() {
            let tab = SheetTab::from_id(region_tab.id);
            let label = if tab == SheetTab::Findings && count > 0 {
                t!("sheet.tab_findings_count", count = count)
            } else {
                (region_tab.label)()
            };
            let tip = (region_tab.tip)();
            let active = open && current == tab;
            if ui
                .selectable_label(active, label)
                .on_hover_text(tip)
                .clicked()
            {
                if active {
                    self.region().bottom_open = false;
                } else {
                    self.open_sheet(tab);
                }
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let (arrow, tip) = if open {
                ("v", t!("sheet.collapse.tip"))
            } else {
                ("^", t!("sheet.expand.tip"))
            };
            if ui.tap_button(arrow).on_hover_text(tip).clicked() {
                self.region().bottom_open = !open;
            }
            if open && current == SheetTab::Findings {
                if ui
                    .tap_button(t!("sheet.run_checks"))
                    .on_hover_text(t!("sheet.run_checks.tip"))
                    .clicked()
                {
                    // Checks are recomputed from the session whenever its
                    // revision differs from ours, so a mismatch forces a rerun.
                    self.revision = self.revision.wrapping_add(1);
                    self.message = t!("sheet.checks_ran");
                }
                let current = self.sheet.filter;
                egui::ComboBox::from_id_salt("sheet-severity")
                    .selected_text(Self::filter_label(current))
                    .show_ui(ui, |ui| {
                        for f in [
                            SeverityFilter::All,
                            SeverityFilter::Errors,
                            SeverityFilter::Warnings,
                            SeverityFilter::Notes,
                        ] {
                            ui.selectable_value(&mut self.sheet.filter, f, Self::filter_label(f));
                        }
                    })
                    .response
                    .named("Filter findings")
                    .on_hover_text(t!("sheet.filter.tip"));
            }
        });
    }

    fn filter_label(f: SeverityFilter) -> String {
        match f {
            SeverityFilter::All => t!("sheet.filter_all"),
            SeverityFilter::Errors => t!("sheet.filter_errors"),
            SeverityFilter::Warnings => t!("sheet.filter_warnings"),
            SeverityFilter::Notes => t!("sheet.filter_notes"),
        }
    }

    fn sheet_findings(&mut self, ui: &mut egui::Ui, rows: Vec<Row>) {
        if rows.is_empty() {
            empty_state(ui, EMPTY_CHECKS, self.muted());
            ui.label(
                egui::RichText::new(t!("checks.checks_view.no_problems"))
                    .size(Self::BODY)
                    .color(self.accent()),
            );
        }
        let filter = self.sheet.filter;
        for (i, row) in rows.into_iter().enumerate() {
            let keep = match filter {
                SeverityFilter::All => true,
                SeverityFilter::Errors => row.rank == 0,
                SeverityFilter::Warnings => row.rank == 1,
                SeverityFilter::Notes => row.rank == 2,
            };
            if keep {
                ui.push_id(i, |ui| self.sheet_row(ui, row));
            }
        }
        ui.add_space(8.0);
        self.checks_view(ui);
    }

    fn sheet_row(&mut self, ui: &mut egui::Ui, row: Row) {
        ui.horizontal(|ui| {
            let (colour, word) = match row.rank {
                0 => (self.accent(), t!("sheet.severity_error")),
                1 => (
                    Color32::from_rgb(230, 170, 60),
                    t!("sheet.severity_warning"),
                ),
                _ => (self.muted(), t!("sheet.severity_note")),
            };
            let (dot, response) =
                ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
            ui.painter().circle_filled(dot.center(), 5.0, colour);
            response.on_hover_text(format!(
                "{word} {}",
                custom_heading(&self.project, &row.code)
            ));
            ui.label(egui::RichText::new(&row.message).size(Self::BODY));
            if let Some(&first) = row.objects.first() {
                let net = self.project.net_name(first);
                if net != "unassigned" {
                    ui.label(
                        egui::RichText::new(net)
                            .size(Self::CAPTION)
                            .background_color(self.colours().icon_fill),
                    )
                    .on_hover_text(t!("sheet.net_chip.tip"));
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let has = !row.objects.is_empty();
                if ui
                    .add_enabled(has, egui::Button::new(t!("sheet.mute")))
                    .on_hover_text(t!("sheet.mute.tip"))
                    .clicked()
                {
                    self.mute_area(&row);
                }
                if ui
                    .add_enabled(has, egui::Button::new(t!("sheet.hide")))
                    .on_hover_text(t!("sheet.hide.tip"))
                    .clicked()
                {
                    self.run(
                        "rules.exclude",
                        json!({"code": row.code, "ids": row.objects.clone()}),
                    );
                }
                if ui
                    .add_enabled(has, egui::Button::new(t!("sheet.show")))
                    .on_hover_text(t!("sheet.show.tip"))
                    .clicked()
                    && let Some(id) = row.objects.first().copied()
                {
                    self.change_tab(if row.key == "erc" {
                        Tab::Schematic
                    } else {
                        Tab::Pcb
                    });
                    self.select(Some(id));
                }
            });
        });
    }

    /// Asks the check to leave the box around this finding's objects alone.
    fn mute_area(&mut self, row: &Row) {
        let spots: Vec<Point> = row
            .objects
            .iter()
            .filter_map(|o| self.project.object_position(*o))
            .collect();
        if spots.is_empty() {
            return;
        }
        let fold = |f: fn(f64, f64) -> f64, g: fn(&Point) -> f64| {
            spots.iter().map(g).reduce(f).unwrap_or(0.0)
        };
        let (min_x, max_x) = (fold(f64::min, |p| p.x), fold(f64::max, |p| p.x));
        let (min_y, max_y) = (fold(f64::min, |p| p.y), fold(f64::max, |p| p.y));
        let pad = 2.0;
        self.run(
            "rules.exclude_region",
            json!({
                "code": row.code,
                "x": min_x - pad,
                "y": min_y - pad,
                "width": (max_x - min_x) + pad * 2.0,
                "height": (max_y - min_y) + pad * 2.0,
            }),
        );
    }

    fn sheet_output(&mut self, ui: &mut egui::Ui) {
        let (fix, warnings, notes) = Self::finding_counts(&self.checks);
        ui.label(t!(
            "sheet.output_summary",
            errors = fix,
            warnings = warnings,
            notes = notes
        ));
        if !self.message.is_empty() {
            ui.label(egui::RichText::new(&self.message).color(self.muted()));
        }
    }

    fn sheet_console(&mut self, ui: &mut egui::Ui) {
        ui.label(
            egui::RichText::new(t!("sheet.console_revision", revision = self.revision))
                .color(self.muted()),
        );
        ui.label(egui::RichText::new(&self.message).color(self.muted()));
    }
}
