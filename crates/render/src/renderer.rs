use std::time::Instant;

use anyhow::{Context, Result};

use crate::blur::BlurPipeline;
use crate::clock;
use crate::font;
use crate::output::{OutputId, OutputState};
use crate::rain::RainPipeline;
use crate::ui::{UiPipeline, UiQuad};
use crate::{Screenshot, SurfaceTarget};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptStatus {
    Typing,
    Authing,
    Failed,
    LockedOut,
}

#[derive(Debug, Clone, Copy)]
pub struct PromptInfo {
    pub chars: u32,
    pub status: PromptStatus,
}

pub struct Renderer {
    instance: wgpu::Instance,
    device_state: Option<DeviceState>,
    outputs: Vec<OutputState>,
    blur_passes: u32,
    blur_offset: f32,
    start: Instant,
    backdrop_dim: f32,
    seed: f32,
    prompt: Option<PromptInfo>,
    prompt_output: Option<OutputId>,
    awake: bool,
    prompt_dots: Vec<DotAnim>,
    last_chars: u32,
    unlock_started: Option<f32>,
    username: String,
    fail_started: Option<f32>,
    last_status: Option<PromptStatus>,
    frame_stats: FrameStats,
    power_save: bool,
}

#[derive(Default)]
struct FrameStats {
    last: Option<Instant>,
    count: u32,
    sum_ms: f32,
    max_ms: f32,
    acq_sum_ms: f32,
    acq_max_ms: f32,
}

impl FrameStats {
    fn record(&mut self, now: Instant, acquire_ms: f32) {
        if let Some(last) = self.last {
            let delta = now.duration_since(last).as_secs_f32() * 1000.0;
            log::trace!("dt {:.3} acq {:.3}", delta, acquire_ms);
            self.count += 1;
            self.sum_ms += delta;
            self.max_ms = self.max_ms.max(delta);
            self.acq_sum_ms += acquire_ms;
            self.acq_max_ms = self.acq_max_ms.max(acquire_ms);
            if self.count >= 240 {
                log::debug!(
                    "frame: avg {:.2}ms max {:.2}ms | acquire avg {:.2}ms max {:.2}ms",
                    self.sum_ms / self.count as f32,
                    self.max_ms,
                    self.acq_sum_ms / self.count as f32,
                    self.acq_max_ms
                );
                self.count = 0;
                self.sum_ms = 0.0;
                self.max_ms = 0.0;
                self.acq_sum_ms = 0.0;
                self.acq_max_ms = 0.0;
            }
        }
        self.last = Some(now);
    }
}

#[derive(Clone, Copy, Default)]
struct DotAnim {
    birth: Option<f32>,
    death: Option<f32>,
}

struct DeviceState {
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    blur: BlurPipeline,
    rain: RainPipeline,
    ui: UiPipeline,
    surface_format: wgpu::TextureFormat,
}

impl Default for Renderer {
    fn default() -> Self {
        Self::new()
    }
}

