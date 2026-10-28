//! Reference-project parity suite (#293).
//!
//! Each case builds or loads a project, then snapshots stable outputs and
//! compares them with goldens under `tests/parity/<case>/`. Text outputs are
//! stored verbatim; binary outputs (PNG) are stored as a length plus FNV-1a
//! checksum summary.
//!
//! Bless goldens on purpose with `PCBCRAFT_BLESS=1 cargo test -p pcbcraft-engine
//! --test parity` or `cargo xtask bless-parity`. Never bless in CI.
//!
//! Not covered yet: Gerber and drill output. The Gerber writer is being fixed
//! (#351), so it is not frozen as golden truth. Only structural properties of
//! the stable KiCad export are asserted until the new writers land.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use pcbcraft_doc::Project;
use pcbcraft_engine::{Command, Session};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn golden_dir(case: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/parity")
        .join(case)
}
fn blessing() -> bool {
    std::env::var_os("PCBCRAFT_BLESS").is_some() && std::env::var_os("CI").is_none()
}

/// 64-bit FNV-1a, enough to detect any change to a binary output.
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
    })
}
fn binary_summary(bytes: &[u8]) -> String {
    format!("bytes: {}\nfnv1a64: {:016x}\n", bytes.len(), fnv1a(bytes))
}

fn round(value: &Value) -> Value {
    match value {
        Value::Number(n) if n.is_f64() => {
            let r = (n.as_f64().unwrap_or(0.0) * 10000.0).round() / 10000.0;
            json!(r)
        }
        Value::Array(a) => Value::Array(a.iter().map(round).collect()),
        Value::Object(o) => Value::Object(o.iter().map(|(k, v)| (k.clone(), round(v))).collect()),
        other => other.clone(),
    }
}
fn sorted(value: &Value) -> Value {
    match value {
        Value::Array(a) => {
            let mut items: Vec<Value> = a.iter().map(sorted).collect();
            items.sort_by_key(|v| v.to_string());
            Value::Array(items)
        }
        Value::Object(o) => Value::Object(o.iter().map(|(k, v)| (k.clone(), sorted(v))).collect()),
        other => other.clone(),
    }
}
/// Normalised checks: sorted findings, rounded numbers, no prose scope text.
fn normalised_checks(session: &Session) -> String {
    let mut checks = session.checks();
    if let Some(o) = checks.as_object_mut() {
        o.remove("scope");
    }
    let mut text = serde_json::to_string_pretty(&sorted(&round(&checks))).unwrap_or_default();
    text.push('\n');
    text
}

fn line_diff(expected: &str, actual: &str) -> String {
    let e: Vec<&str> = expected.lines().collect();
    let a: Vec<&str> = actual.lines().collect();
    let mut out = String::new();
    let mut shown = 0;
    for i in 0..e.len().max(a.len()) {
        let (x, y) = (e.get(i), a.get(i));
        if x != y {
            out.push_str(&format!(
                "  line {}:\n    golden: {}\n    actual: {}\n",
                i + 1,
                x.unwrap_or(&"<missing>"),
                y.unwrap_or(&"<missing>")
            ));
            shown += 1;
            if shown == 20 {
                out.push_str("  ... more differences omitted\n");
                break;
            }
        }
    }
    out
}

fn compare(case: &str, name: &str, actual: &str, failures: &mut Vec<String>) {
    let path = golden_dir(case).join(name);
    if blessing() {
        let _ = std::fs::create_dir_all(golden_dir(case));
        std::fs::write(&path, actual).expect("write golden");
        return;
    }
    match std::fs::read_to_string(&path) {
        Ok(expected) if expected == actual => {}
        Ok(expected) => failures.push(format!(
            "{case}/{name} differs:\n{}",
            line_diff(&expected, actual)
        )),
        Err(_) => failures.push(format!(
            "{case}/{name} has no golden (run with PCBCRAFT_BLESS=1)"
        )),
    }
}

fn run_script(session: &mut Session, file: &str) -> Value {
    let text = std::fs::read_to_string(root().join("examples").join(file)).expect("script");
    let script: Value = serde_json::from_str(&text).expect("script json");
    let commands: Vec<Command> =
        serde_json::from_value(script["commands"].clone()).expect("commands");
    session.batch(&commands, None).expect("batch")
}

fn load_example(file: &str) -> Session {
    let project = pcbcraft_io::load(&root().join("examples").join(file)).expect("load example");
    let mut session = Session::default();
    session.replace(project).expect("replace");
    session
}

/// Every stable text output of a session, by file name.
fn outputs(session: &Session) -> Vec<(String, String)> {
    let p = &session.project;
    let project_json = serde_json::to_string_pretty(p).expect("project json") + "\n";
    let png = |view: &str| binary_summary(&pcbcraft_io::preview_png(p, view).expect("png"));
    vec![
        ("project.json".into(), project_json),
        ("checks.json".into(), normalised_checks(session)),
        (
            "board.kicad_pcb".into(),
            pcbcraft_io::kicad_board(p).expect("kicad"),
        ),
        (
            "pcb.svg".into(),
            pcbcraft_io::svg(p, "pcb").expect("pcb svg"),
        ),
        (
            "schematic.svg".into(),
            pcbcraft_io::svg(p, "schematic").expect("schematic svg"),
        ),
        ("pcb.png.summary".into(), png("pcb")),
        ("schematic.png.summary".into(), png("schematic")),
    ]
}

