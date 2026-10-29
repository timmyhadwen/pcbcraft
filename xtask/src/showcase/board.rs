//! Board half of the showcase generator: outline, stackup, placement, copper.
//!
//! Routing is guided by the engine's own checks: every attempt runs as one
//! atomic batch, is kept only when it adds no finding, and is undone
//! otherwise, so the stored file is clean by construction.
//!
//! # Rules
//! - Runs after `board.sync`; every footprint already exists.
//! - A failed attempt must leave the session exactly as it was.
//!
//! # Dependencies
//! - xtask/src/showcase/design.rs
//!
//! # Do not
//! - Move a part after routing has started.
use super::{Gen, Res, design::PARTS};
use pcbcraft_engine::Command;
use pcbcraft_geom::Point;
use serde_json::{Value, json};

const WIDTH: f64 = 68.0;
const HEIGHT: f64 = 46.0;
const CORNER: f64 = 3.0;

fn pt(x: f64, y: f64) -> Value {
    json!({"x": x, "y": y})
}

/// Rounded rectangle outline as a vertex ring, clockwise from the top left.
fn rounded_outline() -> Vec<Value> {
    let mut v = Vec::new();
    let corners = [
        (CORNER, CORNER, 180.0_f64),
        (WIDTH - CORNER, CORNER, 270.0),
        (WIDTH - CORNER, HEIGHT - CORNER, 0.0),
        (CORNER, HEIGHT - CORNER, 90.0),
    ];
    for (cx, cy, start) in corners {
        for i in 0..=4 {
            let a = (start + 22.5 * f64::from(i)).to_radians();
            v.push(pt(
                round3(cx + CORNER * a.cos()),
                round3(cy + CORNER * a.sin()),
            ));
        }
    }
    v
}

fn round3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

/// Count of design-rule findings that are not plain missing connections.
pub fn bad_count(g: &Gen) -> usize {
    g.s.checks()["drc"]
        .as_array()
        .map_or(0, |a| a.iter().filter(|f| f["code"] != "unrouted").count())
}

/// Count of connections still missing.
pub fn open_count(g: &Gen) -> usize {
    g.s.checks()["airwires"].as_array().map_or(0, Vec::len)
}

/// Run commands as one atomic batch and keep them only when they add no
/// finding; otherwise the session is left exactly as it was.
pub fn attempt(g: &mut Gen, steps: Vec<(&str, Value)>) -> bool {
    let before = bad_count(g);
    let cmds: Vec<Command> = steps
        .into_iter()
        .map(|(c, p)| Command { command: c.into(), params: p })
        .collect();
    if g.s.batch(&cmds, None).is_err() {
        return false;
    }
    if bad_count(g) > before {
        let _ = g.s.undo();
        return false;
    }
    true
}

const SIGNAL: f64 = 0.2;
const POWER: f64 = 0.45;
const VIA: f64 = 0.45;
const DRILL: f64 = 0.2;

fn net_id(g: &Gen, name: &str) -> Res<u64> {
    g.nets.get(name).copied().ok_or_else(|| format!("unknown net {name}"))
}

fn fp(g: &Gen, reference: &str) -> Res<u64> {
    footprint_id(g, g.id_of(reference)?)
}

/// Route two pads on one copper layer with the engine router.
pub fn route_layer(g: &mut Gen, a: (&str, &str), b: (&str, &str), width: f64, layer: &str) -> Res<bool> {
    let (fa, fb) = (fp(g, a.0)?, fp(g, b.0)?);
    Ok(attempt(
        g,
        vec![(
            "track.route",
            json!({"from": {"footprint": fa, "pad": a.1}, "to": {"footprint": fb, "pad": b.1}, "layer": layer, "width": width, "mode": "octilinear"}),
        )],
    ))
}

/// Eight compass directions, nearest to `prefer` first.
fn directions(prefer: Point) -> Vec<(f64, f64)> {
    let mut d: Vec<(f64, f64)> = (0..8)
        .map(|i| {
            let a = f64::from(i) * std::f64::consts::FRAC_PI_4;
            (a.cos().round() * 1.0 + if i % 2 == 1 { a.cos().signum() * 0.0 } else { 0.0 }, a.sin().round())
        })
        .collect();
    let diag = std::f64::consts::FRAC_1_SQRT_2;
    for v in &mut d {
        if v.0 != 0.0 && v.1 != 0.0 {
            v.0 *= diag;
            v.1 *= diag;
        }
    }
    d.sort_by(|a, b| {
        let da = a.0 * prefer.x + a.1 * prefer.y;
        let db = b.0 * prefer.x + b.1 * prefer.y;
        db.partial_cmp(&da).unwrap_or(std::cmp::Ordering::Equal)
    });
    d
}

fn snap(v: f64) -> f64 {
    (v * 20.0).round() / 20.0
}

