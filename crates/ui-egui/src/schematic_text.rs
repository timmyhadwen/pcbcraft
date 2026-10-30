//! Schematic notes: P,T opens a one-line box at the cursor; Enter puts the
//! words on the page on screen as a text graphic, Esc drops them.
//!
//! Dependencies: `crates/ui-egui/src/key_commands.rs` arms it from the keymap,
//! `crates/ui-egui/src/canvas.rs` shows the box each frame, and the engine
//! command `graphic.add` (with its `sheet`) stores the note.
use super::*;

/// Height of a placed note in millimetres.
const NOTE_MM: f64 = 2.0;

impl App {
    /// Open the note box at the cursor on the schematic.
    pub(crate) fn begin_schematic_text(&mut self) {
        if self.tab != Tab::Schematic {
            return;
        }
        match self.cursor.map(canvas::snap) {
            Some(at) => {
                self.schematic_text = Some((at, String::new()));
                self.message = t!("keys.schematic_text.type");
            }
            None => self.message = t!("keys.label.no_cursor"),
        }
    }

    /// Show the open note box, and place or drop the note when it is done.
    ///
    /// # Rules
    /// - Enter with words places them on the page on screen; Enter on an
    ///   empty box or Esc drops the box.
    pub(crate) fn schematic_text_box(&mut self, ctx: &egui::Context) {
        let Some((at, mut words)) = self.schematic_text.take() else {
            return;
        };
        if self.tab != Tab::Schematic {
            return;
        }
        let pos = self.schematic.screen(at, self.canvas_rect);
        let mut done: Option<bool> = None;
        egui::Area::new(egui::Id::new("schematic_text_box"))
            .fixed_pos(pos)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                let response = ui.add(
                    egui::TextEdit::singleline(&mut words)
                        .hint_text(t!("keys.schematic_text.hint"))
                        .desired_width(220.0),
                );
                response.request_focus();
                if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    done = Some(true);
                }
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    done = Some(false);
                }
            });
        match done {
            Some(true) => {
                let text = words.trim().to_owned();
                if !text.is_empty() {
                    self.run(
                        "graphic.add",
                        json!({"layer": "schematic", "shape": "text", "points": [at], "text": text, "width": NOTE_MM, "sheet": self.schematic_page()}),
                    );
                }
            }
            Some(false) => {}
            None => self.schematic_text = Some((at, words)),
        }
    }
}
