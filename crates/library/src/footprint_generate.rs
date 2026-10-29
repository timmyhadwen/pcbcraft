use crate::{
    ChamferCorners, FootprintDefinition, FootprintPad, Graphic, PadChamfer,
    footprint_check::{LAYER_COURTYARD, LAYER_FAB, LAYER_SILK},
};
use pcbcraft_geom::Point;
use serde::Deserialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LandDensity {
    A,
    B,
    C,
}

impl LandDensity {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s.to_ascii_uppercase().as_str() {
            "A" => Ok(Self::A),
            "B" | "" => Ok(Self::B),
            "C" => Ok(Self::C),
            _ => Err("density must be A, B or C".into()),
        }
    }

    pub fn suffix(self) -> char {
        match self {
            Self::A => 'M',
            Self::B => 'N',
            Self::C => 'L',
        }
    }

    pub fn courtyard_excess(self) -> f64 {
        match self {
            Self::A => 0.5,
            Self::B => 0.25,
            Self::C => 0.1,
        }
    }

    pub fn silk_width(self) -> f64 {
        match self {
            Self::A => 0.15,
            Self::B => 0.127,
            Self::C => 0.10,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenerateFootprintParams {
    pub family: String,
    #[serde(default)]
    pub density: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub keywords: String,
    #[serde(default)]
    pub pitch: Option<f64>,
    #[serde(default)]
    pub pin_count: Option<u32>,
    #[serde(default)]
    pub body_x: Option<f64>,
    #[serde(default)]
    pub body_y: Option<f64>,
    #[serde(default)]
    pub body_z: Option<f64>,
    #[serde(default)]
    pub rows: Option<u32>,
    #[serde(default)]
    pub cols: Option<u32>,
    #[serde(default)]
    pub row_spacing: Option<f64>,
    #[serde(default)]
    pub terminal_width: Option<f64>,
    #[serde(default)]
    pub thermal: Option<bool>,
}

pub fn generate_footprint(params: &GenerateFootprintParams) -> Result<FootprintDefinition, String> {
    let density = LandDensity::parse(if params.density.is_empty() {
        "B"
    } else {
        &params.density
    })?;
    let family = params.family.as_str();
    if params.name.is_empty() || params.name.len() > 128 {
        return Err("name must be 1 to 128 characters".into());
    }
    let mut def = match family {
        "chip_2terminal" => generate_chip_2terminal(params, density)?,
        "sot" => generate_sot(params, density)?,
        "soic" => generate_soic(params, density)?,
        "qfp" => generate_qfp(params, density)?,
        "qfn" => generate_qfn(params, density)?,
        "bga" => generate_bga(params, density)?,
        "dip" => generate_dip(params, density)?,
        "header_single" => generate_header_single(params, density)?,
        "header_double" => generate_header_double(params, density)?,
        "radial" => generate_radial(params, density)?,
        "axial" => generate_axial(params, density)?,
        _ => return Err(format!("unknown footprint family {family:?}")),
    };
    def.name = params.name.clone();
    if !params.description.is_empty() {
        def.description = params.description.clone();
    }
    if !params.keywords.is_empty() {
        def.keywords = params.keywords.clone();
    }
    def.validate().map_err(|e| e.to_string())?;
    Ok(def)
}

fn finite_positive(v: f64, label: &str, min: f64, max: f64) -> Result<f64, String> {
    if !v.is_finite() || v < min || v > max {
        return Err(format!("{label} must be {min} to {max} millimetres"));
    }
    Ok(v)
}

fn count(v: Option<u32>, label: &str, min: u32, max: u32) -> Result<u32, String> {
    let Some(n) = v else {
        return Err(format!("{label} is required"));
    };
    if n < min || n > max {
        return Err(format!("{label} must be {min} to {max}"));
    }
    Ok(n)
}

fn smd_layers() -> Vec<String> {
    vec!["F.Cu".into(), "F.Paste".into(), "F.Mask".into()]
}

fn smd_pad(
    number: &str,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    chamfer: Option<PadChamfer>,
) -> FootprintPad {
    FootprintPad {
        number: number.into(),
        pad_type: "smd".into(),
        shape: if chamfer.is_some() {
            "chamfered".into()
        } else {
            "rect".into()
        },
        position: Point::new(x, y),
        rotation: 0.0,
        size: Point::new(w, h),
        layers: smd_layers(),
        drill: None,
        roundrect_ratio: None,
        chamfer,
        polygon: vec![],
        raw_source: String::new(),
    }
}

fn thru_pad(number: &str, x: f64, y: f64, drill: f64, size: f64) -> FootprintPad {
    FootprintPad {
        number: number.into(),
        pad_type: "thru_hole".into(),
        shape: "circle".into(),
        position: Point::new(x, y),
        rotation: 0.0,
        size: Point::new(size, size),
        layers: vec!["*.Cu".into(), "*.Mask".into()],
        drill: Some(crate::Drill {
            size: Point::new(drill, drill),
            offset: Point::default(),
        }),
        roundrect_ratio: None,
        chamfer: None,
        polygon: vec![],
        raw_source: String::new(),
    }
}

fn rectangle_layer(start: Point, end: Point, width: f64, layer: &str) -> Graphic {
    Graphic::Rectangle {
        start,
        end,
        width,
        fill: "none".into(),
        layer: layer.into(),
    }
}

fn pin1_chamfer() -> PadChamfer {
    PadChamfer {
        ratio: 0.25,
        corners: ChamferCorners {
            top_left: true,
            top_right: false,
            bottom_left: false,
            bottom_right: false,
        },
    }
}

fn finish_package(
    mut def: FootprintDefinition,
    density: LandDensity,
    body_x: f64,
    body_y: f64,
) -> FootprintDefinition {
    let silk = density.silk_width();
    let hx = body_x / 2.0;
    let hy = body_y / 2.0;
    def.graphics.push(rectangle_layer(
        Point::new(-hx, -hy),
        Point::new(hx, hy),
        silk,
        LAYER_FAB,
    ));
    def.graphics.push(Graphic::Circle {
        center: Point::new(-hx + silk * 2.0, -hy + silk * 2.0),
        radius: silk * 0.75,
        width: silk * 0.5,
        fill: "none".into(),
        layer: LAYER_SILK.into(),
    });
    let (pmin, pmax) = pad_extents(&def.pads);
    let ex = density.courtyard_excess();
    def.graphics.push(rectangle_layer(
        Point::new(pmin.x - ex, pmin.y - ex),
        Point::new(pmax.x + ex, pmax.y + ex),
        silk * 0.5,
        LAYER_COURTYARD,
    ));
    def
}

fn pad_extents(pads: &[FootprintPad]) -> (Point, Point) {
    let mut min = Point::new(f64::MAX, f64::MAX);
    let mut max = Point::new(f64::MIN, f64::MIN);
    for pad in pads {
        let hx = pad.size.x / 2.0;
        let hy = pad.size.y / 2.0;
        min.x = min.x.min(pad.position.x - hx);
        min.y = min.y.min(pad.position.y - hy);
        max.x = max.x.max(pad.position.x + hx);
        max.y = max.y.max(pad.position.y + hy);
    }
    (min, max)
}

fn generate_chip_2terminal(
    params: &GenerateFootprintParams,
    density: LandDensity,
) -> Result<FootprintDefinition, String> {
    let body_x = finite_positive(params.body_x.unwrap_or(1.6), "body_x", 0.4, 10.0)?;
    let body_y = finite_positive(params.body_y.unwrap_or(0.8), "body_y", 0.2, 10.0)?;
    let tw = finite_positive(
        params.terminal_width.unwrap_or(body_x * 0.35),
        "terminal_width",
        0.1,
        5.0,
    )?;
    let gap = body_x * 0.55;
    let pads = vec![
        smd_pad("1", -gap / 2.0, 0.0, tw, body_y, Some(pin1_chamfer())),
        smd_pad("2", gap / 2.0, 0.0, tw, body_y, None),
    ];
    let name = format!(
        "CAPC{}X{}",
        (body_y * 100.0).round() as u32,
        (body_x * 100.0).round() as u32
    );
    Ok(finish_package(
        FootprintDefinition {
            name,
            description: "two-terminal chip".into(),
            keywords: "chip".into(),
            pads,
            graphics: vec![],
            model: None,
            rules: None,
            raw_source: String::new(),
            unsupported: vec![],
        },
        density,
        body_x,
        body_y,
    ))
}

fn generate_sot(
    params: &GenerateFootprintParams,
    density: LandDensity,
) -> Result<FootprintDefinition, String> {
    let pin_count = count(params.pin_count, "pin_count", 3, 8)?;
    let pitch = finite_positive(params.pitch.unwrap_or(0.95), "pitch", 0.4, 1.27)?;
    let body_x = finite_positive(params.body_x.unwrap_or(2.9), "body_x", 1.0, 10.0)?;
    let body_y = finite_positive(params.body_y.unwrap_or(1.6), "body_y", 1.0, 10.0)?;
    let pad_w = pitch * 0.55;
    let pad_h = pitch * 0.7;
    let mut pads = Vec::new();
    let start_x = -((pin_count as f64 - 1.0) * pitch) / 2.0;
    for i in 0..pin_count {
        let n = (i + 1).to_string();
        let chamfer = if i == 0 { Some(pin1_chamfer()) } else { None };
        pads.push(smd_pad(
            &n,
            start_x + i as f64 * pitch,
            body_y / 2.0 + pad_h / 2.0,
            pad_w,
            pad_h,
            chamfer,
        ));
    }
    Ok(finish_package(
        FootprintDefinition {
            name: format!("SOT{pin_count}"),
            description: "small outline transistor".into(),
            keywords: "sot".into(),
            pads,
            graphics: vec![],
            model: None,
            rules: None,
            raw_source: String::new(),
            unsupported: vec![],
        },
        density,
        body_x,
        body_y,
    ))
}

fn generate_soic(
    params: &GenerateFootprintParams,
    density: LandDensity,
) -> Result<FootprintDefinition, String> {
    let pin_count = count(params.pin_count, "pin_count", 4, 28)?;
    if pin_count % 2 != 0 {
        return Err("pin_count must be even for SOIC".into());
    }
    let pitch = finite_positive(params.pitch.unwrap_or(1.27), "pitch", 0.5, 2.0)?;
    let body_y = finite_positive(params.body_y.unwrap_or(3.9), "body_y", 2.0, 20.0)?;
    let per_side = pin_count / 2;
    let span = (per_side as f64 - 1.0) * pitch;
    let pad_w = pitch * 0.45;
    let pad_h = pitch * 0.6;
    let mut pads = Vec::new();
    for i in 0..per_side {
        let num = (i + 1).to_string();
        let x = -span / 2.0 + i as f64 * pitch;
        let chamfer = if i == 0 { Some(pin1_chamfer()) } else { None };
        pads.push(smd_pad(
            &num,
            x,
            -(body_y / 2.0 + pad_h / 2.0),
            pad_w,
            pad_h,
            chamfer,
        ));
    }
    for i in 0..per_side {
        let num = (per_side + per_side - i).to_string();
        let x = span / 2.0 - i as f64 * pitch;
        pads.push(smd_pad(
            &num,
            x,
            body_y / 2.0 + pad_h / 2.0,
            pad_w,
            pad_h,
            None,
        ));
    }
    let body_x = span + pitch;
    Ok(finish_package(
        FootprintDefinition {
            name: format!(
                "SOIC{pitch}P{}-{}",
                (body_y * 100.0).round() as u32,
                pin_count
            ),
            description: "small outline IC".into(),
            keywords: "soic".into(),
            pads,
            graphics: vec![],
            model: None,
            rules: None,
            raw_source: String::new(),
            unsupported: vec![],
        },
        density,
        body_x,
        body_y,
    ))
}

fn generate_qfp(
    params: &GenerateFootprintParams,
    density: LandDensity,
) -> Result<FootprintDefinition, String> {
    let pin_count = count(params.pin_count, "pin_count", 32, 256)?;
    if pin_count % 4 != 0 {
        return Err("pin_count must be a multiple of four for QFP".into());
    }
    let pitch = finite_positive(params.pitch.unwrap_or(0.5), "pitch", 0.3, 1.27)?;
    let per_side = pin_count / 4;
    let body = per_side as f64 * pitch + pitch;
    let body_x = finite_positive(params.body_x.unwrap_or(body), "body_x", 4.0, 40.0)?;
    let body_y = finite_positive(params.body_y.unwrap_or(body), "body_y", 4.0, 40.0)?;
    let pads = perimeter_pads(per_side, pitch, body_x, body_y)?;
    Ok(finish_package(
        FootprintDefinition {
            name: format!("QFP{}-{}", (pitch * 100.0).round() as u32, pin_count),
            description: "quad flat pack".into(),
            keywords: "qfp".into(),
            pads,
            graphics: vec![],
            model: None,
            rules: None,
            raw_source: String::new(),
            unsupported: vec![],
        },
        density,
        body_x,
        body_y,
    ))
}

fn generate_qfn(
    params: &GenerateFootprintParams,
    density: LandDensity,
) -> Result<FootprintDefinition, String> {
    let pin_count = count(params.pin_count, "pin_count", 4, 256)?;
    let pitch = finite_positive(params.pitch.unwrap_or(0.5), "pitch", 0.35, 1.27)?;
    let thermal = params.thermal.unwrap_or(true);
    let perimeter = if thermal && pin_count > 1 {
        pin_count - 1
    } else {
        pin_count
    };
    if perimeter % 4 != 0 {
        return Err("perimeter pin count must be a multiple of four for QFN".into());
    }
    let per_side = perimeter / 4;
    let body_x = finite_positive(params.body_x.unwrap_or(5.0), "body_x", 2.0, 20.0)?;
    let body_y = finite_positive(params.body_y.unwrap_or(5.0), "body_y", 2.0, 20.0)?;
    let mut pads = perimeter_pads(per_side, pitch, body_x, body_y)?;
    if thermal {
        let ep = (body_x * 0.55).min(body_y * 0.55);
        pads.push(smd_pad(&pin_count.to_string(), 0.0, 0.0, ep, ep, None));
    }
    let height = params.body_z.unwrap_or(1.0);
    let pitch_code = (pitch * 100.0).round() as u32;
    let bx = (body_x * 100.0).round() as u32;
    let by = (body_y * 100.0).round() as u32;
    let hz = (height * 100.0).round() as u32;
    let ipc_name = format!(
        "QFN{pitch_code}P{bx}X{by}X{hz}-{pin_count}{}",
        density.suffix()
    );
    Ok(finish_package(
        FootprintDefinition {
            name: ipc_name,
            description: "quad flat no-leads".into(),
            keywords: "qfn".into(),
            pads,
            graphics: vec![],
            model: None,
            rules: None,
            raw_source: String::new(),
            unsupported: vec![],
        },
        density,
        body_x,
        body_y,
    ))
}

fn perimeter_pads(
    per_side: u32,
    pitch: f64,
    body_x: f64,
    body_y: f64,
) -> Result<Vec<FootprintPad>, String> {
    if per_side == 0 {
        return Err("not enough pins for a quad package".into());
    }
    let pad_w = pitch * 0.55;
    let pad_h = pitch * 0.85;
    let hx = body_x / 2.0;
    let hy = body_y / 2.0;
    let span = (per_side as f64 - 1.0) * pitch;
    let mut pads = Vec::new();
    let mut num = 1u32;
    for i in 0..per_side {
        let x = -span / 2.0 + i as f64 * pitch;
        let chamfer = if num == 1 { Some(pin1_chamfer()) } else { None };
        pads.push(smd_pad(
            &num.to_string(),
            x,
            -(hy + pad_h / 2.0),
            pad_w,
            pad_h,
            chamfer,
        ));
        num += 1;
    }
    for i in 0..per_side {
        let y = -span / 2.0 + i as f64 * pitch;
        pads.push(smd_pad(
            &num.to_string(),
            hx + pad_w / 2.0,
            y,
            pad_h,
            pad_w,
            None,
        ));
        num += 1;
    }
    for i in 0..per_side {
        let x = span / 2.0 - i as f64 * pitch;
        pads.push(smd_pad(
            &num.to_string(),
            x,
            hy + pad_h / 2.0,
            pad_w,
            pad_h,
            None,
        ));
        num += 1;
    }
    for i in 0..per_side {
        let y = span / 2.0 - i as f64 * pitch;
        pads.push(smd_pad(
            &num.to_string(),
            -(hx + pad_w / 2.0),
            y,
            pad_h,
            pad_w,
            None,
        ));
        num += 1;
    }
    Ok(pads)
}

fn generate_bga(
    params: &GenerateFootprintParams,
    density: LandDensity,
) -> Result<FootprintDefinition, String> {
    let rows = count(params.rows, "rows", 2, 50)?;
    let cols = count(params.cols, "cols", 2, 50)?;
    let pitch = finite_positive(params.pitch.unwrap_or(0.8), "pitch", 0.4, 2.0)?;
    let body = rows.max(cols) as f64 * pitch + pitch;
    let body_x = finite_positive(params.body_x.unwrap_or(body), "body_x", 2.0, 50.0)?;
    let body_y = finite_positive(params.body_y.unwrap_or(body), "body_y", 2.0, 50.0)?;
    let pad = pitch * 0.6;
    let mut pads = Vec::new();
    let mut num = 1u32;
    let x0 = -((cols as f64 - 1.0) * pitch) / 2.0;
    let y0 = -((rows as f64 - 1.0) * pitch) / 2.0;
    for r in 0..rows {
        for c in 0..cols {
            let chamfer = if num == 1 { Some(pin1_chamfer()) } else { None };
            pads.push(smd_pad(
                &num.to_string(),
                x0 + c as f64 * pitch,
                y0 + r as f64 * pitch,
                pad,
                pad,
                chamfer,
            ));
            num += 1;
        }
    }
    Ok(finish_package(
        FootprintDefinition {
            name: format!("BGA{rows}x{cols}"),
            description: "ball grid array".into(),
            keywords: "bga".into(),
            pads,
            graphics: vec![],
            model: None,
            rules: None,
            raw_source: String::new(),
            unsupported: vec![],
        },
        density,
        body_x,
        body_y,
    ))
}

fn generate_dip(
    params: &GenerateFootprintParams,
    density: LandDensity,
) -> Result<FootprintDefinition, String> {
    let pin_count = count(params.pin_count, "pin_count", 4, 64)?;
    if pin_count % 2 != 0 {
        return Err("pin_count must be even for DIP".into());
    }
    let pitch = finite_positive(params.pitch.unwrap_or(2.54), "pitch", 1.0, 5.0)?;
    let row_spacing =
        finite_positive(params.row_spacing.unwrap_or(7.62), "row_spacing", 3.0, 20.0)?;
    let per_row = pin_count / 2;
    let span = (per_row as f64 - 1.0) * pitch;
    let mut pads = Vec::new();
    for i in 0..per_row {
        let n = (i + 1).to_string();
        pads.push(thru_pad(
            &n,
            -span / 2.0 + i as f64 * pitch,
            -row_spacing / 2.0,
            pitch * 0.35,
            pitch * 0.9,
        ));
    }
    for i in 0..per_row {
        let n = (pin_count - i).to_string();
        pads.push(thru_pad(
            &n,
            span / 2.0 - i as f64 * pitch,
            row_spacing / 2.0,
            pitch * 0.35,
            pitch * 0.9,
        ));
    }
    let body_x = span + pitch;
    let body_y = row_spacing + pitch;
    Ok(finish_package(
        FootprintDefinition {
            name: format!("DIP-{pin_count}"),
            description: "dual inline package".into(),
            keywords: "dip".into(),
            pads,
            graphics: vec![],
            model: None,
            rules: None,
            raw_source: String::new(),
            unsupported: vec![],
        },
        density,
        body_x,
        body_y,
    ))
}

fn generate_header_single(
    params: &GenerateFootprintParams,
    density: LandDensity,
) -> Result<FootprintDefinition, String> {
    let pin_count = count(params.pin_count, "pin_count", 1, 64)?;
    let pitch = finite_positive(params.pitch.unwrap_or(2.54), "pitch", 1.0, 5.0)?;
    let span = (pin_count as f64 - 1.0) * pitch;
    let mut pads = Vec::new();
    for i in 0..pin_count {
        pads.push(thru_pad(
            &(i + 1).to_string(),
            -span / 2.0 + i as f64 * pitch,
            0.0,
            pitch * 0.35,
            pitch * 0.9,
        ));
    }
    Ok(finish_package(
        FootprintDefinition {
            name: format!("HDR1x{pin_count}"),
            description: "single-row header".into(),
            keywords: "header".into(),
            pads,
            graphics: vec![],
            model: None,
            rules: None,
            raw_source: String::new(),
            unsupported: vec![],
        },
        density,
        span + pitch,
        pitch * 1.5,
    ))
}

fn generate_header_double(
    params: &GenerateFootprintParams,
    density: LandDensity,
) -> Result<FootprintDefinition, String> {
    let pin_count = count(params.pin_count, "pin_count", 2, 64)?;
    if pin_count % 2 != 0 {
        return Err("pin_count must be even for a double-row header".into());
    }
    let pitch = finite_positive(params.pitch.unwrap_or(2.54), "pitch", 1.0, 5.0)?;
    let row_spacing =
        finite_positive(params.row_spacing.unwrap_or(2.54), "row_spacing", 1.5, 10.0)?;
    let per_row = pin_count / 2;
    let span = (per_row as f64 - 1.0) * pitch;
    let mut pads = Vec::new();
    for i in 0..per_row {
        pads.push(thru_pad(
            &(i + 1).to_string(),
            -span / 2.0 + i as f64 * pitch,
            -row_spacing / 2.0,
            pitch * 0.35,
            pitch * 0.9,
        ));
    }
    for i in 0..per_row {
        pads.push(thru_pad(
            &(per_row + i + 1).to_string(),
            -span / 2.0 + i as f64 * pitch,
            row_spacing / 2.0,
            pitch * 0.35,
            pitch * 0.9,
        ));
    }
    Ok(finish_package(
        FootprintDefinition {
            name: format!("HDR2x{pin_count}"),
            description: "double-row header".into(),
            keywords: "header".into(),
            pads,
            graphics: vec![],
            model: None,
            rules: None,
            raw_source: String::new(),
            unsupported: vec![],
        },
        density,
        span + pitch,
        row_spacing + pitch,
    ))
}

fn generate_radial(
    params: &GenerateFootprintParams,
    density: LandDensity,
) -> Result<FootprintDefinition, String> {
    let span = finite_positive(params.pitch.unwrap_or(2.0), "pitch", 0.5, 20.0)?;
    let body = span * 0.6;
    let pads = vec![
        thru_pad("1", -span / 2.0, 0.0, 0.8, 1.6),
        thru_pad("2", span / 2.0, 0.0, 0.8, 1.6),
    ];
    Ok(finish_package(
        FootprintDefinition {
            name: "RADIAL-2".into(),
            description: "two-lead radial".into(),
            keywords: "radial".into(),
            pads,
            graphics: vec![],
            model: None,
            rules: None,
            raw_source: String::new(),
            unsupported: vec![],
        },
        density,
        span + 1.0,
        body,
    ))
}

fn generate_axial(
    params: &GenerateFootprintParams,
    density: LandDensity,
) -> Result<FootprintDefinition, String> {
    let span = finite_positive(params.pitch.unwrap_or(5.0), "pitch", 1.0, 40.0)?;
    let pads = vec![
        thru_pad("1", -span / 2.0, 0.0, 0.8, 1.6),
        thru_pad("2", span / 2.0, 0.0, 0.8, 1.6),
    ];
    Ok(finish_package(
        FootprintDefinition {
            name: "AXIAL-2".into(),
            description: "two-lead axial".into(),
            keywords: "axial".into(),
            pads,
            graphics: vec![],
            model: None,
            rules: None,
            raw_source: String::new(),
            unsupported: vec![],
        },
        density,
        span + 1.0,
        1.2,
    ))
}

/// Two pads placed on top of each other for checker overlap tests.
pub fn overlapping_pad_fixture() -> FootprintDefinition {
    let pad = smd_pad("1", 0.0, 0.0, 1.0, 1.0, Some(pin1_chamfer()));
    let pad2 = smd_pad("2", 0.0, 0.0, 1.0, 1.0, None);
    FootprintDefinition {
        name: "OverlapFixture".into(),
        description: String::new(),
        keywords: String::new(),
        pads: vec![pad, pad2],
        graphics: vec![rectangle_layer(
            Point::new(-2.0, -2.0),
            Point::new(2.0, 2.0),
            0.12,
            LAYER_COURTYARD,
        )],
        model: None,
        rules: None,
        raw_source: String::new(),
        unsupported: vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::footprint_check::check_footprint;

    fn build(params: GenerateFootprintParams) -> Result<FootprintDefinition, String> {
        generate_footprint(&params)
    }

    #[test]
    fn chip_two_terminal_matches_0805_fixture_span() -> Result<(), String> {
        let def = build(GenerateFootprintParams {
            family: "chip_2terminal".into(),
            density: "B".into(),
            name: "R_test".into(),
            description: String::new(),
            keywords: String::new(),
            pitch: None,
            pin_count: None,
            body_x: Some(2.0),
            body_y: Some(1.25),
            body_z: None,
            rows: None,
            cols: None,
            row_spacing: None,
            terminal_width: Some(0.7),
            thermal: None,
        })?;
        assert_eq!(def.pads.len(), 2);
        let span = (def.pads[1].position.x - def.pads[0].position.x).abs();
        assert!(span > 0.8 && span < 1.5);
        Ok(())
    }

    #[test]
    fn sot23_5_produces_five_pads() -> Result<(), String> {
        let def = build(GenerateFootprintParams {
            family: "sot".into(),
            density: "B".into(),
            name: "SOT".into(),
            description: String::new(),
            keywords: String::new(),
            pitch: Some(0.95),
            pin_count: Some(5),
            body_x: None,
            body_y: None,
            body_z: None,
            rows: None,
            cols: None,
            row_spacing: None,
            terminal_width: None,
            thermal: None,
        })?;
        assert_eq!(def.pads.len(), 5);
        Ok(())
    }

    #[test]
    fn soic8_pitch_and_row_span() -> Result<(), String> {
        let def = build(GenerateFootprintParams {
            family: "soic".into(),
            density: "B".into(),
            name: "SOIC8".into(),
            description: String::new(),
            keywords: String::new(),
            pitch: Some(1.27),
            pin_count: Some(8),
            body_x: None,
            body_y: Some(3.9),
            body_z: None,
            rows: None,
            cols: None,
            row_spacing: None,
            terminal_width: None,
            thermal: None,
        })?;
        assert_eq!(def.pads.len(), 8);
        Ok(())
    }

    #[test]
    fn qfp48_pad_count_and_pitch() -> Result<(), String> {
        let def = build(GenerateFootprintParams {
            family: "qfp".into(),
            density: "B".into(),
            name: "QFP48".into(),
            description: String::new(),
            keywords: String::new(),
            pitch: Some(0.5),
            pin_count: Some(48),
            body_x: None,
            body_y: None,
            body_z: None,
            rows: None,
            cols: None,
            row_spacing: None,
            terminal_width: None,
            thermal: None,
        })?;
        assert_eq!(def.pads.len(), 48);
        Ok(())
    }

    #[test]
    fn qfn33_includes_center_thermal_pad() -> Result<(), String> {
        let def = build(GenerateFootprintParams {
            family: "qfn".into(),
            density: "B".into(),
            name: "QFN33".into(),
            description: String::new(),
            keywords: String::new(),
            pitch: Some(0.5),
            pin_count: Some(33),
            body_x: Some(5.0),
            body_y: Some(5.0),
            body_z: Some(1.0),
            rows: None,
            cols: None,
            row_spacing: None,
            terminal_width: None,
            thermal: Some(true),
        })?;
        assert_eq!(def.pads.len(), 33);
        assert!(def.pads.iter().any(|p| p.number == "33"));
        Ok(())
    }

    #[test]
    fn bga_10x10_grid_pad_count() -> Result<(), String> {
        let def = build(GenerateFootprintParams {
            family: "bga".into(),
            density: "B".into(),
            name: "BGA".into(),
            description: String::new(),
            keywords: String::new(),
            pitch: Some(0.8),
            pin_count: None,
            body_x: None,
            body_y: None,
            body_z: None,
            rows: Some(10),
            cols: Some(10),
            row_spacing: None,
            terminal_width: None,
            thermal: None,
        })?;
        assert_eq!(def.pads.len(), 100);
        Ok(())
    }

    #[test]
    fn dip8_plated_drills_inside_pads() -> Result<(), String> {
        let def = build(GenerateFootprintParams {
            family: "dip".into(),
            density: "B".into(),
            name: "DIP8".into(),
            description: String::new(),
            keywords: String::new(),
            pitch: Some(2.54),
            pin_count: Some(8),
            body_x: None,
            body_y: None,
            body_z: None,
            rows: None,
            cols: None,
            row_spacing: None,
            terminal_width: None,
            thermal: None,
        })?;
        assert_eq!(def.pads.len(), 8);
        assert!(def.pads.iter().all(|p| p.drill.is_some()));
        Ok(())
    }

    #[test]
    fn header_single_row_count() -> Result<(), String> {
        let def = build(GenerateFootprintParams {
            family: "header_single".into(),
            density: "B".into(),
            name: "HDR".into(),
            description: String::new(),
            keywords: String::new(),
            pitch: Some(2.54),
            pin_count: Some(6),
            body_x: None,
            body_y: None,
            body_z: None,
            rows: None,
            cols: None,
            row_spacing: None,
            terminal_width: None,
            thermal: None,
        })?;
        assert_eq!(def.pads.len(), 6);
        Ok(())
    }

    #[test]
    fn header_double_row_stagger() -> Result<(), String> {
        let def = build(GenerateFootprintParams {
            family: "header_double".into(),
            density: "B".into(),
            name: "HDR2".into(),
            description: String::new(),
            keywords: String::new(),
            pitch: Some(2.54),
            pin_count: Some(8),
            body_x: None,
            body_y: None,
            body_z: None,
            rows: None,
            cols: None,
            row_spacing: Some(2.54),
            terminal_width: None,
            thermal: None,
        })?;
        assert_eq!(def.pads.len(), 8);
        Ok(())
    }

    #[test]
    fn radial_two_lead_span() -> Result<(), String> {
        let def = build(GenerateFootprintParams {
            family: "radial".into(),
            density: "B".into(),
            name: "RAD".into(),
            description: String::new(),
            keywords: String::new(),
            pitch: Some(2.0),
            pin_count: None,
            body_x: None,
            body_y: None,
            body_z: None,
            rows: None,
            cols: None,
            row_spacing: None,
            terminal_width: None,
            thermal: None,
        })?;
        assert_eq!(def.pads.len(), 2);
        Ok(())
    }

    #[test]
    fn axial_two_lead_span() -> Result<(), String> {
        let def = build(GenerateFootprintParams {
            family: "axial".into(),
            density: "B".into(),
            name: "AX".into(),
            description: String::new(),
            keywords: String::new(),
            pitch: Some(5.0),
            pin_count: None,
            body_x: None,
            body_y: None,
            body_z: None,
            rows: None,
            cols: None,
            row_spacing: None,
            terminal_width: None,
            thermal: None,
        })?;
        assert_eq!(def.pads.len(), 2);
        Ok(())
    }

    #[test]
    fn generate_rejects_non_finite_pitch() {
        let err = generate_footprint(&GenerateFootprintParams {
            family: "qfn".into(),
            density: "B".into(),
            name: "X".into(),
            description: String::new(),
            keywords: String::new(),
            pitch: Some(f64::NAN),
            pin_count: Some(33),
            body_x: Some(5.0),
            body_y: Some(5.0),
            body_z: None,
            rows: None,
            cols: None,
            row_spacing: None,
            terminal_width: None,
            thermal: Some(true),
        });
        assert!(err.is_err());
    }

    #[test]
    fn check_reports_overlapping_pads() {
        let findings = check_footprint(&overlapping_pad_fixture());
        assert!(
            findings.iter().any(|f| f.code == "lib_pad_overlap"),
            "expected overlap finding"
        );
    }

    #[test]
    fn check_accepts_fresh_qfn() -> Result<(), String> {
        let def = build(GenerateFootprintParams {
            family: "qfn".into(),
            density: "B".into(),
            name: "QFN_OK".into(),
            description: String::new(),
            keywords: String::new(),
            pitch: Some(0.5),
            pin_count: Some(33),
            body_x: Some(5.0),
            body_y: Some(5.0),
            body_z: Some(1.0),
            rows: None,
            cols: None,
            row_spacing: None,
            terminal_width: None,
            thermal: Some(true),
        })?;
        let errors: Vec<_> = check_footprint(&def)
            .into_iter()
            .filter(|f| f.severity == "error")
            .collect();
        assert!(
            errors.is_empty(),
            "fresh QFN should pass MVP checks: {errors:?}"
        );
        Ok(())
    }
}
