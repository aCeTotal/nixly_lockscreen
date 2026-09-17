struct VsOut {
    @builtin(position) pos: vec4<f32>,
};

struct Uniforms {
    time: f32,
    seed: f32,
    res: vec2<f32>,
    scale: f32,
    style: f32,
    _pad1: f32,
    _pad2: f32,
};

@group(0) @binding(0) var<uniform> u: Uniforms;

@vertex
fn vs_main(@builtin(vertex_index) vid: u32) -> VsOut {
    var p = array<vec2<f32>, 3>(
        vec2(-1.0, -1.0),
        vec2( 3.0, -1.0),
        vec2(-1.0,  3.0),
    );
    var out: VsOut;
    out.pos = vec4(p[vid], 0.0, 1.0);
    return out;
}

// Both planes render into this full-res texture so the composite's bloom
// taps glow around EVERYTHING — the movie look is mostly that glow.
//
// Far plane: dense wall of small dim columns, nearly every lane filled,
// long trails so the background reads as a continuous glyph curtain.
// Near plane: its own sparse grid of big fast bright columns on top.
const LANE_W: f32 = 20.0;

// Per-style tables (style 0..3):
// 0 classic dense  1 deep DOF  2 neon glow  3 dark cinematic
var<private> LANE_EMPTY_S:  array<f32, 4> = array<f32, 4>(0.02, 0.03, 0.02, 0.22);
var<private> NLANE_EMPTY_S: array<f32, 4> = array<f32, 4>(0.42, 0.58, 0.48, 0.62);
var<private> FARB_S:  array<f32, 4> = array<f32, 4>(1.00, 0.60, 0.80, 0.50);
var<private> NEARB_S: array<f32, 4> = array<f32, 4>(1.60, 1.90, 1.95, 2.10);
var<private> NSCALE_S: array<f32, 4> = array<f32, 4>(1.00, 1.35, 1.30, 1.25);

var<private> CELL_W: array<f32, 4> = array<f32, 4>(7.0, 9.0, 12.0, 16.0);
var<private> CELL_H: array<f32, 4> = array<f32, 4>(11.0, 14.0, 19.0, 25.0);
var<private> DEPTH: array<f32, 4> = array<f32, 4>(0.12, 0.32, 0.60, 1.00);
var<private> BRIGHT: array<f32, 4> = array<f32, 4>(0.42, 0.58, 0.80, 1.05);
var<private> SALT: array<f32, 4> = array<f32, 4>(811.0, 521.0, 137.0, 271.0);
var<private> RAMP: array<f32, 4> = array<f32, 4>(0.0, 0.0, 1.0, 2.0);

const SPAWN_DEADLINE: f32 = 16.0;
const SPAWN_FADE: f32 = 0.6;
const GLYPH_COUNT: u32 = 32u;

// Near plane
const NLANE_W: f32 = 52.0;
const NCELL_W: f32 = 24.0;
const NCELL_H: f32 = 38.0;
const NSPAWN_DEADLINE: f32 = 8.0;
const NSPAWN_FADE: f32 = 0.5;

