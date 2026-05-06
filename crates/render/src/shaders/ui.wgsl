struct Uniforms {
    resolution: vec2<f32>,
    _pad0: f32,
    _pad1: f32,
};

struct Quad {
    pos: vec2<f32>,
    size: vec2<f32>,
    color: vec4<f32>,
    radius: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
};

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var<storage, read> quads: array<Quad>;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) size: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) radius: f32,
};

@vertex
fn vs_main(@builtin(vertex_index) vid: u32, @builtin(instance_index) iid: u32) -> VsOut {
    let q = quads[iid];
    var corners = array<vec2<f32>, 6>(
        vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(0.0, 1.0),
        vec2(1.0, 0.0), vec2(1.0, 1.0), vec2(0.0, 1.0),
    );
    let c = corners[vid];
    let world = q.pos + c * q.size;
    let ndc = vec2(
        world.x / u.resolution.x * 2.0 - 1.0,
        1.0 - world.y / u.resolution.y * 2.0,
    );
    var out: VsOut;
    out.pos = vec4(ndc, 0.0, 1.0);
    out.local = c * q.size;
    out.size = q.size;
    out.color = q.color;
    out.radius = q.radius;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let r = min(in.radius, min(in.size.x, in.size.y) * 0.5);
    let half = in.size * 0.5;
    let q = abs(in.local - half) - (half - vec2(r, r));
    let outside = max(q, vec2(0.0, 0.0));
    let inside = min(max(q.x, q.y), 0.0);
    let dist = length(outside) + inside - r;
    let alpha = clamp(0.5 - dist, 0.0, 1.0);
    return vec4(in.color.rgb, in.color.a * alpha);
}
