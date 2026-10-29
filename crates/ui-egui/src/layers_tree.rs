//! The unified Layers tree of the left aside: one tree for the board, its
//! layers, parts and nets (Layout) or its sheets, parts and nets (Schematic).
//!
//! Context: `lib.rs` hosts the left aside's Layers and Library tabs, builds an
//! [`Env`] from the app each frame, calls [`show`] and applies the returned
//! [`Action`]s. The tree never edits the document and never reads `App`.
//! Look: the approved mockups in `app-design/` (design-system.md section 5.7):
//! 28-point rows, 16-point indent, colour swatch, name, Active badge, count,
//! lock cell and eye cell in a right-aligned column.
//! Local rules: rows are built lazily (only expanded branches), cached by a
//! cheap signature, and drawn virtualized, so a 2,000-part board stays
//! interactive. Visibility, lock and focus are view state, not document state.
//! Every glyph is drawn by [`glyph`]; swapping in the shared icon set is a
//! change to that one function.

use crate::a11y::{Named, name_control};
use crate::palette::ThemeColours;
use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Shape, Stroke, Ui, Vec2, epaint};
use pcbcraft_doc::{Id, Layer, Project};
use pcbcraft_text::t;
use std::collections::BTreeSet;

/// Height of one tree row in points; also the smallest target size.
pub(crate) const ROW_HEIGHT: f32 = 28.0;
/// Horizontal step of one nesting level in points.
const INDENT: f32 = 16.0;
/// Width of the eye and lock cells.
const CELL: f32 = 24.0;
/// Side padding of the aside content.
const PAD: f32 = 8.0;

/// Which tab's tree to build.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum TreeTab {
    /// Board: copper, silkscreen, mask and paste, board, user layers, parts, nets.
    Layout,
    /// Schematic: sheets, parts, nets.
    Schematic,
}

/// What one row stands for; picks its glyph.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum RowKind {
    Section,
    Layer,
    Group,
    Leaf,
    Part,
    Pad,
    Net,
    Track,
    Via,
    Zone,
    Text,
    Hole,
    Label,
    Wire,
    Shape,
}

/// How a row's eye reaches the drawing.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) enum Visibility {
    /// A named drawing toggle of the canvas (`hidden_layers` entry).
    Canvas(&'static str),
    /// A machine layer the designer drew; the document stores its flag.
    Art(Id),
    /// The canvas draws nothing for this row yet; the eye is inert.
    None,
}

/// Eye, lock and focus controls of a layer or group row.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct LayerControls {
    /// The lock and focus key; empty when the row has neither.
    pub key: String,
    pub visibility: Visibility,
    /// True when the canvas can dim everything not on this row (`focus_layer`).
    pub focusable: bool,
}

/// One visible line of the tree.
#[derive(Clone, PartialEq, Debug)]
pub(crate) struct Row {
    /// Path key, unique in the tree, made of `/`-joined segments.
    pub key: String,
    pub depth: u8,
    pub kind: RowKind,
    pub label: String,
    /// Muted text after the label: a value, a net or a size.
    pub sub: String,
    /// Right-aligned count text.
    pub count: String,
    /// The label is drawn in the monospace face (references, net names).
    pub mono: bool,
    /// Drawn at reduced strength: empty or inert.
    pub dim: bool,
    pub expandable: bool,
    /// The document object the canvas selects for this row.
    pub select: Option<Id>,
    /// Whether the row lights up when `select` is the canvas selection.
    pub marks: bool,
    pub controls: Option<LayerControls>,
    pub swatch: Option<Color32>,
    /// Round swatch (a net colour) instead of a square one.
    pub round: bool,
    /// The net this row stands for, which the canvas highlights.
    pub net: Option<Id>,
}

/// What the tree asks the app to do; the app applies each in `lib.rs`.
#[derive(Clone, PartialEq, Debug)]
pub(crate) enum Action {
    /// Select a document object on the canvas and in Properties.
    Select(Id),
    /// Select a row that is not one object (the board outline, a layer, a net).
    SelectKey(String),
    /// Set a canvas drawing toggle shown (true) or hidden.
    SetShown { name: &'static str, on: bool },
    /// Hide exactly these canvas toggles of this tab's layers, show the rest.
    SetHidden(Vec<&'static str>),
    /// Set a machine layer's stored visibility.
    SetArtShown { layer: Id, on: bool },
    /// Focus (dim the rest) on this key, or clear with None.
    Focus(Option<String>),
    /// Highlight a net on the canvas, or clear with None.
    HighlightNet(Option<Id>),
    /// Add a machine layer.
    NewArtLayer,
}

/// The read-only inputs of one frame.
pub(crate) struct Env<'a> {
    pub project: &'a Project,
    pub tab: TreeTab,
    pub colours: ThemeColours,
    /// The app's `hidden_layers` list.
    pub hidden: &'a [String],
    pub focus: Option<&'a str>,
    pub selected: Option<Id>,
    /// The copper side new tracks go on; its row carries the Active badge.
    pub active: Layer,
    /// Bumps on every document change; part of the row cache key.
    pub revision: u64,
}

/// The canvas toggles each tab's layer rows may switch.
fn layer_names(tab: TreeTab) -> &'static [&'static str] {
    match tab {
        TreeTab::Layout => &[
            "Top copper",
            "Bottom copper",
            "Package artwork",
            "Cutouts",
            "Pads",
            "Vias",
            "Copper zones",
        ],
        TreeTab::Schematic => &["Wires", "Symbols", "Net labels", "Graphics"],
    }
}

/// View state owned by the app: what is open, filtered, locked and focused.
///
/// # Rules
/// - Never part of the document or the saved preferences.
///
/// # Do not
/// - Store document ids that outlive a frame; rows are rebuilt from the project.
#[derive(Clone, Debug)]
pub(crate) struct TreeView {
    pub expanded: BTreeSet<String>,
    pub query: String,
    /// Layer keys the canvas may not select from or edit.
    pub locked: BTreeSet<String>,
    /// Keyboard cursor row key.
    pub cursor: Option<String>,
    /// A row selected that is not one object: the board outline, a layer, a net.
    pub selected_key: Option<String>,
    last_selected: Option<Id>,
    scroll_to: Option<String>,
    focus_pending: Option<String>,
    has_focus: bool,
    offset: f32,
    viewport: f32,
    cache: Option<(u64, Vec<Row>)>,
}

impl TreeView {
    /// Put the keyboard cursor and focus on the row with this key, as if it
    /// had been reached with the arrow keys.
    pub(crate) fn focus_row(&mut self, key: &str) {
        self.cursor = Some(key.to_owned());
        self.focus_pending = Some(key.to_owned());
        self.scroll_to = Some(key.to_owned());
    }
}

impl Default for TreeView {
    fn default() -> Self {
        let expanded = [
            "sec:copper",
            "sec:silk",
            "sec:mask",
            "sec:board",
            "sec:user",
            "sec:parts",
            "sec:nets",
            "sec:sheet:0",
            "sec:board/root",
            "sec:copper/layer:copper:front",
            "sec:silk/layer:silk:front",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        Self {
            expanded,
            query: String::new(),
            locked: BTreeSet::new(),
            cursor: None,
            selected_key: None,
            last_selected: None,
            scroll_to: None,
            focus_pending: None,
            has_focus: false,
            offset: 0.0,
            viewport: 400.0,
            cache: None,
        }
    }
}

fn side_label(layer: Layer) -> String {
    if layer == Layer::Front {
        t!("tree.side.top")
    } else {
        t!("tree.side.bottom")
    }
}

fn copper_key(layer: Layer) -> &'static str {
    if layer == Layer::Front {
        "copper:front"
    } else {
        "copper:back"
    }
}

fn side_of(key: &str) -> Layer {
    if key.ends_with("back") {
        Layer::Back
    } else {
        Layer::Front
    }
}

fn segment(path: &str, seg: &str) -> String {
    if path.is_empty() {
        seg.to_owned()
    } else {
        format!("{path}/{seg}")
    }
}

fn row(path: &str, seg: &str, kind: RowKind, label: String) -> Row {
    Row {
        key: segment(path, seg),
        depth: 0,
        kind,
        label,
        sub: String::new(),
        count: String::new(),
        mono: false,
        dim: false,
        expandable: false,
        select: None,
        marks: true,
        controls: None,
        swatch: None,
        round: false,
        net: None,
    }
}

/// A counted row: dimmed and not expandable when the count is zero.
fn counted(mut r: Row, n: usize, expandable: bool) -> Row {
    r.count = n.to_string();
    r.dim = n == 0;
    r.expandable = expandable && n > 0;
    r
}

fn reference_of(project: &Project, footprint: &pcbcraft_doc::Footprint) -> String {
    project
        .component(footprint.component)
        .map_or_else(|| t!("tree.part.unknown"), |c| c.reference.clone())
}

/// A stable colour for a net, from the canvas set.
fn net_colour(colours: &ThemeColours, id: Id) -> Color32 {
    let set = [
        colours.canvas.front,
        colours.canvas.back,
        colours.canvas.wire,
        colours.warn,
        colours.link,
    ];
    set[(id % set.len() as u64) as usize]
}

fn section(key: &str, label: String, n: usize) -> Row {
    let mut r = row("", key, RowKind::Section, label);
    r.expandable = true;
    r.count = n.to_string();
    r
}

#[allow(clippy::too_many_arguments)]
fn layer_row(
    path: &str,
    key: &str,
    label: String,
    swatch: Color32,
    vis: Visibility,
    focusable: bool,
    n: usize,
    expandable: bool,
) -> Row {
    let mut r = counted(row(path, &format!("layer:{key}"), RowKind::Layer, label), n, expandable);
    // A layer with nothing on it still opens when it has fixed children.
    r.expandable = expandable;
    r.dim = n == 0 && vis == Visibility::None;
    r.swatch = Some(swatch);
    r.controls = Some(LayerControls {
        key: key.to_owned(),
        visibility: vis,
        focusable,
    });
    r
}

fn group_row(path: &str, seg: &str, label: String, n: usize, vis: Option<Visibility>) -> Row {
    let mut r = counted(row(path, seg, RowKind::Group, label), n, true);
    r.controls = vis.map(|visibility| LayerControls {
        key: String::new(),
        visibility,
        focusable: false,
    });
    r
}

