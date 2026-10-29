//! The library browser in the left dock: one search box over built-in parts,
//! this project's library and installed libraries, filter chips, and a card
//! with a small drawing for every part.
//!
//! # Rules
//! - Installed libraries are read on a worker thread through `library.list`;
//!   a frame never waits for them. Typing is debounced and every request
//!   carries a sequence number, so an older answer never replaces a newer one.
//! - An empty search shows built-in and project parts only; installed
//!   libraries are listed once the person types or picks a category.
//! - Filter state lives in `App::dock`, not in the design.
//! - Hiding a set hides its parts from this list only; placed parts stay.
//!
//! # Dependencies
//! - crates/engine/src/library.rs#list_over
//! - crates/library/src/listing.rs
//! - crates/ui-egui/src/library.rs#library_geometry_paths
//! - crates/text/strings/en.txt

use super::*;
use pcbcraft_library::SymbolDefinition;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::mpsc::{Receiver, channel};

/// Seconds the search box must rest before installed libraries are asked.
const DEBOUNCE_SECONDS: f64 = 0.2;
/// Requests allowed in flight; older ones are dropped, never applied.
const MAX_INFLIGHT: usize = 4;
/// Most keyword chips shown.
const MAX_TAGS: usize = 12;
/// Installed results kept per request (the engine caps a page at 100).
const PAGE: usize = 100;

/// Kind toggles: the name kept in state, its label and tip catalog keys, and
/// the listing categories it stands for.
const KINDS: [(&str, &str, &str, &[&str]); 4] = [
    (
        "passive",
        "library.dock.kind.passive",
        "library.dock.kind.passive.tip",
        &[
            "resistor",
            "capacitor",
            "inductor",
            "diode",
            "led",
            "crystal",
        ],
    ),
    (
        "ic",
        "library.dock.kind.ic",
        "library.dock.kind.ic.tip",
        &[
            "ic_regulator",
            "ic_mcu",
            "ic_logic",
            "ic_analog",
            "transistor",
            "mosfet",
        ],
    ),
    (
        "connector",
        "library.dock.kind.connector",
        "library.dock.kind.connector.tip",
        &["connector"],
    ),
    (
        "mechanical",
        "library.dock.kind.mechanical",
        "library.dock.kind.mechanical.tip",
        &["mechanical", "module"],
    ),
];

/// Height of one card row, thumbnail included.
const ROW_HEIGHT: f32 = 88.0;
/// Side of the thumbnail on a card.
const THUMB: f32 = 72.0;

/// Pictures drawn on the filter toggles.
#[derive(Clone, Copy)]
enum Glyph {
    BuiltIn,
    Project,
    Installed,
    Online,
    Passive,
    Ic,
    Connector,
    Mechanical,
}

