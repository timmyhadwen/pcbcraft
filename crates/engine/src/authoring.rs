use super::*;
use pcbcraft_library::{FootprintDefinition, SymbolDefinition};

pub(super) fn commands() -> Vec<CommandSpec> {
    [
        ("library.symbol.upsert","Create or replace a project-library symbol; existing placed definitions remain embedded",schema(json!({"definition":{"type":"object"},"notice":{"type":"string"}}),&["definition"])),
        ("library.footprint.upsert","Create or replace a project-library footprint",schema(json!({"definition":{"type":"object"},"notice":{"type":"string"}}),&["definition"])),
        ("library.symbol.remove","Remove a project-library symbol; placed instances retain their embedded definition",schema(json!({"name":{"type":"string"}}),&["name"])),
        ("library.footprint.remove","Remove a project-library footprint; placed instances retain their embedded definition",schema(json!({"name":{"type":"string"}}),&["name"])),
        ("library.load","Load and validate a .pcbcraft-lib collection into the project as one undoable transaction",schema(json!({"path":{"type":"string"}}),&["path"])),
        ("component.place_custom","Place a symbol from the editable project library",schema(json!({"name":{"type":"string"},"position":point_schema(),"reference":{"type":"string"},"value":{"type":"string"}}),&["name","position"])),
        ("footprint.assign_custom","Assign a project-library footprint whose pad numbers match the component pins",schema(json!({"id":id_schema(),"name":{"type":"string"}}),&["id","name"])),
        ("library.footprint.set_model","Attach or replace a 3D model reference on a project-library footprint: a file path with an optional offset, scale and rotation in millimetres and degrees; placed instances keep their embedded copy",schema(json!({"name":{"type":"string"},"path":{"type":"string","maxLength":1024},"offset":{"type":"array","items":{"type":"number"},"minItems":3,"maxItems":3},"scale":{"type":"array","items":{"type":"number"},"minItems":3,"maxItems":3},"rotate":{"type":"array","items":{"type":"number"},"minItems":3,"maxItems":3}}),&["name","path"])),
        ("library.footprint.clear_model","Remove the 3D model reference from a project-library footprint",schema(json!({"name":{"type":"string"}}),&["name"])),
    ].into_iter().map(|(id,description,input_schema)|CommandSpec {id,description,input_schema}).collect()
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SymbolParams {
    definition: SymbolDefinition,
    notice: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FootprintParams {
    definition: FootprintDefinition,
    notice: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PathParams {
    path: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ModelParams {
    name: String,
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
struct Place {
    name: String,
    position: Point,
    reference: Option<String>,
    value: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Assign {
    id: Id,
    name: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LibraryFile {
    format: String,
    version: u32,
    library: ProjectLibrary,
}
pub(super) fn run(
    p: &mut Project,
    next: &mut Id,
    command: &str,
    params: &Value,
) -> Option<Result<Value, Error>> {
    if !commands().iter().any(|c| c.id == command) {
        return None;
    }
    Some((|| match command {
        "library.symbol.upsert" => {
            let a: SymbolParams = parse(params)?;
            a.definition
                .validate()
                .map_err(|e| Error::Invalid(e.to_string()))?;
            let name = a.definition.name.clone();
            if let Some(notice) = a.notice
                && !p.library.notices.contains(&notice)
            {
                p.library.notices.push(notice);
            }
            if let Some(def) = p.library.symbols.iter_mut().find(|s| s.name == name) {
                *def = a.definition;
            } else {
                p.library.symbols.push(a.definition);
            }
            p.library.symbols.sort_by(|a, b| a.name.cmp(&b.name));
            Ok(json!({"name":name}))
        }
        "library.footprint.upsert" => {
            let a: FootprintParams = parse(params)?;
            a.definition
                .validate()
                .map_err(|e| Error::Invalid(e.to_string()))?;
            let name = a.definition.name.clone();
            if let Some(notice) = a.notice
                && !p.library.notices.contains(&notice)
            {
                p.library.notices.push(notice);
            }
            if let Some(def) = p.library.footprints.iter_mut().find(|s| s.name == name) {
                *def = a.definition;
            } else {
                p.library.footprints.push(a.definition);
            }
            p.library.footprints.sort_by(|a, b| a.name.cmp(&b.name));
            Ok(json!({"name":name}))
        }
        "library.symbol.remove" | "library.footprint.remove" => {
            let a: Name = parse(params)?;
            if command == "library.symbol.remove" {
                let before = p.library.symbols.len();
                p.library.symbols.retain(|s| s.name != a.name);
                if before == p.library.symbols.len() {
                    return Err(missing());
                }
            } else {
                let before = p.library.footprints.len();
                p.library.footprints.retain(|s| s.name != a.name);
                if before == p.library.footprints.len() {
                    return Err(missing());
                }
            }
            Ok(json!({"name":a.name}))
        }
        "library.footprint.set_model" => {
            let a: ModelParams = parse(params)?;
            if a.path.is_empty() || a.path.len() > 1024 {
                return Err(Error::Invalid("model path must be 1 to 1024 bytes".into()));
            }
            let sane = |v: &Option<[f64; 3]>, positive: bool| {
                v.is_none_or(|t| {
                    t.iter()
                        .all(|x| x.is_finite() && x.abs() <= 100_000.0 && (!positive || *x > 0.0))
                })
            };
            if !sane(&a.offset, false) || !sane(&a.rotate, false) || !sane(&a.scale, true) {
                return Err(Error::Invalid(
                    "model transforms must be finite, scales positive".into(),
                ));
            }
            let def = p
                .library
                .footprints
                .iter_mut()
                .find(|s| s.name == a.name)
                .ok_or_else(missing)?;
            let offset = a.offset.unwrap_or([0.0; 3]);
            let scale = a.scale.unwrap_or([1.0; 3]);
            let rotate = a.rotate.unwrap_or([0.0; 3]);
            let raw_source = format!(
                "(model \"{}\" (offset (xyz {} {} {})) (scale (xyz {} {} {})) (rotate (xyz {} {} {})))",
                a.path,
                offset[0],
                offset[1],
                offset[2],
                scale[0],
                scale[1],
                scale[2],
                rotate[0],
                rotate[1],
                rotate[2]
            );
            def.model = Some(pcbcraft_library::Model3D {
                path: a.path,
                offset,
                scale,
                rotate,
                raw_source,
            });
            def.validate().map_err(|e| Error::Invalid(e.to_string()))?;
            Ok(json!({"name": a.name, "path": def.model.as_ref().map(|m| m.path.clone())}))
        }
        "library.footprint.clear_model" => {
            let a: Name = parse(params)?;
            let def = p
                .library
                .footprints
                .iter_mut()
                .find(|s| s.name == a.name)
                .ok_or_else(missing)?;
            if def.model.is_none() {
                return Err(missing());
            }
            def.model = None;
            def.validate().map_err(|e| Error::Invalid(e.to_string()))?;
            Ok(json!({"name": a.name}))
        }
        "library.load" => {
            let a: PathParams = parse(params)?;
            let text = pcbcraft_io::read_text(std::path::Path::new(&a.path))?;
            let file: LibraryFile =
                serde_json::from_str(&text).map_err(|e| Error::Invalid(e.to_string()))?;
            if file.format != "pcbcraft-library" || file.version != 1 {
                return Err(Error::Invalid(
                    "unsupported library file format/version".into(),
                ));
            }
            file.library.validate()?;
            p.library = file.library;
            Ok(json!({"symbols":p.library.symbols.len(),"footprints":p.library.footprints.len()}))
        }
        "component.place_custom" => {
            let a: Place = parse(params)?;
            let def = p
                .library
                .symbols
                .iter()
                .find(|s| s.name == a.name)
                .cloned()
                .ok_or_else(missing)?;
            place_definition(
                p,
                next,
                "project".into(),
                def,
                a.position,
                a.reference,
                a.value,
            )
        }
        "footprint.assign_custom" => {
            let a: Assign = parse(params)?;
            let def = p
                .library
                .footprints
                .iter()
                .find(|s| s.name == a.name)
                .cloned()
                .ok_or_else(missing)?;
            assign_definition(p, a.id, "project".into(), def)
        }
        _ => Err(Error::Invalid("unknown library authoring command".into())),
    })())
}
