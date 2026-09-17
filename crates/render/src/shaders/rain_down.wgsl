// Downsample far+near rain planes to quarter res for the bloom.
// Near (full res) gets 4 bilinear taps ~ a 4x4 box; far (half res) is
// already soft, one tap suffices.
struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
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

@group(0) @binding(0) var far_tex: texture_2d<f32>;
@group(0) @binding(1) var near_tex: texture_2d<f32>;
@group(0) @binding(2) var src_samp: sampler;

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let px = 1.0 / vec2<f32>(textureDimensions(near_tex, 0));
    var c = vec3<f32>(0.0);
    c += textureSample(near_tex, src_samp, in.uv + vec2( px.x,  px.y)).rgb;
    c += textureSample(near_tex, src_samp, in.uv + vec2(-px.x,  px.y)).rgb;
    c += textureSample(near_tex, src_samp, in.uv + vec2( px.x, -px.y)).rgb;
    c += textureSample(near_tex, src_samp, in.uv + vec2(-px.x, -px.y)).rgb;
    c *= 0.25;
    c += textureSample(far_tex, src_samp, in.uv).rgb;
    return vec4(c, 1.0);
}
