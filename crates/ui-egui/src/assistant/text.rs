//! Every user-visible string of the assistant pane, as named constants.
//!
//! Context: one place to read and translate the pane's wording. When the shared text catalog
//! lands on main these constants move into it unchanged.
//! Rules: plain short sentences, no tool names of other programs, no em-dashes.

pub(super) const TAB: &str = "Assistant";
pub(super) const TAB_TIP: &str =
    "Ask a model to change the design. Every change waits for your answer.";
pub(super) const NEW_CHAT: &str = "New chat";
pub(super) const NEW_CHAT_TIP: &str = "Forget this conversation and start a fresh one.";
pub(super) const CLEAR: &str = "Clear";
pub(super) const CLEAR_TIP: &str = "Remove the messages shown here. The design is not touched.";
pub(super) const SEND: &str = "Send";
pub(super) const SEND_TIP: &str = "Send this message (Ctrl+Enter). While the model works it is queued.";
pub(super) const STOP: &str = "Stop";
pub(super) const STOP_TIP: &str = "Stop the model. Questions waiting for you are declined.";
pub(super) const ALLOW: &str = "Allow";
pub(super) const ALLOW_TIP: &str = "Run this change now. You can undo it.";
pub(super) const ALWAYS_ALLOW: &str = "Always allow";
pub(super) const ALWAYS_ALLOW_TIP: &str =
    "Run this kind of change without asking again in this session.";
pub(super) const DENY: &str = "Deny";
pub(super) const DENY_TIP: &str = "Do not run this change. The model is told so.";
pub(super) const UNDO: &str = "Undo";
pub(super) const UNDO_TIP: &str = "Take back the last change the model made.";
pub(super) const INPUT_HINT: &str = "Ask for a change, for example: add a 10k resistor";
pub(super) const INPUT_TIP: &str = "Type what you want changed. Ctrl+Enter sends.";
pub(super) const NO_MODEL_TITLE: &str = "No model is connected";
pub(super) const NO_MODEL_BODY: &str = "Nothing leaves this computer until you connect one. \
Open Settings, then Assistant, and enter the address and the model name.";
pub(super) const STATE_OFF: &str = "Not connected";
pub(super) const STATE_STARTING: &str = "Starting";
pub(super) const STATE_READY: &str = "Ready";
pub(super) const STATE_WORKING: &str = "Working";
pub(super) const STATE_WAITING: &str = "Waiting for you";
pub(super) const STATE_TIP: &str = "What the assistant is doing right now.";
pub(super) const QUEUED_TIP: &str = "Waiting for the model to finish its current answer.";
pub(super) const DENIED_NOTE: &str = "The user declined this change.";
pub(super) const NEW_DESIGN_NOTE: &str = "A different design was opened. Earlier changes belong to the old one.";

/// The tab label, with the count of questions waiting for an answer.
pub(super) fn tab_label(waiting: usize) -> String {
    if waiting == 0 {
        TAB.to_string()
    } else {
        format!("{TAB} ({waiting})")
    }
}