/// Drop a via next to a pad, joined to it by a short top-layer stub.
/// Returns the via centre when one fits.
pub fn fanout(g: &mut Gen, net: &str, reference: &str, pad: &str, prefer: Point) -> Res<Option<Point>> {
    let at = pad_at(g, reference, pad)?;
    let n = net_id(g, net)?;
    for dist in [0.8, 1.0, 1.25, 1.6, 2.0] {
        for (dx, dy) in directions(prefer) {
            let v = Point::new(snap(at.x + dx * dist), snap(at.y + dy * dist));
            if attempt(
                g,
                vec![
                    ("track.add", json!({"net": n, "start": pt(at.x, at.y), "end": pt(v.x, v.y), "width": SIGNAL, "layer": "F.Cu"})),
                    ("via.place", json!({"net": n, "position": pt(v.x, v.y), "diameter": VIA, "drill": DRILL})),
                ],
            ) {
                return Ok(Some(v));
            }
        }
    }
    Ok(None)
}

/// Route a net between two pads: top layer first, then through vias on the
/// bottom layer. Returns the layer used, or `None` when nothing fit.
pub fn connect(g: &mut Gen, net: &str, a: (&str, &str), b: (&str, &str), width: f64) -> Res<Option<&'static str>> {
    if route_layer(g, a, b, width, "F.Cu")? {
        return Ok(Some("F.Cu"));
    }
    let n = net_id(g, net)?;
    let pa = pad_at(g, a.0, a.1)?;
    let pb = pad_at(g, b.0, b.1)?;
    let toward_b = Point::new(pb.x - pa.x, pb.y - pa.y);
    let toward_a = Point::new(pa.x - pb.x, pa.y - pb.y);
    let start = g.s.project.clone();
    let Some(va) = fanout(g, net, a.0, a.1, toward_b)? else {
        return Ok(None);
    };
    let Some(vb) = fanout(g, net, b.0, b.1, toward_a)? else {
        rollback(g, start);
        return Ok(None);
    };
    if attempt(
        g,
        vec![(
            "track.route",
            json!({"from": pt(va.x, va.y), "to": pt(vb.x, vb.y), "layer": "B.Cu", "width": width, "mode": "octilinear", "net": n}),
        )],
    ) {
        return Ok(Some("B.Cu"));
    }
    rollback(g, start);
    Ok(None)
}

/// Restore a project snapshot taken before a multi-step attempt.
fn rollback(g: &mut Gen, snapshot: pcbcraft_doc::Project) {
    let _ = g.s.replace(snapshot);
}
/// Print findings by code, for the generator log.
pub fn report(g: &Gen) {
    let checks = g.s.checks();
    let mut counts = std::collections::BTreeMap::<String, usize>::new();
    for f in checks["drc"].as_array().into_iter().flatten() {
        *counts.entry(f["code"].as_str().unwrap_or("?").to_string()).or_default() += 1;
    }
    println!("drc by code: {counts:?}");
}

/// Place every footprint and set the board basics.
pub fn build(g: &mut Gen) -> Res<()> {
    g.cmd("board.resize", json!({"width": WIDTH, "height": HEIGHT}))?;
    g.cmd("board.set_outline", json!({"points": rounded_outline()}))?;
    for (name, kind, t) in [
        ("F.Cu", "copper", 0.035),
        ("Prepreg 1", "prepreg", 0.2104),
        ("In1.Cu", "copper", 0.0152),
        ("Core", "core", 1.065),
        ("In2.Cu", "copper", 0.0152),
        ("Prepreg 2", "prepreg", 0.2104),
        ("B.Cu", "copper", 0.035),
    ] {
        g.cmd("board.set_layer_record", json!({"name": name, "kind": kind, "thickness": t}))?;
    }
    g.cmd("board.set_finish", json!({"finish": "ENIG"}))?;
    for p in PARTS {
        let cid = g.id_of(p.reference)?;
        let fid = footprint_id(g, cid)?;
        g.cmd("footprint.move", json!({"id": fid, "position": pt(p.board.0, p.board.1)}))?;
        g.cmd("footprint.rotate", json!({"id": fid, "degrees": p.board_rot}))?;
    }
    for (net, a, b) in [
        ("LED0", ("U3", "9"), ("R10", "1")),
        ("LED1", ("U3", "10"), ("R11", "1")),
        ("LED2", ("U3", "11"), ("R12", "1")),
        ("SPI_CS", ("U3", "17"), ("U4", "1")),
        ("SPI_SCK", ("U3", "18"), ("U4", "6")),
    ] {
        let used = connect(g, net, a, b, 0.2)?;
        println!("route {net}: {used:?}");
    }
    report(g);
    Ok(())
}

/// Board footprint id of a component.
pub fn footprint_id(g: &Gen, component: u64) -> Res<u64> {
    g.s.project
        .board
        .footprints
        .iter()
        .find(|f| f.component == component)
        .map(|f| f.id)
        .ok_or_else(|| "footprint missing".to_string())
}

/// World position of a pad.
pub fn pad_at(g: &Gen, reference: &str, pad: &str) -> Res<Point> {
    let cid = g.id_of(reference)?;
    g.s.project
        .board
        .footprints
        .iter()
        .find(|f| f.component == cid)
        .and_then(|f| f.pad_position(pad))
        .ok_or_else(|| format!("no pad {reference}.{pad}"))
}
