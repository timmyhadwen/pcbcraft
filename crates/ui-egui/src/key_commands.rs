//! Keyboard command feed: turns this frame's key events into keymap actions,
//! including two-key sequences, and runs the actions that the classic keymap
//! added on top of the original set.
//!
//! # Rules
//! - Runs once per frame before any panel reads keys, and only when no text
//!   field has the keyboard (`App::shortcuts` checks that first).
//! - A key the keymap handles is removed from the frame's events so tool
//!   hotkeys, the layer stepper and widgets do not act on it a second time.
//!   Esc, Enter and Delete are the exception: they run their action and stay,
//!   because dialogs, pinned views and editors also listen for them.
//! - Every design change goes through `App::run` / `App::tool_call` (typed
//!   engine commands); view changes stay in `App` state.
//!
//! # Dependencies
//! - `crates/ui-egui/src/keymap.rs` the table, [`keymap::Keymap::press`].
//! - `crates/ui-egui/src/lib.rs` `App::shortcuts`, `App::dispatch_shortcut`.
//! - `crates/ui-egui/src/snap_grid.rs` the G and Q state.
//! - `crates/ui-egui/src/board_side.rs` `App::step_layer`.
//!
//! # Do not
//! - Read keys here while a text field has focus.
//! - Place an object without a net: the engine refuses labels that name none.

use super::*;
use crate::canvas::gestures::Hit;
use crate::keymap::{KeyAction, Press};

/// Keys that run their action but stay in the event list for other readers.
const SHARED_KEYS: [egui::Key; 3] = [egui::Key::Escape, egui::Key::Enter, egui::Key::Delete];
/// How much one PgUp or PgDn press zooms.
const ZOOM_STEP: f32 = 1.25;

impl App {
    /// Feed this frame's key presses through the active keymap and run what
    /// they resolve to.
    pub(super) fn keyboard_commands(&mut self, ctx: &egui::Context) {
        let map = self.preferences.active_keymap().clone();
        let (tab, command_open) = (self.tab, self.command_open);
        let mut prefix = self.key_prefix;
        let mut actions: Vec<KeyAction> = Vec::new();
        let mut cancelled = false;
        let mut took_tab = false;
        ctx.input_mut(|i| {
            let events = std::mem::take(&mut i.events);
            for event in events {
                let consumed = match &event {
                    egui::Event::Key {
                        key,
                        pressed: true,
                        repeat,
                        modifiers,
                        ..
                    } if !keymap::is_modifier_key(*key) => {
                        let chord = keymap::chord_from_input(*key, *modifiers);
                        if *repeat {
                            // Held keys repeat only the zoom steps.
                            match map.resolve(chord, tab, command_open) {
                                Some(a @ (KeyAction::ZoomIn | KeyAction::ZoomOut))
                                    if prefix.is_none() =>
                                {
                                    actions.push(a);
                                    true
                                }
                                _ => false,
                            }
                        } else {
                            match map.press(&mut prefix, chord, tab, command_open) {
                                Press::Prefix(_) => true,
                                Press::Cancelled => {
                                    cancelled = true;
                                    *key != egui::Key::Escape
                                }
                                Press::Run(
                                    KeyAction::PaletteLineUp
                                    | KeyAction::PaletteLineDown
                                    | KeyAction::PaletteRun,
                                ) => false,
                                Press::Run(action) => {
                                    actions.push(action);
                                    took_tab |= *key == egui::Key::Tab;
                                    !SHARED_KEYS.contains(key)
                                }
                                Press::Pass => false,
                            }
                        }
                    }
                    egui::Event::Text(text) if prefix.is_none() => {
                        match keymap::TEXT_KEYS.iter().find(|(typed, action)| {
                            *typed == text.as_str() && keymap::action_available(*action, tab)
                        }) {
                            Some((_, action)) => {
                                actions.push(*action);
                                true
                            }
                            None => false,
                        }
                    }
                    _ => false,
                };
                if !consumed {
                    i.events.push(event);
                }
            }
        });
        if took_tab {
            // Tab is a shortcut here, not focus travel to the first widget.
            ctx.memory_mut(|m| m.move_focus(egui::FocusDirection::None));
        }
        self.key_prefix = prefix;
        if cancelled {
            self.message = t!("keys.prefix.cancelled");
        }
        for action in actions {
            self.dispatch_shortcut(action);
        }
    }

