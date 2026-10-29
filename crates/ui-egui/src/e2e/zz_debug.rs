use super::super::*;
use super::support::*;

fn dump(ctx: &egui::Context, app: &mut App) {
    frame(ctx, app, vec![], egui::Modifiers::NONE);
    let o = frame(ctx, app, vec![], egui::Modifiers::NONE);
    for s in &o.shapes {
        if let egui::Shape::Text(t) = &s.shape {
            let c = t.pos + t.galley.size() / 2.0;
            let vis = s.clip_rect.contains(c);
            eprintln!("T {:>5.0},{:>5.0} {} {:?}", c.x, c.y, if vis { "v" } else { "-" }, t.galley.text());
        }
    }
    for c in super::audit::controls(ctx, app) {
        eprintln!("C {:>5.0},{:>5.0} {:?} {}", c.rect.center().x, c.rect.center().y, c.id, c.kind);
    }
}

#[test]
fn zz_dump() {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let tab = std::env::var("DUMP_TAB").unwrap_or_default();
    match tab.as_str() {
        "pcb" => app.change_tab(Tab::Pcb),
        "lib" => app.change_tab(Tab::Libraries),
        "files" => app.change_tab(Tab::Files),
        "3d" => app.change_tab(Tab::Board3d),
        "make" => app.change_tab(Tab::PlotFiles),
        "settings" => app.change_tab(Tab::Settings),
        "rules" => app.change_tab(Tab::Rules),
        "checks" => app.change_tab(Tab::Checks),
        _ => {}
    }
    if let Ok(k) = std::env::var("DUMP_ROW") {
        open_tree_row(&ctx, &mut app, &k);
    }
    if let Ok(l) = std::env::var("DUMP_LEFT") {
        app.preferences.left_tab = l.parse().unwrap_or(0);
    }
    dump(&ctx, &mut app);
}
