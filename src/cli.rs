use crate::app::AppState;
use crate::serial;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// CLI arguments via clap.
#[derive(clap::Parser, Debug, Clone)]
#[command(name = "plottercontroll", about = "Pen plotter controller")]
pub struct CliArgs {
    /// Input file (SVG or PNG)
    #[arg(short = 'i', long = "input")]
    pub input: Option<String>,

    /// Text to plot
    #[arg(short = 't', long = "text")]
    pub text: Option<String>,

    /// Font file path (TTF/OTF) for text rendering
    #[arg(long = "font")]
    pub font: Option<String>,

    /// Font size in mm
    #[arg(long = "font-size", default_value = "10.0")]
    pub font_size: f64,

    /// Output G-code file
    #[arg(short = 'o', long = "output")]
    pub output: Option<String>,

    /// Serial port to stream to
    #[arg(short = 'p', long = "port")]
    pub port: Option<String>,

    /// Baud rate
    #[arg(short = 'b', long = "baud", default_value = "115200")]
    pub baud: u32,

    /// Pen-up Z height
    #[arg(long = "pen-up-z", default_value = "5.0")]
    pub pen_up_z: f64,

    /// Pen-down Z height
    #[arg(long = "pen-down-z", default_value = "0.0")]
    pub pen_down_z: f64,

    /// XY feedrate (mm/min)
    #[arg(long = "feedrate-xy", default_value = "3000.0")]
    pub feedrate_xy: f64,

    /// Z feedrate (mm/min)
    #[arg(long = "feedrate-z", default_value = "300.0")]
    pub feedrate_z: f64,

    /// List available serial ports and exit
    #[arg(long = "list-ports")]
    pub list_ports: bool,

    /// Launch GUI mode
    #[arg(long = "gui")]
    pub gui: bool,

    /// Force Hershey single-line font (disable TTF)
    #[arg(long = "force-hershey")]
    pub force_hershey: bool,

    /// Enable path travel optimization (default: true)
    #[arg(long = "optimize", default_value = "true")]
    pub optimize: bool,

    /// Use global (all paths) instead of per-element optimization
    #[arg(long = "optimize-global")]
    pub optimize_global: bool,

    /// Optimization algorithm: "nearest-neighbor" or "2-opt"
    #[arg(long = "optimize-algo", default_value = "nearest-neighbor")]
    pub optimize_algo: String,

    /// Print time and ink usage estimates
    #[arg(long = "stats")]
    pub stats: bool,
}

/// Run the CLI pipeline: load, export, and/or stream.
pub fn run_cli(args: &CliArgs) -> anyhow::Result<()> {
    let mut state = AppState::default();

    // Configure
    state.gcode_config.pen_up_z = args.pen_up_z;
    state.gcode_config.pen_down_z = args.pen_down_z;
    state.gcode_config.feedrate_xy = args.feedrate_xy;
    state.gcode_config.feedrate_z = args.feedrate_z;

    // Load input
    if let Some(ref input_path) = args.input {
        let path_lower = input_path.to_lowercase();
        if path_lower.ends_with(".svg") {
            state.load_svg(input_path)?;
        } else if path_lower.ends_with(".png") {
            state.load_png(input_path)?;
        } else {
            anyhow::bail!("Unsupported file format: {}. Use .svg or .png", input_path);
        }
    } else if let Some(ref text) = args.text {
        let mut text_config = crate::importers::text::TextImportConfig::default();
        if let Some(ref font) = args.font {
            if std::path::Path::new(font).exists() {
                text_config.font_path = Some(font.clone());
            } else {
                log::warn!("Font file not found: {}. Using system font or Hershey.", font);
            }
        }
        text_config.font_size = args.font_size;
        text_config.force_hershey = args.force_hershey;
        state.load_text(text, Some(text_config));
    }

    // Configure optimizer
    state.gcode_config.optimizer_config.enabled = args.optimize;
    if args.optimize_global {
        state.gcode_config.optimizer_config.scope = crate::optimizer::OptimizerScope::Global;
    }
    state.gcode_config.optimizer_config.algorithm = match args.optimize_algo.as_str() {
        "2-opt" => crate::optimizer::OptimizerAlgorithm::TwoOpt,
        _ => crate::optimizer::OptimizerAlgorithm::NearestNeighbor,
    };

    // Auto-fit to bed
    if !state.paths.is_empty() {
        state.auto_fit();
    }

    // Optimize paths
    if state.gcode_config.optimizer_config.enabled && !state.paths.is_empty() {
        state.optimize_paths();
    }

    if state.paths.is_empty() {
        anyhow::bail!("No paths loaded. Provide --input, --text, or use --gui");
    }

    // Print info
    println!("Loaded {} paths ({} points)", state.path_count(), state.point_count());
    println!("Bounding box: {:.1}x{:.1} mm", state.bbox.width(), state.bbox.height());
    println!("Estimated drawing length: {:.0} mm", state.estimated_length());
    println!("G-code size: {} bytes", state.get_gcode().len());

    if args.stats || args.optimize {
        state.refresh_estimate();
        if let Some(ref est) = state.estimate {
            println!();
            println!("--- Time & Ink Estimate ---");
            println!("Drawing:  {}", est.drawing_time_formatted());
            println!("Travel:   {}", est.travel_time_formatted());
            println!("Pen U/D:  {}", est.pen_up_down_time_formatted());
            println!("Total:    {}", est.total_time_formatted());
            println!("Length:   {:.1} m", est.ink_length_m);
            println!("Ink:      {:.1} mm³", est.ink_volume_mm3);
        }
    }

    // Export to file
    if let Some(ref output_path) = args.output {
        state.export_gcode(output_path)?;
        println!("Exported to: {}", output_path);
    }

    // Stream to printer
    if let Some(ref port_name) = args.port {
        stream_to_printer(port_name, args.baud, &state)?;
    }

    // If no output or port specified, print G-code to stdout
    if args.output.is_none() && args.port.is_none() {
        println!("\n--- Generated G-code ---");
        println!("{}", state.get_gcode());
    }

    Ok(())
}

/// Stream G-code to the printer with progress display.
fn stream_to_printer(port_name: &str, baud: u32, state: &AppState) -> anyhow::Result<()> {
    let port = serial::open_port(port_name, baud)?;
    let cancel = Arc::new(AtomicBool::new(false));
    let rx = serial::stream_gcode(port, &state.gcode, cancel.clone());

    println!("Streaming to {}...", port_name);
    for event in rx {
        match event {
            serial::SerialEvent::LineSent { line, line_number, total } => {
                if !line.starts_with(';') && !line.is_empty() {
                    println!("[{}/{}] {}", line_number, total, line);
                }
            }
            serial::SerialEvent::Error(err) => {
                eprintln!("Error: {}", err);
                break;
            }
            serial::SerialEvent::Complete => {
                println!("Streaming complete!");
            }
            serial::SerialEvent::Connected => {
                println!("Connected to printer.");
            }
            serial::SerialEvent::Disconnected => {
                println!("Disconnected from printer.");
            }
            serial::SerialEvent::Response(resp) => {
                if resp != "ok" {
                    println!("<< {}", resp);
                }
            }
        }
    }

    Ok(())
}

/// List all available serial ports.
pub fn list_available_ports() {
    let ports = serial::list_ports();
    if ports.is_empty() {
        println!("No serial ports found.");
    } else {
        println!("Available serial ports:");
        for port in &ports {
            println!("  {}", port);
        }
    }
}
