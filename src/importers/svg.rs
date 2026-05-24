use crate::geometry::{PathData, Point, rdp_simplify};
use crate::importers::text::generate_hatch_fill;

/// Configuration for SVG import.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SvgImportConfig {
    /// Fill closed contours with hatch lines.
    pub fill_enabled: bool,
    /// Spacing between hatch lines (mm).
    pub fill_spacing: f64,
    /// Render SVG to raster and apply threshold (instead of vector extraction).
    pub use_raster: bool,
    /// DPI for raster rendering.
    pub raster_dpi: f64,
    /// Threshold value (0 = auto/Otsu) for raster mode.
    pub threshold_value: u8,
    /// Simplify tolerance in pixels for raster mode.
    pub simplify_epsilon: f64,
    /// Minimum contour length in points for raster mode.
    pub min_contour_len: usize,
}

impl Default for SvgImportConfig {
    fn default() -> Self {
        SvgImportConfig {
            fill_enabled: false,
            fill_spacing: 0.5,
            use_raster: false,
            raster_dpi: 300.0,
            threshold_value: 0,
            simplify_epsilon: 1.5,
            min_contour_len: 10,
        }
    }
}

/// Parse an SVG file and extract paths.
pub fn import_svg(filepath: &str, config: &SvgImportConfig) -> anyhow::Result<Vec<PathData>> {
    let svg_data = std::fs::read(filepath)?;
    let tree = usvg::Tree::from_data(&svg_data, &usvg::Options::default())?;

    let mut paths = if config.use_raster {
        import_svg_raster(&tree, config)?
    } else {
        import_svg_vector(&tree)?
    };

    if config.fill_enabled && config.fill_spacing > 0.0 {
        let closed = paths.iter().filter(|p| p.closed && p.points.len() >= 3).count();
        log::info!("SVG fill: {} closed paths eligible, spacing={} mm", closed, config.fill_spacing);
        let fill = generate_hatch_fill(&paths, config.fill_spacing);
        if !fill.is_empty() {
            log::info!("SVG fill: {} hatch lines generated", fill.len());
            paths.extend(fill);
        } else {
            log::warn!("SVG fill: no hatch lines generated");
        }
    }

    log::info!("SVG import: total {} paths", paths.len());
    Ok(paths)
}

/// Vector extraction: extract sub-paths from the SVG tree directly.
fn import_svg_vector(tree: &usvg::Tree) -> anyhow::Result<Vec<PathData>> {
    let mut paths = Vec::new();
    process_group(tree.root(), &mut paths, usvg::Transform::default())?;
    log::info!("SVG vector: extracted {} paths", paths.len());
    Ok(paths)
}

/// Raster extraction: render SVG to bitmap, threshold, trace contours.
fn import_svg_raster(tree: &usvg::Tree, config: &SvgImportConfig) -> anyhow::Result<Vec<PathData>> {
    let size = tree.size();
    let scale = config.raster_dpi / 25.4; // DPI → pixels per mm
    let w = (size.width() as f64 * scale).ceil() as u32;
    let h = (size.height() as f64 * scale).ceil() as u32;
    if w == 0 || h == 0 {
        anyhow::bail!("SVG has zero size");
    }

    let mut pixmap = resvg::tiny_skia::Pixmap::new(w, h)
        .ok_or_else(|| anyhow::anyhow!("Failed to create pixmap {}x{}", w, h))?;

    // Render SVG to pixmap with white background
    pixmap.fill(resvg::tiny_skia::Color::WHITE);
    resvg::render(tree, resvg::tiny_skia::Transform::default(),
        &mut pixmap.as_mut());

    // Convert to grayscale image
    let gray = pixmap_to_gray(&pixmap);

    // Apply threshold
    let binary = if config.threshold_value > 0 {
        binarize_fixed(&gray, config.threshold_value)
    } else {
        binarize_otsu(&gray)
    };

    // Invert so dark foreground is white
    let inverted = invert_binary(&binary);

    // Trace contours (same logic as extract_monochrome in png.rs)
    let paths = trace_contours(&inverted, config.min_contour_len, config.simplify_epsilon);
    log::info!("SVG raster: {}x{} at {:.0} DPI → {} contours", w, h, config.raster_dpi, paths.len());

    // Scale from pixel coords to mm
    let pixel_to_mm = 1.0 / scale;
    let result: Vec<PathData> = paths.into_iter().map(|mut p| {
        for pt in &mut p.points {
            pt.x *= pixel_to_mm;
            pt.y = -pt.y * pixel_to_mm; // flip Y
        }
        p
    }).collect();

    Ok(result)
}