/// Pads, tracks, zones and vias on one copper side, each with a count.
fn copper_children(env: &Env, path: &str, side: Layer) -> Vec<Row> {
    let board = &env.project.board;
    let front = side == Layer::Front;
    let pads: usize = if front {
        board.footprints.iter().map(|f| f.pads.len()).sum()
    } else {
        0
    };
    let tracks = board.tracks.iter().filter(|t| t.layer == side).count();
    let zones = board.zones.iter().filter(|z| z.layer == side).count();
    let mut out = vec![
        group_row(path, "pads", t!("tree.group.pads"), pads, Some(Visibility::Canvas("Pads"))),
        group_row(path, "tracks", t!("tree.group.tracks"), tracks, None),
        group_row(
            path,
            "zones",
            t!("tree.group.zones"),
            zones,
            Some(Visibility::Canvas("Copper zones")),
        ),
    ];
    out.push(group_row(
        path,
        "vias",
        t!("tree.group.vias"),
        board.vias.len(),
        Some(Visibility::Canvas("Vias")),
    ));
    out
}

/// The top-level sections for the Layout tab.
fn layout_sections(env: &Env) -> Vec<Row> {
    let board = &env.project.board;
    let coppers = board
        .layer_records
        .iter()
        .filter(|l| l.kind == "copper")
        .count()
        .max(2);
    vec![
        section("sec:copper", t!("tree.section.copper"), coppers),
        section("sec:silk", t!("tree.section.silk"), 2),
        section("sec:mask", t!("tree.section.mask"), 4),
        section("sec:board", t!("tree.section.board"), 2),
        section("sec:user", t!("tree.section.user"), board.user_layers.len()),
        section("sec:parts", t!("tree.section.parts"), board.footprints.len()),
        section("sec:nets", t!("tree.section.nets"), env.project.nets.len()),
    ]
}

fn schematic_sections(env: &Env) -> Vec<Row> {
    let project = env.project;
    let mut out = Vec::new();
    let mut sheets: Vec<(Id, String)> = vec![(0, t!("tree.sheet.first"))];
    sheets.extend(project.sheets.iter().map(|s| (s.id, s.name.clone())));
    for (id, name) in sheets {
        let n = if id == 0 { 9 } else { 1 };
        out.push(section(&format!("sec:sheet:{id}"), name, n));
    }
    out.push(section("sec:parts", t!("tree.section.parts"), project.components.len()));
    out.push(section("sec:nets", t!("tree.section.nets"), project.nets.len()));
    out
}

fn schematic_layers(env: &Env, path: &str, sheet: Id) -> Vec<Row> {
    let project = env.project;
    let c = &env.colours;
    let comps = project.components.iter().filter(|c| c.sheet == sheet).count();
    if sheet != 0 {
        return vec![layer_row(
            path,
            "symbols",
            t!("tree.sch.symbols"),
            c.warn,
            Visibility::Canvas("Symbols"),
            false,
            comps,
            true,
        )];
    }
    let power = project.labels.iter().filter(|l| l.kind == "power").count();
    let labels = project.labels.iter().filter(|l| l.kind != "power").count();
    let pins: usize = project.components.iter().map(|c| c.pins.len()).sum();
    let drawing = project.graphics.iter().filter(|g| g.layer == "schematic").count();
    let l = |key: &str, label: String, swatch: Color32, vis: Visibility, n: usize, open: bool| {
        layer_row(path, key, label, swatch, vis, false, n, open)
    };
    vec![
        l("wires", t!("tree.sch.wires"), c.canvas.wire, Visibility::Canvas("Wires"), project.wires.len(), true),
        l("symbols", t!("tree.sch.symbols"), c.warn, Visibility::Canvas("Symbols"), comps, true),
        l("labels", t!("tree.sch.labels"), c.canvas.wire, Visibility::Canvas("Net labels"), labels, true),
        l("pinnums", t!("tree.sch.pin_numbers"), c.link, Visibility::None, pins, false),
        l("power", t!("tree.sch.power_ports"), c.canvas.wire, Visibility::None, power, false),
        l("buses", t!("tree.sch.buses"), c.warn, Visibility::None, 0, false),
        l("notes", t!("tree.sch.notes"), c.muted, Visibility::None, 0, false),
        l("drawing", t!("tree.sch.drawing"), c.link, Visibility::Canvas("Graphics"), drawing, false),
        l("title", t!("tree.sch.title_block"), c.ink, Visibility::None, 1, false),
    ]
}

fn net_rows(env: &Env, path: &str, unit_key: &str) -> Vec<Row> {
    let project = env.project;
    let mut nets: Vec<&pcbcraft_doc::Net> = project.nets.iter().collect();
    nets.sort_by(|a, b| a.name.cmp(&b.name));
    nets.into_iter()
        .map(|n| {
            let count = if env.tab == TreeTab::Layout {
                project
                    .board
                    .footprints
                    .iter()
                    .flat_map(|f| &f.pads)
                    .filter(|p| p.net == Some(n.id))
                    .count()
            } else {
                project
                    .components
                    .iter()
                    .flat_map(|c| &c.pins)
                    .filter(|p| p.net == Some(n.id))
                    .count()
            };
            let mut r = row(path, &format!("net:{}", n.id), RowKind::Net, n.name.clone());
            r.count = if unit_key == "pads" { t!("tree.pads", n = count) } else { t!("tree.pins", n = count) };
            r.swatch = Some(net_colour(&env.colours, n.id));
            r.round = true;
            r.net = Some(n.id);
            r.expandable = true;
            r
        })
        .collect()
}

