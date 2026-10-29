//! Part sources in the library list: remote results appear beside local parts
//! as cards with a source badge, stock, a price break and the source's credit.
use super::*;
use pcbcraft_library::sources::{FixtureSource, PartResult, PartSource, ResponseCache};

const SAMPLE: &str = include_str!("../../library/tests/fixtures/sources/sample.json");

#[derive(Default)]
pub(super) struct PartSources {
    source: Option<Box<dyn PartSource>>,
    cache: ResponseCache,
    notice: String,
}

fn now_secs() -> u64 {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }
    #[cfg(target_arch = "wasm32")]
    {
        0
    }
}

/// One plain line about a result's stock.
pub(super) fn stock_line(part: &PartResult) -> String {
    let stock = if part.stock == 0 {
        "out of stock".to_string()
    } else {
        format!("{} in stock", part.stock)
    };
    let price = part.price_breaks.first().map(|b| {
        format!(
            "from {:.2} {} each at {}",
            b.unit_price, b.currency, b.quantity
        )
    });
    match price {
        Some(price) => format!("{stock} · {price}"),
        None => stock,
    }
}

impl App {
    /// The remote half of the part list. Local parts are drawn by the caller.
    /// The online part source behind the Online toggle: the sample source
    /// until a live one is wired in, asked for the typed query. Returns the
    /// results with the source's credit line, and draws the notes a person
    /// needs when there are none (source off, nothing typed, error).
    ///
    /// # Rules
    /// - Reads go through the cached source; nothing is fetched without a query.
    /// - In the browser build part sources are not available.
    pub(super) fn online_parts(&mut self, ui: &mut egui::Ui) -> Vec<(PartResult, String)> {
        let muted = self.muted();
        let note = |ui: &mut egui::Ui, text: &str| {
            ui.label(egui::RichText::new(text).small().color(muted));
        };
        #[cfg(target_arch = "wasm32")]
        {
            note(ui, "Part sources are available in the desktop app.");
            Vec::new()
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            if self.part_sources.source.is_none() {
                note(
                    ui,
                    "Part sources are off. Only parts on this computer are listed.",
                );
                if ui
                    .button("Try sample source")
                    .on_hover_text(
                        "Turn on a small built-in set of sample parts, to see how a part source looks. No account needed.",
                    )
                    .clicked()
                {
                    match FixtureSource::from_json(SAMPLE) {
                        Ok(source) => self.part_sources.source = Some(Box::new(source)),
                        Err(e) => self.part_sources.notice = e.to_string(),
                    }
                }
                if !self.part_sources.notice.is_empty() {
                    ui.label(egui::RichText::new(&self.part_sources.notice).small());
                }
                return Vec::new();
            }
            let query = self.part_search.trim().to_owned();
            if query.is_empty() {
                note(ui, "Type a part number or keyword to search the sources.");
                return Vec::new();
            }
            let Some(source) = self.part_sources.source.as_deref() else {
                return Vec::new();
            };
            let found = self.part_sources.cache.search(source, &query, now_secs());
            let name = source.name().to_owned();
            let credit = source.attribution().text;
            match found {
                Err(e) => {
                    ui.label(egui::RichText::new(e.to_string()).small());
                    Vec::new()
                }
                Ok(parts) if parts.is_empty() => {
                    note(ui, &format!("{name} has nothing for \"{query}\"."));
                    Vec::new()
                }
                Ok(parts) => parts.into_iter().map(|p| (p, credit.clone())).collect(),
            }
        }
    }
    /// After a source part is placed, write its maker and numbers onto the
    /// new component in one step.
    pub(super) fn attach_pending_source(&mut self, component: Id) {
        let Some(part) = self.pending_source.take() else {
            return;
        };
        if !matches!(self.tool, Tool::Place(_)) {
            return;
        }
        self.run(
            "part.attach_source",
            json!({"id":component,"manufacturer":part.manufacturer,"mpn":part.mpn,
                   "datasheet":part.datasheet_url,"distributor":part.distributor,"sku":part.sku}),
        );
    }
}
