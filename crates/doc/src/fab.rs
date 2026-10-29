//! Fabrication rule minimums and named capability presets.
use crate::Id;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

fn fab_bound(v: f64) -> bool {
    v.is_finite() && (0.001..=100.0).contains(&v)
}

/// Project fabrication minimums and preset metadata. Absent on a board means
/// no fab floor is recorded beyond the default copper clearance.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct FabRules {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_track: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_space: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_drill: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_npth: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_ring_via: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_ring_pth: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hole_to_hole_via: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hole_to_hole_pad: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hole_to_copper_via: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hole_to_copper_pth: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hole_to_copper_npth: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edge_routed: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edge_vscore: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub silk_line: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub silk_text_height: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub silk_to_pad: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mask_dam: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_aspect_ratio: Option<f64>,
    /// Finished-hole diameter added when measuring annular ring against the
    /// drill tool, when the preset records one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plating_allowance: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retrieved: Option<String>,
}

impl FabRules {
    pub fn valid(&self) -> bool {
        let ok = |v: Option<f64>| v.is_none_or(fab_bound);
        ok(self.min_track)
            && ok(self.min_space)
            && ok(self.min_drill)
            && ok(self.min_npth)
            && ok(self.min_ring_via)
            && ok(self.min_ring_pth)
            && ok(self.hole_to_hole_via)
            && ok(self.hole_to_hole_pad)
            && ok(self.hole_to_copper_via)
            && ok(self.hole_to_copper_pth)
            && ok(self.hole_to_copper_npth)
            && ok(self.edge_routed)
            && ok(self.edge_vscore)
            && ok(self.silk_line)
            && ok(self.silk_text_height)
            && ok(self.silk_to_pad)
            && ok(self.mask_dam)
            && self
                .max_aspect_ratio
                .is_none_or(|r| r.is_finite() && (1.0..=50.0).contains(&r))
            && self
                .plating_allowance
                .is_none_or(|p| p.is_finite() && (0.0..=0.5).contains(&p))
            && self
                .preset
                .as_ref()
                .is_none_or(|s| !s.is_empty() && s.len() <= 64)
            && self
                .source_url
                .as_ref()
                .is_none_or(|s| !s.is_empty() && s.len() <= 512)
            && self
                .retrieved
                .as_ref()
                .is_none_or(|s| !s.is_empty() && s.len() <= 32)
    }

    pub fn drill_tool(&self, finished_drill: f64) -> f64 {
        finished_drill + self.plating_allowance.unwrap_or(0.0)
    }
}

/// Clearance between every net in one class and every net in another.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ClassPairRule {
    pub class_a: Id,
    pub class_b: Id,
    pub clearance: f64,
}

impl ClassPairRule {
    pub fn valid(&self) -> bool {
        self.class_a != self.class_b
            && self.class_a != 0
            && self.class_b != 0
            && fab_bound(self.clearance)
    }

    pub fn key(&self) -> (Id, Id) {
        (
            self.class_a.min(self.class_b),
            self.class_a.max(self.class_b),
        )
    }
}

pub const FAB_PRESET_NAMES: &[&str] = &[
    "JLCPCB 1-2L",
    "OSH Park 2L",
    "AISLER 2L HASL 35um",
    "Eurocircuits class 6",
];

#[derive(Clone, Debug)]
pub struct FabPreset {
    pub name: &'static str,
    pub rules: FabRules,
}

pub fn fab_preset(name: &str) -> Option<FabPreset> {
    fab_presets().iter().find(|p| p.name == name).cloned()
}

pub fn fab_preset_list() -> Vec<&'static str> {
    FAB_PRESET_NAMES.to_vec()
}

fn fab_presets() -> &'static [FabPreset] {
    FAB_PRESETS.as_slice()
}

