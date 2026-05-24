use crate::gcode_gen::{export_to_file, generate_gcode, GCodeConfig};
use crate::geometry::{BoundingBox, PathData, Point};
use crate::importers::png::{import_png, PngImportConfig};
use crate::importers::svg::{import_svg, SvgImportConfig};
use crate::importers::text::{text_to_paths, TextImportConfig};
use crate::profiles::{self, DeviceProfile};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum InputType {
    Svg,
    Png,
    Text,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FileSource {
    pub filepath: String,
    pub input_type: InputType,
    pub paths: Vec<PathData>,
    pub path_offset: Point,
    pub scale: f64,
    pub visible: bool,
    #[serde(skip)]
    pub path_start: usize,
    #[serde(skip)]
    pub path_count: usize,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TextElement {
    pub content: String,
    pub config: TextImportConfig,
    pub path_offset: Point,
    pub visible: bool,
    #[serde(skip)]
    pub path_start: usize,
    #[serde(skip)]
    pub path_count: usize,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
pub enum SourceElement {
    #[serde(rename = "file")]
    File(FileSource),
    #[serde(rename = "text")]
    Text(TextElement),
}

impl SourceElement {
    pub fn is_visible(&self) -> bool {
        match self {
            SourceElement::File(fs) => fs.visible,
            SourceElement::Text(te) => te.visible,
        }
    }

    pub fn name(&self) -> String {
        match self {
            SourceElement::File(fs) => {
                std::path::Path::new(&fs.filepath)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| fs.filepath.clone())
            }
            SourceElement::Text(te) => {
                if te.content.len() > 14 {
                    format!("\"{}…\"", &te.content[..14])
                } else {
                    format!("\"{}\"", te.content)
                }
            }
        }
    }

    pub fn type_label(&self) -> &'static str {
        match self {
            SourceElement::File(_) => "File",
            SourceElement::Text(_) => "Text",
        }
    }
}

pub struct AppState {
    pub paths: Vec<PathData>,
    pub source_elements: Vec<SourceElement>,
    pub bbox: BoundingBox,
    pub gcode_config: GCodeConfig,
    pub gcode: String,
    pub png_config: PngImportConfig,
    pub svg_config: SvgImportConfig,
    pub text_config: TextImportConfig,
    pub gcode_dirty: bool,
    pub error: Option<String>,
    pub status: String,
    pub position_offset: Point,
    pub undo_stack: Vec<StateSnapshot>,
    pub redo_stack: Vec<StateSnapshot>,
    pub dragging: bool,
    pub profiles: Vec<DeviceProfile>,
    pub profile_name: String,
}

#[derive(Clone)]
pub struct StateSnapshot {
    pub paths: Vec<PathData>,
    pub source_elements: Vec<SourceElement>,
    pub bbox: BoundingBox,
    pub position_offset: Point,
    pub status: String,
    pub gcode: String,
    pub gcode_dirty: bool,
}

const MAX_UNDO: usize = 50;

impl AppState {
    pub fn save_snapshot(&mut self) {
        self.undo_stack.push(StateSnapshot {
            paths: self.paths.clone(),
            source_elements: self.source_elements.clone(),
            bbox: self.bbox,
            position_offset: self.position_offset,
            status: self.status.clone(),
            gcode: self.gcode.clone(),
            gcode_dirty: self.gcode_dirty,
        });
        if self.undo_stack.len() > MAX_UNDO {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
    }

    pub fn undo(&mut self) {
        let Some(snap) = self.undo_stack.pop() else { return };
        self.redo_stack.push(StateSnapshot {
            paths: std::mem::take(&mut self.paths),
            source_elements: std::mem::take(&mut self.source_elements),
            bbox: self.bbox,
            position_offset: self.position_offset,
            status: self.status.clone(),
            gcode: std::mem::take(&mut self.gcode),
            gcode_dirty: self.gcode_dirty,
        });
        self.paths = snap.paths;
        self.source_elements = snap.source_elements;
        self.bbox = snap.bbox;
        self.position_offset = snap.position_offset;
        self.status = snap.status;
        self.gcode = snap.gcode;
        self.gcode_dirty = snap.gcode_dirty;
        self.dragging = false;
    }

    pub fn redo(&mut self) {
        let Some(snap) = self.redo_stack.pop() else { return };
        self.undo_stack.push(StateSnapshot {
            paths: std::mem::take(&mut self.paths),
            source_elements: std::mem::take(&mut self.source_elements),
            bbox: self.bbox,
            position_offset: self.position_offset,
            status: self.status.clone(),
            gcode: std::mem::take(&mut self.gcode),
            gcode_dirty: self.gcode_dirty,
        });
        self.paths = snap.paths;
        self.source_elements = snap.source_elements;
        self.bbox = snap.bbox;
        self.position_offset = snap.position_offset;
        self.status = snap.status;
        self.gcode = snap.gcode;
        self.gcode_dirty = snap.gcode_dirty;
        self.dragging = false;
    }

    pub fn text_count(&self) -> usize {
        self.source_elements.iter().filter(|e| matches!(e, SourceElement::Text(_))).count()
    }

    pub fn file_count(&self) -> usize {
        self.source_elements.iter().filter(|e| matches!(e, SourceElement::File(_))).count()
    }
}

impl Default for AppState {
    fn default() -> Self {
        let profiles = profiles::builtin_profiles();
        let (gcode_config, profile_name) = if let Some(first) = profiles.first() {
            (first.config.clone(), first.name.clone())
        } else {
            (GCodeConfig::default(), profiles::default_profile_name().to_string())
        };
        AppState {
            paths: Vec::new(),
            source_elements: Vec::new(),
            bbox: BoundingBox::empty(),
            gcode_config,
            gcode: String::new(),
            png_config: PngImportConfig::default(),
            svg_config: SvgImportConfig::default(),
            text_config: TextImportConfig::default(),
            gcode_dirty: true,
            error: None,
            status: "Ready".to_string(),
            position_offset: Point::new(0.0, 0.0),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            dragging: false,
            profiles,
            profile_name,
        }
    }
}

impl AppState {
    pub fn rebuild_all_paths(&mut self) {
        self.paths.clear();
        for elem in &mut self.source_elements {
            if !elem.is_visible() {
                match elem {
                    SourceElement::File(fs) => { fs.path_start = 0; fs.path_count = 0; }
                    SourceElement::Text(te) => { te.path_start = 0; te.path_count = 0; }
                }
                continue;
            }
            match elem {
                SourceElement::File(fs) => {
                    fs.path_start = self.paths.len();
                    fs.path_count = fs.paths.len();
                    for p in &fs.paths {
                        let mut c = p.clone();
                        if fs.scale != 1.0 {
                            c.scale(fs.scale, fs.scale);
                        }
                        if fs.path_offset.x != 0.0 || fs.path_offset.y != 0.0 {
                            c.translate(fs.path_offset.x, fs.path_offset.y);
                        }
                        self.paths.push(c);
                    }
                }
                SourceElement::Text(te) => {
                    let raw = text_to_paths(&te.content, &te.config);
                    if raw.is_empty() {
                        te.path_start = 0;
                        te.path_count = 0;
                        continue;
                    }
                    let stack = if self.paths.is_empty() {
                        Point::new(0.0, 0.0)
                    } else {
                        let current_bbox = BoundingBox::from_paths(&self.paths);
                        let text_bbox = BoundingBox::from_paths(&raw);
                        let gap = 3.0;
                        let dy = current_bbox.min_y - text_bbox.max_y - gap;
                        Point::new(0.0, dy)
                    };
                    te.path_start = self.paths.len();
                    te.path_count = raw.len();
                    for p in raw {
                        let mut c = p;
                        let tx = stack.x + te.path_offset.x;
                        let ty = stack.y + te.path_offset.y;
                        if tx != 0.0 || ty != 0.0 {
                            c.translate(tx, ty);
                        }
                        self.paths.push(c);
                    }
                }
            }
        }
        self.bbox = BoundingBox::from_paths(&self.paths);
        self.gcode_dirty = true;
    }

    pub fn add_text(&mut self, text: &str, config: TextImportConfig) -> usize {
        let raw = text_to_paths(text, &config);
        if raw.is_empty() {
            self.error = Some("No paths generated from text".to_string());
            return usize::MAX;
        }
        let idx = self.source_elements.len();
        self.source_elements.push(SourceElement::Text(TextElement {
            content: text.to_string(),
            config,
            path_offset: Point::new(0.0, 0.0),
            visible: true,
            path_start: 0,
            path_count: 0,
        }));
        self.rebuild_all_paths();
        self.status = format!("Added text '{}'", text);
        log::info!("Added text element {}: '{}'", idx, text);
        idx
    }

    pub fn edit_text(&mut self, idx: usize, text: &str, config: TextImportConfig) {
        let Some(elem) = self.source_elements.get_mut(idx) else {
            self.error = Some("Invalid element index".to_string());
            return;
        };
        match elem {
            SourceElement::Text(ref mut te) => {
                te.content = text.to_string();
                te.config = config;
            }
            _ => {
                self.error = Some("Element is not a text element".to_string());
                return;
            }
        }
        self.rebuild_all_paths();
        self.status = format!("Edited text element {}", idx);
        log::info!("Edited text element {}", idx);
    }

    pub fn remove_element(&mut self, idx: usize) {
        if idx >= self.source_elements.len() {
            self.error = Some("Invalid element index".to_string());
            return;
        }
        self.source_elements.remove(idx);
        self.rebuild_all_paths();
        self.status = format!("Removed element {}", idx);
        log::info!("Removed element {}", idx);
    }

    pub fn move_element(&mut self, idx: usize, dx: f64, dy: f64) {
        let Some(elem) = self.source_elements.get_mut(idx) else { return };
        let (start, count) = match elem {
            SourceElement::File(ref mut fs) => {
                fs.path_offset.x += dx;
                fs.path_offset.y += dy;
                (fs.path_start, fs.path_count)
            }
            SourceElement::Text(ref mut te) => {
                te.path_offset.x += dx;
                te.path_offset.y += dy;
                (te.path_start, te.path_count)
            }
        };
        for p in self.paths.iter_mut().skip(start).take(count) {
            p.translate(dx, dy);
        }
        self.bbox = BoundingBox::from_paths(&self.paths);
        self.gcode_dirty = true;
    }

    pub fn set_file_scale(&mut self, idx: usize, scale: f64) {
        let Some(elem) = self.source_elements.get_mut(idx) else { return };
        match elem {
            SourceElement::File(ref mut fs) => {
                fs.scale = scale.max(0.01).min(100.0);
            }
            _ => return,
        }
        self.rebuild_all_paths();
    }

    pub fn set_text_font_size(&mut self, idx: usize, font_size: f64) {
        let Some(elem) = self.source_elements.get_mut(idx) else { return };
        match elem {
            SourceElement::Text(ref mut te) => {
                te.config.font_size = font_size.max(0.1).min(500.0);
            }
            _ => return,
        }
        self.rebuild_all_paths();
    }

    pub fn set_visible(&mut self, idx: usize, visible: bool) {
        let Some(elem) = self.source_elements.get_mut(idx) else { return };
        match elem {
            SourceElement::File(ref mut fs) => fs.visible = visible,
            SourceElement::Text(ref mut te) => te.visible = visible,
        }
        self.rebuild_all_paths();
    }

    pub fn swap_elements(&mut self, i: usize, j: usize) {
        if i >= self.source_elements.len() || j >= self.source_elements.len() {
            return;
        }
        self.source_elements.swap(i, j);
        self.rebuild_all_paths();
    }

    pub fn add_file_source(&mut self, paths: Vec<PathData>, filepath: &str, input_type: InputType) -> usize {
        if paths.is_empty() {
            self.error = Some("No paths in file source".to_string());
            return usize::MAX;
        }
        let idx = self.source_elements.len();
        self.source_elements.push(SourceElement::File(FileSource {
            filepath: filepath.to_string(),
            input_type,
            paths,
            path_offset: Point::new(0.0, 0.0),
            scale: 1.0,
            visible: true,
            path_start: 0,
            path_count: 0,
        }));
        self.rebuild_all_paths();
        self.status = format!("Added file: {}", filepath);
        log::info!("Added file source {}: {}", idx, filepath);
        idx
    }

    pub fn apply_profile(&mut self, name: &str) {
        for p in &self.profiles {
            if p.name == name {
                self.gcode_config = p.config.clone();
                self.profile_name = p.name.clone();
                self.gcode_dirty = true;
                self.status = format!("Device profile: {}", name);
                return;
            }
        }
        if name == profiles::default_profile_name() {
            self.gcode_config = GCodeConfig::default();
            self.profile_name = profiles::default_profile_name().to_string();
            self.gcode_dirty = true;
            self.status = "Device profile: Custom".to_string();
        }
    }

    pub fn clear(&mut self) {
        self.paths.clear();
        self.source_elements.clear();
        self.bbox = BoundingBox::empty();
        self.gcode = String::new();
        self.gcode_dirty = true;
        self.error = None;
        self.status = "Cleared".to_string();
        self.position_offset = Point::new(0.0, 0.0);
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.dragging = false;
    }

    pub fn auto_fit(&mut self) {
        if self.paths.is_empty() {
            return;
        }
        let margin = 5.0;
        let (scale, offset_x, offset_y) = self.bbox.fit_to(
            self.gcode_config.bed_width,
            self.gcode_config.bed_height,
            margin,
        );
        for path in &mut self.paths {
            path.scale(scale, scale);
            path.translate(offset_x, offset_y);
        }
        self.bbox = BoundingBox::from_paths(&self.paths);
        self.gcode_dirty = true;
        self.status = format!("Auto-fit: scale={:.3}, offset=({:.1}, {:.1})", scale, offset_x, offset_y);
    }

    pub fn load_svg(&mut self, filepath: &str) -> anyhow::Result<()> {
        let paths = import_svg(filepath, &self.svg_config)?;
        if paths.is_empty() {
            anyhow::bail!("No paths found in SVG file");
        }
        self.source_elements.clear();
        self.add_file_source(paths, filepath, InputType::Svg);
        Ok(())
    }

    pub fn load_png(&mut self, filepath: &str) -> anyhow::Result<()> {
        let paths = import_png(filepath, &self.png_config)?;
        if paths.is_empty() {
            anyhow::bail!("No paths extracted from PNG image");
        }
        self.source_elements.clear();
        self.add_file_source(paths, filepath, InputType::Png);
        Ok(())
    }

    pub fn load_text(&mut self, text: &str, config: Option<TextImportConfig>) {
        let config = config.unwrap_or_default();
        self.source_elements.clear();
        self.add_text(text, config);
    }

    pub fn refresh_gcode(&mut self) {
        if self.gcode_dirty {
            if self.position_offset.x != 0.0 || self.position_offset.y != 0.0 {
                let offset = self.position_offset;
                let paths: Vec<PathData> = self.paths.iter().map(|p| {
                    let mut p = p.clone();
                    p.translate(offset.x, offset.y);
                    p
                }).collect();
                self.gcode = generate_gcode(&paths, &self.gcode_config);
            } else {
                self.gcode = generate_gcode(&self.paths, &self.gcode_config);
            }
            self.gcode_dirty = false;
        }
    }

    pub fn export_gcode(&mut self, filepath: &str) -> anyhow::Result<()> {
        self.refresh_gcode();
        export_to_file(&self.paths, &self.gcode_config, filepath)?;
        self.status = format!("Exported G-code to {}", filepath);
        Ok(())
    }

    pub fn font_size_for_source(&self) -> f64 {
        self.source_elements.iter()
            .rev()
            .find_map(|e| match e {
                SourceElement::Text(te) => Some(te.config.font_size),
                _ => None,
            })
            .unwrap_or(10.0)
    }

    pub fn get_gcode(&mut self) -> &str {
        self.refresh_gcode();
        &self.gcode
    }

    pub fn path_count(&self) -> usize {
        self.paths.len()
    }

    pub fn point_count(&self) -> usize {
        self.paths.iter().map(|p| p.points.len()).sum()
    }

    pub fn estimated_length(&self) -> f64 {
        self.paths
            .iter()
            .flat_map(|p| p.points.windows(2))
            .map(|w| w[0].distance_to(w[1]))
            .sum()
    }
}
