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
            "project.replace",
            "Find text across the project and replace it in one call: component references, values and fields, net names, free board and schematic text, and graphic text; a dry run reports every hit without changing anything",
            schema(
                json!({"search":{"type":"string","minLength":1,"maxLength":256},"replace":{"type":"string","maxLength":256},"scope":{"type":"string","enum":["all","components","nets","text"],"default":"all"},"dry_run":{"type":"boolean","default":false}}),
                &["search", "replace"],
            ),
        ),
        (
            "project.set_meta",
            "Record a project metadata entry such as title, revision, company or notes, keyed by 1 to 32 lowercase alphanumeric bytes; recording the same value again is refused",
            schema(
                json!({"key":{"type":"string","minLength":1,"maxLength":32},"value":{"type":"string","maxLength":4096}}),
                &["key", "value"],
            ),
        ),
        (
            "project.clear_meta",
            "Remove one project metadata entry by key",
            schema(
                json!({"key":{"type":"string","minLength":1,"maxLength":32}}),
                &["key"],
            ),
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
            "graphic.group",
            "Group two to 512 drawn graphics into one unit that can be moved together; each graphic joins at most one group",
            schema(
                json!({"graphics":{"type":"array","items":id_schema(),"minItems":2,"maxItems":512}}),
                &["graphics"],
            ),
        ),
        (
            "graphic.ungroup",
            "Dissolve a graphic group again, leaving its member graphics exactly where they are",
            schema(json!({"id":id_schema()}), &["id"]),
        ),
        (
            "graphic.translate",
            "Move every graphic in a group by the same offset in one undoable step",
            schema(
                json!({"id":id_schema(),"delta":point_schema()}),
                &["id", "delta"],
            ),
        ),
        (
            "dimension.add",
            "Draw a linear dimension between two points: witness lines, the dimension line offset to one side, and the measured distance in millimetres as text; the pieces are grouped so the whole annotation moves as one unit",
            schema(
                json!({"start":point_schema(),"end":point_schema(),"layer":{"type":"string","enum":["schematic","board"]},"offset":{"type":"number","minimum":-100,"maximum":100}}),
                &["start", "end", "layer"],
            ),
        ),
        (
            "dimension.radius",
            "Measure a circle or arc: a witness circle, a leader from the centre to a point on the rim, and the radius in millimetres as an R label; the pieces are grouped so the whole annotation moves as one unit",
            schema(
                json!({"center":point_schema(),"point":point_schema(),"layer":{"type":"string","enum":["schematic","board"]}}),
                &["center", "point", "layer"],
            ),
        ),
        (
            "dimension.diameter",
            "Measure a circle across its centre: a witness circle, a leader from the far rim through the centre to the near rim, and the diameter in millimetres as a label; the pieces are grouped so the whole annotation moves as one unit",
            schema(
                json!({"center":point_schema(),"point":point_schema(),"layer":{"type":"string","enum":["schematic","board"]}}),
                &["center", "point", "layer"],
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
            "components.reannotate",
            "Renumber component references in placement order (top to bottom, left to right): optionally restrict to one letter prefix, choose the starting number, and dry-run to preview the mapping; any collision with a reference outside the renumbered group refuses the whole call",
            schema(
                json!({"prefix":{"type":"string","pattern":"^[A-Za-z]{1,16}$"},"start":{"type":"integer","minimum":1,"maximum":1000000},"dry_run":{"type":"boolean"}}),
                &[],
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
            "net.tie",
            "Merge two nets into one: every pin, wire, label, track, via and zone on the source net joins the target net, net class and clearance pair memberships follow, and the source net is removed",
            schema(
                json!({"source":id_schema(),"target":id_schema()}),
                &["source", "target"],
            ),
        ),
        (
            "net.swap",
            "Exchange the names of two nets; each net keeps its id, members and copper, so two nets that crossed during a reassignment can be corrected in one step",
            schema(
                json!({"id_a":id_schema(),"id_b":id_schema()}),
                &["id_a", "id_b"],
            ),
        ),
        (
            "pins.set_all_no_connect",
            "Flag every pin of one component that is neither wired nor assigned, in a single undo step; pins already flagged are skipped",
            schema(json!({"component":id_schema()}), &["component"]),
        ),
        (
            "pins.clear_all_no_connect",
            "Remove every no-connect flag of one component in a single undo step",
            schema(json!({"component":id_schema()}), &["component"]),
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
            "netlist.import_session",
            "Import net assignments from a Specctra session file on disk: every named net with pins is created if missing and its REF-PIN entries are assigned to matching component pins",
            schema(
                json!({"path":{"type":"string","maxLength":1024}}),
                &["path"],
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
            "wire.move",
            "Slide a drawn wire by a distance; junctions sitting on its vertices travel with it",
            schema(
                json!({"id":id_schema(),"delta":point_schema()}),
                &["id", "delta"],
            ),
        ),
        (
            "wire.set_point",
            "Drag one vertex of a drawn wire to a new position, bending or straightening it; refused if a junction would be left off the wire",
            schema(
                json!({"id":id_schema(),"index":{"type":"integer","minimum":0,"maximum":255},"position":point_schema()}),
                &["id", "index", "position"],
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
            "barcode.place",
            "Draw a Code 39 barcode of up to 48 characters as bar graphics at a position, with optional height in millimetres, module width in millimetres and layer",
            schema(
                json!({"text":{"type":"string","minLength":1,"maxLength":48},"position":point_schema(),"height":{"type":"number","minimum":0.5,"maximum":50},"module":{"type":"number","minimum":0.05,"maximum":2},"layer":{"type":"string","enum":["schematic","board"]}}),
                &["text", "position"],
            ),
        ),
        (
            "schematic.table",
            "Draw a parts table on the schematic at a position: one row per component sorted by reference, one column per property key (reference and value by default), as ordinary graphics that capture the design as it stands now",
            schema(
                json!({"position":point_schema(),"fields":{"type":"array","items":{"type":"string","minLength":1,"maxLength":32},"maxItems":3},"max_rows":{"type":"integer","minimum":1,"maximum":200}}),
                &["position"],
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
            "label.set",
            "Retype a net label or point it at a different net in place; junctions must stay on matching wire",
            schema(
                json!({"id":id_schema(),"kind":{"type":"string","enum":["label","global","hierarchical","power","junction"]},"net":id_schema()}),
                &["id"],
            ),
        ),
        (
            "pin.no_connect",
            "Mark a pin intentionally unconnected, suppressing its unconnected-pin violation",
            schema(json!({"pin":pin_schema()}), &["pin"]),
        ),
        (
            "board.hole_place",
            "Drill an unplated through hole at a position: mounting or clearance drills carry no copper and belong to no net",
            schema(
                json!({"position":point_schema(),"diameter":{"type":"number","minimum":0.05,"maximum":25,"description":"drill diameter, millimetres"}}),
                &["position", "diameter"],
            ),
        ),
        (
            "board.hole_move",
            "Move a drilled unplated hole to a new position; its diameter is kept",
            schema(
                json!({"id":id_schema(),"position":point_schema()}),
                &["id", "position"],
            ),
        ),
        (
            "board.hole_set_size",
            "Resize the drill of an unplated hole in millimetres",
            schema(
                json!({"id":id_schema(),"diameter":{"type":"number","minimum":0.05,"maximum":25}}),
                &["id", "diameter"],
            ),
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
            "zone.set_layer",
            "Move a poured zone, with all its cutouts, to the other copper layer without moving its outline",
            schema(
                json!({"id":id_schema(),"layer":{"type":"string","enum":["F.Cu","B.Cu"]}}),
                &["id", "layer"],
            ),
        ),
        (
            "zone.set_point",
            "Move one vertex of a poured zone's outline, or of one of its cutouts, by index; closing vertices stay paired",
            schema(
                json!({"id":id_schema(),"index":{"type":"integer","minimum":0},"position":point_schema(),"cutout":{"type":"integer","minimum":0,"description":"which cutout outline to edit; omit to edit the zone outline"}}),
                &["id", "index", "position"],
            ),
        ),
        (
            "zone.insert_point",
            "Insert a new vertex into a poured zone's outline, or one of its cutouts, at an index; outlines hold 3 to 256 vertices",
            schema(
                json!({"id":id_schema(),"index":{"type":"integer","minimum":0},"position":point_schema(),"cutout":{"type":"integer","minimum":0,"description":"which cutout outline to edit; omit to edit the zone outline"}}),
                &["id", "index", "position"],
            ),
        ),
        (
            "zone.remove_point",
            "Remove one vertex from a poured zone's outline, or one of its cutouts, by index; at least three vertices must remain",
            schema(
                json!({"id":id_schema(),"index":{"type":"integer","minimum":0},"cutout":{"type":"integer","minimum":0,"description":"which cutout outline to edit; omit to edit the zone outline"}}),
                &["id", "index"],
            ),
        ),
        (
            "zone.set_fill",
            "Pour a zone's copper back in, or take it out to a bare outline that carries no copper and reports no findings",
            schema(
                json!({"id":id_schema(),"filled":{"type":"boolean","description":"true pours the copper, false leaves only the outline"}}),
                &["id", "filled"],
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
            "footprint.set_pad",
            "Reposition a placed footprint pad by its local offset, and resize it when it is a plain round pad; shaped pads keep their library shape",
            schema(
                json!({"id":id_schema(),"pad":{"type":"string","description":"pad number, exactly as listed by pad.table"},"offset":point_schema(),"diameter":{"type":"number","exclusiveMinimum":0,"maximum":100}}),
                &["id", "pad"],
            ),
        ),
        (
            "footprint.set_castellated",
            "Flag one placed footprint pad castellated - plated and cut at a board edge so another board can be soldered onto it - or clear the flag",
            schema(
                json!({"id":id_schema(),"pad":{"type":"string","description":"pad number, exactly as listed by pad.table"},"on":{"type":"boolean"}}),
                &["id", "pad", "on"],
            ),
        ),
        (
            "footprint.dock_edge",
            "Slide a placed footprint along one axis so its copper sits flush against the named board edge, as an edge connector should; rotation is preserved",
            schema(
                json!({"id":id_schema(),"edge":{"type":"string","enum":["left","right","top","bottom"]}}),
                &["id", "edge"],
            ),
        ),
        (
            "footprint.set_locked",
            "Lock or unlock a placed footprint; a locked footprint refuses move, rotate, mirror, array and edge docking until it is unlocked again",
            schema(
                json!({"id":id_schema(),"locked":{"type":"boolean"}}),
                &["id", "locked"],
            ),
        ),
        (
            "footprint.array",
            "Step-and-repeat: clone a placed footprint and its component into a rectangular grid of copies at fixed column and row spacing; every copy gets fresh IDs and a numbered reference, the clones must stay within the project object limit, and one undo removes the whole array",
            schema(
                json!({"footprint":id_schema(),"columns":{"type":"integer","minimum":1,"maximum":64},"rows":{"type":"integer","minimum":1,"maximum":64},"dx":{"type":"number","exclusiveMinimum":0,"maximum":10000},"dy":{"type":"number","exclusiveMinimum":0,"maximum":10000}}),
                &["footprint", "columns", "rows", "dx", "dy"],
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
            "track.add_arc",
            "Add a curved copper segment through three points (start, a point on the curve, end); clearance checks sample the arc and fabrication output draws a true arc",
            schema(
                json!({"net":id_schema(),"start":point_schema(),"mid":point_schema(),"end":point_schema(),"width":{"type":"number","exclusiveMinimum":0},"layer":{"type":"string","enum":["F.Cu","B.Cu"]}}),
                &["net", "start", "mid", "end"],
            ),
        ),
        (
            "track.straighten",
            "Turn a curved track back into the straight segment between its ends",
            schema(json!({"id":id_schema()}), &["id"]),
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
            "track.set_layer",
            "Move an existing track segment to the other copper layer without moving its ends",
            schema(
                json!({"id":id_schema(),"layer":{"type":"string","enum":["F.Cu","B.Cu"]}}),
                &["id", "layer"],
            ),
        ),
        (
            "track.set_end",
            "Drag one end of an existing track to a new position, shortening, lengthening or re-aiming it; the other end stays put",
            schema(
                json!({"id":id_schema(),"end":{"type":"string","enum":["start","end"],"description":"which end of the segment to move"},"position":point_schema()}),
                &["id", "end", "position"],
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
            "items.update",
            "Apply property changes to up to 128 placed board objects in one undo step: tracks take net, copper layer and width; vias take net, diameter and drill; labels take text, layer and size",
            schema(
                json!({"items":{"type":"array","minItems":1,"maxItems":128,"items":{"type":"object","properties":{"id":id_schema(),"net":id_schema(),"layer":{"type":"string","enum":["F.Cu","B.Cu"]},"width":{"type":"number"},"diameter":{"type":"number"},"drill":{"type":"number"},"text":{"type":"string","maxLength":1024},"size":{"type":"number"}},"required":["id"],"additionalProperties":false}}}),
                &["items"],
            ),
        ),
        (
            "drill.chart",
            "Report the board's drilled holes grouped by size: every drilled via and through-hole pad counted per round diameter or oval width and height, with the board thickness",
            schema(json!({}), &[]),
        ),
        (
            "board.statistics",
            "Report board counts and totals: footprints, tracks and routed copper length per layer, vias, zones split into poured and outline-only, cutouts, labels, drilled holes, board area and the recorded surface finish",
            schema(json!({}), &[]),
        ),
        (
            "board.set_finish",
            "Record the board's surface finish (for example HASL, ENIG or OSP) for reports and fabrication notes",
            schema(
                json!({"finish":{"type":"string","minLength":1,"maxLength":64}}),
                &["finish"],
            ),
        ),
        (
            "board.set_stackup",
            "Record the stackup used by impedance estimates: dielectric thickness in millimetres and substrate relative permittivity; give at least one",
            schema(
                json!({"thickness":{"type":"number"},"er":{"type":"number"}}),
                &[],
            ),
        ),
        (
            "board.set_drill_origin",
            "Record the drill origin in millimetres that drill file coordinates are measured from",
            schema(json!({"position":point_schema()}), &["position"]),
        ),
        (
            "board.reset_drill_origin",
            "Return the drill origin to the board's lower-left corner",
            schema(json!({}), &[]),
        ),
        (
            "board.set_outline",
            "Trace a free-form closed board outline of 3 to 64 distinct vertices; the rectangular width and height follow the outline's bounding box, and fabrication layers cut the board to the traced shape",
            schema(
                json!({"points":{"type":"array","items":point_schema(),"minItems":3,"maxItems":64}}),
                &["points"],
            ),
        ),
        (
            "board.clear_outline",
            "Drop the traced board outline and return to the plain rectangular board",
            schema(json!({}), &[]),
        ),
        (
            "board.set_layer_record",
            "Record or replace one board layer note: its name, role (copper, core, prepreg, soldermask, silkscreen, paste or custom) and thickness in millimetres; at most 32 notes per board",
            schema(
                json!({"name":{"type":"string","minLength":1,"maxLength":64},"kind":{"type":"string","enum":["copper","core","prepreg","soldermask","silkscreen","paste","custom"]},"thickness":{"type":"number"}}),
                &["name", "kind", "thickness"],
            ),
        ),
        (
            "board.clear_layer_records",
            "Remove every recorded board layer note",
            schema(json!({}), &[]),
        ),
        (
            "board.set_machining",
            "Record machining notes for fabrication: whether edges are plated, the drill diameter tolerance and the rout dimension tolerance in millimetres; give at least one, each given value replaces its stored note",
            schema(
                json!({"edge_plating":{"type":"boolean"},"drill_tolerance":{"type":"number","minimum":0,"maximum":1},"edge_tolerance":{"type":"number","minimum":0,"maximum":1}}),
                &[],
            ),
        ),
        (
            "board.clear_machining",
            "Remove every recorded machining note from the board",
            schema(json!({}), &[]),
        ),
        (
            "board.set_pair_rule",
            "Require a minimum clearance in millimetres between two named nets, stricter than class or board default when larger; the same pair replaces its earlier rule",
            schema(
                json!({"net_a":id_schema(),"net_b":id_schema(),"clearance":{"type":"number","exclusiveMinimum":0,"maximum":100}}),
                &["net_a", "net_b", "clearance"],
            ),
        ),
        (
            "board.remove_pair_rule",
            "Remove the clearance rule between two named nets",
            schema(
                json!({"net_a":id_schema(),"net_b":id_schema()}),
                &["net_a", "net_b"],
            ),
        ),
        (
            "net.lengths",
            "Report routed copper length per net: track segments summed in millimetres, with via counts and unrouted nets listed",
            schema(json!({"net":id_schema()}), &[]),
        ),
        (
            "net.chain",
            "Trace a net chain from a starting pin through two-pin parts in series, reporting each hop's component, reference, pin and net, and how the walk ended: open, branch, multi or loop",
            schema(
                json!({"component":id_schema(),"pin":{"type":"string","minLength":1,"maxLength":16}}),
                &["component", "pin"],
            ),
        ),
        (
            "net.impedance",
            "Report the approximate single-ended impedance of each routed net from its widest track and the recorded stackup; with a target ohm value also report the width that would meet it",
            schema(json!({"net":id_schema(),"target":{"type":"number"}}), &[]),
        ),
        (
            "board.bounds",
            "Report the bounding box in millimetres of the board outline and of placed objects, overall and per copper side, from object anchors, pad and via extents and zone outlines",
            schema(json!({}), &[]),
        ),
        (
            "items.find",
            "List object IDs by kind (track, via, zone, pad, footprint, text or component) with optional net and copper-layer filters",
            schema(
                json!({"kind":{"type":"string","enum":["track","via","zone","pad","footprint","text","component"]},"net":id_schema(),"layer":{"type":"string","enum":["F.Cu","B.Cu"]}}),
                &["kind"],
            ),
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
            "rules.exclude_region",
            "Silence one check code inside a rectangle; a finding drops when every object it names sits within the region",
            schema(
                json!({"code":{"type":"string","description":"check code, as listed by rules.severity"},"x":{"type":"number","minimum":-1000,"maximum":1000,"description":"lower-left corner in millimetres"},"y":{"type":"number","minimum":-1000,"maximum":1000},"width":{"type":"number","exclusiveMinimum":0,"maximum":1000},"height":{"type":"number","exclusiveMinimum":0,"maximum":1000}}),
                &["code", "x", "y", "width", "height"],
            ),
        ),
        (
            "rules.include_region",
            "Lift rectangular region exclusions for one check code; omit index to lift every region of that code",
            schema(
                json!({"code":{"type":"string"},"index":{"type":"integer","minimum":0,"description":"which region of that code to remove, oldest first; omit to remove all"}}),
                &["code"],
            ),
        ),
        (
            "rules.area_add",
            "Add a rectangular rule area that demands a stricter minimum copper separation for copper whose midpoints lie inside it; optionally narrowed to one net and one copper layer, which apply when either side of a pair matches",
            schema(
                json!({"name":{"type":"string","maxLength":64,"description":"optional note describing the area"},"net":id_schema(),"layer":{"type":"string","enum":["F.Cu","B.Cu"]},"clearance":{"type":"number","exclusiveMinimum":0,"maximum":100,"description":"minimum separation inside the area, in millimetres"},"x":{"type":"number","minimum":-1000,"maximum":1000,"description":"lower-left corner in millimetres"},"y":{"type":"number","minimum":-1000,"maximum":1000},"width":{"type":"number","exclusiveMinimum":0,"maximum":1000},"height":{"type":"number","exclusiveMinimum":0,"maximum":1000}}),
                &["clearance", "x", "y", "width", "height"],
            ),
        ),
        (
            "rules.area_remove",
            "Remove one rule area by id",
            schema(json!({"id":id_schema()}), &["id"]),
        ),
        (
            "bom.set_fields",
            "Define the extra bill-of-materials columns this project prints: each column names a component property and the header to show, falling back to the property key; at most 16 columns",
            schema(
                json!({"fields":{"type":"array","items":{"type":"object","properties":{"key":{"type":"string","minLength":1,"maxLength":64},"label":{"type":"string","maxLength":64}},"required":["key"],"additionalProperties":false},"minItems":1,"maxItems":16}}),
                &["fields"],
            ),
        ),
        (
            "bom.clear_fields",
            "Remove the project-defined bill-of-materials columns",
            schema(json!({}), &[]),
        ),
        (
            "file.attach",
            "Attach a text file to the project (licence note, datasheet excerpt); attaching an existing name replaces its contents, at most 32 files of 256 KiB each",
            schema(
                json!({"name":{"type":"string","minLength":1,"maxLength":128},"data":{"type":"string","maxLength":262144}}),
                &["name", "data"],
            ),
        ),
        (
            "file.remove",
            "Detach an attached text file by name",
            schema(
                json!({"name":{"type":"string","minLength":1,"maxLength":128}}),
                &["name"],
            ),
        ),
        (
            "variant.create",
            "Define a named assembly variant, optionally with a note and an initial set of component ids to leave unpopulated",
            schema(
                json!({"name":{"type":"string","minLength":1,"maxLength":64},"note":{"type":"string","maxLength":256},"dnp":{"type":"array","items":id_schema(),"maxItems":512}}),
                &["name"],
            ),
        ),
        (
            "variant.set_dnp",
            "Set which components an assembly variant leaves unpopulated: with dnp true (default) the listed component ids join the set, with dnp false they leave it; an empty list with dnp true clears the set",
            schema(
                json!({"name":{"type":"string","minLength":1,"maxLength":64},"ids":{"type":"array","items":id_schema(),"maxItems":512},"dnp":{"type":"boolean"}}),
                &["name", "ids"],
            ),
        ),
        (
            "variant.set_footprint",
            "Record that an assembly variant populates one component with a different footprint, replacing any earlier swap for that component; a variant bill of materials prints these swaps",
            schema(
                json!({"name":{"type":"string","minLength":1,"maxLength":64},"component":id_schema(),"footprint":{"type":"string","minLength":1,"maxLength":128,"description":"package this build uses in place of the placed footprint"}}),
                &["name", "component", "footprint"],
            ),
        ),
        (
            "variant.clear_footprint",
            "Remove an assembly variant's footprint swap for one component, restoring the footprint the design places",
            schema(
                json!({"name":{"type":"string","minLength":1,"maxLength":64},"component":id_schema()}),
                &["name", "component"],
            ),
        ),
        (
            "variant.remove",
            "Delete an assembly variant by name",
            schema(
                json!({"name":{"type":"string","minLength":1,"maxLength":64}}),
                &["name"],
            ),
        ),
        (
            "bom.export",
            "Export a bill of materials: components grouped by value, kind, footprint and requested properties; with a variant name, that build's unpopulated parts are skipped and its footprint swaps printed",
            schema(
                json!({"fields":{"type":"array","items":{"type":"string"},"maxItems":16},"variant":{"type":"string","minLength":1,"maxLength":64,"description":"assembly variant to print the bill for"}}),
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
            "object.nudge",
            "Slide one placed board object, track, via, footprint, zone, cutout or text label, by a distance in millimetres",
            schema(
                json!({"id":id_schema(),"dx":{"type":"number","minimum":-1000,"maximum":1000},"dy":{"type":"number","minimum":-1000,"maximum":1000}}),
                &["id", "dx", "dy"],
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
            "Change a board text label's string, size, layer, or rotation",
            schema(
                json!({"id":id_schema(),"text":{"type":"string","minLength":1,"maxLength":128},"size":{"type":"number","minimum":0.1,"maximum":50},"layer":{"type":"string","enum":["F.Cu","B.Cu"]},"rotation":{"type":"number","description":"text angle in degrees about the anchor"}}),
                &["id"],
            ),
        ),
        (
            "text.rotate",
            "Turn a board text label to an absolute angle in degrees about its anchor",
            schema(
                json!({"id":id_schema(),"degrees":{"type":"number"}}),
                &["id", "degrees"],
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
        json!({"revision":self.revision,"project":self.project,"history":{"undo":self.undo.len(),"redo":self.redo.len()},"capabilities":{"meta":"project metadata entries such as title, revision, company and notes, recorded, replaced and cleared by key, travelling with the saved design","schematic":"flat explicit pin nets with named net labels that can be retyped or pointed at another net in place, junctions and no-connect flags that can be raised or cleared across a whole component in one step, and pins that can be released from a net again, and nets that can be merged into one another or have just their names exchanged, with drawn wires that slide whole or bend vertex by vertex while their junctions follow, and drawn graphics whose layer, stroke, text and individual vertices stay editable, and graphic groups that shift many graphics together in one step, including linear dimensions drawn from witness lines and measured text and radial dimensions that measure a circle with a leader and an R or diameter label; embedded multi-unit packages with independent unit placement and one physical pin map; references renumbered per prefix in placement order, with a dry run and collision refusal, and series chains through two-pin parts traceable from any pin with the reason the walk ended, and a parts table of the current components, one row each sorted by reference, drawn as ordinary graphics that stay editable","pcb":"front SMD and plated through-hole circle/rect/roundrect/oval pads, circular/oval offset drills, with placed pads repositionable by their local offset and plain round pads resizable, and placed pads flaggable castellated for edge-soldered mezzanine builds, two copper layers, simple tracks including curved segments through three points whose clearance checks sample the arc and whose fabrication output draws a true arc, and plated through-hole vias including stitch grids, and unplated mounting or clearance drills with no copper and no net that move and resize and keep copper and the board edge clear and appear in the drill file, with copper sizes editable after placement and placed copper switchable between nets, tracks movable between the two copper layers with either end draggable and any placed board object slidable by a distance, and placed tracks, vias and labels batch-editable by property with up to 128 objects in one undo step, named assembly variants flagging chosen components do-not-populate and swapping in alternative footprints per component, with a bill of materials printable for one variant using those skips and swaps, placed footprints step-and-repeatable across a grid in one undoable array with numbered references and dockable flush against any board edge as edge connectors and lockable in place so no placement command can disturb them, poured copper zones with cutouts that can also switch copper layer and are editable vertex by vertex, including inserting and removing corners, rectangular outline and circular cutouts that stay editable after placement, and zones that can be taken out of the copper as bare outlines and poured back in, free text labels on either copper side, turnable to any angle about their anchor, Code 39 barcodes drawn as editable bar graphics on either layer, named net classes carrying per-class clearance and default track width, retunable or deleted in place, and a named pair of nets carrying its own stricter clearance, and rectangular rule areas that optionally narrow to one net and copper layer and demand a stricter gap for copper whose midpoints fall inside them; bills of materials grouped by value and footprint with named component properties, and the extra columns themselves definable and clearable as a project setting; project-wide find-and-replace over references, values, properties, net names and free text, with a dry run and collision refusal; classes filled by name pattern with * and ? wildcards; classes can demand target routed lengths within a tolerance, and a single net can carry its own target that takes priority over its class; a board statistics report counts objects and totals per layer, and the board surface finish is recorded for fabrication notes and, with the recorded dielectric thickness and permittivity, for impedance estimates, and each board layer's role and thickness can be recorded and cleared as named fabrication notes, with machining notes for edge plating and drill and rout tolerances, and a free-form closed board outline traced from 3 to 64 vertices that edge clearance and fabrication layers follow while the rectangle resizes to its bounding box","library":"native project-library symbol/footprint authoring, reusable .pcbcraft-lib collections, embedded placed copies, footprint 3D model references with placement transforms that read, edit and export but never render, and per-footprint design rule overrides where a local clearance tightens checks while solder mask and paste margins are recorded and exported; text files attach to the project and travel with saved designs","checks":"initial pin assignment with no-connect flag suppression and conservative copper bounds honouring net class, net-pair, rule-area and footprint-local clearances, with same-pad plated layer bridges and drilled-hole contact tests, plus routed copper length totals per net and per-check-code severity overrides, with per-object and per-region exclusions that silence accepted findings; not comprehensive ERC/DRC","kicad":"native library reading, router design-session net-map import and limited PCB export, plus RS-274X Gerber and Excellon drill fabrication output, oval holes as spanned slots and unplated drills measured from a recorded drill origin, a plain-text netlist, a bill-of-materials CSV, a DXF board drawing and a pick-and-place centroid file, written straight to disk; no general project import or library writer","routing":"orthogonal routes on chosen copper layer; no obstacle avoidance or shove","3d":false}})
    }
    pub fn checks(&self) -> Value {
        json!({"revision":self.revision,"erc":pcbcraft_rules::erc(&self.project),"drc":pcbcraft_rules::drc(&self.project),"airwires":pcbcraft_rules::airwires(&self.project),"scope":"Initial checks: pin assignment (suppressed for pins flagged no-connect), net population, capsule tracks sampled along their curve when curved, board outline edge (traced polygon or rectangle) and circular cutout clearance. Copper clearance uses the strictest of the board default and the clearance of the net classes on either side. A clearance rule set between two named nets can demand a stricter gap. Tracks on classified nets are checked against the class minimum and maximum width when set. Noncircular pads use sampled convex outlines with conservative chord-error clearance and inscribed connectivity bounds. Plated pads bridge the same physical pad between F.Cu/B.Cu; traces wholly inside circular/oval drills do not contact copper. Conservative false findings remain possible. Each check code can be re-levelled to warning or silenced entirely through project severity overrides. No comprehensive pin-type, annular-ring manufacturing rules, zones or hierarchy checks."})
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
/// Approximate single-ended microstrip impedance in ohms, widths and heights
/// in millimetres, assuming a standard 35 um copper thickness.
fn microstrip_ohms(width: f64, height: f64, er: f64) -> f64 {
    const COPPER: f64 = 0.035;
    if width <= height {
        (87.0 / (er + 1.41).sqrt()) * (5.98 * height / (0.8 * width + COPPER)).ln()
    } else {
        let ratio = width / height;
        (120.0 * std::f64::consts::PI / er.sqrt()) / (ratio + 1.393 + 0.667 * (ratio + 1.444).ln())
    }
}
/// Track width that comes closest to a target impedance, found by bisection.
fn width_for_ohms(target: f64, height: f64, er: f64) -> f64 {
    let (mut lo, mut hi) = (0.01f64, 100.0f64);
    for _ in 0..60 {
        let mid = (lo + hi) / 2.0;
        if microstrip_ohms(mid, height, er) > target {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    (lo + hi) / 2.0
}
fn import_nets(p: &mut Project, next: &mut Id, a: &NetlistImportParams) -> Result<Value, Error> {
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
                    Error::Invalid(format!("{} has no pin {}", listed.reference, listed.pin))
                })?;
            if pin
                .net
                .is_some_and(|other| !p.nets.iter().any(|n| n.id == other && n.name == entry.name))
            {
                return Err(Error::Invalid(format!(
                    "{} pin {} already belongs to another net",
                    listed.reference, listed.pin
                )));
            }
            let previous = planned.insert((component.id, listed.pin.clone()), entry.name.clone());
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
struct PadSetParams {
    id: Id,
    pad: String,
    #[serde(default)]
    offset: Option<Point>,
    #[serde(default)]
    diameter: Option<f64>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PadCastellateParams {
    id: Id,
    pad: String,
    on: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DockEdgeParams {
    id: Id,
    edge: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LockParams {
    id: Id,
    locked: bool,
}
fn footprint_locked(project: &pcbcraft_doc::Project, id: Id) -> bool {
    project
        .board
        .footprints
        .iter()
        .any(|f| f.id == id && f.locked.unwrap_or(false))
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
                castellated: None,
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
                castellated: None,
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
struct MetaParams {
    key: String,
    value: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MetaKeyParams {
    key: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReplaceParams {
    search: String,
    replace: String,
    #[serde(default = "replace_all_scope")]
    scope: String,
    #[serde(default)]
    dry_run: bool,
}
fn replace_all_scope() -> String {
    "all".into()
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
struct BarcodeParams {
    text: String,
    position: Point,
    #[serde(default)]
    height: Option<f64>,
    #[serde(default)]
    module: Option<f64>,
    #[serde(default)]
    layer: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SchematicTableParams {
    position: Point,
    #[serde(default)]
    fields: Vec<String>,
    #[serde(default)]
    max_rows: Option<usize>,
}
/// Code 39 element patterns: five bars and four spaces per symbol, three of
/// the nine elements wide, bars at even indices.
const CODE39: &[(char, &str)] = &[
    ('*', "nwnnwnwnn"),
    ('0', "nnnwwnwnn"),
    ('1', "wnnwnnnnw"),
    ('2', "nnwwnnnnw"),
    ('3', "wnwwnnnnn"),
    ('4', "nnnwwnnnw"),
    ('5', "wnnwwnnnn"),
    ('6', "nnwwwnnnn"),
    ('7', "nnnwnnwnw"),
    ('8', "wnnwnnwnn"),
    ('9', "nnwwnnwnn"),
    ('A', "wnnnnwnnw"),
    ('B', "nnwnnwnnw"),
    ('C', "wnwnnwnnn"),
    ('D', "nnnnwwnnw"),
    ('E', "wnnnwwnnn"),
    ('F', "nnwnwwnnn"),
    ('G', "nnnnnwwnw"),
    ('H', "wnnnnwwnn"),
    ('I', "nnwnnwwnn"),
    ('J', "nnnnwwwnn"),
    ('K', "wnnnnnnww"),
    ('L', "nnwnnnnww"),
    ('M', "wnwnnnnwn"),
    ('N', "nnnnwnnww"),
    ('O', "wnnnwnnwn"),
    ('P', "nnwnwnnwn"),
    ('Q', "nnnnnnwww"),
    ('R', "wnnnnnwwn"),
    ('S', "nnwnnnwnw"),
    ('T', "nnnwnnwnn"),
    ('U', "wwnnnnnnw"),
    ('V', "nwwnnnnnw"),
    ('W', "wwwnnnnnn"),
    ('X', "nwnnwnnnw"),
    ('Y', "wwnnwnnnn"),
    ('Z', "nwwnwnnnn"),
    ('-', "nwnnnnwnw"),
    ('.', "nwwnnnnwn"),
    (' ', "nwnnnnwwn"),
    ('$', "nwnwnwnnn"),
    ('/', "nwnwnnnwn"),
    ('+', "nwnnnwnwn"),
    ('%', "nnnwnwnwn"),
];
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
struct NetTieParams {
    source: Id,
    target: Id,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NetSwapParams {
    id_a: Id,
    id_b: Id,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ComponentRef {
    component: Id,
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
struct HolePlaceParams {
    position: Point,
    diameter: f64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HoleSizeParams {
    id: Id,
    diameter: f64,
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
    #[serde(default)]
    variant: Option<String>,
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
struct RuleRegionParams {
    code: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RuleRegionLiftParams {
    code: String,
    #[serde(default)]
    index: Option<usize>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RuleAreaParams {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    net: Option<Id>,
    #[serde(default)]
    layer: Option<String>,
    clearance: f64,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NetLengthsParams {
    #[serde(default)]
    net: Option<Id>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NetChainParams {
    component: Id,
    pin: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StackupParams {
    #[serde(default)]
    thickness: Option<f64>,
    #[serde(default)]
    er: Option<f64>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NetImpedanceParams {
    #[serde(default)]
    net: Option<Id>,
    #[serde(default)]
    target: Option<f64>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DrillOriginParams {
    position: Point,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OutlineParams {
    points: Vec<Point>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LayerRecordParams {
    name: String,
    kind: String,
    thickness: f64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MachiningParams {
    #[serde(default)]
    edge_plating: Option<bool>,
    #[serde(default)]
    drill_tolerance: Option<f64>,
    #[serde(default)]
    edge_tolerance: Option<f64>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PairRuleParams {
    net_a: Id,
    net_b: Id,
    #[serde(default)]
    clearance: Option<f64>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BomFieldsParams {
    fields: Vec<pcbcraft_doc::BomField>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FileParams {
    name: String,
    #[serde(default)]
    data: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct VariantCreateParams {
    name: String,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    dnp: Vec<Id>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct VariantDnpParams {
    name: String,
    ids: Vec<Id>,
    #[serde(default)]
    dnp: Option<bool>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct VariantFootprintParams {
    name: String,
    component: Id,
    #[serde(default)]
    footprint: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct VariantNameParams {
    name: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateItem {
    id: Id,
    #[serde(default)]
    net: Option<Id>,
    #[serde(default)]
    layer: Option<pcbcraft_doc::Layer>,
    #[serde(default)]
    width: Option<f64>,
    #[serde(default)]
    diameter: Option<f64>,
    #[serde(default)]
    drill: Option<f64>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    size: Option<f64>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ItemsUpdateParams {
    items: Vec<UpdateItem>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ItemFindParams {
    kind: String,
    #[serde(default)]
    net: Option<Id>,
    #[serde(default)]
    layer: Option<String>,
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
struct SessionImportParams {
    path: String,
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
struct LabelSetParams {
    id: Id,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    net: Option<Id>,
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
struct WireMoveParams {
    id: Id,
    delta: Point,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WirePointParams {
    id: Id,
    index: u32,
    position: Point,
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
struct IdParams {
    id: Id,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TrackArcParams {
    net: Id,
    start: Point,
    mid: Point,
    end: Point,
    #[serde(default)]
    width: Option<f64>,
    #[serde(default = "front")]
    layer: Layer,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TrackLayerParams {
    id: Id,
    layer: Layer,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TrackEndParams {
    id: Id,
    end: TrackEnd,
    position: Point,
}
#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
enum TrackEnd {
    Start,
    End,
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
struct ZoneLayerParams {
    id: Id,
    layer: Layer,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ZonePointParams {
    id: Id,
    #[serde(default)]
    cutout: Option<usize>,
    index: usize,
    position: Point,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ZoneInsertParams {
    id: Id,
    #[serde(default)]
    cutout: Option<usize>,
    index: usize,
    position: Point,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ZoneRemoveParams {
    id: Id,
    #[serde(default)]
    cutout: Option<usize>,
    index: usize,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ZoneFillParams {
    id: Id,
    filled: bool,
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
struct GraphicGroupParams {
    graphics: Vec<Id>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GraphicGroupIdParams {
    id: Id,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GraphicTranslateParams {
    id: Id,
    delta: Point,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DimensionParams {
    start: Point,
    end: Point,
    layer: String,
    #[serde(default)]
    offset: Option<f64>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RadialParams {
    center: Point,
    point: Point,
    layer: String,
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
    #[serde(default)]
    rotation: Option<f64>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TextRotateParams {
    id: Id,
    degrees: f64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SnapParams {
    ids: Vec<Id>,
    grid: f64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NudgeParams {
    id: Id,
    dx: f64,
    dy: f64,
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
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ArrayParams {
    footprint: Id,
    columns: usize,
    rows: usize,
    dx: f64,
    dy: f64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReannotateParams {
    #[serde(default)]
    prefix: Option<String>,
    #[serde(default = "start_at_one")]
    start: u64,
    #[serde(default)]
    dry_run: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FinishParams {
    finish: String,
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
fn start_at_one() -> u64 {
    1
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
        "project.replace" => {
            let a: ReplaceParams = parse(params)?;
            if !["all", "components", "nets", "text"].contains(&a.scope.as_str()) {
                return Err(Error::Invalid("unknown replace scope".into()));
            }
            if a.search == a.replace {
                return Err(Error::Invalid(
                    "the replacement is the same as the search text".into(),
                ));
            }
            let all = a.scope == "all";
            let wants_components = all || a.scope == "components";
            let wants_nets = all || a.scope == "nets";
            let wants_text = all || a.scope == "text";
            let mut hits: Vec<(Id, &str, usize)> = Vec::new();
            let mut new_refs: Vec<(Id, String)> = Vec::new();
            let mut new_names: Vec<(Id, String)> = Vec::new();
            if wants_components {
                for c in &p.components {
                    if c.reference.contains(&a.search) {
                        let next = c.reference.replace(&a.search, &a.replace);
                        if next.is_empty() || next.len() > 64 {
                            return Err(Error::Invalid(
                                "a replaced reference would be empty or over 64 characters".into(),
                            ));
                        }
                        new_refs.push((c.id, next));
                        hits.push((c.id, "reference", 1));
                    }
                    if c.value.contains(&a.search) {
                        let matches = c.value.matches(&a.search as &str).count();
                        let grown = a.replace.len() as isize - a.search.len() as isize;
                        if grown > 0 && c.value.len() + matches * grown as usize > 256 {
                            return Err(Error::Invalid(
                                "a replaced value would be over 256 characters".into(),
                            ));
                        }
                        hits.push((c.id, "value", matches));
                    }
                    for f in &c.fields {
                        if f.value.contains(&a.search) {
                            let matches = f.value.matches(&a.search as &str).count();
                            let grown = a.replace.len() as isize - a.search.len() as isize;
                            if grown > 0 && f.value.len() + matches * grown as usize > 256 {
                                return Err(Error::Invalid(
                                    "a replaced field value would be over 256 characters".into(),
                                ));
                            }
                            hits.push((c.id, "field", matches));
                        }
                    }
                }
                let mut seen: Vec<&str> = new_refs.iter().map(|(_, r)| r.as_str()).collect();
                let before = seen.len();
                seen.sort();
                seen.dedup();
                if seen.len() != before {
                    return Err(Error::Invalid(
                        "the replacements would collide on one component reference".into(),
                    ));
                }
                if p.components.iter().any(|c| {
                    !new_refs.iter().any(|(id, _)| *id == c.id)
                        && seen.contains(&c.reference.as_str())
                }) {
                    return Err(Error::Invalid(
                        "a replaced reference is already used by another component".into(),
                    ));
                }
            }
            if wants_nets {
                for n in &p.nets {
                    if n.name.contains(&a.search) {
                        let next = n.name.replace(&a.search, &a.replace);
                        if next.is_empty() || next.len() > 128 {
                            return Err(Error::Invalid(
                                "a replaced net name would be empty or over 128 characters".into(),
                            ));
                        }
                        new_names.push((n.id, next));
                        hits.push((n.id, "net", 1));
                    }
                }
                let mut seen: Vec<&str> = new_names.iter().map(|(_, r)| r.as_str()).collect();
                let before = seen.len();
                seen.sort();
                seen.dedup();
                if seen.len() != before {
                    return Err(Error::Invalid(
                        "the replacements would collide on one net name".into(),
                    ));
                }
                if p.nets.iter().any(|n| {
                    !new_names.iter().any(|(id, _)| *id == n.id) && seen.contains(&n.name.as_str())
                }) {
                    return Err(Error::Invalid(
                        "a replaced net name is already used by another net".into(),
                    ));
                }
            }
            if wants_text {
                for t in &p.board.texts {
                    if t.text.contains(&a.search) {
                        let matches = t.text.matches(&a.search as &str).count();
                        let grown = a.replace.len() as isize - a.search.len() as isize;
                        if grown > 0 && t.text.len() + matches * grown as usize > 128 {
                            return Err(Error::Invalid(
                                "a replaced board text would be over 128 characters".into(),
                            ));
                        }
                        hits.push((t.id, "text", matches));
                    }
                }
                for g in &p.graphics {
                    if g.text.contains(&a.search) {
                        let matches = g.text.matches(&a.search as &str).count();
                        let grown = a.replace.len() as isize - a.search.len() as isize;
                        if grown > 0 && g.text.len() + matches * grown as usize > 1_024 {
                            return Err(Error::Invalid(
                                "a replaced graphic text would be over 1024 characters".into(),
                            ));
                        }
                        hits.push((g.id, "graphic", matches));
                    }
                }
            }
            let total: usize = hits.iter().map(|(_, _, n)| n).sum();
            if total == 0 {
                return Err(Error::Invalid(format!(
                    "no project text matches \"{}\" in scope \"{}\"",
                    a.search, a.scope
                )));
            }
            let changes: Vec<Value> = hits
                .iter()
                .map(|(id, field, count)| json!({"id": id, "field": field, "matches": count}))
                .collect();
            if a.dry_run {
                return Ok(json!({"dry_run": true, "count": total, "changes": changes}));
            }
            for c in &mut p.components {
                if let Some((_, next)) = new_refs.iter().find(|(id, _)| *id == c.id) {
                    c.reference = next.clone();
                }
                if c.value.contains(&a.search) {
                    c.value = c.value.replace(&a.search, &a.replace);
                }
                for f in &mut c.fields {
                    if f.value.contains(&a.search) {
                        f.value = f.value.replace(&a.search, &a.replace);
                    }
                }
            }
            for n in &mut p.nets {
                if let Some((_, next)) = new_names.iter().find(|(id, _)| *id == n.id) {
                    n.name = next.clone();
                }
            }
            for t in &mut p.board.texts {
                if t.text.contains(&a.search) {
                    t.text = t.text.replace(&a.search, &a.replace);
                }
            }
            for g in &mut p.graphics {
                if g.text.contains(&a.search) {
                    g.text = g.text.replace(&a.search, &a.replace);
                }
            }
            Ok(json!({"dry_run": false, "count": total, "changes": changes}))
        }
        "project.set_meta" => {
            let a: MetaParams = parse(params)?;
            if a.key.is_empty()
                || a.key.len() > 32
                || !a
                    .key
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
            {
                return Err(Error::Invalid(
                    "metadata keys are 1 to 32 lowercase alphanumeric bytes".into(),
                ));
            }
            if a.value.len() > 4096 {
                return Err(Error::Invalid("metadata values fit 4 KiB".into()));
            }
            if p.meta.get(&a.key).is_some_and(|v| v == &a.value) {
                return Err(Error::Invalid(
                    "that metadata entry already holds that value".into(),
                ));
            }
            if !p.meta.contains_key(&a.key) && p.meta.len() >= 16 {
                return Err(Error::Invalid(
                    "a project records at most 16 metadata entries".into(),
                ));
            }
            p.meta.insert(a.key.clone(), a.value.clone());
            Ok(json!({"key": a.key, "value": a.value}))
        }
        "project.clear_meta" => {
            let a: MetaKeyParams = parse(params)?;
            if p.meta.remove(&a.key).is_none() {
                return Err(missing());
            }
            Ok(json!({"key": a.key}))
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
        "components.reannotate" => {
            let a: ReannotateParams = parse(params)?;
            if a.start == 0 || a.start > 1_000_000 {
                return Err(Error::Invalid("start must be 1-1000000".into()));
            }
            if a.prefix.as_ref().is_some_and(|w| {
                w.is_empty() || w.len() > 16 || !w.bytes().all(|b| b.is_ascii_alphabetic())
            }) {
                return Err(Error::Invalid("prefix must be 1-16 ASCII letters".into()));
            }
            let letters =
                |r: &str| -> String { r.chars().take_while(char::is_ascii_alphabetic).collect() };
            let mut groups: std::collections::BTreeMap<String, Vec<usize>> =
                std::collections::BTreeMap::new();
            for (i, c) in p.components.iter().enumerate() {
                let pre = letters(&c.reference);
                if pre.is_empty()
                    || a.prefix
                        .as_deref()
                        .is_some_and(|w| !w.eq_ignore_ascii_case(&pre))
                {
                    continue;
                }
                groups.entry(pre).or_default().push(i);
            }
            if groups.is_empty() {
                return Err(Error::Invalid("no letter-prefixed references match".into()));
            }
            for g in groups.values_mut() {
                g.sort_by(|x, y| {
                    let ca = &p.components[*x];
                    let cb = &p.components[*y];
                    ca.position
                        .y
                        .total_cmp(&cb.position.y)
                        .then_with(|| ca.position.x.total_cmp(&cb.position.x))
                        .then_with(|| ca.reference.cmp(&cb.reference))
                });
            }
            let in_group: std::collections::BTreeSet<usize> =
                groups.values().flatten().copied().collect();
            let taken: std::collections::BTreeSet<&str> = p
                .components
                .iter()
                .enumerate()
                .filter(|(i, _)| !in_group.contains(i))
                .map(|(_, c)| c.reference.as_str())
                .collect();
            let mut used: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
            let mut planned: Vec<(usize, Id, String, String)> = Vec::new();
            for (pre, g) in &groups {
                let pre = a.prefix.as_deref().unwrap_or(pre);
                for (n, i) in g.iter().enumerate() {
                    let to = format!("{pre}{}", a.start + n as u64);
                    if to.len() > 64 || taken.contains(to.as_str()) || !used.insert(to.clone()) {
                        return Err(Error::Invalid(format!("reference {to} is already in use")));
                    }
                    planned.push((
                        *i,
                        p.components[*i].id,
                        p.components[*i].reference.clone(),
                        to,
                    ));
                }
            }
            if !a.dry_run {
                for (i, _, _, to) in &planned {
                    p.components[*i].reference = to.clone();
                }
            }
            let renamed: Vec<Value> = planned
                .iter()
                .map(|(_, id, from, to)| json!({"id": id, "from": from, "to": to}))
                .collect();
            Ok(json!({"dry_run": a.dry_run, "count": renamed.len(), "renamed": renamed}))
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
        "net.tie" => {
            let a: NetTieParams = parse(params)?;
            if a.source == a.target {
                return Err(Error::Invalid("a net cannot be merged into itself".into()));
            }
            if !p.nets.iter().any(|n| n.id == a.source) || !p.nets.iter().any(|n| n.id == a.target)
            {
                return Err(missing());
            }
            let mut moved = 0;
            for c in p.components.iter_mut() {
                for pin in c.pins.iter_mut() {
                    if pin.net == Some(a.source) {
                        pin.net = Some(a.target);
                        moved += 1;
                    }
                }
            }
            for w in p.drawn_wires.iter_mut() {
                if w.net == Some(a.source) {
                    w.net = Some(a.target);
                    moved += 1;
                }
            }
            for w in p.wires.iter_mut() {
                if w.net == a.source {
                    w.net = a.target;
                }
            }
            for l in p.labels.iter_mut() {
                if l.net == a.source {
                    l.net = a.target;
                    moved += 1;
                }
            }
            for t in p.board.tracks.iter_mut() {
                if t.net == a.source {
                    t.net = a.target;
                    moved += 1;
                }
            }
            for v in p.board.vias.iter_mut() {
                if v.net == a.source {
                    v.net = a.target;
                    moved += 1;
                }
            }
            for z in p.board.zones.iter_mut() {
                if z.net == a.source {
                    z.net = a.target;
                    moved += 1;
                }
            }
            for class in p.net_classes.iter_mut() {
                if class.nets.contains(&a.source) {
                    class.nets.retain(|n| *n != a.source && *n != a.target);
                    class.nets.push(a.target);
                    class.nets.sort_unstable();
                    class.nets.dedup();
                }
            }
            for rule in p.board.pair_rules.iter_mut() {
                if rule.a == a.source {
                    rule.a = a.target;
                }
                if rule.b == a.source {
                    rule.b = a.target;
                }
            }
            p.board.pair_rules.retain(|r| r.a != r.b);
            p.nets.retain(|n| n.id != a.source);
            Ok(json!({"merged":a.source,"into":a.target,"moved":moved}))
        }
        "net.swap" => {
            let a: NetSwapParams = parse(params)?;
            if a.id_a == a.id_b {
                return Err(Error::Invalid("a net cannot be swapped with itself".into()));
            }
            let ia = p
                .nets
                .iter()
                .position(|n| n.id == a.id_a)
                .ok_or_else(missing)?;
            let ib = p
                .nets
                .iter()
                .position(|n| n.id == a.id_b)
                .ok_or_else(missing)?;
            let (na, nb) = p.nets.split_at_mut(ib.min(ia) + 1);
            let (a_name, b_name) = if ia < ib {
                (&mut na[ia].name, &mut nb[ib - ia - 1].name)
            } else {
                (&mut na[ib].name, &mut nb[ia - ib - 1].name)
            };
            std::mem::swap(a_name, b_name);
            Ok(json!({"id_a":a.id_a,"id_b":a.id_b}))
        }
        "pins.set_all_no_connect" => {
            let a: ComponentRef = parse(params)?;
            let component = p.component(a.component).ok_or_else(missing)?;
            let numbers: Vec<String> = component
                .pins
                .iter()
                .map(|pin| pin.number.clone())
                .collect();
            let mut flagged = 0;
            for number in numbers {
                if p.no_connects
                    .iter()
                    .any(|f| f.pin.component == a.component && f.pin.pin == number)
                {
                    continue;
                }
                let connected = p.wires.iter().any(|w| {
                    (w.from.component == a.component && w.from.pin == number)
                        || (w.to.component == a.component && w.to.pin == number)
                }) || p.components.iter().filter(|c| c.id == a.component).any(
                    |c| {
                        c.pins
                            .iter()
                            .any(|pin| pin.number == number && pin.net.is_some())
                    },
                );
                if connected {
                    continue;
                }
                let id = allocate(next)?;
                p.no_connects.push(pcbcraft_doc::NoConnectFlag {
                    id,
                    pin: PinRef {
                        component: a.component,
                        pin: number,
                    },
                });
                flagged += 1;
            }
            if flagged == 0 {
                return Err(Error::Invalid(
                    "no unassigned pins on that component need a no-connect flag".into(),
                ));
            }
            Ok(json!({"component":a.component,"flagged":flagged}))
        }
        "pins.clear_all_no_connect" => {
            let a: ComponentRef = parse(params)?;
            p.component(a.component).ok_or_else(missing)?;
            let before = p.no_connects.len();
            p.no_connects.retain(|f| f.pin.component != a.component);
            let cleared = before - p.no_connects.len();
            if cleared == 0 {
                return Err(Error::Invalid(
                    "that component has no no-connect flags to clear".into(),
                ));
            }
            Ok(json!({"component":a.component,"cleared":cleared}))
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
            import_nets(p, next, &a)
        }
        "netlist.import_session" => {
            let a: SessionImportParams = parse(params)?;
            if a.path.is_empty() || a.path.len() > 1024 {
                return Err(Error::Invalid("path must be 1 to 1024 bytes".into()));
            }
            let text = pcbcraft_io::read_text(std::path::Path::new(&a.path))?;
            let parsed = pcbcraft_library::read_specctra_session(&text)
                .map_err(|e| Error::Invalid(e.to_string()))?;
            let nets: Vec<NetlistNet> = parsed
                .into_iter()
                .map(|n| NetlistNet {
                    name: n.name,
                    pins: n
                        .pins
                        .into_iter()
                        .map(|(reference, pin)| NetlistPin { reference, pin })
                        .collect(),
                })
                .collect();
            import_nets(p, next, &NetlistImportParams { nets })
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
        "wire.move" => {
            let a: WireMoveParams = parse(params)?;
            if !a.delta.valid() {
                return Err(Error::Invalid("wire move is out of range".into()));
            }
            let wire = p
                .drawn_wires
                .iter()
                .find(|w| w.id == a.id)
                .ok_or_else(missing)?;
            let moved: Vec<Point> = wire
                .points
                .iter()
                .map(|point| Point::new(point.x + a.delta.x, point.y + a.delta.y))
                .collect();
            if !moved.iter().all(|point| point.valid()) {
                return Err(Error::Invalid("wire vertex is out of range".into()));
            }
            if moved == wire.points {
                return Err(Error::Invalid("wire move is a no-op".into()));
            }
            let old = wire.points.clone();
            let net = wire.net;
            let wire = p
                .drawn_wires
                .iter_mut()
                .find(|w| w.id == a.id)
                .ok_or_else(missing)?;
            wire.points = moved;
            for label in &mut p.labels {
                if label.kind != "junction"
                    || !net.is_some_and(|n| label.net == n)
                    || !old.contains(&label.position)
                {
                    continue;
                }
                label.position =
                    Point::new(label.position.x + a.delta.x, label.position.y + a.delta.y);
            }
            Ok(json!({"id": a.id}))
        }
        "wire.set_point" => {
            let a: WirePointParams = parse(params)?;
            if !a.position.valid() {
                return Err(Error::Invalid("wire vertex is out of range".into()));
            }
            let wire = p
                .drawn_wires
                .iter()
                .find(|w| w.id == a.id)
                .ok_or_else(missing)?;
            let at = usize::try_from(a.index).map_err(|_| missing())?;
            if at >= wire.points.len() {
                return Err(Error::Invalid("vertex index is out of range".into()));
            }
            let old = wire.points[at];
            if old == a.position {
                return Err(Error::Invalid("vertex is already there".into()));
            }
            if wire.points[..at].last().is_some_and(|p| *p == a.position)
                || wire.points[at + 1..]
                    .first()
                    .is_some_and(|p| *p == a.position)
            {
                return Err(Error::Invalid(
                    "the neighbouring vertex is already there; the segment would collapse".into(),
                ));
            }
            let mut moved = wire.points.clone();
            moved[at] = a.position;
            for label in p
                .labels
                .iter()
                .filter(|l| l.kind == "junction" && wire.net == Some(l.net))
            {
                if label.position == old && !moved.contains(&label.position) {
                    return Err(Error::Invalid(
                        "a junction sits on this vertex; move the junction first".into(),
                    ));
                }
            }
            let wire = p
                .drawn_wires
                .iter_mut()
                .find(|w| w.id == a.id)
                .ok_or_else(missing)?;
            wire.points = moved;
            Ok(json!({"id":a.id,"index":a.index,"position":a.position}))
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
        "graphic.group" => {
            let a: GraphicGroupParams = parse(params)?;
            if a.graphics.len() < 2 || a.graphics.len() > 512 {
                return Err(Error::Invalid("a group holds two to 512 graphics".into()));
            }
            for id in &a.graphics {
                if id == &0
                    || !p.graphics.iter().any(|g| g.id == *id)
                    || p.graphic_groups.iter().any(|g| g.graphics.contains(id))
                {
                    return Err(Error::Invalid(
                        "every grouped graphic must exist and be in no other group".into(),
                    ));
                }
            }
            let unique: std::collections::BTreeSet<Id> = a.graphics.iter().copied().collect();
            if unique.len() != a.graphics.len() {
                return Err(Error::Invalid("a graphic joins a group once".into()));
            }
            if p.graphic_groups.len() >= 256 {
                return Err(Error::Invalid(
                    "a project defines at most 256 graphic groups".into(),
                ));
            }
            let id = allocate(next)?;
            p.graphic_groups.push(pcbcraft_doc::GraphicGroup {
                id,
                graphics: a.graphics.clone(),
            });
            Ok(json!({"id": id, "count": a.graphics.len()}))
        }
        "graphic.ungroup" => {
            let a: GraphicGroupIdParams = parse(params)?;
            let index = p
                .graphic_groups
                .iter()
                .position(|g| g.id == a.id)
                .ok_or_else(missing)?;
            p.graphic_groups.remove(index);
            Ok(json!({"id": a.id}))
        }
        "graphic.translate" => {
            let a: GraphicTranslateParams = parse(params)?;
            let delta = a.delta;
            let members: Vec<Id> = p
                .graphic_groups
                .iter()
                .find(|g| g.id == a.id)
                .ok_or_else(missing)?
                .graphics
                .clone();
            let mut moved_all = Vec::new();
            for id in &members {
                let graphic = p
                    .graphics
                    .iter_mut()
                    .find(|g| g.id == *id)
                    .ok_or_else(missing)?;
                let moved: Vec<Point> = graphic
                    .points
                    .iter()
                    .map(|point| Point::new(point.x + delta.x, point.y + delta.y))
                    .collect();
                if !moved.iter().all(|point| point.valid()) {
                    return Err(Error::Invalid("graphic vertex is out of range".into()));
                }
                moved_all.push((graphic.id, moved));
            }
            for (id, moved) in moved_all {
                if let Some(graphic) = p.graphics.iter_mut().find(|g| g.id == id) {
                    graphic.points = moved;
                }
            }
            Ok(json!({"id": a.id, "moved": members.len()}))
        }
        "dimension.add" => {
            let a: DimensionParams = parse(params)?;
            if a.layer != "schematic" && a.layer != "board" {
                return Err(Error::Invalid(
                    "dimension layer must be schematic or board".into(),
                ));
            }
            if !a.start.valid() || !a.end.valid() || a.start == a.end {
                return Err(Error::Invalid(
                    "dimension ends must be distinct points in range".into(),
                ));
            }
            let offset = a.offset.unwrap_or(2.0);
            if !offset.is_finite() || offset.abs() > 100.0 {
                return Err(Error::Invalid(
                    "dimension offset must be within 100mm".into(),
                ));
            }
            if p.graphics.len() + 6 > 4096 || p.graphic_groups.len() >= 256 {
                return Err(Error::Invalid(
                    "the drawing has no room for another dimension".into(),
                ));
            }
            let dx = a.end.x - a.start.x;
            let dy = a.end.y - a.start.y;
            let distance = dx.hypot(dy);
            let (ux, uy) = (dx / distance, dy / distance);
            let (nx, ny) = (-uy, ux);
            let witness =
                |p: Point, reach: f64| vec![p, Point::new(p.x + nx * reach, p.y + ny * reach)];
            let line_at = |p: Point| Point::new(p.x + nx * offset, p.y + ny * offset);
            let reach = offset + 0.5;
            let pieces: [(&str, Vec<Point>, f64, String); 4] = [
                ("line", witness(a.start, reach), 0.15, String::new()),
                ("line", witness(a.end, reach), 0.15, String::new()),
                (
                    "line",
                    vec![line_at(a.start), line_at(a.end)],
                    0.15,
                    String::new(),
                ),
                (
                    "text",
                    vec![Point::new(
                        (a.start.x + a.end.x) / 2.0 + nx * (offset + 0.8),
                        (a.start.y + a.end.y) / 2.0 + ny * (offset + 0.8),
                    )],
                    2.5,
                    format!("{distance:.2}"),
                ),
            ];
            let mut ids = Vec::new();
            for (shape, points, width, text) in pieces {
                if !points.iter().all(|point| point.valid()) {
                    return Err(Error::Invalid("dimension vertex is out of range".into()));
                }
                let id = allocate(next)?;
                p.graphics.push(pcbcraft_doc::Graphic {
                    id,
                    layer: a.layer.clone(),
                    shape: shape.into(),
                    points,
                    width,
                    text,
                });
                ids.push(id);
            }
            let group = allocate(next)?;
            p.graphic_groups.push(pcbcraft_doc::GraphicGroup {
                id: group,
                graphics: ids,
            });
            Ok(json!({
                "id": group,
                "distance": (distance * 100.0).round() / 100.0,
            }))
        }
        "dimension.radius" | "dimension.diameter" => {
            let diameter = command == "dimension.diameter";
            let a: RadialParams = parse(params)?;
            if a.layer != "schematic" && a.layer != "board" {
                return Err(Error::Invalid(
                    "dimension layer must be schematic or board".into(),
                ));
            }
            if !a.center.valid() || !a.point.valid() || a.center == a.point {
                return Err(Error::Invalid(
                    "dimension centre and point must be distinct points in range".into(),
                ));
            }
            let radius = (a.point.x - a.center.x).hypot(a.point.y - a.center.y);
            if p.graphics.len() + 3 > 4096 || p.graphic_groups.len() >= 256 {
                return Err(Error::Invalid(
                    "the drawing has no room for another dimension".into(),
                ));
            }
            let (ux, uy) = (
                (a.point.x - a.center.x) / radius,
                (a.point.y - a.center.y) / radius,
            );
            let near = Point::new(a.center.x - ux * radius, a.center.y - uy * radius);
            let label = Point::new(
                a.center.x + ux * (radius + 0.8),
                a.center.y + uy * (radius + 0.8),
            );
            let pieces: [(&str, Vec<Point>, f64, String); 3] = [
                ("circle", vec![a.center], radius, String::new()),
                (
                    "line",
                    if diameter {
                        vec![near, a.point]
                    } else {
                        vec![a.center, a.point]
                    },
                    0.15,
                    String::new(),
                ),
                (
                    "text",
                    vec![label],
                    2.5,
                    if diameter {
                        format!("\u{d8} {:.2}", 2.0 * radius)
                    } else {
                        format!("R {:.2}", radius)
                    },
                ),
            ];
            let mut ids = Vec::new();
            for (shape, points, width, text) in pieces {
                if !points.iter().all(|point| point.valid()) {
                    return Err(Error::Invalid("dimension vertex is out of range".into()));
                }
                let id = allocate(next)?;
                p.graphics.push(pcbcraft_doc::Graphic {
                    id,
                    layer: a.layer.clone(),
                    shape: shape.into(),
                    points,
                    width,
                    text,
                });
                ids.push(id);
            }
            let group = allocate(next)?;
            p.graphic_groups.push(pcbcraft_doc::GraphicGroup {
                id: group,
                graphics: ids,
            });
            Ok(json!({
                "id": group,
                "radius": (radius * 100.0).round() / 100.0,
            }))
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
        "barcode.place" => {
            let a: BarcodeParams = parse(params)?;
            let layer = a.layer.unwrap_or_else(|| "board".to_string());
            if layer != "schematic" && layer != "board" {
                return Err(Error::Invalid(
                    "barcode layer must be schematic or board".into(),
                ));
            }
            let height = a.height.unwrap_or(3.0);
            if !height.is_finite() || height < 0.5 || height > 50.0 {
                return Err(Error::Invalid(
                    "barcode height must be between 0.5 and 50 mm".into(),
                ));
            }
            let module = a.module.unwrap_or(0.25);
            if !module.is_finite() || module < 0.05 || module > 2.0 {
                return Err(Error::Invalid(
                    "barcode module width must be between 0.05 and 2 mm".into(),
                ));
            }
            let text = a.text.trim().to_uppercase();
            if text.is_empty() || text.len() > 48 {
                return Err(Error::Invalid(
                    "barcode text must be 1 to 48 characters".into(),
                ));
            }
            if !a.position.valid() {
                return Err(Error::Invalid("barcode position is out of range".into()));
            }
            let mut bars: Vec<(f64, f64)> = Vec::new();
            let mut x = 0.0f64;
            for sym in std::iter::once('*')
                .chain(text.chars())
                .chain(std::iter::once('*'))
            {
                let Some((_, pattern)) = CODE39.iter().find(|(c, _)| *c == sym) else {
                    return Err(Error::Invalid(format!(
                        "a Code 39 barcode cannot show {sym}"
                    )));
                };
                for (i, element) in pattern.chars().enumerate() {
                    let w = if element == 'w' { 3.0 * module } else { module };
                    if i % 2 == 0 {
                        bars.push((x + w / 2.0, w));
                    }
                    x += w;
                }
                x += module;
            }
            let top = a.position.y + height;
            if !Point::new(a.position.x + x, top).valid() {
                return Err(Error::Invalid("the barcode extends out of range".into()));
            }
            if p.graphics.len() + bars.len() > 4096 {
                return Err(Error::Invalid(
                    "the drawing holds at most 4096 graphics".into(),
                ));
            }
            let mut count = 0usize;
            for (cx, w) in bars {
                let id = allocate(next)?;
                p.graphics.push(pcbcraft_doc::Graphic {
                    id,
                    layer: layer.clone(),
                    shape: "line".into(),
                    points: vec![
                        Point::new(a.position.x + cx, a.position.y),
                        Point::new(a.position.x + cx, top),
                    ],
                    width: w,
                    text: String::new(),
                });
                count += 1;
            }
            Ok(json!({"count": count, "width": x - module}))
        }
        "schematic.table" => {
            let a: SchematicTableParams = parse(params)?;
            if !a.position.valid() {
                return Err(Error::Invalid("table position is out of range".into()));
            }
            if a.fields.len() > 3 {
                return Err(Error::Invalid("a table takes at most 3 columns".into()));
            }
            for key in &a.fields {
                if key.is_empty() || key.len() > 32 {
                    return Err(Error::Invalid("table columns must be 1 to 32 bytes".into()));
                }
            }
            if a.max_rows.is_some_and(|n| n == 0 || n > 200) {
                return Err(Error::Invalid(
                    "row counts must be between 1 and 200".into(),
                ));
            }
            let fields: Vec<String> = if a.fields.is_empty() {
                vec!["reference".into(), "value".into()]
            } else {
                a.fields.clone()
            };
            let max_rows = a.max_rows.unwrap_or(64).clamp(1, 200);
            let cell = |c: &pcbcraft_doc::Component, key: &str| -> String {
                match key {
                    "reference" => c.reference.clone(),
                    "value" => c.value.clone(),
                    other => c.field(other).unwrap_or("").to_string(),
                }
            };
            let mut rows: Vec<Vec<String>> = p
                .components
                .iter()
                .map(|c| fields.iter().map(|key| cell(c, key)).collect())
                .collect();
            rows.sort_by(|x, y| x.first().cmp(&y.first()));
            let truncated = rows.len() > max_rows;
            rows.truncate(max_rows);
            let (col_w, row_h, text_size) = (15.0, 4.0, 2.5);
            let width = col_w * fields.len() as f64;
            let height = row_h * (rows.len() + 1) as f64;
            if !Point::new(a.position.x + width, a.position.y - height).valid() {
                return Err(Error::Invalid("the table extends out of range".into()));
            }
            let lines = (rows.len() + 2) + (fields.len() + 1);
            let texts = rows.iter().fold(1usize, |n, r| {
                n + r.iter().filter(|t| !t.is_empty()).count()
            });
            if p.graphics.len() + lines + texts > 4096 {
                return Err(Error::Invalid(
                    "the drawing holds at most 4096 graphics".into(),
                ));
            }
            let add = |shape: &str,
                       points: Vec<Point>,
                       width: f64,
                       text: String,
                       p: &mut pcbcraft_doc::Project,
                       next: &mut Id|
             -> Result<(), Error> {
                let id = allocate(next)?;
                p.graphics.push(pcbcraft_doc::Graphic {
                    id,
                    layer: "schematic".into(),
                    shape: shape.into(),
                    points,
                    width,
                    text,
                });
                Ok(())
            };
            for r in 0..=rows.len() + 1 {
                let y = a.position.y - row_h * r as f64;
                add(
                    "line",
                    vec![
                        Point::new(a.position.x, y),
                        Point::new(a.position.x + width, y),
                    ],
                    0.15,
                    String::new(),
                    p,
                    next,
                )?;
            }
            for c in 0..=fields.len() {
                let x = a.position.x + col_w * c as f64;
                add(
                    "line",
                    vec![
                        Point::new(x, a.position.y),
                        Point::new(x, a.position.y - height),
                    ],
                    0.15,
                    String::new(),
                    p,
                    next,
                )?;
            }
            for (r, row) in std::iter::once(None)
                .chain(rows.iter().map(Some))
                .enumerate()
            {
                for (i, key) in fields.iter().enumerate() {
                    let text = match row {
                        None => key.clone(),
                        Some(values) => values[i].clone(),
                    };
                    if text.is_empty() {
                        continue;
                    }
                    add(
                        "text",
                        vec![Point::new(
                            a.position.x + col_w * i as f64 + 1.0,
                            a.position.y - row_h * r as f64 - 2.8,
                        )],
                        text_size,
                        text,
                        p,
                        next,
                    )?;
                }
            }
            Ok(json!({
                "rows": rows.len(),
                "columns": fields.len(),
                "truncated": truncated,
                "graphics": lines + texts,
            }))
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
        "label.set" => {
            let a: LabelSetParams = parse(params)?;
            if a.kind.is_none() && a.net.is_none() {
                return Err(Error::Invalid("provide a kind or net to change".into()));
            }
            if a.kind
                .as_ref()
                .is_some_and(|k| !pcbcraft_doc::Label::KINDS.iter().any(|x| *x == k))
            {
                return Err(Error::Invalid("unknown label kind".into()));
            }
            if a.net.is_some_and(|n| !p.nets.iter().any(|x| x.id == n)) {
                return Err(missing());
            }
            let index = p
                .labels
                .iter()
                .position(|l| l.id == a.id)
                .ok_or_else(missing)?;
            let kind = a.kind.unwrap_or_else(|| p.labels[index].kind.clone());
            let net = a.net.unwrap_or_else(|| p.labels[index].net);
            if kind == p.labels[index].kind && net == p.labels[index].net {
                return Err(Error::Invalid(
                    "that label already has that kind and net".into(),
                ));
            }
            if kind == "junction"
                && !p
                    .drawn_wires
                    .iter()
                    .any(|w| w.net == Some(net) && w.points.contains(&p.labels[index].position))
            {
                return Err(Error::Invalid(
                    "junction must sit on a vertex of a drawn wire of its net".into(),
                ));
            }
            p.labels[index].kind = kind;
            p.labels[index].net = net;
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
        "board.hole_place" => {
            let a: HolePlaceParams = parse(params)?;
            if !a.position.valid() {
                return Err(Error::Invalid("hole position is out of range".into()));
            }
            if !(0.05..=25.0).contains(&a.diameter) {
                return Err(Error::Invalid(
                    "hole drill must be within 0.05 to 25 mm".into(),
                ));
            }
            if p.board.holes.len() >= 512 {
                return Err(Error::Invalid(
                    "a board carries at most 512 unplated holes".into(),
                ));
            }
            let id = allocate(next)?;
            p.board.holes.push(pcbcraft_doc::Hole {
                id,
                position: a.position,
                diameter: a.diameter,
            });
            Ok(json!({"id":id}))
        }
        "board.hole_move" => {
            let a: Move = parse(params)?;
            let hole = p
                .board
                .holes
                .iter_mut()
                .find(|h| h.id == a.id)
                .ok_or_else(missing)?;
            if !a.position.valid() {
                return Err(Error::Invalid("hole position is out of range".into()));
            }
            hole.position = a.position;
            Ok(json!({"id":a.id}))
        }
        "board.hole_set_size" => {
            let a: HoleSizeParams = parse(params)?;
            let hole = p
                .board
                .holes
                .iter_mut()
                .find(|h| h.id == a.id)
                .ok_or_else(missing)?;
            if !(0.05..=25.0).contains(&a.diameter) {
                return Err(Error::Invalid(
                    "hole drill must be within 0.05 to 25 mm".into(),
                ));
            }
            hole.diameter = a.diameter;
            Ok(json!({"id":a.id,"diameter":a.diameter}))
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
                filled: true,
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
        "zone.set_layer" => {
            let a: ZoneLayerParams = parse(params)?;
            let zone = p
                .board
                .zones
                .iter_mut()
                .find(|z| z.id == a.id)
                .ok_or_else(missing)?;
            if zone.layer == a.layer {
                return Err(Error::Invalid("that zone is already on the layer".into()));
            }
            zone.layer = a.layer;
            Ok(json!({"id":a.id,"layer":zone.layer}))
        }
        "zone.set_point" => {
            let a: ZonePointParams = parse(params)?;
            if !a.position.valid() {
                return Err(Error::Invalid("zone vertex is out of range".into()));
            }
            let zone = p
                .board
                .zones
                .iter_mut()
                .find(|z| z.id == a.id)
                .ok_or_else(missing)?;
            let outline = match a.cutout {
                None => &mut zone.outline,
                Some(which) => zone.cutouts.get_mut(which).ok_or_else(|| {
                    Error::Invalid("cutout index is outside this zone's cutouts".into())
                })?,
            };
            if a.index >= outline.len() {
                return Err(Error::Invalid(
                    "vertex index is outside this outline".into(),
                ));
            }
            let last = outline.len() - 1;
            outline[a.index] = a.position;
            if a.index == 0 {
                outline[last] = a.position;
            } else if a.index == last {
                outline[0] = a.position;
            }
            validate_closed_outline(outline).map_err(Error::Invalid)?;
            Ok(json!({"id":a.id,"cutout":a.cutout,"index":a.index,"position":a.position}))
        }
        "zone.insert_point" => {
            let a: ZoneInsertParams = parse(params)?;
            if !a.position.valid() {
                return Err(Error::Invalid("zone vertex is out of range".into()));
            }
            let zone = p
                .board
                .zones
                .iter_mut()
                .find(|z| z.id == a.id)
                .ok_or_else(missing)?;
            let outline = match a.cutout {
                None => &mut zone.outline,
                Some(which) => zone.cutouts.get_mut(which).ok_or_else(|| {
                    Error::Invalid("cutout index is outside this zone's cutouts".into())
                })?,
            };
            if outline.len() >= 257 {
                return Err(Error::Invalid(
                    "this outline already holds the most vertices".into(),
                ));
            }
            if a.index > outline.len() - 1 {
                return Err(Error::Invalid(
                    "vertex index is outside this outline".into(),
                ));
            }
            outline.insert(a.index, a.position);
            if a.index == 0 {
                let last = outline.len() - 1;
                outline[last] = a.position;
            }
            validate_closed_outline(outline).map_err(Error::Invalid)?;
            Ok(json!({"id":a.id,"cutout":a.cutout,"index":a.index,"position":a.position}))
        }
        "zone.remove_point" => {
            let a: ZoneRemoveParams = parse(params)?;
            let zone = p
                .board
                .zones
                .iter_mut()
                .find(|z| z.id == a.id)
                .ok_or_else(missing)?;
            let outline = match a.cutout {
                None => &mut zone.outline,
                Some(which) => zone.cutouts.get_mut(which).ok_or_else(|| {
                    Error::Invalid("cutout index is outside this zone's cutouts".into())
                })?,
            };
            if a.index > outline.len() - 2 {
                return Err(Error::Invalid(
                    "vertex index is outside this outline".into(),
                ));
            }
            if outline.len() <= 4 {
                return Err(Error::Invalid(
                    "an outline needs at least three vertices".into(),
                ));
            }
            outline.remove(a.index);
            let last = outline.len() - 1;
            outline[last] = outline[0];
            validate_closed_outline(outline).map_err(Error::Invalid)?;
            Ok(json!({"id":a.id,"cutout":a.cutout,"index":a.index}))
        }
        "zone.set_fill" => {
            let a: ZoneFillParams = parse(params)?;
            let zone = p
                .board
                .zones
                .iter_mut()
                .find(|z| z.id == a.id)
                .ok_or_else(missing)?;
            if zone.filled == a.filled {
                return Err(Error::Invalid(if a.filled {
                    "that zone is already filled".into()
                } else {
                    "that zone is already an outline only".into()
                }));
            }
            zone.filled = a.filled;
            Ok(json!({"id": a.id, "filled": a.filled}))
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
                            "castellated": pad.castellated.unwrap_or(false),
                        }),
                    ));
                }
            }
            rows.sort_by(|left, right| (&left.0, &left.1).cmp(&(&right.0, &right.1)));
            let pads: Vec<Value> = rows.into_iter().map(|(_, _, row)| row).collect();
            let count = pads.len();
            Ok(json!({"pads": pads, "count": count, "unassigned": unassigned}))
        }
        "items.update" => {
            let a: ItemsUpdateParams = parse(params)?;
            if a.items.is_empty() || a.items.len() > 128 {
                return Err(Error::Invalid(
                    "items.update updates 1 to 128 objects".into(),
                ));
            }
            let mut changed = 0usize;
            for item in &a.items {
                if item.net.is_none()
                    && item.layer.is_none()
                    && item.width.is_none()
                    && item.diameter.is_none()
                    && item.drill.is_none()
                    && item.text.is_none()
                    && item.size.is_none()
                {
                    return Err(Error::Invalid(
                        "an update entry changes at least one property".into(),
                    ));
                }
                if item
                    .net
                    .is_some_and(|net| !p.nets.iter().any(|n| n.id == net))
                {
                    return Err(Error::Invalid("an update names an unknown net".into()));
                }
                if item
                    .width
                    .is_some_and(|w| !w.is_finite() || w <= 0.0 || w > 100.0)
                {
                    return Err(Error::Invalid("width must be within 0 to 100mm".into()));
                }
                if item
                    .size
                    .is_some_and(|s| !s.is_finite() || s < 0.1 || s > 50.0)
                {
                    return Err(Error::Invalid(
                        "text size must be between 0.1 and 50 mm".into(),
                    ));
                }
                if item
                    .text
                    .as_ref()
                    .is_some_and(|t| t.is_empty() || t.len() > 128)
                {
                    return Err(Error::Invalid("label text must be 1 to 128 bytes".into()));
                }
                if let Some(track) = p.board.tracks.iter_mut().find(|t| t.id == item.id) {
                    if item.diameter.is_some()
                        || item.drill.is_some()
                        || item.text.is_some()
                        || item.size.is_some()
                    {
                        return Err(Error::Invalid(
                            "tracks take net, copper layer and width only".into(),
                        ));
                    }
                    if let Some(net) = item.net {
                        track.net = net;
                    }
                    if let Some(layer) = item.layer {
                        track.layer = layer;
                    }
                    if let Some(width) = item.width {
                        track.width = width;
                    }
                    changed += 1;
                } else if let Some(via) = p.board.vias.iter_mut().find(|v| v.id == item.id) {
                    if item.width.is_some() || item.text.is_some() || item.size.is_some() {
                        return Err(Error::Invalid(
                            "vias take net, diameter and drill only".into(),
                        ));
                    }
                    let diameter = item.diameter.unwrap_or(via.diameter);
                    let drill = item.drill.unwrap_or(via.drill);
                    if !diameter.is_finite() || diameter <= 0.0 || diameter > 100.0 {
                        return Err(Error::Invalid(
                            "via diameter must be within 0 to 100mm".into(),
                        ));
                    }
                    if !drill.is_finite() || drill < 0.0 || drill >= diameter {
                        return Err(Error::Invalid(
                            "via drill must be smaller than its pad".into(),
                        ));
                    }
                    if let Some(net) = item.net {
                        via.net = net;
                    }
                    via.diameter = diameter;
                    via.drill = drill;
                    changed += 1;
                } else if let Some(label) = p.board.texts.iter_mut().find(|t| t.id == item.id) {
                    if item.width.is_some() || item.diameter.is_some() || item.drill.is_some() {
                        return Err(Error::Invalid(
                            "labels take text, copper layer and size only".into(),
                        ));
                    }
                    if let Some(text) = item.text.clone() {
                        label.text = text;
                    }
                    if let Some(layer) = item.layer {
                        label.layer = layer;
                    }
                    if let Some(size) = item.size {
                        label.size = size;
                    }
                    changed += 1;
                } else {
                    return Err(Error::Invalid(format!(
                        "no board object has id {}",
                        item.id
                    )));
                }
            }
            Ok(json!({"changed": changed}))
        }
        "drill.chart" => {
            if params.as_object().is_none_or(|m| !m.is_empty()) {
                return Err(Error::Invalid("drill.chart takes an empty object".into()));
            }
            let mut groups: std::collections::BTreeMap<(i64, i64, i64), usize> =
                std::collections::BTreeMap::new();
            for v in &p.board.vias {
                if v.drill > 0.0 {
                    let d = (v.drill * 1000.0).round() as i64;
                    *groups.entry((0, d, d)).or_default() += 1;
                }
            }
            for f in &p.board.footprints {
                for pad in &f.pads {
                    let Some(size) = pad.geometry.as_ref().and_then(|g| g.drill.as_ref()) else {
                        continue;
                    };
                    let w = (size.size.x * 1000.0).round() as i64;
                    let h = (size.size.y * 1000.0).round() as i64;
                    if w > 0 && h > 0 {
                        let kind = if w == h { 0 } else { 1 };
                        *groups.entry((kind, w, h)).or_default() += 1;
                    }
                }
            }
            let mut holes: Vec<Value> = Vec::new();
            let mut total = 0usize;
            for ((kind, w, h), count) in groups {
                total += count;
                let w = w as f64 / 1000.0;
                let h = h as f64 / 1000.0;
                holes.push(if kind == 0 {
                    json!({"shape":"round","diameter":w,"count":count})
                } else {
                    json!({"shape":"oval","width":w,"height":h,"count":count})
                });
            }
            Ok(json!({"holes": holes, "total": total, "board_thickness": p.board.thickness}))
        }
        "board.bounds" => {
            if params.as_object().is_none_or(|m| !m.is_empty()) {
                return Err(Error::Invalid("board.bounds takes an empty object".into()));
            }
            #[derive(Default)]
            struct Extent {
                min_x: f64,
                min_y: f64,
                max_x: f64,
                max_y: f64,
                count: usize,
            }
            impl Extent {
                fn add(&mut self, x: f64, y: f64) {
                    if self.count == 0 {
                        self.min_x = x;
                        self.min_y = y;
                        self.max_x = x;
                        self.max_y = y;
                    } else {
                        self.min_x = self.min_x.min(x);
                        self.min_y = self.min_y.min(y);
                        self.max_x = self.max_x.max(x);
                        self.max_y = self.max_y.max(y);
                    }
                    self.count += 1;
                }
            }
            fn box_json(b: &Extent) -> Value {
                if b.count == 0 {
                    return json!({"empty": true});
                }
                json!({
                    "min_x": b.min_x,
                    "min_y": b.min_y,
                    "max_x": b.max_x,
                    "max_y": b.max_y,
                    "width": b.max_x - b.min_x,
                    "height": b.max_y - b.min_y,
                    "points": b.count
                })
            }
            let mut front = Extent::default();
            let mut back = Extent::default();
            let mut all = Extent::default();
            for f in &p.board.footprints {
                front.add(f.position.x, f.position.y);
                all.add(f.position.x, f.position.y);
                for pad in &f.pads {
                    let world = f.position.offset(pad.offset.rotate(-f.rotation));
                    let radius = pad
                        .geometry
                        .as_ref()
                        .map(|g| g.size.x.max(g.size.y) / 2.0)
                        .unwrap_or(0.0);
                    front.add(world.x - radius, world.y - radius);
                    front.add(world.x + radius, world.y + radius);
                    all.add(world.x - radius, world.y - radius);
                    all.add(world.x + radius, world.y + radius);
                }
            }
            for t in &p.board.tracks {
                let half = t.width / 2.0;
                let e = if t.layer == pcbcraft_doc::Layer::Back {
                    &mut back
                } else {
                    &mut front
                };
                e.add(t.start.x - half, t.start.y - half);
                e.add(t.end.x + half, t.end.y + half);
                all.add(t.start.x - half, t.start.y - half);
                all.add(t.end.x + half, t.end.y + half);
            }
            for v in &p.board.vias {
                let radius = v.diameter / 2.0;
                front.add(v.position.x - radius, v.position.y - radius);
                front.add(v.position.x + radius, v.position.y + radius);
                all.add(v.position.x - radius, v.position.y - radius);
                all.add(v.position.x + radius, v.position.y + radius);
            }
            for z in &p.board.zones {
                let e = if z.layer == pcbcraft_doc::Layer::Back {
                    &mut back
                } else {
                    &mut front
                };
                for q in &z.outline {
                    e.add(q.x, q.y);
                    all.add(q.x, q.y);
                }
            }
            for t in &p.board.texts {
                let e = if t.layer == pcbcraft_doc::Layer::Back {
                    &mut back
                } else {
                    &mut front
                };
                e.add(t.position.x, t.position.y);
                e.add(t.position.x + t.size, t.position.y + t.size);
                all.add(t.position.x, t.position.y);
                all.add(t.position.x + t.size, t.position.y + t.size);
            }
            Ok(json!({
                "overall": box_json(&all),
                "front": box_json(&front),
                "back": box_json(&back),
                "board": {"width": p.board.width, "height": p.board.height}
            }))
        }
        "items.find" => {
            let a: ItemFindParams = parse(params)?;
            if a.net.is_some_and(|id| !p.nets.iter().any(|n| n.id == id)) {
                return Err(Error::Invalid("unknown net".into()));
            }
            let layer = match a.layer.as_deref() {
                None => None,
                Some("F.Cu") => Some(pcbcraft_doc::Layer::Front),
                Some("B.Cu") => Some(pcbcraft_doc::Layer::Back),
                _ => return Err(Error::Invalid("layer must be one of F.Cu, B.Cu".into())),
            };
            let matches_layer = |l: &pcbcraft_doc::Layer| layer.is_none_or(|w| *l == w);
            let refs: std::collections::BTreeMap<Id, String> = p
                .components
                .iter()
                .map(|c| (c.id, c.reference.clone()))
                .collect();
            let mut found: Vec<Value> = Vec::new();
            match a.kind.as_str() {
                "track" => {
                    for t in &p.board.tracks {
                        if a.net.is_some_and(|n| t.net != n) || !matches_layer(&t.layer) {
                            continue;
                        }
                        found.push(json!({"id": t.id, "net": t.net, "layer": t.layer}));
                    }
                }
                "via" => {
                    for v in &p.board.vias {
                        if a.net.is_some_and(|n| v.net != n) {
                            continue;
                        }
                        found.push(json!({"id": v.id, "net": v.net, "position": v.position}));
                    }
                }
                "zone" => {
                    for z in &p.board.zones {
                        if a.net.is_some_and(|n| z.net != n) || !matches_layer(&z.layer) {
                            continue;
                        }
                        found.push(
                            json!({"id": z.id, "net": z.net, "layer": z.layer, "filled": z.filled}),
                        );
                    }
                }
                "pad" => {
                    for f in &p.board.footprints {
                        for pad in &f.pads {
                            if a.net.is_some_and(|n| pad.net != Some(n)) {
                                continue;
                            }
                            found.push(json!({
                                "id": f.id,
                                "reference": refs.get(&f.component).cloned().unwrap_or_default(),
                                "pad": pad.number
                            }));
                        }
                    }
                }
                "footprint" => {
                    for f in &p.board.footprints {
                        found.push(json!({
                            "id": f.id,
                            "reference": refs.get(&f.component).cloned().unwrap_or_default()
                        }));
                    }
                }
                "text" => {
                    for t in &p.board.texts {
                        if !matches_layer(&t.layer) {
                            continue;
                        }
                        found.push(json!({"id": t.id, "text": t.text, "layer": t.layer}));
                    }
                }
                "component" => {
                    for c in &p.components {
                        if a.net
                            .is_some_and(|n| !c.pins.iter().any(|q| q.net == Some(n)))
                        {
                            continue;
                        }
                        found.push(json!({"id": c.id, "reference": c.reference}));
                    }
                }
                _ => {
                    return Err(Error::Invalid(
                        "kind must be one of track, via, zone, pad, footprint, text, component"
                            .into(),
                    ));
                }
            }
            Ok(json!({"kind": a.kind, "count": found.len(), "items": found}))
        }
        "board.statistics" => {
            if params.as_object().is_none_or(|m| !m.is_empty()) {
                return Err(Error::Invalid(
                    "board.statistics takes an empty object".into(),
                ));
            }
            let mut front_len = 0.0;
            let mut back_len = 0.0;
            let mut front_tracks = 0usize;
            let mut back_tracks = 0usize;
            for t in &p.board.tracks {
                let len = t.length();
                if t.layer == Layer::Front {
                    front_len += len;
                    front_tracks += 1;
                } else {
                    back_len += len;
                    back_tracks += 1;
                }
            }
            let poured = p.board.zones.iter().filter(|z| z.filled).count();
            let mut holes = p.board.vias.iter().filter(|v| v.drill > 0.0).count();
            for f in &p.board.footprints {
                for pad in &f.pads {
                    if pad
                        .geometry
                        .as_ref()
                        .and_then(|g| g.drill.as_ref())
                        .is_some_and(|d| d.size.x > 0.0 && d.size.y > 0.0)
                    {
                        holes += 1;
                    }
                }
            }
            Ok(json!({
                "footprints": p.board.footprints.len(),
                "tracks": {"front": front_tracks, "back": back_tracks},
                "routed_length": {
                    "front": front_len,
                    "back": back_len,
                    "total": front_len + back_len
                },
                "vias": p.board.vias.len(),
                "zones": {"poured": poured, "outline_only": p.board.zones.len() - poured},
                "cutouts": p.board.cutouts.len(),
                "texts": p.board.texts.len(),
                "drilled_holes": holes,
                "board_area": p.board.outline.as_ref().map_or_else(
                    || p.board.width * p.board.height,
                    |o| pcbcraft_doc::outline_area(o),
                ),
                "finish": p.board.finish
            }))
        }
        "board.set_finish" => {
            let a: FinishParams = parse(params)?;
            if a.finish.is_empty() || a.finish.len() > 64 {
                return Err(Error::Invalid("finish must be 1-64 bytes of text".into()));
            }
            p.board.finish = a.finish.clone();
            Ok(json!({"finish": p.board.finish}))
        }
        "board.set_stackup" => {
            let a: StackupParams = parse(params)?;
            if a.thickness.is_none() && a.er.is_none() {
                return Err(Error::Invalid(
                    "give a thickness, a permittivity, or both".into(),
                ));
            }
            if a.thickness.is_some_and(|v| !(0.2..=100.0).contains(&v)) {
                return Err(Error::Invalid("thickness must be 0.2-100 mm".into()));
            }
            if a.er.is_some_and(|v| !(1.0..=20.0).contains(&v)) {
                return Err(Error::Invalid("permittivity must be 1-20".into()));
            }
            if let Some(thickness) = a.thickness {
                p.board.thickness = thickness;
            }
            if let Some(er) = a.er {
                p.board.er = er;
            }
            Ok(json!({"thickness": p.board.thickness, "er": p.board.er}))
        }
        "board.set_drill_origin" => {
            let a: DrillOriginParams = parse(params)?;
            if !a.position.valid() || a.position.x.abs() > 1000.0 || a.position.y.abs() > 1000.0 {
                return Err(Error::Invalid(
                    "drill origin must be a finite position within 1000mm of the corner".into(),
                ));
            }
            p.board.drill_origin = a.position;
            Ok(json!({"drill_origin": p.board.drill_origin}))
        }
        "board.reset_drill_origin" => {
            if params.as_object().is_none_or(|m| !m.is_empty()) {
                return Err(Error::Invalid(
                    "board.reset_drill_origin takes an empty object".into(),
                ));
            }
            p.board.drill_origin = pcbcraft_geom::Point::default();
            Ok(json!({"drill_origin": p.board.drill_origin}))
        }
        "board.set_outline" => {
            let a: OutlineParams = parse(params)?;
            if a.points.len() < 3 || a.points.len() > 64 {
                return Err(Error::Invalid("an outline needs 3 to 64 vertices".into()));
            }
            let distinct: std::collections::BTreeSet<(u64, u64)> = a
                .points
                .iter()
                .map(|p| (p.x.to_bits(), p.y.to_bits()))
                .collect();
            if distinct.len() != a.points.len() {
                return Err(Error::Invalid("outline vertices must be distinct".into()));
            }
            if !a
                .points
                .iter()
                .all(|p| p.valid() && p.x >= 0.0 && p.y >= 0.0)
            {
                return Err(Error::Invalid(
                    "outline vertices must be in range and inside the first quadrant".into(),
                ));
            }
            if pcbcraft_doc::outline_area(&a.points) < 0.01 {
                return Err(Error::Invalid("the traced outline has no area".into()));
            }
            let min_x = a.points.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
            let min_y = a.points.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
            let max_x = a.points.iter().map(|p| p.x).fold(0.0, f64::max);
            let max_y = a.points.iter().map(|p| p.y).fold(0.0, f64::max);
            let shifted: Vec<Point> = a
                .points
                .iter()
                .map(|p| Point::new(p.x - min_x, p.y - min_y))
                .collect();
            let area = (pcbcraft_doc::outline_area(&shifted) * 100.0).round() / 100.0;
            let vertices = shifted.len();
            p.board.outline = Some(shifted);
            p.board.width = max_x - min_x;
            p.board.height = max_y - min_y;
            Ok(json!({
                "vertices": vertices,
                "width": p.board.width,
                "height": p.board.height,
                "area": area,
            }))
        }
        "board.clear_outline" => {
            if params.as_object().is_none_or(|m| !m.is_empty()) {
                return Err(Error::Invalid(
                    "board.clear_outline takes an empty object".into(),
                ));
            }
            if p.board.outline.take().is_none() {
                return Err(missing());
            }
            Ok(json!({"width": p.board.width, "height": p.board.height}))
        }
        "board.set_layer_record" => {
            let a: LayerRecordParams = parse(params)?;
            let name = a.name.trim().to_string();
            if name.is_empty()
                || name.len() > 64
                || !matches!(
                    a.kind.as_str(),
                    "copper"
                        | "core"
                        | "prepreg"
                        | "soldermask"
                        | "silkscreen"
                        | "paste"
                        | "custom"
                )
                || !a.thickness.is_finite()
                || !(0.0..=100.0).contains(&a.thickness)
            {
                return Err(Error::Invalid(
                    "layer record needs a short name, a known role and a thickness within 100mm"
                        .into(),
                ));
            }
            let records = &mut p.board.layer_records;
            if let Some(existing) = records.iter_mut().find(|l| l.name == name) {
                existing.kind = a.kind;
                existing.thickness = a.thickness;
            } else {
                if records.len() >= 32 {
                    return Err(Error::Invalid(
                        "a board records at most 32 layer notes".into(),
                    ));
                }
                records.push(pcbcraft_doc::LayerRecord {
                    name: name.clone(),
                    kind: a.kind,
                    thickness: a.thickness,
                });
                records.sort_by(|x, y| x.name.cmp(&y.name));
            }
            Ok(json!({"name": name, "count": p.board.layer_records.len()}))
        }
        "board.clear_layer_records" => {
            if params.as_object().is_none_or(|m| !m.is_empty()) {
                return Err(Error::Invalid(
                    "board.clear_layer_records takes an empty object".into(),
                ));
            }
            let removed = p.board.layer_records.len();
            p.board.layer_records.clear();
            Ok(json!({"removed": removed}))
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
        "net.chain" => {
            let a: NetChainParams = parse(params)?;
            if a.pin.len() > 16 {
                return Err(Error::Invalid("pin must be at most 16 bytes".into()));
            }
            let net_name = |id: Option<Id>| -> Option<String> {
                id.and_then(|id| p.nets.iter().find(|n| n.id == id).map(|n| n.name.clone()))
            };
            let mut steps: Vec<Value> = vec![];
            let mut visited: Vec<(Id, String)> = vec![];
            let mut current = (a.component, a.pin.clone());
            let terminated = loop {
                let comp = p
                    .components
                    .iter()
                    .find(|c| c.id == current.0)
                    .ok_or_else(missing)?;
                let here = comp
                    .pins
                    .iter()
                    .find(|q| q.number == current.1)
                    .ok_or_else(missing)?;
                visited.push(current.clone());
                steps.push(json!({
                    "component": comp.id,
                    "reference": comp.reference,
                    "pin": here.number,
                    "net": here.net,
                    "net_name": net_name(here.net),
                }));
                let Some(net) = here.net else {
                    break "open";
                };
                let others: Vec<(Id, String)> = p
                    .components
                    .iter()
                    .flat_map(|c| c.pins.iter().map(move |q| (c, q)))
                    .filter(|(c, q)| {
                        q.net == Some(net) && !(c.id == comp.id && q.number == here.number)
                    })
                    .map(|(c, q)| (c.id, q.number.clone()))
                    .collect();
                if others.is_empty() {
                    break "open";
                }
                if others.len() > 1 {
                    break "branch";
                }
                let (next_id, next_pin) = others[0].clone();
                let next = p
                    .components
                    .iter()
                    .find(|c| c.id == next_id)
                    .ok_or_else(missing)?;
                if next.pins.len() != 2 {
                    let arrival = next
                        .pins
                        .iter()
                        .find(|q| q.number == next_pin)
                        .ok_or_else(missing)?;
                    steps.push(json!({
                        "component": next.id,
                        "reference": next.reference,
                        "pin": arrival.number,
                        "net": arrival.net,
                        "net_name": net_name(arrival.net),
                    }));
                    break "multi";
                }
                let onward = next
                    .pins
                    .iter()
                    .find(|q| q.number != next_pin)
                    .ok_or_else(missing)?;
                if visited.contains(&(next.id, onward.number.clone())) {
                    break "loop";
                }
                if steps.len() >= 2048 {
                    return Err(Error::Invalid("chain too long".into()));
                }
                current = (next.id, onward.number.clone());
            };
            Ok(json!({"steps": steps, "parts": steps.len(), "terminated": terminated}))
        }
        "net.impedance" => {
            let a: NetImpedanceParams = parse(params)?;
            if a.net.is_some_and(|id| !p.nets.iter().any(|n| n.id == id)) {
                return Err(missing());
            }
            if a.target.is_some_and(|v| !(1.0..=1000.0).contains(&v)) {
                return Err(Error::Invalid("target must be 1-1000 ohms".into()));
            }
            let height = p.board.thickness;
            let er = p.board.er;
            let mut rows: Vec<Value> = vec![];
            for net in &p.nets {
                if a.net.is_some_and(|id| id != net.id) {
                    continue;
                }
                let width = p
                    .board
                    .tracks
                    .iter()
                    .filter(|t| t.net == net.id)
                    .map(|t| t.width)
                    .fold(0.0f64, f64::max);
                if width <= 0.0 {
                    continue;
                }
                let ohms = microstrip_ohms(width, height, er);
                let mut row = json!({
                    "net": net.id,
                    "net_name": net.name,
                    "width": width,
                    "impedance_ohm": (ohms * 100.0).round() / 100.0,
                });
                if let Some(target) = a.target {
                    row["delta_ohm"] = json!(((ohms - target) * 100.0).round() / 100.0);
                    row["recommended_width"] =
                        json!((width_for_ohms(target, height, er) * 1000.0).round() / 1000.0);
                }
                rows.push(row);
            }
            let count = rows.len();
            Ok(
                json!({"rows": rows, "count": count, "dielectric": height, "er": er, "target_ohm": a.target}),
            )
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
        "rules.exclude_region" => {
            let a: RuleRegionParams = parse(params)?;
            if a.code.is_empty() || a.code.len() > 32 {
                return Err(Error::Invalid("check code must be 1 to 32 bytes".into()));
            }
            if !a.x.is_finite()
                || !a.y.is_finite()
                || !a.width.is_finite()
                || !a.height.is_finite()
                || a.width <= 0.0
                || a.height <= 0.0
                || a.width > 1000.0
                || a.height > 1000.0
                || a.x < -1000.0
                || a.y < -1000.0
                || a.x + a.width > 1000.0
                || a.y + a.height > 1000.0
            {
                return Err(Error::Invalid("region rectangle is out of range".into()));
            }
            if !p.rule_regions.contains_key(&a.code) && p.rule_regions.len() >= 64 {
                return Err(Error::Invalid(
                    "region exclusions cover at most 64 check codes".into(),
                ));
            }
            let regions = p.rule_regions.entry(a.code.clone()).or_default();
            if regions.len() >= 64 {
                return Err(Error::Invalid(
                    "a check code carries at most 64 regions".into(),
                ));
            }
            if regions
                .iter()
                .any(|r| r.x == a.x && r.y == a.y && r.width == a.width && r.height == a.height)
            {
                return Err(Error::Invalid(
                    "that rectangle is already excluded for that code".into(),
                ));
            }
            regions.push(pcbcraft_doc::RuleRegion {
                x: a.x,
                y: a.y,
                width: a.width,
                height: a.height,
            });
            Ok(json!({"code": a.code, "index": regions.len() - 1}))
        }
        "rules.include_region" => {
            let a: RuleRegionLiftParams = parse(params)?;
            if a.code.is_empty() || a.code.len() > 32 {
                return Err(Error::Invalid("check code must be 1 to 32 bytes".into()));
            }
            let count = p
                .rule_regions
                .get(&a.code)
                .map_or(0, |r| if r.is_empty() { 0 } else { r.len() });
            if count == 0 {
                return Err(Error::Invalid(
                    "that check code has no region exclusions".into(),
                ));
            }
            match a.index {
                None => {
                    p.rule_regions.remove(&a.code);
                    Ok(json!({"code": a.code, "removed": count}))
                }
                Some(i) => {
                    if i >= count {
                        return Err(Error::Invalid(
                            "region index is outside this code's regions".into(),
                        ));
                    }
                    let list = p
                        .rule_regions
                        .get_mut(&a.code)
                        .ok_or_else(|| Error::Invalid("unknown check code".into()))?;
                    list.remove(i);
                    if list.is_empty() {
                        p.rule_regions.remove(&a.code);
                    }
                    Ok(json!({"code": a.code, "index": i}))
                }
            }
        }
        "rules.area_add" => {
            let a: RuleAreaParams = parse(params)?;
            if a.name
                .as_ref()
                .is_some_and(|n| n.is_empty() || n.len() > 64)
            {
                return Err(Error::Invalid("area note must be 1 to 64 bytes".into()));
            }
            if !a.clearance.is_finite() || a.clearance <= 0.0 || a.clearance > 100.0 {
                return Err(Error::Invalid(
                    "area clearance must be above zero and at most 100mm".into(),
                ));
            }
            if !a.x.is_finite()
                || !a.y.is_finite()
                || !a.width.is_finite()
                || !a.height.is_finite()
                || a.width <= 0.0
                || a.height <= 0.0
                || a.width > 1000.0
                || a.height > 1000.0
                || a.x < -1000.0
                || a.y < -1000.0
                || a.x + a.width > 1000.0
                || a.y + a.height > 1000.0
            {
                return Err(Error::Invalid("area rectangle is out of range".into()));
            }
            if a.net.is_some_and(|n| !p.nets.iter().any(|x| x.id == n)) {
                return Err(Error::Invalid("area references an unknown net".into()));
            }
            let layer = match a.layer.as_deref() {
                None => None,
                Some("F.Cu") => Some(pcbcraft_doc::Layer::Front),
                Some("B.Cu") => Some(pcbcraft_doc::Layer::Back),
                Some(_) => return Err(Error::Invalid("layer must be F.Cu or B.Cu".into())),
            };
            if p.board.rule_areas.len() >= 64 {
                return Err(Error::Invalid(
                    "a board carries at most 64 rule areas".into(),
                ));
            }
            let id = allocate(next)?;
            p.board.rule_areas.push(pcbcraft_doc::RuleArea {
                id,
                name: a.name,
                net: a.net,
                layer,
                clearance: a.clearance,
                region: pcbcraft_doc::RuleRegion {
                    x: a.x,
                    y: a.y,
                    width: a.width,
                    height: a.height,
                },
            });
            Ok(json!({"id": id}))
        }
        "rules.area_remove" => {
            let a: IdParams = parse(params)?;
            let before = p.board.rule_areas.len();
            p.board.rule_areas.retain(|r| r.id != a.id);
            if p.board.rule_areas.len() == before {
                return Err(missing());
            }
            Ok(json!({"id": a.id}))
        }
        "board.set_machining" => {
            let a: MachiningParams = parse(params)?;
            if a.edge_plating.is_none() && a.drill_tolerance.is_none() && a.edge_tolerance.is_none()
            {
                return Err(Error::Invalid(
                    "board.set_machining needs at least one note".into(),
                ));
            }
            for t in [a.drill_tolerance, a.edge_tolerance].into_iter().flatten() {
                if !t.is_finite() || !(0.0..=1.0).contains(&t) {
                    return Err(Error::Invalid(
                        "machining tolerances must be finite and within 1mm".into(),
                    ));
                }
            }
            let mut notes = p.board.machining.take().unwrap_or_default();
            if a.edge_plating.is_some() {
                notes.edge_plating = a.edge_plating;
            }
            if a.drill_tolerance.is_some() {
                notes.drill_tolerance = a.drill_tolerance;
            }
            if a.edge_tolerance.is_some() {
                notes.edge_tolerance = a.edge_tolerance;
            }
            p.board.machining = Some(notes.clone());
            Ok(json!({"machining": notes}))
        }
        "board.clear_machining" => {
            if params.as_object().is_none_or(|m| !m.is_empty()) {
                return Err(Error::Invalid(
                    "board.clear_machining takes an empty object".into(),
                ));
            }
            let had = p.board.machining.is_some();
            p.board.machining = None;
            Ok(json!({"removed": had}))
        }
        "board.set_pair_rule" => {
            let a: PairRuleParams = parse(params)?;
            let Some(clearance) = a.clearance else {
                return Err(Error::Invalid(
                    "board.set_pair_rule needs a clearance".into(),
                ));
            };
            if a.net_a == a.net_b || a.net_a == 0 || a.net_b == 0 {
                return Err(Error::Invalid(
                    "a pair rule sits between two different nets".into(),
                ));
            }
            if !clearance.is_finite() || clearance <= 0.0 || clearance > 100.0 {
                return Err(Error::Invalid(
                    "clearance must be between 0 and 100 millimetres".into(),
                ));
            }
            for net in [a.net_a, a.net_b] {
                if !p.nets.iter().any(|n| n.id == net) {
                    return Err(Error::Invalid(format!("no net has id {net}")));
                }
            }
            let key = (a.net_a.min(a.net_b), a.net_a.max(a.net_b));
            let rules = &mut p.board.pair_rules;
            if let Some(existing) = rules
                .iter_mut()
                .find(|r| (r.a.min(r.b), r.a.max(r.b)) == key)
            {
                existing.clearance = clearance;
            } else {
                if rules.len() >= 64 {
                    return Err(Error::Invalid(
                        "a board carries at most 64 net pair clearance rules".into(),
                    ));
                }
                rules.push(pcbcraft_doc::NetPairRule {
                    a: key.0,
                    b: key.1,
                    clearance,
                });
                rules.sort_by_key(|r| (r.a, r.b));
            }
            Ok(json!({"a": key.0, "b": key.1, "clearance": clearance}))
        }
        "board.remove_pair_rule" => {
            let a: PairRuleParams = parse(params)?;
            if a.clearance.is_some() {
                return Err(Error::Invalid(
                    "board.remove_pair_rule takes two nets only".into(),
                ));
            }
            let key = (a.net_a.min(a.net_b), a.net_a.max(a.net_b));
            let rules = &mut p.board.pair_rules;
            let Some(index) = rules
                .iter()
                .position(|r| (r.a.min(r.b), r.a.max(r.b)) == key)
            else {
                return Err(Error::Invalid(format!(
                    "no clearance rule between nets {} and {}",
                    key.0, key.1
                )));
            };
            rules.remove(index);
            Ok(json!({"a": key.0, "b": key.1, "count": rules.len()}))
        }
        "bom.set_fields" => {
            let a: BomFieldsParams = parse(params)?;
            if a.fields.is_empty() || a.fields.len() > 16 {
                return Err(Error::Invalid(
                    "a bill of materials lists 1 to 16 properties".into(),
                ));
            }
            let mut keys = std::collections::BTreeSet::new();
            for f in &a.fields {
                if f.key.trim().is_empty()
                    || f.key.len() > 64
                    || f.label.len() > 64
                    || !keys.insert(f.key.clone())
                {
                    return Err(Error::Invalid(
                        "bill-of-materials columns need unique short property keys".into(),
                    ));
                }
            }
            p.bom_fields = a.fields;
            Ok(json!({"count": p.bom_fields.len()}))
        }
        "bom.clear_fields" => {
            if params.as_object().is_none_or(|m| !m.is_empty()) {
                return Err(Error::Invalid(
                    "bom.clear_fields takes an empty object".into(),
                ));
            }
            let removed = p.bom_fields.len();
            p.bom_fields.clear();
            Ok(json!({"removed": removed}))
        }
        "file.attach" => {
            let a: FileParams = parse(params)?;
            let name = a.name.trim().to_string();
            if name.is_empty() || name.len() > 128 {
                return Err(Error::Invalid(
                    "attached file names must be 1 to 128 bytes".into(),
                ));
            }
            if a.data.len() > 262_144 {
                return Err(Error::Invalid(
                    "an attached file holds at most 256 KiB of text".into(),
                ));
            }
            let files = &mut p.files;
            if let Some(existing) = files.iter_mut().find(|f| f.name == name) {
                existing.data = a.data;
            } else {
                if files.len() >= 32 {
                    return Err(Error::Invalid("a project attaches at most 32 files".into()));
                }
                files.push(pcbcraft_doc::EmbeddedFile {
                    name: name.clone(),
                    data: a.data,
                });
                files.sort_by(|x, y| x.name.cmp(&y.name));
            }
            Ok(json!({"name": name, "count": p.files.len()}))
        }
        "file.remove" => {
            let a: FileParams = parse(params)?;
            let name = a.name.trim();
            let files = &mut p.files;
            let Some(index) = files.iter().position(|f| f.name == name) else {
                return Err(Error::Invalid(format!("no attached file named {name}")));
            };
            files.remove(index);
            Ok(json!({"name": name, "count": files.len()}))
        }
        "variant.create" => {
            let a: VariantCreateParams = parse(params)?;
            let name = a.name.trim().to_string();
            if name.is_empty() || name.len() > 64 {
                return Err(Error::Invalid("variant names must be 1 to 64 bytes".into()));
            }
            if a.note.as_ref().is_some_and(|n| n.len() > 256) {
                return Err(Error::Invalid(
                    "a variant note holds at most 256 bytes".into(),
                ));
            }
            if a.dnp.len() > 512 {
                return Err(Error::Invalid(
                    "a variant lists at most 512 components".into(),
                ));
            }
            for id in &a.dnp {
                if !p.components.iter().any(|c| c.id == *id) {
                    return Err(Error::Invalid(format!("no component has id {id}")));
                }
            }
            if p.variants.len() >= 16 {
                return Err(Error::Invalid(
                    "a project defines at most 16 variants".into(),
                ));
            }
            if p.variants.iter().any(|v| v.name == name) {
                return Err(Error::Invalid(format!("a variant named {name} exists")));
            }
            let mut dnp = a.dnp;
            dnp.sort();
            dnp.dedup();
            p.variants.push(pcbcraft_doc::Variant {
                name: name.clone(),
                note: a.note,
                dnp,
                footprints: vec![],
            });
            p.variants.sort_by(|x, y| x.name.cmp(&y.name));
            Ok(json!({"name": name, "count": p.variants.len()}))
        }
        "variant.set_dnp" => {
            let a: VariantDnpParams = parse(params)?;
            let name = a.name.trim();
            if a.ids.len() > 512 {
                return Err(Error::Invalid(
                    "a variant lists at most 512 components".into(),
                ));
            }
            for id in &a.ids {
                if !p.components.iter().any(|c| c.id == *id) {
                    return Err(Error::Invalid(format!("no component has id {id}")));
                }
            }
            let fitting = a.dnp.unwrap_or(true);
            let Some(v) = p.variants.iter_mut().find(|v| v.name == name) else {
                return Err(Error::Invalid(format!("no variant named {name}")));
            };
            if fitting && a.ids.is_empty() {
                v.dnp.clear();
            } else if fitting {
                for id in a.ids {
                    if !v.dnp.contains(&id) {
                        v.dnp.push(id);
                    }
                }
                v.dnp.sort();
            } else {
                v.dnp.retain(|x| !a.ids.contains(x));
            }
            Ok(json!({"name": name, "dnp": v.dnp.len()}))
        }
        "variant.set_footprint" => {
            let a: VariantFootprintParams = parse(params)?;
            let name = a.name.trim();
            let Some(raw) = a.footprint.as_deref().map(str::trim) else {
                return Err(Error::Invalid(
                    "variant.set_footprint needs a footprint name".into(),
                ));
            };
            if name.is_empty() || name.len() > 64 {
                return Err(Error::Invalid("variant names must be 1 to 64 bytes".into()));
            }
            if raw.is_empty() || raw.len() > 128 {
                return Err(Error::Invalid(
                    "footprint names must be 1 to 128 bytes".into(),
                ));
            }
            if !p.components.iter().any(|c| c.id == a.component) {
                return Err(Error::Invalid(format!(
                    "no component has id {}",
                    a.component
                )));
            }
            let Some(v) = p.variants.iter_mut().find(|v| v.name == name) else {
                return Err(Error::Invalid(format!("no variant named {name}")));
            };
            if let Some(existing) = v.footprints.iter_mut().find(|o| o.component == a.component) {
                existing.footprint = raw.to_string();
            } else {
                if v.footprints.len() >= 512 {
                    return Err(Error::Invalid(
                        "a variant swaps at most 512 footprints".into(),
                    ));
                }
                v.footprints.push(pcbcraft_doc::FootprintOverride {
                    component: a.component,
                    footprint: raw.to_string(),
                });
                v.footprints.sort_by_key(|o| o.component);
            }
            Ok(json!({"name": name, "component": a.component, "footprint": raw}))
        }
        "variant.clear_footprint" => {
            let a: VariantFootprintParams = parse(params)?;
            if a.footprint.is_some() {
                return Err(Error::Invalid(
                    "variant.clear_footprint takes a variant name and a component id only".into(),
                ));
            }
            let name = a.name.trim();
            let Some(v) = p.variants.iter_mut().find(|v| v.name == name) else {
                return Err(Error::Invalid(format!("no variant named {name}")));
            };
            let before = v.footprints.len();
            v.footprints.retain(|o| o.component != a.component);
            if v.footprints.len() == before {
                return Err(Error::Invalid(format!(
                    "variant {name} does not swap the footprint of component {}",
                    a.component
                )));
            }
            Ok(json!({
                "name": name,
                "component": a.component,
                "swaps": v.footprints.len(),
            }))
        }
        "variant.remove" => {
            let a: VariantNameParams = parse(params)?;
            let name = a.name.trim();
            let Some(index) = p.variants.iter().position(|v| v.name == name) else {
                return Err(Error::Invalid(format!("no variant named {name}")));
            };
            p.variants.remove(index);
            Ok(json!({"name": name, "count": p.variants.len()}))
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
            // With no explicit request, the columns the project defines are used.
            let keys: Vec<String> = if a.fields.is_empty() {
                p.bom_fields.iter().map(|f| f.key.clone()).collect()
            } else {
                a.fields.clone()
            };
            let variant = match a.variant.as_deref().map(str::trim) {
                None | Some("") => None,
                Some(name) => Some(
                    p.variants
                        .iter()
                        .find(|v| v.name == name)
                        .ok_or_else(|| Error::Invalid(format!("no variant named {name}")))?,
                ),
            };
            let mut groups: std::collections::BTreeMap<
                (String, String, String, Vec<String>),
                Vec<String>,
            > = std::collections::BTreeMap::new();
            let mut populated = 0usize;
            for c in &p.components {
                if variant.is_some_and(|v| v.dnp.contains(&c.id)) {
                    continue;
                }
                populated += 1;
                let footprint =
                    match variant.and_then(|v| v.footprints.iter().find(|o| o.component == c.id)) {
                        Some(swap) => swap.footprint.clone(),
                        None => match &c.library_footprint {
                            Some(footprint) => {
                                format!("{}:{}", footprint.library, footprint.definition.name)
                            }
                            None => "starter pads".into(),
                        },
                    };
                let values: Vec<String> = keys
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
                    let columns = keys.clone();
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
            Ok(json!({
                "items": items,
                "groups": items.len(),
                "components": populated,
                "variant": variant.map(|v| v.name.clone()),
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
                        locked: None,
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
            if footprint_locked(p, a.id) {
                return Err(Error::Invalid(
                    "that footprint is locked against placement changes".into(),
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
            if footprint_locked(p, a.id) {
                return Err(Error::Invalid(
                    "that footprint is locked against placement changes".into(),
                ));
            }
            p.board
                .footprints
                .iter_mut()
                .find(|f| f.id == a.id)
                .ok_or_else(missing)?
                .rotation = a.degrees.rem_euclid(360.0);
            Ok(json!({"id":a.id}))
        }
        "footprint.set_pad" => {
            let a: PadSetParams = parse(params)?;
            if a.offset.is_none() && a.diameter.is_none() {
                return Err(Error::Invalid(
                    "provide an offset or a diameter to change".into(),
                ));
            }
            if a.pad.is_empty() || a.pad.len() > 32 {
                return Err(Error::Invalid("pad number must be 1 to 32 bytes".into()));
            }
            if a.offset.is_some_and(|o| !o.valid()) {
                return Err(Error::Invalid("pad offset is out of range".into()));
            }
            if a.diameter
                .is_some_and(|d| !d.is_finite() || d <= 0.0 || d > 100.0)
            {
                return Err(Error::Invalid(
                    "pad diameter must be within 0 to 100 mm".into(),
                ));
            }
            let pad = p
                .board
                .footprints
                .iter_mut()
                .find(|f| f.id == a.id)
                .and_then(|f| f.pads.iter_mut().find(|pd| pd.number == a.pad))
                .ok_or_else(missing)?;
            if pad.geometry.is_some() && a.diameter.is_some() {
                return Err(Error::Invalid(
                    "a shaped pad keeps its library shape; only its offset is editable".into(),
                ));
            }
            if a.offset.is_none_or(|o| o == pad.offset)
                && a.diameter.is_none_or(|d| d == pad.diameter)
            {
                return Err(Error::Invalid(
                    "that pad already has that offset and diameter".into(),
                ));
            }
            if let Some(offset) = a.offset {
                pad.offset = offset;
            }
            if let Some(diameter) = a.diameter {
                pad.diameter = diameter;
            }
            Ok(json!({"id": a.id, "pad": a.pad}))
        }
        "footprint.set_castellated" => {
            let a: PadCastellateParams = parse(params)?;
            if a.pad.is_empty() || a.pad.len() > 32 {
                return Err(Error::Invalid("pad number must be 1 to 32 bytes".into()));
            }
            let pad = p
                .board
                .footprints
                .iter_mut()
                .find(|f| f.id == a.id)
                .and_then(|f| f.pads.iter_mut().find(|pd| pd.number == a.pad))
                .ok_or_else(missing)?;
            pad.castellated = a.on.then_some(true);
            Ok(json!({"id": a.id, "pad": a.pad, "castellated": a.on}))
        }
        "footprint.dock_edge" => {
            let a: DockEdgeParams = parse(params)?;
            if footprint_locked(p, a.id) {
                return Err(Error::Invalid(
                    "that footprint is locked against placement changes".into(),
                ));
            }
            let footprint = p
                .board
                .footprints
                .iter_mut()
                .find(|f| f.id == a.id)
                .ok_or_else(missing)?;
            let (mut min_x, mut min_y, mut max_x, mut max_y): (f64, f64, f64, f64) =
                (f64::MAX, f64::MAX, 0.0, 0.0);
            for pad in footprint.pads.iter().cloned() {
                let Some(center) = footprint.pad_position(&pad.number) else {
                    continue;
                };
                let radius = pad.bounding_radius();
                min_x = min_x.min(center.x - radius);
                min_y = min_y.min(center.y - radius);
                max_x = max_x.max(center.x + radius);
                max_y = max_y.max(center.y + radius);
            }
            if min_x > max_x {
                return Err(Error::Invalid(
                    "that footprint has no copper to dock with an edge".into(),
                ));
            }
            let shift = match a.edge.as_str() {
                "left" => (0.0 - min_x, 0.0),
                "right" => (p.board.width - max_x, 0.0),
                "bottom" => (0.0, 0.0 - min_y),
                "top" => (0.0, p.board.height - max_y),
                _ => {
                    return Err(Error::Invalid(
                        "edge must be left, right, top or bottom".into(),
                    ));
                }
            };
            if shift.0.abs() < 1e-9 && shift.1.abs() < 1e-9 {
                return Err(Error::Invalid(
                    "that footprint is already flush with that edge".into(),
                ));
            }
            let docked = Point::new(
                footprint.position.x + shift.0,
                footprint.position.y + shift.1,
            );
            if !docked.valid() {
                return Err(Error::Invalid("the docked position is out of range".into()));
            }
            footprint.position = docked;
            Ok(json!({
                "id": a.id,
                "edge": a.edge,
                "position": {"x": footprint.position.x, "y": footprint.position.y},
            }))
        }
        "footprint.set_locked" => {
            let a: LockParams = parse(params)?;
            let footprint = p
                .board
                .footprints
                .iter_mut()
                .find(|f| f.id == a.id)
                .ok_or_else(missing)?;
            footprint.locked = a.locked.then_some(true);
            Ok(json!({"id": a.id, "locked": a.locked}))
        }
        "footprint.array" => {
            let a: ArrayParams = parse(params)?;
            if a.columns == 0
                || a.columns > 64
                || a.rows == 0
                || a.rows > 64
                || !a.dx.is_finite()
                || a.dx <= 0.0
                || a.dx > 10_000.0
                || !a.dy.is_finite()
                || a.dy <= 0.0
                || a.dy > 10_000.0
            {
                return Err(Error::Invalid(
                    "array must be 1-64 by 1-64 with positive spacing".into(),
                ));
            }
            if footprint_locked(p, a.footprint) {
                return Err(Error::Invalid(
                    "that footprint is locked against placement changes".into(),
                ));
            }
            let total = a.columns * a.rows;
            if total < 2 {
                return Err(Error::Invalid("an array needs at least two copies".into()));
            }
            if p.ids().count() + 2 * (total - 1) > 2048 {
                return Err(Error::Invalid(
                    "the array would exceed the project limit".into(),
                ));
            }
            let source = p
                .board
                .footprints
                .iter()
                .find(|f| f.id == a.footprint)
                .ok_or_else(missing)?
                .clone();
            let base = p
                .components
                .iter()
                .find(|c| c.id == source.component)
                .ok_or_else(missing)?
                .clone();
            let mut copies = Vec::new();
            let mut parts = Vec::new();
            for row in 0..a.rows {
                for column in 0..a.columns {
                    if row == 0 && column == 0 {
                        continue;
                    }
                    let reference = format!("{}-{}", base.reference, copies.len() + 2);
                    if reference.len() > 64 || p.components.iter().any(|c| c.reference == reference)
                    {
                        return Err(Error::Invalid(format!(
                            "reference {reference} is already in use"
                        )));
                    }
                    let delta = Point::new(column as f64 * a.dx, row as f64 * a.dy);
                    let position = base.position.offset(delta);
                    let footprint_position = source.position.offset(delta);
                    let component = allocate(next)?;
                    let footprint = allocate(next)?;
                    let mut new_component = base.clone();
                    new_component.id = component;
                    new_component.reference = reference;
                    new_component.position = position;
                    let mut new_footprint = source.clone();
                    new_footprint.id = footprint;
                    new_footprint.component = component;
                    new_footprint.position = footprint_position;
                    p.components.push(new_component);
                    p.board.footprints.push(new_footprint);
                    copies.push(component);
                    parts.push(footprint);
                }
            }
            Ok(json!({
                "count": copies.len() + 1,
                "components": copies,
                "footprints": parts
            }))
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
                arc: None,
            });
            Ok(json!({"id":id, "width": width}))
        }
        "track.add_arc" => {
            let a: TrackArcParams = parse(params)?;
            if !a.start.valid() || !a.mid.valid() || !a.end.valid() {
                return Err(Error::Invalid("track points are out of range".into()));
            }
            let width = a.width.unwrap_or_else(|| class_width(p, a.net));
            if !positive(width) {
                return Err(Error::Invalid("track width must be positive".into()));
            }
            let Some(arc) = pcbcraft_geom::arc_through(a.start, a.mid, a.end) else {
                return Err(Error::Invalid(
                    "the three points must describe a proper arc".into(),
                ));
            };
            let id = allocate(next)?;
            p.board.tracks.push(Track {
                id,
                net: a.net,
                start: a.start,
                end: a.end,
                width,
                layer: a.layer,
                arc: Some(a.mid),
            });
            Ok(json!({
                "id": id,
                "length": (arc.length() * 1000.0).round() / 1000.0,
            }))
        }
        "track.straighten" => {
            let a: IdParams = parse(params)?;
            let track = p
                .board
                .tracks
                .iter_mut()
                .find(|t| t.id == a.id)
                .ok_or_else(missing)?;
            if track.arc.take().is_none() {
                return Err(Error::Invalid("that track is already straight".into()));
            }
            Ok(json!({"id": a.id, "length": track.length()}))
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
            if track.arc.is_some() {
                return Err(Error::Invalid(
                    "a curved track cannot be split; straighten it first".into(),
                ));
            }
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
                arc: None,
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
            if p.board.tracks[ia].arc.is_some() || p.board.tracks[ib].arc.is_some() {
                return Err(Error::Invalid(
                    "curved tracks cannot be chamfered; straighten them first".into(),
                ));
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
                arc: None,
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
            track.arc = track
                .arc
                .map(|m| Point::new(m.x + a.delta.x, m.y + a.delta.y));
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
                        arc: None,
                    });
                }
            }
            Ok(json!({"ids":ids,"net":net,"width":width}))
        }
        "footprint.mirror" => {
            let a: MirrorParams = parse(params)?;
            if footprint_locked(p, a.id) {
                return Err(Error::Invalid(
                    "that footprint is locked against placement changes".into(),
                ));
            }
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
        "object.nudge" => {
            let a: NudgeParams = parse(params)?;
            if !a.dx.is_finite() || !a.dy.is_finite() || a.dx.abs() > 1000.0 || a.dy.abs() > 1000.0
            {
                return Err(Error::Invalid("nudge distance is out of range".into()));
            }
            if a.dx == 0.0 && a.dy == 0.0 {
                return Err(Error::Invalid("a nudge must move something".into()));
            }
            let shift = |pt: Point| Point::new(pt.x + a.dx, pt.y + a.dy);
            let shift_all = |pts: &mut Vec<Point>| -> Result<(), Error> {
                for pt in pts.iter_mut() {
                    *pt = shift(*pt);
                    if !pt.valid() {
                        return Err(Error::Invalid("nudged position is out of range".into()));
                    }
                }
                Ok(())
            };
            if let Some(t) = p.board.tracks.iter_mut().find(|t| t.id == a.id) {
                let (start, end) = (shift(t.start), shift(t.end));
                if !start.valid() || !end.valid() {
                    return Err(Error::Invalid("nudged position is out of range".into()));
                }
                (t.start, t.end) = (start, end);
                return Ok(json!({"id": a.id}));
            }
            if let Some(v) = p.board.vias.iter_mut().find(|v| v.id == a.id) {
                let position = shift(v.position);
                if !position.valid() {
                    return Err(Error::Invalid("nudged position is out of range".into()));
                }
                v.position = position;
                return Ok(json!({"id": a.id}));
            }
            if let Some(f) = p.board.footprints.iter_mut().find(|f| f.id == a.id) {
                let position = shift(f.position);
                if !position.valid() {
                    return Err(Error::Invalid("nudged position is out of range".into()));
                }
                f.position = position;
                return Ok(json!({"id": a.id}));
            }
            if let Some(z) = p.board.zones.iter_mut().find(|z| z.id == a.id) {
                let mut outline = z.outline.clone();
                shift_all(&mut outline)?;
                let mut cutouts = z.cutouts.clone();
                for cut in &mut cutouts {
                    shift_all(cut)?;
                }
                z.outline = outline;
                z.cutouts = cutouts;
                return Ok(json!({"id": a.id}));
            }
            if let Some(c) = p.board.cutouts.iter_mut().find(|c| c.id == a.id) {
                let center = shift(c.center);
                if !center.valid() {
                    return Err(Error::Invalid("nudged position is out of range".into()));
                }
                c.center = center;
                return Ok(json!({"id": a.id}));
            }
            if let Some(t) = p.board.texts.iter_mut().find(|t| t.id == a.id) {
                let position = shift(t.position);
                if !position.valid() {
                    return Err(Error::Invalid("nudged position is out of range".into()));
                }
                t.position = position;
                return Ok(json!({"id": a.id}));
            }
            Err(missing())
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
            p.board.pair_rules.retain(|r| r.a != a.id && r.b != a.id);
            p.board.rule_areas.retain(|r| r.net != Some(a.id));
            for class in p.net_classes.iter_mut() {
                class.nets.retain(|net| *net != a.id);
            }
            p.wires
                .retain(|w| w.id != a.id && w.from.component != a.id && w.to.component != a.id);
            p.drawn_wires
                .retain(|w| w.id != a.id && w.net.is_none_or(|n| n != a.id));
            p.graphics.retain(|g| g.id != a.id);
            for group in p.graphic_groups.iter_mut() {
                group.graphics.retain(|gid| *gid != a.id);
            }
            p.graphic_groups.retain(|g| g.graphics.len() >= 2);
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
            p.board.holes.retain(|h| h.id != a.id);
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
        "track.set_layer" => {
            let a: TrackLayerParams = parse(params)?;
            let track = p
                .board
                .tracks
                .iter_mut()
                .find(|t| t.id == a.id)
                .ok_or_else(missing)?;
            if track.layer == a.layer {
                return Err(Error::Invalid("that track is already on the layer".into()));
            }
            track.layer = a.layer;
            Ok(json!({"id":a.id,"layer":track.layer}))
        }
        "track.set_end" => {
            let a: TrackEndParams = parse(params)?;
            if !a.position.valid() {
                return Err(Error::Invalid("track end is out of range".into()));
            }
            let track = p
                .board
                .tracks
                .iter_mut()
                .find(|t| t.id == a.id)
                .ok_or_else(missing)?;
            let (other, moved) = match a.end {
                TrackEnd::Start => (track.end, &mut track.start),
                TrackEnd::End => (track.start, &mut track.end),
            };
            if other.distance(a.position) < 1e-6 {
                return Err(Error::Invalid("the two ends cannot meet".into()));
            }
            *moved = a.position;
            track.arc = None;
            Ok(json!({
                "id": a.id,
                "end": a.end,
                "position": *moved,
            }))
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
                rotation: 0.0,
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
            if a.text.is_none() && a.size.is_none() && a.layer.is_none() && a.rotation.is_none() {
                return Err(Error::Invalid(
                    "provide a text, size, layer or rotation to change".into(),
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
            if a.rotation
                .is_some_and(|r| !r.is_finite() || r.abs() > 360_000.0)
            {
                return Err(Error::Invalid("invalid text rotation".into()));
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
            if let Some(rotation) = a.rotation {
                text.rotation = rotation;
            }
            Ok(
                json!({"id":text.id,"text":text.text,"size":text.size,"layer":text.layer,"rotation":text.rotation}),
            )
        }
        "text.rotate" => {
            let a: TextRotateParams = parse(params)?;
            if !a.degrees.is_finite() || a.degrees.abs() > 360_000.0 {
                return Err(Error::Invalid("invalid text rotation".into()));
            }
            let text = p
                .board
                .texts
                .iter_mut()
                .find(|t| t.id == a.id)
                .ok_or_else(missing)?;
            text.rotation = a.degrees;
            Ok(json!({"id":text.id,"rotation":text.rotation}))
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
    fn schematic_labels_can_be_retyped_and_reassigned_in_place() -> Result<(), Error> {
        let mut s = Session::default();
        let a = s.execute("net.create", json!({"name":"A"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let b = s.execute("net.create", json!({"name":"B"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        s.execute(
            "wire.draw",
            json!({"net":a,"points":[{"x":10.0,"y":10.0},{"x":20.0,"y":10.0}]}),
            None,
        )?;
        let lab = s.execute(
            "label.place",
            json!({"kind":"label","net":a,"position":{"x":30.0,"y":20.0}}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        s.execute("label.set", json!({"id":lab,"kind":"global"}), None)?;
        s.execute("label.set", json!({"id":lab,"net":b}), None)?;
        let label = s
            .project
            .labels
            .iter()
            .find(|l| l.id == lab)
            .ok_or_else(missing)?;
        assert_eq!(label.kind, "global");
        assert_eq!(label.net, b);
        assert_eq!(label.position, Point::new(30.0, 20.0));
        assert!(s.project.validate().is_ok());
        // An off-wire label cannot become a junction where it stands.
        assert!(
            s.execute("label.set", json!({"id":lab,"kind":"junction"}), None)
                .is_err()
        );
        // A junction on the wire may only name a net whose wire touches it.
        let junc = s.execute(
            "label.place",
            json!({"kind":"junction","net":a,"position":{"x":20.0,"y":10.0}}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        assert!(
            s.execute("label.set", json!({"id":junc,"net":b}), None)
                .is_err()
        );
        s.execute(
            "wire.draw",
            json!({"net":b,"points":[{"x":20.0,"y":10.0},{"x":20.0,"y":15.0}]}),
            None,
        )?;
        s.execute("label.set", json!({"id":junc,"net":b}), None)?;
        // Released from the wire, the old junction moves freely again.
        s.execute("label.set", json!({"id":junc,"kind":"label"}), None)?;
        s.execute(
            "label.move",
            json!({"id":junc,"position":{"x":40.0,"y":40.0}}),
            None,
        )?;
        let before = s.inspect();
        for params in [
            json!({"id":lab}),
            json!({"id":lab,"kind":"global"}),
            json!({"id":lab,"net":b}),
            json!({"id":lab,"kind":"global","net":b}),
            json!({"id":lab,"kind":"nonsense"}),
            json!({"id":lab,"net":9999}),
            json!({"id":9999,"kind":"global"}),
            json!({"id":lab,"kind":"junction"}),
            json!({"id":lab,"extra":1}),
        ] {
            assert!(
                s.execute("label.set", params.clone(), None).is_err(),
                "{params:?} refused"
            );
        }
        assert_eq!(s.inspect(), before, "rejections change nothing");
        s.execute("label.set", json!({"id":lab,"kind":"power"}), None)?;
        s.undo()?;
        let label = s
            .project
            .labels
            .iter()
            .find(|l| l.id == lab)
            .ok_or_else(missing)?;
        assert_eq!(label.kind, "global", "undo restores the kind");
        assert_eq!(label.net, b);
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
    fn drawn_wires_slide_and_bend_with_their_junctions() -> Result<(), Error> {
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
        let junction = s.execute(
            "label.place",
            json!({"kind":"junction","net":net,"position":{"x":20.0,"y":10.0}}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        s.execute(
            "wire.move",
            json!({"id":wire,"delta":{"x":5.0,"y":-2.0}}),
            None,
        )?;
        let points = s.project.drawn_wires[0].points.clone();
        assert_eq!(points[0], Point::new(15.0, 8.0));
        assert_eq!(points[2], Point::new(25.0, 18.0));
        let junction_position = |s: &Session| -> Point {
            s.project
                .labels
                .iter()
                .find(|l| l.id == junction)
                .map(|l| l.position)
                .unwrap_or(Point::new(-1.0, -1.0))
        };
        assert_eq!(junction_position(&s), Point::new(25.0, 8.0));
        s.undo()?;
        let points = s.project.drawn_wires[0].points.clone();
        assert_eq!(points[0], Point::new(10.0, 10.0));
        assert_eq!(junction_position(&s), Point::new(20.0, 10.0));
        s.execute(
            "wire.set_point",
            json!({"id":wire,"index":0,"position":{"x":10.0,"y":15.0}}),
            None,
        )?;
        let points = s.project.drawn_wires[0].points.clone();
        assert_eq!(points[0], Point::new(10.0, 15.0));
        assert_eq!(
            junction_position(&s),
            Point::new(20.0, 10.0),
            "the junction stays where it is when its vertex does not move"
        );
        let before = s.inspect()["project"].clone();
        for params in [
            json!({"id":wire,"delta":{"x":0.0,"y":0.0}}),
            json!({"id":wire,"delta":{"x":900000.0,"y":0.0}}),
            json!({"id":9999,"delta":{"x":1.0,"y":0.0}}),
            json!({"id":wire,"delta":{"x":1.0,"y":0.0},"extra":1}),
            json!({"id":wire,"delta":{"x":f64::NAN,"y":0.0}}),
        ] {
            assert!(s.execute("wire.move", params.clone(), None).is_err());
        }
        for params in [
            json!({"id":wire,"index":3,"position":{"x":1.0,"y":1.0}}),
            json!({"id":wire,"index":1,"position":{"x":20.0,"y":10.0}}),
            json!({"id":wire,"index":1,"position":{"x":10.0,"y":10.0}}),
            json!({"id":wire,"index":1,"position":{"x":1e6,"y":0.0}}),
            json!({"id":wire,"index":0,"position":{"x":10.0,"y":15.0}}),
            json!({"id":9999,"index":0,"position":{"x":1.0,"y":1.0}}),
        ] {
            assert!(s.execute("wire.set_point", params.clone(), None).is_err());
        }
        assert!(
            s.execute(
                "wire.set_point",
                json!({"id":wire,"index":1,"position":{"x":15.0,"y":5.0}}),
                None,
            )
            .is_err(),
            "the junction would be left off the wire"
        );
        assert_eq!(s.inspect()["project"], before, "rejections change nothing");
        Ok(())
    }
    #[test]
    fn project_wide_find_and_replace_updates_every_text_field() -> Result<(), Error> {
        let mut s = Session::default();
        let net = s.execute("net.create", json!({"name":"VCC_MAIN"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let other = s.execute("net.create", json!({"name":"GND"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let r1 = s.execute(
            "component.add",
            json!({"kind":"resistor","reference":"R1","value":"VCC resistor","position":{"x":10.0,"y":10.0}}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        s.execute(
            "component.add",
            json!({"kind":"resistor","reference":"R2","value":"10 k","position":{"x":20.0,"y":10.0}}),
            None,
        )?;
        s.execute(
            "component.field",
            json!({"id":r1,"key":"note","value":"to VCC rail"}),
            None,
        )?;
        s.execute(
            "text.place",
            json!({"text":"VCC testpoint","position":{"x":1.0,"y":1.0}}),
            None,
        )?;
        s.execute(
            "graphic.add",
            json!({"layer":"schematic","shape":"text","points":[{"x":2.0,"y":2.0}],"width":1.5,"text":"VCC note"}),
            None,
        )?;
        let probe = json!({"search":"VCC","replace":"3V3"});
        let dry = s.execute(
            "project.replace",
            json!({"search":"VCC","replace":"3V3","dry_run":true}),
            None,
        )?["result"]
            .clone();
        assert!(dry["dry_run"].as_bool().is_some_and(|b| b));
        assert_eq!(
            dry["count"], 5,
            "net, value, field, board text and graphic text"
        );
        let before = s.inspect()["project"].clone();
        assert!(
            s.execute(
                "project.replace",
                json!({"search":"VCC","replace":"3V3","dry_run":true}),
                None
            )
            .is_ok()
        );
        assert_eq!(
            s.inspect()["project"],
            before,
            "a dry run must not touch the project"
        );
        let done = s.execute("project.replace", probe.clone(), None)?["result"].clone();
        assert_eq!(done["count"], 5);
        assert_eq!(
            s.project
                .nets
                .iter()
                .find(|n| n.id == net)
                .map(|n| n.name.as_str())
                .unwrap_or_default(),
            "3V3_MAIN"
        );
        assert_eq!(
            s.project.component(r1).ok_or_else(missing)?.value,
            "3V3 resistor"
        );
        assert_eq!(
            s.project.component(r1).ok_or_else(missing)?.field("note"),
            Some("to 3V3 rail")
        );
        assert_eq!(s.project.board.texts[0].text, "3V3 testpoint");
        assert_eq!(s.project.graphics[0].text, "3V3 note");
        assert_eq!(
            s.project
                .nets
                .iter()
                .find(|n| n.id == other)
                .map(|n| n.name.as_str())
                .unwrap_or_default(),
            "GND"
        );
        s.undo()?;
        assert_eq!(
            s.project
                .nets
                .iter()
                .find(|n| n.id == net)
                .map(|n| n.name.as_str())
                .unwrap_or_default(),
            "VCC_MAIN",
            "one undo reverses the whole replace"
        );
        s.execute("project.replace", probe.clone(), None)?;
        assert!(
            s.execute(
                "project.replace",
                json!({"search":"VCC","replace":"3V3","scope":"components"}),
                None,
            )
            .is_err(),
            "the components scope has no VCC left"
        );
        let before = s.inspect()["project"].clone();
        for params in [
            json!({"search":"VCC","replace":"VCC"}),
            json!({"search":"NOPE","replace":"x"}),
            json!({"search":"GND","replace":"x","scope":"text"}),
            json!({"search":"GND","replace":"x","scope":"nope"}),
            json!({"search":"1","replace":"2"}),
            json!({"search":"R1","replace":"R2"}),
            json!({"search":"GND","replace":"3V3_MAIN"}),
            json!({"search":"VCC"}),
            json!({"search":"VCC","replace":"x","extra":1}),
        ] {
            assert!(s.execute("project.replace", params.clone(), None).is_err());
        }
        assert_eq!(
            s.inspect()["project"],
            before,
            "rejections and collisions change nothing"
        );
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
    fn drill_chart_groups_every_drilled_hole_by_size() -> Result<(), Error> {
        let mut s = Session::default();
        let net = s.execute("net.create", json!({"name":"N"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        for (x, d) in [(10.0, 0.3), (12.0, 0.3), (14.0, 0.35), (16.0, 0.0)] {
            s.execute(
                "via.place",
                json!({"net":net,"position":{"x":x,"y":10.0},"diameter":0.6,"drill":d}),
                None,
            )?;
        }
        let chip = s.execute(
            "component.place_library",
            json!({"path":library_fixture("lm358.kicad_sym"),"symbol":"LM358","position":{"x":20.0,"y":40.0}}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        s.execute(
            "footprint.assign",
            json!({"id":chip,"path":library_fixture("DIP-8_W7.62mm.kicad_mod")}),
            None,
        )?;
        s.execute("board.sync", json!({}), None)?;
        let chart = s.execute("drill.chart", json!({}), None)?["result"].clone();
        assert_eq!(chart["total"], 11, "three drilled vias and eight pads");
        assert_eq!(chart["board_thickness"], 1.6);
        let holes = chart["holes"].as_array().ok_or_else(missing)?;
        assert_eq!(holes.len(), 3, "the unplated via has no hole to count");
        assert_eq!(holes[0]["shape"], "round");
        assert_eq!(holes[0]["diameter"], 0.3);
        assert_eq!(holes[0]["count"], 2);
        assert_eq!(holes[1]["diameter"], 0.35);
        assert_eq!(holes[1]["count"], 1);
        assert_eq!(holes[2]["diameter"], 0.8);
        assert_eq!(holes[2]["count"], 8);
        let before = s.inspect()["project"].clone();
        assert!(s.execute("drill.chart", json!({"extra": 1}), None).is_err());
        assert!(s.execute("drill.chart", json!([]), None).is_err());
        assert_eq!(
            s.inspect()["project"],
            before,
            "the report changes no project data"
        );
        Ok(())
    }
    #[test]
    fn board_statistics_count_objects_and_the_finish_is_recorded() -> Result<(), Error> {
        let mut s = Session::default();
        let net = s.execute("net.create", json!({"name":"N"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        s.execute(
            "track.add",
            json!({"net":net,"start":{"x":0.0,"y":0.0},"end":{"x":10.0,"y":0.0},"width":0.3}),
            None,
        )?;
        s.execute(
            "track.add",
            json!({"net":net,"start":{"x":0.0,"y":0.0},"end":{"x":0.0,"y":4.0},"width":0.3,"layer":"B.Cu"}),
            None,
        )?;
        s.execute(
            "via.place",
            json!({"net":net,"position":{"x":5.0,"y":5.0},"diameter":0.6,"drill":0.3}),
            None,
        )?;
        s.execute(
            "via.place",
            json!({"net":net,"position":{"x":6.0,"y":5.0},"diameter":0.6,"drill":0.0}),
            None,
        )?;
        s.execute(
            "board.cutout.add",
            json!({"center":{"x":30.0,"y":20.0},"radius":2.0}),
            None,
        )?;
        s.execute(
            "text.place",
            json!({"text":"REV A","position":{"x":10.0,"y":35.0},"size":1.5}),
            None,
        )?;
        let stats = s.execute("board.statistics", json!({}), None)?["result"].clone();
        assert_eq!(stats["tracks"]["front"], 1);
        assert_eq!(stats["tracks"]["back"], 1);
        assert_eq!(stats["routed_length"]["total"], 14.0);
        assert_eq!(stats["vias"], 2);
        assert_eq!(
            stats["drilled_holes"], 1,
            "vias without a drill are not holes"
        );
        assert_eq!(stats["cutouts"], 1);
        assert_eq!(stats["texts"], 1);
        assert_eq!(stats["board_area"], 2400.0);
        assert_eq!(stats["finish"], "HASL", "a fresh board records a finish");
        s.execute("board.set_finish", json!({"finish":"ENIG"}), None)?;
        let stats = s.execute("board.statistics", json!({}), None)?["result"].clone();
        assert_eq!(stats["finish"], "ENIG");
        let before = s.inspect()["project"].clone();
        assert!(
            s.execute("board.set_finish", json!({"finish":""}), None)
                .is_err()
        );
        assert!(
            s.execute("board.set_finish", json!({"finish":"x".repeat(65)}), None)
                .is_err(),
            "the finish stays bounded"
        );
        assert!(
            s.execute("board.set_finish", json!({"finish":"ENIG","extra":1}), None)
                .is_err(),
            "unknown parameters are refused"
        );
        assert!(
            s.execute("board.statistics", json!({"extra":1}), None)
                .is_err()
        );
        assert_eq!(
            s.inspect()["project"],
            before,
            "refusals and reports change no project data"
        );
        s.undo()?; // steps back over the no-op statistics report
        s.undo()?; // steps back over the finish change itself
        assert_eq!(s.inspect()["project"]["board"]["finish"], "HASL");
        Ok(())
    }
    #[test]
    fn references_can_be_renumbered_in_placement_order_with_a_dry_run() -> Result<(), Error> {
        let mut s = Session::default();
        for (reference, kind, x, y) in [
            ("R5", "resistor", 30.0, 10.0),
            ("R3", "resistor", 20.0, 20.0),
            ("R1", "resistor", 10.0, 30.0),
            ("C2", "capacitor", 5.0, 5.0),
        ] {
            s.execute(
                "component.add",
                json!({"kind":kind,"reference":reference,"value":"1","position":{"x":x,"y":y}}),
                None,
            )?;
        }
        let plan = s.execute(
            "components.reannotate",
            json!({"prefix":"R","dry_run":true}),
            None,
        )?["result"]
            .clone();
        assert_eq!(plan["dry_run"], true);
        assert_eq!(plan["count"], 3);
        assert_eq!(plan["renamed"][0]["from"], "R5", "topmost first");
        assert_eq!(plan["renamed"][0]["to"], "R1");
        assert_eq!(plan["renamed"][2]["to"], "R3");
        let before = s.inspect()["project"].clone();
        assert!(
            s.execute(
                "components.reannotate",
                json!({"prefix":"R","start":0}),
                None
            )
            .is_err(),
            "the start number stays positive"
        );
        assert!(
            s.execute("components.reannotate", json!({"prefix":"R9"}), None)
                .is_err(),
            "the prefix is letters only"
        );
        assert!(
            s.execute("components.reannotate", json!({"prefix":"Q"}), None)
                .is_err(),
            "an empty group is an error, not a silent success"
        );
        assert_eq!(
            s.inspect()["project"],
            before,
            "dry runs and refusals change nothing"
        );
        s.execute("components.reannotate", json!({}), None)?;
        let refs: Vec<String> = s.inspect()["project"]["components"]
            .as_array()
            .ok_or_else(missing)?
            .iter()
            .map(|c| c["reference"].as_str().unwrap_or_default().to_string())
            .collect();
        assert_eq!(
            refs,
            ["R1", "R2", "R3", "C1"],
            "each prefix renumbers itself"
        );
        s.execute(
            "component.add",
            json!({"kind":"resistor","reference":"r7","value":"1","position":{"x":40.0,"y":40.0}}),
            None,
        )?;
        let before = s.inspect()["project"].clone();
        assert!(
            s.execute(
                "components.reannotate",
                json!({"prefix":"R","start":1}),
                None
            )
            .is_err(),
            "two letter spellings that would produce the same names refuse the call"
        );
        assert_eq!(s.inspect()["project"], before);
        Ok(())
    }
    #[test]
    fn a_placed_footprint_can_be_stepped_and_repeated_as_one_undoable_array() -> Result<(), Error> {
        let mut s = Session::default();
        let chip = s.execute(
            "component.place_library",
            json!({"path":library_fixture("lm358.kicad_sym"),"symbol":"LM358","reference":"U1","position":{"x":20.0,"y":10.0}}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        s.execute(
            "footprint.assign",
            json!({"id":chip,"path":library_fixture("DIP-8_W7.62mm.kicad_mod")}),
            None,
        )?;
        s.execute("board.sync", json!({}), None)?;
        let fp = s.inspect()["project"]["board"]["footprints"][0]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let array = s.execute(
            "footprint.array",
            json!({"footprint":fp,"columns":3,"rows":2,"dx":5.0,"dy":6.0}),
            None,
        )?["result"]
            .clone();
        assert_eq!(array["count"], 6, "the source plus five copies");
        let project = s.inspect()["project"].clone();
        let footprints = project["board"]["footprints"]
            .as_array()
            .ok_or_else(missing)?;
        assert_eq!(footprints.len(), 6);
        let refs: Vec<String> = project["components"]
            .as_array()
            .ok_or_else(missing)?
            .iter()
            .map(|c| c["reference"].as_str().unwrap_or_default().to_string())
            .collect();
        assert_eq!(refs, ["U1", "U1-2", "U1-3", "U1-4", "U1-5", "U1-6"]);
        let last = footprints.last().ok_or_else(missing)?;
        assert_eq!(
            last["position"]["x"],
            footprints[0]["position"]["x"]
                .as_f64()
                .ok_or_else(missing)?
                + 10.0,
            "two columns right"
        );
        assert_eq!(
            last["position"]["y"],
            footprints[0]["position"]["y"]
                .as_f64()
                .ok_or_else(missing)?
                + 6.0,
            "one row down"
        );
        for bad in [
            json!({"footprint":fp,"columns":0,"rows":2,"dx":5.0,"dy":6.0}),
            json!({"footprint":fp,"columns":1,"rows":1,"dx":5.0,"dy":6.0}),
            json!({"footprint":fp,"columns":2,"rows":2,"dx":0.0,"dy":6.0}),
            json!({"footprint":999,"columns":2,"rows":2,"dx":5.0,"dy":6.0}),
        ] {
            assert!(s.execute("footprint.array", bad, None).is_err());
        }
        assert_eq!(s.inspect()["project"], project, "refusals change nothing");
        s.undo()?;
        let back = s.inspect()["project"].clone();
        assert_eq!(
            back["board"]["footprints"]
                .as_array()
                .ok_or_else(missing)?
                .len(),
            1
        );
        assert_eq!(back["components"].as_array().ok_or_else(missing)?.len(), 1);
        Ok(())
    }
    #[test]
    fn net_chains_walk_series_parts_and_report_how_the_walk_ends() -> Result<(), Error> {
        let mut s = Session::default();
        let mut ids = vec![];
        for r in ["R1", "R2", "R3", "R4", "R5", "R6", "R7", "R8", "R9"] {
            ids.push(
                s.execute(
                    "component.add",
                    json!({"kind":"resistor","reference":r,"position":{"x":10.0,"y":10.0}}),
                    None,
                )?["result"]["id"]
                    .as_u64()
                    .ok_or_else(missing)?,
            );
        }
        s.execute(
            "netlist.import",
            json!({"nets":[
                {"name":"A","pins":[{"reference":"R1","pin":"2"},{"reference":"R2","pin":"1"}]},
                {"name":"B","pins":[{"reference":"R2","pin":"2"},{"reference":"R3","pin":"1"}]},
                {"name":"C","pins":[{"reference":"R3","pin":"2"},{"reference":"R4","pin":"1"},{"reference":"R5","pin":"1"}]},
                {"name":"D","pins":[{"reference":"R4","pin":"2"}]},
                {"name":"L1","pins":[{"reference":"R6","pin":"1"},{"reference":"R7","pin":"2"}]},
                {"name":"L2","pins":[{"reference":"R7","pin":"1"},{"reference":"R8","pin":"2"}]},
                {"name":"L3","pins":[{"reference":"R8","pin":"1"},{"reference":"R6","pin":"2"}]},
            ]}),
            None,
        )?;
        let chain = |s: &mut Session, component: Id, pin: &str| -> Result<Value, Error> {
            Ok(
                s.execute("net.chain", json!({"component":component,"pin":pin}), None)?["result"]
                    .clone(),
            )
        };
        let open = chain(&mut s, ids[0], "1")?;
        assert_eq!(open["terminated"], "open", "a pin with no net ends at once");
        assert_eq!(open["parts"], 1);
        assert!(open["steps"][0]["net"].is_null());
        let branch = chain(&mut s, ids[0], "2")?;
        assert_eq!(branch["parts"], 3, "R1 through R2 into R3");
        assert_eq!(branch["terminated"], "branch", "two ways forward at R3");
        let refs: Vec<&str> = branch["steps"]
            .as_array()
            .ok_or_else(missing)?
            .iter()
            .map(|q| q["reference"].as_str().unwrap_or_default())
            .collect();
        assert_eq!(refs, ["R1", "R2", "R3"]);
        assert_eq!(branch["steps"][2]["net_name"], "C");
        let back = chain(&mut s, ids[2], "1")?;
        assert_eq!(
            back["terminated"], "open",
            "the chain walks both directions"
        );
        assert_eq!(back["parts"], 3);
        assert_eq!(back["steps"][2]["pin"], "1");
        let start_branch = chain(&mut s, ids[3], "1")?;
        assert_eq!(start_branch["parts"], 1);
        assert_eq!(start_branch["terminated"], "branch");
        let cycle = chain(&mut s, ids[5], "1")?;
        assert_eq!(
            cycle["terminated"], "loop",
            "a ring of series parts is detected"
        );
        assert_eq!(
            cycle["parts"], 3,
            "each ring part is listed once before the walk notices the close"
        );
        s.execute(
            "component.place_library",
            json!({"path":library_fixture("lm358.kicad_sym"),"symbol":"LM358","reference":"U1","position":{"x":40.0,"y":10.0}}),
            None,
        )?;
        s.execute(
            "netlist.import",
            json!({"nets":[{"name":"M","pins":[{"reference":"U1","pin":"1"},{"reference":"R9","pin":"1"}]}]}),
            None,
        )?;
        let multi = chain(&mut s, ids[8], "1")?;
        assert_eq!(
            multi["terminated"], "multi",
            "a many-pin part stops the chain"
        );
        assert_eq!(multi["parts"], 2);
        assert_eq!(multi["steps"][1]["reference"], "U1");
        for bad in [
            json!({"component":9999,"pin":"1"}),
            json!({"component":ids[0],"pin":"9"}),
            json!({"component":ids[0],"pin":"12345678901234567"}),
            json!({"component":ids[0]}),
        ] {
            assert!(s.execute("net.chain", bad, None).is_err());
        }
        Ok(())
    }
    #[test]
    fn recorded_stackup_drives_impedance_estimates_and_width_advice() -> Result<(), Error> {
        let mut s = Session::default();
        s.execute(
            "component.add",
            json!({"kind":"resistor","reference":"R1","position":{"x":10.0,"y":10.0}}),
            None,
        )?;
        s.execute(
            "component.add",
            json!({"kind":"resistor","reference":"R2","position":{"x":30.0,"y":10.0}}),
            None,
        )?;
        s.execute(
            "netlist.import",
            json!({"nets":[{"name":"SIG","pins":[{"reference":"R1","pin":"1"},{"reference":"R2","pin":"1"}]}]}),
            None,
        )?;
        let net = s.inspect()["project"]["nets"][0]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        s.execute(
            "track.add",
            json!({"net":net,"start":{"x":12.0,"y":10.0},"end":{"x":28.0,"y":10.0},"width":0.3}),
            None,
        )?;
        let report = s.execute("net.impedance", json!({}), None)?["result"].clone();
        assert_eq!(
            report["count"], 1,
            "only a net with copper has an impedance"
        );
        assert_eq!(report["rows"][0]["net_name"], "SIG");
        assert_eq!(report["rows"][0]["width"], 0.3);
        assert_eq!(report["dielectric"], 1.6, "the default board thickness");
        assert_eq!(report["er"], 4.4, "the default glass-epoxy permittivity");
        let wide = report["rows"][0]["impedance_ohm"]
            .as_f64()
            .ok_or_else(missing)?;
        assert!((120.0..=140.0).contains(&wide), "a plausible 0.3 mm value");
        let advised = s.execute("net.impedance", json!({"target":60.0}), None)?["result"].clone();
        assert_eq!(
            advised["rows"][0]["delta_ohm"],
            ((wide - 60.0) * 100.0).round() / 100.0
        );
        let suggested = advised["rows"][0]["recommended_width"]
            .as_f64()
            .ok_or_else(missing)?;
        assert!(
            (1.5..=3.0).contains(&suggested),
            "a wide track is needed for 60 ohms on this stackup"
        );
        s.execute(
            "track.add",
            json!({"net":net,"start":{"x":12.0,"y":12.0},"end":{"x":28.0,"y":12.0},"width":0.6}),
            None,
        )?;
        let thicker = s.execute("net.impedance", json!({}), None)?["result"].clone();
        assert_eq!(thicker["rows"][0]["width"], 0.6, "the widest track decides");
        assert!(
            thicker["rows"][0]["impedance_ohm"]
                .as_f64()
                .ok_or_else(missing)?
                < wide,
            "wider copper means lower impedance"
        );
        let stack =
            s.execute("board.set_stackup", json!({"thickness":0.8,"er":3.5}), None)?["result"]
                .clone();
        assert_eq!(stack["thickness"], 0.8);
        assert_eq!(stack["er"], 3.5);
        let thinner = s.execute("net.impedance", json!({}), None)?["result"].clone();
        assert_eq!(thinner["dielectric"], 0.8);
        assert_eq!(thinner["er"], 3.5);
        assert_ne!(
            thinner["rows"][0]["impedance_ohm"], thicker["rows"][0]["impedance_ohm"],
            "the stackup feeds the estimate"
        );
        for bad in [
            json!({}),
            json!({"thickness":0.05}),
            json!({"er":0.5}),
            json!({"er":21.0}),
            json!({"er":5.0,"extra":1}),
        ] {
            assert!(s.execute("board.set_stackup", bad, None).is_err());
        }
        for bad in [
            json!({"net":9999}),
            json!({"target":0.5}),
            json!({"target":1001.0}),
            json!({"width":0.3}),
        ] {
            assert!(s.execute("net.impedance", bad, None).is_err());
        }
        let before = s.inspect()["project"].clone();
        s.undo()?;
        s.undo()?;
        let back = s.inspect()["project"].clone();
        assert_ne!(back, before, "the stackup edit undoes");
        assert_eq!(back["board"]["er"], 4.4);
        assert_eq!(back["board"]["thickness"], 1.6);
        Ok(())
    }
    #[test]
    fn drill_origin_is_recorded_undone_and_feeds_the_drill_map() -> Result<(), Error> {
        let mut s = Session::default();
        s.execute("net.create", json!({"name":"SIG"}), None)?;
        let net = s.inspect()["project"]["nets"][0]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        s.execute(
            "via.place",
            json!({"net":net,"position":{"x":30.0,"y":20.0},"diameter":0.7,"drill":0.3}),
            None,
        )?;
        let set = s.execute(
            "board.set_drill_origin",
            json!({"position":{"x":20.0,"y":10.0}}),
            None,
        )?["result"]
            .clone();
        assert_eq!(set["drill_origin"]["x"], 20.0);
        assert_eq!(set["drill_origin"]["y"], 10.0);
        let drill = pcbcraft_io::gerber_files(&s.project)?["pcb-Drill.drl"].clone();
        assert!(
            drill.contains("X10.000Y10.000\n"),
            "the recorded origin moves drill coordinates"
        );
        assert!(
            !drill.contains("X30.000Y20.000"),
            "absolute coordinates are gone once an origin is recorded"
        );
        let back = s.execute("board.reset_drill_origin", json!({}), None)?["result"].clone();
        assert_eq!(back["drill_origin"]["x"], 0.0);
        assert_eq!(back["drill_origin"]["y"], 0.0);
        for bad in [
            json!({"position":{"x":1001.0,"y":0.0}}),
            json!({"position":{"x":0.0,"y":-1001.0}}),
            json!({"position":[0.0]}),
            json!({"position":{"x":0.0,"y":0.0},"extra":1}),
            json!({}),
        ] {
            assert!(s.execute("board.set_drill_origin", bad, None).is_err());
        }
        assert!(
            s.execute("board.reset_drill_origin", json!({"x":0.0}), None)
                .is_err()
        );
        let before = s.inspect()["project"].clone();
        s.execute(
            "board.set_drill_origin",
            json!({"position":{"x":5.0,"y":5.0}}),
            None,
        )?;
        s.undo()?;
        assert_eq!(s.inspect()["project"], before, "the origin edit undoes");
        Ok(())
    }
    #[test]
    fn layer_records_are_named_replaced_ordered_undone_and_cleared() -> Result<(), Error> {
        let mut s = Session::default();
        let core = s.execute(
            "board.set_layer_record",
            json!({"name":"Core","kind":"core","thickness":1.2}),
            None,
        )?["result"]
            .clone();
        assert_eq!(core["count"], 1, "the first note is recorded");
        s.execute(
            "board.set_layer_record",
            json!({"name":"F.Cu","kind":"copper","thickness":0.035}),
            None,
        )?;
        let replaced = s.execute(
            "board.set_layer_record",
            json!({"name":"Core","kind":"prepreg","thickness":0.2}),
            None,
        )?["result"]
            .clone();
        assert_eq!(replaced["count"], 2, "a repeated name replaces its note");
        let records = s.inspect()["project"]["board"]["layer_records"].clone();
        assert_eq!(records[0]["name"], "Core");
        assert_eq!(records[0]["kind"], "prepreg");
        assert_eq!(records[0]["thickness"], 0.2);
        assert_eq!(records[1]["name"], "F.Cu", "notes stay ordered by name");
        for bad in [
            json!({"name":"  ","kind":"core","thickness":1.0}),
            json!({"name":"L1","kind":"bronze","thickness":1.0}),
            json!({"name":"L1","kind":"core","thickness":-0.1}),
            json!({"name":"L1","kind":"core","thickness":101.0}),
            json!({"name":"L1","kind":"core"}),
        ] {
            assert!(
                s.execute("board.set_layer_record", bad, None).is_err(),
                "bad layer notes are refused"
            );
        }
        assert!(
            s.execute("board.clear_layer_records", json!({"name":"Core"}), None)
                .is_err(),
            "board.clear_layer_records takes an empty object"
        );
        s.undo()?;
        let records = s.inspect()["project"]["board"]["layer_records"].clone();
        assert_eq!(records[0]["kind"], "core", "a replace undoes cleanly");
        assert_eq!(records[0]["thickness"], 1.2);
        let cleared = s.execute("board.clear_layer_records", json!({}), None)?["result"].clone();
        assert_eq!(cleared["removed"], 2, "clearing reports what it dropped");
        assert!(
            s.inspect()["project"]["board"]
                .get("layer_records")
                .is_none_or(|a| a.as_array().is_some_and(|a| a.is_empty())),
            "the board is left with no notes"
        );
        s.undo()?;
        assert_eq!(
            s.inspect()["project"]["board"]["layer_records"]
                .as_array()
                .map_or(0, |a| a.len()),
            2,
            "the clear undoes"
        );
        Ok(())
    }
    #[test]
    fn project_metadata_records_replaces_clears_and_refuses_bad_keys() -> Result<(), Error> {
        let mut s = Session::default();
        let out = s.execute(
            "project.set_meta",
            json!({"key":"title","value":"Sensor Board"}),
            None,
        )?["result"]
            .clone();
        assert_eq!(out["value"], "Sensor Board", "the value is reported");
        s.execute(
            "project.set_meta",
            json!({"key":"revision","value":"2.1"}),
            None,
        )?;
        let meta = s.inspect()["project"]["meta"].clone();
        assert_eq!(meta["title"], "Sensor Board", "metadata is stored");
        assert_eq!(meta["revision"], "2.1", "entries accumulate");
        s.execute(
            "project.set_meta",
            json!({"key":"title","value":"Sensor Board Rev A"}),
            None,
        )?;
        for bad in [
            json!({"key":"","value":"x"}),
            json!({"key":"Title","value":"x"}),
            json!({"key":"a-b","value":"x"}),
            json!({"key":"title","value":"same","again":1}),
        ] {
            assert!(
                s.execute("project.set_meta", bad, None).is_err(),
                "bad metadata requests are refused"
            );
        }
        assert!(
            s.execute(
                "project.set_meta",
                json!({"key":"title","value":"Sensor Board Rev A"}),
                None,
            )
            .is_err(),
            "recording the same value again is refused"
        );
        s.execute("project.clear_meta", json!({"key":"revision"}), None)?;
        assert!(
            s.inspect()["project"]["meta"].get("revision").is_none(),
            "clearing removes the entry"
        );
        assert!(
            s.execute("project.clear_meta", json!({"key":"revision"}), None)
                .is_err(),
            "clearing an absent entry is refused"
        );
        s.undo()?;
        assert_eq!(
            s.inspect()["project"]["meta"]["revision"],
            "2.1",
            "clearing undoes"
        );
        s.execute(
            "project.set_meta",
            json!({"key":"company","value":"Quasar"}),
            None,
        )?;
        s.undo()?;
        assert!(
            s.inspect()["project"]["meta"].get("company").is_none(),
            "recording undoes"
        );
        Ok(())
    }
    #[test]
    fn dimensions_measure_group_and_undo_in_one_step() -> Result<(), Error> {
        let mut s = Session::default();
        let out = s.execute(
            "dimension.add",
            json!({"start":[0.0,0.0],"end":[30.0,0.0],"layer":"schematic"}),
            None,
        )?["result"]
            .clone();
        assert_eq!(out["distance"], 30.0, "the distance is reported");
        let gid = out["id"].as_u64().ok_or_else(missing)?;
        let groups = s.inspect()["project"]["graphic_groups"].clone();
        let group = groups
            .as_array()
            .and_then(|g| g.iter().find(|g| g["id"] == gid))
            .cloned()
            .ok_or_else(missing)?;
        assert_eq!(
            group["graphics"].as_array().ok_or_else(missing)?.len(),
            4,
            "witness lines, dimension line and text are grouped"
        );
        let graphics = s.inspect()["project"]["graphics"].clone();
        let rows = graphics.as_array().ok_or_else(missing)?;
        let measured = rows
            .iter()
            .find(|g| g["shape"] == "text")
            .cloned()
            .ok_or_else(missing)?;
        assert_eq!(measured["text"], "30.00", "the text states the span");
        s.execute(
            "graphic.translate",
            json!({"id":gid,"delta":[0.0,10.0]}),
            None,
        )?;
        let moved = s.inspect()["project"]["graphics"].clone();
        let moved_rows = moved.as_array().ok_or_else(missing)?;
        let first = moved_rows
            .iter()
            .find(|g| g["shape"] == "text")
            .cloned()
            .ok_or_else(missing)?;
        let near = |a: f64, b: f64| (a - b).abs() < 1e-9;
        assert!(
            near(first["points"][0]["y"].as_f64().unwrap_or(0.0), 12.8),
            "the whole dimension shifts together"
        );
        s.undo()?;
        s.undo()?;
        assert!(
            s.inspect()["project"]["graphics"]
                .as_array()
                .is_some_and(|r| r.is_empty())
                && s.inspect()["project"].get("graphic_groups").is_none(),
            "the dimension undoes as one step"
        );
        let out = s.execute(
            "dimension.add",
            json!({"start":[0.0,0.0],"end":[3.0,4.0],"layer":"board","offset":-3.0}),
            None,
        )?["result"]
            .clone();
        assert_eq!(out["distance"], 5.0, "diagonal spans measure exactly");
        for bad in [
            json!({"start":[0.0,0.0],"end":[0.0,0.0],"layer":"schematic"}),
            json!({"start":[0.0,0.0],"end":[1.0,0.0],"layer":"paste"}),
            json!({"start":[0.0,0.0],"end":[1.0,0.0],"layer":"schematic","offset":200.0}),
            json!({"start":[0.0,0.0],"end":[1.0,0.0],"layer":"schematic","nope":1}),
        ] {
            assert!(
                s.execute("dimension.add", bad, None).is_err(),
                "bad dimension requests are refused"
            );
        }
        Ok(())
    }
    #[test]
    fn board_outline_reshapes_the_board_and_its_edge_checks() -> Result<(), Error> {
        let mut s = Session::default();
        let r = s.execute(
            "board.set_outline",
            json!({"points":[[0.0,0.0],[60.0,0.0],[60.0,40.0],[30.0,40.0],[30.0,20.0],[0.0,20.0]]}),
            None,
        )?["result"]
            .clone();
        assert_eq!(r["vertices"], 6);
        assert_eq!(r["area"], 1800.0);
        let stats = s.execute("board.statistics", json!({}), None)?["result"].clone();
        assert_eq!(
            stats["board_area"], 1800.0,
            "statistics count the traced shape, not the bounding box"
        );
        let a = s.execute("net.create", json!({"name":"A"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        s.execute(
            "track.add",
            json!({"net":a,"start":{"x":5.0,"y":30.0},"end":{"x":10.0,"y":30.0},"width":0.2}),
            None,
        )?;
        assert!(
            s.checks()["drc"]
                .as_array()
                .ok_or_else(missing)?
                .iter()
                .any(|f| f["code"] == "board_edge"),
            "copper in the cut-away corner sits outside the traced outline"
        );
        s.undo()?;
        s.execute(
            "track.add",
            json!({"net":a,"start":{"x":40.0,"y":30.0},"end":{"x":50.0,"y":30.0},"width":0.2}),
            None,
        )?;
        assert!(
            s.checks()["drc"].as_array().ok_or_else(missing)?.is_empty(),
            "the same copper inside the traced outline is clean"
        );
        s.undo()?;
        for bad in [
            json!({"points":[[0.0,0.0],[10.0,0.0]]}),
            json!({"points":[[0.0,0.0],[10.0,0.0],[10.0,0.0]]}),
            json!({"points":[[1.0,1.0],[2.0,1.0],[1.5,1.0]]}),
            json!({"points":[[-1.0,0.0],[10.0,0.0],[10.0,10.0]]}),
        ] {
            assert!(
                s.execute("board.set_outline", bad, None).is_err(),
                "degenerate outlines are refused"
            );
        }
        s.execute(
            "board.set_outline",
            json!({"points":[[10.0,10.0],[30.0,10.0],[30.0,30.0]]}),
            None,
        )?;
        s.execute("board.clear_outline", json!({}), None)?;
        s.undo()?;
        assert_eq!(
            s.project.board.outline.as_ref().map_or(0, |o| o.len()),
            3,
            "one undo restores the traced outline"
        );
        s.execute("board.clear_outline", json!({}), None)?;
        let stats = s.execute("board.statistics", json!({}), None)?["result"].clone();
        assert_eq!(
            stats["board_area"], 400.0,
            "the rectangle keeps the outline's bounding box size"
        );
        assert!(
            s.execute("board.clear_outline", json!({}), None).is_err(),
            "clearing twice is a missing outline"
        );
        Ok(())
    }
    #[test]
    fn radial_dimensions_measure_circles_and_undo_as_one_unit() -> Result<(), Error> {
        let mut s = Session::default();
        let r = s.execute(
            "dimension.radius",
            json!({"center":{"x":10.0,"y":10.0},"point":{"x":14.0,"y":10.0},"layer":"board"}),
            None,
        )?["result"]
            .clone();
        assert_eq!(r["radius"], 4.0);
        assert_eq!(s.project.graphic_groups.len(), 1);
        assert_eq!(s.project.graphic_groups[0].graphics.len(), 3);
        let last = s.project.graphics.len();
        assert_eq!(s.project.graphics[last - 3].shape, "circle");
        assert_eq!(s.project.graphics[last - 3].width, 4.0);
        assert_eq!(
            s.project.graphics[last - 2].points[0],
            Point::new(10.0, 10.0)
        );
        assert_eq!(
            s.project.graphics[last - 1].text,
            "R 4.00",
            "the label carries the measured radius"
        );
        s.undo()?;
        assert!(
            s.project.graphics.is_empty() && s.project.graphic_groups.is_empty(),
            "one undo removes every piece of the dimension"
        );
        let d = s.execute(
            "dimension.diameter",
            json!({"center":{"x":0.0,"y":0.0},"point":{"x":3.0,"y":4.0},"layer":"schematic"}),
            None,
        )?["result"]
            .clone();
        assert_eq!(d["radius"], 5.0);
        let last = s.project.graphics.len();
        assert_eq!(
            s.project.graphics[last - 2].points[0],
            Point::new(-3.0, -4.0),
            "the diameter leader starts on the far rim"
        );
        assert_eq!(s.project.graphics[last - 1].text, "\u{d8} 10.00");
        s.undo()?;
        assert!(s.project.graphics.is_empty());
        for bad in [
            json!({"center":{"x":1.0,"y":1.0},"point":{"x":1.0,"y":1.0},"layer":"board"}),
            json!({"center":{"x":1.0,"y":1.0},"point":{"x":2.0,"y":1.0},"layer":"copper"}),
            json!({"center":{"x":1.0,"y":1.0},"point":{"x":2.0,"y":1.0}}),
        ] {
            assert!(
                s.execute("dimension.radius", bad, None).is_err(),
                "bad radial dimension requests are refused"
            );
        }
        assert!(
            s.execute(
                "dimension.diameter",
                json!({"center":{"x":1.0,"y":1.0},"point":{"x":1.0,"y":1.0},"layer":"board"}),
                None,
            )
            .is_err()
        );
        Ok(())
    }
    #[test]
    fn curved_tracks_measure_shorten_and_keep_their_shape() -> Result<(), Error> {
        let mut s = Session::default();
        let a = s.execute("net.create", json!({"name":"A"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let b = s.execute("net.create", json!({"name":"B"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let t = s.execute(
            "track.add_arc",
            json!({"net":a,"start":{"x":10.0,"y":10.0},"mid":{"x":15.0,"y":12.0},"end":{"x":20.0,"y":10.0},"width":0.2}),
            None,
        )?["result"]
        .clone();
        let length = t["length"].as_f64().ok_or_else(missing)?;
        assert!(
            (length - 11.037).abs() < 0.01,
            "the arc is longer than its 10 mm chord"
        );
        s.execute(
            "track.add",
            json!({"net":b,"start":{"x":14.0,"y":12.09},"end":{"x":16.0,"y":12.09},"width":0.2}),
            None,
        )?;
        let stats = s.execute("board.statistics", json!({}), None)?["result"].clone();
        let total = stats["routed_length"]["total"]
            .as_f64()
            .ok_or_else(missing)?;
        assert!(
            (total - (length + 2.0)).abs() < 0.01,
            "statistics count the curve, not the chord"
        );
        assert!(
            s.checks()["drc"]
                .as_array()
                .ok_or_else(missing)?
                .iter()
                .any(|f| f["code"] == "clearance" || f["code"] == "copper_short"),
            "copper 0.09 mm over the curve apex must be reported"
        );
        s.execute("track.straighten", json!({"id": t["id"]}), None)?;
        assert!(
            s.checks()["drc"].as_array().ok_or_else(missing)?.is_empty(),
            "straightening moves the copper away"
        );
        s.undo()?;
        assert!(s.project.board.tracks[0].arc.is_some());
        assert!(
            s.checks()["drc"]
                .as_array()
                .ok_or_else(missing)?
                .iter()
                .any(|f| f["code"] == "clearance" || f["code"] == "copper_short"),
            "undo restores the curve and its finding"
        );
        assert!(
            s.execute(
                "track.split",
                json!({"id": t["id"], "at": {"x": 15.0, "y": 12.0}}),
                None,
            )
            .is_err(),
            "a curved track cannot be split"
        );
        s.execute(
            "track.move",
            json!({"id": t["id"], "delta": {"x": 0.0, "y": -2.0}}),
            None,
        )?;
        assert_eq!(
            s.project.board.tracks[0].arc,
            Some(Point::new(15.0, 10.0)),
            "moving a curved track carries its midpoint along"
        );
        assert!(
            s.checks()["drc"].as_array().ok_or_else(missing)?.is_empty(),
            "the moved curve clears the straight track"
        );
        s.execute(
            "track.set_end",
            json!({"id": t["id"], "end": "end", "position": {"x": 22.0, "y": 8.0}}),
            None,
        )?;
        assert!(
            s.project.board.tracks[0].arc.is_none(),
            "dragging an end straightens the track"
        );
        for bad in [
            json!({"net":a,"start":{"x":10.0,"y":10.0},"mid":{"x":15.0,"y":10.0},"end":{"x":20.0,"y":10.0},"width":0.2}),
            json!({"net":a,"start":{"x":10.0,"y":10.0},"mid":{"x":10.0,"y":10.0},"end":{"x":20.0,"y":10.0},"width":0.2}),
        ] {
            assert!(
                s.execute("track.add_arc", bad, None).is_err(),
                "collinear or repeated points cannot describe an arc"
            );
        }
        s.undo()?;
        assert_eq!(s.project.board.tracks[0].arc, Some(Point::new(15.0, 10.0)));
        Ok(())
    }
    #[test]
    fn locked_footprints_refuse_placement_and_unlock_again() -> Result<(), Error> {
        let mut s = Session::default();
        s.execute(
            "component.add",
            json!({"kind":"connector","position":[10,10],"reference":"J1","value":"edge"}),
            None,
        )?;
        s.execute("board.sync", json!({}), None)?;
        let fp = s.inspect()["project"]["board"]["footprints"][0]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let out =
            s.execute("footprint.set_locked", json!({"id":fp,"locked":true}), None)?["result"]
                .clone();
        assert_eq!(out["locked"], true, "the lock is reported");
        assert_eq!(
            s.inspect()["project"]["board"]["footprints"][0]["locked"],
            true,
            "the lock is stored"
        );
        for (cmd, params) in [
            ("footprint.move", json!({"id":fp,"position":[20,20]})),
            ("footprint.rotate", json!({"id":fp,"degrees":90})),
            ("footprint.mirror", json!({"id":fp,"axis":"horizontal"})),
            (
                "footprint.array",
                json!({"footprint":fp,"columns":2,"rows":1,"dx":5,"dy":5}),
            ),
            ("footprint.dock_edge", json!({"id":fp,"edge":"left"})),
        ] {
            assert!(
                s.execute(cmd, params, None).is_err(),
                "{cmd} refuses a locked footprint"
            );
        }
        s.execute(
            "footprint.set_locked",
            json!({"id":fp,"locked":false}),
            None,
        )?;
        assert!(
            s.inspect()["project"]["board"]["footprints"][0]
                .get("locked")
                .is_none(),
            "unlocking drops the stored flag"
        );
        s.execute("footprint.move", json!({"id":fp,"position":[20,20]}), None)?;
        s.execute("footprint.set_locked", json!({"id":fp,"locked":true}), None)?;
        s.undo()?;
        assert!(
            s.inspect()["project"]["board"]["footprints"][0]
                .get("locked")
                .is_none(),
            "the lock undoes"
        );
        for bad in [
            json!({"id":999,"locked":true}),
            json!({"id":fp}),
            json!({"id":fp,"locked":true,"nope":1}),
        ] {
            assert!(
                s.execute("footprint.set_locked", bad, None).is_err(),
                "bad lock requests are refused"
            );
        }
        Ok(())
    }
    #[test]
    fn footprints_dock_flush_to_board_edges_and_undo() -> Result<(), Error> {
        let mut s = Session::default();
        s.execute(
            "component.add",
            json!({"kind":"connector","position":[10,10],"reference":"J1","value":"edge"}),
            None,
        )?;
        s.execute("board.sync", json!({}), None)?;
        let fp = s.inspect()["project"]["board"]["footprints"][0]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let near = |a: f64, b: f64| (a - b).abs() < 1e-6;
        let pad_x = |s: &mut Session, want: &str| -> Result<f64, Error> {
            let pads = s.execute("pad.table", json!({}), None)?["result"]["pads"].clone();
            let row = pads
                .as_array()
                .and_then(|rows| rows.iter().find(|r| r["pad"] == want))
                .cloned()
                .ok_or_else(missing)?;
            row["position"]["x"].as_f64().ok_or_else(missing)
        };
        let left = s.execute("footprint.dock_edge", json!({"id":fp,"edge":"left"}), None)?;
        assert!(
            near(left["result"]["position"]["x"].as_f64().unwrap_or(0.0), 3.3),
            "the leftmost pad radius lands on x = 0"
        );
        assert!(
            near(pad_x(&mut s, "1")?, 0.8) && near(pad_x(&mut s, "2")?, 5.8),
            "pads sit flush along the left edge"
        );
        let top = s.execute("footprint.dock_edge", json!({"id":fp,"edge":"top"}), None)?;
        assert!(
            near(top["result"]["position"]["y"].as_f64().unwrap_or(0.0), 39.2),
            "the topmost pad radius lands on the board height"
        );
        assert!(
            s.execute("footprint.dock_edge", json!({"id":fp,"edge":"top"}), None)
                .is_err(),
            "docking an already flush footprint is refused"
        );
        s.undo()?;
        let back = s.inspect()["project"]["board"]["footprints"][0]["position"]["y"]
            .as_f64()
            .ok_or_else(missing)?;
        assert!(near(back, 10.0), "the vertical dock undoes");
        for bad in [
            json!({"id":999,"edge":"left"}),
            json!({"id":fp,"edge":"middle"}),
            json!({"id":fp}),
            json!({"id":fp,"edge":"left","nope":1}),
        ] {
            assert!(
                s.execute("footprint.dock_edge", bad, None).is_err(),
                "bad dock requests are refused"
            );
        }
        Ok(())
    }
    #[test]
    fn schematic_table_draws_sorted_rows_and_refuses_bad_requests() -> Result<(), Error> {
        let mut s = Session::default();
        s.execute(
            "component.add",
            json!({"kind":"resistor","position":[10,10],"reference":"R2","value":"10k"}),
            None,
        )?;
        s.execute(
            "component.add",
            json!({"kind":"resistor","position":[20,20],"reference":"R1","value":"1k"}),
            None,
        )?;
        let count = |s: &Session| -> Result<usize, Error> {
            Ok(s.inspect()["project"]["graphics"]
                .as_array()
                .ok_or_else(missing)?
                .len())
        };
        let before = count(&s)?;
        let out = s.execute(
            "schematic.table",
            json!({"position":[0,60],"max_rows":1}),
            None,
        )?["result"]
            .clone();
        assert_eq!(out["rows"], 1, "row cap honoured");
        assert_eq!(out["columns"], 2, "reference and value by default");
        assert_eq!(out["truncated"], true, "truncation is reported");
        let graphics = s.inspect()["project"]["graphics"].clone();
        let rows = graphics.as_array().ok_or_else(missing)?;
        assert_eq!(rows.len() - before, 10, "grid lines plus text");
        let texts: Vec<&str> = rows
            .iter()
            .filter(|g| g["shape"] == "text")
            .map(|g| g["text"].as_str().unwrap_or(""))
            .collect();
        assert_eq!(
            texts,
            vec!["reference", "value", "R1", "1k"],
            "header then the first sorted row"
        );
        s.undo()?;
        assert_eq!(count(&s)?, before, "the whole table undoes as one step");
        let out = s.execute(
            "schematic.table",
            json!({"position":[0,60],"fields":["reference"]}),
            None,
        )?["result"]
            .clone();
        assert_eq!(out["columns"], 1, "one requested column");
        assert_eq!(out["rows"], 2, "both sorted rows");
        for bad in [
            json!({"position":[0,60],"fields":[""]}),
            json!({"position":[0,60],"fields":["a","b","c","d"]}),
            json!({"position":[0,60],"max_rows":0}),
            json!({"position":[99_990,0]}),
            json!({"position":[0,60],"nope":1}),
        ] {
            assert!(
                s.execute("schematic.table", bad, None).is_err(),
                "bad table requests are refused"
            );
        }
        Ok(())
    }
    #[test]
    fn castellated_flags_mark_pads_reportedly_and_undo() -> Result<(), Error> {
        let mut s = Session::default();
        s.execute(
            "component.add",
            json!({"kind":"connector","position":[10,10],"reference":"J1","value":"edge"}),
            None,
        )?;
        s.execute("board.sync", json!({}), None)?;
        let fp = s.inspect()["project"]["board"]["footprints"][0]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let on = s.execute(
            "footprint.set_castellated",
            json!({"id":fp,"pad":"1","on":true}),
            None,
        )?["result"]
            .clone();
        assert_eq!(on["castellated"], true, "the flag is reported");
        let pads = s.execute("pad.table", json!({}), None)?["result"]["pads"].clone();
        let first = pads
            .as_array()
            .and_then(|rows| rows.iter().find(|r| r["pad"] == "1"))
            .cloned()
            .ok_or_else(missing)?;
        assert_eq!(first["castellated"], true, "the pad table shows the flag");
        let second = pads
            .as_array()
            .and_then(|rows| rows.iter().find(|r| r["pad"] == "2"))
            .cloned()
            .ok_or_else(missing)?;
        assert_eq!(second["castellated"], false, "other pads stay plain");
        s.execute(
            "footprint.set_castellated",
            json!({"id":fp,"pad":"1","on":false}),
            None,
        )?;
        assert!(
            s.inspect()["project"]["board"]["footprints"][0]["pads"][0]
                .get("castellated")
                .is_none(),
            "clearing the flag drops the field"
        );
        for bad in [
            json!({"id":999,"pad":"1","on":true}),
            json!({"id":fp,"pad":"9","on":true}),
            json!({"id":fp,"pad":"","on":true}),
            json!({"id":fp,"pad":"1"}),
            json!({"id":fp,"pad":"1","on":true,"nope":1}),
        ] {
            assert!(
                s.execute("footprint.set_castellated", bad, None).is_err(),
                "bad castellated requests are refused"
            );
        }
        s.execute(
            "footprint.set_castellated",
            json!({"id":fp,"pad":"1","on":true}),
            None,
        )?;
        s.undo()?;
        assert!(
            s.inspect()["project"]["board"]["footprints"][0]["pads"][0]
                .get("castellated")
                .is_none(),
            "the flag undoes"
        );
        Ok(())
    }
    #[test]
    fn barcode_place_draws_bars_in_one_step_and_refuses_bad_text() -> Result<(), Error> {
        let mut s = Session::default();
        let out = s.execute(
            "barcode.place",
            json!({"text":"pcb-1","position":[5,5]}),
            None,
        )?["result"]
            .clone();
        assert_eq!(out["count"], 35, "each symbol contributes five bars");
        let graphics = s.inspect()["project"]["graphics"].clone();
        assert_eq!(graphics.as_array().map_or(0, |g| g.len()), 35);
        assert_eq!(graphics[0]["layer"], "board");
        assert_eq!(graphics[0]["shape"], "line");
        assert_eq!(graphics[0]["points"][0]["x"], 5.125, "bars are centred");
        assert_eq!(graphics[0]["width"], 0.25, "a narrow bar is one module");
        assert_eq!(graphics[0]["points"][1]["y"], 8.0, "default height is 3 mm");
        s.undo()?;
        assert!(
            s.inspect()["project"]["graphics"].get(0).is_none(),
            "the whole barcode undoes in one step"
        );
        for bad in [
            json!({"text":"AI-9#","position":[5,5]}),
            json!({"text":"","position":[5,5]}),
            json!({"text":"  ","position":[5,5]}),
            json!({"text":"A".repeat(49),"position":[5,5]}),
            json!({"text":"AB","position":[5,5],"height":0.4}),
            json!({"text":"AB","position":[5,5],"module":0.04}),
            json!({"text":"AB","position":[5,5],"layer":"copper"}),
            json!({"text":"AB"}),
        ] {
            assert!(
                s.execute("barcode.place", bad, None).is_err(),
                "bad barcodes are refused"
            );
        }
        let out2 = s.execute(
            "barcode.place",
            json!({"text":"OK","position":[0,0],"layer":"schematic","module":0.5,"height":5}),
            None,
        )?["result"]
            .clone();
        assert_eq!(out2["count"], 20, "four symbols times five bars");
        let g = s.inspect()["project"]["graphics"].clone();
        assert_eq!(g[0]["layer"], "schematic");
        assert_eq!(g[0]["width"], 0.5, "the module sets the narrow bar width");
        Ok(())
    }
    #[test]
    fn net_pair_rules_tighten_clearance_report_and_undo() -> Result<(), Error> {
        let mut s = Session::default();
        let a = s.execute("net.create", json!({"name":"A"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let b = s.execute("net.create", json!({"name":"B"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        s.execute(
            "track.add",
            json!({"net":a,"start":[10,10],"end":[30,10],"width":0.2}),
            None,
        )?;
        s.execute(
            "track.add",
            json!({"net":b,"start":[10,10.5],"end":[30,10.5],"width":0.2}),
            None,
        )?;
        let clear = |s: &Session| {
            s.checks()["drc"]
                .as_array()
                .is_some_and(|f| f.iter().any(|x| x["code"] == "clearance"))
        };
        assert!(!clear(&s), "0.3 mm of copper gap passes the 0.2 mm default");
        s.execute(
            "board.set_pair_rule",
            json!({"net_a":a,"net_b":b,"clearance":0.5}),
            None,
        )?;
        assert!(clear(&s), "the pair rule demands a wider gap");
        s.execute(
            "board.set_pair_rule",
            json!({"net_a":b,"net_b":a,"clearance":0.25}),
            None,
        )?;
        assert!(!clear(&s), "a relaxed replacement rule passes again");
        let rules = s.inspect()["project"]["board"]["pair_rules"].clone();
        assert_eq!(
            rules.as_array().map_or(0, |r| r.len()),
            1,
            "the same pair replaces its rule"
        );
        assert_eq!(rules[0]["a"], a.min(b), "net ids are stored ordered");
        assert_eq!(rules[0]["clearance"], 0.25);
        for bad in [
            json!({"net_a":a,"net_b":a,"clearance":0.5}),
            json!({"net_a":a,"net_b":999,"clearance":0.5}),
            json!({"net_a":a,"net_b":b,"clearance":0.0}),
            json!({"net_a":a,"net_b":b,"clearance":101.0}),
            json!({"net_a":a,"net_b":b}),
            json!({"net_a":a,"net_b":b,"clearance":0.5,"nope":1}),
        ] {
            assert!(
                s.execute("board.set_pair_rule", bad, None).is_err(),
                "bad pair rules are refused"
            );
        }
        assert!(
            s.execute(
                "board.remove_pair_rule",
                json!({"net_a":a,"net_b":b,"clearance":0.5}),
                None
            )
            .is_err(),
            "board.remove_pair_rule takes two nets only"
        );
        assert!(
            s.execute("board.remove_pair_rule", json!({"net_a":9,"net_b":9}), None)
                .is_err(),
            "removing an absent rule is refused"
        );
        let removed =
            s.execute("board.remove_pair_rule", json!({"net_a":b,"net_b":a}), None)?["result"]
                .clone();
        assert_eq!(removed["count"], 0, "the removal reports what remains");
        assert!(
            s.inspect()["project"]["board"].get("pair_rules").is_none(),
            "the rule is gone"
        );
        s.undo()?;
        assert_eq!(
            s.inspect()["project"]["board"]["pair_rules"][0]["clearance"],
            0.25,
            "the removal undoes"
        );
        s.undo()?;
        assert_eq!(
            s.inspect()["project"]["board"]["pair_rules"][0]["clearance"],
            0.5,
            "the replacement undoes"
        );
        assert!(clear(&s), "and the finding returns with it");
        Ok(())
    }
    #[test]
    fn rule_areas_tighten_clearance_only_inside_their_rectangle() -> Result<(), Error> {
        let mut s = Session::default();
        let a = s.execute("net.create", json!({"name":"A"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let b = s.execute("net.create", json!({"name":"B"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let c = s.execute("net.create", json!({"name":"C"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        s.execute(
            "track.add",
            json!({"net":a,"start":[10,10],"end":[30,10],"width":0.2}),
            None,
        )?;
        s.execute(
            "track.add",
            json!({"net":b,"start":[10,10.5],"end":[30,10.5],"width":0.2}),
            None,
        )?;
        let clear = |s: &Session| {
            s.checks()["drc"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .find(|x| x["code"] == "clearance")
        };
        assert!(
            clear(&s).is_none(),
            "0.3 mm of copper gap passes the 0.2 mm default"
        );
        let cover = |s: &mut Session| -> Result<u64, Error> {
            let out = s.execute(
                "rules.area_add",
                json!({"clearance":0.5,"x":5,"y":9,"width":30,"height":3,"layer":"F.Cu","net":a}),
                None,
            )?;
            let id = out["result"]["id"].as_u64().ok_or_else(missing)?;
            Ok(id)
        };
        let wide = s.execute(
            "rules.area_add",
            json!({"name":"sensor","clearance":0.5,"x":5,"y":9,"width":30,"height":3}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let finding = clear(&s).ok_or_else(missing)?;
        assert!(
            finding["message"]
                .as_str()
                .is_some_and(|m| m.contains("0.500 mm required")),
            "the area's gap shows in the finding"
        );
        let stored = s.inspect()["project"]["board"]["rule_areas"].clone();
        assert_eq!(stored.as_array().map_or(0, |r| r.len()), 1);
        assert_eq!(stored[0]["name"], "sensor");
        assert_eq!(stored[0]["clearance"], 0.5);
        assert_eq!(stored[0]["region"]["width"], 30.0);
        assert!(
            stored[0].get("net").is_none(),
            "an open area covers every net"
        );
        s.execute("rules.area_remove", json!({"id":wide}), None)?;
        assert!(
            clear(&s).is_none(),
            "removing the area relaxes the board again"
        );
        let miss = s.execute(
            "rules.area_add",
            json!({"net":c,"clearance":0.5,"x":5,"y":9,"width":30,"height":3}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        assert!(
            clear(&s).is_none(),
            "an area narrowed to an unrelated net does not apply"
        );
        s.execute("rules.area_remove", json!({"id":miss}), None)?;
        let one_side = cover(&mut s)?;
        assert!(
            clear(&s).is_some(),
            "the area applies when either side of the pair matches its net"
        );
        s.execute("rules.area_remove", json!({"id":one_side}), None)?;
        let wrong_layer = s.execute(
            "rules.area_add",
            json!({"layer":"B.Cu","clearance":0.5,"x":5,"y":9,"width":30,"height":3}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        assert!(
            clear(&s).is_none(),
            "an area on the other copper layer does not apply"
        );
        s.execute("rules.area_remove", json!({"id":wrong_layer}), None)?;
        let elsewhere = s.execute(
            "rules.area_add",
            json!({"clearance":0.5,"x":40,"y":9,"width":20,"height":3}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        assert!(
            clear(&s).is_none(),
            "copper outside the rectangle keeps the default gap"
        );
        s.execute("rules.area_remove", json!({"id":elsewhere}), None)?;
        for bad in [
            json!({"clearance":0.0,"x":5,"y":9,"width":30,"height":3}),
            json!({"clearance":101.0,"x":5,"y":9,"width":30,"height":3}),
            json!({"clearance":0.5,"x":5,"y":9,"width":0,"height":3}),
            json!({"clearance":0.5,"x":980,"y":9,"width":30,"height":3}),
            json!({"clearance":0.5,"x":5,"y":9,"width":30,"height":3,"net":999}),
            json!({"clearance":0.5,"x":5,"y":9,"width":30,"height":3,"layer":"C.Cu"}),
            json!({"clearance":0.5,"x":5,"y":9,"width":30,"height":3,"name":""}),
            json!({"clearance":0.5,"x":5,"y":9,"width":30,"height":3,"nope":1}),
            json!({"x":5,"y":9,"width":30,"height":3}),
        ] {
            assert!(
                s.execute("rules.area_add", bad, None).is_err(),
                "bad rule areas are refused"
            );
        }
        assert!(
            s.execute("rules.area_remove", json!({"id":9999}), None)
                .is_err(),
            "removing an absent area is refused"
        );
        s.execute(
            "rules.area_add",
            json!({"net":c,"clearance":0.5,"x":5,"y":9,"width":30,"height":3}),
            None,
        )?;
        assert!(
            s.execute("object.delete", json!({"id":c}), None).is_ok(),
            "a net with no copper on it can go"
        );
        assert!(
            s.inspect()["project"]["board"].get("rule_areas").is_none(),
            "deleting a net drops the areas scoped to it"
        );
        s.execute(
            "rules.area_add",
            json!({"clearance":0.4,"x":5,"y":9,"width":30,"height":3}),
            None,
        )?;
        s.undo()?;
        assert!(
            s.inspect()["project"]["board"].get("rule_areas").is_none(),
            "adding an area undoes"
        );
        Ok(())
    }
    #[test]
    fn library_footprint_model_references_read_edit_and_export() -> Result<(), Error> {
        let mut s = Session::default();
        let definition = library::footprint(std::path::Path::new(&library_fixture(
            "R_0805_2012Metric.kicad_mod",
        )))?;
        s.execute(
            "library.footprint.upsert",
            json!({"definition": definition}),
            None,
        )?;
        let stored = s.inspect()["project"]["library"]["footprints"][0]["model"].clone();
        assert!(
            stored["path"]
                .as_str()
                .is_some_and(|p| p.ends_with("R_0805_2012Metric.step")),
            "reading a footprint records its 3D model reference"
        );
        let r = s.execute(
            "component.add",
            json!({"kind":"resistor","position":[10,10],"reference":"R1","value":"10k"}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        s.execute("board.sync", json!({}), None)?;
        s.execute(
            "footprint.assign_custom",
            json!({"id":r,"name":"R_0805_2012Metric"}),
            None,
        )?;
        let board = pcbcraft_io::kicad_board(&s.project)?;
        assert!(
            board.contains("R_0805_2012Metric.step"),
            "the embedded model travels into the board export"
        );
        s.execute(
            "library.footprint.set_model",
            json!({"name":"R_0805_2012Metric","path":"parts/resistor.wrl","offset":[0,0,0.5],"rotate":[0,0,90]}),
            None,
        )?;
        let m = s.inspect()["project"]["library"]["footprints"][0]["model"].clone();
        assert_eq!(m["path"], "parts/resistor.wrl");
        assert_eq!(m["offset"][2], 0.5);
        assert_eq!(
            m["scale"],
            json!([1.0, 1.0, 1.0]),
            "an absent scale is unit"
        );
        assert_eq!(m["rotate"][2], 90.0);
        let board_now = pcbcraft_io::kicad_board(&s.project)?;
        assert!(
            board_now.contains("R_0805_2012Metric.step"),
            "placed instances keep their embedded copy until reassigned"
        );
        s.execute(
            "footprint.assign_custom",
            json!({"id":r,"name":"R_0805_2012Metric"}),
            None,
        )?;
        let board_new = pcbcraft_io::kicad_board(&s.project)?;
        assert!(
            board_new.contains("(model \"parts/resistor.wrl\""),
            "reassigning carries the new model into the export"
        );
        for bad in [
            json!({"name":"R_0805_2012Metric","path":""}),
            json!({"name":"R_0805_2012Metric","path":"p".repeat(1025)}),
            json!({"name":"R_0805_2012Metric","path":"a.wrl","offset":[0,0]}),
            json!({"name":"R_0805_2012Metric","path":"a.wrl","scale":[1,0,1]}),
            json!({"name":"nope","path":"a.wrl"}),
            json!({"name":"R_0805_2012Metric","path":"a.wrl","nope":1}),
            json!({"path":"a.wrl"}),
        ] {
            assert!(
                s.execute("library.footprint.set_model", bad, None).is_err(),
                "bad model requests are refused"
            );
        }
        s.execute(
            "library.footprint.clear_model",
            json!({"name":"R_0805_2012Metric"}),
            None,
        )?;
        assert!(
            s.inspect()["project"]["library"]["footprints"][0]
                .get("model")
                .is_none(),
            "clearing drops the model field"
        );
        assert!(
            s.execute(
                "library.footprint.clear_model",
                json!({"name":"R_0805_2012Metric"}),
                None
            )
            .is_err(),
            "clearing an absent model is refused"
        );
        s.execute(
            "library.footprint.set_model",
            json!({"name":"R_0805_2012Metric","path":"again.wrl"}),
            None,
        )?;
        s.undo()?;
        assert!(
            s.inspect()["project"]["library"]["footprints"][0]
                .get("model")
                .is_none(),
            "setting a model undoes back to the cleared state"
        );
        Ok(())
    }
    #[test]
    fn specctra_session_files_import_net_assignments() -> Result<(), Error> {
        let mut s = Session::default();
        for (reference, x) in [("R1", 10.0), ("R2", 20.0)] {
            s.execute(
                "component.add",
                json!({"kind":"resistor","position":[x,10],"reference":reference,"value":"10k"}),
                None,
            )?;
        }
        let path = std::env::temp_dir().join(format!(
            "pcbcraft-session-import-{}.ses",
            std::process::id()
        ));
        std::fs::write(
            &path,
            "(session (net_map_out (net (number 0) (name UNCONNECTED)) (net (number 1) (name GND) (pins (pin R1-2) (pin R2-2))) (net (number 2) (name VCC) (pins (pin R1-1) (pin R2-1)))))",
        )
        .map_err(|e| Error::Invalid(e.to_string()))?;
        let params = json!({"path": path.to_string_lossy()});
        let out = s.execute("netlist.import_session", params.clone(), None)?["result"].clone();
        assert_eq!(out["nets"], 2, "the pin-less net is not imported");
        assert_eq!(out["created"], 2);
        assert_eq!(out["pins"], 4);
        let nets = s.inspect()["project"]["nets"].clone();
        assert_eq!(nets.as_array().map_or(0, |n| n.len()), 2);
        assert_eq!(nets[0]["name"], "GND");
        let first = &s.inspect()["project"]["components"][0]["pins"];
        assert!(
            first[0]["net"].is_number() && first[1]["net"].is_number(),
            "every imported pin lands on a net"
        );
        let again = s.execute("netlist.import_session", params.clone(), None)?["result"].clone();
        assert_eq!(
            again["created"], 0,
            "re-importing the same session reuses nets"
        );
        s.undo()?;
        assert_eq!(
            s.inspect()["project"]["nets"]
                .as_array()
                .map_or(0, |n| n.len()),
            2,
            "the no-op re-import undoes to the imported state"
        );
        s.undo()?;
        assert!(
            s.inspect()["project"]["nets"].get(0).is_none(),
            "the import itself undoes in one step"
        );
        for bad in [
            json!({"path": ""}),
            json!({"path": "/nonexistent/dir/none.ses"}),
            json!({"path": path.to_string_lossy(), "nope": 1}),
            json!({}),
        ] {
            assert!(
                s.execute("netlist.import_session", bad, None).is_err(),
                "bad session requests are refused"
            );
        }
        std::fs::remove_file(&path).map_err(|e| Error::Invalid(e.to_string()))?;
        Ok(())
    }
    #[test]
    fn footprint_local_rules_tighten_clearance_and_read_edit_export() -> Result<(), Error> {
        let mut s = Session::default();
        let definition = library::footprint(std::path::Path::new(&library_fixture(
            "R_0805_2012Metric.kicad_mod",
        )))?;
        s.execute(
            "library.footprint.upsert",
            json!({"definition": definition}),
            None,
        )?;
        let r = s.execute(
            "component.add",
            json!({"kind":"resistor","position":[10,10],"reference":"R1","value":"10k"}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        s.execute("board.sync", json!({}), None)?;
        s.execute(
            "footprint.assign_custom",
            json!({"id":r,"name":"R_0805_2012Metric"}),
            None,
        )?;
        let a = s.execute("net.create", json!({"name":"SIG"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let b = s.execute("net.create", json!({"name":"GND"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        s.execute(
            "net.connect",
            json!({"net":a,"pins":[{"component":r,"pin":"1"}]}),
            None,
        )?;
        s.execute(
            "net.connect",
            json!({"net":b,"pins":[{"component":r,"pin":"2"}]}),
            None,
        )?;
        s.execute(
            "track.add",
            json!({"net":b,"start":[8.175,9],"end":[8.175,11],"width":0.2}),
            None,
        )?;
        let bound = |s: &Session| {
            s.checks()["drc"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .find(|x| x["code"] == "pad_clearance_bound")
        };
        assert!(
            bound(&s).is_none(),
            "0.3 mm of copper gap passes the 0.2 mm default"
        );
        s.execute(
            "library.footprint.set_rules",
            json!({"name":"R_0805_2012Metric","clearance":0.5,"paste_margin":-0.02}),
            None,
        )?;
        let stored = s.inspect()["project"]["library"]["footprints"][0]["rules"].clone();
        assert_eq!(stored["clearance"], 0.5);
        assert_eq!(stored["paste_margin"], -0.02);
        assert!(
            stored.get("mask_margin").is_none(),
            "an unset margin stays absent"
        );
        s.execute(
            "footprint.assign_custom",
            json!({"id":r,"name":"R_0805_2012Metric"}),
            None,
        )?;
        let board = pcbcraft_io::kicad_board(&s.project)?;
        assert!(
            board.contains("(clearance 0.5)") && board.contains("(solder_paste_margin -0.02)"),
            "overrides travel into the board export with the embedded copy"
        );
        let hit = bound(&s).ok_or_else(missing)?;
        assert!(
            hit["message"]
                .as_str()
                .is_some_and(|m| m.contains("0.500 mm required")),
            "the footprint's own clearance tightens the check"
        );
        s.execute(
            "library.footprint.clear_rules",
            json!({"name":"R_0805_2012Metric"}),
            None,
        )?;
        assert!(
            s.inspect()["project"]["library"]["footprints"][0]
                .get("rules")
                .is_none(),
            "clearing removes the overrides"
        );
        s.undo()?;
        assert_eq!(
            s.inspect()["project"]["library"]["footprints"][0]["rules"]["clearance"],
            0.5,
            "clearing undoes back to the overrides"
        );
        for bad in [
            json!({"name": "R_0805_2012Metric"}),
            json!({"name": "R_0805_2012Metric", "clearance": 150.0}),
            json!({"name": "nope", "clearance": 0.3}),
            json!({"name": "R_0805_2012Metric", "clearance": 0.3, "nope": 1}),
            json!({}),
        ] {
            assert!(
                s.execute("library.footprint.set_rules", bad, None).is_err(),
                "bad rule requests are refused"
            );
        }
        assert!(
            s.execute(
                "library.footprint.clear_rules",
                json!({"name":"R_0805_2012Metric"}),
                None
            )
            .is_ok(),
            "the overrides are still there after the refusals"
        );
        assert!(
            s.execute(
                "library.footprint.clear_rules",
                json!({"name":"R_0805_2012Metric"}),
                None
            )
            .is_err(),
            "clearing an absent override is refused"
        );
        Ok(())
    }
    #[test]
    fn variants_are_named_flagged_by_dnp_sets_removed_and_undone() -> Result<(), Error> {
        let mut s = Session::default();
        let r1 = s.execute(
            "component.add",
            json!({"kind":"resistor","position":[0,0],"reference":"R1","value":"10k"}),
            None,
        )?["result"]["id"]
            .clone();
        let r2 = s.execute(
            "component.add",
            json!({"kind":"resistor","position":[10,0],"reference":"R2","value":"22k"}),
            None,
        )?["result"]["id"]
            .clone();
        let created = s.execute(
            "variant.create",
            json!({"name":"No LED build","note":"factory option","dnp":[r2]}),
            None,
        )?["result"]
            .clone();
        assert_eq!(created["count"], 1, "the first variant is defined");
        s.execute("variant.create", json!({"name":"Budget"}), None)?;
        let variants = s.inspect()["project"]["variants"].clone();
        assert_eq!(
            variants[0]["name"], "Budget",
            "variants stay ordered by name"
        );
        assert_eq!(variants[1]["dnp"][0], r2);
        assert_eq!(variants[1]["note"], "factory option");
        let added = s.execute(
            "variant.set_dnp",
            json!({"name":"No LED build","ids":[r1]}),
            None,
        )?["result"]
            .clone();
        assert_eq!(added["dnp"], 2, "ids join the do-not-populate set");
        let dropped = s.execute(
            "variant.set_dnp",
            json!({"name":"No LED build","ids":[r2],"dnp":false}),
            None,
        )?["result"]
            .clone();
        assert_eq!(dropped["dnp"], 1, "ids can leave the set");
        let cleared = s.execute(
            "variant.set_dnp",
            json!({"name":"No LED build","ids":[]}),
            None,
        )?["result"]
            .clone();
        assert_eq!(cleared["dnp"], 0, "an empty list clears the set");
        for bad in [
            json!({"name":"  "}),
            json!({"name":"Budget"}),
            json!({"name":"x","dnp":[999]}),
            json!({"name":"x".repeat(65)}),
            json!({"name":"y","note":"x".repeat(257)}),
            json!({"name":"y","dnp": vec![1u64; 513]}),
        ] {
            assert!(
                s.execute("variant.create", bad, None).is_err(),
                "bad variant definitions are refused"
            );
        }
        for bad in [
            json!({"name":"absent","ids":[]}),
            json!({"name":"Budget","ids":[999]}),
            json!({"name":"Budget","ids":[1],"nope":true}),
            json!({"name":"Budget"}),
        ] {
            assert!(
                s.execute("variant.set_dnp", bad, None).is_err(),
                "bad do-not-populate updates are refused"
            );
        }
        assert!(
            s.execute("variant.remove", json!({"name":"absent"}), None)
                .is_err(),
            "removing an absent variant is refused"
        );
        let removed =
            s.execute("variant.remove", json!({"name":"Budget"}), None)?["result"].clone();
        assert_eq!(removed["count"], 1, "the removal reports what remains");
        s.undo()?;
        assert_eq!(
            s.inspect()["project"]["variants"][0]["name"],
            "Budget",
            "the removal undoes"
        );
        s.undo()?;
        assert_eq!(
            s.inspect()["project"]["variants"][1]["dnp"]
                .as_array()
                .map_or(0, |d| d.len()),
            1,
            "the clear undoes"
        );
        s.undo()?;
        s.undo()?;
        assert_eq!(s.inspect()["project"]["variants"][1]["dnp"][0], r2);
        s.undo()?;
        assert_eq!(
            s.inspect()["project"]["variants"]
                .as_array()
                .map_or(0, |v| v.len()),
            1,
            "creating the second variant undoes too"
        );
        assert_eq!(
            s.inspect()["project"]["variants"][0]["name"],
            "No LED build"
        );
        for i in 0..15 {
            s.execute("variant.create", json!({"name":format!("v{i:02}")}), None)?;
        }
        assert!(
            s.execute("variant.create", json!({"name":"v15"}), None)
                .is_err(),
            "a seventeenth variant is refused"
        );
        Ok(())
    }
    #[test]
    fn variant_footprint_swaps_reach_a_variant_bill_of_materials() -> Result<(), Error> {
        let mut s = Session::default();
        let r1 = s
            .execute(
                "component.add",
                json!({"kind":"resistor","reference":"R1","value":"10k","position":{"x":10.0,"y":10.0}}),
                None,
            )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let r2 = s
            .execute(
                "component.add",
                json!({"kind":"resistor","reference":"R2","value":"22k","position":{"x":20.0,"y":10.0}}),
                None,
            )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        s.execute("variant.create", json!({"name":"Slim"}), None)?;
        let swap = s.execute(
            "variant.set_footprint",
            json!({"name":"Slim","component":r1,"footprint":"R_0603"}),
            None,
        )?["result"]
            .clone();
        assert_eq!(swap["footprint"], "R_0603", "the swap is echoed");
        let plain = s.execute("bom.export", json!({}), None)?["result"].clone();
        assert_eq!(plain["components"], 2, "the plain bill counts both parts");
        assert_eq!(plain["items"][0]["footprint"], "starter pads");
        let slim = s.execute("bom.export", json!({"variant":"Slim"}), None)?["result"].clone();
        assert_eq!(
            slim["variant"], "Slim",
            "the bill reports the build it prints"
        );
        assert_eq!(slim["components"], 2);
        assert_eq!(
            slim["items"]
                .as_array()
                .ok_or_else(missing)?
                .iter()
                .find(|item| item["references"][0] == "R1")
                .ok_or_else(missing)?["footprint"],
            "R_0603",
            "the variant bill prints the swap"
        );
        s.execute("variant.set_dnp", json!({"name":"Slim","ids":[r2]}), None)?;
        let slim = s.execute("bom.export", json!({"variant":"Slim"}), None)?["result"].clone();
        assert_eq!(slim["components"], 1, "the unpopulated part is skipped");
        assert_eq!(slim["groups"], 1);
        let plain = s.execute("bom.export", json!({}), None)?["result"].clone();
        assert_eq!(plain["components"], 2, "the plain bill is untouched");
        s.execute(
            "variant.set_footprint",
            json!({"name":"Slim","component":r1,"footprint":"R_0402"}),
            None,
        )?;
        let swaps = s.inspect()["project"]["variants"][0]["footprints"]
            .as_array()
            .ok_or_else(missing)?
            .clone();
        assert_eq!(swaps.len(), 1, "re-swapping one part replaces its entry");
        assert_eq!(swaps[0]["footprint"], "R_0402");
        let cleared = s.execute(
            "variant.clear_footprint",
            json!({"name":"Slim","component":r1}),
            None,
        )?["result"]
            .clone();
        assert_eq!(cleared["swaps"], 0);
        s.undo()?;
        assert_eq!(
            s.inspect()["project"]["variants"][0]["footprints"]
                .as_array()
                .ok_or_else(missing)?
                .len(),
            1,
            "undoing the clear restores the swap"
        );
        s.redo()?;
        assert!(
            s.inspect()["project"]["variants"][0]
                .get("footprints")
                .is_none(),
            "redoing the clear strips the swap again"
        );
        let slim = s.execute("bom.export", json!({"variant":"Slim"}), None)?["result"].clone();
        assert_eq!(
            slim["items"][0]["footprint"], "starter pads",
            "clearing restores the placed footprint"
        );
        for bad in [
            json!({"name":"Ghost","component":r1,"footprint":"X"}),
            json!({"name":"Slim","component":999,"footprint":"X"}),
            json!({"name":"Slim","component":r1,"footprint":"   "}),
            json!({"name":"Slim","component":r1,"footprint":"x".repeat(129)}),
        ] {
            assert!(
                s.execute("variant.set_footprint", bad, None).is_err(),
                "malformed swaps are refused"
            );
        }
        assert!(
            s.execute(
                "variant.clear_footprint",
                json!({"name":"Slim","component":r1}),
                None
            )
            .is_err(),
            "clearing an absent swap is refused"
        );
        assert!(
            s.execute(
                "variant.clear_footprint",
                json!({"name":"Slim","component":r1,"footprint":"X"}),
                None,
            )
            .is_err(),
            "the clear command rejects a stray footprint"
        );
        assert!(
            s.execute("bom.export", json!({"variant":"Ghost"}), None)
                .is_err(),
            "a bill for an unknown variant is refused"
        );
        Ok(())
    }
    #[test]
    fn items_update_edits_tracks_vias_and_labels_in_one_undoable_batch() -> Result<(), Error> {
        let mut s = Session::default();
        let net = s.execute("net.create", json!({"name":"SIG"}), None)?["result"]["id"].clone();
        let other = s.execute("net.create", json!({"name":"GND"}), None)?["result"]["id"].clone();
        let track = s.execute(
            "track.add",
            json!({"net":net,"start":[0,0],"end":[10,0]}),
            None,
        )?["result"]["id"]
            .clone();
        let via = s.execute(
            "via.place",
            json!({"net":net,"position":[5,5],"diameter":0.8,"drill":0.4}),
            None,
        )?["result"]["id"]
            .clone();
        let label = s.execute(
            "text.place",
            json!({"text":"REV A","position":[1,1],"size":1.0}),
            None,
        )?["result"]["id"]
            .clone();
        let out = s.execute(
            "items.update",
            json!({"items":[
                {"id":track,"width":0.5,"net":other},
                {"id":via,"diameter":1.0,"drill":0.5},
                {"id":label,"text":"REV B","size":2.0,"layer":"B.Cu"}
            ]}),
            None,
        )?["result"]
            .clone();
        assert_eq!(out["changed"], 3, "all three objects changed");
        let board = &s.inspect()["project"]["board"];
        assert_eq!(board["tracks"][0]["width"], 0.5);
        assert_eq!(board["tracks"][0]["net"], other, "the track moved nets");
        assert_eq!(board["vias"][0]["diameter"], 1.0);
        assert_eq!(board["vias"][0]["drill"], 0.5);
        assert_eq!(board["texts"][0]["text"], "REV B");
        assert_eq!(board["texts"][0]["size"], 2.0);
        assert_eq!(board["texts"][0]["layer"], "B.Cu");
        for bad in [
            json!({"items":[]}),
            json!({"items":[{"id":track}]}),
            json!({"items":[{"id":track,"diameter":1.0}]}),
            json!({"items":[{"id":track,"width":0.0}]}),
            json!({"items":[{"id":via,"width":0.2}]}),
            json!({"items":[{"id":via,"drill":1.2}]}),
            json!({"items":[{"id":label,"drill":0.3}]}),
            json!({"items":[{"id":label,"text":""}]}),
            json!({"items":[{"id":9999,"width":0.2}]}),
            json!({"items":[{"id":track,"nope":1}]}),
        ] {
            assert!(
                s.execute("items.update", bad, None).is_err(),
                "bad batch updates are refused"
            );
        }
        let many = json!({"items": vec![json!({"id":track,"width":0.2}); 129]});
        assert!(
            s.execute("items.update", many, None).is_err(),
            "a 129-item batch is refused"
        );
        s.undo()?;
        let board = &s.inspect()["project"]["board"];
        assert_eq!(
            board["tracks"][0]["net"], net,
            "the batch undoes in one step"
        );
        assert_eq!(board["vias"][0]["drill"], 0.4);
        assert_eq!(board["texts"][0]["text"], "REV A");
        Ok(())
    }
    #[test]
    fn machining_notes_merge_replace_and_clear_with_undo() -> Result<(), Error> {
        let mut s = Session::default();
        let first = s.execute(
            "board.set_machining",
            json!({"edge_plating":true,"drill_tolerance":0.05}),
            None,
        )?["result"]
            .clone();
        assert_eq!(first["machining"]["edge_plating"], true);
        assert_eq!(first["machining"]["drill_tolerance"], 0.05);
        assert_eq!(
            first["machining"].get("edge_tolerance"),
            None,
            "unmentioned notes stay absent"
        );
        let merged =
            s.execute("board.set_machining", json!({"edge_tolerance":0.1}), None)?["result"]
                .clone();
        assert_eq!(
            merged["machining"]["edge_plating"], true,
            "a partial update keeps the other notes"
        );
        assert_eq!(merged["machining"]["edge_tolerance"], 0.1);
        let replaced =
            s.execute("board.set_machining", json!({"drill_tolerance":0.02}), None)?["result"]
                .clone();
        assert_eq!(replaced["machining"]["drill_tolerance"], 0.02);
        assert_eq!(replaced["machining"]["edge_tolerance"], 0.1);
        for bad in [
            json!({}),
            json!({"drill_tolerance":-0.01}),
            json!({"edge_tolerance":1.1}),
            json!({"edge_plating":"yes"}),
            json!({"tolerance":0.05}),
        ] {
            assert!(
                s.execute("board.set_machining", bad, None).is_err(),
                "bad machining notes are refused"
            );
        }
        assert!(
            s.execute("board.clear_machining", json!({"edge_plating":true}), None)
                .is_err(),
            "board.clear_machining takes an empty object"
        );
        let cleared = s.execute("board.clear_machining", json!({}), None)?["result"].clone();
        assert_eq!(cleared["removed"], true, "clearing reports it held notes");
        assert!(s.inspect()["project"]["board"].get("machining").is_none());
        s.undo()?;
        assert_eq!(
            s.inspect()["project"]["board"]["machining"]["drill_tolerance"],
            0.02,
            "the clear undoes"
        );
        Ok(())
    }
    #[test]
    fn attached_files_are_named_replaced_ordered_removed_and_undone() -> Result<(), Error> {
        let mut s = Session::default();
        let first = s.execute(
            "file.attach",
            json!({"name":"notes.txt","data":"built to house style"}),
            None,
        )?["result"]
            .clone();
        assert_eq!(first["count"], 1, "the first file attaches");
        s.execute("file.attach", json!({"name":"LICENCE","data":"MIT"}), None)?;
        let replaced = s.execute(
            "file.attach",
            json!({"name":"notes.txt","data":"revised house style"}),
            None,
        )?["result"]
            .clone();
        assert_eq!(replaced["count"], 2, "a repeated name replaces contents");
        let files = s.inspect()["project"]["files"].clone();
        assert_eq!(files[0]["name"], "LICENCE");
        assert_eq!(files[1]["name"], "notes.txt", "files stay ordered by name");
        assert_eq!(files[1]["data"], "revised house style");
        for bad in [
            json!({"name":"  ","data":"x"}),
            json!({"name":"x".repeat(129),"data":"x"}),
            json!({"name":"big","data":"x".repeat(262_145)}),
            json!({"data":"x"}),
        ] {
            assert!(
                s.execute("file.attach", bad, None).is_err(),
                "bad attachments are refused"
            );
        }
        assert!(
            s.execute("file.remove", json!({"name":"absent"}), None)
                .is_err(),
            "removing an absent file is refused"
        );
        let removed =
            s.execute("file.remove", json!({"name":"notes.txt"}), None)?["result"].clone();
        assert_eq!(removed["count"], 1, "the removal reports what remains");
        assert_eq!(
            s.inspect()["project"]["files"][0]["name"],
            "LICENCE",
            "the right file went"
        );
        s.undo()?;
        assert_eq!(
            s.inspect()["project"]["files"]
                .as_array()
                .map_or(0, |f| f.len()),
            2,
            "the removal undoes"
        );
        s.undo()?;
        assert_eq!(
            s.inspect()["project"]["files"][1]["data"],
            "built to house style",
            "a replace undoes to the previous contents"
        );
        Ok(())
    }
    #[test]
    fn bom_fields_are_defined_used_by_exports_cleared_and_undone() -> Result<(), Error> {
        let mut s = Session::default();
        s.execute(
            "component.add",
            json!({"kind":"resistor","reference":"R1","value":"10k","position":{"x":10.0,"y":10.0}}),
            None,
        )?;
        let set = s.execute(
            "bom.set_fields",
            json!({"fields":[{"key":"MPN","label":"Manufacturer part"},{"key":"Rating"}]}),
            None,
        )?["result"]
            .clone();
        assert_eq!(set["count"], 2, "columns are recorded in order");
        let fields = s.inspect()["project"]["bom_fields"].clone();
        assert_eq!(fields[0]["key"], "MPN");
        assert_eq!(fields[0]["label"], "Manufacturer part");
        assert_eq!(fields[1]["label"], "", "an omitted label defaults to empty");
        s.execute(
            "component.field",
            json!({"id":1,"key":"MPN","value":"RC0603"}),
            None,
        )?;
        let bom = s.execute("bom.export", json!({}), None)?["result"].clone();
        assert_eq!(
            bom["items"][0]["fields"]["MPN"], "RC0603",
            "the recorded columns feed the export"
        );
        let explicit =
            s.execute("bom.export", json!({"fields":["Rating"]}), None)?["result"].clone();
        assert_eq!(
            explicit["items"][0]["fields"]["MPN"],
            Value::Null,
            "an explicit request overrides the recorded columns"
        );
        assert_eq!(explicit["items"][0]["fields"]["Rating"], "");
        for bad in [
            json!({"fields":[]}),
            json!({"fields":[{"key":"","label":"x"}]}),
            json!({"fields":[{"key":"A"},{"key":"A"}]}),
            json!({"fields":[{"key":"A","label":"x".repeat(65)}]}),
            json!({"fields":(1..=17).map(|i| json!({"key":format!("f{i}")})).collect::<Vec<Value>>()}),
        ] {
            assert!(
                s.execute("bom.set_fields", bad, None).is_err(),
                "bad column definitions are refused"
            );
        }
        assert!(
            s.execute("bom.clear_fields", json!({"fields":[]}), None)
                .is_err(),
            "bom.clear_fields takes an empty object"
        );
        let cleared = s.execute("bom.clear_fields", json!({}), None)?["result"].clone();
        assert_eq!(cleared["removed"], 2, "clearing reports what it dropped");
        assert!(
            s.inspect()["project"]
                .get("bom_fields")
                .is_none_or(|f| f.as_array().is_some_and(|f| f.is_empty())),
            "the project is left with no columns"
        );
        s.undo()?;
        assert_eq!(
            s.inspect()["project"]["bom_fields"]
                .as_array()
                .map_or(0, |f| f.len()),
            2,
            "the clear undoes"
        );
        Ok(())
    }
    #[test]
    fn board_bounds_cover_tracks_and_vias_per_copper_side() -> Result<(), Error> {
        let mut s = Session::default();
        s.execute("net.create", json!({"name":"SIG"}), None)?;
        let net = s.inspect()["project"]["nets"][0]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        s.execute(
            "track.add",
            json!({"net":net,"start":{"x":10.0,"y":10.0},"end":{"x":30.0,"y":12.0},"width":0.4}),
            None,
        )?;
        s.execute(
            "track.add",
            json!({"net":net,"start":{"x":5.0,"y":5.0},"end":{"x":5.0,"y":20.0},"width":0.2,"layer":"B.Cu"}),
            None,
        )?;
        s.execute(
            "via.place",
            json!({"net":net,"position":{"x":20.0,"y":14.0},"diameter":0.7}),
            None,
        )?;
        let bounds = s.execute("board.bounds", json!({}), None)?["result"].clone();
        assert_eq!(
            bounds["overall"]["points"], 6,
            "two tracks and a via, corner pairs"
        );
        assert_eq!(
            bounds["front"]["points"], 4,
            "the back track is its own box"
        );
        assert_eq!(bounds["back"]["min_x"], 4.9, "track width feeds the box");
        assert_eq!(bounds["back"]["max_y"], 20.1);
        assert_eq!(
            bounds["front"]["max_y"], 14.35,
            "the via pad diameter feeds the box"
        );
        assert_eq!(bounds["overall"]["width"], 30.2 - 4.9);
        assert_eq!(bounds["board"]["width"], 60.0);
        let mut fresh = Session::default();
        let empty = fresh.execute("board.bounds", json!({}), None)?["result"].clone();
        assert_eq!(empty["overall"]["empty"], true, "a bare board has no box");
        assert!(
            fresh
                .execute("board.bounds", json!({"layer":"F.Cu"}), None)
                .is_err(),
            "board.bounds takes no parameters"
        );
        Ok(())
    }
    #[test]
    fn items_find_lists_ids_by_kind_with_net_and_layer_filters() -> Result<(), Error> {
        let mut s = Session::default();
        s.execute("net.create", json!({"name":"SIG"}), None)?;
        s.execute("net.create", json!({"name":"GND"}), None)?;
        let sig = s.inspect()["project"]["nets"][0]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let gnd = s.inspect()["project"]["nets"][1]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        s.execute(
            "track.add",
            json!({"net":sig,"start":{"x":10.0,"y":10.0},"end":{"x":30.0,"y":10.0}}),
            None,
        )?;
        s.execute(
            "track.add",
            json!({"net":gnd,"start":{"x":10.0,"y":12.0},"end":{"x":30.0,"y":12.0},"layer":"B.Cu"}),
            None,
        )?;
        let found = s.execute("items.find", json!({"kind":"track"}), None)?["result"].clone();
        assert_eq!(found["count"], 2, "both tracks by default");
        let one =
            s.execute("items.find", json!({"kind":"track","net":sig}), None)?["result"].clone();
        assert_eq!(one["count"], 1, "the net filter narrows the list");
        assert_eq!(one["items"][0]["net"], sig);
        let back = s.execute("items.find", json!({"kind":"track","layer":"B.Cu"}), None)?["result"]
            .clone();
        assert_eq!(back["count"], 1, "the copper-side filter narrows the list");
        let none = s.execute("items.find", json!({"kind":"zone"}), None)?["result"].clone();
        assert_eq!(none["count"], 0, "a kind with nothing placed lists nothing");
        for bad in [
            json!({"kind":"widget"}),
            json!({"kind":"track","net":9999}),
            json!({"kind":"track","layer":"F.Adge"}),
            json!({"kind":"track","extra":1}),
            json!({}),
        ] {
            assert!(s.execute("items.find", bad, None).is_err());
        }
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
    fn check_findings_can_be_excluded_by_rectangle_and_restored() -> Result<(), Error> {
        let mut s = Session::default();
        let a = s.execute("net.create", json!({"name":"A"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let b = s.execute("net.create", json!({"name":"B"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        for (net, sx) in [(a, 5.0), (b, 15.1), (a, 35.0), (b, 45.1)] {
            let y = if sx < 30.0 { 10.0 } else { 30.0 };
            s.execute(
                "track.add",
                json!({"net":net,"start":{"x":sx,"y":y},"end":{"x":sx + 10.0,"y":y},"width":0.3}),
                None,
            )?;
        }
        let shorts = |s: &Session| -> usize {
            s.checks()["drc"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .iter()
                .filter(|f| f["code"] == "copper_short")
                .count()
        };
        assert_eq!(shorts(&s), 2, "two pairs of tracks touch in two places");
        s.execute(
            "rules.exclude_region",
            json!({"code":"copper_short","x":4.0,"y":5.0,"width":22.0,"height":10.0}),
            None,
        )?;
        assert_eq!(
            shorts(&s),
            1,
            "only the short inside the rectangle falls silent"
        );
        s.execute(
            "rules.exclude_region",
            json!({"code":"copper_short","x":34.0,"y":25.0,"width":22.0,"height":10.0}),
            None,
        )?;
        assert_eq!(shorts(&s), 0, "a second rectangle covers the other one");
        assert!(
            s.execute(
                "rules.include_region",
                json!({"code":"copper_short","index":2}),
                None,
            )
            .is_err()
        );
        s.execute(
            "rules.include_region",
            json!({"code":"copper_short","index":0}),
            None,
        )?;
        assert_eq!(
            shorts(&s),
            1,
            "lifting the first rectangle restores its finding"
        );
        s.execute("rules.include_region", json!({"code":"copper_short"}), None)?;
        assert_eq!(
            shorts(&s),
            2,
            "lifting every rectangle restores both findings"
        );
        assert!(s.project.rule_regions.is_empty());
        let before = s.inspect();
        for params in [
            json!({"code":"copper_short","x":4.0,"y":5.0,"width":0.0,"height":10.0}),
            json!({"code":"copper_short","x":4.0,"y":5.0,"width":-5.0,"height":10.0}),
            json!({"code":"copper_short","x":-1001.0,"y":5.0,"width":10.0,"height":10.0}),
            json!({"code":"copper_short","x":4.0,"y":5.0,"width":1e9,"height":10.0}),
            json!({"code":"copper_short","x":null,"y":5.0,"width":10.0,"height":10.0}),
            json!({"code":"","x":4.0,"y":5.0,"width":10.0,"height":10.0}),
            json!({"code":"copper_short","x":4.0,"y":5.0,"width":10.0}),
            json!({"code":"copper_short","x":4.0,"y":5.0,"width":10.0,"height":10.0,"extra":1}),
        ] {
            assert!(
                s.execute("rules.exclude_region", params.clone(), None)
                    .is_err(),
                "{params:?} refused"
            );
        }
        for params in [
            json!({"code":"nothing_here"}),
            json!({"code":"copper_short"}),
            json!({"code":"copper_short","index":0}),
            json!({"code":"copper_short","index":-1}),
            json!({"code":""}),
            json!({"code":"copper_short","extra":1}),
        ] {
            assert!(
                s.execute("rules.include_region", params.clone(), None)
                    .is_err(),
                "{params:?} refused"
            );
        }
        assert_eq!(s.inspect(), before, "rejections change nothing");
        s.execute(
            "rules.exclude_region",
            json!({"code":"copper_short","x":4.0,"y":5.0,"width":22.0,"height":10.0}),
            None,
        )?;
        assert_eq!(shorts(&s), 1);
        s.undo()?;
        assert!(
            s.project.rule_regions.is_empty(),
            "undo removes the rectangle again"
        );
        assert_eq!(shorts(&s), 2, "and both findings return");
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
        let sig = s.execute("net.create", json!({"name":"SIG"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let near = s.execute(
            "track.add",
            json!({"net":sig,"start":{"x":5.0,"y":5.1},"end":{"x":40.0,"y":5.1},"width":0.2}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let touching = |s: &Session| -> usize {
            s.checks()["drc"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .iter()
                .filter(|f| {
                    (f["code"] == json!("copper_short") || f["code"] == json!("clearance"))
                        && f["objects"]
                            .as_array()
                            .is_some_and(|o| o.contains(&json!(track)) && o.contains(&json!(near)))
                })
                .count()
        };
        assert_eq!(
            touching(&s),
            1,
            "two nets overlapping on the same layer touch"
        );
        let moved = s.execute("track.set_layer", json!({"id":near,"layer":"B.Cu"}), None)?;
        assert_eq!(moved["result"]["layer"], json!("B.Cu"));
        assert_eq!(
            s.project
                .board
                .tracks
                .iter()
                .find(|t| t.id == near)
                .map(|t| t.layer),
            Some(pcbcraft_doc::Layer::Back),
            "the segment keeps its ends and only changes layer"
        );
        assert_eq!(
            touching(&s),
            0,
            "copper on opposite layers no longer touches"
        );
        for params in [
            json!({"id":near,"layer":"C.Cu"}),
            json!({"id":near,"layer":"B.Cu"}),
            json!({"id":near,"layer":null}),
            json!({"id":near,"layer":"B.Cu","extra":1}),
            json!({"id":999,"layer":"B.Cu"}),
            json!({"id":near}),
        ] {
            assert!(s.execute("track.set_layer", params, None).is_err());
        }
        s.undo()?;
        assert_eq!(
            s.project
                .board
                .tracks
                .iter()
                .find(|t| t.id == near)
                .map(|t| t.layer),
            Some(pcbcraft_doc::Layer::Front),
            "undo brings the segment back to the front"
        );
        assert_eq!(touching(&s), 1, "and the touch reappears");
        Ok(())
    }
    #[test]
    fn poured_zones_can_switch_copper_layers() -> Result<(), Error> {
        let mut s = Session::default();
        let gnd = s.execute("net.create", json!({"name":"GND"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let sig = s.execute("net.create", json!({"name":"SIG"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let zone = s.execute(
            "zone.place",
            json!({"net":gnd,"outline":[{"x":10.0,"y":15.0},{"x":50.0,"y":15.0},{"x":50.0,"y":30.0},{"x":10.0,"y":30.0},{"x":10.0,"y":15.0}]}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        s.execute(
            "zone.cutout",
            json!({"zone":zone,"outline":[{"x":20.0,"y":16.0},{"x":30.0,"y":16.0},{"x":30.0,"y":24.0},{"x":20.0,"y":24.0},{"x":20.0,"y":16.0}]}),
            None,
        )?;
        let track = s.execute(
            "track.add",
            json!({"net":sig,"start":{"x":10.0,"y":15.0},"end":{"x":45.0,"y":15.0},"width":0.3}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let touching = |s: &Session| -> usize {
            s.checks()["drc"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .iter()
                .filter(|f| {
                    f["objects"]
                        .as_array()
                        .is_some_and(|o| o.contains(&json!(zone)) && o.contains(&json!(track)))
                })
                .count()
        };
        assert_eq!(
            touching(&s),
            1,
            "a signal track crossing a ground zone on the same layer shorts"
        );
        let out = s.execute("zone.set_layer", json!({"id":zone,"layer":"B.Cu"}), None)?;
        assert_eq!(out["result"]["layer"], json!("B.Cu"));
        assert_eq!(
            s.project.board.zones[0].outline,
            [
                Point::new(10.0, 15.0),
                Point::new(50.0, 15.0),
                Point::new(50.0, 30.0),
                Point::new(10.0, 30.0),
                Point::new(10.0, 15.0),
            ],
            "the outline does not move when the layer changes"
        );
        assert_eq!(
            s.project.board.zones[0].cutouts.len(),
            1,
            "cutouts stay attached to their zone"
        );
        assert_eq!(
            touching(&s),
            0,
            "copper on opposite sides no longer touches"
        );
        for params in [
            json!({"id":zone,"layer":"C.Cu"}),
            json!({"id":zone,"layer":"B.Cu"}),
            json!({"id":zone,"layer":null}),
            json!({"id":zone,"layer":"B.Cu","extra":1}),
            json!({"id":999,"layer":"F.Cu"}),
            json!({"id":zone}),
        ] {
            assert!(s.execute("zone.set_layer", params, None).is_err());
        }
        s.undo()?;
        assert_eq!(
            s.project.board.zones[0].layer,
            pcbcraft_doc::Layer::Front,
            "undo pours the zone back on the front"
        );
        assert_eq!(touching(&s), 1, "and the short reappears");
        Ok(())
    }
    #[test]
    fn track_ends_drag_to_shorten_lengthen_and_reaim() -> Result<(), Error> {
        let mut s = Session::default();
        let a = s.execute("net.create", json!({"name":"A"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let b = s.execute("net.create", json!({"name":"B"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let t1 = s.execute(
            "track.add",
            json!({"net":a,"layer":"F.Cu","width":0.3,"start":{"x":10.0,"y":40.0},"end":{"x":20.0,"y":40.0}}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let t2 = s.execute(
            "track.add",
            json!({"net":b,"layer":"F.Cu","width":0.3,"start":{"x":25.0,"y":40.0},"end":{"x":35.0,"y":40.0}}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        s.execute(
            "track.set_end",
            json!({"id":t1,"end":"end","position":{"x":18.0,"y":40.0}}),
            None,
        )?;
        assert_eq!(
            s.project.board.tracks[0].end,
            Point::new(18.0, 40.0),
            "the track grew two millimetres shorter"
        );
        assert_eq!(
            s.project.board.tracks[0].start,
            Point::new(10.0, 40.0),
            "the other end did not move"
        );
        s.execute(
            "track.set_end",
            json!({"id":t1,"end":"start","position":{"x":6.0,"y":42.0}}),
            None,
        )?;
        assert_eq!(
            s.project.board.tracks[0].start,
            Point::new(6.0, 42.0),
            "the far end can be dragged too"
        );
        let near = |s: &Session| -> usize {
            s.checks()["drc"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .iter()
                .filter(|f| {
                    f["objects"]
                        .as_array()
                        .is_some_and(|o| o.contains(&json!(t1)) && o.contains(&json!(t2)))
                })
                .count()
        };
        assert_eq!(near(&s), 0, "the gap between the two nets is comfortable");
        s.execute(
            "track.set_end",
            json!({"id":t1,"end":"end","position":{"x":24.9,"y":40.0}}),
            None,
        )?;
        assert_eq!(
            near(&s),
            1,
            "extending the track closes the gap until the checks complain"
        );
        s.undo()?;
        assert_eq!(near(&s), 0, "undoing the extension opens the gap again");
        let before = s.inspect();
        for params in [
            json!({"id":t1,"end":"end","position":{"x":6.0,"y":42.0}}),
            json!({"id":t1,"end":"middle","position":{"x":12.0,"y":40.0}}),
            json!({"id":t1,"end":null,"position":{"x":12.0,"y":40.0}}),
            json!({"id":t1,"position":{"x":12.0,"y":40.0}}),
            json!({"id":t1,"end":"end","position":{"x":1e9,"y":40.0}}),
            json!({"id":t1,"end":"end","position":{"x":null,"y":40.0}}),
            json!({"id":999,"end":"start","position":{"x":12.0,"y":40.0}}),
            json!({"id":t1,"end":"end","position":{"x":12.0,"y":40.0},"extra":1}),
        ] {
            assert!(
                s.execute("track.set_end", params.clone(), None).is_err(),
                "{params:?} refused"
            );
        }
        assert_eq!(s.inspect(), before, "rejections change nothing");
        s.undo()?;
        assert_eq!(
            s.project.board.tracks[0].start,
            Point::new(10.0, 40.0),
            "undo restores the original end point"
        );
        Ok(())
    }
    #[test]
    fn placed_footprint_pads_can_be_offset_and_resized() -> Result<(), Error> {
        let mut s = Session::default();
        s.execute(
            "component.add",
            json!({"kind":"resistor","position":{"x":20.0,"y":20.0}}),
            None,
        )?;
        s.execute("board.sync", json!({}), None)?;
        let footprint = s.project.board.footprints[0].id;
        let before = s.project.board.footprints[0].pads.clone();
        assert_eq!(before.len(), 2, "a synced resistor has two pads");
        let p1 = &before[0];
        let shifted = Point::new(p1.offset.x, p1.offset.y + 1.5);
        s.execute(
            "footprint.set_pad",
            json!({"id":footprint,"pad":p1.number,"offset":{"x":shifted.x,"y":shifted.y}}),
            None,
        )?;
        assert_eq!(s.project.board.footprints[0].pads[0].offset, shifted);
        assert_eq!(
            s.project.board.footprints[0].pads[1], before[1],
            "the other pad is untouched"
        );
        s.execute(
            "footprint.set_pad",
            json!({"id":footprint,"pad":"2","diameter":0.9}),
            None,
        )?;
        assert_eq!(s.project.board.footprints[0].pads[1].diameter, 0.9);
        assert!(s.project.validate().is_ok());
        let snap = s.inspect();
        for params in [
            json!({"id":footprint,"pad":p1.number}),
            json!({"id":footprint,"pad":"9","offset":{"x":0.0,"y":0.0}}),
            json!({"id":9999,"pad":"1","diameter":0.5}),
            json!({"id":footprint,"pad":"1","diameter":0.0}),
            json!({"id":footprint,"pad":"1","diameter":1e9}),
            json!({"id":footprint,"pad":"1","offset":{"x":null,"y":0.0}}),
            json!({"id":footprint,"pad":"","diameter":0.5}),
            json!({"id":footprint,"pad":"2","diameter":0.9}),
            json!({"id":footprint,"pad":"1","diameter":0.5,"extra":1}),
        ] {
            assert!(
                s.execute("footprint.set_pad", params.clone(), None)
                    .is_err(),
                "{params:?} refused"
            );
        }
        assert_eq!(s.inspect(), snap, "rejections change nothing");
        s.execute(
            "footprint.set_pad",
            json!({"id":footprint,"pad":p1.number,"offset":{"x":0.0,"y":0.0}}),
            None,
        )?;
        s.undo()?;
        assert_eq!(
            s.project.board.footprints[0].pads[0].offset, shifted,
            "undo puts the pad back"
        );
        Ok(())
    }
    #[test]
    fn placed_board_objects_slide_by_a_distance_with_object_nudge() -> Result<(), Error> {
        let mut s = Session::default();
        let net = s.execute("net.create", json!({"name":"N"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let track = s.execute(
            "track.add",
            json!({"net":net,"start":{"x":10.0,"y":10.0},"end":{"x":20.0,"y":10.0},"width":0.2}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let via = s.execute(
            "via.place",
            json!({"net":net,"position":{"x":25.0,"y":15.0},"diameter":0.8}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let zone = s.execute(
            "zone.place",
            json!({"net":net,"outline":[{"x":40.0,"y":10.0},{"x":50.0,"y":10.0},{"x":50.0,"y":20.0},{"x":40.0,"y":20.0},{"x":40.0,"y":10.0}]}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        s.execute(
            "zone.cutout",
            json!({"zone":zone,"outline":[{"x":44.0,"y":14.0},{"x":46.0,"y":14.0},{"x":46.0,"y":16.0},{"x":44.0,"y":16.0},{"x":44.0,"y":14.0}]}),
            None,
        )?;
        let text = s.execute(
            "text.place",
            json!({"text":"TOP","position":{"x":5.0,"y":5.0}}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        s.execute("object.nudge", json!({"id":track,"dx":1.5,"dy":2.0}), None)?;
        assert_eq!(s.project.board.tracks[0].start, Point::new(11.5, 12.0));
        assert_eq!(s.project.board.tracks[0].end, Point::new(21.5, 12.0));
        s.execute("object.nudge", json!({"id":via,"dx":0.0,"dy":-4.0}), None)?;
        assert_eq!(s.project.board.vias[0].position, Point::new(25.0, 11.0));
        s.execute("object.nudge", json!({"id":zone,"dx":-5.0,"dy":0.0}), None)?;
        assert_eq!(s.project.board.zones[0].outline[0], Point::new(35.0, 10.0));
        assert_eq!(
            s.project.board.zones[0].cutouts[0][0],
            Point::new(39.0, 14.0),
            "the cutout travels with its zone"
        );
        s.execute("object.nudge", json!({"id":text,"dx":2.0,"dy":2.0}), None)?;
        assert_eq!(s.project.board.texts[0].position, Point::new(7.0, 7.0));
        assert!(s.project.validate().is_ok());
        let before = s.inspect();
        for params in [
            json!({"id":track,"dx":0.0,"dy":0.0}),
            json!({"id":track,"dx":1.0}),
            json!({"id":9999,"dx":1.0,"dy":1.0}),
            json!({"id":track,"dx":1e9,"dy":0.0}),
            json!({"id":track,"dx":1001.0,"dy":0.0}),
            json!({"id":track,"dx":null,"dy":0.0}),
            json!({"id":track,"dx":1.0,"dy":1.0,"extra":2}),
            json!({"id":net,"dx":1.0,"dy":1.0}),
        ] {
            assert!(
                s.execute("object.nudge", params.clone(), None).is_err(),
                "{params:?} refused"
            );
        }
        assert_eq!(s.inspect(), before, "rejections change nothing");
        assert_eq!(
            s.project.board.zones[0].outline[0],
            Point::new(35.0, 10.0),
            "a refused zone nudge leaves every vertex alone"
        );
        s.execute(
            "object.nudge",
            json!({"id":track,"dx":-1.5,"dy":-2.0}),
            None,
        )?;
        s.undo()?;
        assert_eq!(
            s.project.board.tracks[0].start,
            Point::new(11.5, 12.0),
            "undo puts the nudge back"
        );
        Ok(())
    }
    #[test]
    fn zone_outlines_and_cutouts_are_editable_vertex_by_vertex() -> Result<(), Error> {
        let mut s = Session::default();
        let gnd = s.execute("net.create", json!({"name":"GND"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let zone = s.execute(
            "zone.place",
            json!({"net":gnd,"outline":[{"x":10.0,"y":10.0},{"x":50.0,"y":10.0},{"x":50.0,"y":30.0},{"x":10.0,"y":30.0},{"x":10.0,"y":10.0}]}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        s.execute(
            "zone.cutout",
            json!({"zone":zone,"outline":[{"x":20.0,"y":15.0},{"x":30.0,"y":15.0},{"x":30.0,"y":25.0},{"x":20.0,"y":25.0},{"x":20.0,"y":15.0}]}),
            None,
        )?;
        s.execute(
            "zone.set_point",
            json!({"id":zone,"index":2,"position":{"x":45.0,"y":28.0}}),
            None,
        )?;
        assert_eq!(
            s.project.board.zones[0].outline[2],
            Point::new(45.0, 28.0),
            "one corner moves"
        );
        assert_eq!(
            s.project.board.zones[0].outline[1],
            Point::new(50.0, 10.0),
            "the neighbours stay put"
        );
        s.execute(
            "zone.set_point",
            json!({"id":zone,"index":0,"position":{"x":12.0,"y":12.0}}),
            None,
        )?;
        assert_eq!(
            s.project.board.zones[0].outline.first(),
            s.project.board.zones[0].outline.last(),
            "moving the opening vertex moves the closing vertex with it"
        );
        s.execute(
            "zone.set_point",
            json!({"id":zone,"cutout":0,"index":1,"position":{"x":25.0,"y":15.0}}),
            None,
        )?;
        assert_eq!(
            s.project.board.zones[0].cutouts[0][1],
            Point::new(25.0, 15.0),
            "cutout vertices move through the same command"
        );
        let sig = s.execute("net.create", json!({"name":"SIG"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        s.execute(
            "track.add",
            json!({"net":sig,"layer":"F.Cu","width":0.3,"start":{"x":5.0,"y":12.0},"end":{"x":60.0,"y":12.0}}),
            None,
        )?;
        let touching = |s: &Session| -> usize {
            s.checks()["drc"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .iter()
                .filter(|f| {
                    f["objects"]
                        .as_array()
                        .is_some_and(|o| o.contains(&json!(zone)))
                })
                .count()
        };
        assert_eq!(
            touching(&s),
            1,
            "the track runs along the zone's lower edge and touches it"
        );
        s.execute(
            "zone.set_point",
            json!({"id":zone,"index":1,"position":{"x":50.0,"y":14.0}}),
            None,
        )?;
        s.execute(
            "zone.set_point",
            json!({"id":zone,"index":0,"position":{"x":12.0,"y":14.0}}),
            None,
        )?;
        assert_eq!(
            touching(&s),
            0,
            "lifting both lower corners off the track clears the finding"
        );
        s.undo()?;
        assert_eq!(touching(&s), 1, "and undo brings it back");
        assert!(s.project.validate().is_ok());
        let before = s.inspect();
        for params in [
            json!({"id":zone,"index":5,"position":{"x":10.0,"y":10.0}}),
            json!({"id":zone,"index":-1,"position":{"x":10.0,"y":10.0}}),
            json!({"id":zone,"index":1,"position":{"x":1e9,"y":10.0}}),
            json!({"id":zone,"index":1,"position":{"x":null,"y":10.0}}),
            json!({"id":zone,"cutout":1,"index":0,"position":{"x":20.0,"y":15.0}}),
            json!({"id":zone,"cutout":-1,"index":0,"position":{"x":20.0,"y":15.0}}),
            json!({"id":999,"index":0,"position":{"x":10.0,"y":10.0}}),
            json!({"id":zone,"position":{"x":10.0,"y":10.0}}),
            json!({"id":zone,"index":1,"position":{"x":10.0,"y":10.0},"extra":1}),
        ] {
            assert!(s.execute("zone.set_point", params, None).is_err());
        }
        assert_eq!(s.inspect(), before, "rejections change nothing");
        s.execute(
            "zone.place",
            json!({"net":gnd,"outline":[{"x":10.0,"y":10.0},{"x":70.0,"y":10.0},{"x":10.0,"y":30.0},{"x":10.0,"y":10.0}]}),
            None,
        )?;
        let thin = s.project.board.zones.len() - 1;
        assert!(
            s.execute(
                "zone.set_point",
                json!({"id":thin,"index":1,"position":{"x":10.0,"y":30.0}}),
                None,
            )
            .is_err(),
            "collapsing an outline to a line is refused"
        );
        s.undo()?;
        assert_eq!(
            s.project.board.zones.len(),
            1,
            "undo removes the test triangle again"
        );
        s.undo()?;
        assert_eq!(
            s.project.board.zones[0].outline[1],
            Point::new(50.0, 10.0),
            "undo restores the lifted corner"
        );
        Ok(())
    }
    #[test]
    fn zone_outlines_gain_and_lose_one_vertex_at_a_time() -> Result<(), Error> {
        let mut s = Session::default();
        let gnd = s.execute("net.create", json!({"name":"GND"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let zone = s.execute(
            "zone.place",
            json!({"net":gnd,"outline":[{"x":10.0,"y":10.0},{"x":50.0,"y":10.0},{"x":50.0,"y":30.0},{"x":10.0,"y":30.0},{"x":10.0,"y":10.0}]}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        s.execute(
            "zone.cutout",
            json!({"zone":zone,"outline":[{"x":20.0,"y":15.0},{"x":30.0,"y":15.0},{"x":30.0,"y":25.0},{"x":20.0,"y":25.0},{"x":20.0,"y":15.0}]}),
            None,
        )?;
        s.execute(
            "zone.insert_point",
            json!({"id":zone,"index":2,"position":{"x":55.0,"y":20.0}}),
            None,
        )?;
        assert_eq!(
            s.project.board.zones[0].outline,
            [
                Point::new(10.0, 10.0),
                Point::new(50.0, 10.0),
                Point::new(55.0, 20.0),
                Point::new(50.0, 30.0),
                Point::new(10.0, 30.0),
                Point::new(10.0, 10.0),
            ],
            "a new corner slides between the two it was inserted before"
        );
        s.execute(
            "zone.insert_point",
            json!({"id":zone,"index":0,"position":{"x":8.0,"y":12.0}}),
            None,
        )?;
        assert_eq!(
            s.project.board.zones[0].outline.first(),
            s.project.board.zones[0].outline.last(),
            "inserting at the opening keeps the outline closed"
        );
        s.execute(
            "zone.insert_point",
            json!({"id":zone,"cutout":0,"index":1,"position":{"x":25.0,"y":14.0}}),
            None,
        )?;
        assert_eq!(
            s.project.board.zones[0].cutouts[0][1],
            Point::new(25.0, 14.0),
            "cutouts gain corners through the same command"
        );
        s.execute(
            "zone.remove_point",
            json!({"id":zone,"cutout":0,"index":1}),
            None,
        )?;
        assert_eq!(
            s.project.board.zones[0].cutouts[0],
            [
                Point::new(20.0, 15.0),
                Point::new(30.0, 15.0),
                Point::new(30.0, 25.0),
                Point::new(20.0, 25.0),
                Point::new(20.0, 15.0),
            ],
            "and lose them again"
        );
        s.execute("zone.remove_point", json!({"id":zone,"index":0}), None)?;
        assert_eq!(
            s.project.board.zones[0].outline.first(),
            s.project.board.zones[0].outline.last(),
            "removing the opening vertex keeps the outline closed"
        );
        s.execute("zone.remove_point", json!({"id":zone,"index":2}), None)?;
        assert_eq!(
            s.project.board.zones[0].outline,
            [
                Point::new(10.0, 10.0),
                Point::new(50.0, 10.0),
                Point::new(50.0, 30.0),
                Point::new(10.0, 30.0),
                Point::new(10.0, 10.0),
            ],
            "two removals return the square to its original corners"
        );
        assert!(s.project.validate().is_ok());
        let before = s.inspect();
        for params in [
            json!({"id":zone,"index":5,"position":{"x":20.0,"y":20.0}}),
            json!({"id":zone,"index":-1,"position":{"x":20.0,"y":20.0}}),
            json!({"id":zone,"index":1,"position":{"x":1e9,"y":20.0}}),
            json!({"id":zone,"index":1,"position":{"x":20.0,"y":null}}),
            json!({"id":zone,"cutout":1,"index":0,"position":{"x":20.0,"y":20.0}}),
            json!({"id":999,"index":0,"position":{"x":20.0,"y":20.0}}),
            json!({"id":zone,"position":{"x":20.0,"y":20.0}}),
            json!({"id":zone,"index":1,"position":{"x":20.0,"y":20.0},"extra":1}),
        ] {
            assert!(
                s.execute("zone.insert_point", params.clone(), None)
                    .is_err(),
                "insert {params:?} refused"
            );
        }
        for params in [
            json!({"id":zone,"index":4}),
            json!({"id":zone,"index":-1}),
            json!({"id":zone,"cutout":1,"index":0}),
            json!({"id":zone,"cutout":-1,"index":0}),
            json!({"id":999,"index":0}),
            json!({"id":zone}),
            json!({"id":zone,"index":1,"extra":1}),
        ] {
            assert!(
                s.execute("zone.remove_point", params.clone(), None)
                    .is_err(),
                "remove {params:?} refused"
            );
        }
        assert_eq!(s.inspect(), before, "rejections change nothing");
        let ring: Vec<serde_json::Value> = (0..256)
            .map(|i| {
                let t = std::f64::consts::TAU * i as f64 / 256.0;
                json!({"x":30.0 + 20.0 * t.cos(),"y":30.0 + 20.0 * t.sin()})
            })
            .collect();
        let mut fat = ring.clone();
        fat.push(ring[0].clone());
        let round =
            s.execute("zone.place", json!({"net":gnd,"outline":fat}), None)?["result"]["id"]
                .as_u64()
                .ok_or_else(missing)?;
        assert!(
            s.execute(
                "zone.insert_point",
                json!({"id":round,"index":0,"position":{"x":30.0,"y":50.5}}),
                None,
            )
            .is_err(),
            "a full outline cannot gain another vertex"
        );
        let three = s.execute(
            "zone.place",
            json!({"net":gnd,"outline":[{"x":55.0,"y":10.0},{"x":58.0,"y":10.0},{"x":56.5,"y":13.0},{"x":55.0,"y":10.0}]}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        assert!(
            s.execute("zone.remove_point", json!({"id":three,"index":1}), None)
                .is_err(),
            "removing a corner from a triangle would leave a line"
        );
        assert!(s.project.validate().is_ok());
        s.undo()?;
        s.undo()?;
        assert_eq!(
            s.project.board.zones.len(),
            1,
            "undo removes both test zones again"
        );
        s.undo()?;
        assert_eq!(
            s.project.board.zones[0].outline.len(),
            6,
            "undo brings the removed corner back"
        );
        Ok(())
    }
    #[test]
    fn zone_copper_can_be_taken_out_and_poured_back_in() -> Result<(), Error> {
        let mut s = Session::default();
        let a = s.execute("net.create", json!({"name":"A"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let b = s.execute("net.create", json!({"name":"B"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let zone = s.execute(
            "zone.place",
            json!({"net":a,"outline":[{"x":10.0,"y":10.0},{"x":30.0,"y":10.0},{"x":30.0,"y":30.0},{"x":10.0,"y":30.0},{"x":10.0,"y":10.0}]}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let track = s.execute(
            "track.add",
            json!({"net":b,"start":{"x":10.0,"y":10.0},"end":{"x":20.0,"y":10.0},"width":0.2}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let pairs = |s: &Session| -> usize {
            s.checks()["drc"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .iter()
                .filter(|f| {
                    f["objects"]
                        .as_array()
                        .is_some_and(|o| o.contains(&json!(zone)) && o.contains(&json!(track)))
                })
                .count()
        };
        assert!(pairs(&s) >= 1, "the track rides the filled zone edge");
        s.execute("zone.set_fill", json!({"id":zone,"filled":false}), None)?;
        assert_eq!(pairs(&s), 0, "an unfilled zone carries no copper");
        assert!(!s.project.board.zones[0].filled);
        s.execute("zone.set_fill", json!({"id":zone,"filled":true}), None)?;
        assert!(pairs(&s) >= 1, "pouring it back restores the finding");
        s.execute("zone.set_fill", json!({"id":zone,"filled":false}), None)?;
        let before = s.inspect();
        for params in [
            json!({"id":zone,"filled":false}),
            json!({"id":zone,"filled":true,"extra":1}),
            json!({"id":zone}),
            json!({"id":zone,"filled":"yes"}),
            json!({"id":zone,"filled":null}),
            json!({"id":9999,"filled":true}),
        ] {
            assert!(
                s.execute("zone.set_fill", params.clone(), None).is_err(),
                "{params:?} refused"
            );
        }
        assert_eq!(s.inspect(), before, "rejections change nothing");
        s.execute("zone.set_fill", json!({"id":zone,"filled":true}), None)?;
        assert!(pairs(&s) >= 1);
        s.undo()?;
        assert!(
            !s.project.board.zones[0].filled,
            "undo takes the copper out again"
        );
        assert_eq!(pairs(&s), 0, "and the finding falls silent");
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
        assert_eq!(
            s.project.board.texts[0].rotation, 0.0,
            "a placed label starts upright"
        );
        let turned = s.execute("text.rotate", json!({"id":id,"degrees":90.0}), None)?;
        assert_eq!(turned["result"]["rotation"], 90.0);
        s.execute("text.set", json!({"id":id,"rotation":-45.0}), None)?;
        assert_eq!(s.project.board.texts[0].rotation, -45.0);
        assert_eq!(
            s.project.board.texts[0].text, "Acme 2026",
            "turning a label leaves its string and size alone"
        );
        for params in [
            json!({"id":id,"degrees":null}),
            json!({"id":id,"degrees":1e9}),
            json!({"id":id,"degrees":90.0,"extra":1}),
            json!({"id":999,"degrees":90.0}),
            json!({"id":id}),
            json!({"id":id,"rotation":90.0}),
        ] {
            assert!(s.execute("text.rotate", params, None).is_err());
        }
        for params in [
            json!({"id":id}),
            json!({"id":id,"rotation":1e9}),
            json!({"id":id,"rotation":null,"text":null}),
        ] {
            assert!(s.execute("text.set", params, None).is_err());
        }
        s.undo()?;
        assert_eq!(
            s.project.board.texts[0].rotation, 90.0,
            "undo restores the previous angle"
        );
        assert!(s.project.validate().is_ok());
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

    #[test]
    fn unplated_holes_drill_move_resize_and_keep_copper_away() -> Result<(), Error> {
        let mut s = Session::default();
        let net = s.execute("net.create", json!({"name":"GND"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        s.execute(
            "board.hole_place",
            json!({"position":{"x":30.0,"y":20.0},"diameter":3.2}),
            None,
        )?;
        assert_eq!(s.project.board.holes.len(), 1);
        s.undo()?;
        assert!(s.project.board.holes.is_empty());
        let id = s.execute(
            "board.hole_place",
            json!({"position":{"x":30.0,"y":20.0},"diameter":3.2}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        s.execute(
            "board.hole_place",
            json!({"position":{"x":5.0,"y":20.0},"diameter":0.3}),
            None,
        )?;
        for bad in [
            json!({"position":{"x":30.0,"y":20.0},"diameter":0.01}),
            json!({"position":{"x":30.0,"y":20.0},"diameter":40.0}),
            json!({"position":{"x":null,"y":20.0},"diameter":3.2}),
            json!({"position":{"x":30.0,"y":20.0},"diameter":3.2,"net":1}),
        ] {
            assert!(s.execute("board.hole_place", bad, None).is_err());
        }
        s.execute(
            "board.hole_move",
            json!({"id":id,"position":{"x":28.0,"y":18.0}}),
            None,
        )?;
        s.execute("board.hole_set_size", json!({"id":id,"diameter":4.0}), None)?;
        assert_eq!(s.project.board.holes[0].diameter, 4.0);
        assert!(
            s.execute("board.hole_set_size", json!({"id":id,"diameter":0.0}), None)
                .is_err()
        );
        // A track crossing the drilled annulus is a clearance violation even
        // though nothing shares a net with an unplated hole.
        s.execute(
            "track.add",
            json!({"net":net,"start":{"x":20.0,"y":18.0},"end":{"x":36.0,"y":18.0},"width":0.2}),
            None,
        )?;
        let drc = s.checks()["drc"].clone();
        let findings = drc.as_array().ok_or_else(missing)?;
        assert!(
            findings
                .iter()
                .any(|f| f["objects"].as_array().is_some_and(|o| {
                    o.iter().any(|x| x.as_u64() == Some(id))
                        && f["code"].as_str() != Some("board_edge")
                }))
        );
        let second = s.project.board.holes[1].id;
        s.execute("object.delete", json!({"id":id}), None)?;
        s.execute("object.delete", json!({"id":second}), None)?;
        assert!(s.project.board.holes.is_empty());
        Ok(())
    }

    #[test]
    fn net_tie_merges_everything_and_net_swap_exchanges_names_only() -> Result<(), Error> {
        let mut s = Session::default();
        let a = s.execute("net.create", json!({"name":"A"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let b = s.execute("net.create", json!({"name":"B"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let r1 = s.execute(
            "component.add",
            json!({"kind":"resistor","reference":"R1","position":{"x":10.0,"y":10.0}}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let r2 = s.execute(
            "component.add",
            json!({"kind":"resistor","reference":"R2","position":{"x":30.0,"y":10.0}}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        s.execute(
            "wire.connect",
            json!({"net":a,"from":{"component":r1,"pin":"1"},"to":{"component":r2,"pin":"1"}}),
            None,
        )?;
        s.execute(
            "wire.connect",
            json!({"net":b,"from":{"component":r1,"pin":"2"},"to":{"component":r2,"pin":"2"}}),
            None,
        )?;
        s.execute(
            "board.set_pair_rule",
            json!({"net_a":a,"net_b":b,"clearance":0.5}),
            None,
        )?;
        // While both nets exist, a swap exchanges only their names.
        s.execute("net.swap", json!({"id_a":a,"id_b":b}), None)?;
        let name = |s: &Session, id: u64| {
            s.project
                .nets
                .iter()
                .find(|n| n.id == id)
                .map(|n| n.name.clone())
                .unwrap_or_default()
        };
        assert_eq!(name(&s, a), "B");
        assert_eq!(name(&s, b), "A");
        for c in &s.project.components {
            for pin in &c.pins {
                let expected = if pin.number == "1" { a } else { b };
                assert_eq!(pin.net, Some(expected));
            }
        }
        s.undo()?;
        assert_eq!(name(&s, a), "A");
        assert_eq!(name(&s, b), "B");
        assert!(
            s.execute("net.swap", json!({"id_a":a,"id_b":a}), None)
                .is_err()
        );
        assert!(
            s.execute("net.swap", json!({"id_a":a,"id_b":99}), None)
                .is_err()
        );
        // Merging folds every member of one net into the other.
        assert!(
            s.execute("net.tie", json!({"source":a,"target":a}), None)
                .is_err()
        );
        assert!(
            s.execute("net.tie", json!({"source":b,"target":99}), None)
                .is_err()
        );
        let moved = s.execute("net.tie", json!({"source":b,"target":a}), None)?["result"]["moved"]
            .as_u64()
            .ok_or_else(missing)?;
        assert!(moved >= 2, "the merge should report the members it moved");
        assert!(!s.project.nets.iter().any(|n| n.id == b));
        for c in &s.project.components {
            for pin in &c.pins {
                assert_eq!(pin.net, Some(a));
            }
        }
        assert!(s.project.board.pair_rules.is_empty());
        s.undo()?;
        assert!(s.project.nets.iter().any(|n| n.id == b));
        assert_eq!(s.project.board.pair_rules.len(), 1);
        Ok(())
    }

    #[test]
    fn batch_no_connect_flags_and_clears_a_whole_component() -> Result<(), Error> {
        let mut s = Session::default();
        s.execute(
            "component.add",
            json!({"kind":"resistor","reference":"R9","position":{"x":10.0,"y":10.0}}),
            None,
        )?;
        let r9 = s
            .project
            .components
            .last()
            .map(|c| c.id)
            .unwrap_or_default();
        let flagged = s.execute(
            "pins.set_all_no_connect",
            json!({"component":r9}),
            None,
        )?["result"]["flagged"]
            .as_u64()
            .ok_or_else(missing)?;
        assert_eq!(flagged, 2);
        assert_eq!(s.project.no_connects.len(), 2);
        assert!(s.project.validate().is_ok());
        assert!(
            s.execute("pins.set_all_no_connect", json!({"component":r9}), None)
                .is_err()
        );
        s.undo()?;
        assert!(s.project.no_connects.is_empty());
        s.execute("pins.set_all_no_connect", json!({"component":r9}), None)?;
        let n1 = s.execute("net.create", json!({"name":"N1"}), None)?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let r2 = s.execute(
            "component.add",
            json!({"kind":"resistor","reference":"R8","position":{"x":20.0,"y":10.0}}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or_else(missing)?;
        let flag = s
            .project
            .no_connects
            .iter()
            .find(|f| f.pin.component == r9 && f.pin.pin == "1")
            .map(|f| f.id)
            .unwrap_or_default();
        s.execute("object.delete", json!({"id":flag}), None)?;
        s.execute(
            "wire.connect",
            json!({"net":n1,"from":{"component":r9,"pin":"1"},"to":{"component":r2,"pin":"1"}}),
            None,
        )?;
        let cleared = s.execute(
            "pins.clear_all_no_connect",
            json!({"component":r9}),
            None,
        )?["result"]["cleared"]
            .as_u64()
            .ok_or_else(missing)?;
        assert_eq!(cleared, 1);
        assert!(s.project.no_connects.is_empty());
        assert!(
            s.execute("pins.clear_all_no_connect", json!({"component":r9}), None)
                .is_err()
        );
        Ok(())
    }
}
