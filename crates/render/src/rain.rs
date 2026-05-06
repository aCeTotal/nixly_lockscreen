use std::time::Instant;

use rayon::prelude::*;
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    backdrop_dim: f32,
    awake: f32,
    _pad1: f32,
    _pad2: f32,
}

#[derive(Clone, Copy, Debug)]
struct Drop {
    drop_x: f32,
    head_y: f32,
    drop_length: f32,
    cell_w: f32,
    cell_h: f32,
    s: f32,
    glyph_rate: f32,
    streak_intensity: f32,
    brightness: f32,
}

struct LayerCfg {
    cell_w: f32,
    cell_h: f32,
    n_slots: u32,
    salt: f32,
    base_brightness: f32,
    ramp_t_start: f32,
}

const LAYERS: &[LayerCfg] = &[
    LayerCfg { cell_w: 10.0,  cell_h: 15.0,   n_slots: 60, salt: 521.0,  base_brightness: 0.65, ramp_t_start: 0.0 },
    LayerCfg { cell_w: 14.0,  cell_h: 21.0,   n_slots: 40, salt: 137.0,  base_brightness: 0.90, ramp_t_start: 1.0 },
    LayerCfg { cell_w: 20.0,  cell_h: 30.0,   n_slots: 28, salt: 271.0,  base_brightness: 1.15, ramp_t_start: 2.0 },
];

const EARLY_LAYER_END: usize = 3;
const SPAWN_DEADLINE: f32 = 35.0;
const SPAWN_FADE: f32 = 0.6;

const FRAME_TARGET_MS: f32 = 30.0;
const FRAME_FAST_MS: f32 = 24.0;
const DENSITY_MIN: f32 = 0.15;
const DENSITY_MAX: f32 = 1.0;
const DENSITY_STEP_DOWN: f32 = 0.04;
const DENSITY_STEP_UP: f32 = 0.004;
const FRAME_EMA_ALPHA: f32 = 0.18;

#[inline(always)]
fn hash11(p: f32) -> f32 {
    let bits = p.to_bits().wrapping_mul(0x27d4_eb2du32);
    let bits = bits ^ (bits >> 15);
    let bits = bits.wrapping_mul(0x9e37_79b1u32);
    let bits = bits ^ (bits >> 13);
    let bits = bits.wrapping_mul(0x85eb_ca77u32);
    let bits = bits ^ (bits >> 16);
    (bits & 0x00FF_FFFF) as f32 / 16_777_216.0
}

#[inline(always)]
fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    if edge1 <= edge0 {
        return if x < edge0 { 0.0 } else { 1.0 };
    }
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

const GLYPH_COUNT: u32 = 32;
const GLYPH_TABLE: [u32; 224] = [
    0x02, 0x02, 0x1F, 0x02, 0x02, 0x02, 0x02,
    0x15, 0x15, 0x15, 0x15, 0x15, 0x15, 0x15,
    0x10, 0x08, 0x04, 0x02, 0x01, 0x02, 0x04,
    0x01, 0x03, 0x05, 0x09, 0x05, 0x03, 0x01,
    0x1F, 0x04, 0x04, 0x0E, 0x04, 0x04, 0x0E,
    0x0A, 0x11, 0x00, 0x0A, 0x00, 0x11, 0x0A,
    0x1E, 0x02, 0x02, 0x02, 0x02, 0x02, 0x1E,
    0x0F, 0x08, 0x08, 0x08, 0x08, 0x08, 0x0F,
    0x10, 0x08, 0x0C, 0x06, 0x02, 0x01, 0x10,
    0x04, 0x0A, 0x11, 0x0A, 0x04, 0x0A, 0x11,
    0x0A, 0x0A, 0x1F, 0x0A, 0x1F, 0x0A, 0x0A,
    0x02, 0x0F, 0x10, 0x0A, 0x11, 0x10, 0x0A,
    0x0E, 0x11, 0x10, 0x0C, 0x02, 0x01, 0x1F,
    0x1F, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1F,
    0x07, 0x08, 0x16, 0x09, 0x0A, 0x0C, 0x10,
    0x04, 0x0A, 0x15, 0x04, 0x04, 0x04, 0x04,
    0x04, 0x04, 0x04, 0x04, 0x15, 0x0A, 0x04,
    0x01, 0x03, 0x02, 0x06, 0x04, 0x0C, 0x08,
    0x15, 0x0A, 0x15, 0x0A, 0x15, 0x0A, 0x15,
    0x11, 0x0A, 0x04, 0x04, 0x04, 0x0A, 0x11,
    0x0C, 0x12, 0x02, 0x02, 0x02, 0x12, 0x0C,
    0x1F, 0x0A, 0x04, 0x0A, 0x04, 0x0A, 0x1F,
    0x05, 0x0A, 0x09, 0x12, 0x05, 0x0A, 0x09,
    0x12, 0x12, 0x1F, 0x12, 0x0A, 0x12, 0x11,
    0x11, 0x11, 0x15, 0x15, 0x1B, 0x11, 0x00,
    0x1E, 0x01, 0x01, 0x01, 0x02, 0x04, 0x18,
    0x0E, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x0E,
    0x15, 0x15, 0x15, 0x0E, 0x04, 0x04, 0x04,
    0x10, 0x08, 0x1E, 0x04, 0x0F, 0x02, 0x01,
    0x1C, 0x02, 0x01, 0x01, 0x01, 0x02, 0x1C,
    0x0A, 0x15, 0x0A, 0x1F, 0x0A, 0x15, 0x0A,
    0x01, 0x0F, 0x10, 0x0E, 0x01, 0x10, 0x0E,
];