impl Renderer {
    pub fn new() -> Self {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            // GL fallback removed: wgpu-hal 23 EGL probing can panic (BadDisplay)
            // and take the whole locker down before anything is drawn.
            backends: wgpu::Backends::VULKAN,
            flags: wgpu::InstanceFlags::default(),
            dx12_shader_compiler: Default::default(),
            gles_minor_version: Default::default(),
        });
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| (d.as_nanos() % 1_000_000) as f32 * 0.001)
            .unwrap_or(0.0);
        Self {
            instance,
            device_state: None,
            outputs: Vec::new(),
            blur_passes: 4,
            blur_offset: 3.0,
            start: Instant::now(),
            backdrop_dim: 0.0,
            seed,
            prompt: None,
            prompt_output: None,
            awake: false,
            prompt_dots: vec![DotAnim::default(); MAX_DOTS as usize],
            last_chars: 0,
            unlock_started: None,
            username: String::new(),
            fail_started: None,
            last_status: None,
            frame_stats: FrameStats::default(),
            power_save: false,
        }
    }

    /// Battery mode: plain black background, no matrix rain, no blur.
    /// The prompt UI renders unchanged on top.
    pub fn set_power_save(&mut self, on: bool) {
        self.power_save = on;
    }

    pub fn set_username(&mut self, name: impl Into<String>) {
        self.username = name.into();
    }

    pub fn set_blur(&mut self, passes: u32, offset: f32) {
        self.blur_passes = passes.max(1);
        self.blur_offset = offset.max(0.5);
    }

    pub fn set_prompt(&mut self, prompt: Option<PromptInfo>) {
        let now = self.start.elapsed().as_secs_f32();
        let new_status = prompt.map(|p| p.status);
        if new_status == Some(PromptStatus::Failed)
            && self.last_status != Some(PromptStatus::Failed)
        {
            self.fail_started = Some(now);
        }
        self.last_status = new_status;
        let new_chars = prompt.map(|p| p.chars).unwrap_or(0).min(MAX_DOTS);
        if new_chars > self.last_chars {
            for i in self.last_chars..new_chars {
                let d = &mut self.prompt_dots[i as usize];
                d.death = None;
                d.birth = Some(now);
            }
        } else if new_chars < self.last_chars {
            for i in new_chars..self.last_chars {
                let d = &mut self.prompt_dots[i as usize];
                if d.death.is_none() {
                    d.death = Some(now);
                }
            }
        }
        self.last_chars = new_chars;
        self.prompt = prompt;
    }

    pub fn set_prompt_output(&mut self, id: Option<OutputId>) {
        self.prompt_output = id;
    }

    pub fn start_unlock(&mut self) {
        if self.unlock_started.is_none() {
            let now = self.start.elapsed().as_secs_f32();
            self.unlock_started = Some(now);
        }
    }

    pub fn unlock_done(&self) -> bool {
        match self.unlock_started {
            Some(t) => self.start.elapsed().as_secs_f32() - t >= UNLOCK_FADE_DUR,
            None => false,
        }
    }

    fn unlock_fade(&self, time: f32) -> f32 {
        match self.unlock_started {
            None => 1.0,
            Some(t) => {
                let p = ((time - t) / UNLOCK_FADE_DUR).clamp(0.0, 1.0);
                1.0 - ease_out_cubic(p)
            }
        }
    }

    pub fn set_awake(&mut self, awake: bool) {
        self.awake = awake;
    }

    pub fn add_output(
        &mut self,
        target: SurfaceTarget,
        screenshot: &Screenshot,
    ) -> Result<OutputId> {
        let surface = unsafe {
            self.instance
                .create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
                    raw_display_handle: target.raw_display,
                    raw_window_handle: target.raw_window,
                })?
        };

        if self.device_state.is_none() {
            let adapter = pollster::block_on(self.instance.request_adapter(
                &wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::LowPower,
                    compatible_surface: Some(&surface),
                    force_fallback_adapter: false,
                },
            ))
            .context("no compatible wgpu adapter")?;
            log::info!("wgpu adapter: {:?}", adapter.get_info());
            let (device, queue) = pollster::block_on(adapter.request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("nixly-lockscreen"),
                    required_features: wgpu::Features::empty(),
                    // downlevel_defaults caps textures at 2048px, which fails on
                    // 1440p/4K outputs; lift to what the adapter actually supports.
                    required_limits: wgpu::Limits::downlevel_defaults()
                        .using_resolution(adapter.limits()),
                    memory_hints: wgpu::MemoryHints::Performance,
                },
                None,
            ))?;

            let caps = surface.get_capabilities(&adapter);
            let surface_format = pick_format(&caps);
            let blur = BlurPipeline::new(&device, surface_format);
            let rain = RainPipeline::new(&device, surface_format);
            let ui = UiPipeline::new(&device, surface_format);
            self.device_state = Some(DeviceState {
                adapter,
                device,
                queue,
                blur,
                rain,
                ui,
                surface_format,
            });
        }

        let ds = self.device_state.as_ref().unwrap();
        let caps = surface.get_capabilities(&ds.adapter);
        let format = ds.surface_format;
        let alpha_mode = caps
            .alpha_modes
            .iter()
            .copied()
            .find(|m| matches!(*m, wgpu::CompositeAlphaMode::Opaque))
            .unwrap_or(caps.alpha_modes[0]);

        surface.configure(
            &ds.device,
            &wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format,
                width: target.width.max(1),
                height: target.height.max(1),
                present_mode: wgpu::PresentMode::Fifo,
                alpha_mode,
                view_formats: vec![],
                desired_maximum_frame_latency: 2,
            },
        );

        let (bg_texture, bg_view) = upload_screenshot(&ds.device, &ds.queue, screenshot);

        let id = OutputId(self.outputs.len());
        self.outputs.push(OutputState {
            surface,
            format,
            alpha_mode,
            width: target.width.max(1),
            height: target.height.max(1),
            bg_view,
            bg_width: screenshot.width,
            bg_height: screenshot.height,
            mip_chain: Vec::new(),
            blurred_offset: None,
            _bg_texture: bg_texture,
        });
        Ok(id)
    }

    pub fn resize(&mut self, id: &OutputId, width: u32, height: u32) {
        let Some(ds) = self.device_state.as_ref() else { return };
        let Some(o) = self.outputs.get_mut(id.0) else { return };
        o.width = width.max(1);
        o.height = height.max(1);
        o.surface.configure(
            &ds.device,
            &wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format: o.format,
                width: o.width,
                height: o.height,
                present_mode: wgpu::PresentMode::Fifo,
                alpha_mode: o.alpha_mode,
                view_formats: vec![],
                desired_maximum_frame_latency: 2,
            },
        );
        o.mip_chain.clear();
    }

    pub fn render(&mut self, id: &OutputId) -> Result<()> {
        let time = self.start.elapsed().as_secs_f32();
        let fade = self.unlock_fade(time);
        let progresses = self.tick_dots(time);
        let prompt = self.prompt;
        let prompt_output = self.prompt_output;
        let blur_passes = self.blur_passes;
        let effective_blur_offset = self.blur_offset * fade;
        let backdrop_dim = self.backdrop_dim;
        let seed = self.seed;
        let awake = self.awake;
        let username = self.username.clone();
        let fail_age = self.fail_started.map(|t0| (time - t0).max(0.0));

        let power_save = self.power_save;
        let ds = self.device_state.as_mut().context("no device")?;
        let o = self.outputs.get_mut(id.0).context("no output")?;

        let chain_rebuilt = if power_save {
            false
        } else {
            ds.blur
                .ensure_chain(&ds.device, &mut o.mip_chain, o.width, o.height, blur_passes)
        };
        if chain_rebuilt {
            o.blurred_offset = None;
        }

        let acquire_start = Instant::now();
        let frame = match o.surface.get_current_texture() {
            Ok(f) if f.suboptimal => {
                drop(f);
                o.surface.configure(
                    &ds.device,
                    &wgpu::SurfaceConfiguration {
                        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                        format: o.format,
                        width: o.width,
                        height: o.height,
                        present_mode: wgpu::PresentMode::Fifo,
                        alpha_mode: o.alpha_mode,
                        view_formats: vec![],
                        desired_maximum_frame_latency: 2,
                    },
                );
                o.surface
                    .get_current_texture()
                    .map_err(anyhow::Error::from)?
            }
            Ok(f) => f,
            Err(wgpu::SurfaceError::Lost) | Err(wgpu::SurfaceError::Outdated) => {
                o.surface.configure(
                    &ds.device,
                    &wgpu::SurfaceConfiguration {
                        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                        format: o.format,
                        width: o.width,
                        height: o.height,
                        present_mode: wgpu::PresentMode::Fifo,
                        alpha_mode: o.alpha_mode,
                        view_formats: vec![],
                        desired_maximum_frame_latency: 2,
                    },
                );
                return Ok(());
            }
            Err(e) => return Err(e.into()),
        };
        let acquire_end = Instant::now();
        if id.0 == 0 {
            let acquire_ms = acquire_end.duration_since(acquire_start).as_secs_f32() * 1000.0;
            self.frame_stats.record(acquire_end, acquire_ms);
        }
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = ds
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame"),
            });

        if power_save {
            // Battery: static black background — no blur, no rain. The
            // empty pass just clears; the UI pass below loads on top.
            encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("black bg"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
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
        } else {
            if o.blurred_offset != Some(effective_blur_offset) {
                ds.blur.render_blur(
                    &ds.device,
                    &mut encoder,
                    &o.bg_view,
                    o.bg_width,
                    o.bg_height,
                    &o.mip_chain,
                    effective_blur_offset,
                );
                o.blurred_offset = Some(effective_blur_offset);
            }

            let blurred_view = if let Some(first) = o.mip_chain.first() {
                &first.view
            } else {
                &o.bg_view
            };

            ds.rain.render(
                &ds.device,
                &ds.queue,
                &mut encoder,
                id.0,
                chain_rebuilt,
                blurred_view,
                &view,
                o.width,
                o.height,
                time,
                backdrop_dim,
                seed,
                awake,
            );
        }

        let show_prompt = match prompt_output {
            Some(target) => target == *id,
            None => true,
        };
        if show_prompt {
            if let Some(p) = prompt {
                let mut quads =
                    build_prompt_quads(&p, &progresses, o.width, o.height, time, fade, fail_age);
                build_clock_and_welcome_quads(
                    &username,
                    o.width,
                    o.height,
                    time,
                    fade,
                    &mut quads,
                );
                if let Some(t) = fail_age {
                    let shake_x = SHAKE_AMP * (-t * SHAKE_DECAY).exp() * (t * SHAKE_FREQ).sin();
                    if shake_x.abs() > 0.05 {
                        for q in quads.iter_mut() {
                            q.pos[0] += shake_x;
                        }
                    }
                    let alpha = 0.30 * (-t * RED_DECAY).exp();
                    if alpha > 0.005 {
                        quads.push(UiQuad {
                            pos: [0.0, 0.0],
                            size: [o.width as f32, o.height as f32],
                            color: [0.95, 0.10, 0.12, alpha * fade],
                            radius: 0.0,
                            ..Default::default()
                        });
                    }
                }
                if !quads.is_empty() {
                    ds.ui.render(
                        &ds.device,
                        &ds.queue,
                        &mut encoder,
                        &view,
                        o.width,
                        o.height,
                        &quads,
                    );
                }
            }
        }

        ds.queue.submit(Some(encoder.finish()));
        frame.present();
        Ok(())
    }
}

