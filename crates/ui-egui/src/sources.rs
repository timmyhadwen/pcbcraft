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
fn stock_line(part: &PartResult) -> String {
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
    pub(super) fn source_section(&mut self, ui: &mut egui::Ui) {
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new("From part sources")
                .size(10.0)
                .color(self.muted()),
        );
        #[cfg(target_arch = "wasm32")]
        {
            ui.label(
                egui::RichText::new("Part sources are available in the desktop app.")
                    .small()
                    .color(self.muted()),
            );
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            if self.part_sources.source.is_none() {
                ui.label(
                    egui::RichText::new(
                        "Part sources are off. Only parts on this computer are listed.",
                    )
                    .small()
                    .color(self.muted()),
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
                return;
            }
            let query = self.part_search.trim().to_owned();
            if query.is_empty() {
                ui.label(
                    egui::RichText::new("Type a part number or keyword to search the sources.")
                        .small()
                        .color(self.muted()),
                );
                return;
            }
            let Some(source) = self.part_sources.source.as_deref() else {
                return;
            };
            let before = self.part_sources.cache.misses;
            let found = self.part_sources.cache.search(source, &query, now_secs());
            if self.part_sources.cache.misses != before {
                eprintln!(
                    "part search source={} results={}",
                    source.id(),
                    found.as_ref().map(Vec::len).unwrap_or(0)
                );
            }
            let name = source.name().to_owned();
            let credit = source.attribution();
            match found {
                Err(e) => {
                    ui.label(egui::RichText::new(e.to_string()).small());
                }
                Ok(parts) if parts.is_empty() => {
                    ui.label(
                        egui::RichText::new(format!("{name} has nothing for \"{query}\"."))
                            .small()
                            .color(self.muted()),
                    );
                }
                Ok(parts) => {
                    for part in parts {
                        self.source_card(ui, &part, &credit.text);
                    }
                }
            }
            if ui
                .small_button("Forget saved answers")
                .on_hover_text(
                    "Clear the answers kept for a day so the next search asks the source again.",
                )
                .clicked()
            {
                self.part_sources.cache.purge();
            }
        }
    }
    fn source_card(&mut self, ui: &mut egui::Ui, part: &PartResult, credit: &str) {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if ui
                .button(&part.mpn)
                .on_hover_text("Pick this part; the next click on the schematic places it with its maker and part number filled in.")
                .clicked()
            {
                let kind = TEMPLATES
                    .iter()
                    .find(|t| t.kind == part.kind)
                    .map(|t| t.kind)
                    .unwrap_or("resistor");
                self.change_tab(Tab::Schematic);
                self.tool = Tool::Place(kind.into());
                self.pending_source = Some(part.clone());
                self.palette = false;
                self.part_search.clear();
            }
            ui.label(egui::RichText::new(&part.source).small().strong())
                .on_hover_text(format!("This result came from {}.", part.source));
        });
        let facts = format!("{} · {}", part.manufacturer, part.description);
        ui.label(egui::RichText::new(facts).small().color(self.muted()));
        ui.label(egui::RichText::new(stock_line(part)).small());
        if !part.lifecycle.is_empty() {
            ui.label(
                egui::RichText::new(format!("Status: {}", part.lifecycle))
                    .small()
                    .color(self.muted()),
            );
        }
        if !credit.is_empty() {
            ui.label(egui::RichText::new(credit).size(9.0).color(self.muted()));
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

#[cfg(test)]
mod tests {
    use super::*;
    fn frame(ctx: &egui::Context, app: &mut App, events: Vec<egui::Event>) {
        let mut out = ctx.run_ui(
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
        out.textures_delta.clear();
    }
    fn painted(ctx: &egui::Context, app: &mut App, needle: &str) -> bool {
        for _ in 0..3 {
            let mut out = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        Vec2::new(1440.0, 940.0),
                    )),
                    ..Default::default()
                },
                |ui| app.show(ui),
            );
            out.textures_delta.clear();
            if out.shapes.iter().any(
                |s| matches!(&s.shape, egui::Shape::Text(t) if t.galley.text().contains(needle)),
            ) {
                return true;
            }
        }
        false
    }
    fn type_text(ctx: &egui::Context, app: &mut App, text: &str) {
        for letter in text.chars() {
            frame(ctx, app, vec![egui::Event::Text(letter.into())]);
        }
        frame(ctx, app, vec![]);
    }

    #[test]
    fn a_newcomer_finds_a_real_part_by_number_and_places_it() -> Result<(), String> {
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(shared.clone(), Preferences::default(), None, None);
        app.refresh();
        let ctx = egui::Context::default();
        frame(&ctx, &mut app, vec![]);
        authoring::tests::tap(&ctx, &mut app, "Add part")?;
        frame(&ctx, &mut app, vec![]);
        authoring::tests::tap(&ctx, &mut app, "Search parts by name")?;
        type_text(&ctx, &mut app, "resistor");
        assert!(
            painted(&ctx, &mut app, "Part sources are off"),
            "with no source the list says sources are off"
        );
        assert!(
            !painted(&ctx, &mut app, "SAMPLE-R-10K"),
            "no remote parts appear while sources are off"
        );
        authoring::tests::tap(&ctx, &mut app, "Try sample source")?;
        app.part_search = "SAMPLE-R-10K".into();
        frame(&ctx, &mut app, vec![]);
        assert!(
            painted(&ctx, &mut app, "12500 in stock"),
            "the card shows stock"
        );
        assert!(
            painted(&ctx, &mut app, "0.10 USD each"),
            "the card shows a price"
        );
        assert!(
            painted(&ctx, &mut app, "Sample data, not a real catalogue"),
            "the card shows the source's credit"
        );
        authoring::tests::tap(&ctx, &mut app, "SAMPLE-R-10K")?;
        frame(&ctx, &mut app, vec![]);
        assert!(matches!(&app.tool, Tool::Place(k) if k == "resistor"));
        // A click on the sheet runs exactly this command.
        app.run(
            "component.add",
            json!({"kind":"resistor","position":{"x":20.0,"y":20.0}}),
        );
        let session = shared.lock().map_err(|_| "session")?;
        let part = session
            .project
            .components
            .last()
            .ok_or("no part was placed")?;
        assert_eq!(part.field("MPN"), Some("SAMPLE-R-10K"));
        assert_eq!(part.field("Manufacturer"), Some("Example Components"));
        Ok(())
    }
}
