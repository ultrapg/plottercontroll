use crate::app::AppState;
use crate::geometry::Point;
use crate::importers::png::{import_png, PngImportConfig};
use crate::importers::svg::{import_svg, SvgImportConfig};
use crate::serial;
use crate::text_editor::TextEditor;
use eframe::egui::{self, Color32, Pos2, Rounding, Sense, Shape, Stroke, Vec2};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::Arc;
use std::thread::JoinHandle;

#[derive(PartialEq)]
enum DragMode {
    Pan,
    Position,
}

/// The egui application.
pub struct PlotterApp {
    state: AppState,
    pan: Vec2,
    zoom: f64,
    needs_zoom_fit: bool,
    selected_port: String,
    baud_rate: u32,
    available_ports: Vec<String>,
    stream_active: bool,
    stream_cancel: Option<Arc<AtomicBool>>,
    stream_rx: Option<Receiver<serial::SerialEvent>>,
    stream_progress: (u32, u32),
    stream_log: Vec<String>,
    show_error: bool,
    error_message: String,
    // Text dialog state
    show_text_dialog: bool,
    dialog_text: String,
    dialog_font_size: f64,
    dialog_font_path: String,
    dialog_force_hershey: bool,
    dialog_fill_enabled: bool,
    dialog_fill_spacing: f64,
    // Drag mode
    drag_mode: DragMode,
    // Tracks whether we've saved a snapshot for the current position-drag
    drag_snapshotted: bool,
    // Index in source_elements of element being edited (None = adding new)
    editing_element_index: Option<usize>,
    // Index in source_elements of element selected for per-element movement (None = move all)
    selected_element: Option<usize>,
    // Embedded text editor
    text_editor: TextEditor,
    // PNG import dialog
    show_png_dialog: bool,
    png_dialog_config: PngImportConfig,
    pending_png_path: Option<String>,
    png_importing: bool,
    png_import_rx: Option<Receiver<anyhow::Result<Vec<crate::geometry::PathData>>>>,
    png_import_handle: Option<JoinHandle<()>>,
    // SVG import dialog
    show_svg_dialog: bool,
    svg_dialog_config: SvgImportConfig,
    pending_svg_path: Option<String>,
    svg_importing: bool,
    svg_import_rx: Option<Receiver<anyhow::Result<Vec<crate::geometry::PathData>>>>,
    svg_import_handle: Option<JoinHandle<()>>,
    show_feedrate_help: bool,
}

impl Default for PlotterApp {
    fn default() -> Self {
        PlotterApp {
            state: AppState::default(),
            pan: Vec2::ZERO,
            zoom: 1.0,
            needs_zoom_fit: false,
            selected_port: String::new(),
            baud_rate: 115200,
            available_ports: serial::list_ports(),
            stream_active: false,
            stream_cancel: None,
            stream_rx: None,
            stream_progress: (0, 0),
            stream_log: Vec::new(),
            show_error: false,
            error_message: String::new(),
            show_text_dialog: false,
            dialog_text: String::new(),
            dialog_font_size: 10.0,
            dialog_font_path: String::new(),
            dialog_force_hershey: false,
            dialog_fill_enabled: false,
            dialog_fill_spacing: 0.5,
            drag_mode: DragMode::Pan,
            drag_snapshotted: false,
            editing_element_index: None,
            selected_element: None,
            text_editor: TextEditor::default(),
            show_png_dialog: false,
            png_dialog_config: PngImportConfig::default(),
            pending_png_path: None,
            png_importing: false,
            png_import_rx: None,
            png_import_handle: None,
            show_svg_dialog: false,
            svg_dialog_config: SvgImportConfig::default(),
            pending_svg_path: None,
            svg_importing: false,
            svg_import_rx: None,
            svg_import_handle: None,
            show_feedrate_help: false,
        }
    }
}