/// The rows directly under `key` (empty for the virtual root). Only called
/// for expanded rows.
fn children(env: &Env, key: &str) -> Vec<Row> {
    let project = env.project;
    let board = &project.board;
    let c = env.colours;
    let segs: Vec<&str> = key.split('/').collect();
    let mut out = Vec::new();
    let front_pads: usize = board.footprints.iter().map(|f| f.pads.len()).sum();
    match (env.tab, segs.as_slice()) {
        (TreeTab::Layout, [""]) => out = layout_sections(env),
        (TreeTab::Schematic, [""]) => out = schematic_sections(env),
        (TreeTab::Layout, ["sec:copper"]) => {
            let layer = |k: &str, label: String, sw: Color32, name: &'static str, n: usize| {
                layer_row(key, k, label, sw, Visibility::Canvas(name), true, n, true)
            };
            let tracks = |l: Layer| board.tracks.iter().filter(|t| t.layer == l).count();
            let zones = |l: Layer| board.zones.iter().filter(|z| z.layer == l).count();
            out.push(layer(
                "copper:front",
                t!("tree.layer.top_copper"),
                c.canvas.front,
                "Top copper",
                front_pads + tracks(Layer::Front) + zones(Layer::Front),
            ));
            let coppers: Vec<&pcbcraft_doc::LayerRecord> = board
                .layer_records
                .iter()
                .filter(|l| l.kind == "copper")
                .collect();
            if coppers.len() > 2 {
                for (i, record) in coppers[1..coppers.len() - 1].iter().enumerate() {
                    out.push(layer_row(
                        key,
                        &format!("copper:inner:{}", i + 1),
                        record.name.clone(),
                        c.canvas.front.gamma_multiply(0.6),
                        Visibility::None,
                        false,
                        0,
                        false,
                    ));
                }
            }
            out.push(layer(
                "copper:back",
                t!("tree.layer.bottom_copper"),
                c.canvas.back,
                "Bottom copper",
                tracks(Layer::Back) + zones(Layer::Back),
            ));
        }
        (TreeTab::Layout, ["sec:silk"]) => {
            for (k, label, side) in [
                ("silk:front", t!("tree.layer.top_silk"), Layer::Front),
                ("silk:back", t!("tree.layer.bottom_silk"), Layer::Back),
            ] {
                let parts = if side == Layer::Front { board.footprints.len() } else { 0 };
                let texts = board.texts.iter().filter(|x| x.layer == side).count();
                out.push(layer_row(
                    key,
                    k,
                    label,
                    c.ink,
                    Visibility::Canvas("Package artwork"),
                    false,
                    parts * 2 + texts,
                    parts > 0 || texts > 0,
                ));
            }
        }
        (TreeTab::Layout, ["sec:mask"]) => {
            let smd = front_pads;
            for (k, label, sw, n) in [
                ("mask:front", t!("tree.layer.top_mask"), c.canvas.mask, smd),
                ("mask:back", t!("tree.layer.bottom_mask"), c.canvas.mask, 0),
                ("paste:front", t!("tree.layer.top_paste"), c.muted, smd),
                ("paste:back", t!("tree.layer.bottom_paste"), c.muted, 0),
            ] {
                out.push(layer_row(key, k, label, sw, Visibility::None, false, n, false));
            }
        }
        (TreeTab::Layout, ["sec:board"]) => {
            let mut outline = layer_row(
                key,
                "outline",
                t!("tree.layer.outline"),
                c.warn,
                Visibility::Canvas("Cutouts"),
                false,
                1 + board.cutouts.len() + board.holes.len(),
                true,
            );
            outline.key = segment(key, "root");
            outline.select = None;
            out.push(outline);
            out.push(layer_row(
                key,
                "courtyard",
                t!("tree.layer.courtyard"),
                if c.dark { Color32::from_rgb(0xB5, 0x8A, 0xE0) } else { Color32::from_rgb(0x8A, 0x52, 0xB8) },
                Visibility::None,
                false,
                board.footprints.len(),
                false,
            ));
        }
        (TreeTab::Layout, ["sec:board", "root"]) => {
            let mut edge = row(
                key,
                "edge",
                RowKind::Leaf,
                t!("tree.edge", w = format!("{}", board.width), h = format!("{}", board.height)),
            );
            edge.dim = true;
            out.push(edge);
            for cut in &board.cutouts {
                let mut r = row(
                    key,
                    &format!("cut:{}", cut.id),
                    RowKind::Hole,
                    t!("tree.cutout"),
                );
                r.sub = format!("{:.0}, {:.0}", cut.center.x, cut.center.y);
                r.select = Some(cut.id);
                out.push(r);
            }
            for h in &board.holes {
                let mut r = row(
                    key,
                    &format!("hole:{}", h.id),
                    RowKind::Hole,
                    t!("tree.mounting_hole"),
                );
                r.sub = format!("{:.0}, {:.0}", h.position.x, h.position.y);
                r.select = Some(h.id);
                out.push(r);
            }
        }
        (TreeTab::Layout, ["sec:user"]) => {
            for art in &board.user_layers {
                let mut r = layer_row(
                    key,
                    &format!("art:{}", art.name),
                    art.name.clone(),
                    c.link,
                    Visibility::Art(art.id),
                    true,
                    art.shapes.len(),
                    !art.shapes.is_empty(),
                );
                r.key = segment(key, &format!("layer:art:{}", art.id));
                out.push(r);
            }
        }
        (TreeTab::Schematic, [sec]) if sec.starts_with("sec:sheet:") => {
            let id: Id = sec[10..].parse().unwrap_or(0);
            out = schematic_layers(env, key, id);
        }
        (_, ["sec:parts"]) => {
            if env.tab == TreeTab::Layout {
                for f in &board.footprints {
                    let mut r = row(key, &format!("fp:{}", f.id), RowKind::Part, reference_of(project, f));
                    r.sub = project.component(f.component).map(|c| c.value.clone()).unwrap_or_default();
                    r.count = f.pads.len().to_string();
                    r.mono = true;
                    r.select = Some(f.id);
                    r.expandable = !f.pads.is_empty();
                    out.push(r);
                }
            } else {
                for comp in &project.components {
                    out.push(component_row(key, comp));
                }
            }
        }
        (_, ["sec:nets"]) => {
            let unit = if env.tab == TreeTab::Layout { "pads" } else { "pins" };
            out = net_rows(env, key, unit);
        }
        (TreeTab::Layout, ["sec:nets", net]) => {
            let id: Id = net[4..].parse().unwrap_or(0);
            for f in &board.footprints {
                for (i, pad) in f.pads.iter().enumerate() {
                    if pad.net == Some(id) {
                        let mut r = pad_row(key, f, i, pad);
                        r.sub = reference_of(project, f);
                        out.push(r);
                    }
                }
            }
            for t in board.tracks.iter().filter(|t| t.net == id) {
                out.push(track_row(key, t));
            }
            for v in board.vias.iter().filter(|v| v.net == id) {
                out.push(via_row(key, project, v));
            }
        }
        (TreeTab::Schematic, ["sec:nets", net]) => {
            let id: Id = net[4..].parse().unwrap_or(0);
            for comp in &project.components {
                for pin in comp.pins.iter().filter(|p| p.net == Some(id)) {
                    let mut r = row(
                        key,
                        &format!("npin:{}:{}", comp.id, pin.number),
                        RowKind::Pad,
                        t!("tree.pin", number = format!("{}.{}", comp.reference, pin.number), name = pin.name),
                    );
                    r.select = Some(comp.id);
                    r.marks = false;
                    out.push(r);
                }
            }
        }
        (TreeTab::Layout, [.., fp]) if fp.starts_with("fp:") => {
            let id: Id = fp[3..].parse().unwrap_or(0);
            if let Some(f) = project.footprint(id) {
                for (i, pad) in f.pads.iter().enumerate() {
                    let mut r = pad_row(key, f, i, pad);
                    r.sub = pad.net.map(|n| project.net_name(n).to_owned()).unwrap_or_default();
                    out.push(r);
                }
            }
        }
        (TreeTab::Schematic, [.., comp]) if comp.starts_with("comp:") => {
            let id: Id = comp[5..].parse().unwrap_or(0);
            if let Some(cmp) = project.component(id) {
                for (i, pin) in cmp.pins.iter().enumerate() {
                    let mut r = row(
                        key,
                        &format!("pin:{i}"),
                        RowKind::Pad,
                        t!("tree.pin", number = pin.number, name = pin.name),
                    );
                    r.sub = pin.net.map(|n| project.net_name(n).to_owned()).unwrap_or_default();
                    r.select = Some(id);
                    r.marks = false;
                    out.push(r);
                }
            }
        }
        (TreeTab::Layout, [.., layer]) if layer.starts_with("layer:") => {
            layer_children(env, key, &layer[6..], &mut out);
        }
        (TreeTab::Schematic, [.., layer]) if layer.starts_with("layer:") => {
            sheet_layer_children(env, key, &layer[6..], &mut out);
        }
        (TreeTab::Layout, [.., layer, "tracks"]) if layer.starts_with("layer:") => {
            let side = side_of(layer);
            let mut counts: std::collections::BTreeMap<Id, usize> = Default::default();
            for t in board.tracks.iter().filter(|t| t.layer == side) {
                *counts.entry(t.net).or_default() += 1;
            }
            let mut named: Vec<(String, Id, usize)> = counts
                .into_iter()
                .map(|(id, n)| (project.net_name(id).to_owned(), id, n))
                .collect();
            named.sort();
            for (name, id, n) in named {
                let mut r = row(key, &format!("net:{id}"), RowKind::Net, name);
                r.count = n.to_string();
                r.swatch = Some(net_colour(&env.colours, id));
                r.round = true;
                r.net = Some(id);
                r.expandable = true;
                out.push(r);
            }
        }
        (TreeTab::Layout, [.., layer, "tracks", net]) if layer.starts_with("layer:") => {
            let side = side_of(layer);
            let id: Id = net[4..].parse().unwrap_or(0);
            for t in board.tracks.iter().filter(|t| t.layer == side && t.net == id) {
                let mut r = track_row(key, t);
                r.net = Some(id);
                out.push(r);
            }
        }
        (TreeTab::Layout, [.., layer, "pads"]) if layer.starts_with("layer:") => {
            for f in &board.footprints {
                if f.pads.is_empty() {
                    continue;
                }
                let mut r = row(key, &format!("fp:{}", f.id), RowKind::Part, reference_of(project, f));
                r.count = f.pads.len().to_string();
                r.mono = true;
                r.select = Some(f.id);
                r.expandable = true;
                out.push(r);
            }
        }
        (TreeTab::Layout, [.., layer, "zones"]) if layer.starts_with("layer:") => {
            let side = side_of(layer);
            for z in board.zones.iter().filter(|z| z.layer == side) {
                let mut r = row(
                    key,
                    &format!("zone:{}", z.id),
                    RowKind::Zone,
                    t!("tree.zone", net = project.net_name(z.net)),
                );
                r.select = Some(z.id);
                r.net = Some(z.net);
                out.push(r);
            }
        }
        (TreeTab::Layout, [.., layer, "vias"]) if layer.starts_with("layer:") => {
            for v in &board.vias {
                out.push(via_row(key, project, v));
            }
        }
        (TreeTab::Layout, [.., layer, "text"]) if layer.starts_with("layer:") => {
            let side = side_of(layer);
            for x in board.texts.iter().filter(|x| x.layer == side) {
                let mut r = row(key, &format!("text:{}", x.id), RowKind::Text, x.text.clone());
                r.select = Some(x.id);
                out.push(r);
            }
        }
        _ => {}
    }
    out
}

fn component_row(path: &str, c: &pcbcraft_doc::Component) -> Row {
    let mut r = row(path, &format!("comp:{}", c.id), RowKind::Part, c.reference.clone());
    r.sub = c.value.clone();
    r.count = c.pins.len().to_string();
    r.mono = true;
    r.select = Some(c.id);
    r.expandable = !c.pins.is_empty();
    r
}

fn layer_children(env: &Env, key: &str, layer: &str, out: &mut Vec<Row>) {
    let board = &env.project.board;
    match layer {
        "copper:front" => out.extend(copper_children(env, key, Layer::Front)),
        "copper:back" => out.extend(copper_children(env, key, Layer::Back)),
        "silk:front" | "silk:back" => {
            let side = side_of(layer);
            let parts = if side == Layer::Front { board.footprints.len() } else { 0 };
            let mut a = counted(row(key, "refs", RowKind::Leaf, t!("tree.silk.reference_text")), parts, false);
            a.dim = parts == 0;
            let mut b = counted(row(key, "outlines", RowKind::Leaf, t!("tree.silk.outlines")), parts, false);
            b.dim = parts == 0;
            out.push(a);
            out.push(b);
            let texts = board.texts.iter().filter(|x| x.layer == side).count();
            if texts > 0 {
                out.push(group_row(key, "text", t!("tree.group.text"), texts, None));
            }
        }
        other if other.starts_with("art:") => {
            let id: Id = other[4..].parse().unwrap_or(0);
            if let Some(art) = board.user_layers.iter().find(|l| l.id == id) {
                for (i, shape) in art.shapes.iter().enumerate() {
                    out.push(row(key, &format!("shape:{i}"), RowKind::Shape, crate::art_shape_text(shape)));
                }
            }
        }
        _ => {}
    }
}

fn sheet_layer_children(env: &Env, key: &str, layer: &str, out: &mut Vec<Row>) {
    let project = env.project;
    let sheet: Id = key
        .split('/')
        .next()
        .and_then(|s| s.strip_prefix("sec:sheet:"))
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    match layer {
        "wires" => {
            for w in &project.wires {
                let mut r = row(key, &format!("wire:{}", w.id), RowKind::Wire, project.net_name(w.net).to_owned());
                let name = |p: &pcbcraft_doc::PinRef| {
                    project
                        .component(p.component)
                        .map_or_else(|| "?".to_owned(), |c| format!("{}.{}", c.reference, p.pin))
                };
                r.sub = t!("tree.wire_ends", from = name(&w.from), to = name(&w.to));
                r.select = Some(w.id);
                out.push(r);
            }
        }
        "symbols" => {
            for c in project.components.iter().filter(|c| c.sheet == sheet) {
                out.push(component_row(key, c));
            }
        }
        "labels" => {
            for l in project.labels.iter().filter(|l| l.kind != "power") {
                let mut r = row(key, &format!("label:{}", l.id), RowKind::Label, project.net_name(l.net).to_owned());
                r.sub = l.kind.clone();
                r.select = Some(l.id);
                out.push(r);
            }
        }
        _ => {}
    }
}

