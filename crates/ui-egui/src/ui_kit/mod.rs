//! The component kit of the approved app design: small widget helpers that every
//! page adopts instead of drawing its own buttons, tabs, rows and fields.
//!
//! Context: tokens come from `crates/ui-egui/src/theme.rs`; the gallery in
//! `./gallery.rs` draws every component in both themes and is the visual proof.
//! Local rules: each helper takes a label and a tooltip, returns a `Response`
//! and sets `widget_info`; colour, size and radius come from `theme`, never literals.
//!
//! # Dependencies
//! - crates/ui-egui/src/theme.rs
//! - crates/ui-egui/src/ui_kit/gallery.rs
//! - crates/ui-egui/src/ui_kit/button.rs
//! - crates/ui-egui/src/ui_kit/chip.rs
//! - crates/ui-egui/src/ui_kit/focus.rs
//! - crates/ui-egui/src/ui_kit/icons.rs
//!
//! # Do not
//! - Touch `crates/ui-egui/src/lib.rs` from here; pages call into the kit, not the reverse.

pub mod button;
pub mod chip;
pub mod focus;
pub mod gallery;
pub mod icons;
