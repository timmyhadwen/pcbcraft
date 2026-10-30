//! Keyboard shortcut table: one source for dispatch, Help, menus and Settings.
//!
//! Two kinds of binding share the table. A *chord* is one key with optional
//! Ctrl, Shift and Alt (Ctrl+Z, Space, Shift+S). A *sequence* is two plain
//! letters typed one after the other, the classic menu-accelerator style
//! (P then W arms the wire tool). The first letter of a sequence is a
//! *prefix*: pressing it only shows a pending hint in the status bar until the
//! second key arrives, and Esc or any key that ends no sequence cancels it.
//!
//! # Rules
//! - The factory set is the classic keymap ([`Keymap::default_bindings`]);
//!   bump [`CLASSIC_SCHEME`] whenever the factory set changes so a stored
//!   Default profile from an older build is replaced on load.
//! - A plain key that starts a sequence in a context is never also a plain
//!   chord in an overlapping context; `classic_prefixes_never_shadow_a_chord`
//!   enforces it, and [`Keymap::set_binding`] refuses such a rebind.
//! - Action ids are persisted in preferences and profile files: never rename
//!   a [`KeyAction`] variant, only add.
//! - Pure data and lookups: no `App`, no painting. egui is used for `Key` and
//!   `Modifiers` only.
//!
//! # Dependencies
//! - `crates/ui-egui/src/lib.rs` `App::shortcuts` feeds keys through
//!   [`Keymap::press`], `App::dispatch_shortcut` runs the actions, and
//!   `Preferences::keymap_profiles` stores [`Keymap`].
//! - `crates/ui-egui/src/e2e/keymap.rs` drives the bindings headlessly.
//!
//! # Do not
//! - Bind a sequence to a key that egui reports only as text (`*`); list such
//!   keys in [`TEXT_KEYS`] instead.
//! - Invent an action here for a feature the app does not have; the action
//!   must map to an existing command or view change.

use crate::Tab;
use egui::Key;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Version of the factory keymap; see the `# Rules` above.
pub const CLASSIC_SCHEME: u32 = 1;

/// Stable action id stored in profiles and used for dispatch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyAction {
    Undo,
    Redo,
    Save,
    CommandPalette,
    ToggleShortcutsHelp,
    SymbolPalette,
    WireTool,
    RouteTool,
    MoveTool,
    Rotate,
    OpenInspector,
    DeleteSelection,
    ReturnToSelect,
    ZoomFitDesign,
    ZoomToSelection,
    ViaTool,
    FlipHorizontal,
    MirrorVertical,
    SelectConnected,
    ToggleNetHighlight,
    LayerNext,
    LayerPrev,
    RepeatLastCommand,
    FindInTree,
    PaletteLineUp,
    PaletteLineDown,
    PaletteRun,
    ToggleAutoSync,
    UpdateBoardFromSchematic,
    ToggleAssistant,
    ZoomIn,
    ZoomOut,
    Redraw,
    CycleGrid,
    ToggleUnits,
    Measure,
    PlaceNetLabel,
    PlacePowerPort,
    PlaceJunction,
    PlaceFootprint,
    TextTool,
    CopperLayerNext,
    SingleLayerMode,
    LayerPanel,
    CycleCornerMode,
    CycleRouteMode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct KeyChord {
    pub key: Key,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) struct KeyChordStore {
    key: String,
    ctrl: bool,
    shift: bool,
    alt: bool,
}

impl KeyChord {
    const fn plain(key: Key) -> Self {
        Self {
            key,
            ctrl: false,
            shift: false,
            alt: false,
        }
    }
    const fn ctrl(key: Key) -> Self {
        Self {
            key,
            ctrl: true,
            shift: false,
            alt: false,
        }
    }
    const fn shift(key: Key) -> Self {
        Self {
            key,
            ctrl: false,
            shift: true,
            alt: false,
        }
    }
    const fn ctrl_shift(key: Key) -> Self {
        Self {
            key,
            ctrl: true,
            shift: true,
            alt: false,
        }
    }
    /// True for a key with no Ctrl, Shift or Alt: the only kind a sequence uses.
    fn is_plain(self) -> bool {
        !self.ctrl && !self.shift && !self.alt
    }

    fn store(self) -> KeyChordStore {
        KeyChordStore {
            key: storage_key_name(self.key),
            ctrl: self.ctrl,
            shift: self.shift,
            alt: self.alt,
        }
    }

    fn load(raw: KeyChordStore) -> Result<Self, String> {
        let key = parse_storage_key(&raw.key).ok_or_else(|| format!("unknown key {}", raw.key))?;
        Ok(Self {
            key,
            ctrl: raw.ctrl,
            shift: raw.shift,
            alt: raw.alt,
        })
    }
}

/// Two plain keys typed one after the other, like P then W.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct KeySequence {
    pub first: Key,
    pub second: Key,
}

impl KeySequence {
    fn store(self) -> [String; 2] {
        [storage_key_name(self.first), storage_key_name(self.second)]
    }
    fn load(raw: &[String; 2]) -> Result<Self, String> {
        let key = |name: &str| parse_storage_key(name).ok_or_else(|| format!("unknown key {name}"));
        Ok(Self {
            first: key(&raw[0])?,
            second: key(&raw[1])?,
        })
    }
}

fn storage_key_name(key: Key) -> String {
    if key == Key::Slash {
        "slash".into()
    } else {
        key.name().to_string()
    }
}