fn track_row(path: &str, t: &pcbcraft_doc::Track) -> Row {
    let mut r = row(
        path,
        &format!("track:{}", t.id),
        RowKind::Track,
        t!("tree.track", width = format!("{:.2}", t.width)),
    );
    r.sub = side_label(t.layer);
    r.select = Some(t.id);
    r
}

fn via_row(path: &str, project: &Project, v: &pcbcraft_doc::Via) -> Row {
    let mut r = row(
        path,
        &format!("via:{}", v.id),
        RowKind::Via,
        t!("tree.via", drill = format!("{:.2}", v.drill)),
    );
    r.sub = crate::via_span::via_tree_span(project, v);
    r.select = Some(v.id);
    r.net = Some(v.net);
    r
}

fn pad_row(path: &str, f: &pcbcraft_doc::Footprint, i: usize, pad: &pcbcraft_doc::Pad) -> Row {
    let mut r = row(
        path,
        &format!("pad:{}:{i}", f.id),
        RowKind::Pad,
        t!("tree.pad", number = pad.number),
    );
    r.select = Some(f.id);
    r.marks = false;
    r.net = pad.net;
    r
}

/// Walk `row` and its expanded descendants into `out`; with a search active,
/// every branch opens and only matching rows and their ancestors stay.
fn walk(env: &Env, view: &TreeView, row: Row, depth: u8, out: &mut Vec<Row>) -> bool {
    let mut row = row;
    row.depth = depth;
    let start = out.len();
    let searching = !view.query.trim().is_empty();
    let own = searching && matches_query(&row, view.query.trim());
    let open = row.expandable && (view.expanded.contains(&row.key) || searching);
    let key = row.key.clone();
    out.push(row);
    let mut any = false;
    if open {
        for child in children(env, &key) {
            any |= walk(env, view, child, depth.saturating_add(1), out);
        }
    }
    if searching {
        if !(own || any) {
            out.truncate(start);
            return false;
        }
        if own && !any {
            out.truncate(start + 1);
        }
    }
    true
}

fn matches_query(row: &Row, query: &str) -> bool {
    let q = query.to_lowercase();
    row.label.to_lowercase().contains(&q) || row.sub.to_lowercase().contains(&q)
}

fn signature(env: &Env, view: &TreeView) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    env.revision.hash(&mut h);
    (env.tab == TreeTab::Layout).hash(&mut h);
    view.expanded.hash(&mut h);
    view.query.hash(&mut h);
    let b = &env.project.board;
    (
        b.tracks.len(),
        b.footprints.len(),
        b.vias.len(),
        b.zones.len(),
        b.texts.len(),
        b.holes.len(),
        b.cutouts.len(),
        b.user_layers.len(),
        env.project.nets.len(),
        env.project.components.len(),
        env.project.wires.len(),
        env.project.labels.len(),
    )
        .hash(&mut h);
    h.finish()
}

/// The visible rows for this frame: every section and each expanded branch.
///
/// # Rules
/// - Builds only expanded branches unless a search is active.
/// - Cached on a signature of the revision, view state and collection sizes.
///
/// # Do not
/// - Call it from a loop; take the slice once per frame.
pub(crate) fn rows(env: &Env, view: &mut TreeView) -> Vec<Row> {
    let sig = signature(env, view);
    if let Some((cached, rows)) = &view.cache
        && *cached == sig
    {
        return rows.clone();
    }
    let mut out = Vec::new();
    for top in children(env, "") {
        walk(env, view, top, 0, &mut out);
    }
    view.cache = Some((sig, out.clone()));
    out
}

/// The keys to open, outermost first, and the key of the row that stands for
/// object `id`, so a canvas selection can reveal its row.
///
/// # Rules
/// - None when no row stands for the object on this tab.
pub(crate) fn reveal_path(env: &Env, id: Id) -> Option<(Vec<String>, String)> {
    let project = env.project;
    let board = &project.board;
    let target: String = match env.tab {
        TreeTab::Layout => {
            if let Some(t) = board.tracks.iter().find(|t| t.id == id) {
                format!(
                    "sec:copper/layer:{0}/tracks/net:{1}/track:{id}",
                    copper_key(t.layer),
                    t.net
                )
            } else if board.vias.iter().any(|v| v.id == id) {
                format!("sec:copper/layer:copper:front/vias/via:{id}")
            } else if let Some(z) = board.zones.iter().find(|z| z.id == id) {
                format!("sec:copper/layer:{0}/zones/zone:{id}", copper_key(z.layer))
            } else if let Some(x) = board.texts.iter().find(|x| x.id == id) {
                let k = if x.layer == Layer::Front { "silk:front" } else { "silk:back" };
                format!("sec:silk/layer:{k}/text/text:{id}")
            } else if board.cutouts.iter().any(|c| c.id == id) {
                format!("sec:board/root/cut:{id}")
            } else if board.holes.iter().any(|h| h.id == id) {
                format!("sec:board/root/hole:{id}")
            } else if board.footprints.iter().any(|f| f.id == id) {
                format!("sec:parts/fp:{id}")
            } else {
                return None;
            }
        }
        TreeTab::Schematic => {
            if project.component(id).is_some() {
                format!("sec:parts/comp:{id}")
            } else if let Some(l) = project.labels.iter().find(|l| l.id == id) {
                let _ = l;
                format!("sec:sheet:0/layer:labels/label:{id}")
            } else if project.wires.iter().any(|w| w.id == id) {
                format!("sec:sheet:0/layer:wires/wire:{id}")
            } else {
                return None;
            }
        }
    };
    let parts: Vec<&str> = target.split('/').collect();
    let opens = (1..parts.len()).map(|n| parts[..n].join("/")).collect();
    Some((opens, target))
}

/// True when the canvas may not select or edit object `id` because a layer
/// lock covers it.
///
/// # Rules
/// - Tracks, zones, vias and text follow their layer's lock; parts follow
///   their own lock flag in the document, not this set.
pub(crate) fn is_locked(project: &Project, locked: &BTreeSet<String>, id: Id) -> bool {
    if locked.is_empty() {
        return false;
    }
    let board = &project.board;
    if let Some(t) = board.tracks.iter().find(|t| t.id == id) {
        return locked.contains(copper_key(t.layer));
    }
    if board.vias.iter().any(|v| v.id == id) {
        return locked.contains("copper:front") || locked.contains("copper:back");
    }
    if let Some(z) = board.zones.iter().find(|z| z.id == id) {
        return locked.contains(copper_key(z.layer));
    }
    if let Some(x) = board.texts.iter().find(|x| x.id == id) {
        return locked.contains(if x.layer == Layer::Front { "silk:front" } else { "silk:back" });
    }
    if board.cutouts.iter().any(|c| c.id == id) || board.holes.iter().any(|h| h.id == id) {
        return locked.contains("outline");
    }
    false
}

/// Small vector shapes the tree draws instead of text symbols.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Glyph {
    ChevronRight,
    ChevronDown,
    Eye,
    EyeOff,
    Lock,
    Unlock,
    Focus,
    Layer,
    Part,
    Pad,
    Net,
    Track,
    Via,
    Zone,
    Text,
    Graphic,
    Hole,
    Label,
    Plus,
    Search,
    Cross,
    Wire,
    Outline,
}

