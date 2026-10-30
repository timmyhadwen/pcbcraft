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

/// Time of a whole window frame (panels, trees, canvas) on each workspace
/// of the showcase. Run with
/// `cargo test --release -p pcbcraft-ui-egui whole_frame_time -- --ignored --nocapture`.
#[test]
#[ignore = "timing report, run by hand in release"]
fn whole_frame_time() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    if !app.open_showcase() {
        return Err(app.message.clone());
    }
    let ctx = egui::Context::default();
    for tab in [Tab::Schematic, Tab::Pcb] {
        app.change_tab(tab);
        for _ in 0..3 {
            frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        }
        let mut times = Vec::new();
        for _ in 0..9 {
            let start = std::time::Instant::now();
            frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
            times.push(start.elapsed());
        }
        times.sort();
        println!(
            "{tab:?}: median {:?}, best {:?}, worst {:?}",
            times[4], times[0], times[8]
        );
    }
    Ok(())
}

/// Time of the full checks (connectivity, design rules, airwires) the app
/// recomputes after every change, on the showcase.
#[test]
#[ignore = "timing report, run by hand in release"]
fn checks_time() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared.clone(), Preferences::default(), None, None);
    app.refresh();
    if !app.open_showcase() {
        return Err(app.message.clone());
    }
    let session = shared.lock().map_err(|_| "session")?;
    for name in ["erc", "drc", "airwires", "all"] {
        let start = std::time::Instant::now();
        match name {
            "erc" => drop(pcbcraft_rules::erc(&session.project)),
            "drc" => drop(pcbcraft_rules::drc(&session.project)),
            "airwires" => drop(pcbcraft_rules::airwires(&session.project)),
            _ => drop(session.checks()),
        }
        println!("{name}: {:?}", start.elapsed());
    }
    Ok(())
}

/// Write the checks of both showcase files to `PCBCRAFT_CHECKS_DUMP`, so two
/// builds of the rules can be compared byte for byte.
#[test]
#[ignore = "comparison dump, run by hand"]
fn dump_checks() -> Result<(), String> {
    let Ok(out) = std::env::var("PCBCRAFT_CHECKS_DUMP") else {
        return Ok(());
    };
    let mut text = String::new();
    for name in ["showcase.pcbcraft", "showcase-with-findings.pcbcraft"] {
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(shared.clone(), Preferences::default(), None, None);
        app.refresh();
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples")
            .join(name);
        app.tool_call("open", json!({"path": path.to_string_lossy()}));
        let session = shared.lock().map_err(|_| "session")?;
        let mut checks = session.checks();
        if let Some(map) = checks.as_object_mut() {
            map.remove("revision");
        }
        text.push_str(&serde_json::to_string_pretty(&checks).map_err(|e| e.to_string())?);
    }
    std::fs::write(out, text).map_err(|e| e.to_string())
}

/// Dragging a part keeps what is wired to it: every drawn wire end that was
/// on one of its pins, and every track end on one of its pads, is still on
/// it after the move.
#[test]
fn a_designer_drags_parts_and_their_wiring_comes_along() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    if !app.open_showcase() {
        return Err(app.message.clone());
    }
    let id_of = |app: &App, r: &str| {
        app.project
            .components
            .iter()
            .find(|c| c.reference == r)
            .map(|c| c.id)
            .ok_or(format!("{r} exists"))
    };
    // Schematic: R5 has a stub on each pin.
    let r5 = id_of(&app, "R5")?;
    let pins = |app: &App| pcbcraft_engine::drag::anchors(&app.project, r5);
    let ends = |app: &App| -> Vec<Point> {
        app.project
            .drawn_wires
            .iter()
            .flat_map(|w| [w.points.first().copied(), w.points.last().copied()])
            .flatten()
            .collect()
    };
    let on = |points: &[Point], at: Point| points.iter().any(|q| q.distance(at) < 1e-6);
    let wired = pins(&app)
        .iter()
        .filter_map(|(at, _)| *at)
        .filter(|at| on(&ends(&app), *at))
        .count();
    assert!(wired >= 2, "R5 starts wired");
    let to = app
        .project
        .component(r5)
        .ok_or("R5")?
        .position
        .offset(Point::new(5.08, 2.54));
    app.run(
        "component.move",
        json!({"id": r5, "position": to, "drag": true}),
    );
    let after = ends(&app);
    let still = pins(&app)
        .iter()
        .filter_map(|(at, _)| *at)
        .filter(|at| on(&after, *at))
        .count();
    assert_eq!(
        still, wired,
        "every wired pin keeps its wire: {}",
        app.message
    );
    // Board: U3's pads carry tracks.
    let u3 = app
        .project
        .board
        .footprints
        .iter()
        .find(|f| Some(f.component) == id_of(&app, "U3").ok())
        .map(|f| (f.id, f.position))
        .ok_or("U3 footprint")?;
    let pads = |app: &App| pcbcraft_engine::drag::anchors(&app.project, u3.0);
    let track_ends = |app: &App| -> Vec<Point> {
        app.project
            .board
            .tracks
            .iter()
            .flat_map(|t| [t.start, t.end])
            .collect()
    };
    let routed = pads(&app)
        .iter()
        .filter_map(|(at, _)| *at)
        .filter(|at| on(&track_ends(&app), *at))
        .count();
    assert!(routed > 4, "U3 starts routed");
    app.run(
        "footprint.move",
        json!({"id": u3.0, "position": u3.1.offset(Point::new(1.0, 0.5)), "drag": true}),
    );
    let after = track_ends(&app);
    let still = pads(&app)
        .iter()
        .filter_map(|(at, _)| *at)
        .filter(|at| on(&after, *at))
        .count();
    assert_eq!(
        still, routed,
        "every routed pad keeps its track: {}",
        app.message
    );
    Ok(())
}