/// Convert a resvg tiny_skia Pixmap to grayscale image.
fn pixmap_to_gray(pixmap: &resvg::tiny_skia::Pixmap) -> image::GrayImage {
    let (w, h) = (pixmap.width(), pixmap.height());
    let mut gray = image::GrayImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let px = pixmap.pixel(x, y).unwrap();
            let r = px.red() as f32;
            let g = px.green() as f32;
            let b = px.blue() as f32;
            let luma = (0.2126 * r + 0.7152 * g + 0.0722 * b) as u8;
            gray.put_pixel(x, y, image::Luma([luma]));
        }
    }
    gray
}

/// Trace contours from a binary image.
fn trace_contours(binary: &image::GrayImage, min_len: usize, epsilon: f64) -> Vec<PathData> {
    let contours = imageproc::contours::find_contours::<i32>(binary);
    let mut paths = Vec::new();
    for contour in &contours {
        let pts: Vec<Point> = contour.points.iter()
            .map(|p| Point::new(p.x as f64, p.y as f64))
            .collect();
        if pts.len() < min_len { continue; }
        let simplified = rdp_simplify(&pts, epsilon);
        if simplified.len() >= 2 {
            paths.push(PathData { points: simplified, closed: true });
        }
    }
    paths
}

// Reusable binarization helpers (shared with png.rs logic)

fn binarize_otsu(gray: &image::GrayImage) -> image::GrayImage {
    let (w, h) = gray.dimensions();
    let total_pixels = (w * h) as usize;
    if total_pixels == 0 { return gray.clone(); }

    let mut hist = [0u64; 256];
    for pixel in gray.pixels() {
        hist[pixel.0[0] as usize] += 1;
    }

    let mut sum_b = 0.0f64;
    let mut w_b = 0.0f64;
    let mut max_variance = 0.0f64;
    let mut threshold = 0u8;
    let total = total_pixels as f64;
    let sum_all: f64 = hist.iter().enumerate().map(|(i, &c)| i as f64 * c as f64).sum();

    for t in 0..256 {
        w_b += hist[t] as f64;
        if w_b == 0.0 { continue; }
        let w_f = total - w_b;
        if w_f == 0.0 { break; }
        sum_b += t as f64 * hist[t] as f64;
        let m_b = sum_b / w_b;
        let m_f = (sum_all - sum_b) / w_f;
        let variance = w_b * w_f * (m_b - m_f) * (m_b - m_f);
        if variance > max_variance {
            max_variance = variance;
            threshold = t as u8;
        }
    }

    let mut out = image::GrayImage::new(w, h);
    for (x, y, p) in gray.enumerate_pixels() {
        out.put_pixel(x, y, image::Luma([if p.0[0] > threshold { 255 } else { 0 }]));
    }
    out
}

fn binarize_fixed(gray: &image::GrayImage, threshold: u8) -> image::GrayImage {
    let (w, h) = gray.dimensions();
    let mut out = image::GrayImage::new(w, h);
    for (x, y, p) in gray.enumerate_pixels() {
        out.put_pixel(x, y, image::Luma([if p.0[0] > threshold { 255 } else { 0 }]));
    }
    out
}

fn invert_binary(binary: &image::GrayImage) -> image::GrayImage {
    let (w, h) = binary.dimensions();
    let mut out = image::GrayImage::new(w, h);
    for (x, y, p) in binary.enumerate_pixels() {
        out.put_pixel(x, y, image::Luma([if p.0[0] > 128 { 0 } else { 255 }]));
    }
    out
}

// Vector extraction helpers (unchanged from original)