var<private> GLYPHS: array<u32, 224> = array<u32, 224>(
    0x02u, 0x02u, 0x1Fu, 0x02u, 0x02u, 0x02u, 0x02u,
    0x15u, 0x15u, 0x15u, 0x15u, 0x15u, 0x15u, 0x15u,
    0x10u, 0x08u, 0x04u, 0x02u, 0x01u, 0x02u, 0x04u,
    0x01u, 0x03u, 0x05u, 0x09u, 0x05u, 0x03u, 0x01u,
    0x1Fu, 0x04u, 0x04u, 0x0Eu, 0x04u, 0x04u, 0x0Eu,
    0x0Au, 0x11u, 0x00u, 0x0Au, 0x00u, 0x11u, 0x0Au,
    0x1Eu, 0x02u, 0x02u, 0x02u, 0x02u, 0x02u, 0x1Eu,
    0x0Fu, 0x08u, 0x08u, 0x08u, 0x08u, 0x08u, 0x0Fu,
    0x10u, 0x08u, 0x0Cu, 0x06u, 0x02u, 0x01u, 0x10u,
    0x04u, 0x0Au, 0x11u, 0x0Au, 0x04u, 0x0Au, 0x11u,
    0x0Au, 0x0Au, 0x1Fu, 0x0Au, 0x1Fu, 0x0Au, 0x0Au,
    0x02u, 0x0Fu, 0x10u, 0x0Au, 0x11u, 0x10u, 0x0Au,
    0x0Eu, 0x11u, 0x10u, 0x0Cu, 0x02u, 0x01u, 0x1Fu,
    0x1Fu, 0x10u, 0x10u, 0x10u, 0x10u, 0x10u, 0x1Fu,
    0x07u, 0x08u, 0x16u, 0x09u, 0x0Au, 0x0Cu, 0x10u,
    0x04u, 0x0Au, 0x15u, 0x04u, 0x04u, 0x04u, 0x04u,
    0x04u, 0x04u, 0x04u, 0x04u, 0x15u, 0x0Au, 0x04u,
    0x01u, 0x03u, 0x02u, 0x06u, 0x04u, 0x0Cu, 0x08u,
    0x15u, 0x0Au, 0x15u, 0x0Au, 0x15u, 0x0Au, 0x15u,
    0x11u, 0x0Au, 0x04u, 0x04u, 0x04u, 0x0Au, 0x11u,
    0x0Cu, 0x12u, 0x02u, 0x02u, 0x02u, 0x12u, 0x0Cu,
    0x1Fu, 0x0Au, 0x04u, 0x0Au, 0x04u, 0x0Au, 0x1Fu,
    0x05u, 0x0Au, 0x09u, 0x12u, 0x05u, 0x0Au, 0x09u,
    0x12u, 0x12u, 0x1Fu, 0x12u, 0x0Au, 0x12u, 0x11u,
    0x11u, 0x11u, 0x15u, 0x15u, 0x1Bu, 0x11u, 0x00u,
    0x1Eu, 0x01u, 0x01u, 0x01u, 0x02u, 0x04u, 0x18u,
    0x0Eu, 0x11u, 0x11u, 0x1Fu, 0x11u, 0x11u, 0x0Eu,
    0x15u, 0x15u, 0x15u, 0x0Eu, 0x04u, 0x04u, 0x04u,
    0x10u, 0x08u, 0x1Eu, 0x04u, 0x0Fu, 0x02u, 0x01u,
    0x1Cu, 0x02u, 0x01u, 0x01u, 0x01u, 0x02u, 0x1Cu,
    0x0Au, 0x15u, 0x0Au, 0x1Fu, 0x0Au, 0x15u, 0x0Au,
    0x01u, 0x0Fu, 0x10u, 0x0Eu, 0x01u, 0x10u, 0x0Eu,
);

fn hash11(p: f32) -> f32 {
    var b = bitcast<u32>(p) * 0x27d4eb2du;
    b = b ^ (b >> 15u);
    b = b * 0x9e3779b1u;
    b = b ^ (b >> 13u);
    b = b * 0x85ebca77u;
    b = b ^ (b >> 16u);
    return f32(b & 0x00FFFFFFu) / 16777216.0;
}

fn glyph_bit(idx: u32, x: i32, y: i32) -> f32 {
    if (x < 0 || x > 4 || y < 0 || y > 6) {
        return 0.0;
    }
    let bits = GLYPHS[idx * 7u + u32(y)];
    return f32((bits >> u32(x)) & 1u);
}

// Bilinear coverage over the 5x7 bitmap, re-sharpened: ~1px AA edge.
fn glyph_cov(idx: u32, gx: f32, gy: f32) -> f32 {
    let sx = gx - 0.5;
    let sy = gy - 0.5;
    let ix = i32(floor(sx));
    let iy = i32(floor(sy));
    let fx = fract(sx);
    let fy = fract(sy);
    let b00 = glyph_bit(idx, ix, iy);
    let b10 = glyph_bit(idx, ix + 1, iy);
    let b01 = glyph_bit(idx, ix, iy + 1);
    let b11 = glyph_bit(idx, ix + 1, iy + 1);
    let cov = mix(mix(b00, b10, fx), mix(b01, b11, fx), fy);
    return smoothstep(0.40, 0.60, cov);
}

