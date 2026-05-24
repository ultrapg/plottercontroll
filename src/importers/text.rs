use crate::geometry::{PathData, Point};
use crate::hershey;
use std::path::PathBuf;
use ttf_parser::{Face, GlyphId, OutlineBuilder};

/// Configuration for text import.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TextImportConfig {
    pub font_path: Option<String>,
    pub font_size: f64,
    pub letter_spacing: f64,
    pub force_hershey: bool,
    pub center: bool,
    pub fill_enabled: bool,
    pub fill_spacing: f64,
}

impl Default for TextImportConfig {
    fn default() -> Self {
        TextImportConfig {
            font_path: find_system_font(),
            font_size: 10.0,
            letter_spacing: 1.0,
            force_hershey: false,
            center: false,
            fill_enabled: false,
            fill_spacing: 0.5,
        }
    }
}

/// Search common system font directories for a usable TTF/OTF font.
/// Returns the first found font path, or None.
#[cfg(target_os = "linux")]
pub fn find_system_font() -> Option<String> {
    let candidates = [
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
        "/usr/share/fonts/truetype/ubuntu/Ubuntu-R.ttf",
        "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
        "/usr/share/fonts/opentype/noto/NotoSans-Regular.otf",
        "/usr/share/fonts/truetype/freefont/FreeSans.ttf",
        "/usr/share/fonts/truetype/ttf-dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/TTF/DejaVuSans.ttf",
        "/usr/share/fonts/truetype/arphic/uming.ttc",
        "/usr/share/fonts/wqy-microhei/wqy-microhei.ttc",
    ];
    for path in &candidates {
        let p = PathBuf::from(path);
        if p.exists() {
            log::info!("Found system font: {}", path);
            return Some(path.to_string());
        }
    }
    // Recursive search in standard font directories
    for dir in &["/usr/share/fonts", "/usr/local/share/fonts"] {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    let name = path.file_name().unwrap_or_default().to_string_lossy();
                    let lower = name.to_lowercase();
                    if lower.contains("dejavu") && lower.contains("sans") && lower.ends_with(".ttf") {
                        log::info!("Found system font: {}", path.display());
                        return Some(path.to_string_lossy().to_string());
                    }
                }
            }
        }
    }
    log::warn!("No system font found. Hershey single-line font will be used.");
    None
}

#[cfg(target_os = "windows")]
pub fn find_system_font() -> Option<String> {
    let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".to_string());
    let font_dir = format!("{}\\Fonts", windir);
    let candidates = [
        format!("{}\\arial.ttf", font_dir),
        format!("{}\\segoeui.ttf", font_dir),
        format!("{}\\tahoma.ttf", font_dir),
        format!("{}\\calibri.ttf", font_dir),
    ];
    for path in &candidates {
        let p = PathBuf::from(path);
        if p.exists() {
            log::info!("Found system font: {}", path);
            return Some(path.clone());
        }
    }
    // Fallback: scan the fonts directory
    if let Ok(entries) = std::fs::read_dir(&font_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                let lower = path.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
                if lower.ends_with(".ttf") || lower.ends_with(".otf") {
                    log::info!("Found system font: {}", path.display());
                    return Some(path.to_string_lossy().to_string());
                }
            }
        }
    }
    log::warn!("No system font found on Windows. Hershey single-line font will be used.");
    None
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
pub fn find_system_font() -> Option<String> {
    None
}

/// Render text to plotter paths.
/// Tries TTF outline first (with auto-detected or user-specified font),
/// then falls back to Hershey single-line font.
pub fn text_to_paths(text: &str, config: &TextImportConfig) -> Vec<PathData> {
    let mut paths = if !config.force_hershey {
        if let Some(ref font_path) = config.font_path {
            match render_ttf_outline(text, font_path, config.font_size, config.letter_spacing) {
                Ok(p) => {
                    log::info!("Text rendered with TTF font: {}", font_path);
                    p
                }
                Err(e) => {
                    log::warn!("TTF rendering failed ({}), falling back to Hershey font", e);
                    hershey_render(text, config)
                }
            }
        } else {
            hershey_render(text, config)
        }
    } else {
        hershey_render(text, config)
    };

    if config.fill_enabled {
        let fill = generate_hatch_fill(&paths, config.fill_spacing);
        paths.extend(fill);
    }

    paths
}

