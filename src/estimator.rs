use crate::gcode_gen::{segment_feedrate, GCodeConfig};
use crate::geometry::{PathData, Point};
use std::f64::consts::PI;

#[derive(Debug, Clone)]
pub struct EstimateResult {
    pub drawing_time_s: f64,
    pub travel_time_s: f64,
    pub pen_up_down_time_s: f64,
    pub total_time_s: f64,
    pub drawing_distance_mm: f64,
    pub travel_distance_mm: f64,
    pub total_paths: usize,
    pub ink_volume_mm3: f64,
    pub ink_length_m: f64,
}

impl EstimateResult {
    pub fn drawing_time_formatted(&self) -> String {
        format_time(self.drawing_time_s)
    }
    pub fn travel_time_formatted(&self) -> String {
        format_time(self.travel_time_s)
    }
    pub fn total_time_formatted(&self) -> String {
        format_time(self.total_time_s)
    }
    pub fn pen_up_down_time_formatted(&self) -> String {
        format_time(self.pen_up_down_time_s)
    }
}

fn format_time(secs: f64) -> String {
    if secs < 60.0 {
        format!("{:.1} s", secs)
    } else {
        let m = (secs / 60.0).floor() as u32;
        let s = secs % 60.0;
        format!("{} min {:.0} s", m, s)
    }
}

pub fn estimate(paths: &[PathData], config: &GCodeConfig) -> EstimateResult {
    let mut drawing_distance_mm = 0.0;
    let mut drawing_time_s = 0.0;

    for path in paths {
        if path.points.len() < 2 {
            continue;
        }
        for w in path.points.windows(2) {
            let seg_len = w[0].distance_to(w[1]);
            drawing_distance_mm += seg_len;

            let feed = segment_feedrate(seg_len, config);
            drawing_time_s += seg_len / feed * 60.0;
        }

        if path.closed && path.points.len() > 2 {
            let first = path.points[0];
            let last = *path.points.last().unwrap();
            let seg_len = first.distance_to(last);
            if seg_len > 0.01 {
                drawing_distance_mm += seg_len;
                let feed = segment_feedrate(seg_len, config);
                drawing_time_s += seg_len / feed * 60.0;
            }
        }
    }

    let total_paths = paths.iter().filter(|p| p.points.len() >= 2).count();

    let travel_distance_mm = if total_paths > 1 {
        let mut dist = 0.0;
        let mut prev: Option<Point> = None;
        for path in paths {
            if path.points.len() < 2 {
                continue;
            }
            if let Some(prev_pt) = prev {
                dist += prev_pt.distance_to(path.points[0]);
            }
            prev = Some(path.points[path.points.len() - 1]);
        }
        dist
    } else {
        0.0
    };

    let travel_time_s = if config.travel_feedrate > 0.0 {
        travel_distance_mm / config.travel_feedrate * 60.0
    } else {
        0.0
    };

    let pen_up_distance = (config.pen_up_z - config.pen_down_z).abs();
    let pen_up_down_time_s = if config.feedrate_z > 0.0 && total_paths > 0 {
        (2.0 * total_paths as f64 + 1.0) * pen_up_distance / config.feedrate_z * 60.0
    } else {
        0.0
    };

    let total_time_s = drawing_time_s + travel_time_s + pen_up_down_time_s;

    let ink_length_m = drawing_distance_mm / 1000.0;

    let radius = config.pen_thickness / 2.0;
    let ink_volume_mm3 = drawing_distance_mm * PI * radius * radius;

    EstimateResult {
        drawing_time_s,
        travel_time_s,
        pen_up_down_time_s,
        total_time_s,
        drawing_distance_mm,
        travel_distance_mm,
        total_paths,
        ink_volume_mm3,
        ink_length_m,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Point;

    fn make_path(points: &[(f64, f64)]) -> PathData {
        PathData {
            points: points.iter().map(|&(x, y)| Point::new(x, y)).collect(),
            closed: false,
        }
    }

    #[test]
    fn test_empty_paths() {
        let r = estimate(&[], &GCodeConfig::default());
        assert_eq!(r.drawing_distance_mm, 0.0);
        assert_eq!(r.total_paths, 0);
    }

    #[test]
    fn test_single_path() {
        let p = make_path(&[(0.0, 0.0), (10.0, 0.0)]);
        let r = estimate(&[p], &GCodeConfig::default());
        assert!((r.drawing_distance_mm - 10.0).abs() < 0.001);
        assert_eq!(r.total_paths, 1);
        assert_eq!(r.travel_distance_mm, 0.0);
    }

    #[test]
    fn test_path_distances() {
        let paths = vec![
            make_path(&[(0.0, 0.0), (10.0, 0.0)]),
            make_path(&[(20.0, 0.0), (30.0, 0.0)]),
        ];
        let r = estimate(&paths, &GCodeConfig::default());
        assert!((r.drawing_distance_mm - 20.0).abs() < 0.001);
        assert!((r.travel_distance_mm - 10.0).abs() < 0.001);
    }

    #[test]
    fn test_ink_volume() {
        let p = make_path(&[(0.0, 0.0), (10.0, 0.0)]);
        let mut config = GCodeConfig::default();
        config.pen_thickness = 0.5;
        let r = estimate(&[p], &config);
        let expected = 10.0 * PI * 0.25 * 0.25;
        assert!((r.ink_volume_mm3 - expected).abs() < 0.001);
    }
}
