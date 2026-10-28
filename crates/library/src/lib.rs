//! Bounded KiCad library readers. Geometry is millimetres in screen (y-down) coordinates.
mod paths;
mod reader;
mod sexpr;
mod validation;
use pcbcraft_geom::Point;
pub use reader::{read_footprint, read_symbols};
use serde::{Deserialize, Serialize};
pub use sexpr::{Error, Limits};
pub use validation::custom_polygon_error;

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
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FootprintDefinition {
    pub name: String,
    pub description: String,
    pub keywords: String,
    pub pads: Vec<FootprintPad>,
    pub graphics: Vec<Graphic>,
    pub raw_source: String,
    pub unsupported: Vec<String>,
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
    /// Corner cuts of a `chamfered` pad; absent for every other shape.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chamfer: Option<PadChamfer>,
    /// Pad-local copper outline of a `custom` pad as an open vertex ring.
    /// The rectangular anchor given by `size` must lie inside it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub polygon: Vec<Point>,
    pub raw_source: String,
}
/// Most vertices accepted in a custom pad polygon.
pub const MAX_PAD_POLYGON_VERTICES: usize = 256;
/// Corners removed from a chamfered pad, named in pad-local y-down coordinates.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChamferCorners {
    pub top_left: bool,
    pub top_right: bool,
    pub bottom_left: bool,
    pub bottom_right: bool,
}
impl ChamferCorners {
    pub const ALL: Self = Self {
        top_left: true,
        top_right: true,
        bottom_left: true,
        bottom_right: true,
    };
    pub fn any(self) -> bool {
        self.top_left || self.top_right || self.bottom_left || self.bottom_right
    }
}
/// A chamfer cuts each selected corner by `ratio` times the shorter pad side
/// along both edges. The ratio is in (0, 0.5].
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PadChamfer {
    pub ratio: f64,
    pub corners: ChamferCorners,
}
/// Typed view of a pad's copper shape.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PadShape<'a> {
    Circle,
    Rect,
    Oval,
    RoundRect { ratio: f64 },
    Chamfered(PadChamfer),
    Custom { polygon: &'a [Point] },
    Unsupported,
}
impl FootprintPad {
    pub fn shape_kind(&self) -> PadShape<'_> {
        match (self.shape.as_str(), self.chamfer) {
            ("circle", _) => PadShape::Circle,
            ("rect", _) => PadShape::Rect,
            ("oval", _) => PadShape::Oval,
            ("roundrect", _) => PadShape::RoundRect {
                ratio: self.roundrect_ratio.unwrap_or(0.0),
            },
            ("chamfered", Some(chamfer)) => PadShape::Chamfered(chamfer),
            ("custom", _) if !self.polygon.is_empty() => PadShape::Custom {
                polygon: &self.polygon,
            },
            _ => PadShape::Unsupported,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Drill {
    pub size: Point,
    pub offset: Point,
}