fn allocate_slot_intervals(width: f32) -> Vec<(f32, f32)> {
    let total = total_slots();
    let mut out = vec![(f32::NAN, f32::NAN); total];

    let mut entries: Vec<(usize, f32)> = Vec::with_capacity(total);
    let mut idx = 0usize;
    for layer in LAYERS {
        for _ in 0..layer.n_slots {
            entries.push((idx, layer.cell_w));
            idx += 1;
        }
    }
    entries.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

    let mut free: Vec<(f32, f32)> = vec![(0.0, width.max(0.0))];

    for (gi, cw) in entries {
        let mut best: Option<usize> = None;
        let mut best_size = -1.0_f32;
        for (i, &(a, b)) in free.iter().enumerate() {
            let s = b - a;
            if s + 1e-3 >= cw && s > best_size {
                best_size = s;
                best = Some(i);
            }
        }
        if let Some(fi) = best {
            let (a, b) = free[fi];
            let max_off = (b - a - cw).max(0.0);
            let r = hash11(gi as f32 * 17.31 + 3.71);
            let pos = a + r * max_off;
            let end = pos + cw;
            out[gi] = (pos, end);
            free.swap_remove(fi);
            if pos - a > 1.0 {
                free.push((a, pos));
            }
            if b - end > 1.0 {
                free.push((end, b));
            }
        }
    }
    out
}

fn build_drops(
    time: f32,
    seed: f32,
    res: [f32; 2],
    density: f32,
    slots: &mut [f32],
    intervals: &[(f32, f32)],
    out: &mut Vec<Drop>,
) {
    out.clear();
    let mut idx = 0usize;
    for (li, layer) in LAYERS.iter().enumerate() {
        let is_early = li < EARLY_LAYER_END;
        let spawn_window = (SPAWN_DEADLINE - layer.ramp_t_start).max(8.0);
        for j in 0..layer.n_slots {
            let slot_idx = idx;
            idx += 1;

            let (sa, sb) = intervals[slot_idx];
            if !sa.is_finite() {
                continue;
            }

            let salt_j = layer.salt + j as f32 * 173.7;

            let spawn_rand_raw = hash11(salt_j + 17.3 + seed * 0.41);
            let pinned = li == 0 && j == 0;
            let scheduled = if pinned {
                0.0
            } else {
                layer.ramp_t_start + spawn_rand_raw.sqrt() * spawn_window
            };

            let allowed_by_density = pinned || spawn_rand_raw <= density;

            let mut actual_spawn = slots[slot_idx];
            if !actual_spawn.is_finite() {
                if allowed_by_density && time >= scheduled {
                    actual_spawn = time;
                    slots[slot_idx] = actual_spawn;
                } else {
                    continue;
                }
            }

            let r_dur = hash11(salt_j + 0.7 + seed * 0.13);
            let cycle_dur = if is_early {
                let speed_pick = hash11(salt_j + 9.91 + seed * 0.07);
                if speed_pick < 0.7 {
                    4.5 + (speed_pick / 0.7) * 2.5
                } else {
                    3.0 + ((speed_pick - 0.7) / 0.3) * 1.5
                }
            } else {
                3.0 + r_dur * 4.0
            };

            let local_time = time - actual_spawn;
            let cycle_idx_f = (local_time / cycle_dur).floor();
            let cycle_idx = cycle_idx_f as i32;

            if !allowed_by_density && cycle_idx >= 1 {
                slots[slot_idx] = f32::INFINITY;
                continue;
            }

            let cycle_age = local_time - cycle_idx_f * cycle_dur;
            let s = salt_j + cycle_idx_f * 31.7 + seed * 1.31;
            let lane_span = (sb - sa - layer.cell_w).max(0.0);
            let drop_x = sa + hash11(s + 1.0) * lane_span;
            let drop_length = res[1] * (0.5 + hash11(s + 3.0) * 1.0);
            let drop_speed = (res[1] + drop_length + layer.cell_h * 2.0) / cycle_dur;
            let glyph_rate = 4.0 + hash11(s + 4.0) * 14.0;
            let streak_intensity = 0.04 + hash11(s + 0.51) * 0.04;
            let head_y = cycle_age * drop_speed;

            let fade = smoothstep(actual_spawn, actual_spawn + SPAWN_FADE, time);
            let brightness = layer.base_brightness * fade;

            out.push(Drop {
                drop_x,
                head_y,
                drop_length,
                cell_w: layer.cell_w,
                cell_h: layer.cell_h,
                s,
                glyph_rate,
                streak_intensity,
                brightness,
            });
        }
    }
}