fn parse_storage_key(name: &str) -> Option<Key> {
    if name == "slash" || name == "?" {
        return Some(Key::Slash);
    }
    Key::ALL.iter().find(|k| k.name() == name).copied()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ShortcutContext {
    Anywhere,
    Schematic,
    Pcb,
    CommandPalette,
}

struct ActionSpec {
    action: KeyAction,
    /// Short menu-style name, also shown in the pending-prefix hint.
    name: &'static str,
    label: &'static str,
    tooltip: &'static str,
    context: ShortcutContext,
    rebindable: bool,
    /// Factory chords; the first is the primary one shown in menus.
    chords: &'static [KeyChord],
    /// Factory two-key sequences, as (first, second).
    sequences: &'static [(Key, Key)],
}

const SPECS: &[ActionSpec] = &[
    ActionSpec {
        action: KeyAction::Undo,
        name: "Undo",
        label: "Undo the last action",
        tooltip: "Step back one edit in the design.",
        context: ShortcutContext::Anywhere,
        rebindable: true,
        chords: &[KeyChord::ctrl(Key::Z)],
        sequences: &[],
    },
    ActionSpec {
        action: KeyAction::Redo,
        name: "Redo",
        label: "Redo an undone action",
        tooltip: "Replay the last edit you stepped back from.",
        context: ShortcutContext::Anywhere,
        rebindable: true,
        chords: &[KeyChord::ctrl(Key::Y), KeyChord::ctrl_shift(Key::Z)],
        sequences: &[],
    },
    ActionSpec {
        action: KeyAction::Save,
        name: "Save",
        label: "Save the design to its file",
        tooltip: "Write the open project to disk at its current path.",
        context: ShortcutContext::Anywhere,
        rebindable: true,
        chords: &[KeyChord::ctrl(Key::S)],
        sequences: &[],
    },
    ActionSpec {
        action: KeyAction::CommandPalette,
        name: "Command palette",
        label: "Open the command palette",
        tooltip: "Search every command by name and run it with parameters.",
        context: ShortcutContext::Anywhere,
        rebindable: true,
        chords: &[KeyChord::ctrl(Key::K)],
        sequences: &[],
    },
    ActionSpec {
        action: KeyAction::ToggleShortcutsHelp,
        name: "Shortcut list",
        label: "Show or hide this shortcut list",
        tooltip: "Open or close the keyboard shortcut overlay for the active keymap.",
        context: ShortcutContext::Anywhere,
        rebindable: true,
        chords: &[KeyChord::plain(Key::F1), KeyChord::plain(Key::Slash)],
        sequences: &[],
    },
    ActionSpec {
        action: KeyAction::SymbolPalette,
        name: "Part",
        label: "Place a part from the symbol palette",
        tooltip: "Open the symbol palette to pick a part to place on the schematic.",
        context: ShortcutContext::Schematic,
        rebindable: true,
        chords: &[],
        sequences: &[(Key::P, Key::P)],
    },
    ActionSpec {
        action: KeyAction::WireTool,
        name: "Wire",
        label: "Wire tool: click two pins to connect them",
        tooltip: "Arm the wire tool on the schematic canvas.",
        context: ShortcutContext::Schematic,
        rebindable: true,
        chords: &[KeyChord::ctrl(Key::W)],
        sequences: &[(Key::P, Key::W)],
    },
    ActionSpec {
        action: KeyAction::PlaceNetLabel,
        name: "Net label",
        label: "Place a net label at the cursor",
        tooltip: "Label the net of the wire or pin under the cursor (or the selection).",
        context: ShortcutContext::Schematic,
        rebindable: true,
        chords: &[],
        sequences: &[(Key::P, Key::N)],
    },
    ActionSpec {
        action: KeyAction::PlacePowerPort,
        name: "Power port",
        label: "Place a power port at the cursor",
        tooltip: "Add a power port for the net under the cursor, or the page's power net.",
        context: ShortcutContext::Schematic,
        rebindable: true,
        chords: &[],
        sequences: &[(Key::P, Key::O)],
    },
    ActionSpec {
        action: KeyAction::PlaceJunction,
        name: "Junction",
        label: "Place a junction on the wire under the cursor",
        tooltip: "Drop a junction dot where the cursor sits on a wire.",
        context: ShortcutContext::Schematic,
        rebindable: true,
        chords: &[],
        sequences: &[(Key::P, Key::J)],
    },
    ActionSpec {
        action: KeyAction::RouteTool,
        name: "Interactive route",
        label: "Route tool: click two pads to start a track",
        tooltip: "Arm the route tool on the board canvas.",
        context: ShortcutContext::Pcb,
        rebindable: true,
        chords: &[KeyChord::ctrl(Key::W)],
        sequences: &[(Key::P, Key::T)],
    },
    ActionSpec {
        action: KeyAction::ViaTool,
        name: "Via",
        label: "Via tool: click to place a via on the board",
        tooltip: "Place a via at the cursor on the active copper layer.",
        context: ShortcutContext::Pcb,
        rebindable: true,
        chords: &[],
        sequences: &[(Key::P, Key::V)],
    },
    ActionSpec {
        action: KeyAction::PlaceFootprint,
        name: "Footprint",
        label: "Browse the library to place a footprint",
        tooltip: "Open the Library panel; drag an entry onto the board to place it.",
        context: ShortcutContext::Pcb,
        rebindable: true,
        chords: &[],
        sequences: &[(Key::P, Key::P)],
    },
    ActionSpec {
        action: KeyAction::TextTool,
        name: "String",
        label: "Text tool: click to place board text",
        tooltip: "Arm the text tool on the active drawing layer.",
        context: ShortcutContext::Pcb,
        rebindable: true,
        chords: &[],
        sequences: &[(Key::P, Key::S)],
    },
    ActionSpec {
        action: KeyAction::MoveTool,
        name: "Move",
        label: "Move tool: drop the selection at the cursor",
        tooltip: "Pick up the selection and place it with the next click.",
        context: ShortcutContext::Anywhere,
        rebindable: true,
        chords: &[],
        sequences: &[(Key::M, Key::M)],
    },
    ActionSpec {
        action: KeyAction::Rotate,
        name: "Rotate",
        label: "Rotate the hovered or selected object by 90 degrees",
        tooltip: "Turn the selection a quarter turn, also while dragging it.",
        context: ShortcutContext::Anywhere,
        rebindable: true,
        chords: &[KeyChord::plain(Key::Space)],
        sequences: &[],
    },
    ActionSpec {
        action: KeyAction::FlipHorizontal,
        name: "Mirror X",
        label: "Flip the selection horizontally",
        tooltip: "Mirror the selected footprint or symbol left to right.",
        context: ShortcutContext::Anywhere,
        rebindable: true,
        chords: &[KeyChord::plain(Key::X)],
        sequences: &[],
    },
    ActionSpec {
        action: KeyAction::MirrorVertical,
        name: "Mirror Y",
        label: "Mirror the selection vertically",
        tooltip: "Mirror the selected footprint or symbol top to bottom.",
        context: ShortcutContext::Anywhere,
        rebindable: true,
        chords: &[KeyChord::plain(Key::Y)],
        sequences: &[],
    },
    ActionSpec {
        action: KeyAction::OpenInspector,
        name: "Properties",
        label: "Edit the properties of the hovered or selected object",
        tooltip: "Load the object under the cursor into the Properties panel.",
        context: ShortcutContext::Anywhere,
        rebindable: true,
        chords: &[KeyChord::plain(Key::Tab)],
        sequences: &[],
    },
    ActionSpec {
        action: KeyAction::DeleteSelection,
        name: "Delete",
        label: "Delete the selected objects",
        tooltip: "Remove every selected object from the design.",
        context: ShortcutContext::Anywhere,
        rebindable: true,
        chords: &[KeyChord::plain(Key::Delete)],
        sequences: &[(Key::E, Key::D)],
    },
    ActionSpec {
        action: KeyAction::ReturnToSelect,
        name: "Cancel",
        label: "Cancel the active tool and close open palettes",
        tooltip: "Back out of the current command to the select tool.",
        context: ShortcutContext::Anywhere,
        rebindable: true,
        chords: &[KeyChord::plain(Key::Escape)],
        sequences: &[],
    },
    ActionSpec {
        action: KeyAction::ZoomFitDesign,
        name: "Fit document",
        label: "Zoom the view so the whole design fits",
        tooltip: "Frame the entire schematic or board in the work area.",
        context: ShortcutContext::Anywhere,
        rebindable: true,
        chords: &[KeyChord::ctrl(Key::PageDown)],
        sequences: &[(Key::V, Key::F), (Key::V, Key::D)],
    },
    ActionSpec {
        action: KeyAction::ZoomToSelection,
        name: "Fit selected",
        label: "Zoom the view to the current selection",
        tooltip: "Frame just the selected symbol, footprint, track, or via.",
        context: ShortcutContext::Anywhere,
        rebindable: true,
        chords: &[],
        sequences: &[(Key::V, Key::E)],
    },
    ActionSpec {
        action: KeyAction::ZoomIn,
        name: "Zoom in",
        label: "Zoom in about the cursor",
        tooltip: "Magnify the view one step around the mouse position.",
        context: ShortcutContext::Anywhere,
        rebindable: true,
        chords: &[KeyChord::plain(Key::PageUp)],
        sequences: &[],
    },
    ActionSpec {
        action: KeyAction::ZoomOut,
        name: "Zoom out",
        label: "Zoom out about the cursor",
        tooltip: "Shrink the view one step around the mouse position.",
        context: ShortcutContext::Anywhere,
        rebindable: true,
        chords: &[KeyChord::plain(Key::PageDown)],
        sequences: &[],
    },
    ActionSpec {
        action: KeyAction::Redraw,
        name: "Refresh",
        label: "Redraw the view",
        tooltip: "Repaint the canvas now.",
        context: ShortcutContext::Anywhere,
        rebindable: true,
        chords: &[KeyChord::plain(Key::End)],
        sequences: &[],
    },
    ActionSpec {
        action: KeyAction::CycleGrid,
        name: "Snap grid",
        label: "Cycle the snap grid to the next step",
        tooltip: "Step the placement snap through 0.1, 0.25, 0.5, 1, 1.27 and 2.54 mm.",
        context: ShortcutContext::Anywhere,
        rebindable: true,
        chords: &[KeyChord::plain(Key::G)],
        sequences: &[],
    },
    ActionSpec {
        action: KeyAction::ToggleUnits,
        name: "Toggle units",
        label: "Toggle the cursor readout between mm and mil",
        tooltip: "Switch the status bar coordinates and grid between millimetres and mils.",
        context: ShortcutContext::Anywhere,
        rebindable: true,
        chords: &[KeyChord::plain(Key::Q)],
        sequences: &[],
    },
    ActionSpec {
        action: KeyAction::Measure,
        name: "Measure distance",
        label: "Measure the distance between two points",
        tooltip: "Arm the measure tool; click two points, Esc clears.",
        context: ShortcutContext::Anywhere,
        rebindable: true,
        chords: &[KeyChord::ctrl(Key::M)],
        sequences: &[],
    },
    ActionSpec {
        action: KeyAction::SelectConnected,
        name: "Select connected",
        label: "Step through copper on the same connection",
        tooltip: "Cycle the selection among tracks and vias on one net.",
        context: ShortcutContext::Pcb,
        rebindable: true,
        chords: &[KeyChord::plain(Key::C)],
        sequences: &[],
    },
    ActionSpec {
        action: KeyAction::ToggleNetHighlight,
        name: "Highlight net",
        label: "Toggle spotlight on the connection of the selection",
        tooltip: "Highlight or clear the net tied to the current selection.",
        context: ShortcutContext::Anywhere,
        rebindable: true,
        chords: &[KeyChord::plain(Key::N)],
        sequences: &[],
    },
    ActionSpec {
        action: KeyAction::LayerNext,
        name: "Next layer",
        label: "Make the next layer in the stack active",
        tooltip: "Step the active layer one level away from the viewer.",
        context: ShortcutContext::Pcb,
        rebindable: true,
        chords: &[
            KeyChord::plain(Key::Plus),
            KeyChord::plain(Key::Equals),
            KeyChord::plain(Key::CloseBracket),
        ],
        sequences: &[],
    },
    ActionSpec {
        action: KeyAction::LayerPrev,
        name: "Previous layer",
        label: "Make the previous layer in the stack active",
        tooltip: "Step the active layer one level toward the viewer.",
        context: ShortcutContext::Pcb,
        rebindable: true,
        chords: &[
            KeyChord::plain(Key::Minus),
            KeyChord::plain(Key::OpenBracket),
        ],
        sequences: &[],
    },
    ActionSpec {
        action: KeyAction::CopperLayerNext,
        name: "Next copper layer",
        label: "Route on the next copper layer",
        tooltip: "Step the active layer to the next copper layer, skipping the others.",
        context: ShortcutContext::Pcb,
        rebindable: false,
        chords: &[],
        sequences: &[],
    },
    ActionSpec {
        action: KeyAction::SingleLayerMode,
        name: "Single layer mode",
        label: "Show only the active layer, or everything again",
        tooltip: "Hide copper on other layers; works for the outer copper layers and drawing layers.",
        context: ShortcutContext::Pcb,
        rebindable: true,
        chords: &[KeyChord::shift(Key::S)],
        sequences: &[],
    },
    ActionSpec {
        action: KeyAction::LayerPanel,
        name: "Layers panel",
        label: "Show the Layers panel",
        tooltip: "Open the layer and view configuration in the left dock.",
        context: ShortcutContext::Pcb,
        rebindable: true,
        chords: &[KeyChord::plain(Key::L)],
        sequences: &[],
    },
    ActionSpec {
        action: KeyAction::CycleCornerMode,
        name: "Corner mode",
        label: "Switch routed corners between 45 and 90 degrees",
        tooltip: "Toggle the corner style new tracks use.",
        context: ShortcutContext::Pcb,
        rebindable: true,
        chords: &[KeyChord::shift(Key::Space)],
        sequences: &[],
    },
    ActionSpec {
        action: KeyAction::CycleRouteMode,
        name: "Routing conflict mode",
        label: "Switch routing between push and stop at obstacles",
        tooltip: "Toggle whether a new route pushes other nets' tracks aside; push needs 45 degree corners.",
        context: ShortcutContext::Pcb,
        rebindable: true,
        chords: &[KeyChord::shift(Key::R)],
        sequences: &[],
    },
    ActionSpec {
        action: KeyAction::ToggleAutoSync,
        name: "Automatic board sync",
        label: "Turn automatic board sync on or off",
        tooltip: "Toggle whether schematic edits update the board in the same undo step.",
        context: ShortcutContext::Anywhere,
        rebindable: true,
        chords: &[KeyChord::ctrl_shift(Key::B)],
        sequences: &[],
    },
    ActionSpec {
        action: KeyAction::UpdateBoardFromSchematic,
        name: "Update board",
        label: "Update the board from the schematic",
        tooltip: "Apply pending schematic changes to the board when automatic sync is off.",
        context: ShortcutContext::Anywhere,
        rebindable: true,
        chords: &[KeyChord::ctrl(Key::B)],
        sequences: &[],
    },
    ActionSpec {
        action: KeyAction::ToggleAssistant,
        name: "Assistant",
        label: "Show or hide the Assistant tab",
        tooltip: "Open the Assistant in the lower-right aside, or return to Properties.",
        context: ShortcutContext::Anywhere,
        rebindable: true,
        chords: &[KeyChord::ctrl_shift(Key::A)],
        sequences: &[],
    },
    ActionSpec {
        action: KeyAction::RepeatLastCommand,
        name: "Repeat last",
        label: "Repeat the last command",
        tooltip: "Run the most recent successful edit command again.",
        context: ShortcutContext::Anywhere,
        rebindable: true,
        chords: &[KeyChord::plain(Key::Period)],
        sequences: &[],
    },
    ActionSpec {
        action: KeyAction::FindInTree,
        name: "Jump to component",
        label: "Find a component or net by name",
        tooltip: "Jump to the project tree search box to find a part or net.",
        context: ShortcutContext::Anywhere,
        rebindable: true,
        chords: &[KeyChord::ctrl(Key::F)],
        sequences: &[(Key::J, Key::C)],
    },
    ActionSpec {
        action: KeyAction::PaletteLineUp,
        name: "Up",
        label: "Move the highlight up in the command list",
        tooltip: "Step the command palette selection toward the top.",
        context: ShortcutContext::CommandPalette,
        rebindable: true,
        chords: &[KeyChord::plain(Key::ArrowUp)],
        sequences: &[],
    },
    ActionSpec {
        action: KeyAction::PaletteLineDown,
        name: "Down",
        label: "Move the highlight down in the command list",
        tooltip: "Step the command palette selection toward the bottom.",
        context: ShortcutContext::CommandPalette,
        rebindable: true,
        chords: &[KeyChord::plain(Key::ArrowDown)],
        sequences: &[],
    },
    ActionSpec {
        action: KeyAction::PaletteRun,
        name: "Load command",
        label: "Load the highlighted command into the parameter box",
        tooltip: "Copy the selected palette command so you can edit and run it.",
        context: ShortcutContext::CommandPalette,
        rebindable: true,
        chords: &[KeyChord::plain(Key::Enter)],
        sequences: &[],
    },
];

/// Keys egui reports only as typed text, with the action each runs.
pub const TEXT_KEYS: &[(&str, KeyAction)] = &[("*", KeyAction::CopperLayerNext)];

/// Fixed pointer gestures and system clipboard keys, listed in Help.
const GESTURE_ROWS: &[(&str, &str, &str)] = &[
    ("Ctrl+C", "Copy the selection", "canvas"),
    ("Ctrl+X", "Cut the selection", "canvas"),
    ("Ctrl+V", "Paste the last copy", "canvas"),
    ("Backspace", "Take back the last corner of a route", "board"),
    ("Middle-drag", "Pan the view", "canvas"),
    ("Right-drag", "Pan the view", "canvas"),
    ("Scroll", "Zoom the view about the cursor", "canvas"),
];

fn spec(action: KeyAction) -> Option<&'static ActionSpec> {
    SPECS.iter().find(|s| s.action == action)
}