fn hershey_render(text: &str, config: &TextImportConfig) -> Vec<PathData> {
    log::info!("Text rendered with Hershey single-line font");
    let hs = config.font_size / 14.0;
    let mut paths = hershey::render_text(text, 0.0, 0.0, hs, config.letter_spacing);

    if config.center {
        let total_w = hershey::text_width(text, hs, config.letter_spacing);
        for path in &mut paths {
            path.translate(-total_w / 2.0, 0.0);
        }
    }

    paths
}

struct CurveCollector {
    paths: Vec<PathData>,
    current: Vec<Point>,
    scale: f64,
    offset_x: f64,
    offset_y: f64,
}

impl CurveCollector {
    fn new(scale: f64, offset_x: f64, offset_y: f64) -> Self {
        CurveCollector {
            paths: Vec::new(),
            current: Vec::new(),
            scale,
            offset_x,
            offset_y,
        }
    }
}

impl OutlineBuilder for CurveCollector {
    fn move_to(&mut self, x: f32, y: f32) {
        if self.current.len() >= 2 {
            self.paths.push(PathData {
                points: std::mem::take(&mut self.current),
                closed: false,
            });
        }
        self.current.clear();
        self.current.push(Point::new(
            self.offset_x + x as f64 * self.scale,
            self.offset_y + y as f64 * self.scale,
        ));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.current.push(Point::new(
            self.offset_x + x as f64 * self.scale,
            self.offset_y + y as f64 * self.scale,
        ));
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        if self.current.is_empty() {
            return;
        }
        let start = *self.current.last().unwrap();
        let cx1 = self.offset_x + x1 as f64 * self.scale;
        let cy1 = self.offset_y + y1 as f64 * self.scale;
        let cx = self.offset_x + x as f64 * self.scale;
        let cy = self.offset_y + y as f64 * self.scale;
        let steps = 10usize;
        for i in 1..=steps {
            let t = i as f64 / steps as f64;
            let mt = 1.0 - t;
            let px = mt * mt * start.x + 2.0 * mt * t * cx1 + t * t * cx;
            let py = mt * mt * start.y + 2.0 * mt * t * cy1 + t * t * cy;
            self.current.push(Point::new(px, py));
        }
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        if self.current.is_empty() {
            return;
        }
        let start = *self.current.last().unwrap();
        let cx1 = self.offset_x + x1 as f64 * self.scale;
        let cy1 = self.offset_y + y1 as f64 * self.scale;
        let cx2 = self.offset_x + x2 as f64 * self.scale;
        let cy2 = self.offset_y + y2 as f64 * self.scale;
        let cx = self.offset_x + x as f64 * self.scale;
        let cy = self.offset_y + y as f64 * self.scale;
        let steps = 10usize;
        for i in 1..=steps {
            let t = i as f64 / steps as f64;
            let t2 = t * t;
            let t3 = t2 * t;
            let mt = 1.0 - t;
            let mt2 = mt * mt;
            let mt3 = mt2 * mt;
            let px = mt3 * start.x
                + 3.0 * mt2 * t * cx1
                + 3.0 * mt * t2 * cx2
                + t3 * cx;
            let py = mt3 * start.y
                + 3.0 * mt2 * t * cy1
                + 3.0 * mt * t2 * cy2
                + t3 * cy;
            self.current.push(Point::new(px, py));
        }
    }

    fn close(&mut self) {
        if self.current.len() >= 2 {
            self.paths.push(PathData {
                points: std::mem::take(&mut self.current),
                closed: true,
            });
        }
        self.current.clear();
    }
}