fn total_slots() -> usize {
    LAYERS.iter().map(|l| l.n_slots as usize).sum()
}

#[inline(always)]
fn rasterize_drop(
    d: &Drop,
    time: f32,
    chunk_y0: i32,
    chunk_y1: i32,
    w: i32,
    h: i32,
    accum: &mut [f32],
) {
    let cell_x = d.cell_w;
    let cell_y = d.cell_h;
    let drop_x = d.drop_x;
    let head_y = d.head_y;
    let head_row = head_y / cell_y;
    let length_rows = d.drop_length / cell_y;
    let bn = d.brightness;
    if bn <= 0.0 {
        return;
    }

    let x0 = drop_x.floor().max(0.0) as i32;
    let x1 = ((drop_x + cell_x).ceil() as i32).min(w);
    if x1 <= x0 {
        return;
    }

    let screen_cells = ((h as f32) / cell_y).ceil() as i32 + 1;
    let max_cell = ((head_row + 5.0).floor() as i32).min(screen_cells);
    let cell_first = ((chunk_y0 as f32 / cell_y).floor() as i32).max(0);
    let cell_last = (((chunk_y1 - 1) as f32 / cell_y).floor() as i32 + 1).min(max_cell);

    let glyph_size_x = cell_x * 0.72;
    let glyph_size_y = cell_y * 0.78;
    let inner_origin_x = (cell_x - glyph_size_x) * 0.5;
    let inner_origin_y = (cell_y - glyph_size_y) * 0.5;

    for cy in cell_first..cell_last {
        let cell_id_y = cy as f32;
        let cell_top = cell_id_y * cell_y;
        let dist = head_row - cell_id_y;
        let col_active = cell_id_y <= head_row + 0.5
            && head_y >= 0.0
            && dist < length_rows;

        let mut cr = 0.0f32;
        let mut cg = 0.0f32;
        let mut cb = 0.0f32;

        if col_active {
            let si = d.streak_intensity;
            cr += 0.015 * si;
            cg += 0.18 * si;
            cb += 0.04 * si;
        }

        let trail_active = dist >= -0.5 && dist < length_rows;
        let mut tr = 0.0f32;
        let mut tg = 0.0f32;
        let mut tb = 0.0f32;
        let mut glyph_idx = 0u32;
        if trail_active {
            let head_white = 1.0 - smoothstep(0.0, 0.85, dist);
            let head_green = (1.0 - (dist / 4.0).clamp(0.0, 1.0)).powi(2);
            let trail_t = smoothstep(0.0, 0.5, dist) * (1.0 - dist / length_rows);
            let trail_smooth = trail_t.max(0.0).powf(1.2);
            let glyph_tick = (time * d.glyph_rate + cell_id_y * 7.0).floor();
            glyph_idx = (hash11(glyph_tick + cell_id_y * 0.97 + d.s) * GLYPH_COUNT as f32) as u32
                % GLYPH_COUNT;
            let flicker_tick = (time * 9.0 + cell_id_y * 11.3 + d.s).floor();
            let flicker = 0.72 + hash11(flicker_tick) * 0.28;
            tr = 2.5 * head_white + 0.05 * head_green + 0.18 * trail_smooth * flicker;
            tg = 2.5 * head_white + 1.0 * head_green + 1.30 * trail_smooth * flicker;
            tb = 2.5 * head_white + 0.15 * head_green + 0.50 * trail_smooth * flicker;
        }

        let particle_active = if col_active {
            let particle_tick = (time * 4.0 + cell_id_y * 23.0 + d.s).floor();
            hash11(particle_tick + 0.31) > 0.985
        } else {
            false
        };

        if cr == 0.0 && cg == 0.0 && cb == 0.0 && !trail_active && !particle_active {
            continue;
        }

        let py0 = (cell_top.floor() as i32).max(chunk_y0).max(0);
        let py1 = (((cell_top + cell_y).ceil()) as i32).min(chunk_y1).min(h);

        for y in py0..py1 {
            let fy = y as f32 + 0.5;
            let row_base = ((y - chunk_y0) as usize) * (w as usize) * 3;

            let gy_in = trail_active && fy >= cell_top + inner_origin_y
                && fy < cell_top + inner_origin_y + glyph_size_y;
            let gy = if gy_in {
                let inner_uy = (fy - cell_top - inner_origin_y) / glyph_size_y;
                ((inner_uy * 7.0).floor() as i32).clamp(0, 6) as u32
            } else {
                0
            };
            let row_bits = if gy_in {
                GLYPH_TABLE[(glyph_idx as usize) * 7 + gy as usize]
            } else {
                0
            };

            for x in x0..x1 {
                let off = row_base + (x as usize) * 3;
                let fx = x as f32 + 0.5;

                let mut r = cr;
                let mut g = cg;
                let mut b = cb;

                if gy_in {
                    let inner_ux = (fx - drop_x - inner_origin_x) / glyph_size_x;
                    let gx = (inner_ux * 5.0).floor() as i32;
                    if gx >= 0 && gx < 5 && (row_bits >> (gx as u32)) & 1 != 0 {
                        r += tr;
                        g += tg;
                        b += tb;
                    }
                }

                if particle_active {
                    let p_ux = (fx - drop_x) / cell_x - 0.5;
                    let p_uy = (fy - cell_top) / cell_y - 0.5;
                    let p_r2 = p_ux * p_ux + p_uy * p_uy;
                    if p_r2 < 0.15 * 0.15 {
                        let p_falloff = 1.0 - smoothstep(0.0, 0.15, p_r2.sqrt());
                        r += 0.6 * p_falloff;
                        g += 1.0 * p_falloff;
                        b += 0.7 * p_falloff;
                    }
                }

                accum[off] += r * bn;
                accum[off + 1] += g * bn;
                accum[off + 2] += b * bn;
            }
        }
    }
}