fn pick_format(caps: &wgpu::SurfaceCapabilities) -> wgpu::TextureFormat {
    caps.formats
        .iter()
        .copied()
        .find(|f| matches!(*f, wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Rgba8Unorm))
        .unwrap_or(caps.formats[0])
}

fn upload_screenshot(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    shot: &Screenshot,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("screenshot"),
        size: wgpu::Extent3d {
            width: shot.width.max(1),
            height: shot.height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Bgra8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::ImageCopyTexture {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        shot.pixels,
        wgpu::ImageDataLayout {
            offset: 0,
            bytes_per_row: Some(shot.stride),
            rows_per_image: Some(shot.height),
        },
        wgpu::Extent3d {
            width: shot.width,
            height: shot.height,
            depth_or_array_layers: 1,
        },
    );
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

const MAX_DOTS: u32 = 32;
const DOT_FADE_IN: f32 = 0.18;
const DOT_FADE_OUT: f32 = 0.14;
const UNLOCK_FADE_DUR: f32 = 0.28;
const SHAKE_AMP: f32 = 18.0;
const SHAKE_FREQ: f32 = 38.0;
const SHAKE_DECAY: f32 = 4.5;
const RED_DECAY: f32 = 2.6;
const DOT_SIZE: f32 = 16.0;
const DOT_GAP: f32 = 8.0;
const PILL_HEIGHT: f32 = 56.0;
const PILL_PAD_X: f32 = 28.0;
const PILL_MIN_INNER: f32 = 22.0;

fn ease_out_cubic(t: f32) -> f32 {
    let u = 1.0 - t.clamp(0.0, 1.0);
    1.0 - u * u * u
}

impl Renderer {
    fn tick_dots(&mut self, time: f32) -> [f32; MAX_DOTS as usize] {
        let mut out = [0.0f32; MAX_DOTS as usize];
        for i in 0..MAX_DOTS as usize {
            let d = &mut self.prompt_dots[i];
            if let Some(death) = d.death {
                let t = ((time - death) / DOT_FADE_OUT).clamp(0.0, 1.0);
                let prog = 1.0 - ease_out_cubic(t);
                if t >= 1.0 {
                    d.birth = None;
                    d.death = None;
                }
                out[i] = prog;
            } else if let Some(birth) = d.birth {
                let t = ((time - birth) / DOT_FADE_IN).clamp(0.0, 1.0);
                out[i] = ease_out_cubic(t);
            }
        }
        out
    }
}

fn build_prompt_quads(
    p: &PromptInfo,
    progresses: &[f32],
    w: u32,
    h: u32,
    time: f32,
    fade: f32,
    fail_age: Option<f32>,
) -> Vec<UiQuad> {
    if fade <= 0.001 {
        return Vec::new();
    }

    let cx = w as f32 * 0.5;
    let cy = h as f32 * 0.7;

    let pulse = if matches!(p.status, PromptStatus::LockedOut) {
        0.6 + 0.4 * (time * 4.0).sin().abs()
    } else {
        1.0
    };
    let red_mix = match (p.status, fail_age) {
        (PromptStatus::Failed, _) => 1.0,
        (_, Some(t)) => (-t * 1.8).exp(),
        _ => 0.0,
    };
    let dot_color = match p.status {
        PromptStatus::Typing => [0.62, 0.96, 0.72, 1.0],
        PromptStatus::Authing => [0.98, 0.82, 0.40, 1.0],
        PromptStatus::Failed => [1.0, 0.20, 0.22, 1.0],
        PromptStatus::LockedOut => [0.98 * pulse, 0.20 * pulse, 0.22 * pulse, 1.0],
    };

    let step = DOT_SIZE + DOT_GAP;
    let total_units: f32 = progresses.iter().sum();
    let inner_w = (total_units * step).max(PILL_MIN_INNER);
    let box_w = inner_w + PILL_PAD_X * 2.0;
    let box_h = PILL_HEIGHT;

    let mut quads: Vec<UiQuad> = Vec::with_capacity(2 + MAX_DOTS as usize);

    let pill_dark = [0.05, 0.07, 0.10];
    let pill_red = [0.55, 0.08, 0.10];
    let pill_color = [
        pill_dark[0] * (1.0 - red_mix) + pill_red[0] * red_mix,
        pill_dark[1] * (1.0 - red_mix) + pill_red[1] * red_mix,
        pill_dark[2] * (1.0 - red_mix) + pill_red[2] * red_mix,
        0.82 * fade + 0.10 * fade * red_mix,
    ];
    quads.push(UiQuad {
        pos: [cx - box_w * 0.5, cy - box_h * 0.5],
        size: [box_w, box_h],
        color: pill_color,
        radius: box_h * 0.5,
        ..Default::default()
    });

    let row_w = total_units * step;
    let row_left = cx - row_w * 0.5;
    let mut acc = 0.0f32;
    for &prog in progresses.iter() {
        if prog <= 0.001 {
            continue;
        }
        let span = prog * step;
        let cx_dot = row_left + acc + span * 0.5;
        acc += span;
        let size = DOT_SIZE * (0.55 + 0.45 * prog);
        let alpha = prog * fade;
        quads.push(UiQuad {
            pos: [cx_dot - size * 0.5, cy - size * 0.5],
            size: [size, size],
            color: [dot_color[0], dot_color[1], dot_color[2], dot_color[3] * alpha],
            radius: size * 0.5,
            ..Default::default()
        });
    }

    quads
}

fn local_hh_mm() -> (u8, u8) {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0) as libc::time_t;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    let res = unsafe { libc::localtime_r(&secs, &mut tm) };
    if res.is_null() {
        return (0, 0);
    }
    (tm.tm_hour as u8, tm.tm_min as u8)
}

fn build_clock_and_welcome_quads(
    username: &str,
    w: u32,
    h: u32,
    time: f32,
    fade: f32,
    out: &mut Vec<UiQuad>,
) {
    if fade <= 0.001 {
        return;
    }
    let cx = w as f32 * 0.5;

    let clock_h = (h as f32 * 0.18).clamp(96.0, 240.0);
    let clock_cy = h as f32 * 0.18;
    let (hh, mm) = local_hh_mm();
    let blink = 0.55 + 0.45 * (time * std::f32::consts::PI).cos().abs();
    let clock_color = [0.96, 0.98, 1.00, 0.92 * fade];
    let colon_color = [0.96, 0.98, 1.00, 0.92 * fade * blink];
    clock::build(hh, mm, cx, clock_cy, clock_h, clock_color, colon_color, out);

    let scale = (h as f32 / 380.0).clamp(2.0, 6.0).round();
    let label = if username.is_empty() {
        "WELCOME BACK".to_string()
    } else {
        format!("WELCOME BACK, {}", username.to_uppercase())
    };
    let text_w = font::measure(&label, scale);
    let text_h = 7.0 * scale;
    let pill_cy = h as f32 * 0.7;
    let text_y = pill_cy - PILL_HEIGHT * 0.5 - text_h - 28.0;
    let text_x = cx - text_w * 0.5;
    let color = [0.92, 0.95, 1.00, 0.78 * fade];
    font::build_text_quads(&label, text_x, text_y, scale, color, out);
}