/// Draw one glyph inside `rect` (about 14 points square).
///
/// # Rules
/// - Strokes only, so it reads on any theme colour.
///
/// # Do not
/// - Add text here; the glyphs are the screen-reader-free face of a control.
pub(crate) fn glyph(painter: &egui::Painter, rect: Rect, g: Glyph, col: Color32) {
    let c = rect.center();
    let s = rect.width().min(rect.height()) / 14.0;
    let p = |x: f32, y: f32| Pos2::new(c.x + x * s, c.y + y * s);
    let stroke = Stroke::new(1.3, col);
    let line = |a: Pos2, b: Pos2| painter.line_segment([a, b], stroke);
    let poly = |pts: Vec<Pos2>, closed: bool| {
        painter.add(Shape::Path(epaint::PathShape {
            points: pts,
            closed,
            fill: Color32::TRANSPARENT,
            stroke: epaint::PathStroke::new(1.3, col),
        }));
    };
    let arc = |cx: f32, cy: f32, r: f32, a0: f32, a1: f32| -> Vec<Pos2> {
        (0..=12)
            .map(|i| {
                let a = a0 + (a1 - a0) * i as f32 / 12.0;
                p(cx + r * a.cos(), cy + r * a.sin())
            })
            .collect()
    };
    match g {
        Glyph::ChevronRight => {
            painter.add(Shape::convex_polygon(
                vec![p(-2.0, -4.0), p(3.0, 0.0), p(-2.0, 4.0)],
                col,
                Stroke::NONE,
            ));
        }
        Glyph::ChevronDown => {
            painter.add(Shape::convex_polygon(
                vec![p(-4.0, -2.0), p(4.0, -2.0), p(0.0, 3.0)],
                col,
                Stroke::NONE,
            ));
        }
        Glyph::Eye | Glyph::EyeOff => {
            let mut lens: Vec<Pos2> = (0..=10)
                .map(|i| {
                    let x = -6.5 + 13.0 * i as f32 / 10.0;
                    p(x, -4.2 * (1.0 - (x / 6.5).powi(2)))
                })
                .collect();
            lens.extend((0..=10).rev().map(|i| {
                let x = -6.5 + 13.0 * i as f32 / 10.0;
                p(x, 4.2 * (1.0 - (x / 6.5).powi(2)))
            }));
            poly(lens, true);
            if g == Glyph::Eye {
                painter.circle_filled(p(0.0, 0.0), 2.0 * s, col);
            } else {
                line(p(-5.5, 5.5), p(5.5, -5.5));
            }
        }
        Glyph::Lock | Glyph::Unlock => {
            let body = Rect::from_min_max(p(-4.5, -0.5), p(4.5, 5.5));
            if g == Glyph::Lock {
                painter.rect_filled(body, 1.5, col);
                poly(arc(0.0, -1.0, 3.0, std::f32::consts::PI, std::f32::consts::TAU), false);
                line(p(-3.0, -1.0), p(-3.0, -0.5));
                line(p(3.0, -1.0), p(3.0, -0.5));
            } else {
                painter.rect_stroke(body, 1.5, stroke, egui::StrokeKind::Inside);
                poly(arc(0.0, -3.0, 3.0, std::f32::consts::PI, std::f32::consts::TAU), false);
                line(p(-3.0, -3.0), p(-3.0, -0.5));
            }
        }
        Glyph::Focus => {
            painter.circle_stroke(p(0.0, 0.0), 4.0 * s, stroke);
            painter.circle_filled(p(0.0, 0.0), 1.3 * s, col);
            line(p(0.0, -7.0), p(0.0, -5.0));
            line(p(0.0, 5.0), p(0.0, 7.0));
            line(p(-7.0, 0.0), p(-5.0, 0.0));
            line(p(5.0, 0.0), p(7.0, 0.0));
        }
        Glyph::Layer => {
            poly(vec![p(0.0, -5.5), p(6.0, -2.5), p(0.0, 0.5), p(-6.0, -2.5)], true);
            poly(vec![p(-6.0, 0.5), p(0.0, 3.5), p(6.0, 0.5)], false);
            poly(vec![p(-6.0, 3.5), p(0.0, 6.5), p(6.0, 3.5)], false);
        }
        Glyph::Part => {
            painter.rect_stroke(
                Rect::from_min_max(p(-4.0, -4.0), p(4.0, 4.0)),
                1.0,
                stroke,
                egui::StrokeKind::Inside,
            );
            for y in [-2.0, 2.0] {
                line(p(-6.5, y), p(-4.0, y));
                line(p(4.0, y), p(6.5, y));
            }
        }
        Glyph::Pad => {
            painter.circle_stroke(p(0.0, 0.0), 4.5 * s, stroke);
            painter.circle_filled(p(0.0, 0.0), 1.6 * s, col);
        }
        Glyph::Net => {
            line(p(-4.5, 3.5), p(0.0, -3.5));
            line(p(0.0, -3.5), p(4.5, 3.5));
            for (x, y) in [(-4.5, 3.5), (0.0, -3.5), (4.5, 3.5)] {
                painter.circle_filled(p(x, y), 1.8 * s, col);
            }
        }
        Glyph::Track => {
            painter.line_segment(
                [p(-5.0, 4.0), p(-1.0, 4.0)],
                Stroke::new(2.0, col),
            );
            painter.line_segment([p(-1.0, 4.0), p(2.0, 0.0)], Stroke::new(2.0, col));
            painter.line_segment([p(2.0, 0.0), p(5.0, 0.0)], Stroke::new(2.0, col));
            painter.circle_filled(p(5.5, 0.0), 1.6 * s, col);
        }
        Glyph::Via => {
            painter.circle_stroke(p(0.0, 0.0), 4.5 * s, Stroke::new(2.0, col));
            painter.circle_filled(p(0.0, 0.0), 1.2 * s, col);
        }
        Glyph::Zone => {
            let r = Rect::from_min_max(p(-5.0, -5.0), p(5.0, 5.0));
            painter.rect_stroke(r, 1.0, stroke, egui::StrokeKind::Inside);
            line(p(-5.0, 1.0), p(1.0, -5.0));
            line(p(-5.0, 5.0), p(5.0, -5.0));
            line(p(-1.0, 5.0), p(5.0, -1.0));
        }
        Glyph::Text => {
            line(p(-4.5, -4.0), p(4.5, -4.0));
            line(p(0.0, -4.0), p(0.0, 5.0));
        }
        Glyph::Graphic => {
            painter.circle_stroke(p(-1.5, 1.5), 3.5 * s, stroke);
            painter.rect_stroke(
                Rect::from_min_max(p(0.0, -5.5), p(5.5, 0.0)),
                0.5,
                stroke,
                egui::StrokeKind::Inside,
            );
        }
        Glyph::Hole => {
            painter.circle_stroke(p(0.0, 0.0), 4.5 * s, stroke);
        }
        Glyph::Label => {
            poly(
                vec![p(-6.0, -3.5), p(2.5, -3.5), p(6.0, 0.0), p(2.5, 3.5), p(-6.0, 3.5)],
                true,
            );
        }
        Glyph::Plus => {
            line(p(-5.0, 0.0), p(5.0, 0.0));
            line(p(0.0, -5.0), p(0.0, 5.0));
        }
        Glyph::Search => {
            painter.circle_stroke(p(-1.5, -1.5), 4.0 * s, stroke);
            line(p(1.5, 1.5), p(6.0, 6.0));
        }
        Glyph::Wire => {
            poly(vec![p(-6.0, 3.0), p(-2.0, 3.0), p(-2.0, -3.0), p(2.0, -3.0), p(2.0, 3.0), p(6.0, 3.0)], false);
        }
        Glyph::Outline => {
            painter.rect_stroke(Rect::from_min_max(p(-5.0, -4.0), p(5.0, 4.0)), 1.0, stroke, egui::StrokeKind::Inside);
        }
        Glyph::Cross => {
            line(p(-4.0, -4.0), p(4.0, 4.0));
            line(p(-4.0, 4.0), p(4.0, -4.0));
        }
    }
}

fn glyph_for(kind: RowKind) -> Glyph {
    match kind {
        RowKind::Section | RowKind::Layer => Glyph::Layer,
        RowKind::Group => Glyph::Pad,
        RowKind::Leaf => Glyph::Outline,
        RowKind::Part => Glyph::Part,
        RowKind::Pad => Glyph::Outline,
        RowKind::Net => Glyph::Net,
        RowKind::Track => Glyph::Track,
        RowKind::Via => Glyph::Via,
        RowKind::Zone => Glyph::Zone,
        RowKind::Text => Glyph::Text,
        RowKind::Hole => Glyph::Hole,
        RowKind::Label => Glyph::Label,
        RowKind::Wire => Glyph::Wire,
        RowKind::Shape => Glyph::Graphic,
    }
}

/// The glyph of a group row, by the segment its key ends in.
fn group_glyph(key: &str) -> Glyph {
    match key.rsplit('/').next().unwrap_or("") {
        "pads" => Glyph::Outline,
        "tracks" => Glyph::Track,
        "zones" => Glyph::Zone,
        "vias" => Glyph::Via,
        "text" => Glyph::Text,
        _ => Glyph::Outline,
    }
}

fn visible(env: &Env, ctl: &LayerControls) -> bool {
    match &ctl.visibility {
        Visibility::Canvas(name) => !env.hidden.iter().any(|h| h == name),
        Visibility::Art(id) => env
            .project
            .board
            .user_layers
            .iter()
            .find(|l| l.id == *id)
            .is_none_or(|l| l.visible),
        Visibility::None => true,
    }
}

/// A square icon button drawn with [`glyph`], with a tooltip and a name for
/// screen readers.
#[allow(clippy::too_many_arguments)]
fn icon_button(
    ui: &mut Ui,
    id: egui::Id,
    rect: Rect,
    g: Glyph,
    col: Color32,
    hover_fill: Color32,
    name: &str,
    tip: &str,
) -> egui::Response {
    let resp = ui.interact(rect, id, Sense::click());
    if resp.hovered() || resp.has_focus() {
        ui.painter().rect_filled(rect.shrink(2.0), 4.0, hover_fill);
    }
    glyph(ui.painter(), Rect::from_center_size(rect.center(), Vec2::splat(16.0)), g, col);
    name_control(&resp, name, None);
    resp.on_hover_text(tip)
}

/// The aside's tab strip (design-system.md 5.5): 40 points high, the active
/// tab in the text colour with a 2-point accent bar under it. Returns the
/// index clicked, if any.
///
/// # Rules
/// - Each tab gets a tooltip and an accessible name; the strip never wraps.
///
/// # Do not
/// - Use it for page navigation; it is for the two tabs of the left aside.
pub(crate) fn aside_tabs(ui: &mut Ui, colours: &ThemeColours, active: u8, tabs: &[(Glyph, String, String)]) -> Option<u8> {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 40.0), Sense::hover());
    let mut clicked = None;
    let mut x = rect.left() + PAD + 4.0;
    for (i, (g, label, tip)) in tabs.iter().enumerate() {
        let font = FontId::proportional(13.0);
        let galley = ui.painter().layout_no_wrap(label.clone(), font, colours.ink);
        let w = galley.size().x + 16.0 + 6.0 + 20.0;
        let tab = Rect::from_min_size(Pos2::new(x, rect.top()), Vec2::new(w, 40.0));
        let resp = ui.interact(tab, ui.id().with(("aside-tab", i)), Sense::click());
        let on = i as u8 == active;
        if resp.hovered() && !on {
            ui.painter().rect_filled(tab.shrink2(Vec2::new(0.0, 6.0)), 4.0, colours.hover);
        }
        let col = if on || resp.hovered() { colours.ink } else { colours.muted };
        glyph(
            ui.painter(),
            Rect::from_center_size(Pos2::new(tab.left() + 17.0, tab.center().y), Vec2::splat(14.0)),
            *g,
            col,
        );
        ui.painter().galley(
            Pos2::new(tab.left() + 30.0, tab.center().y - galley.size().y / 2.0),
            galley,
            col,
        );
        if on {
            ui.painter().rect_filled(
                Rect::from_min_max(
                    Pos2::new(tab.left() + 6.0, tab.bottom() - 2.0),
                    Pos2::new(tab.right() - 6.0, tab.bottom()),
                ),
                1.0,
                colours.accent,
            );
        }
        name_control(&resp, label, Some(on));
        if resp.on_hover_text(tip).clicked() {
            clicked = Some(i as u8);
        }
        x += w + 4.0;
    }
    ui.painter().line_segment(
        [Pos2::new(rect.left(), rect.bottom()), Pos2::new(rect.right(), rect.bottom())],
        Stroke::new(1.0, colours.muted.gamma_multiply(0.25)),
    );
    clicked
}

