use super::super::*;
use super::support::*;
use crate::authoring::Draft;
use pcbcraft_library::Graphic;

#[test]
fn a_newcomer_searches_for_a_part_and_it_is_ready_to_place() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Add part")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        paints_text(&ctx, &mut app, "Search parts by name")?,
        "the part search opens with a box that says what to type"
    );
    authoring::tests::tap(&ctx, &mut app, "Search parts by name")?;
    for letter in "resistor".chars() {
        frame(
            &ctx,
            &mut app,
            vec![egui::Event::Text(letter.into())],
            egui::Modifiers::NONE,
        );
    }
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert_eq!(
        app.part_search, "resistor",
        "typing in the search box arrives letter by letter"
    );
    assert!(
        paints_text(&ctx, &mut app, "Resistor")?,
        "the matching part stays in the list"
    );
    assert!(
        !paints_text(&ctx, &mut app, "Capacitor")?,
        "parts that do not match the search disappear from the list"
    );
    authoring::tests::tap(&ctx, &mut app, "Resistor")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        matches!(&app.tool, Tool::Place(kind) if kind == "resistor"),
        "picking the part arms the schematic to place it, got {:?}",
        app.tool
    );
    assert_eq!(
        app.tab,
        Tab::Schematic,
        "the sheet comes forward to receive it"
    );
    Ok(())
}
#[test]
fn a_newcomer_sees_what_each_part_is_and_hides_a_whole_set() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    let r1 = app
        .project
        .components
        .iter()
        .find(|c| c.reference == "R1")
        .map(|c| c.id)
        .ok_or("the practice board should have a resistor")?;
    app.tool_call(
        "component.field",
        json!({"id": r1, "key": "Description", "value": "blinky red"}),
    );
    app.tool_call(
        "library.symbol.upsert",
        json!({"definition": {
            "name": "SetR",
            "reference": "R",
            "value": "10k",
            "description": "thin film",
            "keywords": "resistor trim",
            "footprint": "",
            "set": "Power bits",
            "pins": [{"number": "1", "name": "", "position": {"x": -5.0, "y": 0.0},
                "angle": 0.0, "length": 2.5, "electrical_type": "passive",
                "unit": 1, "body_style": 0}],
            "graphics": [],
            "extends": null,
            "raw_source": "",
            "unsupported": []
        }}),
    );
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        paints_text(&ctx, &mut app, "    blinky red · built in")?,
        "a part row says what the part is and where it came from: {}",
        app.message
    );
    authoring::tests::tap(&ctx, &mut app, "Add part")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        paints_text(&ctx, &mut app, "Power bits (1 part)")?,
        "library parts are grouped under their set name: {}",
        app.message
    );
    assert!(
        paints_text(&ctx, &mut app, "thin film · search words: resistor · trim")?,
        "each library part is a card that says what it is: {}",
        app.message
    );
    assert!(
        paints_text(&ctx, &mut app, "SetR")?,
        "the part itself is offered under its set: {}",
        app.message
    );
    authoring::tests::tap(&ctx, &mut app, "Power bits (1 part)")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        !paints_text(&ctx, &mut app, "SetR")?,
        "unticking the set hides every part inside it: {}",
        app.message
    );
    authoring::tests::tap(&ctx, &mut app, "Power bits (1 part)")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        paints_text(&ctx, &mut app, "SetR")?,
        "ticking it again brings the group back: {}",
        app.message
    );
    Ok(())
}

