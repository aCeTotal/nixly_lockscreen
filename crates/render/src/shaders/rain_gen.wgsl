struct VsOut {
    @builtin(position) pos: vec4<f32>,
};

struct Uniforms {
    time: f32,
    seed: f32,
    res: vec2<f32>,
    scale: f32,
    _pad0: f32,
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

// Every column lives in its own fixed-width lane; a lane hash assigns it to one
// depth layer (or leaves it empty), so columns can never overlap horizontally.
const LANE_W: f32 = 26.0;
const LANE_EMPTY: f32 = 0.10;

var<private> CELL_W: array<f32, 4> = array<f32, 4>(7.0, 10.0, 14.0, 20.0);
var<private> CELL_H: array<f32, 4> = array<f32, 4>(11.0, 15.0, 21.0, 30.0);
var<private> DEPTH: array<f32, 4> = array<f32, 4>(0.12, 0.32, 0.60, 1.00);
var<private> BRIGHT: array<f32, 4> = array<f32, 4>(0.40, 0.62, 0.90, 1.20);
var<private> SALT: array<f32, 4> = array<f32, 4>(811.0, 521.0, 137.0, 271.0);
var<private> RAMP: array<f32, 4> = array<f32, 4>(0.0, 0.0, 1.0, 2.0);

const SPAWN_DEADLINE: f32 = 35.0;
const SPAWN_FADE: f32 = 0.6;
const GLYPH_COUNT: u32 = 32u;

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

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    // Target is half resolution; work in full-resolution coordinates so the
    // lane/cell layout is identical to an on-screen render.
    let px = in.pos.xy * u.scale;
    let black = vec4<f32>(0.0, 0.0, 0.0, 1.0);

    let lane = floor(px.x / LANE_W);
    let lane_r = hash11(lane * 91.17 + u.seed * 0.77);
    if (lane_r < LANE_EMPTY) {
        return black;
    }
    let pick = (lane_r - LANE_EMPTY) / (1.0 - LANE_EMPTY);
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
    let window = max(SPAWN_DEADLINE - RAMP[li], 8.0);
    let spawn = RAMP[li] + sqrt(spawn_rand) * window;
    if (u.time < spawn) {
        return black;
    }
    let fade = smoothstep(spawn, spawn + SPAWN_FADE, u.time);

    let margin = (LANE_W - cw) * 0.5;
    let sway = sin(u.time * 0.22 + lane * 0.9) * max(margin - 1.0, 0.0) * depth;
    let cell_x0 = lane * LANE_W + margin + sway;
    let u_x = px.x - cell_x0;
    if (u_x < 0.0 || u_x >= cw) {
        return black;
    }

    let speed_pick = hash11(salt_j + 9.91 + u.seed * 0.07);
    var dur: f32;
    if (speed_pick < 0.7) {
        dur = 4.5 + (speed_pick / 0.7) * 2.5;
    } else {
        dur = 3.0 + ((speed_pick - 0.7) / 0.3) * 1.5;
    }
    dur = dur * (1.35 - 0.5 * depth);

    let local_t = u.time - spawn;
    let ci = floor(local_t / dur);
    let age = local_t - ci * dur;
    let s = salt_j + ci * 31.7 + u.seed * 1.31;

    let drop_len = u.res.y * (0.5 + hash11(s + 3.0));
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

        var tr = 2.5 * head_white + 0.05 * head_green + 0.16 * trail_smooth * flicker;
        var tg = 2.5 * head_white + 1.0 * head_green + 1.35 * trail_smooth * flicker;
        var tb = 2.5 * head_white + 0.15 * head_green + 0.42 * trail_smooth * flicker;

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

    return vec4(c * BRIGHT[li] * fade, 1.0);
}
