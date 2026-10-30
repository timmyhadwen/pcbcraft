//! The canvas cursor and the heads-up display: a crosshair drawn at the
//! snapped cursor and a small translucent readout beside it with the cursor
//! position, the distance from the last click and, on the board, the layer
//! the tools draw on. Also the view preferences that switch these, the
//! rulers and the board dimension arrows on and off.
//!
//! Context: `App::canvas` in `crates/ui-egui/src/canvas.rs` calls
//! [`App::paint_cursor_and_hud`] last, over everything else it draws; the
//! Settings page calls [`App::canvas_view_settings`]; the View menu
//! (`crates/ui-egui/src/classic_menus.rs`) toggles the same preferences.
//!
//! # Rules
//! - Colours come from `BoardArt` on the board and `SheetColours` on the
//!   schematic; text uses `crate::fonts` roles.
//! - Painting only: the readout costs a few shapes per frame, whatever the
//!   size of the design.
//!
//! # Do not
//! - Read modifier keys here; the snapped, constrained point comes from the
//!   caller.
use super::*;
use serde::{Deserialize, Serialize};

/// How the pointer looks over the canvases.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub(crate) enum CursorStyle {
    /// The system arrow, nothing drawn.
    Arrow,
    /// A short crosshair at the snapped point; the system pointer hides.
    #[default]
    SmallCrosshair,
    /// Lines across the whole canvas through the snapped point.
    FullCrosshair,
}

impl CursorStyle {
    /// Every style, in the order Settings lists them.
    pub(crate) const ALL: [Self; 3] = [Self::Arrow, Self::SmallCrosshair, Self::FullCrosshair];

    fn name(self) -> String {
        match self {
            Self::Arrow => t!("canvas.cursor.arrow"),
            Self::SmallCrosshair => t!("canvas.cursor.small"),
            Self::FullCrosshair => t!("canvas.cursor.full"),
        }
    }
}

/// View preferences of the two canvases, saved with the other preferences.
///
/// # Rules
/// - A field missing from an older preferences file takes its default.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct CanvasPrefs {
    /// The readout beside the cursor (Shift+H).
    pub hud: bool,
    /// How the pointer looks over the canvases.
    pub cursor: CursorStyle,
    /// The millimetre rulers along the top and left of the board.
    pub rulers: bool,
    /// The width and height arrows outside the board edge.
    pub dimensions: bool,
}

impl Default for CanvasPrefs {
    fn default() -> Self {
        Self {
            hud: true,
            cursor: CursorStyle::SmallCrosshair,
            rulers: false,
            dimensions: false,
        }
    }
}

/// Screen distance of the readout from the pointer, right and down.
const HUD_OFFSET: Vec2 = Vec2::new(18.0, 18.0);
/// Half the length of the small crosshair's arms.
const SMALL_ARM: f32 = 10.0;
/// Where the canvas keeps the last click of an active tool, for dX and dY.
const ANCHOR_KEY: &str = "canvas-hud-anchor";

/// Colours of the cursor and the readout for one canvas.
#[derive(Clone, Copy)]
struct HudInk {
    cursor: Color32,
    fill: Color32,
    edge: Color32,
    text: Color32,
    muted: Color32,
}

impl App {
    /// The Settings rows for the canvas view: readout, cursor, rulers and
    /// dimension arrows.
    pub(crate) fn canvas_view_settings(&mut self, ui: &mut egui::Ui) {
        let prefs = &mut self.preferences.canvas;
        ui.checkbox(&mut prefs.hud, t!("settings.canvas.hud"))
            .on_hover_text(t!("settings.canvas.hud.tip"));
        ui.horizontal_wrapped(|ui| {
            ui.label(t!("settings.canvas.cursor"));
            for style in CursorStyle::ALL {
                ui.selectable_value(&mut prefs.cursor, style, style.name())
                    .on_hover_text(t!("settings.canvas.cursor.tip"));
            }
        });
        ui.checkbox(&mut prefs.rulers, t!("menu.view.rulers"))
            .on_hover_text(t!("menu.view.rulers.tip"));
        ui.checkbox(&mut prefs.dimensions, t!("menu.view.dimensions"))
            .on_hover_text(t!("menu.view.dimensions.tip"));
    }

    /// Remember where an active tool was last clicked, or forget it once the
    /// select tool is back.
    pub(super) fn note_hud_anchor(&self, ctx: &egui::Context, clicked: Option<Point>) {
        let id = egui::Id::new(ANCHOR_KEY);
        if matches!(self.tool, Tool::Select) {
            ctx.data_mut(|d| d.remove_temp::<Point>(id));
        } else if let Some(at) = clicked {
            ctx.data_mut(|d| d.insert_temp(id, snap(at)));
        }
    }

    fn hud_ink(&self) -> HudInk {
        if self.tab == Tab::Pcb {
            let art = self.art();
            HudInk {
                cursor: art.cursor,
                fill: art.hud_fill,
                edge: art.grid_major,
                text: art.text,
                muted: art.muted,
            }
        } else {
            let sc = sheet::SheetColours::for_theme(self.colours().dark);
            HudInk {
                cursor: sc.cursor,
                fill: sc.hud_fill,
                edge: sc.frame,
                text: sc.text,
                muted: sc.muted,
            }
        }
    }

