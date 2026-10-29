use super::super::*;
use super::support::*;

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
fn a_newcomer_makes_a_qfn_from_its_datasheet_numbers() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(
        shared.clone(),
        Preferences::default(),
        None,
        Some("footprint-editor"),
    );
    app.refresh();
    let ctx = egui::Context::default();
    authoring::tests::tap(&ctx, &mut app, "Generate footprint")?;
    assert!(
        paints_text(&ctx, &mut app, "Type the numbers from the package drawing in the datasheet. Dimensions are millimetres.")?,
        "the form tells a newcomer what to type"
    );
    // The datasheet says: 0.5 mm pitch, 32 pins, 5 by 5 mm body.
    app.library.editor.wizard.family = "qfn";
    app.library.editor.wizard.name = "QFN32_5X5".into();
    app.library.editor.wizard.pitch = 0.5;
    app.library.editor.wizard.pin_count = 32;
    app.library.editor.wizard.body_length = 5.0;
    app.library.editor.wizard.body_width = 5.0;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        paints_text(&ctx, &mut app, "33 pads, with a thermal pad")?,
        "the preview counts the pads and shows the thermal pad"
    );
    authoring::tests::tap(&ctx, &mut app, "Create footprint")?;
    authoring::tests::tap(&ctx, &mut app, "Save footprint to project library")?;
    let session = shared.lock().map_err(|_| "session unavailable")?;
    let saved = session
        .project
        .library
        .footprints
        .iter()
        .find(|f| f.name == "QFN32_5X5")
        .ok_or("the library does not hold the new footprint")?;
    assert_eq!(saved.pads.len(), 33, "32 pins plus the thermal pad");
    assert!(saved.pads.iter().any(|p| p.number == "EP"));
    Ok(())
}

#[test]
fn a_newcomer_sees_why_impossible_numbers_cannot_make_a_footprint() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, Some("footprint-editor"));
    app.refresh();
    let ctx = egui::Context::default();
    authoring::tests::tap(&ctx, &mut app, "Generate footprint")?;
    app.library.editor.wizard.pitch = 40.0;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        paints_text(&ctx, &mut app, "pitch must be between 0.1 and 25.4 millimetres")?,
        "a pitch that no package has is explained in plain words"
    );
    Ok(())
}