/// Draw a small line picture of `glyph` inside `rect`.
fn paint_glyph(painter: &egui::Painter, rect: Rect, glyph: Glyph, color: Color32) {
    let c = rect.center();
    let r = rect.width().min(rect.height()) * 0.3;
    let stroke = Stroke::new(1.5, color);
    let line = |a: egui::Pos2, b: egui::Pos2| {
        painter.line_segment([a, b], stroke);
    };
    let p = |x: f32, y: f32| egui::pos2(c.x + x * r, c.y + y * r);
    match glyph {
        Glyph::BuiltIn => {
            painter.rect_stroke(
                Rect::from_center_size(c, egui::vec2(r * 2.0, r * 2.0)),
                2.0,
                stroke,
                egui::StrokeKind::Inside,
            );
            painter.circle_filled(c, r * 0.35, color);
        }
        Glyph::Project => {
            line(p(-1.0, -0.1), p(0.0, -1.0));
            line(p(0.0, -1.0), p(1.0, -0.1));
            painter.rect_stroke(
                Rect::from_min_max(p(-0.7, -0.1), p(0.7, 1.0)),
                1.0,
                stroke,
                egui::StrokeKind::Inside,
            );
        }
        Glyph::Installed => {
            for y in [-0.8_f32, 0.0, 0.8] {
                line(p(-1.0, y), p(1.0, y));
            }
        }
        Glyph::Online => {
            painter.circle_stroke(c, r * 1.1, stroke);
            line(p(-1.1, 0.0), p(1.1, 0.0));
            line(p(0.0, -1.1), p(0.0, 1.1));
            painter.circle_stroke(c, r * 0.5, stroke);
        }
        Glyph::Passive => {
            let pts = [
                (-1.2, 0.0),
                (-0.7, 0.0),
                (-0.45, -0.8),
                (0.0, 0.8),
                (0.45, -0.8),
                (0.7, 0.0),
                (1.2, 0.0),
            ];
            for w in pts.windows(2) {
                line(p(w[0].0, w[0].1), p(w[1].0, w[1].1));
            }
        }
        Glyph::Ic => {
            painter.rect_stroke(
                Rect::from_min_max(p(-0.7, -0.8), p(0.7, 0.8)),
                1.0,
                stroke,
                egui::StrokeKind::Inside,
            );
            for y in [-0.4_f32, 0.4] {
                line(p(-1.2, y), p(-0.7, y));
                line(p(0.7, y), p(1.2, y));
            }
        }
        Glyph::Connector => {
            painter.circle_stroke(p(0.5, 0.0), r * 0.5, stroke);
            line(p(-1.2, 0.0), p(0.0, 0.0));
        }
        Glyph::Mechanical => {
            painter.circle_stroke(c, r * 0.55, stroke);
            for i in 0..6 {
                let a = i as f32 * std::f32::consts::TAU / 6.0;
                line(
                    p(a.cos() * 0.8, a.sin() * 0.8),
                    p(a.cos() * 1.2, a.sin() * 1.2),
                );
            }
        }
    }
}

/// A magnifier drawn inside `rect`.
fn paint_magnifier(painter: &egui::Painter, rect: Rect, color: Color32) {
    let c = rect.center();
    let r = rect.height() * 0.28;
    let stroke = Stroke::new(1.6, color);
    painter.circle_stroke(c - egui::vec2(r * 0.25, r * 0.25), r, stroke);
    painter.line_segment(
        [
            c + egui::vec2(r * 0.55, r * 0.55),
            c + egui::vec2(r * 1.5, r * 1.5),
        ],
        stroke,
    );
}

type Reply = Receiver<Result<Value, String>>;

/// One installed part as the listing reports it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Installed {
    pub(crate) path: String,
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) package: String,
    pub(crate) category: String,
}

/// Browser state: the filters, the last installed answer, and the drawings.
pub(crate) struct Dock {
    /// Seconds of silence before a request goes out; tests lower it.
    pub(crate) debounce: f64,
    /// Requests started so far.
    pub(crate) requests_sent: u64,
    /// Highest request number whose answer is on screen.
    pub(crate) applied: u64,
    pub(crate) items: Vec<Installed>,
    pub(crate) total: usize,
    pub(crate) error: String,
    /// Source chips switched off.
    pub(crate) hide_built_in: bool,
    pub(crate) hide_project: bool,
    pub(crate) hide_installed: bool,
    pub(crate) hide_online: bool,
    /// Kind toggles switched on (`passive`, `ic`, `connector`, `mechanical`).
    pub(crate) kinds: BTreeSet<String>,
    /// Keyword chip switched on.
    pub(crate) tag: Option<String>,
    /// Folders to read instead of the user's installed ones (tests).
    pub(crate) roots: Option<Vec<std::path::PathBuf>>,
    sent: String,
    wanted: String,
    changed_at: f64,
    inflight: Vec<(u64, Reply)>,
    drawings: BTreeMap<String, Vec<Vec<Point>>>,
    drawing_request: Option<(String, Reply)>,
    placing: Option<(String, String, Reply)>,
}