pub fn action_label(action: KeyAction) -> &'static str {
    spec(action).map(|s| s.label).unwrap_or("Unknown action")
}

/// The short menu-style name of an action, like "Wire" or "Fit document".
pub fn action_name(action: KeyAction) -> &'static str {
    spec(action).map(|s| s.name).unwrap_or("Unknown")
}

pub fn action_tooltip(action: KeyAction) -> &'static str {
    spec(action)
        .map(|s| s.tooltip)
        .unwrap_or("No description yet.")
}

/// Every action the Settings page lists, in table order.
pub fn rebindable_actions() -> impl Iterator<Item = KeyAction> {
    SPECS.iter().filter(|s| s.rebindable).map(|s| s.action)
}

fn context_name(context: ShortcutContext) -> &'static str {
    match context {
        ShortcutContext::Anywhere => "anywhere",
        ShortcutContext::Schematic => "schematic tab",
        ShortcutContext::Pcb => "pcb tab",
        ShortcutContext::CommandPalette => "command palette",
    }
}

pub fn format_chord(chord: KeyChord) -> String {
    let mut parts: Vec<&str> = Vec::new();
    if chord.ctrl {
        parts.push("Ctrl");
    }
    if chord.shift {
        parts.push("Shift");
    }
    if chord.alt {
        parts.push("Alt");
    }
    parts.push(chord_key_name(chord.key, chord.shift));
    parts.join("+")
}

