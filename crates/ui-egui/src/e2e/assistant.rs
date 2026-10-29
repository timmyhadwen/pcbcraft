//! Stories for the Assistant pane, driven with a fake model (no network).
use super::super::*;
use super::support::*;
use pcbcraft_assistant::{ChatBackend, Event, PromptPart, ToolRequest};
use std::sync::mpsc::{Receiver, Sender, channel};

/// A model that answers any message with one request to add a 10k resistor.
struct FakeModel {
    tools: Sender<ToolRequest>,
    events: Vec<Event>,
    answers: Vec<Receiver<Value>>,
}

impl FakeModel {
    fn new(tools: Sender<ToolRequest>) -> Self {
        Self {
            tools,
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
        let (reply, answer) = channel();
        let _ = self.tools.send(ToolRequest {
            name: "execute".into(),
            args: json!({"command":"component.add","params":{
                "kind":"resistor","reference":"R1","value":"10k","position":{"x":20.0,"y":20.0}}}),
            reply,
        });
        self.answers.push(answer);
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

fn with_fake_model() -> App {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.preferences.aside_lower_tab = 1;
    let model = FakeModel::new(app.assistant.start_context(std::path::PathBuf::new()).tools.unwrap_or_else(|| channel().0));
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
    assert!(paints_text(&ctx, &mut app, "Add resistor R1 (10k)")?, "the card names the change");
    assert!(!has_r1(&app), "nothing runs before the user answers");
    authoring::tests::tap(&ctx, &mut app, "Allow")?;
    for _ in 0..4 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    assert!(has_r1(&app), "the part is on the sheet");
    assert!(paints_text(&ctx, &mut app, "Ready")?, "the model finished its turn");
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
