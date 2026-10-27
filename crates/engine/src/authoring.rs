use super::*;
use pcbcraft_library::{FootprintDefinition, FootprintPad, SymbolDefinition};

pub(super) fn commands() -> Vec<CommandSpec> {
    [
        ("library.symbol.upsert","Create or replace a project-library symbol; existing placed definitions remain embedded",schema(json!({"definition":{"type":"object"},"notice":{"type":"string"}}),&["definition"])),
        ("library.footprint.upsert","Create or replace a project-library footprint",schema(json!({"definition":{"type":"object"},"notice":{"type":"string"}}),&["definition"])),
        ("library.symbol.remove","Remove a project-library symbol; placed instances retain their embedded definition",schema(json!({"name":{"type":"string"}}),&["name"])),
        ("library.footprint.remove","Remove a project-library footprint; placed instances retain their embedded definition",schema(json!({"name":{"type":"string"}}),&["name"])),
        ("library.load","Load and validate a .pcbcraft-lib collection into the project as one undoable transaction",schema(json!({"path":{"type":"string"}}),&["path"])),
        ("library.import_footprint_file","Read one footprint from a footprint library text file on disk and add or replace it in the project library as one undoable transaction",schema(json!({"path":{"type":"string","minLength":1,"maxLength":1024}}),&["path"])),
        ("library.import_symbol_file","Read the symbols from a symbol library text file on disk and add or replace them in the project library as one undoable transaction",schema(json!({"path":{"type":"string","minLength":1,"maxLength":1024}}),&["path"])),
        ("library.import_footprint_dir","Read every footprint library text file in a library folder on disk and add or replace them in the project library as one undoable transaction, so a whole vendor folder arrives in one step",schema(json!({"path":{"type":"string","minLength":1,"maxLength":1024}}),&["path"])),
        ("component.place_custom","Place a symbol from the editable project library",schema(json!({"name":{"type":"string"},"position":point_schema(),"reference":{"type":"string"},"value":{"type":"string"}}),&["name","position"])),
        ("footprint.assign_custom","Assign a project-library footprint whose pad numbers match the component pins",schema(json!({"id":id_schema(),"name":{"type":"string"}}),&["id","name"])),
        ("library.footprint.set_model","Attach or replace a 3D model reference on a project-library footprint: a file path with an optional offset, scale and rotation in millimetres and degrees; placed instances keep their embedded copy",schema(json!({"name":{"type":"string"},"path":{"type":"string","maxLength":1024},"offset":{"type":"array","items":{"type":"number"},"minItems":3,"maxItems":3},"scale":{"type":"array","items":{"type":"number"},"minItems":3,"maxItems":3},"rotate":{"type":"array","items":{"type":"number"},"minItems":3,"maxItems":3}}),&["name","path"])),
        ("library.footprint.clear_model","Remove the 3D model reference from a project-library footprint",schema(json!({"name":{"type":"string"}}),&["name"])),
        ("library.footprint.set_rules","Set design rule overrides on a project-library footprint: a local copper clearance and solder mask/paste margin shifts in millimetres; the clearance tightens clearance checks while the margins are recorded and exported",schema(json!({"name":{"type":"string"},"clearance":{"type":"number"},"mask_margin":{"type":"number"},"paste_margin":{"type":"number"}}),&["name"])),
        ("library.footprint.clear_rules","Remove the design rule overrides from a project-library footprint",schema(json!({"name":{"type":"string"}}),&["name"])),
        ("library.footprint.renumber_pads","Renumber pads on a project-library footprint with an ordered old-to-new map; embedded copies, their placed pads and matching component pins follow, and a target number already held by an untouched pad is refused",schema(json!({"name":{"type":"string"},"renames":{"type":"array","maxItems":512,"items":{"type":"object","properties":{"old":{"type":"string","maxLength":32},"new":{"type":"string","maxLength":32}},"required":["old","new"],"additionalProperties":false}}}),&["name","renames"])),
        ("library.footprint.generate_courtyard","Generate a courtyard rectangle on a project-library footprint: the outline of its pads and artwork grown by a margin, replacing any courtyard the footprint already carries",schema(json!({"name":{"type":"string"},"margin":{"type":"number","minimum":0,"maximum":5,"description":"ring beyond the outermost copper and artwork, millimetres"}}),&["name","margin"])),
        ("library.footprint.pad_set","Add or replace one pad on a project-library footprint by its number, so custom packages gain exposed pads, thermal vias or extra terminals without rewriting the whole definition; placed instances keep their embedded copy until footprint.refresh brings the change across",schema(json!({"name":{"type":"string"},"pad":{"type":"object","description":"a full pad: number, shape, size, position, layers and optional drill, as library readers emit them"}}),&["name","pad"])),
        ("library.footprint.pad_remove","Remove one pad from a project-library footprint by its number; the footprint keeps at least one pad, and placed instances keep their embedded copy until footprint.refresh brings the change across",schema(json!({"name":{"type":"string"},"pad":{"type":"string","description":"pad number, exactly as stored"}}),&["name","pad"])),
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
struct PadRename {
    old: String,
    new: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RenumberParams {
    name: String,
    renames: Vec<PadRename>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CourtyardParams {
    name: String,
    margin: f64,
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
struct PadSetParams {
    name: String,
    pad: FootprintPad,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PadRemoveParams {
    name: String,
    pad: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DirParams {
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
struct RulesParams {
    name: String,
    #[serde(default)]
    clearance: Option<f64>,
    #[serde(default)]
    mask_margin: Option<f64>,
    #[serde(default)]
    paste_margin: Option<f64>,
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
        "library.footprint.set_rules" => {
            let a: RulesParams = parse(params)?;
            if a.clearance.is_none() && a.mask_margin.is_none() && a.paste_margin.is_none() {
                return Err(Error::Invalid("set at least one rule override".into()));
            }
            let bounded = |v: &Option<f64>| v.is_none_or(|x| x.is_finite() && x.abs() <= 100.0);
            if !bounded(&a.clearance) || !bounded(&a.mask_margin) || !bounded(&a.paste_margin) {
                return Err(Error::Invalid(
                    "rule overrides must be finite and at most 100 mm".into(),
                ));
            }
            let def = p
                .library
                .footprints
                .iter_mut()
                .find(|s| s.name == a.name)
                .ok_or_else(missing)?;
            def.rules = Some(pcbcraft_library::FootprintRules {
                clearance: a.clearance,
                mask_margin: a.mask_margin,
                paste_margin: a.paste_margin,
            });
            def.validate().map_err(|e| Error::Invalid(e.to_string()))?;
            Ok(json!({"name": a.name, "rules": def.rules}))
        }
        "library.footprint.clear_rules" => {
            let a: Name = parse(params)?;
            let def = p
                .library
                .footprints
                .iter_mut()
                .find(|s| s.name == a.name)
                .ok_or_else(missing)?;
            if def.rules.is_none() {
                return Err(missing());
            }
            def.rules = None;
            def.validate().map_err(|e| Error::Invalid(e.to_string()))?;
            Ok(json!({"name": a.name}))
        }
        "library.footprint.generate_courtyard" => {
            let a: CourtyardParams = parse(params)?;
            if !a.margin.is_finite() || !(0.0..=5.0).contains(&a.margin) {
                return Err(Error::Invalid(
                    "the courtyard margin must be 0 to 5 millimetres".into(),
                ));
            }
            let def = p
                .library
                .footprints
                .iter_mut()
                .find(|s| s.name == a.name)
                .ok_or_else(missing)?;
            let mut min = Point::new(f64::MAX, f64::MAX);
            let mut max = Point::new(f64::MIN, f64::MIN);
            let grow = |p: &mut Point, q: &mut Point, at: Point, radius: f64| {
                if !at.valid() || !radius.is_finite() || radius < 0.0 {
                    return;
                }
                p.x = p.x.min(at.x - radius);
                p.y = p.y.min(at.y - radius);
                q.x = q.x.max(at.x + radius);
                q.y = q.y.max(at.y + radius);
            };
            for pad in &def.pads {
                let half_x = pad.size.x / 2.0;
                let half_y = pad.size.y / 2.0;
                let (c, s) = (
                    pad.rotation.to_radians().cos(),
                    pad.rotation.to_radians().sin(),
                );
                let reach = (half_x * c).abs() + (half_y * s).abs();
                let reach_y = (half_x * s).abs() + (half_y * c).abs();
                grow(&mut min, &mut max, pad.position, reach.max(reach_y));
            }
            for graphic in &def.graphics {
                if matches!(
                    graphic,
                    pcbcraft_library::Graphic::Rectangle { layer, .. }
                        if layer == "pcbcraft.courtyard"
                ) {
                    continue;
                }
                let (points, radius): (Vec<Point>, f64) = match graphic {
                    pcbcraft_library::Graphic::Line {
                        start, end, width, ..
                    } => (vec![*start, *end], *width / 2.0),
                    pcbcraft_library::Graphic::Rectangle {
                        start, end, width, ..
                    } => (vec![*start, *end], *width / 2.0),
                    pcbcraft_library::Graphic::Circle {
                        center,
                        radius,
                        width,
                        ..
                    } => (vec![*center], *radius + width / 2.0),
                    pcbcraft_library::Graphic::Arc {
                        start,
                        mid,
                        end,
                        width,
                        ..
                    } => (vec![*start, *mid, *end], *width / 2.0),
                    pcbcraft_library::Graphic::Polyline { points, width, .. } => {
                        (points.clone(), *width / 2.0)
                    }
                    _ => (vec![], 0.0),
                };
                for at in points {
                    grow(&mut min, &mut max, at, radius);
                }
            }
            if min.x > max.x || min.y > max.y {
                return Err(Error::Invalid(
                    "this footprint has no pads or artwork to surround".into(),
                ));
            }
            let before = def.graphics.len();
            def.graphics
                .retain(|g| !matches!(g, pcbcraft_library::Graphic::Rectangle { layer, .. } if layer == "pcbcraft.courtyard"));
            def.graphics.push(pcbcraft_library::Graphic::Rectangle {
                start: Point::new(min.x - a.margin, min.y - a.margin),
                end: Point::new(max.x + a.margin, max.y + a.margin),
                width: 0.1,
                fill: String::new(),
                layer: "pcbcraft.courtyard".into(),
            });
            if let Err(e) = def.validate() {
                def.graphics.truncate(before);
                return Err(Error::Invalid(e.to_string()));
            }
            Ok(json!({
                "name": a.name,
                "courtyard": [
                    min.x - a.margin,
                    min.y - a.margin,
                    max.x + a.margin,
                    max.y + a.margin
                ],
                "margin": a.margin
            }))
        }
        "library.footprint.renumber_pads" => {
            let a: RenumberParams = parse(params)?;
            if a.renames.is_empty() || a.renames.len() > 512 {
                return Err(Error::Invalid(
                    "pad renumbering needs 1 to 512 old-new pairs".into(),
                ));
            }
            for r in &a.renames {
                if r.old.is_empty() || r.new.is_empty() || r.old.len() > 32 || r.new.len() > 32 {
                    return Err(Error::Invalid("pad numbers must be 1 to 32 bytes".into()));
                }
            }
            let mut sources: Vec<&str> = a.renames.iter().map(|r| r.old.as_str()).collect();
            sources.sort_unstable();
            sources.dedup();
            if sources.len() != a.renames.len() {
                return Err(Error::Invalid(
                    "each old pad number may appear once in the map".into(),
                ));
            }
            let targets: Vec<&str> = a.renames.iter().map(|r| r.new.as_str()).collect();
            let mut dedup: Vec<&str> = targets.clone();
            dedup.sort_unstable();
            dedup.dedup();
            if dedup.len() != targets.len() {
                return Err(Error::Invalid(
                    "two pads cannot be renumbered to the same number".into(),
                ));
            }
            let def = p
                .library
                .footprints
                .iter_mut()
                .find(|s| s.name == a.name)
                .ok_or_else(missing)?;
            for r in &a.renames {
                if !def.pads.iter().any(|pad| pad.number == r.old) {
                    return Err(Error::Invalid(format!(
                        "this footprint has no pad {}",
                        r.old
                    )));
                }
            }
            for target in &targets {
                if !a.renames.iter().any(|r| r.old == **target)
                    && def.pads.iter().any(|pad| pad.number == **target)
                {
                    return Err(Error::Invalid(format!(
                        "pad number {target} is already used by a pad this map leaves alone"
                    )));
                }
            }
            let touched: Vec<(usize, String)> = def
                .pads
                .iter()
                .enumerate()
                .filter(|(_, pad)| a.renames.iter().any(|r| r.old == pad.number))
                .map(|(i, pad)| (i, pad.number.clone()))
                .collect();
            for &(i, _) in &touched {
                def.pads[i].number = String::new();
            }
            for (i, old) in &touched {
                if let Some(r) = a.renames.iter().find(|r| r.old == *old) {
                    def.pads[*i].number = r.new.clone();
                }
            }
            if def.pads.iter().any(|pad| pad.number.is_empty()) {
                return Err(Error::Invalid(
                    "every renamed pad needs a new number".into(),
                ));
            }
            def.validate().map_err(|e| Error::Invalid(e.to_string()))?;
            let affected: Vec<Id> = p
                .components
                .iter()
                .filter(|c| {
                    c.library_footprint
                        .as_ref()
                        .is_some_and(|f| f.definition.name == a.name)
                })
                .map(|c| c.id)
                .collect();
            for c in &mut p.components {
                if !affected.contains(&c.id) {
                    continue;
                }
                if let Some(embedded) = &mut c.library_footprint {
                    for pad in &mut embedded.definition.pads {
                        if let Some(r) = a.renames.iter().find(|r| r.old == pad.number) {
                            pad.number = r.new.clone();
                        }
                    }
                }
                for pin in &mut c.pins {
                    if let Some(r) = a.renames.iter().find(|r| r.old == pin.number) {
                        pin.number = r.new.clone();
                    }
                }
            }
            for f in &mut p.board.footprints {
                if !affected.contains(&f.component) {
                    continue;
                }
                for pad in &mut f.pads {
                    if let Some(r) = a.renames.iter().find(|r| r.old == pad.number) {
                        pad.number = r.new.clone();
                    }
                    let geometry_new = pad.geometry.as_ref().and_then(|geometry| {
                        a.renames
                            .iter()
                            .find(|r| r.old == geometry.number)
                            .map(|r| r.new.clone())
                    });
                    if let (Some(geometry), Some(new)) =
                        (pad.geometry.as_mut(), geometry_new.as_deref())
                    {
                        geometry.number = new.to_string();
                    }
                }
            }
            Ok(json!({
                "name": a.name,
                "renamed": touched.len(),
                "components": affected.len()
            }))
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
        "library.import_footprint_file" | "library.import_symbol_file" => {
            let a: PathParams = parse(params)?;
            let text =
                pcbcraft_io::read_text(std::path::Path::new(&a.path)).map_err(Error::Invalid)?;
            let mut imported: Vec<String> = Vec::new();
            if command == "library.import_footprint_file" {
                let def = pcbcraft_library::read_footprint(&text)
                    .map_err(|e| Error::Invalid(e.to_string()))?;
                def.validate().map_err(|e| Error::Invalid(e.to_string()))?;
                imported.push(def.name.clone());
                import_footprint(p, def);
            } else {
                let defs = pcbcraft_library::read_symbols(&text)
                    .map_err(|e| Error::Invalid(e.to_string()))?;
                if defs.is_empty() {
                    return Err(Error::Invalid("the library file defines no symbols".into()));
                }
                for def in defs {
                    def.validate().map_err(|e| Error::Invalid(e.to_string()))?;
                    imported.push(def.name.clone());
                    import_symbol(p, def);
                }
            }
            p.library.validate()?;
            Ok(json!({"names": imported}))
        }
        "library.import_footprint_dir" => {
            let a: DirParams = parse(params)?;
            let dir = std::path::Path::new(&a.path);
            let entries = std::fs::read_dir(dir).map_err(|e| Error::Invalid(e.to_string()))?;
            let mut files: Vec<std::path::PathBuf> = Vec::new();
            for entry in entries {
                let entry = entry.map_err(|e| Error::Invalid(e.to_string()))?;
                let path = entry.path();
                if path.is_file()
                    && path
                        .extension()
                        .and_then(|e| e.to_str())
                        .is_some_and(|e| e.eq_ignore_ascii_case("kicad_mod"))
                {
                    files.push(path);
                }
            }
            files.sort();
            if files.is_empty() {
                return Err(Error::Invalid(
                    "the folder holds no footprint library text files".into(),
                ));
            }
            if files.len() > 256 {
                return Err(Error::Invalid(
                    "a library folder may hold at most 256 footprint files".into(),
                ));
            }
            let mut imported: Vec<String> = Vec::new();
            for path in &files {
                let text = pcbcraft_io::read_text(path).map_err(Error::Invalid)?;
                let def = pcbcraft_library::read_footprint(&text)
                    .map_err(|e| Error::Invalid(e.to_string()))?;
                def.validate().map_err(|e| Error::Invalid(e.to_string()))?;
                imported.push(def.name.clone());
                import_footprint(p, def);
            }
            p.library.validate()?;
            Ok(json!({"imported": imported.len(), "names": imported}))
        }
        "library.footprint.pad_set" => {
            let a: PadSetParams = parse(params)?;
            if a.name.is_empty() || a.name.len() > 128 {
                return Err(Error::Invalid(
                    "footprint name must be 1 to 128 bytes".into(),
                ));
            }
            if a.pad.number.is_empty() || a.pad.number.len() > 32 {
                return Err(Error::Invalid("pad number must be 1 to 32 bytes".into()));
            }
            let number = a.pad.number.clone();
            let pads = {
                let footprint = p
                    .library
                    .footprints
                    .iter_mut()
                    .find(|f| f.name == a.name)
                    .ok_or_else(missing)?;
                let replaced = footprint.pads.iter().any(|q| q.number == number);
                if let Some(existing) = footprint.pads.iter_mut().find(|q| q.number == number) {
                    *existing = a.pad;
                } else {
                    footprint.pads.push(a.pad);
                }
                footprint
                    .validate()
                    .map_err(|e| Error::Invalid(e.to_string()))?;
                (footprint.pads.len(), !replaced)
            };
            p.library.validate()?;
            Ok(json!({"name": a.name, "pad": number, "added": pads.1, "pads": pads.0}))
        }
        "library.footprint.pad_remove" => {
            let a: PadRemoveParams = parse(params)?;
            if a.name.is_empty() || a.name.len() > 128 {
                return Err(Error::Invalid(
                    "footprint name must be 1 to 128 bytes".into(),
                ));
            }
            if a.pad.is_empty() || a.pad.len() > 32 {
                return Err(Error::Invalid("pad number must be 1 to 32 bytes".into()));
            }
            let name = a.name.clone();
            let pads = {
                let footprint = p
                    .library
                    .footprints
                    .iter_mut()
                    .find(|f| f.name == name)
                    .ok_or_else(missing)?;
                let before = footprint.pads.len();
                footprint.pads.retain(|q| q.number != a.pad);
                if footprint.pads.len() == before {
                    return Err(Error::Invalid(format!(
                        "footprint {name} has no pad {}",
                        a.pad
                    )));
                }
                if footprint.pads.is_empty() {
                    return Err(Error::Invalid(
                        "a footprint keeps at least one pad; remove the footprint itself instead"
                            .into(),
                    ));
                }
                footprint
                    .validate()
                    .map_err(|e| Error::Invalid(e.to_string()))?;
                footprint.pads.len()
            };
            p.library.validate()?;
            Ok(json!({"name": name, "pad": a.pad, "pads": pads}))
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

fn import_footprint(p: &mut Project, def: pcbcraft_library::FootprintDefinition) {
    if let Some(existing) = p.library.footprints.iter_mut().find(|f| f.name == def.name) {
        *existing = def;
    } else {
        p.library.footprints.push(def);
    }
    p.library.footprints.sort_by(|a, b| a.name.cmp(&b.name));
}

fn import_symbol(p: &mut Project, def: pcbcraft_library::SymbolDefinition) {
    if let Some(existing) = p.library.symbols.iter_mut().find(|s| s.name == def.name) {
        *existing = def;
    } else {
        p.library.symbols.push(def);
    }
    p.library.symbols.sort_by(|a, b| a.name.cmp(&b.name));
}
