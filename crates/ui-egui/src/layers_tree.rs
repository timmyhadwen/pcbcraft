//! The unified Layers tree of the left aside: one tree for the board, its
//! layers, parts and nets (Layout) or its sheets, parts and nets (Schematic).
//!
//! Context: `lib.rs` hosts the left aside's Layers and Library tabs, builds an
//! [`Env`] from the app each frame, calls [`show`] and applies the returned
//! [`Action`]s. The tree never edits the document and never reads `App`.
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
pub(crate) const ROW_HEIGHT: f32 = 24.0;
/// Horizontal step of one nesting level in points.
const INDENT: f32 = 14.0;

/// Which tab's tree to build.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum TreeTab {
    /// Board: layers, parts, nets.
    Layout,
    /// Schematic: sheets, parts, nets and labels.
    Schematic,
}

/// What one row stands for; picks its glyph and which kind filter hides it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum RowKind {
    Root,
    Layer,
    Group,
    Part,
    Pad,
    Net,
    Track,
    Via,
    Zone,
    Text,
    Graphic,
    Hole,
    Sheet,
    Label,
    Wire,
    Shape,
    Add,
}

/// The entity kinds the filter row can switch off.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub(crate) enum KindFilter {
    Parts,
    Nets,
    Tracks,
    Vias,
    Zones,
    Text,
}

impl KindFilter {
    /// Every filter, in the order the buttons appear.
    pub(crate) const ALL: [KindFilter; 6] = [
        KindFilter::Parts,
        KindFilter::Nets,
        KindFilter::Tracks,
        KindFilter::Vias,
        KindFilter::Zones,
        KindFilter::Text,
    ];
    fn glyph(self) -> Glyph {
        match self {
            KindFilter::Parts => Glyph::Part,
            KindFilter::Nets => Glyph::Net,
            KindFilter::Tracks => Glyph::Track,
            KindFilter::Vias => Glyph::Via,
            KindFilter::Zones => Glyph::Zone,
            KindFilter::Text => Glyph::Text,
        }
    }
    fn name(self) -> String {
        match self {
            KindFilter::Parts => t!("tree.kind.parts"),
            KindFilter::Nets => t!("tree.kind.nets"),
            KindFilter::Tracks => t!("tree.kind.tracks"),
            KindFilter::Vias => t!("tree.kind.vias"),
            KindFilter::Zones => t!("tree.kind.zones"),
            KindFilter::Text => t!("tree.kind.text"),
        }
    }
    fn tip(self) -> String {
        match self {
            KindFilter::Parts => t!("tree.kind.parts.tip"),
            KindFilter::Nets => t!("tree.kind.nets.tip"),
            KindFilter::Tracks => t!("tree.kind.tracks.tip"),
            KindFilter::Vias => t!("tree.kind.vias.tip"),
            KindFilter::Zones => t!("tree.kind.zones.tip"),
            KindFilter::Text => t!("tree.kind.text.tip"),
        }
    }
}

/// How a layer row's eye reaches the drawing.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) enum Visibility {
    /// A named drawing toggle of the canvas (`hidden_layers` entry).
    Canvas(&'static str),
    /// A machine layer the designer drew; the document stores its flag.
    Art(Id),
    /// The canvas draws nothing for this layer yet; the eye is inert.
    None,
}

/// Eye, lock and focus controls of a layer row.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct LayerControls {
    /// The `focus_layer` key; the canvas dims everything not on it.
    pub focus_key: String,
    pub visibility: Visibility,
}

/// One visible line of the tree.
#[derive(Clone, PartialEq, Debug)]
pub(crate) struct Row {
    /// Path key, unique in the tree, made of `/`-joined segments.
    pub key: String,
    pub depth: u8,
    pub kind: RowKind,
    pub label: String,
    /// Muted text after the label: a count, a net or a size.
    pub detail: String,
    pub expandable: bool,
    /// The document object the canvas selects for this row.
    pub select: Option<Id>,
    pub layer: Option<LayerControls>,
    pub swatch: Option<Color32>,
    /// The net this row stands for, which the canvas highlights.
    pub net: Option<Id>,
}

