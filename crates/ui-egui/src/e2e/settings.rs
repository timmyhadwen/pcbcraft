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

/// Click the board width box, type `text`, press Enter, and give the frame
/// that follows.
fn type_into_the_width_box(
    ctx: &egui::Context,
    app: &mut App,
    shown: &str,
    text: &str,
) -> Result<(), String> {
    authoring::tests::tap(ctx, app, shown)?;
    let enter = egui::Event::Key {
        key: egui::Key::Enter,
        physical_key: Some(egui::Key::Enter),
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    };
    frame(
        ctx,
        app,
        vec![egui::Event::Text(text.to_string()), enter],
        egui::Modifiers::NONE,
    );
    frame(ctx, app, vec![], egui::Modifiers::NONE);
    Ok(())
}

#[test]
fn a_newcomer_types_a_decimal_comma_in_german() -> Result<(), String> {
    let mut app = fresh();
    let ctx = egui::Context::default();
    app.set_language("de");
    app.change_tab(Tab::Settings);
    app.width = 12.5;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        paints_text(&ctx, &mut app, "12,5")?,
        "a German install shows the board width with a comma"
    );
    type_into_the_width_box(&ctx, &mut app, "12,5", "1,6")?;
    assert_eq!(app.width, 1.6, "typing 1,6 stores 1.6");
    assert!(paints_text(&ctx, &mut app, "1,6")?, "the box shows 1,6");
    type_into_the_width_box(&ctx, &mut app, "1,6", "2.4")?;
    assert_eq!(app.width, 2.4, "typing a point from habit also works");
    assert!(paints_text(&ctx, &mut app, "2,4")?, "the box shows 2,4");
    Ok(())
}