    /// The pending-prefix hint for the status bar: "P … W Wire · N Net label".
    pub(super) fn prefix_hint(&self) -> Option<String> {
        let first = self.key_prefix?;
        let options: Vec<String> = self
            .preferences
            .active_keymap()
            .completions(first, self.tab, self.command_open)
            .into_iter()
            .map(|(key, action)| {
                format!("{} {}", keymap::key_name(key), keymap::action_name(action))
            })
            .collect();
        Some(t!(
            "keys.prefix.pending",
            key = keymap::key_name(first),
            options = options.join(" · ")
        ))
    }

    /// Run one of the actions the classic keymap added. Returns false for an
    /// action this function does not own.
    pub(super) fn classic_action(&mut self, action: KeyAction) -> bool {
        match action {
            KeyAction::ZoomIn => self.zoom_step(ZOOM_STEP),
            KeyAction::ZoomOut => self.zoom_step(1.0 / ZOOM_STEP),
            KeyAction::Redraw => self.message = t!("keys.redraw"),
            KeyAction::CycleGrid => {
                let step = snap_grid::cycle();
                self.message = t!(
                    "keys.grid",
                    step = snap_grid::format_length(step, self.preferences.mil_units)
                );
            }
            KeyAction::ToggleUnits => {
                self.preferences.mil_units = !self.preferences.mil_units;
                self.message = if self.preferences.mil_units {
                    t!("keys.units.mil")
                } else {
                    t!("keys.units.mm")
                };
            }
            KeyAction::Measure => self.arm_measure(),
            KeyAction::PlaceNetLabel => self.place_label_at_cursor("label"),
            KeyAction::PlaceJunction => self.place_label_at_cursor("junction"),
            KeyAction::PlacePowerPort => {
                if self.net_under_cursor().is_some() {
                    self.place_label_at_cursor("power");
                } else {
                    self.add_power_port(self.active_page);
                }
            }
            KeyAction::PlaceFootprint => {
                self.preferences.left_tab = 1;
                self.message = t!("keys.place_footprint");
            }
            KeyAction::TextTool => self.arm_hotkey(tool_modifiers::Hotkey::Text),
            KeyAction::CopperLayerNext => self.next_copper_layer(),
            KeyAction::SingleLayerMode => self.toggle_single_layer(),
            KeyAction::LayerPanel => {
                self.preferences.left_tab = 0;
                self.message = t!("keys.layer_panel");
            }
            KeyAction::CycleCornerMode => {
                let chamfer = !self.preferences.chamfer_corners;
                self.preferences.chamfer_corners = chamfer;
                // Pushing other tracks aside needs 45 degree corners.
                if !chamfer {
                    self.preferences.route_shove = false;
                }
                self.message = if chamfer {
                    t!("keys.corners.45")
                } else {
                    t!("keys.corners.90")
                };
            }
            KeyAction::CycleRouteMode => {
                let shove = !self.preferences.route_shove;
                self.preferences.route_shove = shove;
                if shove {
                    self.preferences.chamfer_corners = true;
                }
                self.message = if shove {
                    t!("keys.route_mode.push")
                } else {
                    t!("keys.route_mode.stop")
                };
            }
            _ => return false,
        }
        true
    }

    /// Zoom the active canvas by `factor`, keeping the point under the cursor
    /// still when the cursor is over the canvas.
    fn zoom_step(&mut self, factor: f32) {
        let rect = self.canvas_rect;
        let cursor = self.cursor;
        let camera = if self.tab == Tab::Pcb {
            &mut self.pcb
        } else {
            &mut self.schematic
        };
        let anchor = cursor.map(|c| camera.screen(c, rect));
        camera.zoom = (camera.zoom * factor).clamp(0.02, 100.0);
        if let (Some(world), Some(screen)) = (cursor, anchor) {
            let after = camera.world(screen, rect);
            camera.center = camera
                .center
                .offset(Point::new(world.x - after.x, world.y - after.y));
        }
    }

