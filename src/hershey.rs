/// Hershey single-line vector font renderer.
/// Contains a complete set of ASCII printable characters (32-126)
/// using a compact coordinate format.
use crate::geometry::{PathData, Point};

/// A glyph defined by strokes.
/// points is an array of (x, y) coordinate pairs.
/// The sentinel (-128, -128) marks a pen-up (new stroke).
/// width is the character advance width in tenths of a unit.
struct HersheyGlyph {
    points: &'static [(i8, i8)],
    width: i8,
}

/// Look up and render a character into paths.
pub fn render_char(ch: char, x: f64, y: f64, scale: f64) -> Vec<PathData> {
    if ch == ' ' {
        return Vec::new();
    }
    let idx = (ch as usize).wrapping_sub(32);
    if idx >= GLYPHS.len() {
        return Vec::new();
    }
    if let Some(ref glyph) = GLYPHS[idx] {
        glyph_to_paths(glyph, x, y, scale)
    } else {
        Vec::new()
    }
}

/// Render a string into paths.
pub fn render_text(text: &str, x: f64, y: f64, scale: f64, letter_spacing: f64) -> Vec<PathData> {
    let mut all_paths = Vec::new();
    let mut cursor_x = x;
    let mut cursor_y = y;
    let line_height = 18.0 * scale; // ~14 unit glyph height + gap
    for ch in text.chars() {
        if ch == '\n' {
            cursor_x = x;
            cursor_y -= line_height;
            continue;
        }
        let paths = render_char(ch, cursor_x, cursor_y, scale);
        all_paths.extend(paths);
        cursor_x += char_width(ch) as f64 * scale + letter_spacing;
    }
    all_paths
}

/// Get the advance width of a character in tenths of a unit.
pub fn char_width(ch: char) -> i8 {
    let idx = (ch as usize).wrapping_sub(32);
    if idx >= GLYPHS.len() {
        return 8;
    }
    GLYPH_WIDTHS[idx]
}

fn glyph_to_paths(glyph: &HersheyGlyph, origin_x: f64, origin_y: f64, scale: f64) -> Vec<PathData> {
    let mut paths = Vec::new();
    let mut current_path = PathData::new();

    for &(px, py) in glyph.points {
        if px == -128 && py == -128 {
            if current_path.points.len() >= 2 {
                paths.push(current_path);
            } else {
                current_path.points.clear();
            }
            current_path = PathData::new();
            continue;
        }
        let rx = origin_x + px as f64 * scale;
        let ry = origin_y + py as f64 * scale;
        current_path.points.push(Point::new(rx, ry));
    }

    if current_path.points.len() >= 2 {
        paths.push(current_path);
    }

    paths
}

/// Get the width of a text string in tenths of a unit.
pub fn text_width(text: &str, scale: f64, letter_spacing: f64) -> f64 {
    let mut w = 0.0;
    for ch in text.chars() {
        w += char_width(ch) as f64 * scale + letter_spacing;
    }
    if !text.is_empty() {
        w -= letter_spacing;
    }
    w
}

type G = HersheyGlyph;