impl Default for Dock {
    fn default() -> Self {
        Self {
            debounce: DEBOUNCE_SECONDS,
            requests_sent: 0,
            applied: 0,
            items: Vec::new(),
            total: 0,
            error: String::new(),
            hide_built_in: false,
            hide_project: false,
            hide_installed: false,
            hide_online: false,
            kinds: BTreeSet::new(),
            tag: None,
            roots: None,
            sent: String::new(),
            wanted: String::new(),
            changed_at: 0.0,
            inflight: Vec::new(),
            drawings: BTreeMap::new(),
            drawing_request: None,
            placing: None,
        }
    }
}

/// A row of the visible list.
#[derive(Clone)]
enum Card {
    BuiltIn(&'static str, &'static str),
    Project(Box<SymbolDefinition>),
    Installed(Installed),
}

/// Run `work` off the frame and hand back where its answer will arrive. In
/// the browser build there are no threads, so it runs at once.
fn spawn_read(
    ctx: &egui::Context,
    work: impl FnOnce() -> Result<Value, String> + Send + 'static,
) -> Reply {
    let (tx, rx) = channel();
    #[cfg(not(target_arch = "wasm32"))]
    {
        let ctx = ctx.clone();
        let failed = tx.clone();
        if std::thread::Builder::new()
            .name("pcbcraft-dock".into())
            .spawn(move || {
                let _ = tx.send(work());
                ctx.request_repaint();
            })
            .is_err()
        {
            let _ = failed.send(Err("the reader could not start".into()));
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = ctx;
        let _ = tx.send(work());
    }
    rx
}

/// The words of keyword lists across `definitions`, most frequent first.
fn top_tags(definitions: &[SymbolDefinition]) -> Vec<String> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for d in definitions {
        for word in d.keywords.split_whitespace() {
            if word.len() > 1 {
                *counts.entry(word.to_lowercase()).or_default() += 1;
            }
        }
    }
    let mut ranked: Vec<(String, usize)> = counts.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    ranked.into_iter().take(MAX_TAGS).map(|(w, _)| w).collect()
}

/// True when the query finds this text: a plain substring or letters in order.
fn finds(query: &str, text: &str) -> bool {
    let text = text.to_lowercase();
    text.contains(query) || fuzzy_match(query, &text)
}