impl eframe::App for PlotterApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Refresh available ports periodically
        let time = ctx.input(|i| i.time);
        if self.available_ports.is_empty() || (time as u64) % 5 == 0 {
            self.available_ports = serial::list_ports();
        }

        self.poll_serial();

        // Zoom to fit after loading
        if self.needs_zoom_fit && !self.state.paths.is_empty() {
            let area_size = ctx.available_rect().size();
            let sidebar_w = 280.0;
            let margin = 60.0;
            let avail_w = (area_size.x - sidebar_w - margin).max(1.0) as f64;
            let avail_h = (area_size.y - margin).max(1.0) as f64;
            let path_w = self.state.bbox.width().max(1.0);
            let path_h = self.state.bbox.height().max(1.0);
            self.zoom = (avail_w / path_w).min(avail_h / path_h).min(10.0).max(0.05);
            let c = self.state.bbox.center();
            self.pan = Vec2::new(
                -(c.x * self.zoom) as f32,
                (c.y * self.zoom) as f32,
            );
            self.needs_zoom_fit = false;
        }

        // Error popup
        if self.show_error {
            egui::Window::new("Error")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.label(&self.error_message);
                    if ui.button("OK").clicked() {
                        self.show_error = false;
                    }
                });
        }

        // PNG import config dialog
        if self.show_png_dialog {
            egui::Window::new("PNG Import Settings")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.label("Configure how the PNG image is processed:");
                    ui.separator();

                    // Mode selector
                    ui.horizontal(|ui| {
                        ui.label("Mode:");
                        let current_mode = self.png_dialog_config.mode.name();
                        egui::ComboBox::from_id_salt("png_mode")
                            .selected_text(current_mode)
                            .show_ui(ui, |ui| {
                                for mode in crate::importers::png::PngImportMode::all() {
                                    if ui.selectable_label(
                                        self.png_dialog_config.mode == *mode,
                                        mode.name(),
                                    ).clicked() {
                                        self.png_dialog_config.mode = *mode;
                                    }
                                }
                            });
                    });

                    // Mode-specific options
                    match self.png_dialog_config.mode {
                        crate::importers::png::PngImportMode::EdgeDetection => {
                            ui.horizontal(|ui| {
                                ui.label("Canny low:");
                                ui.add(egui::DragValue::new(&mut self.png_dialog_config.canny_low)
                                    .speed(1.0).range(0.0..=100.0));
                            });
                            ui.horizontal(|ui| {
                                ui.label("Canny high:");
                                ui.add(egui::DragValue::new(&mut self.png_dialog_config.canny_high)
                                    .speed(1.0).range(0.0..=100.0));
                            });
                        }
                        crate::importers::png::PngImportMode::Monochrome => {
                            ui.horizontal(|ui| {
                                ui.label("Threshold (0 = auto/Otsu):");
                                let mut t = self.png_dialog_config.threshold_value as f64;
                                ui.add(egui::DragValue::new(&mut t)
                                    .speed(1.0).range(0.0..=255.0));
                                self.png_dialog_config.threshold_value = t.round() as u8;
                            });
                            ui.label("0 = automatic Otsu threshold");
                        }
                        crate::importers::png::PngImportMode::Centerlines => {
                            ui.label("Binarizes + thins to centerlines (no closed contours)");
                        }
                    }

                    // Fill options (only for closed-contour modes)
                    if self.png_dialog_config.mode != crate::importers::png::PngImportMode::Centerlines {
                        ui.checkbox(&mut self.png_dialog_config.fill_enabled, "Fill contours with hatch");
                        if self.png_dialog_config.fill_enabled {
                            ui.horizontal(|ui| {
                                ui.label("Fill spacing:");
                                ui.add(egui::DragValue::new(&mut self.png_dialog_config.fill_spacing)
                                    .speed(0.05).range(0.1..=5.0));
                                ui.label("mm");
                            });
                        }
                    }

                    // Common options
                    ui.horizontal(|ui| {
                        ui.label("Simplify tolerance:");
                        ui.add(egui::DragValue::new(&mut self.png_dialog_config.simplify_epsilon)
                            .speed(0.1).range(0.1..=10.0));
                        ui.label("px");
                    });
                    ui.horizontal(|ui| {
                        ui.label("Min contour length:");
                        ui.add(egui::DragValue::new(&mut self.png_dialog_config.min_contour_len)
                            .speed(1).range(1..=200));
                        ui.label("points");
                    });
                    ui.horizontal(|ui| {
                        ui.label("Pixel scale:");
                        ui.add(egui::DragValue::new(&mut self.png_dialog_config.pixel_to_mm)
                            .speed(0.01).range(0.01..=10.0));
                        ui.label("mm/px");
                    });
                    ui.separator();
                    ui.horizontal(|ui| {
                            if ui.button("Import").clicked() {
                                if !self.state.paths.is_empty() {
                                    self.state.save_snapshot();
                                }
                                self.state.png_config = self.png_dialog_config.clone();
                            let path = self.pending_png_path.take().unwrap_or_default();
                            let config = self.png_dialog_config.clone();
                            let (tx, rx) = mpsc::channel();
                            self.png_import_rx = Some(rx);
                            self.png_importing = true;
                            self.show_png_dialog = false;
                            let handle = std::thread::spawn(move || {
                                let result = import_png(&path, &config);
                                tx.send(result).ok();
                            });
                            self.png_import_handle = Some(handle);
                        }
                        if ui.button("Cancel").clicked() {
                            self.show_png_dialog = false;
                            self.pending_png_path = None;
                        }
                    });
                });
        }

        // PNG import progress dialog
        if self.png_importing {
            egui::Window::new("Importing PNG")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.label("Processing PNG image...");
                    ui.add_space(8.0);
                    ui.add(egui::Spinner::new());
                    ui.add_space(4.0);
                    ui.label("This may take a moment for large images.");
                    ctx.request_repaint();
                });

            if let Some(ref rx) = self.png_import_rx {
                match rx.try_recv() {
                    Ok(Ok(paths)) => {
                        let png_path = self.pending_png_path.clone().unwrap_or_default();
                        self.state.add_file_source(paths, &png_path, crate::app::InputType::Png);
                        self.needs_zoom_fit = true;
                        self.png_importing = false;
                        self.png_import_rx = None;
                        self.png_import_handle = None;
                        self.pending_png_path = None;
                    }
                    Ok(Err(e)) => {
                        self.error_message = format!("PNG import failed: {}", e);
                        self.show_error = true;
                        self.png_importing = false;
                        self.png_import_rx = None;
                        self.png_import_handle = None;
                    }
                    Err(TryRecvError::Empty) => {}
                    Err(TryRecvError::Disconnected) => {
                        self.error_message = String::from("PNG import thread crashed — image may be too large or corrupt. Try adjusting threshold values.");
                        self.show_error = true;
                        self.png_importing = false;
                        self.png_import_rx = None;
                        self.png_import_handle = None;
                    }
                }
            }
        }

        // SVG import config dialog
        if self.show_svg_dialog {
            egui::Window::new("SVG Import Settings")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.label("Configure how the SVG image is processed:");
                    ui.separator();
                    ui.checkbox(&mut self.svg_dialog_config.use_raster,
                        "Raster mode (render + threshold instead of vector extraction)");
                    if self.svg_dialog_config.use_raster {
                        ui.horizontal(|ui| {
                            ui.label("Raster DPI:");
                            ui.add(egui::DragValue::new(&mut self.svg_dialog_config.raster_dpi)
                                .speed(10.0).range(72.0..=1200.0));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Threshold (0 = auto/Otsu):");
                            let mut t = self.svg_dialog_config.threshold_value as f64;
                            ui.add(egui::DragValue::new(&mut t)
                                .speed(1.0).range(0.0..=255.0));
                            self.svg_dialog_config.threshold_value = t.round() as u8;
                        });
                        ui.horizontal(|ui| {
                            ui.label("Simplify tolerance:");
                            ui.add(egui::DragValue::new(&mut self.svg_dialog_config.simplify_epsilon)
                                .speed(0.1).range(0.1..=10.0));
                            ui.label("px");
                        });
                        ui.horizontal(|ui| {
                            ui.label("Min contour length:");
                            ui.add(egui::DragValue::new(&mut self.svg_dialog_config.min_contour_len)
                                .speed(1).range(1..=200));
                            ui.label("points");
                        });
                    }
                    ui.checkbox(&mut self.svg_dialog_config.fill_enabled, "Fill contours with hatch");
                    if self.svg_dialog_config.fill_enabled {
                        ui.horizontal(|ui| {
                            ui.label("Fill spacing:");
                            ui.add(egui::DragValue::new(&mut self.svg_dialog_config.fill_spacing)
                                .speed(0.05).range(0.1..=5.0));
                            ui.label("mm");
                        });
                    }
                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui.button("Import").clicked() {
                            if !self.state.paths.is_empty() {
                                self.state.save_snapshot();
                            }
                            self.state.svg_config = self.svg_dialog_config.clone();
                            let path = self.pending_svg_path.take().unwrap_or_default();
                            let config = self.svg_dialog_config.clone();
                            let (tx, rx) = mpsc::channel();
                            self.svg_import_rx = Some(rx);
                            self.svg_importing = true;
                            self.show_svg_dialog = false;
                            let handle = std::thread::spawn(move || {
                                let result = import_svg(&path, &config);
                                tx.send(result).ok();
                            });
                            self.svg_import_handle = Some(handle);
                        }
                        if ui.button("Cancel").clicked() {
                            self.show_svg_dialog = false;
                            self.pending_svg_path = None;
                        }
                    });
                });
        }

        // SVG import progress dialog
        if self.svg_importing {
            egui::Window::new("Importing SVG")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.label("Processing SVG image...");
                    ui.add_space(8.0);
                    ui.add(egui::Spinner::new());
                    ui.add_space(4.0);
                    ui.label("This may take a moment for large images.");
                    ctx.request_repaint();
                });

            if let Some(ref rx) = self.svg_import_rx {
                match rx.try_recv() {
                    Ok(Ok(paths)) => {
                        let svg_path = self.pending_svg_path.clone().unwrap_or_default();
                        self.state.add_file_source(paths, &svg_path, crate::app::InputType::Svg);
                        self.needs_zoom_fit = true;
                        self.svg_importing = false;
                        self.svg_import_rx = None;
                        self.svg_import_handle = None;
                        self.pending_svg_path = None;
                    }
                    Ok(Err(e)) => {
                        self.error_message = format!("SVG import failed: {}", e);
                        self.show_error = true;
                        self.svg_importing = false;
                        self.svg_import_rx = None;
                        self.svg_import_handle = None;
                    }
                    Err(TryRecvError::Empty) => {}
                    Err(TryRecvError::Disconnected) => {
                        self.error_message = String::from("SVG import thread crashed — image may be too large or corrupt.");
                        self.show_error = true;
                        self.svg_importing = false;
                        self.svg_import_rx = None;
                        self.svg_import_handle = None;
                    }
                }
            }
        }

        // Embedded text editor (rendered first to capture keyboard focus)
        if let Some(result) = self.text_editor.show(ctx) {
            if result.is_empty() {
                // Cancelled — do nothing
            } else {
                self.dialog_text = result;
            }
        }

        // Feedrate help popup
        if self.show_feedrate_help {
            let w = ctx.available_rect().width();
            egui::Window::new("Feedrate & Speed Guide")
                .collapsible(false)
                .resizable(false)
                .auto_sized()
                .min_width(w * 0.60)
                .max_width(w * 0.66)
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.add_space(10.0);
                    ui.label("• Max: fastest the pen will move during drawing. Too fast = uneven lines, skipping.");
                    ui.label("• Min: slowest the pen will move. Too slow = ink pools and soaks the paper.");
                    ui.label("• Z: speed for lifting/lowering the pen. Keeps movements crisp.");
                    ui.label("• Travel: speed for non-drawing moves between paths. Set high to save time.");
                    ui.separator();
                    ui.label("Constant Pen Speed — evens out the pen speed across all segments.");
                    ui.label("Each segment gets its own feedrate so it takes the same minimum time:");
                    ui.label("  feedrate = length ÷ segment_time × 60");
                    ui.label("Then clamped to the Min/Max F limits below it.");
                    ui.label("• Seg. time: target duration for any single move.");
                    ui.label("  Higher = smoother but slower overall.");
                    ui.label("  Start at 0.05s and adjust up if you hear stuttering.");
                    ui.label("• Min F: lowest feedrate allowed (prevents ink bleeding on long segments).");
                    ui.label("• Max F: highest feedrate allowed (prevents uneven lines on short segments).");
                    ui.separator();
                    ui.add_space(10.0);
                    if ui.button("Close").clicked() {
                        self.show_feedrate_help = false;
                    }
                    ui.add_space(10.0);
                });
        }

        // Text input dialog
        if self.show_text_dialog {
            egui::Window::new("Add Text")
                .collapsible(false)
                .resizable(false)
                .auto_sized()
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Text:");
                        if ui.button("Open Editor").clicked() {
                            self.text_editor.open_with(&self.dialog_text);
                        }
                    });
                    if !self.dialog_text.is_empty() {
                        ui.add_space(2.0);
                        let preview = if self.dialog_text.len() > 200 {
                            format!("{}…", &self.dialog_text[..200])
                        } else {
                            self.dialog_text.clone()
                        };
                        let lines: Vec<&str> = preview.lines().collect();
                        let preview = if lines.len() > 10 {
                            format!("{}…\n({} lines total)", lines[..10].join("\n"), self.dialog_text.lines().count())
                        } else {
                            preview
                        };
                        egui::Frame::group(ui.style()).show(ui, |ui| {
                            ui.label(egui::RichText::new(preview).monospace().size(12.0));
                        });
                    }
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label("Size:");
                        if ui.button("−").clicked() {
                            let pct = (self.dialog_font_size / 10.0 * 100.0 - 10.0).max(10.0);
                            self.dialog_font_size = (pct / 100.0 * 10.0 * 10.0).round() / 10.0;
                        }
                        let pct = (self.dialog_font_size / 10.0 * 100.0) as i32;
                        ui.label(format!("{}%", pct));
                        if ui.button("+").clicked() {
                            let pct = (self.dialog_font_size / 10.0 * 100.0 + 10.0).min(500.0);
                            self.dialog_font_size = (pct / 100.0 * 10.0 * 10.0).round() / 10.0;
                        }
                        ui.label(format!("({:.1} mm)", self.dialog_font_size));
                    });
                    ui.horizontal(|ui| {
                        ui.label("Font:");
                        if ui.button("Browse...").clicked() {
                            if let Some(path) = open_file_dialog("ttf") {
                                self.dialog_font_path = path;
                            }
                        }
                        if !self.dialog_font_path.is_empty() && ui.button("X").clicked() {
                            self.dialog_font_path.clear();
                        }
                    });
                    if !self.dialog_font_path.is_empty() {
                        ui.label(&self.dialog_font_path);
                    }
                    ui.checkbox(&mut self.dialog_force_hershey, "Force Hershey (single-line)");
                    ui.checkbox(&mut self.dialog_fill_enabled, "Fill");
                    if self.dialog_fill_enabled {
                        ui.horizontal(|ui| {
                            ui.label("Fill spacing:");
                            ui.add(egui::DragValue::new(&mut self.dialog_fill_spacing)
                                .speed(0.05).range(0.1..=5.0));
                            ui.label("mm");
                        });
                    }
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        let can_add = !self.dialog_text.trim().is_empty();
                        if ui.add_enabled(can_add, egui::Button::new("Add to Canvas")).clicked() {
                            let mut text_config = crate::importers::text::TextImportConfig::default();
                            if !self.dialog_font_path.is_empty()
                                && std::path::Path::new(&self.dialog_font_path).exists()
                            {
                                text_config.font_path = Some(self.dialog_font_path.clone());
                            }
                            text_config.font_size = self.dialog_font_size;
                            text_config.force_hershey = self.dialog_force_hershey;
                            text_config.fill_enabled = self.dialog_fill_enabled;
                            text_config.fill_spacing = self.dialog_fill_spacing;
                            match self.editing_element_index {
                                Some(idx) => {
                                    self.state.save_snapshot();
                                    self.state.edit_text(idx, &self.dialog_text, text_config);
                                }
                                None => {
                                    self.state.save_snapshot();
                                    self.state.add_text(&self.dialog_text, text_config);
                                }
                            }
                            self.editing_element_index = None;
                            if let Some(ref err) = self.state.error {
                                self.error_message = err.clone();
                                self.show_error = true;
                                self.state.error = None;
                            } else {
                                self.state.position_offset = Point::new(0.0, 0.0);
                                self.needs_zoom_fit = true;
                            }
                            self.show_text_dialog = false;
                        }
                        if ui.button("Cancel").clicked() {
                            self.show_text_dialog = false;
                        }
                    });
                });
        }

        // Top menu bar
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Export G-code...").clicked() {
                        if let Some(path) = save_file_dialog("gcode", "G-Code Files") {
                            self.export_gcode(&path);
                        }
                        ui.close_menu();
                    }
                    if ui.button("Save Project...").clicked() {
                        if let Some(path) = save_file_dialog("pcp", "PlotterControll Project") {
                            if let Err(e) = crate::project::save_project(&self.state, &path) {
                                self.error_message = format!("Failed to save project: {}", e);
                                self.show_error = true;
                            }
                        }
                        ui.close_menu();
                    }
                    if ui.button("Open Project...").clicked() {
                        if let Some(path) = open_file_dialog("pcp") {
                            self.state.clear();
                            if let Err(e) = crate::project::load_project(&mut self.state, &path) {
                                self.error_message = format!("Failed to load project: {}", e);
                                self.show_error = true;
                            }
                            self.needs_zoom_fit = true;
                        }
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Clear").clicked() {
                        self.state.clear();
                        self.pan = Vec2::ZERO;
                        self.zoom = 1.0;
                    }
                    if ui.button("Quit").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                ui.menu_button("Import", |ui| {
                    if ui.button("SVG...").clicked() {
                        if let Some(path) = open_file_dialog("svg") {
                            self.pending_svg_path = Some(path);
                            self.svg_dialog_config = self.state.svg_config.clone();
                            self.show_svg_dialog = true;
                        }
                        ui.close_menu();
                    }
                    if ui.button("PNG...").clicked() {
                        if let Some(path) = open_file_dialog("png") {
                            self.pending_png_path = Some(path);
                            self.png_dialog_config = self.state.png_config.clone();
                            self.show_png_dialog = true;
                        }
                        ui.close_menu();
                    }
                });
                ui.menu_button("View", |ui| {
                    if ui.button("Zoom to Fit").clicked() {
                        self.needs_zoom_fit = true;
                        ui.close_menu();
                    }
                    if ui.button("Zoom In").clicked() {
                        self.zoom = (self.zoom * 1.25).min(50.0);
                        ui.close_menu();
                    }
                    if ui.button("Zoom Out").clicked() {
                        self.zoom = (self.zoom * 0.8).max(0.05);
                        ui.close_menu();
                    }
                    if ui.button("Reset Zoom").clicked() {
                        self.zoom = 1.0;
                        ui.close_menu();
                    }
                    if ui.button("Reset Pan").clicked() {
                        self.pan = Vec2::ZERO;
                        ui.close_menu();
                    }
                });
                ui.menu_button("Device", |ui| {
                    let mut names: Vec<String> = std::iter::once("Custom".to_string())
                        .chain(self.state.profiles.iter().map(|p| p.name.clone()))
                        .collect();
                    for n in &names {
                        let active = self.state.profile_name == *n;
                        let label = if active { format!("> {}", n) } else { n.clone() };
                        if ui.button(label).clicked() {
                            self.state.apply_profile(n);
                            self.needs_zoom_fit = true;
                            ui.close_menu();
                        }
                    }
                });
            });
        });

        // Left sidebar
        egui::SidePanel::left("sidebar")
            .resizable(true)
            .default_width(280.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                egui::CollapsingHeader::new("Pen")
                    .id_salt("sb_pen")
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.label("Z Heights (mm)")
                            .on_hover_text("Position of the pen when up (off paper) and down (touching paper).");
                        ui.horizontal(|ui| {
                            ui.label("Up:")
                                .on_hover_text("Z height when pen is raised above the paper.");
                            ui.add(egui::DragValue::new(&mut self.state.gcode_config.pen_up_z)
                                .speed(0.1).range(0.0..=50.0));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Down:")
                                .on_hover_text("Z height when pen tip touches the paper.");
                            ui.add(egui::DragValue::new(&mut self.state.gcode_config.pen_down_z)
                                .speed(0.1).range(-10.0..=50.0));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Thickness:")
                                .on_hover_text("Estimated line width — used for preview scaling, not G-code.");
                            ui.add(egui::DragValue::new(&mut self.state.gcode_config.pen_thickness)
                                .speed(0.05).range(0.1..=5.0));
                            ui.label("mm");
                        });
                        ui.separator();
                        ui.horizontal(|ui| {
                            ui.label("XY Speeds (mm/min)")
                                .on_hover_text("Feedrate limits for drawing moves.");
                            if ui.button("?")
                                .on_hover_text("Open detailed guide about all speed settings.")
                                .clicked() {
                                self.show_feedrate_help = true;
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label("Max:")
                                .on_hover_text("Fastest the pen moves while drawing. Too fast = uneven lines.");
                            ui.add(egui::DragValue::new(&mut self.state.gcode_config.feedrate_xy)
                                .speed(50.0).range(100.0..=15000.0));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Min:")
                                .on_hover_text("Slowest the pen moves. Too slow = ink pools and soaks paper.");
                            ui.add(egui::DragValue::new(&mut self.state.gcode_config.min_feedrate_xy)
                                .speed(50.0).range(10.0..=5000.0));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Z:")
                                .on_hover_text("Speed for raising/lowering the pen (Z axis).");
                            ui.add(egui::DragValue::new(&mut self.state.gcode_config.feedrate_z)
                                .speed(10.0).range(10.0..=5000.0));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Travel:")
                                .on_hover_text("Speed for non-drawing moves between paths. Set high to save time.");
                            ui.add(egui::DragValue::new(&mut self.state.gcode_config.travel_feedrate)
                                .speed(100.0).range(500.0..=30000.0));
                        });
                        ui.separator();
                        ui.label("Bed Dimensions (mm)")
                            .on_hover_text("Physical bed size. Used for preview fitting and coordinate bounds.");
                        ui.horizontal(|ui| {
                            ui.label("Width:")
                                .on_hover_text("Bed width along the X axis.");
                            ui.add(egui::DragValue::new(&mut self.state.gcode_config.bed_width)
                                .speed(1.0).range(10.0..=1000.0));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Height:")
                                .on_hover_text("Bed height along the Y axis.");
                            ui.add(egui::DragValue::new(&mut self.state.gcode_config.bed_height)
                                .speed(1.0).range(10.0..=1000.0));
                        });
                        ui.separator();
                        if ui.checkbox(&mut self.state.gcode_config.auto_home, "Auto-home on start")
                            .on_hover_text("Home all axes (G28) before starting the plot.")
                            .changed() {
                            self.state.gcode_dirty = true;
                        }
                        if ui.checkbox(&mut self.state.gcode_config.y_axis_invert, "Invert Y axis")
                            .on_hover_text("Flip Y direction — needed for bed-slinger printers (Y moves opposite).")
                            .changed() {
                            self.state.gcode_dirty = true;
                        }
                        ui.separator();
                        ui.horizontal(|ui| {
                            if ui.checkbox(&mut self.state.gcode_config.const_speed, "Constant pen speed")
                                .on_hover_text("Adjusts per-segment feedrate so every move takes the same minimum time. Prevents stutter on short segments and ink bleeding on long ones.")
                                .changed() {
                                self.state.gcode_dirty = true;
                            }
                            if ui.button("?").clicked() {
                                self.show_feedrate_help = true;
                            }
                        });
                        if self.state.gcode_config.const_speed {
                            ui.horizontal(|ui| {
                                ui.label("Seg. time:")
                                    .on_hover_text("Target duration for each segment. Higher = smoother but slower. Start at 0.05 s.");
                                if ui.add(egui::DragValue::new(&mut self.state.gcode_config.min_segment_time)
                                    .speed(0.005).range(0.005..=1.0).suffix("s")).changed() {
                                    self.state.gcode_dirty = true;
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.label("Min F:")
                                    .on_hover_text("Lowest feedrate allowed when constant speed is active. Prevents ink soaking on long segments.");
                                if ui.add(egui::DragValue::new(&mut self.state.gcode_config.const_speed_min_feedrate)
                                    .speed(50.0).range(10.0..=5000.0)).changed() {
                                    self.state.gcode_dirty = true;
                                }
                                ui.label("Max F:")
                                    .on_hover_text("Highest feedrate allowed when constant speed is active. Prevents uneven lines on short segments.");
                                if ui.add(egui::DragValue::new(&mut self.state.gcode_config.const_speed_max_feedrate)
                                    .speed(100.0).range(100.0..=15000.0)).changed() {
                                    self.state.gcode_dirty = true;
                                }
                            });
                        }
                    });

                egui::CollapsingHeader::new("Positioning")
                    .id_salt("sb_pos")
                    .default_open(true)
                    .show(ui, |ui| {
                        let pos_label = match self.drag_mode {
                            DragMode::Pan => "Drag Mode: Pan",
                            DragMode::Position => "Drag Mode: Position",
                        };
                        if ui.button(pos_label)
                            .on_hover_text("Pan: click & drag to move the view. Position: click & drag to move the selected element.")
                            .clicked() {
                                self.drag_mode = match self.drag_mode {
                                    DragMode::Pan => DragMode::Position,
                                    DragMode::Position => DragMode::Pan,
                                };
                        }
                        if ui.button("Reset Position")
                            .on_hover_text("Reset all element position offsets back to zero.")
                            .clicked()
                            && (self.state.position_offset.x != 0.0
                                || self.state.position_offset.y != 0.0)
                        {
                            self.state.save_snapshot();
                            self.state.position_offset = Point::new(0.0, 0.0);
                            self.state.gcode_dirty = true;
                            self.needs_zoom_fit = true;
                        }
                    });

                egui::CollapsingHeader::new("Serial Port")
                    .id_salt("sb_serial")
                    .default_open(true)
                    .show(ui, |ui| {
                        let port_label: &str = if self.selected_port.is_empty() {
                            "Select port..."
                        } else {
                            &self.selected_port
                        };
                        egui::ComboBox::from_id_salt("port_selector")
                            .selected_text(port_label)
                            .show_ui(ui, |ui| {
                                for port in &self.available_ports {
                                    ui.selectable_value(&mut self.selected_port, port.clone(), port);
                                }
                            })
                            .response
                            .on_hover_text("USB port your printer is connected to. Select one before streaming.");
                        ui.horizontal(|ui| {
                            ui.label("Baud:")
                                .on_hover_text("Baud rate matching your printer firmware (usually 115200 or 250000).");
                            ui.add(egui::DragValue::new(&mut self.baud_rate)
                                .speed(1.0).range(9600..=250000));
                        });
                        if ui.button("Refresh Ports")
                            .on_hover_text("Re-scan all available serial ports.")
                            .clicked() {
                            self.available_ports = serial::list_ports();
                        }
                    });

                egui::CollapsingHeader::new("Actions")
                    .id_salt("sb_actions")
                    .default_open(true)
                    .show(ui, |ui| {
                        let can_undo = !self.state.undo_stack.is_empty();
                        let can_redo = !self.state.redo_stack.is_empty();
                        ui.horizontal(|ui| {
                            if ui.add_enabled(can_undo, egui::Button::new("Undo"))
                                .on_hover_text("Undo the last change (up to 50 steps).")
                                .clicked() {
                                self.state.undo();
                                self.needs_zoom_fit = true;
                            }
                            if ui.add_enabled(can_redo, egui::Button::new("Redo"))
                                .on_hover_text("Redo a previously undone change.")
                                .clicked() {
                                self.state.redo();
                                self.needs_zoom_fit = true;
                            }
                        });

                        if ui.button("Auto-Fit to Bed")
                            .on_hover_text("Scale and center all elements to fit within the bed dimensions.")
                            .clicked() {
                            self.state.save_snapshot();
                            self.state.auto_fit();
                            self.needs_zoom_fit = true;
                        }

                        if ui.button("Export G-code...")
                            .on_hover_text("Save the generated G-code as a .gcode file for later use.")
                            .clicked() {
                            if let Some(path) = save_file_dialog("gcode", "G-Code Files") {
                                self.export_gcode(&path);
                            }
                        }

                        if !self.stream_active {
                            let btn = egui::Button::new("Stream to Printer");
                            let resp = if self.selected_port.is_empty() {
                                ui.add_enabled(false, btn)
                            } else {
                                ui.add(btn)
                            };
                            if resp.clicked() {
                                self.start_streaming();
                            }
                        } else {
                            if ui.button("Stop Streaming")
                                .on_hover_text("Cancel the streaming job immediately.")
                                .clicked() {
                                self.stop_streaming();
                            }
                            ui.label(format!("Progress: {}/{}",
                                self.stream_progress.0, self.stream_progress.1))
                                .on_hover_text("G-code lines sent vs total.");
                            if self.stream_progress.1 > 0 {
                                let p = self.stream_progress.0 as f32 / self.stream_progress.1 as f32;
                                ui.add(egui::ProgressBar::new(p).show_percentage());
                            }
                        }
                    });

                egui::CollapsingHeader::new("Info")
                    .id_salt("sb_info")
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.label(&self.state.status);
                        ui.label(format!("Paths: {}", self.state.path_count()));
                        ui.label(format!("Points: {}", self.state.point_count()));
                        ui.label(format!("Est. length: {:.0} mm", self.state.estimated_length()));
                    ui.label(format!("Elements: {}", self.state.source_elements.len()));
                    ui.label(format!("Files: {}, Text: {}",
                        self.state.file_count(), self.state.text_count()));
                    });
                });
            });

        // Right panel: element manager
        egui::SidePanel::right("element_manager")
            .resizable(true)
            .default_width(260.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.heading("Sources");
                    ui.separator();

                    if self.state.source_elements.is_empty() {
                        ui.label("No sources loaded.");
                        ui.label("Import a file or add text.");
                    } else {
                        let count = self.state.source_elements.len();
                        let mut to_remove: Option<usize> = None;
                        let mut to_swap: Option<(usize, usize)> = None;
                        let mut to_select: Option<Option<usize>> = None;

                        let elements = self.state.source_elements.clone();
                        for (i, elem) in elements.iter().enumerate() {
                            let is_selected = self.selected_element == Some(i);
                            let bg = if is_selected {
                                egui::Frame::group(ui.style())
                            } else {
                                egui::Frame::none()
                            };
                            bg.show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    // Up/Down reorder
                                    let can_up = i > 0;
                                    let can_down = i + 1 < count;
                                    if ui.add_enabled(can_up, egui::Button::new("Up").small()).clicked() {
                                        to_swap = Some((i - 1, i));
                                    }
                                    if ui.add_enabled(can_down, egui::Button::new("Dn").small()).clicked() {
                                        to_swap = Some((i, i + 1));
                                    }
                                    // Type icon + name
                                    let icon = match elem {
                                        crate::app::SourceElement::File(_) => "[F]",
                                        crate::app::SourceElement::Text(_) => "[T]",
                                    };
                                    let label = format!("{} {}", icon, elem.name());
                                    if ui.selectable_label(is_selected, label).clicked() {
                                        to_select = if is_selected { Some(None) } else { Some(Some(i)) };
                                    }
                                    // Remove
                                    if ui.small_button("×").clicked() {
                                        to_remove = Some(i);
                                    }
                                });

                                // Per-element controls
                                if is_selected {
                                    match elem {
                                        crate::app::SourceElement::File(ref fs) => {
                                            ui.horizontal(|ui| {
                                                ui.label("Scale:");
                                                let mut scale = fs.scale;
                                                ui.add(egui::DragValue::new(&mut scale)
                                                    .speed(0.01).range(0.01..=100.0)
                                                    .suffix("x").prefix(""));
                                                let pct = (scale * 100.0) as i32;
                                                ui.label(format!("{}%", pct));
                                                if ui.button("−").clicked() {
                                                    scale = (scale - 0.05).max(0.01);
                                                }
                                                if ui.button("+").clicked() {
                                                    scale = (scale + 0.05).min(100.0);
                                                }
                                                if ui.button("R").clicked() {
                                                    scale = 1.0;
                                                }
                                                if scale != fs.scale {
                                                    self.state.save_snapshot();
                                                    self.state.set_file_scale(i, scale);
                                                    self.needs_zoom_fit = true;
                                                }
                                            });
                                            ui.horizontal(|ui| {
                                                let mut scale = fs.scale;
                                                ui.add(egui::Slider::new(&mut scale, 0.01..=100.0)
                                                    .step_by(0.01));
                                                if scale != fs.scale {
                                                    self.state.save_snapshot();
                                                    self.state.set_file_scale(i, scale);
                                                    self.needs_zoom_fit = true;
                                                }
                                            });
                                            if fs.path_offset.x != 0.0 || fs.path_offset.y != 0.0 {
                                                ui.label(format!("Offset: ({:.0}, {:.0})",
                                                    fs.path_offset.x, fs.path_offset.y));
                                            }
                                            ui.label(format!("Paths: {}", fs.paths.len()));
                                        }
                                        crate::app::SourceElement::Text(ref te) => {
                                            ui.horizontal(|ui| {
                                                ui.label("Size:");
                                                if ui.button("−").clicked() {
                                                    let sz = te.config.font_size;
                                                    let new_size = ((sz - 1.0) * 10.0).round() / 10.0;
                                                    let new_size = new_size.max(1.0);
                                                    if (new_size - sz).abs() > 0.01 {
                                                        self.state.save_snapshot();
                                                        self.state.set_text_font_size(i, new_size);
                                                        self.needs_zoom_fit = true;
                                                    }
                                                }
                                                let sz = te.config.font_size;
                                                let pct = (sz / 10.0 * 100.0) as i32;
                                                ui.label(format!("{}%", pct));
                                                if ui.button("+").clicked() {
                                                    let sz = te.config.font_size;
                                                    let new_size = ((sz + 1.0) * 10.0).round() / 10.0;
                                                    let new_size = new_size.min(500.0);
                                                    if (new_size - sz).abs() > 0.01 {
                                                        self.state.save_snapshot();
                                                        self.state.set_text_font_size(i, new_size);
                                                        self.needs_zoom_fit = true;
                                                    }
                                                }
                                                ui.label(format!("({:.1} mm)", sz));
                                            });
                                            if te.path_offset.x != 0.0 || te.path_offset.y != 0.0 {
                                                ui.label(format!("Offset: ({:.0}, {:.0})",
                                                    te.path_offset.x, te.path_offset.y));
                                            }
                                            ui.horizontal(|ui| {
                                                if ui.button("Edit").clicked() {
                                                    self.dialog_text = te.content.clone();
                                                    self.dialog_font_size = te.config.font_size;
                                                    self.dialog_force_hershey = te.config.force_hershey;
                                                    self.dialog_fill_enabled = te.config.fill_enabled;
                                                    self.dialog_fill_spacing = te.config.fill_spacing;
                                                    self.dialog_font_path = te.config.font_path.clone()
                                                        .unwrap_or_default();
                                                    self.editing_element_index = Some(i);
                                                    self.show_text_dialog = true;
                                                }
                                            });
                                        }
                                    }
                                    // Visibility toggle
                                    let visible = elem.is_visible();
                                    if ui.button(if visible { "Hide" } else { "Show" }).clicked() {
                                        self.state.save_snapshot();
                                        self.state.set_visible(i, !visible);
                                        self.needs_zoom_fit = true;
                                    }
                                }
                            });
                            ui.separator();
                        }

                        // Apply deferred actions
                        if let Some(idx) = to_remove {
                            self.state.save_snapshot();
                            self.state.remove_element(idx);
                            if self.selected_element == Some(idx) {
                                self.selected_element = None;
                            } else if let Some(sel) = self.selected_element {
                                if sel > idx {
                                    self.selected_element = Some(sel - 1);
                                }
                            }
                            self.needs_zoom_fit = true;
                        }
                        if let Some((i, j)) = to_swap {
                            self.state.save_snapshot();
                            self.state.swap_elements(i, j);
                            if self.selected_element == Some(i) {
                                self.selected_element = Some(j);
                            } else if self.selected_element == Some(j) {
                                self.selected_element = Some(i);
                            }
                            self.needs_zoom_fit = true;
                        }
                        if let Some(sel) = to_select {
                            self.selected_element = sel;
                        }
                    }

                    // Add Text button (always visible)
                    ui.add_space(4.0);
                    if ui.button("+ Add Text").clicked() {
                        self.editing_element_index = None;
                        self.dialog_font_size = 10.0;
                        self.dialog_text.clear();
                        self.dialog_font_path.clear();
                        self.dialog_force_hershey = false;
                        self.dialog_fill_enabled = false;
                        self.dialog_fill_spacing = 0.5;
                        self.show_text_dialog = true;
                    }
                });
            });

        // Bottom panel: stream log
        if !self.stream_log.is_empty() {
            egui::TopBottomPanel::bottom("stream_log")
                .resizable(true)
                .default_height(120.0)
                .show(ctx, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for line in self.stream_log.iter().rev().take(50) {
                            ui.label(line);
                        }
                    });
                });
        }

        // Central panel: preview
        egui::CentralPanel::default().show(ctx, |ui| {
            let (response, painter) =
                ui.allocate_painter(ui.available_size(), Sense::click_and_drag());

            let area_min = response.rect.min;
            let area_size = response.rect.size();

            // Pan or Position
            if response.dragged() && !self.state.paths.is_empty() {
                match self.drag_mode {
                    DragMode::Pan => {
                        self.pan += response.drag_delta();
                        self.drag_snapshotted = false;
                    }
                    DragMode::Position => {
                        if !self.drag_snapshotted {
                            self.state.save_snapshot();
                            self.drag_snapshotted = true;
                        }
                        let d = response.drag_delta();
                        let z = self.zoom.max(0.05) as f64;
                        let dx = d.x as f64 / z;
                        let dy = -(d.y as f64 / z);
                        if let Some(idx) = self.selected_element {
                            self.state.move_element(idx, dx, dy);
                        } else {
                            self.state.position_offset.x += dx;
                            self.state.position_offset.y += dy;
                            for p in &mut self.state.paths {
                                p.translate(dx, dy);
                            }
                            self.state.bbox = crate::geometry::BoundingBox::from_paths(&self.state.paths);
                            self.state.gcode_dirty = true;
                        }
                    }
                }
            } else {
                self.drag_snapshotted = false;
            }

            // Zoom with scroll
            if response.hovered() {
                ctx.input(|i| {
                    let scroll = i.raw_scroll_delta.y;
                    if scroll != 0.0 {
                        let old_zoom = self.zoom;
                        self.zoom *= 1.0 + (scroll as f64) * 0.001;
                        self.zoom = self.zoom.clamp(0.05, 50.0);
                        if let Some(pos) = i.pointer.hover_pos() {
                            let cursor = pos - area_min.to_vec2();
                            let factor = self.zoom / old_zoom;
                            self.pan = cursor - (cursor - self.pan) * (factor as f32);
                        }
                    }
                });
            }

            if self.state.paths.is_empty() {
                painter.text(
                    area_min + area_size / 2.0,
                    egui::Align2::CENTER_CENTER,
                    "Load an SVG, PNG, or enter text to preview",
                    egui::FontId::proportional(16.0),
                    Color32::GRAY,
                );
                return;
            }

            let center_x = area_min.x + area_size.x / 2.0 + self.pan.x;
            let center_y = area_min.y + area_size.y / 2.0 + self.pan.y;

            // Draw grid
            let grid_color = Color32::from_gray(40);
            let grid_spacing = (self.zoom * 10.0) as f32;
            if grid_spacing > 5.0 {
                let start_x = center_x.rem_euclid(grid_spacing);
                let mut gx = start_x;
                while gx < area_min.x + area_size.x {
                    painter.line_segment(
                        [Pos2::new(gx, area_min.y), Pos2::new(gx, area_min.y + area_size.y)],
                        Stroke::new(0.5, grid_color),
                    );
                    gx += grid_spacing;
                }
                gx = start_x - grid_spacing;
                while gx > area_min.x {
                    painter.line_segment(
                        [Pos2::new(gx, area_min.y), Pos2::new(gx, area_min.y + area_size.y)],
                        Stroke::new(0.5, grid_color),
                    );
                    gx -= grid_spacing;
                }

                let start_y = center_y.rem_euclid(grid_spacing);
                let mut gy = start_y;
                while gy < area_min.y + area_size.y {
                    painter.line_segment(
                        [Pos2::new(area_min.x, gy), Pos2::new(area_min.x + area_size.x, gy)],
                        Stroke::new(0.5, grid_color),
                    );
                    gy += grid_spacing;
                }
                gy = start_y - grid_spacing;
                while gy > area_min.y {
                    painter.line_segment(
                        [Pos2::new(area_min.x, gy), Pos2::new(area_min.x + area_size.x, gy)],
                        Stroke::new(0.5, grid_color),
                    );
                    gy -= grid_spacing;
                }
            }

            // Draw bed outline
            let zoom_f = self.zoom as f32;
            let bed_w_f = (self.state.gcode_config.bed_width * self.zoom) as f32;
            let bed_h_f = (self.state.gcode_config.bed_height * self.zoom) as f32;
            let bed_left = center_x;
            let bed_top = center_y - bed_h_f;
            let bed_rect = egui::Rect::from_min_size(
                Pos2::new(bed_left, bed_top),
                Vec2::new(bed_w_f, bed_h_f),
            );
            painter.rect_stroke(bed_rect, Rounding::ZERO, Stroke::new(1.0, Color32::from_rgb(80, 80, 80)));
            painter.rect_filled(bed_rect, Rounding::ZERO, Color32::from_black_alpha(20));

            // Draw paths with color cycling
            let colors = [
                Color32::YELLOW,
                Color32::LIGHT_BLUE,
                Color32::LIGHT_GREEN,
                Color32::LIGHT_RED,
                Color32::from_rgb(100, 200, 200),
                Color32::from_rgb(200, 180, 255),
            ];

            for (path_idx, path) in self.state.paths.iter().enumerate() {
                if path.points.len() < 2 {
                    continue;
                }
                let color = colors[path_idx % colors.len()];

                let ox = self.state.position_offset.x as f32;
                let oy = self.state.position_offset.y as f32;
                let points: Vec<Pos2> = path
                    .points
                    .iter()
                    .map(|p| {
                        let sx = center_x + ((p.x as f32 + ox) * zoom_f);
                        let sy = center_y + ((-(p.y as f32 + oy)) * zoom_f);
                        Pos2::new(sx, sy)
                    })
                    .collect();

                painter.add(Shape::line(points, Stroke::new(1.5, color)));
            }

            // Start point marker
            if let Some(first_path) = self.state.paths.first() {
                if let Some(first_pt) = first_path.points.first() {
                    let ox = self.state.position_offset.x as f32;
                    let oy = self.state.position_offset.y as f32;
                    let sx = center_x + ((first_pt.x as f32 + ox) * zoom_f);
                    let sy = center_y + ((-(first_pt.y as f32 + oy)) * zoom_f);
                    painter.circle_filled(Pos2::new(sx, sy), 4.0, Color32::GREEN);
                }
            }

            // Status text
            let drag_hint = match self.drag_mode {
                DragMode::Pan => "Pan mode".to_string(),
                DragMode::Position => {
                    match self.selected_element {
                        Some(i) => format!("Move elem #{} (click empty=global)", i),
                        None => "Move all (click element=per-elem)".to_string(),
                    }
                }
            };
            painter.text(
                Pos2::new(area_min.x + 10.0, area_min.y + area_size.y - 10.0),
                egui::Align2::LEFT_BOTTOM,
                format!(
                    "Zoom: {:.0}%  |  Paths: {}  |  Points: {}  |  {}  |  Scroll to zoom",
                    self.zoom * 100.0,
                    self.state.path_count(),
                    self.state.point_count(),
                    drag_hint,
                ),
                egui::FontId::proportional(12.0),
                Color32::GRAY,
            );
        });

        if self.stream_active {
            ctx.request_repaint();
        }
    }
}

