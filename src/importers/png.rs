use crate::geometry::{PathData, Point, rdp_simplify};
use image::GenericImageView;

/// Import mode for PNG processing.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum PngImportMode {
    EdgeDetection,
    Monochrome,
    Centerlines,
}

impl PngImportMode {
    pub fn all() -> &'static [Self] {
        &[PngImportMode::EdgeDetection, PngImportMode::Monochrome, PngImportMode::Centerlines]
    }
    pub fn name(&self) -> &'static str {
        match self {
            PngImportMode::EdgeDetection => "Edge Detection",
            PngImportMode::Monochrome => "Monochrome",
            PngImportMode::Centerlines => "Centerlines",
        }
    }
}

/// Configuration for PNG import.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PngImportConfig {
    /// Processing mode.
    pub mode: PngImportMode,
    /// Canny low threshold (0-100).
    pub canny_low: f32,
    /// Canny high threshold (0-100).
    pub canny_high: f32,
    /// Fixed threshold for monochrome mode (0 = auto/Otsu).
    pub threshold_value: u8,
    /// Simplify tolerance in pixels.
    pub simplify_epsilon: f64,
    /// Minimum contour length (in points) to keep.
    pub min_contour_len: usize,
    /// Scale factor from pixels to mm.
    pub pixel_to_mm: f64,
    /// Fill closed contours with hatch lines.
    pub fill_enabled: bool,
    /// Spacing between hatch lines (mm).
    pub fill_spacing: f64,
}

impl Default for PngImportConfig {
    fn default() -> Self {
        PngImportConfig {
            mode: PngImportMode::EdgeDetection,
            canny_low: 30.0,
            canny_high: 80.0,
            threshold_value: 0,
            simplify_epsilon: 1.5,
            min_contour_len: 10,
            pixel_to_mm: 0.1,
            fill_enabled: false,
            fill_spacing: 0.5,
        }
    }
}

/// Import a PNG file and extract vector paths.
pub fn import_png(filepath: &str, config: &PngImportConfig) -> anyhow::Result<Vec<PathData>> {
    let img = image::open(filepath)?;
    let (w, h) = img.dimensions();
    log::info!("PNG import: {}x{} image, mode={:?}", w, h, config.mode);

    let gray = img.to_luma8();

    let mut all_paths = match config.mode {
        PngImportMode::EdgeDetection => {
            let p = extract_edges(&gray, config);
            let closed = p.iter().filter(|x| x.closed).count();
            log::info!("Edge detection: {} contours ({} closed)", p.len(), closed);
            p
        }
        PngImportMode::Monochrome => {
            let p = extract_monochrome(&gray, config);
            let closed = p.iter().filter(|x| x.closed).count();
            log::info!("Monochrome: {} contours ({} closed, min {} pts)", p.len(), closed, config.min_contour_len);
            p
        }
        PngImportMode::Centerlines => {
            let p = extract_centerlines(&gray, config);
            log::info!("Centerlines: {} paths", p.len());
            p
        }
    };

    // Scale paths from pixel coords to mm and flip Y (image origin is top-left)
    let pixel_to_mm = config.pixel_to_mm;
    for path in &mut all_paths {
        for p in &mut path.points {
            p.x *= pixel_to_mm;
            p.y = -p.y * pixel_to_mm; // flip Y so image is right-side up
        }
    }

    // Apply fill if enabled (paths are now in mm, so spacing is in mm too)
    if config.fill_enabled && config.fill_spacing > 0.0 {
        let closed_count = all_paths.iter().filter(|p| p.closed && p.points.len() >= 3).count();
        log::info!("Fill: {} closed paths eligible, spacing={} mm", closed_count, config.fill_spacing);
        let fill = generate_fill(&all_paths, config.fill_spacing);
        if !fill.is_empty() {
            let count = fill.len();
            all_paths.extend(fill);
            log::info!("Fill: {} hatch lines generated", count);
        } else {
            log::warn!("Fill: no hatch lines generated (paths may be too small or spacing too large)");
        }
    }

    log::info!("PNG import: total {} paths", all_paths.len());
    Ok(all_paths)
}

