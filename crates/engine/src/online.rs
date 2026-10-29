//! Commands that bring an online library part into the project: import it
//! into the project library with its licence notice. The engine never touches the network; the caller fetches
//! the files (crates/library/src/online_import.rs) and passes parsed definitions.
//!
//! # Rules
//! - The command runs inside one `Session::batch` step, so one undo reverts everything.
//! - A name already held by different content is refused unless `on_conflict` says otherwise.
//!
//! # Dependencies
//! - crates/engine/src/authoring.rs
//! - crates/engine/src/lib.rs
//! - crates/library/src/online_import.rs
use super::authoring::{self, ModelParams};
use super::*;
use pcbcraft_library::{FootprintDefinition, SymbolDefinition};

const MAX_NOTICES: usize = 4;
const MAX_NOTICE_BYTES: usize = 1024;

pub(super) fn commands() -> Vec<CommandSpec> {
    let import_props = json!({
        "symbol":{"type":"object","description":"a symbol definition as the library readers emit it"},
        "footprint":{"type":"object","description":"an optional footprint definition as the library readers emit it"},
        "model":{"type":"object","description":"an optional 3D model reference for the footprint: path with optional offset, scale and rotate",
            "properties":{"path":{"type":"string","maxLength":1024},
                "offset":{"type":"array","items":{"type":"number"},"minItems":3,"maxItems":3},
                "scale":{"type":"array","items":{"type":"number"},"minItems":3,"maxItems":3},
                "rotate":{"type":"array","items":{"type":"number"},"minItems":3,"maxItems":3}},
            "required":["path"],"additionalProperties":false},
        "notices":{"type":"array","maxItems":MAX_NOTICES,"items":{"type":"string","maxLength":MAX_NOTICE_BYTES},"description":"source and licence notices to record with the part"},
        "on_conflict":{"type":"string","enum":["refuse","replace","keep_both"],"default":"refuse","description":"what to do when a different part already holds the same name"}
    });
    vec![CommandSpec {
        id: "library.import_online",
        description: "Add a part fetched from an online library to the project library as one undoable step: its symbol, an optional footprint and 3D model reference, and the source and licence notices; a part already there with identical content changes nothing, and a different part with the same name is refused unless replace or keep_both is chosen",
        input_schema: schema(import_props, &["symbol"]),
    }]
}

