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
            "footprint.assign_pattern",
            "Assign one footprint file to every component whose reference matches a * or ? pattern; components whose pins do not match the pads are skipped and reported",
            schema(
                json!({"pattern":{"type":"string","description":"reference wildcard, case sensitive"},"path":{"type":"string"}}),
                &["pattern", "path"],
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
            "component.mirror",
            "Mirror a placed symbol left-to-right or top-to-bottom about its own origin",
            schema(
                json!({"id":id_schema(),"axis":{"type":"string","enum":["horizontal","vertical"]}}),
                &["id", "axis"],
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
            "component.reference",
            "Rename a component's reference designator; the new name must be unique",
            schema(
                json!({"id":id_schema(),"reference":{"type":"string"}}),
                &["id", "reference"],
            ),
        ),
        (
            "component.field",
            "Set one named component property (datasheet, manufacturer, custom note), or clear it when value is null or omitted",
            schema(
                json!({"id":id_schema(),"key":{"type":"string"},"value":{"type":["string","null"]}}),
                &["id", "key"],
            ),
        ),
        (
            "component.find",
            "Find components by reference wildcard, exact value or kind; * matches any run of characters and ? matches one",
            schema(
                json!({"reference":{"type":"string","description":"case-sensitive reference pattern, must contain * or ?","maxLength":64},"value":{"type":"string","maxLength":256},"kind":{"type":"string","maxLength":64}}),
                &[],
            ),
        ),
        (
            "net.create",
            "Create a named logical net",
            schema(json!({"name":{"type":"string"}}), &["name"]),
        ),
        (
            "net.rename",
            "Rename an existing net; the new name must be unique",
            schema(
                json!({"id":id_schema(),"name":{"type":"string"}}),
                &["id", "name"],
            ),
        ),
        (
            "net.set_length",
            "Set a target routed length and tolerance for one net, or clear the target by omitting it; nets in a class follow their own target when they have one",
            schema(
                json!({"id":id_schema(),"target":{"type":["number","null"],"exclusiveMinimum":0,"maximum":100000},"tolerance":{"type":["number","null"],"minimum":0,"maximum":100000}}),
                &["id"],
            ),
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
            "net.disconnect",
            "Take component pins out of a net, clearing their placed pads as well",
            schema(
                json!({"net":id_schema(),"pins":{"type":"array","items":pin_schema(),"minItems":1,"maxItems":128}}),
                &["net", "pins"],
            ),
        ),
        (
            "copper.set_net",
            "Move an existing track, via or copper zone onto a different net",
            schema(json!({"id":id_schema(),"net":id_schema()}), &["id", "net"]),
        ),
        (
            "netclass.add",
            "Create a net class with a clearance requirement, a default track width and optional member nets",
            schema(
                json!({"name":{"type":"string"},"clearance":{"type":"number","exclusiveMinimum":0},"width":{"type":"number","exclusiveMinimum":0},"nets":{"type":"array","items":id_schema(),"maxItems":256}}),
                &["name", "clearance", "width"],
            ),
        ),
        (
            "netclass.assign",
            "Move a net into a net class, or out of every class when the class is null",
            schema(
                json!({"net":id_schema(),"class":{"oneOf":[id_schema(),{"type":"null"}]}}),
                &["net"],
            ),
        ),
        (
            "netclass.width_rules",
            "Set the smallest and largest track width a net class allows; omitted bounds stay as they were",
            schema(
                json!({"class":id_schema(),"min_width":{"type":["number","null"],"exclusiveMinimum":0,"maximum":100},"max_width":{"type":["number","null"],"exclusiveMinimum":0,"maximum":100}}),
                &["class"],
            ),
        ),
        (
            "netclass.length_rules",
            "Set the target routed length and tolerance a net class demands; omitted values stay as they were",
            schema(
                json!({"class":id_schema(),"target_length":{"type":["number","null"],"exclusiveMinimum":0,"maximum":10000},"length_tolerance":{"type":["number","null"],"minimum":0,"maximum":1000}}),
                &["class"],
            ),
        ),
        (
            "netclass.clearance",
            "Change the clearance a net class requires between its copper and everything else",
            schema(
                json!({"class":id_schema(),"clearance":{"type":"number","exclusiveMinimum":0,"maximum":100,"description":"minimum copper separation in millimetres"}}),
                &["class", "clearance"],
            ),
        ),
        (
            "netclass.remove",
            "Delete a net class and return its nets to the board default rules",
            schema(json!({"class":id_schema()}), &["class"]),
        ),
        (
            "netclass.assign_pattern",
            "Move every net whose name matches a wildcard pattern into a net class in one step; * matches any run of characters and ? matches one",
            schema(
                json!({"class":id_schema(),"pattern":{"type":"string","description":"case-sensitive name pattern, must contain * or ?","maxLength":64}}),
                &["class", "pattern"],
            ),
        ),
        (
            "netlist.import",
            "Import a netlist table: assign listed pins to named nets by reference, creating missing nets",
            schema(
                json!({"nets":{"type":"array","minItems":1,"maxItems":256,"items":{"type":"object","properties":{"name":{"type":"string"},"pins":{"type":"array","minItems":1,"maxItems":128,"items":{"type":"object","properties":{"reference":{"type":"string"},"pin":{"type":"string"}},"required":["reference","pin"],"additionalProperties":false}}},"required":["name","pins"],"additionalProperties":false}}}),
                &["nets"],
            ),
        ),
        (
            "netlist.export",
            "Export the connectivity table: every net with its pins by reference, plus unconnected pins",
            schema(json!({"net":id_schema()}), &[]),
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
            "wire.add_point",
            "Insert a vertex into a drawn wire at any position along it, extending either end",
            schema(
                json!({"id":id_schema(),"index":{"type":"integer","minimum":0,"maximum":256,"description":"vertex position the new point takes"},"position":point_schema()}),
                &["id", "index", "position"],
            ),
        ),
        (
            "wire.remove_point",
            "Remove one vertex from a drawn wire, shortening it or straightening a corner",
            schema(
                json!({"id":id_schema(),"index":{"type":"integer","minimum":0,"maximum":255}}),
                &["id", "index"],
            ),
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
            "graphic.rotate",
            "Rotate an existing schematic or board graphic in 90-degree steps about its centre or a given pivot",
            schema(
                json!({"id":id_schema(),"degrees":{"type":"number"},"around":point_schema()}),
                &["id", "degrees"],
            ),
        ),
        (
            "graphic.set",
            "Change an existing graphic's layer, stroke width or radius, or its text; omitted fields stay as they were",
            schema(
                json!({"id":id_schema(),"layer":{"type":"string","enum":["schematic","board"]},"width":{"type":"number","exclusiveMinimum":0,"maximum":1000},"text":{"type":"string","minLength":1,"maxLength":1024}}),
                &["id"],
            ),
        ),
        (
            "graphic.set_point",
            "Move one vertex of an existing graphic to a new position",
            schema(
                json!({"id":id_schema(),"index":{"type":"integer","minimum":0},"position":point_schema()}),
                &["id", "index", "position"],
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
            "label.rotate",
            "Rotate an existing label, global label or power label in degrees",
            schema(
                json!({"id":id_schema(),"degrees":{"type":"number"}}),
                &["id", "degrees"],
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
            "zone.move",
            "Translate a poured zone by a delta; outline and every cutout move together",
            schema(
                json!({"id":id_schema(),"delta":point_schema()}),
                &["id", "delta"],
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
            "board.cutout.move",
            "Move an existing board cutout to a new centre, keeping it inside the board",
            schema(
                json!({"id":id_schema(),"center":point_schema()}),
                &["id", "center"],
            ),
        ),
        (
            "board.cutout.resize",
            "Change the radius of an existing board cutout, keeping it inside the board",
            schema(
                json!({"id":id_schema(),"radius":{"type":"number","exclusiveMinimum":0,"maximum":1000}}),
                &["id", "radius"],
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
            "track.chamfer",
            "Cut the corner between two joined same-net track segments with a diagonal segment",
            schema(
                json!({"a":id_schema(),"b":id_schema(),"size":{"type":"number","description":"distance cut back from the corner along each segment, millimetres"}}),
                &["a", "b", "size"],
            ),
        ),
        (
            "track.set_width",
            "Change the width of an existing track segment",
            schema(
                json!({"id":id_schema(),"width":{"type":"number","exclusiveMinimum":0,"maximum":100,"description":"copper width in millimetres"}}),
                &["id", "width"],
            ),
        ),
        (
            "via.set_size",
            "Change the pad and drill diameters of an existing via",
            schema(
                json!({"id":id_schema(),"diameter":{"type":"number","exclusiveMinimum":0,"maximum":100,"description":"outer copper pad diameter in millimetres"},"drill":{"type":"number","minimum":0,"description":"drilled hole diameter in millimetres; keeps the current drill when omitted"}}),
                &["id", "diameter"],
            ),
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
            "footprint.mirror",
            "Mirror a board footprint left-to-right or top-to-bottom about its own origin",
            schema(
                json!({"id":id_schema(),"axis":{"type":"string","enum":["horizontal","vertical"]}}),
                &["id", "axis"],
            ),
        ),
        (
            "pad.table",
            "Report every board pad: number, net, position, shape, size and drill, by component reference",
            schema(json!({"component":id_schema()}), &[]),
        ),
        (
            "net.lengths",
            "Report routed copper length per net: track segments summed in millimetres, with via counts and unrouted nets listed",
            schema(json!({"net":id_schema()}), &[]),
        ),
        (
            "rules.severity",
            "Change how one check code reports: error, warning, or off to silence it; null or omitting severity restores its default",
            schema(
                json!({"code":{"type":"string","description":"check code: unassigned_pin, single_pin_net, clearance, copper_short, pad_clearance_bound, board_edge, cutout_clearance, track_width, track_length, unrouted, missing_footprint"},"severity":{"type":["string","null"],"enum":["error","warning","off",null]}}),
                &["code"],
            ),
        ),
        (
            "rules.exclude",
            "Silence one check code for particular design objects; a finding drops when every object it names is excluded",
            schema(
                json!({"code":{"type":"string","description":"check code, as listed by rules.severity"},"ids":{"type":"array","items":id_schema(),"minItems":1,"maxItems":256,"description":"component, net, track, via, zone, cutout, wire or footprint ids to exempt"}}),
                &["code", "ids"],
            ),
        ),
        (
            "rules.include",
            "Lift object exclusions for one check code; omit ids to lift every exclusion of that code",
            schema(
                json!({"code":{"type":"string"},"ids":{"type":"array","items":id_schema(),"maxItems":256}}),
                &["code"],
            ),
        ),
        (
            "bom.export",
            "Export a bill of materials: components grouped by value, kind, footprint and requested properties",
            schema(
                json!({"fields":{"type":"array","items":{"type":"string"},"maxItems":16}}),
                &[],
            ),
        ),
        (
            "align.distribute",
            "Align or evenly distribute components or board footprints along an axis, named by id or selected by reference/value/kind filter",
            schema(
                json!({"ids":{"type":"array","maxItems":128,"items":id_schema()},"filter":schema(json!({"reference":{"type":"string","description":"case-sensitive reference pattern, must contain * or ?","maxLength":64},"value":{"type":"string","maxLength":256},"kind":{"type":"string","maxLength":64}}),&[]),"mode":{"type":"string","enum":["left","center_x","right","top","center_y","bottom","distribute_x","distribute_y"]}}),
                &["mode"],
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
        (
            "text.place",
            "Place a text label on the board with a size in millimetres and a layer",
            schema(
                json!({"text":{"type":"string","minLength":1,"maxLength":128},"position":point_schema(),"size":{"type":"number","minimum":0.1,"maximum":50,"description":"glyph height in millimetres, default 1.0"},"layer":{"type":"string","enum":["F.Cu","B.Cu"]}}),
                &["text", "position"],
            ),
        ),
        (
            "text.move",
            "Move a board text label to a new position",
            schema(
                json!({"id":id_schema(),"position":point_schema()}),
                &["id", "position"],
            ),
        ),
        (
            "text.set",
            "Change a board text label's string, size, or layer",
            schema(
                json!({"id":id_schema(),"text":{"type":"string","minLength":1,"maxLength":128},"size":{"type":"number","minimum":0.1,"maximum":50},"layer":{"type":"string","enum":["F.Cu","B.Cu"]}}),
                &["id"],
            ),
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
        json!({"revision":self.revision,"project":self.project,"history":{"undo":self.undo.len(),"redo":self.redo.len()},"capabilities":{"schematic":"flat explicit pin nets with named net labels, junctions and no-connect flags, and pins that can be released from a net again, with drawn graphics whose layer, stroke, text and individual vertices stay editable; embedded multi-unit packages with independent unit placement and one physical pin map","pcb":"front SMD and plated through-hole circle/rect/roundrect/oval pads, circular/oval offset drills; two copper layers, simple tracks and plated through-hole vias including stitch grids, with copper sizes editable after placement and placed copper switchable between nets, poured copper zones with cutouts, rectangular outline and circular cutouts that stay editable after placement, free text labels on either copper side, named net classes carrying per-class clearance and default track width, retunable or deleted in place; bills of materials grouped by value and footprint with named component properties; classes filled by name pattern with * and ? wildcards; classes can demand target routed lengths within a tolerance, and a single net can carry its own target that takes priority over its class","library":"native project-library symbol/footprint authoring, reusable .pcbcraft-lib collections, embedded placed copies","checks":"initial pin assignment with no-connect flag suppression and conservative copper bounds honouring net class clearances, with same-pad plated layer bridges and drilled-hole contact tests, plus routed copper length totals per net and per-check-code severity overrides, with per-object exclusions that silence accepted findings; not comprehensive ERC/DRC","kicad":"native library reading and limited PCB export; no general project import or library writer","routing":"orthogonal routes on chosen copper layer; no obstacle avoidance or shove","3d":false}})
    }
    pub fn checks(&self) -> Value {
        json!({"revision":self.revision,"erc":pcbcraft_rules::erc(&self.project),"drc":pcbcraft_rules::drc(&self.project),"airwires":pcbcraft_rules::airwires(&self.project),"scope":"Initial checks: pin assignment (suppressed for pins flagged no-connect), net population, capsule tracks, rectangular edge and circular cutout clearance. Copper clearance uses the strictest of the board default and the clearance of the net classes on either side. Tracks on classified nets are checked against the class minimum and maximum width when set. Noncircular pads use sampled convex outlines with conservative chord-error clearance and inscribed connectivity bounds. Plated pads bridge the same physical pad between F.Cu/B.Cu; traces wholly inside circular/oval drills do not contact copper. Conservative false findings remain possible. Each check code can be re-levelled to warning or silenced entirely through project severity overrides. No comprehensive pin-type, annular-ring manufacturing rules, zones or hierarchy checks."})
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
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AssignPatternParams {
    pattern: String,
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
struct CopperNetParams {
    id: Id,
    net: Id,
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
struct GraphicSetParams {
    id: Id,
    #[serde(default)]
    layer: Option<String>,
    #[serde(default)]
    width: Option<f64>,
    #[serde(default)]
    text: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GraphicPointParams {
    id: Id,
    index: usize,
    position: Point,
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
struct NetClassParams {
    name: String,
    clearance: f64,
    width: f64,
    #[serde(default)]
    nets: Vec<Id>,
}
/// Case-sensitive net-name match: `*` matches any run of characters,
/// `?` matches exactly one. Iterative so long patterns cannot recurse away.
fn class_pattern_matches(pattern: &str, name: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let name: Vec<char> = name.chars().collect();
    let mut p = 0usize;
    let mut n = 0usize;
    let mut star: Option<usize> = None;
    let mut star_name = 0usize;
    while n < name.len() {
        if p < pattern.len() && (pattern[p] == '?' || pattern[p] == name[n]) {
            p += 1;
            n += 1;
        } else if p < pattern.len() && pattern[p] == '*' {
            star = Some(p);
            star_name = n;
            p += 1;
        } else if let Some(sp) = star {
            p = sp + 1;
            star_name += 1;
            n = star_name;
        } else {
            return false;
        }
    }
    while p < pattern.len() && pattern[p] == '*' {
        p += 1;
    }
    p == pattern.len()
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WidthRulesParams {
    class: Id,
    #[serde(default)]
    min_width: Option<f64>,
    #[serde(default)]
    max_width: Option<f64>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LengthRulesParams {
    class: Id,
    #[serde(default)]
    target_length: Option<f64>,
    #[serde(default)]
    length_tolerance: Option<f64>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ClassClearanceParams {
    class: Id,
    clearance: f64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ClassRemoveParams {
    class: Id,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NetLengthParams {
    id: Id,
    #[serde(default)]
    target: Option<f64>,
    #[serde(default)]
    tolerance: Option<f64>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NetClassPatternParams {
    class: Id,
    pattern: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NetClassAssignParams {
    net: Id,
    #[serde(default)]
    class: Option<Id>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BomParams {
    #[serde(default)]
    fields: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RuleSeverityParams {
    code: String,
    #[serde(default)]
    severity: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RuleExcludeParams {
    code: String,
    ids: Vec<Id>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RuleIncludeParams {
    code: String,
    #[serde(default)]
    ids: Option<Vec<Id>>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NetLengthsParams {
    #[serde(default)]
    net: Option<Id>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PadTableParams {
    #[serde(default)]
    component: Option<Id>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NetlistExportParams {
    #[serde(default)]
    net: Option<Id>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NetlistImportParams {
    nets: Vec<NetlistNet>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NetlistNet {
    name: String,
    pins: Vec<NetlistPin>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NetlistPin {
    reference: String,
    pin: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FindParams {
    #[serde(flatten)]
    filter: ComponentFilter,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FieldParams {
    id: Id,
    key: String,
    #[serde(default)]
    value: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReferenceParams {
    id: Id,
    reference: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RenameParams {
    id: Id,
    name: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GraphicRotateParams {
    id: Id,
    degrees: f64,
    around: Option<Point>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LabelRotateParams {
    id: Id,
    degrees: f64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AddPointParams {
    id: Id,
    index: u32,
    position: Point,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RemovePointParams {
    id: Id,
    index: u32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ChamferParams {
    a: Id,
    b: Id,
    size: f64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TrackWidthParams {
    id: Id,
    width: f64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ViaSizeParams {
    id: Id,
    diameter: f64,
    #[serde(default)]
    drill: Option<f64>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MirrorParams {
    id: Id,
    axis: String,
}
/// Mirror a library graphic primitive about the local origin; horizontal
/// flips left-to-right, vertical flips top-to-bottom. Arcs keep their three
/// defining points, so the mirrored arc is the reflected curve.
fn mirror_primitive(
    graphic: pcbcraft_library::Graphic,
    horizontal: bool,
) -> pcbcraft_library::Graphic {
    use pcbcraft_library::Graphic as G;
    let mirror = |point: Point| {
        if horizontal {
            Point::new(-point.x, point.y)
        } else {
            Point::new(point.x, -point.y)
        }
    };
    match graphic {
        G::Line {
            start,
            end,
            width,
            layer,
        } => G::Line {
            start: mirror(start),
            end: mirror(end),
            width,
            layer,
        },
        G::Rectangle {
            start,
            end,
            width,
            fill,
            layer,
        } => G::Rectangle {
            start: mirror(start),
            end: mirror(end),
            width,
            fill,
            layer,
        },
        G::Circle {
            center,
            radius,
            width,
            fill,
            layer,
        } => G::Circle {
            center: mirror(center),
            radius,
            width,
            fill,
            layer,
        },
        G::Arc {
            start,
            mid,
            end,
            width,
            layer,
        } => G::Arc {
            start: mirror(start),
            mid: mirror(mid),
            end: mirror(end),
            width,
            layer,
        },
        G::Polyline {
            points,
            width,
            fill,
            layer,
        } => G::Polyline {
            points: points.iter().map(|point| mirror(*point)).collect(),
            width,
            fill,
            layer,
        },
        G::Text {
            position,
            angle,
            text,
            size,
            layer,
        } => G::Text {
            position: mirror(position),
            angle: if horizontal { 180.0 - angle } else { -angle },
            text,
            size,
            layer,
        },
    }
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
struct TextPlaceParams {
    text: String,
    position: Point,
    #[serde(default = "text_size")]
    size: f64,
    #[serde(default = "front")]
    layer: Layer,
}
fn text_size() -> f64 {
    1.0
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TextMoveParams {
    id: Id,
    position: Point,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TextSetParams {
    id: Id,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    size: Option<f64>,
    #[serde(default)]
    layer: Option<Layer>,
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
    #[serde(default)]
    ids: Vec<Id>,
    #[serde(default)]
    filter: Option<ComponentFilter>,
    mode: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ComponentFilter {
    #[serde(default)]
    reference: Option<String>,
    #[serde(default)]
    value: Option<String>,
    #[serde(default)]
    kind: Option<String>,
}
/// Shared validation for reference/value/kind selection filters.
fn validate_filter(filter: &ComponentFilter) -> Result<(), Error> {
    if filter
        .reference
        .as_deref()
        .is_some_and(|r| r.is_empty() || r.len() > 64 || !r.contains(['*', '?']))
    {
        return Err(Error::Invalid(
            "reference pattern must be 1 to 64 bytes and contain * or ?".into(),
        ));
    }
    if filter
        .value
        .as_deref()
        .is_some_and(|v| v.is_empty() || v.len() > 256)
    {
        return Err(Error::Invalid("value filter must be 1 to 256 bytes".into()));
    }
    if filter
        .kind
        .as_deref()
        .is_some_and(|k| k.is_empty() || k.len() > 64)
    {
        return Err(Error::Invalid("kind filter must be 1 to 64 bytes".into()));
    }
    if filter.reference.is_none() && filter.value.is_none() && filter.kind.is_none() {
        return Err(Error::Invalid("filter needs at least one criterion".into()));
    }
    Ok(())
}
fn filter_matches(filter: &ComponentFilter, c: &Component) -> bool {
    filter
        .reference
        .as_deref()
        .is_none_or(|r| class_pattern_matches(r, &c.reference))
        && filter.value.as_deref().is_none_or(|v| c.value == v)
        && filter.kind.as_deref().is_none_or(|k| c.kind == k)
}
/// Center of an align item's bounding box on the requested axis.
/// True when any design object (component, net, wire, footprint, track, via,
/// zone or cutout) carries this id, so exclusions cannot name thin air.
fn object_exists(p: &Project, id: Id) -> bool {
    p.components.iter().any(|c| c.id == id)
        || p.nets.iter().any(|n| n.id == id)
        || p.wires.iter().any(|w| w.id == id)
        || p.drawn_wires.iter().any(|w| w.id == id)
        || p.board.footprints.iter().any(|f| f.id == id)
        || p.board.tracks.iter().any(|t| t.id == id)
        || p.board.vias.iter().any(|v| v.id == id)
        || p.board.zones.iter().any(|z| z.id == id)
        || p.board.cutouts.iter().any(|c| c.id == id)
        || p.board.texts.iter().any(|t| t.id == id)
}
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
struct CutoutMoveParams {
    id: Id,
    center: Point,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CutoutSizeParams {
    id: Id,
    radius: f64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TrackParams {
    net: Id,
    start: Point,
    end: Point,
    #[serde(default)]
    width: Option<f64>,
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
    #[serde(default)]
    width: Option<f64>,
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
/// Track width to use when a command leaves width out: the width of the net's
/// class, or the standard width for unclassified nets.
fn class_width(p: &Project, net: Id) -> f64 {
    p.net_class(net)
        .map(|class| class.width)
        .unwrap_or_else(default_width)
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
        "footprint.assign_pattern" => {
            let a: AssignPatternParams = parse(params)?;
            if a.pattern.is_empty() || a.pattern.len() > 64 || !a.pattern.contains(['*', '?']) {
                return Err(Error::Invalid(
                    "reference pattern must be 1 to 64 bytes and contain * or ?".into(),
                ));
            }
            if a.path.is_empty() || a.path.len() > 1024 {
                return Err(Error::Invalid("path must be 1 to 1024 bytes".into()));
            }
            let path = std::path::Path::new(&a.path);
            let definition = library::footprint(path)?;
            definition
                .validate()
                .map_err(|e| Error::Invalid(e.to_string()))?;
            validate_footprint_support(&definition)?;
            let library_name = path
                .parent()
                .and_then(|p| p.file_stem())
                .and_then(|s| s.to_str())
                .unwrap_or("library")
                .to_string();
            let matched: Vec<Id> = p
                .components
                .iter()
                .filter(|c| class_pattern_matches(&a.pattern, &c.reference))
                .map(|c| c.id)
                .collect();
            if matched.is_empty() {
                return Err(Error::Invalid(
                    "no component reference matches that pattern".into(),
                ));
            }
            let pads: std::collections::BTreeSet<_> =
                definition.pads.iter().map(|p| p.number.as_str()).collect();
            let mut assigned = vec![];
            let mut skipped = vec![];
            for id in matched {
                let c = p
                    .components
                    .iter_mut()
                    .find(|c| c.id == id)
                    .ok_or_else(missing)?;
                let pins: std::collections::BTreeSet<_> =
                    c.pins.iter().map(|p| p.number.as_str()).collect();
                if pins != pads {
                    skipped.push(id);
                    continue;
                }
                c.library_footprint = Some(LibraryFootprint {
                    library: library_name.clone(),
                    definition: definition.clone(),
                });
                if let Some(f) = p.board.footprints.iter_mut().find(|f| f.component == id) {
                    f.pads = component_pads(c);
                }
                assigned.push(id);
            }
            if assigned.is_empty() {
                return Err(Error::Invalid(
                    "no matched component can take this footprint; pad numbers differ".into(),
                ));
            }
            Ok(json!({"pattern": a.pattern, "assigned": assigned, "skipped": skipped}))
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
        "component.mirror" => {
            let a: MirrorParams = parse(params)?;
            let horizontal = match a.axis.as_str() {
                "horizontal" => true,
                "vertical" => false,
                _ => return Err(Error::Invalid("axis must be horizontal or vertical".into())),
            };
            let c = p
                .components
                .iter_mut()
                .find(|c| c.id == a.id)
                .ok_or_else(missing)?;
            let mirror = |point: Point| {
                if horizontal {
                    Point::new(-point.x, point.y)
                } else {
                    Point::new(point.x, -point.y)
                }
            };
            let anchor = c.position;
            let units: Vec<(Point, f64)> = c
                .units
                .iter()
                .map(|unit| {
                    let offset = mirror(Point::new(
                        unit.position.x - anchor.x,
                        unit.position.y - anchor.y,
                    ));
                    (
                        Point::new(anchor.x + offset.x, anchor.y + offset.y),
                        -unit.rotation,
                    )
                })
                .collect();
            if !units.iter().all(|(position, _)| position.valid()) {
                return Err(Error::Invalid(
                    "mirrored unit position is out of range".into(),
                ));
            }
            for pin in &mut c.pins {
                pin.offset = mirror(pin.offset);
                pin.direction = pin
                    .direction
                    .map(|d| if horizontal { 180.0 - d } else { -d });
            }
            c.rotation = -c.rotation;
            if let Some(symbol) = &mut c.library_symbol {
                for pin in &mut symbol.definition.pins {
                    pin.position = mirror(pin.position);
                    pin.angle = if horizontal {
                        180.0 - pin.angle
                    } else {
                        -pin.angle
                    };
                }
                for graphic in &mut symbol.definition.graphics {
                    graphic.primitive = mirror_primitive(graphic.primitive.clone(), horizontal);
                }
            }
            for (unit, (position, rotation)) in c.units.iter_mut().zip(units) {
                unit.position = position;
                unit.rotation = rotation;
            }
            Ok(json!({"id": a.id, "axis": a.axis}))
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
        "component.reference" => {
            let a: ReferenceParams = parse(params)?;
            if a.reference.is_empty() || a.reference.len() > 64 {
                return Err(Error::Invalid("reference must be 1 to 64 bytes".into()));
            }
            if p.components
                .iter()
                .any(|c| c.id != a.id && c.reference == a.reference)
            {
                return Err(Error::Invalid("reference already in use".into()));
            }
            let c = p
                .components
                .iter_mut()
                .find(|c| c.id == a.id)
                .ok_or_else(missing)?;
            c.reference = a.reference;
            Ok(json!({"id": a.id}))
        }
        "component.field" => {
            let a: FieldParams = parse(params)?;
            if a.key.is_empty() || a.key.len() > 64 {
                return Err(Error::Invalid("field name must be 1 to 64 bytes".into()));
            }
            if a.value.as_ref().is_some_and(|v| v.len() > 256) {
                return Err(Error::Invalid(
                    "field value must be at most 256 bytes".into(),
                ));
            }
            let component = p
                .components
                .iter_mut()
                .find(|c| c.id == a.id)
                .ok_or_else(missing)?;
            let existing = component.fields.iter().position(|f| f.key == a.key);
            let set = match (&a.value, existing) {
                (Some(value), Some(index)) => {
                    component.fields[index].value = value.clone();
                    true
                }
                (Some(value), None) => {
                    if component.fields.len() >= 32 {
                        return Err(Error::Invalid(
                            "a component carries at most 32 fields".into(),
                        ));
                    }
                    component.fields.push(ComponentField {
                        key: a.key.clone(),
                        value: value.clone(),
                    });
                    true
                }
                (None, Some(index)) => {
                    component.fields.remove(index);
                    false
                }
                (None, None) => {
                    return Err(Error::Invalid("component has no such field".into()));
                }
            };
            Ok(json!({"id": a.id, "key": a.key, "set": set}))
        }
        "component.find" => {
            let a: FindParams = parse(params)?;
            validate_filter(&a.filter)?;
            let mut matches: Vec<&Component> = p
                .components
                .iter()
                .filter(|c| filter_matches(&a.filter, c))
                .collect();
            matches.sort_by(|a, b| a.reference.cmp(&b.reference));
            let matches = matches
                .iter()
                .map(|c| {
                    json!({
                        "id": c.id,
                        "reference": c.reference,
                        "value": c.value,
                        "kind": c.kind,
                        "placed": p.board.footprints.iter().any(|f| f.component == c.id),
                    })
                })
                .collect::<Vec<Value>>();
            let count = matches.len();
            Ok(json!({"components": matches, "count": count}))
        }
        "net.create" => {
            let a: Name = parse(params)?;
            let id = allocate(next)?;
            p.nets.push(Net {
                id,
                name: a.name,
                ..Default::default()
            });
            Ok(json!({"id":id}))
        }
        "net.rename" => {
            let a: RenameParams = parse(params)?;
            if a.name.is_empty() || a.name.len() > 128 {
                return Err(Error::Invalid("net name must be 1 to 128 bytes".into()));
            }
            if p.nets.iter().any(|n| n.id != a.id && n.name == a.name) {
                return Err(Error::Invalid("net name already in use".into()));
            }
            let net = p
                .nets
                .iter_mut()
                .find(|n| n.id == a.id)
                .ok_or_else(missing)?;
            net.name = a.name;
            Ok(json!({"id": a.id}))
        }
        "net.set_length" => {
            let a: NetLengthParams = parse(params)?;
            if a.target
                .is_some_and(|t| !t.is_finite() || t <= 0.0 || t > 100_000.0)
            {
                return Err(Error::Invalid(
                    "length target must be between 0 and 100000 millimetres".into(),
                ));
            }
            if a.tolerance
                .is_some_and(|t| !t.is_finite() || t < 0.0 || t > 100_000.0)
            {
                return Err(Error::Invalid(
                    "length tolerance must be between 0 and 100000 millimetres".into(),
                ));
            }
            if a.target.is_none() && a.tolerance.is_some() {
                return Err(Error::Invalid(
                    "a length tolerance needs a length target".into(),
                ));
            }
            let net = p
                .nets
                .iter_mut()
                .find(|n| n.id == a.id)
                .ok_or_else(missing)?;
            net.target_length = a.target;
            net.length_tolerance = if a.target.is_some() {
                a.tolerance
            } else {
                None
            };
            Ok(json!({
                "id":a.id,
                "target":net.target_length,
                "tolerance":net.length_tolerance.unwrap_or(0.0),
            }))
        }
        "net.connect" => {
            let a: Connect = parse(params)?;
            assign(p, a.net, &a.pins)?;
            Ok(json!({"net":a.net,"pins":a.pins.len()}))
        }
        "net.disconnect" => {
            let a: Connect = parse(params)?;
            if a.pins.is_empty() || a.pins.len() > 128 {
                return Err(Error::Invalid("invalid pin list or unknown net".into()));
            }
            if !p.nets.iter().any(|n| n.id == a.net) {
                return Err(Error::Invalid("invalid pin list or unknown net".into()));
            }
            for r in &a.pins {
                let pin = p
                    .components
                    .iter()
                    .find(|c| c.id == r.component)
                    .and_then(|c| c.pins.iter().find(|pin| pin.number == r.pin))
                    .ok_or_else(missing)?;
                if pin.net != Some(a.net) {
                    return Err(Error::Invalid(format!(
                        "pin {} of component {} is not on that net",
                        r.pin, r.component
                    )));
                }
            }
            let mut released = 0;
            for r in &a.pins {
                for c in p.components.iter_mut().filter(|c| c.id == r.component) {
                    for pin in c.pins.iter_mut().filter(|pin| pin.number == r.pin) {
                        pin.net = None;
                        released += 1;
                    }
                }
                for f in p
                    .board
                    .footprints
                    .iter_mut()
                    .filter(|f| f.component == r.component)
                {
                    for pad in f.pads.iter_mut().filter(|pad| pad.number == r.pin) {
                        pad.net = None;
                    }
                }
            }
            Ok(json!({"net":a.net,"released":released}))
        }
        "copper.set_net" => {
            let a: CopperNetParams = parse(params)?;
            if !p.nets.iter().any(|n| n.id == a.net) {
                return Err(Error::Invalid("unknown net".into()));
            }
            let already = || Error::Invalid("that copper is already on the net".into());
            if let Some(i) = p.board.tracks.iter().position(|t| t.id == a.id) {
                let track = p.board.tracks.get_mut(i).ok_or_else(missing)?;
                if track.net == a.net {
                    return Err(already());
                }
                track.net = a.net;
            } else if let Some(i) = p.board.vias.iter().position(|v| v.id == a.id) {
                let via = p.board.vias.get_mut(i).ok_or_else(missing)?;
                if via.net == a.net {
                    return Err(already());
                }
                via.net = a.net;
            } else if let Some(i) = p.board.zones.iter().position(|z| z.id == a.id) {
                let zone = p.board.zones.get_mut(i).ok_or_else(missing)?;
                if zone.net == a.net {
                    return Err(already());
                }
                zone.net = a.net;
            } else {
                return Err(Error::Invalid(
                    "id is not a placed track, via or copper zone".into(),
                ));
            }
            Ok(json!({"id":a.id,"net":a.net}))
        }
        "netclass.add" => {
            let a: NetClassParams = parse(params)?;
            if a.name.is_empty() || a.name.len() > 128 {
                return Err(Error::Invalid(
                    "net class name must be 1 to 128 bytes".into(),
                ));
            }
            if p.net_classes.iter().any(|c| c.name == a.name) {
                return Err(Error::Invalid("net class name already in use".into()));
            }
            if !a.clearance.is_finite() || a.clearance <= 0.0 || a.clearance > 100.0 {
                return Err(Error::Invalid(
                    "clearance must be between 0 and 100 millimetres".into(),
                ));
            }
            if !a.width.is_finite() || a.width <= 0.0 || a.width > 100.0 {
                return Err(Error::Invalid(
                    "width must be between 0 and 100 millimetres".into(),
                ));
            }
            if a.nets.len() > 256 {
                return Err(Error::Invalid("a class holds at most 256 nets".into()));
            }
            for net in &a.nets {
                if !p.nets.iter().any(|n| n.id == *net) {
                    return Err(Error::Invalid("unknown net in class membership".into()));
                }
                if let Some(existing) = p.net_class(*net) {
                    return Err(Error::Invalid(format!(
                        "net already belongs to class {}",
                        existing.name
                    )));
                }
            }
            let id = allocate(next)?;
            p.net_classes.push(NetClass {
                id,
                name: a.name,
                clearance: a.clearance,
                width: a.width,
                min_width: None,
                max_width: None,
                target_length: None,
                length_tolerance: None,
                nets: a.nets,
            });
            Ok(json!({"id": id}))
        }
        "netclass.assign" => {
            let a: NetClassAssignParams = parse(params)?;
            if !p.nets.iter().any(|n| n.id == a.net) {
                return Err(missing());
            }
            if a.class
                .is_some_and(|c| !p.net_classes.iter().any(|k| k.id == c))
            {
                return Err(Error::Invalid("unknown net class".into()));
            }
            if a.class.is_some_and(|class| {
                p.net_classes
                    .iter()
                    .any(|k| k.id == class && k.nets.len() >= 256 && !k.nets.contains(&a.net))
            }) {
                return Err(Error::Invalid("that class already holds 256 nets".into()));
            }
            for class in p.net_classes.iter_mut() {
                class.nets.retain(|net| *net != a.net);
            }
            if let Some(target) = p.net_classes.iter_mut().find(|k| a.class == Some(k.id)) {
                target.nets.push(a.net);
            }
            Ok(json!({"net": a.net, "class": a.class}))
        }
        "netclass.width_rules" => {
            let a: WidthRulesParams = parse(params)?;
            for bound in [a.min_width, a.max_width] {
                if bound.is_some_and(|w| !w.is_finite() || w <= 0.0 || w > 100.0) {
                    return Err(Error::Invalid(
                        "width bounds must be above 0 and at most 100 mm".into(),
                    ));
                }
            }
            if a.min_width
                .zip(a.max_width)
                .is_some_and(|(min, max)| min > max)
            {
                return Err(Error::Invalid(
                    "the minimum width cannot exceed the maximum".into(),
                ));
            }
            let class = p
                .net_classes
                .iter_mut()
                .find(|k| k.id == a.class)
                .ok_or_else(missing)?;
            if a.min_width.is_none() && a.max_width.is_none() {
                return Err(Error::Invalid("give at least one width bound".into()));
            }
            let min_width = a.min_width.or(class.min_width);
            let max_width = a.max_width.or(class.max_width);
            if min_width.zip(max_width).is_some_and(|(min, max)| min > max) {
                return Err(Error::Invalid(
                    "the new bounds disagree with the bound being kept".into(),
                ));
            }
            class.min_width = min_width;
            class.max_width = max_width;
            Ok(json!({
                "class": a.class,
                "min_width": class.min_width,
                "max_width": class.max_width,
            }))
        }
        "netclass.length_rules" => {
            let a: LengthRulesParams = parse(params)?;
            if a.target_length
                .is_some_and(|t| !t.is_finite() || t <= 0.0 || t > 10_000.0)
            {
                return Err(Error::Invalid(
                    "target length must be above 0 and at most 10000 mm".into(),
                ));
            }
            if a.length_tolerance
                .is_some_and(|t| !t.is_finite() || t < 0.0 || t > 1000.0)
            {
                return Err(Error::Invalid(
                    "length tolerance must be between 0 and 1000 mm".into(),
                ));
            }
            if a.target_length.is_none() && a.length_tolerance.is_none() {
                return Err(Error::Invalid("give a target length or a tolerance".into()));
            }
            let class = p
                .net_classes
                .iter_mut()
                .find(|k| k.id == a.class)
                .ok_or_else(missing)?;
            let target_length = a.target_length.or(class.target_length);
            let length_tolerance = a.length_tolerance.or(class.length_tolerance);
            if length_tolerance.is_some() && target_length.is_none() {
                return Err(Error::Invalid(
                    "a length tolerance needs a target length".into(),
                ));
            }
            class.target_length = target_length;
            class.length_tolerance = length_tolerance;
            Ok(json!({
                "class": a.class,
                "target_length": class.target_length,
                "length_tolerance": class.length_tolerance,
            }))
        }
        "netclass.clearance" => {
            let a: ClassClearanceParams = parse(params)?;
            if !a.clearance.is_finite() || a.clearance <= 0.0 || a.clearance > 100.0 {
                return Err(Error::Invalid(
                    "clearance must be between 0 and 100 millimetres".into(),
                ));
            }
            let class = p
                .net_classes
                .iter_mut()
                .find(|k| k.id == a.class)
                .ok_or_else(missing)?;
            class.clearance = a.clearance;
            Ok(json!({"class":a.class,"clearance":class.clearance,"nets":class.nets.len()}))
        }
        "netclass.remove" => {
            let a: ClassRemoveParams = parse(params)?;
            let index = p
                .net_classes
                .iter()
                .position(|k| k.id == a.class)
                .ok_or_else(missing)?;
            let freed = p.net_classes.remove(index).nets.len();
            Ok(json!({"removed":a.class,"nets_released":freed}))
        }
        "netclass.assign_pattern" => {
            let a: NetClassPatternParams = parse(params)?;
            if a.pattern.is_empty() || a.pattern.len() > 64 {
                return Err(Error::Invalid("pattern must be 1 to 64 bytes".into()));
            }
            if !a.pattern.contains(['*', '?']) {
                return Err(Error::Invalid(
                    "pattern must contain * or ?; a plain name is one netclass.assign".into(),
                ));
            }
            if !p.net_classes.iter().any(|k| k.id == a.class) {
                return Err(Error::Invalid("unknown net class".into()));
            }
            let matched: Vec<Id> = p
                .nets
                .iter()
                .filter(|n| class_pattern_matches(&a.pattern, &n.name))
                .map(|n| n.id)
                .collect();
            if matched.is_empty() {
                return Err(Error::Invalid("no net name matches that pattern".into()));
            }
            let class = p
                .net_classes
                .iter()
                .find(|k| k.id == a.class)
                .ok_or_else(missing)?;
            let outside = matched
                .iter()
                .filter(|net| !class.nets.contains(net))
                .count();
            if class.nets.len() + outside > 256 {
                return Err(Error::Invalid("that class already holds 256 nets".into()));
            }
            for class in p.net_classes.iter_mut() {
                class.nets.retain(|net| !matched.contains(net));
            }
            if let Some(target) = p.net_classes.iter_mut().find(|k| k.id == a.class) {
                target.nets.extend(matched.iter().copied());
            }
            let count = matched.len();
            Ok(json!({"class": a.class, "pattern": a.pattern, "matched": count}))
        }
        "netlist.import" => {
            let a: NetlistImportParams = parse(params)?;
            if a.nets.is_empty() || a.nets.len() > 256 {
                return Err(Error::Invalid("netlist must hold 1 to 256 nets".into()));
            }
            let total: usize = a.nets.iter().map(|n| n.pins.len()).sum();
            if total > 4096 {
                return Err(Error::Invalid("netlist has too many pins".into()));
            }
            let mut planned: std::collections::BTreeMap<(Id, String), String> =
                std::collections::BTreeMap::new();
            let mut plan: Vec<(&str, Vec<PinRef>)> = Vec::new();
            for entry in &a.nets {
                if entry.name.is_empty() || entry.name.len() > 128 {
                    return Err(Error::Invalid("net name must be 1 to 128 bytes".into()));
                }
                if entry.pins.is_empty() || entry.pins.len() > 128 {
                    return Err(Error::Invalid(
                        "each netlist net must list 1 to 128 pins".into(),
                    ));
                }
                let mut pins = Vec::with_capacity(entry.pins.len());
                for listed in &entry.pins {
                    if listed.reference.is_empty()
                        || listed.reference.len() > 64
                        || listed.pin.is_empty()
                        || listed.pin.len() > 64
                    {
                        return Err(Error::Invalid(
                            "netlist references must be 1 to 64 bytes".into(),
                        ));
                    }
                    let component = p
                        .components
                        .iter()
                        .find(|c| c.reference == listed.reference)
                        .ok_or_else(|| {
                            Error::Invalid(format!(
                                "netlist names unknown reference {}",
                                listed.reference
                            ))
                        })?;
                    let pin = component
                        .pins
                        .iter()
                        .find(|pin| pin.number == listed.pin)
                        .ok_or_else(|| {
                            Error::Invalid(format!(
                                "{} has no pin {}",
                                listed.reference, listed.pin
                            ))
                        })?;
                    if pin.net.is_some_and(|other| {
                        !p.nets.iter().any(|n| n.id == other && n.name == entry.name)
                    }) {
                        return Err(Error::Invalid(format!(
                            "{} pin {} already belongs to another net",
                            listed.reference, listed.pin
                        )));
                    }
                    let previous =
                        planned.insert((component.id, listed.pin.clone()), entry.name.clone());
                    if previous.is_some_and(|previous| previous != entry.name) {
                        return Err(Error::Invalid(format!(
                            "{} pin {} is listed under two nets",
                            listed.reference, listed.pin
                        )));
                    }
                    pins.push(PinRef {
                        component: component.id,
                        pin: listed.pin.clone(),
                    });
                }
                plan.push((&entry.name, pins));
            }
            let mut created = 0usize;
            let mut assigned = 0usize;
            for (name, pins) in &plan {
                let id = match p.nets.iter().find(|n| &n.name == name).map(|n| n.id) {
                    Some(id) => id,
                    None => {
                        let id = allocate(next)?;
                        p.nets.push(Net {
                            id,
                            name: (*name).to_string(),
                            ..Default::default()
                        });
                        created += 1;
                        id
                    }
                };
                assign(p, id, pins)?;
                assigned += pins.len();
            }
            Ok(json!({"nets": a.nets.len(), "created": created, "pins": assigned}))
        }
        "netlist.export" => {
            let a: NetlistExportParams = parse(params)?;
            if a.net.is_some_and(|id| !p.nets.iter().any(|n| n.id == id)) {
                return Err(Error::Invalid("unknown net".into()));
            }
            let mut chosen: Vec<(String, Id)> = p
                .nets
                .iter()
                .filter(|n| a.net.is_none_or(|id| n.id == id))
                .map(|n| (n.name.clone(), n.id))
                .collect();
            chosen.sort();
            let mut nets: Vec<Value> = Vec::new();
            let mut listed = 0usize;
            for (name, net) in chosen {
                let mut pins: Vec<(String, String)> = Vec::new();
                for c in &p.components {
                    for pin in &c.pins {
                        if pin.net == Some(net) {
                            pins.push((c.reference.clone(), pin.number.clone()));
                        }
                    }
                }
                pins.sort();
                listed += pins.len();
                nets.push(json!({
                    "name": name,
                    "pins": pins
                        .into_iter()
                        .map(|(reference, pin)| json!({"reference": reference, "pin": pin}))
                        .collect::<Vec<Value>>(),
                }));
            }
            let mut unconnected: Vec<(String, String)> = Vec::new();
            for c in &p.components {
                for pin in &c.pins {
                    if pin.net.is_none()
                        && !p
                            .no_connects
                            .iter()
                            .any(|f| f.pin.component == c.id && f.pin.pin == pin.number)
                    {
                        unconnected.push((c.reference.clone(), pin.number.clone()));
                    }
                }
            }
            unconnected.sort();
            Ok(json!({
                "nets": nets,
                "pins": listed,
                "unconnected": unconnected
                    .into_iter()
                    .map(|(reference, pin)| json!({"reference": reference, "pin": pin}))
                    .collect::<Vec<Value>>(),
            }))
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
                    ..Default::default()
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
        "wire.add_point" => {
            let a: AddPointParams = parse(params)?;
            if !a.position.valid() {
                return Err(Error::Invalid("vertex is out of range".into()));
            }
            let wire = p
                .drawn_wires
                .iter_mut()
                .find(|w| w.id == a.id)
                .ok_or_else(missing)?;
            if wire.points.len() >= 256 {
                return Err(Error::Invalid("wire has too many vertices".into()));
            }
            let at = usize::try_from(a.index).map_err(|_| missing())?;
            if at > wire.points.len() {
                return Err(Error::Invalid("insert index is out of range".into()));
            }
            let near = (at > 0 && wire.points[at - 1].distance(a.position) < 0.01)
                || (at < wire.points.len() && wire.points[at].distance(a.position) < 0.01);
            if near {
                return Err(Error::Invalid(
                    "vertex is too close to a neighbouring vertex".into(),
                ));
            }
            wire.points.insert(at, a.position);
            Ok(json!({"id": a.id, "index": at}))
        }
        "wire.remove_point" => {
            let a: RemovePointParams = parse(params)?;
            let wire = p
                .drawn_wires
                .iter_mut()
                .find(|w| w.id == a.id)
                .ok_or_else(missing)?;
            if wire.points.len() <= 2 {
                return Err(Error::Invalid(
                    "a wire must keep at least two vertices".into(),
                ));
            }
            let at = usize::try_from(a.index).map_err(|_| missing())?;
            if at >= wire.points.len() {
                return Err(Error::Invalid("vertex index is out of range".into()));
            }
            let removed = wire.points[at];
            if p.labels
                .iter()
                .any(|l| l.kind == "junction" && wire.net == Some(l.net) && l.position == removed)
            {
                return Err(Error::Invalid(
                    "a junction sits on this vertex; move the junction first".into(),
                ));
            }
            wire.points.remove(at);
            Ok(json!({"id": a.id}))
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
        "graphic.rotate" => {
            let a: GraphicRotateParams = parse(params)?;
            if !a.degrees.is_finite() {
                return Err(Error::Invalid("rotation must be finite".into()));
            }
            let steps = (a.degrees / 90.0).round();
            if steps == 0.0 || (a.degrees - steps * 90.0).abs() > 1e-9 {
                return Err(Error::Invalid(
                    "graphic rotation must be a nonzero multiple of 90 degrees".into(),
                ));
            }
            if a.around.is_some_and(|p| !p.valid()) {
                return Err(Error::Invalid("pivot point is out of range".into()));
            }
            let graphic = p
                .graphics
                .iter_mut()
                .find(|g| g.id == a.id)
                .ok_or_else(missing)?;
            let center = match a.around {
                Some(point) => point,
                None => {
                    let mut bounds = (
                        f64::INFINITY,
                        f64::NEG_INFINITY,
                        f64::INFINITY,
                        f64::NEG_INFINITY,
                    );
                    for point in &graphic.points {
                        bounds.0 = bounds.0.min(point.x);
                        bounds.1 = bounds.1.max(point.x);
                        bounds.2 = bounds.2.min(point.y);
                        bounds.3 = bounds.3.max(point.y);
                    }
                    Point::new((bounds.0 + bounds.1) / 2.0, (bounds.2 + bounds.3) / 2.0)
                }
            };
            let rotated: Vec<Point> = graphic
                .points
                .iter()
                .map(|p| {
                    let r = Point::new(p.x - center.x, p.y - center.y).rotate(steps * 90.0);
                    Point::new(center.x + r.x, center.y + r.y)
                })
                .collect();
            if !rotated.iter().all(|p| p.valid()) {
                return Err(Error::Invalid("rotated vertex is out of range".into()));
            }
            graphic.points = rotated;
            Ok(json!({"id": a.id}))
        }
        "graphic.set" => {
            let a: GraphicSetParams = parse(params)?;
            if a.layer.is_none() && a.width.is_none() && a.text.is_none() {
                return Err(Error::Invalid(
                    "give a layer, width or text to change".into(),
                ));
            }
            if a.layer
                .as_deref()
                .is_some_and(|l| l != "schematic" && l != "board")
            {
                return Err(Error::Invalid(
                    "graphic layer must be schematic or board".into(),
                ));
            }
            if a.width
                .is_some_and(|w| !w.is_finite() || w <= 0.0 || w > 1_000.0)
            {
                return Err(Error::Invalid(
                    "graphic width, radius or text size must be within 0 to 1000 mm".into(),
                ));
            }
            if a.text
                .as_ref()
                .is_some_and(|t| t.is_empty() || t.len() > 1024)
            {
                return Err(Error::Invalid("graphic text is too long".into()));
            }
            let graphic = p
                .graphics
                .iter_mut()
                .find(|g| g.id == a.id)
                .ok_or_else(missing)?;
            if a.text.is_some() && graphic.shape != "text" {
                return Err(Error::Invalid("only text graphics may carry text".into()));
            }
            if let Some(layer) = a.layer {
                graphic.layer = layer;
            }
            if let Some(width) = a.width {
                graphic.width = width;
            }
            if let Some(text) = a.text {
                graphic.text = text;
            }
            Ok(json!({
                "id":a.id,
                "layer":graphic.layer,
                "width":graphic.width,
                "text":graphic.text,
            }))
        }
        "graphic.set_point" => {
            let a: GraphicPointParams = parse(params)?;
            if !a.position.valid() {
                return Err(Error::Invalid("graphic vertex is out of range".into()));
            }
            let graphic = p
                .graphics
                .iter_mut()
                .find(|g| g.id == a.id)
                .ok_or_else(missing)?;
            if a.index >= graphic.points.len() {
                return Err(Error::Invalid(
                    "vertex index is outside this graphic's points".into(),
                ));
            }
            graphic.points[a.index] = a.position;
            Ok(json!({"id":a.id,"index":a.index,"position":a.position}))
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
        "label.rotate" => {
            let a: LabelRotateParams = parse(params)?;
            if !a.degrees.is_finite() {
                return Err(Error::Invalid("rotation must be finite".into()));
            }
            let index = p
                .labels
                .iter()
                .position(|l| l.id == a.id)
                .ok_or_else(missing)?;
            p.labels[index].rotation = (p.labels[index].rotation + a.degrees).rem_euclid(360.0);
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
        "zone.move" => {
            let a: DeltaParams = parse(params)?;
            let zone = p
                .board
                .zones
                .iter_mut()
                .find(|z| z.id == a.id)
                .ok_or_else(missing)?;
            let shift = |point: &Point| Point::new(point.x + a.delta.x, point.y + a.delta.y);
            let outline: Vec<Point> = zone.outline.iter().map(shift).collect();
            let cutouts: Vec<Vec<Point>> = zone
                .cutouts
                .iter()
                .map(|c| c.iter().map(shift).collect())
                .collect();
            if !outline
                .iter()
                .chain(cutouts.iter().flatten())
                .all(|point| point.valid())
            {
                return Err(Error::Invalid("zone vertex is out of range".into()));
            }
            zone.outline = outline;
            zone.cutouts = cutouts;
            Ok(json!({"id": a.id}))
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
        "pad.table" => {
            let a: PadTableParams = parse(params)?;
            if a.component
                .is_some_and(|id| !p.components.iter().any(|c| c.id == id))
            {
                return Err(missing());
            }
            let mut rows: Vec<(String, String, Value)> = Vec::new();
            let mut unassigned = 0usize;
            for footprint in &p.board.footprints {
                let Some(component) = p.component(footprint.component) else {
                    continue;
                };
                if a.component.is_some_and(|id| id != footprint.component) {
                    continue;
                }
                for pad in &footprint.pads {
                    let position = footprint.pad_position(&pad.number).unwrap_or(pad.offset);
                    let net = pad
                        .net
                        .and_then(|id| p.nets.iter().find(|n| n.id == id).map(|n| n.name.clone()));
                    if net.is_none() {
                        unassigned += 1;
                    }
                    let (shape, size, drill) = match &pad.geometry {
                        Some(g) => (
                            g.shape.clone(),
                            json!({"x": g.size.x, "y": g.size.y}),
                            g.drill
                                .as_ref()
                                .map(|d| json!({"x": d.size.x, "y": d.size.y}))
                                .unwrap_or_default(),
                        ),
                        None => (
                            "circle".into(),
                            json!({"x": pad.diameter, "y": pad.diameter}),
                            Value::Null,
                        ),
                    };
                    rows.push((
                        component.reference.clone(),
                        pad.number.clone(),
                        json!({
                            "reference": component.reference,
                            "footprint": footprint.id,
                            "pad": pad.number,
                            "net": net,
                            "position": {"x": position.x, "y": position.y},
                            "shape": shape,
                            "size": size,
                            "drill": drill,
                        }),
                    ));
                }
            }
            rows.sort_by(|left, right| (&left.0, &left.1).cmp(&(&right.0, &right.1)));
            let pads: Vec<Value> = rows.into_iter().map(|(_, _, row)| row).collect();
            let count = pads.len();
            Ok(json!({"pads": pads, "count": count, "unassigned": unassigned}))
        }
        "net.lengths" => {
            let a: NetLengthsParams = parse(params)?;
            if a.net.is_some_and(|id| !p.nets.iter().any(|n| n.id == id)) {
                return Err(missing());
            }
            struct Row {
                segments: usize,
                length: f64,
                vias: usize,
            }
            let mut rows: std::collections::BTreeMap<Id, Row> = std::collections::BTreeMap::new();
            for track in &p.board.tracks {
                if !track.start.x.is_finite()
                    || !track.start.y.is_finite()
                    || !track.end.x.is_finite()
                    || !track.end.y.is_finite()
                {
                    return Err(Error::Invalid(
                        "a track position must be a finite number".into(),
                    ));
                }
                let row = rows.entry(track.net).or_insert(Row {
                    segments: 0,
                    length: 0.0,
                    vias: 0,
                });
                row.segments += 1;
                row.length += ((track.end.x - track.start.x).powi(2)
                    + (track.end.y - track.start.y).powi(2))
                .sqrt();
            }
            for via in &p.board.vias {
                rows.entry(via.net)
                    .or_insert(Row {
                        segments: 0,
                        length: 0.0,
                        vias: 0,
                    })
                    .vias += 1;
            }
            let mut nets: Vec<Value> = Vec::new();
            let mut unrouted: Vec<Value> = Vec::new();
            for net in &p.nets {
                if a.net.is_some_and(|id| net.id != id) {
                    continue;
                }
                match rows.get(&net.id) {
                    Some(row) => {
                        let class = p.net_class(net.id);
                        let own = net.target_length.is_some();
                        nets.push(json!({
                            "id": net.id,
                            "name": net.name,
                            "segments": row.segments,
                            "vias": row.vias,
                            "length": row.length,
                            "target_length": net.target_length.or(class.and_then(|k| k.target_length)),
                            "length_tolerance": if own {
                                net.length_tolerance
                            } else {
                                class
                                    .filter(|k| k.target_length.is_some())
                                    .and_then(|k| k.length_tolerance)
                            },
                        }))
                    }
                    None => unrouted.push(json!({"id": net.id, "name": net.name})),
                }
            }
            let total: f64 = nets
                .iter()
                .map(|n| n["length"].as_f64().unwrap_or(0.0))
                .sum();
            let routed = nets.len();
            nets.sort_by(|a, b| {
                let left = a["length"].as_f64().unwrap_or(0.0);
                let right = b["length"].as_f64().unwrap_or(0.0);
                right.total_cmp(&left)
            });
            Ok(json!({
                "nets": nets,
                "routed": routed,
                "unrouted": unrouted,
                "total": total,
            }))
        }
        "rules.severity" => {
            let a: RuleSeverityParams = parse(params)?;
            if a.code.is_empty() || a.code.len() > 32 {
                return Err(Error::Invalid("check code must be 1 to 32 bytes".into()));
            }
            if a.severity
                .as_deref()
                .is_some_and(|s| !matches!(s, "error" | "warning" | "off"))
            {
                return Err(Error::Invalid(
                    "severity must be error, warning or off".into(),
                ));
            }
            let set = match &a.severity {
                Some(severity) => {
                    if !p.rule_severity.contains_key(&a.code) && p.rule_severity.len() >= 64 {
                        return Err(Error::Invalid(
                            "severity overrides cover at most 64 check codes".into(),
                        ));
                    }
                    p.rule_severity.insert(a.code.clone(), severity.clone());
                    true
                }
                None => {
                    if p.rule_severity.remove(&a.code).is_none() {
                        return Err(Error::Invalid(
                            "that check code has no severity override".into(),
                        ));
                    }
                    false
                }
            };
            Ok(json!({"code": a.code, "set": set}))
        }
        "rules.exclude" => {
            let a: RuleExcludeParams = parse(params)?;
            if a.code.is_empty() || a.code.len() > 32 {
                return Err(Error::Invalid("check code must be 1 to 32 bytes".into()));
            }
            if a.ids.len() > 256 {
                return Err(Error::Invalid(
                    "a check code excludes at most 256 objects".into(),
                ));
            }
            for id in &a.ids {
                if !object_exists(p, *id) {
                    return Err(Error::Invalid(format!("no design object with id {id}")));
                }
            }
            let existing = p.rule_exclusions.get(&a.code).cloned().unwrap_or_default();
            if existing.is_empty() && p.rule_exclusions.len() >= 64 {
                return Err(Error::Invalid(
                    "exclusions cover at most 64 check codes".into(),
                ));
            }
            let mut merged = existing;
            let mut added = 0usize;
            for id in &a.ids {
                if !merged.contains(id) {
                    merged.push(*id);
                    added += 1;
                }
            }
            if added == 0 {
                return Err(Error::Invalid(
                    "those objects are already excluded for that code".into(),
                ));
            }
            merged.sort_unstable();
            p.rule_exclusions.insert(a.code.clone(), merged);
            Ok(json!({"code": a.code, "added": added}))
        }
        "rules.include" => {
            let a: RuleIncludeParams = parse(params)?;
            if a.code.is_empty() || a.code.len() > 32 {
                return Err(Error::Invalid("check code must be 1 to 32 bytes".into()));
            }
            if a.ids
                .as_ref()
                .is_some_and(|v| v.is_empty() || v.len() > 256)
            {
                return Err(Error::Invalid(
                    "ids must hold between 1 and 256 entries".into(),
                ));
            }
            match a.ids {
                None => {
                    let Some(list) = p.rule_exclusions.remove(&a.code) else {
                        return Err(Error::Invalid(
                            "that check code has no object exclusions".into(),
                        ));
                    };
                    Ok(json!({"code": a.code, "removed": list.len()}))
                }
                Some(ids) => {
                    let Some(list) = p.rule_exclusions.get_mut(&a.code) else {
                        return Err(Error::Invalid(
                            "that check code has no object exclusions".into(),
                        ));
                    };
                    let before = list.len();
                    list.retain(|x| !ids.contains(x));
                    let removed = before - list.len();
                    if removed == 0 {
                        return Err(Error::Invalid(
                            "none of those objects are excluded for that code".into(),
                        ));
                    }
                    if list.is_empty() {
                        p.rule_exclusions.remove(&a.code);
                    }
                    Ok(json!({"code": a.code, "removed": removed}))
                }
            }
        }
        "bom.export" => {
            let a: BomParams = parse(params)?;
            if a.fields.len() > 16 {
                return Err(Error::Invalid(
                    "a bill of materials lists at most 16 properties".into(),
                ));
            }
            for key in &a.fields {
                if key.is_empty() || key.len() > 64 {
                    return Err(Error::Invalid(
                        "property names must be 1 to 64 bytes".into(),
                    ));
                }
            }
            let mut groups: std::collections::BTreeMap<
                (String, String, String, Vec<String>),
                Vec<String>,
            > = std::collections::BTreeMap::new();
            for c in &p.components {
                let footprint = match &c.library_footprint {
                    Some(footprint) => {
                        format!("{}:{}", footprint.library, footprint.definition.name)
                    }
                    None => "starter pads".into(),
                };
                let values: Vec<String> = a
                    .fields
                    .iter()
                    .map(|key| c.field(key).unwrap_or("").to_string())
                    .collect();
                groups
                    .entry((c.value.clone(), c.kind.clone(), footprint, values))
                    .or_default()
                    .push(c.reference.clone());
            }
            let items: Vec<Value> = groups
                .into_iter()
                .map(|((value, kind, footprint, values), mut references)| {
                    references.sort();
                    let columns = a.fields.clone();
                    let mut fields = serde_json::Map::new();
                    for (key, value) in columns.into_iter().zip(values) {
                        fields.insert(key, json!(value));
                    }
                    json!({
                        "quantity": references.len(),
                        "references": references,
                        "value": value,
                        "kind": kind,
                        "footprint": footprint,
                        "fields": Value::Object(fields),
                    })
                })
                .collect();
            let components = p.components.len();
            Ok(json!({
                "items": items,
                "groups": items.len(),
                "components": components,
            }))
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
        "board.cutout.move" => {
            let a: CutoutMoveParams = parse(params)?;
            if !a.center.valid() {
                return Err(Error::Invalid("cutout centre is out of range".into()));
            }
            let cutout = p
                .board
                .cutouts
                .iter_mut()
                .find(|c| c.id == a.id)
                .ok_or_else(missing)?;
            if a.center.x - cutout.radius <= 0.0
                || a.center.y - cutout.radius <= 0.0
                || a.center.x + cutout.radius >= p.board.width
                || a.center.y + cutout.radius >= p.board.height
            {
                return Err(Error::Invalid(
                    "cutout must be strictly inside the board".into(),
                ));
            }
            cutout.center = a.center;
            Ok(json!({"id":a.id,"center":a.center}))
        }
        "board.cutout.resize" => {
            let a: CutoutSizeParams = parse(params)?;
            if !a.radius.is_finite() || a.radius <= 0.0 || a.radius > 1000.0 {
                return Err(Error::Invalid(
                    "radius must be between 0 and 1000 millimetres".into(),
                ));
            }
            let cutout = p
                .board
                .cutouts
                .iter_mut()
                .find(|c| c.id == a.id)
                .ok_or_else(missing)?;
            if cutout.center.x - a.radius <= 0.0
                || cutout.center.y - a.radius <= 0.0
                || cutout.center.x + a.radius >= p.board.width
                || cutout.center.y + a.radius >= p.board.height
            {
                return Err(Error::Invalid(
                    "cutout must be strictly inside the board".into(),
                ));
            }
            cutout.radius = a.radius;
            Ok(json!({"id":a.id,"radius":a.radius}))
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
            let width = a.width.unwrap_or_else(|| class_width(p, a.net));
            if !positive(width) {
                return Err(Error::Invalid("width must be positive".into()));
            }
            let id = allocate(next)?;
            p.board.tracks.push(Track {
                id,
                net: a.net,
                start: a.start,
                end: a.end,
                width,
                layer: a.layer,
            });
            Ok(json!({"id":id, "width": width}))
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
        "track.chamfer" => {
            let a: ChamferParams = parse(params)?;
            if !a.size.is_finite() || a.size <= 0.0 {
                return Err(Error::Invalid("chamfer size must be positive".into()));
            }
            let ia = p
                .board
                .tracks
                .iter()
                .position(|t| t.id == a.a)
                .ok_or_else(missing)?;
            let ib = p
                .board
                .tracks
                .iter()
                .position(|t| t.id == a.b)
                .ok_or_else(missing)?;
            if ia == ib {
                return Err(Error::Invalid("the two tracks must be different".into()));
            }
            let (net, layer) = (p.board.tracks[ia].net, p.board.tracks[ia].layer);
            if p.board.tracks[ib].net != net || p.board.tracks[ib].layer != layer {
                return Err(Error::Invalid(
                    "tracks must share one net and one layer".into(),
                ));
            }
            let ends = |t: &Track| [t.start, t.end];
            let ta = ends(&p.board.tracks[ia]);
            let tb = ends(&p.board.tracks[ib]);
            let shared: Vec<Point> = ta
                .iter()
                .flat_map(|pa| {
                    tb.iter()
                        .filter(move |pb| pb.distance(*pa) <= 1e-6)
                        .copied()
                })
                .collect();
            if shared.len() != 1 {
                return Err(Error::Invalid(
                    "tracks must meet at exactly one endpoint".into(),
                ));
            }
            let corner = shared[0];
            let away = |ends: [Point; 2]| {
                if ends[0].distance(corner) <= 1e-6 {
                    ends[1]
                } else {
                    ends[0]
                }
            };
            let (fa, fb) = (away(ta), away(tb));
            let (la, lb) = (corner.distance(fa), corner.distance(fb));
            if a.size > la.min(lb) * 0.5 {
                return Err(Error::Invalid(
                    "chamfer size must not exceed half the shorter segment".into(),
                ));
            }
            let cut = |far: Point, length: f64| {
                let t = a.size / length;
                Point::new(
                    corner.x + (far.x - corner.x) * t,
                    corner.y + (far.y - corner.y) * t,
                )
            };
            let (p1, p2) = (cut(fa, la), cut(fb, lb));
            if !p1.valid() || !p2.valid() {
                return Err(Error::Invalid("chamfer point is out of range".into()));
            }
            if p1.distance(p2) < 0.01 {
                return Err(Error::Invalid("corner is too shallow to chamfer".into()));
            }
            let width = p.board.tracks[ia].width.min(p.board.tracks[ib].width);
            for (index, target) in [(ia, p1), (ib, p2)] {
                let track = &mut p.board.tracks[index];
                if track.start.distance(corner) <= 1e-6 {
                    track.start = target;
                } else {
                    track.end = target;
                }
            }
            let id = allocate(next)?;
            p.board.tracks.push(Track {
                id,
                net,
                start: p1,
                end: p2,
                width,
                layer,
            });
            Ok(json!({"id": id, "a": a.a, "b": a.b}))
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
            let width = a.width.unwrap_or_else(|| class_width(p, net));
            if net != end_net || start.distance(end) < 1e-6 || !positive(width) {
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
                        width,
                        layer: a.layer,
                    });
                }
            }
            Ok(json!({"ids":ids,"net":net,"width":width}))
        }
        "footprint.mirror" => {
            let a: MirrorParams = parse(params)?;
            let horizontal = match a.axis.as_str() {
                "horizontal" => true,
                "vertical" => false,
                _ => return Err(Error::Invalid("axis must be horizontal or vertical".into())),
            };
            let index = p
                .board
                .footprints
                .iter()
                .position(|f| f.id == a.id)
                .ok_or_else(missing)?;
            let component = p.board.footprints[index].component;
            let mirror = |point: Point| {
                if horizontal {
                    Point::new(-point.x, point.y)
                } else {
                    Point::new(point.x, -point.y)
                }
            };
            let footprint = &mut p.board.footprints[index];
            footprint.rotation = -footprint.rotation;
            for pad in &mut footprint.pads {
                pad.offset = mirror(pad.offset);
                if let Some(geometry) = &mut pad.geometry {
                    geometry.position = mirror(geometry.position);
                    geometry.rotation = -geometry.rotation;
                    if let Some(drill) = &mut geometry.drill {
                        drill.offset = mirror(drill.offset);
                    }
                }
            }
            if let Some(library) = p
                .components
                .iter_mut()
                .find(|c| c.id == component)
                .and_then(|c| c.library_footprint.as_mut())
            {
                for pad in &mut library.definition.pads {
                    pad.position = mirror(pad.position);
                    pad.rotation = -pad.rotation;
                    if let Some(drill) = &mut pad.drill {
                        drill.offset = mirror(drill.offset);
                    }
                }
                for graphic in &mut library.definition.graphics {
                    *graphic = mirror_primitive(graphic.clone(), horizontal);
                }
            }
            Ok(json!({"id": a.id, "axis": a.axis}))
        }
        "align.distribute" => {
            let a: AlignParams = parse(params)?;
            let mode = a.mode.as_str();
            if let Some(filter) = &a.filter {
                validate_filter(filter)?;
            }
            if a.ids.is_empty() && a.filter.is_none() {
                return Err(Error::Invalid("give ids or a filter".into()));
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
            let mut effective_ids = a.ids.clone();
            if let Some(filter) = &a.filter {
                for c in p.components.iter().filter(|c| filter_matches(filter, c)) {
                    if !effective_ids.contains(&c.id) {
                        effective_ids.push(c.id);
                    }
                }
            }
            if effective_ids.len() < 2 {
                return Err(Error::Invalid("alignment needs at least two items".into()));
            }
            let mut seen: Vec<Id> = vec![];
            let mut items: Vec<(Id, bool, f64, f64, f64, f64)> = vec![];
            for id in &effective_ids {
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
            for class in p.net_classes.iter_mut() {
                class.nets.retain(|net| *net != a.id);
            }
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
            p.board.texts.retain(|t| t.id != a.id);
            Ok(json!({"deleted":a.id}))
        }
        "track.set_width" => {
            let a: TrackWidthParams = parse(params)?;
            if !positive(a.width) || a.width > 100.0 {
                return Err(Error::Invalid("width must be within 0 to 100 mm".into()));
            }
            let track = p
                .board
                .tracks
                .iter_mut()
                .find(|t| t.id == a.id)
                .ok_or_else(missing)?;
            track.width = a.width;
            Ok(json!({"id":a.id,"width":a.width}))
        }
        "via.set_size" => {
            let a: ViaSizeParams = parse(params)?;
            if !a.diameter.is_finite() || a.diameter <= 0.0 || a.diameter > 100.0 {
                return Err(Error::Invalid(
                    "via diameter must be within 0 to 100 mm".into(),
                ));
            }
            let via = p
                .board
                .vias
                .iter_mut()
                .find(|v| v.id == a.id)
                .ok_or_else(missing)?;
            let drill = a.drill.unwrap_or(via.drill);
            if !drill.is_finite() || drill < 0.0 || drill >= a.diameter {
                return Err(Error::Invalid(
                    "via drill must be smaller than its pad".into(),
                ));
            }
            via.diameter = a.diameter;
            via.drill = drill;
            Ok(json!({"id":a.id,"diameter":via.diameter,"drill":via.drill}))
        }
        "text.place" => {
            let a: TextPlaceParams = parse(params)?;
            if a.text.is_empty() || a.text.len() > 128 {
                return Err(Error::Invalid("text must be 1 to 128 bytes".into()));
            }
            if !a.position.x.is_finite() || !a.position.y.is_finite() {
                return Err(Error::Invalid("text position must be finite".into()));
            }
            if !a.size.is_finite() || a.size < 0.1 || a.size > 50.0 {
                return Err(Error::Invalid(
                    "text size must be between 0.1 and 50 mm".into(),
                ));
            }
            if p.board.texts.len() >= 256 {
                return Err(Error::Invalid(
                    "a board carries at most 256 text labels".into(),
                ));
            }
            let id = allocate(next)?;
            p.board.texts.push(pcbcraft_doc::BoardText {
                id,
                text: a.text,
                position: a.position,
                size: a.size,
                layer: a.layer,
            });
            Ok(json!({"id":id}))
        }
        "text.move" => {
            let a: TextMoveParams = parse(params)?;
            if !a.position.x.is_finite() || !a.position.y.is_finite() {
                return Err(Error::Invalid("text position must be finite".into()));
            }
            let text = p
                .board
                .texts
                .iter_mut()
                .find(|t| t.id == a.id)
                .ok_or_else(missing)?;
            text.position = a.position;
            Ok(json!({"id":a.id,"position":a.position}))
        }
        "text.set" => {
            let a: TextSetParams = parse(params)?;
            if a.text.is_none() && a.size.is_none() && a.layer.is_none() {
                return Err(Error::Invalid(
                    "provide a text, size, or layer to change".into(),
                ));
            }
            if a.text
                .as_ref()
                .is_some_and(|t| t.is_empty() || t.len() > 128)
            {
                return Err(Error::Invalid("text must be 1 to 128 bytes".into()));
            }
            if a.size
                .is_some_and(|s| !s.is_finite() || s < 0.1 || s > 50.0)
            {
                return Err(Error::Invalid(
                    "text size must be between 0.1 and 50 mm".into(),
                ));
            }
            let text = p
                .board
                .texts
                .iter_mut()
                .find(|t| t.id == a.id)
                .ok_or_else(missing)?;
            if let Some(new) = a.text {
                text.text = new;
            }
            if let Some(size) = a.size {
                text.size = size;
            }
            if let Some(layer) = a.layer {
                text.layer = layer;
            }
            Ok(json!({"id":text.id,"text":text.text,"size":text.size,"layer":text.layer}))
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
    fn alignment_can_select_its_group_by_filter() -> Result<(), Error> {
        let mut s = Session::default();
        for (kind, reference, x, y) in [
            ("resistor", "R1", 10.0, 10.0),
            ("resistor", "R2", 20.0, 14.0),
            ("resistor", "R3", 35.0, 8.0),
            ("capacitor", "C1", 50.0, 30.0),
        ] {
            s.execute(
                "component.add",
                json!({"kind":kind,"position":{"x":x,"y":y},"reference":reference}),
                None,
            )?;
        }
        s.execute(
            "align.distribute",
            json!({"filter":{"reference":"R*"},"mode":"left"}),
            None,
        )?;
        for c in &s.project.components {
            if c.kind == "resistor" {
                assert_eq!(c.position.x, 10.0, "every matched part aligns");
            } else {
                assert_eq!(c.position.x, 50.0, "the unmatched part stays");
            }
        }
        s.undo()?;
        assert_eq!(
            s.project
                .components
                .iter()
                .find(|c| c.reference == "R2")
                .ok_or_else(missing)?
                .position
                .x,
            20.0
        );
        s.execute(
            "align.distribute",
            json!({"filter":{"reference":"*1"},"mode":"top"}),
            None,
        )?;
        let ys: Vec<f64> = s
            .project
            .components
            .iter()
            .filter(|c| c.reference == "R1" || c.reference == "C1")
            .map(|c| c.position.y)
            .collect();
        assert_eq!(ys[0], ys[1], "wildcard across kinds aligns both");
        assert_ne!(
            s.project
                .components
                .iter()
                .find(|c| c.reference == "R2")
                .ok_or_else(missing)?
                .position
                .y,
            ys[0],
            "unmatched parts stay put"
        );
        for params in [
            json!({"mode":"left"}),
            json!({"filter":{},"mode":"left"}),
            json!({"filter":{"reference":"R1"},"mode":"left"}),
            json!({"filter":{"reference":""},"mode":"left"}),
            json!({"filter":{"kind":""},"mode":"left"}),
            json!({"filter":{"kind":"capacitor"},"mode":"left"}),
            json!({"filter":{"reference":"R*"},"mode":"left","extra":1}),
        ] {
            assert!(s.execute("align.distribute", params, None).is_err());
        }
        let r2_x = s
            .project
            .components
            .iter()
            .find(|c| c.reference == "R2")
            .ok_or_else(missing)?
            .position
            .x;
        assert_eq!(r2_x, 20.0, "rejections must not move anything");
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
    fn moving_a_zone_shifts_outline_and_cutouts_together() -> Result<(), Error> {
        let mut s = Session::default();
        let net = s.execute("net.create", json!({"name":"GND"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let zone = s.execute(
            "zone.place",
            json!({"net":net,"layer":"F.Cu","outline":[{"x":10.0,"y":10.0},{"x":40.0,"y":10.0},{"x":40.0,"y":30.0},{"x":10.0,"y":10.0}]}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        s.execute(
            "zone.cutout",
            json!({"zone":zone,"outline":[{"x":20.0,"y":15.0},{"x":25.0,"y":15.0},{"x":25.0,"y":20.0},{"x":20.0,"y":15.0}]}),
            None,
        )?;
        s.execute(
            "zone.move",
            json!({"id":zone,"delta":{"x":5.0,"y":5.0}}),
            None,
        )?;
        assert_eq!(s.project.board.zones[0].outline[1], Point::new(45.0, 15.0));
        assert_eq!(
            s.project.board.zones[0].cutouts[0][0],
            Point::new(25.0, 20.0)
        );
        assert!(s.project.validate().is_ok());
        for params in [
            json!({"id":zone,"delta":{"x":1e6,"y":0.0}}),
            json!({"id":9999,"delta":{"x":1.0,"y":1.0}}),
            json!({"id":zone,"delta":{"x":1.0,"y":1.0},"extra":1}),
        ] {
            assert!(s.execute("zone.move", params, None).is_err());
        }
        assert_eq!(s.project.board.zones[0].outline[1], Point::new(45.0, 15.0));
        s.undo()?;
        assert_eq!(s.project.board.zones[0].outline[1], Point::new(40.0, 10.0));
        assert_eq!(
            s.project.board.zones[0].cutouts[0][0],
            Point::new(20.0, 15.0)
        );
        Ok(())
    }
    #[test]
    fn mirroring_symbols_and_footprints_flips_geometry_exactly() -> Result<(), Error> {
        let mut s = Session::default();
        let r1 = s.execute(
            "component.add",
            json!({"kind":"resistor","position":{"x":20.0,"y":15.0},"reference":"R1"}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let before: Vec<Point> = s
            .project
            .component(r1)
            .ok_or_else(missing)?
            .pins
            .iter()
            .map(|pin| pin.offset)
            .collect();
        s.execute("component.rotate", json!({"id":r1,"degrees":90.0}), None)?;
        s.execute(
            "component.mirror",
            json!({"id":r1,"axis":"horizontal"}),
            None,
        )?;
        let mirrored = s.project.component(r1).ok_or_else(missing)?.clone();
        assert_eq!(mirrored.rotation, -90.0);
        for (original, now) in before
            .iter()
            .zip(mirrored.pins.iter().map(|pin| pin.offset))
        {
            assert_eq!(now, Point::new(-original.x, original.y));
        }
        assert!(s.project.validate().is_ok());
        s.execute(
            "component.mirror",
            json!({"id":r1,"axis":"horizontal"}),
            None,
        )?;
        let restored = s.project.component(r1).ok_or_else(missing)?.clone();
        assert_eq!(restored.rotation, 90.0);
        for (original, now) in before
            .iter()
            .zip(restored.pins.iter().map(|pin| pin.offset))
        {
            assert_eq!(now, *original);
        }
        s.execute(
            "component.place_library",
            json!({"path":library_fixture("device.kicad_sym"),"symbol":"R","position":{"x":50.0,"y":40.0}}),
            None,
        )?;
        let placed = s.project.components.last().ok_or_else(missing)?.id;
        let outline_before = s
            .project
            .component(placed)
            .ok_or_else(missing)?
            .drawing_points();
        s.execute(
            "component.mirror",
            json!({"id":placed,"axis":"horizontal"}),
            None,
        )?;
        let outline_after = s
            .project
            .component(placed)
            .ok_or_else(missing)?
            .drawing_points();
        assert_eq!(
            outline_after,
            outline_before
                .iter()
                .map(|point| Point::new(-point.x, point.y))
                .collect::<Vec<_>>()
        );
        assert!(s.project.validate().is_ok());
        s.execute("board.sync", json!({}), None)?;
        let footprint = s
            .project
            .board
            .footprints
            .iter()
            .find(|f| f.component == r1)
            .cloned()
            .ok_or_else(missing)?;
        s.execute(
            "footprint.mirror",
            json!({"id":footprint.id,"axis":"vertical"}),
            None,
        )?;
        let flipped = s
            .project
            .footprint(footprint.id)
            .ok_or_else(missing)?
            .clone();
        for (original, now) in footprint.pads.iter().zip(flipped.pads.iter()) {
            assert_eq!(
                now.offset,
                Point::new(original.offset.x, -original.offset.y)
            );
        }
        s.execute(
            "footprint.mirror",
            json!({"id":footprint.id,"axis":"vertical"}),
            None,
        )?;
        assert_eq!(
            s.project.footprint(footprint.id).ok_or_else(missing)?.pads,
            footprint.pads,
            "two mirrors cancel exactly"
        );
        assert!(s.project.validate().is_ok());
        for params in [
            json!({"id":9999,"axis":"horizontal"}),
            json!({"id":r1,"axis":"diagonal"}),
            json!({"id":r1,"axis":"horizontal","extra":1}),
        ] {
            assert!(s.execute("component.mirror", params.clone(), None).is_err());
            assert!(s.execute("footprint.mirror", params, None).is_err());
        }
        assert_eq!(
            s.project
                .component(placed)
                .ok_or_else(missing)?
                .drawing_points(),
            outline_after
        );
        Ok(())
    }
    #[test]
    fn chamfering_a_track_corner_replaces_it_with_a_diagonal() -> Result<(), Error> {
        let mut s = Session::default();
        let net = s.execute("net.create", json!({"name":"SIG"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let a = s.execute(
            "track.add",
            json!({"net":net,"start":{"x":0.0,"y":10.0},"end":{"x":10.0,"y":10.0}}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let b = s.execute(
            "track.add",
            json!({"net":net,"start":{"x":10.0,"y":10.0},"end":{"x":10.0,"y":20.0}}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let other = s.execute("net.create", json!({"name":"OTHER"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let d = s.execute(
            "track.add",
            json!({"net":other,"start":{"x":0.0,"y":0.0},"end":{"x":10.0,"y":0.0}}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        for params in [
            json!({"a":a,"b":b,"size":0.0}),
            json!({"a":a,"b":b,"size":6.0}),
            json!({"a":a,"b":a,"size":1.0}),
            json!({"a":a,"b":9999,"size":1.0}),
            json!({"a":a,"b":b,"size":1.0,"extra":1}),
            json!({"a":a,"b":d,"size":1.0}),
            json!({"a":b,"b":d,"size":1.0}),
        ] {
            assert!(s.execute("track.chamfer", params, None).is_err());
        }
        assert_eq!(s.project.board.tracks.len(), 3);
        let c = s.execute("track.chamfer", json!({"a":a,"b":b,"size":2.0}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let tracks = &s.project.board.tracks;
        assert_eq!(tracks.len(), 4);
        let find = |id| {
            tracks
                .iter()
                .find(|t| t.id == id)
                .cloned()
                .ok_or_else(missing)
        };
        assert_eq!(find(a)?.end, Point::new(8.0, 10.0));
        assert_eq!(find(a)?.start, Point::new(0.0, 10.0));
        assert_eq!(find(b)?.start, Point::new(10.0, 12.0));
        assert_eq!(find(b)?.end, Point::new(10.0, 20.0));
        let bridge = find(c)?;
        assert_eq!(bridge.net, net);
        assert_eq!(bridge.start, Point::new(8.0, 10.0));
        assert_eq!(bridge.end, Point::new(10.0, 12.0));
        assert!(s.project.validate().is_ok());
        s.undo()?;
        assert_eq!(s.project.board.tracks.len(), 3);
        assert_eq!(
            s.project
                .board
                .tracks
                .iter()
                .find(|t| t.id == a)
                .ok_or_else(missing)?
                .end,
            Point::new(10.0, 10.0)
        );
        Ok(())
    }
    #[test]
    fn wire_vertices_can_be_inserted_and_removed() -> Result<(), Error> {
        let mut s = Session::default();
        let net = s.execute("net.create", json!({"name":"N1"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let wire = s.execute(
            "wire.draw",
            json!({"net":net,"points":[{"x":10.0,"y":10.0},{"x":20.0,"y":10.0},{"x":20.0,"y":20.0}]}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        fn points(s: &Session) -> Vec<Point> {
            s.project.drawn_wires[0].points.clone()
        }
        s.execute(
            "wire.add_point",
            json!({"id":wire,"index":1,"position":{"x":15.0,"y":12.0}}),
            None,
        )?;
        assert_eq!(points(&s)[1], Point::new(15.0, 12.0));
        s.execute(
            "wire.add_point",
            json!({"id":wire,"index":4,"position":{"x":25.0,"y":20.0}}),
            None,
        )?;
        assert_eq!(points(&s).len(), 5);
        assert_eq!(points(&s)[4], Point::new(25.0, 20.0));
        s.execute(
            "wire.add_point",
            json!({"id":wire,"index":0,"position":{"x":5.0,"y":10.0}}),
            None,
        )?;
        assert_eq!(points(&s)[0], Point::new(5.0, 10.0));
        for params in [
            json!({"id":wire,"index":9,"position":{"x":0.0,"y":0.0}}),
            json!({"id":wire,"index":1,"position":{"x":5.0,"y":10.0}}),
            json!({"id":wire,"index":3,"position":{"x":20.0,"y":10.0}}),
            json!({"id":wire,"index":1,"position":{"x":1e6,"y":0.0}}),
            json!({"id":9999,"index":0,"position":{"x":1.0,"y":1.0}}),
            json!({"id":wire,"index":0,"position":{"x":1.0,"y":1.0},"extra":1}),
        ] {
            assert!(s.execute("wire.add_point", params, None).is_err());
        }
        assert_eq!(points(&s).len(), 6);
        s.execute(
            "label.place",
            json!({"kind":"junction","net":net,"position":{"x":20.0,"y":20.0}}),
            None,
        )?;
        let before = points(&s);
        assert!(
            s.execute("wire.remove_point", json!({"id":wire,"index":4}), None)
                .is_err()
        );
        s.execute("wire.remove_point", json!({"id":wire,"index":5}), None)?;
        assert_eq!(
            points(&s),
            vec![before[0], before[1], before[2], before[3], before[4]]
        );
        s.execute("wire.remove_point", json!({"id":wire,"index":0}), None)?;
        s.execute("wire.remove_point", json!({"id":wire,"index":1}), None)?;
        s.execute("wire.remove_point", json!({"id":wire,"index":1}), None)?;
        assert_eq!(points(&s).len(), 2);
        assert!(s.project.validate().is_ok());
        assert!(
            s.execute("wire.remove_point", json!({"id":wire,"index":0}), None)
                .is_err()
        );
        for params in [
            json!({"id":wire,"index":9}),
            json!({"id":9999,"index":0}),
            json!({"id":wire,"index":0,"extra":1}),
        ] {
            assert!(s.execute("wire.remove_point", params, None).is_err());
        }
        assert_eq!(points(&s).len(), 2);
        s.undo()?;
        assert_eq!(points(&s).len(), 3);
        Ok(())
    }
    #[test]
    fn graphics_and_labels_rotate_in_steps() -> Result<(), Error> {
        let mut s = Session::default();
        let line = s.execute(
            "graphic.add",
            json!({"layer":"schematic","shape":"line","points":[{"x":0.0,"y":0.0},{"x":10.0,"y":0.0}],"width":0.2}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        fn graphic(s: &Session, id: u64) -> Vec<Point> {
            s.project
                .graphics
                .iter()
                .find(|g| g.id == id)
                .ok_or_else(missing)
                .map(|g| g.points.clone())
                .unwrap_or_default()
        }
        fn same(s: &Session, id: u64, expected: &[(f64, f64)]) -> bool {
            let points = graphic(s, id);
            points.len() == expected.len()
                && points
                    .iter()
                    .zip(expected)
                    .all(|(p, (x, y))| (p.x - x).abs() < 1e-9 && (p.y - y).abs() < 1e-9)
        }
        s.execute("graphic.rotate", json!({"id":line,"degrees":90.0}), None)?;
        assert!(same(&s, line, &[(5.0, -5.0), (5.0, 5.0)]));
        s.execute("graphic.rotate", json!({"id":line,"degrees":-90.0}), None)?;
        assert!(same(&s, line, &[(0.0, 0.0), (10.0, 0.0)]));
        s.execute(
            "graphic.rotate",
            json!({"id":line,"degrees":180.0,"around":{"x":0.0,"y":0.0}}),
            None,
        )?;
        assert!(same(&s, line, &[(0.0, 0.0), (-10.0, 0.0)]));
        let rect = s.execute(
            "graphic.add",
            json!({"layer":"board","shape":"rect","points":[{"x":0.0,"y":0.0},{"x":10.0,"y":2.0}],"width":0.2}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        s.execute("graphic.rotate", json!({"id":rect,"degrees":90.0}), None)?;
        assert!(same(&s, rect, &[(6.0, -4.0), (4.0, 6.0)]));
        assert!(s.project.validate().is_ok());
        for params in [
            json!({"id":line,"degrees":45.0}),
            json!({"id":line,"degrees":0.0}),
            json!({"id":line,"degrees":90.0,"around":{"x":1e6,"y":0.0}}),
            json!({"id":9999,"degrees":90.0}),
            json!({"id":line,"degrees":90.0,"extra":1}),
        ] {
            assert!(s.execute("graphic.rotate", params, None).is_err());
        }
        assert!(same(&s, line, &[(0.0, 0.0), (-10.0, 0.0)]));
        let net = s.execute("net.create", json!({"name":"N2"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let label = s.execute(
            "label.place",
            json!({"kind":"label","net":net,"position":{"x":30.0,"y":30.0}}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        s.execute("label.rotate", json!({"id":label,"degrees":45.0}), None)?;
        assert_eq!(s.project.labels[0].rotation, 45.0);
        s.execute("label.rotate", json!({"id":label,"degrees":-90.0}), None)?;
        assert_eq!(s.project.labels[0].rotation, 315.0);
        for params in [
            json!({"id":9999,"degrees":90.0}),
            json!({"id":label,"degrees":1.0,"extra":2}),
        ] {
            assert!(s.execute("label.rotate", params, None).is_err());
        }
        s.undo()?;
        assert_eq!(s.project.labels[0].rotation, 45.0);
        Ok(())
    }
    #[test]
    fn renaming_references_and_nets_is_validated_unique() -> Result<(), Error> {
        let mut s = Session::default();
        let r1 = s.execute(
            "component.add",
            json!({"kind":"resistor","position":{"x":10.0,"y":10.0},"reference":"R1"}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let r2 = s.execute(
            "component.add",
            json!({"kind":"resistor","position":{"x":30.0,"y":10.0},"reference":"R2"}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        s.execute(
            "component.reference",
            json!({"id":r1,"reference":"RV1"}),
            None,
        )?;
        assert_eq!(
            s.project.component(r1).ok_or_else(missing)?.reference,
            "RV1"
        );
        assert_eq!(s.project.component(r2).ok_or_else(missing)?.reference, "R2");
        assert!(s.project.validate().is_ok());
        let long = "R".repeat(65);
        for params in [
            json!({"id":r1,"reference":"R2"}),
            json!({"id":r1,"reference":""}),
            json!({"id":r1,"reference":long}),
            json!({"id":9999,"reference":"R9"}),
            json!({"id":r1,"reference":"R9","extra":1}),
        ] {
            assert!(s.execute("component.reference", params, None).is_err());
        }
        assert_eq!(
            s.project.component(r1).ok_or_else(missing)?.reference,
            "RV1"
        );
        s.execute(
            "component.reference",
            json!({"id":r1,"reference":"R1"}),
            None,
        )?;
        let a = s.execute("net.create", json!({"name":"A"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let b = s.execute("net.create", json!({"name":"B"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        s.execute("net.rename", json!({"id":a,"name":"VCC"}), None)?;
        assert_eq!(s.project.nets[0].name, "VCC");
        assert_eq!(s.project.nets[1].name, "B");
        assert!(s.project.validate().is_ok());
        let long = "N".repeat(129);
        for params in [
            json!({"id":a,"name":"B"}),
            json!({"id":a,"name":""}),
            json!({"id":a,"name":long}),
            json!({"id":9999,"name":"Z"}),
            json!({"id":a,"name":"Z","extra":1}),
        ] {
            assert!(s.execute("net.rename", params, None).is_err());
        }
        assert_eq!(s.project.nets[0].name, "VCC");
        assert_eq!(b, a + 1);
        s.undo()?;
        assert_eq!(s.project.nets[0].name, "A");
        Ok(())
    }
    #[test]
    fn importing_a_netlist_assigns_pins_by_reference() -> Result<(), Error> {
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
            json!({"kind":"resistor","position":{"x":30.0,"y":10.0},"reference":"R2"}),
            None,
        )?;
        let result = s.execute(
            "netlist.import",
            json!({"nets":[{"name":"PWR","pins":[{"reference":"R1","pin":"1"},{"reference":"R2","pin":"1"}]},{"name":"GND","pins":[{"reference":"R1","pin":"2"},{"reference":"R2","pin":"2"}]}]}),
            None,
        )?["result"]
            .clone();
        assert_eq!(result["created"], 2);
        assert_eq!(result["pins"], 4);
        assert_eq!(s.project.nets.len(), 2);
        fn net_id(s: &Session, name: &str) -> Option<u64> {
            s.project.nets.iter().find(|n| n.name == name).map(|n| n.id)
        }
        fn pin_net(s: &Session, component: u64, pin: &str) -> Option<u64> {
            s.project
                .component(component)?
                .pins
                .iter()
                .find(|p| p.number == pin)?
                .net
        }
        assert_eq!(pin_net(&s, r1, "1"), net_id(&s, "PWR"));
        assert_eq!(pin_net(&s, r1, "2"), net_id(&s, "GND"));
        assert!(s.project.validate().is_ok());
        let again = s.execute(
            "netlist.import",
            json!({"nets":[{"name":"PWR","pins":[{"reference":"R1","pin":"1"}]}]}),
            None,
        )?["result"]
            .clone();
        assert_eq!(again["created"], 0);
        assert_eq!(again["pins"], 1);
        for params in [
            json!({"nets":[]}),
            json!({"nets":[{"name":"X","pins":[]}]}),
            json!({"nets":[{"name":"","pins":[{"reference":"R1","pin":"1"}]}]}),
            json!({"nets":[{"name":"X","pins":[{"reference":"R9","pin":"1"}]}]}),
            json!({"nets":[{"name":"X","pins":[{"reference":"R1","pin":"9"}]}]}),
            json!({"nets":[{"name":"A","pins":[{"reference":"R1","pin":"1"}]},{"name":"B","pins":[{"reference":"R1","pin":"1"}]}]}),
            json!({"nets":[{"name":"OTHER","pins":[{"reference":"R1","pin":"1"}]}]}),
            json!({"nets":[{"name":"X","pins":[{"reference":"R1","pin":"1","extra":1}]}]}),
        ] {
            assert!(s.execute("netlist.import", params, None).is_err());
        }
        assert_eq!(s.project.nets.len(), 2);
        assert_eq!(pin_net(&s, r1, "1"), net_id(&s, "PWR"));
        assert_eq!(pin_net(&s, r1, "2"), net_id(&s, "GND"));
        s.undo()?;
        s.undo()?;
        assert!(s.project.nets.is_empty());
        assert_eq!(pin_net(&s, r1, "1"), None);
        Ok(())
    }
    #[test]
    fn exporting_a_netlist_lists_pins_by_reference() -> Result<(), Error> {
        let mut s = Session::default();
        let r1 = s.execute(
            "component.add",
            json!({"kind":"resistor","position":{"x":10.0,"y":10.0},"reference":"R1"}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let r2 = s.execute(
            "component.add",
            json!({"kind":"resistor","position":{"x":30.0,"y":10.0},"reference":"R2"}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        s.execute(
            "netlist.import",
            json!({"nets":[{"name":"PWR","pins":[{"reference":"R2","pin":"1"},{"reference":"R1","pin":"1"}]},{"name":"GND","pins":[{"reference":"R1","pin":"2"}]}]}),
            None,
        )?;
        let before = s.project.clone();
        let result = s.execute("netlist.export", json!({}), None)?["result"].clone();
        assert_eq!(result["nets"].as_array().ok_or_else(missing)?.len(), 2);
        assert_eq!(result["pins"], 3);
        assert_eq!(result["nets"][0]["name"], "GND");
        assert_eq!(result["nets"][1]["pins"][0]["reference"], "R1");
        assert_eq!(result["nets"][1]["pins"][1]["reference"], "R2");
        assert_eq!(
            result["unconnected"].as_array().ok_or_else(missing)?.len(),
            1
        );
        assert_eq!(result["unconnected"][0]["reference"], "R2");
        assert_eq!(result["unconnected"][0]["pin"], "2");
        assert_eq!(s.project, before);
        let pwr = s
            .project
            .nets
            .iter()
            .find(|n| n.name == "PWR")
            .ok_or_else(missing)?
            .id;
        let filtered = s.execute("netlist.export", json!({"net": pwr}), None)?["result"].clone();
        assert_eq!(filtered["nets"].as_array().ok_or_else(missing)?.len(), 1);
        assert_eq!(filtered["nets"][0]["name"], "PWR");
        assert_eq!(filtered["pins"], 2);
        s.execute(
            "pin.no_connect",
            json!({"pin":{"component":r2,"pin":"2"}}),
            None,
        )?;
        let flagged = s.execute("netlist.export", json!({}), None)?["result"].clone();
        assert!(
            flagged["unconnected"]
                .as_array()
                .ok_or_else(missing)?
                .is_empty()
        );
        for params in [json!({"net": 9999}), json!({"extra": 1})] {
            assert!(s.execute("netlist.export", params, None).is_err());
        }
        let after = s.project.clone();
        s.execute("netlist.export", json!({}), None)?;
        assert_eq!(s.project, after);
        assert!(
            s.project.component(r1).ok_or_else(missing)?.pins[0]
                .net
                .is_some()
        );
        Ok(())
    }
    #[test]
    fn pad_tables_report_nets_geometry_and_drills() -> Result<(), Error> {
        let mut s = Session::default();
        let r1 = s.execute(
            "component.add",
            json!({"kind":"resistor","position":{"x":10.0,"y":10.0},"reference":"R1"}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let r2 = s.execute(
            "component.add",
            json!({"kind":"resistor","position":{"x":30.0,"y":10.0},"reference":"R2"}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        s.execute(
            "netlist.import",
            json!({"nets":[{"name":"PWR","pins":[{"reference":"R2","pin":"1"},{"reference":"R1","pin":"1"}]}]}),
            None,
        )?;
        s.execute("board.sync", json!({}), None)?;
        let result = s.execute("pad.table", json!({}), None)?["result"].clone();
        assert_eq!(result["count"], 4);
        assert_eq!(result["unassigned"], 2);
        let pads = result["pads"].as_array().ok_or_else(missing)?;
        assert_eq!(pads[0]["reference"], "R1");
        assert_eq!(pads[0]["pad"], "1");
        assert_eq!(pads[0]["net"], "PWR");
        assert_eq!(pads[1]["reference"], "R1");
        assert_eq!(pads[1]["pad"], "2");
        assert_eq!(pads[1]["net"], json!(null));
        assert_eq!(pads[0]["shape"], "circle");
        assert_eq!(pads[0]["size"]["x"], 1.6);
        assert_eq!(pads[0]["drill"], json!(null));
        let footprint = s.project.board.footprints.first().ok_or_else(missing)?;
        let expected = footprint.pad_position("1").ok_or_else(missing)?;
        let reported = pads[0]["position"].as_object().ok_or_else(missing)?;
        let rx = reported["x"].as_f64().ok_or_else(missing)?;
        let ry = reported["y"].as_f64().ok_or_else(missing)?;
        assert!((rx - expected.x).abs() < 1e-9 && (ry - expected.y).abs() < 1e-9);
        for (component, reference) in [(r1, "R1"), (r2, "R2")] {
            let filtered =
                s.execute("pad.table", json!({"component": component}), None)?["result"].clone();
            assert_eq!(filtered["count"], 2);
            assert_eq!(filtered["pads"][0]["reference"], reference);
            assert_eq!(filtered["pads"][0]["net"], "PWR");
        }
        for params in [json!({"component": 9999}), json!({"extra": 1})] {
            assert!(s.execute("pad.table", params, None).is_err());
        }
        let u = s.execute(
            "component.place_library",
            json!({"path":library_fixture("lm358.kicad_sym"),"symbol":"LM358","position":{"x":20.0,"y":40.0}}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        s.execute(
            "footprint.assign",
            json!({"id":u,"path":library_fixture("DIP-8_W7.62mm.kicad_mod")}),
            None,
        )?;
        s.execute("board.sync", json!({}), None)?;
        let geometry = s.execute("pad.table", json!({"component": u}), None)?["result"].clone();
        assert_eq!(geometry["count"], 8);
        assert_eq!(geometry["unassigned"], 8);
        let pads = geometry["pads"].as_array().ok_or_else(missing)?;
        assert_eq!(
            pads[0]["reference"],
            s.project.component(u).ok_or_else(missing)?.reference
        );
        assert_eq!(pads[0]["shape"], "roundrect");
        assert_eq!(pads[0]["size"]["x"], 1.6);
        assert_eq!(pads[0]["drill"]["x"], 0.8);
        assert_eq!(pads[7]["pad"], "8");
        Ok(())
    }
    #[test]
    fn routed_copper_lengths_are_reported_per_net() -> Result<(), Error> {
        let mut s = Session::default();
        let r1 = s.execute(
            "component.add",
            json!({"kind":"resistor","position":{"x":10.0,"y":10.0},"reference":"R1"}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let r2 = s.execute(
            "component.add",
            json!({"kind":"resistor","position":{"x":30.0,"y":10.0},"reference":"R2"}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        s.execute(
            "netlist.import",
            json!({"nets":[{"name":"PWR","pins":[{"reference":"R2","pin":"1"},{"reference":"R1","pin":"1"}]},{"name":"GND","pins":[{"reference":"R1","pin":"2"}]}]}),
            None,
        )?;
        s.execute("board.sync", json!({}), None)?;
        let before = s.execute("net.lengths", json!({}), None)?["result"].clone();
        assert_eq!(before["routed"], 0);
        assert_eq!(before["unrouted"].as_array().ok_or_else(missing)?.len(), 2);
        let before_total = before["total"].as_f64().ok_or_else(missing)?;
        assert!(before_total.abs() < 1e-9);
        let pwr = s
            .project
            .nets
            .iter()
            .find(|n| n.name == "PWR")
            .ok_or_else(missing)?
            .id;
        let gnd = s
            .project
            .nets
            .iter()
            .find(|n| n.name == "GND")
            .ok_or_else(missing)?
            .id;
        s.execute(
            "track.add",
            json!({"net":pwr,"start":{"x":0.0,"y":0.0},"end":{"x":3.0,"y":0.0}}),
            None,
        )?;
        s.execute(
            "track.add",
            json!({"net":pwr,"start":{"x":0.0,"y":0.0},"end":{"x":0.0,"y":4.0},"layer":"B.Cu"}),
            None,
        )?;
        s.execute(
            "via.place",
            json!({"net":gnd,"position":{"x":1.0,"y":1.0},"diameter":1.5,"drill":0.8}),
            None,
        )?;
        let after = s.execute("net.lengths", json!({}), None)?["result"].clone();
        assert_eq!(after["routed"], 2);
        assert!(after["unrouted"].as_array().ok_or_else(missing)?.is_empty());
        let nets = after["nets"].as_array().ok_or_else(missing)?;
        assert_eq!(nets[0]["name"], "PWR");
        assert_eq!(nets[0]["segments"], 2);
        assert_eq!(nets[0]["vias"], 0);
        let length = nets[0]["length"].as_f64().ok_or_else(missing)?;
        assert!((length - 7.0).abs() < 1e-9);
        assert_eq!(nets[1]["name"], "GND");
        assert_eq!(nets[1]["segments"], 0);
        assert_eq!(nets[1]["vias"], 1);
        let gnd_length = nets[1]["length"].as_f64().ok_or_else(missing)?;
        assert!((gnd_length - 0.0).abs() < 1e-9);
        let total = after["total"].as_f64().ok_or_else(missing)?;
        assert!((total - 7.0).abs() < 1e-9);
        let single = s.execute("net.lengths", json!({"net": gnd}), None)?["result"].clone();
        assert_eq!(single["routed"], 1);
        assert_eq!(single["nets"].as_array().ok_or_else(missing)?.len(), 1);
        assert_eq!(single["nets"][0]["name"], "GND");
        for params in [json!({"net": 9999}), json!({"extra": 1})] {
            assert!(s.execute("net.lengths", params, None).is_err());
        }
        let unchanged = s.project.clone();
        s.execute("net.lengths", json!({"net": pwr}), None)?;
        assert_eq!(s.project, unchanged);
        assert!(r1 > 0 && r2 > 0);
        Ok(())
    }
    #[test]
    fn net_class_width_bounds_flag_member_tracks_only() -> Result<(), Error> {
        let mut s = Session::default();
        let power = s.execute("net.create", json!({"name":"PWR"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let free = s.execute("net.create", json!({"name":"SIG"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let class = s.execute(
            "netclass.add",
            json!({"name":"POWER","clearance":0.3,"width":0.6,"nets":[power]}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        fn widths(s: &Session) -> Vec<String> {
            s.checks()["drc"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .iter()
                .filter(|f| f["code"] == json!("track_width"))
                .map(|f| f["message"].as_str().unwrap_or_default().to_string())
                .collect()
        }
        let thin = s.execute(
            "track.add",
            json!({"net":power,"start":{"x":0.0,"y":0.0},"end":{"x":10.0,"y":0.0},"width":0.2}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let wide = s.execute(
            "track.add",
            json!({"net":power,"start":{"x":0.0,"y":5.0},"end":{"x":10.0,"y":5.0},"width":0.9}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let loose = s.execute(
            "track.add",
            json!({"net":free,"start":{"x":0.0,"y":10.0},"end":{"x":10.0,"y":10.0},"width":0.05}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        assert!(
            widths(&s).is_empty(),
            "a class without width bounds flags nothing"
        );
        s.execute(
            "netclass.width_rules",
            json!({"class":class,"min_width":0.4,"max_width":0.8}),
            None,
        )?;
        let flagged = widths(&s);
        assert_eq!(flagged.len(), 2);
        assert!(
            flagged
                .iter()
                .any(|m| m.contains("at least 0.400 mm") && m.contains("PWR"))
        );
        assert!(
            flagged
                .iter()
                .any(|m| m.contains("at most 0.800 mm") && m.contains("PWR"))
        );
        assert!(
            !flagged.iter().any(|m| m.contains("SIG")),
            "unclassified nets keep their old behaviour"
        );
        s.execute(
            "track.move",
            json!({"id":thin,"delta":{"x":0.0,"y":20.0}}),
            None,
        )?;
        s.execute(
            "track.add",
            json!({"net":power,"start":{"x":0.0,"y":30.0},"end":{"x":10.0,"y":30.0},"width":0.5}),
            None,
        )?;
        let flagged = widths(&s);
        assert_eq!(flagged.len(), 2, "in-bounds copper stays quiet");
        s.execute(
            "netclass.width_rules",
            json!({"class":class,"min_width":0.1}),
            None,
        )?;
        let flagged = widths(&s);
        assert_eq!(flagged.len(), 1);
        assert!(flagged[0].contains("at most 0.800 mm"));
        s.execute(
            "rules.severity",
            json!({"code":"track_width","severity":"off"}),
            None,
        )?;
        assert!(widths(&s).is_empty(), "the new code is silenceable");
        s.execute(
            "rules.severity",
            json!({"code":"track_width","severity":"warning"}),
            None,
        )?;
        let downgraded = s.checks()["drc"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .filter(|f| f["code"] == json!("track_width"))
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(downgraded.len(), 1);
        assert_eq!(downgraded[0]["severity"], "warning");
        assert!(s.project.validate().is_ok());
        let text = serde_json::to_string(&s.project).map_err(|e| Error::Invalid(e.to_string()))?;
        let loaded: Project =
            serde_json::from_str(&text).map_err(|e| Error::Invalid(e.to_string()))?;
        loaded.validate()?;
        assert_eq!(loaded, s.project);
        s.undo()?;
        s.undo()?;
        s.undo()?;
        assert_eq!(
            s.project.net_classes.first().ok_or_else(missing)?.min_width,
            Some(0.4)
        );
        assert!(!s.project.rule_severity.contains_key("track_width"));
        for params in [
            json!({"class":9999,"min_width":0.4}),
            json!({"class":class,"min_width":0.0}),
            json!({"class":class,"max_width":101.0}),
            json!({"class":class,"min_width":0.9,"max_width":0.3}),
            json!({"class":class,"min_width":0.7,"max_width":0.6}),
            json!({"class":class}),
            json!({"class":class,"min_width":0.4,"extra":1}),
        ] {
            assert!(s.execute("netclass.width_rules", params, None).is_err());
        }
        let before = s.inspect();
        assert_eq!(s.inspect(), before);
        assert!(s.project.validate().is_ok());
        assert!(thin > 0 && wide > 0 && loose > 0);
        Ok(())
    }
    #[test]
    fn a_single_net_can_carry_its_own_length_target() -> Result<(), Error> {
        let mut s = Session::default();
        let sig = s.execute("net.create", json!({"name":"SIG"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let diff = s.execute("net.create", json!({"name":"DIFF"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        fn flagged(s: &Session, net: u64) -> Vec<String> {
            s.checks()["drc"].as_array().map_or_else(Vec::new, |a| {
                a.iter()
                    .filter(|f| f["code"] == "track_length" && f["objects"][0] == net)
                    .map(|f| f["message"].as_str().unwrap_or("").to_string())
                    .collect()
            })
        }
        s.execute(
            "track.add",
            json!({"net":sig,"start":{"x":10.0,"y":10.0},"end":{"x":18.0,"y":10.0},"width":0.3}),
            None,
        )?;
        assert!(
            flagged(&s, sig).is_empty(),
            "an untargeted net is never a length finding"
        );
        let out = s.execute(
            "net.set_length",
            json!({"id":sig,"target":12.0,"tolerance":0.5}),
            None,
        )?;
        assert_eq!(out["result"]["target"], 12.0);
        let messages = flagged(&s, sig);
        assert_eq!(messages.len(), 1, "8 mm misses a 12 mm target");
        assert!(
            messages[0].contains("8.000 mm") && messages[0].contains("targets 12.000 mm"),
            "the message names the routed length and the net's own target"
        );
        s.execute(
            "net.set_length",
            json!({"id":sig,"target":12.0,"tolerance":8.0}),
            None,
        )?;
        assert!(
            flagged(&s, sig).is_empty(),
            "a wider tolerance closes the gap"
        );
        let rows = s.execute("net.lengths", json!({"net":sig}), None)?["result"]["nets"]
            .as_array()
            .ok_or_else(missing)?
            .clone();
        assert_eq!(
            rows[0]["target_length"], 12.0,
            "lengths show the net target"
        );
        assert_eq!(rows[0]["length_tolerance"], 8.0);
        s.execute("net.set_length", json!({"id":sig}), None)?;
        assert!(
            flagged(&s, sig).is_empty(),
            "clearing the target stops the findings"
        );
        assert!(
            s.project
                .nets
                .iter()
                .any(|n| n.id == sig && n.target_length.is_none() && n.length_tolerance.is_none()),
            "clearing also drops the tolerance"
        );
        let class = s.execute(
            "netclass.add",
            json!({"name":"USB","clearance":0.2,"width":0.3,"nets":[diff]}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        s.execute(
            "netclass.length_rules",
            json!({"class":class,"target_length":20.0}),
            None,
        )?;
        s.execute(
            "track.add",
            json!({"net":diff,"start":{"x":30.0,"y":10.0},"end":{"x":45.0,"y":10.0},"width":0.3}),
            None,
        )?;
        assert_eq!(
            flagged(&s, diff).len(),
            1,
            "the class target still applies on its own"
        );
        s.execute("net.set_length", json!({"id":diff,"target":15.0}), None)?;
        assert!(
            flagged(&s, diff).is_empty(),
            "the net's own target wins over the class target"
        );
        let before = s.inspect();
        for params in [
            json!({"id":sig,"target":0.0}),
            json!({"id":sig,"target":-4.0}),
            json!({"id":sig,"target":1e9}),
            json!({"id":sig,"target":null,"tolerance":1.0}),
            json!({"id":sig,"tolerance":-1.0}),
            json!({"id":999,"target":12.0}),
            json!({"id":sig,"target":12.0,"extra":1}),
        ] {
            assert!(s.execute("net.set_length", params, None).is_err());
        }
        assert_eq!(s.inspect(), before, "rejections change nothing");
        s.undo()?;
        assert_eq!(
            flagged(&s, diff).len(),
            1,
            "undo hands the net back to its class target"
        );
        Ok(())
    }
    #[test]
    fn net_class_length_targets_flag_member_nets_outside_the_window() -> Result<(), Error> {
        let mut s = Session::default();
        let power = s.execute("net.create", json!({"name":"PWR"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let sig = s.execute("net.create", json!({"name":"SIG"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let class = s.execute(
            "netclass.add",
            json!({"name":"USB","clearance":0.2,"width":0.3,"nets":[power]}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        fn flagged(s: &Session) -> Vec<String> {
            s.checks()["drc"].as_array().map_or_else(Vec::new, |a| {
                a.iter()
                    .filter(|f| f["code"] == "track_length")
                    .map(|f| f["message"].as_str().unwrap_or("").to_string())
                    .collect()
            })
        }
        for (start, end) in [
            (json!({"x":10.0,"y":10.0}), json!({"x":20.0,"y":10.0})),
            (json!({"x":20.0,"y":10.0}), json!({"x":20.0,"y":14.0})),
        ] {
            s.execute(
                "track.add",
                json!({"net":power,"start":start,"end":end,"width":0.3}),
                None,
            )?;
        }
        s.execute(
            "netclass.length_rules",
            json!({"class":class,"target_length":16.0,"length_tolerance":1.0}),
            None,
        )?;
        let messages = flagged(&s);
        assert_eq!(messages.len(), 1, "14 mm misses a 16 mm target");
        assert!(
            messages[0].contains("14.000 mm") && messages[0].contains("targets 16.000 mm"),
            "the message names the routed length and the target"
        );
        s.execute(
            "track.add",
            json!({"net":power,"start":{"x":20.0,"y":14.0},"end":{"x":22.0,"y":14.0},"width":0.3}),
            None,
        )?;
        assert!(flagged(&s).is_empty(), "16 mm lands inside the window");
        s.undo()?;
        assert_eq!(flagged(&s).len(), 1, "undo reopens the gap");
        s.execute(
            "netclass.length_rules",
            json!({"class":class,"length_tolerance":2.0}),
            None,
        )?;
        assert!(
            flagged(&s).is_empty(),
            "a wider tolerance keeps the stored target"
        );
        s.execute("netclass.assign", json!({"net":sig,"class":class}), None)?;
        s.execute(
            "netclass.length_rules",
            json!({"class":class,"length_tolerance":0.5}),
            None,
        )?;
        let messages = flagged(&s);
        assert_eq!(
            messages.len(),
            1,
            "an unrouted member is not a length finding"
        );
        assert!(messages[0].contains("PWR"));
        let rows = s.execute("net.lengths", json!({}), None)?["result"]["nets"]
            .as_array()
            .ok_or_else(missing)?
            .clone();
        let row = rows.iter().find(|n| n["id"] == power).ok_or_else(missing)?;
        assert_eq!(row["target_length"], 16.0, "lengths name the target");
        assert_eq!(row["length_tolerance"], 0.5);
        for params in [
            json!({"class":class,"target_length":0.0}),
            json!({"class":class,"target_length":10001.0}),
            json!({"class":class,"length_tolerance":-0.1}),
            json!({"class":class,"length_tolerance":1001.0}),
            json!({"class":class}),
            json!({"class":999,"target_length":5.0}),
            json!({"class":class,"target_length":5.0,"extra":1}),
        ] {
            assert!(s.execute("netclass.length_rules", params, None).is_err());
        }
        let plain = s.execute(
            "netclass.add",
            json!({"name":"OTHER","clearance":0.2,"width":0.3}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        assert!(
            s.execute(
                "netclass.length_rules",
                json!({"class":plain,"length_tolerance":1.0}),
                None
            )
            .is_err(),
            "a tolerance needs a target"
        );
        assert!(s.project.validate().is_ok());
        let before = s.inspect();
        assert_eq!(s.inspect(), before, "rejections change nothing");
        Ok(())
    }
    #[test]
    fn net_classes_absorb_every_net_matching_a_wildcard_pattern() -> Result<(), Error> {
        let mut s = Session::default();
        let names = ["USB_DP", "USB_DM", "USB_STAT", "FB_IN", "OUT_FB", "usb_low"];
        let mut ids = Vec::new();
        for name in names {
            ids.push(
                s.execute("net.create", json!({"name": name}), None)?["result"]["id"]
                    .as_u64()
                    .ok_or_else(missing)?,
            );
        }
        let power = s.execute(
            "netclass.add",
            json!({"name":"POWER","clearance":0.5,"width":0.6}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let usb = s.execute(
            "netclass.add",
            json!({"name":"USB","clearance":0.3,"width":0.25}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let result = s.execute(
            "netclass.assign_pattern",
            json!({"class":power,"pattern":"*_IN"}),
            None,
        )?["result"]
            .clone();
        assert_eq!(result["matched"], 1);
        fn class_of(s: &Session, net: u64) -> Option<u64> {
            s.project
                .net_classes
                .iter()
                .find(|c| c.nets.contains(&net))
                .map(|c| c.id)
        }
        assert_eq!(class_of(&s, ids[3]), Some(power));
        assert_eq!(class_of(&s, ids[4]), None);
        let result = s.execute(
            "netclass.assign_pattern",
            json!({"class":usb,"pattern":"USB*"}),
            None,
        )?["result"]
            .clone();
        assert_eq!(result["matched"], 3);
        for net in [ids[0], ids[1], ids[2]] {
            assert_eq!(class_of(&s, net), Some(usb));
        }
        assert_eq!(class_of(&s, ids[3]), Some(power));
        assert!(s.project.validate().is_ok());
        s.execute(
            "netclass.assign_pattern",
            json!({"class":power,"pattern":"usb*"}),
            None,
        )?;
        assert_eq!(class_of(&s, ids[5]), Some(power));
        let single = s.execute(
            "netclass.assign_pattern",
            json!({"class":usb,"pattern":"*"}),
            None,
        )?["result"]
            .clone();
        assert_eq!(single["matched"], 6);
        for net in &ids {
            assert_eq!(class_of(&s, *net), Some(usb), "one class holds them all");
        }
        s.execute(
            "netclass.assign_pattern",
            json!({"class":power,"pattern":"USB?DP"}),
            None,
        )?;
        assert_eq!(class_of(&s, ids[0]), Some(power));
        let before = s.inspect();
        for params in [
            json!({"class":power,"pattern":"POWER"}),
            json!({"class":power,"pattern":"NOSUCH*"}),
            json!({"class":9999,"pattern":"*"}),
            json!({"class":power,"pattern":""}),
            json!({"class":power,"pattern":"x".repeat(65)}),
            json!({"class":power,"pattern":"*","extra":1}),
        ] {
            assert!(s.execute("netclass.assign_pattern", params, None).is_err());
        }
        assert_eq!(s.inspect(), before);
        s.undo()?;
        assert_eq!(class_of(&s, ids[0]), Some(usb));
        let text = serde_json::to_string(&s.project).map_err(|e| Error::Invalid(e.to_string()))?;
        let loaded: Project =
            serde_json::from_str(&text).map_err(|e| Error::Invalid(e.to_string()))?;
        loaded.validate()?;
        assert_eq!(loaded, s.project);
        Ok(())
    }
    #[test]
    fn check_codes_can_be_relevelled_and_silenced() -> Result<(), Error> {
        let mut s = Session::default();
        let a = s.execute("net.create", json!({"name":"A"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let b = s.execute("net.create", json!({"name":"B"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        s.execute(
            "track.add",
            json!({"net":a,"start":{"x":10.0,"y":10.0},"end":{"x":30.0,"y":10.0},"width":0.2}),
            None,
        )?;
        s.execute(
            "track.add",
            json!({"net":b,"start":{"x":10.0,"y":10.3},"end":{"x":30.0,"y":10.3},"width":0.2}),
            None,
        )?;
        fn drc(s: &Session) -> Vec<Value> {
            s.checks()["drc"].as_array().cloned().unwrap_or_default()
        }
        let findings = drc(&s);
        let clearance = findings
            .iter()
            .find(|f| f["code"] == json!("clearance"))
            .ok_or_else(missing)?
            .clone();
        assert_eq!(clearance["severity"], "error");
        s.execute(
            "rules.severity",
            json!({"code":"clearance","severity":"warning"}),
            None,
        )?;
        let findings = drc(&s);
        let downgraded = findings
            .iter()
            .find(|f| f["code"] == json!("clearance"))
            .ok_or_else(missing)?
            .clone();
        assert_eq!(downgraded["severity"], "warning");
        assert_eq!(downgraded["message"], clearance["message"]);
        s.execute(
            "rules.severity",
            json!({"code":"clearance","severity":"off"}),
            None,
        )?;
        assert!(
            drc(&s).iter().all(|f| f["code"] != json!("clearance")),
            "a silenced code must not be reported"
        );
        s.execute("rules.severity", json!({"code":"clearance"}), None)?;
        let findings = drc(&s);
        assert_eq!(
            findings
                .iter()
                .find(|f| f["code"] == json!("clearance"))
                .ok_or_else(missing)?["severity"],
            "error"
        );
        let single_pin = {
            let erc = s.checks()["erc"].as_array().ok_or_else(missing)?.clone();
            erc.iter()
                .find(|f| f["code"] == json!("single_pin_net"))
                .cloned()
                .ok_or_else(missing)?
        };
        assert_eq!(single_pin["severity"], "warning");
        s.execute(
            "rules.severity",
            json!({"code":"single_pin_net","severity":"error"}),
            None,
        )?;
        let erc = s.checks()["erc"].as_array().ok_or_else(missing)?.clone();
        assert_eq!(
            erc.iter()
                .find(|f| f["code"] == json!("single_pin_net"))
                .ok_or_else(missing)?["severity"],
            "error"
        );
        assert!(s.project.validate().is_ok());
        let text = serde_json::to_string(&s.project).map_err(|e| Error::Invalid(e.to_string()))?;
        let loaded: Project =
            serde_json::from_str(&text).map_err(|e| Error::Invalid(e.to_string()))?;
        loaded.validate()?;
        assert_eq!(loaded, s.project);
        s.undo()?;
        assert!(!s.project.rule_severity.contains_key("single_pin_net"));
        let before = s.inspect();
        for params in [
            json!({"code":"","severity":"warning"}),
            json!({"code":"x".repeat(33),"severity":"warning"}),
            json!({"code":"clearance","severity":"fatal"}),
            json!({"code":"nope"}),
            json!({"code":"clearance","severity":"warning","extra":1}),
        ] {
            assert!(s.execute("rules.severity", params, None).is_err());
        }
        assert_eq!(s.inspect(), before);
        assert!(s.project.validate().is_ok());
        assert!(a > 0 && b > 0);
        Ok(())
    }
    #[test]
    fn check_findings_can_be_excluded_per_object_and_restored() -> Result<(), Error> {
        let mut s = Session::default();
        let a = s.execute("net.create", json!({"name":"A"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let b = s.execute("net.create", json!({"name":"B"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        s.execute(
            "netclass.add",
            json!({"name":"POWER","clearance":0.5,"width":0.6,"nets":[a]}),
            None,
        )?;
        let mut track_ids = vec![];
        for (net, y) in [(a, 10.0), (b, 10.5)] {
            let out = s.execute(
                "track.add",
                json!({"net":net,"start":{"x":10.0,"y":y},"end":{"x":30.0,"y":y},"width":0.2}),
                None,
            )?;
            track_ids.push(out["result"]["id"].as_u64().ok_or_else(missing)?);
        }
        fn clearance(s: &Session) -> Result<usize, Error> {
            Ok(s.checks()["drc"]
                .as_array()
                .ok_or_else(missing)?
                .iter()
                .filter(|f| f["code"] == "clearance")
                .count())
        }
        assert_eq!(clearance(&s)?, 1);
        s.execute(
            "rules.exclude",
            json!({"code":"clearance","ids":[track_ids[0]]}),
            None,
        )?;
        assert_eq!(clearance(&s)?, 1, "one of two objects is not enough");
        s.execute(
            "rules.exclude",
            json!({"code":"clearance","ids":[track_ids[1]]}),
            None,
        )?;
        assert_eq!(clearance(&s)?, 0, "every named object excluded");
        s.undo()?;
        assert_eq!(clearance(&s)?, 1, "undo brings the finding back");
        s.redo()?;
        assert_eq!(clearance(&s)?, 0);
        s.execute(
            "rules.include",
            json!({"code":"clearance","ids":[track_ids[1]]}),
            None,
        )?;
        assert_eq!(clearance(&s)?, 1);
        s.execute(
            "rules.exclude",
            json!({"code":"clearance","ids":[track_ids[1]]}),
            None,
        )?;
        assert_eq!(clearance(&s)?, 0);
        s.execute("rules.include", json!({"code":"clearance"}), None)?;
        assert_eq!(
            clearance(&s)?,
            1,
            "lifting every exclusion restores the finding"
        );
        assert!(s.project.rule_exclusions.is_empty());
        s.execute(
            "rules.exclude",
            json!({"code":"clearance","ids":[track_ids[0]]}),
            None,
        )?;
        let before = s.inspect();
        for params in [
            json!({"code":"","ids":[track_ids[1]]}),
            json!({"code":"clearance","ids":[]}),
            json!({"code":"clearance","ids":[9999]}),
            json!({"code":"clearance","ids":[track_ids[0]]}),
            json!({"code":"clearance","ids":[track_ids[1]],"extra":1}),
            json!({"code":"clearance"}),
        ] {
            assert!(s.execute("rules.exclude", params, None).is_err());
        }
        for params in [
            json!({"code":"clearance","ids":[track_ids[1]]}),
            json!({"code":"nothing_here"}),
            json!({"code":""}),
            json!({"code":"clearance","ids":[]}),
            json!({"code":"clearance","extra":1}),
        ] {
            assert!(s.execute("rules.include", params, None).is_err());
        }
        assert_eq!(s.inspect(), before, "rejections change nothing");
        assert!(s.project.validate().is_ok());
        s.undo()?;
        assert!(s.project.rule_exclusions.is_empty());
        Ok(())
    }
    #[test]
    fn net_classes_drive_clearance_and_default_track_width() -> Result<(), Error> {
        let mut s = Session::default();
        let a = s.execute("net.create", json!({"name":"A"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let b = s.execute("net.create", json!({"name":"B"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        for (net, y) in [(a, 10.0), (b, 10.5)] {
            s.execute(
                "track.add",
                json!({"net":net,"start":{"x":10.0,"y":y},"end":{"x":30.0,"y":y},"width":0.2}),
                None,
            )?;
        }
        fn drc(s: &Session) -> Result<Vec<Value>, Error> {
            Ok(s.checks()["drc"].as_array().ok_or_else(missing)?.clone())
        }
        assert!(!drc(&s)?.iter().any(|f| f["code"] == "clearance"));
        let power = s.execute(
            "netclass.add",
            json!({"name":"POWER","clearance":0.5,"width":0.6,"nets":[a]}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        assert!(
            drc(&s)?.iter().any(|f| f["code"] == "clearance"
                && f["message"]
                    .as_str()
                    .is_some_and(|m| m.contains("0.500 mm required"))),
            "the class clearance raises the requirement"
        );
        let added = s.execute(
            "track.add",
            json!({"net":a,"start":{"x":10.0,"y":20.0},"end":{"x":20.0,"y":20.0}}),
            None,
        )?["result"]
            .clone();
        assert_eq!(added["width"], 0.6);
        let explicit = s.execute(
            "track.add",
            json!({"net":b,"start":{"x":10.0,"y":25.0},"end":{"x":20.0,"y":25.0},"width":0.2}),
            None,
        )?["result"]
            .clone();
        assert_eq!(explicit["width"], 0.2);
        let logic = s.execute(
            "netclass.add",
            json!({"name":"LOGIC","clearance":0.1,"width":0.25}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        s.execute("netclass.assign", json!({"net": b, "class": logic}), None)?;
        assert!(drc(&s)?.iter().any(|f| f["code"] == "clearance"));
        s.execute("netclass.assign", json!({"net": a, "class": null}), None)?;
        assert!(!drc(&s)?.iter().any(|f| f["code"] == "clearance"));
        s.execute("netclass.assign", json!({"net": b, "class": power}), None)?;
        assert_eq!(
            s.project
                .net_classes
                .iter()
                .filter(|k| k.nets.contains(&b))
                .count(),
            1
        );
        assert_eq!(s.project.net_classes[0].nets.len(), 1);
        assert!(s.project.net_classes[1].nets.is_empty());
        let before = s.inspect();
        for params in [
            json!({"name":"POWER","clearance":0.4,"width":0.3}),
            json!({"name":"","clearance":0.4,"width":0.3}),
            json!({"name":"X","clearance":0.0,"width":0.3}),
            json!({"name":"X","clearance":0.4,"width":0.0}),
            json!({"name":"X","clearance":0.4,"width":0.3,"nets":[9999]}),
            json!({"name":"X","clearance":0.4,"width":0.3,"nets":[b]}),
            json!({"name":"X","clearance":0.4,"width":0.3,"extra":1}),
            json!({"net": 9999, "class": power}),
            json!({"net": b, "class": 9999}),
            json!({"net": b, "class": power, "extra": 1}),
        ] {
            let command = if params.get("class").is_some() {
                "netclass.assign"
            } else {
                "netclass.add"
            };
            assert!(s.execute(command, params, None).is_err());
        }
        assert_eq!(s.inspect(), before);
        let c = s.execute("net.create", json!({"name":"C"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        s.execute("netclass.assign", json!({"net": c, "class": logic}), None)?;
        assert!(s.project.net_classes[1].nets.contains(&c));
        s.execute("object.delete", json!({"id": c}), None)?;
        assert!(s.project.net_classes.iter().all(|k| !k.nets.contains(&c)));
        assert!(s.project.validate().is_ok());
        s.undo()?;
        assert!(s.project.net_classes[1].nets.contains(&c));
        assert!(s.project.validate().is_ok());
        Ok(())
    }
    #[test]
    fn component_fields_are_set_replaced_cleared_and_copied() -> Result<(), Error> {
        let mut s = Session::default();
        let r1 = s.execute(
            "component.add",
            json!({"kind":"resistor","position":{"x":10.0,"y":10.0},"reference":"R1"}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        fn field(s: &Session, id: u64, key: &str) -> Option<String> {
            s.project.component(id)?.field(key).map(str::to_string)
        }
        s.execute(
            "component.field",
            json!({"id":r1,"key":"Datasheet","value":"ds1.pdf"}),
            None,
        )?;
        assert_eq!(field(&s, r1, "Datasheet").as_deref(), Some("ds1.pdf"));
        s.execute(
            "component.field",
            json!({"id":r1,"key":"Datasheet","value":"ds2.pdf"}),
            None,
        )?;
        assert_eq!(field(&s, r1, "Datasheet").as_deref(), Some("ds2.pdf"));
        s.execute(
            "component.field",
            json!({"id":r1,"key":"Manufacturer","value":"Acme"}),
            None,
        )?;
        assert_eq!(s.project.component(r1).ok_or_else(missing)?.fields.len(), 2);
        assert!(s.project.validate().is_ok());
        let copy = s.execute(
            "object.duplicate",
            json!({"ids":[r1],"offset":{"x":0.0,"y":10.0}}),
            None,
        )?["result"]["duplicates"][0]["component"]
            .as_u64()
            .ok_or_else(missing)?;
        assert_eq!(field(&s, copy, "Datasheet").as_deref(), Some("ds2.pdf"));
        assert_ne!(
            s.project.component(copy).ok_or_else(missing)?.reference,
            s.project.component(r1).ok_or_else(missing)?.reference
        );
        s.execute(
            "component.field",
            json!({"id":r1,"key":"Datasheet","value":null}),
            None,
        )?;
        assert_eq!(field(&s, r1, "Datasheet"), None);
        assert_eq!(s.project.component(r1).ok_or_else(missing)?.fields.len(), 1);
        s.execute(
            "component.field",
            json!({"id":r1,"key":"Part","value":""}),
            None,
        )?;
        assert_eq!(field(&s, r1, "Part").as_deref(), Some(""));
        for i in 0..30 {
            s.execute(
                "component.field",
                json!({"id":copy,"key":format!("note{i}"),"value":"v"}),
                None,
            )?;
        }
        assert_eq!(
            s.project.component(copy).ok_or_else(missing)?.fields.len(),
            32
        );
        let before = s.inspect();
        for params in [
            json!({"id":copy,"key":"note30","value":"v"}),
            json!({"id":r1,"key":""}),
            json!({"id":r1,"key":"x".repeat(65),"value":"v"}),
            json!({"id":r1,"key":"k","value":"x".repeat(257)}),
            json!({"id":9999,"key":"k","value":"v"}),
            json!({"id":r1,"key":"Nope"}),
            json!({"id":r1,"key":"k","value":"v","extra":1}),
        ] {
            assert!(s.execute("component.field", params, None).is_err());
        }
        assert_eq!(s.inspect(), before);
        s.execute(
            "component.field",
            json!({"id":r1,"key":"Manufacturer","value":"Globemans"}),
            None,
        )?;
        s.undo()?;
        assert_eq!(field(&s, r1, "Manufacturer").as_deref(), Some("Acme"));
        let text = serde_json::to_string(&s.project).map_err(|e| Error::Invalid(e.to_string()))?;
        let loaded: Project =
            serde_json::from_str(&text).map_err(|e| Error::Invalid(e.to_string()))?;
        loaded.validate()?;
        assert_eq!(loaded, s.project);
        Ok(())
    }
    #[test]
    fn components_can_be_found_by_reference_pattern_value_and_kind() -> Result<(), Error> {
        let mut s = Session::default();
        for (kind, reference, value) in [
            ("resistor", "R1", "1k"),
            ("resistor", "R2", "10k"),
            ("resistor", "RV1", "1k"),
            ("capacitor", "C1", "100n"),
        ] {
            s.execute(
                "component.add",
                json!({"kind":kind,"position":{"x":10.0,"y":10.0},"reference":reference,"value":value}),
                None,
            )?;
        }
        let r1 = s
            .project
            .components
            .iter()
            .find(|c| c.reference == "R1")
            .ok_or_else(missing)?
            .id;
        s.execute("board.sync", json!({}), None)?;
        fn references(found: &Value) -> Vec<String> {
            found["components"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .iter()
                .map(|c| c["reference"].as_str().unwrap_or_default().to_string())
                .collect()
        }
        let by_pattern =
            s.execute("component.find", json!({"reference":"R*"}), None)?["result"].clone();
        assert_eq!(references(&by_pattern), ["R1", "R2", "RV1"]);
        assert_eq!(by_pattern["count"], 3);
        assert!(
            by_pattern["components"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .iter()
                .any(|c| c["reference"] == "R1" && c["id"] == r1 && c["placed"] == json!(true))
        );
        let exact = s.execute(
            "component.find",
            json!({"reference":"R?", "value":"1k"}),
            None,
        )?["result"]
            .clone();
        assert_eq!(references(&exact), ["R1"]);
        let by_value = s.execute("component.find", json!({"value":"1k"}), None)?["result"].clone();
        assert_eq!(references(&by_value), ["R1", "RV1"]);
        let by_kind =
            s.execute("component.find", json!({"kind":"resistor"}), None)?["result"].clone();
        assert_eq!(by_kind["count"], 3);
        let c1 = s
            .project
            .components
            .iter()
            .find(|c| c.reference == "C1")
            .ok_or_else(missing)?
            .id;
        let combo = s.execute(
            "component.find",
            json!({"reference":"*1","kind":"capacitor"}),
            None,
        )?["result"]
            .clone();
        assert_eq!(references(&combo), ["C1"]);
        assert_eq!(combo["components"][0]["id"], c1);
        let none = s.execute("component.find", json!({"reference":"ZZ*"}), None)?["result"].clone();
        assert_eq!(none["count"], 0);
        assert!(none["components"].as_array().is_some_and(|c| c.is_empty()));
        let before = s.inspect();
        for params in [
            json!({}),
            json!({"reference":"R1"}),
            json!({"reference":""}),
            json!({"reference":"x".repeat(65)}),
            json!({"value":""}),
            json!({"kind":""}),
            json!({"kind":"x".repeat(65)}),
            json!({"reference":"R*","extra":1}),
        ] {
            assert!(s.execute("component.find", params, None).is_err());
        }
        assert_eq!(s.inspect(), before);
        let unchanged = s.project.clone();
        s.execute("component.find", json!({"reference":"?*"}), None)?;
        assert_eq!(s.project, unchanged);
        Ok(())
    }
    #[test]
    fn footprints_can_be_assigned_to_every_component_matching_a_pattern() -> Result<(), Error> {
        let mut s = Session::default();
        for (kind, reference, x) in [
            ("resistor", "R1", 10.0),
            ("resistor", "R2", 25.0),
            ("resistor", "R3", 40.0),
            ("connector", "J1", 55.0),
        ] {
            s.execute(
                "component.add",
                json!({"kind":kind,"reference":reference,"position":{"x":x,"y":10.0}}),
                None,
            )?;
        }
        let path = library_fixture("R_0805_2012Metric.kicad_mod");
        let out = s.execute(
            "footprint.assign_pattern",
            json!({"pattern":"R*","path":path}),
            None,
        )?;
        assert_eq!(out["result"]["assigned"].as_array().map(Vec::len), Some(3));
        assert_eq!(out["result"]["skipped"].as_array().map(Vec::len), Some(0));
        for c in &s.project.components {
            if c.kind == "resistor" {
                assert!(
                    c.library_footprint.is_some(),
                    "{} took the footprint",
                    c.reference
                );
            } else {
                assert!(
                    c.library_footprint.is_none(),
                    "{} was left alone",
                    c.reference
                );
            }
        }
        s.undo()?;
        assert!(
            s.project
                .components
                .iter()
                .all(|c| c.library_footprint.is_none()),
            "one undo releases every bulk assignment"
        );
        s.execute(
            "component.place_library",
            json!({"path":library_fixture("lm358.kicad_sym"),"symbol":"LM2904","reference":"U1","position":{"x":70.0,"y":30.0}}),
            None,
        )?;
        let out = s.execute(
            "footprint.assign_pattern",
            json!({"pattern":"*","path":path}),
            None,
        )?;
        assert_eq!(out["result"]["assigned"].as_array().map(Vec::len), Some(4));
        assert_eq!(out["result"]["skipped"].as_array().map(Vec::len), Some(1));
        assert_eq!(
            s.project
                .components
                .iter()
                .filter(|c| c.library_footprint.is_some())
                .count(),
            4,
            "the eight-pin package cannot take a two-pad resistor footprint"
        );
        let before = s.inspect();
        for params in [
            json!({"pattern":"R1","path":path}),
            json!({"pattern":"","path":path}),
            json!({"pattern":"X*","path":path}),
            json!({"pattern":"R*","path":library_fixture("DIP-8_W7.62mm.kicad_mod")}),
            json!({"pattern":"R*","path":""}),
            json!({"pattern":"R*","path":library_fixture("absent.kicad_mod")}),
            json!({"pattern":"R*","path":path,"extra":1}),
        ] {
            assert!(s.execute("footprint.assign_pattern", params, None).is_err());
        }
        assert_eq!(s.inspect(), before, "rejections change nothing");
        assert!(s.project.validate().is_ok());
        Ok(())
    }
    #[test]
    fn bills_of_materials_group_by_value_footprint_and_properties() -> Result<(), Error> {
        let mut s = Session::default();
        let r1 = s.execute(
            "component.add",
            json!({"kind":"resistor","position":{"x":10.0,"y":10.0},"reference":"R1"}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let r2 = s.execute(
            "component.add",
            json!({"kind":"resistor","position":{"x":30.0,"y":10.0},"reference":"R2"}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let c1 = s.execute(
            "component.add",
            json!({"kind":"capacitor","position":{"x":50.0,"y":10.0},"reference":"C1"}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        for (id, value) in [(r1, "1k"), (r2, "1k"), (c1, "100n")] {
            s.execute("component.value", json!({"id": id, "value": value}), None)?;
        }
        for id in [r1, r2] {
            s.execute(
                "component.field",
                json!({"id":id,"key":"Datasheet","value":"dsA.pdf"}),
                None,
            )?;
        }
        s.execute(
            "component.field",
            json!({"id":r1,"key":"Manufacturer","value":"Acme"}),
            None,
        )?;
        fn row_in(table: &Value, reference: &str) -> Result<Value, Error> {
            table["items"]
                .as_array()
                .ok_or_else(missing)?
                .iter()
                .find(|i| {
                    i["references"]
                        .as_array()
                        .is_some_and(|r| r.contains(&json!(reference)))
                })
                .cloned()
                .ok_or_else(missing)
        }
        let plain = s.execute("bom.export", json!({}), None)?["result"].clone();
        assert_eq!(plain["components"], 3);
        assert_eq!(plain["groups"], 2);
        assert_eq!(plain["items"][0]["value"], "100n");
        assert_eq!(plain["items"][0]["quantity"], 1);
        assert_eq!(plain["items"][0]["footprint"], "starter pads");
        assert!(
            plain["items"][0]["fields"]
                .as_object()
                .is_some_and(|f| f.is_empty())
        );
        assert_eq!(plain["items"][1]["quantity"], 2);
        assert_eq!(plain["items"][1]["references"][0], "R1");
        assert_eq!(plain["items"][1]["references"][1], "R2");
        let with_fields = s.execute(
            "bom.export",
            json!({"fields":["Datasheet","Manufacturer"]}),
            None,
        )?["result"]
            .clone();
        assert_eq!(with_fields["groups"], 3);
        assert_eq!(row_in(&with_fields, "R1")?["quantity"], 1);
        assert_eq!(
            row_in(&with_fields, "R1")?["fields"]["Manufacturer"],
            "Acme"
        );
        assert_eq!(row_in(&with_fields, "R2")?["fields"]["Manufacturer"], "");
        assert_eq!(
            row_in(&with_fields, "R2")?["fields"]["Datasheet"],
            "dsA.pdf"
        );
        s.execute(
            "footprint.assign",
            json!({"id":r1,"path":library_fixture("R_0805_2012Metric.kicad_mod")}),
            None,
        )?;
        let assigned = s.execute("bom.export", json!({}), None)?["result"].clone();
        assert_eq!(assigned["groups"], 3);
        assert_eq!(
            row_in(&assigned, "R1")?["footprint"],
            "fixtures:R_0805_2012Metric"
        );
        assert_eq!(row_in(&assigned, "R2")?["footprint"], "starter pads");
        assert_eq!(row_in(&assigned, "R2")?["quantity"], 1);
        let before = s.inspect();
        for params in [
            json!({"fields":[""]}),
            json!({"fields":["x".repeat(65)]}),
            json!({"fields":(0..17).map(|i| format!("f{i}")).collect::<Vec<String>>()}),
            json!({"fields":["Datasheet"],"extra":1}),
        ] {
            assert!(s.execute("bom.export", params, None).is_err());
        }
        assert_eq!(s.inspect(), before);
        let unchanged = s.project.clone();
        s.execute("bom.export", json!({"fields":["Datasheet"]}), None)?;
        assert_eq!(s.project, unchanged);
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
    #[test]
    fn drawn_graphics_keep_their_properties_editable() -> Result<(), Error> {
        let mut s = Session::default();
        let line = s.execute(
            "graphic.add",
            json!({"layer":"board","shape":"line","points":[{"x":5.0,"y":5.0},{"x":25.0,"y":5.0}],"width":0.2}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let note = s.execute(
            "graphic.add",
            json!({"layer":"schematic","shape":"text","points":[{"x":10.0,"y":30.0}],"width":1.2,"text":"note"}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let circle = s.execute(
            "graphic.add",
            json!({"layer":"board","shape":"circle","points":[{"x":40.0,"y":20.0}],"width":2.0}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let out = s.execute(
            "graphic.set",
            json!({"id":line,"layer":"schematic","width":0.5}),
            None,
        )?;
        assert_eq!(out["result"]["layer"].as_str(), Some("schematic"));
        assert_eq!(out["result"]["width"].as_f64(), Some(0.5));
        s.execute("graphic.set", json!({"id":note,"text":"changed"}), None)?;
        assert_eq!(
            s.project
                .graphics
                .iter()
                .find(|g| g.id == note)
                .map(|g| g.text.as_str()),
            Some("changed")
        );
        s.execute(
            "graphic.set_point",
            json!({"id":line,"index":1,"position":{"x":25.0,"y":15.0}}),
            None,
        )?;
        assert_eq!(
            s.project
                .graphics
                .iter()
                .find(|g| g.id == line)
                .map(|g| g.points[1]),
            Some(Point::new(25.0, 15.0)),
            "one corner moves and the other stays put"
        );
        s.execute("graphic.set", json!({"id":circle,"width":3.5}), None)?;
        assert_eq!(
            s.project
                .graphics
                .iter()
                .find(|g| g.id == circle)
                .map(|g| g.width),
            Some(3.5),
            "the same field is the radius on a circle"
        );
        assert!(s.project.validate().is_ok());
        let before = s.inspect();
        for params in [
            json!({"id":line}),
            json!({"id":line,"layer":"copper"}),
            json!({"id":line,"width":0.0}),
            json!({"id":line,"width":1001.0}),
            json!({"id":line,"text":"x"}),
            json!({"id":note,"text":""}),
            json!({"id":999,"width":0.3}),
            json!({"id":line,"width":0.3,"extra":1}),
        ] {
            assert!(s.execute("graphic.set", params, None).is_err());
        }
        for params in [
            json!({"id":line,"index":2,"position":{"x":1.0,"y":1.0}}),
            json!({"id":line,"index":0,"position":{"x":null,"y":1.0}}),
            json!({"id":line,"index":0,"position":{"x":1e9,"y":1.0}}),
            json!({"id":999,"index":0,"position":{"x":1.0,"y":1.0}}),
            json!({"id":line,"position":{"x":1.0,"y":1.0}}),
            json!({"id":line,"index":0,"position":{"x":1.0,"y":1.0},"extra":1}),
        ] {
            assert!(s.execute("graphic.set_point", params, None).is_err());
        }
        assert_eq!(s.inspect(), before, "rejections change nothing");
        s.undo()?;
        assert_eq!(
            s.project
                .graphics
                .iter()
                .find(|g| g.id == circle)
                .map(|g| g.width),
            Some(2.0)
        );
        Ok(())
    }
    #[test]
    fn pins_leave_a_net_and_placed_copper_can_switch_nets() -> Result<(), Error> {
        let mut s = Session::default();
        let one = s.execute(
            "component.add",
            json!({"kind":"resistor","position":{"x":10.0,"y":10.0}}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let two = s.execute(
            "component.add",
            json!({"kind":"resistor","position":{"x":30.0,"y":10.0}}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let v1 = s.execute("net.create", json!({"name":"V1"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let v2 = s.execute("net.create", json!({"name":"V2"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        s.execute(
            "net.connect",
            json!({"net":v1,"pins":[{"component":one,"pin":"1"},{"component":two,"pin":"1"}]}),
            None,
        )?;
        s.execute(
            "footprint.assign",
            json!({"id":one,"path":library_fixture("R_0805_2012Metric.kicad_mod")}),
            None,
        )?;
        s.execute("board.sync", json!({}), None)?;
        assert_eq!(
            s.project.board.footprints[0]
                .pads
                .iter()
                .find(|pad| pad.number == "1")
                .and_then(|pad| pad.net),
            Some(v1),
            "the placed pad carries the net its pin was given"
        );
        let track = s.execute(
            "track.add",
            json!({"net":v1,"start":{"x":8.0,"y":20.0},"end":{"x":20.0,"y":20.0},"width":0.25}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let via = s.execute(
            "via.place",
            json!({"net":v1,"position":{"x":25.0,"y":20.0},"diameter":0.7,"drill":0.35}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let zone = s.execute(
            "zone.place",
            json!({"net":v1,"layer":"B.Cu","outline":[{"x":5.0,"y":25.0},{"x":25.0,"y":25.0},{"x":25.0,"y":35.0},{"x":5.0,"y":35.0},{"x":5.0,"y":25.0}]}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let out = s.execute(
            "net.disconnect",
            json!({"net":v1,"pins":[{"component":one,"pin":"1"}]}),
            None,
        )?;
        assert_eq!(out["result"]["released"].as_u64(), Some(1));
        assert_eq!(
            s.project
                .components
                .iter()
                .find(|c| c.id == one)
                .and_then(|c| c.pins.iter().find(|pin| pin.number == "1"))
                .and_then(|pin| pin.net),
            None,
            "the symbol pin is free again"
        );
        assert_eq!(
            s.project.board.footprints[0]
                .pads
                .iter()
                .find(|pad| pad.number == "1")
                .and_then(|pad| pad.net),
            None,
            "its placed pad is cleared too"
        );
        assert_eq!(
            s.project
                .components
                .iter()
                .find(|c| c.id == two)
                .and_then(|c| c.pins.iter().find(|pin| pin.number == "1"))
                .and_then(|pin| pin.net),
            Some(v1),
            "the other pin keeps its assignment"
        );
        for id in [track, via, zone] {
            s.execute("copper.set_net", json!({"id":id,"net":v2}), None)?;
        }
        assert_eq!(s.project.board.tracks[0].net, v2);
        assert_eq!(s.project.board.vias[0].net, v2);
        assert_eq!(s.project.board.zones[0].net, v2);
        assert!(s.project.validate().is_ok());
        let before = s.inspect();
        for params in [
            json!({"net":999,"pins":[{"component":one,"pin":"1"}]}),
            json!({"net":v1,"pins":[{"component":one,"pin":"2"}]}),
            json!({"net":v1,"pins":[{"component":999,"pin":"1"}]}),
            json!({"net":v1,"pins":[]}),
            json!({"net":v1}),
            json!({"net":v1,"pins":[{"component":one,"pin":"1"}],"extra":1}),
        ] {
            assert!(s.execute("net.disconnect", params, None).is_err());
        }
        for params in [
            json!({"id":track,"net":999}),
            json!({"id":track,"net":v2}),
            json!({"id":one,"net":v1}),
            json!({"id":999,"net":v1}),
            json!({"id":track}),
            json!({"id":track,"net":v1,"layer":"B.Cu"}),
        ] {
            assert!(s.execute("copper.set_net", params, None).is_err());
        }
        assert_eq!(s.inspect(), before, "rejections change nothing");
        s.undo()?;
        assert_eq!(s.project.board.zones[0].net, v1);
        Ok(())
    }
    #[test]
    fn board_cutouts_move_and_resize_within_the_outline() -> Result<(), Error> {
        let mut s = Session::default();
        let hole = s.execute(
            "board.cutout.add",
            json!({"center":{"x":30.0,"y":20.0},"radius":1.6}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let near = s.execute(
            "board.cutout.add",
            json!({"center":{"x":5.0,"y":5.0},"radius":1.0}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let gnd = s.execute("net.create", json!({"name":"GND"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        s.execute(
            "track.add",
            json!({"net":gnd,"start":{"x":5.0,"y":20.0},"end":{"x":25.0,"y":20.0},"width":0.2}),
            None,
        )?;
        fn tight(s: &Session) -> Result<usize, Error> {
            Ok(s.checks()["drc"]
                .as_array()
                .ok_or_else(missing)?
                .iter()
                .filter(|f| f["code"] == json!("cutout_clearance"))
                .count())
        }
        assert_eq!(tight(&s)?, 0, "the hole starts clear of the copper");
        s.execute(
            "board.cutout.move",
            json!({"id":hole,"center":{"x":26.0,"y":20.0}}),
            None,
        )?;
        assert_eq!(s.project.board.cutouts[0].center.x, 26.0);
        assert_eq!(
            tight(&s)?,
            1,
            "moving the hole next to the track breaks clearance"
        );
        s.execute(
            "board.cutout.move",
            json!({"id":hole,"center":{"x":35.0,"y":20.0}}),
            None,
        )?;
        assert_eq!(tight(&s)?, 0, "moving it away clears the finding");
        s.execute(
            "board.cutout.resize",
            json!({"id":hole,"radius":12.0}),
            None,
        )?;
        assert_eq!(s.project.board.cutouts[0].radius, 12.0);
        assert_eq!(tight(&s)?, 1, "a bigger hole reaches the copper again");
        assert!(s.project.validate().is_ok());
        let before = s.inspect();
        for params in [
            json!({"id":hole,"center":{"x":1.0,"y":20.0}}),
            json!({"id":hole,"center":{"x":59.0,"y":20.0}}),
            json!({"id":hole,"center":{"x":null,"y":20.0}}),
            json!({"id":hole,"center":{"x":1e200,"y":20.0}}),
            json!({"id":999,"center":{"x":10.0,"y":10.0}}),
            json!({"id":hole}),
            json!({"id":hole,"center":{"x":10.0,"y":10.0},"radius":1.0}),
        ] {
            assert!(s.execute("board.cutout.move", params, None).is_err());
        }
        for params in [
            json!({"id":near,"radius":6.0}),
            json!({"id":hole,"radius":20.0}),
            json!({"id":hole,"radius":0.0}),
            json!({"id":hole,"radius":-1.0}),
            json!({"id":hole,"radius":1001.0}),
            json!({"id":999,"radius":2.0}),
            json!({"id":hole}),
            json!({"id":hole,"radius":2.0,"center":{"x":1.0,"y":1.0}}),
        ] {
            assert!(s.execute("board.cutout.resize", params, None).is_err());
        }
        assert_eq!(
            s.project.board.cutouts[0].radius, 12.0,
            "the corner cutout stays where it was"
        );
        assert_eq!(s.inspect(), before, "rejections change nothing");
        s.undo()?;
        assert_eq!(s.project.board.cutouts[0].radius, 1.6);
        Ok(())
    }
    #[test]
    fn net_classes_can_be_retuned_and_removed_releasing_their_nets() -> Result<(), Error> {
        let mut s = Session::default();
        let a = s.execute("net.create", json!({"name":"A"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let b = s.execute("net.create", json!({"name":"B"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        for (net, y) in [(a, 10.0), (b, 10.5)] {
            s.execute(
                "track.add",
                json!({"net":net,"start":{"x":10.0,"y":y},"end":{"x":30.0,"y":y},"width":0.2}),
                None,
            )?;
        }
        fn tight(s: &Session) -> Result<usize, Error> {
            Ok(s.checks()["drc"]
                .as_array()
                .ok_or_else(missing)?
                .iter()
                .filter(|f| f["code"] == json!("clearance"))
                .count())
        }
        let class = s.execute(
            "netclass.add",
            json!({"name":"POWER","clearance":0.5,"width":0.4,"nets":[a]}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        assert_eq!(tight(&s)?, 1, "the class raises the requirement");
        s.execute(
            "netclass.clearance",
            json!({"class":class,"clearance":0.25}),
            None,
        )?;
        assert_eq!(
            tight(&s)?,
            0,
            "relaxing the class clears the finding at once"
        );
        s.execute(
            "netclass.clearance",
            json!({"class":class,"clearance":0.45}),
            None,
        )?;
        assert!(
            s.checks()["drc"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .iter()
                .any(|f| f["code"] == json!("clearance")
                    && f["message"]
                        .as_str()
                        .is_some_and(|m| m.contains("0.450 mm required"))),
            "the new clearance is the one reported"
        );
        let removed = s.execute("netclass.remove", json!({"class":class}), None)?;
        assert_eq!(removed["result"]["nets_released"].as_u64(), Some(1));
        assert!(s.project.net_classes.is_empty());
        assert!(
            s.project.net_class(a).is_none(),
            "the freed net falls back to the board default"
        );
        assert_eq!(tight(&s)?, 0, "the board default is looser than the class");
        s.undo()?;
        assert_eq!(s.project.net_classes.len(), 1);
        assert_eq!(s.project.net_classes[0].clearance, 0.45);
        assert_eq!(s.project.net_classes[0].nets, vec![a]);
        let before = s.inspect();
        for params in [
            json!({"class":class,"clearance":0.0}),
            json!({"class":class,"clearance":-0.1}),
            json!({"class":class,"clearance":100.5}),
            json!({"class":class,"clearance":null}),
            json!({"class":class}),
            json!({"class":999,"clearance":0.3}),
            json!({"class":class,"clearance":0.3,"width":0.9}),
        ] {
            assert!(s.execute("netclass.clearance", params, None).is_err());
        }
        for params in [
            json!({"class":999}),
            json!({"class":999,"extra":1}),
            json!({"class":null}),
        ] {
            assert!(s.execute("netclass.remove", params, None).is_err());
        }
        assert_eq!(
            s.inspect(),
            before,
            "rejections change nothing and undo stays put"
        );
        Ok(())
    }
    #[test]
    fn copper_sizes_can_be_edited_after_placement() -> Result<(), Error> {
        let mut s = Session::default();
        let power = s.execute("net.create", json!({"name":"PWR"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let class = s.execute(
            "netclass.add",
            json!({"name":"POWER","clearance":0.3,"width":0.6,"nets":[power]}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let track = s.execute(
            "track.add",
            json!({"net":power,"start":{"x":5.0,"y":5.0},"end":{"x":40.0,"y":5.0},"width":0.2}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let via = s.execute(
            "via.place",
            json!({"net":power,"position":{"x":40.0,"y":5.0},"diameter":0.6,"drill":0.3}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        fn thin_tracks(s: &Session) -> usize {
            s.checks()["drc"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .iter()
                .filter(|f| f["code"] == json!("track_width"))
                .count()
        }
        s.execute(
            "netclass.width_rules",
            json!({"class":class,"min_width":0.4}),
            None,
        )?;
        assert_eq!(
            thin_tracks(&s),
            1,
            "the thin track sits below the class floor"
        );
        let out = s.execute("track.set_width", json!({"id":track,"width":0.7}), None)?;
        assert_eq!(out["result"]["width"].as_f64(), Some(0.7));
        assert_eq!(
            thin_tracks(&s),
            0,
            "resizing the track clears the finding at once"
        );
        s.execute("via.set_size", json!({"id":via,"diameter":1.2}), None)?;
        assert_eq!(s.project.board.vias[0].diameter, 1.2);
        assert_eq!(
            s.project.board.vias[0].drill, 0.3,
            "an omitted drill keeps the size it was placed with"
        );
        s.execute(
            "via.set_size",
            json!({"id":via,"diameter":0.8,"drill":0.4}),
            None,
        )?;
        assert_eq!(s.project.board.vias[0].diameter, 0.8);
        assert_eq!(s.project.board.vias[0].drill, 0.4);
        let before = s.inspect();
        for params in [
            json!({"id":track,"width":0.0}),
            json!({"id":track,"width":-0.2}),
            json!({"id":track,"width":120.0}),
            json!({"id":track,"width":null}),
            json!({"id":track}),
            json!({"id":999,"width":0.5}),
            json!({"id":track,"width":0.5,"layer":"B.Cu"}),
        ] {
            assert!(s.execute("track.set_width", params, None).is_err());
        }
        for params in [
            json!({"id":via,"diameter":0.0}),
            json!({"id":via,"diameter":120.0}),
            json!({"id":via,"diameter":1.0,"drill":1.0}),
            json!({"id":via,"diameter":1.0,"drill":-0.1}),
            json!({"id":via,"diameter":0.3}),
            json!({"id":via,"diameter":1.0,"extra":1}),
            json!({"id":via}),
            json!({"id":999,"diameter":1.0}),
        ] {
            assert!(s.execute("via.set_size", params, None).is_err());
        }
        assert_eq!(s.inspect(), before, "rejections change nothing");
        s.execute("track.set_width", json!({"id":track,"width":0.3}), None)?;
        assert_eq!(s.project.board.tracks[0].width, 0.3);
        s.undo()?;
        assert_eq!(s.project.board.tracks[0].width, 0.7);
        assert_eq!(s.inspect()["project"], before["project"]);
        Ok(())
    }
    #[test]
    fn board_text_labels_place_move_edit_and_delete_with_one_undo() -> Result<(), Error> {
        let mut s = Session::default();
        let placed = s.execute(
            "text.place",
            json!({"text":"Acme Labs","position":{"x":10.0,"y":5.0}}),
            None,
        )?;
        let id = placed["result"]["id"].as_u64().ok_or_else(missing)?;
        assert_eq!(s.project.board.texts.len(), 1);
        assert_eq!(s.project.board.texts[0].size, 1.0);
        assert_eq!(
            s.project.board.texts[0].layer,
            pcbcraft_doc::Layer::Front,
            "labels default to the front copper side"
        );
        s.execute(
            "text.move",
            json!({"id":id,"position":{"x":12.0,"y":6.0}}),
            None,
        )?;
        assert_eq!(s.project.board.texts[0].position.x, 12.0);
        s.execute(
            "text.set",
            json!({"id":id,"text":"Acme 2026","size":2.5,"layer":"B.Cu"}),
            None,
        )?;
        assert_eq!(s.project.board.texts[0].text, "Acme 2026");
        assert_eq!(s.project.board.texts[0].size, 2.5);
        assert_eq!(s.project.board.texts[0].layer, pcbcraft_doc::Layer::Back);
        for params in [
            json!({"text":"","position":{"x":1.0,"y":1.0}}),
            json!({"text":"x".repeat(200),"position":{"x":1.0,"y":1.0}}),
            json!({"text":"x","position":{"x":null,"y":1.0}}),
            json!({"text":"x","position":{"x":1.0,"y":1.0},"size":0.01}),
            json!({"text":"x","position":{"x":1.0,"y":1.0},"size":90.0}),
            json!({"text":"x","position":{"x":1.0,"y":1.0},"layer":"C.Cu"}),
            json!({"text":"x","position":{"x":1.0,"y":1.0},"extra":1}),
        ] {
            assert!(s.execute("text.place", params, None).is_err());
        }
        assert_eq!(s.project.board.texts[0].text, "Acme 2026");
        assert!(
            s.execute(
                "text.move",
                json!({"id":id + 7,"position":{"x":1.0,"y":1.0}}),
                None
            )
            .is_err()
        );
        assert!(s.execute("text.set", json!({"id":id}), None).is_err());
        assert!(
            s.execute("text.set", json!({"id":id,"size":0.02}), None)
                .is_err()
        );
        assert_eq!(s.project.board.texts[0].size, 2.5);
        s.execute("object.delete", json!({"id":id}), None)?;
        assert!(s.project.board.texts.is_empty());
        s.undo()?;
        assert_eq!(s.project.board.texts.len(), 1);
        assert_eq!(s.project.board.texts[0].text, "Acme 2026");
        Ok(())
    }
}