const ROWS_PER_CHUNK: usize = 32;

fn rasterize(
    drops: &[Drop],
    w: u32,
    h: u32,
    time: f32,
    accum_buf: &mut [f32],
    pixel_buf: &mut [u8],
) {
    let w_i = w as i32;
    let h_i = h as i32;
    let row_f32 = w as usize * 3;
    let row_u8 = w as usize * 4;
    let chunk_f32 = ROWS_PER_CHUNK * row_f32;
    let chunk_u8 = ROWS_PER_CHUNK * row_u8;

    accum_buf
        .par_chunks_mut(chunk_f32)
        .zip(pixel_buf.par_chunks_mut(chunk_u8))
        .enumerate()
        .for_each(|(ci, (accum, pixels))| {
            for v in accum.iter_mut() {
                *v = 0.0;
            }
            let y0 = (ci * ROWS_PER_CHUNK) as i32;
            let y1 = (y0 + ROWS_PER_CHUNK as i32).min(h_i);
            if y1 <= y0 {
                for px in pixels.iter_mut() {
                    *px = 0;
                }
                return;
            }

            for d in drops {
                rasterize_drop(d, time, y0, y1, w_i, h_i, accum);
            }

            let rows_in_chunk = (y1 - y0) as usize;
            for y in 0..rows_in_chunk {
                let acc_row = &accum[y * row_f32..y * row_f32 + row_f32];
                let px_row = &mut pixels[y * row_u8..y * row_u8 + row_u8];
                for x in 0..(w as usize) {
                    let r = (acc_row[x * 3] * 255.0).clamp(0.0, 255.0) as u8;
                    let g = (acc_row[x * 3 + 1] * 255.0).clamp(0.0, 255.0) as u8;
                    let b = (acc_row[x * 3 + 2] * 255.0).clamp(0.0, 255.0) as u8;
                    px_row[x * 4] = r;
                    px_row[x * 4 + 1] = g;
                    px_row[x * 4 + 2] = b;
                    px_row[x * 4 + 3] = 255;
                }
            }
            for y in rows_in_chunk..ROWS_PER_CHUNK {
                let off = y * row_u8;
                if off >= pixels.len() {
                    break;
                }
                let end = (off + row_u8).min(pixels.len());
                for px in &mut pixels[off..end] {
                    *px = 0;
                }
            }
        });
}

