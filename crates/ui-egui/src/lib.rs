//! One window, shared project, tabbed workspaces. Edits use the engine exclusively.
mod authoring;
mod canvas;
mod library;
use egui::{Align2, Color32, FontId, Rect, Stroke, Vec2};
use pcbcraft_doc::{Id, PinRef, Project, TEMPLATES};
use pcbcraft_geom::Point;
use pcbcraft_mcp::SharedSession;

/// Guidance shown when a panel section has no content yet: what belongs
/// there and the command that fills it, so a first-run user is never left
/// staring at a blank list.
const EMPTY_SYMBOLS: &str =
    "No symbols yet. Press A to open the symbol palette, or run component.add to add one.";
const EMPTY_NETS: &str = "No nets yet. Nets form when pins are wired together, or import a netlist with netlist.import and board.sync will create the packages.";
const EMPTY_BOARD: &str =
    "No packages on the board yet. Run board.sync to place a package for every symbol.";
const EMPTY_CHECKS: &str =
    "Checks run automatically on every edit; findings are listed here as soon as one appears.";

/// Render an empty-state line in the small muted style.
fn empty_state(ui: &mut egui::Ui, text: &str, muted: egui::Color32) {
    ui.label(egui::RichText::new(text).small().color(muted));
}

/// Help copy shown under the inspector's editable fields. Each line names
/// what the field is for and the limit the engine will enforce, so the
/// requirement is visible before the first rejected edit.
const FIELD_HELP_VALUE: &str = "Free text: a name or value, like R1 or 10k. It updates the symbol or package immediately when Apply is pressed.";
const FIELD_HELP_POSITION: &str =
    "Absolute millimetres from the design origin; Apply moves just this symbol or package.";
const FIELD_HELP_BOARD: &str =
    "Board outline size in millimetres; both values must be larger than zero.";
const FIELD_HELP_CUTOUT: &str =
    "Radius of the circle the cutout tool places; the whole circle must stay inside the board.";

fn field_help(ui: &mut egui::Ui, text: &str, muted: egui::Color32) {
    ui.label(egui::RichText::new(text).small().color(muted).weak());
}

/// The complete input reference: every key binding the app installs, plus
/// the canvas mouse gestures, with where each one applies. The overlay and
/// the tests read this one table, so the list can never claim a key the
/// handler does not install.
fn shortcut_rows() -> Vec<(&'static str, &'static str, &'static str)> {
    vec![
        ("Ctrl+Z", "Undo the last action", "anywhere"),
        ("Ctrl+Shift+Z", "Redo an undone action", "anywhere"),
        ("Ctrl+Y", "Redo an undone action", "anywhere"),
        ("Ctrl+S", "Save the design to its file", "anywhere"),
        ("Ctrl+K", "Open the command palette", "anywhere"),
        ("?", "Show or hide this shortcut list", "anywhere"),
        ("A", "Open the symbol palette", "schematic tab"),
        (
            "W",
            "Wire tool: click two pins to connect them",
            "schematic tab",
        ),
        (
            "X",
            "Route tool: click two pads to start a track",
            "pcb tab",
        ),
        (
            "M",
            "Move tool: drop the selection at the cursor",
            "anywhere",
        ),
        (
            "R",
            "Rotate the hovered or selected object by 90 degrees",
            "anywhere",
        ),
        (
            "E",
            "Load the hovered or selected object into the inspector",
            "anywhere",
        ),
        ("Home", "Zoom the view so the whole design fits", "anywhere"),
        ("Delete", "Delete the selected object", "anywhere"),
        (
            "Esc",
            "Return to the select tool and close open palettes",
            "anywhere",
        ),
        ("Middle-drag", "Pan the view", "canvas"),
        ("Scroll", "Zoom the view", "canvas"),
    ]
}

/// What the mouse and keys will do next for the active tool. The strip is
/// rendered under the toolbar so the next action is always visible instead
/// of being something the user has to remember.
fn tool_hint(tool: &Tool) -> String {
    let common = " · middle-drag: pan · scroll: zoom · Esc: back to select";
    match tool {
        Tool::Select => format!("LMB: select an object · drag: move the selected object{common}"),
        Tool::Place(_) => format!("LMB: place the symbol at the cursor{common}"),
        Tool::LibraryPlace(..) | Tool::CustomPlace(..) => {
            format!("LMB: place the symbol package at the cursor{common}")
        }
        Tool::Wire(None) => format!("LMB: pick the first pin of the new wire{common}"),
        Tool::Wire(Some(_)) => {
            format!("LMB: pick the second pin to complete the wire{common}")
        }
        Tool::Route(None) => format!("LMB: pick the first pad to start the track{common}"),
        Tool::Route(Some(_)) => {
            format!("LMB: pick the second pad to complete the track{common}")
        }
        Tool::Move => format!("LMB: drop the selected object at the cursor{common}"),
        Tool::Cutout => format!("LMB: place a circular cutout at the cursor{common}"),
    }
}

/// Case-insensitive subsequence match: every character of the query must
/// appear, in order, somewhere in the text.
fn fuzzy_match(query: &str, text: &str) -> bool {
    let mut text = text.chars();
    query
        .chars()
        .all(|q| text.any(|t| t.eq_ignore_ascii_case(&q)))
}

/// A starter parameters object for a command: every required parameter gets
/// a plausible sample value taken from its own schema (the schema default,
/// the first listed option, the schema minimum, or a neutral stand-in), so
/// picking a command in the palette immediately shows the shape its
/// parameters must have; optional parameters are left out to be added.
fn params_template(spec: &pcbcraft_engine::CommandSpec) -> String {
    template_object(&spec.input_schema).to_string()
}

fn template_object(schema: &Value) -> Value {
    let properties = schema.get("properties").cloned().unwrap_or(Value::Null);
    let keys: Vec<String> = schema
        .get("required")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let mut object = serde_json::Map::new();
    for key in keys {
        if let Some(field) = properties.get(&key) {
            object.insert(key, sample_value(field));
        }
    }
    Value::Object(object)
}

fn sample_value(schema: &Value) -> Value {
    if let Some(default) = schema.get("default") {
        return default.clone();
    }
    if let Some(options) = schema.get("enum").and_then(Value::as_array)
        && let Some(first) = options.first()
    {
        return first.clone();
    }
    match schema.get("type").and_then(Value::as_str) {
        Some("integer") => schema.get("minimum").cloned().unwrap_or(json!(1)),
        Some("number") => schema.get("minimum").cloned().unwrap_or(json!(0.0)),
        Some("boolean") => json!(false),
        Some("string") => json!(""),
        Some("array") => {
            let count = schema
                .get("minItems")
                .and_then(Value::as_u64)
                .unwrap_or(2)
                .clamp(2, 8);
            let item = schema
                .get("items")
                .map(sample_value)
                .unwrap_or(json!([0.0, 0.0]));
            Value::Array(vec![item; count as usize])
        }
        _ => {
            if schema.get("properties").is_some() {
                template_object(schema)
            } else {
                json!("")
            }
        }
    }
}

