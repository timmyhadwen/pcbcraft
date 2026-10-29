//! Stories for the Assistant pane, driven with a fake model (no network).
use super::super::*;
use super::support::*;
use pcbcraft_assistant::{ChatBackend, Event, PromptPart, ToolRequest};
use std::sync::mpsc::{Receiver, Sender, channel};

/// A model that answers any message with one request to add a 10k resistor.
struct FakeModel {
    tools: Sender<ToolRequest>,
    calls: Vec<(String, Value)>,
    events: Vec<Event>,
    answers: Vec<Receiver<Value>>,
}

impl FakeModel {
    fn new(tools: Sender<ToolRequest>, calls: Vec<(String, Value)>) -> Self {
        Self {
            tools,
            calls,
            events: vec![Event::Ready {
                agent: "fake".into(),
                images: false,
            }],
            answers: Vec::new(),
        }
    }
}

impl ChatBackend for FakeModel {
    fn poll(&mut self) -> Vec<Event> {
        let mut out = std::mem::take(&mut self.events);
        let mut waiting = Vec::new();
        for answer in self.answers.drain(..) {
            match answer.try_recv() {
                Ok(_) => out.push(Event::TurnEnded {
                    reason: String::new(),
                }),
                Err(_) => waiting.push(answer),
            }
        }
        self.answers = waiting;
        out
    }
    fn prompt(&mut self, _: &[PromptPart]) {
        for (name, args) in self.calls.clone() {
            let (reply, answer) = channel();
            let _ = self.tools.send(ToolRequest { name, args, reply });
            self.answers.push(answer);
        }
        self.events.push(Event::Text {
            thought: false,
            text: "I will add the resistor.".into(),
        });
    }
    fn cancel(&mut self) {}
    fn new_session(&mut self) {}
    fn answer_permission(&mut self, _: u64, _: Option<&str>) {}
    fn authenticate(&mut self, _: &str) {}
    fn set_model(&mut self, _: &str) {}
    fn set_mode(&mut self, _: &str) {}
    fn shutdown(&mut self, _: bool) {}
}

fn add_part(reference: &str, value: &str, x: f64) -> (String, Value) {
    (
        "execute".into(),
        json!({"command":"component.add","params":{
            "kind":"resistor","reference":reference,"value":value,"position":{"x":x,"y":20.0}}}),
    )
}

fn with_fake_model() -> App {
    with_calls(vec![add_part("R1", "10k", 20.0)])
}

fn with_calls(calls: Vec<(String, Value)>) -> App {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.preferences.aside_lower_tab = 1;
    let model = FakeModel::new(
        app.assistant
            .start_context(std::path::PathBuf::new())
            .tools
            .unwrap_or_else(|| channel().0),
    );
    app.assistant_use(Box::new(model));
    app
}

fn has_r1(app: &App) -> bool {
    app.project.components.iter().any(|c| c.reference == "R1")
}

#[test]
fn a_newcomer_opens_the_assistant_and_is_told_how_to_connect_a_model() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Assistant")?;
    assert!(paints_text(&ctx, &mut app, "No model is connected")?);
    assert!(paints_text(&ctx, &mut app, "Nothing leaves this computer")?);
    Ok(())
}

#[test]
fn a_newcomer_asks_the_assistant_to_add_a_resistor() -> Result<(), String> {
    let mut app = with_fake_model();
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.assistant.input = "add a 10k resistor".into();
    authoring::tests::tap(&ctx, &mut app, "Send")?;
    for _ in 0..4 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    assert!(
        paints_text(&ctx, &mut app, "Add resistor R1 (10k)")?,
        "the card names the change"
    );
    assert!(!has_r1(&app), "nothing runs before the user answers");
    authoring::tests::tap(&ctx, &mut app, "Allow")?;
    for _ in 0..4 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    assert!(has_r1(&app), "the part is on the sheet");
    assert!(
        paints_text(&ctx, &mut app, "Ready")?,
        "the model finished its turn"
    );
    Ok(())
}

#[test]
fn a_newcomer_denies_a_change_and_the_design_stays_as_it_was() -> Result<(), String> {
    let mut app = with_fake_model();
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.assistant.input = "add a 10k resistor".into();
    authoring::tests::tap(&ctx, &mut app, "Send")?;
    for _ in 0..4 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    authoring::tests::tap(&ctx, &mut app, "Deny")?;
    for _ in 0..4 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    assert!(!has_r1(&app));
    assert!(paints_text(&ctx, &mut app, "Denied")?);
    Ok(())
}

fn settle(ctx: &egui::Context, app: &mut App) {
    for _ in 0..4 {
        frame(ctx, app, vec![], egui::Modifiers::NONE);
    }
}

#[test]
fn a_newcomer_allows_two_changes_and_takes_them_back_with_one_undo() -> Result<(), String> {
    let mut app = with_calls(vec![
        add_part("R1", "10k", 20.0),
        add_part("R2", "4k7", 30.0),
    ]);
    let ctx = egui::Context::default();
    settle(&ctx, &mut app);
    app.assistant.input = "add two resistors".into();
    authoring::tests::tap(&ctx, &mut app, "Send")?;
    settle(&ctx, &mut app);
    assert!(paints_text(&ctx, &mut app, "Add resistor R1 (10k)")?);
    assert!(paints_text(&ctx, &mut app, "Add resistor R2 (4k7)")?);
    assert!(
        paints_text(&ctx, &mut app, "adds R1")?,
        "the card previews the change"
    );
    for _ in 0..2 {
        authoring::tests::tap(&ctx, &mut app, "Allow")?;
        settle(&ctx, &mut app);
    }
    assert!(has_r1(&app));
    assert_eq!(app.project.components.len(), 2);
    authoring::tests::tap(&ctx, &mut app, "Undo")?;
    settle(&ctx, &mut app);
    assert!(
        app.project.components.is_empty(),
        "one Undo takes back both parts"
    );
    Ok(())
}

#[test]
fn a_newcomer_sees_a_malformed_change_refused_with_the_reason() -> Result<(), String> {
    let bad = (
        "execute".to_string(),
        json!({"command":"component.add","params":{"kind":"resistor","position":"nowhere"}}),
    );
    let mut app = with_calls(vec![bad]);
    let ctx = egui::Context::default();
    settle(&ctx, &mut app);
    app.assistant.input = "add a resistor".into();
    authoring::tests::tap(&ctx, &mut app, "Send")?;
    settle(&ctx, &mut app);
    assert!(
        !paints_text(&ctx, &mut app, "Allow")?,
        "a refused change has no Allow button"
    );
    assert!(app.project.components.is_empty());
    assert!(
        app.assistant
            .transcript
            .items
            .iter()
            .any(|i| matches!(i, pcbcraft_assistant::Item::Note { error: true, .. })),
        "the reason is in the conversation"
    );
    Ok(())
}
