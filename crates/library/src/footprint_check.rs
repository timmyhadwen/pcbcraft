use crate::{FootprintDefinition, FootprintPad, Graphic, PadShape};
use pcbcraft_geom::Point;
use serde::{Deserialize, Serialize};

pub const LAYER_COURTYARD: &str = "pcbcraft.courtyard";
pub const LAYER_SILK: &str = "F.SilkS";
pub const LAYER_FAB: &str = "F.Fab";

const SILK_PAD_CLEARANCE: f64 = 0.2;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FootprintFinding {
    pub code: String,
    pub severity: String,
    pub message: String,
    pub hint: String,
}

pub fn check_footprint(def: &FootprintDefinition) -> Vec<FootprintFinding> {
    let mut findings = Vec::new();
    check_pad_numbers(def, &mut findings);
    check_pad_overlap(def, &mut findings);
    check_pin1_mark(def, &mut findings);
    check_courtyard(def, &mut findings);
    check_silk_over_pad(def, &mut findings);
    check_fab_outline(def, &mut findings);
    findings
}

fn push(
    findings: &mut Vec<FootprintFinding>,
    code: &str,
    severity: &str,
    message: &str,
    hint: &str,
) {
    findings.push(FootprintFinding {
        code: code.into(),
        severity: severity.into(),
        message: message.into(),
        hint: hint.into(),
    });
}

fn check_pad_numbers(def: &FootprintDefinition, findings: &mut Vec<FootprintFinding>) {
    let mut seen = std::collections::BTreeSet::new();
    for pad in &def.pads {
        if !seen.insert(pad.number.clone()) {
            push(
                findings,
                "lib_pad_number_duplicate",
                "error",
                &format!("pad number {} appears more than once", pad.number),
                "give each pad a unique number",
            );
        }
    }
}

fn check_pad_overlap(def: &FootprintDefinition, findings: &mut Vec<FootprintFinding>) {
    let outlines: Vec<Vec<Point>> = def.pads.iter().map(pad_outline).collect();
    for (i, a) in outlines.iter().enumerate() {
        if a.len() < 3 {
            continue;
        }
        for (j, b) in outlines.iter().enumerate().skip(i + 1) {
            if b.len() < 3 {
                continue;
            }
            if polygons_overlap(a, b) {
                push(
                    findings,
                    "lib_pad_overlap",
                    "error",
                    &format!(
                        "pads {} and {} overlap",
                        def.pads[i].number, def.pads[j].number
                    ),
                    "move or shrink the pads so their copper does not intersect",
                );
                return;
            }
        }
    }
}

fn check_pin1_mark(def: &FootprintDefinition, findings: &mut Vec<FootprintFinding>) {
    let pin1 = def.pads.iter().find(|p| p.number == "1");
    let chamfered = pin1.is_some_and(|p| matches!(p.shape_kind(), PadShape::Chamfered(_)));
    let silk_dot = def.graphics.iter().any(|g| match g {
        Graphic::Circle { layer, radius, .. } => layer == LAYER_SILK && *radius >= 0.05,
        Graphic::Line { layer, .. } | Graphic::Polyline { layer, .. } => layer == LAYER_SILK,
        _ => false,
    });
    if !chamfered && !silk_dot {
        push(
            findings,
            "lib_pin1_missing",
            "error",
            "no pin 1 mark on silk or a chamfered first pad",
            "chamfer pad 1 or draw a small silk circle near pin 1",
        );
    }
}

fn check_courtyard(def: &FootprintDefinition, findings: &mut Vec<FootprintFinding>) {
    let Some((cmin, cmax)) = courtyard_bounds(def) else {
        push(
            findings,
            "lib_courtyard_missing",
            "error",
            "no courtyard rectangle on layer pcbcraft.courtyard",
            "run library.footprint.generate_courtyard or regenerate the footprint",
        );
        return;
    };
    let (pmin, pmax) = pad_bounds(def);
    if pmin.x < cmin.x || pmin.y < cmin.y || pmax.x > cmax.x || pmax.y > cmax.y {
        push(
            findings,
            "lib_courtyard_too_small",
            "error",
            "courtyard does not fully enclose the pads",
            "grow the courtyard so every pad sits inside it",
        );
    }
}