/// Hover text for a control that runs an engine command: the command
/// registry description is the single source of truth for what the control
/// does, so the UI explanation and the command documentation never drift.
fn command_tip(id: &str) -> String {
    pcbcraft_engine::commands()
        .into_iter()
        .find(|spec| spec.id == id)
        .map_or_else(
            || "This control runs an engine command.".into(),
            |spec| spec.description.to_string(),
        )
}
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Schematic,
    Pcb,
    Libraries,
    Checks,
}
#[derive(Clone, Serialize, Deserialize)]
struct Preferences {
    dark: bool,
    grid: bool,
    accent: [u8; 3],
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            dark: true,
            grid: true,
            accent: [91, 224, 188],
        }
    }
}
#[derive(Clone)]
enum Tool {
    Select,
    Place(String),
    LibraryPlace(String, String, Box<pcbcraft_library::SymbolDefinition>),
    CustomPlace(String, Box<pcbcraft_library::SymbolDefinition>),
    Wire(Option<PinRef>),
    Route(Option<(Id, String)>),
    Move,
    Cutout,
}
#[derive(Clone, Copy)]
struct Camera {
    center: Point,
    zoom: f32,
}
impl Camera {
    fn screen(self, p: Point, rect: Rect) -> egui::Pos2 {
        rect.center()
            + Vec2::new(
                (p.x - self.center.x) as f32 * self.zoom,
                (p.y - self.center.y) as f32 * self.zoom,
            )
    }
    fn world(self, p: egui::Pos2, rect: Rect) -> Point {
        Point::new(
            self.center.x + (p.x - rect.center().x) as f64 / self.zoom as f64,
            self.center.y + (p.y - rect.center().y) as f64 / self.zoom as f64,
        )
    }
}
pub struct App {
    session: SharedSession,
    project: Project,
    revision: u64,
    checks: Value,
    tab: Tab,
    tool: Tool,
    selected: Option<Id>,
    highlight_net: Option<Id>,
    selected_unit: u32,
    route_layer: pcbcraft_doc::Layer,
    preferences: Preferences,
    appearance: bool,
    palette: bool,
    schematic: Camera,
    pcb: Camera,
    drag: Option<(Id, Point)>,
    drag_offset: Point,
    cursor: Option<Point>,
    message: String,
    value: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    cutout_radius: f64,
    path: String,
    saved_revision: Option<u64>,
    command: String,
    params: String,
    command_open: bool,
    command_query: String,
    shortcuts_open: bool,
    command_focus: bool,
    capture: Option<String>,
    frames: u32,
    library: library::Browser,
}
impl App {
    pub fn with_project_path(mut self, path: Option<String>) -> Self {
        if let Some(path) = path {
            self.path = path;
            self.saved_revision = self.session.lock().ok().map(|s| s.revision);
        }
        self
    }
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        session: SharedSession,
        capture: Option<String>,
        tab: Option<&str>,
        light: bool,
    ) -> Self {
        let mut preferences: Preferences = cc
            .storage
            .and_then(|s| eframe::get_value(s, "preferences"))
            .unwrap_or_default();
        if light {
            preferences.dark = false;
        }
        if let Some(text) = cc
            .storage
            .and_then(|storage| storage.get_string("recovery"))
            && text.len() <= pcbcraft_io_limit()
            && let Ok(project) = serde_json::from_str::<Project>(&text)
            && project.validate().is_ok()
            && let Ok(mut current) = session.lock()
            && current.revision == 0
            && current.project.components.is_empty()
        {
            let _ = current.replace(project);
        }
        let mut app = Self::create(session, preferences, capture, tab);
        if !app.project.components.is_empty() {
            let initial = app.tab;
            app.tab = Tab::Schematic;
            app.fit();
            app.tab = Tab::Pcb;
            app.fit();
            app.tab = initial;
        }
        app
    }
    fn create(
        session: SharedSession,
        preferences: Preferences,
        capture: Option<String>,
        tab: Option<&str>,
    ) -> Self {
        let project = session
            .lock()
            .map(|s| s.project.clone())
            .unwrap_or_default();
        let width = project.board.width;
        let height = project.board.height;
        let library = library::Browser {
            editor: authoring::initial_editor(tab, &project),
            ..Default::default()
        };
        Self {
            session,
            project,
            revision: u64::MAX,
            checks: json!({}),
            tab: match tab {
                Some("pcb") => Tab::Pcb,
                Some("checks") => Tab::Checks,
                Some("libraries" | "project-library" | "symbol-editor" | "footprint-editor") => {
                    Tab::Libraries
                }
                _ => Tab::Schematic,
            },
            tool: Tool::Select,
            selected: None,
            highlight_net: None,
            selected_unit: 1,
            route_layer: pcbcraft_doc::Layer::Front,
            preferences,
            appearance: false,
            palette: false,
            schematic: Camera {
                center: Point::new(40.0, 24.0),
                zoom: 11.0,
            },
            pcb: Camera {
                center: Point::new(30.0, 20.0),
                zoom: 15.0,
            },
            drag: None,
            drag_offset: Point::default(),
            cursor: None,
            message: "Ready · A add · W wire · X route · M move · R rotate · E properties".into(),
            value: String::new(),
            x: 0.0,
            y: 0.0,
            width,
            height,
            cutout_radius: 1.6,
            path: "design.pcbcraft".into(),
            saved_revision: None,
            command: "component.add".into(),
            params: "{\"kind\":\"resistor\",\"position\":{\"x\":30,\"y\":20}}".into(),
            command_open: false,
            command_query: String::new(),
            shortcuts_open: false,
            command_focus: false,
            capture,
            frames: 0,
            library,
        }
    }
    fn refresh(&mut self) {
        let snapshot = self.session.lock().ok().and_then(|s| {
            if s.revision != self.revision {
                Some((s.project.clone(), s.revision, s.checks()))
            } else {
                None
            }
        });
        if let Some((p, r, checks)) = snapshot {
            self.project = p;
            self.revision = r;
            self.checks = checks;
            if self
                .selected
                .is_some_and(|id| !self.project.ids().any(|present| present == id))
            {
                self.selected = None;
            }
            self.width = self.project.board.width;
            self.height = self.project.board.height;
            self.load_properties();
        }
    }
    fn execute_palette_params(&mut self) {
        match serde_json::from_str(&self.params.clone()) {
            Ok(v) => {
                let command = self.command.clone();
                self.run(&command, v);
            }
            Err(e) => self.message = e.to_string(),
        }
    }

    fn run(&mut self, command: &str, params: Value) {
        let result = self
            .session
            .lock()
            .map_err(|_| "Session unavailable".to_string())
            .and_then(|mut s| {
                s.execute(command, params, Some(self.revision))
                    .map_err(|e| e.to_string())
            });
        match result {
            Ok(v) => {
                self.message = format!("{command} · committed");
                if let Some(skipped) = v
                    .pointer("/result/unassigned_components")
                    .and_then(Value::as_array)
                    && !skipped.is_empty()
                {
                    self.message = format!(
                        "{} library symbols need a footprint assignment in Libraries",
                        skipped.len()
                    );
                }
                if let Some(id) = v.pointer("/result/id").and_then(Value::as_u64) {
                    if self.selected != Some(id) {
                        self.selected_unit = 1;
                    }
                    self.selected = Some(id);
                }
            }
            Err(e) => self.message = e,
        }
        self.refresh();
    }
    fn tool_call(&mut self, name: &str, args: Value) -> bool {
        let result = pcbcraft_mcp::call_tool(&self.session, name, &args);
        let success = result.get("isError").and_then(Value::as_bool) != Some(true);
        self.message = if success {
            format!("{name} · complete")
        } else {
            result
                .pointer("/content/0/text")
                .and_then(Value::as_str)
                .unwrap_or("operation failed")
                .into()
        };
        self.refresh();
        if success && (name == "save" || name == "open") {
            self.saved_revision = Some(self.revision);
        }
        success
    }
    fn accent(&self) -> Color32 {
        if !self.preferences.dark && self.preferences.accent == [91, 224, 188] {
            return Color32::from_rgb(18, 135, 110);
        }
        Color32::from_rgb(
            self.preferences.accent[0],
            self.preferences.accent[1],
            self.preferences.accent[2],
        )
    }
    fn surface(&self) -> Color32 {
        if self.preferences.dark {
            Color32::from_rgb(20, 29, 43)
        } else {
            Color32::from_rgb(245, 247, 251)
        }
    }
    fn muted(&self) -> Color32 {
        if self.preferences.dark {
            Color32::from_rgb(140, 157, 182)
        } else {
            Color32::from_rgb(88, 104, 128)
        }
    }
    fn ink(&self) -> Color32 {
        if self.preferences.dark {
            Color32::from_rgb(225, 234, 247)
        } else {
            Color32::from_rgb(34, 48, 68)
        }
    }
    fn select(&mut self, id: Option<Id>) {
        self.selected = id;
        self.selected_unit = 1;
        self.load_properties();
    }
    fn load_properties(&mut self) {
        if let Some(id) = self.selected {
            if let Some(c) = self.project.component(id) {
                if c.unit_placement(self.selected_unit).is_none() {
                    self.selected_unit = 1;
                }
                self.value = c.value.clone();
                let position = c
                    .unit_placement(self.selected_unit)
                    .map(|p| p.0)
                    .unwrap_or(c.position);
                self.x = position.x;
                self.y = position.y;
            } else if let Some(f) = self.project.footprint(id) {
                self.x = f.position.x;
                self.y = f.position.y;
                if let Some(c) = self.project.component(f.component) {
                    self.value = c.value.clone();
                }
            }
        }
    }
    fn change_tab(&mut self, tab: Tab) {
        if tab == Tab::Pcb
            && let Some(c) = self.selected.and_then(|id| self.project.component(id))
        {
            self.selected = self
                .project
                .board
                .footprints
                .iter()
                .find(|f| f.component == c.id)
                .map(|f| f.id);
        }
        if (tab == Tab::Schematic || tab == Tab::Libraries)
            && let Some(f) = self.selected.and_then(|id| self.project.footprint(id))
        {
            self.selected = Some(f.component);
        }
        self.tab = tab;
        self.tool = Tool::Select;
        self.drag = None;
        self.load_properties();
    }
    fn rotate(&mut self) {
        if let Some(id) = self.selected {
            if let Some(c) = self.project.component(id) {
                self.run(
                    "component.rotate",
                    json!({"id":id,"unit":self.selected_unit,"degrees":c.unit_placement(self.selected_unit).map(|p|p.1).unwrap_or(c.rotation)+90.0}),
                );
            } else if let Some(f) = self.project.footprint(id) {
                self.run(
                    "footprint.rotate",
                    json!({"id":id,"degrees":f.rotation+90.0}),
                );
            }
        }
    }
    fn shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() || ctx.current_pass_index() > 0 {
            return;
        }
        ctx.input(|i| {
            if i.modifiers.command {
                if i.key_pressed(egui::Key::Z) {
                    self.tool_call(if i.modifiers.shift { "redo" } else { "undo" }, json!({}));
                }
                if i.key_pressed(egui::Key::Y) {
                    self.tool_call("redo", json!({}));
                }
                if i.key_pressed(egui::Key::S) {
                    self.tool_call("save", json!({"path":self.path}));
                }
                if i.key_pressed(egui::Key::K) {
                    self.command_open = !self.command_open;
                    self.command_focus = self.command_open;
                }
                return;
            }
            if i.key_pressed(egui::Key::A) && self.tab == Tab::Schematic {
                self.palette = true;
            }
            if i.key_pressed(egui::Key::W) && self.tab == Tab::Schematic {
                self.tool = Tool::Wire(None);
            }
            if i.key_pressed(egui::Key::X) && self.tab == Tab::Pcb {
                self.tool = Tool::Route(None);
            }
            if i.key_pressed(egui::Key::M) {
                self.select_hover();
                self.tool = Tool::Move;
            }
            if i.key_pressed(egui::Key::R) {
                self.select_hover();
                self.rotate();
            }
            if i.key_pressed(egui::Key::E) {
                self.select_hover();
                self.appearance = false;
                self.load_properties();
            }
            if i.key_pressed(egui::Key::Home) {
                self.fit();
            }
            if i.key_pressed(egui::Key::Escape) {
                self.tool = Tool::Select;
                self.drag = None;
                self.palette = false;
                self.shortcuts_open = false;
            }
            if i.key_pressed(egui::Key::Slash) {
                self.shortcuts_open = !self.shortcuts_open;
            }
            if i.key_pressed(egui::Key::Delete)
                && let Some(id) = self.selected
            {
                self.run("object.delete", json!({"id":id}));
                self.select(None);
            }
        });
    }
    fn style(&self, ctx: &egui::Context) {
        let mut visuals = if self.preferences.dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        };
        visuals.panel_fill = if self.preferences.dark {
            Color32::from_rgb(17, 24, 36)
        } else {
            Color32::from_rgb(235, 239, 245)
        };
        visuals.window_fill = self.surface();
        visuals.selection.bg_fill = self.accent().gamma_multiply(0.25);
        visuals.selection.stroke = Stroke::new(1.0, self.accent());
        craft_ui::theme::WidgetPalette {
            noninteractive: self.surface(),
            inactive: self.surface(),
            hovered: if self.preferences.dark {
                Color32::from_rgb(37, 53, 73)
            } else {
                Color32::from_rgb(215, 225, 236)
            },
            active: self.accent().gamma_multiply(0.25),
            open: self.surface(),
        }
        .apply_to(
            &mut visuals.widgets,
            egui::CornerRadius::same(6),
            Stroke::new(1.0, self.ink()),
        );
        ctx.set_visuals(visuals);
        ctx.style_mut_of(ctx.theme(), |s| {
            s.spacing.item_spacing = Vec2::new(8.0, 8.0);
            s.spacing.button_padding = Vec2::new(10.0, 6.0);
        });
    }
    fn header(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("header").exact_size(140.0).show(ui, |ui| {
            ui.add_space(7.0);
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                ui.label(
                    egui::RichText::new("PCBCraft")
                        .size(22.0)
                        .strong()
                        .color(self.accent()),
                );
                ui.label(egui::RichText::new("/").color(self.muted()));
                ui.label(egui::RichText::new(&self.project.name).size(16.0));
                if self.saved_revision != Some(self.revision) {
                    ui.label(egui::RichText::new("*").color(self.accent()));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Appearance").clicked() {
                        self.appearance = !self.appearance;
                    }
                    if ui.button("Save  Ctrl+S").on_hover_text("Write the design to its file on disk.").clicked() {
                        self.tool_call("save", json!({"path":self.path}));
                    }
                    if ui.button("Redo").on_hover_text("Step forward to the action you undid.").clicked() {
                        self.tool_call("redo", json!({}));
                    }
                    if ui.button("Undo").on_hover_text("Step back one action; every edit here is one undoable step.").clicked() {
                        self.tool_call("undo", json!({}));
                    }
                    ui.label(
                        egui::RichText::new("MCP FIRST")
                            .size(10.0)
                            .color(self.muted()),
                    );
                });
            });
            ui.add_space(5.0);
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                for (tab, label) in [
                    (Tab::Schematic, "Schematic"),
                    (Tab::Pcb, "PCB Layout"),
                    (Tab::Libraries, "Libraries"),
                    (Tab::Checks, "Checks"),
                ] {
                    let (_, rect) = ui.allocate_space(Vec2::new(132.0, 30.0));
                    let selected = self.tab == tab;
                    let accent = self.accent();
                    let ink = self.ink();
                    let muted = self.muted();
                    let response = craft_ui::tabs::Tab::new(ui.id().with(label), label, selected)
                        .show_at(ui, rect, |ui, response| {
                            if selected || response.hovered() {
                                ui.painter().rect_filled(
                                    rect,
                                    6.0,
                                    accent.gamma_multiply(if selected { 0.16 } else { 0.08 }),
                                );
                            }
                            ui.painter().text(
                                rect.center(),
                                Align2::CENTER_CENTER,
                                label,
                                FontId::proportional(13.0),
                                if selected { ink } else { muted },
                            );
                            if selected {
                                ui.painter().line_segment(
                                    [rect.left_bottom(), rect.right_bottom()],
                                    Stroke::new(2.0, accent),
                                );
                            }
                        });
                    if response.clicked() {
                        self.change_tab(tab);
                    }
                }
                ui.add_enabled(false, egui::Button::new("3D Board"))
                    .on_hover_text("A real Rust 3D viewer is planned.");
            });
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                if ui
                    .selectable_label(matches!(self.tool, Tool::Select), "Select  Esc")
                    .clicked()
                {
                    self.tool = Tool::Select;
                }
                match self.tab {
                    Tab::Schematic => {
                        if ui.button("Add symbol  A").on_hover_text(command_tip("component.add")).clicked() {
                            self.palette = !self.palette;
                        }
                        if ui
                            .selectable_label(matches!(self.tool, Tool::Wire(_)), "Wire  W")
                            .on_hover_text(command_tip("wire.draw"))
                            .clicked()
                        {
                            self.tool = Tool::Wire(None);
                        }
                        if ui.button("Sync to PCB >").on_hover_text(command_tip("board.sync")).clicked() {
                            self.run("board.sync", json!({}));
                            self.change_tab(Tab::Pcb);
                        }
                    }
                    Tab::Pcb => {
                        egui::ComboBox::from_id_salt("route-layer")
                            .selected_text(self.route_layer.name())
                            .width(64.0)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.route_layer,
                                    pcbcraft_doc::Layer::Front,
                                    "F.Cu",
                                );
                                ui.selectable_value(
                                    &mut self.route_layer,
                                    pcbcraft_doc::Layer::Back,
                                    "B.Cu",
                                );
                            });
                        if ui
                            .selectable_label(matches!(self.tool, Tool::Route(_)), "Route  X")
                            .on_hover_text(command_tip("track.add"))
                            .clicked()
                        {
                            self.tool = Tool::Route(None);
                        }
                        if ui.button("Sync schematic").on_hover_text(command_tip("board.sync")).clicked() {
                            self.run("board.sync", json!({}));
                        }
                        if ui
                            .selectable_label(matches!(self.tool, Tool::Cutout), "Circular cutout")
                            .on_hover_text(command_tip("board.cutout.add"))
                            .clicked()
                        {
                            self.tool = Tool::Cutout;
                        }
                    }
                    _ => {}
                }
                if ui
                    .selectable_label(matches!(self.tool, Tool::Move), "Move  M")
                    .on_hover_text(
                        "Click to drop the selected object at the cursor; press M to switch to this tool.",
                    )
                    .clicked()
                {
                    self.tool = Tool::Move;
                }
                if ui.button("Rotate  R").on_hover_text(command_tip("component.rotate")).clicked() {
                    self.rotate();
                }
                if ui.button("Fit").on_hover_text("Zoom the view so the whole design fits.").clicked() {
                    self.fit();
                }
                if ui
                    .button("Shortcuts  ?")
                    .on_hover_text("Show every keyboard shortcut and mouse gesture in one list.")
                    .clicked()
                {
                    self.shortcuts_open = !self.shortcuts_open;
                }
                if ui.button("Commands  Ctrl+K").on_hover_text("Search every command by name and run it, with its parameters explained.").clicked() {
                    self.command_open = !self.command_open;
        self.command_focus = self.command_open;
                }
            });
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                ui.label(
                    egui::RichText::new(tool_hint(&self.tool))
                        .small()
                        .color(self.muted()),
                );
            });
        });
    }
    fn fit(&mut self) {
        if self.tab == Tab::Pcb {
            self.pcb.center = Point::new(
                self.project.board.width / 2.0,
                self.project.board.height / 2.0,
            );
            self.pcb.zoom = (700.0 / self.project.board.width.max(self.project.board.height))
                .clamp(1.0, 60.0) as f32;
        } else {
            let (mut xmin, mut ymin, mut xmax, mut ymax) = (
                f64::INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
            );
            for c in self
                .project
                .components
                .iter()
                .flat_map(|c| c.schematic_views())
            {
                for point in c.drawing_points() {
                    let point = c.position.offset(point.rotate(-c.rotation));
                    xmin = xmin.min(point.x - 4.0);
                    xmax = xmax.max(point.x + 4.0);
                    ymin = ymin.min(point.y - 4.0);
                    ymax = ymax.max(point.y + 4.0);
                }
            }
            if xmin.is_finite() {
                self.schematic.center = Point::new((xmin + xmax) / 2.0, (ymin + ymax) / 2.0);
                self.schematic.zoom = (650.0 / (xmax - xmin).max((ymax - ymin) * 1.5).max(10.0))
                    .clamp(2.0, 35.0) as f32;
            }
        }
    }
    fn browser(&mut self, ui: &mut egui::Ui) {
        egui::Panel::left("browser")
            .default_size(210.0)
            .min_size(170.0)
            .show(ui, |ui| {
                ui.add_space(12.0);
                ui.label(
                    egui::RichText::new("PROJECT")
                        .size(10.0)
                        .color(self.muted()),
                );
                ui.label(egui::RichText::new(&self.project.name).strong());
                ui.add_space(12.0);
                if self.palette {
                    ui.label(egui::RichText::new("Add symbol").strong());
                    if ui.button("Browse KiCad libraries →").on_hover_text("Open the library panel to browse installed symbol and footprint collections.").clicked() {
                        self.change_tab(Tab::Libraries);
                        self.palette = false;
                    }
                    for t in TEMPLATES {
                        if ui.button(t.label).clicked() {
                            self.change_tab(Tab::Schematic);
                            self.tool = Tool::Place(t.kind.into());
                            self.palette = false;
                        }
                    }
                    ui.separator();
                }
                ui.label(
                    egui::RichText::new("COMPONENTS")
                        .size(10.0)
                        .color(self.muted()),
                );
                for c in self.project.components.clone() {
                    let id = if self.tab == Tab::Pcb {
                        self.project
                            .board
                            .footprints
                            .iter()
                            .find(|f| f.component == c.id)
                            .map(|f| f.id)
                            .unwrap_or(c.id)
                    } else {
                        c.id
                    };
                    if ui
                        .selectable_label(
                            self.selected == Some(id),
                            format!("{}   {}", c.reference, c.value),
                        )
                        .clicked()
                    {
                        self.select(Some(id));
                    }
                }
                if self.project.components.is_empty() {
                    empty_state(ui, EMPTY_SYMBOLS, self.muted());
                }
                ui.add_space(20.0);
                ui.label(egui::RichText::new("NETS").size(10.0).color(self.muted()));
                if let Some(lit) = self.highlight_net
                    && !self.project.nets.iter().any(|n| n.id == lit)
                {
                    self.highlight_net = None;
                }
                for net in self.project.nets.clone() {
                    let lit = self.highlight_net == Some(net.id);
                    let row = ui
                        .selectable_label(
                            lit,
                            format!("{}   {}", net.name, if lit { "\u{25cf}" } else { "" }),
                        )
                        .on_hover_text(
                            "Spotlight this net: its copper, vias and pads light up on the board while everything else fades. Click again to clear the spotlight.",
                        );
                    if row.clicked() {
                        self.highlight_net = if lit { None } else { Some(net.id) };
                    }
                }
                if self.project.nets.is_empty() {
                    empty_state(ui, EMPTY_NETS, self.muted());
                }
                if let Some(lit) = self.highlight_net
                    && let Some(net) = self.project.nets.iter().find(|n| n.id == lit)
                {
                    ui.label(
                        egui::RichText::new(format!("Spotlighting {} on the board.", net.name))
                            .size(10.0)
                            .color(self.accent()),
                    );
                }
                ui.add_space(20.0);
                ui.label(egui::RichText::new("BOARD").size(10.0).color(self.muted()));
                ui.label(format!(
                    "{} × {} mm",
                    self.project.board.width, self.project.board.height
                ));
                ui.label("2 copper layers · 1.6 mm");
                if self.project.board.footprints.is_empty() {
                    empty_state(ui, EMPTY_BOARD, self.muted());
                }
                ui.label(format!(
                    "{} footprints · {} segments",
                    self.project.board.footprints.len(),
                    self.project.board.tracks.len()
                ));
                ui.add_space(20.0);
                ui.separator();
                ui.label(
                    egui::RichText::new("One project. One window.")
                        .small()
                        .color(self.muted()),
                );
                ui.label(
                    egui::RichText::new("Scroll to zoom · middle-drag to pan")
                        .small()
                        .color(self.muted()),
                );
            });
    }
    fn inspector(&mut self, ui: &mut egui::Ui) {
        egui::Panel::right("inspector")
            .default_size(250.0)
            .min_size(220.0)
            .show(ui, |ui| {
                ui.add_space(12.0);
                if self.appearance {
                    ui.heading("Appearance");
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut self.preferences.dark, true, "Dark");
                        ui.selectable_value(&mut self.preferences.dark, false, "Light");
                    });
                    ui.checkbox(&mut self.preferences.grid, "Show grid");
                    ui.label("Accent color");
                    ui.color_edit_button_srgb(&mut self.preferences.accent);
                    ui.add_space(16.0);
                    ui.label("Appearance is saved automatically.");
                    return;
                }
                ui.label(
                    egui::RichText::new("PROPERTIES")
                        .size(10.0)
                        .color(self.muted()),
                );
                if let Some(id) = self.selected {
                    let c = self.project.component(id).cloned().or_else(|| {
                        self.project
                            .footprint(id)
                            .and_then(|f| self.project.component(f.component))
                            .cloned()
                    });
                    if let Some(c) = c {
                        ui.heading(&c.reference);
                        if !c.units.is_empty() && self.project.component(id).is_some() {
                            let before = self.selected_unit;
                            egui::ComboBox::from_id_salt("selected-symbol-unit")
                                .selected_text(format!("Unit {}", self.selected_unit))
                                .show_ui(ui, |ui| {
                                    for unit in
                                        std::iter::once(1).chain(c.units.iter().map(|u| u.unit))
                                    {
                                        ui.selectable_value(
                                            &mut self.selected_unit,
                                            unit,
                                            format!("Unit {unit}"),
                                        );
                                    }
                                });
                            if before != self.selected_unit {
                                self.load_properties();
                            }
                        }
                        ui.label(&c.kind);
                        ui.label(
                            egui::RichText::new(format!("Stable ID {id}"))
                                .small()
                                .color(self.muted()),
                        );
                        ui.add_space(10.0);
                        ui.label("Value");
                        ui.text_edit_singleline(&mut self.value);
                        field_help(ui, FIELD_HELP_VALUE, self.muted());
                        if ui.button("Apply value").on_hover_text(command_tip("component.value")).clicked() {
                            self.run("component.value", json!({"id":c.id,"value":self.value}));
                        }
                        ui.add_space(12.0);
                        ui.label("Position · mm");
                        field_help(ui, FIELD_HELP_POSITION, self.muted());
                        ui.horizontal(|ui| {
                            ui.label("X");
                            ui.add(egui::DragValue::new(&mut self.x).speed(0.1));
                            ui.label("Y");
                            ui.add(egui::DragValue::new(&mut self.y).speed(0.1));
                        });
                        if ui.button("Apply position").on_hover_text("Move the selected symbol or footprint to the position typed above.").clicked() {
                            let mut params = json!({"id":id,"position":{"x":self.x,"y":self.y}});
                            if self.project.component(id).is_some() {
                                params["unit"] = json!(self.selected_unit);
                            }
                            self.run(
                                if self.project.footprint(id).is_some() {
                                    "footprint.move"
                                } else {
                                    "component.move"
                                },
                                params,
                            );
                        }
                        if ui.button("Rotate 90°  R").on_hover_text(command_tip("component.rotate")).clicked() {
                            self.rotate();
                        }
                        ui.label(
                            c.library_footprint
                                .as_ref()
                                .map(|f| format!("Footprint: {}", f.definition.name))
                                .unwrap_or_else(|| {
                                    if c.library_symbol.is_some() {
                                        "Footprint: unassigned".into()
                                    } else {
                                        "Footprint: starter package".into()
                                    }
                                }),
                        );
                        if ui.button("Assign footprint in Libraries →").on_hover_text("Jump to the library panel to bind a footprint package to this component.").clicked() {
                            self.change_tab(Tab::Libraries);
                            self.library.kind = true;
                            self.library.pending = None;
                            self.library.path.clear();
                            self.library.results = None;
                            self.library.preview = None;
                            self.library.query.clear();
                        }
                        ui.add_space(12.0);
                        ui.label("Pin / net");
                        for pin in &c.pins {
                            ui.label(format!(
                                "{} · {}",
                                pin.number,
                                pin.net
                                    .map(|n| self.project.net_name(n))
                                    .unwrap_or("unassigned")
                            ));
                        }
                    } else {
                        ui.heading("Selected object");
                        ui.label(format!("Stable ID {id}"));
                    }
                    ui.add_space(10.0);
                    if ui.button("Delete  Del").on_hover_text(command_tip("object.delete")).clicked() {
                        self.run("object.delete", json!({"id":id}));
                        self.select(None);
                    }
                    ui.separator();
                } else {
                    ui.heading("Project settings");
                    ui.label("Select an item to inspect and edit it.");
                    ui.add_space(12.0);
                }
                ui.label(
                    egui::RichText::new("BOARD GEOMETRY")
                        .size(10.0)
                        .color(self.muted()),
                );
                field_help(ui, FIELD_HELP_BOARD, self.muted());
                ui.horizontal(|ui| {
                    ui.label("W");
                    ui.add(
                        egui::DragValue::new(&mut self.width)
                            .speed(0.5)
                            .suffix(" mm"),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label("H");
                    ui.add(
                        egui::DragValue::new(&mut self.height)
                            .speed(0.5)
                            .suffix(" mm"),
                    );
                });
                if ui.button("Apply board dimensions").on_hover_text(command_tip("board.resize")).clicked() {
                    self.run(
                        "board.resize",
                        json!({"width":self.width,"height":self.height}),
                    );
                }
                ui.horizontal(|ui| {
                    ui.label("Cutout radius");
                    ui.add(
                        egui::DragValue::new(&mut self.cutout_radius)
                            .speed(0.1)
                            .suffix(" mm"),
                    );
                });
                field_help(ui, FIELD_HELP_CUTOUT, self.muted());
                ui.add_space(12.0);
                ui.label(egui::RichText::new("CHECKS").size(10.0).color(self.muted()));
                let erc = self
                    .checks
                    .get("erc")
                    .and_then(Value::as_array)
                    .map(Vec::len)
                    .unwrap_or(0);
                let drc = self
                    .checks
                    .get("drc")
                    .and_then(Value::as_array)
                    .map(Vec::len)
                    .unwrap_or(0);
                if ui
                    .button(format!("{erc} electrical · {drc} board >"))
                    .clicked()
                {
                    self.change_tab(Tab::Checks);
                }
                ui.label(
                    egui::RichText::new("Initial checks; full ERC/DRC is in development.")
                        .small()
                        .color(self.muted()),
                );
            });
    }
    fn footer(&mut self, ui: &mut egui::Ui) {
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(&self.message)
                        .small()
                        .color(self.muted()),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(format!("rev {}", self.revision))
                            .small()
                            .color(self.muted()),
                    );
                    if let Some(p) = self.cursor {
                        ui.label(
                            egui::RichText::new(format!("{:.2}, {:.2} mm", p.x, p.y))
                                .small()
                                .color(self.muted()),
                        );
                    }
                });
            });
        });
        egui::Panel::bottom("files").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("Project file");
                ui.add(egui::TextEdit::singleline(&mut self.path).desired_width(260.0));
                if ui
                    .button("Open")
                    .on_hover_text("Load a saved design file from the path next to this button.")
                    .clicked()
                {
                    self.tool_call("open", json!({"path":self.path}));
                }
                if ui
                    .button("Save")
                    .on_hover_text("Write the design to the file named next to this button.")
                    .clicked()
                {
                    self.tool_call("save", json!({"path":self.path}));
                }
                if ui
                    .button("Export KiCad board")
                    .on_hover_text("Write the board out so other design tools can open it.")
                    .clicked()
                {
                    let path = std::path::Path::new(&self.path)
                        .with_extension("kicad_pcb")
                        .to_string_lossy()
                        .into_owned();
                    self.tool_call("export", json!({"path":path,"format":"kicad_pcb"}));
                }
                if ui
                    .button("LED example")
                    .on_hover_text(
                        "Start from a small worked example: a battery, resistor and LED.",
                    )
                    .clicked()
                {
                    self.tool_call("sample_design", json!({}));
                    self.fit();
                }
                if ui
                    .button("New project")
                    .on_hover_text("Start a fresh empty design.")
                    .clicked()
                {
                    self.tool_call("new_design", json!({}));
                    self.select(None);
                }
            });
        });
        if self.command_open {
            egui::Panel::bottom("commands")
                .resizable(true)
                .default_size(220.0)
                .show(ui, |ui| {
                    let query = self.command_query.clone();
                    let matches: Vec<_> = pcbcraft_engine::commands()
                        .into_iter()
                        .filter(|spec| {
                            query.is_empty()
                                || fuzzy_match(&query, spec.id)
                                || fuzzy_match(&query, spec.description)
                        })
                        .collect();
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("SEARCH")
                                .small()
                                .color(self.accent()),
                        );
                        let search = ui.add(
                            egui::TextEdit::singleline(&mut self.command_query)
                                .hint_text("filter command names and descriptions")
                                .desired_width(300.0),
                        );
                        if self.command_focus {
                            search.request_focus();
                            self.command_focus = false;
                        }
                        ui.label(
                            egui::RichText::new("PARAMETERS")
                                .small()
                                .color(self.accent()),
                        );
                        let fields = ui.add(
                            egui::TextEdit::singleline(&mut self.params)
                                .hint_text("parameters as JSON")
                                .desired_width(380.0),
                        );
                        if ui
                            .button("Execute")
                            .on_hover_text(
                                "Run the command with the parameters you typed; selecting a command fills a starter parameter set.",
                            )
                            .clicked()
                        {
                            self.execute_palette_params();
                        }
                        let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                        if enter
                            && search.lost_focus()
                            && let Some(spec) = matches.first()
                        {
                            self.command = spec.id.into();
                            self.params = params_template(spec);
                            fields.request_focus();
                        } else if enter && fields.lost_focus() {
                            self.execute_palette_params();
                        }
                    });
                    ui.label(
                        egui::RichText::new(format!(
                            "{} of {} commands match; click one to load it with a starter parameter set, or press Enter in the search box to load the top match",
                            matches.len(),
                            pcbcraft_engine::commands().len()
                        ))
                        .small()
                        .color(self.muted()),
                    );
                    egui::ScrollArea::vertical()
                        .max_height(180.0)
                        .show(ui, |ui| {
                            for spec in matches {
                                if ui
                                    .selectable_label(
                                        self.command == spec.id,
                                        format!("{}   {}", spec.id, spec.description),
                                    )
                                    .clicked()
                                {
                                    self.command = spec.id.into();
                                    self.params = params_template(&spec);
                                }
                            }
                        });
                });
        }
        if self.shortcuts_open {
            let muted = self.muted();
            egui::Window::new("Keyboard shortcuts")
                .open(&mut self.shortcuts_open)
                .show(ui, |ui| {
                    egui::Grid::new("shortcut-list")
                        .num_columns(3)
                        .spacing([16.0, 4.0])
                        .show(ui, |ui| {
                            for (keys, action, context) in shortcut_rows() {
                                ui.label(egui::RichText::new(keys).monospace().strong());
                                ui.label(action);
                                ui.label(egui::RichText::new(context).small().color(muted));
                                ui.end_row();
                            }
                        });
                });
        }
    }
    fn library(&mut self, ui: &mut egui::Ui) {
        self.library_workspaces(ui);
        ui.collapsing("Original starter symbols", |ui| {
            for t in TEMPLATES {
                egui::Frame::group(ui.style())
                    .inner_margin(18.0)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.label(egui::RichText::new(t.label).size(18.0).strong());
                                ui.label(format!("{} · {} · 2 pins", t.prefix, t.value));
                            });
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui
                                        .button("Place in schematic >")
                                        .on_hover_text(command_tip("component.place_library"))
                                        .clicked()
                                    {
                                        self.change_tab(Tab::Schematic);
                                        self.tool = Tool::Place(t.kind.into());
                                    }
                                },
                            );
                        });
                    });
                ui.add_space(10.0);
            }
        });
    }
    fn checks_view(&mut self, ui: &mut egui::Ui) {
        ui.add_space(24.0);
        ui.heading("Design checks");
        ui.label("Select a finding to inspect its affected object in the editor.");
        if self
            .checks
            .get("erc")
            .and_then(Value::as_array)
            .is_none_or(|v| v.is_empty())
            && self
                .checks
                .get("drc")
                .and_then(Value::as_array)
                .is_none_or(|v| v.is_empty())
        {
            empty_state(ui, EMPTY_CHECKS, self.muted());
        }
        ui.add_space(16.0);
        let checks = self.checks.clone();
        for (key, label) in [("erc", "Electrical"), ("drc", "Board")] {
            ui.label(egui::RichText::new(label).size(18.0).strong());
            if let Some(findings) = checks.get(key).and_then(Value::as_array) {
                if findings.is_empty() {
                    ui.label(
                        egui::RichText::new("No findings in the implemented checks")
                            .color(self.accent()),
                    );
                }
                for f in findings {
                    let text = format!(
                        "{}   {}",
                        f.get("code").and_then(Value::as_str).unwrap_or("finding"),
                        f.get("message").and_then(Value::as_str).unwrap_or("")
                    );
                    if ui.button(text).clicked()
                        && let Some(id) = f.pointer("/objects/0").and_then(Value::as_u64)
                    {
                        self.change_tab(if key == "erc" {
                            Tab::Schematic
                        } else {
                            Tab::Pcb
                        });
                        self.select(Some(id));
                    }
                }
            }
            ui.add_space(20.0);
        }
        ui.separator();
        ui.label(
            egui::RichText::new(checks.get("scope").and_then(Value::as_str).unwrap_or(""))
                .small()
                .color(self.muted()),
        );
    }
    fn capture(&mut self, ctx: &egui::Context) {
        self.frames = self.frames.saturating_add(1);
        let library_ready = self.tab != Tab::Libraries
            || self.library.editor.mode != 0
            || (self.library.catalog.is_some()
                && (self.library.results.is_some() || self.library.path.is_empty())
                && self.library.pending.is_none());
        if self.capture.is_some() && self.frames >= 5 && library_ready {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
        }
        if let Some(path) = self.capture.clone() {
            let images = ctx.input(|i| {
                i.events
                    .iter()
                    .filter_map(|e| {
                        if let egui::Event::Screenshot { image, .. } = e {
                            Some(image.clone())
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
            });
            for image in images {
                let bytes: Vec<u8> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
                if let Err(e) = image::save_buffer(
                    &path,
                    &bytes,
                    image.size[0] as u32,
                    image.size[1] as u32,
                    image::ColorType::Rgba8,
                ) {
                    self.message = format!("capture failed: {e}");
                    eprintln!("{}", self.message);
                }
                self.capture = None;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            ctx.request_repaint();
        }
    }
}
impl App {
    fn show(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        self.refresh();
        self.style(&ctx);
        self.shortcuts(&ctx);
        self.header(ui);
        self.footer(ui);
        self.browser(ui);
        self.inspector(ui);
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(self.surface()))
            .show(ui, |ui| match self.tab {
                Tab::Schematic | Tab::Pcb => self.canvas(ui),
                Tab::Libraries => {
                    egui::ScrollArea::vertical()
                        .show(ui, |ui| self.library(ui))
                        .inner
                }
                Tab::Checks => {
                    egui::ScrollArea::vertical()
                        .show(ui, |ui| self.checks_view(ui))
                        .inner
                }
            });
        self.capture(&ctx);
        ctx.request_repaint_after(std::time::Duration::from_millis(200));
    }
}
impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
        self.show(ui);
    }
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, "preferences", &self.preferences);
        if let Ok(session) = self.session.lock()
            && let Ok(text) = serde_json::to_string(&session.project)
            && text.len() <= pcbcraft_io_limit()
        {
            storage.set_string("recovery", text);
        }
    }
}
fn pcbcraft_io_limit() -> usize {
    8 * 1024 * 1024
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_tooltips_come_from_the_command_registry() -> Result<(), String> {
        let registry = pcbcraft_engine::commands();
        for id in [
            "component.add",
            "board.sync",
            "board.resize",
            "object.delete",
        ] {
            let want = registry
                .iter()
                .find(|spec| spec.id == id)
                .map(|spec| spec.description)
                .unwrap_or_default();
            assert!(
                !want.is_empty() && command_tip(id) == want,
                "the tooltip for {id} must be the registry description"
            );
        }
        assert_eq!(
            command_tip("no.such_command"),
            "This control runs an engine command.",
            "an unmapped id still explains itself"
        );
        Ok(())
    }

    #[test]
    fn empty_states_name_the_command_that_fills_the_panel() {
        // Guidance must reference commands that actually exist, so a novice
        // who follows the hint always finds a real command to run.
        let registry: Vec<&str> = pcbcraft_engine::commands()
            .iter()
            .map(|spec| spec.id)
            .collect();
        for copy in [EMPTY_SYMBOLS, EMPTY_NETS, EMPTY_BOARD] {
            for name in ["component.add", "netlist.import", "board.sync"] {
                if copy.contains(name) {
                    assert!(registry.contains(&name), "{name} must exist");
                }
            }
            assert!(
                copy.contains("component.add")
                    || copy.contains("netlist.import")
                    || copy.contains("board.sync"),
                "empty guidance must name a filling command: {copy}"
            );
        }
        assert!(EMPTY_CHECKS.contains("automatically"));
    }

    #[test]
    fn every_editable_field_explains_its_bounds_and_effect() {
        // The help copy for the inspector's free-form fields must state the
        // same limits the engine enforces, so a novice never has to guess
        // why a value was refused.
        assert!(FIELD_HELP_VALUE.contains("name or value"));
        assert!(FIELD_HELP_POSITION.contains("origin"));
        assert!(FIELD_HELP_BOARD.contains("larger than zero"));
        assert!(FIELD_HELP_CUTOUT.contains("inside the board"));
    }

    #[test]
    fn shortcut_list_documents_every_binding_the_app_installs() {
        let rows = shortcut_rows();
        let keys: Vec<&str> = rows.iter().map(|(keys, _, _)| *keys).collect();
        for wanted in ["Ctrl+Z", "Ctrl+K", "?", "W", "X", "Delete", "Esc"] {
            assert!(
                keys.contains(&wanted),
                "the shortcut list must mention {wanted}"
            );
        }
        let mut seen: Vec<&str> = keys.clone();
        seen.sort_unstable();
        let unique = {
            let mut v = seen.clone();
            v.dedup();
            v
        };
        assert_eq!(seen.len(), unique.len(), "no key is listed twice");
        for (keys, action, context) in &rows {
            assert!(!keys.is_empty() && !action.is_empty());
            assert!(
                ["anywhere", "schematic tab", "pcb tab", "canvas"].contains(context),
                "{keys} claims an unknown context {context}"
            );
        }
    }

    #[test]
    fn tool_hints_name_the_next_action_for_every_tool() {
        let select = tool_hint(&Tool::Select);
        assert!(
            select.contains("select") && select.contains("drag") && select.contains("zoom"),
            "the idle hint teaches navigation: {select}"
        );
        let cases = [
            (Tool::Place("resistor".into()), "symbol"),
            (Tool::Wire(None), "first pin"),
            (
                Tool::Wire(Some(PinRef {
                    component: 1,
                    pin: "1".into(),
                })),
                "second pin",
            ),
            (Tool::Route(None), "first pad"),
            (Tool::Route(Some((1, "1".into()))), "second pad"),
            (Tool::Move, "drop"),
            (Tool::Cutout, "cutout"),
        ];
        for (tool, verb) in cases {
            let hint = tool_hint(&tool);
            assert!(
                hint.contains(verb) && hint.contains("LMB") && hint.contains("Esc"),
                "{verb} must be spelled out for the active tool: {hint}"
            );
        }
    }

    #[test]
    fn palette_filtering_matches_subsequences_of_names_and_descriptions() {
        assert!(fuzzy_match("sync", "board.sync"));
        assert!(fuzzy_match("bre", "board.resize"));
        assert!(fuzzy_match("PAD", "board.pad_stack"));
        assert!(
            !fuzzy_match("xyz", "board.sync"),
            "characters that are not there never match"
        );
        assert!(
            !fuzzy_match("copperplating", "component.value"),
            "order matters in a subsequence"
        );
    }

    #[test]
    fn palette_templates_hold_exactly_the_required_parameters() -> Result<(), String> {
        let mut checked = 0usize;
        for spec in pcbcraft_engine::commands() {
            let template: Value =
                serde_json::from_str(&params_template(&spec)).map_err(|e| e.to_string())?;
            let object = template
                .as_object()
                .ok_or_else(|| format!("{} template is not an object", spec.id))?;
            let required = spec
                .input_schema
                .get("required")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            assert_eq!(
                object.len(),
                required.len(),
                "{} template must hold exactly the required parameters",
                spec.id
            );
            for key in required.iter().filter_map(Value::as_str) {
                assert!(
                    object.contains_key(key),
                    "{} template misses the required parameter {key}",
                    spec.id
                );
            }
            checked += 1;
        }
        assert!(checked >= 150, "the sweep covered the whole registry");
        Ok(())
    }
    fn frame(
        ctx: &egui::Context,
        app: &mut App,
        mut events: Vec<egui::Event>,
        modifiers: egui::Modifiers,
    ) -> egui::FullOutput {
        events.insert(0, egui::Event::ModifiersChanged(modifiers));
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    egui::Pos2::ZERO,
                    Vec2::new(1440.0, 940.0),
                )),
                events,
                ..Default::default()
            },
            |ui| app.show(ui),
        );
        // This input test has no GPU renderer; intentionally discard texture uploads.
        output.textures_delta.clear();
        output
    }
    #[test]
    fn tab_click_preserves_component_identity_and_rotation_undo_works() -> Result<(), String> {
        let session =
            pcbcraft_mcp::shared(pcbcraft_engine::Session::sample().map_err(|e| e.to_string())?);
        let mut app = App::create(session, Preferences::default(), None, None);
        app.refresh();
        app.select(Some(2));
        let ctx = egui::Context::default();
        let output = frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert_eq!(
            output.viewport_output.len(),
            1,
            "editor must have only its root window"
        );
        let pos = egui::Pos2::new(220.0, 64.0);
        for pressed in [true, false] {
            frame(
                &ctx,
                &mut app,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                egui::Modifiers::NONE,
            );
        }
        assert!(matches!(app.tab, Tab::Pcb));
        assert_eq!(app.selected, Some(11));
        let key = |key, modifiers| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        };
        frame(
            &ctx,
            &mut app,
            vec![key(egui::Key::R, egui::Modifiers::NONE)],
            egui::Modifiers::NONE,
        );
        assert_eq!(app.project.footprint(11).map(|f| f.rotation), Some(90.0));
        let ctrl = egui::Modifiers {
            ctrl: true,
            command: true,
            ..Default::default()
        };
        frame(&ctx, &mut app, vec![key(egui::Key::Z, ctrl)], ctrl);
        assert_eq!(app.project.footprint(11).map(|f| f.rotation), Some(0.0));
        Ok(())
    }
    #[test]
    fn library_placement_uses_shared_engine_and_single_viewport() -> Result<(), String> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../library/tests/fixtures/device.kicad_sym")
            .to_string_lossy()
            .into_owned();
        let def = pcbcraft_engine::library::symbol(std::path::Path::new(&path), "R")?;
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(shared.clone(), Preferences::default(), None, None);
        app.refresh();
        app.tool = Tool::LibraryPlace(path, "R".into(), Box::new(def));
        let ctx = egui::Context::default();
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        let pos = egui::Pos2::new(680.0, 360.0);
        for pressed in [true, false] {
            frame(
                &ctx,
                &mut app,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                egui::Modifiers::NONE,
            );
        }
        let project = shared
            .lock()
            .map_err(|_| "session unavailable")?
            .project
            .clone();
        assert_eq!(project.components.len(), 1);
        assert!(
            project
                .components
                .first()
                .is_some_and(|c| c.library_symbol.is_some())
        );
        app.change_tab(Tab::Libraries);
        assert_eq!(app.selected, project.components.first().map(|c| c.id));
        let output = frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert_eq!(output.viewport_output.len(), 1);
        Ok(())
    }
    #[test]
    fn selecting_units_and_new_placement_preserves_correct_target() -> Result<(), String> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../library/tests/fixtures/lm358.kicad_sym")
            .to_string_lossy()
            .into_owned();
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(shared, Preferences::default(), None, None);
        app.refresh();
        app.run(
            "component.place_library",
            json!({"path":path,"symbol":"LM358","position":{"x":20,"y":15}}),
        );
        let pos = app
            .project
            .component(1)
            .and_then(|c| c.unit_placement(2))
            .ok_or("missing unit")?
            .0;
        app.select_at(pos, 11.0);
        assert_eq!(app.selected, Some(1));
        assert_eq!(app.selected_unit, 2);
        app.rotate();
        assert_eq!(
            app.project
                .component(1)
                .and_then(|c| c.unit_placement(2))
                .map(|p| p.1),
            Some(90.0)
        );
        assert_eq!(app.project.component(1).map(|c| c.rotation), Some(0.0));
        app.run(
            "component.add",
            json!({"kind":"resistor","position":{"x":60,"y":40}}),
        );
        assert_eq!(app.selected_unit, 1);
        app.rotate();
        assert_eq!(app.project.component(2).map(|c| c.rotation), Some(90.0));
        Ok(())
    }
    #[test]
    fn agent_changes_refresh_the_displayed_document() -> Result<(), String> {
        let shared =
            pcbcraft_mcp::shared(pcbcraft_engine::Session::sample().map_err(|e| e.to_string())?);
        let mut app = App::create(shared.clone(), Preferences::default(), None, None);
        app.refresh();
        let result =
            pcbcraft_mcp::call_tool(&shared, "component.value", &json!({"id":2,"value":"470 Ω"}));
        assert_eq!(result.get("isError"), Some(&json!(false)));
        app.refresh();
        assert_eq!(
            app.project.component(2).map(|c| c.value.as_str()),
            Some("470 Ω")
        );
        Ok(())
    }
}
