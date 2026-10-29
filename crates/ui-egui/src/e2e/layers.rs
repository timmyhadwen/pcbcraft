//! Stories for the unified Layers tree in the left aside (#756).
use super::super::*;
use super::support::*;

fn pcb_app() -> (egui::Context, App) {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    app.change_tab(Tab::Pcb);
    app.refresh();
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    (ctx, app)
}

#[test]
fn a_newcomer_finds_anything_through_one_tree() -> Result<(), String> {
    let (ctx, mut app) = pcb_app();
    assert!(
        paints_text(&ctx, &mut app, "Layer 1 · Top")?,
        "the tree opens on the top layer: {}",
        app.message
    );
    assert!(
        paints_text(&ctx, &mut app, "Copper")?,
        "the copper sublayer is a row of the tree: {}",
        app.message
    );
    // Search a net, select it, see it highlighted.
    let net = app.project.nets[0].clone();
    app.tree.query = net.name.clone();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    press_on(
        &ctx,
        &mut app,
        &format!("nets/net:{}", net.id),
        egui::Key::Enter,
    );
    assert_eq!(
        app.highlight_net,
        Some(net.id),
        "selecting a net row lights the net on the drawing: {}",
        app.message
    );
    app.tree.query.clear();
    // Focus a layer: the others dim.
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    press_on(
        &ctx,
        &mut app,
        "side:top/copper",
        egui::Key::F,
    );
    assert_eq!(
        app.focus_layer.as_deref(),
        Some("copper:front"),
        "F focuses the layer under the cursor: {}",
        app.message
    );
    assert!(
        paints_text(&ctx, &mut app, "FOCUSED ON Top copper")?,
        "the drawing says what is focused: {}",
        app.message
    );
    key(&ctx, &mut app, egui::Key::F);
    assert_eq!(app.focus_layer, None, "F again brings everything back");
    // The board outline row opens the board settings in Properties.
    press_on(&ctx, &mut app, "root", egui::Key::Enter);
    assert!(
        paints_text(&ctx, &mut app, "Board settings")?,
        "the board row opens board settings: {}",
        app.message
    );
    Ok(())
}

#[test]
fn a_newcomer_hides_and_locks_a_layer_from_the_tree() -> Result<(), String> {
    let (ctx, mut app) = pcb_app();
    press_on(
        &ctx,
        &mut app,
        "side:top/copper",
        egui::Key::Space,
    );
    assert!(
        app.preferences
            .hidden_layers
            .iter()
            .any(|h| h == "Top copper"),
        "Space hides the layer: {}",
        app.message
    );
    key(&ctx, &mut app, egui::Key::Space);
    assert!(
        !app.preferences
            .hidden_layers
            .iter()
            .any(|h| h == "Top copper"),
        "Space shows it again"
    );
    app.project.board.tracks.push(pcbcraft_doc::Track {
        id: 9_999,
        net: app.project.nets[0].id,
        start: Point::new(10.0, 10.0),
        end: Point::new(20.0, 10.0),
        width: 0.25,
        layer: pcbcraft_doc::Layer::Front,
        arc: None,
    });
    let spot = Point::new(15.0, 10.0);
    assert_eq!(app.pick_object(spot, 10.0), Some(9_999));
    key(&ctx, &mut app, egui::Key::L);
    assert!(app.tree.locked.contains("copper:front"));
    assert_eq!(
        app.pick_object(spot, 10.0),
        None,
        "a locked layer cannot be picked on the drawing"
    );
    key(&ctx, &mut app, egui::Key::L);
    assert_eq!(app.pick_object(spot, 10.0), Some(9_999));
    Ok(())
}

#[test]
fn selecting_on_the_canvas_reveals_the_row_in_the_tree() -> Result<(), String> {
    let (ctx, mut app) = pcb_app();
    app.tree.expanded.remove("components");
    let id = app.project.board.footprints[0].id;
    app.select(Some(id));
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        app.tree.expanded.contains("components"),
        "the Parts group opens for a part picked on the drawing"
    );
    assert_eq!(
        app.tree.cursor.as_deref(),
        Some(format!("components/fp:{id}").as_str())
    );
    let reference = app
        .project
        .component(app.project.board.footprints[0].component)
        .map(|c| c.reference.clone())
        .ok_or("no part")?;
    assert!(
        paints_text(&ctx, &mut app, &reference)?,
        "the part's row is on screen: {}",
        app.message
    );
    Ok(())
}

#[test]
fn a_board_of_two_thousand_parts_keeps_the_tree_interactive() -> Result<(), String> {
    let (ctx, mut app) = pcb_app();
    let template = app.project.board.footprints[0].clone();
    for i in 0..2000u64 {
        let mut f = template.clone();
        f.id = 1_000_000 + i;
        app.project.board.footprints.push(f);
    }
    app.revision = app.revision.wrapping_add(1);
    app.tree.expanded.insert("components".to_owned());
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let start = std::time::Instant::now();
    for _ in 0..5 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    let each = start.elapsed() / 5;
    assert!(
        each.as_millis() < 400,
        "a frame with 2,000 parts took {each:?}"
    );
    Ok(())
}
