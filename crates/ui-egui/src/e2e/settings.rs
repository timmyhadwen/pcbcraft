use super::super::*;
use super::support::*;

fn fresh() -> App {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app
}

#[test]
fn a_newcomer_visits_settings_and_comes_back_to_the_same_zoom() -> Result<(), String> {
    let mut app = fresh();
    let ctx = egui::Context::default();
    app.change_tab(Tab::Pcb);
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.pcb.zoom = 33.0;
    for tab in [Tab::Settings, Tab::Rules, Tab::Checks, Tab::Pcb] {
        app.change_tab(tab);
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    assert_eq!(app.pcb.zoom, 33.0, "the board stays as the user left it");
    Ok(())
}

#[test]
fn a_newcomer_reads_settings_with_room_around_the_text() -> Result<(), String> {
    let mut app = fresh();
    let ctx = egui::Context::default();
    for tab in [Tab::Settings, Tab::Rules, Tab::Checks] {
        app.change_tab(tab);
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        let output = frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        let left = app.page_left;
        let mut tight = Vec::new();
        for shape in &output.shapes {
            if let egui::Shape::Text(text) = &shape.shape
                && text.pos.x > left
                && text.pos.x < left + 16.0
                && text.pos.y > 80.0
            {
                tight.push(text.galley.text().to_owned());
            }
        }
        assert!(tight.is_empty(), "{tab:?} text hugs the edge: {tight:?}");
    }
    Ok(())
}

#[test]
fn a_newcomer_finds_appearance_on_the_settings_page() -> Result<(), String> {
    let mut app = fresh();
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.change_tab(Tab::Settings);
    assert!(
        paints_text(&ctx, &mut app, "APPEARANCE")?,
        "appearance lives on the Settings page"
    );
    app.change_tab(Tab::Pcb);
    assert!(
        !paints_text(&ctx, &mut app, "APPEARANCE")?,
        "the inspector no longer shows appearance"
    );
    Ok(())
}

fn sample_in_settings() -> Result<(App, egui::Context), String> {
    let mut app = fresh();
    app.tool_call("sample_design", json!({}));
    let ctx = egui::Context::default();
    app.change_tab(Tab::Settings);
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    Ok((app, ctx))
}

#[test]
fn a_newcomer_resizes_the_board_and_cuts_a_round_hole() -> Result<(), String> {
    let (mut app, ctx) = sample_in_settings()?;
    type_into(&ctx, &mut app, "spinbutton:Board width", "80")?;
    type_into(&ctx, &mut app, "spinbutton:Board height", "60")?;
    tap_id(&ctx, &mut app, "button:Apply board dimensions")?;
    assert!(
        (app.project.board.width - 80.0).abs() < 1e-9
            && (app.project.board.height - 60.0).abs() < 1e-9,
        "the typed size becomes the board size: {}",
        app.message
    );
    let holes = app.project.board.cutouts.len();
    type_into(&ctx, &mut app, "spinbutton:Cutout X", "20")?;
    type_into(&ctx, &mut app, "spinbutton:Cutout Y", "25")?;
    type_into(&ctx, &mut app, "spinbutton:Cutout radius", "3")?;
    tap_id(&ctx, &mut app, "button:Add hole")?;
    let hole = app
        .project
        .board
        .cutouts
        .get(holes)
        .ok_or_else(|| format!("one click adds one round hole: {}", app.message))?;
    assert!(
        (hole.center.x - 20.0).abs() < 1e-9
            && (hole.center.y - 25.0).abs() < 1e-9
            && (hole.radius - 3.0).abs() < 1e-9,
        "the hole sits where it was typed: {:?} {} {}",
        hole,
        app.cut_x,
        app.message
    );
    Ok(())
}

fn files_in(folder: &std::path::Path) -> usize {
    std::fs::read_dir(folder).map_or(0, |entries| entries.count())
}

#[test]
fn a_newcomer_pours_copper_and_prints_words_on_both_sides() -> Result<(), String> {
    let (mut app, ctx) = sample_in_settings()?;
    let pours = app.project.board.zones.len();
    type_into(&ctx, &mut app, "spinbutton:Pour X", "3")?;
    type_into(&ctx, &mut app, "spinbutton:Pour Y", "4")?;
    type_into(&ctx, &mut app, "spinbutton:Pour width", "10")?;
    type_into(&ctx, &mut app, "spinbutton:Pour height", "8")?;
    tap_id(&ctx, &mut app, "combobox:Pour connection")?;
    tap_id(&ctx, &mut app, "combobox:Pour connection")?;
    tap_id(&ctx, &mut app, "button:Pour copper")?;
    app.refresh();
    let zone = app
        .project
        .board
        .zones
        .get(pours)
        .ok_or_else(|| format!("one click pours one block of copper: {}", app.message))?;
    assert!(
        zone.outline.iter().all(|p| p.x >= 3.0 - 1e-9
            && p.x <= 13.0 + 1e-9
            && p.y >= 4.0 - 1e-9
            && p.y <= 12.0 + 1e-9)
            && zone.outline.iter().any(|p| (p.x - 13.0).abs() < 1e-9),
        "the copper covers the rectangle that was typed"
    );
    let words = app.project.board.texts.len();
    type_into(&ctx, &mut app, "textinput:what to print", "HELLO")?;
    type_into(&ctx, &mut app, "spinbutton:Text X", "5")?;
    type_into(&ctx, &mut app, "spinbutton:Text Y", "6")?;
    type_into(&ctx, &mut app, "spinbutton:Text size", "2")?;
    tap_id(&ctx, &mut app, "button:Bottom")?;
    tap_id(&ctx, &mut app, "button:Add label")?;
    let text = app
        .project
        .board
        .texts
        .get(words)
        .ok_or_else(|| format!("one click prints the words: {}", app.message))?;
    assert_eq!(text.text, "HELLO", "the words are the ones typed");
    assert!(
        (text.position.x - 5.0).abs() < 1e-9 && (text.size - 2.0).abs() < 1e-9,
        "the words sit where and as big as typed"
    );
    assert_eq!(
        text.layer,
        pcbcraft_doc::Layer::Back,
        "Bottom puts the words on the back"
    );
    tap_id(&ctx, &mut app, "button:Top")?;
    tap_id(&ctx, &mut app, "button:Add label")?;
    assert_eq!(
        app.project.board.texts.last().map(|t| t.layer),
        Some(pcbcraft_doc::Layer::Front),
        "Top puts the next words on the front"
    );
    Ok(())
}

#[test]
fn a_newcomer_makes_every_file_the_factory_asks_for() -> Result<(), String> {
    let (mut app, ctx) = sample_in_settings()?;
    let folder = std::env::temp_dir().join(format!("pcbcraftmake-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    type_into(
        &ctx,
        &mut app,
        "textinput:folder for the files (blank means \"outputs\" beside the desig",
        &folder.to_string_lossy(),
    )?;
    assert_eq!(
        app.make_dir,
        folder.to_string_lossy(),
        "the folder is the one typed"
    );
    let mut seen = 0;
    for button in [
        "button:Copper & drill files",
        "button:Parts list (CSV)",
        "button:Net list",
        "button:Pick and place file",
        "button:Design report",
        "button:Print art, bottom silkscreen",
    ] {
        tap_id(&ctx, &mut app, button)?;
        let now = files_in(&folder);
        assert!(now > seen, "{button} leaves new files: {}", app.message);
        seen = now;
    }
    let _ = std::fs::remove_dir_all(&folder);
    Ok(())
}

#[test]
fn a_newcomer_plans_a_panel_with_their_own_gaps() -> Result<(), String> {
    let (mut app, ctx) = sample_in_settings()?;
    type_into(&ctx, &mut app, "spinbutton:Copies across", "3")?;
    type_into(&ctx, &mut app, "spinbutton:Copies down", "2")?;
    type_into(&ctx, &mut app, "spinbutton:Gap between copies", "2")?;
    type_into(&ctx, &mut app, "spinbutton:Rail width", "5")?;
    tap_id(&ctx, &mut app, "button:Preview panel")?;
    assert_eq!(
        app.panel_plan.get("copies").and_then(Value::as_u64),
        Some(6),
        "three across and two down is six copies: {}",
        app.message
    );
    Ok(())
}

#[test]
fn a_newcomer_brings_in_a_drawing_and_uses_its_edge_as_the_board() -> Result<(), String> {
    let (mut app, ctx) = sample_in_settings()?;
    let file = std::env::temp_dir().join(format!("pcbcraftdraw-{}.svg", std::process::id()));
    std::fs::write(
        &file,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"40\" height=\"30\" viewBox=\"0 0 40 30\"><path d=\"M0 0 L40 0 L40 30 L0 30 Z\" fill=\"none\" stroke=\"black\"/></svg>",
    )
    .map_err(|e| e.to_string())?;
    type_into(
        &ctx,
        &mut app,
        "textinput:drawing file path (.svg)",
        &file.to_string_lossy(),
    )?;
    assert_eq!(
        app.svg_path,
        file.to_string_lossy(),
        "the path is the one typed"
    );
    type_into(&ctx, &mut app, "spinbutton:Cutout tool radius", "2.5")?;
    assert!(
        (app.cutout_radius - 2.5).abs() < 1e-9,
        "the cutout tool keeps the typed radius"
    );
    tap_id(&ctx, &mut app, "button:Import drawing")?;
    let said_import = app.message.clone();
    tap_id(&ctx, &mut app, "button:Import as board edge")?;
    assert!(
        !said_import.is_empty() && !app.message.is_empty(),
        "the app answers both imports in words"
    );
    let _ = std::fs::remove_file(&file);
    Ok(())
}

#[test]
fn a_newcomer_changes_how_the_app_looks() -> Result<(), String> {
    let (mut app, ctx) = sample_in_settings()?;
    tap_id(&ctx, &mut app, "button:Light")?;
    assert!(!app.preferences.dark, "Light switches the app to light");
    let grid = app.preferences.grid;
    tap_id(&ctx, &mut app, "checkbox:Show grid")?;
    assert_ne!(app.preferences.grid, grid, "the grid tick flips");
    for (button, labels) in [
        ("button:Pads: net", PadLabels::Net),
        ("button:Pads: number", PadLabels::Number),
        ("button:Pads: off", PadLabels::Off),
    ] {
        tap_id(&ctx, &mut app, button)?;
        assert_eq!(
            app.preferences.pad_labels, labels,
            "{button} picks what is drawn on each pad"
        );
    }
    tap_id(&ctx, &mut app, "colorwell:Accent color")?;
    assert!(
        paints_text(&ctx, &mut app, "Accent color")?,
        "the accent colour row stays on the page"
    );
    tap_id(&ctx, &mut app, "button:#%")?;
    assert!(
        paints_text(&ctx, &mut app, "%")?,
        "the status bar always shows how far the drawing is zoomed"
    );
    Ok(())
}

#[test]
fn a_newcomer_jumps_from_settings_to_the_list_of_findings() -> Result<(), String> {
    let (mut app, ctx) = sample_in_settings()?;
    tap_id(&ctx, &mut app, "button:# electrical · # board >")?;
    assert_eq!(
        app.tab,
        Tab::Checks,
        "the findings button opens the checks tab"
    );
    Ok(())
}

#[test]
fn a_newcomer_decides_how_much_the_assistant_may_change() -> Result<(), String> {
    let (mut app, ctx) = sample_in_settings()?;
    app.preferences.assistant_enabled = false;
    tap_id(&ctx, &mut app, "checkbox:Show assistant")?;
    assert!(
        app.preferences.assistant_enabled,
        "ticking the box brings the assistant settings back"
    );
    for (button, mode) in [
        (
            "radiobutton:Auto-allow design edits",
            pcbcraft_assistant::Mode::Edits,
        ),
        (
            "radiobutton:Allow everything",
            pcbcraft_assistant::Mode::All,
        ),
        ("radiobutton:Ask every time", pcbcraft_assistant::Mode::Ask),
    ] {
        tap_id(&ctx, &mut app, button)?;
        assert_eq!(
            app.preferences.assistant_mode, mode,
            "{button} sets the permission level"
        );
    }
    type_into(&ctx, &mut app, "spinbutton:Changes per message", "7")?;
    assert_eq!(
        app.preferences.assistant_turn_limit, 7,
        "the typed limit sticks"
    );
    app.preferences
        .assistant_always_allow
        .insert("local".into(), vec!["board.resize".into()]);
    tap_id(&ctx, &mut app, "button:Forget Always allow")?;
    assert!(
        app.preferences.assistant_always_allow.is_empty(),
        "one click forgets every standing allowance"
    );
    Ok(())
}