fn far_field(px: vec2<f32>) -> vec3<f32> {
    let si = min(u32(u.style + 0.5), 3u);
    let lane_empty = LANE_EMPTY_S[si];
    let lane = floor(px.x / LANE_W);
    let lane_r = hash11(lane * 91.17 + u.seed * 0.77);
    if (lane_r < lane_empty) {
        return vec3<f32>(0.0);
    }
    let pick = (lane_r - lane_empty) / (1.0 - lane_empty);
    var li: u32;
    if (pick < 0.30) {
        li = 0u;
    } else if (pick < 0.57) {
        li = 1u;
    } else if (pick < 0.82) {
        li = 2u;
    } else {
        li = 3u;
    }

    let cw = CELL_W[li];
    let ch = CELL_H[li];
    let depth = DEPTH[li];
    let salt_j = SALT[li] + lane * 173.7;

    let spawn_rand = hash11(salt_j + 17.3 + u.seed * 0.41);
    let window = max(SPAWN_DEADLINE - RAMP[li], 6.0);
    let spawn = RAMP[li] + sqrt(spawn_rand) * window;
    if (u.time < spawn) {
        return vec3<f32>(0.0);
    }
    let fade = smoothstep(spawn, spawn + SPAWN_FADE, u.time);

    let margin = (LANE_W - cw) * 0.5;
    let sway = sin(u.time * 0.22 + lane * 0.9) * max(margin - 1.0, 0.0) * depth;
    let cell_x0 = lane * LANE_W + margin + sway;
    let u_x = px.x - cell_x0;
    if (u_x < 0.0 || u_x >= cw) {
        return vec3<f32>(0.0);
    }

    let speed_pick = hash11(salt_j + 9.91 + u.seed * 0.07);
    var dur: f32;
    if (speed_pick < 0.7) {
        dur = 4.5 + (speed_pick / 0.7) * 2.5;
    } else {
        dur = 3.0 + ((speed_pick - 0.7) / 0.3) * 1.5;
    }
    // Far plane drifts slower than the near plane so the speed gap reads
    // as distance.
    dur = dur * (2.1 - 0.8 * depth);

    let local_t = u.time - spawn;
    let ci = floor(local_t / dur);
    let age = local_t - ci * dur;
    let s = salt_j + ci * 31.7 + u.seed * 1.31;

    // Long trails: the background should read as a nearly continuous
    // curtain, not short drops with black gaps.
    let drop_len = u.res.y * (1.1 + hash11(s + 3.0) * 1.1);
    let speed = (u.res.y + drop_len + ch * 2.0) / dur;
    let head_y = age * speed;
    let head_row = head_y / ch;
    let length_rows = drop_len / ch;
    let row = floor(px.y / ch);
    let dist = head_row - row;

    var c = vec3<f32>(0.0);

    if (row <= head_row + 0.5 && dist < length_rows) {
        let si = 0.04 + hash11(s + 0.51) * 0.04;
        c = c + vec3(0.015, 0.18, 0.04) * si;
    }

    if (dist >= -0.5 && dist < length_rows) {
        let head_white = 1.0 - smoothstep(0.0, 0.85, dist);
        let head_green = pow(clamp(1.0 - dist / 4.0, 0.0, 1.0), 2.0);
        let trail_t = smoothstep(0.0, 0.5, dist) * (1.0 - dist / length_rows);
        let trail_smooth = pow(max(trail_t, 0.0), 1.2);
        let glyph_rate = 1.5 + hash11(s + 4.0) * 3.5;
        let glyph_tick = floor(u.time * glyph_rate + row * 7.0);
        let glyph_idx = u32(hash11(glyph_tick + row * 0.97 + s) * f32(GLYPH_COUNT)) % GLYPH_COUNT;
        let flicker = 0.72 + hash11(floor(u.time * 9.0 + row * 11.3 + s)) * 0.28;

        // Occasional "hot" cycles: a whole column lights up bright the way
        // random streams flare in the film.
        let hot = 1.0 + 1.3 * step(0.80, hash11(s + 7.7));

        var tr = 2.5 * head_white + 0.05 * head_green + 0.16 * trail_smooth * flicker * hot;
        var tg = 2.5 * head_white + 1.0 * head_green + 1.35 * trail_smooth * flicker * hot;
        var tb = 2.5 * head_white + 0.15 * head_green + 0.42 * trail_smooth * flicker * hot;

        if (dist > 1.0 && hash11(floor(u.time * 3.0 + row * 23.0 + s) + 0.31) > 0.985) {
            tr = max(tr, 1.5);
            tg = max(tg, 1.9);
            tb = max(tb, 1.5);
        }

        let defocus = 1.0 - depth;
        c = c + vec3(tr, tg, tb) * defocus * 0.20;
        let sharp = 0.45 + 0.55 * depth;

        let cell_y0 = row * ch;
        let gsx = cw * 0.72;
        let gsy = ch * 0.78;
        let lx = u_x - (cw - gsx) * 0.5;
        let ly = px.y - cell_y0 - (ch - gsy) * 0.5;
        if (lx >= 0.0 && lx < gsx && ly >= 0.0 && ly < gsy) {
            let gx = u32(lx / gsx * 5.0);
            let gy = u32(ly / gsy * 7.0);
            let bits = GLYPHS[glyph_idx * 7u + gy];
            if (((bits >> gx) & 1u) != 0u) {
                c = c + vec3(tr, tg, tb) * sharp;
            }
        }
    }

    return c * BRIGHT[li] * fade * FARB_S[si];
}