#[derive(Deserialize, Default, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Conflict {
    #[default]
    Refuse,
    Replace,
    KeepBoth,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ModelRef {
    path: String,
    #[serde(default)]
    offset: Option<[f64; 3]>,
    #[serde(default)]
    scale: Option<[f64; 3]>,
    #[serde(default)]
    rotate: Option<[f64; 3]>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ImportParams {
    symbol: SymbolDefinition,
    #[serde(default)]
    footprint: Option<FootprintDefinition>,
    #[serde(default)]
    model: Option<ModelRef>,
    #[serde(default)]
    notices: Vec<String>,
    #[serde(default)]
    on_conflict: Conflict,
}

/// What an import did, for the command's answer.
struct Imported {
    symbol: SymbolDefinition,
    footprint: Option<FootprintDefinition>,
    unchanged: bool,
}

pub(super) fn run(
    p: &mut Project,
    _next: &mut Id,
    command: &str,
    params: &Value,
) -> Option<Result<Value, Error>> {
    match command {
        "library.import_online" => Some(import_command(p, params)),
        _ => None,
    }
}

fn import_command(p: &mut Project, params: &Value) -> Result<Value, Error> {
    let a: ImportParams = parse(params)?;
    let done = import(p, a.symbol, a.footprint, a.model, &a.notices, a.on_conflict)?;
    Ok(json!({
        "symbol": done.symbol.name,
        "footprint": done.footprint.as_ref().map(|f| f.name.clone()),
        "unchanged": done.unchanged,
    }))
}

/// Add the definitions and notices to the project library.
fn import(
    p: &mut Project,
    mut symbol: SymbolDefinition,
    mut footprint: Option<FootprintDefinition>,
    model: Option<ModelRef>,
    notices: &[String],
    on_conflict: Conflict,
) -> Result<Imported, Error> {
    if notices.len() > MAX_NOTICES
        || notices
            .iter()
            .any(|n| n.is_empty() || n.len() > MAX_NOTICE_BYTES || n.chars().any(char::is_control))
    {
        return Err(Error::Invalid(format!(
            "at most {MAX_NOTICES} notices of 1 to {MAX_NOTICE_BYTES} bytes without control characters"
        )));
    }
    symbol
        .validate()
        .map_err(|e| Error::Invalid(e.to_string()))?;
    if model.is_some() && footprint.is_none() {
        return Err(Error::Invalid(
            "a 3D model needs a footprint to attach to".into(),
        ));
    }
    if let (Some(fp), Some(m)) = (footprint.as_mut(), model) {
        let built = authoring::build_model(&ModelParams {
            name: fp.name.clone(),
            path: m.path,
            offset: m.offset,
            scale: m.scale,
            rotate: m.rotate,
        })?;
        fp.model = Some(built);
    }
    if let Some(fp) = &footprint {
        fp.validate().map_err(|e| Error::Invalid(e.to_string()))?;
    }

    let mut write_symbol = false;
    match p.library.symbols.iter().find(|s| s.name == symbol.name) {
        None => write_symbol = true,
        Some(e) if *e == symbol => {}
        Some(_) => match on_conflict {
            Conflict::Refuse => return Err(refusal("symbol", &symbol.name)),
            Conflict::Replace => write_symbol = true,
            Conflict::KeepBoth => {
                symbol.name = free_name(&symbol.name, |n| {
                    p.library.symbols.iter().any(|s| s.name == n)
                })?;
                write_symbol = true;
            }
        },
    }
    let mut write_footprint = false;
    if let Some(fp) = footprint.as_mut() {
        match p.library.footprints.iter().find(|f| f.name == fp.name) {
            None => write_footprint = true,
            Some(e) if *e == *fp => {}
            Some(_) => match on_conflict {
                Conflict::Refuse => return Err(refusal("footprint", &fp.name)),
                Conflict::Replace => write_footprint = true,
                Conflict::KeepBoth => {
                    fp.name = free_name(&fp.name, |n| {
                        p.library.footprints.iter().any(|f| f.name == n)
                    })?;
                    write_footprint = true;
                }
            },
        }
    }
    if write_symbol {
        upsert_symbol(p, symbol.clone());
    }
    if write_footprint && let Some(fp) = &footprint {
        upsert_footprint(p, fp.clone());
    }
    let mut notice_added = false;
    for n in notices {
        if !p.library.notices.contains(n) {
            p.library.notices.push(n.clone());
            notice_added = true;
        }
    }
    let unchanged = !(write_symbol || write_footprint || notice_added);
    p.library.validate()?;
    Ok(Imported {
        symbol,
        footprint,
        unchanged,
    })
}

fn refusal(what: &str, name: &str) -> Error {
    Error::Invalid(format!(
        "a different {what} named {name} is already in the project library; \
         choose replace to overwrite it or keep_both to add this one under a new name"
    ))
}

/// First `name_2`, `name_3`, ... that is not taken.
fn free_name(name: &str, taken: impl Fn(&str) -> bool) -> Result<String, Error> {
    for n in 2..=99 {
        let candidate = format!("{name}_{n}");
        if !taken(&candidate) {
            return Ok(candidate);
        }
    }
    Err(Error::Invalid(format!(
        "no free name found for {name} in the project library"
    )))
}

fn upsert_symbol(p: &mut Project, def: SymbolDefinition) {
    if let Some(existing) = p.library.symbols.iter_mut().find(|s| s.name == def.name) {
        *existing = def;
    } else {
        p.library.symbols.push(def);
    }
    p.library.symbols.sort_by(|a, b| a.name.cmp(&b.name));
}

fn upsert_footprint(p: &mut Project, def: FootprintDefinition) {
    if let Some(existing) = p.library.footprints.iter_mut().find(|f| f.name == def.name) {
        *existing = def;
    } else {
        p.library.footprints.push(def);
    }
    p.library.footprints.sort_by(|a, b| a.name.cmp(&b.name));
}

#[cfg(test)]
mod tests;
