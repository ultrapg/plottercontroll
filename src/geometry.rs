/// A 2D point in millimeters.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub fn new(x: f64, y: f64) -> Self {
        Point { x, y }
    }

    pub fn distance_to(self, other: Point) -> f64 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        (dx * dx + dy * dy).sqrt()
    }
}

/// A path composed of line segments (polyline).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PathData {
    pub points: Vec<Point>,
    pub closed: bool,
}

impl PathData {
    pub fn new() -> Self {
        PathData {
            points: Vec::new(),
            closed: false,
        }
    }

    pub fn with_capacity(cap: usize) -> Self {
        PathData {
            points: Vec::with_capacity(cap),
            closed: false,
        }
    }

    pub fn len(&self) -> usize {
        self.points.len()
    }

    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    pub fn translate(&mut self, dx: f64, dy: f64) {
        for p in &mut self.points {
            p.x += dx;
            p.y += dy;
        }
    }

    pub fn scale(&mut self, sx: f64, sy: f64) {
        for p in &mut self.points {
            p.x *= sx;
            p.y *= sy;
        }
    }

    pub fn bbox(&self) -> BoundingBox {
        let mut bb = BoundingBox::empty();
        for p in &self.points {
            bb.expand(*p);
        }
        bb
    }

    pub fn reverse(&mut self) {
        self.points.reverse();
    }
}

/// Axis-aligned bounding box.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct BoundingBox {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}

impl BoundingBox {
    pub fn empty() -> Self {
        BoundingBox {
            min_x: f64::INFINITY,
            min_y: f64::INFINITY,
            max_x: f64::NEG_INFINITY,
            max_y: f64::NEG_INFINITY,
        }
    }

    pub fn new(min_x: f64, min_y: f64, max_x: f64, max_y: f64) -> Self {
        BoundingBox {
            min_x,
            min_y,
            max_x,
            max_y,
        }
    }

    pub fn from_paths(paths: &[PathData]) -> Self {
        let mut bb = BoundingBox::empty();
        for path in paths {
            bb = bb.union(&path.bbox());
        }
        bb
    }

    pub fn expand(&mut self, p: Point) {
        self.min_x = self.min_x.min(p.x);
        self.min_y = self.min_y.min(p.y);
        self.max_x = self.max_x.max(p.x);
        self.max_y = self.max_y.max(p.y);
    }

    pub fn union(&self, other: &BoundingBox) -> Self {
        BoundingBox {
            min_x: self.min_x.min(other.min_x),
            min_y: self.min_y.min(other.min_y),
            max_x: self.max_x.max(other.max_x),
            max_y: self.max_y.max(other.max_y),
        }
    }

    pub fn width(&self) -> f64 {
        if self.is_empty() {
            0.0
        } else {
            self.max_x - self.min_x
        }
    }

    pub fn height(&self) -> f64 {
        if self.is_empty() {
            0.0
        } else {
            self.max_y - self.min_y
        }
    }

    pub fn is_empty(&self) -> bool {
        self.min_x > self.max_x || self.min_y > self.max_y
    }

    pub fn center(&self) -> Point {
        Point::new(
            (self.min_x + self.max_x) / 2.0,
            (self.min_y + self.max_y) / 2.0,
        )
    }

    /// Compute uniform scale + offset to fit into target dimensions with margin.
    /// Returns (scale, offset_x, offset_y).
    pub fn fit_to(&self, target_w: f64, target_h: f64, margin: f64) -> (f64, f64, f64) {
        let content_w = target_w - 2.0 * margin;
        let content_h = target_h - 2.0 * margin;
        if content_w <= 0.0 || content_h <= 0.0 || self.is_empty() {
            return (1.0, margin, margin);
        }
        let scale = (content_w / self.width()).min(content_h / self.height());
        let scaled_w = self.width() * scale;
        let scaled_h = self.height() * scale;
        let offset_x = margin + (content_w - scaled_w) / 2.0 - self.min_x * scale;
        let offset_y = margin + (content_h - scaled_h) / 2.0 - self.min_y * scale;
        (scale, offset_x, offset_y)
    }

    pub fn longest_dimension(&self) -> f64 {
        self.width().max(self.height())
    }
}

/// Determine if a polygon is clockwise or counter-clockwise.
pub fn is_clockwise(points: &[Point]) -> bool {
    if points.len() < 3 {
        return true;
    }
    let area: f64 = points
        .windows(2)
        .map(|w| w[0].x * w[1].y - w[1].x * w[0].y)
        .sum();
    area < 0.0
}

/// Simplify a polyline using the Ramer-Douglas-Peucker algorithm.
pub fn rdp_simplify(points: &[Point], epsilon: f64) -> Vec<Point> {
    if points.is_empty() {
        return Vec::new();
    }
    if points.len() <= 2 {
        return points.to_vec();
    }

    let mut result = Vec::new();
    rdp_recursive(points, epsilon, &mut result);
    result
}

fn rdp_recursive(points: &[Point], epsilon: f64, result: &mut Vec<Point>) {
    if points.len() <= 2 {
        result.extend_from_slice(points);
        return;
    }

    let start = points[0];
    let end = *points.last().unwrap();

    let mut dmax = 0.0;
    let mut index = 0;

    for i in 1..points.len() - 1 {
        let d = perpendicular_distance(points[i], start, end);
        if d > dmax {
            dmax = d;
            index = i;
        }
    }

    if dmax > epsilon {
        rdp_recursive(&points[..=index], epsilon, result);
        result.pop();
        rdp_recursive(&points[index..], epsilon, result);
    } else {
        result.push(start);
        result.push(end);
    }
}

fn perpendicular_distance(p: Point, a: Point, b: Point) -> f64 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let length_sq = dx * dx + dy * dy;
    if length_sq == 0.0 {
        return p.distance_to(a);
    }
    let t = ((p.x - a.x) * dx + (p.y - a.y) * dy) / length_sq;
    let t = t.clamp(0.0, 1.0);
    let proj = Point::new(a.x + t * dx, a.y + t * dy);
    p.distance_to(proj)
}
