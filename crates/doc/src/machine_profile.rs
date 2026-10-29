//! Shop machine profiles stored on the design for fabrication artifacts.
use serde::{Deserialize, Serialize};

pub const MAX_PROFILE_ROWS: usize = 64;

const MAX_ID_LEN: usize = 64;
const MAX_NAME_LEN: usize = 128;
const MAX_VENDOR_LEN: usize = 128;
const MAX_MODEL_LEN: usize = 128;
const MAX_NOTES_LEN: usize = 4096;
const MAX_SHORT: usize = 64;
const MAX_MEDIUM: usize = 256;

fn valid_slug(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_ID_LEN
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn bounded(s: &str, max: usize) -> bool {
    !s.is_empty() && s.len() <= max
}

fn optional_bounded(s: &str, max: usize) -> bool {
    s.len() <= max
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct WorkArea {
    pub x: f64,
    pub y: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub z: Option<f64>,
    pub origin: String,
    pub y_up: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct MachineOutput {
    pub format: String,
    pub dialect: String,
    pub units: String,
    pub line_ending: String,
    pub mirror_bottom: bool,
    pub mirror_axis: String,
    pub invert: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct LaserOperation {
    pub name: String,
    pub mode: String,
    pub speed_mm_s: f64,
    pub power_percent: f64,
    pub passes: u32,
    pub frequency_khz: f64,
    pub pulse_width_ns: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_interval_mm: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hatch_angle_deg: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub angle_increment_deg: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kerf_offset_mm: Option<f64>,
    #[serde(default)]
    pub verified: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct LaserSection {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub operation: Vec<LaserOperation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field_x_mm: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field_y_mm: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_watts: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wavelength_nm: Option<f64>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct SpindleSection {
    pub speed_style: String,
    pub max_rpm: f64,
    pub default_percent: f64,
    pub min_percent: f64,
    pub max_percent: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct GcodeSection {
    pub header_type: String,
    pub tool_head: String,
    pub machine_label: String,
    pub flavor: String,
    pub arcs_allowed: bool,
    pub tool_change: String,
    pub safe_z_mm: f64,
    pub work_speed_mm_min: f64,
    pub jog_speed_mm_min: f64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub preamble: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub postamble: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct MillTool {
    pub id: String,
    pub diameter_mm: f64,
    pub feed_mm_min: f64,
    pub plunge_mm_min: f64,
    pub spindle_percent: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct MillMaterial {
    pub name: String,
    pub thickness_mm: f64,
    pub safe_z_mm: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct MillSection {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spindle: Option<SpindleSection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gcode: Option<GcodeSection>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool: Vec<MillTool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub material: Option<MillMaterial>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct PickPlaceBoard {
    pub max_x_mm: f64,
    pub max_y_mm: f64,
    pub origin_top: String,
    pub origin_bottom: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct PickPlaceRotation {
    pub min_deg: f64,
    pub max_deg: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct PickPlaceCsv {
    pub native_header: String,
    pub import_columns: u32,
    pub line_ending: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct FeederBank {
    pub name: String,
    pub first: u32,
    pub last: u32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct NozzleRow {
    pub id: String,
    pub diameter_mm: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct RotationOffsetRow {
    pub comment: String,
    pub footprint: String,
    pub offset_deg: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct FootprintAliasRow {
    pub pattern: String,
    pub replacement: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct FeederMapRow {
    pub comment: String,
    pub footprint: String,
    pub feeder_no: u32,
    pub nozzle: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct PickPlaceSection {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub board: Option<PickPlaceBoard>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation: Option<PickPlaceRotation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub csv: Option<PickPlaceCsv>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub feeders: Vec<FeederBank>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nozzle: Vec<NozzleRow>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rotation_offset: Vec<RotationOffsetRow>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub footprint_alias: Vec<FootprintAliasRow>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub feeder_map: Vec<FeederMapRow>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct UvPrintSection {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dpi_options: Vec<u32>,
    pub default_dpi: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub channels: Vec<String>,
    pub registration: String,
    pub white_choke_mm: f64,
    pub max_substrate_height_mm: f64,
    pub relief_mm: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct UvArtworkSection {
    pub default_bleed_mm: f64,
    pub pad_clearance_mm: f64,
    pub min_text_height_mm: f64,
    pub min_stroke_mm: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct UvLimitsSection {
    pub max_width_mm: f64,
    pub max_height_mm: f64,
    pub max_pixels: u32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct UvPrinterSection {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub print: Option<UvPrintSection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artwork: Option<UvArtworkSection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limits: Option<UvLimitsSection>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct MachineProfile {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub vendor: String,
    pub model: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
    pub work_area: WorkArea,
    pub output: MachineOutput,
    #[serde(default, skip_serializing_if = "LaserSection::is_empty")]
    pub laser: LaserSection,
    #[serde(default, skip_serializing_if = "MillSection::is_empty")]
    pub mill: MillSection,
    #[serde(default, skip_serializing_if = "PickPlaceSection::is_empty")]
    pub pick_place: PickPlaceSection,
    #[serde(default, skip_serializing_if = "UvPrinterSection::is_empty")]
    pub uv_printer: UvPrinterSection,
}

impl LaserSection {
    fn is_empty(&self) -> bool {
        self.operation.is_empty()
            && self.field_x_mm.is_none()
            && self.field_y_mm.is_none()
            && self.source_watts.is_none()
            && self.wavelength_nm.is_none()
    }
}

impl MillSection {
    fn is_empty(&self) -> bool {
        self.spindle.is_none()
            && self.gcode.is_none()
            && self.tool.is_empty()
            && self.material.is_none()
    }
}

impl PickPlaceSection {
    fn is_empty(&self) -> bool {
        self.board.is_none()
            && self.rotation.is_none()
            && self.csv.is_none()
            && self.feeders.is_empty()
            && self.nozzle.is_empty()
            && self.rotation_offset.is_empty()
            && self.footprint_alias.is_empty()
            && self.feeder_map.is_empty()
    }
}

impl UvPrinterSection {
    fn is_empty(&self) -> bool {
        self.print.is_none() && self.artwork.is_none() && self.limits.is_none()
    }
}

impl MachineProfile {
    pub fn validate(&self) -> Result<(), String> {
        if !valid_slug(&self.id) {
            return Err("machine profile id must be 1 to 64 lowercase letters, digits or hyphens".into());
        }
        if !bounded(&self.name, MAX_NAME_LEN) {
            return Err("machine profile name must be 1 to 128 bytes".into());
        }
        if !optional_bounded(&self.vendor, MAX_VENDOR_LEN)
            || !optional_bounded(&self.model, MAX_MODEL_LEN)
            || !optional_bounded(&self.notes, MAX_NOTES_LEN)
        {
            return Err("machine profile vendor, model or notes exceed the length limit".into());
        }
        let kind = self.kind.as_str();
        if !matches!(
            kind,
            "laser_galvo" | "laser_gantry" | "cnc_mill" | "pick_place" | "uv_printer"
        ) {
            return Err("machine profile kind must be laser_galvo, laser_gantry, cnc_mill, pick_place or uv_printer".into());
        }
        self.validate_work_area()?;
        self.validate_output()?;
        match kind {
            "laser_galvo" | "laser_gantry" => {
                if !self.mill.is_empty() || !self.pick_place.is_empty() || !self.uv_printer.is_empty()
                {
                    return Err("laser profiles may only carry laser settings".into());
                }
                self.validate_laser()?;
            }
            "cnc_mill" => {
                if !self.laser.is_empty() || !self.pick_place.is_empty() || !self.uv_printer.is_empty()
                {
                    return Err("mill profiles may only carry mill settings".into());
                }
                self.validate_mill()?;
            }
            "pick_place" => {
                if !self.laser.is_empty() || !self.mill.is_empty() || !self.uv_printer.is_empty() {
                    return Err("pick and place profiles may only carry placement settings".into());
                }
                self.validate_pick_place()?;
            }
            "uv_printer" => {
                if !self.laser.is_empty() || !self.mill.is_empty() || !self.pick_place.is_empty() {
                    return Err("uv printer profiles may only carry print settings".into());
                }
                self.validate_uv_printer()?;
            }
            _ => {}
        }
        Ok(())
    }

    fn validate_work_area(&self) -> Result<(), String> {
        let w = &self.work_area;
        if !w.x.is_finite() || !w.y.is_finite() || w.x <= 0.0 || w.y <= 0.0 || w.x > 5000.0 || w.y > 5000.0
        {
            return Err("work area width and height must be positive finite millimetres".into());
        }
        if w.z.is_some_and(|z| !z.is_finite() || z <= 0.0 || z > 5000.0) {
            return Err("work area depth must be a positive finite millimetre value".into());
        }
        if !optional_bounded(&w.origin, MAX_SHORT) {
            return Err("work area origin label is too long".into());
        }
        Ok(())
    }

    fn validate_output(&self) -> Result<(), String> {
        let o = &self.output;
        if !optional_bounded(&o.format, MAX_SHORT)
            || !optional_bounded(&o.dialect, MAX_SHORT)
            || !optional_bounded(&o.units, MAX_SHORT)
            || !optional_bounded(&o.line_ending, MAX_SHORT)
            || !optional_bounded(&o.mirror_axis, MAX_SHORT)
        {
            return Err("output header fields are too long".into());
        }
        Ok(())
    }

    fn validate_laser(&self) -> Result<(), String> {
        if self.laser.operation.len() > MAX_PROFILE_ROWS {
            return Err("laser profiles carry at most 64 operations".into());
        }
        for op in &self.laser.operation {
            if !bounded(&op.name, MAX_SHORT) || !optional_bounded(&op.mode, MAX_SHORT) {
                return Err("laser operation name or mode is invalid".into());
            }
            if !op.speed_mm_s.is_finite()
                || op.speed_mm_s <= 0.0
                || op.speed_mm_s > 10_000.0
                || !op.power_percent.is_finite()
                || op.power_percent < 0.0
                || op.power_percent > 100.0
                || op.passes == 0
                || op.passes > 1000
                || !op.frequency_khz.is_finite()
                || op.frequency_khz <= 0.0
                || !op.pulse_width_ns.is_finite()
                || op.pulse_width_ns <= 0.0
            {
                return Err("laser operation speed, power or timing is out of range".into());
            }
        }
        Ok(())
    }

    fn validate_mill(&self) -> Result<(), String> {
        if self.mill.tool.len() > MAX_PROFILE_ROWS {
            return Err("mill profiles carry at most 64 tools".into());
        }
        if let Some(spindle) = &self.mill.spindle {
            if !spindle.default_percent.is_finite()
                || spindle.default_percent < 50.0
                || spindle.default_percent > 100.0
                || !spindle.min_percent.is_finite()
                || spindle.min_percent < 50.0
                || spindle.min_percent > 100.0
                || !spindle.max_percent.is_finite()
                || spindle.max_percent < 50.0
                || spindle.max_percent > 100.0
                || spindle.min_percent > spindle.max_percent
            {
                return Err("mill spindle percent must stay between 50 and 100".into());
            }
        }
        for tool in &self.mill.tool {
            if !bounded(&tool.id, MAX_SHORT) {
                return Err("mill tool id must be 1 to 64 bytes".into());
            }
            if !tool.diameter_mm.is_finite()
                || tool.diameter_mm <= 0.0
                || tool.diameter_mm > 100.0
                || !tool.feed_mm_min.is_finite()
                || tool.feed_mm_min <= 0.0
                || tool.feed_mm_min > 50_000.0
                || !tool.plunge_mm_min.is_finite()
                || tool.plunge_mm_min <= 0.0
                || tool.plunge_mm_min > 50_000.0
                || !tool.spindle_percent.is_finite()
                || tool.spindle_percent < 50.0
                || tool.spindle_percent > 100.0
            {
                return Err("mill tool diameter, feed or spindle percent is out of range".into());
            }
        }
        Ok(())
    }

    fn validate_pick_place(&self) -> Result<(), String> {
        if self.pick_place.rotation_offset.len() > MAX_PROFILE_ROWS
            || self.pick_place.footprint_alias.len() > MAX_PROFILE_ROWS
            || self.pick_place.feeder_map.len() > MAX_PROFILE_ROWS
            || self.pick_place.nozzle.len() > MAX_PROFILE_ROWS
            || self.pick_place.feeders.len() > MAX_PROFILE_ROWS
        {
            return Err("pick and place tables are capped at 64 rows each".into());
        }
        Ok(())
    }

    fn validate_uv_printer(&self) -> Result<(), String> {
        if let Some(print) = &self.uv_printer.print {
            if print.dpi_options.len() > 8 || print.dpi_options.is_empty() {
                return Err("uv printer needs one to eight dpi choices".into());
            }
            if print.dpi_options.iter().any(|d| *d < 300 || *d > 4800) {
                return Err("uv printer dpi choices must be between 300 and 4800".into());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_laser() -> MachineProfile {
        MachineProfile {
            id: "test-laser".into(),
            name: "Test laser".into(),
            kind: "laser_galvo".into(),
            vendor: "Lab".into(),
            model: "X".into(),
            notes: String::new(),
            work_area: WorkArea {
                x: 150.0,
                y: 150.0,
                z: None,
                origin: "center".into(),
                y_up: true,
            },
            output: MachineOutput {
                format: "svg".into(),
                dialect: "laser_job".into(),
                units: "mm".into(),
                line_ending: "lf".into(),
                mirror_bottom: true,
                mirror_axis: "x".into(),
                invert: false,
            },
            laser: LaserSection {
                operation: vec![LaserOperation {
                    name: "fiducials".into(),
                    mode: "line".into(),
                    speed_mm_s: 100.0,
                    power_percent: 30.0,
                    passes: 1,
                    frequency_khz: 50.0,
                    pulse_width_ns: 200.0,
                    line_interval_mm: None,
                    hatch_angle_deg: None,
                    angle_increment_deg: None,
                    kerf_offset_mm: None,
                    verified: false,
                }],
                field_x_mm: Some(150.0),
                field_y_mm: Some(150.0),
                source_watts: Some(30.0),
                wavelength_nm: Some(1064.0),
            },
            ..Default::default()
        }
    }

    fn minimal_mill() -> MachineProfile {
        MachineProfile {
            id: "test-mill".into(),
            name: "Test mill".into(),
            kind: "cnc_mill".into(),
            vendor: "Lab".into(),
            model: "Y".into(),
            notes: String::new(),
            work_area: WorkArea {
                x: 320.0,
                y: 325.0,
                z: Some(275.0),
                origin: "lower-left".into(),
                y_up: false,
            },
            output: MachineOutput {
                format: "gcode".into(),
                dialect: "snapmaker".into(),
                units: "mm".into(),
                line_ending: "lf".into(),
                mirror_bottom: false,
                mirror_axis: "x".into(),
                invert: false,
            },
            mill: MillSection {
                spindle: Some(SpindleSection {
                    speed_style: "percent".into(),
                    max_rpm: 18_000.0,
                    default_percent: 100.0,
                    min_percent: 50.0,
                    max_percent: 100.0,
                }),
                gcode: Some(GcodeSection {
                    header_type: "cnc".into(),
                    tool_head: "levelTwoCNCToolheadForSM2".into(),
                    machine_label: "Snapmaker 2.0 A350".into(),
                    flavor: "marlin".into(),
                    arcs_allowed: false,
                    tool_change: "new_file".into(),
                    safe_z_mm: 5.0,
                    work_speed_mm_min: 300.0,
                    jog_speed_mm_min: 1500.0,
                    preamble: vec!["G90".into(), "G21".into()],
                    postamble: vec!["M400".into(), "M5".into()],
                }),
                tool: vec![MillTool {
                    id: "iso".into(),
                    diameter_mm: 0.2,
                    feed_mm_min: 300.0,
                    plunge_mm_min: 100.0,
                    spindle_percent: 100.0,
                }],
                material: Some(MillMaterial {
                    name: "fr4".into(),
                    thickness_mm: 1.6,
                    safe_z_mm: 2.0,
                }),
            },
            ..Default::default()
        }
    }

    #[test]
    fn machine_profile_rejects_negative_work_area() {
        let mut p = minimal_laser();
        p.work_area.x = -1.0;
        assert!(p.validate().is_err());
        p.work_area.x = 150.0;
        p.work_area.y = 0.0;
        assert!(p.validate().is_err());
    }

    #[test]
    fn machine_profile_rejects_feed_or_spindle_out_of_range() {
        let mut p = minimal_mill();
        p.mill.tool[0].feed_mm_min = 0.0;
        assert!(p.validate().is_err());
        p.mill.tool[0].feed_mm_min = 300.0;
        p.mill.tool[0].spindle_percent = 40.0;
        assert!(p.validate().is_err());
        if let Some(spindle) = p.mill.spindle.as_mut() {
            spindle.default_percent = 40.0;
        }
        p.mill.tool[0].spindle_percent = 100.0;
        assert!(p.validate().is_err());
    }

    #[test]
    fn machine_profiles_round_trip_through_project_json() -> Result<(), String> {
        let mut project = crate::Project::default();
        project.machine_profiles.push(minimal_laser());
        project.machine_profiles.push(minimal_mill());
        project.validate()?;
        let json = serde_json::to_string(&project).map_err(|e| e.to_string())?;
        let back: crate::Project = serde_json::from_str(&json).map_err(|e| e.to_string())?;
        back.validate()?;
        if back.machine_profiles.len() != 2 || back.machine_profiles[0].id != "test-laser" {
            return Err("machine profiles did not round trip".into());
        }
        Ok(())
    }
}