/// What the tree asks the app to do; the app applies each in `lib.rs`.
#[derive(Clone, PartialEq, Debug)]
pub(crate) enum Action {
    /// Select a document object on the canvas and in Properties.
    Select(Id),
    /// Select a row that is not one object (the board, a layer, a net).
    SelectKey(String),
    /// Set a canvas drawing toggle shown (true) or hidden.
    SetShown { name: &'static str, on: bool },
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
    /// Bumps on every document change; part of the row cache key.
    pub revision: u64,
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
    pub kinds_off: BTreeSet<KindFilter>,
    /// Layer keys the canvas may not select from or edit.
    pub locked: BTreeSet<String>,
    /// Keyboard cursor row key.
    pub cursor: Option<String>,
    /// A row selected that is not one object: "root", a layer key, a net key.
    pub selected_key: Option<String>,
    last_selected: Option<Id>,
    scroll_to: Option<String>,
    focus_pending: Option<String>,
    has_focus: bool,
    offset: f32,
    viewport: f32,
    cache: Option<(u64, Vec<Row>)>,
}

impl Default for TreeView {
    fn default() -> Self {
        Self {
            expanded: ["root".to_owned()].into_iter().collect(),
            query: String::new(),
            kinds_off: BTreeSet::new(),
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
        detail: String::new(),
        expandable: false,
        select: None,
        layer: None,
        swatch: None,
        net: None,
    }
}

fn reference_of(project: &Project, footprint: &pcbcraft_doc::Footprint) -> String {
    project
        .component(footprint.component)
        .map_or_else(|| t!("tree.part.unknown"), |c| c.reference.clone())
}

/// The layer rows of the board, in stackup order, then the fixed rows, then
/// the machine layers the designer drew.
fn layer_rows(env: &Env, colours: &ThemeColours) -> Vec<Row> {
    let project = env.project;
    let mut rows = Vec::new();
    let mut layer = |key: &str,
                     label: String,
                     swatch: Color32,
                     visibility: Visibility,
                     expandable: bool| {
        let mut r = row("", &format!("layer:{key}"), RowKind::Layer, label);
        r.expandable = expandable;
        r.swatch = Some(swatch);
        r.layer = Some(LayerControls {
            focus_key: key.to_owned(),
            visibility,
        });
        rows.push(r);
    };
    layer(
        "copper:front",
        t!("tree.layer.top_copper"),
        colours.canvas.front,
        Visibility::Canvas("Top copper"),
        true,
    );
    let coppers: Vec<&pcbcraft_doc::LayerRecord> = project
        .board
        .layer_records
        .iter()
        .filter(|l| l.kind == "copper")
        .collect();
    if coppers.len() > 2 {
        for (i, record) in coppers[1..coppers.len() - 1].iter().enumerate() {
            layer(
                &format!("copper:inner:{}", i + 1),
                record.name.clone(),
                colours.canvas.front.gamma_multiply(0.6),
                Visibility::None,
                false,
            );
        }
    }
    layer(
        "copper:back",
        t!("tree.layer.bottom_copper"),
        colours.canvas.back,
        Visibility::Canvas("Bottom copper"),
        true,
    );
    layer(
        "silk",
        t!("tree.layer.silk"),
        colours.ink,
        Visibility::Canvas("Package artwork"),
        true,
    );
    layer(
        "mask",
        t!("tree.layer.mask"),
        colours.canvas.mask,
        Visibility::None,
        true,
    );
    layer(
        "paste",
        t!("tree.layer.paste"),
        colours.muted,
        Visibility::None,
        false,
    );
    layer(
        "courtyard",
        t!("tree.layer.courtyard"),
        colours.link,
        Visibility::None,
        false,
    );
    layer(
        "fab",
        t!("tree.layer.fab"),
        colours.warn,
        Visibility::None,
        false,
    );
    layer(
        "edge",
        t!("tree.layer.edge"),
        colours.accent,
        Visibility::Canvas("Cutouts"),
        true,
    );
    for art in &project.board.user_layers {
        layer(
            &format!("art:{}", art.name),
            art.name.clone(),
            colours.accent.gamma_multiply(0.8),
            Visibility::Art(art.id),
            !art.shapes.is_empty(),
        );
        if let Some(r) = rows.last_mut() {
            r.detail = t!("tree.count", n = art.shapes.len());
            r.key = format!("layer:art:{}", art.id);
            r.select = None;
        }
    }
    rows
}

fn count_detail(n: usize) -> String {
    t!("tree.count", n = n)
}

/// The copper layers' child groups, each with a count, empty ones left out.
fn layer_groups(env: &Env, view: &TreeView, path: &str, side: Layer) -> Vec<Row> {
    let board = &env.project.board;
    let mut out = Vec::new();
    let off = |k: KindFilter| view.kinds_off.contains(&k);
    let mut group = |seg: &str, label: String, n: usize, out: &mut Vec<Row>| {
        if n > 0 {
            let mut r = row(path, seg, RowKind::Group, label);
            r.expandable = true;
            r.detail = count_detail(n);
            out.push(r);
        }
    };
    if !off(KindFilter::Tracks) {
        let n = board.tracks.iter().filter(|t| t.layer == side).count();
        group("tracks", t!("tree.group.tracks"), n, &mut out);
    }
    if !off(KindFilter::Parts) && side == Layer::Front {
        let n: usize = board.footprints.iter().map(|f| f.pads.len()).sum();
        group("pads", t!("tree.group.pads"), n, &mut out);
    }
    if !off(KindFilter::Zones) {
        let n = board.zones.iter().filter(|z| z.layer == side).count();
        group("zones", t!("tree.group.zones"), n, &mut out);
    }
    if !off(KindFilter::Vias) {
        group("vias", t!("tree.group.vias"), board.vias.len(), &mut out);
    }
    out
}

/// The rows directly under `key`. Only called for expanded rows.
fn children(env: &Env, view: &TreeView, key: &str, colours: &ThemeColours) -> Vec<Row> {
    let project = env.project;
    let board = &project.board;
    let segs: Vec<&str> = key.split('/').collect();
    let off = |k: KindFilter| view.kinds_off.contains(&k);
    let mut out = Vec::new();
    match (env.tab, segs.as_slice()) {
        (TreeTab::Layout, ["root"]) => {
            out = layer_rows(env, colours);
            if !off(KindFilter::Parts) {
                let mut r = row("", "parts", RowKind::Group, t!("tree.branch.parts"));
                r.expandable = !board.footprints.is_empty();
                r.detail = count_detail(board.footprints.len());
                out.push(r);
            }
            if !off(KindFilter::Nets) {
                let mut r = row("", "nets", RowKind::Group, t!("tree.branch.nets"));
                r.expandable = !project.nets.is_empty();
                r.detail = count_detail(project.nets.len());
                out.push(r);
            }
            let mut add = row("", "add-art", RowKind::Add, t!("tree.add_layer"));
            add.expandable = false;
            out.push(add);
        }
        (TreeTab::Schematic, ["root"]) => {
            let mut sheets: Vec<(Id, String)> = vec![(0, t!("tree.sheet.first"))];
            sheets.extend(project.sheets.iter().map(|s| (s.id, s.name.clone())));
            for (id, name) in sheets {
                let n = project.components.iter().filter(|c| c.sheet == id).count();
                let mut r = row("", &format!("sheet:{id}"), RowKind::Sheet, name);
                r.expandable = n > 0;
                r.detail = count_detail(n);
                out.push(r);
            }
            if !off(KindFilter::Nets) {
                let mut r = row("", "nets", RowKind::Group, t!("tree.branch.nets"));
                r.expandable = !project.nets.is_empty();
                r.detail = count_detail(project.nets.len());
                out.push(r);
            }
        }
        (TreeTab::Schematic, [sheet]) if sheet.starts_with("sheet:") => {
            let id: Id = sheet[6..].parse().unwrap_or(0);
            if !off(KindFilter::Parts) {
                for c in project.components.iter().filter(|c| c.sheet == id) {
                    let mut r = row(key, &format!("comp:{}", c.id), RowKind::Part, c.reference.clone());
                    r.detail = c.value.clone();
                    r.select = Some(c.id);
                    r.expandable = !c.pins.is_empty();
                    out.push(r);
                }
            }
        }
        (TreeTab::Schematic, [.., comp]) if comp.starts_with("comp:") => {
            let id: Id = comp[5..].parse().unwrap_or(0);
            if let Some(c) = project.component(id) {
                for (i, pin) in c.pins.iter().enumerate() {
                    let mut r = row(
                        key,
                        &format!("pin:{i}"),
                        RowKind::Pad,
                        t!("tree.pin", number = pin.number, name = pin.name),
                    );
                    r.detail = pin.net.map(|n| project.net_name(n).to_owned()).unwrap_or_default();
                    r.select = Some(id);
                    out.push(r);
                }
            }
        }
        (_, ["nets"]) => {
            let mut nets: Vec<&pcbcraft_doc::Net> = project.nets.iter().collect();
            nets.sort_by(|a, b| a.name.cmp(&b.name));
            for n in nets {
                let mut r = row(key, &format!("net:{}", n.id), RowKind::Net, n.name.clone());
                r.net = Some(n.id);
                r.expandable = true;
                out.push(r);
            }
        }
        (TreeTab::Layout, ["nets", net]) => {
            let id: Id = net[4..].parse().unwrap_or(0);
            if !off(KindFilter::Tracks) {
                for t in board.tracks.iter().filter(|t| t.net == id) {
                    let mut r = track_row(key, t);
                    r.net = Some(id);
                    out.push(r);
                }
            }
            if !off(KindFilter::Vias) {
                for v in board.vias.iter().filter(|v| v.net == id) {
                    out.push(via_row(key, v));
                }
            }
            if !off(KindFilter::Parts) {
                for f in &board.footprints {
                    for (i, pad) in f.pads.iter().enumerate() {
                        if pad.net == Some(id) {
                            let mut r = pad_row(key, f, i, pad, false);
                            r.detail = reference_of(project, f);
                            out.push(r);
                        }
                    }
                }
            }
        }
        (TreeTab::Schematic, ["nets", net]) => {
            let id: Id = net[4..].parse().unwrap_or(0);
            for l in project.labels.iter().filter(|l| l.net == id) {
                let mut r = row(key, &format!("label:{}", l.id), RowKind::Label, l.kind.clone());
                r.detail = project.net_name(id).to_owned();
                r.select = Some(l.id);
                out.push(r);
            }
            for w in project.wires.iter().filter(|w| w.net == id) {
                let mut r = row(key, &format!("wire:{}", w.id), RowKind::Wire, t!("tree.wire"));
                r.select = Some(w.id);
                out.push(r);
            }
        }
        (TreeTab::Layout, ["parts"]) => {
            for f in &board.footprints {
                let mut r = row(key, &format!("fp:{}", f.id), RowKind::Part, reference_of(project, f));
                r.detail = project
                    .component(f.component)
                    .map(|c| c.value.clone())
                    .unwrap_or_default();
                r.select = Some(f.id);
                r.expandable = !f.pads.is_empty();
                out.push(r);
            }
        }
        (TreeTab::Layout, [.., fp]) if fp.starts_with("fp:") => {
            let id: Id = fp[3..].parse().unwrap_or(0);
            if let Some(f) = project.footprint(id) {
                for (i, pad) in f.pads.iter().enumerate() {
                    out.push(pad_row(key, f, i, pad, true).with_net_detail(project, pad.net));
                }
            }
        }
        (TreeTab::Layout, [layer]) if layer.starts_with("layer:") => {
            layer_children(env, view, key, &layer[6..], &mut out);
        }
        (TreeTab::Layout, [layer, "tracks"]) => {
            let side = if layer.ends_with("front") { Layer::Front } else { Layer::Back };
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
                r.detail = count_detail(n);
                r.net = Some(id);
                r.expandable = true;
                out.push(r);
            }
        }
        (TreeTab::Layout, [layer, "tracks", net]) => {
            let side = if layer.ends_with("front") { Layer::Front } else { Layer::Back };
            let id: Id = net[4..].parse().unwrap_or(0);
            for t in board.tracks.iter().filter(|t| t.layer == side && t.net == id) {
                let mut r = track_row(key, t);
                r.net = Some(id);
                out.push(r);
            }
        }
        (TreeTab::Layout, [layer, "pads"]) if layer.ends_with("front") => {
            for f in &board.footprints {
                if f.pads.is_empty() {
                    continue;
                }
                let mut r = row(key, &format!("fp:{}", f.id), RowKind::Part, reference_of(project, f));
                r.detail = count_detail(f.pads.len());
                r.select = Some(f.id);
                r.expandable = true;
                out.push(r);
            }
        }
        (TreeTab::Layout, [layer, "zones"]) => {
            let side = if layer.ends_with("front") { Layer::Front } else { Layer::Back };
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
        (TreeTab::Layout, [_, "vias"]) => {
            for v in &board.vias {
                out.push(via_row(key, v));
            }
        }
        (TreeTab::Layout, [_, "text"]) => {
            for x in &board.texts {
                let mut r = row(key, &format!("text:{}", x.id), RowKind::Text, x.text.clone());
                r.detail = side_label(x.layer);
                r.select = Some(x.id);
                out.push(r);
            }
        }
        (TreeTab::Layout, [_, "graphics"]) => {
            for g in board.graphics.iter().filter(|g| g.layer == "board") {
                let mut r = row(key, &format!("gfx:{}", g.id), RowKind::Graphic, g.shape.clone());
                r.detail = g.sublayer.clone();
                r.select = Some(g.id);
                out.push(r);
            }
        }
        (TreeTab::Layout, [_, "holes"]) => {
            for c in &board.cutouts {
                let mut r = row(
                    key,
                    &format!("cut:{}", c.id),
                    RowKind::Hole,
                    t!("tree.cutout", size = format!("{:.1}", c.radius * 2.0)),
                );
                r.select = Some(c.id);
                out.push(r);
            }
            for h in &board.holes {
                let mut r = row(
                    key,
                    &format!("hole:{}", h.id),
                    RowKind::Hole,
                    t!("tree.hole", size = format!("{:.2}", h.diameter)),
                );
                r.select = Some(h.id);
                out.push(r);
            }
        }
        _ => {}
    }
    for r in &mut out {
        if r.key.is_empty() {
            r.key = segment(key, "row");
        }
    }
    out
}

fn layer_children(env: &Env, view: &TreeView, key: &str, layer: &str, out: &mut Vec<Row>) {
    let board = &env.project.board;
    let off = |k: KindFilter| view.kinds_off.contains(&k);
    match layer {
        "copper:front" => out.extend(layer_groups(env, view, key, Layer::Front)),
        "copper:back" => out.extend(layer_groups(env, view, key, Layer::Back)),
        "silk" => {
            let mut group = |seg: &str, label: String, n: usize| {
                if n > 0 {
                    let mut r = row(key, seg, RowKind::Group, label);
                    r.expandable = true;
                    r.detail = count_detail(n);
                    out.push(r);
                }
            };
            if !off(KindFilter::Text) {
                group("text", t!("tree.group.text"), board.texts.len());
            }
            group(
                "graphics",
                t!("tree.group.graphics"),
                board.graphics.iter().filter(|g| g.layer == "board").count(),
            );
        }
        "mask" => {
            let n = board.graphics.iter().filter(|g| g.sublayer.contains("Mask")).count();
            if n > 0 {
                let mut r = row(key, "graphics", RowKind::Group, t!("tree.group.graphics"));
                r.expandable = true;
                r.detail = count_detail(n);
                out.push(r);
            }
        }
        "edge" => {
            let n = board.cutouts.len() + board.holes.len();
            if n > 0 {
                let mut r = row(key, "holes", RowKind::Group, t!("tree.group.holes"));
                r.expandable = true;
                r.detail = count_detail(n);
                out.push(r);
            }
        }
        other if other.starts_with("art:") => {
            let id: Id = other[4..].parse().unwrap_or(0);
            if let Some(art) = board.user_layers.iter().find(|l| l.id == id) {
                for (i, shape) in art.shapes.iter().enumerate() {
                    let mut r = row(key, &format!("shape:{i}"), RowKind::Shape, crate::art_shape_text(shape));
                    r.select = None;
                    out.push(r);
                }
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
    r.detail = side_label(t.layer);
    r.select = Some(t.id);
    r
}

fn via_row(path: &str, v: &pcbcraft_doc::Via) -> Row {
    let mut r = row(
        path,
        &format!("via:{}", v.id),
        RowKind::Via,
        t!("tree.via", drill = format!("{:.2}", v.drill)),
    );
    r.select = Some(v.id);
    r.net = Some(v.net);
    r
}

fn pad_row(path: &str, f: &pcbcraft_doc::Footprint, i: usize, pad: &pcbcraft_doc::Pad, _own: bool) -> Row {
    let mut r = row(
        path,
        &format!("pad:{}:{i}", f.id),
        RowKind::Pad,
        t!("tree.pad", number = pad.number),
    );
    r.select = Some(f.id);
    r.net = pad.net;
    r
}

impl Row {
    fn with_net_detail(mut self, project: &Project, net: Option<Id>) -> Row {
        self.detail = net.map(|n| project.net_name(n).to_owned()).unwrap_or_default();
        self
    }
}

/// Walk `row` and its expanded descendants into `out`; with a search active,
/// every branch opens and only matching rows and their ancestors stay.
fn walk(env: &Env, view: &TreeView, colours: &ThemeColours, mut row: Row, depth: u8, out: &mut Vec<Row>) -> bool {
    row.depth = depth;
    let start = out.len();
    let searching = !view.query.trim().is_empty();
    let own = searching && matches_query(&row, view.query.trim());
    let open = row.expandable && (view.expanded.contains(&row.key) || searching);
    let key = row.key.clone();
    let is_root = key == "root";
    out.push(row);
    let mut any = false;
    if open || is_root {
        for child in children(env, view, &key, colours) {
            any |= walk(env, view, colours, child, depth.saturating_add(1), out);
        }
    }
    if searching && !is_root {
        if !(own || any) {
            out.truncate(start);
            return false;
        }
        if own && !any {
            out.truncate(start + 1);
        }
        return true;
    }
    true
}

fn matches_query(row: &Row, query: &str) -> bool {
    let q = query.to_lowercase();
    row.label.to_lowercase().contains(&q) || row.detail.to_lowercase().contains(&q)
}

fn signature(env: &Env, view: &TreeView) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    env.revision.hash(&mut h);
    (env.tab == TreeTab::Layout).hash(&mut h);
    view.expanded.hash(&mut h);
    view.query.hash(&mut h);
    view.kinds_off.iter().for_each(|k| (*k as u8).hash(&mut h));
    let b = &env.project.board;
    (
        b.tracks.len(),
        b.footprints.len(),
        b.vias.len(),
        b.zones.len(),
        b.texts.len(),
        b.user_layers.len(),
        env.project.nets.len(),
        env.project.components.len(),
    )
        .hash(&mut h);
    h.finish()
}

/// The visible rows for this frame: the root, then every expanded branch.
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
    let colours = env.colours;
    let name = if env.project.name.trim().is_empty() {
        t!("tree.root_fallback")
    } else {
        env.project.name.clone()
    };
    let mut root = row("", "root", RowKind::Root, name);
    root.expandable = true;
    let mut out = Vec::new();
    walk(env, view, &colours, root, 0, &mut out);
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
                format!("layer:{0}/tracks/net:{1}/track:{id}", copper_key(t.layer), t.net)
            } else if let Some(v) = board.vias.iter().find(|v| v.id == id) {
                let _ = v;
                format!("layer:copper:front/vias/via:{id}")
            } else if let Some(z) = board.zones.iter().find(|z| z.id == id) {
                format!("layer:{0}/zones/zone:{id}", copper_key(z.layer))
            } else if board.texts.iter().any(|x| x.id == id) {
                format!("layer:silk/text/text:{id}")
            } else if board.cutouts.iter().any(|c| c.id == id) {
                format!("layer:edge/holes/cut:{id}")
            } else if board.holes.iter().any(|h| h.id == id) {
                format!("layer:edge/holes/hole:{id}")
            } else if board.footprints.iter().any(|f| f.id == id) {
                format!("parts/fp:{id}")
            } else {
                return None;
            }
        }
        TreeTab::Schematic => {
            if let Some(c) = project.component(id) {
                format!("sheet:{0}/comp:{id}", c.sheet)
            } else if let Some(l) = project.labels.iter().find(|l| l.id == id) {
                format!("nets/net:{0}/label:{id}", l.net)
            } else if let Some(w) = project.wires.iter().find(|w| w.id == id) {
                format!("nets/net:{0}/wire:{id}", w.net)
            } else {
                return None;
            }
        }
    };
    let parts: Vec<&str> = target.split('/').collect();
    let mut opens = vec!["root".to_owned()];
    for n in 1..parts.len() {
        opens.push(parts[..n].join("/"));
    }
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
    if board.texts.iter().any(|x| x.id == id) {
        return locked.contains("silk");
    }
    if board.cutouts.iter().any(|c| c.id == id) || board.holes.iter().any(|h| h.id == id) {
        return locked.contains("edge");
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
    Board,
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
    Sheet,
    Label,
    Plus,
    Search,
    Cross,
}

fn glyph_for(kind: RowKind) -> Glyph {
    match kind {
        RowKind::Root => Glyph::Board,
        RowKind::Layer | RowKind::Group => Glyph::Layer,
        RowKind::Part => Glyph::Part,
        RowKind::Pad => Glyph::Pad,
        RowKind::Net | RowKind::Wire => Glyph::Net,
        RowKind::Track => Glyph::Track,
        RowKind::Via => Glyph::Via,
        RowKind::Zone => Glyph::Zone,
        RowKind::Text => Glyph::Text,
        RowKind::Graphic | RowKind::Shape => Glyph::Graphic,
        RowKind::Hole => Glyph::Hole,
        RowKind::Sheet => Glyph::Sheet,
        RowKind::Label => Glyph::Label,
        RowKind::Add => Glyph::Plus,
    }
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
        Glyph::Board => {
            painter.rect_stroke(
                Rect::from_min_max(p(-6.0, -4.5), p(6.0, 4.5)),
                1.5,
                stroke,
                egui::StrokeKind::Inside,
            );
            painter.circle_filled(p(-3.5, -2.0), 0.9 * s, col);
            painter.circle_filled(p(3.5, 2.0), 0.9 * s, col);
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
        Glyph::Sheet => {
            poly(
                vec![p(-4.5, -6.0), p(2.0, -6.0), p(4.5, -3.5), p(4.5, 6.0), p(-4.5, 6.0)],
                true,
            );
            line(p(-2.0, 0.0), p(2.0, 0.0));
            line(p(-2.0, 3.0), p(2.0, 3.0));
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
        Glyph::Cross => {
            line(p(-4.0, -4.0), p(4.0, 4.0));
            line(p(-4.0, 4.0), p(4.0, -4.0));
        }
    }
}

/// A square icon button drawn with [`glyph`], with a tooltip and a name for
/// screen readers. Returns the response; `on` paints it as active.
fn icon_button(ui: &mut Ui, id: egui::Id, rect: Rect, g: Glyph, col: Color32, on: Option<Color32>, name: &str, tip: &str) -> egui::Response {
    let resp = ui.interact(rect, id, Sense::click());
    if resp.hovered() || resp.has_focus() {
        ui.painter()
            .rect_filled(rect.shrink(1.0), 4.0, col.gamma_multiply(0.16));
    }
    let shown = on.unwrap_or(col);
    glyph(ui.painter(), Rect::from_center_size(rect.center(), Vec2::splat(14.0)), g, shown);
    name_control(&resp, name, None);
    resp.on_hover_text(tip)
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

/// Draw the tree and return what the user did.
///
/// # Rules
/// - Rows are virtualized: only rows in view are laid out.
/// - Arrows move, Right expands, Left collapses, Space toggles visibility,
///   F focuses, Enter selects; keys work while a tree row has keyboard focus.
/// - A canvas selection that changed since last frame opens its ancestors and
///   scrolls its row into view.
///
/// # Dependencies
/// - `crates/ui-egui/src/lib.rs` `App::left_aside` calls it and applies the actions.
/// - `crates/ui-egui/src/a11y.rs` names the controls.
///
/// # Do not
/// - Mutate the project; return an [`Action`] instead.
pub(crate) fn show(ui: &mut Ui, env: &Env, view: &mut TreeView) -> Vec<Action> {
    let mut actions = Vec::new();
    let colours = env.colours;
    // Canvas selection reveals its row.
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
    search_header(ui, view, &colours);
    ui.add_space(4.0);
    let all = rows(env, view);
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
    if n <= 1 && !view.query.trim().is_empty() {
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new(t!("tree.no_match", query = view.query.trim()))
                .small()
                .color(colours.muted),
        );
    }
    actions
}

fn search_header(ui: &mut Ui, view: &mut TreeView, colours: &ThemeColours) {
    ui.horizontal(|ui| {
        let (icon_rect, _) = ui.allocate_exact_size(Vec2::splat(16.0), Sense::hover());
        glyph(ui.painter(), icon_rect, Glyph::Search, colours.muted);
        ui.add(
            crate::targets::text_field(&mut view.query)
                .hint_text(t!("tree.search"))
                .desired_width(ui.available_width() - 30.0),
        )
        .named(&t!("tree.search"))
        .on_hover_text(t!("tree.search.tip"));
        if !view.query.is_empty() {
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(ROW_HEIGHT), Sense::hover());
            let id = ui.id().with("tree-clear");
            if icon_button(ui, id, rect, Glyph::Cross, colours.muted, None, &t!("tree.clear"), &t!("tree.clear.tip")).clicked() {
                view.query.clear();
            }
        }
    });
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        for kind in KindFilter::ALL {
            let on = !view.kinds_off.contains(&kind);
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(ROW_HEIGHT), Sense::hover());
            let id = ui.id().with(("tree-kind", kind as u8));
            let col = if on { colours.ink } else { colours.muted.gamma_multiply(0.5) };
            let resp = ui.interact(rect, id, Sense::click());
            if on {
                ui.painter().rect_filled(rect.shrink(1.0), 4.0, colours.accent.gamma_multiply(0.22));
            } else if resp.hovered() {
                ui.painter().rect_filled(rect.shrink(1.0), 4.0, colours.hover);
            }
            glyph(ui.painter(), Rect::from_center_size(rect.center(), Vec2::splat(14.0)), kind.glyph(), col);
            name_control(&resp, &kind.name(), Some(on));
            if resp
                .on_hover_text(kind.tip())
                .clicked()
                && !view.kinds_off.remove(&kind)
            {
                view.kinds_off.insert(kind);
            }
        }
    });
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
    let selected = r.select.is_some() && r.select == env.selected
        || view.selected_key.as_deref() == Some(r.key.as_str());
    let expanded = view.expanded.contains(&r.key) || !view.query.trim().is_empty();
    let hidden_layer = r.layer.as_ref().is_some_and(|c| !visible(env, c));
    let locked = r.layer.as_ref().is_some_and(|c| view.locked.contains(&c.focus_key));
    let focused_layer = r
        .layer
        .as_ref()
        .is_some_and(|c| env.focus == Some(c.focus_key.as_str()));
    let painter = ui.painter();
    if selected {
        painter.rect_filled(rect, 0.0, colours.accent.gamma_multiply(0.20));
        painter.rect_filled(
            Rect::from_min_size(rect.min, Vec2::new(2.0, rect.height())),
            0.0,
            colours.accent,
        );
    } else if resp.hovered() {
        painter.rect_filled(rect, 0.0, colours.hover);
    }
    if focused {
        painter.rect_stroke(rect.shrink(1.0), 2.0, Stroke::new(1.5, colours.accent), egui::StrokeKind::Inside);
    }
    let left = rect.left() + 6.0;
    for d in 0..r.depth {
        let x = left + d as f32 * INDENT + INDENT / 2.0;
        painter.line_segment(
            [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
            Stroke::new(1.0, colours.muted.gamma_multiply(0.22)),
        );
    }
    let x0 = left + r.depth as f32 * INDENT;
    let cy = rect.center().y;
    let chevron_rect = Rect::from_center_size(Pos2::new(x0 + 7.0, cy), Vec2::splat(ROW_HEIGHT));
    let mut chevron_clicked = false;
    if r.expandable && r.kind != RowKind::Root {
        let cr = ui.interact(chevron_rect, id.with("chevron"), Sense::click());
        glyph(
            ui.painter(),
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
    let icon_rect = Rect::from_center_size(Pos2::new(x0 + 14.0 + 9.0, cy), Vec2::splat(14.0));
    let dim = if hidden_layer { 0.4 } else { 1.0 };
    if let Some(swatch) = r.swatch {
        let sw = Rect::from_center_size(icon_rect.center(), Vec2::splat(11.0));
        ui.painter().rect_filled(sw, 3.0, swatch.gamma_multiply(dim));
        ui.painter().rect_stroke(sw, 3.0, Stroke::new(1.0, colours.ink.gamma_multiply(0.35)), egui::StrokeKind::Inside);
    } else {
        glyph(
            ui.painter(),
            icon_rect,
            glyph_for(r.kind),
            if r.kind == RowKind::Add { colours.accent } else { colours.muted },
        );
    }
    // Right side controls for layer rows.
    let mut right = rect.right() - 4.0;
    if let Some(ctl) = &r.layer {
        let hovered = resp.hovered() || focused;
        let button = |ui: &mut Ui, slot: usize, key: &str| {
            let rect = Rect::from_min_size(
                Pos2::new(right - ROW_HEIGHT * (slot as f32 + 1.0), rect.top()),
                Vec2::splat(ROW_HEIGHT),
            );
            (rect, id.with(key))
        };
        let (eye_rect, eye_id) = button(ui, 0, "eye");
        let (lock_rect, lock_id) = button(ui, 1, "lock");
        let (focus_rect, focus_id) = button(ui, 2, "focus");
        let can_hide = ctl.visibility != Visibility::None;
        let eye_col = if hidden_layer { colours.muted.gamma_multiply(0.7) } else { colours.ink };
        let (shown_name, eye_tip) = if hidden_layer {
            (t!("tree.show", name = r.label), t!("tree.show.tip"))
        } else if can_hide {
            (t!("tree.hide", name = r.label), t!("tree.hide.tip"))
        } else {
            (t!("tree.hide", name = r.label), t!("tree.hide.none.tip"))
        };
        let eye = icon_button(
            ui,
            eye_id,
            eye_rect,
            if hidden_layer { Glyph::EyeOff } else { Glyph::Eye },
            if can_hide { eye_col } else { colours.muted.gamma_multiply(0.45) },
            None,
            &shown_name,
            &eye_tip,
        );
        if eye.clicked() && can_hide {
            match &ctl.visibility {
                Visibility::Canvas(name) => actions.push(Action::SetShown { name, on: hidden_layer }),
                Visibility::Art(layer) => actions.push(Action::SetArtShown { layer: *layer, on: hidden_layer }),
                Visibility::None => {}
            }
        }
        if locked || hovered {
            let lock = icon_button(
                ui,
                lock_id,
                lock_rect,
                if locked { Glyph::Lock } else { Glyph::Unlock },
                if locked { colours.warn } else { colours.muted },
                None,
                &if locked { t!("tree.unlock", name = r.label) } else { t!("tree.lock", name = r.label) },
                &if locked { t!("tree.unlock.tip") } else { t!("tree.lock.tip") },
            );
            if lock.clicked() && !view.locked.remove(&ctl.focus_key) {
                view.locked.insert(ctl.focus_key.clone());
            }
        }
        if focused_layer || hovered {
            let fo = icon_button(
                ui,
                focus_id,
                focus_rect,
                Glyph::Focus,
                if focused_layer { colours.accent } else { colours.muted },
                None,
                &if focused_layer { t!("tree.unfocus", name = r.label) } else { t!("tree.focus", name = r.label) },
                &if focused_layer { t!("tree.unfocus.tip") } else { t!("tree.focus.tip") },
            );
            if fo.clicked() {
                actions.push(Action::Focus(if focused_layer { None } else { Some(ctl.focus_key.clone()) }));
            }
        }
        right = focus_rect.left() - 2.0;
    } else {
        right -= 4.0;
    }
    // Label and detail.
    let label_left = x0 + 14.0 + 22.0;
    let body = ui.style().text_styles.get(&egui::TextStyle::Body).cloned().unwrap_or_else(|| FontId::proportional(13.0));
    let small = ui.style().text_styles.get(&egui::TextStyle::Small).cloned().unwrap_or_else(|| FontId::proportional(10.0));
    let ink = if hidden_layer { colours.muted } else { colours.ink };
    let font_body = FontId::new(body.size.min(13.5), body.family.clone());
    let clip = Rect::from_min_max(Pos2::new(label_left, rect.top()), Pos2::new(right, rect.bottom()));
    let label_galley = ui.painter().layout_no_wrap(r.label.clone(), font_body, if selected { colours.ink } else { ink });
    let label_w = label_galley.size().x;
    let p = ui.painter().with_clip_rect(clip);
    p.galley(Pos2::new(label_left, cy - label_galley.size().y / 2.0), label_galley, ink);
    if !r.detail.is_empty() {
        let dx = label_left + label_w + 8.0;
        if dx < right - 6.0 {
            p.text(
                Pos2::new(dx, cy),
                Align2::LEFT_CENTER,
                &r.detail,
                FontId::new(small.size.max(10.5), small.family.clone()),
                colours.muted,
            );
        }
    }
    if focused_layer {
        ui.painter().rect_filled(
            Rect::from_min_size(Pos2::new(rect.right() - 2.0, rect.top()), Vec2::new(2.0, rect.height())),
            0.0,
            colours.accent,
        );
    }
    // Row interaction.
    let state = if r.layer.is_some() {
        format!(
            "{}, {}, {}",
            r.label,
            if hidden_layer { t!("tree.state.hidden") } else { t!("tree.state.shown") },
            if locked { t!("tree.state.locked") } else { t!("tree.state.unlocked") }
        )
    } else if r.detail.is_empty() {
        r.label.clone()
    } else {
        format!("{}, {}", r.label, r.detail)
    };
    name_control(&resp, &state, Some(selected));
    let tip = match r.kind {
        RowKind::Root => t!("tree.root.tip"),
        RowKind::Layer => t!("tree.layer.tip"),
        RowKind::Add => t!("tree.add_layer.tip"),
        _ => t!("tree.row.tip"),
    };
    let resp = resp.on_hover_text(tip);
    let double = resp.double_clicked();
    if chevron_clicked || (double && r.expandable && r.kind != RowKind::Root) {
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
    match (r.kind, r.select) {
        (RowKind::Add, _) => actions.push(Action::NewArtLayer),
        (_, Some(id)) => {
            view.selected_key = None;
            actions.push(Action::Select(id));
        }
        _ => {
            view.selected_key = Some(r.key.clone());
            actions.push(Action::SelectKey(r.key.clone()));
            actions.push(Action::HighlightNet(r.net));
            if r.expandable && r.kind != RowKind::Root {
                toggle(view, &r.key);
            }
        }
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
    let mut go = |view: &mut TreeView, index: usize| {
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
        if row.expandable && view.expanded.contains(&row.key) && row.kind != RowKind::Root {
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
    } else if press(ui, egui::Key::F)
        && let Some(ctl) = layer_of(all, current)
    {
        let on = env.focus == Some(ctl.focus_key.as_str());
        actions.push(Action::Focus(if on { None } else { Some(ctl.focus_key) }));
    }
}

/// The controls of the layer row that contains row `index` (itself or an
/// ancestor), so Space and F work from any row of a layer.
fn layer_of(all: &[Row], index: usize) -> Option<LayerControls> {
    all[..=index]
        .iter()
        .rev()
        .take_while(|r| r.kind != RowKind::Root)
        .find(|r| r.layer.is_some() && r.depth == 1)
        .and_then(|r| r.layer.clone())
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
            revision: 1,
        }
    }

    #[test]
    fn the_board_is_the_root_and_layers_come_first() {
        let project = sample();
        let mut view = TreeView::default();
        let list = rows(&env(&project, &[]), &mut view);
        assert_eq!(list[0].kind, RowKind::Root);
        assert_eq!(list[1].key, "layer:copper:front");
        assert!(list.iter().any(|r| r.key == "parts"));
        assert!(list.iter().any(|r| r.key == "nets"));
    }

    #[test]
    fn only_expanded_branches_are_built() {
        let project = sample();
        let mut view = TreeView::default();
        let closed = rows(&env(&project, &[]), &mut view).len();
        view.expanded.insert("parts".to_owned());
        let open = rows(&env(&project, &[]), &mut view).len();
        assert_eq!(open, closed + project.board.footprints.len());
    }

    #[test]
    fn a_search_keeps_matches_and_their_ancestors() {
        let project = sample();
        let net = project.nets[0].name.clone();
        let mut view = TreeView::default();
        view.query = net.clone();
        let list = rows(&env(&project, &[]), &mut view);
        assert!(list.iter().any(|r| r.kind == RowKind::Net && r.label == net));
        assert!(list.len() < 40);
        view.query = "zzzz-no-such-thing".to_owned();
        assert_eq!(rows(&env(&project, &[]), &mut view).len(), 1);
    }

    #[test]
    fn a_part_selected_on_the_canvas_opens_its_row() {
        let project = sample();
        let id = project.board.footprints[0].id;
        let (opens, target) = reveal_path(&env(&project, &[]), id).expect("row");
        assert_eq!(target, format!("parts/fp:{id}"));
        assert!(opens.contains(&"parts".to_owned()));
        let mut view = TreeView::default();
        view.expanded.extend(opens);
        let list = rows(&env(&project, &[]), &mut view);
        assert!(list.iter().any(|r| r.key == target));
    }

    #[test]
    fn kind_filter_removes_a_branch() {
        let project = sample();
        let mut view = TreeView::default();
        view.kinds_off.insert(KindFilter::Parts);
        let list = rows(&env(&project, &[]), &mut view);
        assert!(!list.iter().any(|r| r.key == "parts"));
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
        view.expanded.insert("parts".to_owned());
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

impl crate::App {
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
    /// itself (size, stackup, finish, origin) or a net (its connection editor).
    ///
    /// # Dependencies
    /// - `crates/ui-egui/src/lib.rs` `stackup_section`, `holes_section`, `datums_section`,
    ///   `fiducials_section`, `via_spans_section` and `nets_tab` draw the sections.
    pub(crate) fn tree_row_properties(&mut self, ui: &mut Ui) {
        let Some(key) = self.tree.selected_key.clone() else {
            return;
        };
        let net = key
            .rsplit('/')
            .next()
            .and_then(|s| s.strip_prefix("net:"))
            .is_some()
            || key == "nets";
        if key == "root" && self.tab == crate::Tab::Pcb {
            ui.heading(t!("tree.board_settings"));
            ui.label(egui::RichText::new(t!("tree.board_settings.help")).small().color(self.muted()));
            ui.add_space(8.0);
            self.stackup_section(ui);
            ui.add_space(8.0);
            self.holes_section(ui);
            self.datums_section(ui);
            self.fiducials_section(ui);
            self.via_spans_section(ui);
        } else if net {
            ui.heading(t!("tree.net_settings"));
            self.nets_tab(ui);
        } else {
            ui.heading(
                self.tree
                    .cursor
                    .as_deref()
                    .and_then(|k| k.rsplit('/').next())
                    .map(|s| s.rsplit(':').next().unwrap_or(s).to_owned())
                    .unwrap_or_default(),
            );
            ui.label(egui::RichText::new(t!("tree.row_properties.help")).small().color(self.muted()));
        }
    }
}