/// Extract edge contours using Canny edge detection.
fn extract_edges(gray: &image::GrayImage, config: &PngImportConfig) -> Vec<PathData> {
    // Wrap canny in catch_unwind because imageproc 0.25 can overflow on some images
    let edges = match std::panic::catch_unwind(|| {
        imageproc::edges::canny(gray, config.canny_low, config.canny_high)
    }) {
        Ok(e) => e,
        Err(e) => {
            let msg = if let Some(s) = e.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = e.downcast_ref::<String>() {
                s.clone()
            } else {
                "Unknown overflow in edge detection".to_string()
            };
            log::warn!("Canny edge detection failed: {}", msg);
            log::info!("Falling back to Sobel gradient magnitude edge detection");
            // Fallback: gradient magnitude threshold
            let h = imageproc::gradients::horizontal_sobel(gray);
            let v = imageproc::gradients::vertical_sobel(gray);
            let (w, h_dims) = gray.dimensions();
            let mut edge_img = image::GrayImage::new(w, h_dims);
            for y in 0..h_dims {
                for x in 0..w {
                    let gx = h.get_pixel(x, y).0[0].abs() as f32;
                    let gy = v.get_pixel(x, y).0[0].abs() as f32;
                    let mag = (gx + gy) * 0.5; // approximate magnitude
                    let val = if mag > 64.0 { 255u8 } else { 0u8 };
                    edge_img.put_pixel(x, y, image::Luma([val]));
                }
            }
            edge_img
        }
    };
    let contours = imageproc::contours::find_contours::<i32>(&edges);

    let mut paths = Vec::new();
    for contour in &contours {
        let pts: Vec<Point> = contour
            .points
            .iter()
            .map(|p| Point::new(p.x as f64, p.y as f64))
            .collect();

        if pts.len() < config.min_contour_len {
            continue;
        }

        let simplified = rdp_simplify(&pts, config.simplify_epsilon);
        if simplified.len() >= 2 {
            paths.push(PathData {
                points: simplified,
                closed: true,
            });
        }
    }

    paths
}

/// Extract closed contours from a thresholded (monochrome) image.
fn extract_monochrome(gray: &image::GrayImage, config: &PngImportConfig) -> Vec<PathData> {
    let binary = if config.threshold_value > 0 {
        binarize_fixed(gray, config.threshold_value)
    } else {
        binarize_otsu(gray)
    };
    // Invert so foreground (dark) pixels are 255
    let inverted = invert_binary(&binary);
    let contours = imageproc::contours::find_contours::<i32>(&inverted);

    let mut paths = Vec::new();
    for contour in &contours {
        let pts: Vec<Point> = contour
            .points
            .iter()
            .map(|p| Point::new(p.x as f64, p.y as f64))
            .collect();
        if pts.len() < config.min_contour_len {
            continue;
        }
        let simplified = rdp_simplify(&pts, config.simplify_epsilon);
        if simplified.len() >= 2 {
            paths.push(PathData {
                points: simplified,
                closed: true,
            });
        }
    }
    paths
}

