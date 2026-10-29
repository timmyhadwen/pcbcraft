//! The assistant pane's state and its loop with a chat backend.
//!
//! Context: the right aside's Assistant tab talks to a [`pcbcraft_assistant::ChatBackend`].
//! The backend asks for design tools over a channel; this module answers them on the displayed
//! session through `pcbcraft_mcp::call_tool`, the same typed command surface the MCP server uses.
//! Rules: nothing here opens a network connection (the host passes an `HttpFn`); keys never enter
//! the design, the transcript or any log; reads run at once, everything else waits for an answer.
//! Keep the pane body in `pane.rs` a self-contained function so the dock work can move it.

mod cards;
mod gate;
pub(crate) mod pane;
mod text;

use super::App;
use pcbcraft_assistant::{
    ChatBackend, Event, HttpFn, Item, PromptPart, Secrets, StartContext, ToolRequest, Transcript,
    Turn,
};
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};

/// The tab label with the count of waiting questions.
pub(crate) fn tab_label(waiting: usize) -> String {
    text::tab_label(waiting)
}

/// Tooltip of the Assistant tab.
pub(crate) const TAB_TIP: &str = text::TAB_TIP;

/// What the desktop or web shell offers the assistant. `None` means the shell has no such thing.
#[derive(Clone, Default)]
pub struct Host {
    /// Sends one HTTP request. Absent in the browser build.
    pub http: Option<HttpFn>,
    /// The system keychain for model keys. Absent where the shell has none.
    pub secrets: Option<Arc<dyn Secrets>>,
}

/// A tool call held back until the user answers.
struct Waiting {
    id: u64,
    key: String,
    request: ToolRequest,
}

/// Everything the pane keeps between frames.
pub(crate) struct State {
    pub(crate) transcript: Transcript,
    pub(crate) input: String,
    pub(crate) host: Host,
    backend: Option<Box<dyn ChatBackend>>,
    ready: bool,
    busy: bool,
    turn: Option<Turn>,
    tools: (Sender<ToolRequest>, Receiver<ToolRequest>),
    waiting: Vec<Waiting>,
    next_id: u64,
    always: Vec<String>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            transcript: Transcript::default(),
            input: String::new(),
            host: Host::default(),
            backend: None,
            ready: false,
            busy: false,
            turn: None,
            tools: channel(),
            waiting: Vec::new(),
            next_id: 1,
            always: Vec::new(),
        }
    }
}

/// The word shown beside the state dot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    Off,
    Starting,
    Ready,
    Working,
    Waiting,
}

impl State {
    /// A start context for a backend that runs its own tool loop; tool calls come back to `poll`.
    pub(crate) fn start_context(&self, work_dir: std::path::PathBuf) -> StartContext {
        StartContext {
            work_dir,
            tools: Some(self.tools.0.clone()),
        }
    }

    pub(crate) fn phase(&self) -> Phase {
        if self.backend.is_none() {
            Phase::Off
        } else if !self.waiting.is_empty() {
            Phase::Waiting
        } else if self.busy {
            Phase::Working
        } else if self.ready {
            Phase::Ready
        } else {
            Phase::Starting
        }
    }

    /// Questions waiting for an answer (the count on the collapsed tab).
    pub(crate) fn waiting(&self) -> usize {
        self.waiting.len()
    }
}

impl App {
    /// Give the assistant what the shell offers (an HTTP sender, a keychain).
    pub fn with_host(mut self, host: Host) -> Self {
        self.assistant.host = host;
        self
    }

    /// Use `backend` for the conversation. It reports `Ready` when it can take a message.
    pub(crate) fn assistant_use(&mut self, backend: Box<dyn ChatBackend>) {
        self.assistant.backend = Some(backend);
        self.assistant.ready = false;
        self.assistant.busy = false;
    }

    /// Show or hide the Assistant tab (Ctrl+Shift+A).
    pub(crate) fn assistant_toggle(&mut self) {
        self.preferences.aside_lower_tab = u8::from(self.preferences.aside_lower_tab != 1);
    }

    /// Send the typed message, or queue it while the model is busy. Empty text is ignored.
    pub(crate) fn assistant_send(&mut self) {
        let text = self.assistant.input.trim().to_string();
        if text.is_empty() || self.assistant.backend.is_none() {
            return;
        }
        if let Err(reason) = pcbcraft_assistant::check_prompt_length(text.chars().count()) {
            self.assistant.transcript.push(Item::Note {
                text: reason,
                error: true,
            });
            return;
        }
        self.assistant.input.clear();
        if self.assistant.busy || !self.assistant.ready {
            self.assistant.transcript.push(Item::Queued { text });
            return;
        }
        self.assistant_prompt(text);
    }