/// The listing category name the library's own table gives this text.
fn category_name(source: &str, text: &str) -> String {
    serde_json::to_value(pcbcraft_library::listing::category_from(source, text))
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

impl App {
    /// The whole left-dock browser body: search, chips, sets, cards, sources.
    ///
    /// # Rules
    /// - Never waits for a library; see the file header.
    /// - Picking a card arms the placement tool and shows the schematic.
    ///
    /// # Dependencies
    /// - crates/ui-egui/src/library.rs#library_geometry_paths
    ///
    /// # Do not
    /// - Call `pcbcraft_engine::library` on the frame thread.
    pub(crate) fn library_dock_body(&mut self, ui: &mut egui::Ui, viewport: Rect) {
        let now = ui.input(|i| i.time);
        self.dock_poll(ui.ctx());
        let field = egui::Margin {
            left: 32,
            right: 8,
            top: 6,
            bottom: 6,
        };
        let search = ui
            .add(
                crate::targets::text_field(&mut self.part_search)
                    .hint_text(t!("library.browser.search_parts_by_name"))
                    .margin(field)
                    .desired_width(f32::INFINITY),
            )
            .on_hover_text(t!("library.browser.search_parts_by_name.tip"));
        paint_magnifier(
            ui.painter(),
            Rect::from_min_size(search.rect.min, egui::vec2(32.0, search.rect.height())),
            self.muted(),
        );
        let enabled = search.enabled();
        search.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::WidgetType::TextEdit,
                enabled,
                t!("library.browser.search_parts_by_name"),
            )
        });
        if self.part_search_focus {
            search.request_focus();
            self.part_search_focus = false;
        }
        ui.add_space(8.0);
        self.dock_chips(ui);
        ui.add_space(4.0);
        let query = self.part_search.trim().to_lowercase();
        let want_installed =
            !self.dock.hide_installed && (!query.is_empty() || !self.dock.kinds.is_empty());
        let signature = if want_installed {
            format!("{query}|{:?}", self.dock.kinds)
        } else {
            String::new()
        };
        if signature != self.dock.wanted {
            self.dock.wanted = signature.clone();
            self.dock.changed_at = now;
        }
        if signature != self.dock.sent {
            if now - self.dock.changed_at >= self.dock.debounce {
                self.dock_request(ui.ctx(), &query, want_installed);
                self.dock.sent = signature;
            } else {
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_secs_f64(self.dock.debounce));
            }
        }
        let project_parts: Vec<SymbolDefinition> = if self.dock.hide_project {
            Vec::new()
        } else {
            self.project.library.symbols.clone()
        };
        let tags = top_tags(&project_parts);
        if !tags.is_empty() {
            ui.horizontal_wrapped(|ui| {
                for tag in &tags {
                    let on = self.dock.tag.as_deref() == Some(tag.as_str());
                    if ui
                        .add(
                            egui::Button::new(tag)
                                .selected(on)
                                .min_size(egui::Vec2::splat(crate::targets::MIN_TARGET)),
                        )
                        .on_hover_text(t!("library.dock.tag.tip", tag = tag))
                        .clicked()
                    {
                        self.dock.tag = if on { None } else { Some(tag.clone()) };
                    }
                }
            });
        }
        let mut cards: Vec<Card> = Vec::new();
        let mut sets: BTreeMap<String, usize> = BTreeMap::new();
        for d in &project_parts {
            let matches = query.is_empty()
                || finds(&query, &d.name)
                || d.keywords.to_lowercase().contains(query.as_str())
                || d.set.to_lowercase().contains(query.as_str());
            let tagged = self
                .dock
                .tag
                .as_ref()
                .is_none_or(|t| d.keywords.to_lowercase().split_whitespace().any(|w| w == t));
            let kind_ok =
                self.kind_allows(&category_name("", &format!("{} {}", d.keywords, d.name)));
            if matches && tagged && kind_ok {
                let set = d.set.trim().to_owned();
                if !set.is_empty() {
                    *sets.entry(set.clone()).or_default() += 1;
                }
                if set.is_empty() || !self.hidden_sets.contains(&set) {
                    cards.push(Card::Project(Box::new(d.clone())));
                }
            }
        }
        if !self.dock.hide_installed && self.dock.tag.is_none() {
            for item in &self.dock.items {
                cards.push(Card::Installed(item.clone()));
            }
        }
        if !self.dock.hide_built_in && self.dock.tag.is_none() {
            for t in TEMPLATES {
                let kind_ok = self.kind_allows(&category_name("", t.kind));
                if kind_ok
                    && (query.is_empty()
                        || finds(&query, t.label)
                        || t.kind.contains(query.as_str()))
                {
                    cards.push(Card::BuiltIn(t.kind, t.label));
                }
            }
        }
        self.dock_sets(ui, &sets);
        if want_installed && !self.dock.inflight.is_empty() {
            ui.label(
                egui::RichText::new(t!("library.dock.searching"))
                    .small()
                    .color(self.muted()),
            );
        } else if !self.dock.error.is_empty() {
            ui.label(
                egui::RichText::new(self.dock.error.clone())
                    .small()
                    .color(self.muted()),
            );
        } else if self.dock.total > self.dock.items.len() && want_installed {
            ui.label(
                egui::RichText::new(t!(
                    "library.dock.more",
                    shown = self.dock.items.len(),
                    total = self.dock.total
                ))
                .small()
                .color(self.muted()),
            );
        } else if !want_installed && !self.dock.hide_installed {
            ui.label(
                egui::RichText::new(t!("library.dock.type_for_installed"))
                    .small()
                    .color(self.muted()),
            );
        }
        if cards.is_empty() {
            ui.label(
                egui::RichText::new(t!(
                    "library.browser.nothing_here_matches_trim_yet",
                    trim = self.part_search.trim()
                ))
                .small()
                .color(self.muted()),
            );
        }
        let top = ui.cursor().min.y - ui.min_rect().min.y;
        let first =
            (((viewport.min.y - top) / ROW_HEIGHT).floor().max(0.0) as usize).min(cards.len());
        let last =
            (((viewport.max.y - top) / ROW_HEIGHT).ceil().max(0.0) as usize + 1).min(cards.len());
        ui.spacing_mut().item_spacing.y = 0.0;
        ui.add_space(first as f32 * ROW_HEIGHT);
        for card in &cards[first..last] {
            self.dock_card(ui, card);
        }
        ui.add_space((cards.len() - last) as f32 * ROW_HEIGHT);
        ui.spacing_mut().item_spacing.y = 4.0;
        if !self.dock.hide_online {
            self.source_section(ui);
        }
        if ui
            .button(t!("library.browser.browse_component_libraries"))
            .on_hover_text(t!("library.browser.browse_component_libraries.tip"))
            .clicked()
        {
            self.change_tab(Tab::Settings);
        }
    }

    /// True when no kind toggle is on, or this listing category is in one.
    fn kind_allows(&self, category: &str) -> bool {
        self.dock.kinds.is_empty()
            || KINDS
                .iter()
                .any(|(k, .., cats)| self.dock.kinds.contains(*k) && cats.contains(&category))
    }

    /// One toggle button showing a small picture; returns true when clicked.
    fn dock_toggle(
        &self,
        ui: &mut egui::Ui,
        glyph: Glyph,
        on: bool,
        name: &str,
        tip: &str,
    ) -> bool {
        let (rect, response) = ui.allocate_exact_size(
            egui::Vec2::splat(crate::targets::MIN_TARGET + 4.0),
            egui::Sense::click(),
        );
        let enabled = response.enabled();
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::WidgetType::Button, enabled, on, name)
        });
        let response = response.on_hover_text(tip);
        let painter = ui.painter();
        if on {
            painter.rect_filled(rect, 5.0, self.accent().gamma_multiply(0.28));
        } else if response.hovered() {
            painter.rect_filled(rect, 5.0, ui.visuals().widgets.hovered.weak_bg_fill);
        }
        let color = if on { self.accent() } else { self.muted() };
        paint_glyph(painter, rect, glyph, color);
        response.clicked()
    }

    /// One row of picture toggles: sources, then kinds.
    fn dock_chips(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let sources = [
                (
                    Glyph::BuiltIn,
                    self.dock.hide_built_in,
                    "library.dock.built_in",
                    "library.dock.built_in.tip",
                ),
                (
                    Glyph::Project,
                    self.dock.hide_project,
                    "library.dock.project",
                    "library.dock.project.tip",
                ),
                (
                    Glyph::Installed,
                    self.dock.hide_installed,
                    "library.dock.installed",
                    "library.dock.installed.tip",
                ),
                (
                    Glyph::Online,
                    self.dock.hide_online,
                    "library.dock.online",
                    "library.dock.online.tip",
                ),
            ];
            for (i, (glyph, hidden, label, tip)) in sources.into_iter().enumerate() {
                if self.dock_toggle(
                    ui,
                    glyph,
                    !hidden,
                    &pcbcraft_text::lookup(label, &[]),
                    &pcbcraft_text::lookup(tip, &[]),
                ) {
                    match i {
                        0 => self.dock.hide_built_in = !hidden,
                        1 => self.dock.hide_project = !hidden,
                        2 => self.dock.hide_installed = !hidden,
                        _ => self.dock.hide_online = !hidden,
                    }
                }
            }
            ui.add_space(8.0);
            let glyphs = [
                Glyph::Passive,
                Glyph::Ic,
                Glyph::Connector,
                Glyph::Mechanical,
            ];
            for ((name, label, tip, _), glyph) in KINDS.iter().zip(glyphs) {
                let on = self.dock.kinds.contains(*name);
                if self.dock_toggle(
                    ui,
                    glyph,
                    on,
                    &pcbcraft_text::lookup(label, &[]),
                    &pcbcraft_text::lookup(tip, &[]),
                ) {
                    if on {
                        self.dock.kinds.remove(*name);
                    } else {
                        self.dock.kinds.insert((*name).to_owned());
                    }
                }
            }
        });
    }

    /// One tickbox per set of the project's parts.
    fn dock_sets(&mut self, ui: &mut egui::Ui, sets: &BTreeMap<String, usize>) {
        for (set, count) in sets {
            let mut shown = !self.hidden_sets.contains(set);
            let row = ui
                .checkbox(
                    &mut shown,
                    egui::RichText::new(t!(
                        "library.browser.set_len_part_v1",
                        set = set,
                        len = count,
                        v1 = if *count == 1 { "" } else { "s" }
                    ))
                    .strong(),
                )
                .on_hover_text(t!("library.browser.set_len_part_v1.tip"));
            if row.changed() {
                if shown {
                    self.hidden_sets.retain(|h| h != set);
                } else {
                    self.hidden_sets.push(set.clone());
                }
            }
        }
    }

    /// Start a `library.list` read for installed parts, or clear the answer
    /// when installed parts are not wanted any more.
    fn dock_request(&mut self, ctx: &egui::Context, query: &str, want_installed: bool) {
        self.dock.requests_sent += 1;
        let seq = self.dock.requests_sent;
        if !want_installed {
            self.dock.items.clear();
            self.dock.total = 0;
            self.dock.error.clear();
            self.dock.applied = seq;
            self.dock.inflight.clear();
            return;
        }
        let mut filter = json!({"sources": ["installed"]});
        if !query.is_empty() {
            filter["text"] = json!(query);
        }
        if !self.dock.kinds.is_empty() {
            let wanted: Vec<&str> = KINDS
                .iter()
                .filter(|(k, ..)| self.dock.kinds.contains(*k))
                .flat_map(|(.., cats)| cats.iter().copied())
                .collect();
            filter["categories"] = json!(wanted);
        }
        let args = json!({"filter": filter, "limit": PAGE});
        let project = self.project.clone();
        let roots = self.dock.roots.clone();
        let reply = spawn_read(ctx, move || match roots {
            Some(roots) => pcbcraft_engine::library::list_over(&args, Some(&project), roots),
            None => pcbcraft_engine::library::list_with_project(&args, Some(&project)),
        });
        self.dock.inflight.push((seq, reply));
        while self.dock.inflight.len() > MAX_INFLIGHT {
            self.dock.inflight.remove(0);
        }
    }

    /// Take in finished reads: a list answer only when it is newer than what
    /// is shown, a drawing for the cache, a part ready to place.
    fn dock_poll(&mut self, ctx: &egui::Context) {
        let mut done: Vec<(u64, Result<Value, String>)> = Vec::new();
        self.dock.inflight.retain(|(seq, rx)| match rx.try_recv() {
            Ok(result) => {
                done.push((*seq, result));
                false
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => true,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => false,
        });
        done.sort_by_key(|(seq, _)| *seq);
        for (seq, result) in done {
            if seq <= self.dock.applied {
                continue;
            }
            self.dock.applied = seq;
            match result {
                Ok(value) => {
                    self.dock.error.clear();
                    self.dock.total = value
                        .get("total_known")
                        .and_then(Value::as_u64)
                        .and_then(|n| usize::try_from(n).ok())
                        .unwrap_or(0);
                    self.dock.items = value
                        .get("items")
                        .and_then(Value::as_array)
                        .map(|all| {
                            all.iter()
                                .filter_map(|i| {
                                    Some(Installed {
                                        path: i.get("source_id")?.as_str()?.to_owned(),
                                        name: i.get("source_ref")?.as_str()?.to_owned(),
                                        description: i
                                            .get("description")
                                            .and_then(Value::as_str)
                                            .unwrap_or_default()
                                            .to_owned(),
                                        package: i
                                            .get("package_text")
                                            .and_then(Value::as_str)
                                            .unwrap_or_default()
                                            .to_owned(),
                                        category: i
                                            .get("category")
                                            .and_then(Value::as_str)
                                            .unwrap_or_default()
                                            .to_owned(),
                                    })
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                }
                Err(e) => {
                    self.dock.items.clear();
                    self.dock.total = 0;
                    self.dock.error = e;
                }
            }
        }
        if let Some((key, rx)) = &self.dock.drawing_request {
            match rx.try_recv() {
                Ok(result) => {
                    let paths = result
                        .ok()
                        .and_then(|v| v.get("definition").cloned())
                        .and_then(|d| serde_json::from_value::<SymbolDefinition>(d).ok())
                        .map(|d| library::library_geometry_paths(Some(&d), None, 1))
                        .unwrap_or_default();
                    self.dock.drawings.insert(key.clone(), paths);
                    self.dock.drawing_request = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.dock.drawing_request = None;
                }
            }
        }
        if let Some((path, name, rx)) = &self.dock.placing {
            match rx.try_recv() {
                Ok(result) => {
                    let (path, name) = (path.clone(), name.clone());
                    self.dock.placing = None;
                    match result
                        .ok()
                        .and_then(|v| v.get("definition").cloned())
                        .and_then(|d| serde_json::from_value::<SymbolDefinition>(d).ok())
                    {
                        Some(def) => {
                            self.change_tab(Tab::Schematic);
                            self.tool = Tool::LibraryPlace(path, name, Box::new(def));
                        }
                        None => self.message = t!("library.dock.could_not_read", name = name),
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(100));
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.dock.placing = None,
            }
        }
    }

    /// One part card: a thumbnail, the name, a two-line description, the
    /// source in the accent colour and a muted line of facts. The whole card
    /// picks the part; hover and the armed part are shown.
    fn dock_card(&mut self, ui: &mut egui::Ui, card: &Card) {
        let (key, drawn, name, what, source, meta, armed) = match card {
            Card::BuiltIn(kind, label) => (
                format!("built-in/{kind}"),
                false,
                (*label).to_owned(),
                String::new(),
                t!("library.dock.note.built_in"),
                String::new(),
                matches!(&self.tool, Tool::Place(k) if k == kind),
            ),
            Card::Project(d) => {
                let key = format!("project/{}/{}", d.name, d.graphics.len() + d.pins.len());
                if !self.dock.drawings.contains_key(&key) {
                    let paths = library::library_geometry_paths(Some(d), None, 1);
                    self.dock.drawings.insert(key.clone(), paths);
                }
                let mut facts: Vec<String> = Vec::new();
                if !d.footprint.trim().is_empty() {
                    facts.push(d.footprint.trim().to_owned());
                }
                if !d.pins.is_empty() {
                    facts.push(t!("library.dock.pins", count = d.pins.len()));
                }
                let source = if d.set.trim().is_empty() {
                    t!("library.dock.note.project")
                } else {
                    d.set.trim().to_owned()
                };
                (
                    key,
                    true,
                    d.name.clone(),
                    d.description.trim().to_owned(),
                    source,
                    facts.join(" · "),
                    matches!(&self.tool, Tool::CustomPlace(n, _) if *n == d.name),
                )
            }
            Card::Installed(i) => {
                let library = i
                    .path
                    .rsplit(['/', '\\'])
                    .next()
                    .unwrap_or(&i.path)
                    .trim_end_matches(".kicad_sym")
                    .to_owned();
                (
                    format!("installed/{}/{}", i.path, i.name),
                    true,
                    i.name.clone(),
                    i.description.trim().to_owned(),
                    library,
                    i.package.clone(),
                    matches!(&self.tool, Tool::LibraryPlace(p, n, _) if *p == i.path && *n == i.name),
                )
            }
        };
        if let Card::Installed(i) = card
            && !self.dock.drawings.contains_key(&key)
            && self.dock.drawing_request.is_none()
        {
            let (path, name) = (i.path.clone(), i.name.clone());
            let reply = spawn_read(ui.ctx(), move || {
                pcbcraft_engine::library::read_tool(
                    "library.inspect_symbol",
                    &json!({"path": path, "name": name}),
                )
            });
            self.dock.drawing_request = Some((key.clone(), reply));
        }
        let paths = if drawn {
            self.dock.drawings.get(&key).cloned()
        } else {
            None
        };
        let width = ui.available_width();
        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(width, ROW_HEIGHT), egui::Sense::click());
        let enabled = response.enabled();
        let label = format!("{name}, {source}");
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::WidgetType::Button, enabled, armed, label.clone())
        });
        let response = response.on_hover_text(t!("library.dock.pick.tip"));
        let painter = ui.painter_at(rect);
        let accent = self.accent();
        if armed {
            painter.rect_filled(rect, 0.0, accent.gamma_multiply(0.16));
            painter.rect_filled(
                Rect::from_min_size(rect.min, egui::vec2(3.0, rect.height())),
                0.0,
                accent,
            );
        } else if response.hovered() {
            painter.rect_filled(rect, 0.0, ui.visuals().widgets.hovered.weak_bg_fill);
        }
        painter.hline(
            rect.x_range(),
            rect.bottom(),
            Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
        );
        let thumb = Rect::from_min_size(rect.min + egui::vec2(12.0, 8.0), egui::Vec2::splat(THUMB));
        painter.rect_filled(thumb, 6.0, self.surface());
        match &paths {
            Some(paths) if !paths.is_empty() => {
                let (center, extent) = library::library_paths_extent(paths);
                let camera = Camera {
                    center,
                    zoom: ((thumb.width() - 12.0) / extent.x as f32)
                        .min((thumb.height() - 12.0) / extent.y as f32)
                        .clamp(0.01, 40.0),
                };
                library::paint_library_paths(&painter, thumb, camera, paths, accent);
            }
            _ => {
                painter.text(
                    thumb.center(),
                    egui::Align2::CENTER_CENTER,
                    name.chars()
                        .next()
                        .unwrap_or('?')
                        .to_uppercase()
                        .to_string(),
                    egui::FontId::proportional(22.0),
                    self.muted(),
                );
            }
        }
        let text_left = thumb.right() + 12.0;
        let text_width = (rect.right() - 12.0 - text_left).max(40.0);
        let mut y = rect.top() + 8.0;
        let title = painter.layout(
            name.clone(),
            egui::FontId::proportional(13.0),
            ui.visuals().strong_text_color(),
            text_width,
        );
        let title_height = title.size().y;
        painter.galley(egui::pos2(text_left, y), title, Color32::WHITE);
        y += title_height + 2.0;
        if !what.is_empty() {
            let mut job = egui::text::LayoutJob::simple(
                what,
                egui::FontId::proportional(12.0),
                self.muted(),
                text_width,
            );
            job.wrap.max_rows = 2;
            job.wrap.break_anywhere = false;
            job.wrap.overflow_character = Some('\u{2026}');
            let galley = painter.layout_job(job);
            let h = galley.size().y;
            painter.galley(egui::pos2(text_left, y), galley, Color32::WHITE);
            y += h + 2.0;
        }
        let source_galley =
            painter.layout(source, egui::FontId::proportional(11.0), accent, text_width);
        let h = source_galley.size().y;
        painter.galley(egui::pos2(text_left, y), source_galley, Color32::WHITE);
        y += h + 1.0;
        if !meta.is_empty() && y + 12.0 < rect.bottom() {
            let galley = painter.layout(
                meta,
                egui::FontId::proportional(11.0),
                self.muted(),
                text_width,
            );
            painter.galley(egui::pos2(text_left, y), galley, Color32::WHITE);
        }
        if response.clicked() || response.double_clicked() {
            match card {
                Card::BuiltIn(kind, _) => {
                    self.change_tab(Tab::Schematic);
                    self.tool = Tool::Place((*kind).into());
                }
                Card::Project(d) => {
                    self.change_tab(Tab::Schematic);
                    self.tool = Tool::CustomPlace(d.name.clone(), d.clone());
                }
                Card::Installed(i) => {
                    let (path, name) = (i.path.clone(), i.name.clone());
                    let (p, n) = (path.clone(), name.clone());
                    let reply = spawn_read(ui.ctx(), move || {
                        pcbcraft_engine::library::read_tool(
                            "library.inspect_symbol",
                            &json!({"path": p, "name": n}),
                        )
                    });
                    self.dock.placing = Some((path, name, reply));
                }
            }
        }
    }
}
