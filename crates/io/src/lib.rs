//! Native project persistence and original, limited KiCad PCB / preview writers.
use pcbcraft_doc::Project;
use pcbcraft_geom::Point;
use std::collections::BTreeMap;
use std::io::Read;
#[cfg(not(target_arch = "wasm32"))]
use std::io::Write;
use std::path::Path;
mod library_render;
use library_render::*;

pub const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;

pub fn load(path: &Path) -> Result<Project, String> {
    let text = read_text(path)?;
    let project: Project = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    project.validate()?;
    Ok(project)
}
pub fn read_text(path: &Path) -> Result<String, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut text = String::new();
    file.take(MAX_FILE_BYTES + 1)
        .read_to_string(&mut text)
        .map_err(|e| e.to_string())?;
    if text.len() as u64 > MAX_FILE_BYTES {
        return Err("project exceeds 8 MiB".into());
    }
    Ok(text)
}
#[cfg(not(target_arch = "wasm32"))]
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    temporary.write_all(bytes).map_err(|e| e.to_string())?;
    temporary.as_file().sync_all().map_err(|e| e.to_string())?;
    temporary.persist(path).map_err(|e| e.to_string())?;
    Ok(())
}
#[cfg(target_arch = "wasm32")]
pub fn atomic_write(_path: &Path, _bytes: &[u8]) -> Result<(), String> {
    Err("host filesystem writes are unavailable in the browser; download support is planned".into())
}
pub fn save(project: &Project, path: &Path) -> Result<(), String> {
    project.validate()?;
    let bytes = serde_json::to_vec_pretty(project).map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err("project exceeds 8 MiB".into());
    }
    atomic_write(path, &bytes)
}
fn quote(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".into())
}
/// SVG fragment for one project graphic: its outline as a stroked polyline, or a
/// text element for the text shape.
fn project_graphic_svg(g: &pcbcraft_doc::Graphic) -> String {
    if g.shape == "text" {
        let anchor = g.points.first().copied().unwrap_or_default();
        return format!(
            "<text x=\"{}\" y=\"{}\" font-size=\"{}\">{}</text>",
            anchor.x,
            anchor.y,
            g.width,
            escape(&g.text)
        );
    }
    let points = g
        .outline()
        .unwrap_or_else(|| g.points.clone())
        .iter()
        .map(|p| format!("{},{}", p.x, p.y))
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        "<polyline points=\"{points}\" fill=\"none\" stroke=\"#5ae0bc\" stroke-width=\"{}\"/>",
        g.width.min(0.4)
    )
}
/// Text drawn for a label, with per-kind punctuation.
fn label_text(l: &pcbcraft_doc::Label, net_name: &str) -> String {
    match l.kind.as_str() {
        "global" => format!("{{{net_name}}}"),
        "hierarchical" => format!("/{net_name}"),
        _ => net_name.to_string(),
    }
}
fn uuid(id: u64, kind: usize) -> String {
    format!("{kind:08x}-0000-4000-8000-{id:012x}")
}
fn footprint_bounds(p: &Project, f: &pcbcraft_doc::Footprint) -> (Point, Point) {
    let points = f.drawing_points(p);
    let min = points
        .iter()
        .fold(Point::new(f64::INFINITY, f64::INFINITY), |bounds, p| {
            Point::new(bounds.x.min(p.x), bounds.y.min(p.y))
        });
    let max = points.iter().fold(
        Point::new(f64::NEG_INFINITY, f64::NEG_INFINITY),
        |bounds, p| Point::new(bounds.x.max(p.x), bounds.y.max(p.y)),
    );
    if min.valid() && max.valid() {
        (min, max)
    } else {
        (Point::new(-3.4, -1.4), Point::new(3.4, 1.4))
    }
}

