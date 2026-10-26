//! Bounded KiCad library readers. Geometry is millimetres in screen (y-down) coordinates.
mod paths;
mod reader;
mod sexpr;
mod validation;
use pcbcraft_geom::Point;
pub use reader::{read_footprint, read_specctra_session, read_symbols};
use serde::{Deserialize, Serialize};
pub use sexpr::{Error, Limits};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SymbolDefinition {
    pub name: String,
    pub reference: String,
    pub value: String,
    pub description: String,
    pub keywords: String,
    pub footprint: String,
    pub pins: Vec<SymbolPin>,
    pub graphics: Vec<UnitGraphic>,
    pub extends: Option<String>,
    pub raw_source: String,
    pub unsupported: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SymbolPin {
    pub number: String,
    pub name: String,
    pub position: Point,
    /// KiCad angle, counterclockwise in a y-up Cartesian frame.
    pub angle: f64,
    pub length: f64,
    pub electrical_type: String,
    pub unit: u32,
    pub body_style: u32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnitGraphic {
    pub unit: u32,
    pub body_style: u32,
    pub primitive: Graphic,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Graphic {
    Line {
        start: Point,
        end: Point,
        width: f64,
        layer: String,
    },
    Rectangle {
        start: Point,
        end: Point,
        width: f64,
        fill: String,
        layer: String,
    },
    Circle {
        center: Point,
        radius: f64,
        width: f64,
        fill: String,
        layer: String,
    },
    Arc {
        start: Point,
        mid: Point,
        end: Point,
        width: f64,
        layer: String,
    },
    Polyline {
        points: Vec<Point>,
        width: f64,
        fill: String,
        layer: String,
    },
    Text {
        position: Point,
        angle: f64,
        text: String,
        size: Point,
        layer: String,
    },
}
/// One net of a Specctra design session and the component pins it names,
/// each pin given as a (reference, pin number) pair.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionNet {
    pub name: String,
    pub pins: Vec<(String, String)>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FootprintDefinition {
    pub name: String,
    pub description: String,
    pub keywords: String,
    pub pads: Vec<FootprintPad>,
    pub graphics: Vec<Graphic>,
    /// A 3D model reference when the footprint carries one. The transform is
    /// kept exactly as the source states it; this tool records and re-emits
    /// it without rendering geometry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<Model3D>,
    /// Design rule overrides scoped to this footprint: a local copper
    /// clearance and solder mask/paste margin shifts in millimetres. The
    /// clearance tightens checks; the margins are recorded and exported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rules: Option<FootprintRules>,
    pub raw_source: String,
    pub unsupported: Vec<String>,
}
/// Per-footprint design rule overrides. Each value is a millimetre amount;
/// a negative margin shrinks the paste or mask opening.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FootprintRules {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clearance: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mask_margin: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paste_margin: Option<f64>,
}
/// A reference to a 3D model file attached to a footprint, with the
/// placement transform from the source file's coordinate frame.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Model3D {
    pub path: String,
    #[serde(default)]
    pub offset: [f64; 3],
    #[serde(default = "unit_scale")]
    pub scale: [f64; 3],
    #[serde(default)]
    pub rotate: [f64; 3],
    pub raw_source: String,
}
fn unit_scale() -> [f64; 3] {
    [1.0, 1.0, 1.0]
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FootprintPad {
    pub number: String,
    pub pad_type: String,
    pub shape: String,
    pub position: Point,
    pub rotation: f64,
    pub size: Point,
    pub layers: Vec<String>,
    pub drill: Option<Drill>,
    pub roundrect_ratio: Option<f64>,
    pub raw_source: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Drill {
    pub size: Point,
    pub offset: Point,
}