/// A ghost button: icon and one to three words, 28 points high.
fn ghost_button(ui: &mut Ui, colours: &ThemeColours, g: Glyph, label: &str, tip: &str) -> egui::Response {
    let galley = ui.painter().layout_no_wrap(label.to_owned(), FontId::proportional(13.0), colours.ink);
    let w = galley.size().x + 14.0 + 6.0 + 16.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 28.0), Sense::hover());
    let resp = ui.interact(rect, ui.id().with(("ghost", label)), Sense::click());
    if resp.hovered() || resp.has_focus() {
        ui.painter().rect_filled(rect, 4.0, colours.hover);
    }
    glyph(
        ui.painter(),
        Rect::from_center_size(Pos2::new(rect.left() + 15.0, rect.center().y), Vec2::splat(14.0)),
        g,
        colours.ink,
    );
    ui.painter().galley(
        Pos2::new(rect.left() + 28.0, rect.center().y - galley.size().y / 2.0),
        galley,
        colours.ink,
    );
    name_control(&resp, label, None);
    resp.on_hover_text(tip)
}

/// Draw the tree and return what the user did.
///
/// # Rules
/// - Rows are virtualized: only rows in view are laid out.
/// - Arrows move, Right expands, Left collapses, Space toggles visibility,
///   F focuses, L locks, Enter selects; keys work while a tree row has keyboard focus.
/// - A canvas selection that changed since last frame opens its ancestors and
///   scrolls its row into view.
///
/// # Dependencies
/// - `crates/ui-egui/src/lib.rs` `App::layers_tree_panel` calls it and applies the actions.
/// - `crates/ui-egui/src/a11y.rs` names the controls.
///
/// # Do not
/// - Mutate the project; return an [`Action`] instead.
pub(crate) fn show(ui: &mut Ui, env: &Env, view: &mut TreeView) -> Vec<Action> {
    let mut actions = Vec::new();
    let colours = env.colours;
    if env.selected != view.last_selected {
        view.last_selected = env.selected;
        if let Some(id) = env.selected {
            view.selected_key = None;
            if let Some((opens, target)) = reveal_path(env, id) {
                view.expanded.extend(opens);
                view.cursor = Some(target.clone());
                view.scroll_to = Some(target);
            }
        }
    }
    let all = rows(env, view);
    header(ui, env, view, &all, &colours, &mut actions);
    let n = all.len();
    keyboard(ui, env, view, &all, &mut actions);
    let mut scroll = egui::ScrollArea::vertical()
        .id_salt("layers-tree-scroll")
        .auto_shrink([false, false]);
    if let Some(target) = view.scroll_to.take()
        && let Some(index) = all.iter().position(|r| r.key == target)
    {
        let top = index as f32 * ROW_HEIGHT;
        if top < view.offset || top + ROW_HEIGHT > view.offset + view.viewport {
            scroll = scroll.vertical_scroll_offset((top - view.viewport / 2.0).max(0.0));
        }
    }
    let mut has_focus = false;
    let output = scroll.show_rows(ui, ROW_HEIGHT, n, |ui, range| {
        ui.spacing_mut().item_spacing.y = 0.0;
        for r in &all[range] {
            has_focus |= paint_row(ui, env, view, r, &colours, &mut actions);
        }
    });
    view.offset = output.state.offset.y;
    view.viewport = output.inner_rect.height();
    view.has_focus = has_focus;
    if n == 0 && !view.query.trim().is_empty() {
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new(t!("tree.no_match", query = view.query.trim()))
                .small()
                .color(colours.muted),
        );
    }
    actions
}

/// Filter field with its add button, then Show all, Hide others, New layer.
fn header(ui: &mut Ui, env: &Env, view: &mut TreeView, all: &[Row], colours: &ThemeColours, actions: &mut Vec<Action>) {
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.add_space(PAD + 4.0);
        ui.spacing_mut().item_spacing.x = 8.0;
        let width = ui.available_width() - 28.0 - PAD - 12.0;
        let field = ui.add(
            crate::targets::text_field(&mut view.query)
                .hint_text(t!("tree.search"))
                .desired_width(width),
        );
        field.clone().named(&t!("tree.search")).on_hover_text(t!("tree.search.tip"));
        let r = field.rect;
        glyph(
            ui.painter(),
            Rect::from_center_size(Pos2::new(r.right() - 14.0, r.center().y), Vec2::splat(14.0)),
            if view.query.is_empty() { Glyph::Search } else { Glyph::Cross },
            colours.muted,
        );
        let (cell, _) = ui.allocate_exact_size(Vec2::splat(28.0), Sense::hover());
        let id = ui.id().with("tree-add");
        let resp = ui.interact(cell, id, Sense::click());
        ui.painter().rect_stroke(cell, 4.0, Stroke::new(1.0, colours.muted.gamma_multiply(0.6)), egui::StrokeKind::Inside);
        if resp.hovered() {
            ui.painter().rect_filled(cell.shrink(1.0), 4.0, colours.hover);
        }
        glyph(ui.painter(), Rect::from_center_size(cell.center(), Vec2::splat(14.0)), Glyph::Plus, colours.ink);
        name_control(&resp, &t!("tree.add"), None);
        if resp.on_hover_text(t!("tree.add.tip")).clicked() {
            actions.push(Action::NewArtLayer);
        }
        if !view.query.is_empty() {
            let (x, y) = (r.right() - 22.0, r.top());
            let cross = Rect::from_min_size(Pos2::new(x, y), Vec2::new(22.0, r.height()));
            let resp = ui.interact(cross, ui.id().with("tree-clear"), Sense::click());
            name_control(&resp, &t!("tree.clear"), None);
            if resp.on_hover_text(t!("tree.clear.tip")).clicked() {
                view.query.clear();
            }
        }
    });
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.add_space(PAD);
        ui.spacing_mut().item_spacing.x = 4.0;
        if ghost_button(ui, colours, Glyph::Eye, &t!("tree.show_all"), &t!("tree.show_all.tip")).clicked() {
            actions.push(Action::SetHidden(Vec::new()));
        }
        if ghost_button(ui, colours, Glyph::EyeOff, &t!("tree.hide_others"), &t!("tree.hide_others.tip")).clicked() {
            let keep = view
                .cursor
                .as_deref()
                .and_then(|k| all.iter().find(|r| r.key == k).and_then(|_| layer_visibility_name(all, k)))
                .or(match env.tab {
                    TreeTab::Layout => Some(if env.active == Layer::Front { "Top copper" } else { "Bottom copper" }),
                    TreeTab::Schematic => Some("Wires"),
                });
            actions.push(Action::SetHidden(
                layer_names(env.tab).iter().copied().filter(|n| Some(*n) != keep).collect(),
            ));
        }
        if ghost_button(ui, colours, Glyph::Plus, &t!("tree.new_layer"), &t!("tree.add_layer.tip")).clicked() {
            actions.push(Action::NewArtLayer);
        }
    });
    ui.add_space(6.0);
    let (line, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
    ui.painter().rect_filled(line, 0.0, colours.muted.gamma_multiply(0.22));
}

/// The canvas toggle name of the layer row that `key` sits on or under.
fn layer_visibility_name(all: &[Row], key: &str) -> Option<&'static str> {
    let mut path = key;
    loop {
        if let Some(r) = all.iter().find(|r| r.key == path)
            && let Some(LayerControls { visibility: Visibility::Canvas(name), .. }) = &r.controls
        {
            return Some(name);
        }
        path = path.rsplit_once('/')?.0;
    }
}