pub fn kicad_board(p: &Project) -> Result<String, String> {
    p.validate()?;
    let b = &p.board;
    let mut s = format!(
        "(kicad_pcb (version 20240108) (generator \"pcbcraft\")\n (general (thickness {})) (paper \"A4\")\n (layers (0 \"F.Cu\" signal) (31 \"B.Cu\" signal) (34 \"B.Paste\" user) (35 \"F.Paste\" user) (36 \"B.SilkS\" user \"b.silkscreen\") (37 \"F.SilkS\" user \"f.silkscreen\") (38 \"B.Mask\" user) (39 \"F.Mask\" user) (44 \"Edge.Cuts\" user) (46 \"B.CrtYd\" user) (47 \"F.CrtYd\" user) (48 \"B.Fab\" user) (49 \"F.Fab\" user))\n (setup (pad_to_mask_clearance 0))\n (net 0 \"\")\n",
        b.thickness
    );
    for (i, n) in p.nets.iter().enumerate() {
        s.push_str(&format!(" (net {} {})\n", i + 1, quote(&n.name)));
    }
    let ordinal = |net: Option<u64>| {
        p.nets
            .iter()
            .position(|n| Some(n.id) == net)
            .map(|i| i + 1)
            .unwrap_or(0)
    };
    for f in &b.footprints {
        let Some(c) = p.component(f.component) else {
            return Err("missing component".into());
        };
        let name = c
            .library_footprint
            .as_ref()
            .map(|l| {
                if l.library == "project" {
                    format!("pcbcraft_{}", l.definition.name)
                } else {
                    format!("{}:{}", l.library, l.definition.name)
                }
            })
            .unwrap_or_else(|| format!("pcbcraft_{}", c.kind));
        let attr = if f.pads.iter().any(|p| {
            p.geometry
                .as_ref()
                .is_some_and(|g| g.pad_type == "thru_hole")
        }) {
            "through_hole"
        } else {
            "smd"
        };
        let (min, max) = footprint_bounds(p, f);
        let label_x = (min.x + max.x) / 2.0;
        let reference_y = min.y - 1.2;
        let value_y = max.y + 1.2;
        s.push_str(&format!(" (footprint {} (layer \"F.Cu\") (uuid {}) (at {} {} {})\n (property \"Reference\" {} (at {label_x} {reference_y} {}) (layer \"F.SilkS\") (effects (font (size 1 1) (thickness 0.15))))\n (property \"Value\" {} (at {label_x} {value_y} {}) (layer \"F.SilkS\") (effects (font (size 1 1) (thickness 0.15))))\n (attr {attr})\n",quote(&name),uuid(f.id,1),f.position.x,f.position.y,f.rotation,quote(&c.reference),f.rotation,quote(&c.value),f.rotation));
        if let Some(library) = &c.library_footprint {
            for graphic in &library.definition.graphics {
                s.push_str(&kicad_graphic(graphic, f.rotation)?);
            }
        } else {
            s.push_str(" (fp_rect (start -3.4 -1.4) (end 3.4 1.4) (stroke (width 0.12) (type solid)) (fill none) (layer \"F.SilkS\"))\n");
        }
        for (i, pad) in f.pads.iter().enumerate() {
            let net = pad.net.map(|n| p.net_name(n)).unwrap_or("");
            let (pad_type, shape, size, rotation, layers, ratio, drill) =
                if let Some(g) = &pad.geometry {
                    if !matches!(g.pad_type.as_str(), "smd" | "thru_hole")
                        || !matches!(g.shape.as_str(), "circle" | "rect" | "oval" | "roundrect")
                        || (g.pad_type == "smd" && g.drill.is_some())
                        || (g.pad_type == "thru_hole" && g.drill.is_none())
                    {
                        return Err(
                        "KiCad export supports standard SMD or plated through-hole pad geometry"
                            .into(),
                    );
                    }
                    (
                        g.pad_type.as_str(),
                        g.shape.as_str(),
                        g.size,
                        f.rotation + g.rotation,
                        g.layers
                            .iter()
                            .map(|l| quote(l))
                            .collect::<Vec<_>>()
                            .join(" "),
                        if g.shape == "roundrect" {
                            format!(" (roundrect_rratio {})", g.roundrect_ratio.unwrap_or(0.0))
                        } else {
                            String::new()
                        },
                        g.drill
                            .as_ref()
                            .map(|d| {
                                let shape = if (d.size.x - d.size.y).abs() < 1e-9 {
                                    d.size.x.to_string()
                                } else {
                                    format!("oval {} {}", d.size.x, d.size.y)
                                };
                                format!(" (drill {shape} (offset {} {}))", d.offset.x, d.offset.y)
                            })
                            .unwrap_or_default(),
                    )
                } else {
                    (
                        "smd",
                        "circle",
                        Point::new(pad.diameter, pad.diameter),
                        f.rotation,
                        "\"F.Cu\" \"F.Mask\"".into(),
                        String::new(),
                        String::new(),
                    )
                };
            s.push_str(&format!(" (pad {} {pad_type} {shape} (at {} {} {rotation}) (size {} {}) (layers {layers}){ratio}{drill} (net {} {}) (uuid {}))\n",quote(&pad.number),pad.offset.x,pad.offset.y,size.x,size.y,ordinal(pad.net),quote(net),uuid(f.id,i+2)));
        }
        s.push_str(" )\n");
    }
    for t in &b.tracks {
        s.push_str(&format!(
            " (segment (start {} {}) (end {} {}) (width {}) (layer {}) (net {}) (uuid {}))\n",
            t.start.x,
            t.start.y,
            t.end.x,
            t.end.y,
            t.width,
            quote(t.layer.name()),
            ordinal(Some(t.net)),
            uuid(t.id, 1)
        ));
    }
    s.push_str(&format!(" (gr_rect (start 0 0) (end {} {}) (stroke (width 0.05) (type solid)) (fill none) (layer \"Edge.Cuts\"))\n",b.width,b.height));
    for c in &b.cutouts {
        s.push_str(&format!(" (gr_circle (center {} {}) (end {} {}) (stroke (width 0.05) (type solid)) (fill none) (layer \"Edge.Cuts\") (uuid {}))\n",c.center.x,c.center.y,c.center.x+c.radius,c.center.y,uuid(c.id,1)));
    }
    s.push_str(")\n");
    Ok(s)
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
fn preview_bounds(p: &Project, view: &str) -> (Point, Point) {
    let extend = |min: &mut Point, max: &mut Point, point: Point| {
        min.x = min.x.min(point.x);
        min.y = min.y.min(point.y);
        max.x = max.x.max(point.x);
        max.y = max.y.max(point.y);
    };
    if view == "pcb" {
        let mut min = Point::new(p.board.width, p.board.height);
        let mut max = Point::default();
        for g in p.graphics.iter().filter(|g| g.layer == "board") {
            for point in g.points.iter().flat_map(|pt| {
                g.outline()
                    .unwrap_or_else(|| vec![*pt])
                    .into_iter()
                    .chain(std::iter::once(*pt))
            }) {
                extend(&mut min, &mut max, point);
            }
        }
        for t in &p.board.texts {
            if t.position.x.is_finite() && t.position.y.is_finite() {
                extend(&mut min, &mut max, t.position);
            }
        }
        if min.x.is_finite() && max.x.is_finite() {
            return (
                Point::new(min.x.min(0.0), min.y.min(0.0)),
                Point::new(max.x.max(p.board.width), max.y.max(p.board.height)),
            );
        }
        return (Point::default(), Point::new(p.board.width, p.board.height));
    }
    let mut min = Point::new(f64::INFINITY, f64::INFINITY);
    let mut max = Point::new(f64::NEG_INFINITY, f64::NEG_INFINITY);
    for c in p.components.iter().flat_map(|c| c.schematic_views()) {
        for point in c.drawing_points() {
            let point = c.position.offset(point.rotate(-c.rotation));
            extend(&mut min, &mut max, point);
        }
    }
    for w in &p.drawn_wires {
        for point in &w.points {
            extend(&mut min, &mut max, *point);
        }
    }
    for g in p.graphics.iter().filter(|g| g.layer == "schematic") {
        for point in &g.points {
            extend(&mut min, &mut max, *point);
        }
        if let Some(outline) = g.outline() {
            for point in outline {
                extend(&mut min, &mut max, point);
            }
        }
    }
    if !min.x.is_finite() || !min.y.is_finite() || !max.x.is_finite() || !max.y.is_finite() {
        return (Point::default(), Point::new(60.0, 40.0));
    }
    (min, max)
}
/// RS-274X copper/outline layers plus an Excellon drill file, as filename to
/// text. Copper layers carry zones, tracks, via and pad copper; the outline
/// layer carries the board edge and circular cutouts; the drill file carries
/// every plated and unplated hole, round holes as single hits and oval holes
/// as G85 spans along the rotated long axis. Back-copper coordinates are
/// mirrored about x=0 as fabrication houses expect for bottom-side films.
pub fn gerber_files(p: &Project) -> Result<BTreeMap<String, String>, String> {
    p.validate()?;
    const MAX_OPS: usize = 100_000;
    const MAX_APERTURES: usize = 64;
    const MAX_HITS: usize = 50_000;
    #[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
    enum Ap {
        Circle(u64),
        Rect(u64, u64),
        Obround(u64, u64),
        Round(u64, u64, u64),
    }
    struct LayerFile {
        apertures: BTreeMap<Ap, u32>,
        body: String,
        arcs: String,
        ops: usize,
    }
    impl LayerFile {
        fn new() -> Self {
            Self {
                apertures: BTreeMap::new(),
                body: String::new(),
                arcs: String::new(),
                ops: 0,
            }
        }
        fn code(&mut self, ap: Ap) -> Result<u32, String> {
            if let Some(code) = self.apertures.get(&ap) {
                return Ok(*code);
            }
            if self.apertures.len() >= MAX_APERTURES {
                return Err("gerber export needs more than 64 distinct apertures".into());
            }
            let code = 10 + self.apertures.len() as u32;
            self.apertures.insert(ap, code);
            Ok(code)
        }
        fn flash(&mut self, code: u32, x: f64, y: f64) -> Result<(), String> {
            self.ops += 1;
            if self.ops > MAX_OPS {
                return Err("gerber export exceeds the object limit".into());
            }
            self.body
                .push_str(&format!("D{code}X{:.6}Y{:.6}D03*\n", x, y));
            Ok(())
        }
        fn stroke(&mut self, code: u32, points: &[(f64, f64)], closed: bool) -> Result<(), String> {
            self.ops += points.len();
            if self.ops > MAX_OPS {
                return Err("gerber export exceeds the object limit".into());
            }
            self.body.push_str(&format!("D{code}D02*\n"));
            for (i, (x, y)) in points.iter().enumerate() {
                self.body.push_str(&format!(
                    "X{:.6}Y{:.6}D0{}*\n",
                    x,
                    y,
                    if i == 0 { 2 } else { 1 }
                ));
            }
            if closed && let Some((x, y)) = points.first() {
                self.body.push_str(&format!("X{:.6}Y{:.6}D01*\n", x, y));
            }
            Ok(())
        }
        fn fill(&mut self, code: u32, points: &[(f64, f64)]) -> Result<(), String> {
            self.ops += points.len();
            if self.ops > MAX_OPS {
                return Err("gerber export exceeds the object limit".into());
            }
            self.body.push_str(&format!("G36*\nD{code}D02*\n"));
            for (i, (x, y)) in points.iter().enumerate() {
                self.body.push_str(&format!(
                    "X{:.6}Y{:.6}D0{}*\n",
                    x,
                    y,
                    if i == 0 { 2 } else { 1 }
                ));
            }
            self.body.push_str("G37*\n");
            Ok(())
        }
        fn finish(&self, mirror: bool, name: &str) -> String {
            let mirror = if mirror { "D" } else { "A" };
            let mut out =
                format!("G04 {name} generated by PCBCraft *\n%FSLANX*X{mirror}Y{mirror}MOMM*%\n");
            for (ap, code) in &self.apertures {
                match *ap {
                    Ap::Circle(d) => {
                        out.push_str(&format!("%ADD{code}C,{:.6}*%\n", d as f64 / 1000.0))
                    }
                    Ap::Rect(w, h) => out.push_str(&format!(
                        "%ADD{code}R,{:.6}X{:.6}*%\n",
                        w as f64 / 1000.0,
                        h as f64 / 1000.0
                    )),
                    Ap::Obround(w, h) => out.push_str(&format!(
                        "%ADD{code}O,{:.6}X{:.6}*%\n",
                        w as f64 / 1000.0,
                        h as f64 / 1000.0
                    )),
                    Ap::Round(w, h, r) => out.push_str(&format!(
                        "%ADD{code}R2,{} R1,{:.6}X{:.6} {r} 0 0*%\n",
                        if r * 2 == h {
                            "0".to_string()
                        } else {
                            format!("{:.6}", r as f64 / 1000.0)
                        },
                        w as f64 / 1000.0,
                        h as f64 / 1000.0
                    )),
                }
            }
            out.push_str("%LPD*%\nG01*\n");
            out.push_str(&self.body);
            out.push_str(&self.arcs);
            out.push_str("M02*\n");
            out
        }
    }
    let mut front = LayerFile::new();
    let mut back = LayerFile::new();
    let mirror =
        |x: f64, y: f64, back: bool| -> (f64, f64) { if back { (-x + 0.0, y) } else { (x, y) } };
    for z in &p.board.zones {
        if !z.filled {
            continue;
        }
        let is_back = z.layer == pcbcraft_doc::Layer::Back;
        let file = if is_back { &mut back } else { &mut front };
        let code = file.code(Ap::Circle(1))?;
        let outline: Vec<(f64, f64)> = z
            .outline
            .iter()
            .chain(z.cutouts.iter().flatten())
            .map(|q| mirror(q.x, q.y, is_back))
            .collect();
        file.fill(code, &outline)?;
    }
    for t in &p.board.tracks {
        let is_back = t.layer == pcbcraft_doc::Layer::Back;
        let file = if is_back { &mut back } else { &mut front };
        let w = (t.width * 1_000.0).round() as u64;
        let code = file.code(Ap::Circle(w.max(1)))?;
        file.stroke(
            code,
            &[
                mirror(t.start.x, t.start.y, is_back),
                mirror(t.end.x, t.end.y, is_back),
            ],
            false,
        )?;
    }
    for f in &p.board.footprints {
        for pad in &f.pads {
            let world = f.position.offset(pad.offset.rotate(-f.rotation));
            let through = pad
                .geometry
                .as_ref()
                .is_some_and(|g| g.drill.is_some() || g.pad_type.starts_with("thru_hole"));
            let is_back = pad.geometry.as_ref().is_some_and(|g| {
                !g.layers.is_empty() && !g.layers.iter().any(|l| l.starts_with("F."))
            });
            let ap = match &pad.geometry {
                Some(g) => {
                    let w = (g.size.x * 1_000.0).round() as u64;
                    let h = (g.size.y * 1_000.0).round() as u64;
                    match g.shape.as_str() {
                        "circle" => Ap::Circle(w.max(1)),
                        "rect" => Ap::Rect(w.max(1), h.max(1)),
                        "oval" => Ap::Obround(w.max(1), h.max(1)),
                        "roundrect" => Ap::Round(
                            w.max(1),
                            h.max(1),
                            ((w.min(h) as f64 * g.roundrect_ratio.unwrap_or(0.0))
                                .round()
                                .max(1.0)) as u64,
                        ),
                        _ => Ap::Circle((pad.diameter.max(0.001) * 1_000.0).round() as u64),
                    }
                }
                None => Ap::Circle((pad.diameter.max(0.001) * 1_000.0).round() as u64),
            };
            let (front_code, back_code) = if through {
                (front.code(ap.clone())?, back.code(ap.clone())?)
            } else if is_back {
                (0, back.code(ap)?)
            } else {
                (front.code(ap)?, 0)
            };
            if front_code != 0 {
                front.flash(front_code, world.x, world.y)?;
            }
            if back_code != 0 {
                let (x, y) = mirror(world.x, world.y, true);
                back.flash(back_code, x, y)?;
            }
        }
    }
    for v in &p.board.vias {
        let d = (v.diameter * 1_000.0).round() as u64;
        let ap = Ap::Circle(d.max(1));
        let front_code = front.code(ap.clone())?;
        let back_code = back.code(ap)?;
        front.flash(front_code, v.position.x, v.position.y)?;
        let (x, y) = mirror(v.position.x, v.position.y, true);
        back.flash(back_code, x, y)?;
    }
    let mut outline = LayerFile::new();
    let edge_code = outline.code(Ap::Circle(100))?;
    outline.stroke(
        edge_code,
        &[
            (0.0, 0.0),
            (p.board.width, 0.0),
            (p.board.width, p.board.height),
            (0.0, p.board.height),
        ],
        true,
    )?;
    for c in &p.board.cutouts {
        outline.ops += 1;
        if outline.ops > MAX_OPS {
            return Err("gerber export exceeds the object limit".into());
        }
        outline.arcs.push_str(&format!(
            "X{:.6}Y{:.6}D02*\nI{:.6}J{:.6}G02*\n",
            c.center.x - c.radius,
            c.center.y,
            c.radius,
            0.0
        ));
    }
    type DrillHit = (f64, f64, f64, f64);
    let mut drills: BTreeMap<(u64, u64), Vec<DrillHit>> = BTreeMap::new();
    let mut add_hole = |d: u64, span: u64, x: f64, y: f64, ux: f64, uy: f64| {
        drills.entry((d, span)).or_default().push((x, y, ux, uy));
    };
    for v in &p.board.vias {
        if v.drill > 0.0 {
            add_hole(
                (v.drill * 1_000.0).round() as u64,
                0,
                v.position.x,
                v.position.y,
                0.0,
                0.0,
            );
        }
    }
    for f in &p.board.footprints {
        for pad in &f.pads {
            let Some(g) = pad.geometry.as_ref() else {
                continue;
            };
            let Some(drill) = g.drill.as_ref() else {
                continue;
            };
            let (wide, narrow) = (
                drill.size.x.max(drill.size.y),
                drill.size.x.min(drill.size.y),
            );
            let d = (narrow * 1_000.0).round() as u64;
            if d == 0 {
                continue;
            }
            let span = ((wide - narrow) * 1_000.0).round() as u64;
            let local = pad.offset.offset(drill.offset.rotate(-g.rotation));
            let world = f.position.offset(local.rotate(-f.rotation));
            let (ux, uy) = if span == 0 {
                (0.0, 0.0)
            } else {
                let axis = if drill.size.x >= drill.size.y {
                    Point::new((wide - narrow) / 2.0, 0.0)
                } else {
                    Point::new(0.0, (wide - narrow) / 2.0)
                };
                let dir = axis.rotate(-(g.rotation + f.rotation));
                (dir.x, dir.y)
            };
            add_hole(d, span, world.x, world.y, ux, uy);
        }
    }
    let total: usize = drills.values().map(|v| v.len()).sum();
    if total > MAX_HITS {
        return Err("gerber export exceeds the drill hit limit".into());
    }
    let mut drill = String::from("M48\nFMAT,2\nMETRIC,TZ\n");
    for (n, (d, _)) in drills.keys().enumerate() {
        drill.push_str(&format!("T{}C{:.3}\n", n + 1, *d as f64 / 1000.0));
    }
    drill.push_str("%\nG90\nG05\n");
    let origin = p.board.drill_origin;
    for (n, holes) in drills.values().enumerate() {
        drill.push_str(&format!("T{}\n", n + 1));
        for (x, y, ux, uy) in holes {
            let (x, y) = (x - origin.x, y - origin.y);
            if *ux == 0.0 && *uy == 0.0 {
                drill.push_str(&format!("X{x:.3}Y{y:.3}\n"));
            } else {
                drill.push_str(&format!(
                    "X{a:.3}Y{b:.3}G85X{c:.3}Y{d:.3}\n",
                    a = x - ux,
                    b = y - uy,
                    c = x + ux,
                    d = y + uy
                ));
            }
        }
    }
    drill.push_str("M30\n");
    let mut files = BTreeMap::new();
    files.insert(
        "pcb-F.Cu.gbr".into(),
        front.finish(false, "front copper layer"),
    );
    files.insert(
        "pcb-B.Cu.gbr".into(),
        back.finish(true, "back copper layer"),
    );
    files.insert(
        "pcb-Edge.gbr".into(),
        outline.finish(false, "board outline"),
    );
    files.insert("pcb-Drill.drl".into(), drill);
    Ok(files)
}
/// Bill of materials as CSV: identical parts collapsed into one row with a
/// sorted, quoted reference list. Requested property keys become extra columns.
pub fn bom_csv(p: &Project, fields: &[String]) -> Result<String, String> {
    p.validate()?;
    if fields.len() > 16 {
        return Err("a bill of materials lists at most 16 properties".into());
    }
    for key in fields {
        if key.is_empty() || key.len() > 64 {
            return Err("property names must be 1 to 64 bytes".into());
        }
    }
    let mut groups: BTreeMap<(String, String, String, Vec<String>), Vec<String>> = BTreeMap::new();
    for c in &p.components {
        let footprint = match &c.library_footprint {
            Some(footprint) => format!("{}:{}", footprint.library, footprint.definition.name),
            None => "starter pads".into(),
        };
        let values: Vec<String> = fields
            .iter()
            .map(|key| c.field(key).unwrap_or("").to_string())
            .collect();
        groups
            .entry((c.value.clone(), c.kind.clone(), footprint, values))
            .or_default()
            .push(c.reference.clone());
    }
    let mut out = String::from("quantity,references,value,kind,footprint");
    for key in fields {
        out.push(',');
        out.push_str(&csv_cell(key));
    }
    out.push('\n');
    for ((value, kind, footprint, values), mut references) in groups {
        references.sort();
        let joined = references.join(",");
        out.push_str(&references.len().to_string());
        for cell in [joined, value, kind, footprint] {
            out.push(',');
            out.push_str(&csv_cell(&cell));
        }
        for value in values {
            out.push(',');
            out.push_str(&csv_cell(&value));
        }
        out.push('\n');
    }
    Ok(out)
}
/// Quote a CSV cell only when it needs it: commas, quotes or newlines.
fn csv_cell(text: &str) -> String {
    if text.contains([',', '"', '\n']) {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text.to_string()
    }
}
/// Plain-text netlist: one line per connected net, listing the component
/// pins it carries as `REFERENCE.PIN`, sorted by net name and reference.
pub fn netlist_text(p: &Project) -> Result<String, String> {
    p.validate()?;
    const MAX_PINS: usize = 50_000;
    let names: std::collections::BTreeMap<u64, String> =
        p.nets.iter().map(|n| (n.id, n.name.clone())).collect();
    let mut rows: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut pins = 0usize;
    for c in &p.components {
        for pin in &c.pins {
            let Some(net) = pin.net else {
                continue;
            };
            pins += 1;
            if pins > MAX_PINS {
                return Err("netlist exceeds the pin limit".into());
            }
            let name = names
                .get(&net)
                .ok_or("pin references an unknown net")?
                .clone();
            rows.entry(name)
                .or_default()
                .push(format!("{}.{}", c.reference, pin.number));
        }
    }
    let mut out = String::from("# PCBCraft netlist\n");
    out.push_str(&format!(
        "# board: {} x {} mm\n# nets: {}\n",
        p.board.width,
        p.board.height,
        rows.len()
    ));
    for (name, list) in &mut rows {
        list.sort();
        out.push_str(name);
        for entry in list {
            out.push(' ');
            out.push_str(entry);
        }
        out.push('\n');
    }
    Ok(out)
}
/// Board drawing as an ASCII DXF (R12 entities): tracks become lines, vias
/// and round pads circles, oval and rectangular pads closed polylines, zones
/// and board-layer graphics outlines, arcs three-point arcs, labels text.
/// The board edge and circular cutouts are drawn on their own layer.
pub fn dxf_board(p: &Project) -> Result<String, String> {
    p.validate()?;
    const MAX_ENTITIES: usize = 20_000;
    struct Writer {
        entities: String,
        count: usize,
        min_x: f64,
        min_y: f64,
        max_x: f64,
        max_y: f64,
    }
    impl Writer {
        fn follow(&mut self, x: f64, y: f64) {
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
        }
        fn keep(&mut self, layer: &str, body: String) -> Result<(), String> {
            self.count += 1;
            if self.count > MAX_ENTITIES {
                return Err("DXF export exceeds the entity limit".into());
            }
            self.entities.push_str("0\nENTITY\n8\n");
            self.entities.push_str(layer);
            self.entities.push('\n');
            self.entities.push_str(&body);
            Ok(())
        }
        fn line(&mut self, layer: &str, a: (f64, f64), b: (f64, f64)) -> Result<(), String> {
            self.follow(a.0, a.1);
            self.follow(b.0, b.1);
            self.keep(
                layer,
                format!(
                    "0\nLINE\n10\n{:.3}\n20\n{:.3}\n11\n{:.3}\n21\n{:.3}\n",
                    a.0, a.1, b.0, b.1
                ),
            )
        }
        fn circle(&mut self, layer: &str, x: f64, y: f64, r: f64) -> Result<(), String> {
            self.follow(x - r, y - r);
            self.follow(x + r, y + r);
            self.keep(
                layer,
                format!("0\nCIRCLE\n10\n{x:.3}\n20\n{y:.3}\n40\n{r:.3}\n"),
            )
        }
        fn polyline(
            &mut self,
            layer: &str,
            points: &[(f64, f64)],
            closed: bool,
        ) -> Result<(), String> {
            let mut body = format!(
                "0\nLWPOLYLINE\n90\n{}\n70\n{}\n",
                points.len(),
                u8::from(closed)
            );
            for (x, y) in points {
                self.follow(*x, *y);
                body.push_str(&format!("10\n{x:.3}\n20\n{y:.3}\n"));
            }
            self.keep(layer, body)
        }
        fn text(
            &mut self,
            layer: &str,
            x: f64,
            y: f64,
            height: f64,
            rotation: f64,
            value: &str,
        ) -> Result<(), String> {
            self.follow(x, y);
            self.keep(
                layer,
                format!(
                    "0\nTEXT\n10\n{x:.3}\n20\n{y:.3}\n40\n{height:.3}\n50\n{rotation:.1}\n1\n{value}\n"
                ),
            )
        }
    }
    fn xy(q: &pcbcraft_geom::Point) -> (f64, f64) {
        (q.x, q.y)
    }
    let mut w = Writer {
        entities: String::new(),
        count: 0,
        min_x: 0.0,
        min_y: 0.0,
        max_x: 0.0,
        max_y: 0.0,
    };
    let corners = [
        (0.0, 0.0),
        (p.board.width, 0.0),
        (p.board.width, p.board.height),
        (0.0, p.board.height),
    ];
    w.polyline("Edge", &corners, true)?;
    for c in &p.board.cutouts {
        w.circle("Edge", c.center.x, c.center.y, c.radius)?;
    }
    for t in &p.board.tracks {
        let layer = if t.layer == pcbcraft_doc::Layer::Back {
            "B_Cu"
        } else {
            "F_Cu"
        };
        w.line(layer, xy(&t.start), xy(&t.end))?;
    }
    for v in &p.board.vias {
        w.circle("Via", v.position.x, v.position.y, v.diameter / 2.0)?;
        if v.drill > 0.0 {
            w.circle("Drill", v.position.x, v.position.y, v.drill / 2.0)?;
        }
    }
    for f in &p.board.footprints {
        for pad in &f.pads {
            let world = f.position.offset(pad.offset.rotate(-f.rotation));
            let back = pad.geometry.as_ref().is_some_and(|g| {
                !g.layers.is_empty() && !g.layers.iter().any(|l| l.starts_with("F."))
            });
            let layer = if back { "B_Pads" } else { "F_Pads" };
            let geometry = match &pad.geometry {
                Some(g) => g,
                None => {
                    w.circle(layer, world.x, world.y, pad.diameter / 2.0)?;
                    continue;
                }
            };
            if geometry.shape == "circle" {
                w.circle(
                    layer,
                    world.x,
                    world.y,
                    geometry.size.x.max(geometry.size.y) / 2.0,
                )?;
            } else {
                let half_x = geometry.size.x / 2.0;
                let half_y = geometry.size.y / 2.0;
                w.polyline(
                    layer,
                    &[
                        (world.x - half_x, world.y - half_y),
                        (world.x + half_x, world.y - half_y),
                        (world.x + half_x, world.y + half_y),
                        (world.x - half_x, world.y + half_y),
                    ],
                    true,
                )?;
            }
            if let Some(drill) = &geometry.drill {
                w.circle(
                    "Drill",
                    world.x,
                    world.y,
                    drill.size.x.max(drill.size.y) / 2.0,
                )?;
            }
        }
    }
    for z in &p.board.zones {
        let layer = if z.layer == pcbcraft_doc::Layer::Back {
            "B_Cu"
        } else {
            "F_Cu"
        };
        w.polyline(layer, &z.outline.iter().map(xy).collect::<Vec<_>>(), true)?;
        for cutout in &z.cutouts {
            w.polyline(layer, &cutout.iter().map(xy).collect::<Vec<_>>(), true)?;
        }
    }
    for g in p.graphics.iter().filter(|g| g.layer == "board") {
        let layer = "Board_Graphics";
        if g.shape == "arc" && g.points.len() == 3 {
            let (a, b, c) = (g.points[0], g.points[1], g.points[2]);
            let d = 2.0 * (a.x * (b.y - c.y) + b.x * (c.y - a.y) + c.x * (a.y - b.y));
            if d.abs() > 1e-9 {
                let a2 = a.x * a.x + a.y * a.y;
                let b2 = b.x * b.x + b.y * b.y;
                let c2 = c.x * c.x + c.y * c.y;
                let ux = (a2 * (b.y - c.y) + b2 * (c.y - a.y) + c2 * (a.y - b.y)) / d;
                let uy = (a2 * (c.x - b.x) + b2 * (a.x - c.x) + c2 * (b.x - a.x)) / d;
                let angle = |x: f64, y: f64| (y - uy).atan2(x - ux).to_degrees().rem_euclid(360.0);
                let x0 = angle(a.x, a.y);
                let xm = angle(b.x, b.y);
                let x1 = angle(c.x, c.y);
                let ccw = (x1 - x0).rem_euclid(360.0);
                let to_mid = (xm - x0).rem_euclid(360.0);
                let (start, end) = if to_mid <= ccw { (x0, x1) } else { (x1, x0) };
                let radius = ((a.x - ux).powi(2) + (a.y - uy).powi(2)).sqrt();
                w.follow(ux - radius, uy - radius);
                w.follow(ux + radius, uy + radius);
                w.keep(
                    layer,
                    format!(
                        "0\nARC\n10\n{ux:.3}\n20\n{uy:.3}\n40\n{radius:.3}\n50\n{start:.3}\n51\n{end:.3}\n"
                    ),
                )?;
                continue;
            }
        }
        if let Some(outline) = g.outline() {
            let closed = g.shape != "line";
            w.polyline(layer, &outline.iter().map(xy).collect::<Vec<_>>(), closed)?;
        } else if g.shape == "text"
            && let Some(anchor) = g.points.first()
        {
            w.text(layer, anchor.x, anchor.y, g.width, 0.0, &g.text)?;
        }
    }
    for t in &p.board.texts {
        let layer = if t.layer == pcbcraft_doc::Layer::Back {
            "B_Silk"
        } else {
            "F_Silk"
        };
        w.text(
            layer,
            t.position.x,
            t.position.y,
            t.size,
            t.rotation,
            &t.text,
        )?;
    }
    Ok(format!(
        "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1009\n9\n$INSUNITS\n70\n4\n9\n$EXTMIN\n10\n{:.3}\n20\n{:.3}\n9\n$EXTMAX\n10\n{:.3}\n20\n{:.3}\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n{}0\nENDSEC\n0\nEOF\n",
        w.min_x, w.min_y, w.max_x, w.max_y, w.entities
    ))
}
pub fn svg(p: &Project, view: &str) -> Result<String, String> {
    p.validate()?;
    if view != "pcb" && view != "schematic" {
        return Err("view must be pcb or schematic".into());
    }
    let (min, max) = preview_bounds(p, view);
    let width = (max.x - min.x).max(1.0) + 16.0;
    let height = (max.y - min.y).max(1.0) + 16.0;
    let mut s = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1000\" height=\"680\" viewBox=\"{} {} {width} {height}\"><rect x=\"{}\" y=\"{}\" width=\"{width}\" height=\"{height}\" fill=\"#131c29\"/><g font-family=\"sans-serif\" font-size=\"1.5\" fill=\"#dbe7f2\">",
        min.x - 8.0,
        min.y - 8.0,
        min.x - 8.0,
        min.y - 8.0
    );
    if view == "pcb" {
        s.push_str(&format!("<rect width=\"{}\" height=\"{}\" rx=\"0.4\" fill=\"#193a36\" stroke=\"#5ae0bc\" stroke-width=\"0.15\"/>",p.board.width,p.board.height));
        for c in &p.board.cutouts {
            s.push_str(&format!("<circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"#131c29\" stroke=\"#5ae0bc\" stroke-width=\"0.1\"/>",c.center.x,c.center.y,c.radius));
        }
        for z in &p.board.zones {
            let color = if z.layer == pcbcraft_doc::Layer::Front {
                "#f58d79"
            } else {
                "#74a7ec"
            };
            let points: Vec<String> = z
                .outline
                .iter()
                .map(|pt| format!("{} {}", pt.x, pt.y))
                .collect();
            s.push_str(&format!(
                "<polygon points=\"{}\" fill=\"{color}\" fill-opacity=\"{}\" stroke=\"{color}\" stroke-width=\"0.1\"{} />",
                points.join(" "),
                if z.filled { "0.3" } else { "0" },
                if z.filled { "" } else { " stroke-dasharray=\"0.6 0.4\"" },
            ));
            for cut in &z.cutouts {
                let cut_points: Vec<String> =
                    cut.iter().map(|pt| format!("{} {}", pt.x, pt.y)).collect();
                s.push_str(&format!(
                    "<polygon points=\"{}\" fill=\"none\" stroke=\"{color}\" stroke-width=\"0.1\" stroke-dasharray=\"0.6 0.3\"/>",
                    cut_points.join(" ")
                ));
            }
        }
        for t in &p.board.tracks {
            s.push_str(&format!("<path d=\"M {} {} L {} {}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{}\" stroke-linecap=\"round\"/>",t.start.x,t.start.y,t.end.x,t.end.y,if t.layer==pcbcraft_doc::Layer::Front {"#f58d79"} else {"#74a7ec"},t.width));
        }
        for v in &p.board.vias {
            s.push_str(&format!("<circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"#f58d79\" stroke=\"#f58d79\" stroke-width=\"0.05\"/>",v.position.x,v.position.y,v.diameter/2.0));
            if v.drill > 0.0 {
                s.push_str(&format!(
                    "<circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"#131c29\"/>",
                    v.position.x,
                    v.position.y,
                    v.drill / 2.0
                ));
            }
        }
        for t in &p.board.texts {
            let spin = if t.rotation == 0.0 {
                String::new()
            } else {
                format!(
                    " transform=\"rotate({} {} {})\"",
                    -t.rotation, t.position.x, t.position.y
                )
            };
            s.push_str(&format!(
                "<text x=\"{}\" y=\"{}\" font-size=\"{}\" fill=\"{}\"{}>{}</text>",
                t.position.x,
                t.position.y,
                t.size,
                if t.layer == pcbcraft_doc::Layer::Front {
                    "#dbe7f2"
                } else {
                    "#9ec5ff"
                },
                spin,
                escape(&t.text)
            ));
        }
        for g in p.graphics.iter().filter(|g| g.layer == "board") {
            s.push_str(&project_graphic_svg(g));
        }
        for f in &p.board.footprints {
            if let Some(library) = p
                .component(f.component)
                .and_then(|c| c.library_footprint.as_ref())
            {
                s.push_str(&format!(
                    "<g transform=\"translate({} {}) rotate({})\">",
                    f.position.x, f.position.y, -f.rotation
                ));
                for g in &library.definition.graphics {
                    let mut g = g.clone();
                    if let pcbcraft_library::Graphic::Text { text, .. } = &mut g
                        && let Some(c) = p.component(f.component)
                    {
                        *text = text
                            .replace("${REFERENCE}", &c.reference)
                            .replace("${VALUE}", &c.value)
                            .replace("%R", &c.reference)
                            .replace("%V", &c.value);
                    }
                    s.push_str(&svg_graphic(&g));
                }
                s.push_str("</g>");
            }
            for pad in &f.pads {
                let points = pad
                    .outline_points()
                    .into_iter()
                    .map(|pos| f.position.offset(pos.rotate(-f.rotation)))
                    .collect::<Vec<_>>();
                s.push_str(&svg_path(&points, "#e6c27d", 0.0, "#e6c27d"));
                let drill = pad
                    .drill_outline_points()
                    .into_iter()
                    .map(|pos| f.position.offset(pos.rotate(-f.rotation)))
                    .collect::<Vec<_>>();
                if !drill.is_empty() {
                    s.push_str(&format!(
                        "<g data-drill=\"{}\">{}</g>",
                        escape(&pad.number),
                        svg_path(&drill, "#131c29", 0.0, "#131c29")
                    ));
                }
            }
            if let Some(c) = p.component(f.component) {
                let (min, max) = footprint_bounds(p, f);
                let label_x = (min.x + max.x) / 2.0;
                for (label, y) in [(&c.reference, min.y - 1.2), (&c.value, max.y + 1.2)] {
                    let point = f
                        .position
                        .offset(Point::new(label_x, y).rotate(-f.rotation));
                    s.push_str(&format!("<text x=\"{}\" y=\"{}\" text-anchor=\"middle\" dominant-baseline=\"middle\" font-size=\"1\" transform=\"rotate({} {} {})\">{}</text>",point.x,point.y,-f.rotation,point.x,point.y,escape(label)));
                }
            }
        }
    } else {
        for w in &p.wires {
            if let Some(path) = p.wire_path(w) {
                let points = path
                    .iter()
                    .map(|p| format!("{},{}", p.x, p.y))
                    .collect::<Vec<_>>()
                    .join(" ");
                s.push_str(&format!("<polyline points=\"{points}\" fill=\"none\" stroke=\"#5ae0bc\" stroke-width=\"0.2\"/>"));
            }
        }
        for w in &p.drawn_wires {
            let points = w
                .points
                .iter()
                .map(|p| format!("{},{}", p.x, p.y))
                .collect::<Vec<_>>()
                .join(" ");
            s.push_str(&format!("<polyline points=\"{points}\" fill=\"none\" stroke=\"#5ae0bc\" stroke-width=\"0.2\"/>"));
        }
        for g in p.graphics.iter().filter(|g| g.layer == "schematic") {
            s.push_str(&project_graphic_svg(g));
        }
        for l in &p.labels {
            if l.kind == "junction" {
                s.push_str(&format!(
                    "<circle cx=\"{}\" cy=\"{}\" r=\"0.35\" fill=\"#5ae0bc\"/>",
                    l.position.x, l.position.y
                ));
            } else {
                s.push_str(&format!(
                    "<text x=\"{}\" y=\"{}\">{}</text>",
                    l.position.x,
                    l.position.y,
                    escape(&label_text(l, p.net_name(l.net)))
                ));
            }
        }
        for f in &p.no_connects {
            if let Some(point) = p.pin_position(&f.pin) {
                s.push_str(&format!("<path d=\"M {} {} L {} {} M {} {} L {} {}\" stroke=\"#f58d79\" stroke-width=\"0.25\"/>",point.x-0.6,point.y-0.6,point.x+0.6,point.y+0.6,point.x-0.6,point.y+0.6,point.x+0.6,point.y-0.6));
            }
        }
        for c in p.components.iter().flat_map(|c| c.schematic_views()) {
            s.push_str(&format!(
                "<g transform=\"translate({} {}) rotate({})\">",
                c.position.x, c.position.y, -c.rotation
            ));
            if let Some(library) = &c.library_symbol {
                for g in library
                    .definition
                    .graphics
                    .iter()
                    .filter(|g| (g.unit == 0 || g.unit == library.unit) && g.body_style <= 1)
                {
                    s.push_str(&svg_graphic(&g.primitive));
                }
                for pin in library.definition.pins.iter().filter(|pin| {
                    (pin.unit == 0 && library.unit == 1 || pin.unit == library.unit)
                        && pin.body_style <= 1
                }) {
                    let angle = pin.angle.to_radians();
                    let end = pin.position.offset(Point::new(
                        pin.length * angle.cos(),
                        -pin.length * angle.sin(),
                    ));
                    s.push_str(&svg_path(&[pin.position, end], "#dbe7f2", 0.15, "none"));
                    s.push_str(&format!(
                        "<text x=\"{}\" y=\"{}\" font-size=\"0.9\">{}</text>",
                        pin.position.x + 0.3,
                        pin.position.y - 0.3,
                        escape(&pin.number)
                    ));
                }
            } else {
                s.push_str("<rect x=\"-3\" y=\"-1.5\" width=\"6\" height=\"3\" fill=\"#1e2c40\" stroke=\"#dbe7f2\" stroke-width=\"0.15\"/><path d=\"M -5 0 H -3 M 3 0 H 5\" stroke=\"#dbe7f2\" stroke-width=\"0.2\"/>");
            }
            s.push_str(&format!(
                "</g><text x=\"{}\" y=\"{}\" text-anchor=\"middle\">{} · {}</text>",
                c.position.x,
                c.position.y - 3.0,
                escape(&c.reference),
                escape(&c.value)
            ));
        }
    }
    s.push_str("</g></svg>");
    Ok(s)
}

/// A bounded CPU preview for headless MCP. The desktop has its own actual UI capture.
pub fn preview_png(p: &Project, view: &str) -> Result<Vec<u8>, String> {
    p.validate()?;
    if view != "pcb" && view != "schematic" {
        return Err("view must be pcb or schematic".into());
    }
    let mut image = image::RgbImage::from_pixel(1000, 680, image::Rgb([19, 28, 41]));
    let (mut min, mut max) = preview_bounds(p, view);
    let scale = if view == "pcb" {
        (880.0 / p.board.width).min(560.0 / p.board.height)
    } else {
        min = min.offset(Point::new(-4.0, -4.0));
        max = max.offset(Point::new(4.0, 4.0));
        (880.0 / (max.x - min.x).max(1.0)).min(560.0 / (max.y - min.y).max(1.0))
    };
    let to_pixel =
        |p: Point| Point::new(60.0 + (p.x - min.x) * scale, 60.0 + (p.y - min.y) * scale);
    if view == "pcb" {
        let w = (p.board.width * scale).min(880.0) as u32;
        let h = (p.board.height * scale).min(560.0) as u32;
        for x in 60..60 + w {
            for y in 60..60 + h {
                image.put_pixel(x, y, image::Rgb([25, 58, 54]));
            }
        }
        for c in &p.board.cutouts {
            disk(
                &mut image,
                to_pixel(c.center),
                c.radius * scale,
                [19, 28, 41],
            );
        }
        for z in &p.board.zones {
            let color = if z.layer == pcbcraft_doc::Layer::Front {
                [245, 141, 121]
            } else {
                [116, 167, 236]
            };
            let points: Vec<Point> = z.outline.iter().map(|pt| to_pixel(*pt)).collect();
            if z.filled {
                fill_polygon(&mut image, &points, color);
            } else {
                for w in points.windows(2) {
                    line(&mut image, w[0], w[1], 0.6, color);
                }
            }
            for cut in &z.cutouts {
                let points: Vec<Point> = cut.iter().map(|pt| to_pixel(*pt)).collect();
                fill_polygon(&mut image, &points, [25, 58, 54]);
            }
        }
        for t in &p.board.tracks {
            line(
                &mut image,
                to_pixel(t.start),
                to_pixel(t.end),
                t.width * scale / 2.0,
                if t.layer == pcbcraft_doc::Layer::Front {
                    [245, 141, 121]
                } else {
                    [116, 167, 236]
                },
            );
        }
        for v in &p.board.vias {
            let point = to_pixel(v.position);
            let radius = (v.diameter / 2.0 * scale).max(1.0);
            line(
                &mut image,
                Point::new(point.x - radius, point.y),
                Point::new(point.x + radius, point.y),
                radius * 2.0,
                [245, 141, 121],
            );
            if v.drill > 0.0 {
                let hole = (v.drill / 2.0 * scale).max(0.5);
                line(
                    &mut image,
                    Point::new(point.x - hole, point.y),
                    Point::new(point.x + hole, point.y),
                    hole * 2.0,
                    [19, 28, 41],
                );
            }
        }
        for g in p.graphics.iter().filter(|g| g.layer == "board") {
            for pair in g.outline().unwrap_or_default().windows(2) {
                if let (Some(a), Some(b)) = (pair.first(), pair.get(1)) {
                    line(&mut image, to_pixel(*a), to_pixel(*b), 1.0, [220, 220, 220]);
                }
            }
        }
        for f in &p.board.footprints {
            if let Some(library) = p
                .component(f.component)
                .and_then(|c| c.library_footprint.as_ref())
            {
                for g in &library.definition.graphics {
                    png_graphic(
                        &mut image,
                        g,
                        |point| to_pixel(f.position.offset(point.rotate(-f.rotation))),
                        scale,
                    );
                }
            }
            for pad in &f.pads {
                let points = pad
                    .outline_points()
                    .into_iter()
                    .map(|pos| to_pixel(f.position.offset(pos.rotate(-f.rotation))))
                    .collect::<Vec<_>>();
                polygon(&mut image, &points, [230, 194, 125]);
                let drill = pad
                    .drill_outline_points()
                    .into_iter()
                    .map(|pos| to_pixel(f.position.offset(pos.rotate(-f.rotation))))
                    .collect::<Vec<_>>();
                polygon(&mut image, &drill, [19, 28, 41]);
            }
        }
    } else {
        for w in &p.wires {
            if let Some(points) = p.wire_path(w) {
                for pair in points.windows(2) {
                    if let (Some(a), Some(b)) = (pair.first(), pair.get(1)) {
                        line(&mut image, to_pixel(*a), to_pixel(*b), 1.0, [90, 224, 188]);
                    }
                }
            }
        }
        for w in &p.drawn_wires {
            for pair in w.points.windows(2) {
                if let (Some(a), Some(b)) = (pair.first(), pair.get(1)) {
                    line(&mut image, to_pixel(*a), to_pixel(*b), 1.0, [90, 224, 188]);
                }
            }
        }
        for g in p.graphics.iter().filter(|g| g.layer == "schematic") {
            for pair in g.outline().unwrap_or_default().windows(2) {
                if let (Some(a), Some(b)) = (pair.first(), pair.get(1)) {
                    line(&mut image, to_pixel(*a), to_pixel(*b), 1.0, [90, 224, 188]);
                }
            }
        }
        for l in p.labels.iter().filter(|l| l.kind == "junction") {
            let point = to_pixel(l.position);
            line(
                &mut image,
                Point::new(point.x - 2.0, point.y),
                Point::new(point.x + 2.0, point.y),
                2.0,
                [90, 224, 188],
            );
        }
        for f in &p.no_connects {
            if let Some(anchor) = p.pin_position(&f.pin) {
                let point = to_pixel(anchor);
                line(
                    &mut image,
                    Point::new(point.x - 3.0, point.y - 3.0),
                    Point::new(point.x + 3.0, point.y + 3.0),
                    1.0,
                    [245, 141, 121],
                );
                line(
                    &mut image,
                    Point::new(point.x - 3.0, point.y + 3.0),
                    Point::new(point.x + 3.0, point.y - 3.0),
                    1.0,
                    [245, 141, 121],
                );
            }
        }
        for c in p.components.iter().flat_map(|c| c.schematic_views()) {
            if let Some(library) = &c.library_symbol {
                for g in library
                    .definition
                    .graphics
                    .iter()
                    .filter(|g| (g.unit == 0 || g.unit == library.unit) && g.body_style <= 1)
                {
                    png_graphic(
                        &mut image,
                        &g.primitive,
                        |point| to_pixel(c.position.offset(point.rotate(-c.rotation))),
                        scale,
                    );
                }
                for pin in library.definition.pins.iter().filter(|pin| {
                    (pin.unit == 0 && library.unit == 1 || pin.unit == library.unit)
                        && pin.body_style <= 1
                }) {
                    let angle = pin.angle.to_radians();
                    let end = pin.position.offset(Point::new(
                        pin.length * angle.cos(),
                        -pin.length * angle.sin(),
                    ));
                    line(
                        &mut image,
                        to_pixel(c.position.offset(pin.position.rotate(-c.rotation))),
                        to_pixel(c.position.offset(end.rotate(-c.rotation))),
                        0.7,
                        [219, 231, 242],
                    );
                }
                continue;
            }
            let outline = [
                Point::new(-3.0, -1.5),
                Point::new(3.0, -1.5),
                Point::new(3.0, 1.5),
                Point::new(-3.0, 1.5),
                Point::new(-3.0, -1.5),
            ];
            for pair in outline.windows(2) {
                if let (Some(a), Some(b)) = (pair.first(), pair.get(1)) {
                    line(
                        &mut image,
                        to_pixel(c.position.offset(a.rotate(-c.rotation))),
                        to_pixel(c.position.offset(b.rotate(-c.rotation))),
                        1.0,
                        [219, 231, 242],
                    );
                }
            }
            for pin in &c.pins {
                line(
                    &mut image,
                    to_pixel(c.position.offset(pin.offset.rotate(-c.rotation))),
                    to_pixel(
                        c.position.offset(
                            Point::new(pin.offset.x.signum() * 3.0, 0.0).rotate(-c.rotation),
                        ),
                    ),
                    1.0,
                    [219, 231, 242],
                );
            }
        }
    }
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(image)
        .write_to(&mut bytes, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(bytes.into_inner())
}
fn disk(image: &mut image::RgbImage, p: Point, r: f64, color: [u8; 3]) {
    let radius = r.clamp(0.7, 1200.0);
    let left = (p.x - radius).max(0.0) as u32;
    let right = (p.x + radius).min(999.0) as u32;
    let top = (p.y - radius).max(0.0) as u32;
    let bottom = (p.y + radius).min(679.0) as u32;
    for x in left..=right {
        for y in top..=bottom {
            if Point::new(x as f64, y as f64).distance(p) <= radius {
                image.put_pixel(x, y, image::Rgb(color));
            }
        }
    }
}
fn line(image: &mut image::RgbImage, a: Point, b: Point, r: f64, color: [u8; 3]) {
    // Pixel-space bounding scan clips hostile/out-of-view geometry without long stepping loops.
    let radius = r.clamp(0.7, 1200.0);
    let left = (a.x.min(b.x) - radius).max(0.0) as u32;
    let right = (a.x.max(b.x) + radius).min(999.0) as u32;
    let top = (a.y.min(b.y) - radius).max(0.0) as u32;
    let bottom = (a.y.max(b.y) + radius).min(679.0) as u32;
    for x in left..=right {
        for y in top..=bottom {
            if pcbcraft_geom::point_segment(Point::new(x as f64, y as f64), a, b) <= radius {
                image.put_pixel(x, y, image::Rgb(color));
            }
        }
    }
}

fn fill_polygon(image: &mut image::RgbImage, points: &[Point], color: [u8; 3]) {
    // Scanline fill in pixel space: pixel centres inside the polygon get the
    // zone colour. Hostile or off-view geometry is clipped to the image.
    if points.len() < 3 {
        return;
    }
    let top = points
        .iter()
        .map(|p| p.y)
        .fold(f64::INFINITY, f64::min)
        .max(0.0) as u32;
    let bottom = points
        .iter()
        .map(|p| p.y)
        .fold(f64::NEG_INFINITY, f64::max)
        .min(679.0) as u32;
    for y in top..=bottom {
        let row = f64::from(y) + 0.5;
        let mut crossings: Vec<f64> = vec![];
        for edge in points.windows(2) {
            let (a, b) = (edge[0], edge[1]);
            if (a.y > row) != (b.y > row) {
                crossings.push(a.x + (row - a.y) * (b.x - a.x) / (b.y - a.y));
            }
        }
        crossings.sort_by(f64::total_cmp);
        for pair in crossings.chunks(2) {
            if pair.len() < 2 {
                continue;
            }
            let left = pair[0].max(0.0) as u32;
            let right = (pair[1].min(999.0) as u32).min(999);
            for x in left..=right {
                image.put_pixel(x, y, image::Rgb(color));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn real_library_project() -> Result<Project, String> {
        let definition = pcbcraft_library::read_symbols(include_str!(
            "../../library/tests/fixtures/device.kicad_sym"
        ))
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|s| s.name == "R")
        .ok_or("fixture resistor missing")?;
        let footprint = pcbcraft_library::read_footprint(include_str!(
            "../../library/tests/fixtures/R_0805_2012Metric.kicad_mod"
        ))
        .map_err(|e| e.to_string())?;
        let pins = definition
            .pins
            .iter()
            .filter(|p| p.unit <= 1 && p.body_style <= 1)
            .map(|p| pcbcraft_doc::Pin {
                number: p.number.clone(),
                name: p.name.clone(),
                offset: p.position,
                net: None,
                electrical_type: p.electrical_type.clone(),
                direction: Some(p.angle),
                unit: p.unit,
            })
            .collect();
        let pads = footprint
            .pads
            .iter()
            .map(|p| pcbcraft_doc::Pad {
                number: p.number.clone(),
                offset: p.position,
                diameter: p.size.x.min(p.size.y),
                net: None,
                geometry: Some(p.clone()),
            })
            .collect();
        let mut project = Project::default();
        project.components.push(pcbcraft_doc::Component {
            id: 1,
            reference: "R1".into(),
            value: "1k".into(),
            kind: "library".into(),
            position: Point::new(20.0, 20.0),
            rotation: 0.0,
            pins,
            units: vec![],
            library_symbol: Some(pcbcraft_doc::LibrarySymbol {
                library: "Device".into(),
                unit: 1,
                definition,
            }),
            library_footprint: Some(pcbcraft_doc::LibraryFootprint {
                library: "Resistor_SMD".into(),
                definition: footprint,
            }),
            fields: vec![],
        });
        project.board.footprints.push(pcbcraft_doc::Footprint {
            id: 2,
            component: 1,
            position: Point::new(10.0, 10.0),
            rotation: 0.0,
            pads,
        });
        project.validate()?;
        Ok(project)
    }
    fn extract_footprint(board: &str) -> Result<&str, String> {
        let start = board.find("(footprint ").ok_or("no exported footprint")?;
        let mut depth = 0;
        let mut quote = false;
        let mut escape = false;
        for (offset, c) in board[start..].char_indices() {
            if escape {
                escape = false;
                continue;
            }
            if quote && c == '\\' {
                escape = true;
                continue;
            }
            if c == '"' {
                quote = !quote;
            }
            if quote {
                continue;
            }
            if c == '(' {
                depth += 1;
            } else if c == ')' {
                depth -= 1;
                if depth == 0 {
                    return Ok(&board[start..start + offset + 1]);
                }
            }
        }
        Err("unclosed exported footprint".into())
    }
    fn real_dip_project() -> Result<Project, String> {
        let mut project = real_library_project()?;
        let definition = pcbcraft_library::read_symbols(include_str!(
            "../../library/tests/fixtures/lm358.kicad_sym"
        ))
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|s| s.name == "LM358")
        .ok_or("fixture LM358 missing")?;
        let footprint = pcbcraft_library::read_footprint(include_str!(
            "../../library/tests/fixtures/DIP-8_W7.62mm.kicad_mod"
        ))
        .map_err(|e| e.to_string())?;
        let c = &mut project.components[0];
        c.reference = "U1".into();
        c.value = "LM358".into();
        c.pins = definition
            .pins
            .iter()
            .filter(|p| p.body_style <= 1)
            .map(|p| pcbcraft_doc::Pin {
                number: p.number.clone(),
                name: p.name.clone(),
                offset: p.position,
                net: None,
                electrical_type: p.electrical_type.clone(),
                direction: Some(p.angle),
                unit: p.unit,
            })
            .collect();
        c.library_symbol = Some(pcbcraft_doc::LibrarySymbol {
            library: "Amplifier_Operational".into(),
            unit: 1,
            definition,
        });
        c.units = vec![
            pcbcraft_doc::SymbolUnitPlacement {
                unit: 2,
                position: Point::new(40.0, 20.0),
                rotation: 90.0,
            },
            pcbcraft_doc::SymbolUnitPlacement {
                unit: 3,
                position: Point::new(60.0, 20.0),
                rotation: 0.0,
            },
        ];
        project.board.footprints[0].pads = footprint
            .pads
            .iter()
            .map(|p| pcbcraft_doc::Pad {
                number: p.number.clone(),
                offset: p.position,
                diameter: p.size.x.min(p.size.y),
                net: None,
                geometry: Some(p.clone()),
            })
            .collect();
        c.library_footprint = Some(pcbcraft_doc::LibraryFootprint {
            library: "Package_DIP".into(),
            definition: footprint,
        });
        project.validate()?;
        Ok(project)
    }
    #[test]
    fn through_hole_export_preserves_drills_layers_and_offset_slots() -> Result<(), String> {
        let mut project = real_dip_project()?;
        project.board.footprints[0].rotation = 90.0;
        // A slot with a local offset tests metadata beyond the circular DIP fixture.
        let slot = pcbcraft_library::Drill {
            size: Point::new(0.8, 0.5),
            offset: Point::new(0.1, 0.0),
        };
        let c = &mut project.components[0];
        let source = c.library_footprint.as_mut().ok_or("no footprint")?;
        source.definition.pads[0].drill = Some(slot.clone());
        project.board.footprints[0].pads[0].geometry = Some(source.definition.pads[0].clone());
        let board = kicad_board(&project)?;
        assert!(board.contains("(attr through_hole)"));
        assert!(board.contains("(drill oval 0.8 0.5 (offset 0.1 0))"));
        let exported = pcbcraft_library::read_footprint(extract_footprint(&board)?)
            .map_err(|e| e.to_string())?;
        assert_eq!(exported.pads.len(), 8);
        assert!(
            exported
                .pads
                .iter()
                .all(|p| p.pad_type == "thru_hole" && p.layers == vec!["*.Cu", "*.Mask"])
        );
        assert_eq!(exported.pads[0].drill, Some(slot));
        assert_eq!(exported.pads[0].rotation, 90.0);
        Ok(())
    }
    #[test]
    fn dip_reference_and_value_are_outside_actual_footprint_bounds() -> Result<(), String> {
        let mut project = real_dip_project()?;
        project.board.footprints[0].rotation = 90.0;
        let f = &project.board.footprints[0];
        let (min, max) = footprint_bounds(&project, f);
        let x = (min.x + max.x) / 2.0;
        let board = kicad_board(&project)?;
        assert!(board.contains(&format!(
            "(property \"Reference\" \"U1\" (at {x} {} 90)",
            min.y - 1.2
        )));
        assert!(board.contains(&format!(
            "(property \"Value\" \"LM358\" (at {x} {} 90)",
            max.y + 1.2
        )));
        assert!(
            max.y > 7.62,
            "DIP8 geometry extends beyond generic 2.5mm value location"
        );
        let exported = pcbcraft_library::read_footprint(extract_footprint(&board)?)
            .map_err(|e| e.to_string())?;
        let original = project.components[0]
            .library_footprint
            .as_ref()
            .ok_or("missing footprint")?;
        let mut graphics = original.definition.graphics.clone();
        for g in &mut graphics {
            if let pcbcraft_library::Graphic::Text { angle, .. } = g {
                *angle += 90.0;
            }
        }
        assert_eq!(
            exported.graphics, graphics,
            "property placement must not alter library artwork"
        );
        Ok(())
    }
    #[test]
    fn all_symbol_units_and_drilled_holes_appear_in_previews() -> Result<(), String> {
        let project = real_dip_project()?;
        let views = project.components[0].schematic_views();
        assert_eq!(views.len(), 3);
        let schematic = svg(&project, "schematic")?;
        assert!(schematic.contains("translate(40 20) rotate(-90)"));
        assert!(schematic.contains("translate(60 20)"));
        for view in views {
            assert!(schematic.contains(&format!("{} ·", escape(&view.reference))));
        }
        assert!(!preview_png(&project, "schematic")?.is_empty());
        let pcb = svg(&project, "pcb")?;
        assert_eq!(pcb.matches("data-drill=").count(), 8);
        let image = image::load_from_memory(&preview_png(&project, "pcb")?)
            .map_err(|e| e.to_string())?
            .to_rgb8();
        let scale = (880.0 / project.board.width).min(560.0 / project.board.height);
        let x = (60.0 + 10.0 * scale).round() as u32;
        let y = (60.0 + 10.0 * scale).round() as u32;
        assert_eq!(image.get_pixel(x, y).0, [19, 28, 41]);
        assert_eq!(
            image.get_pixel(x + (0.65 * scale) as u32, y).0,
            [230, 194, 125]
        );
        Ok(())
    }
    #[test]
    fn shared_physical_pin_is_rendered_only_on_primary_unit() -> Result<(), String> {
        let mut project = real_library_project()?;
        let mut definition = project.components[0]
            .library_symbol
            .as_ref()
            .ok_or("no symbol")?
            .definition
            .clone();
        definition.pins[1].unit = 2;
        let mut common = definition.pins[0].clone();
        common.number = "C".into();
        common.name = "Common".into();
        common.unit = 0;
        common.position = Point::new(-20.0, 0.0);
        common.angle = 0.0;
        definition.pins.push(common);
        project.components[0] = pcbcraft_doc::library_component(
            definition,
            "test".into(),
            1,
            "X1".into(),
            Point::new(20.0, 20.0),
            None,
        )?;
        project.board.footprints.clear();
        project.validate()?;
        let schematic = svg(&project, "schematic")?;
        assert_eq!(
            schematic.matches(">C</text>").count(),
            1,
            "unit0 pin is one physical pin in primary unit, not a ghost pin in every view"
        );
        Ok(())
    }
    #[test]
    fn actual_footprint_export_preserves_geometry_layers_and_rotation() -> Result<(), String> {
        let mut project = real_library_project()?;
        project.board.footprints[0].rotation = 90.0;
        let board = kicad_board(&project)?;
        assert!(!board.contains("(start -3.4 -1.4)"));
        assert!(board.contains("(35 \"F.Paste\" user)"));
        let exported = pcbcraft_library::read_footprint(extract_footprint(&board)?)
            .map_err(|e| e.to_string())?;
        assert_eq!(exported.name, "Resistor_SMD:R_0805_2012Metric");
        let original = project.components[0]
            .library_footprint
            .as_ref()
            .ok_or("no embedded footprint")?;
        assert_eq!(exported.pads.len(), original.definition.pads.len());
        for (actual, expected) in exported.pads.iter().zip(&original.definition.pads) {
            assert_eq!(actual.number, expected.number);
            assert_eq!(actual.shape, expected.shape);
            assert_eq!(actual.size, expected.size);
            assert_eq!(actual.position, expected.position);
            assert_eq!(actual.layers, expected.layers);
            assert_eq!(actual.roundrect_ratio, expected.roundrect_ratio);
            assert_eq!(actual.rotation, expected.rotation + 90.0);
        }
        let mut expected_graphics = original.definition.graphics.clone();
        for graphic in &mut expected_graphics {
            if let pcbcraft_library::Graphic::Text { angle, .. } = graphic {
                *angle += 90.0;
            }
        }
        assert_eq!(exported.graphics, expected_graphics);
        let native = serde_json::to_string(&project).map_err(|e| e.to_string())?;
        let restored: Project = serde_json::from_str(&native).map_err(|e| e.to_string())?;
        restored.validate()?;
        assert_eq!(restored, project);
        Ok(())
    }
    #[test]
    fn project_footprint_export_uses_embedded_name_without_external_nickname() -> Result<(), String>
    {
        let mut project = real_library_project()?;
        let footprint = project.components[0]
            .library_footprint
            .as_mut()
            .ok_or("missing footprint")?;
        footprint.library = "project".into();
        footprint.definition.name = "Edited0805".into();
        let board = kicad_board(&project)?;
        let exported = pcbcraft_library::read_footprint(extract_footprint(&board)?)
            .map_err(|e| e.to_string())?;
        assert_eq!(exported.name, "pcbcraft_Edited0805");
        assert!(!board.contains("project:Edited0805"));
        assert_eq!(exported.pads.len(), 2);
        Ok(())
    }
    #[test]
    fn drawn_wires_render_in_the_schematic_preview_without_components() -> Result<(), String> {
        let mut project = Project::default();
        project.drawn_wires.push(pcbcraft_doc::DrawnWire {
            id: 1,
            net: None,
            points: vec![
                Point::new(10.0, 10.0),
                Point::new(20.0, 10.0),
                Point::new(20.0, 20.0),
            ],
        });
        project.validate()?;
        let schematic = svg(&project, "schematic")?;
        assert!(
            schematic.contains("points=\"10,10 20,10 20,20\""),
            "drawn wire vertices must render in order"
        );
        Ok(())
    }
    #[test]
    fn rotated_board_text_carries_its_angle_in_the_pcb_preview() -> Result<(), String> {
        let mut project = Project::default();
        project.board.texts.push(pcbcraft_doc::BoardText {
            id: 1,
            text: "Acme Labs".into(),
            position: Point::new(10.0, 5.0),
            size: 2.0,
            layer: pcbcraft_doc::Layer::Front,
            rotation: 90.0,
        });
        project.board.texts.push(pcbcraft_doc::BoardText {
            id: 2,
            text: "Rev B".into(),
            position: Point::new(30.0, 5.0),
            size: 1.0,
            layer: pcbcraft_doc::Layer::Back,
            rotation: 0.0,
        });
        project.validate()?;
        let pcb = svg(&project, "pcb")?;
        assert!(
            pcb.contains("<text x=\"10\" y=\"5\" font-size=\"2\" fill=\"#dbe7f2\" transform=\"rotate(-90 10 5)\">Acme Labs</text>"),
            "a turned label must render with its angle about the anchor"
        );
        assert!(
            pcb.contains("<text x=\"30\" y=\"5\" font-size=\"1\" fill=\"#9ec5ff\">Rev B</text>"),
            "an upright label keeps the plain form"
        );
        Ok(())
    }
    #[test]
    fn zones_render_in_the_pcb_previews() -> Result<(), String> {
        let mut project = Project::default();
        project.nets.push(pcbcraft_doc::Net {
            id: 1,
            name: "GND".into(),
            ..Default::default()
        });
        project.board.zones.push(pcbcraft_doc::CopperZone {
            id: 2,
            net: 1,
            layer: pcbcraft_doc::Layer::Front,
            outline: vec![
                Point::new(5.0, 5.0),
                Point::new(45.0, 5.0),
                Point::new(45.0, 25.0),
                Point::new(5.0, 25.0),
                Point::new(5.0, 5.0),
            ],
            cutouts: vec![vec![
                Point::new(30.0, 15.0),
                Point::new(40.0, 15.0),
                Point::new(40.0, 20.0),
                Point::new(30.0, 20.0),
                Point::new(30.0, 15.0),
            ]],
            filled: true,
        });
        project.validate()?;
        let pcb = svg(&project, "pcb")?;
        assert!(
            pcb.contains(
                "<polygon points=\"5 5 45 5 45 25 5 25 5 5\" fill=\"#f58d79\" fill-opacity=\"0.3\""
            ),
            "the zone outline must render as a filled copper polygon"
        );
        assert!(
            pcb.contains("<polygon points=\"30 15 40 15 40 20 30 20 30 15\" fill=\"none\""),
            "the zone cutout must render as an unfilled dashed outline"
        );
        let image = image::load_from_memory(&preview_png(&project, "pcb")?)
            .map_err(|e| e.to_string())?
            .to_rgb8();
        assert_eq!(
            image.get_pixel(500, 200).0,
            [245, 141, 121],
            "inside the zone the copper colour must be painted"
        );
        assert_eq!(
            image.get_pixel(700, 400).0,
            [25, 58, 54],
            "inside the zone cutout the substrate colour must show through"
        );
        assert_eq!(
            image.get_pixel(100, 600).0,
            [25, 58, 54],
            "outside the zone the board keeps its substrate colour"
        );
        Ok(())
    }
    #[test]
    fn gerber_files_export_copper_outline_and_drills() -> Result<(), String> {
        let mut project = Project::default();
        project.nets.push(pcbcraft_doc::Net {
            id: 8,
            name: "GND".into(),
            ..Default::default()
        });
        project.board.tracks.push(pcbcraft_doc::Track {
            id: 3,
            net: 8,
            start: Point::new(10.0, 10.0),
            end: Point::new(30.0, 10.0),
            width: 0.3,
            layer: pcbcraft_doc::Layer::Back,
        });
        project.board.vias.push(pcbcraft_doc::Via {
            id: 4,
            net: 8,
            position: Point::new(30.0, 10.0),
            diameter: 0.6,
            drill: 0.3,
        });
        project.board.zones.push(pcbcraft_doc::CopperZone {
            id: 5,
            net: 8,
            layer: pcbcraft_doc::Layer::Front,
            outline: vec![
                Point::new(5.0, 5.0),
                Point::new(45.0, 5.0),
                Point::new(45.0, 25.0),
                Point::new(5.0, 25.0),
                Point::new(5.0, 5.0),
            ],
            cutouts: vec![],
            filled: true,
        });
        project.board.cutouts.push(pcbcraft_doc::Cutout {
            id: 6,
            center: Point::new(46.0, 20.0),
            radius: 2.0,
        });
        let through_pad = pcbcraft_doc::Pad {
            number: "1".into(),
            offset: Point::new(0.0, 0.0),
            diameter: 1.6,
            net: None,
            geometry: Some(pcbcraft_library::FootprintPad {
                number: "1".into(),
                pad_type: "thru_hole".into(),
                shape: "circle".into(),
                position: Point::new(0.0, 0.0),
                rotation: 0.0,
                size: Point::new(1.6, 1.6),
                layers: vec!["*.Cu".into(), "*.Mask".into()],
                drill: Some(pcbcraft_library::Drill {
                    size: Point::new(0.8, 0.8),
                    offset: Point::default(),
                }),
                roundrect_ratio: None,
                raw_source: String::new(),
            }),
        };
        let embedded_pad = through_pad.geometry.as_ref().ok_or("pad geometry")?.clone();
        let mut pad2 = through_pad.clone();
        pad2.number = "2".into();
        pad2.offset = Point::new(2.0, 0.0);
        if let Some(g) = pad2.geometry.as_mut() {
            g.number = "2".into();
            g.position = Point::new(2.0, 0.0);
        }
        let mut embedded_pad2 = embedded_pad.clone();
        embedded_pad2.number = "2".into();
        embedded_pad2.position = Point::new(2.0, 0.0);
        project.board.footprints.push(pcbcraft_doc::Footprint {
            id: 2,
            component: 1,
            position: Point::new(0.0, 0.0),
            rotation: 0.0,
            pads: vec![through_pad, pad2],
        });
        project.components.push(pcbcraft_doc::Component {
            id: 1,
            reference: "R1".into(),
            value: "1k".into(),
            kind: "resistor".into(),
            position: Point::new(20.0, 30.0),
            rotation: 0.0,
            pins: vec![
                pcbcraft_doc::Pin {
                    number: "1".into(),
                    name: String::new(),
                    offset: Point::new(-1.0, 0.0),
                    net: None,
                    electrical_type: "passive".into(),
                    direction: Some(180.0),
                    unit: 1,
                },
                pcbcraft_doc::Pin {
                    number: "2".into(),
                    name: String::new(),
                    offset: Point::new(1.0, 0.0),
                    net: None,
                    electrical_type: "passive".into(),
                    direction: Some(0.0),
                    unit: 1,
                },
            ],
            units: vec![],
            library_symbol: None,
            library_footprint: Some(pcbcraft_doc::LibraryFootprint {
                library: "ThroughHole".into(),
                definition: pcbcraft_library::FootprintDefinition {
                    name: "PAD-1.6".into(),
                    description: String::new(),
                    keywords: String::new(),
                    pads: vec![embedded_pad, embedded_pad2],
                    graphics: vec![],
                    raw_source: String::new(),
                    unsupported: vec![],
                },
            }),
            fields: vec![],
        });
        project.validate()?;
        let files = gerber_files(&project)?;
        assert_eq!(
            files.keys().collect::<Vec<_>>(),
            vec![
                "pcb-B.Cu.gbr",
                "pcb-Drill.drl",
                "pcb-Edge.gbr",
                "pcb-F.Cu.gbr"
            ]
        );
        let front = &files["pcb-F.Cu.gbr"];
        assert!(
            front.starts_with(
                "G04 front copper layer generated by PCBCraft *\n%FSLANX*XAYAMOMM*%\n"
            )
        );
        assert!(front.contains("%ADD10C,0.001000*%\n"), "zone fill aperture");
        assert!(
            front.contains("%ADD12C,0.600000*%\n"),
            "via copper aperture"
        );
        assert!(
            front.contains("%ADD11C,1.600000*%\n"),
            "through-hole pad aperture"
        );
        assert!(
            front.contains("D10D02*\nX5.000000Y5.000000D02*\nX45.000000Y5.000000D01*\n"),
            "the poured zone must appear as a filled region"
        );
        assert!(
            front.contains("D12X30.000000Y10.000000D03*\n"),
            "the via must flash on the front layer"
        );
        assert!(
            front.contains("D11X2.000000Y0.000000D03*\n"),
            "the plated pad must flash on the front layer"
        );
        assert!(front.ends_with("M02*\n"));
        let back = &files["pcb-B.Cu.gbr"];
        assert!(
            back.contains("%FSLANX*XDYDMOMM*%\n"),
            "back layer mirrors x"
        );
        assert!(
            back.contains("X-30.000000Y10.000000D01*\n"),
            "the back track end carries the mirrored coordinate"
        );
        assert!(
            back.contains("D12X-30.000000Y10.000000D03*\n"),
            "the via must flash on the back layer too"
        );
        let outline = &files["pcb-Edge.gbr"];
        assert!(
            outline.contains("X0.000000Y0.000000D02*\nX60.000000Y0.000000D01*\n"),
            "the board edge must be traced"
        );
        assert!(
            outline.contains("X44.000000Y20.000000D02*\nI2.000000J0.000000G02*\n"),
            "the circular cutout must trace a circle"
        );
        let drill = &files["pcb-Drill.drl"];
        assert!(drill.starts_with("M48\nFMAT,2\nMETRIC,TZ\n"));
        assert!(drill.contains("T1C0.300\n"), "via drill tool");
        assert!(drill.contains("T2C0.800\n"), "pad drill tool");
        assert!(drill.contains("T1\nX30.000Y10.000\n"), "via hole hit");
        assert!(drill.contains("T2\nX0.000Y0.000\n"), "pad hole hit");
        assert!(drill.ends_with("M30\n"));
        Ok(())
    }
    #[test]
    fn oval_drills_export_as_spanned_slots_following_pad_rotation() -> Result<(), String> {
        let mut project = Project::default();
        let oval_geometry = || pcbcraft_library::FootprintPad {
            number: "1".into(),
            pad_type: "thru_hole".into(),
            shape: "oval".into(),
            position: Point::new(0.0, 0.0),
            rotation: 0.0,
            size: Point::new(1.6, 1.6),
            layers: vec!["*.Cu".into(), "*.Mask".into()],
            drill: Some(pcbcraft_library::Drill {
                size: Point::new(0.8, 0.5),
                offset: Point::default(),
            }),
            roundrect_ratio: None,
            raw_source: String::new(),
        };
        let oval_pad = |component: u64| pcbcraft_doc::Footprint {
            id: 10 + component,
            component,
            position: if component == 1 {
                Point::new(10.0, 10.0)
            } else {
                Point::new(30.0, 10.0)
            },
            rotation: if component == 1 { 0.0 } else { 90.0 },
            pads: vec![pcbcraft_doc::Pad {
                number: "1".into(),
                offset: Point::new(0.0, 0.0),
                diameter: 1.6,
                net: None,
                geometry: Some(oval_geometry()),
            }],
        };
        for id in 1..=2u64 {
            project.components.push(pcbcraft_doc::Component {
                id,
                reference: format!("J{id}"),
                value: "slot".into(),
                kind: "connector".into(),
                position: Point::new(10.0, 30.0),
                rotation: 0.0,
                pins: vec![pcbcraft_doc::Pin {
                    number: "1".into(),
                    name: String::new(),
                    offset: Point::new(0.0, 0.0),
                    net: None,
                    electrical_type: "passive".into(),
                    direction: None,
                    unit: 1,
                }],
                units: vec![],
                library_symbol: None,
                library_footprint: Some(pcbcraft_doc::LibraryFootprint {
                    library: "Slots".into(),
                    definition: pcbcraft_library::FootprintDefinition {
                        name: "SLOT-1".into(),
                        description: String::new(),
                        keywords: String::new(),
                        pads: vec![oval_geometry()],
                        graphics: vec![],
                        raw_source: String::new(),
                        unsupported: vec![],
                    },
                }),
                fields: vec![],
            });
            project.board.footprints.push(oval_pad(id));
        }
        project.validate()?;
        let files = gerber_files(&project)?;
        let drill = &files["pcb-Drill.drl"];
        assert!(
            drill.contains("T1C0.500\n"),
            "one narrow tool carries both slots"
        );
        assert!(
            drill.contains("X9.850Y10.000G85X10.150Y10.000\n"),
            "an unrotated oval spans along x"
        );
        assert!(
            drill.contains("X30.000Y10.150G85X30.000Y9.850\n"),
            "a rotated footprint rotates its slot"
        );
        assert!(drill.ends_with("M30\n"));
        Ok(())
    }
    #[test]
    fn drill_origin_shifts_drill_coordinates_but_not_copper() -> Result<(), String> {
        let mut project = Project::default();
        project.nets.push(pcbcraft_doc::Net {
            id: 7,
            name: "SIG".into(),
            ..Default::default()
        });
        project.board.vias.push(pcbcraft_doc::Via {
            id: 9,
            net: 7,
            position: Point::new(30.0, 20.0),
            diameter: 0.7,
            drill: 0.3,
        });
        project.board.drill_origin = Point::new(20.0, 10.0);
        project.validate()?;
        let files = gerber_files(&project)?;
        let drill = &files["pcb-Drill.drl"];
        assert!(
            drill.contains("X10.000Y10.000\n"),
            "drill coordinates are measured from the recorded origin"
        );
        assert!(
            !drill.contains("X30.000Y20.000"),
            "absolute coordinates are gone once an origin is recorded"
        );
        let copper = &files["pcb-F.Cu.gbr"];
        assert!(
            copper.contains("X30.000000Y20.000000") || copper.contains("X30.000Y20.000"),
            "copper layers keep absolute coordinates"
        );
        project.board.drill_origin = Point::new(-5.0, 0.0);
        let files = gerber_files(&project)?;
        assert!(
            files["pcb-Drill.drl"].contains("X35.000Y20.000\n"),
            "a negative origin shifts the other way"
        );
        Ok(())
    }
    #[test]
    fn dxf_board_draws_tracks_vias_arcs_and_labels_as_entities() -> Result<(), String> {
        let mut project = Project::default();
        project.nets.push(pcbcraft_doc::Net {
            id: 7,
            name: "SIG".into(),
            ..Default::default()
        });
        project.board.tracks.push(pcbcraft_doc::Track {
            id: 10,
            net: 7,
            start: Point::new(10.0, 10.0),
            end: Point::new(30.0, 10.0),
            width: 0.3,
            layer: pcbcraft_doc::Layer::Front,
        });
        project.board.vias.push(pcbcraft_doc::Via {
            id: 11,
            net: 7,
            position: Point::new(30.0, 20.0),
            diameter: 0.7,
            drill: 0.3,
        });
        project.graphics.push(pcbcraft_doc::Graphic {
            id: 12,
            layer: "board".into(),
            shape: "arc".into(),
            points: vec![
                Point::new(40.0, 10.0),
                Point::new(47.071, 17.071),
                Point::new(54.0, 10.0),
            ],
            width: 0.2,
            text: String::new(),
        });
        project.graphics.push(pcbcraft_doc::Graphic {
            id: 13,
            layer: "schematic".into(),
            shape: "line".into(),
            points: vec![Point::new(0.0, 0.0), Point::new(1.0, 1.0)],
            width: 0.2,
            text: String::new(),
        });
        project.board.texts.push(pcbcraft_doc::BoardText {
            id: 14,
            text: "TOP".into(),
            position: Point::new(5.0, 5.0),
            size: 1.5,
            layer: pcbcraft_doc::Layer::Front,
            rotation: 90.0,
        });
        project.validate()?;
        let dxf = dxf_board(&project)?;
        assert!(
            dxf.starts_with("0\nSECTION\n2\nHEADER\n"),
            "a DXF file starts with a header section"
        );
        assert!(dxf.contains("$INSUNITS\n70\n4\n"), "millimetre units");
        assert!(
            dxf.ends_with("0\nENDSEC\n0\nEOF\n"),
            "a DXF file ends at EOF"
        );
        assert!(
            dxf.contains("8\nEdge\n0\nLWPOLYLINE\n90\n4\n70\n1\n"),
            "the board edge is a closed four-vertex polyline"
        );
        assert!(
            dxf.contains("8\nF_Cu\n0\nLINE\n10\n10.000\n20\n10.000\n11\n30.000\n21\n10.000\n"),
            "the front track becomes a line on the front copper layer"
        );
        assert!(
            dxf.contains("8\nVia\n0\nCIRCLE\n10\n30.000\n20\n20.000\n40\n0.350\n"),
            "the via pad becomes a circle"
        );
        assert!(
            dxf.contains("8\nDrill\n0\nCIRCLE\n10\n30.000\n20\n20.000\n40\n0.150\n"),
            "the via drill becomes its own circle"
        );
        assert!(
            dxf.contains("8\nBoard_Graphics\n0\nARC\n10\n47.000\n20\n10.071\n40\n7.000\n50\n359.419\n51\n180.581\n"),
            "a three-point board arc becomes a centre-radius arc"
        );
        assert!(
            !dxf.contains("8\nBoard_Graphics\n0\nLINE\n"),
            "schematic graphics stay out of the board drawing"
        );
        assert!(
            dxf.contains("8\nF_Silk\n0\nTEXT\n10\n5.000\n20\n5.000\n40\n1.500\n50\n90.0\n1\nTOP\n"),
            "board labels become rotated text entities"
        );
        assert_eq!(
            dxf.matches("\n0\nENTITY\n").count(),
            6,
            "six entities in all"
        );
        Ok(())
    }
    #[test]
    fn bom_csv_groups_identical_parts_and_quotes_tricky_cells() -> Result<(), String> {
        let mut project = Project::default();
        for (id, reference, value) in [
            (1, "R3", "10k"),
            (2, "R1", "10k"),
            (3, "R2", "10,000"),
            (4, "C1", "100n"),
        ] {
            project.components.push(pcbcraft_doc::Component {
                id,
                reference: reference.into(),
                value: value.into(),
                kind: if value == "100n" {
                    "capacitor"
                } else {
                    "resistor"
                }
                .into(),
                position: Point::new(10.0, 10.0),
                rotation: 0.0,
                pins: vec![
                    pcbcraft_doc::Pin {
                        number: "1".into(),
                        name: String::new(),
                        offset: Point::new(-1.0, 0.0),
                        net: None,
                        electrical_type: "passive".into(),
                        direction: None,
                        unit: 1,
                    },
                    pcbcraft_doc::Pin {
                        number: "2".into(),
                        name: String::new(),
                        offset: Point::new(1.0, 0.0),
                        net: None,
                        electrical_type: "passive".into(),
                        direction: None,
                        unit: 1,
                    },
                ],
                units: vec![],
                library_symbol: None,
                library_footprint: None,
                fields: vec![pcbcraft_doc::ComponentField {
                    key: "Tolerance".into(),
                    value: if reference == "C1" {
                        "10%".into()
                    } else {
                        "".into()
                    },
                }],
            });
        }
        project.validate()?;
        let fields = ["Tolerance".to_string()];
        let text = bom_csv(&project, &fields)?;
        assert_eq!(
            text.lines().next(),
            Some("quantity,references,value,kind,footprint,Tolerance"),
            "requested properties become extra columns"
        );
        assert_eq!(
            text.lines().nth(1),
            Some("1,R2,\"10,000\",resistor,starter pads,"),
            "rows sort by value, kind and footprint"
        );
        assert_eq!(
            text.lines().nth(2),
            Some("1,C1,100n,capacitor,starter pads,10%"),
            "the property column follows the requested key"
        );
        assert_eq!(
            text.lines().nth(3),
            Some("2,\"R1,R3\",10k,resistor,starter pads,"),
            "identical parts collapse into one sorted reference list"
        );
        assert_eq!(
            bom_csv(&project, &[])?.lines().count(),
            4,
            "no extra columns, same three groups"
        );
        assert!(bom_csv(&project, &["".to_string()]).is_err(), "empty key");
        assert!(
            bom_csv(&project, &["x".repeat(65)]).is_err(),
            "overlong key"
        );
        assert!(
            bom_csv(
                &project,
                &(1..=17).map(|i| format!("f{i}")).collect::<Vec<String>>()
            )
            .is_err(),
            "too many properties"
        );
        Ok(())
    }
    #[test]
    fn netlist_text_lists_every_net_with_sorted_component_pins() -> Result<(), String> {
        let mut project = Project::default();
        for (id, name) in [(7, "UNUSED"), (8, "SIG"), (9, "GND")] {
            project.nets.push(pcbcraft_doc::Net {
                id,
                name: name.into(),
                ..Default::default()
            });
        }
        for (id, reference, p1, p2) in [(1, "R2", 9u64, 8), (2, "R1", 9, 9)] {
            project.components.push(pcbcraft_doc::Component {
                id,
                reference: reference.into(),
                value: "1k".into(),
                kind: "resistor".into(),
                position: Point::new(10.0, 10.0),
                rotation: 0.0,
                pins: vec![
                    pcbcraft_doc::Pin {
                        number: "1".into(),
                        name: String::new(),
                        offset: Point::new(-1.0, 0.0),
                        net: Some(p1),
                        electrical_type: "passive".into(),
                        direction: None,
                        unit: 1,
                    },
                    pcbcraft_doc::Pin {
                        number: "2".into(),
                        name: String::new(),
                        offset: Point::new(1.0, 0.0),
                        net: if reference == "R1" { None } else { Some(p2) },
                        electrical_type: "passive".into(),
                        direction: None,
                        unit: 1,
                    },
                ],
                units: vec![],
                library_symbol: None,
                library_footprint: None,
                fields: vec![],
            });
        }
        project.validate()?;
        let text = netlist_text(&project)?;
        assert!(
            text.starts_with("# PCBCraft netlist\n"),
            "a parseable header"
        );
        assert!(
            text.contains("# nets: 2\n"),
            "nets without pins are not listed"
        );
        assert!(
            text.contains("GND R1.1 R2.1\n"),
            "pins sort by reference within the net"
        );
        assert!(text.contains("SIG R2.2\n"), "each net gets its own line");
        assert!(!text.contains("R1.2"), "a pin with no net is left out");
        Ok(())
    }
    #[test]
    fn vias_render_pad_and_drill_in_the_pcb_preview() -> Result<(), String> {
        let mut project = Project::default();
        project.nets.push(pcbcraft_doc::Net {
            id: 1,
            name: "GND".into(),
            ..Default::default()
        });
        project.board.vias.push(pcbcraft_doc::Via {
            id: 2,
            net: 1,
            position: Point::new(30.0, 20.0),
            diameter: 1.4,
            drill: 0.8,
        });
        project.validate()?;
        let pcb = svg(&project, "pcb")?;
        assert!(
            pcb.contains("<circle cx=\"30\" cy=\"20\" r=\"0.7\""),
            "the via pad must render at half its diameter"
        );
        assert!(
            pcb.contains("<circle cx=\"30\" cy=\"20\" r=\"0.4\" fill=\"#131c29\""),
            "the drilled hole must render as board interior"
        );
        Ok(())
    }
    #[test]
    fn labels_and_no_connect_flags_render_in_the_schematic_preview() -> Result<(), String> {
        let mut project = Project::default();
        project.nets.push(pcbcraft_doc::Net {
            id: 1,
            name: "VCC".into(),
            ..Default::default()
        });
        project.components.push(pcbcraft_doc::Component {
            id: 3,
            reference: "R9".into(),
            value: "1 k".into(),
            kind: "resistor".into(),
            position: Point::new(50.0, 30.0),
            rotation: 0.0,
            pins: vec![
                pcbcraft_doc::Pin {
                    number: "1".into(),
                    name: String::new(),
                    offset: Point::new(-1.0, 0.0),
                    net: None,
                    electrical_type: "passive".into(),
                    direction: Some(180.0),
                    unit: 1,
                },
                pcbcraft_doc::Pin {
                    number: "2".into(),
                    name: String::new(),
                    offset: Point::new(1.0, 0.0),
                    net: None,
                    electrical_type: "passive".into(),
                    direction: Some(0.0),
                    unit: 1,
                },
            ],
            units: vec![],
            library_symbol: None,
            library_footprint: None,
            fields: vec![],
        });
        project.drawn_wires.push(pcbcraft_doc::DrawnWire {
            id: 2,
            net: Some(1),
            points: vec![Point::new(10.0, 5.0), Point::new(20.0, 5.0)],
        });
        project.labels.push(pcbcraft_doc::Label {
            id: 4,
            kind: "global".into(),
            net: 1,
            position: Point::new(10.0, 5.0),
            rotation: 0.0,
        });
        project.labels.push(pcbcraft_doc::Label {
            id: 5,
            kind: "junction".into(),
            net: 1,
            position: Point::new(20.0, 5.0),
            rotation: 0.0,
        });
        project.no_connects.push(pcbcraft_doc::NoConnectFlag {
            id: 6,
            pin: pcbcraft_doc::PinRef {
                component: 3,
                pin: "2".into(),
            },
        });
        project.validate()?;
        let schematic = svg(&project, "schematic")?;
        assert!(
            schematic.contains(">{VCC}</text>"),
            "a global label must render its braced net name"
        );
        assert!(
            schematic.contains("<circle cx=\"20\" cy=\"5\" r=\"0.35\""),
            "a junction must render as a dot on its wire vertex"
        );
        assert!(
            schematic.contains("stroke=\"#f58d79\""),
            "a no-connect flag must render its cross at the pin"
        );
        Ok(())
    }
    #[test]
    fn graphics_render_outlines_and_text_in_previews() -> Result<(), String> {
        let mut project = Project::default();
        project.graphics.push(pcbcraft_doc::Graphic {
            id: 1,
            layer: "schematic".into(),
            shape: "rect".into(),
            points: vec![Point::new(10.0, 10.0), Point::new(20.0, 15.0)],
            width: 0.2,
            text: String::new(),
        });
        project.graphics.push(pcbcraft_doc::Graphic {
            id: 2,
            layer: "schematic".into(),
            shape: "text".into(),
            points: vec![Point::new(12.0, 12.0)],
            width: 1.5,
            text: "REV B".into(),
        });
        project.graphics.push(pcbcraft_doc::Graphic {
            id: 3,
            layer: "board".into(),
            shape: "circle".into(),
            points: vec![Point::new(30.0, 20.0)],
            width: 4.0,
            text: String::new(),
        });
        project.validate()?;
        let schematic = svg(&project, "schematic")?;
        assert!(
            schematic.contains("points=\"10,10 20,10 20,15 10,15 10,10\""),
            "rect graphic must render as a closed outline"
        );
        assert!(
            schematic.contains(">REV B</text>"),
            "text graphic must render its content"
        );
        assert!(
            !schematic.contains("30,20"),
            "board graphics must not leak into the schematic preview"
        );
        let pcb = svg(&project, "pcb")?;
        let circle = pcb
            .match_indices("<polyline points=\"34")
            .next()
            .map(|(i, _)| i)
            .ok_or("circle graphic missing from the pcb preview")?;
        assert!(
            pcb[circle..].contains("30,24"),
            "circle must pass through its topmost point"
        );
        Ok(())
    }
    #[test]
    fn previews_use_actual_symbol_and_non_circular_copper() -> Result<(), String> {
        let project = real_library_project()?;
        let schematic = svg(&project, "schematic")?;
        assert!(!schematic.contains("width=\"6\" height=\"3\""));
        assert!(schematic.contains("<polyline"));
        let pcb = svg(&project, "pcb")?;
        assert!(pcb.contains("#75928b"));
        assert!(pcb.contains("#e6c27d"));
        assert!(!pcb.contains("${REFERENCE}"));
        assert!(pcb.contains(">R1</text>"));
        let image = image::load_from_memory(&preview_png(&project, "pcb")?)
            .map_err(|e| e.to_string())?
            .to_rgb8();
        let scale = (880.0 / project.board.width).min(560.0 / project.board.height);
        let pad = &project.board.footprints[0].pads[0];
        let x = (60.0 + (10.0 + pad.offset.x) * scale).round() as u32;
        let y = (60.0 + (10.0 + 0.6) * scale).floor() as u32;
        // At y=0.6 mm the real 1.4-mm-tall pad still has copper; a generic
        // 1.025-mm-diameter circular replacement would miss this location.
        assert_eq!(image.get_pixel(x, y).0, [230, 194, 125]);
        assert!(!preview_png(&project, "schematic")?.is_empty());
        Ok(())
    }
    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    #[ignore = "requires installed kicad-cli"]
    fn kicad_cli_reads_rotated_real_library_export() -> Result<(), String> {
        for mut project in [real_library_project()?, real_dip_project()?] {
            project.board.footprints[0].rotation = 90.0;
            let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
            let board = directory.path().join("library-board.kicad_pcb");
            std::fs::write(&board, kicad_board(&project)?).map_err(|e| e.to_string())?;
            let output = std::process::Command::new("kicad-cli")
                .args([
                    "pcb",
                    "export",
                    "svg",
                    "--mode-single",
                    "--layers",
                    "F.Cu,F.SilkS,F.Fab,F.CrtYd",
                    "--output",
                ])
                .arg(directory.path().join("library-board.svg"))
                .arg(&board)
                .output()
                .map_err(|e| e.to_string())?;
            if !output.status.success() {
                return Err(format!(
                    "KiCad rejected export: {} {}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                ));
            }
            assert!(
                std::fs::read_dir(directory.path())
                    .map_err(|e| e.to_string())?
                    .filter_map(Result::ok)
                    .any(|entry| entry.path().extension().and_then(|s| s.to_str()) == Some("svg"))
            );
            let output = std::process::Command::new("kicad-cli")
                .args(["pcb", "export", "drill", "--format", "excellon", "--output"])
                .arg(directory.path())
                .arg(&board)
                .output()
                .map_err(|e| e.to_string())?;
            if !output.status.success() {
                return Err(format!(
                    "KiCad rejected drill export: {}",
                    String::from_utf8_lossy(&output.stderr)
                ));
            }
            if project.board.footprints[0].pads.iter().any(|p| {
                p.geometry
                    .as_ref()
                    .is_some_and(|g| g.pad_type == "thru_hole")
            }) {
                let mut positions = 0;
                for entry in std::fs::read_dir(directory.path()).map_err(|e| e.to_string())? {
                    let path = entry.map_err(|e| e.to_string())?.path();
                    if path.extension().and_then(|s| s.to_str()) == Some("drl") {
                        let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
                        positions += text
                            .lines()
                            .filter(|line| line.starts_with('X') && line.contains('Y'))
                            .count();
                    }
                }
                assert_eq!(
                    positions, 8,
                    "DIP8 must export all eight plated drill holes"
                );
            }
        }
        Ok(())
    }
    #[test]
    fn atomic_save_replaces_existing_project_and_load_validates() -> Result<(), String> {
        let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
        let path = directory.path().join("design.pcbcraft");
        let mut p = Project::default();
        save(&p, &path)?;
        p.name = "Changed".into();
        save(&p, &path)?;
        assert_eq!(load(&path)?, p);
        atomic_write(&path, b"{\"format\":\"bad\"}")?;
        assert!(load(&path).is_err());
        Ok(())
    }
    #[test]
    fn document_roundtrip_and_unsupported_format() {
        let p = Project::default();
        let text = serde_json::to_string(&p);
        assert!(text.is_ok());
        if let Ok(text) = text {
            let loaded = serde_json::from_str::<Project>(&text);
            assert!(loaded.is_ok_and(|v| v == p));
        }
        let mut bad = p;
        bad.version = 999;
        assert!(bad.validate().is_err());
    }
}
