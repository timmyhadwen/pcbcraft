//! Assistant conversation model: backend seam, transcript, and turn accounting (no UI).
mod api;
mod backend;
mod prompt;
mod script;
mod text;
mod transcript;
mod turn;

pub use api::{
    ApiBackend, ApiConfig, Protocol, MAX_HISTORY, MAX_REQUESTS_PER_TURN as MAX_API_REQUESTS_PER_TURN, MAX_TOOL_TEXT, anthropic_model_list,
    anthropic_tools, endpoint, http_error, key_safe_for, model_list, openai_tools, test_anthropic_endpoint, test_endpoint,
};
pub use prompt::SYSTEM_NOTE;
pub use backend::{
    AuthMethod, AuthKind, ChatBackend, Event, HttpFn, HttpRequest, ModelInfo, PermissionOption, PromptPart, Secrets,
    StartContext, ToolStatus, base64, content_blocks,
};
pub use script::ScriptBackend;
pub use text::{check_prompt_length, prompt_too_long_reason, MAX_PROMPT};
pub use transcript::{Item, Transcript, MAX_ITEMS};
pub use turn::{Turn, ToolRequest, DEFAULT_TURN_LIMIT, MAX_REQUESTS_PER_TURN};

#[cfg(test)]
mod api_tests;