#[test]
fn a_newcomer_draws_a_two_pin_symbol_with_the_mouse() -> Result<(), String> {
    let (shared, mut app, ctx) = authoring::tests::editor("symbol-editor")?;
    authoring::tests::clear_symbol_draft(&mut app);
    authoring::tests::tap(&ctx, &mut app, "Fit")?;
    authoring::tests::tap(&ctx, &mut app, "Pin")?;
    authoring::tests::library_world_click(&ctx, &mut app, Point::new(-2.0, 0.0))?;
    authoring::tests::library_world_click(&ctx, &mut app, Point::new(2.0, 0.0))?;
    authoring::tests::tap(&ctx, &mut app, "Draw rectangle")?;
    authoring::tests::library_world_click(&ctx, &mut app, Point::new(-1.0, -1.0))?;
    authoring::tests::library_world_click(&ctx, &mut app, Point::new(1.0, 1.0))?;
    let draft = authoring::tests::draft_symbol(&app)?;
    assert_eq!(draft.pins.len(), 2, "two pin clicks place two pins");
    assert!(
        draft
            .pins
            .iter()
            .any(|p| p.position == Point::new(-2.0, 0.0)),
        "pin one sits on the first grid click"
    );
    assert!(
        draft
            .pins
            .iter()
            .any(|p| p.position == Point::new(2.0, 0.0)),
        "pin two sits on the second grid click"
    );
    let rect = draft
        .graphics
        .iter()
        .find_map(|g| match &g.primitive {
            Graphic::Rectangle { start, end, .. } => Some((*start, *end)),
            _ => None,
        })
        .ok_or("rectangle body missing")?;
    assert_eq!(rect.0, Point::new(-1.0, -1.0));
    assert_eq!(rect.1, Point::new(1.0, 1.0));
    authoring::tests::tap(&ctx, &mut app, "Save symbol to project library")?;
    let session = shared.lock().map_err(|_| "session unavailable")?;
    assert_eq!(session.project.library.symbols.len(), 1);
    assert_eq!(session.project.library.symbols[0].pins.len(), 2);
    Ok(())
}

#[test]
fn a_newcomer_drags_a_pad_into_place() -> Result<(), String> {
    let (shared, mut app, ctx) = authoring::tests::editor("footprint-editor")?;
    authoring::tests::tap(&ctx, &mut app, "Fit")?;
    authoring::tests::tap(&ctx, &mut app, "Select")?;
    authoring::tests::tap(&ctx, &mut app, "Save footprint to project library")?;
    let before = app
        .library
        .editor
        .draft
        .as_ref()
        .and_then(|d| match d {
            Draft::Footprint(f) => f.pads.get(1).map(|p| p.position),
            _ => None,
        })
        .ok_or("pad 2 missing from the default footprint")?;
    authoring::tests::library_world_drag(
        &ctx,
        &mut app,
        before,
        before.offset(Point::new(1.0, 0.0)),
    )?;
    let after_drag = app
        .library
        .editor
        .draft
        .as_ref()
        .and_then(|d| match d {
            Draft::Footprint(f) => f.pads.get(1).map(|p| p.position),
            _ => None,
        })
        .ok_or("draft lost after drag")?;
    assert!(
        (after_drag.x - before.x - 1.0).abs() < 1e-6,
        "pad 2 moves one 1.0 mm step on the 0.5 mm grid"
    );
    authoring::tests::tap(&ctx, &mut app, "Save footprint to project library")?;
    {
        let session = shared.lock().map_err(|_| "session unavailable")?;
        let saved = &session.project.library.footprints[0].pads[1].position;
        assert!((saved.x - after_drag.x).abs() < 1e-6);
    }
    app.tool_call("undo", json!({}));
    let session = shared.lock().map_err(|_| "session unavailable")?;
    assert!(
        (session.project.library.footprints[0].pads[1].position.x - before.x).abs() < 1e-6,
        "undo puts pad 2 back where it started"
    );
    Ok(())
}

#[test]
fn library_editor_canvas_and_form_show_the_same_pin_position_after_drag() -> Result<(), String> {
    let (_shared, mut app, ctx) = authoring::tests::editor("symbol-editor")?;
    authoring::tests::tap(&ctx, &mut app, "Fit")?;
    authoring::tests::tap(&ctx, &mut app, "Select")?;
    let start = authoring::tests::draft_symbol(&app)?.pins[0].position;
    let target = canvas::snap(start.offset(Point::new(1.0, 0.5)));
    authoring::tests::library_world_drag(&ctx, &mut app, start, target)?;
    let draft_pos = authoring::tests::draft_symbol(&app)?.pins[0].position;
    assert_eq!(draft_pos, target, "drag updates the draft pin position");
    authoring::tests::tap(&ctx, &mut app, "Pin 1 · unit 1")?;
    authoring::tests::frame(&ctx, &mut app, vec![]);
    let shown = authoring::tests::draft_symbol(&app)?.pins[0].position;
    assert_eq!(
        shown, draft_pos,
        "the form reads the same coordinates the canvas wrote"
    );
    Ok(())
}
