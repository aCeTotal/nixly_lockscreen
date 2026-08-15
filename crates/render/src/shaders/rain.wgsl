struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

struct Uniforms {
    backdrop_dim: f32,
    awake: f32,
    _pad1: f32,
    _pad2: f32,
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
@group(0) @binding(2) var rain_tex: texture_2d<f32>;
@group(0) @binding(3) var rain_samp: sampler;
@group(0) @binding(4) var<uniform> u: Uniforms;

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let bg_full = textureSample(bg_tex, bg_samp, in.uv).rgb;
    if (u.awake > 0.5) {
        return vec4<f32>(bg_full, 1.0);
    }
    let bg = bg_full * u.backdrop_dim;
    let rain = textureSample(rain_tex, rain_samp, in.uv).rgb;

    let dims = vec2<f32>(textureDimensions(rain_tex, 0));
    let px = 1.0 / dims;

    // rain_tex is half resolution, so one texel is ~2 output pixels and linear
    // filtering already averages 2x2 neighbourhoods: 4 diagonal taps give the
    // tight glow, 4 wider axis taps the halo.
    var bloom_near = vec3<f32>(0.0);
    bloom_near += textureSample(rain_tex, rain_samp, in.uv + vec2( px.x,  px.y)).rgb;
    bloom_near += textureSample(rain_tex, rain_samp, in.uv + vec2(-px.x,  px.y)).rgb;
    bloom_near += textureSample(rain_tex, rain_samp, in.uv + vec2( px.x, -px.y)).rgb;
    bloom_near += textureSample(rain_tex, rain_samp, in.uv + vec2(-px.x, -px.y)).rgb;
    bloom_near *= 0.20;

    var bloom_far = vec3<f32>(0.0);
    let r = 3.0;
    bloom_far += textureSample(rain_tex, rain_samp, in.uv + vec2( px.x*r,  0.0)).rgb;
    bloom_far += textureSample(rain_tex, rain_samp, in.uv + vec2(-px.x*r,  0.0)).rgb;
    bloom_far += textureSample(rain_tex, rain_samp, in.uv + vec2( 0.0,  px.y*r)).rgb;
    bloom_far += textureSample(rain_tex, rain_samp, in.uv + vec2( 0.0, -px.y*r)).rgb;
    bloom_far *= 0.11;

    let lin = rain + bloom_near + bloom_far;
    let toned = (vec3<f32>(1.0) - exp(-lin * 1.6)) * vec3<f32>(0.94, 1.0, 0.96);

    let dc = in.uv - vec2<f32>(0.5, 0.5);
    let vig = 1.0 - 0.40 * smoothstep(0.45, 1.0, length(dc) * 1.5);

    return vec4<f32>((bg + toned) * vig, 1.0);
}