impl PlotterApp {
    fn load_file(&mut self, path: &str) {
        if !self.state.paths.is_empty() {
            self.state.save_snapshot();
        }
        let path_lower = path.to_lowercase();
        let result = if path_lower.ends_with(".svg") {
            self.state.load_svg(path)
        } else if path_lower.ends_with(".png") {
            self.state.load_png(path)
        } else {
            Err(anyhow::anyhow!("Unsupported file format. Use .svg or .png"))
        };

        match result {
            Ok(()) => {
                self.state.auto_fit();
                self.needs_zoom_fit = true;
            }
            Err(e) => {
                self.error_message = format!("Failed to load: {}", e);
                self.show_error = true;
            }
        }
    }

    fn export_gcode(&mut self, path: &str) {
        if let Err(e) = self.state.export_gcode(path) {
            self.error_message = format!("Export failed: {}", e);
            self.show_error = true;
        }
    }

    fn start_streaming(&mut self) {
        if self.selected_port.is_empty() {
            self.error_message = "No serial port selected.".to_string();
            self.show_error = true;
            return;
        }

        self.state.refresh_gcode();
        if self.state.gcode.is_empty() {
            self.error_message = "No G-code to stream.".to_string();
            self.show_error = true;
            return;
        }

        match serial::open_port(&self.selected_port, self.baud_rate) {
            Ok(port) => {
                let cancel = Arc::new(AtomicBool::new(false));
                let rx = serial::stream_gcode(port, &self.state.gcode, cancel.clone());
                self.stream_cancel = Some(cancel);
                self.stream_rx = Some(rx);
                self.stream_active = true;
                self.stream_progress = (0, 0);
                self.stream_log.clear();
                self.stream_log.push(format!("Connected to {}", self.selected_port));
            }
            Err(e) => {
                self.error_message = format!("Failed to open port: {}", e);
                self.show_error = true;
            }
        }
    }

