mod host;
fn main() -> eframe::Result {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let option = |key: &str| {
        args.iter()
            .position(|a| a == key)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!(
            "PCBCraft — one-window Rust EDA\n\n--sample | --project FILE\n--control 127.0.0.1:7991\n--size WxH\n--tab files|schematic|pcb|rules|settings|libraries|project-library|symbol-editor|footprint-editor|checks|3d\n--light\n--capture /absolute/path.png\n--select REFERENCE\n--target silk|edge"
        );
        return Ok(());
    }
    let mut session = if args.iter().any(|a| a == "--sample") {
        match pcbcraft_engine::Session::sample() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("sample: {e}");
                std::process::exit(1);
            }
        }
    } else {
        pcbcraft_engine::Session::default()
    };
    if let Some(path) = option("--project") {
        let result = pcbcraft_io::load(std::path::Path::new(&path))
            .and_then(|project| session.replace(project).map_err(|e| e.to_string()));
        if let Err(e) = result {
            eprintln!("project: {e}");
            std::process::exit(1);
        }
    }
    let hub = match pcbcraft_mcp::DocumentHub::new(session, option("--project")) {
        Ok(h) => h,
        Err(e) => {
            eprintln!("documents: {e}");
            std::process::exit(1);
        }
    };
    let shared = match hub.active_session() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("documents: {e}");
            std::process::exit(1);
        }
    };
    if let Some(address) = option("--control")
        && let Err(e) = pcbcraft_mcp::start_bridge(hub.clone(), &address)
    {
        eprintln!("control bridge: {e}");
        std::process::exit(1);
    }
    let capture = option("--capture");
    let tab = option("--tab");
    let light = args.iter().any(|a| a == "--light");
    let project_path = option("--project");
    let select = option("--select");
    let target = option("--target");
    let options = eframe::NativeOptions {
        viewport: egui_viewport(),
        ..Default::default()
    };
    eframe::run_native(
        "PCBCraft",
        options,
        Box::new(move |cc| {
            Ok(Box::new(
                pcbcraft_ui_egui::App::new(cc, shared, capture, tab.as_deref(), light)
                    .with_project_path(project_path)
                    .with_capture_select(select)
                    .with_capture_target(target)
                    .with_host(host::host()),
            ))
        }),
    )
}
fn egui_viewport() -> eframe::egui::ViewportBuilder {
    let size = std::env::args()
        .skip_while(|a| a != "--size")
        .nth(1)
        .and_then(|s: String| {
            let (w, h) = s.split_once('x')?;
            Some([w.parse::<f32>().ok()?, h.parse::<f32>().ok()?])
        })
        .unwrap_or([1440.0, 940.0]);
    eframe::egui::ViewportBuilder::default()
        .with_inner_size(size)
        .with_min_inner_size([1000.0, 680.0])
}