/// "P, W": how Help, menus and Settings spell a sequence.
pub fn format_sequence(sequence: KeySequence) -> String {
    format!(
        "{}, {}",
        chord_key_name(sequence.first, false),
        chord_key_name(sequence.second, false)
    )
}

/// The name of one key on its own, as the pending-prefix hint shows it.
pub fn key_name(key: Key) -> &'static str {
    chord_key_name(key, false)
}

fn chord_key_name(key: Key, shift: bool) -> &'static str {
    match key {
        Key::Slash if !shift => "?",
        Key::ArrowUp => "Up",
        Key::ArrowDown => "Down",
        Key::OpenBracket => "[",
        Key::CloseBracket => "]",
        Key::Period => ".",
        Key::Plus => "+",
        Key::Minus => "-",
        Key::Equals => "=",
        Key::Home => "Home",
        Key::End => "End",
        Key::PageUp => "PgUp",
        Key::PageDown => "PgDn",
        Key::Delete => "Delete",
        Key::Escape => "Esc",
        Key::Enter => "Enter",
        Key::Space => "Space",
        Key::Tab => "Tab",
        other => other.name(),
    }
}

/// What one key press did to the keymap's sequence state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Press {
    /// The key started a sequence; it is now the pending prefix.
    Prefix(Key),
    /// The key finished a sequence or matched a chord: run this action.
    Run(KeyAction),
    /// A prefix was pending and this key ended no sequence (or was Esc).
    Cancelled,
    /// The keymap has nothing for this key; leave it to other handlers.
    Pass,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Keymap {
    bindings: HashMap<KeyAction, KeyChord>,
    extra_chords: HashMap<KeyAction, Vec<KeyChord>>,
    sequences: HashMap<KeyAction, Vec<KeySequence>>,
    /// The factory scheme this map was built from; see [`CLASSIC_SCHEME`].
    scheme: u32,
}

