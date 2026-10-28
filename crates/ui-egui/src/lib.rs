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
        (
            "Up/Down",
            "Move the highlight through the command list",
            "command palette",
        ),
        (
            "Enter",
            "Load the highlighted command, then run it from the parameters box",
            "command palette",
        ),
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

/// Move the palette highlight by `delta` rows, wrapping around the ends so
/// the list behaves like a ring; an empty list always lands on row 0 and a
/// stale index (after the filter shrinks the list) is pulled back in range.
fn step_index(current: usize, count: usize, delta: isize) -> usize {
    if count == 0 {
        return 0;
    }
    let base = current.min(count - 1) as isize;
    (base + delta).rem_euclid(count as isize) as usize
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

/// One line per required parameter of a command, read from the command's own
/// schema: the parameter name, the shape it accepts (type, bounds, options)
/// and the schema's own description. The palette shows this under the search
/// row so the parameters are explained before a command is ever refused.
fn param_help(spec: &pcbcraft_engine::CommandSpec) -> Vec<String> {
    let schema = &spec.input_schema;
    let properties = schema.get("properties").cloned().unwrap_or(Value::Null);
    schema
        .get("required")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(Value::as_str)
                .filter_map(|key| {
                    properties.get(key).map(|field| {
                        let mut line = format!("{key} ({}):", type_note(field));
                        match field.get("description").and_then(Value::as_str) {
                            Some(text) => line.push_str(&format!(" {text}")),
                            None => line.push_str(" required"),
                        }
                        line
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Plain-words shape of one schema field: the listed options when there are
/// any, otherwise the type with the bounds the schema states.
fn type_note(field: &Value) -> String {
    if let Some(options) = field.get("enum").and_then(Value::as_array) {
        let names: Vec<String> = options
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect();
        if !names.is_empty() {
            return format!("one of {}", names.join(" or "));
        }
    }
    match field.get("type").and_then(Value::as_str) {
        Some("integer") | Some("number") => {
            let min = field
                .get("minimum")
                .or_else(|| field.get("exclusiveMinimum"))
                .and_then(Value::as_f64);
            let max = field.get("maximum").and_then(Value::as_f64);
            match (min, max) {
                (Some(lo), Some(hi)) => format!("number {lo} to {hi}"),
                (Some(lo), None) => format!("number from {lo}"),
                (None, Some(hi)) => format!("number up to {hi}"),
                (None, None) => "number".into(),
            }
        }
        Some("string") => "text".into(),
        Some("boolean") => "true or false".into(),
        Some("array") => "list".into(),
        Some("object") => "object".into(),
        _ => "value".into(),
    }
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

#[derive(Clone, Copy, Debug, PartialEq)]
enum Tab {
    Schematic,
    Pcb,
    Libraries,
    Checks,
}
/// What the board canvas draws at each pad centre.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, Default)]
enum PadLabels {
    /// The pad's own number, the default assembly view.
    #[default]
    Number,
    /// The net the pad sits on, blank for unconnected pads.
    Net,
    /// Nothing, for a clean copper picture.
    Off,
}
impl PadLabels {
    fn name(&self) -> &'static str {
        match self {
            Self::Number => "Pads: number",
            Self::Net => "Pads: net",
            Self::Off => "Pads: off",
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
struct Preferences {
    dark: bool,
    grid: bool,
    accent: [u8; 3],
    pad_labels: PadLabels,
    #[serde(default)]
    hidden_layers: Vec<String>,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            dark: true,
            grid: true,
            accent: [91, 224, 188],
            pad_labels: PadLabels::Number,
            hidden_layers: Vec::new(),
        }
    }
}
#[derive(Clone, Debug)]
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
    recent_designs: Vec<String>,
    home_open: bool,
    home_path_focus: bool,
    left_tab: usize,
    make_dir: String,
    rule_draft: Option<(pcbcraft_doc::Id, f64, f64)>,
    net_editor: Option<pcbcraft_doc::Id>,
    pin_pick: Option<String>,
    net_draft: Option<(pcbcraft_doc::Id, String)>,
    appearance: bool,
    palette: bool,
    part_search: String,
    svg_path: String,
    part_search_focus: bool,
    needs_fit: bool,
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
    command_index: usize,
    command_scroll: bool,
    capture: Option<String>,
    frames: u32,
    library: library::Browser,
}
/// Plain-language facts about a selected board or schematic object that is not
/// a component, so the properties panel can describe tracks, vias, zones,
/// labels, wires and graphics instead of showing a bare id.
fn object_detail_rows(project: &Project, id: Id) -> Vec<(String, String)> {
    let net = |id: Id| project.net_name(id).to_string();
    let layer = |value: &pcbcraft_doc::Layer| {
        serde_json::to_value(value)
            .ok()
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_default()
    };
    let mut rows: Vec<(String, String)> = Vec::new();
    if let Some(t) = project.board.tracks.iter().find(|t| t.id == id) {
        rows.push(("Net".into(), net(t.net)));
        rows.push(("Layer".into(), layer(&t.layer)));
        rows.push(("Width".into(), format!("{:.3} mm", t.width)));
        rows.push((
            "From".into(),
            format!("{:.2}, {:.2} mm", t.start.x, t.start.y),
        ));
        rows.push(("To".into(), format!("{:.2}, {:.2} mm", t.end.x, t.end.y)));
        if t.arc.is_some() {
            rows.push(("Shape".into(), "curved through a midpoint".into()));
        }
    } else if let Some(v) = project.board.vias.iter().find(|v| v.id == id) {
        rows.push(("Net".into(), net(v.net)));
        rows.push(("Copper".into(), format!("{:.2} mm", v.diameter)));
        rows.push(("Drill".into(), format!("{:.2} mm", v.drill)));
        rows.push((
            "Position".into(),
            format!("{:.2}, {:.2} mm", v.position.x, v.position.y),
        ));
    } else if let Some(z) = project.board.zones.iter().find(|z| z.id == id) {
        rows.push(("Net".into(), net(z.net)));
        rows.push(("Layer".into(), layer(&z.layer)));
        rows.push((
            "Copper".into(),
            if z.filled {
                "poured".into()
            } else {
                "outline only".into()
            },
        ));
        rows.push(("Outline".into(), format!("{} vertices", z.outline.len())));
        if !z.cutouts.is_empty() {
            rows.push(("Cutouts".into(), format!("{}", z.cutouts.len())));
        }
    } else if let Some(l) = project.labels.iter().find(|l| l.id == id) {
        rows.push(("Kind".into(), l.kind.clone()));
        rows.push(("Net".into(), net(l.net)));
        rows.push((
            "Position".into(),
            format!("{:.2}, {:.2} mm", l.position.x, l.position.y),
        ));
    } else if let Some(w) = project.drawn_wires.iter().find(|w| w.id == id) {
        rows.push((
            "Net".into(),
            w.net.map(net).unwrap_or_else(|| "unbound".into()),
        ));
        rows.push(("Points".into(), format!("{}", w.points.len())));
    } else if let Some(t) = project.board.texts.iter().find(|t| t.id == id) {
        rows.push(("Text".into(), t.text.clone()));
        rows.push(("Size".into(), format!("{:.2} mm", t.size)));
        rows.push(("Layer".into(), layer(&t.layer)));
        rows.push(("Angle".into(), format!("{:.0} degrees", t.rotation)));
    } else if let Some(g) = project.graphics.iter().find(|g| g.id == id) {
        rows.push(("Shape".into(), g.shape.clone()));
        rows.push(("Layer".into(), g.layer.clone()));
        if !g.text.is_empty() {
            rows.push(("Text".into(), g.text.clone()));
        }
        rows.push(("Stroke".into(), format!("{:.3} mm", g.width)));
        rows.push(("Points".into(), format!("{}", g.points.len())));
    } else if let Some(gr) = project.graphic_groups.iter().find(|gr| gr.id == id) {
        rows.push((
            "Group".into(),
            format!("{} graphics move as one", gr.graphics.len()),
        ));
    }
    rows
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
        let show_home = capture.is_none();
        let mut app = Self::create(session, preferences, capture, tab);
        app.home_open = show_home && app.project.components.is_empty();
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
            recent_designs: pcbcraft_engine::recents::load_recent_paths(),
            home_open: false,
            home_path_focus: false,
            left_tab: 0,
            make_dir: String::new(),
            rule_draft: None,
            net_editor: None,
            pin_pick: None,
            net_draft: None,
            appearance: false,
            palette: false,
            part_search: String::new(),
            svg_path: String::new(),
            part_search_focus: false,
            needs_fit: true,
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
            command_index: 0,
            command_scroll: false,
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
            pcbcraft_engine::recents::record_recent_path(&self.path);
            self.recent_designs = pcbcraft_engine::recents::load_recent_paths();
            self.saved_revision = Some(self.revision);
        }
        if success && (name == "open" || name == "new_design" || name == "sample_design") {
            self.needs_fit = true;
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
        self.needs_fit = true;
        self.tool = Tool::Select;
        self.drag = None;
        self.load_properties();
    }
    fn layer_shown(&self, name: &str) -> bool {
        !self
            .preferences
            .hidden_layers
            .iter()
            .any(|hidden| hidden == name)
    }
    fn layers_tab(&mut self, ui: &mut egui::Ui) {
        let rows: &[&str] = match self.tab {
            Tab::Schematic => &["Grid", "Symbols", "Wires", "Net labels", "Graphics"],
            Tab::Pcb => &[
                "Grid",
                "Top copper",
                "Bottom copper",
                "Pads",
                "Vias",
                "Copper zones",
                "Cutouts",
                "Package artwork",
            ],
            _ => &[],
        };
        if rows.is_empty() {
            ui.label(
                egui::RichText::new(
                    "Switch to the schematic or the board to show or hide parts of the drawing.",
                )
                .small()
                .color(self.muted()),
            );
            ui.add_space(20.0);
            self.board_summary(ui);
            return;
        }
        ui.label(
            egui::RichText::new("WHAT THE DRAWING SHOWS")
                .size(10.0)
                .color(self.muted()),
        );
        for name in rows {
            let mut shown = self.layer_shown(name);
            if ui
                .checkbox(&mut shown, *name)
                .on_hover_text(format!(
                    "Show or hide {name} on the drawing. Hiding changes only the view, never the design."
                ))
                .clicked()
            {
                if shown {
                    self.preferences.hidden_layers.retain(|hidden| hidden != name);
                } else {
                    self.preferences.hidden_layers.push((*name).to_string());
                }
            }
        }
        ui.add_space(20.0);
        self.board_summary(ui);
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
                if i.key_pressed(egui::Key::F) {
                    self.needs_fit = true;
                }
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
                    self.command_index = 0;
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
                self.needs_fit = true;
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
        egui::Panel::top("header").exact_size(100.0).show(ui, |ui| {
            ui.add_space(7.0);
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                let brand = ui.add(
                    egui::Label::new(
                        egui::RichText::new("PCBCraft")
                            .size(20.0)
                            .strong()
                            .color(self.accent()),
                    )
                    .sense(egui::Sense::click()),
                );
                if brand
                    .on_hover_text("Back to the start screen, where you open or begin a design.")
                    .clicked()
                {
                    self.home_open = true;
                }
                if self.library.editor.mode == 0 {
                    ui.separator();
                    self.file_menu(ui);
                    self.edit_menu(ui);
                    self.settings_menu(ui);
                    self.help_menu(ui);
                }
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
                ui.label(
                    egui::RichText::new(tool_hint(&self.tool))
                        .small()
                        .color(self.muted()),
                );
            });
        });
    }
    fn start_home(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(self.surface()))
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.vertical_centered(|ui| {
                            ui.add_space(36.0);
                            ui.label(
                                egui::RichText::new("Welcome to PCBCraft")
                                    .size(30.0)
                                    .strong()
                                    .color(self.accent()),
                            );
                            ui.label(
                                egui::RichText::new(
                                    "Draw a circuit, place the parts, and finish with a board ready to order.",
                                )
                                .size(14.0)
                                .color(self.muted()),
                            );
                            ui.add_space(24.0);
                            if ui
                                .add_sized(
                                    [300.0, 64.0],
                                    egui::Button::new(
                                        (
                                            egui::RichText::new("New design")
                                                .size(16.0)
                                                .strong(),
                                            egui::RichText::new("Begin with a blank page.")
                                                .small()
                                                .color(self.muted()),
                                        ),
                                    ),
                                )
                                .on_hover_text("Start a fresh empty design.")
                                .clicked()
                            {
                                self.tool_call("new_design", json!({}));
                                self.select(None);
                                self.home_open = false;
                            }
                            ui.add_space(6.0);
                            if ui
                                .add_sized(
                                    [300.0, 64.0],
                                    egui::Button::new(
                                        (
                                            egui::RichText::new("Worked example")
                                                .size(16.0)
                                                .strong(),
                                            egui::RichText::new(
                                                "A small battery, resistor and LED to explore.",
                                            )
                                            .small()
                                            .color(self.muted()),
                                        ),
                                    ),
                                )
                                .on_hover_text(
                                    "Start from a small worked example: a battery, resistor and LED.",
                                )
                                .clicked()
                            {
                                self.tool_call("sample_design", json!({}));
                                self.home_open = false;
                            }
                            ui.add_space(6.0);
                            if ui
                                .add_sized(
                                    [300.0, 64.0],
                                    egui::Button::new(
                                        (
                                            egui::RichText::new("Open a design file…")
                                                .size(16.0)
                                                .strong(),
                                            egui::RichText::new(
                                                "Type a path, or pick one from the list below.",
                                            )
                                            .small()
                                            .color(self.muted()),
                                        ),
                                    ),
                                )
                                .on_hover_text(
                                    "Load a saved design file from the path beside this button.",
                                )
                                .clicked()
                            {
                                self.home_path_focus = true;
                            }
                            ui.add_space(6.0);
                            ui.horizontal(|ui| {
                                let path_box = ui
                                    .add(
                                        egui::TextEdit::singleline(&mut self.path)
                                            .hint_text("design file path")
                                            .desired_width(240.0),
                                    )
                                    .on_hover_text(
                                        "The file that Open on this page, and in the File menu, uses.",
                                    );
                                if self.home_path_focus {
                                    path_box.request_focus();
                                    self.home_path_focus = false;
                                }
                                if ui
                                    .button("Open")
                                    .on_hover_text(
                                        "Load the design file named in the box beside this button.",
                                    )
                                    .clicked()
                                {
                                    self.tool_call("open", json!({"path": self.path}));
                                    if self.message.starts_with("open") {
                                        self.home_open = false;
                                    }
                                }
                            });
                            if !self.recent_designs.is_empty() {
                                ui.add_space(18.0);
                                ui.label(
                                    egui::RichText::new("Open one you worked on recently")
                                        .size(14.0)
                                        .strong(),
                                );
                                for design in self.recent_designs.clone() {
                                    if ui
                                        .add(
                                            egui::Button::new(
                                                egui::RichText::new(&design).small(),
                                            )
                                            .min_size(egui::vec2(300.0, 28.0)),
                                        )
                                        .on_hover_text(
                                            "Open this design again where you left off.",
                                        )
                                        .clicked()
                                    {
                                        self.path = design;
                                        self.tool_call("open", json!({"path": self.path}));
                                        if self.message.starts_with("open") {
                                            self.home_open = false;
                                        }
                                    }
                                }
                            }
                            ui.add_space(14.0);
                            ui.label(
                                egui::RichText::new(
                                    "Press Escape, or click PCBCraft in the corner, to return here any time.",
                                )
                                .small()
                                .color(self.muted()),
                            );
                            ui.add_space(24.0);
                        });
                    });
            });
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.home_open = false;
        }
    }
    fn file_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("File", |ui| {
            if ui
                .button("New")
                .on_hover_text("Start a fresh empty design.")
                .clicked()
            {
                self.tool_call("new_design", json!({}));
                self.select(None);
            }
            if ui
                .button("Open")
                .on_hover_text(
                    "Choose a design to open; your recent designs are listed on the screen that appears.",
                )
                .clicked()
            {
                self.home_open = true;
            }
            if ui
                .button("Save")
                .on_hover_text("Write the design to its file on disk. (Ctrl+S works too.)")
                .clicked()
            {
                self.tool_call("save", json!({"path":self.path}));
            }
            ui.separator();
            if ui
                .button("Export interchange board")
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
                .button("Copy view picture to clipboard")
                .on_hover_text("Copy a PNG snapshot of the current view, the same picture a plotter would draw, so it can be pasted into a document.")
                .clicked()
            {
                let ctx = ui.ctx().clone();
                self.copy_plot_to_clipboard(&ctx);
            }
        })
        .response
        .on_hover_text("Start, open, save, export and share this design.");
    }
    fn edit_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("Edit", |ui| {
            if ui
                .button("Undo")
                .on_hover_text("Step back one action; every edit here is one undoable step.")
                .clicked()
            {
                self.tool_call("undo", json!({}));
            }
            if ui
                .button("Redo")
                .on_hover_text("Step forward to the action you undid.")
                .clicked()
            {
                self.tool_call("redo", json!({}));
            }
            ui.separator();
            if ui
                .button("Update board from schematic")
                .on_hover_text(
                    "Copy every symbol, wire and net onto the board, then show the board.",
                )
                .clicked()
            {
                self.run("board.sync", json!({}));
                self.change_tab(Tab::Pcb);
            }
            ui.separator();
            if let Some(lit) = self.highlight_net
                && let Some(net) = self.project.nets.iter().find(|n| n.id == lit)
            {
                let name = net.name.clone();
                if ui
                    .button(format!("Clear wiring on {name}"))
                    .on_hover_text(
                        "Lift every piece of copper drawn for the lit connection. The connection itself stays.",
                    )
                    .clicked()
                {
                    self.run("net.unroute", json!({"net": lit}));
                }
            }
            if ui
                .button("Clear all wiring")
                .on_hover_text(
                    "Start the copper over on the whole board. Parts, connections and copper zones stay.",
                )
                .clicked()
            {
                self.run("board.unroute", json!({}));
            }
        })
        .response
        .on_hover_text(
            "Step back and forward, sync the board, and start the wiring over on one connection or the whole board.",
        );
    }
    fn settings_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("Settings", |ui| {
            if ui
                .button("Appearance…")
                .on_hover_text("Pick the theme, accent colour and what sits on each pad.")
                .clicked()
            {
                self.appearance = !self.appearance;
            }
            if ui
                .button(if self.preferences.dark {
                    "Switch to the light theme"
                } else {
                    "Switch to the dark theme"
                })
                .on_hover_text("Change between the dark and light look.")
                .clicked()
            {
                self.preferences.dark = !self.preferences.dark;
            }
            if ui
                .button(if self.preferences.grid {
                    "Hide the grid"
                } else {
                    "Show the grid"
                })
                .on_hover_text("Show or hide the placement grid in the work area.")
                .clicked()
            {
                self.preferences.grid = !self.preferences.grid;
            }
            ui.menu_button("Route on", |ui| {
                ui.selectable_value(&mut self.route_layer, pcbcraft_doc::Layer::Front, "Top");
                ui.selectable_value(&mut self.route_layer, pcbcraft_doc::Layer::Back, "Bottom");
            })
            .response
            .on_hover_text("Choose which copper layer new tracks are drawn on.");
            ui.menu_button("Pad labels", |ui| {
                ui.selectable_value(
                    &mut self.preferences.pad_labels,
                    PadLabels::Number,
                    PadLabels::Number.name(),
                );
                ui.selectable_value(
                    &mut self.preferences.pad_labels,
                    PadLabels::Net,
                    PadLabels::Net.name(),
                );
                ui.selectable_value(
                    &mut self.preferences.pad_labels,
                    PadLabels::Off,
                    PadLabels::Off.name(),
                );
            })
            .response
            .on_hover_text(
                "Pick what sits at each pad centre on the board: its number, its net, or nothing.",
            );
            ui.separator();
            if ui
                .button("Library sources…")
                .on_hover_text("Manage the folders that feed the component list.")
                .clicked()
            {
                self.change_tab(Tab::Libraries);
            }
        })
        .response
        .on_hover_text("Theme, grid and library folders.");
    }
    fn help_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("Help", |ui| {
            if ui
                .button("Keyboard shortcuts")
                .on_hover_text("Show every keyboard shortcut and mouse gesture in one list.")
                .clicked()
            {
                self.shortcuts_open = !self.shortcuts_open;
            }
            if ui
                .button("Commands  Ctrl+K")
                .on_hover_text(
                    "Search every command by name and run it, with its parameters explained.",
                )
                .clicked()
            {
                self.command_open = !self.command_open;
                self.command_focus = self.command_open;
                self.command_index = 0;
            }
        })
        .response
        .on_hover_text("Shortcuts list and the command search.");
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
                ui.horizontal(|ui| {
                    if ui
                        .selectable_label(self.left_tab == 0, "Parts")
                        .on_hover_text(
                            "Everything placed in this design, and quick ways to add more.",
                        )
                        .clicked()
                    {
                        self.left_tab = 0;
                    }
                    if ui
                        .selectable_label(self.left_tab == 1, "Nets")
                        .on_hover_text(
                            "Every wire connection by name: add one, rename one, or light one up.",
                        )
                        .clicked()
                    {
                        self.left_tab = 1;
                    }
                    if ui
                        .selectable_label(self.left_tab == 2, "Layers")
                        .on_hover_text(
                            "Tick what the drawing shows: hide copper, silk or wires without touching the design.",
                        )
                        .clicked()
                    {
                        self.left_tab = 2;
                    }
                    if ui
                        .selectable_label(self.left_tab == 3, "Rules")
                        .on_hover_text(
                            "The gaps and wire widths the checks hold this design to, per group of connections.",
                        )
                        .clicked()
                    {
                        self.left_tab = 3;
                    }
                });
                ui.separator();
                if self.left_tab == 1 {
                    self.nets_tab(ui);
                    ui.add_space(20.0);
                    self.board_summary(ui);
                    return;
                }
                if self.left_tab == 2 {
                    self.layers_tab(ui);
                    return;
                }
                if self.left_tab == 3 {
                    self.rules_tab(ui);
                    return;
                }
                if ui
                    .button("Add part")
                    .on_hover_text("Search for a part to place on the schematic.")
                    .clicked()
                {
                    self.palette = true;
                    self.part_search_focus = true;
                }
                if self.palette {
                    ui.label(egui::RichText::new("Add a part").strong());
                    let query_box = ui
                        .add(
                            egui::TextEdit::singleline(&mut self.part_search)
                                .hint_text("Search parts by name")
                                .desired_width(190.0),
                        )
                        .on_hover_text(
                            "Type any part name; the list below narrows as you type.",
                        );
                    if self.part_search_focus {
                        query_box.request_focus();
                        self.part_search_focus = false;
                    }
                    if ui
                        .button("Browse component libraries →")
                        .on_hover_text(
                            "Open the library panel to browse installed symbol and footprint collections.",
                        )
                        .clicked()
                    {
                        self.change_tab(Tab::Libraries);
                        self.palette = false;
                    }
                    let query = self.part_search.trim().to_lowercase();
                    let built_in: Vec<(&str, &str)> = TEMPLATES
                        .iter()
                        .filter(|t| {
                            query.is_empty()
                                || t.label.to_lowercase().contains(&query)
                                || t.kind.contains(query.as_str())
                        })
                        .map(|t| (t.kind, t.label))
                        .collect();
                    let saved: Vec<pcbcraft_library::SymbolDefinition> = self
                        .project
                        .library
                        .symbols
                        .iter()
                        .filter(|d| {
                            query.is_empty() || d.name.to_lowercase().contains(&query)
                        })
                        .cloned()
                        .collect();
                    if built_in.is_empty() && saved.is_empty() {
                        ui.label(
                            egui::RichText::new(format!(
                                "Nothing here matches \"{}\" yet.",
                                self.part_search.trim()
                            ))
                            .small()
                            .color(self.muted()),
                        );
                    }
                    if !built_in.is_empty() {
                        ui.label(
                            egui::RichText::new("Common parts")
                                .size(10.0)
                                .color(self.muted()),
                        );
                        for (kind, label) in built_in {
                            if ui
                                .button(label)
                                .on_hover_text(
                                    "Pick this part; the next click on the schematic places it.",
                                )
                                .clicked()
                            {
                                self.change_tab(Tab::Schematic);
                                self.tool = Tool::Place(kind.into());
                                self.palette = false;
                                self.part_search.clear();
                            }
                        }
                    }
                    if !saved.is_empty() {
                        ui.label(
                            egui::RichText::new("From this project's library")
                                .size(10.0)
                                .color(self.muted()),
                        );
                        for definition in saved {
                            if ui
                                .button(&definition.name)
                                .on_hover_text(
                                    "Pick this saved part; the next click on the schematic places it.",
                                )
                                .clicked()
                            {
                                self.change_tab(Tab::Schematic);
                                self.tool = Tool::CustomPlace(
                                    definition.name.clone(),
                                    Box::new(definition),
                                );
                                self.palette = false;
                                self.part_search.clear();
                            }
                        }
                    }
                    if ui
                        .small_button("Close")
                        .on_hover_text("Hide the part search without changing anything.")
                        .clicked()
                    {
                        self.palette = false;
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
                ui.label(egui::RichText::new("BOARD").size(10.0).color(self.muted()));
                self.board_body(ui);
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
    /// The Nets tab of the left dock: add, spotlight and rename connections.
    fn nets_tab(&mut self, ui: &mut egui::Ui) {
        if let Some(lit) = self.highlight_net
            && !self.project.nets.iter().any(|n| n.id == lit)
        {
            self.highlight_net = None;
        }
        if self
            .net_draft
            .as_ref()
            .is_some_and(|(id, _)| !self.project.nets.iter().any(|n| n.id == *id))
        {
            self.net_draft = None;
        }
        if self
            .net_editor
            .is_some_and(|id| !self.project.nets.iter().any(|n| n.id == id))
        {
            self.net_editor = None;
            self.pin_pick = None;
        }
        if ui
            .button("New net")
            .on_hover_text("Add a blank connection you can name and draw wires onto.")
            .clicked()
        {
            let name = format!("Net {}", self.project.nets.len() + 1);
            self.tool_call("net.create", json!({"name": name}));
            self.net_editor = self
                .project
                .nets
                .iter()
                .rev()
                .find(|n| n.name == name)
                .map(|n| n.id);
        }
        ui.add_space(4.0);
        for net in self.project.nets.clone() {
            let lit = self.highlight_net == Some(net.id);
            let renaming = self.net_draft.as_ref().is_some_and(|(id, _)| *id == net.id);
            ui.horizontal(|ui| {
                if ui
                    .add(egui::Label::new(if lit { "\u{25cf}" } else { "\u{25cb}" }).sense(
                        egui::Sense::click(),
                    ))
                    .on_hover_text(
                        "Spotlight this net: its copper, vias and pads light up on the board while everything else fades. Click again to clear the spotlight.",
                    )
                    .clicked()
                {
                    self.highlight_net = if lit { None } else { Some(net.id) };
                }
                if renaming {
                    let Some((_, draft)) = self.net_draft.as_mut() else {
                        return;
                    };
                    ui.add(
                        egui::TextEdit::singleline(draft)
                            .desired_width(120.0)
                            .hint_text("net name"),
                    )
                    .on_hover_text("1 to 128 characters; every net needs its own name.");
                    if ui
                        .button("Use name")
                        .on_hover_text("Keep this name for the connection.")
                        .clicked()
                    {
                        let name = draft.clone();
                        self.net_draft = None;
                        self.tool_call("net.rename", json!({"id": net.id, "name": name}));
                    }
                    if ui
                        .button("Cancel")
                        .on_hover_text("Leave the name as it was.")
                        .clicked()
                    {
                        self.net_draft = None;
                    }
                } else {
                    if ui
                        .selectable_label(lit, &net.name)
                        .on_hover_text(
                            "Spotlight this net: its copper, vias and pads light up on the board while everything else fades. Click again to clear the spotlight.",
                        )
                        .clicked()
                    {
                        self.highlight_net = if lit { None } else { Some(net.id) };
                    }
                    if ui
                        .small_button("Rename")
                        .on_hover_text("Type a new name for this connection.")
                        .clicked()
                    {
                        self.net_draft = Some((net.id, net.name.clone()));
                    }
                    let open = self.net_editor == Some(net.id);
                    if ui
                        .small_button(if open { "Hide pins" } else { "Pins" })
                        .on_hover_text("Show or hide the list of pins on this connection.")
                        .clicked()
                    {
                        self.net_editor = if open { None } else { Some(net.id) };
                        self.pin_pick = None;
                    }
                }
            });
            if self.net_editor == Some(net.id) {
                let mut members: Vec<(String, PinRef)> = Vec::new();
                let mut spares: Vec<(String, PinRef)> = Vec::new();
                for comp in &self.project.components {
                    for pin in &comp.pins {
                        let label = if pin.name.is_empty() || pin.name == pin.number {
                            format!("{} pin {}", comp.reference, pin.number)
                        } else {
                            format!("{} pin {} ({})", comp.reference, pin.number, pin.name)
                        };
                        let r = PinRef {
                            component: comp.id,
                            pin: pin.number.clone(),
                        };
                        if pin.net == Some(net.id) {
                            members.push((label, r));
                        } else if pin.net.is_none() {
                            spares.push((label, r));
                        }
                    }
                }
                let mut pick = self.pin_pick.clone().unwrap_or_default();
                ui.indent(("net-pins", net.id), |ui| {
                    if members.is_empty() {
                        ui.label(
                            egui::RichText::new("No pins on this connection yet.")
                                .size(10.0)
                                .color(self.muted()),
                        );
                    } else {
                        for (label, r) in &members {
                            ui.horizontal(|ui| {
                                ui.label(label);
                                if ui
                                    .small_button("\u{d7}")
                                    .on_hover_text(
                                        "Take this pin off the connection. Nothing moves on the board.",
                                    )
                                    .clicked()
                                {
                                    self.tool_call(
                                        "net.disconnect",
                                        json!({"net": net.id, "pins": [r]}),
                                    );
                                    self.pin_pick = None;
                                }
                            });
                        }
                    }
                    ui.horizontal(|ui| {
                        egui::ComboBox::from_id_salt(("pin-pick", net.id))
                            .selected_text(if pick.is_empty() {
                                "choose a spare pin".to_owned()
                            } else {
                                pick.clone()
                            })
                            .show_ui(ui, |ui| {
                                for (label, _) in &spares {
                                    if ui
                                        .selectable_label(&pick == label, label)
                                        .on_hover_text(
                                            "A pin that is not on any connection yet.",
                                        )
                                        .clicked()
                                    {
                                        pick = label.clone();
                                        ui.close();
                                    }
                                }
                            })
                            .response
                            .on_hover_text("Pick a pin to add to this connection.");
                        if ui
                            .add_enabled(
                                !pick.is_empty(),
                                egui::Button::new("Connect"),
                            )
                            .on_hover_text("Add the pin chosen above to this connection.")
                            .clicked()
                            && let Some((_, r)) =
                                spares.iter().find(|(label, _)| *label == pick)
                        {
                            self.tool_call("net.connect", json!({"net": net.id, "pins": [r]}));
                            self.pin_pick = None;
                        }
                    });
                    if spares.is_empty() {
                        ui.label(
                            egui::RichText::new(
                                "Every pin in this design is already on a connection.",
                            )
                            .size(10.0)
                            .color(self.muted()),
                        );
                    }
                });
                self.pin_pick = if pick.is_empty() { None } else { Some(pick) };
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
    }
    fn rules_tab(&mut self, ui: &mut egui::Ui) {
        if self
            .rule_draft
            .is_some_and(|(id, _, _)| !self.project.net_classes.iter().any(|k| k.id == id))
        {
            self.rule_draft = None;
        }
        ui.label(
            egui::RichText::new(
                "Design rules travel with the design and steer the checks and any new wiring.",
            )
            .small()
            .color(self.muted()),
        );
        if ui
            .button("New class")
            .on_hover_text(
                "Add a rule set with a starting gap and wire width; rename and tune it from its row.",
            )
            .clicked()
        {
            let name = format!("Class {}", self.project.net_classes.len() + 1);
            self.run(
                "netclass.add",
                json!({"name": name, "clearance": 0.2, "width": 0.25}),
            );
        }
        ui.add_space(4.0);
        for class in self.project.net_classes.clone() {
            let editing = self.rule_draft.is_some_and(|(id, _, _)| id == class.id);
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(&class.name).strong());
                    ui.label(format!("{} nets", class.nets.len())).on_hover_text(
                        "Connections that follow this rule set. Assign them from the command search.",
                    );
                    if !editing {
                        if ui
                            .small_button("Tune")
                            .on_hover_text("Change this rule set's gap and wire width.")
                            .clicked()
                        {
                            self.rule_draft =
                                Some((class.id, class.clearance, class.width));
                        }
                        if ui
                            .small_button("Remove")
                            .on_hover_text(
                                "Delete this rule set; its connections fall back to the default rules.",
                            )
                            .clicked()
                        {
                            self.run("netclass.remove", json!({"class": class.id}));
                        }
                    }
                });
                if editing {
                    let Some((_, clearance, width)) = self.rule_draft.as_mut() else {
                        return;
                    };
                    ui.horizontal(|ui| {
                        ui.label("Gap");
                        ui.add(
                            egui::DragValue::new(clearance)
                                .speed(0.01)
                                .range(0.001..=100.0)
                                .suffix(" mm"),
                        )
                        .on_hover_text(
                            "Smallest allowed clear space between this group's copper and everything else.",
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.label("Wire width");
                        ui.add(
                            egui::DragValue::new(width)
                                .speed(0.01)
                                .range(0.001..=100.0)
                                .suffix(" mm"),
                        )
                        .on_hover_text(
                            "Width new tracks on these connections start with.",
                        );
                    });
                    ui.horizontal(|ui| {
                        if ui
                            .button("Apply")
                            .on_hover_text("Keep these numbers for this rule set.")
                            .clicked()
                            && let Some((id, clearance, width)) = self.rule_draft.take()
                        {
                            self.run(
                                "netclass.clearance",
                                json!({"class": id, "clearance": clearance}),
                            );
                            self.run("netclass.set", json!({"class": id, "width": width}));
                        }
                        if ui
                            .small_button("Cancel")
                            .on_hover_text("Leave this rule set as it was.")
                            .clicked()
                        {
                            self.rule_draft = None;
                        }
                    });
                } else {
                    ui.label(
                        egui::RichText::new(format!(
                            "Gap {:.3} mm · wire {:.3} mm",
                            class.clearance, class.width
                        ))
                        .small()
                        .color(self.muted()),
                    );
                }
            });
        }
        if self.project.net_classes.is_empty() {
            ui.label(
                egui::RichText::new(
                    "No rule sets yet; the default rules apply to every connection.",
                )
                .small()
                .color(self.muted()),
            );
        }
    }
    fn board_summary(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new("BOARD").size(10.0).color(self.muted()));
        self.board_body(ui);
    }
    /// The folder make files land in: the typed one, or "outputs" beside the design.
    fn make_directory(&self) -> String {
        if !self.make_dir.trim().is_empty() {
            return self.make_dir.trim().to_owned();
        }
        let parent = std::path::Path::new(&self.path)
            .parent()
            .map_or_else(|| std::path::PathBuf::from("."), |p| p.to_path_buf());
        parent.join("outputs").to_string_lossy().into_owned()
    }
    /// Run a write-only make file tool into the make folder, creating it first.
    fn write_make_files(&mut self, tool: &str) {
        let directory = self.make_directory();
        if std::fs::create_dir_all(&directory).is_err() {
            self.message = "that folder could not be created".into();
            return;
        }
        self.tool_call(tool, json!({"path": directory}));
    }
    fn board_body(&mut self, ui: &mut egui::Ui) {
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
                        for (name, value) in object_detail_rows(&self.project, id) {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new(name).small().color(self.muted()));
                                ui.label(value);
                            });
                        }
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
                ui.add_space(8.0);
                ui.label(egui::RichText::new("BOARD EDGE").size(10.0).color(self.muted()));
                let corners = self.project.board.outline.as_ref().map_or(0, |o| o.len());
                if corners >= 3 {
                    ui.label(
                        egui::RichText::new(format!(
                            "A free shape with {corners} corners."
                        ))
                        .small()
                        .color(self.muted()),
                    );
                    if ui
                        .button("Back to a plain rectangle")
                        .on_hover_text(
                            "Trade the drawn board edge for the simple rectangle sized by W and H above.",
                        )
                        .clicked()
                    {
                        self.run("board.clear_outline", json!({}));
                    }
                }
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.svg_path)
                            .hint_text("drawing file path (.svg)")
                            .desired_width(150.0),
                    )
                    .on_hover_text(
                        "A vector drawing whose edge can become the board outline, or whose shapes can be laid onto the board as artwork.",
                    );
                });
                if ui
                    .button("Import drawing")
                    .on_hover_text(
                        "Lay the shapes from the drawing file onto the board as editable artwork.",
                    )
                    .clicked()
                {
                    self.run(
                        "svg.import",
                        json!({"path": self.svg_path.clone(), "use_outline": false}),
                    );
                }
                if ui
                    .button("Import as board edge")
                    .on_hover_text(
                        "Trace the closed shape in the drawing file into the board outline.",
                    )
                    .clicked()
                {
                    self.run(
                        "svg.import",
                        json!({"path": self.svg_path.clone(), "use_outline": true}),
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
                ui.add_space(12.0);
                ui.label(egui::RichText::new("MAKE FILES").size(10.0).color(self.muted()));
                ui.label(
                    egui::RichText::new(
                        "Plain files for the board factory, written beside your design unless you name another folder.",
                    )
                    .small()
                    .color(self.muted()),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut self.make_dir)
                        .hint_text("folder for the files (blank means \"outputs\" beside the design)")
                        .desired_width(220.0),
                )
                .on_hover_text(
                    "Where the make files land. Leave blank to use a folder named outputs next to the design file.",
                );
                if ui
                    .button("Copper & drill files")
                    .on_hover_text(
                        "The Gerber copper layers, board outline and drill file a board house asks for.",
                    )
                    .clicked()
                {
                    self.write_make_files("gerber.export");
                }
                if ui
                    .button("Parts list (CSV)")
                    .on_hover_text(
                        "A spreadsheet of every part: one row per unique part with all its references.",
                    )
                    .clicked()
                {
                    self.write_make_files("bom.write");
                }
                if ui
                    .button("Net list")
                    .on_hover_text("A plain text list of every connection and the pins on it.")
                    .clicked()
                {
                    self.write_make_files("netlist.write");
                }
                ui.label(
                    egui::RichText::new("Writing these files changes nothing in the design.")
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
                        if query != self.command_query {
                            self.command_index = 0;
                        }
                        let stepped = ui.input(|i| {
                            i.key_pressed(egui::Key::ArrowDown) as isize
                                - i.key_pressed(egui::Key::ArrowUp) as isize
                        });
                        if stepped != 0 {
                            self.command_index =
                                step_index(self.command_index, matches.len(), stepped);
                            self.command_scroll = true;
                            if !search.has_focus() {
                                search.request_focus();
                            }
                        }
                        if enter
                            && search.lost_focus()
                            && let Some(spec) =
                                matches.get(step_index(self.command_index, matches.len(), 0))
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
                            "{} of {} commands match; click one to load it with a starter parameter set, or step the highlight with Up and Down and press Enter in the search box to load it",
                            matches.len(),
                            pcbcraft_engine::commands().len()
                        ))
                        .small()
                        .color(self.muted()),
                    );
                    egui::ScrollArea::vertical()
                        .max_height(180.0)
                        .show(ui, |ui| {
                            if let Some(spec) = pcbcraft_engine::commands()
                                .into_iter()
                                .find(|spec| spec.id == self.command)
                            {
                                for line in param_help(&spec) {
                                    ui.label(
                                        egui::RichText::new(line)
                                            .small()
                                            .color(self.muted()),
                                    );
                                }
                            }
                            for (row, spec) in matches.iter().enumerate() {
                                let response = ui.selectable_label(
                                    self.command == spec.id
                                        || row == step_index(self.command_index, matches.len(), 0),
                                    format!("{}   {}", spec.id, spec.description),
                                );
                                if row == self.command_index && self.command_scroll {
                                    response.scroll_to_me(Some(egui::Align::Center));
                                }
                                if response.clicked() {
                                    self.command = spec.id.into();
                                    self.params = params_template(spec);
                                    self.command_index = row;
                                }
                            }
                            self.command_scroll = false;
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
    fn copy_plot_to_clipboard(&mut self, ctx: &egui::Context) {
        let view = if matches!(self.tab, Tab::Schematic) {
            "schematic"
        } else {
            "pcb"
        };
        let bytes = self
            .session
            .lock()
            .ok()
            .and_then(|session| pcbcraft_io::preview_png(&session.project, view).ok())
            .and_then(|bytes| {
                let mut reader = std::io::Cursor::new(bytes);
                image::load(&mut reader, image::ImageFormat::Png)
                    .map(|image| image.into_rgba8())
                    .ok()
            });
        match bytes {
            Some(image) => {
                ctx.copy_image(egui::ColorImage::from_rgba_unmultiplied(
                    [image.width() as usize, image.height() as usize],
                    image.as_raw(),
                ));
                self.message = "plot copied to the clipboard".into();
            }
            None => self.message = "copy failed: no plot to copy".into(),
        }
    }

    fn show(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        self.refresh();
        self.style(&ctx);
        self.shortcuts(&ctx);
        self.header(ui);
        self.footer(ui);
        if self.home_open {
            self.start_home(ui);
        } else {
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
        }
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
    fn selected_board_objects_describe_themselves_in_the_properties_panel() -> Result<(), String> {
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let made = |command: &str, params: Value| -> Result<u64, String> {
            shared
                .lock()
                .map_err(|_| "session lock unavailable")?
                .execute(command, params, None)
                .map_err(|e| e.to_string())?["result"]["id"]
                .as_u64()
                .ok_or_else(|| format!("{command} returned no id"))
        };
        let net = made("net.create", json!({"name":"PWR"}))?;
        let track = made(
            "track.add",
            json!({"net":net,"start":{"x":0.0,"y":0.0},"end":{"x":10.0,"y":0.0},"width":0.3}),
        )?;
        let via = made(
            "via.place",
            json!({"net":net,"position":{"x":5.0,"y":5.0},"diameter":0.7,"drill":0.3}),
        )?;
        let zone = made(
            "zone.place",
            json!({"net":net,"layer":"F.Cu","outline":[{"x":0.0,"y":0.0},{"x":10.0,"y":0.0},{"x":10.0,"y":10.0},{"x":0.0,"y":10.0},{"x":0.0,"y":0.0}]}),
        )?;
        let text = made(
            "text.place",
            json!({"text":"REV A","position":{"x":20.0,"y":20.0},"size":1.5,"layer":"F.Cu"}),
        )?;
        let label = made(
            "label.place",
            json!({"kind":"label","net":net,"position":{"x":1.0,"y":1.0}}),
        )?;
        let project = shared
            .lock()
            .map_err(|_| "session lock unavailable")?
            .project
            .clone();
        let find = |id: u64, name: &str| {
            object_detail_rows(&project, id)
                .into_iter()
                .find(|(row, _)| row == name)
                .map(|(_, value)| value)
        };
        assert_eq!(
            find(track, "Net").as_deref(),
            Some("PWR"),
            "a track names its net"
        );
        assert_eq!(
            find(track, "Width").as_deref(),
            Some("0.300 mm"),
            "a track states its width"
        );
        assert_eq!(
            find(via, "Drill").as_deref(),
            Some("0.30 mm"),
            "a via states its drill"
        );
        assert_eq!(
            find(zone, "Copper").as_deref(),
            Some("poured"),
            "a zone states its pour"
        );
        assert_eq!(
            find(zone, "Outline").as_deref(),
            Some("5 vertices"),
            "a zone counts its outline"
        );
        assert_eq!(
            find(text, "Text").as_deref(),
            Some("REV A"),
            "text shows its string"
        );
        assert_eq!(
            find(label, "Kind").as_deref(),
            Some("label"),
            "a schematic label states its kind"
        );
        assert!(
            object_detail_rows(&project, 999).is_empty(),
            "an unknown id stays quiet"
        );
        Ok(())
    }

    #[test]
    fn a_folder_added_from_anywhere_joins_the_list_and_removal_forgets_it_again()
    -> Result<(), String> {
        let _turn = pcbcraft_engine::library::SOURCES_TEST_LOCK
            .lock()
            .map_err(|_| "source test lock unavailable")?;
        let file =
            std::env::temp_dir().join(format!("pcbcraft-source-e2e-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&file);
        {
            let mut slot = pcbcraft_engine::library::SOURCES_PATH_TEST
                .lock()
                .map_err(|_| "source path slot unavailable")?;
            *slot = Some(file.clone());
        }
        let outcome = source_round_trip(&file);
        {
            let mut slot = pcbcraft_engine::library::SOURCES_PATH_TEST
                .lock()
                .map_err(|_| "source path slot unavailable")?;
            *slot = None;
        }
        let _ = std::fs::remove_file(&file);
        outcome
    }
    /// The visible loop a newcomer follows: type a folder, press Add folder,
    /// watch its collections appear, select one, press Remove source, and
    /// watch both the list and the remembered config forget it.
    fn source_round_trip(file: &std::path::Path) -> Result<(), String> {
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(shared, Preferences::default(), None, None);
        app.change_tab(Tab::Libraries);
        app.refresh();
        let ctx = egui::Context::default();
        for _ in 0..80 {
            frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
            if app.library.catalog.is_some() && app.library.sources.is_some() {
                break;
            }
        }
        assert!(
            app.library.catalog.is_some(),
            "the catalog arrives on its own"
        );
        let shelf = std::fs::canonicalize(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../library/tests/fixtures"),
        )
        .map_err(|e| format!("fixture shelf missing: {e}"))?
        .to_string_lossy()
        .into_owned();
        app.library.new_source = shelf.clone();
        authoring::tests::tap(&ctx, &mut app, "Add folder")?;
        let mut added = false;
        for _ in 0..80 {
            frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
            if app.library.notice.contains("Added") {
                added = true;
                break;
            }
        }
        assert!(
            added,
            "adding the folder reports back: {}",
            app.library.notice
        );
        let saved =
            std::fs::read_to_string(file).map_err(|e| format!("config not written: {e}"))?;
        assert!(
            saved.contains(&shelf),
            "the folder is remembered in the config file"
        );
        let mut listed = false;
        for _ in 0..80 {
            frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
            listed = app
                .library
                .catalog
                .as_ref()
                .and_then(|catalog| catalog.get("symbol_libraries"))
                .and_then(Value::as_array)
                .is_some_and(|libs| {
                    libs.iter().any(|lib| {
                        lib.get("path")
                            .and_then(Value::as_str)
                            .is_some_and(|path| path.starts_with(&shelf))
                    })
                });
            if listed {
                break;
            }
        }
        assert!(
            listed,
            "the added folder's collections join the unified list"
        );
        let added_path = app
            .library
            .catalog
            .as_ref()
            .and_then(|catalog| catalog.get("symbol_libraries"))
            .and_then(Value::as_array)
            .and_then(|libs| {
                libs.iter().find_map(|lib| {
                    lib.get("path")
                        .and_then(Value::as_str)
                        .filter(|path| path.starts_with(&shelf))
                        .map(str::to_owned)
                })
            })
            .ok_or("the added collection vanished from the list")?;
        app.library.path = added_path;
        authoring::tests::tap(&ctx, &mut app, "Remove source")?;
        let mut removed = false;
        for _ in 0..80 {
            frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
            if app.library.notice.contains("Removed") {
                removed = true;
                break;
            }
        }
        assert!(removed, "removal reports back: {}", app.library.notice);
        let saved = std::fs::read_to_string(file).map_err(|_| "config vanished")?;
        assert!(
            !saved.contains(&shelf),
            "the forgotten folder is out of the config file"
        );
        Ok(())
    }

    #[test]
    fn the_file_menu_is_where_you_open_save_and_start_a_design() -> Result<(), String> {
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(shared, Preferences::default(), None, None);
        app.refresh();
        let file = std::env::temp_dir()
            .join(format!(
                "pcbcraft-menu-save-{}.pcbcraft",
                std::process::id()
            ))
            .to_string_lossy()
            .into_owned();
        app.path = file.clone();
        let ctx = egui::Context::default();
        for _ in 0..4 {
            frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        }
        assert!(
            !paints_text(&ctx, &mut app, "Project file")?,
            "the old bottom file strip is gone; file jobs live in the File menu"
        );
        app.tool_call("sample_design", json!({}));
        authoring::tests::tap(&ctx, &mut app, "File")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert!(
            !paints_text(&ctx, &mut app, "Load LED example")?,
            "the example lives on the start screen, not in the menu"
        );
        authoring::tests::tap(&ctx, &mut app, "Save")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert!(
            app.message.starts_with("save"),
            "saving answers: {}",
            app.message
        );
        assert!(
            std::fs::metadata(&file).is_ok_and(|m| m.len() > 0),
            "the design lands in the file the menu names"
        );
        authoring::tests::tap(&ctx, &mut app, "File")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        authoring::tests::tap(&ctx, &mut app, "New")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert!(
            app.project.components.is_empty(),
            "New empties the design: {} parts left",
            app.project.components.len()
        );
        authoring::tests::tap(&ctx, &mut app, "Edit")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        authoring::tests::tap(&ctx, &mut app, "Undo")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert!(
            app.message.starts_with("undo"),
            "undo answers: {}",
            app.message
        );
        assert!(
            !app.project.components.is_empty(),
            "undo brings the design back"
        );
        let _ = std::fs::remove_file(&file);
        Ok(())
    }
    /// True when the named words appear anywhere on screen this frame or the
    /// next two, so tests can assert what a person actually sees.
    fn paints_text(ctx: &egui::Context, app: &mut App, needle: &str) -> Result<bool, String> {
        for _ in 0..3 {
            let output = frame(ctx, app, vec![], egui::Modifiers::NONE);
            if output.shapes.iter().any(|shape| {
                if let egui::Shape::Text(text) = &shape.shape {
                    return text.galley.text().contains(needle);
                }
                false
            }) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    #[test]
    fn a_drawn_shape_from_a_file_becomes_the_board_edge_and_back_again() -> Result<(), String> {
        let file =
            std::env::temp_dir().join(format!("pcbcraft-board-edge-{}.svg", std::process::id()));
        std::fs::write(
            &file,
            r#"<svg width="30mm" height="20mm" stroke="none"><polygon points="1,1 29,1 29,19 15,19 1,12" stroke="edge"/><line x1="2mm" y1="2mm" x2="28mm" y2="2mm" stroke="artwork"/></svg>"#,
        )
        .map_err(|e| e.to_string())?;
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(shared, Preferences::default(), None, None);
        app.refresh();
        let ctx = egui::Context::default();
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert!(
            paints_text(&ctx, &mut app, "BOARD EDGE")?,
            "the board's edge lives in its own plain-language section"
        );
        app.svg_path = file.to_string_lossy().into_owned();
        authoring::tests::tap(&ctx, &mut app, "Import as board edge")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        let corners = app.project.board.outline.as_ref().map_or(0, |o| o.len());
        assert!(
            corners >= 3,
            "the drawn shape became the board edge: {corners} corners"
        );
        assert!(
            paints_text(&ctx, &mut app, "Back to a plain rectangle")?,
            "the shape explains itself and offers a way back"
        );
        authoring::tests::tap(&ctx, &mut app, "Back to a plain rectangle")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert!(
            app.project.board.outline.is_none_or(|o| o.len() < 3),
            "the board is a plain rectangle again"
        );
        let _ = std::fs::remove_file(&file);
        Ok(())
    }

    #[test]
    fn switching_a_layer_off_hides_it_on_the_sheet_without_touching_the_design()
    -> Result<(), String> {
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(shared, Preferences::default(), None, None);
        app.refresh();
        app.tool_call("sample_design", json!({}));
        let revision = {
            let session = app.session.lock().map_err(|_| "session unavailable")?;
            session.revision
        };
        let wire_count = |app: &mut App, ctx: &egui::Context| -> Result<usize, String> {
            let output = frame(ctx, app, vec![], egui::Modifiers::NONE);
            Ok(output
                .shapes
                .iter()
                .filter(|shape| {
                    matches!(
                        &shape.shape,
                        egui::Shape::Path(path) if (path.stroke.width - 1.8).abs() < 0.01
                    )
                })
                .count())
        };
        let ctx = egui::Context::default();
        let before = wire_count(&mut app, &ctx)?;
        assert!(before > 0, "the worked example is wired up");
        authoring::tests::tap(&ctx, &mut app, "Layers")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert!(
            paints_text(&ctx, &mut app, "Wires")?,
            "the layers page lists wires as its own row"
        );
        authoring::tests::tap(&ctx, &mut app, "Wires")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert!(
            app.preferences.hidden_layers.iter().any(|h| h == "Wires"),
            "unticking the row hides the wires layer"
        );
        let after = wire_count(&mut app, &ctx)?;
        assert!(
            after < before,
            "the wires vanish from the sheet: {before} strokes became {after}"
        );
        let now = {
            let session = app.session.lock().map_err(|_| "session unavailable")?;
            session.revision
        };
        assert_eq!(
            now, revision,
            "hiding a layer is only a view change; the design itself is untouched"
        );
        Ok(())
    }

    #[test]
    fn a_newcomer_searches_for_a_part_and_it_is_ready_to_place() -> Result<(), String> {
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(shared, Preferences::default(), None, None);
        app.refresh();
        app.tool_call("sample_design", json!({}));
        let ctx = egui::Context::default();
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        authoring::tests::tap(&ctx, &mut app, "Add part")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert!(
            paints_text(&ctx, &mut app, "Search parts by name")?,
            "the part search opens with a box that says what to type"
        );
        authoring::tests::tap(&ctx, &mut app, "Search parts by name")?;
        for letter in "resistor".chars() {
            frame(
                &ctx,
                &mut app,
                vec![egui::Event::Text(letter.into())],
                egui::Modifiers::NONE,
            );
        }
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert_eq!(
            app.part_search, "resistor",
            "typing in the search box arrives letter by letter"
        );
        assert!(
            paints_text(&ctx, &mut app, "Resistor")?,
            "the matching part stays in the list"
        );
        assert!(
            !paints_text(&ctx, &mut app, "Capacitor")?,
            "parts that do not match the search disappear from the list"
        );
        authoring::tests::tap(&ctx, &mut app, "Resistor")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert!(
            matches!(&app.tool, Tool::Place(kind) if kind == "resistor"),
            "picking the part arms the schematic to place it, got {:?}",
            app.tool
        );
        assert_eq!(
            app.tab,
            Tab::Schematic,
            "the sheet comes forward to receive it"
        );
        Ok(())
    }

    #[test]
    fn selection_grows_handles_and_the_old_button_bar_is_gone() -> Result<(), String> {
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(shared, Preferences::default(), None, None);
        app.refresh();
        app.tool_call("sample_design", json!({}));
        let ctx = egui::Context::default();
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert!(
            !paints_text(&ctx, &mut app, "Select  Esc")?,
            "the wall of tool buttons is gone from the top"
        );
        assert!(
            !paints_text(&ctx, &mut app, "Sync to PCB >")?,
            "sync moved into the Edit menu"
        );
        assert!(
            paints_text(&ctx, &mut app, "Fit")?,
            "a floating Fit control sits in the work area"
        );
        let first = app.project.components[0].id;
        let before = app.project.components[0].rotation;
        app.select(Some(first));
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert!(
            paints_text(&ctx, &mut app, "Rotate")?,
            "a selected part grows a Rotate handle beside it"
        );
        authoring::tests::tap(&ctx, &mut app, "Rotate")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        let after = app
            .project
            .component(first)
            .map(|c| c.rotation)
            .unwrap_or(before);
        assert!(
            (after - before).abs() > 1e-6,
            "the handle turns the part: {before} -> {after}"
        );
        Ok(())
    }

    #[test]
    fn the_nets_tab_adds_names_and_shines_a_spotlight() -> Result<(), String> {
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(shared, Preferences::default(), None, None);
        app.refresh();
        app.tool_call("sample_design", json!({}));
        let before = app.project.nets.len();
        let ctx = egui::Context::default();
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        authoring::tests::tap(&ctx, &mut app, "Nets")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert!(
            paints_text(&ctx, &mut app, "New net")?,
            "the Nets tab offers a way to add a connection"
        );
        authoring::tests::tap(&ctx, &mut app, "New net")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert_eq!(
            app.project.nets.len(),
            before + 1,
            "one click adds one named connection"
        );
        let added = app.project.nets.len() - 1;
        let fresh = app.project.nets[added].name.clone();
        authoring::tests::tap(&ctx, &mut app, &fresh)?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert_eq!(
            app.highlight_net,
            Some(app.project.nets[added].id),
            "clicking a net lights it up on the board"
        );
        let fresh_id = app.project.nets[added].id;
        app.net_draft = Some((fresh_id, "Flashlight".into()));
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        authoring::tests::tap(&ctx, &mut app, "Use name")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert!(
            app.project
                .nets
                .iter()
                .any(|n| n.id == fresh_id && n.name == "Flashlight"),
            "the rename sticks: {}",
            app.message
        );
        Ok(())
    }

    #[test]
    fn a_newcomer_puts_a_pin_on_a_connection_and_takes_it_back_off() -> Result<(), String> {
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(shared, Preferences::default(), None, None);
        app.refresh();
        app.tool_call(
            "component.add",
            json!({"kind":"resistor","reference":"R1","value":"330","position":{"x":40.0,"y":14.0}}),
        );
        let ctx = egui::Context::default();
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        authoring::tests::tap(&ctx, &mut app, "Nets")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        authoring::tests::tap(&ctx, &mut app, "New net")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert_eq!(app.project.nets.len(), 1, "one click adds one connection");
        assert!(
            paints_text(&ctx, &mut app, "No pins on this connection yet.")?,
            "a fresh connection says so plainly: {}",
            app.message
        );
        let net_id = app.project.nets[0].id;
        let on_net = |app: &App| -> usize {
            app.project
                .components
                .iter()
                .flat_map(|c| c.pins.iter())
                .filter(|p| p.net == Some(net_id))
                .count()
        };
        assert_eq!(on_net(&app), 0, "the resistor starts unconnected");
        authoring::tests::tap(&ctx, &mut app, "choose a spare pin")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        authoring::tests::tap(&ctx, &mut app, "R1 pin 1")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        authoring::tests::tap(&ctx, &mut app, "Connect")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert_eq!(
            on_net(&app),
            1,
            "the chosen pin joins the connection: {}",
            app.message
        );
        assert!(
            paints_text(&ctx, &mut app, "R1 pin 1")?,
            "the pin is listed on the connection"
        );
        authoring::tests::tap(&ctx, &mut app, "\u{d7}")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert_eq!(
            on_net(&app),
            0,
            "one click takes the pin back off: {}",
            app.message
        );
        Ok(())
    }

    #[test]
    fn the_make_files_section_writes_the_factory_folder() -> Result<(), String> {
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(shared, Preferences::default(), None, None);
        app.refresh();
        app.tool_call("sample_design", json!({}));
        let directory = std::env::temp_dir().join(format!("pcbcraft-make-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        app.make_dir = directory.to_string_lossy().into_owned();
        let revision = app.revision;
        let ctx = egui::Context::default();
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert!(
            paints_text(&ctx, &mut app, "MAKE FILES")?,
            "the inspector should offer the factory files"
        );
        authoring::tests::tap(&ctx, &mut app, "Copper & drill files")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        let listed = |directory: &std::path::Path| -> Result<Vec<String>, String> {
            std::fs::read_dir(directory)
                .map_err(|e| format!("the folder should exist: {e}"))?
                .filter_map(|entry| entry.ok())
                .map(|entry| Ok(entry.file_name().to_string_lossy().into_owned()))
                .collect()
        };
        let names = listed(&directory)?;
        assert!(
            !names.is_empty(),
            "copper and drill files should land in the folder: {}",
            app.message
        );
        authoring::tests::tap(&ctx, &mut app, "Parts list (CSV)")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        authoring::tests::tap(&ctx, &mut app, "Net list")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        let names = listed(&directory)?;
        assert!(
            names.iter().any(|n| n.ends_with(".csv")) && names.iter().any(|n| n.ends_with(".net")),
            "the parts list and net list should join the folder: {names:?}"
        );
        assert_eq!(
            app.revision, revision,
            "writing files leaves the design itself untouched"
        );
        let _ = std::fs::remove_dir_all(&directory);
        Ok(())
    }

    #[test]
    fn a_newcomer_clears_the_wiring_on_one_connection_then_the_whole_board() -> Result<(), String> {
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(shared, Preferences::default(), None, None);
        app.refresh();
        app.tool_call("sample_design", json!({}));
        app.tool_call(
            "track.route",
            json!({"from":{"footprint":10,"pad":"1"},"to":{"footprint":11,"pad":"1"},"layer":"F.Cu"}),
        );
        let routed = app.project.board.tracks.len();
        assert!(
            routed > 0,
            "routing a pad to pad should draw copper: {}",
            app.message
        );
        let vcc = app
            .project
            .nets
            .iter()
            .find(|n| n.name == "VCC")
            .map(|n| n.id)
            .ok_or("the sample should have a VCC connection")?;
        app.highlight_net = Some(vcc);
        let ctx = egui::Context::default();
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        authoring::tests::tap(&ctx, &mut app, "Edit")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert!(
            paints_text(&ctx, &mut app, "Clear wiring on VCC")?,
            "the lit connection gets its own clear item"
        );
        authoring::tests::tap(&ctx, &mut app, "Clear wiring on VCC")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert!(
            app.project.board.tracks.len() < routed,
            "the connection's copper lifts: {}",
            app.message
        );
        app.tool_call(
            "track.route",
            json!({"from":{"footprint":10,"pad":"1"},"to":{"footprint":11,"pad":"1"},"layer":"F.Cu"}),
        );
        let routed = app.project.board.tracks.len();
        assert!(
            routed > 0,
            "the copper should be back for the whole-board clear"
        );
        authoring::tests::tap(&ctx, &mut app, "Edit")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        authoring::tests::tap(&ctx, &mut app, "Clear all wiring")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert_eq!(
            app.project.board.tracks.len(),
            0,
            "the board starts over clean: {}",
            app.message
        );
        assert_eq!(app.project.nets.len(), 3, "the connections themselves stay");
        Ok(())
    }

    #[test]
    fn a_newcomer_adds_a_rule_set_and_tunes_its_numbers() -> Result<(), String> {
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(shared, Preferences::default(), None, None);
        app.refresh();
        app.tool_call("sample_design", json!({}));
        let ctx = egui::Context::default();
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        authoring::tests::tap(&ctx, &mut app, "Rules")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert!(
            paints_text(&ctx, &mut app, "No rule sets yet")?,
            "a design without rule sets says so"
        );
        authoring::tests::tap(&ctx, &mut app, "New class")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert_eq!(
            app.project.net_classes.len(),
            1,
            "one click adds one rule set: {}",
            app.message
        );
        assert!(
            paints_text(&ctx, &mut app, "Gap 0.200 mm")?,
            "the new rule set shows its starting numbers"
        );
        let id = app.project.net_classes[0].id;
        authoring::tests::tap(&ctx, &mut app, "Tune")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        app.rule_draft = Some((id, 0.5, 0.6));
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        authoring::tests::tap(&ctx, &mut app, "Apply")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        let class = &app.project.net_classes[0];
        assert!(
            (class.clearance - 0.5).abs() < 1e-9 && (class.width - 0.6).abs() < 1e-9,
            "the tuned numbers stick: {} {} · {}",
            class.clearance,
            class.width,
            app.message
        );
        authoring::tests::tap(&ctx, &mut app, "Remove")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert!(
            app.project.net_classes.is_empty(),
            "the rule set is removed: {}",
            app.message
        );
        Ok(())
    }

    #[test]
    fn the_start_screen_is_the_first_thing_a_newcomer_sees() -> Result<(), String> {
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(shared, Preferences::default(), None, None);
        app.refresh();
        app.home_open = true;
        let ctx = egui::Context::default();
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert!(
            paints_text(&ctx, &mut app, "New design")?,
            "the start screen offers a blank page"
        );
        assert!(
            paints_text(&ctx, &mut app, "Worked example")?,
            "the start screen offers something to explore"
        );
        authoring::tests::tap(&ctx, &mut app, "Worked example")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert!(
            !app.home_open,
            "picking the example leaves the start screen"
        );
        assert!(
            !app.project.components.is_empty(),
            "the example fills the design from one click"
        );
        Ok(())
    }

    #[test]
    fn the_start_screen_lists_designs_you_can_open_again() -> Result<(), String> {
        let _turn = pcbcraft_engine::recents::RECENTS_TEST_LOCK
            .lock()
            .map_err(|_| "test lock unavailable")?;
        let directory =
            std::env::temp_dir().join(format!("pcbcraft-home-recents-{}", std::process::id()));
        std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
        *pcbcraft_engine::recents::RECENTS_PATH_TEST
            .lock()
            .map_err(|_| "test lock unavailable")? = Some(directory.join("recent-designs.json"));
        pcbcraft_engine::recents::clear_recent_paths();
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(shared, Preferences::default(), None, None);
        app.refresh();
        let file = directory
            .join("flashlight.pcbcraft")
            .to_string_lossy()
            .into_owned();
        app.path = file.clone();
        app.tool_call("sample_design", json!({}));
        app.tool_call("save", json!({"path": file}));
        app.home_open = true;
        let ctx = egui::Context::default();
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert!(
            paints_text(&ctx, &mut app, "flashlight.pcbcraft")?,
            "a design you saved is listed for one-click return"
        );
        let want = app.path.clone();
        authoring::tests::tap(&ctx, &mut app, &want)?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert!(
            !app.home_open,
            "clicking a recent design opens it and leaves the start screen"
        );
        pcbcraft_engine::recents::clear_recent_paths();
        *pcbcraft_engine::recents::RECENTS_PATH_TEST
            .lock()
            .map_err(|_| "test lock unavailable")? = None;
        let _ = std::fs::remove_dir_all(&directory);
        Ok(())
    }

    #[test]
    fn the_library_split_pages_now_open_one_unified_page() -> Result<(), String> {
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(shared, Preferences::default(), None, None);
        app.refresh();
        app.change_tab(Tab::Libraries);
        app.library.editor.mode = 1; // an old split-view selection
        let ctx = egui::Context::default();
        let output = frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert_eq!(
            app.library.editor.mode, 0,
            "the separate project page is gone; one library shows everything"
        );
        assert_eq!(output.viewport_output.len(), 1, "the unified page paints");
        Ok(())
    }

    #[test]
    fn the_library_browser_groups_collections_by_the_source_they_came_from() {
        let catalog = json!({"symbol_libraries":[
            {"name":"Device","path":"/installed/Device.kicad_sym","source":"Installed collections"},
            {"name":"DigiKeyParts","path":"/home/me/digikey/DigiKeyParts.kicad_sym","source":"Digi-Key shelf"},
            {"name":"R","path":"/installed/R.kicad_sym"}
        ]});
        let items = catalog["symbol_libraries"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        assert_eq!(
            library::entry_source(&items[2]),
            "Installed collections",
            "an entry without a source tag still answers with one"
        );
        assert_eq!(
            library::distinct_sources(&items),
            vec!["Digi-Key shelf", "Installed collections"],
            "every source is named once, in list order"
        );
    }

    #[test]
    fn the_libraries_tab_paints_the_unified_list_from_added_sources() -> Result<(), String> {
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(shared, Preferences::default(), None, None);
        app.refresh();
        app.change_tab(Tab::Libraries);
        app.library.catalog = Some(json!({"symbol_libraries":[
            {"name":"Device","path":"/installed/Device.kicad_sym","source":"Installed collections"},
            {"name":"DigiKeyParts","path":"/home/me/digikey/DigiKeyParts.kicad_sym","source":"Digi-Key shelf"}
        ],"footprint_libraries":[],"scan_truncated":false}));
        app.library.sources = Some(json!({"sources":[
            {"name":"Digi-Key shelf","path":"/home/me/digikey","kind":"added","symbol_libraries":1,"footprint_libraries":0},
            {"name":"Installed collections","path":"/usr/share/design-collections","kind":"built-in","symbol_libraries":1,"footprint_libraries":0}
        ]}));
        app.library.results = Some(json!({"total":2,"entries":[
            {"name":"Device","path":"/installed/Device.kicad_sym","kind":"symbol"},
            {"name":"DigiKeyParts","path":"/home/me/digikey/DigiKeyParts.kicad_sym","kind":"symbol"}
        ]}));
        let ctx = egui::Context::default();
        let output = frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert_eq!(
            output.viewport_output.len(),
            1,
            "the unified library view paints"
        );
        Ok(())
    }

    #[test]
    fn library_filter_narrows_listed_entries_by_name_or_description() {
        let led = json!({"name":"LED","description":"light emitting diode"});
        let resistor = json!({"name":"R","description":"resistor"});
        assert!(
            library::entry_matches(&led, ""),
            "an empty filter shows everything"
        );
        assert!(
            library::entry_matches(&led, "lig"),
            "descriptions match too"
        );
        assert!(
            library::entry_matches(&resistor, "RESIST"),
            "the filter ignores case"
        );
        assert!(!library::entry_matches(&resistor, "led"));
        assert!(!library::entry_matches(&led, "9"));
    }
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
    fn no_control_asks_the_user_to_know_a_tool_brand() {
        let brand = ["Ki", "Cad"].concat();
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut offenders = Vec::new();
        for file in ["lib.rs", "canvas.rs", "library.rs", "authoring.rs"] {
            let text = std::fs::read_to_string(dir.join(file)).unwrap_or_default();
            for line in text.lines() {
                let code = line.split("//").next().unwrap_or("");
                if !code.contains('"') || !code.contains(&brand) {
                    continue;
                }
                let allowed = code.contains("credits the")
                    || code.contains("-derived data")
                    || code.contains("LICENSE-")
                    || code.contains(".contains(&brand)")
                    || code.contains("code.contains");
                if !allowed {
                    offenders.push(format!("{file}: {}", code.trim()));
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "buttons and labels must speak plainly, not name other tools: {offenders:?}"
        );
    }

    #[test]
    fn designs_opened_on_this_computer_are_remembered_for_the_start_screen() -> Result<(), String> {
        let _turn = pcbcraft_engine::recents::RECENTS_TEST_LOCK
            .lock()
            .map_err(|_| "test lock unavailable")?;
        let directory =
            std::env::temp_dir().join(format!("pcbcraft-ui-recents-{}", std::process::id()));
        std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
        *pcbcraft_engine::recents::RECENTS_PATH_TEST
            .lock()
            .map_err(|_| "test lock unavailable")? = Some(directory.join("recent-designs.json"));
        pcbcraft_engine::recents::clear_recent_paths();
        pcbcraft_engine::recents::record_recent_path("/designs/flashlight.board.json");
        pcbcraft_engine::recents::record_recent_path("/designs/radio.board.json");
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(shared, Preferences::default(), None, None);
        app.refresh();
        assert_eq!(
            app.recent_designs,
            vec![
                "/designs/radio.board.json",
                "/designs/flashlight.board.json"
            ],
            "the newest design leads the list"
        );
        let ctx = egui::Context::default();
        let output = frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert_eq!(
            output.viewport_output.len(),
            1,
            "the app keeps the recent list ready for the Start screen"
        );
        pcbcraft_engine::recents::clear_recent_paths();
        *pcbcraft_engine::recents::RECENTS_PATH_TEST
            .lock()
            .map_err(|_| "test lock unavailable")? = None;
        let _ = std::fs::remove_dir_all(&directory);
        Ok(())
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
                [
                    "anywhere",
                    "schematic tab",
                    "pcb tab",
                    "canvas",
                    "command palette",
                ]
                .contains(context),
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
    fn palette_highlight_walks_the_list_as_a_ring() {
        assert_eq!(step_index(0, 5, 1), 1);
        assert_eq!(
            step_index(4, 5, 1),
            0,
            "walking past the last row wraps to the first"
        );
        assert_eq!(
            step_index(0, 5, -1),
            4,
            "walking above the first row wraps to the last"
        );
        assert_eq!(step_index(9, 3, 0), 2, "a stale index is pulled back");
        assert_eq!(step_index(0, 0, 1), 0, "an empty list stays put");
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
    fn palette_help_explains_every_required_parameter() -> Result<(), String> {
        let spec = pcbcraft_engine::commands()
            .into_iter()
            .find(|spec| spec.id == "padstack.define")
            .ok_or_else(|| "padstack.define must be a command".to_string())?;
        let help = param_help(&spec);
        assert_eq!(
            help.len(),
            spec.input_schema
                .get("required")
                .and_then(Value::as_array)
                .map_or(0, |list| list.len()),
            "one help line per required parameter"
        );
        let joined = help.join("\n");
        assert!(joined.contains("name (text)"), "name is text: {joined}");
        assert!(
            joined.contains("circle or rect or oval or roundrect"),
            "the shape options are listed: {joined}"
        );
        assert!(
            joined.contains("number 0.05 to 100"),
            "the copper bounds are stated before the first refusal: {joined}"
        );
        for spec in pcbcraft_engine::commands() {
            assert_eq!(
                param_help(&spec).len(),
                spec.input_schema
                    .get("required")
                    .and_then(Value::as_array)
                    .map_or(0, |list| list.len()),
                "{} must explain exactly its required parameters",
                spec.id
            );
        }
        Ok(())
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
    fn copy_plot_renders_the_current_view_for_the_clipboard() -> Result<(), String> {
        let session =
            pcbcraft_mcp::shared(pcbcraft_engine::Session::sample().map_err(|e| e.to_string())?);
        let mut app = App::create(session, Preferences::default(), None, None);
        app.refresh();
        let ctx = egui::Context::default();
        app.copy_plot_to_clipboard(&ctx);
        assert_eq!(app.message, "plot copied to the clipboard");
        app.tab = Tab::Schematic;
        app.copy_plot_to_clipboard(&ctx);
        assert_eq!(app.message, "plot copied to the clipboard");
        Ok(())
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