    fn stop_streaming(&mut self) {
        if let Some(ref cancel) = self.stream_cancel {
            cancel.store(true, Ordering::Relaxed);
        }
        self.stream_active = false;
        self.stream_log.push("Streaming stopped.".to_string());
    }

    fn poll_serial(&mut self) {
        if let Some(ref rx) = self.stream_rx {
            loop {
                match rx.try_recv() {
                    Ok(serial::SerialEvent::LineSent { line, line_number, total }) => {
                        self.stream_progress = (line_number, total);
                        if !line.is_empty() && !line.starts_with(';') {
                            self.stream_log.push(format!("[{}/{}] {}", line_number, total, line));
                        }
                    }
                    Ok(serial::SerialEvent::Error(err)) => {
                        self.stream_log.push(format!("Error: {}", err));
                        self.stream_active = false;
                    }
                    Ok(serial::SerialEvent::Complete) => {
                        self.stream_log.push("Streaming complete!".to_string());
                        self.stream_active = false;
                        self.state.status = "Streaming complete!".to_string();
                    }
                    Ok(serial::SerialEvent::Connected) => {
                        self.stream_log.push("Connected.".to_string());
                    }
                    Ok(serial::SerialEvent::Disconnected) => {
                        self.stream_log.push("Disconnected.".to_string());
                        self.stream_active = false;
                    }
                    Ok(serial::SerialEvent::Response(resp)) => {
                        if resp != "ok" {
                            self.stream_log.push(format!("<< {}", resp));
                        }
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        self.stream_active = false;
                        break;
                    }
                }
            }
        }
    }
}

fn open_file_dialog(extension: &str) -> Option<String> {
    #[cfg(feature = "gui")]
    {
        use rfd::FileDialog;
        let mut dialog = FileDialog::new();
        match extension {
            "svg" => dialog = dialog.add_filter("SVG Files", &["svg"]),
            "png" => dialog = dialog.add_filter("PNG Images", &["png"]),
            "ttf" => dialog = dialog.add_filter("Font Files", &["ttf", "otf"]),
            "pcp" => dialog = dialog.add_filter("PlotterControll Project", &["pcp"]),
            _ => {}
        }
        dialog.pick_file().map(|p| p.to_string_lossy().to_string())
    }
    #[cfg(not(feature = "gui"))]
    {
        let _ = extension;
        None
    }
}

fn save_file_dialog(extension: &str, filter_name: &str) -> Option<String> {
    #[cfg(feature = "gui")]
    {
        use rfd::FileDialog;
        let mut dialog = FileDialog::new();
        if !extension.is_empty() {
            dialog = dialog.add_filter(filter_name, &[extension]);
        }
        dialog.save_file()
            .map(|p| p.to_string_lossy().to_string())
    }
    #[cfg(not(feature = "gui"))]
    {
        let _ = (extension, filter_name);
        None
    }
}