#[derive(Serialize, Deserialize)]
struct KeymapStore {
    bindings: HashMap<KeyAction, KeyChordStore>,
    #[serde(default)]
    extra_chords: HashMap<KeyAction, Vec<KeyChordStore>>,
    #[serde(default)]
    sequences: HashMap<KeyAction, Vec<[String; 2]>>,
    #[serde(default)]
    scheme: u32,
}

fn store_sequences(
    sequences: &HashMap<KeyAction, Vec<KeySequence>>,
) -> HashMap<KeyAction, Vec<[String; 2]>> {
    sequences
        .iter()
        .map(|(a, list)| (*a, list.iter().map(|s| s.store()).collect()))
        .collect()
}

fn load_sequences(
    raw: HashMap<KeyAction, Vec<[String; 2]>>,
) -> Result<HashMap<KeyAction, Vec<KeySequence>>, String> {
    let mut out = HashMap::new();
    for (action, list) in raw {
        let mut parsed = Vec::new();
        for pair in &list {
            parsed.push(KeySequence::load(pair)?);
        }
        out.insert(action, parsed);
    }
    Ok(out)
}

impl Serialize for Keymap {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let store = KeymapStore {
            bindings: self.bindings.iter().map(|(a, c)| (*a, c.store())).collect(),
            extra_chords: self
                .extra_chords
                .iter()
                .map(|(a, list)| (*a, list.iter().map(|c| c.store()).collect::<Vec<_>>()))
                .collect(),
            sequences: store_sequences(&self.sequences),
            scheme: self.scheme,
        };
        store.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Keymap {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let store = KeymapStore::deserialize(deserializer)?;
        let mut bindings = HashMap::new();
        for (action, raw) in store.bindings {
            let chord = KeyChord::load(raw).map_err(serde::de::Error::custom)?;
            bindings.insert(action, chord);
        }
        let mut extra_chords = HashMap::new();
        for (action, list) in store.extra_chords {
            let mut parsed = Vec::new();
            for raw in list {
                parsed.push(KeyChord::load(raw).map_err(serde::de::Error::custom)?);
            }
            extra_chords.insert(action, parsed);
        }
        let sequences = load_sequences(store.sequences).map_err(serde::de::Error::custom)?;
        Ok(Self {
            bindings,
            extra_chords,
            sequences,
            scheme: store.scheme,
        })
    }
}

