//! Tests for `library.import_online`.
use super::*;
use pcbcraft_library::{read_footprint, read_symbols};

const DEVICE: &str = include_str!("../../../library/tests/fixtures/device.kicad_sym");
const R0805: &str = include_str!("../../../library/tests/fixtures/R_0805_2012Metric.kicad_mod");
const NOTICE: &str = "Imported symbol from Open library: Device.kicad_symdir/R.kicad_sym. Licence CC-BY-SA 4.0 (https://creativecommons.org/licenses/by-sa/4.0/).";

fn symbol(text: &str, name: &str) -> Result<SymbolDefinition, Error> {
    read_symbols(text)
        .map_err(|e| Error::Invalid(e.to_string()))?
        .into_iter()
        .find(|s| s.name == name)
        .ok_or_else(missing)
}

fn footprint() -> Result<FootprintDefinition, Error> {
    read_footprint(R0805).map_err(|e| Error::Invalid(e.to_string()))
}

fn import_params() -> Result<Value, Error> {
    Ok(json!({
        "symbol": symbol(DEVICE, "R")?,
        "footprint": footprint()?,
        "notices": [NOTICE],
    }))
}

#[test]
fn the_command_is_listed_with_its_schema() {
    let all = commands();
    assert!(all.iter().any(|c| c.id == "library.import_online"));
}

#[test]
fn import_online_adds_symbol_footprint_and_notice() -> Result<(), Error> {
    let mut s = Session::default();
    let mut params = import_params()?;
    params["model"] = json!({"path": "/cache/R_0805.step"});
    let out = s.execute("library.import_online", params, None)?;
    assert_eq!(out["result"]["symbol"], "R");
    assert_eq!(out["result"]["footprint"], "R_0805_2012Metric");
    assert_eq!(out["result"]["unchanged"], false);
    assert!(s.project.library.symbols.iter().any(|x| x.name == "R"));
    let fp = s
        .project
        .library
        .footprints
        .iter()
        .find(|f| f.name == "R_0805_2012Metric")
        .ok_or_else(missing)?;
    assert_eq!(
        fp.model.as_ref().map(|m| m.path.as_str()),
        Some("/cache/R_0805.step"),
        "the 3D model reference is attached"
    );
    assert_eq!(s.project.library.notices, vec![NOTICE.to_owned()]);
    Ok(())
}

#[test]
fn import_online_is_one_undo_step() -> Result<(), Error> {
    let mut s = Session::default();
    let before = s.project.library.clone();
    s.execute("library.import_online", import_params()?, None)?;
    assert_ne!(s.project.library, before);
    s.undo()?;
    assert_eq!(
        s.project.library, before,
        "one undo removes the symbol, footprint and notice"
    );
    Ok(())
}

#[test]
fn import_online_twice_is_a_no_op() -> Result<(), Error> {
    let mut s = Session::default();
    s.execute("library.import_online", import_params()?, None)?;
    let once = s.project.library.clone();
    let again = s.execute("library.import_online", import_params()?, None)?;
    assert_eq!(again["result"]["unchanged"], true);
    assert_eq!(s.project.library, once);
    assert_eq!(
        s.project.library.notices.len(),
        1,
        "the notice is kept once"
    );
    Ok(())
}

#[test]
fn import_online_refuses_a_different_part_with_the_same_name() -> Result<(), Error> {
    let mut s = Session::default();
    s.execute("library.import_online", import_params()?, None)?;
    let before = s.project.library.clone();
    let mut other = symbol(DEVICE, "R")?;
    other.description = "A different resistor".into();
    let mut params = import_params()?;
    params["symbol"] = serde_json::to_value(&other).map_err(|e| Error::Invalid(e.to_string()))?;
    let err = s
        .execute("library.import_online", params, None)
        .err()
        .ok_or_else(missing)?;
    let text = err.to_string();
    assert!(
        text.contains("replace") && text.contains("keep_both"),
        "{text}"
    );
    assert_eq!(s.project.library, before);
    Ok(())
}

#[test]
fn import_online_replace_and_keep_both() -> Result<(), Error> {
    let mut s = Session::default();
    s.execute("library.import_online", import_params()?, None)?;
    let mut other = symbol(DEVICE, "R")?;
    other.description = "A different resistor".into();
    let changed = serde_json::to_value(&other).map_err(|e| Error::Invalid(e.to_string()))?;

    let mut keep = import_params()?;
    keep["symbol"] = changed.clone();
    keep["on_conflict"] = json!("keep_both");
    let out = s.execute("library.import_online", keep, None)?;
    assert_eq!(out["result"]["symbol"], "R_2");
    assert_eq!(
        s.project
            .library
            .symbols
            .iter()
            .filter(|x| x.name.starts_with('R'))
            .count(),
        2
    );

    let mut replace = import_params()?;
    replace["symbol"] = changed;
    replace["on_conflict"] = json!("replace");
    s.execute("library.import_online", replace, None)?;
    let r = s
        .project
        .library
        .symbols
        .iter()
        .find(|x| x.name == "R")
        .ok_or_else(missing)?;
    assert_eq!(r.description, "A different resistor");
    Ok(())
}

#[test]
fn import_online_with_a_bad_definition_changes_nothing() -> Result<(), Error> {
    let mut s = Session::default();
    let revision = s.revision;
    let mut bad = import_params()?;
    bad["symbol"]["name"] = json!("");
    assert!(s.execute("library.import_online", bad, None).is_err());

    let mut too_many = import_params()?;
    too_many["notices"] = json!(["a", "b", "c", "d", "e"]);
    assert!(s.execute("library.import_online", too_many, None).is_err());

    let mut stray = import_params()?;
    stray["unexpected"] = json!(1);
    assert!(s.execute("library.import_online", stray, None).is_err());

    assert_eq!(s.revision, revision);
    assert!(s.project.library.symbols.is_empty());
    assert!(s.project.library.notices.is_empty());
    Ok(())
}
