//! The single mutation boundary used by UI, CLI and MCP.
mod authoring;
pub mod library;
use pcbcraft_doc::*;
use pcbcraft_geom::{Point, positive};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::VecDeque;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Invalid(String),
    #[error("stale revision: expected {expected}, current {current}")]
    Stale { expected: u64, current: u64 },
}
impl From<String> for Error {
    fn from(s: String) -> Self {
        Self::Invalid(s)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Command {
    pub command: String,
    #[serde(default)]
    pub params: Value,
}
#[derive(Clone, Debug, Serialize)]
pub struct CommandSpec {
    pub id: &'static str,
    pub description: &'static str,
    pub input_schema: Value,
}

fn schema(properties: Value, required: &[&str]) -> Value {
    json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
}
fn id_schema() -> Value {
    json!({"type":"integer","minimum":1})
}
fn point_schema() -> Value {
    schema(
        json!({"x":{"type":"number","description":"millimetres"},"y":{"type":"number","description":"millimetres"}}),
        &["x", "y"],
    )
}
fn pin_schema() -> Value {
    schema(
        json!({"component":id_schema(),"pin":{"type":"string"}}),
        &["component", "pin"],
    )
}
pub fn commands() -> Vec<CommandSpec> {
    let entries = [
        (
            "project.rename",
            "Rename the current project",
            schema(json!({"name":{"type":"string"}}), &["name"]),
        ),
        (
            "component.add",
            "Place an original built-in symbol: resistor, capacitor, led or connector",
            schema(
                json!({"kind":{"type":"string","enum":["resistor","capacitor","led","connector"]},"position":point_schema(),"reference":{"type":"string"},"value":{"type":"string"}}),
                &["kind", "position"],
            ),
        ),
        (
            "component.place_library",
            "Place a KiCad symbol package with independently movable units and embedded definitions",
            schema(
                json!({"path":{"type":"string"},"symbol":{"type":"string"},"position":point_schema(),"reference":{"type":"string"},"value":{"type":"string"}}),
                &["path", "symbol", "position"],
            ),
        ),
        (
            "footprint.assign",
            "Assign a KiCad front SMD or plated through-hole footprint with matching physical pins",
            schema(
                json!({"id":id_schema(),"path":{"type":"string"}}),
                &["id", "path"],
            ),
        ),
        (
            "component.move",
            "Move a schematic component; connected wire endpoints follow",
            schema(
                json!({"id":id_schema(),"position":point_schema(),"unit":{"type":"integer","minimum":1,"maximum":32}}),
                &["id", "position"],
            ),
        ),
        (
            "component.rotate",
            "Set schematic component rotation in degrees",
            schema(
                json!({"id":id_schema(),"degrees":{"type":"number"},"unit":{"type":"integer","minimum":1,"maximum":32}}),
                &["id", "degrees"],
            ),
        ),
        (
            "component.value",
            "Change the component value",
            schema(
                json!({"id":id_schema(),"value":{"type":"string"}}),
                &["id", "value"],
            ),
        ),
        (
            "net.create",
            "Create a named logical net",
            schema(json!({"name":{"type":"string"}}), &["name"]),
        ),
        (
            "net.connect",
            "Assign unassigned or same-net component pins to a net",
            schema(
                json!({"net":id_schema(),"pins":{"type":"array","items":pin_schema(),"minItems":1,"maxItems":128}}),
                &["net", "pins"],
            ),
        ),
        (
            "wire.connect",
            "Connect two pins; infer their existing net or atomically create a net if neither is assigned",
            schema(
                json!({"net":id_schema(),"from":pin_schema(),"to":pin_schema()}),
                &["from", "to"],
            ),
        ),
        (
            "wire.draw",
            "Draw a free-form schematic wire with explicit vertices, optionally bound to a net",
            schema(
                json!({"points":{"type":"array","items":point_schema(),"minItems":2,"maxItems":256},"net":id_schema()}),
                &["points"],
            ),
        ),
        (
            "wire.split",
            "Split a drawn wire into two at a point on one of its segments",
            schema(json!({"id":id_schema(),"at":point_schema()}), &["id", "at"]),
        ),
        (
            "graphic.add",
            "Add a free-standing schematic or board graphic: line, rect, circle, arc, polygon or text",
            schema(
                json!({"layer":{"type":"string","enum":["schematic","board"]},"shape":{"type":"string","enum":["line","rect","circle","arc","polygon","text"]},"points":{"type":"array","items":point_schema(),"minItems":1,"maxItems":256},"width":{"type":"number","exclusiveMinimum":0,"maximum":1000},"text":{"type":"string","maxLength":1024}}),
                &["layer", "shape", "points", "width"],
            ),
        ),
        (
            "graphic.move",
            "Translate an existing schematic or board graphic by a delta",
            schema(
                json!({"id":id_schema(),"delta":point_schema()}),
                &["id", "delta"],
            ),
        ),
        (
            "label.place",
            "Place a local, global, hierarchical or power label, or a junction, naming an existing net",
            schema(
                json!({"kind":{"type":"string","enum":["label","global","hierarchical","power","junction"]},"net":id_schema(),"position":point_schema(),"rotation":{"type":"number","minimum":-360000.0,"maximum":360000.0}}),
                &["kind", "net", "position"],
            ),
        ),
        (
            "label.move",
            "Move an existing net label, global label or junction to a new position",
            schema(
                json!({"id":id_schema(),"position":point_schema()}),
                &["id", "position"],
            ),
        ),
        (
            "pin.no_connect",
            "Mark a pin intentionally unconnected, suppressing its unconnected-pin violation",
            schema(json!({"pin":pin_schema()}), &["pin"]),
        ),
        (
            "via.place",
            "Place a plated through-hole via joining front and back copper on an existing net",
            schema(
                json!({"net":id_schema(),"position":point_schema(),"diameter":{"type":"number","exclusiveMinimum":0,"maximum":100},"drill":{"type":"number","minimum":0,"maximum":100,"description":"drilled hole diameter, millimetres"}}),
                &["net", "position", "diameter"],
            ),
        ),
        (
            "via.move",
            "Move a placed via to a new position; geometry and net are kept",
            schema(
                json!({"id":id_schema(),"position":point_schema()}),
                &["id", "position"],
            ),
        ),
        (
            "via.stitch",
            "Fill a rectangular area with a grid of stitched vias on an existing net",
            schema(
                json!({"net":id_schema(),"bounds":schema(json!({"x":{"type":"number"},"y":{"type":"number"},"width":{"type":"number","exclusiveMinimum":0},"height":{"type":"number","exclusiveMinimum":0}}),&["x","y","width","height"]),"spacing":{"type":"number","minimum":0.5,"maximum":50,"description":"grid pitch, millimetres"},"diameter":{"type":"number","exclusiveMinimum":0,"maximum":100},"drill":{"type":"number","minimum":0,"maximum":100,"description":"drilled hole diameter, millimetres"}}),
                &["net", "bounds", "diameter"],
            ),
        ),
        (
            "zone.place",
            "Place a poured copper zone: a closed polygon of copper on one layer bound to an existing net",
            schema(
                json!({"net":id_schema(),"layer":{"type":"string","enum":["F.Cu","B.Cu"]},"outline":{"type":"array","minItems":4,"maxItems":257,"items":point_schema(),"description":"closed polygon, millimetres; first vertex repeated last"}}),
                &["net", "outline"],
            ),
        ),
        (
            "zone.cutout",
            "Add a closed cutout outline that removes copper from an existing zone",
            schema(
                json!({"zone":id_schema(),"outline":{"type":"array","minItems":4,"maxItems":257,"items":point_schema(),"description":"closed polygon, millimetres; first vertex repeated last"}}),
                &["zone", "outline"],
            ),
        ),
        (
            "board.sync",
            "Reconcile schematic components and pin nets into PCB footprints; retain placements and report unassigned library symbols",
            schema(json!({}), &[]),
        ),
        (
            "board.resize",
            "Set rectangular board dimensions, in millimetres",
            schema(
                json!({"width":{"type":"number","exclusiveMinimum":0},"height":{"type":"number","exclusiveMinimum":0}}),
                &["width", "height"],
            ),
        ),
        (
            "board.rules.set",
            "Set minimum copper clearance in millimetres",
            schema(
                json!({"clearance":{"type":"number","exclusiveMinimum":0}}),
                &["clearance"],
            ),
        ),
        (
            "board.cutout.add",
            "Add a dimensioned circular Edge.Cuts cutout strictly within the board",
            schema(
                json!({"center":point_schema(),"radius":{"type":"number","exclusiveMinimum":0}}),
                &["center", "radius"],
            ),
        ),
        (
            "footprint.move",
            "Move a board footprint; existing routed tracks stay fixed",
            schema(
                json!({"id":id_schema(),"position":point_schema()}),
                &["id", "position"],
            ),
        ),
        (
            "footprint.rotate",
            "Set footprint rotation in degrees; existing tracks stay fixed",
            schema(
                json!({"id":id_schema(),"degrees":{"type":"number"}}),
                &["id", "degrees"],
            ),
        ),
        (
            "track.add",
            "Add a straight copper segment; checks report clearance errors rather than hiding them",
            schema(
                json!({"net":id_schema(),"start":point_schema(),"end":point_schema(),"width":{"type":"number","exclusiveMinimum":0},"layer":{"type":"string","enum":["F.Cu","B.Cu"]}}),
                &["net", "start", "end"],
            ),
        ),
        (
            "track.split",
            "Split one copper segment into two at a point on the track",
            schema(json!({"id":id_schema(),"at":point_schema()}), &["id", "at"]),
        ),
        (
            "track.move",
            "Translate an existing copper segment by a delta; both ends move together",
            schema(
                json!({"id":id_schema(),"delta":point_schema()}),
                &["id", "delta"],
            ),
        ),
        (
            "track.route",
            "Connect same-net pads on the chosen copper layer with an orthogonal route (not shove routing)",
            schema(
                json!({"from":schema(json!({"footprint":id_schema(),"pad":{"type":"string"}}),&["footprint","pad"]),"to":schema(json!({"footprint":id_schema(),"pad":{"type":"string"}}),&["footprint","pad"]),"width":{"type":"number","exclusiveMinimum":0},"layer":{"type":"string","enum":["F.Cu","B.Cu"]}}),
                &["from", "to"],
            ),
        ),
        (
            "align.distribute",
            "Align or evenly distribute components or board footprints along an axis",
            schema(
                json!({"ids":{"type":"array","minItems":2,"maxItems":128,"items":id_schema()},"mode":{"type":"string","enum":["left","center_x","right","top","center_y","bottom","distribute_x","distribute_y"]}}),
                &["ids", "mode"],
            ),
        ),
        (
            "object.snap",
            "Snap component or footprint placements to the nearest multiple of a grid",
            schema(
                json!({"ids":{"type":"array","minItems":1,"maxItems":128,"items":id_schema()},"grid":{"type":"number"}}),
                &["ids", "grid"],
            ),
        ),
        (
            "object.duplicate",
            "Duplicate components at an offset, each with a fresh reference and ids, bringing its footprint when one exists",
            schema(
                json!({"ids":{"type":"array","minItems":1,"maxItems":64,"items":id_schema()},"offset":point_schema()}),
                &["ids", "offset"],
            ),
        ),
        (
            "object.delete",
            "Delete a component, wire, track, via, zone, footprint, cutout or unused net by stable ID",
            schema(json!({"id":id_schema()}), &["id"]),
        ),
    ];
    entries
        .into_iter()
        .map(|(id, description, input_schema)| CommandSpec {
            id,
            description,
            input_schema,
        })
        .chain(authoring::commands())
        .collect()
}

#[derive(Clone)]
pub struct Session {
    pub project: Project,
    pub revision: u64,
    next_id: Id,
    undo: VecDeque<Project>,
    redo: VecDeque<Project>,
}
impl Default for Session {
    fn default() -> Self {
        Self {
            project: Project::default(),
            revision: 0,
            next_id: 1,
            undo: VecDeque::new(),
            redo: VecDeque::new(),
        }
    }
}
impl Session {
    fn check_revision(&self, expected: Option<u64>) -> Result<(), Error> {
        if let Some(expected) = expected
            && expected != self.revision
        {
            return Err(Error::Stale {
                expected,
                current: self.revision,
            });
        }
        Ok(())
    }
    pub fn execute(
        &mut self,
        id: &str,
        params: Value,
        expected: Option<u64>,
    ) -> Result<Value, Error> {
        let result = self.batch(
            &[Command {
                command: id.into(),
                params,
            }],
            expected,
        )?;
        Ok(
            json!({"revision":self.revision,"result":result.get("results").and_then(Value::as_array).and_then(|a|a.first()).cloned().unwrap_or(Value::Null)}),
        )
    }
    pub fn batch(&mut self, commands: &[Command], expected: Option<u64>) -> Result<Value, Error> {
        self.check_revision(expected)?;
        if commands.is_empty() || commands.len() > 128 {
            return Err(Error::Invalid("batch must contain 1–128 commands".into()));
        }
        let mut candidate = self.project.clone();
        let mut next = self.next_id;
        let mut results = vec![];
        for command in commands {
            results.push(run(
                &mut candidate,
                &mut next,
                &command.command,
                &command.params,
            )?);
            candidate.validate()?;
        }
        self.commit(candidate);
        self.next_id = next;
        Ok(json!({"revision":self.revision,"results":results}))
    }
    fn commit(&mut self, p: Project) {
        self.undo.push_back(std::mem::replace(&mut self.project, p));
        while self.undo.len() > 32 {
            self.undo.pop_front();
        }
        self.redo.clear();
        self.revision = self.revision.saturating_add(1);
    }
    pub fn undo(&mut self) -> Result<Value, Error> {
        let p = self
            .undo
            .pop_back()
            .ok_or_else(|| Error::Invalid("nothing to undo".into()))?;
        self.redo.push_back(std::mem::replace(&mut self.project, p));
        self.revision = self.revision.saturating_add(1);
        Ok(json!({"revision":self.revision}))
    }
    pub fn redo(&mut self) -> Result<Value, Error> {
        let p = self
            .redo
            .pop_back()
            .ok_or_else(|| Error::Invalid("nothing to redo".into()))?;
        self.undo.push_back(std::mem::replace(&mut self.project, p));
        self.revision = self.revision.saturating_add(1);
        Ok(json!({"revision":self.revision}))
    }
    pub fn replace(&mut self, p: Project) -> Result<Value, Error> {
        p.validate()?;
        self.next_id = self
            .next_id
            .max(p.ids().max().unwrap_or(0).saturating_add(1));
        self.commit(p);
        Ok(json!({"revision":self.revision}))
    }
    pub fn inspect(&self) -> Value {
        json!({"revision":self.revision,"project":self.project,"history":{"undo":self.undo.len(),"redo":self.redo.len()},"capabilities":{"schematic":"flat explicit pin nets with named net labels, junctions and no-connect flags; embedded multi-unit packages with independent unit placement and one physical pin map","pcb":"front SMD and plated through-hole circle/rect/roundrect/oval pads, circular/oval offset drills; two copper layers, simple tracks, plated through-hole vias including stitch grids, poured copper zones with cutouts, rectangular outline and circular cutouts","library":"native project-library symbol/footprint authoring, reusable .pcbcraft-lib collections, embedded placed copies","checks":"initial pin assignment with no-connect flag suppression and conservative copper bounds with same-pad plated layer bridges and drilled-hole contact tests; not comprehensive ERC/DRC","kicad":"native library reading and limited PCB export; no general project import or library writer","routing":"orthogonal routes on chosen copper layer; no obstacle avoidance or shove","3d":false}})
    }
    pub fn checks(&self) -> Value {
        json!({"revision":self.revision,"erc":pcbcraft_rules::erc(&self.project),"drc":pcbcraft_rules::drc(&self.project),"airwires":pcbcraft_rules::airwires(&self.project),"scope":"Initial checks: pin assignment (suppressed for pins flagged no-connect), net population, capsule tracks, rectangular edge and circular cutout clearance. Noncircular pads use sampled convex outlines with conservative chord-error clearance and inscribed connectivity bounds. Plated pads bridge the same physical pad between F.Cu/B.Cu; traces wholly inside circular/oval drills do not contact copper. Conservative false findings remain possible. No comprehensive pin-type, annular-ring manufacturing rules, zones or hierarchy checks."})
    }
    pub fn sample() -> Result<Self, Error> {
        let mut s = Self::default();
        s.execute("project.rename", json!({"name":"LED indicator"}), None)?;
        s.execute("component.add",json!({"kind":"connector","reference":"J1","value":"5V input","position":{"x":15,"y":26}}),None)?;
        s.execute(
            "component.add",
            json!({"kind":"resistor","reference":"R1","value":"330 Ω","position":{"x":40,"y":14}}),
            None,
        )?;
        s.execute(
            "component.add",
            json!({"kind":"led","reference":"D1","value":"Green LED","position":{"x":65,"y":26}}),
            None,
        )?;
        for name in ["VCC", "LED", "GND"] {
            s.execute("net.create", json!({"name":name}), None)?;
        }
        for (net, a, ap, b, bp) in [
            (4, 1, "1", 2, "1"),
            (5, 2, "2", 3, "1"),
            (6, 1, "2", 3, "2"),
        ] {
            s.execute(
                "wire.connect",
                json!({"net":net,"from":{"component":a,"pin":ap},"to":{"component":b,"pin":bp}}),
                None,
            )?;
        }
        s.execute("board.sync", json!({}), None)?;
        for (id, x, y) in [(10, 12, 20), (11, 28, 12), (12, 44, 20)] {
            s.execute(
                "footprint.move",
                json!({"id":id,"position":{"x":x,"y":y}}),
                None,
            )?;
        }
        s.execute(
            "board.cutout.add",
            json!({"center":{"x":5,"y":5},"radius":1.6}),
            None,
        )?;
        s.execute(
            "board.cutout.add",
            json!({"center":{"x":55,"y":35},"radius":1.6}),
            None,
        )?;
        s.undo.clear();
        s.redo.clear();
        Ok(s)
    }
}

fn parse<T: serde::de::DeserializeOwned>(v: &Value) -> Result<T, Error> {
    serde_json::from_value(v.clone()).map_err(|e| Error::Invalid(e.to_string()))
}
fn allocate(next: &mut Id) -> Result<Id, Error> {
    let id = *next;
    if id == 0 || id > u32::MAX as u64 {
        return Err(Error::Invalid("object ID space exhausted".into()));
    }
    *next = next.saturating_add(1);
    Ok(id)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Add {
    kind: String,
    position: Point,
    reference: Option<String>,
    value: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PlaceLibrary {
    path: String,
    symbol: String,
    position: Point,
    reference: Option<String>,
    value: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AssignFootprint {
    id: Id,
    path: String,
}
fn component_pads(c: &Component) -> Vec<Pad> {
    if let Some(library) = &c.library_footprint {
        library
            .definition
            .pads
            .iter()
            .map(|pad| Pad {
                number: pad.number.clone(),
                offset: pad.position,
                diameter: pad.size.x.min(pad.size.y),
                net: c
                    .pins
                    .iter()
                    .find(|p| p.number == pad.number)
                    .and_then(|p| p.net),
                geometry: Some(pad.clone()),
            })
            .collect()
    } else {
        c.pins
            .iter()
            .enumerate()
            .map(|(i, pin)| Pad {
                number: pin.number.clone(),
                offset: Point::new(if i == 0 { -2.5 } else { 2.5 }, 0.0),
                diameter: 1.6,
                net: pin.net,
                geometry: None,
            })
            .collect()
    }
}
fn place_definition(
    p: &mut Project,
    next: &mut Id,
    library: String,
    definition: pcbcraft_library::SymbolDefinition,
    position: Point,
    reference: Option<String>,
    value: Option<String>,
) -> Result<Value, Error> {
    let prefix = if definition.reference.is_empty() {
        "U"
    } else {
        &definition.reference
    };
    let reference = reference.unwrap_or_else(|| {
        let mut n = 1;
        while p
            .components
            .iter()
            .any(|c| c.reference == format!("{prefix}{n}"))
        {
            n += 1;
        }
        format!("{prefix}{n}")
    });
    let id = allocate(next)?;
    let component = library_component(definition, library, id, reference, position, value)?;
    p.components.push(component);
    Ok(json!({"id":id}))
}
fn assign_definition(
    p: &mut Project,
    id: Id,
    library: String,
    definition: pcbcraft_library::FootprintDefinition,
) -> Result<Value, Error> {
    definition
        .validate()
        .map_err(|e| Error::Invalid(e.to_string()))?;
    validate_footprint_support(&definition)?;
    let c = p
        .components
        .iter_mut()
        .find(|c| c.id == id)
        .ok_or_else(missing)?;
    let pins: std::collections::BTreeSet<_> = c.pins.iter().map(|p| p.number.as_str()).collect();
    let pads: std::collections::BTreeSet<_> =
        definition.pads.iter().map(|p| p.number.as_str()).collect();
    if pins != pads {
        return Err(Error::Invalid(
            "footprint pad numbers must match all physical symbol pins exactly".into(),
        ));
    }
    c.library_footprint = Some(LibraryFootprint {
        library,
        definition,
    });
    if let Some(f) = p.board.footprints.iter_mut().find(|f| f.component == id) {
        f.pads = component_pads(c);
    }
    Ok(json!({"component":id}))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Move {
    id: Id,
    position: Point,
    unit: Option<u32>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Rotate {
    id: Id,
    degrees: f64,
    unit: Option<u32>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ValueParams {
    id: Id,
    value: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Name {
    name: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Connect {
    net: Id,
    pins: Vec<PinRef>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireParams {
    net: Option<Id>,
    from: PinRef,
    to: PinRef,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DrawnWireParams {
    points: Vec<Point>,
    net: Option<Id>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GraphicParams {
    layer: String,
    shape: String,
    points: Vec<Point>,
    width: f64,
    #[serde(default)]
    text: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LabelParams {
    kind: String,
    net: Id,
    position: Point,
    #[serde(default)]
    rotation: f64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NoConnectParams {
    pin: PinRef,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ViaParams {
    net: Id,
    position: Point,
    diameter: f64,
    #[serde(default)]
    drill: f64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ZoneParams {
    net: Id,
    #[serde(default = "front")]
    layer: Layer,
    outline: Vec<Point>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ZoneCutoutParams {
    zone: Id,
    outline: Vec<Point>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StitchBounds {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StitchParams {
    net: Id,
    bounds: StitchBounds,
    diameter: f64,
    #[serde(default)]
    drill: f64,
    #[serde(default = "default_stitch_spacing")]
    spacing: f64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PositionParams {
    id: Id,
    position: Point,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DeltaParams {
    id: Id,
    delta: Point,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GraphicMoveParams {
    id: Id,
    delta: Point,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LabelMoveParams {
    id: Id,
    position: Point,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SnapParams {
    ids: Vec<Id>,
    grid: f64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DuplicateParams {
    ids: Vec<Id>,
    offset: Point,
}
/// A reference label that no component in the project already carries.
fn unique_reference(p: &Project, base: &str) -> String {
    let base: String = base.chars().take(50).collect();
    let mut n = 2;
    loop {
        let candidate = if n == 2 {
            format!("{base} copy")
        } else {
            format!("{base} copy {n}")
        };
        if !p.components.iter().any(|c| c.reference == candidate) {
            return candidate;
        }
        n += 1;
        if n > 4096 {
            return format!("{base} {}", p.components.len() + 1);
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SplitParams {
    id: Id,
    at: Point,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AlignParams {
    ids: Vec<Id>,
    mode: String,
}
/// Center of an align item's bounding box on the requested axis.
fn align_center(item: (Id, bool, f64, f64, f64, f64), axis_x: bool) -> f64 {
    if axis_x {
        (item.2 + item.4) / 2.0
    } else {
        (item.3 + item.5) / 2.0
    }
}
fn default_stitch_spacing() -> f64 {
    1.0
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Resize {
    width: f64,
    height: f64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Clearance {
    clearance: f64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Cut {
    center: Point,
    radius: f64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TrackParams {
    net: Id,
    start: Point,
    end: Point,
    #[serde(default = "default_width")]
    width: f64,
    #[serde(default = "front")]
    layer: Layer,
}
#[derive(Deserialize, Clone)]
#[serde(deny_unknown_fields)]
struct PadRef {
    footprint: Id,
    pad: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Route {
    from: PadRef,
    to: PadRef,
    #[serde(default = "default_width")]
    width: f64,
    #[serde(default = "front")]
    layer: Layer,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Object {
    id: Id,
}
fn default_width() -> f64 {
    0.35
}
fn front() -> Layer {
    Layer::Front
}
fn missing() -> Error {
    Error::Invalid("object not found".into())
}
fn assign(p: &mut Project, net: Id, pins: &[PinRef]) -> Result<(), Error> {
    if pins.is_empty() || pins.len() > 128 || !p.nets.iter().any(|n| n.id == net) {
        return Err(Error::Invalid("invalid pin list or unknown net".into()));
    }
    for r in pins {
        let pin = p
            .components
            .iter_mut()
            .find(|c| c.id == r.component)
            .and_then(|c| c.pins.iter_mut().find(|pin| pin.number == r.pin))
            .ok_or_else(missing)?;
        if pin.net.is_some_and(|n| n != net) {
            return Err(Error::Invalid(
                "pin already belongs to another net; explicit net merge is not implemented".into(),
            ));
        }
        pin.net = Some(net);
        for f in p
            .board
            .footprints
            .iter_mut()
            .filter(|f| f.component == r.component)
        {
            for pad in f.pads.iter_mut().filter(|pad| pad.number == r.pin) {
                pad.net = Some(net);
            }
        }
    }
    Ok(())
}
fn pad(p: &Project, r: &PadRef, layer: Layer) -> Result<(Point, Id), Error> {
    let f = p.footprint(r.footprint).ok_or_else(missing)?;
    let position = f.pad_position(&r.pad).ok_or_else(missing)?;
    let pad = f
        .pads
        .iter()
        .find(|pad| pad.number == r.pad)
        .ok_or_else(missing)?;
    let on_layer = pad
        .geometry
        .as_ref()
        .map(|g| g.layers.iter().any(|l| l == layer.name() || l == "*.Cu"))
        .unwrap_or(layer == Layer::Front);
    if !on_layer {
        return Err(Error::Invalid(format!(
            "pad {} has no copper on {}",
            r.pad,
            layer.name()
        )));
    }
    let net = f
        .pads
        .iter()
        .find(|pad| pad.number == r.pad)
        .and_then(|pad| pad.net)
        .ok_or_else(|| Error::Invalid("pad has no net".into()))?;
    Ok((position, net))
}
fn run(p: &mut Project, next: &mut Id, command: &str, params: &Value) -> Result<Value, Error> {
    if let Some(result) = authoring::run(p, next, command, params) {
        return result;
    }
    match command {
        "project.rename" => {
            let a: Name = parse(params)?;
            p.name = a.name;
            Ok(json!({"name":p.name}))
        }
        "component.add" => {
            let a: Add = parse(params)?;
            let template = TEMPLATES
                .iter()
                .find(|t| t.kind == a.kind)
                .ok_or_else(|| Error::Invalid("unknown symbol kind".into()))?;
            let reference = match a.reference {
                Some(r) => r,
                None => {
                    let mut n = 1;
                    while p
                        .components
                        .iter()
                        .any(|c| c.reference == format!("{}{n}", template.prefix))
                    {
                        n += 1;
                    }
                    format!("{}{n}", template.prefix)
                }
            };
            let id = allocate(next)?;
            let mut c =
                component_template(&a.kind, id, reference, a.position).ok_or_else(missing)?;
            if let Some(value) = a.value {
                c.value = value;
            }
            p.components.push(c);
            Ok(json!({"id":id}))
        }
        "component.place_library" => {
            let a: PlaceLibrary = parse(params)?;
            let path = std::path::Path::new(&a.path);
            let definition = library::symbol(path, &a.symbol)?;
            place_definition(
                p,
                next,
                path.file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("library")
                    .into(),
                definition,
                a.position,
                a.reference,
                a.value,
            )
        }
        "footprint.assign" => {
            let a: AssignFootprint = parse(params)?;
            let path = std::path::Path::new(&a.path);
            assign_definition(
                p,
                a.id,
                path.parent()
                    .and_then(|p| p.file_stem())
                    .and_then(|s| s.to_str())
                    .unwrap_or("library")
                    .into(),
                library::footprint(path)?,
            )
        }
        "component.move" => {
            let a: Move = parse(params)?;
            let c = p
                .components
                .iter_mut()
                .find(|c| c.id == a.id)
                .ok_or_else(missing)?;
            if a.unit.unwrap_or(1) == 1 {
                c.position = a.position;
            } else {
                c.units
                    .iter_mut()
                    .find(|u| Some(u.unit) == a.unit)
                    .ok_or_else(missing)?
                    .position = a.position;
            }
            Ok(json!({"id":a.id,"unit":a.unit.unwrap_or(1)}))
        }
        "component.rotate" => {
            let a: Rotate = parse(params)?;
            if !a.degrees.is_finite() {
                return Err(Error::Invalid("rotation must be finite".into()));
            }
            let c = p
                .components
                .iter_mut()
                .find(|c| c.id == a.id)
                .ok_or_else(missing)?;
            if a.unit.unwrap_or(1) == 1 {
                c.rotation = a.degrees.rem_euclid(360.0);
            } else {
                c.units
                    .iter_mut()
                    .find(|u| Some(u.unit) == a.unit)
                    .ok_or_else(missing)?
                    .rotation = a.degrees.rem_euclid(360.0);
            }
            Ok(json!({"id":a.id,"unit":a.unit.unwrap_or(1)}))
        }
        "component.value" => {
            let a: ValueParams = parse(params)?;
            p.components
                .iter_mut()
                .find(|c| c.id == a.id)
                .ok_or_else(missing)?
                .value = a.value;
            Ok(json!({"id":a.id}))
        }
        "net.create" => {
            let a: Name = parse(params)?;
            let id = allocate(next)?;
            p.nets.push(Net { id, name: a.name });
            Ok(json!({"id":id}))
        }
        "net.connect" => {
            let a: Connect = parse(params)?;
            assign(p, a.net, &a.pins)?;
            Ok(json!({"net":a.net,"pins":a.pins.len()}))
        }
        "wire.connect" => {
            let a: WireParams = parse(params)?;
            if a.from == a.to {
                return Err(Error::Invalid("wire needs two distinct pins".into()));
            }
            let from = p.pin(&a.from).ok_or_else(missing)?.net;
            let to = p.pin(&a.to).ok_or_else(missing)?.net;
            let net = if let Some(net) = a.net.or(from).or(to) {
                net
            } else {
                let id = allocate(next)?;
                let mut index = p.nets.len() + 1;
                while p.nets.iter().any(|n| n.name == format!("N${index}")) {
                    index += 1;
                }
                p.nets.push(Net {
                    id,
                    name: format!("N${index}"),
                });
                id
            };
            assign(p, net, &[a.from.clone(), a.to.clone()])?;
            let id = allocate(next)?;
            p.wires.push(Wire {
                id,
                net,
                from: a.from,
                to: a.to,
            });
            Ok(json!({"id":id,"net":net}))
        }
        "wire.draw" => {
            let a: DrawnWireParams = parse(params)?;
            if a.points.len() < 2 || a.points.len() > 256 {
                return Err(Error::Invalid("wire needs 2–256 vertices".into()));
            }
            if !a.points.iter().all(|p| p.valid()) {
                return Err(Error::Invalid("wire vertex is out of range".into()));
            }
            if a.points.windows(2).any(|w| w[0] == w[1]) {
                return Err(Error::Invalid("wire repeats a vertex".into()));
            }
            if a.net.is_some_and(|net| !p.nets.iter().any(|n| n.id == net)) {
                return Err(missing());
            }
            let id = allocate(next)?;
            let vertices = a.points.len();
            p.drawn_wires.push(DrawnWire {
                id,
                net: a.net,
                points: a.points,
            });
            Ok(json!({"id":id,"vertices":vertices}))
        }
        "wire.split" => {
            let a: SplitParams = parse(params)?;
            if !a.at.valid() {
                return Err(Error::Invalid("split point is out of range".into()));
            }
            let index = p
                .drawn_wires
                .iter()
                .position(|w| w.id == a.id)
                .ok_or_else(missing)?;
            let wire = &p.drawn_wires[index];
            let segment = (0..wire.points.len() - 1)
                .find(|i| {
                    let (from, to) = (wire.points[*i], wire.points[*i + 1]);
                    a.at.distance(from) >= 0.01
                        && a.at.distance(to) >= 0.01
                        && pcbcraft_geom::point_segment(a.at, from, to) <= 1e-4
                })
                .ok_or_else(|| {
                    Error::Invalid(
                        "split point must lie on a wire segment, clear of its vertices".into(),
                    )
                })?;
            let id = allocate(next)?;
            let net = wire.net;
            let mut head = wire.points[..=segment].to_vec();
            head.push(a.at);
            let mut tail = vec![a.at];
            tail.extend_from_slice(&wire.points[segment + 1..]);
            p.drawn_wires[index].points = head;
            p.drawn_wires.push(DrawnWire {
                id,
                net,
                points: tail,
            });
            Ok(json!({"id":id,"kept":a.id}))
        }
        "graphic.add" => {
            let a: GraphicParams = parse(params)?;
            if a.layer != "schematic" && a.layer != "board" {
                return Err(Error::Invalid(
                    "graphic layer must be schematic or board".into(),
                ));
            }
            let (min, max) = match pcbcraft_doc::Graphic::vertex_rule(&a.shape) {
                Some(rule) => rule,
                None => return Err(Error::Invalid("unknown graphic shape".into())),
            };
            if a.points.len() < min || a.points.len() > max {
                return Err(Error::Invalid(
                    "graphic has the wrong number of vertices for its shape".into(),
                ));
            }
            if !a.points.iter().all(|p| p.valid()) {
                return Err(Error::Invalid("graphic vertex is out of range".into()));
            }
            if !a.width.is_finite() || a.width <= 0.0 || a.width > 1_000.0 {
                return Err(Error::Invalid(
                    "graphic width, radius or text size must be within 0–1000mm".into(),
                ));
            }
            if a.text.len() > 1024 {
                return Err(Error::Invalid("graphic text is too long".into()));
            }
            if (a.shape == "text") != !a.text.is_empty() {
                return Err(Error::Invalid("only text graphics may carry text".into()));
            }
            let id = allocate(next)?;
            p.graphics.push(pcbcraft_doc::Graphic {
                id,
                layer: a.layer,
                shape: a.shape,
                points: a.points,
                width: a.width,
                text: a.text,
            });
            Ok(json!({"id":id}))
        }
        "graphic.move" => {
            let a: GraphicMoveParams = parse(params)?;
            let graphic = p
                .graphics
                .iter_mut()
                .find(|g| g.id == a.id)
                .ok_or_else(missing)?;
            let moved: Vec<Point> = graphic
                .points
                .iter()
                .map(|point| Point::new(point.x + a.delta.x, point.y + a.delta.y))
                .collect();
            if !moved.iter().all(|point| point.valid()) {
                return Err(Error::Invalid("graphic vertex is out of range".into()));
            }
            graphic.points = moved;
            Ok(json!({"id": a.id}))
        }
        "label.place" => {
            let a: LabelParams = parse(params)?;
            if !pcbcraft_doc::Label::KINDS.iter().any(|k| *k == a.kind) {
                return Err(Error::Invalid("unknown label kind".into()));
            }
            if !p.nets.iter().any(|n| n.id == a.net) {
                return Err(missing());
            }
            if !a.position.valid() {
                return Err(Error::Invalid("label position is out of range".into()));
            }
            if !a.rotation.is_finite() || a.rotation.abs() > 360_000.0 {
                return Err(Error::Invalid("invalid label rotation".into()));
            }
            if a.kind == "junction"
                && !p
                    .drawn_wires
                    .iter()
                    .any(|w| w.net == Some(a.net) && w.points.contains(&a.position))
            {
                return Err(Error::Invalid(
                    "junction must sit on a vertex of a drawn wire of its net".into(),
                ));
            }
            let id = allocate(next)?;
            p.labels.push(pcbcraft_doc::Label {
                id,
                kind: a.kind,
                net: a.net,
                position: a.position,
                rotation: a.rotation,
            });
            Ok(json!({"id":id}))
        }
        "label.move" => {
            let a: LabelMoveParams = parse(params)?;
            if !a.position.valid() {
                return Err(Error::Invalid("label position is out of range".into()));
            }
            let index = p
                .labels
                .iter()
                .position(|l| l.id == a.id)
                .ok_or_else(missing)?;
            let label = &p.labels[index];
            if label.kind == "junction"
                && !p
                    .drawn_wires
                    .iter()
                    .any(|w| w.net == Some(label.net) && w.points.contains(&a.position))
            {
                return Err(Error::Invalid(
                    "junction must sit on a vertex of a drawn wire of its net".into(),
                ));
            }
            p.labels[index].position = a.position;
            Ok(json!({"id": a.id}))
        }
        "pin.no_connect" => {
            let a: NoConnectParams = parse(params)?;
            if !p
                .component(a.pin.component)
                .is_some_and(|c| c.pins.iter().any(|pin| pin.number == a.pin.pin))
            {
                return Err(missing());
            }
            let component = a.pin.component;
            let connected = p.wires.iter().any(|w| {
                (w.from.component == component && w.from.pin == a.pin.pin)
                    || (w.to.component == component && w.to.pin == a.pin.pin)
            });
            if connected {
                return Err(Error::Invalid(
                    "no-connect flag conflicts with a connected pin".into(),
                ));
            }
            let duplicate = p
                .no_connects
                .iter()
                .any(|f| f.pin.component == a.pin.component && f.pin.pin == a.pin.pin);
            if duplicate {
                return Err(Error::Invalid("pin already has a no-connect flag".into()));
            }
            let id = allocate(next)?;
            p.no_connects
                .push(pcbcraft_doc::NoConnectFlag { id, pin: a.pin });
            Ok(json!({"id":id}))
        }
        "via.place" => {
            let a: ViaParams = parse(params)?;
            if !p.nets.iter().any(|n| n.id == a.net) {
                return Err(missing());
            }
            if !a.position.valid() {
                return Err(Error::Invalid("via position is out of range".into()));
            }
            if !a.diameter.is_finite() || a.diameter <= 0.0 || a.diameter > 100.0 {
                return Err(Error::Invalid("via diameter must be within 0–100mm".into()));
            }
            if !a.drill.is_finite() || a.drill < 0.0 || a.drill >= a.diameter {
                return Err(Error::Invalid(
                    "via drill must be smaller than its pad".into(),
                ));
            }
            let id = allocate(next)?;
            p.board.vias.push(pcbcraft_doc::Via {
                id,
                net: a.net,
                position: a.position,
                diameter: a.diameter,
                drill: a.drill,
            });
            Ok(json!({"id":id}))
        }
        "via.move" => {
            let a: PositionParams = parse(params)?;
            if !a.position.valid() {
                return Err(Error::Invalid("via position is out of range".into()));
            }
            let via = p
                .board
                .vias
                .iter_mut()
                .find(|v| v.id == a.id)
                .ok_or_else(missing)?;
            via.position = a.position;
            Ok(json!({"id": a.id}))
        }
        "via.stitch" => {
            let a: StitchParams = parse(params)?;
            if !p.nets.iter().any(|n| n.id == a.net) {
                return Err(missing());
            }
            if !a.diameter.is_finite() || a.diameter <= 0.0 || a.diameter > 100.0 {
                return Err(Error::Invalid("via diameter must be within 0–100mm".into()));
            }
            if !a.drill.is_finite() || a.drill < 0.0 || a.drill >= a.diameter {
                return Err(Error::Invalid(
                    "via drill must be smaller than its pad".into(),
                ));
            }
            if !a.spacing.is_finite() || !(0.5..=50.0).contains(&a.spacing) {
                return Err(Error::Invalid(
                    "stitch spacing must be within 0.5–50mm".into(),
                ));
            }
            let b = &a.bounds;
            if !b.x.is_finite()
                || !b.y.is_finite()
                || !b.width.is_finite()
                || !b.height.is_finite()
                || b.x < 0.0
                || b.y < 0.0
                || b.width <= 0.0
                || b.height <= 0.0
                || b.x + b.width > p.board.width
                || b.y + b.height > p.board.height
            {
                return Err(Error::Invalid(
                    "stitch bounds must lie inside the board".into(),
                ));
            }
            let columns = (b.width / a.spacing).floor() as i64 + 1;
            let rows = (b.height / a.spacing).floor() as i64 + 1;
            if columns * rows > 4096 {
                return Err(Error::Invalid(
                    "a stitch grid is limited to 4096 vias".into(),
                ));
            }
            let mut ids = vec![];
            for row in 0..rows {
                for column in 0..columns {
                    let position = Point::new(
                        b.x + (column as f64 + 0.5) * a.spacing,
                        b.y + (row as f64 + 0.5) * a.spacing,
                    );
                    if !position.valid() {
                        return Err(Error::Invalid("stitch via position is out of range".into()));
                    }
                    let id = allocate(next)?;
                    p.board.vias.push(pcbcraft_doc::Via {
                        id,
                        net: a.net,
                        position,
                        diameter: a.diameter,
                        drill: a.drill,
                    });
                    ids.push(id);
                }
            }
            Ok(json!({"count": ids.len(), "ids": ids}))
        }
        "zone.place" => {
            let a: ZoneParams = parse(params)?;
            if !p.nets.iter().any(|n| n.id == a.net) {
                return Err(missing());
            }
            validate_closed_outline(&a.outline).map_err(Error::Invalid)?;
            let id = allocate(next)?;
            p.board.zones.push(pcbcraft_doc::CopperZone {
                id,
                net: a.net,
                layer: a.layer,
                outline: a.outline,
                cutouts: vec![],
            });
            Ok(json!({"id":id}))
        }
        "zone.cutout" => {
            let a: ZoneCutoutParams = parse(params)?;
            let zone = p
                .board
                .zones
                .iter_mut()
                .find(|z| z.id == a.zone)
                .ok_or_else(missing)?;
            validate_closed_outline(&a.outline).map_err(Error::Invalid)?;
            zone.cutouts.push(a.outline);
            Ok(json!({"cutouts": zone.cutouts.len()}))
        }
        "board.sync" => {
            if params.as_object().is_none_or(|m| !m.is_empty()) {
                return Err(Error::Invalid("board.sync takes an empty object".into()));
            }
            let mut skipped = vec![];
            for (index, c) in p.components.iter().enumerate() {
                if c.library_symbol.is_some() && c.library_footprint.is_none() {
                    skipped.push(c.id);
                    continue;
                }
                if let Some(f) = p.board.footprints.iter_mut().find(|f| f.component == c.id) {
                    f.pads = component_pads(c);
                } else {
                    let id = allocate(next)?;
                    p.board.footprints.push(Footprint {
                        id,
                        component: c.id,
                        position: Point::new(
                            10.0 + (index % 4) as f64 * 12.0,
                            10.0 + (index / 4) as f64 * 8.0,
                        ),
                        rotation: 0.0,
                        pads: component_pads(c),
                    });
                }
            }
            Ok(
                json!({"footprints":p.board.footprints.iter().map(|f|json!({"id":f.id,"component":f.component})).collect::<Vec<_>>(),"unassigned_components":skipped}),
            )
        }
        "board.resize" => {
            let a: Resize = parse(params)?;
            p.board.width = a.width;
            p.board.height = a.height;
            Ok(json!({"width":a.width,"height":a.height}))
        }
        "board.rules.set" => {
            let a: Clearance = parse(params)?;
            p.board.clearance = a.clearance;
            Ok(json!({"clearance":a.clearance}))
        }
        "board.cutout.add" => {
            let a: Cut = parse(params)?;
            let id = allocate(next)?;
            p.board.cutouts.push(Cutout {
                id,
                center: a.center,
                radius: a.radius,
            });
            Ok(json!({"id":id}))
        }
        "footprint.move" => {
            let a: Move = parse(params)?;
            if a.unit.is_some() {
                return Err(Error::Invalid(
                    "PCB footprints do not have schematic units".into(),
                ));
            }
            p.board
                .footprints
                .iter_mut()
                .find(|f| f.id == a.id)
                .ok_or_else(missing)?
                .position = a.position;
            Ok(json!({"id":a.id}))
        }
        "footprint.rotate" => {
            let a: Rotate = parse(params)?;
            if a.unit.is_some() {
                return Err(Error::Invalid(
                    "PCB footprints do not have schematic units".into(),
                ));
            }
            if !a.degrees.is_finite() {
                return Err(Error::Invalid("rotation must be finite".into()));
            }
            p.board
                .footprints
                .iter_mut()
                .find(|f| f.id == a.id)
                .ok_or_else(missing)?
                .rotation = a.degrees.rem_euclid(360.0);
            Ok(json!({"id":a.id}))
        }
        "track.add" => {
            let a: TrackParams = parse(params)?;
            let id = allocate(next)?;
            p.board.tracks.push(Track {
                id,
                net: a.net,
                start: a.start,
                end: a.end,
                width: a.width,
                layer: a.layer,
            });
            Ok(json!({"id":id}))
        }
        "track.split" => {
            let a: SplitParams = parse(params)?;
            if !a.at.valid() {
                return Err(Error::Invalid("split point is out of range".into()));
            }
            let index = p
                .board
                .tracks
                .iter()
                .position(|t| t.id == a.id)
                .ok_or_else(missing)?;
            let track = &p.board.tracks[index];
            if a.at.distance(track.start) < 0.01 || a.at.distance(track.end) < 0.01 {
                return Err(Error::Invalid(
                    "split point must sit clear of both track ends".into(),
                ));
            }
            if pcbcraft_geom::point_segment(a.at, track.start, track.end) > 1e-4 {
                return Err(Error::Invalid("split point must lie on the track".into()));
            }
            let id = allocate(next)?;
            let (net, width, layer, end) = (track.net, track.width, track.layer, track.end);
            p.board.tracks[index].end = a.at;
            p.board.tracks.push(Track {
                id,
                net,
                start: a.at,
                end,
                width,
                layer,
            });
            Ok(json!({"id":id,"kept":a.id}))
        }
        "track.move" => {
            let a: DeltaParams = parse(params)?;
            let track = p
                .board
                .tracks
                .iter_mut()
                .find(|t| t.id == a.id)
                .ok_or_else(missing)?;
            let start = Point::new(track.start.x + a.delta.x, track.start.y + a.delta.y);
            let end = Point::new(track.end.x + a.delta.x, track.end.y + a.delta.y);
            if !start.valid() || !end.valid() {
                return Err(Error::Invalid("track endpoint is out of range".into()));
            }
            track.start = start;
            track.end = end;
            Ok(json!({"id": a.id}))
        }
        "track.route" => {
            let a: Route = parse(params)?;
            let (start, net) = pad(p, &a.from, a.layer)?;
            let (end, end_net) = pad(p, &a.to, a.layer)?;
            if net != end_net || start.distance(end) < 1e-6 || !positive(a.width) {
                return Err(Error::Invalid(
                    "route requires distinct pads on the same net and a valid width".into(),
                ));
            }
            let bend = Point::new(end.x, start.y);
            let mut ids = vec![];
            for (from, to) in [(start, bend), (bend, end)] {
                if from.distance(to) > 1e-6 {
                    let id = allocate(next)?;
                    ids.push(id);
                    p.board.tracks.push(Track {
                        id,
                        net,
                        start: from,
                        end: to,
                        width: a.width,
                        layer: a.layer,
                    });
                }
            }
            Ok(json!({"ids":ids,"net":net}))
        }
        "align.distribute" => {
            let a: AlignParams = parse(params)?;
            let mode = a.mode.as_str();
            if a.ids.len() < 2 {
                return Err(Error::Invalid("alignment needs at least two items".into()));
            }
            if !matches!(
                mode,
                "left"
                    | "center_x"
                    | "right"
                    | "top"
                    | "center_y"
                    | "bottom"
                    | "distribute_x"
                    | "distribute_y"
            ) {
                return Err(Error::Invalid("unknown alignment mode".into()));
            }
            let mut seen: Vec<Id> = vec![];
            let mut items: Vec<(Id, bool, f64, f64, f64, f64)> = vec![];
            for id in &a.ids {
                if seen.contains(id) {
                    continue;
                }
                seen.push(*id);
                let points: Vec<Point> = if let Some(c) = p.component(*id) {
                    c.schematic_views()
                        .iter()
                        .flat_map(|view| {
                            view.drawing_points()
                                .into_iter()
                                .map(move |point| point.rotate(view.rotation).offset(view.position))
                        })
                        .collect()
                } else if let Some(f) = p.footprint(*id) {
                    f.drawing_points(p)
                        .into_iter()
                        .map(|point| point.rotate(f.rotation).offset(f.position))
                        .collect()
                } else {
                    return Err(missing());
                };
                if points.is_empty() {
                    return Err(Error::Invalid("item has no drawing outline".into()));
                }
                let min = points
                    .iter()
                    .fold(Point::new(f64::INFINITY, f64::INFINITY), |m, point| {
                        Point::new(m.x.min(point.x), m.y.min(point.y))
                    });
                let max = points.iter().fold(
                    Point::new(f64::NEG_INFINITY, f64::NEG_INFINITY),
                    |m, point| Point::new(m.x.max(point.x), m.y.max(point.y)),
                );
                items.push((*id, p.footprint(*id).is_some(), min.x, min.y, max.x, max.y));
            }
            let left = items.iter().map(|i| i.2).fold(f64::INFINITY, f64::min);
            let top = items.iter().map(|i| i.3).fold(f64::INFINITY, f64::min);
            let right = items.iter().map(|i| i.4).fold(f64::NEG_INFINITY, f64::max);
            let bottom = items.iter().map(|i| i.5).fold(f64::NEG_INFINITY, f64::max);
            if !(left.is_finite() && top.is_finite() && right.is_finite() && bottom.is_finite()) {
                return Err(Error::Invalid("item outlines are out of range".into()));
            }
            let mut plan: Vec<(Id, bool, f64, f64)> = vec![];
            if mode == "distribute_x" || mode == "distribute_y" {
                let axis_x = mode == "distribute_x";
                let mut order: Vec<usize> = (0..items.len()).collect();
                order.sort_by(|a, b| {
                    let ca = align_center(items[*a], axis_x);
                    let cb = align_center(items[*b], axis_x);
                    ca.partial_cmp(&cb).unwrap_or(std::cmp::Ordering::Equal)
                });
                let first = align_center(items[order[0]], axis_x);
                let last = align_center(items[*order.last().ok_or_else(missing)?], axis_x);
                let steps = order.len() - 1;
                for (step, item) in order.iter().enumerate().skip(1) {
                    if step == steps {
                        continue;
                    }
                    let target = first + (last - first) * step as f64 / steps as f64;
                    let delta = target - align_center(items[*item], axis_x);
                    plan.push((
                        items[*item].0,
                        items[*item].1,
                        if axis_x { delta } else { 0.0 },
                        if axis_x { 0.0 } else { delta },
                    ));
                }
            } else {
                for (id, footprint, minx, miny, maxx, maxy) in &items {
                    let (dx, dy) = match mode {
                        "left" => (left - minx, 0.0),
                        "right" => (right - maxx, 0.0),
                        "center_x" => ((left + right) / 2.0 - (minx + maxx) / 2.0, 0.0),
                        "top" => (0.0, top - miny),
                        "bottom" => (0.0, bottom - maxy),
                        _ => (0.0, (top + bottom) / 2.0 - (miny + maxy) / 2.0),
                    };
                    plan.push((*id, *footprint, dx, dy));
                }
            }
            for (id, footprint, dx, dy) in plan {
                if footprint {
                    let f = p
                        .board
                        .footprints
                        .iter_mut()
                        .find(|f| f.id == id)
                        .ok_or_else(missing)?;
                    let position = Point::new(f.position.x + dx, f.position.y + dy);
                    if !position.valid() {
                        return Err(Error::Invalid("aligned position is out of range".into()));
                    }
                    f.position = position;
                } else {
                    let c = p
                        .components
                        .iter_mut()
                        .find(|c| c.id == id)
                        .ok_or_else(missing)?;
                    let position = Point::new(c.position.x + dx, c.position.y + dy);
                    if !position.valid() {
                        return Err(Error::Invalid("aligned position is out of range".into()));
                    }
                    c.position = position;
                    for unit in &mut c.units {
                        let position = Point::new(unit.position.x + dx, unit.position.y + dy);
                        if !position.valid() {
                            return Err(Error::Invalid("aligned position is out of range".into()));
                        }
                        unit.position = position;
                    }
                }
            }
            Ok(json!({"aligned": seen.len(), "mode": mode}))
        }
        "object.snap" => {
            let a: SnapParams = parse(params)?;
            if a.ids.is_empty() || a.ids.len() > 128 {
                return Err(Error::Invalid("snap accepts 1–128 items".into()));
            }
            if !(0.05..=25.0).contains(&a.grid) {
                return Err(Error::Invalid("grid must be within 0.05–25mm".into()));
            }
            let mut seen: Vec<Id> = vec![];
            for id in &a.ids {
                if seen.contains(id) {
                    continue;
                }
                if p.component(*id).is_none() && p.footprint(*id).is_none() {
                    return Err(missing());
                }
                seen.push(*id);
            }
            let snap = |value: f64| (value / a.grid).round() * a.grid;
            let mut moved = 0usize;
            for c in &mut p.components {
                if !seen.contains(&c.id) {
                    continue;
                }
                let position = Point::new(snap(c.position.x), snap(c.position.y));
                if !position.valid() {
                    return Err(Error::Invalid("snapped position is out of range".into()));
                }
                if position != c.position {
                    c.position = position;
                    moved += 1;
                }
                for unit in &mut c.units {
                    let position = Point::new(snap(unit.position.x), snap(unit.position.y));
                    if !position.valid() {
                        return Err(Error::Invalid("snapped position is out of range".into()));
                    }
                    unit.position = position;
                }
            }
            for f in &mut p.board.footprints {
                if !seen.contains(&f.id) {
                    continue;
                }
                let position = Point::new(snap(f.position.x), snap(f.position.y));
                if !position.valid() {
                    return Err(Error::Invalid("snapped position is out of range".into()));
                }
                if position != f.position {
                    f.position = position;
                    moved += 1;
                }
            }
            Ok(json!({"moved": moved, "grid": a.grid}))
        }
        "object.duplicate" => {
            let a: DuplicateParams = parse(params)?;
            if a.ids.is_empty() || a.ids.len() > 64 {
                return Err(Error::Invalid("duplicate accepts 1–64 items".into()));
            }
            let mut seen: Vec<Id> = vec![];
            let mut clones: Vec<(Component, Option<Footprint>)> = vec![];
            for id in &a.ids {
                if seen.contains(id) {
                    continue;
                }
                seen.push(*id);
                let mut c = p.component(*id).ok_or_else(missing)?.clone();
                c.position = Point::new(c.position.x + a.offset.x, c.position.y + a.offset.y);
                if !c.position.valid() {
                    return Err(Error::Invalid("duplicate position is out of range".into()));
                }
                for unit in &mut c.units {
                    unit.position =
                        Point::new(unit.position.x + a.offset.x, unit.position.y + a.offset.y);
                    if !unit.position.valid() {
                        return Err(Error::Invalid("duplicate position is out of range".into()));
                    }
                }
                let footprint = p
                    .board
                    .footprints
                    .iter()
                    .find(|f| f.component == *id)
                    .cloned()
                    .map(|mut f| {
                        f.position =
                            Point::new(f.position.x + a.offset.x, f.position.y + a.offset.y);
                        f
                    });
                if footprint.as_ref().is_some_and(|f| !f.position.valid()) {
                    return Err(Error::Invalid("duplicate position is out of range".into()));
                }
                clones.push((c, footprint));
            }
            let mut duplicates = vec![];
            for (mut c, footprint) in clones {
                c.id = allocate(next)?;
                c.reference = unique_reference(p, &c.reference);
                let component = c.id;
                let mut entry = serde_json::Map::new();
                entry.insert("component".into(), json!(component));
                p.components.push(c);
                if let Some(mut f) = footprint {
                    f.id = allocate(next)?;
                    f.component = component;
                    entry.insert("footprint".into(), json!(f.id));
                    p.board.footprints.push(f);
                }
                duplicates.push(Value::Object(entry));
            }
            Ok(json!({"count": duplicates.len(), "duplicates": duplicates}))
        }
        "object.delete" => {
            let a: Object = parse(params)?;
            if !p.ids().any(|id| id == a.id) {
                return Err(missing());
            }
            if p.nets.iter().any(|n| n.id == a.id)
                && (p
                    .components
                    .iter()
                    .flat_map(|c| &c.pins)
                    .any(|pin| pin.net == Some(a.id))
                    || p.board.tracks.iter().any(|t| t.net == a.id))
            {
                return Err(Error::Invalid("cannot delete a net that is in use".into()));
            }
            p.components.retain(|c| c.id != a.id);
            p.nets.retain(|n| n.id != a.id);
            p.wires
                .retain(|w| w.id != a.id && w.from.component != a.id && w.to.component != a.id);
            p.drawn_wires
                .retain(|w| w.id != a.id && w.net.is_none_or(|n| n != a.id));
            p.graphics.retain(|g| g.id != a.id);
            p.labels.retain(|l| l.id != a.id && l.net != a.id);
            p.no_connects
                .retain(|f| f.id != a.id && f.pin.component != a.id);
            p.board
                .footprints
                .retain(|f| f.id != a.id && f.component != a.id);
            p.board.tracks.retain(|t| t.id != a.id && t.net != a.id);
            p.board.vias.retain(|v| v.id != a.id && v.net != a.id);
            p.board.zones.retain(|z| z.id != a.id && z.net != a.id);
            p.board.cutouts.retain(|c| c.id != a.id);
            Ok(json!({"deleted":a.id}))
        }
        _ => Err(Error::Invalid(format!("unknown command: {command}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn library_fixture(name: &str) -> String {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../library/tests/fixtures")
            .join(name)
            .to_string_lossy()
            .into_owned()
    }
    #[test]
    fn drawn_wires_keep_vertices_bind_nets_and_undo_cleanly() -> Result<(), Error> {
        let mut s = Session::default();
        let net = s.execute("net.create", json!({"name":"N$1"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        s.execute(
            "wire.draw",
            json!({"points":[{"x":10.0,"y":10.0},{"x":20.0,"y":10.0},{"x":20.0,"y":20.0}],"net":net}),
            None,
        )?;
        assert_eq!(s.project.drawn_wires.len(), 1);
        assert_eq!(s.project.drawn_wires[0].points.len(), 3);
        assert_eq!(s.project.drawn_wires[0].net, Some(net));
        assert!(s.project.validate().is_ok());
        // A wire may be drawn before its net exists, and stays unconnected.
        s.execute(
            "wire.draw",
            json!({"points":[{"x":0.0,"y":0.0},{"x":5.0,"y":0.0}]}),
            None,
        )?;
        assert_eq!(s.project.drawn_wires[1].net, None);
        // Repeated vertices and unknown nets are rejected without touching state.
        assert!(
            s.execute(
                "wire.draw",
                json!({"points":[{"x":1.0,"y":1.0},{"x":1.0,"y":1.0}]}),
                None
            )
            .is_err()
        );
        assert!(
            s.execute(
                "wire.draw",
                json!({"points":[{"x":1.0,"y":1.0},{"x":2.0,"y":1.0}],"net":9999}),
                None
            )
            .is_err()
        );
        assert_eq!(s.project.drawn_wires.len(), 2);
        let drawn = s.project.drawn_wires[0].id;
        s.execute("object.delete", json!({"id":drawn}), None)?;
        assert_eq!(s.project.drawn_wires.len(), 1);
        s.undo()?;
        assert_eq!(s.project.drawn_wires.len(), 2);
        Ok(())
    }
    #[test]
    fn graphic_add_validates_each_shape_and_deletes_and_undoes_cleanly() -> Result<(), Error> {
        let mut s = Session::default();
        for params in [
            json!({"layer":"schematic","shape":"line","points":[{"x":1.0,"y":1.0},{"x":10.0,"y":1.0}],"width":0.2}),
            json!({"layer":"schematic","shape":"rect","points":[{"x":0.0,"y":0.0},{"x":5.0,"y":3.0}],"width":0.2}),
            json!({"layer":"board","shape":"circle","points":[{"x":10.0,"y":10.0}],"width":2.0}),
            json!({"layer":"board","shape":"arc","points":[{"x":0.0,"y":0.0},{"x":5.0,"y":7.0},{"x":10.0,"y":0.0}],"width":0.2}),
            json!({"layer":"board","shape":"polygon","points":[{"x":0.0,"y":0.0},{"x":4.0,"y":0.0},{"x":2.0,"y":3.0}],"width":0.1}),
            json!({"layer":"schematic","shape":"text","points":[{"x":2.0,"y":2.0}],"width":1.5,"text":"REV B"}),
        ] {
            s.execute("graphic.add", params, None)?;
        }
        assert_eq!(s.project.graphics.len(), 6);
        assert!(s.project.validate().is_ok());
        assert_eq!(
            s.project
                .graphics
                .iter()
                .filter(|g| g.shape == "circle")
                .flat_map(|g| g.outline().unwrap_or_default())
                .count(),
            25
        );
        // Unknown shapes, wrong vertex counts and misuse of text are rejected
        // without touching state.
        for params in [
            json!({"layer":"schematic","shape":"spline","points":[{"x":1.0,"y":1.0}],"width":0.2}),
            json!({"layer":"schematic","shape":"line","points":[{"x":1.0,"y":1.0}],"width":0.2}),
            json!({"layer":"schematic","shape":"text","points":[{"x":1.0,"y":1.0}],"width":1.0}),
            json!({"layer":"schematic","shape":"line","points":[{"x":1.0,"y":1.0},{"x":2.0,"y":1.0}],"width":0.2,"text":"oops"}),
            json!({"layer":"copper","shape":"line","points":[{"x":1.0,"y":1.0},{"x":2.0,"y":1.0}],"width":0.2}),
            json!({"layer":"schematic","shape":"line","points":[{"x":1.0,"y":1.0},{"x":2.0,"y":1.0}],"width":0.0}),
        ] {
            assert!(s.execute("graphic.add", params, None).is_err());
        }
        assert_eq!(s.project.graphics.len(), 6);
        let graphic = s.project.graphics[2].id;
        s.execute("object.delete", json!({"id":graphic}), None)?;
        assert_eq!(s.project.graphics.len(), 5);
        s.undo()?;
        assert_eq!(s.project.graphics.len(), 6);
        Ok(())
    }
    #[test]
    fn labels_and_no_connect_flags_validate_bind_and_undo_cleanly() -> Result<(), Error> {
        let mut s = Session::default();
        let net = s.execute("net.create", json!({"name":"VCC"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        for kind in ["label", "global", "hierarchical", "power"] {
            s.execute(
                "label.place",
                json!({"kind":kind,"net":net,"position":{"x":10.0,"y":5.0},"rotation":90.0}),
                None,
            )?;
        }
        assert_eq!(s.project.labels.len(), 4);
        assert!(s.project.validate().is_ok());
        // A junction needs a drawn wire of its net under one of its vertices.
        assert!(
            s.execute(
                "label.place",
                json!({"kind":"junction","net":net,"position":{"x":10.0,"y":5.0}}),
                None
            )
            .is_err()
        );
        s.execute(
            "wire.draw",
            json!({"points":[{"x":10.0,"y":5.0},{"x":20.0,"y":5.0}],"net":net}),
            None,
        )?;
        s.execute(
            "label.place",
            json!({"kind":"junction","net":net,"position":{"x":10.0,"y":5.0}}),
            None,
        )?;
        assert_eq!(s.project.labels.len(), 5);
        // Unknown kinds, unknown nets and off-wire junctions change nothing.
        for params in [
            json!({"kind":"port","net":net,"position":{"x":1.0,"y":1.0}}),
            json!({"kind":"label","net":9999,"position":{"x":1.0,"y":1.0}}),
            json!({"kind":"junction","net":net,"position":{"x":15.0,"y":9.0}}),
        ] {
            assert!(s.execute("label.place", params, None).is_err());
        }
        assert_eq!(s.project.labels.len(), 5);
        // No-connect flags need a real, unconnected pin.
        s.execute(
            "component.add",
            json!({"kind":"resistor","reference":"R9","value":"1 k","position":{"x":50.0,"y":30.0}}),
            None,
        )?;
        let resistor = s
            .project
            .components
            .last()
            .ok_or_else(|| "missing resistor".to_string())?
            .id;
        let other = s.project.components[0].id;
        s.execute(
            "pin.no_connect",
            json!({"pin":{"component":resistor,"pin":"1"}}),
            None,
        )?;
        assert_eq!(s.project.no_connects.len(), 1);
        for params in [
            json!({"pin":{"component":9999,"pin":"1"}}),
            json!({"pin":{"component":resistor,"pin":"7"}}),
            json!({"pin":{"component":other,"pin":"1"}}),
            json!({"pin":{"component":resistor,"pin":"1"}}),
        ] {
            assert!(s.execute("pin.no_connect", params, None).is_err());
        }
        assert_eq!(s.project.no_connects.len(), 1);
        // Deleting the net removes the labels that named it.
        s.execute("object.delete", json!({"id":net}), None)?;
        assert!(s.project.labels.is_empty());
        s.undo()?;
        assert_eq!(s.project.labels.len(), 5);
        assert_eq!(s.project.no_connects.len(), 1);
        Ok(())
    }
    #[test]
    fn via_place_validates_binds_to_nets_and_undoes_cleanly() -> Result<(), Error> {
        let mut s = Session::default();
        let net = s.execute("net.create", json!({"name":"GND"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        s.execute(
            "via.place",
            json!({"net":net,"position":{"x":10.0,"y":10.0},"diameter":1.4,"drill":0.4}),
            None,
        )?;
        s.execute(
            "via.place",
            json!({"net":net,"position":{"x":12.0,"y":10.0},"diameter":1.0}),
            None,
        )?;
        assert_eq!(s.project.board.vias.len(), 2);
        assert!(s.project.validate().is_ok());
        for params in [
            json!({"net":9999,"position":{"x":10.0,"y":10.0},"diameter":1.4}),
            json!({"net":net,"position":{"x":10.0,"y":10.0},"diameter":0.0}),
            json!({"net":net,"position":{"x":10.0,"y":10.0},"diameter":1.4,"drill":1.4}),
            json!({"net":net,"position":{"x":10.0,"y":10.0},"diameter":1.4,"drill":-0.1}),
            json!({"net":net,"position":{"x":10.0,"y":10.0},"diameter":101.0}),
        ] {
            assert!(s.execute("via.place", params, None).is_err());
        }
        assert_eq!(s.project.board.vias.len(), 2);
        let via = s.project.board.vias[0].id;
        s.execute("object.delete", json!({"id":via}), None)?;
        assert_eq!(s.project.board.vias.len(), 1);
        // Deleting the net removes the vias that carry it.
        s.execute("object.delete", json!({"id":net}), None)?;
        assert!(s.project.board.vias.is_empty());
        s.undo()?;
        assert_eq!(s.project.board.vias.len(), 1);
        Ok(())
    }
    #[test]
    fn via_stitch_places_grids_and_validates_without_mutating() -> Result<(), Error> {
        let mut s = Session::default();
        let net = s.execute("net.create", json!({"name":"GND"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let out = s.execute(
            "via.stitch",
            json!({"net":net,"bounds":{"x":5.0,"y":5.0,"width":10.0,"height":10.0},"spacing":5.0,"diameter":1.4,"drill":0.4}),
            None,
        )?;
        assert_eq!(out["result"]["count"], 9);
        assert_eq!(s.project.board.vias.len(), 9);
        assert_eq!(s.project.board.vias[0].position, Point::new(7.5, 7.5));
        assert_eq!(s.project.board.vias[8].position, Point::new(17.5, 17.5));
        assert!(s.project.validate().is_ok());
        for params in [
            json!({"net":9999,"bounds":{"x":5.0,"y":5.0,"width":10.0,"height":10.0},"diameter":1.4}),
            json!({"net":net,"bounds":{"x":5.0,"y":5.0,"width":10.0,"height":10.0},"diameter":1.4,"spacing":0.4}),
            json!({"net":net,"bounds":{"x":5.0,"y":5.0,"width":10.0,"height":10.0},"diameter":1.4,"spacing":51.0}),
            json!({"net":net,"bounds":{"x":50.0,"y":5.0,"width":20.0,"height":10.0},"diameter":1.4}),
            json!({"net":net,"bounds":{"x":0.0,"y":0.0,"width":100.0,"height":100.0},"diameter":1.4,"spacing":0.5}),
            json!({"net":net,"bounds":{"x":5.0,"y":5.0,"width":10.0,"height":10.0},"diameter":1.4,"drill":1.4}),
        ] {
            assert!(s.execute("via.stitch", params, None).is_err());
        }
        assert_eq!(s.project.board.vias.len(), 9);
        s.undo()?;
        assert!(s.project.board.vias.is_empty());
        Ok(())
    }
    #[test]
    fn align_and_distribute_move_groups_and_undo_cleanly() -> Result<(), Error> {
        let mut s = Session::default();
        let mut ids = vec![];
        for (x, y) in [(10.0, 10.0), (20.0, 14.0), (35.0, 8.0)] {
            let out = s.execute(
                "component.add",
                json!({"kind":"resistor","position":{"x":x,"y":y}}),
                None,
            )?;
            ids.push(out["result"]["id"].as_u64().ok_or_else(missing)?);
        }
        s.execute("align.distribute", json!({"ids":ids,"mode":"left"}), None)?;
        for c in &s.project.components {
            assert_eq!(c.position.x, 10.0, "left edge alignment");
        }
        s.undo()?;
        assert_eq!(s.project.components[1].position.x, 20.0);
        s.execute("align.distribute", json!({"ids":ids,"mode":"top"}), None)?;
        for c in &s.project.components {
            assert_eq!(c.position.y, 8.0, "top edge alignment");
        }
        s.execute(
            "align.distribute",
            json!({"ids":ids,"mode":"distribute_x"}),
            None,
        )?;
        let xs: Vec<f64> = s.project.components.iter().map(|c| c.position.x).collect();
        assert_eq!(
            xs,
            vec![10.0, 22.5, 35.0],
            "even distribution between the ends"
        );
        let sync = s.execute("board.sync", json!({}), None)?;
        let fp_ids: Vec<u64> = sync["result"]["footprints"]
            .as_array()
            .ok_or_else(missing)?
            .iter()
            .map(|f| f["id"].as_u64().unwrap_or_default())
            .collect();
        s.execute(
            "footprint.move",
            json!({"id":fp_ids[2],"position":{"x":30.0,"y":25.0}}),
            None,
        )?;
        s.execute("align.distribute", json!({"ids":fp_ids,"mode":"top"}), None)?;
        for f in &s.project.board.footprints {
            assert_eq!(f.position.y, 10.0, "footprint top alignment");
        }
        for params in [
            json!({"ids":[ids[0],9999],"mode":"left"}),
            json!({"ids":ids,"mode":"sideways"}),
            json!({"ids":[ids[0]],"mode":"left"}),
            json!({"ids":ids,"mode":"left","extra":1}),
        ] {
            assert!(s.execute("align.distribute", params, None).is_err());
        }
        let xs: Vec<f64> = s.project.components.iter().map(|c| c.position.x).collect();
        assert_eq!(
            xs,
            vec![10.0, 22.5, 35.0],
            "rejections must not move anything"
        );
        let ys: Vec<f64> = s
            .project
            .board
            .footprints
            .iter()
            .map(|f| f.position.y)
            .collect();
        assert_eq!(ys, vec![10.0, 10.0, 10.0]);
        Ok(())
    }
    #[test]
    fn splitting_tracks_and_wires_preserves_geometry_and_rejects_bad_points() -> Result<(), Error> {
        let mut s = Session::default();
        let net = s.execute("net.create", json!({"name":"SIG"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let wire = s.execute(
            "wire.draw",
            json!({"points":[{"x":10.0,"y":10.0},{"x":20.0,"y":10.0},{"x":20.0,"y":20.0}],"net":net}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let tail = s.execute(
            "wire.split",
            json!({"id":wire,"at":{"x":15.0,"y":10.0}}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        assert_eq!(s.project.drawn_wires[0].points.len(), 2);
        assert_eq!(s.project.drawn_wires[1].points.len(), 3);
        assert_eq!(s.project.drawn_wires[1].points[0], Point::new(15.0, 10.0));
        assert_eq!(s.project.drawn_wires[0].net, Some(net));
        assert_eq!(s.project.drawn_wires[1].net, Some(net));
        assert_eq!(s.project.drawn_wires[1].id, tail);
        assert!(s.project.validate().is_ok());
        let track = s.execute(
            "track.add",
            json!({"net":net,"start":{"x":30.0,"y":30.0},"end":{"x":40.0,"y":30.0},"width":0.25,"layer":"B.Cu"}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let second = s.execute(
            "track.split",
            json!({"id":track,"at":{"x":35.0,"y":30.0}}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        assert_eq!(s.project.board.tracks[0].end, Point::new(35.0, 30.0));
        assert_eq!(s.project.board.tracks[1].start, Point::new(35.0, 30.0));
        assert_eq!(s.project.board.tracks[1].end, Point::new(40.0, 30.0));
        assert_eq!(s.project.board.tracks[1].layer, pcbcraft_doc::Layer::Back);
        assert_eq!(s.project.board.tracks[1].net, net);
        assert!(s.project.validate().is_ok());
        for params in [
            json!({"id":wire,"at":{"x":15.0,"y":12.0}}),
            json!({"id":wire,"at":{"x":20.0,"y":10.0}}),
            json!({"id":9999,"at":{"x":15.0,"y":10.0}}),
            json!({"id":track,"at":{"x":35.0,"y":31.0}}),
            json!({"id":track,"at":{"x":30.005,"y":30.0}}),
            json!({"id":track,"at":{"x":1e6,"y":30.0}}),
        ] {
            let command =
                if params["id"].as_u64() == Some(track) || params["id"].as_u64() == Some(9999) {
                    "track.split"
                } else {
                    "wire.split"
                };
            assert!(s.execute(command, params, None).is_err());
        }
        assert_eq!(s.project.board.tracks.len(), 2);
        assert_eq!(s.project.drawn_wires.len(), 2);
        assert_eq!(s.project.board.tracks[1].id, second);
        s.undo()?;
        assert_eq!(s.project.board.tracks.len(), 1);
        assert_eq!(s.project.board.tracks[0].end, Point::new(40.0, 30.0));
        assert_eq!(s.project.drawn_wires[0].points.len(), 2);
        s.undo()?;
        assert!(s.project.board.tracks.is_empty());
        assert_eq!(s.project.drawn_wires.len(), 2);
        s.undo()?;
        assert_eq!(s.project.drawn_wires.len(), 1);
        assert_eq!(s.project.drawn_wires[0].points.len(), 3);
        Ok(())
    }
    #[test]
    fn duplicating_components_renews_references_and_brings_footprints() -> Result<(), Error> {
        let mut s = Session::default();
        let r1 = s.execute(
            "component.add",
            json!({"kind":"resistor","position":{"x":10.0,"y":10.0},"reference":"R1"}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        s.execute(
            "component.add",
            json!({"kind":"capacitor","position":{"x":30.0,"y":10.0},"reference":"C1"}),
            None,
        )?;
        s.execute("board.sync", json!({}), None)?;
        let out = s.execute(
            "object.duplicate",
            json!({"ids":[r1],"offset":{"x":5.0,"y":5.0}}),
            None,
        )?;
        let new_id = out["result"]["duplicates"][0]["component"]
            .as_u64()
            .ok_or_else(missing)?;
        let new_fp = out["result"]["duplicates"][0]["footprint"]
            .as_u64()
            .ok_or_else(missing)?;
        assert_eq!(s.project.components.len(), 3);
        let dup = s.project.component(new_id).ok_or_else(missing)?.clone();
        assert_eq!(dup.reference, "R1 copy");
        assert_eq!(dup.position, Point::new(15.0, 15.0));
        assert_eq!(dup.kind, "resistor");
        let fp = s.project.footprint(new_fp).ok_or_else(missing)?.clone();
        assert_eq!(fp.component, new_id);
        let original = s
            .project
            .board
            .footprints
            .iter()
            .find(|f| f.component == r1)
            .cloned()
            .ok_or_else(missing)?;
        assert_eq!(
            fp.position,
            Point::new(original.position.x + 5.0, original.position.y + 5.0)
        );
        assert_eq!(fp.pads, original.pads);
        assert!(s.project.validate().is_ok());
        s.execute(
            "object.duplicate",
            json!({"ids":[r1],"offset":{"x":0.0,"y":10.0}}),
            None,
        )?;
        let references: Vec<String> = s
            .project
            .components
            .iter()
            .map(|c| c.reference.clone())
            .collect();
        assert_eq!(references, vec!["R1", "C1", "R1 copy", "R1 copy 3"]);
        for params in [
            json!({"ids":[9999],"offset":{"x":1.0,"y":1.0}}),
            json!({"ids":[],"offset":{"x":1.0,"y":1.0}}),
            json!({"ids":[r1],"offset":{"x":1e6,"y":0.0}}),
            json!({"ids":[r1],"offset":{"x":1.0,"y":1.0},"extra":1}),
        ] {
            assert!(s.execute("object.duplicate", params, None).is_err());
        }
        assert_eq!(s.project.components.len(), 4);
        assert_eq!(s.project.board.footprints.len(), 4);
        s.undo()?;
        assert_eq!(s.project.components.len(), 3);
        assert_eq!(s.project.board.footprints.len(), 3);
        Ok(())
    }
    #[test]
    fn snapping_moves_placements_to_the_grid_only_when_needed() -> Result<(), Error> {
        let mut s = Session::default();
        let r1 = s.execute(
            "component.add",
            json!({"kind":"resistor","position":{"x":11.4,"y":13.7},"reference":"R1"}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let r2 = s.execute(
            "component.add",
            json!({"kind":"resistor","position":{"x":20.0,"y":20.0},"reference":"R2"}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let out = s.execute("object.snap", json!({"ids":[r1,r2],"grid":2.5}), None)?;
        assert_eq!(out["result"]["moved"], 1);
        assert_eq!(s.project.components[0].position, Point::new(12.5, 12.5));
        assert_eq!(s.project.components[1].position, Point::new(20.0, 20.0));
        s.execute("board.sync", json!({}), None)?;
        let fp = s
            .project
            .board
            .footprints
            .iter()
            .find(|f| f.component == r1)
            .cloned()
            .ok_or_else(missing)?;
        s.execute(
            "footprint.move",
            json!({"id":fp.id,"position":{"x":23.3,"y":17.9}}),
            None,
        )?;
        let out = s.execute("object.snap", json!({"ids":[fp.id],"grid":2.5}), None)?;
        assert_eq!(out["result"]["moved"], 1);
        assert_eq!(
            s.project.footprint(fp.id).ok_or_else(missing)?.position,
            Point::new(22.5, 17.5)
        );
        for params in [
            json!({"ids":[r1],"grid":0.01}),
            json!({"ids":[r1],"grid":30.0}),
            json!({"ids":[9999],"grid":2.5}),
            json!({"ids":[],"grid":2.5}),
            json!({"ids":[r1],"grid":2.5,"extra":1}),
        ] {
            assert!(s.execute("object.snap", params, None).is_err());
        }
        assert_eq!(s.project.components[0].position, Point::new(12.5, 12.5));
        s.undo()?;
        assert_eq!(
            s.project.footprint(fp.id).ok_or_else(missing)?.position,
            Point::new(23.3, 17.9),
            "undo returns only the last snap"
        );
        assert_eq!(s.project.components[0].position, Point::new(12.5, 12.5));
        Ok(())
    }
    #[test]
    fn moving_graphics_and_labels_keeps_rules_and_undoes_cleanly() -> Result<(), Error> {
        let mut s = Session::default();
        let net = s.execute("net.create", json!({"name":"SIG"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let line = s.execute(
            "graphic.add",
            json!({"layer":"board","shape":"line","points":[{"x":10.0,"y":10.0},{"x":20.0,"y":15.0}],"width":0.2}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        s.execute(
            "graphic.move",
            json!({"id":line,"delta":{"x":5.0,"y":-5.0}}),
            None,
        )?;
        assert_eq!(
            s.project.graphics[0].points,
            vec![Point::new(15.0, 5.0), Point::new(25.0, 10.0)]
        );
        let label = s.execute(
            "label.place",
            json!({"kind":"global","net":net,"position":{"x":30.0,"y":20.0}}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        s.execute(
            "label.move",
            json!({"id":label,"position":{"x":40.0,"y":25.0}}),
            None,
        )?;
        assert_eq!(s.project.labels[0].position, Point::new(40.0, 25.0));
        s.execute(
            "wire.draw",
            json!({"points":[{"x":10.0,"y":10.0},{"x":20.0,"y":10.0}],"net":net}),
            None,
        )?;
        let junction = s.execute(
            "label.place",
            json!({"kind":"junction","net":net,"position":{"x":10.0,"y":10.0}}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        s.execute(
            "label.move",
            json!({"id":junction,"position":{"x":20.0,"y":10.0}}),
            None,
        )?;
        assert_eq!(s.project.labels[1].position, Point::new(20.0, 10.0));
        for params in [
            json!({"id":junction,"position":{"x":15.0,"y":10.0}}),
            json!({"id":line,"delta":{"x":1e6,"y":0.0}}),
            json!({"id":9999,"delta":{"x":1.0,"y":1.0}}),
            json!({"id":9999,"position":{"x":1.0,"y":1.0}}),
            json!({"id":line,"delta":{"x":1.0,"y":1.0},"extra":1}),
        ] {
            let command = if params.get("delta").is_some() {
                "graphic.move"
            } else {
                "label.move"
            };
            assert!(s.execute(command, params, None).is_err());
        }
        assert_eq!(s.project.graphics[0].points[0], Point::new(15.0, 5.0));
        assert_eq!(s.project.labels[1].position, Point::new(20.0, 10.0));
        assert!(s.project.validate().is_ok());
        s.undo()?;
        assert_eq!(s.project.labels[1].position, Point::new(10.0, 10.0));
        s.undo()?;
        assert_eq!(s.project.labels.len(), 1);
        assert_eq!(s.project.labels[0].position, Point::new(40.0, 25.0));
        s.undo()?;
        assert!(s.project.drawn_wires.is_empty());
        s.undo()?;
        assert_eq!(s.project.labels[0].position, Point::new(30.0, 20.0));
        Ok(())
    }
    #[test]
    fn moving_tracks_and_vias_updates_clearance_checks() -> Result<(), Error> {
        let mut s = Session::default();
        let a = s.execute("net.create", json!({"name":"A"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let b = s.execute("net.create", json!({"name":"B"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let t1 = s.execute(
            "track.add",
            json!({"net":a,"start":{"x":10.0,"y":10.0},"end":{"x":30.0,"y":10.0},"width":0.2}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let t2 = s.execute(
            "track.add",
            json!({"net":b,"start":{"x":10.0,"y":10.3},"end":{"x":30.0,"y":10.3},"width":0.2,"layer":"F.Cu"}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        assert!(
            !s.checks()["drc"].as_array().ok_or_else(missing)?.is_empty(),
            "parallel tracks closer than the clearance must be reported"
        );
        s.execute(
            "track.move",
            json!({"id":t2,"delta":{"x":0.0,"y":0.5}}),
            None,
        )?;
        assert_eq!(s.project.board.tracks[1].start, Point::new(10.0, 10.8));
        assert_eq!(s.project.board.tracks[1].end, Point::new(30.0, 10.8));
        assert!(
            s.checks()["drc"].as_array().ok_or_else(missing)?.is_empty(),
            "moving apart must clear the finding"
        );
        let via = s.execute(
            "via.place",
            json!({"net":a,"position":{"x":20.0,"y":20.0},"diameter":1.4,"drill":0.4}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        s.execute(
            "via.move",
            json!({"id":via,"position":{"x":25.0,"y":30.0}}),
            None,
        )?;
        assert_eq!(s.project.board.vias[0].position, Point::new(25.0, 30.0));
        assert_eq!(s.project.board.vias[0].diameter, 1.4);
        for params in [
            json!({"id":t1,"delta":{"x":1e6,"y":0.0}}),
            json!({"id":9999,"delta":{"x":1.0,"y":1.0}}),
            json!({"id":via,"position":{"x":1e6,"y":0.0}}),
            json!({"id":9999,"position":{"x":1.0,"y":1.0}}),
            json!({"id":t1,"delta":{"x":1.0,"y":1.0},"extra":1}),
        ] {
            assert!(s.execute("track.move", params.clone(), None).is_err());
            assert!(s.execute("via.move", params, None).is_err());
        }
        assert_eq!(s.project.board.tracks[1].start, Point::new(10.0, 10.8));
        assert_eq!(s.project.board.vias[0].position, Point::new(25.0, 30.0));
        s.undo()?;
        assert_eq!(s.project.board.vias[0].position, Point::new(20.0, 20.0));
        s.undo()?;
        assert!(s.project.board.vias.is_empty());
        s.undo()?;
        assert_eq!(s.project.board.tracks[1].start, Point::new(10.0, 10.3));
        assert!(
            !s.checks()["drc"].as_array().ok_or_else(missing)?.is_empty(),
            "undo restores the finding"
        );
        Ok(())
    }
    #[test]
    fn zone_place_validates_and_undoes_cleanly() -> Result<(), Error> {
        let mut s = Session::default();
        let net = s.execute("net.create", json!({"name":"GND"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let square = json!([
            {"x":5.0,"y":5.0},
            {"x":55.0,"y":5.0},
            {"x":55.0,"y":35.0},
            {"x":5.0,"y":35.0},
            {"x":5.0,"y":5.0}
        ]);
        s.execute(
            "zone.place",
            json!({"net":net,"layer":"F.Cu","outline":square}),
            None,
        )?;
        s.execute("zone.place", json!({"net":net,"outline":square}), None)?;
        assert_eq!(s.project.board.zones.len(), 2);
        assert_eq!(s.project.board.zones[0].layer, pcbcraft_doc::Layer::Front);
        assert!(s.project.validate().is_ok());
        for params in [
            json!({"net":9999,"outline":square}),
            json!({"net":net,"outline":[{"x":0.0,"y":0.0},{"x":10.0,"y":0.0},{"x":0.0,"y":0.0}]}),
            json!({"net":net,"outline":[{"x":0.0,"y":0.0},{"x":10.0,"y":0.0},{"x":10.0,"y":5.0},{"x":1.0,"y":5.0}]}),
            json!({"net":net,"outline":[{"x":0.0,"y":0.0},{"x":10.0,"y":0.0},{"x":10.0,"y":5.0},{"x":0.0,"y":5.0}]}),
            json!({"net":net,"outline":[{"x":0.0,"y":0.0},{"x":0.0,"y":0.0},{"x":0.0,"y":0.0},{"x":0.0,"y":0.0}]}),
            json!({"net":net,"outline":[{"x":100001.0,"y":0.0},{"x":10.0,"y":0.0},{"x":10.0,"y":5.0},{"x":0.0,"y":5.0},{"x":0.0,"y":0.0}]}),
        ] {
            assert!(s.execute("zone.place", params, None).is_err());
        }
        assert_eq!(s.project.board.zones.len(), 2);
        let zone = s.project.board.zones[0].id;
        s.execute("object.delete", json!({"id":zone}), None)?;
        assert_eq!(s.project.board.zones.len(), 1);
        s.execute("object.delete", json!({"id":net}), None)?;
        assert!(s.project.board.zones.is_empty());
        s.undo()?;
        assert_eq!(s.project.board.zones.len(), 1);
        Ok(())
    }
    #[test]
    fn zone_cutout_validates_and_undoes_cleanly() -> Result<(), Error> {
        let mut s = Session::default();
        let net = s.execute("net.create", json!({"name":"GND"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let square = json!([
            {"x":5.0,"y":5.0},
            {"x":55.0,"y":5.0},
            {"x":55.0,"y":35.0},
            {"x":5.0,"y":35.0},
            {"x":5.0,"y":5.0}
        ]);
        s.execute("zone.place", json!({"net":net,"outline":square}), None)?;
        let zone = s.project.board.zones[0].id;
        let cut = json!([
            {"x":20.0,"y":15.0},
            {"x":30.0,"y":15.0},
            {"x":30.0,"y":25.0},
            {"x":20.0,"y":25.0},
            {"x":20.0,"y":15.0}
        ]);
        let out = s.execute("zone.cutout", json!({"zone":zone,"outline":cut}), None)?;
        assert_eq!(out["result"]["cutouts"], 1);
        assert!(s.project.validate().is_ok());
        for params in [
            json!({"zone":9999,"outline":cut}),
            json!({"zone":zone,"outline":[{"x":0.0,"y":0.0},{"x":10.0,"y":0.0},{"x":10.0,"y":5.0}]}),
            json!({"zone":zone,"outline":[{"x":0.0,"y":0.0},{"x":10.0,"y":0.0},{"x":10.0,"y":5.0},{"x":1.0,"y":5.0}]}),
            json!({"zone":zone,"outline":[{"x":0.0,"y":0.0},{"x":0.0,"y":0.0},{"x":0.0,"y":0.0},{"x":0.0,"y":0.0}]}),
        ] {
            assert!(s.execute("zone.cutout", params, None).is_err());
        }
        assert_eq!(s.project.board.zones[0].cutouts.len(), 1);
        s.undo()?;
        assert!(s.project.board.zones[0].cutouts.is_empty());
        Ok(())
    }
    #[test]
    fn imported_definitions_survive_roundtrip_and_assignment_is_undoable() -> Result<(), Error> {
        let mut s = Session::default();
        s.execute("component.place_library",json!({"path":library_fixture("device.kicad_sym"),"symbol":"R","position":{"x":20,"y":15}}),None)?;
        s.execute("board.sync", json!({}), None)?;
        assert!(
            s.project.board.footprints.is_empty(),
            "unassigned symbols must not invent a physical package"
        );
        s.execute(
            "footprint.assign",
            json!({"id":1,"path":library_fixture("R_0805_2012Metric.kicad_mod")}),
            None,
        )?;
        assert!(
            s.project
                .component(1)
                .is_some_and(|c| c.library_footprint.is_some())
        );
        s.undo()?;
        assert!(
            s.project
                .component(1)
                .is_some_and(|c| c.library_footprint.is_none())
        );
        s.redo()?;
        s.execute("board.sync", json!({}), None)?;
        let f = s.project.board.footprints.first().ok_or_else(missing)?;
        assert_eq!(f.pads.len(), 2);
        assert!(
            f.pads
                .iter()
                .all(|p| p.geometry.as_ref().is_some_and(|g| g.shape == "roundrect"))
        );
        assert!(
            f.pads
                .first()
                .is_some_and(|p| (p.offset.x + 0.9125).abs() < 1e-6)
        );
        let text = serde_json::to_string(&s.project).map_err(|e| Error::Invalid(e.to_string()))?;
        let loaded: Project =
            serde_json::from_str(&text).map_err(|e| Error::Invalid(e.to_string()))?;
        loaded.validate()?;
        assert_eq!(loaded, s.project);
        let mut tampered = loaded;
        if let Some(pad) = tampered
            .board
            .footprints
            .first_mut()
            .and_then(|f| f.pads.first_mut())
        {
            pad.geometry = None;
        }
        assert!(
            tampered.validate().is_err(),
            "embedded physical geometry must not silently become generic"
        );
        Ok(())
    }
    #[test]
    fn multiunit_package_has_one_footprint_and_independent_unit_history() -> Result<(), Error> {
        let mut s = Session::default();
        s.execute("component.place_library",json!({"path":library_fixture("lm358.kicad_sym"),"symbol":"LM358","position":{"x":20,"y":15}}),None)?;
        let c = s.project.component(1).ok_or_else(missing)?;
        assert_eq!(c.pins.len(), 8);
        assert_eq!(c.units.len(), 2);
        assert_eq!(
            c.schematic_views()
                .iter()
                .map(|c| c.reference.as_str())
                .collect::<Vec<_>>(),
            vec!["U1A", "U1B", "U1C"]
        );
        let first = c.pin_position("1").ok_or_else(missing)?;
        let second = c.pin_position("5").ok_or_else(missing)?;
        s.execute(
            "component.move",
            json!({"id":1,"unit":2,"position":{"x":50,"y":28}}),
            None,
        )?;
        s.execute(
            "component.rotate",
            json!({"id":1,"unit":2,"degrees":90}),
            None,
        )?;
        let c = s.project.component(1).ok_or_else(missing)?;
        assert_eq!(c.pin_position("1"), Some(first));
        assert_ne!(c.pin_position("5"), Some(second));
        s.undo()?;
        s.undo()?;
        assert_eq!(
            s.project.component(1).and_then(|c| c.pin_position("5")),
            Some(second)
        );
        s.redo()?;
        s.redo()?;
        let before = s.inspect();
        assert!(
            s.execute(
                "footprint.assign",
                json!({"id":1,"path":library_fixture("R_0805_2012Metric.kicad_mod")}),
                None
            )
            .is_err()
        );
        assert_eq!(s.inspect(), before);
        s.execute(
            "footprint.assign",
            json!({"id":1,"path":library_fixture("DIP-8_W7.62mm.kicad_mod")}),
            None,
        )?;
        s.execute("board.sync", json!({}), None)?;
        assert_eq!(s.project.board.footprints.len(), 1);
        assert_eq!(
            s.project.board.footprints.first().map(|f| f.pads.len()),
            Some(8)
        );
        let text = serde_json::to_string(&s.project).map_err(|e| Error::Invalid(e.to_string()))?;
        let loaded: Project =
            serde_json::from_str(&text).map_err(|e| Error::Invalid(e.to_string()))?;
        loaded.validate()?;
        assert_eq!(loaded, s.project);
        // A project with a corrupted pin map is rejected at the document boundary.
        let mut corrupted = s.project.clone();
        if let Some(pin) = corrupted
            .components
            .first_mut()
            .and_then(|c| c.pins.first_mut())
        {
            pin.number = "99".into();
        }
        assert!(s.replace(corrupted).is_err());
        let before = s.inspect();
        let batch = [
            Command {
                command: "project.rename".into(),
                params: json!({"name":"uncommitted"}),
            },
            Command {
                command: "footprint.assign".into(),
                params: json!({"id":999,"path":library_fixture("R_0805_2012Metric.kicad_mod")}),
            },
        ];
        assert!(s.batch(&batch, None).is_err());
        assert_eq!(s.inspect(), before);
        Ok(())
    }
    #[test]
    fn editable_library_entries_are_atomic_and_placed_copies_are_stable() -> Result<(), Error> {
        let mut s = Session::default();
        let mut symbol = library::symbol(
            std::path::Path::new(&library_fixture("device.kicad_sym")),
            "R",
        )?;
        symbol.name = "CustomR".into();
        let footprint = library::footprint(std::path::Path::new(&library_fixture(
            "R_0805_2012Metric.kicad_mod",
        )))?;
        s.batch(&[Command {command:"library.symbol.upsert".into(),params:json!({"definition":symbol,"notice":"KiCad-derived fixture; license retained"})},Command {command:"library.footprint.upsert".into(),params:json!({"definition":footprint})}],None)?;
        assert_eq!(s.project.library.symbols.len(), 1);
        assert_eq!(s.project.library.notices.len(), 1);
        s.undo()?;
        assert!(s.project.library.symbols.is_empty());
        s.redo()?;
        s.execute(
            "component.place_custom",
            json!({"name":"CustomR","position":{"x":20,"y":10}}),
            None,
        )?;
        s.execute(
            "footprint.assign_custom",
            json!({"id":1,"name":"R_0805_2012Metric"}),
            None,
        )?;
        s.execute("board.sync", json!({}), None)?;
        let before = s.project.components.clone();
        symbol.pins.first_mut().ok_or_else(missing)?.position.x += 1.0;
        s.execute("library.symbol.upsert", json!({"definition":symbol}), None)?;
        assert_eq!(
            s.project.components, before,
            "editing library does not silently change placed circuits"
        );
        let saved = s.inspect();
        let mut invalid = footprint.clone();
        invalid.pads.first_mut().ok_or_else(missing)?.size.x = -1.0;
        assert!(
            s.batch(
                &[
                    Command {
                        command: "library.symbol.remove".into(),
                        params: json!({"name":"CustomR"})
                    },
                    Command {
                        command: "library.footprint.upsert".into(),
                        params: json!({"definition":invalid})
                    }
                ],
                None
            )
            .is_err()
        );
        assert_eq!(s.inspect(), saved);
        s.execute("library.symbol.remove", json!({"name":"CustomR"}), None)?;
        assert_eq!(s.project.components, before);
        s.project.validate()?;
        Ok(())
    }
    #[test]
    fn auto_net_and_wire_commit_as_one_undo_step() -> Result<(), Error> {
        let mut s = Session::default();
        for x in [10, 30] {
            s.execute(
                "component.add",
                json!({"kind":"resistor","position":{"x":x,"y":10}}),
                None,
            )?;
        }
        s.execute(
            "wire.connect",
            json!({"from":{"component":1,"pin":"2"},"to":{"component":2,"pin":"1"}}),
            None,
        )?;
        assert_eq!(s.project.nets.len(), 1);
        assert_eq!(s.project.wires.len(), 1);
        s.undo()?;
        assert!(s.project.nets.is_empty());
        assert!(s.project.wires.is_empty());
        assert_eq!(s.project.components.len(), 2);
        Ok(())
    }
    #[test]
    fn sample_wire_paths_never_run_through_foreign_pins() -> Result<(), Error> {
        let s = Session::sample()?;
        for w in &s.project.wires {
            let path = s.project.wire_path(w).ok_or_else(missing)?;
            for c in &s.project.components {
                for pin in &c.pins {
                    if pin.net == Some(w.net) {
                        continue;
                    }
                    let point = c.pin_position(&pin.number).ok_or_else(missing)?;
                    for segment in path.windows(2) {
                        if let Some((a, b)) = segment.first().zip(segment.get(1)) {
                            assert!(
                                pcbcraft_geom::point_segment(point, *a, *b) > 0.5,
                                "distinct nets must not overlap visually in sample"
                            );
                        }
                    }
                }
            }
        }
        Ok(())
    }
    #[test]
    fn counterclockwise_pad_rotation_matches_y_down_board_coordinates() -> Result<(), Error> {
        let mut s = Session::sample()?;
        s.execute("footprint.rotate", json!({"id":11,"degrees":90}), None)?;
        let f = s.project.footprint(11).ok_or_else(missing)?;
        let pin = f.pad_position("1").ok_or_else(missing)?;
        assert!((pin.x - 28.0).abs() < 1e-6);
        assert!((pin.y - 14.5).abs() < 1e-6);
        Ok(())
    }
    #[test]
    fn failed_batch_is_atomic_and_stale_edits_reject() {
        let mut s = Session::default();
        let before = s.inspect();
        let batch = [
            Command {
                command: "net.create".into(),
                params: json!({"name":"GND"}),
            },
            Command {
                command: "component.move".into(),
                params: json!({"id":999,"position":{"x":1,"y":2}}),
            },
        ];
        assert!(s.batch(&batch, None).is_err());
        assert_eq!(s.inspect(), before);
        assert!(
            s.execute("net.create", json!({"name":"GND"}), Some(100))
                .is_err()
        );
        assert_eq!(s.inspect(), before);
    }
    #[test]
    fn undo_branch_never_reuses_ids() {
        let mut s = Session::default();
        assert!(s.execute("net.create", json!({"name":"A"}), None).is_ok());
        assert!(s.undo().is_ok());
        assert!(s.execute("net.create", json!({"name":"B"}), None).is_ok());
        assert_eq!(s.project.nets.first().map(|n| n.id), Some(2));
        assert!(s.redo().is_err());
    }
    #[test]
    fn sample_is_valid_and_pcb_sync_retains_placements() {
        let result = Session::sample();
        assert!(result.is_ok());
        if let Ok(mut s) = result {
            assert!(s.project.validate().is_ok());
            let before = s.project.board.footprints.clone();
            assert!(s.execute("board.sync", json!({}), None).is_ok());
            assert_eq!(before, s.project.board.footprints);
            assert!(pcbcraft_rules::erc(&s.project).is_empty());
        }
    }
    #[test]
    fn hostile_inputs_leave_state_untouched() {
        let mut s = Session::default();
        let before = s.inspect();
        for params in [
            json!({"kind":"resistor","position":{"x":1e200,"y":0}}),
            json!({"kind":"resistor","position":{"x":0,"y":0},"unexpected":true}),
            json!({"kind":"bogus","position":{"x":0,"y":0}}),
        ] {
            assert!(s.execute("component.add", params, None).is_err());
            assert_eq!(s.inspect(), before);
        }
    }
}