impl Default for Keymap {
    fn default() -> Self {
        Self::default_bindings()
    }
}

impl Keymap {
    /// The classic factory keymap: chords plus two-key menu sequences.
    pub fn default_bindings() -> Self {
        let mut bindings: HashMap<KeyAction, KeyChord> = HashMap::new();
        let mut extra_chords: HashMap<KeyAction, Vec<KeyChord>> = HashMap::new();
        let mut sequences: HashMap<KeyAction, Vec<KeySequence>> = HashMap::new();
        for entry in SPECS {
            let mut chords = entry.chords.iter().copied();
            if let Some(first) = chords.next() {
                bindings.insert(entry.action, first);
            }
            let rest: Vec<KeyChord> = chords.collect();
            if !rest.is_empty() {
                extra_chords.insert(entry.action, rest);
            }
            if !entry.sequences.is_empty() {
                sequences.insert(
                    entry.action,
                    entry
                        .sequences
                        .iter()
                        .map(|(first, second)| KeySequence {
                            first: *first,
                            second: *second,
                        })
                        .collect(),
                );
            }
        }
        Self {
            bindings,
            extra_chords,
            sequences,
            scheme: CLASSIC_SCHEME,
        }
    }

    /// True when this map predates the current factory scheme.
    pub fn is_outdated(&self) -> bool {
        self.scheme < CLASSIC_SCHEME
    }

    pub fn chord_for(&self, action: KeyAction) -> Option<KeyChord> {
        self.bindings.get(&action).copied()
    }

    pub fn export_bindings(&self) -> HashMap<KeyAction, KeyChord> {
        self.bindings.clone()
    }

    pub fn chords_for(&self, action: KeyAction) -> Vec<KeyChord> {
        let mut out = Vec::new();
        if let Some(primary) = self.bindings.get(&action) {
            out.push(*primary);
        }
        if let Some(extra) = self.extra_chords.get(&action) {
            out.extend(extra.iter().copied());
        }
        out
    }

    pub fn sequences_for(&self, action: KeyAction) -> Vec<KeySequence> {
        self.sequences.get(&action).cloned().unwrap_or_default()
    }

    /// Every way to trigger `action`, sequences first: "P, W  or  Ctrl+W".
    pub fn keys_text(&self, action: KeyAction) -> String {
        let mut parts: Vec<String> = self
            .sequences_for(action)
            .into_iter()
            .map(format_sequence)
            .collect();
        parts.extend(self.chords_for(action).into_iter().map(format_chord));
        if let Some((text, _)) = TEXT_KEYS.iter().find(|(_, a)| *a == action) {
            parts.push((*text).to_string());
        }
        parts.join("  or  ")
    }

    /// The one key text a menu shows on the right of an item, if any.
    pub fn menu_keys(&self, action: KeyAction) -> Option<String> {
        if let Some(sequence) = self.sequences_for(action).first() {
            return Some(format_sequence(*sequence));
        }
        self.chord_for(action).map(format_chord)
    }

    pub fn set_binding(&mut self, action: KeyAction, chord: KeyChord) -> Result<(), String> {
        if spec(action).is_none_or(|s| !s.rebindable) {
            return Err("that action cannot be rebound".into());
        }
        let scope = action_context(action);
        if let Some(owner) = self.owner_in_scope(chord, scope, None) {
            return Err(format!(
                "{} already uses {}",
                action_label(owner),
                format_chord(chord)
            ));
        }
        if chord.is_plain()
            && let Some(owner) = self.prefix_owner(chord.key, scope)
        {
            return Err(format!(
                "{} starts the {} shortcuts",
                key_name(chord.key),
                action_name(owner)
            ));
        }
        self.bindings.insert(action, chord);
        self.extra_chords.remove(&action);
        Ok(())
    }

    fn owner_in_scope(
        &self,
        chord: KeyChord,
        scope: ShortcutContext,
        skip: Option<KeyAction>,
    ) -> Option<KeyAction> {
        for (action, bound) in &self.bindings {
            if skip == Some(*action) {
                continue;
            }
            if *bound != chord {
                continue;
            }
            if contexts_overlap(action_context(*action), scope) {
                return Some(*action);
            }
        }
        for (action, extras) in &self.extra_chords {
            if skip == Some(*action) {
                continue;
            }
            for bound in extras {
                if *bound == chord && contexts_overlap(action_context(*action), scope) {
                    return Some(*action);
                }
            }
        }
        None
    }

    /// An action whose sequence starts with `key` in a context overlapping `scope`.
    fn prefix_owner(&self, key: Key, scope: ShortcutContext) -> Option<KeyAction> {
        self.sequences.iter().find_map(|(action, list)| {
            (list.iter().any(|s| s.first == key)
                && contexts_overlap(action_context(*action), scope))
            .then_some(*action)
        })
    }

    pub fn resolve(&self, chord: KeyChord, tab: Tab, command_open: bool) -> Option<KeyAction> {
        for (action, bound) in &self.bindings {
            if *bound != chord {
                continue;
            }
            if context_matches(action_context(*action), tab, command_open) {
                return Some(*action);
            }
        }
        for (action, extras) in &self.extra_chords {
            for bound in extras {
                if *bound == chord && context_matches(action_context(*action), tab, command_open) {
                    return Some(*action);
                }
            }
        }
        None
    }

    /// The second keys that finish a sequence starting with `first` here,
    /// with their actions, in table order.
    pub fn completions(&self, first: Key, tab: Tab, command_open: bool) -> Vec<(Key, KeyAction)> {
        let mut out = Vec::new();
        for entry in SPECS {
            if !context_matches(entry.context, tab, command_open) {
                continue;
            }
            for sequence in self.sequences_for(entry.action) {
                if sequence.first == first {
                    out.push((sequence.second, entry.action));
                }
            }
        }
        out
    }