    /// The schematic net under the pointer, else the selected label's or wire's.
    fn net_under_cursor(&self) -> Option<Id> {
        let wire_net = |id: Id| {
            self.project
                .drawn_wires
                .iter()
                .find(|w| w.id == id)
                .and_then(|w| w.net)
        };
        let label_net = |id: Id| {
            self.project
                .labels
                .iter()
                .find(|l| l.id == id)
                .map(|l| l.net)
        };
        let hovered = self.edit.hover.as_ref().and_then(|h| match &h.hit {
            Hit::WireSegment(id, _) | Hit::Node(id, _) => wire_net(*id),
            Hit::NetLabel(id) | Hit::Junction(id) => label_net(*id),
            Hit::Pin(pin) => self
                .project
                .wires
                .iter()
                .find(|w| w.from == *pin || w.to == *pin)
                .map(|w| w.net),
            _ => None,
        });
        hovered.or_else(|| {
            let id = self.selected?;
            wire_net(id).or_else(|| label_net(id))
        })
    }

    /// Where a junction or label lands: on the hovered wire when there is one,
    /// else the snapped cursor.
    fn label_point(&self) -> Option<Point> {
        if let Some(hover) = &self.edit.hover {
            let wire = |id: Id| self.project.drawn_wires.iter().find(|w| w.id == id);
            match hover.hit {
                Hit::Node(id, i) => {
                    if let Some(p) = wire(id).and_then(|w| w.points.get(i)) {
                        return Some(*p);
                    }
                }
                Hit::WireSegment(id, i) => {
                    if let (Some(w), Some(at)) = (wire(id), self.cursor)
                        && let (Some(a), Some(b)) = (w.points.get(i), w.points.get(i + 1))
                    {
                        return Some(closest_on_segment(at, *a, *b));
                    }
                }
                _ => {}
            }
        }
        self.cursor.map(canvas::snap)
    }

    fn place_label_at_cursor(&mut self, kind: &str) {
        let Some(net) = self.net_under_cursor() else {
            self.message = t!("keys.label.no_net");
            return;
        };
        let Some(at) = self.label_point() else {
            self.message = t!("keys.label.no_cursor");
            return;
        };
        self.run(
            "label.place",
            json!({"kind": kind, "net": net, "position": {"x": at.x, "y": at.y}, "sheet": self.active_page}),
        );
    }

    /// Step down the stack to the next copper layer, wrapping to the top.
    fn next_copper_layer(&mut self) {
        use crate::draw_target::Target;
        let start = self.current_target();
        let mut wrapped = false;
        // The stack has at most a few dozen levels; the bound only guards a loop.
        for _ in 0..256 {
            let before = self.current_target();
            self.step_layer(false);
            let now = self.current_target();
            if now == before {
                if wrapped {
                    return;
                }
                wrapped = true;
                for _ in 0..256 {
                    let at = self.current_target();
                    self.step_layer(true);
                    if self.current_target() == at {
                        break;
                    }
                }
            }
            let now = self.current_target();
            if matches!(now, Target::Copper(_)) && now != start {
                return;
            }
        }
    }

    /// Shift+S: dim everything but the active layer, or show all again.
    fn toggle_single_layer(&mut self) {
        use crate::draw_target::Target;
        if self.focus_layer.take().is_some() {
            self.message = t!("keys.single_layer.off");
            return;
        }
        let key = match self.current_target() {
            Target::Copper(l) if l == pcbcraft_doc::Layer::Front => Some("copper:front".into()),
            Target::Copper(l) if l == pcbcraft_doc::Layer::Back => Some("copper:back".into()),
            Target::User(id) => self
                .project
                .board
                .user_layers
                .iter()
                .find(|l| l.id == id)
                .map(|l| format!("art:{}", l.name)),
            _ => None,
        };
        match key {
            Some(key) => {
                self.focus_layer = Some(key);
                self.message = t!("keys.single_layer.on", name = self.target_label());
            }
            None => self.message = t!("keys.single_layer.unsupported"),
        }
    }
}

/// The point on segment `a`..`b` nearest to `p`.
fn closest_on_segment(p: Point, a: Point, b: Point) -> Point {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let length2 = dx * dx + dy * dy;
    if length2 <= f64::EPSILON {
        return a;
    }
    let t = (((p.x - a.x) * dx + (p.y - a.y) * dy) / length2).clamp(0.0, 1.0);
    Point::new(a.x + t * dx, a.y + t * dy)
}