fn invert_binary(binary: &image::GrayImage) -> image::GrayImage {
    let (w, h) = binary.dimensions();
    let mut out = image::GrayImage::new(w, h);
    for (x, y, p) in binary.enumerate_pixels() {
        out.put_pixel(x, y, image::Luma([if p.0[0] > 128 { 0 } else { 255 }]));
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

/// Generate hatch fill lines inside all closed paths (pixel coords).
fn generate_fill(paths: &[PathData], spacing: f64) -> Vec<PathData> {
    use crate::importers::text::generate_hatch_fill;
    generate_hatch_fill(paths, spacing)
}
fn extract_centerlines(gray: &image::GrayImage, config: &PngImportConfig) -> Vec<PathData> {
    // Binarize: apply adaptive threshold
    let binary = binarize_otsu(gray);

    // Apply thinning (morphological skeletonization)
    let skeleton = thin_image(&binary);

    // Trace skeleton paths
    trace_skeleton_paths(&skeleton, config)
}

/// Binarize using Otsu's method.
fn binarize_otsu(gray: &image::GrayImage) -> image::GrayImage {
    let (w, h) = gray.dimensions();
    let total_pixels = (w * h) as usize;
    if total_pixels == 0 {
        return gray.clone();
    }

    // Compute histogram
    let mut hist = [0u64; 256];
    for pixel in gray.pixels() {
        hist[pixel.0[0] as usize] += 1;
    }

    // Otsu threshold
    let mut sum_b: f64 = 0.0;
    let mut w_b: f64 = 0.0;
    let mut max_variance: f64 = 0.0;
    let mut threshold: u8 = 0;
    let total = total_pixels as f64;

    let sum_all: f64 = hist
        .iter()
        .enumerate()
        .map(|(i, &c)| i as f64 * c as f64)
        .sum();

    for t in 0..256 {
        w_b += hist[t] as f64;
        if w_b == 0.0 {
            continue;
        }
        let w_f = total - w_b;
        if w_f == 0.0 {
            break;
        }
        sum_b += t as f64 * hist[t] as f64;
        let m_b = sum_b / w_b;
        let m_f = (sum_all - sum_b) / w_f;
        let variance = w_b * w_f * (m_b - m_f) * (m_b - m_f);
        if variance > max_variance {
            max_variance = variance;
            threshold = t as u8;
        }
    }

    // Apply threshold
    let mut out = image::GrayImage::new(w, h);
    for (x, y, p) in gray.enumerate_pixels() {
        let val = if p.0[0] > threshold { 255u8 } else { 0u8 };
        out.put_pixel(x, y, image::Luma([val]));
    }

    out
}

/// Morphological thinning (Zhang-Suen algorithm).
fn thin_image(binary: &image::GrayImage) -> image::GrayImage {
    let (w, h) = binary.dimensions();
    let mut img = binary.clone();

    loop {
        // Step 1
        let mut changed = false;
        let mut step1 = img.clone();
        for y in 1..h as i32 - 1 {
            for x in 1..w as i32 - 1 {
                if img.get_pixel(x as u32, y as u32).0[0] == 0 {
                    continue;
                }
                let p = get_pixel_region(&img, x, y);
                if zhang_suen_condition1(p) {
                    step1.put_pixel(x as u32, y as u32, image::Luma([0]));
                    changed = true;
                }
            }
        }

        // Step 2
        let mut step2 = step1.clone();
        for y in 1..h as i32 - 1 {
            for x in 1..w as i32 - 1 {
                if step1.get_pixel(x as u32, y as u32).0[0] == 0 {
                    continue;
                }
                let p = get_pixel_region(&step1, x, y);
                if zhang_suen_condition2(p) {
                    step2.put_pixel(x as u32, y as u32, image::Luma([0]));
                    changed = true;
                }
            }
        }

        img = step2;
        if !changed {
            break;
        }
    }

    img
}

type PixelRegion = [u8; 9]; // P9 P2 P3  /  P8 P1 P4  /  P7 P6 P5

fn get_pixel_region(img: &image::GrayImage, cx: i32, cy: i32) -> PixelRegion {
    // P9 P2 P3
    // P8 P1 P4
    // P7 P6 P5
    let p1 = img.get_pixel(cx as u32, cy as u32).0[0];
    let p2 = img.get_pixel(cx as u32, (cy - 1) as u32).0[0];
    let p3 = img.get_pixel((cx + 1) as u32, (cy - 1) as u32).0[0];
    let p4 = img.get_pixel((cx + 1) as u32, cy as u32).0[0];
    let p5 = img.get_pixel((cx + 1) as u32, (cy + 1) as u32).0[0];
    let p6 = img.get_pixel(cx as u32, (cy + 1) as u32).0[0];
    let p7 = img.get_pixel((cx - 1) as u32, (cy + 1) as u32).0[0];
    let p8 = img.get_pixel((cx - 1) as u32, cy as u32).0[0];
    let p9 = img.get_pixel((cx - 1) as u32, (cy - 1) as u32).0[0];

    let b = |v: u8| if v > 128 { 1u8 } else { 0u8 };

    [b(p1), b(p2), b(p3), b(p4), b(p5), b(p6), b(p7), b(p8), b(p9)]
}

fn zhang_suen_condition1(p: PixelRegion) -> bool {
    // 2 <= B(P1) <= 6
    let b = p[1] + p[2] + p[3] + p[4] + p[5] + p[6] + p[7] + p[8];
    if b < 2 || b > 6 {
        return false;
    }

    // A(P1) = 1
    let transitions = count_transitions(p);
    if transitions != 1 {
        return false;
    }

    // P2 * P4 * P6 = 0
    if p[1] * p[3] * p[5] != 0 {
        return false;
    }

    // P4 * P6 * P8 = 0
    if p[3] * p[5] * p[7] != 0 {
        return false;
    }

    true
}

fn zhang_suen_condition2(p: PixelRegion) -> bool {
    let b = p[1] + p[2] + p[3] + p[4] + p[5] + p[6] + p[7] + p[8];
    if b < 2 || b > 6 {
        return false;
    }

    let transitions = count_transitions(p);
    if transitions != 1 {
        return false;
    }

    // P2 * P4 * P8 = 0
    if p[1] * p[3] * p[7] != 0 {
        return false;
    }

    // P2 * P6 * P8 = 0
    if p[1] * p[5] * p[7] != 0 {
        return false;
    }

    true
}

fn count_transitions(p: PixelRegion) -> u8 {
    // Count 0→1 transitions in the ordered sequence P2,P3,P4,P5,P6,P7,P8,P9,P2
    let seq = [p[1], p[2], p[3], p[4], p[5], p[6], p[7], p[8], p[1]];
    let mut count = 0u8;
    for i in 0..8 {
        if seq[i] == 0 && seq[i + 1] == 1 {
            count += 1;
        }
    }
    count
}

/// Trace paths from a skeletonized binary image.
fn trace_skeleton_paths(skeleton: &image::GrayImage, config: &PngImportConfig) -> Vec<PathData> {
    let (w, h) = skeleton.dimensions();
    let mut visited = image::GrayImage::new(w, h);
    let mut paths = Vec::new();

    for y in 0..h {
        for x in 0..w {
            if skeleton.get_pixel(x, y).0[0] > 128 && visited.get_pixel(x, y).0[0] == 0 {
                let path = trace_path(skeleton, &mut visited, x, y);
                let simplified = rdp_simplify(&path, config.simplify_epsilon);
                if simplified.len() >= config.min_contour_len {
                    paths.push(PathData {
                        points: simplified,
                        closed: false,
                    });
                }
            }
        }
    }

    paths
}

/// Trace a single path starting from an unvisited skeleton pixel.
fn trace_path(
    skeleton: &image::GrayImage,
    visited: &mut image::GrayImage,
    start_x: u32,
    start_y: u32,
) -> Vec<Point> {
    let mut points = Vec::new();
    let mut stack = vec![(start_x, start_y)];

    while let Some((x, y)) = stack.pop() {
        if visited.get_pixel(x, y).0[0] > 0 {
            continue;
        }
        visited.put_pixel(x, y, image::Luma([255]));
        points.push(Point::new(x as f64, y as f64));

        // Check 8-connected neighbors
        for dx in [-1i32, 0, 1] {
            for dy in [-1i32, 0, 1] {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let nx = x as i32 + dx;
                let ny = y as i32 + dy;
                if nx >= 0 && nx < skeleton.width() as i32 && ny >= 0 && ny < skeleton.height() as i32
                {
                    let nx = nx as u32;
                    let ny = ny as u32;
                    if skeleton.get_pixel(nx, ny).0[0] > 128
                        && visited.get_pixel(nx, ny).0[0] == 0
                    {
                        stack.push((nx, ny));
                    }
                }
            }
        }
    }

    points
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_binarize_otsu() {
        let gray = image::GrayImage::new(10, 10);
        let binary = binarize_otsu(&gray);
        assert_eq!(binary.dimensions(), (10, 10));
    }

    #[test]
    fn test_zhang_suen_conditions() {
        // Test with a simple 3x3 region
        let p = [1u8; 9];
        assert!(!zhang_suen_condition1(p));
        assert!(!zhang_suen_condition2(p));
    }
}
