//! The Projects tab of the left dock: the open design as a tree of its
//! source documents, libraries and outputs.
//!
//! Context: the first tab of the left dock (`workspace.rs`, `LeftTab`). The
//! work areas (Schematic, Layout, Manufacture) are the documents of the
//! design; this tree names each one and opens it with the action the rest of
//! the window already uses: `App::show_page` for a schematic page,
//! `App::change_tab` for the board and the outputs, and the Components tab
//! for the project library.
//!
//! Local rules: the tree only navigates. It never edits the design, so it
//! needs no undo step. The row of the document on screen is selected. The
//! tree is short, so its sections always stay open: a chevron would be a
//! control too small to hit.

use super::*;
use crate::ui_kit::rows::{TreeRow, tree_row};
use crate::workspace::LeftTab;

/// Where a row of the Projects tree goes when it is clicked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Go {
    /// A schematic page, by id; 0 is the Main page.
    Page(Id),
    /// The board, in the Layout work area.
    Board,
    /// The Components tab of the left dock.
    Components,
    /// The Manufacture work area.
    Outputs,
}

/// What one row of the Projects tree shows and does.
struct Row {
    depth: usize,
    name: String,
    tip: String,
    count: Option<String>,
    /// `None` for the design row and the section rows, which only group.
    go: Option<Go>,
    selected: bool,
    dimmed: bool,
}

impl Row {
    fn leaf(name: String, tip: String, go: Go, selected: bool) -> Self {
        Self {
            depth: 2,
            name,
            tip,
            count: None,
            go: Some(go),
            selected,
            dimmed: false,
        }
    }

    fn section(name: String, tip: String, count: usize) -> Self {
        Self {
            depth: 1,
            name,
            tip,
            count: Some(count.to_string()),
            go: None,
            selected: false,
            dimmed: false,
        }
    }
}

impl App {
    /// Body of the Projects tab: the design, its pages and board, its
    /// libraries and its output jobs, one tree row each.
    ///
    /// # Rules
    /// - A click opens the row's document; the design and section rows only group.
    /// - The row of the document on screen is drawn selected.
    ///
    /// # Dependencies
    /// - `crates/ui-egui/src/ui_kit/rows.rs` (`tree_row`)
    /// - `crates/ui-egui/src/pages.rs` (`show_page`, `page_name`)
    pub(crate) fn projects_tab(&mut self, ui: &mut egui::Ui) {
        let rows = self.project_rows();
        let mut go = None;
        egui::ScrollArea::vertical()
            .id_salt("projects-scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                for (index, row) in rows.iter().enumerate() {
                    let response = tree_row(
                        ui,
                        ("projects-row", index),
                        &TreeRow {
                            depth: row.depth,
                            expanded: None,
                            swatch: None,
                            icon: None,
                            name: &row.name,
                            count: row.count.clone(),
                            locked: None,
                            visible: None,
                            selected: row.selected,
                            dimmed: row.dimmed,
                        },
                    );
                    if response.row.on_hover_text(&row.tip).clicked() && row.go.is_some() {
                        go = row.go;
                    }
                }
            });
        if let Some(go) = go {
            self.project_go(go);
        }
    }

    /// The rows of the Projects tree, top to bottom.
    fn project_rows(&self) -> Vec<Row> {
        let on_schematic = self.tab == Tab::Schematic;
        let page = self.schematic_page();
        let mut rows = vec![self.project_root_row()];

        let pages: Vec<Id> = std::iter::once(0)
            .chain(self.project.sheets.iter().map(|s| s.id))
            .collect();
        rows.push(Row::section(
            t!("projects.documents"),
            t!("projects.documents.tip"),
            pages.len() + 1,
        ));
        for id in pages {
            rows.push(Row::leaf(
                self.page_name(id),
                t!("projects.page.tip"),
                Go::Page(id),
                on_schematic && page == id,
            ));
        }
        rows.push(Row::leaf(
            t!("projects.board"),
            t!("projects.board.tip"),
            Go::Board,
            matches!(self.tab, Tab::Pcb | Tab::Board3d),
        ));

        rows.push(Row::section(
            t!("projects.libraries"),
            t!("projects.libraries.tip"),
            1,
        ));
        let library = &self.project.library;
        let mut own = Row::leaf(
            t!("projects.project_library"),
            t!("projects.project_library.tip"),
            Go::Components,
            false,
        );
        own.count = Some((library.symbols.len() + library.footprints.len()).to_string());
        rows.push(own);

        let jobs = &self.project.output_jobs;
        rows.push(Row::section(
            t!("projects.outputs"),
            t!("projects.outputs.tip"),
            jobs.len(),
        ));
        let on_outputs = self.tab == Tab::PlotFiles;
        if jobs.is_empty() {
            let mut none = Row::leaf(
                t!("projects.no_outputs"),
                t!("projects.no_outputs.tip"),
                Go::Outputs,
                on_outputs,
            );
            none.dimmed = true;
            rows.push(none);
        }
        for job in jobs {
            let mut row = Row::leaf(
                job.name.clone(),
                t!("projects.output_job.tip"),
                Go::Outputs,
                on_outputs,
            );
            row.count = Some(job.kinds.len().to_string());
            rows.push(row);
        }
        rows
    }

    /// The top row: the design's name, marked when it has unsaved changes.
    ///
    /// # Rules
    /// - The name is the project's own, else the file's name, else Untitled.
    /// - Unsaved means the design changed since it was last opened or saved.
    fn project_root_row(&self) -> Row {
        let stem = std::path::Path::new(&self.path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        let name = [self.project.name.trim(), stem.trim()]
            .into_iter()
            .find(|n| !n.is_empty())
            .map_or_else(|| t!("projects.untitled"), str::to_owned);
        let unsaved = self.saved_revision.is_some_and(|r| r != self.revision);
        let (name, tip) = if unsaved {
            (
                t!("projects.root.unsaved", name = name),
                t!("projects.root.unsaved.tip"),
            )
        } else {
            (name, t!("projects.root.tip"))
        };
        Row {
            depth: 0,
            name,
            tip,
            count: None,
            go: None,
            selected: false,
            dimmed: false,
        }
    }

    /// Open what a Projects row stands for, through the window's own actions.
    fn project_go(&mut self, go: Go) {
        match go {
            Go::Page(id) => {
                if self.tab != Tab::Schematic {
                    self.change_tab(Tab::Schematic);
                }
                self.show_page(id);
            }
            Go::Board => {
                if !matches!(self.tab, Tab::Pcb | Tab::Board3d) {
                    // The Layout work area remembers whether it was in 3D.
                    let remembered_3d = self
                        .preferences
                        .layouts
                        .get("layout")
                        .is_some_and(|l| l.view_3d);
                    self.change_tab(if remembered_3d {
                        Tab::Board3d
                    } else {
                        Tab::Pcb
                    });
                }
            }
            Go::Components => {
                // Manufacture has no Components tab; the parts belong to the Schematic.
                if !self.show_left_tab(LeftTab::Components) {
                    self.change_tab(Tab::Schematic);
                    self.show_left_tab(LeftTab::Components);
                }
            }
            Go::Outputs => {
                if self.tab != Tab::PlotFiles {
                    self.change_tab(Tab::PlotFiles);
                }
            }
        }
    }
}
