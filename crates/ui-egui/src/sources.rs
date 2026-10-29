//! Part sources in the library list: remote results appear beside local parts
//! as cards with a source badge, stock, a price break and the source's credit.
//! The one source today is the open library, behind `OpenLibrarySource`: it
//! lists parts (`PartSource`) and imports them (`CadSource`).
//!
//! # Rules
//! - Runtime reads are cache-only until the live transport lands: `CacheOnly`
//!   never touches the network. Tests inject a recorded fetcher through
//!   `App::use_open_source`.
//!
//! # Dependencies
//! - crates/library/src/open_library.rs
//! - crates/library/src/cad_cache.rs
//! - crates/library/src/sources.rs
use super::*;
use pcbcraft_library::cad::CadSource;
use pcbcraft_library::cad_cache::{CadCache, default_cache_dir};
use pcbcraft_library::open_library::{FetchError, Fetcher, OpenLibrarySource};
use pcbcraft_library::sources::{PartResult, PartSource, ResponseCache};
use std::sync::Arc;

/// The transport used until the live one lands: it answers nothing, so only
/// files already kept on this computer are served.
///
/// Contract: crates/library/src/open_library.rs (Fetcher)
pub(super) struct CacheOnly;

impl Fetcher for CacheOnly {
    fn get(&self, _url: &str) -> Result<Vec<u8>, FetchError> {
        Err(FetchError::Offline(
            "Online parts are read from this computer only for now.".into(),
        ))
    }
}

#[derive(Default)]
pub(super) struct PartSources {
    source: Option<Arc<OpenLibrarySource>>,
    cache: ResponseCache,
    notice: String,
}

impl PartSources {
    /// The open source, when it is switched on.
    pub(super) fn open(&self) -> Option<Arc<OpenLibrarySource>> {
        self.source.clone()
    }
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
                    .button("Turn on Open library")
                    .on_hover_text(
                        "Search the open symbol and footprint libraries. Parts you have opened before work offline. No account needed.",
                    )
                    .clicked()
                {
                    self.use_open_source(Box::new(CacheOnly), None);
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
            let Some(source) = self.part_sources.source.clone() else {
                return Vec::new();
            };
            let found = self
                .part_sources
                .cache
                .search(source.as_ref(), &query, now_secs());
            let name = PartSource::name(source.as_ref()).to_owned();
            let credit = format!(
                "{}. {}",
                source.license().name,
                CadSource::attribution(source.as_ref()).text
            );
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
    /// Switch the open source on over `fetcher`, keeping files in `cache_dir`
    /// (the default folder when `None`, memory when there is none).
    pub(super) fn use_open_source(
        &mut self,
        fetcher: Box<dyn Fetcher>,
        cache_dir: Option<std::path::PathBuf>,
    ) {
        let cache = match cache_dir.or_else(default_cache_dir) {
            Some(dir) => CadCache::open(&dir),
            None => CadCache::in_memory(),
        };
        self.part_sources.source = Some(Arc::new(OpenLibrarySource::new(fetcher, cache)));
        self.part_sources.cache = ResponseCache::default();
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
