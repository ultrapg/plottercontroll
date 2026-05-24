use crate::geometry::{PathData, Point};

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum OptimizerScope {
    PerElement,
    Global,
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum OptimizerAlgorithm {
    NearestNeighbor,
    TwoOpt,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OptimizerConfig {
    pub enabled: bool,
    pub algorithm: OptimizerAlgorithm,
    pub scope: OptimizerScope,
    pub reverse_paths: bool,
    pub start_at_closest_to_origin: bool,
}

impl Default for OptimizerConfig {
    fn default() -> Self {
        OptimizerConfig {
            enabled: true,
            algorithm: OptimizerAlgorithm::NearestNeighbor,
            scope: OptimizerScope::PerElement,
            reverse_paths: true,
            start_at_closest_to_origin: true,
        }
    }
}

fn endpoint_dist(path: &PathData, from: Point, reverse_ok: bool) -> (f64, bool) {
    if path.points.is_empty() {
        return (f64::MAX, false);
    }
    let d_start = from.distance_to(path.points[0]);
    if !reverse_ok {
        return (d_start, false);
    }
    let d_end = from.distance_to(path.points[path.points.len() - 1]);
    if d_end < d_start {
        (d_end, true)
    } else {
        (d_start, false)
    }
}

fn endpoint_of(path: &PathData) -> Point {
    if path.points.is_empty() {
        Point::new(0.0, 0.0)
    } else {
        path.points[path.points.len() - 1]
    }
}

fn start_pt(path: &PathData) -> Point {
    if path.points.is_empty() {
        Point::new(0.0, 0.0)
    } else {
        path.points[0]
    }
}

fn find_closest(paths: &[PathData], from: Point, reverse_ok: bool, visited: &[bool]) -> Option<(usize, bool)> {
    let mut best_idx = None;
    let mut best_dist = f64::MAX;
    let mut best_rev = false;
    for (i, p) in paths.iter().enumerate() {
        if visited[i] || p.points.len() < 2 {
            continue;
        }
        let (d, rev) = endpoint_dist(p, from, reverse_ok);
        if d < best_dist {
            best_dist = d;
            best_idx = Some(i);
            best_rev = rev;
        }
    }
    best_idx.map(|i| (i, best_rev))
}

fn optimize_directions(paths: &mut [PathData], reverse_ok: bool) {
    if paths.len() < 2 || !reverse_ok {
        return;
    }
    for i in 1..paths.len() {
        let prev_end = endpoint_of(&paths[i - 1]);
        let d_forward = prev_end.distance_to(start_pt(&paths[i]));
        let d_reverse = prev_end.distance_to(endpoint_of(&paths[i]));
        if d_reverse < d_forward {
            paths[i].reverse();
        }
    }
}

fn travel_cost(paths: &[PathData]) -> f64 {
    let mut cost = 0.0;
    for w in paths.windows(2) {
        cost += endpoint_of(&w[0]).distance_to(start_pt(&w[1]));
    }
    cost
}

fn optimize_nearest_neighbor(paths: &[PathData], config: &OptimizerConfig) -> Vec<PathData> {
    if paths.len() <= 1 {
        return paths.to_vec();
    }
    let n = paths.len();
    let mut visited = vec![false; n];
    let mut result = Vec::with_capacity(n);

    let start_idx = if config.start_at_closest_to_origin {
        let origin = Point::new(0.0, 0.0);
        let mut best = 0;
        let mut best_d = f64::MAX;
        for (i, p) in paths.iter().enumerate() {
            if p.points.is_empty() { continue; }
            let d = origin.distance_to(p.points[0]);
            if d < best_d { best_d = d; best = i; }
        }
        best
    } else {
        0
    };

    visited[start_idx] = true;
    result.push(paths[start_idx].clone());
    let mut current_end = endpoint_of(&paths[start_idx]);

    for _ in 1..n {
        if let Some((idx, rev)) = find_closest(paths, current_end, config.reverse_paths, &visited) {
            visited[idx] = true;
            let mut p = paths[idx].clone();
            if rev {
                p.reverse();
            }
            current_end = endpoint_of(&p);
            result.push(p);
        }
    }

    if config.reverse_paths {
        optimize_directions(&mut result, true);
    }

    result
}

fn optimize_two_opt(paths: &[PathData], config: &OptimizerConfig) -> Vec<PathData> {
    let mut current = optimize_nearest_neighbor(paths, config);
    if current.len() < 3 {
        return current;
    }

    let mut improved = true;
    while improved {
        improved = false;
        let best_cost = travel_cost(&current);

        for i in 0..current.len() - 2 {
            for j in i + 2..current.len() {
                let mut candidate = current.clone();
                candidate[i..=j].reverse();

                if config.reverse_paths {
                    optimize_directions(&mut candidate, true);
                }

                let new_cost = travel_cost(&candidate);
                if new_cost < best_cost - 0.001 {
                    current = candidate;
                    improved = true;
                    break;
                }
            }
            if improved {
                break;
            }
        }
    }

    current
}

fn optimize_one_group(paths: &[PathData], config: &OptimizerConfig) -> Vec<PathData> {
    match config.algorithm {
        OptimizerAlgorithm::NearestNeighbor => optimize_nearest_neighbor(paths, config),
        OptimizerAlgorithm::TwoOpt => optimize_two_opt(paths, config),
    }
}

pub fn optimize_path_order(
    paths: &[PathData],
    config: &OptimizerConfig,
    element_ranges: &[(usize, usize)],
) -> (Vec<PathData>, Vec<(usize, usize)>) {
    let mut out_paths = Vec::with_capacity(paths.len());
    let mut out_ranges = Vec::with_capacity(element_ranges.len());

    match config.scope {
        OptimizerScope::Global => {
            let optimized = optimize_one_group(paths, config);
            out_paths = optimized;
            out_ranges.push((0, out_paths.len()));
        }
        OptimizerScope::PerElement => {
            for &(start, count) in element_ranges {
                if count == 0 {
                    out_ranges.push((out_paths.len(), 0));
                    continue;
                }
                let group = &paths[start..start + count];
                let optimized = optimize_one_group(group, config);
                let new_start = out_paths.len();
                out_paths.extend(optimized);
                out_ranges.push((new_start, out_paths.len() - new_start));
            }
        }
    }

    (out_paths, out_ranges)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_path(points: &[(f64, f64)]) -> PathData {
        PathData {
            points: points.iter().map(|&(x, y)| Point::new(x, y)).collect(),
            closed: false,
        }
    }

    fn travel_dist(paths: &[PathData]) -> f64 {
        let mut d = 0.0;
        for w in paths.windows(2) {
            d += endpoint_of(&w[0]).distance_to(start_pt(&w[1]));
        }
        d
    }

    #[test]
    fn test_single_path() {
        let p = make_path(&[(0.0, 0.0), (10.0, 0.0)]);
        let (out, ranges) = optimize_path_order(&[p], &OptimizerConfig::default(), &[(0, 1)]);
        assert_eq!(out.len(), 1);
        assert_eq!(ranges, [(0, 1)]);
    }

    #[test]
    fn test_nearest_neighbor_reorder() {
        let paths = vec![
            make_path(&[(100.0, 100.0), (110.0, 100.0)]),
            make_path(&[(0.0, 0.0), (10.0, 0.0)]),
            make_path(&[(50.0, 50.0), (55.0, 55.0)]),
        ];
        let mut config = OptimizerConfig::default();
        config.start_at_closest_to_origin = true;
        let (out, _) = optimize_path_order(&paths, &config, &[(0, 3)]);
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].points[0].x, 0.0);
        assert_eq!(out[0].points[0].y, 0.0);
    }

    #[test]
    fn test_per_element() {
        let paths = vec![
            make_path(&[(100.0, 0.0), (110.0, 0.0)]),
            make_path(&[(0.0, 100.0), (0.0, 110.0)]),
        ];
        let (out, ranges) = optimize_path_order(&paths, &OptimizerConfig::default(), &[(0, 1), (1, 1)]);
        assert_eq!(out.len(), 2);
        assert_eq!(ranges, [(0, 1), (1, 1)]);
    }

    #[test]
    fn test_reversal() {
        let paths = vec![
            make_path(&[(0.0, 0.0), (10.0, 0.0)]),
            make_path(&[(10.0, 10.0), (0.0, 10.0)]),
        ];
        let mut config = OptimizerConfig::default();
        config.reverse_paths = true;
        config.start_at_closest_to_origin = true;
        let (out, _) = optimize_path_order(&paths, &config, &[(0, 2)]);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].points[0].x, 0.0);
        assert_eq!(out[0].points[0].y, 0.0);
        assert_eq!(out[1].points[0].x, 10.0);
        assert_eq!(out[1].points[0].y, 10.0);
    }

    #[test]
    fn test_two_opt_improves() {
        let paths = vec![
            make_path(&[(0.0, 0.0), (10.0, 0.0)]),
            make_path(&[(100.0, 100.0), (110.0, 100.0)]),
            make_path(&[(5.0, 0.0), (15.0, 0.0)]),
            make_path(&[(95.0, 100.0), (105.0, 100.0)]),
        ];
        let mut config = OptimizerConfig::default();
        config.algorithm = OptimizerAlgorithm::TwoOpt;
        config.start_at_closest_to_origin = true;
        config.reverse_paths = true;
        let (out, _) = optimize_path_order(&paths, &config, &[(0, 4)]);

        let nn_config = OptimizerConfig { algorithm: OptimizerAlgorithm::NearestNeighbor, ..config.clone() };
        let (nn_out, _) = optimize_path_order(&paths, &nn_config, &[(0, 4)]);

        let nn_dist = travel_dist(&nn_out);
        let two_dist = travel_dist(&out);
        assert!(two_dist <= nn_dist + 0.001);
    }

    #[test]
    fn test_two_opt_no_regression() {
        let paths = vec![
            make_path(&[(0.0, 0.0), (1.0, 0.0)]),
            make_path(&[(2.0, 0.0), (3.0, 0.0)]),
        ];
        let mut config = OptimizerConfig::default();
        config.algorithm = OptimizerAlgorithm::TwoOpt;
        let (out, _) = optimize_path_order(&paths, &config, &[(0, 2)]);
        assert_eq!(out.len(), 2);
    }
}