    /// The crosshair at `at` (the snapped cursor in world units) and the
    /// readout beside it. Hides the system pointer while a crosshair shows.
    ///
    /// # Rules
    /// - Draws nothing while the pointer is off the canvas.
    /// - The readout flips to the other side of the pointer near the right
    ///   or bottom edge, so it never leaves the canvas.
    pub(super) fn paint_cursor_and_hud(
        &self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        rect: Rect,
        camera: Camera,
        at: Option<Point>,
        hovered: bool,
    ) {
        let Some(at) = at.filter(|_| hovered) else {
            return;
        };
        let ink = self.hud_ink();
        let screen = camera.screen(at, rect);
        let stroke = Stroke::new(1.0, ink.cursor);
        match self.preferences.canvas.cursor {
            CursorStyle::Arrow => {}
            CursorStyle::SmallCrosshair => {
                painter.line_segment(
                    [
                        screen - Vec2::new(SMALL_ARM, 0.0),
                        screen + Vec2::new(SMALL_ARM, 0.0),
                    ],
                    stroke,
                );
                painter.line_segment(
                    [
                        screen - Vec2::new(0.0, SMALL_ARM),
                        screen + Vec2::new(0.0, SMALL_ARM),
                    ],
                    stroke,
                );
            }
            CursorStyle::FullCrosshair => {
                painter.line_segment(
                    [
                        Pos2::new(rect.left(), screen.y),
                        Pos2::new(rect.right(), screen.y),
                    ],
                    stroke,
                );
                painter.line_segment(
                    [
                        Pos2::new(screen.x, rect.top()),
                        Pos2::new(screen.x, rect.bottom()),
                    ],
                    stroke,
                );
            }
        }
        if self.preferences.canvas.cursor != CursorStyle::Arrow {
            ui.ctx().set_cursor_icon(egui::CursorIcon::None);
        }
        if !self.preferences.canvas.hud {
            return;
        }
        let mil = self.preferences.mil_units;
        let font = crate::fonts::Role::Caption.font();
        let mut lines = vec![(
            t!(
                "canvas.hud.position",
                value = crate::snap_grid::format_point(at.x, at.y, mil)
            ),
            None,
        )];
        let anchor: Option<Point> = ui
            .ctx()
            .data(|d| d.get_temp(egui::Id::new(ANCHOR_KEY)))
            .filter(|_| !matches!(self.tool, Tool::Select));
        if let Some(from) = anchor {
            lines.push((
                t!(
                    "canvas.hud.delta",
                    value = crate::snap_grid::format_point(at.x - from.x, at.y - from.y, mil)
                ),
                None,
            ));
        }
        if self.tab == Tab::Pcb {
            let target = self.current_target();
            lines.push((self.target_label(), Some(self.target_colour(target))));
        }
        let chip = 8.0;
        let galleys: Vec<_> = lines
            .into_iter()
            .enumerate()
            .map(|(i, (text, colour))| {
                let tint = if i == 0 { ink.text } else { ink.muted };
                (painter.layout_no_wrap(text, font.clone(), tint), colour)
            })
            .collect();
        let pad = Vec2::new(6.0, 4.0);
        let gap = 1.0;
        let width = galleys
            .iter()
            .map(|(g, c)| g.size().x + if c.is_some() { chip + 4.0 } else { 0.0 })
            .fold(0.0f32, f32::max);
        let height = galleys.iter().map(|(g, _)| g.size().y + gap).sum::<f32>() - gap;
        let size = Vec2::new(width, height) + pad * 2.0;
        let mut origin = screen + HUD_OFFSET;
        if origin.x + size.x > rect.right() {
            origin.x = screen.x - HUD_OFFSET.x - size.x;
        }
        if origin.y + size.y > rect.bottom() {
            origin.y = screen.y - HUD_OFFSET.y - size.y;
        }
        let card = Rect::from_min_size(origin, size);
        painter.rect_filled(card, 3.0, ink.fill);
        painter.rect_stroke(
            card,
            3.0,
            Stroke::new(1.0, ink.edge),
            egui::StrokeKind::Inside,
        );
        let mut y = card.top() + pad.y;
        for (galley, colour) in galleys {
            let mut x = card.left() + pad.x;
            let line_height = galley.size().y;
            if let Some(colour) = colour {
                let swatch = Rect::from_min_size(
                    Pos2::new(x, y + (line_height - chip) / 2.0),
                    Vec2::splat(chip),
                );
                painter.rect_filled(swatch, 1.0, colour);
                x += chip + 4.0;
            }
            painter.galley(Pos2::new(x, y), galley, ink.text);
            y += line_height + gap;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_old_preferences_file_gets_the_new_view_defaults() -> Result<(), String> {
        let prefs: CanvasPrefs = serde_json::from_str("{}").map_err(|e| e.to_string())?;
        assert_eq!(prefs, CanvasPrefs::default());
        assert!(prefs.hud, "the readout starts on");
        assert_eq!(prefs.cursor, CursorStyle::SmallCrosshair);
        assert!(!prefs.rulers && !prefs.dimensions, "placed dimensions only");
        Ok(())
    }
}