/// Paint one row; returns true when it holds keyboard focus.
fn paint_row(
    ui: &mut Ui,
    env: &Env,
    view: &mut TreeView,
    r: &Row,
    colours: &ThemeColours,
    actions: &mut Vec<Action>,
) -> bool {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, ROW_HEIGHT), Sense::hover());
    let id = egui::Id::new(("layers-tree-row", &r.key));
    let resp = ui.interact(rect, id, Sense::click());
    if view.focus_pending.as_deref() == Some(r.key.as_str()) {
        resp.request_focus();
        view.focus_pending = None;
    }
    let focused = resp.has_focus();
    let selected = (r.marks && r.select.is_some() && r.select == env.selected)
        || view.selected_key.as_deref() == Some(r.key.as_str());
    let searching = !view.query.trim().is_empty();
    let expanded = view.expanded.contains(&r.key) || searching;
    let hidden = r.controls.as_ref().is_some_and(|c| !visible(env, c));
    let locked = r
        .controls
        .as_ref()
        .is_some_and(|c| !c.key.is_empty() && view.locked.contains(&c.key));
    let focus_on = r
        .controls
        .as_ref()
        .is_some_and(|c| c.focusable && env.focus == Some(c.key.as_str()));
    let is_section = r.kind == RowKind::Section;
    let dim = if hidden || r.dim { 0.45 } else { 1.0 };
    let painter = ui.painter().clone();
    if selected {
        painter.rect_filled(rect, 0.0, colours.accent.gamma_multiply(0.16));
        painter.rect_filled(Rect::from_min_size(rect.min, Vec2::new(3.0, rect.height())), 0.0, colours.accent);
    } else if resp.hovered() && !is_section {
        painter.rect_filled(rect, 0.0, colours.hover);
    }
    if focused {
        painter.rect_stroke(rect.shrink(1.0), 2.0, Stroke::new(2.0, colours.ink), egui::StrokeKind::Inside);
    }
    let level = r.depth.saturating_sub(1) as f32;
    let level = if is_section { 0.0 } else { level };
    let x0 = rect.left() + PAD + level * INDENT;
    let cy = rect.center().y;
    // Indent guides.
    for d in 0..(level as u32) {
        let x = rect.left() + PAD + d as f32 * INDENT + 8.0;
        painter.line_segment(
            [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
            Stroke::new(1.0, colours.muted.gamma_multiply(0.18)),
        );
    }
    // Chevron cell.
    let mut chevron_clicked = false;
    let chevron_rect = Rect::from_center_size(Pos2::new(x0 + 8.0, cy), Vec2::new(16.0, ROW_HEIGHT));
    if r.expandable {
        let cr = ui.interact(chevron_rect, id.with("chevron"), Sense::click());
        glyph(
            &painter,
            Rect::from_center_size(chevron_rect.center(), Vec2::splat(12.0)),
            if expanded { Glyph::ChevronDown } else { Glyph::ChevronRight },
            if cr.hovered() { colours.ink } else { colours.muted },
        );
        name_control(
            &cr,
            &if expanded { t!("tree.collapse", name = r.label) } else { t!("tree.expand", name = r.label) },
            None,
        );
        chevron_clicked = cr.on_hover_text(t!("tree.chevron.tip")).clicked();
    }
    // Right column: eye, lock, count (design-system 5.7).
    let mut right = rect.right() - PAD;
    let eye_cell = Rect::from_min_size(Pos2::new(right - CELL, rect.top() + 2.0), Vec2::new(CELL, ROW_HEIGHT - 4.0));
    let lock_cell = eye_cell.translate(Vec2::new(-CELL, 0.0));
    let focus_cell = lock_cell.translate(Vec2::new(-CELL, 0.0));
    let hovered = resp.hovered() || focused;
    if let Some(ctl) = &r.controls {
        let can_hide = ctl.visibility != Visibility::None;
        let eye_col = if !can_hide { colours.muted.gamma_multiply(0.4) } else if hidden { colours.muted.gamma_multiply(0.7) } else { colours.ink };
        let (name, tip) = if hidden {
            (t!("tree.show", name = r.label), t!("tree.show.tip"))
        } else if can_hide {
            (t!("tree.hide", name = r.label), t!("tree.hide.tip"))
        } else {
            (t!("tree.hide", name = r.label), t!("tree.hide.none.tip"))
        };
        let eye = icon_button(
            ui,
            id.with("eye"),
            eye_cell,
            if hidden { Glyph::EyeOff } else { Glyph::Eye },
            eye_col,
            colours.hover,
            &name,
            &tip,
        );
        if eye.clicked() && can_hide {
            match &ctl.visibility {
                Visibility::Canvas(name) => actions.push(Action::SetShown { name, on: hidden }),
                Visibility::Art(layer) => actions.push(Action::SetArtShown { layer: *layer, on: hidden }),
                Visibility::None => {}
            }
        }
        if !ctl.key.is_empty() {
            let lock = icon_button(
                ui,
                id.with("lock"),
                lock_cell,
                if locked { Glyph::Lock } else { Glyph::Unlock },
                if locked { colours.ink } else { colours.muted.gamma_multiply(if hovered { 1.0 } else { 0.55 }) },
                colours.hover,
                &if locked { t!("tree.unlock", name = r.label) } else { t!("tree.lock", name = r.label) },
                &if locked { t!("tree.unlock.tip") } else { t!("tree.lock.tip") },
            );
            if lock.clicked() && !view.locked.remove(&ctl.key) {
                view.locked.insert(ctl.key.clone());
            }
        }
        if ctl.focusable && (hovered || focus_on) {
            let fo = icon_button(
                ui,
                id.with("focus"),
                focus_cell,
                Glyph::Focus,
                if focus_on { colours.accent } else { colours.muted },
                colours.hover,
                &if focus_on { t!("tree.unfocus", name = r.label) } else { t!("tree.focus", name = r.label) },
                &if focus_on { t!("tree.unfocus.tip") } else { t!("tree.focus.tip") },
            );
            if fo.clicked() {
                actions.push(Action::Focus(if focus_on { None } else { Some(ctl.key.clone()) }));
            }
        }
        right = if ctl.key.is_empty() { eye_cell.left() - 8.0 } else { lock_cell.left() - 8.0 };
    } else if r.kind == RowKind::Section {
        right = rect.right() - PAD - 4.0;
    } else {
        right = eye_cell.left() - 8.0;
    }
    let mono = FontId::monospace(12.0);
    let mut label_right = right;
    if !r.count.is_empty() {
        let count_col = if is_section { colours.muted } else { colours.muted.gamma_multiply(dim) };
        let rect_c = painter.text(Pos2::new(right, cy), Align2::RIGHT_CENTER, &r.count, mono.clone(), count_col);
        label_right = rect_c.left() - 8.0;
    }
    let active = env.tab == TreeTab::Layout
        && r.kind == RowKind::Layer
        && r.controls.as_ref().is_some_and(|c| c.key == copper_key(env.active));
    if active {
        let badge = painter.layout_no_wrap(t!("tree.active"), FontId::proportional(11.0), colours.accent);
        let w = badge.size().x + 12.0;
        let b = Rect::from_min_size(Pos2::new(label_right - w, cy - 9.0), Vec2::new(w, 18.0));
        painter.rect_filled(b, 4.0, colours.accent.gamma_multiply(0.16));
        painter.galley(Pos2::new(b.left() + 6.0, cy - badge.size().y / 2.0), badge, colours.accent);
        label_right = b.left() - 6.0;
    }
    // Icon or swatch, then the label.
    let icon_c = Pos2::new(x0 + 16.0 + 8.0 + 2.0, cy);
    let label_left;
    if is_section {
        label_left = x0 + 20.0;
    } else {
        label_left = x0 + 16.0 + 24.0;
        if let Some(sw) = r.swatch {
            let box_ = Rect::from_center_size(icon_c, Vec2::splat(12.0));
            if r.round {
                painter.circle_filled(icon_c, 5.0, sw.gamma_multiply(dim));
            } else {
                painter.rect_filled(box_, 3.0, sw.gamma_multiply(dim));
            }
        } else {
            let g = if r.kind == RowKind::Group { group_glyph(&r.key) } else { glyph_for(r.kind) };
            glyph(
                &painter,
                Rect::from_center_size(icon_c, Vec2::splat(14.0)),
                g,
                colours.muted.gamma_multiply(dim),
            );
        }
    }
    let clip = Rect::from_min_max(Pos2::new(label_left, rect.top()), Pos2::new(label_right.max(label_left), rect.bottom()));
    let p = painter.with_clip_rect(clip);
    let top_level = matches!(r.kind, RowKind::Layer | RowKind::Part | RowKind::Net) || is_section;
    let ink = if is_section {
        colours.muted
    } else if top_level {
        colours.ink.gamma_multiply(dim)
    } else {
        colours.muted.gamma_multiply(dim.max(0.7))
    };
    let font = if is_section {
        FontId::proportional(11.0)
    } else if r.mono {
        FontId::monospace(12.0)
    } else {
        FontId::proportional(13.0)
    };
    let text = if is_section { r.label.to_uppercase() } else { r.label.clone() };
    let galley = painter.layout_no_wrap(text, font, ink);
    let label_w = galley.size().x;
    p.galley(Pos2::new(label_left, cy - galley.size().y / 2.0), galley, ink);
    if !r.sub.is_empty() {
        let dx = label_left + label_w + 6.0;
        if dx < label_right {
            p.text(
                Pos2::new(dx, cy),
                Align2::LEFT_CENTER,
                &r.sub,
                FontId::monospace(12.0),
                colours.muted.gamma_multiply(dim),
            );
        }
    }
    // Interaction.
    let state = match &r.controls {
        Some(ctl) if !ctl.key.is_empty() => format!(
            "{}, {}, {}",
            r.label,
            if hidden { t!("tree.state.hidden") } else { t!("tree.state.shown") },
            if locked { t!("tree.state.locked") } else { t!("tree.state.unlocked") }
        ),
        _ if !r.sub.is_empty() => format!("{}, {}", r.label, r.sub),
        _ => r.label.clone(),
    };
    name_control(&resp, &state, Some(selected));
    let tip = match r.kind {
        RowKind::Section => t!("tree.section.tip"),
        RowKind::Layer => t!("tree.layer.tip"),
        _ => t!("tree.row.tip"),
    };
    let resp = resp.on_hover_text(tip);
    if chevron_clicked || (resp.double_clicked() && r.expandable) {
        toggle(view, &r.key);
    } else if resp.clicked() {
        view.cursor = Some(r.key.clone());
        activate(view, r, actions);
    }
    focused
}

fn toggle(view: &mut TreeView, key: &str) {
    if !view.expanded.remove(key) {
        view.expanded.insert(key.to_owned());
    }
}

/// The result of clicking or pressing Enter on a row.
fn activate(view: &mut TreeView, r: &Row, actions: &mut Vec<Action>) {
    if let Some(id) = r.select {
        view.selected_key = None;
        actions.push(Action::Select(id));
        return;
    }
    if r.kind == RowKind::Section {
        toggle(view, &r.key);
        return;
    }
    view.selected_key = Some(r.key.clone());
    actions.push(Action::SelectKey(r.key.clone()));
    actions.push(Action::HighlightNet(r.net));
    if r.expandable && !r.key.ends_with("/root") {
        toggle(view, &r.key);
    }
}

fn keyboard(ui: &mut Ui, env: &Env, view: &mut TreeView, all: &[Row], actions: &mut Vec<Action>) {
    if !view.has_focus || all.is_empty() {
        return;
    }
    let current = view
        .cursor
        .as_ref()
        .and_then(|k| all.iter().position(|r| &r.key == k))
        .unwrap_or(0);
    let press = |ui: &mut Ui, key: egui::Key| ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, key));
    let go = |view: &mut TreeView, index: usize| {
        let index = index.min(all.len() - 1);
        let key = all[index].key.clone();
        view.cursor = Some(key.clone());
        view.focus_pending = Some(key.clone());
        view.scroll_to = Some(key);
    };
    let row = &all[current];
    if press(ui, egui::Key::ArrowDown) {
        go(view, current + 1);
    } else if press(ui, egui::Key::ArrowUp) {
        go(view, current.saturating_sub(1));
    } else if press(ui, egui::Key::Home) {
        go(view, 0);
    } else if press(ui, egui::Key::End) {
        go(view, all.len() - 1);
    } else if press(ui, egui::Key::ArrowRight) {
        if row.expandable && !view.expanded.contains(&row.key) {
            view.expanded.insert(row.key.clone());
        } else if all.get(current + 1).is_some_and(|n| n.depth > row.depth) {
            go(view, current + 1);
        }
    } else if press(ui, egui::Key::ArrowLeft) {
        if row.expandable && view.expanded.contains(&row.key) {
            view.expanded.remove(&row.key);
        } else if let Some(parent) = all[..current].iter().rposition(|p| p.depth < row.depth) {
            go(view, parent);
        }
    } else if press(ui, egui::Key::Enter) {
        activate(view, row, actions);
    } else if press(ui, egui::Key::Space) {
        if let Some(ctl) = layer_of(all, current)
            && ctl.visibility != Visibility::None
        {
            let hidden = !visible(env, &ctl);
            match ctl.visibility {
                Visibility::Canvas(name) => actions.push(Action::SetShown { name, on: hidden }),
                Visibility::Art(layer) => actions.push(Action::SetArtShown { layer, on: hidden }),
                Visibility::None => {}
            }
        }
    } else if press(ui, egui::Key::L)
        && let Some(ctl) = layer_of(all, current)
        && !ctl.key.is_empty()
    {
        if !view.locked.remove(&ctl.key) {
            view.locked.insert(ctl.key);
        }
    } else if press(ui, egui::Key::F)
        && let Some(ctl) = layer_of(all, current)
        && ctl.focusable
    {
        let on = env.focus == Some(ctl.key.as_str());
        actions.push(Action::Focus(if on { None } else { Some(ctl.key) }));
    }
}

