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
/// Height of one card row, without spacing.
const CARD_HEIGHT: f32 = 60.0;
/// Side of the drawing on a card.
const PREVIEW_SIDE: f32 = 56.0;
/// Installed results kept per request (the engine caps a page at 100).
const PAGE: usize = 100;

/// Category chips: the listing category name, its label key and tip key.
const CATEGORIES: [(&str, &str, &str); 6] = [
    (
        "resistor",
        "library.dock.cat.resistor",
        "library.dock.cat.resistor.tip",
    ),
    (
        "capacitor",
        "library.dock.cat.capacitor",
        "library.dock.cat.capacitor.tip",
    ),
    (
        "inductor",
        "library.dock.cat.inductor",
        "library.dock.cat.inductor.tip",
    ),
    (
        "diode",
        "library.dock.cat.diode",
        "library.dock.cat.diode.tip",
    ),
    ("led", "library.dock.cat.led", "library.dock.cat.led.tip"),
    (
        "connector",
        "library.dock.cat.connector",
        "library.dock.cat.connector.tip",
    ),
];

type Reply = Receiver<Result<Value, String>>;

/// One installed part as the listing reports it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Installed {
    pub(crate) path: String,
    pub(crate) name: String,
    pub(crate) description: String,
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
    /// Category chips switched on.
    pub(crate) categories: BTreeSet<String>,
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
            categories: BTreeSet::new(),
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
    pub(crate) fn library_dock_body(&mut self, ui: &mut egui::Ui) {
        let now = ui.input(|i| i.time);
        self.dock_poll(ui.ctx());
        ui.add_space(4.0);
        let search = ui
            .add(
                crate::targets::text_field(&mut self.part_search)
                    .hint_text(t!("library.browser.search_parts_by_name"))
                    .desired_width(f32::INFINITY),
            )
            .on_hover_text(t!("library.browser.search_parts_by_name.tip"));
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
        ui.add_space(6.0);
        self.dock_chips(ui);
        let query = self.part_search.trim().to_lowercase();
        let want_installed =
            !self.dock.hide_installed && (!query.is_empty() || !self.dock.categories.is_empty());
        let signature = if want_installed {
            format!("{query}|{:?}", self.dock.categories)
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
                    if self
                        .dock_chip(ui, tag, on)
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
            if matches && tagged {
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
        if !self.dock.hide_built_in && self.dock.tag.is_none() && self.dock.categories.is_empty() {
            for t in TEMPLATES {
                if query.is_empty() || finds(&query, t.label) || t.kind.contains(query.as_str()) {
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
        let height = (ui.available_height() * 0.55).max(CARD_HEIGHT * 3.0);
        egui::ScrollArea::vertical()
            .id_salt("library-dock-cards")
            .max_height(height)
            .auto_shrink([false, true])
            .show_rows(ui, CARD_HEIGHT, cards.len(), |ui, range| {
                for index in range {
                    if let Some(card) = cards.get(index) {
                        self.dock_card(ui, card);
                    }
                }
            });
        self.source_section(ui);
        if ui
            .button(t!("library.browser.browse_component_libraries"))
            .on_hover_text(t!("library.browser.browse_component_libraries.tip"))
            .clicked()
        {
            self.change_tab(Tab::Settings);
        }
    }

    /// A toggle chip at least as tall as the pointer-target minimum.
    fn dock_chip(&self, ui: &mut egui::Ui, text: &str, on: bool) -> egui::Response {
        ui.add(
            egui::Button::new(text)
                .selected(on)
                .min_size(egui::Vec2::splat(crate::targets::MIN_TARGET)),
        )
    }

    /// Source and category chips.
    fn dock_chips(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            for (hidden, label, tip) in [
                (
                    self.dock.hide_built_in,
                    t!("library.dock.built_in"),
                    t!("library.dock.built_in.tip"),
                ),
                (
                    self.dock.hide_project,
                    t!("library.dock.project"),
                    t!("library.dock.project.tip"),
                ),
                (
                    self.dock.hide_installed,
                    t!("library.dock.installed"),
                    t!("library.dock.installed.tip"),
                ),
            ] {
                if self
                    .dock_chip(ui, &label, !hidden)
                    .on_hover_text(tip)
                    .clicked()
                {
                    if label == t!("library.dock.built_in") {
                        self.dock.hide_built_in = !hidden;
                    } else if label == t!("library.dock.project") {
                        self.dock.hide_project = !hidden;
                    } else {
                        self.dock.hide_installed = !hidden;
                    }
                }
            }
        });
        ui.horizontal_wrapped(|ui| {
            for (name, label, tip) in CATEGORIES {
                let on = self.dock.categories.contains(name);
                if self
                    .dock_chip(ui, &pcbcraft_text::lookup(label, &[]), on)
                    .on_hover_text(pcbcraft_text::lookup(tip, &[]))
                    .clicked()
                {
                    if on {
                        self.dock.categories.remove(name);
                    } else {
                        self.dock.categories.insert(name.to_owned());
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
        if !self.dock.categories.is_empty() {
            filter["categories"] = json!(self.dock.categories);
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

    /// One part card: a drawing, a name that picks the part, and a note.
    fn dock_card(&mut self, ui: &mut egui::Ui, card: &Card) {
        let (key, paths, name, note) = match card {
            Card::BuiltIn(kind, label) => (
                format!("built-in/{kind}"),
                None,
                (*label).to_owned(),
                t!("library.dock.note.built_in"),
            ),
            Card::Project(d) => {
                let key = format!("project/{}/{}", d.name, d.graphics.len() + d.pins.len());
                if !self.dock.drawings.contains_key(&key) {
                    let paths = library::library_geometry_paths(Some(d), None, 1);
                    self.dock.drawings.insert(key.clone(), paths);
                }
                let tags = d
                    .keywords
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" · ");
                let note = match (d.description.trim(), tags.as_str()) {
                    ("", "") => t!("library.dock.note.project"),
                    ("", tags) => t!("schematic.part_card.search_words_tags", tags = tags),
                    (what, "") => what.to_owned(),
                    (what, tags) => t!(
                        "schematic.part_card.what_search_words_tags",
                        what = what,
                        tags = tags
                    ),
                };
                (key.clone(), Some(key), d.name.clone(), note)
            }
            Card::Installed(i) => {
                let key = format!("installed/{}/{}", i.path, i.name);
                let library = i
                    .path
                    .rsplit(['/', '\\'])
                    .next()
                    .unwrap_or(&i.path)
                    .trim_end_matches(".kicad_sym");
                let note = if i.description.trim().is_empty() {
                    t!("library.dock.note.installed", library = library)
                } else {
                    t!(
                        "library.dock.note.installed_what",
                        what = i.description.trim(),
                        library = library
                    )
                };
                (key.clone(), Some(key), i.name.clone(), note)
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
        let drawing = paths.and_then(|k| self.dock.drawings.get(&k).cloned());
        let mut picked = false;
        ui.horizontal(|ui| {
            let (rect, _) =
                ui.allocate_exact_size(egui::Vec2::splat(PREVIEW_SIDE), egui::Sense::hover());
            let painter = ui.painter_at(rect);
            painter.rect_filled(rect, 6.0, self.surface());
            match &drawing {
                Some(paths) if !paths.is_empty() => {
                    let (center, extent) = library::library_paths_extent(paths);
                    let camera = Camera {
                        center,
                        zoom: ((rect.width() - 10.0) / extent.x as f32)
                            .min((rect.height() - 10.0) / extent.y as f32)
                            .clamp(0.01, 40.0),
                    };
                    library::paint_library_paths(&painter, rect, camera, paths, self.accent());
                }
                _ => {
                    painter.text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        name.chars()
                            .next()
                            .unwrap_or('?')
                            .to_uppercase()
                            .to_string(),
                        egui::FontId::proportional(20.0),
                        self.muted(),
                    );
                }
            }
            ui.vertical(|ui| {
                if ui
                    .add(
                        egui::Button::new(&name)
                            .min_size(egui::Vec2::splat(crate::targets::MIN_TARGET)),
                    )
                    .on_hover_text(t!("library.dock.pick.tip"))
                    .clicked()
                {
                    picked = true;
                }
                ui.label(egui::RichText::new(note).small().color(self.muted()));
            });
        });
        if picked {
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
