struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

struct Uniforms {
    backdrop_dim: f32,
    awake: f32,
    time: f32,
    seed: f32,
    res: vec2<f32>,
    style: f32,
    _pad: f32,
};

@vertex
fn vs_main(@builtin(vertex_index) vid: u32) -> VsOut {
    var p = array<vec2<f32>, 3>(
        vec2(-1.0, -1.0),
        vec2( 3.0, -1.0),
        vec2(-1.0,  3.0),
    );
    var uv = array<vec2<f32>, 3>(
        vec2(0.0, 1.0),
        vec2(2.0, 1.0),
        vec2(0.0, -1.0),
    );
    var out: VsOut;
    out.pos = vec4(p[vid], 0.0, 1.0);
    out.uv = uv[vid];
    return out;
}

@group(0) @binding(0) var bg_tex: texture_2d<f32>;
@group(0) @binding(1) var bg_samp: sampler;
@group(0) @binding(2) var near_tex: texture_2d<f32>;
@group(0) @binding(3) var rain_samp: sampler;
@group(0) @binding(4) var<uniform> u: Uniforms;
@group(0) @binding(5) var blur_tex: texture_2d<f32>;
@group(0) @binding(6) var far_tex: texture_2d<f32>;

// Per-style bloom/tone tables (style 0..3):
// 0 classic dense  1 deep DOF  2 neon glow  3 dark cinematic
var<private> GLOW_W:   array<f32, 4> = array<f32, 4>(0.55, 0.50, 1.10, 0.40);
var<private> STREAK_W: array<f32, 4> = array<f32, 4>(0.22, 0.18, 0.50, 0.16);
var<private> TONE:     array<f32, 4> = array<f32, 4>(1.9,  1.8,  2.4,  1.7);

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let bg_full = textureSample(bg_tex, bg_samp, in.uv).rgb;
    if (u.awake > 0.5) {
        return vec4<f32>(bg_full, 1.0);
    }
    let si = min(u32(u.style + 0.5), 3u);
    let bg = bg_full * u.backdrop_dim;
    // Near plane sampled 1:1 (sharp), far plane linear-upsampled from half
    // res (soft) — free depth-of-field.
    let rain = textureSample(near_tex, rain_samp, in.uv).rgb
        + textureSample(far_tex, rain_samp, in.uv).rgb;

    // Glow from the quarter-res downsampled copy: linear upsampling of the
    // small texture IS the wide soft halo, and every tap is cache-cheap.
    // Diagonal taps widen the halo (~1 blur texel = 4 screen px); vertical
    // taps stretch it along the fall direction — the film's streaky bleed.
    let bpx = 1.0 / vec2<f32>(textureDimensions(blur_tex, 0));
    var glow = textureSample(blur_tex, rain_samp, in.uv).rgb * 0.5;
    glow += textureSample(blur_tex, rain_samp, in.uv + vec2( bpx.x,  bpx.y)).rgb * 0.125;
    glow += textureSample(blur_tex, rain_samp, in.uv + vec2(-bpx.x,  bpx.y)).rgb * 0.125;
    glow += textureSample(blur_tex, rain_samp, in.uv + vec2( bpx.x, -bpx.y)).rgb * 0.125;
    glow += textureSample(blur_tex, rain_samp, in.uv + vec2(-bpx.x, -bpx.y)).rgb * 0.125;
    glow *= GLOW_W[si];

    var streak = vec3<f32>(0.0);
    streak += textureSample(blur_tex, rain_samp, in.uv + vec2(0.0,  bpx.y*2.0)).rgb;
    streak += textureSample(blur_tex, rain_samp, in.uv + vec2(0.0, -bpx.y*2.0)).rgb;
    streak += textureSample(blur_tex, rain_samp, in.uv + vec2(0.0,  bpx.y*3.5)).rgb;
    streak += textureSample(blur_tex, rain_samp, in.uv + vec2(0.0, -bpx.y*3.5)).rgb;
    streak *= STREAK_W[si] * 0.25;

    let lin = rain + glow + streak;
    let toned = (vec3<f32>(1.0) - exp(-lin * TONE[si])) * vec3<f32>(0.94, 1.0, 0.96);

    let dc = in.uv - vec2<f32>(0.5, 0.5);
    let vig = 1.0 - 0.40 * smoothstep(0.45, 1.0, length(dc) * 1.5);

    return vec4<f32>((bg + toned) * vig, 1.0);
}