fn process_group(
    group: &usvg::Group,
    paths: &mut Vec<PathData>,
    parent_transform: usvg::Transform,
) -> anyhow::Result<()> {
    let transform = parent_transform.pre_concat(group.abs_transform());
    for child in group.children() {
        extract_paths(child, paths, transform)?;
    }
    Ok(())
}

fn extract_paths(
    node: &usvg::Node,
    paths: &mut Vec<PathData>,
    transform: usvg::Transform,
) -> anyhow::Result<()> {
    match node {
        usvg::Node::Group(ref group) => process_group(group.as_ref(), paths, transform),
        usvg::Node::Path(ref path_node) => {
            if let Some(sub_paths) = extract_sub_paths(path_node, &transform) {
                paths.extend(sub_paths);
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn extract_sub_paths(path_node: &usvg::Path, transform: &usvg::Transform) -> Option<Vec<PathData>> {
    use usvg::tiny_skia_path::PathSegment;

    let skia_path = path_node.data();
    let mut result = Vec::new();
    let mut current = Vec::new();

    for segment in skia_path.segments() {
        match segment {
            PathSegment::MoveTo(pt) => {
                if current.len() >= 2 {
                    let transformed = apply_transform_f64(&current, transform);
                    result.push(PathData { points: transformed, closed: false });
                }
                current.clear();
                current.push(Point::new(pt.x as f64, pt.y as f64));
            }
            PathSegment::LineTo(pt) => {
                current.push(Point::new(pt.x as f64, pt.y as f64));
            }
            PathSegment::QuadTo(ctrl, pt) => {
                if current.is_empty() { continue; }
                let start = *current.last().unwrap();
                let steps = 10usize;
                for i in 1..=steps {
                    let t = i as f64 / steps as f64;
                    let mt = 1.0 - t;
                    let px = mt * mt * start.x + 2.0 * mt * t * ctrl.x as f64 + t * t * pt.x as f64;
                    let py = mt * mt * start.y + 2.0 * mt * t * ctrl.y as f64 + t * t * pt.y as f64;
                    current.push(Point::new(px, py));
                }
            }
            PathSegment::CubicTo(ctrl1, ctrl2, pt) => {
                if current.is_empty() { continue; }
                let start = *current.last().unwrap();
                let steps = 10usize;
                for i in 1..=steps {
                    let t = i as f64 / steps as f64;
                    let t2 = t * t;
                    let t3 = t2 * t;
                    let mt = 1.0 - t;
                    let mt2 = mt * mt;
                    let mt3 = mt2 * mt;
                    let px = mt3 * start.x
                        + 3.0 * mt2 * t * ctrl1.x as f64
                        + 3.0 * mt * t2 * ctrl2.x as f64
                        + t3 * pt.x as f64;
                    let py = mt3 * start.y
                        + 3.0 * mt2 * t * ctrl1.y as f64
                        + 3.0 * mt * t2 * ctrl2.y as f64
                        + t3 * pt.y as f64;
                    current.push(Point::new(px, py));
                }
            }
            PathSegment::Close => {
                if current.len() >= 2 {
                    let transformed = apply_transform_f64(&current, transform);
                    result.push(PathData { points: transformed, closed: true });
                }
                current.clear();
            }
        }
    }

    if current.len() >= 2 {
        let transformed = apply_transform_f64(&current, transform);
        result.push(PathData { points: transformed, closed: false });
    }

    if result.is_empty() { None } else { Some(result) }
}

fn apply_transform_f64(points: &[Point], transform: &usvg::Transform) -> Vec<Point> {
    points.iter().map(|p| {
        let x = transform.sx as f64 * p.x + transform.kx as f64 * p.y + transform.tx as f64;
        let y = transform.ky as f64 * p.x + transform.sy as f64 * p.y + transform.ty as f64;
        Point::new(x, y)
    }).collect()
}

/// Simplify SVG paths to reduce point count while preserving shape.
pub fn simplify_paths(paths: &[PathData], epsilon: f64) -> Vec<PathData> {
    paths.iter().map(|path| {
        let simplified = rdp_simplify(&path.points, epsilon);
        PathData { points: simplified, closed: path.closed }
    }).filter(|p| p.points.len() >= 2).collect()
}
