use super::super::*;
use super::support::*;

#[test]
fn a_newcomer_reads_the_status_strip() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        paints_text(&ctx, &mut app, "Tool: Pick")?,
        "the status strip should say what a click does: {}",
        app.message
    );
    let pct = (app.schematic.zoom / 15.0 * 100.0).round() as i32;
    let zoom_label = format!("{pct}%");
    assert!(
        paints_text(&ctx, &mut app, &zoom_label)?,
        "the status strip should show the zoom level ({zoom_label}): {}",
        app.message
    );
    let mut mind = 0usize;
    for key in ["erc", "drc"] {
        if let Some(findings) = app.checks.get(key).and_then(serde_json::Value::as_array) {
            for finding in findings {
                let code = finding
                    .get("code")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("finding");
                if app.project.rule_severity(code, "warning") != "error" {
                    mind += 1;
                }
            }
        }
    }
    assert!(
        mind > 0,
        "the practice board should leave something to mind"
    );
    let mind_label = format!("{mind} to mind");
    assert!(
        paints_text(&ctx, &mut app, &mind_label)?,
        "the status strip should count the warnings ({mind_label}): {}",
        app.message
    );
    authoring::tests::tap(&ctx, &mut app, &mind_label)?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert_eq!(
        app.tab,
        Tab::Checks,
        "tapping the warning count should open the checks list: {}",
        app.message
    );
    Ok(())
}
#[test]
fn a_newcomer_adds_assembly_marks_for_the_build_camera() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    app.tab = Tab::Pcb;
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Assembly mark")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.canvas_click(Point::new(-27.0, -17.0), 1.0);
    assert_eq!(
        app.project.board.fiducials.len(),
        1,
        "one click lays one mark: {}",
        app.message
    );
    app.canvas_click(Point::new(27.0, 17.0), 1.0);
    assert_eq!(
        app.project.board.fiducials.len(),
        2,
        "the tool stays ready for the next corner: {}",
        app.message
    );
    assert!(
        app.project
            .board
            .fiducials
            .iter()
            .all(|fid| (fid.diameter - 1.0).abs() < 1e-9),
        "both marks are the usual 1 mm disc: {}",
        app.message
    );
    app.tool = Tool::Select;
    app.tab = Tab::Schematic;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Layers")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        scrolled_to(&ctx, &mut app, "1.00 mm on Top")?,
        "the marks are listed with their size and side"
    );
    let last = app.project.board.fiducials[1].id;
    authoring::tests::tap(&ctx, &mut app, "Tune")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.fiducial_edit = Some((last, 1.2, false));
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Apply")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        app.project.board.fiducials.iter().any(|fid| {
            fid.id == last
                && (fid.diameter - 1.2).abs() < 1e-9
                && fid.layer == pcbcraft_doc::Layer::Back
        }),
        "the mark is resized and moved to the bottom: {}",
        app.message
    );
    authoring::tests::tap(&ctx, &mut app, "\u{d7}")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert_eq!(
        app.project.board.fiducials.len(),
        1,
        "the other mark comes off the board: {}",
        app.message
    );
    Ok(())
}
#[test]
fn a_newcomer_finds_everything_where_the_workbench_puts_it() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        paints_text(&ctx, &mut app, "Library")?,
        "the left dock is the library browser: {}",
        app.message
    );
    assert!(
        paints_text(&ctx, &mut app, "Layers")?,
        "the right dock leads with the layers tree: {}",
        app.message
    );
    assert!(
        paints_text(&ctx, &mut app, "Properties")?,
        "object details sit in the lower right tab: {}",
        app.message
    );
    authoring::tests::tap(&ctx, &mut app, "Files")?;
    assert!(
        paints_text(&ctx, &mut app, "Project files")?,
        "build outputs moved under Files: {}",
        app.message
    );
    authoring::tests::tap(&ctx, &mut app, "Layout")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        paints_text(&ctx, &mut app, "Mounting hole")?,
        "board tools sit in the strip under the tabs: {}",
        app.message
    );
    Ok(())
}

#[test]
fn a_newcomer_picks_the_move_tool_from_the_strip_icons() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Layout")?;
    authoring::tests::tap_icon(&ctx, &mut app, "tool-move")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        matches!(app.tool, Tool::Move),
        "tapping the Move icon selects the Move tool"
    );
    Ok(())
}