fn rules(
    min_track: Option<f64>,
    min_space: Option<f64>,
    min_drill: Option<f64>,
    min_npth: Option<f64>,
    min_ring_via: Option<f64>,
    min_ring_pth: Option<f64>,
    hole_to_hole_via: Option<f64>,
    hole_to_hole_pad: Option<f64>,
    hole_to_copper_via: Option<f64>,
    hole_to_copper_pth: Option<f64>,
    hole_to_copper_npth: Option<f64>,
    edge_routed: Option<f64>,
    edge_vscore: Option<f64>,
    silk_line: Option<f64>,
    silk_text_height: Option<f64>,
    silk_to_pad: Option<f64>,
    mask_dam: Option<f64>,
    max_aspect_ratio: Option<f64>,
    plating_allowance: Option<f64>,
    preset: &str,
    source_url: &str,
    retrieved: &str,
) -> FabRules {
    FabRules {
        min_track,
        min_space,
        min_drill,
        min_npth,
        min_ring_via,
        min_ring_pth,
        hole_to_hole_via,
        hole_to_hole_pad,
        hole_to_copper_via,
        hole_to_copper_pth,
        hole_to_copper_npth,
        edge_routed,
        edge_vscore,
        silk_line,
        silk_text_height,
        silk_to_pad,
        mask_dam,
        max_aspect_ratio,
        plating_allowance,
        preset: Some(preset.into()),
        source_url: Some(source_url.into()),
        retrieved: Some(retrieved.into()),
    }
}

static FAB_PRESETS: LazyLock<Vec<FabPreset>> = LazyLock::new(|| {
    vec![
        FabPreset {
            name: "JLCPCB 1-2L",
            rules: rules(
                Some(0.127),
                Some(0.127),
                Some(0.3),
                Some(0.3),
                Some(0.075),
                Some(0.075),
                Some(0.3),
                Some(0.3),
                Some(0.2),
                Some(0.2),
                Some(0.2),
                Some(0.3),
                None,
                Some(0.15),
                Some(1.0),
                Some(0.15),
                None,
                Some(20.0),
                Some(0.025),
                "JLCPCB 1-2L",
                "https://jlcpcb.com/capabilities/Capabilities",
                "2026-03-28",
            ),
        },
        FabPreset {
            name: "OSH Park 2L",
            rules: rules(
                Some(0.1524),
                Some(0.1524),
                Some(0.254),
                Some(0.254),
                Some(0.1524),
                Some(0.1524),
                Some(0.254),
                Some(0.254),
                Some(0.254),
                Some(0.254),
                Some(0.254),
                Some(0.254),
                None,
                Some(0.1524),
                Some(1.016),
                Some(0.1524),
                None,
                Some(16.0),
                Some(0.025),
                "OSH Park 2L",
                "https://docs.oshpark.com/design-rules/2-layer-design-rules/",
                "2026-03-28",
            ),
        },
        FabPreset {
            name: "AISLER 2L HASL 35um",
            rules: rules(
                Some(0.15),
                Some(0.15),
                Some(0.3),
                Some(0.3),
                Some(0.075),
                Some(0.075),
                Some(0.3),
                Some(0.3),
                Some(0.2),
                Some(0.2),
                Some(0.2),
                Some(0.3),
                None,
                Some(0.15),
                Some(1.0),
                Some(0.15),
                None,
                Some(20.0),
                Some(0.035),
                "AISLER 2L HASL 35um",
                "https://aisler.net/pcb-specs",
                "2026-03-28",
            ),
        },
        FabPreset {
            name: "Eurocircuits class 6",
            rules: rules(
                Some(0.1),
                Some(0.1),
                Some(0.25),
                Some(0.25),
                Some(0.05),
                Some(0.075),
                Some(0.25),
                Some(0.25),
                Some(0.15),
                Some(0.15),
                Some(0.15),
                Some(0.25),
                None,
                Some(0.125),
                Some(0.8),
                Some(0.125),
                None,
                Some(25.0),
                Some(0.025),
                "Eurocircuits class 6",
                "https://www.eurocircuits.com/pcb-design-guidelines/",
                "2026-03-28",
            ),
        },
    ]
});
