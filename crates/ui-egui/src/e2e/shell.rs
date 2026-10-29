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

/// Every `.rs` file under `src`, as `(path relative to src, text)`.
fn ui_sources(dir: &std::path::Path, base: &std::path::Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(ui_sources(&path, base));
        } else if path.extension().is_some_and(|x| x == "rs")
            && let (Ok(text), Ok(rel)) = (std::fs::read_to_string(&path), path.strip_prefix(base))
        {
            out.push((rel.to_string_lossy().replace('\\', "/"), text));
        }
    }
    out
}

#[test]
fn no_user_visible_literal_outside_the_catalog() -> Result<(), String> {
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let files = ui_sources(&src, &src);
    assert!(!files.is_empty(), "the ui sources were found");
    let problems = pcbcraft_text::scan::problems(&files);
    assert!(
        problems.is_empty(),
        "text must come from the catalog (crates/text/strings/en.txt):\n{}",
        problems.join("\n")
    );
    Ok(())
}

#[test]
fn a_newcomer_switches_the_app_to_their_language() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(
        shared.clone(),
        Preferences {
            language: "en".into(),
            ..Preferences::default()
        },
        None,
        None,
    );
    app.refresh();
    let ctx = egui::Context::default();
    assert!(
        paints_text(&ctx, &mut app, "Settings")?,
        "the app opens in English and the top bar says Settings"
    );
    assert!(
        !paints_text(&ctx, &mut app, "Paramètres")?,
        "no French shows yet"
    );
    // The person opens the Settings page and picks a language.
    app.change_tab(Tab::Settings);
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Language")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Français (test)")?;
    assert!(
        paints_text(&ctx, &mut app, "Fichier")?,
        "the File menu now reads Fichier at once, with no restart: {}",
        app.message
    );
    assert!(
        paints_text(&ctx, &mut app, "Accueil")?,
        "the Home button reads Accueil"
    );
    assert_eq!(app.preferences.language, "fr", "the choice is remembered");
    // Closing and reopening the app keeps the saved choice.
    let saved = app.preferences.clone();
    let mut reopened = App::create(shared, saved, None, None);
    reopened.refresh();
    assert!(
        paints_text(&ctx, &mut reopened, "Fichier")?,
        "the reopened app still reads French"
    );
    // A language tag the app does not carry quietly shows English.
    reopened.set_language("zz");
    assert!(
        paints_text(&ctx, &mut reopened, "File")?,
        "an unknown language falls back to English"
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