fn key(key: egui::Key) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: Some(key),
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }
}
fn practice_app() -> (egui::Context, App) {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    (ctx, app)
}
/// Press Tab until the named control holds focus; the order it took is
/// returned so a story can say what a keyboard user walks past.
fn tab_to(ctx: &egui::Context, app: &mut App, name: &str) -> Result<Vec<String>, String> {
    let mut seen = Vec::new();
    for _ in 0..250 {
        let out = frame(ctx, app, vec![key(egui::Key::Tab)], egui::Modifiers::NONE);
        if let Some(label) = a11y::audit::focused_name(ctx, &out) {
            seen.push(label.clone());
            if label == name {
                return Ok(seen);
            }
        }
    }
    Err(format!("Tab never reached {name}; it passed {seen:?}"))
}

#[test]
fn a_newcomer_opens_settings_from_the_app_bar_with_the_keyboard_only() -> Result<(), String> {
    let (ctx, mut app) = practice_app();
    let seen = tab_to(&ctx, &mut app, "Settings")?;
    assert_eq!(
        seen.first().map(String::as_str),
        Some("PCBCraft start screen"),
        "the first stop is the app name: {seen:?}"
    );
    assert!(
        seen.iter().any(|n| n == "Home") && seen.iter().any(|n| n == "Undo"),
        "the app bar buttons come in reading order: {seen:?}"
    );
    let shown = frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        shown.shapes.iter().any(|s| matches!(&s.shape,
            egui::Shape::Rect(r) if r.stroke.width >= 2.0 && r.stroke.color == app.accent())),
        "the focused control wears a visible ring"
    );
    frame(
        &ctx,
        &mut app,
        vec![key(egui::Key::Enter)],
        egui::Modifiers::NONE,
    );
    assert_eq!(app.tab, Tab::Settings, "Enter presses it: {}", app.message);
    Ok(())
}

#[test]
fn a_newcomer_switches_view_with_the_keyboard_only() -> Result<(), String> {
    let (ctx, mut app) = practice_app();
    tab_to(&ctx, &mut app, "Layout")?;
    frame(
        &ctx,
        &mut app,
        vec![key(egui::Key::Space)],
        egui::Modifiers::NONE,
    );
    assert_eq!(app.tab, Tab::Pcb, "Space opens the layout: {}", app.message);
    // Tool buttons are reachable too and say what they do, not "tool-route".
    let seen = tab_to(&ctx, &mut app, "Route")?;
    assert!(
        seen.iter().any(|n| n == "Select"),
        "tool names are spoken: {seen:?}"
    );
    Ok(())
}

#[test]
fn every_interactive_accesskit_node_has_a_name_on_each_tab() -> Result<(), String> {
    let (ctx, mut app) = practice_app();
    let mut gaps = Vec::new();
    for tab in [
        Tab::Files,
        Tab::Schematic,
        Tab::Pcb,
        Tab::Board3d,
        Tab::Rules,
        Tab::Checks,
        Tab::Settings,
    ] {
        app.change_tab(tab);
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        let out = frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        let nodes = a11y::audit::interactive_nodes(&out);
        assert!(
            !nodes.is_empty(),
            "{tab:?} exposes no controls to a screen reader"
        );
        let blank = a11y::audit::unnamed(&out);
        if !blank.is_empty() {
            gaps.push(format!("{tab:?}: {blank:?}"));
        }
    }
    assert!(gaps.is_empty(), "unnamed controls: {gaps:?}");
    Ok(())
}

#[test]
fn preferences_ui_scale_clamps_and_persists() -> Result<(), String> {
    assert_eq!(a11y::clamp_ui_scale(9.0), a11y::MAX_UI_SCALE);
    assert_eq!(a11y::clamp_ui_scale(0.1), a11y::MIN_UI_SCALE);
    assert_eq!(a11y::clamp_ui_scale(f32::NAN), a11y::DEFAULT_UI_SCALE);
    let prefs = Preferences {
        ui_scale: 1.5,
        ..Preferences::default()
    };
    let text = serde_json::to_string(&prefs).map_err(|e| e.to_string())?;
    let back: Preferences = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    assert_eq!(back.ui_scale, 1.5, "the scale survives a restart");
    let mut old = serde_json::to_value(Preferences::default()).map_err(|e| e.to_string())?;
    if let Some(map) = old.as_object_mut() {
        map.remove("ui_scale");
    }
    let legacy: Preferences = serde_json::from_value(old).map_err(|e| e.to_string())?;
    assert_eq!(
        legacy.ui_scale, 1.0,
        "older saved settings load at normal size"
    );
    let (ctx, mut app) = practice_app();
    app.preferences.ui_scale = 1.5;
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    assert!(
        (ctx.zoom_factor() - 1.5).abs() < 1e-3,
        "the window rescales"
    );
    app.preferences.ui_scale = 40.0;
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    assert_eq!(
        ctx.zoom_factor(),
        a11y::MAX_UI_SCALE,
        "absurd values are held in range"
    );
    Ok(())
}
