use crate::ui::UiQuad;

const W: usize = 5;
const H: usize = 7;

const fn enc(rows: [&'static [u8]; H]) -> u64 {
    let mut val: u64 = 0;
    let mut row = 0;
    while row < H {
        let bytes = rows[row];
        let mut col = 0;
        while col < W {
            if col < bytes.len() && bytes[col] == b'1' {
                val |= 1u64 << (row * W + col);
            }
            col += 1;
        }
        row += 1;
    }
    val
}

const A: u64 = enc([b"01110", b"10001", b"10001", b"11111", b"10001", b"10001", b"10001"]);
const B: u64 = enc([b"11110", b"10001", b"10001", b"11110", b"10001", b"10001", b"11110"]);
const C: u64 = enc([b"01110", b"10001", b"10000", b"10000", b"10000", b"10001", b"01110"]);
const D: u64 = enc([b"11110", b"10001", b"10001", b"10001", b"10001", b"10001", b"11110"]);
const E: u64 = enc([b"11111", b"10000", b"10000", b"11110", b"10000", b"10000", b"11111"]);
const F: u64 = enc([b"11111", b"10000", b"10000", b"11110", b"10000", b"10000", b"10000"]);
const G: u64 = enc([b"01110", b"10001", b"10000", b"10111", b"10001", b"10001", b"01110"]);
const H_: u64 = enc([b"10001", b"10001", b"10001", b"11111", b"10001", b"10001", b"10001"]);
const I: u64 = enc([b"01110", b"00100", b"00100", b"00100", b"00100", b"00100", b"01110"]);
const J: u64 = enc([b"00111", b"00010", b"00010", b"00010", b"00010", b"10010", b"01100"]);
const K: u64 = enc([b"10001", b"10010", b"10100", b"11000", b"10100", b"10010", b"10001"]);
const L: u64 = enc([b"10000", b"10000", b"10000", b"10000", b"10000", b"10000", b"11111"]);
const M: u64 = enc([b"10001", b"11011", b"10101", b"10101", b"10001", b"10001", b"10001"]);
const N: u64 = enc([b"10001", b"10001", b"11001", b"10101", b"10011", b"10001", b"10001"]);
const O: u64 = enc([b"01110", b"10001", b"10001", b"10001", b"10001", b"10001", b"01110"]);
const P: u64 = enc([b"11110", b"10001", b"10001", b"11110", b"10000", b"10000", b"10000"]);
const Q: u64 = enc([b"01110", b"10001", b"10001", b"10001", b"10101", b"10010", b"01101"]);
const R: u64 = enc([b"11110", b"10001", b"10001", b"11110", b"10100", b"10010", b"10001"]);
const S: u64 = enc([b"01111", b"10000", b"10000", b"01110", b"00001", b"00001", b"11110"]);
const T: u64 = enc([b"11111", b"00100", b"00100", b"00100", b"00100", b"00100", b"00100"]);
const U: u64 = enc([b"10001", b"10001", b"10001", b"10001", b"10001", b"10001", b"01110"]);
const V: u64 = enc([b"10001", b"10001", b"10001", b"10001", b"10001", b"01010", b"00100"]);
const W_: u64 = enc([b"10001", b"10001", b"10001", b"10101", b"10101", b"10101", b"01010"]);
const X: u64 = enc([b"10001", b"10001", b"01010", b"00100", b"01010", b"10001", b"10001"]);
const Y: u64 = enc([b"10001", b"10001", b"10001", b"01010", b"00100", b"00100", b"00100"]);
const Z: u64 = enc([b"11111", b"00001", b"00010", b"00100", b"01000", b"10000", b"11111"]);

const N0: u64 = enc([b"01110", b"10001", b"10011", b"10101", b"11001", b"10001", b"01110"]);
const N1: u64 = enc([b"00100", b"01100", b"00100", b"00100", b"00100", b"00100", b"01110"]);
const N2: u64 = enc([b"01110", b"10001", b"00001", b"00010", b"00100", b"01000", b"11111"]);
const N3: u64 = enc([b"11111", b"00010", b"00100", b"00010", b"00001", b"10001", b"01110"]);
const N4: u64 = enc([b"00010", b"00110", b"01010", b"10010", b"11111", b"00010", b"00010"]);
const N5: u64 = enc([b"11111", b"10000", b"11110", b"00001", b"00001", b"10001", b"01110"]);
const N6: u64 = enc([b"00110", b"01000", b"10000", b"11110", b"10001", b"10001", b"01110"]);
const N7: u64 = enc([b"11111", b"00001", b"00010", b"00100", b"01000", b"01000", b"01000"]);
const N8: u64 = enc([b"01110", b"10001", b"10001", b"01110", b"10001", b"10001", b"01110"]);
const N9: u64 = enc([b"01110", b"10001", b"10001", b"01111", b"00001", b"00010", b"01100"]);

const COMMA: u64 = enc([b"00000", b"00000", b"00000", b"00000", b"00100", b"00100", b"01000"]);
const DOT: u64 = enc([b"00000", b"00000", b"00000", b"00000", b"00000", b"01100", b"01100"]);
const COLON: u64 = enc([b"00000", b"01100", b"01100", b"00000", b"01100", b"01100", b"00000"]);
const DASH: u64 = enc([b"00000", b"00000", b"00000", b"11111", b"00000", b"00000", b"00000"]);
const UNDERSCORE: u64 = enc([b"00000", b"00000", b"00000", b"00000", b"00000", b"00000", b"11111"]);
const EXCLAIM: u64 = enc([b"00100", b"00100", b"00100", b"00100", b"00100", b"00000", b"00100"]);
const QUESTION: u64 = enc([b"01110", b"10001", b"00010", b"00100", b"00100", b"00000", b"00100"]);

fn glyph(c: char) -> Option<u64> {
    let g = match c {
        'A' => A, 'B' => B, 'C' => C, 'D' => D, 'E' => E, 'F' => F, 'G' => G,
        'H' => H_, 'I' => I, 'J' => J, 'K' => K, 'L' => L, 'M' => M, 'N' => N,
        'O' => O, 'P' => P, 'Q' => Q, 'R' => R, 'S' => S, 'T' => T, 'U' => U,
        'V' => V, 'W' => W_, 'X' => X, 'Y' => Y, 'Z' => Z,
        '0' => N0, '1' => N1, '2' => N2, '3' => N3, '4' => N4,
        '5' => N5, '6' => N6, '7' => N7, '8' => N8, '9' => N9,
        ',' => COMMA, '.' => DOT, ':' => COLON, '-' => DASH, '_' => UNDERSCORE,
        '!' => EXCLAIM, '?' => QUESTION,
        _ => return None,
    };
    Some(g)
}

fn norm(c: char) -> char {
    if c.is_ascii_lowercase() {
        c.to_ascii_uppercase()
    } else {
        c
    }
}

pub fn measure(text: &str, scale: f32) -> f32 {
    let mut w = 0.0;
    let space = (W as f32 + 1.0) * scale;
    for c in text.chars() {
        if c == ' ' {
            w += space;
        } else if glyph(norm(c)).is_some() {
            w += space;
        }
    }
    (w - scale).max(0.0)
}

pub fn build_text_quads(
    text: &str,
    x: f32,
    y: f32,
    scale: f32,
    color: [f32; 4],
    out: &mut Vec<UiQuad>,
) {
    let mut cx = x;
    let advance = (W as f32 + 1.0) * scale;
    for ch in text.chars() {
        if ch == ' ' {
            cx += advance;
            continue;
        }
        let Some(g) = glyph(norm(ch)) else {
            cx += advance;
            continue;
        };
        for row in 0..H {
            for col in 0..W {
                let bit = (g >> (row * W + col)) & 1;
                if bit == 1 {
                    out.push(UiQuad {
                        pos: [cx + col as f32 * scale, y + row as f32 * scale],
                        size: [scale, scale],
                        color,
                        radius: 0.0,
                        ..Default::default()
                    });
                }
            }
        }
        cx += advance;
    }
}
