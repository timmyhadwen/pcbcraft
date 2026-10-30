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

thread_local! {
    static STEP: Cell<f64> = const { Cell::new(DEFAULT_STEP) };
}

/// The current snap step in millimetres.
pub fn step() -> f64 {
    STEP.with(Cell::get)
}

/// Set the snap step; non-finite or non-positive values are ignored.
pub fn set_step(mm: f64) {
    if mm.is_finite() && mm > 0.0 {
        STEP.with(|s| s.set(mm));
    }
}

/// Move to the next step in [`STEPS`], wrapping, and return it.
pub fn cycle() -> f64 {
    let now = step();
    let at = STEPS
        .iter()
        .position(|s| (s - now).abs() < 1e-9)
        .map_or(0, |i| (i + 1) % STEPS.len());
    set_step(STEPS[at]);
    STEPS[at]
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
}