/// The controls of the layer row that contains row `index` (itself or an
/// ancestor), so Space and F work from any row of a layer.
fn layer_of(all: &[Row], index: usize) -> Option<LayerControls> {
    if let Some(own) = &all[index].controls {
        return Some(own.clone());
    }
    all[..=index]
        .iter()
        .rev()
        .take_while(|r| r.kind != RowKind::Section)
        .find(|r| r.kind == RowKind::Layer)
        .and_then(|r| r.controls.clone())
}

impl crate::App {
    /// True when a tree row should appear for the Nets list: no search is
    /// active, or the text contains the search.
    pub(crate) fn tree_matches(&self, text: &str) -> bool {
        let query = self.tree.query.trim().to_lowercase();
        query.is_empty() || text.to_lowercase().contains(&query)
    }

    /// The Layers tab of the left aside: draws the tree and applies what the
    /// user did in it.
    ///
    /// # Dependencies
    /// - `crates/ui-egui/src/layers_tree.rs` `show` builds and draws the rows.
    ///
    /// # Do not
    /// - Edit the project here except through `run` and engine commands.
    pub(crate) fn layers_tree_panel(&mut self, ui: &mut Ui) {
        let colours = self.colours();
        let tab = if self.tab == crate::Tab::Schematic {
            TreeTab::Schematic
        } else {
            TreeTab::Layout
        };
        let actions = {
            let env = Env {
                project: &self.project,
                tab,
                colours,
                hidden: &self.preferences.hidden_layers,
                focus: self.focus_layer.as_deref(),
                selected: self.selected,
                active: self.route_layer,
                revision: self.revision,
            };
            show(ui, &env, &mut self.tree)
        };
        for action in actions {
            match action {
                Action::Select(id) => {
                    self.select(Some(id));
                    if !matches!(self.tab, crate::Tab::Pcb | crate::Tab::Schematic) {
                        self.change_tab(crate::Tab::Pcb);
                    }
                }
                Action::SelectKey(_) => self.select(None),
                Action::SetShown { name, on } => {
                    let hidden = &mut self.preferences.hidden_layers;
                    hidden.retain(|h| h != name);
                    if !on {
                        hidden.push(name.to_owned());
                    }
                }
                Action::SetHidden(names) => {
                    let all = layer_names(tab);
                    let hidden = &mut self.preferences.hidden_layers;
                    hidden.retain(|h| !all.contains(&h.as_str()));
                    hidden.extend(names.into_iter().map(str::to_owned));
                    let ids: Vec<(Id, bool)> = self
                        .project
                        .board
                        .user_layers
                        .iter()
                        .map(|l| (l.id, l.visible))
                        .collect();
                    if self.preferences.hidden_layers.len() < all.len() {
                        for (id, shown) in ids {
                            if !shown {
                                self.run("board.layer_visible_set", serde_json::json!({"layer": id, "on": true}));
                            }
                        }
                    }
                }
                Action::SetArtShown { layer, on } => {
                    self.run("board.layer_visible_set", serde_json::json!({"layer": layer, "on": on}));
                }
                Action::Focus(key) => self.focus_layer = key,
                Action::HighlightNet(net) => self.highlight_net = net,
                Action::NewArtLayer => {
                    let name = t!("tree.new_layer_name", n = self.project.board.user_layers.len() + 1);
                    self.run("board.layer_add", serde_json::json!({"name": name, "kind": "other"}));
                    if self.message.starts_with("board.layer_add") {
                        self.art_layer = self.selected;
                    }
                }
            }
        }
    }

    /// Properties for a tree row that is not one document object: the board
    /// outline (size, stackup, finish, origin) or a net (its connection editor).
    ///
    /// # Dependencies
    /// - `crates/ui-egui/src/lib.rs` `stackup_section`, `holes_section`, `datums_section`,
    ///   `fiducials_section`, `via_spans_section` and `nets_tab` draw the sections.
    pub(crate) fn tree_row_properties(&mut self, ui: &mut Ui) {
        let Some(key) = self.tree.selected_key.clone() else {
            return;
        };
        let last = key.rsplit('/').next().unwrap_or("");
        if key.ends_with("/root") {
            ui.heading(t!("tree.board_settings"));
            ui.label(egui::RichText::new(t!("tree.board_settings.help")).small().color(self.muted()));
            ui.add_space(8.0);
            self.stackup_section(ui);
            ui.add_space(8.0);
            self.holes_section(ui);
            self.datums_section(ui);
            self.fiducials_section(ui);
            self.via_spans_section(ui);
        } else if last.starts_with("net:") || key == "sec:nets" {
            ui.heading(t!("tree.net_settings"));
            self.nets_tab(ui);
        } else {
            let name = self
                .tree
                .cursor
                .as_deref()
                .and_then(|k| k.rsplit('/').next())
                .map(|s| s.rsplit(':').next().unwrap_or(s).to_owned())
                .unwrap_or_default();
            ui.heading(name);
            ui.label(egui::RichText::new(t!("tree.row_properties.help")).small().color(self.muted()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Project {
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = crate::App::create(shared, crate::Preferences::default(), None, None);
        app.tool_call("sample_design", serde_json::json!({}));
        app.project.clone()
    }

    fn env<'a>(project: &'a Project, hidden: &'a [String]) -> Env<'a> {
        Env {
            project,
            tab: TreeTab::Layout,
            colours: crate::palette::palette(true, [0, 200, 160]),
            hidden,
            focus: None,
            selected: None,
            active: Layer::Front,
            revision: 1,
        }
    }

    #[test]
    fn sections_come_in_the_mockup_order() {
        let project = sample();
        let mut view = TreeView::default();
        let list = rows(&env(&project, &[]), &mut view);
        let sections: Vec<&str> = list
            .iter()
            .filter(|r| r.kind == RowKind::Section)
            .map(|r| r.key.as_str())
            .collect();
        assert_eq!(
            sections,
            ["sec:copper", "sec:silk", "sec:mask", "sec:board", "sec:user", "sec:parts", "sec:nets"]
        );
        assert!(list.iter().any(|r| r.key == "sec:copper/layer:copper:front"));
    }

    #[test]
    fn only_expanded_branches_are_built() {
        let project = sample();
        let mut view = TreeView::default();
        view.expanded.remove("sec:parts");
        let closed = rows(&env(&project, &[]), &mut view).len();
        view.expanded.insert("sec:parts".to_owned());
        let open = rows(&env(&project, &[]), &mut view).len();
        assert_eq!(open, closed + project.board.footprints.len());
    }

    #[test]
    fn a_search_keeps_matches_and_their_ancestors() {
        let project = sample();
        let net = project.nets[0].name.clone();
        let mut view = TreeView {
            query: net.clone(),
            ..Default::default()
        };
        let list = rows(&env(&project, &[]), &mut view);
        assert!(list.iter().any(|r| r.kind == RowKind::Net && r.label == net));
        assert!(list.iter().any(|r| r.key == "sec:nets"));
        view.query = "zzzz-no-such-thing".to_owned();
        assert!(rows(&env(&project, &[]), &mut view).is_empty());
    }

    #[test]
    fn a_part_selected_on_the_canvas_opens_its_row() -> Result<(), String> {
        let project = sample();
        let id = project.board.footprints[0].id;
        let (opens, target) = reveal_path(&env(&project, &[]), id).ok_or("no row")?;
        assert_eq!(target, format!("sec:parts/fp:{id}"));
        assert!(opens.contains(&"sec:parts".to_owned()));
        let mut view = TreeView::default();
        view.expanded.extend(opens);
        assert!(rows(&env(&project, &[]), &mut view).iter().any(|r| r.key == target));
        Ok(())
    }

    #[test]
    fn locking_a_layer_locks_its_tracks_only() {
        let project = sample();
        let mut locked = BTreeSet::new();
        locked.insert("copper:front".to_owned());
        if let Some(t) = project.board.tracks.iter().find(|t| t.layer == Layer::Front) {
            assert!(is_locked(&project, &locked, t.id));
        }
        assert!(!is_locked(&project, &locked, project.board.footprints[0].id));
    }

    #[test]
    fn the_board_outline_row_opens_board_settings() -> Result<(), String> {
        let project = sample();
        let mut view = TreeView::default();
        let list = rows(&env(&project, &[]), &mut view);
        let outline = list
            .iter()
            .find(|r| r.key == "sec:board/root")
            .ok_or("no outline row")?;
        let mut actions = Vec::new();
        activate(&mut view, outline, &mut actions);
        assert_eq!(view.selected_key.as_deref(), Some("sec:board/root"));
        assert!(actions.contains(&Action::SelectKey("sec:board/root".to_owned())));
        Ok(())
    }

    #[test]
    fn two_thousand_parts_build_quickly() {
        let mut project = sample();
        let template = project.board.footprints[0].clone();
        let component = project.components[0].clone();
        for i in 0..2000u64 {
            let mut f = template.clone();
            f.id = 1_000_000 + i;
            f.component = component.id;
            project.board.footprints.push(f);
        }
        let mut view = TreeView::default();
        view.expanded.insert("sec:parts".to_owned());
        let e = env(&project, &[]);
        let start = std::time::Instant::now();
        let list = rows(&e, &mut view);
        let first = start.elapsed();
        let start = std::time::Instant::now();
        let again = rows(&e, &mut view);
        let cached = start.elapsed();
        assert!(list.len() > 2000 && again.len() == list.len());
        assert!(first.as_millis() < 250, "first build took {first:?}");
        assert!(cached.as_millis() < 50, "cached build took {cached:?}");
    }
}
