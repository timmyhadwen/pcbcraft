//! Stories for the canvas view: the layer tabs under the board, single layer
//! mode, the heads-up readout beside the cursor and the View menu items that
//! replaced the floating view buttons.
use super::super::*;
use super::support::*;
use crate::draw_target::{Side, Target};

fn board_app() -> (App, egui::Context) {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    app.change_tab(Tab::Pcb);
    app.refresh();
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    (app, ctx)
}

#[test]
fn a_newcomer_clicks_a_layer_tab_and_draws_on_that_layer() -> Result<(), String> {
    let (mut app, ctx) = board_app();
    assert_eq!(
        app.current_target(),
        Target::Copper(pcbcraft_doc::Layer::Front)
    );
    let bottom = app.bottom_copper();
    let name = app.project.layer_name(bottom);
    authoring::tests::tap_icon(&ctx, &mut app, &format!("layer-tab:{name}"))?;
    assert_eq!(
        app.current_target(),
        Target::Copper(bottom),
        "the bottom copper tab makes it the drawing layer"
    );
    assert_eq!(app.route_layer, bottom, "routing follows the tab");
    authoring::tests::tap_icon(&ctx, &mut app, "layer-tab:Top silk")?;
    assert_eq!(app.current_target(), Target::Silk(Side::Top));
    assert!(
        paints_text(&ctx, &mut app, "Layer 1 of")?,
        "the status bar names the new layer"
    );
    Ok(())
}

#[test]
fn a_newcomer_turns_single_layer_mode_on_and_off_with_ls() -> Result<(), String> {
    let (mut app, ctx) = board_app();
    assert!(app.focus_layer.is_none());
    authoring::tests::tap_icon(&ctx, &mut app, "layer-tabs-ls")?;
    assert_eq!(
        app.focus_layer.as_deref(),
        Some("copper:front"),
        "LS shows only the current layer"
    );
    // With single layer mode on, another tab moves the focus with it.
    let bottom = app.bottom_copper();
    let name = app.project.layer_name(bottom);
    authoring::tests::tap_icon(&ctx, &mut app, &format!("layer-tab:{name}"))?;
    assert_eq!(app.focus_layer.as_deref(), Some("copper:back"));
    authoring::tests::tap_icon(&ctx, &mut app, "layer-tabs-ls")?;
    assert!(app.focus_layer.is_none(), "LS again shows every layer");
    Ok(())
}

/// The text of every shape painted in `output`.
fn painted(output: &egui::FullOutput) -> Vec<String> {
    output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_newcomer_reads_the_cursor_position_beside_the_cursor() -> Result<(), String> {
    for tab in [Tab::Pcb, Tab::Schematic] {
        let (mut app, ctx) = board_app();
        app.change_tab(tab);
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        let over = app.canvas_rect.center();
        let mut seen = vec![];
        for _ in 0..3 {
            let output = frame(
                &ctx,
                &mut app,
                vec![egui::Event::PointerMoved(over)],
                egui::Modifiers::NONE,
            );
            seen = painted(&output);
        }
        let at = app.cursor.ok_or("the pointer is over the canvas")?;
        let snapped = canvas::snap(at);
        let expected = crate::snap_grid::format_point(snapped.x, snapped.y, false);
        assert!(
            seen.iter()
                .any(|t| t.starts_with("X, Y") && t.contains(&expected)),
            "the readout shows {expected} on {tab:?}: {seen:?}"
        );
        if tab == Tab::Pcb {
            let layer = app.target_label();
            assert!(
                seen.contains(&layer),
                "the board readout names the layer {layer}"
            );
        }
        // Shift+H hides it.
        app.dispatch_shortcut(keymap::KeyAction::ToggleHud);
        let output = frame(
            &ctx,
            &mut app,
            vec![egui::Event::PointerMoved(over)],
            egui::Modifiers::NONE,
        );
        assert!(
            !painted(&output).iter().any(|t| t.starts_with("X, Y")),
            "Shift+H hides the readout"
        );
        assert!(!app.preferences.canvas.hud);
    }
    Ok(())
}

#[test]
fn a_newcomer_measures_from_the_last_click_in_the_readout() -> Result<(), String> {
    let (mut app, ctx) = board_app();
    app.tool = Tool::Measure;
    let first = app.canvas_rect.center();
    tap_at(&ctx, &mut app, first)?;
    let later = first + Vec2::new(60.0, 0.0);
    let mut seen = vec![];
    for _ in 0..2 {
        seen = painted(&frame(
            &ctx,
            &mut app,
            vec![egui::Event::PointerMoved(later)],
            egui::Modifiers::NONE,
        ));
    }
    assert!(
        seen.iter().any(|t| t.starts_with("dX, dY")),
        "a distance from the click shows while the tool is busy: {seen:?}"
    );
    Ok(())
}

#[test]
fn a_newcomer_finds_the_view_controls_in_the_view_menu() -> Result<(), String> {
    let (mut app, ctx) = board_app();
    assert!(
        !app.icon_rects.contains_key("canvas-fit") && !app.icon_rects.contains_key("handle-rotate"),
        "no floating buttons sit over the design"
    );
    menu_item(&ctx, &mut app, "View", "3D view")?;
    assert_eq!(app.tab, Tab::Board3d, "View > 3D view shows the 3D board");
    assert!(app.region().view_3d, "the Layout workspace remembers 3D");
    menu_item(&ctx, &mut app, "View", "2D layout")?;
    assert_eq!(app.tab, Tab::Pcb, "View > 2D layout brings the board back");
    menu_item(&ctx, &mut app, "View", "Flip board")?;
    assert!(app.pcb.mirror, "Flip board looks from below");
    menu_item(&ctx, &mut app, "View", "Rotate view right")?;
    assert_eq!(app.pcb.turns, 1, "the view turns a quarter");
    menu_item(&ctx, &mut app, "View", "Reset view")?;
    assert!(
        !app.pcb.mirror && app.pcb.turns == 0,
        "Reset view is upright from the top"
    );
    let before = app.pcb.zoom;
    menu_item(&ctx, &mut app, "View", "Zoom out")?;
    assert!(app.pcb.zoom < before, "View > Zoom out zooms out");
    menu_item(&ctx, &mut app, "View", "Fit document")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(!app.needs_fit, "Fit document framed the board");
    assert!(!app.preferences.canvas.rulers, "rulers start off");
    menu_item(&ctx, &mut app, "View", "Rulers")?;
    assert!(app.preferences.canvas.rulers, "View > Rulers shows them");
    menu_item(&ctx, &mut app, "View", "Board dimensions")?;
    assert!(app.preferences.canvas.dimensions);
    assert!(
        paints_text(&ctx, &mut app, "%")?,
        "the zoom level shows in the status bar"
    );
    Ok(())
}