fn check_silk_over_pad(def: &FootprintDefinition, findings: &mut Vec<FootprintFinding>) {
    let pad_polys: Vec<Vec<Point>> = def.pads.iter().map(pad_outline).collect();
    for graphic in &def.graphics {
        if !graphic_on_layer(graphic, LAYER_SILK) {
            continue;
        }
        for sample in silk_samples(graphic) {
            for poly in &pad_polys {
                if poly.len() >= 3 && point_in_polygon(sample, poly) {
                    push(
                        findings,
                        "lib_silk_over_pad",
                        "error",
                        "silkscreen artwork sits on a pad opening",
                        "move silk at least 0.2 mm away from pad copper",
                    );
                    return;
                }
            }
            for poly in &pad_polys {
                if poly.len() >= 3 {
                    let expanded = expand_polygon(poly, SILK_PAD_CLEARANCE);
                    if point_in_polygon(sample, &expanded) {
                        push(
                            findings,
                            "lib_silk_over_pad",
                            "error",
                            "silkscreen is closer than 0.2 mm to a pad",
                            "move silk at least 0.2 mm away from pad copper",
                        );
                        return;
                    }
                }
            }
        }
    }
}

fn check_fab_outline(def: &FootprintDefinition, findings: &mut Vec<FootprintFinding>) {
    let has_fab = def.graphics.iter().any(|g| graphic_on_layer(g, LAYER_FAB));
    if !has_fab {
        push(
            findings,
            "lib_fab_outline_missing",
            "error",
            "no fabrication outline on layer F.Fab",
            "add a rectangle or polyline on F.Fab showing the package body",
        );
    }
}

fn graphic_on_layer(g: &Graphic, layer: &str) -> bool {
    match g {
        Graphic::Line { layer: l, .. }
        | Graphic::Rectangle { layer: l, .. }
        | Graphic::Circle { layer: l, .. }
        | Graphic::Arc { layer: l, .. }
        | Graphic::Polyline { layer: l, .. }
        | Graphic::Text { layer: l, .. } => l == layer,
    }
}

fn courtyard_bounds(def: &FootprintDefinition) -> Option<(Point, Point)> {
    let mut min = Point::new(f64::MAX, f64::MAX);
    let mut max = Point::new(f64::MIN, f64::MIN);
    let mut any = false;
    for g in &def.graphics {
        if let Graphic::Rectangle {
            start, end, layer, ..
        } = g
        {
            if layer != LAYER_COURTYARD {
                continue;
            }
            any = true;
            min.x = min.x.min(start.x.min(end.x));
            min.y = min.y.min(start.y.min(end.y));
            max.x = max.x.max(start.x.max(end.x));
            max.y = max.y.max(start.y.max(end.y));
        }
    }
    if any { Some((min, max)) } else { None }
}

fn pad_bounds(def: &FootprintDefinition) -> (Point, Point) {
    let mut min = Point::new(f64::MAX, f64::MAX);
    let mut max = Point::new(f64::MIN, f64::MIN);
    for pad in &def.pads {
        for p in pad_outline(pad) {
            min.x = min.x.min(p.x);
            min.y = min.y.min(p.y);
            max.x = max.x.max(p.x);
            max.y = max.y.max(p.y);
        }
    }
    if def.pads.is_empty() {
        (Point::default(), Point::default())
    } else {
        (min, max)
    }
}

fn pad_outline(pad: &FootprintPad) -> Vec<Point> {
    let local = match pad.shape_kind() {
        PadShape::Circle | PadShape::Oval => {
            let r = pad.size.x.min(pad.size.y) / 2.0;
            circle_points(r, 16)
        }
        PadShape::RoundRect { ratio } => rounded_rect(pad.size, pad.size.x.min(pad.size.y) * ratio),
        PadShape::Chamfered(ch) => chamfered_rect(pad.size, ch),
        PadShape::Custom { polygon } => polygon.to_vec(),
        PadShape::Rect | PadShape::Unsupported => rounded_rect(pad.size, 0.0),
    };
    local
        .into_iter()
        .map(|mut p| {
            p = p.rotate(-pad.rotation);
            pad.position.offset(p)
        })
        .collect()
}

fn rounded_rect(size: Point, radius: f64) -> Vec<Point> {
    let hx = size.x / 2.0;
    let hy = size.y / 2.0;
    if radius <= 0.0 {
        return vec![
            Point::new(-hx, -hy),
            Point::new(hx, -hy),
            Point::new(hx, hy),
            Point::new(-hx, hy),
        ];
    }
    let r = radius.min(hx).min(hy);
    let mut pts = Vec::new();
    for (cx, cy, a0) in [
        (hx - r, hy - r, 0.0),
        (-hx + r, hy - r, 90.0),
        (-hx + r, -hy + r, 180.0),
        (hx - r, -hy + r, 270.0),
    ] {
        for i in 0..4 {
            let a = (a0 + i as f64 * 22.5).to_radians();
            pts.push(Point::new(cx + r * a.cos(), cy - r * a.sin()));
        }
    }
    pts
}