struct RainTarget {
    width: u32,
    height: u32,
    pixel_buf: Vec<u8>,
    accum_buf: Vec<f32>,
    tex: wgpu::Texture,
    view: wgpu::TextureView,
    last_rasterize_time: f32,
}

const REUSE_EPSILON_S: f32 = 0.004;

pub struct RainPipeline {
    pipeline: wgpu::RenderPipeline,
    bgl: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    drops_cpu: Vec<Drop>,
    targets: Vec<RainTarget>,
    density: f32,
    frame_ema_ms: f32,
    slot_spawn: Vec<f32>,
    slot_intervals: Vec<(f32, f32)>,
    slot_intervals_width: u32,
}

impl RainPipeline {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rain bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rain"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/rain.wgsl").into()),
        });

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rain pl"),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("rain"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview: None,
            cache: None,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("rain sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        Self {
            pipeline,
            bgl,
            sampler,
            drops_cpu: Vec::new(),
            targets: Vec::new(),
            density: DENSITY_MAX,
            frame_ema_ms: FRAME_FAST_MS,
            slot_spawn: vec![f32::INFINITY; total_slots()],
            slot_intervals: vec![(f32::NAN, f32::NAN); total_slots()],
            slot_intervals_width: 0,
        }
    }

    fn ensure_target(&mut self, device: &wgpu::Device, w: u32, h: u32) -> usize {
        if let Some(i) = self.targets.iter().position(|t| t.width == w && t.height == h) {
            return i;
        }
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("rain tex"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
        let chunks_h = ((h as usize).div_ceil(ROWS_PER_CHUNK)) * ROWS_PER_CHUNK;
        self.targets.push(RainTarget {
            width: w,
            height: h,
            pixel_buf: vec![0u8; chunks_h * (w as usize) * 4],
            accum_buf: vec![0f32; chunks_h * (w as usize) * 3],
            tex,
            view,
            last_rasterize_time: f32::NEG_INFINITY,
        });
        self.targets.len() - 1
    }

    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        bg_view: &wgpu::TextureView,
        target_view: &wgpu::TextureView,
        width: u32,
        height: u32,
        time: f32,
        backdrop_dim: f32,
        seed: f32,
        awake: bool,
    ) {
        let ti = self.ensure_target(device, width, height);

        if self.slot_intervals_width != width {
            self.slot_intervals = allocate_slot_intervals(width as f32);
            self.slot_intervals_width = width;
            for s in self.slot_spawn.iter_mut() {
                *s = f32::INFINITY;
            }
        }

        let res = [width as f32, height as f32];
        let need_rasterize = !awake
            && (time - self.targets[ti].last_rasterize_time).abs() > REUSE_EPSILON_S;

        if need_rasterize {
            let cpu_t0 = Instant::now();
            build_drops(
                time,
                seed,
                res,
                self.density,
                &mut self.slot_spawn,
                &self.slot_intervals,
                &mut self.drops_cpu,
            );
            let target = &mut self.targets[ti];
            rasterize(
                &self.drops_cpu,
                width,
                height,
                time,
                &mut target.accum_buf,
                &mut target.pixel_buf,
            );
            let cpu_ms = cpu_t0.elapsed().as_secs_f32() * 1000.0;
            self.frame_ema_ms =
                self.frame_ema_ms * (1.0 - FRAME_EMA_ALPHA) + cpu_ms * FRAME_EMA_ALPHA;
            if self.frame_ema_ms > FRAME_TARGET_MS {
                self.density = (self.density - DENSITY_STEP_DOWN).max(DENSITY_MIN);
            } else if self.frame_ema_ms < FRAME_FAST_MS {
                self.density = (self.density + DENSITY_STEP_UP).min(DENSITY_MAX);
            }

            let target = &mut self.targets[ti];
            let upload_rows = height as usize;
            let row_u8 = width as usize * 4;
            queue.write_texture(
                wgpu::ImageCopyTexture {
                    texture: &target.tex,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &target.pixel_buf[..upload_rows * row_u8],
                wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(width * 4),
                    rows_per_image: Some(height),
                },
                wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
            );
            target.last_rasterize_time = time;
        }

        let uniforms = Uniforms {
            backdrop_dim,
            awake: if awake { 1.0 } else { 0.0 },
            _pad1: 0.0,
            _pad2: 0.0,
        };
        let _ = seed;
        let ub = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("rain ub"),
            contents: bytemuck::bytes_of(&uniforms),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let rain_view = &self.targets[ti].view;
        let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rain bg"),
            layout: &self.bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(bg_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(rain_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: ub.as_entire_binding(),
                },
            ],
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("rain"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &bg, &[]);
        pass.draw(0..3, 0..1);
    }
}
