use pcbcraft_library::{Graphic, read_footprint, read_symbols};

#[test]
fn real_device_symbols_preserve_pin_types_and_graphics() -> Result<(), Box<dyn std::error::Error>> {
    let symbols = read_symbols(include_str!("fixtures/device.kicad_sym"))?;
    let resistor = symbols.iter().find(|s| s.name == "R").ok_or("R missing")?;
    assert_eq!(resistor.reference, "R");
    assert_eq!(resistor.pins.len(), 2);
    assert!(resistor.unsupported.is_empty());
    assert!(resistor.pins.iter().all(|p| p.electrical_type == "passive"));
    assert!(resistor.pins.iter().any(|p| p.position.y < 0.0));
    assert!(
        resistor
            .graphics
            .iter()
            .any(|g| matches!(g.primitive, Graphic::Rectangle { .. }))
    );
    let led = symbols
        .iter()
        .find(|s| s.name == "LED")
        .ok_or("LED missing")?;
    assert_eq!(led.pins.len(), 2);
    assert!(led.pins.iter().any(|p| p.name == "A"));
    assert!(!led.graphics.is_empty());
    assert!(!led.raw_source.starts_with("(kicad_symbol_lib"));
    Ok(())
}
#[test]
fn real_ic_resolves_inheritance_and_units() -> Result<(), Box<dyn std::error::Error>> {
    let symbols = read_symbols(include_str!("fixtures/lm358.kicad_sym"))?;
    let ic = symbols
        .iter()
        .find(|s| s.name == "LM358")
        .ok_or("LM358 missing")?;
    assert_eq!(ic.extends.as_deref(), Some("LM2904"));
    assert_eq!(ic.value, "LM358");
    assert_eq!(ic.pins.len(), 8);
    assert!(
        ic.pins
            .iter()
            .any(|p| p.unit == 3 && p.electrical_type == "power_in")
    );
    assert!(ic.graphics.iter().any(|g| g.unit == 2));
    Ok(())
}
#[test]
fn real_0805_preserves_roundrect_pads_and_layers() -> Result<(), Box<dyn std::error::Error>> {
    let fp = read_footprint(include_str!("fixtures/R_0805_2012Metric.kicad_mod"))?;
    assert_eq!(fp.pads.len(), 2);
    assert!(fp.pads.iter().all(|p| p.shape == "roundrect"
        && p.pad_type == "smd"
        && p.layers.iter().any(|l| l == "F.Cu")));
    assert!(fp.pads[0].position.x < 0.0);
    assert!(fp.pads[0].roundrect_ratio.is_some());
    assert!(
        fp.graphics
            .iter()
            .any(|g| matches!(g,Graphic::Rectangle{layer,..}if layer=="F.CrtYd"))
    );
    assert!(fp.unsupported.is_empty());
    assert!(fp.raw_source.contains("(model"));
    Ok(())
}

#[test]
fn real_dip8_preserves_through_holes_and_reference_marker() -> Result<(), Box<dyn std::error::Error>>
{
    let fp = read_footprint(include_str!("fixtures/DIP-8_W7.62mm.kicad_mod"))?;
    assert_eq!(fp.pads.len(), 8);
    assert!(fp.unsupported.is_empty());
    assert!(fp.pads.iter().all(|p| {
        p.pad_type == "thru_hole"
            && p.layers.iter().any(|l| l == "*.Cu")
            && p.drill
                .as_ref()
                .is_some_and(|d| d.size.x == 0.8 && d.size.y == 0.8)
    }));
    assert_eq!(fp.pads[0].shape, "roundrect");
    assert!(fp.raw_source.contains("(point"));
    assert!(read_footprint("(footprint X (point (at NaN 0) (size 1) (layer F.Fab)))").is_err());
    Ok(())
}

#[test]
#[ignore = "requires installed KiCad 10 libraries in Linux/WSL"]
fn installed_library_catalogs() -> Result<(), Box<dyn std::error::Error>> {
    for name in ["Device", "Amplifier_Operational"] {
        let text = std::fs::read_to_string(format!("/usr/share/kicad/symbols/{name}.kicad_sym"))?;
        let symbols = read_symbols(&text)?;
        assert!(symbols.len() > 100);
    }
    Ok(())
}
#[test]
fn hostile_inputs_and_native_dto_validation() -> Result<(), Box<dyn std::error::Error>> {
    for source in [
        "(",
        "(kicad_symbol_lib))",
        "(kicad_symbol_lib (symbol A (extends B)) (symbol B (extends A)))",
        "(kicad_symbol_lib (symbol A (extends Missing)))",
    ] {
        assert!(read_symbols(source).is_err());
    }
    let deep = format!("{}{}", "(".repeat(70), ")".repeat(70));
    assert!(read_symbols(&deep).is_err());
    assert!(
        read_footprint("(footprint X (pad 1 smd circle (at NaN 0) (size 1 1) (layers F.Cu)))")
            .is_err()
    );
    let mut fp = read_footprint(include_str!("fixtures/R_0805_2012Metric.kicad_mod"))?;
    fp.pads[0].size.x = f64::INFINITY;
    assert!(fp.validate().is_err());
    let mut symbols = read_symbols(include_str!("fixtures/device.kicad_sym"))?;
    let s = symbols.first_mut().ok_or("no symbols")?;
    s.pins.push(s.pins[0].clone());
    assert!(s.validate().is_err());
    Ok(())
}
#[test]
fn arbitrary_pin_numbers_unknown_geometry_and_drills() -> Result<(), Box<dyn std::error::Error>> {
    let s = read_symbols(
        "(kicad_symbol_lib (symbol X (symbol X_1_1 (pin input line (at 1 2 90) (length 2) (name CLK) (number A12)))))",
    )?;
    assert_eq!(s[0].pins[0].number, "A12");
    assert_eq!(s[0].pins[0].position.y, -2.0);
    let fp = read_footprint(
        "(footprint X (pad A12 thru_hole custom (at 1 2 90) (size 2 3) (drill oval 1 2 (offset 0.1 0.2)) (layers *.Cu *.Mask) (primitives (gr_poly))))",
    )?;
    assert_eq!(fp.pads[0].position.y, 2.0);
    assert_eq!(fp.pads[0].rotation, 90.0);
    assert_eq!(fp.pads[0].drill.as_ref().ok_or("no drill")?.size.y, 2.0);
    assert!(!fp.unsupported.is_empty());
    Ok(())
}