    fn assistant_prompt(&mut self, text: String) {
        let revision = self.session.lock().map_or(0, |s| s.revision);
        self.assistant.turn = Some(Turn::new(revision, 0, text.clone()));
        self.assistant
            .transcript
            .push(Item::User { text: text.clone() });
        self.assistant.busy = true;
        if let Some(b) = self.assistant.backend.as_mut() {
            b.prompt(&[PromptPart::Text(text)]);
        }
    }

    /// Stop the model; every question waiting for the user is declined.
    pub(crate) fn assistant_stop(&mut self) {
        let waiting: Vec<u64> = self.assistant.waiting.iter().map(|w| w.id).collect();
        for id in waiting {
            self.assistant_answer(id, false, false);
        }
        if let Some(b) = self.assistant.backend.as_mut() {
            b.cancel();
        }
    }

    /// Start a fresh conversation.
    pub(crate) fn assistant_new_chat(&mut self) {
        self.assistant_stop();
        self.assistant.transcript.items.clear();
        self.assistant.busy = false;
        if let Some(b) = self.assistant.backend.as_mut() {
            b.new_session();
        }
    }

    /// Answer the approval card `id`: run the held call, or tell the model it was declined.
    pub(crate) fn assistant_answer(&mut self, id: u64, allow: bool, always: bool) {
        let Some(at) = self.assistant.waiting.iter().position(|w| w.id == id) else {
            return;
        };
        let held = self.assistant.waiting.remove(at);
        if allow && always {
            self.assistant.always.push(held.key.clone());
        }
        for item in &mut self.assistant.transcript.items {
            if let Item::Approval { id: x, answer, .. } = item
                && *x == id
            {
                *answer = Some(allow);
            }
        }
        if allow {
            self.assistant_run(held.request);
        } else {
            let _ = held.request.reply.send(json!({
                "isError": true,
                "content": [{"type": "text", "text": text::DENIED_NOTE}],
            }));
        }
    }

    fn assistant_run(&mut self, request: ToolRequest) {
        let result = pcbcraft_mcp::call_tool(&self.session, &request.name, &request.args);
        if let Some(turn) = self.assistant.turn.as_mut() {
            turn.requests += 1;
            if gate::changes(&request.name) {
                turn.changes += 1;
            }
        }
        self.refresh();
        let _ = request.reply.send(result);
    }

    /// Called once per frame: apply what the backend reported and answer its tool calls.
    pub(crate) fn assistant_poll(&mut self) {
        let events = self
            .assistant
            .backend
            .as_mut()
            .map(|b| b.poll())
            .unwrap_or_default();
        for event in events {
            match event {
                Event::Ready { .. } => self.assistant.ready = true,
                Event::TurnEnded { .. } => {
                    self.assistant.busy = false;
                    self.assistant.turn = None;
                }
                Event::Exited { .. } => {
                    self.assistant.backend = None;
                    self.assistant.busy = false;
                    self.assistant.ready = false;
                }
                other => self.assistant.transcript.on_event(other),
            }
        }
        while let Ok(request) = self.assistant.tools.1.try_recv() {
            let key = gate::tool_key(&request.name, &request.args);
            if !gate::needs_answer(&request.name) || self.assistant.always.contains(&key) {
                self.assistant_run(request);
                continue;
            }
            let id = self.assistant.next_id;
            self.assistant.next_id += 1;
            self.assistant.transcript.push(Item::Approval {
                id,
                title: gate::describe(&request.name, &request.args),
                answer: None,
            });
            self.assistant.waiting.push(Waiting { id, key, request });
        }
        if !self.assistant.busy
            && self.assistant.ready
            && let Some(at) = self
                .assistant
                .transcript
                .items
                .iter()
                .position(|i| matches!(i, Item::Queued { .. }))
            && let Item::Queued { text } = self.assistant.transcript.items.remove(at)
        {
            self.assistant_prompt(text);
        }
    }

    /// A design was opened or replaced: questions about the old one are declined.
    pub(crate) fn assistant_design_opened(&mut self) {
        if self.assistant.transcript.items.is_empty() && self.assistant.waiting.is_empty() {
            return;
        }
        let waiting: Vec<u64> = self.assistant.waiting.iter().map(|w| w.id).collect();
        for id in waiting {
            self.assistant_answer(id, false, false);
        }
        self.assistant.transcript.push(Item::Note {
            text: text::NEW_DESIGN_NOTE.into(),
            error: false,
        });
    }

    /// Undo the last change (the Undo button of a finished message).
    pub(crate) fn assistant_undo(&mut self) {
        let _: Value = pcbcraft_mcp::call_tool(&self.session, "undo", &json!({}));
        self.refresh();
    }
}
