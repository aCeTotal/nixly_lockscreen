use crate::ui::UiQuad;

const SEG_DIGITS: [u8; 10] = [
    0b0111111, // 0: a b c d e f
    0b0000110, // 1: b c
    0b1011011, // 2: a b d e g
    0b1001111, // 3: a b c d g
    0b1100110, // 4: b c f g
    0b1101101, // 5: a c d f g
    0b1111101, // 6: a c d e f g
    0b0000111, // 7: a b c
    0b1111111, // 8: all
    0b1101111, // 9: a b c d f g
];

pub fn digit_width(height: f32) -> f32 {
    height * 0.55
}

fn seg_quad(x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) -> UiQuad {
    let r = (w.min(h)) * 0.45;
    UiQuad {
        pos: [x, y],
        size: [w, h],
        color,
        radius: r,
        ..Default::default()
    }
}

fn push_digit(d: u8, x: f32, y: f32, w: f32, h: f32, color: [f32; 4], out: &mut Vec<UiQuad>) {
    let mask = SEG_DIGITS[d as usize % 10];
    let dim_color = [color[0] * 0.10, color[1] * 0.10, color[2] * 0.10, color[3] * 0.20];

    let t = (w.min(h) * 0.14).max(2.0);
    let half = (h - t) * 0.5;
    let inner_w = w - t * 2.0;
    let inner_v = half - t * 0.5;

    let segs = [
        // a: top
        (x + t, y, inner_w, t),
        // b: top-right
        (x + w - t, y + t * 0.5, t, inner_v),
        // c: bot-right
        (x + w - t, y + half + t * 0.5, t, inner_v),
        // d: bottom
        (x + t, y + h - t, inner_w, t),
        // e: bot-left
        (x, y + half + t * 0.5, t, inner_v),
        // f: top-left
        (x, y + t * 0.5, t, inner_v),
        // g: middle
        (x + t, y + half, inner_w, t),
    ];

    for (i, (sx, sy, sw, sh)) in segs.iter().enumerate() {
        let on = (mask >> i) & 1 == 1;
        let c = if on { color } else { dim_color };
        out.push(seg_quad(*sx, *sy, *sw, *sh, c));
    }
}

fn push_colon(x: f32, y: f32, w: f32, h: f32, color: [f32; 4], out: &mut Vec<UiQuad>) {
    let dot = (w.min(h * 0.18)).max(4.0);
    let cx = x + w * 0.5 - dot * 0.5;
    let dy_top = y + h * 0.30 - dot * 0.5;
    let dy_bot = y + h * 0.70 - dot * 0.5;
    out.push(UiQuad {
        pos: [cx, dy_top],
        size: [dot, dot],
        color,
        radius: dot * 0.5,
        ..Default::default()
    });
    out.push(UiQuad {
        pos: [cx, dy_bot],
        size: [dot, dot],
        color,
        radius: dot * 0.5,
        ..Default::default()
    });
}

pub fn build(
    hour: u8,
    minute: u8,
    cx: f32,
    cy: f32,
    height: f32,
    color: [f32; 4],
    colon_color: [f32; 4],
    out: &mut Vec<UiQuad>,
) {
    let dw = digit_width(height);
    let gap = height * 0.06;
    let colon_w = height * 0.22;
    let total_w = dw * 4.0 + gap * 4.0 + colon_w;
    let mut x = cx - total_w * 0.5;
    let y = cy - height * 0.5;

    let h_tens = hour / 10;
    let h_ones = hour % 10;
    let m_tens = minute / 10;
    let m_ones = minute % 10;

    push_digit(h_tens, x, y, dw, height, color, out);
    x += dw + gap;
    push_digit(h_ones, x, y, dw, height, color, out);
    x += dw + gap;
    push_colon(x, y, colon_w, height, colon_color, out);
    x += colon_w + gap;
    push_digit(m_tens, x, y, dw, height, color, out);
    x += dw + gap;
    push_digit(m_ones, x, y, dw, height, color, out);
}