fn near_field(px: vec2<f32>) -> vec3<f32> {
    let si = min(u32(u.style + 0.5), 3u);
    let nlane_empty = NLANE_EMPTY_S[si];
    let lane = floor(px.x / NLANE_W);
    let lane_r = hash11(lane * 57.31 + u.seed * 0.91);
    if (lane_r < nlane_empty) {
        return vec3<f32>(0.0);
    }
    // Per-lane size jitter keeps the near plane from reading as a grid.
    let s01 = (lane_r - nlane_empty) / (1.0 - nlane_empty);
    let scale = (0.82 + s01 * 0.48) * NSCALE_S[si];
    let cw = NCELL_W * scale;
    let ch = NCELL_H * scale;
    let salt = 431.7 + lane * 91.3;

    let spawn_rand = hash11(salt + 17.3 + u.seed * 0.41);
    let spawn = sqrt(spawn_rand) * NSPAWN_DEADLINE;
    if (u.time < spawn) {
        return vec3<f32>(0.0);
    }
    let fade = smoothstep(spawn, spawn + NSPAWN_FADE, u.time);

    let margin = (NLANE_W - cw) * 0.5;
    let sway = sin(u.time * 0.13 + lane * 1.7) * max(margin - 2.0, 0.0) * 0.35;
    let cell_x0 = lane * NLANE_W + margin + sway;
    let u_x = px.x - cell_x0;
    if (u_x < 0.0 || u_x >= cw) {
        return vec3<f32>(0.0);
    }

    // Close = fast: 2-3x the far plane.
    let dur = (2.2 + hash11(salt + 9.91 + u.seed * 0.07) * 1.1) / scale;

    let local_t = u.time - spawn;
    let ci = floor(local_t / dur);
    let age = local_t - ci * dur;
    let s = salt + ci * 31.7 + u.seed * 1.31;

    let drop_len = u.res.y * (0.9 + hash11(s + 3.0) * 0.9);
    let speed = (u.res.y + drop_len + ch * 2.0) / dur;
    let head_y = age * speed;
    let head_row = head_y / ch;
    let length_rows = drop_len / ch;
    let row = floor(px.y / ch);
    let dist = head_row - row;

    if (dist < -0.5 || dist >= length_rows) {
        return vec3<f32>(0.0);
    }

    let head_white = 1.0 - smoothstep(0.0, 0.9, dist);
    let head_green = pow(clamp(1.0 - dist / 5.0, 0.0, 1.0), 2.0);
    let trail_t = smoothstep(0.0, 0.5, dist) * (1.0 - dist / length_rows);
    let trail_smooth = pow(max(trail_t, 0.0), 1.1);
    let glyph_rate = 1.2 + hash11(s + 4.0) * 2.8;
    let glyph_tick = floor(u.time * glyph_rate + row * 7.0);
    let glyph_idx = u32(hash11(glyph_tick + row * 0.97 + s) * f32(GLYPH_COUNT)) % GLYPH_COUNT;
    let flicker = 0.78 + hash11(floor(u.time * 9.0 + row * 11.3 + s)) * 0.22;

    let hot = 1.0 + 0.9 * step(0.82, hash11(s + 7.7));

    var tr = 3.8 * head_white + 0.06 * head_green + 0.16 * trail_smooth * flicker * hot;
    var tg = 3.8 * head_white + 1.25 * head_green + 1.70 * trail_smooth * flicker * hot;
    var tb = 3.8 * head_white + 0.20 * head_green + 0.52 * trail_smooth * flicker * hot;

    if (dist > 1.0 && hash11(floor(u.time * 3.0 + row * 23.0 + s) + 0.31) > 0.982) {
        tr = max(tr, 1.7);
        tg = max(tg, 2.2);
        tb = max(tb, 1.7);
    }

    var c = vec3<f32>(0.0);
    // Faint cell haze so the whole column reads as one solid stream.
    c = c + vec3(0.012, 0.14, 0.035) * (0.35 * head_green + 0.20 * trail_smooth);

    let cell_y0 = row * ch;
    let gsx = cw * 0.74;
    let gsy = ch * 0.80;
    let lx = u_x - (cw - gsx) * 0.5;
    let ly = px.y - cell_y0 - (ch - gsy) * 0.5;
    if (lx >= 0.0 && lx < gsx && ly >= 0.0 && ly < gsy) {
        let gx = lx / gsx * 5.0;
        let gy = ly / gsy * 7.0;
        let cov = glyph_cov(glyph_idx, gx, gy);
        c = c + vec3(tr, tg, tb) * cov;
    }

    return c * fade * NEARB_S[si];
}

// Far plane renders into a half-res texture: 4x cheaper, and the linear
// upsample in the composite is free depth-of-field blur. Near plane
// renders full res and stays razor sharp — the sharp/soft contrast is
// most of the 3D read.
@fragment
fn fs_far(in: VsOut) -> @location(0) vec4<f32> {
    return vec4(far_field(in.pos.xy * 2.0), 1.0);
}

@fragment
fn fs_near(in: VsOut) -> @location(0) vec4<f32> {
    return vec4(near_field(in.pos.xy), 1.0);
}