    /// Feed one key press through the sequence state.
    ///
    /// # Rules
    /// - With a prefix pending, every key is consumed: it finishes the
    ///   sequence or cancels it (Esc cancels without running Cancel).
    /// - Without one, a plain key that starts a sequence here becomes the
    ///   prefix; anything else resolves as a chord.
    pub fn press(
        &self,
        pending: &mut Option<Key>,
        chord: KeyChord,
        tab: Tab,
        command_open: bool,
    ) -> Press {
        if let Some(first) = pending.take() {
            if chord.ctrl || chord.alt || chord.key == Key::Escape {
                return Press::Cancelled;
            }
            return self
                .completions(first, tab, command_open)
                .into_iter()
                .find(|(second, _)| *second == chord.key)
                .map_or(Press::Cancelled, |(_, action)| Press::Run(action));
        }
        if chord.is_plain() && !self.completions(chord.key, tab, command_open).is_empty() {
            *pending = Some(chord.key);
            return Press::Prefix(chord.key);
        }
        self.resolve(chord, tab, command_open)
            .map_or(Press::Pass, Press::Run)
    }

    /// Help rows: keys, what they do and where, sequences before chords.
    pub fn rows_for_help(&self) -> Vec<(String, String, String)> {
        let mut rows = Vec::new();
        for spec in SPECS {
            let keys = self.keys_text(spec.action);
            if keys.is_empty() {
                continue;
            }
            rows.push((
                keys,
                spec.label.to_string(),
                context_name(spec.context).to_string(),
            ));
        }
        for (keys, action, context) in GESTURE_ROWS {
            rows.push((keys.to_string(), action.to_string(), context.to_string()));
        }
        rows
    }

    pub fn sanitize(&mut self) {
        let known: HashSet<KeyAction> = SPECS.iter().map(|s| s.action).collect();
        self.bindings.retain(|action, _| known.contains(action));
        self.extra_chords.retain(|action, _| known.contains(action));
        self.sequences.retain(|action, _| known.contains(action));
    }

    pub fn reset_to_default(&mut self) {
        *self = Self::default_bindings();
    }
}

fn action_context(action: KeyAction) -> ShortcutContext {
    spec(action)
        .map(|s| s.context)
        .unwrap_or(ShortcutContext::Anywhere)
}

/// Whether an action of `action_ctx` may fire on `tab`.
pub fn context_matches(action_ctx: ShortcutContext, tab: Tab, command_open: bool) -> bool {
    match action_ctx {
        ShortcutContext::Anywhere => !command_open,
        ShortcutContext::Schematic => !command_open && tab == Tab::Schematic,
        ShortcutContext::Pcb => !command_open && tab == Tab::Pcb,
        ShortcutContext::CommandPalette => command_open,
    }
}

/// Whether `action` may fire on `tab` right now (menus grey out the rest).
pub fn action_available(action: KeyAction, tab: Tab) -> bool {
    context_matches(action_context(action), tab, false)
}

fn contexts_overlap(a: ShortcutContext, b: ShortcutContext) -> bool {
    if a == b {
        return true;
    }
    if a == ShortcutContext::CommandPalette || b == ShortcutContext::CommandPalette {
        return false;
    }
    a == ShortcutContext::Anywhere || b == ShortcutContext::Anywhere
}

pub fn chord_from_input(key: Key, input: egui::Modifiers) -> KeyChord {
    let egui::Modifiers {
        ctrl,
        shift,
        alt,
        command,
        ..
    } = input;
    KeyChord {
        key,
        ctrl: ctrl || command,
        shift,
        alt,
    }
}

