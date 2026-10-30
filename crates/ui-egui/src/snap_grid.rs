//! The placement snap step and the readout units, switched from the keyboard.
//!
//! G steps the snap grid through [`STEPS`]; Q flips the status bar between
//! millimetres and mils. The step lives in a thread-local like the snap
//! suspension in `tool_modifiers`, so `canvas::snap` can read it without an
//! `App` and each headless test thread keeps its own value.
//!
//! # Rules
//! - The design is always millimetres; mils are a readout only.
//! - The step is never zero or negative: [`set_step`] ignores such values.
//!
//! # Dependencies
//! - `crates/ui-egui/src/canvas.rs` `snap` rounds to [`step`].
//! - `crates/ui-egui/src/lib.rs` `App::dispatch_shortcut` and the status bar.
//!
//! # Do not
//! - Change the painted dot grid from here; it keeps its own spacing.

use std::cell::Cell;

/// Snap steps in millimetres, in the order G walks them.
pub const STEPS: [f64; 6] = [0.1, 0.25, 0.5, 1.0, 1.27, 2.54];
/// The step a new session starts with.
pub const DEFAULT_STEP: f64 = 0.5;
/// Millimetres in one mil (a thousandth of an inch).
const MM_PER_MIL: f64 = 0.0254;

/// Schematic snap steps: half, whole and double the 100 mil pin pitch, so a
/// part dropped on the sheet keeps its pins on the grid wires join on.
pub const SHEET_STEPS: [f64; 3] = [1.27, 2.54, 5.08];
/// The schematic step a new session starts with: 100 mil.
pub const SHEET_DEFAULT_STEP: f64 = 2.54;

thread_local! {
    static STEP: Cell<f64> = const { Cell::new(DEFAULT_STEP) };
    static SHEET_STEP: Cell<f64> = const { Cell::new(SHEET_DEFAULT_STEP) };
    /// True while the schematic is the canvas on screen.
    static ON_SHEET: Cell<bool> = const { Cell::new(false) };
}

/// Say which canvas is on screen; the schematic keeps a grid of its own.
pub fn set_sheet(on_sheet: bool) {
    ON_SHEET.with(|s| s.set(on_sheet));
}

/// The current snap step in millimetres, for the canvas on screen.
pub fn step() -> f64 {
    if ON_SHEET.with(Cell::get) {
        SHEET_STEP.with(Cell::get)
    } else {
        STEP.with(Cell::get)
    }
}

/// Set the snap step of the canvas on screen; non-finite or non-positive
/// values are ignored.
pub fn set_step(mm: f64) {
    if mm.is_finite() && mm > 0.0 {
        if ON_SHEET.with(Cell::get) {
            SHEET_STEP.with(|s| s.set(mm));
        } else {
            STEP.with(|s| s.set(mm));
        }
    }
}

/// Move to the next step of the canvas on screen, wrapping, and return it.
pub fn cycle() -> f64 {
    let steps: &[f64] = if ON_SHEET.with(Cell::get) {
        &SHEET_STEPS
    } else {
        &STEPS
    };
    let now = step();
    let at = steps
        .iter()
        .position(|s| (s - now).abs() < 1e-9)
        .map_or(0, |i| (i + 1) % steps.len());
    set_step(steps[at]);
    steps[at]
}

/// Round a coordinate in millimetres to the current step.
pub fn round(mm: f64) -> f64 {
    let s = step();
    (mm / s).round() * s
}

/// A length in the chosen readout units, with its unit suffix.
pub fn format_length(mm: f64, mil: bool) -> String {
    if mil {
        format!("{:.1} mil", mm / MM_PER_MIL)
    } else {
        format!("{mm:.2} mm")
    }
}

/// An x, y pair in the chosen readout units: "12.50, 3.00 mm".
pub fn format_point(x: f64, y: f64, mil: bool) -> String {
    if mil {
        format!("{:.1}, {:.1} mil", x / MM_PER_MIL, y / MM_PER_MIL)
    } else {
        format!("{x:.2}, {y:.2} mm")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn g_walks_every_step_and_wraps_back() {
        set_step(DEFAULT_STEP);
        let mut seen = vec![];
        for _ in 0..STEPS.len() {
            seen.push(cycle());
        }
        assert_eq!(seen.last().copied(), Some(DEFAULT_STEP));
        assert!(STEPS.iter().all(|s| seen.contains(s)));
        assert!((round(1.3) - 1.5).abs() < 1e-9);
    }

    #[test]
    fn mils_are_a_readout_of_millimetres() {
        assert_eq!(format_point(2.54, 0.0, true), "100.0, 0.0 mil");
        assert_eq!(format_length(1.0, false), "1.00 mm");
        set_step(-1.0);
        assert!(step() > 0.0);
    }

    #[test]
    fn the_schematic_keeps_its_own_grid() {
        set_sheet(false);
        set_step(DEFAULT_STEP);
        set_sheet(true);
        set_step(SHEET_DEFAULT_STEP);
        assert!(
            (round(3.9) - 5.08).abs() < 1e-9,
            "parts land on the 100 mil grid"
        );
        assert!((cycle() - 5.08).abs() < 1e-9, "G walks the sheet steps");
        set_sheet(false);
        assert!(
            (step() - DEFAULT_STEP).abs() < 1e-9,
            "the board grid is untouched"
        );
        set_sheet(true);
        set_step(SHEET_DEFAULT_STEP);
        set_sheet(false);
    }
}
