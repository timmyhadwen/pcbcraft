//! Native project persistence and original, limited KiCad PCB / preview writers.
use pcbcraft_doc::Project;
use pcbcraft_geom::Point;
use std::collections::{BTreeMap, BTreeSet};
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
pub fn read_bytes(path: &Path) -> Result<Vec<u8>, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err("file exceeds 8 MiB".into());
    }
    Ok(bytes)
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
        "bus" => l.text.clone(),
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

/// CRC-32 (IEEE, reflected polynomial) over a byte slice, for ZIP entries.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for byte in bytes {
        crc = (crc >> 8) ^ CRC_TABLE[((crc as u8) ^ byte) as usize];
    }
    !crc
}
const CRC_TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut crc = i as u32;
        let mut bit = 0;
        while bit < 8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xedb8_8320
            } else {
                crc >> 1
            };
            bit += 1;
        }
        table[i] = crc;
        i += 1;
    }
    table
};

/// A self-contained project archive: a ZIP file (stored, no compression)
/// holding design.json plus one entry per attached text file, written
/// without any compression dependency.
pub fn archive_zip(p: &Project) -> Result<Vec<u8>, String> {
    let design = serde_json::to_string_pretty(p).map_err(|e| e.to_string())?;
    let mut entries: Vec<(String, Vec<u8>)> = vec![("design.json".into(), design.into_bytes())];
    for file in &p.files {
        entries.push((
            format!("files/{}", file.name),
            file.data.clone().into_bytes(),
        ));
    }
    let mut zip: Vec<u8> = Vec::new();
    let mut central: Vec<u8> = Vec::new();
    let mut offset = 0u32;
    for (name, data) in &entries {
        if name.len() > 255 {
            return Err("archive entry name is too long".into());
        }
        let crc = crc32(data);
        let mut local = Vec::new();
        local.extend_from_slice(b"PK\x03\x04");
        local.extend_from_slice(&[20, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        local.extend_from_slice(&crc.to_le_bytes());
        local.extend_from_slice(&(data.len() as u32).to_le_bytes());
        local.extend_from_slice(&(data.len() as u32).to_le_bytes());
        local.extend_from_slice(&(name.len() as u16).to_le_bytes());
        local.extend_from_slice(&0u16.to_le_bytes());
        local.extend_from_slice(name.as_bytes());
        local.extend_from_slice(data);
        let mut dir = Vec::new();
        dir.extend_from_slice(b"PK\x01\x02");
        dir.extend_from_slice(&[20, 0, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        dir.extend_from_slice(&crc.to_le_bytes());
        dir.extend_from_slice(&(data.len() as u32).to_le_bytes());
        dir.extend_from_slice(&(data.len() as u32).to_le_bytes());
        dir.extend_from_slice(&(name.len() as u16).to_le_bytes());
        dir.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        dir.extend_from_slice(&offset.to_le_bytes());
        dir.extend_from_slice(name.as_bytes());
        zip.extend_from_slice(&local);
        central.extend_from_slice(&dir);
        offset += local.len() as u32;
    }
    let directory_start = zip.len() as u32;
    zip.extend_from_slice(&central);
    zip.extend_from_slice(b"PK\x05\x06");
    zip.extend_from_slice(&[0, 0, 0, 0]);
    zip.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    zip.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    zip.extend_from_slice(&(central.len() as u32).to_le_bytes());
    zip.extend_from_slice(&directory_start.to_le_bytes());
    zip.extend_from_slice(&0u16.to_le_bytes());
    Ok(zip)
}

/// Read back the project stored in an archive written by `archive_zip`.
pub fn archive_project(bytes: &[u8]) -> Result<Project, String> {
    let eocd = bytes
        .windows(22)
        .rposition(|w| w.starts_with(b"PK\x05\x06"))
        .ok_or_else(|| "not a project archive: no archive directory".to_string())?;
    let read_u16 = |at: usize| -> u16 { u16::from_le_bytes([bytes[at], bytes[at + 1]]) };
    let read_u32 = |at: usize| -> u32 {
        u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
    };
    let entries = read_u16(eocd + 10) as usize;
    let mut at = read_u32(eocd + 16) as usize;
    let mut design: Option<&[u8]> = None;
    for _ in 0..entries {
        if !bytes[at..].starts_with(b"PK\x01\x02") || at + 46 > bytes.len() {
            return Err("corrupt archive directory entry".into());
        }
        let size = read_u32(at + 20) as usize;
        let name_len = read_u16(at + 28) as usize;
        let extra_len = read_u16(at + 30) as usize;
        let comment_len = read_u16(at + 32) as usize;
        let local_at = read_u32(at + 42) as usize;
        if name_len == 0 || at + 46 + name_len > bytes.len() {
            return Err("corrupt archive directory entry".into());
        }
        let name = &bytes[at + 46..at + 46 + name_len];
        if name == b"design.json" {
            if local_at + 30 > bytes.len() || !bytes[local_at..].starts_with(b"PK\x03\x04") {
                return Err("corrupt archive entry header".into());
            }
            let local_name = read_u16(local_at + 26) as usize;
            let local_extra = read_u16(local_at + 28) as usize;
            if local_at + 30 + local_name > bytes.len()
                || &bytes[local_at + 30..local_at + 30 + local_name] != name
            {
                return Err("archive entry headers disagree about the name".into());
            }
            let data_at = local_at + 30 + local_name + local_extra;
            if data_at + size > bytes.len() {
                return Err("archive entry runs past the end of the file".into());
            }
            design = Some(&bytes[data_at..data_at + size]);
        }
        at += 46 + name_len + extra_len + comment_len;
    }
    let design = design.ok_or_else(|| "archive holds no design.json".to_string())?;
    let text = std::str::from_utf8(design).map_err(|_| "design.json is not UTF-8".to_string())?;
    let project: Project =
        serde_json::from_str(text).map_err(|e| format!("design.json is invalid: {e}"))?;
    project
        .validate()
        .map_err(|e| format!("archived design is invalid: {e}"))?;
    Ok(project)
}

/// A small vector PDF writer: the drawing is collected as path and text
/// operations in page space (PostScript points, origin bottom left) and
/// assembled into a one-page document without any external dependency.
struct PdfPage {
    ops: String,
    count: usize,
    scale: f64,
    origin_x: f64,
    origin_y: f64,
}
const PDF_MAX_OPS: usize = 20_000;
const PDF_K: f64 = 72.0 / 25.4;

impl PdfPage {
    fn new() -> Self {
        PdfPage {
            ops: String::new(),
            count: 0,
            scale: 1.0,
            origin_x: 0.0,
            origin_y: 0.0,
        }
    }
    fn fit(&mut self, min: Point, max: Point) {
        let design_w = (max.x - min.x).max(1.0);
        let design_h = (max.y - min.y).max(1.0);
        let margin = 28.0;
        let fit_w = (842.0 - 2.0 * margin) / (design_w * PDF_K);
        let fit_h = (595.0 - 2.0 * margin) / (design_h * PDF_K);
        self.scale = fit_w.min(fit_h).clamp(0.02, 4.0);
        let draw_w = design_w * PDF_K * self.scale;
        let draw_h = design_h * PDF_K * self.scale;
        self.origin_x = (842.0 - draw_w) / 2.0 - min.x * PDF_K * self.scale;
        self.origin_y = (595.0 + draw_h) / 2.0 + min.y * PDF_K * self.scale;
    }
    fn at(&self, p: Point) -> (f64, f64) {
        (
            self.origin_x + p.x * PDF_K * self.scale,
            self.origin_y - p.y * PDF_K * self.scale,
        )
    }
    fn n(v: f64) -> String {
        format!("{:.3}", v + 0.0)
    }
    fn escape(s: &str) -> String {
        let ascii: String = s
            .chars()
            .map(|c| {
                if c.is_ascii_graphic() || c == ' ' {
                    c
                } else {
                    '?'
                }
            })
            .collect();
        ascii
            .replace('\\', r"\\")
            .replace('(', r"\(")
            .replace(')', r"\)")
    }
    fn op(&mut self, text: &str) -> Result<(), String> {
        self.count += 1;
        if self.count > PDF_MAX_OPS {
            return Err("the drawing is too complex for one PDF page".into());
        }
        self.ops.push_str(text);
        self.ops.push('\n');
        Ok(())
    }
    fn color(&mut self, rgb: [f64; 3], stroke: bool) -> Result<(), String> {
        let [r, g, b] = rgb;
        self.op(&format!(
            "{} {} {} {}",
            Self::n(r),
            Self::n(g),
            Self::n(b),
            if stroke { "RG" } else { "rg" }
        ))
    }
    fn width(&mut self, w: f64) -> Result<(), String> {
        self.op(&format!("{} w", Self::n((w * PDF_K * self.scale).max(0.2))))
    }
    /// Trace a polyline subpath; `close` connects the last point to the first.
    fn subpath(&mut self, points: &[Point], close: bool) -> Result<(), String> {
        if points.len() < 2 {
            return Ok(());
        }
        let (x, y) = self.at(points[0]);
        self.op(&format!("{} {} m", Self::n(x), Self::n(y)))?;
        for p in &points[1..] {
            let (x, y) = self.at(*p);
            self.op(&format!("{} {} l", Self::n(x), Self::n(y)))?;
        }
        if close {
            self.op("h")?;
        }
        Ok(())
    }
    fn path(&mut self, points: &[Point], rgb: [f64; 3], w: f64, fill: bool) -> Result<(), String> {
        if points.len() < 2 {
            return Ok(());
        }
        self.color(rgb, true)?;
        self.color(rgb, false)?;
        self.width(w)?;
        self.subpath(points, true)?;
        self.op(if fill { "B" } else { "S" })
    }
    /// A circle approximated by four cubic Bézier quarters.
    fn circle(
        &mut self,
        center: Point,
        radius: f64,
        rgb: [f64; 3],
        w: f64,
        fill: bool,
    ) -> Result<(), String> {
        const C: f64 = 0.552_284_749_8;
        self.color(rgb, true)?;
        self.color(rgb, false)?;
        self.width(w)?;
        let (x, y) = self.at(Point::new(center.x - radius, center.y));
        self.op(&format!("{} {} m", Self::n(x), Self::n(y)))?;
        let k = radius * C;
        let arcs = [
            (
                Point::new(center.x - radius, center.y + k),
                Point::new(center.x - k, center.y + radius),
                Point::new(center.x, center.y + radius),
            ),
            (
                Point::new(center.x + k, center.y + radius),
                Point::new(center.x + radius, center.y + k),
                Point::new(center.x + radius, center.y),
            ),
            (
                Point::new(center.x + radius, center.y - k),
                Point::new(center.x + k, center.y - radius),
                Point::new(center.x, center.y - radius),
            ),
            (
                Point::new(center.x - k, center.y - radius),
                Point::new(center.x - radius, center.y - k),
                Point::new(center.x - radius, center.y),
            ),
        ];
        for (c1, c2, end) in arcs {
            let (x1, y1) = self.at(c1);
            let (x2, y2) = self.at(c2);
            let (x3, y3) = self.at(end);
            self.op(&format!(
                "{} {} {} {} {} {} c",
                Self::n(x1),
                Self::n(y1),
                Self::n(x2),
                Self::n(y2),
                Self::n(x3),
                Self::n(y3)
            ))?;
        }
        self.op("h")?;
        self.op(if fill { "B" } else { "S" })
    }
    fn text(&mut self, at: Point, size: f64, rgb: [f64; 3], value: &str) -> Result<(), String> {
        if value.is_empty() {
            return Ok(());
        }
        let (x, y) = self.at(at);
        self.color(rgb, false)?;
        self.op(&format!(
            "BT /F1 {} Tf {} {} Td ({}) Tj ET",
            Self::n((size * PDF_K * self.scale).clamp(4.0, 72.0)),
            Self::n(x),
            Self::n(y),
            Self::escape(value),
        ))
    }
}

/// Draw one library graphic at a position.
fn pdf_graphic(
    page: &mut PdfPage,
    g: &pcbcraft_library::Graphic,
    at: Point,
    rotation: f64,
) -> Result<(), String> {
    let spin = |p: Point| at.offset(p.rotate(-rotation));
    if let pcbcraft_library::Graphic::Text {
        position,
        size,
        text,
        ..
    } = g
    {
        return page.text(spin(*position), size.y, [0.0, 0.0, 0.0], text);
    }
    let points: Vec<Point> = g.path_points().iter().map(|p| spin(*p)).collect();
    let (width, fill, layer) = graphic_style(g);
    let [r8, g8, b8] = layer_color(layer).1;
    let rgb = [r8 as f64 / 255.0, g8 as f64 / 255.0, b8 as f64 / 255.0];
    page.path(&points, rgb, width, fill)
}

/// Draw one project graphic (free board or schematic artwork).
fn pdf_project_graphic(page: &mut PdfPage, g: &pcbcraft_doc::Graphic) -> Result<(), String> {
    if g.shape == "text" {
        let anchor = g.points.first().copied().unwrap_or_default();
        return page.text(anchor, g.width.max(1.0), [0.0, 0.0, 0.0], &g.text);
    }
    let points = g.outline().unwrap_or_else(|| g.points.clone());
    page.path(&points, [0.1, 0.45, 0.35], g.width.min(0.4), false)
}

/// Render one view of the design onto the page.
fn pdf_draw(page: &mut PdfPage, p: &Project, view: &str) -> Result<(), String> {
    let front = [0.85, 0.25, 0.15];
    let back = [0.2, 0.45, 0.8];
    let copper_edge = [0.0, 0.0, 0.0];
    let pad_gold = [0.72, 0.55, 0.15];
    let ink = [0.05, 0.05, 0.05];
    let wire_green = [0.0, 0.45, 0.35];
    if view == "pcb" {
        match &p.board.outline {
            Some(o) => page.path(o, copper_edge, 0.2, true)?,
            None => {
                let rect = [
                    Point::new(0.0, 0.0),
                    Point::new(p.board.width, 0.0),
                    Point::new(p.board.width, p.board.height),
                    Point::new(0.0, p.board.height),
                ];
                page.path(&rect, copper_edge, 0.2, true)?;
            }
        }
        for c in &p.board.cutouts {
            page.circle(c.center, c.radius, [1.0, 1.0, 1.0], 0.1, true)?;
        }
        for z in &p.board.zones {
            let rgb = if z.layer == pcbcraft_doc::Layer::Front {
                front
            } else {
                back
            };
            page.path(&z.outline, rgb, 0.1, z.filled)?;
            for cut in &z.cutouts {
                page.path(cut, rgb, 0.1, false)?;
            }
        }
        for t in &p.board.tracks {
            let rgb = if t.layer == pcbcraft_doc::Layer::Front {
                front
            } else {
                back
            };
            match t.curve() {
                Some(arc) => {
                    let mut points = vec![t.start];
                    let steps = 24;
                    for i in 1..steps {
                        let f = i as f64 / steps as f64;
                        points.push(arc.point_at(f));
                    }
                    points.push(t.end);
                    page.path(&points, rgb, t.width, false)?;
                }
                None => page.path(&[t.start, t.end], rgb, t.width, false)?,
            }
        }
        for v in &p.board.vias {
            page.circle(v.position, v.diameter / 2.0, front, 0.05, true)?;
            if v.drill > 0.0 {
                page.circle(v.position, v.drill / 2.0, [1.0, 1.0, 1.0], 0.0, true)?;
            }
        }
        for t in &p.board.texts {
            let rgb = if t.layer == pcbcraft_doc::Layer::Front {
                ink
            } else {
                [0.2, 0.35, 0.6]
            };
            page.text(t.position, t.size.max(1.0), rgb, &t.text)?;
        }
        for g in p.graphics.iter().filter(|g| g.layer == "board") {
            pdf_project_graphic(page, g)?;
        }
        for f in &p.board.footprints {
            if let Some(library) = p
                .component(f.component)
                .and_then(|c| c.library_footprint.as_ref())
            {
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
                    pdf_graphic(page, &g, f.position, f.rotation)?;
                }
            }
            for pad in &f.pads {
                let points: Vec<Point> = pad
                    .outline_points()
                    .into_iter()
                    .map(|pos| f.position.offset(pos.rotate(-f.rotation)))
                    .collect();
                page.path(&points, pad_gold, 0.0, true)?;
                let drill: Vec<Point> = pad
                    .drill_outline_points()
                    .into_iter()
                    .map(|pos| f.position.offset(pos.rotate(-f.rotation)))
                    .collect();
                if !drill.is_empty() {
                    page.path(&drill, [1.0, 1.0, 1.0], 0.0, true)?;
                }
            }
            if let Some(c) = p.component(f.component) {
                let (min, max) = footprint_bounds(p, f);
                let label_x = (min.x + max.x) / 2.0;
                for (label, y) in [(&c.reference, min.y - 1.2), (&c.value, max.y + 1.2)] {
                    let point = f
                        .position
                        .offset(Point::new(label_x, y).rotate(-f.rotation));
                    page.text(point, 1.0, ink, label)?;
                }
            }
        }
    } else {
        for w in &p.wires {
            if let Some(path) = p.wire_path(w) {
                page.path(&path, wire_green, 0.2, false)?;
            }
        }
        for w in &p.drawn_wires {
            page.path(&w.points, wire_green, 0.2, false)?;
        }
        for g in p.graphics.iter().filter(|g| g.layer == "schematic") {
            pdf_project_graphic(page, g)?;
        }
        for l in &p.labels {
            if l.kind == "junction" {
                page.circle(l.position, 0.35, wire_green, 0.0, true)?;
            } else {
                page.text(l.position, 1.5, ink, &label_text(l, p.net_name(l.net)))?;
            }
        }
        for fc in &p.no_connects {
            if let Some(point) = p.pin_position(&fc.pin) {
                page.path(
                    &[
                        Point::new(point.x - 0.6, point.y - 0.6),
                        Point::new(point.x + 0.6, point.y + 0.6),
                    ],
                    front,
                    0.25,
                    false,
                )?;
                page.path(
                    &[
                        Point::new(point.x - 0.6, point.y + 0.6),
                        Point::new(point.x + 0.6, point.y - 0.6),
                    ],
                    front,
                    0.25,
                    false,
                )?;
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
                    pdf_graphic(page, &g.primitive, c.position, c.rotation)?;
                }
                for pin in library.definition.pins.iter().filter(|pin| {
                    (pin.unit == 0 && library.unit == 1 || pin.unit == library.unit)
                        && pin.body_style <= 1
                }) {
                    let angle = pin.angle.to_radians();
                    let local_end = pin.position.offset(Point::new(
                        pin.length * angle.cos(),
                        -pin.length * angle.sin(),
                    ));
                    let start = c.position.offset(pin.position.rotate(-c.rotation));
                    let end = c.position.offset(local_end.rotate(-c.rotation));
                    page.path(&[start, end], ink, 0.15, false)?;
                    page.text(start, 0.9, ink, &pin.number)?;
                }
            } else {
                let box_points = [
                    Point::new(-3.0, -1.5),
                    Point::new(3.0, -1.5),
                    Point::new(3.0, 1.5),
                    Point::new(-3.0, 1.5),
                ];
                let body: Vec<Point> = box_points
                    .iter()
                    .map(|p| c.position.offset(p.rotate(-c.rotation)))
                    .collect();
                page.path(&body, ink, 0.15, false)?;
                for stub in [[-5.0, -3.0], [3.0, 5.0]] {
                    let a = c
                        .position
                        .offset(Point::new(stub[0], 0.0).rotate(-c.rotation));
                    let b = c
                        .position
                        .offset(Point::new(stub[1], 0.0).rotate(-c.rotation));
                    page.path(&[a, b], ink, 0.2, false)?;
                }
            }
            page.text(
                Point::new(c.position.x, c.position.y - 3.0),
                1.5,
                ink,
                &format!("{} {}", c.reference, c.value),
            )?;
        }
    }
    Ok(())
}

/// Export the design as a one-page vector PDF document, in bytes.
pub fn pdf_document(p: &Project, view: &str) -> Result<Vec<u8>, String> {
    p.validate()?;
    if view != "pcb" && view != "schematic" {
        return Err("view must be pcb or schematic".into());
    }
    let (min, max) = preview_bounds(p, view);
    let mut page = PdfPage::new();
    page.fit(min, max);
    page.color([1.0, 1.0, 1.0], false)?;
    page.op("0 0 842 595 re f")?;
    pdf_draw(&mut page, p, view)?;
    let content = page.ops.as_bytes();
    let mut pdf: Vec<u8> = Vec::new();
    let mut offsets = [0u32; 5];
    pdf.extend_from_slice(b"%PDF-1.4\n");
    for (index, body) in [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 842 595] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>".to_string(),
        format!(
            "<< /Length {} >>\nstream\n{}endstream",
            content.len(),
            String::from_utf8_lossy(content)
        ),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
    ]
    .iter()
    .enumerate()
    {
        offsets[index] = pdf.len() as u32;
        pdf.extend_from_slice(
            format!("{} 0 obj\n{}\nendobj\n", index + 1, body)
                .as_bytes(),
        );
    }
    let xref_at = pdf.len() as u32;
    pdf.extend_from_slice(b"xref\n0 5\n0000000000 65535 f \n");
    for offset in offsets.iter() {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!("trailer\n<< /Size 5 /Root 1 0 R >>\nstartxref\n{xref_at}\n%%EOF\n").as_bytes(),
    );
    Ok(pdf)
}

/// A minimal AP214 (STEP) writer. The board outline and every placed pad
/// outline become closed prisms (two caps and one planar side face per edge)
/// in one shape representation. Coordinates are millimetres on the board
/// plane; Z spans the board thickness. Written by hand, no dependency.
const STEP_MAX_ENTITIES: u32 = 60_000;
/// Polygon rings are thinned to at most this many corners so rounded pads
/// stay affordable in one file.
const STEP_MAX_RING: usize = 32;

type Coords = (f64, f64, f64);

struct StepDoc {
    lines: Vec<String>,
    next: u32,
}

impl StepDoc {
    fn new() -> Self {
        StepDoc {
            lines: Vec::new(),
            next: 1,
        }
    }
    fn add(&mut self, body: &str) -> Result<u32, String> {
        if self.next > STEP_MAX_ENTITIES {
            return Err("the board is too complex for one STEP file".to_string());
        }
        let id = self.next;
        self.next += 1;
        self.lines.push(format!("#{id} = {body};"));
        Ok(id)
    }
    fn set(&mut self, id: u32, body: &str) {
        self.lines[id as usize - 1] = format!("#{id} = {body};");
    }
    fn n(v: f64) -> String {
        format!("{:.4}", v + 0.0)
    }
    fn point(&mut self, c: Coords) -> Result<u32, String> {
        self.add(&format!(
            "CARTESIAN_POINT('',({},{},{}))",
            Self::n(c.0),
            Self::n(c.1),
            Self::n(c.2)
        ))
    }
    fn direction(&mut self, d: Coords) -> Result<u32, String> {
        self.add(&format!(
            "DIRECTION('',({},{},{}))",
            Self::n(d.0),
            Self::n(d.1),
            Self::n(d.2)
        ))
    }
    fn vector(&mut self, dir: u32, length: f64) -> Result<u32, String> {
        self.add(&format!(
            "VECTOR('',#{dir},{})",
            Self::n(length.max(0.0001))
        ))
    }
    /// One planar face bounded by a closed loop over the given corners, with
    /// the plane normal and an in-plane reference direction.
    fn face(
        &mut self,
        corners: &[Coords],
        normal: Coords,
        reference: Coords,
    ) -> Result<u32, String> {
        let count = corners.len();
        let mut points = Vec::new();
        for c in corners {
            points.push(self.point(*c)?);
        }
        let mut vertices = Vec::new();
        for p in &points {
            vertices.push(self.add(&format!("VERTEX_POINT('',#{p})"))?);
        }
        let mut oriented = Vec::new();
        for index in 0..count {
            let next = (index + 1) % count;
            let a = corners[index];
            let b = corners[next];
            let delta = (b.0 - a.0, b.1 - a.1, b.2 - a.2);
            let length = (delta.0 * delta.0 + delta.1 * delta.1 + delta.2 * delta.2).sqrt();
            let dir = self.direction(delta)?;
            let vec = self.vector(dir, length)?;
            let line = self.add(&format!("LINE('',#{},#{vec})", points[index]))?;
            let edge = self.add(&format!(
                "EDGE_LINE('',#{line},#{},#{})",
                vertices[index], vertices[next]
            ))?;
            oriented.push(self.add(&format!("ORIENTED_EDGE('',*,*,#{edge},.T.)"))?);
        }
        let listed: Vec<String> = oriented.iter().map(|e| format!("#{e}")).collect();
        let loop_id = self.add(&format!("EDGE_LOOP('',({}))", listed.join(",")))?;
        let bound = self.add(&format!("FACE_OUTER_BOUND('',#{loop_id},.T.)"))?;
        let axis = self.direction(normal)?;
        let ref_dir = self.direction(reference)?;
        let placement = self.add(&format!(
            "AXIS2_PLACEMENT_3D('',#{},#{axis},#{ref_dir})",
            points[0]
        ))?;
        let plane = self.add(&format!("PLANE('',#{placement})"))?;
        self.add(&format!("ADVANCED_FACE((),(#{bound}),#{plane},.T.)"))
    }
    /// Extrude a closed polygon from z=0 to z=depth, returning the advanced
    /// face ids that close the prism.
    fn prism(&mut self, ring: &[Point], depth: f64) -> Result<Vec<u32>, String> {
        let ring = thin_ring(ring);
        let count = ring.len();
        if count < 3 {
            return Ok(Vec::new());
        }
        let mut faces = Vec::new();
        for index in 0..count {
            let next = (index + 1) % count;
            let a = ring[index];
            let b = ring[next];
            let dx = b.x - a.x;
            let dy = b.y - a.y;
            let length = (dx * dx + dy * dy).sqrt();
            if length < 1e-9 {
                continue;
            }
            let corners = [
                (a.x, a.y, 0.0),
                (b.x, b.y, 0.0),
                (b.x, b.y, depth),
                (a.x, a.y, depth),
            ];
            faces.push(self.face(
                &corners,
                (dy / length, -dx / length, 0.0),
                (dx / length, dy / length, 0.0),
            )?);
        }
        let bottom: Vec<Coords> = ring.iter().map(|p| (p.x, p.y, 0.0)).collect();
        let mut top: Vec<Coords> = ring.iter().map(|p| (p.x, p.y, depth)).collect();
        top.reverse();
        faces.push(self.face(&bottom, (0.0, 0.0, -1.0), (1.0, 0.0, 0.0))?);
        faces.push(self.face(&top, (0.0, 0.0, 1.0), (1.0, 0.0, 0.0))?);
        Ok(faces)
    }
}

/// Drop the duplicated closing vertex and thin long rings so rounded pads
/// stay affordable.
fn thin_ring(points: &[Point]) -> Vec<Point> {
    let mut ring: Vec<Point> = points.to_vec();
    if let (Some(first), Some(last)) = (ring.first().copied(), ring.last().copied())
        && ring.len() > 1
        && (first.x - last.x).abs() < 1e-9
        && (first.y - last.y).abs() < 1e-9
    {
        ring.pop();
    }
    while ring.len() > STEP_MAX_RING {
        ring = ring.into_iter().step_by(2).collect();
    }
    ring
}

/// Export the board as an AP214 STEP file: the outline and every placed pad
/// as a solid prism, in millimetres.
pub fn step_board(p: &Project) -> Result<String, String> {
    p.validate()?;
    let depth = p.board.thickness;
    let outline: Vec<Point> = match &p.board.outline {
        Some(o) if o.len() >= 3 => o.clone(),
        _ => vec![
            Point::new(0.0, 0.0),
            Point::new(p.board.width, 0.0),
            Point::new(p.board.width, p.board.height),
            Point::new(0.0, p.board.height),
        ],
    };
    let mut doc = StepDoc::new();
    doc.add("PRODUCT('BOARD','pcbcraft board','',('Mechanical Design'))")?;
    doc.add("PRODUCT_DEFINITION_FORMATION_WITH_SPECIFIED_SOURCE('','',#1,.NOT_KNOWN.)")?;
    doc.add("PRODUCT_CONTEXT('',#1,'mechanical')")?;
    doc.add("PRODUCT_DEFINITION('design','',#2,#3)")?;
    doc.add("PRODUCT_DEFINITION_SHAPE('','',#4)")?;
    let shape = doc.add("SHAPE_REPRESENTATION('board',(#14,#15),#7)")?;
    doc.add("(GEOMETRIC_REPRESENTATION_CONTEXT(3)GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT((#16))GLOBAL_UNIT_ASSIGNED_CONTEXT((#8,#9,#10))REPRESENTATION_CONTEXT('','',''))")?;
    doc.add("(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT(.MILLI.,.METRE.)LENGTH_MEASURE())")?;
    doc.add("(NAMED_UNIT(*)PLANE_ANGLE_UNIT()SI_UNIT($,.RADIAN.))")?;
    doc.add("(NAMED_UNIT(*)SI_UNIT($,.STERADIAN.)SOLID_ANGLE_MEASURE())")?;
    doc.add("UNCERTAINTY_MEASURE_WITH_UNIT(LENGTH_MEASURE(0.001),#8,'distance_accuracy_value','confusion accuracy')")?;
    let origin = doc.point((0.0, 0.0, 0.0))?;
    let axis = doc.direction((0.0, 0.0, 1.0))?;
    let reference = doc.direction((1.0, 0.0, 0.0))?;
    let base = doc.add(&format!(
        "AXIS2_PLACEMENT_3D('',#{origin},#{axis},#{reference})"
    ))?;
    doc.add("SHAPE_DEFINITION_REPRESENTATION(#5,#6)")?;
    let mut items = vec![base];
    items.extend(doc.prism(&outline, depth)?);
    for footprint in &p.board.footprints {
        for pad in &footprint.pads {
            let ring: Vec<Point> = pad
                .outline_points()
                .into_iter()
                .map(|pos| footprint.position.offset(pos.rotate(-footprint.rotation)))
                .collect();
            items.extend(doc.prism(&ring, depth)?);
        }
    }
    let listed: Vec<String> = items.iter().map(|i| format!("#{i}")).collect();
    doc.set(
        shape,
        &format!("SHAPE_REPRESENTATION('board',({}),#7)", listed.join(",")),
    );
    let mut out = String::new();
    out.push_str("ISO-10303-21;\nHEADER;\n");
    out.push_str("FILE_DESCRIPTION(('pcbcraft board export'),'2;1');\n");
    out.push_str("FILE_NAME('board.step','','','(pcbcraft)','pcbcraft','pcbcraft','');\n");
    out.push_str("FILE_SCHEMA(('AUTOMOTIVE_DESIGN { 1 0 10303 214 1 1 1 1 }'));\n");
    out.push_str("ENDSEC;\nDATA;\n");
    for line in &doc.lines {
        out.push_str(line);
        out.push('\n');
    }
    out.push_str("ENDSEC;\nEND-ISO-10303-21;\n");
    Ok(out)
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
            if let Some(m) = &library.definition.model {
                s.push_str(&format!(
                    " (model {} (offset (xyz {} {} {})) (scale (xyz {} {} {})) (rotate (xyz {} {} {})))\n",
                    quote(&m.path),
                    m.offset[0],
                    m.offset[1],
                    m.offset[2],
                    m.scale[0],
                    m.scale[1],
                    m.scale[2],
                    m.rotate[0],
                    m.rotate[1],
                    m.rotate[2]
                ));
            }
            if let Some(r) = &library.definition.rules {
                if let Some(v) = r.clearance {
                    s.push_str(&format!(" (clearance {v})\n"));
                }
                if let Some(v) = r.mask_margin {
                    s.push_str(&format!(" (solder_mask_margin {v})\n"));
                }
                if let Some(v) = r.paste_margin {
                    s.push_str(&format!(" (solder_paste_margin {v})\n"));
                }
            }
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
        match t.arc {
            Some(mid) => s.push_str(&format!(
                " (arc (start {} {}) (mid {} {}) (end {} {}) (width {}) (layer {}) (net {}) (uuid {}))\n",
                t.start.x,
                t.start.y,
                mid.x,
                mid.y,
                t.end.x,
                t.end.y,
                t.width,
                quote(t.layer.name()),
                ordinal(Some(t.net)),
                uuid(t.id, 1)
            )),
            None => s.push_str(&format!(
                " (segment (start {} {}) (end {} {}) (width {}) (layer {}) (net {}) (uuid {}))\n",
                t.start.x,
                t.start.y,
                t.end.x,
                t.end.y,
                t.width,
                quote(t.layer.name()),
                ordinal(Some(t.net)),
                uuid(t.id, 1)
            )),
        }
    }
    match &b.outline {
        Some(o) => {
            s.push_str(" (gr_poly (pts");
            for pt in o {
                s.push_str(&format!(" (start {} {})", pt.x, pt.y));
            }
            s.push_str(
                ") (stroke (width 0.05) (type solid)) (fill none) (layer \"Edge.Cuts\"))\n",
            );
        }
        None => s.push_str(&format!(" (gr_rect (start 0 0) (end {} {}) (stroke (width 0.05) (type solid)) (fill none) (layer \"Edge.Cuts\"))\n",b.width,b.height)),
    }
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
/// Edge.Cuts corner or vertex list for fabrication output: the traced
/// outline when the board has one, the plain rectangle otherwise.
fn board_edge_points(p: &Project) -> Vec<(f64, f64)> {
    match &p.board.outline {
        Some(o) => o.iter().map(|pt| (pt.x, pt.y)).collect(),
        None => vec![
            (0.0, 0.0),
            (p.board.width, 0.0),
            (p.board.width, p.board.height),
            (0.0, p.board.height),
        ],
    }
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
    // An inch project gets imperial fabrication files; coordinates are still
    // stored in millimetres, so every written value is rescaled here.
    let inch = p.unit == "inch";
    let scale = if inch { 1.0 / 25.4 } else { 1.0 };
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
        scale: f64,
    }
    impl LayerFile {
        fn new(scale: f64) -> Self {
            Self {
                apertures: BTreeMap::new(),
                body: String::new(),
                arcs: String::new(),
                ops: 0,
                scale,
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
            self.body.push_str(&format!(
                "D{code}X{:.6}Y{:.6}D03*\n",
                x * self.scale,
                y * self.scale
            ));
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
                    x * self.scale,
                    y * self.scale,
                    if i == 0 { 2 } else { 1 }
                ));
            }
            if closed && let Some((x, y)) = points.first() {
                self.body.push_str(&format!(
                    "X{:.6}Y{:.6}D01*\n",
                    x * self.scale,
                    y * self.scale
                ));
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
                    x * self.scale,
                    y * self.scale,
                    if i == 0 { 2 } else { 1 }
                ));
            }
            self.body.push_str("G37*\n");
            Ok(())
        }
        fn finish(&self, mirror: bool, name: &str) -> String {
            let mirror = if mirror { "D" } else { "A" };
            let mo = if self.scale != 1.0 { "MOIN" } else { "MOMM" };
            let mut out =
                format!("G04 {name} generated by PCBCraft *\n%FSLANX*X{mirror}Y{mirror}{mo}*%\n");
            for (ap, code) in &self.apertures {
                match *ap {
                    Ap::Circle(d) => out.push_str(&format!(
                        "%ADD{code}C,{:.6}*%\n",
                        d as f64 / 1000.0 * self.scale
                    )),
                    Ap::Rect(w, h) => out.push_str(&format!(
                        "%ADD{code}R,{:.6}X{:.6}*%\n",
                        w as f64 / 1000.0 * self.scale,
                        h as f64 / 1000.0 * self.scale
                    )),
                    Ap::Obround(w, h) => out.push_str(&format!(
                        "%ADD{code}O,{:.6}X{:.6}*%\n",
                        w as f64 / 1000.0 * self.scale,
                        h as f64 / 1000.0 * self.scale
                    )),
                    Ap::Round(w, h, r) => out.push_str(&format!(
                        "%ADD{code}R2,{} R1,{:.6}X{:.6} {r} 0 0*%\n",
                        if r * 2 == h {
                            "0".to_string()
                        } else {
                            format!("{:.6}", r as f64 / 1000.0 * self.scale)
                        },
                        w as f64 / 1000.0 * self.scale,
                        h as f64 / 1000.0 * self.scale
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
    let mut front = LayerFile::new(scale);
    let mut back = LayerFile::new(scale);
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
        if let Some(arc) = t.curve() {
            file.ops += 1;
            if file.ops > MAX_OPS {
                return Err("gerber export exceeds the object limit".into());
            }
            let (sx, sy) = mirror(t.start.x, t.start.y, is_back);
            let (ex, ey) = mirror(t.end.x, t.end.y, is_back);
            let (cx, cy) = mirror(arc.center.x, arc.center.y, is_back);
            let clockwise = if is_back {
                arc.sweep > 0.0
            } else {
                arc.sweep < 0.0
            };
            file.body.push_str(&format!(
                "D{code}D02*\nX{:.6}Y{:.6}D02*\nX{:.6}Y{:.6}I{:.6}J{:.6}G0{}*\n",
                sx,
                sy,
                ex,
                ey,
                cx - sx,
                cy - sy,
                if clockwise { 2 } else { 3 }
            ));
        } else {
            file.stroke(
                code,
                &[
                    mirror(t.start.x, t.start.y, is_back),
                    mirror(t.end.x, t.end.y, is_back),
                ],
                false,
            )?;
        }
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
    let mut outline = LayerFile::new(scale);
    let edge_code = outline.code(Ap::Circle(100))?;
    outline.stroke(edge_code, &board_edge_points(p), true)?;
    for c in &p.board.cutouts {
        outline.ops += 1;
        if outline.ops > MAX_OPS {
            return Err("gerber export exceeds the object limit".into());
        }
        outline.arcs.push_str(&format!(
            "X{:.6}Y{:.6}D02*\nI{:.6}J{:.6}G02*\n",
            (c.center.x - c.radius) * scale,
            c.center.y * scale,
            c.radius * scale,
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
    for h in &p.board.holes {
        add_hole(
            (h.diameter * 1_000.0).round() as u64,
            0,
            h.position.x,
            h.position.y,
            0.0,
            0.0,
        );
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
    let (drill_header, drill_prec) = if inch {
        ("INCH,TZ", 4)
    } else {
        ("METRIC,TZ", 3)
    };
    let mut drill = format!("M48\nFMAT,2\n{drill_header}\n");
    for (n, (d, _)) in drills.keys().enumerate() {
        drill.push_str(&format!(
            "T{}C{:.drill_prec$}\n",
            n + 1,
            *d as f64 / 1000.0 * scale
        ));
    }
    drill.push_str("%\nG90\nG05\n");
    let origin = p.board.drill_origin;
    for (n, holes) in drills.values().enumerate() {
        drill.push_str(&format!("T{}\n", n + 1));
        for (x, y, ux, uy) in holes {
            let (x, y) = (x - origin.x, y - origin.y);
            let (x, y) = (x * scale, y * scale);
            let (ux, uy) = (ux * scale, uy * scale);
            if ux == 0.0 && uy == 0.0 {
                drill.push_str(&format!("X{x:.drill_prec$}Y{y:.drill_prec$}\n"));
            } else {
                drill.push_str(&format!(
                    "X{a:.drill_prec$}Y{b:.drill_prec$}G85X{c:.drill_prec$}Y{d:.drill_prec$}\n",
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
/// sorted, quoted reference list. Requested property keys become extra
/// columns; with no request, the columns the project itself defines are
/// printed under their labels.
pub fn bom_csv(p: &Project, fields: &[String], variant: Option<&str>) -> Result<String, String> {
    p.validate()?;
    if fields.len() > 16 {
        return Err("a bill of materials lists at most 16 properties".into());
    }
    for key in fields {
        if key.is_empty() || key.len() > 64 {
            return Err("property names must be 1 to 64 bytes".into());
        }
    }
    let variant = match variant.map(str::trim) {
        None | Some("") => None,
        Some(name) => Some(
            p.variants
                .iter()
                .find(|v| v.name == name)
                .ok_or_else(|| format!("no variant named {name}"))?,
        ),
    };
    // Columns come from the caller's list, or from the columns the project
    // itself defines when the caller asks for none.
    let columns: Vec<(String, String)> = if fields.is_empty() {
        p.bom_fields
            .iter()
            .map(|f| {
                (
                    f.key.clone(),
                    if f.label.is_empty() {
                        f.key.clone()
                    } else {
                        f.label.clone()
                    },
                )
            })
            .collect()
    } else {
        fields
            .iter()
            .map(|key| (key.clone(), key.clone()))
            .collect()
    };
    let mut groups: BTreeMap<(String, String, String, Vec<String>), Vec<String>> = BTreeMap::new();
    for c in &p.components {
        if variant.is_some_and(|v| v.dnp.contains(&c.id)) {
            continue;
        }
        let footprint = match variant
            .and_then(|v| v.footprints.iter().find(|o| o.component == c.id))
        {
            Some(swap) => swap.footprint.clone(),
            None => match &c.library_footprint {
                Some(footprint) => format!("{}:{}", footprint.library, footprint.definition.name),
                None => "starter pads".into(),
            },
        };
        let values: Vec<String> = columns
            .iter()
            .map(|(key, _)| c.field(key).unwrap_or("").to_string())
            .collect();
        groups
            .entry((c.value.clone(), c.kind.clone(), footprint, values))
            .or_default()
            .push(c.reference.clone());
    }
    let mut out = String::from("quantity,references,value,kind,footprint");
    for (_, header) in &columns {
        out.push(',');
        out.push_str(&csv_cell(header));
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
/// Pick-and-place placement file: one row per placed footprint in the classic
/// centroid format (`SMT|THT,reference,X,Y,rotation,side,package`), sorted by
/// reference. Side comes from the pad layers; height is unknown to this tool.
/// In-circuit test netlist in IPC-D-356A style fixed columns: five-digit net
/// number, twelve-column component reference, ten-column pin name, a 000000
/// operation code and eight-digit X and Y coordinates in micrometres, sorted
/// by net number, then reference, then pin. Comment lines start with `*`.
pub fn ipc356_netlist(p: &Project) -> Result<String, String> {
    p.validate()?;
    const MAX_PINS: usize = 50_000;
    let numbers: BTreeMap<u64, usize> = p
        .nets
        .iter()
        .enumerate()
        .map(|(i, n)| (n.id, i + 1))
        .collect();
    let refs: BTreeMap<u64, &pcbcraft_doc::Component> =
        p.components.iter().map(|c| (c.id, c)).collect();
    let mut rows: Vec<(usize, String, String, i64, i64)> = Vec::new();
    for f in &p.board.footprints {
        let c = refs
            .get(&f.component)
            .ok_or("footprint references an unknown component")?;
        if c.reference.len() > 12 {
            return Err("reference is too long for the test netlist".into());
        }
        for pad in &f.pads {
            if pad.number.len() > 10 {
                return Err("pin name is too long for the test netlist".into());
            }
            let net = match pad.net {
                Some(n) => *numbers.get(&n).ok_or("pad references an unknown net")?,
                None => 0,
            };
            let world = f.position.offset(pad.offset.rotate(-f.rotation));
            if !world.x.is_finite() || !world.y.is_finite() {
                return Err("pad position is not a number".into());
            }
            let (x, y) = (
                (world.x * 1000.0).round() as i64,
                (world.y * 1000.0).round() as i64,
            );
            if x < 0 || y < 0 || x > 99_999_999 || y > 99_999_999 {
                return Err("pad coordinates fall outside the test file range".into());
            }
            rows.push((net, c.reference.clone(), pad.number.clone(), x, y));
        }
    }
    if rows.len() > MAX_PINS {
        return Err("test netlist exceeds the pin limit".into());
    }
    rows.sort();
    let mut out = format!(
        "*- PCBCraft IPC-D-356A test netlist\n*- board {} x {} mm, {} nets\n",
        p.board.width,
        p.board.height,
        p.nets.len()
    );
    for (net, reference, pin, x, y) in &rows {
        out.push_str(&format!(
            "{net:05}{reference:<12}{pin:<10}000000{x:08}{y:08}\n"
        ));
    }
    Ok(out)
}

pub fn placement_text(p: &Project) -> Result<String, String> {
    p.validate()?;
    const MAX_PLACED: usize = 5_000;
    let refs: BTreeMap<u64, &pcbcraft_doc::Component> =
        p.components.iter().map(|c| (c.id, c)).collect();
    if p.board.footprints.len() > MAX_PLACED {
        return Err("placement file exceeds the placed-object limit".into());
    }
    let mut rows: Vec<String> = Vec::new();
    for f in &p.board.footprints {
        let c = refs
            .get(&f.component)
            .ok_or("footprint references an unknown component")?;
        let back = f.pads.iter().any(|pad| {
            pad.geometry.as_ref().is_some_and(|g| {
                g.pad_type == "smd"
                    && !g.layers.is_empty()
                    && !g.layers.iter().any(|l| l.starts_with("F."))
            })
        });
        let tht = f.pads.iter().any(|pad| {
            pad.geometry
                .as_ref()
                .is_some_and(|g| g.pad_type == "thru_hole")
        });
        let package = match &c.library_footprint {
            Some(footprint) => footprint.definition.name.clone(),
            None => c.kind.clone(),
        };
        rows.push(format!(
            "{},\"{}\",{:.3},{:.3},{:.1},{},\"{}\"",
            if tht { "THT" } else { "SMT" },
            c.reference,
            f.position.x,
            f.position.y,
            f.rotation.rem_euclid(360.0),
            if back { "B" } else { "T" },
            package
        ));
    }
    for (i, f) in p.board.fiducials.iter().enumerate() {
        rows.push(format!(
            "SMT,\"FID{:02}\",{:.3},{:.3},0.0,{},FIDUCIAL",
            i + 1,
            f.position.x,
            f.position.y,
            if f.layer == pcbcraft_doc::Layer::Back {
                "B"
            } else {
                "T"
            }
        ));
    }
    rows.sort_by(|a, b| {
        a.split('"')
            .nth(1)
            .unwrap_or("")
            .cmp(b.split('"').nth(1).unwrap_or(""))
    });
    Ok(format!(
        "File Format,ASCII,\n#\n#\tPick and Place File\t{}\n#\nSMT,THT,X,Y,Rotation,Side,Package\n{}\n",
        p.name,
        rows.join("\n")
    ))
}
/// Board as a legacy trace file for legacy importers and viewers: every pad
/// with its absolute position in whole mils and a front or back layer code,
/// followed by every net with its member pads. Coordinates are whole mils.
pub fn cmp_text(p: &Project) -> Result<String, String> {
    p.validate()?;
    const MAX_LINES: usize = 50_000;
    let refs: BTreeMap<u64, &pcbcraft_doc::Component> =
        p.components.iter().map(|c| (c.id, c)).collect();
    let mils = |v: f64| (v / 0.0254).round() as i64;
    let mut components: Vec<String> = Vec::new();
    for f in &p.board.footprints {
        let c = refs
            .get(&f.component)
            .ok_or("footprint references an unknown component")?;
        for pad in &f.pads {
            let back = pad.geometry.as_ref().is_some_and(|g| {
                !g.layers.is_empty()
                    && if g.pad_type == "smd" {
                        !g.layers.iter().any(|l| l.starts_with("F."))
                    } else {
                        g.layers.iter().any(|l| l == "B.Cu")
                    }
            });
            let at = f.position.offset(pad.offset.rotate(-f.rotation));
            components.push(format!(
                "$CD {}\t{}\t{}\t{}\t{}\t{}\t0",
                c.reference,
                pad.number,
                mils(at.x),
                mils(at.y),
                if back { 1 } else { 0 },
                usize::from(pad.diameter.max(0.0) > 0.0)
            ));
        }
    }
    let mut nets: Vec<String> = Vec::new();
    for n in &p.nets {
        let mut members: BTreeSet<(u64, usize)> = BTreeSet::new();
        for f in &p.board.footprints {
            for (i, pad) in f.pads.iter().enumerate() {
                if pad.net == Some(n.id) {
                    members.insert((f.component, i + 1));
                }
            }
        }
        if members.is_empty() {
            continue;
        }
        let list = members
            .iter()
            .map(|(c, i)| format!("{}\t{}", c, i))
            .collect::<Vec<_>>()
            .join("\t");
        nets.push(format!("{}\t{}\t{}", n.name, members.len(), list));
    }
    if components.is_empty() && nets.is_empty() {
        return Err("board has no placed pads or connected nets to export".into());
    }
    if components.len() + nets.len() > MAX_LINES {
        return Err("trace file exceeds the element limit".into());
    }
    Ok(format!(
        "$FILE\nTYPE:BOARD\nCODE:ASCII\n#\n{}\n$COMPONENTS\n{}\n$END\n#\n$NETS\n{}\n$END\n#\n$END\n",
        p.name,
        components.join("\n"),
        nets.join("\n")
    ))
}
/// Board as an IDF mechanical drawing for enclosure tools: board size, origin
/// and reference, one entry per placed footprint with its position, side and
/// rotation, and a matching default-height entry per component. The native
/// format stores no component heights, so heights are emitted as 1 mm.
pub fn idf_board(p: &Project) -> Result<String, String> {
    p.validate()?;
    const MAX_PLACED: usize = 5_000;
    if p.board.footprints.len() > MAX_PLACED {
        return Err("mechanical drawing exceeds the placed-object limit".into());
    }
    if p.board.footprints.is_empty() {
        return Err("board has no placed footprints to export".into());
    }
    let refs: BTreeMap<u64, &pcbcraft_doc::Component> =
        p.components.iter().map(|c| (c.id, c)).collect();
    let n3 = |v: f64| -> String {
        let r = (v * 1000.0).round() / 1000.0;
        if r == 0.0 {
            "0.000".into()
        } else {
            format!("{r:.3}")
        }
    };
    let mut body = String::new();
    for (pin, f) in p.board.footprints.iter().enumerate() {
        let c = refs
            .get(&f.component)
            .ok_or("footprint references an unknown component")?;
        let back = f.pads.iter().any(|pad| {
            pad.geometry.as_ref().is_some_and(|g| {
                g.pad_type == "smd"
                    && !g.layers.is_empty()
                    && !g.layers.iter().any(|l| l.starts_with("F."))
            })
        });
        body.push_str(&format!(
            ".END_BODY {}\t{}\t{}\t{}\t{}\t{}\n",
            c.reference,
            n3(f.position.x),
            n3(f.position.y),
            if back { "BACK" } else { "FRONT" },
            n3(f.rotation),
            pin
        ));
    }
    let mut pinbody = String::new();
    for f in &p.board.footprints {
        let c = refs
            .get(&f.component)
            .ok_or("footprint references an unknown component")?;
        pinbody.push_str(&format!(
            ".END_PIN {{\t}}\t{}\t{}\t{}\t0.000\t0.000\t1.000\t0\n",
            c.reference,
            n3(f.position.x),
            n3(f.position.y)
        ));
    }
    Ok(format!(
        ".FILE_IDF\n.3DIDF VERSION: 2.0\n.UNITS:\tmillmm\n\
         .BOARD_PATH\n\t.BODY {w} {h} 0.000 0.000\n\t.OFFSET 0.000 0.000\n\t.REFERENCE \"{name}\"\n.END_BOARD_PATH\n\
         .PINUP 0\n.PINUP_END\n.TPIN 0\n.TPIN_END\n.PLPIN 0\n.PLPIN_END\n\
         .ELEC_PATH\n{body}.END_ELEC_PATH\n\
         .PIN_FILE\n.PIN {count}\n{pin}.END_PIN_FILE\n.END_FILE_IDF\n",
        w = n3(p.board.width),
        h = n3(p.board.height),
        name = p.name.replace('"', "'"),
        body = body,
        count = p.board.footprints.len(),
        pin = pinbody,
    ))
}
/// Board as a Specctra-style board file for external autorouters: the board
/// boundary, layer stack with a through via type, net classes, every net with
/// its pins, component and pin placements, track wires (arcs sampled into
/// segments), vias, zone pours and rectangular rule areas as keepouts.
/// Coordinates are whole micrometres at a resolution of 1000 per millimetre.
pub fn specctra_dsn(p: &Project) -> Result<String, String> {
    p.validate()?;
    const MAX_ELEMENTS: usize = 50_000;
    let mut count = 0usize;
    let bump = |n: &mut usize| -> Result<(), String> {
        *n += 1;
        if *n > MAX_ELEMENTS {
            return Err("board file exceeds the element limit".into());
        }
        Ok(())
    };
    let um = |v: f64| -> Result<i64, String> {
        let scaled = (v * 1000.0).round();
        if !scaled.is_finite() || scaled.abs() > 2_000_000_000.0 {
            return Err("board coordinate falls outside the file range".into());
        }
        Ok(scaled as i64)
    };
    let point =
        |q: &Point| -> Result<String, String> { Ok(format!("({} {})", um(q.x)?, um(q.y)?)) };
    let numbers: BTreeMap<u64, usize> = p
        .nets
        .iter()
        .enumerate()
        .map(|(i, n)| (n.id, i + 1))
        .collect();
    let net_number = |id: Option<u64>| -> Result<usize, String> {
        match id {
            None => Ok(0),
            Some(n) => numbers
                .get(&n)
                .copied()
                .ok_or_else(|| "element references an unknown net".to_string()),
        }
    };
    let refs: BTreeMap<u64, &pcbcraft_doc::Component> =
        p.components.iter().map(|c| (c.id, c)).collect();
    let mut out = format!(
        "(PCB {}\n  (resolution 1000.0 1000.0)\n  (units mm)\n",
        quote(&p.name)
    );
    out.push_str("  (network\n");
    for (i, net) in p.nets.iter().enumerate() {
        bump(&mut count)?;
        let mut pins: Vec<String> = Vec::new();
        for f in &p.board.footprints {
            let c = refs
                .get(&f.component)
                .ok_or("footprint references an unknown component")?;
            for pad in &f.pads {
                if pad.net == Some(net.id) {
                    pins.push(format!("{}.{}", quote(&c.reference), quote(&pad.number)));
                }
            }
        }
        out.push_str(&format!(
            "    (net {} {} (pins {}))\n",
            i + 1,
            quote(&net.name),
            pins.join(" ")
        ));
    }
    for class in &p.net_classes {
        bump(&mut count)?;
        out.push_str(&format!(
            "    (netclass {} (use {}))\n",
            quote(&class.name),
            quote(&class.name)
        ));
    }
    out.push_str("  )\n");
    for class in &p.net_classes {
        out.push_str(&format!(
            "  (classpath {} (circuit (minClearance {}) (sameNetClearance {}) (defaultTraceWidth {})) (layer \"F.Cu\" \"(c: {}))\" (layer \"B.Cu\" \"(c: {}))\")\n",
            quote(&class.name),
            um(class.clearance)?,
            um(class.clearance)?,
            um(class.width)?,
            um(class.width)?,
            um(class.width)?
        ));
    }
    let clearance = um(p.board.clearance)?;
    out.push_str(&format!(
        "  (structure\n    (layer \"signal\" \"F.Cu\" (circuit (minClearance {c}) (viaSize 600 600) (viaDepth 0 0)))\n    (layer \"signal\" \"B.Cu\" (circuit (minClearance {c}) (viaSize 600 600) (viaDepth 0 0)))\n    (layer \"through_via\" \"VIA\" (circuit (viaType \"VIA\")))\n    (layer \"boundary\" \"BOARD_BOUNDARY\" (boundary (direction \"positive\")))\n    (via \"VIA\" (type \"through_via\") (shape (circle (path (0 0)) (width 600))))\n  )\n",
        c = clearance
    ));
    out.push_str("  (image\n");
    let corners: Vec<Point> = board_edge_points(p)
        .iter()
        .map(|q| Point::new(q.0, q.1))
        .collect();
    let boundary: Result<Vec<String>, String> = corners.iter().map(point).collect();
    out.push_str(&format!(
        "    (polygon (layer BOARD_BOUNDARY) (net 0) (path {}))\n",
        boundary?.join(" ")
    ));
    for f in &p.board.footprints {
        let c = refs
            .get(&f.component)
            .ok_or("footprint references an unknown component")?;
        bump(&mut count)?;
        out.push_str(&format!(
            "    (component (name {}) (place (type fixed) (side top) (rotate {:.1}) (at {})))\n",
            quote(&c.reference),
            f.rotation.rem_euclid(360.0),
            point(&f.position)?
        ));
        for pad in &f.pads {
            bump(&mut count)?;
            let world = f.position.offset(pad.offset.rotate(-f.rotation));
            out.push_str(&format!(
                "    (pin (name {}.{}) (length 0) (at {}) (shape (circle (path (0 0)) (width {}))) (number {}))\n",
                quote(&c.reference),
                quote(&pad.number),
                point(&world)?,
                um(pad.diameter.max(0.1))?,
                quote(&pad.number)
            ));
        }
    }
    for t in &p.board.tracks {
        bump(&mut count)?;
        let layer = if t.layer == pcbcraft_doc::Layer::Back {
            "B.Cu"
        } else {
            "F.Cu"
        };
        let width = um(t.width)?;
        let net = net_number(Some(t.net))?;
        let segments: Vec<(Point, Point)> = match t.curve() {
            Some(arc) => {
                let steps = 8;
                let at = |i: usize| {
                    let a = arc.start_angle + arc.sweep * (i as f64 / steps as f64);
                    Point::new(
                        arc.center.x + arc.radius * a.cos(),
                        arc.center.y + arc.radius * a.sin(),
                    )
                };
                (0..steps).map(|i| (at(i), at(i + 1))).collect()
            }
            None => vec![(t.start, t.end)],
        };
        for (a, b) in segments {
            out.push_str(&format!(
                "    (wire (path (net {net}) (layer {layer}) (width {width}) {} {}))\n",
                point(&a)?,
                point(&b)?
            ));
        }
    }
    for v in &p.board.vias {
        bump(&mut count)?;
        let net = net_number(Some(v.net))?;
        out.push_str(&format!(
            "    (via (net {net}) (type VIA) (at {}))\n",
            point(&v.position)?
        ));
    }
    for z in &p.board.zones {
        bump(&mut count)?;
        let layer = if z.layer == pcbcraft_doc::Layer::Back {
            "B.Cu"
        } else {
            "F.Cu"
        };
        let net = net_number(Some(z.net))?;
        let path: Result<Vec<String>, String> = z.outline.iter().map(point).collect();
        out.push_str(&format!(
            "    (polygon (layer {layer}) (net {net}) (path {}))\n",
            path?.join(" ")
        ));
    }
    for area in &p.board.rule_areas {
        bump(&mut count)?;
        let layers: Vec<&str> = match area.layer {
            Some(l) => vec![if l == pcbcraft_doc::Layer::Back {
                "B.Cu"
            } else {
                "F.Cu"
            }],
            None => vec!["F.Cu", "B.Cu"],
        };
        let lo = Point::new(area.region.x, area.region.y);
        let hi = Point::new(
            area.region.x + area.region.width,
            area.region.y + area.region.height,
        );
        for layer in layers {
            out.push_str(&format!(
                "    (keepout (layer {layer}) (net 0) (rect {} {}))\n",
                point(&lo)?,
                point(&hi)?
            ));
        }
    }
    out.push_str("  )\n)\n");
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
        fn arc(
            &mut self,
            layer: &str,
            c: (f64, f64),
            r: f64,
            from_deg: f64,
            to_deg: f64,
        ) -> Result<(), String> {
            self.follow(c.0 - r, c.1 - r);
            self.follow(c.0 + r, c.1 + r);
            self.keep(
                layer,
                format!(
                    "0\nARC\n10\n{:.3}\n20\n{:.3}\n40\n{:.3}\n50\n{:.6}\n51\n{:.6}\n",
                    c.0, c.1, r, from_deg, to_deg
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
    let corners = board_edge_points(p);
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
        match t.curve() {
            Some(arc) => {
                let deg = |a: f64| {
                    let mut d = a.to_degrees() % 360.0;
                    if d < 0.0 {
                        d += 360.0;
                    }
                    d
                };
                let end_angle = arc.start_angle + arc.sweep;
                let (from, to) = if arc.sweep >= 0.0 {
                    (arc.start_angle, end_angle)
                } else {
                    (end_angle, arc.start_angle)
                };
                w.arc(layer, xy(&arc.center), arc.radius, deg(from), deg(to))?;
            }
            None => w.line(layer, xy(&t.start), xy(&t.end))?,
        }
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
/// A drawable entity read back from a DXF drawing file.
#[derive(Clone, Debug, PartialEq)]
pub enum DxfShape {
    Line {
        start: Point,
        end: Point,
    },
    Circle {
        center: Point,
        radius: f64,
    },
    Arc {
        center: Point,
        radius: f64,
        start_deg: f64,
        end_deg: f64,
    },
    Polyline {
        points: Vec<Point>,
        closed: bool,
    },
    Text {
        at: Point,
        height: f64,
        rotation: f64,
        value: String,
    },
}
/// One entity of a DXF drawing, with the layer name it was drawn on.
#[derive(Clone, Debug, PartialEq)]
pub struct DxfEntity {
    pub layer: String,
    pub shape: DxfShape,
}

#[derive(Default)]
struct DxfAcc {
    name: String,
    layer: String,
    points: Vec<Point>,
    pending_x: Option<f64>,
    pending_y: Option<f64>,
    end: Option<Point>,
    end_x: Option<f64>,
    end_y: Option<f64>,
    radius: Option<f64>,
    start_deg: Option<f64>,
    end_deg: Option<f64>,
    closed: bool,
    value: String,
}

impl DxfAcc {
    fn finish(&self) -> Result<Option<DxfShape>, String> {
        let number = |v: Option<f64>, what: &str| -> Result<f64, String> {
            v.filter(|v| v.is_finite())
                .ok_or_else(|| format!("DXF {what} is missing or not a number"))
        };
        match self.name.as_str() {
            "LINE" => {
                if self.points.len() != 1 {
                    return Err("DXF line needs one start point".into());
                }
                let end = self
                    .end
                    .filter(|p| p.valid())
                    .ok_or("DXF line needs a valid end point")?;
                let start = self.points[0];
                if !start.valid() {
                    return Err("DXF line start is out of range".into());
                }
                Ok(Some(DxfShape::Line { start, end }))
            }
            "CIRCLE" => {
                if self.points.len() != 1 {
                    return Err("DXF circle needs one centre".into());
                }
                let radius = number(self.radius, "circle radius")?;
                if radius <= 0.0 || radius > 100_000.0 {
                    return Err("DXF circle radius is out of range".into());
                }
                if !self.points[0].valid() {
                    return Err("DXF circle centre is out of range".into());
                }
                Ok(Some(DxfShape::Circle {
                    center: self.points[0],
                    radius,
                }))
            }
            "ARC" => {
                if self.points.len() != 1 {
                    return Err("DXF arc needs one centre".into());
                }
                let radius = number(self.radius, "arc radius")?;
                let start_deg = number(self.start_deg, "arc start angle")?;
                let end_deg = number(self.end_deg, "arc end angle")?;
                if radius <= 0.0 || radius > 100_000.0 {
                    return Err("DXF arc radius is out of range".into());
                }
                if !self.points[0].valid() {
                    return Err("DXF arc centre is out of range".into());
                }
                Ok(Some(DxfShape::Arc {
                    center: self.points[0],
                    radius,
                    start_deg,
                    end_deg,
                }))
            }
            "LWPOLYLINE" | "POLYLINE" => {
                if self.points.len() < 2 || self.points.len() > 4096 {
                    return Err("DXF polylines need 2 to 4096 vertices".into());
                }
                if !self.points.iter().all(|p| p.valid()) {
                    return Err("DXF polyline vertex is out of range".into());
                }
                Ok(Some(DxfShape::Polyline {
                    points: self.points.clone(),
                    closed: self.closed,
                }))
            }
            "TEXT" => {
                if self.points.len() != 1 {
                    return Err("DXF text needs one anchor".into());
                }
                if !self.points[0].valid() {
                    return Err("DXF text anchor is out of range".into());
                }
                let height = number(self.radius, "text height")?;
                if height <= 0.0 || height > 10_000.0 {
                    return Err("DXF text height is out of range".into());
                }
                if self.value.len() > 1024 {
                    return Err("DXF text is too long".into());
                }
                Ok(Some(DxfShape::Text {
                    at: self.points[0],
                    height,
                    rotation: self.start_deg.unwrap_or(0.0),
                    value: self.value.clone(),
                }))
            }
            _ => Ok(None),
        }
    }
}

/// Read the drawable entities of an ASCII DXF drawing file. Entities on
/// unsupported entity types are skipped; malformed or oversized geometry is
/// rejected. Units are taken as millimetres, matching the exporter.
pub fn dxf_entities(text: &str) -> Result<Vec<DxfEntity>, String> {
    const MAX_ENTITIES: usize = 20_000;
    let mut out: Vec<DxfEntity> = Vec::new();
    let mut in_entities = false;
    let mut section = false;
    let mut acc: Option<DxfAcc> = None;
    let mut carried_layer: Option<String> = None;
    let flush = |acc: &mut Option<DxfAcc>, out: &mut Vec<DxfEntity>| -> Result<(), String> {
        let Some(a) = acc.take() else { return Ok(()) };
        if let Some(shape) = a.finish()? {
            if out.len() >= MAX_ENTITIES {
                return Err("DXF drawing exceeds 20000 entities".into());
            }
            out.push(DxfEntity {
                layer: a.layer.clone(),
                shape,
            });
        }
        Ok(())
    };
    let mut lines = text.lines().map(|l| l.trim_end_matches('\r'));
    while let Some(first) = lines.next() {
        let second = match lines.next() {
            Some(s) => s,
            None => return Err("DXF file has an odd number of lines".into()),
        };
        let Ok(what) = first.trim().parse::<i32>() else {
            return Err(format!("DXF group code \"{first}\" is not a number"));
        };
        let value = second.trim();
        match what {
            0 => {
                flush(&mut acc, &mut out)?;
                let name = value.to_string();
                if name == "SECTION" {
                    section = true;
                    in_entities = false;
                } else if name == "ENDSEC" || name == "ENDTAB" || name == "EOF" {
                    section = false;
                    in_entities = false;
                } else if in_entities && name != "ENTITY" {
                    acc = Some(DxfAcc {
                        layer: carried_layer.take().unwrap_or_default(),
                        name,
                        ..Default::default()
                    });
                }
            }
            2 if section => {
                in_entities = value == "ENTITIES";
                section = false;
            }
            _ if in_entities => {
                if acc.is_none() {
                    if what == 8 {
                        carried_layer = Some(value.to_string());
                    }
                    continue;
                }
                let Some(a) = acc.as_mut() else { continue };
                match what {
                    8 => a.layer = value.to_string(),
                    10 | 20 | 11 | 21 => {
                        let v = value
                            .parse::<f64>()
                            .ok()
                            .filter(|v| v.is_finite())
                            .ok_or_else(|| format!("DXF coordinate \"{value}\" is not a number"))?;
                        match what {
                            10 => a.pending_x = Some(v),
                            20 => a.pending_y = Some(v),
                            11 => a.end_x = Some(v),
                            _ => a.end_y = Some(v),
                        }
                        if let (Some(x), Some(y)) = (a.pending_x, a.pending_y) {
                            a.pending_x = None;
                            a.pending_y = None;
                            a.points.push(Point::new(x, y));
                        }
                        if let (Some(x), Some(y)) = (a.end_x, a.end_y) {
                            a.end_x = None;
                            a.end_y = None;
                            a.end = Some(Point::new(x, y));
                        }
                    }
                    40 => {
                        a.radius = value.parse::<f64>().ok().filter(|v| v.is_finite());
                    }
                    50 => {
                        a.start_deg = value.parse::<f64>().ok().filter(|v| v.is_finite());
                    }
                    51 => {
                        a.end_deg = value.parse::<f64>().ok().filter(|v| v.is_finite());
                    }
                    70 => {
                        a.closed = value
                            .parse::<i64>()
                            .map(|flags| flags & 1 == 1)
                            .unwrap_or(false);
                    }
                    1 => a.value = value.to_string(),
                    _ => {}
                }
            }
            _ => {}
        }
    }
    flush(&mut acc, &mut out)?;
    Ok(out)
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
        match &p.board.outline {
            Some(o) => {
                s.push_str("<polygon points=\"");
                for pt in o {
                    s.push_str(&format!("{},{} ", pt.x, pt.y));
                }
                s.push_str("\" fill=\"#193a36\" stroke=\"#5ae0bc\" stroke-width=\"0.15\"/>");
            }
            None => s.push_str(&format!("<rect width=\"{}\" height=\"{}\" rx=\"0.4\" fill=\"#193a36\" stroke=\"#5ae0bc\" stroke-width=\"0.15\"/>",p.board.width,p.board.height)),
        }
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
            let color = if t.layer == pcbcraft_doc::Layer::Front {
                "#f58d79"
            } else {
                "#74a7ec"
            };
            let d = match t.curve() {
                Some(arc) => format!(
                    "M {} {} A {:.6} {:.6} 0 {} {} {} {}",
                    t.start.x,
                    t.start.y,
                    arc.radius,
                    arc.radius,
                    usize::from(arc.sweep.abs() > std::f64::consts::PI),
                    usize::from(arc.sweep < 0.0),
                    t.end.x,
                    t.end.y
                ),
                None => format!("M {} {} L {} {}", t.start.x, t.start.y, t.end.x, t.end.y),
            };
            s.push_str(&format!("<path d=\"{d}\" fill=\"none\" stroke=\"{color}\" stroke-width=\"{}\" stroke-linecap=\"round\"/>",t.width));
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
            let color = if t.layer == pcbcraft_doc::Layer::Front {
                [245, 141, 121]
            } else {
                [116, 167, 236]
            };
            match t.curve() {
                Some(arc) => {
                    let steps = 16;
                    for i in 0..steps {
                        let a = to_pixel(arc.point_at(i as f64 / steps as f64));
                        let b = to_pixel(arc.point_at((i + 1) as f64 / steps as f64));
                        line(&mut image, a, b, t.width * scale / 2.0, color);
                    }
                }
                None => {
                    line(
                        &mut image,
                        to_pixel(t.start),
                        to_pixel(t.end),
                        t.width * scale / 2.0,
                        color,
                    );
                }
            }
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

// ---- Library text writers -------------------------------------------------

/// Quote a value as a library text token, or emit it bare when it is a
/// simple word.
fn lib_token(value: &str, default: &str) -> String {
    let value = if value.is_empty() { default } else { value };
    if value
        .chars()
        .any(|c| matches!(c, ' ' | '\t' | '\n' | '\r' | '"' | '(' | ')' | '\0'))
    {
        lib_text(value)
    } else {
        value.to_owned()
    }
}

/// Quote a value as a library text string with escapes the reader decodes.
fn lib_text(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {}
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn lib_xy(x: f64, y: f64) -> String {
    format!("{x} {y}")
}

/// Emit one artwork primitive. Library symbol bodies are stored y-up while
/// the file frame is y-down, so `flip` mirrors every ordinate back.
fn lib_graphic(graphic: &pcbcraft_library::Graphic, flip: bool) -> String {
    use pcbcraft_library::Graphic;
    let y = |v: f64| if flip { -v } else { v };
    match graphic {
        Graphic::Line {
            start,
            end,
            width,
            layer,
        } => format!(
            "(fp_line (start {}) (end {}) (stroke (width {width})) (layer {}))",
            lib_xy(start.x, y(start.y)),
            lib_xy(end.x, y(end.y)),
            lib_layer(layer)
        ),
        Graphic::Rectangle {
            start,
            end,
            width,
            fill,
            layer,
        } => format!(
            "(fp_rect (start {}) (end {}) (stroke (width {width})) (fill (type {})) (layer {}))",
            lib_xy(start.x, y(start.y)),
            lib_xy(end.x, y(end.y)),
            lib_token(fill, "none"),
            lib_layer(layer)
        ),
        Graphic::Circle {
            center,
            radius,
            width,
            fill,
            layer,
        } => format!(
            "(fp_circle (center {}) (end {}) (stroke (width {width})) (fill (type {})) (layer {}))",
            lib_xy(center.x, y(center.y)),
            lib_xy(center.x + radius, y(center.y)),
            lib_token(fill, "none"),
            lib_layer(layer)
        ),
        Graphic::Arc {
            start,
            mid,
            end,
            width,
            layer,
        } => format!(
            "(fp_arc (start {}) (mid {}) (end {}) (stroke (width {width})) (layer {}))",
            lib_xy(start.x, y(start.y)),
            lib_xy(mid.x, y(mid.y)),
            lib_xy(end.x, y(end.y)),
            lib_layer(layer)
        ),
        Graphic::Polyline {
            points,
            width,
            fill,
            layer,
        } => format!(
            "(fp_poly (pts {}) (stroke (width {width})) (fill (type {})) (layer {}))",
            points
                .iter()
                .map(|p| format!("(xy {} {})", p.x, y(p.y)))
                .collect::<Vec<_>>()
                .join(" "),
            lib_token(fill, "none"),
            lib_layer(layer)
        ),
        Graphic::Text {
            position,
            angle,
            text,
            size,
            layer,
        } => format!(
            "(fp_text graphics {} (at {} {} {angle}) (effects (font (size {} {}))) (layer {}))",
            lib_text(text),
            position.x,
            y(position.y),
            size.x,
            size.y,
            lib_layer(layer)
        ),
    }
}

fn lib_layer(layer: &str) -> String {
    lib_text(layer)
}

/// Format one footprint definition as a footprint library text file. The
/// result re-parses to the same pads, artwork, model reference and rule
/// overrides; placed footprint and board state are untouched.
pub fn footprint_library_text(
    footprint: &pcbcraft_library::FootprintDefinition,
) -> Result<String, String> {
    footprint
        .validate()
        .map_err(|e| format!("footprint is invalid: {e}"))?;
    let mut out = format!(
        "(footprint {}\n  (version 20260206)\n  (generator pcbcraft)\n  (layer \"F.Cu\")\n",
        lib_text(&footprint.name)
    );
    if !footprint.description.is_empty() {
        out.push_str(&format!("  (descr {})\n", lib_text(&footprint.description)));
    }
    if !footprint.keywords.is_empty() {
        out.push_str(&format!("  (tags {})\n", lib_text(&footprint.keywords)));
    }
    out.push_str("  (property \"Reference\" \"REF**\" (at 0 0 0) (layer \"F.SilkS\") (effects (font (size 1.27 1.27) (thickness 0.15))))\n");
    out.push_str(&format!(
        "  (property \"Value\" {} (at 0 0 0) (layer \"F.Fab\") (effects (font (size 1.27 1.27) (thickness 0.15))))\n",
        lib_text(&footprint.name)
    ));
    let smd = footprint.pads.iter().any(|p| p.pad_type == "smd");
    out.push_str(&format!(
        "  (attr {})\n",
        if smd { "smd" } else { "through_hole" }
    ));
    for pad in &footprint.pads {
        out.push_str(&format!(
            "  (pad {} {} {} (at {} {} {}) (size {} {})",
            lib_text(&pad.number),
            lib_token(&pad.pad_type, "smd"),
            lib_token(&pad.shape, "circle"),
            pad.position.x,
            pad.position.y,
            pad.rotation,
            pad.size.x,
            pad.size.y
        ));
        if let Some(drill) = &pad.drill {
            if drill.size.x != drill.size.y {
                out.push_str(&format!(" (drill oval {} {}", drill.size.x, drill.size.y));
            } else {
                out.push_str(&format!(" (drill {}", drill.size.x));
            }
            if drill.offset.x != 0.0 || drill.offset.y != 0.0 {
                out.push_str(&format!(" (offset {} {})", drill.offset.x, drill.offset.y));
            }
            out.push(')');
        }
        out.push_str(" (layers");
        for layer in &pad.layers {
            out.push_str(&format!(" {}", lib_text(layer)));
        }
        out.push(')');
        if let Some(ratio) = pad.roundrect_ratio {
            out.push_str(&format!(" (roundrect_rratio {ratio})"));
        }
        out.push_str(")\n");
    }
    for graphic in &footprint.graphics {
        out.push_str(&format!("  {}\n", lib_graphic(graphic, false)));
    }
    if let Some(model) = &footprint.model {
        let triple = |v: &[f64; 3]| format!("(xyz {} {} {})", v[0], v[1], v[2]);
        out.push_str(&format!(
            "  (model {} (offset {}) (scale {}) (rotate {}))\n",
            lib_text(&model.path),
            triple(&model.offset),
            triple(&model.scale),
            triple(&model.rotate)
        ));
    }
    if let Some(rules) = &footprint.rules {
        if let Some(value) = rules.clearance {
            out.push_str(&format!("  (clearance {value})\n"));
        }
        if let Some(value) = rules.mask_margin {
            out.push_str(&format!("  (solder_mask_margin {value})\n"));
        }
        if let Some(value) = rules.paste_margin {
            out.push_str(&format!("  (solder_paste_margin {value})\n"));
        }
    }
    out.push_str("  (embedded_fonts no)\n)\n");
    if out.len() as u64 > MAX_FILE_BYTES {
        return Err("footprint library text exceeds the file size limit".into());
    }
    Ok(out)
}

/// Format one symbol definition as a symbol library text file. Pins and
/// artwork regroup into unit/body sub-symbols and the stored y-up drawing
/// coordinates mirror back to the file frame; the result re-parses to the
/// same pins and artwork.
pub fn symbol_library_text(symbol: &pcbcraft_library::SymbolDefinition) -> Result<String, String> {
    symbol
        .validate()
        .map_err(|e| format!("symbol is invalid: {e}"))?;
    let mut out = String::from("(kicad_symbol_lib (version 20260206) (generator pcbcraft)\n");
    out.push_str(&format!("  (symbol {}\n", lib_text(&symbol.name)));
    for (key, value) in [
        ("Reference", &symbol.reference),
        ("Value", &symbol.value),
        ("Description", &symbol.description),
        ("ki_keywords", &symbol.keywords),
        ("Footprint", &symbol.footprint),
    ] {
        if !value.is_empty() {
            out.push_str(&format!(
                "    (property \"{key}\" {} (at 0 0 0) (effects (font (size 1.27 1.27) (thickness 0.15))))\n",
                lib_text(value)
            ));
        }
    }
    let mut units: std::collections::BTreeSet<(u32, u32)> =
        symbol.pins.iter().map(|p| (p.unit, p.body_style)).collect();
    units.extend(symbol.graphics.iter().map(|g| (g.unit, g.body_style)));
    for (unit, body_style) in units {
        out.push_str(&format!(
            "    (symbol {}\n",
            lib_text(&format!("{}_{}_{}", symbol.name, unit, body_style))
        ));
        for pin in symbol
            .pins
            .iter()
            .filter(|p| p.unit == unit && p.body_style == body_style)
        {
            out.push_str(&format!(
                "      (pin {} line (at {} {} {}) (length {}) (name {}) (number {}))\n",
                lib_token(&pin.electrical_type, "passive"),
                pin.position.x,
                -pin.position.y,
                pin.angle,
                pin.length,
                lib_text(&pin.name),
                lib_text(&pin.number)
            ));
        }
        for graphic in symbol
            .graphics
            .iter()
            .filter(|g| g.unit == unit && g.body_style == body_style)
        {
            out.push_str(&format!(
                "      {}\n",
                lib_graphic(&graphic.primitive, true)
            ));
        }
        out.push_str("    )\n");
    }
    out.push_str("  )\n)\n");
    if out.len() as u64 > MAX_FILE_BYTES {
        return Err("symbol library text exceeds the file size limit".into());
    }
    Ok(out)
}

/// Render one library footprint as a standalone SVG preview: copper pads
/// with their holes, courtyard and other artwork, pad numbers, all in
/// millimetre coordinates drawn y-up so the view matches the board.
pub fn footprint_svg(footprint: &pcbcraft_library::FootprintDefinition) -> Result<String, String> {
    footprint
        .validate()
        .map_err(|e| format!("footprint is invalid: {e}"))?;
    let mut body = String::new();
    let mut box_min = Point::new(-1.0, -1.0);
    let mut box_max = Point::new(1.0, 1.0);
    let mut grow = |p: Point| {
        box_min.x = box_min.x.min(p.x);
        box_min.y = box_min.y.min(p.y);
        box_max.x = box_max.x.max(p.x);
        box_max.y = box_max.y.max(p.y);
    };
    for g in &footprint.graphics {
        lib_svg_graphic(g, &mut body, &mut grow);
    }
    for pad in &footprint.pads {
        let hx = pad.size.x / 2.0;
        let hy = pad.size.y / 2.0;
        let spin = |p: Point| pad.position.offset(p.rotate(-pad.rotation));
        let radius = match pad.shape.as_str() {
            "circle" | "oval" => hx.min(hy),
            "roundrect" => pad.size.x.min(pad.size.y) * pad.roundrect_ratio.unwrap_or(0.0),
            _ => 0.0,
        };
        if pad.shape == "circle" {
            grow(Point::new(
                pad.position.x + hx.max(hy),
                pad.position.y + hx.max(hy),
            ));
            grow(Point::new(
                pad.position.x - hx.max(hy),
                pad.position.y - hx.max(hy),
            ));
            body.push_str(&format!(
                "<circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"#b87333\"/>\n",
                svg_num(pad.position.x),
                svg_num(pad.position.y),
                svg_num(hx.max(hy))
            ));
        } else if radius <= 1e-9 {
            for corner in [
                Point::new(-hx, -hy),
                Point::new(hx, -hy),
                Point::new(hx, hy),
                Point::new(-hx, hy),
            ] {
                grow(spin(corner));
            }
            body.push_str(&format!(
                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"#b87333\" transform=\"rotate({} {} {})\"/>\n",
                svg_num(-hx),
                svg_num(-hy),
                svg_num(pad.size.x),
                svg_num(pad.size.y),
                svg_num(pad.rotation),
                svg_num(pad.position.x),
                svg_num(pad.position.y)
            ));
        } else {
            let mut points = Vec::new();
            for (x, y, start) in [
                (hx - radius, hy - radius, 0.0),
                (-hx + radius, hy - radius, 90.0),
                (-hx + radius, -hy + radius, 180.0),
                (hx - radius, -hy + radius, 270.0),
            ] {
                for i in 0..=16 {
                    let angle = (start + f64::from(i) * 90.0 / 16.0).to_radians();
                    points.push(spin(Point::new(
                        x + radius * angle.cos(),
                        y + radius * angle.sin(),
                    )));
                }
            }
            for p in &points {
                grow(*p);
            }
            let outline = points
                .iter()
                .map(|p| format!("{},{}", svg_num(p.x), svg_num(p.y)))
                .collect::<Vec<_>>()
                .join(" ");
            body.push_str(&format!(
                "<polygon points=\"{outline}\" fill=\"#b87333\"/>\n"
            ));
        }
        if let Some(drill) = &pad.drill {
            let dr = (drill.size.x.max(drill.size.y)) / 2.0;
            let center = pad.position.offset(drill.offset.rotate(-pad.rotation));
            grow(Point::new(center.x + dr, center.y + dr));
            grow(Point::new(center.x - dr, center.y - dr));
            if drill.size.x != drill.size.y {
                let along = Point::new(drill.size.x.max(drill.size.y) / 2.0 - dr, 0.0)
                    .rotate(-pad.rotation);
                body.push_str(&format!(
                    "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"#202020\" stroke-width=\"{}\" stroke-linecap=\"round\"/>\n",
                    svg_num(center.x - along.x),
                    svg_num(center.y - along.y),
                    svg_num(center.x + along.x),
                    svg_num(center.y + along.y),
                    svg_num(dr * 2.0)
                ));
            } else {
                body.push_str(&format!(
                    "<circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"#202020\"/>\n",
                    svg_num(center.x),
                    svg_num(center.y),
                    svg_num(dr)
                ));
            }
        }
        if !pad.number.is_empty() {
            body.push_str(&format!(
                "<text x=\"{}\" y=\"{}\" font-size=\"{}\" text-anchor=\"middle\" fill=\"#202020\">{}</text>\n",
                svg_num(pad.position.x),
                svg_num(pad.position.y),
                svg_num(hx.min(hy).clamp(0.3, 1.0) * 1.2),
                lib_svg_escape(&pad.number)
            ));
        }
    }
    Ok(svg_document(&footprint.name, box_min, box_max, body))
}

/// Render one library symbol as a standalone SVG preview: body and unit
/// artwork with pin lines and pin numbers, in millimetre coordinates drawn
/// y-up like the schematic sheet.
pub fn symbol_svg(symbol: &pcbcraft_library::SymbolDefinition) -> Result<String, String> {
    if symbol.name.trim().is_empty() {
        return Err("symbol is missing a name".into());
    }
    let mut body = String::new();
    let mut box_min = Point::new(-1.0, -1.0);
    let mut box_max = Point::new(1.0, 1.0);
    let mut grow = |p: Point| {
        box_min.x = box_min.x.min(p.x);
        box_min.y = box_min.y.min(p.y);
        box_max.x = box_max.x.max(p.x);
        box_max.y = box_max.y.max(p.y);
    };
    for g in symbol.graphics.iter().filter(|g| g.body_style <= 1) {
        lib_svg_graphic(&g.primitive, &mut body, &mut grow);
    }
    for pin in symbol.pins.iter().filter(|pin| pin.body_style <= 1) {
        grow(pin.position);
        let angle = pin.angle.to_radians();
        let end = pin.position.offset(Point::new(
            pin.length * angle.cos(),
            -pin.length * angle.sin(),
        ));
        grow(end);
        body.push_str(&format!(
            "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"black\" stroke-width=\"0.25\"/>\n",
            svg_num(pin.position.x),
            svg_num(pin.position.y),
            svg_num(end.x),
            svg_num(end.y)
        ));
        body.push_str(&format!(
            "<text x=\"{}\" y=\"{}\" font-size=\"0.8\" text-anchor=\"middle\" fill=\"black\">{}</text>\n",
            svg_num(pin.position.x),
            svg_num(pin.position.y),
            lib_svg_escape(&pin.number)
        ));
    }
    Ok(svg_document(&symbol.name, box_min, box_max, body))
}

fn svg_num(v: f64) -> String {
    let r = (v * 1000.0).round() / 1000.0;
    let r = if r == 0.0 { 0.0 } else { r };
    r.to_string()
}

fn lib_svg_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn lib_svg_graphic(g: &pcbcraft_library::Graphic, body: &mut String, grow: &mut impl FnMut(Point)) {
    match g {
        pcbcraft_library::Graphic::Line {
            start, end, width, ..
        } => {
            grow(*start);
            grow(*end);
            body.push_str(&format!(
                "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"black\" stroke-width=\"{}\"/>\n",
                svg_num(start.x),
                svg_num(start.y),
                svg_num(end.x),
                svg_num(end.y),
                svg_num(width.max(0.12))
            ));
        }
        pcbcraft_library::Graphic::Rectangle {
            start,
            end,
            width,
            fill,
            ..
        } => {
            grow(*start);
            grow(*end);
            body.push_str(&format!(
                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\" stroke=\"black\" stroke-width=\"{}\"/>\n",
                svg_num(start.x.min(end.x)),
                svg_num(start.y.min(end.y)),
                svg_num((end.x - start.x).abs()),
                svg_num((end.y - start.y).abs()),
                svg_fill(fill),
                svg_num(width.max(0.12))
            ));
        }
        pcbcraft_library::Graphic::Circle {
            center,
            radius,
            width,
            fill,
            ..
        } => {
            grow(Point::new(center.x + radius, center.y + radius));
            grow(Point::new(center.x - radius, center.y - radius));
            body.push_str(&format!(
                "<circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"{}\" stroke=\"black\" stroke-width=\"{}\"/>\n",
                svg_num(center.x),
                svg_num(center.y),
                svg_num(*radius),
                svg_fill(fill),
                svg_num(width.max(0.12))
            ));
        }
        pcbcraft_library::Graphic::Arc {
            start,
            mid,
            end,
            width,
            ..
        } => {
            grow(*start);
            grow(*mid);
            grow(*end);
            let (center, radius) = match svg_arc_center(start, mid, end) {
                Some(v) => v,
                None => return,
            };
            let sweep = if lib_svg_arc_ccw(&center, start, mid, end) {
                1
            } else {
                0
            };
            body.push_str(&format!(
                "<path d=\"M {},{} A {},{} 0 0 {},{} {}\" fill=\"none\" stroke=\"black\" stroke-width=\"{}\"/>\n",
                svg_num(start.x),
                svg_num(start.y),
                svg_num(radius),
                svg_num(radius),
                sweep,
                svg_num(end.x),
                svg_num(end.y),
                svg_num(width.max(0.12))
            ));
        }
        pcbcraft_library::Graphic::Polyline {
            points,
            width,
            fill,
            ..
        } => {
            if points.len() < 2 {
                return;
            }
            for p in points {
                grow(*p);
            }
            let list = points
                .iter()
                .map(|p| format!("{},{}", svg_num(p.x), svg_num(p.y)))
                .collect::<Vec<_>>()
                .join(" ");
            body.push_str(&format!(
                "<polyline points=\"{list}\" fill=\"{}\" stroke=\"black\" stroke-width=\"{}\"/>\n",
                svg_fill(fill),
                svg_num(width.max(0.12))
            ));
        }
        pcbcraft_library::Graphic::Text {
            position,
            text,
            size,
            ..
        } => {
            grow(*position);
            body.push_str(&format!(
                "<text x=\"{}\" y=\"{}\" font-size=\"{}\" fill=\"black\">{}</text>\n",
                svg_num(position.x),
                svg_num(position.y),
                svg_num(size.x.max(0.5)),
                lib_svg_escape(text)
            ));
        }
    }
}

fn svg_fill(fill: &str) -> &'static str {
    if fill.is_empty() || fill.eq_ignore_ascii_case("none") || fill.eq_ignore_ascii_case("solid") {
        "none"
    } else {
        "black"
    }
}

fn svg_arc_center(start: &Point, mid: &Point, end: &Point) -> Option<(Point, f64)> {
    let d =
        2.0 * (start.x * (mid.y - end.y) + mid.x * (end.y - start.y) + end.x * (start.y - mid.y));
    if d.abs() < 1e-12 {
        return None;
    }
    let start2 = start.x * start.x + start.y * start.y;
    let mid2 = mid.x * mid.x + mid.y * mid.y;
    let end2 = end.x * end.x + end.y * end.y;
    let ux = (start2 * (mid.y - end.y) + mid2 * (end.y - start.y) + end2 * (start.y - mid.y)) / d;
    let uy = (start2 * (end.x - mid.x) + mid2 * (start.x - end.x) + end2 * (mid.x - start.x)) / d;
    let center = Point::new(ux, uy);
    let radius = ((center.x - start.x).powi(2) + (center.y - start.y).powi(2)).sqrt();
    Some((center, radius))
}

fn lib_svg_arc_ccw(center: &Point, start: &Point, mid: &Point, end: &Point) -> bool {
    let cross = |a: &Point, b: &Point| {
        (a.x - center.x) * (b.y - center.y) - (a.y - center.y) * (b.x - center.x)
    };
    let quarter = |a: &Point, b: &Point| {
        let dot = (a.x - center.x) * (b.x - center.x) + (a.y - center.y) * (b.y - center.y);
        cross(a, b) >= 0.0 && dot >= 0.0
    };
    if quarter(start, mid) {
        cross(start, mid) >= 0.0
    } else if quarter(mid, end) {
        cross(mid, end) >= 0.0
    } else {
        cross(start, end) >= 0.0
    }
}

fn svg_document(name: &str, box_min: Point, box_max: Point, body: String) -> String {
    let margin = 1.0;
    let width = (box_max.x - box_min.x + 2.0 * margin) * 10.0;
    let height = (box_max.y - box_min.y + 2.0 * margin) * 10.0;
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}mm\" height=\"{}mm\" viewBox=\"{} {} {} {}\">\n<title>{}</title>\n<g transform=\"scale(1,-1)\">\n{body}</g>\n</svg>\n",
        svg_num(width),
        svg_num(height),
        svg_num((box_min.x - margin) * 10.0),
        svg_num(-(box_max.y + margin) * 10.0),
        svg_num(width),
        svg_num(height),
        lib_svg_escape(name)
    )
}

#[cfg(test)]
mod tests {

    use super::*;
    #[test]
    fn archive_round_trips_a_project_and_its_attached_files() -> Result<(), String> {
        let mut project = Project {
            library_table: Vec::new(),
            padstacks: Vec::new(),
            name: "Archive test".into(),
            ..Project::default()
        };
        project.files.push(pcbcraft_doc::EmbeddedFile {
            name: "notes.txt".into(),
            data: "hello from the archive".into(),
        });
        let zip = archive_zip(&project)?;
        assert!(
            zip.starts_with(b"PK\x03\x04"),
            "a ZIP starts with a local header"
        );
        assert!(zip.windows(4).any(|w| w == b"PK\x05\x06"));
        assert_eq!(
            crc32(b"123456789"),
            0xcbf4_3926,
            "the CRC must match the value every unzip tool expects"
        );
        let back = archive_project(&zip)?;
        assert_eq!(back, project, "the archive must restore the exact design");
        assert!(archive_project(b"not a zip at all").is_err());
        let mut truncated = zip.clone();
        truncated.truncate(zip.len() - 30);
        assert!(archive_project(&truncated).is_err());
        let mut wrong = zip.clone();
        let at = wrong
            .windows(11)
            .position(|w| w == b"design.json")
            .ok_or("entry name missing")?;
        wrong[at] = b"x"[0];
        assert!(
            archive_project(&wrong).is_err(),
            "a renamed design is not found"
        );
        Ok(())
    }

    #[test]
    fn pdf_documents_render_both_views_with_valid_cross_references() -> Result<(), String> {
        let mut project = Project::default();
        project.board.tracks.push(pcbcraft_doc::Track {
            id: 101,
            net: 1,
            start: Point::new(5.0, 5.0),
            end: Point::new(40.0, 30.0),
            width: 0.3,
            layer: pcbcraft_doc::Layer::Front,
            arc: Some(Point::new(20.0, 20.0)),
        });
        project.board.vias.push(pcbcraft_doc::Via {
            id: 102,
            net: 1,
            position: Point::new(40.0, 30.0),
            diameter: 0.6,
            drill: 0.3,
        });
        project.board.texts.push(pcbcraft_doc::BoardText {
            id: 103,
            text: "GND (pour) 100%".into(),
            position: Point::new(10.0, 10.0),
            rotation: 0.0,
            size: 1.0,
            layer: pcbcraft_doc::Layer::Front,
        });
        project.drawn_wires.push(pcbcraft_doc::DrawnWire {
            id: 104,
            net: None,
            points: vec![Point::new(10.0, 5.0), Point::new(20.0, 5.0)],
        });
        project.nets.push(pcbcraft_doc::Net {
            id: 1,
            name: "GND".into(),
            ..Default::default()
        });
        project.labels.push(pcbcraft_doc::Label {
            id: 105,
            kind: "global".into(),
            net: 1,
            position: Point::new(10.0, 5.0),
            rotation: 0.0,
            text: String::new(),
        });
        project.validate()?;
        for view in ["pcb", "schematic"] {
            let pdf = pdf_document(&project, view)?;
            assert!(pdf.starts_with(b"%PDF-1.4"), "a PDF names its version");
            assert!(pdf.windows(6).any(|w| w == b"%%EOF\n"));
            let text = String::from_utf8(pdf.clone()).map_err(|_| "PDFs are ASCII")?;
            let xref_at = text.find("\nxref\n").map(|at| at + 1).unwrap_or(usize::MAX);
            let stated: usize = text
                .rsplit_once("startxref\n")
                .map(|(_, tail)| tail.lines().next().unwrap_or_default())
                .and_then(|line| line.trim().parse().ok())
                .unwrap_or(usize::MAX);
            assert_eq!(
                stated, xref_at,
                "the trailer points exactly at the cross-reference table"
            );
            assert_eq!(
                &text[xref_at..xref_at + 4],
                "xref",
                "the cross-reference table sits where the trailer says"
            );
            let entries: Vec<usize> = text[xref_at..]
                .lines()
                .skip(2)
                .take(6)
                .filter_map(|l| l.split(' ').next().and_then(|v| v.parse().ok()))
                .collect();
            assert_eq!(
                entries.len(),
                6,
                "the free slot and the five objects are listed"
            );
            assert_eq!(entries[0], 0, "the first slot is the free entry");
            for (index, offset) in entries[1..].iter().enumerate() {
                assert!(
                    text[*offset..].starts_with(&format!("{} 0 obj", index + 1)),
                    "cross-reference entry {} points at object {}",
                    index + 1,
                    index + 1
                );
            }
            assert!(text.contains("BT /F1"));
            if view == "pcb" {
                assert!(
                    text.contains("(GND \\(pour\\) 100%)"),
                    "board text lands on the page with brackets escaped"
                );
            } else {
                assert!(
                    text.contains("({GND})"),
                    "the schematic page draws the global label"
                );
            }
        }
        assert!(pdf_document(&project, "layout").is_err());
        assert!(pdf_document(&Project::default(), "pcb")?.starts_with(b"%PDF-1.4"));
        Ok(())
    }

    #[test]
    fn step_board_extrudes_outline_and_pads_with_intact_references() -> Result<(), String> {
        let project = real_library_project()?;
        let step = step_board(&project)?;
        assert!(step.starts_with("ISO-10303-21;\nHEADER;"));
        assert!(step.ends_with("ENDSEC;\nEND-ISO-10303-21;\n"));
        assert!(step.contains("AUTOMOTIVE_DESIGN { 1 0 10303 214 1 1 1 1 }"));
        let data = step
            .split_once("DATA;\n")
            .ok_or("data section missing")?
            .1
            .split_once("ENDSEC;")
            .ok_or("data section unterminated")?
            .0;
        let mut defined = std::collections::BTreeSet::new();
        for line in data.lines() {
            if let Some(rest) = line.strip_prefix('#') {
                let id: u32 = rest
                    .split_whitespace()
                    .next()
                    .and_then(|t| t.split_whitespace().next())
                    .unwrap_or_default()
                    .parse()
                    .map_err(|_| "entity ids must be numbers")?;
                if !defined.insert(id) {
                    return Err("duplicate entity ids".into());
                }
            }
        }
        assert_eq!(defined.iter().next(), Some(&1), "numbering starts at one");
        assert_eq!(
            defined.len(),
            *defined.iter().next_back().unwrap_or(&0) as usize,
            "every id up to the highest is defined"
        );
        let mut references = Vec::new();
        let bytes = data.as_bytes();
        let mut index = 0;
        while index < bytes.len() {
            if bytes[index] == b'#' {
                let mut end = index + 1;
                while end < bytes.len() && bytes[end].is_ascii_digit() {
                    end += 1;
                }
                let id: u32 = data[index + 1..end].parse().map_err(|_| "bad reference")?;
                references.push(id);
                index = end;
            } else {
                index += 1;
            }
        }
        assert!(
            references.iter().all(|r| defined.contains(r)),
            "every reference names a defined entity"
        );
        let faces = data.matches("ADVANCED_FACE(").count();
        let loops = data.matches("EDGE_LOOP(").count();
        assert_eq!(faces, loops, "every face has exactly one outer bound");
        assert!(
            faces >= 18,
            "the outline and both pads close at least six faces each"
        );
        assert!(data.contains("CARTESIAN_POINT('',(0.0000,0.0000,0.0000))"));
        let ring = vec![Point::new(0.0, 0.0); 100];
        assert!(
            thin_ring(&ring).len() <= 32,
            "long rounded rings are thinned for one file"
        );
        Ok(())
    }

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
                castellated: None,
                geometry: Some(p.clone()),
            })
            .collect();
        let mut project = Project::default();
        project.components.push(pcbcraft_doc::Component {
            sheet: 0,
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
            locked: None,
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
                castellated: None,
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

            kind: None,
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
    fn inch_projects_export_imperial_fabrication_files() -> Result<(), String> {
        let mut project = Project::default();
        project.nets.push(pcbcraft_doc::Net {
            id: 8,
            name: "GND".into(),
            ..Default::default()
        });
        project.board.tracks.push(pcbcraft_doc::Track {
            id: 3,
            net: 8,
            start: Point::new(25.4, 25.4),
            end: Point::new(50.8, 25.4),
            width: 0.508,
            layer: pcbcraft_doc::Layer::Back,
            arc: None,
        });
        project.board.vias.push(pcbcraft_doc::Via {
            id: 4,
            net: 8,
            position: Point::new(50.8, 25.4),
            diameter: 0.6,
            drill: 0.3,
        });
        project.unit = "inch".into();
        project.validate()?;
        let files = gerber_files(&project)?;
        let back = &files["pcb-B.Cu.gbr"];
        assert!(
            back.contains("MOIN*%"),
            "the format statement declares inches"
        );
        assert!(
            back.contains("X-1.000000Y1.000000D02*"),
            "25.4 mm becomes exactly one inch, mirrored on the back: {back}"
        );
        assert!(
            back.contains("C,0.020000*%"),
            "a 0.508 mm track is a 0.02 inch flash: {back}"
        );
        let drill = &files["pcb-Drill.drl"];
        assert!(
            drill.starts_with("M48\nFMAT,2\nINCH,TZ\n"),
            "the drill header declares inches: {drill}"
        );
        assert!(
            drill.contains("T1C0.0118\n"),
            "a 0.3 mm bit is a 0.0118 inch bit"
        );
        assert!(
            drill.contains("X2.0000Y1.0000\n"),
            "the hit lands at exactly two inches by one inch: {drill}"
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
            arc: None,
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

            kind: None,
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
            castellated: None,
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
            locked: None,
            pads: vec![through_pad, pad2],
        });
        project.components.push(pcbcraft_doc::Component {
            sheet: 0,
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
                    model: None,
                    rules: None,
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
            locked: None,
            pads: vec![pcbcraft_doc::Pad {
                number: "1".into(),
                offset: Point::new(0.0, 0.0),
                diameter: 1.6,
                net: None,
                castellated: None,
                geometry: Some(oval_geometry()),
            }],
        };
        for id in 1..=2u64 {
            project.components.push(pcbcraft_doc::Component {
                sheet: 0,
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
                        model: None,
                        rules: None,
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
    fn placement_text_lists_placed_parts_by_reference_with_side_and_rotation() -> Result<(), String>
    {
        let mut project = Project::default();
        let geometry = |number: &str, x: f64, back: bool| pcbcraft_library::FootprintPad {
            number: number.into(),
            pad_type: if back { "thru_hole" } else { "smd" }.into(),
            shape: "rect".into(),
            position: Point::new(x, 0.0),
            rotation: 0.0,
            size: Point::new(1.0, 1.0),
            layers: if back {
                vec![
                    "F.Cu".into(),
                    "B.Cu".into(),
                    "F.Mask".into(),
                    "B.Mask".into(),
                ]
            } else {
                vec!["F.Cu".into(), "F.Mask".into()]
            },
            drill: back.then(|| pcbcraft_library::Drill {
                size: Point::new(0.5, 0.5),
                offset: Point::default(),
            }),
            roundrect_ratio: None,
            raw_source: String::new(),
        };
        let component =
            |id: u64, reference: &str, definition: &str, back: bool| pcbcraft_doc::Component {
                sheet: 0,
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
                library_footprint: Some(pcbcraft_doc::LibraryFootprint {
                    library: "Test".into(),
                    definition: pcbcraft_library::FootprintDefinition {
                        name: definition.into(),
                        description: String::new(),
                        keywords: String::new(),
                        pads: vec![geometry("1", -1.0, back), geometry("2", 1.0, back)],
                        graphics: vec![],
                        model: None,
                        rules: None,
                        raw_source: String::new(),
                        unsupported: vec![],
                    },
                }),
                fields: vec![],
            };
        let footprint = |id: u64, component: u64, x: f64, y: f64, rotation: f64, back: bool| {
            pcbcraft_doc::Footprint {
                id,
                component,
                position: Point::new(x, y),
                rotation,
                locked: None,
                pads: vec![
                    pcbcraft_doc::Pad {
                        number: "1".into(),
                        offset: Point::new(-1.0, 0.0),
                        diameter: 1.0,
                        net: None,
                        castellated: None,
                        geometry: Some(geometry("1", -1.0, back)),
                    },
                    pcbcraft_doc::Pad {
                        number: "2".into(),
                        offset: Point::new(1.0, 0.0),
                        diameter: 1.0,
                        net: None,
                        castellated: None,
                        geometry: Some(geometry("2", 1.0, back)),
                    },
                ],
            }
        };
        project.components.push(component(1, "R5", "R0603", false));
        project.components.push(component(2, "U1", "SOIC-2", true));
        project
            .board
            .footprints
            .push(footprint(10, 1, 12.0, 8.0, 90.0, false));
        project
            .board
            .footprints
            .push(footprint(11, 2, 20.0, 15.0, -45.0, true));
        project.validate()?;
        let text = placement_text(&project)?;
        assert!(
            text.starts_with("File Format,ASCII,\n"),
            "a centroid file starts with its format line"
        );
        assert!(
            text.contains("SMT,THT,X,Y,Rotation,Side,Package\n"),
            "the column header names the fields"
        );
        let rows: Vec<&str> = text
            .lines()
            .filter(|l| l.starts_with("SMT,\"") || l.starts_with("THT,\""))
            .collect();
        assert_eq!(
            rows.first(),
            Some(&"SMT,\"R5\",12.000,8.000,90.0,T,\"R0603\""),
            "front parts carry their position, rotation and package"
        );
        assert_eq!(
            rows.last(),
            Some(&"THT,\"U1\",20.000,15.000,315.0,T,\"SOIC-2\""),
            "plated through-hole pads are listed as THT and the rotation wraps positive"
        );
        assert_eq!(rows.len(), 2, "one row per placed part");
        Ok(())
    }
    #[test]
    fn cmp_text_lists_every_pad_and_connected_net_in_mils() -> Result<(), String> {
        let mut project = Project::default();
        let geometry = |number: &str, x: f64, back: bool| pcbcraft_library::FootprintPad {
            number: number.into(),
            pad_type: if back { "thru_hole" } else { "smd" }.into(),
            shape: "rect".into(),
            position: Point::new(x, 0.0),
            rotation: 0.0,
            size: Point::new(1.0, 1.0),
            layers: if back {
                vec![
                    "F.Cu".into(),
                    "B.Cu".into(),
                    "F.Mask".into(),
                    "B.Mask".into(),
                ]
            } else {
                vec!["F.Cu".into(), "F.Mask".into()]
            },
            drill: back.then(|| pcbcraft_library::Drill {
                size: Point::new(0.5, 0.5),
                offset: Point::default(),
            }),
            roundrect_ratio: None,
            raw_source: String::new(),
        };
        let component =
            |id: u64, reference: &str, definition: &str, back: bool| pcbcraft_doc::Component {
                sheet: 0,
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
                        net: Some(50),
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
                library_footprint: Some(pcbcraft_doc::LibraryFootprint {
                    library: "Test".into(),
                    definition: pcbcraft_library::FootprintDefinition {
                        name: definition.into(),
                        description: String::new(),
                        keywords: String::new(),
                        pads: vec![geometry("1", -1.0, back), geometry("2", 1.0, back)],
                        graphics: vec![],
                        model: None,
                        rules: None,
                        raw_source: String::new(),
                        unsupported: vec![],
                    },
                }),
                fields: vec![],
            };
        let footprint = |id: u64, component: u64, x: f64, y: f64, rotation: f64, back: bool| {
            pcbcraft_doc::Footprint {
                id,
                component,
                position: Point::new(x, y),
                rotation,
                locked: None,
                pads: vec![
                    pcbcraft_doc::Pad {
                        number: "1".into(),
                        offset: Point::new(-1.0, 0.0),
                        diameter: 1.0,
                        net: Some(50),
                        castellated: None,
                        geometry: Some(geometry("1", -1.0, back)),
                    },
                    pcbcraft_doc::Pad {
                        number: "2".into(),
                        offset: Point::new(1.0, 0.0),
                        diameter: 1.0,
                        net: None,
                        castellated: None,
                        geometry: Some(geometry("2", 1.0, back)),
                    },
                ],
            }
        };
        project.nets.push(pcbcraft_doc::Net {
            id: 50,
            name: "PWR".into(),
            target_length: None,
            length_tolerance: None,
        });
        project.components.push(component(1, "R5", "R0603", false));
        project.components.push(component(2, "U1", "SOIC-2", true));
        project
            .board
            .footprints
            .push(footprint(10, 1, 12.0, 8.0, 0.0, false));
        project
            .board
            .footprints
            .push(footprint(11, 2, 20.0, 15.0, 0.0, true));
        project.validate()?;
        let text = cmp_text(&project)?;
        assert!(
            text.starts_with("$FILE\nTYPE:BOARD\nCODE:ASCII\n"),
            "a trace file starts with its header"
        );
        let pads: Vec<&str> = text.lines().filter(|l| l.starts_with("$CD ")).collect();
        assert_eq!(pads.len(), 4, "one line per placed pad");
        assert!(
            pads.contains(&"$CD R5\t1\t433\t315\t0\t1\t0"),
            "front pad one of R5 sits at 11 mm, 8 mm in whole mils: {pads:?}"
        );
        assert!(
            pads.iter()
                .any(|l| l.starts_with("$CD U1\t1\t") && l.contains("\t1\t")),
            "back-side pads carry the back layer code"
        );
        let net_line = text
            .lines()
            .find(|l| l.starts_with("PWR\t"))
            .ok_or("the named net is missing")?;
        assert!(
            net_line.starts_with("PWR\t2\t1\t1\t2\t1"),
            "the net lists both footprints' first pad: {net_line}"
        );
        assert!(
            text.ends_with("$NETS\n") || text.contains("$NETS"),
            "sections close"
        );
        assert_eq!(
            cmp_text(&Project::default()).err(),
            Some("board has no placed pads or connected nets to export".into()),
            "an empty board is rejected"
        );
        Ok(())
    }
    #[test]
    fn idf_board_carries_size_and_placed_footprints_for_mechanical_tools() -> Result<(), String> {
        let mut project = Project::default();
        project.board.width = 60.0;
        project.board.height = 40.0;
        let geometry = |number: &str, x: f64| pcbcraft_library::FootprintPad {
            number: number.into(),
            pad_type: "smd".into(),
            shape: "rect".into(),
            position: Point::new(x, 0.0),
            rotation: 0.0,
            size: Point::new(1.0, 1.0),
            layers: vec!["F.Cu".into()],
            drill: None,
            roundrect_ratio: None,
            raw_source: String::new(),
        };
        project.components.push(pcbcraft_doc::Component {
            sheet: 0,
            id: 1,
            reference: "R5".into(),
            value: "1k".into(),
            kind: "resistor".into(),
            position: Point::new(10.0, 10.0),
            rotation: 0.0,
            pins: vec![pcbcraft_doc::Pin {
                number: "1".into(),
                name: String::new(),
                offset: Point::new(-1.0, 0.0),
                net: None,
                electrical_type: "passive".into(),
                direction: None,
                unit: 1,
            }],
            units: vec![],
            library_symbol: None,
            library_footprint: Some(pcbcraft_doc::LibraryFootprint {
                library: "Test".into(),
                definition: pcbcraft_library::FootprintDefinition {
                    name: "TEST_ONE".into(),
                    description: String::new(),
                    keywords: String::new(),
                    pads: vec![geometry("1", -1.0)],
                    graphics: vec![],
                    model: None,
                    rules: None,
                    raw_source: String::new(),
                    unsupported: vec![],
                },
            }),
            fields: vec![],
        });
        project.components.push(pcbcraft_doc::Component {
            sheet: 0,
            id: 2,
            reference: "U1".into(),
            value: "1k".into(),
            kind: "resistor".into(),
            position: Point::new(20.0, 10.0),
            rotation: 0.0,
            pins: vec![pcbcraft_doc::Pin {
                number: "1".into(),
                name: String::new(),
                offset: Point::new(-1.0, 0.0),
                net: None,
                electrical_type: "passive".into(),
                direction: None,
                unit: 1,
            }],
            units: vec![],
            library_symbol: None,
            library_footprint: Some(pcbcraft_doc::LibraryFootprint {
                library: "Test".into(),
                definition: pcbcraft_library::FootprintDefinition {
                    name: "TEST_ONE".into(),
                    description: String::new(),
                    keywords: String::new(),
                    pads: vec![geometry("1", 0.0)],
                    graphics: vec![],
                    model: None,
                    rules: None,
                    raw_source: String::new(),
                    unsupported: vec![],
                },
            }),
            fields: vec![],
        });
        project.board.footprints.push(pcbcraft_doc::Footprint {
            id: 10,
            component: 1,
            position: Point::new(12.5, 8.25),
            rotation: 90.0,
            locked: None,
            pads: vec![pcbcraft_doc::Pad {
                number: "1".into(),
                offset: Point::new(-1.0, 0.0),
                diameter: 1.0,
                net: None,
                castellated: None,
                geometry: Some(geometry("1", -1.0)),
            }],
        });
        project.board.footprints.push(pcbcraft_doc::Footprint {
            id: 11,
            component: 2,
            position: Point::new(30.0, 20.0),
            rotation: 0.0,
            locked: None,
            pads: vec![pcbcraft_doc::Pad {
                number: "1".into(),
                offset: Point::new(0.0, 0.0),
                diameter: 1.0,
                net: None,
                castellated: None,
                geometry: Some(geometry("1", 0.0)),
            }],
        });
        project.validate()?;
        let text = idf_board(&project)?;
        assert!(
            text.starts_with(".FILE_IDF\n.3DIDF VERSION: 2.0\n.UNITS:\tmillmm\n"),
            "a mechanical drawing declares itself in millimetres"
        );
        assert!(
            text.contains(".BODY 60.000 40.000 0.000 0.000"),
            "the board outline size is carried"
        );
        assert!(
            text.contains(".END_BODY R5\t12.500\t8.250\tFRONT\t90.000\t0\n"),
            "front parts list position, side and rotation"
        );
        assert!(
            text.contains(".END_BODY U1\t30.000\t20.000\tFRONT\t0.000\t1\n"),
            "every part is listed in placement order"
        );
        assert!(
            text.contains(".PIN 2\n") && text.matches(".END_PIN {\t}").count() == 2,
            "the embedded pin file lists every part with a default height"
        );
        assert!(text.ends_with(".END_FILE_IDF\n"), "the file closes");
        assert_eq!(
            idf_board(&Project::default()).err(),
            Some("board has no placed footprints to export".into()),
            "an empty board is rejected"
        );
        Ok(())
    }
    #[test]
    fn fiducial_marks_travel_in_the_pick_and_place_export() -> Result<(), String> {
        let mut project = Project::default();
        project.board.fiducials.extend([
            pcbcraft_doc::Fiducial {
                id: 901,
                position: Point::new(2.0, 3.0),
                diameter: 1.0,
                layer: pcbcraft_doc::Layer::Front,
            },
            pcbcraft_doc::Fiducial {
                id: 902,
                position: Point::new(58.0, 37.0),
                diameter: 1.2,
                layer: pcbcraft_doc::Layer::Back,
            },
        ]);
        let text = placement_text(&project)?;
        assert!(
            text.contains("SMT,\"FID01\",2.000,3.000,0.0,T,FIDUCIAL"),
            "the front mark is listed as a top-side fiducial"
        );
        assert!(
            text.contains("SMT,\"FID02\",58.000,37.000,0.0,B,FIDUCIAL"),
            "the back mark is listed on the bottom side"
        );
        Ok(())
    }

    #[test]
    fn specctra_dsn_carries_nets_placements_tracks_vias_and_keepouts() -> Result<(), String> {
        let mut project = Project::default();
        project.nets.extend([
            pcbcraft_doc::Net {
                id: 5,
                name: "GND".into(),
                ..Default::default()
            },
            pcbcraft_doc::Net {
                id: 9,
                name: "SIG".into(),
                ..Default::default()
            },
        ]);
        project.net_classes.push(pcbcraft_doc::NetClass {
            id: 11,
            name: "POWER".into(),
            clearance: 0.4,
            width: 0.6,
            min_width: None,
            max_width: None,
            target_length: None,
            length_tolerance: None,
            nets: vec![5],
        });
        let component =
            |id: u64, reference: &str, nets: [Option<u64>; 2]| pcbcraft_doc::Component {
                sheet: 0,
                id,
                reference: reference.into(),
                value: "1k".into(),
                kind: "resistor".into(),
                position: Point::new(10.0, 10.0),
                rotation: 0.0,
                pins: ["1", "2"]
                    .iter()
                    .map(|number| pcbcraft_doc::Pin {
                        number: (*number).into(),
                        name: String::new(),
                        offset: Point::default(),
                        net: nets[if *number == "1" { 0 } else { 1 }],
                        electrical_type: "passive".into(),
                        direction: None,
                        unit: 1,
                    })
                    .collect(),
                units: vec![],
                library_symbol: None,
                library_footprint: None,
                fields: vec![],
            };
        project
            .components
            .push(component(1, "R1", [Some(9), Some(5)]));
        project.components.push(component(2, "R2", [Some(9), None]));
        let pad = |number: &str, x: f64, y: f64, net: Option<u64>| pcbcraft_doc::Pad {
            number: number.into(),
            offset: Point::new(x, y),
            diameter: 0.6,
            net,
            castellated: None,
            geometry: None,
        };
        project.board.footprints.push(pcbcraft_doc::Footprint {
            id: 3,
            component: 1,
            position: Point::new(10.0, 10.0),
            rotation: 0.0,
            locked: None,
            pads: vec![pad("1", -1.0, 0.0, Some(9)), pad("2", 1.0, 0.0, Some(5))],
        });
        project.board.footprints.push(pcbcraft_doc::Footprint {
            id: 4,
            component: 2,
            position: Point::new(30.0, 10.0),
            rotation: 0.0,
            locked: None,
            pads: vec![pad("1", -1.0, 0.0, Some(9)), pad("2", 1.0, 0.0, None)],
        });
        project.board.tracks.push(pcbcraft_doc::Track {
            id: 6,
            net: 9,
            start: Point::new(11.0, 10.0),
            end: Point::new(29.0, 10.0),
            width: 0.3,
            layer: pcbcraft_doc::Layer::Front,
            arc: None,
        });
        project.board.vias.push(pcbcraft_doc::Via {
            id: 7,
            net: 9,
            position: Point::new(20.0, 15.0),
            diameter: 0.6,
            drill: 0.3,
        });
        project.board.rule_areas.push(pcbcraft_doc::RuleArea {
            id: 8,
            name: None,
            net: None,
            layer: None,
            clearance: 0.5,
            max_height: None,
            region: pcbcraft_doc::RuleRegion {
                x: 5.0,
                y: 5.0,
                width: 10.0,
                height: 8.0,
            },
        });
        let dsn = specctra_dsn(&project)?;
        assert!(
            dsn.starts_with("(PCB \""),
            "the file is an s-expression PCB"
        );
        assert!(dsn.contains("(resolution 1000.0 1000.0)"));
        assert!(
            dsn.contains("(net 2 \"SIG\" (pins \"R1\".\"1\" \"R2\".\"1\"))"),
            "nets list their pins by reference and pad number"
        );
        assert!(dsn.contains("(netclass \"POWER\" (use \"POWER\"))"));
        assert!(
            dsn.contains("(classpath \"POWER\" (circuit (minClearance 400)"),
            "classes carry their clearance in micrometres"
        );
        assert!(
            dsn.contains("(component (name \"R1\") (place (type fixed) (side top) (rotate 0.0) (at (10000 10000))))"),
            "components are placed with fixed type"
        );
        assert!(
            dsn.contains("(pin (name \"R1\".\"1\") (length 0) (at (9000 10000))"),
            "pads land at their world position"
        );
        assert!(
            dsn.contains(
                "(wire (path (net 2) (layer F.Cu) (width 300) (11000 10000) (29000 10000)))"
            ),
            "tracks become net-carrying wires"
        );
        assert!(dsn.contains("(via (net 2) (type VIA) (at (20000 15000)))"));
        assert!(
            dsn.contains("(polygon (layer BOARD_BOUNDARY) (net 0) (path (0 0) (60000 0) (60000 40000) (0 40000)))"),
            "the board edge closes the boundary"
        );
        assert!(
            dsn.contains("(keepout (layer F.Cu) (net 0) (rect (5000 5000) (15000 13000)))")
                && dsn.contains("(keepout (layer B.Cu) (net 0) (rect (5000 5000) (15000 13000)))"),
            "rule areas become keepouts on both copper layers"
        );
        Ok(())
    }

    #[test]
    fn ipc356_netlist_orders_pads_by_net_and_prints_fixed_columns() -> Result<(), String> {
        let mut project = Project::default();
        project.nets.extend([
            pcbcraft_doc::Net {
                id: 5,
                name: "GND".into(),
                ..Default::default()
            },
            pcbcraft_doc::Net {
                id: 9,
                name: "SIG".into(),
                ..Default::default()
            },
        ]);
        let component =
            |id: u64, reference: &str, nets: [Option<u64>; 2]| pcbcraft_doc::Component {
                sheet: 0,
                id,
                reference: reference.into(),
                value: "1k".into(),
                kind: "resistor".into(),
                position: Point::new(10.0, 10.0),
                rotation: 0.0,
                pins: ["1", "2"]
                    .iter()
                    .map(|number| pcbcraft_doc::Pin {
                        number: (*number).into(),
                        name: String::new(),
                        offset: Point::default(),
                        net: nets[if *number == "1" { 0 } else { 1 }],
                        electrical_type: "passive".into(),
                        direction: None,
                        unit: 1,
                    })
                    .collect(),
                units: vec![],
                library_symbol: None,
                library_footprint: None,
                fields: vec![],
            };
        project.components.push(component(1, "R2", [None, Some(9)]));
        project
            .components
            .push(component(2, "R1", [Some(5), Some(9)]));
        let pad = |number: &str, x: f64, y: f64, net: Option<u64>| pcbcraft_doc::Pad {
            number: number.into(),
            offset: Point::new(x, y),
            diameter: 0.6,
            net,
            castellated: None,
            geometry: None,
        };
        project.board.footprints.push(pcbcraft_doc::Footprint {
            id: 3,
            component: 1,
            position: Point::new(20.0, 10.0),
            rotation: 90.0,
            locked: None,
            pads: vec![pad("1", -1.0, 0.0, None), pad("2", 1.0, 0.0, Some(9))],
        });
        project.board.footprints.push(pcbcraft_doc::Footprint {
            id: 4,
            component: 2,
            position: Point::new(10.0, 10.0),
            rotation: 0.0,
            locked: None,
            pads: vec![pad("1", 0.0, 0.0, Some(5)), pad("2", 2.0, 0.0, Some(9))],
        });
        let text = ipc356_netlist(&project)?;
        let lines: Vec<&str> = text.lines().filter(|l| !l.starts_with('*')).collect();
        assert_eq!(lines.len(), 4, "one line per pad");
        assert!(
            lines[0].starts_with("00000R2"),
            "unconnected pads sort first"
        );
        assert!(
            lines[1].starts_with("00001R1"),
            "nets number in project order"
        );
        assert!(lines[2].starts_with("00002R1"));
        assert!(
            lines[3].starts_with("00002R2"),
            "same net sorts by reference"
        );
        for line in &lines {
            assert_eq!(line.chars().count(), 49, "fixed 49-column records");
        }
        assert!(
            lines[0].ends_with("0002000000011000"),
            "pad coordinates follow the rotated footprint"
        );
        assert!(lines[1].ends_with("0001000000010000"));
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
    fn a_traced_board_outline_replaces_the_rectangle_in_fabrication_output() -> Result<(), String> {
        let mut project = Project::default();
        project.board.outline = Some(vec![
            Point::new(0.0, 0.0),
            Point::new(60.0, 0.0),
            Point::new(60.0, 40.0),
            Point::new(30.0, 40.0),
            Point::new(30.0, 20.0),
            Point::new(0.0, 20.0),
        ]);
        project.validate()?;
        let board = kicad_board(&project)?;
        assert!(
            board.contains("gr_poly") && !board.contains("gr_rect"),
            "the outline exports as one polygon, not a rectangle"
        );
        let files = gerber_files(&project)?;
        let outline = &files["pcb-Edge.gbr"];
        assert!(
            outline.contains("X30.000000Y40.000000D01*")
                && outline.contains("X30.000000Y20.000000D01*"),
            "the gerber edge layer traces every vertex"
        );
        let drawing = svg(&project, "pcb")?;
        assert!(
            drawing.contains("<polygon") && !drawing.contains("<rect width=\"60\""),
            "the preview draws the traced shape"
        );
        let dxf = dxf_board(&project)?;
        assert!(
            dxf.contains("30.0") && dxf.contains("40.0"),
            "the DXF edge polyline carries the outline vertices"
        );
        Ok(())
    }
    #[test]
    fn curved_tracks_export_as_true_arcs_in_every_format() -> Result<(), String> {
        let mut project = Project::default();
        project.nets.push(pcbcraft_doc::Net {
            id: 1,
            name: "A".into(),
            ..Default::default()
        });
        project.board.tracks.push(pcbcraft_doc::Track {
            id: 2,
            net: 1,
            start: Point::new(10.0, 10.0),
            end: Point::new(20.0, 10.0),
            width: 0.2,
            layer: pcbcraft_doc::Layer::Front,
            arc: Some(Point::new(15.0, 12.0)),
        });
        project.validate()?;
        let board = kicad_board(&project)?;
        assert!(
            board.contains("(arc (start 10 10) (mid 15 12) (end 20 10)"),
            "the board export keeps the curve as an arc, not a segment"
        );
        let files = gerber_files(&project)?;
        let copper = &files["pcb-F.Cu.gbr"];
        assert!(
            copper.contains("G02*") && copper.contains("J-5.250000"),
            "the gerber draws a clockwise arc to the computed centre"
        );
        let drawing = svg(&project, "pcb")?;
        assert!(
            drawing.contains("A 7.250000 7.250000 0 0 1 20 10"),
            "the preview arc uses the true radius and flags"
        );
        let dxf = dxf_board(&project)?;
        assert!(
            dxf.contains("0\nARC") && dxf.contains("40\n7.250"),
            "the DXF drawing carries an ARC entity"
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
            arc: None,
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
    fn dxf_entities_reads_back_the_shape_kinds_it_exports() -> Result<(), String> {
        let text = "0\nSECTION\n2\nENTITIES\n\
            0\nLINE\n8\nF_Cu\n10\n1.5\n20\n2.5\n11\n6.0\n21\n2.5\n\
            0\nCIRCLE\n8\nDrill\n10\n30\n20\n20\n40\n0.15\n\
            0\nARC\n8\nF_Cu\n10\n47\n20\n10\n40\n7\n50\n359\n51\n180\n\
            0\nLWPOLYLINE\n8\nEdge\n90\n4\n70\n1\n10\n0\n20\n0\n10\n60\n20\n0\n10\n60\n20\n40\n10\n0\n20\n40\n\
            0\nTEXT\n8\nF_Silk\n10\n5\n20\n5\n40\n1.5\n50\n90\n1\nR7\n\
            0\nSPLINE\n8\nX\n10\n1\n20\n1\n\
            0\nENDSEC\n0\nEOF\n";
        let entities = dxf_entities(text)?;
        assert_eq!(entities.len(), 5, "the unsupported spline is skipped");
        assert_eq!(
            entities[0],
            DxfEntity {
                layer: "F_Cu".into(),
                shape: DxfShape::Line {
                    start: Point::new(1.5, 2.5),
                    end: Point::new(6.0, 2.5),
                }
            }
        );
        assert!(matches!(
            entities[1].shape,
            DxfShape::Circle {
                center: Point { x: 30.0, y: 20.0 },
                radius: 0.15
            }
        ));
        assert!(matches!(
            entities[2].shape,
            DxfShape::Arc {
                radius: 7.0,
                start_deg: 359.0,
                end_deg: 180.0,
                ..
            }
        ));
        match &entities[3].shape {
            DxfShape::Polyline { points, closed } => {
                assert!(*closed, "the closed polyline reads back closed");
                assert_eq!(points.len(), 4);
                assert_eq!(points[3], Point::new(0.0, 40.0));
            }
            _ => return Err("a polyline should read back as a polyline".into()),
        }
        assert_eq!(
            entities[4],
            DxfEntity {
                layer: "F_Silk".into(),
                shape: DxfShape::Text {
                    at: Point::new(5.0, 5.0),
                    height: 1.5,
                    rotation: 90.0,
                    value: "R7".into(),
                }
            }
        );
        Ok(())
    }

    #[test]
    fn dxf_entities_reads_the_paired_export_format_with_layers() -> Result<(), String> {
        let mut project = Project::default();
        project.graphics.push(pcbcraft_doc::Graphic {
            id: 1,
            layer: "board".into(),
            shape: "line".into(),
            points: vec![Point::new(1.0, 2.0), Point::new(9.0, 2.0)],
            width: 0.2,
            text: String::new(),
        });
        let text = dxf_board(&project)?;
        let entities = dxf_entities(&text)?;
        assert!(
            entities.iter().any(|e| e.layer == "Edge"
                && matches!(&e.shape, DxfShape::Polyline { closed: true, points } if points.len() == 4)),
            "the board outline reads back as a closed rectangle"
        );
        assert!(
            entities.iter().any(|e| e.layer == "Board_Graphics"
                && matches!(&e.shape, DxfShape::Polyline { points, closed: false }
                    if points == &vec![Point::new(1.0, 2.0), Point::new(9.0, 2.0)])),
            "the board graphic reads back on its layer"
        );
        Ok(())
    }

    #[test]
    fn dxf_entities_rejects_malformed_drawings() {
        let bad = [
            "0\nSECTION\n2\nENTITIES\n0\nLINE\n8\nX\n",
            "0\nSECTION\n2\nENTITIES\n0\nLINE\n10\n1\n20\n2\n",
            "0\nSECTION\n2\nENTITIES\n0\nCIRCLE\n10\n1\n20\n2\n40\n0\n",
            "0\nSECTION\n2\nENTITIES\n0\nLWPOLYLINE\n70\n1\n10\n1\n20\n1\n",
            "0\nSECTION\n2\nENTITIES\n0\nLINE\n10\n1e999\n20\n2\n11\n3\n21\n4\n",
            "0\nSECTION\n2\nENTITIES\n0\nLINE\n10\nnot-a-number\n20\n2\n11\n3\n21\n4\n",
        ];
        for text in bad {
            assert!(dxf_entities(text).is_err(), "{text} should be rejected");
        }
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
                sheet: 0,
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
        let text = bom_csv(&project, &fields, None)?;
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
            bom_csv(&project, &[], None)?.lines().count(),
            4,
            "no extra columns, same three groups"
        );
        assert!(
            bom_csv(&project, &["".to_string()], None).is_err(),
            "empty key"
        );
        assert!(
            bom_csv(&project, &["x".repeat(65)], None).is_err(),
            "overlong key"
        );
        assert!(
            bom_csv(
                &project,
                &(1..=17).map(|i| format!("f{i}")).collect::<Vec<String>>(),
                None,
            )
            .is_err(),
            "too many properties"
        );
        Ok(())
    }
    #[test]
    fn bom_csv_prints_project_defined_columns_under_their_labels() -> Result<(), String> {
        let mut project = Project::default();
        project.components.push(pcbcraft_doc::Component {
            sheet: 0,
            id: 1,
            reference: "R1".into(),
            value: "10k".into(),
            kind: "resistor".into(),
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
            fields: vec![
                pcbcraft_doc::ComponentField {
                    key: "MPN".into(),
                    value: "RC0603-10K".into(),
                },
                pcbcraft_doc::ComponentField {
                    key: "Rating".into(),
                    value: "0.1W".into(),
                },
            ],
        });
        project.bom_fields = vec![
            pcbcraft_doc::BomField {
                key: "MPN".into(),
                label: "Manufacturer part".into(),
            },
            pcbcraft_doc::BomField {
                key: "Rating".into(),
                label: String::new(),
            },
        ];
        project.validate()?;
        let text = bom_csv(&project, &[], None)?;
        assert!(
            text.starts_with("quantity,references,value,kind,footprint,Manufacturer part,Rating\n"),
            "recorded columns print under their labels, falling back to the key"
        );
        assert!(
            text.contains("RC0603-10K,0.1W"),
            "recorded columns read the matching component properties"
        );
        let explicit = bom_csv(&project, &["MPN".to_string()], None)?;
        assert!(
            explicit.lines().next().is_some_and(|l| l.ends_with(",MPN")),
            "an explicit request overrides the recorded columns"
        );
        project.bom_fields.push(pcbcraft_doc::BomField {
            key: "MPN".into(),
            label: String::new(),
        });
        assert!(
            bom_csv(&project, &[], None).is_err(),
            "duplicate recorded keys are refused"
        );
        Ok(())
    }
    #[test]
    fn bom_csv_follows_a_variants_skips_and_footprint_swaps() -> Result<(), String> {
        let mut project = Project::default();
        for (id, reference) in [(1, "R1"), (2, "R2")] {
            project.components.push(pcbcraft_doc::Component {
                sheet: 0,
                id,
                reference: reference.into(),
                value: "10k".into(),
                kind: "resistor".into(),
                position: Point::new(10.0, 10.0),
                rotation: 0.0,
                pins: [(-1.0, "1"), (1.0, "2")]
                    .map(|(x, number)| pcbcraft_doc::Pin {
                        number: number.into(),
                        name: String::new(),
                        offset: Point::new(x, 0.0),
                        net: None,
                        electrical_type: "passive".into(),
                        direction: None,
                        unit: 1,
                    })
                    .to_vec(),
                units: vec![],
                library_symbol: None,
                library_footprint: None,
                fields: vec![],
            });
        }
        project.variants.push(pcbcraft_doc::Variant {
            name: "Slim".into(),
            note: None,
            dnp: vec![2],
            footprints: vec![pcbcraft_doc::FootprintOverride {
                component: 1,
                footprint: "R_0603".into(),
            }],
        });
        project.validate()?;
        let plain = bom_csv(&project, &[], None)?;
        assert!(
            plain.contains("starter pads") && plain.contains("R1,R2") && !plain.contains("R_0603"),
            "the plain bill lists both parts with the placed footprint"
        );
        let slim = bom_csv(&project, &[], Some("Slim"))?;
        assert!(
            slim.contains("R_0603") && !slim.contains("R2"),
            "the variant bill swaps the package and skips the unpopulated part"
        );
        assert!(
            bom_csv(&project, &[], Some("Ghost")).is_err(),
            "an unknown variant name is refused"
        );
        assert!(
            bom_csv(&project, &[], Some("")).is_ok(),
            "a blank variant name prints the plain bill"
        );
        Ok(())
    }
    #[test]
    fn library_footprint_svg_shows_pads_holes_and_artwork() -> Result<(), String> {
        let definition = pcbcraft_library::read_footprint(include_str!(
            "../../library/tests/fixtures/DIP-8_W7.62mm.kicad_mod"
        ))
        .map_err(|e| e.to_string())?;
        let svg = footprint_svg(&definition)?;
        assert!(svg.starts_with("<?xml"), "the file is a real SVG document");
        assert!(svg.contains("<title>DIP-8_W7.62mm</title>"));
        let circles = svg.matches("<circle").count();
        assert!(
            (15..=16).contains(&circles),
            "eight pads plus eight holes, oval drills drawn as slots: {circles}"
        );
        assert!(
            svg.matches("<polygon").count() + svg.matches("<rect").count() >= 1,
            "rectangular artwork or pad outlines are drawn"
        );
        assert!(svg.contains("scale(1,-1)"), "the view is drawn y-up");
        assert!(
            svg.contains("width=\"") && svg.contains("mm\""),
            "sizes are in millimetres"
        );
        let empty = pcbcraft_library::FootprintDefinition {
            name: "BLANK".into(),
            description: String::new(),
            keywords: String::new(),
            pads: Vec::new(),
            graphics: Vec::new(),
            model: None,
            rules: None,
            raw_source: String::new(),
            unsupported: Vec::new(),
        };
        let svg = footprint_svg(&empty)?;
        assert!(svg.starts_with("<?xml"), "an empty footprint still renders");
        Ok(())
    }
    #[test]
    fn library_symbol_svg_shows_pins_and_body_artwork() -> Result<(), String> {
        let symbol = pcbcraft_library::read_symbols(include_str!(
            "../../library/tests/fixtures/lm358.kicad_sym"
        ))
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|s| s.name == "LM358")
        .ok_or("fixture symbol missing")?;
        let svg = symbol_svg(&symbol)?;
        assert!(svg.starts_with("<?xml"), "the file is a real SVG document");
        assert!(svg.contains("<title>LM358</title>"));
        assert!(symbol.pins.len() >= 8, "the fixture carries a full package");
        assert!(
            svg.matches("<line").count() >= symbol.pins.len(),
            "every pin of every unit is drawn as a line"
        );
        assert!(svg.contains(">8<"), "pin numbers are labelled");
        assert!(
            svg.contains("<rect") || svg.contains("<polyline"),
            "the body is drawn"
        );
        assert_eq!(
            svg.matches("<svg").count(),
            1,
            "exactly one root element is opened"
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
                sheet: 0,
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
            sheet: 0,
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
            text: String::new(),
        });
        project.labels.push(pcbcraft_doc::Label {
            id: 5,
            kind: "junction".into(),
            net: 1,
            position: Point::new(20.0, 5.0),
            rotation: 0.0,
            text: String::new(),
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
    #[test]
    fn footprint_library_text_round_trips_through_the_reader() -> Result<(), String> {
        for source in [
            include_str!("../../library/tests/fixtures/R_0805_2012Metric.kicad_mod"),
            include_str!("../../library/tests/fixtures/DIP-8_W7.62mm.kicad_mod"),
        ] {
            let footprint = pcbcraft_library::read_footprint(source).map_err(|e| e.to_string())?;
            let text = footprint_library_text(&footprint)?;
            let again = pcbcraft_library::read_footprint(&text).map_err(|e| e.to_string())?;
            assert_eq!(again.name, footprint.name);
            assert_eq!(again.description, footprint.description);
            assert_eq!(again.keywords, footprint.keywords);
            assert_eq!(again.pads.len(), footprint.pads.len());
            for (a, b) in again.pads.iter().zip(&footprint.pads) {
                assert_eq!(a.number, b.number);
                assert_eq!(a.pad_type, b.pad_type);
                assert_eq!(a.shape, b.shape);
                assert_eq!(a.position, b.position);
                assert_eq!(a.rotation, b.rotation);
                assert_eq!(a.size, b.size);
                assert_eq!(a.layers, b.layers);
                assert_eq!(a.drill, b.drill);
                assert_eq!(a.roundrect_ratio, b.roundrect_ratio);
            }
            assert_eq!(again.graphics, footprint.graphics);
            assert_eq!(
                again
                    .model
                    .as_ref()
                    .map(|m| (&m.path, m.offset, m.scale, m.rotate)),
                footprint
                    .model
                    .as_ref()
                    .map(|m| (&m.path, m.offset, m.scale, m.rotate))
            );
            assert_eq!(again.rules, footprint.rules);
        }
        Ok(())
    }
    #[test]
    fn symbol_library_text_round_trips_through_the_reader() -> Result<(), String> {
        for source in [
            include_str!("../../library/tests/fixtures/device.kicad_sym"),
            include_str!("../../library/tests/fixtures/lm358.kicad_sym"),
        ] {
            let symbols = pcbcraft_library::read_symbols(source).map_err(|e| e.to_string())?;
            for symbol in symbols
                .iter()
                .filter(|s| !s.pins.is_empty() && !s.graphics.is_empty())
            {
                let text = symbol_library_text(symbol)?;
                let again = pcbcraft_library::read_symbols(&text).map_err(|e| e.to_string())?;
                assert_eq!(again.len(), 1, "{} wrote stray symbols", symbol.name);
                let again = &again[0];
                assert_eq!(again.name, symbol.name);
                assert_eq!(again.reference, symbol.reference);
                assert_eq!(again.value, symbol.value);
                assert_eq!(again.description, symbol.description);
                assert_eq!(again.keywords, symbol.keywords);
                assert_eq!(again.footprint, symbol.footprint);
                assert_eq!(again.pins, symbol.pins);
                assert_eq!(again.graphics, symbol.graphics);
            }
        }
        Ok(())
    }
}