// Glyph definitions: indexed by ASCII-32, None = undefined → rendered as space
const GLYPHS: &[Option<G>] = &[
    // 32 space
    Some(G { points: &[], width: 8 }),
    // 33 !
    Some(G { points: &[(4, 0), (4, 12), (-128, -128), (4, 14), (4, 14)], width: 8 }),
    // 34 "
    Some(G { points: &[(2, 12), (2, 10), (-128, -128), (6, 12), (6, 10)], width: 8 }),
    // 35 #
    Some(G { points: &[
        (2, 2), (6, 14), (-128, -128), (8, 2), (4, 14),
        (-128, -128), (1, 6), (9, 6), (-128, -128), (1, 10), (9, 10),
    ], width: 10 }),
    // 36 $
    Some(G { points: &[
        (5, 16), (5, 0), (-128, -128), (2, 12), (4, 14), (8, 14), (9, 12), (8, 10),
        (2, 6), (1, 4), (2, 2), (6, 2), (9, 4),
    ], width: 10 }),
    // 37 %
    Some(G { points: &[
        (8, 14), (2, 0), (-128, -128), (3, 14), (5, 14), (5, 12), (3, 12), (3, 14),
        (-128, -128), (5, 2), (7, 2), (7, 0), (5, 0), (5, 2),
    ], width: 10 }),
    // 38 &
    Some(G { points: &[
        (9, 4), (8, 2), (5, 0), (2, 2), (1, 5), (2, 8), (5, 11), (9, 14),
        (9, 0), (8, 2),
    ], width: 10 }),
    // 39 '
    Some(G { points: &[(4, 12), (4, 10)], width: 5 }),
    // 40 (
    Some(G { points: &[
        (6, 15), (4, 13), (3, 10), (3, 4), (4, 1), (6, -2),
    ], width: 7 }),
    // 41 )
    Some(G { points: &[
        (2, 15), (4, 13), (5, 10), (5, 4), (4, 1), (2, -2),
    ], width: 7 }),
    // 42 *
    Some(G { points: &[
        (5, 12), (5, 14), (-128, -128), (3, 14), (7, 14), (-128, -128), (4, 13), (6, 15),
    ], width: 10 }),
    // 43 +
    Some(G { points: &[
        (5, 2), (5, 12), (-128, -128), (1, 7), (9, 7),
    ], width: 10 }),
    // 44 ,
    Some(G { points: &[(4, 0), (4, -3)], width: 5 }),
    // 45 -
    Some(G { points: &[(1, 7), (9, 7)], width: 10 }),
    // 46 .
    Some(G { points: &[(4, 0), (4, 0)], width: 5 }),
    // 47 /
    Some(G { points: &[(8, 14), (2, 0)], width: 10 }),
    // 48 0
    Some(G { points: &[
        (1, 14), (9, 14), (9, 0), (1, 0), (1, 14),
        (-128, -128), (1, 0), (9, 14),
    ], width: 10 }),
    // 49 1
    Some(G { points: &[
        (4, 12), (5, 14), (5, 0), (2, 0), (7, 0),
    ], width: 10 }),
    // 50 2
    Some(G { points: &[
        (1, 12), (2, 14), (8, 14), (9, 12), (9, 9), (1, 2), (1, 0), (9, 0),
    ], width: 10 }),
    // 51 3
    Some(G { points: &[
        (1, 14), (9, 14), (9, 8), (3, 8), (9, 8), (9, 0), (1, 0),
    ], width: 10 }),
    // 52 4
    Some(G { points: &[
        (6, 14), (2, 5), (10, 5), (6, 14), (6, 0),
    ], width: 10 }),
    // 53 5
    Some(G { points: &[
        (9, 14), (1, 14), (1, 8), (8, 8), (9, 6), (9, 0), (1, 0),
    ], width: 10 }),
    // 54 6
    Some(G { points: &[
        (9, 12), (8, 14), (2, 14), (1, 12), (1, 2), (2, 0), (7, 0), (9, 2), (9, 6),
        (2, 6),
    ], width: 10 }),
    // 55 7
    Some(G { points: &[
        (1, 14), (9, 14), (9, 10), (6, 0),
    ], width: 10 }),
    // 56 8
    Some(G { points: &[
        (2, 14), (8, 14), (9, 12), (9, 9), (8, 7), (2, 7), (1, 9), (1, 12), (2, 14),
        (-128, -128), (2, 7), (8, 7), (9, 5), (9, 2), (8, 0), (2, 0), (1, 2), (1, 5), (2, 7),
    ], width: 10 }),
    // 57 9
    Some(G { points: &[
        (8, 8), (1, 8), (1, 12), (2, 14), (8, 14), (9, 12), (9, 2), (8, 0), (3, 0), (1, 2),
    ], width: 10 }),
    // 58 :
    Some(G { points: &[(4, 0), (4, 0), (-128, -128), (4, 8), (4, 8)], width: 5 }),
    // 59 ;
    Some(G { points: &[(4, 0), (4, -3), (-128, -128), (4, 7), (4, 7)], width: 5 }),
    // 60 <
    Some(G { points: &[(9, 12), (1, 7), (9, 2)], width: 10 }),
    // 61 =
    Some(G { points: &[(1, 9), (9, 9), (-128, -128), (1, 5), (9, 5)], width: 10 }),
    // 62 >
    Some(G { points: &[(1, 12), (9, 7), (1, 2)], width: 10 }),
    // 63 ?
    Some(G { points: &[
        (1, 12), (2, 14), (8, 14), (9, 12), (9, 9), (6, 7), (6, 5),
        (-128, -128), (6, 0), (6, 0),
    ], width: 10 }),
    // 64 @
    Some(G { points: &[
        (7, 10), (6, 12), (3, 12), (1, 10), (1, 5), (3, 3), (6, 3), (8, 5), (8, 8),
        (6, 8), (5, 6),
    ], width: 12 }),
    // 65 A
    Some(G { points: &[
        (0, 0), (5, 14), (10, 0),
        (-128, -128), (1, 5), (9, 5),
    ], width: 10 }),
    // 66 B
    Some(G { points: &[
        (1, 0), (1, 14), (8, 14), (10, 12), (10, 9), (8, 7), (1, 7),
        (-128, -128), (1, 7), (8, 7), (10, 5), (10, 2), (8, 0), (1, 0),
    ], width: 11 }),
    // 67 C
    Some(G { points: &[
        (10, 12), (8, 14), (2, 14), (0, 12), (0, 2), (2, 0), (8, 0), (10, 2),
    ], width: 10 }),
    // 68 D
    Some(G { points: &[
        (1, 0), (1, 14), (6, 14), (10, 11), (10, 3), (6, 0), (1, 0),
    ], width: 11 }),
    // 69 E
    Some(G { points: &[
        (10, 14), (1, 14), (1, 0), (10, 0),
        (-128, -128), (1, 7), (8, 7),
    ], width: 10 }),
    // 70 F
    Some(G { points: &[
        (10, 14), (1, 14), (1, 0),
        (-128, -128), (1, 7), (8, 7),
    ], width: 10 }),
    // 71 G
    Some(G { points: &[
        (10, 12), (8, 14), (2, 14), (0, 12), (0, 2), (2, 0), (8, 0), (10, 2), (10, 7),
        (6, 7),
    ], width: 11 }),
    // 72 H
    Some(G { points: &[
        (1, 0), (1, 14), (-128, -128), (9, 0), (9, 14),
        (-128, -128), (1, 7), (9, 7),
    ], width: 10 }),
    // 73 I
    Some(G { points: &[
        (4, 0), (4, 14), (-128, -128), (2, 14), (6, 14), (-128, -128), (2, 0), (6, 0),
    ], width: 8 }),
    // 74 J
    Some(G { points: &[
        (8, 14), (8, 3), (7, 0), (3, 0), (2, 2),
    ], width: 10 }),
    // 75 K
    Some(G { points: &[
        (1, 0), (1, 14), (-128, -128), (9, 14), (1, 7), (9, 0),
    ], width: 10 }),
    // 76 L
    Some(G { points: &[
        (1, 14), (1, 0), (10, 0),
    ], width: 10 }),
    // 77 M
    Some(G { points: &[
        (0, 0), (0, 14), (5, 2), (10, 14), (10, 0),
    ], width: 10 }),
    // 78 N
    Some(G { points: &[
        (1, 0), (1, 14), (9, 0), (9, 14),
    ], width: 10 }),
    // 79 O
    Some(G { points: &[
        (2, 14), (8, 14), (10, 12), (10, 2), (8, 0), (2, 0), (0, 2), (0, 12), (2, 14),
    ], width: 10 }),
    // 80 P
    Some(G { points: &[
        (1, 0), (1, 14), (8, 14), (10, 12), (10, 9), (8, 7), (1, 7),
    ], width: 10 }),
    // 81 Q
    Some(G { points: &[
        (2, 14), (8, 14), (10, 12), (10, 2), (8, 0), (2, 0), (0, 2), (0, 12), (2, 14),
        (-128, -128), (6, 4), (10, 0),
    ], width: 10 }),
    // 82 R
    Some(G { points: &[
        (1, 0), (1, 14), (8, 14), (10, 12), (10, 9), (8, 7), (1, 7),
        (-128, -128), (1, 7), (10, 0),
    ], width: 11 }),
    // 83 S
    Some(G { points: &[
        (10, 12), (8, 14), (2, 14), (0, 12), (0, 9), (2, 7), (8, 7), (10, 5), (10, 2),
        (8, 0), (2, 0), (0, 2),
    ], width: 10 }),
    // 84 T
    Some(G { points: &[
        (5, 0), (5, 14), (-128, -128), (0, 14), (10, 14),
    ], width: 10 }),
    // 85 U
    Some(G { points: &[
        (1, 14), (1, 3), (3, 0), (7, 0), (9, 3), (9, 14),
    ], width: 10 }),
    // 86 V
    Some(G { points: &[
        (0, 14), (5, 0), (10, 14),
    ], width: 10 }),
    // 87 W
    Some(G { points: &[
        (0, 14), (2, 0), (5, 10), (8, 0), (10, 14),
    ], width: 10 }),
    // 88 X
    Some(G { points: &[
        (0, 14), (10, 0), (-128, -128), (10, 14), (0, 0),
    ], width: 10 }),
    // 89 Y
    Some(G { points: &[
        (0, 14), (5, 7), (10, 14), (-128, -128), (5, 7), (5, 0),
    ], width: 10 }),
    // 90 Z
    Some(G { points: &[
        (0, 14), (10, 14), (0, 0), (10, 0),
    ], width: 10 }),
    // 91 [
    Some(G { points: &[
        (5, 15), (3, 15), (3, -3), (5, -3),
    ], width: 7 }),
    // 92 backslash
    Some(G { points: &[(2, 14), (8, 0)], width: 10 }),
    // 93 ]
    Some(G { points: &[
        (3, 15), (5, 15), (5, -3), (3, -3),
    ], width: 7 }),
    // 94 ^
    Some(G { points: &[(1, 10), (5, 14), (9, 10)], width: 10 }),
    // 95 _
    Some(G { points: &[(0, -1), (10, -1)], width: 10 }),
    // 96 `
    Some(G { points: &[(5, 12), (3, 10)], width: 6 }),
    // 97 a
    Some(G { points: &[
        (8, 7), (8, 0), (-128, -128), (8, 5), (6, 7), (2, 7), (0, 5), (0, 2), (2, 0),
        (8, 0),
    ], width: 8 }),
    // 98 b
    Some(G { points: &[
        (2, 10), (2, 0), (-128, -128), (2, 5), (4, 7), (7, 7), (9, 5), (9, 2), (7, 0),
        (2, 0),
    ], width: 9 }),
    // 99 c
    Some(G { points: &[
        (9, 5), (7, 7), (3, 7), (1, 5), (1, 2), (3, 0), (7, 0), (9, 2),
    ], width: 8 }),
    // 100 d
    Some(G { points: &[
        (8, 10), (8, 0), (-128, -128), (8, 5), (6, 7), (3, 7), (1, 5), (1, 2), (3, 0),
        (8, 0),
    ], width: 9 }),
    // 101 e
    Some(G { points: &[
        (1, 3), (9, 3), (9, 5), (7, 7), (3, 7), (1, 5), (1, 2), (3, 0), (7, 0), (9, 2),
    ], width: 8 }),
    // 102 f
    Some(G { points: &[
        (8, 10), (6, 10), (4, 8), (4, 0), (-128, -128), (2, 6), (7, 6),
    ], width: 7 }),
    // 103 g
    Some(G { points: &[
        (8, 7), (8, -2), (6, -4), (2, -4), (0, -2),
        (-128, -128), (8, 5), (6, 7), (2, 7), (0, 5), (0, 2), (2, 0), (8, 0),
    ], width: 8 }),
    // 104 h
    Some(G { points: &[
        (2, 10), (2, 0), (-128, -128), (2, 5), (4, 7), (7, 7), (8, 5), (8, 0),
    ], width: 8 }),
    // 105 i
    Some(G { points: &[
        (4, 10), (4, 0), (-128, -128), (4, 12), (4, 12),
    ], width: 5 }),
    // 106 j
    Some(G { points: &[
        (6, 10), (6, -2), (5, -4), (2, -4),
        (-128, -128), (6, 12), (6, 12),
    ], width: 6 }),
    // 107 k
    Some(G { points: &[
        (3, 10), (3, 0), (-128, -128), (8, 7), (3, 4), (7, 0),
    ], width: 8 }),
    // 108 l
    Some(G { points: &[
        (4, 10), (4, 0), (-128, -128), (2, 10), (6, 10),
    ], width: 6 }),
    // 109 m
    Some(G { points: &[
        (0, 7), (0, 0), (-128, -128), (0, 5), (2, 7), (4, 7), (5, 5), (5, 0),
        (-128, -128), (5, 5), (7, 7), (9, 7), (10, 5), (10, 0),
    ], width: 10 }),
    // 110 n
    Some(G { points: &[
        (0, 7), (0, 0), (-128, -128), (0, 5), (2, 7), (6, 7), (8, 5), (8, 0),
    ], width: 8 }),
    // 111 o
    Some(G { points: &[
        (2, 7), (7, 7), (9, 5), (9, 2), (7, 0), (2, 0), (0, 5), (0, 2), (2, 7),
    ], width: 9 }),
    // 112 p
    Some(G { points: &[
        (0, 7), (0, -4), (-128, -128), (0, 5), (2, 7), (6, 7), (8, 5), (8, 2), (6, 0),
        (0, 0),
    ], width: 8 }),
    // 113 q
    Some(G { points: &[
        (8, 7), (8, -4), (-128, -128), (8, 5), (6, 7), (2, 7), (0, 5), (0, 2), (2, 0),
        (8, 0),
    ], width: 8 }),
    // 114 r
    Some(G { points: &[
        (0, 7), (0, 0), (-128, -128), (0, 5), (3, 7), (5, 7),
    ], width: 6 }),
    // 115 s
    Some(G { points: &[
        (8, 5), (6, 7), (2, 7), (0, 5), (0, 3), (2, 1), (7, 1), (9, -1), (7, -3),
        (2, -3),
    ], width: 8 }),
    // 116 t
    Some(G { points: &[
        (4, 10), (4, 0), (-128, -128), (2, 7), (7, 7), (-128, -128), (6, 0), (2, 0),
    ], width: 7 }),
    // 117 u
    Some(G { points: &[
        (0, 7), (0, 2), (2, 0), (6, 0), (8, 2), (8, 7), (-128, -128), (8, 7), (8, 0),
    ], width: 8 }),
    // 118 v
    Some(G { points: &[
        (0, 7), (4, 0), (8, 7),
    ], width: 8 }),
    // 119 w
    Some(G { points: &[
        (0, 7), (1, 0), (4, 5), (6, 0), (8, 7),
    ], width: 8 }),
    // 120 x
    Some(G { points: &[
        (0, 7), (8, 0), (-128, -128), (8, 7), (0, 0),
    ], width: 8 }),
    // 121 y
    Some(G { points: &[
        (0, 7), (4, 0), (8, 7),
        (-128, -128), (4, 0), (4, -4),
    ], width: 8 }),
    // 122 z
    Some(G { points: &[
        (0, 7), (8, 7), (0, 0), (8, 0),
    ], width: 8 }),
    // 123 {
    Some(G { points: &[
        (5, 15), (3, 14), (3, 9), (1, 7), (3, 5), (3, 0), (5, -1),
    ], width: 6 }),
    // 124 |
    Some(G { points: &[(4, 14), (4, -4)], width: 4 }),
    // 125 }
    Some(G { points: &[
        (1, 15), (3, 14), (3, 9), (5, 7), (3, 5), (3, 0), (1, -1),
    ], width: 6 }),
    // 126 ~
    Some(G { points: &[
        (1, 6), (3, 8), (7, 4), (9, 6),
    ], width: 10 }),
];