fn chamfered_rect(size: Point, chamfer: crate::PadChamfer) -> Vec<Point> {
    let hx = size.x / 2.0;
    let hy = size.y / 2.0;
    let cut = chamfer.ratio * size.x.min(size.y);
    let mut pts = vec![
        Point::new(-hx + if chamfer.corners.top_left { cut } else { 0.0 }, -hy),
        Point::new(hx - if chamfer.corners.top_right { cut } else { 0.0 }, -hy),
        Point::new(
            hx,
            -hy + if chamfer.corners.bottom_right {
                cut
            } else {
                0.0
            },
        ),
        Point::new(
            hx,
            hy - if chamfer.corners.bottom_right {
                cut
            } else {
                0.0
            },
        ),
        Point::new(
            hx - if chamfer.corners.bottom_right {
                cut
            } else {
                0.0
            },
            hy,
        ),
        Point::new(
            -hx + if chamfer.corners.bottom_left {
                cut
            } else {
                0.0
            },
            hy,
        ),
        Point::new(
            -hx,
            hy - if chamfer.corners.bottom_left {
                cut
            } else {
                0.0
            },
        ),
        Point::new(-hx, -hy + if chamfer.corners.top_left { cut } else { 0.0 }),
    ];
    if pts.len() > 1 && pts.first() == pts.last() {
        pts.pop();
    }
    pts
}

fn circle_points(radius: f64, segments: usize) -> Vec<Point> {
    (0..segments)
        .map(|i| {
            let a = i as f64 / segments as f64 * std::f64::consts::TAU;
            Point::new(radius * a.cos(), radius * a.sin())
        })
        .collect()
}

fn silk_samples(g: &Graphic) -> Vec<Point> {
    match g {
        Graphic::Line { start, end, .. } => vec![*start, *end],
        Graphic::Rectangle { start, end, .. } => vec![*start, *end],
        Graphic::Circle { center, .. } => vec![*center],
        Graphic::Arc {
            start, mid, end, ..
        } => vec![*start, *mid, *end],
        Graphic::Polyline { points, .. } => points.clone(),
        Graphic::Text { position, .. } => vec![*position],
    }
}

fn expand_polygon(poly: &[Point], margin: f64) -> Vec<Point> {
    if poly.is_empty() || margin <= 0.0 {
        return poly.to_vec();
    }
    let mut cx = 0.0;
    let mut cy = 0.0;
    for p in poly {
        cx += p.x;
        cy += p.y;
    }
    cx /= poly.len() as f64;
    cy /= poly.len() as f64;
    poly.iter()
        .map(|p| {
            let dx = p.x - cx;
            let dy = p.y - cy;
            let len = (dx * dx + dy * dy).sqrt();
            if len <= 1e-9 {
                *p
            } else {
                Point::new(p.x + dx / len * margin, p.y + dy / len * margin)
            }
        })
        .collect()
}

fn point_in_polygon(point: Point, poly: &[Point]) -> bool {
    let mut inside = false;
    let mut j = poly.len().saturating_sub(1);
    for i in 0..poly.len() {
        let pi = poly[i];
        let pj = poly[j];
        if ((pi.y > point.y) != (pj.y > point.y))
            && (point.x < (pj.x - pi.x) * (point.y - pi.y) / (pj.y - pi.y + 1e-18) + pi.x)
        {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn segments_intersect(a0: Point, a1: Point, b0: Point, b1: Point) -> bool {
    fn orient(a: Point, b: Point, c: Point) -> f64 {
        (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
    }
    fn on_segment(a: Point, b: Point, c: Point) -> bool {
        c.x >= a.x.min(b.x) - 1e-9
            && c.x <= a.x.max(b.x) + 1e-9
            && c.y >= a.y.min(b.y) - 1e-9
            && c.y <= a.y.max(b.y) + 1e-9
    }
    let o1 = orient(a0, a1, b0);
    let o2 = orient(a0, a1, b1);
    let o3 = orient(b0, b1, a0);
    let o4 = orient(b0, b1, a1);
    if o1 * o2 < 0.0 && o3 * o4 < 0.0 {
        return true;
    }
    if o1.abs() < 1e-9 && on_segment(a0, a1, b0) {
        return true;
    }
    if o2.abs() < 1e-9 && on_segment(a0, a1, b1) {
        return true;
    }
    if o3.abs() < 1e-9 && on_segment(b0, b1, a0) {
        return true;
    }
    if o4.abs() < 1e-9 && on_segment(b0, b1, a1) {
        return true;
    }
    false
}

fn polygons_overlap(a: &[Point], b: &[Point]) -> bool {
    for p in a {
        if point_in_polygon(*p, b) {
            return true;
        }
    }
    for p in b {
        if point_in_polygon(*p, a) {
            return true;
        }
    }
    for i in 0..a.len() {
        let a0 = a[i];
        let a1 = a[(i + 1) % a.len()];
        for j in 0..b.len() {
            let b0 = b[j];
            let b1 = b[(j + 1) % b.len()];
            if segments_intersect(a0, a1, b0, b1) {
                return true;
            }
        }
    }
    false
}