#[test]
fn inheritance_overrides_properties_and_replaces_local_units()
-> Result<(), Box<dyn std::error::Error>> {
    let symbols = read_symbols(
        r#"(kicad_symbol_lib
        (symbol Parent (property "Reference" "U") (property "Description" "base")
          (symbol Parent_1_1 (pin passive line (at 0 0) (length 1) (name OLD) (number 1)))
          (symbol Parent_0_1 (rectangle (start -1 -1) (end 1 1))))
        (symbol Child (extends "Parent") (property "Description" "override")
          (symbol Child_1_1 (pin input line (at 1 2 90) (length 2) (name NEW) (number A))))
    )"#,
    )?;
    let child = symbols
        .iter()
        .find(|s| s.name == "Child")
        .ok_or("missing child")?;
    assert_eq!(child.reference, "U");
    assert_eq!(child.description, "override");
    assert_eq!(child.pins.len(), 1);
    assert_eq!(child.pins[0].number, "A");
    assert_eq!(child.graphics[0].unit, 0);
    assert!(read_symbols("(kicad_symbol_lib (symbol X (symbol X_1_1) (symbol X_1_1)))").is_err());
    Ok(())
}

/// An original footprint written for this test: a chamfered polarity pad, a
/// concave custom pad, and unsupported variants that must be reported rather
/// than silently simplified.
const SHAPED_PADS: &str = r#"(footprint "ShapedPads" (version 20240108) (generator "test")
  (layer "F.Cu")
  (pad "1" smd roundrect (at -1.5 0) (size 1.2 1.6) (layers "F.Cu" "F.Mask")
    (roundrect_rratio 0) (chamfer_ratio 0.3) (chamfer top_left bottom_left))
  (pad "2" smd custom (at 1.5 0) (size 0.4 0.4) (layers "F.Cu" "F.Mask")
    (options (clearance outline) (anchor rect))
    (primitives
      (gr_poly (pts (xy -0.6 -0.8) (xy 0.6 -0.8) (xy 0.6 0.8) (xy 0 0.4) (xy -0.6 0.8)) (width 0) (fill yes))))
  (pad "3" smd rect (at 0 2) (size 1 1) (layers "F.Cu") (chamfer_ratio 0.2) (chamfer bottom_right))
  (pad "4" smd roundrect (at 0 -2) (size 1 1) (layers "F.Cu") (roundrect_rratio 0.25) (chamfer_ratio 0.2) (chamfer top_right))
  (pad "5" smd custom (at 3 2) (size 0.4 0.4) (layers "F.Cu")
    (primitives (gr_poly (pts (xy -1 -1) (xy 1 1) (xy 1 -1) (xy -1 1)) (width 0) (fill yes))))
  (pad "6" smd custom (at 3 -2) (size 0.4 0.4) (layers "F.Cu")
    (primitives (gr_circle (center 0 0) (end 0.5 0) (width 0) (fill yes))))
)"#;

#[test]
fn chamfered_and_custom_kicad_pads_import_with_exact_outlines()
-> Result<(), Box<dyn std::error::Error>> {
    use pcbcraft_library::{ChamferCorners, PadShape};
    let f = read_footprint(SHAPED_PADS)?;
    let pad = |n: &str| f.pads.iter().find(|p| p.number == n).ok_or("missing pad");
    let one = pad("1")?;
    assert_eq!(one.shape, "chamfered");
    assert_eq!(one.roundrect_ratio, None);
    assert_eq!(
        one.shape_kind(),
        PadShape::Chamfered(pcbcraft_library::PadChamfer {
            ratio: 0.3,
            corners: ChamferCorners {
                top_left: true,
                bottom_left: true,
                ..Default::default()
            }
        })
    );
    let two = pad("2")?;
    assert_eq!(two.shape, "custom");
    assert_eq!(two.polygon.len(), 5);
    assert!(matches!(two.shape_kind(), PadShape::Custom { polygon } if polygon.len() == 5));
    // A plain rect with a chamfer is a chamfered pad as well.
    assert_eq!(pad("3")?.shape, "chamfered");
    // Rounded and chamfered together, self-intersecting polygons and
    // non-polygon primitives are kept as data but flagged unsupported.
    assert_eq!(pad("4")?.shape, "roundrect");
    assert!(pad("4")?.chamfer.is_none());
    assert!(pad("5")?.polygon.is_empty());
    assert!(pad("6")?.polygon.is_empty());
    for n in [
        "pad 4 chamfer",
        "pad 5:",
        "pad 6 custom primitive gr_circle",
    ] {
        assert!(
            f.unsupported.iter().any(|u| u.starts_with(n)),
            "{n} not reported in {:?}",
            f.unsupported
        );
    }
    assert_eq!(f.unsupported.len(), 3);
    Ok(())
}
