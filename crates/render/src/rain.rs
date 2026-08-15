#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GenUniforms {
    time: f32,
    seed: f32,
    res: [f32; 2],
    scale: f32,
    _pad: [f32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct CompUniforms {
    backdrop_dim: f32,
    awake: f32,
    _pad1: f32,
    _pad2: f32,
}

struct RainTarget {
    width: u32,
    height: u32,
    view: wgpu::TextureView,
    _tex: wgpu::Texture,
    comp_bg: Option<wgpu::BindGroup>,
}

pub struct RainPipeline {
    gen_pipeline: wgpu::RenderPipeline,
    gen_bg: wgpu::BindGroup,
    gen_ub: wgpu::Buffer,
    comp_pipeline: wgpu::RenderPipeline,
    comp_bgl: wgpu::BindGroupLayout,
    comp_ub: wgpu::Buffer,
    sampler: wgpu::Sampler,
    targets: Vec<Option<RainTarget>>,
}

impl RainPipeline {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let gen_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rain gen bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let gen_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rain gen"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/rain_gen.wgsl").into()),
        });

        let gen_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rain gen pl"),
            bind_group_layouts: &[&gen_bgl],
            push_constant_ranges: &[],
        });

        let gen_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("rain gen"),
            layout: Some(&gen_layout),
            vertex: wgpu::VertexState {
                module: &gen_shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &gen_shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview: None,
            cache: None,
        });

        let gen_ub = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rain gen ub"),
            size: std::mem::size_of::<GenUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let gen_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rain gen bg"),
            layout: &gen_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: gen_ub.as_entire_binding(),
            }],
        });

        let comp_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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

        let comp_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rain"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/rain.wgsl").into()),
        });

        let comp_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rain pl"),
            bind_group_layouts: &[&comp_bgl],
            push_constant_ranges: &[],
        });

        let comp_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("rain"),
            layout: Some(&comp_layout),
            vertex: wgpu::VertexState {
                module: &comp_shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &comp_shader,
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

        let comp_ub = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rain ub"),
            size: std::mem::size_of::<CompUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
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
            gen_pipeline,
            gen_bg,
            gen_ub,
            comp_pipeline,
            comp_bgl,
            comp_ub,
            sampler,
            targets: Vec::new(),
        }
    }

    fn ensure_target(&mut self, device: &wgpu::Device, key: usize, w: u32, h: u32) {
        if self.targets.len() <= key {
            self.targets.resize_with(key + 1, || None);
        }
        if matches!(&self.targets[key], Some(t) if t.width == w && t.height == h) {
            return;
        }
        // The rain texture is half resolution: same lane/cell geometry (the gen
        // shader scales frag coords back up), quarter the shading cost.
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("rain tex"),
            size: wgpu::Extent3d {
                width: (w / 2).max(1),
                height: (h / 2).max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
        self.targets[key] = Some(RainTarget {
            width: w,
            height: h,
            view,
            _tex: tex,
            comp_bg: None,
        });
    }

    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target_key: usize,
        bg_changed: bool,
        bg_view: &wgpu::TextureView,
        target_view: &wgpu::TextureView,
        width: u32,
        height: u32,
        time: f32,
        backdrop_dim: f32,
        seed: f32,
        awake: bool,
    ) {
        self.ensure_target(device, target_key, width, height);
        let target = self.targets[target_key].as_mut().unwrap();
        if bg_changed {
            target.comp_bg = None;
        }

        if !awake {
            queue.write_buffer(
                &self.gen_ub,
                0,
                bytemuck::bytes_of(&GenUniforms {
                    time,
                    seed,
                    res: [width as f32, height as f32],
                    scale: 2.0,
                    _pad: [0.0; 3],
                }),
            );
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("rain gen"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target.view,
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
            pass.set_pipeline(&self.gen_pipeline);
            pass.set_bind_group(0, &self.gen_bg, &[]);
            pass.draw(0..3, 0..1);
        }

        queue.write_buffer(
            &self.comp_ub,
            0,
            bytemuck::bytes_of(&CompUniforms {
                backdrop_dim,
                awake: if awake { 1.0 } else { 0.0 },
                _pad1: 0.0,
                _pad2: 0.0,
            }),
        );

        if target.comp_bg.is_none() {
            target.comp_bg = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("rain bg"),
                layout: &self.comp_bgl,
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
                        resource: wgpu::BindingResource::TextureView(&target.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: self.comp_ub.as_entire_binding(),
                    },
                ],
            }));
        }
        let comp_bg = target.comp_bg.as_ref().unwrap();

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
        pass.set_pipeline(&self.comp_pipeline);
        pass.set_bind_group(0, comp_bg, &[]);
        pass.draw(0..3, 0..1);
    }
}
