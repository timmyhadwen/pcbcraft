use super::super::*;
use super::support::frame;
use crate::keymap::{KeyAction, KeyChord};
use egui::Key;
use serde_json::json;

#[test]
fn a_newcomer_rebinds_rotate_to_another_key() -> Result<(), String> {
    let session =
        pcbcraft_mcp::shared(pcbcraft_engine::Session::sample().map_err(|e| e.to_string())?);
    let mut app = App::create(session, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    app.change_tab(Tab::Pcb);
    app.select(Some(11));
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.change_tab(Tab::Settings);
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    super::support::tap_id(&ctx, &mut app, "button:Edit shortcuts…")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.binding_capture = Some(KeyAction::Rotate);
    let t = KeyChord {
        key: Key::T,
        ctrl: false,
        shift: false,
        alt: false,
    };
    app.preferences
        .active_keymap_mut()
        .set_binding(KeyAction::Rotate, t)
        .map_err(|e| e.to_string())?;
    app.binding_capture = None;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let key = |key, modifiers| egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    };
    frame(
        &ctx,
        &mut app,
        vec![key(Key::T, egui::Modifiers::NONE)],
        egui::Modifiers::NONE,
    );
    assert_eq!(
        app.project.footprint(11).map(|f| f.rotation),
        Some(90.0),
        "T should rotate the footprint: {}",
        app.message
    );
    frame(
        &ctx,
        &mut app,
        vec![key(Key::R, egui::Modifiers::NONE)],
        egui::Modifiers::NONE,
    );
    assert_eq!(
        app.project.footprint(11).map(|f| f.rotation),
        Some(90.0),
        "R should no longer rotate after the rebind: {}",
        app.message
    );
    Ok(())
}

#[test]
fn a_newcomer_turns_automatic_board_sync_off_from_the_keyboard() -> Result<(), String> {
    let session = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(session, Preferences::default(), None, None);
    app.refresh();
    assert!(app.project.auto_sync, "new designs start with sync on");
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    frame(
        &ctx,
        &mut app,
        vec![egui::Event::Key {
            key: Key::B,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers {
                ctrl: true,
                shift: true,
                ..Default::default()
            },
        }],
        egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
    );
    app.refresh();
    assert!(
        !app.project.auto_sync,
        "Ctrl+Shift+B should turn automatic sync off: {}",
        app.message
    );
    Ok(())
}

/// A press and release of `key`, as a real keyboard sends it.
fn tap(key: Key, modifiers: egui::Modifiers) -> Vec<egui::Event> {
    [true, false]
        .into_iter()
        .map(|pressed| egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers,
        })
        .collect()
}

fn sample_app(tab: Tab) -> Result<(egui::Context, App), String> {
    let session =
        pcbcraft_mcp::shared(pcbcraft_engine::Session::sample().map_err(|e| e.to_string())?);
    let mut app = App::create(session, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    app.change_tab(tab);
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    Ok((ctx, app))
}

#[test]
fn a_newcomer_types_p_then_w_and_sees_the_prefix_wait_in_the_status_bar() -> Result<(), String> {
    let (ctx, mut app) = sample_app(Tab::Schematic)?;
    frame(
        &ctx,
        &mut app,
        tap(Key::P, egui::Modifiers::NONE),
        egui::Modifiers::NONE,
    );
    assert_eq!(app.key_prefix, Some(Key::P));
    let hint = app.prefix_hint().ok_or("a pending prefix shows a hint")?;
    assert!(hint.contains("W Wire"), "{hint}");
    assert!(
        matches!(app.tool, Tool::Select),
        "P alone does not arm anything"
    );
    frame(
        &ctx,
        &mut app,
        tap(Key::W, egui::Modifiers::NONE),
        egui::Modifiers::NONE,
    );
    assert!(
        matches!(app.tool, Tool::Wire(None)),
        "P, W arms the wire tool"
    );
    assert_eq!(app.key_prefix, None);
    Ok(())
}

#[test]
fn a_newcomer_cancels_a_prefix_with_esc_without_dropping_the_tool() -> Result<(), String> {
    let (ctx, mut app) = sample_app(Tab::Pcb)?;
    let mut keys = tap(Key::P, egui::Modifiers::NONE);
    keys.extend(tap(Key::T, egui::Modifiers::NONE));
    frame(&ctx, &mut app, keys, egui::Modifiers::NONE);
    assert!(
        matches!(app.tool, Tool::Route(None)),
        "P, T routes on the board"
    );
    frame(
        &ctx,
        &mut app,
        tap(Key::P, egui::Modifiers::NONE),
        egui::Modifiers::NONE,
    );
    frame(
        &ctx,
        &mut app,
        tap(Key::Escape, egui::Modifiers::NONE),
        egui::Modifiers::NONE,
    );
    assert_eq!(app.key_prefix, None, "Esc cancels the prefix");
    assert!(
        matches!(app.tool, Tool::Route(None)),
        "the first Esc only cancels the prefix"
    );
    frame(
        &ctx,
        &mut app,
        tap(Key::Escape, egui::Modifiers::NONE),
        egui::Modifiers::NONE,
    );
    assert!(matches!(app.tool, Tool::Select), "the next Esc backs out");
    Ok(())
}

#[test]
fn a_newcomer_zooms_with_page_keys_and_cycles_grid_and_units() -> Result<(), String> {
    let (ctx, mut app) = sample_app(Tab::Pcb)?;
    let before = app.pcb.zoom;
    frame(
        &ctx,
        &mut app,
        tap(Key::PageUp, egui::Modifiers::NONE),
        egui::Modifiers::NONE,
    );
    assert!(app.pcb.zoom > before, "PgUp zooms in");
    frame(
        &ctx,
        &mut app,
        tap(Key::PageDown, egui::Modifiers::NONE),
        egui::Modifiers::NONE,
    );
    assert!(
        (app.pcb.zoom - before).abs() < before * 1e-3,
        "PgDn zooms back out"
    );
    let grid = crate::snap_grid::step();
    frame(
        &ctx,
        &mut app,
        tap(Key::G, egui::Modifiers::NONE),
        egui::Modifiers::NONE,
    );
    assert_ne!(crate::snap_grid::step(), grid, "G steps the snap grid");
    frame(
        &ctx,
        &mut app,
        tap(Key::Q, egui::Modifiers::NONE),
        egui::Modifiers::NONE,
    );
    assert!(app.preferences.mil_units, "Q switches the readout to mils");
    Ok(())
}

#[test]
fn a_newcomer_isolates_the_top_copper_with_shift_s_and_opens_help_with_f1() -> Result<(), String> {
    let (ctx, mut app) = sample_app(Tab::Pcb)?;
    frame(
        &ctx,
        &mut app,
        tap(Key::S, egui::Modifiers::SHIFT),
        egui::Modifiers::SHIFT,
    );
    assert_eq!(app.focus_layer.as_deref(), Some("copper:front"));
    frame(
        &ctx,
        &mut app,
        tap(Key::S, egui::Modifiers::SHIFT),
        egui::Modifiers::SHIFT,
    );
    assert_eq!(app.focus_layer, None, "Shift+S again shows every layer");
    frame(
        &ctx,
        &mut app,
        tap(Key::F1, egui::Modifiers::NONE),
        egui::Modifiers::NONE,
    );
    assert!(app.shortcuts_open, "F1 opens the shortcut list");
    Ok(())
}
