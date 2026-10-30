use super::super::*;
use super::support::*;

/// A newcomer picks the showcase board from the Start page and lands in a
/// finished design with its sheets, parts and routed copper.
#[test]
fn a_newcomer_opens_the_showcase_example() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.home_open = true;
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        paints_text(&ctx, &mut app, "Showcase board")?,
        "the start screen offers the finished sample board"
    );
    authoring::tests::tap(&ctx, &mut app, "Showcase board")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(!app.home_open, "picking the board leaves the start screen");
    assert_eq!(app.project.components.len(), 55, "every part arrives");
    assert_eq!(
        app.project.sheets.len() + 1,
        3,
        "the root page and two named pages arrive: {}",
        app.message
    );
    authoring::tests::tap(&ctx, &mut app, "Schematic")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    for page in ["Compute", "Sensors and indicators"] {
        assert!(
            app.project.sheets.iter().any(|s| s.name == page),
            "the design names the page {page}"
        );
    }
    assert!(
        !app.project.board.tracks.is_empty(),
        "the board comes routed"
    );
    Ok(())
}

/// Every page of the showcase schematic reads cleanly at a full-HD window:
/// no text overlaps another and every text sits inside the sheet frame,
/// measured on the glyphs the canvas actually paints.
#[test]
fn the_showcase_schematic_text_is_clear_on_every_page() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    if !app.open_showcase() {
        return Err(format!("the showcase opens: {}", app.message));
    }
    app.change_tab(Tab::Schematic);
    let pages: Vec<Id> = std::iter::once(0)
        .chain(app.project.sheets.iter().map(|s| s.id))
        .collect();
    let mut problems = Vec::new();
    for page in pages {
        app.show_page(page);
        let (ink, frame) = app.schematic_ink(egui::Vec2::new(1920.0, 1240.0));
        assert!(ink.len() > 20, "page {page} paints its texts");
        for (i, (text, rect)) in ink.iter().enumerate() {
            if !frame.contains_rect(*rect) {
                problems.push(format!("page {page}: {text:?} leaves the frame"));
            }
            for (other, next) in &ink[i + 1..] {
                if rect.intersects(*next) {
                    problems.push(format!("page {page}: {text:?} overlaps {other:?}"));
                }
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
    Ok(())
}