const GLYPH_WIDTHS: &[i8] = &[
    8, 8, 8, 10, 10, 10, 10, 5, 7, 7, 10, 10, 5, 10, 5, 10,
    10, 10, 10, 10, 10, 10, 10, 10, 10, 10,
    10, 8, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 11,
    10, 11, 10, 10, 10, 10, 10, 11, 10, 10,
    10, 11, 10, 10, 10, 10, 10, 10, 10, 10,
    10, 7, 10, 7, 10, 10, 10, 8,
    8, 9, 8, 9, 8, 7, 8, 8, 5, 6, 8, 6, 10, 8, 9, 8,
    8, 8, 7, 8, 8, 8, 8, 8, 8, 8,
    6, 4, 6, 10,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_char() {
        let paths = render_char('A', 0.0, 0.0, 10.0);
        assert!(!paths.is_empty(), "Should produce paths for 'A'");
        let total_pts: usize = paths.iter().map(|p| p.points.len()).sum();
        assert!(total_pts >= 3, "Should have at least 3 points");
    }

    #[test]
    fn test_render_text() {
        let paths = render_text("HELLO", 0.0, 0.0, 10.0, 1.0);
        assert!(!paths.is_empty());
    }

    #[test]
    fn test_char_widths() {
        assert!(char_width('A') > 0);
        assert_eq!(char_width(' '), 8);
    }

    #[test]
    fn test_all_chars() {
        for c in (32u8..127u8).map(|b| b as char) {
            let paths = render_char(c, 0.0, 0.0, 10.0);
            // All defined chars should produce some output or be space
            if c != ' ' {
                assert!(paths.len() > 0 || c == ' ' || (c as usize - 32) >= GLYPHS.len() || GLYPHS[c as usize - 32].is_none(),
                    "Character '{}' should produce paths", c);
            }
        }
    }
}
