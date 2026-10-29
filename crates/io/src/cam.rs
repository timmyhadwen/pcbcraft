//! Isolation milling G-code generation.
use crate::cam_shapes::{LayerShape, layer_shapes};
use pcbcraft_doc::{Layer, Project};
use pcbcraft_geom::{
    OffsetJoin, Point, Ring, polygon_boundary_distance, polygon_offset, polygon_union,
    segment_distance,
};
use serde::{Deserialize, Serialize};

const LINEARISE_TOL_MM: f64 = 0.01;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Choice of dialect.
pub enum Dialect {
    /// The grbl case.
    Grbl,
    /// The linux cnc case.
    LinuxCnc,
    /// The marlin case.
    Marlin,
    /// The snapmaker case.
    Snapmaker,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
/// Choice of spindle.
pub enum Spindle {
    /// The rpm case.
    Rpm(u32),
    /// The percent case.
    Percent(u8),
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
/// Settings for tool.
pub struct Tool {
    /// Diameter in millimetres.
    pub diameter_mm: f64,
    /// Feed in millimetres per minute.
    pub feed_mm_min: f64,
    /// Plunge in millimetres per minute.
    pub plunge_mm_min: f64,
    /// Depth in millimetres.
    pub depth_mm: f64,
    /// Passes.
    pub passes: u8,
    /// Spindle.
    pub spindle: Spindle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Choice of side.
pub enum Side {
    /// The top case.
    Top,
    /// The bottom case.
    Bottom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Choice of mirror axis.
pub enum MirrorAxis {
    /// The x case.
    X,
    /// The y case.
    Y,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RubRegion {
    pub min: Point,
    pub max: Point,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
/// Settings for cnc profile.
pub struct CncProfile {
    /// Dialect.
    pub dialect: Dialect,
    /// Work min.
    pub work_min: Point,
    /// Work max.
    pub work_max: Point,
    /// Safe z in millimetres.
    pub safe_z_mm: f64,
    /// Jog in millimetres per minute.
    pub jog_mm_min: f64,
}

impl Default for CncProfile {
    fn default() -> Self {
        Self {
            dialect: Dialect::Grbl,
            work_min: Point::new(0.0, 0.0),
            work_max: Point::new(320.0, 350.0),
            safe_z_mm: 5.0,
            jog_mm_min: 3000.0,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Settings for isolation options.
pub struct IsolationOptions {
    /// Side.
    pub side: Side,
    /// Tool.
    pub tool: Tool,
    /// Overlap as a percentage.
    pub overlap_pct: u8,
    /// Rubout.
    pub rubout: Option<RubRegion>,
    /// Mirror axis.
    pub mirror_axis: MirrorAxis,
    /// Alignment holes.
    pub alignment_holes: bool,
}

#[derive(Clone, Debug)]
/// Settings for gcode writer.
pub struct GcodeWriter {
    dialect: Dialect,
    lines: Vec<String>,
    bounds: (Point, Point),
    cut_mm: f64,
    plunge_mm: f64,
    rapid_mm: f64,
    estimate_s: f64,
    at: Option<Point>,
    z: f64,
    feed: f64,
}

impl GcodeWriter {
    /// New.
    pub fn new(dialect: Dialect, safe_z: f64) -> Self {
        Self {
            dialect,
            lines: vec![],
            bounds: (
                Point::new(f64::INFINITY, f64::INFINITY),
                Point::new(f64::NEG_INFINITY, f64::NEG_INFINITY),
            ),
            cut_mm: 0.0,
            plunge_mm: 0.0,
            rapid_mm: 0.0,
            estimate_s: 0.0,
            at: None,
            z: safe_z,
            feed: 0.0,
        }
    }

    fn note(&mut self, p: Point) {
        self.bounds.0.x = self.bounds.0.x.min(p.x);
        self.bounds.0.y = self.bounds.0.y.min(p.y);
        self.bounds.1.x = self.bounds.1.x.max(p.x);
        self.bounds.1.y = self.bounds.1.y.max(p.y);
    }

    fn move_time(&mut self, to: Point, cut: bool) {
        if let Some(from) = self.at {
            let dist = from.distance(to);
            if cut {
                self.cut_mm += dist;
                if self.feed > 0.0 {
                    self.estimate_s += dist / self.feed * 60.0;
                }
            } else if self.feed > 0.0 {
                self.rapid_mm += dist;
                self.estimate_s += dist / self.feed * 60.0;
            }
        }
        self.at = Some(to);
        self.note(to);
    }

    /// Comment.
    pub fn comment(&mut self, text: &str) {
        self.lines.push(format!("; {text}"));
    }

    /// Rapid.
    pub fn rapid(&mut self, p: Point, jog: f64) {
        self.feed = jog;
        self.move_time(p, false);
        self.lines.push(format!("G0 X{:.3} Y{:.3}", p.x, p.y));
    }

    /// Cut.
    pub fn cut(&mut self, p: Point, feed: f64) {
        self.feed = feed;
        self.move_time(p, true);
        self.lines
            .push(format!("G1 X{:.3} Y{:.3} F{:.0}", p.x, p.y, feed));
    }

    /// Plunge.
    pub fn plunge(&mut self, z: f64, rate: f64) {
        if let Some(at) = self.at {
            let dist = (self.z - z).abs();
            self.plunge_mm += dist;
            if rate > 0.0 {
                self.estimate_s += dist / rate * 60.0;
            }
            self.z = z;
            self.lines.push(format!("G1 Z{:.3} F{:.0}", z, rate));
            self.note(at);
        }
    }

    /// Retract.
    pub fn retract(&mut self, safe_z: f64, rate: f64) {
        if let Some(at) = self.at {
            let dist = (safe_z - self.z).abs();
            self.plunge_mm += dist;
            if rate > 0.0 {
                self.estimate_s += dist / rate * 60.0;
            }
            self.z = safe_z;
            self.lines.push(format!("G1 Z{:.3} F{:.0}", safe_z, rate));
            self.note(at);
        }
    }

    /// Spindle on.
    pub fn spindle_on(&mut self, spindle: Spindle) {
        let line = match (self.dialect, spindle) {
            (Dialect::Snapmaker, Spindle::Percent(p)) => format!("M3 P{p}"),
            (_, Spindle::Rpm(r)) => format!("M3 S{r}"),
            (_, Spindle::Percent(p)) => format!("M3 S{}", p * 100),
        };
        self.lines.push(line);
    }

    /// Spindle off.
    pub fn spindle_off(&mut self) {
        self.lines.push("M5".into());
    }

    /// Finish.
    pub fn finish(self) -> String {
        self.lines.join("\n") + "\n"
    }

    /// Estimate s.
    pub fn estimate_s(&self) -> f64 {
        self.estimate_s
    }
}

fn decimal(value: f64) -> String {
    let text = format!("{value:.3}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn board_centre(p: &Project) -> Point {
    Point::new(p.board.width / 2.0, p.board.height / 2.0)
}

fn mirror_point(p: Point, axis: MirrorAxis, centre: Point) -> Point {
    match axis {
        MirrorAxis::X => Point::new(2.0 * centre.x - p.x, p.y),
        MirrorAxis::Y => Point::new(p.x, 2.0 * centre.y - p.y),
    }
}

fn transform_point(p: Point, side: Side, axis: MirrorAxis, centre: Point) -> Point {
    if side == Side::Top {
        p
    } else {
        mirror_point(p, axis, centre)
    }
}

fn min_shape_distance(a: &LayerShape, b: &LayerShape) -> f64 {
    if a.net == b.net {
        return f64::INFINITY;
    }
    let edges_a: Vec<(Point, Point)> = pcbcraft_geom::polygon_edges(&a.outline).collect();
    let edges_b: Vec<(Point, Point)> = pcbcraft_geom::polygon_edges(&b.outline).collect();
    let mut best = f64::INFINITY;
    for v in a.outline.iter().copied().chain(b.outline.iter().copied()) {
        best = best.min(polygon_boundary_distance(v, &b.outline));
        best = best.min(polygon_boundary_distance(v, &a.outline));
    }
    for (a0, a1) in &edges_a {
        for (b0, b1) in &edges_b {
            best = best.min(segment_distance(*a0, *a1, *b0, *b1));
        }
    }
    best
}

fn check_net_clearance(shapes: &[LayerShape], tool_d: f64) -> Result<(), String> {
    for i in 0..shapes.len() {
        for j in (i + 1)..shapes.len() {
            let d = min_shape_distance(&shapes[i], &shapes[j]);
            if d < tool_d {
                return Err(format!(
                    "nets {} and {} are {:.3} mm apart, less than the {:.3} mm tool; widen clearance between objects {} and {}",
                    shapes[i].net, shapes[j].net, d, tool_d, shapes[i].id, shapes[j].id
                ));
            }
        }
    }
    Ok(())
}

fn linearise_ring(ring: &Ring) -> Vec<Point> {
    let open = {
        let mut pts: Vec<Point> = ring.to_vec();
        if pts.len() >= 2
            && pts
                .first()
                .zip(pts.last())
                .is_some_and(|(a, b)| a.distance(*b) < 1e-9)
        {
            pts.pop();
        }
        pts
    };
    if open.len() < 2 {
        return vec![];
    }
    let mut path = vec![open[0]];
    for window in open.windows(2) {
        let a = window[0];
        let b = window[1];
        if a.distance(b) <= LINEARISE_TOL_MM {
            path.push(b);
            continue;
        }
        let steps = ((a.distance(b) / LINEARISE_TOL_MM).ceil() as usize).max(1);
        for i in 1..=steps {
            let t = i as f64 / steps as f64;
            path.push(Point::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t));
        }
    }
    path
}

/// Isolation paths.
pub fn isolation_paths(p: &Project, opts: &IsolationOptions) -> Result<Vec<Ring>, String> {
    if opts.rubout.is_some() {
        return Err("rubout isolation is not implemented yet".into());
    }
    if opts.overlap_pct > 90 {
        return Err("overlap must be between 0 and 90 percent".into());
    }
    let layer = match opts.side {
        Side::Top => Layer::Front,
        Side::Bottom => Layer::Back,
    };
    let shapes = layer_shapes(p, layer)?;
    check_net_clearance(&shapes, opts.tool.diameter_mm)?;
    let rings: Vec<Ring> = shapes.iter().map(|s| s.outline.clone()).collect();
    let union = polygon_union(&rings);
    if union.is_empty() {
        return Ok(vec![]);
    }
    let radius = opts.tool.diameter_mm / 2.0;
    let step = opts.tool.diameter_mm * (1.0 - opts.overlap_pct as f64 / 100.0);
    let mut paths = Vec::new();
    for pass in 0..opts.tool.passes.max(1) {
        let delta = radius + pass as f64 * step;
        paths.extend(polygon_offset(&union, delta, OffsetJoin::Round));
    }
    Ok(paths)
}

fn in_work_area(p: Point, profile: &CncProfile) -> bool {
    p.x >= profile.work_min.x
        && p.y >= profile.work_min.y
        && p.x <= profile.work_max.x
        && p.y <= profile.work_max.y
}

fn check_bounds(points: &[Point], profile: &CncProfile) -> Result<(), String> {
    for p in points {
        if !p.valid() || !in_work_area(*p, profile) {
            return Err(format!(
                "coordinate ({:.3}, {:.3}) is outside the machine work area",
                p.x, p.y
            ));
        }
    }
    Ok(())
}

fn header(profile: &CncProfile, estimate_s: f64, tool: &Tool) -> Vec<String> {
    let mut lines = vec![
        "G21".into(),
        "G90".into(),
        "G17".into(),
        format!("; estimate_s={:.1}", estimate_s),
        format!(
            "; tool_d={} feed={} plunge={} depth={} passes={}",
            decimal(tool.diameter_mm),
            decimal(tool.feed_mm_min),
            decimal(tool.plunge_mm_min),
            decimal(tool.depth_mm),
            tool.passes
        ),
    ];
    if profile.dialect == Dialect::Snapmaker {
        lines.insert(0, "; Snapmaker CNC".into());
    }
    lines
}

fn write_paths(
    writer: &mut GcodeWriter,
    paths: &[Ring],
    tool: &Tool,
    profile: &CncProfile,
    side: Side,
    axis: MirrorAxis,
    centre: Point,
) -> Result<(), String> {
    let safe = profile.safe_z_mm;
    for ring in paths {
        let mut path = linearise_ring(ring);
        if path.is_empty() {
            continue;
        }
        for pt in &mut path {
            *pt = transform_point(*pt, side, axis, centre);
        }
        check_bounds(&path, profile)?;
        writer.rapid(path[0], profile.jog_mm_min);
        writer.plunge(-tool.depth_mm, tool.plunge_mm_min);
        for pt in path.iter().skip(1) {
            writer.cut(*pt, tool.feed_mm_min);
        }
        writer.retract(safe, tool.plunge_mm_min);
    }
    Ok(())
}

/// Settings for isolation gcode result.
pub struct IsolationGcodeResult {
    /// Files.
    pub files: Vec<(String, String)>,
    /// Estimate in seconds.
    pub estimate_s: f64,
    /// Alignment.
    pub alignment: Option<[Point; 2]>,
}

/// Isolation gcode.
pub fn isolation_gcode(
    p: &Project,
    opts: &IsolationOptions,
    profile: &CncProfile,
) -> Result<IsolationGcodeResult, String> {
    p.validate()?;
    if opts.tool.depth_mm > p.board.thickness {
        return Err(format!(
            "cut depth {:.3} mm exceeds board thickness {:.3} mm",
            opts.tool.depth_mm, p.board.thickness
        ));
    }
    if opts.tool.diameter_mm <= 0.0
        || opts.tool.feed_mm_min <= 0.0
        || opts.tool.plunge_mm_min <= 0.0
    {
        return Err("tool diameter, feed and plunge must be positive".into());
    }
    let paths = isolation_paths(p, opts)?;
    let centre = board_centre(p);
    let mut writer = GcodeWriter::new(profile.dialect, profile.safe_z_mm);
    for line in header(profile, 0.0, &opts.tool) {
        writer.lines.push(line);
    }
    writer.spindle_on(opts.tool.spindle);
    write_paths(
        &mut writer,
        &paths,
        &opts.tool,
        profile,
        opts.side,
        opts.mirror_axis,
        centre,
    )?;
    writer.spindle_off();
    writer.lines.push("M2".into());
    let estimate = writer.estimate_s();
    if let Some(first) = writer
        .lines
        .iter_mut()
        .find(|l| l.starts_with("; estimate_s="))
    {
        *first = format!("; estimate_s={:.1}", estimate);
    }
    let body = writer.finish();
    let name = match opts.side {
        Side::Top => "isolation-top.nc".into(),
        Side::Bottom => "isolation-bottom.nc".into(),
    };
    let mut alignment = None;
    if opts.side == Side::Bottom && opts.alignment_holes {
        let outline = p
            .board
            .outline
            .as_ref()
            .ok_or("alignment holes need a board outline".to_string())?;
        let mut min_x = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        for pt in outline {
            min_x = min_x.min(pt.x);
            max_x = max_x.max(pt.x);
        }
        let y = match opts.mirror_axis {
            MirrorAxis::X => centre.y,
            MirrorAxis::Y => outline.iter().map(|p| p.y).sum::<f64>() / outline.len() as f64,
        };
        let holes = [
            transform_point(
                Point::new(min_x - 5.0, y),
                opts.side,
                opts.mirror_axis,
                centre,
            ),
            transform_point(
                Point::new(max_x + 5.0, y),
                opts.side,
                opts.mirror_axis,
                centre,
            ),
        ];
        check_bounds(&holes, profile)?;
        alignment = Some([holes[0], holes[1]]);
    }
    Ok(IsolationGcodeResult {
        files: vec![(name, body)],
        estimate_s: estimate,
        alignment,
    })
}