/// Render text using a TTF/OTF font via ttf_parser.
fn render_ttf_outline(
    text: &str,
    font_path: &str,
    font_size: f64,
    letter_spacing: f64,
) -> anyhow::Result<Vec<PathData>> {
    let font_data = std::fs::read(font_path)?;
    let face = Face::parse(&font_data, 0)
        .map_err(|_| anyhow::anyhow!("Failed to parse font: {}", font_path))?;

    let units_per_em = face.units_per_em() as f64;
    let scale = font_size / units_per_em;
    let mut all_paths = Vec::new();
    let mut cursor_x = 0.0f64;
    let mut cursor_y = 0.0f64;
    let line_height = font_size * 1.3;

    for ch in text.chars() {
        if ch == '\n' {
            cursor_x = 0.0;
            cursor_y -= line_height;
            continue;
        }
        if ch == ' ' {
            if let Some(advance) = face
                .glyph_hor_advance(face.glyph_index(' ').unwrap_or(GlyphId(0)))
            {
                cursor_x += advance as f64 * scale + letter_spacing;
            } else {
                cursor_x += font_size * 0.3;
            }
            continue;
        }

        let glyph_id = face.glyph_index(ch);
        if glyph_id.is_none() {
            cursor_x += font_size * 0.5;
            continue;
        }
        let glyph_id = glyph_id.unwrap();

        let mut collector = CurveCollector::new(scale, cursor_x, cursor_y);
        let result = face.outline_glyph(glyph_id, &mut collector);

        if result.is_some() && !collector.paths.is_empty() {
            all_paths.extend(collector.paths);
        }

        if let Some(advance) = face.glyph_hor_advance(glyph_id) {
            cursor_x += advance as f64 * scale + letter_spacing;
        } else {
            cursor_x += font_size * 0.5;
        }
    }

    if all_paths.is_empty() {
        anyhow::bail!("No outlines found in text");
    }

    Ok(all_paths)
}

/// Generate hatch fill lines inside closed paths using the global even-odd rule.
/// Collects ALL closed contours together and sorts intersections from all contours,
/// then pairs them in even-odd fashion. This ensures interior holes (inner contours)
/// are correctly excluded — the fill never leaks outside the outer character shapes.
pub fn generate_hatch_fill(paths: &[PathData], spacing: f64) -> Vec<PathData> {
    let mut fill_paths = Vec::new();
    if spacing <= 0.0 {
        return fill_paths;
    }

    let closed: Vec<&PathData> = paths.iter().filter(|p| p.closed && p.points.len() >= 3).collect();
    if closed.is_empty() {
        return fill_paths;
    }

    let min_y = closed.iter()
        .flat_map(|p| p.points.iter().map(|pt| pt.y))
        .fold(f64::MAX, f64::min);
    let max_y = closed.iter()
        .flat_map(|p| p.points.iter().map(|pt| pt.y))
        .fold(f64::NEG_INFINITY, f64::max);

    let mut y = min_y + spacing;
    while y < max_y {
        let mut intersections = Vec::new();

        for path in &closed {
            let n = path.points.len();
            for i in 0..n {
                let j = (i + 1) % n;
                let p1 = &path.points[i];
                let p2 = &path.points[j];
                if (p1.y < y && p2.y >= y) || (p2.y < y && p1.y >= y) {
                    let t = (y - p1.y) / (p2.y - p1.y);
                    let x = p1.x + t * (p2.x - p1.x);
                    intersections.push(x);
                }
            }
        }

        intersections.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        for pair in intersections.chunks_exact(2) {
            let mut seg = PathData::new();
            seg.points.push(Point::new(pair[0], y));
            seg.points.push(Point::new(pair[1], y));
            fill_paths.push(seg);
        }

        y += spacing;
    }

    fill_paths
}

/// Get the name of the default font (for display in GUI).
pub fn default_font_name() -> String {
    if let Some(path) = find_system_font() {
        let path = std::path::Path::new(&path);
        path.file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "System Font".to_string())
    } else {
        "Hershey (single-line)".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_text_to_paths_hershey() {
        let config = TextImportConfig {
            font_path: None,
            force_hershey: true,
            ..Default::default()
        };
        let paths = text_to_paths("TEST", &config);
        assert!(!paths.is_empty());
    }
}