/// Keys that are only modifiers; they never start or finish a sequence.
pub fn is_modifier_key(key: Key) -> bool {
    matches!(
        key,
        Key::ShiftLeft
            | Key::ShiftRight
            | Key::ControlLeft
            | Key::ControlRight
            | Key::AltLeft
            | Key::AltRight
            | Key::SuperLeft
            | Key::SuperRight
    )
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct KeymapProfileFile {
    pub version: u32,
    pub name: String,
    pub bindings: HashMap<KeyAction, KeyChordStore>,
    #[serde(default)]
    pub sequences: HashMap<KeyAction, Vec<[String; 2]>>,
}

impl KeymapProfileFile {
    pub const VERSION: u32 = 1;

    pub fn from_keymap(name: String, map: &Keymap) -> Self {
        Self {
            version: Self::VERSION,
            name,
            bindings: map
                .export_bindings()
                .into_iter()
                .map(|(action, chord)| (action, chord.store()))
                .collect(),
            sequences: store_sequences(&map.sequences),
        }
    }

    pub fn into_keymap(self) -> Result<Keymap, String> {
        if self.version != Self::VERSION {
            return Err(format!("unsupported profile version {}", self.version));
        }
        let mut map = Keymap {
            bindings: HashMap::new(),
            extra_chords: HashMap::new(),
            sequences: load_sequences(self.sequences)?,
            scheme: CLASSIC_SCHEME,
        };
        for (action, stored) in self.bindings {
            map.set_binding(action, KeyChord::load(stored)?)?;
        }
        map.sanitize();
        Ok(map)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn default_bindings_have_unique_chords_per_context() -> Result<(), String> {
        let map = Keymap::default_bindings();
        let mut seen: BTreeMap<(String, bool, bool, bool, ShortcutContext), KeyAction> =
            BTreeMap::new();
        for action in rebindable_actions() {
            for chord in map.chords_for(action) {
                let ctx = action_context(action);
                let key = (
                    storage_key_name(chord.key),
                    chord.ctrl,
                    chord.shift,
                    chord.alt,
                    ctx,
                );
                if let Some(other) = seen.insert(key, action) {
                    return Err(format!(
                        "{other:?} and {action:?} both use {} in {ctx:?}",
                        format_chord(chord)
                    ));
                }
            }
        }
        Ok(())
    }

    #[test]
    fn classic_prefixes_never_shadow_a_chord() -> Result<(), String> {
        let map = Keymap::default_bindings();
        for spec in SPECS {
            for chord in map.chords_for(spec.action) {
                if chord.is_plain()
                    && let Some(owner) = map.prefix_owner(chord.key, spec.context)
                {
                    return Err(format!(
                        "{:?} uses {} which starts {owner:?}",
                        spec.action,
                        format_chord(chord)
                    ));
                }
            }
        }
        Ok(())
    }

    #[test]
    fn no_two_actions_share_a_sequence_in_one_context() -> Result<(), String> {
        let map = Keymap::default_bindings();
        for tab in [Tab::Schematic, Tab::Pcb] {
            let mut seen: HashMap<KeySequence, KeyAction> = HashMap::new();
            for spec in SPECS {
                if !context_matches(spec.context, tab, false) {
                    continue;
                }
                for sequence in map.sequences_for(spec.action) {
                    if let Some(other) = seen.insert(sequence, spec.action) {
                        return Err(format!(
                            "{other:?} and {:?} share {} on {tab:?}",
                            spec.action,
                            format_sequence(sequence)
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    #[test]
    fn p_then_w_arms_the_wire_on_the_schematic_and_p_then_t_routes_on_the_board() {
        let map = Keymap::default_bindings();
        let mut pending = None;
        let p = KeyChord::plain(Key::P);
        assert_eq!(
            map.press(&mut pending, p, Tab::Schematic, false),
            Press::Prefix(Key::P)
        );
        assert_eq!(
            map.press(&mut pending, KeyChord::plain(Key::W), Tab::Schematic, false),
            Press::Run(KeyAction::WireTool)
        );
        assert_eq!(pending, None);
        map.press(&mut pending, p, Tab::Pcb, false);
        assert_eq!(
            map.press(&mut pending, KeyChord::plain(Key::T), Tab::Pcb, false),
            Press::Run(KeyAction::RouteTool)
        );
    }

    #[test]
    fn esc_or_an_unknown_second_key_cancels_a_pending_prefix() {
        let map = Keymap::default_bindings();
        for second in [Key::Escape, Key::Q] {
            let mut pending = None;
            map.press(&mut pending, KeyChord::plain(Key::P), Tab::Schematic, false);
            assert_eq!(
                map.press(&mut pending, KeyChord::plain(second), Tab::Schematic, false),
                Press::Cancelled
            );
            assert_eq!(pending, None);
        }
    }

    #[test]
    fn chords_still_resolve_without_a_prefix() {
        let map = Keymap::default_bindings();
        let mut pending = None;
        assert_eq!(
            map.press(&mut pending, KeyChord::ctrl(Key::Z), Tab::Pcb, false),
            Press::Run(KeyAction::Undo)
        );
        assert_eq!(
            map.press(&mut pending, KeyChord::ctrl(Key::W), Tab::Schematic, false),
            Press::Run(KeyAction::WireTool)
        );
        assert_eq!(
            map.press(&mut pending, KeyChord::plain(Key::F12), Tab::Pcb, false),
            Press::Pass
        );
    }

    #[test]
    fn every_action_has_non_empty_label_and_tooltip() {
        for spec in SPECS {
            assert!(!action_label(spec.action).is_empty());
            assert!(!action_tooltip(spec.action).is_empty());
            assert!(!action_name(spec.action).is_empty());
        }
    }

    #[test]
    fn help_rows_include_sequences_and_non_rebindable_gestures() {
        let rows = Keymap::default_bindings().rows_for_help();
        assert!(rows.iter().any(|(k, _, _)| k == "Middle-drag"));
        assert!(rows.iter().any(|(k, _, _)| k == "Scroll"));
        assert!(rows.iter().any(|(k, _, _)| k.contains("P, W")));
        assert!(rows.iter().any(|(k, _, _)| k == "*"));
    }

    #[test]
    fn keymap_reject_conflicting_rebind() -> Result<(), String> {
        let mut map = Keymap::default_bindings();
        let rotate = map.chord_for(KeyAction::Rotate).ok_or("rotate chord")?;
        let err = match map.set_binding(KeyAction::Redraw, rotate) {
            Err(e) => e,
            Ok(()) => return Err("conflict".into()),
        };
        if !err.contains("Rotate") {
            return Err(err);
        }
        let err = match map.set_binding(KeyAction::Redraw, KeyChord::plain(Key::P)) {
            Err(e) => e,
            Ok(()) => return Err("prefix conflict".into()),
        };
        if !err.contains("starts") {
            return Err(err);
        }
        Ok(())
    }

    #[test]
    fn keymap_import_export_round_trip() -> Result<(), String> {
        let mut map = Keymap::default_bindings();
        let t = KeyChord::plain(Key::T);
        map.set_binding(KeyAction::Rotate, t)?;
        let file = KeymapProfileFile::from_keymap("Custom".into(), &map);
        let json = serde_json::to_string(&file).map_err(|e| e.to_string())?;
        let parsed: KeymapProfileFile = serde_json::from_str(&json).map_err(|e| e.to_string())?;
        let back = parsed.into_keymap()?;
        assert_eq!(back.chord_for(KeyAction::Rotate), Some(t));
        assert_eq!(
            back.sequences_for(KeyAction::WireTool),
            map.sequences_for(KeyAction::WireTool)
        );
        Ok(())
    }

    #[test]
    fn a_stored_keymap_from_before_sequences_reads_as_outdated() -> Result<(), String> {
        let old = r#"{"bindings":{"undo":{"key":"Z","ctrl":true,"shift":false,"alt":false}}}"#;
        let map: Keymap = serde_json::from_str(old).map_err(|e| e.to_string())?;
        assert!(map.is_outdated());
        assert!(!Keymap::default_bindings().is_outdated());
        Ok(())
    }
}
