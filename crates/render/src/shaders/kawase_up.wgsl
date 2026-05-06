struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

struct Uniforms {
    offset: f32,
    _pad0: f32,
    tex_size: vec2<f32>,
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

@group(0) @binding(0) var src_tex: texture_2d<f32>;
@group(0) @binding(1) var src_samp: sampler;
@group(0) @binding(2) var<uniform> u: Uniforms;

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let off = vec2<f32>(u.offset, u.offset) / u.tex_size;
    var sum = textureSample(src_tex, src_samp, in.uv + vec2(-off.x * 2.0, 0.0));
    sum = sum + textureSample(src_tex, src_samp, in.uv + vec2(-off.x,  off.y)) * 2.0;
    sum = sum + textureSample(src_tex, src_samp, in.uv + vec2( 0.0,    off.y * 2.0));
    sum = sum + textureSample(src_tex, src_samp, in.uv + vec2( off.x,  off.y)) * 2.0;
    sum = sum + textureSample(src_tex, src_samp, in.uv + vec2( off.x * 2.0, 0.0));
    sum = sum + textureSample(src_tex, src_samp, in.uv + vec2( off.x, -off.y)) * 2.0;
    sum = sum + textureSample(src_tex, src_samp, in.uv + vec2( 0.0,   -off.y * 2.0));
    sum = sum + textureSample(src_tex, src_samp, in.uv + vec2(-off.x, -off.y)) * 2.0;
    return sum / 12.0;
}
