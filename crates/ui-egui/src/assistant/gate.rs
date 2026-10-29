//! The first, small gate: which tool calls run at once and which wait for the user.
//!
//! Context: the permission modes, previews and refusals arrive with the gate in the assistant
//! crate; until then every call that is not a read waits for an answer on a card.
//! Rules: a name this list does not know as a read always waits.
//! Do not: add a tool here that writes a file or edits the design to the read list.

use serde_json::Value;

/// Tools that only read; they run without asking.
const READS: &[&str] = &[
    "list_commands",
    "inspect_design",
    "checks",
    "board_mesh",
    "library.catalog",
    "library.search",
    "library.inspect_symbol",
    "library.inspect_footprint",
    "library.sources",
    "library.compare",
    "design.recents",
];

/// True when the call must wait for the user.
pub(super) fn needs_answer(tool: &str) -> bool {
    !READS.contains(&tool)
}

/// True when the call can change the design (it counts toward the turn's changes).
pub(super) fn changes(tool: &str) -> bool {
    needs_answer(tool) && tool != "screenshot"
}

/// The name "Always allow" remembers: the command for `execute`, else the tool name.
pub(super) fn tool_key(tool: &str, args: &Value) -> String {
    if tool == "execute"
        && let Some(command) = args.get("command").and_then(Value::as_str)
    {
        return command.to_string();
    }
    tool.to_string()
}

/// A plain title for the approval card.
pub(super) fn describe(tool: &str, args: &Value) -> String {
    let text = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
    match tool {
        "batch" => {
            let n = args
                .get("commands")
                .and_then(Value::as_array)
                .map_or(0, Vec::len);
            format!("Batch of {n} commands")
        }
        "execute" => {
            let command = text(args, "command").unwrap_or_default();
            let params = args.get("params").cloned().unwrap_or(Value::Null);
            describe_command(&command, &params)
        }
        "undo" => "Undo the last change".into(),
        "redo" => "Redo the last change".into(),
        "new_design" => "Start a new empty design".into(),
        "sample_design" => "Load the example design".into(),
        "open" | "save" => {
            let path = text(args, "path").unwrap_or_default();
            let name = path.rsplit(['/', '\\']).next().unwrap_or("").to_string();
            let verb = if tool == "open" {
                "Open"
            } else {
                "Save the design to"
            };
            format!("{verb} {name}")
        }
        other => describe_command(other, args),
    }
}

fn describe_command(command: &str, params: &Value) -> String {
    let get = |k: &str| params.get(k).and_then(Value::as_str);
    match command {
        "component.add" => {
            let kind = get("kind").unwrap_or("part");
            match (get("reference"), get("value")) {
                (Some(r), Some(v)) => format!("Add {kind} {r} ({v})"),
                (Some(r), None) => format!("Add {kind} {r}"),
                _ => format!("Add a {kind}"),
            }
        }
        other => other.replace(['.', '_'], " "),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_run_and_everything_else_waits() {
        assert!(!needs_answer("inspect_design"));
        assert!(needs_answer("execute"));
        assert!(needs_answer("save"));
        assert!(needs_answer("not_a_tool"));
    }

    #[test]
    fn the_card_title_names_the_part() {
        let args = json!({"command":"component.add","params":{"kind":"resistor","reference":"R1","value":"10k"}});
        assert_eq!(describe("execute", &args), "Add resistor R1 (10k)");
        assert_eq!(tool_key("execute", &args), "component.add");
        assert_eq!(
            describe("batch", &json!({"commands":[{},{}]})),
            "Batch of 2 commands"
        );
    }
}
