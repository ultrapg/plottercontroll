mod app;
mod cli;
mod estimator;
mod gcode_gen;
mod geometry;
mod hershey;
mod importers;
mod optimizer;
mod profiles;
mod project;
mod serial;

#[cfg(feature = "gui")]
mod text_editor;

#[cfg(feature = "gui")]
mod gui;

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let args: cli::CliArgs = clap::Parser::parse();

    // Handle --list-ports
    if args.list_ports {
        cli::list_available_ports();
        return Ok(());
    }

    // GUI mode if --gui flag or no CLI arguments provided
    let has_cli_input = args.input.is_some()
        || args.text.is_some()
        || args.output.is_some()
        || args.port.is_some();

    if args.gui || !has_cli_input {
        run_gui();
    } else {
        cli::run_cli(&args)?;
    }

    Ok(())
}

#[cfg(feature = "gui")]
fn run_gui() {
    use eframe::egui;
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("PlotterControll")
            .with_inner_size([1280.0, 800.0]),
        ..Default::default()
    };
    eframe::run_native(
        "PlotterControll",
        options,
        Box::new(|_cc| Ok(Box::new(gui::PlotterApp::default()))),
    )
    .expect("Failed to launch GUI");
}

#[cfg(not(feature = "gui"))]
fn run_gui() {
    eprintln!("GUI mode requires the 'gui' feature. Rebuild with:\n  cargo run --features gui");
    std::process::exit(1);
}