fn check_case(case: &str, session: &Session, extra: Vec<(String, String)>) {
    let mut failures = vec![];
    for (name, text) in outputs(session).into_iter().chain(extra) {
        compare(case, &name, &text, &mut failures);
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

fn led_routed() -> (Session, Value, Value) {
    let mut session = Session::default();
    let build = run_script(&mut session, "led-indicator.json");
    let route = run_script(&mut session, "route-led.json");
    (session, build, route)
}

#[test]
fn parity_led_indicator_matches_goldens() {
    let (session, build, route) = led_routed();
    let findings = session.checks();
    // docs/validation.md records zero findings and zero airwires once routed.
    assert_eq!(findings["drc"], json!([]));
    assert_eq!(findings["airwires"], json!([]));
    let results = format!(
        "{}\n{}\n",
        serde_json::to_string_pretty(&build).unwrap_or_default(),
        serde_json::to_string_pretty(&route).unwrap_or_default()
    );
    check_case(
        "led-indicator",
        &session,
        vec![("command-results.json".into(), results)],
    );
}

#[test]
fn parity_led_indicator_unrouted_reports_airwires() {
    let mut session = Session::default();
    run_script(&mut session, "led-indicator.json");
    let airwires = session.checks()["airwires"].as_array().map_or(0, Vec::len);
    assert_eq!(airwires, 3, "docs/validation.md: three disconnected links");
    let mut failures = vec![];
    compare(
        "led-indicator-unrouted",
        "checks.json",
        &normalised_checks(&session),
        &mut failures,
    );
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

#[test]
fn parity_dual_amplifier_matches_goldens() {
    check_case(
        "dual-amplifier",
        &load_example("dual-amplifier.pcbcraft"),
        vec![],
    );
}

#[test]
fn parity_library_rc_matches_goldens() {
    check_case(
        "kicad-library-rc",
        &load_example("kicad-library-rc.pcbcraft"),
        vec![],
    );
}

#[test]
fn failed_batch_leaves_session_untouched() {
    let (mut session, _, _) = led_routed();
    let before = (session.project.clone(), session.revision);
    let bad = [
        Command {
            command: "project.rename".into(),
            params: json!({"name":"half applied"}),
        },
        Command {
            command: "track.add".into(),
            params: json!({"net":4,"start":{"x":0,"y":0},"end":{"x":1e12,"y":0},"width":0.2}),
        },
    ];
    assert!(session.batch(&bad, None).is_err());
    assert_eq!(before, (session.project.clone(), session.revision));
}

fn all_cases() -> Vec<(&'static str, Session)> {
    vec![
        ("led-indicator", led_routed().0),
        ("dual-amplifier", load_example("dual-amplifier.pcbcraft")),
        (
            "kicad-library-rc",
            load_example("kicad-library-rc.pcbcraft"),
        ),
    ]
}

#[test]
fn every_writer_is_deterministic() {
    for (case, session) in all_cases() {
        let first = outputs(&session);
        let second = outputs(&session);
        assert_eq!(first, second, "{case}: writer output changed between runs");
        // A rebuilt session from the same source must also match.
        assert_eq!(first, outputs(&session.clone()), "{case}");
    }
}

#[test]
fn every_reader_round_trips_its_own_writer() {
    let dir = tempfile::tempdir().expect("tempdir");
    for (case, session) in all_cases() {
        let path = dir.path().join(format!("{case}.pcbcraft"));
        pcbcraft_io::save(&session.project, &path).expect("save");
        let loaded: Project = pcbcraft_io::load(&path).expect("load");
        assert_eq!(loaded, session.project, "{case}: native round trip");
        // Saving the loaded project is byte-identical.
        let again = dir.path().join(format!("{case}-again.pcbcraft"));
        pcbcraft_io::save(&loaded, &again).expect("save again");
        assert_eq!(
            std::fs::read(&path).expect("read"),
            std::fs::read(&again).expect("read again"),
            "{case}: save is not idempotent"
        );
    }
}

/// Structural properties only. Gerber and drill output are pending #351 and
/// must not be frozen as goldens; the limited KiCad export is stable.
#[test]
fn kicad_export_is_structurally_sound() {
    for (case, session) in all_cases() {
        let text = pcbcraft_io::kicad_board(&session.project).expect("kicad");
        assert!(text.starts_with("(kicad_pcb"), "{case}");
        let (mut open, mut close, mut in_string, mut escaped) = (0i64, 0i64, false, false);
        for c in text.chars() {
            if in_string {
                match (escaped, c) {
                    (true, _) => escaped = false,
                    (false, '\\') => escaped = true,
                    (false, '"') => in_string = false,
                    _ => {}
                }
            } else {
                match c {
                    '"' => in_string = true,
                    '(' => open += 1,
                    ')' => close += 1,
                    _ => {}
                }
            }
        }
        assert_eq!(open, close, "{case}: unbalanced s-expression");
        let footprints = text.matches("(footprint ").count();
        assert_eq!(
            footprints,
            session.project.board.footprints.len(),
            "{case}: footprint count"
        );
    }
}
