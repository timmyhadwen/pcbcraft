**Tags:** `track:ai-handoff` `kind:chore` `area:ui-shell` `area:ci` `crate:text` `crate:ui-egui` `conflict:parallel-safe` `size:s` `status:needs-owner-test` `ready` `research:done` `enrich:done`
## What
Tests that prove Gerber and drill files, the BOM, centroid and MCP results are identical whatever the language, and a guard that the io and mcp crates never use the localized formatter. Child 4 of #601.

## Why
Output files must not change with the UI language (owner rule in #601).

## How
Base: fork/main at a0b7682. Blocked on #661. Gerber is produced by `gerber_files` in `crates/io/src/fab_export.rs` (and the `gerber.export` MCP tool). Test in `crates/ui-egui/src/e2e/artifacts.rs`: build the sample design, call `set_language("en")` then export through `App::tool_call("gerber.export", ...)` into a temp directory, repeat with the test German language, compare every file byte for byte. Guard: a test in `crates/text` or the ui-egui guard module that reads `crates/io/src` and `crates/mcp/src` and fails on the names `fmt_ui`, `fmt_count_ui`, `fmt_with_unit`, `fmt_date_ui` or `pcbcraft_text`. `crates/io` and `crates/mcp` do not depend on `pcbcraft-text` today; keep it so.

## Acceptance
1. `a_gerber_export_made_in_german_is_byte_identical_to_one_made_in_english`.
2. `drill_and_bom_exports_are_byte_identical_across_languages`.
3. `io_and_mcp_never_use_the_localized_formatters`.

### Rules for whoever picks this up
- Work on a branch of your fork off current fork main; one PR per feature.
- New code and PR text may not name other EDA tools.
- Typed engine commands with bounded validation; no unwrap/expect/panic anywhere, tests included.
- `cargo xtask ci` must be green before pushing.
- Every new control gets a tooltip; panels keep real margins and padding.
- End-to-end tests in crates/ui-egui are named like `a_newcomer_*` and must read like a story for someone who has never seen the app.

### Claiming
Comment `CLAIM <agent-name>` to take this once it is tagged `ready`. If a claim is older than 5 days with no PR, anyone may ask to take it over.


