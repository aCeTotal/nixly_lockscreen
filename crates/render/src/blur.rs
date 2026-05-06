use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    offset: f32,
    _pad0: f32,
    tex_size: [f32; 2],
}

pub struct MipLevel {
    pub _texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub width: u32,
    pub height: u32,
}

pub struct BlurPipeline {
    down_pipeline: wgpu::RenderPipeline,
    up_pipeline: wgpu::RenderPipeline,
    blur_bgl: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    format: wgpu::TextureFormat,
}

impl BlurPipeline {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let blur_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blur bgl"),
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
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let down_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("kawase_down"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/kawase_down.wgsl").into()),
        });
        let up_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("kawase_up"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/kawase_up.wgsl").into()),
        });

        let blur_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("blur pl"),
            bind_group_layouts: &[&blur_bgl],
            push_constant_ranges: &[],
        });

        let down_pipeline = make_pipeline(device, &blur_pl, &down_shader, format);
        let up_pipeline = make_pipeline(device, &blur_pl, &up_shader, format);

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("blur sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        Self {
            down_pipeline,
            up_pipeline,
            blur_bgl,
            sampler,
            format,
        }
    }

    pub fn ensure_chain(
        &self,
        device: &wgpu::Device,
        chain: &mut Vec<MipLevel>,
        w: u32,
        h: u32,
        passes: u32,
    ) {
        let target_first = (w.max(2) / 2).max(1);
        if chain.len() == passes as usize
            && chain.first().map(|m| m.width == target_first).unwrap_or(false)
        {
            return;
        }
        chain.clear();
        let mut cw = w.max(2) / 2;
        let mut ch = h.max(2) / 2;
        for _ in 0..passes {
            let cw_c = cw.max(1);
            let ch_c = ch.max(1);
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("blur mip"),
                size: wgpu::Extent3d {
                    width: cw_c,
                    height: ch_c,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: self.format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            chain.push(MipLevel {
                _texture: texture,
                view,
                width: cw_c,
                height: ch_c,
            });
            cw = (cw / 2).max(1);
            ch = (ch / 2).max(1);
        }
    }

    pub fn render_blur(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        input_view: &wgpu::TextureView,
        input_w: u32,
        input_h: u32,
        chain: &[MipLevel],
        offset: f32,
    ) {
        let mut prev_view: &wgpu::TextureView = input_view;
        let mut prev_w = input_w;
        let mut prev_h = input_h;

        for mip in chain {
            self.run_blur_pass(
                device,
                encoder,
                &self.down_pipeline,
                prev_view,
                prev_w,
                prev_h,
                offset,
                &mip.view,
                "down",
            );
            prev_view = &mip.view;
            prev_w = mip.width;
            prev_h = mip.height;
        }

        if chain.len() >= 2 {
            for i in (0..chain.len() - 1).rev() {
                let dst = &chain[i];
                self.run_blur_pass(
                    device,
                    encoder,
                    &self.up_pipeline,
                    prev_view,
                    prev_w,
                    prev_h,
                    offset,
                    &dst.view,
                    "up",
                );
                prev_view = &dst.view;
                prev_w = dst.width;
                prev_h = dst.height;
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn run_blur_pass(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        pipeline: &wgpu::RenderPipeline,
        input: &wgpu::TextureView,
        input_w: u32,
        input_h: u32,
        offset: f32,
        target: &wgpu::TextureView,
        label: &'static str,
    ) {
        let uniforms = Uniforms {
            offset,
            _pad0: 0.0,
            tex_size: [input_w as f32, input_h as f32],
        };
        let ub = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("blur ub"),
            contents: bytemuck::bytes_of(&uniforms),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(label),
            layout: &self.blur_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(input),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: ub.as_entire_binding(),
                },
            ],
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(label),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
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
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &bg, &[]);
        pass.draw(0..3, 0..1);
    }
}

fn make_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("pipeline"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
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
    })
}
