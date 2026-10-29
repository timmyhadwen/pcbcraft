# Getting started with PCBCraft

## Getting started

PCBCraft on the fork is the codebase of record until the maintainer opens an
upstream pull request. Clone [wdunn001/pcbcraft](https://github.com/wdunn001/pcbcraft)
branch `main`, install Rust 1.95+ and the window libraries listed in
[README.md](../README.md), then run the sample desktop session:

```sh
git clone https://github.com/wdunn001/pcbcraft.git
cd pcbcraft
cargo run -p pcbcraft -- --sample
```

For headless automation, build the CLI and start MCP on stdio:

```sh
cargo build -p pcbcraft-cli
cargo run -p pcbcraft-cli -- mcp
```

Agents and contributors branch from fork `main`, merge the latest `origin/main`
before the final push, and push a feature branch only. Green GitHub Actions on
that branch is the required proof; local `cargo xtask ci` is optional on the
fork. Full pipeline rules live in [CONTRIBUTING.md](../CONTRIBUTING.md) and
[AGENTS.md](../AGENTS.md). Fork onboarding summary:
[issue #1](https://github.com/wdunn001/pcbcraft/issues/1).

Your first edit in the UI: load the LED example, place symbols with **A**, wire
with **W**, **Sync to PCB**, route with **X**, then open **Checks**. Step-by-step
detail is under **First circuit** in the README.

## Details

| Goal | Where to read |
| --- | --- |
| What is implemented today | [README.md](../README.md) **What works** |
| Planned work and checkboxes | [ROADMAP.md](../ROADMAP.md) |
| Layering and MCP-first design | [architecture.md](architecture.md) |
| MCP tools, batches and live bridge | [mcp.md](mcp.md) |
| CI jobs and fork branch pushes | [ci.md](ci.md) |
| Agent roles and leases | [agents/README.md](agents/README.md) |
| KiCad library reading limits | [kicad-libraries.md](kicad-libraries.md) |

Engine and MCP surface area on fork `main`: **306** typed engine commands (each
also a direct MCP tool) plus **53** session, library, export and inspection MCP
tools (**359** tools total). Counts come from `pcbcraft_engine::commands()` and
`tool_list()` in the MCP crate.

## Limits

PCBCraft is a first working foundation, not a full production EDA replacement.
Routing defaults to simple orthogonal pad-to-pad paths; optional octilinear modes
avoid some obstacles on one layer but do not replace an interactive router. Checks
are initial ERC/DRC, not a complete manufacturing sign-off. Browser host
open/save/export and many roadmap items in ROADMAP.md remain open.

Do not run the full workspace CI suite on a shared laptop if it freezes the
machine; push the branch and read the Actions result instead.

## See also

- [validation.md](validation.md) for how the initial subset was verified
- [ATTRIBUTION.md](../ATTRIBUTION.md) for GPL adaptations and library licenses
- Upstream tracker: [tangency-labs/pcbcraft](https://github.com/tangency-labs/pcbcraft)
