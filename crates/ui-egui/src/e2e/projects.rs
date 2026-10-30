//! Stories for the Projects tab of the left dock: the design's documents in
//! one tree, each row opening its document, and the left dock's tab order.
use super::super::*;
use super::support::*;

/// A design with the Main page and two more, on the Schematic, with the
/// Projects tab showing.
fn three_pages() -> (egui::Context, App) {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    app.change_tab(Tab::Schematic);
    app.add_page();
    app.add_page();
    app.show_page(0);
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    (ctx, app)
}

#[test]
fn a_newcomer_finds_projects_components_and_layers_in_the_left_dock() -> Result<(), String> {
    let (ctx, mut app) = three_pages();
    assert_eq!(
        app.region().left_tab,
        "projects",
        "a fresh profile opens on Projects"
    );
    let ids: Vec<&str> = app.workspace().left_tabs().iter().map(|t| t.id).collect();
    assert_eq!(
        ids,
        ["projects", "library", "layers"],
        "Projects, Components, Layers"
    );
    authoring::tests::tap(&ctx, &mut app, "Components")?;
    assert_eq!(
        app.region().left_tab,
        "library",
        "Components shows the parts"
    );
    authoring::tests::tap(&ctx, &mut app, "Layers")?;
    assert_eq!(app.region().left_tab, "layers", "Layers shows the tree");
    authoring::tests::tap(&ctx, &mut app, "Projects")?;
    assert_eq!(app.region().left_tab, "projects", "Projects comes back");
    Ok(())
}

#[test]
fn a_newcomer_opens_every_page_from_the_projects_tab() -> Result<(), String> {
    let (ctx, mut app) = three_pages();
    let pages: Vec<(Id, String)> = std::iter::once(0)
        .chain(app.project.sheets.iter().map(|s| s.id))
        .map(|id| (id, app.page_name(id)))
        .collect();
    assert_eq!(pages.len(), 3, "Main and the two added pages");
    for word in ["Source documents", "Libraries", "Outputs", "Board"] {
        assert!(
            paints_text(&ctx, &mut app, word)?,
            "the Projects tree shows {word}"
        );
    }
    // Leave for the board first, so each click has to come back.
    app.change_tab(Tab::Pcb);
    for (id, name) in pages.iter().rev() {
        tap_left(&ctx, &mut app, name)?;
        assert_eq!(app.tab, Tab::Schematic, "{name} opens the Schematic");
        assert_eq!(app.schematic_page(), *id, "{name} is the page on screen");
    }
    Ok(())
}

#[test]
fn a_newcomer_opens_the_board_and_the_outputs_from_the_projects_tab() -> Result<(), String> {
    let (ctx, mut app) = three_pages();
    tap_left(&ctx, &mut app, "Board")?;
    assert_eq!(app.tab, Tab::Pcb, "the board row opens Layout");
    assert_eq!(
        app.region().left_tab,
        "projects",
        "Layout opens on Projects too"
    );
    tap_left(&ctx, &mut app, "Project library")?;
    assert_eq!(
        app.region().left_tab,
        "library",
        "the project library row opens Components"
    );
    app.region().left_tab = "projects".to_owned();
    let output = if app.project.output_jobs.is_empty() {
        "No output jobs yet".to_owned()
    } else {
        app.project.output_jobs[0].name.clone()
    };
    tap_left(&ctx, &mut app, &output)?;
    assert_eq!(app.tab, Tab::PlotFiles, "an output row opens Manufacture");
    Ok(())
}

#[test]
fn a_left_tab_saved_by_an_older_build_stays_on_a_listed_tab() -> Result<(), String> {
    // An older build kept a numeric `left_tab` and ids that may no longer exist.
    let mut stored = serde_json::to_value(Preferences::default()).map_err(|e| e.to_string())?;
    stored["left_tab"] = json!(7);
    stored["layouts"] = json!({
        "schematic": {"left_tab": "no-such-tab", "bottom_tab": "findings", "bottom_open": false},
        "layout": {"left_tab": "library", "bottom_tab": "findings", "bottom_open": false},
        "manufacture": {"left_tab": "layers", "bottom_tab": "output", "bottom_open": false},
    });
    let preferences: Preferences = serde_json::from_value(stored).map_err(|e| e.to_string())?;
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, preferences, None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    let ctx = egui::Context::default();
    for (tab, expected) in [
        (Tab::Schematic, "projects"),
        (Tab::Pcb, "library"),
        (Tab::PlotFiles, "projects"),
    ] {
        app.change_tab(tab);
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        let current = app.region().left_tab.clone();
        assert_eq!(current, expected, "{tab:?} lands on a tab it lists");
        assert!(
            app.workspace().left_tabs().iter().any(|t| t.id == current),
            "{current} is in range for {tab:?}"
        );
    }
    Ok(())
}

#[test]
fn the_layer_and_footprint_keys_open_their_left_tabs() -> Result<(), String> {
    let (_ctx, mut app) = three_pages();
    app.classic_action(keymap::KeyAction::LayerPanel);
    assert_eq!(
        app.region().left_tab,
        "layers",
        "the layer key opens Layers"
    );
    app.classic_action(keymap::KeyAction::PlaceFootprint);
    assert_eq!(
        app.region().left_tab,
        "library",
        "the footprint key opens Components"
    );
    Ok(())
}

#[test]
fn the_left_dock_tabs_fit_the_narrowest_dock() -> Result<(), String> {
    let (ctx, mut app) = three_pages();
    let output = frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    // The tab row sits just under the tool bar; the last label is Layers.
    let right = output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Text(t) if t.galley.text() == "Layers" && t.pos.y < 140.0 => {
                Some(t.pos.x + t.galley.size().x)
            }
            _ => None,
        })
        .reduce(f32::max)
        .ok_or("the Layers tab is on screen")?;
    // Its side padding is 14; the narrowest dock is 272 wide.
    assert!(
        right + 14.0 <= 272.0,
        "the Layers tab ends at {right}, past the narrowest dock"
    );
    Ok(())
}
